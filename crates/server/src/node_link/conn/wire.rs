//! 信封、序号与错误 body 的集中处（`docs/NODE_LINK_PROTOCOL.md` §2.2/§2.4/§14、`design.md` D2/D9）。
//!
//! 本模块只做**判定与装配**，不做 IO：`conn::session` 负责让判定结果产生对端可见的后果。
//! 判定口径逐条对应协议：
//!
//! - 帧级：binary 帧按 §2.1 回 `link.error`（`invalid_json`）并以 4400 关闭；
//! - JSON 结构：整帧超过 §2.5 的三个固定上限（嵌套深度 64 / 单对象字段数 1,024 / 单数组元素数 10,000）
//!   → `nodelink.protocol.schema_invalid`（消息不生效、连接保持可用）；判定在 wire DTO 边界
//!   （`node_link_protocol::structure`，§2.4）而不是传输层：它是同一帧内的结构，与单条 message 的
//!   1 MiB 上限（由接入层以 1009 拒绝）不是同一件事；
//! - JSON 语法：文本不是合法 JSON → `nodelink.protocol.invalid_json`（消息不生效，连接保持可用）；
//!   判定在结构上限之后（见 `Envelope::decode` 的顺序说明：两者都处置为「消息不生效」）；
//! - 信封/schema：未知信封字段、连接字段与阶段不匹配、必需字段缺失 → `nodelink.protocol.schema_invalid`；
//!   消息 body 不是 JSON 同样是 `invalid_json`；body 的字段校验由各消息的类型化解码（`deny_unknown_fields`）
//!   承担，失败一律 `schema_invalid`；
//! - 版本：`protocolVersion != 1` 或版本交集为空 → `nodelink.protocol.version_unsupported` + 4406；
//! - 序号：`connectionId` 不匹配、重复、回退、跳号 → `nodelink.protocol.sequence_invalid`；
//!   序号超过 v1 上界（`2^63-1`）是 schema 错误（§3.2「接收方不得截断、回绕或钳制」）；
//! - type：未知 `type` 与 v1 首切片的 post_mvp 消息族 → `nodelink.protocol.type_unsupported`；
//!   认证前收到只许认证后出现的业务消息（R38）也是 4401 关闭而不是「schema 错误」：连接阶段本身就不对，
//!   只回一条可恢复的错误会让业务消息看起来「被接受但没生效」。
//!
//! 序号记账与 type/形状无关（§2.2）：`connectionSequence` 属于**对端发出的消息**，本机接不接受它的
//! type/body/信封形状都不影响对端已经用掉的序号。因此只要帧文本能读出「与本连接相同的 `connectionId`」
//! 与「规范十进制、不超过 v1 上界、正好等于期望值的 `connectionSequence`」，**每一条消息级拒绝都推进
//! 期望序号**（[`sequence_after_rejection`]）：无论拒绝码是 `type_unsupported`、`schema_invalid` 还是
//! `invalid_json`，一个显式拒绝不得让连接之后全部错位——否则对端其后每条消息都被判
//! `nodelink.protocol.sequence_invalid` 且永不推进，连接退化成只出不进的哑管道
//! （`openspec/changes/node-link-malformed-sequence/specs/node-link-owner-server/spec.md` 的
//! 「每次消息级拒绝都推进入站期望序号」）。
//!
//! 不记账的只有**读不出该序号**的帧：文本不是合法 JSON、连接字段不成对或缺席、`connectionId` 属于别的
//! 连接、序号不是规范十进制/超过 v1 上界/不等于期望值——这些帧不能证明对端消耗了它的下一个序号。
//! **已知残留**：`sequence_after_rejection` 按文本重解析来读序号，而解析器默认递归上限是 128（检查在进入容器
//! 时先自减再判零，因此实际只能解析嵌套 127 层的帧），高于 §2.5 的嵌套上限（64）；因此结构类超限**只被收窄、
//! 未被消除**——深度 65..=127 可解析（推进），≥128 即便文本是合法 JSON 也解析不出序号（不推进）。该残留登记
//! 在规格的例外条款里，不得声称已完全消除。
//! 「读得出」由 [`sequence_after_rejection`] 自己判定，因此两个分支（形状错误与词表外 `type`）的记账口径
//! 逐条一致；关闭连接的判定（版本不符等）不参与记账——连接不会再有后续消息。

use node_link_protocol::common::{DecimalString, Nullable, RawObject, Uuid};
use node_link_protocol::envelope::{AuthState, Envelope, EnvelopeError, MessageType, Phase};
use node_link_protocol::error::{Body, ErrorCode};

/// v1 的序号上界（`SYNC_PROTOCOL.md` §3.2 沿用同一上界）。超出即 `schema_invalid`。
pub(crate) const MAX_SEQUENCE: u64 = i64::MAX as u64;

/// post_mvp 消息族（`NODE_LINK_PROTOCOL.md` §12.1）：v1 首切片显式拒绝，绝不静默忽略。
const POST_MVP: [MessageType; 5] = [
    MessageType::CatalogChanged,
    MessageType::ResourceDetach,
    MessageType::NodeRotateKeyRequest,
    MessageType::NodeRotateKeyResult,
    MessageType::LinkBackpressure,
];

/// 入站帧的判定结果。
pub(crate) enum Inbound {
    /// 通过阶段/信封/序号校验的消息。
    Message(Envelope),
    /// 消息不生效、连接保持可用：会话回 `link.error`（code/message 来自这里）。
    Rejected {
        code: ErrorCode,
        message: &'static str,
    },
    /// 致命：会话回 `link.error` 后按 `close_code` 关闭。
    Fatal {
        code: ErrorCode,
        message: &'static str,
        close_code: u16,
    },
}

/// 解码 + 阶段校验 + 序号校验 + type 允许性判定。
///
/// `expected_sequence`：本方向下一个应到的序号（认证后从 `1` 开始）；返回 `Some(next)` 表示序号已推进。
/// `allow_pre_auth_types`：握手阶段只允许 `node.hello`/`node.proof`（`node.challenge` 是 Owner→Access，
/// 反向收到即拒绝）；认证后为 `false`（由 post_mvp 与路由分别处理）。
///
/// `Envelope::decode` 是唯一的帧解码入口（结构上限、语法、信封形状都在它里面），因此本条路径对
/// 所有入站帧一致生效；区分 `invalid_json` 与 `schema_invalid` 所需的「文本是不是 JSON」只在信封解码
/// 已经失败时再解析一次——合法帧不为此多付一次通用 JSON 解析。
pub(crate) fn judge(
    text: &str,
    phase: Phase,
    expected_sequence: Option<u64>,
    connection_id: Option<&Uuid>,
) -> (Inbound, Option<u64>) {
    let envelope = match Envelope::decode(text) {
        Ok(envelope) => envelope,
        // §2.5 的三个固定 JSON 结构上限：与信封形状失败同类，消息级拒绝（不属于改 type 的语义错误）。
        // 序号记账与信封形状失败同口径：结构超限的帧同样可能带着「本连接的下一个序号」到达 wire，
        // 判定先于解析并不改变那个序号已被对端用掉的事实；读不出序号时该函数自己返回 `None`。
        Err(EnvelopeError::Structure(_)) => {
            return (
                Inbound::Rejected {
                    code: ErrorCode::ProtocolSchemaInvalid,
                    message: STRUCTURE_MESSAGE,
                },
                sequence_after_rejection(text, phase, expected_sequence, connection_id),
            );
        }
        Err(EnvelopeError::UnsupportedVersion { .. }) => {
            return (
                fatal(
                    ErrorCode::ProtocolVersionUnsupported,
                    VERSION_MESSAGE,
                    crate::node_link::conn::close::VERSION,
                ),
                None,
            );
        }
        // 未知 type 与被篡改的 type 都收敛到同一类（不泄露「存在但未实现」与「完全未知」的差异）。
        Err(EnvelopeError::UnknownType { .. }) => {
            // 序号记账与形状错误同口径：帧文本能读出「本连接的 connectionId + 正好是下一个的序号」时推
            // 期望序号（词表外取值经 `MessageType::from_str` 折成 `UnknownType`，与 Sync 侧走 serde 的
            // `Malformed` 是同一判定的两个入口）。
            return (
                Inbound::Rejected {
                    code: ErrorCode::ProtocolTypeUnsupported,
                    message: TYPE_MESSAGE,
                },
                sequence_after_rejection(text, phase, expected_sequence, connection_id),
            );
        }
        // `Envelope::decode` 不产出这一支（`Wire.body` 是 `RawValue`，body 非 JSON 时整帧 serde 直接报
        // `Malformed`）；`BodyNotJson` 只来自 `encode_body` 的装配。两条入口同口径记账，以免它们分叉。
        Err(EnvelopeError::BodyNotJson(_)) => {
            return (
                Inbound::Rejected {
                    code: ErrorCode::ProtocolInvalidJson,
                    message: "the message body is not valid JSON",
                },
                sequence_after_rejection(text, phase, expected_sequence, connection_id),
            );
        }
        Err(
            EnvelopeError::Malformed(_)
            | EnvelopeError::ConnectionFieldsMismatch
            | EnvelopeError::ConnectionFieldsRequired { .. }
            | EnvelopeError::ConnectionFieldsForbidden { .. },
        ) => {
            // 语法与信封形状共用 `Malformed` 一族，需要重新解析一次才能分开：文本根本不是 JSON 时
            // §2.4 要求 `invalid_json`，否则是信封/schema 形状错误。
            let (code, message) = if serde_json::from_str::<serde_json::value::Value>(text).is_err()
            {
                (
                    ErrorCode::ProtocolInvalidJson,
                    "the frame is not valid JSON",
                )
            } else {
                (ErrorCode::ProtocolSchemaInvalid, SCHEMA_MESSAGE)
            };
            // 序号记账对这一族逐条一致：`ConnectionFieldsForbidden`（帧自称「只许认证前出现」的 type，
            // 却带着连接字段）与「已知 type、信封另有形状错误」（未登记的信封字段、缺 `messageId`/`body`
            // 等）的连接字段都可读，序号正好是期望值时照常推进——那一帧已经在 wire 上占用了对端的下一个
            // 序号（§2.2 的序号属于对端已发出的消息），不推进会让连接退化成只出不进。`Mismatch`/`Required`
            // （连接字段不成对或缺席）与文本解不出 JSON 的情形，[`sequence_after_rejection`] 自己返回
            // `None`——那些帧读不出该序号。
            return (
                Inbound::Rejected { code, message },
                sequence_after_rejection(text, phase, expected_sequence, connection_id),
            );
        }
    };

    match phase {
        // 认证前：先判「这条消息在这个阶段是否允许存在」，再判信封形状。
        Phase::PreAuth => {
            // §2.1/R38：认证完成前的业务消息（只许认证后出现的 type）以 4401 关闭；
            // v1 的 post_mvp 一族全部属此类，因此它们的 4401 优先于 type_unsupported。
            if envelope.message_type().auth_state() == AuthState::PostAuth {
                return (
                    fatal(
                        ErrorCode::ProtocolTypeUnsupported,
                        TYPE_MESSAGE,
                        crate::node_link::conn::close::UNAUTHENTICATED,
                    ),
                    None,
                );
            }
            // 认证前消息必须省略连接字段（§2.2）；带上它们是 schema 错误（消息不生效、连接可用）。
            if envelope.validate_phase(Phase::PreAuth).is_err() {
                return (
                    Inbound::Rejected {
                        code: ErrorCode::ProtocolSchemaInvalid,
                        message: SCHEMA_MESSAGE,
                    },
                    None,
                );
            }
            (Inbound::Message(envelope), None)
        }
        Phase::PostAuth => {
            // 认证后消息必须携带连接字段（§2.2）；缺失是 schema 错误。
            if envelope.validate_phase(Phase::PostAuth).is_err() {
                return (
                    Inbound::Rejected {
                        code: ErrorCode::ProtocolSchemaInvalid,
                        message: SCHEMA_MESSAGE,
                    },
                    None,
                );
            }
            let Some(expected) = expected_sequence else {
                // 不该发生：认证后必须给出期望序号（会话状态机的接线错误）。
                return (
                    Inbound::Fatal {
                        code: ErrorCode::InternalUnavailable,
                        message: INTERNAL_MESSAGE,
                        close_code: crate::node_link::conn::close::UNAVAILABLE,
                    },
                    None,
                );
            };
            let Some(fields) = envelope.connection() else {
                return (
                    Inbound::Rejected {
                        code: ErrorCode::ProtocolSchemaInvalid,
                        message: SCHEMA_MESSAGE,
                    },
                    None,
                );
            };
            if connection_id.is_some_and(|id| fields.connection_id != *id) {
                return (
                    Inbound::Rejected {
                        code: ErrorCode::ProtocolSequenceInvalid,
                        message: SEQUENCE_MESSAGE,
                    },
                    None,
                );
            }
            let Ok(sequence) = fields.connection_sequence.as_str().parse::<u64>() else {
                return (
                    Inbound::Rejected {
                        code: ErrorCode::ProtocolSchemaInvalid,
                        message: SCHEMA_MESSAGE,
                    },
                    None,
                );
            };
            if sequence > MAX_SEQUENCE {
                return (
                    Inbound::Rejected {
                        code: ErrorCode::ProtocolSchemaInvalid,
                        message: "the sequence exceeds the v1 upper bound",
                    },
                    None,
                );
            }
            if sequence != expected {
                return (
                    Inbound::Rejected {
                        code: ErrorCode::ProtocolSequenceInvalid,
                        message: SEQUENCE_MESSAGE,
                    },
                    None,
                );
            }
            // 序号先记账再判 type：对端为这条消息用了它的下一个序号，即使本机随后拒绝这条消息
            // （post_mvp/未实现的 type），下一个序号仍必须被接受，否则一次拒绝就让连接永远错位。
            if POST_MVP.contains(&envelope.message_type()) {
                return (
                    Inbound::Rejected {
                        code: ErrorCode::ProtocolTypeUnsupported,
                        message: TYPE_MESSAGE,
                    },
                    Some(sequence + 1),
                );
            }
            (Inbound::Message(envelope), Some(sequence + 1))
        }
    }
}

/// [`judge`] 在**解码失败**的消息级拒绝路径上的序号记账（§2.2 的「序号属于对端发出的消息」）。
///
/// 这里**没有**「信封形状已经验证过」这个前提：`Envelope::decode` 的失败分支覆盖 `Structure`、
/// `UnknownType`、`BodyNotJson`、`Malformed` 一族与连接字段三类，失败路径上拿不到已解析的信封，因此帧
/// 文本必须重新解析。记账只看四件事：阶段是认证后、`connectionId` 与本次连接相同、`connectionSequence`
/// 是规范十进制串且不超过 v1 上界、正好等于期望值。任一不满足就不记账——文本不是合法 JSON、连接字段不
/// 成对或缺席、`connectionId` 属于别的连接、序号不是规范十进制/超过 v1 上界/不等于期望值都落在这一类：
/// 对端没有为这条消息消耗它的下一个序号。
///
/// 条件命中时，无论拒绝码是 `type_unsupported`、`schema_invalid` 还是 `invalid_json`，都必须推进：帧已经
/// 在 wire 上占用了对端的下一个序号
/// （`openspec/changes/node-link-malformed-sequence/specs/node-link-owner-server/spec.md` 的
/// 「每次消息级拒绝都推进入站期望序号」），不推进会让对端其后每条消息都被判
/// `nodelink.protocol.sequence_invalid` 且永不推进，连接退化成只出不进的哑管道。已知 type 的路径
/// （`POST_MVP` 与路由拒绝）用同一口径，只是那时序号已经由 `Envelope` 解出、不需要重读帧。
fn sequence_after_rejection(
    text: &str,
    phase: Phase,
    expected_sequence: Option<u64>,
    connection_id: Option<&Uuid>,
) -> Option<u64> {
    if phase != Phase::PostAuth {
        // 认证前的消息不携带连接字段（§2.2），也没有期望序号。
        return None;
    }
    let expected = expected_sequence?;
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let object = value.as_object()?;
    let id = Uuid::parse(object.get("connectionId")?.as_str()?).ok()?;
    if connection_id.is_some_and(|connection| id != *connection) {
        return None;
    }
    let sequence = DecimalString::parse(object.get("connectionSequence")?.as_str()?).ok()?;
    let sequence = sequence.as_str().parse::<u64>().ok()?;
    if sequence != expected || sequence > MAX_SEQUENCE {
        return None;
    }
    Some(sequence + 1)
}

/// 认证前消息 type 的允许性（§2.1/§12.2：连接建立后的第一条必须是 `node.hello`）。
///
/// `link.error` 在两种阶段都合法（`AuthState::Either`），因此始终允许。
pub(crate) fn pre_auth_allowed(message_type: MessageType, expecting_proof: bool) -> bool {
    if message_type.auth_state() == AuthState::Either {
        return true;
    }
    if expecting_proof {
        message_type == MessageType::NodeProof
    } else {
        message_type == MessageType::NodeHello
    }
}

/// `link.error` 的 body（`error.schema.json` 的 `linkErrorBody`）。
///
/// `details` 只允许登记表里的字段（§14.1）：未登记的 code 一律送 `{}`；`correlationId` 只在能指向
/// 触发本次错误的入站消息时给出。`retryable` 一律取错误码在 registry 里登记的语义。
pub(crate) fn error_body(
    code: ErrorCode,
    message: &str,
    details: RawObject,
    correlation_id: Option<Uuid>,
) -> Option<Body> {
    let retryable = code.default_retryable();
    let body =
        Body::new(code, message, retryable).or_else(|_| Body::new(code, code.as_str(), retryable));
    let Ok(mut body) = body else {
        // 两条文本都是固定短文本，`Text<1024>` 不可能失败；仍然显式处理（正常路径不得 unwrap/expect）。
        return None;
    };
    body.correlation_id = Nullable::from_option(correlation_id);
    body.details = details;
    Some(body)
}

/// `details.features`（`nodelink.protocol.feature_required` 的登记字段，§11.3/§14.1）。
///
/// 调用方保证已排序去重；本函数只负责编码（不可能失败时退回空 object，不让握手因装配细节失败）。
pub(crate) fn feature_details(missing: &[String]) -> RawObject {
    match serde_json::to_string(&serde_json::json!({ "features": missing })) {
        Ok(text) => RawObject::parse(&text).unwrap_or_else(|_| RawObject::empty()),
        Err(_) => RawObject::empty(),
    }
}

/// `details.retryAfterMs`（`nodelink.resource.rate_limited` 的登记字段，§14.1）。
pub(crate) fn rate_limit_details(retry_after: std::time::Duration) -> RawObject {
    let text = serde_json::json!({ "retryAfterMs": retry_after.as_millis() as u64 }).to_string();
    RawObject::parse(&text).unwrap_or_else(|_| RawObject::empty())
}

pub(crate) const SCHEMA_MESSAGE: &str = "the message does not match the v1 schema";
pub(crate) const STRUCTURE_MESSAGE: &str = "the frame exceeds the fixed JSON structure limits";
pub(crate) const SEQUENCE_MESSAGE: &str = "the connection sequence is not the next expected value";
pub(crate) const TYPE_MESSAGE: &str = "the message type is not supported";
pub(crate) const VERSION_MESSAGE: &str = "the protocol version is not supported";
pub(crate) const INTERNAL_MESSAGE: &str = "the owner node cannot serve this connection";

const fn fatal(code: ErrorCode, message: &'static str, close_code: u16) -> Inbound {
    Inbound::Fatal {
        code,
        message,
        close_code,
    }
}
