//! Development-fixture ownership of the original SQLite lifecycle; no replacement roots.
use crate::{
    error::StorageError,
    storage::{Storage, StorageLifecycleHost, StorageOpenOwner, StorageOpenRoots},
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::{path::Path, sync::Arc};

#[derive(Clone)]
pub struct FixtureStorage(Arc<FixtureStorageOwner>);
struct FixtureStorageOwner {
    storage: Arc<Storage>,
    owner: StorageOpenOwner,
}
impl std::ops::Deref for FixtureStorage {
    type Target = Storage;
    fn deref(&self) -> &Storage {
        &self.0.storage
    }
}
impl Drop for FixtureStorageOwner {
    fn drop(&mut self) {
        self.owner.request_stop();
    }
}

pub struct SqliteFixtureScope {
    owner: StorageOpenOwner,
    host: Arc<StorageLifecycleHost>,
}
impl SqliteFixtureScope {
    pub(crate) fn prepare<T: Send + Sync + 'static>(database: &Path, roots: Arc<T>) -> Self {
        let owner = StorageOpenOwner::prepare(
            &tokio::runtime::Handle::current(),
            StorageOpenRoots::retained(database, roots),
        )
        .expect("original fixture SQLite owner");
        let host = StorageLifecycleHost::install(owner.clone());
        Self { owner, host }
    }
    pub(crate) async fn open(&self) -> Result<FixtureStorage, Arc<StorageError>> {
        self.owner
            .start()
            .expect("original fixture driver admission")
            .ready()
            .await
            .map(|storage| {
                FixtureStorage(Arc::new(FixtureStorageOwner {
                    storage,
                    owner: self.owner.clone(),
                }))
            })
    }
    pub(crate) async fn open_with(
        &self,
        options: SqlitePoolOptions,
    ) -> Result<FixtureStorage, Arc<StorageError>> {
        self.owner
            .start_with_pool_options(options)
            .expect("original fixture driver admission")
            .ready()
            .await
            .map(|storage| {
                FixtureStorage(Arc::new(FixtureStorageOwner {
                    storage,
                    owner: self.owner.clone(),
                }))
            })
    }
    pub(crate) async fn open_catalog(
        &self,
        fault: Option<crate::session_storage::schema::MigrationFault>,
        namespace: Option<crate::session_storage::schema9_namespace_test_hook::NamespaceScenario>,
    ) -> Result<FixtureStorage, Arc<StorageError>> {
        self.owner
            .start_with(
                SqlitePoolOptions::new().max_connections(1),
                false,
                fault,
                namespace,
            )
            .expect("original catalog fixture admission")
            .ready()
            .await
            .map(|storage| {
                FixtureStorage(Arc::new(FixtureStorageOwner {
                    storage,
                    owner: self.owner.clone(),
                }))
            })
    }
    pub(crate) async fn preflight(&self) -> Result<(), Arc<StorageError>> {
        self.owner
            .start_preflight()
            .expect("original fixture preflight admission")
            .preflight_ready()
            .await
    }
    pub(crate) async fn start_raw(&self) {
        self.owner
            .start_worker_scope()
            .expect("original raw fixture admission")
            .preflight_ready()
            .await
            .expect("original raw fixture ready");
    }
    pub(crate) fn options(&self) -> SqliteConnectOptions {
        self.owner
            .connection_options()
            .expect("original fixture scope options")
    }
    pub(crate) async fn close_raw_pool(pool: &sqlx::SqlitePool) {
        loop {
            pool.close().await;
            if pool.size() == 0 {
                return;
            }
            // A return begun before close can publish an idle connection after the last drain.
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
    pub(crate) async fn stop_and_observe(&self) -> bool {
        self.owner.request_stop();
        self.host.observe_actual().await
    }
}

#[cfg(test)]
mod tests {
    use super::SqliteFixtureScope;
    use std::{
        fs,
        future::{Future, poll_fn},
        os::windows::fs::OpenOptionsExt,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        task::Poll,
        time::Duration,
    };
    use tokio::sync::Notify;

    #[tokio::test]
    async fn raw_pool_close_drains_connection_returning_after_original_close_starts() {
        let directory = std::env::temp_dir().join(format!(
            "acp-fixture-raw-close-{}-{}",
            std::process::id(),
            crate::random::hex(16).unwrap()
        ));
        fs::create_dir(&directory).unwrap();
        let roots = Arc::new(directory);
        let path = roots.join("node.sqlite3");
        let scope = SqliteFixtureScope::prepare(&path, roots.clone());
        scope.start_raw().await;
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let armed = Arc::new(AtomicBool::new(false));
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .after_release({
                let entered = entered.clone();
                let release = release.clone();
                let armed = armed.clone();
                move |_, _| {
                    let entered = entered.clone();
                    let release = release.clone();
                    let armed = armed.clone();
                    Box::pin(async move {
                        if armed.swap(false, Ordering::SeqCst) {
                            entered.notify_one();
                            release.notified().await;
                        }
                        Ok(true)
                    })
                }
            })
            .connect_with(scope.options().create_if_missing(true))
            .await
            .unwrap();
        let mut connection = pool.acquire().await.unwrap();
        armed.store(true, Ordering::SeqCst);
        let returning = tokio::spawn(async move {
            connection.return_to_pool().await;
        });
        entered.notified().await;
        {
            let closing = SqliteFixtureScope::close_raw_pool(&pool);
            tokio::pin!(closing);
            let first_pending =
                poll_fn(|cx| Poll::Ready(closing.as_mut().poll(cx).is_pending())).await;
            assert!(
                first_pending,
                "original close must wait for the held real return"
            );
            assert!(pool.is_closed());
            release.notify_one();
            tokio::time::timeout(Duration::from_secs(5), &mut closing)
                .await
                .unwrap();
        }
        returning.await.unwrap();
        let first_size = pool.size();
        let first_exclusive = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&path)
            .map(drop);
        // Always finish the owned failed fixture before reporting the captured first result.
        while pool.size() != 0 {
            pool.close().await;
            tokio::task::yield_now().await;
        }
        assert!(scope.stop_and_observe().await);
        assert_eq!(
            first_size, 0,
            "original raw pool still retained a late idle connection"
        );
        first_exclusive.expect("first physical exclusive probe after raw pool closure");
        drop(pool);
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let file = roots.join(format!("node.sqlite3{suffix}"));
            if file.exists() {
                fs::remove_file(file).unwrap();
            }
        }
        fs::remove_dir(roots.as_path()).unwrap();
    }
}
