use super::super::schema::{self, MigrationFault};
use crate::domain::EntityId;
use sha2::{Digest, Sha256};
use sqlx::{Connection, Row, SqliteConnection, sqlite::SqlitePoolOptions};
use std::path::PathBuf;

pub(super) struct AreaRoots {
    root: PathBuf,
    directory: PathBuf,
}
pub(super) struct Area {
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
    pub(super) async fn open(
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
    pub(super) async fn raw_scope(
        &self,
    ) -> std::sync::Arc<crate::sqlite_fixture_scope::SqliteFixtureScope> {
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
    pub(super) async fn observe(&self) {
        let scopes = std::mem::take(&mut *self.scopes.lock().expect("fixture scopes"));
        for scope in scopes {
            scope.stop_and_observe().await;
        }
    }

    pub(super) fn new() -> Self {
        let root = std::env::temp_dir().canonicalize().unwrap();
        let directory = root.join(format!("acpr-schema3-{}", crate::random::hex(12).unwrap()));
        std::fs::create_dir(&directory).unwrap();
        Self {
            roots: std::sync::Arc::new(AreaRoots { root, directory }),
            scopes: std::sync::Mutex::new(Vec::new()),
        }
    }
    pub(super) fn database(&self) -> PathBuf {
        self.directory.join("test.sqlite3")
    }
}
impl Drop for AreaRoots {
    fn drop(&mut self) {
        if self.directory.parent() == Some(self.root.as_path())
            && self
                .directory
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("acpr-schema3-"))
        {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
}
fn id(n: u8) -> String {
    EntityId::from_bytes([n; 16]).encode()
}

pub(super) async fn connect(area: &Area) -> SqliteConnection {
    let scope = area.raw_scope().await;
    SqliteConnection::connect_with(&scope.options().create_if_missing(true).foreign_keys(true))
        .await
        .unwrap()
}

pub(super) async fn legacy(area: &Area, version: i64) {
    let mut connection = connect(area).await;
    for definition in schema::CORE {
        sqlx::query(definition)
            .execute(&mut connection)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO node_meta VALUES(1,?,'original-node',X'040102','identity.key')")
        .bind(version)
        .execute(&mut connection)
        .await
        .unwrap();
    sqlx::query("INSERT INTO node_state VALUES(1,42)")
        .execute(&mut connection)
        .await
        .unwrap();
    sqlx::query("INSERT INTO clients VALUES(?,X'01','original','trusted',7,10,NULL),(?,X'02','revoked','revoked',11,20,30)").bind(id(1)).bind(id(2)).execute(&mut connection).await.unwrap();
    sqlx::query("INSERT INTO management_commands VALUES(?,'prior-request','device.revoke','digest','original-result',99999999)").bind(id(1)).execute(&mut connection).await.unwrap();
    sqlx::query("INSERT INTO security_audit VALUES(8,?,'device.revoke',?,'revoked',30)")
        .bind(id(1))
        .bind(id(2))
        .execute(&mut connection)
        .await
        .unwrap();
    if version == 2 {
        for definition in schema::SESSION {
            sqlx::query(definition)
                .execute(&mut connection)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO harnesses VALUES('legacy-h','old','old.exe','[]',NULL)")
            .execute(&mut connection)
            .await
            .unwrap();
        sqlx::query("INSERT INTO workspaces VALUES('legacy-w','D:/synthetic-test')")
            .execute(&mut connection)
            .await
            .unwrap();
        sqlx::query("INSERT INTO sessions VALUES('legacy-s','legacy-h','legacy-w','released','scope','native','{}')").execute(&mut connection).await.unwrap();
        let body = b"synthetic-legacy-body";
        sqlx::query(
            "INSERT INTO content_objects VALUES('legacy-content','legacy-s',?,7,'input',?,?,?)",
        )
        .bind(id(1))
        .bind(body.as_slice())
        .bind(hex::encode(Sha256::digest(body)))
        .bind(i64::try_from(body.len()).unwrap())
        .execute(&mut connection)
        .await
        .unwrap();
        sqlx::query("INSERT INTO session_commands VALUES('legacy-c',?,'legacy-r','session.input','digest','legacy-s','completed')").bind(id(1)).execute(&mut connection).await.unwrap();
        sqlx::query("INSERT INTO turns VALUES('legacy-t','legacy-s','legacy-c',0)")
            .execute(&mut connection)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO runs VALUES('legacy-run','legacy-s','legacy-c','legacy-t','completed')",
        )
        .execute(&mut connection)
        .await
        .unwrap();
        sqlx::query("INSERT INTO conversation_items(item_id,session_id,turn_id,source_id,first_position,kind,content_ref,content_version,committed_version) VALUES('legacy-i','legacy-s','legacy-t','legacy-source',0,'user','legacy-content',1,1)").execute(&mut connection).await.unwrap();
        sqlx::query("INSERT INTO approval_connections VALUES('legacy-instance',1,'closed')")
            .execute(&mut connection)
            .await
            .unwrap();
        sqlx::query("INSERT INTO approvals(approval_id,session_id,run_id,turn_id,instance_id,connection_generation,native_session_id,native_request_id,source_digest,request_kind,protocol,protocol_version,raw_request,scope_raw,status) VALUES('legacy-a','legacy-s','legacy-run','legacy-t','legacy-instance',1,'native','1','digest','tool_permission','acp','1',X'7b7d',X'7b7d','invalid')").execute(&mut connection).await.unwrap();
    }
    sqlx::query(&format!("PRAGMA user_version={version}"))
        .execute(&mut connection)
        .await
        .unwrap();
    connection.close().await.unwrap();
    area.observe().await;
    std::fs::write(
        area.directory.join("identity.key"),
        b"synthetic protected identity fixture",
    )
    .unwrap();
}

pub(super) async fn assert_preserved(
    connection: &mut SqliteConnection,
    old_version: i64,
    expected_version: i64,
) {
    let row = sqlx::query("SELECT schema_version,node_id,public_key,key_file_ref FROM node_meta")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    assert_eq!(row.get::<i64, _>("schema_version"), expected_version);
    assert_eq!(row.get::<String, _>("node_id"), "original-node");
    assert_eq!(row.get::<Vec<u8>, _>("public_key"), [4, 1, 2]);
    assert_eq!(row.get::<String, _>("key_file_ref"), "identity.key");
    let clients: Vec<(String, String, i64, Option<i64>)> = sqlx::query_as(
        "SELECT client_id,status,auth_generation,revoked_at FROM clients ORDER BY auth_generation",
    )
    .fetch_all(&mut *connection)
    .await
    .unwrap();
    assert_eq!(
        clients,
        [
            (id(1), "trusted".into(), 7, None),
            (id(2), "revoked".into(), 11, Some(30))
        ]
    );
    let revision: i64 = sqlx::query_scalar("SELECT revision FROM node_state")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    assert_eq!(revision, 42);
    let command: String = sqlx::query_scalar("SELECT result_json FROM management_commands")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    assert_eq!(command, "original-result");
    let audit: (i64, String) = sqlx::query_as("SELECT audit_id,outcome FROM security_audit")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    assert_eq!(audit, (8, "revoked".into()));
    if old_version == 2 {
        let body: Vec<u8> = sqlx::query_scalar("SELECT body FROM content_objects")
            .fetch_one(&mut *connection)
            .await
            .unwrap();
        assert_eq!(body, b"synthetic-legacy-body");
        let approval: (String, Vec<u8>, Vec<u8>) =
            sqlx::query_as("SELECT status,raw_request,scope_raw FROM approvals")
                .fetch_one(&mut *connection)
                .await
                .unwrap();
        assert_eq!(approval, ("invalid".into(), b"{}".to_vec(), b"{}".to_vec()));
        let item: (String, String, i64) =
            sqlx::query_as("SELECT item_id,content_ref,committed_version FROM conversation_items")
                .fetch_one(&mut *connection)
                .await
                .unwrap();
        assert_eq!(item, ("legacy-i".into(), "legacy-content".into(), 1));
    }
    let pragma: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(connection)
        .await
        .unwrap();
    assert_eq!(pragma, expected_version);
}

#[tokio::test]
async fn schema_one_and_two_upgrade_preserve_identity_revocation_and_legacy_bodies() {
    for version in [1, 2] {
        let area = Area::new();
        legacy(&area, version).await;
        let storage = area.open().await.unwrap();
        storage.close().await;
        area.observe().await;
        let mut connection = connect(&area).await;
        assert_preserved(&mut connection, version, crate::storage::SCHEMA_VERSION).await;
        let epoch: String = sqlx::query_scalar("SELECT request_epoch FROM product_control")
            .fetch_one(&mut connection)
            .await
            .unwrap();
        assert!(EntityId::decode(&epoch).is_ok());
        sqlx::query("UPDATE product_control SET time_floor_ms=12345")
            .execute(&mut connection)
            .await
            .unwrap();
        connection.close().await.unwrap();
        area.observe().await;
        let storage = area.open().await.unwrap();
        storage.close().await;
        area.observe().await;
        let mut connection = connect(&area).await;
        let state: (String, i64) =
            sqlx::query_as("SELECT request_epoch,time_floor_ms FROM product_control")
                .fetch_one(&mut connection)
                .await
                .unwrap();
        assert_eq!(state, (epoch, 12345));
        assert_eq!(
            std::fs::read(area.directory.join("identity.key")).unwrap(),
            b"synthetic protected identity fixture"
        );
        connection.close().await.unwrap();
        area.observe().await;
    }
}

#[tokio::test]
async fn every_migration_checkpoint_rolls_back_both_legacy_versions() {
    for version in [1, 2] {
        for fault in [
            MigrationFault::Ddl,
            MigrationFault::Metadata,
            MigrationFault::Pragma,
            MigrationFault::Validation,
            MigrationFault::Commit,
        ] {
            let area = Area::new();
            legacy(&area, version).await;
            let raw_scope = area.raw_scope().await;
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(raw_scope.options().foreign_keys(true))
                .await
                .unwrap();
            assert!(schema::initialize_for_test(&pool, fault).await.is_err());
            pool.close().await;
            area.observe().await;
            let mut connection = connect(&area).await;
            assert_preserved(&mut connection, version, version).await;
            let added: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_schema WHERE name GLOB 'product_*'",
            )
            .fetch_one(&mut connection)
            .await
            .unwrap();
            assert_eq!(added, 0);
            connection.close().await.unwrap();
            area.observe().await;
        }
    }
}

#[tokio::test]
async fn actual_deferred_commit_failure_rolls_back_schema_three_and_versions() {
    for version in [1, 2] {
        let area = Area::new();
        legacy(&area, version).await;
        let mut connection = connect(&area).await;
        sqlx::query("CREATE TABLE commit_guard(actor TEXT REFERENCES clients(client_id) DEFERRABLE INITIALLY DEFERRED)").execute(&mut connection).await.unwrap();
        sqlx::query("CREATE TRIGGER real_commit_fault AFTER UPDATE ON node_meta BEGIN INSERT INTO commit_guard VALUES('absent'); END").execute(&mut connection).await.unwrap();
        connection.close().await.unwrap();
        area.observe().await;
        assert!(area.open().await.is_err());
        let mut connection = connect(&area).await;
        assert_preserved(&mut connection, version, version).await;
        let added: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name GLOB 'product_*'")
                .fetch_one(&mut connection)
                .await
                .unwrap();
        assert_eq!(added, 0);
        connection.close().await.unwrap();
        area.observe().await;
    }
}

#[tokio::test]
async fn malformed_control_missing_schema_and_partial_upgrade_are_rejected_read_only() {
    for fault in [
        "epoch",
        "floor",
        "missing",
        "partial",
        "version",
        "metadata2",
        "metadata3",
    ] {
        let area = Area::new();
        legacy(&area, 2).await;
        if !["partial", "metadata2"].contains(&fault) {
            let storage = area.open().await.unwrap();
            storage.close().await;
            area.observe().await;
        }
        let mut connection = connect(&area).await;
        match fault {
            "metadata2" | "metadata3" => {
                sqlx::query("DELETE FROM node_meta")
                    .execute(&mut connection)
                    .await
                    .unwrap();
            }
            "epoch" => {
                sqlx::query("UPDATE product_control SET request_epoch='!!!!!!!!!!!!!!!!!!!!!!'")
                    .execute(&mut connection)
                    .await
                    .unwrap();
            }
            "floor" => {
                sqlx::query("DELETE FROM product_control")
                    .execute(&mut connection)
                    .await
                    .unwrap();
            }
            "missing" => {
                sqlx::query("DROP INDEX product_one_resource_owner")
                    .execute(&mut connection)
                    .await
                    .unwrap();
            }
            "partial" => {
                sqlx::query("CREATE TABLE product_control(unrecognized TEXT)")
                    .execute(&mut connection)
                    .await
                    .unwrap();
            }
            _ => {
                sqlx::query("UPDATE node_meta SET schema_version=2")
                    .execute(&mut connection)
                    .await
                    .unwrap();
            }
        }
        connection.close().await.unwrap();
        area.observe().await;
        let before = std::fs::read(area.database()).unwrap();
        assert!(area.open().await.is_err(), "{fault}");
        assert_eq!(
            std::fs::read(area.database()).unwrap(),
            before,
            "preflight mutation: {fault}"
        );
    }
}

#[tokio::test]
async fn no_run_approval_requires_its_own_target_resource_and_integer_metadata() {
    let area = Area::new();
    let storage = area.open().await.unwrap();
    storage.close().await;
    area.observe().await;
    let mut connection = connect(&area).await;
    sqlx::query("INSERT INTO product_sources VALUES(?,'test')")
        .bind(id(1))
        .execute(&mut connection)
        .await
        .unwrap();
    for value in [2, 3] {
        sqlx::query("INSERT INTO product_targets VALUES(?,?,?,NULL)")
            .bind(id(value))
            .bind(id(1))
            .bind(format!("native-{value}"))
            .execute(&mut connection)
            .await
            .unwrap();
        sqlx::query("INSERT INTO product_resources VALUES(?,?,?,?,?,'loaded',0)")
            .bind(id(value + 10))
            .bind(id(value))
            .bind(id(20))
            .bind(id(21))
            .bind(id(22))
            .execute(&mut connection)
            .await
            .unwrap();
    }
    let sql = "INSERT INTO product_approvals(approval_id,target_id,load_epoch,native_request_digest,request_digest,run_id,protocol_version,state) VALUES(?,?,?,?,?,NULL,2,'pending')";
    assert!(
        sqlx::query(sql)
            .bind(id(30))
            .bind(id(2))
            .bind(id(13))
            .bind("a".repeat(64))
            .bind("b".repeat(64))
            .execute(&mut connection)
            .await
            .is_err()
    );
    sqlx::query(sql)
        .bind(id(30))
        .bind(id(2))
        .bind(id(12))
        .bind("a".repeat(64))
        .bind("b".repeat(64))
        .execute(&mut connection)
        .await
        .unwrap();
    assert!(
        sqlx::query("UPDATE product_control SET time_floor_ms=1.5")
            .execute(&mut connection)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE product_control SET time_floor_ms=-1")
            .execute(&mut connection)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE product_control SET time_floor_ms=9007199254740992")
            .execute(&mut connection)
            .await
            .is_err()
    );
    connection.close().await.unwrap();
    area.observe().await;
}

#[cfg(windows)]
#[tokio::test]
async fn readonly_preflight_reads_committed_uncheckpointed_wal_without_changing_its_bytes() {
    let source = Area::new();
    legacy(&source, 2).await;
    let mut writer = connect(&source).await;
    sqlx::query("PRAGMA journal_mode=WAL")
        .execute(&mut writer)
        .await
        .unwrap();
    sqlx::query("PRAGMA wal_autocheckpoint=0")
        .execute(&mut writer)
        .await
        .unwrap();
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&mut writer)
        .await
        .unwrap();
    // Only the WAL contains this committed version; main database remains a valid schema2.
    sqlx::query("PRAGMA user_version=77")
        .execute(&mut writer)
        .await
        .unwrap();
    let fixed = Area::new();
    std::fs::copy(source.database(), fixed.database()).unwrap();
    let wal_path = |path: PathBuf| {
        let mut name = path.into_os_string();
        name.push("-wal");
        PathBuf::from(name)
    };
    std::fs::copy(wal_path(source.database()), wal_path(fixed.database())).unwrap();
    writer.close().await.unwrap();
    source.observe().await;
    let db_before = std::fs::read(fixed.database()).unwrap();
    let wal_before = std::fs::read(wal_path(fixed.database())).unwrap();
    assert!(!wal_before.is_empty());
    let preflight_scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
        &fixed.database(),
        fixed.roots.clone(),
    );
    let observed = preflight_scope.preflight().await;
    preflight_scope.stop_and_observe().await;
    assert!(
        matches!(observed, Err(ref error) if matches!(error.as_ref(), crate::error::StorageError::UnknownSchema(77))),
        "fixed WAL observation: {observed:?}"
    );
    assert_eq!(std::fs::read(fixed.database()).unwrap(), db_before);
    assert_eq!(
        std::fs::read(wal_path(fixed.database())).unwrap(),
        wal_before
    );
    for file in std::fs::read_dir(&fixed.directory).unwrap() {
        assert!(
            ["test.sqlite3", "test.sqlite3-wal", "test.sqlite3-shm"]
                .contains(&file.unwrap().file_name().to_str().unwrap())
        );
    }
}
