//! 进程监督：启动、stdio 分帧、请求登记、超时、stderr 有界采集与关闭顺序。
//!
//! 所有权图（`AGENTS.md` §7「异步任务必须有所有者、取消路径与关闭顺序」）：
//!
//! - [`Supervisor`] 拥有 4 个子任务：`stdout` 读取、`stderr` 采集、子进程退出监视、写出队列。
//! - `stdout` 任务把「响应」直接结算给对应的未完成请求，把「通知/请求」投给上层路由器。
//! - 退出监视在进程退出时把全部未完成请求以明确错误收敛，并唤醒关闭路径。
//! - [`Supervisor::shutdown`] 按顺序执行：停止接受新请求 → 未完成请求失败 → 关闭 stdin →
//!   等 grace → 结束进程树 → join 子任务。没有任何 detached task。
//!
//! `session/prompt` 的响应不设超时：它到达时表示 turn 结束，由会话层据此产出终态事件。

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use acp_protocol::{Envelope, RawDocument};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::sync::{Notify, mpsc, oneshot};

use crate::error::HostError;
use crate::launch::LaunchSpec;
use crate::limits;
use crate::platform::ProcessTree;

type Pending = HashMap<u64, oneshot::Sender<Result<Value, HostError>>>;

/// 退出状态（供失败分类与日志使用；不含任何正文）。
#[derive(Debug)]
struct ExitState {
    done: AtomicBool,
    status: Mutex<Option<String>>,
    notify: Notify,
}

impl ExitState {
    fn new() -> Self {
        Self {
            done: AtomicBool::new(false),
            status: Mutex::new(None),
            notify: Notify::new(),
        }
    }

    fn status(&self) -> Option<String> {
        self.status.lock().ok().and_then(|slot| slot.clone())
    }

    fn mark(&self, status: String) {
        if let Ok(mut slot) = self.status.lock() {
            *slot = Some(status);
        }
        self.done.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }
}

/// stderr 环形缓冲（有界）。
#[derive(Debug, Default)]
struct StderrRing {
    buf: VecDeque<u8>,
    dropped: u64,
}

impl StderrRing {
    fn push(&mut self, bytes: &[u8]) {
        self.buf.extend(bytes.iter().copied());
        while self.buf.len() > limits::STDERR_RING_BYTES {
            self.buf.pop_front();
            self.dropped += 1;
        }
    }

    fn snapshot(&self) -> (String, u64) {
        let bytes: Vec<u8> = self.buf.iter().copied().collect();
        (String::from_utf8_lossy(&bytes).into_owned(), self.dropped)
    }
}

/// 一个 Agent 进程的监督者。
pub struct Supervisor {
    program: String,
    tree: Arc<ProcessTree>,
    outbound: Mutex<Option<mpsc::UnboundedSender<Vec<u8>>>>,
    pending: Arc<Mutex<Pending>>,
    next_id: AtomicU64,
    exit: Arc<ExitState>,
    stderr: Arc<Mutex<StderrRing>>,
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    closing: AtomicBool,
    /// 已经协商好的能力（首次 `initialize` 的结果）。
    capabilities: Mutex<Option<acp_protocol::capability::AgentCapabilities>>,
}

impl std::fmt::Debug for Supervisor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Supervisor")
            .field("program", &self.program)
            .field("running", &self.is_running())
            .finish_non_exhaustive()
    }
}

impl Supervisor {
    /// 启动 Agent 进程并开始监督。
    ///
    /// 返回监督者与**进站消息**通道：通道里是已分类的信封（通知与 agent→client 请求），
    /// 响应不进通道（它们在 `stdout` 任务里直接结算给对应的未完成请求）。
    pub async fn start(
        spec: LaunchSpec,
    ) -> Result<(Arc<Self>, mpsc::UnboundedReceiver<Envelope>), HostError> {
        let mut command = Command::new(&spec.program);
        command.args(&spec.args);
        // 子进程环境**先清空**再逐项注入：未列入白名单的宿主变量（含任何密钥）都不得进入。
        command.env_clear();
        for (name, value) in &spec.env {
            command.env(name, value);
        }
        command.stdin(std::process::Stdio::piped());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        command.kill_on_drop(false);

        let tree = ProcessTree::prepare(&mut command)?;
        let mut child = command.spawn().map_err(|error| HostError::SpawnFailed {
            detail: error.to_string(),
        })?;
        // spawn 之后、写 stdin 之前立刻挂进进程树。
        if let Err(error) = tree.attach(&child) {
            // 已经 spawn 出来了：任何后续失败都必须结束它，不能把孤儿留给系统。
            tree.terminate();
            let _ = child.start_kill();
            return Err(error);
        }

        let pipes = (|| -> Result<(_, _, _), HostError> {
            let stdin = child.stdin.take().ok_or_else(|| HostError::SpawnFailed {
                detail: "stdin 未建立管道".to_owned(),
            })?;
            let stdout = child.stdout.take().ok_or_else(|| HostError::SpawnFailed {
                detail: "stdout 未建立管道".to_owned(),
            })?;
            let stderr = child.stderr.take().ok_or_else(|| HostError::SpawnFailed {
                detail: "stderr 未建立管道".to_owned(),
            })?;
            Ok((stdin, stdout, stderr))
        })();
        let (stdin, stdout, stderr) = match pipes {
            Ok(pipes) => pipes,
            Err(error) => {
                tree.terminate();
                let _ = child.start_kill();
                return Err(error);
            }
        };

        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
        let pending: Arc<Mutex<Pending>> = Arc::new(Mutex::new(HashMap::new()));
        let exit = Arc::new(ExitState::new());
        let stderr_ring = Arc::new(Mutex::new(StderrRing::default()));

        let tree = Arc::new(tree);
        let supervisor = Arc::new(Self {
            program: spec.program.clone(),
            tree: Arc::clone(&tree),
            outbound: Mutex::new(Some(outbound_tx)),
            pending: Arc::clone(&pending),
            next_id: AtomicU64::new(1),
            exit: Arc::clone(&exit),
            stderr: Arc::clone(&stderr_ring),
            tasks: Mutex::new(Vec::new()),
            closing: AtomicBool::new(false),
            capabilities: Mutex::new(None),
        });

        let tasks = vec![
            tokio::spawn(write_loop(outbound_rx, stdin)),
            tokio::spawn(read_loop(
                stdout,
                Arc::clone(&pending),
                incoming_tx,
                Arc::clone(&exit),
                Arc::clone(&tree),
            )),
            tokio::spawn(stderr_loop(stderr, stderr_ring)),
            tokio::spawn(wait_loop(child, Arc::clone(&exit), pending)),
        ];
        if let Ok(mut slot) = supervisor.tasks.lock() {
            *slot = tasks;
        }

        Ok((supervisor, incoming_rx))
    }

    /// 进程是否仍在运行。
    #[must_use]
    pub fn is_running(&self) -> bool {
        !self.exit.done.load(Ordering::SeqCst) && !self.closing.load(Ordering::SeqCst)
    }

    /// 进程是否已经退出（用于失败路径的判定）。
    #[must_use]
    pub fn has_exited(&self) -> bool {
        self.exit.done.load(Ordering::SeqCst)
    }

    /// 退出状态的可读描述。
    #[must_use]
    pub fn exit_status(&self) -> Option<String> {
        self.exit.status()
    }

    /// stderr 快照与丢弃计数（测试与诊断用；内容已按上限截断）。
    #[must_use]
    pub fn stderr_snapshot(&self) -> (String, u64) {
        self.stderr
            .lock()
            .map(|ring| ring.snapshot())
            .unwrap_or_else(|_| (String::new(), 0))
    }

    /// 已协商的能力（未协商时为 `None`）。
    #[must_use]
    pub fn negotiated_capabilities(&self) -> Option<acp_protocol::capability::AgentCapabilities> {
        self.capabilities.lock().ok().and_then(|slot| slot.clone())
    }

    /// 记录协商结果。
    pub fn set_negotiated_capabilities(
        &self,
        capabilities: acp_protocol::capability::AgentCapabilities,
    ) {
        if let Ok(mut slot) = self.capabilities.lock() {
            *slot = Some(capabilities);
        }
    }

    /// 发送一条请求并登记等待（**不**阻塞）。
    pub fn begin_request(
        &self,
        method: &str,
        params: &Value,
    ) -> Result<oneshot::Receiver<Result<Value, HostError>>, HostError> {
        if !self.is_running() {
            return Err(self.not_running_error());
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let document = acp_protocol::message::request_with_u64_id(id, method, params)?;
        let (tx, rx) = oneshot::channel();
        if let Ok(mut pending) = self.pending.lock() {
            pending.insert(id, tx);
        }
        self.write_document(&document)?;
        Ok(rx)
    }

    /// 发送一条请求并等待结果（有超时的短请求）。
    pub async fn request(
        &self,
        method: &str,
        params: &Value,
        timeout: Duration,
    ) -> Result<Value, HostError> {
        let rx = self.begin_request(method, params)?;
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(self.not_running_error()),
            Err(_) => Err(HostError::Timeout {
                method: method.to_owned(),
            }),
        }
    }

    /// 发送一条通知（没有响应）。
    pub fn send_notification(&self, method: &str, params: &Value) -> Result<(), HostError> {
        if !self.is_running() {
            return Err(self.not_running_error());
        }
        let document = acp_protocol::message::notification(method, params)?;
        self.write_document(&document)
    }

    /// 发送一条响应（回答 Agent 发来的请求）。
    pub fn send_response(
        &self,
        id: &acp_protocol::IdLiteral,
        result: &Value,
    ) -> Result<(), HostError> {
        let document = acp_protocol::message::response(id, result)?;
        self.write_document(&document)
    }

    /// 发送一条错误响应（显式不支持等）。
    pub fn send_error_response(
        &self,
        id: &acp_protocol::IdLiteral,
        code: i64,
        message: &str,
    ) -> Result<(), HostError> {
        let document = acp_protocol::message::error_response(id, code, message)?;
        self.write_document(&document)
    }

    fn write_document(&self, document: &RawDocument) -> Result<(), HostError> {
        let mut payload = Vec::with_capacity(document.len() + 1);
        payload.extend_from_slice(document.bytes());
        payload.push(b'\n');
        let sender = self
            .outbound
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().cloned())
            .ok_or_else(|| self.not_running_error())?;
        sender.send(payload).map_err(|_| self.not_running_error())
    }

    fn not_running_error(&self) -> HostError {
        match self.exit.status() {
            Some(status) => HostError::AgentExited { status },
            None => HostError::NotRunning,
        }
    }

    /// 立即结束整棵进程树（不等 grace）。
    ///
    /// 用于必须马上停止 Agent 的失败路径（stdout 超限、显式终止）与回归测试；正常关闭路径用
    /// [`Supervisor::shutdown`] 的 grace 顺序。
    pub fn terminate_tree(&self) {
        self.tree.terminate();
    }

    /// 等待进程退出（不主动杀死它）。
    async fn wait_exit(&self) {
        loop {
            // 先登记 waiter 再判 done：`notify_waiters` 只唤醒已注册的 waiter，
            // 反过来的顺序会丢掉唤醒，让关闭路径白等整个 grace。
            let notified = self.exit.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.exit.done.load(Ordering::SeqCst) {
                return;
            }
            notified.await;
        }
    }

    /// 关闭顺序（见模块文档）。
    pub async fn shutdown(&self) {
        if self.closing.swap(true, Ordering::SeqCst) {
            return;
        }
        // 1) 未完成请求以明确错误结束（不静默悬挂）。
        self.fail_pending(|| HostError::ShutdownPending);
        // 2) 关闭 stdin：丢弃写出队列的发送端，写出任务随之结束并关闭 stdin（EOF）。
        let sender = self.outbound.lock().ok().and_then(|mut slot| slot.take());
        drop(sender);
        // 3) 等 grace；超时则结束整棵进程树。
        if tokio::time::timeout(limits::SHUTDOWN_GRACE, self.wait_exit())
            .await
            .is_err()
        {
            self.tree.terminate();
            let _ = tokio::time::timeout(limits::EXIT_DRAIN_TIMEOUT, self.wait_exit()).await;
        }
        // 4) 释放句柄（Windows：关闭 Job 句柄 ⇒ KILL_ON_JOB_CLOSE；Unix：结束进程组）。
        self.tree.close();
        // 5) join 全部子任务（先给它们一点时间自然结束，再兜底 abort）。
        let handles: Vec<_> = self
            .tasks
            .lock()
            .map(|mut slot| slot.drain(..).collect())
            .unwrap_or_default();
        for mut handle in handles {
            if tokio::time::timeout(Duration::from_secs(1), &mut handle)
                .await
                .is_err()
            {
                // 子任务没有及时结束：显式 abort，不留 detached task。
                handle.abort();
                let _ = handle.await;
            }
        }
    }

    /// 把一个外部任务登记进本监督者的任务集：**所有权仍在本结构**，关闭时一并 join。
    pub fn adopt(&self, handle: tokio::task::JoinHandle<()>) {
        if let Ok(mut slot) = self.tasks.lock() {
            slot.push(handle);
        }
    }

    /// 起一个由本监督者拥有的任务（turn 等待、进站路由等）。
    pub fn spawn_owned<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let handle = tokio::spawn(future);
        self.adopt(handle);
    }

    fn fail_pending(&self, error: impl Fn() -> HostError) {
        let drained: Vec<(u64, oneshot::Sender<Result<Value, HostError>>)> = self
            .pending
            .lock()
            .map(|mut pending| pending.drain().collect())
            .unwrap_or_default();
        for (_, sender) in drained {
            let _ = sender.send(Err(error()));
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        // 结构被丢弃时进程树也必须结束：不能留下孤儿 Agent。
        self.tree.close();
    }
}

/// 写出任务：把行缓冲写到 stdin。
async fn write_loop(
    mut rx: mpsc::UnboundedReceiver<Vec<u8>>,
    mut stdin: tokio::process::ChildStdin,
) {
    while let Some(payload) = rx.recv().await {
        if stdin.write_all(&payload).await.is_err() {
            break;
        }
        if stdin.flush().await.is_err() {
            break;
        }
    }
    // 结束前显式关闭 stdin（EOF），让 Agent 有时间自行退出。
    let _ = stdin.shutdown().await;
}

/// stdout 读取任务：按 LF 分帧、限制单条上限、结算响应、投递进站消息。
///
/// 分帧的关键是**持久缓冲**：一次 `read` 可能带回多行，也可能带回半行；缓冲区必须跨读保留，
/// 否则「一次读里包含两行」会丢掉第二行之后的全部内容。
async fn read_loop(
    mut stdout: tokio::process::ChildStdout,
    pending: Arc<Mutex<Pending>>,
    incoming: mpsc::UnboundedSender<Envelope>,
    exit: Arc<ExitState>,
    tree: Arc<crate::platform::ProcessTree>,
) {
    let mut buffer: Vec<u8> = Vec::new();
    let mut scratch = vec![0u8; 8192];
    loop {
        if let Some(position) = buffer.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = buffer.drain(..=position).collect();
            line.pop(); // 去掉分帧用的 `\n`
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if !handle_line(&line, &pending, &incoming, &exit, &tree) {
                break;
            }
            continue;
        }
        if buffer.len() > limits::MAX_MESSAGE_BYTES {
            // 超限不是「忽略这一条」：按 `SECURITY_DESIGN.md` §12.2 必须结束该 Agent，
            // 否则未完成请求会永久悬挂、坏进程还会被当成「仍在运行」继续复用。
            abort_agent(
                "stdout 单条消息超限",
                limits::MAX_MESSAGE_BYTES,
                buffer.len(),
                &pending,
                &exit,
                &tree,
            );
            break;
        }
        match stdout.read(&mut scratch).await {
            Ok(0) => {
                // EOF：最后一段没有换行时按一帧处理，避免静默丢弃。
                if !buffer.is_empty() {
                    let line = std::mem::take(&mut buffer);
                    let _ = handle_line(&line, &pending, &incoming, &exit, &tree);
                }
                break;
            }
            Ok(read) => buffer.extend_from_slice(&scratch[..read]),
            Err(_) => break,
        }
    }
}

/// 结束该 Agent 并收敛全部未完成请求（超限、协议破坏等必须显式停止而非跳过）。
fn abort_agent(
    reason: &str,
    limit: usize,
    actual: usize,
    pending: &Arc<Mutex<Pending>>,
    exit: &Arc<ExitState>,
    tree: &crate::platform::ProcessTree,
) {
    tracing::error!(reason, limit, actual, "ACP stdout 违反上限，结束该 Agent");
    // 顺序即契约（`local-agent-host` 增量规范「超限结束的失败关闭顺序」）：先标记退出
    // （`is_running()` 立即为假，坏 runtime 不会被继续复用），再唤醒等待中的请求，最后结束整棵树。
    // 反过来则被错误唤醒的调用方会在「错误已可见、Agent 仍显示在运行」的窗口里观察到不一致。
    exit.mark(format!("ACP 消息超限（{actual} > {limit} 字节）"));
    let drained: Vec<(u64, oneshot::Sender<Result<Value, HostError>>)> = pending
        .lock()
        .map(|mut map| map.drain().collect())
        .unwrap_or_default();
    for (_, sender) in drained {
        let _ = sender.send(Err(HostError::Protocol(acp_protocol::AcpError::Oversize {
            limit,
            actual,
        })));
    }
    tree.terminate();
}

/// 处理一帧；返回 `false` 表示进站通道已关闭（上层不再关心）。
fn handle_line(
    line: &[u8],
    pending: &Arc<Mutex<Pending>>,
    incoming: &mpsc::UnboundedSender<Envelope>,
    exit: &Arc<ExitState>,
    tree: &crate::platform::ProcessTree,
) -> bool {
    if line.is_empty() {
        return true;
    }
    if line.len() > limits::MAX_MESSAGE_BYTES {
        // 与「尚未收完就超限」同一条判据：结束该 Agent，而不是静默跳过这一条。
        abort_agent(
            "stdout 单条消息超限",
            limits::MAX_MESSAGE_BYTES,
            line.len(),
            pending,
            exit,
            tree,
        );
        return false;
    }
    let Ok(text) = std::str::from_utf8(line) else {
        tracing::warn!("ACP stdout 出现非 UTF-8 消息，忽略该条");
        return true;
    };
    let document = match RawDocument::parse(text.to_owned()) {
        Ok(document) => document,
        Err(error) => {
            tracing::warn!(error = %error, "ACP stdout 出现非法 JSON，忽略该条");
            return true;
        }
    };
    let envelope = match Envelope::classify(document) {
        Ok(envelope) => envelope,
        Err(error) => {
            tracing::warn!(error = %error, "ACP stdout 出现非法信封，忽略该条");
            return true;
        }
    };
    if envelope.class() == acp_protocol::MessageClass::Response {
        let id = envelope
            .id()
            .and_then(acp_protocol::IdLiteral::number_text)
            .and_then(|text| text.parse::<u64>().ok());
        match id {
            Some(id) => {
                let sender = pending.lock().ok().and_then(|mut map| map.remove(&id));
                match sender {
                    Some(sender) => {
                        let _ = sender.send(settle(&envelope));
                    }
                    None => tracing::warn!(id, "收到与任何未完成请求都不匹配的响应（协议错误）"),
                }
            }
            // 字符串 id（Zed 的形态）：本 crate 只发数字 id，因此不可能匹配得上。
            None => tracing::warn!("响应的 id 不是本进程发出的数字 id（协议错误）"),
        }
        return true;
    }
    incoming.send(envelope).is_ok()
}

/// 把响应结算成结果。
fn settle(envelope: &Envelope) -> Result<Value, HostError> {
    match envelope.error_info() {
        Some((code, message)) => Err(HostError::AgentRejected { code, message }),
        None => envelope.result().map_err(HostError::from),
    }
}

/// stderr 采集：有界环形缓冲 + 丢弃计数（内容绝不进入 ACP 通道）。
async fn stderr_loop(mut stderr: tokio::process::ChildStderr, ring: Arc<Mutex<StderrRing>>) {
    let mut scratch = vec![0u8; 8192];
    let mut received: u64 = 0;
    let mut announced: u64 = 0;
    let mut last_log = std::time::Instant::now();
    loop {
        match stderr.read(&mut scratch).await {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                received += read as u64;
                let dropped = if let Ok(mut ring) = ring.lock() {
                    ring.push(&scratch[..read]);
                    ring.dropped
                } else {
                    0
                };
                // 结构化日志只记计数与字节数：stderr 可能夹带密钥或 prompt 片段，
                // 因此内容**永不**进日志（`SECURITY_DESIGN.md` §14.1「默认日志允许字段」、
                // `MODULE_ARCHITECTURE.md` §4.5）。
                // 限频：最多每秒一条丢弃计数日志（超限丢弃最旧数据本来就是持续行为）。
                if dropped > announced && last_log.elapsed() >= std::time::Duration::from_secs(1) {
                    announced = dropped;
                    last_log = std::time::Instant::now();
                    tracing::warn!(
                        dropped_bytes = dropped,
                        retained_bytes = limits::STDERR_RING_BYTES,
                        "agent stderr 超出环形缓冲上限，已丢弃最旧字节"
                    );
                }
            }
        }
    }
    // 结束日志读**最终**计数：限频只影响 warn 的播报节奏，不应让汇总少报。
    let dropped_bytes = ring.lock().map(|ring| ring.dropped).unwrap_or(announced);
    tracing::info!(
        received_bytes = received,
        dropped_bytes,
        "agent stderr 采集结束"
    );
}

/// 退出监视：进程退出时收敛未完成请求并唤醒关闭路径。
async fn wait_loop(mut child: Child, exit: Arc<ExitState>, pending: Arc<Mutex<Pending>>) {
    let status = match child.wait().await {
        Ok(status) => format!("{status}"),
        Err(error) => format!("wait 失败：{error}"),
    };
    let drained: Vec<(u64, oneshot::Sender<Result<Value, HostError>>)> = pending
        .lock()
        .map(|mut map| map.drain().collect())
        .unwrap_or_default();
    for (_, sender) in drained {
        let _ = sender.send(Err(HostError::AgentExited {
            status: status.clone(),
        }));
    }
    tracing::info!(status = %status, "Agent 进程退出");
    exit.mark(status);
}
