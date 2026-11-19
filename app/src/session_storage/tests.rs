use super::*;
use crate::{
    domain::EntityId,
    error::StorageError,
    storage::{NodeMeta, SCHEMA_VERSION},
};
use sqlx::{Connection, Row, SqliteConnection, sqlite::SqlitePoolOptions};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct AreaRoots {
    directory: PathBuf,
}

struct Area {
    roots: std::sync::Arc<AreaRoots>,
    scopes: std::sync::Mutex<Vec<std::sync::Arc<crate::sqlite_fixture_scope::SqliteFixtureScope>>>,
}

impl std::ops::Deref for Area {
    type Target = AreaRoots;
    fn deref(&self) -> &AreaRoots {
        &self.roots
    }
}

impl Area {
    async fn open(
        &self,
    ) -> Result<
        crate::sqlite_fixture_scope::FixtureStorage,
        std::sync::Arc<crate::error::StorageError>,
    > {
        let scope = std::sync::Arc::new(crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
            &self.database(),
            self.roots.clone(),
        ));
        self.scopes
            .lock()
            .expect("fixture scopes")
            .push(scope.clone());
        scope.open().await
    }

    async fn raw_scope(&self) -> std::sync::Arc<crate::sqlite_fixture_scope::SqliteFixtureScope> {
        let scope = std::sync::Arc::new(crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
            &self.database(),
            self.roots.clone(),
        ));
        self.scopes
            .lock()
            .expect("fixture scopes")
            .push(scope.clone());
        scope.start_raw().await;
        scope
    }

    async fn observe(&self) {
        let scopes = std::mem::take(&mut *self.scopes.lock().expect("fixture scopes"));
        for scope in scopes {
            scope.stop_and_observe().await;
        }
    }

    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "acp-fbl-v15-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("fixture clock")
                .as_nanos()
        ));
        std::fs::create_dir(&directory).expect("owned fixture directory");
        Self {
            roots: std::sync::Arc::new(AreaRoots { directory }),
            scopes: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn database(&self) -> PathBuf {
        self.directory.join("node.sqlite3")
    }
}

impl Drop for AreaRoots {
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
                .starts_with("acp-fbl-v15-")
        );
        if self.directory.exists() {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
}

fn id(value: u8) -> String {
    EntityId::from_bytes([value; 16]).encode()
}

async fn count_legacy_objects(pool: &sqlx::SqlitePool) -> i64 {
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM main.sqlite_schema
         UNION ALL
         SELECT tbl_name FROM main.sqlite_schema",
    )
    .fetch_all(pool)
    .await
    .expect("sqlite_schema names");
    i64::try_from(
        names
            .into_iter()
            .filter(|name| schema::is_retired_legacy_object(name))
            .count(),
    )
    .expect("legacy count")
}

async fn schema_one(area: &Area) -> SqliteConnection {
    let raw_scope = area.raw_scope().await;
    let options = raw_scope
        .options()
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
    sqlx::query("INSERT INTO clients VALUES('trusted',X'01','web','trusted',7,10,NULL),('revoked',X'02','old','revoked',11,20,30)")
        .execute(&mut connection)
        .await
        .expect("clients");
    sqlx::query("INSERT INTO management_commands VALUES('trusted','prior-request','device.revoke','prior-digest','prior-result',9999999999)")
        .execute(&mut connection)
        .await
        .expect("commands");
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

async fn schema_fourteen_with_cyclic_legacy_rows(area: &Area) {
    let storage = area.open().await.expect("bootstrap current layout");
    storage
        .write_node_meta(&NodeMeta {
            schema_version: SCHEMA_VERSION,
            node_id: "v14-node".into(),
            public_key: vec![4, 1, 2, 3],
            key_file_ref: "identity.key".into(),
        })
        .await
        .expect("node_meta");
    let pool = storage.pool_for_test();
    for definition in schema::LEGACY_SESSION_DEFINITIONS {
        sqlx::query(definition)
            .execute(pool)
            .await
            .expect("legacy ddl");
    }
    sqlx::query("INSERT INTO clients VALUES(?,X'01','trusted','trusted',7,10,NULL)")
        .bind(id(1))
        .execute(pool)
        .await
        .expect("client");
    sqlx::query("INSERT INTO harnesses VALUES('h-active','active-harness','codex','[]',NULL)")
        .execute(pool)
        .await
        .expect("harness");
    sqlx::query("INSERT INTO workspaces VALUES('w-active','D:/synthetic/workspace')")
        .execute(pool)
        .await
        .expect("workspace");
    sqlx::query("INSERT INTO product_sources VALUES(?,'active-source')")
        .bind(id(2))
        .execute(pool)
        .await
        .expect("source");
    sqlx::query("INSERT INTO product_targets VALUES(?,?,'native-target',NULL)")
        .bind(id(3))
        .bind(id(2))
        .execute(pool)
        .await
        .expect("target");
    sqlx::query("INSERT INTO security_audit VALUES(99,?,'session.legacy','target','accepted',50)")
        .bind(id(1))
        .execute(pool)
        .await
        .expect("audit");
    sqlx::query(
        "INSERT INTO sessions VALUES('legacy-s','h-active','w-active','released',NULL,NULL,NULL)",
    )
    .execute(pool)
    .await
    .expect("session");
    sqlx::query(
        "INSERT INTO content_objects VALUES('legacy-c','legacy-s',?,7,'input',X'616263',?,3)",
    )
    .bind(id(1))
    .bind("a".repeat(64))
    .execute(pool)
    .await
    .expect("content");
    sqlx::query("INSERT INTO session_commands VALUES('legacy-cmd',?,'req-1','session.input','digest','legacy-s','completed')")
        .bind(id(1))
        .execute(pool)
        .await
        .expect("command");
    sqlx::query("INSERT INTO turns VALUES('legacy-turn','legacy-s','legacy-cmd',0)")
        .execute(pool)
        .await
        .expect("turn");
    sqlx::query(
        "INSERT INTO runs VALUES('legacy-run','legacy-s','legacy-cmd','legacy-turn','completed')",
    )
    .execute(pool)
    .await
    .expect("run");
    sqlx::query("INSERT INTO conversation_items(item_id,session_id,turn_id,source_id,first_position,kind,content_ref,content_version,committed_version) VALUES('legacy-item','legacy-s','legacy-turn','src-1',0,'user','legacy-c',1,1)")
        .execute(pool)
        .await
        .expect("item");
    seed_legacy_approval_and_history_rows(pool).await;
    sqlx::query("UPDATE main.node_meta SET schema_version=14")
        .execute(pool)
        .await
        .expect("meta 14");
    sqlx::query("PRAGMA main.user_version=14")
        .execute(pool)
        .await
        .expect("pragma 14");
    std::fs::write(
        area.directory.join("identity.key"),
        b"opaque protected identity fixture; never decrypt or replace",
    )
    .expect("key sentinel");
    storage.close().await;
    area.observe().await;
}

async fn seed_legacy_approval_and_history_rows(pool: &sqlx::SqlitePool) {
    sqlx::query("INSERT INTO approval_connections VALUES('legacy-inst',1,'closed')")
        .execute(pool)
        .await
        .expect("conn");
    sqlx::query("INSERT INTO approval_run_sources VALUES('legacy-run','legacy-s','legacy-turn','legacy-inst',1,'native-1','invalid','closed')")
        .execute(pool)
        .await
        .expect("run source");
    sqlx::query("INSERT INTO approvals(approval_id,session_id,run_id,turn_id,instance_id,connection_generation,native_session_id,native_request_id,source_digest,request_kind,protocol,protocol_version,raw_request,scope_raw,status,selected_choice_id) VALUES('legacy-appr','legacy-s','legacy-run','legacy-turn','legacy-inst',1,'native-1','req-1','digest','tool_permission','acp','1',X'7b7d',X'7b7d','decided',NULL)")
        .execute(pool)
        .await
        .expect("approval");
    sqlx::query("INSERT INTO approval_options VALUES('legacy-choice','legacy-appr',0,'allow','Allow','allow_once')")
        .execute(pool)
        .await
        .expect("option");
    sqlx::query(
        "UPDATE approvals SET selected_choice_id='legacy-choice' WHERE approval_id='legacy-appr'",
    )
    .execute(pool)
    .await
    .expect("bidirectional fk cycle");
    sqlx::query("INSERT INTO approval_deliveries VALUES('legacy-appr','transport_written','claim-1','write_completed',1)")
        .execute(pool)
        .await
        .expect("delivery");
    sqlx::query("INSERT INTO approval_audit_links VALUES(99,'legacy-appr','legacy-cmd','basis',7,'legacy-inst',1)")
        .execute(pool)
        .await
        .expect("audit link");
    sqlx::query("INSERT INTO session_state VALUES('legacy-s',1,1,1)")
        .execute(pool)
        .await
        .expect("session state");
    sqlx::query("INSERT INTO history_integrity VALUES('legacy-s','legacy-run','ok',0,0,0)")
        .execute(pool)
        .await
        .expect("history integrity");
    sqlx::query("INSERT INTO history_content_pins VALUES('legacy-pin','legacy-c')")
        .execute(pool)
        .await
        .expect("pin");
}

#[tokio::test]
async fn fresh_v0_to_v15_creates_catalog_tables_without_legacy_tables_or_indexes() {
    assert_eq!(SCHEMA_VERSION, 15);
    let area = Area::new();
    let storage = area.open().await.expect("fresh v15");
    storage
        .write_node_meta(&NodeMeta {
            schema_version: SCHEMA_VERSION,
            node_id: "fresh-node".into(),
            public_key: vec![4; 65],
            key_file_ref: "identity.key".into(),
        })
        .await
        .expect("write meta");
    let pool = storage.pool_for_test();
    let pragma: i64 = sqlx::query_scalar("PRAGMA main.user_version")
        .fetch_one(pool)
        .await
        .expect("pragma");
    assert_eq!(pragma, 15);
    let catalog_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM main.sqlite_schema WHERE type='table' AND name IN ('harnesses','workspaces')",
    )
    .fetch_one(pool)
    .await
    .expect("catalog count");
    assert_eq!(catalog_count, 2);
    assert_eq!(count_legacy_objects(pool).await, 0);
    assert!(
        sqlx::query("PRAGMA main.foreign_key_check")
            .fetch_all(pool)
            .await
            .expect("fk check")
            .is_empty()
    );
    storage.close().await;
    area.observe().await;
    let reopened = area.open().await.expect("reopen v15");
    assert_eq!(
        reopened
            .load_node_meta()
            .await
            .expect("load")
            .expect("meta")
            .schema_version,
        15
    );
    reopened.close().await;
    area.observe().await;
}

#[tokio::test]
async fn v14_with_bidirectional_fk_cycle_upgrades_to_v15_and_cleans_all_legacy_objects() {
    let area = Area::new();
    schema_fourteen_with_cyclic_legacy_rows(&area).await;
    let storage = area.open().await.expect("v14 -> v15 migration");
    let pool = storage.pool_for_test();
    let pragma: i64 = sqlx::query_scalar("PRAGMA main.user_version")
        .fetch_one(pool)
        .await
        .expect("pragma");
    let meta_ver: i64 = sqlx::query_scalar("SELECT schema_version FROM main.node_meta")
        .fetch_one(pool)
        .await
        .expect("meta");
    assert_eq!((pragma, meta_ver), (15, 15));
    assert_eq!(count_legacy_objects(pool).await, 0);
    let harness_name: String =
        sqlx::query_scalar("SELECT name FROM main.harnesses WHERE harness_id='h-active'")
            .fetch_one(pool)
            .await
            .expect("harness preserved");
    assert_eq!(harness_name, "active-harness");
    let workspace_path: String = sqlx::query_scalar(
        "SELECT canonical_path FROM main.workspaces WHERE workspace_id='w-active'",
    )
    .fetch_one(pool)
    .await
    .expect("workspace preserved");
    assert_eq!(workspace_path, "D:/synthetic/workspace");
    let target_native: String =
        sqlx::query_scalar("SELECT native_id FROM main.product_targets WHERE target_id=?")
            .bind(id(3))
            .fetch_one(pool)
            .await
            .expect("product target preserved");
    assert_eq!(target_native, "native-target");
    assert!(
        sqlx::query("PRAGMA main.foreign_key_check")
            .fetch_all(pool)
            .await
            .expect("fk check")
            .is_empty()
    );
    storage.close().await;
    area.observe().await;
}

#[tokio::test]
async fn v14_to_v15_fault_injection_rolls_back_atomically_and_recovers_on_retry() {
    for fault in [
        schema::MigrationFault::LegacyCleanupDdl,
        schema::MigrationFault::LegacyCleanupMetadata,
        schema::MigrationFault::LegacyCleanupPragma,
        schema::MigrationFault::Validation,
        schema::MigrationFault::Commit,
        schema::MigrationFault::DeferredCommit,
    ] {
        let area = Area::new();
        schema_fourteen_with_cyclic_legacy_rows(&area).await;
        let raw_scope = area.raw_scope().await;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(raw_scope.options().foreign_keys(true))
            .await
            .expect("pool");
        assert!(schema::initialize_for_test(&pool, fault).await.is_err());
        let pragma: i64 = sqlx::query_scalar("PRAGMA main.user_version")
            .fetch_one(&pool)
            .await
            .expect("pragma");
        let meta: i64 = sqlx::query_scalar("SELECT schema_version FROM main.node_meta")
            .fetch_one(&pool)
            .await
            .expect("meta");
        assert_eq!((pragma, meta), (14, 14), "{fault:?}");
        let choice: Option<String> = sqlx::query_scalar(
            "SELECT selected_choice_id FROM main.approvals WHERE approval_id='legacy-appr'",
        )
        .fetch_one(&pool)
        .await
        .expect("cyclic fk preserved on rollback");
        assert_eq!(choice.as_deref(), Some("legacy-choice"), "{fault:?}");
        pool.close().await;
        area.observe().await;
        let upgraded = area.open().await.expect("retry succeeds");
        assert_eq!(count_legacy_objects(upgraded.pool_for_test()).await, 0);
        upgraded.close().await;
        area.observe().await;
    }
}

#[tokio::test]
async fn schema_one_upgrade_retains_identity_trust_revocation_audit_and_revision() {
    let area = Area::new();
    schema_one(&area).await.close().await.expect("close");
    area.observe().await;
    let key = std::fs::read(area.directory.join("identity.key")).expect("key");
    let storage = area.open().await.expect("upgrade");
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
    let rows = sqlx::query(
        "SELECT client_id,public_key,status,auth_generation,created_at,revoked_at FROM clients ORDER BY client_id",
    )
    .fetch_all(storage.pool_for_test())
    .await
    .expect("clients");
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
    assert_eq!(version, SCHEMA_VERSION);
    assert_eq!(count_legacy_objects(storage.pool_for_test()).await, 0);
    assert_eq!(
        std::fs::read(area.directory.join("identity.key")).expect("preserved key"),
        key
    );
    storage.close().await;
    area.observe().await;
}

#[tokio::test]
async fn migration_ddl_version_update_and_real_commit_faults_leave_complete_schema_one() {
    for fault in ["ddl", "version", "commit"] {
        let area = Area::new();
        let mut connection = schema_one(&area).await;
        match fault {
            "ddl" => {
                sqlx::query("CREATE TABLE harnesses(collision TEXT)")
                    .execute(&mut connection)
                    .await
                    .expect("DDL collision");
            }
            "version" => {
                sqlx::query("CREATE TRIGGER migration_fault BEFORE UPDATE ON node_meta BEGIN SELECT RAISE(ABORT,'fixture'); END")
                    .execute(&mut connection)
                    .await
                    .expect("version trigger");
            }
            _ => {
                sqlx::query("CREATE TABLE migration_guard(value TEXT REFERENCES clients(client_id) DEFERRABLE INITIALLY DEFERRED)")
                    .execute(&mut connection)
                    .await
                    .expect("guard");
                sqlx::query("CREATE TRIGGER migration_fault AFTER UPDATE ON node_meta BEGIN INSERT INTO migration_guard VALUES('absent'); END")
                    .execute(&mut connection)
                    .await
                    .expect("commit fault");
            }
        }
        connection.close().await.expect("close fixture");
        area.observe().await;
        assert!(area.open().await.is_err(), "{fault}");
        let mut connection =
            SqliteConnection::connect_with(&area.raw_scope().await.options().foreign_keys(true))
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
        let added: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_schema WHERE name IN ('workspaces','product_control','node_request_registrations')",
        )
        .fetch_one(&mut connection)
        .await
        .expect("added structures");
        assert_eq!(added, 0, "all DDL rolled back: {fault}");
        let clients: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM clients")
            .fetch_one(&mut connection)
            .await
            .expect("preserved clients");
        assert_eq!(clients, 2);
        connection.close().await.expect("close inspect");
        area.observe().await;
    }
}

#[tokio::test]
async fn unknown_inconsistent_and_v15_tampered_or_legacy_residual_schemas_are_rejected_read_only() {
    for fault in [
        "unknown_16",
        "unknown_77",
        "versions",
        "missing",
        "constraints",
        "legacy_table_sessions",
        "legacy_table_approvals",
        "legacy_index_tool_reference_key",
        "legacy_view_sessions",
        "harnesses_trigger",
        "missing_workspaces",
    ] {
        let area = Area::new();
        let storage = area.open().await.expect("v15");
        storage
            .write_node_meta(&NodeMeta {
                schema_version: SCHEMA_VERSION,
                node_id: "v15-node".into(),
                public_key: vec![4, 1, 2, 3],
                key_file_ref: "identity.key".into(),
            })
            .await
            .expect("meta");
        storage.close().await;
        area.observe().await;

        let mut connection =
            SqliteConnection::connect_with(&area.raw_scope().await.options().foreign_keys(false))
                .await
                .expect("tamper connection");
        sqlx::query(
            "CREATE TABLE sentinel(value TEXT); INSERT INTO sentinel VALUES('preserve-me');",
        )
        .execute(&mut connection)
        .await
        .expect("sentinel");
        apply_v15_tamper(&mut connection, fault).await;
        sqlx::query("PRAGMA journal_mode=DELETE")
            .execute(&mut connection)
            .await
            .expect("checkpoint");
        connection.close().await.expect("close");
        area.observe().await;

        let before = std::fs::read(area.database()).expect("before");
        let error = area.open().await.err().expect("must reject");
        match fault {
            "unknown_16" => assert!(matches!(error.as_ref(), StorageError::UnknownSchema(16))),
            "unknown_77" => assert!(matches!(error.as_ref(), StorageError::UnknownSchema(77))),
            _ => assert!(
                matches!(error.as_ref(), StorageError::UnrecognizedSchema),
                "{fault}: {error:?}"
            ),
        }
        assert_eq!(
            std::fs::read(area.database()).expect("after"),
            before,
            "no rewrite {fault}"
        );
    }

    let area = Area::new();
    let storage = area.open().await.expect("v15 temp shadow test");
    let pool = storage.pool_for_test();
    let mut conn = pool.acquire().await.expect("conn");
    sqlx::query("CREATE TEMP TABLE sessions(stale TEXT)")
        .execute(&mut *conn)
        .await
        .expect("temp sessions");
    assert!(matches!(
        schema::validate(&mut conn, 15).await,
        Err(StorageError::UnrecognizedSchema)
    ));
    sqlx::query("DROP TABLE temp.sessions")
        .execute(&mut *conn)
        .await
        .expect("drop temp sessions");
    sqlx::query("CREATE TEMP VIEW tool_reference_key AS SELECT 1 AS stale")
        .execute(&mut *conn)
        .await
        .expect("temp view");
    assert!(matches!(
        schema::validate(&mut conn, 15).await,
        Err(StorageError::UnrecognizedSchema)
    ));
    drop(conn);
    storage.close().await;
    area.observe().await;
}

async fn apply_v15_tamper(connection: &mut sqlx::SqliteConnection, fault: &str) {
    let sql = match fault {
        "unknown_16" => "PRAGMA user_version=16",
        "unknown_77" => "PRAGMA user_version=77",
        "versions" => "UPDATE node_meta SET schema_version=14",
        "missing" => "DROP TABLE management_commands",
        "constraints" => "ALTER TABLE clients ADD COLUMN unexpected TEXT",
        "legacy_table_sessions" => "CREATE TABLE sessions(stale TEXT)",
        "legacy_table_approvals" => "CREATE TABLE approvals(stale TEXT)",
        "legacy_index_tool_reference_key" => {
            "CREATE INDEX tool_reference_key ON node_meta(node_id)"
        }
        "legacy_view_sessions" => "CREATE VIEW sessions AS SELECT 1 AS stale",
        "harnesses_trigger" => {
            "CREATE TRIGGER h_trig AFTER INSERT ON harnesses BEGIN SELECT 1; END"
        }
        _ => "DROP TABLE workspaces",
    };
    sqlx::query(sql)
        .execute(connection)
        .await
        .expect("apply tamper fault");
}
