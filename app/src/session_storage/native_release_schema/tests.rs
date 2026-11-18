//! Schema 11 checks use an exact schema 9 fixture plus a fresh owned database.
#![allow(
    clippy::too_many_lines,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use crate::session_storage::{native_release_schema, schema};
use crate::storage::SCHEMA_VERSION;
use sqlx::{
    Row,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

const CANARY_SOURCE: &str = "EREREREREREREREREREREQ";

async fn pool() -> sqlx::SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .in_memory(true)
                .foreign_keys(true),
        )
        .await
        .expect("owned database")
}

async fn exact_nine() -> sqlx::SqlitePool {
    let pool = pool().await;
    schema::initialize(&pool).await.expect("fresh migration");
    remove_capacity_fixture(&pool).await;
    for index in (0..super::super::source_sessions_schema::DEFINITIONS.len()).rev() {
        let definition = super::super::source_sessions_schema::DEFINITIONS[index];
        if definition.starts_with("CREATE INDEX") {
            let name = definition.split_whitespace().nth(2).unwrap();
            sqlx::query(&format!("DROP INDEX main.{name}"))
                .execute(&pool)
                .await
                .expect("remove schema11 index");
        } else {
            let name = definition
                .split_whitespace()
                .nth(2)
                .unwrap()
                .split('(')
                .next()
                .unwrap();
            sqlx::query(&format!("DROP TABLE main.{name}"))
                .execute(&pool)
                .await
                .expect("remove schema11 table");
        }
    }
    sqlx::query("DROP INDEX main.node_native_release_actor_lookup")
        .execute(&pool)
        .await
        .expect("remove schema10 index");
    sqlx::query("DROP TABLE main.node_native_release_operations")
        .execute(&pool)
        .await
        .expect("remove schema10 table");
    sqlx::query(
        "INSERT INTO main.node_meta(
           singleton,schema_version,node_id,public_key,key_file_ref
         ) VALUES(1,9,'schema9-fixture',X'01','schema9-key')",
    )
    .execute(&pool)
    .await
    .expect("schema9 metadata");
    sqlx::query("INSERT INTO main.product_sources(source_id,name) VALUES(?,'schema9 canary')")
        .bind(CANARY_SOURCE)
        .execute(&pool)
        .await
        .expect("schema9 data");
    sqlx::query("PRAGMA main.user_version=9")
        .execute(&pool)
        .await
        .expect("schema9 pragma");
    pool
}

async fn exact_ten_with_release_fact() -> sqlx::SqlitePool {
    let pool = exact_nine().await;
    schema::initialize(&pool)
        .await
        .expect("schema9 to schema11 seed");
    remove_capacity_fixture(&pool).await;
    for index in (0..super::super::source_sessions_schema::DEFINITIONS.len()).rev() {
        let definition = super::super::source_sessions_schema::DEFINITIONS[index];
        if definition.starts_with("CREATE INDEX") {
            let name = definition.split_whitespace().nth(2).unwrap();
            sqlx::query(&format!("DROP INDEX main.{name}"))
                .execute(&pool)
                .await
                .expect("remove schema11 index");
        } else {
            let name = definition
                .split_whitespace()
                .nth(2)
                .unwrap()
                .split('(')
                .next()
                .unwrap();
            sqlx::query(&format!("DROP TABLE main.{name}"))
                .execute(&pool)
                .await
                .expect("remove schema11 table");
        }
    }
    sqlx::query("PRAGMA main.user_version=10")
        .execute(&pool)
        .await
        .expect("schema10 pragma");
    sqlx::query("UPDATE main.node_meta SET schema_version=10")
        .execute(&pool)
        .await
        .expect("schema10 metadata");
    sqlx::query("INSERT INTO main.clients(client_id,public_key,display_name,status,auth_generation,created_at) VALUES('AAAAAAAAAAAAAAAAAAAAAA',X'01','release actor','trusted',7,0)")
        .execute(&pool)
        .await
        .expect("release actor");
    sqlx::query("INSERT INTO main.product_targets(target_id,source_id,native_id) VALUES('AQEBAQEBAQEBAQEBAQEBAQ','EREREREREREREREREREREQ','native-target')")
        .execute(&pool)
        .await
        .expect("release target");
    sqlx::query("INSERT INTO main.node_native_release_operations(operation_id,request_id,actor_id,actor_auth_generation,target_id,phase,error_code,registered_at_ms,updated_at_ms) VALUES('AgICAgICAgICAgICAgICAg','AwMDAwMDAwMDAwMDAwMDAw','AAAAAAAAAAAAAAAAAAAAAA',7,'AQEBAQEBAQEBAQEBAQEBAQ','released',NULL,10,11)")
        .execute(&pool)
        .await
        .expect("release fact");
    seed_preserved_security_facts(&pool).await;
    pool
}

async fn seed_preserved_security_facts(pool: &sqlx::SqlitePool) {
    for statement in [
        "INSERT INTO main.clients(client_id,public_key,display_name,status,auth_generation,created_at,revoked_at) VALUES('BBBBBBBBBBBBBBBBBBBBBB',X'0203','revoked actor','revoked',9,2,12)",
        "INSERT INTO main.security_audit(actor_id,action,target_id,outcome,occurred_at) VALUES('BBBBBBBBBBBBBBBBBBBBBB','client.revoke','BBBBBBBBBBBBBBBBBBBBBB','revoked',12)",
        "INSERT INTO main.node_native_release_operations(operation_id,request_id,actor_id,actor_auth_generation,target_id,phase,error_code,registered_at_ms,updated_at_ms) VALUES('DDDDDDDDDDDDDDDDDDDDDD','FFFFFFFFFFFFFFFFFFFFFF','BBBBBBBBBBBBBBBBBBBBBB',8,'AQEBAQEBAQEBAQEBAQEBAQ','failed','release_unconfirmed',20,23)",
        "INSERT INTO main.node_native_source_instances(instance_id,source_id,runtime_epoch,connection_epoch,config_identity,config_revision,owner_kind,lifecycle,scope_binding_id,scope_bound_at_ms,created_at_ms,updated_at_ms) VALUES('JJJJJJJJJJJJJJJJJJJJJJ','EREREREREREREREREREREQ','KKKKKKKKKKKKKKKKKKKKKK','LLLLLLLLLLLLLLLLLLLLLL','MMMMMMMMMMMMMMMMMMMMMM',X'0102030405060708','test_owned_stream','reserved','NNNNNNNNNNNNNNNNNNNNNN',3,1,3)",
        "INSERT INTO main.node_approval_requests(approval_id,obligation_id,source_id,runtime_epoch,instance_id,connection_epoch,scope_binding_id,rpc_kind,rpc_digest,native_session_digest,request_digest,protocol_version,observed_sequence,observed_receipt,observed_at_ms,dispatch_id,context_kind) VALUES('OOOOOOOOOOOOOOOOOOOOOO','QQQQQQQQQQQQQQQQQQQQQQ','EREREREREREREREREREREQ','KKKKKKKKKKKKKKKKKKKKKK','JJJJJJJJJJJJJJJJJJJJJJ','LLLLLLLLLLLLLLLLLLLLLL','NNNNNNNNNNNNNNNNNNNNNN','string',zeroblob(32),zeroblob(32),zeroblob(32),2,X'0100000000000000',X'0200000000000000',4,NULL,'session_request')",
        "INSERT INTO main.node_approval_associations(approval_id,target_id,association_kind,load_epoch,owner_id,association_fingerprint,associated_at_ms) VALUES('OOOOOOOOOOOOOOOOOOOOOO','AQEBAQEBAQEBAQEBAQEBAQ','registered_session',NULL,NULL,zeroblob(32),5)",
        "INSERT INTO main.node_approval_commands(registration_id,approval_id,target_id,actor_id,registration_auth_generation,option_digest,intent_fingerprint,runtime_epoch,registered_at_ms,state,operation_id,admission_auth_generation,admitted_at_ms,rejection_reason) VALUES('SSSSSSSSSSSSSSSSSSSSSS','OOOOOOOOOOOOOOOOOOOOOO','AQEBAQEBAQEBAQEBAQEBAQ','BBBBBBBBBBBBBBBBBBBBBB',8,zeroblob(32),zeroblob(32),'KKKKKKKKKKKKKKKKKKKKKK',6,'accepted','UUUUUUUUUUUUUUUUUUUUUU',8,7,NULL)",
        "INSERT INTO main.node_approval_decisions(approval_id,registration_id,operation_id,actor_id,admission_auth_generation,option_digest,known_option_kind,authorization_fingerprint,decision_fingerprint,accepted_at_ms) VALUES('OOOOOOOOOOOOOOOOOOOOOO','SSSSSSSSSSSSSSSSSSSSSS','UUUUUUUUUUUUUUUUUUUUUU','BBBBBBBBBBBBBBBBBBBBBB',8,zeroblob(32),'allow_once',zeroblob(32),zeroblob(32),7)",
        "INSERT INTO main.node_approval_invalidations(invalidation_id,approval_id,reason,evidence_fingerprint,observed_sequence,observed_at_ms,recorded_at_ms) VALUES('VVVVVVVVVVVVVVVVVVVVVV','OOOOOOOOOOOOOOOOOOOOOO','source_closed',zeroblob(32),X'0300000000000000',8,9)",
        "INSERT INTO main.node_approval_audit(record_id,approval_id,basis_fingerprint,event_kind,actor_id,target_id,outcome,occurred_at_ms,recorded_at_ms,retain_until_ms,retention_hold) VALUES('preserved-decision','OOOOOOOOOOOOOOOOOOOOOO',zeroblob(32),'approval.decided','BBBBBBBBBBBBBBBBBBBBBB','AQEBAQEBAQEBAQEBAQEBAQ','accepted',7,9,2592000009,1)",
    ] {
        sqlx::query(statement)
            .execute(pool)
            .await
            .expect("non-empty schema10 security facts");
    }
    assert!(
        sqlx::query("PRAGMA main.foreign_key_check")
            .fetch_all(pool)
            .await
            .expect("valid fixture references")
            .is_empty()
    );
}

/// Exact SQLite literals preserve NULL, BLOB bytes, numbers and text separately.
/// Only the schema version is excluded: all identity and old table data remain.
async fn preserved_facts(pool: &sqlx::SqlitePool) -> Vec<(String, Vec<Vec<String>>)> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM main.sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'node_source_session_%' AND name NOT LIKE 'node_session_capacity%' AND name NOT LIKE 'node_session_automatic_%' AND name!='node_session_parents' ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .expect("old table names");
    let mut snapshot = Vec::new();
    for table in tables {
        let columns: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM pragma_table_info(?) WHERE NOT (?='node_meta' AND name='schema_version') ORDER BY cid",
        )
        .bind(&table)
        .bind(&table)
        .fetch_all(pool)
        .await
        .expect("old table columns");
        let projection = columns
            .iter()
            .map(|name| format!("quote(\"{}\")", name.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(",");
        let rows = sqlx::query(&format!(
            "SELECT {projection} FROM main.\"{}\"",
            table.replace('"', "\"\"")
        ))
        .fetch_all(pool)
        .await
        .expect("full old table snapshot");
        let mut values: Vec<Vec<String>> = rows
            .iter()
            .map(|row| (0..columns.len()).map(|i| row.get(i)).collect())
            .collect();
        values.sort();
        snapshot.push((table, values));
    }
    snapshot
}

const PARENT_HARNESS: &str = "HHHHHHHHHHHHHHHHHHHHHH";
const PARENT_WORKSPACE: &str = "WWWWWWWWWWWWWWWWWWWWWW";
const PARENT_PROFILE: &str = "PPPPPPPPPPPPPPPPPPPPPP";
const PARENT_REQUEST: &str = "CCCCCCCCCCCCCCCCCCCCCC";
const PARENT_OPERATION: &str = "RRRRRRRRRRRRRRRRRRRRRR";
const PARENT_RUNTIME: &str = "TTTTTTTTTTTTTTTTTTTTTT";
const PARENT_INSTANCE: &str = "IIIIIIIIIIIIIIIIIIIIII";
const PARENT_CONNECTION: &str = "EEEEEEEEEEEEEEEEEEEEEE";

async fn source_parent_fixture() -> sqlx::SqlitePool {
    let pool = exact_ten_with_release_fact().await;
    schema::initialize(&pool).await.expect("schema11 fixture");
    sqlx::query(
        "INSERT INTO main.harnesses(harness_id,name,executable,arguments_json,config_ref)
         VALUES(?, 'fixture harness', 'fixture', '[]', NULL)",
    )
    .bind(PARENT_HARNESS)
    .execute(&pool)
    .await
    .expect("harness parent");
    sqlx::query("INSERT INTO main.workspaces(workspace_id,canonical_path) VALUES(?, 'fixture')")
        .bind(PARENT_WORKSPACE)
        .execute(&pool)
        .await
        .expect("workspace parent");
    sqlx::query(
        "INSERT INTO main.node_catalog_commands(
           request_id,actor_id,register_auth_generation,action,state,registered_at_ms,
           intent_cipher,intent_digest
         ) VALUES(?, 'AAAAAAAAAAAAAAAAAAAAAA', 7, 'harness.install', 'registered', 1, X'01', zeroblob(32))",
    )
    .bind(PARENT_REQUEST)
    .execute(&pool)
    .await
    .expect("catalog command parent");
    sqlx::query(
        "INSERT INTO main.node_launch_profiles(
           profile_id,revision,harness_id,source_id,payload_format,protected_payload,
           payload_digest,created_by_request_id,created_at_ms
         ) VALUES(?, X'0101010101010101', ?, ?, 1, X'01', zeroblob(32), ?, 1)",
    )
    .bind(PARENT_PROFILE)
    .bind(PARENT_HARNESS)
    .bind(CANARY_SOURCE)
    .bind(PARENT_REQUEST)
    .execute(&pool)
    .await
    .expect("profile parent");
    sqlx::query(
        "INSERT INTO main.node_source_session_registrations(
           operation_id,action,actor_id,register_auth_generation,source_id,harness_id,
           profile_id,profile_revision,workspace_id,launch_path,cursor,intent_digest,registered_at_ms
         ) VALUES(?, 'discover', 'AAAAAAAAAAAAAAAAAAAAAA', 7, ?, ?, ?, X'0101010101010101', ?,
                  'launch', NULL, zeroblob(32), 1)",
    )
    .bind(PARENT_OPERATION)
    .bind(CANARY_SOURCE)
    .bind(PARENT_HARNESS)
    .bind(PARENT_PROFILE)
    .bind(PARENT_WORKSPACE)
    .execute(&pool)
    .await
    .expect("registration parent fixture");
    pool
}

async fn heads(pool: &sqlx::SqlitePool) -> (i64, i64) {
    let pragma = sqlx::query_scalar("PRAGMA main.user_version")
        .fetch_one(pool)
        .await
        .expect("pragma head");
    let metadata = sqlx::query_scalar("SELECT schema_version FROM main.node_meta")
        .fetch_one(pool)
        .await
        .expect("metadata head");
    (pragma, metadata)
}

async fn complete_layout(pool: &sqlx::SqlitePool) -> Vec<(String, Option<String>)> {
    sqlx::query_as("SELECT name,sql FROM main.sqlite_schema ORDER BY name")
        .fetch_all(pool)
        .await
        .expect("complete schema layout")
}

async fn source_object_count(pool: &sqlx::SqlitePool) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM main.sqlite_schema WHERE name IN ('node_source_session_registrations','node_source_session_operations','node_source_session_runtime_operations','node_source_session_created_owners')",
    )
    .fetch_one(pool)
    .await
    .expect("source object count")
}

async fn old_layout(pool: &sqlx::SqlitePool) -> Vec<(String, Option<String>)> {
    sqlx::query_as(
        "SELECT name,sql FROM main.sqlite_schema \
         WHERE name NOT LIKE 'node_native_release_%' \
           AND name NOT LIKE 'sqlite_autoindex_node_native_release_%' \
           AND name NOT LIKE 'node_source_session_%' \
           AND name NOT LIKE 'sqlite_autoindex_node_source_session_%' \
           AND name NOT LIKE 'node_session_capacity%' AND name NOT LIKE 'node_session_automatic_%' \
           AND name NOT LIKE 'sqlite_autoindex_node_session_capacity%' AND name NOT LIKE 'sqlite_autoindex_node_session_automatic_%' \
           AND name NOT IN ('node_session_parents','sqlite_autoindex_node_session_parents_1') \
         ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .expect("schema9 layout")
}

async fn assert_schema9_intact(pool: &sqlx::SqlitePool, before: &[(String, Option<String>)]) {
    assert_eq!(heads(pool).await, (9, 9));
    assert_eq!(old_layout(pool).await, before);
    let source_objects: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM main.sqlite_schema WHERE name IN ('node_source_session_registrations','node_source_session_operations','node_source_session_runtime_operations','node_source_session_created_owners')",
    )
    .fetch_one(pool)
    .await
    .expect("source object count");
    assert_eq!(source_objects, 0);
    let canary: String =
        sqlx::query_scalar("SELECT name FROM main.product_sources WHERE source_id=?")
            .bind(CANARY_SOURCE)
            .fetch_one(pool)
            .await
            .expect("preserved schema9 data");
    assert_eq!(canary, "schema9 canary");
    let release_objects: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM main.sqlite_schema WHERE name LIKE 'node_native_release_%'",
    )
    .fetch_one(pool)
    .await
    .expect("release objects");
    assert_eq!(release_objects, 0);
    let identity: (String, Vec<u8>, String) = sqlx::query_as(
        "SELECT node_id,public_key,key_file_ref FROM main.node_meta WHERE singleton=1",
    )
    .fetch_one(pool)
    .await
    .expect("schema9 identity");
    assert_eq!(
        identity,
        ("schema9-fixture".into(), vec![1], "schema9-key".into())
    );
}

#[tokio::test]
async fn schema11_upgrades_exact_nine_and_preserves_old_layout_and_data() {
    let pool = exact_nine().await;
    let before = old_layout(&pool).await;
    let facts_before = preserved_facts(&pool).await;
    schema::initialize(&pool)
        .await
        .expect("schema9 through schema10 to schema11");
    assert_eq!(heads(&pool).await, (SCHEMA_VERSION, SCHEMA_VERSION));
    assert_eq!(old_layout(&pool).await, before);
    // Schema10/11 add empty tables; every schema9 table and row is unchanged.
    let after = preserved_facts(&pool).await;
    for fact in &facts_before {
        assert_eq!(after.iter().find(|entry| entry.0 == fact.0), Some(fact));
    }
    let source_objects: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM main.sqlite_schema WHERE name IN ('node_source_session_registrations','node_source_session_operations','node_source_session_runtime_operations','node_source_session_created_owners')",
    )
    .fetch_one(&pool)
    .await
    .expect("source object count");
    assert_eq!(source_objects, 4);
    let canary: String =
        sqlx::query_scalar("SELECT name FROM main.product_sources WHERE source_id=?")
            .bind(CANARY_SOURCE)
            .fetch_one(&pool)
            .await
            .expect("schema9 data after upgrade");
    assert_eq!(canary, "schema9 canary");
    let release_columns: Vec<String> = sqlx::query(
        "SELECT name FROM pragma_table_info('node_native_release_operations') ORDER BY cid",
    )
    .fetch_all(&pool)
    .await
    .expect("release columns")
    .into_iter()
    .map(|row| row.get("name"))
    .collect();
    assert_eq!(
        release_columns,
        [
            "operation_id",
            "request_id",
            "actor_id",
            "actor_auth_generation",
            "target_id",
            "phase",
            "error_code",
            "registered_at_ms",
            "updated_at_ms",
        ]
    );
    assert_eq!(native_release_schema::DEFINITIONS.len(), 2);
    schema::initialize(&pool)
        .await
        .expect("exact schema11 reopen");
    assert_eq!(heads(&pool).await, (SCHEMA_VERSION, SCHEMA_VERSION));
    assert_eq!(preserved_facts(&pool).await, after);
    pool.close().await;
}

#[tokio::test]
async fn schema11_upgrades_exact_ten_and_preserves_release_rows_and_indexes() {
    let pool = exact_ten_with_release_fact().await;
    let facts_before = preserved_facts(&pool).await;
    let layout_before = complete_layout(&pool).await;
    let before: Vec<(String, Option<String>)> = sqlx::query_as("SELECT name,sql FROM main.sqlite_schema WHERE name LIKE 'node_native_release_%' OR name LIKE 'sqlite_autoindex_node_native_release_%' ORDER BY name")
        .fetch_all(&pool).await.expect("release layout before");
    schema::initialize(&pool)
        .await
        .expect("schema10 to schema11");
    assert_eq!(heads(&pool).await, (SCHEMA_VERSION, SCHEMA_VERSION));
    let after: Vec<(String, Option<String>)> = sqlx::query_as("SELECT name,sql FROM main.sqlite_schema WHERE name LIKE 'node_native_release_%' OR name LIKE 'sqlite_autoindex_node_native_release_%' ORDER BY name")
        .fetch_all(&pool).await.expect("release layout after");
    assert_eq!(after, before);
    let old_objects_after: Vec<_> = complete_layout(&pool)
        .await
        .into_iter()
        .filter(|(name, _)| {
            !name.starts_with("node_source_session_")
                && !name.starts_with("sqlite_autoindex_node_source_session_")
                && !name.starts_with("node_session_capacity")
                && !name.starts_with("node_session_automatic_")
                && !name.starts_with("sqlite_autoindex_node_session_capacity")
                && !name.starts_with("sqlite_autoindex_node_session_automatic_")
                && name != "node_session_parents"
                && name != "sqlite_autoindex_node_session_parents_1"
        })
        .collect();
    assert_eq!(old_objects_after, layout_before);
    assert_eq!(source_object_count(&pool).await, 4);
    assert_eq!(preserved_facts(&pool).await, facts_before);
    assert_eq!(native_release_schema::DEFINITIONS.len(), 2);
    pool.close().await;
}

#[tokio::test]
async fn release_precommit_faults_roll_back_to_exact_nine() {
    for fault in [
        schema::MigrationFault::ReleaseDdl,
        schema::MigrationFault::ReleaseMetadata,
        schema::MigrationFault::ReleasePragma,
        schema::MigrationFault::Validation,
        schema::MigrationFault::Commit,
        schema::MigrationFault::DeferredCommit,
    ] {
        let pool = exact_nine().await;
        let before = old_layout(&pool).await;
        assert!(
            schema::initialize_for_test(&pool, fault).await.is_err(),
            "{fault:?}"
        );
        assert_schema9_intact(&pool, &before).await;
        schema::initialize(&pool)
            .await
            .expect("retry schema9 to schema11");
        assert_eq!(
            heads(&pool).await,
            (SCHEMA_VERSION, SCHEMA_VERSION),
            "{fault:?}"
        );
        assert_eq!(old_layout(&pool).await, before, "{fault:?}");
        pool.close().await;
    }
}

#[tokio::test]
async fn release_commit_report_loss_keeps_complete_schema11() {
    let pool = exact_nine().await;
    let before = old_layout(&pool).await;
    let facts_before = preserved_facts(&pool).await;
    assert!(matches!(
        schema::initialize_for_test(&pool, schema::MigrationFault::CommitUnknown).await,
        Err(crate::error::StorageError::MigrationInterrupted)
    ));
    assert_eq!(heads(&pool).await, (SCHEMA_VERSION, SCHEMA_VERSION));
    assert_eq!(old_layout(&pool).await, before);
    let facts_after = preserved_facts(&pool).await;
    for fact in &facts_before {
        assert_eq!(
            facts_after.iter().find(|entry| entry.0 == fact.0),
            Some(fact)
        );
    }
    let canary: String =
        sqlx::query_scalar("SELECT name FROM main.product_sources WHERE source_id=?")
            .bind(CANARY_SOURCE)
            .fetch_one(&pool)
            .await
            .expect("data after committed report loss");
    assert_eq!(canary, "schema9 canary");
    schema::initialize(&pool)
        .await
        .expect("complete schema11 after report loss");
    assert_eq!(heads(&pool).await, (SCHEMA_VERSION, SCHEMA_VERSION));
    assert_eq!(preserved_facts(&pool).await, facts_after);
    pool.close().await;
}

#[tokio::test]
async fn source_ddl_and_head_faults_roll_back_exact_schema10_with_release_fact() {
    for fault in [
        schema::MigrationFault::SourceDdlBefore(1),
        schema::MigrationFault::SourceDdlAfter(1),
        schema::MigrationFault::SourceDdlBefore(2),
        schema::MigrationFault::SourceDdlAfter(2),
        schema::MigrationFault::SourceDdlBefore(3),
        schema::MigrationFault::SourceDdlAfter(3),
        schema::MigrationFault::SourceDdlBefore(4),
        schema::MigrationFault::SourceDdlAfter(4),
        schema::MigrationFault::SourceMetadata,
        schema::MigrationFault::SourcePragma,
        schema::MigrationFault::SourceValidationAfter,
        schema::MigrationFault::Validation,
        schema::MigrationFault::Commit,
    ] {
        let pool = exact_ten_with_release_fact().await;
        let before = complete_layout(&pool).await;
        let facts_before = preserved_facts(&pool).await;
        assert!(
            matches!(
                schema::initialize_for_test(&pool, fault).await,
                Err(crate::error::StorageError::MigrationInterrupted)
            ),
            "{fault:?}"
        );
        assert_eq!(heads(&pool).await, (10, 10), "{fault:?}");
        assert_eq!(complete_layout(&pool).await, before, "{fault:?}");
        assert_eq!(source_object_count(&pool).await, 0, "{fault:?}");
        assert_eq!(preserved_facts(&pool).await, facts_before, "{fault:?}");
        pool.close().await;
    }
}

#[tokio::test]
async fn source_deferred_commit_failure_is_actual_sqlite_foreign_key_787() {
    let pool = exact_ten_with_release_fact().await;
    let facts_before = preserved_facts(&pool).await;
    let layout_before = complete_layout(&pool).await;
    let error = schema::initialize_for_test(&pool, schema::MigrationFault::DeferredCommit)
        .await
        .expect_err("actual deferred FK");
    let crate::error::StorageError::Database(sqlx::Error::Database(database)) = error else {
        panic!("wrong error class")
    };
    assert_eq!(database.kind(), sqlx::error::ErrorKind::ForeignKeyViolation);
    assert_eq!(database.code().as_deref(), Some("787"));
    assert!(database.message().contains("FOREIGN KEY"));
    assert_eq!(heads(&pool).await, (10, 10));
    assert_eq!(source_object_count(&pool).await, 0);
    assert_eq!(complete_layout(&pool).await, layout_before);
    assert_eq!(preserved_facts(&pool).await, facts_before);
    pool.close().await;
}

#[tokio::test]
async fn source_commit_report_loss_keeps_complete_schema11_and_release_fact() {
    let pool = exact_ten_with_release_fact().await;
    let facts_before = preserved_facts(&pool).await;
    assert!(matches!(
        schema::initialize_for_test(&pool, schema::MigrationFault::CommitUnknown).await,
        Err(crate::error::StorageError::MigrationInterrupted)
    ));
    assert_eq!(heads(&pool).await, (SCHEMA_VERSION, SCHEMA_VERSION));
    assert_eq!(source_object_count(&pool).await, 4);
    assert_eq!(preserved_facts(&pool).await, facts_before);
    let layout = complete_layout(&pool).await;
    schema::initialize(&pool)
        .await
        .expect("schema11 report-loss reopen");
    assert_eq!(complete_layout(&pool).await, layout);
    assert_eq!(heads(&pool).await, (SCHEMA_VERSION, SCHEMA_VERSION));
    assert_eq!(preserved_facts(&pool).await, facts_before);
    pool.close().await;
}

#[tokio::test]
async fn source_partial_and_unrecognized_layouts_are_rejected_without_repair() {
    let partial = exact_ten_with_release_fact().await;
    sqlx::query("CREATE TABLE main.node_source_session_registrations(value TEXT)")
        .execute(&partial)
        .await
        .expect("partial source table");
    let before = complete_layout(&partial).await;
    assert!(matches!(
        schema::initialize(&partial).await,
        Err(crate::error::StorageError::UnrecognizedSchema)
    ));
    assert_eq!(heads(&partial).await, (10, 10));
    assert_eq!(complete_layout(&partial).await, before);
    partial.close().await;

    let trigger = exact_ten_with_release_fact().await;
    schema::initialize(&trigger)
        .await
        .expect("schema11 fixture");
    sqlx::query("CREATE TRIGGER main.reject_source_layout AFTER INSERT ON node_source_session_operations BEGIN SELECT 1; END")
        .execute(&trigger)
        .await
        .expect("unexpected source trigger");
    let before = complete_layout(&trigger).await;
    assert!(matches!(
        schema::initialize(&trigger).await,
        Err(crate::error::StorageError::UnrecognizedSchema)
    ));
    assert_eq!(heads(&trigger).await, (SCHEMA_VERSION, SCHEMA_VERSION));
    assert_eq!(complete_layout(&trigger).await, before);
    trigger.close().await;

    let missing_index = exact_ten_with_release_fact().await;
    schema::initialize(&missing_index)
        .await
        .expect("schema11 fixture");
    sqlx::query("DROP INDEX main.node_source_session_created_owners")
        .execute(&missing_index)
        .await
        .expect("remove required source index");
    let before = complete_layout(&missing_index).await;
    assert!(matches!(
        schema::initialize(&missing_index).await,
        Err(crate::error::StorageError::UnrecognizedSchema)
    ));
    assert_eq!(
        heads(&missing_index).await,
        (SCHEMA_VERSION, SCHEMA_VERSION)
    );
    assert_eq!(complete_layout(&missing_index).await, before);
    missing_index.close().await;

    let extra_index = exact_ten_with_release_fact().await;
    schema::initialize(&extra_index)
        .await
        .expect("schema11 fixture");
    sqlx::query("CREATE INDEX main.unexpected_source_operation_index ON node_source_session_operations(updated_at_ms)")
        .execute(&extra_index)
        .await
        .expect("unexpected source index");
    let before = complete_layout(&extra_index).await;
    assert!(matches!(
        schema::initialize(&extra_index).await,
        Err(crate::error::StorageError::UnrecognizedSchema)
    ));
    assert_eq!(heads(&extra_index).await, (SCHEMA_VERSION, SCHEMA_VERSION));
    assert_eq!(complete_layout(&extra_index).await, before);
    extra_index.close().await;

    let shadow = exact_ten_with_release_fact().await;
    schema::initialize(&shadow).await.expect("schema11 fixture");
    sqlx::query("CREATE TEMP TABLE node_source_session_runtime_operations(value TEXT)")
        .execute(&shadow)
        .await
        .expect("temporary source shadow");
    let before = complete_layout(&shadow).await;
    assert!(matches!(
        schema::initialize(&shadow).await,
        Err(crate::error::StorageError::UnrecognizedSchema)
    ));
    assert_eq!(heads(&shadow).await, (SCHEMA_VERSION, SCHEMA_VERSION));
    assert_eq!(complete_layout(&shadow).await, before);
    let shadow_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM temp.sqlite_schema WHERE name='node_source_session_runtime_operations'",
    )
    .fetch_one(&shadow)
    .await
    .expect("temporary shadow remains");
    assert_eq!(shadow_count, 1);
    shadow.close().await;

    let mismatched_head = exact_ten_with_release_fact().await;
    schema::initialize(&mismatched_head)
        .await
        .expect("schema11 fixture");
    sqlx::query("UPDATE main.node_meta SET schema_version=10")
        .execute(&mismatched_head)
        .await
        .expect("mismatched metadata head");
    let before = complete_layout(&mismatched_head).await;
    assert!(matches!(
        schema::initialize(&mismatched_head).await,
        Err(crate::error::StorageError::UnrecognizedSchema)
    ));
    assert_eq!(heads(&mismatched_head).await, (SCHEMA_VERSION, 10));
    assert_eq!(complete_layout(&mismatched_head).await, before);
    mismatched_head.close().await;

    let mismatched_pragma = exact_ten_with_release_fact().await;
    schema::initialize(&mismatched_pragma)
        .await
        .expect("schema11 fixture");
    sqlx::query("PRAGMA main.user_version=10")
        .execute(&mismatched_pragma)
        .await
        .expect("mismatched pragma head");
    let before = complete_layout(&mismatched_pragma).await;
    assert!(matches!(
        schema::initialize(&mismatched_pragma).await,
        Err(crate::error::StorageError::UnrecognizedSchema)
    ));
    assert_eq!(heads(&mismatched_pragma).await, (10, SCHEMA_VERSION));
    assert_eq!(complete_layout(&mismatched_pragma).await, before);
    mismatched_pragma.close().await;

    let extra_column = exact_ten_with_release_fact().await;
    schema::initialize(&extra_column)
        .await
        .expect("schema11 fixture");
    sqlx::query("ALTER TABLE main.node_source_session_registrations ADD COLUMN unexpected TEXT")
        .execute(&extra_column)
        .await
        .expect("unexpected source column");
    let before = complete_layout(&extra_column).await;
    assert!(matches!(
        schema::initialize(&extra_column).await,
        Err(crate::error::StorageError::UnrecognizedSchema)
    ));
    assert_eq!(heads(&extra_column).await, (SCHEMA_VERSION, SCHEMA_VERSION));
    assert_eq!(complete_layout(&extra_column).await, before);
    extra_column.close().await;

    let wrong_foreign_key = exact_ten_with_release_fact().await;
    schema::initialize(&wrong_foreign_key)
        .await
        .expect("schema11 fixture");
    sqlx::query("DROP INDEX main.node_source_session_created_owners")
        .execute(&wrong_foreign_key)
        .await
        .expect("drop source index before table replacement");
    sqlx::query("DROP INDEX main.node_source_session_runtime_operations")
        .execute(&wrong_foreign_key)
        .await
        .expect("drop runtime index before table replacement");
    sqlx::query("DROP TABLE main.node_source_session_operations")
        .execute(&wrong_foreign_key)
        .await
        .expect("drop source operation table");
    let malformed_operations = super::super::source_sessions_schema::DEFINITIONS[1].replace(
        "FOREIGN KEY(operation_id,action) REFERENCES node_source_session_registrations(operation_id,action)",
        "FOREIGN KEY(operation_id,action) REFERENCES node_source_session_registrations(operation_id,operation_id)",
    );
    sqlx::query(&super::super::source_sessions_schema::main_ddl(
        &malformed_operations,
    ))
    .execute(&wrong_foreign_key)
    .await
    .expect("wrong but valid foreign key target");
    let before = complete_layout(&wrong_foreign_key).await;
    assert!(matches!(
        schema::initialize(&wrong_foreign_key).await,
        Err(crate::error::StorageError::UnrecognizedSchema)
    ));
    assert_eq!(
        heads(&wrong_foreign_key).await,
        (SCHEMA_VERSION, SCHEMA_VERSION)
    );
    assert_eq!(complete_layout(&wrong_foreign_key).await, before);
    wrong_foreign_key.close().await;

    let broken_source_row = exact_ten_with_release_fact().await;
    schema::initialize(&broken_source_row)
        .await
        .expect("schema11 fixture");
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&broken_source_row)
        .await
        .expect("disable foreign keys for corrupt row fixture");
    sqlx::query(
        "INSERT INTO main.node_source_session_operations(
           operation_id,action,runtime_epoch,admission_auth_generation,admitted_at_ms,
           operation_state,outcome,updated_at_ms
         ) VALUES('EREREREREREREREREREREQ','discover','FRFRFRFRFRFRFRFRFRFRFQ',0,1,
                  'owned','pending',1)",
    )
    .execute(&broken_source_row)
    .await
    .expect("orphan source operation");
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&broken_source_row)
        .await
        .expect("restore foreign keys");
    assert!(matches!(
        schema::initialize(&broken_source_row).await,
        Err(crate::error::StorageError::UnrecognizedSchema)
    ));
    assert_eq!(
        heads(&broken_source_row).await,
        (SCHEMA_VERSION, SCHEMA_VERSION)
    );
    assert_eq!(source_object_count(&broken_source_row).await, 4);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM main.node_source_session_operations")
            .fetch_one(&broken_source_row)
            .await
            .expect("orphan row retained"),
        1
    );
    broken_source_row.close().await;

    let old_with_source = exact_ten_with_release_fact().await;
    sqlx::query("CREATE TABLE main.node_source_session_registrations(value TEXT)")
        .execute(&old_with_source)
        .await
        .expect("old schema with partial source object");
    let before = complete_layout(&old_with_source).await;
    assert!(matches!(
        schema::initialize(&old_with_source).await,
        Err(crate::error::StorageError::UnrecognizedSchema)
    ));
    assert_eq!(heads(&old_with_source).await, (10, 10));
    assert_eq!(complete_layout(&old_with_source).await, before);
    old_with_source.close().await;

    let high_version = exact_ten_with_release_fact().await;
    schema::initialize(&high_version)
        .await
        .expect("schema11 fixture");
    sqlx::query("PRAGMA main.user_version=99")
        .execute(&high_version)
        .await
        .expect("unknown high version");
    let before = complete_layout(&high_version).await;
    assert!(matches!(
        schema::initialize(&high_version).await,
        Err(crate::error::StorageError::UnknownSchema(99))
    ));
    assert_eq!(heads(&high_version).await, (99, SCHEMA_VERSION));
    assert_eq!(complete_layout(&high_version).await, before);
    high_version.close().await;

    let missing_table = exact_ten_with_release_fact().await;
    schema::initialize(&missing_table)
        .await
        .expect("schema11 fixture");
    sqlx::query("DROP INDEX main.node_source_session_created_owners")
        .execute(&missing_table)
        .await
        .expect("drop partial index before table");
    sqlx::query("DROP INDEX main.node_source_session_runtime_operations")
        .execute(&missing_table)
        .await
        .expect("drop runtime index before table");
    sqlx::query("DROP TABLE main.node_source_session_operations")
        .execute(&missing_table)
        .await
        .expect("remove required source table");
    let before = complete_layout(&missing_table).await;
    assert!(matches!(
        schema::initialize(&missing_table).await,
        Err(crate::error::StorageError::UnrecognizedSchema)
    ));
    assert_eq!(
        heads(&missing_table).await,
        (SCHEMA_VERSION, SCHEMA_VERSION)
    );
    assert_eq!(complete_layout(&missing_table).await, before);
    missing_table.close().await;

    let temp_view = exact_ten_with_release_fact().await;
    schema::initialize(&temp_view)
        .await
        .expect("schema11 fixture");
    sqlx::query("CREATE TEMP VIEW node_source_session_runtime_operations AS SELECT 1 AS value")
        .execute(&temp_view)
        .await
        .expect("source shadow view");
    let before = complete_layout(&temp_view).await;
    assert!(matches!(
        schema::initialize(&temp_view).await,
        Err(crate::error::StorageError::UnrecognizedSchema)
    ));
    assert_eq!(heads(&temp_view).await, (SCHEMA_VERSION, SCHEMA_VERSION));
    assert_eq!(complete_layout(&temp_view).await, before);
    let temp_objects: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM temp.sqlite_schema WHERE name='node_source_session_runtime_operations'",
    )
    .fetch_one(&temp_view)
    .await
    .expect("temp view retained");
    assert_eq!(temp_objects, 1);
    temp_view.close().await;
}

#[tokio::test]
async fn source_schema11_allows_a_valid_empty_identity_database() {
    let pool = pool().await;
    schema::initialize(&pool)
        .await
        .expect("fresh schema11 fixture");
    sqlx::query("DELETE FROM main.node_meta")
        .execute(&pool)
        .await
        .expect("remove identity from otherwise empty database");

    schema::initialize(&pool)
        .await
        .expect("valid empty identity database");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA main.user_version")
            .fetch_one(&pool)
            .await
            .expect("schema head"),
        SCHEMA_VERSION
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM main.node_meta")
            .fetch_one(&pool)
            .await
            .expect("empty identity"),
        0
    );
    assert_eq!(source_object_count(&pool).await, 4);
    pool.close().await;
}

#[tokio::test]
async fn source_schema11_rejects_non_empty_source_tables_without_identity() {
    let registrations = pool().await;
    schema::initialize(&registrations)
        .await
        .expect("fresh schema11 fixture");
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&registrations)
        .await
        .expect("disable foreign keys for isolated fixture");
    sqlx::query(
        "INSERT INTO main.node_source_session_registrations(
           operation_id,action,actor_id,register_auth_generation,source_id,harness_id,
           profile_id,profile_revision,workspace_id,launch_path,cursor,intent_digest,registered_at_ms
         ) VALUES('EREREREREREREREREREREQ','discover','actor',0,'source','harness',
                  'profile',X'0101010101010101','workspace','launch',NULL,
                  X'0101010101010101010101010101010101010101010101010101010101010101',1)",
    )
    .execute(&registrations)
    .await
    .expect("source registration fixture");
    sqlx::query("DELETE FROM main.node_meta")
        .execute(&registrations)
        .await
        .expect("remove identity");
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&registrations)
        .await
        .expect("restore foreign keys");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA main.foreign_keys")
            .fetch_one(&registrations)
            .await
            .expect("foreign keys restored"),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM main.node_meta")
            .fetch_one(&registrations)
            .await
            .expect("identity absent"),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM main.node_source_session_registrations")
            .fetch_one(&registrations)
            .await
            .expect("registration row retained"),
        1
    );
    assert!(matches!(
        schema::initialize(&registrations).await,
        Err(crate::error::StorageError::UnrecognizedSchema)
    ));
    registrations.close().await;

    let operations = pool().await;
    schema::initialize(&operations)
        .await
        .expect("fresh schema11 fixture");
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&operations)
        .await
        .expect("disable foreign keys for isolated fixture");
    sqlx::query(
        "INSERT INTO main.node_source_session_operations(
           operation_id,action,runtime_epoch,admission_auth_generation,admitted_at_ms,
           operation_state,outcome,updated_at_ms
         ) VALUES('EREREREREREREREREREREQ','discover','FRFRFRFRFRFRFRFRFRFRFQ',0,1,
                  'owned','pending',1)",
    )
    .execute(&operations)
    .await
    .expect("source operation fixture");
    sqlx::query("DELETE FROM main.node_meta")
        .execute(&operations)
        .await
        .expect("remove identity");
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&operations)
        .await
        .expect("restore foreign keys");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA main.foreign_keys")
            .fetch_one(&operations)
            .await
            .expect("foreign keys restored"),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM main.node_meta")
            .fetch_one(&operations)
            .await
            .expect("identity absent"),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM main.node_source_session_operations")
            .fetch_one(&operations)
            .await
            .expect("operation row retained"),
        1
    );
    assert!(matches!(
        schema::initialize(&operations).await,
        Err(crate::error::StorageError::UnrecognizedSchema)
    ));
    operations.close().await;
}

#[tokio::test]
async fn source_schema11_rejects_valid_source_rows_without_identity() {
    // An operation row must reference its registration, so only construct
    // non-empty layouts that can retain valid parent references.
    for (keep_registration, keep_operation) in [(true, false), (true, true)] {
        let pool = source_parent_fixture().await;
        sqlx::query("PRAGMA foreign_keys=OFF")
            .execute(&pool)
            .await
            .expect("disable foreign keys for identity fixture");
        if !keep_registration {
            sqlx::query("DELETE FROM main.node_source_session_registrations")
                .execute(&pool)
                .await
                .expect("remove registration row");
        }
        if keep_operation {
            sqlx::query(
                "INSERT INTO main.node_source_session_operations(
                   operation_id,action,runtime_epoch,admission_auth_generation,admitted_at_ms,
                   operation_state,outcome,updated_at_ms
                 ) VALUES(?, 'discover', ?, 7, 2, 'owned', 'pending', 2)",
            )
            .bind(PARENT_OPERATION)
            .bind(PARENT_RUNTIME)
            .execute(&pool)
            .await
            .expect("operation row with valid registration parent");
        }
        sqlx::query("DELETE FROM main.node_meta")
            .execute(&pool)
            .await
            .expect("remove identity");
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&pool)
            .await
            .expect("restore foreign keys");
        assert_eq!(
            sqlx::query_scalar::<_, i64>("PRAGMA main.foreign_keys")
                .fetch_one(&pool)
                .await
                .expect("foreign keys restored"),
            1
        );
        assert!(
            sqlx::query("PRAGMA main.foreign_key_check")
                .fetch_all(&pool)
                .await
                .expect("all remaining parent references are valid")
                .is_empty()
        );

        // `heads` requires a node_meta row. Here the missing identity is the
        // condition under test, so observe the version head and identity count
        // directly instead of calling that helper.
        let before_heads: (i64, i64) = sqlx::query_as(
            "SELECT
               (SELECT user_version FROM pragma_user_version),
               (SELECT COUNT(*) FROM main.node_meta)",
        )
        .fetch_one(&pool)
        .await
        .expect("version head and missing identity before validation");
        let before_layout = complete_layout(&pool).await;
        let before_counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT
               (SELECT COUNT(*) FROM main.node_meta),
               (SELECT COUNT(*) FROM main.node_source_session_registrations),
               (SELECT COUNT(*) FROM main.node_source_session_operations)",
        )
        .fetch_one(&pool)
        .await
        .expect("identity and source rows before validation");
        assert_eq!(before_heads, (SCHEMA_VERSION, 0));
        assert_eq!(before_counts.0, 0);
        assert_eq!(before_counts.1, i64::from(keep_registration));
        assert_eq!(before_counts.2, i64::from(keep_operation));

        assert!(matches!(
            schema::initialize(&pool).await,
            Err(crate::error::StorageError::MissingNodeMeta)
        ));
        let after_heads: (i64, i64) = sqlx::query_as(
            "SELECT
               (SELECT user_version FROM pragma_user_version),
               (SELECT COUNT(*) FROM main.node_meta)",
        )
        .fetch_one(&pool)
        .await
        .expect("version head and missing identity after validation");
        assert_eq!(after_heads, before_heads);
        assert_eq!(complete_layout(&pool).await, before_layout);
        assert_eq!(
            sqlx::query_as::<_, (i64, i64, i64)>(
                "SELECT
                   (SELECT COUNT(*) FROM main.node_meta),
                   (SELECT COUNT(*) FROM main.node_source_session_registrations),
                   (SELECT COUNT(*) FROM main.node_source_session_operations)",
            )
            .fetch_one(&pool)
            .await
            .expect("identity and source rows after validation"),
            before_counts
        );
        pool.close().await;
    }
}

#[tokio::test]
async fn source_schema11_rejects_broken_parent_references_without_repair() {
    for column in [
        "actor_id",
        "source_id",
        "harness_id",
        "workspace_id",
        "profile_id",
    ] {
        let pool = source_parent_fixture().await;
        let invalid_value = format!("missing-{column}");
        sqlx::query("PRAGMA foreign_keys=OFF")
            .execute(&pool)
            .await
            .expect("disable FK for isolated registration reference");
        sqlx::query(&format!(
            "UPDATE main.node_source_session_registrations SET {column}=? WHERE operation_id=?"
        ))
        .bind(&invalid_value)
        .bind(PARENT_OPERATION)
        .execute(&pool)
        .await
        .expect("inject registration parent mismatch");
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&pool)
            .await
            .expect("restore FK after isolated registration reference");
        assert_eq!(
            sqlx::query_scalar::<_, i64>("PRAGMA main.foreign_keys")
                .fetch_one(&pool)
                .await
                .expect("FK setting restored"),
            1
        );
        let before_heads = heads(&pool).await;
        let before_layout = complete_layout(&pool).await;
        assert_eq!(before_heads, (SCHEMA_VERSION, SCHEMA_VERSION));
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM main.node_source_session_registrations WHERE operation_id=?",
            )
            .bind(PARENT_OPERATION)
            .fetch_one(&pool)
            .await
            .expect("broken registration row retained"),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(&format!(
                "SELECT {column} FROM main.node_source_session_registrations WHERE operation_id=?"
            ))
            .bind(PARENT_OPERATION)
            .fetch_one(&pool)
            .await
            .expect("broken registration value retained"),
            invalid_value
        );
        assert!(matches!(
            schema::initialize(&pool).await,
            Err(crate::error::StorageError::UnrecognizedSchema)
        ));
        assert_eq!(heads(&pool).await, before_heads);
        assert_eq!(complete_layout(&pool).await, before_layout);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM main.node_source_session_registrations WHERE operation_id=?",
            )
            .bind(PARENT_OPERATION)
            .fetch_one(&pool)
            .await
            .expect("broken registration row remains after rejection"),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(&format!(
                "SELECT {column} FROM main.node_source_session_registrations WHERE operation_id=?"
            ))
            .bind(PARENT_OPERATION)
            .fetch_one(&pool)
            .await
            .expect("broken registration value remains after rejection"),
            invalid_value
        );
        pool.close().await;
    }

    let pool = source_parent_fixture().await;
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&pool)
        .await
        .expect("disable FK for isolated profile revision reference");
    sqlx::query(
        "UPDATE main.node_source_session_registrations
         SET profile_revision=X'0202020202020202' WHERE operation_id=?",
    )
    .bind(PARENT_OPERATION)
    .execute(&pool)
    .await
    .expect("inject profile revision mismatch");
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&pool)
        .await
        .expect("restore FK after isolated profile revision reference");
    let before_heads = heads(&pool).await;
    let before_layout = complete_layout(&pool).await;
    assert!(matches!(
        schema::initialize(&pool).await,
        Err(crate::error::StorageError::UnrecognizedSchema)
    ));
    assert_eq!(heads(&pool).await, before_heads);
    assert_eq!(complete_layout(&pool).await, before_layout);
    assert_eq!(
        sqlx::query_scalar::<_, Vec<u8>>(
            "SELECT profile_revision FROM main.node_source_session_registrations WHERE operation_id=?",
        )
        .bind(PARENT_OPERATION)
        .fetch_one(&pool)
        .await
        .expect("profile revision remains after rejection"),
        vec![2; 8]
    );
    pool.close().await;

    let pool = source_parent_fixture().await;
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&pool)
        .await
        .expect("disable FK for isolated source-instance reference");
    sqlx::query(
        "INSERT INTO main.node_source_session_operations(
           operation_id,action,runtime_epoch,admission_auth_generation,admitted_at_ms,
           operation_state,outcome,source_instance_id,connection_epoch,protocol_version,updated_at_ms
         ) VALUES(?, 'discover', ?, 7, 2, 'owned', 'pending', ?, ?, 1, 2)",
    )
    .bind(PARENT_OPERATION)
    .bind(PARENT_RUNTIME)
    .bind(PARENT_INSTANCE)
    .bind(PARENT_CONNECTION)
    .execute(&pool)
    .await
    .expect("orphan source-instance operation");
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&pool)
        .await
        .expect("restore FK after isolated source-instance reference");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA main.foreign_keys")
            .fetch_one(&pool)
            .await
            .expect("FK setting restored"),
        1
    );
    let before_heads = heads(&pool).await;
    let before_layout = complete_layout(&pool).await;
    assert_eq!(before_heads, (SCHEMA_VERSION, SCHEMA_VERSION));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM main.node_source_session_operations WHERE operation_id=? AND source_instance_id=? AND connection_epoch=?",
        )
        .bind(PARENT_OPERATION)
        .bind(PARENT_INSTANCE)
        .bind(PARENT_CONNECTION)
        .fetch_one(&pool)
        .await
        .expect("orphan operation retained before rejection"),
        1
    );
    assert!(matches!(
        schema::initialize(&pool).await,
        Err(crate::error::StorageError::UnrecognizedSchema)
    ));
    assert_eq!(heads(&pool).await, before_heads);
    assert_eq!(complete_layout(&pool).await, before_layout);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM main.node_source_session_operations WHERE operation_id=? AND source_instance_id=? AND connection_epoch=?",
        )
        .bind(PARENT_OPERATION)
        .bind(PARENT_INSTANCE)
        .bind(PARENT_CONNECTION)
        .fetch_one(&pool)
        .await
        .expect("orphan operation retained after rejection"),
        1
    );
    pool.close().await;
}

#[tokio::test]
async fn source_schema11_rejects_main_and_temp_source_triggers_and_temp_shadows() {
    for table in [
        "node_source_session_registrations",
        "node_source_session_operations",
    ] {
        for schema_name in ["main", "temp"] {
            let pool = exact_ten_with_release_fact().await;
            schema::initialize(&pool).await.expect("schema11 fixture");
            let trigger = if schema_name == "main" {
                format!(
                    "CREATE TRIGGER main.unexpected_source_trigger_{table} \
                     AFTER INSERT ON {table} BEGIN SELECT 1; END"
                )
            } else {
                format!(
                    "CREATE TEMP TRIGGER unexpected_source_trigger_{table} \
                     AFTER INSERT ON main.{table} BEGIN SELECT 1; END"
                )
            };
            sqlx::query(&trigger)
                .execute(&pool)
                .await
                .expect("unexpected Source trigger");
            let before = complete_layout(&pool).await;
            assert!(matches!(
                schema::initialize(&pool).await,
                Err(crate::error::StorageError::UnrecognizedSchema)
            ));
            assert_eq!(heads(&pool).await, (SCHEMA_VERSION, SCHEMA_VERSION));
            assert_eq!(complete_layout(&pool).await, before);
            pool.close().await;
        }
    }

    for (name, object) in [
        (
            "node_source_session_registrations",
            "TABLE node_source_session_registrations(value TEXT)",
        ),
        (
            "node_source_session_operations",
            "VIEW node_source_session_operations AS SELECT 1 AS value",
        ),
        (
            "node_source_session_runtime_operations",
            "TABLE node_source_session_runtime_operations(value TEXT)",
        ),
        (
            "node_source_session_created_owners",
            "VIEW node_source_session_created_owners AS SELECT 1 AS value",
        ),
    ] {
        let pool = exact_ten_with_release_fact().await;
        schema::initialize(&pool).await.expect("schema11 fixture");
        sqlx::query(&format!("CREATE TEMP {object}"))
            .execute(&pool)
            .await
            .expect("temporary Source object shadow");
        let before = complete_layout(&pool).await;
        assert!(matches!(
            schema::initialize(&pool).await,
            Err(crate::error::StorageError::UnrecognizedSchema)
        ));
        assert_eq!(heads(&pool).await, (SCHEMA_VERSION, SCHEMA_VERSION));
        assert_eq!(complete_layout(&pool).await, before);
        let retained: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM temp.sqlite_schema WHERE name=? COLLATE NOCASE",
        )
        .bind(name)
        .fetch_one(&pool)
        .await
        .expect("temporary shadow remains");
        assert_eq!(retained, 1);
        pool.close().await;
    }
}

#[tokio::test]
async fn source_schema11_rejects_each_registration_parent_foreign_key_shape() {
    let registration_definition = super::super::source_sessions_schema::DEFINITIONS[0];
    let operation_definition = super::super::source_sessions_schema::DEFINITIONS[1];
    let invalid_definitions = [
        registration_definition.replace(
            "REFERENCES clients(client_id)",
            "REFERENCES clients(public_key)",
        ),
        registration_definition.replace(
            "REFERENCES product_sources(source_id)",
            "REFERENCES product_sources(name)",
        ),
        registration_definition.replace(
            "REFERENCES harnesses(harness_id)",
            "REFERENCES harnesses(name)",
        ),
        registration_definition.replace(
            "FOREIGN KEY(profile_id,profile_revision) REFERENCES node_launch_profiles(profile_id,revision)",
            "FOREIGN KEY(profile_revision,profile_id) REFERENCES node_launch_profiles(revision,profile_id)",
        ),
        registration_definition.replace(
            "REFERENCES workspaces(workspace_id)",
            "REFERENCES workspaces(canonical_path)",
        ),
    ];
    for malformed_registration in invalid_definitions {
        let pool = exact_ten_with_release_fact().await;
        schema::initialize(&pool).await.expect("schema11 fixture");
        sqlx::query("DROP INDEX main.node_source_session_created_owners")
            .execute(&pool)
            .await
            .expect("drop dependent Source index");
        sqlx::query("DROP INDEX main.node_source_session_runtime_operations")
            .execute(&pool)
            .await
            .expect("drop dependent runtime index");
        sqlx::query("DROP TABLE main.node_source_session_operations")
            .execute(&pool)
            .await
            .expect("drop dependent Source operation table");
        sqlx::query("DROP TABLE main.node_source_session_registrations")
            .execute(&pool)
            .await
            .expect("drop registration table");
        sqlx::query(&super::super::source_sessions_schema::main_ddl(
            &malformed_registration,
        ))
        .execute(&pool)
        .await
        .expect("malformed parent foreign key remains structurally creatable");
        sqlx::query(&super::super::source_sessions_schema::main_ddl(
            operation_definition,
        ))
        .execute(&pool)
        .await
        .expect("restore Source operation table");
        for definition in &super::super::source_sessions_schema::DEFINITIONS[2..] {
            sqlx::query(&super::super::source_sessions_schema::main_ddl(definition))
                .execute(&pool)
                .await
                .expect("restore Source index");
        }
        let before = complete_layout(&pool).await;
        assert!(matches!(
            schema::initialize(&pool).await,
            Err(crate::error::StorageError::UnrecognizedSchema)
        ));
        assert_eq!(heads(&pool).await, (SCHEMA_VERSION, SCHEMA_VERSION));
        assert_eq!(complete_layout(&pool).await, before);
        pool.close().await;
    }
}

#[tokio::test]
async fn source_schema11_rejects_each_operation_parent_foreign_key_shape() {
    let operation_definition = super::super::source_sessions_schema::DEFINITIONS[1];
    let invalid_definitions = [
        operation_definition.replace(
            "FOREIGN KEY(operation_id,action) REFERENCES node_source_session_registrations(operation_id,action)",
            "FOREIGN KEY(operation_id,action) REFERENCES node_source_session_registrations(operation_id,operation_id)",
        ),
        operation_definition.replace(
            "REFERENCES node_native_source_instances(instance_id,connection_epoch)",
            "REFERENCES node_native_source_instances(instance_id,instance_id)",
        ),
    ];

    for malformed_operation in invalid_definitions {
        let pool = exact_ten_with_release_fact().await;
        schema::initialize(&pool).await.expect("schema11 fixture");
        sqlx::query("DROP INDEX main.node_source_session_created_owners")
            .execute(&pool)
            .await
            .expect("drop dependent Source index");
        sqlx::query("DROP INDEX main.node_source_session_runtime_operations")
            .execute(&pool)
            .await
            .expect("drop runtime Source index");
        sqlx::query("DROP TABLE main.node_source_session_operations")
            .execute(&pool)
            .await
            .expect("drop Source operation table");
        sqlx::query(&super::super::source_sessions_schema::main_ddl(
            &malformed_operation,
        ))
        .execute(&pool)
        .await
        .expect("malformed operation foreign key remains structurally creatable");
        for definition in &super::super::source_sessions_schema::DEFINITIONS[2..] {
            sqlx::query(&super::super::source_sessions_schema::main_ddl(definition))
                .execute(&pool)
                .await
                .expect("restore Source index");
        }

        let before_heads = heads(&pool).await;
        let before_layout = complete_layout(&pool).await;
        assert!(matches!(
            schema::initialize(&pool).await,
            Err(crate::error::StorageError::UnrecognizedSchema)
        ));
        assert_eq!(heads(&pool).await, before_heads);
        assert_eq!(complete_layout(&pool).await, before_layout);
        pool.close().await;
    }
}

async fn remove_capacity_fixture(pool: &sqlx::SqlitePool) {
    // Exclusively the fresh in-memory database created by these tests.
    super::super::automatic_capacity_release_schema::remove_owned_fixture_for_test(pool).await;
    sqlx::query("DROP TABLE main.node_session_capacity_releases")
        .execute(pool)
        .await
        .expect("remove generated schema12 release associations");
    sqlx::query("DROP TABLE main.node_session_capacity")
        .execute(pool)
        .await
        .expect("remove generated schema12 fixture");
}

#[tokio::test]
async fn phase1_capacity_migration_contract() {
    let pool = pool().await;
    schema::initialize(&pool)
        .await
        .expect("actual supported migration");
    let table: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE name='node_session_capacity' AND type='table')")
        .fetch_one(&pool).await.expect("actual migrated layout");
    assert_eq!(
        table, 1,
        "phase1 requires durable capacity responsibilities"
    );
    pool.close().await;
}
