//! Disposable presentation metadata. None of these values authorizes a read or native write.
use super::EntityId;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ViewError {
    #[error("native read failed: {0}")]
    NativeFailure(super::native_dispatch::DispatchError),
    #[error("view client is not authenticated")]
    Unauthenticated,
    #[error("view socket differs")]
    WrongSocket,
    #[error("view source is invalid")]
    InvalidSource,
    #[error("view configuration differs")]
    InvalidConfiguration,
    #[error("view operation is unsupported")]
    Unsupported,
    #[error("view capability is unsupported")]
    UnsupportedCapability,
    #[error("view owner is already attached")]
    AlreadyAttached,
    #[error("view capture is unavailable")]
    CaptureUnavailable,
    #[error("view operation stage differs")]
    InvalidStage,
    #[error("view source was replaced")]
    SourceReplaced,
    #[error("view stream is sealed")]
    StreamSealed,
    #[error("view protocol failed")]
    ProtocolFailure,
    #[error("view credit differs")]
    InvalidCredit,
    #[error("view queue rejected")]
    QueueRejected,
    #[error("view transport failed")]
    TransportFailure,
    #[error("view shutdown is unconfirmed")]
    ShutdownUnconfirmed,
    #[error("view client is invalid")]
    InvalidClient,
    #[error("view client was revoked")]
    Revoked,
    #[error("view target is not authorized")]
    InvalidTarget,
    #[error("view owner is stopping")]
    NodeStopping,
    #[error("view source differs")]
    InvalidScope,
    #[error("view source order differs")]
    CaptureGap,
    #[error("view allocation failed")]
    AllocationFailure,
    #[error("view random source is unavailable")]
    RandomUnavailable,
    #[error("view counter is exhausted")]
    SequenceExhausted,
    #[error("view cursor is stale")]
    StaleCursor,
    #[error("view record is unavailable")]
    InvalidCursor,
    #[error("view source was released")]
    SourceReleased,
    #[error("view projection failed")]
    ProjectionFailure,
    #[error("view state is unavailable")]
    ReadFailure,
    #[error("view size cannot be represented")]
    LengthUnrepresentable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewEpoch(EntityId);
impl ViewEpoch {
    /// Generates a new presentation generation, never a protocol identity.
    /// # Errors
    /// Returns `RandomUnavailable` when the operating system random source fails.
    pub fn fresh() -> Result<Self, ViewError> {
        crate::random::bytes()
            .map(EntityId::from_bytes)
            .map(Self)
            .map_err(|_| ViewError::RandomUnavailable)
    }
    #[must_use]
    pub const fn as_entity_id(self) -> EntityId {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadEpoch(EntityId);
impl ReadEpoch {
    /// Generates a read generation independently of view and resource identities.
    /// # Errors
    /// Returns `RandomUnavailable` if the operating system random source fails.
    pub fn fresh() -> Result<Self, ViewError> {
        crate::random::bytes()
            .map(EntityId::from_bytes)
            .map(Self)
            .map_err(|_| ViewError::RandomUnavailable)
    }
    #[must_use]
    pub const fn as_entity_id(self) -> EntityId {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawRecordId(pub(crate) u64);
impl RawRecordId {
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnchorId(pub(crate) usize);
impl AnchorId {
    #[must_use]
    pub const fn as_usize(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionState {
    Available,
    PartialKnownStructure,
    ProjectionUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadMode {
    CurrentOnly,
    ReplayRequested,
    Replaying,
    ReplayComplete,
    ReplayIncomplete,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Freshness {
    CurrentObservation,
    Revalidating,
    Stale,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JoinState {
    ProvenNativeIds,
    RebuiltNoStableIds,
    UnassociatedSegments,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderBasis {
    NativePosition,
    ConnectionObservation,
    Unknown,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewRevision(pub(crate) u64);
impl ViewRevision {
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewSourceStamp {
    pub(crate) binding: EntityId,
    pub(crate) source: super::native::NativeSourceId,
}
impl ViewSourceStamp {
    #[must_use]
    pub const fn binding_id(self) -> EntityId {
        self.binding
    }
    #[must_use]
    pub const fn source(self) -> super::native::NativeSourceId {
        self.source
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewDescriptor {
    pub(crate) epoch: ViewEpoch,
    pub(crate) read: Option<ReadEpoch>,
    pub(crate) source: ViewSourceStamp,
    pub(crate) revision: ViewRevision,
    pub(crate) mode: ReadMode,
    pub(crate) freshness: Freshness,
    pub(crate) projection: ProjectionState,
    pub(crate) join: JoinState,
    pub(crate) order: OrderBasis,
    pub(crate) failure: Option<ViewError>,
}
impl ViewDescriptor {
    #[must_use]
    pub const fn view_epoch(self) -> ViewEpoch {
        self.epoch
    }
    #[must_use]
    pub const fn read_epoch(self) -> Option<ReadEpoch> {
        self.read
    }
    #[must_use]
    pub const fn source_stamp(self) -> ViewSourceStamp {
        self.source
    }
    #[must_use]
    pub const fn revision(self) -> ViewRevision {
        self.revision
    }
    #[must_use]
    pub const fn read_mode(self) -> ReadMode {
        self.mode
    }
    #[must_use]
    pub const fn freshness(self) -> Freshness {
        self.freshness
    }
    #[must_use]
    pub const fn projection(self) -> ProjectionState {
        self.projection
    }
    #[must_use]
    pub const fn join(self) -> JoinState {
        self.join
    }
    #[must_use]
    pub const fn order_basis(self) -> OrderBasis {
        self.order
    }
    #[must_use]
    pub const fn history_incomplete(self) -> bool {
        !matches!(self.mode, ReadMode::ReplayComplete)
    }
    #[must_use]
    pub const fn cache_recoverable(self) -> bool {
        false
    }
    #[must_use]
    pub const fn failure(self) -> Option<ViewError> {
        self.failure
    }
}

/// A platform snapshot boundary, not a native turn or continuation token.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub(crate) epoch: ViewEpoch,
    pub(crate) highwater: u64,
    pub(crate) anchors: usize,
}
impl Snapshot {
    #[must_use]
    pub const fn view_epoch(self) -> ViewEpoch {
        self.epoch
    }
    #[must_use]
    pub const fn highwater(self) -> u64 {
        self.highwater
    }
    #[must_use]
    pub const fn anchor_count(self) -> usize {
        self.anchors
    }
}
