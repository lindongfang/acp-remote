//! Isolated SQLite store (`transcript-cache.sqlite3`) operations, retention, LRU eviction, and shutdown.

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteJournalMode, SqlitePoolOptions},
};
use tokio::sync::{Mutex as AsyncMutex, watch};

use crate::{
    adapters::acp::{RawMessage, Version},
    config::TranscriptCacheConfig,
    data_dir::secure_transcript_cache_at_base,
    domain::{
        EntityId,
        native::{NativeSessionKey, NativeSourceId},
    },
    error::AppError,
    platform::protect_private_file,
    storage::{StorageLifecycleHost, StorageOpenOwner, StorageOpenRoots},
};

use super::{
    CacheHealth, CacheRetentionClock, CachedRawRecord, CapabilityProfile, CleanupReport,
    LoadedTranscriptSnapshot, NativeRecoveryState, NoProtectedSessions, PersistedTranscriptSession,
    ProtectedSessionsPort, ReplaceOutcome, TranscriptCacheError,
    digest::compute_content_digest,
    entity_id_to_bytes,
    schema::initialize_schema,
    writer::{TranscriptCacheWriter, WriterControl, spawn_writer},
};

mod history;
pub use history::{CachedPage, PageReadError};

const CLEANUP_INTERVAL: Duration = Duration::from_secs(300);

struct OpenBackend {
    db_path: PathBuf,
    base_dir: PathBuf,
    is_default_cache_name: bool,
    pool: SqlitePool,
    owner: StorageOpenOwner,
    host: Arc<StorageLifecycleHost>,
    write_mutex: Arc<AsyncMutex<()>>,
    writer_handle: Option<tokio::task::JoinHandle<()>>,
    cleanup_handle: Option<tokio::task::JoinHandle<()>>,
    stop_tx: watch::Sender<bool>,
}

enum StoreState {
    Open(OpenBackend),
    Degraded,
    Closed,
}

struct RowCandidate {
    key: NativeSessionKey,
    byte_size: u64,
    last_updated_at_ms: i64,
    deleted: bool,
}

/// Isolated display snapshot SQLite store backed by `transcript-cache.sqlite3`.
#[derive(Clone)]
pub struct TranscriptCacheStore {
    inner: Arc<StoreInner>,
}

struct StoreInner {
    db_path: PathBuf,
    config: TranscriptCacheConfig,
    clock: Arc<dyn CacheRetentionClock>,
    protected: std::sync::RwLock<Arc<dyn ProtectedSessionsPort>>,
    control: Arc<WriterControl>,
    writer: TranscriptCacheWriter,
    state: AsyncMutex<StoreState>,
    cached_health: Mutex<CacheHealth>,
}

impl TranscriptCacheStore {
    /// Opens an isolated `transcript-cache.sqlite3` database at `db_path`, returning an error
    /// if initialization, integrity checks, or owner-only ACL hardening fail.
    ///
    /// # Errors
    ///
    /// Returns `TranscriptCacheError` on unsafe paths, corrupted SQLite files, schema mismatch, or
    /// worker lifecycle initialization failure.
    pub async fn open(
        db_path: PathBuf,
        config: TranscriptCacheConfig,
        clock: Arc<dyn CacheRetentionClock>,
        protected: Arc<dyn ProtectedSessionsPort>,
    ) -> Result<Self, TranscriptCacheError> {
        let base_dir = db_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| {
                TranscriptCacheError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "transcript cache path must have a parent directory",
                ))
            })?
            .to_path_buf();
        let is_default_cache_name =
            db_path.file_name() == Some(OsStr::new("transcript-cache.sqlite3"));

        std::fs::create_dir_all(&base_dir)?;
        harden_cache_files(&db_path, &base_dir, is_default_cache_name)?;

        let owner = StorageOpenOwner::prepare(
            &tokio::runtime::Handle::current(),
            StorageOpenRoots::path(&db_path),
        )?;
        let host = StorageLifecycleHost::install(owner.clone());

        let opened = open_pool_and_schema(&db_path, &base_dir, is_default_cache_name, &owner).await;
        let pool = match opened {
            Ok(pool) => pool,
            Err(error) => {
                owner.request_stop();
                let _ = host.observe_actual().await;
                return Err(error);
            }
        };

        let control = Arc::new(WriterControl::new(CacheHealth::Healthy));
        let write_mutex = Arc::new(AsyncMutex::new(()));
        let (writer, writer_handle) = spawn_writer(
            pool.clone(),
            Arc::clone(&write_mutex),
            Arc::clone(&clock),
            Arc::clone(&control),
        );
        let (stop_tx, stop_rx) = watch::channel(false);

        let inner = Arc::new(StoreInner {
            db_path: db_path.clone(),
            config,
            clock,
            protected: std::sync::RwLock::new(protected),
            control,
            writer,
            state: AsyncMutex::new(StoreState::Open(OpenBackend {
                db_path,
                base_dir,
                is_default_cache_name,
                pool,
                owner,
                host,
                write_mutex,
                writer_handle: Some(writer_handle),
                cleanup_handle: None,
                stop_tx,
            })),
            cached_health: Mutex::new(CacheHealth::Healthy),
        });

        let cleanup_store = Self {
            inner: Arc::clone(&inner),
        };
        let cleanup_handle = tokio::spawn(run_periodic_cleanup(cleanup_store, stop_rx));
        {
            let mut guard = inner.state.lock().await;
            if let StoreState::Open(ref mut backend) = *guard {
                backend.cleanup_handle = Some(cleanup_handle);
            } else {
                cleanup_handle.abort();
            }
            drop(guard);
        }

        Ok(Self { inner })
    }

    /// Opens an isolated `transcript-cache.sqlite3` database, degrading cleanly with
    /// `CacheHealth::DegradedWriteWarning` instead of failing startup if the cache file is corrupt
    /// or unwritable.
    pub async fn open_or_degrade(
        db_path: PathBuf,
        config: TranscriptCacheConfig,
        clock: Arc<dyn CacheRetentionClock>,
        protected: Arc<dyn ProtectedSessionsPort>,
    ) -> Self {
        match Self::open(
            db_path.clone(),
            config,
            Arc::clone(&clock),
            Arc::clone(&protected),
        )
        .await
        {
            Ok(store) => store,
            Err(error) => {
                tracing::warn!(%error, "transcript cache unavailable at startup; degrading to live-only mode");
                let health = CacheHealth::DegradedWriteWarning {
                    reason: error.to_string(),
                };
                let control = Arc::new(WriterControl::new(health.clone()));
                Self {
                    inner: Arc::new(StoreInner {
                        db_path,
                        config,
                        clock,
                        protected: std::sync::RwLock::new(protected),
                        writer: TranscriptCacheWriter::disabled(Arc::clone(&control)),
                        control,
                        state: AsyncMutex::new(StoreState::Degraded),
                        cached_health: Mutex::new(health),
                    }),
                }
            }
        }
    }

    /// Opens a degraded, non-persisting store with an explicit initial warning reason.
    #[must_use]
    pub fn degraded(
        db_path: PathBuf,
        config: TranscriptCacheConfig,
        clock: Arc<dyn CacheRetentionClock>,
        reason: String,
    ) -> Self {
        let health = CacheHealth::DegradedWriteWarning { reason };
        let control = Arc::new(WriterControl::new(health.clone()));
        Self {
            inner: Arc::new(StoreInner {
                db_path,
                config,
                clock,
                protected: std::sync::RwLock::new(Arc::new(NoProtectedSessions)),
                writer: TranscriptCacheWriter::disabled(Arc::clone(&control)),
                control,
                state: AsyncMutex::new(StoreState::Degraded),
                cached_health: Mutex::new(health),
            }),
        }
    }

    /// Replaces the `ProtectedSessionsPort` consulted during expiry and LRU cleanup.
    pub fn set_protected_sessions(&self, protected: Arc<dyn ProtectedSessionsPort>) {
        let mut guard = self
            .inner
            .protected
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = protected;
    }

    #[must_use]
    pub fn db_path(&self) -> &Path {
        &self.inner.db_path
    }

    #[must_use]
    pub fn config(&self) -> TranscriptCacheConfig {
        self.inner.config
    }

    #[must_use]
    pub fn is_snapshot_expired(&self, session: &PersistedTranscriptSession) -> bool {
        let elapsed = self
            .inner
            .clock
            .now_millis()
            .saturating_sub(session.last_updated_at_ms);
        elapsed > self.inner.config.retention_millis()
    }

    #[must_use]
    pub fn writer(&self) -> TranscriptCacheWriter {
        self.inner.writer.clone()
    }

    #[cfg(test)]
    pub(crate) async fn pool_for_test(&self) -> Option<SqlitePool> {
        let guard = self.inner.state.lock().await;
        let pool = match *guard {
            StoreState::Open(ref backend) => Some(backend.pool.clone()),
            StoreState::Degraded | StoreState::Closed => None,
        };
        drop(guard);
        pool
    }

    #[must_use]
    pub fn health(&self) -> CacheHealth {
        let writer_health = self.inner.control.health();
        if !matches!(writer_health, CacheHealth::Healthy) {
            return writer_health;
        }
        self.inner
            .cached_health
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn current_protected(&self) -> Arc<dyn ProtectedSessionsPort> {
        Arc::clone(
            &self
                .inner
                .protected
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    fn record_degraded(&self, reason: String) {
        self.inner.control.mark_degraded(reason.clone());
        let mut guard = self
            .inner
            .cached_health
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = CacheHealth::DegradedWriteWarning { reason };
    }

    async fn open_handles(&self) -> Option<(SqlitePool, Arc<AsyncMutex<()>>)> {
        let guard = self.inner.state.lock().await;
        let handles = match *guard {
            StoreState::Open(ref backend) => {
                Some((backend.pool.clone(), Arc::clone(&backend.write_mutex)))
            }
            StoreState::Degraded | StoreState::Closed => None,
        };
        drop(guard);
        handles
    }

    /// Loads a cold display snapshot from SQLite without bumping `last_updated_at_ms`.
    ///
    /// Returns `Ok(None)` when missing or when the store is running in degraded mode.
    ///
    /// # Errors
    ///
    /// Returns `TranscriptCacheError` on corrupt JSON/schema content or storage failure.
    pub async fn load_snapshot(
        &self,
        key: &NativeSessionKey,
    ) -> Result<Option<LoadedTranscriptSnapshot>, TranscriptCacheError> {
        let Some((pool, _)) = self.open_handles().await else {
            return Ok(None);
        };

        let source_id_bytes = entity_id_to_bytes(key.source().0)?;
        let source_bytes = source_id_bytes.as_slice();
        let native_id = key.native_id();

        let session_row = sqlx::query(
            r"
            SELECT active_generation, protocol_version, content_digest,
                   last_updated_at_ms, last_synced_at_ms, capability_profile,
                   native_recovery_state, record_count, byte_size
            FROM transcript_sessions
            WHERE source_id = ? AND native_session_id = ?
            ",
        )
        .bind(source_bytes)
        .bind(native_id)
        .fetch_optional(&pool)
        .await;

        let session_row = match session_row {
            Ok(Some(row)) => row,
            Ok(None) => return Ok(None),
            Err(error) => {
                self.record_degraded(error.to_string());
                return Err(TranscriptCacheError::Sqlite(error));
            }
        };

        let session = parse_session_row(key.clone(), &session_row)?;
        let generation_i64 = i64::try_from(session.active_generation).map_err(|_| {
            TranscriptCacheError::CorruptDatabase("active_generation exceeds i64".to_owned())
        })?;

        let record_rows = sqlx::query(
            r"
            SELECT ordinal, connection_sequence, raw_json
            FROM transcript_records
            WHERE source_id = ? AND native_session_id = ? AND generation = ?
            ORDER BY ordinal ASC
            ",
        )
        .bind(source_bytes)
        .bind(native_id)
        .bind(generation_i64)
        .fetch_all(&pool)
        .await?;

        let mut records = Vec::with_capacity(record_rows.len());
        for row in record_rows {
            let ordinal_i64: i64 = row.try_get("ordinal")?;
            let conn_seq_i64: i64 = row.try_get("connection_sequence")?;
            let raw_bytes: Vec<u8> = row.try_get("raw_json")?;
            let raw = RawMessage::parse(raw_bytes).map_err(|error| {
                TranscriptCacheError::Protocol(format!("corrupt cached frame JSON: {error}"))
            })?;
            records.push(CachedRawRecord {
                ordinal: u64::try_from(ordinal_i64).unwrap_or(1),
                connection_sequence: u64::try_from(conn_seq_i64).unwrap_or(0),
                raw,
            });
        }

        Ok(Some(LoadedTranscriptSnapshot { session, records }))
    }

    /// Replaces the persisted snapshot atomically from a full native replay.
    ///
    /// When the canonical `content_digest` matches the existing stored digest, only
    /// `last_synced_at_ms` (and capability/recovery metadata) is updated; `last_updated_at_ms` and
    /// `active_generation` are deliberately preserved so read-only replays never falsely renew the
    /// 30-day retention window.
    ///
    /// # Errors
    ///
    /// Returns `TranscriptCacheError` on frame normalization or SQLite write failure.
    pub async fn replace_with_replay(
        &self,
        key: &NativeSessionKey,
        protocol_version: Version,
        capability_profile: CapabilityProfile,
        frames: Vec<(RawMessage, u64)>,
    ) -> Result<ReplaceOutcome, TranscriptCacheError> {
        let raw_only: Vec<RawMessage> = frames.iter().map(|(raw, _)| raw.clone()).collect();
        let new_digest = compute_content_digest(protocol_version, &raw_only)?;

        let (pool, write_mutex) = self
            .open_handles()
            .await
            .ok_or(TranscriptCacheError::DatabaseUnavailable)?;

        self.inner.writer.reset_key(key).await;
        let _tx_lock = write_mutex.lock().await;
        let now_ms = self.inner.clock.now_millis();
        apply_replay_generation(
            &pool,
            key,
            protocol_version,
            capability_profile,
            &new_digest,
            &frames,
            now_ms,
        )
        .await
    }

    /// Updates capability profile and recovery state on an existing cached session without changing
    /// `last_updated_at_ms`.
    ///
    /// # Errors
    ///
    /// Returns `TranscriptCacheError` on SQLite failure.
    pub async fn update_session_status(
        &self,
        key: &NativeSessionKey,
        capability_profile: Option<CapabilityProfile>,
        native_recovery_state: Option<NativeRecoveryState>,
    ) -> Result<(), TranscriptCacheError> {
        let Some((pool, write_mutex)) = self.open_handles().await else {
            return Ok(());
        };

        let _tx_lock = write_mutex.lock().await;
        let source_id_bytes = entity_id_to_bytes(key.source().0)?;
        let source_bytes = source_id_bytes.as_slice();
        let native_id = key.native_id();

        if let Some(profile) = capability_profile {
            sqlx::query(
                r"
                UPDATE transcript_sessions
                SET capability_profile = ?
                WHERE source_id = ? AND native_session_id = ?
                ",
            )
            .bind(profile.as_str())
            .bind(source_bytes)
            .bind(native_id)
            .execute(&pool)
            .await?;
        }

        if let Some(recovery) = native_recovery_state {
            sqlx::query(
                r"
                UPDATE transcript_sessions
                SET native_recovery_state = ?
                WHERE source_id = ? AND native_session_id = ?
                ",
            )
            .bind(recovery.as_str())
            .bind(source_bytes)
            .bind(native_id)
            .execute(&pool)
            .await?;
        }

        Ok(())
    }

    /// Synchronously deletes all cached rows and WAL plaintext pages for a deleted native session.
    ///
    /// Seals any buffered live writes for `key`, removes rows inside SQLite (with
    /// `PRAGMA secure_delete = ON`), and checkpoints WAL (`TRUNCATE`) before returning.
    ///
    /// # Errors
    ///
    /// Returns `TranscriptCacheError` if SQLite delete or WAL checkpoint fails.
    pub async fn purge_deleted_native_session(
        &self,
        key: &NativeSessionKey,
    ) -> Result<(), TranscriptCacheError> {
        let new_epoch = self.inner.control.bump_purge_epoch(key);
        self.inner.writer.seal_purged_key(key, new_epoch).await;

        let Some((pool, write_mutex)) = self.open_handles().await else {
            return Ok(());
        };

        let _tx_lock = write_mutex.lock().await;
        let source_id_bytes = entity_id_to_bytes(key.source().0)?;
        let source_bytes = source_id_bytes.as_slice();
        let native_id = key.native_id();

        let mut tx = pool.begin().await?;
        sqlx::query(
            r"
            DELETE FROM transcript_sessions
            WHERE source_id = ? AND native_session_id = ?
            ",
        )
        .bind(source_bytes)
        .bind(native_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&pool)
            .await?;
        Ok(())
    }

    /// Enforces 30-day sliding inactivity expiration and global `max_bytes` LRU eviction while
    /// preserving any session protected by active slots or active chunk-read leases.
    ///
    /// # Errors
    ///
    /// Returns `TranscriptCacheError` if querying or deleting expired rows fails.
    pub async fn enforce_retention_and_budget(
        &self,
        protected: Option<&dyn ProtectedSessionsPort>,
    ) -> Result<CleanupReport, TranscriptCacheError> {
        let fallback_protected = self.current_protected();
        let protected = protected.unwrap_or_else(|| fallback_protected.as_ref());

        let Some((pool, write_mutex)) = self.open_handles().await else {
            return Ok(CleanupReport::default());
        };

        let _tx_lock = write_mutex.lock().await;
        let now_ms = self.inner.clock.now_millis();
        let retention_ms = self.inner.config.retention_millis();

        let mut tx = pool.begin().await?;
        let candidates = sqlx::query(
            r"
            SELECT source_id, native_session_id, byte_size, last_updated_at_ms
            FROM transcript_sessions
            ORDER BY last_updated_at_ms ASC, source_id ASC, native_session_id ASC
            ",
        )
        .fetch_all(&mut *tx)
        .await?;

        let (mut parsed, mut total_bytes) = parse_candidates(&candidates)?;

        let mut expired_sessions: usize = 0;
        let mut lru_evicted_sessions: usize = 0;

        for candidate in &mut parsed {
            if now_ms.saturating_sub(candidate.last_updated_at_ms) <= retention_ms {
                continue;
            }
            if protected.is_protected(&candidate.key) {
                continue;
            }
            delete_session_row(&mut tx, &candidate.key).await?;
            candidate.deleted = true;
            expired_sessions += 1;
            total_bytes = total_bytes.saturating_sub(candidate.byte_size);
        }

        if total_bytes > self.inner.config.max_bytes {
            for candidate in &mut parsed {
                if total_bytes <= self.inner.config.max_bytes {
                    break;
                }
                if candidate.deleted || protected.is_protected(&candidate.key) {
                    continue;
                }
                delete_session_row(&mut tx, &candidate.key).await?;
                candidate.deleted = true;
                lru_evicted_sessions += 1;
                total_bytes = total_bytes.saturating_sub(candidate.byte_size);
            }
        }

        let remaining_sessions = parsed.iter().filter(|entry| !entry.deleted).count();
        tx.commit().await?;

        if expired_sessions > 0 || lru_evicted_sessions > 0 {
            sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
                .execute(&pool)
                .await?;
        }

        Ok(CleanupReport {
            expired_sessions,
            lru_evicted_sessions,
            remaining_sessions,
            remaining_bytes: total_bytes,
        })
    }

    /// Deterministically flushes buffered writes, stops background cleanup tasks, closes the
    /// SQLite pool, and waits for SQLite worker thread shutdown so files on Windows can be removed
    /// immediately.
    ///
    /// # Errors
    ///
    /// Returns `TranscriptCacheError` if closing or re-securing fails.
    pub async fn shutdown(&self) -> Result<(), TranscriptCacheError> {
        let backend = {
            let mut guard = self.inner.state.lock().await;
            let prev = std::mem::replace(&mut *guard, StoreState::Closed);
            drop(guard);
            match prev {
                StoreState::Open(backend) => Some(backend),
                StoreState::Degraded | StoreState::Closed => None,
            }
        };

        let Some(mut backend) = backend else {
            return Ok(());
        };

        let _ = backend.stop_tx.send(true);
        if let Some(handle) = backend.cleanup_handle.take() {
            let _ = handle.await;
        }

        self.inner.writer.send_shutdown().await;
        if let Some(handle) = backend.writer_handle.take() {
            let _ = handle.await;
        }

        loop {
            backend.pool.close().await;
            if backend.pool.size() == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        backend.owner.request_stop();
        let _ = backend.host.observe_actual().await;

        let _ = harden_cache_files(
            &backend.db_path,
            &backend.base_dir,
            backend.is_default_cache_name,
        );

        Ok(())
    }
}

async fn apply_replay_generation(
    pool: &SqlitePool,
    key: &NativeSessionKey,
    protocol_version: Version,
    capability_profile: CapabilityProfile,
    new_digest: &[u8; 32],
    frames: &[(RawMessage, u64)],
    now_ms: i64,
) -> Result<ReplaceOutcome, TranscriptCacheError> {
    let source_id_bytes = entity_id_to_bytes(key.source().0)?;
    let source_bytes = source_id_bytes.as_slice();
    let native_id = key.native_id();
    let version_i64 = match protocol_version {
        Version::V1 => 1_i64,
        Version::V2 => 2_i64,
    };

    let mut tx = pool.begin().await?;
    let existing = sqlx::query(
        r"
        SELECT active_generation, content_digest
        FROM transcript_sessions
        WHERE source_id = ? AND native_session_id = ?
        ",
    )
    .bind(source_bytes)
    .bind(native_id)
    .fetch_optional(&mut *tx)
    .await?;

    let next_generation = if let Some(row) = existing {
        let prev_generation: i64 = row.try_get("active_generation")?;
        let existing_digest: Vec<u8> = row.try_get("content_digest")?;
        if existing_digest.as_slice() == new_digest.as_slice() {
            sqlx::query(
                r"
                UPDATE transcript_sessions
                SET capability_profile = ?,
                    native_recovery_state = 'normal',
                    last_synced_at_ms = ?
                WHERE source_id = ? AND native_session_id = ?
                ",
            )
            .bind(capability_profile.as_str())
            .bind(now_ms)
            .bind(source_bytes)
            .bind(native_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(ReplaceOutcome::UnchangedDigest);
        }
        prev_generation.checked_add(1).ok_or_else(|| {
            TranscriptCacheError::Protocol("active_generation overflow".to_owned())
        })?
    } else {
        1_i64
    };

    write_replay_generation_rows(
        &mut tx,
        source_bytes,
        native_id,
        next_generation,
        version_i64,
        capability_profile,
        new_digest,
        frames,
        now_ms,
    )
    .await?;

    tx.commit().await?;
    Ok(ReplaceOutcome::Updated)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Replay generation write binds explicit transaction and SQL columns cleanly"
)]
async fn write_replay_generation_rows(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    source_bytes: &[u8],
    native_id: &str,
    next_generation: i64,
    version_i64: i64,
    capability_profile: CapabilityProfile,
    new_digest: &[u8; 32],
    frames: &[(RawMessage, u64)],
    now_ms: i64,
) -> Result<(), TranscriptCacheError> {
    let record_count = i64::try_from(frames.len())
        .map_err(|_| TranscriptCacheError::Protocol("record count exceeds i64".to_owned()))?;
    let total_bytes: u64 = frames.iter().map(|(f, _)| f.bytes().len() as u64).sum();
    let total_bytes_i64 = i64::try_from(total_bytes)
        .map_err(|_| TranscriptCacheError::Protocol("byte size exceeds i64".to_owned()))?;

    sqlx::query(
        r"
        INSERT INTO transcript_sessions (
            source_id,
            native_session_id,
            active_generation,
            protocol_version,
            content_digest,
            last_updated_at_ms,
            last_synced_at_ms,
            capability_profile,
            native_recovery_state,
            record_count,
            byte_size
        , generation_token) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'normal', ?, ?, randomblob(16))
        ON CONFLICT(source_id, native_session_id) DO UPDATE SET
            active_generation = excluded.active_generation,
            generation_token = excluded.generation_token,
            turn_state = 'pending',
            turn_count = 0,
            protocol_version = excluded.protocol_version,
            content_digest = excluded.content_digest,
            last_updated_at_ms = excluded.last_updated_at_ms,
            last_synced_at_ms = excluded.last_synced_at_ms,
            capability_profile = excluded.capability_profile,
            native_recovery_state = 'normal',
            record_count = excluded.record_count,
            byte_size = excluded.byte_size
        ",
    )
    .bind(source_bytes)
    .bind(native_id)
    .bind(next_generation)
    .bind(version_i64)
    .bind(new_digest.as_slice())
    .bind(now_ms)
    .bind(Some(now_ms))
    .bind(capability_profile.as_str())
    .bind(record_count)
    .bind(total_bytes_i64)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        r"
        DELETE FROM transcript_records
        WHERE source_id = ? AND native_session_id = ?
        ",
    )
    .bind(source_bytes)
    .bind(native_id)
    .execute(&mut **tx)
    .await?;

    write_replay_frames(
        tx,
        source_bytes,
        native_id,
        next_generation,
        version_i64,
        frames,
    )
    .await?;
    Ok(())
}

fn parse_candidates(
    candidates: &[sqlx::sqlite::SqliteRow],
) -> Result<(Vec<RowCandidate>, u64), TranscriptCacheError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    let mut total_bytes: u64 = 0;

    for row in candidates {
        let source_blob: Vec<u8> = row.try_get("source_id")?;
        let native_id: String = row.try_get("native_session_id")?;
        let byte_size_i64: i64 = row.try_get("byte_size")?;
        let last_updated_at_ms: i64 = row.try_get("last_updated_at_ms")?;

        let source_arr: [u8; 16] = source_blob.try_into().map_err(|_| {
            TranscriptCacheError::CorruptDatabase("source_id blob length != 16".to_owned())
        })?;
        let source = NativeSourceId(EntityId::from_bytes(source_arr));
        let key = NativeSessionKey::from_config(Some(source.0), &native_id).map_err(|error| {
            TranscriptCacheError::CorruptDatabase(format!(
                "invalid stored native_session_id: {error}"
            ))
        })?;
        let byte_size = u64::try_from(byte_size_i64).unwrap_or(0);
        total_bytes = total_bytes.saturating_add(byte_size);
        parsed.push(RowCandidate {
            key,
            byte_size,
            last_updated_at_ms,
            deleted: false,
        });
    }

    Ok((parsed, total_bytes))
}

async fn run_periodic_cleanup(store: TranscriptCacheStore, mut stop_rx: watch::Receiver<bool>) {
    let mut interval = tokio::time::interval(CLEANUP_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    interval.tick().await;

    loop {
        tokio::select! {
            biased;
            changed = stop_rx.changed() => {
                if changed.is_err() || *stop_rx.borrow() {
                    break;
                }
            }
            _ = interval.tick() => {
                if *stop_rx.borrow() {
                    break;
                }
                if let Err(error) = store.enforce_retention_and_budget(None).await {
                    tracing::warn!(%error, "periodic transcript cache cleanup failed");
                }
            }
        }
    }
}

async fn open_pool_and_schema(
    db_path: &Path,
    base_dir: &Path,
    is_default_cache_name: bool,
    owner: &StorageOpenOwner,
) -> Result<SqlitePool, TranscriptCacheError> {
    let ticket = owner.start_worker_scope()?;
    ticket.preflight_ready().await?;

    let connect_options = owner
        .connection_options()?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .pragma("secure_delete", "ON")
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                sqlx::query("PRAGMA foreign_keys = ON; PRAGMA secure_delete = ON;")
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect_with(connect_options)
        .await?;

    if let Err(error) = initialize_schema(&pool).await {
        loop {
            pool.close().await;
            if pool.size() == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        return Err(error);
    }

    harden_cache_files(db_path, base_dir, is_default_cache_name)?;
    Ok(pool)
}

fn harden_cache_files(
    db_path: &Path,
    base_dir: &Path,
    is_default_cache_name: bool,
) -> Result<(), TranscriptCacheError> {
    if is_default_cache_name {
        secure_transcript_cache_at_base(base_dir)?;
    } else {
        check_and_protect_single_path(db_path)?;
        if let Some(stem) = db_path.file_name().and_then(|n| n.to_str()) {
            for suffix in ["-journal", "-shm", "-wal"] {
                let companion = base_dir.join(format!("{stem}{suffix}"));
                check_and_protect_single_path(&companion)?;
            }
        }
    }
    Ok(())
}

fn check_and_protect_single_path(path: &Path) -> Result<(), TranscriptCacheError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
                if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                    return Err(TranscriptCacheError::App(
                        AppError::UnsafeDataDirectoryEntry,
                    ));
                }
            }
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(TranscriptCacheError::App(
                    AppError::UnsafeDataDirectoryEntry,
                ));
            }
            protect_private_file(path)?;
            Ok(())
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(TranscriptCacheError::Io(source)),
    }
}

async fn delete_session_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    key: &NativeSessionKey,
) -> Result<(), TranscriptCacheError> {
    let source_id_bytes = entity_id_to_bytes(key.source().0)?;
    sqlx::query(
        r"
        DELETE FROM transcript_sessions
        WHERE source_id = ? AND native_session_id = ?
        ",
    )
    .bind(source_id_bytes.as_slice())
    .bind(key.native_id())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn parse_session_row(
    key: NativeSessionKey,
    row: &sqlx::sqlite::SqliteRow,
) -> Result<PersistedTranscriptSession, TranscriptCacheError> {
    let active_generation_i64: i64 = row.try_get("active_generation")?;
    let active_generation = u64::try_from(active_generation_i64).map_err(|_| {
        TranscriptCacheError::CorruptDatabase("active_generation negative".to_owned())
    })?;
    let version_num: i64 = row.try_get("protocol_version")?;
    let protocol_version = match version_num {
        1 => Version::V1,
        2 => Version::V2,
        other => {
            return Err(TranscriptCacheError::CorruptDatabase(format!(
                "invalid protocol_version {other}"
            )));
        }
    };
    let digest_blob: Vec<u8> = row.try_get("content_digest")?;
    let content_digest: [u8; 32] = digest_blob.try_into().map_err(|_| {
        TranscriptCacheError::CorruptDatabase("content_digest blob length != 32".to_owned())
    })?;
    let cap_str: String = row.try_get("capability_profile")?;
    let capability_profile = CapabilityProfile::parse(&cap_str).ok_or_else(|| {
        TranscriptCacheError::CorruptDatabase(format!("invalid capability_profile {cap_str}"))
    })?;
    let rec_str: String = row.try_get("native_recovery_state")?;
    let native_recovery_state = NativeRecoveryState::parse(&rec_str).ok_or_else(|| {
        TranscriptCacheError::CorruptDatabase(format!("invalid native_recovery_state {rec_str}"))
    })?;
    let record_count_i64: i64 = row.try_get("record_count")?;
    let byte_size_i64: i64 = row.try_get("byte_size")?;
    Ok(PersistedTranscriptSession {
        key,
        active_generation,
        protocol_version,
        content_digest,
        last_updated_at_ms: row.try_get("last_updated_at_ms")?,
        last_synced_at_ms: row.try_get("last_synced_at_ms")?,
        capability_profile,
        native_recovery_state,
        record_count: u64::try_from(record_count_i64).unwrap_or(0),
        byte_size: u64::try_from(byte_size_i64).unwrap_or(0),
    })
}

async fn write_replay_frames(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    source_bytes: &[u8],
    native_id: &str,
    next_generation: i64,
    version_i64: i64,
    frames: &[(RawMessage, u64)],
) -> Result<(), TranscriptCacheError> {
    let rows = super::turns::TurnRows {
        source: source_bytes,
        native: native_id,
        generation: next_generation,
    };
    rows.clear(tx).await?;
    let mut turns = crate::domain::history_page::TurnIndexer::default();
    let version = match version_i64 {
        1 => Version::V1,
        2 => Version::V2,
        _ => {
            return Err(TranscriptCacheError::Protocol(String::from(
                "invalid replay version",
            )));
        }
    };
    for (index, (frame, conn_seq)) in frames.iter().enumerate() {
        let ordinal_i64 = i64::try_from(index + 1)
            .map_err(|_| TranscriptCacheError::Protocol("ordinal exceeds i64".to_owned()))?;
        rows.observe(tx, &mut turns, version, frame, ordinal_i64)
            .await?;
        let conn_seq_i64 = i64::try_from(*conn_seq).map_err(|_| {
            TranscriptCacheError::Protocol("connection_sequence exceeds i64".to_owned())
        })?;
        sqlx::query(
            r"
            INSERT INTO transcript_records (
                source_id,
                native_session_id,
                generation,
                ordinal,
                connection_sequence,
                raw_json
            ) VALUES (?, ?, ?, ?, ?, ?)
            ",
        )
        .bind(source_bytes)
        .bind(native_id)
        .bind(next_generation)
        .bind(ordinal_i64)
        .bind(conn_seq_i64)
        .bind(frame.bytes())
        .execute(&mut **tx)
        .await?;
    }
    rows.finish(tx, &turns).await?;
    Ok(())
}
