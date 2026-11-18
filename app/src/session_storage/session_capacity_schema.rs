//! Schema12 preserves capacity responsibilities without inferring root relationships.
use crate::error::StorageError;
use sqlx::SqliteConnection;

pub(super) const DEFINITIONS: &[&str] = &[
    r"CREATE TABLE node_session_capacity (
  reservation_id TEXT PRIMARY KEY CHECK(length(reservation_id)=22),
  source_id TEXT NOT NULL REFERENCES product_sources(source_id),
  native_id TEXT CHECK(native_id IS NULL OR length(CAST(native_id AS BLOB)) BETWEEN 1 AND 4096),
  instance_id TEXT NOT NULL REFERENCES node_native_source_instances(instance_id),
  runtime_epoch TEXT NOT NULL CHECK(length(runtime_epoch)=22),
  create_operation TEXT REFERENCES node_source_session_operations(operation_id),
  root_verified INTEGER NOT NULL CHECK(root_verified IN (0,1)),
  physically_closed INTEGER NOT NULL CHECK(physically_closed IN (0,1)),
  released INTEGER NOT NULL CHECK(released IN (0,1)),
  CHECK(root_verified=0 OR (native_id IS NOT NULL AND create_operation IS NOT NULL)),
  CHECK(released=0 OR physically_closed=1)
) STRICT",
    r"CREATE INDEX node_session_capacity_native
  ON node_session_capacity(source_id,native_id,released)",
    r"CREATE INDEX node_session_capacity_instance
  ON node_session_capacity(instance_id,runtime_epoch,released)",
    r"CREATE TABLE node_session_capacity_releases (
  operation_id TEXT NOT NULL REFERENCES node_native_release_operations(operation_id),
  reservation_id TEXT NOT NULL REFERENCES node_session_capacity(reservation_id),
  PRIMARY KEY(operation_id,reservation_id)
) STRICT",
];

pub(super) const TABLE: &str = "node_session_capacity";
const OBJECTS: &[&str] = &[
    TABLE,
    "node_session_capacity_native",
    "node_session_capacity_instance",
    "node_session_capacity_releases",
];

pub(super) async fn validate(c: &mut SqliteConnection, current: i64) -> Result<(), StorageError> {
    for object in OBJECTS {
        let shadow: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM temp.sqlite_schema WHERE name=? COLLATE NOCASE)",
        )
        .bind(object)
        .fetch_one(&mut *c)
        .await?;
        if shadow != 0 {
            return Err(StorageError::UnrecognizedSchema);
        }
        if current < 12 {
            let partial: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE name=? COLLATE NOCASE)",
            )
            .bind(object)
            .fetch_one(&mut *c)
            .await?;
            if partial != 0 {
                return Err(StorageError::UnrecognizedSchema);
            }
        }
    }
    if current < 12 {
        return Ok(());
    }
    super::schema::validate_definitions(c, DEFINITIONS).await?;
    let strict: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM main.pragma_table_list WHERE schema='main' AND name IN ('node_session_capacity','node_session_capacity_releases') AND type='table' AND strict=1")
        .fetch_one(&mut *c).await?;
    let extra: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE type='trigger' AND tbl_name COLLATE NOCASE IN ('node_session_capacity','node_session_capacity_releases')) OR EXISTS(SELECT 1 FROM temp.sqlite_schema WHERE type='trigger' AND tbl_name COLLATE NOCASE IN ('node_session_capacity','node_session_capacity_releases')) OR EXISTS(SELECT 1 FROM main.sqlite_schema WHERE type='index' AND tbl_name IN ('node_session_capacity','node_session_capacity_releases') AND sql IS NOT NULL AND name NOT IN ('node_session_capacity_native','node_session_capacity_instance'))")
        .fetch_one(&mut *c).await?;
    let invalid: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_session_capacity c JOIN main.node_native_source_instances s ON s.instance_id=c.instance_id WHERE c.source_id!=s.source_id OR c.runtime_epoch!=s.runtime_epoch) OR EXISTS(SELECT 1 FROM main.node_session_capacity c JOIN main.node_source_session_operations o ON o.operation_id=c.create_operation JOIN main.node_source_session_registrations r ON r.operation_id=o.operation_id WHERE o.action!='create' OR c.source_id!=r.source_id OR c.instance_id!=o.source_instance_id OR c.runtime_epoch!=o.runtime_epoch OR (c.root_verified=1 AND (o.response_kind IS NULL OR o.response_kind!='created' OR o.ack_state IS NULL OR o.ack_state!='confirmed' OR o.native_session_id IS NULL OR o.native_session_id!=c.native_id)))")
        .fetch_one(&mut *c).await?;
    let fk: i64 = sqlx::query_scalar("PRAGMA main.foreign_keys")
        .fetch_one(&mut *c)
        .await?;
    let wrong_release: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_session_capacity_releases r JOIN main.node_session_capacity c ON c.reservation_id=r.reservation_id JOIN main.node_native_release_operations o ON o.operation_id=r.operation_id JOIN main.product_targets t ON t.target_id=o.target_id WHERE c.source_id!=t.source_id OR c.native_id IS NULL OR c.native_id!=t.native_id)")
        .fetch_one(&mut *c).await?;
    if strict != 2
        || extra != 0
        || invalid != 0
        || wrong_release != 0
        || fk != 1
        || !sqlx::query("PRAGMA main.foreign_key_check('node_session_capacity')")
            .fetch_all(&mut *c)
            .await?
            .is_empty()
        || !sqlx::query("PRAGMA main.foreign_key_check('node_session_capacity_releases')")
            .fetch_all(&mut *c)
            .await?
            .is_empty()
    {
        return Err(StorageError::UnrecognizedSchema);
    }
    Ok(())
}

pub(super) async fn recover_legacy(c: &mut SqliteConnection) -> Result<(), StorageError> {
    // Already released bindings are settled. Every other binding keeps its original owner.
    sqlx::query("INSERT INTO main.node_session_capacity(reservation_id,source_id,native_id,instance_id,runtime_epoch,create_operation,root_verified,physically_closed,released) SELECT b.load_epoch,t.source_id,t.native_id,b.instance_id,b.runtime_epoch,NULL,0,0,0 FROM main.node_native_resource_bindings b JOIN main.product_targets t ON t.target_id=b.target_id WHERE b.lifecycle!='released'")
        .execute(&mut *c).await?;
    // A zero write proves no creation; missing/armed/unknown/written transport with an
    // original instance can have created a resource and must survive the upgrade.
    sqlx::query("INSERT INTO main.node_session_capacity(reservation_id,source_id,native_id,instance_id,runtime_epoch,create_operation,root_verified,physically_closed,released) SELECT o.operation_id,r.source_id,CASE WHEN o.response_kind='created' AND length(CAST(o.native_session_id AS BLOB)) BETWEEN 1 AND 4096 THEN o.native_session_id ELSE NULL END,o.source_instance_id,o.runtime_epoch,o.operation_id,0,0,0 FROM main.node_source_session_operations o JOIN main.node_source_session_registrations r ON r.operation_id=o.operation_id WHERE o.action='create' AND o.source_instance_id IS NOT NULL AND (o.transport_state IS NULL OR o.transport_state!='zero') AND NOT (o.operation_state='terminal' AND o.outcome='refused' AND o.response_kind IS NULL AND (o.transport_state IS NULL OR o.transport_state='reserved')) AND (o.response_kind IS NULL OR o.response_kind!='remote_error' OR o.ack_state!='confirmed')")
        .execute(c).await?;
    Ok(())
}

pub(super) async fn require_empty(c: &mut SqliteConnection) -> Result<(), StorageError> {
    let count: i64 = sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM main.node_session_capacity)+(SELECT COUNT(*) FROM main.node_session_capacity_releases)")
        .fetch_one(c)
        .await?;
    if count != 0 {
        return Err(StorageError::MissingNodeMeta);
    }
    Ok(())
}

#[cfg(test)]
mod development_tests {
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
            .expect("owned in-memory SQLite")
    }

    #[tokio::test]
    async fn schema12_each_necessary_boundary_rolls_back_and_retries() {
        for fault in [
            MigrationFault::CapacityDdl,
            MigrationFault::CapacityRecovery,
            MigrationFault::CapacityMetadata,
            MigrationFault::CapacityPragma,
            MigrationFault::Validation,
            MigrationFault::Commit,
            MigrationFault::DeferredCommit,
        ] {
            let pool = pool().await;
            assert!(
                schema::initialize_for_test(&pool, fault).await.is_err(),
                "{fault:?}"
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("PRAGMA user_version")
                    .fetch_one(&pool)
                    .await
                    .expect("head"),
                0
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM main.sqlite_schema WHERE name NOT LIKE 'sqlite_%'"
                )
                .fetch_one(&pool)
                .await
                .expect("objects"),
                0
            );
            schema::initialize(&pool)
                .await
                .expect("retry entire migration");
            schema::initialize(&pool).await.expect("exact revalidation");
            pool.close().await;
        }
    }

    #[tokio::test]
    async fn schema12_rejects_unknown_objects_triggers_shadows_and_missing_index() {
        for statement in [
            "CREATE TRIGGER capacity_injected AFTER UPDATE ON main.node_session_capacity BEGIN SELECT 1; END",
            "CREATE TEMP TRIGGER capacity_injected AFTER UPDATE ON main.node_session_capacity BEGIN SELECT 1; END",
            "CREATE TEMP VIEW node_session_capacity AS SELECT 1 AS reservation_id",
            "CREATE INDEX capacity_extra ON node_session_capacity(released)",
            "DROP INDEX node_session_capacity_native",
            "ALTER TABLE node_session_capacity ADD COLUMN ignored TEXT",
            "CREATE TRIGGER capacity_release_injected AFTER INSERT ON main.node_session_capacity_releases BEGIN SELECT 1; END",
            "CREATE TEMP TRIGGER capacity_release_injected AFTER INSERT ON main.node_session_capacity_releases BEGIN SELECT 1; END",
            "CREATE TEMP VIEW node_session_capacity_releases AS SELECT 1 AS operation_id",
            "CREATE INDEX capacity_release_extra ON node_session_capacity_releases(reservation_id)",
            "ALTER TABLE node_session_capacity_releases ADD COLUMN ignored TEXT",
        ] {
            let pool = pool().await;
            schema::initialize(&pool).await.expect("fresh layout");
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("owned malformed fixture");
            assert!(schema::initialize(&pool).await.is_err(), "{statement}");
            assert_eq!(
                sqlx::query_scalar::<_, i64>("PRAGMA user_version")
                    .fetch_one(&pool)
                    .await
                    .expect("unchanged head"),
                crate::storage::SCHEMA_VERSION
            );
            pool.close().await;
        }
        for current in [0, 11] {
            for table in ["node_session_capacity", "node_session_capacity_releases"] {
                let pool = pool().await;
                if current == 11 {
                    schema::initialize(&pool).await.expect("new layout");
                    crate::session_storage::automatic_capacity_release_schema::remove_owned_fixture_for_test(&pool).await;
                    sqlx::query("DROP TABLE node_session_capacity_releases")
                        .execute(&pool)
                        .await
                        .expect("owned association downgrade");
                    sqlx::query("DROP TABLE node_session_capacity")
                        .execute(&pool)
                        .await
                        .expect("owned downgrade");
                    sqlx::query("PRAGMA user_version=11")
                        .execute(&pool)
                        .await
                        .expect("old head");
                }
                sqlx::query(&format!("CREATE TABLE {table}(value TEXT)"))
                    .execute(&pool)
                    .await
                    .expect("partial old-version object");
                assert!(schema::initialize(&pool).await.is_err());
                pool.close().await;
            }
        }
    }
}
