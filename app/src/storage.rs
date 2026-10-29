#[cfg(test)]
use std::sync::Arc;
use std::{path::Path, time::Duration};

use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};

use crate::error::StorageError;

pub const SCHEMA_VERSION: i64 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeMeta {
    pub schema_version: i64,
    pub node_id: String,
    pub public_key: Vec<u8>,
    pub key_file_ref: String,
}

pub struct Storage {
    pool: SqlitePool,
    #[cfg(test)]
    close_barrier: std::sync::Mutex<Option<Arc<crate::test_support::AsyncBarrier>>>,
}

impl Storage {
    /// Opens the SQLite database and initializes the supported schema.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot be opened, contains an unknown schema, or the
    /// schema transaction cannot be completed. An initialization failure closes the connection
    /// pool before returning.
    pub async fn open(path: &Path) -> Result<Self, StorageError> {
        let existed = path.exists();
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Full)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        let storage = Self {
            pool,
            #[cfg(test)]
            close_barrier: std::sync::Mutex::new(None),
        };
        if let Err(error) = storage.initialize_schema(existed).await {
            storage.pool.close().await;
            return Err(error);
        }
        Ok(storage)
    }

    async fn initialize_schema(&self, existed: bool) -> Result<(), StorageError> {
        let current_version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&self.pool)
            .await?;
        if current_version != 0 && current_version != SCHEMA_VERSION {
            return Err(StorageError::UnknownSchema(current_version));
        }
        if current_version == 0 && existed {
            let table_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_schema \
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            )
            .fetch_one(&self.pool)
            .await?;
            if table_count != 0 {
                return Err(StorageError::UnrecognizedSchema);
            }
        }

        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS node_meta (
                singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                schema_version INTEGER NOT NULL,
                node_id TEXT NOT NULL,
                public_key BLOB NOT NULL,
                key_file_ref TEXT NOT NULL
            )",
        )
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS clients (
                client_id TEXT PRIMARY KEY,
                public_key BLOB NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                status TEXT NOT NULL CHECK (status IN ('trusted', 'revoked')),
                auth_generation INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                revoked_at INTEGER
            )",
        )
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS management_commands (
                actor_id TEXT NOT NULL,
                request_id TEXT NOT NULL,
                action TEXT NOT NULL,
                payload_digest TEXT NOT NULL,
                result_json TEXT NOT NULL,
                expires_at INTEGER NOT NULL,
                PRIMARY KEY (actor_id, request_id)
            )",
        )
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS security_audit (
                audit_id INTEGER PRIMARY KEY AUTOINCREMENT,
                actor_id TEXT,
                action TEXT NOT NULL,
                target_id TEXT,
                outcome TEXT NOT NULL,
                occurred_at INTEGER NOT NULL
            )",
        )
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS node_state (
                singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                revision INTEGER NOT NULL
            )",
        )
        .execute(&mut *transaction)
        .await?;
        sqlx::query("INSERT OR IGNORE INTO node_state(singleton, revision) VALUES (1, 0)")
            .execute(&mut *transaction)
            .await?;
        sqlx::query("PRAGMA user_version = 1")
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Loads the persisted node identity metadata, if present.
    ///
    /// # Errors
    ///
    /// Returns an error when the metadata query fails.
    pub async fn load_node_meta(&self) -> Result<Option<NodeMeta>, StorageError> {
        let row = sqlx::query(
            "SELECT schema_version, node_id, public_key, key_file_ref \
             FROM node_meta WHERE singleton = 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| NodeMeta {
            schema_version: row.get("schema_version"),
            node_id: row.get("node_id"),
            public_key: row.get("public_key"),
            key_file_ref: row.get("key_file_ref"),
        }))
    }

    /// Atomically writes the node identity metadata singleton.
    ///
    /// # Errors
    ///
    /// Returns an error when the metadata transaction cannot be committed.
    pub async fn write_node_meta(&self, meta: &NodeMeta) -> Result<(), StorageError> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO node_meta(singleton, schema_version, node_id, public_key, key_file_ref) \
             VALUES (1, ?, ?, ?, ?) \
             ON CONFLICT(singleton) DO UPDATE SET \
                schema_version = excluded.schema_version, \
                node_id = excluded.node_id, \
                public_key = excluded.public_key, \
                key_file_ref = excluded.key_file_ref",
        )
        .bind(meta.schema_version)
        .bind(&meta.node_id)
        .bind(&meta.public_key)
        .bind(&meta.key_file_ref)
        .execute(&mut *transaction)
        .await?;
        #[cfg(test)]
        crate::test_support::crash_at("identity-before-meta-commit");
        transaction.commit().await?;
        #[cfg(test)]
        crate::test_support::crash_at("identity-after-meta-commit");
        Ok(())
    }

    pub(crate) async fn has_identity_history(&self) -> Result<bool, StorageError> {
        let present: i64 = sqlx::query_scalar(
            "SELECT CASE WHEN
                EXISTS(SELECT 1 FROM clients)
                OR EXISTS(SELECT 1 FROM management_commands)
                OR EXISTS(SELECT 1 FROM security_audit)
             THEN 1 ELSE 0 END",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(present != 0)
    }

    /// Reads the current device-list revision.
    ///
    /// # Errors
    ///
    /// Returns an error when the revision row cannot be read.
    pub async fn list_revision(&self) -> Result<i64, StorageError> {
        sqlx::query_scalar("SELECT revision FROM node_state WHERE singleton = 1")
            .fetch_one(&self.pool)
            .await
            .map_err(StorageError::from)
    }

    pub async fn close(&self) {
        #[cfg(test)]
        {
            let barrier = self
                .close_barrier
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            if let Some(barrier) = barrier {
                barrier.pause().await;
            }
        }
        self.pool.close().await;
    }

    #[cfg(test)]
    pub(crate) fn pause_next_close(&self) -> Arc<crate::test_support::AsyncBarrier> {
        let barrier = Arc::new(crate::test_support::AsyncBarrier::default());
        let mut slot = self
            .close_barrier
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(slot.is_none(), "only one close barrier can be installed");
        *slot = Some(Arc::clone(&barrier));
        barrier
    }

    #[must_use]
    pub(crate) fn trust_store(&self) -> crate::trust::TrustStore {
        crate::trust::TrustStore::new(self.pool.clone())
    }

    #[cfg(test)]
    pub(crate) const fn pool_for_test(&self) -> &SqlitePool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn test_database() -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "acp-remote-storage-{}-{nonce}.sqlite3",
            std::process::id()
        ))
    }

    #[tokio::test]
    async fn initializes_and_persists_node_metadata() {
        let path = test_database();
        let storage = Storage::open(&path).await.expect("storage");
        assert_eq!(storage.load_node_meta().await.expect("meta"), None);
        let meta = NodeMeta {
            schema_version: SCHEMA_VERSION,
            node_id: "a".repeat(64),
            public_key: vec![4; 65],
            key_file_ref: "identity.key".to_owned(),
        };
        storage.write_node_meta(&meta).await.expect("write meta");
        assert_eq!(
            storage.load_node_meta().await.expect("read meta"),
            Some(meta)
        );
        storage.close().await;
        std::fs::remove_file(path).expect("cleanup");
    }

    #[tokio::test]
    async fn rejects_an_unknown_schema_without_modifying_data() {
        let path = test_database();
        let storage = Storage::open(&path).await.expect("storage");
        sqlx::query("PRAGMA user_version = 77")
            .execute(&storage.pool)
            .await
            .expect("set schema");
        sqlx::query("CREATE TABLE sentinel(value TEXT NOT NULL)")
            .execute(&storage.pool)
            .await
            .expect("create sentinel");
        sqlx::query("INSERT INTO sentinel(value) VALUES ('preserve-me')")
            .execute(&storage.pool)
            .await
            .expect("insert sentinel");
        storage.close().await;

        let error = Storage::open(&path).await.err().expect("unknown schema");
        assert!(matches!(error, StorageError::UnknownSchema(77)));

        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(false);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("inspect preserved database");
        let value: String = sqlx::query_scalar("SELECT value FROM sentinel")
            .fetch_one(&pool)
            .await
            .expect("read sentinel");
        assert_eq!(value, "preserve-me");
        pool.close().await;
        std::fs::remove_file(path).expect("cleanup");
    }
}
