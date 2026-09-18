//! 节点配对 HTTP payload（`schemas/node-link/v1/pairing.schema.json`、
//! `docs/NODE_LINK_PROTOCOL.md` §13）。
//!
//! 这里只做 wire 形状与取值域校验：二维码载荷、claim 请求/响应、状态查询请求/响应、HTTP 错误体。
//! 与 Sync 的配对载荷（`sync-protocol/src/pairing.rs`）差别只在字段与值域——Node Link 没有
//! `canonicalOrigin`/`deviceId`/`hostId`，对应位置是 `endpoint` 与 `ownerNodeId`/`accessNodeId`。
//!
//! HMAC/proof 的密码学验证、`pairingSecret` 的一次性消费、Node Trust 的持久化与「pending 的 claim 只能
//! 被 Owner 本机确认」都是状态机（§8.2/§13.3），不在本模块。

use std::fmt;
use std::str::FromStr;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::common::{
    Base64Url, GrantList, NodeName, ProtocolVersionV1, Timestamp, Uuid, ValueError,
    deserialize_optional_non_null,
};

/// `pairing.schema.json#/$defs/httpError`：与连接级 `link.error` body 同形状（§13.4 与 §12.6）。
///
/// 两处 schema 的字段集合、类型与必需性逐条相同（`code`/`message`/`retryable`/`correlationId`/
/// `details`，五个键都必需），因此不重复建模；HTTP 状态码到该 body 的映射由 `server::node_link` 负责。
pub type HttpError = crate::error::Body;

/// `pairing.schema.json#/$defs/endpoint`：Node Link v1 的 WSS 端点。
///
/// pattern 只允许 `wss://<authority>/node-link/v1`（authority 不含 `/`、`?`、`#`）；这里另外拒绝空白与
/// 控制字符、并把长度上限固定为 schema 的 `maxLength: 2048`，因为 `format: uri`（ajv-formats 与浏览器
/// URL 解析）同样不接受它们，而该 host 会被用来推导 HTTP origin（§13.1）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Endpoint(String);

/// `endpoint` 的 `maxLength`。
const MAX_ENDPOINT: usize = 2048;

impl Endpoint {
    pub fn parse(text: &str) -> Result<Self, ValueError> {
        const SCHEME: &str = "wss://";
        const PATH: &str = "/node-link/v1";

        let authority = text
            .strip_prefix(SCHEME)
            .and_then(|rest| rest.strip_suffix(PATH));
        let well_formed = text.chars().count() <= MAX_ENDPOINT
            && authority.is_some_and(|authority| {
                !authority.is_empty()
                    && !authority.contains(['/', '?', '#'])
                    && !authority
                        .chars()
                        .any(|character| character.is_whitespace() || character.is_control())
            });
        if !well_formed {
            return Err(ValueError::Shape {
                field: "endpoint",
                expected: "wss://<authority>/node-link/v1（≤2048 字符；authority 非空且不含 / ? # 与空白）",
            });
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for Endpoint {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Endpoint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(DeError::custom)
    }
}

/// `qrPayload.pairingProtocol`：v1 只有 `acp-remote-nodelink-v1` 一个取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PairingProtocolV1;

impl PairingProtocolV1 {
    pub const VALUE: &'static str = "acp-remote-nodelink-v1";
}

impl Serialize for PairingProtocolV1 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(Self::VALUE)
    }
}

impl<'de> Deserialize<'de> for PairingProtocolV1 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text != Self::VALUE {
            return Err(DeError::custom(ValueError::Enumerated {
                field: "pairingProtocol",
                value: text,
            }));
        }
        Ok(Self)
    }
}

/// `claimRequest.nodeKind`：`common.schema.json#/$defs/nodeKind` 与 `const "access"` 的交集。
///
/// Node Link 配对只允许 Access Node 发起 claim（§13.2），所以这个位置不是 [`crate::common::NodeKind`]
/// 的完整取值域：`"owner"` 是合法 nodeKind 但不是合法的 claim 发起方，在这里就必须被拒。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AccessNodeKind;

impl AccessNodeKind {
    pub const VALUE: &'static str = "access";
}

impl Serialize for AccessNodeKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(Self::VALUE)
    }
}

impl<'de> Deserialize<'de> for AccessNodeKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text != Self::VALUE {
            return Err(DeError::custom(ValueError::Enumerated {
                field: "nodeKind",
                value: text,
            }));
        }
        Ok(Self)
    }
}

/// `claimResponse.status`：v1 只有 `pending_confirmation` 一个取值（§13.2：claim 之后必须由 Owner 本机
/// 确认，Access 只能轮询状态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PendingConfirmation;

impl PendingConfirmation {
    pub const VALUE: &'static str = "pending_confirmation";
}

impl Serialize for PendingConfirmation {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(Self::VALUE)
    }
}

impl<'de> Deserialize<'de> for PendingConfirmation {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text != Self::VALUE {
            return Err(DeError::custom(ValueError::Enumerated {
                field: "status",
                value: text,
            }));
        }
        Ok(Self)
    }
}

/// `statusResponse.status`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PairingStatus {
    /// `pending_confirmation`：claim 已登记，等待 Owner 本机确认。
    PendingConfirmation,
    /// `approved`：已确认，响应携带 `node` 与 `owner`。
    Approved,
    /// `rejected`：Owner 明确拒绝。
    Rejected,
    /// `expired`：超过 `expiresAt`。
    Expired,
    /// `consumed`：`pairingSecret` 已被使用（一次性）。
    Consumed,
}

impl PairingStatus {
    /// schema 的 enum 取值，顺序一致。
    pub const ALL: [PairingStatus; 5] = [
        PairingStatus::PendingConfirmation,
        PairingStatus::Approved,
        PairingStatus::Rejected,
        PairingStatus::Expired,
        PairingStatus::Consumed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PairingStatus::PendingConfirmation => "pending_confirmation",
            PairingStatus::Approved => "approved",
            PairingStatus::Rejected => "rejected",
            PairingStatus::Expired => "expired",
            PairingStatus::Consumed => "consumed",
        }
    }

    /// 只有 `approved` 的响应允许携带 `node` 与 `owner`（schema 的 `if/then/else`）。
    pub fn carries_node_and_owner(self) -> bool {
        self == PairingStatus::Approved
    }
}

impl fmt::Display for PairingStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for PairingStatus {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        PairingStatus::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "status",
                value: text.to_owned(),
            })
    }
}

impl Serialize for PairingStatus {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PairingStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        PairingStatus::from_str(&text).map_err(DeError::custom)
    }
}

/// `pairing.schema.json#/$defs/qrPayload`：二维码里编码的配对邀请。
///
/// `ownerPublicKey` 是 P-256 的 SEC1 未压缩点（65 字节），`pairingSecret` 是 32 字节一次性共享密钥。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QrPayload {
    #[serde(rename = "pairingProtocol")]
    pub pairing_protocol: PairingProtocolV1,
    #[serde(rename = "ownerNodeId")]
    pub owner_node_id: Uuid,
    #[serde(rename = "ownerPublicKey")]
    pub owner_public_key: Base64Url<65>,
    pub endpoint: Endpoint,
    #[serde(rename = "pairingId")]
    pub pairing_id: Uuid,
    #[serde(rename = "pairingSecret")]
    pub pairing_secret: Base64Url<32>,
    #[serde(rename = "expiresAt")]
    pub expires_at: Timestamp,
}

/// `pairing.schema.json#/$defs/claimRequest`：Access Node 的 claim 请求（`POST /node-link/v1/pairing/claim`）。
///
/// `proof` 是 `HMAC-SHA256(pairingSecret, node-link-pairing-proof/v1 transcript)` 的无填充 base64url
/// 编码；密码学验证属 `identity-auth`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimRequest {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: ProtocolVersionV1,
    #[serde(rename = "pairingId")]
    pub pairing_id: Uuid,
    #[serde(rename = "ownerNodeId")]
    pub owner_node_id: Uuid,
    #[serde(rename = "accessNodeId")]
    pub access_node_id: Uuid,
    #[serde(rename = "nodeName")]
    pub node_name: NodeName,
    #[serde(rename = "nodeKind")]
    pub node_kind: AccessNodeKind,
    pub endpoint: Endpoint,
    #[serde(rename = "accessPublicKey")]
    pub access_public_key: Base64Url<65>,
    #[serde(rename = "clientNonce")]
    pub client_nonce: Base64Url<32>,
    pub proof: Base64Url<32>,
}

/// `pairing.schema.json#/$defs/claimResponse`：claim 的受理结果，永远是 `pending_confirmation`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimResponse {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: ProtocolVersionV1,
    #[serde(rename = "pairingRequestId")]
    pub pairing_request_id: Uuid,
    #[serde(rename = "serverNonce")]
    pub server_nonce: Base64Url<32>,
    #[serde(rename = "ownerProof")]
    pub owner_proof: Base64Url<64>,
    pub status: PendingConfirmation,
    #[serde(rename = "expiresAt")]
    pub expires_at: Timestamp,
}

/// `pairing.schema.json#/$defs/statusRequest`：Access 轮询自己的 claim 状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusRequest {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: ProtocolVersionV1,
    #[serde(rename = "ownerNodeId")]
    pub owner_node_id: Uuid,
    #[serde(rename = "accessNodeId")]
    pub access_node_id: Uuid,
    #[serde(rename = "pairingId")]
    pub pairing_id: Uuid,
    #[serde(rename = "pairingRequestId")]
    pub pairing_request_id: Uuid,
    #[serde(rename = "requestNonce")]
    pub request_nonce: Base64Url<32>,
    pub proof: Base64Url<32>,
}

/// `statusResponse.node`：只在 `status = "approved"` 时出现，给出被批准的 Access Node 与授予的 grants。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeBlock {
    #[serde(rename = "accessNodeId")]
    pub access_node_id: Uuid,
    pub name: NodeName,
    pub scopes: GrantList,
}

/// `statusResponse.owner`：只在 `status = "approved"` 时出现，给出 Owner 身份与公开密钥。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerBlock {
    #[serde(rename = "ownerNodeId")]
    pub owner_node_id: Uuid,
    #[serde(rename = "ownerPublicKey")]
    pub owner_public_key: Base64Url<65>,
}

/// `pairing.schema.json#/$defs/statusResponse`。
///
/// `node` 与 `owner` 只在 `status = "approved"` 时出现（schema 的 `if/then/else`），因此构造与反序列化
/// 都经 [`RawStatusResponse`] 做一次跨字段校验：approved 必须两者都有，其余状态必须都没有。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusResponse {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: ProtocolVersionV1,
    #[serde(rename = "pairingRequestId")]
    pub pairing_request_id: Uuid,
    pub status: PairingStatus,
    #[serde(rename = "expiresAt")]
    pub expires_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<OwnerBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStatusResponse {
    #[serde(rename = "protocolVersion")]
    protocol_version: ProtocolVersionV1,
    #[serde(rename = "pairingRequestId")]
    pairing_request_id: Uuid,
    status: PairingStatus,
    #[serde(rename = "expiresAt")]
    expires_at: Timestamp,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    node: Option<NodeBlock>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    owner: Option<OwnerBlock>,
}

impl TryFrom<RawStatusResponse> for StatusResponse {
    type Error = ValueError;

    fn try_from(raw: RawStatusResponse) -> Result<Self, Self::Error> {
        let carries = raw.status.carries_node_and_owner();
        if carries != raw.node.is_some() || carries != raw.owner.is_some() {
            return Err(ValueError::Shape {
                field: "node",
                expected: "status = \"approved\" 时必须给出 node 与 owner，其余状态必须都不给出",
            });
        }
        Ok(Self {
            protocol_version: raw.protocol_version,
            pairing_request_id: raw.pairing_request_id,
            status: raw.status,
            expires_at: raw.expires_at,
            node: raw.node,
            owner: raw.owner,
        })
    }
}

impl<'de> Deserialize<'de> for StatusResponse {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawStatusResponse::deserialize(deserializer)?;
        Self::try_from(raw).map_err(DeError::custom)
    }
}
