//! v1 信封与消息类型分派（`docs/SYNC_PROTOCOL.md` §4.1、`schemas/sync/v1/message.schema.json`）。
//!
//! `body` 以 `RawValue` **原样承载**：传输层只按 `type` 分派，不把 body 解析成通用 DTO 再重新
//! 序列化（`AGENTS.md` §3 的保真要求）。家族 body 由调用方用 `auth`/`control`/`error` 的类型
//! 显式解析；尚未实现 body 的家族保持字节原样转发，不降级成文本、不丢弃。

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;

use crate::common::{DecimalString, ProtocolVersionV1, Uuid};

/// v1 的全部消息类型，逐条对应 `schemas/sync/v1/*.schema.json` 里的 `properties.type.const`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageType {
    AuthClientHello,
    AuthServerChallenge,
    AuthClientProof,
    AuthAuthenticated,
    SyncSubscribe,
    SyncCaughtUp,
    SyncResetRequired,
    SyncSnapshotRequest,
    SyncSnapshotBegin,
    SyncSnapshotChunk,
    SyncSnapshotEnd,
    SyncAck,
    ControlPing,
    ControlPong,
    Error,
    Event,
    Command,
    CommandResult,
}

/// 消息类型在认证前后的归属，决定信封是否必须携带连接字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthState {
    /// 只允许在认证完成前出现，信封不得携带 `connectionId`/`connectionSequence`。
    PreAuth,
    /// 只允许在认证完成后出现，信封必须携带两者。
    PostAuth,
    /// 两种信封都合法（`error`，见 `schemas/sync/v1/error.schema.json` 的两个 `oneOf` 分支）。
    Either,
}

impl MessageType {
    /// 与 schema 的 `type` 常量逐条相等的完整集合，顺序即 `docs/SYNC_PROTOCOL.md` §4 的家族顺序。
    pub const ALL: [MessageType; 18] = [
        MessageType::AuthClientHello,
        MessageType::AuthServerChallenge,
        MessageType::AuthClientProof,
        MessageType::AuthAuthenticated,
        MessageType::SyncSubscribe,
        MessageType::SyncCaughtUp,
        MessageType::SyncResetRequired,
        MessageType::SyncSnapshotRequest,
        MessageType::SyncSnapshotBegin,
        MessageType::SyncSnapshotChunk,
        MessageType::SyncSnapshotEnd,
        MessageType::SyncAck,
        MessageType::ControlPing,
        MessageType::ControlPong,
        MessageType::Error,
        MessageType::Event,
        MessageType::Command,
        MessageType::CommandResult,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            MessageType::AuthClientHello => "auth.client_hello",
            MessageType::AuthServerChallenge => "auth.server_challenge",
            MessageType::AuthClientProof => "auth.client_proof",
            MessageType::AuthAuthenticated => "auth.authenticated",
            MessageType::SyncSubscribe => "sync.subscribe",
            MessageType::SyncCaughtUp => "sync.caught_up",
            MessageType::SyncResetRequired => "sync.reset_required",
            MessageType::SyncSnapshotRequest => "sync.snapshot_request",
            MessageType::SyncSnapshotBegin => "sync.snapshot_begin",
            MessageType::SyncSnapshotChunk => "sync.snapshot_chunk",
            MessageType::SyncSnapshotEnd => "sync.snapshot_end",
            MessageType::SyncAck => "sync.ack",
            MessageType::ControlPing => "control.ping",
            MessageType::ControlPong => "control.pong",
            MessageType::Error => "error",
            MessageType::Event => "event",
            MessageType::Command => "command",
            MessageType::CommandResult => "command.result",
        }
    }

    pub fn auth_state(self) -> AuthState {
        match self {
            MessageType::AuthClientHello
            | MessageType::AuthServerChallenge
            | MessageType::AuthClientProof => AuthState::PreAuth,
            MessageType::Error => AuthState::Either,
            _ => AuthState::PostAuth,
        }
    }
}

impl fmt::Display for MessageType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for MessageType {
    type Err = EnvelopeError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        MessageType::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| EnvelopeError::UnknownType {
                value: text.to_owned(),
            })
    }
}

impl Serialize for MessageType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for MessageType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        MessageType::from_str(&text).map_err(serde::de::Error::custom)
    }
}

/// 连接所处的认证阶段。
///
/// 连接字段规则由连接状态机决定，不能只按 `type` 静态判定：`error` 是唯一在两种阶段都合法、
/// 且形状由阶段决定的类型（`schemas/sync/v1/error.schema.json` 的两个 `oneOf` 分支）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// 握手阶段：信封不得携带 `connectionId`/`connectionSequence`。
    PreAuth,
    /// 认证完成后：信封必须携带两者。
    PostAuth,
}

/// 认证完成后的连接字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionFields {
    pub connection_id: Uuid,
    pub connection_sequence: DecimalString,
}

/// 信封错误。全部是显式错误：未知 `type`、未知字段、字段组合与 `type` 不符都必须拒绝，不静默接受。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EnvelopeError {
    /// 适配器必须把它映射为 `protocol.schema_invalid`（`docs/SYNC_PROTOCOL.md` §4.2）。
    #[error("信封不是合法 JSON 或不匹配 v1 形状：{0}")]
    Malformed(String),
    /// 适配器必须把它映射为 `protocol.type_unsupported`（`docs/SYNC_PROTOCOL.md` §4.2），
    /// 不得静默忽略。
    #[error("未知消息类型：{value}")]
    UnknownType { value: String },
    #[error("connectionId 与 connectionSequence 必须同时出现或同时省略")]
    ConnectionFieldsMismatch,
    #[error("{message_type} 在认证完成前出现，信封不得携带连接字段")]
    ConnectionFieldsForbidden { message_type: MessageType },
    #[error("{message_type} 在认证完成后出现，信封必须携带 connectionId 与 connectionSequence")]
    ConnectionFieldsRequired { message_type: MessageType },
    #[error("body 不是合法 JSON：{0}")]
    BodyNotJson(String),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    #[serde(rename = "protocolVersion")]
    protocol_version: ProtocolVersionV1,
    #[serde(rename = "type")]
    message_type: MessageType,
    #[serde(rename = "messageId")]
    message_id: Uuid,
    #[serde(rename = "connectionId", skip_serializing_if = "Option::is_none")]
    connection_id: Option<Uuid>,
    #[serde(rename = "connectionSequence", skip_serializing_if = "Option::is_none")]
    connection_sequence: Option<DecimalString>,
    #[serde(rename = "body")]
    body: Box<RawValue>,
}

/// v1 信封：`{protocolVersion, type, messageId}` + 认证后的 `{connectionId, connectionSequence}`。
#[derive(Debug, Clone)]
pub struct Envelope {
    message_type: MessageType,
    message_id: Uuid,
    connection: Option<ConnectionFields>,
    body: Box<RawValue>,
}

impl PartialEq for Envelope {
    fn eq(&self, other: &Self) -> bool {
        self.message_type == other.message_type
            && self.message_id == other.message_id
            && self.connection == other.connection
            && self.body.get() == other.body.get()
    }
}

impl Envelope {
    /// 校验信封形状，并把 body 保持为原始字节。
    pub fn decode(text: &str) -> Result<Self, EnvelopeError> {
        let wire: Wire = serde_json::from_str(text)
            .map_err(|error| EnvelopeError::Malformed(error.to_string()))?;
        Self::assemble(
            wire.message_type,
            wire.message_id,
            wire.connection_id,
            wire.connection_sequence,
            wire.body,
        )
    }

    /// 校验信封形状，并把 body 保持为原始字节；随后按 `phase` 施加连接字段规则。
    pub fn decode_in_phase(text: &str, phase: Phase) -> Result<Self, EnvelopeError> {
        let envelope = Self::decode(text)?;
        envelope.validate_phase(phase)?;
        Ok(envelope)
    }

    /// 组装信封，并施加与 [`Envelope::decode`] 完全相同的连接字段规则。
    pub fn new(
        message_type: MessageType,
        message_id: Uuid,
        connection: Option<ConnectionFields>,
        body: Box<RawValue>,
    ) -> Result<Self, EnvelopeError> {
        match (message_type.auth_state(), connection.is_some()) {
            (AuthState::PreAuth, true) => {
                Err(EnvelopeError::ConnectionFieldsForbidden { message_type })
            }
            (AuthState::PostAuth, false) => {
                Err(EnvelopeError::ConnectionFieldsRequired { message_type })
            }
            _ => Ok(Self {
                message_type,
                message_id,
                connection,
                body,
            }),
        }
    }

    fn assemble(
        message_type: MessageType,
        message_id: Uuid,
        connection_id: Option<Uuid>,
        connection_sequence: Option<DecimalString>,
        body: Box<RawValue>,
    ) -> Result<Self, EnvelopeError> {
        let connection = match (connection_id, connection_sequence) {
            (Some(connection_id), Some(connection_sequence)) => Some(ConnectionFields {
                connection_id,
                connection_sequence,
            }),
            (None, None) => None,
            _ => return Err(EnvelopeError::ConnectionFieldsMismatch),
        };
        Self::new(message_type, message_id, connection, body)
    }

    pub fn message_type(&self) -> MessageType {
        self.message_type
    }

    pub fn message_id(&self) -> &Uuid {
        &self.message_id
    }

    pub fn connection(&self) -> Option<&ConnectionFields> {
        self.connection.as_ref()
    }

    /// 按连接状态校验连接字段：认证前必须缺席，认证后必须存在。
    ///
    /// [`Envelope::decode`] 只做 schema 级校验——`error` 在两种阶段都合法且形状由阶段决定，
    /// 解码本身无法判定阶段，因此连接状态机必须再调用本方法。
    pub fn validate_phase(&self, phase: Phase) -> Result<(), EnvelopeError> {
        match (phase, self.connection.is_some()) {
            (Phase::PreAuth, true) => Err(EnvelopeError::ConnectionFieldsForbidden {
                message_type: self.message_type,
            }),
            (Phase::PostAuth, false) => Err(EnvelopeError::ConnectionFieldsRequired {
                message_type: self.message_type,
            }),
            _ => Ok(()),
        }
    }

    /// body 的原始字节，未经任何通用 DTO 往返。
    pub fn body(&self) -> &RawValue {
        &self.body
    }

    pub fn encode(&self) -> Result<String, EnvelopeError> {
        let wire = Wire {
            protocol_version: ProtocolVersionV1::new(1).expect("1 在 1..=1 内"),
            message_type: self.message_type,
            message_id: self.message_id.clone(),
            connection_id: self
                .connection
                .as_ref()
                .map(|fields| fields.connection_id.clone()),
            connection_sequence: self
                .connection
                .as_ref()
                .map(|fields| fields.connection_sequence.clone()),
            body: self.body.clone(),
        };
        serde_json::to_string(&wire).map_err(|error| EnvelopeError::Malformed(error.to_string()))
    }
}

/// 把家族 body 序列化成信封可承载的原始 JSON。
pub fn encode_body<T: Serialize>(body: &T) -> Result<Box<RawValue>, EnvelopeError> {
    let text = serde_json::to_string(body)
        .map_err(|error| EnvelopeError::BodyNotJson(error.to_string()))?;
    RawValue::from_string(text).map_err(|error| EnvelopeError::BodyNotJson(error.to_string()))
}
