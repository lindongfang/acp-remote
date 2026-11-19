//! Development regression tests; the synthetic v8 layout is not a historical acceptance fixture.
use super::*;
use crate::session_storage::schema::{self, MigrationFault};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;
async fn pool() -> Result<sqlx::SqlitePool, sqlx::Error> {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .in_memory(true)
                .foreign_keys(true),
        )
        .await
}
async fn synthetic_eight(pool: &sqlx::SqlitePool) -> Result<(), StorageError> {
    schema::initialize(pool).await?;
    super::super::automatic_capacity_release_schema::remove_owned_fixture_for_test(pool).await;
    sqlx::query("DROP TABLE main.node_session_capacity_releases")
        .execute(pool)
        .await
        .expect("remove generated capacity associations");
    sqlx::query("DROP TABLE main.node_session_capacity")
        .execute(pool)
        .await
        .expect("remove generated capacity fixture");
    for index in (0..super::super::source_sessions_schema::DEFINITIONS.len()).rev() {
        let definition = super::super::source_sessions_schema::DEFINITIONS[index];
        if definition.starts_with("CREATE INDEX") {
            let name = definition.split_whitespace().nth(2).unwrap();
            sqlx::query(&format!("DROP INDEX main.{name}"))
                .execute(pool)
                .await?;
        } else {
            let name = definition
                .split_whitespace()
                .nth(2)
                .unwrap()
                .split('(')
                .next()
                .unwrap();
            sqlx::query(&format!("DROP TABLE main.{name}"))
                .execute(pool)
                .await?;
        }
    }
    for table in TABLES {
        sqlx::query(&format!("DROP TABLE main.{table}"))
            .execute(pool)
            .await?;
    }
    sqlx::query("DROP INDEX main.node_native_release_actor_lookup")
        .execute(pool)
        .await?;
    sqlx::query("DROP TABLE main.node_native_release_operations")
        .execute(pool)
        .await?;
    let source_objects: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM main.sqlite_schema WHERE name IN ('node_source_session_registrations','node_source_session_operations','node_source_session_runtime_operations','node_source_session_created_owners')",
    )
    .fetch_one(pool)
    .await?;
    if source_objects != 0 {
        return Err(StorageError::UnrecognizedSchema);
    }
    sqlx::query("INSERT INTO main.node_meta(singleton,schema_version,node_id,public_key,key_file_ref) VALUES(1,8,'catalog-schema8-fixture',X'01','catalog-schema8-key')")
        .execute(pool)
        .await?;
    sqlx::query("PRAGMA main.user_version=8")
        .execute(pool)
        .await?;
    schema::install_legacy_session_for_test(pool).await?;
    Ok(())
}
async fn head_and_tables(pool: &sqlx::SqlitePool) -> Result<(i64, i64), sqlx::Error> {
    let head = sqlx::query_scalar("PRAGMA main.user_version")
        .fetch_one(pool)
        .await?;
    let count = sqlx::query_scalar("SELECT COUNT(*) FROM main.sqlite_schema WHERE type='table' AND name IN ('node_catalog_commands','node_launch_profiles','node_target_launch_selection')").fetch_one(pool).await?;
    let metadata: Option<i64> =
        sqlx::query_scalar("SELECT schema_version FROM main.node_meta WHERE singleton=1")
            .fetch_optional(pool)
            .await?;
    if let Some(metadata) = metadata {
        assert_eq!(metadata, head, "metadata and PRAGMA heads must agree");
    }
    Ok((head, count))
}

#[tokio::test]
async fn fresh_migration_is_valid_and_reopening_keeps_layout() -> TestResult {
    let pool = pool().await?;
    schema::initialize(&pool).await?;
    assert_eq!(
        head_and_tables(&pool).await?,
        (crate::storage::SCHEMA_VERSION, 3)
    );
    let before: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT name,sql FROM main.sqlite_schema ORDER BY name")
            .fetch_all(&pool)
            .await?;
    schema::initialize(&pool).await?;
    let after: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT name,sql FROM main.sqlite_schema ORDER BY name")
            .fetch_all(&pool)
            .await?;
    assert_eq!(before, after);
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn every_catalog_precommit_fault_rolls_back_original_eight() -> TestResult {
    for fault in [
        MigrationFault::CatalogDdlBefore(1),
        MigrationFault::CatalogDdlAfter(1),
        MigrationFault::CatalogDdlBefore(2),
        MigrationFault::CatalogDdlAfter(2),
        MigrationFault::CatalogDdlBefore(3),
        MigrationFault::CatalogDdlAfter(3),
        MigrationFault::CatalogMetadata,
        MigrationFault::CatalogPragma,
        MigrationFault::Validation,
        MigrationFault::Commit,
        MigrationFault::DeferredCommit,
    ] {
        let pool = pool().await?;
        synthetic_eight(&pool).await?;
        let before: Vec<(String, Option<String>)> =
            sqlx::query_as("SELECT name,sql FROM main.sqlite_schema ORDER BY name")
                .fetch_all(&pool)
                .await?;
        assert!(
            schema::initialize_for_test(&pool, fault).await.is_err(),
            "{fault:?}"
        );
        assert_eq!(head_and_tables(&pool).await?, (8, 0), "{fault:?}");
        let identity: (i64, String, Vec<u8>, String) = sqlx::query_as(
            "SELECT schema_version,node_id,public_key,key_file_ref FROM main.node_meta WHERE singleton=1",
        )
        .fetch_one(&pool)
        .await?;
        assert_eq!(
            identity,
            (
                8,
                "catalog-schema8-fixture".into(),
                vec![1],
                "catalog-schema8-key".into()
            ),
            "{fault:?}"
        );
        let after: Vec<(String, Option<String>)> =
            sqlx::query_as("SELECT name,sql FROM main.sqlite_schema ORDER BY name")
                .fetch_all(&pool)
                .await?;
        assert_eq!(before, after, "{fault:?}");
        let source_objects: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM main.sqlite_schema WHERE name IN ('node_source_session_registrations','node_source_session_operations','node_source_session_runtime_operations','node_source_session_created_owners')",
        )
        .fetch_one(&pool)
        .await?;
        assert_eq!(source_objects, 0, "{fault:?}");
        pool.close().await;
    }
    Ok(())
}

#[tokio::test]
async fn lost_report_after_real_commit_keeps_complete_current() -> TestResult {
    let pool = pool().await?;
    synthetic_eight(&pool).await?;
    assert!(matches!(
        schema::initialize_for_test(&pool, MigrationFault::CommitUnknown).await,
        Err(StorageError::MigrationInterrupted)
    ));
    assert_eq!(
        head_and_tables(&pool).await?,
        (crate::storage::SCHEMA_VERSION, 3)
    );
    let release_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM main.sqlite_schema WHERE type='table' AND name='node_native_release_operations'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(release_tables, 1);
    schema::initialize(&pool).await?;
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn fresh_namespace_shadow_and_partial_objects_are_rejected() -> TestResult {
    for ddl in [
        "CREATE TEMP VIEW node_meta AS SELECT 1 AS singleton",
        "CREATE TEMP VIEW pragma_foreign_key_list AS SELECT 1 AS id",
        "CREATE TABLE node_launch_profiles(value TEXT)",
        "CREATE VIEW node_catalog_commands AS SELECT 1 AS value",
    ] {
        let pool = pool().await?;
        sqlx::query(ddl).execute(&pool).await?;
        assert!(
            matches!(
                schema::initialize(&pool).await,
                Err(StorageError::UnrecognizedSchema)
            ),
            "{ddl}"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("PRAGMA main.user_version")
                .fetch_one(&pool)
                .await?,
            0
        );
        pool.close().await;
    }
    Ok(())
}

#[tokio::test]
async fn required_catalog_trigger_and_external_index_are_rejected() -> TestResult {
    for ddl in [
        "CREATE INDEX extra_catalog_index ON node_catalog_commands(actor_id)",
        "CREATE TEMP TRIGGER catalog_shadow AFTER INSERT ON main.node_catalog_commands BEGIN SELECT 1; END",
    ] {
        let pool = pool().await?;
        schema::initialize(&pool).await?;
        sqlx::query(ddl).execute(&pool).await?;
        assert!(
            matches!(
                schema::initialize(&pool).await,
                Err(StorageError::UnrecognizedSchema)
            ),
            "{ddl}"
        );
        pool.close().await;
    }
    Ok(())
}

#[tokio::test]
async fn invalid_catalog_ordinals_return_before_pool_acquisition() -> TestResult {
    let pool = pool().await?;
    let held = pool.acquire().await?;
    for ordinal in [0, 4, u8::MAX] {
        for fault in [
            MigrationFault::CatalogDdlBefore(ordinal),
            MigrationFault::CatalogDdlAfter(ordinal),
        ] {
            let result = tokio::time::timeout(
                std::time::Duration::from_millis(100),
                schema::initialize_for_test(&pool, fault),
            )
            .await?;
            assert!(matches!(result, Err(StorageError::MigrationInterrupted)));
        }
    }
    held.close().await?;
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn catalog_history_without_identity_is_missing_metadata() -> TestResult {
    let pool = pool().await?;
    schema::initialize(&pool).await?;
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO main.node_catalog_commands(request_id,actor_id,register_auth_generation,action,state,registered_at_ms,intent_cipher,intent_digest) VALUES('AAAAAAAAAAAAAAAAAAAAAA','missing-client',0,'harness.install','registered',0,X'01',zeroblob(32))").execute(&pool).await?;
    sqlx::query("PRAGMA foreign_keys=ON").execute(&pool).await?;
    assert!(matches!(
        schema::initialize(&pool).await,
        Err(StorageError::MissingNodeMeta)
    ));
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn catalog_foreign_key_damage_is_necessary_integrity_failure() -> TestResult {
    let pool = pool().await?;
    schema::initialize(&pool).await?;
    sqlx::query("INSERT INTO main.node_meta VALUES(1,9,'AAAAAAAAAAAAAAAAAAAAAA',X'01','dev-key')")
        .execute(&pool)
        .await?;
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO main.node_catalog_commands(request_id,actor_id,register_auth_generation,action,state,registered_at_ms,intent_cipher,intent_digest) VALUES('AAAAAAAAAAAAAAAAAAAAAA','missing-client',0,'harness.install','registered',0,X'01',zeroblob(32))").execute(&pool).await?;
    sqlx::query("PRAGMA foreign_keys=ON").execute(&pool).await?;
    assert!(matches!(
        schema::initialize(&pool).await,
        Err(StorageError::UnrecognizedSchema)
    ));
    pool.close().await;
    Ok(())
}
