//! Additive necessary source-operation facts; no response bodies or credentials.
use crate::error::StorageError;
use sqlx::SqliteConnection;

pub(super) const DEFINITIONS: &[&str] = &[
    r#"CREATE TABLE node_source_session_registrations (
  operation_id TEXT PRIMARY KEY CHECK(length(operation_id)=22),
  action TEXT NOT NULL CHECK(action IN ('discover','create')),
  actor_id TEXT NOT NULL REFERENCES clients(client_id),
  register_auth_generation INTEGER NOT NULL CHECK(register_auth_generation>=0),
  source_id TEXT NOT NULL REFERENCES product_sources(source_id),
  harness_id TEXT NOT NULL REFERENCES harnesses(harness_id),
  profile_id TEXT NOT NULL,
  profile_revision BLOB NOT NULL CHECK(length(profile_revision)=8 AND profile_revision!=X'0000000000000000'),
  workspace_id TEXT NOT NULL REFERENCES workspaces(workspace_id),
  launch_path TEXT NOT NULL CHECK(length(launch_path)>0),
  cursor TEXT,
  intent_digest BLOB NOT NULL CHECK(length(intent_digest)=32),
  registered_at_ms INTEGER NOT NULL,
  UNIQUE(operation_id,action),
  FOREIGN KEY(profile_id,profile_revision) REFERENCES node_launch_profiles(profile_id,revision),
  CHECK(action='discover' OR cursor IS NULL)
) STRICT"#,
    r#"CREATE TABLE node_source_session_operations (
  operation_id TEXT PRIMARY KEY,
  action TEXT NOT NULL CHECK(action IN ('discover','create')),
  runtime_epoch TEXT NOT NULL CHECK(length(runtime_epoch)=22),
  admission_auth_generation INTEGER NOT NULL CHECK(admission_auth_generation>=0),
  admitted_at_ms INTEGER NOT NULL,
  operation_state TEXT NOT NULL CHECK(operation_state IN ('owned','terminal','interrupted')),
  outcome TEXT NOT NULL CHECK(outcome IN ('pending','succeeded','refused','unknown')),
  source_instance_id TEXT,
  connection_epoch TEXT,
  protocol_version INTEGER CHECK(protocol_version IN (1,2)),
  rpc_id_kind TEXT CHECK(rpc_id_kind IN ('integer','string')),
  rpc_id_bytes BLOB,
  frame_length BLOB CHECK(frame_length IS NULL OR (length(frame_length)=8 AND frame_length>X'0000000000000000')),
  accepted_length BLOB CHECK(accepted_length IS NULL OR length(accepted_length)=8),
  transport_state TEXT CHECK(transport_state IN ('reserved','armed','zero','unknown','written')),
  transport_reason TEXT CHECK(transport_reason IN
    ('cancelled','deadline','fence_rejected','connection_closed','abandoned',
     'io','invalid_fence','connection_lost','restart')),
  transport_recorded_at_ms INTEGER,
  response_kind TEXT CHECK(response_kind IN ('created','sessions','remote_error')),
  native_session_id TEXT,
  remote_error_code INTEGER,
  response_sequence BLOB CHECK(response_sequence IS NULL OR length(response_sequence)=8),
  response_receipt BLOB CHECK(response_receipt IS NULL OR length(response_receipt)=8),
  response_evidence BLOB CHECK(response_evidence IS NULL OR length(response_evidence)=32),
  result_bytes BLOB CHECK(result_bytes IS NULL OR length(result_bytes)=8),
  ack_state TEXT CHECK(ack_state IN ('pending','confirmed','rejected','unknown')),
  failure_class TEXT CHECK(failure_class IN
    ('configuration','protocol','source','metadata','ownership','allocation','interrupted')),
  updated_at_ms INTEGER NOT NULL,
  terminal_at_ms INTEGER,
  FOREIGN KEY(operation_id,action) REFERENCES node_source_session_registrations(operation_id,action),
  FOREIGN KEY(source_instance_id,connection_epoch)
    REFERENCES node_native_source_instances(instance_id,connection_epoch),
  UNIQUE(connection_epoch,rpc_id_kind,rpc_id_bytes),
  CHECK((source_instance_id IS NULL AND connection_epoch IS NULL AND protocol_version IS NULL)
     OR (source_instance_id IS NOT NULL AND length(source_instance_id)=22
         AND connection_epoch IS NOT NULL AND length(connection_epoch)=22)),
  CHECK((rpc_id_kind IS NULL AND rpc_id_bytes IS NULL AND frame_length IS NULL
         AND accepted_length IS NULL AND transport_state IS NULL AND transport_reason IS NULL
         AND transport_recorded_at_ms IS NULL)
     OR (source_instance_id IS NOT NULL AND protocol_version IS NOT NULL
         AND rpc_id_kind IS NOT NULL AND rpc_id_bytes IS NOT NULL AND frame_length IS NOT NULL
         AND accepted_length IS NOT NULL AND accepted_length<=frame_length
         AND transport_state IS NOT NULL
         AND (rpc_id_kind='string' OR length(rpc_id_bytes)=8))),
  CHECK(transport_state IS NULL OR
    (transport_state IN ('reserved','armed') AND accepted_length=X'0000000000000000'
      AND transport_reason IS NULL AND transport_recorded_at_ms IS NULL) OR
    (transport_state='zero' AND accepted_length=X'0000000000000000'
      AND transport_reason IS NOT NULL AND transport_reason IN ('cancelled','deadline','fence_rejected','connection_closed','abandoned')
      AND transport_recorded_at_ms IS NOT NULL) OR
    (transport_state='unknown' AND transport_reason IS NOT NULL AND transport_reason IN
       ('io','deadline','cancelled','fence_rejected','invalid_fence','connection_lost','restart')
      AND transport_recorded_at_ms IS NOT NULL) OR
    (transport_state='written' AND accepted_length=frame_length
      AND transport_reason IS NULL AND transport_recorded_at_ms IS NOT NULL)),
  CHECK((response_kind IS NULL AND native_session_id IS NULL AND remote_error_code IS NULL
         AND response_sequence IS NULL AND response_receipt IS NULL AND response_evidence IS NULL
         AND result_bytes IS NULL AND ack_state IS NULL)
     OR (response_kind IS NOT NULL AND rpc_id_kind IS NOT NULL
         AND response_sequence IS NOT NULL AND response_receipt IS NOT NULL
         AND response_evidence IS NOT NULL AND result_bytes IS NOT NULL AND ack_state IS NOT NULL
         AND ((response_kind='created' AND action='create' AND native_session_id IS NOT NULL
               AND remote_error_code IS NULL)
           OR (response_kind='sessions' AND action='discover' AND native_session_id IS NULL
               AND remote_error_code IS NULL)
           OR (response_kind='remote_error' AND native_session_id IS NULL
               AND remote_error_code IS NOT NULL)))),
  CHECK((operation_state='owned' AND outcome='pending' AND terminal_at_ms IS NULL)
     OR (operation_state='terminal' AND outcome IN ('succeeded','refused','unknown')
         AND terminal_at_ms IS NOT NULL)
     OR (operation_state='interrupted' AND outcome='unknown' AND terminal_at_ms IS NOT NULL)),
  CHECK(outcome!='succeeded' OR (response_kind IS NOT NULL AND ack_state IS NOT NULL
      AND response_kind IN ('created','sessions') AND ack_state='confirmed')),
  CHECK(response_kind IS NULL OR (transport_state IS NOT NULL
      AND transport_state IN ('armed','unknown','written'))),
  CHECK(outcome!='refused' OR failure_class IS NOT NULL
      OR (response_kind IS NOT NULL AND response_kind='remote_error')),
  CHECK(outcome!='refused' OR response_kind IS NULL
     OR (response_kind='remote_error' AND ack_state IS NOT NULL AND ack_state='confirmed'))
) STRICT"#,
    r#"CREATE INDEX node_source_session_runtime_operations
  ON node_source_session_operations(runtime_epoch,operation_state)"#,
    r#"CREATE INDEX node_source_session_created_owners
  ON node_source_session_operations(native_session_id,source_instance_id)
  WHERE response_kind='created'"#,
];

const TABLES: &[&str] = &[
    "node_source_session_registrations",
    "node_source_session_operations",
];
const OBJECTS: &[&str] = &[
    "node_source_session_registrations",
    "node_source_session_operations",
    "node_source_session_runtime_operations",
    "node_source_session_created_owners",
];

pub(super) fn main_ddl(definition: &str) -> String {
    if definition.starts_with("CREATE TABLE ") {
        definition.replacen("CREATE TABLE ", "CREATE TABLE main.", 1)
    } else {
        definition.replacen("CREATE INDEX ", "CREATE INDEX main.", 1)
    }
}

pub(super) async fn validate(
    connection: &mut SqliteConnection,
    current: i64,
) -> Result<(), StorageError> {
    for name in OBJECTS {
        let shadow: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM temp.sqlite_schema WHERE name=? COLLATE NOCASE)",
        )
        .bind(name)
        .fetch_one(&mut *connection)
        .await?;
        if shadow != 0 {
            return Err(StorageError::UnrecognizedSchema);
        }
        if current < 11 {
            let partial: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE name=? COLLATE NOCASE)",
            )
            .bind(name)
            .fetch_one(&mut *connection)
            .await?;
            if partial != 0 {
                return Err(StorageError::UnrecognizedSchema);
            }
        }
    }
    if current < 11 {
        return Ok(());
    }
    super::schema::validate_definitions(connection, DEFINITIONS).await?;
    let fk: i64 = sqlx::query_scalar("PRAGMA main.foreign_keys")
        .fetch_one(&mut *connection)
        .await?;
    if fk != 1 {
        return Err(StorageError::UnrecognizedSchema);
    }
    for table in TABLES {
        let strict: i64 = sqlx::query_scalar("SELECT strict FROM main.pragma_table_list WHERE schema='main' AND name=? AND type='table'")
            .bind(table).fetch_one(&mut *connection).await?;
        let extra: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE type='trigger' AND tbl_name=? COLLATE NOCASE) OR EXISTS(SELECT 1 FROM temp.sqlite_schema WHERE type='trigger' AND tbl_name=? COLLATE NOCASE) OR EXISTS(SELECT 1 FROM main.sqlite_schema WHERE type='index' AND tbl_name=? AND sql IS NOT NULL AND name NOT IN ('node_source_session_runtime_operations','node_source_session_created_owners'))")
            .bind(table).bind(table).bind(table).fetch_one(&mut *connection).await?;
        if strict != 1 || extra != 0 {
            return Err(StorageError::UnrecognizedSchema);
        }
        if !sqlx::query(&format!("PRAGMA main.foreign_key_check('{table}')"))
            .fetch_all(&mut *connection)
            .await?
            .is_empty()
        {
            return Err(StorageError::UnrecognizedSchema);
        }
    }
    // A selected profile must belong to this explicit source/harness and workspace.
    let invalid: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_source_session_registrations r JOIN main.node_launch_profiles p ON p.profile_id=r.profile_id AND p.revision=r.profile_revision WHERE p.source_id!=r.source_id OR p.harness_id!=r.harness_id) OR EXISTS(SELECT 1 FROM main.node_source_session_operations o JOIN main.node_source_session_registrations r ON r.operation_id=o.operation_id JOIN main.node_native_source_instances s ON s.instance_id=o.source_instance_id AND s.connection_epoch=o.connection_epoch WHERE s.source_id!=r.source_id)")
        .fetch_one(&mut *connection).await?;
    if invalid != 0 {
        return Err(StorageError::UnrecognizedSchema);
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
