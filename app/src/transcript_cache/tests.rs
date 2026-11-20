#![allow(
    clippy::too_many_lines,
    reason = "Sequential storage lifecycle acceptance tests"
)]

use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use super::*;
use crate::{
    config::{
        DEFAULT_TRANSCRIPT_CACHE_MAX_BYTES, DEFAULT_TRANSCRIPT_CACHE_RETENTION_DAYS, NodeConfig,
        TranscriptCacheConfig,
    },
    data_dir::DataDirectory,
    domain::{native::NativeSourceId, shared_projection::StructuredItemKind},
    storage::{SCHEMA_VERSION, Storage},
};

static TEST_DIR_SEQ: AtomicU64 = AtomicU64::new(1);

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(tag: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let seq = TEST_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "acp-remote-tcache-{tag}-{}-{nonce}-{seq}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        Self(dir)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sample_key(source_byte: u8, native_id: &str) -> NativeSessionKey {
    let source = NativeSourceId(crate::domain::EntityId::from_bytes([source_byte; 16]));
    NativeSessionKey::from_config(Some(source.0), native_id).expect("valid key")
}

fn raw_msg(json: &str) -> RawMessage {
    RawMessage::parse(json.as_bytes().to_vec()).expect("valid JSON frame")
}

#[derive(Default)]
struct SetProtectedSessions {
    keys: Mutex<HashSet<NativeSessionKey>>,
}

impl SetProtectedSessions {
    fn insert(&self, key: NativeSessionKey) {
        self.keys.lock().expect("lock").insert(key);
    }

    fn remove(&self, key: &NativeSessionKey) {
        self.keys.lock().expect("lock").remove(key);
    }
}

impl ProtectedSessionsPort for SetProtectedSessions {
    fn is_protected(&self, key: &NativeSessionKey) -> bool {
        self.keys.lock().expect("lock").contains(key)
    }
}

#[tokio::test]
async fn stc_001_isolated_db_schema_version_projection_and_windows_handle_cleanup() {
    let temp = TempDirGuard::new("stc-001");
    let data_dir = DataDirectory::acquire(temp.path()).expect("acquire data dir");
    let cfg = NodeConfig::parse_toml(
        r#"
        data_dir = "data-stc-001"
        listen = "127.0.0.1:8443"
        canonical_origin = "https://node.example.test"
        tls_cert = "server.pem"
        tls_key = "server-key.pem"
        "#,
    )
    .expect("default config");
    assert_eq!(
        cfg.transcript_cache().retention_days,
        DEFAULT_TRANSCRIPT_CACHE_RETENTION_DAYS
    );
    assert_eq!(
        cfg.transcript_cache().max_bytes,
        DEFAULT_TRANSCRIPT_CACHE_MAX_BYTES
    );

    let main_storage = Storage::open(&data_dir.database_path())
        .await
        .expect("open node.sqlite3");
    let main_version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(main_storage.pool_for_test())
        .await
        .expect("query user_version");
    assert_eq!(main_version, SCHEMA_VERSION);

    let clock = Arc::new(ManualRetentionClock::new(1_700_000_000_000));
    let cache = TranscriptCacheStore::open_or_degrade(
        data_dir.transcript_cache_path(),
        cfg.transcript_cache(),
        clock,
        Arc::new(NoProtectedSessions),
    )
    .await;
    assert_eq!(cache.health(), CacheHealth::Healthy);
    assert!(data_dir.transcript_cache_path().exists());

    // Ensure node.sqlite3 has no transcript_* tables.
    let main_tables: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE name LIKE 'transcript_%'")
            .fetch_one(main_storage.pool_for_test())
            .await
            .expect("count node tables");
    assert_eq!(main_tables, 0);

    // Check WAL, foreign_keys = ON, secure_delete = ON, and the current cache schema version on transcript-cache.sqlite3.
    let cache_pool = cache.pool_for_test().await.expect("cache pool open");
    let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&cache_pool)
        .await
        .expect("journal_mode");
    assert!(journal_mode.eq_ignore_ascii_case("wal"));
    let fk: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&cache_pool)
        .await
        .expect("foreign_keys");
    assert_eq!(fk, 1);
    let secure_delete: i64 = sqlx::query_scalar("PRAGMA secure_delete")
        .fetch_one(&cache_pool)
        .await
        .expect("secure_delete");
    assert_eq!(secure_delete, 1);
    let schema_ver: i64 =
        sqlx::query_scalar("SELECT schema_version FROM transcript_cache_meta WHERE id = 1")
            .fetch_one(&cache_pool)
            .await
            .expect("schema version");
    assert_eq!(schema_ver, 2);

    let key_v1 = sample_key(1, "sess-v1");
    let key_v2 = sample_key(2, "sess-v2");
    let v1_msg = raw_msg(
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"sess-v1","update":{"sessionUpdate":"tool_call","toolCallId":"tc-1","title":"V1 call"}}}"#,
    );
    let v2_msg = raw_msg(
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"sess-v2","update":{"sessionUpdate":"tool_call_content_chunk","toolCallId":"tc-2","content":{"type":"text","text":"V2 chunk"}}}}"#,
    );

    let writer = cache.writer();
    assert!(writer.try_enqueue_frame(&key_v1, Version::V1, 10, v1_msg));
    assert!(writer.try_enqueue_frame(&key_v2, Version::V2, 20, v2_msg));
    writer.flush().await.expect("flush live frames");

    let snap_v1 = cache
        .load_snapshot(&key_v1)
        .await
        .expect("load v1")
        .expect("v1 exists");
    let snap_v2 = cache
        .load_snapshot(&key_v2)
        .await
        .expect("load v2")
        .expect("v2 exists");
    assert_eq!(snap_v1.session.protocol_version, Version::V1);
    assert_eq!(snap_v1.session.active_generation, 1);
    assert_eq!(snap_v1.session.last_synced_at_ms, None);
    assert_eq!(snap_v1.session.content_digest.len(), 32);
    assert_eq!(snap_v1.records[0].connection_sequence, 10);

    assert_eq!(snap_v2.session.protocol_version, Version::V2);
    assert_eq!(snap_v2.records[0].connection_sequence, 20);

    let journal_v1 = snap_v1.project_into_journal().expect("project v1 offline");
    let journal_v2 = snap_v2.project_into_journal().expect("project v2 offline");
    assert_eq!(
        journal_v1
            .snapshot()
            .expect("snap v1")
            .item(0)
            .expect("item 0")
            .kind,
        StructuredItemKind::Tool
    );
    assert_eq!(
        journal_v2
            .snapshot()
            .expect("snap v2")
            .item(0)
            .expect("item 0")
            .kind,
        StructuredItemKind::Tool
    );

    drop(cache_pool);
    // Deterministic shutdown releases Windows SQLite file handles so remove_dir_all succeeds immediately.
    cache.shutdown().await.expect("shutdown transcript cache");
    main_storage.close().await;
    data_dir.unlock().expect("unlock");
    drop(data_dir);
    std::fs::remove_dir_all(temp.path())
        .expect("immediate Windows dir cleanup without file lock error");
}

#[tokio::test]
async fn stc_003_corrupted_db_degrades_gracefully_and_rejects_unsafe_reparse_entry() {
    let temp = TempDirGuard::new("stc-003");
    let db_path = temp.path().join("transcript-cache.sqlite3");
    std::fs::write(&db_path, b"not-a-valid-sqlite-database-header").expect("seed corrupt file");

    let clock = Arc::new(ManualRetentionClock::new(1_700_000_000_000));
    let cache = TranscriptCacheStore::open_or_degrade(
        db_path,
        TranscriptCacheConfig::default(),
        clock.clone(),
        Arc::new(NoProtectedSessions),
    )
    .await;

    assert!(matches!(
        cache.health(),
        CacheHealth::DegradedWriteWarning { .. }
    ));

    let key = sample_key(3, "sess-degraded");
    let writer = cache.writer();
    assert!(!writer.try_enqueue_frame(
        &key,
        Version::V1,
        1,
        raw_msg(r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s"}}"#),
    ));
    assert!(
        cache
            .load_snapshot(&key)
            .await
            .expect("safe miss")
            .is_none()
    );
    cache.shutdown().await.expect("shutdown degraded store");

    #[cfg(windows)]
    {
        let junction_base = TempDirGuard::new("stc-003-jbase");
        let target_dir = TempDirGuard::new("stc-003-jtarget");
        let impostor = junction_base.path().join("transcript-cache.sqlite3");
        let output = std::process::Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                &impostor.to_string_lossy(),
                &target_dir.path().to_string_lossy(),
            ])
            .output()
            .expect("mklink /J");
        assert!(output.status.success(), "mklink /J should succeed");
        let err = TranscriptCacheStore::open(
            impostor.clone(),
            TranscriptCacheConfig::default(),
            clock,
            Arc::new(NoProtectedSessions),
        )
        .await
        .err()
        .expect("reparse entry must be rejected");
        assert!(matches!(
            err,
            TranscriptCacheError::App(AppError::UnsafeDataDirectoryEntry)
        ));
        let _ = std::fs::remove_dir(&impostor);
    }
}

#[tokio::test]
async fn stc_011_identical_digest_replay_and_read_do_not_extend_30d_retention() {
    let temp = TempDirGuard::new("stc-011");
    let db_path = temp.path().join("transcript-cache.sqlite3");
    let day_ms: i64 = 24 * 60 * 60 * 1000;
    let t0 = 1_700_000_000_000_i64;
    let clock = Arc::new(ManualRetentionClock::new(t0));

    let cache = TranscriptCacheStore::open(
        db_path,
        TranscriptCacheConfig::default(),
        clock.clone(),
        Arc::new(NoProtectedSessions),
    )
    .await
    .expect("open cache");

    let key_a = sample_key(4, "sess-a");
    let key_b = sample_key(5, "sess-b");
    let compact_msg = raw_msg(
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"sess-digest","update":{"sessionUpdate":"user_message","content":{"type":"text","text":"hello"}}}}"#,
    );
    let spaced_msg = raw_msg(
        "{\n  \"jsonrpc\" : \"2.0\",\n  \"method\" : \"session/update\",\n  \"params\" : {\n    \"sessionId\" : \"sess-digest\",\n    \"update\" : {\n      \"sessionUpdate\" : \"user_message\",\n      \"content\" : { \"type\" : \"text\", \"text\" : \"hello\" }\n    }\n  }\n}",
    );

    assert_eq!(
        cache
            .replace_with_replay(
                &key_a,
                Version::V1,
                CapabilityProfile::ReplayCapable,
                vec![(compact_msg.clone(), 1)],
            )
            .await
            .expect("initial insert A"),
        ReplaceOutcome::Updated
    );
    assert_eq!(
        cache
            .replace_with_replay(
                &key_b,
                Version::V1,
                CapabilityProfile::ReplayCapable,
                vec![(compact_msg.clone(), 1)],
            )
            .await
            .expect("initial insert B"),
        ReplaceOutcome::Updated
    );

    // Day 10 & Day 25: read B + replay unchanged content (with extra formatting whitespace).
    clock.set_millis(t0 + 10 * day_ms);
    let loaded_b = cache
        .load_snapshot(&key_b)
        .await
        .expect("read B")
        .expect("B present");
    assert_eq!(loaded_b.session.last_updated_at_ms, t0);

    let outcome = cache
        .replace_with_replay(
            &key_b,
            Version::V1,
            CapabilityProfile::ReplayCapable,
            vec![(spaced_msg, 1)],
        )
        .await
        .expect("unchanged replay B");
    assert_eq!(outcome, ReplaceOutcome::UnchangedDigest);

    // Day 20: semantic update on A refreshes A's last_updated_at_ms.
    let t_a_update = t0 + 20 * day_ms;
    clock.set_millis(t_a_update);
    let new_turn = raw_msg(
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"sess-a","update":{"sessionUpdate":"agent_message","content":{"type":"text","text":"new reply"}}}}"#,
    );
    assert_eq!(
        cache
            .replace_with_replay(
                &key_a,
                Version::V1,
                CapabilityProfile::ReplayCapable,
                vec![(compact_msg, 1), (new_turn, 2)],
            )
            .await
            .expect("semantic update A"),
        ReplaceOutcome::Updated
    );
    let updated_a = cache
        .load_snapshot(&key_a)
        .await
        .expect("read A")
        .expect("A present");
    assert_eq!(updated_a.session.last_updated_at_ms, t_a_update);
    assert_eq!(updated_a.session.active_generation, 2);

    // Day 25: another unchanged replay on B only moves last_synced_at_ms, not last_updated_at_ms.
    let t_b_sync = t0 + 25 * day_ms;
    clock.set_millis(t_b_sync);
    let after_same_replay = cache
        .load_snapshot(&key_b)
        .await
        .expect("read B2")
        .expect("B present");
    assert_eq!(
        after_same_replay.session.last_synced_at_ms,
        Some(t0 + 10 * day_ms)
    );
    assert_eq!(after_same_replay.session.last_updated_at_ms, t0);

    // Day 31: B is past 30 days without semantic update and MUST expire, while A remains.
    clock.set_millis(t0 + 31 * day_ms);
    let report = cache
        .enforce_retention_and_budget(None)
        .await
        .expect("enforce retention");
    assert_eq!(report.expired_sessions, 1);
    assert_eq!(report.remaining_sessions, 1);
    assert!(cache.load_snapshot(&key_b).await.expect("B gone").is_none());
    assert!(
        cache
            .load_snapshot(&key_a)
            .await
            .expect("A preserved")
            .is_some()
    );

    cache.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn stc_012_lru_budget_evicts_oldest_whole_unprotected_session() {
    let temp = TempDirGuard::new("stc-012");
    let db_path = temp.path().join("transcript-cache.sqlite3");
    let clock = Arc::new(ManualRetentionClock::new(1_000_000));
    let protected = Arc::new(SetProtectedSessions::default());

    let frame = raw_msg(
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"user_message","content":{"type":"text","text":"payload-0123456789"}}}}"#,
    );
    let one_frame_bytes = frame.bytes().len() as u64;
    let config = TranscriptCacheConfig::new(30, one_frame_bytes * 3).expect("valid config");

    let cache = TranscriptCacheStore::open(db_path, config, clock.clone(), protected.clone())
        .await
        .expect("open cache");

    let key_x = sample_key(10, "old-protected-x");
    let key_z = sample_key(11, "lease-protected-z");
    let key_y = sample_key(12, "old-unprotected-y");
    let key_w = sample_key(13, "newest-w");
    let key_v = sample_key(14, "overflow-v");

    protected.insert(key_x.clone());
    protected.insert(key_z.clone());

    cache
        .replace_with_replay(
            &key_x,
            Version::V1,
            CapabilityProfile::ReplayCapable,
            vec![(frame.clone(), 1)],
        )
        .await
        .expect("insert X");
    clock.set_millis(1_002_000);
    cache
        .replace_with_replay(
            &key_z,
            Version::V1,
            CapabilityProfile::ReplayCapable,
            vec![(frame.clone(), 1)],
        )
        .await
        .expect("insert Z");
    clock.set_millis(1_003_000);
    cache
        .replace_with_replay(
            &key_y,
            Version::V1,
            CapabilityProfile::ReplayCapable,
            vec![(frame.clone(), 1)],
        )
        .await
        .expect("insert Y");
    clock.set_millis(1_004_000);
    cache
        .replace_with_replay(
            &key_w,
            Version::V1,
            CapabilityProfile::ReplayCapable,
            vec![(frame.clone(), 1)],
        )
        .await
        .expect("insert W");

    // 4 frames stored, max = 3 frames: protected X and Z survive, oldest unprotected Y is evicted, newest W stays.
    let report = cache
        .enforce_retention_and_budget(None)
        .await
        .expect("lru cleanup 1");
    assert_eq!(report.lru_evicted_sessions, 1);
    assert_eq!(report.remaining_sessions, 3);
    assert!(cache.load_snapshot(&key_x).await.expect("X").is_some());
    assert!(cache.load_snapshot(&key_z).await.expect("Z").is_some());
    assert!(cache.load_snapshot(&key_y).await.expect("Y").is_none());
    assert!(cache.load_snapshot(&key_w).await.expect("W").is_some());

    // Release Z's active read lease and insert V -> Z is now eligible and evicted while X (slot active) & W & V stay.
    protected.remove(&key_z);
    clock.set_millis(1_005_000);
    cache
        .replace_with_replay(
            &key_v,
            Version::V1,
            CapabilityProfile::ReplayCapable,
            vec![(frame, 1)],
        )
        .await
        .expect("insert V");
    let report2 = cache
        .enforce_retention_and_budget(None)
        .await
        .expect("lru cleanup 2");
    assert_eq!(report2.lru_evicted_sessions, 1);
    assert_eq!(report2.remaining_sessions, 3);
    assert!(cache.load_snapshot(&key_x).await.expect("X2").is_some());
    assert!(cache.load_snapshot(&key_z).await.expect("Z2").is_none());
    assert!(cache.load_snapshot(&key_w).await.expect("W2").is_some());
    assert!(cache.load_snapshot(&key_v).await.expect("V2").is_some());

    // Ensure foreign-key cascade left zero orphaned transcript_records.
    let pool = cache.pool_for_test().await.expect("pool");
    let rec_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transcript_records")
        .fetch_one(&pool)
        .await
        .expect("count records");
    assert_eq!(rec_count, 3);
    drop(pool);

    cache.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn stc_013_purge_deleted_native_session_wipes_sqlite_wal_and_seals_writer() {
    let temp = TempDirGuard::new("stc-013");
    let db_path = temp.path().join("transcript-cache.sqlite3");
    let wal_path = temp.path().join("transcript-cache.sqlite3-wal");
    let clock = Arc::new(ManualRetentionClock::new(1_700_000_000_000));

    let cache = TranscriptCacheStore::open(
        db_path.clone(),
        TranscriptCacheConfig::default(),
        clock,
        Arc::new(NoProtectedSessions),
    )
    .await
    .expect("open cache");

    let secret_marker = "UNIQUE_TRANSCRIPT_SECRET_MARKER_9F8A7B";
    let key = sample_key(20, "to-delete");
    let writer = cache.writer();
    let msg = raw_msg(&format!(
        r#"{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"to-delete","update":{{"sessionUpdate":"user_message","content":{{"type":"text","text":"{secret_marker}"}}}}}}}}"#
    ));
    assert!(writer.try_enqueue_frame(&key, Version::V1, 1, msg.clone()));
    writer.flush().await.expect("flush secret frame");

    // Re-queue one buffered frame concurrently right before purge to prove writer sealing.
    let _ = writer.try_enqueue_frame(&key, Version::V1, 2, msg);
    cache
        .purge_deleted_native_session(&key)
        .await
        .expect("purge session");
    writer.flush().await.expect("flush after purge");

    assert!(cache.load_snapshot(&key).await.expect("query").is_none());

    cache.shutdown().await.expect("shutdown");

    let db_bytes = std::fs::read(&db_path).expect("read sqlite file");
    assert!(
        !db_bytes
            .windows(secret_marker.len())
            .any(|w| w == secret_marker.as_bytes()),
        "deleted native transcript plaintext must not remain in transcript-cache.sqlite3"
    );
    if wal_path.exists() {
        let wal_bytes = std::fs::read(&wal_path).expect("read wal file");
        assert!(
            !wal_bytes
                .windows(secret_marker.len())
                .any(|w| w == secret_marker.as_bytes()),
            "deleted native transcript plaintext must not remain in transcript-cache.sqlite3-wal"
        );
    }
}

#[tokio::test]
async fn history_page_read_indexes_pending_rows_without_renewing_content() {
    use crate::domain::history_page::{Direction, PageSize, TurnRange};
    let temp = TempDirGuard::new("history-index");
    let clock = Arc::new(ManualRetentionClock::new(1_000_000));
    let cache = TranscriptCacheStore::open(
        temp.path().join("transcript-cache.sqlite3"),
        TranscriptCacheConfig::new(30, 1_000_000).unwrap(),
        clock.clone(),
        Arc::new(NoProtectedSessions),
    )
    .await
    .unwrap();
    let key = sample_key(44, "s");
    let mut frames = Vec::new();
    for turn in 1..=25 {
        let raw = serde_json::json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"user_message_chunk","messageId":turn.to_string(),"content":{"type":"text","text":"turn"}}}});
        frames.push((raw_msg(&raw.to_string()), turn));
    }
    cache
        .replace_with_replay(
            &key,
            Version::V1,
            CapabilityProfile::ReplayCapable,
            frames.clone(),
        )
        .await
        .unwrap();
    let pool = cache.pool_for_test().await.unwrap();
    sqlx::query("DELETE FROM transcript_turns")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE transcript_sessions SET turn_state='pending', turn_count=0")
        .execute(&pool)
        .await
        .unwrap();
    clock.set_millis(1_001_000);
    let latest = cache
        .read_turn_page(&key, |_, total| {
            TurnRange::first(total, PageSize::default(), false).map(Some)
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(latest.total, 25);
    assert_eq!(
        latest.boundaries,
        (16..=25).map(|i| (i, i)).collect::<Vec<_>>()
    );
    assert_eq!(latest.snapshot.records.len(), 10);
    assert_eq!(latest.snapshot.session.last_updated_at_ms, 1_000_000);
    let previous = cache
        .read_turn_page(&key, |_, total| {
            TurnRange::locate(total, PageSize::default(), Direction::Older, 15).map(Some)
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        previous.boundaries,
        (6..=15).map(|i| (i, i)).collect::<Vec<_>>()
    );
    assert_eq!(previous.generation, latest.generation);
    frames.pop();
    cache
        .replace_with_replay(&key, Version::V1, CapabilityProfile::ReplayCapable, frames)
        .await
        .unwrap();
    let changed = cache
        .read_turn_page(&key, |_, total| {
            TurnRange::first(total, PageSize::default(), true).map(Some)
        })
        .await
        .unwrap()
        .unwrap();
    assert_ne!(changed.generation, latest.generation);
    assert_eq!(changed.total, 24);
    drop(pool);
    cache.shutdown().await.unwrap();
}
