//! §9.1：migration 的幂等与「版本过新拒绝启动且不写入任何行」。

mod support;

use acp_core::model::Timestamp;
use storage_sqlite::error::StorageError;
use storage_sqlite::migrate::{FILE_FORMAT_VERSION, StorageConfig};
use storage_sqlite::session_store::SqliteStore;
use support::*;

const TABLES: [&str; 12] = [
    "meta",
    "owned_session",
    "owned_turn",
    "owned_event",
    "owned_command",
    "owned_interaction",
    "owned_audit",
    "owned_attachment",
    "owned_attachment_link",
    "imported_import",
    "imported_session",
    "imported_delivery_index",
];

fn at() -> Timestamp {
    Timestamp::new(AT).expect("timestamp")
}

fn still_writable(dir: &std::path::Path) -> StorageConfig {
    StorageConfig::new(dir)
}

/// 夹具库已经是 v1：启动**不得**改动 `sqlite_master`、`meta` 或 `user_version`，连续两次启动后逐字节相同。
#[tokio::test]
async fn v1_fixture_is_untouched_by_two_consecutive_starts() {
    let dir = temp_dir("migrate-idempotent");
    let path = copy_fixture("empty.sqlite3", &dir);

    // 启动前：夹具自身的形态就是判据基线。
    let pool = raw_pool(&path).await;
    let before_schema = schema_sql(&pool).await;
    let before_meta = meta_rows(&pool).await;
    let before_version = scalar_i64(&pool, "PRAGMA user_version").await;
    assert_eq!(before_version, FILE_FORMAT_VERSION);
    let server_epoch = before_meta
        .iter()
        .find(|(key, _)| key == "server_epoch")
        .map(|(_, value)| value.clone())
        .expect("fixture carries server_epoch");
    pool.close().await;

    for round in 1..=2 {
        let store = SqliteStore::open(still_writable(&dir), &at())
            .await
            .unwrap_or_else(|error| panic!("round {round} must open: {error}"));
        assert_eq!(store.metadata().owned_schema_version, 1);
        assert_eq!(store.metadata().imported_schema_version, 1);
        store.close().await;

        let pool = raw_pool(&path).await;
        assert_eq!(
            scalar_i64(&pool, "PRAGMA user_version").await,
            before_version,
            "round {round} changed user_version"
        );
        assert_eq!(
            schema_sql(&pool).await,
            before_schema,
            "round {round} rewrote sqlite_master"
        );
        assert_eq!(
            meta_rows(&pool).await,
            before_meta,
            "round {round} rewrote meta"
        );
        assert_eq!(
            meta_rows(&pool)
                .await
                .iter()
                .find(|(key, _)| key == "server_epoch")
                .map(|(_, value)| value.clone()),
            Some(server_epoch.clone()),
            "server_epoch must survive restarts unchanged"
        );
        pool.close().await;
    }
}

/// 空目录首次启动：建库并把 `user_version`/两族版本写到 v1。
#[tokio::test]
async fn fresh_directory_is_created_at_version_one() {
    let dir = temp_dir("migrate-fresh");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("create store");
    store.close().await;

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "PRAGMA user_version").await,
        FILE_FORMAT_VERSION
    );
    let meta = meta_rows(&pool).await;
    for key in [
        "server_epoch",
        "created_at",
        "last_prune_at",
        "owned_schema_version",
        "imported_schema_version",
    ] {
        assert!(
            meta.iter().any(|(name, _)| name == key),
            "meta.{key} must exist"
        );
    }
    assert_eq!(
        meta.iter()
            .find(|(key, _)| key == "owned_schema_version")
            .map(|(_, value)| value.as_str()),
        Some("1")
    );
    assert_eq!(
        meta.iter()
            .find(|(key, _)| key == "created_at")
            .map(|(_, value)| value.as_str()),
        Some(AT)
    );
    pool.close().await;
}

/// §9.1：`user_version = 2` 的库必须被具名拒绝，且一行都不写。
#[tokio::test]
async fn too_new_database_is_rejected_without_writing_rows() {
    let dir = temp_dir("migrate-too-new");
    let path = copy_fixture("too-new.sqlite3", &dir);

    let pool = raw_pool(&path).await;
    let before_schema = schema_sql(&pool).await;
    let before_meta = meta_rows(&pool).await;
    let mut before_rows = Vec::new();
    for table in TABLES {
        before_rows.push((table, table_snapshot(&pool, table).await));
    }
    pool.close().await;

    let error = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect_err("a newer file format must be rejected");
    match error {
        StorageError::FileFormatTooNew { found, supported } => {
            assert_eq!(found, 2);
            assert_eq!(supported, FILE_FORMAT_VERSION);
        }
        other => panic!("expected FileFormatTooNew, got {other}"),
    }

    let pool = raw_pool(&path).await;
    assert_eq!(scalar_i64(&pool, "PRAGMA user_version").await, 2);
    assert_eq!(schema_sql(&pool).await, before_schema);
    assert_eq!(meta_rows(&pool).await, before_meta);
    for (table, before) in before_rows {
        assert_eq!(
            table_snapshot(&pool, table).await,
            before,
            "{table} must be untouched after a rejected start"
        );
    }
    pool.close().await;
}

/// §8：损坏的数据库文件不得让 `open` panic。两种可接受结局：返回具名错误，或进入只读失败关闭
/// （`health().read_only == true`，写路径返回 `PortError::Corrupt`）；库若仍可读，`read_view()` 可用。
#[tokio::test]
async fn corrupt_database_fails_closed() {
    use acp_core::ports::{OwnedCommit, SessionStore};
    use storage_sqlite::session_store::SqliteStore;
    use support::*;

    let dir = temp_dir("migrate-corrupt");
    let path = copy_fixture("empty.sqlite3", &dir);
    // 把文件头（前 100 字节）写成与之无关的字节，破坏 SQLite 头（magic/page size）。
    let mut bytes = std::fs::read(&path).expect("read fixture");
    for (index, byte) in bytes.iter_mut().take(100).enumerate() {
        *byte = u8::try_from(index % 251).expect("byte");
    }
    std::fs::write(&path, &bytes).expect("write corrupt fixture");

    let opened = SqliteStore::open(StorageConfig::new(&dir), &at()).await;
    match opened {
        Err(error) => {
            // 具名错误（不是 panic）：文件格式/损坏/底层 sqlite 错误三者之一。
            assert!(
                matches!(
                    error,
                    StorageError::FileFormatTooNew { .. }
                        | StorageError::Corrupt(_)
                        | StorageError::Sql(_)
                ),
                "a corrupt file must fail with a named error, got {error}"
            );
        }
        Ok(store) => {
            let health = store.health().await.expect("health");
            assert!(
                health.read_only && !health.integrity_ok,
                "a partially readable corrupt file must enter read-only fail-closed mode"
            );
            let error = store
                .commit(OwnedCommit {
                    session: None,
                    at: at(),
                    expected_version: None,
                    state: None,
                    turns: Vec::new(),
                    events: Vec::new(),
                    interactions: Vec::new(),
                    compacted: Vec::new(),
                    idempotency: None,
                    command_terminal: None,
                    origin_epoch: None,
                })
                .await
                .expect_err("write paths must be refused in fail-closed mode");
            assert!(
                matches!(error, acp_core::model::PortError::Corrupt(_)),
                "expected PortError::Corrupt, got {error}"
            );
            // 只读查询继续（§8）。
            store.read_view().await.expect("read view must still work");
            store.close().await;
        }
    }
}
