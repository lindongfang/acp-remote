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
