mod support;

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use acp_core::model::Timestamp;
use storage_sqlite::migrate::{
    DATABASE_FILE, FILE_FORMAT_VERSION, OWNED_SCHEMA_VERSION, StorageConfig,
};
use storage_sqlite::session_store::SqliteStore;
use support::{
    AT, column_names, digest_text, meta_rows, raw_pool, raw_write_pool, scalar_i64, schema_sql,
    table_exists, temp_dir,
};

const SESSION: &str = "11111111-1111-4111-8111-111111111111";
const EPOCH: &str = "22222222-2222-4222-8222-222222222222";
const REQUEST: &str = "33333333-3333-4333-8333-333333333333";
const MESSAGE: &str = "44444444-4444-4444-8444-444444444444";
const EVENT: &str = "55555555-5555-4555-8555-555555555555";
const ATTACHMENT: &str = "66666666-6666-4666-8666-666666666666";
const DIRECTORY: &str = "77777777-7777-4777-8777-777777777777";
const TURN: &str = "88888888-8888-4888-8888-888888888888";
const INTERACTION: &str = "99999999-9999-4999-8999-999999999999";
const TRANSFER: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const SUBMISSION: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
const DEVICE: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
const NODE: &str = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
const PAIRING: &str = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee";

const FORMAL_TABLES: [&str; 21] = [
    "owned_attachment",
    "owned_attachment_link",
    "owned_attachment_submission",
    "owned_attachment_submission_item",
    "owned_attachment_transfer",
    "owned_audit",
    "owned_command",
    "owned_device",
    "owned_directory",
    "owned_event",
    "owned_interaction",
    "owned_media_ref",
    "owned_message",
    "owned_node",
    "owned_pairing",
    "owned_pairing_peer",
    "owned_peer_key",
    "owned_session",
    "owned_timeline_anchor",
    "owned_turn",
    "owned_user_input",
];

fn at() -> Timestamp {
    Timestamp::new(AT).expect("timestamp")
}

async fn create_v19(dir: &Path) {
    let store = SqliteStore::open(StorageConfig::new(dir), &at())
        .await
        .expect("create v19 database");
    store.close().await;
}

async fn seed_formal_data(pool: &sqlx::SqlitePool) {
    sqlx::query(
        "INSERT INTO owned_session
           (session_id, title, agent_id, agent_name, state, origin_epoch, version,
            created_at, updated_at, workspace_directory_id, custom_title)
         VALUES (?1, 'Retained title', 'agent', 'Agent', 'idle', ?2, 7, ?3, ?3, ?4,
                 'Custom retained title')",
    )
    .bind(SESSION)
    .bind(EPOCH)
    .bind(AT)
    .bind(DIRECTORY)
    .execute(pool)
    .await
    .expect("session");

    sqlx::query(
        "INSERT INTO owned_directory
           (directory_id, canonical_path, created_at, last_used_at)
         VALUES (?1, '/retained/project', ?2, ?2)",
    )
    .bind(DIRECTORY)
    .bind(AT)
    .execute(pool)
    .await
    .expect("directory");

    sqlx::query(
        "INSERT INTO owned_turn
           (turn_id, session_id, state, queue_index, causation, started_at, ended_at)
         VALUES (?1, ?2, 'completed', 1, ?3, ?4, ?4)",
    )
    .bind(TURN)
    .bind(SESSION)
    .bind(REQUEST)
    .bind(AT)
    .execute(pool)
    .await
    .expect("turn");

    sqlx::query(
        "INSERT INTO owned_message
           (message_id, session_id, turn_id, channel, state, content_json,
            created_at, updated_at, first_sequence, first_origin_sequence)
         VALUES (?1, ?2, ?3, 'user', 'completed',
                 '[{\"type\":\"text\",\"text\":\"keep me\"}]', ?4, ?4, 40, 40)",
    )
    .bind(MESSAGE)
    .bind(SESSION)
    .bind(TURN)
    .bind(AT)
    .execute(pool)
    .await
    .expect("message");

    sqlx::query(
        "INSERT INTO owned_event
           (session_id, session_sequence, origin_epoch, origin_sequence, event_id,
            turn_id, event_type, kind, policy, origin_kind, causation, payload_json,
            payload_digest, created_at)
         VALUES (?1, 41, ?2, 41, ?3, ?4, 'agent.message.completed', 'final_message',
                 'durable', 'local_cli', ?5, '{}', ?6, ?7)",
    )
    .bind(SESSION)
    .bind(EPOCH)
    .bind(EVENT)
    .bind(TURN)
    .bind(REQUEST)
    .bind(digest_text("{}"))
    .bind(AT)
    .execute(pool)
    .await
    .expect("event");

    sqlx::query(
        "INSERT INTO owned_command
           (actor_kind, actor_id, request_id, session_id, command, kind,
            expected_version, request_fingerprint, accepted_at, status)
         VALUES ('cli', 'cli', ?1, ?2, 'session.title.set', 'mutation',
                 NULL, ?3, ?4, 'accepted')",
    )
    .bind(REQUEST)
    .bind(SESSION)
    .bind(digest_text("formal command"))
    .bind(AT)
    .execute(pool)
    .await
    .expect("command");

    sqlx::query(
        "INSERT INTO owned_interaction
           (interaction_id, session_id, kind, request_event, state, created_at, turn_id)
         VALUES (?1, ?2, 'permission',
                 (SELECT global_sequence FROM owned_event WHERE event_id=?3),
                 'pending', ?4, ?5)",
    )
    .bind(INTERACTION)
    .bind(SESSION)
    .bind(EVENT)
    .bind(AT)
    .bind(TURN)
    .execute(pool)
    .await
    .expect("interaction");

    sqlx::query(
        "INSERT INTO owned_audit
           (at, action, actor_kind, actor_id, target_kind, target_id, outcome)
         VALUES (?1, 'storage.integrity_failed', 'cli', 'cli', 'session', ?2, 'success')",
    )
    .bind(AT)
    .bind(SESSION)
    .execute(pool)
    .await
    .expect("audit");

    sqlx::query(
        "INSERT INTO owned_attachment
           (attachment_id, sha256, byte_length, media_type, relative_path,
            created_at, last_used_at)
         VALUES (?1, ?2, 4, 'text/plain', 'attachments/formal', ?3, ?3)",
    )
    .bind(ATTACHMENT)
    .bind(digest_text("formal attachment"))
    .bind(AT)
    .execute(pool)
    .await
    .expect("attachment");

    sqlx::query(
        "INSERT INTO owned_attachment_link
           (session_id, attachment_id, generation, sha256)
         VALUES (?1, ?2, 3, ?3)",
    )
    .bind(SESSION)
    .bind(ATTACHMENT)
    .bind(digest_text("formal attachment"))
    .execute(pool)
    .await
    .expect("attachment link");

    sqlx::query(
        "INSERT INTO owned_attachment_transfer
           (transfer_id, session_id, subject_kind, subject_id, total_length,
            received_length, chunk_size, state, media_type, file_name,
            created_at, updated_at)
         VALUES (?1, ?2, 'device', ?3, 8, 4, 32768, 'receiving', 'text/plain',
                 'formal.txt', ?4, ?4)",
    )
    .bind(TRANSFER)
    .bind(SESSION)
    .bind(DEVICE)
    .bind(AT)
    .execute(pool)
    .await
    .expect("transfer");

    sqlx::query(
        "INSERT INTO owned_attachment_submission
           (submission_id, session_id, subject_kind, subject_id, state,
            attachment_count, manifest_digest, created_at, sealed_at)
         VALUES (?1, ?2, 'device', ?3, 'sealed', 1, ?4, ?5, ?5)",
    )
    .bind(SUBMISSION)
    .bind(SESSION)
    .bind(DEVICE)
    .bind(digest_text("formal manifest"))
    .bind(AT)
    .execute(pool)
    .await
    .expect("submission");

    sqlx::query(
        "INSERT INTO owned_attachment_submission_item
           (submission_id, position, attachment_id)
         VALUES (?1, 0, ?2)",
    )
    .bind(SUBMISSION)
    .bind(ATTACHMENT)
    .execute(pool)
    .await
    .expect("submission item");

    sqlx::query(
        "INSERT INTO owned_timeline_anchor
           (session_id, entity_kind, entity_id, first_sequence, first_origin_sequence)
         VALUES (?1, 'message', ?2, 41, 41)",
    )
    .bind(SESSION)
    .bind(MESSAGE)
    .execute(pool)
    .await
    .expect("timeline anchor");

    sqlx::query(
        "INSERT INTO owned_media_ref
           (message_id, position, attachment_id, kind)
         VALUES (?1, 0, ?2, 'file')",
    )
    .bind(MESSAGE)
    .bind(ATTACHMENT)
    .execute(pool)
    .await
    .expect("media ref");

    sqlx::query(
        "INSERT INTO owned_user_input
           (message_id, session_id, turn_id, actor_kind, actor_id, request_id,
            content_digest, attachment_count, created_at)
         VALUES (?1, ?2, ?3, 'cli', 'cli', ?4, ?5, 1, ?6)",
    )
    .bind(MESSAGE)
    .bind(SESSION)
    .bind(TURN)
    .bind(REQUEST)
    .bind(digest_text("formal user input"))
    .bind(AT)
    .execute(pool)
    .await
    .expect("user input");

    let device_key = vec![4_u8; 65];
    sqlx::query(
        "INSERT INTO owned_device
           (device_id, display_name, public_key, fingerprint, state, created_at, last_seen_at)
         VALUES (?1, 'Retained device', ?2, ?3, 'active', ?4, ?4)",
    )
    .bind(DEVICE)
    .bind(&device_key)
    .bind("a".repeat(64))
    .bind(AT)
    .execute(pool)
    .await
    .expect("device");

    sqlx::query(
        "INSERT INTO owned_node
           (node_id, kind, display_name, fingerprint, state, owner_endpoint,
            created_at, last_connected_at)
         VALUES (?1, 'owner', 'Retained node', ?2, 'paired',
                 'https://node.example', ?3, ?3)",
    )
    .bind(NODE)
    .bind("b".repeat(64))
    .bind(AT)
    .execute(pool)
    .await
    .expect("node");

    sqlx::query(
        "INSERT INTO owned_peer_key
           (peer_kind, peer_id, public_key, fingerprint, bound_at)
         VALUES ('device', ?1, ?2, ?3, ?4)",
    )
    .bind(DEVICE)
    .bind(&device_key)
    .bind("a".repeat(64))
    .bind(AT)
    .execute(pool)
    .await
    .expect("peer key");

    sqlx::query(
        "INSERT INTO owned_pairing
           (pairing_id, target_kind, state, display_name, secret_digest,
            host_binding, created_at, expires_at)
         VALUES (?1, 'device', 'created', 'Retained pairing', ?2,
                 'https://origin.example', ?3, ?4)",
    )
    .bind(PAIRING)
    .bind(digest_text("pairing secret hash"))
    .bind(AT)
    .bind("2026-10-26T01:00:00Z")
    .execute(pool)
    .await
    .expect("pairing");

    sqlx::query(
        "INSERT INTO owned_pairing_peer
           (pairing_id, peer_kind, peer_id, display_name, public_key,
            fingerprint, client_nonce, claimed_at)
         VALUES (?1, 'device', ?2, 'Retained device', ?3, ?4, 'nonce', ?5)",
    )
    .bind(PAIRING)
    .bind(DEVICE)
    .bind(&device_key)
    .bind("c".repeat(64))
    .bind(AT)
    .execute(pool)
    .await
    .expect("pairing peer");

    sqlx::query(
        "INSERT INTO owned_first_submission
           (session_id, first_submission_id, user_message_id, first_request_id,
            current_request_id, actor_kind, actor_id, request_fingerprint,
            input_json, input_digest, mode_id, config_json, stage,
            ever_dispatched, agent_session_id, resource_epoch, terminal_outcome,
            terminal_at, error_code, error_message, error_details_json,
            retryable, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4, 'device', 'device', ?5,
                 '[]', ?6, NULL, '{}', 'saved_not_dispatched',
                 0, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, ?7, ?7)",
    )
    .bind(SESSION)
    .bind("12121212-1212-4212-8212-121212121212")
    .bind(MESSAGE)
    .bind(REQUEST)
    .bind(digest_text("legacy preparation request"))
    .bind(digest_text("legacy preparation input"))
    .bind(AT)
    .execute(pool)
    .await
    .expect("legacy preparation row");

    for (table, sequence) in [
        ("owned_event", 101_i64),
        ("owned_command", 102),
        ("owned_audit", 103),
    ] {
        sqlx::query("UPDATE sqlite_sequence SET seq=?1 WHERE name=?2")
            .bind(sequence)
            .bind(table)
            .execute(pool)
            .await
            .expect("sequence bump");
    }
}

async fn create_v18(dir: &Path, seed: bool) -> PathBuf {
    let path = dir.join(DATABASE_FILE);
    let pool = raw_write_pool(&path).await;
    sqlx::raw_sql(include_str!("fixtures/v18_formal.sql"))
        .execute(&pool)
        .await
        .expect("create frozen v18 fixture");
    if seed {
        seed_formal_data(&pool).await;
    }
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&pool)
        .await
        .expect("checkpoint");
    pool.close().await;

    let preparation = dir.join("attachments").join(".preparation");
    std::fs::create_dir_all(preparation.join("nested")).expect("preparation dir");
    std::fs::write(preparation.join("nested/upload.part"), b"uncommitted")
        .expect("preparation file");
    path
}

#[derive(Debug, PartialEq)]
struct FormalSnapshot {
    tables: Vec<(String, Vec<String>)>,
    sequences: Vec<String>,
}

async fn formal_snapshot(pool: &sqlx::SqlitePool) -> FormalSnapshot {
    let table_names: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master
         WHERE type='table' AND substr(name, 1, 6)='owned_'
           AND name<>'owned_first_submission'
         ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .expect("formal table names");
    assert_eq!(
        table_names,
        FORMAL_TABLES.map(str::to_owned).to_vec(),
        "the frozen fixture must contain every formal owned table"
    );

    let mut tables = Vec::new();
    for table in table_names {
        let columns: Vec<String> = column_names(pool, &table)
            .await
            .into_iter()
            .filter(|column| table != "owned_session" || column != "deleted_at")
            .collect();
        let projection = columns
            .iter()
            .map(|column| format!("quote(\"{}\")", column.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" || '|' || ");
        let rows = sqlx::query_scalar::<_, String>(&format!(
            "SELECT {projection} FROM \"{table}\" ORDER BY rowid"
        ))
        .fetch_all(pool)
        .await
        .unwrap_or_else(|error| panic!("snapshot {table}: {error}"));
        tables.push((table, rows));
    }
    let sequences = sqlx::query_scalar::<_, String>(
        "SELECT name || '=' || seq FROM sqlite_sequence ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .expect("sequence snapshot");
    FormalSnapshot { tables, sequences }
}

#[tokio::test]
async fn v18_to_v19_drops_preparation_and_preserves_formal_data_idempotently() {
    let dir = temp_dir("migration-v19-success");
    let path = create_v18(&dir, true).await;

    let before_pool = raw_pool(&path).await;
    assert_eq!(scalar_i64(&before_pool, "PRAGMA user_version").await, 18);
    let before = formal_snapshot(&before_pool).await;
    let before_meta: Vec<_> = meta_rows(&before_pool)
        .await
        .into_iter()
        .filter(|(key, _)| key != "owned_schema_version")
        .collect();
    assert!(table_exists(&before_pool, "owned_first_submission").await);
    assert_eq!(
        scalar_i64(&before_pool, "SELECT COUNT(*) FROM owned_first_submission").await,
        1
    );
    assert!(
        !column_names(&before_pool, "owned_session")
            .await
            .iter()
            .any(|column| column == "deleted_at")
    );
    before_pool.close().await;

    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("upgrade v18 to v19");
    assert_eq!(store.metadata().owned_schema_version, OWNED_SCHEMA_VERSION);
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(&pool, "PRAGMA user_version").await,
        FILE_FORMAT_VERSION
    );
    assert_eq!(FILE_FORMAT_VERSION, 19);
    assert_eq!(OWNED_SCHEMA_VERSION, 19);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT value FROM meta WHERE key='owned_schema_version'",)
            .fetch_one(&pool)
            .await
            .expect("owned schema version"),
        "19"
    );
    assert_eq!(
        column_names(&pool, "owned_session")
            .await
            .last()
            .map(String::as_str),
        Some("deleted_at")
    );
    assert!(!table_exists(&pool, "owned_first_submission").await);
    assert!(!dir.join("attachments/.preparation").exists());
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_user_input").await,
        1,
        "formal user input must remain intact"
    );
    assert_eq!(formal_snapshot(&pool).await, before);
    assert_eq!(
        meta_rows(&pool)
            .await
            .into_iter()
            .filter(|(key, _)| key != "owned_schema_version")
            .collect::<Vec<_>>(),
        before_meta
    );
    let schema_after_upgrade = schema_sql(&pool).await;
    let meta_after_upgrade = meta_rows(&pool).await;
    pool.close().await;

    for _ in 0..2 {
        let store = SqliteStore::open(StorageConfig::new(&dir), &at())
            .await
            .expect("repeat open");
        store.close().await;
    }
    let pool = raw_pool(&path).await;
    assert_eq!(schema_sql(&pool).await, schema_after_upgrade);
    assert_eq!(meta_rows(&pool).await, meta_after_upgrade);
    assert_eq!(formal_snapshot(&pool).await, before);
    pool.close().await;
}

#[tokio::test]
async fn v19_migration_failure_rolls_back_column_table_and_versions() {
    let dir = temp_dir("migration-v19-rollback");
    let path = create_v18(&dir, true).await;
    let pool = raw_write_pool(&path).await;
    sqlx::raw_sql(
        "CREATE TRIGGER reject_v19 BEFORE UPDATE OF value ON meta
         WHEN NEW.key='owned_schema_version' AND NEW.value='19'
         BEGIN
           SELECT RAISE(ABORT, 'reject v19');
         END;",
    )
    .execute(&pool)
    .await
    .expect("reject trigger");
    let before = formal_snapshot(&pool).await;
    let schema_before = schema_sql(&pool).await;
    pool.close().await;

    assert!(
        SqliteStore::open(StorageConfig::new(&dir), &at())
            .await
            .is_err(),
        "the injected final metadata failure must fail the migration"
    );

    let pool = raw_write_pool(&path).await;
    assert_eq!(scalar_i64(&pool, "PRAGMA user_version").await, 18);
    assert!(table_exists(&pool, "owned_first_submission").await);
    assert!(
        !column_names(&pool, "owned_session")
            .await
            .iter()
            .any(|column| column == "deleted_at")
    );
    assert_eq!(schema_sql(&pool).await, schema_before);
    assert_eq!(formal_snapshot(&pool).await, before);
    assert!(dir.join("attachments/.preparation").exists());
    sqlx::raw_sql("DROP TRIGGER reject_v19;")
        .execute(&pool)
        .await
        .expect("drop trigger");
    pool.close().await;

    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("retry succeeds");
    store.close().await;
    let pool = raw_pool(&path).await;
    assert_eq!(scalar_i64(&pool, "PRAGMA user_version").await, 19);
    assert!(!table_exists(&pool, "owned_first_submission").await);
    pool.close().await;
}

#[tokio::test]
async fn preparation_cleanup_failure_keeps_committed_v19_and_retries() {
    let dir = temp_dir("migration-v19-cleanup-failure");
    let path = create_v18(&dir, false).await;
    let preparation = dir.join("attachments/.preparation");
    let locked_file = preparation.join("nested/upload.part");

    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    let held = options.open(&locked_file).expect("hold preparation file");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&preparation, std::fs::Permissions::from_mode(0o555))
            .expect("make preparation read-only");
    }

    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("post-commit cleanup failure must warn instead of blocking startup");
    store.close().await;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&preparation, std::fs::Permissions::from_mode(0o700))
            .expect("restore preparation permissions");
    }
    drop(held);

    let pool = raw_pool(&path).await;
    assert_eq!(scalar_i64(&pool, "PRAGMA user_version").await, 19);
    assert!(!table_exists(&pool, "owned_first_submission").await);
    assert!(
        column_names(&pool, "owned_session")
            .await
            .iter()
            .any(|column| column == "deleted_at")
    );
    pool.close().await;
    assert!(
        preparation.exists(),
        "the post-commit cleanup failure must leave durable SQL and retryable filesystem state"
    );

    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("retry after cleanup becomes possible");
    store.close().await;
    assert!(!preparation.exists());
}

#[tokio::test]
async fn future_file_and_owned_schema_versions_are_rejected_without_writes() {
    for (name, statement) in [
        (
            "file",
            "PRAGMA user_version = 20; INSERT INTO meta (key,value) VALUES ('sentinel','kept');",
        ),
        (
            "owned",
            "UPDATE meta SET value='20' WHERE key='owned_schema_version';
             INSERT INTO meta (key,value) VALUES ('sentinel','kept');",
        ),
    ] {
        let dir = temp_dir(&format!("migration-v19-future-{name}"));
        create_v19(&dir).await;
        let path = dir.join(DATABASE_FILE);
        let pool = raw_write_pool(&path).await;
        sqlx::raw_sql(statement)
            .execute(&pool)
            .await
            .expect("advance future version");
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&pool)
            .await
            .expect("checkpoint");
        pool.close().await;
        let before = std::fs::read(&path).expect("database bytes");

        assert!(
            SqliteStore::open(StorageConfig::new(&dir), &at())
                .await
                .is_err(),
            "future {name} version must be rejected"
        );
        assert_eq!(
            std::fs::read(&path).expect("database bytes after rejection"),
            before,
            "future {name} version rejection must leave the file untouched"
        );
    }
}
