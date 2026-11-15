#![cfg_attr(not(test), allow(dead_code))]
//! Developer checks against the actual normal library and patched SQLite driver.
use acp_remote::{
    data_dir::DataDirectory,
    storage::{StorageLifecycleHost, StorageOpenOwner, StorageOpenRoots},
};
#[cfg(test)]
use std::path::PathBuf;
use std::{fs, os::windows::fs::OpenOptionsExt, path::Path, sync::Arc};
#[cfg(test)]
struct DevArea {
    path: PathBuf,
    cleanup: bool,
}
#[cfg(test)]
impl std::ops::Deref for DevArea {
    type Target = Path;
    fn deref(&self) -> &Self::Target {
        &self.path
    }
}
#[cfg(test)]
impl Drop for DevArea {
    fn drop(&mut self) {
        if self.cleanup {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
#[cfg(test)]
fn area(label: &str) -> DevArea {
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    if let Some(root) = std::env::var_os("CLDB_DEV_DATA").filter(|value| !value.is_empty()) {
        let path = PathBuf::from(root).join(label);
        assert!(!path.exists(), "never reuse a prior execution area");
        fs::create_dir_all(&path).unwrap();
        return DevArea {
            path,
            cleanup: false,
        };
    }
    let nonce = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let base = std::env::temp_dir()
        .canonicalize()
        .unwrap_or_else(|_| std::env::temp_dir());
    let path = base.join(format!(
        "acpr-sqlite-lifecycle-dev-{}-{stamp}-{nonce}-{label}",
        std::process::id()
    ));
    assert!(!path.exists(), "never reuse a prior execution area");
    fs::create_dir_all(&path).unwrap();
    DevArea {
        path,
        cleanup: true,
    }
}
fn exclusive(path: &Path) {
    let _file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(path)
        .expect("actual first share0 after complete close");
}
fn scope(dir: &Path) -> (StorageOpenOwner, Arc<StorageLifecycleHost>) {
    let directory = Arc::new(DataDirectory::acquire(dir).unwrap());
    let owner = StorageOpenOwner::prepare(
        &tokio::runtime::Handle::current(),
        StorageOpenRoots::directory(directory),
    )
    .unwrap();
    let host = StorageLifecycleHost::install(owner.clone());
    (owner, host)
}
#[tokio::test]
async fn dev_scoped_open_close_reopen_preflight() {
    let dir = area("open-close-preflight");
    let db = dir.join("node.sqlite3");
    let (owner, host) = scope(&dir);
    let storage = owner.start().unwrap().ready().await.unwrap();
    storage.close().await;
    assert!(host.observe_actual().await);
    exclusive(&db);
    drop(storage);
    let (second_owner, second_host) = scope(&dir);
    let second = second_owner.start().unwrap().ready().await.unwrap();
    second.close().await;
    assert!(second_host.observe_actual().await);
    exclusive(&db);
}
#[tokio::test]
async fn dev_original_directory_owner_retained_until_close() {
    let dir = area("directory-owner");
    let directory = Arc::new(DataDirectory::acquire(&dir).unwrap());
    let db = directory.database_path();
    let owner = StorageOpenOwner::prepare(
        &tokio::runtime::Handle::current(),
        StorageOpenRoots::directory(directory.clone()),
    )
    .unwrap();
    let host = StorageLifecycleHost::install(owner.clone());
    let storage = owner.start().unwrap().ready().await.unwrap();
    drop(directory);
    assert!(DataDirectory::acquire(&dir).is_err());
    storage.close().await;
    assert!(host.observe_actual().await);
    exclusive(&db);
    let released =
        DataDirectory::acquire(&dir).expect("actual original owner released after original joins");
    drop(released);
}
#[tokio::test]
async fn dev_original_preflight_error_keeps_error_and_closes_worker() {
    use sqlx::{Connection, SqliteConnection};
    let dir = area("preflight-error");
    let db = dir.join("node.sqlite3");
    let (raw_owner, raw_host) = scope(&dir);
    let raw_ticket = raw_owner.start_worker_scope().unwrap();
    raw_ticket.preflight_ready().await.unwrap();
    let options = raw_owner
        .connection_options()
        .unwrap()
        .create_if_missing(true);
    let mut connection = SqliteConnection::connect_with(&options).await.unwrap();
    sqlx::query("PRAGMA user_version=77")
        .execute(&mut connection)
        .await
        .unwrap();
    connection.close().await.unwrap();
    raw_owner.request_stop();
    assert!(raw_host.observe_actual().await);
    let (owner, host) = scope(&dir);
    let result = owner.start().unwrap().ready().await;
    assert!(matches!(
        result.as_ref().map_err(AsRef::as_ref),
        Err(acp_remote::error::StorageError::UnknownSchema(77))
    ));
    assert!(host.observe_actual().await);
    exclusive(&db);
}
#[tokio::test]
async fn dev_real_sqlite_scope_original_thread_and_app_join() {
    use sqlx::{Connection, SqliteConnection};
    let dir = area("raw-worker");
    let db = dir.join("node.sqlite3");
    let (owner, host) = scope(&dir);
    let ticket = owner.start_worker_scope().unwrap();
    ticket.preflight_ready().await.unwrap();
    let options = owner.connection_options().unwrap().create_if_missing(true);
    let connection = SqliteConnection::connect_with(&options).await.unwrap();
    let worker = connection.worker_registration().unwrap();
    connection.close().await.unwrap();
    owner.request_stop();
    assert!(host.observe_actual().await);
    let original = worker.snapshot().unwrap();
    assert_eq!(original.handle, sqlx::sqlite::WorkerHandleState::Joined);
    assert_eq!(
        original.join_report,
        Some(sqlx::sqlite::WorkerJoinReport::Ok)
    );
    exclusive(&db);
}

#[derive(Default)]
struct SyntheticProtector;
impl acp_remote::platform::SecretProtector for SyntheticProtector {
    fn protect(
        &self,
        plain: &[u8],
        context: &[u8],
    ) -> Result<Vec<u8>, acp_remote::error::PlatformError> {
        let mut result = b"CLDB-DEVELOPMENT-SYNTHETIC".to_vec();
        result.extend_from_slice(context);
        result.extend_from_slice(plain);
        Ok(result)
    }
    fn unprotect(
        &self,
        cipher: &[u8],
        context: &[u8],
    ) -> Result<Vec<u8>, acp_remote::error::PlatformError> {
        let prefix = [b"CLDB-DEVELOPMENT-SYNTHETIC".as_slice(), context].concat();
        if !cipher.starts_with(&prefix) {
            return Err(acp_remote::error::PlatformError::Protection(
                "development synthetic context mismatch".into(),
            ));
        }
        Ok(cipher[prefix.len()..].to_vec())
    }
}
fn node_config(dir: &std::path::Path) -> acp_remote::config::NodeConfig {
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert = dir.join("server.pem");
    let key = dir.join("server-key.pem");
    fs::write(&cert, certified.cert.pem()).unwrap();
    fs::write(&key, certified.signing_key.serialize_pem()).unwrap();
    acp_remote::config::NodeConfig {
        data_dir: dir.join("data"),
        listen: "127.0.0.1:0".parse().unwrap(),
        canonical_origin: "https://node.example.test".into(),
        tls_cert: cert,
        tls_key: key,
    }
}
#[tokio::test]
async fn dev_real_node_same_runtime_success_and_original_join() {
    let dir = area("node-success");
    let config = node_config(&dir);
    let data = config.data_dir.clone();
    let owner = acp_remote::node::NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        config,
        Arc::new(SyntheticProtector),
    );
    let node = owner.ready().await.unwrap();
    let address = node.ipc_address().clone();
    let stop = acp_remote::ipc::stop(&address);
    let run = node.wait_for_stop();
    let (response, run) = tokio::join!(stop, run);
    assert_eq!(response.unwrap().state, "stopped");
    run.unwrap();
    owner.join_actual().await.unwrap();
    exclusive(&data.join("node.sqlite3"));
    drop(DataDirectory::acquire(&data).unwrap());
}
#[tokio::test]
async fn dev_real_node_original_startup_error_joins_same_storage() {
    let dir = area("node-error");
    let mut config = node_config(&dir);
    let data = config.data_dir.clone();
    config.tls_cert = dir.join("missing.pem");
    let owner = acp_remote::node::NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        config,
        Arc::new(SyntheticProtector),
    );
    assert!(owner.ready().await.is_err());
    owner.join_actual().await.unwrap();
    exclusive(&data.join("node.sqlite3"));
    drop(DataDirectory::acquire(&data).unwrap());
}

fn main() {}
