//! `link.error` / `link.ping` / `link.pong` / `link.backpressure` 的 body 与 v1 错误码
//! （`schemas/node-link/v1/error.schema.json`，`docs/NODE_LINK_PROTOCOL.md` §12.6/§14.1）。
//!
//! 错误码的唯一机器来源是 `compatibility/errors/v1/errors.json` 的 `protocols.node_link.errors`
//! （`schemas/node-link/v1/common.schema.json` 的 `errorCode.enum` 与它逐条相等，由
//! `scripts/check-error-registry.mjs` 保证；§14.1 的列表是同一份）。本枚举与它逐条相等。
//!
//! `link.error` 在认证前与认证后共用同一 body 形状（schema 的 `linkErrorBody`）；两种信封形状
//! （`preAuthBase` 的 4 键与 `postAuthBase` 的 6 键）由 `envelope` 层负责，不在本模块。

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::common::{Base64Url, Nullable, RawObject, Text, UIntAtLeast, Uuid, ValueError};

/// Node Link v1 的错误码（24 个，顺序即 registry 与 `docs/NODE_LINK_PROTOCOL.md` §14.1 的顺序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    /// `nodelink.protocol.invalid_json`
    ProtocolInvalidJson,
    /// `nodelink.protocol.schema_invalid`
    ProtocolSchemaInvalid,
    /// `nodelink.protocol.message_too_large`
    ProtocolMessageTooLarge,
    /// `nodelink.protocol.version_unsupported`
    ProtocolVersionUnsupported,
    /// `nodelink.protocol.feature_required`
    ProtocolFeatureRequired,
    /// `nodelink.protocol.type_unsupported`
    ProtocolTypeUnsupported,
    /// `nodelink.protocol.sequence_invalid`
    ProtocolSequenceInvalid,
    /// `nodelink.auth.proof_invalid`
    AuthProofInvalid,
    /// `nodelink.auth.node_unknown`
    AuthNodeUnknown,
    /// `nodelink.auth.node_revoked`
    AuthNodeRevoked,
    /// `nodelink.auth.catalog_revision_invalid`
    AuthCatalogRevisionInvalid,
    /// `nodelink.export.not_found`
    ExportNotFound,
    /// `nodelink.export.revoked`
    ExportRevoked,
    /// `nodelink.export.not_granted`
    ExportNotGranted,
    /// `nodelink.resource.attach_generation_stale`
    ResourceAttachGenerationStale,
    /// `nodelink.resource.snapshot_unavailable`
    ResourceSnapshotUnavailable,
    /// `nodelink.resource.owner_unavailable`
    ResourceOwnerUnavailable,
    /// `nodelink.resource.rate_limited`
    ResourceRateLimited,
    /// `nodelink.command.unsupported`
    CommandUnsupported,
    /// `nodelink.command.not_found`
    CommandNotFound,
    /// `nodelink.command.idempotency_conflict`
    CommandIdempotencyConflict,
    /// `nodelink.command.unsupported_field`
    CommandUnsupportedField,
    /// `nodelink.command.uncertain`
    CommandUncertain,
    /// `nodelink.internal.unavailable`
    InternalUnavailable,
}

impl ErrorCode {
    /// 与 `compatibility/errors/v1/errors.json` 的 node_link 列表逐条相等。
    pub const ALL: [ErrorCode; 24] = [
        ErrorCode::ProtocolInvalidJson,
        ErrorCode::ProtocolSchemaInvalid,
        ErrorCode::ProtocolMessageTooLarge,
        ErrorCode::ProtocolVersionUnsupported,
        ErrorCode::ProtocolFeatureRequired,
        ErrorCode::ProtocolTypeUnsupported,
        ErrorCode::ProtocolSequenceInvalid,
        ErrorCode::AuthProofInvalid,
        ErrorCode::AuthNodeUnknown,
        ErrorCode::AuthNodeRevoked,
        ErrorCode::AuthCatalogRevisionInvalid,
        ErrorCode::ExportNotFound,
        ErrorCode::ExportRevoked,
        ErrorCode::ExportNotGranted,
        ErrorCode::ResourceAttachGenerationStale,
        ErrorCode::ResourceSnapshotUnavailable,
        ErrorCode::ResourceOwnerUnavailable,
        ErrorCode::ResourceRateLimited,
        ErrorCode::CommandUnsupported,
        ErrorCode::CommandNotFound,
        ErrorCode::CommandIdempotencyConflict,
        ErrorCode::CommandUnsupportedField,
        ErrorCode::CommandUncertain,
        ErrorCode::InternalUnavailable,
    ];

    /// registry 记录的该码默认 retryable 语义（`compatibility/errors/v1/errors.json` 的
    /// `protocols.node_link.errors[].retryable`）：只有 §14.1 中「可重试」的 5 个码为 `true`。
    ///
    /// 发送方不得自行发明：`Body::new` 的调用者应当以它为准。
    pub fn default_retryable(self) -> bool {
        matches!(
            self,
            ErrorCode::ResourceAttachGenerationStale
                | ErrorCode::ResourceSnapshotUnavailable
                | ErrorCode::ResourceOwnerUnavailable
                | ErrorCode::ResourceRateLimited
                | ErrorCode::InternalUnavailable
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::ProtocolInvalidJson => "nodelink.protocol.invalid_json",
            ErrorCode::ProtocolSchemaInvalid => "nodelink.protocol.schema_invalid",
            ErrorCode::ProtocolMessageTooLarge => "nodelink.protocol.message_too_large",
            ErrorCode::ProtocolVersionUnsupported => "nodelink.protocol.version_unsupported",
            ErrorCode::ProtocolFeatureRequired => "nodelink.protocol.feature_required",
            ErrorCode::ProtocolTypeUnsupported => "nodelink.protocol.type_unsupported",
            ErrorCode::ProtocolSequenceInvalid => "nodelink.protocol.sequence_invalid",
            ErrorCode::AuthProofInvalid => "nodelink.auth.proof_invalid",
            ErrorCode::AuthNodeUnknown => "nodelink.auth.node_unknown",
            ErrorCode::AuthNodeRevoked => "nodelink.auth.node_revoked",
            ErrorCode::AuthCatalogRevisionInvalid => "nodelink.auth.catalog_revision_invalid",
            ErrorCode::ExportNotFound => "nodelink.export.not_found",
            ErrorCode::ExportRevoked => "nodelink.export.revoked",
            ErrorCode::ExportNotGranted => "nodelink.export.not_granted",
            ErrorCode::ResourceAttachGenerationStale => "nodelink.resource.attach_generation_stale",
            ErrorCode::ResourceSnapshotUnavailable => "nodelink.resource.snapshot_unavailable",
            ErrorCode::ResourceOwnerUnavailable => "nodelink.resource.owner_unavailable",
            ErrorCode::ResourceRateLimited => "nodelink.resource.rate_limited",
            ErrorCode::CommandUnsupported => "nodelink.command.unsupported",
            ErrorCode::CommandNotFound => "nodelink.command.not_found",
            ErrorCode::CommandIdempotencyConflict => "nodelink.command.idempotency_conflict",
            ErrorCode::CommandUnsupportedField => "nodelink.command.unsupported_field",
            ErrorCode::CommandUncertain => "nodelink.command.uncertain",
            ErrorCode::InternalUnavailable => "nodelink.internal.unavailable",
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

/// `common.schema.json#/$defs/publicError` 的 Node Link 具体化（`command.rejected.error` 等位置用它）。
///
/// `link.error` 的 body（[`Body`]）比它多一个 `correlationId`，因此是独立类型而不是它的别名。
pub type PublicError = acpr_wire::PublicError<ErrorCode>;

/// `link.error` 的 body（`error.schema.json` 的 `linkErrorBody`）。
///
/// `correlationId` 是 `required` 且可 `null`：[`Nullable`] 的 `Deserialize` 走 `deserialize_any`，
/// 缺键会得到 missing-field 错误，`null` 才是「本次错误没有可关联的请求」。`details` 是开放扩展点，
/// 按 [`RawObject`] 保留字节；§14.1 未登记 `details` 字段的 code 必须送 `{}`。
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
    /// 构造不带 `correlationId`、`details` 为 `{}` 的错误 body。
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

/// `link.ping` 的 body（`error.schema.json` 的 `linkPing`）：16 字节 nonce，`link.pong` 必须原样回填。
///
/// 与 Sync 的 `control.ping`/`control.pong` 同形（`SYNC_PROTOCOL.md` §2.5），但两个协议 crate 互不
/// 依赖（`AGENTS.md` §4），因此各持一份定义。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ping {
    pub nonce: Base64Url<16>,
}

/// `link.pong` 的 body（`error.schema.json` 的 `linkPong`）：与 [`Ping`] 逐字段同形，方向由对称的
/// `type` 区分。
pub type Pong = Ping;

/// `link.backpressure.body.scope`（`error.schema.json` 的 `linkBackpressure`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackpressureScope {
    /// `"resource"`
    Resource,
    /// `"connection"`
    Connection,
}

impl BackpressureScope {
    pub const ALL: [BackpressureScope; 2] =
        [BackpressureScope::Resource, BackpressureScope::Connection];

    pub fn as_str(self) -> &'static str {
        match self {
            BackpressureScope::Resource => "resource",
            BackpressureScope::Connection => "connection",
        }
    }
}

impl fmt::Display for BackpressureScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for BackpressureScope {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        BackpressureScope::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "scope",
                value: text.to_owned(),
            })
    }
}

impl Serialize for BackpressureScope {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for BackpressureScope {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        BackpressureScope::from_str(&text).map_err(serde::de::Error::custom)
    }
}

/// `link.backpressure` 的 body（`error.schema.json` 的 `linkBackpressure`）：提示对端暂停发送，
/// 达到高水位后仍可断开（`NODE_LINK_PROTOCOL.md` §12.6）。
///
/// `retryAfterMs` 是建议退避毫秒数（schema 的 `minimum: 0`，无上限）。形状与 Sync §14 的退避语义同形，
/// 但同样是本 crate 独立持有的定义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Backpressure {
    pub scope: BackpressureScope,
    #[serde(rename = "retryAfterMs")]
    pub retry_after_ms: UIntAtLeast<0>,
}
