//! Body-free dispatch metadata. These values do not authenticate or authorize IO.
use super::{
    EntityId,
    native::{NativeSessionKey, ResourceEpoch, RuntimeEpoch},
    request_registry::RegistrationId,
};
use std::{fmt, sync::Arc};

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum NativeProtocolFailure {
    #[error("ACP NotInitialized")]
    NotInitialized,
    #[error("ACP AlreadyInitialized")]
    AlreadyInitialized,
    #[error("ACP InvalidMessage")]
    InvalidMessage,
    #[error("ACP IncompatibleVersion")]
    IncompatibleVersion,
    #[error("ACP UnsupportedCapability")]
    UnsupportedCapability,
    #[error("ACP Capacity")]
    Capacity,
    #[error("ACP UnmatchedResponse")]
    UnmatchedResponse,
    #[error("ACP DeliveryUnknown")]
    DeliveryUnknown,
    #[error("ACP Deadline")]
    Deadline,
    #[error("ACP Io")]
    Io,
    #[error("ACP InvalidOption")]
    InvalidOption,
    #[error("ACP MissingSnapshot")]
    MissingSnapshot,
    #[error("ACP SessionReadBusy")]
    SessionReadBusy,
    #[error("ACP RuntimeUnavailable")]
    RuntimeUnavailable,
    #[error("ACP Allocation")]
    Allocation,
    #[error("ACP InvalidFence")]
    InvalidFence,
    #[error("ACP InvalidFrame")]
    InvalidFrame,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum DispatchError {
    #[error("all native session capacity is occupied")]
    SessionCapacityFull,
    #[error("native main session tree cannot be verified")]
    SessionTreeUnverified,
    #[error("native protocol failed: {0}")]
    ProtocolFault(NativeProtocolFailure),
    #[error("native runtime is stopping")]
    NodeStopping,
    #[error("native runtime already exists")]
    RuntimeAlreadyStarted,
    #[error("necessary native storage is unavailable")]
    StorageUnavailable,
    #[error("native commit confirmation unavailable")]
    CommitUnconfirmed,
    #[error("native SQL connection close is unconfirmed (commit acknowledged: {commit_confirmed})")]
    SqlCloseUnconfirmed { commit_confirmed: bool },
    #[error("native metadata is corrupt")]
    CorruptState,
    #[error("native metadata conflicts")]
    Conflict,
    #[error("native target is invalid")]
    InvalidTarget,
    #[error("native source is invalid")]
    InvalidSource,
    #[error("native scope is invalid")]
    InvalidScope,
    #[error("native resource is invalid")]
    InvalidResource,
    #[error("native configuration is invalid or incomplete")]
    InvalidConfiguration,
    #[error("native prompt reservation is invalid")]
    InvalidPromptReservation,
    #[error("native frame length cannot be represented")]
    LengthUnrepresentable,
    #[error("native selection is stale")]
    StaleSelection,
    #[error("native content revision changed")]
    RevisionChanged,
    #[error("native target is not ready")]
    NotReady,
    #[error("necessary native facts are unavailable")]
    RequiredStateUnavailable,
    #[error("native local sequence exhausted")]
    SequenceExhausted,
    #[error("native protocol is unavailable")]
    ProtocolUnavailable,
    #[error("native protocol rejected the operation")]
    ProtocolRejected,
    #[error("native enqueue rejected the original objects")]
    EnqueueRejected,
    #[error("native observation is not next")]
    ObservationOrder,
    #[error("native observation was not applied")]
    ObservationUnapplied,
    #[error("native observation was dropped")]
    ObservationDropped,
    #[error("native resource convergence is unconfirmed")]
    ResourceUnconfirmed,
    #[error("native source operation {operation} failed (Windows {code})")]
    SourcePlatform { operation: &'static str, code: i32 },
    #[error("native source has stopped")]
    Stopped,
}
impl From<crate::domain::request_registry::RegistryError> for DispatchError {
    fn from(e: crate::domain::request_registry::RegistryError) -> Self {
        use crate::domain::request_registry::RegistryError as R;
        match e {
            R::CommitUnconfirmed => Self::CommitUnconfirmed,
            R::CorruptState => Self::CorruptState,
            R::Conflict => Self::Conflict,
            R::NodeStopping => Self::NodeStopping,
            R::RuntimeAlreadyStarted => Self::RuntimeAlreadyStarted,
            R::InvalidTarget => Self::InvalidTarget,
            _ => Self::StorageUnavailable,
        }
    }
}
#[derive(Clone, Eq, Hash, PartialEq)]
pub enum RpcScalar {
    Integer(i64),
    String(Arc<str>),
}
impl fmt::Debug for RpcScalar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RpcScalar(..)")
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolVersion {
    V1,
    V2,
}
impl ProtocolVersion {
    #[must_use]
    pub const fn number(self) -> i64 {
        match self {
            Self::V1 => 1,
            Self::V2 => 2,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportReason {
    Cancelled,
    Deadline,
    FenceRejected,
    ConnectionClosed,
    Abandoned,
    Io,
    InvalidFence,
    ConnectionLost,
    Restart,
}
impl TransportReason {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::Deadline => "deadline",
            Self::FenceRejected => "fence_rejected",
            Self::ConnectionClosed => "connection_closed",
            Self::Abandoned => "abandoned",
            Self::Io => "io",
            Self::InvalidFence => "invalid_fence",
            Self::ConnectionLost => "connection_lost",
            Self::Restart => "restart",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportFact {
    Zero {
        reason: TransportReason,
    },
    Unknown {
        accepted: u64,
        cause: TransportReason,
    },
    Written,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportProgress {
    Reserved,
    Queued,
    Writing { accepted: u64, total: u64 },
    Flushing { total: u64 },
    Finished(TransportFact),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContentOrigin {
    pub submit_request: RegistrationId,
    pub content_request: RegistrationId,
    pub revision: u64,
    pub digest: [u8; 32],
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionKind {
    Waiting,
    ExistingActive,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DispatchPlan {
    pub dispatch_id: EntityId,
    pub planned_run_id: EntityId,
    pub canonical_target: EntityId,
    pub native: NativeSessionKey,
    pub runtime: RuntimeEpoch,
    pub owner_id: EntityId,
    pub instance_id: EntityId,
    pub resource: ResourceEpoch,
    pub item_id: EntityId,
    pub revision: u64,
    pub origin: ContentOrigin,
    pub config_identity: EntityId,
    pub config_revision: u64,
    pub selection_kind: SelectionKind,
    pub prepared_at_ms: i64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeOwnerKind {
    WindowsJob,
    TestOwnedStream,
}
impl NativeOwnerKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::WindowsJob => "windows_job",
            Self::TestOwnedStream => "test_owned_stream",
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceMetadata {
    pub source_id: EntityId,
    pub instance_id: EntityId,
    pub runtime: RuntimeEpoch,
    pub connection_epoch: EntityId,
    pub config_identity: EntityId,
    pub config_revision: u64,
    pub owner_kind: NativeOwnerKind,
    pub created_at_ms: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceMetadata {
    pub source: SourceMetadata,
    pub target: EntityId,
    pub owner: EntityId,
    pub load_epoch: EntityId,
    pub reserved_at_ms: i64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnownStopReason {
    EndTurn,
    MaxTokens,
    MaxTurnRequests,
    Refusal,
    Cancelled,
    Error,
    Unknown,
}
impl KnownStopReason {
    #[must_use]
    pub fn classify(value: Option<&str>) -> Self {
        match value {
            Some("end_turn") => Self::EndTurn,
            Some("max_tokens") => Self::MaxTokens,
            Some("max_turn_requests") => Self::MaxTurnRequests,
            Some("refusal") => Self::Refusal,
            Some("cancelled") => Self::Cancelled,
            Some("error") => Self::Error,
            _ => Self::Unknown,
        }
    }
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::EndTurn => "end_turn",
            Self::MaxTokens => "max_tokens",
            Self::MaxTurnRequests => "max_turn_requests",
            Self::Refusal => "refusal",
            Self::Cancelled => "cancelled",
            Self::Error => "error",
            Self::Unknown => "unknown",
        }
    }
    #[must_use]
    pub const fn reliable(self) -> bool {
        !matches!(self, Self::Unknown)
    }
}
