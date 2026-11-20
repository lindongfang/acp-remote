#![allow(clippy::too_many_lines)]

//! Public-API acceptance tests (`STC-002`, `STC-003`, `STC-004`, `STC-006`, `STC-014`, `STC-015`)
//! for the isolated `transcript-cache.sqlite3` store, ACL hardening, Windows file cleanup,
//! non-blocking fault degradation, 30-day sliding retention, global byte LRU, and idle policy.

use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use acp_remote::{
    adapters::acp::{RawMessage, Version},
    config::{
        DEFAULT_TRANSCRIPT_CACHE_MAX_BYTES, DEFAULT_TRANSCRIPT_CACHE_RETENTION_DAYS, NodeConfig,
        TranscriptCacheConfig,
    },
    data_dir::DataDirectory,
    domain::{
        EntityId,
        native::NativeSessionKey,
        session_capacity::{
            CapacityIdlePolicy, Coverage, DEFAULT_IDLE_TIMEOUT, DEFAULT_SESSION_LIMIT,
            RecycleEligibility,
        },
    },
    error::AppError,
    transcript_cache::{
        CacheHealth, CapabilityProfile, ManualRetentionClock, NativeRecoveryState,
        NoProtectedSessions, ProtectedSessionsPort, ReplaceOutcome, TranscriptCacheCoordinator,
        TranscriptCacheStore,
    },
};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use tokio::runtime::Handle;

static SEQ: AtomicU64 = AtomicU64::new(1);

fn temp_test_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "acp-remote-{prefix}-{}-{nanos}-{seq}",
        std::process::id()
    ))
}

fn make_session_key(source_byte: u8, native_id: &str) -> NativeSessionKey {
    NativeSessionKey::from_config(Some(EntityId::from_bytes([source_byte; 16])), native_id)
        .expect("valid native session key")
}

fn make_raw_update(session_id: &str, message_id: &str, text: &str) -> RawMessage {
    let json = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": session_id,
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "messageId": message_id,
                "content": { "type": "text", "text": text }
            }
        }
    });
    RawMessage::parse(serde_json::to_vec(&json).expect("serialize jsonrpc update"))
        .expect("valid RawMessage")
}

const fn eligibility_safe() -> RecycleEligibility {
    RecycleEligibility {
        foreground: Coverage::Idle,
        background: Coverage::Idle,
        children: Coverage::Idle,
        pending_approvals: Some(0),
        root_verified: true,
        management_reconciled: true,
        close_supported: true,
    }
}

struct SetProtected(HashSet<NativeSessionKey>);

impl ProtectedSessionsPort for SetProtected {
    fn is_protected(&self, key: &NativeSessionKey) -> bool {
        self.0.contains(key)
    }
}

#[test]
fn stc_002_config_parses_transcript_cache_defaults_and_rejects_invalid() {
    let base = r#"
data_dir = "D:\\temp\\acp-data"
listen = "127.0.0.1:8443"
canonical_origin = "https://localhost:8443"
tls_cert = "D:\\temp\\cert.pem"
tls_key = "D:\\temp\\key.pem"
"#;
    let default_cfg = NodeConfig::parse_toml(base).expect("default config without section");
    let defaults = default_cfg.transcript_cache();
    assert_eq!(
        defaults.retention_days,
        DEFAULT_TRANSCRIPT_CACHE_RETENTION_DAYS
    );
    assert_eq!(defaults.retention_days, 30);
    assert_eq!(defaults.max_bytes, DEFAULT_TRANSCRIPT_CACHE_MAX_BYTES);
    assert_eq!(defaults.max_bytes, 10 * 1024 * 1024 * 1024);

    let custom_toml = format!("{base}\n[transcript_cache]\nretention_days = 7\nmax_bytes = 4096\n");
    let custom_cfg = NodeConfig::parse_toml(&custom_toml).expect("valid custom [transcript_cache]");
    assert_eq!(custom_cfg.transcript_cache().retention_days, 7);
    assert_eq!(custom_cfg.transcript_cache().max_bytes, 4096);

    for invalid_section in [
        "[transcript_cache]\nretention_days = 0\n",
        "[transcript_cache]\nretention_days = -3\n",
        "[transcript_cache]\nmax_bytes = 0\n",
        "[transcript_cache]\nmax_bytes = -1\n",
        "[transcript_cache]\nretention_days = 9223372036854775807\n",
        "[transcript_cache]\nunknown_key = true\n",
    ] {
        let bad_toml = format!("{base}\n{invalid_section}");
        assert!(
            matches!(NodeConfig::parse_toml(&bad_toml), Err(AppError::Config(_))),
            "expected AppError::Config for invalid section: {invalid_section}"
        );
    }
}

#[tokio::test]
async fn stc_003_isolated_storage_acl_v1_v2_offline_projection_and_windows_shutdown() {
    let dir_path = temp_test_dir("stc003");
    let data_dir = DataDirectory::acquire(&dir_path).expect("acquire DataDirectory");
    let cache_path = data_dir.transcript_cache_path();
    assert_eq!(cache_path, data_dir.path().join("transcript-cache.sqlite3"));
    assert_ne!(cache_path, data_dir.database_path());

    let clock = Arc::new(ManualRetentionClock::new(1_700_000_000_000));
    let store = TranscriptCacheStore::open(
        cache_path.clone(),
        TranscriptCacheConfig::default(),
        clock.clone(),
        Arc::new(NoProtectedSessions),
    )
    .await
    .expect("open transcript-cache.sqlite3");
    assert_eq!(store.health(), CacheHealth::Healthy);

    data_dir
        .secure_transcript_cache_files()
        .expect("harden ACLs on transcript cache files");

    #[cfg(windows)]
    {
        let output = std::process::Command::new("icacls")
            .arg(&cache_path)
            .output()
            .expect("invoke icacls on Windows");
        assert!(output.status.success(), "icacls should succeed");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.contains("Everyone:") && !stdout.contains("BUILTIN\\Users:"),
            "transcript-cache.sqlite3 ACL must remain owner-only, got:\n{stdout}"
        );
    }

    let key_v1 = make_session_key(1, "native-v1");
    let key_v2 = make_session_key(1, "native-v2");
    let frame_v1 = make_raw_update("native-v1", "msg-v1-1", "hello from v1");
    let frame_v2 = make_raw_update("native-v2", "msg-v2-1", "hello from v2");

    let replace_v1 = store
        .replace_with_replay(
            &key_v1,
            Version::V1,
            CapabilityProfile::ReplayCapable,
            vec![(frame_v1.clone(), 10)],
        )
        .await
        .expect("write v1 replay");
    assert_eq!(replace_v1, ReplaceOutcome::Updated);

    assert!(store.writer().try_enqueue_frame_with_profile(
        &key_v2,
        Version::V2,
        CapabilityProfile::ResumeOnly,
        20,
        frame_v2.clone(),
    ));
    store.writer().flush().await.expect("flush v2 writer frame");

    let snap_v1 = store
        .load_snapshot(&key_v1)
        .await
        .expect("load v1")
        .expect("v1 exists");
    assert_eq!(snap_v1.session.protocol_version, Version::V1);
    assert_eq!(
        snap_v1.session.capability_profile,
        CapabilityProfile::ReplayCapable
    );
    assert_eq!(snap_v1.records.len(), 1);
    assert_eq!(snap_v1.records[0].raw.bytes(), frame_v1.bytes());

    let snap_v2 = store
        .load_snapshot(&key_v2)
        .await
        .expect("load v2")
        .expect("v2 exists");
    assert_eq!(snap_v2.session.protocol_version, Version::V2);
    assert_eq!(
        snap_v2.session.capability_profile,
        CapabilityProfile::ResumeOnly
    );
    assert_eq!(snap_v2.records.len(), 1);
    assert_eq!(snap_v2.records[0].raw.bytes(), frame_v2.bytes());

    assert_eq!(
        snap_v1.session.native_recovery_state,
        NativeRecoveryState::Normal
    );
    assert_eq!(snap_v1.frames().len(), 1);
    let _journal_v1 = snap_v1
        .project_into_journal()
        .expect("offline projection for V1");

    assert_eq!(
        snap_v2.session.native_recovery_state,
        NativeRecoveryState::Normal
    );
    assert_eq!(snap_v2.frames().len(), 1);
    let _journal_v2 = snap_v2
        .project_into_journal()
        .expect("offline projection for V2");

    store.shutdown().await.expect("shutdown store cleanly");
    drop(store);
    drop(data_dir);

    std::fs::remove_dir_all(&dir_path)
        .expect("Windows immediate remove_dir_all after store shutdown");

    // Impostor directory or junction at transcript-cache.sqlite3 must fail closed.
    let bad_dir = temp_test_dir("stc003-impostor");
    std::fs::create_dir_all(bad_dir.join("transcript-cache.sqlite3")).expect("impostor dir");
    assert!(matches!(
        DataDirectory::acquire(&bad_dir),
        Err(AppError::UnsafeDataDirectoryEntry)
    ));
    std::fs::remove_dir_all(&bad_dir).expect("cleanup bad_dir");
}

#[tokio::test]
async fn stc_004_corrupt_header_and_runtime_write_fault_degrade_without_blocking() {
    let corrupt_dir = temp_test_dir("stc004-corrupt");
    std::fs::create_dir_all(&corrupt_dir).expect("create corrupt dir");
    let corrupt_db = corrupt_dir.join("transcript-cache.sqlite3");
    std::fs::write(&corrupt_db, b"NOT-A-VALID-SQLITE-HEADER!!").expect("corrupt db header");

    let clock = Arc::new(ManualRetentionClock::new(1_700_000_000_000));
    let degraded = TranscriptCacheStore::open_or_degrade(
        corrupt_db,
        TranscriptCacheConfig::default(),
        clock.clone(),
        Arc::new(NoProtectedSessions),
    )
    .await;
    assert!(
        matches!(degraded.health(), CacheHealth::DegradedWriteWarning { .. }),
        "corrupt header must degrade store instead of failing startup"
    );
    let key = make_session_key(2, "missing");
    assert!(
        degraded
            .load_snapshot(&key)
            .await
            .expect("load returns Ok(None)")
            .is_none()
    );
    degraded.shutdown().await.expect("shutdown degraded store");
    drop(degraded);
    std::fs::remove_dir_all(&corrupt_dir).expect("cleanup corrupt_dir");

    // Runtime SQLite write failure via BEFORE INSERT trigger must mark health DegradedWriteWarning
    // while coordinator/writer never panics or blocks callers.
    let trigger_dir = temp_test_dir("stc004-trigger");
    std::fs::create_dir_all(&trigger_dir).expect("create trigger dir");
    let db_path = trigger_dir.join("transcript-cache.sqlite3");
    let store = Arc::new(
        TranscriptCacheStore::open(
            db_path.clone(),
            TranscriptCacheConfig::default(),
            clock,
            Arc::new(NoProtectedSessions),
        )
        .await
        .expect("open healthy store"),
    );
    let coordinator = TranscriptCacheCoordinator::new(&store, Handle::current());

    let ext_pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db_path))
        .await
        .expect("connect external pool for fault injection");
    sqlx::query(
        r"
        CREATE TRIGGER fault_on_record_insert
        BEFORE INSERT ON transcript_records
        BEGIN
            SELECT RAISE(ABORT, 'injected write fault');
        END;
        ",
    )
    .execute(&ext_pool)
    .await
    .expect("install write fault trigger");
    ext_pool.close().await;

    assert!(store.writer().try_enqueue_frame(
        &key,
        Version::V1,
        1,
        make_raw_update("missing", "m1", "live chunk during fault"),
    ));
    let flush_result = store.writer().flush().await;
    assert!(flush_result.is_err(), "flush reports sqlite abort error");
    assert!(
        matches!(store.health(), CacheHealth::DegradedWriteWarning { .. }),
        "writer fault must surface DegradedWriteWarning"
    );

    store.shutdown().await.expect("shutdown after fault");
    drop(coordinator);
    drop(store);
    std::fs::remove_dir_all(&trigger_dir).expect("cleanup trigger_dir");
}

#[tokio::test]
async fn stc_006_and_stc_015_retention_lru_protection_and_idle_policy() {
    let dir = temp_test_dir("stc015-retention");
    std::fs::create_dir_all(&dir).expect("create retention dir");
    let db_path = dir.join("transcript-cache.sqlite3");
    let start_ms = 1_700_000_000_000_i64;
    let clock = Arc::new(ManualRetentionClock::new(start_ms));

    let k1 = make_session_key(3, "s1-protected-old");
    let k2 = make_session_key(3, "s2-unprotected-old");
    let k3 = make_session_key(3, "s3-unprotected-new");
    let f1 = make_raw_update("s1-protected-old", "m1", "alpha-text");
    let f2 = make_raw_update("s2-unprotected-old", "m2", "bravo-text-padding-for-size");
    let f3 = make_raw_update("s3-unprotected-new", "m3", "charlie-text");

    // Cap byte budget tightly so s3 + s1 slightly exceeds budget after k2 expires.
    let single_bytes = u64::try_from(f1.bytes().len() + f3.bytes().len()).expect("len");
    let max_bytes = single_bytes.saturating_sub(1);
    let store = TranscriptCacheStore::open(
        db_path,
        TranscriptCacheConfig::new(30, max_bytes).expect("valid config"),
        clock.clone(),
        Arc::new(NoProtectedSessions),
    )
    .await
    .expect("open store");

    store
        .replace_with_replay(
            &k1,
            Version::V1,
            CapabilityProfile::ReplayCapable,
            vec![(f1.clone(), 1)],
        )
        .await
        .expect("write k1");
    clock.advance_millis(5_000);
    store
        .replace_with_replay(
            &k2,
            Version::V1,
            CapabilityProfile::ReplayCapable,
            vec![(f2.clone(), 1)],
        )
        .await
        .expect("write k2");
    clock.advance_millis(10 * 86_400_000);
    store
        .replace_with_replay(
            &k3,
            Version::V1,
            CapabilityProfile::ReplayCapable,
            vec![(f3.clone(), 1)],
        )
        .await
        .expect("write k3");

    // Cold read + unchanged digest replay at day 25 must NOT slide `last_updated_at_ms`.
    clock.set_millis(start_ms + 25 * 86_400_000);
    let cold_k1 = store
        .load_snapshot(&k1)
        .await
        .expect("load k1")
        .expect("k1 present");
    assert_eq!(cold_k1.session.last_updated_at_ms, start_ms);
    let unchanged = store
        .replace_with_replay(
            &k1,
            Version::V1,
            CapabilityProfile::ReplayCapable,
            vec![(f1, 1)],
        )
        .await
        .expect("unchanged digest replay");
    assert_eq!(unchanged, ReplaceOutcome::UnchangedDigest);
    let after_unchanged = store
        .load_snapshot(&k1)
        .await
        .expect("reload k1")
        .expect("k1");
    assert_eq!(after_unchanged.session.last_updated_at_ms, start_ms);
    assert_eq!(
        after_unchanged.session.last_synced_at_ms,
        Some(start_ms + 25 * 86_400_000)
    );

    // Advance past 30 days since k1/k2 creation (while k3 is only 20 days old).
    // Protect k1, leave k2/k3 unprotected -> k2 expires; k1 is preserved despite expiry;
    // then remaining (k1 + k3) exceeds `max_bytes`, so unprotected k3 is LRU evicted while protected k1 survives.
    clock.set_millis(start_ms + 30 * 86_400_000 + 60_000);
    let mut protected_set = HashSet::new();
    protected_set.insert(k1.clone());
    let protector = SetProtected(protected_set);
    let report = store
        .enforce_retention_and_budget(Some(&protector))
        .await
        .expect("enforce retention and budget");
    assert_eq!(report.expired_sessions, 1, "only unprotected k2 expires");
    assert_eq!(
        report.lru_evicted_sessions, 1,
        "unprotected k3 evicted to satisfy byte budget"
    );
    assert_eq!(report.remaining_sessions, 1);
    assert!(
        store
            .load_snapshot(&k1)
            .await
            .expect("k1 survives")
            .is_some()
    );
    assert!(store.load_snapshot(&k2).await.expect("k2 gone").is_none());
    assert!(store.load_snapshot(&k3).await.expect("k3 gone").is_none());

    store.shutdown().await.expect("shutdown store");
    drop(store);
    std::fs::remove_dir_all(&dir).expect("cleanup retention dir");

    // Verify CapacityIdlePolicy 29:59 vs 30:00 and replay activity semantics.
    assert_eq!(DEFAULT_SESSION_LIMIT, 50);
    assert_eq!(DEFAULT_IDLE_TIMEOUT, Duration::from_mins(30));

    let t0 = Instant::now();
    let mut policy = CapacityIdlePolicy::new(t0);
    policy.observe_safety(t0, eligibility_safe()).expect("safe");
    assert!(!policy.idle_timeout_reached(t0 + Duration::from_secs(1799)));
    assert!(policy.idle_timeout_reached(t0 + Duration::from_mins(30)));

    // Actual received replay refreshes `last_activity` without clearing `safe_since`,
    // so it stays a valid safe candidate under capacity pressure while pushing timeout by 30m.
    policy
        .received_activity(t0 + Duration::from_mins(29))
        .expect("replay activity");
    assert_eq!(policy.candidate_since(), Some(t0 + Duration::from_mins(29)));
    assert!(!policy.idle_timeout_reached(t0 + Duration::from_mins(30)));
    assert!(!policy.idle_timeout_reached(t0 + Duration::from_secs(58 * 60 + 59)));
    assert!(policy.idle_timeout_reached(t0 + Duration::from_mins(59)));
}
