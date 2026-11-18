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
        .expect("owned SQLite")
}

async fn historical13(pool: &SqlitePool) {
    sqlx::raw_sql(include_str!("../session_capacity/schema12_fixture.sql"))
        .execute(pool)
        .await
        .expect("fixed historical schema12");
    sqlx::raw_sql(include_str!("schema13_additions_fixture.sql"))
        .execute(pool)
        .await
        .expect("fixed historical schema13 additions");
}

async fn head(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(pool)
        .await
        .expect("head")
}

#[tokio::test]
async fn session_tree_reserved_namespace_red_check() {
    for current in [0, 13] {
        for surface in ["main", "temp"] {
            let pool = pool().await;
            if current == 13 {
                historical13(&pool).await;
            }
            sqlx::query(&format!(
                "CREATE TABLE {surface}.node_session_parents(value TEXT)"
            ))
            .execute(&pool)
            .await
            .expect("foreign future namespace");
            assert!(schema::initialize(&pool).await.is_err());
            assert_eq!(head(&pool).await, current);
            pool.close().await;
        }
    }
}

#[tokio::test]
async fn session_tree_schema14_preserves_historical13_and_adds_no_facts() {
    let pool = pool().await;
    historical13(&pool).await;
    let before: Vec<(String, String)> = sqlx::query_as(
        "SELECT name,sql FROM main.sqlite_schema WHERE sql IS NOT NULL ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .expect("old DDL snapshot");
    schema::initialize(&pool).await.expect("additive migration");
    assert_eq!(head(&pool).await, 14);
    for (name, definition) in before {
        let after: String = sqlx::query_scalar("SELECT sql FROM main.sqlite_schema WHERE name=?")
            .bind(name)
            .fetch_one(&pool)
            .await
            .expect("preserved object");
        assert_eq!(after, definition);
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_session_parents")
        .fetch_one(&pool)
        .await
        .expect("no invented facts");
    assert_eq!(count, 0);
    schema::initialize(&pool).await.expect("reopen validates");
    pool.close().await;
}

#[tokio::test]
async fn session_tree_schema14_necessary_boundaries_roll_back() {
    for fault in [
        MigrationFault::SessionTreeDdl,
        MigrationFault::SessionTreeMetadata,
        MigrationFault::SessionTreePragma,
        MigrationFault::Validation,
        MigrationFault::Commit,
        MigrationFault::DeferredCommit,
    ] {
        let pool = pool().await;
        historical13(&pool).await;
        assert!(
            schema::initialize_for_test(&pool, fault).await.is_err(),
            "{fault:?}"
        );
        assert_eq!(head(&pool).await, 13);
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_schema WHERE name='node_session_parents'",
        )
        .fetch_one(&pool)
        .await
        .expect("no partial");
        assert_eq!(count, 0);
        schema::initialize(&pool)
            .await
            .expect("same database retry");
        assert_eq!(head(&pool).await, 14);
        pool.close().await;
    }
    let pool = pool().await;
    historical13(&pool).await;
    assert!(
        schema::initialize_for_test(&pool, MigrationFault::CommitUnknown)
            .await
            .is_err()
    );
    assert_eq!(head(&pool).await, 14);
    schema::initialize(&pool)
        .await
        .expect("actual committed version revalidated");
    pool.close().await;
}

#[tokio::test]
async fn session_tree_schema14_rejects_shadow_triggers_indexes_and_layout_changes() {
    for statement in [
        "CREATE TRIGGER tree_injected AFTER INSERT ON node_session_parents BEGIN SELECT 1; END",
        "CREATE TEMP TRIGGER tree_injected AFTER INSERT ON main.node_session_parents BEGIN SELECT 1; END",
        "CREATE TEMP VIEW node_session_parents AS SELECT 1 AS source_id",
        "CREATE INDEX tree_extra ON node_session_parents(parent_native_id)",
        "ALTER TABLE node_session_parents ADD COLUMN ignored TEXT",
        "DROP TABLE node_session_parents",
    ] {
        let pool = pool().await;
        schema::initialize(&pool).await.expect("current layout");
        sqlx::query(statement)
            .execute(&pool)
            .await
            .expect("owned malformed layout");
        assert!(schema::initialize(&pool).await.is_err(), "{statement}");
        assert_eq!(head(&pool).await, 14);
        pool.close().await;
    }
}

#[tokio::test]
async fn session_tree_schema14_checks_foreign_keys_and_empty_identity() {
    let pool = pool().await;
    schema::initialize(&pool).await.expect("current");
    let mut connection = pool.acquire().await.expect("same connection");
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&mut *connection)
        .await
        .expect("owned corruption fixture");
    sqlx::query("INSERT INTO node_session_parents VALUES('AAAAAAAAAAAAAAAAAAAAAA','child','parent','AAAAAAAAAAAAAAAAAAAAAA','AAAAAAAAAAAAAAAAAAAAAA','AAAAAAAAAAAAAAAAAAAAAA',X'0000000000000001',X'0000000000000001',zeroblob(32),1)")
        .execute(&mut *connection).await.expect("owned orphan");
    assert!(matches!(
        require_empty(&mut connection).await,
        Err(StorageError::MissingNodeMeta)
    ));
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&mut *connection)
        .await
        .expect("restore");
    assert!(validate(&mut connection, 14).await.is_err());
    drop(connection);
    pool.close().await;
}
