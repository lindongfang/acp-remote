//! Developer migration checks use only a fresh, owned in-memory database.
use crate::session_storage::{native_dispatch_schema, schema};
use sqlx::sqlite::SqlitePoolOptions;

#[tokio::test]
async fn exact_schema6_is_additive_and_validates_on_second_open() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("owned db");
    schema::initialize(&pool).await.expect("migration");
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&pool)
        .await
        .expect("version");
    assert_eq!(version, crate::storage::SCHEMA_VERSION);
    let tables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name LIKE 'node_native_%'",
    )
    .fetch_one(&pool)
    .await
    .expect("tables");
    assert_eq!(tables, 12);
    let indexes:i64=sqlx::query_scalar("SELECT count(*) FROM sqlite_schema WHERE type='index' AND sql IS NOT NULL AND name LIKE 'node_native_%'").fetch_one(&pool).await.expect("indexes");
    assert_eq!(indexes, 4);
    assert_eq!(native_dispatch_schema::DEFINITIONS.len(), 14);
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_schema WHERE name IN ('node_request_registrations','product_resources')",
    )
    .fetch_one(&pool)
    .await
    .expect("active layouts");
    assert_eq!(active, 2);
    let retired: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_schema WHERE name IN ('content_objects','conversation_items','approvals')",
    )
    .fetch_one(&pool)
    .await
    .expect("retired layouts absent");
    assert_eq!(retired, 0);
    schema::initialize(&pool)
        .await
        .expect("exact reopen validation");
    pool.close().await;
}

#[tokio::test]
async fn native_ddl_and_version_faults_roll_back_the_entire_migration() {
    for fault in [
        schema::MigrationFault::NativeDdl,
        schema::MigrationFault::NativeMetadata,
        schema::MigrationFault::NativePragma,
    ] {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("db");
        assert!(schema::initialize_for_test(&pool, fault).await.is_err());
        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&pool)
            .await
            .expect("version");
        assert_eq!(version, 0);
        let objects: i64 =
            sqlx::query_scalar("SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'")
                .fetch_one(&pool)
                .await
                .expect("objects");
        assert_eq!(objects, 0);
        schema::initialize(&pool)
            .await
            .expect("retry real migration");
        pool.close().await;
    }
}
