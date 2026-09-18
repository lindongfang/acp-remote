//! `auth.*` 家族 body（`schemas/sync/v1/auth.schema.json`）：握手的四个消息。
//!
//! 这里只做 wire 形状与取值域校验；证明的密码学验证（P-256/HMAC）属于 `identity-auth`，
//! 本模块不生成、不校验任何密钥材料。

use std::fmt;
use std::str::FromStr;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::common::{
    Base64Url, BoundedU64, DecimalString, FeatureList, NonEmptyText, UIntAtLeast, Uuid, ValueError,
};

/// 客户端形态（`clientKind`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientKind {
    Pwa,
    Android,
    Ios,
    Desktop,
}

impl ClientKind {
    pub const ALL: [ClientKind; 4] = [
        ClientKind::Pwa,
        ClientKind::Android,
        ClientKind::Ios,
        ClientKind::Desktop,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ClientKind::Pwa => "pwa",
            ClientKind::Android => "android",
            ClientKind::Ios => "ios",
            ClientKind::Desktop => "desktop",
        }
    }
}

impl fmt::Display for ClientKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ClientKind {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        ClientKind::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "clientKind",
                value: text.to_owned(),
            })
    }
}

impl Serialize for ClientKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ClientKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        ClientKind::from_str(&text).map_err(DeError::custom)
    }
}

/// `auth.client_hello` 的 body。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientHello {
    #[serde(rename = "minProtocolVersion")]
    pub min_protocol_version: UIntAtLeast<1>,
    #[serde(rename = "maxProtocolVersion")]
    pub max_protocol_version: UIntAtLeast<1>,
    #[serde(rename = "hostId")]
    pub host_id: Uuid,
    #[serde(rename = "deviceId")]
    pub device_id: Uuid,
    #[serde(rename = "clientKind")]
    pub client_kind: ClientKind,
    #[serde(rename = "clientNonce")]
    pub client_nonce: Base64Url<32>,
    #[serde(rename = "supportedFeatures")]
    pub supported_features: FeatureList,
    #[serde(rename = "requiredFeatures")]
    pub required_features: FeatureList,
}

/// `auth.server_challenge` 的 body。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerChallenge {
    #[serde(rename = "selectedProtocolVersion")]
    pub selected_protocol_version: UIntAtLeast<1>,
    #[serde(rename = "hostId")]
    pub host_id: Uuid,
    #[serde(rename = "connectionId")]
    pub connection_id: Uuid,
    #[serde(rename = "serverNonce")]
    pub server_nonce: Base64Url<32>,
    #[serde(rename = "selectedFeatures")]
    pub selected_features: FeatureList,
    #[serde(rename = "hostProof")]
    pub host_proof: Base64Url<64>,
}

/// `auth.client_proof` 的 body。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientProof {
    #[serde(rename = "connectionId")]
    pub connection_id: Uuid,
    #[serde(rename = "deviceId")]
    pub device_id: Uuid,
    #[serde(rename = "deviceProof")]
    pub device_proof: Base64Url<64>,
}

/// `scopes`：非空、去重的 scope 名列表。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scopes(Vec<NonEmptyText<128>>);

impl Scopes {
    pub fn as_slice(&self) -> &[NonEmptyText<128>] {
        &self.0
    }

    fn validate(&self) -> Result<(), ValueError> {
        for (index, scope) in self.0.iter().enumerate() {
            if self.0[index + 1..]
                .iter()
                .any(|other| other.as_str() == scope.as_str())
            {
                return Err(ValueError::RepeatedItem {
                    field: "scopes",
                    value: scope.as_str().to_owned(),
                });
            }
        }
        Ok(())
    }
}

impl Serialize for Scopes {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Scopes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let scopes = Self(Vec::<NonEmptyText<128>>::deserialize(deserializer)?);
        scopes.validate().map_err(DeError::custom)?;
        Ok(scopes)
    }
}

/// `auth.authenticated` 的 `limits`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    #[serde(rename = "maxMessageBytes")]
    pub max_message_bytes: UIntAtLeast<1024>,
    #[serde(rename = "maxPromptBytes")]
    pub max_prompt_bytes: UIntAtLeast<1>,
    #[serde(rename = "maxReplayEventsPerBatch")]
    pub max_replay_events_per_batch: UIntAtLeast<1>,
}

/// `auth.authenticated` 的 body。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Authenticated {
    #[serde(rename = "deviceId")]
    pub device_id: Uuid,
    pub scopes: Scopes,
    #[serde(rename = "serverEpoch")]
    pub server_epoch: Uuid,
    #[serde(rename = "headGlobalSequence")]
    pub head_global_sequence: DecimalString,
    #[serde(rename = "heartbeatIntervalMs")]
    pub heartbeat_interval_ms: BoundedU64<1000, 300000>,
    pub limits: Limits,
}
