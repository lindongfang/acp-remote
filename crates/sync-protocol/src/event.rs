//! `event` 消息的 body（`schemas/sync/v1/event.schema.json`，语义见 `docs/SYNC_PROTOCOL.md` §10.1）。
//!
//! 本模块只做 wire 形状与取值域校验；`payload.view` 按 `eventType` 的类型化投影在 [`crate::views`]。
//!
//! 关于 `remoteOrigin` 的一致性：schema 用 `required` + `oneOf` 表达"键必须存在、值可为 `null`"，
//! **没有** `if/then`，且 `origin.kind` 的取值域里不存在 `remote`（`docs/SYNC_PROTOCOL.md` §10.1
//! 明确写着"远程节点不作为 `origin.kind` 取值"，imported 事件的来源只写在 `remoteOrigin` 里）。
//! 因此这里校验文档里**可判定**的那条一致性要求：`remoteOrigin` 非 `null` 时
//! `eventId` 必须等于 `remoteOrigin.originEventId`（§9.6 与 §10.1 各写了一遍）。

use std::fmt;
use std::str::FromStr;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::common::{
    DecimalString, Nullable, RawAcp, RawObject, RemoteOrigin, Timestamp, Uuid, ValueError,
    deserialize_optional_non_null,
};

/// `body.eventType`：`^[a-z0-9_.-]{1,128}$`。
///
/// 字符集合法但未登记的取值仍是合法事件类型——`docs/SYNC_PROTOCOL.md` §10.1 把事件类型定义为
/// 开放集合，客户端必须为未知类型提供降级视图；已登记的取值见 [`crate::views::VIEW_TYPES`]。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventType(String);

impl EventType {
    pub fn parse(text: &str) -> Result<Self, ValueError> {
        let length = text.chars().count();
        let charset_ok = text.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'.' | b'-')
        });
        if length == 0 || length > 128 || !charset_ok {
            return Err(ValueError::Enumerated {
                field: "eventType",
                value: text.to_owned(),
            });
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for EventType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for EventType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(DeError::custom)
    }
}

/// `body.origin.kind`：`docs/SYNC_PROTOCOL.md` §10.1 的四个产生来源。
///
/// 没有 `remote`：远程跳数只由 `remoteOrigin` 表达，不放进 `kind`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventOriginKind {
    Agent,
    Device,
    Daemon,
    LocalCli,
}

impl EventOriginKind {
    pub const ALL: [EventOriginKind; 4] = [
        EventOriginKind::Agent,
        EventOriginKind::Device,
        EventOriginKind::Daemon,
        EventOriginKind::LocalCli,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            EventOriginKind::Agent => "agent",
            EventOriginKind::Device => "device",
            EventOriginKind::Daemon => "daemon",
            EventOriginKind::LocalCli => "local_cli",
        }
    }
}

impl fmt::Display for EventOriginKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for EventOriginKind {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        EventOriginKind::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "origin.kind",
                value: text.to_owned(),
            })
    }
}

impl Serialize for EventOriginKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for EventOriginKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse::<Self>().map_err(DeError::custom)
    }
}

/// `body.origin`：形状是关闭的（`additionalProperties: false`），两个键都必需，`deviceId` 可为 `null`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventOrigin {
    pub kind: EventOriginKind,
    #[serde(rename = "deviceId")]
    pub device_id: Nullable<Uuid>,
}

/// `body.payload`：形状是关闭的，`view` 必需、`acp` 可选且没有 `null` 分支。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payload {
    /// 公共结构化视图。它是 schema 的 `{"type": "object"}`：原样保留字节（键序、空白、超出 u64
    /// 的整数字面量都不改写），解析成类型化视图见 [`crate::views::project`]。
    pub view: RawObject,
    /// ACP 原文或其不可用说明（`common.schema.json#/$defs/rawAcp` 的两个分支）。
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub acp: Option<RawAcp>,
}

/// `event` 的 body（`schemas/sync/v1/event.schema.json` 的 `body`）。
///
/// 结构体本身不派生 `Deserialize`：`remoteOrigin` 与 `eventId` 的一致性要求跨字段校验，
/// 由 [`RawBody`] 反序列化后再 [`TryFrom`] 完成（`try_from` 是 serde 为跨字段校验提供的机制）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "RawBody")]
pub struct Body {
    #[serde(rename = "globalSequence")]
    pub global_sequence: DecimalString,
    #[serde(rename = "sessionSequence")]
    pub session_sequence: Nullable<DecimalString>,
    #[serde(rename = "eventId")]
    pub event_id: Uuid,
    #[serde(rename = "sessionId")]
    pub session_id: Nullable<Uuid>,
    #[serde(rename = "eventType")]
    pub event_type: EventType,
    #[serde(rename = "causationRequestId")]
    pub causation_request_id: Nullable<Uuid>,
    pub origin: EventOrigin,
    #[serde(rename = "remoteOrigin")]
    pub remote_origin: Nullable<RemoteOrigin>,
    #[serde(rename = "createdAt")]
    pub created_at: Timestamp,
    pub payload: Payload,
}

/// [`Body`] 的线形状：关闭形状在这里落地，字段与 `Body` 一一对应（见 [`TryFrom`] 的实现）。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBody {
    #[serde(rename = "globalSequence")]
    global_sequence: DecimalString,
    #[serde(rename = "sessionSequence")]
    session_sequence: Nullable<DecimalString>,
    #[serde(rename = "eventId")]
    event_id: Uuid,
    #[serde(rename = "sessionId")]
    session_id: Nullable<Uuid>,
    #[serde(rename = "eventType")]
    event_type: EventType,
    #[serde(rename = "causationRequestId")]
    causation_request_id: Nullable<Uuid>,
    origin: EventOrigin,
    #[serde(rename = "remoteOrigin")]
    remote_origin: Nullable<RemoteOrigin>,
    #[serde(rename = "createdAt")]
    created_at: Timestamp,
    payload: Payload,
}

impl TryFrom<RawBody> for Body {
    type Error = ValueError;

    fn try_from(raw: RawBody) -> Result<Self, Self::Error> {
        // `docs/SYNC_PROTOCOL.md` §9.6/§10.1：imported 事件的 `eventId` 等于 `remoteOrigin.originEventId`。
        // 本地事件（`remoteOrigin` 为 null）不受此约束。
        if let Some(remote) = raw.remote_origin.as_ref() {
            if remote.origin_event_id != raw.event_id {
                return Err(ValueError::Shape {
                    field: "remoteOrigin",
                    expected: "remoteOrigin 非 null 时 eventId 必须等于 remoteOrigin.originEventId（SYNC_PROTOCOL §9.6）",
                });
            }
        }

        Ok(Self {
            global_sequence: raw.global_sequence,
            session_sequence: raw.session_sequence,
            event_id: raw.event_id,
            session_id: raw.session_id,
            event_type: raw.event_type,
            causation_request_id: raw.causation_request_id,
            origin: raw.origin,
            remote_origin: raw.remote_origin,
            created_at: raw.created_at,
            payload: raw.payload,
        })
    }
}
