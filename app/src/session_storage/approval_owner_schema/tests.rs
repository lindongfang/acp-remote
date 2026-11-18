//! Real SQLite migrations on fresh self-owned databases; no native grant is constructed.
#![allow(
    clippy::too_many_lines,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use crate::{
    domain::EntityId,
    error::StorageError,
    session_storage::{
        approval_owner_schema, management_audit_schema, native_dispatch_schema, product_schema,
        registration_schema, schema,
    },
};
use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};

async fn empty_pool() -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("fresh owned database")
}

async fn exact_seven() -> SqlitePool {
    let pool = empty_pool().await;
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .expect("begin fixture");
    for definition in schema::CORE
        .iter()
        .chain(schema::SESSION)
        .chain(product_schema::DEFINITIONS)
        .chain(product_schema::DELIVERY_DEFINITIONS)
        .chain(registration_schema::DEFINITIONS)
        .chain(native_dispatch_schema::DEFINITIONS)
    {
        sqlx::query(definition)
            .execute(&mut *tx)
            .await
            .expect("original schema1 through6 definition");
    }
    for definition in [
        management_audit_schema::TABLE_DDL,
        management_audit_schema::INDEX_DDL,
    ] {
        sqlx::query(definition)
            .execute(&mut *tx)
            .await
            .expect("original schema7");
    }
    sqlx::query(
        "INSERT INTO main.node_meta VALUES(1,7,'synthetic-migration-node',X'01','synthetic-key')",
    )
    .execute(&mut *tx)
    .await
    .expect("synthetic identity metadata");
    sqlx::query("INSERT INTO main.node_state VALUES(1,0)")
        .execute(&mut *tx)
        .await
        .expect("state");
    sqlx::query("INSERT INTO main.product_control VALUES(1,?,0)")
        .bind(EntityId::from_bytes([1; 16]).encode())
        .execute(&mut *tx)
        .await
        .expect("control");
    sqlx::query(
        "INSERT INTO main.clients VALUES('synthetic-client',X'02','synthetic','revoked',7,1,2)",
    )
    .execute(&mut *tx)
    .await
    .expect("revoked identity");
    sqlx::query("INSERT INTO main.harnesses VALUES('synthetic-harness','synthetic','never-started','[]',NULL)")
        .execute(&mut *tx).await.expect("fixture legacy harness metadata");
    sqlx::query("INSERT INTO main.workspaces VALUES('synthetic-workspace','synthetic-root')")
        .execute(&mut *tx)
        .await
        .expect("fixture workspace metadata");
    sqlx::query("INSERT INTO main.sessions VALUES('synthetic-session','synthetic-harness','synthetic-workspace','released',NULL,NULL,NULL)")
        .execute(&mut *tx).await.expect("fixture legacy session");
    sqlx::query("INSERT INTO main.content_objects VALUES('synthetic-content','synthetic-session','synthetic-client',7,'input',X'616263',?,3)")
        .bind("0".repeat(64)).execute(&mut *tx).await.expect("legacy body canary");
    sqlx::query("PRAGMA main.user_version=7")
        .execute(&mut *tx)
        .await
        .expect("version7");
    tx.commit().await.expect("fixture7 committed");
    pool
}

async fn old_definitions(pool: &SqlitePool) -> Vec<(String, Option<String>)> {
    sqlx::query_as("SELECT name,sql FROM main.sqlite_schema WHERE tbl_name NOT GLOB 'node_approval_*' ORDER BY name")
        .fetch_all(pool).await.expect("old definitions snapshot")
}

async fn heads(pool: &SqlitePool) -> (i64, i64) {
    let version = sqlx::query_scalar("PRAGMA main.user_version")
        .fetch_one(pool)
        .await
        .expect("pragma");
    let metadata = sqlx::query_scalar("SELECT schema_version FROM main.node_meta")
        .fetch_one(pool)
        .await
        .expect("metadata");
    (version, metadata)
}

async fn old_values_preserved(pool: &SqlitePool) {
    let row = sqlx::query(
        "SELECT status,auth_generation FROM main.clients WHERE client_id='synthetic-client'",
    )
    .fetch_one(pool)
    .await
    .expect("original client");
    assert_eq!(row.get::<String, _>("status"), "revoked");
    assert_eq!(row.get::<i64, _>("auth_generation"), 7);
    let body: Vec<u8> = sqlx::query_scalar(
        "SELECT body FROM main.content_objects WHERE content_ref='synthetic-content'",
    )
    .fetch_one(pool)
    .await
    .expect("original body");
    assert!(body == b"abc", "legacy body remains original");
}

#[tokio::test]
async fn approval_schema8_fresh_database_and_metadata_empty_reopen_are_valid() {
    let pool = empty_pool().await;
    schema::initialize(&pool)
        .await
        .expect("actual fresh0 through9");
    let version: i64 = sqlx::query_scalar("PRAGMA main.user_version")
        .fetch_one(&pool)
        .await
        .expect("version");
    assert_eq!(version, crate::storage::SCHEMA_VERSION);
    schema::initialize(&pool)
        .await
        .expect("empty identity second open");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM main.node_approval_requests")
        .fetch_one(&pool)
        .await
        .expect("new metadata only");
    assert_eq!(count, 0);
    pool.close().await;
}

#[tokio::test]
async fn approval_schema8_upgrades_exact_seven_without_rewriting_old_definitions_or_values() {
    let pool = exact_seven().await;
    let before = old_definitions(&pool).await;
    schema::initialize(&pool)
        .await
        .expect("actual migration through current");
    assert_eq!(
        heads(&pool).await,
        (
            crate::storage::SCHEMA_VERSION,
            crate::storage::SCHEMA_VERSION
        )
    );
    let after = old_definitions(&pool).await;
    let allowed_added_names = [
        "node_catalog_commands",
        "node_launch_profiles",
        "node_target_launch_selection",
        "sqlite_autoindex_node_catalog_commands_1",
        "sqlite_autoindex_node_launch_profiles_1",
        "sqlite_autoindex_node_launch_profiles_2",
        "sqlite_autoindex_node_target_launch_selection_1",
        "sqlite_autoindex_node_target_launch_selection_2",
        "node_native_release_operations",
        "node_native_release_actor_lookup",
        "sqlite_autoindex_node_native_release_operations_1",
        "sqlite_autoindex_node_native_release_operations_2",
        "node_source_session_registrations",
        "node_source_session_operations",
        "node_source_session_runtime_operations",
        "node_source_session_created_owners",
        "sqlite_autoindex_node_source_session_registrations_1",
        "sqlite_autoindex_node_source_session_registrations_2",
        "sqlite_autoindex_node_source_session_operations_1",
        "sqlite_autoindex_node_source_session_operations_2",
        "node_session_capacity",
        "node_session_capacity_native",
        "node_session_capacity_instance",
        "sqlite_autoindex_node_session_capacity_1",
        "node_session_capacity_releases",
        "sqlite_autoindex_node_session_capacity_releases_1",
        "node_session_automatic_releases",
        "node_session_automatic_release_members",
        "node_session_automatic_release_member_lookup",
        "sqlite_autoindex_node_session_automatic_releases_1",
        "sqlite_autoindex_node_session_automatic_release_members_1",
        "node_session_parents",
        "sqlite_autoindex_node_session_parents_1",
    ];
    let retained_old: Vec<_> = after
        .iter()
        .filter(|(name, _)| !allowed_added_names.contains(&name.as_str()))
        .cloned()
        .collect();
    assert_eq!(
        retained_old, before,
        "every original schema7 definition stays exact"
    );
    let added_objects: Vec<(String, String, String, bool)> = sqlx::query_as(
        "SELECT name,type,tbl_name,sql IS NOT NULL FROM main.sqlite_schema WHERE tbl_name IN ('node_catalog_commands','node_launch_profiles','node_target_launch_selection') ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .expect("schema9 additions are checked separately");
    let expected_added_objects = [
        (
            "node_catalog_commands",
            "table",
            "node_catalog_commands",
            true,
        ),
        (
            "node_launch_profiles",
            "table",
            "node_launch_profiles",
            true,
        ),
        (
            "node_target_launch_selection",
            "table",
            "node_target_launch_selection",
            true,
        ),
        (
            "sqlite_autoindex_node_catalog_commands_1",
            "index",
            "node_catalog_commands",
            false,
        ),
        (
            "sqlite_autoindex_node_launch_profiles_1",
            "index",
            "node_launch_profiles",
            false,
        ),
        (
            "sqlite_autoindex_node_launch_profiles_2",
            "index",
            "node_launch_profiles",
            false,
        ),
        (
            "sqlite_autoindex_node_target_launch_selection_1",
            "index",
            "node_target_launch_selection",
            false,
        ),
        (
            "sqlite_autoindex_node_target_launch_selection_2",
            "index",
            "node_target_launch_selection",
            false,
        ),
    ]
    .map(|(name, kind, table, sql)| (name.to_owned(), kind.to_owned(), table.to_owned(), sql));
    assert_eq!(
        added_objects, expected_added_objects,
        "only fixed three tables and their five automatic indexes are added"
    );
    old_values_preserved(&pool).await;
    let count:i64 = sqlx::query_scalar("SELECT count(*) FROM main.sqlite_schema WHERE name GLOB 'node_approval_*' AND sql IS NOT NULL")
        .fetch_one(&pool).await.expect("objects");
    assert_eq!(count, 12);
    assert_eq!(approval_owner_schema::DEFINITIONS.len(), 12);
    schema::initialize(&pool).await.expect("exact second open");
    pool.close().await;
}

#[tokio::test]
async fn approval_schema8_each_ddl_before_after_and_both_heads_fault_rolls_back_exact_seven() {
    let mut faults = Vec::new();
    for ordinal in 1..=12 {
        faults.push(schema::MigrationFault::ApprovalDdlBefore(ordinal));
        faults.push(schema::MigrationFault::ApprovalDdlAfter(ordinal));
    }
    faults.extend([
        schema::MigrationFault::ApprovalMetadata,
        schema::MigrationFault::ApprovalPragma,
    ]);
    for fault in faults {
        let pool = exact_seven().await;
        let before = old_definitions(&pool).await;
        assert!(matches!(
            schema::initialize_for_test(&pool, fault).await,
            Err(StorageError::MigrationInterrupted)
        ));
        assert_eq!(heads(&pool).await, (7, 7));
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM main.sqlite_schema WHERE name GLOB 'node_approval_*'",
        )
        .fetch_one(&pool)
        .await
        .expect("no partial objects");
        assert_eq!(count, 0);
        assert_eq!(old_definitions(&pool).await, before);
        old_values_preserved(&pool).await;
        pool.close().await;
    }
}

#[tokio::test]
async fn approval_schema8_commit_foreign_key_error_is_actual787_and_report_loss_keeps_complete8() {
    let pool = exact_seven().await;
    let error = schema::initialize_for_test(&pool, schema::MigrationFault::DeferredCommit)
        .await
        .expect_err("actual deferred FK");
    let StorageError::Database(sqlx::Error::Database(database)) = error else {
        panic!("wrong error class")
    };
    assert_eq!(database.kind(), sqlx::error::ErrorKind::ForeignKeyViolation);
    assert_eq!(database.code().as_deref(), Some("787"));
    assert_eq!(heads(&pool).await, (7, 7));
    old_values_preserved(&pool).await;
    assert!(matches!(
        schema::initialize_for_test(&pool, schema::MigrationFault::CommitUnknown).await,
        Err(StorageError::MigrationInterrupted)
    ));
    assert_eq!(
        heads(&pool).await,
        (
            crate::storage::SCHEMA_VERSION,
            crate::storage::SCHEMA_VERSION
        )
    );
    schema::initialize(&pool)
        .await
        .expect("committed current schema remains valid");
    old_values_preserved(&pool).await;
    pool.close().await;
}

#[tokio::test]
async fn approval_schema8_required_layout_and_same_connection_temp_shadow_fail_closed() {
    let pool = exact_seven().await;
    schema::initialize(&pool).await.expect("eight");
    {
        let mut connection = pool.acquire().await.expect("same physical connection");
        sqlx::query("CREATE TEMP TABLE node_approval_decisions(fake TEXT)")
            .execute(&mut *connection)
            .await
            .expect("owned shadow fixture");
        assert!(matches!(
            schema::validate_approval_necessary(&mut connection).await,
            Err(StorageError::UnrecognizedSchema)
        ));
        sqlx::query("DROP TABLE temp.node_approval_decisions")
            .execute(&mut *connection)
            .await
            .expect("remove only synthetic shadow");
        schema::validate_approval_necessary(&mut connection)
            .await
            .expect("same source recovered");
    }
    sqlx::query("DROP INDEX main.node_approval_by_source")
        .execute(&pool)
        .await
        .expect("owned required index fault");
    assert!(matches!(
        schema::initialize(&pool).await,
        Err(StorageError::UnrecognizedSchema)
    ));
    old_values_preserved(&pool).await;
    pool.close().await;
}

#[tokio::test]
async fn approval_schema8_optional_audit_layout_is_distinct_from_necessary_approval_integrity() {
    let pool = exact_seven().await;
    schema::initialize(&pool).await.expect("eight");
    sqlx::query("DROP INDEX main.node_approval_audit_expiry")
        .execute(&pool)
        .await
        .expect("owned optional fault");
    schema::initialize(&pool)
        .await
        .expect("only supplemental audit layout is degraded");
    let mut connection = pool.acquire().await.expect("same owner");
    schema::validate_approval_necessary(&mut connection)
        .await
        .expect("necessary layout reliable");
    assert!(matches!(
        schema::validate_approval_audit(&mut connection).await,
        Err(crate::domain::management_audit::AuditStoreError::Layout)
    ));
    drop(connection);
    old_values_preserved(&pool).await;
    pool.close().await;
}

#[tokio::test]
async fn approval_schema8_invalid_ordinal_has_no_effect_and_future_version_is_not_repaired() {
    let pool = exact_seven().await;
    for ordinal in [0, 13] {
        assert!(matches!(
            schema::initialize_for_test(&pool, schema::MigrationFault::ApprovalDdlBefore(ordinal))
                .await,
            Err(StorageError::MigrationInterrupted)
        ));
        assert_eq!(heads(&pool).await, (7, 7));
    }
    schema::initialize(&pool)
        .await
        .expect("current supported schema");
    let first_unknown = crate::storage::SCHEMA_VERSION
        .checked_add(1)
        .expect("current schema has a representable next version");
    sqlx::query(&format!("PRAGMA main.user_version={first_unknown}"))
        .execute(&pool)
        .await
        .expect("owned smallest unknown version fixture");
    sqlx::query("UPDATE main.node_meta SET schema_version=?")
        .bind(first_unknown)
        .execute(&pool)
        .await
        .expect("future metadata");
    assert!(matches!(
        schema::initialize(&pool).await,
        Err(StorageError::UnknownSchema(actual)) if actual == first_unknown
    ));
    assert_eq!(heads(&pool).await, (first_unknown, first_unknown));
    old_values_preserved(&pool).await;
    pool.close().await;
}
