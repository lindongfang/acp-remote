//! History identities and committed facts; persistence policy follows session-contract §5–6.
use super::EntityId;
use std::{fmt, sync::Arc};
pub const MAX_VERSION: u64 = 9_007_199_254_740_991;
macro_rules! history_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub struct $name(pub EntityId);
        impl $name {
            #[must_use]
            pub fn encode(self) -> String {
                self.0.encode()
            }
        }
    };
}
history_id!(SessionId);
history_id!(RunId);
history_id!(TurnId);
history_id!(ItemId);
history_id!(SourceId);
history_id!(InstanceId);
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OutputScope {
    pub session: SessionId,
    pub run: RunId,
    pub turn: TurnId,
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Position(pub(crate) u64);
impl Position {
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ContentVersion(pub(crate) u64);
impl ContentVersion {
    /// # Errors
    /// Rejects zero, unsafe integers and exhausted counters.
    pub const fn new(value: u64) -> Result<Self, HistoryError> {
        if value == 0 || value > MAX_VERSION {
            Err(HistoryError::CounterExhausted)
        } else {
            Ok(Self(value))
        }
    }
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}
#[derive(Debug)]
pub struct PositionReservation {
    pub(crate) next: u64,
    pub(crate) end: u64,
}
impl PositionReservation {
    pub const fn take(&mut self) -> Option<Position> {
        if self.next >= self.end {
            None
        } else {
            let position = Position(self.next);
            self.next += 1;
            Some(position)
        }
    }
}
/// Runtime data coordinates, never a source credential.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConnectionEpoch {
    pub instance: InstanceId,
    pub generation: u64,
}
impl ConnectionEpoch {
    /// # Errors
    /// Rejects generations outside the durable protocol-safe integer range.
    pub const fn validate(self) -> Result<(), HistoryError> {
        if self.generation > MAX_VERSION {
            Err(HistoryError::IdentityConflict)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct NativeSessionId(Arc<str>);
impl NativeSessionId {
    /// # Errors
    /// Rejects empty identifiers and identifiers exceeding the content-object budget.
    pub fn new(value: Arc<str>) -> Result<Self, HistoryError> {
        if value.is_empty() || value.len() > 16 * 1024 * 1024 {
            Err(HistoryError::IdentityConflict)
        } else {
            Ok(Self(value))
        }
    }
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for NativeSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeSessionId")
            .field("bytes", &self.0.len())
            .finish()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceOrigin {
    pub connection: ConnectionEpoch,
    pub native_session: NativeSessionId,
}
#[derive(Clone)]
pub struct ToolDescriptor {
    pub source: SourceId,
    pub protocol: Arc<str>,
    pub protocol_version: Arc<str>,
    pub origin: SourceOrigin,
    pub tool_call_id: NativeToolCallId,
}
/// A protocol tool identifier, not text extracted from a tool's body.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct NativeToolCallId(Arc<str>);
impl NativeToolCallId {
    /// # Errors
    /// Rejects empty identifiers or identifiers beyond the existing content-object budget.
    pub fn new(value: Arc<str>) -> Result<Self, HistoryError> {
        if value.is_empty() || value.len() > 16 * 1024 * 1024 {
            Err(HistoryError::IdentityConflict)
        } else {
            Ok(Self(value))
        }
    }
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for NativeToolCallId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeToolCallId")
            .field("bytes", &self.0.len())
            .finish()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolReference {
    pub session: SessionId,
    pub item: ItemId,
    pub source: SourceId,
}
#[derive(Clone)]
pub struct ItemDescriptor {
    pub source: SourceId,
    pub kind: Arc<str>,
    pub protocol: Arc<str>,
    pub protocol_version: Arc<str>,
    pub tool_call_id: Option<NativeToolCallId>,
    pub tool_origin: Option<SourceOrigin>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemIdentity {
    pub scope: OutputScope,
    pub item: ItemId,
    pub source: SourceId,
    pub position: Position,
    pub kind: Arc<str>,
    pub protocol: Arc<str>,
    pub protocol_version: Arc<str>,
    pub tool_call_id: Option<NativeToolCallId>,
    pub tool_origin: Option<SourceOrigin>,
}
/// Immutable original bytes, including unsupported fields and nested structure.
#[derive(Clone)]
pub struct RawContent(pub Arc<[u8]>);
impl fmt::Debug for RawContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawContent")
            .field("bytes", &self.0.len())
            .finish()
    }
}
#[derive(Clone, Debug)]
pub struct HistoryItem {
    pub identity: Arc<ItemIdentity>,
    pub version: ContentVersion,
    pub content: RawContent,
}
impl HistoryItem {
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.content
            .0
            .len()
            .saturating_add(1024)
            .saturating_add(self.identity.kind.len())
            .saturating_add(self.identity.protocol.len())
            .saturating_add(self.identity.protocol_version.len())
            .saturating_add(
                self.identity
                    .tool_call_id
                    .as_ref()
                    .map_or(0, |id| id.as_str().len()),
            )
            .saturating_add(
                self.identity
                    .tool_origin
                    .as_ref()
                    .map_or(0, |origin| origin.native_session.as_str().len()),
            )
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitAck {
    pub item: ItemId,
    pub version: ContentVersion,
}
#[derive(Clone, Debug)]
pub struct CommittedItem {
    pub item: HistoryItem,
    pub content_ref: EntityId,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlushReason {
    Periodic,
    ItemEnd,
    RunEnd,
    BeforeApproval,
    Release,
    Shutdown,
}
impl FlushReason {
    #[must_use]
    pub const fn forced(self) -> bool {
        !matches!(self, Self::Periodic)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegrityReason {
    SaveFailed,
    PersistentFailure,
    Capacity,
    Interrupted,
    Cancelled,
}
impl IntegrityReason {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::SaveFailed => "save_failed",
            Self::PersistentFailure => "persistent_failure",
            Self::Capacity => "capacity",
            Self::Interrupted => "interrupted",
            Self::Cancelled => "cancelled",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MissingRange {
    pub first: Position,
    pub last: Position,
    pub dropped_bytes: u64,
}
impl MissingRange {
    pub fn include(&mut self, position: Position, dropped_bytes: usize) {
        self.first = self.first.min(position);
        self.last = self.last.max(position);
        self.dropped_bytes = self
            .dropped_bytes
            .saturating_add(u64::try_from(dropped_bytes).unwrap_or(u64::MAX));
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IntegrityFault {
    pub reason: IntegrityReason,
    pub possible_loss: Option<MissingRange>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegrityChange {
    Keep,
    Set(IntegrityFault),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopReceipt {
    Requested,
    ResponsibilityRetained,
}
#[derive(Clone, Debug)]
pub struct ReadPin {
    pub pin: EntityId,
    pub content_ref: EntityId,
    pub version: ContentVersion,
}
#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("history storage is unavailable")]
    StorageUnavailable,
    #[error("history state is invalid")]
    InvalidState,
    #[error("history identity conflicts with stored source")]
    IdentityConflict,
    #[error("history version conflicts with stored content")]
    VersionConflict,
    #[error("history counter is exhausted")]
    CounterExhausted,
    #[error("history data is outside the retained budget")]
    Capacity,
    #[error("history output is stopped")]
    Stopped,
    #[error("history writer is closed")]
    Closed,
    #[error("history retry is not yet due")]
    RetryDeferred,
    #[error("history clock moved backwards")]
    ClockReversed,
    #[error("history owner state is unavailable")]
    OwnerUnavailable,
    #[error("history owner did not close within its deadline")]
    CloseTimeout,
    #[error("history identifier generation is unavailable")]
    RandomUnavailable,
}
