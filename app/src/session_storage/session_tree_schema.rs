use crate::{domain::EntityId, error::StorageError};
use sqlx::{Row, SqliteConnection};
use std::collections::{HashMap, HashSet};

pub(super) const DEFINITIONS: &[&str] = &[r"CREATE TABLE node_session_parents (
  source_id TEXT NOT NULL REFERENCES product_sources(source_id),
  child_native_id TEXT NOT NULL CHECK(length(CAST(child_native_id AS BLOB)) BETWEEN 1 AND 4096),
  parent_native_id TEXT NOT NULL CHECK(length(CAST(parent_native_id AS BLOB)) BETWEEN 1 AND 4096),
  observed_instance TEXT NOT NULL REFERENCES node_native_source_instances(instance_id),
  runtime_epoch TEXT NOT NULL CHECK(length(runtime_epoch)=22),
  connection_epoch TEXT NOT NULL CHECK(length(connection_epoch)=22),
  sequence BLOB NOT NULL CHECK(length(sequence)=8 AND sequence!=zeroblob(8)),
  receipt BLOB NOT NULL CHECK(length(receipt)=8 AND receipt!=zeroblob(8)),
  fingerprint BLOB NOT NULL CHECK(length(fingerprint)=32),
  observed_at_ms INTEGER NOT NULL,
  PRIMARY KEY(source_id,child_native_id),
  CHECK(child_native_id!=parent_native_id)
) STRICT"];

const TABLE: &str = "node_session_parents";

pub(super) async fn validate(
    connection: &mut SqliteConnection,
    current: i64,
) -> Result<(), StorageError> {
    let shadow: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM temp.sqlite_schema WHERE name=? COLLATE NOCASE)",
    )
    .bind(TABLE)
    .fetch_one(&mut *connection)
    .await?;
    let partial: i64 = if current < 14 {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE name=? COLLATE NOCASE)",
        )
        .bind(TABLE)
        .fetch_one(&mut *connection)
        .await?
    } else {
        0
    };
    if shadow != 0 || partial != 0 {
        return Err(StorageError::UnrecognizedSchema);
    }
    if current < 14 {
        return Ok(());
    }
    super::schema::validate_definitions(connection, DEFINITIONS).await?;
    let strict: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM main.pragma_table_list WHERE schema='main' AND name='node_session_parents' AND type='table' AND strict=1")
        .fetch_one(&mut *connection).await?;
    let extra: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE type='trigger' AND tbl_name='node_session_parents' COLLATE NOCASE) OR EXISTS(SELECT 1 FROM temp.sqlite_schema WHERE type='trigger' AND tbl_name='node_session_parents' COLLATE NOCASE) OR EXISTS(SELECT 1 FROM main.sqlite_schema WHERE type='index' AND tbl_name='node_session_parents' COLLATE NOCASE AND name!='sqlite_autoindex_node_session_parents_1')")
        .fetch_one(&mut *connection).await?;
    if strict != 1
        || extra != 0
        || !sqlx::query("PRAGMA main.foreign_key_check('node_session_parents')")
            .fetch_all(&mut *connection)
            .await?
            .is_empty()
    {
        return Err(StorageError::UnrecognizedSchema);
    }
    validate_records(connection).await
}

async fn validate_records(connection: &mut SqliteConnection) -> Result<(), StorageError> {
    let invalid: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_session_parents p JOIN main.node_native_source_instances s ON s.instance_id=p.observed_instance WHERE p.source_id!=s.source_id OR p.runtime_epoch!=s.runtime_epoch OR p.connection_epoch!=s.connection_epoch OR length(CAST(p.child_native_id AS BLOB)) NOT BETWEEN 1 AND 4096 OR length(CAST(p.parent_native_id AS BLOB)) NOT BETWEEN 1 AND 4096 OR length(p.sequence)!=8 OR p.sequence=zeroblob(8) OR length(p.receipt)!=8 OR p.receipt=zeroblob(8) OR length(p.fingerprint)!=32) OR EXISTS(SELECT 1 FROM main.node_session_parents p JOIN main.node_session_capacity c ON c.source_id=p.source_id AND c.native_id=p.child_native_id WHERE c.root_verified=1)")
        .fetch_one(&mut *connection).await?;
    if invalid != 0 {
        return Err(StorageError::UnrecognizedSchema);
    }
    let records = sqlx::query("SELECT source_id,child_native_id,parent_native_id,observed_instance,runtime_epoch,connection_epoch FROM main.node_session_parents")
        .fetch_all(connection).await?;
    let mut parents = HashMap::new();
    for record in records {
        for column in [
            "source_id",
            "observed_instance",
            "runtime_epoch",
            "connection_epoch",
        ] {
            if EntityId::decode(&record.try_get::<String, _>(column)?).is_err() {
                return Err(StorageError::UnrecognizedSchema);
            }
        }
        let source: String = record.try_get("source_id")?;
        let child: String = record.try_get("child_native_id")?;
        let parent: String = record.try_get("parent_native_id")?;
        parents.insert((source.clone(), child), (source, parent));
    }
    let mut verified = HashSet::new();
    for key in parents.keys() {
        let mut seen = HashSet::new();
        let mut current = key;
        while let Some(parent) = parents.get(current) {
            if verified.contains(current) {
                break;
            }
            if !seen.insert(current.clone()) {
                return Err(StorageError::UnrecognizedSchema);
            }
            current = parent;
        }
        verified.extend(seen);
    }
    Ok(())
}

pub(super) async fn require_empty(connection: &mut SqliteConnection) -> Result<(), StorageError> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM main.node_session_parents")
        .fetch_one(connection)
        .await?;
    if count != 0 {
        return Err(StorageError::MissingNodeMeta);
    }
    Ok(())
}

#[cfg(test)]
pub(super) async fn remove_owned_fixture_for_test(pool: &sqlx::SqlitePool) {
    sqlx::query("DROP TABLE main.node_session_parents")
        .execute(pool)
        .await
        .expect("remove this test's generated schema14 table");
}

#[cfg(test)]
mod development_tests;
