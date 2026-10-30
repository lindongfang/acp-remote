//! Permission observations, immutable decisions and transport facts are separate records.
use super::{
    ActorContext, EntityId,
    history::{MAX_VERSION, NativeToolCallId, OutputScope, RawContent, ToolReference},
};
use std::{fmt, sync::Arc};
pub const MAX_APPROVAL_BYTES: usize = 16 * 1024 * 1024;
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ApprovalId(pub EntityId);
impl ApprovalId {
    #[must_use]
    pub fn encode(self) -> String {
        self.0.encode()
    }
}
pub use super::history::{ConnectionEpoch, InstanceId, NativeSessionId};
#[derive(Clone, Eq, PartialEq)]
pub struct NativeRequestId(Arc<str>);
impl NativeRequestId {
    /// # Errors
    /// Rejects identities beyond the existing bounded content-object size.
    pub fn string(value: &str) -> Result<Self, ApprovalError> {
        if value.len() > MAX_APPROVAL_BYTES {
            return Err(ApprovalError::ContentTooLarge);
        }
        let json = serde_json::to_string(value).map_err(|_| ApprovalError::InvalidSource)?;
        if json.len() > MAX_APPROVAL_BYTES {
            return Err(ApprovalError::ContentTooLarge);
        }
        Ok(Self(Arc::from(json)))
    }
    /// # Errors
    /// Rejects numeric request identifiers that cannot round-trip safely.
    pub fn integer(value: i64) -> Result<Self, ApprovalError> {
        if value.unsigned_abs() > MAX_VERSION {
            return Err(ApprovalError::InvalidSource);
        }
        Ok(Self(Arc::from(value.to_string())))
    }
    /// # Errors
    /// Accepts only the supported JSON-RPC string/integer identifier types.
    pub fn from_json(value: &str) -> Result<Self, ApprovalError> {
        match serde_json::from_str::<serde_json::Value>(value)
            .map_err(|_| ApprovalError::InvalidSource)?
        {
            serde_json::Value::String(value) => Self::string(&value),
            serde_json::Value::Number(value) => value
                .as_i64()
                .ok_or(ApprovalError::InvalidSource)
                .and_then(Self::integer),
            _ => Err(ApprovalError::InvalidSource),
        }
    }
    #[must_use]
    pub fn as_json(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for NativeRequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeRequestId")
            .field("encoded_bytes", &self.0.len())
            .finish()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalBinding {
    pub scope: OutputScope,
    pub connection: ConnectionEpoch,
    pub native_session: NativeSessionId,
    pub native_request: NativeRequestId,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PermissionKind {
    ToolPermission,
    Unsupported(Arc<str>),
}
impl PermissionKind {
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::ToolPermission => "tool_permission",
            Self::Unsupported(value) => value,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PermissionOptionKind {
    AllowOnce,
    AllowAlways,
    RejectOnce,
    RejectAlways,
    Unsupported(Arc<str>),
}
impl PermissionOptionKind {
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::AllowOnce => "allow_once",
            Self::AllowAlways => "allow_always",
            Self::RejectOnce => "reject_once",
            Self::RejectAlways => "reject_always",
            Self::Unsupported(value) => value,
        }
    }
    #[must_use]
    pub fn from_label(value: &str) -> Self {
        match value {
            "allow_once" => Self::AllowOnce,
            "allow_always" => Self::AllowAlways,
            "reject_once" => Self::RejectOnce,
            "reject_always" => Self::RejectAlways,
            _ => Self::Unsupported(Arc::from(value)),
        }
    }
    #[must_use]
    pub const fn supported(&self) -> bool {
        !matches!(self, Self::Unsupported(_))
    }
}
#[derive(Clone)]
pub struct NativePermissionOption {
    pub native_id: Arc<str>,
    pub name: Arc<str>,
    pub kind: PermissionOptionKind,
}
impl fmt::Debug for NativePermissionOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativePermissionOption")
            .field("id_bytes", &self.native_id.len())
            .field("name_bytes", &self.name.len())
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Debug)]
pub struct ApprovalObservation {
    pub binding: ApprovalBinding,
    pub kind: PermissionKind,
    pub tool_call_id: Option<NativeToolCallId>,
    pub options: Vec<NativePermissionOption>,
    pub scope_understood: bool,
    pub scope_raw: RawContent,
    pub raw_request: RawContent,
    pub protocol: Arc<str>,
    pub protocol_version: Arc<str>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalRestriction {
    UnknownRequest,
    UnknownOption,
    UnknownScope,
    UnresolvedTool,
    AmbiguousOptions,
}
impl ApprovalRestriction {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::UnknownRequest => "unknown_request",
            Self::UnknownOption => "unknown_option",
            Self::UnknownScope => "unknown_scope",
            Self::UnresolvedTool => "unresolved_tool",
            Self::AmbiguousOptions => "ambiguous_options",
        }
    }
    #[must_use]
    pub fn from_code(value: &str) -> Option<Self> {
        match value {
            "unknown_request" => Some(Self::UnknownRequest),
            "unknown_option" => Some(Self::UnknownOption),
            "unknown_scope" => Some(Self::UnknownScope),
            "unresolved_tool" => Some(Self::UnresolvedTool),
            "ambiguous_options" => Some(Self::AmbiguousOptions),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalState {
    Pending,
    Decided,
    Unsupported,
    Invalid,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryState {
    NotSent,
    Claimed,
    TransportWritten,
    Unknown,
    Suppressed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunLifecycle {
    CancelRequested,
    Completed,
    Cancelled,
    Failed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidationReason {
    Cancelled,
    RunCompleted,
    RunFailed,
    ConnectionClosed,
    ConnectionReplaced,
    Restarted,
}
impl InvalidationReason {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::RunCompleted => "run_completed",
            Self::RunFailed => "run_failed",
            Self::ConnectionClosed => "connection_closed",
            Self::ConnectionReplaced => "connection_replaced",
            Self::Restarted => "restarted",
        }
    }
}
#[derive(Clone, Debug)]
pub struct ApprovalChoice {
    pub choice_id: EntityId,
    pub option: NativePermissionOption,
}
#[derive(Clone, Debug)]
pub struct ApprovalView {
    pub id: ApprovalId,
    pub binding: ApprovalBinding,
    pub state: ApprovalState,
    pub restriction: Option<ApprovalRestriction>,
    pub tool: Option<ToolReference>,
    pub choices: Vec<ApprovalChoice>,
    pub scope_raw: RawContent,
    pub raw_request: RawContent,
    pub protocol: Arc<str>,
    pub protocol_version: Arc<str>,
    pub delivery_claim: Option<EntityId>,
    pub decision_command: Option<EntityId>,
    pub selected_choice: Option<EntityId>,
    pub delivery: DeliveryState,
}
#[derive(Clone, Copy, Debug)]
pub struct DecisionInput {
    pub session: super::history::SessionId,
    pub approval: ApprovalId,
    pub request_id: EntityId,
    pub choice: EntityId,
}
#[derive(Clone, Debug)]
pub struct DecisionRecord {
    pub command: EntityId,
    pub approval: ApprovalId,
    pub actor: EntityId,
    pub choice: EntityId,
    pub binding: ApprovalBinding,
    pub delivery: DeliveryState,
    pub replayed: bool,
}
#[derive(Clone, Debug)]
pub enum DecisionProbe {
    Replay(DecisionRecord),
    Fresh(ApprovalView),
}
/// A one-time database dispatch claim. WP3 must still verify its real issuer, actor gate and source fence.
pub struct ApprovalDeliveryClaim {
    pub(crate) token: EntityId,
    pub(crate) approval: ApprovalId,
    pub(crate) command: EntityId,
    pub(crate) actor: ActorContext,
    pub(crate) binding: ApprovalBinding,
    pub(crate) native_option: Arc<str>,
}
impl fmt::Debug for ApprovalDeliveryClaim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApprovalDeliveryClaim")
            .field("token", &self.token)
            .field("approval", &self.approval)
            .finish_non_exhaustive()
    }
}
impl ApprovalDeliveryClaim {
    #[must_use]
    pub const fn token(&self) -> EntityId {
        self.token
    }
    #[must_use]
    pub const fn approval(&self) -> ApprovalId {
        self.approval
    }
    #[must_use]
    pub const fn binding(&self) -> &ApprovalBinding {
        &self.binding
    }
    #[must_use]
    pub fn native_option(&self) -> &str {
        &self.native_option
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportReport {
    WriteCompleted,
    Unknown,
}
impl TransportReport {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::WriteCompleted => "write_completed",
            Self::Unknown => "unknown",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransportRecord {
    pub state: DeliveryState,
    pub reported: TransportReport,
    pub source_current_at_report: bool,
}
#[derive(Debug, thiserror::Error)]
pub enum ApprovalError {
    #[error("approval actor is not current")]
    InvalidActor,
    #[error("approval source is not current")]
    InvalidSource,
    #[error("approval request is invalid or expired")]
    InvalidApproval,
    #[error("approval request is not supported")]
    UnsupportedRequest,
    #[error("approval option is not supported or not advertised")]
    UnsupportedOption,
    #[error("approval already has a final decision")]
    AlreadyDecided,
    #[error("approval request identifier conflicts with prior content")]
    RequestConflict,
    #[error("approval native request identity conflicts with prior content")]
    SourceConflict,
    #[error("approval command capacity is exhausted")]
    CapacityExhausted,
    #[error("approval request exceeds the content-object budget")]
    ContentTooLarge,
    #[error("approval storage is unavailable")]
    StorageUnavailable,
    #[error("approval stored state is invalid")]
    CorruptState,
    #[error("approval requires a successful forced history flush")]
    HistoryUnavailable,
    #[error("approval delivery has already been claimed")]
    DeliveryAlreadyClaimed,
    #[error("approval transport report conflicts with a prior report")]
    ReportConflict,
    #[error("approval identifier generation is unavailable")]
    RandomUnavailable,
}

impl From<super::history::HistoryError> for ApprovalError {
    fn from(_: super::history::HistoryError) -> Self {
        Self::InvalidSource
    }
}
