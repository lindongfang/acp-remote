//! Physically isolated persistent display snapshot store (`transcript-cache.sqlite3`),
//! 30-day sliding inactivity retention, global byte budget eviction, and synchronous delete purge.

mod coordinator;
mod digest;
mod schema;
mod store;
mod turns;
mod writer;

#[cfg(test)]
mod tests;

use std::{
    sync::atomic::{AtomicI64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

use crate::{
    adapters::acp::{RawMessage, Version, shared_view_projection::AcpViewProjector},
    application::shared_views::indexed_journal::{IndexedJournal, ViewProjector},
    domain::{
        EntityId,
        native::NativeSessionKey,
        shared_view::{RawRecordId, ViewEpoch, ViewError},
    },
    error::{AppError, PlatformError, StorageError},
};

pub use coordinator::{
    CachedViewHandle, RevalidationOutcome, TranscriptCacheCoordinator, encode_cache_state_json,
};
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HistoryGeneration(Option<(Vec<u8>, Vec<u8>)>);
pub(crate) use digest::IncrementalDigest;
pub use digest::compute_content_digest;
pub use store::{CachedPage, PageReadError, TranscriptCacheStore};
pub use writer::TranscriptCacheWriter;

pub(crate) fn entity_id_to_bytes(id: EntityId) -> Result<[u8; 16], TranscriptCacheError> {
    let encoded = id.encode();
    let decoded = URL_SAFE_NO_PAD.decode(encoded.as_bytes()).map_err(|_| {
        TranscriptCacheError::CorruptDatabase("invalid EntityId encoding".to_owned())
    })?;
    decoded
        .try_into()
        .map_err(|_| TranscriptCacheError::CorruptDatabase("EntityId byte length != 16".to_owned()))
}

/// Non-blocking operational health indicator for the local transcript cache store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CacheHealth {
    Healthy,
    DegradedWriteWarning { reason: String },
}

/// Persisted capability classification discovered from a native agent's ACP initialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityProfile {
    Unknown,
    ReplayCapable,
    ResumeOnly,
    PureMemory,
}

impl CapabilityProfile {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::ReplayCapable => "replay_capable",
            Self::ResumeOnly => "resume_only",
            Self::PureMemory => "pure_memory",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "unknown" => Some(Self::Unknown),
            "replay_capable" => Some(Self::ReplayCapable),
            "resume_only" => Some(Self::ResumeOnly),
            "pure_memory" => Some(Self::PureMemory),
            _ => None,
        }
    }
}

/// Persisted native session recoverability state after validation or `session/resume`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeRecoveryState {
    Normal,
    NativeUnrecoverable,
}

impl NativeRecoveryState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::NativeUnrecoverable => "native_unrecoverable",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "normal" => Some(Self::Normal),
            "native_unrecoverable" => Some(Self::NativeUnrecoverable),
            _ => None,
        }
    }
}

/// Clock source used for 30-day retention and cleanup timestamps.
pub trait CacheRetentionClock: Send + Sync {
    fn now_millis(&self) -> i64;
}

/// Default wall-clock implementation backed by `SystemTime::now()`.
#[derive(Debug, Default)]
pub struct SystemRetentionClock;

impl CacheRetentionClock for SystemRetentionClock {
    fn now_millis(&self) -> i64 {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
    }
}

/// Controllable millisecond clock for deterministic retention/eviction testing.
#[derive(Debug)]
pub struct ManualRetentionClock {
    now_ms: AtomicI64,
}

impl ManualRetentionClock {
    #[must_use]
    pub const fn new(initial_ms: i64) -> Self {
        Self {
            now_ms: AtomicI64::new(initial_ms),
        }
    }

    pub fn set_millis(&self, millis: i64) {
        self.now_ms.store(millis, Ordering::Release);
    }

    pub fn advance_millis(&self, delta_ms: i64) {
        self.now_ms.fetch_add(delta_ms, Ordering::AcqRel);
    }
}

impl CacheRetentionClock for ManualRetentionClock {
    fn now_millis(&self) -> i64 {
        self.now_ms.load(Ordering::Acquire)
    }
}

/// Read-only port queried before evicting sessions for expiration or byte budget.
pub trait ProtectedSessionsPort: Send + Sync {
    fn is_protected(&self, key: &NativeSessionKey) -> bool;
}

/// Default protection policy when no coordinator is attached yet.
#[derive(Debug, Default)]
pub struct NoProtectedSessions;

impl ProtectedSessionsPort for NoProtectedSessions {
    fn is_protected(&self, _key: &NativeSessionKey) -> bool {
        false
    }
}

/// One persisted ACP `session/update` frame paired with its observed connection sequence.
#[derive(Clone, Debug)]
pub struct CachedRawRecord {
    pub ordinal: u64,
    pub connection_sequence: u64,
    pub raw: RawMessage,
}

/// Metadata stored in `transcript_sessions`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedTranscriptSession {
    pub key: NativeSessionKey,
    pub active_generation: u64,
    pub protocol_version: Version,
    pub content_digest: [u8; 32],
    pub last_updated_at_ms: i64,
    pub last_synced_at_ms: Option<i64>,
    pub capability_profile: CapabilityProfile,
    pub native_recovery_state: NativeRecoveryState,
    pub record_count: u64,
    pub byte_size: u64,
}

/// Loaded cold display snapshot ready for offline projection into `SharedViews`.
#[derive(Clone, Debug)]
pub struct LoadedTranscriptSnapshot {
    pub session: PersistedTranscriptSession,
    pub records: Vec<CachedRawRecord>,
}

impl LoadedTranscriptSnapshot {
    #[must_use]
    pub fn frames(&self) -> Vec<RawMessage> {
        self.records.iter().map(|entry| entry.raw.clone()).collect()
    }

    /// Projects persisted raw ACP frames through the session's recorded `protocol_version`.
    ///
    /// # Errors
    ///
    /// Returns `ViewError` if any frame triggers projection overflow or invalid segment sequencing.
    pub fn project_into_journal(&self) -> Result<IndexedJournal<RawMessage>, ViewError> {
        let mut journal = IndexedJournal::new()?;
        let segment = ViewEpoch::fresh()?;
        let projector = AcpViewProjector::for_version(self.session.protocol_version);
        for (idx, record) in self.records.iter().enumerate() {
            let revision = u64::try_from(idx + 1).map_err(|_| ViewError::SequenceExhausted)?;
            let envelope = projector.inspect(&record.raw);
            journal.append_segment_at(
                segment,
                0,
                RawRecordId(revision),
                revision,
                record.raw.clone(),
                record.connection_sequence,
                record.ordinal,
                envelope,
            )?;
        }
        Ok(journal)
    }
}

/// Outcome of replacing a snapshot from full native replay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplaceOutcome {
    Updated,
    UnchangedDigest,
}

/// Summary of one retention & LRU budget enforcement pass.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CleanupReport {
    pub expired_sessions: usize,
    pub lru_evicted_sessions: usize,
    pub remaining_sessions: usize,
    pub remaining_bytes: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum TranscriptCacheError {
    #[error("transcript cache IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("transcript cache SQLite error: {0}")]
    Sqlite(#[from] sqlx::Error),
    #[error("transcript cache lifecycle error: {0}")]
    Storage(#[from] StorageError),
    #[error("transcript cache driver error: {0}")]
    Lifecycle(#[from] std::sync::Arc<StorageError>),
    #[error("transcript cache platform ACL error: {0}")]
    Platform(#[from] PlatformError),
    #[error("transcript cache application error: {0}")]
    App(#[from] AppError),
    #[error("transcript cache view projection error: {0}")]
    View(#[from] ViewError),
    #[error("transcript cache protocol normalization error: {0}")]
    Protocol(String),
    #[error("transcript cache database is corrupted: {0}")]
    CorruptDatabase(String),
    #[error("transcript cache schema mismatch (expected {expected}, got {actual})")]
    SchemaMismatch { expected: i64, actual: i64 },
    #[error("transcript cache is running in degraded mode: {0}")]
    Degraded(String),
    #[error("transcript cache database is unavailable")]
    DatabaseUnavailable,
    #[error("transcript cache is shutting down")]
    ShuttingDown,
}
