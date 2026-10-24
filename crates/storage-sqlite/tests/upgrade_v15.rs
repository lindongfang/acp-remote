mod support;

use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::*;

#[tokio::test]
async fn v14_upgrade_preserves_rows_and_v15_custom_titles_survive_reopen() {
    let dir = temp_dir("upgrade-v15");
    let at = acp_core::model::Timestamp::new(AT).expect("timestamp");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at)
        .await
        .expect("fresh");
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    let fresh_columns = column_names(&pool, "owned_session").await;
    let epoch: String = sqlx::query_scalar("SELECT value FROM meta WHERE key = 'server_epoch'")
        .fetch_one(&pool)
        .await
        .expect("epoch");
    sqlx::raw_sql(
        "ALTER TABLE owned_session DROP COLUMN custom_title;
         INSERT INTO owned_session
           (session_id, title, agent_id, agent_name, state, origin_epoch, version, created_at, updated_at)
         VALUES ('11111111-1111-4111-8111-111111111111', 'Agent title', 'agent', 'Agent', 'idle',
                 '22222222-2222-4222-8222-222222222222', 1,
                 '2026-09-18T00:00:00.000Z', '2026-09-18T00:00:00.000Z');
         PRAGMA user_version = 14;
         UPDATE meta SET value = '14' WHERE key = 'owned_schema_version';",
    ).execute(&pool).await.expect("seed v14");
    pool.close().await;

    let store = SqliteStore::open(StorageConfig::new(&dir), &at)
        .await
        .expect("upgrade");
    store.close().await;
    let pool = raw_write_pool(&path).await;
    assert_eq!(column_names(&pool, "owned_session").await, fresh_columns);
    let row: (String, Option<String>) =
        sqlx::query_as("SELECT title, custom_title FROM owned_session")
            .fetch_one(&pool)
            .await
            .expect("preserved row");
    assert_eq!(row, ("Agent title".to_owned(), None));
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&pool)
        .await
        .expect("version");
    assert_eq!(version, 17);
    sqlx::query("UPDATE owned_session SET custom_title = 'My title'")
        .execute(&pool)
        .await
        .expect("set custom title");
    let before = meta_rows(&pool).await;
    pool.close().await;

    for _ in 0..2 {
        let store = SqliteStore::open(StorageConfig::new(&dir), &at)
            .await
            .expect("reopen current");
        store.close().await;
    }
    let pool = raw_pool(&path).await;
    assert_eq!(meta_rows(&pool).await, before);
    let after_epoch: String =
        sqlx::query_scalar("SELECT value FROM meta WHERE key = 'server_epoch'")
            .fetch_one(&pool)
            .await
            .expect("epoch");
    assert_eq!(after_epoch, epoch);
    let row: (String, String) = sqlx::query_as("SELECT title, custom_title FROM owned_session")
        .fetch_one(&pool)
        .await
        .expect("preserved titles");
    assert_eq!(row, ("Agent title".to_owned(), "My title".to_owned()));
    pool.close().await;
}
