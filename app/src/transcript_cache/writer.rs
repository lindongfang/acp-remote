//! Asynchronous bounded batch writer for persisting live `session/update` frames without blocking live capture.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use sqlx::{Row, SqlitePool};
use tokio::sync::{Mutex as AsyncMutex, mpsc, oneshot};

use crate::{
    adapters::acp::{RawMessage, Version},
    domain::{history_page::TurnIndexer, native::NativeSessionKey},
};

use super::{
    CacheHealth, CacheRetentionClock, CapabilityProfile, NativeRecoveryState, TranscriptCacheError,
    digest::IncrementalDigest, entity_id_to_bytes,
};

const WRITER_QUEUE_CAPACITY: usize = 256;
const MAX_BATCH_COMMANDS: usize = 64;
const BATCH_COALESCE_WAIT: Duration = Duration::from_millis(25);

#[derive(Clone, Debug)]
struct LiveFrameItem {
    key: NativeSessionKey,
    protocol_version: Version,
    capability_profile: CapabilityProfile,
    connection_sequence: u64,
    raw: RawMessage,
    purge_epoch: u64,
}

enum WriterCommand {
    AppendFrame(LiveFrameItem),
    ResetKey {
        key: NativeSessionKey,
        ack: oneshot::Sender<()>,
    },
    SealPurgedKey {
        key: NativeSessionKey,
        new_epoch: u64,
        ack: oneshot::Sender<()>,
    },
    Flush(oneshot::Sender<Result<(), TranscriptCacheError>>),
    Shutdown,
}

#[derive(Default)]
pub struct WriterControl {
    purge_epochs: Mutex<HashMap<NativeSessionKey, u64>>,
    health: Mutex<Option<CacheHealth>>,
}

impl WriterControl {
    #[must_use]
    pub fn new(initial_health: CacheHealth) -> Self {
        Self {
            purge_epochs: Mutex::new(HashMap::new()),
            health: Mutex::new(Some(initial_health)),
        }
    }

    pub fn current_purge_epoch(&self, key: &NativeSessionKey) -> u64 {
        self.purge_epochs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(key)
            .copied()
            .unwrap_or(0)
    }

    pub fn bump_purge_epoch(&self, key: &NativeSessionKey) -> u64 {
        let mut guard = self
            .purge_epochs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let next = guard.get(key).copied().unwrap_or(0).saturating_add(1);
        guard.insert(key.clone(), next);
        next
    }

    pub fn health(&self) -> CacheHealth {
        self.health
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .unwrap_or(CacheHealth::Healthy)
    }

    pub fn mark_degraded(&self, reason: String) {
        let mut guard = self
            .health
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = Some(CacheHealth::DegradedWriteWarning { reason });
    }
}

/// Non-blocking handle for queueing live ACP raw frames into the isolated transcript cache.
#[derive(Clone)]
pub struct TranscriptCacheWriter {
    sender: Option<mpsc::Sender<WriterCommand>>,
    control: Arc<WriterControl>,
}

impl TranscriptCacheWriter {
    pub(crate) const fn disabled(control: Arc<WriterControl>) -> Self {
        Self {
            sender: None,
            control,
        }
    }

    /// Attempts to enqueue a live raw ACP frame without blocking live stream or queue paths.
    ///
    /// Returns `true` when accepted into the bounded writer buffer, or `false` if the cache is
    /// degraded, closed, or backpressured (in which case a non-fatal write warning is recorded).
    #[must_use]
    pub fn try_enqueue_frame(
        &self,
        key: &NativeSessionKey,
        protocol_version: Version,
        connection_sequence: u64,
        raw: RawMessage,
    ) -> bool {
        self.try_enqueue_frame_with_profile(
            key,
            protocol_version,
            CapabilityProfile::Unknown,
            connection_sequence,
            raw,
        )
    }

    /// Attempts to enqueue a live raw ACP frame along with its discovered `CapabilityProfile`
    /// without blocking live stream or queue paths.
    #[must_use]
    pub fn try_enqueue_frame_with_profile(
        &self,
        key: &NativeSessionKey,
        protocol_version: Version,
        capability_profile: CapabilityProfile,
        connection_sequence: u64,
        raw: RawMessage,
    ) -> bool {
        let Some(sender) = &self.sender else {
            return false;
        };
        let purge_epoch = self.control.current_purge_epoch(key);
        let cmd = WriterCommand::AppendFrame(LiveFrameItem {
            key: key.clone(),
            protocol_version,
            capability_profile,
            connection_sequence,
            raw,
            purge_epoch,
        });
        match sender.try_send(cmd) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.control.mark_degraded(
                    "transcript cache writer queue full; dropping frame to protect live throughput"
                        .to_owned(),
                );
                false
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        }
    }

    #[must_use]
    pub fn health(&self) -> CacheHealth {
        self.control.health()
    }

    /// Waits for all currently queued frames to flush to SQLite.
    ///
    /// # Errors
    ///
    /// Returns an error if the writer has stopped or a SQLite batch commit fails.
    pub async fn flush(&self) -> Result<(), TranscriptCacheError> {
        let Some(sender) = &self.sender else {
            return Ok(());
        };
        let (tx, rx) = oneshot::channel();
        sender
            .send(WriterCommand::Flush(tx))
            .await
            .map_err(|_| TranscriptCacheError::ShuttingDown)?;
        rx.await.map_err(|_| TranscriptCacheError::ShuttingDown)?
    }

    pub(crate) async fn seal_purged_key(&self, key: &NativeSessionKey, new_epoch: u64) {
        let Some(sender) = &self.sender else {
            return;
        };
        let (tx, rx) = oneshot::channel();
        if sender
            .send(WriterCommand::SealPurgedKey {
                key: key.clone(),
                new_epoch,
                ack: tx,
            })
            .await
            .is_ok()
        {
            let _ = rx.await;
        }
    }

    pub(crate) async fn reset_key(&self, key: &NativeSessionKey) {
        let Some(sender) = &self.sender else {
            return;
        };
        let (tx, rx) = oneshot::channel();
        if sender
            .send(WriterCommand::ResetKey {
                key: key.clone(),
                ack: tx,
            })
            .await
            .is_ok()
        {
            let _ = rx.await;
        }
    }

    pub(crate) async fn send_shutdown(&self) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(WriterCommand::Shutdown).await;
        }
    }
}

struct SessionWriterState {
    turns: TurnIndexer,
    active_generation: i64,
    next_ordinal: u64,
    record_count: u64,
    total_bytes: u64,
    digest: IncrementalDigest,
    capability_profile: CapabilityProfile,
    native_recovery_state: NativeRecoveryState,
    last_synced_at_ms: Option<i64>,
    exists_in_db: bool,
}

pub fn spawn_writer(
    pool: SqlitePool,
    write_mutex: Arc<AsyncMutex<()>>,
    clock: Arc<dyn CacheRetentionClock>,
    control: Arc<WriterControl>,
) -> (TranscriptCacheWriter, tokio::task::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel(WRITER_QUEUE_CAPACITY);
    let writer = TranscriptCacheWriter {
        sender: Some(tx),
        control: Arc::clone(&control),
    };
    let handle = tokio::spawn(run_writer_loop(rx, pool, write_mutex, clock, control));
    (writer, handle)
}

async fn run_writer_loop(
    mut rx: mpsc::Receiver<WriterCommand>,
    pool: SqlitePool,
    write_mutex: Arc<AsyncMutex<()>>,
    clock: Arc<dyn CacheRetentionClock>,
    control: Arc<WriterControl>,
) {
    let mut cache: HashMap<NativeSessionKey, SessionWriterState> = HashMap::new();
    let mut pending_frames: Vec<LiveFrameItem> = Vec::new();
    let mut flushes: Vec<oneshot::Sender<Result<(), TranscriptCacheError>>> = Vec::new();
    let mut acks: Vec<oneshot::Sender<()>> = Vec::new();

    loop {
        let Some(first) = rx.recv().await else {
            break;
        };

        let mut shutdown = false;
        handle_command(
            first,
            &mut cache,
            &mut pending_frames,
            &mut flushes,
            &mut acks,
            &mut shutdown,
        );

        if !shutdown && flushes.is_empty() && acks.is_empty() {
            let deadline = tokio::time::sleep(BATCH_COALESCE_WAIT);
            tokio::pin!(deadline);
            while pending_frames.len() < MAX_BATCH_COMMANDS {
                tokio::select! {
                    biased;
                    maybe_cmd = rx.recv() => {
                        let Some(cmd) = maybe_cmd else {
                            shutdown = true;
                            break;
                        };
                        let stop_coalescing = matches!(
                            cmd,
                            WriterCommand::Flush(_)
                                | WriterCommand::SealPurgedKey { .. }
                                | WriterCommand::Shutdown
                        );
                        handle_command(
                            cmd,
                            &mut cache,
                            &mut pending_frames,
                            &mut flushes,
                            &mut acks,
                            &mut shutdown,
                        );
                        if stop_coalescing {
                            break;
                        }
                    }
                    () = &mut deadline => {
                        break;
                    }
                }
            }
        } else {
            while pending_frames.len() < MAX_BATCH_COMMANDS {
                match rx.try_recv() {
                    Ok(cmd) => handle_command(
                        cmd,
                        &mut cache,
                        &mut pending_frames,
                        &mut flushes,
                        &mut acks,
                        &mut shutdown,
                    ),
                    Err(_) => break,
                }
            }
        }

        for ack in std::mem::take(&mut acks) {
            let _ = ack.send(());
        }

        let batch_result = if pending_frames.is_empty() {
            Ok(())
        } else {
            let frames = std::mem::take(&mut pending_frames);
            commit_frame_batch(&pool, &write_mutex, &clock, &control, &mut cache, frames).await
        };

        if let Err(ref error) = batch_result {
            cache.clear(); // Memory advanced inside the rolled-back transaction.
            tracing::warn!(%error, "transcript cache background write failed; degrading cache health");
            control.mark_degraded(error.to_string());
        }

        for tx in std::mem::take(&mut flushes) {
            let _ = tx.send(match &batch_result {
                Ok(()) => Ok(()),
                Err(err) => Err(TranscriptCacheError::Degraded(err.to_string())),
            });
        }

        if shutdown {
            break;
        }
    }
}

fn handle_command(
    cmd: WriterCommand,
    cache: &mut HashMap<NativeSessionKey, SessionWriterState>,
    pending_frames: &mut Vec<LiveFrameItem>,
    flushes: &mut Vec<oneshot::Sender<Result<(), TranscriptCacheError>>>,
    acks: &mut Vec<oneshot::Sender<()>>,
    shutdown: &mut bool,
) {
    match cmd {
        WriterCommand::AppendFrame(item) => {
            pending_frames.push(item);
        }
        WriterCommand::ResetKey { key, ack } => {
            cache.remove(&key);
            pending_frames.retain(|item| item.key != key);
            acks.push(ack);
        }
        WriterCommand::SealPurgedKey {
            key,
            new_epoch,
            ack,
        } => {
            cache.remove(&key);
            pending_frames.retain(|item| !(item.key == key && item.purge_epoch < new_epoch));
            acks.push(ack);
        }
        WriterCommand::Flush(tx) => flushes.push(tx),
        WriterCommand::Shutdown => *shutdown = true,
    }
}

async fn commit_frame_batch(
    pool: &SqlitePool,
    write_mutex: &AsyncMutex<()>,
    clock: &Arc<dyn CacheRetentionClock>,
    control: &WriterControl,
    cache: &mut HashMap<NativeSessionKey, SessionWriterState>,
    frames: Vec<LiveFrameItem>,
) -> Result<(), TranscriptCacheError> {
    let _tx_lock = write_mutex.lock().await;
    let now_ms = clock.now_millis();

    let filtered: Vec<LiveFrameItem> = frames
        .into_iter()
        .filter(|item| item.purge_epoch >= control.current_purge_epoch(&item.key))
        .collect();
    if filtered.is_empty() {
        return Ok(());
    }

    let mut tx = pool.begin().await?;
    for item in filtered {
        let latest_epoch = control.current_purge_epoch(&item.key);
        if item.purge_epoch < latest_epoch {
            cache.remove(&item.key);
            continue;
        }
        persist_live_item(&mut tx, cache, item, now_ms).await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn persist_live_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    cache: &mut HashMap<NativeSessionKey, SessionWriterState>,
    item: LiveFrameItem,
    now_ms: i64,
) -> Result<(), TranscriptCacheError> {
    if !cache.contains_key(&item.key) {
        let loaded = load_or_init_session_state(tx, &item.key, item.protocol_version).await?;
        cache.insert(item.key.clone(), loaded);
    }

    let state = cache.get_mut(&item.key).ok_or_else(|| {
        TranscriptCacheError::Degraded("missing writer state after initialization".to_owned())
    })?;
    if state.digest.version() != item.protocol_version {
        let mut rebuilt = load_or_init_session_state(tx, &item.key, item.protocol_version).await?;
        if rebuilt.exists_in_db && rebuilt.digest.version() != item.protocol_version {
            rebuilt.digest = IncrementalDigest::new(item.protocol_version);
        }
        *state = rebuilt;
    }

    if item.capability_profile != CapabilityProfile::Unknown {
        state.capability_profile = item.capability_profile;
    }
    state.digest.push_frame(&item.raw)?;
    let ordinal = state.next_ordinal;
    state.next_ordinal = state
        .next_ordinal
        .checked_add(1)
        .ok_or_else(|| TranscriptCacheError::Protocol("record ordinal overflow".to_owned()))?;
    state.record_count = state
        .record_count
        .checked_add(1)
        .ok_or_else(|| TranscriptCacheError::Protocol("record count overflow".to_owned()))?;
    let raw_bytes = item.raw.bytes();
    let byte_len = u64::try_from(raw_bytes.len())
        .map_err(|_| TranscriptCacheError::Protocol("raw frame length overflow".to_owned()))?;
    state.total_bytes = state.total_bytes.saturating_add(byte_len);

    let record_count_i64 = i64::try_from(state.record_count)
        .map_err(|_| TranscriptCacheError::Protocol("record count exceeds i64".to_owned()))?;
    let total_bytes_i64 = i64::try_from(state.total_bytes)
        .map_err(|_| TranscriptCacheError::Protocol("byte size exceeds i64".to_owned()))?;
    let ordinal_i64 = i64::try_from(ordinal)
        .map_err(|_| TranscriptCacheError::Protocol("ordinal exceeds i64".to_owned()))?;
    let conn_seq_i64 = i64::try_from(item.connection_sequence).map_err(|_| {
        TranscriptCacheError::Protocol("connection_sequence exceeds i64".to_owned())
    })?;
    let version_i64 = match item.protocol_version {
        Version::V1 => 1_i64,
        Version::V2 => 2_i64,
    };
    let digest = state.digest.finalize_snapshot();
    let source_id_bytes = entity_id_to_bytes(item.key.source().0)?;
    let source_bytes = source_id_bytes.as_slice();
    let native_id = item.key.native_id();

    upsert_live_session(
        tx,
        state,
        source_bytes,
        native_id,
        version_i64,
        &digest,
        now_ms,
        record_count_i64,
        total_bytes_i64,
    )
    .await?;

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
    .bind(state.active_generation)
    .bind(ordinal_i64)
    .bind(conn_seq_i64)
    .bind(raw_bytes)
    .execute(&mut **tx)
    .await?;
    let rows = super::turns::TurnRows {
        source: source_bytes,
        native: native_id,
        generation: state.active_generation,
    };
    rows.observe(
        tx,
        &mut state.turns,
        item.protocol_version,
        &item.raw,
        ordinal_i64,
    )
    .await?;
    rows.finish(tx, &state.turns).await?;
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "Live session upsert binds explicit SQL columns without allocating a mirror wrapper struct"
)]
async fn upsert_live_session(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    state: &mut SessionWriterState,
    source_bytes: &[u8],
    native_id: &str,
    version_i64: i64,
    digest: &[u8; 32],
    now_ms: i64,
    record_count_i64: i64,
    total_bytes_i64: i64,
) -> Result<(), TranscriptCacheError> {
    if state.exists_in_db {
        sqlx::query(
            r"
            UPDATE transcript_sessions
            SET protocol_version = ?,
                content_digest = ?,
                last_updated_at_ms = ?,
                capability_profile = ?,
                record_count = ?,
                byte_size = ?
            WHERE source_id = ? AND native_session_id = ?
            ",
        )
        .bind(version_i64)
        .bind(digest.as_slice())
        .bind(now_ms)
        .bind(state.capability_profile.as_str())
        .bind(record_count_i64)
        .bind(total_bytes_i64)
        .bind(source_bytes)
        .bind(native_id)
        .execute(&mut **tx)
        .await?;
    } else {
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
                byte_size, generation_token
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, randomblob(16))
            ",
        )
        .bind(source_bytes)
        .bind(native_id)
        .bind(state.active_generation)
        .bind(version_i64)
        .bind(digest.as_slice())
        .bind(now_ms)
        .bind(state.last_synced_at_ms)
        .bind(state.capability_profile.as_str())
        .bind(state.native_recovery_state.as_str())
        .bind(record_count_i64)
        .bind(total_bytes_i64)
        .execute(&mut **tx)
        .await?;
        state.exists_in_db = true;
    }
    Ok(())
}

async fn load_or_init_session_state(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    key: &NativeSessionKey,
    default_version: Version,
) -> Result<SessionWriterState, TranscriptCacheError> {
    let source_id_bytes = entity_id_to_bytes(key.source().0)?;
    let source_bytes = source_id_bytes.as_slice();
    let native_id = key.native_id();
    let existing = sqlx::query(
        r"
        SELECT active_generation, protocol_version, capability_profile, native_recovery_state,
               last_synced_at_ms, record_count, byte_size
        FROM transcript_sessions
        WHERE source_id = ? AND native_session_id = ?
        ",
    )
    .bind(source_bytes)
    .bind(native_id)
    .fetch_optional(&mut **tx)
    .await?;

    let Some(row) = existing else {
        return Ok(SessionWriterState {
            turns: TurnIndexer::default(),
            active_generation: 1,
            next_ordinal: 1,
            record_count: 0,
            total_bytes: 0,
            digest: IncrementalDigest::new(default_version),
            capability_profile: CapabilityProfile::Unknown,
            native_recovery_state: NativeRecoveryState::Normal,
            last_synced_at_ms: None,
            exists_in_db: false,
        });
    };

    let active_generation: i64 = row.try_get("active_generation")?;
    let version_num: i64 = row.try_get("protocol_version")?;
    let version = match version_num {
        1 => Version::V1,
        2 => Version::V2,
        other => {
            return Err(TranscriptCacheError::CorruptDatabase(format!(
                "invalid protocol_version {other}"
            )));
        }
    };
    let cap_str: String = row.try_get("capability_profile")?;
    let capability_profile = CapabilityProfile::parse(&cap_str).ok_or_else(|| {
        TranscriptCacheError::CorruptDatabase(format!("invalid capability_profile {cap_str}"))
    })?;
    let rec_str: String = row.try_get("native_recovery_state")?;
    let native_recovery_state = NativeRecoveryState::parse(&rec_str).ok_or_else(|| {
        TranscriptCacheError::CorruptDatabase(format!("invalid native_recovery_state {rec_str}"))
    })?;
    let last_synced_at_ms: Option<i64> = row.try_get("last_synced_at_ms")?;
    let stored_bytes: i64 = row.try_get("byte_size")?;
    let total_bytes = u64::try_from(stored_bytes).unwrap_or(0);

    let rows = super::turns::TurnRows {
        source: source_bytes,
        native: native_id,
        generation: active_generation,
    };
    let (turns, digest, record_count, max_ordinal) =
        restore_frame_state(tx, &rows, version).await?;
    Ok(SessionWriterState {
        turns,
        active_generation,
        next_ordinal: max_ordinal.saturating_add(1),
        record_count,
        total_bytes,
        digest,
        capability_profile,
        native_recovery_state,
        last_synced_at_ms,
        exists_in_db: true,
    })
}

async fn restore_frame_state(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    rows: &super::turns::TurnRows<'_>,
    version: Version,
) -> Result<(TurnIndexer, IncrementalDigest, u64, u64), TranscriptCacheError> {
    let record_rows = sqlx::query(
        r"
        SELECT ordinal, raw_json
        FROM transcript_records
        WHERE source_id = ? AND native_session_id = ? AND generation = ?
        ORDER BY ordinal ASC
        ",
    )
    .bind(rows.source)
    .bind(rows.native)
    .bind(rows.generation)
    .fetch_all(&mut **tx)
    .await?;

    rows.clear(tx).await?;
    let mut turns = TurnIndexer::default();
    let mut digest = IncrementalDigest::new(version);
    let mut max_ordinal: u64 = 0;
    let record_count = u64::try_from(record_rows.len()).unwrap_or(0);
    for rec in record_rows {
        let ordinal_i64: i64 = rec.try_get("ordinal")?;
        let ordinal = u64::try_from(ordinal_i64).unwrap_or(0);
        if ordinal > max_ordinal {
            max_ordinal = ordinal;
        }
        let raw_bytes: Vec<u8> = rec.try_get("raw_json")?;
        let raw = RawMessage::parse(raw_bytes).map_err(|error| {
            TranscriptCacheError::Protocol(format!("invalid cached raw frame: {error}"))
        })?;
        rows.observe(tx, &mut turns, version, &raw, ordinal_i64)
            .await?;
        digest.push_frame(&raw)?;
    }

    Ok((turns, digest, record_count, max_ordinal))
}
