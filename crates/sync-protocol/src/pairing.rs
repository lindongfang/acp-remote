//! Pairing v1 的 HTTPS payload（`schemas/sync/v1/pairing.schema.json`，`docs/SYNC_PROTOCOL.md` §7）。
//!
//! 这里只做 wire 形状与取值域校验：claim/status 请求与响应、配对码 lookup 与扫码取回（qr-lookup）的
//! 请求与响应、HTTP 错误体；新设备短二维码自身的 `q2` 载荷与地址编解码在 [`crate::device_qr`]。签名与 HMAC 的密码学验证属于 `identity-auth`；「一个设备身份只绑定一个 canonical
//! origin」属于安全状态机，本模块只保证 `canonicalOrigin` 是合法的 origin 字面量、且不含路径、查询
//! 或片段。配对码的派生、归一化与单次消耗属于 `identity-auth`/服务端，本模块只校验它的 wire 字符集与
//! 长度（8 位 Crockford base32，可带一个 `-`）。

use std::fmt;
use std::str::FromStr;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::auth::ClientKind;
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
///
/// 设备方向没有授权维度（`pairing-model` 删除设备权限层）：设备块只承载非秘密元数据，
/// `schemas/sync/v1/pairing.schema.json` 的 `device.additionalProperties: false` 使携带
/// `scopes` 键的响应被拒（配对不携带任何授权集合）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceBlock {
    #[serde(rename = "deviceId")]
    pub device_id: Uuid,
    pub name: NonEmptyText<128>,
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
    #[serde(rename = "protocolVersion")]
    pub protocol_version: ProtocolVersionV1,
    #[serde(rename = "pairingRequestId")]
    pub pairing_request_id: Uuid,
    pub status: PairingStatus,
    #[serde(rename = "expiresAt")]
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

/// `lookupRequest.pairingCode`：8 位 Crockford base32（40 bit），可带一个 `-` 分隔符。
///
/// wire 上大小写不敏感，且允许 Crockford 的易混字符 `I`/`L`/`O`（服务端归一化为 `1`/`1`/`0`）；
/// `U` 不在 Crockford 字母表内。这里只校验字符集与长度，保留原始写法——归一化与「成功即消耗」
/// 属于服务端，DTO 不替它做决定。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PairingCode(String);

impl PairingCode {
    /// 去掉分隔符后的有效位数。
    pub const LENGTH: usize = 8;

    pub fn parse(text: &str) -> Result<Self, ValueError> {
        let compact: Vec<char> = text.chars().filter(|character| *character != '-').collect();
        let hyphens = text.chars().count() - compact.len();
        let well_formed = compact.len() == Self::LENGTH
            && hyphens <= 1
            && (hyphens == 0 || text.chars().nth(4) == Some('-'))
            && compact
                .iter()
                .all(|character| is_crockford_base32(*character));
        if !well_formed {
            // 不回显输入：配对码是取回 secret 的凭据，错误文本可能进入日志。
            return Err(ValueError::Shape {
                field: "pairingCode",
                expected: "8 位 Crockford base32（大小写不敏感，可带一个 4+4 的 `-` 分隔符，不含 U）",
            });
        }
        Ok(Self(text.to_owned()))
    }

    /// 原始 wire 写法（未归一化）。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Crockford base32 字母表（大小写不敏感）加上会被归一化的 `I`/`L`/`O`；`U` 被排除。
fn is_crockford_base32(character: char) -> bool {
    let uppercase = character.to_ascii_uppercase();
    uppercase.is_ascii_digit() || (uppercase.is_ascii_uppercase() && uppercase != 'U')
}

impl Serialize for PairingCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for PairingCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(DeError::custom)
    }
}

/// `pairing.schema.json#/$defs/lookupRequest`：`POST /sync/v1/pairing/lookup` 的请求体。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookupRequest {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: ProtocolVersionV1,
    #[serde(rename = "pairingCode")]
    pub pairing_code: PairingCode,
}

/// `pairing.schema.json#/$defs/lookupResponse`：lookup 成功返回的配对绑定与 secret。
///
/// 响应不含任何授权集合；`pairingSecret` 是完整 256 bit，后续 claim/status 与二维码入口完全一致。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookupResponse {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: ProtocolVersionV1,
    #[serde(rename = "pairingId")]
    pub pairing_id: Uuid,
    #[serde(rename = "hostId")]
    pub host_id: Uuid,
    #[serde(rename = "hostPublicKey")]
    pub host_public_key: Base64Url<65>,
    #[serde(rename = "canonicalOrigin")]
    pub canonical_origin: CanonicalOrigin,
    #[serde(rename = "pairingSecret")]
    pub pairing_secret: Base64Url<32>,
    #[serde(rename = "expiresAt")]
    pub expires_at: Timestamp,
}

/// `pairing.schema.json#/$defs/qrLookupRequest`：`POST /sync/v1/pairing/qr-lookup` 的请求体。
///
/// `qrToken` 是设备创建时独立生成的 32 字节扫码取回凭据（不是配对码、不是配对秘密）；
/// 请求不携带 Origin 字段——来源由 HTTP `Origin` 头与配对登记的 canonical origin 比较。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QrLookupRequest {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: ProtocolVersionV1,
    #[serde(rename = "qrToken")]
    pub qr_token: Base64Url<32>,
}

/// `pairing.schema.json#/$defs/qrLookupResponse`：扫码取回成功时的配对绑定。
///
/// 与配对码 lookup 的成功响应同形（七个字段），因为两者交付的是同一份配对资料；取回本身不推进
/// 状态、不消耗网页配对码、不创建信任。
pub type QrLookupResponse = LookupResponse;
