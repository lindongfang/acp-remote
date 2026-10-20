//! ACP 终端服务：`terminal/create`、`output`、`wait_for_exit`、`kill`、`release` 的真实实现。
//!
//! 语义边界（`openspec/changes/conversation-area-complete/specs/agent-terminal-service/spec.md`、
//! `design.md` D7、`SECURITY_DESIGN.md` §12.6）：
//!
//! - **真实子进程**：命令按参数数组直接 spawn（不经 shell），stdin 关闭，stdout/stderr 分开读取。
//!   输出按实际接收次序以 `terminal.output` 事件交付（`terminalId` + 十进制 `chunkIndex` +
//!   `stream`），UTF-8 跨读取边界由增量解码器处理。
//! - **预算与截断**：保留正文受 Agent 请求的 `outputByteLimit` 与固定服务上限的较小值约束；超出时
//!   从**开头**丢弃、落在字符边界，并在 `terminal/output` 的响应里置 `truncated = true`。响应还要
//!   留在 ACP 单条消息预算内（含 JSON-RPC 信封与 JSON 转义后的实际字节），放不下时再次从开头裁剪
//!   并同样置 `truncated = true`——保留正文上限与帧上限相等时，转义会膨胀，不能直接整段回传。
//!   实时事件不截断（每个分块本身有上限），因此事件永远是该次读取的真实内容。
//! - **kill 与 release**：kill 只结束进程树、保留标识，之后仍可 `output`/`wait_for_exit`；
//!   release 结束未退出任务并移除标识，之后一切操作都返回「未知终端」。两者都幂等：kill 与自然退出
//!   竞争时只记录**一个**退出事实。Unix 被信号终止时退出事实带信号名（如 `SIGKILL`），Windows 带
//!   退出码；两者都取不到时给确定性的非成功退出码，等待者因此永远不会拿到空事实。
//! - **所有权与清理**：每个终端有自己的 Job Object（Windows）/进程组（Unix）。会话关闭、Agent 退出
//!   与 Daemon 关闭的顺序是：停止接受新请求 → 唤醒 `wait_for_exit` 等待者（返回明确错误，不悬挂）→
//!   结束终端进程树 → 排空已读输出 → 结束驱动任务。同步关闭只负责停止新请求与结束进程树，被关闭
//!   的终端保留在 `closing` 列表里，异步关闭据此等待驱动任务把已读输出交付完并结束；
//!   驱动任务由 [`crate::process::Supervisor`] 收养，随进程监督者一起 join，不存在 detached task。
//! - **会话绑定**：终端标识只在本会话的服务里存在；另一个会话拿同一个标识只会得到「未知终端」。
//!
//! 日志只写方法名与类别：命令行、参数、环境变量值与路径都不进日志（`SECURITY_DESIGN.md` §14.1）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use acp_core::model::{EndpointEvent, SessionId};
use acp_core::ports::{Clock, EventSink};
use acp_protocol::terminal::{
    CreateTerminalRequest, TerminalExitStatus, TerminalOutputResponse, WaitForTerminalExitResponse,
};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::Notify;

use crate::error::HostError;
use crate::launch;
use crate::limits;
use crate::mapper;
use crate::platform::ProcessTree;
use crate::process::Supervisor;

/// 全局递增的终端标识来源（同一 Daemon 进程内不会重复；不依赖会话重建）。
static NEXT_TERMINAL_ID: AtomicU64 = AtomicU64::new(1);

/// 终端服务的固定输入（由会话提供）。
pub(crate) struct TerminalConfig {
    /// core 会话标识（诊断与事件归属）。
    pub session: SessionId,
    /// 会话工作目录的规范绝对路径（终端的默认 cwd，也是显式 cwd 的允许边界）。
    pub workspace: PathBuf,
    /// 时钟（事件时间戳）。
    pub clock: Arc<dyn Clock>,
    /// 终端服务的任务所有权（驱动任务由它收养）。
    pub supervisor: Arc<Supervisor>,
    /// 事件出口（会话重绑时替换）。
    pub sink: EventSink,
    /// 是否允许创建终端（`clientCapabilities.terminal` 已由组合根宣告）。
    pub enabled: bool,
}

/// 一个会话的终端服务。
pub(crate) struct TerminalService {
    session: SessionId,
    workspace: PathBuf,
    clock: Arc<dyn Clock>,
    supervisor: Arc<Supervisor>,
    sink: Arc<Mutex<EventSink>>,
    /// 已登记（含正在 spawn 的占位项）的终端。占位项让「关闭判定 + 登记」处于同一临界区：
    /// 关闭要么在 spawn 之前看到它并拒绝/回收，要么在 spawn 之后能把它排空，不会漏掉已启动的树。
    terminals: Mutex<HashMap<String, Option<Arc<Terminal>>>>,
    /// 同步关闭排空出的终端；异步关闭据此等待输出排空与驱动任务结束（同步关闭不消费这条线索）。
    closing: Mutex<Vec<Arc<Terminal>>>,
    enabled: bool,
    closed: AtomicBool,
    closed_notify: Notify,
}

impl std::fmt::Debug for TerminalService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalService")
            .field("session", &self.session)
            .field("enabled", &self.enabled)
            .field("active", &self.terminal_count())
            .field("closed", &self.is_closed())
            .finish_non_exhaustive()
    }
}

impl TerminalService {
    /// 构造。
    pub(crate) fn new(config: TerminalConfig) -> Self {
        Self {
            session: config.session,
            workspace: config.workspace,
            clock: config.clock,
            supervisor: config.supervisor,
            sink: Arc::new(Mutex::new(config.sink)),
            terminals: Mutex::new(HashMap::new()),
            closing: Mutex::new(Vec::new()),
            enabled: config.enabled,
            closed: AtomicBool::new(false),
            closed_notify: Notify::new(),
        }
    }

    /// 组合根是否宣告了 `clientCapabilities.terminal`（未宣告时不得接受 `terminal/*`）。
    #[must_use]
    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// 会话是否已关闭。
    #[must_use]
    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// 当前活动终端数（诊断与上限判定）。
    #[must_use]
    pub(crate) fn terminal_count(&self) -> usize {
        lock(&self.terminals).len()
    }

    /// 会话重绑（`open`）后把事件出口指向新 sink：已运行终端的后续输出仍交付给当前客户端。
    pub(crate) fn set_sink(&self, sink: EventSink) {
        *lock(&self.sink) = sink;
    }

    /// `terminal/create`：启动一个真实子进程并登记。
    ///
    /// 「已关闭判定 + 并发上限 + 登记」在同一个临界区内完成：先插入占位项，再在锁外 spawn。
    /// 关闭在 spawn 之前发生时会排空占位项，本次调用在 spawn 之后看到关闭标志并结束刚启动的树；
    /// 关闭在 spawn 之后发生时会排空真实登记并结束树。两条路径都不会让已经 spawn 的终端逃出
    /// `drain`。
    pub(crate) fn create(&self, request: &CreateTerminalRequest) -> Result<String, HostError> {
        if self.is_closed() {
            return Err(HostError::SessionClosed);
        }
        if !self.enabled {
            // 未宣告的能力不接受调用（组合根没有把能力接完整）。
            return Err(HostError::CapabilityNotDeclared {
                capability: "clientCapabilities.terminal".to_owned(),
            });
        }
        if request.command.is_empty() {
            return Err(HostError::TerminalRejected {
                detail: "命令为空".to_owned(),
            });
        }

        let cwd = self.terminal_cwd(request.cwd.as_deref())?;
        let mut command = Command::new(&request.command);
        command.args(&request.args);
        // 子进程环境**先清空**再注入：必要进程项 + Agent 显式给出的变量（名字仍受保留前缀约束）。
        command.env_clear();
        for name in launch::NECESSARY_ENV {
            if let Ok(value) = std::env::var(name) {
                command.env(name, value);
            }
        }
        for variable in &request.env {
            launch::validate_env_name(&variable.name)?;
            command.env(&variable.name, &variable.value);
        }
        command.current_dir(cwd);
        command.stdin(std::process::Stdio::null());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        command.kill_on_drop(false);

        let limit = request
            .output_byte_limit
            .map_or(limits::TERMINAL_OUTPUT_BYTES, |requested| {
                usize::try_from(requested).unwrap_or(usize::MAX)
            })
            .min(limits::TERMINAL_OUTPUT_BYTES);
        let id = format!("term-{}", NEXT_TERMINAL_ID.fetch_add(1, Ordering::SeqCst));
        {
            let mut terminals = lock(&self.terminals);
            if self.is_closed() {
                return Err(HostError::SessionClosed);
            }
            if terminals.len() >= limits::TERMINAL_MAX_ACTIVE {
                return Err(HostError::TerminalRejected {
                    detail: "活动终端数已达上限".to_owned(),
                });
            }
            terminals.insert(id.clone(), None);
        }

        let tree = match ProcessTree::prepare(&mut command) {
            Ok(tree) => tree,
            Err(error) => {
                self.discard_placeholder(&id);
                return Err(error);
            }
        };
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(_) => {
                self.discard_placeholder(&id);
                return Err(HostError::TerminalRejected {
                    detail: "命令无法启动".to_owned(),
                });
            }
        };
        if let Err(error) = tree.attach(&child) {
            // 已经 spawn：必须结束它，不能留孤儿；wait 交给监督者收养，避免僵尸。
            tree.terminate();
            let _ = child.start_kill();
            self.reap(child);
            self.discard_placeholder(&id);
            return Err(error);
        }
        let (stdout, stderr) = match (child.stdout.take(), child.stderr.take()) {
            (Some(stdout), Some(stderr)) => (stdout, stderr),
            _ => {
                tree.terminate();
                let _ = child.start_kill();
                self.reap(child);
                self.discard_placeholder(&id);
                return Err(HostError::TerminalRejected {
                    detail: "标准输出/错误管道未建立".to_owned(),
                });
            }
        };

        let terminal = Arc::new(Terminal {
            id: id.clone(),
            tree: Arc::new(tree),
            output: Mutex::new(OutputBuffer::new(limit)),
            exit: Mutex::new(None),
            exited: Notify::new(),
            drained: AtomicBool::new(false),
            drained_notify: Notify::new(),
        });
        {
            let mut terminals = lock(&self.terminals);
            if self.is_closed() {
                // 关闭在本终端 spawn 期间发生：占位项已被排空，必须结束刚启动的树，
                // 并把「不再接受请求」作为结果返回（不登记一个关闭后的终端）。
                drop(terminals);
                terminal.tree.terminate();
                let _ = child.start_kill();
                self.reap(child);
                return Err(HostError::SessionClosed);
            }
            terminals.insert(id.clone(), Some(Arc::clone(&terminal)));
        }

        // 驱动任务由监督者收养：随 Agent 进程一起 join，不留下 detached task。
        let sink = Arc::clone(&self.sink);
        let clock = Arc::clone(&self.clock);
        let driver = tokio::spawn(drive(child, stdout, stderr, terminal, sink, clock));
        self.supervisor.adopt(driver);
        Ok(id)
    }

    /// 移除尚未完成的占位登记（spawn 失败路径；关闭已排空时是无操作）。
    fn discard_placeholder(&self, id: &str) {
        lock(&self.terminals).remove(id);
    }

    /// 已经结束的子进程交给监督者 `wait`（回收僵尸）；失败路径不会留下无人等待的 Child。
    fn reap(&self, mut child: tokio::process::Child) {
        self.supervisor.spawn_owned(async move {
            let _ = child.wait().await;
        });
    }

    /// `terminal/output`：读取保留的输出与（已退出时的）退出状态。
    ///
    /// `frame_budget` 是**序列化后的 `result` 对象**可用的字节数（调用方按实际 `id` 从
    /// [`limits::MAX_MESSAGE_BYTES`] 扣掉 JSON-RPC 信封得到）。保留正文放不进这个预算时从开头
    /// 再裁剪一次，并继续以 `truncated = true` 如实上报。
    pub(crate) fn output(
        &self,
        terminal_id: &str,
        frame_budget: usize,
    ) -> Result<TerminalOutputResponse, HostError> {
        let terminal = self.find(terminal_id)?;
        let (output, truncated) = lock(&terminal.output).snapshot();
        let exit_status = lock(&terminal.exit).clone();
        let (output, truncated) =
            fit_to_frame(output, truncated, exit_status.as_ref(), frame_budget);
        Ok(TerminalOutputResponse {
            output,
            truncated,
            exit_status,
            meta: None,
        })
    }

    /// `terminal/kill`：结束进程树但保留标识（之后仍可读取输出与退出结果）。
    pub(crate) fn kill(&self, terminal_id: &str) -> Result<(), HostError> {
        let terminal = self.find(terminal_id)?;
        // 关闭 Job/结束进程组；真正的退出事实由驱动任务在 `wait` 返回时记录（唯一来源）。
        terminal.tree.terminate();
        Ok(())
    }

    /// `terminal/wait_for_exit`：等待唯一的退出事实；会话关闭时显式失败而不是永久悬挂。
    pub(crate) async fn wait_for_exit(
        &self,
        terminal_id: &str,
    ) -> Result<WaitForTerminalExitResponse, HostError> {
        let terminal = self.find(terminal_id)?;
        loop {
            // 两个唤醒源都先登记 waiter 再判状态：反过来的顺序会丢掉 `notify_waiters`
            // （关闭通知不保存许可，落在检查与登记之间就永久丢失）。
            let notified = terminal.exited.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let closed = self.closed_notify.notified();
            tokio::pin!(closed);
            closed.as_mut().enable();
            if let Some(status) = lock(&terminal.exit).clone() {
                return Ok(WaitForTerminalExitResponse::from_status(&status));
            }
            if self.is_closed() {
                return Err(HostError::SessionClosed);
            }
            tokio::select! {
                () = notified.as_mut() => {}
                () = closed.as_mut() => {
                    if self.is_closed() {
                        return Err(HostError::SessionClosed);
                    }
                }
            }
        }
    }

    /// `terminal/release`：结束未退出任务、移除标识；已读输出与历史事件不受影响。
    pub(crate) async fn release(&self, terminal_id: &str) -> Result<(), HostError> {
        let terminal = lock(&self.terminals)
            .remove(terminal_id)
            .flatten()
            .ok_or(HostError::UnknownTerminal)?;
        terminal.tree.terminate();
        // 排空已可提交的输出：等到驱动任务结束（退出事实已记录，输出已交付），超时则不阻塞会话关闭。
        let _ =
            tokio::time::timeout(limits::TERMINAL_DRAIN_TIMEOUT, await_drained(&terminal)).await;
        Ok(())
    }

    /// 同步关闭：标记关闭、唤醒等待者、结束全部终端进程树。
    ///
    /// 排空出的终端同时记入 `closing`：接下来的 [`TerminalService::shutdown`] 必须还能拿到它们，
    /// 否则「排空已读输出」在标准关闭路径（`close_session` → `shutdown_terminals`）上会变成空操作。
    pub(crate) fn shutdown_sync(&self) {
        for terminal in self.begin_close() {
            terminal.tree.terminate();
        }
    }

    /// 异步关闭：在同步关闭之后再等驱动任务把已读输出交付完（有界）。
    pub(crate) async fn shutdown(&self) {
        let terminals = self.begin_close();
        for terminal in &terminals {
            terminal.tree.terminate();
        }
        for terminal in &terminals {
            let _ =
                tokio::time::timeout(limits::TERMINAL_DRAIN_TIMEOUT, await_drained(terminal)).await;
        }
    }

    /// 置关闭标志并把当前登记（含占位项）搬到 `closing`；重复调用返回同一批终端。
    ///
    /// 取锁顺序固定为 `terminals` → `closing`（`create` 只取前者），不存在的反向顺序因此不会死锁。
    fn begin_close(&self) -> Vec<Arc<Terminal>> {
        let mut terminals = lock(&self.terminals);
        let mut closing = lock(&self.closing);
        if !self.closed.swap(true, Ordering::SeqCst) {
            self.closed_notify.notify_waiters();
            closing.extend(terminals.drain().filter_map(|(_, terminal)| terminal));
        }
        closing.clone()
    }

    fn find(&self, terminal_id: &str) -> Result<Arc<Terminal>, HostError> {
        if self.is_closed() {
            return Err(HostError::SessionClosed);
        }
        let terminal = lock(&self.terminals).get(terminal_id).cloned().flatten();
        match terminal {
            Some(terminal) => Ok(terminal),
            // `begin_close` 在持有 `terminals` 锁时置关闭标志并清空登记：上面的检查与这次取锁之间
            // 可能刚好关闭。此时“找不到”表达的是会话已关闭，而不是一个从未存在的标识。
            None if self.is_closed() => Err(HostError::SessionClosed),
            None => Err(HostError::UnknownTerminal),
        }
    }

    /// 校验 `cwd`：必须是已存在目录，且规范化后仍在会话工作目录内。
    fn terminal_cwd(&self, requested: Option<&str>) -> Result<PathBuf, HostError> {
        let root = self
            .workspace
            .canonicalize()
            .map_err(|_| HostError::TerminalRejected {
                detail: "会话工作目录不可用".to_owned(),
            })?;
        let Some(requested) = requested else {
            return Ok(root);
        };
        let path = Path::new(requested);
        if !path.is_absolute() {
            return Err(HostError::TerminalRejected {
                detail: "cwd 必须是绝对路径".to_owned(),
            });
        }
        let canonical = path
            .canonicalize()
            .map_err(|_| HostError::TerminalRejected {
                detail: "cwd 不存在".to_owned(),
            })?;
        if !canonical.is_dir() {
            return Err(HostError::TerminalRejected {
                detail: "cwd 不是目录".to_owned(),
            });
        }
        if !canonical.starts_with(&root) {
            return Err(HostError::TerminalRejected {
                detail: "cwd 逃逸出会话工作目录".to_owned(),
            });
        }
        Ok(canonical)
    }
}

/// 一个已登记的终端任务。
struct Terminal {
    id: String,
    tree: Arc<ProcessTree>,
    output: Mutex<OutputBuffer>,
    exit: Mutex<Option<TerminalExitStatus>>,
    exited: Notify,
    /// 驱动任务已经结束（退出事实已记录且输出已排空或排空预算到期）。
    ///
    /// 退出事实现在与管道 EOF 解耦，先于排空完成；关闭/释放路径必须等它，才能保证
    /// 「返回后不会再有 `terminal.output` 事件」。
    drained: AtomicBool,
    drained_notify: Notify,
}

impl Terminal {
    /// 记录退出事实（**只记一次**：kill、自然退出与关闭竞争都收敛到这里）。
    fn record_exit(&self, status: Option<std::process::ExitStatus>) {
        let exit = crate::platform::exit_status(status);
        let status = TerminalExitStatus {
            exit_code: exit.exit_code,
            signal: exit.signal,
            meta: None,
        };
        let mut slot = lock(&self.exit);
        if slot.is_none() {
            *slot = Some(status);
            drop(slot);
            self.exited.notify_waiters();
        }
    }

    /// 标记驱动任务结束（输出已排空）；只通知一次。
    fn mark_drained(&self) {
        if !self.drained.swap(true, Ordering::SeqCst) {
            self.drained_notify.notify_waiters();
        }
    }
}

/// 一个响应的 `result` 对象可用的字节预算：ACP 单条消息上限减去 JSON-RPC 信封。
///
/// 信封形如 `{"jsonrpc":"2.0","id":<id>,"result":<result>}`。`id` 按**回填后**的长度计入：
/// 字符串 id 会先解码再重新转义（通常不变或更短），数字文本可能被规范化（`1e3` → `1000.0`），
/// 因此不能用原字面量长度直接估算。
#[must_use]
pub(crate) fn result_frame_budget(id: &acp_protocol::IdLiteral) -> usize {
    const WRAPPER: usize = "{\"jsonrpc\":\"2.0\",\"id\":,\"result\":}".len();
    // 信封回填时 `message::response` 会先把 id 字面量解析成 `Value` 再序列化：数字文本可能被规范化
    // （`1e3` → `1000.0`），因此按**回填后**的长度计算，而不是原字面量长度。
    let id_len = serde_json::from_str::<serde_json::Value>(id.as_json())
        .map(|value| value.to_string().len())
        .unwrap_or_else(|_| id.as_json().len());
    limits::MAX_MESSAGE_BYTES.saturating_sub(WRAPPER.saturating_add(id_len))
}

/// 把保留正文裁到帧预算内：从**开头**丢弃、落在字符边界，并置 `truncated`。
///
/// 预算按**实际 JSON 转义后**的字节数计算（`\n` 是 2 字节、ANSI 的 ESC 是 6 字节、普通 UTF-8
/// 字符按原字节数），因此不会出现「正文恰好 1 MiB、序列化后超过 1 MiB」的悬挂请求。
fn fit_to_frame(
    output: String,
    truncated: bool,
    exit_status: Option<&TerminalExitStatus>,
    frame_budget: usize,
) -> (String, bool) {
    // 先用空正文量出 `result` 的固定部分（字段名、truncated、exitStatus、外层括号）。
    let base = TerminalOutputResponse {
        output: String::new(),
        truncated,
        exit_status: exit_status.cloned(),
        meta: None,
    };
    let Ok(base) = serde_json::to_string(&base) else {
        return (String::new(), true);
    };
    let Some(remaining) = frame_budget.checked_sub(base.len()) else {
        return (String::new(), true);
    };

    // 反向累加每个字符的 JSON 开销，保留放得下的最长后缀。
    let mut cost = 0usize;
    let mut start = output.len();
    for (index, ch) in output.char_indices().rev() {
        let char_cost = json_string_cost(ch);
        if cost.saturating_add(char_cost) > remaining {
            break;
        }
        cost += char_cost;
        start = index;
    }
    if start == 0 {
        (output, truncated)
    } else {
        (output[start..].to_owned(), true)
    }
}

/// 一个字符在 `serde_json` 字符串字面量里的字节数（与默认序列化规则一致）。
///
/// 只转义 `"`、`\` 与 U+0000–U+001F；控制字符中最常见的是 `\n`（2 字节）与 ESC（`\u001b`，
/// 6 字节），ANSI 序列因此会明显膨胀。
fn json_string_cost(ch: char) -> usize {
    match ch {
        '"' | '\\' | '\u{08}' | '\u{0C}' | '\n' | '\r' | '\t' => 2,
        ch if (ch as u32) < 0x20 => 6,
        ch => ch.len_utf8(),
    }
}

/// 等待驱动任务结束（输出已排空）；调用方负责超时。
async fn await_drained(terminal: &Terminal) {
    loop {
        // 先登记 waiter 再判状态：反过来的顺序会丢掉 `notify_waiters`。
        let notified = terminal.drained_notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if terminal.drained.load(Ordering::SeqCst) {
            return;
        }
        notified.await;
    }
}

/// 有界输出缓冲：从开头丢弃超预算字节，且始终落在字符边界。
struct OutputBuffer {
    bytes: Vec<u8>,
    truncated: bool,
    limit: usize,
    next_chunk: u64,
}

impl OutputBuffer {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            truncated: false,
            limit,
            next_chunk: 0,
        }
    }

    /// 追加一段（已解码的）输出并分配本分块序号。
    fn push(&mut self, text: &str) -> u64 {
        let index = self.next_chunk;
        self.next_chunk += 1;
        if !text.is_empty() {
            self.bytes.extend_from_slice(text.as_bytes());
            if self.bytes.len() > self.limit {
                let mut cut = self.bytes.len() - self.limit;
                // 不能从 UTF-8 续字节开始：向前推进到下一个字符边界。
                while cut < self.bytes.len() && (self.bytes[cut] & 0xC0) == 0x80 {
                    cut += 1;
                }
                if cut > 0 {
                    self.bytes.drain(..cut);
                    self.truncated = true;
                }
            }
        }
        index
    }

    fn snapshot(&self) -> (String, bool) {
        (
            String::from_utf8_lossy(&self.bytes).into_owned(),
            self.truncated,
        )
    }
}

/// 增量 UTF-8 解码：读取边界可能落在多字节字符中间，未完成的后缀留到下一次读取。
#[derive(Default)]
struct Utf8Decoder {
    pending: Vec<u8>,
}

impl Utf8Decoder {
    fn decode(&mut self, data: &[u8]) -> String {
        self.pending.extend_from_slice(data);
        let mut out = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(text) => {
                    out.push_str(text);
                    self.pending.clear();
                    break;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    if valid > 0 {
                        let prefix =
                            std::str::from_utf8(&self.pending[..valid]).unwrap_or_default();
                        out.push_str(prefix);
                        self.pending.drain(..valid);
                    }
                    match error.error_len() {
                        // 非法字节（不是「还没收完」）：用替换字符消费掉，避免永久卡住。
                        Some(length) => {
                            out.push('\u{FFFD}');
                            self.pending.drain(..length);
                        }
                        None => break,
                    }
                }
            }
        }
        out
    }

    /// 输入结束时把剩余的不完整字节按替换字符输出（不静默丢弃）。
    fn finish(&mut self) -> String {
        let out = String::from_utf8_lossy(&self.pending).into_owned();
        self.pending.clear();
        out
    }
}

/// 驱动任务：读 stdout/stderr、交付真实增量事件、等待退出并记录唯一退出事实。
///
/// **退出事实与管道 EOF 解耦**：`wait` 与管道读取并发，`wait` 一返回就记录退出事实；随后只在
/// [`limits::TERMINAL_DRAIN_TIMEOUT`] 内继续排空已到达的输出。后台子进程继承并持有 stdout/stderr
/// 写端时 EOF 可能永远不来——把「两个管道 EOF」当作退出前提会让 `terminal/wait_for_exit` 永久悬挂，
/// 而进程本身早已退出。
async fn drive(
    mut child: tokio::process::Child,
    mut stdout: tokio::process::ChildStdout,
    mut stderr: tokio::process::ChildStderr,
    terminal: Arc<Terminal>,
    sink: Arc<Mutex<EventSink>>,
    clock: Arc<dyn Clock>,
) {
    let mut out_decoder = Utf8Decoder::default();
    let mut err_decoder = Utf8Decoder::default();
    let mut out_open = true;
    let mut err_open = true;
    let mut out_buffer = vec![0u8; limits::TERMINAL_CHUNK_BYTES];
    let mut err_buffer = vec![0u8; limits::TERMINAL_CHUNK_BYTES];

    let wait = child.wait();
    tokio::pin!(wait);
    let mut exited = false;

    // 退出之前：读管道与 `wait` 并发。`wait` 分支放在 `biased` 的第一位：即使输出持续可读，
    // 退出事实也不会被输出洪流拖延。
    while !exited {
        if !out_open && !err_open {
            // 两个管道都已 EOF：进程即将退出，等 `wait` 给出退出事实。
            terminal.record_exit(wait.as_mut().await.ok());
            break;
        }
        tokio::select! {
            biased;
            status = wait.as_mut() => {
                exited = true;
                terminal.record_exit(status.ok());
            }
            open = read_pipe(
                &mut stdout,
                &mut out_decoder,
                &mut out_buffer,
                &terminal,
                "stdout",
                &sink,
                clock.as_ref(),
            ), if out_open => {
                out_open = open;
            }
            open = read_pipe(
                &mut stderr,
                &mut err_decoder,
                &mut err_buffer,
                &terminal,
                "stderr",
                &sink,
                clock.as_ref(),
            ), if err_open => {
                err_open = open;
            }
        }
    }

    // 退出后：后台进程可能仍持有管道写端，EOF 不保证到来；只在排空预算内继续交付已到达的输出。
    // 到期即停止读取——这些管道持有者不是本终端的退出依据。
    if out_open || err_open {
        let drain = async {
            while out_open || err_open {
                tokio::select! {
                    biased;
                    open = read_pipe(
                        &mut stdout,
                        &mut out_decoder,
                        &mut out_buffer,
                        &terminal,
                        "stdout",
                        &sink,
                        clock.as_ref(),
                    ), if out_open => {
                        out_open = open;
                    }
                    open = read_pipe(
                        &mut stderr,
                        &mut err_decoder,
                        &mut err_buffer,
                        &terminal,
                        "stderr",
                        &sink,
                        clock.as_ref(),
                    ), if err_open => {
                        err_open = open;
                    }
                }
            }
        };
        let _ = tokio::time::timeout(limits::TERMINAL_DRAIN_TIMEOUT, drain).await;
    }

    // 退出事实已记录、已到达的输出已交付（或排空预算到期）：关闭/释放路径可以返回。
    terminal.mark_drained();
}

/// 从一条管道读一次并交付解码后的增量；返回管道是否仍然打开（`false` 表示 EOF 或读失败）。
async fn read_pipe<R>(
    pipe: &mut R,
    decoder: &mut Utf8Decoder,
    buffer: &mut [u8],
    terminal: &Terminal,
    stream: &str,
    sink: &Mutex<EventSink>,
    clock: &dyn Clock,
) -> bool
where
    R: tokio::io::AsyncRead + Unpin,
{
    match pipe.read(buffer).await {
        Ok(0) | Err(_) => {
            emit_text(terminal, stream, &decoder.finish(), sink, clock);
            false
        }
        Ok(read) => {
            let text = decoder.decode(&buffer[..read]);
            emit_text(terminal, stream, &text, sink, clock);
            true
        }
    }
}

fn emit_text(
    terminal: &Terminal,
    stream: &str,
    text: &str,
    sink: &Mutex<EventSink>,
    clock: &dyn Clock,
) {
    if text.is_empty() {
        return;
    }
    let index = lock(&terminal.output).push(text);
    emit_event(terminal, stream, index, text, sink, clock);
}

fn emit_event(
    terminal: &Terminal,
    stream: &str,
    index: u64,
    text: &str,
    sink: &Mutex<EventSink>,
    clock: &dyn Clock,
) {
    let event: EndpointEvent = match mapper::terminal_output_event(
        &terminal.id,
        index,
        stream,
        text,
        false,
        clock.now(),
    ) {
        Ok(event) => event,
        Err(_) => {
            tracing::warn!("terminal.output 事件无法构造（已跳过该分块）");
            return;
        }
    };
    lock(sink).send(event);
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use acp_core::model::{SessionId, Timestamp};

    use super::*;

    const SESSION: &str = "11111111-1111-4111-8111-111111111111";

    struct TestClock;

    impl Clock for TestClock {
        fn now(&self) -> Timestamp {
            Timestamp::new("2026-10-20T00:00:00.000Z").expect("timestamp")
        }
    }

    fn message_bytes(response: &TerminalOutputResponse, id: &str) -> usize {
        let id = acp_protocol::IdLiteral::from_literal(id).expect("id");
        let result = serde_json::to_value(response).expect("result");
        acp_protocol::message::response(&id, &result)
            .expect("响应必须能编码")
            .as_str()
            .len()
    }

    #[test]
    fn frame_trim_keeps_a_near_limit_plain_output_within_the_message_limit() {
        let id = "1";
        let output = "x".repeat(limits::MAX_MESSAGE_BYTES);
        let (trimmed, truncated) = fit_to_frame(
            output,
            false,
            None,
            result_frame_budget(&acp_protocol::IdLiteral::from_literal(id).expect("id")),
        );
        assert!(truncated, "超出帧预算必须置 truncated");
        assert!(
            trimmed.len() > 1_000_000,
            "纯文本只应裁掉信封开销：实际保留 {} 字节",
            trimmed.len()
        );
        let response = TerminalOutputResponse {
            output: trimmed,
            truncated,
            exit_status: None,
            meta: None,
        };
        assert!(
            message_bytes(&response, id) <= limits::MAX_MESSAGE_BYTES,
            "响应必须留在 ACP 单条消息上限内：{}",
            message_bytes(&response, id)
        );
    }

    #[test]
    fn frame_trim_leaves_small_output_untouched() {
        let id = "1";
        let budget = result_frame_budget(&acp_protocol::IdLiteral::from_literal(id).expect("id"));
        let (output, truncated) = fit_to_frame("hello\n".to_owned(), false, None, budget);
        assert_eq!(output, "hello\n");
        assert!(!truncated);
    }

    #[test]
    fn frame_trim_accounts_for_newlines_and_ansi_escapes() {
        let id = "1";
        let output = "\x1b[31mX\n".repeat(120_000);
        let budget = result_frame_budget(&acp_protocol::IdLiteral::from_literal(id).expect("id"));
        let (trimmed, truncated) = fit_to_frame(output.clone(), false, None, budget);
        assert!(truncated, "转义膨胀后必须再次裁剪");
        assert!(
            output.ends_with(&trimmed),
            "裁剪只从开头丢弃，保留的必须是原串后缀"
        );
        assert!(
            trimmed.len() > 500_000,
            "换行/ANSI 只应裁掉转义膨胀部分：实际保留 {} 字节",
            trimmed.len()
        );
        let response = TerminalOutputResponse {
            output: trimmed,
            truncated,
            exit_status: None,
            meta: None,
        };
        assert!(
            message_bytes(&response, id) <= limits::MAX_MESSAGE_BYTES,
            "含转义的响应同样必须留在上限内：{}",
            message_bytes(&response, id)
        );
    }

    #[test]
    fn frame_budget_accounts_for_a_normalized_numeric_id() {
        let id = "1e3";
        let output = "x".repeat(limits::MAX_MESSAGE_BYTES);
        let (trimmed, truncated) = fit_to_frame(
            output,
            false,
            None,
            result_frame_budget(&acp_protocol::IdLiteral::from_literal(id).expect("id")),
        );
        assert!(truncated);
        let response = TerminalOutputResponse {
            output: trimmed,
            truncated,
            exit_status: None,
            meta: None,
        };
        assert!(
            message_bytes(&response, id) <= limits::MAX_MESSAGE_BYTES,
            "id 回填后的长度也必须计入预算：{}",
            message_bytes(&response, id)
        );
    }

    #[test]
    fn json_string_cost_matches_serde_json() {
        let sample = "a\"b\\c\n\r\t\u{08}\u{0c}\u{01}\u{1b}\u{7f}\u{2028}🙂é/";
        let escaped = serde_json::to_string(sample).expect("json");
        let cost: usize = sample.chars().map(json_string_cost).sum();
        assert_eq!(escaped.len(), cost + 2, "转义长度必须与 serde_json 一致");
    }

    fn fake_agent_binary() -> PathBuf {
        let mut path = std::env::current_exe().expect("current exe");
        path.pop(); // deps
        path.pop(); // debug|release
        let base = path.join("acpr-fake-acp-agent");
        if base.exists() {
            return base;
        }
        let exe = base.with_extension("exe");
        assert!(
            exe.exists(),
            "找不到 fake ACP Agent 可执行文件（cargo test 必须构建 bin 目标）：{}",
            exe.display()
        );
        exe
    }

    fn launch_spec(program: &Path) -> crate::launch::LaunchSpec {
        let env = launch::NECESSARY_ENV
            .iter()
            .filter_map(|name| {
                std::env::var(name)
                    .ok()
                    .map(|value| ((*name).to_owned(), value))
            })
            .collect();
        crate::launch::LaunchSpec {
            program: program.to_string_lossy().into_owned(),
            args: Vec::new(),
            env,
        }
    }

    fn service(supervisor: Arc<Supervisor>) -> TerminalService {
        TerminalService::new(TerminalConfig {
            session: SessionId::new(SESSION).expect("session id"),
            workspace: std::env::temp_dir(),
            clock: Arc::new(TestClock),
            supervisor,
            sink: EventSink::new(|_| {}),
            enabled: true,
        })
    }

    fn terminal_request(program: &Path, mode: &str) -> CreateTerminalRequest {
        CreateTerminalRequest {
            session_id: SESSION.to_owned(),
            command: program.to_string_lossy().into_owned(),
            args: vec!["--terminal-child".to_owned(), mode.to_owned()],
            env: Vec::new(),
            cwd: None,
            output_byte_limit: None,
            meta: None,
        }
    }

    /// 标准关闭路径必须先结束进程树、再排空输出：同步关闭把终端放进 `closing`，
    /// 异步关闭据此等到驱动任务结束（输出已排空）。旧实现里 `drain()` 在同步关闭就消费了列表，
    /// 异步关闭拿到空表，最后的输出可能丢失。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_drains_output_and_records_the_exit_fact() {
        let program = fake_agent_binary();
        let (supervisor, _incoming) = Supervisor::start(launch_spec(&program))
            .await
            .expect("启动监督者");
        let service = service(Arc::clone(&supervisor));
        let id = service
            .create(&terminal_request(&program, "hang"))
            .expect("创建终端");

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let response = service.output(&id, 4096).expect("读取输出");
            if response.output.contains("started") {
                break;
            }
            assert!(Instant::now() < deadline, "终端没有写出启动行");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        service.shutdown_sync();
        service.shutdown().await;

        let closing = lock(&service.closing).clone();
        assert_eq!(closing.len(), 1, "同步关闭排空出的终端必须保留给异步关闭");
        let terminal = &closing[0];
        assert_eq!(
            lock(&terminal.output).snapshot().0,
            "started\n",
            "关闭后才能交付的输出不能丢"
        );
        assert!(
            lock(&terminal.exit).is_some(),
            "异步关闭必须等到驱动任务记录退出事实"
        );
        assert!(
            terminal.drained.load(Ordering::SeqCst),
            "异步关闭必须等到驱动任务结束（输出已排空）"
        );
        assert_eq!(service.terminal_count(), 0);
        supervisor.shutdown().await;
    }

    /// create 与关闭并发：关闭不能在 spawn 之后漏掉这个终端。
    /// 旧的「锁外判 closed、spawn 后再 insert」会让 create 在关闭排空之后登记一个新终端。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn create_racing_shutdown_never_leaves_a_registered_terminal() {
        let program = fake_agent_binary();
        let handle = tokio::runtime::Handle::current();
        for _ in 0..8 {
            let (supervisor, _incoming) = Supervisor::start(launch_spec(&program))
                .await
                .expect("启动监督者");
            let service = Arc::new(service(Arc::clone(&supervisor)));
            let request = terminal_request(&program, "hang");

            let creator_service = Arc::clone(&service);
            let creator_handle = handle.clone();
            let creator = std::thread::spawn(move || {
                let _guard = creator_handle.enter();
                creator_service.create(&request)
            });
            let closer_service = Arc::clone(&service);
            let closer = std::thread::spawn(move || closer_service.shutdown_sync());

            let created = creator.join().expect("create 线程");
            closer.join().expect("close 线程");
            service.shutdown().await;

            assert_eq!(
                service.terminal_count(),
                0,
                "关闭后不得留下已登记（含已 spawn）的终端"
            );
            assert!(service.is_closed());
            match created {
                Ok(id) => assert!(
                    matches!(service.find(&id), Err(HostError::SessionClosed)),
                    "成功返回的终端也必须已被关闭路径排空"
                ),
                Err(HostError::SessionClosed) | Err(HostError::TerminalRejected { .. }) => {}
                Err(other) => panic!("create 只允许成功或明确的关闭/拒绝结果：{other}"),
            }
            supervisor.shutdown().await;
        }
    }

    /// 命令退出但其后台子进程继承并持有 stdout/stderr 写端：EOF 永不到来。
    /// 旧实现把「两个管道 EOF」当作退出前提，`wait_for_exit`/`terminal/output` 会永久悬挂；
    /// 退出事实必须与管道 EOF 解耦（finding 1 的回归点）。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn exit_is_recorded_while_a_background_child_keeps_the_pipes_open() {
        let program = fake_agent_binary();
        let (supervisor, _incoming) = Supervisor::start(launch_spec(&program))
            .await
            .expect("启动监督者");
        let service = service(Arc::clone(&supervisor));
        let id = service
            .create(&terminal_request(&program, "exit-with-background"))
            .expect("创建终端");

        // 后台子进程一直持有写端，旧实现会卡在管道读取循环里，这里会超时。
        let response = tokio::time::timeout(Duration::from_secs(10), service.wait_for_exit(&id))
            .await
            .expect("退出事实不得被后台子进程持有的管道拖住")
            .expect("wait_for_exit");
        assert_eq!(
            response.exit_code,
            Some(7),
            "退出码必须原样上报给 wait_for_exit"
        );

        let output = service.output(&id, 4096).expect("读取输出");
        assert_eq!(
            output.exit_status.and_then(|status| status.exit_code),
            Some(7),
            "terminal/output 也必须带上退出事实"
        );
        // `wait` 可能先于管道排空完成：已到达的正文在排空预算内交付，因此轮询到它出现。
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let output = service.output(&id, 4096).expect("读取输出");
            if output.output.contains("exiting") {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "退出前的输出必须已交付：{:?}",
                output.output
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // release 结束整棵树（含后台子进程），驱动任务随之结束。
        service.release(&id).await.expect("release");
        supervisor.shutdown().await;
    }

    /// 一个没有驱动任务的终端：`exited` 永远不会被通知。
    /// 用来把「关闭唤醒」与「退出唤醒」分开，避免真实进程退出掩盖丢失的关闭通知。
    fn dummy_terminal(id: &str) -> Arc<Terminal> {
        let mut command = Command::new("unused");
        let tree = ProcessTree::prepare(&mut command).expect("进程树句柄");
        Arc::new(Terminal {
            id: id.to_owned(),
            tree: Arc::new(tree),
            output: Mutex::new(OutputBuffer::new(4096)),
            exit: Mutex::new(None),
            exited: Notify::new(),
            drained: AtomicBool::new(false),
            drained_notify: Notify::new(),
        })
    }

    /// 会话关闭必须唤醒 `wait_for_exit`，即使终端本身永远不会退出、`exited` 永不被通知。
    /// 旧实现只在 `select!` 里创建 `closed_notify.notified()`：通知一旦落在 `is_closed()` 检查与
    /// 登记之间就永久丢失，等待者再无唤醒源（finding 2 的回归点）。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn wait_for_exit_wakes_on_session_close_without_an_exit_fact() {
        let program = fake_agent_binary();
        let (supervisor, _incoming) = Supervisor::start(launch_spec(&program))
            .await
            .expect("启动监督者");
        let service = Arc::new(service(Arc::clone(&supervisor)));
        lock(&service.terminals).insert(
            "term-never-exits".to_owned(),
            Some(dummy_terminal("term-never-exits")),
        );

        let waiter = {
            let service = Arc::clone(&service);
            tokio::spawn(async move { service.wait_for_exit("term-never-exits").await })
        };
        // 让等待者通过 is_closed() 检查并停在 select 上（多线程 runtime 上确实会被调度）。
        tokio::time::sleep(Duration::from_millis(50)).await;
        service.shutdown_sync();

        let result = tokio::time::timeout(Duration::from_secs(5), waiter)
            .await
            .expect("关闭必须唤醒等待者")
            .expect("等待任务");
        assert!(
            matches!(&result, Err(HostError::SessionClosed)),
            "关闭后 wait_for_exit 必须显式失败：{result:?}"
        );
        supervisor.shutdown().await;
    }

    /// 竞态回归：多轮把 `shutdown_sync` 与 `wait_for_exit` 的「登记 + 判定」交错执行，
    /// 每一轮都必须以 `SessionClosed` 收敛（不能因丢失关闭唤醒而挂到退出事实上）。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn close_race_never_loses_the_wakeup() {
        let program = fake_agent_binary();
        let (supervisor, _incoming) = Supervisor::start(launch_spec(&program))
            .await
            .expect("启动监督者");
        for round in 0..64 {
            let service = Arc::new(service(Arc::clone(&supervisor)));
            let id = format!("term-race-{round}");
            lock(&service.terminals).insert(id.clone(), Some(dummy_terminal(&id)));

            let mut waiters = Vec::new();
            for _ in 0..4 {
                let service = Arc::clone(&service);
                let id = id.clone();
                waiters.push(tokio::spawn(
                    async move { service.wait_for_exit(&id).await },
                ));
            }
            let closer = {
                let service = Arc::clone(&service);
                std::thread::spawn(move || service.shutdown_sync())
            };
            closer.join().expect("关闭线程");

            for waiter in waiters {
                let result = tokio::time::timeout(Duration::from_secs(5), waiter)
                    .await
                    .expect("关闭唤醒不得丢失")
                    .expect("等待任务");
                assert!(
                    matches!(&result, Err(HostError::SessionClosed)),
                    "第 {round} 轮：关闭后必须显式失败：{result:?}"
                );
            }
        }
        supervisor.shutdown().await;
    }
}
