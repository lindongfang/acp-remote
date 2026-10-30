//! Real SQLite storage tests; output control below is a declared test port, not a Harness.
use super::*;
use crate::{
    application::history::{HistoryClock, HistoryCoordinator, OutputControl},
    domain::{
        ActorContext, InputRequest,
        history::{IntegrityReason, RunId, SessionId, TurnId},
    },
    storage::{NodeMeta, SCHEMA_VERSION, Storage},
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
const CHILD_ROOT: &str = "ACP_WP2_HISTORY_CRASH_ROOT";
struct Area {
    directory: PathBuf,
}
impl Area {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "acp-wp2-history-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).expect("owned fixture");
        Self { directory }
    }
    fn database(&self) -> PathBuf {
        self.directory.join("node.sqlite3")
    }
}
impl Drop for Area {
    fn drop(&mut self) {
        assert_eq!(
            self.directory.parent(),
            Some(std::env::temp_dir().as_path())
        );
        assert!(
            self.directory
                .file_name()
                .expect("name")
                .to_string_lossy()
                .starts_with("acp-wp2-history-")
        );
        if self.directory.exists() {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
}
fn id(value: u8) -> EntityId {
    EntityId::from_bytes([value; 16])
}
async fn seed(storage: &Storage) -> OutputScope {
    storage
        .write_node_meta(&NodeMeta {
            schema_version: SCHEMA_VERSION,
            node_id: "fixture".into(),
            public_key: vec![4; 65],
            key_file_ref: "identity.key".into(),
        })
        .await
        .expect("meta");
    sqlx::query("INSERT INTO clients(client_id,public_key,display_name,status,auth_generation,created_at) VALUES(?,X'01','fixture','trusted',1,0)").bind(id(1).encode()).execute(storage.pool_for_test()).await.expect("client");
    sqlx::query("INSERT INTO harnesses VALUES('h','fixture','not-launched','[]',NULL)")
        .execute(storage.pool_for_test())
        .await
        .expect("harness");
    sqlx::query("INSERT INTO workspaces VALUES('w','fixture')")
        .execute(storage.pool_for_test())
        .await
        .expect("workspace");
    sqlx::query(
        "INSERT INTO sessions(session_id,harness_id,workspace_id,state) VALUES(?,'h','w','loaded')",
    )
    .bind(id(3).encode())
    .execute(storage.pool_for_test())
    .await
    .expect("session");
    sqlx::query("INSERT INTO session_state VALUES(?,0,0,0)")
        .bind(id(3).encode())
        .execute(storage.pool_for_test())
        .await
        .expect("sequence");
    let actor = ActorContext {
        client_id: id(1),
        auth_generation: 1,
    };
    let reference = storage
        .sessions()
        .persist_input(actor, id(3), b"synthetic input")
        .await
        .expect("input content");
    let accepted = storage
        .sessions()
        .admit_input(
            actor,
            InputRequest {
                request_id: id(5),
                session_id: id(3),
                content_ref: reference,
            },
        )
        .await
        .expect("accepted");
    OutputScope {
        session: SessionId(id(3)),
        run: RunId(EntityId::decode(&accepted.command.input.run_id).expect("run")),
        turn: TurnId(EntityId::decode(&accepted.command.input.turn_id).expect("turn")),
    }
}
async fn scalar(storage: &Storage, sql: &str) -> i64 {
    sqlx::query_scalar(sql)
        .fetch_one(storage.pool_for_test())
        .await
        .expect("scalar")
}
fn item(
    scope: OutputScope,
    position: Position,
    value: u8,
    kind: &str,
    version: u64,
    body: &[u8],
) -> HistoryItem {
    HistoryItem {
        identity: Arc::new(ItemIdentity {
            scope,
            item: ItemId(id(value)),
            source: SourceId(id(value + 30)),
            position,
            kind: Arc::from(kind),
            protocol: Arc::from("acp"),
            protocol_version: Arc::from("2-draft"),
            tool_call_id: None,
            tool_origin: None,
        }),
        version: ContentVersion::new(version).expect("version"),
        content: RawContent(Arc::from(body)),
    }
}

#[tokio::test]
async fn source_order_tool_updates_and_unknown_raw_fields_survive_latest_value_storage() {
    let area = Area::new();
    let storage = Storage::open(&area.database()).await.expect("storage");
    let scope = seed(&storage).await;
    let history = storage.histories();
    let mut positions = history
        .reserve_positions(scope, 64)
        .await
        .expect("positions");
    let tool = item(
        scope,
        positions.take().expect("tool"),
        10,
        "tool",
        1,
        br#"{"tool":{"state":"running"},"extension":[{"a":1}]}"#,
    );
    let text = item(
        scope,
        positions.take().expect("text"),
        11,
        "text",
        1,
        b"same visible text",
    );
    let separate = item(
        scope,
        positions.take().expect("separate"),
        12,
        "text",
        1,
        b"same visible text",
    );
    let unknown=serde_json::to_vec(&serde_json::json!({"nativeId":"opaque-native-id-".repeat(4096),"unknown":{"nested":[1,{"retained":true,"children":[{"raw":"synthetic"}]}]},"binary":{"encoding":"base64","data":"AAEC"}})).expect("unknown payload");
    let opaque = item(
        scope,
        positions.take().expect("unknown"),
        13,
        "unsupported",
        1,
        &unknown,
    );
    history
        .commit_items(
            scope,
            &[tool.clone(), text.clone(), separate.clone(), opaque.clone()],
            IntegrityChange::Keep,
        )
        .await
        .expect("first save");
    let mut complete = tool.clone();
    complete.version = ContentVersion::new(2).expect("version2");
    complete.content=RawContent(Arc::from(br#"{"tool":{"state":"completed","output":{"x":true}},"extension":[{"a":1}],"future":{"preserved":true}}"#.as_slice()));
    history
        .commit_items(scope, &[complete.clone()], IntegrityChange::Keep)
        .await
        .expect("tool completion");
    assert_eq!(
        scalar(&storage, "SELECT COUNT(*) FROM conversation_items").await,
        5
    );
    let loaded = history
        .committed_item(scope, tool.identity.item)
        .await
        .expect("tool");
    assert_eq!(loaded.item.identity, tool.identity);
    assert_eq!(loaded.item.version.value(), 2);
    assert_eq!(loaded.item.content.0, complete.content.0);
    let loaded = history
        .committed_item(scope, opaque.identity.item)
        .await
        .expect("unknown");
    assert_eq!(loaded.item.content.0.as_ref(), unknown);
    assert_eq!(loaded.item.identity.protocol_version.as_ref(), "2-draft");
    let positions: Vec<i64> =
        sqlx::query_scalar("SELECT first_position FROM conversation_items ORDER BY first_position")
            .fetch_all(storage.pool_for_test())
            .await
            .expect("order");
    assert_eq!(positions, vec![0, 1, 2, 3, 4]);
    assert_eq!(
        scalar(
            &storage,
            "SELECT COUNT(*) FROM content_objects WHERE purpose='output'"
        )
        .await,
        4,
        "latest objects only, no version log"
    );
    storage.close().await;
}

#[tokio::test]
async fn stale_versions_never_overwrite_and_same_version_content_or_source_conflicts_fail() {
    let area = Area::new();
    let storage = Storage::open(&area.database()).await.expect("storage");
    let scope = seed(&storage).await;
    let history = storage.histories();
    let position = history
        .reserve_positions(scope, 64)
        .await
        .expect("positions")
        .take()
        .expect("position");
    let old = item(scope, position, 10, "tool", 1, b"old");
    let mut newest = old.clone();
    newest.version = ContentVersion::new(3).expect("v3");
    newest.content = RawContent(Arc::from(b"latest".as_slice()));
    history
        .commit_items(scope, &[newest.clone()], IntegrityChange::Keep)
        .await
        .expect("v3");
    let ack = history
        .commit_items(scope, std::slice::from_ref(&old), IntegrityChange::Keep)
        .await
        .expect("stale save");
    assert_eq!(ack[0].version.value(), 3);
    assert_eq!(
        history
            .committed_item(scope, old.identity.item)
            .await
            .expect("latest")
            .item
            .content
            .0
            .as_ref(),
        b"latest"
    );
    let mut conflict = newest.clone();
    conflict.content = RawContent(Arc::from(b"different".as_slice()));
    assert!(matches!(
        history
            .commit_items(scope, &[conflict], IntegrityChange::Keep)
            .await,
        Err(HistoryError::VersionConflict)
    ));
    let mut other = (*newest.identity).clone();
    other.source = SourceId(id(99));
    let conflict = HistoryItem {
        identity: Arc::new(other),
        ..newest
    };
    assert!(matches!(
        history
            .commit_items(scope, &[conflict], IntegrityChange::Keep)
            .await,
        Err(HistoryError::IdentityConflict)
    ));
    assert_eq!(
        scalar(
            &storage,
            "SELECT COUNT(*) FROM content_objects WHERE purpose='output'"
        )
        .await,
        1
    );
    storage.close().await;
}

#[tokio::test]
async fn immutable_read_pins_delay_only_unreferenced_old_content_cleanup_and_restart_clears_them() {
    let area = Area::new();
    let storage = Storage::open(&area.database()).await.expect("storage");
    let scope = seed(&storage).await;
    let history = storage.histories();
    let position = history
        .reserve_positions(scope, 64)
        .await
        .expect("reserve")
        .take()
        .expect("position");
    let first = item(scope, position, 10, "tool", 1, b"first immutable body");
    history
        .commit_items(scope, std::slice::from_ref(&first), IntegrityChange::Keep)
        .await
        .expect("v1");
    let pin = history
        .pin_version(scope, first.identity.item)
        .await
        .expect("pin1");
    let pin2 = history
        .pin_version(scope, first.identity.item)
        .await
        .expect("pin2");
    let mut second = first.clone();
    second.version = ContentVersion::new(2).expect("v2");
    second.content = RawContent(Arc::from(b"second immutable body".as_slice()));
    history
        .commit_items(scope, &[second.clone()], IntegrityChange::Keep)
        .await
        .expect("v2");
    let pinned: Vec<u8> =
        sqlx::query_scalar("SELECT body FROM content_objects WHERE content_ref=?")
            .bind(pin.content_ref.encode())
            .fetch_one(storage.pool_for_test())
            .await
            .expect("pinned bytes");
    assert_eq!(pinned, b"first immutable body");
    history
        .release_pin(pin)
        .await
        .expect("release first reader");
    assert_eq!(
        scalar(
            &storage,
            "SELECT COUNT(*) FROM content_objects WHERE purpose='output'"
        )
        .await,
        2
    );
    history
        .release_pin(pin2)
        .await
        .expect("release last reader");
    assert_eq!(
        scalar(
            &storage,
            "SELECT COUNT(*) FROM content_objects WHERE purpose='output'"
        )
        .await,
        1
    );
    let _old_pin = history
        .pin_version(scope, first.identity.item)
        .await
        .expect("new pin");
    let mut third = second;
    third.version = ContentVersion::new(3).expect("v3");
    history
        .commit_items(scope, &[third], IntegrityChange::Keep)
        .await
        .expect("v3");
    storage
        .sessions()
        .recover_interrupted()
        .await
        .expect("startup owns all old pins");
    assert_eq!(
        scalar(&storage, "SELECT COUNT(*) FROM history_content_pins").await,
        0
    );
    assert_eq!(
        scalar(
            &storage,
            "SELECT COUNT(*) FROM content_objects WHERE purpose='output'"
        )
        .await,
        1
    );
    storage.close().await;
}

#[tokio::test]
async fn history_body_item_revision_and_real_commit_faults_roll_back_all_versions() {
    for failure in ["body", "item", "revision", "commit"] {
        let area = Area::new();
        let storage = Storage::open(&area.database()).await.expect("storage");
        let scope = seed(&storage).await;
        let history = storage.histories();
        let position = history
            .reserve_positions(scope, 64)
            .await
            .expect("reserve")
            .take()
            .expect("position");
        let value = item(scope, position, 10, "tool", 1, b"structured output");
        let revision = scalar(&storage, "SELECT revision FROM session_state").await;
        match failure {
            "body" => {
                sqlx::query("CREATE TRIGGER history_fault BEFORE INSERT ON content_objects WHEN NEW.purpose='output' BEGIN SELECT RAISE(ABORT,'fixture'); END").execute(storage.pool_for_test()).await.expect("body fault");
            }
            "item" => {
                sqlx::query("CREATE TRIGGER history_fault BEFORE INSERT ON conversation_items WHEN NEW.kind!='user' BEGIN SELECT RAISE(ABORT,'fixture'); END").execute(storage.pool_for_test()).await.expect("item fault");
            }
            "revision" => {
                sqlx::query("CREATE TRIGGER history_fault BEFORE UPDATE OF revision ON session_state BEGIN SELECT RAISE(ABORT,'fixture'); END").execute(storage.pool_for_test()).await.expect("revision fault");
            }
            _ => {
                sqlx::query("CREATE TABLE history_guard(value TEXT REFERENCES sessions(session_id) DEFERRABLE INITIALLY DEFERRED)").execute(storage.pool_for_test()).await.expect("guard");
                sqlx::query("CREATE TRIGGER history_fault AFTER INSERT ON conversation_items WHEN NEW.kind!='user' BEGIN INSERT INTO history_guard VALUES('absent'); END").execute(storage.pool_for_test()).await.expect("real commit fault");
            }
        }
        assert!(
            history
                .commit_items(scope, std::slice::from_ref(&value), IntegrityChange::Keep)
                .await
                .is_err(),
            "{failure}"
        );
        assert_eq!(
            scalar(&storage, "SELECT COUNT(*) FROM content_objects").await,
            1
        );
        assert_eq!(
            scalar(&storage, "SELECT COUNT(*) FROM conversation_items").await,
            1
        );
        assert_eq!(
            scalar(&storage, "SELECT revision FROM session_state").await,
            revision
        );
        assert_eq!(
            scalar(&storage, "SELECT next_position FROM session_state").await,
            65,
            "reservation is separate committed fact"
        );
        sqlx::query("DROP TRIGGER history_fault")
            .execute(storage.pool_for_test())
            .await
            .expect("clear fault");
        history
            .commit_items(scope, &[value], IntegrityChange::Keep)
            .await
            .expect("retry same identity/position");
        storage.close().await;
    }
}

#[tokio::test]
async fn a_later_item_fault_rolls_back_the_entire_batch_and_failed_cas_keeps_old_content() {
    let area = Area::new();
    let storage = Storage::open(&area.database()).await.expect("storage");
    let scope = seed(&storage).await;
    let history = storage.histories();
    let mut positions = history.reserve_positions(scope, 64).await.expect("reserve");
    let first = item(
        scope,
        positions.take().expect("first"),
        10,
        "tool",
        1,
        b"old",
    );
    let second = item(
        scope,
        positions.take().expect("second"),
        11,
        "break",
        1,
        b"second",
    );
    sqlx::query("CREATE TRIGGER history_fault BEFORE INSERT ON conversation_items WHEN NEW.kind='break' BEGIN SELECT RAISE(ABORT,'fixture'); END").execute(storage.pool_for_test()).await.expect("batch fault");
    assert!(
        history
            .commit_items(scope, &[first.clone(), second], IntegrityChange::Keep)
            .await
            .is_err()
    );
    assert_eq!(
        scalar(
            &storage,
            "SELECT COUNT(*) FROM content_objects WHERE purpose='output'"
        )
        .await,
        0
    );
    sqlx::query("DROP TRIGGER history_fault")
        .execute(storage.pool_for_test())
        .await
        .expect("clear");
    history
        .commit_items(scope, std::slice::from_ref(&first), IntegrityChange::Keep)
        .await
        .expect("v1");
    sqlx::query("CREATE TRIGGER history_fault BEFORE UPDATE OF content_version ON conversation_items BEGIN SELECT RAISE(ABORT,'fixture'); END").execute(storage.pool_for_test()).await.expect("update fault");
    let mut changed = first.clone();
    changed.version = ContentVersion::new(2).expect("v2");
    changed.content = RawContent(Arc::from(b"new".as_slice()));
    assert!(
        history
            .commit_items(scope, &[changed], IntegrityChange::Keep)
            .await
            .is_err()
    );
    assert_eq!(
        history
            .committed_item(scope, first.identity.item)
            .await
            .expect("old still committed")
            .item
            .content
            .0
            .as_ref(),
        b"old"
    );
    assert_eq!(
        scalar(
            &storage,
            "SELECT COUNT(*) FROM content_objects WHERE purpose='output'"
        )
        .await,
        1
    );
    storage.close().await;
}

#[derive(Clone, Default)]
struct Clock(Arc<AtomicU64>);
impl HistoryClock for Clock {
    fn millis(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }
}
#[derive(Clone, Default)]
struct Output(Arc<AtomicUsize>);
impl OutputControl for Output {
    fn request_stop(
        &self,
        _: OutputScope,
        _: IntegrityFault,
    ) -> crate::domain::history::StopReceipt {
        self.0.fetch_add(1, Ordering::AcqRel);
        crate::domain::history::StopReceipt::Requested
    }
    fn finish_stop(
        &self,
        _: OutputScope,
    ) -> impl std::future::Future<Output = Result<(), HistoryError>> {
        std::future::ready(Ok(()))
    }
}
#[tokio::test]
async fn coordinator_real_sqlite_faults_never_advance_commit_and_stop_at_three_failures() {
    let area = Area::new();
    let storage = Storage::open(&area.database()).await.expect("storage");
    let scope = seed(&storage).await;
    let clock = Clock::default();
    let output = Output::default();
    let owner = HistoryCoordinator::new(storage.histories(), clock.clone(), output.clone());
    owner.attach(scope).await.expect("attach");
    let handle = owner
        .begin_item(
            scope,
            SourceId(id(80)),
            Arc::from("tool"),
            Arc::from("acp"),
            Arc::from("2"),
            vec![b'x'; 65536],
        )
        .await
        .expect("item");
    sqlx::query("CREATE TRIGGER history_fault BEFORE INSERT ON content_objects WHEN NEW.purpose='output' BEGIN SELECT RAISE(ABORT,'fixture'); END").execute(storage.pool_for_test()).await.expect("real fault");
    for now in [0, 250, 500] {
        clock.0.store(now, Ordering::Release);
        assert!(owner.tick().await.is_err());
        assert_eq!(handle.committed_version(), 0);
    }
    let status = owner.status(scope).expect("fault");
    assert!(status.stopped);
    assert_eq!(
        status.fault.expect("fault").reason,
        IntegrityReason::PersistentFailure
    );
    assert_eq!(output.0.load(Ordering::Acquire), 1);
    sqlx::query("DROP TRIGGER history_fault")
        .execute(storage.pool_for_test())
        .await
        .expect("restore");
    clock.0.store(750, Ordering::Release);
    owner.tick().await.expect("persist retained failure facts");
    assert_eq!(handle.committed_version(), 1);
    assert!(owner.status(scope).expect("never auto resume").stopped);
    let reason: String = sqlx::query_scalar("SELECT reason FROM history_integrity")
        .fetch_one(storage.pool_for_test())
        .await
        .expect("durable error");
    assert_eq!(reason, "persistent_failure");
    storage.close().await;
}

#[tokio::test]
async fn position_reservation_fault_and_counter_limit_leave_the_durable_counter_unchanged() {
    let area = Area::new();
    let storage = Storage::open(&area.database()).await.expect("storage");
    let scope = seed(&storage).await;
    let history = storage.histories();
    sqlx::query("CREATE TRIGGER position_fault BEFORE UPDATE OF next_position ON session_state BEGIN SELECT RAISE(ABORT,'fixture'); END").execute(storage.pool_for_test()).await.expect("fault");
    assert!(history.reserve_positions(scope, 64).await.is_err());
    assert_eq!(
        scalar(&storage, "SELECT next_position FROM session_state").await,
        1
    );
    sqlx::query("DROP TRIGGER position_fault")
        .execute(storage.pool_for_test())
        .await
        .expect("clear");
    sqlx::query("UPDATE session_state SET next_position=9007199254740980")
        .execute(storage.pool_for_test())
        .await
        .expect("safe bound");
    assert!(matches!(
        history.reserve_positions(scope, 64).await,
        Err(HistoryError::CounterExhausted)
    ));
    assert_eq!(
        scalar(&storage, "SELECT next_position FROM session_state").await,
        9_007_199_254_740_980
    );
    storage.close().await;
}

#[tokio::test]
async fn abrupt_child_exit_loses_only_uncommitted_tail_and_never_reuses_reserved_positions() {
    if std::env::var_os(CHILD_ROOT).is_some() {
        return;
    }
    let area = Area::new();
    let storage = Storage::open(&area.database()).await.expect("storage");
    let scope = seed(&storage).await;
    storage.close().await;
    let mut child = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .arg("session_storage::history::tests::crash_after_reservation_child")
        .arg("--exact")
        .arg("--nocapture")
        .env(CHILD_ROOT, &area.directory)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("owned child");
    let started = std::time::Instant::now();
    loop {
        if child.try_wait().expect("owned child status").is_some() {
            break;
        }
        if started.elapsed() > std::time::Duration::from_secs(10) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("owned crash fixture exceeded10s and was reaped");
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let child = child.wait_with_output().expect("owned child output");
    assert_eq!(child.status.code(), Some(93));
    assert!(String::from_utf8_lossy(&child.stdout).contains("VISIBLE_POSITION=1"));
    let storage = Storage::open(&area.database())
        .await
        .expect("reopen after abrupt exit");
    assert_eq!(
        scalar(&storage, "SELECT next_position FROM session_state").await,
        65
    );
    assert_eq!(
        scalar(&storage, "SELECT COUNT(*) FROM conversation_items").await,
        1,
        "only committed user input remains"
    );
    let mut next = storage
        .histories()
        .reserve_positions(scope, 64)
        .await
        .expect("fresh range");
    assert_eq!(next.take().expect("next position").value(), 65);
    storage
        .sessions()
        .recover_interrupted()
        .await
        .expect("unknown tail reconciliation");
    let row = sqlx::query("SELECT reason,first_position,last_position FROM history_integrity")
        .fetch_one(storage.pool_for_test())
        .await
        .expect("loss fact");
    assert_eq!(row.get::<String, _>("reason"), "interrupted");
    assert!(row.get::<i64, _>("first_position") <= 1);
    assert!(row.get::<i64, _>("last_position") >= 64);
    storage.close().await;
}
#[tokio::test]
async fn crash_after_reservation_child() {
    let Some(directory) = std::env::var_os(CHILD_ROOT) else {
        return;
    };
    let database = PathBuf::from(directory).join("node.sqlite3");
    let storage = Storage::open(&database).await.expect("child storage");
    let row = sqlx::query("SELECT run_id,turn_id FROM runs")
        .fetch_one(storage.pool_for_test())
        .await
        .expect("run");
    let scope = OutputScope {
        session: SessionId(id(3)),
        run: RunId(EntityId::decode(&row.get::<String, _>("run_id")).expect("run id")),
        turn: TurnId(EntityId::decode(&row.get::<String, _>("turn_id")).expect("turn id")),
    };
    let owner = HistoryCoordinator::new(storage.histories(), Clock::default(), Output::default());
    owner
        .attach(scope)
        .await
        .expect("reserve committed before visible");
    let item = owner
        .begin_item(
            scope,
            SourceId(id(80)),
            Arc::from("text"),
            Arc::from("acp"),
            Arc::from("1"),
            b"synthetic visible uncommitted tail".to_vec(),
        )
        .await
        .expect("visible unsaved");
    println!("VISIBLE_POSITION={}", item.identity().position.value());
    std::process::exit(93);
}

#[tokio::test]
async fn integrity_facts_belong_to_their_original_run_and_survive_a_later_clean_run() {
    let area = Area::new();
    let storage = Storage::open(&area.database()).await.expect("storage");
    let old = seed(&storage).await;
    let history = storage.histories();
    let mut positions = history.reserve_positions(old, 64).await.expect("reserve");
    let value = item(
        old,
        positions.take().expect("position"),
        10,
        "tool",
        1,
        b"old structured item",
    );
    let fault = IntegrityFault {
        reason: IntegrityReason::Interrupted,
        possible_loss: Some(MissingRange {
            first: value.identity.position,
            last: value.identity.position,
            dropped_bytes: 7,
        }),
    };
    history
        .commit_items(
            old,
            std::slice::from_ref(&value),
            IntegrityChange::Set(fault),
        )
        .await
        .expect("old fault");
    sqlx::query("UPDATE runs SET status='completed'")
        .execute(storage.pool_for_test())
        .await
        .expect("terminal fixture");
    sqlx::query("UPDATE session_commands SET status='completed'")
        .execute(storage.pool_for_test())
        .await
        .expect("terminal fixture");
    let actor = ActorContext {
        client_id: id(1),
        auth_generation: 1,
    };
    let content = storage
        .sessions()
        .persist_input(actor, id(3), b"next input fixture")
        .await
        .expect("next body");
    let next = storage
        .sessions()
        .admit_input(
            actor,
            InputRequest {
                request_id: id(6),
                session_id: id(3),
                content_ref: content,
            },
        )
        .await
        .expect("next accepted");
    let scope = OutputScope {
        session: old.session,
        run: RunId(EntityId::decode(&next.command.input.run_id).expect("run")),
        turn: TurnId(EntityId::decode(&next.command.input.turn_id).expect("turn")),
    };
    let mut next_positions = history
        .reserve_positions(scope, 64)
        .await
        .expect("new range");
    let current = item(
        scope,
        next_positions.take().expect("new position"),
        11,
        "tool",
        1,
        b"new complete body",
    );
    history
        .commit_items(scope, &[current], IntegrityChange::Keep)
        .await
        .expect("clean later save");
    assert_eq!(
        history.integrity(old).await.expect("old error"),
        Some(fault)
    );
    assert_eq!(history.integrity(scope).await.expect("new run"), None);
    storage.close().await;
}

#[tokio::test]
async fn history_replacement_cleanup_preserves_prepared_unaccepted_input_references() {
    let area = Area::new();
    let storage = Storage::open(&area.database()).await.expect("storage");
    let scope = seed(&storage).await;
    let actor = ActorContext {
        client_id: id(1),
        auth_generation: 1,
    };
    let prepared = storage
        .sessions()
        .persist_input(actor, id(3), b"prepared but not accepted fixture")
        .await
        .expect("prepared input");
    let history = storage.histories();
    let mut positions = history.reserve_positions(scope, 64).await.expect("reserve");
    let mut value = item(
        scope,
        positions.take().expect("position"),
        10,
        "tool",
        1,
        b"old output",
    );
    history
        .commit_items(scope, std::slice::from_ref(&value), IntegrityChange::Keep)
        .await
        .expect("old history");
    let pin = history
        .pin_version(scope, value.identity.item)
        .await
        .expect("reader pin");
    value.version = ContentVersion::new(2).expect("v2");
    value.content = RawContent(Arc::from(b"new output".as_slice()));
    history
        .commit_items(scope, &[value], IntegrityChange::Keep)
        .await
        .expect("replace latest");
    history
        .release_pin(pin)
        .await
        .expect("cleanup orphan old output");
    assert_eq!(
        scalar(
            &storage,
            "SELECT COUNT(*) FROM content_objects WHERE purpose='input'"
        )
        .await,
        2
    );
    let prepared_body: Vec<u8> =
        sqlx::query_scalar("SELECT body FROM content_objects WHERE content_ref=?")
            .bind(prepared.encode())
            .fetch_one(storage.pool_for_test())
            .await
            .expect("unaccepted input survives cleanup");
    assert_eq!(prepared_body, b"prepared but not accepted fixture");
    // The external reliable-terminal fixture frees the foreground slot; no native resume is claimed.
    sqlx::query("UPDATE runs SET status='completed'")
        .execute(storage.pool_for_test())
        .await
        .expect("terminal fixture");
    sqlx::query("UPDATE session_commands SET status='completed'")
        .execute(storage.pool_for_test())
        .await
        .expect("terminal command fixture");
    let accepted = storage
        .sessions()
        .admit_input(
            actor,
            InputRequest {
                request_id: id(7),
                session_id: id(3),
                content_ref: prepared,
            },
        )
        .await
        .expect("original prepared reference still accepted");
    assert!(!accepted.replayed);
    assert_eq!(scalar(&storage, "SELECT COUNT(*) FROM runs").await, 2);
    storage.close().await;
}

#[tokio::test]
async fn independent_sqlite_pin_and_replacement_transactions_have_no_unpinned_read_window() {
    let area = Area::new();
    let first = Storage::open(&area.database()).await.expect("storage");
    let scope = seed(&first).await;
    let second = Storage::open(&area.database())
        .await
        .expect("independent writer");
    let reader = first.histories();
    let writer = second.histories();
    let mut positions = reader.reserve_positions(scope, 64).await.expect("reserve");
    let original = item(
        scope,
        positions.take().expect("position"),
        10,
        "tool",
        1,
        b"version-1",
    );
    reader
        .commit_items(
            scope,
            std::slice::from_ref(&original),
            IntegrityChange::Keep,
        )
        .await
        .expect("v1");
    for version in 2..=17 {
        let mut next = original.clone();
        next.version = ContentVersion::new(version).expect("version");
        next.content = RawContent(Arc::from(format!("version-{version}").into_bytes()));
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let reader_barrier = Arc::clone(&barrier);
        let (pin, saved) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            tokio::join!(
                async {
                    reader_barrier.wait().await;
                    reader.pin_version(scope, original.identity.item).await
                },
                async {
                    barrier.wait().await;
                    writer
                        .commit_items(scope, std::slice::from_ref(&next), IntegrityChange::Keep)
                        .await
                }
            )
        })
        .await
        .expect("pin/write race bounded10s");
        saved.expect("replace");
        let pin = pin.expect("pin");
        assert!([version - 1, version].contains(&pin.version.value()));
        let bytes: Vec<u8> =
            sqlx::query_scalar("SELECT body FROM content_objects WHERE content_ref=?")
                .bind(pin.content_ref.encode())
                .fetch_one(first.pool_for_test())
                .await
                .expect("pinned object exists despite replacement cleanup");
        assert_eq!(bytes, format!("version-{}", pin.version.value()).as_bytes());
        reader.release_pin(pin).await.expect("release");
        assert_eq!(
            scalar(
                &first,
                "SELECT COUNT(*) FROM content_objects WHERE purpose='output'"
            )
            .await,
            1
        );
    }
    second.close().await;
    first.close().await;
}
