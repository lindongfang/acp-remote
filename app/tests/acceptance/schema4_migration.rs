// Independent CL-004 schema4 layout-only acceptance. Legacy fixtures are pinned literals, not a new DB downgraded.
use crate::{
    domain::EntityId,
    error::StorageError,
    session_storage::schema::{self, MigrationFault},
    storage::Storage,
};
use sqlx::{
    Connection, Row, SqliteConnection,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
const ROOT: &str = "C:/Users/jack/.codex/visualizations/2026/10/30/01a18dc1-45b2-7f82-981e-964264221cef/deliver-codex-session-loop-tester/schema4-fixtures";
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Area(PathBuf);
impl Area {
    fn new() -> Self {
        let p = PathBuf::from(ROOT).join(format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn db(&self) -> PathBuf {
        self.0.join("node.sqlite3")
    }
}
impl Drop for Area {
    fn drop(&mut self) {
        assert_eq!(self.0.parent(), Some(Path::new(ROOT)));
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn id(n: u8) -> String {
    EntityId::from_bytes([n; 16]).encode()
}
async fn connect(path: &Path) -> SqliteConnection {
    SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true),
    )
    .await
    .unwrap()
}
async fn exec(c: &mut SqliteConnection, sql: &str) {
    sqlx::query(sql).execute(c).await.unwrap();
}
async fn fixture(a: &Area, v: i64) {
    let mut c = connect(&a.db()).await;
    for ddl in LEGACY_CORE {
        exec(&mut c, ddl).await;
    }
    sqlx::query(
        "INSERT INTO node_meta VALUES(1,?,'pinned-original-node',X'04010200ff','identity.key')",
    )
    .bind(v)
    .execute(&mut c)
    .await
    .unwrap();
    exec(&mut c, "INSERT INTO node_state VALUES(1,987)").await;
    sqlx::query("INSERT INTO clients VALUES(?,X'0100ff','trusted-original','trusted',12,100,NULL),(?,X'0200ff','revoked-original','revoked',19,200,300)").bind(id(1)).bind(id(2)).execute(&mut c).await.unwrap();
    exec(&mut c,"INSERT INTO management_commands VALUES('old-actor','old-request','device.revoke','old-digest','opaque-old-result',4102444800)").await;
    exec(&mut c,"INSERT INTO security_audit VALUES(42,'old-actor','device.revoke','old-target','revoked',300)").await;
    if v >= 2 {
        for ddl in LEGACY_SESSION {
            exec(&mut c, ddl).await;
        }
        exec(
            &mut c,
            "INSERT INTO harnesses VALUES('old-h','original','old.exe','[]',NULL)",
        )
        .await;
        exec(
            &mut c,
            "INSERT INTO workspaces VALUES('old-w','D:/synthetic-only')",
        )
        .await;
        exec(&mut c,"INSERT INTO sessions VALUES('old-s','old-h','old-w','interrupted','old-source','native-original','{}')").await;
        sqlx::query("INSERT INTO content_objects VALUES('old-content','old-s',?,12,'output',X'00ff7b7d','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',4)").bind(id(1)).execute(&mut c).await.unwrap();
        sqlx::query("INSERT INTO session_commands VALUES('old-c',?,'old-r','session.input','old-digest','old-s','delivery_unknown')").bind(id(1)).execute(&mut c).await.unwrap();
        exec(
            &mut c,
            "INSERT INTO turns VALUES('old-t','old-s','old-c',17)",
        )
        .await;
        exec(
            &mut c,
            "INSERT INTO runs VALUES('old-run','old-s','old-c','old-t','unknown')",
        )
        .await;
        exec(&mut c,"INSERT INTO conversation_items(item_id,session_id,turn_id,source_id,first_position,kind,content_ref,content_version,committed_version) VALUES('old-i','old-s','old-t','original-source',11,'unknown-extension','old-content',5,3)").await;
        exec(
            &mut c,
            "INSERT INTO approval_connections VALUES('old-instance',7,'closed')",
        )
        .await;
        exec(&mut c,"INSERT INTO approval_run_sources VALUES('old-run','old-s','old-t','old-instance',7,'native-original','invalid','connection-lost')").await;
        exec(&mut c,"INSERT INTO approvals(approval_id,session_id,run_id,turn_id,instance_id,connection_generation,native_session_id,native_request_id,source_digest,request_kind,protocol,protocol_version,raw_request,scope_raw,status) VALUES('old-a','old-s','old-run','old-t','old-instance',7,'native-original','1','digest','tool_permission','acp','1',X'00ff7b7d',X'010203','invalid')").await;
        exec(&mut c,"INSERT INTO approval_options VALUES('old-choice','old-a',0,'once','synthetic option','allow_once')").await;
        exec(
            &mut c,
            "INSERT INTO approval_deliveries VALUES('old-a','unknown','old-claim','unknown',0)",
        )
        .await;
        exec(&mut c,"INSERT INTO approval_audit_links VALUES(42,'old-a','old-c','old-basis',12,'old-instance',7)").await;
        exec(&mut c, "INSERT INTO session_state VALUES('old-s',23,12,18)").await;
        exec(
            &mut c,
            "INSERT INTO history_integrity VALUES('old-s','old-run','synthetic-original',11,11,4)",
        )
        .await;
        exec(
            &mut c,
            "INSERT INTO history_content_pins VALUES('old-pin','old-content')",
        )
        .await;
    }
    if v == 3 {
        for ddl in PINNED_PRODUCT3 {
            exec(&mut c, ddl).await;
        }
        sqlx::query("INSERT INTO product_control VALUES(1,?,123456)")
            .bind(id(60))
            .execute(&mut c)
            .await
            .unwrap();
        seed_product3(&mut c).await;
    }
    exec(&mut c, &format!("PRAGMA user_version={v}")).await;
    c.close().await.unwrap();
    std::fs::write(
        a.0.join("identity.key"),
        b"synthetic-protected-key-byte-sentinel\0\xff",
    )
    .unwrap();
}
#[derive(Debug, PartialEq, Eq)]
struct LegacySnapshot {
    definitions: BTreeMap<String, String>,
    rows: BTreeMap<String, Vec<String>>,
    key: Vec<u8>,
}
async fn snapshot(a: &Area, names: Option<&[String]>) -> LegacySnapshot {
    let mut c = connect(&a.db()).await;
    let defs:Vec<(String,String)>=sqlx::query_as("SELECT name,sql FROM sqlite_schema WHERE type IN ('table','index','trigger') AND name NOT LIKE 'sqlite_%' ORDER BY name").fetch_all(&mut c).await.unwrap();
    let definitions = defs
        .into_iter()
        .filter(|(name, _)| names.is_none_or(|ns| ns.contains(name)))
        .collect();
    let tables:Vec<String>=sqlx::query_scalar("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").fetch_all(&mut c).await.unwrap();
    let mut rows = BTreeMap::new();
    for t in tables
        .into_iter()
        .filter(|n| names.is_none_or(|ns| ns.contains(n)))
    {
        let cols = sqlx::query(&format!("PRAGMA table_info(\"{t}\")"))
            .fetch_all(&mut c)
            .await
            .unwrap();
        let parts: Vec<String> = cols
            .iter()
            .map(|r| r.get::<String, _>("name"))
            .filter(|col| t != "node_meta" || col != "schema_version")
            .map(|col| format!("quote(\"{col}\")"))
            .collect();
        let sql = format!("SELECT {} FROM \"{t}\"", parts.join("||'|'||"));
        let mut vals: Vec<String> = sqlx::query_scalar(&sql).fetch_all(&mut c).await.unwrap();
        vals.sort();
        rows.insert(t, vals);
    }
    c.close().await.unwrap();
    LegacySnapshot {
        definitions,
        rows,
        key: std::fs::read(a.0.join("identity.key")).unwrap(),
    }
}
async fn versions(c: &mut SqliteConnection) -> (i64, i64) {
    let p = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&mut *c)
        .await
        .unwrap();
    let m = sqlx::query_scalar("SELECT schema_version FROM node_meta")
        .fetch_one(c)
        .await
        .unwrap();
    (p, m)
}
async fn check_integrity(c: &mut SqliteConnection) {
    let s: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut *c)
        .await
        .unwrap();
    assert_eq!(s, "ok");
    assert!(
        sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(c)
            .await
            .unwrap()
            .is_empty()
    );
}
async fn unchanged(a: &Area, before: &LegacySnapshot, v: i64) {
    let names: Vec<String> = before.definitions.keys().cloned().collect();
    assert_eq!(&snapshot(a, Some(&names)).await, before);
    let mut c = connect(&a.db()).await;
    assert_eq!(versions(&mut c).await, (v, v));
    check_integrity(&mut c).await;
    c.close().await.unwrap();
}

async fn seed_product3(c: &mut SqliteConnection) {
    sqlx::query("INSERT INTO product_sources VALUES(?,'pinned-source')")
        .bind(id(10))
        .execute(&mut *c)
        .await
        .unwrap();
    for n in [11, 12] {
        sqlx::query("INSERT INTO product_targets VALUES(?,?,?,NULL)")
            .bind(id(n))
            .bind(id(10))
            .bind(format!("native-{n}"))
            .execute(&mut *c)
            .await
            .unwrap();
        sqlx::query("INSERT INTO product_resources VALUES(?,?,?,?,?,'loaded',7)")
            .bind(id(n + 10))
            .bind(id(n))
            .bind(id(50))
            .bind(id(51))
            .bind(id(52))
            .execute(&mut *c)
            .await
            .unwrap();
        let command = n + 20;
        sqlx::query("INSERT INTO product_commands(command_id,actor_id,auth_generation,request_id,request_epoch,action,target_kind,target_id,payload_digest,issued_at,expires_at,status) VALUES(?,?,12,?,?,'session.input','session',?,?,100,86400100,'completed')").bind(id(command)).bind(id(1)).bind(id(command+10)).bind(id(60)).bind(id(n)).bind("a".repeat(64)).execute(&mut *c).await.unwrap();
        sqlx::query("INSERT INTO product_runs VALUES(?,?,?,'completed')")
            .bind(id(command + 10))
            .bind(id(command))
            .bind(id(n))
            .execute(&mut *c)
            .await
            .unwrap();
        for stage in ["admission", "dispatch", "result", "reconciliation"] {
            sqlx::query("INSERT INTO product_audit_slots(command_id,stage,actor_id,reserved_at) VALUES(?,?,?,100)").bind(id(command)).bind(stage).bind(id(1)).execute(&mut *c).await.unwrap();
        }
    }
    for n in 70..76 {
        let command = n + 20;
        let decided = n < 74;
        sqlx::query("INSERT INTO product_approvals(approval_id,target_id,load_epoch,native_request_digest,request_digest,run_id,protocol_version,state) VALUES(?,?,?,?,?,?,2,'pending')").bind(id(n)).bind(id(11)).bind(id(21)).bind(format!("{n:064x}")).bind("b".repeat(64)).bind(if n%2==0 {None} else {Some(id(41))}).execute(&mut *c).await.unwrap();
        if decided {
            sqlx::query("INSERT INTO product_commands(command_id,actor_id,auth_generation,request_id,request_epoch,action,target_kind,target_id,payload_digest,issued_at,expires_at,status) VALUES(?,?,12,?,?,'approval.decide','approval',?,?,100,86400100,'completed')").bind(id(command)).bind(id(1)).bind(id(command+20)).bind(id(60)).bind(id(n)).bind("c".repeat(64)).execute(&mut *c).await.unwrap();
            sqlx::query("UPDATE product_approvals SET state='decided',decision_command_id=?,deciding_actor_id=?,choice_digest=?,decision_kind='allow_once' WHERE approval_id=?").bind(id(command)).bind(id(1)).bind("d".repeat(64)).bind(id(n)).execute(&mut *c).await.unwrap();
            for stage in ["admission", "dispatch", "result", "reconciliation"] {
                sqlx::query("INSERT INTO product_audit_slots(command_id,stage,actor_id,reserved_at) VALUES(?,?,?,100)").bind(id(command)).bind(stage).bind(id(1)).execute(&mut *c).await.unwrap();
            }
        }
        let state = match n {
            70 => "claimed",
            71 => "unknown",
            72 => "transport_written",
            73 | 74 => "not_sent",
            _ => "suppressed",
        };
        sqlx::query("INSERT INTO product_deliveries VALUES(?,?,?)")
            .bind(id(n))
            .bind(state)
            .bind(if n < 73 { Some(id(n + 60)) } else { None })
            .execute(&mut *c)
            .await
            .unwrap();
    }
}

async fn current(a: &Area) {
    let s = Storage::open(&a.db()).await.unwrap();
    s.close().await;
}
async fn pool(a: &Area) -> sqlx::SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(a.db())
                .foreign_keys(true),
        )
        .await
        .unwrap()
}
async fn verify_four(a: &Area) {
    let mut c = connect(&a.db()).await;
    assert_eq!(versions(&mut c).await, (4, 4));
    let present:i64=sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name IN ('product_delivery_attempts','product_delivery_claim_identity')").fetch_one(&mut c).await.unwrap();
    assert_eq!(present, 2);
    check_integrity(&mut c).await;
    let tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_list WHERE name GLOB 'product_*' AND strict=1",
    )
    .fetch_one(&mut c)
    .await
    .unwrap();
    assert_eq!(tables, 11);
    c.close().await.unwrap();
}

#[tokio::test]
async fn cl004_schema4_nonempty_pinned_one_two_three_upgrade_preserves_every_old_fact() {
    for v in [1, 2, 3] {
        let a = Area::new();
        fixture(&a, v).await;
        let before = snapshot(&a, None).await;
        let names: Vec<String> = before.definitions.keys().cloned().collect();
        current(&a).await;
        verify_four(&a).await;
        assert_eq!(snapshot(&a, Some(&names)).await, before);
        let mut c = connect(&a.db()).await;
        let attempts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM product_delivery_attempts")
            .fetch_one(&mut c)
            .await
            .unwrap();
        assert_eq!(attempts, 0);
        if v == 3 {
            let control: (String, i64) =
                sqlx::query_as("SELECT request_epoch,time_floor_ms FROM product_control")
                    .fetch_one(&mut c)
                    .await
                    .unwrap();
            assert_eq!(control, (id(60), 123_456));
            let approvals: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM product_approvals")
                .fetch_one(&mut c)
                .await
                .unwrap();
            assert_eq!(approvals, 6);
            let slots: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM product_audit_slots")
                .fetch_one(&mut c)
                .await
                .unwrap();
            assert_eq!(slots, 24);
        }
        c.close().await.unwrap();
        let upgraded = snapshot(&a, None).await;
        for _ in 0..2 {
            current(&a).await;
        }
        assert_eq!(snapshot(&a, None).await, upgraded);
    }
}

#[tokio::test]
async fn cl004_schema4_empty_bootstrap_reopen_keeps_one_epoch() {
    let a = Area::new();
    std::fs::write(a.0.join("identity.key"), b"bootstrap-synthetic-only").unwrap();
    current(&a).await;
    let mut c = connect(&a.db()).await;
    let control: (String, i64) =
        sqlx::query_as("SELECT request_epoch,time_floor_ms FROM product_control")
            .fetch_one(&mut c)
            .await
            .unwrap();
    assert!(EntityId::decode(&control.0).is_ok());
    assert_eq!(control.1, 0);
    let meta: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_meta")
        .fetch_one(&mut c)
        .await
        .unwrap();
    assert_eq!(meta, 0);
    c.close().await.unwrap();
    current(&a).await;
    let mut c = connect(&a.db()).await;
    let after: (String, i64) =
        sqlx::query_as("SELECT request_epoch,time_floor_ms FROM product_control")
            .fetch_one(&mut c)
            .await
            .unwrap();
    assert_eq!(after, control);
    let pragma: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&mut c)
        .await
        .unwrap();
    assert_eq!(pragma, 4);
    c.close().await.unwrap();
}

#[tokio::test]
async fn cl004_schema4_each_fixed_precommit_fault_rolls_back_all_changes() {
    for v in [1, 2, 3] {
        for fault in [
            MigrationFault::Ddl,
            MigrationFault::Product3Ready,
            MigrationFault::DeliveryIndex,
            MigrationFault::DeliveryTable,
            MigrationFault::Metadata,
            MigrationFault::Pragma,
            MigrationFault::Validation,
            MigrationFault::Commit,
        ] {
            let a = Area::new();
            fixture(&a, v).await;
            let before = snapshot(&a, None).await;
            let p = pool(&a).await;
            assert!(
                matches!(
                    schema::initialize_for_test(&p, fault).await,
                    Err(StorageError::MigrationInterrupted)
                ),
                "v{v} {fault:?}"
            );
            p.close().await;
            unchanged(&a, &before, v).await;
            assert_eq!(
                snapshot(&a, None).await,
                before,
                "unexpected new object after {fault:?}"
            );
            current(&a).await;
            verify_four(&a).await;
        }
    }
}

#[tokio::test]
async fn cl004_schema4_real_deferred_engine_commit_failure_rolls_back_not_hook_success() {
    for v in [1, 2, 3] {
        let a = Area::new();
        fixture(&a, v).await;
        let before = snapshot(&a, None).await;
        let p = pool(&a).await;
        match schema::initialize_for_test(&p, MigrationFault::DeferredCommit).await {
            Err(StorageError::Database(sqlx::Error::Database(error))) => {
                assert_eq!(error.code().as_deref(), Some("787"));
                assert!(error.message().contains("FOREIGN KEY"));
            }
            other => panic!("expected SQLite deferred COMMIT FK failure: {other:?}"),
        }
        p.close().await;
        unchanged(&a, &before, v).await;
        assert_eq!(snapshot(&a, None).await, before);
        current(&a).await;
        verify_four(&a).await;
    }
}

#[tokio::test]
async fn cl004_schema4_commit_success_lost_response_is_durable_not_rolled_back() {
    for v in [1, 2, 3] {
        let a = Area::new();
        fixture(&a, v).await;
        let before = snapshot(&a, None).await;
        let names: Vec<String> = before.definitions.keys().cloned().collect();
        let p = pool(&a).await;
        assert!(
            schema::initialize_for_test(&p, MigrationFault::CommitUnknown)
                .await
                .is_err()
        );
        // verify_four and snapshot open independent connections before the fault pool is closed.
        verify_four(&a).await;
        assert_eq!(snapshot(&a, Some(&names)).await, before);
        p.close().await;
        current(&a).await;
        verify_four(&a).await;
    }
}

async fn reject_unchanged(a: &Area) {
    let mut setup = connect(&a.db()).await;
    exec(&mut setup, "PRAGMA journal_mode=DELETE").await;
    setup.close().await.unwrap();
    let before = std::fs::read(a.db()).unwrap();
    let key = std::fs::read(a.0.join("identity.key")).unwrap();
    assert!(schema::preflight(&a.db()).await.is_err());
    assert!(Storage::open(&a.db()).await.is_err());
    assert_eq!(std::fs::read(a.db()).unwrap(), before);
    assert_eq!(std::fs::read(a.0.join("identity.key")).unwrap(), key);
}

#[tokio::test]
async fn cl004_schema4_rejects_partial_objects_versions_and_missing_identity_readonly() {
    for fault in [
        "index-in-three",
        "table-in-three",
        "version-meta",
        "future",
        "missing-index",
        "missing-table",
        "missing-legacy",
        "missing-control",
        "missing-meta-product",
        "missing-meta-floor",
        "missing-meta-attempt",
    ] {
        let a = Area::new();
        fixture(&a, 3).await;
        if matches!(
            fault,
            "missing-index"
                | "missing-table"
                | "missing-legacy"
                | "missing-control"
                | "missing-meta-product"
                | "missing-meta-floor"
                | "missing-meta-attempt"
        ) {
            current(&a).await;
        }
        let mut c = connect(&a.db()).await;
        if fault == "missing-meta-attempt" {
            attempt(
                &mut c,
                70,
                &id(130),
                "cancelled",
                None,
                None,
                None,
                "unknown",
                "management_failure",
                1,
            )
            .await
            .unwrap();
        }
        match fault {
            "index-in-three"=>exec(&mut c,"CREATE UNIQUE INDEX product_delivery_claim_identity ON product_deliveries(approval_id,claim_id)").await,
            "table-in-three"=>exec(&mut c,"CREATE TABLE product_delivery_attempts(fake TEXT)").await,
            "version-meta"=>exec(&mut c,"UPDATE node_meta SET schema_version=4").await,
            "future"=>{exec(&mut c,"UPDATE node_meta SET schema_version=77").await;exec(&mut c,"PRAGMA user_version=77").await;},
            "missing-index"=>exec(&mut c,"DROP INDEX product_delivery_claim_identity").await,
            "missing-table"=>exec(&mut c,"DROP TABLE product_delivery_attempts").await,
            "missing-legacy"=>exec(&mut c,"DROP INDEX tool_reference_key").await,
            "missing-control"=>exec(&mut c,"DELETE FROM product_control").await,
            _=>{
                exec(&mut c,"PRAGMA foreign_keys=OFF").await;
                let tables:Vec<String>=sqlx::query_scalar("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'").fetch_all(&mut c).await.unwrap();
                for table in tables { if table!="node_state" && table!="product_control" && (fault!="missing-meta-product" || table!="product_sources") && (fault!="missing-meta-attempt" || table!="product_delivery_attempts") {exec(&mut c,&format!("DELETE FROM {table}")).await;} }
                exec(&mut c,if fault=="missing-meta-floor" {"UPDATE product_control SET time_floor_ms=1"} else {"UPDATE product_control SET time_floor_ms=0"}).await;
            }
        }
        c.close().await.unwrap();
        reject_unchanged(&a).await;
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Explicit SQL columns keep the independent NULL truth-table dimensions visible"
)]
async fn attempt(
    c: &mut SqliteConnection,
    approval: u8,
    claim: &str,
    planned: &str,
    planned_digest: Option<&str>,
    actual: Option<&str>,
    actual_digest: Option<&str>,
    transport: &str,
    reason: &str,
    time: i64,
) -> Result<sqlx::sqlite::SqliteQueryResult, sqlx::Error> {
    sqlx::query("INSERT INTO product_delivery_attempts VALUES(?,?,?,?,?,?,?,?,?)")
        .bind(id(approval))
        .bind(claim)
        .bind(planned)
        .bind(planned_digest)
        .bind(actual)
        .bind(actual_digest)
        .bind(reason)
        .bind(transport)
        .bind(time)
        .execute(c)
        .await
}

#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "One fixture owns the complete SQL NULL and format matrix, including cleanup after every cell"
)]
async fn cl004_schema4_null_truth_table_and_digest_type_constraints() {
    let a = Area::new();
    fixture(&a, 3).await;
    current(&a).await;
    let mut c = connect(&a.db()).await;
    let digest = "a".repeat(64);
    for planned in ["selected", "cancelled"] {
        for planned_digest in [None, Some(digest.as_str())] {
            for actual in [None, Some("selected"), Some("cancelled")] {
                for actual_digest in [None, Some(digest.as_str())] {
                    for state in ["claimed", "transport_written", "unknown"] {
                        let expected = (planned == "selected") == planned_digest.is_some()
                            && match actual {
                                Some("selected") => actual_digest.is_some(),
                                _ => actual_digest.is_none(),
                            }
                            && (state != "claimed" || actual.is_none())
                            && (state != "transport_written" || actual.is_some());
                        let result = attempt(
                            &mut c,
                            70,
                            &id(130),
                            planned,
                            planned_digest,
                            actual,
                            actual_digest,
                            state,
                            "user_choice",
                            0,
                        )
                        .await;
                        assert_eq!(
                            result.is_ok(),
                            expected,
                            "{planned}/{planned_digest:?} {actual:?}/{actual_digest:?} {state}"
                        );
                        exec(&mut c, "DELETE FROM product_delivery_attempts").await;
                    }
                }
            }
        }
    }
    for bad in [
        "A".repeat(64),
        "g".repeat(64),
        "a".repeat(63),
        "a".repeat(65),
    ] {
        assert!(
            attempt(
                &mut c,
                70,
                &id(130),
                "selected",
                Some(&bad),
                None,
                None,
                "claimed",
                "user_choice",
                0
            )
            .await
            .is_err()
        );
        assert!(
            attempt(
                &mut c,
                70,
                &id(130),
                "cancelled",
                None,
                Some("selected"),
                Some(&bad),
                "unknown",
                "user_choice",
                0
            )
            .await
            .is_err()
        );
    }
    for (planned, actual, state, reason, time) in [
        ("bad", None, "unknown", "user_choice", 0),
        ("cancelled", Some("bad"), "unknown", "user_choice", 0),
        ("cancelled", None, "bad", "user_choice", 0),
        ("cancelled", None, "unknown", "bad", 0),
        ("cancelled", None, "unknown", "user_choice", -1),
        (
            "cancelled",
            None,
            "unknown",
            "user_choice",
            9_007_199_254_740_992,
        ),
    ] {
        assert!(
            attempt(
                &mut c,
                70,
                &id(130),
                planned,
                None,
                actual,
                None,
                state,
                reason,
                time
            )
            .await
            .is_err()
        );
    }
    for time in [0, 9_007_199_254_740_991] {
        attempt(
            &mut c,
            70,
            &id(130),
            "cancelled",
            None,
            None,
            None,
            "unknown",
            "user_choice",
            time,
        )
        .await
        .unwrap();
        for sql in [
            "UPDATE product_delivery_attempts SET observed_at=0.5",
            "UPDATE product_delivery_attempts SET observed_at='not-a-number'",
            "UPDATE product_delivery_attempts SET observed_at=X'00'",
            "UPDATE product_delivery_attempts SET planned_option_digest=X'00'",
        ] {
            assert!(sqlx::query(sql).execute(&mut c).await.is_err(), "{sql}");
        }
        exec(&mut c, "DELETE FROM product_delivery_attempts").await;
    }
    c.close().await.unwrap();
}

#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "The FK and canonical-boundary sequence intentionally observes the same two delivery rows"
)]
async fn cl004_schema4_composite_fk_uniqueness_and_sql_length_only_boundary() {
    let a = Area::new();
    fixture(&a, 3).await;
    current(&a).await;
    let mut c = connect(&a.db()).await;
    assert!(
        attempt(
            &mut c,
            70,
            &id(131),
            "cancelled",
            None,
            None,
            None,
            "unknown",
            "management_failure",
            1
        )
        .await
        .is_err()
    );
    assert!(
        attempt(
            &mut c,
            74,
            &id(134),
            "cancelled",
            None,
            None,
            None,
            "unknown",
            "management_failure",
            1
        )
        .await
        .is_err()
    );
    assert!(
        attempt(
            &mut c,
            99,
            &id(130),
            "cancelled",
            None,
            None,
            None,
            "unknown",
            "management_failure",
            1
        )
        .await
        .is_err()
    );
    attempt(
        &mut c,
        70,
        &id(130),
        "cancelled",
        None,
        None,
        None,
        "unknown",
        "management_failure",
        1,
    )
    .await
    .unwrap();
    assert!(
        attempt(
            &mut c,
            70,
            &id(130),
            "cancelled",
            None,
            None,
            None,
            "unknown",
            "management_failure",
            1
        )
        .await
        .is_err()
    );
    assert!(
        attempt(
            &mut c,
            71,
            &id(130),
            "cancelled",
            None,
            None,
            None,
            "unknown",
            "management_failure",
            1
        )
        .await
        .is_err()
    );
    // Length22 is deliberately not a canonical128 SQL decoder. Domain/API validation is later.
    let noncanonical = "!".repeat(22);
    sqlx::query("UPDATE product_deliveries SET claim_id=? WHERE approval_id=?")
        .bind(&noncanonical)
        .bind(id(71))
        .execute(&mut c)
        .await
        .unwrap();
    assert!(EntityId::decode(&noncanonical).is_err());
    attempt(
        &mut c,
        71,
        &noncanonical,
        "cancelled",
        None,
        None,
        None,
        "unknown",
        "management_failure",
        1,
    )
    .await
    .unwrap();
    for invalid in ["x".repeat(21), "x".repeat(23)] {
        assert!(
            sqlx::query("UPDATE product_delivery_attempts SET claim_id=? WHERE approval_id=?")
                .bind(invalid)
                .bind(id(71))
                .execute(&mut c)
                .await
                .is_err()
        );
    }
    check_integrity(&mut c).await;
    c.close().await.unwrap();
}

#[tokio::test]
async fn cl004_schema4_raw_sql_transaction_rollback_does_not_claim_application_atomicity() {
    let a = Area::new();
    fixture(&a, 3).await;
    current(&a).await;
    let before = snapshot(&a, None).await;
    let mut c = connect(&a.db()).await;
    exec(&mut c, "BEGIN IMMEDIATE").await;
    attempt(
        &mut c,
        70,
        &id(130),
        "selected",
        Some(&"a".repeat(64)),
        None,
        None,
        "claimed",
        "user_choice",
        1,
    )
    .await
    .unwrap();
    sqlx::query("UPDATE product_deliveries SET state='unknown' WHERE approval_id=?")
        .bind(id(70))
        .execute(&mut c)
        .await
        .unwrap();
    assert!(sqlx::query("UPDATE product_delivery_attempts SET transport_state='transport_written' WHERE approval_id=?").bind(id(70)).execute(&mut c).await.is_err());
    exec(&mut c, "ROLLBACK").await;
    c.close().await.unwrap();
    assert_eq!(snapshot(&a, None).await, before);
}

#[tokio::test]
#[cfg(windows)]
async fn cl004_schema4_wal_rejection_allows_only_empty_new_wal_and_private_owner_sidecars() {
    let area = Area::new();
    let owner = std::sync::Arc::new(crate::data_dir::DataDirectory::acquire(&area.0).unwrap());
    assert!(matches!(
        crate::data_dir::DataDirectory::acquire(&area.0),
        Err(crate::error::AppError::DataDirectoryBusy)
    ));
    fixture(&area, 2).await;
    let storage = Storage::open_owned(std::sync::Arc::clone(&owner))
        .await
        .unwrap();
    storage.close().await;
    let mut setup = connect(&area.db()).await;
    exec(&mut setup, "DELETE FROM node_meta").await;
    setup.close().await.unwrap();
    let inventory = || {
        let mut names: Vec<_> = std::fs::read_dir(&area.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        names.sort();
        names
    };
    let before = inventory();
    let db_before = std::fs::read(area.db()).unwrap();
    let key_before = std::fs::read(area.0.join("identity.key")).unwrap();
    assert!(!area.0.join("node.sqlite3-wal").exists());
    assert!(matches!(
        Storage::open_owned(std::sync::Arc::clone(&owner)).await,
        Err(StorageError::MissingNodeMeta)
    ));
    for entry in inventory()
        .into_iter()
        .filter(|name| !before.contains(name))
    {
        assert!(entry == "node.sqlite3-shm" || entry == "node.sqlite3-wal");
    }
    let wal = area.0.join("node.sqlite3-wal");
    if wal.exists() {
        assert_eq!(std::fs::metadata(wal).unwrap().len(), 0);
        println!("CL004 observed newly created WAL: exactly 0 bytes");
    }
    assert_eq!(std::fs::read(area.db()).unwrap(), db_before);
    assert_eq!(
        std::fs::read(area.0.join("identity.key")).unwrap(),
        key_before
    );
    assert_private_owner_acl(&area.0);
    assert!(matches!(
        crate::data_dir::DataDirectory::acquire(&area.0),
        Err(crate::error::AppError::DataDirectoryBusy)
    ));
    drop(owner);
    let next_owner = crate::data_dir::DataDirectory::acquire(&area.0).unwrap();
    drop(next_owner);
}

#[cfg(windows)]
fn assert_private_owner_acl(directory: &Path) {
    const SCRIPT: &str = r"
$ErrorActionPreference='Stop'
$directory=$env:ACP_CL004_PRIVATE_DIRECTORY
$expected=[Security.Principal.WindowsIdentity]::GetCurrent().User
$paths=@(Get-Item -LiteralPath $directory)+@(Get-ChildItem -LiteralPath $directory -Force)
foreach($path in $paths){
    $acl=Get-Acl -LiteralPath $path.FullName
    if($acl.GetOwner([Security.Principal.SecurityIdentifier]) -ne $expected){throw 'fixture owner mismatch'}
    $allow=@($acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])|Where-Object {$_.AccessControlType -eq 'Allow'})
    if($allow.Count -eq 0){throw 'fixture has no owner access'}
    foreach($rule in $allow){if($rule.IdentityReference -ne $expected){throw 'fixture has non-owner allow ACE'}}
    if($path.PSIsContainer -and !$acl.AreAccessRulesProtected){throw 'private directory DACL is not protected'}
    if($path.Name -eq 'node.sqlite3-shm' -and !(@($allow|Where-Object {$_.IsInherited}).Count)){throw 'new SHM did not inherit private DACL'}
}
Write-Output 'CL004 owner/private ACL and sidefile inheritance PASS'
";
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .env_remove("PSModulePath")
        .env("ACP_CL004_PRIVATE_DIRECTORY", directory)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ACL probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}

#[tokio::test]
#[cfg(windows)]
async fn cl004_schema4_uncheckpointed_wal_version_and_data_are_read_without_changing_committed_bytes()
 {
    let source = Area::new();
    let source_owner =
        std::sync::Arc::new(crate::data_dir::DataDirectory::acquire(&source.0).unwrap());
    fixture(&source, 2).await;
    let storage = Storage::open_owned(std::sync::Arc::clone(&source_owner))
        .await
        .unwrap();
    storage.close().await;
    let mut writer = connect(&source.db()).await;
    exec(&mut writer, "PRAGMA wal_autocheckpoint=0").await;
    exec(&mut writer, "PRAGMA wal_checkpoint(TRUNCATE)").await;
    exec(&mut writer, "PRAGMA user_version=77").await;
    exec(
        &mut writer,
        "UPDATE security_audit SET outcome='committed-wal-marker'",
    )
    .await;
    // The writer remains open but performs no further writes while copying.
    // The destination is an exclusively owned fixed snapshot with no writer.
    let fixed = Area::new();
    let fixed_owner =
        std::sync::Arc::new(crate::data_dir::DataDirectory::acquire(&fixed.0).unwrap());
    std::fs::copy(source.db(), fixed.db()).unwrap();
    std::fs::copy(
        source.0.join("node.sqlite3-wal"),
        fixed.0.join("node.sqlite3-wal"),
    )
    .unwrap();
    std::fs::copy(source.0.join("identity.key"), fixed.0.join("identity.key")).unwrap();
    let db_before = std::fs::read(fixed.db()).unwrap();
    let wal_before = std::fs::read(fixed.0.join("node.sqlite3-wal")).unwrap();
    assert!(!wal_before.is_empty());
    // Assert the preflight itself reads 77: an RW retry cannot hide immutable's miss.
    assert!(matches!(
        schema::preflight(&fixed.db()).await,
        Err(StorageError::UnknownSchema(77))
    ));
    assert!(matches!(
        Storage::open_owned(std::sync::Arc::clone(&fixed_owner)).await,
        Err(StorageError::UnknownSchema(77))
    ));
    let mut read_view = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(fixed.db())
            .read_only(true)
            .create_if_missing(false),
    )
    .await
    .unwrap();
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&mut read_view)
        .await
        .unwrap();
    let marker: String = sqlx::query_scalar("SELECT outcome FROM security_audit WHERE audit_id=42")
        .fetch_one(&mut read_view)
        .await
        .unwrap();
    assert_eq!(version, 77);
    assert_eq!(marker, "committed-wal-marker");
    read_view.close().await.unwrap();
    assert_eq!(std::fs::read(fixed.db()).unwrap(), db_before);
    assert_eq!(
        std::fs::read(fixed.0.join("node.sqlite3-wal")).unwrap(),
        wal_before
    );
    assert_private_owner_acl(&fixed.0);
    assert!(matches!(
        crate::data_dir::DataDirectory::acquire(&fixed.0),
        Err(crate::error::AppError::DataDirectoryBusy)
    ));
    writer.close().await.unwrap();
}

const PINNED_PRODUCT3: &[&str] = &[
    r"CREATE TABLE product_control (
        singleton INTEGER PRIMARY KEY CHECK(singleton=1),
        request_epoch TEXT NOT NULL CHECK(length(request_epoch)=22),
        time_floor_ms INTEGER NOT NULL CHECK(time_floor_ms BETWEEN 0 AND 9007199254740991)) STRICT",
    r"CREATE TABLE product_sources (
        source_id TEXT PRIMARY KEY CHECK(length(source_id)=22),
        name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 80)) STRICT",
    r"CREATE TABLE product_harness_sources (
        harness_id TEXT PRIMARY KEY REFERENCES harnesses(harness_id),
        source_id TEXT NOT NULL REFERENCES product_sources(source_id)) STRICT",
    r"CREATE TABLE product_targets (
        target_id TEXT PRIMARY KEY CHECK(length(target_id)=22),
        source_id TEXT NOT NULL REFERENCES product_sources(source_id),
        native_id TEXT NOT NULL CHECK(length(CAST(native_id AS BLOB)) BETWEEN 1 AND 4096),
        workspace_id TEXT REFERENCES workspaces(workspace_id),
        UNIQUE(source_id,native_id)) STRICT",
    r"CREATE TABLE product_commands (
        command_id TEXT PRIMARY KEY CHECK(length(command_id)=22),
        actor_id TEXT NOT NULL REFERENCES clients(client_id),
        auth_generation INTEGER NOT NULL CHECK(auth_generation BETWEEN 0 AND 9007199254740991),
        request_id TEXT NOT NULL CHECK(length(request_id)=22),
        request_epoch TEXT NOT NULL CHECK(length(request_epoch)=22),
        action TEXT NOT NULL CHECK(action IN ('harness.register','workspace.register','session.create','session.load','session.input','session.cancel','session.release','session.resume','approval.decide')),
        target_kind TEXT NOT NULL CHECK(target_kind IN ('harness','workspace','source','session','approval')),
        target_id TEXT NOT NULL CHECK(length(target_id)=22),
        payload_digest TEXT NOT NULL CHECK(length(payload_digest)=64 AND payload_digest NOT GLOB '*[^0-9a-f]*'),
        issued_at INTEGER NOT NULL CHECK(issued_at BETWEEN 0 AND 9007199254740991),
        expires_at INTEGER NOT NULL CHECK(expires_at BETWEEN 0 AND 9007199254740991 AND expires_at>issued_at AND expires_at-issued_at<=86400000),
        status TEXT NOT NULL CHECK(status IN ('accepted','dispatching','completed','failed','unknown','expired')),
        claim_id TEXT UNIQUE CHECK(claim_id IS NULL OR length(claim_id)=22),
        CHECK(status!='dispatching' OR claim_id IS NOT NULL),
        UNIQUE(actor_id,request_id)) STRICT",
    r"CREATE TABLE product_runs (
        run_id TEXT PRIMARY KEY CHECK(length(run_id)=22),
        command_id TEXT NOT NULL UNIQUE REFERENCES product_commands(command_id),
        target_id TEXT NOT NULL REFERENCES product_targets(target_id),
        status TEXT NOT NULL CHECK(status IN ('accepted','dispatching','running','cancelling','completed','cancelled','failed','interrupted','unknown')),
        UNIQUE(target_id,run_id)) STRICT",
    r"CREATE TABLE product_resources (
        load_epoch TEXT PRIMARY KEY CHECK(length(load_epoch)=22),
        target_id TEXT NOT NULL REFERENCES product_targets(target_id),
        runtime_epoch TEXT NOT NULL CHECK(length(runtime_epoch)=22),
        connection_epoch TEXT NOT NULL CHECK(length(connection_epoch)=22),
        instance_id TEXT NOT NULL CHECK(length(instance_id)=22),
        state TEXT NOT NULL CHECK(state IN ('reserved','loaded','releasing','released','unknown')),
        revision INTEGER NOT NULL CHECK(revision BETWEEN 0 AND 9007199254740991),
        UNIQUE(target_id,load_epoch)) STRICT",
    r"CREATE TABLE product_approvals (
        approval_id TEXT PRIMARY KEY CHECK(length(approval_id)=22),
        target_id TEXT NOT NULL REFERENCES product_targets(target_id),
        load_epoch TEXT NOT NULL,
        native_request_digest TEXT NOT NULL CHECK(length(native_request_digest)=64 AND native_request_digest NOT GLOB '*[^0-9a-f]*'),
        request_digest TEXT NOT NULL CHECK(length(request_digest)=64 AND request_digest NOT GLOB '*[^0-9a-f]*'),
        run_id TEXT,
        protocol_version INTEGER NOT NULL CHECK(protocol_version IN (1,2)),
        state TEXT NOT NULL CHECK(state IN ('pending','decided','invalid','unsupported')),
        decision_command_id TEXT UNIQUE REFERENCES product_commands(command_id),
        deciding_actor_id TEXT REFERENCES clients(client_id),
        choice_digest TEXT CHECK(choice_digest IS NULL OR (length(choice_digest)=64 AND choice_digest NOT GLOB '*[^0-9a-f]*')),
        decision_kind TEXT CHECK(decision_kind IN ('allow_once','allow_always','reject_once','reject_always')),
        CHECK(state!='decided' OR (decision_command_id IS NOT NULL AND deciding_actor_id IS NOT NULL AND choice_digest IS NOT NULL AND decision_kind IS NOT NULL)),
        UNIQUE(load_epoch,native_request_digest),
        FOREIGN KEY(target_id,load_epoch) REFERENCES product_resources(target_id,load_epoch),
        FOREIGN KEY(target_id,run_id) REFERENCES product_runs(target_id,run_id)) STRICT",
    r"CREATE TABLE product_deliveries (
        approval_id TEXT PRIMARY KEY REFERENCES product_approvals(approval_id),
        state TEXT NOT NULL CHECK(state IN ('not_sent','claimed','transport_written','unknown','suppressed')),
        claim_id TEXT UNIQUE CHECK(claim_id IS NULL OR length(claim_id)=22),
        CHECK(state NOT IN ('claimed','transport_written','unknown') OR claim_id IS NOT NULL)) STRICT",
    r"CREATE TABLE product_audit_slots (
        command_id TEXT NOT NULL REFERENCES product_commands(command_id),
        stage TEXT NOT NULL CHECK(stage IN ('admission','dispatch','result','reconciliation')),
        actor_id TEXT NOT NULL REFERENCES clients(client_id),
        reserved_at INTEGER NOT NULL CHECK(reserved_at BETWEEN 0 AND 9007199254740991),
        occurred_at INTEGER CHECK(occurred_at BETWEEN 0 AND 9007199254740991),
        retain_until INTEGER CHECK(retain_until BETWEEN 0 AND 9007199254740991),
        outcome TEXT CHECK(outcome IN ('accepted','dispatching','completed','failed','unknown','expired','reconciled','denied','cancelled')),
        CHECK((occurred_at IS NULL AND retain_until IS NULL AND outcome IS NULL) OR
              (occurred_at IS NOT NULL AND retain_until IS NOT NULL AND outcome IS NOT NULL AND retain_until>=occurred_at AND retain_until-occurred_at>=7776000000)),
        PRIMARY KEY(command_id,stage)) STRICT",
    r"CREATE UNIQUE INDEX product_one_foreground_run ON product_runs(target_id) WHERE status IN ('accepted','dispatching','running','cancelling','unknown')",
    r"CREATE UNIQUE INDEX product_one_resource_owner ON product_resources(target_id) WHERE state!='released'",
];
const LEGACY_CORE: &[&str] = &[
    r"CREATE TABLE IF NOT EXISTS node_meta (
                singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                schema_version INTEGER NOT NULL,
                node_id TEXT NOT NULL,
                public_key BLOB NOT NULL,
                key_file_ref TEXT NOT NULL
            )",
    r"CREATE TABLE IF NOT EXISTS clients (
                client_id TEXT PRIMARY KEY,
                public_key BLOB NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                status TEXT NOT NULL CHECK (status IN ('trusted', 'revoked')),
                auth_generation INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                revoked_at INTEGER
            )",
    r"CREATE TABLE IF NOT EXISTS management_commands (
                actor_id TEXT NOT NULL,
                request_id TEXT NOT NULL,
                action TEXT NOT NULL,
                payload_digest TEXT NOT NULL,
                result_json TEXT NOT NULL,
                expires_at INTEGER NOT NULL,
                PRIMARY KEY (actor_id, request_id)
            )",
    r"CREATE TABLE IF NOT EXISTS security_audit (
                audit_id INTEGER PRIMARY KEY AUTOINCREMENT,
                actor_id TEXT,
                action TEXT NOT NULL,
                target_id TEXT,
                outcome TEXT NOT NULL,
                occurred_at INTEGER NOT NULL
            )",
    r"CREATE TABLE IF NOT EXISTS node_state (
                singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                revision INTEGER NOT NULL
            )",
];

const LEGACY_SESSION: &[&str] = &[
    r"CREATE TABLE harnesses(harness_id TEXT PRIMARY KEY, name TEXT NOT NULL, executable TEXT NOT NULL, arguments_json TEXT NOT NULL, config_ref TEXT)",
    r"CREATE TABLE workspaces(workspace_id TEXT PRIMARY KEY, canonical_path TEXT NOT NULL)",
    r"CREATE TABLE sessions(session_id TEXT PRIMARY KEY, harness_id TEXT NOT NULL REFERENCES harnesses(harness_id), workspace_id TEXT NOT NULL REFERENCES workspaces(workspace_id), state TEXT NOT NULL CHECK(state IN ('loaded','released','releasing','interrupted')), native_scope TEXT, native_session_id TEXT, recovery_json TEXT)",
    r"CREATE TABLE content_objects(content_ref TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(session_id), actor_id TEXT NOT NULL REFERENCES clients(client_id), auth_generation INTEGER NOT NULL CHECK(auth_generation >= 0), purpose TEXT NOT NULL CHECK(purpose IN ('input','output')), body BLOB NOT NULL, sha256 TEXT NOT NULL CHECK(length(sha256) = 64), total_bytes INTEGER NOT NULL CHECK(total_bytes >= 0 AND total_bytes <= 16777216 AND length(body) = total_bytes))",
    r"CREATE TABLE session_commands(command_id TEXT PRIMARY KEY, actor_id TEXT NOT NULL REFERENCES clients(client_id), request_id TEXT NOT NULL, action TEXT NOT NULL, payload_digest TEXT NOT NULL, session_id TEXT NOT NULL REFERENCES sessions(session_id), status TEXT NOT NULL CHECK(status IN ('accepted','dispatching','completed','failed','delivery_unknown')), UNIQUE(actor_id, request_id))",
    r"CREATE TABLE turns(turn_id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(session_id), command_id TEXT NOT NULL UNIQUE REFERENCES session_commands(command_id), ordinal INTEGER NOT NULL CHECK(ordinal >= 0 AND ordinal <= 9007199254740991), UNIQUE(session_id, ordinal))",
    r"CREATE TABLE runs(run_id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(session_id), command_id TEXT NOT NULL UNIQUE REFERENCES session_commands(command_id), turn_id TEXT NOT NULL UNIQUE REFERENCES turns(turn_id), status TEXT NOT NULL CHECK(status IN ('accepted','dispatching','running','cancelling','completed','cancelled','failed','interrupted','unknown')))",
    r"CREATE TABLE conversation_items(item_id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(session_id), turn_id TEXT NOT NULL REFERENCES turns(turn_id), source_id TEXT NOT NULL, first_position INTEGER NOT NULL CHECK(first_position >= 0 AND first_position <= 9007199254740991), kind TEXT NOT NULL, protocol TEXT NOT NULL DEFAULT 'platform', protocol_version TEXT NOT NULL DEFAULT '1', native_tool_id TEXT, native_tool_digest TEXT, tool_instance_id TEXT, tool_generation INTEGER, tool_native_session_id TEXT, tool_native_session_digest TEXT, content_ref TEXT NOT NULL REFERENCES content_objects(content_ref), content_version INTEGER NOT NULL CHECK(content_version >= 1 AND content_version <= 9007199254740991), committed_version INTEGER NOT NULL CHECK(committed_version >= 1 AND committed_version <= content_version), UNIQUE(session_id, first_position), UNIQUE(session_id, source_id))",
    r"CREATE TABLE approval_connections(instance_id TEXT PRIMARY KEY, generation INTEGER NOT NULL CHECK(generation >= 0 AND generation <= 9007199254740991), state TEXT NOT NULL CHECK(state IN ('active','closed')))",
    r"CREATE TABLE approval_run_sources(run_id TEXT PRIMARY KEY REFERENCES runs(run_id), session_id TEXT NOT NULL REFERENCES sessions(session_id), turn_id TEXT NOT NULL REFERENCES turns(turn_id), instance_id TEXT NOT NULL REFERENCES approval_connections(instance_id), generation INTEGER NOT NULL CHECK(generation >= 0 AND generation <= 9007199254740991), native_session_id TEXT NOT NULL, state TEXT NOT NULL CHECK(state IN ('active','invalid')), invalid_reason TEXT)",
    r"CREATE TABLE approvals(approval_id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(session_id), run_id TEXT NOT NULL REFERENCES runs(run_id), turn_id TEXT NOT NULL REFERENCES turns(turn_id), instance_id TEXT NOT NULL REFERENCES approval_connections(instance_id), connection_generation INTEGER NOT NULL, native_session_id TEXT NOT NULL, native_request_id TEXT NOT NULL, source_digest TEXT NOT NULL, request_kind TEXT NOT NULL, protocol TEXT NOT NULL, protocol_version TEXT NOT NULL, raw_request BLOB NOT NULL CHECK(length(raw_request) <= 16777216), scope_raw BLOB NOT NULL CHECK(length(scope_raw) <= 16777216), tool_native_id TEXT, tool_item_id TEXT, tool_source_id TEXT, status TEXT NOT NULL CHECK(status IN ('pending','decided','unsupported','invalid')), restriction TEXT, invalid_reason TEXT, decision_command_id TEXT UNIQUE REFERENCES session_commands(command_id), selected_choice_id TEXT REFERENCES approval_options(choice_id), deciding_actor_id TEXT REFERENCES clients(client_id), deciding_generation INTEGER, UNIQUE(instance_id,connection_generation,native_request_id), FOREIGN KEY(session_id,tool_item_id,tool_source_id) REFERENCES conversation_items(session_id,item_id,source_id))",
    r"CREATE TABLE approval_options(choice_id TEXT PRIMARY KEY, approval_id TEXT NOT NULL REFERENCES approvals(approval_id), ordinal INTEGER NOT NULL CHECK(ordinal >= 0), native_id TEXT NOT NULL, name TEXT NOT NULL, kind TEXT NOT NULL, UNIQUE(approval_id,ordinal))",
    r"CREATE TABLE approval_deliveries(approval_id TEXT PRIMARY KEY REFERENCES approvals(approval_id), state TEXT NOT NULL CHECK(state IN ('not_sent','claimed','transport_written','unknown','suppressed')), claim_id TEXT UNIQUE, write_report TEXT CHECK(write_report IN ('write_completed','unknown')), source_current_at_report INTEGER CHECK(source_current_at_report IN (0,1)))",
    r"CREATE TABLE approval_audit_links(audit_id INTEGER PRIMARY KEY REFERENCES security_audit(audit_id), approval_id TEXT NOT NULL REFERENCES approvals(approval_id), command_id TEXT REFERENCES session_commands(command_id), basis TEXT NOT NULL, actor_generation INTEGER, instance_id TEXT NOT NULL, connection_generation INTEGER NOT NULL)",
    r"CREATE TABLE session_state(session_id TEXT PRIMARY KEY REFERENCES sessions(session_id), revision INTEGER NOT NULL CHECK(revision >= 0 AND revision <= 9007199254740991), next_position INTEGER NOT NULL CHECK(next_position >= 0 AND next_position <= 9007199254740991), next_turn INTEGER NOT NULL CHECK(next_turn >= 0 AND next_turn <= 9007199254740991))",
    r"CREATE TABLE history_integrity(session_id TEXT NOT NULL REFERENCES sessions(session_id), run_id TEXT NOT NULL REFERENCES runs(run_id), reason TEXT NOT NULL, first_position INTEGER, last_position INTEGER, dropped_bytes INTEGER NOT NULL CHECK(dropped_bytes >= 0), PRIMARY KEY(session_id,run_id), CHECK((first_position IS NULL AND last_position IS NULL) OR (first_position >= 0 AND last_position >= first_position)))",
    r"CREATE TABLE history_content_pins(pin_id TEXT PRIMARY KEY, content_ref TEXT NOT NULL REFERENCES content_objects(content_ref))",
    r"CREATE UNIQUE INDEX tool_reference_key ON conversation_items(session_id,item_id,source_id)",
    r"CREATE UNIQUE INDEX tool_native_identity ON conversation_items(session_id,turn_id,tool_instance_id,tool_generation,tool_native_session_digest,native_tool_digest) WHERE native_tool_digest IS NOT NULL",
    r"CREATE UNIQUE INDEX one_foreground_run ON runs(session_id) WHERE status IN ('accepted','dispatching','running','cancelling','unknown')",
];
