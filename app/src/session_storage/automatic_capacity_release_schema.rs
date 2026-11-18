//! Schema13 adds system release intents without adopting user actors or targets.
use crate::{domain::EntityId, error::StorageError};
use sqlx::SqliteConnection;

pub(super) const DEFINITIONS: &[&str] = &[
    r"CREATE TABLE node_session_automatic_releases (
  operation_id TEXT PRIMARY KEY CHECK(length(operation_id)=22),
  anchor_reservation TEXT NOT NULL
    REFERENCES node_session_capacity(reservation_id),
  reason TEXT NOT NULL
    CHECK(reason IN ('idle_timeout','capacity_pressure')),
  phase TEXT NOT NULL
    CHECK(phase IN ('pending','finished','withdrawn')),
  outcome TEXT
    CHECK(outcome IN ('requested','failed','unknown','withdrawn')),
  CHECK((phase='withdrawn')=(outcome IS 'withdrawn'))
) STRICT",
    r"CREATE TABLE node_session_automatic_release_members (
  operation_id TEXT NOT NULL
    REFERENCES node_session_automatic_releases(operation_id),
  reservation_id TEXT NOT NULL
    REFERENCES node_session_capacity(reservation_id),
  PRIMARY KEY(operation_id,reservation_id)
) STRICT",
    r"CREATE INDEX node_session_automatic_release_member_lookup
  ON node_session_automatic_release_members(reservation_id,operation_id)",
];

const OBJECTS: &[&str] = &[
    "node_session_automatic_releases",
    "node_session_automatic_release_members",
    "node_session_automatic_release_member_lookup",
];

pub(super) async fn validate(c: &mut SqliteConnection, current: i64) -> Result<(), StorageError> {
    for object in OBJECTS {
        let shadow: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM temp.sqlite_schema WHERE name=? COLLATE NOCASE)",
        )
        .bind(object)
        .fetch_one(&mut *c)
        .await?;
        let partial: i64 = if current < 13 {
            sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE name=? COLLATE NOCASE)",
            )
            .bind(object)
            .fetch_one(&mut *c)
            .await?
        } else {
            0
        };
        if shadow != 0 || partial != 0 {
            return Err(StorageError::UnrecognizedSchema);
        }
    }
    if current < 13 {
        return Ok(());
    }
    super::schema::validate_definitions(c, DEFINITIONS).await?;
    let strict: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM main.pragma_table_list WHERE schema='main' AND name IN ('node_session_automatic_releases','node_session_automatic_release_members') AND type='table' AND strict=1")
        .fetch_one(&mut *c).await?;
    let extra: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE type='trigger' AND tbl_name COLLATE NOCASE IN ('node_session_automatic_releases','node_session_automatic_release_members')) OR EXISTS(SELECT 1 FROM temp.sqlite_schema WHERE type='trigger' AND tbl_name COLLATE NOCASE IN ('node_session_automatic_releases','node_session_automatic_release_members')) OR EXISTS(SELECT 1 FROM main.sqlite_schema WHERE type='index' AND tbl_name IN ('node_session_automatic_releases','node_session_automatic_release_members') AND name NOT IN ('node_session_automatic_release_member_lookup','sqlite_autoindex_node_session_automatic_releases_1','sqlite_autoindex_node_session_automatic_release_members_1'))")
        .fetch_one(&mut *c).await?;
    if strict != 2 || extra != 0 {
        return Err(StorageError::UnrecognizedSchema);
    }
    for table in [
        "node_session_automatic_releases",
        "node_session_automatic_release_members",
    ] {
        if !sqlx::query(&format!("PRAGMA main.foreign_key_check('{table}')"))
            .fetch_all(&mut *c)
            .await?
            .is_empty()
        {
            return Err(StorageError::UnrecognizedSchema);
        }
    }
    validate_records(c).await
}

async fn validate_records(c: &mut SqliteConnection) -> Result<(), StorageError> {
    let invalid: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_session_automatic_releases o JOIN main.node_session_capacity a ON a.reservation_id=o.anchor_reservation WHERE a.native_id IS NULL OR NOT EXISTS(SELECT 1 FROM main.node_session_capacity r WHERE r.source_id=a.source_id AND r.native_id=a.native_id AND r.root_verified=1) OR NOT EXISTS(SELECT 1 FROM main.node_session_automatic_release_members m WHERE m.operation_id=o.operation_id AND m.reservation_id=o.anchor_reservation)) OR EXISTS(SELECT 1 FROM main.node_session_automatic_release_members m JOIN main.node_session_automatic_releases o ON o.operation_id=m.operation_id JOIN main.node_session_capacity a ON a.reservation_id=o.anchor_reservation JOIN main.node_session_capacity c ON c.reservation_id=m.reservation_id WHERE c.source_id!=a.source_id OR c.native_id IS NOT a.native_id OR c.instance_id!=a.instance_id OR c.runtime_epoch!=a.runtime_epoch OR (o.phase='finished' AND (c.physically_closed!=1 OR c.released!=1)))")
        .fetch_one(&mut *c).await?;
    if invalid != 0 {
        return Err(StorageError::UnrecognizedSchema);
    }
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT operation_id FROM main.node_session_automatic_releases")
            .fetch_all(c)
            .await?;
    if ids.iter().any(|value| EntityId::decode(value).is_err()) {
        return Err(StorageError::UnrecognizedSchema);
    }
    Ok(())
}

pub(super) async fn require_empty(c: &mut SqliteConnection) -> Result<(), StorageError> {
    let count: i64 = sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM main.node_session_automatic_releases)+(SELECT COUNT(*) FROM main.node_session_automatic_release_members)")
        .fetch_one(c).await?;
    if count != 0 {
        return Err(StorageError::MissingNodeMeta);
    }
    Ok(())
}

#[cfg(test)]
pub(super) async fn remove_owned_fixture_for_test(pool: &sqlx::SqlitePool) {
    super::session_tree_schema::remove_owned_fixture_for_test(pool).await;
    // Only callers that just initialized their own temporary database may downgrade it.
    sqlx::raw_sql("DROP TABLE main.node_session_automatic_release_members; DROP TABLE main.node_session_automatic_releases;")
        .execute(pool).await.expect("remove this test's generated schema13 objects");
}

#[cfg(test)]
mod development_tests;
