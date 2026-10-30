//! Supported structures and additive schema migration.
use crate::error::StorageError;
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use std::path::Path;
pub(super) const CORE: &[&str] = &[
    r"CREATE TABLE IF NOT EXISTS node_meta (
                singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                schema_version INTEGER NOT NULL,
                node_id TEXT NOT NULL,
                public_key BLOB NOT NULL,
                key_file_ref TEXT NOT NULL
            )",
    r"CREATE TABLE IF NOT EXISTS clients (
                client_id TEXT PRIMARY KEY,
                public_key BLOB NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                status TEXT NOT NULL CHECK (status IN ('trusted', 'revoked')),
                auth_generation INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                revoked_at INTEGER
            )",
    r"CREATE TABLE IF NOT EXISTS management_commands (
                actor_id TEXT NOT NULL,
                request_id TEXT NOT NULL,
                action TEXT NOT NULL,
                payload_digest TEXT NOT NULL,
                result_json TEXT NOT NULL,
                expires_at INTEGER NOT NULL,
                PRIMARY KEY (actor_id, request_id)
            )",
    r"CREATE TABLE IF NOT EXISTS security_audit (
                audit_id INTEGER PRIMARY KEY AUTOINCREMENT,
                actor_id TEXT,
                action TEXT NOT NULL,
                target_id TEXT,
                outcome TEXT NOT NULL,
                occurred_at INTEGER NOT NULL
            )",
    r"CREATE TABLE IF NOT EXISTS node_state (
                singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                revision INTEGER NOT NULL
            )",
];
const SESSION: &[&str] = &[
    r"CREATE TABLE harnesses(harness_id TEXT PRIMARY KEY, name TEXT NOT NULL, executable TEXT NOT NULL, arguments_json TEXT NOT NULL, config_ref TEXT)",
    r"CREATE TABLE workspaces(workspace_id TEXT PRIMARY KEY, canonical_path TEXT NOT NULL)",
    r"CREATE TABLE sessions(session_id TEXT PRIMARY KEY, harness_id TEXT NOT NULL REFERENCES harnesses(harness_id), workspace_id TEXT NOT NULL REFERENCES workspaces(workspace_id), state TEXT NOT NULL CHECK(state IN ('loaded','released','releasing','interrupted')), native_scope TEXT, native_session_id TEXT, recovery_json TEXT)",
    r"CREATE TABLE content_objects(content_ref TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(session_id), actor_id TEXT NOT NULL REFERENCES clients(client_id), auth_generation INTEGER NOT NULL CHECK(auth_generation >= 0), purpose TEXT NOT NULL CHECK(purpose IN ('input','output')), body BLOB NOT NULL, sha256 TEXT NOT NULL CHECK(length(sha256) = 64), total_bytes INTEGER NOT NULL CHECK(total_bytes >= 0 AND total_bytes <= 16777216 AND length(body) = total_bytes))",
    r"CREATE TABLE session_commands(command_id TEXT PRIMARY KEY, actor_id TEXT NOT NULL REFERENCES clients(client_id), request_id TEXT NOT NULL, action TEXT NOT NULL, payload_digest TEXT NOT NULL, session_id TEXT NOT NULL REFERENCES sessions(session_id), status TEXT NOT NULL CHECK(status IN ('accepted','dispatching','completed','failed','delivery_unknown')), UNIQUE(actor_id, request_id))",
    r"CREATE TABLE turns(turn_id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(session_id), command_id TEXT NOT NULL UNIQUE REFERENCES session_commands(command_id), ordinal INTEGER NOT NULL CHECK(ordinal >= 0 AND ordinal <= 9007199254740991), UNIQUE(session_id, ordinal))",
    r"CREATE TABLE runs(run_id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(session_id), command_id TEXT NOT NULL UNIQUE REFERENCES session_commands(command_id), turn_id TEXT NOT NULL UNIQUE REFERENCES turns(turn_id), status TEXT NOT NULL CHECK(status IN ('accepted','dispatching','running','cancelling','completed','cancelled','failed','interrupted','unknown')))",
    r"CREATE TABLE conversation_items(item_id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(session_id), turn_id TEXT NOT NULL REFERENCES turns(turn_id), source_id TEXT NOT NULL, first_position INTEGER NOT NULL CHECK(first_position >= 0 AND first_position <= 9007199254740991), kind TEXT NOT NULL, protocol TEXT NOT NULL DEFAULT 'platform', protocol_version TEXT NOT NULL DEFAULT '1', native_tool_id TEXT, native_tool_digest TEXT, tool_instance_id TEXT, tool_generation INTEGER, tool_native_session_id TEXT, tool_native_session_digest TEXT, content_ref TEXT NOT NULL REFERENCES content_objects(content_ref), content_version INTEGER NOT NULL CHECK(content_version >= 1 AND content_version <= 9007199254740991), committed_version INTEGER NOT NULL CHECK(committed_version >= 1 AND committed_version <= content_version), UNIQUE(session_id, first_position), UNIQUE(session_id, source_id))",
    r"CREATE TABLE approval_connections(instance_id TEXT PRIMARY KEY, generation INTEGER NOT NULL CHECK(generation >= 0 AND generation <= 9007199254740991), state TEXT NOT NULL CHECK(state IN ('active','closed')))",
    r"CREATE TABLE approval_run_sources(run_id TEXT PRIMARY KEY REFERENCES runs(run_id), session_id TEXT NOT NULL REFERENCES sessions(session_id), turn_id TEXT NOT NULL REFERENCES turns(turn_id), instance_id TEXT NOT NULL REFERENCES approval_connections(instance_id), generation INTEGER NOT NULL CHECK(generation >= 0 AND generation <= 9007199254740991), native_session_id TEXT NOT NULL, state TEXT NOT NULL CHECK(state IN ('active','invalid')), invalid_reason TEXT)",
    r"CREATE TABLE approvals(approval_id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(session_id), run_id TEXT NOT NULL REFERENCES runs(run_id), turn_id TEXT NOT NULL REFERENCES turns(turn_id), instance_id TEXT NOT NULL REFERENCES approval_connections(instance_id), connection_generation INTEGER NOT NULL, native_session_id TEXT NOT NULL, native_request_id TEXT NOT NULL, source_digest TEXT NOT NULL, request_kind TEXT NOT NULL, protocol TEXT NOT NULL, protocol_version TEXT NOT NULL, raw_request BLOB NOT NULL CHECK(length(raw_request) <= 16777216), scope_raw BLOB NOT NULL CHECK(length(scope_raw) <= 16777216), tool_native_id TEXT, tool_item_id TEXT, tool_source_id TEXT, status TEXT NOT NULL CHECK(status IN ('pending','decided','unsupported','invalid')), restriction TEXT, invalid_reason TEXT, decision_command_id TEXT UNIQUE REFERENCES session_commands(command_id), selected_choice_id TEXT REFERENCES approval_options(choice_id), deciding_actor_id TEXT REFERENCES clients(client_id), deciding_generation INTEGER, UNIQUE(instance_id,connection_generation,native_request_id), FOREIGN KEY(session_id,tool_item_id,tool_source_id) REFERENCES conversation_items(session_id,item_id,source_id))",
    r"CREATE TABLE approval_options(choice_id TEXT PRIMARY KEY, approval_id TEXT NOT NULL REFERENCES approvals(approval_id), ordinal INTEGER NOT NULL CHECK(ordinal >= 0), native_id TEXT NOT NULL, name TEXT NOT NULL, kind TEXT NOT NULL, UNIQUE(approval_id,ordinal))",
    r"CREATE TABLE approval_deliveries(approval_id TEXT PRIMARY KEY REFERENCES approvals(approval_id), state TEXT NOT NULL CHECK(state IN ('not_sent','claimed','transport_written','unknown','suppressed')), claim_id TEXT UNIQUE, write_report TEXT CHECK(write_report IN ('write_completed','unknown')), source_current_at_report INTEGER CHECK(source_current_at_report IN (0,1)))",
    r"CREATE TABLE approval_audit_links(audit_id INTEGER PRIMARY KEY REFERENCES security_audit(audit_id), approval_id TEXT NOT NULL REFERENCES approvals(approval_id), command_id TEXT REFERENCES session_commands(command_id), basis TEXT NOT NULL, actor_generation INTEGER, instance_id TEXT NOT NULL, connection_generation INTEGER NOT NULL)",
    r"CREATE TABLE session_state(session_id TEXT PRIMARY KEY REFERENCES sessions(session_id), revision INTEGER NOT NULL CHECK(revision >= 0 AND revision <= 9007199254740991), next_position INTEGER NOT NULL CHECK(next_position >= 0 AND next_position <= 9007199254740991), next_turn INTEGER NOT NULL CHECK(next_turn >= 0 AND next_turn <= 9007199254740991))",
    r"CREATE TABLE history_integrity(session_id TEXT NOT NULL REFERENCES sessions(session_id), run_id TEXT NOT NULL REFERENCES runs(run_id), reason TEXT NOT NULL, first_position INTEGER, last_position INTEGER, dropped_bytes INTEGER NOT NULL CHECK(dropped_bytes >= 0), PRIMARY KEY(session_id,run_id), CHECK((first_position IS NULL AND last_position IS NULL) OR (first_position >= 0 AND last_position >= first_position)))",
    r"CREATE TABLE history_content_pins(pin_id TEXT PRIMARY KEY, content_ref TEXT NOT NULL REFERENCES content_objects(content_ref))",
    r"CREATE UNIQUE INDEX tool_reference_key ON conversation_items(session_id,item_id,source_id)",
    r"CREATE UNIQUE INDEX tool_native_identity ON conversation_items(session_id,turn_id,tool_instance_id,tool_generation,tool_native_session_digest,native_tool_digest) WHERE native_tool_digest IS NOT NULL",
    r"CREATE UNIQUE INDEX one_foreground_run ON runs(session_id) WHERE status IN ('accepted','dispatching','running','cancelling','unknown')",
];
fn normalized(sql: &str) -> String {
    let mut result = String::new();
    let mut quoted = false;
    for character in sql.chars() {
        if character == '\'' {
            quoted = !quoted;
            result.push(character);
        } else if quoted {
            result.push(character);
        } else if !character.is_whitespace() {
            result.push(character.to_ascii_lowercase());
        }
    }
    result.replacen("createtableifnotexists", "createtable", 1)
}

async fn validate_definitions(
    connection: &mut SqliteConnection,
    definitions: &[&str],
) -> Result<(), StorageError> {
    for definition in definitions {
        let words: Vec<_> = definition.split_whitespace().collect();
        let name = if definition.starts_with("CREATE TABLE IF NOT EXISTS") {
            words.get(5)
        } else if definition.starts_with("CREATE UNIQUE INDEX") {
            words.get(3)
        } else {
            words.get(2)
        }
        .and_then(|value| value.split('(').next())
        .ok_or(StorageError::UnrecognizedSchema)?;
        let actual: Option<String> = sqlx::query_scalar(
            "SELECT sql FROM sqlite_schema WHERE name = ? AND type IN ('table','index')",
        )
        .bind(name)
        .fetch_optional(&mut *connection)
        .await?;
        if actual.is_none_or(|value| normalized(&value) != normalized(definition)) {
            return Err(StorageError::UnrecognizedSchema);
        }
    }
    Ok(())
}
async fn version(connection: &mut SqliteConnection) -> Result<i64, StorageError> {
    Ok(sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(connection)
        .await?)
}
async fn validate(connection: &mut SqliteConnection, current: i64) -> Result<(), StorageError> {
    if !(0..=2).contains(&current) {
        return Err(StorageError::UnknownSchema(current));
    }
    if current == 0 {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        )
        .fetch_one(&mut *connection)
        .await?;
        return if count == 0 {
            Ok(())
        } else {
            Err(StorageError::UnrecognizedSchema)
        };
    }
    validate_definitions(connection, CORE).await?;
    let versions: Vec<i64> = sqlx::query_scalar("SELECT schema_version FROM node_meta")
        .fetch_all(&mut *connection)
        .await?;
    if versions.len() > 1 || versions.iter().any(|value| *value != current) {
        return Err(StorageError::UnrecognizedSchema);
    }
    let revisions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_state WHERE singleton=1")
        .fetch_one(&mut *connection)
        .await?;
    if revisions != 1 {
        return Err(StorageError::UnrecognizedSchema);
    }
    if current == 1 && versions.is_empty() {
        let history: i64 = sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM clients)+(SELECT COUNT(*) FROM management_commands)+(SELECT COUNT(*) FROM security_audit)").fetch_one(&mut *connection).await?;
        if history != 0 {
            return Err(StorageError::MissingNodeMeta);
        }
    }
    if current == 2 {
        validate_definitions(connection, SESSION).await?;
    }
    Ok(())
}
/// Read-only validation precedes opening existing files with write/WAL options.
pub async fn preflight(path: &Path) -> Result<(), StorageError> {
    if !path.exists() {
        return Ok(());
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .create_if_missing(false);
    let mut connection = SqliteConnection::connect_with(&options).await?;
    let result = async {
        let current = version(&mut connection).await?;
        validate(&mut connection, current).await
    }
    .await;
    let closed = connection.close().await;
    result?;
    closed?;
    Ok(())
}
pub async fn initialize(pool: &sqlx::SqlitePool) -> Result<(), StorageError> {
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
    let current = version(&mut transaction).await?;
    validate(&mut transaction, current).await?;
    if current == 0 {
        for definition in CORE {
            sqlx::query(definition).execute(&mut *transaction).await?;
        }
        sqlx::query("INSERT INTO node_state(singleton, revision) VALUES(1, 0)")
            .execute(&mut *transaction)
            .await?;
    }
    if current < 2 {
        for definition in SESSION {
            sqlx::query(definition).execute(&mut *transaction).await?;
        }
        sqlx::query("UPDATE node_meta SET schema_version = 2")
            .execute(&mut *transaction)
            .await?;
        sqlx::query("PRAGMA user_version = 2")
            .execute(&mut *transaction)
            .await?;
    }
    validate(&mut transaction, 2).await?;
    transaction.commit().await?;
    Ok(())
}
