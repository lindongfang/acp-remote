use super::Storage;
use sqlx::{Executor, SqlitePool, sqlite::SqlitePoolOptions};
use std::{
    fs,
    future::Future,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::Poll,
    time::Duration,
};
use tokio::sync::Notify;

struct OwnedDatabase {
    parent: PathBuf,
    directory: PathBuf,
    marker: String,
}
impl OwnedDatabase {
    fn create() -> Self {
        let parent = std::env::temp_dir().canonicalize().expect("test parent");
        let marker = crate::random::hex(16).expect("owned test nonce");
        let directory = parent.join(format!("acp-pool-close-{marker}"));
        fs::create_dir(&directory).expect("unique new test child");
        fs::write(directory.join("owner"), &marker).expect("owned marker");
        Self {
            parent,
            directory,
            marker,
        }
    }
    fn database(&self) -> PathBuf {
        self.directory.join("owned.sqlite3")
    }
    fn snapshot(&self, pool: &SqlitePool) -> Snapshot {
        let mut exclusive_errors = Vec::new();
        for suffix in ["", "-wal", "-shm"] {
            let file = self.directory.join(format!("owned.sqlite3{suffix}"));
            if !suffix.is_empty() && !file.exists() {
                continue;
            }
            if let Err(error) = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(0)
                .open(file)
            {
                exclusive_errors.push((suffix, error.raw_os_error()));
            }
        }
        Snapshot {
            size: pool.size(),
            exclusive_errors,
        }
    }
    fn cleanup(&self) {
        let actual = self.directory.canonicalize().expect("owned child remains");
        assert_eq!(actual, self.directory);
        assert_eq!(actual.parent(), Some(self.parent.as_path()));
        assert_eq!(
            fs::read_to_string(actual.join("owner")).expect("marker"),
            self.marker
        );
        assert_eq!(
            fs::symlink_metadata(&actual)
                .expect("child metadata")
                .file_attributes()
                & 0x400,
            0
        );
        for name in [
            "owned.sqlite3",
            "owned.sqlite3-wal",
            "owned.sqlite3-shm",
            "owner",
        ] {
            let file = actual.join(name);
            if file.exists() {
                let metadata = fs::symlink_metadata(&file).expect("owned file metadata");
                assert!(metadata.is_file());
                assert_eq!(metadata.file_attributes() & 0x400, 0);
                fs::remove_file(file).expect("remove only closed owned file");
            }
        }
        fs::remove_dir(actual).expect("remove only empty verified owned child");
    }
}
fn prepare_storage(
    area: &Arc<OwnedDatabase>,
    options: SqlitePoolOptions,
) -> (
    Arc<crate::storage::StorageLifecycleHost>,
    crate::storage::StorageOpenTicket,
) {
    use crate::storage::{StorageLifecycleHost, StorageOpenOwner, StorageOpenRoots};
    let owner = StorageOpenOwner::prepare(
        &tokio::runtime::Handle::current(),
        StorageOpenRoots::retained(&area.database(), Arc::clone(area)),
    )
    .expect("original fixture directory and marker roots");
    let host = StorageLifecycleHost::install(owner.clone());
    let ticket = owner
        .start_with_pool_options(options)
        .expect("start original fixture storage after parent installation");
    (host, ticket)
}

#[derive(Debug)]
struct Snapshot {
    size: u32,
    exclusive_errors: Vec<(&'static str, Option<i32>)>,
}
impl Snapshot {
    fn is_closed(&self) -> bool {
        self.size == 0 && self.exclusive_errors.is_empty()
    }
}
struct ReleaseOnDrop(Arc<Notify>);
impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        self.0.notify_one();
    }
}

#[tokio::test]
async fn storage_close_drains_late_return_while_original_pool_clones_remain() {
    let area = Arc::new(OwnedDatabase::create());
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let armed = Arc::new(AtomicBool::new(false));
    let options = SqlitePoolOptions::new().max_connections(1).after_release({
        let entered = Arc::clone(&entered);
        let release = Arc::clone(&release);
        let armed = Arc::clone(&armed);
        move |_, _| {
            let entered = Arc::clone(&entered);
            let release = Arc::clone(&release);
            let pause = armed.swap(false, Ordering::SeqCst);
            Box::pin(async move {
                if pause {
                    entered.notify_one();
                    release.notified().await;
                }
                Ok(true)
            })
        }
    });
    let (host, ticket) = prepare_storage(&area, options);
    let storage = ticket
        .ready()
        .await
        .expect("actual initialized Storage")
        .projection(host);
    let observer = storage.pool_for_test().clone();
    let mut connection = observer.acquire().await.expect("same real pool connection");
    connection
        .execute("SELECT revision FROM node_state WHERE singleton=1")
        .await
        .expect("real SQL before return");
    let release_on_exit = ReleaseOnDrop(Arc::clone(&release));
    armed.store(true, Ordering::SeqCst);
    drop(connection);
    let close_result = async {
        tokio::time::timeout(Duration::from_secs(2), entered.notified()).await.map_err(|_| "after_release not reached")?;
        {
            let marked = observer.close_event();
            tokio::pin!(marked);
            let close = storage.close();
            tokio::pin!(close);
            tokio::time::timeout(Duration::from_secs(2), async {
                tokio::select! {
                    biased;
                    () = &mut close => return Err("Storage close returned while the real lease was paused"),
                    () = &mut marked => {},
                }
                if !observer.is_closed() { return Err("close event did not mark the same pool closed"); }
                let pending = std::future::poll_fn(|cx| Poll::Ready(close.as_mut().poll(cx).is_pending())).await;
                if !pending { return Err("Storage close was not pending at the return barrier"); }
                release.notify_one();
                close.await;
                Ok(())
            }).await.map_err(|_| "actual Storage close timed out")??;
        }
        Ok::<_, &'static str>(area.snapshot(&observer))
    }.await;
    // The assertion observes the first actual Storage call before this failure-cleanup drain.
    release.notify_one();
    let cleaned = tokio::time::timeout(Duration::from_secs(2), async {
        observer.close().await;
        observer.close().await;
    })
    .await;
    drop(release_on_exit);
    cleaned.expect("cleanup of this test's pool completed");
    area.cleanup();
    let snapshot = close_result.expect("real Storage close stage reached");
    assert!(
        snapshot.is_closed(),
        "first Storage.close left real SQLite files held: {snapshot:?}"
    );
}

#[tokio::test]
async fn storage_normal_return_and_repeated_close_physically_release_files() {
    let area = Arc::new(OwnedDatabase::create());
    let (host, ticket) = prepare_storage(&area, SqlitePoolOptions::new().max_connections(1));
    let storage = ticket
        .ready()
        .await
        .expect("normal Storage")
        .projection(host);
    let observer = storage.pool_for_test().clone();
    let mut connection = observer.acquire().await.expect("same normal pool");
    connection
        .execute("SELECT revision FROM node_state WHERE singleton=1")
        .await
        .expect("real SQL");
    connection.return_to_pool().await;
    drop(connection);
    tokio::time::timeout(Duration::from_secs(2), storage.close())
        .await
        .expect("normal first close");
    let first = area.snapshot(&observer);
    tokio::time::timeout(Duration::from_secs(2), storage.close())
        .await
        .expect("repeat completed close");
    let repeated = area.snapshot(&observer);
    observer.close().await;
    area.cleanup();
    assert!(first.is_closed(), "normal close: {first:?}");
    assert!(repeated.is_closed(), "repeat close: {repeated:?}");
}

#[cfg(windows)]
mod size_fence_tests;

#[cfg(windows)]
mod independent_size_fence_tests;

#[cfg(windows)]
mod owned_acquire_tests;
