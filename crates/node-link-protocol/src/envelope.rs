//! v1 信封与消息类型分派（`docs/NODE_LINK_PROTOCOL.md` §2.2、`schemas/node-link/v1/message.schema.json`）。
//!
//! `body` 以 `RawValue` **原样承载**：传输层只按 `type` 分派，不把 body 解析成通用 DTO 再重新
//! 序列化（`AGENTS.md` §3 的保真要求）。家族 body 由调用方用 `handshake`/`catalog`/`error`/
//! `resource`/`command` 的类型显式解析；尚未实现 body 的家族保持字节原样转发，不降级成文本、不丢弃。
//!
//! 本模块不依赖任何家族 body 类型：`MessageType` 只是 `type` 的封闭词表，body 的解释权在调用方。

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;

use crate::common::{DecimalString, Uuid};

/// 唯一受支持的 major wire version（`docs/NODE_LINK_PROTOCOL.md` §2.2 的 `protocolVersion`）。
///
/// 取值域与 [`crate::common::ProtocolVersionV1`]（`BoundedU64<1, 1>`）相同；解码时显式判定，
/// 是为了把「版本非 1」与「JSON 畸形」分成两个可区分的错误，而不是都塌进 `Malformed`。
const PROTOCOL_VERSION_V1: u64 = 1;

/// v1 的全部消息类型，逐条对应 `schemas/node-link/v1/*.schema.json` 里的 `properties.type.const`。
///
/// 抽取规则与 `scripts/check-schema-fixtures.mjs` 的 `declaredTypes` 相同：从
/// `message.schema.json` 的 `oneOf` 出发，沿 `$ref`/`allOf`/`oneOf`/`anyOf` 收集分支根上的
/// `properties.type.const`，**不**进入 `properties`（否则 body 与 `$defs` 内部引用的物体会混进
/// 消息名集合）。顺序即 `docs/NODE_LINK_PROTOCOL.md` §12.8 的「消息名 ↔ schema 对照」表。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageType {
    NodeHello,
    NodeChallenge,
    NodeProof,
    NodeReady,
    CatalogSubscribe,
    CatalogSnapshot,
    CatalogChanged,
    ExportRevoked,
    NodeTrustRevoked,
    NodeRotateKeyRequest,
    NodeRotateKeyResult,
    ResourceAttach,
    ResourceAttached,
    ResourceDetach,
    ResourceSubscribe,
    ResourceSnapshotBegin,
    ResourceSnapshotChunk,
    ResourceSnapshotEnd,
    ResourceEvent,
    ResourceAck,
    CommandSubmit,
    CommandAccepted,
    CommandRejected,
    CommandTerminal,
    CommandStatus,
    LinkError,
    LinkPing,
    LinkPong,
    LinkBackpressure,
}

/// 消息类型在认证前后的归属，决定信封是否必须携带连接字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthState {
    /// 只允许在认证完成前出现，信封不得携带 `connectionId`/`connectionSequence`。
    PreAuth,
    /// 只允许在认证完成后出现，信封必须携带两者。
    PostAuth,
    /// 两种信封都合法（`link.error`，见 `docs/NODE_LINK_PROTOCOL.md` §2.2 与
    /// `schemas/node-link/v1/error.schema.json` 的两个 `oneOf` 分支）。
    Either,
}

impl MessageType {
    /// 与 schema 的 `type` 常量逐条相等的完整集合。
    pub const ALL: [MessageType; 29] = [
        MessageType::NodeHello,
        MessageType::NodeChallenge,
        MessageType::NodeProof,
        MessageType::NodeReady,
        MessageType::CatalogSubscribe,
        MessageType::CatalogSnapshot,
        MessageType::CatalogChanged,
        MessageType::ExportRevoked,
        MessageType::NodeTrustRevoked,
        MessageType::NodeRotateKeyRequest,
        MessageType::NodeRotateKeyResult,
        MessageType::ResourceAttach,
        MessageType::ResourceAttached,
        MessageType::ResourceDetach,
        MessageType::ResourceSubscribe,
        MessageType::ResourceSnapshotBegin,
        MessageType::ResourceSnapshotChunk,
        MessageType::ResourceSnapshotEnd,
        MessageType::ResourceEvent,
        MessageType::ResourceAck,
        MessageType::CommandSubmit,
        MessageType::CommandAccepted,
        MessageType::CommandRejected,
        MessageType::CommandTerminal,
        MessageType::CommandStatus,
        MessageType::LinkError,
        MessageType::LinkPing,
        MessageType::LinkPong,
        MessageType::LinkBackpressure,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            MessageType::NodeHello => "node.hello",
            MessageType::NodeChallenge => "node.challenge",
            MessageType::NodeProof => "node.proof",
            MessageType::NodeReady => "node.ready",
            MessageType::CatalogSubscribe => "catalog.subscribe",
            MessageType::CatalogSnapshot => "catalog.snapshot",
            MessageType::CatalogChanged => "catalog.changed",
            MessageType::ExportRevoked => "export.revoked",
            MessageType::NodeTrustRevoked => "node.trust.revoked",
            MessageType::NodeRotateKeyRequest => "node.rotate-key.request",
            MessageType::NodeRotateKeyResult => "node.rotate-key.result",
            MessageType::ResourceAttach => "resource.attach",
            MessageType::ResourceAttached => "resource.attached",
            MessageType::ResourceDetach => "resource.detach",
            MessageType::ResourceSubscribe => "resource.subscribe",
            MessageType::ResourceSnapshotBegin => "resource.snapshot_begin",
            MessageType::ResourceSnapshotChunk => "resource.snapshot_chunk",
            MessageType::ResourceSnapshotEnd => "resource.snapshot_end",
            MessageType::ResourceEvent => "resource.event",
            MessageType::ResourceAck => "resource.ack",
            MessageType::CommandSubmit => "command.submit",
            MessageType::CommandAccepted => "command.accepted",
            MessageType::CommandRejected => "command.rejected",
            MessageType::CommandTerminal => "command.terminal",
            MessageType::CommandStatus => "command.status",
            MessageType::LinkError => "link.error",
            MessageType::LinkPing => "link.ping",
            MessageType::LinkPong => "link.pong",
            MessageType::LinkBackpressure => "link.backpressure",
        }
    }

    /// 认证前后的归属（`docs/NODE_LINK_PROTOCOL.md` §2.2、§12.1）。
    ///
    /// `node.ready` 虽然仍属握手流程，但它是**认证后**的第一条消息，信封必须携带连接字段；
    /// `link.error` 在两种阶段都会出现，形状由阶段决定，因此单独归为 [`AuthState::Either`]。
    pub fn auth_state(self) -> AuthState {
        match self {
            MessageType::NodeHello | MessageType::NodeChallenge | MessageType::NodeProof => {
                AuthState::PreAuth
            }
            MessageType::LinkError => AuthState::Either,
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

    /// 未知 `type` 必须显式失败（`docs/NODE_LINK_PROTOCOL.md` §2.4：返回
    /// `nodelink.protocol.type_unsupported`，不得静默丢弃）；`post_mvp` 消息族在 v1 首切片
    /// 也按未知 type 处理（§12.1）。
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
/// 连接字段规则由连接状态机决定，不能只按 `type` 静态判定：`link.error` 是唯一在两种阶段都合法、
/// 且形状由阶段决定的类型（`schemas/node-link/v1/error.schema.json` 的两个 `oneOf` 分支）。
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
    /// JSON 畸形、未知信封字段，或信封字段的类型/必需性不符合 §2.2 的形状。
    ///
    /// 适配器必须把它映射为 `nodelink.protocol.invalid_json` 或
    /// `nodelink.protocol.schema_invalid`（`docs/NODE_LINK_PROTOCOL.md` §2.4）。
    #[error("信封不是合法 JSON 或不匹配 v1 形状：{0}")]
    Malformed(String),
    /// 适配器必须把它映射为 `nodelink.protocol.type_unsupported`（`docs/NODE_LINK_PROTOCOL.md`
    /// §2.4），不得静默忽略。
    #[error("未知消息类型：{value}")]
    UnknownType { value: String },
    /// `protocolVersion` 不是 v1：适配器必须映射为 `nodelink.protocol.version_unsupported`（§2.3）。
    #[error("protocolVersion 必须为 1，实际 {found}")]
    UnsupportedVersion { found: u64 },
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
    protocol_version: u64,
    #[serde(rename = "type")]
    message_type: String,
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
///
/// `body` 保存的是该帧里 `body` 成员的**原始字节**（`RawValue` 保留输入子串），`encode` 原样写回，
/// 因此本类型不会因为"认不出 body"而重排键、改写数字或丢弃未知字段。
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
        Self::assemble(wire)
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

    fn assemble(wire: Wire) -> Result<Self, EnvelopeError> {
        if wire.protocol_version != PROTOCOL_VERSION_V1 {
            return Err(EnvelopeError::UnsupportedVersion {
                found: wire.protocol_version,
            });
        }
        let message_type = MessageType::from_str(&wire.message_type)?;
        let connection = match (wire.connection_id, wire.connection_sequence) {
            (Some(connection_id), Some(connection_sequence)) => Some(ConnectionFields {
                connection_id,
                connection_sequence,
            }),
            (None, None) => None,
            _ => return Err(EnvelopeError::ConnectionFieldsMismatch),
        };
        Self::new(message_type, wire.message_id, connection, wire.body)
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
    /// [`Envelope::decode`] 只做 schema 级校验——`link.error` 在两种阶段都合法且形状由阶段决定，
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
            protocol_version: PROTOCOL_VERSION_V1,
            message_type: self.message_type.as_str().to_owned(),
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
