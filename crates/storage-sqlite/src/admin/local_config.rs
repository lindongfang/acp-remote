//! `LocalConfigStore` 的 SQLite 实现（目录记录）。
//!
//! 权威是 `docs/CORE_PORTS_AND_STORAGE.md` §11.6 与 `CONFIG_REFERENCE.md` 的「配置与管理状态的权威」。
//! Agent 声明来自启动配置的 `[agent]` 表，不在本存储内；Provider 凭据由各 Agent 自己持有。

use async_trait::async_trait;
use sqlx::sqlite::SqliteRow;

use acp_core::model::{DirectoryId, DirectoryRecord, PortError};
use acp_core::ports::LocalConfigStore;

use crate::error::StorageError;
use crate::session_store::{Db, SqliteStore, decode, text};

/// `owned_directory` 的读列。
const DIRECTORY_COLUMNS: &str = "directory_id, canonical_path, created_at, last_used_at";

fn directory_from_row(row: &SqliteRow) -> Result<DirectoryRecord, StorageError> {
    DirectoryRecord::try_new(
        decode(&text(row, "directory_id")?, "owned_directory.directory_id")?,
        &text(row, "canonical_path")?,
        decode(&text(row, "created_at")?, "owned_directory.created_at")?,
        decode(&text(row, "last_used_at")?, "owned_directory.last_used_at")?,
    )
    .map_err(StorageError::from)
}

#[async_trait]
impl LocalConfigStore for SqliteStore {
    async fn directories(&self) -> Result<Vec<DirectoryRecord>, PortError> {
        let sql =
            format!("SELECT {DIRECTORY_COLUMNS} FROM owned_directory ORDER BY canonical_path ASC");
        let rows = sqlx::query(&sql).fetch_all(&self.pools().read).await.db()?;
        let mut records = Vec::with_capacity(rows.len());
        for row in &rows {
            records.push(directory_from_row(row)?);
        }
        Ok(records)
    }

    async fn directory(
        &self,
        directory_id: &DirectoryId,
    ) -> Result<Option<DirectoryRecord>, PortError> {
        let sql =
            format!("SELECT {DIRECTORY_COLUMNS} FROM owned_directory WHERE directory_id = ?1");
        let row = sqlx::query(&sql)
            .bind(directory_id.as_str())
            .fetch_optional(&self.pools().read)
            .await
            .db()?;
        Ok(row.as_ref().map(directory_from_row).transpose()?)
    }
}
