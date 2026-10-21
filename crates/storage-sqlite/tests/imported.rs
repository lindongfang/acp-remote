//! `access-owner-data-stateless` 的存储白名单：Access 不再保存任何 Owner 远程资源、命令状态或交付进度。
//!
//! 本文件替换原先的 `imported_*` 家族测试，断言三件事：
//!
//! 1. **新库只有 owned 家族 + meta**：`sqlite_master` 里没有任何 `imported_` 表，`meta` 里也没有
//!    `imported_schema_version`；文件格式版本是当前版本（§7.2）；
//! 2. **旧库升级后同样干净**：v1 夹具里的 owned 会话/事件/审计/幂等逐项保留，远程表被删除；
//! 3. **启动清理门禁可重试且幂等**：首次执行 `VACUUM` 并写完成标记，成功后再次执行不重写；标记缺失
//!    （模拟清理过程中断）时下一次启动会重新完成清理，owned 行不变。

mod support;

use acp_core::model::Timestamp;
use acp_core::ports::SessionStore;
use storage_sqlite::migrate::{
    FILE_FORMAT_VERSION, META_STARTUP_CLEANUP_VERSION, OWNED_SCHEMA_VERSION, StorageConfig,
};
use storage_sqlite::session_store::SqliteStore;
use support::*;

fn at() -> Timestamp {
    Timestamp::new(AT).expect("timestamp")
}

/// 当前封闭白名单里的表：owned 家族 + `meta`（连接/信任资料在 `owned_node`/`owned_device`/
/// `owned_pairing*`/`owned_peer_key` 里）。
const EXPECTED_TABLES: &[&str] = &[
    "meta",
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
    "owned_message",
    "owned_media_ref",
    "owned_node",
    "owned_pairing",
    "owned_pairing_peer",
    "owned_peer_key",
    "owned_session",
    "owned_turn",
    "owned_workspace",
];

async fn tables(pool: &sqlx::SqlitePool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .expect("table names")
}

async fn assert_whitelist(pool: &sqlx::SqlitePool) {
    let names = tables(pool).await;
    assert!(
        !names.iter().any(|name| name.starts_with("imported_")),
        "闭表白名单不得包含 imported 表：{names:?}"
    );
    for name in &names {
        assert!(
            EXPECTED_TABLES.contains(&name.as_str()),
            "出现白名单之外的表 {name}"
        );
    }
    let meta = meta_rows(pool).await;
    assert!(
        !meta.iter().any(|(key, _)| key == "imported_schema_version"),
        "meta 不得保留 imported_schema_version"
    );
}

#[tokio::test]
async fn a_fresh_database_contains_only_the_closed_whitelist() {
    let dir = temp_dir("stateless-fresh-whitelist");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("open fresh store");
    assert_eq!(store.metadata().owned_schema_version, OWNED_SCHEMA_VERSION);
    store.close().await;

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "PRAGMA user_version").await,
        FILE_FORMAT_VERSION
    );
    assert_whitelist(&pool).await;
    let meta = meta_rows(&pool).await;
    assert_eq!(
        meta.iter()
            .find(|(key, _)| key == "owned_schema_version")
            .map(|(_, value)| value.as_str()),
        Some(OWNED_SCHEMA_VERSION.to_string().as_str()),
        "owned_schema_version 必须写成当前版本"
    );
    pool.close().await;
}

#[tokio::test]
async fn an_upgraded_database_keeps_owned_rows_and_drops_every_remote_table() {
    let dir = temp_dir("stateless-upgrade-whitelist");
    copy_fixture("from-v1.sqlite3", &dir);

    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("upgrade v1 fixture");
    // v1 夹具的三条事件必须仍可重放（owned 事件/序号逐条保留）。
    let view = store.read_view().await.expect("read view");
    let head = view.head().await.expect("head");
    assert_eq!(
        head.global_sequence,
        acp_core::model::Sequence::new(3).expect("sequence"),
        "升级不得重新编号 owned 事件"
    );
    drop(view);
    store.close().await;

    let pool = raw_write_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_whitelist(&pool).await;
    assert_eq!(
        scalar_i64(&pool, "PRAGMA user_version").await,
        FILE_FORMAT_VERSION
    );
    let audit_ids: Vec<i64> =
        sqlx::query_scalar("SELECT audit_id FROM owned_audit ORDER BY audit_id")
            .fetch_all(&pool)
            .await
            .expect("audit ids");
    assert_eq!(
        audit_ids,
        vec![1, 2, 5],
        "owned 审计行（含编号空洞）必须逐项保留"
    );
    let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM owned_session")
        .fetch_one(&pool)
        .await
        .expect("session count");
    assert_eq!(sessions, 1, "v1 夹具的 owned 会话必须保留");
    let idempotency: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM owned_command")
        .fetch_one(&pool)
        .await
        .expect("command count");
    assert_eq!(idempotency, 1, "owned 命令幂等行必须保留");
    pool.close().await;
}

#[tokio::test]
async fn the_startup_gate_rewrites_once_and_can_be_retried_after_a_crash() {
    let dir = temp_dir("stateless-startup-gate");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("open");
    assert!(
        store.run_startup_gate().await.expect("first gate"),
        "首次启动必须执行 checkpoint + 重写"
    );
    assert!(
        !store.run_startup_gate().await.expect("second gate"),
        "完成标记存在时不得重复重写"
    );
    store.close().await;

    let pool = raw_write_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM meta WHERE key = 'startup_cleanup_version'"
        )
        .await,
        1,
        "完成标记必须恰好一行"
    );
    let marker: String =
        sqlx::query_scalar("SELECT value FROM meta WHERE key = 'startup_cleanup_version'")
            .fetch_one(&pool)
            .await
            .expect("marker");
    assert_eq!(marker, FILE_FORMAT_VERSION.to_string());
    let schema_before = schema_sql(&pool).await;
    let rows_before = table_snapshot(&pool, "meta").await;
    // 模拟「重写完成、写标记前中断」：删掉完成标记后下一次启动必须重新执行并成功。
    sqlx::query("DELETE FROM meta WHERE key = 'startup_cleanup_version'")
        .execute(&pool)
        .await
        .expect("simulate interrupted gate");
    pool.close().await;

    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("reopen after interrupted gate");
    assert!(
        store.run_startup_gate().await.expect("retry gate"),
        "标记缺失时必须重新完成清理"
    );
    store.close().await;

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        schema_sql(&pool).await,
        schema_before,
        "重试只重写页，不改变结构"
    );
    let marker: String =
        sqlx::query_scalar("SELECT value FROM meta WHERE key = 'startup_cleanup_version'")
            .fetch_one(&pool)
            .await
            .expect("marker");
    assert_eq!(marker, FILE_FORMAT_VERSION.to_string());
    // 其余 meta 行逐字节不变（只有被删掉又重写的完成标记参与变化）。
    let meta_after = table_snapshot(&pool, "meta").await;
    let filtered_before: Vec<String> = rows_before
        .into_iter()
        .filter(|row| !row.contains(META_STARTUP_CLEANUP_VERSION))
        .collect();
    let filtered_after: Vec<String> = meta_after
        .into_iter()
        .filter(|row| !row.contains(META_STARTUP_CLEANUP_VERSION))
        .collect();
    assert_eq!(filtered_after, filtered_before, "owned/连接资料不变");
    assert_whitelist(&pool).await;
    pool.close().await;
}
