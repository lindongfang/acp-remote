//! Catalog 家族的消息 body（`schemas/node-link/v1/catalog.schema.json`，`docs/NODE_LINK_PROTOCOL.md` §12.3）。
//!
//! 信封（`postAuthBase` 的 `connectionId`/`connectionSequence`、`type` 与版本校验）在 `envelope` 层；
//! 这里只有 catalog 消息的 `body`，以及它们引用的 `common.schema.json#/$defs` 值对象。

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::common::{
    AgentEntry, AgentId, Base64Url, DecimalString, DirectoryEntry, DirectoryId, Nullable,
    SessionState, Text, Timestamp, Uuid, ValueError,
};

/// schema 对 catalog 各列表的 `maxItems`。
const MAX_BATCH_ITEMS: usize = 500;

/// `catalog.session`：会话的目录/Agent/状态投影，不含任何正文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogSession {
    #[serde(rename = "sessionId")]
    pub session_id: Uuid,
    #[serde(rename = "agentId")]
    pub agent_id: AgentId,
    pub state: SessionState,
    pub version: DecimalString,
    #[serde(rename = "createdAt")]
    pub created_at: Timestamp,
    #[serde(rename = "workspaceDirectoryId")]
    pub workspace_directory_id: Nullable<DirectoryId>,
}

macro_rules! list_type {
    ($(#[$meta:meta])* $name:ident, $item:ty, $field:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name(Vec<$item>);

        impl $name {
            pub fn new(items: Vec<$item>) -> Result<Self, ValueError> {
                let list = Self(items);
                if list.0.len() > MAX_BATCH_ITEMS {
                    return Err(ValueError::TooManyItems {
                        field: $field,
                        max: MAX_BATCH_ITEMS,
                        actual: list.0.len(),
                    });
                }
                Ok(list)
            }

            pub fn as_slice(&self) -> &[$item] {
                &self.0
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.0.serialize(serializer)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                Self::new(Vec::<$item>::deserialize(deserializer)?)
                    .map_err(serde::de::Error::custom)
            }
        }
    };
}

list_type!(
    /// `catalog.snapshot.agents`：本机配置声明的 Agent（schema 的 `maxItems: 500`）。
    AgentEntryList,
    AgentEntry,
    "agents"
);

list_type!(
    /// `catalog.snapshot.directories`：本机已登记的目录（schema 的 `maxItems: 500`）。
    DirectoryEntryList,
    DirectoryEntry,
    "directories"
);

list_type!(
    /// `catalog.snapshot.sessions`：本机会话投影（schema 的 `maxItems: 500`）。
    CatalogSessionList,
    CatalogSession,
    "sessions"
);

/// `catalog.subscribe`（Access → Owner）：`null` 表示首次获取完整视图，非空表示请求增量。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogSubscribe {
    #[serde(rename = "knownRevision")]
    pub known_revision: Nullable<DecimalString>,
}

/// `catalog.snapshot`（Owner → Access）：本机全部资源清单，不按 Export/授权集合过滤。
///
/// 三类列表超过连接 limits 时按 limits 分批分成多条消息，批次内条目顺序必须稳定（§12.3）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogSnapshot {
    pub revision: DecimalString,
    pub agents: AgentEntryList,
    pub directories: DirectoryEntryList,
    pub sessions: CatalogSessionList,
}

/// `node.trust.revoked`（Owner → Access）：撤回该 Access Node 的长期信任。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeTrustRevoked {
    #[serde(rename = "revokedNodeId")]
    pub revoked_node_id: Uuid,
    #[serde(rename = "revokedAt")]
    pub revoked_at: Timestamp,
    pub reason: Text<512>,
}

/// `node.rotate-key.request`（Access → Owner，`post_mvp`）：轮换请求按 `requestId` 幂等。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeRotateKeyRequest {
    #[serde(rename = "requestId")]
    pub request_id: Uuid,
    #[serde(rename = "newNodePublicKey")]
    pub new_node_public_key: Base64Url<65>,
    #[serde(rename = "requestNonce")]
    pub request_nonce: Base64Url<32>,
}

/// `node.rotate-key.result.body.status`（`catalog.schema.json` 的 `enum`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RotateKeyStatus {
    /// `"accepted"`：只有在新公钥已被 Owner 信任记录接受后才返回。
    Accepted,
    /// `"rejected"`
    Rejected,
}

impl RotateKeyStatus {
    pub const ALL: [RotateKeyStatus; 2] = [RotateKeyStatus::Accepted, RotateKeyStatus::Rejected];

    pub fn as_str(self) -> &'static str {
        match self {
            RotateKeyStatus::Accepted => "accepted",
            RotateKeyStatus::Rejected => "rejected",
        }
    }
}

impl fmt::Display for RotateKeyStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for RotateKeyStatus {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        RotateKeyStatus::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "status",
                value: text.to_owned(),
            })
    }
}

impl Serialize for RotateKeyStatus {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RotateKeyStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        RotateKeyStatus::from_str(&text).map_err(serde::de::Error::custom)
    }
}

/// `node.rotate-key.result`（Owner → Access，`post_mvp`）：`reason` 是 required 且可 `null`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeRotateKeyResult {
    #[serde(rename = "requestId")]
    pub request_id: Uuid,
    pub status: RotateKeyStatus,
    #[serde(rename = "newNodePublicKey")]
    pub new_node_public_key: Base64Url<65>,
    #[serde(rename = "rotatedAt")]
    pub rotated_at: Timestamp,
    pub reason: Nullable<Text<512>>,
}
