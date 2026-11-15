use std::{ffi::OsStr, io::Write, sync::Arc};

use tokio::sync::Notify;

const CRASH_POINT_ENV: &str = "ACP_REMOTE_IDENTITY_CRASH_POINT";

pub fn crash_at(point: &str) {
    if std::env::var_os(CRASH_POINT_ENV).as_deref() == Some(OsStr::new(point)) {
        println!("CRASH_POINT={point}");
        let _ = std::io::stdout().flush();
        std::process::abort();
    }
}

pub fn crash_identity_write_at(point: &str, written: usize, total: usize) {
    if std::env::var_os(CRASH_POINT_ENV).as_deref() == Some(OsStr::new(point)) {
        println!("IDENTITY_WRITE_BYTES={written}/{total}");
    }
    crash_at(point);
}

#[cfg(windows)]
fn resolve_or_build_dev_child(
    env_vars: &[&str],
    cache: &std::sync::OnceLock<std::path::PathBuf>,
    bin_name: &str,
    feature: &str,
) -> std::path::PathBuf {
    static BUILD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    for name in env_vars {
        if let Some(value) = std::env::var_os(name)
            && !value.is_empty()
        {
            return std::path::PathBuf::from(value);
        }
    }
    if let Some(cached) = cache.get() {
        return cached.clone();
    }
    let guard = BUILD_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(cached) = cache.get() {
        drop(guard);
        return cached.clone();
    }
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| std::ffi::OsString::from("cargo"));
    let output = std::process::Command::new(cargo)
        .arg("build")
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--locked")
        .arg("--bin")
        .arg(bin_name)
        .arg("--features")
        .arg(feature)
        .arg("--message-format=json")
        .output()
        .expect("invoke cargo to build dev child binary");
    assert!(
        output.status.success(),
        "cargo build --bin {bin_name} --features {feature} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let executable = String::from_utf8(output.stdout)
        .expect("cargo json output utf8")
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|msg| {
            msg.get("reason").and_then(serde_json::Value::as_str) == Some("compiler-artifact")
                && msg
                    .get("target")
                    .and_then(|t| t.get("name"))
                    .and_then(serde_json::Value::as_str)
                    == Some(bin_name)
        })
        .and_then(|msg| {
            msg.get("executable")
                .and_then(serde_json::Value::as_str)
                .map(std::path::PathBuf::from)
        })
        .expect("dev child executable path in cargo output");
    assert!(
        executable.is_file(),
        "built dev child executable missing at {}",
        executable.display()
    );
    let _ = cache.set(executable.clone());
    drop(guard);
    executable
}

#[cfg(windows)]
pub fn native_dispatch_dev_child(env_vars: &[&str]) -> std::path::PathBuf {
    static BIN: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    resolve_or_build_dev_child(
        env_vars,
        &BIN,
        "native_dispatch_dev_child",
        "native-dispatch-dev-fixtures",
    )
}

#[cfg(all(windows, feature = "approval-owner-dev-fixtures"))]
pub fn approval_owner_dev_child() -> std::path::PathBuf {
    static BIN: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    resolve_or_build_dev_child(
        &["APPROVAL_OWNER_DEV_CHILD"],
        &BIN,
        "approval_owner_dev_child",
        "approval-owner-dev-fixtures",
    )
}

#[derive(Default)]
pub struct AsyncBarrier {
    entered: Notify,
    release: Notify,
}

impl AsyncBarrier {
    pub async fn pause(&self) {
        self.entered.notify_one();
        self.release.notified().await;
    }

    pub async fn wait_until_entered(&self) {
        self.entered.notified().await;
    }

    pub fn release(&self) {
        self.release.notify_one();
    }
}

pub type SharedAsyncBarrier = Arc<AsyncBarrier>;
