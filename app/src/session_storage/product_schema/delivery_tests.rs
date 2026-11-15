use super::{
    DEFINITIONS, DELIVERY_DEFINITIONS,
    tests::{Area, assert_preserved, connect, legacy},
};
use crate::{
    domain::EntityId,
    error::StorageError,
    session_storage::schema::{self, MigrationFault},
};
use sqlx::{Connection, SqliteConnection, sqlite::SqlitePoolOptions};
fn id(n: u8) -> String {
    EntityId::from_bytes([n; 16]).encode()
}
async fn execute(c: &mut SqliteConnection, sql: &str) {
    sqlx::query(sql).execute(c).await.unwrap();
}

/// A real schema3 fixture, constructed without opening/migrating through current Storage.
async fn schema_three(area: &Area) {
    legacy(area, 2).await;
    let mut c = connect(area).await;
    for ddl in DEFINITIONS {
        execute(&mut c, ddl).await;
    }
    sqlx::query("INSERT INTO product_control VALUES(1,?,321)")
        .bind(id(16))
        .execute(&mut c)
        .await
        .unwrap();
    sqlx::query("INSERT INTO product_sources VALUES(?,'source')")
        .bind(id(10))
        .execute(&mut c)
        .await
        .unwrap();
    sqlx::query("INSERT INTO product_harness_sources VALUES('legacy-h',?)")
        .bind(id(10))
        .execute(&mut c)
        .await
        .unwrap();
    sqlx::query("INSERT INTO product_targets VALUES(?,?,'native-current','legacy-w')")
        .bind(id(11))
        .bind(id(10))
        .execute(&mut c)
        .await
        .unwrap();
    sqlx::query("INSERT INTO product_resources VALUES(?,?,?,?,?,'loaded',2)")
        .bind(id(12))
        .bind(id(11))
        .bind(id(18))
        .bind(id(19))
        .bind(id(20))
        .execute(&mut c)
        .await
        .unwrap();
    for (command, request, action, kind, target, status, claim) in [
        (14, 15, "session.input", "session", 11, "completed", 25),
        (23, 24, "approval.decide", "approval", 21, "dispatching", 22),
    ] {
        sqlx::query("INSERT INTO product_commands VALUES(?,?,7,?,?,?, ?,?,?,100,2000,?,?)")
            .bind(id(command))
            .bind(id(1))
            .bind(id(request))
            .bind(id(16))
            .bind(action)
            .bind(kind)
            .bind(id(target))
            .bind("a".repeat(64))
            .bind(status)
            .bind(id(claim))
            .execute(&mut c)
            .await
            .unwrap();
        for stage in ["admission", "dispatch", "result", "reconciliation"] {
            sqlx::query("INSERT INTO product_audit_slots(command_id,stage,actor_id,reserved_at) VALUES(?,?,?,100)").bind(id(command)).bind(stage).bind(id(1)).execute(&mut c).await.unwrap();
        }
        sqlx::query("UPDATE product_audit_slots SET occurred_at=100,retain_until=7776000100,outcome='accepted' WHERE command_id=? AND stage='admission'").bind(id(command)).execute(&mut c).await.unwrap();
    }
    sqlx::query("INSERT INTO product_runs VALUES(?,?,?,'completed')")
        .bind(id(17))
        .bind(id(14))
        .bind(id(11))
        .execute(&mut c)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO product_approvals VALUES(?,?,?, ?,?,?,2,'decided',?,?,?,'allow_once')",
    )
    .bind(id(21))
    .bind(id(11))
    .bind(id(12))
    .bind("b".repeat(64))
    .bind("c".repeat(64))
    .bind(id(17))
    .bind(id(23))
    .bind(id(1))
    .bind("d".repeat(64))
    .execute(&mut c)
    .await
    .unwrap();
    sqlx::query("INSERT INTO product_deliveries VALUES(?,'claimed',?)")
        .bind(id(21))
        .bind(id(22))
        .execute(&mut c)
        .await
        .unwrap();
    execute(&mut c, "UPDATE node_meta SET schema_version=3").await;
    execute(&mut c, "PRAGMA user_version=3").await;
    c.close().await.unwrap();
    area.observe().await;
}

async fn version(c: &mut SqliteConnection) -> (i64, i64) {
    (
        sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&mut *c)
            .await
            .unwrap(),
        sqlx::query_scalar("SELECT schema_version FROM node_meta")
            .fetch_one(c)
            .await
            .unwrap(),
    )
}
async fn assert_product_preserved(c: &mut SqliteConnection) {
    assert_eq!(
        sqlx::query_as::<_, (String, i64)>(
            "SELECT request_epoch,time_floor_ms FROM product_control"
        )
        .fetch_one(&mut *c)
        .await
        .unwrap(),
        (id(16), 321)
    );
    let expected = [
        ("product_sources", 1),
        ("product_harness_sources", 1),
        ("product_targets", 1),
        ("product_resources", 1),
        ("product_commands", 2),
        ("product_runs", 1),
        ("product_approvals", 1),
        ("product_deliveries", 1),
        ("product_audit_slots", 8),
    ];
    for (table, count) in expected {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&mut *c)
                .await
                .unwrap(),
            count,
            "{table}"
        );
    }
    assert_eq!(
        sqlx::query_as::<_, (String, String)>("SELECT state,claim_id FROM product_deliveries")
            .fetch_one(&mut *c)
            .await
            .unwrap(),
        ("claimed".into(), id(22))
    );
    assert_eq!(
        sqlx::query_as::<_, (String, String, String)>(
            "SELECT state,run_id,decision_kind FROM product_approvals"
        )
        .fetch_one(c)
        .await
        .unwrap(),
        ("decided".into(), id(17), "allow_once".into())
    );
}

#[tokio::test]
async fn existing_three_migrates_without_inferring_attempt_or_changing_any_control_fact() {
    let a = Area::new();
    schema_three(&a).await;
    let storage = a.open().await.unwrap();
    storage.close().await;
    a.observe().await;
    let mut c = connect(&a).await;
    assert_preserved(&mut c, 2, crate::storage::SCHEMA_VERSION).await;
    assert_product_preserved(&mut c).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM product_delivery_attempts")
            .fetch_one(&mut c)
            .await
            .unwrap(),
        0
    );
    c.close().await.unwrap();
    a.observe().await;
    let reopened = a.open().await.unwrap();
    reopened.close().await;
    a.observe().await;
    let mut c = connect(&a).await;
    assert_eq!(
        version(&mut c).await,
        (
            crate::storage::SCHEMA_VERSION,
            crate::storage::SCHEMA_VERSION
        )
    );
    assert_product_preserved(&mut c).await;
    c.close().await.unwrap();
    a.observe().await;
}

#[tokio::test]
async fn every_four_upgrade_checkpoint_and_real_deferred_commit_roll_back_all_old_versions() {
    for old in [1, 2, 3] {
        for fault in [
            MigrationFault::Ddl,
            MigrationFault::Product3Ready,
            MigrationFault::DeliveryIndex,
            MigrationFault::DeliveryTable,
            MigrationFault::Metadata,
            MigrationFault::Pragma,
            MigrationFault::Validation,
            MigrationFault::Commit,
            MigrationFault::DeferredCommit,
        ] {
            let a = Area::new();
            if old == 3 {
                schema_three(&a).await;
            } else {
                legacy(&a, old).await;
            }
            let raw_scope = a.raw_scope().await;
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(raw_scope.options().foreign_keys(true))
                .await
                .unwrap();
            let result = schema::initialize_for_test(&pool, fault).await;
            assert!(result.is_err(), "{old} {fault:?}");
            if fault == MigrationFault::DeferredCommit {
                assert!(
                    matches!(result,Err(StorageError::Database(ref e)) if e.as_database_error().and_then(sqlx::error::DatabaseError::code).as_deref()==Some("787")),
                    "real FK COMMIT: {result:?}"
                );
            }
            pool.close().await;
            a.observe().await;
            let mut c = connect(&a).await;
            assert_preserved(&mut c, old.min(2), old).await;
            if old == 3 {
                assert_product_preserved(&mut c).await;
            }
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM sqlite_schema WHERE name IN ('product_delivery_attempts','product_delivery_claim_identity','migration_test_parent','migration_test_child')").fetch_one(&mut c).await.unwrap(),0);
            c.close().await.unwrap();
            a.observe().await;
        }
    }
}

#[tokio::test]
async fn commit_success_response_unknown_is_durable_four_not_a_rollback() {
    for old in [1, 2, 3] {
        let a = Area::new();
        if old == 3 {
            schema_three(&a).await;
        } else {
            legacy(&a, old).await;
        }
        let raw_scope = a.raw_scope().await;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(raw_scope.options().foreign_keys(true))
            .await
            .unwrap();
        assert!(matches!(
            schema::initialize_for_test(&pool, MigrationFault::CommitUnknown).await,
            Err(StorageError::MigrationInterrupted)
        ));
        pool.close().await;
        a.observe().await;
        let mut c = connect(&a).await;
        assert_preserved(&mut c, old.min(2), crate::storage::SCHEMA_VERSION).await;
        if old == 3 {
            assert_product_preserved(&mut c).await;
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM product_delivery_attempts")
                .fetch_one(&mut c)
                .await
                .unwrap(),
            0
        );
        c.close().await.unwrap();
        a.observe().await;
        let reopened = a.open().await.unwrap();
        reopened.close().await;
        a.observe().await;
    }
}

#[tokio::test]
async fn attempt_nullability_actual_kind_and_cross_delivery_claim_are_enforced_by_sqlite() {
    let a = Area::new();
    schema_three(&a).await;
    let storage = a.open().await.unwrap();
    storage.close().await;
    a.observe().await;
    let mut c = connect(&a).await;
    let digest = "e".repeat(64);
    let cases = [
        (
            "selected",
            Some(digest.as_str()),
            None,
            None,
            "claimed",
            true,
        ),
        (
            "selected",
            Some(digest.as_str()),
            Some("cancelled"),
            None,
            "transport_written",
            true,
        ),
        ("cancelled", None, Some("cancelled"), None, "unknown", true),
        ("selected", None, None, None, "claimed", false),
        (
            "cancelled",
            Some(digest.as_str()),
            None,
            None,
            "claimed",
            false,
        ),
        (
            "selected",
            Some(digest.as_str()),
            None,
            Some(digest.as_str()),
            "unknown",
            false,
        ),
        (
            "selected",
            Some(digest.as_str()),
            Some("selected"),
            None,
            "unknown",
            false,
        ),
        (
            "selected",
            Some(digest.as_str()),
            Some("cancelled"),
            Some(digest.as_str()),
            "unknown",
            false,
        ),
        (
            "selected",
            Some(digest.as_str()),
            Some("selected"),
            Some(digest.as_str()),
            "claimed",
            false,
        ),
        (
            "selected",
            Some(digest.as_str()),
            None,
            None,
            "transport_written",
            false,
        ),
    ];
    for (planned, planned_digest, actual, actual_digest, state, valid) in cases {
        execute(&mut c, "BEGIN").await;
        let result = sqlx::query(
            "INSERT INTO product_delivery_attempts VALUES(?,?,?,?,?,?,'user_choice',?,100)",
        )
        .bind(id(21))
        .bind(id(22))
        .bind(planned)
        .bind(planned_digest)
        .bind(actual)
        .bind(actual_digest)
        .bind(state)
        .execute(&mut c)
        .await;
        assert_eq!(
            result.is_ok(),
            valid,
            "{planned} {actual:?} {actual_digest:?} {state}: {result:?}"
        );
        execute(&mut c, "ROLLBACK").await;
    }
    assert!(sqlx::query("INSERT INTO product_delivery_attempts VALUES(?,?,'cancelled',NULL,NULL,NULL,'node_stop','claimed',100)").bind(id(21)).bind(id(99)).execute(&mut c).await.is_err());
    assert!(sqlx::query("INSERT INTO product_delivery_attempts VALUES(?,?,'cancelled',NULL,NULL,NULL,'node_stop','claimed',1.5)").bind(id(21)).bind(id(22)).execute(&mut c).await.is_err());
    c.close().await.unwrap();
    a.observe().await;
}

#[tokio::test]
async fn partial_future_and_corrupt_four_layouts_reject_before_application_writes() {
    for case in [
        "three-with-index",
        "three-with-full-four",
        "four-without-table",
        "four-without-index",
        "future",
    ] {
        let a = Area::new();
        schema_three(&a).await;
        if case.starts_with("four") || case == "future" {
            let storage = a.open().await.unwrap();
            storage.close().await;
            a.observe().await;
        }
        let mut c = connect(&a).await;
        match case {
            "three-with-index" => execute(&mut c, DELIVERY_DEFINITIONS[0]).await,
            "three-with-full-four" => {
                for ddl in DELIVERY_DEFINITIONS {
                    execute(&mut c, ddl).await;
                }
            }
            "four-without-table" => execute(&mut c, "DROP TABLE product_delivery_attempts").await,
            "four-without-index" => {
                execute(&mut c, "DROP INDEX product_delivery_claim_identity").await;
            }
            _ => {
                execute(&mut c, "PRAGMA user_version=77").await;
                execute(&mut c, "UPDATE node_meta SET schema_version=77").await;
            }
        }
        c.close().await.unwrap();
        a.observe().await;
        let before = std::fs::read(a.database()).unwrap();
        assert!(a.open().await.is_err(), "{case}");
        assert_eq!(std::fs::read(a.database()).unwrap(), before, "{case}");
    }
}
