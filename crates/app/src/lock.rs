//! 单实例锁与 `instanceId`（`docs/SECURITY_DESIGN.md` §12.1「单实例锁和 IPC endpoint 必须防止其他本机
//! 用户抢占」、`design.md` 决策 4）。
//!
//! 口径：
//!
//! - **互斥靠 OS advisory 文件锁**（`fs4`：unix `flock(LOCK_EX)`、Windows
//!   `LockFileEx(LOCKFILE_EXCLUSIVE_LOCK)`），锁文件是 `<data_dir>/daemon.lock`；获取失败即明确失败——
//!   调用方不得强杀已有进程，也不得启动第二个实例。
//! - **记录单独落在 `<data_dir>/daemon.instance.json`（为什么不是锁文件本身）**：Windows 的字节范围锁是
//!   **强制**的，锁覆盖的区间连读取都会被拒（实测：另一句柄 `File::open` 得 `ERROR_LOCK_VIOLATION`）。
//!   把记录写进被锁的同一个文件会让 CLI 在 Windows 上读不到 `instanceId`/`endpoint`。因此互斥与记录分家：
//!   锁文件只承载「能否加锁」，记录文件承载「本次运行的 `instanceId`/pid/endpoint」。
//! - **不删除锁文件**：unlink 与 advisory 锁并用会产生「两个进程各持一个 inode 的锁」的经典竞态。文件
//!   常驻，互斥只由「能否加锁」判定；CLI 因此必须**先尝试加锁**再读记录，不能只看文件存在（§7）。
//! - 记录里的 `endpoint` 是**定位串**（Windows pipe 名 / Unix socket 路径）。它进记录不是为了授权
//!   （授权由 endpoint 的 SDDL/权限与对端凭据校验承担），而是唯一可行的定位手段：**Windows 上 CLI 无法
//!   自行推导 pipe 名**（名字含当前用户 SID 的摘要，而取得 SID 需要 `windows-local-ipc` 的 FFI，`app`
//!   按 §5 依赖矩阵不得依赖它）。`app::client` 与 `app::daemon` 因此共用同一份记录形状。
//! - 记录用「临时文件 + rename」写出（rename 在 Windows 上替换已有文件），因此读取方要么看到上一份完整
//!   记录、要么看到本次完整记录，不会读到半截 JSON。

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use server::transport::local::InstanceId;

/// 锁文件名（`<daemon.data_dir>/daemon.lock`）。
pub const LOCK_FILE_NAME: &str = "daemon.lock";

/// 运行记录文件名（`<daemon.data_dir>/daemon.instance.json`）。
pub const RECORD_FILE_NAME: &str = "daemon.instance.json";

/// 运行记录。
///
/// 记录存在**不等于** Daemon 在运行：判定必须另经 [`DaemonLock::acquire`]（§7）。记录在「已持锁、尚未
/// 发布」的极短窗口里可能是上一次运行的旧值，CLI 遇到「有锁但连接不上」必须按 §7 报错（不视为「未运行」、
/// 不直接读库）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LockRecord {
    /// §2.1 的实例标识（16 字符小写 hex，64 bit CSPRNG）；同一运行期内不变。
    pub instance_id: String,
    /// 持有者的进程 id（诊断用；不参与互斥判定）。
    pub pid: u32,
    /// 本地管理通道的定位串（Windows pipe 名 / Unix socket 路径）。
    pub endpoint: Option<String>,
}

impl LockRecord {
    /// 解析并校验 `instanceId` 的形状（16 字符小写 hex）。
    pub fn instance_id(&self) -> Result<InstanceId, LockError> {
        InstanceId::new(&self.instance_id).map_err(|_| LockError::Corrupt)
    }
}

/// 锁的失败。全部失败关闭：调用方必须拒绝启动。
#[derive(Debug, thiserror::Error)]
pub enum LockError {
    /// 已由另一个实例持有（不得强杀、不得启动第二个实例）。
    #[error("已有另一个 acp-remote Daemon 持有单实例锁")]
    Held,
    /// 锁文件的文件系统操作失败。
    #[error("单实例锁文件不可用：{source}")]
    Io {
        /// 底层错误。
        #[source]
        source: std::io::Error,
    },
    /// 记录文件内容不是本模块写出的记录。
    #[error("单实例锁记录内容损坏")]
    Corrupt,
}

/// 已持有的单实例锁。`Drop` 释放（并显式 `unlock`）。
pub struct DaemonLock {
    file: File,
    path: PathBuf,
    record_path: PathBuf,
}

impl std::fmt::Debug for DaemonLock {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DaemonLock")
            .field("file_name", &LOCK_FILE_NAME)
            .finish_non_exhaustive()
    }
}

impl DaemonLock {
    /// 取锁（创建锁文件，必要时不阻塞地加独占锁）。
    ///
    /// 必须在打开存储**之后**调用：锁文件位于 `daemon.data_dir`，而该目录由存储层按 `0700` 创建。
    pub fn acquire(path: &Path) -> Result<Self, LockError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|source| LockError::Io { source })?;
        // 必须**全限定**调用：固定工具链的 `std::fs::File::try_lock`（1.89 稳定）与 `fs4::FileExt::try_lock`
        // 同名且方法解析优先级更高，写成 `file.try_lock()` 就等于把 MSRV 抬到 1.89
        // （`docs/MODULE_ARCHITECTURE.md` §3.1 的调用点约束）。
        match fs4::FileExt::try_lock(&file) {
            Ok(()) => Ok(Self {
                file,
                path: path.to_path_buf(),
                record_path: record_path_for(path),
            }),
            Err(fs4::TryLockError::WouldBlock) => Err(LockError::Held),
            Err(fs4::TryLockError::Error(source)) => Err(LockError::Io { source }),
        }
    }

    /// 锁文件路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 运行记录路径。
    pub fn record_path(&self) -> &Path {
        &self.record_path
    }

    /// 发布本次运行的记录（覆盖旧记录；调用方必须已持锁）。
    pub fn publish(&mut self, record: &LockRecord) -> Result<(), LockError> {
        let payload = serde_json::to_vec(record).map_err(|_| LockError::Corrupt)?;
        let temporary = self.record_path.with_extension("json.tmp");
        let io_error = |source: std::io::Error| LockError::Io { source };
        {
            let mut file = File::create(&temporary).map_err(io_error)?;
            use std::io::Write as _;
            file.write_all(&payload).map_err(io_error)?;
            file.flush().map_err(io_error)?;
        }
        std::fs::rename(&temporary, &self.record_path).map_err(io_error)
    }

    /// 删除运行记录（正常关闭的清理一步；锁文件本身常驻，见模块头注释）。
    pub fn remove_record(&self) {
        match std::fs::remove_file(&self.record_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => tracing::warn!(
                event = "daemon.record_remove_failed",
                error = %error,
                "运行记录未能删除（下次启动会覆盖它）"
            ),
        }
    }
}

impl Drop for DaemonLock {
    /// 释放锁。unlock 失败只记日志：进程即将退出，Drop 不得 panic。
    fn drop(&mut self) {
        if let Err(error) = fs4::FileExt::unlock(&self.file) {
            tracing::warn!(event = "daemon.lock_release_failed", error = %error);
        }
    }
}

/// 锁文件路径（`<data_dir>/daemon.lock`）。
pub fn lock_path(data_dir: &Path) -> PathBuf {
    data_dir.join(LOCK_FILE_NAME)
}

/// 记录文件路径（`<data_dir>/daemon.instance.json`）。
pub fn record_path(data_dir: &Path) -> PathBuf {
    data_dir.join(RECORD_FILE_NAME)
}

/// 由锁文件路径得到记录文件路径（同一目录）。
fn record_path_for(lock_path: &Path) -> PathBuf {
    lock_path.with_file_name(RECORD_FILE_NAME)
}

/// 读回运行记录（不改变锁状态）。
///
/// 返回 `Ok(None)` 表示记录不存在或为空（没有进程发布过记录，或上一次运行已正常清理）。**读到记录不等于
/// 有 Daemon 在运行**：判定必须另经 [`DaemonLock::acquire`]（§7）。
pub fn read_record(path: &Path) -> Result<Option<LockRecord>, LockError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(LockError::Io { source }),
    };
    if text.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|_| LockError::Corrupt)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个用例一个独立临时目录（用例结束即删）。
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "acpr-wp4a-lock-{label}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("临时目录");
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn a_second_holder_is_rejected_and_the_record_round_trips() {
        let dir = TempDir::new("held");
        let path = lock_path(&dir.path);
        let mut held = DaemonLock::acquire(&path).expect("首个持有者");
        let record = LockRecord {
            instance_id: "0123456789abcdef".to_owned(),
            pid: std::process::id(),
            endpoint: Some("endpoint-locator".to_owned()),
        };
        held.publish(&record).expect("发布记录");
        assert_eq!(held.record_path(), record_path(&dir.path));
        assert_eq!(
            read_record(held.record_path()).expect("可读回"),
            Some(record.clone())
        );
        assert_eq!(
            record.instance_id().expect("形状合法").as_str(),
            "0123456789abcdef"
        );

        // 同一进程内再取一次：advisory 锁对同一进程的第二个 fd 同样互斥（unix flock 与 Windows
        // LockFileEx 都不允许同一文件被两次独占加锁）。
        match DaemonLock::acquire(&path) {
            Err(LockError::Held) => {}
            other => panic!("第二个持有者必须被拒绝，实际 {other:?}"),
        }
        // 已有持有者不受影响：记录仍可读回（记录不在被锁的区间里）。
        assert_eq!(
            read_record(held.record_path()).expect("可读回"),
            Some(record)
        );
    }

    #[test]
    fn releasing_the_lock_allows_a_new_holder_without_losing_the_file() {
        let dir = TempDir::new("released");
        let path = lock_path(&dir.path);
        {
            let mut held = DaemonLock::acquire(&path).expect("首个持有者");
            held.publish(&LockRecord {
                instance_id: "fedcba9876543210".to_owned(),
                pid: 1,
                endpoint: None,
            })
            .expect("发布记录");
        }
        let again = DaemonLock::acquire(&path).expect("释放后必须可取锁");
        assert!(path.exists(), "锁文件常驻（unlink 会破坏互斥语义）");
        assert_eq!(
            read_record(again.record_path())
                .expect("可读回")
                .expect("记录仍在")
                .instance_id,
            "fedcba9876543210"
        );
        // 正常关闭的清理一步：删除运行记录。
        again.remove_record();
        assert_eq!(read_record(again.record_path()).expect("已删除"), None);
        drop(again);
    }

    #[test]
    fn a_missing_or_empty_record_reads_as_none_and_corruption_is_reported() {
        let dir = TempDir::new("read");
        let path = record_path(&dir.path);
        assert_eq!(read_record(&path).expect("缺失"), None);
        std::fs::write(&path, b"   \n").expect("空文件");
        assert_eq!(read_record(&path).expect("空内容"), None);
        std::fs::write(&path, b"{ not json").expect("损坏内容");
        assert!(matches!(read_record(&path), Err(LockError::Corrupt)));
        std::fs::write(
            &path,
            br#"{"instanceId":"TOO-SHORT","pid":1,"endpoint":null}"#,
        )
        .expect("写入");
        let record = read_record(&path).expect("可解析").expect("记录");
        assert!(
            matches!(record.instance_id(), Err(LockError::Corrupt)),
            "非法 instanceId 形状必须被拒绝"
        );
        std::fs::write(
            &path,
            br#"{"instanceId":"0123456789abcdef","pid":1,"endpoint":null}"#,
        )
        .expect("写入");
        let record = read_record(&path).expect("可解析").expect("记录");
        assert_eq!(
            record.instance_id().expect("形状合法").as_str(),
            "0123456789abcdef"
        );
        // 缺少字段或出现未知字段 → 损坏（closed object）。
        std::fs::write(&path, br#"{"instanceId":"0123456789abcdef"}"#).expect("写入");
        assert!(matches!(read_record(&path), Err(LockError::Corrupt)));
        std::fs::write(
            &path,
            br#"{"instanceId":"0123456789abcdef","pid":1,"endpoint":null,"extra":1}"#,
        )
        .expect("写入");
        assert!(matches!(read_record(&path), Err(LockError::Corrupt)));
    }
}
