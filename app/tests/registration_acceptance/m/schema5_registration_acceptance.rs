//! Independent M layer, included only in an isolated same-crate test overlay.
mod fixtures;
use crate::{
    domain::EntityId,
    error::StorageError,
    session_storage::schema::{self, MigrationFault},
    storage::Storage,
};
use fixtures::{Area, connect, exec, fixture_base};
use sqlx::{
    Connection, Row, SqliteConnection,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::{collections::BTreeMap, path::Path};

fn id(n: u8) -> String {
    EntityId::from_bytes([n; 16]).encode()
}
const DELIVERY4: &[&str] = &[
    "CREATE UNIQUE INDEX product_delivery_claim_identity ON product_deliveries(approval_id,claim_id)",
    r"CREATE TABLE product_delivery_attempts (
      approval_id TEXT PRIMARY KEY REFERENCES product_approvals(approval_id),
      claim_id TEXT NOT NULL UNIQUE CHECK(length(claim_id)=22),
      planned_response_kind TEXT NOT NULL CHECK(planned_response_kind IN ('selected','cancelled')),
      planned_option_digest TEXT CHECK(planned_option_digest IS NULL OR
        (length(planned_option_digest)=64 AND planned_option_digest NOT GLOB '*[^0-9a-f]*')),
      actual_response_kind TEXT CHECK(actual_response_kind IN ('selected','cancelled')),
      actual_option_digest TEXT CHECK(actual_option_digest IS NULL OR
        (length(actual_option_digest)=64 AND actual_option_digest NOT GLOB '*[^0-9a-f]*')),
      reason TEXT NOT NULL CHECK(reason IN
        ('user_choice','request_cancelled','source_invalid','management_failure','node_stop')),
      transport_state TEXT NOT NULL CHECK(transport_state IN ('claimed','transport_written','unknown')),
      observed_at INTEGER NOT NULL CHECK(observed_at BETWEEN 0 AND 9007199254740991),
      CHECK((planned_response_kind='selected' AND planned_option_digest IS NOT NULL) OR
            (planned_response_kind='cancelled' AND planned_option_digest IS NULL)),
      CHECK((actual_response_kind IS NULL AND actual_option_digest IS NULL) OR
            (actual_response_kind IS NOT NULL AND
              ((actual_response_kind='selected' AND actual_option_digest IS NOT NULL) OR
               (actual_response_kind='cancelled' AND actual_option_digest IS NULL)))),
      CHECK(transport_state!='claimed' OR actual_response_kind IS NULL),
      CHECK(transport_state!='transport_written' OR actual_response_kind IS NOT NULL),
      FOREIGN KEY(approval_id,claim_id) REFERENCES product_deliveries(approval_id,claim_id)
    ) STRICT",
];
async fn fixture(area: &Area, version: i64) {
    if version == 0 {
        std::fs::write(area.0.join("identity.key"), b"synthetic-empty-key-sentinel").unwrap();
        return;
    }
    fixture_base(area, version.min(3)).await;
    if version == 4 {
        let mut c = connect(&area.db()).await;
        for ddl in DELIVERY4 {
            exec(&mut c, ddl).await;
        }
        exec(&mut c, "UPDATE node_meta SET schema_version=4").await;
        exec(&mut c, "PRAGMA user_version=4").await;
        sqlx::query("INSERT INTO product_sources VALUES(?,'old-schema4-source')")
            .bind(id(61))
            .execute(&mut c)
            .await
            .unwrap();
        sqlx::query("INSERT INTO product_targets VALUES(?,?, 'old-native-schema4',NULL)")
            .bind(id(62))
            .bind(id(61))
            .execute(&mut c)
            .await
            .unwrap();
        sqlx::query("INSERT INTO product_resources VALUES(?,?,?,?,?,'loaded',1)")
            .bind(id(63))
            .bind(id(62))
            .bind(id(64))
            .bind(id(65))
            .bind(id(66))
            .execute(&mut c)
            .await
            .unwrap();
        for (command, request, action, target_kind, target, digest) in [
            (67, 68, "session.input", "session", 62, "a"),
            (71, 72, "approval.decide", "approval", 70, "c"),
        ] {
            sqlx::query("INSERT INTO product_commands(command_id,actor_id,auth_generation,request_id,request_epoch,action,target_kind,target_id,payload_digest,issued_at,expires_at,status) VALUES(?,?,12,?,?,?,?,?,?,100,86400100,'completed')")
                .bind(id(command)).bind(id(1)).bind(id(request)).bind(id(60)).bind(action).bind(target_kind).bind(id(target)).bind(digest.repeat(64)).execute(&mut c).await.unwrap();
        }
        sqlx::query("INSERT INTO product_runs VALUES(?,?,?,'completed')")
            .bind(id(69))
            .bind(id(67))
            .bind(id(62))
            .execute(&mut c)
            .await
            .unwrap();
        sqlx::query("INSERT INTO product_approvals(approval_id,target_id,load_epoch,native_request_digest,request_digest,run_id,protocol_version,state,decision_command_id,deciding_actor_id,choice_digest,decision_kind) VALUES(?,?,?,?,?,?,2,'decided',?,?,?,'allow_once')")
            .bind(id(70)).bind(id(62)).bind(id(63)).bind("a".repeat(64)).bind("b".repeat(64)).bind(id(69)).bind(id(71)).bind(id(1)).bind("d".repeat(64)).execute(&mut c).await.unwrap();
        sqlx::query("INSERT INTO product_deliveries VALUES(?,'claimed',?)")
            .bind(id(70))
            .bind(id(73))
            .execute(&mut c)
            .await
            .unwrap();
        sqlx::query("INSERT INTO product_delivery_attempts VALUES(?,?,'selected',?,NULL,NULL,'user_choice','claimed',123)").bind(id(70)).bind(id(73)).bind("d".repeat(64)).execute(&mut c).await.unwrap();
        exec(&mut c, "UPDATE product_control SET time_floor_ms=335577").await;
        c.close().await.unwrap();
    }
}
#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    defs: BTreeMap<String, String>,
    rows: BTreeMap<String, Vec<String>>,
    key: Vec<u8>,
}
async fn snapshot(area: &Area, names: Option<&[String]>) -> Snapshot {
    let mut c = connect(&area.db()).await;
    let definitions:Vec<(String,String)>=sqlx::query_as("SELECT name,sql FROM sqlite_schema WHERE type IN ('table','index','trigger') AND name NOT LIKE 'sqlite_%' ORDER BY name").fetch_all(&mut c).await.unwrap();
    let defs = definitions
        .into_iter()
        .filter(|(name, _)| names.is_none_or(|list| list.contains(name)))
        .collect();
    let tables:Vec<String>=sqlx::query_scalar("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").fetch_all(&mut c).await.unwrap();
    let mut rows = BTreeMap::new();
    for table in tables
        .into_iter()
        .filter(|name| names.is_none_or(|list| list.contains(name)))
    {
        let columns = sqlx::query(&format!("PRAGMA table_info(\"{table}\")"))
            .fetch_all(&mut c)
            .await
            .unwrap();
        let fields: Vec<String> = columns
            .iter()
            .map(|r| r.get::<String, _>("name"))
            .filter(|col| table != "node_meta" || col != "schema_version")
            .map(|col| format!("quote(\"{col}\")"))
            .collect();
        let mut values: Vec<String> = sqlx::query_scalar(&format!(
            "SELECT {} FROM \"{table}\"",
            fields.join("||'|'||")
        ))
        .fetch_all(&mut c)
        .await
        .unwrap();
        values.sort();
        rows.insert(table, values);
    }
    c.close().await.unwrap();
    Snapshot {
        defs,
        rows,
        key: std::fs::read(area.0.join("identity.key")).unwrap(),
    }
}
fn normalize(sql: &str) -> String {
    sql.chars()
        .filter(|c| !c.is_whitespace() && *c != ';')
        .flat_map(char::to_lowercase)
        .collect()
}
async fn verify_five(area: &Area) {
    let mut c = connect(&area.db()).await;
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&mut c)
        .await
        .unwrap();
    assert_eq!(version, crate::storage::SCHEMA_VERSION);
    let expected = include_str!("expected-schema5.sql")
        .lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n");
    for definition in expected.split(';').filter(|s| !s.trim().is_empty()) {
        let words: Vec<_> = definition.split_whitespace().collect();
        let name = words[2];
        let actual: String = sqlx::query_scalar("SELECT sql FROM sqlite_schema WHERE name=?")
            .bind(name)
            .fetch_one(&mut c)
            .await
            .unwrap();
        assert_eq!(normalize(&actual), normalize(definition), "{name}");
    }
    let tables:i64=sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_list WHERE name IN ('node_request_registrations','node_request_operations') AND strict=1").fetch_one(&mut c).await.unwrap();
    assert_eq!(tables, 2);
    for table in ["node_request_registrations", "node_request_operations"] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&mut c)
            .await
            .unwrap();
        assert_eq!(count, 0, "old facts were copied into {table}");
    }
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut c)
        .await
        .unwrap();
    assert_eq!(integrity, "ok");
    assert!(
        sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&mut c)
            .await
            .unwrap()
            .is_empty()
    );
    c.close().await.unwrap();
}
async fn migrate(area: &Area) {
    let storage = Storage::open(&area.db()).await.unwrap();
    storage.close().await;
}
async fn pool(path: &Path) -> sqlx::SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .foreign_keys(true),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn cl004_reg01_all_pinned_zero_to_four_upgrade_to_five_without_any_old_fact_loss() {
    for version in 0..=4 {
        let area = Area::new();
        fixture(&area, version).await;
        let mut before = snapshot(&area, None).await;
        let names: Vec<_> = before
            .defs
            .keys()
            .filter(|name| !super::is_retired_legacy_object(name))
            .cloned()
            .collect();
        before
            .defs
            .retain(|name, _| !super::is_retired_legacy_object(name));
        before
            .rows
            .retain(|name, _| !super::is_retired_legacy_object(name));
        migrate(&area).await;
        verify_five(&area).await;
        let current_snapshot = snapshot(&area, None).await;
        assert!(
            !current_snapshot
                .defs
                .keys()
                .chain(current_snapshot.rows.keys())
                .any(|name| super::is_retired_legacy_object(name))
        );
        assert_eq!(snapshot(&area, Some(&names)).await, before);
        let upgraded = snapshot(&area, None).await;
        migrate(&area).await;
        assert_eq!(snapshot(&area, None).await, upgraded);
        if version == 4 {
            let mut c = connect(&area.db()).await;
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM product_delivery_attempts")
                .fetch_one(&mut c)
                .await
                .unwrap();
            assert_eq!(count, 1);
            let floor: i64 = sqlx::query_scalar("SELECT time_floor_ms FROM product_control")
                .fetch_one(&mut c)
                .await
                .unwrap();
            assert_eq!(floor, 335_577);
            c.close().await.unwrap();
        }
    }
}

#[tokio::test]
async fn cl004_reg02_each_registration_migration_fault_rolls_back_whole_transaction() {
    for version in 0..=4 {
        for fault in [
            MigrationFault::RegistrationTable,
            MigrationFault::OperationTable,
            MigrationFault::RegistrationIndex,
            MigrationFault::RegistrationMetadata,
            MigrationFault::RegistrationPragma,
            MigrationFault::Validation,
            MigrationFault::Commit,
        ] {
            let area = Area::new();
            fixture(&area, version).await;
            let before = snapshot(&area, None).await;
            let p = pool(&area.db()).await;
            assert!(
                matches!(
                    schema::initialize_for_test(&p, fault).await,
                    Err(StorageError::MigrationInterrupted)
                ),
                "{version}/{fault:?}"
            );
            p.close().await;
            assert_eq!(snapshot(&area, None).await, before);
            let mut c = connect(&area.db()).await;
            let actual: i64 = sqlx::query_scalar("PRAGMA user_version")
                .fetch_one(&mut c)
                .await
                .unwrap();
            assert_eq!(actual, version);
            let partial: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_schema WHERE name LIKE 'node_request_%'",
            )
            .fetch_one(&mut c)
            .await
            .unwrap();
            assert_eq!(partial, 0);
            c.close().await.unwrap();
            migrate(&area).await;
            verify_five(&area).await;
        }
    }
}

#[tokio::test]
async fn cl004_reg02_real_commit_unknown_is_readable_from_second_connection_at_five() {
    for version in 0..=4 {
        let area = Area::new();
        fixture(&area, version).await;
        let mut before = snapshot(&area, None).await;
        let names: Vec<_> = before
            .defs
            .keys()
            .filter(|name| !super::is_retired_legacy_object(name))
            .cloned()
            .collect();
        before
            .defs
            .retain(|name, _| !super::is_retired_legacy_object(name));
        before
            .rows
            .retain(|name, _| !super::is_retired_legacy_object(name));
        let p = pool(&area.db()).await;
        assert!(
            schema::initialize_for_test(&p, MigrationFault::CommitUnknown)
                .await
                .is_err()
        );
        verify_five(&area).await;
        assert_eq!(snapshot(&area, Some(&names)).await, before);
        p.close().await;
        migrate(&area).await;
        verify_five(&area).await;
    }
}

#[tokio::test]
async fn cl004_reg03_half_schema_five_and_true_future_are_readonly_rejected() {
    for version in 0..=4 {
        let area = Area::new();
        fixture(&area, version).await;
        let mut c = connect(&area.db()).await;
        exec(&mut c, "CREATE TABLE node_request_registrations(fake TEXT)").await;
        c.close().await.unwrap();
        let before = snapshot(&area, None).await;
        let bytes = std::fs::read(area.db()).unwrap();
        assert!(matches!(
            Storage::open(&area.db()).await,
            Err(StorageError::UnrecognizedSchema)
        ));
        assert_eq!(snapshot(&area, None).await, before);
        assert_eq!(std::fs::read(area.db()).unwrap(), bytes);
    }
    for version in [5, 77] {
        let area = Area::new();
        fixture(&area, 4).await;
        let mut c = connect(&area.db()).await;
        exec(&mut c, &format!("PRAGMA user_version={version}")).await;
        exec(
            &mut c,
            &format!("UPDATE node_meta SET schema_version={version}"),
        )
        .await;
        c.close().await.unwrap();
        let before = snapshot(&area, None).await;
        let result = Storage::open(&area.db()).await;
        match (version, result) {
            (5, Err(StorageError::UnrecognizedSchema))
            | (77, Err(StorageError::UnknownSchema(77))) => {}
            _ => panic!("wrong malformed/future version classification"),
        }
        assert_eq!(snapshot(&area, None).await, before);
    }
}

async fn ledger_seed(c: &mut SqliteConnection) {
    sqlx::query("INSERT INTO product_sources VALUES(?,'synthetic-ledger-source')")
        .bind(id(80))
        .execute(&mut *c)
        .await
        .unwrap();
    sqlx::query("INSERT INTO product_targets VALUES(?,?, 'synthetic-ledger-native',NULL)")
        .bind(id(81))
        .bind(id(80))
        .execute(c)
        .await
        .unwrap();
}
async fn insert_registration(
    c: &mut SqliteConnection,
    n: u8,
    action: &str,
    revision: Option<Vec<u8>>,
) -> Result<sqlx::sqlite::SqliteQueryResult, sqlx::Error> {
    sqlx::query("INSERT INTO node_request_registrations(request_id,intent_version,actor_id,register_auth_generation,action,target_id,item_id,expected_revision,body_digest,registered_at_ms) VALUES(?,1,?,12,?,?,?, ?,?, -1)")
        .bind(id(n)).bind(id(1)).bind(action).bind(id(81)).bind(if action=="input.edit"{Some(id(82))}else{None}).bind(revision).bind(vec![7_u8;32]).execute(c).await
}

#[tokio::test]
async fn cl004_reg04_full_u64_revision_blob_roundtrip_and_signed_time() {
    let area = Area::new();
    fixture(&area, 4).await;
    migrate(&area).await;
    let mut c = connect(&area.db()).await;
    ledger_seed(&mut c).await;
    for (n, value) in [(90, 0_u64), (91, 1), (92, u64::MAX)] {
        insert_registration(&mut c, n, "input.edit", Some(value.to_be_bytes().to_vec()))
            .await
            .unwrap();
        let bytes: Vec<u8> = sqlx::query_scalar(
            "SELECT expected_revision FROM node_request_registrations WHERE request_id=?",
        )
        .bind(id(n))
        .fetch_one(&mut c)
        .await
        .unwrap();
        assert_eq!(bytes, value.to_be_bytes());
        assert_eq!(u64::from_be_bytes(bytes.try_into().unwrap()), value);
        let at: i64 = sqlx::query_scalar(
            "SELECT registered_at_ms FROM node_request_registrations WHERE request_id=?",
        )
        .bind(id(n))
        .fetch_one(&mut c)
        .await
        .unwrap();
        assert_eq!(at, -1);
    }
    c.close().await.unwrap();
}

#[tokio::test]
async fn cl004_reg04_actual_strict_fk_shape_and_operation_constraints() {
    let area = Area::new();
    fixture(&area, 4).await;
    migrate(&area).await;
    let mut c = connect(&area.db()).await;
    ledger_seed(&mut c).await;
    insert_registration(&mut c, 90, "input.submit", None)
        .await
        .unwrap();
    for sql in [
        "UPDATE node_request_registrations SET retired=2",
        "UPDATE node_request_registrations SET register_auth_generation=-1",
        "UPDATE node_request_registrations SET intent_version=2",
        "UPDATE node_request_registrations SET action='unknown'",
        "UPDATE node_request_registrations SET body_digest=zeroblob(31)",
        "UPDATE node_request_registrations SET body_digest='not-a-blob'",
        "UPDATE node_request_registrations SET item_id='too-short'",
        "UPDATE node_request_registrations SET expected_revision=zeroblob(8)",
        "UPDATE node_request_registrations SET actor_id='missing'",
        "UPDATE node_request_registrations SET target_id='missing'",
    ] {
        assert!(
            sqlx::query(sql).execute(&mut c).await.is_err(),
            "accepted invalid registration SQL {sql}"
        );
    }
    sqlx::query("INSERT INTO node_request_operations(request_id,operation_id,runtime_epoch,admission_auth_generation,phase,begun_at_ms) VALUES(?,?,?,12,'applying',-10)").bind(id(90)).bind(id(93)).bind(id(94)).execute(&mut c).await.unwrap();
    for sql in [
        "UPDATE node_request_operations SET phase='settled'",
        "UPDATE node_request_operations SET phase='interrupted'",
        "UPDATE node_request_operations SET result_code='queued'",
        "UPDATE node_request_operations SET admission_auth_generation=-1",
        "UPDATE node_request_operations SET result_revision=zeroblob(7)",
        "UPDATE node_request_operations SET restored_count=4",
        "UPDATE node_request_operations SET rejection_code='unknown'",
        "UPDATE node_request_operations SET operation_id='short'",
    ] {
        assert!(
            sqlx::query(sql).execute(&mut c).await.is_err(),
            "accepted invalid operation SQL {sql}"
        );
    }
    sqlx::query("UPDATE node_request_operations SET phase='settled',resolved_at_ms=-9,result_code='queued',result_item_id=?,result_revision=?").bind(id(90)).bind(1_u64.to_be_bytes().to_vec()).execute(&mut c).await.unwrap();
    let row = sqlx::query("SELECT phase,resolved_at_ms FROM node_request_operations")
        .fetch_one(&mut c)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("phase"), "settled");
    assert_eq!(row.get::<i64, _>("resolved_at_ms"), -9);
    c.close().await.unwrap();
}

#[tokio::test]
async fn cl004_reg03_five_new_table_definition_and_identity_history_are_validated_readonly() {
    let area = Area::new();
    fixture(&area, 4).await;
    migrate(&area).await;
    let mut c = connect(&area.db()).await;
    exec(
        &mut c,
        "ALTER TABLE node_request_operations ADD COLUMN unreviewed INTEGER",
    )
    .await;
    c.close().await.unwrap();
    let before = snapshot(&area, None).await;
    assert!(matches!(
        Storage::open(&area.db()).await,
        Err(StorageError::UnrecognizedSchema)
    ));
    assert_eq!(snapshot(&area, None).await, before);
    let area = Area::new();
    fixture(&area, 0).await;
    migrate(&area).await;
    let mut c = connect(&area.db()).await;
    exec(&mut c, "PRAGMA foreign_keys=OFF").await;
    sqlx::query("INSERT INTO node_request_registrations(request_id,intent_version,actor_id,register_auth_generation,action,target_id,registered_at_ms) VALUES(?,1,?,0,'input.resume',?,0)").bind(id(90)).bind(id(91)).bind(id(92)).execute(&mut c).await.unwrap();
    c.close().await.unwrap();
    let before = snapshot(&area, None).await;
    assert!(matches!(
        Storage::open(&area.db()).await,
        Err(StorageError::MissingNodeMeta)
    ));
    assert_eq!(snapshot(&area, None).await, before);
}
