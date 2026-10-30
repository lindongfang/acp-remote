use super::*;
use crate::{
    error::StorageError,
    storage::{NodeMeta, SCHEMA_VERSION, Storage},
};
use sqlx::{
    Connection,
    sqlite::{SqliteConnectOptions, SqliteConnection},
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Area {
    directory: PathBuf,
}
impl Area {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "acp-wp2-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).expect("owned fixture directory");
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
                .expect("owned directory")
                .to_string_lossy()
                .starts_with("acp-wp2-")
        );
        if self.directory.exists() {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
}
fn id(value: u8) -> EntityId {
    EntityId::from_bytes([value; 16])
}
fn actor(value: u8) -> ActorContext {
    ActorContext {
        client_id: id(value),
        auth_generation: 7,
    }
}
async fn seed(storage: &Storage) {
    storage
        .write_node_meta(&NodeMeta {
            schema_version: SCHEMA_VERSION,
            node_id: "fixture-node".into(),
            public_key: vec![4; 65],
            key_file_ref: "identity.key".into(),
        })
        .await
        .expect("fixture metadata");
    for value in [1, 2] {
        sqlx::query("INSERT INTO clients(client_id,public_key,display_name,status,auth_generation,created_at) VALUES(?,?,'fixture','trusted',7,0)")
            .bind(id(value).encode()).bind(vec![value;65]).execute(storage.pool_for_test()).await.expect("client");
    }
    sqlx::query("INSERT INTO harnesses VALUES('h','fixture','not-launched','[]',NULL)")
        .execute(storage.pool_for_test())
        .await
        .expect("harness");
    sqlx::query("INSERT INTO workspaces VALUES('w','fixture-only')")
        .execute(storage.pool_for_test())
        .await
        .expect("workspace");
    for session in [id(3), id(4)] {
        sqlx::query("INSERT INTO sessions(session_id,harness_id,workspace_id,state) VALUES(?,'h','w','loaded')").bind(session.encode()).execute(storage.pool_for_test()).await.expect("session");
        sqlx::query("INSERT INTO session_state VALUES(?,0,0,0)")
            .bind(session.encode())
            .execute(storage.pool_for_test())
            .await
            .expect("sequence");
    }
}
async fn input(
    store: &SessionStore,
    who: ActorContext,
    session: EntityId,
    request: u8,
) -> InputRequest {
    let content_ref = store
        .persist_input(
            who,
            session,
            br#"{"type":"text","text":"synthetic fixture","unknown":{"retained":true}}"#,
        )
        .await
        .expect("persist content");
    InputRequest {
        request_id: id(request),
        session_id: session,
        content_ref,
    }
}
async fn count(storage: &Storage, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(storage.pool_for_test())
        .await
        .expect("count")
}
async fn assert_no_acceptance(storage: &Storage) {
    for table in [
        "session_commands",
        "turns",
        "runs",
        "conversation_items",
        "security_audit",
    ] {
        assert_eq!(count(storage, table).await, 0, "{table}");
    }
    let next: i64 =
        sqlx::query_scalar("SELECT SUM(revision+next_position+next_turn) FROM session_state")
            .fetch_one(storage.pool_for_test())
            .await
            .expect("sequence");
    assert_eq!(next, 0);
    assert_eq!(
        count(storage, "content_objects").await,
        1,
        "pre-existing upload survives failed admission"
    );
}

#[tokio::test]
async fn atomic_admission_preserves_content_and_metadata_without_body_in_audit() {
    let area = Area::new();
    let storage = Storage::open(&area.database()).await.expect("storage");
    seed(&storage).await;
    let store = storage.sessions();
    let request = input(&store, actor(1), id(3), 5).await;
    let accepted = crate::application::InputService::new(store.clone())
        .submit(actor(1), request)
        .await
        .expect("admit");
    assert!(!accepted.replayed);
    for table in [
        "session_commands",
        "turns",
        "runs",
        "conversation_items",
        "security_audit",
    ] {
        assert_eq!(count(&storage, table).await, 1, "{table}");
    }
    let row=sqlx::query("SELECT c.session_id,c.status,r.command_id,r.turn_id,i.content_ref,i.content_version,i.committed_version,i.first_position FROM session_commands c JOIN runs r USING(command_id) JOIN conversation_items i USING(turn_id)")
        .fetch_one(storage.pool_for_test()).await.expect("relations");
    assert_eq!(
        row.get::<String, _>("command_id"),
        accepted.command.input.command_id
    );
    assert_eq!(
        row.get::<String, _>("turn_id"),
        accepted.command.input.turn_id
    );
    assert_eq!(
        row.get::<String, _>("content_ref"),
        request.content_ref.encode()
    );
    assert_eq!(row.get::<i64, _>("content_version"), 1);
    assert_eq!(row.get::<i64, _>("committed_version"), 1);
    assert_eq!(row.get::<i64, _>("first_position"), 0);
    let audit = sqlx::query("SELECT actor_id,action,target_id,outcome FROM security_audit")
        .fetch_one(storage.pool_for_test())
        .await
        .expect("audit");
    assert_eq!(
        audit.get::<String, _>("actor_id"),
        actor(1).client_id.encode()
    );
    assert_eq!(
        audit.get::<String, _>("target_id"),
        accepted.command.input.command_id
    );
    assert_eq!(audit.get::<String, _>("action"), "session.input");
    assert_eq!(audit.get::<String, _>("outcome"), "accepted");
    storage.close().await;
}

#[tokio::test]
async fn each_statement_and_real_deferred_commit_failure_roll_back_every_association() {
    for failure in [
        "session_commands",
        "turns",
        "runs",
        "conversation_items",
        "session_state",
        "security_audit",
        "commit",
    ] {
        let area = Area::new();
        let storage = Storage::open(&area.database()).await.expect("storage");
        seed(&storage).await;
        let store = storage.sessions();
        let request = input(&store, actor(1), id(3), 5).await;
        if failure == "commit" {
            sqlx::query("CREATE TABLE deferred_guard(value TEXT REFERENCES sessions(session_id) DEFERRABLE INITIALLY DEFERRED)").execute(storage.pool_for_test()).await.expect("guard");
            sqlx::query("CREATE TRIGGER fail_commit AFTER INSERT ON security_audit BEGIN INSERT INTO deferred_guard VALUES('missing-session'); END").execute(storage.pool_for_test()).await.expect("deferred trigger");
        } else {
            let event = if failure == "session_state" {
                "UPDATE"
            } else {
                "INSERT"
            };
            sqlx::query(&format!("CREATE TRIGGER fail_statement BEFORE {event} ON {failure} BEGIN SELECT RAISE(ABORT,'injected fixture failure'); END")).execute(storage.pool_for_test()).await.expect("trigger");
        }
        assert!(
            matches!(
                store.admit_input(actor(1), request).await,
                Err(SessionError::StorageUnavailable)
            ),
            "{failure}"
        );
        assert_no_acceptance(&storage).await;
        sqlx::query(if failure == "commit" {
            "DROP TRIGGER fail_commit"
        } else {
            "DROP TRIGGER fail_statement"
        })
        .execute(storage.pool_for_test())
        .await
        .expect("remove fault");
        let accepted = store
            .admit_input(actor(1), request)
            .await
            .expect("retry after rollback");
        assert!(!accepted.replayed);
        storage.close().await;
    }
}

#[tokio::test]
async fn concurrent_retries_return_one_association_and_one_dispatch_permit() {
    let area = Area::new();
    let storage = Storage::open(&area.database()).await.expect("storage");
    seed(&storage).await;
    let store = storage.sessions();
    let request = input(&store, actor(1), id(3), 5).await;
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..16 {
        let store = store.clone();
        tasks.spawn(async move { store.admit_input(actor(1), request).await });
    }
    let mut first = None;
    let mut fresh = 0;
    while let Some(result) = tasks.join_next().await {
        let result = result.expect("join").expect("admission");
        if !result.replayed {
            fresh += 1;
        }
        if let Some(ref first) = first {
            assert_eq!(&result.command.input, first);
        } else {
            first = Some(result.command.input);
        }
    }
    assert_eq!(fresh, 1);
    assert_eq!(count(&storage, "runs").await, 1);
    let command = first.expect("association").command_id;
    for _ in 0..16 {
        let store = store.clone();
        tasks.spawn(async move { store.admit_input(actor(1), request).await });
    }
    while let Some(result) = tasks.join_next().await {
        result.expect("join").expect("retry");
    }
    let (a, b) = tokio::join!(
        store.claim_dispatch(actor(1), &command),
        store.claim_dispatch(actor(1), &command)
    );
    assert_eq!(
        usize::from(a.expect("claim").is_some()) + usize::from(b.expect("claim").is_some()),
        1
    );
    storage.close().await;
}

#[tokio::test]
async fn competing_actors_are_busy_and_references_conflicts_and_actor_queries_are_scoped() {
    let area = Area::new();
    let storage = Storage::open(&area.database()).await.expect("storage");
    seed(&storage).await;
    let store = storage.sessions();
    let first = input(&store, actor(1), id(3), 5).await;
    let second = input(&store, actor(2), id(3), 5).await;
    let foreign = InputRequest {
        content_ref: first.content_ref,
        ..second
    };
    assert!(matches!(
        store.admit_input(actor(2), foreign).await,
        Err(SessionError::InvalidContent)
    ));
    let (a, b) = tokio::join!(
        store.admit_input(actor(1), first),
        store.admit_input(actor(2), second)
    );
    let (accepted, who, request, other) = match (a, b) {
        (Ok(a), Err(SessionError::Busy)) => (a, actor(1), first, actor(2)),
        (Err(SessionError::Busy), Ok(b)) => (b, actor(2), second, actor(1)),
        other => panic!("unexpected {other:?}"),
    };
    assert!(matches!(
        store
            .command(other, &accepted.command.input.command_id)
            .await,
        Err(SessionError::NotFound)
    ));
    let replacement = input(&store, who, id(3), 5).await;
    assert!(matches!(
        store.admit_input(who, replacement).await,
        Err(SessionError::RequestConflict)
    ));
    let stale = ActorContext {
        auth_generation: 6,
        ..who
    };
    assert!(matches!(
        store.admit_input(stale, request).await,
        Err(SessionError::InvalidActor)
    ));
    let replay = store
        .admit_input(who, request)
        .await
        .expect("replay while busy");
    assert!(replay.replayed);
    assert_eq!(count(&storage, "runs").await, 1);
    storage.close().await;
}

#[tokio::test]
async fn restart_preserves_accepted_and_dispatched_associations_without_redispatch() {
    for claimed in [false, true] {
        let area = Area::new();
        let storage = Storage::open(&area.database()).await.expect("storage");
        seed(&storage).await;
        let store = storage.sessions();
        let request = input(&store, actor(1), id(3), 5).await;
        let accepted = store.admit_input(actor(1), request).await.expect("admit");
        let mut simulated_dispatches = 0;
        if claimed {
            let permit = store
                .claim_dispatch(actor(1), &accepted.command.input.command_id)
                .await
                .expect("claim")
                .expect("first permit");
            assert_eq!(permit.into_input(), accepted.command.input);
            simulated_dispatches += 1;
        }
        storage.close().await;
        drop(store);
        drop(storage);
        let reopened = Storage::open(&area.database()).await.expect("reopen");
        let store = reopened.sessions();
        assert_eq!(
            store
                .recover_interrupted()
                .await
                .expect("startup reconcile"),
            1
        );
        assert_eq!(
            store
                .recover_interrupted()
                .await
                .expect("idempotent reconcile"),
            0
        );
        let replay = store
            .admit_input(actor(1), request)
            .await
            .expect("original result");
        assert!(replay.replayed);
        assert_eq!(replay.command.input, accepted.command.input);
        assert_eq!(replay.command.status, CommandStatus::DeliveryUnknown);
        assert!(
            store
                .claim_dispatch(actor(1), &accepted.command.input.command_id)
                .await
                .expect("no claim")
                .is_none()
        );
        let next = input(&store, actor(1), id(3), 6).await;
        assert!(matches!(
            store.admit_input(actor(1), next).await,
            Err(SessionError::InvalidState)
        ));
        assert_eq!(simulated_dispatches, usize::from(claimed));
        reopened.close().await;
    }
}

#[tokio::test]
async fn cancelling_run_and_releasing_session_keep_foreground_input_blocked() {
    let area = Area::new();
    let storage = Storage::open(&area.database()).await.expect("storage");
    seed(&storage).await;
    let store = storage.sessions();
    let request = input(&store, actor(1), id(3), 5).await;
    store.admit_input(actor(1), request).await.expect("admit");
    sqlx::query("UPDATE runs SET status='cancelling'")
        .execute(storage.pool_for_test())
        .await
        .expect("cancel requested only");
    let next = input(&store, actor(1), id(3), 6).await;
    assert!(matches!(
        store.admit_input(actor(1), next).await,
        Err(SessionError::Busy)
    ));
    sqlx::query("UPDATE runs SET status='completed'")
        .execute(storage.pool_for_test())
        .await
        .expect("reliable terminal fixture");
    sqlx::query("UPDATE sessions SET state='releasing'")
        .execute(storage.pool_for_test())
        .await
        .expect("release lock");
    assert!(matches!(
        store.admit_input(actor(1), next).await,
        Err(SessionError::InvalidState)
    ));
    storage.close().await;
}

#[tokio::test]
async fn command_capacity_and_safe_sequence_limit_reject_without_eviction() {
    let area = Area::new();
    let storage = Storage::open(&area.database()).await.expect("storage");
    seed(&storage).await;
    let store = storage.sessions();
    let request = input(&store, actor(1), id(3), 5).await;
    sqlx::query("UPDATE session_state SET next_position=9007199254740991")
        .execute(storage.pool_for_test())
        .await
        .expect("limit");
    assert!(matches!(
        store.admit_input(actor(1), request).await,
        Err(SessionError::CapacityExhausted)
    ));
    sqlx::query("UPDATE session_state SET next_position=0")
        .execute(storage.pool_for_test())
        .await
        .expect("reset");
    sqlx::query("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000) INSERT INTO session_commands SELECT 'capacity-'||x,?,'request-'||x,'fixture','digest',?,'failed' FROM n")
        .bind(actor(1).client_id.encode()).bind(id(3).encode()).execute(storage.pool_for_test()).await.expect("full ledger");
    assert!(matches!(
        store.admit_input(actor(1), request).await,
        Err(SessionError::CapacityExhausted)
    ));
    assert_eq!(count(&storage, "session_commands").await, 100_000);
    assert_eq!(count(&storage, "runs").await, 0);
    storage.close().await;
}

async fn schema_one(area: &Area) -> SqliteConnection {
    let options = SqliteConnectOptions::new()
        .filename(area.database())
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .expect("schema1 fixture");
    for statement in schema::CORE {
        sqlx::query(statement)
            .execute(&mut connection)
            .await
            .expect("core schema");
    }
    sqlx::query("PRAGMA user_version=1")
        .execute(&mut connection)
        .await
        .expect("version1");
    sqlx::query("INSERT INTO node_meta VALUES(1,1,'original-node',X'04010203','identity.key')")
        .execute(&mut connection)
        .await
        .expect("identity");
    sqlx::query("INSERT INTO node_state VALUES(1,42)")
        .execute(&mut connection)
        .await
        .expect("revision");
    sqlx::query("INSERT INTO clients VALUES('trusted',X'01','web','trusted',7,10,NULL),('revoked',X'02','old','revoked',11,20,30)").execute(&mut connection).await.expect("clients");
    sqlx::query("INSERT INTO management_commands VALUES('trusted','prior-request','device.revoke','prior-digest','prior-result',9999999999)").execute(&mut connection).await.expect("commands");
    sqlx::query(
        "INSERT INTO security_audit VALUES(8,'trusted','device.revoke','revoked','revoked',30)",
    )
    .execute(&mut connection)
    .await
    .expect("audit");
    std::fs::write(
        area.directory.join("identity.key"),
        b"opaque protected identity fixture; never decrypt or replace",
    )
    .expect("key sentinel");
    connection
}

#[tokio::test]
async fn schema_one_upgrade_retains_identity_trust_revocation_audit_and_revision() {
    let area = Area::new();
    schema_one(&area).await.close().await.expect("close");
    let key = std::fs::read(area.directory.join("identity.key")).expect("key");
    let storage = Storage::open(&area.database()).await.expect("upgrade");
    assert_eq!(
        storage.load_node_meta().await.expect("meta"),
        Some(NodeMeta {
            schema_version: SCHEMA_VERSION,
            node_id: "original-node".into(),
            public_key: vec![4, 1, 2, 3],
            key_file_ref: "identity.key".into()
        })
    );
    assert_eq!(storage.list_revision().await.expect("revision"), 42);
    let rows=sqlx::query("SELECT client_id,public_key,status,auth_generation,created_at,revoked_at FROM clients ORDER BY client_id").fetch_all(storage.pool_for_test()).await.expect("clients");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get::<String, _>("status"), "revoked");
    assert_eq!(rows[0].get::<i64, _>("auth_generation"), 11);
    assert_eq!(rows[0].get::<Vec<u8>, _>("public_key"), vec![2]);
    assert_eq!(rows[0].get::<i64, _>("revoked_at"), 30);
    assert_eq!(rows[1].get::<i64, _>("auth_generation"), 7);
    let result: String = sqlx::query_scalar("SELECT result_json FROM management_commands")
        .fetch_one(storage.pool_for_test())
        .await
        .expect("old command");
    assert_eq!(result, "prior-result");
    let audit: i64 = sqlx::query_scalar("SELECT audit_id FROM security_audit")
        .fetch_one(storage.pool_for_test())
        .await
        .expect("old audit");
    assert_eq!(audit, 8);
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(storage.pool_for_test())
        .await
        .expect("version");
    assert_eq!(version, 2);
    assert_eq!(
        std::fs::read(area.directory.join("identity.key")).expect("preserved key"),
        key
    );
    storage.close().await;
    let old = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(area.database())
            .read_only(true),
    )
    .await
    .expect("old version probe");
    let mut old = old;
    let seen: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&mut old)
        .await
        .expect("old gate");
    assert_ne!(seen, 1, "old schema1 initializer must reject this version");
    old.close().await.expect("close");
}

#[tokio::test]
async fn migration_ddl_version_update_and_real_commit_faults_leave_complete_schema_one() {
    for fault in ["ddl", "version", "commit"] {
        let area = Area::new();
        let mut connection = schema_one(&area).await;
        match fault {
            "ddl" => {
                sqlx::query("CREATE TABLE sessions(collision TEXT)")
                    .execute(&mut connection)
                    .await
                    .expect("DDL collision");
            }
            "version" => {
                sqlx::query("CREATE TRIGGER migration_fault BEFORE UPDATE ON node_meta BEGIN SELECT RAISE(ABORT,'fixture'); END").execute(&mut connection).await.expect("version trigger");
            }
            _ => {
                sqlx::query("CREATE TABLE migration_guard(value TEXT REFERENCES clients(client_id) DEFERRABLE INITIALLY DEFERRED)").execute(&mut connection).await.expect("guard");
                sqlx::query("CREATE TRIGGER migration_fault AFTER UPDATE ON node_meta BEGIN INSERT INTO migration_guard VALUES('absent'); END").execute(&mut connection).await.expect("commit fault");
            }
        }
        connection.close().await.expect("close fixture");
        assert!(Storage::open(&area.database()).await.is_err(), "{fault}");
        let mut connection = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(area.database())
                .foreign_keys(true),
        )
        .await
        .expect("inspect");
        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&mut connection)
            .await
            .expect("version");
        let meta: i64 = sqlx::query_scalar("SELECT schema_version FROM node_meta")
            .fetch_one(&mut connection)
            .await
            .expect("meta");
        assert_eq!((version, meta), (1, 1), "{fault}");
        let added:i64=sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name IN ('harnesses','workspaces','session_commands','runs')").fetch_one(&mut connection).await.expect("added structures");
        assert_eq!(added, 0, "all DDL rolled back: {fault}");
        let clients: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM clients")
            .fetch_one(&mut connection)
            .await
            .expect("preserved clients");
        assert_eq!(clients, 2);
        connection.close().await.expect("close inspect");
    }
}

#[tokio::test]
async fn unknown_inconsistent_and_incomplete_schemas_are_rejected_read_only() {
    for fault in ["unknown", "versions", "missing", "constraints"] {
        let area = Area::new();
        let mut connection = schema_one(&area).await;
        match fault {
            "unknown" => {
                sqlx::query("PRAGMA user_version=77")
                    .execute(&mut connection)
                    .await
                    .expect("unknown");
            }
            "versions" => {
                sqlx::query("UPDATE node_meta SET schema_version=2")
                    .execute(&mut connection)
                    .await
                    .expect("mismatch");
            }
            "missing" => {
                sqlx::query("DROP TABLE management_commands")
                    .execute(&mut connection)
                    .await
                    .expect("missing");
            }
            _ => {
                sqlx::query("ALTER TABLE clients ADD COLUMN unexpected TEXT")
                    .execute(&mut connection)
                    .await
                    .expect("shape");
            }
        }
        connection.close().await.expect("close");
        let before = std::fs::read(area.database()).expect("before");
        let error = Storage::open(&area.database())
            .await
            .err()
            .expect("must reject");
        assert!(
            matches!(
                error,
                StorageError::UnknownSchema(_) | StorageError::UnrecognizedSchema
            ),
            "{fault}"
        );
        assert_eq!(
            std::fs::read(area.database()).expect("after"),
            before,
            "no rewrite {fault}"
        );
    }
}

#[tokio::test]
async fn dispatch_claim_rechecks_session_release_and_schema_two_rejects_missing_index() {
    let area = Area::new();
    let storage = Storage::open(&area.database()).await.expect("storage");
    seed(&storage).await;
    let store = storage.sessions();
    let request = input(&store, actor(1), id(3), 5).await;
    let accepted = store.admit_input(actor(1), request).await.expect("admit");
    sqlx::query("UPDATE sessions SET state='releasing'")
        .execute(storage.pool_for_test())
        .await
        .expect("release starts");
    assert!(matches!(
        store
            .claim_dispatch(actor(1), &accepted.command.input.command_id)
            .await,
        Err(SessionError::InvalidState)
    ));
    assert_eq!(
        store
            .command(actor(1), &accepted.command.input.command_id)
            .await
            .expect("record")
            .status,
        CommandStatus::Accepted
    );
    sqlx::query("DROP INDEX one_foreground_run")
        .execute(storage.pool_for_test())
        .await
        .expect("damaged constraint");
    storage.close().await;
    drop(store);
    drop(storage);
    let before = std::fs::read(area.database()).expect("before");
    assert!(matches!(
        Storage::open(&area.database()).await,
        Err(StorageError::UnrecognizedSchema)
    ));
    assert_eq!(std::fs::read(area.database()).expect("after"), before);
}

#[tokio::test]
async fn independent_sqlite_connections_serialize_foreground_admission() {
    let area = Area::new();
    let first = Storage::open(&area.database()).await.expect("storage");
    seed(&first).await;
    let second = Storage::open(&area.database())
        .await
        .expect("independent pool");
    let a = first.sessions();
    let b = second.sessions();
    let one = input(&a, actor(1), id(3), 5).await;
    let two = input(&b, actor(2), id(3), 6).await;
    let (left, right) = tokio::join!(a.admit_input(actor(1), one), b.admit_input(actor(2), two));
    assert!(matches!(
        (left, right),
        (Ok(_), Err(SessionError::Busy)) | (Err(SessionError::Busy), Ok(_))
    ));
    assert_eq!(count(&first, "runs").await, 1);
    second.close().await;
    first.close().await;
}
