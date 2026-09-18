//! `error` 消息的 body 与 v1 错误码。
//!
//! 错误码的唯一机器来源是 `compatibility/errors/v1/errors.json` 的 `protocols.sync.errors`
//! （`schemas/sync/v1/common.schema.json` 的 `errorCode.enum` 与它逐条相等，由
//! `scripts/check-error-registry.mjs` 保证）。本枚举与它逐条相等，由 `tests/schema_drift.rs` 断言。

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::common::{Nullable, RawObject, Text, Uuid, ValueError};

/// Sync v1 的错误码（34 个，顺序即 `docs/SYNC_PROTOCOL.md` §12.2 与 registry 的顺序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    /// `protocol.invalid_json`
    ProtocolInvalidJson,
    /// `protocol.schema_invalid`
    ProtocolSchemaInvalid,
    /// `protocol.message_too_large`
    ProtocolMessageTooLarge,
    /// `protocol.version_unsupported`
    ProtocolVersionUnsupported,
    /// `protocol.feature_required`
    ProtocolFeatureRequired,
    /// `protocol.type_unsupported`
    ProtocolTypeUnsupported,
    /// `protocol.sequence_invalid`
    ProtocolSequenceInvalid,
    /// `auth.required`
    AuthRequired,
    /// `auth.host_mismatch`
    AuthHostMismatch,
    /// `auth.device_unknown`
    AuthDeviceUnknown,
    /// `auth.device_revoked`
    AuthDeviceRevoked,
    /// `auth.origin_mismatch`
    AuthOriginMismatch,
    /// `auth.proof_invalid`
    AuthProofInvalid,
    /// `authorization.scope_denied`
    AuthorizationScopeDenied,
    /// `pairing.already_claimed`
    PairingAlreadyClaimed,
    /// `pairing.expired`
    PairingExpired,
    /// `pairing.consumed`
    PairingConsumed,
    /// `sync.cursor_invalid`
    SyncCursorInvalid,
    /// `command.unsupported`
    CommandUnsupported,
    /// `command.not_found`
    CommandNotFound,
    /// `command.idempotency_conflict`
    CommandIdempotencyConflict,
    /// `command.uncertain`
    CommandUncertain,
    /// `state.version_conflict`
    StateVersionConflict,
    /// `session.not_found`
    SessionNotFound,
    /// `session.busy`
    SessionBusy,
    /// `interaction.already_resolved`
    InteractionAlreadyResolved,
    /// `capability.unsupported_by_client`
    CapabilityUnsupportedByClient,
    /// `capability.unsupported_by_broker`
    CapabilityUnsupportedByBroker,
    /// `capability.unsupported_by_agent`
    CapabilityUnsupportedByAgent,
    /// `resource.rate_limited`
    ResourceRateLimited,
    /// `resource.backpressure`
    ResourceBackpressure,
    /// `resource.result_too_large`
    ResourceResultTooLarge,
    /// `resource.remote_unavailable`
    ResourceRemoteUnavailable,
    /// `internal.unavailable`
    InternalUnavailable,
}

impl ErrorCode {
    /// 与 `compatibility/errors/v1/errors.json` 的 sync 列表逐条相等。
    pub const ALL: [ErrorCode; 34] = [
        ErrorCode::ProtocolInvalidJson,
        ErrorCode::ProtocolSchemaInvalid,
        ErrorCode::ProtocolMessageTooLarge,
        ErrorCode::ProtocolVersionUnsupported,
        ErrorCode::ProtocolFeatureRequired,
        ErrorCode::ProtocolTypeUnsupported,
        ErrorCode::ProtocolSequenceInvalid,
        ErrorCode::AuthRequired,
        ErrorCode::AuthHostMismatch,
        ErrorCode::AuthDeviceUnknown,
        ErrorCode::AuthDeviceRevoked,
        ErrorCode::AuthOriginMismatch,
        ErrorCode::AuthProofInvalid,
        ErrorCode::AuthorizationScopeDenied,
        ErrorCode::PairingAlreadyClaimed,
        ErrorCode::PairingExpired,
        ErrorCode::PairingConsumed,
        ErrorCode::SyncCursorInvalid,
        ErrorCode::CommandUnsupported,
        ErrorCode::CommandNotFound,
        ErrorCode::CommandIdempotencyConflict,
        ErrorCode::CommandUncertain,
        ErrorCode::StateVersionConflict,
        ErrorCode::SessionNotFound,
        ErrorCode::SessionBusy,
        ErrorCode::InteractionAlreadyResolved,
        ErrorCode::CapabilityUnsupportedByClient,
        ErrorCode::CapabilityUnsupportedByBroker,
        ErrorCode::CapabilityUnsupportedByAgent,
        ErrorCode::ResourceRateLimited,
        ErrorCode::ResourceBackpressure,
        ErrorCode::ResourceResultTooLarge,
        ErrorCode::ResourceRemoteUnavailable,
        ErrorCode::InternalUnavailable,
    ];

    /// registry 记录的该码默认 retryable 语义（compatibility/errors/v1/errors.json）。
    ///
    /// 发送方不得自行发明：`Body::new` 的调用者应当以它为准。registry 与这里的映射由
    /// `tests/schema_drift.rs` 逐对断言。
    pub fn default_retryable(self) -> bool {
        matches!(
            self,
            ErrorCode::SyncCursorInvalid
                | ErrorCode::StateVersionConflict
                | ErrorCode::SessionBusy
                | ErrorCode::ResourceRateLimited
                | ErrorCode::ResourceBackpressure
                | ErrorCode::ResourceRemoteUnavailable
                | ErrorCode::InternalUnavailable
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::ProtocolInvalidJson => "protocol.invalid_json",
            ErrorCode::ProtocolSchemaInvalid => "protocol.schema_invalid",
            ErrorCode::ProtocolMessageTooLarge => "protocol.message_too_large",
            ErrorCode::ProtocolVersionUnsupported => "protocol.version_unsupported",
            ErrorCode::ProtocolFeatureRequired => "protocol.feature_required",
            ErrorCode::ProtocolTypeUnsupported => "protocol.type_unsupported",
            ErrorCode::ProtocolSequenceInvalid => "protocol.sequence_invalid",
            ErrorCode::AuthRequired => "auth.required",
            ErrorCode::AuthHostMismatch => "auth.host_mismatch",
            ErrorCode::AuthDeviceUnknown => "auth.device_unknown",
            ErrorCode::AuthDeviceRevoked => "auth.device_revoked",
            ErrorCode::AuthOriginMismatch => "auth.origin_mismatch",
            ErrorCode::AuthProofInvalid => "auth.proof_invalid",
            ErrorCode::AuthorizationScopeDenied => "authorization.scope_denied",
            ErrorCode::PairingAlreadyClaimed => "pairing.already_claimed",
            ErrorCode::PairingExpired => "pairing.expired",
            ErrorCode::PairingConsumed => "pairing.consumed",
            ErrorCode::SyncCursorInvalid => "sync.cursor_invalid",
            ErrorCode::CommandUnsupported => "command.unsupported",
            ErrorCode::CommandNotFound => "command.not_found",
            ErrorCode::CommandIdempotencyConflict => "command.idempotency_conflict",
            ErrorCode::CommandUncertain => "command.uncertain",
            ErrorCode::StateVersionConflict => "state.version_conflict",
            ErrorCode::SessionNotFound => "session.not_found",
            ErrorCode::SessionBusy => "session.busy",
            ErrorCode::InteractionAlreadyResolved => "interaction.already_resolved",
            ErrorCode::CapabilityUnsupportedByClient => "capability.unsupported_by_client",
            ErrorCode::CapabilityUnsupportedByBroker => "capability.unsupported_by_broker",
            ErrorCode::CapabilityUnsupportedByAgent => "capability.unsupported_by_agent",
            ErrorCode::ResourceRateLimited => "resource.rate_limited",
            ErrorCode::ResourceBackpressure => "resource.backpressure",
            ErrorCode::ResourceResultTooLarge => "resource.result_too_large",
            ErrorCode::ResourceRemoteUnavailable => "resource.remote_unavailable",
            ErrorCode::InternalUnavailable => "internal.unavailable",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ErrorCode {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        ErrorCode::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::UnknownErrorCode(text.to_owned()))
    }
}

impl Serialize for ErrorCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ErrorCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        ErrorCode::from_str(&text).map_err(serde::de::Error::custom)
    }
}

/// `error` 消息的 body；认证前与认证后共用同一形状（`schemas/sync/v1/error.schema.json` 的
/// `errorBody`）。
///
/// `correlationId` 是 `required` 且可 `null`：[`Nullable`] 的 `Deserialize` 走
/// `deserialize_any`，缺键会得到 missing-field 错误，`null` 才是"明确没有关联请求"，
/// 因此这里不需要任何中间形态。`details` 是 [`RawObject`]，字节保真。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Body {
    pub code: ErrorCode,
    pub message: Text<1024>,
    pub retryable: bool,
    #[serde(rename = "correlationId")]
    pub correlation_id: Nullable<Uuid>,
    pub details: RawObject,
}

impl Body {
    /// 构造不带 correlationId、details 为 `{}` 的错误 body。
    pub fn new(code: ErrorCode, message: &str, retryable: bool) -> Result<Self, ValueError> {
        Ok(Self {
            code,
            message: Text::parse(message)?,
            retryable,
            correlation_id: Nullable::null(),
            details: RawObject::empty(),
        })
    }
}
