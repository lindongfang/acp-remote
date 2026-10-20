//! 信封、序号与错误 body 的集中处（`docs/SYNC_PROTOCOL.md` §4.1/§4.2/§12/§13）。
//!
//! 本模块只做**判定与装配**，不做 IO：`conn::session` 负责让判定结果产生对端可见的后果。判定口径
//! 逐条对应协议：
//!
//! - 帧级：binary 帧由会话拒绝并以 4400 关闭（§3.1；本模块不管帧，只处理文本）；
//! - JSON 语法：文本不是合法 JSON → `protocol.invalid_json`（消息不生效、连接保持可用）；
//! - 信封/schema：未知信封字段、连接字段与阶段不匹配、必需字段缺失 → `protocol.schema_invalid`；
//!   body 不是 JSON 同样是 `invalid_json`；body 的字段校验由各消息的类型化解码（`deny_unknown_fields`）
//!   承担，失败一律 `schema_invalid`；
//! - 版本：信封 `protocolVersion != 1` → `protocol.version_unsupported` + 4406（§4.1 的 `const 1`；
//!   版本交集为空由 `conn::handshake` 在 hello 解码后判定，同样回该码与 4406，§5.1）；
//! - 序号：`connectionId` 不匹配、重复、回退、跳号 → `protocol.sequence_invalid`；序号超过 v1 上界
//!   （`2^63-1`）是 schema 错误（§3.3「接收方不得截断、回绕或钳制」）；
//! - type：未知 `type` → `protocol.type_unsupported`（词表外的取值经 serde 折叠进 `Malformed`，因此这条
//!   判定在**失败路径**上重读帧的形状得出，见 [`malformed_inbound`]）；认证前收到只许认证后出现的消息是 4401 关闭
//!   而不是「schema 错误」（连接阶段本身就不对，只回一条可恢复的错误会让业务消息看起来「被接受但
//!   没生效」，§8）。
//!
//! 序号记账与 type/形状无关（§4.1）：`connectionSequence` 属于**对端发出的消息**，本机接不接受它的
//! type/body/信封形状都不影响对端已经用掉的序号。因此只要帧文本能读出「与本连接相同的 `connectionId`」
//! 与「规范十进制、不超过 v1 上界、正好等于期望值的 `connectionSequence`」，**每一条消息级拒绝都推进
//! 期望序号**（[`sequence_after_rejection`]）：一个显式拒绝不得让连接之后全部错位，否则连接退化成只出
//! 不进的哑管道（`openspec/changes/archive/2026-10-15-sync-server-connection/specs/sync-device-auth/spec.md` 的
//! 「协议错误码与入站序号推进」）。

use sync_protocol::common::{DecimalString, RawObject, Uuid};
use sync_protocol::envelope::{AuthState, Envelope, EnvelopeError, MessageType, Phase};
use sync_protocol::error::{Body, ErrorCode};

/// v1 的序号上界（§3.3）。超出即 `schema_invalid`。
pub(crate) const MAX_SEQUENCE: u64 = i64::MAX as u64;

/// 入站帧的判定结果。
pub(crate) enum Inbound {
    /// 通过阶段/信封/序号校验的消息。
    Message(Envelope),
    /// 消息不生效、连接保持可用：会话回 `error`（code/message 来自这里）。
    Rejected {
        code: ErrorCode,
        message: &'static str,
    },
    /// 致命：会话回 `error` 后按 `close_code` 关闭。
    Fatal {
        code: ErrorCode,
        message: &'static str,
        close_code: u16,
    },
}

/// 解码 + 阶段校验 + 序号校验。
///
/// `expected_sequence`：本方向下一个应到的序号（认证后从 `1` 开始）；返回 `Some(next)` 表示序号已推进。
/// `connection_id`：本连接在 `auth.server_challenge` 里下发的连接标识（认证前为 `None`）。
///
/// [`Envelope::decode`] 是唯一的帧解码入口（语法、信封形状、连接字段成对性都在它里面），因此本条路径
/// 对所有入站帧一致生效；区分 `invalid_json`/`version_unsupported`/`schema_invalid` 所需的重读只在信封
/// 解码已经失败时发生——合法帧不为此多付一次通用 JSON 解析。
pub(crate) fn judge(
    text: &str,
    phase: Phase,
    expected_sequence: Option<u64>,
    connection_id: Option<&Uuid>,
) -> (Inbound, Option<u64>) {
    let envelope = match Envelope::decode(text) {
        Ok(envelope) => envelope,
        Err(EnvelopeError::BodyNotJson(_)) => {
            return (
                Inbound::Rejected {
                    code: ErrorCode::ProtocolInvalidJson,
                    message: "the message body is not valid JSON",
                },
                None,
            );
        }
        Err(
            EnvelopeError::ConnectionFieldsMismatch
            | EnvelopeError::ConnectionFieldsForbidden { .. }
            | EnvelopeError::ConnectionFieldsRequired { .. },
        ) => {
            // 认证前的业务消息即使信封形状也不对（例如缺少连接字段），仍按 §8 以 4401 关闭：判定只看
            // `type`。否则一条「业务消息」会因为另一个 schema 缺陷退化成消息级拒绝、让连接活下来，与
            // 「认证完成前收到任何业务消息 MUST 以 4401 关闭」不符。重读帧只发生在解码已经失败的路径上。
            if phase == Phase::PreAuth && declares_post_auth_type(text) {
                return (
                    fatal(
                        ErrorCode::AuthRequired,
                        AUTH_REQUIRED_MESSAGE,
                        crate::sync::conn::close::UNAUTHENTICATED,
                    ),
                    None,
                );
            }
            // 其余情形按消息级拒绝。这里的三个变体里只有 `ConnectionFieldsForbidden`（认证后收到「认证前
            // 才合法」的 type）的连接字段**可读**：两者同时存在且序号正好是期望值时照常推进期望序号——
            // 那一帧已经在 wire 上占用了对端的下一个序号，不推进会让连接退化成只出不进的哑管道（§4.1；
            // 见 [`sequence_after_rejection`]）。`Mismatch`/`Required` 读不出成对的连接字段，该函数自己
            // 返回 `None`。
            return (
                Inbound::Rejected {
                    code: ErrorCode::ProtocolSchemaInvalid,
                    message: SCHEMA_MESSAGE,
                },
                sequence_after_rejection(text, phase, expected_sequence, connection_id),
            );
        }
        Err(EnvelopeError::UnknownType { .. } | EnvelopeError::Malformed(_)) => {
            // 未知 `type` 与语法/版本/信封形状共走这一族：`Wire::message_type` 的 `Deserialize` 把词表外的
            // 取值经 `serde::de::Error::custom` 折成字符串，`Envelope::decode` 于是把两者都报成
            // `Malformed`（`UnknownType` 在 Sync 侧实际不会被产出，列在合并分支里只为枚举完整）。细分靠
            // 重读帧的形状：非 JSON → `invalid_json`；`protocolVersion != 1` → 4406；`type` 是字符串但不在
            // 词表里 → `type_unsupported`；其余是信封/schema 形状错误。序号记账与已知 type 的路径一致。
            return malformed_inbound(text, phase, expected_sequence, connection_id);
        }
    };

    match phase {
        Phase::PreAuth => {
            // §8：认证完成前的业务消息（只许认证后出现的 type）以 4401 关闭；它们的 4401 优先于
            // post-auth 信封形状判定。
            if envelope.message_type().auth_state() == AuthState::PostAuth {
                return (
                    fatal(
                        ErrorCode::AuthRequired,
                        AUTH_REQUIRED_MESSAGE,
                        crate::sync::conn::close::UNAUTHENTICATED,
                    ),
                    None,
                );
            }
            // 认证前消息必须省略连接字段（§4.1）；带上它们是 schema 错误（消息不生效、连接可用）。
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
            // 认证后消息必须携带连接字段（§4.1）；缺失是 schema 错误。
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
                        close_code: crate::sync::conn::close::UNAVAILABLE,
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
            (Inbound::Message(envelope), Some(sequence + 1))
        }
    }
}

/// `Envelope::decode` 失败时的细分：非 JSON / 版本不符 / 词表外的 `type` / schema 形状错误。
///
/// `type` 的词表判定必须在**失败路径**上重读帧才能做：`Envelope::decode` 经 serde 解码，`type` 的
/// `Deserialize` 把词表外取值折成 `Malformed`（§4.2 要求它与 schema 形状错误回不同的错误码），因此这里
/// 按 JSON 形状再读一次 `type`——与 `protocolVersion` 的重读同形。合法帧不为这些重读多付一次解析。
///
/// 序号记账对**两条**细分路径逐条一致（[`sequence_after_rejection`]）：词表外的 `type` 与「已知 `type` 但
/// 信封另有形状错误」（未登记的信封字段、缺 `messageId`/`body` 等）都不改变「连接字段可读、序号正好是期望
/// 值」这一事实——那两种帧同样已经在 wire 上占用了对端的下一个序号，所以两条路径都**推进**入站期望序号
/// （§4.1 的序号属于对端已发出的消息）。只推进未知 `type` 那一条会留下同类哑管道：对端其后每条消息都被判
/// `protocol.sequence_invalid` 且永不推进，连接退化成只出不进。
fn malformed_inbound(
    text: &str,
    phase: Phase,
    expected_sequence: Option<u64>,
    connection_id: Option<&Uuid>,
) -> (Inbound, Option<u64>) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return (
            Inbound::Rejected {
                code: ErrorCode::ProtocolInvalidJson,
                message: "the frame is not valid JSON",
            },
            None,
        );
    };
    if value
        .get("protocolVersion")
        .and_then(serde_json::Value::as_u64)
        .is_some_and(|version| version != super::handshake::PROTOCOL_VERSION)
    {
        return (
            fatal(
                ErrorCode::ProtocolVersionUnsupported,
                VERSION_MESSAGE,
                crate::sync::conn::close::VERSION,
            ),
            None,
        );
    }
    if is_unknown_type(&value) {
        return (
            Inbound::Rejected {
                code: ErrorCode::ProtocolTypeUnsupported,
                message: TYPE_MESSAGE,
            },
            sequence_after_rejection(text, phase, expected_sequence, connection_id),
        );
    }
    // 已知 `type` 但信封另有形状错误：错误码是 `schema_invalid`，序号记账与上面那条同口径——连接字段
    // 可读且正好是期望值时同样推进，否则这条拒绝会把连接变成只出不进的哑管道。
    (
        Inbound::Rejected {
            code: ErrorCode::ProtocolSchemaInvalid,
            message: SCHEMA_MESSAGE,
        },
        sequence_after_rejection(text, phase, expected_sequence, connection_id),
    )
}

/// 帧的 `type` 是否是「是字符串、但不在 v1 词表里」。
///
/// 缺 `type`、`type` 不是字符串（形状本身就不对）都**不算**词表外：那些是 schema 错误，只有「自称一个
/// 不存在的消息类型」才按 §4.2 回 `protocol.type_unsupported`。
fn is_unknown_type(value: &serde_json::Value) -> bool {
    value
        .get("type")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|name| name.parse::<MessageType>().is_err())
}

/// 帧文本声明的 `type` 是否是「只允许认证后出现」的业务类型（§8 的 4401 判定输入）。
///
/// 只在信封解码已经失败、且阶段是认证前时调用：它不校验任何其它字段，只回答「这一帧自称是业务消息吗」。
fn declares_post_auth_type(text: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return false;
    };
    value
        .get("type")
        .and_then(serde_json::Value::as_str)
        .and_then(|name| name.parse::<MessageType>().ok())
        .is_some_and(|message_type| message_type.auth_state() == AuthState::PostAuth)
}

/// [`judge`]/[`malformed_inbound`] 在**解码失败**的消息级拒绝路径上的序号记账（§4.1 的「序号属于对端
/// 发出的消息」）。
///
/// 这里**没有**「信封形状已经验证过」这个前提：`Envelope::decode` 只会在 `Malformed` 一族上落到这些
/// 路径（词表外的 `type` 也被 serde 折进 `Malformed`，`UnknownType` 不会从 `decode` 产出），因此帧文本
/// 必须重新解析——失败路径上拿不到已解析的信封。记账只看四件事：阶段是认证后、`connectionId` 与本次连接
/// 相同、`connectionSequence` 是规范十进制串且不超过 v1 上界、正好等于期望值。任一不满足就不记账——对端
/// 没有为这条消息消耗它的下一个序号。
///
/// 条件命中时，无论拒绝码是 `type_unsupported` 还是 `schema_invalid`，都必须推进：帧已经在 wire 上占用
/// 了对端的下一个序号（`specs/sync-device-auth/spec.md` 的「协议错误码与入站序号推进」），不推进会让对端
/// 其后每条消息都被判 `protocol.sequence_invalid` 且永不推进，连接退化成只出不进的哑管道。
pub(crate) fn sequence_after_rejection(
    text: &str,
    phase: Phase,
    expected_sequence: Option<u64>,
    connection_id: Option<&Uuid>,
) -> Option<u64> {
    if phase != Phase::PostAuth {
        // 认证前的消息不携带连接字段（§4.1），也没有期望序号。
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

/// 认证前消息 type 的允许性（§8：连接建立后的第一条必须是 `auth.client_hello`，其后是
/// `auth.client_proof`；§14「认证前最多只接收 `client_hello` 和 `client_proof` 各一次」）。
///
/// `error` 在两种阶段都合法（`AuthState::Either`），因此始终允许。`auth.server_challenge` 是本机→对端
/// 的方向，反向收到即拒绝。
pub(crate) fn pre_auth_allowed(message_type: MessageType, expecting_proof: bool) -> bool {
    if message_type.auth_state() == AuthState::Either {
        return true;
    }
    if expecting_proof {
        message_type == MessageType::AuthClientProof
    } else {
        message_type == MessageType::AuthClientHello
    }
}

/// `error` 的 body（`schemas/sync/v1/error.schema.json` 的 `errorBody`）。
///
/// `details` 只允许 §12.2 登记表里的字段：未登记的 code 一律送 `{}`；`correlationId` 只在能指向触发本次
/// 错误的入站消息时给出。`retryable` 一律取错误码在 registry 里登记的语义。
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
    body.correlation_id = sync_protocol::common::Nullable::from_option(correlation_id);
    body.details = details;
    Some(body)
}

/// `details.features`（`protocol.feature_required` 的登记字段，§4.2/§12.2）。
///
/// 调用方保证已排序去重；本函数只负责编码（不可能失败时退回空 object，不让握手因装配细节失败）。
pub(crate) fn feature_details(missing: &[String]) -> RawObject {
    match serde_json::to_string(&serde_json::json!({ "features": missing })) {
        Ok(text) => RawObject::parse(&text).unwrap_or_else(|_| RawObject::empty()),
        Err(_) => RawObject::empty(),
    }
}

/// `details.retryAfterMs`（`resource.rate_limited` 的登记字段，§12.2）。
pub(crate) fn rate_limit_details(retry_after: std::time::Duration) -> RawObject {
    let text = serde_json::json!({ "retryAfterMs": retry_after.as_millis() as u64 }).to_string();
    RawObject::parse(&text).unwrap_or_else(|_| RawObject::empty())
}

pub(crate) const SCHEMA_MESSAGE: &str = "the message does not match the v1 schema";
pub(crate) const SEQUENCE_MESSAGE: &str = "the connection sequence is not the next expected value";
pub(crate) const TYPE_MESSAGE: &str = "the message type is not supported";
pub(crate) const VERSION_MESSAGE: &str = "the protocol version is not supported";
pub(crate) const AUTH_REQUIRED_MESSAGE: &str =
    "only auth.client_hello and auth.client_proof are allowed before authentication";
pub(crate) const INTERNAL_MESSAGE: &str = "the host cannot serve this connection";

const fn fatal(code: ErrorCode, message: &'static str, close_code: u16) -> Inbound {
    Inbound::Fatal {
        code,
        message,
        close_code,
    }
}
