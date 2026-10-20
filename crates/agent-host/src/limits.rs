//! 固定 v1 常量（**不是配置键**）。
//!
//! 取值来自 `docs/SECURITY_DESIGN.md` §12.2 的表；实现不得自行发明未列出的数值。
//! 唯一随部署变化的是会话空闲回收，它由组合根从 `sessions.idle_timeout_ms` 读出后经
//! [`crate::HostConfig`] 注入。

use std::time::Duration;

/// Agent 启动 → `initialize` 完成。
pub const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

/// 短请求（`initialize`/`cancel`/`set_mode`/`set_config_option` 等）的超时。
///
/// **不适用于 `session/prompt`**：turn 没有超时（见 [`TURN_TIMEOUT`]）。
pub const SHORT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// `session/prompt`（turn）的超时：`None` 表示不设超时。
///
/// 长任务是合法的；只有显式取消或进程退出才结束 turn，超时杀进程会破坏 ACP 语义。
pub const TURN_TIMEOUT: Option<Duration> = None;

/// 关闭 grace（友好终止 → 强杀）。必须小于 `daemon.shutdown_grace_ms`（10 s）。
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// 单条 ACP 消息（stdout 分帧）的解析上限。
///
/// 与 `acp-protocol::limits::MAX_MESSAGE_BYTES` 和 Sync 的 `maxMessageBytes` 同值，避免同一条消息在两跳
/// 上有两个上限。
pub const MAX_MESSAGE_BYTES: usize = acp_protocol::limits::MAX_MESSAGE_BYTES;

/// stderr 环形缓冲上限。
pub const STDERR_RING_BYTES: usize = 256 * 1024;

/// 空闲回收的检查间隔（规则由 `sessions.idle_timeout_ms` 决定，这里只是轮询粒度）。
pub const IDLE_SWEEP_INTERVAL: Duration = Duration::from_secs(1);

/// 进程关闭后等待其退出的上限（避免 `wait` 永久悬挂）。
pub const EXIT_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------------------------
// ACP 终端服务（`terminal/*`）的固定 v1 常量
// ---------------------------------------------------------------------------------------------

/// 单个终端保留的输出字节上限（Agent 未给 `outputByteLimit` 时，以及 Agent 给得更大时的上限）。
///
/// Agent 请求的更小预算优先（ACP `CreateTerminalRequest.outputByteLimit` 的语义就是「最多保留多少」）；
/// 这里只封顶，避免一个终端把 Daemon 的内存无界占住。取值来自 `SECURITY_DESIGN.md` §12.2。
pub const TERMINAL_OUTPUT_BYTES: usize = 1_048_576;

/// 单次读取与单条 `terminal.output` 事件的最大文本字节数。
pub const TERMINAL_CHUNK_BYTES: usize = 8_192;

/// 同一会话同时存在的终端数上限（运行保护，不是产品额度）。
pub const TERMINAL_MAX_ACTIVE: usize = 8;

/// 释放/关闭终端时等待输出排空的时长；超时即结束该终端的进程树并放弃剩余读取。
pub const TERMINAL_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
