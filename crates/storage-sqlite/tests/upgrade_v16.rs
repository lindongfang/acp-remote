//! v15 → v16 升级（`docs/CORE_PORTS_AND_STORAGE.md` §7.2）：新增 `owned_first_submission`、把
//! `owned_message.channel` 扩到 `user`，且**不回填**任何旧会话。

mod support;

use acp_core::model::Timestamp;
use storage_sqlite::migrate::{FILE_FORMAT_VERSION, OWNED_SCHEMA_VERSION, StorageConfig};
use storage_sqlite::session_store::SqliteStore;
use support::*;

const SESSION: &str = "11111111-1111-4111-8111-111111111111";
const EPOCH: &str = "22222222-2222-4222-8222-222222222222";
const MESSAGE: &str = "33333333-3333-4333-8333-333333333333";
const ATTACHMENT: &str = "44444444-4444-4444-8444-444444444444";

fn at() -> Timestamp {
    Timestamp::new(AT).expect("timestamp")
}

/// 把新建的 v16 库改造成 v15 形状（`owned_message` 不含 `user`，且没有首条表）。
async fn downgrade_to_v15(pool: &sqlx::SqlitePool, with_bad_channel: bool) {
    sqlx::raw_sql(
        "PRAGMA foreign_keys = OFF;
         DROP TABLE owned_first_submission;
         CREATE TABLE owned_message_v15 (
           message_id TEXT PRIMARY KEY,
           session_id TEXT NOT NULL REFERENCES owned_session(session_id) ON DELETE CASCADE,
           turn_id TEXT,
           channel TEXT NOT NULL CHECK (channel IN ('reply','thought')),
           state TEXT NOT NULL CHECK (state IN ('partial','completed','failed','cancelled')),
           content_json TEXT NOT NULL,
           partial_reason TEXT,
           created_at TEXT NOT NULL,
           updated_at TEXT NOT NULL,
           CHECK (state <> 'completed' OR partial_reason IS NULL)
         ) STRICT;
         INSERT INTO owned_message_v15
           (message_id, session_id, turn_id, channel, state, content_json, partial_reason, created_at, updated_at)
           SELECT message_id, session_id, turn_id, channel, state, content_json, partial_reason, created_at, updated_at
           FROM owned_message;
         DROP TABLE owned_message;
         ALTER TABLE owned_message_v15 RENAME TO owned_message;
         CREATE INDEX owned_message_session ON owned_message(session_id, created_at, message_id);
         PRAGMA user_version = 15;
         UPDATE meta SET value = '15' WHERE key = 'owned_schema_version';
         PRAGMA foreign_keys = ON;",
    )
    .execute(pool)
    .await
    .expect("downgrade");
    if with_bad_channel {
        // 只在失败回滚用例里造一个 v15 CHECK 本身不允许、但无 CHECK 的替身允许的行。
        sqlx::raw_sql(
            "PRAGMA ignore_check_constraints = ON;
             UPDATE owned_message SET channel = 'bogus';",
        )
        .execute(pool)
        .await
        .expect("bad channel");
    }
}

#[tokio::test]
async fn v15_upgrades_without_touching_old_sessions_and_keeps_references() {
    let dir = temp_dir("upgrade-v16");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("fresh");
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    let fresh_message_columns = column_names(&pool, "owned_message").await;
    let epoch: String = sqlx::query_scalar("SELECT value FROM meta WHERE key = 'server_epoch'")
        .fetch_one(&pool)
        .await
        .expect("epoch");
    sqlx::raw_sql(&format!(
        "INSERT INTO owned_session
           (session_id, title, agent_id, agent_name, state, origin_epoch, version, created_at, updated_at)
         VALUES ('{SESSION}', 'old', 'agent', 'Agent', 'idle', '{EPOCH}', 1, '{AT}', '{AT}');
         INSERT INTO owned_attachment
           (attachment_id, sha256, byte_length, media_type, relative_path, created_at, last_used_at)
         VALUES ('{ATTACHMENT}', '{sha}', 3, 'text/plain', 'attachments/{sha}', '{AT}', '{AT}');
         INSERT INTO owned_message
           (message_id, session_id, turn_id, channel, state, content_json, created_at, updated_at)
         VALUES ('{MESSAGE}', '{SESSION}', NULL, 'reply', 'completed', '[{{\"type\":\"text\",\"text\":\"old\"}}]', '{AT}', '{AT}');
         INSERT INTO owned_media_ref (message_id, position, attachment_id, kind)
         VALUES ('{MESSAGE}', 0, '{ATTACHMENT}', 'file');",
        sha = digest_text("old"),
    ))
    .execute(&pool)
    .await
    .expect("seed v15");
    downgrade_to_v15(&pool, false).await;
    let seeded: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&pool)
        .await
        .expect("user version");
    assert_eq!(seeded, 15);
    pool.close().await;

    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("upgrade");
    store.close().await;
    let pool = raw_write_pool(&path).await;

    assert_eq!(
        column_names(&pool, "owned_message").await,
        fresh_message_columns
    );
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&pool)
        .await
        .expect("version");
    assert_eq!(version, FILE_FORMAT_VERSION);
    assert_eq!(FILE_FORMAT_VERSION, 16);
    let owned: String =
        sqlx::query_scalar("SELECT value FROM meta WHERE key = 'owned_schema_version'")
            .fetch_one(&pool)
            .await
            .expect("owned version");
    assert_eq!(owned, OWNED_SCHEMA_VERSION.to_string());
    // 旧行与引用原样保留。
    let old: (String, String) =
        sqlx::query_as("SELECT channel, content_json FROM owned_message WHERE message_id = ?1")
            .bind(MESSAGE)
            .fetch_one(&pool)
            .await
            .expect("old message");
    assert_eq!(old.0, "reply");
    assert!(old.1.contains("old"));
    let media: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM owned_media_ref WHERE message_id = ?1")
            .bind(MESSAGE)
            .fetch_one(&pool)
            .await
            .expect("media");
    assert_eq!(media, 1);
    // 旧会话不被回填成准备，也没有首条关联。
    let submissions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM owned_first_submission")
        .fetch_one(&pool)
        .await
        .expect("submissions");
    assert_eq!(submissions, 0);
    // 新通道可用；外键仍阻止无会话的首条行。
    sqlx::query(
        "INSERT INTO owned_message
           (message_id, session_id, turn_id, channel, state, content_json, created_at, updated_at)
         VALUES ('55555555-5555-4555-8555-555555555555', ?1, NULL, 'user', 'completed', '[]', ?2, ?2)",
    )
    .bind(SESSION)
    .bind(AT)
    .execute(&pool)
    .await
    .expect("user channel accepted");
    let foreign_key = sqlx::query(
        "INSERT INTO owned_first_submission
           (session_id, first_submission_id, user_message_id, first_request_id, current_request_id,
            actor_kind, actor_id, request_fingerprint, input_json, input_digest, config_json,
            stage, ever_dispatched, created_at, updated_at)
         VALUES ('99999999-9999-4999-8999-999999999999',
                 '66666666-6666-4666-8666-666666666666',
                 '77777777-7777-4777-8777-777777777777',
                 '88888888-8888-4888-8888-888888888888',
                 '88888888-8888-4888-8888-888888888888',
                 'device', 'x', 'y', '[]', 'z', '{}',
                 'saved_not_dispatched', 0, ?1, ?1)",
    )
    .bind(AT)
    .execute(&pool)
    .await;
    assert!(
        foreign_key.is_err(),
        "首条行必须引用存在的会话（ON DELETE RESTRICT）"
    );
    let before_meta = meta_rows(&pool).await;
    pool.close().await;

    // 第二次打开：没有任何 DDL/行写入。
    for _ in 0..2 {
        let store = SqliteStore::open(StorageConfig::new(&dir), &at())
            .await
            .expect("reopen");
        store.close().await;
    }
    let pool = raw_pool(&path).await;
    assert_eq!(meta_rows(&pool).await, before_meta);
    let after_epoch: String =
        sqlx::query_scalar("SELECT value FROM meta WHERE key = 'server_epoch'")
            .fetch_one(&pool)
            .await
            .expect("epoch");
    assert_eq!(after_epoch, epoch);
    let messages: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM owned_message")
        .fetch_one(&pool)
        .await
        .expect("messages");
    assert_eq!(messages, 2);
    pool.close().await;
}

#[tokio::test]
async fn failed_v16_upgrade_rolls_back_the_whole_transaction() {
    let dir = temp_dir("upgrade-v16-rollback");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("fresh");
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    sqlx::raw_sql(&format!(
        "INSERT INTO owned_session
           (session_id, title, agent_id, agent_name, state, origin_epoch, version, created_at, updated_at)
         VALUES ('{SESSION}', 'old', 'agent', 'Agent', 'idle', '{EPOCH}', 1, '{AT}', '{AT}');
         INSERT INTO owned_message
           (message_id, session_id, turn_id, channel, state, content_json, created_at, updated_at)
         VALUES ('{MESSAGE}', '{SESSION}', NULL, 'reply', 'completed', '[]', '{AT}', '{AT}');"
    ))
    .execute(&pool)
    .await
    .expect("seed");
    downgrade_to_v15(&pool, false).await;
    // 让 v16 的重建在 DDL 阶段失败：冲突的中间表名已存在。
    sqlx::query("CREATE TABLE owned_message_v16_new (x INTEGER)")
        .execute(&pool)
        .await
        .expect("conflict table");
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&pool)
        .await
        .expect("version");
    assert_eq!(version, 15);
    pool.close().await;

    let error = SqliteStore::open(StorageConfig::new(&dir), &at()).await;
    assert!(error.is_err(), "v16 升级的 DDL 失败必须让整段升级失败");
    let pool = raw_pool(&path).await;
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&pool)
        .await
        .expect("version");
    assert_eq!(version, 15, "失败升级不得推进版本");
    let table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'owned_first_submission'",
    )
    .fetch_one(&pool)
    .await
    .expect("table");
    assert_eq!(table, 0, "失败升级不得留下新表");
    let channel: String = sqlx::query_scalar("SELECT channel FROM owned_message")
        .fetch_one(&pool)
        .await
        .expect("row");
    assert_eq!(channel, "reply", "回滚后行保持原样");
    let old_table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'owned_message_v15'",
    )
    .fetch_one(&pool)
    .await
    .expect("temp table");
    assert_eq!(old_table, 0, "升级失败不留下临时表");
    pool.close().await;
}
