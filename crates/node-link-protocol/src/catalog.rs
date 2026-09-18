//! Catalog 家族的消息 body（`schemas/node-link/v1/catalog.schema.json`，`docs/NODE_LINK_PROTOCOL.md` §12.3）。
//!
//! 信封（`postAuthBase` 的 `connectionId`/`connectionSequence`、`type` 与版本校验）在 `envelope` 层；
//! 这里只有七个消息的 `body`，以及它们引用的 `common.schema.json#/$defs` 值对象。

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::common::{
    Base64Url, DecimalString, ExportEntry, ExportId, Nullable, Text, Timestamp, Uuid, ValueError,
};

/// schema 对 `catalog.snapshot.exports`、`catalog.changed.added`/`updated`/`removed` 的 `maxItems`。
const MAX_BATCH_ITEMS: usize = 500;

/// Export 条目批次（schema 的 `maxItems: 500`）。
///
/// `catalog.snapshot.exports`、`catalog.changed.added`/`updated` 三个字段逐条同形、同一上限，共用本
/// 类型；越限时 [`ValueError::TooManyItems`] 的字段名取其规范名 `exports`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportEntryList(Vec<ExportEntry>);

impl ExportEntryList {
    pub fn new(entries: Vec<ExportEntry>) -> Result<Self, ValueError> {
        let list = Self(entries);
        list.validate()?;
        Ok(list)
    }

    pub fn as_slice(&self) -> &[ExportEntry] {
        &self.0
    }

    fn validate(&self) -> Result<(), ValueError> {
        if self.0.len() > MAX_BATCH_ITEMS {
            return Err(ValueError::TooManyItems {
                field: "exports",
                max: MAX_BATCH_ITEMS,
                actual: self.0.len(),
            });
        }
        Ok(())
    }
}

impl Serialize for ExportEntryList {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ExportEntryList {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let list = Self(Vec::<ExportEntry>::deserialize(deserializer)?);
        list.validate().map_err(serde::de::Error::custom)?;
        Ok(list)
    }
}

/// `catalog.changed.removed`：被移除的 Export id（schema 的 `maxItems: 500` + `uniqueItems: true`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportIdList(Vec<ExportId>);

impl ExportIdList {
    pub fn new(export_ids: Vec<ExportId>) -> Result<Self, ValueError> {
        let list = Self(export_ids);
        list.validate()?;
        Ok(list)
    }

    pub fn as_slice(&self) -> &[ExportId] {
        &self.0
    }

    fn validate(&self) -> Result<(), ValueError> {
        if self.0.len() > MAX_BATCH_ITEMS {
            return Err(ValueError::TooManyItems {
                field: "removed",
                max: MAX_BATCH_ITEMS,
                actual: self.0.len(),
            });
        }
        for (index, export_id) in self.0.iter().enumerate() {
            if self.0[index + 1..].contains(export_id) {
                return Err(ValueError::RepeatedItem {
                    field: "removed",
                    value: export_id.as_str().to_owned(),
                });
            }
        }
        Ok(())
    }
}

impl Serialize for ExportIdList {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ExportIdList {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let list = Self(Vec::<ExportId>::deserialize(deserializer)?);
        list.validate().map_err(serde::de::Error::custom)?;
        Ok(list)
    }
}

/// `catalog.subscribe`（Access → Owner）：`null` 表示首次获取完整视图，非空表示请求增量。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogSubscribe {
    #[serde(rename = "knownRevision")]
    pub known_revision: Nullable<DecimalString>,
}

/// `catalog.snapshot`（Owner → Access）：当前可见 Export 的完整视图。
///
/// `exports` 超过连接 limits 时按 limits 分批分成多条消息，批次内条目顺序必须稳定（§12.3）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogSnapshot {
    pub revision: DecimalString,
    pub exports: ExportEntryList,
}

/// `catalog.changed`（Owner → Access，`post_mvp`）：增量更新，Access 按 `revision` 去重。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogChanged {
    pub revision: DecimalString,
    pub added: ExportEntryList,
    pub updated: ExportEntryList,
    pub removed: ExportIdList,
}

/// `export.revoked`（Owner → Access）：立即生效，拒绝该 Export 的新命令与订阅。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportRevoked {
    #[serde(rename = "exportId")]
    pub export_id: ExportId,
    #[serde(rename = "revokedAt")]
    pub revoked_at: Timestamp,
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
