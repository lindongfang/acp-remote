//! Real historical-DDL migration fixtures; no current-head downgrade is used as upgrade proof.
use super::*;
use crate::session_storage::schema::{self, MigrationFault};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

async fn pool() -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .in_memory(true)
                .foreign_keys(true),
        )
        .await
        .expect("owned SQLite fixture")
}
async fn historical12(pool: &SqlitePool) {
    sqlx::raw_sql(include_str!("../session_capacity/schema12_fixture.sql"))
        .execute(pool)
        .await
        .expect("pinned schema12 DDL, before schema13 existed");
    sqlx::raw_sql("INSERT INTO node_state VALUES(1,0); INSERT INTO product_control VALUES(1,'AAAAAAAAAAAAAAAAAAAAAA',0); PRAGMA user_version=12;")
        .execute(pool).await.expect("historical empty identity state");
}
async fn head(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(pool)
        .await
        .expect("version")
}
async fn auto_objects(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM main.sqlite_schema WHERE name LIKE 'node_session_automatic_%'",
    )
    .fetch_one(pool)
    .await
    .expect("necessary objects")
}

#[tokio::test]
async fn automatic_namespace_partial_at_old_head_is_rejected() {
    for current in [0, 12] {
        for object in OBJECTS {
            for surface in ["main", "temp"] {
                let pool = pool().await;
                if current == 12 {
                    historical12(&pool).await;
                }
                sqlx::query(&format!("CREATE TABLE {surface}.{object}(value TEXT)"))
                    .execute(&pool)
                    .await
                    .expect("foreign partial namespace fixture");
                assert!(
                    schema::initialize(&pool).await.is_err(),
                    "old head must reject a preexisting necessary automatic-release object: {current} {surface}.{object}"
                );
                assert_eq!(head(&pool).await, current);
                pool.close().await;
            }
        }
    }
}

#[tokio::test]
async fn automatic_schema13_preserves_real_historical12_definitions_and_empty_identity() {
    let pool = pool().await;
    historical12(&pool).await;
    let before: Vec<(String, String)> = sqlx::query_as(
        "SELECT name,sql FROM main.sqlite_schema WHERE sql IS NOT NULL ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .expect("exact old layout snapshot");
    assert_eq!(head(&pool).await, 12);
    assert_eq!(auto_objects(&pool).await, 0);
    schema::initialize(&pool)
        .await
        .expect("real historical12 migration");
    assert_eq!(head(&pool).await, crate::storage::SCHEMA_VERSION);
    assert_eq!(auto_objects(&pool).await, 3);
    for (name, definition) in before {
        let current: String = sqlx::query_scalar("SELECT sql FROM main.sqlite_schema WHERE name=?")
            .bind(name)
            .fetch_one(&pool)
            .await
            .expect("preserved object");
        assert_eq!(current, definition);
    }
    let mut connection = pool.acquire().await.expect("same owned SQLite");
    require_empty(&mut connection)
        .await
        .expect("no adopted automatic operation");
    drop(connection);
    schema::initialize(&pool).await.expect("head revalidation");
    pool.close().await;
}

#[tokio::test]
async fn automatic_schema13_each_necessary_boundary_rolls_back_and_retries() {
    for fault in [
        MigrationFault::AutomaticReleaseDdl(0),
        MigrationFault::AutomaticReleaseDdl(1),
        MigrationFault::AutomaticReleaseDdl(2),
        MigrationFault::AutomaticReleaseMetadata,
        MigrationFault::AutomaticReleasePragma,
        MigrationFault::Validation,
        MigrationFault::Commit,
        MigrationFault::DeferredCommit,
    ] {
        let pool = pool().await;
        historical12(&pool).await;
        assert!(
            schema::initialize_for_test(&pool, fault).await.is_err(),
            "{fault:?}"
        );
        assert_eq!(head(&pool).await, 12, "{fault:?}");
        assert_eq!(auto_objects(&pool).await, 0, "{fault:?}");
        schema::initialize(&pool)
            .await
            .expect("retry same historical database");
        schema::initialize(&pool).await.expect("head validates");
        pool.close().await;
    }
    let pool = pool().await;
    historical12(&pool).await;
    assert!(
        schema::initialize_for_test(&pool, MigrationFault::CommitUnknown)
            .await
            .is_err()
    );
    assert_eq!(
        head(&pool).await,
        crate::storage::SCHEMA_VERSION,
        "unknown report does not undo actual commit"
    );
    assert_eq!(auto_objects(&pool).await, 3);
    schema::initialize(&pool)
        .await
        .expect("unknown commit retry validates actual head");
    pool.close().await;
}

#[tokio::test]
async fn automatic_schema13_rejects_triggers_shadows_extra_indexes_and_changed_layout() {
    for sql in [
        "CREATE TRIGGER auto_injected AFTER INSERT ON node_session_automatic_releases BEGIN SELECT 1; END",
        "CREATE TEMP TRIGGER auto_injected AFTER INSERT ON main.node_session_automatic_releases BEGIN SELECT 1; END",
        "CREATE TRIGGER auto_injected AFTER INSERT ON node_session_automatic_release_members BEGIN SELECT 1; END",
        "CREATE TEMP TRIGGER auto_injected AFTER INSERT ON main.node_session_automatic_release_members BEGIN SELECT 1; END",
        "CREATE INDEX auto_extra ON node_session_automatic_releases(phase)",
        "CREATE INDEX auto_extra ON node_session_automatic_release_members(reservation_id)",
        "DROP INDEX node_session_automatic_release_member_lookup",
        "ALTER TABLE node_session_automatic_releases ADD COLUMN ignored TEXT",
        "ALTER TABLE node_session_automatic_release_members ADD COLUMN ignored TEXT",
        "CREATE TEMP VIEW node_session_automatic_releases AS SELECT 1 AS operation_id",
        "CREATE TEMP VIEW node_session_automatic_release_members AS SELECT 1 AS operation_id",
        "CREATE TEMP VIEW node_session_automatic_release_member_lookup AS SELECT 1 AS operation_id",
    ] {
        let pool = pool().await;
        schema::initialize(&pool).await.expect("current layout");
        sqlx::query(sql)
            .execute(&pool)
            .await
            .expect("owned malformed fixture");
        assert!(schema::initialize(&pool).await.is_err(), "{sql}");
        assert_eq!(
            head(&pool).await,
            crate::storage::SCHEMA_VERSION,
            "never repairs foreign layout"
        );
        pool.close().await;
    }
}

#[tokio::test]
async fn automatic_schema13_nonempty_history_requires_identity_and_checks_foreign_keys() {
    let pool = pool().await;
    schema::initialize(&pool).await.expect("fresh");
    let mut connection = pool.acquire().await.expect("owned connection");
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&mut *connection)
        .await
        .expect("malformed fixture only");
    sqlx::query("INSERT INTO node_session_automatic_releases VALUES('AAAAAAAAAAAAAAAAAAAAAA','BBBBBBBBBBBBBBBBBBBBBB','idle_timeout','pending',NULL)")
        .execute(&mut *connection).await.expect("foreign anchor fixture");
    assert!(matches!(
        require_empty(&mut connection).await,
        Err(StorageError::MissingNodeMeta)
    ));
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&mut *connection)
        .await
        .expect("restore foreign keys");
    assert!(
        validate(&mut connection, 13).await.is_err(),
        "necessary FK integrity is mandatory"
    );
    drop(connection);
    pool.close().await;
}
