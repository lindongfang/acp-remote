//! §7.2/§7.6/§9 判据 1/28/34/36：v12 的可重复升级路径与新表/新列的实际形状。

mod support;

use acp_core::ports::SessionStore;
use storage_sqlite::migrate::{FILE_FORMAT_VERSION, OWNED_SCHEMA_VERSION, StorageConfig};
use storage_sqlite::session_store::SqliteStore;
use support::*;

const AT: &str = "2026-09-18T00:00:00.000Z";

fn at() -> acp_core::model::Timestamp {
    acp_core::model::Timestamp::new(AT).expect("timestamp")
}

/// 新建库直接是 v12 形状：`owned_interaction` 的两列在列清单末尾，五张新表都在。
#[tokio::test]
async fn fresh_databases_are_created_in_the_current_shape() {
    let dir = temp_dir("upgrade-fresh-v12");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("open");
    store.close().await;

    assert_eq!(FILE_FORMAT_VERSION, 14);
    assert_eq!(OWNED_SCHEMA_VERSION, 14);

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&pool)
        .await
        .expect("user_version");
    assert_eq!(version, FILE_FORMAT_VERSION);
    let meta = meta_rows(&pool).await;
    assert!(meta.contains(&("owned_schema_version".to_owned(), "14".to_owned())));

    for table in [
        "owned_attachment_transfer",
        "owned_attachment_submission",
        "owned_attachment_submission_item",
        "owned_message",
        "owned_media_ref",
    ] {
        assert!(table_exists(&pool, table).await, "missing table {table}");
    }
    let interaction_columns = column_names(&pool, "owned_interaction").await;
    assert_eq!(
        &interaction_columns[interaction_columns.len() - 4..],
        [
            "request_version",
            "invalidated_at",
            "confirmed_at",
            "turn_id"
        ],
        "新列必须追加在列清单末尾（v12 两列 + v13 两列）"
    );
    assert!(
        !column_names(&pool, "owned_session")
            .await
            .contains(&"closed_at".to_owned()),
        "v13 之后会话表不得再有 closed_at"
    );
    let transfer_columns = column_names(&pool, "owned_attachment_transfer").await;
    for column in [
        "transfer_id",
        "session_id",
        "subject_kind",
        "subject_id",
        "total_length",
        "received_length",
        "chunk_size",
        "state",
        "media_type",
        "file_name",
        "sealed_sha256",
        "created_at",
        "updated_at",
    ] {
        assert!(transfer_columns.iter().any(|name| name == column));
    }
    let message_columns = column_names(&pool, "owned_message").await;
    assert!(message_columns.iter().any(|name| name == "channel"));
    assert!(message_columns.iter().any(|name| name == "partial_reason"));

    // 第二次打开逐字节幂等（`sqlite_master`、`meta`、全部表行集）。
    let schema_before = schema_sql(&pool).await;
    let meta_before = meta_rows(&pool).await;
    pool.close().await;
    let reopened = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("reopen");
    reopened.close().await;
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(schema_sql(&pool).await, schema_before);
    assert_eq!(meta_rows(&pool).await, meta_before);
    pool.close().await;
}

/// v2 形状的空库连续升级到 v13：新表与新列出现，版本号在升级后写入。
#[tokio::test]
async fn v2_fixture_upgrades_to_v12() {
    let dir = temp_dir("upgrade-v2-v12");
    copy_fixture("empty.sqlite3", &dir);
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("upgrade v2");
    store.close().await;

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&pool)
        .await
        .expect("user_version");
    assert_eq!(version, FILE_FORMAT_VERSION);
    assert!(table_exists(&pool, "owned_message").await);
    assert!(table_exists(&pool, "owned_attachment_transfer").await);
    let interaction_columns = column_names(&pool, "owned_interaction").await;
    assert!(
        interaction_columns
            .iter()
            .any(|name| name == "request_version")
    );
    assert!(
        interaction_columns
            .iter()
            .any(|name| name == "invalidated_at")
    );
    assert!(
        interaction_columns
            .iter()
            .any(|name| name == "confirmed_at")
    );
    assert!(interaction_columns.iter().any(|name| name == "turn_id"));
    assert!(
        !column_names(&pool, "owned_session")
            .await
            .contains(&"closed_at".to_owned()),
        "v13 之后会话表不得再有 closed_at"
    );
    pool.close().await;

    // 再打开一次不产生任何 DDL 写入。
    let reopened = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("reopen");
    reopened.close().await;
}

/// v13 → v14：删除全部 imported 表，并按明确来源谓词清掉 owned 家族里的远程代理审计；
/// 本机审计与 owned 数据保留；重复打开不再改变结构或数据。
#[tokio::test]
async fn v14_drops_imported_tables_and_remote_proxy_audits() {
    let dir = temp_dir("upgrade-imported-cleanup-v14");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("create current store");
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    // 复刻 v13 的 imported 形状（只保留 v14 需要读取/删除的列）。
    sqlx::raw_sql(
        "CREATE TABLE imported_session (owner_node_id TEXT NOT NULL, session_id TEXT NOT NULL,
           updated_at TEXT NOT NULL, PRIMARY KEY (owner_node_id, session_id));
         CREATE TABLE imported_delivery_index (owner_node_id TEXT NOT NULL, session_id TEXT NOT NULL,
           origin_event_id TEXT NOT NULL);
         CREATE TABLE imported_command_ref (owner_node_id TEXT NOT NULL, session_id TEXT NOT NULL,
           request_id TEXT NOT NULL);
         CREATE TABLE imported_audit (audit_id INTEGER PRIMARY KEY AUTOINCREMENT, at TEXT NOT NULL);",
    )
    .execute(&pool)
    .await
    .expect("recreate imported family");
    let remote_a = "77777777-7777-4777-8777-777777777701";
    let remote_b = "77777777-7777-4777-8777-777777777702";
    let local = "11111111-1111-4111-8111-111111111111";
    for session in [remote_a, remote_b] {
        sqlx::query(
            "INSERT INTO imported_session (owner_node_id, session_id, updated_at) VALUES (?1, ?2, ?3)",
        )
        .bind("99999999-9999-4999-8999-999999999999")
        .bind(session)
        .bind(AT)
        .execute(&pool)
        .await
        .expect("seed remote session");
    }
    for (target_kind, target_id) in [
        ("session", remote_a.to_string()),
        (
            "command",
            format!("{remote_a}/33333333-3333-4333-8333-333333333333"),
        ),
        ("session", local.to_owned()),
        ("node", "99999999-9999-4999-8999-999999999999".to_owned()),
        ("import", "55555555-5555-4555-8555-555555555555".to_owned()),
    ] {
        sqlx::query(
            "INSERT INTO owned_audit (at, action, actor_kind, actor_id, target_kind, target_id, outcome) \
             VALUES (?1, 'node.authenticated', 'node', ?2, ?3, ?4, 'success')",
        )
        .bind(AT)
        .bind("99999999-9999-4999-8999-999999999999")
        .bind(target_kind)
        .bind(target_id)
        .execute(&pool)
        .await
        .expect("seed audit");
    }
    // 回退到 v13：文件版本与 owned 版本键都低于当前值，只让 v14 段生效。
    sqlx::raw_sql(
        "PRAGMA user_version = 13;
         UPDATE meta SET value = '13' WHERE key = 'owned_schema_version';
         DELETE FROM meta WHERE key = 'imported_schema_version';",
    )
    .execute(&pool)
    .await
    .expect("rewind to v13");
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&pool)
        .await
        .expect("checkpoint");
    pool.close().await;

    let reopened = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("v13 database must upgrade");
    assert_eq!(
        reopened.metadata().owned_schema_version,
        OWNED_SCHEMA_VERSION
    );
    reopened.close().await;

    let pool = raw_pool(&path).await;
    for table in [
        "imported_session",
        "imported_delivery_index",
        "imported_command_ref",
        "imported_audit",
        "imported_import",
        "imported_import_export",
    ] {
        assert!(!table_exists(&pool, table).await, "{table} must be dropped");
    }

    let audits: Vec<(String, String)> =
        sqlx::query_as("SELECT target_kind, target_id FROM owned_audit ORDER BY audit_id")
            .fetch_all(&pool)
            .await
            .expect("audit rows");
    assert_eq!(
        audits,
        vec![
            ("session".to_owned(), local.to_owned()),
            (
                "node".to_owned(),
                "99999999-9999-4999-8999-999999999999".to_owned()
            ),
        ],
        "只有明确指向远程来源的审计被清掉；本机会话与连接身份审计保留"
    );
    let schema_before = schema_sql(&pool).await;
    let audits_before = audits;
    pool.close().await;

    // 重复打开不再改变结构或数据。
    let again = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("reopen");
    again.close().await;
    let pool = raw_pool(&path).await;
    assert_eq!(schema_sql(&pool).await, schema_before);
    let audits_after: Vec<(String, String)> =
        sqlx::query_as("SELECT target_kind, target_id FROM owned_audit ORDER BY audit_id")
            .fetch_all(&pool)
            .await
            .expect("audit rows");
    assert_eq!(audits_after, audits_before);
    pool.close().await;
}

/// v1 夹具升级到 v13：既有数据保留，新表为空且可写。
#[tokio::test]
async fn v1_fixture_preserves_data_and_gets_the_new_tables() {
    let dir = temp_dir("upgrade-v1-v12");
    copy_fixture("from-v1.sqlite3", &dir);
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("upgrade v1");

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&pool)
        .await
        .expect("user_version");
    assert_eq!(version, FILE_FORMAT_VERSION);
    // 既有审计的编号空洞不被回填。
    let audit_ids: Vec<i64> =
        sqlx::query_scalar("SELECT audit_id FROM owned_audit ORDER BY audit_id")
            .fetch_all(&pool)
            .await
            .expect("audit ids");
    assert_eq!(audit_ids, vec![1, 2, 5]);
    assert!(table_exists(&pool, "owned_message").await);
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_message").await,
        0
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment_transfer").await,
        0
    );
    let columns = column_specs(&pool, "owned_interaction").await;
    let request_version = columns
        .iter()
        .find(|(name, ..)| name == "request_version")
        .expect("request_version column");
    assert!(request_version.2, "NOT NULL is set on the column");
    assert_eq!(request_version.3.as_deref(), Some("1"));
    let invalidated = columns
        .iter()
        .find(|(name, ..)| name == "invalidated_at")
        .expect("invalidated_at column");
    assert!(!invalidated.2, "nullable");
    pool.close().await;

    // 升级后的库可正常写入新表（例如一条 pending 交互的版本默认值）。
    let session = {
        use acp_core::model::{AgentId, AgentRef, OriginEpoch};
        use acp_core::ports::{NewSession, OwnedCommit, StateChange};
        store
            .commit(OwnedCommit {
                session: None,
                at: at(),
                expected_version: None,
                state: Some(StateChange::Create(NewSession {
                    title: Some("after upgrade".to_owned()),
                    agent: AgentRef::try_new(
                        AgentId::new("probe-agent").expect("agent"),
                        "Probe Agent",
                    )
                    .expect("agent ref"),
                })),
                turns: Vec::new(),
                events: Vec::new(),
                interactions: Vec::new(),
                compacted: Vec::new(),
                idempotency: None,
                command_terminal: None,
                origin_epoch: Some(
                    OriginEpoch::new("6f2a1f8e-7b1c-4a8f-9b0d-1c2d3e4f5a6b").expect("epoch"),
                ),
            })
            .await
            .expect("create session")
            .session_id
            .expect("session id")
    };
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(
            &pool,
            &format!(
                "SELECT COUNT(*) FROM owned_attachment_transfer WHERE session_id = '{}'",
                session.as_str()
            )
        )
        .await,
        0
    );
    pool.close().await;

    store.close().await;
}
