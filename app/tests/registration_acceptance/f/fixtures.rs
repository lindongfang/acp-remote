// Independent pinned pre-schema5 fixture helper; not a downgraded current database.
use crate::domain::EntityId;
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
const ROOT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../.tmp/q3-registration-fault-fixtures"
);
static NEXT: AtomicU64 = AtomicU64::new(0);
pub(super) struct Area(pub(super) PathBuf);
impl Area {
    pub(super) fn new() -> Self {
        let p = PathBuf::from(ROOT).join(format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(ROOT).unwrap();
        std::fs::create_dir(&p).unwrap();
        let root = std::fs::canonicalize(ROOT).unwrap();
        let owned = std::fs::canonicalize(&p).unwrap();
        assert_eq!(owned.parent(), Some(root.as_path()));
        Self(owned)
    }
    pub(super) fn db(&self) -> PathBuf {
        self.0.join("node.sqlite3")
    }
}
impl Drop for Area {
    fn drop(&mut self) {
        let root = std::fs::canonicalize(ROOT).unwrap();
        let owned = std::fs::canonicalize(&self.0).unwrap();
        assert_eq!(owned.parent(), Some(root.as_path()));
        assert_eq!(owned, root.join(self.0.file_name().unwrap()));
        assert!(owned.starts_with(&root));
        std::fs::remove_dir_all(&owned).unwrap();
    }
}
fn id(n: u8) -> String {
    EntityId::from_bytes([n; 16]).encode()
}
pub(super) async fn connect(path: &Path) -> SqliteConnection {
    SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true),
    )
    .await
    .unwrap()
}
pub(super) async fn exec(c: &mut SqliteConnection, sql: &str) {
    sqlx::query(sql).execute(c).await.unwrap();
}
pub(super) async fn fixture_base(a: &Area, v: i64) {
    let mut c = connect(&a.db()).await;
    for ddl in LEGACY_CORE {
        exec(&mut c, ddl).await;
    }
    sqlx::query(
        "INSERT INTO node_meta VALUES(1,?,'pinned-original-node',X'04010200ff','identity.key')",
    )
    .bind(v)
    .execute(&mut c)
    .await
    .unwrap();
    exec(&mut c, "INSERT INTO node_state VALUES(1,987)").await;
    sqlx::query("INSERT INTO clients VALUES(?,X'0100ff','trusted-original','trusted',12,100,NULL),(?,X'0200ff','revoked-original','revoked',19,200,300)").bind(id(1)).bind(id(2)).execute(&mut c).await.unwrap();
    exec(&mut c,"INSERT INTO management_commands VALUES('old-actor','old-request','device.revoke','old-digest','opaque-old-result',4102444800)").await;
    exec(&mut c,"INSERT INTO security_audit VALUES(42,'old-actor','device.revoke','old-target','revoked',300)").await;
    if v >= 2 {
        for ddl in LEGACY_SESSION {
            exec(&mut c, ddl).await;
        }
        exec(
            &mut c,
            "INSERT INTO harnesses VALUES('old-h','original','old.exe','[]',NULL)",
        )
        .await;
        exec(
            &mut c,
            "INSERT INTO workspaces VALUES('old-w','D:/synthetic-only')",
        )
        .await;
        exec(&mut c,"INSERT INTO sessions VALUES('old-s','old-h','old-w','interrupted','old-source','native-original','{}')").await;
        sqlx::query("INSERT INTO content_objects VALUES('old-content','old-s',?,12,'output',X'00ff7b7d','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',4)").bind(id(1)).execute(&mut c).await.unwrap();
        sqlx::query("INSERT INTO session_commands VALUES('old-c',?,'old-r','session.input','old-digest','old-s','delivery_unknown')").bind(id(1)).execute(&mut c).await.unwrap();
        exec(
            &mut c,
            "INSERT INTO turns VALUES('old-t','old-s','old-c',17)",
        )
        .await;
        exec(
            &mut c,
            "INSERT INTO runs VALUES('old-run','old-s','old-c','old-t','unknown')",
        )
        .await;
        exec(&mut c,"INSERT INTO conversation_items(item_id,session_id,turn_id,source_id,first_position,kind,content_ref,content_version,committed_version) VALUES('old-i','old-s','old-t','original-source',11,'unknown-extension','old-content',5,3)").await;
        exec(
            &mut c,
            "INSERT INTO approval_connections VALUES('old-instance',7,'closed')",
        )
        .await;
        exec(&mut c,"INSERT INTO approval_run_sources VALUES('old-run','old-s','old-t','old-instance',7,'native-original','invalid','connection-lost')").await;
        exec(&mut c,"INSERT INTO approvals(approval_id,session_id,run_id,turn_id,instance_id,connection_generation,native_session_id,native_request_id,source_digest,request_kind,protocol,protocol_version,raw_request,scope_raw,status) VALUES('old-a','old-s','old-run','old-t','old-instance',7,'native-original','1','digest','tool_permission','acp','1',X'00ff7b7d',X'010203','invalid')").await;
        exec(&mut c,"INSERT INTO approval_options VALUES('old-choice','old-a',0,'once','synthetic option','allow_once')").await;
        exec(
            &mut c,
            "INSERT INTO approval_deliveries VALUES('old-a','unknown','old-claim','unknown',0)",
        )
        .await;
        exec(&mut c,"INSERT INTO approval_audit_links VALUES(42,'old-a','old-c','old-basis',12,'old-instance',7)").await;
        exec(&mut c, "INSERT INTO session_state VALUES('old-s',23,12,18)").await;
        exec(
            &mut c,
            "INSERT INTO history_integrity VALUES('old-s','old-run','synthetic-original',11,11,4)",
        )
        .await;
        exec(
            &mut c,
            "INSERT INTO history_content_pins VALUES('old-pin','old-content')",
        )
        .await;
    }
    if v == 3 {
        for ddl in PINNED_PRODUCT3 {
            exec(&mut c, ddl).await;
        }
        sqlx::query("INSERT INTO product_control VALUES(1,?,0)")
            .bind(id(60))
            .execute(&mut c)
            .await
            .unwrap();
    }
    exec(&mut c, &format!("PRAGMA user_version={v}")).await;
    c.close().await.unwrap();
    std::fs::write(
        a.0.join("identity.key"),
        b"synthetic-protected-key-byte-sentinel\0\xff",
    )
    .unwrap();
}
const PINNED_PRODUCT3: &[&str] = &[
    r"CREATE TABLE product_control (
        singleton INTEGER PRIMARY KEY CHECK(singleton=1),
        request_epoch TEXT NOT NULL CHECK(length(request_epoch)=22),
        time_floor_ms INTEGER NOT NULL CHECK(time_floor_ms BETWEEN 0 AND 9007199254740991)) STRICT",
    r"CREATE TABLE product_sources (
        source_id TEXT PRIMARY KEY CHECK(length(source_id)=22),
        name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 80)) STRICT",
    r"CREATE TABLE product_harness_sources (
        harness_id TEXT PRIMARY KEY REFERENCES harnesses(harness_id),
        source_id TEXT NOT NULL REFERENCES product_sources(source_id)) STRICT",
    r"CREATE TABLE product_targets (
        target_id TEXT PRIMARY KEY CHECK(length(target_id)=22),
        source_id TEXT NOT NULL REFERENCES product_sources(source_id),
        native_id TEXT NOT NULL CHECK(length(CAST(native_id AS BLOB)) BETWEEN 1 AND 4096),
        workspace_id TEXT REFERENCES workspaces(workspace_id),
        UNIQUE(source_id,native_id)) STRICT",
    r"CREATE TABLE product_commands (
        command_id TEXT PRIMARY KEY CHECK(length(command_id)=22),
        actor_id TEXT NOT NULL REFERENCES clients(client_id),
        auth_generation INTEGER NOT NULL CHECK(auth_generation BETWEEN 0 AND 9007199254740991),
        request_id TEXT NOT NULL CHECK(length(request_id)=22),
        request_epoch TEXT NOT NULL CHECK(length(request_epoch)=22),
        action TEXT NOT NULL CHECK(action IN ('harness.register','workspace.register','session.create','session.load','session.input','session.cancel','session.release','session.resume','approval.decide')),
        target_kind TEXT NOT NULL CHECK(target_kind IN ('harness','workspace','source','session','approval')),
        target_id TEXT NOT NULL CHECK(length(target_id)=22),
        payload_digest TEXT NOT NULL CHECK(length(payload_digest)=64 AND payload_digest NOT GLOB '*[^0-9a-f]*'),
        issued_at INTEGER NOT NULL CHECK(issued_at BETWEEN 0 AND 9007199254740991),
        expires_at INTEGER NOT NULL CHECK(expires_at BETWEEN 0 AND 9007199254740991 AND expires_at>issued_at AND expires_at-issued_at<=86400000),
        status TEXT NOT NULL CHECK(status IN ('accepted','dispatching','completed','failed','unknown','expired')),
        claim_id TEXT UNIQUE CHECK(claim_id IS NULL OR length(claim_id)=22),
        CHECK(status!='dispatching' OR claim_id IS NOT NULL),
        UNIQUE(actor_id,request_id)) STRICT",
    r"CREATE TABLE product_runs (
        run_id TEXT PRIMARY KEY CHECK(length(run_id)=22),
        command_id TEXT NOT NULL UNIQUE REFERENCES product_commands(command_id),
        target_id TEXT NOT NULL REFERENCES product_targets(target_id),
        status TEXT NOT NULL CHECK(status IN ('accepted','dispatching','running','cancelling','completed','cancelled','failed','interrupted','unknown')),
        UNIQUE(target_id,run_id)) STRICT",
    r"CREATE TABLE product_resources (
        load_epoch TEXT PRIMARY KEY CHECK(length(load_epoch)=22),
        target_id TEXT NOT NULL REFERENCES product_targets(target_id),
        runtime_epoch TEXT NOT NULL CHECK(length(runtime_epoch)=22),
        connection_epoch TEXT NOT NULL CHECK(length(connection_epoch)=22),
        instance_id TEXT NOT NULL CHECK(length(instance_id)=22),
        state TEXT NOT NULL CHECK(state IN ('reserved','loaded','releasing','released','unknown')),
        revision INTEGER NOT NULL CHECK(revision BETWEEN 0 AND 9007199254740991),
        UNIQUE(target_id,load_epoch)) STRICT",
    r"CREATE TABLE product_approvals (
        approval_id TEXT PRIMARY KEY CHECK(length(approval_id)=22),
        target_id TEXT NOT NULL REFERENCES product_targets(target_id),
        load_epoch TEXT NOT NULL,
        native_request_digest TEXT NOT NULL CHECK(length(native_request_digest)=64 AND native_request_digest NOT GLOB '*[^0-9a-f]*'),
        request_digest TEXT NOT NULL CHECK(length(request_digest)=64 AND request_digest NOT GLOB '*[^0-9a-f]*'),
        run_id TEXT,
        protocol_version INTEGER NOT NULL CHECK(protocol_version IN (1,2)),
        state TEXT NOT NULL CHECK(state IN ('pending','decided','invalid','unsupported')),
        decision_command_id TEXT UNIQUE REFERENCES product_commands(command_id),
        deciding_actor_id TEXT REFERENCES clients(client_id),
        choice_digest TEXT CHECK(choice_digest IS NULL OR (length(choice_digest)=64 AND choice_digest NOT GLOB '*[^0-9a-f]*')),
        decision_kind TEXT CHECK(decision_kind IN ('allow_once','allow_always','reject_once','reject_always')),
        CHECK(state!='decided' OR (decision_command_id IS NOT NULL AND deciding_actor_id IS NOT NULL AND choice_digest IS NOT NULL AND decision_kind IS NOT NULL)),
        UNIQUE(load_epoch,native_request_digest),
        FOREIGN KEY(target_id,load_epoch) REFERENCES product_resources(target_id,load_epoch),
        FOREIGN KEY(target_id,run_id) REFERENCES product_runs(target_id,run_id)) STRICT",
    r"CREATE TABLE product_deliveries (
        approval_id TEXT PRIMARY KEY REFERENCES product_approvals(approval_id),
        state TEXT NOT NULL CHECK(state IN ('not_sent','claimed','transport_written','unknown','suppressed')),
        claim_id TEXT UNIQUE CHECK(claim_id IS NULL OR length(claim_id)=22),
        CHECK(state NOT IN ('claimed','transport_written','unknown') OR claim_id IS NOT NULL)) STRICT",
    r"CREATE TABLE product_audit_slots (
        command_id TEXT NOT NULL REFERENCES product_commands(command_id),
        stage TEXT NOT NULL CHECK(stage IN ('admission','dispatch','result','reconciliation')),
        actor_id TEXT NOT NULL REFERENCES clients(client_id),
        reserved_at INTEGER NOT NULL CHECK(reserved_at BETWEEN 0 AND 9007199254740991),
        occurred_at INTEGER CHECK(occurred_at BETWEEN 0 AND 9007199254740991),
        retain_until INTEGER CHECK(retain_until BETWEEN 0 AND 9007199254740991),
        outcome TEXT CHECK(outcome IN ('accepted','dispatching','completed','failed','unknown','expired','reconciled','denied','cancelled')),
        CHECK((occurred_at IS NULL AND retain_until IS NULL AND outcome IS NULL) OR
              (occurred_at IS NOT NULL AND retain_until IS NOT NULL AND outcome IS NOT NULL AND retain_until>=occurred_at AND retain_until-occurred_at>=7776000000)),
        PRIMARY KEY(command_id,stage)) STRICT",
    r"CREATE UNIQUE INDEX product_one_foreground_run ON product_runs(target_id) WHERE status IN ('accepted','dispatching','running','cancelling','unknown')",
    r"CREATE UNIQUE INDEX product_one_resource_owner ON product_resources(target_id) WHERE state!='released'",
];

const LEGACY_CORE: &[&str] = &[
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

const LEGACY_SESSION: &[&str] = &[
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
