//! `event` 帧的装配（`SYNC_PROTOCOL.md` §10.1）：core 的 [`SyncEventRecord`] → wire 的
//! `sync_protocol::event::Body`。
//!
//! 三条边界：
//!
//! - **字段逐项映射，不增不减**：`origin.kind` 五个取值原样往返（core 侧词表与 wire 侧同集），
//!   `causationRequestId` 原样（含 `null`），`remoteOrigin` 恒为 `null`（本模块只读 owned 事件，
//!   见 4.9 的登记口径）。
//! - **身份字段按 feature 门控**：`core.event-origin.v1` 协商后 `origin` 带 `nodeId` 键（没有节点
//!   产生者时取 `null`）；未协商时整键缺席——`additionalProperties: false` 下补一个 `"nodeId":null`
//!   会让旧客户端按 `protocol.schema_invalid` 拒绝整条事件（§10.1）。
//! - **无法组装即失败关闭**：任一线上值对象构造不出来（id/序号/时间戳形状不符）都返回
//!   [`EventFrameFault`]，调用方 MUST NOT 伪造字段、也 MUST NOT 静默跳过。
//!
//! 本模块不做可见性判定：scope 过滤与序号空洞是投递层的事（`subscribe::advance`）。

use acp_core::model::{
    AcpRaw as CoreAcpRaw, EventOrigin as CoreEventOrigin, EventPayload as CoreEventPayload,
};
use acp_core::ports::SyncEventRecord;
use sync_protocol::common::{
    Base64Url, DecimalString, Nullable, RawAcp, RawAcpUnavailableReason, RawObject, Timestamp, Uuid,
};
use sync_protocol::event::{Body, EventOrigin, EventOriginKind, EventType, Payload};

/// `core.event-origin.v1` 的门控：协商后 `origin` 带 `nodeId` 键与 `node` 取值（§10.1）。
pub const EVENT_ORIGIN_FEATURE: &str = "core.event-origin.v1";

/// 内嵌 ACP 原文的单条上限（§10.1：超过即 `acp.rawUnavailable{size_limit}`）。
const MAX_INLINE_ACP_BYTES: usize = 256 * 1024;

/// core 的 origin kind → wire 词表（五个取值一一对应；core 侧词表外取值在读面已是 `Corrupt`）。
fn wire_kind(kind: CoreEventOrigin) -> Option<EventOriginKind> {
    EventOriginKind::ALL
        .into_iter()
        .find(|candidate| candidate.as_str() == kind.as_str())
}

/// 一条已持久化事件 → `event` 帧的 body（§10.1 的全部字段）。
///
/// `error` 是显式失败关闭的载体：调用方记诊断并让该次订阅失败，而不是发一条缺字段的帧。
///
/// `negotiated` 是本连接对 `core.event-origin.v1` 的协商结果：协商后 `origin` 恒带 `nodeId` 键
/// （无节点产生者取 `null`），未协商时整键缺席。因此**同一个事件库**在两种连接上给出两种 shape，
/// 而 `kind`/`deviceId` 逐字节相同——`kind` 绝不因客户端版本降级（§10.1）。
pub fn event_body(record: &SyncEventRecord, negotiated: bool) -> Result<Body, EventFrameFault> {
    let event = &record.event;
    let kind = wire_kind(record.origin_kind).ok_or(EventFrameFault::OriginKind)?;
    let device_id = match &record.origin_device_id {
        Some(device) => Nullable::value(
            Uuid::parse(device.as_str())
                .map_err(|_| EventFrameFault::Identity { field: "deviceId" })?,
        ),
        None => Nullable::null(),
    };
    let node_id = if negotiated {
        Some(match &record.origin_node_id {
            Some(node) => Nullable::value(
                Uuid::parse(node.as_str())
                    .map_err(|_| EventFrameFault::Identity { field: "nodeId" })?,
            ),
            None => Nullable::null(),
        })
    } else {
        None
    };
    Ok(Body {
        global_sequence: decimal(&event.global_sequence.get().to_string())?,
        session_sequence: match &event.session_sequence {
            Some(sequence) => Nullable::value(decimal(&sequence.get().to_string())?),
            None => Nullable::null(),
        },
        event_id: Uuid::parse(event.id.as_str()).map_err(|_| EventFrameFault::EventId)?,
        session_id: match &event.session {
            Some(session) => Nullable::value(
                Uuid::parse(session.as_str()).map_err(|_| EventFrameFault::SessionId)?,
            ),
            None => Nullable::null(),
        },
        event_type: EventType::parse(event.event_type.as_str())
            .map_err(|_| EventFrameFault::EventType)?,
        causation_request_id: match &record.causation {
            Some(request) => Nullable::value(
                Uuid::parse(request.as_str()).map_err(|_| EventFrameFault::Causation)?,
            ),
            None => Nullable::null(),
        },
        origin: EventOrigin {
            kind,
            device_id,
            node_id,
        },
        // 本模块只读 owned 事件：imported 事件的回源投递本次不做（4.9），因此没有来源可写。
        remote_origin: Nullable::null(),
        created_at: Timestamp::parse(event.created_at.as_str())
            .map_err(|_| EventFrameFault::CreatedAt)?,
        payload: wire_payload(&record.payload)?,
    })
}

/// 十进制串（序号/byteLength 的线上形状）。
fn decimal(text: &str) -> Result<DecimalString, EventFrameFault> {
    DecimalString::parse(text).map_err(|_| EventFrameFault::Sequence)
}

/// `payload` 的映射（`view` 逐字节、`acp` 两个分支）。
fn wire_payload(payload: &CoreEventPayload) -> Result<Payload, EventFrameFault> {
    let view = RawObject::parse(payload.view.as_str()).map_err(|_| EventFrameFault::Payload)?;
    let acp = match payload.acp.as_ref() {
        Some(CoreAcpRaw::Available {
            media_type,
            raw_json,
            byte_length,
            sha256,
        }) => {
            let digest = Base64Url::<32>::parse(sha256.as_str())
                .map_err(|_| EventFrameFault::PayloadDigest)?;
            if media_type != "application/json" || raw_json.len() > MAX_INLINE_ACP_BYTES {
                // ① wire 的 `rawAcp.mediaType` 是固定值 `application/json`：其它 media type 无法诚实
                //    表达；② 超过 256 KiB 的内嵌内容必须显式降级而不是截断（§10.1）。
                let reason = if media_type != "application/json" {
                    RawAcpUnavailableReason::StorageFailure
                } else {
                    RawAcpUnavailableReason::SizeLimit
                };
                Some(RawAcp::Unavailable {
                    reason,
                    byte_length: decimal(&byte_length.to_string())?,
                    sha256: Nullable::value(digest),
                })
            } else {
                Some(RawAcp::Complete {
                    // `rawJson` 是 JSON **字符串**（内容是 ACP 原文档文本），因此这里编码一次字符串字面量。
                    raw_json: serde_json::value::RawValue::from_string(
                        serde_json::to_string(raw_json).map_err(|_| EventFrameFault::Payload)?,
                    )
                    .map_err(|_| EventFrameFault::Payload)?,
                    byte_length: decimal(&byte_length.to_string())?,
                    sha256: digest,
                })
            }
        }
        Some(CoreAcpRaw::Unavailable {
            reason,
            byte_length,
            sha256,
        }) => Some(RawAcp::Unavailable {
            reason: match reason {
                CoreRawUnavailableReason::SizeLimit => RawAcpUnavailableReason::SizeLimit,
                CoreRawUnavailableReason::RetentionExpired => {
                    RawAcpUnavailableReason::RetentionExpired
                }
                CoreRawUnavailableReason::StorageFailure => RawAcpUnavailableReason::StorageFailure,
            },
            byte_length: decimal(&byte_length.to_string())?,
            sha256: match sha256 {
                Some(digest) => Nullable::value(
                    Base64Url::<32>::parse(digest.as_str())
                        .map_err(|_| EventFrameFault::PayloadDigest)?,
                ),
                None => Nullable::null(),
            },
        }),
        None => None,
    };
    Ok(Payload { view, acp })
}

/// core 侧 `acp` 不可用原因（与 wire 词表一一对应）。
use acp_core::model::RawUnavailableReason as CoreRawUnavailableReason;

/// 无法按 §10.1 组装合法 `event` 帧的原因
///
/// 每一项都让该次投递**失败关闭**：构造不出合法帧时绝不能发一条缺字段/伪造字段的帧。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventFrameFault {
    /// `origin.kind` 是 wire 词表外取值（列值损坏；core 读面本应已失败关闭）。
    OriginKind,
    /// 身份字段的 UUID 形状不符。
    Identity {
        /// 出错的字段名（`deviceId`/`nodeId`）。
        field: &'static str,
    },
    /// `globalSequence`/`sessionSequence`/`byteLength` 的十进制串形状不符。
    Sequence,
    /// `eventId` 不是合法 UUID。
    EventId,
    /// `sessionId` 不是合法 UUID。
    SessionId,
    /// `eventType` 超出 `^[a-z0-9_.-]{1,128}$`。
    EventType,
    /// `causationRequestId` 不是合法 UUID。
    Causation,
    /// `createdAt` 不是合法时间戳。
    CreatedAt,
    /// `payload.view` 不是良构 JSON 对象。
    Payload,
    /// `payload.acp.sha256` 不是规范 base64url。
    PayloadDigest,
}

impl std::fmt::Display for EventFrameFault {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OriginKind => formatter.write_str("origin.kind"),
            Self::Identity { field } => write!(formatter, "origin.{field}"),
            Self::Sequence => formatter.write_str("globalSequence"),
            Self::EventId => formatter.write_str("eventId"),
            Self::SessionId => formatter.write_str("sessionId"),
            Self::EventType => formatter.write_str("eventType"),
            Self::Causation => formatter.write_str("causationRequestId"),
            Self::CreatedAt => formatter.write_str("createdAt"),
            Self::Payload => formatter.write_str("payload"),
            Self::PayloadDigest => formatter.write_str("payload.acp.sha256"),
        }
    }
}
