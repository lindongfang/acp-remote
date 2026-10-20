//! 平台差异：完整进程树的结束方式，以及退出事实（退出码/信号）的映射。
//!
//! 只有本模块出现 `cfg`（`docs/MODULE_ARCHITECTURE.md` §4.5）。两条实现都要满足同一个性质：
//! **系统结束该 Agent 或关闭它持有的句柄后，孙进程不再存活**。
//!
//! - Windows：`win32job` 的 Job Object，`KILL_ON_JOB_CLOSE` 打开；结束该 Agent 的方式是**关闭 Job 句柄**
//!   （内核随之结束树内全部进程）。`win32job` 2.x 没有封装 `TerminateJobObject`，而
//!   `unsafe_code = "forbid"` 不允许直接写 FFI，因此走「丢弃句柄」这条等价路径（`tasks.md` 2.17 允许）。
//! - Unix：spawn 时把子进程放进**自己的进程组**（`process_group(0)`），结束用 `killpg`；没有句柄可关，
//!   因此「关闭」显式等于结束该进程组。
//!
//! 这里不出现任何 `unsafe`（workspace 固定 `unsafe_code = "forbid"`），平台能力全部来自 wrapper crate。

use crate::error::HostError;

/// 在 `PATH` 里查找命令时要尝试的文件名（Windows 需要补 `.exe`）。
///
/// 放在本模块是为了让平台分支只出现在这里（`docs/MODULE_ARCHITECTURE.md` §4.5）。
#[must_use]
pub fn command_candidates(command: &str) -> Vec<String> {
    if cfg!(windows) {
        vec![command.to_owned(), format!("{command}.exe")]
    } else {
        vec![command.to_owned()]
    }
}

/// 进程退出事实的平台视图（ACP `TerminalExitStatus` 的两个字段）。
///
/// 正常退出有 `exit_code`；Unix 被信号终止时 `code()` 为 `None`，必须用 `ExitStatusExt::signal()`
/// 映射成信号名（例如 `SIGKILL`），否则 `terminal/wait_for_exit` 会拿到一个两个字段都为空的
/// 「退出事实」，调用方无法判定结果。Windows 没有信号语义，只填退出码。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExitInfo {
    /// 退出码（被信号终止时为 `None`）。
    pub(crate) exit_code: Option<u32>,
    /// 终止信号名（正常退出或 Windows 上为 `None`）。
    pub(crate) signal: Option<String>,
}

/// 无法从平台取到任何退出事实时使用的确定性非成功退出码。
///
/// `wait_for_exit` 的契约是「返回一个可判定的退出事实」；`status` 缺失（`wait` 失败）时不能让两个
/// 字段同时缺席，否则调用方只能当作悬挂。`1` 是通用的非成功值，不会被误读成成功。
pub(crate) const UNKNOWN_EXIT_CODE: u32 = 1;

/// 把平台退出状态映射成退出事实；两边都取不到时给 [`UNKNOWN_EXIT_CODE`]。
#[must_use]
pub(crate) fn exit_status(status: Option<std::process::ExitStatus>) -> ExitInfo {
    let info = platform_exit_status(status);
    if info.exit_code.is_none() && info.signal.is_none() {
        ExitInfo {
            exit_code: Some(UNKNOWN_EXIT_CODE),
            signal: None,
        }
    } else {
        info
    }
}

#[cfg(windows)]
fn platform_exit_status(status: Option<std::process::ExitStatus>) -> ExitInfo {
    ExitInfo {
        exit_code: windows_exit_code(status.and_then(|status| status.code())),
        signal: None,
    }
}

/// Windows 退出码的位模式：`ExitStatus::code()` 返回 `i32`，但 Windows 的退出码语义是 `DWORD`。
///
/// 高位置位的退出码（例如 `0xC0000005` 访问冲突、`0xC000013A` 强制终止）是负的 `i32`；
/// `u32::try_from` 会失败并让调用方回退到 [`UNKNOWN_EXIT_CODE`]，把真实退出码误报成 `1`。
/// `as u32` 保留原始位模式，正是 ACP `exitCode`（uint32）需要的值。
#[cfg(windows)]
fn windows_exit_code(code: Option<i32>) -> Option<u32> {
    code.map(|code| code as u32)
}

#[cfg(unix)]
fn platform_exit_status(status: Option<std::process::ExitStatus>) -> ExitInfo {
    use std::os::unix::process::ExitStatusExt;

    let Some(status) = status else {
        return ExitInfo {
            exit_code: None,
            signal: None,
        };
    };
    if let Some(code) = status.code() {
        return ExitInfo {
            exit_code: u32::try_from(code).ok(),
            signal: None,
        };
    }
    ExitInfo {
        exit_code: None,
        signal: status.signal().map(signal_name),
    }
}

#[cfg(unix)]
fn signal_name(signal: i32) -> String {
    nix::sys::signal::Signal::try_from(signal)
        .map(|signal| signal.as_str().to_owned())
        .unwrap_or_else(|_| format!("SIG{signal}"))
}

#[cfg(not(any(windows, unix)))]
fn platform_exit_status(_status: Option<std::process::ExitStatus>) -> ExitInfo {
    ExitInfo {
        exit_code: None,
        signal: None,
    }
}

/// 一个 Agent 的进程树句柄。粒度是**每个 Agent 一个**：结束某个 Agent 不影响其它 Agent
/// （单个全局 Job 无法只结束一棵树，那正是 `docs/MODULE_ARCHITECTURE.md` §4.5 的收口点）。
#[derive(Debug)]
pub struct ProcessTree {
    inner: inner::Inner,
}

impl ProcessTree {
    /// spawn **之前**的准备。
    ///
    /// Windows：先创建 Job 并打开 `KILL_ON_JOB_CLOSE`，这样 spawn 之后可以立刻把子进程放进去。
    /// Unix：给命令设置新的进程组。
    pub fn prepare(command: &mut tokio::process::Command) -> Result<Self, HostError> {
        Ok(Self {
            inner: inner::Inner::prepare(command)?,
        })
    }

    /// spawn **之后、写 stdin 之前**立刻调用。
    pub fn attach(&self, child: &tokio::process::Child) -> Result<(), HostError> {
        self.inner.attach(child)
    }

    /// 结束整棵树（Windows：关闭 Job 句柄；Unix：`killpg`）。
    pub fn terminate(&self) {
        self.inner.terminate();
    }

    /// 释放句柄（与 [`ProcessTree::terminate`] 同义；幂等）。
    pub fn close(&self) {
        self.inner.close();
    }
}

#[cfg(windows)]
mod inner {
    use std::sync::Mutex;

    use win32job::{ExtendedLimitInfo, Job};

    use crate::error::HostError;

    /// Windows 的进程树句柄：一个 Job，句柄被取出后置为 `None`（幂等）。
    #[derive(Debug, Default)]
    pub struct Inner {
        job: Mutex<Option<Job>>,
    }

    impl Inner {
        pub fn prepare(_command: &mut tokio::process::Command) -> Result<Self, HostError> {
            let mut info = ExtendedLimitInfo::new();
            // 句柄关闭即结束整棵树：这是「父与孙一并停止」的判据（`INITIAL_DESIGN.md` §16 第 4 条）。
            info.limit_kill_on_job_close();
            let job =
                Job::create_with_limit_info(&info).map_err(|error| HostError::SpawnFailed {
                    detail: format!("创建 Job Object 失败：{error}"),
                })?;
            Ok(Self {
                job: Mutex::new(Some(job)),
            })
        }

        pub fn attach(&self, child: &tokio::process::Child) -> Result<(), HostError> {
            // `raw_handle()` 给的是 `*mut c_void`，而 `win32job` 收 `isize`（HANDLE 的内部表示）。
            let handle = child.raw_handle().ok_or_else(|| HostError::SpawnFailed {
                detail: "进程没有可用的原始句柄".to_owned(),
            })? as isize;
            let guard = lock(&self.job);
            let job = guard.as_ref().ok_or(HostError::NotRunning)?;
            job.assign_process(handle)
                .map_err(|error| HostError::SpawnFailed {
                    detail: format!("把进程加入 Job Object 失败：{error}"),
                })
        }

        pub fn terminate(&self) {
            // 取出并丢弃句柄：内核按 KILL_ON_JOB_CLOSE 结束整棵树。
            drop(lock(&self.job).take());
        }

        pub fn close(&self) {
            self.terminate();
        }
    }

    fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        match mutex.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[cfg(unix)]
mod inner {
    use std::sync::Mutex;

    use crate::error::HostError;

    /// 子进程的 pid（等于它自己进程组的 id）在 attach 时记下来，供 `killpg` 使用。
    #[derive(Debug, Default)]
    pub struct Inner {
        pgid: Mutex<Option<i32>>,
    }

    impl Inner {
        pub fn prepare(command: &mut tokio::process::Command) -> Result<Self, HostError> {
            // 子进程成为自己进程组的组长：结束该组即可覆盖整棵树。
            // 注意：tokio 的 Command 在 unix 上自带 `process_group`，无需引入 std 的 CommandExt。
            command.process_group(0);
            Ok(Self {
                pgid: Mutex::new(None),
            })
        }

        pub fn attach(&self, child: &tokio::process::Child) -> Result<(), HostError> {
            let pid = child.id().ok_or_else(|| HostError::SpawnFailed {
                detail: "进程没有可用的 pid".to_owned(),
            })?;
            let pid = i32::try_from(pid).map_err(|_| HostError::SpawnFailed {
                detail: "pid 超出 i32".to_owned(),
            })?;
            *lock(&self.pgid) = Some(pid);
            Ok(())
        }

        pub fn terminate(&self) {
            // 结束整个进程组：父与孙一并停止。`killpg` 的安全封装来自 `nix`，因此不需要 `unsafe`。
            if let Some(pgid) = *lock(&self.pgid) {
                let _ = nix::sys::signal::killpg(
                    nix::unistd::Pid::from_raw(pgid),
                    nix::sys::signal::Signal::SIGKILL,
                );
            }
        }

        pub fn close(&self) {
            self.terminate();
        }
    }

    fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        match mutex.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[cfg(not(any(windows, unix)))]
mod inner {
    use crate::error::HostError;

    #[derive(Debug)]
    pub struct Inner;

    impl Inner {
        pub fn prepare(_command: &mut tokio::process::Command) -> Result<Self, HostError> {
            Err(HostError::SpawnFailed {
                detail: "当前平台没有进程树清理实现".to_owned(),
            })
        }

        pub fn attach(&self, _child: &tokio::process::Child) -> Result<(), HostError> {
            Ok(())
        }

        pub fn terminate(&self) {}

        pub fn close(&self) {}
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::process::Command;

    use super::*;

    /// Unix 上被 SIGKILL 终止时 `code()` 为 `None`：必须给出信号名，否则 `wait_for_exit` 的
    /// 「退出事实」两个字段都为空，调用方无法判定结果（finding 2 的回归点）。
    #[test]
    fn sigkill_is_reported_as_a_signal_not_an_empty_exit_fact() {
        let status = Command::new("sh")
            .args(["-c", "kill -9 $$"])
            .status()
            .expect("启动 sh");
        let info = exit_status(Some(status));
        assert_eq!(info.exit_code, None);
        assert_eq!(info.signal.as_deref(), Some("SIGKILL"));
    }

    /// 正常退出仍然只带退出码（不伪造信号）。
    #[test]
    fn normal_exit_reports_only_the_code() {
        let status = Command::new("sh")
            .args(["-c", "exit 7"])
            .status()
            .expect("启动 sh");
        let info = exit_status(Some(status));
        assert_eq!(info.exit_code, Some(7));
        assert_eq!(info.signal, None);
    }
}

/// `std::process::ExitStatus` 不能在测试里构造任意位模式，因此直接断言转换函数：
/// 高位退出码必须保留原始 u32，而不是回退到 [`UNKNOWN_EXIT_CODE`]（finding 3 的回归点）。
#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    #[test]
    fn high_bit_exit_codes_keep_their_bit_pattern() {
        for code in [0xC000_0005u32, 0xC000_013A, 0x8000_0000, 0xFFFF_FFFF] {
            assert_eq!(
                windows_exit_code(Some(code as i32)),
                Some(code),
                "高位退出码 {code:#010X} 必须原样保留"
            );
        }
    }

    #[test]
    fn ordinary_and_missing_codes_are_unchanged() {
        assert_eq!(windows_exit_code(Some(0)), Some(0));
        assert_eq!(windows_exit_code(Some(7)), Some(7));
        assert_eq!(windows_exit_code(None), None);
    }
}
