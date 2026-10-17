//! Node Link v1 的协议专属公共类型（`schemas/node-link/v1/common.schema.json`）。
//!
//! 跨协议共用的值对象与字段校验机制（`uuid`、`decimalString`、`timestamp`、`base64url*`、`featureId`、
//! `featureList`、`rawAcp`、`Nullable`、`RawObject`、`ExtraFields`、`Text`、`NonEmptyText`、`BoundedU64`、
//! `ValueError`）来自叶子 crate `acpr-wire`（[ADR-0007](../../../docs/adr/0007-shared-wire-value-crate.md)），
//! 这里只做命名空间再导出；本文件拥有的是 Node Link 自己的值对象——节点、Export、workspace 模板、origin
//! cursor、会话元数据与 limits。
//!
//! 校验只发生在反序列化：构造出来的值必然满足对应 schema，后续层不需要重复校验。

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub use acpr_wire::{
    Base64Url, BoundedU64, DecimalString, ExtraFields, FeatureId, FeatureList, NonEmptyText,
    Nullable, ProtocolVersionV1, RawAcp, RawObject, Text, Timestamp, UIntAtLeast, Uuid, ValueError,
    deserialize_optional_non_null,
};

/// `common.schema.json#/$defs/publicError`，以 Node Link 自己的错误码枚举具体化。
///
/// 类型定义在 `crate::error`（错误码词表在那里），这里只做命名空间再导出，让家族模块统一从
/// `crate::common` 取值。
pub use crate::error::PublicError;

/// `common.schema.json#/$defs/nodeName`：1..=128 的文本，无 pattern。
pub type NodeName = NonEmptyText<128>;

/// 按 schema 的 `pattern`（与可选长度上限）校验的字符串 newtype。
///
/// 这些类型共享同一套「取值域只在反序列化校验」的形状，用宏生成可以保证四个 impl 不会各自写歪；
/// 每个类型的文档注释仍指向它对应的 `$defs`。
macro_rules! pattern_string {
    ($(#[$meta:meta])* $name:ident, $pattern:expr, $max:expr) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// 按 schema 的 `pattern` 校验；`$max` 是字符数上限。
            pub fn parse(text: &str) -> Result<Self, ValueError> {
                let valid =
                    !text.is_empty() && text.chars().count() <= $max && $pattern(text);
                if !valid {
                    return Err(ValueError::Pattern {
                        field: stringify!($name),
                        value: text.to_owned(),
                    });
                }
                Ok(Self(text.to_owned()))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = String::deserialize(deserializer)?;
                Self::parse(&text).map_err(DeError::custom)
            }
        }
    };
}

/// 只含 ASCII 字母、数字、`_`、`.`、`-` 的文本（schema 的 `^[A-Za-z0-9._-]+$`）。
fn is_ascii_ref(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

/// `^[0-9a-f]{32}$`：由规范化路径派生的目录标识（不是路径）。
fn is_directory_id(text: &str) -> bool {
    text.len() == 32
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pattern_string!(
    /// `common.schema.json#/$defs/agentId`：1..=128 的文本（schema 只约束长度）。
    AgentId,
    |text: &str| !text.is_empty(),
    128
);
pattern_string!(
    /// `common.schema.json#/$defs/opaqueRef`：Owner 侧不透明引用，接收方只能原样回传。
    OpaqueRef,
    is_ascii_ref,
    128
);
pattern_string!(
    /// `common.schema.json#/$defs/directoryId`：`^[0-9a-f]{32}$`，不透明标识、不是路径。
    DirectoryId,
    is_directory_id,
    32
);

/// `common.schema.json#/$defs/nodeKind`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Owner,
    Access,
}

impl NodeKind {
    pub const ALL: [NodeKind; 2] = [NodeKind::Owner, NodeKind::Access];

    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Owner => "owner",
            NodeKind::Access => "access",
        }
    }
}

impl fmt::Display for NodeKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for NodeKind {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        NodeKind::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "nodeKind",
                value: text.to_owned(),
            })
    }
}

impl Serialize for NodeKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for NodeKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        NodeKind::from_str(&text).map_err(DeError::custom)
    }
}

/// `common.schema.json#/$defs/originCursor`：跨节点排序一律以 `(originEpoch, originSequence)` 为准。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginCursor {
    #[serde(rename = "originEpoch")]
    pub origin_epoch: Uuid,
    #[serde(rename = "originSequence")]
    pub origin_sequence: DecimalString,
}

/// `common.schema.json#/$defs/remoteSessionRef`：Owner 侧会话的跨节点引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteSessionRef {
    #[serde(rename = "ownerNodeId")]
    pub owner_node_id: Uuid,
    #[serde(rename = "sessionId")]
    pub session_id: Uuid,
}

/// `common.schema.json#/$defs/sessionState`：与 Sync 的 `sessionSummary.state` 同一取值集合。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionState {
    Idle,
    Queued,
    Running,
    WaitingInput,
    WaitingPermission,
    Failed,
    Closed,
}

impl SessionState {
    pub const ALL: [SessionState; 7] = [
        SessionState::Idle,
        SessionState::Queued,
        SessionState::Running,
        SessionState::WaitingInput,
        SessionState::WaitingPermission,
        SessionState::Failed,
        SessionState::Closed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SessionState::Idle => "idle",
            SessionState::Queued => "queued",
            SessionState::Running => "running",
            SessionState::WaitingInput => "waiting_input",
            SessionState::WaitingPermission => "waiting_permission",
            SessionState::Failed => "failed",
            SessionState::Closed => "closed",
        }
    }
}

impl fmt::Display for SessionState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for SessionState {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        SessionState::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "state",
                value: text.to_owned(),
            })
    }
}

impl Serialize for SessionState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SessionState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        SessionState::from_str(&text).map_err(DeError::custom)
    }
}

/// `common.schema.json#/$defs/sessionMeta`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionMeta {
    pub state: SessionState,
    pub version: DecimalString,
}

/// `common.schema.json#/$defs/sessionCreateResult`：`session.create` 的成功终态结果。
///
/// `command.accepted` 的 `result` 必须为 `null`；会话创建完成后由 `command.terminal` 携带本形状
/// （`docs/NODE_LINK_PROTOCOL.md` §12.7）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionCreateResult {
    #[serde(rename = "remoteSessionRef")]
    pub remote_session_ref: RemoteSessionRef,
    #[serde(rename = "sessionMeta")]
    pub session_meta: SessionMeta,
}

/// `common.schema.json#/$defs/sessionResumeResult`：`session.resume` 的成功终态结果。
///
/// 字段形状与 [`SessionCreateResult`] 一致（`remoteSessionRef` + `sessionMeta`）；恢复完成后由
/// `command.terminal` 携带本形状（`docs/NODE_LINK_PROTOCOL.md` §12.7）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionResumeResult {
    #[serde(rename = "remoteSessionRef")]
    pub remote_session_ref: RemoteSessionRef,
    #[serde(rename = "sessionMeta")]
    pub session_meta: SessionMeta,
}

/// `common.schema.json#/$defs/pendingInteraction.kind`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InteractionKind {
    Permission,
    Elicitation,
}

impl InteractionKind {
    pub const ALL: [InteractionKind; 2] =
        [InteractionKind::Permission, InteractionKind::Elicitation];

    pub fn as_str(self) -> &'static str {
        match self {
            InteractionKind::Permission => "permission",
            InteractionKind::Elicitation => "elicitation",
        }
    }
}

impl fmt::Display for InteractionKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for InteractionKind {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        InteractionKind::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "kind",
                value: text.to_owned(),
            })
    }
}

impl Serialize for InteractionKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for InteractionKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        InteractionKind::from_str(&text).map_err(DeError::custom)
    }
}

/// `common.schema.json#/$defs/pendingInteraction`：待处理交互的索引条目（无正文，只有摘要）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingInteraction {
    #[serde(rename = "interactionId")]
    pub interaction_id: Uuid,
    pub kind: InteractionKind,
    #[serde(rename = "createdAt")]
    pub created_at: Timestamp,
    #[serde(rename = "payloadDigest")]
    pub payload_digest: Base64Url<32>,
}

/// `common.schema.json#/$defs/agentEntry`：本机配置声明的 Agent。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentEntry {
    #[serde(rename = "agentId")]
    pub agent_id: AgentId,
    pub name: NonEmptyText<128>,
    #[serde(rename = "capabilitiesRef")]
    pub capabilities_ref: OpaqueRef,
}

/// `common.schema.json#/$defs/directoryEntry`：本机已登记的目录标识（不含路径）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectoryEntry {
    #[serde(rename = "directoryId")]
    pub directory_id: DirectoryId,
}

/// `common.schema.json#/$defs/limits`：`node.ready` 下发的 Node Link 限额。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeLinkLimits {
    #[serde(rename = "maxMessageBytes")]
    pub max_message_bytes: UIntAtLeast<1024>,
    #[serde(rename = "catalogSnapshotBatchSize")]
    pub catalog_snapshot_batch_size: UIntAtLeast<1>,
    #[serde(rename = "resourceSnapshotBatchSize")]
    pub resource_snapshot_batch_size: UIntAtLeast<1>,
    #[serde(rename = "maxInFlightCommands")]
    pub max_in_flight_commands: UIntAtLeast<1>,
    #[serde(rename = "maxPendingQueueBytes")]
    pub max_pending_queue_bytes: UIntAtLeast<1024>,
    #[serde(rename = "maxPendingQueueMessages")]
    pub max_pending_queue_messages: UIntAtLeast<1>,
    #[serde(rename = "heartbeatIntervalMs")]
    pub heartbeat_interval_ms: BoundedU64<1000, 300_000>,
}

/// `common.schema.json#/$defs/transcriptCodec`：与 `SYNC_PROTOCOL.md` §6.2 同一二进制 codec。
pub const TRANSCRIPT_CODEC: &str = "acpr-transcript-v1";

/// `common.schema.json#/$defs/transcriptDomain`：Node Link 的五个 domain。
///
/// 取值必须与 `crate::domains::DOMAINS` 的表逐条一致（由契约测试断言）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TranscriptDomain {
    PairingProof,
    PairingOwnerProof,
    PairingStatus,
    Challenge,
    Proof,
}

impl TranscriptDomain {
    pub const ALL: [TranscriptDomain; 5] = [
        TranscriptDomain::PairingProof,
        TranscriptDomain::PairingOwnerProof,
        TranscriptDomain::PairingStatus,
        TranscriptDomain::Challenge,
        TranscriptDomain::Proof,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            TranscriptDomain::PairingProof => "acp-remote/node-link-pairing-proof/v1",
            TranscriptDomain::PairingOwnerProof => "acp-remote/node-link-pairing-owner-proof/v1",
            TranscriptDomain::PairingStatus => "acp-remote/node-link-pairing-status/v1",
            TranscriptDomain::Challenge => "acp-remote/node-link-challenge/v1",
            TranscriptDomain::Proof => "acp-remote/node-link-proof/v1",
        }
    }
}

impl fmt::Display for TranscriptDomain {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for TranscriptDomain {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        TranscriptDomain::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "domain",
                value: text.to_owned(),
            })
    }
}

impl Serialize for TranscriptDomain {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TranscriptDomain {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        TranscriptDomain::from_str(&text).map_err(DeError::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_strings_reject_out_of_domain_values() {
        assert!(DirectoryId::parse("0123456789abcdef0123456789abcdef").is_ok());
        assert!(
            DirectoryId::parse("0123456789ABCDEF0123456789abcdef").is_err(),
            "必须小写十六进制"
        );
        assert!(DirectoryId::parse("0123456789abcdef").is_err());
        assert!(DirectoryId::parse("/abs/path").is_err(), "标识不是路径");
        assert!(DirectoryId::parse(&"g".repeat(32)).is_err());

        assert!(OpaqueRef::parse("ceiling-1").is_ok());
        assert!(AgentId::parse("omp").is_ok());
    }

    #[test]
    fn limits_bounds_are_enforced() {
        let limits = |heartbeat: u64| {
            format!(
                "{{\"maxMessageBytes\":1048576,\"catalogSnapshotBatchSize\":64,\"resourceSnapshotBatchSize\":64,\"maxInFlightCommands\":16,\"maxPendingQueueBytes\":1048576,\"maxPendingQueueMessages\":256,\"heartbeatIntervalMs\":{heartbeat}}}"
            )
        };
        assert!(serde_json::from_str::<NodeLinkLimits>(&limits(30_000)).is_ok());
        assert!(serde_json::from_str::<NodeLinkLimits>(&limits(999)).is_err());
        assert!(serde_json::from_str::<NodeLinkLimits>(&limits(300_001)).is_err());

        let too_small =
            limits(30_000).replace("\"maxMessageBytes\":1048576", "\"maxMessageBytes\":1023");
        assert!(serde_json::from_str::<NodeLinkLimits>(&too_small).is_err());
    }

    #[test]
    fn transcript_domains_match_the_table() {
        let from_table: Vec<&str> = crate::domains::DOMAINS
            .iter()
            .map(|domain| domain.domain)
            .collect();
        let from_enum: Vec<&str> = TranscriptDomain::ALL
            .iter()
            .map(|domain| domain.as_str())
            .collect();
        assert_eq!(
            from_enum, from_table,
            "domain 枚举必须与 transcript 表逐条相等"
        );
        assert_eq!(TRANSCRIPT_CODEC, "acpr-transcript-v1");
    }
}
