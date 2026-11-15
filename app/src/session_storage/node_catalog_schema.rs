//! Additive schema9 catalog layout. Fixed definitions are shared by migration and validation.
use crate::error::StorageError;
use sqlx::{Row, SqliteConnection};

pub(super) const DEFINITIONS: &[&str] = &[
    r"CREATE TABLE node_catalog_commands (
 request_id TEXT PRIMARY KEY CHECK(length(request_id)=22),
 actor_id TEXT NOT NULL REFERENCES clients(client_id),
 register_auth_generation INTEGER NOT NULL CHECK(register_auth_generation>=0),
 action TEXT NOT NULL CHECK(action IN ('harness.install','profile.publish','target.bind')),
 state TEXT NOT NULL CHECK(state IN ('registered','committed','rejected')),
 registered_at_ms INTEGER NOT NULL,
 intent_cipher BLOB,
 intent_digest BLOB NOT NULL CHECK(length(intent_digest)=32),
 admission_auth_generation INTEGER CHECK(admission_auth_generation IS NULL OR admission_auth_generation>=0),
 result_kind TEXT CHECK(result_kind IN ('harness','profile','target','rejected')),
 result_id TEXT CHECK(result_id IS NULL OR length(result_id)=22),
 profile_id TEXT,
 profile_revision BLOB CHECK(profile_revision IS NULL OR length(profile_revision)=8),
 result_at_ms INTEGER,
 FOREIGN KEY(profile_id,profile_revision) REFERENCES node_launch_profiles(profile_id,revision),
 CHECK((profile_id IS NULL)=(profile_revision IS NULL)),
 CHECK((state='registered' AND intent_cipher IS NOT NULL AND admission_auth_generation IS NULL
        AND result_kind IS NULL AND result_id IS NULL AND profile_id IS NULL AND result_at_ms IS NULL)
    OR (state='committed' AND intent_cipher IS NULL AND admission_auth_generation IS NOT NULL
        AND result_kind IS NOT NULL AND result_kind IN ('harness','profile','target') AND result_id IS NOT NULL AND profile_id IS NOT NULL AND result_at_ms IS NOT NULL)
    OR (state='rejected' AND intent_cipher IS NULL AND admission_auth_generation IS NOT NULL
        AND result_kind IS NOT NULL AND result_kind='rejected' AND result_id IS NULL AND profile_id IS NULL AND result_at_ms IS NOT NULL))
) STRICT",
    r"CREATE TABLE node_launch_profiles (
 profile_id TEXT NOT NULL CHECK(length(profile_id)=22),
 revision BLOB NOT NULL CHECK(length(revision)=8 AND revision!=X'0000000000000000'),
 harness_id TEXT NOT NULL REFERENCES harnesses(harness_id),
 source_id TEXT NOT NULL REFERENCES product_sources(source_id),
 payload_format INTEGER NOT NULL CHECK(payload_format=1),
 protected_payload BLOB NOT NULL CHECK(length(protected_payload)>0),
 payload_digest BLOB NOT NULL CHECK(length(payload_digest)=32),
 created_by_request_id TEXT NOT NULL UNIQUE REFERENCES node_catalog_commands(request_id),
 created_at_ms INTEGER NOT NULL,
 PRIMARY KEY(profile_id,revision)
) STRICT",
    r"CREATE TABLE node_target_launch_selection (
 target_id TEXT PRIMARY KEY REFERENCES product_targets(target_id),
 profile_id TEXT NOT NULL,
 profile_revision BLOB NOT NULL CHECK(length(profile_revision)=8),
 workspace_id TEXT NOT NULL REFERENCES workspaces(workspace_id),
 launch_path TEXT NOT NULL,
 observed_canonical_path TEXT NOT NULL,
 selected_by_request_id TEXT NOT NULL UNIQUE REFERENCES node_catalog_commands(request_id),
 selected_at_ms INTEGER NOT NULL,
 FOREIGN KEY(profile_id,profile_revision) REFERENCES node_launch_profiles(profile_id,revision)
) STRICT",
];

pub(super) fn main_ddl(definition: &str) -> String {
    definition.replacen("CREATE TABLE ", "CREATE TABLE main.", 1)
}

const TABLES: &[&str] = &[
    "node_catalog_commands",
    "node_launch_profiles",
    "node_target_launch_selection",
];
const PARENTS: &[&str] = &[
    "node_meta",
    "clients",
    "product_sources",
    "product_harness_sources",
    "product_targets",
    "harnesses",
    "workspaces",
];

pub(super) async fn validate(
    connection: &mut SqliteConnection,
    current: i64,
) -> Result<(), StorageError> {
    // Check before the fresh-database early return, on the original connection.
    for name in TABLES.iter().chain(PARENTS) {
        let shadow: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM temp.sqlite_schema WHERE name=? COLLATE NOCASE AND type IN ('table','view','trigger')) OR EXISTS(SELECT 1 FROM main.sqlite_schema WHERE type='trigger' AND tbl_name=? COLLATE NOCASE) OR EXISTS(SELECT 1 FROM temp.sqlite_schema WHERE type='trigger' AND tbl_name=? COLLATE NOCASE)",
        ).bind(name).bind(name).bind(name).fetch_one(&mut *connection).await?;
        if shadow != 0 {
            return Err(StorageError::UnrecognizedSchema);
        }
    }
    let pragma_shadow: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE substr(lower(name),1,7)='pragma_') OR EXISTS(SELECT 1 FROM temp.sqlite_schema WHERE substr(lower(name),1,7)='pragma_')",
    ).fetch_one(&mut *connection).await?;
    if pragma_shadow != 0 {
        return Err(StorageError::UnrecognizedSchema);
    }
    if current < 9 {
        for table in TABLES {
            let partial: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE name=? COLLATE NOCASE)",
            )
            .bind(table)
            .fetch_one(&mut *connection)
            .await?;
            if partial != 0 {
                return Err(StorageError::UnrecognizedSchema);
            }
        }
        return Ok(());
    }
    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA main.foreign_keys")
        .fetch_one(&mut *connection)
        .await?;
    if foreign_keys != 1 {
        return Err(StorageError::UnrecognizedSchema);
    }
    super::schema::validate_definitions(connection, DEFINITIONS).await?;
    for (ordinal, table) in TABLES.iter().enumerate() {
        validate_keys(connection, table, ordinal).await?;
    }
    Ok(())
}

pub(super) async fn validate_foreign_keys(
    connection: &mut SqliteConnection,
) -> Result<(), StorageError> {
    for table in TABLES {
        let errors = sqlx::query(&format!("PRAGMA main.foreign_key_check('{table}')"))
            .fetch_all(&mut *connection)
            .await?;
        if !errors.is_empty() {
            return Err(StorageError::UnrecognizedSchema);
        }
    }
    Ok(())
}

pub(super) async fn require_empty(connection: &mut SqliteConnection) -> Result<(), StorageError> {
    for table in TABLES {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM main.{table}"))
            .fetch_one(&mut *connection)
            .await?;
        if count != 0 {
            return Err(StorageError::MissingNodeMeta);
        }
    }
    Ok(())
}

// These ordered keys come from the fixed DDL above. Object names are never supplied by a caller.
const fn expected_indexes(ordinal: usize) -> &'static [(&'static str, &'static [&'static str])] {
    match ordinal {
        0 => &[("pk", &["request_id"])],
        1 => &[
            ("pk", &["profile_id", "revision"]),
            ("u", &["created_by_request_id"]),
        ],
        _ => &[("pk", &["target_id"]), ("u", &["selected_by_request_id"])],
    }
}
const fn expected_foreign_keys(ordinal: usize) -> &'static [&'static str] {
    match ordinal {
        0 => &[
            "clients:0:actor_id:client_id",
            "node_launch_profiles:0:profile_id:profile_id|node_launch_profiles:1:profile_revision:revision",
        ],
        1 => &[
            "harnesses:0:harness_id:harness_id",
            "product_sources:0:source_id:source_id",
            "node_catalog_commands:0:created_by_request_id:request_id",
        ],
        _ => &[
            "product_targets:0:target_id:target_id",
            "workspaces:0:workspace_id:workspace_id",
            "node_catalog_commands:0:selected_by_request_id:request_id",
            "node_launch_profiles:0:profile_id:profile_id|node_launch_profiles:1:profile_revision:revision",
        ],
    }
}
async fn validate_keys(
    connection: &mut SqliteConnection,
    table: &str,
    ordinal: usize,
) -> Result<(), StorageError> {
    validate_indexes(connection, table, expected_indexes(ordinal)).await?;
    let columns = sqlx::query(&format!("PRAGMA main.table_info('{table}')"))
        .fetch_all(&mut *connection)
        .await?;
    let mut primary = Vec::new();
    for column in columns {
        let position: i64 = column.try_get("pk")?;
        if position > 0 {
            primary.push((position, column.try_get::<String, _>("name")?));
        }
    }
    primary.sort();
    let expected_primary = expected_indexes(ordinal)[0].1;
    if primary.len() != expected_primary.len()
        || primary.iter().zip(expected_primary).enumerate().any(
            |(index, ((position, name), expected))| {
                usize::try_from(*position).ok() != Some(index + 1) || name != expected
            },
        )
    {
        return Err(StorageError::UnrecognizedSchema);
    }
    let keys = sqlx::query(&format!("PRAGMA main.foreign_key_list('{table}')"))
        .fetch_all(&mut *connection)
        .await?;
    let mut grouped = std::collections::BTreeMap::<i64, Vec<(i64, String)>>::new();
    for key in keys {
        if key.try_get::<String, _>("on_update")? != "NO ACTION"
            || key.try_get::<String, _>("on_delete")? != "NO ACTION"
            || key.try_get::<String, _>("match")? != "NONE"
        {
            return Err(StorageError::UnrecognizedSchema);
        }
        let sequence: i64 = key.try_get("seq")?;
        let entry = format!(
            "{}:{sequence}:{}:{}",
            key.try_get::<String, _>("table")?,
            key.try_get::<String, _>("from")?,
            key.try_get::<String, _>("to")?
        );
        grouped
            .entry(key.try_get("id")?)
            .or_default()
            .push((sequence, entry));
    }
    let mut actual = Vec::new();
    for mut group in grouped.into_values() {
        group.sort();
        if group
            .iter()
            .enumerate()
            .any(|(index, (sequence, _))| usize::try_from(*sequence).ok() != Some(index))
        {
            return Err(StorageError::UnrecognizedSchema);
        }
        actual.push(
            group
                .into_iter()
                .map(|(_, value)| value)
                .collect::<Vec<_>>()
                .join("|"),
        );
    }
    actual.sort();
    let mut expected = expected_foreign_keys(ordinal).to_vec();
    expected.sort_unstable();
    if actual != expected {
        return Err(StorageError::UnrecognizedSchema);
    }
    Ok(())
}

async fn validate_indexes(
    connection: &mut SqliteConnection,
    table: &str,
    expected: &[(&str, &[&str])],
) -> Result<(), StorageError> {
    let indexes = sqlx::query(&format!("PRAGMA main.index_list('{table}')"))
        .fetch_all(&mut *connection)
        .await?;
    if indexes.len() != expected.len() {
        return Err(StorageError::UnrecognizedSchema);
    }
    let mut remaining = expected.to_vec();
    for index in indexes {
        let name: String = index.try_get("name")?;
        let prefix = format!("sqlite_autoindex_{table}_");
        // The only dynamic PRAGMA identifier is a checked SQLite-generated name.
        if !name.strip_prefix(&prefix).is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        }) || index.try_get::<i64, _>("unique")? != 1
            || index.try_get::<i64, _>("partial")? != 0
        {
            return Err(StorageError::UnrecognizedSchema);
        }
        let origin: String = index.try_get("origin")?;
        let entries = sqlx::query(&format!("PRAGMA main.index_xinfo('{name}')"))
            .fetch_all(&mut *connection)
            .await?;
        let mut columns = Vec::new();
        for entry in entries {
            if entry.try_get::<i64, _>("key")? == 0 {
                continue;
            }
            if entry.try_get::<i64, _>("desc")? != 0
                || entry.try_get::<String, _>("coll")? != "BINARY"
                || entry.try_get::<i64, _>("cid")? < 0
            {
                return Err(StorageError::UnrecognizedSchema);
            }
            columns.push(entry.try_get::<String, _>("name")?);
        }
        let Some(position) = remaining.iter().position(|(kind, names)| {
            *kind == origin && columns.iter().map(String::as_str).eq(names.iter().copied())
        }) else {
            return Err(StorageError::UnrecognizedSchema);
        };
        remaining.remove(position);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
