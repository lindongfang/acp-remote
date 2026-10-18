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
//! 6. 构建 Node Link 与 Sync 接入面、注册静态面并绑定 `daemon.port`/`daemon.bind`（TLS `direct` 的 PEM 在绑定前加载，
//!    任一失败都拒绝启动：`design.md` D10/D11；静态面的注册在协议端点之后，**发布构建**携带占位入口
//!    同样拒绝启动：`sync-ui-hosting` 的 D1/D2）；
//! 7. 本地管理 endpoint（`LocalEndpoint::bind`，`runtime_dir = None` 表示按 §2.1 读 `XDG_RUNTIME_DIR`）；
//! 8. 发布锁记录（含 endpoint 定位串）→ 开放网络接入与本地通道的接受循环。
//!
//! 关闭顺序（`SECURITY_DESIGN.md` §12.1）：停接入层（本地接受循环已退出 + 网络 listener 停止 accept 并
//! 按 `daemon.shutdown_grace_ms` 排空在途连接）→ 取消后台任务（**超时也要 abort 并回收**：任务的端口
//! 克隆还在时 `Arc::try_unwrap` 会失败，`Composition::close` 会报 `StoreStillShared`）→ 停止 Agent →
//! 刷新存储（`wal_checkpoint(TRUNCATE)`）→ 清理 endpoint 残留与运行记录 → 释放单实例锁。
//!
//! 排空**不计发出该 `daemon.stop` 的那条连接**：它的语义是「请求关闭」而不是「正在进行业务」。识别点是
//! [`ShuttingDownGate`]——它按方法判定「这是关闭请求」，并把标记写进该连接自己的 [`ConnectionShutdown`]；
//! 连接因此在写回 `accepted` 响应后自行结束，排空不为它等待对端。其余在途连接仍照旧按宽限等待（宽限耗尽
//! 仍未结束者被中止并留下 `daemon.drain_timeout`）。否则排空时长会取决于该客户端进程何时被调度，而不是
//! 取决于服务端有多少在途工作。
//!
//! 收尾由 [`ShutdownCleanup`] 持有：**无论前面的步骤成功、失败还是提前返回**，清理都会执行，且单实例锁
//! 一定在记录删除之后才释放（否则外部会看到「锁可获取但运行记录仍在」的陈旧记录窗口）。`daemon.stop`
//! 在本序列**开始**时返回 `{accepted: true}`，因此序列的第一步保留了在途连接的排空窗口（见
//! [`MIN_DRAIN_MS`]）。

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
    AdminError, AdminOutcome, AdminRequest, AdminResponse, DaemonAgent, DaemonControl,
    DaemonCounts, DaemonStatus, LocalAdminDeps, LocalAdminHandler, LocalAdminRouter,
    LocalErrorCode, Method, PairingSessions,
};
use server::node_link::{
    CLAIM_PATH, CatalogRoute, CommandRoute, NodeLinkConfig, NodeLinkConn, PairingHttp,
    PairingHttpConfig, ResourceRoute, Routes, STATUS_PATH, WS_PATH, WS_SUBPROTOCOL, WsEndpoint,
    conn::ConnectionRegistry,
};
use server::sync::command::CommandRoute as SyncCommandRoute;
use server::sync::conn::MessageRoute as SyncMessageRoute;
use server::sync::conn::{
    ConnectionRegistry as SyncConnectionRegistry, SyncConfig, SyncConn, WS_PATH as SYNC_WS_PATH,
    WS_SUBPROTOCOL as SYNC_WS_SUBPROTOCOL, WsEndpoint as SyncWsEndpoint,
};
use server::sync::pairing::{
    CLAIM_PATH as SYNC_CLAIM_PATH, LOOKUP_PATH as SYNC_LOOKUP_PATH, PairingHttp as SyncPairingHttp,
    QR_LOOKUP_PATH as SYNC_QR_LOOKUP_PATH, STATUS_PATH as SYNC_STATUS_PATH,
};
use server::sync::router::SyncRoutes;
use server::sync::subscribe::fanout::SyncPublisher;
use server::sync::subscribe::{OwnerReachability, SubscribeRoute};
use server::transport::local::{
    AuditHook, ConnectionShutdown, EndpointError, InstanceId, LocalConnectionHandlers,
    LocalEndpoint, LocalEndpointConfig, LocalStream, serve_connection,
};
// `MissingFrontendBuild` 以别名导入：本模块的 `NetworkError` 有一个同名变体，别名让「错误类型」与
// 「错误变体」在读代码时不会互相顶替（变体携带的就是这个类型）。
use server::transport::net::{
    EmbeddedUi, MissingFrontendBuild as MissingFrontendBuildError, NetConfig, NetError,
    NetListener, RouteError, Shutdown, TlsMode, check_release_build,
};
use tokio::sync::{Notify, mpsc};
use tokio::task::{JoinHandle, JoinSet};

use crate::compose::{
    AuditWriter, ComposeError, Composition, NodeLinkCloser, NodeLinkReachability,
    forked_publisher_with_sync,
};
use crate::config::Config;
use crate::config::Loaded;
use crate::lock::{DaemonLock, LockError, LockRecord};
use storage_sqlite::session_store::SqliteStore;

#[cfg(test)]
mod sync_ingress_tests;
#[cfg(test)]
mod ui_hosting_tests;

/// 周期清理的间隔（`daemon-lifecycle` 规格：每 60 s 的 prune/expire_pairings/sweep_orphans）。
const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(60);

/// 每轮孤儿回收处理的文件数上限（§7.5 的 `limit`；不是配置键）。
const ORPHAN_SWEEP_LIMIT: u32 = 256;

/// 关闭序列里排空在途连接的最小窗口（毫秒）。
///
/// 规格要求 `daemon.stop` **先**返回 `{accepted: true}`（§5.2），而响应由连接任务在 `stop` 返回之后才
/// 写回。`graceMs = 0` 会把「不等待任何在途工作」与「已接受的请求仍要落地」压到同一拍上，因此这里保留
/// 一个很小的下限：关闭请求自身那条连接不被排空等待，它由 `serve_connection` 在写回响应后自行结束
/// （[`ConnectionShutdown`]），因此不占用排空预算；这个下限是给「它写完这条响应，以及同一预算内的网络
/// 接入面收尾」留出的余地。其余在途工作仍按 `graceMs` 处理。实际窗口记在日志里（`event = "daemon.drain"` 的 `window_ms`），不静默。
const MIN_DRAIN_MS: u64 = 200;

/// `accept` 连续失败时的退避（避免在 endpoint 暂时不可用时热循环；F3 修正后失败可重试）。
const ACCEPT_RETRY_BACKOFF: Duration = Duration::from_millis(100);

/// 关闭序列里等待后台任务自行退出的时间上限。
const TASK_STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// 静态面的注册路径（`openspec/changes/archive/2026-10-15-sync-ui-hosting/design.md` D1）：根路径。它同时是「未命中嵌入资源
/// 且不属于协议族」的请求的单页回退落点，因此这里只有这一个字面量。
const STATIC_UI_PATH: &str = "/";

/// 网络接入面（`daemon.port`/`daemon.bind`/`daemon.tls.*`）的启动失败：绑定、TLS 配置或路由注册。
///
/// 三者都是失败关闭：任何一项失败都不得降级为「无网络接入」继续运行（`specs/node-link-listener` 的
/// R1/R3/R14）。
#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    /// `NetListener::bind` 失败（地址非法、端口被占用、TLS 配置不可用、明文开发模式与监听地址冲突）。
    #[error("绑定 `daemon.port`/`daemon.bind` 失败")]
    Listener(#[source] NetError),
    /// 路由注册失败（path 重复或形态非法）：属配置/装配错误，不得静默跳过该端点。
    #[error("注册接入面路由失败")]
    Route(#[source] RouteError),
    /// 发布构建携带的是**占位入口**（构建时没有真实前端产物）。
    ///
    /// 与绑定/注册失败同级：必须拒绝启动，**不**降级为「没有静态面」——那样会把「页面打不开」变成
    /// 「根路径 404」，把构建期缺陷伪装成一个运行期现象（`design.md` D2 的失败关闭）。
    #[error("发布构建缺少前端产物：{0}")]
    MissingFrontendBuild(#[source] MissingFrontendBuildError),
}

impl NetworkError {
    /// 不含完整敏感路径的简短说明（TLS 文件路径不进日志，`SECURITY_DESIGN.md` §14.1）。
    fn message(&self) -> String {
        match self {
            Self::Listener(NetError::ProxyNonLoopback { .. }) => {
                "`proxy` mode only permits a loopback `daemon.bind`".to_owned()
            }
            Self::Listener(NetError::PlaintextDevNonLoopback { .. }) => {
                "`dev_mode.allow_plaintext` only applies to a loopback `daemon.bind`".to_owned()
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
                "`daemon.bind`/`daemon.port` cannot be bound (address in use or not permitted)"
                    .to_owned()
            }
            Self::Listener(NetError::Serve { .. }) => {
                "the listener failed while serving".to_owned()
            }
            Self::Route(_) => "the ingress route cannot be registered".to_owned(),
            // 逐字给出 `MissingFrontendBuild` 的说明：它写明了下一步（先在 `clients/app` 构建前端），
            // 把它归约成一句「分类」就等于让运维拿到一个查不到动作的错误码。
            Self::MissingFrontendBuild(error) => error.to_string(),
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
///
/// 它同时是**关闭请求者的识别点**：「哪条连接发出了 `daemon.stop`」在这里判定，判定的结果写进本连接的
/// 标记（[`ConnectionShutdown`]），连接据此在写回 `accepted` 响应后自行结束。关闭序列因此不必（也不能）
/// 按「谁先关/谁最后关」猜：发出关闭请求的那条连接不被排空等待（它会自行结束，因此不占用排空预算），
/// 排空时长不再取决于该客户端进程何时被调度（`daemon-lifecycle` 规格的「关闭时的连接排空」）。
struct ShuttingDownGate {
    inner: Arc<dyn LocalAdminHandler>,
    shutdown: Arc<ShutdownSignal>,
    /// 本连接的关闭请求标记（每条连接一份，与 `serve_connection` 看到的是同一个）。
    connection: Arc<ConnectionShutdown>,
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
        let closing = request.method() == Method::DaemonStop;
        let response = self.inner.handle(request).await;
        // 只有**被接受**的关闭请求才标记本连接（`accepted: true`）：参数非法等失败不启动关闭序列，那条
        // 连接照旧是一条普通的管理连接。置位与响应写回同属一条连接任务的先后两步——置位必然发生在
        // `serve_connection` 看到标记之前，因此不构成竞态。
        if closing && matches!(response.outcome(), AdminOutcome::Success { .. }) {
            self.connection.request();
        }
        response
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
    /// `daemon.status.uiBuildVersion`：本次二进制**嵌入并被托管**的前端构建版本（`design.md` D3）。
    ///
    /// 取值来自注册为静态面的那份嵌入资源表（[`NetIngress::ui_build_version`]，即 [`EmbeddedUi::build_version`]
    /// 的结果），因此版本与浏览器拿到的资源必然出自同一次构建。`None` = 版本缺失（占位入口，或产物里
    /// 没有可解析的版本文件），是**显式**的缺失语义：不编造、也不拿资源哈希顶替。
    ui_build_version: Option<String>,
    shutdown: Arc<ShutdownSignal>,
}

#[async_trait::async_trait]
impl DaemonControl for AppDaemonControl {
    async fn status(&self) -> DaemonStatus {
        // `links` 恒空：Node Link 的**出站**重连管理器属切片 6（入站连接由 `server::node_link` 的
        // 连接注册表管理，不进 `daemon.status`，§5.2 的字段注记）。
        DaemonStatus {
            ui_build_version: self.ui_build_version.clone(),
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
    /// 组装期状态的**唯一**注入点：`run` 与用例共用它，因此「`daemon.status` 的每个字段从哪来」不会在
    /// 生产路径与测试之间分叉。
    ///
    /// `ingress` 是网络接入面的所有者句柄（[`NetIngress`]）：`listen` 与 `ui_build_version` 都从它取，
    /// 而**前端构建版本**取的正是注册为静态面的那份嵌入资源表——`daemon.status` 回答的版本与浏览器拿到
    /// 的资源因此是同一份（`design.md` D3 的「同源」由这条接线保证，而不是靠两处各自读一次编译期常量）。
    fn new(
        composition: &Composition,
        instance_id: &InstanceId,
        ingress: &NetIngress,
        shutdown: Arc<ShutdownSignal>,
    ) -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            instance_id: instance_id.as_str().to_owned(),
            data_dir: composition.config().data_dir.display().to_string(),
            public_origin: composition.config().public_origin.clone(),
            listen: ingress.listen(),
            started_at: composition.started_at().clone(),
            started_instant: composition.started_instant(),
            node_id: composition.identity().node_id().clone(),
            node_public_key: composition.node_public_key().clone(),
            core: Arc::clone(composition.use_cases()),
            ui_build_version: ingress.ui_build_version(),
            shutdown,
        }
    }

    /// `counts`：由 core 查询聚合（与持久记录一致）。
    async fn counts(&self) -> DaemonCounts {
        let actor = Actor::LocalCli;
        DaemonCounts {
            devices: count(self.core.devices(&actor).await, "devices"),
            nodes: count(self.core.nodes(&actor).await, "nodes"),
        }
    }

    /// `agents`：profile 存在且 `command` 可解析（`design.md` 决策 6 的本切片口径）。
    async fn agents(&self) -> Vec<DaemonAgent> {
        match self.core.agent_profiles(&Actor::LocalCli).await {
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

    /// 取消并等待全部任务结束（`deadline` 之前；超时才 abort，但**仍要等它被回收**）。
    ///
    /// 超时分支不能只丢掉 `JoinHandle`（那是 detach）：被 detach 的任务会继续运行，并在
    /// `Composition::close` 之前一直持有端口克隆（每个端口都是同一个 `Arc<SqliteStore>` 的
    /// `store.clone()`，见 `crate::compose`），于是关闭序列随机报 `StoreStillShared`。
    async fn cancel_all(self, deadline: Instant) {
        for task in &self.tasks {
            task.cancel.notify_waiters();
        }
        for task in self.tasks {
            let name = task.name;
            let mut handle = task.handle;
            // 借 `&mut handle` 等：超时后还要 abort 并 `await` 同一个句柄，所以不能把它移动进
            // `timeout_at`（那会在超时时直接 drop 句柄 = detach）。
            match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), &mut handle)
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
                Err(_) => {
                    handle.abort();
                    let _ = handle.await;
                    tracing::warn!(
                        event = "daemon.task_stop_timeout",
                        task = name,
                        "后台任务未在期限内退出：已中止并回收"
                    );
                }
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
    let authority = Arc::clone(composition.authority());
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
                &authority,
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

/// 会话空闲回收：`sessions.idle_timeout_ms` 非零时按 `agent-host` 的固定粒度扫描（规则本身由配置
/// 决定）。与其它周期任务共享取消路径，关闭序列里先停它再停 Agent。
fn spawn_agent_idle_sweep(
    tasks: &mut OwnedTasks,
    composition: &Composition,
    shutdown: Arc<ShutdownSignal>,
) {
    let Some(idle_timeout) = composition.config().sessions.idle_timeout else {
        return;
    };
    let cancel = Arc::new(Notify::new());
    let task_cancel = Arc::clone(&cancel);
    let host = Arc::clone(composition.host());
    tasks.spawn("agent_idle_sweep", cancel, async move {
        let mut ticker = tokio::time::interval(agent_host::limits::IDLE_SWEEP_INTERVAL);
        loop {
            tokio::select! {
                _ = task_cancel.notified() => break,
                _ = ticker.tick() => {}
            }
            if shutdown.is_requested() {
                break;
            }
            host.sweep_idle(idle_timeout).await;
        }
    });
}

/// 一轮清理：`prune` + 配对内存/持久过期 + `sweep_orphans`（各自失败只记日志，不中断后续）。
async fn maintenance_tick(
    core: &Arc<acp_core::use_cases::UseCases>,
    authority: &Arc<Authority>,
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
    let at = clock.now();
    let expired_pairings = match expire_due_pairings(core, authority, &at).await {
        Ok(count) => count,
        Err(error) => {
            tracing::warn!(
                event = "daemon.maintenance_failed",
                step = "expire_pairings",
                error = crate::compose::port_error_token(&error)
            );
            // 列表读取失败时退回存储层的时间扫描：内存 secret 留到下一轮，但持久记录不会因此停在
            // `created`（过期时间一到仍可被终结）。
            match core.expire_pairings(&actor).await {
                Ok(count) => count,
                Err(fallback) => {
                    tracing::warn!(
                        event = "daemon.maintenance_failed",
                        step = "expire_pairings_fallback",
                        error = crate::compose::port_error_token(&fallback)
                    );
                    0
                }
            }
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

/// 进程启动时的配对内存态恢复（`IDENTITY_AND_AUTH_CONTRACT.md` §4.3、`identity-pairing` 的
/// 「配对码与 secret 的生命周期」）。
///
/// 顺序固定为：`Authority::reset_memory`（丢弃上一个进程留下的 secret 与配对码映射——它们不能从持久化
/// 摘要重建）→ 读全部配对记录 → `Authority::unrecoverable_after_restart`（`created`/`pending_confirmation`
/// 已无法继续验密）→ 用同一写集把这些行终结为 `expired`。已批准的信任记录不在未确认集合里，不受影响。
///
/// 返回被终结的配对数。失败时调用方只记日志、不阻止启动：这些配对本来就不能被认领（内存 secret 已清空），
/// 到期后仍会被周期维护终结。
///
/// 公开是为了让集成测试用**真实**的 core/Authority 调同一个入口（`app::daemon` 的其他接线点同样公开给
/// 受控路径集成测试）；它不是管理协议的一部分。
pub async fn recover_pairings_after_restart(
    core: &Arc<UseCases>,
    authority: &Arc<Authority>,
) -> Result<u64, PortError> {
    let (secrets, challenges) = authority.reset_memory();
    tracing::info!(
        event = "daemon.pairing_memory_reset",
        secrets,
        challenges,
        "已清空上一个进程的配对 secret 与挑战缓存"
    );
    let actor = Actor::LocalCli;
    let records = core.pairings(&actor).await?;
    let unrecoverable = authority.unrecoverable_after_restart(&records);
    let terminated = core.expire_pairings_by_id(&actor, &unrecoverable).await?;
    if terminated > 0 {
        tracing::warn!(
            event = "daemon.pairings_terminated_on_restart",
            pairings = terminated,
            "重启后已无法继续验密的未确认配对已被终结"
        );
    }
    Ok(terminated)
}

/// 周期维护的配对一步：读全部记录 → `Authority::due_pairings`（同时清除内存 secret 与配对码映射）→
/// 按返回的标识把持久记录终结为 `expired`。
///
/// 只终结 `due_pairings` 判定的记录；它已经排除了终态记录与仍有效的配对，已批准但已到期的记录只清内存
/// secret、不改写持久状态（存储层只更新未确认的行）。返回被终结的配对数。
pub async fn expire_due_pairings(
    core: &Arc<UseCases>,
    authority: &Arc<Authority>,
    at: &Timestamp,
) -> Result<u64, PortError> {
    let actor = Actor::LocalCli;
    let records = core.pairings(&actor).await?;
    let due = authority.due_pairings(&records, at);
    core.expire_pairings_by_id(&actor, &due).await
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
        port: config.port,
        bind: config.bind,
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

/// `app::config` → `server::sync::conn::SyncConfig`（`sync.*` 三个键的**唯一消费点**）。
///
/// 「只下调」在配置解析期就已收敛（[`crate::config`]：高于 §14 默认值的取值一律取默认值），因此这里是一对
/// 一映射、不再夹取。协议层 `SessionLimits::negotiate` 仍会再夹一次 `[wire 下限, §14 默认值]`——那是
/// 「任何路径都不得放宽」的构造期不变量，与本函数不重复。
pub fn sync_config(config: &Config) -> SyncConfig {
    config.sync.clone()
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

/// Sync 入站面的装配件：活跃连接注册表（撤销钩子与后续事件扇出共用同一个句柄）。
///
/// 连接面本身（[`SyncConn`]）不在这里持有：它由注册给 listener 的 WS handler 拥有，与 listener 同生命周期。
pub struct SyncIngress {
    registry: Arc<SyncConnectionRegistry>,
    /// 实时事件扇出的接收端（`NetIngress::start` spawn 分发循环；发布端由组合根的
    /// `EventPublisher` 分叉持有，见 `compose::forked_publisher`）。
    event_queue: tokio::sync::mpsc::Receiver<CommittedEvent>,
    /// 订阅面路由。它同时是注册给 `/sync/v1` 的 [`SyncRoutes`] 的一个子路由：分发循环按它的屏障状态找
    /// 实时目标，因此这里必须持有**同一个**实例（另一份克隆会让扇出看不到路由线程建立的屏障）。
    subscribe: Arc<SubscribeRoute>,
    /// 命令面路由。组合根必须持有**同一个**实例来 spawn 它的终态观察者（`CommandRoute::dispatch`），
    /// 否则 `accepted` 之后的 mutation 终态永远没人推。
    command: Arc<SyncCommandRoute>,
}

impl SyncIngress {
    /// Sync 的活跃连接注册表（键是 `(hostId, deviceId)`；设备撤销钩子在这里请求关闭）。
    pub fn registry(&self) -> &Arc<SyncConnectionRegistry> {
        &self.registry
    }

    /// 命令面路由（终态观察者的 spawn 来源）。
    pub fn command(&self) -> &Arc<SyncCommandRoute> {
        &self.command
    }

    /// 拆出实时扇出的三个零件（路由 + 注册表 + 队列），交给组合根 spawn 分发循环。
    pub fn into_fanout(
        self,
    ) -> (
        Arc<SubscribeRoute>,
        Arc<SyncConnectionRegistry>,
        tokio::sync::mpsc::Receiver<CommittedEvent>,
    ) {
        (self.subscribe, self.registry, self.event_queue)
    }
}

/// 把 Sync 的全部 path 注册到共享 listener（WSS + 配对 claim/status）并返回连接注册表句柄。
///
/// 这是**唯一的 Sync 接线点**：组合根（[`NetIngress::start`]）与受控路径集成测试都调它，因此
/// 「subprotocol 常量」「认证后消息的路由组合」「配对端点的安全响应头」不会在两处漂移：认证后的消息由
/// [`SyncRoutes`]（订阅面 → 命令面）承接，`sync.*` 走 `SubscribeRoute`、`command` 走 `SyncCommandRoute`。
/// 命令面本身覆盖 §11.5 的全部 13 条命令——包括只读目录浏览 `workspace.browse`：它的目录读取由
/// `UseCases` 持有的目录浏览端口承担，因此组合根这里**不需要**再接一个目录读取实现（端口表见
/// [`crate::compose`]）。
///
/// 配对端点的四个安全响应头经 `SyncPairingHttp::default_response_headers` **同时**声明给接入层
/// （`register_post` 的每路径默认响应头）：否则接入层在调用处理器前产生的 413/Host 400 不带这些头。
/// 四个配对 path 都注册在这里：claim、status、网页入口的 lookup（`/sync/v1/pairing/lookup`）与
/// 原生 App 扫码入口的 qr-lookup（`/sync/v1/pairing/qr-lookup`）。
///
/// `public_origin` 是 `daemon.public_origin`：lookup 与 qr-lookup 用它做**前置**来源校验（跨源请求不得
/// 消耗配对码或探测扫码凭据），并与配对登记的 `host_binding` 再核对一次。`None` = 未配置：两条入口
/// 一律统一失败。
///
/// `owner` 是 Owner 可达性（`sync.subscribe`/快照的运行时输入）：`server::sync` 不得调用平级的
/// `server::node_link`，因此这条事实只能由组合根注入（见 [`NodeLinkReachability`]）。
#[allow(clippy::too_many_arguments)] // 接线参数各自是独立持有者，收进结构体只会把装配顺序藏起来
pub fn register_sync_paths(
    listener: &mut NetListener,
    core: &Arc<UseCases>,
    authority: &Arc<Authority>,
    broker: &Arc<Broker>,
    config: &SyncConfig,
    public_origin: Option<String>,
    owner: Arc<dyn OwnerReachability>,
    event_queue: Option<tokio::sync::mpsc::Receiver<CommittedEvent>>,
) -> Result<SyncIngress, NetworkError> {
    let registry = SyncConnectionRegistry::new();
    // 认证后消息路由：订阅面（`sync.subscribe`/`sync.snapshot_request`/`sync.ack`）与它的事件投递腿
    // （增量重放 + 实时扇出，见 `server::sync::subscribe` 的模块文档）。
    let subscribe = Arc::new(SubscribeRoute::new(
        Arc::clone(core),
        Arc::clone(broker),
        Arc::clone(&registry),
        Arc::clone(&owner),
    ));
    // 命令面（`command`）：`SyncConn` 只有一个路由槽，因此两者由 `SyncRoutes` 按序组合（订阅面在前）；
    // `on_authenticated` 由组合件转发给两者，本次协商出的 feature 因此同时到达订阅面与命令面的门控。
    let command = Arc::new(SyncCommandRoute::new(
        Arc::clone(core),
        Arc::clone(broker),
        Arc::clone(authority),
        Arc::clone(&registry),
        owner,
    ));
    let route = Arc::new(SyncRoutes::new(vec![
        Arc::clone(&subscribe) as Arc<dyn SyncMessageRoute>,
        Arc::clone(&command) as Arc<dyn SyncMessageRoute>,
    ]));
    // 实时扇出的接收端由组合根经 `sync_publisher.take_receiver()` 取出后交进来：发布端先装进组合根的
    // `EventPublisher` 分叉（两侧生命周期必然错开）。`None` = 取过了，这条进线上不会有实时投递——
    // 接线错误，日志如实记下（快照与重放腿不受影响，客户端断线重连后仍能补齐）。
    let Some(event_queue) = event_queue else {
        tracing::error!(
            event = "daemon.sync_fanout_receiver_missing",
            "Sync 实时事件接收端已被取走：实时投递不会发生（重放腿不受影响）"
        );
        return Err(NetworkError::Route(RouteError::InvalidPath {
            path: "/sync/v1".to_owned(),
        }));
    };
    let conn = Arc::new(
        SyncConn::new(
            Arc::clone(core),
            Arc::clone(authority),
            config.clone(),
            Arc::clone(&registry),
        )
        .with_route(route),
    );
    listener
        .register_ws(
            SYNC_WS_PATH,
            SYNC_WS_SUBPROTOCOL,
            SyncWsEndpoint::handler(conn),
        )
        .map_err(NetworkError::Route)?;
    let pairing = Arc::new(SyncPairingHttp::new(
        Arc::clone(core),
        Arc::clone(authority),
        public_origin,
    ));
    let default_headers = pairing.default_response_headers();
    listener
        .register_post(SYNC_CLAIM_PATH, pairing.claim_handler(), default_headers)
        .map_err(NetworkError::Route)?;
    listener
        .register_post(SYNC_STATUS_PATH, pairing.status_handler(), default_headers)
        .map_err(NetworkError::Route)?;
    listener
        .register_post(SYNC_LOOKUP_PATH, pairing.lookup_handler(), default_headers)
        .map_err(NetworkError::Route)?;
    listener
        .register_post(
            SYNC_QR_LOOKUP_PATH,
            pairing.qr_lookup_handler(),
            default_headers,
        )
        .map_err(NetworkError::Route)?;
    Ok(SyncIngress {
        registry,
        event_queue,
        subscribe,
        command,
    })
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
    /// Sync 的活跃连接注册表（设备撤销钩子的关闭对象）。
    sync_registry: Arc<SyncConnectionRegistry>,
    /// 装配期读到的前端构建版本（`EmbeddedUi::build_version`）；`None` = 版本缺失。
    ///
    /// 在 [`NetIngress::start`] 里从**注册为静态面的那份资源表**读一次并留在这里：`daemon.status` 的回答
    /// 因此与浏览器拿到的资源同源，且不在每次 `daemon.status` 时重复解析版本文件。
    ui_build_version: Option<String>,
    tasks: Vec<(&'static str, JoinHandle<()>)>,
}

impl NetIngress {
    /// 装配并启动网络接入面：绑定 `daemon.port`/`daemon.bind`（含 TLS 加载）→ 注册 Node Link 与 Sync 的全部 path
    /// → spawn 三个任务。
    ///
    /// 绑定或注册失败时返回 `Err`：调用方必须拒绝启动（**不**降级为「无网络接入」），且此时还没有任何
    /// 任务被 spawn（listener 在出错路径上随 `listener` 一起 drop，端口立即释放）。
    pub async fn start(
        composition: &Composition,
        event_queue: mpsc::Receiver<CommittedEvent>,
        sync_events: Option<mpsc::Receiver<CommittedEvent>>,
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
        // Sync 面：Owner 可达性只从 Node Link 的连接注册表读（两个适配器不互相调用，组合根是唯一的缝合点）。
        let sync = register_sync_paths(
            &mut listener,
            &core,
            &authority,
            composition.broker(),
            &sync_config(composition.config()),
            composition.config().public_origin.clone(),
            Arc::new(NodeLinkReachability::new(Arc::clone(ingress.registry()))),
            sync_events,
        )?;
        // 静态面（`design.md` D1/D2/D3）：第三类端点，注册在**全部协议端点之后**（`tasks.md` 3.1 的装配
        // 要求）。注意「协议端点优先」**不**由这个顺序保证，而是由 `server` 侧的结构保证：协议端点是精确
        // 路由，静态面只是 `router.fallback(static_ui::serve)`，因此命中的请求根本到不了静态面。注册顺序
        // 也不是靠冲突检查兜住的——`RouteTable` 的重复 path 检查只对**同一 path**（以及第二条静态端点）
        // 报错，而静态面的 path 是 `/`、协议 path 是 `/sync/v1`、`/node-link/v1` 及各自的 pairing 子路径，
        // 所以「先注册静态面」会被静默接受。顺序仍然照 3.1 写，是因为它是被审阅过的装配形态，不是因为它
        // 会被检查出来。
        //
        // 资源表在 `server` 侧是编译期二选一：真实产物或树内占位入口。**发布构建**携带占位入口即拒绝
        // 启动（失败关闭，不降级为「根路径 404」）；`release_build` 由组合根判定，不写死在
        // `check_release_build` 里，这样两条分支都各自可测（`!cfg!(debug_assertions)`：产物不入库，
        // CI 的 Rust 作业永远面对占位入口，无条件失败关闭会把整片 `cargo test --workspace` 弄红）。
        let ui = Arc::new(EmbeddedUi::new());
        check_release_build(ui.is_placeholder(), !cfg!(debug_assertions))
            .map_err(NetworkError::MissingFrontendBuild)?;
        // 版本必须在把 `ui` 交给静态面**之前**从同一个实例读：`daemon.status` 回答的版本与浏览器拿到的
        // 资源因此同属一份资源表，不可能分属两次构建。
        let ui_build_version = ui.build_version();
        // `Arc<EmbeddedUi>` → `Arc<dyn UiAssetSource>` 的 unsize 强转由**实参位置**完成（函数调用的实参
        // 是强转点，与本文件 `owner: Arc<dyn OwnerReachability>` 的接线同款）。不要改写成
        // `Arc::clone(&ui)`：那会让 `Arc::clone` 的返回类型被实参的期望类型反推成 `Arc<dyn …>`，它的参数
        // `&Arc<EmbeddedUi>` 于是不匹配（E0308）。
        listener
            .register_static(STATIC_UI_PATH, ui)
            .map_err(NetworkError::Route)?;
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
        // Sync 的实时事件扇出（`event` 帧的实时腿；重放腿在订阅面的 `advance` 里）。
        // 停读信号（§14 的高水位）由每连接的出站账本负责、在分发侧消费；`shutdown` 触发即退出。
        let sync_registry = Arc::clone(sync.registry());
        // Sync 命令面的终态观察者：mutation 的 `accepted` 之后，终态由它轮询持久记录后推回提交方。
        let sync_command = Arc::clone(sync.command());
        tasks.push((
            "sync_commands",
            tokio::spawn({
                let signal = signal.clone();
                async move { sync_command.dispatch(signal).await }
            }),
        ));
        let (sync_route, fanout_registry, sync_events) = sync.into_fanout();
        let sync_reader = sync_route.reader();
        tasks.push((
            "sync_fanout",
            tokio::spawn(async move {
                server::sync::subscribe::fanout::dispatch(
                    fanout_registry,
                    sync_route,
                    sync_reader,
                    sync_events,
                    signal,
                )
                .await
            }),
        ));
        tracing::info!(
            event = "daemon.ingress_ready",
            listen = ?listen,
            // 版本进日志是 D3 的诊断入口之一（「刷新后仍拿到旧 JS 去连新 Daemon」）：`None` = 缺失语义，
            // 不静默当成「版本未知」。
            ui_build_version = ?ui_build_version,
            "网络接入面已就绪：Node Link 与 Sync 的 WSS 与配对 HTTP 开始接受连接，根路径由前端静态面接管"
        );
        Ok(Self {
            shutdown,
            listen,
            command: ingress.command,
            registry: ingress.registry,
            sync_registry,
            ui_build_version,
            tasks,
        })
    }

    /// 实际绑定的监听地址（`daemon.status.listen` 的来源）。
    pub fn listen(&self) -> Vec<String> {
        self.listen.clone()
    }

    /// 本次二进制嵌入并被托管的前端构建版本（`daemon.status.uiBuildVersion` 的来源）。
    ///
    /// 由 [`NetIngress::start`] 从注册为静态面的那份资源表读出（`design.md` D3）；`None` 表示版本缺失
    /// （占位入口或产物里没有可解析的版本文件），是显式的缺失语义。
    pub fn ui_build_version(&self) -> Option<String> {
        self.ui_build_version.clone()
    }

    /// 命令路由句柄（撤销通知缝 [`NetIngress::connection_closer`] 的内部来源）。
    pub fn command(&self) -> &Arc<CommandRoute> {
        &self.command
    }

    /// 连接注册表句柄（撤销传播的观察面；目前只由测试与 `CommandRoute` 使用）。
    pub fn registry(&self) -> &Arc<ConnectionRegistry> {
        &self.registry
    }

    /// Sync 的连接注册表句柄（设备撤销钩子 `NodeLinkCloser` 装配用：`device.revoke` 提交后关的就是它）。
    pub fn sync_registry(&self) -> &Arc<SyncConnectionRegistry> {
        &self.sync_registry
    }

    /// 撤销通知缝的装配：Node Link 的命令管线（节点撤销/重新配对的连接作废与 Export 通知）**加上**
    /// Sync 的连接注册表（设备撤销关闭的活动连接）。
    ///
    /// 这是**唯一**的撤销缝接线点——组合根（[`run`]）与受控路径用例都调它，因此「设备撤销关的是哪张
    /// 连接表」不会在两处漂移（此前 `close_device` 恒报「没有可关闭的连接」正是因为这里没有第二张表）。
    pub fn connection_closer(&self) -> NodeLinkCloser {
        NodeLinkCloser::new(Arc::clone(&self.command), Arc::clone(&self.sync_registry))
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
    let bind_is_non_loopback = config
        .bind
        .map_or(!matches!(&config.tls, TlsMode::Proxy), |ip| {
            !ip.is_loopback()
        });
    if config.public_origin.is_none() {
        if bind_is_non_loopback {
            tracing::warn!(
                event = "daemon.public_origin_missing",
                "绑定非 loopback 但未配置 `daemon.public_origin`：已有设备仍可认证，新配对与 Node Link 将以 local.unavailable 失败关闭"
            );
        } else {
            tracing::debug!(
                event = "daemon.public_origin_missing",
                "未配置 `daemon.public_origin`：仅 loopback 访问不需要它；配对方法会在使用前明确失败"
            );
        }
    }
    let flush_interval = Duration::from_millis(config.flush_interval_ms);
    // 事件扇出的分叉在装配 broker 之前建立：发布端口注入 `Composition`，队列交给网络接入面的分发循环
    // （`design.md` D6）。两者必须同时存在，否则已提交事件会在消费者出现前被丢弃。
    //
    // Sync 的实时腿（`event` 帧）挂在同一个分叉上：发布端先进 `Composition`，接收端由
    // `NetIngress::start` 从 `SyncPublisher` 上取走。`server::sync` 不得调用平级的
    // `server::node_link`，因此这条缝合只能发生在 app 侧。
    let mut sync_publisher = SyncPublisher::channel();
    let sync_events = sync_publisher.take_receiver();
    let (publisher, event_queue) = forked_publisher_with_sync(Some(sync_publisher));

    let composition = Composition::assemble(config, publisher)
        .await
        .map_err(DaemonError::Compose)?;

    // §6 第 16 条：启动恢复在取锁与开始监听**之前**（`LocalCli`）。
    let recovered = composition
        .use_cases()
        .recover_unsettled(&Actor::LocalCli, acp_core::ports::ReplayLimit::default())
        .await
        .map_err(|error| DaemonError::Compose(ComposeError::Port(error)))?;
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

    // 配对内存态的重启语义（§4.3、`CORE_PORTS_AND_STORAGE.md` §11.6 第 1 条）：取锁成功**之后**、
    // 开始监听之前先清空 secret/配对码，再终结已无法继续验密的未确认配对——否则第二个进程会在启动
    // 竞争里把活着的 daemon 的已认领配对清掉。失败只记日志、不阻止启动：这些配对本来就不能被认领，
    // 到期后仍会被周期维护终结。
    if let Err(error) =
        recover_pairings_after_restart(composition.use_cases(), composition.authority()).await
    {
        tracing::warn!(
            event = "daemon.pairing_recovery_failed",
            error = crate::compose::port_error_token(&error),
            "重启后的配对终结未能提交：这些配对将在到期时被周期维护清理"
        );
    }

    // Node Link 接入面：恢复、种子导入与启动恢复都已完成，而本地通道尚未开放（`design.md` D11）。
    // 绑定失败（地址非法、端口被占用、TLS `direct` 的 PEM 不可用、明文开发模式与非 loopback 冲突）
    // 一律拒绝启动，不降级为「无网络接入」运行。
    let net_ingress = NetIngress::start(&composition, event_queue, sync_events)
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
    let control: Arc<dyn DaemonControl> = Arc::new(AppDaemonControl::new(
        &composition,
        &instance_id,
        &net_ingress,
        Arc::clone(&shutdown),
    ));
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
            composition.config().node_name.clone(),
            // `node.revoke`/`export.revoke` 持久提交后的通知缝：真实实现（推送 + 4410 关闭），
            // 不再是「只记日志」的桩（`design.md` D7）。`device.revoke` 关的是 Sync 的连接注册表。
            Arc::new(net_ingress.connection_closer()),
        )),
    });
    // 共享的管理处理器（router）。**每条连接**的处理器由 `accept_loop` 按连接装配：关闭请求标记
    // （[`ConnectionShutdown`]）必须与 `serve_connection` 看到的是同一个，因此它不能在这里跨连接复用。
    let admin: Arc<dyn LocalAdminHandler> = Arc::new(router);

    let mut tasks = OwnedTasks::new();
    spawn_maintenance(&mut tasks, &composition, Arc::clone(&shutdown));
    spawn_agent_idle_sweep(&mut tasks, &composition, Arc::clone(&shutdown));
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

    let outcome = accept_loop(endpoint, admin, &shutdown).await;
    // 连接处理器持有 router（含 `Arc<UseCases>` 与审计端口）：必须在 `Composition::close` 之前释放，
    // 否则存储仍被共享，检查点会被上报为 `StoreStillShared` 而不是静默跳过。`accept_loop` 返回时已释放
    // 它持有的处理器引用，各连接任务在排空（或中止）时释放各自的。
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
    /// 已建立的连接任务（由关闭序列排空）。发出关闭请求的那条连接写出响应后即自行结束，因此它不会让
    /// 排空等在对端上。
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
    admin: Arc<dyn LocalAdminHandler>,
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
                    // 每条连接一份处理器：关闭请求标记（`ConnectionShutdown`）由管理面置位、由
                    // `serve_connection` 读取，两者必须看到同一个，因此在这里成对创建。
                    let connection = Arc::new(ConnectionShutdown::new());
                    let handlers = Arc::new(LocalConnectionHandlers::with_shutdown(
                        Arc::new(ShuttingDownGate {
                            inner: Arc::clone(&admin),
                            shutdown: Arc::clone(shutdown),
                            connection: Arc::clone(&connection),
                        }),
                        connection,
                    ));
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
    // 收尾守卫：持有单实例锁与 endpoint 定位串。此后任何提前返回（`?`、panic）都会经 `Drop` 走
    // 「清理 endpoint 与运行记录 → 释放锁」，因此不会留下「锁可获取但运行记录仍在」的窗口。
    let cleanup = ShutdownCleanup::new(endpoint_locator, lock);
    let deadline = started + Duration::from_millis(grace_ms.max(MIN_DRAIN_MS));

    // 1) 停接入层：本地接受循环已在 `accept_loop` 退出时释放 endpoint（不再接受新连接）；这里**先以同步
    //    调用**触发网络关停——listener 立即停止 accept 并按 `daemon.shutdown_grace_ms` 排空在途连接
    //    （与本地连接的排空共用同一预算）。触发之后才记「已停止接受新连接」：合并成一个 `async fn`
    //    会把触发推迟到 `await` 点，日志与实际时机就会对不上。
    net_ingress.trigger_shutdown();
    tracing::info!(event = "daemon.ingress_stopped", "已停止接受新连接");

    // 接入任务必须先真正结束（accept 已停）才进入排空：`trigger_shutdown` 只是发出取消信号，接受循环
    // 此刻可能仍在 accept（内核 backlog 也会照常完成握手）。规格 §daemon-lifecycle 的顺序是
    // 「停接入层（不再接受新连接）→ 取消后台周期任务」，排空在途连接夹在中间会让「停接入层」退化成
    // 一个信号而非事实。`wait_stopped` 之后 `daemon.task_stopped` 已记录，那才是 accept 已停的观测点。
    net_ingress.wait_stopped(deadline).await;

    // 排空在途连接：`daemon.stop` 的响应此刻仍在写出路径上（发出该请求的那条连接写出后即自行结束，
    // 因此不为它等待对端）；其余在途连接必须在宽限内结束，否则被中止并记 `daemon.drain_timeout`。
    //
    // 两个字段必须分开报，不能互相顶替：`drain_ms` 是**本次排空实际等了多久**（`daemon-lifecycle` 规格
    // 「关闭时的连接排空」的判据：无其它在途连接时它远小于宽限），`window_ms` 是**本轮最多能等多久**
    // ——它由 `grace_ms` 与 `MIN_DRAIN_MS` 的下限共同决定，正常时总 ≈ 宽限，不代表排空真等了这么久。
    let drain_started = Instant::now();
    let window_ms = deadline
        .saturating_duration_since(drain_started)
        .as_millis();
    drain_connections(&mut connections, deadline).await;
    let drain_ms = drain_started.elapsed().as_millis();
    tracing::info!(
        event = "daemon.drain",
        grace_ms,
        window_ms = u64::try_from(window_ms).unwrap_or(u64::MAX),
        drain_ms = u64::try_from(drain_ms).unwrap_or(u64::MAX),
        "在途连接排空结束"
    );

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
    let stored = composition.close().await.map_err(DaemonError::StoreClose);
    if stored.is_ok() {
        tracing::info!(
            event = "daemon.storage_closed",
            "存储已刷新（wal_checkpoint(TRUNCATE)）并关闭连接池"
        );
    }

    // 5) 收尾：清理 endpoint 残留与本次运行记录，然后释放单实例锁。无条件执行——上一步失败时也必须
    //    走到这里，否则会留下陈旧运行记录（旧实现用 `?` 直接返回，把它整个跳过了）。
    cleanup.finish(
        stored,
        u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    )
}

/// 关闭序列的收尾守卫：清理 endpoint 残留与本次运行记录，**然后**释放单实例锁。
///
/// 为什么是守卫而不是顺序调用：`close()` 里任何一处提前返回（`?`、panic）都会跳过收尾，而收尾被跳过的
/// 后果正是「锁已释放、运行记录还在」（CLI 与 `daemon status` 会把它读成上一次运行的残留）。`Drop` 因此
/// 兜底执行同一条清理路径。`lock` 放在 [`Option`] 里、清理时 `take` 出来并在删除记录之后释放，保证锁
/// 一定最后释放（顺序不可交换）。清理幂等：记录已不存在、endpoint 已消失都不算失败。
struct ShutdownCleanup {
    endpoint_locator: String,
    lock: Option<DaemonLock>,
}

impl ShutdownCleanup {
    fn new(endpoint_locator: String, lock: DaemonLock) -> Self {
        Self {
            endpoint_locator,
            lock: Some(lock),
        }
    }

    /// 清理 endpoint 残留 → 删除运行记录 → 释放单实例锁。
    fn cleanup(&mut self) {
        cleanup_endpoint(&self.endpoint_locator);
        if let Some(lock) = self.lock.take() {
            lock.remove_record();
            // `lock` 在此 drop = 释放单实例锁：必须在运行记录已删除之后。
        }
    }

    /// 显式收尾：清理后释放锁，并把关闭结果**原样**返回（失败不得被收尾吞掉）。
    fn finish(
        mut self,
        outcome: Result<(), DaemonError>,
        elapsed_ms: u64,
    ) -> Result<(), DaemonError> {
        self.cleanup();
        tracing::info!(
            event = "daemon.stopped",
            elapsed_ms,
            failed = outcome.is_err(),
            "关闭序列结束：运行记录已清理，单实例锁已释放"
        );
        outcome
    }
}

impl Drop for ShutdownCleanup {
    /// 提前返回或 panic 的兜底：走与 [`ShutdownCleanup::finish`] 相同的清理路径（幂等）。
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// 排空在途连接：到 `deadline` 仍未结束的连接被中止（响应已无法送达，也不允许在连接之外继续持有语义）。
///
/// 发出该 `daemon.stop` 的那条连接不在这里被等待：它由 `serve_connection` 在写回响应后自行结束
/// （[`ConnectionShutdown`]），因此不占用排空预算——否则排空时长会取决于该客户端进程何时被调度。
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
            connection: Arc::new(ConnectionShutdown::new()),
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

    /// 临时目录（`Drop` 时递归删除）。
    struct TempDir {
        path: std::path::PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let unique = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "acpr-daemon-{label}-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("创建临时目录");
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// 单元测试临时目录名的去重计数。
    static NEXT_TEMP_DIR: AtomicU32 = AtomicU32::new(0);

    /// 取锁 + 发布运行记录（两个收尾用例的共同前置）。
    fn locked_with_record(
        label: &str,
    ) -> (TempDir, std::path::PathBuf, std::path::PathBuf, DaemonLock) {
        let dir = TempDir::new(label);
        let lock_path = crate::lock::lock_path(&dir.path);
        let record_path = crate::lock::record_path(&dir.path);
        let mut lock = DaemonLock::acquire(&lock_path).expect("取锁");
        lock.publish(&LockRecord {
            instance_id: "0123456789abcdef".to_owned(),
            pid: std::process::id(),
            endpoint: None,
        })
        .expect("发布运行记录");
        assert!(
            crate::lock::read_record(&record_path)
                .expect("可读")
                .is_some(),
            "前置：运行记录已发布"
        );
        (dir, lock_path, record_path, lock)
    }

    /// 取消超时不得留下 detached task（`AGENTS.md` §8）：被 detach 的任务会继续运行，并在
    /// `Composition::close` 之前一直持有端口克隆（每个端口都是同一个 `Arc<SqliteStore>` 的克隆），
    /// 让关闭序列随机报 `StoreStillShared`。
    #[tokio::test]
    async fn cancel_all_reaps_a_task_that_ignores_the_cancel_signal() {
        struct MarkOnDrop(Arc<AtomicBool>);
        impl Drop for MarkOnDrop {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }

        let dropped = Arc::new(AtomicBool::new(false));
        let mut tasks = OwnedTasks::new();
        tasks.spawn("never_stops", Arc::new(Notify::new()), {
            let dropped = Arc::clone(&dropped);
            async move {
                let _mark = MarkOnDrop(dropped);
                std::future::pending::<()>().await;
            }
        });

        // 期限已过 = 超时分支：关闭宽限被前面的步骤（在途连接排空）用满时的真实形态。
        tasks.cancel_all(Instant::now()).await;

        assert!(
            dropped.load(Ordering::SeqCst),
            "超时分支必须 abort 并等任务回收，不得留下 detached task"
        );
    }

    /// 关闭步骤失败（例如 `Composition::close` 报 `StoreStillShared`）时，收尾仍必须删除运行记录、
    /// 释放单实例锁，并把失败**原样**返回。
    #[test]
    fn shutdown_cleanup_removes_the_record_and_releases_the_lock_on_failure() {
        let (_dir, lock_path, record_path, lock) = locked_with_record("cleanup-failed");

        let outcome = ShutdownCleanup::new("acpr-no-such-endpoint".to_owned(), lock).finish(
            Err(DaemonError::StoreClose(ComposeError::StoreStillShared)),
            0,
        );

        assert!(
            matches!(
                outcome,
                Err(DaemonError::StoreClose(ComposeError::StoreStillShared))
            ),
            "关闭失败必须原样返回，不能被收尾吞掉"
        );
        assert_eq!(
            crate::lock::read_record(&record_path).expect("可读"),
            None,
            "失败路径也必须删除运行记录"
        );
        assert!(
            matches!(crate::lock::probe(&lock_path), crate::lock::LockState::Free),
            "失败路径也必须释放单实例锁"
        );
    }

    /// 提前返回或 panic 走 `Drop` 兜底时走同一条清理路径（锁同样在记录删除之后才释放）。
    #[test]
    fn shutdown_cleanup_falls_back_to_drop() {
        let (_dir, lock_path, record_path, lock) = locked_with_record("cleanup-drop");

        drop(ShutdownCleanup::new(
            "acpr-no-such-endpoint".to_owned(),
            lock,
        ));

        assert_eq!(
            crate::lock::read_record(&record_path).expect("可读"),
            None,
            "`Drop` 兜底也必须删除运行记录"
        );
        assert!(
            matches!(crate::lock::probe(&lock_path), crate::lock::LockState::Free),
            "`Drop` 兜底也必须释放单实例锁"
        );
    }
}
