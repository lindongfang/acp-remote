//! Real WAL competition through the original revocation transaction, without injected SQL errors.
use super::*;
use sqlx::{Connection, SqliteConnection};
use std::{
    fs::{self, OpenOptions},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use tokio::sync::{oneshot, watch};

const WATCHDOG: Duration = Duration::from_secs(15);
const OBSERVATION: Duration = Duration::from_millis(200);
const MARKER: &str = "root-node-revoke-repair-owned-fixture-v1";
type CaseResult<T> = Result<T, &'static str>;

#[derive(Default)]
struct Events {
    next: AtomicU64,
    entries: Mutex<Vec<(&'static str, u64)>>,
}
impl Events {
    fn mark(&self, label: &'static str) -> u64 {
        let sequence = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((label, sequence));
        sequence
    }

    fn print(&self) -> CaseResult<()> {
        println!(
            "NRV_EVENTS {}",
            serde_json::to_string(&*self.entries.lock().unwrap_or_else(PoisonError::into_inner))
                .map_err(|_| "event_encoding")?
        );
        Ok(())
    }
}

#[derive(Default, serde::Serialize)]
struct WriterReport {
    begin_sequence: u64,
    updated_rows: u64,
    acquired_sequence: u64,
    observed_done_sequence: u64,
    committed: bool,
    rolled_back: bool,
    closed: bool,
    failure: Option<&'static str>,
}
impl WriterReport {
    fn complete(&self) -> bool {
        self.begin_sequence > 0
            && self.updated_rows == 1
            && self.acquired_sequence > 0
            && self.observed_done_sequence > self.acquired_sequence
            && self.committed
            && self.closed
            && self.failure.is_none()
    }
}

async fn actual_writer(
    mut connection: SqliteConnection,
    acquired: oneshot::Sender<()>,
    mut done: watch::Receiver<bool>,
    events: Arc<Events>,
) -> WriterReport {
    let mut report = WriterReport::default();
    let mut notification = Some(acquired);
    match connection.begin_with("BEGIN IMMEDIATE").await {
        Ok(mut transaction) => {
            report.begin_sequence = events.mark("writer_begin_ack");
            match sqlx::query("UPDATE node_state SET revision=revision WHERE singleton=1")
                .execute(&mut *transaction)
                .await
            {
                Ok(result) if result.rows_affected() == 1 => {
                    report.updated_rows = result.rows_affected();
                    report.acquired_sequence = events.mark("writer_update_ack_and_held");
                    if let Some(sender) = notification.take() {
                        let _ = sender.send(());
                    }
                    let mut available = true;
                    while !*done.borrow_and_update() {
                        if done.changed().await.is_err() {
                            available = false;
                            break;
                        }
                    }
                    if available {
                        report.observed_done_sequence = events.mark("writer_observed_done");
                        report.committed = transaction.commit().await.is_ok();
                        events.mark("writer_commit_returned");
                        if !report.committed {
                            report.failure = Some("writer_commit");
                        }
                    } else {
                        report.rolled_back = transaction.rollback().await.is_ok();
                        report.failure = Some("done_channel_closed");
                    }
                }
                _ => {
                    report.rolled_back = transaction.rollback().await.is_ok();
                    report.failure = Some("writer_update");
                }
            }
        }
        Err(_) => report.failure = Some("writer_begin"),
    }
    drop(notification);
    report.closed = connection.close().await.is_ok();
    events.mark("writer_close_returned");
    report
}

#[derive(serde::Serialize)]
struct Images {
    target: (String, i64, Option<i64>),
    unrelated: (String, i64, Option<i64>),
    revision: i64,
    command_count: i64,
    audit_count: i64,
    expired_count: i64,
}

struct FixtureRoots {
    root: PathBuf,
    directory: PathBuf,
    database: PathBuf,
}
struct Fixture {
    roots: Arc<FixtureRoots>,
    scope: crate::sqlite_fixture_scope::SqliteFixtureScope,
    raw_scope: crate::sqlite_fixture_scope::SqliteFixtureScope,
    storage: crate::sqlite_fixture_scope::FixtureStorage,
    trust: TrustStore,
    target: String,
    unrelated: String,
    request: String,
    expired_request: String,
    connection: Option<SqliteConnection>,
    same_database: bool,
}

impl std::ops::Deref for Fixture {
    type Target = FixtureRoots;
    fn deref(&self) -> &FixtureRoots {
        &self.roots
    }
}
impl Fixture {
    async fn new(cancel: bool) -> CaseResult<Self> {
        let root = std::env::temp_dir()
            .canonicalize()
            .map_err(|_| "temp_root")?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "fixture_clock")?
            .as_nanos();
        let label = if cancel { "cancel" } else { "success" };
        let directory = root.join(format!(
            "acp-revoke-upgrade-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).map_err(|_| "create_fixture")?;
        fs::write(directory.join("owner.txt"), MARKER).map_err(|_| "fixture_marker")?;
        let directory = directory.canonicalize().map_err(|_| "fixture_canonical")?;
        if directory.parent() != Some(root.as_path()) {
            return Err("fixture_outside_owned_root");
        }
        let database = directory.join("node.sqlite3");
        let roots = Arc::new(FixtureRoots {
            root,
            directory,
            database,
        });
        let scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
            &roots.database,
            roots.clone(),
        );
        let raw_scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
            &roots.database,
            roots.clone(),
        );
        let storage = scope.open().await.map_err(|_| "open_storage")?;
        let mut trust = TrustStore::new(storage.request_registry());
        trust.clock = Arc::new(TestClock::new());
        let mut fixture = Self {
            roots,
            scope,
            raw_scope,
            storage,
            trust,
            target: Id128::new([31; 16]).to_base64url(),
            unrelated: Id128::new([32; 16]).to_base64url(),
            request: Id128::new([52; 16]).to_base64url(),
            expired_request: Id128::new([53; 16]).to_base64url(),
            connection: None,
            same_database: false,
        };
        if let Err(stage) = fixture.seed().await {
            if let Some(connection) = fixture.connection.take() {
                let _ = connection.close().await;
            }
            fixture.raw_scope.stop_and_observe().await;
            fixture.storage.close().await;
            fixture.scope.stop_and_observe().await;
            eprintln!(
                "NRV_SETUP_FAILED stage={stage}; owned fixture retained, physical tail unconfirmed"
            );
            return Err(stage);
        }
        Ok(fixture)
    }

    async fn seed(&mut self) -> CaseResult<()> {
        let key = p256::ecdsa::SigningKey::from_slice(&[7; 32]).map_err(|_| "node_key")?;
        let public_key = key.verifying_key().to_sec1_point(false).as_bytes().to_vec();
        self.storage
            .write_node_meta(&crate::storage::NodeMeta {
                schema_version: crate::storage::SCHEMA_VERSION,
                node_id: hex::encode(Sha256::digest(&public_key)),
                public_key,
                key_file_ref: "synthetic-trust-fixture.ref".into(),
            })
            .await
            .map_err(|_| "fixture_metadata")?;
        for number in [31_u8, 32, 33] {
            let key =
                p256::ecdsa::SigningKey::from_slice(&[number; 32]).map_err(|_| "client_key")?;
            sqlx::query("INSERT INTO clients(client_id,public_key,display_name,status,auth_generation,created_at,revoked_at) VALUES(?,?,'synthetic client','trusted',1,1700000000,NULL)")
                .bind(Id128::new([number; 16]).to_base64url())
                .bind(key.verifying_key().to_sec1_point(false).as_bytes())
                .execute(self.storage.pool_for_test())
                .await
                .map_err(|_| "fixture_client")?;
        }
        self.trust
            .revoke(
                "owner",
                &self.expired_request,
                &Id128::new([33; 16]).to_base64url(),
            )
            .await
            .map_err(|_| "seed_real_cached_result")?;
        let clock = Arc::new(TestClock::new());
        clock.advance(u64::try_from(REVOKE_RESULT_LIFETIME_SECONDS).map_err(|_| "lifetime")?);
        self.trust.clock = clock;

        self.raw_scope.start_raw().await;
        let options = self
            .raw_scope
            .options()
            .create_if_missing(false)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Full)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        self.connection = Some(
            SqliteConnection::connect_with(&options)
                .await
                .map_err(|_| "second_connection")?,
        );
        let connection = self
            .connection
            .as_mut()
            .ok_or("second_connection_missing")?;
        let row = sqlx::query("PRAGMA database_list")
            .fetch_one(&mut *connection)
            .await
            .map_err(|_| "second_database_list")?;
        let file: String = row.try_get("file").map_err(|_| "second_database_file")?;
        self.same_database = Path::new(&file)
            .canonicalize()
            .map_err(|_| "second_canonical")?
            == self
                .roots
                .database
                .canonicalize()
                .map_err(|_| "first_canonical")?;
        let journal: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&mut *connection)
            .await
            .map_err(|_| "journal")?;
        let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&mut *connection)
            .await
            .map_err(|_| "foreign_keys")?;
        let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
            .fetch_one(&mut *connection)
            .await
            .map_err(|_| "synchronous")?;
        let busy: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
            .fetch_one(connection)
            .await
            .map_err(|_| "busy_timeout")?;
        if !self.same_database
            || journal != "wal"
            || foreign_keys != 1
            || synchronous != 2
            || busy != 5000
        {
            return Err("second_connection_configuration");
        }
        Ok(())
    }

    async fn images(&self) -> CaseResult<Images> {
        let pool = self.storage.pool_for_test();
        let client = |id: String| async move {
            sqlx::query_as::<_, (String, i64, Option<i64>)>(
                "SELECT status,auth_generation,revoked_at FROM clients WHERE client_id=?",
            )
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|_| "client_postimage")
        };
        Ok(Images {
            target: client(self.target.clone()).await?,
            unrelated: client(self.unrelated.clone()).await?,
            revision: self
                .storage
                .list_revision()
                .await
                .map_err(|_| "revision_postimage")?,
            command_count: sqlx::query_scalar(
                "SELECT COUNT(*) FROM management_commands WHERE actor_id='owner' AND request_id=?",
            )
            .bind(&self.request)
            .fetch_one(pool)
            .await
            .map_err(|_| "command_postimage")?,
            audit_count: sqlx::query_scalar(
                "SELECT COUNT(*) FROM security_audit WHERE action='device.revoke' AND target_id=?",
            )
            .bind(&self.target)
            .fetch_one(pool)
            .await
            .map_err(|_| "audit_postimage")?,
            expired_count: sqlx::query_scalar(
                "SELECT COUNT(*) FROM management_commands WHERE actor_id='owner' AND request_id=?",
            )
            .bind(&self.expired_request)
            .fetch_one(pool)
            .await
            .map_err(|_| "expired_postimage")?,
        })
    }

    async fn finish(mut self) -> serde_json::Value {
        let unused_second_closed = if let Some(connection) = self.connection.take() {
            connection.close().await.is_ok()
        } else {
            true
        };
        self.raw_scope.stop_and_observe().await;
        self.storage.close().await;
        self.scope.stop_and_observe().await;
        let registry_complete = self.storage.registry_close_status()
            == crate::session_storage::ConnectionCloseStatus::Complete;
        let pool_size = self.storage.pool_for_test().size();
        drop(self.trust);
        drop(self.storage);
        let roots = self.roots;
        let mut files = Vec::new();
        let mut exclusive = true;
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let mut file_name = roots.database.as_os_str().to_os_string();
            file_name.push(suffix);
            let path = PathBuf::from(file_name);
            let state = match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.file_attributes() & 0x400 != 0 => {
                    exclusive = false;
                    serde_json::json!({"suffix":suffix,"reparse":true})
                }
                Ok(_) => match OpenOptions::new()
                    .read(true)
                    .write(true)
                    .share_mode(0)
                    .open(&path)
                {
                    Ok(file) => {
                        drop(file);
                        serde_json::json!({"suffix":suffix,"exists":true,"first_exclusive":true})
                    }
                    Err(error) => {
                        exclusive = false;
                        serde_json::json!({"suffix":suffix,"exists":true,"first_exclusive":false,"os_code":error.raw_os_error()})
                    }
                },
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound && !suffix.is_empty() =>
                {
                    serde_json::json!({"suffix":suffix,"exists":false})
                }
                Err(error) => {
                    exclusive = false;
                    serde_json::json!({"suffix":suffix,"first_exclusive":false,"os_code":error.raw_os_error()})
                }
            };
            files.push(state);
        }
        let owned = roots
            .directory
            .canonicalize()
            .is_ok_and(|directory| directory.parent() == Some(roots.root.as_path()))
            && fs::read_to_string(roots.directory.join("owner.txt"))
                .is_ok_and(|marker| marker == MARKER);
        let removed = owned
            && exclusive
            && pool_size == 0
            && registry_complete
            && unused_second_closed
            && fs::remove_dir_all(&roots.directory).is_ok();
        serde_json::json!({"fixture":roots.directory.file_name().and_then(|name|name.to_str()),"registry_complete":registry_complete,"pool_size":pool_size,"files":files,"owned":owned,"unused_second_closed":unused_second_closed,"first_removed":removed})
    }
}

#[derive(Default, serde::Serialize)]
struct OwnershipEvidence {
    same_controller: bool,
    same_client_gate: bool,
    same_database: bool,
}

#[derive(Default, serde::Serialize)]
struct OutcomeEvidence {
    result_ok: bool,
    reply_matches: bool,
    cancelled: bool,
    error_kind: Option<&'static str>,
    sqlite_code: Option<i64>,
}

#[derive(Default, serde::Serialize)]
struct RetryEvidence {
    duplicate_same: bool,
    duplicate_no_notification: bool,
    conflict_retained: bool,
}

#[derive(Default, serde::Serialize)]
struct CaseEvidence {
    ownership: OwnershipEvidence,
    outcome: OutcomeEvidence,
    retries: RetryEvidence,
    original_client_gate_reacquired: bool,
    notification_changed: bool,
    schedule: &'static str,
    release_sequence: u64,
    result_sequence: u64,
    done_sequence: u64,
    writer: WriterReport,
}

type OriginalJoined = Option<Result<Result<RevokeResult, TrustError>, tokio::task::JoinError>>;

impl CaseEvidence {
    fn record_original(&mut self, original: &OriginalJoined, target: &str) {
        match original {
            Some(Ok(Ok(result))) => {
                self.outcome.result_ok = true;
                self.outcome.reply_matches =
                    result.client_id == target && result.status == ClientStatus::Revoked;
            }
            Some(Ok(Err(error))) => {
                let (kind, code) = error_identity(error);
                self.outcome.error_kind = Some(kind);
                self.outcome.sqlite_code = code;
            }
            Some(Err(error)) => {
                self.outcome.cancelled = error.is_cancelled();
                self.outcome.error_kind = Some(if error.is_cancelled() {
                    "JoinError.Cancelled"
                } else {
                    "JoinError.Other"
                });
            }
            None => self.outcome.error_kind = Some("revoke_watchdog_cancelled"),
        }
    }

    async fn collect_images_and_retries(
        &mut self,
        fixture: &Fixture,
        original: &OriginalJoined,
        revision: &mut watch::Receiver<u64>,
    ) -> CaseResult<(Images, Images)> {
        self.notification_changed = revision.has_changed().map_err(|_| "revision_channel")?;
        let _ = revision.borrow_and_update();
        let after = fixture.images().await?;
        if let Some(Ok(Ok(first))) = original {
            let repeated = fixture
                .trust
                .revoke("owner", &fixture.request, &fixture.target)
                .await;
            self.retries.duplicate_same = repeated.as_ref().is_ok_and(|result| result == first);
            self.retries.duplicate_no_notification =
                !revision.has_changed().map_err(|_| "duplicate_revision")?;
            self.retries.conflict_retained = matches!(
                fixture
                    .trust
                    .revoke("owner", &fixture.request, &fixture.unrelated)
                    .await,
                Err(TrustError::RequestConflict)
            );
        }
        Ok((after, fixture.images().await?))
    }
}

fn error_identity(error: &TrustError) -> (&'static str, Option<i64>) {
    match error {
        TrustError::Database(sqlx::Error::Database(database)) => (
            "TrustError.Database.Database",
            database.code().and_then(|value| value.parse().ok()),
        ),
        TrustError::Database(_) => ("TrustError.Database.Other", None),
        TrustError::RequestConflict => ("TrustError.RequestConflict", None),
        _ => ("TrustError.Other", None),
    }
}

async fn controlled_case(
    fixture: &mut Fixture,
    cancel: bool,
) -> CaseResult<(CaseEvidence, Images, Images, Images)> {
    let before = fixture.images().await?;
    let barrier = Arc::new(crate::test_support::AsyncBarrier::default());
    *fixture
        .trust
        .revoke_write_barrier
        .lock()
        .map_err(|_| "barrier_poisoned")? = Some(barrier.clone());
    let trust = fixture.trust.clone();
    let client_gate = fixture.trust.client_gate(&fixture.target);
    let mut evidence = CaseEvidence {
        ownership: OwnershipEvidence {
            same_controller: Arc::ptr_eq(
                &trust.revoke_write_barrier,
                &fixture.trust.revoke_write_barrier,
            ),
            same_client_gate: Arc::ptr_eq(&client_gate, &trust.client_gate(&fixture.target)),
            same_database: fixture.same_database,
        },
        ..CaseEvidence::default()
    };
    let mut revision = fixture.trust.revision_receiver();
    let _ = revision.borrow_and_update();
    let target = fixture.target.clone();
    let request = fixture.request.clone();
    let connection = fixture.connection.take().ok_or("writer_missing")?;
    let mut revoke = tokio::spawn(async move { trust.revoke("owner", &request, &target).await });
    if tokio::time::timeout(WATCHDOG, barrier.wait_until_entered())
        .await
        .is_err()
    {
        barrier.release();
        revoke.abort();
        let _ = revoke.await;
        let _ = connection.close().await;
        return Err("after_read_barrier_unreached");
    }
    let events = Arc::new(Events::default());
    events.mark("after_read_entered");
    let (done, receiver) = watch::channel(false);
    let (acquired, observed) = oneshot::channel();
    let mut writer = tokio::spawn(actual_writer(
        connection,
        acquired,
        receiver,
        events.clone(),
    ));
    evidence.schedule = match tokio::time::timeout(OBSERVATION, observed).await {
        Ok(Ok(())) => "actual_begin_update_ack",
        Ok(Err(_)) => "writer_failed_before_acquired",
        Err(_) => "observation_window_only",
    };
    evidence.release_sequence = events.mark("barrier_release_or_cancel_cut");
    if cancel {
        revoke.abort();
    } else {
        barrier.release();
    }
    let original = if let Ok(result) = tokio::time::timeout(WATCHDOG, &mut revoke).await {
        Some(result)
    } else {
        revoke.abort();
        let _ = revoke.await;
        None
    };
    evidence.result_sequence = events.mark("original_result_or_cancel_join");
    evidence.done_sequence = events.mark("done_sent_after_original_result");
    done.send_replace(true);
    if cancel {
        barrier.release();
    }
    let writer_join = tokio::time::timeout(WATCHDOG, &mut writer).await;
    match writer_join {
        Ok(Ok(report)) => evidence.writer = report,
        Ok(Err(_)) => return Err("writer_panicked_or_cancelled_close_unconfirmed"),
        Err(_) => {
            writer.abort();
            let _ = writer.await;
            return Err("writer_join_or_close_unconfirmed");
        }
    }
    let gate_guard = tokio::time::timeout(WATCHDOG, client_gate.lock())
        .await
        .map_err(|_| "original_client_gate_not_released")?;
    drop(gate_guard);
    evidence.original_client_gate_reacquired = true;
    evidence.record_original(&original, &fixture.target);
    let (after, repeated) = evidence
        .collect_images_and_retries(fixture, &original, &mut revision)
        .await?;
    events.print()?;
    Ok((evidence, before, after, repeated))
}

async fn run_and_assert(cancel: bool) -> CaseResult<()> {
    let mut fixture = Fixture::new(cancel).await?;
    let result = controlled_case(&mut fixture, cancel).await;
    let tail = fixture.finish().await;
    println!("NRV_TAIL {tail}");
    let (evidence, before, after, repeated) = result?;
    println!(
        "NRV_CASE {}",
        serde_json::json!({"cancel_requested":cancel,"t1_rollback_ack":null,"evidence":evidence,"before":before,"after":after,"repeated":repeated})
    );
    assert_eq!(
        tail["first_removed"], true,
        "original first physical close/removal must succeed"
    );
    assert!(
        evidence.ownership.same_controller
            && evidence.ownership.same_client_gate
            && evidence.original_client_gate_reacquired
            && evidence.ownership.same_database
            && evidence.writer.complete(),
        "same real controller/database/writer must complete"
    );
    assert!(
        evidence.done_sequence > evidence.result_sequence
            && evidence.writer.observed_done_sequence > evidence.done_sequence,
        "writer commit waits for original result/cancel"
    );
    if cancel {
        assert!(
            evidence.outcome.cancelled && !evidence.outcome.result_ok,
            "actual caller cancellation must join"
        );
        assert_eq!(after.target, before.target);
        assert_eq!(after.revision, before.revision);
        assert_eq!(after.command_count, before.command_count);
        assert_eq!(after.audit_count, before.audit_count);
        assert_eq!(after.expired_count, before.expired_count);
        assert!(!evidence.notification_changed);
    } else {
        assert!(
            evidence.outcome.result_ok && evidence.outcome.reply_matches,
            "revocation must succeed once; safe error category/code are in NRV_CASE"
        );
        assert_eq!(after.target.0, "revoked");
        assert_eq!(after.target.1, before.target.1 + 1);
        assert!(after.target.2.is_some());
        assert_eq!(after.revision, before.revision + 1);
        assert_eq!(after.command_count, 1);
        assert_eq!(after.audit_count, 1);
        assert_eq!(before.expired_count, 1);
        assert_eq!(after.expired_count, 0);
        assert!(
            evidence.notification_changed
                && evidence.retries.duplicate_same
                && evidence.retries.duplicate_no_notification
                && evidence.retries.conflict_retained
        );
        assert_eq!(repeated.target, after.target);
        assert_eq!(repeated.revision, after.revision);
        assert_eq!(repeated.command_count, after.command_count);
        assert_eq!(repeated.audit_count, after.audit_count);
    }
    assert_eq!(after.unrelated, before.unrelated);
    assert!(
        evidence.writer.acquired_sequence > evidence.release_sequence,
        "fixed writer may acquire after T1 commit and before Rust result, but not during paused T1"
    );
    Ok(())
}

#[tokio::test]
async fn revoke_upgrade_actual_second_writer_preserves_success_and_idempotency() -> CaseResult<()> {
    run_and_assert(false).await
}

#[tokio::test]
async fn revoke_upgrade_after_read_caller_cancel_converges_without_business_writes()
-> CaseResult<()> {
    run_and_assert(true).await
}
