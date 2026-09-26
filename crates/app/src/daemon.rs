//! Daemon 的启动与关闭序列（`design.md` 决策 2/4、`SECURITY_DESIGN.md` §12.1、
//! `docs/LOCAL_ADMIN_PROTOCOL.md` §7）。
//!
//! 启动顺序（每一步失败即拒绝启动，失败关闭）：
//!
//! 1. 加载配置（`crate::config`，由 `crate::cli` 传入已解析结果）；
//! 2. 打开（必要时迁移）存储并装配组合根（[`Composition::assemble`]，内部含 `recover_unsettled` 前置的
//!    端口构造）；
//! 3. 首次种子导入（`seed_state`/`mark_seeded` 单事务）；
//! 4. 启动恢复（`recover_unsettled`，§6 第 16 条：在取锁与开始监听**之前**）；
//! 5. 单实例锁 + `instanceId`（`fs4`，见 `crate::lock`）；
//! 6. 构建 Node Link 接入面并绑定 `daemon.listen`（TLS `direct` 的 PEM 在绑定前加载，两者失败都拒绝
//!    启动：`design.md` D10/D11）；
//! 7. 本地管理 endpoint（`LocalEndpoint::bind`，`runtime_dir = None` 表示按 §2.1 读 `XDG_RUNTIME_DIR`）；
//! 8. 发布锁记录（含 endpoint 定位串）→ 开放网络接入与本地通道的接受循环。
//!
//! 关闭顺序（`SECURITY_DESIGN.md` §12.1）：停接入层（本地接受循环已退出 + 网络 listener 停止 accept 并
//! 按 `daemon.shutdown_grace_ms` 排空在途连接）→ 取消后台任务 → 停止 Agent → 刷新存储
//! （`wal_checkpoint(TRUNCATE)`）→ 清理并释放单实例锁。`daemon.stop` 在本序列**开始**时返回
//! `{accepted: true}`，因此序列的第一步保留了在途连接的排空窗口（见 [`MIN_DRAIN_MS`]）。

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use acp_core::broker::Broker;
use acp_core::model::{
    Actor, CommittedEvent, NodeId, PeerPublicKey, PortError, SessionId, SessionState, Timestamp,
};
use acp_core::ports::{AttachmentStore as _, SessionQuery};
use acp_core::use_cases::UseCases;
use identity_auth::Authority;
use server::local_admin::{
    AdminError, AdminRequest, AdminResponse, DaemonAgent, DaemonControl, DaemonCounts,
    DaemonStatus, LocalAdminDeps, LocalAdminHandler, LocalAdminRouter, LocalErrorCode,
    PairingSessions,
};
use server::node_link::{
    CLAIM_PATH, CatalogRoute, CommandRoute, NodeLinkConfig, NodeLinkConn, PairingHttp,
    PairingHttpConfig, ResourceRoute, Routes, STATUS_PATH, WS_PATH, WS_SUBPROTOCOL, WsEndpoint,
    conn::ConnectionRegistry,
};
use server::transport::local::{
    AuditHook, EndpointError, InstanceId, LocalConnectionHandlers, LocalEndpoint,
    LocalEndpointConfig, LocalStream, serve_connection,
};
use server::transport::net::{NetConfig, NetError, NetListener, RouteError, Shutdown};
use tokio::sync::{Notify, mpsc};
use tokio::task::{JoinHandle, JoinSet};

use crate::compose::{AuditWriter, ComposeError, Composition, NodeLinkCloser, forked_publisher};
use crate::config::Config;
use crate::config::Loaded;
use crate::lock::{DaemonLock, LockError, LockRecord};
use storage_sqlite::session_store::SqliteStore;

/// 周期清理的间隔（`daemon-lifecycle` 规格：每 60 s 的 prune/expire_pairings/sweep_orphans）。
const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(60);

/// 每轮孤儿回收处理的文件数上限（§7.5 的 `limit`；不是配置键）。
const ORPHAN_SWEEP_LIMIT: u32 = 256;

/// 关闭序列里排空在途连接的最小窗口（毫秒）。
///
/// 规格要求 `daemon.stop` **先**返回 `{accepted: true}`（§5.2），而响应由连接任务在 `stop` 返回之后才
/// 写回。`graceMs = 0` 会让「不等待任何在途工作」与「先回响应」互相冲突，因此这里保留一个很小的下限，
/// 只用于把已经进入响应写出路径的那一条连接放完；其余在途工作仍按 `graceMs` 处理。实际窗口记在日志里
/// （`event = "daemon.drain"`），不静默。
const MIN_DRAIN_MS: u64 = 200;

/// `accept` 连续失败时的退避（避免在 endpoint 暂时不可用时热循环；F3 修正后失败可重试）。
const ACCEPT_RETRY_BACKOFF: Duration = Duration::from_millis(100);

/// 关闭序列里等待后台任务自行退出的时间上限。
const TASK_STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// 网络接入面（`daemon.listen`/`daemon.tls.*`）的启动失败：绑定、TLS 配置或路由注册。
///
/// 三者都是失败关闭：任何一项失败都不得降级为「无网络接入」继续运行（`specs/node-link-listener` 的
/// R1/R3/R14）。
#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    /// `NetListener::bind` 失败（地址非法、端口被占用、TLS 配置不可用、明文开发模式与监听地址冲突）。
    #[error("绑定 `daemon.listen` 失败")]
    Listener(#[source] NetError),
    /// 路由注册失败（path 重复或形态非法）：属配置/装配错误，不得静默跳过该端点。
    #[error("注册接入面路由失败")]
    Route(#[source] RouteError),
}

impl NetworkError {
    /// 不含完整敏感路径的简短说明（TLS 文件路径不进日志，`SECURITY_DESIGN.md` §14.1）。
    fn message(&self) -> String {
        match self {
            Self::Listener(NetError::InvalidListen { .. }) => {
                "`daemon.listen` is not a valid `ip:port` literal".to_owned()
            }
            Self::Listener(NetError::PlaintextDevNonLoopback { .. }) => {
                "`dev_mode.allow_plaintext` only applies to a loopback `daemon.listen`".to_owned()
            }
            Self::Listener(NetError::InvalidPublicOrigin { .. }) => {
                "`daemon.public_origin` is not a valid `http(s)://host[:port]`".to_owned()
            }
            Self::Listener(NetError::InvalidAllowedHost { .. }) => {
                "`daemon.allowed_hosts` contains an invalid host".to_owned()
            }
            Self::Listener(NetError::InvalidTrustedProxy { .. }) => {
                "`daemon.trusted_proxies` contains an invalid address".to_owned()
            }
            Self::Listener(NetError::InvalidLimit { name }) => {
                format!("the limit `{name}` must not be zero")
            }
            Self::Listener(NetError::TlsFileUnreadable { role, .. }) => {
                format!("the TLS {} file is missing or unreadable", tls_role(*role))
            }
            Self::Listener(NetError::TlsPemInvalid { role }) => {
                format!("the TLS {} file is not a usable PEM", tls_role(*role))
            }
            Self::Listener(NetError::TlsInsecurePermissions { role, .. }) => format!(
                "the TLS {} file permissions cannot be guaranteed",
                tls_role(*role)
            ),
            Self::Listener(NetError::TlsKeyCertificateMismatch) => {
                "the TLS certificate and private key do not match".to_owned()
            }
            Self::Listener(NetError::TlsConfigBuild) => {
                "the TLS configuration cannot be built".to_owned()
            }
            Self::Listener(NetError::Bind { .. }) => {
                "`daemon.listen` cannot be bound (address in use or not permitted)".to_owned()
            }
            Self::Listener(NetError::Serve { .. }) => {
                "the listener failed while serving".to_owned()
            }
            Self::Route(_) => "the ingress route cannot be registered".to_owned(),
        }
    }
}

/// TLS 文件角色的英文名（`serve` 侧的 `describe()` 是给用户看的中文，组合根的错误消息统一用英文分类）。
fn tls_role(role: server::transport::net::TlsFile) -> &'static str {
    match role {
        server::transport::net::TlsFile::Certificate => "certificate",
        server::transport::net::TlsFile::PrivateKey => "private key",
    }
}

/// Daemon 启动或运行失败。全部错误以非零退出码结束，并给出一行结构化 stderr。
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    /// 组合根装配失败。
    #[error("组合根装配失败：{0}")]
    Compose(#[source] ComposeError),
    /// 单实例锁已被持有（不得强杀、不得启动第二个实例）。
    #[error("已有另一个 acp-remote Daemon 在运行")]
    AlreadyRunning,
    /// 锁文件不可用。
    #[error("单实例锁不可用")]
    Lock(#[source] LockError),
    /// endpoint 创建失败（权限不符、路径被占用、符号链接、旧 socket 仍存活……）。
    #[error("本地管理 endpoint 创建失败")]
    Endpoint(#[source] EndpointError),
    /// 网络接入面不可用（绑定失败、TLS 配置失败、路由注册失败）。
    #[error("网络接入面不可用")]
    Network(#[source] NetworkError),
    /// 关闭时存储未能完成检查点（仍有共享句柄）。
    #[error("关闭时存储未能完成检查点")]
    StoreClose(#[source] ComposeError),
}

impl DaemonError {
    /// stderr 结构化行里的 `code`：取值来自 `LOCAL_ADMIN_PROTOCOL.md` §6 的本地错误码表。
    pub fn code(&self) -> &'static str {
        match self {
            // 「目标存在但当前状态不允许该操作」：此处目标是单实例锁。
            Self::AlreadyRunning => LocalErrorCode::Conflict.as_str(),
            Self::Compose(ComposeError::InvalidSeed { .. }) => {
                LocalErrorCode::InvalidParams.as_str()
            }
            Self::Compose(_) | Self::Lock(_) | Self::Endpoint(_) | Self::StoreClose(_) => {
                LocalErrorCode::Unavailable.as_str()
            }
            Self::Network(_) => LocalErrorCode::Unavailable.as_str(),
        }
    }

    /// 不含完整敏感路径与配置值的简短说明。
    pub fn message(&self) -> String {
        match self {
            Self::AlreadyRunning => {
                "another acp-remote daemon already holds the single-instance lock".to_owned()
            }
            Self::Compose(error) => error.message(),
            Self::Lock(LockError::Held) => "the single-instance lock is held".to_owned(),
            Self::Lock(_) => "the single-instance lock file is not usable".to_owned(),
            Self::Endpoint(EndpointError::AlreadyInUse) => {
                "the local admin endpoint is already in use by a live instance".to_owned()
            }
            Self::Endpoint(EndpointError::UnsafePath { .. }) => {
                "the local admin endpoint path must not be a symlink or a foreign object type"
                    .to_owned()
            }
            Self::Endpoint(EndpointError::InsecurePermissions { kind, .. }) => {
                format!("the local admin endpoint {kind} permissions cannot be guaranteed")
            }
            Self::Endpoint(EndpointError::Os { source }) => {
                format!("cannot create the local admin endpoint ({})", source.kind())
            }
            Self::Endpoint(EndpointError::PeerRejected) => {
                "the local admin endpoint rejected a peer".to_owned()
            }
            Self::StoreClose(error) => error.message(),
            Self::Network(error) => error.message(),
        }
    }
}

/// 关闭信号：`daemon.stop`/Ctrl+C/SIGTERM 都会置位，`daemon.stop` 可以同时给本次关闭一个宽限值。
///
/// 触发者（本地通道 / OS 信号）也记录在信号里：关闭日志必须能回答「因为什么开始关闭」。
///
/// 「接受」日志（`daemon.stop_accepted`/`daemon.stop_repeated`）由调用方在 `request` **之前**
/// 输出：置位之后主任务会立刻进入关闭序列并写 `daemon.shutdown_begin`，置位后再记日志会让
/// 两条日志的先后成为调度竞赛（Linux CI 实踩）。先取「是否首次」再记日志，最后置位；
/// 两个连接并发 stop 的窗口只会让「accepted/repeated」的日志归属互换，不影哴关闭语义。
#[derive(Debug)]
pub struct ShutdownSignal {
    requested: AtomicBool,
    notify: Notify,
    grace_ms: std::sync::Mutex<Option<u64>>,
    reason: std::sync::Mutex<&'static str>,
}

impl ShutdownSignal {
    /// 构造。
    pub fn new() -> Self {
        Self {
            requested: AtomicBool::new(false),
            notify: Notify::new(),
            grace_ms: std::sync::Mutex::new(None),
            reason: std::sync::Mutex::new("daemon.stop"),
        }
    }

    /// 请求关闭（幂等）。`grace_ms` 是 CLI 显式给出的宽限，`None` = 用 `daemon.shutdown_grace_ms`。
    ///
    /// 返回 `true` 表示本次调用**首次**请求关闭。
    pub fn request(&self, grace_ms: Option<u64>) -> bool {
        if let Some(grace_ms) = grace_ms {
            match self.grace_ms.lock() {
                Ok(mut current) => {
                    if current.is_none() {
                        *current = Some(grace_ms);
                    }
                }
                Err(poisoned) => {
                    *poisoned.into_inner() = Some(grace_ms);
                }
            }
        }
        let first = !self.requested.swap(true, Ordering::SeqCst);
        // 无论是否首次都唤醒等待者：它们可能在 `swap` 之前就开始等待。
        self.notify.notify_waiters();
        first
    }

    /// OS 信号触发的关闭请求（原因记进信号，供关闭日志使用）。
    pub fn request_from_signal(&self, reason: &'static str) -> bool {
        match self.reason.lock() {
            Ok(mut current) => *current = reason,
            Err(poisoned) => *poisoned.into_inner() = reason,
        }
        self.request(None)
    }

    /// 是否已经开始关闭序列（连接门闸用它判定「停止期间新请求」）。
    pub fn is_requested(&self) -> bool {
        self.requested.load(Ordering::SeqCst)
    }

    /// 本次关闭使用的宽限值（显式覆盖优先）。
    pub fn grace_ms(&self) -> Option<u64> {
        self.grace_ms.lock().map_or(None, |grace| *grace)
    }

    /// 触发本次关闭的原因。
    pub fn reason(&self) -> &'static str {
        self.reason.lock().map_or("daemon.stop", |reason| *reason)
    }

    /// 等待关闭请求。取消安全。
    pub async fn wait(&self) {
        if self.is_requested() {
            return;
        }
        // 「先检查再等待」之间可能已经置位：`notify_waiters` 只唤醒当前等待者，因此等待前再查一次。
        let notified = self.notify.notified();
        if self.is_requested() {
            return;
        }
        notified.await;
    }
}

impl Default for ShutdownSignal {
    fn default() -> Self {
        Self::new()
    }
}

/// 关闭期门闸：关闭序列开始后，任何**新**请求都以 `local.unavailable` 失败且不产生状态变更（R14）。
///
/// 已建立的连接在排空窗口内仍被服务（`daemon.stop` 的响应要写回），但它们的后续请求都会被这里拦下。
struct ShuttingDownGate {
    inner: Arc<dyn LocalAdminHandler>,
    shutdown: Arc<ShutdownSignal>,
}

#[async_trait::async_trait]
impl LocalAdminHandler for ShuttingDownGate {
    async fn handle(&self, request: AdminRequest) -> AdminResponse {
        if self.shutdown.is_requested() {
            tracing::warn!(
                event = "daemon.shutting_down_request",
                "关闭序列进行中：管理请求以 local.unavailable 失败"
            );
            return AdminResponse::failure(
                request.id().clone(),
                AdminError::new(LocalErrorCode::Unavailable, "daemon is shutting down"),
            );
        }
        self.inner.handle(request).await
    }
}

/// `daemon.status`/`daemon.stop` 的组合根实现（`DaemonControl` 是 WP3 冻结的注入口）。
struct AppDaemonControl {
    version: String,
    instance_id: String,
    data_dir: String,
    public_origin: Option<String>,
    /// `daemon.status.listen`：实际绑定的网络监听地址（`NetListener::local_addrs` 的文本形式）。
    listen: Vec<String>,
    started_at: Timestamp,
    started_instant: Instant,
    node_id: NodeId,
    node_public_key: PeerPublicKey,
    core: Arc<acp_core::use_cases::UseCases>,
    shutdown: Arc<ShutdownSignal>,
}

#[async_trait::async_trait]
impl DaemonControl for AppDaemonControl {
    async fn status(&self) -> DaemonStatus {
        // `links` 恒空：Node Link 的**出站**重连管理器属切片 6（入站连接由 `server::node_link` 的
        // 连接注册表管理，不进 `daemon.status`，§5.2 的字段注记）。
        DaemonStatus {
            version: self.version.clone(),
            instance_id: self.instance_id.clone(),
            node_id: self.node_id.clone(),
            node_public_key: self.node_public_key.clone(),
            started_at: self.started_at.clone(),
            uptime_ms: u64::try_from(self.started_instant.elapsed().as_millis())
                .unwrap_or(u64::MAX),
            data_dir: self.data_dir.clone(),
            listen: self.listen.clone(),
            public_origin: self.public_origin.clone(),
            counts: self.counts().await,
            agents: self.agents().await,
            links: Vec::new(),
        }
    }

    async fn stop(&self, grace_ms: Option<u64>) -> Result<(), AdminError> {
        // 先记日志再置位（原因见 `ShutdownSignal` 文档）：保证 `stop_accepted` 先于 `shutdown_begin`。
        let first = !self.shutdown.is_requested();
        if first {
            tracing::info!(
                event = "daemon.stop_accepted",
                grace_ms_override = ?grace_ms,
                "已接受关闭请求：响应先于关闭序列写回"
            );
        } else {
            tracing::debug!(event = "daemon.stop_repeated", "重复的关闭请求");
        }
        self.shutdown.request(grace_ms);
        // §5.2：`accepted` 恒为 `true`，失败必须走 `error`；本实现没有可失败的步骤。
        Ok(())
    }
}

impl AppDaemonControl {
    /// `counts`：由 core 查询聚合（与持久记录一致）。
    async fn counts(&self) -> DaemonCounts {
        let actor = Actor::LocalCli;
        DaemonCounts {
            devices: count(self.core.devices(&actor).await, "devices"),
            nodes: count(self.core.nodes(&actor).await, "nodes"),
            exports: count(self.core.exports(&actor).await, "exports"),
            imports: count(self.core.imports(&actor).await, "imports"),
        }
    }

    /// `agents`：profile 存在且 `command` 可解析（`design.md` 决策 6 的本切片口径）。
    async fn agents(&self) -> Vec<DaemonAgent> {
        match self.core.profiles(&Actor::LocalCli).await {
            Ok(profiles) => profiles
                .iter()
                .map(|profile| DaemonAgent {
                    agent_id: profile.id().clone(),
                    available: command_available(profile.command()),
                })
                .collect(),
            Err(error) => {
                tracing::warn!(
                    event = "daemon.status_agents_failed",
                    error = crate::compose::port_error_token(&error),
                    "读取 Agent profile 失败：`agents` 按空数组回答"
                );
                Vec::new()
            }
        }
    }
}

/// 计数查询失败时只记结构化日志并计 0（`DaemonControl::status` 的签名没有 `Result`）。
fn count<T>(result: Result<Vec<T>, PortError>, what: &'static str) -> u64 {
    match result {
        Ok(rows) => rows.len() as u64,
        Err(error) => {
            tracing::warn!(
                event = "daemon.status_count_failed",
                what,
                error = crate::compose::port_error_token(&error),
                "管理记录计数失败：该字段按 0 回答"
            );
            0
        }
    }
}

/// `command` 是否可解析（`design.md` 决策 6）：带路径分隔符看文件是否存在，否则在 `PATH` 里找。
///
/// 与 `agent-host` 的私有 `command_resolves` 同口径但**不是同一个函数**（那边还要求凭据可解析）；
/// `daemon.status.agents[].available` 按决策 6 只表达「profile 存在且 command 可解析」。
fn command_available(command: &str) -> bool {
    if command.is_empty() {
        return false;
    }
    let path = Path::new(command);
    if path.components().count() > 1 {
        return path.is_file();
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    let candidates = candidate_names(command);
    std::env::split_paths(&paths)
        .any(|directory| candidates.iter().any(|name| directory.join(name).is_file()))
}

/// `PATH` 查找时尝试的名字（Windows 上补可执行扩展名）。
fn candidate_names(command: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        vec![
            command.to_owned(),
            format!("{command}.exe"),
            format!("{command}.cmd"),
            format!("{command}.bat"),
        ]
    }
    #[cfg(not(windows))]
    {
        vec![command.to_owned()]
    }
}

/// 后台任务（周期任务与信号监听）的所有者与取消路径。
///
/// 每个任务都持有一个 [`Notify`]：取消是**协作式**的（任务自己在等待点退出），因此不会在事务中途
/// 被 abort；只有在超时仍未退出时才 abort，并在日志里如实说明。
struct OwnedTask {
    name: &'static str,
    cancel: Arc<Notify>,
    handle: JoinHandle<()>,
}

#[derive(Default)]
struct OwnedTasks {
    tasks: Vec<OwnedTask>,
}

impl OwnedTasks {
    fn new() -> Self {
        Self::default()
    }

    /// 派生一个由本结构持有的任务。
    fn spawn<F>(&mut self, name: &'static str, cancel: Arc<Notify>, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let handle = tokio::spawn(future);
        self.tasks.push(OwnedTask {
            name,
            cancel,
            handle,
        });
    }

    /// 取消并等待全部任务结束（`deadline` 之前；超时才 abort）。
    async fn cancel_all(self, deadline: Instant) {
        for task in &self.tasks {
            task.cancel.notify_waiters();
        }
        for task in self.tasks {
            let name = task.name;
            match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), task.handle)
                .await
            {
                Ok(Ok(())) => tracing::info!(
                    event = "daemon.task_stopped",
                    task = name,
                    "后台任务已取消并回收"
                ),
                Ok(Err(error)) if error.is_cancelled() => tracing::info!(
                    event = "daemon.task_stopped",
                    task = name,
                    cancelled = true,
                    "后台任务已被取消"
                ),
                Ok(Err(error)) => tracing::error!(
                    event = "daemon.task_failed",
                    task = name,
                    error = %error,
                    "后台任务异常结束"
                ),
                Err(_) => tracing::warn!(
                    event = "daemon.task_stop_timeout",
                    task = name,
                    "后台任务未在期限内退出：强制中止"
                ),
            }
        }
    }
}

/// 派生 OS 终止信号监听任务（Ctrl+C；Unix 还包括 SIGTERM）。
fn spawn_signal_watcher(tasks: &mut OwnedTasks, shutdown: Arc<ShutdownSignal>) {
    let cancel = Arc::new(Notify::new());
    let watcher_cancel = Arc::clone(&cancel);
    tasks.spawn("signal_watcher", cancel, async move {
        let reason = tokio::select! {
            _ = watcher_cancel.notified() => return,
            reason = termination_signal() => reason,
        };
        if shutdown.request_from_signal(reason) {
            tracing::warn!(
                event = "daemon.termination_signal",
                reason,
                "收到 OS 终止信号：开始关闭序列"
            );
        }
    });
}

/// 等待任一 OS 终止信号。
async fn termination_signal() -> &'static str {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut terminate) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => "ctrl_c",
                _ = terminate.recv() => "sigterm",
            }
        } else {
            tracing::warn!(event = "daemon.signal_unavailable");
            let _ = tokio::signal::ctrl_c().await;
            "ctrl_c"
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
        "ctrl_c"
    }
}

/// 周期清理任务：启动初清理 + 每 [`MAINTENANCE_INTERVAL`] 的 prune/expire_pairings/sweep_orphans（§7.5）。
fn spawn_maintenance(
    tasks: &mut OwnedTasks,
    composition: &Composition,
    shutdown: Arc<ShutdownSignal>,
) {
    let cancel = Arc::new(Notify::new());
    let task_cancel = Arc::clone(&cancel);
    let core = Arc::clone(composition.use_cases());
    let policy = composition.retention_policy();
    let clock = Arc::clone(composition.clock());
    let attachments = composition.attachments();
    tasks.spawn("maintenance", cancel, async move {
        let mut next = tokio::time::Instant::now();
        let mut startup = true;
        loop {
            tokio::select! {
                _ = task_cancel.notified() => break,
                _ = tokio::time::sleep_until(next) => {}
            }
            if shutdown.is_requested() {
                break;
            }
            maintenance_tick(
                &core,
                &policy,
                &clock,
                attachments.as_ref(),
                if startup { "startup" } else { "periodic" },
            )
            .await;
            startup = false;
            next = tokio::time::Instant::now() + MAINTENANCE_INTERVAL;
        }
    });
}

/// 一轮清理：`prune` + `expire_pairings` + `sweep_orphans`（三者各自失败只记日志，不中断后续）。
async fn maintenance_tick(
    core: &Arc<acp_core::use_cases::UseCases>,
    policy: &acp_core::ports::RetentionPolicy,
    clock: &Arc<dyn acp_core::ports::Clock>,
    attachments: &SqliteStore,
    reason: &'static str,
) {
    let actor = Actor::LocalCli;
    let mut removed_events = 0u64;
    let mut still_over_limit = false;
    match core.prune(&actor, *policy).await {
        Ok(report) => {
            removed_events = report.removed_events;
            still_over_limit = report.still_over_limit;
        }
        Err(error) => tracing::warn!(
            event = "daemon.maintenance_failed",
            step = "prune",
            error = crate::compose::port_error_token(&error)
        ),
    }
    let expired_pairings = match core.expire_pairings(&actor).await {
        Ok(count) => count,
        Err(error) => {
            tracing::warn!(
                event = "daemon.maintenance_failed",
                step = "expire_pairings",
                error = crate::compose::port_error_token(&error)
            );
            0
        }
    };
    let swept_orphans = match attachments
        .sweep_orphans(clock.now(), ORPHAN_SWEEP_LIMIT)
        .await
    {
        Ok(count) => count,
        Err(error) => {
            tracing::warn!(
                event = "daemon.maintenance_failed",
                step = "sweep_orphans",
                error = crate::compose::port_error_token(&error)
            );
            0
        }
    };
    tracing::info!(
        event = "daemon.maintenance",
        reason,
        removed_events,
        expired_pairings,
        swept_orphans,
        still_over_limit,
        "周期清理完成"
    );
}

/// 合并窗口的驱动面（§6 第 10 条）。
///
/// 拆成「枚举会话」与「驱动单个会话」两步是为了给定时任务留下可注入的接缝：本切片的本地通道方法集不含
/// `session.create`（`LOCAL_ADMIN_PROTOCOL.md` §5.8），集成测试造不出 owned 会话，因此「按配置间隔真的
/// 被调用了」只能靠用例注入 spy 断言（见 `daemon::tests` 的合并窗口用例）。
#[async_trait::async_trait]
trait MergeWindow: Send + Sync {
    /// 本轮需要驱动的会话。
    async fn sessions(&self) -> Result<Vec<SessionId>, PortError>;
    /// 驱动单个会话（落盘缓冲事件 + 派发下一个排队 turn）。
    async fn pump(&self, session: &SessionId) -> Result<(), PortError>;
}

/// 本轮枚举的状态集合（§6 第 10 条的合并窗口只对「有内存缓冲的会话」有意义）。
///
/// 会话状态转换与触发它的事件批次**在同一事务**提交（§6 第 1/2 条），因此持久状态为 `idle`/`failed`/
/// `closed` 蕴含该会话没有待提交的缓冲；反过来，正在跑或排队的会话必然是 `queued`/`running`/`waiting_*`。
/// 只取这四种状态而不取全表，是因为 `SessionQuery` 没有 offset/cursor（`core::ports` §4 的查询形状），
/// 无法分页续取——状态过滤是这里唯一能把返回集限制在少数活动会话的手段（过滤在 SQL 侧完成）。
const MERGE_WINDOW_STATES: [SessionState; 4] = [
    SessionState::Queued,
    SessionState::Running,
    SessionState::WaitingInput,
    SessionState::WaitingPermission,
];

/// 生产实现：`UseCases::list_sessions` 枚举 + `Broker::pump` 驱动。
struct BrokerMergeWindow {
    core: Arc<UseCases>,
    broker: Arc<Broker>,
}

impl BrokerMergeWindow {
    fn new(core: Arc<UseCases>, broker: Arc<Broker>) -> Self {
        Self { core, broker }
    }
}

#[async_trait::async_trait]
impl MergeWindow for BrokerMergeWindow {
    async fn sessions(&self) -> Result<Vec<SessionId>, PortError> {
        let query = SessionQuery {
            only: None,
            states: MERGE_WINDOW_STATES.to_vec(),
            limit: None,
        };
        let summaries = self.core.list_sessions(&Actor::LocalCli, query).await?;
        Ok(summaries
            .iter()
            .map(|summary| summary.session_id().clone())
            .collect())
    }

    async fn pump(&self, session: &SessionId) -> Result<(), PortError> {
        self.broker.pump(session).await
    }
}

/// 按 `storage.flush_interval_ms` 驱动 broker 的合并窗口（`CORE_PORTS_AND_STORAGE.md` §6 第 10 条）。
///
/// core 不读时钟、不设定时器（`crates/core/src/broker.rs` 模块头）：后端事件经同步的 `EventSink` 进
/// broker 的缓冲，**必须**由持有 runtime 的组合根周期调 `pump` 才落盘并广播。本任务就是那个定时器：
/// 每个周期枚举一次活动会话，逐个 `pump`（落盘缓冲 + 派发下一个排队 turn）。
///
/// 不吞错：单会话失败记结构化警告（含 `session_id` 与错误类别）后继续本轮其余会话，也不结束任务——
/// 缓冲仍在内存里，下个周期会重试；枚举失败同样只记警告。取消路径与其它周期任务相同（协作式 `Notify`），
/// 而关闭序列在停 Agent 与刷盘之前先调 `OwnedTasks::cancel_all`（`CORE_PORTS_AND_STORAGE.md` §7.5 第 4 条）。
fn spawn_merge_window<M: MergeWindow + 'static>(
    tasks: &mut OwnedTasks,
    interval: Duration,
    window: Arc<M>,
    shutdown: Arc<ShutdownSignal>,
) {
    let cancel = Arc::new(Notify::new());
    let task_cancel = Arc::clone(&cancel);
    let interval_ms = u64::try_from(interval.as_millis()).unwrap_or(u64::MAX);
    let mut ticks = 0u64;
    tasks.spawn("merge_window", cancel, async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = task_cancel.notified() => break,
                _ = ticker.tick() => {}
            }
            if shutdown.is_requested() {
                break;
            }
            ticks += 1;
            let sessions = match window.sessions().await {
                Ok(sessions) => sessions,
                Err(error) => {
                    tracing::warn!(
                        event = "daemon.merge_window_scan_failed",
                        ticks,
                        error = crate::compose::port_error_token(&error),
                        "枚举活动会话失败：本轮不驱动任何会话"
                    );
                    continue;
                }
            };
            let mut pumped = 0u64;
            for session in &sessions {
                match window.pump(session).await {
                    Ok(()) => pumped += 1,
                    Err(error) => tracing::warn!(
                        event = "daemon.merge_window_failed",
                        ticks,
                        session_id = session.as_str(),
                        error = crate::compose::port_error_token(&error),
                        "驱动会话的合并窗口失败：本轮跳过该会话，下轮重试"
                    ),
                }
            }
            // 每个 tick 都记会导致 250ms 一级的稳定噪声，因此空转轮走 debug；有会话或失败时才 info。
            if sessions.is_empty() {
                tracing::debug!(
                    event = "daemon.merge_window",
                    ticks,
                    interval_ms,
                    "本轮无活动会话"
                );
            } else {
                tracing::info!(
                    event = "daemon.merge_window",
                    ticks,
                    interval_ms,
                    sessions = sessions.len(),
                    pumped,
                    "已按配置间隔驱动 broker 的合并窗口"
                );
            }
        }
    });
}

/// `app::config` → `server::transport::net::NetConfig`（**唯一的字段映射点**）。
///
/// `node_link.*` 的可下调限额在本 WP 仍是「已知但未接线」（`app::config::Config::unwired`）：本函数
/// 因此不把 `node_link.max_message_bytes` 接进接入层，而取 `NetConfig::default()` 的单消息上限——它与
/// `NodeLinkConfig::default()` 是同一个 1 MiB，因此「传输层在分配前拒绝」与「`node.ready.limits`
/// 下发给对端的值」不会分叉。接线 `node_link.*` 属同一收敛任务（键清单的改写需先由主 Agent 确认），
/// 本函数是该接线的唯一落点。
pub fn net_config(config: &Config) -> NetConfig {
    NetConfig {
        listen: config.listen.clone(),
        public_origin: config.public_origin.clone(),
        allowed_hosts: config.allowed_hosts.clone(),
        trusted_proxies: config.trusted_proxies.clone(),
        tls: config.tls.clone(),
        allow_plaintext_dev: config.dev_mode.allow_plaintext,
        // 网络 listener 的排空宽限与整条关闭序列同源（`daemon.shutdown_grace_ms`）。
        drain_grace: Duration::from_millis(config.shutdown_grace_ms),
        ..NetConfig::default()
    }
}

/// Node Link 入站面的装配件：连接注册表与三个路由（catalog/resource/command）。
///
/// 路由的 node id **必须**是本机（Owner）node id（`Authority::local_node`）：它是
/// `remoteSessionRef.ownerNodeId` 的判定基准，传错会让**全部** `resource.attach`/`resource.ack` 被拒。
/// 两者都只从 `authority` 派生（唯一来源），并由全链路集成测试真实跑通 attach/ack 钉死。
pub struct NodeLinkIngress {
    registry: Arc<ConnectionRegistry>,
    resource: Arc<ResourceRoute>,
    command: Arc<CommandRoute>,
}

impl NodeLinkIngress {
    /// 连接注册表（撤销传播与扇出共用同一个句柄）。
    pub fn registry(&self) -> &Arc<ConnectionRegistry> {
        &self.registry
    }

    /// 资源路由（attach/subscribe/ack 与事件扇出）。
    pub fn resource(&self) -> &Arc<ResourceRoute> {
        &self.resource
    }

    /// 命令路由（`command.submit`/`command.status` 与撤销传播）。
    pub fn command(&self) -> &Arc<CommandRoute> {
        &self.command
    }

    /// 装配三个路由（不触碰 listener 与网络）。
    pub fn assemble(core: Arc<UseCases>, authority: Arc<Authority>) -> Self {
        let registry = ConnectionRegistry::new();
        let resource = Arc::new(ResourceRoute::new(
            Arc::clone(&core),
            Arc::clone(&registry),
            authority.local_node().clone(),
        ));
        let command = Arc::new(CommandRoute::new(
            Arc::clone(&core),
            Arc::clone(&registry),
            Arc::clone(&resource),
            authority,
        ));
        Self {
            registry,
            resource,
            command,
        }
    }

    /// 本路由集的组合件：`Routes` 按序询问，第一个认领的胜出（catalog → resource → command）。
    fn routes(&self, core: Arc<UseCases>) -> Routes {
        let parts: Vec<Arc<dyn server::node_link::MessageRoute>> = vec![
            Arc::new(CatalogRoute::new(core)),
            Arc::clone(&self.resource) as Arc<dyn server::node_link::MessageRoute>,
            Arc::clone(&self.command) as Arc<dyn server::node_link::MessageRoute>,
        ];
        Routes::new(parts)
    }
}

/// 把 Node Link 的全部 path 注册到共享 listener（WSS + 配对 claim/status）并返回路由集。
///
/// 这是**唯一的 Node Link 接线点**：组合根（[`NetIngress::start`]）与受控路径全链路集成测试都调它，
/// 因此「路由顺序」「node id 来源」「配对端点的安全响应头」不会在两处漂移。
///
/// 配对端点的四个安全响应头经 `PairingHttp::default_response_headers` **同时**声明给接入层
/// （`register_post` 的每路径默认响应头）：否则接入层在调用处理器前产生的 413/Host 400 不带这些头。
pub fn register_node_link_paths(
    listener: &mut NetListener,
    core: &Arc<UseCases>,
    authority: &Arc<Authority>,
    link: &NodeLinkConfig,
) -> Result<NodeLinkIngress, NetworkError> {
    let ingress = NodeLinkIngress::assemble(Arc::clone(core), Arc::clone(authority));
    let conn = Arc::new(
        NodeLinkConn::new(
            Arc::clone(core),
            Arc::clone(authority),
            link.clone(),
            Arc::clone(&ingress.registry),
        )
        .with_route(Arc::new(ingress.routes(Arc::clone(core)))),
    );
    listener
        .register_ws(WS_PATH, WS_SUBPROTOCOL, WsEndpoint::handler(conn))
        .map_err(NetworkError::Route)?;
    let pairing = Arc::new(PairingHttp::new(
        Arc::clone(core),
        Arc::clone(authority),
        PairingHttpConfig {
            // 同一份快照：`node_link` 的握手 endpoint 与配对端点都取 `daemon.public_origin`。
            public_origin: link.public_origin.clone(),
        },
    ));
    let default_headers = pairing.default_response_headers();
    listener
        .register_post(CLAIM_PATH, pairing.claim_handler(), default_headers)
        .map_err(NetworkError::Route)?;
    listener
        .register_post(STATUS_PATH, pairing.status_handler(), default_headers)
        .map_err(NetworkError::Route)?;
    Ok(ingress)
}

/// 网络接入面的所有者句柄：listener、事件扇出与命令终态观察共用同一个关闭信号（`design.md` D11）。
///
/// 三个任务都由本结构持有；「停接入层」就是触发该信号并等它们结束（超时 abort），因此在关闭路径上
/// 不遗留 detached task。
pub struct NetIngress {
    /// 停止 accept、排空在途连接与退出两个分发循环的信号。
    shutdown: server::transport::net::ShutdownHandle,
    /// 实际绑定的监听地址（`daemon.status.listen`）。
    listen: Vec<String>,
    command: Arc<CommandRoute>,
    registry: Arc<ConnectionRegistry>,
    tasks: Vec<(&'static str, JoinHandle<()>)>,
}

impl NetIngress {
    /// 装配并启动网络接入面：绑定 `daemon.listen`（含 TLS 加载）→ 注册全部 path → spawn 三个任务。
    ///
    /// 绑定或注册失败时返回 `Err`：调用方必须拒绝启动（**不**降级为「无网络接入」），且此时还没有任何
    /// 任务被 spawn（listener 在出错路径上随 `listener` 一起 drop，端口立即释放）。
    pub async fn start(
        composition: &Composition,
        event_queue: mpsc::Receiver<CommittedEvent>,
    ) -> Result<Self, NetworkError> {
        let mut listener = NetListener::bind(net_config(composition.config()))
            .await
            .map_err(NetworkError::Listener)?;
        // 启动期告警（非 loopback 监听、proxy 模式的明文面、平台权限不可核验）必须出现在启动输出里：
        // `ListenerWarning` 只描述事实，是否继续由运维决定（它们都不是失败关闭）。
        for warning in listener.warnings() {
            tracing::warn!(event = "daemon.listener_warning", warning = %warning, "{warning}");
        }
        let core = Arc::clone(composition.use_cases());
        let authority = Arc::clone(composition.authority());
        let ingress = register_node_link_paths(
            &mut listener,
            &core,
            &authority,
            &NodeLinkConfig {
                public_origin: composition.config().public_origin.clone(),
                ..NodeLinkConfig::default()
            },
        )?;
        let listen = listener
            .local_addrs()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<String>>();
        let (shutdown, signal) = Shutdown::channel();
        let listener_signal = signal.clone();
        let mut tasks: Vec<(&'static str, JoinHandle<()>)> = Vec::new();
        tasks.push((
            "node_link_listener",
            tokio::spawn(async move {
                if let Err(error) = listener.serve(listener_signal).await {
                    tracing::warn!(
                        event = "daemon.listener_failed",
                        error = %error,
                        "网络接入面在服务期间失败"
                    );
                }
            }),
        ));
        tasks.push((
            "node_link_fanout",
            tokio::spawn({
                let resource = Arc::clone(&ingress.resource);
                let signal = signal.clone();
                async move { resource.dispatch(event_queue, signal).await }
            }),
        ));
        tasks.push((
            "node_link_commands",
            tokio::spawn({
                let command = Arc::clone(&ingress.command);
                let signal = signal.clone();
                async move { command.dispatch(signal).await }
            }),
        ));
        tracing::info!(
            event = "daemon.ingress_ready",
            listen = ?listen,
            "网络接入面已就绪：Node Link 的 WSS 与配对 HTTP 开始接受连接"
        );
        Ok(Self {
            shutdown,
            listen,
            command: ingress.command,
            registry: ingress.registry,
            tasks,
        })
    }

    /// 实际绑定的监听地址（`daemon.status.listen` 的来源）。
    pub fn listen(&self) -> Vec<String> {
        self.listen.clone()
    }

    /// 命令路由句柄（撤销通知缝 `NodeLinkCloser` 装配用）。
    pub fn command(&self) -> &Arc<CommandRoute> {
        &self.command
    }

    /// 连接注册表句柄（撤销传播的观察面；目前只由测试与 `CommandRoute` 使用）。
    pub fn registry(&self) -> &Arc<ConnectionRegistry> {
        &self.registry
    }

    /// 停接入层的**第一步（同步）**：触发关闭信号。触发后 listener 停止 accept（并按
    /// `NetConfig::drain_grace` 排空在途连接），事件扇出与命令分发两个循环随之退出。
    ///
    /// 它必须是一个同步方法：关闭序列要**先**真的让网络面停 accept，再去排空本地在途连接。合并成
    /// `async fn` 会让触发推迟到 `await` 点执行，于是整个本地排空窗口内网络 listener 仍在接受新连接。
    /// 等待三个任务结束是第二步（[`NetIngress::wait_stopped`]）。
    pub fn trigger_shutdown(&self) {
        self.shutdown.trigger();
    }

    /// 停接入层的**第二步（异步）**：等三个任务结束；到 `deadline` 仍未退出的任务被 abort 并记一条
    /// 警告。任务全部结束意味着 accept 已停、在途连接已按宽限排空或已被强制中止。
    pub async fn wait_stopped(self, deadline: Instant) {
        for (name, mut handle) in self.tasks {
            match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), &mut handle)
                .await
            {
                Ok(Ok(())) => tracing::info!(
                    event = "daemon.task_stopped",
                    task = name,
                    "网络接入面任务已结束"
                ),
                Ok(Err(error)) if error.is_cancelled() => tracing::info!(
                    event = "daemon.task_stopped",
                    task = name,
                    cancelled = true,
                    "网络接入面任务已被取消"
                ),
                Ok(Err(error)) => tracing::error!(
                    event = "daemon.task_failed",
                    task = name,
                    error = %error,
                    "网络接入面任务异常结束"
                ),
                Err(_) => {
                    handle.abort();
                    let _ = handle.await;
                    tracing::warn!(
                        event = "daemon.task_stop_timeout",
                        task = name,
                        "网络接入面任务未在期限内退出：已强制中止"
                    );
                }
            }
        }
    }
}

/// 运行前台 Daemon，直到关闭序列完成。
///
/// 返回 `Ok(())` 表示按 `SECURITY_DESIGN.md` §12.1 的顺序正常关闭；任何启动失败都以 `Err` 结束
/// （调用方据此以非零退出码结束）。
pub async fn run(loaded: Loaded) -> Result<(), DaemonError> {
    let Loaded {
        config,
        default_file_missing,
    } = loaded;
    if default_file_missing {
        tracing::warn!(
            event = "daemon.config_default_missing",
            "平台默认位置没有配置文件：按内置默认值启动（`ACP_REMOTE_CONFIG` 可指定路径）"
        );
    }
    for key in &config.unwired {
        tracing::debug!(
            event = "daemon.config_unwired",
            key = key.as_str(),
            "该配置键本切片不消费：已解析但不生效"
        );
    }
    if config.dev_mode.enabled {
        tracing::warn!(
            event = "daemon.dev_mode",
            "开发模式已启用：这不是安全部署形态（dev_mode.enabled = true）"
        );
    }
    if config.public_origin.is_none() {
        tracing::warn!(
            event = "daemon.public_origin_missing",
            "未配置 `daemon.public_origin`：配对方法在任何副作用之前以 local.unavailable 失败关闭"
        );
    }
    let flush_interval = Duration::from_millis(config.flush_interval_ms);
    // 事件扇出的分叉在装配 broker 之前建立：发布端口注入 `Composition`，队列交给网络接入面的分发循环
    // （`design.md` D6）。两者必须同时存在，否则已提交事件会在消费者出现前被丢弃。
    let (publisher, event_queue) = forked_publisher();

    let composition = Composition::assemble(config, publisher)
        .await
        .map_err(DaemonError::Compose)?;
    match composition.seed_if_needed().await {
        Ok(crate::compose::SeedOutcome::Imported { count }) => tracing::info!(
            event = "daemon.seed_imported",
            profiles = count,
            "首次初始化：种子 profile 与「已初始化」标记已同一事务提交"
        ),
        Ok(crate::compose::SeedOutcome::AlreadySeeded) => {
            tracing::debug!(
                event = "daemon.seed_skipped",
                "管理存储已初始化：跳过种子导入"
            )
        }
        Err(error) => return Err(DaemonError::Compose(error)),
    }

    // §6 第 16 条：启动恢复在取锁与开始监听**之前**（`LocalCli`）。
    let recovered = composition
        .use_cases()
        .recover_unsettled(&Actor::LocalCli, acp_core::ports::ReplayLimit::default())
        .await
        .map_err(|error| DaemonError::Compose(ComposeError::Seed(error)))?;
    if recovered > 0 {
        tracing::warn!(
            event = "daemon.recovered_unsettled",
            commands = recovered,
            "启动恢复终结了未决命令（不确定终态，未重放副作用）"
        );
    }

    // 单实例锁：失败即明确错误退出，绝不强杀已有进程。
    let instance_id = InstanceId::generate();
    let lock_path = crate::lock::lock_path(&composition.config().data_dir);
    let mut lock = match DaemonLock::acquire(&lock_path) {
        Ok(lock) => lock,
        Err(LockError::Held) => return Err(DaemonError::AlreadyRunning),
        Err(error) => return Err(DaemonError::Lock(error)),
    };

    // Node Link 接入面：恢复、种子导入与启动恢复都已完成，而本地通道尚未开放（`design.md` D11）。
    // 绑定失败（地址非法、端口被占用、TLS `direct` 的 PEM 不可用、明文开发模式与非 loopback 冲突）
    // 一律拒绝启动，不降级为「无网络接入」运行。
    let net_ingress = NetIngress::start(&composition, event_queue)
        .await
        .map_err(DaemonError::Network)?;

    // 连接级拒绝的审计写任务：endpoint 需要 hook，因此先于 endpoint 创建（由本函数持有到关闭序列）。
    let (audit_hook, audit_writer): (Arc<dyn AuditHook>, AuditWriter) = {
        let (sink, writer) = AuditWriter::start(
            composition.audit_store(),
            Arc::clone(composition.clock()),
            composition.identity().node_id().clone(),
        );
        (sink, writer)
    };

    // endpoint：失败即拒绝启动（§7），不降级为「无管理通道」。
    // `runtime_dir = None`：Unix 按 §2.1 读 `XDG_RUNTIME_DIR`，未设置时回落 `<data_dir>/run`。
    let endpoint_config = LocalEndpointConfig {
        data_dir: composition.config().data_dir.clone(),
        instance_id: instance_id.clone(),
        runtime_dir: None,
    };
    let endpoint = LocalEndpoint::bind(endpoint_config, audit_hook)
        .await
        .map_err(DaemonError::Endpoint)?;
    let endpoint_locator = endpoint.describe();

    // 锁记录在 endpoint 就绪后发布：`endpoint` 字段是 CLI 唯一可行的定位手段（见 `crate::lock`）。
    lock.publish(&LockRecord {
        instance_id: instance_id.as_str().to_owned(),
        pid: std::process::id(),
        endpoint: Some(endpoint_locator.clone()),
    })
    .map_err(DaemonError::Lock)?;

    let shutdown = Arc::new(ShutdownSignal::new());
    let control: Arc<dyn DaemonControl> = Arc::new(AppDaemonControl {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        instance_id: instance_id.as_str().to_owned(),
        data_dir: composition.config().data_dir.display().to_string(),
        public_origin: composition.config().public_origin.clone(),
        listen: net_ingress.listen(),
        started_at: composition.started_at().clone(),
        started_instant: composition.started_instant(),
        node_id: composition.identity().node_id().clone(),
        node_public_key: composition.node_public_key().clone(),
        core: Arc::clone(composition.use_cases()),
        shutdown: Arc::clone(&shutdown),
    });
    let router = LocalAdminRouter::new(LocalAdminDeps {
        // 移入（而非再克隆）：`run` 不得在关闭序列前多持有一个 `Arc<UseCases>` 句柄。
        daemon: control,
        core: Arc::clone(composition.use_cases()),
        keystore: Arc::clone(composition.keystore()),
        audit: composition.audit_store(),
        clock: Arc::clone(composition.clock()),
        pairing: Arc::new(PairingSessions::new(
            Arc::clone(composition.authority()),
            composition.config().public_origin.clone(),
            // `node.revoke`/`export.revoke` 持久提交后的通知缝：真实实现（推送 + 4410 关闭），
            // 不再是「只记日志」的桩（`design.md` D7）。
            Arc::new(NodeLinkCloser::new(Arc::clone(net_ingress.command()))),
        )),
    });
    let handlers = Arc::new(LocalConnectionHandlers::new(Arc::new(ShuttingDownGate {
        inner: Arc::new(router),
        shutdown: Arc::clone(&shutdown),
    })));

    let mut tasks = OwnedTasks::new();
    spawn_maintenance(&mut tasks, &composition, Arc::clone(&shutdown));
    // 合并窗口：按 `storage.flush_interval_ms` 驱动 broker（`Broker` 与 `UseCases` 内是同一实例）。
    let merge_window = Arc::new(BrokerMergeWindow::new(
        Arc::clone(composition.use_cases()),
        Arc::clone(composition.broker()),
    ));
    spawn_merge_window(
        &mut tasks,
        flush_interval,
        merge_window,
        Arc::clone(&shutdown),
    );
    spawn_signal_watcher(&mut tasks, Arc::clone(&shutdown));

    tracing::info!(
        event = "daemon.ready",
        instance_id = instance_id.as_str(),
        endpoint = endpoint_locator.as_str(),
        listen = ?net_ingress.listen(),
        node_id = composition.identity().node_id().as_str(),
        "Daemon 已就绪：本地管理通道与网络接入面开始接受连接"
    );

    let outcome = accept_loop(endpoint, Arc::clone(&handlers), &shutdown).await;
    // 连接处理器持有 router（含 `Arc<UseCases>` 与审计端口）：必须在 `Composition::close` 之前释放，
    // 否则存储仍被共享，检查点会被上报为 `StoreStillShared` 而不是静默跳过。
    drop(handlers);
    let grace_ms = shutdown
        .grace_ms()
        .unwrap_or(composition.config().shutdown_grace_ms);
    tracing::info!(
        event = "daemon.shutdown_begin",
        reason = shutdown.reason(),
        grace_ms,
        "开始关闭序列：停接入层 → 取消后台任务 → 停止 Agent → 刷新存储 → 释放锁"
    );

    close(
        endpoint_locator,
        outcome.connections,
        tasks,
        audit_writer,
        net_ingress,
        Arc::clone(composition.host()),
        composition,
        lock,
        grace_ms,
    )
    .await
}

/// 接受循环的产物。
struct AcceptOutcome {
    /// 已建立的连接任务（由关闭序列排空）。
    connections: JoinSet<()>,
}

/// 接受循环：`PeerRejected` 非致命（本次连接已被拒绝并记审计，继续接受下一条）；其余错误按 F3 修正后的
/// 语义处理——endpoint 在失败后仍可重试，因此记录结构化警告、退避后继续接受，不静默降级为「无管理通道」。
///
/// **接受必须由独立任务承担（不能被 `select!` 取消）**：`LocalEndpoint::accept()` 在 Windows 上会**移走**
/// 当前待连接的 pipe 实例（`take_pending`），中途取消就会把这个实例连同已经连上它的客户端一起丢掉——
/// 客户端已完成 `open()` 却在服务端看到 EOF（实测可复现）。因此这里用一个所有者为接受循环的任务把结果
/// 经容量 1 的通道交出来，循环只 `recv`，不直接取消 `accept()`。
///
/// 补建/失败语义仍由循环决定（它在接受任务之外），因此退避、计数与日志不会因为这条通道而变。
async fn accept_loop(
    endpoint: LocalEndpoint,
    handlers: Arc<LocalConnectionHandlers>,
    shutdown: &Arc<ShutdownSignal>,
) -> AcceptOutcome {
    let mut connections: JoinSet<()> = JoinSet::new();
    let mut consecutive_failures: u32 = 0;
    // 容量 1：同一时刻只排一条待处理连接，既给接受任务自然背压，也保证「已接受的连接立刻被处理」。
    let (sender, mut receiver) =
        tokio::sync::mpsc::channel::<Result<LocalStream, EndpointError>>(1);
    let acceptor = tokio::spawn(async move {
        let mut endpoint = endpoint;
        loop {
            let accepted = endpoint.accept().await;
            if sender.send(accepted).await.is_err() {
                // 循环已退出（关闭序列）：接受任务随之结束，endpoint 在此被 drop（= 停接入层）。
                break;
            }
        }
    });
    loop {
        tokio::select! {
            biased;
            _ = shutdown.wait() => break,
            Some(joined) = connections.join_next(), if !connections.is_empty() => {
                if let Err(error) = joined {
                    tracing::warn!(event = "daemon.connection_task_failed", error = %error);
                }
            }
            accepted = receiver.recv() => match accepted {
                // 接受任务已结束（通道关闭）：与关闭序列同时发生，按关闭处理。
                None => break,
                Some(Ok(stream)) => {
                    consecutive_failures = 0;
                    let handlers = Arc::clone(&handlers);
                    connections.spawn(async move {
                        match serve_connection(stream, handlers).await {
                            Ok(close) => tracing::debug!(
                                event = "daemon.connection_closed",
                                reason = close.as_str()
                            ),
                            Err(error) => tracing::warn!(
                                event = "daemon.connection_failed",
                                error = %error,
                                "本地通道连接以错误结束（按 §3/§4 关闭，不发错误帧）"
                            ),
                        }
                    });
                }
                Some(Err(EndpointError::PeerRejected)) => {
                    // 本次连接已被拒绝（未发送任何 frame、审计已记录）：继续接受下一条。
                    consecutive_failures = 0;
                }
                Some(Err(error)) => {
                    consecutive_failures += 1;
                    tracing::warn!(
                        event = "daemon.accept_failed",
                        consecutive_failures,
                        "接受连接失败：退避后继续（endpoint 在失败后仍可重试）"
                    );
                    tracing::debug!(event = "daemon.accept_failed_detail", error = %error);
                    tokio::time::sleep(ACCEPT_RETRY_BACKOFF).await;
                }
            },
        }
    }
    // 停止接受：先关闭接收端（接受任务会在下一次 `send` 失败或被 abort 时结束），再 abort 掉它——
    // 它可能正阻塞在 `connect()` 上，不 abort 就会拖到下一个客户端才结束。两种方式都会 drop endpoint，
    // 也就是 `close()` 第一步的「停接入层」。这里只 abort 不 await：接受任务没有需要保留的状态。
    drop(receiver);
    acceptor.abort();
    AcceptOutcome { connections }
}

/// 关闭序列（`SECURITY_DESIGN.md` §12.1）。
#[allow(clippy::too_many_arguments)] // 关闭需要把各持有者按顺序移交，参数即顺序
async fn close(
    endpoint_locator: String,
    mut connections: JoinSet<()>,
    tasks: OwnedTasks,
    audit_writer: AuditWriter,
    net_ingress: NetIngress,
    host: Arc<agent_host::AgentHost>,
    composition: Composition,
    lock: DaemonLock,
    grace_ms: u64,
) -> Result<(), DaemonError> {
    let started = Instant::now();
    let deadline = started + Duration::from_millis(grace_ms.max(MIN_DRAIN_MS));

    // 1) 停接入层：本地接受循环已在 `accept_loop` 退出时释放 endpoint（不再接受新连接）；这里**先以同步
    //    调用**触发网络关停——listener 立即停止 accept 并按 `daemon.shutdown_grace_ms` 排空在途连接
    //    （与本地连接的排空共用同一预算）。触发之后才记「已停止接受新连接」：合并成一个 `async fn`
    //    会把触发推迟到 `await` 点，日志与实际时机就会对不上（RV1-WP7-F2）。
    net_ingress.trigger_shutdown();
    tracing::info!(event = "daemon.ingress_stopped", "已停止接受新连接");

    // 排空在途连接：`daemon.stop` 的响应此刻仍在写出路径上，必须给它机会落地。
    let drain_ms = deadline
        .saturating_duration_since(Instant::now())
        .as_millis();
    tracing::info!(
        event = "daemon.drain",
        grace_ms,
        drain_ms = u64::try_from(drain_ms).unwrap_or(u64::MAX),
        "等待在途连接结束"
    );
    drain_connections(&mut connections, deadline).await;
    // 第二步：等三个网络接入任务结束（超时 abort）——它们全部结束时 accept 已停。
    net_ingress.wait_stopped(deadline).await;

    // 2) 取消后台周期任务与信号监听（先于停止 Agent）。
    tasks
        .cancel_all(deadline.min(Instant::now() + TASK_STOP_TIMEOUT))
        .await;
    audit_writer.close().await;

    // 3) 停止 Agent 进程（本切片没有已启动的 Agent，`shutdown_all` 因此立即返回）。
    if tokio::time::timeout_at(
        tokio::time::Instant::from_std(deadline),
        host.shutdown_all(),
    )
    .await
    .is_err()
    {
        tracing::error!(
            event = "daemon.agent_stop_timeout",
            "Agent 进程未在期限内停止（继续关闭序列）"
        );
    } else {
        tracing::info!(event = "daemon.agents_stopped", "本机 Agent 已停止");
    }
    // `AgentHost` 持有存储句柄（`LocalConfigStore`）：必须在 `Composition::close` 之前释放本函数持有的
    // 这个 `Arc` 克隆，否则检查点会被上报为 `StoreStillShared`。
    drop(host);

    // 4) 刷新存储（含 `wal_checkpoint(TRUNCATE)`）。
    composition.close().await.map_err(DaemonError::StoreClose)?;
    tracing::info!(
        event = "daemon.storage_closed",
        "存储已刷新（wal_checkpoint(TRUNCATE)）并关闭连接池"
    );

    // 5) 清理 endpoint 残留与本次运行记录，然后释放单实例锁。
    cleanup_endpoint(&endpoint_locator);
    lock.remove_record();
    drop(lock);
    tracing::info!(
        event = "daemon.stopped",
        elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        "关闭序列完成，单实例锁已释放"
    );
    Ok(())
}

/// 排空在途连接：到 `deadline` 仍未结束的连接被中止（响应已无法送达，也不允许在连接之外继续持有语义）。
async fn drain_connections(connections: &mut JoinSet<()>, deadline: Instant) {
    if connections.is_empty() {
        return;
    }
    let wait = async { while connections.join_next().await.is_some() {} };
    if tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), wait)
        .await
        .is_err()
    {
        let remaining = connections.len();
        connections.abort_all();
        while connections.join_next().await.is_some() {}
        tracing::warn!(
            event = "daemon.drain_timeout",
            remaining,
            "在途连接未在宽限期内结束：已中止"
        );
    }
}

/// 清理 Unix socket 文件残留（Windows 的 Named Pipe 随 listener 释放自动消失）。
///
/// 只删除「确实是 socket 且不是符号链接」的路径：不在持锁期内跟随符号链接删除别的对象。
fn cleanup_endpoint(endpoint_locator: &str) {
    #[cfg(unix)]
    {
        let path = Path::new(endpoint_locator);
        if let Ok(metadata) = std::fs::symlink_metadata(path) {
            use std::os::unix::fs::FileTypeExt as _;
            if metadata.file_type().is_socket() && std::fs::remove_file(path).is_ok() {
                tracing::debug!(event = "daemon.endpoint_removed");
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = endpoint_locator;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    /// 关闭信号：幂等、宽限值只记第一次、等待者会被唤醒。
    #[tokio::test]
    async fn the_shutdown_signal_is_idempotent_and_wakes_waiters() {
        let signal = Arc::new(ShutdownSignal::new());
        assert!(!signal.is_requested());
        let waiter = {
            let signal = Arc::clone(&signal);
            tokio::spawn(async move { signal.wait().await })
        };
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(signal.request(Some(1500)), "首次请求");
        assert!(!signal.request(Some(2000)), "重复请求不是首次");
        assert_eq!(signal.grace_ms(), Some(1500), "宽限值只记第一次");
        tokio::time::timeout(Duration::from_secs(2), waiter)
            .await
            .expect("等待者必须被唤醒")
            .expect("任务正常结束");

        // 已经置位后再等待：立即返回（不挂起）。
        tokio::time::timeout(Duration::from_millis(200), signal.wait())
            .await
            .expect("已请求时立即返回");
    }

    /// 门闸：关闭序列开始后任何请求都以 `local.unavailable` 失败且不触达内层处理器。
    #[tokio::test]
    async fn the_gate_fails_requests_during_shutdown() {
        struct PanicHandler;

        #[async_trait::async_trait]
        impl LocalAdminHandler for PanicHandler {
            async fn handle(&self, _request: AdminRequest) -> AdminResponse {
                unreachable!("关闭期不得触达内层处理器")
            }
        }

        let shutdown = Arc::new(ShutdownSignal::new());
        let gate = ShuttingDownGate {
            inner: Arc::new(PanicHandler),
            shutdown: Arc::clone(&shutdown),
        };
        shutdown.request(None);
        let payload = serde_json::to_vec(&serde_json::json!({
            "v": 1,
            "id": "2ae1c07c-0000-4000-8000-000000000001",
            "method": "daemon.status",
            "params": {},
        }))
        .expect("编码");
        let request = server::local_admin::decode_request(&payload).expect("信封合法");
        let response = gate.handle(request).await;
        match response.outcome() {
            server::local_admin::AdminOutcome::Failure { error } => {
                assert_eq!(error.code(), LocalErrorCode::Unavailable);
                assert!(
                    error.message().contains("shutting down"),
                    "{}",
                    error.message()
                );
            }
            other => panic!("关闭期必须以 local.unavailable 失败，实际 {other:?}"),
        }
    }

    /// 周期任务的取消路径（R16 的机制级覆盖）：协作式取消必须能让一个**已经到点的**短周期任务停下，
    /// 并且 `cancel_all` 只在任务真正退出后才返回——进程里不留下 detached 任务。
    ///
    /// 这里用 20ms 的短周期代替 60s 常量（常量是生产口径，无法在用例里等待），
    /// 覆盖的是同一段取消逻辑：`OwnedTasks::spawn` + `cancel_all`。
    #[tokio::test]
    async fn owned_tasks_stop_a_due_periodic_task_on_cancel() {
        let ticks = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let mut tasks = OwnedTasks::new();
        let cancel = Arc::new(Notify::new());
        let task_cancel = Arc::clone(&cancel);
        let counter = Arc::clone(&ticks);
        tasks.spawn("short_interval", cancel, async move {
            let mut next = tokio::time::Instant::now();
            loop {
                tokio::select! {
                    _ = task_cancel.notified() => break,
                    _ = tokio::time::sleep_until(next) => {}
                }
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                next = tokio::time::Instant::now() + Duration::from_millis(20);
            }
        });
        // 等到任务真的跑过至少两轮（证明它在运转，而不是恰好被取消在第一次 tick 之前）。
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while ticks.load(std::sync::atomic::Ordering::SeqCst) < 2 {
            assert!(
                tokio::time::Instant::now() < deadline,
                "短周期任务没有按时运行"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        tasks
            .cancel_all(std::time::Instant::now() + Duration::from_secs(2))
            .await;
        let after_cancel = ticks.load(std::sync::atomic::Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            ticks.load(std::sync::atomic::Ordering::SeqCst),
            after_cancel,
            "取消后不得再有 tick"
        );
    }

    /// 合并窗口的 spy：记录「枚举」与「pump」的实际调用次数。
    ///
    /// 本切片的本地通道方法集不含 `session.create`（`LOCAL_ADMIN_PROTOCOL.md` §5.8），进程级用例造不出
    /// owned 会话，因此「定时器真的按配置间隔驱动了 broker」只能靠可注入的 spy 断言。
    struct MergeWindowSpy {
        sessions: Vec<SessionId>,
        failing: Option<SessionId>,
        scans: AtomicU32,
        pumps: AtomicU32,
        failures: AtomicU32,
    }

    impl MergeWindowSpy {
        fn new(sessions: Vec<SessionId>, failing: Option<SessionId>) -> Self {
            Self {
                sessions,
                failing,
                scans: AtomicU32::new(0),
                pumps: AtomicU32::new(0),
                failures: AtomicU32::new(0),
            }
        }

        fn scans(&self) -> u32 {
            self.scans.load(Ordering::SeqCst)
        }

        fn pumps(&self) -> u32 {
            self.pumps.load(Ordering::SeqCst)
        }

        fn failures(&self) -> u32 {
            self.failures.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl MergeWindow for MergeWindowSpy {
        async fn sessions(&self) -> Result<Vec<SessionId>, PortError> {
            self.scans.fetch_add(1, Ordering::SeqCst);
            Ok(self.sessions.clone())
        }

        async fn pump(&self, session: &SessionId) -> Result<(), PortError> {
            self.pumps.fetch_add(1, Ordering::SeqCst);
            if self.failing.as_ref() == Some(session) {
                self.failures.fetch_add(1, Ordering::SeqCst);
                return Err(PortError::Unavailable(
                    acp_core::model::UnavailableKind::StorageFull,
                ));
            }
            Ok(())
        }
    }

    fn session(label: &str) -> SessionId {
        SessionId::new(&format!("11111111-1111-4111-8111-{label:0>12}"))
            .expect("canonical UUID 文本")
    }

    /// 合并窗口任务真的按间隔运转：每个 tick 枚举一次、对每个会话各 `pump` 一次；取消后不再调用。
    ///
    /// 这里用 20ms 的短间隔代替配置值的 250ms（配置的解析与默认值由 `crate::config` 的用例断言），
    /// 覆盖的是同一段循环：间隔 → 枚举 → 逐会话 `pump`。
    #[tokio::test]
    async fn the_merge_window_pumps_every_active_session_each_interval_until_cancelled() {
        let spy = Arc::new(MergeWindowSpy::new(vec![session("1"), session("2")], None));
        let shutdown = Arc::new(ShutdownSignal::new());
        let mut tasks = OwnedTasks::new();
        let started = std::time::Instant::now();
        spawn_merge_window(
            &mut tasks,
            Duration::from_millis(20),
            Arc::clone(&spy),
            Arc::clone(&shutdown),
        );

        // 等到真的跑过至少两轮（证明它在运转，而不是恰好卡在第一次 tick 之前）。
        let deadline = started + Duration::from_secs(10);
        while spy.scans() < 2 {
            assert!(
                std::time::Instant::now() < deadline,
                "合并窗口没有按间隔运行"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        // 按间隔而不是自旋：20ms 的间隔在 10s 内不可能产生几百轮（这里只给一个宽松上界，
        // 用于捕获「间隔被忽略、循环退化成热循环」这类真实错误）。
        let scans = spy.scans();
        assert!(
            u64::from(scans) <= elapsed_ms / 20 + 2,
            "{scans} 轮 / {elapsed_ms}ms 与 20ms 的间隔不符（疑似热循环）"
        );

        // 先取消任务、再对**冻结后的同一快照**断言：三个计数必须是同一次读取的结果。若在两次独立
        // load 之间跨过一次 tick 边界（20ms 周期），会读到「枚举已完成、只 pump 了一部分会话」
        // 的中间态（低概率假失败）。
        tasks
            .cancel_all(std::time::Instant::now() + Duration::from_secs(2))
            .await;
        let frozen = (spy.scans(), spy.pumps(), spy.failures());
        assert_eq!(
            frozen.1,
            frozen.0 * 2,
            "每个 tick 必须对每个活动会话各 pump 一次"
        );
        assert_eq!(frozen.2, 0);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            (spy.scans(), spy.pumps(), spy.failures()),
            frozen,
            "取消后不得再枚举或 pump"
        );
    }

    /// 单会话失败不吞不中止：错误被记录（spy 计数即失败次数）、本轮其余会话照常 `pump`、任务继续跑。
    #[tokio::test]
    async fn a_failing_session_does_not_stop_the_rest_of_the_round_or_the_task() {
        let spy = Arc::new(MergeWindowSpy::new(
            vec![session("1"), session("2")],
            Some(session("1")),
        ));
        let shutdown = Arc::new(ShutdownSignal::new());
        let mut tasks = OwnedTasks::new();
        spawn_merge_window(
            &mut tasks,
            Duration::from_millis(20),
            Arc::clone(&spy),
            Arc::clone(&shutdown),
        );

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while spy.failures() < 2 {
            assert!(
                std::time::Instant::now() < deadline,
                "失败的会话没有让任务停下"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        // 关闭请求也要求任务自行退出（``select!`` 的第二个等待点）；先停任务，再对冻结后的**同一
        // 快照**断言，避免两次独立 load 之间跨过 tick 边界（低概率假失败）。
        shutdown.request(None);
        tasks
            .cancel_all(std::time::Instant::now() + Duration::from_secs(2))
            .await;
        let frozen = (spy.scans(), spy.pumps(), spy.failures());
        assert_eq!(frozen.1, frozen.0 * 2, "失败会话之外的会话仍必须收到 pump");
        assert_eq!(frozen.2, frozen.0, "每轮恰好有一个会话失败（不重复、不吞）");
    }

    /// `command` 可解析判定（`daemon.status.agents[].available`）覆盖绝对路径与 `PATH` 查找。
    #[test]
    fn command_availability_covers_paths_and_path_lookup() {
        let existing = std::env::current_exe().expect("当前可执行文件");
        assert!(command_available(
            existing.to_str().expect("可执行文件路径是 UTF-8")
        ));
        assert!(!command_available("./acpr-not-a-real-command-9f3"));
        assert!(!command_available(""));
        // `PATH` 里必然存在的目录本身不是文件；用一个不可能存在的名字断言查找失败。
        assert!(!command_available("acpr-not-a-real-command-9f3"));
        assert!(!candidate_names("x").is_empty());
    }
}
