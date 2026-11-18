//! Supported structures and additive schema migration.
use super::approval_owner_schema;
use super::automatic_capacity_release_schema;
use super::management_audit_schema;
use super::native_dispatch_schema;
use super::native_release_schema;
use super::node_catalog_schema;
use super::product_schema;
use super::registration_schema;
use super::session_capacity_schema;
use super::session_tree_schema;
use super::source_sessions_schema;
use crate::domain::EntityId;
use crate::domain::management_audit::AuditStoreError;
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
pub(super) const SESSION: &[&str] = &[
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

fn required_table(definition: &str) -> Option<&str> {
    if !definition.starts_with("CREATE TABLE") {
        return None;
    }
    let position = if definition.starts_with("CREATE TABLE IF NOT EXISTS") {
        5
    } else {
        2
    };
    definition
        .split_whitespace()
        .nth(position)
        .and_then(|name| name.split('(').next())
}
async fn validate_required_triggers(connection: &mut SqliteConnection) -> Result<(), StorageError> {
    for definition in CORE
        .iter()
        .chain(SESSION)
        .chain(product_schema::DEFINITIONS)
        .chain(product_schema::DELIVERY_DEFINITIONS)
        .chain(registration_schema::DEFINITIONS)
        .chain(native_dispatch_schema::DEFINITIONS)
        .chain(native_release_schema::DEFINITIONS)
        .chain(source_sessions_schema::DEFINITIONS)
        .chain(session_capacity_schema::DEFINITIONS)
        .chain(automatic_capacity_release_schema::DEFINITIONS)
        .chain(session_tree_schema::DEFINITIONS)
        .chain(approval_owner_schema::NECESSARY)
    {
        if let Some(table) = required_table(definition) {
            let exists: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE type='trigger' AND tbl_name=? COLLATE NOCASE) OR EXISTS(SELECT 1 FROM temp.sqlite_schema WHERE type='trigger' AND tbl_name=? COLLATE NOCASE)")
                .bind(table).bind(table).fetch_one(&mut *connection).await?;
            if exists != 0 {
                return Err(StorageError::UnrecognizedSchema);
            }
        }
    }
    Ok(())
}

/// Only supplemental layout errors are degradable; engine/required errors are distinct.
pub(super) async fn validate_management_audit(
    connection: &mut SqliteConnection,
) -> Result<(), AuditStoreError> {
    use AuditStoreError::{Layout, NecessaryIntegrity};
    validate_required_triggers(connection)
        .await
        .map_err(|_| NecessaryIntegrity)?;
    let shadow: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM temp.sqlite_schema WHERE type IN ('table','view') AND name COLLATE NOCASE IN ('node_management_audit','node_request_registrations','node_request_operations','clients','product_targets','pragma_foreign_key_list','pragma_index_list')) OR EXISTS(SELECT 1 FROM main.sqlite_schema WHERE name COLLATE NOCASE IN ('pragma_foreign_key_list','pragma_index_list'))")
        .fetch_one(&mut *connection).await.map_err(|_| NecessaryIntegrity)?;
    if shadow != 0 {
        return Err(Layout);
    }
    for (name, kind, expected) in [
        (
            "node_management_audit",
            "table",
            management_audit_schema::TABLE,
        ),
        (
            "node_management_audit_expiry",
            "index",
            management_audit_schema::INDEX,
        ),
    ] {
        let actual: Option<String> = sqlx::query_scalar("SELECT sql FROM main.sqlite_schema WHERE name=? AND type=? AND tbl_name='node_management_audit'")
            .bind(name).bind(kind).fetch_optional(&mut *connection).await.map_err(|_| NecessaryIntegrity)?;
        if actual.is_none_or(|sql| normalized(&sql) != normalized(expected)) {
            return Err(Layout);
        }
    }
    let unknown: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE type='trigger' AND tbl_name='node_management_audit' COLLATE NOCASE) OR EXISTS(SELECT 1 FROM temp.sqlite_schema WHERE type='trigger' AND tbl_name='node_management_audit' COLLATE NOCASE) OR EXISTS(SELECT 1 FROM main.sqlite_schema WHERE type='index' AND tbl_name='node_management_audit' AND name!='node_management_audit_expiry' AND (sql IS NOT NULL OR name NOT GLOB 'sqlite_autoindex_node_management_audit_*'))")
        .fetch_one(&mut *connection).await.map_err(|_| NecessaryIntegrity)?;
    if unknown != 0 {
        return Err(Layout);
    }
    let keys: Vec<(String,String,String,String,String)> = sqlx::query_as("SELECT \"from\",\"table\",\"to\",on_update,on_delete FROM main.pragma_foreign_key_list('node_management_audit','main')")
        .fetch_all(&mut *connection).await.map_err(|_| NecessaryIntegrity)?;
    let expected = [
        ("request_id", "node_request_registrations", "request_id"),
        ("operation_id", "node_request_operations", "operation_id"),
        ("actor_id", "clients", "client_id"),
        ("target_id", "product_targets", "target_id"),
    ];
    if keys.len() != 4
        || expected.iter().any(|(from, table, to)| {
            !keys.iter().any(|key| {
                key.0 == *from
                    && key.1 == *table
                    && key.2 == *to
                    && key.3 == "NO ACTION"
                    && key.4 == "NO ACTION"
            })
        })
    {
        return Err(Layout);
    }
    let main_ref: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.sqlite_schema s JOIN main.pragma_foreign_key_list(s.name,'main') f WHERE s.type='table' AND f.\"table\"='node_management_audit' COLLATE NOCASE)")
        .fetch_one(&mut *connection).await.map_err(|_| NecessaryIntegrity)?;
    let temp_ref: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM temp.sqlite_schema s JOIN main.pragma_foreign_key_list(s.name,'temp') f WHERE s.type='table' AND f.\"table\"='node_management_audit' COLLATE NOCASE)")
        .fetch_one(&mut *connection).await.map_err(|_| NecessaryIntegrity)?;
    if main_ref != 0 || temp_ref != 0 {
        return Err(Layout);
    }
    Ok(())
}

pub(super) async fn validate_definitions(
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
            "SELECT sql FROM main.sqlite_schema WHERE name = ? AND type IN ('table','index')",
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
    if !(0..=crate::storage::SCHEMA_VERSION).contains(&current) {
        return Err(StorageError::UnknownSchema(current));
    }
    node_catalog_schema::validate(connection, current).await?;
    automatic_capacity_release_schema::validate(connection, current).await?;
    session_tree_schema::validate(connection, current).await?;
    if current == 0 {
        session_capacity_schema::validate(connection, current).await?;
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
    validate_required_triggers(connection).await?;
    if current < 7 {
        let partial: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM main.sqlite_schema WHERE name IN ('node_management_audit','node_management_audit_expiry') COLLATE NOCASE")
            .fetch_one(&mut *connection).await?;
        if partial != 0 {
            return Err(StorageError::UnrecognizedSchema);
        }
    }
    validate_registration_layout(connection, current).await?;
    validate_approval_layout(connection, current).await?;
    validate_native_release_layout(connection, current).await?;
    source_sessions_schema::validate(connection, current).await?;
    session_capacity_schema::validate(connection, current).await?;
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
    if versions.is_empty() {
        validate_empty_identity(connection, current).await?;
    }
    if current >= 2 {
        validate_definitions(connection, SESSION).await?;
    }
    validate_native_layout(connection, current).await?;
    validate_product_layout(connection, current).await?;
    if current >= 9 {
        node_catalog_schema::validate_foreign_keys(connection).await?;
    }
    Ok(())
}

async fn validate_empty_identity(
    connection: &mut SqliteConnection,
    current: i64,
) -> Result<(), StorageError> {
    if current >= 9 {
        node_catalog_schema::require_empty(connection).await?;
    }
    if current >= 10 {
        require_empty_identity_tables(connection, native_release_schema::DEFINITIONS).await?;
    }
    if current >= 11 {
        source_sessions_schema::require_empty(connection).await?;
    }
    if current >= 12 {
        session_capacity_schema::require_empty(connection).await?;
    }
    if current >= 13 {
        automatic_capacity_release_schema::require_empty(connection).await?;
    }
    if current >= 14 {
        session_tree_schema::require_empty(connection).await?;
    }
    if current >= 8 {
        validate_approval_audit(connection)
            .await
            .map_err(|_| StorageError::MissingNodeMeta)?;
        for definition in approval_owner_schema::DEFINITIONS {
            if let Some(table) = required_table(definition) {
                let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM main.{table}"))
                    .fetch_one(&mut *connection)
                    .await?;
                if count != 0 {
                    return Err(StorageError::MissingNodeMeta);
                }
            }
        }
    }
    if current >= 7 {
        validate_management_audit(connection)
            .await
            .map_err(|_| StorageError::MissingNodeMeta)?;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM main.node_management_audit")
            .fetch_one(&mut *connection)
            .await?;
        if count != 0 {
            return Err(StorageError::MissingNodeMeta);
        }
    }
    if current >= 5 {
        require_empty_identity_tables(connection, registration_schema::DEFINITIONS).await?;
    }
    if current >= 6 {
        require_empty_identity_tables(connection, native_dispatch_schema::DEFINITIONS).await?;
    }
    let history: i64 = sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM clients)+(SELECT COUNT(*) FROM management_commands)+(SELECT COUNT(*) FROM security_audit)").fetch_one(&mut *connection).await?;
    if history != 0 {
        return Err(StorageError::MissingNodeMeta);
    }
    if current >= 2 {
        require_empty_identity_tables(connection, SESSION).await?;
    }
    if current >= 3 {
        require_empty_identity_tables(connection, product_schema::DEFINITIONS).await?;
        if current >= 4 {
            require_empty_identity_tables(connection, product_schema::DELIVERY_DEFINITIONS).await?;
        }
        let floor: Option<i64> =
            sqlx::query_scalar("SELECT time_floor_ms FROM product_control WHERE singleton=1")
                .fetch_optional(&mut *connection)
                .await?;
        if floor.is_some_and(|value| value != 0) {
            return Err(StorageError::MissingNodeMeta);
        }
    }
    Ok(())
}

async fn validate_registration_layout(
    connection: &mut SqliteConnection,
    current: i64,
) -> Result<(), StorageError> {
    if current < 5 {
        let partial: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name IN ('node_request_registrations','node_request_operations','node_request_target_lookup')")
            .fetch_one(&mut *connection).await?;
        if partial != 0 {
            return Err(StorageError::UnrecognizedSchema);
        }
    }
    if current >= 5 {
        validate_definitions(connection, registration_schema::DEFINITIONS).await?;
    }
    Ok(())
}

async fn validate_approval_layout(
    connection: &mut SqliteConnection,
    current: i64,
) -> Result<(), StorageError> {
    if current < 8 {
        let partial: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM main.sqlite_schema WHERE name GLOB 'node_approval_*'",
        )
        .fetch_one(connection)
        .await?;
        return if partial == 0 {
            Ok(())
        } else {
            Err(StorageError::UnrecognizedSchema)
        };
    }
    validate_approval_necessary(connection).await
}

async fn validate_native_release_layout(
    connection: &mut SqliteConnection,
    current: i64,
) -> Result<(), StorageError> {
    if current < 10 {
        let partial: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM main.sqlite_schema WHERE name LIKE 'node_native_release_%'",
        )
        .fetch_one(&mut *connection)
        .await?;
        return if partial == 0 {
            Ok(())
        } else {
            Err(StorageError::UnrecognizedSchema)
        };
    }
    validate_definitions(connection, native_release_schema::DEFINITIONS).await
}

pub(super) async fn validate_approval_necessary(
    connection: &mut SqliteConnection,
) -> Result<(), StorageError> {
    validate_definitions(connection, approval_owner_schema::NECESSARY).await?;
    validate_required_triggers(connection).await?;
    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&mut *connection)
        .await?;
    if foreign_keys != 1 {
        return Err(StorageError::UnrecognizedSchema);
    }
    for definition in approval_owner_schema::NECESSARY {
        let name = definition
            .split_whitespace()
            .nth(2)
            .and_then(|value| value.split('(').next())
            .ok_or(StorageError::UnrecognizedSchema)?;
        let shadow: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM temp.sqlite_schema WHERE name=? COLLATE NOCASE",
        )
        .bind(name)
        .fetch_one(&mut *connection)
        .await?;
        if shadow != 0 {
            return Err(StorageError::UnrecognizedSchema);
        }
        if definition.starts_with("CREATE TABLE") {
            let extra: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM main.sqlite_schema WHERE type='index' AND tbl_name=? AND sql IS NOT NULL AND name NOT IN ('node_approval_by_source','node_approval_commands_by_actor','node_approval_replies_by_request')")
                .bind(name).fetch_one(&mut *connection).await?;
            if extra != 0 {
                return Err(StorageError::UnrecognizedSchema);
            }
        }
    }
    Ok(())
}

pub(super) async fn validate_approval_audit(
    connection: &mut SqliteConnection,
) -> Result<(), AuditStoreError> {
    use AuditStoreError::{Layout, NecessaryIntegrity};
    validate_approval_necessary(connection)
        .await
        .map_err(|_| NecessaryIntegrity)?;
    let shadow: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM temp.sqlite_schema WHERE name COLLATE NOCASE IN ('node_approval_audit','node_approval_audit_expiry','pragma_foreign_key_list')) OR EXISTS(SELECT 1 FROM main.sqlite_schema WHERE name='pragma_foreign_key_list' COLLATE NOCASE)")
        .fetch_one(&mut *connection).await.map_err(|_| NecessaryIntegrity)?;
    if shadow != 0 {
        return Err(Layout);
    }
    for (name, kind, definition) in [
        (
            "node_approval_audit",
            "table",
            approval_owner_schema::DEFINITIONS[7],
        ),
        (
            "node_approval_audit_expiry",
            "index",
            approval_owner_schema::DEFINITIONS[11],
        ),
    ] {
        let actual: Option<String> = sqlx::query_scalar("SELECT sql FROM main.sqlite_schema WHERE name=? AND type=? AND tbl_name='node_approval_audit'")
            .bind(name).bind(kind).fetch_optional(&mut *connection).await.map_err(|_| NecessaryIntegrity)?;
        if actual.is_none_or(|sql| normalized(&sql) != normalized(definition)) {
            return Err(Layout);
        }
    }
    let extra: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE type='trigger' AND tbl_name='node_approval_audit' COLLATE NOCASE) OR EXISTS(SELECT 1 FROM temp.sqlite_schema WHERE type='trigger' AND tbl_name='node_approval_audit' COLLATE NOCASE) OR EXISTS(SELECT 1 FROM main.sqlite_schema WHERE type='index' AND tbl_name='node_approval_audit' AND name!='node_approval_audit_expiry' AND (sql IS NOT NULL OR name NOT GLOB 'sqlite_autoindex_node_approval_audit_*'))")
        .fetch_one(&mut *connection).await.map_err(|_| NecessaryIntegrity)?;
    if extra != 0 {
        return Err(Layout);
    }
    let keys: Vec<(String, String, String, String, String)> = sqlx::query_as("SELECT \"from\",\"table\",\"to\",on_update,on_delete FROM main.pragma_foreign_key_list('node_approval_audit','main')")
        .fetch_all(&mut *connection).await.map_err(|_| NecessaryIntegrity)?;
    let expected = [
        ("approval_id", "node_approval_requests", "approval_id"),
        ("actor_id", "clients", "client_id"),
        ("target_id", "product_targets", "target_id"),
    ];
    if keys.len() != expected.len()
        || expected.iter().any(|(from, table, to)| {
            !keys.iter().any(|key| {
                key.0 == *from
                    && key.1 == *table
                    && key.2 == *to
                    && key.3 == "NO ACTION"
                    && key.4 == "NO ACTION"
            })
        })
    {
        return Err(Layout);
    }
    let main_ref: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.sqlite_schema s JOIN main.pragma_foreign_key_list(s.name,'main') f WHERE s.type='table' AND f.\"table\"='node_approval_audit' COLLATE NOCASE)")
        .fetch_one(&mut *connection).await.map_err(|_| NecessaryIntegrity)?;
    let temp_ref: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM temp.sqlite_schema s JOIN main.pragma_foreign_key_list(s.name,'temp') f WHERE s.type='table' AND f.\"table\"='node_approval_audit' COLLATE NOCASE)")
        .fetch_one(&mut *connection).await.map_err(|_| NecessaryIntegrity)?;
    if main_ref != 0 || temp_ref != 0 {
        return Err(Layout);
    }
    Ok(())
}
async fn validate_native_layout(
    connection: &mut SqliteConnection,
    current: i64,
) -> Result<(), StorageError> {
    if current >= 6 {
        validate_definitions(connection, native_dispatch_schema::DEFINITIONS).await
    } else {
        let partial: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_schema WHERE name LIKE 'node_native_%'",
        )
        .fetch_one(&mut *connection)
        .await?;
        if partial == 0 {
            Ok(())
        } else {
            Err(StorageError::UnrecognizedSchema)
        }
    }
}
async fn validate_product_layout(
    connection: &mut SqliteConnection,
    current: i64,
) -> Result<(), StorageError> {
    if current >= 3 {
        validate_definitions(connection, product_schema::DEFINITIONS).await?;
        let controls: Vec<(String, i64)> = sqlx::query_as(
            "SELECT request_epoch,time_floor_ms FROM product_control WHERE singleton=1",
        )
        .fetch_all(&mut *connection)
        .await?;
        if controls.len() != 1
            || controls.iter().any(|(epoch, floor)| {
                EntityId::decode(epoch).is_err() || !(0..=9_007_199_254_740_991).contains(floor)
            })
        {
            return Err(StorageError::UnrecognizedSchema);
        }
        if current >= 4 {
            validate_definitions(connection, product_schema::DELIVERY_DEFINITIONS).await?;
        } else {
            let partial: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_schema WHERE name IN ('product_delivery_claim_identity','product_delivery_attempts')",
            )
            .fetch_one(&mut *connection)
            .await?;
            if partial != 0 {
                return Err(StorageError::UnrecognizedSchema);
            }
        }
    } else {
        let partial: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name LIKE 'product_%'")
                .fetch_one(&mut *connection)
                .await?;
        if partial != 0 {
            return Err(StorageError::UnrecognizedSchema);
        }
    }
    Ok(())
}

async fn require_empty_identity_tables(
    connection: &mut SqliteConnection,
    definitions: &[&str],
) -> Result<(), StorageError> {
    for definition in definitions
        .iter()
        .filter(|definition| definition.starts_with("CREATE TABLE"))
    {
        let name = definition
            .split_whitespace()
            .nth(2)
            .and_then(|value| value.split('(').next())
            .ok_or(StorageError::UnrecognizedSchema)?;
        if name == "product_control" {
            continue;
        }
        let exists: i64 =
            sqlx::query_scalar(&format!("SELECT EXISTS(SELECT 1 FROM {name} LIMIT 1)"))
                .fetch_one(&mut *connection)
                .await?;
        if exists != 0 {
            return Err(StorageError::MissingNodeMeta);
        }
    }
    Ok(())
}
/// Read-only validation precedes opening existing files with write/WAL options.
#[cfg(test)]
pub fn preflight(
    path: &Path,
) -> impl std::future::Future<Output = Result<(), StorageError>> + Send + use<> {
    use super::sqlite_lifecycle::{StorageLifecycleHost, StorageOpenOwner, StorageOpenRoots};
    let prepared = (|| {
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| StorageError::Lifecycle("original preflight runtime unavailable"))?;
        let owner = StorageOpenOwner::prepare(&runtime, StorageOpenRoots::path(path))?;
        let host = StorageLifecycleHost::install(owner.clone());
        let ticket = owner.start_preflight()?;
        Ok::<_, StorageError>((host, ticket))
    })();
    async move {
        let (host, ticket) = prepared?;
        let result = ticket.preflight_ready().await;
        if !host.observe_actual().await {
            std::future::pending::<()>().await;
        }
        drop(ticket);
        drop(host);
        result.map_err(StorageError::from)
    }
}
pub async fn preflight_in_scope(
    path: &Path,
    scope: &std::sync::Arc<super::sqlite_lifecycle::SqliteOpenState>,
) -> Result<(), StorageError> {
    if !path.exists() {
        return Ok(());
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .worker_registry(scope.registry().clone())
        .read_only(true)
        .create_if_missing(false);
    let mut connection = SqliteConnection::connect_with(&options).await?;
    let registration = connection
        .worker_registration()
        .ok_or(StorageError::Lifecycle(
            "preflight worker registration missing",
        ))?;
    #[cfg(test)]
    scope
        .hooks
        .visit(super::sqlite_lifecycle::TestPoint::PreflightConnectedBeforeValidation)
        .await;
    let result = async {
        let current = version(&mut connection).await?;
        validate(&mut connection, current).await
    }
    .await;
    let closed = connection.close().await;
    let joined = scope.wait_worker(&registration).await;
    result?;
    closed?;
    if !joined {
        return Err(StorageError::Lifecycle(
            "preflight original worker join unconfirmed",
        ));
    }
    Ok(())
}
pub async fn initialize(pool: &sqlx::SqlitePool) -> Result<(), StorageError> {
    initialize_inner(
        pool,
        #[cfg(test)]
        None,
        #[cfg(test)]
        None,
        #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
        None,
    )
    .await
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MigrationFault {
    CatalogDdlBefore(u8),
    CatalogDdlAfter(u8),
    CatalogMetadata,
    CatalogPragma,
    ReleaseDdl,
    ReleaseMetadata,
    ReleasePragma,
    SourceDdlBefore(u8),
    SourceDdlAfter(u8),
    SourceMetadata,
    SourcePragma,
    SourceValidationAfter,
    CapacityDdl,
    CapacityRecovery,
    CapacityMetadata,
    CapacityPragma,
    AutomaticReleaseDdl(u8),
    AutomaticReleaseMetadata,
    AutomaticReleasePragma,
    SessionTreeDdl,
    SessionTreeMetadata,
    SessionTreePragma,
    ApprovalDdlBefore(u8),
    ApprovalDdlAfter(u8),
    ApprovalMetadata,
    ApprovalPragma,
    AuditTable,
    AuditIndex,
    AuditMetadata,
    AuditPragma,
    Ddl,
    Product3Ready,
    DeliveryIndex,
    DeliveryTable,
    Metadata,
    Pragma,
    Validation,
    Commit,
    DeferredCommit,
    CommitUnknown,
    RegistrationTable,
    OperationTable,
    RegistrationIndex,
    RegistrationMetadata,
    RegistrationPragma,
    NativeDdl,
    NativeDdlAt(usize),
    NativeMetadata,
    NativePragma,
}

#[cfg(test)]
fn checkpoint(fault: Option<MigrationFault>, step: MigrationFault) -> Result<(), StorageError> {
    if fault == Some(step) {
        Err(StorageError::MigrationInterrupted)
    } else {
        Ok(())
    }
}

#[cfg(test)]
pub async fn initialize_for_test(
    pool: &sqlx::SqlitePool,
    fault: MigrationFault,
) -> Result<(), StorageError> {
    initialize_inner(
        pool,
        Some(fault),
        None,
        #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
        None,
    )
    .await
}

#[expect(
    clippy::too_many_lines,
    reason = "All schema steps and both version heads are validated within the same real migration transaction."
)]
async fn initialize_inner(
    pool: &sqlx::SqlitePool,
    #[cfg(test)] fault: Option<MigrationFault>,
    #[cfg(test)] namespace: Option<super::schema9_namespace_test_hook::NamespaceScenario>,
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))] approval_trace: Option<
        std::sync::Arc<super::approval_owner::test_control::ApprovalMigrationTrace>,
    >,
) -> Result<(), StorageError> {
    #[cfg(test)]
    if matches!(
        fault,
        Some(
            MigrationFault::CatalogDdlBefore(0 | 4..=u8::MAX)
                | MigrationFault::CatalogDdlAfter(0 | 4..=u8::MAX)
                | MigrationFault::SourceDdlBefore(0 | 5..=u8::MAX)
                | MigrationFault::SourceDdlAfter(0 | 5..=u8::MAX)
                | MigrationFault::ApprovalDdlBefore(0 | 13..=u8::MAX)
                | MigrationFault::ApprovalDdlAfter(0 | 13..=u8::MAX)
        )
    ) {
        return Err(StorageError::MigrationInterrupted);
    }
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
    #[cfg(test)]
    if let Some(scenario) = namespace {
        scenario.install(&mut transaction).await?;
    }
    let current = version(&mut transaction).await?;
    validate(&mut transaction, current).await?;
    #[cfg(test)]
    if current < 4 {
        checkpoint(fault, MigrationFault::Ddl)?;
    }
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
    }
    if current < 3 {
        for definition in product_schema::DEFINITIONS {
            sqlx::query(definition).execute(&mut *transaction).await?;
        }
        let epoch = EntityId::from_bytes(
            crate::random::bytes().map_err(|_| StorageError::MigrationRandomUnavailable)?,
        );
        sqlx::query(
            "INSERT INTO product_control(singleton,request_epoch,time_floor_ms) VALUES(1,?,0)",
        )
        .bind(epoch.encode())
        .execute(&mut *transaction)
        .await?;
    }
    if current < 4 {
        #[cfg(test)]
        checkpoint(fault, MigrationFault::Product3Ready)?;
        for definition in product_schema::DELIVERY_DEFINITIONS {
            sqlx::query(definition).execute(&mut *transaction).await?;
            #[cfg(test)]
            checkpoint(
                fault,
                if definition.starts_with("CREATE UNIQUE INDEX") {
                    MigrationFault::DeliveryIndex
                } else {
                    MigrationFault::DeliveryTable
                },
            )?;
        }
        #[cfg(test)]
        checkpoint(fault, MigrationFault::Metadata)?;
        sqlx::query("UPDATE node_meta SET schema_version = 4")
            .execute(&mut *transaction)
            .await?;
        #[cfg(test)]
        checkpoint(fault, MigrationFault::Pragma)?;
        sqlx::query("PRAGMA user_version = 4")
            .execute(&mut *transaction)
            .await?;
    }
    if current < 5 {
        add_registration_schema(
            &mut transaction,
            #[cfg(test)]
            fault,
        )
        .await?;
    }
    if current < 6 {
        for (native_index, definition) in native_dispatch_schema::DEFINITIONS.iter().enumerate() {
            sqlx::query(definition).execute(&mut *transaction).await?;
            #[cfg(test)]
            checkpoint(fault, MigrationFault::NativeDdl)?;
            #[cfg(test)]
            checkpoint(fault, MigrationFault::NativeDdlAt(native_index))?;
            #[cfg(not(test))]
            let _ = native_index;
        }
        #[cfg(test)]
        checkpoint(fault, MigrationFault::NativeMetadata)?;
        sqlx::query("UPDATE node_meta SET schema_version=6")
            .execute(&mut *transaction)
            .await?;
        #[cfg(test)]
        checkpoint(fault, MigrationFault::NativePragma)?;
        sqlx::query("PRAGMA user_version=6")
            .execute(&mut *transaction)
            .await?;
    }
    if current < 7 {
        sqlx::query(management_audit_schema::TABLE_DDL)
            .execute(&mut *transaction)
            .await?;
        #[cfg(test)]
        checkpoint(fault, MigrationFault::AuditTable)?;
        sqlx::query(management_audit_schema::INDEX_DDL)
            .execute(&mut *transaction)
            .await?;
        #[cfg(test)]
        checkpoint(fault, MigrationFault::AuditIndex)?;
        validate_management_audit(&mut transaction)
            .await
            .map_err(|_| StorageError::UnrecognizedSchema)?;
        sqlx::query("UPDATE main.node_meta SET schema_version=7")
            .execute(&mut *transaction)
            .await?;
        #[cfg(test)]
        checkpoint(fault, MigrationFault::AuditMetadata)?;
        sqlx::query("PRAGMA main.user_version=7")
            .execute(&mut *transaction)
            .await?;
        #[cfg(test)]
        checkpoint(fault, MigrationFault::AuditPragma)?;
    }
    if current < 8 {
        for (index, definition) in approval_owner_schema::DEFINITIONS.iter().enumerate() {
            #[cfg(test)]
            let ordinal =
                u8::try_from(index + 1).map_err(|_| StorageError::MigrationInterrupted)?;
            #[cfg(test)]
            {
                #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
                if let Some(trace) = &approval_trace {
                    trace.record(
                        ordinal,
                        super::approval_owner::test_control::ApprovalDdlEdge::Before,
                    )?;
                }
                checkpoint(fault, MigrationFault::ApprovalDdlBefore(ordinal))?;
            }
            sqlx::query(&approval_owner_schema::main_ddl(definition))
                .execute(&mut *transaction)
                .await?;
            #[cfg(test)]
            {
                #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
                if let Some(trace) = &approval_trace {
                    trace.record(
                        ordinal,
                        super::approval_owner::test_control::ApprovalDdlEdge::After,
                    )?;
                }
                checkpoint(fault, MigrationFault::ApprovalDdlAfter(ordinal))?;
            }
            #[cfg(not(test))]
            let _ = index;
        }
        sqlx::query("UPDATE main.node_meta SET schema_version=8")
            .execute(&mut *transaction)
            .await?;
        #[cfg(test)]
        checkpoint(fault, MigrationFault::ApprovalMetadata)?;
        sqlx::query("PRAGMA main.user_version=8")
            .execute(&mut *transaction)
            .await?;
        #[cfg(test)]
        checkpoint(fault, MigrationFault::ApprovalPragma)?;
    }
    if current < 9 {
        for (index, definition) in node_catalog_schema::DEFINITIONS.iter().enumerate() {
            #[cfg(test)]
            let ordinal =
                u8::try_from(index + 1).map_err(|_| StorageError::MigrationInterrupted)?;
            #[cfg(test)]
            checkpoint(fault, MigrationFault::CatalogDdlBefore(ordinal))?;
            sqlx::query(&node_catalog_schema::main_ddl(definition))
                .execute(&mut *transaction)
                .await?;
            #[cfg(test)]
            checkpoint(fault, MigrationFault::CatalogDdlAfter(ordinal))?;
            #[cfg(not(test))]
            let _ = index;
        }
        sqlx::query("UPDATE main.node_meta SET schema_version=9")
            .execute(&mut *transaction)
            .await?;
        #[cfg(test)]
        checkpoint(fault, MigrationFault::CatalogMetadata)?;
        sqlx::query("PRAGMA main.user_version=9")
            .execute(&mut *transaction)
            .await?;
        #[cfg(test)]
        checkpoint(fault, MigrationFault::CatalogPragma)?;
    }
    if current < 10 {
        for definition in native_release_schema::DEFINITIONS {
            sqlx::query(definition).execute(&mut *transaction).await?;
        }
        #[cfg(test)]
        checkpoint(fault, MigrationFault::ReleaseDdl)?;
        sqlx::query("UPDATE main.node_meta SET schema_version=10")
            .execute(&mut *transaction)
            .await?;
        #[cfg(test)]
        checkpoint(fault, MigrationFault::ReleaseMetadata)?;
        sqlx::query("PRAGMA main.user_version=10")
            .execute(&mut *transaction)
            .await?;
        #[cfg(test)]
        checkpoint(fault, MigrationFault::ReleasePragma)?;
    }
    if current < 11 {
        for (index, definition) in source_sessions_schema::DEFINITIONS.iter().enumerate() {
            #[cfg(test)]
            let ordinal =
                u8::try_from(index + 1).map_err(|_| StorageError::MigrationInterrupted)?;
            #[cfg(test)]
            checkpoint(fault, MigrationFault::SourceDdlBefore(ordinal))?;
            sqlx::query(&source_sessions_schema::main_ddl(definition))
                .execute(&mut *transaction)
                .await?;
            #[cfg(test)]
            checkpoint(fault, MigrationFault::SourceDdlAfter(ordinal))?;
            #[cfg(not(test))]
            let _ = index;
        }
        #[cfg(test)]
        checkpoint(fault, MigrationFault::SourceMetadata)?;
        sqlx::query("UPDATE main.node_meta SET schema_version=11")
            .execute(&mut *transaction)
            .await?;
        #[cfg(test)]
        checkpoint(fault, MigrationFault::SourcePragma)?;
        sqlx::query("PRAGMA main.user_version=11")
            .execute(&mut *transaction)
            .await?;
    }
    if current < 12 {
        for definition in session_capacity_schema::DEFINITIONS {
            sqlx::query(definition).execute(&mut *transaction).await?;
        }
        #[cfg(test)]
        checkpoint(fault, MigrationFault::CapacityDdl)?;
        session_capacity_schema::recover_legacy(&mut transaction).await?;
        #[cfg(test)]
        checkpoint(fault, MigrationFault::CapacityRecovery)?;
        sqlx::query("UPDATE main.node_meta SET schema_version=12")
            .execute(&mut *transaction)
            .await?;
        #[cfg(test)]
        checkpoint(fault, MigrationFault::CapacityMetadata)?;
        sqlx::query("PRAGMA main.user_version=12")
            .execute(&mut *transaction)
            .await?;
        #[cfg(test)]
        checkpoint(fault, MigrationFault::CapacityPragma)?;
    }
    if current < 13 {
        for (index, definition) in automatic_capacity_release_schema::DEFINITIONS
            .iter()
            .enumerate()
        {
            sqlx::query(definition).execute(&mut *transaction).await?;
            #[cfg(test)]
            checkpoint(
                fault,
                MigrationFault::AutomaticReleaseDdl(
                    u8::try_from(index).map_err(|_| StorageError::UnrecognizedSchema)?,
                ),
            )?;
            #[cfg(not(test))]
            let _ = index;
        }
        sqlx::query("UPDATE main.node_meta SET schema_version=13")
            .execute(&mut *transaction)
            .await?;
        #[cfg(test)]
        checkpoint(fault, MigrationFault::AutomaticReleaseMetadata)?;
        sqlx::query("PRAGMA main.user_version=13")
            .execute(&mut *transaction)
            .await?;
        #[cfg(test)]
        checkpoint(fault, MigrationFault::AutomaticReleasePragma)?;
    }
    if current < 14 {
        for definition in session_tree_schema::DEFINITIONS {
            sqlx::query(definition).execute(&mut *transaction).await?;
        }
        #[cfg(test)]
        checkpoint(fault, MigrationFault::SessionTreeDdl)?;
        sqlx::query("UPDATE main.node_meta SET schema_version=14")
            .execute(&mut *transaction)
            .await?;
        #[cfg(test)]
        checkpoint(fault, MigrationFault::SessionTreeMetadata)?;
        sqlx::query("PRAGMA main.user_version=14")
            .execute(&mut *transaction)
            .await?;
        #[cfg(test)]
        checkpoint(fault, MigrationFault::SessionTreePragma)?;
    }
    #[cfg(test)]
    checkpoint(fault, MigrationFault::Validation)?;
    validate(&mut transaction, crate::storage::SCHEMA_VERSION).await?;
    #[cfg(test)]
    checkpoint(fault, MigrationFault::SourceValidationAfter)?;
    #[cfg(test)]
    if fault == Some(MigrationFault::DeferredCommit) {
        sqlx::query("CREATE TABLE migration_test_parent(id INTEGER PRIMARY KEY)")
            .execute(&mut *transaction)
            .await?;
        sqlx::query("CREATE TABLE migration_test_child(parent_id INTEGER REFERENCES migration_test_parent(id) DEFERRABLE INITIALLY DEFERRED)")
            .execute(&mut *transaction)
            .await?;
        sqlx::query("INSERT INTO migration_test_child VALUES(1)")
            .execute(&mut *transaction)
            .await?;
    }
    #[cfg(test)]
    checkpoint(fault, MigrationFault::Commit)?;
    transaction.commit().await?;
    #[cfg(test)]
    checkpoint(fault, MigrationFault::CommitUnknown)?;
    Ok(())
}

async fn add_registration_schema(
    transaction: &mut SqliteConnection,
    #[cfg(test)] fault: Option<MigrationFault>,
) -> Result<(), StorageError> {
    for (index, definition) in registration_schema::DEFINITIONS.iter().enumerate() {
        sqlx::query(definition).execute(&mut *transaction).await?;
        #[cfg(test)]
        checkpoint(
            fault,
            match index {
                0 => MigrationFault::RegistrationTable,
                1 => MigrationFault::OperationTable,
                _ => MigrationFault::RegistrationIndex,
            },
        )?;
        #[cfg(not(test))]
        let _ = index;
    }
    #[cfg(test)]
    checkpoint(fault, MigrationFault::RegistrationMetadata)?;
    sqlx::query("UPDATE node_meta SET schema_version=5")
        .execute(&mut *transaction)
        .await?;
    #[cfg(test)]
    checkpoint(fault, MigrationFault::RegistrationPragma)?;
    sqlx::query("PRAGMA user_version=5")
        .execute(&mut *transaction)
        .await?;
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/registration_acceptance/m/schema5_registration_acceptance.rs"]
mod independent_schema5;

#[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
pub(crate) async fn initialize_approval_for_test(
    pool: &sqlx::SqlitePool,
    fault: Option<super::approval_owner::test_control::ApprovalMigrationFault>,
    trace: std::sync::Arc<super::approval_owner::test_control::ApprovalMigrationTrace>,
) -> Result<(), StorageError> {
    use super::approval_owner::test_control::{ApprovalDdlEdge, ApprovalMigrationFault as A};
    let fault = fault.map(|fault| match fault {
        A::Ddl {
            ordinal,
            edge: ApprovalDdlEdge::Before,
        } => MigrationFault::ApprovalDdlBefore(ordinal.get()),
        A::Ddl {
            ordinal,
            edge: ApprovalDdlEdge::After,
        } => MigrationFault::ApprovalDdlAfter(ordinal.get()),
        A::Metadata => MigrationFault::ApprovalMetadata,
        A::Pragma => MigrationFault::ApprovalPragma,
        A::DeferredCommit => MigrationFault::DeferredCommit,
        A::AfterCommitReportLost => MigrationFault::CommitUnknown,
    });
    initialize_inner(pool, fault, None, Some(trace)).await
}

#[cfg(test)]
pub async fn initialize_catalog_for_test(
    pool: &sqlx::SqlitePool,
    fault: Option<MigrationFault>,
    namespace: Option<super::schema9_namespace_test_hook::NamespaceScenario>,
) -> Result<(), StorageError> {
    initialize_inner(
        pool,
        fault,
        namespace,
        #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
        None,
    )
    .await
}
