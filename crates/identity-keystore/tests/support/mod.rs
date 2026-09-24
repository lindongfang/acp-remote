//! `identity-keystore` 测试基座：自建临时目录、可重跑熵源与「不出秘密」的断言辅助。

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use identity_auth::{EntropyError, EntropySource};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// 自建且在析构时删除的临时目录（不依赖外部 crate，也不碰用户主目录）。
pub struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    /// 在系统临时目录下新建一个唯一子目录（本方法创建目录，供 `root()` 返回）。
    pub fn new(tag: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "acpr-keystore-{tag}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("临时目录必须可创建");
        Self { path }
    }

    /// 目录路径。
    pub fn root(&self) -> &Path {
        &self.path
    }

    /// 目录内所有文件（相对路径，排序）——用于断言「失败时零写入」。
    pub fn files(&self) -> Vec<String> {
        let mut found = Vec::new();
        collect(&self.path, &self.path, &mut found);
        found.sort();
        found
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn collect(root: &Path, current: &Path, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(current) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, found);
        } else if let Ok(relative) = path.strip_prefix(root) {
            found.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// 定序熵源：同一次运行可重跑，且不读系统随机数。
#[derive(Debug, Default)]
pub struct FixedEntropy {
    counter: Mutex<u8>,
    fail: Mutex<bool>,
}

impl FixedEntropy {
    /// 新建。
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// 让后续 `fill` 失败（覆盖「熵源不可用 → 失败关闭」）。
    pub fn fail_next(&self) {
        *self.fail.lock().expect("熵源锁") = true;
    }
}

impl EntropySource for FixedEntropy {
    fn fill(&self, out: &mut [u8]) -> Result<(), EntropyError> {
        if *self.fail.lock().expect("熵源锁") {
            return Err(EntropyError::Unavailable);
        }
        let mut counter = self.counter.lock().expect("熵源锁");
        for byte in out.iter_mut() {
            *counter = counter.wrapping_add(1);
            *byte = *counter;
        }
        Ok(())
    }
}

/// 明文是否出现在文件字节里（用于断言秘密值不以明文落盘）。
pub fn contains_plaintext(file: &Path, plaintext: &[u8]) -> bool {
    let bytes = std::fs::read(file).expect("条目必须可读");
    bytes
        .windows(plaintext.len())
        .any(|window| window == plaintext)
}

/// 无 runtime 的最小 executor（与 `identity-auth` 测试同构）。
///
/// 端口实现里的 `async fn` 都是同步完成的（文件 IO + DPAPI 都是阻塞调用，包在端口形状里），
/// 因此轮询到 `Ready` 即可；长时间 `Pending` 时以明确消息失败，而不是假装「再等一会」。
pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::task::{Context, Poll, Waker};

    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = std::pin::pin!(future);
    for _ in 0..1_000_000 {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
    }
    panic!("future 长时间未就绪：端口实现必须立即完成（本 executor 不驱动真实 IO）");
}
