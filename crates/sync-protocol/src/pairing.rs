//! Pairing v1 的 HTTPS payload（`schemas/sync/v1/pairing.schema.json`，`docs/SYNC_PROTOCOL.md` §7）。
//!
//! 这里只做 wire 形状与取值域校验：二维码载荷、claim/status 请求与响应、HTTP 错误体。签名与 HMAC 的
//! 密码学验证属于 `identity-auth`；「一个设备身份只绑定一个 canonical origin」属于安全状态机，本模块
//! 只保证 `canonicalOrigin` 是合法的 origin 字面量、且不含路径、查询或片段。

use std::fmt;
use std::str::FromStr;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::auth::{ClientKind, Scopes};
use crate::common::{
    Base64Url, NonEmptyText, ProtocolVersionV1, Timestamp, Uuid, ValueError,
    deserialize_optional_non_null,
};

/// `pairing.schema.json#/$defs/httpError`：与连接级 `error` body 同形状（§7 / §12）。
///
/// 两处 schema 的字段集合、类型与必需性逐条相同，因此不重复建模；HTTP 状态码到该 body 的映射由
/// `server::sync` 负责。
pub type HttpError = crate::error::Body;

/// `pairing.schema.json#/$defs/canonicalOrigin`：`https://<authority>`，不含路径、查询或片段。
///
/// pattern 只允许 `https://` 开头且其后不含 `/`、`?`、`#`；这里另外拒绝空白与控制字符，因为
/// `format: uri`（ajv-formats 与浏览器 URL 解析）同样不接受它们，而 canonical origin 会参与身份
/// 绑定，必须在 wire 边界就排除含歧义的写法。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CanonicalOrigin(String);

impl CanonicalOrigin {
    pub fn parse(text: &str) -> Result<Self, ValueError> {
        let authority = text.strip_prefix("https://");
        let well_formed = text.chars().count() <= 2048
            && authority.is_some_and(|authority| {
                !authority.is_empty()
                    && !authority.contains(['/', '?', '#'])
                    && !authority
                        .chars()
                        .any(|character| character.is_whitespace() || character.is_control())
            });
        if !well_formed {
            return Err(ValueError::CanonicalOrigin(text.to_owned()));
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for CanonicalOrigin {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CanonicalOrigin {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(DeError::custom)
    }
}

/// `pairingProtocol`：v1 只有 `acp-remote-pairing-v1` 一个取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PairingProtocolV1;

impl PairingProtocolV1 {
    pub const VALUE: &'static str = "acp-remote-pairing-v1";
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

/// `claimResponse.status`：v1 只有 `pending_confirmation` 一个取值。
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
    PendingConfirmation,
    Approved,
    Rejected,
    Expired,
    Consumed,
}

impl PairingStatus {
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

    /// 只有 `approved` 的响应允许携带 `device` 与 `host`（schema 的 `if/then/else`）。
    pub fn carries_device_and_host(self) -> bool {
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QrPayload {
    #[serde(rename = "pairingProtocol")]
    pub pairing_protocol: PairingProtocolV1,
    #[serde(rename = "hostId")]
    pub host_id: Uuid,
    #[serde(rename = "hostPublicKey")]
    pub host_public_key: Base64Url<65>,
    #[serde(rename = "canonicalOrigin")]
    pub canonical_origin: CanonicalOrigin,
    #[serde(rename = "pairingId")]
    pub pairing_id: Uuid,
    #[serde(rename = "pairingSecret")]
    pub pairing_secret: Base64Url<32>,
    #[serde(rename = "expiresAt")]
    pub expires_at: Timestamp,
}

/// `pairing.schema.json#/$defs/claimRequest`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimRequest {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: ProtocolVersionV1,
    #[serde(rename = "pairingId")]
    pub pairing_id: Uuid,
    #[serde(rename = "hostId")]
    pub host_id: Uuid,
    #[serde(rename = "deviceId")]
    pub device_id: Uuid,
    #[serde(rename = "deviceName")]
    pub device_name: NonEmptyText<128>,
    #[serde(rename = "clientKind")]
    pub client_kind: ClientKind,
    #[serde(rename = "canonicalOrigin")]
    pub canonical_origin: CanonicalOrigin,
    #[serde(rename = "devicePublicKey")]
    pub device_public_key: Base64Url<65>,
    #[serde(rename = "clientNonce")]
    pub client_nonce: Base64Url<32>,
    pub proof: Base64Url<32>,
}

/// `pairing.schema.json#/$defs/claimResponse`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimResponse {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: ProtocolVersionV1,
    #[serde(rename = "pairingRequestId")]
    pub pairing_request_id: Uuid,
    #[serde(rename = "serverNonce")]
    pub server_nonce: Base64Url<32>,
    #[serde(rename = "hostProof")]
    pub host_proof: Base64Url<64>,
    pub status: PendingConfirmation,
    #[serde(rename = "expiresAt")]
    pub expires_at: Timestamp,
}

/// `pairing.schema.json#/$defs/statusRequest`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusRequest {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: ProtocolVersionV1,
    #[serde(rename = "hostId")]
    pub host_id: Uuid,
    #[serde(rename = "deviceId")]
    pub device_id: Uuid,
    #[serde(rename = "pairingId")]
    pub pairing_id: Uuid,
    #[serde(rename = "pairingRequestId")]
    pub pairing_request_id: Uuid,
    #[serde(rename = "requestNonce")]
    pub request_nonce: Base64Url<32>,
    pub proof: Base64Url<32>,
}

/// `statusResponse.device`：仅在 `status = "approved"` 时出现。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceBlock {
    #[serde(rename = "deviceId")]
    pub device_id: Uuid,
    pub name: NonEmptyText<128>,
    pub scopes: Scopes,
}

/// `statusResponse.host`：仅在 `status = "approved"` 时出现。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostBlock {
    #[serde(rename = "hostId")]
    pub host_id: Uuid,
    #[serde(rename = "hostPublicKey")]
    pub host_public_key: Base64Url<65>,
}

/// `pairing.schema.json#/$defs/statusResponse`。
///
/// `device` 与 `host` 只在 `status = "approved"` 时出现（schema 的 `if/then/else`），因此构造与
/// 反序列化都经 [`RawStatusResponse`] 做一次跨字段校验：approved 必须两者都有，其余状态必须都没有。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusResponse {
    pub protocol_version: ProtocolVersionV1,
    pub pairing_request_id: Uuid,
    pub status: PairingStatus,
    pub expires_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<DeviceBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<HostBlock>,
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
    device: Option<DeviceBlock>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    host: Option<HostBlock>,
}

impl TryFrom<RawStatusResponse> for StatusResponse {
    type Error = ValueError;

    fn try_from(raw: RawStatusResponse) -> Result<Self, Self::Error> {
        let carries = raw.status.carries_device_and_host();
        if carries != raw.device.is_some() || carries != raw.host.is_some() {
            return Err(ValueError::Shape {
                field: "device",
                expected: "status = \"approved\" 时必须给出 device 与 host，其余状态必须都不给出",
            });
        }
        Ok(Self {
            protocol_version: raw.protocol_version,
            pairing_request_id: raw.pairing_request_id,
            status: raw.status,
            expires_at: raw.expires_at,
            device: raw.device,
            host: raw.host,
        })
    }
}

impl<'de> Deserialize<'de> for StatusResponse {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawStatusResponse::deserialize(deserializer)?;
        Self::try_from(raw).map_err(DeError::custom)
    }
}
