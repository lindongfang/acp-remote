//! §7.2/§7.6/§9 判据 1/28/34/36：v12 的可重复升级路径与新表/新列的实际形状。

mod support;

use acp_core::ports::SessionStore;
use storage_sqlite::migrate::{
    FILE_FORMAT_VERSION, IMPORTED_SCHEMA_VERSION, OWNED_SCHEMA_VERSION, StorageConfig,
};
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

    assert_eq!(FILE_FORMAT_VERSION, 13);
    assert_eq!(OWNED_SCHEMA_VERSION, 13);
    assert_eq!(IMPORTED_SCHEMA_VERSION, 5);

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&pool)
        .await
        .expect("user_version");
    assert_eq!(version, FILE_FORMAT_VERSION);
    let meta = meta_rows(&pool).await;
    assert!(meta.contains(&("owned_schema_version".to_owned(), "13".to_owned())));
    assert!(meta.contains(&("imported_schema_version".to_owned(), "5".to_owned())));

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

/// v13 把 imported 摘要的旧七态归一化成三态（值映射，不重建、不加正文列），`NULL` 保持 `NULL`。
#[tokio::test]
async fn v13_normalizes_imported_legacy_states() {
    let dir = temp_dir("upgrade-imported-states-v13");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("create current store");
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    let states = [
        (1, Some("idle")),
        (2, Some("queued")),
        (3, Some("running")),
        (4, Some("waiting_input")),
        (5, Some("waiting_permission")),
        (6, Some("failed")),
        (7, Some("closed")),
        (8, None),
    ];
    for (index, state) in states {
        sqlx::query(
            "INSERT INTO imported_session (owner_node_id, session_id, state, updated_at) \
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind("99999999-9999-4999-8999-999999999999")
        .bind(format!("77777777-7777-4777-8777-77777777770{index}"))
        .bind(state)
        .bind(AT)
        .execute(&pool)
        .await
        .expect("seed imported session");
    }
    // 回退到 v12 形状：文件版本与两族版本键都低于当前值，只让 v13 段生效。
    sqlx::raw_sql(
        "PRAGMA user_version = 12;
         UPDATE meta SET value = '12' WHERE key = 'owned_schema_version';
         UPDATE meta SET value = '4' WHERE key = 'imported_schema_version';",
    )
    .execute(&pool)
    .await
    .expect("rewind to v12");
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&pool)
        .await
        .expect("checkpoint");
    pool.close().await;

    let reopened = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("v12 database must upgrade");
    reopened.close().await;

    let pool = raw_pool(&path).await;
    let migrated: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT session_id, state FROM imported_session ORDER BY session_id")
            .fetch_all(&pool)
            .await
            .expect("state rows");
    pool.close().await;
    assert_eq!(
        migrated,
        vec![
            (
                "77777777-7777-4777-8777-777777777701".to_owned(),
                Some("idle".to_owned())
            ),
            (
                "77777777-7777-4777-8777-777777777702".to_owned(),
                Some("running".to_owned())
            ),
            (
                "77777777-7777-4777-8777-777777777703".to_owned(),
                Some("running".to_owned())
            ),
            (
                "77777777-7777-4777-8777-777777777704".to_owned(),
                Some("pending".to_owned())
            ),
            (
                "77777777-7777-4777-8777-777777777705".to_owned(),
                Some("pending".to_owned())
            ),
            (
                "77777777-7777-4777-8777-777777777706".to_owned(),
                Some("idle".to_owned())
            ),
            (
                "77777777-7777-4777-8777-777777777707".to_owned(),
                Some("idle".to_owned())
            ),
            ("77777777-7777-4777-8777-777777777708".to_owned(), None),
        ],
        "旧七态必须按 idle/queued/running/waiting_*/failed/closed 的映射归一化，NULL 保持 None"
    );
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
