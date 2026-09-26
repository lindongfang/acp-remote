//! `handshake` 家族 body（`schemas/node-link/v1/handshake.schema.json`）：`node.hello`、
//! `node.challenge`、`node.proof`、`node.ready`（`docs/NODE_LINK_PROTOCOL.md` §12.2）。
//!
//! 这里只做 wire 形状与取值域校验；`nodeProof` 的密码学验证（P-256 的 P1363 签名，§9.4 的两个
//! 连接节点挑战 domain）属于 `identity-auth`，本模块不生成、不校验任何密钥材料。
//!
//! 四个 body 的字段与 `required` 逐项照 schema：没有可选字段，因而不使用 `Nullable`/
//! `deserialize_optional_non_null`；每个结构都关掉未知字段（§2.4 的 closed object 规则）。

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize};

use crate::common::{
    Base64Url, DecimalString, FeatureList, NodeKind, NodeLinkLimits, ProtocolVersionV1, Uuid,
    ValueError,
};

/// `node.hello.role` 在 schema 里是 `const "access"`。
///
/// [`NodeKind`] 是 owner/access 的共用词表，这里必须把 `owner` 挡在反序列化期：否则 Access
/// 侧可以自称 owner，绕过 §12.2「`role` 固定 `"access"` 以阻止角色混用」的约束。
fn deserialize_access_role<'de, D>(deserializer: D) -> Result<NodeKind, D::Error>
where
    D: Deserializer<'de>,
{
    let kind = NodeKind::deserialize(deserializer)?;
    if kind == NodeKind::Access {
        Ok(kind)
    } else {
        Err(DeError::custom(ValueError::Shape {
            field: "role",
            expected: "const \"access\"",
        }))
    }
}

/// `node.hello` 的 body：Access → Owner，认证后的第一条消息（§2.1：连接建立后第一条必须是它）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeHello {
    /// schema 里是 `const 1`：v1 只有版本 1，版本交集由 `maxProtocolVersion` 表达（§2.3）。
    #[serde(rename = "minProtocolVersion")]
    pub min_protocol_version: ProtocolVersionV1,
    #[serde(rename = "maxProtocolVersion")]
    pub max_protocol_version: ProtocolVersionV1,
    #[serde(rename = "accessNodeId")]
    pub access_node_id: Uuid,
    /// schema 固定为 `"access"`，见 [`deserialize_access_role`]。
    #[serde(rename = "role", deserialize_with = "deserialize_access_role")]
    pub role: NodeKind,
    #[serde(rename = "clientNonce")]
    pub client_nonce: Base64Url<32>,
    #[serde(rename = "supportedFeatures")]
    pub supported_features: FeatureList,
    /// §11：其中的必需 feature 未被 Owner 选中时，握手不得进入业务阶段。
    #[serde(rename = "requiredFeatures")]
    pub required_features: FeatureList,
}

/// `node.challenge` 的 body：Owner → Access。
///
/// 信封是认证前的（§2.2）：连接 ID 只在 body 里下发，与 Sync 的 `auth.server_challenge` 同一处理。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeChallenge {
    /// schema 里是 `const 1`；无法形成交集时 Owner 改为返回 `link.error`（§2.3）。
    #[serde(rename = "selectedProtocolVersion")]
    pub selected_protocol_version: ProtocolVersionV1,
    #[serde(rename = "connectionId")]
    pub connection_id: Uuid,
    #[serde(rename = "ownerNodeId")]
    pub owner_node_id: Uuid,
    #[serde(rename = "serverNonce")]
    pub server_nonce: Base64Url<32>,
    #[serde(rename = "selectedFeatures")]
    pub selected_features: FeatureList,
    /// Owner 当前目录修订号，与 `node.ready.catalogRevision` 同源。
    ///
    /// 两个连接 transcript domain（`node-link-challenge/v1`、`node-link-proof/v1`）的 tag 6 都取本字段：
    /// Access 必须能用它验证 `nodeProof` 并构造自己的 `node.proof`。字段缺失即握手无法完成，
    /// 因此 schema 与本文把它列为必需（2026-09-26 的 v1 内合同修订，design.md D13）。
    #[serde(rename = "catalogRevision")]
    pub catalog_revision: DecimalString,
    /// §9.4 连接节点挑战 domain 的 P1363 签名（64 字节）。
    #[serde(rename = "nodeProof")]
    pub node_proof: Base64Url<64>,
}

/// `node.proof` 的 body：Access → Owner，完成双向认证。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeProof {
    #[serde(rename = "connectionId")]
    pub connection_id: Uuid,
    #[serde(rename = "accessNodeId")]
    pub access_node_id: Uuid,
    /// §9.4 连接节点证明 domain 的 P1363 签名（64 字节）。
    #[serde(rename = "nodeProof")]
    pub node_proof: Base64Url<64>,
}

/// `node.ready` 的 body：Owner → Access，认证完成后的第一条消息。
///
/// 对应的信封已经必须携带 `connectionId`/`connectionSequence`（§12.2）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeReady {
    #[serde(rename = "ownerNodeId")]
    pub owner_node_id: Uuid,
    /// Access 侧用于 `catalog.subscribe` 的已知 revision（§12.3）。
    #[serde(rename = "catalogRevision")]
    pub catalog_revision: DecimalString,
    /// §2.5 的限额：只能把可下调项调低，不能上调。
    pub limits: NodeLinkLimits,
    /// 本次 Owner 事件保留窗口的 epoch，与 Sync 的 `serverEpoch` 同义。
    #[serde(rename = "serverEpoch")]
    pub server_epoch: Uuid,
}
