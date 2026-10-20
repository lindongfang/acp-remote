//! 连接所有权、重连与撤销：每个 Owner 一个 [`OwnerLink`]，由 supervisor 任务独占。
//!
//! 生命周期（`NODE_LINK_PROTOCOL.md` §15、`MODULE_ARCHITECTURE.md` §4.6）：
//!
//! 1. `NodeLinkClient::open` 创建共享句柄（发件箱 + 观察快照）并 spawn **一个** supervisor 任务；
//! 2. supervisor 反复执行「连接 → 握手 → 运行」，断线按 [`ReconnectPolicy`] 退避；
//! 3. 每次重连后的恢复动作**只**包括：`catalog.subscribe`、每个已登记且未被 Owner 明确拒绝的
//!    imported 会话的 `resource.attach` + `resource.subscribe`（cursor 来自持久化 ACK）、以及不确定
//!    命令的 `command.status` 查询。**绝不**重放 prompt/取消/配置变更等副作用命令；
//! 4. `node.trust.revoked` 或 [`OwnerLink::shutdown`] 立即结束 supervisor：撤销之后不再重连，
//!    共享快照转为离线，后续命令以 [`ClientError::OwnerOffline`] 失败；
//! 5. supervisor 的 `JoinHandle` 由 [`OwnerLink`] 持有，`shutdown` 会 await 它——没有 detached task。
//!
//! 附件连接（[`crate::attachment`]）不另建 supervisor：它在每次上传/读取时经
//! [`OwnerLink::open_attachment_connection`] 建立一条独立认证连接，完成后关闭。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use acp_core::model::NodeId;
use acp_core::ports::RemoteDeliveryStore;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::delivery;
use crate::error::ClientError;
use crate::handshake::{Established, HANDSHAKE_TIMEOUT, HandshakeConfig, OwnerTrust, handshake};
use crate::identity::NodeIdentity;
use crate::link::{ImportedSink, Link, LinkEnd, ResumeSession, SharedLink, channel};
use crate::remote_session::{RemoteSession, RemoteSessionFactory};
use crate::transport::{
    ConnectionTarget, MessageSink, MessageSource, TlsTrust, Transport, TransportFactory,
};

/// 重连退避策略（指数退避，有上限；不是协议可协商项）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconnectPolicy {
    pub initial: Duration,
    pub max: Duration,
}

impl ReconnectPolicy {
    /// 默认：1 秒起、最长 60 秒。
    pub const DEFAULT: Self = Self {
        initial: Duration::from_secs(1),
        max: Duration::from_secs(60),
    };

    fn next(self, current: Duration) -> Duration {
        let doubled = current.saturating_mul(2);
        if doubled > self.max {
            self.max
        } else {
            doubled
        }
    }
}

/// 一个 Owner 的连接配置。
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Owner 的已配对身份（来自本机信任记录）。
    pub owner: OwnerTrust,
    /// 业务端点（`wss://<authority>/node-link/v1`）。
    pub endpoint: String,
    /// TLS 信任方式（生产路径必须显式给出信任锚）。
    pub trust: TlsTrust,
    /// 单条消息上限（默认 1 MiB；服务端可再下调）。
    pub max_message_bytes: usize,
    /// 重连退避。
    pub reconnect: ReconnectPolicy,
    /// 本端支持的 feature（默认 [`crate::handshake::SUPPORTED_FEATURES`]）。
    pub supported_features: Vec<String>,
    /// 本端要求 Owner 必须选中的 feature。
    pub required_features: Vec<String>,
}

impl ClientConfig {
    /// 使用默认 feature 集合与 1 MiB 上限。
    pub fn new(owner: OwnerTrust, endpoint: impl Into<String>, trust: TlsTrust) -> Self {
        Self {
            owner,
            endpoint: endpoint.into(),
            trust,
            max_message_bytes: crate::transport::DEFAULT_MAX_MESSAGE_BYTES,
            reconnect: ReconnectPolicy::DEFAULT,
            supported_features: crate::handshake::SUPPORTED_FEATURES
                .iter()
                .map(|feature| (*feature).to_owned())
                .collect(),
            required_features: crate::handshake::REQUIRED_FEATURES
                .iter()
                .map(|feature| (*feature).to_owned())
                .collect(),
        }
    }
}

/// 一个 Owner 的运行时句柄。
pub struct OwnerLink {
    shared: Arc<SharedLink>,
    /// 与 supervisor 共用的传输（附件连接走同一注入，测试与生产不出现第二条构造路径）。
    transport: Arc<dyn Transport>,
    supervisor: std::sync::Mutex<Option<JoinHandle<()>>>,
    revoked: Arc<AtomicBool>,
    endpoint: String,
    handshake: Arc<HandshakeConfig>,
}

impl std::fmt::Debug for OwnerLink {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OwnerLink")
            .field("owner", self.shared.owner())
            .field("online", &self.shared.is_online())
            .field("revoked", &self.revoked.load(Ordering::SeqCst))
            .finish_non_exhaustive()
    }
}

impl OwnerLink {
    /// Owner node id。
    pub fn owner(&self) -> &NodeId {
        self.shared.owner()
    }

    /// 观察快照（在线状态、协商 feature、限额）。
    pub fn snapshot(&self) -> crate::link::LinkSnapshot {
        self.shared.snapshot()
    }

    /// 是否在线。
    pub fn is_online(&self) -> bool {
        self.shared.is_online()
    }

    /// 是否已被 Owner 撤销（撤销后不再重连）。
    pub fn is_revoked(&self) -> bool {
        self.revoked.load(Ordering::SeqCst)
    }

    /// 本连接在本机使用的节点标识（附件 `subject.id` 必须与它一致）。
    pub fn access_node_id(&self) -> NodeId {
        self.handshake.identity.node_id().clone()
    }

    /// 连接句柄（远程会话与恢复路径共用）。
    pub fn shared(&self) -> &Arc<SharedLink> {
        &self.shared
    }

    /// 打开一个 imported 会话的端点。
    pub fn session(
        &self,
        remote: acp_core::model::RemoteSessionRef,
        deliveries: Arc<dyn RemoteDeliveryStore>,
    ) -> RemoteSession {
        RemoteSession::open(Arc::clone(&self.shared), remote, deliveries)
    }

    /// `SessionBackendFactory` 形状的远程实现。
    pub fn factory(&self, deliveries: Arc<dyn RemoteDeliveryStore>) -> RemoteSessionFactory {
        RemoteSessionFactory::new(Arc::clone(&self.shared), deliveries)
    }

    /// 建立一条独立认证的附件连接（每次调用一条新连接，用完即关）。
    ///
    /// 返回值的最后一项是本链接的关闭信号：调用方（[`crate::attachment::AttachmentForwarder`]）必须在
    /// 读写循环里 select 它，使 `revoke`/`shutdown` 立即中止在途上传与读取，而不是等对端先断开。
    /// 握手与业务连接使用同一个 [`HANDSHAKE_TIMEOUT`]；超时会显式关闭 socket 并返回
    /// [`ClientError::Timeout`]。
    pub async fn open_attachment_connection(
        &self,
    ) -> Result<
        (
            Box<dyn MessageSink>,
            Box<dyn MessageSource>,
            Established,
            watch::Receiver<bool>,
        ),
        ClientError,
    > {
        if !self.shared.is_online() {
            return Err(ClientError::OwnerOffline);
        }
        let target = ConnectionTarget::attachments(&self.endpoint)?;
        let connection = self.transport.connect(&target).await?;
        let (mut sink, mut source) = connection.split();
        let shutdown = self.shared.shutdown_receiver();
        let established = match tokio::time::timeout(
            HANDSHAKE_TIMEOUT,
            handshake(&mut *sink, &mut *source, self.handshake.as_ref()),
        )
        .await
        {
            Ok(Ok(established)) => established,
            Ok(Err(error)) => {
                let _ = sink.close().await;
                return Err(error);
            }
            Err(_) => {
                let _ = sink.close().await;
                return Err(ClientError::Timeout);
            }
        };
        Ok((sink, source, established, shutdown))
    }

    /// 停止连接与重连，并等待 supervisor 结束（幂等）。
    ///
    /// 关闭信号取自 [`SharedLink`]（与 supervisor 同一个 `watch` 通道），因此不再存在「发给无接收者的
    /// sender」这一死路径：supervisor 与全部附件连接在同一时刻看到关闭。等待仍有 5 s 上限作为兜底
    /// （只有异常卡住的 supervisor 才会走到 `abort`）；正常情况下 `shutdown` 立即返回。
    pub async fn shutdown(&self) {
        self.shared.stop();
        let handle = self
            .supervisor
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(mut handle) = handle {
            if tokio::time::timeout(Duration::from_secs(5), &mut handle)
                .await
                .is_err()
            {
                handle.abort();
                let _ = handle.await;
            }
        }
    }
}

/// 出站客户端：按 Owner 建立 [`OwnerLink`]。
pub struct NodeLinkClient {
    factory: Arc<dyn TransportFactory>,
    identity: Arc<dyn NodeIdentity>,
    sink: Arc<dyn ImportedSink>,
    deliveries: Arc<dyn RemoteDeliveryStore>,
}

impl NodeLinkClient {
    /// 装配。生产用 [`crate::transport::WssTransportFactory`]；测试可注入脚本化工厂。
    ///
    /// 传输**按 Owner 的 `ClientConfig`** 构造：`trust` 与 `max_message_bytes` 因此真正生效，
    /// 不会因为进程级共享一个传输而把 `ws://` dev 端点误当 TLS 端点（或反过来）。
    pub fn new(
        factory: Arc<dyn TransportFactory>,
        identity: Arc<dyn NodeIdentity>,
        sink: Arc<dyn ImportedSink>,
        deliveries: Arc<dyn RemoteDeliveryStore>,
    ) -> Self {
        Self {
            factory,
            identity,
            sink,
            deliveries,
        }
    }

    /// 建立（并启动）到一个 Owner 的连接。
    ///
    /// 返回时 supervisor 已 spawn、但**不一定**已认证；调用方可以用
    /// [`OwnerLink::is_online`] 或等待 `snapshot().online`。
    pub fn open(&self, config: ClientConfig) -> Result<Arc<OwnerLink>, ClientError> {
        // 端点形状在启动前校验：配置错误不进入重连循环。
        let _ = ConnectionTarget::business(&config.endpoint)?;
        // 每个 Owner 的信任方式与单消息上限来自它自己的配置；工厂失败（例如空 `Roots`）在
        // spawn 之前拒绝，避免把配置错误伪装成无限重连。
        let transport = self
            .factory
            .build(&config.trust, config.max_message_bytes)?;
        let transport_for_supervisor = Arc::clone(&transport);
        let handshake = Arc::new(HandshakeConfig {
            identity: Arc::clone(&self.identity),
            owner: config.owner.clone(),
            supported_features: config.supported_features.clone(),
            required_features: config.required_features.clone(),
        });
        let (shared, outgoing, shutdown_rx) = channel(config.owner.node_id.clone());
        let revoked = Arc::new(AtomicBool::new(false));
        let owner = config.owner.node_id.clone();
        let endpoint = config.endpoint.clone();
        let sink = Arc::clone(&self.sink);
        let deliveries = Arc::clone(&self.deliveries);
        let identity = Arc::clone(&self.identity);
        let handshake_for_supervisor = Arc::clone(&handshake);
        let heartbeat = crate::link::DEFAULT_HEARTBEAT;
        let silence = crate::link::DEFAULT_SILENCE_TIMEOUT;
        let reconnect = config.reconnect;
        let revoked_for_task = Arc::clone(&revoked);
        let shared_for_task = Arc::clone(&shared);

        let supervisor = tokio::spawn(async move {
            supervise(
                owner,
                endpoint,
                transport_for_supervisor,
                Arc::new(HandshakeConfig {
                    identity,
                    owner: handshake_for_supervisor.owner.clone(),
                    supported_features: handshake_for_supervisor.supported_features.clone(),
                    required_features: handshake_for_supervisor.required_features.clone(),
                }),
                sink,
                deliveries,
                shared_for_task,
                outgoing,
                shutdown_rx,
                revoked_for_task,
                heartbeat,
                silence,
                reconnect,
            )
            .await;
        });

        Ok(Arc::new(OwnerLink {
            shared,
            transport,
            supervisor: std::sync::Mutex::new(Some(supervisor)),
            revoked,
            endpoint: config.endpoint,
            handshake,
        }))
    }
}

#[allow(clippy::too_many_arguments)] // supervisor 的每个参数都是一个独立的所有权事实
async fn supervise(
    owner: NodeId,
    endpoint: String,
    transport: Arc<dyn Transport>,
    handshake: Arc<HandshakeConfig>,
    sink: Arc<dyn ImportedSink>,
    deliveries: Arc<dyn RemoteDeliveryStore>,
    shared: Arc<SharedLink>,
    mut outgoing: tokio::sync::mpsc::Receiver<crate::link::Outbound>,
    mut shutdown: watch::Receiver<bool>,
    revoked: Arc<AtomicBool>,
    heartbeat: Duration,
    silence: Duration,
    reconnect: ReconnectPolicy,
) {
    let mut backoff = reconnect.initial;
    loop {
        if *shutdown.borrow() {
            return;
        }
        let target = match ConnectionTarget::business(&endpoint) {
            Ok(target) => target,
            Err(error) => {
                tracing::warn!(
                    event = "node_link_client.endpoint_invalid",
                    owner = %owner,
                    error = %error,
                    "the configured owner endpoint is not usable"
                );
                return;
            }
        };
        let connection = match connect_or_shutdown(transport.as_ref(), &target, &mut shutdown).await
        {
            Some(connection) => connection,
            None => return,
        };
        let connection = match connection {
            Ok(connection) => connection,
            Err(error) => {
                tracing::debug!(
                    event = "node_link_client.connect_failed",
                    owner = %owner,
                    error = %error,
                    retry_in_ms = backoff.as_millis(),
                    "the owner is not reachable; retrying"
                );
                if wait_backoff(&mut shutdown, backoff).await {
                    return;
                }
                backoff = reconnect.next(backoff);
                continue;
            }
        };
        let (sink_half, source_half) = connection.split();
        // 恢复计划在建链前从持久化索引取：只看非正文事实。
        let startup = match delivery::subscriptions(deliveries.as_ref(), &owner).await {
            Ok(rows) => {
                let mut plan = Vec::new();
                for (remote, cursor) in rows {
                    match crate::remote_session::wire_remote(&remote) {
                        Ok(remote) => {
                            // 已被 Owner 以不可重试错误拒绝的订阅不再进入计划：否则每次重连都会
                            // 重复 attach 同一个不可能成功的会话（重连循环）。
                            if shared.subscription_rejected(&remote) {
                                tracing::debug!(
                                    event = "node_link_client.subscription_skipped",
                                    owner = %owner,
                                    session_id = remote.session_id.as_str(),
                                    "the owner rejected this subscription permanently; it is skipped"
                                );
                                continue;
                            }
                            plan.push(ResumeSession { remote, cursor });
                        }
                        Err(error) => tracing::warn!(
                            event = "node_link_client.subscription_plan_invalid",
                            owner = %owner,
                            error = %error,
                            "an imported session reference cannot be encoded; it is skipped"
                        ),
                    }
                }
                plan
            }
            Err(error) => {
                tracing::warn!(
                    event = "node_link_client.subscription_plan_failed",
                    owner = %owner,
                    error = ?error,
                    "the imported session index cannot be read; this connection resumes without subscriptions"
                );
                Vec::new()
            }
        };
        let link = Link::new(
            Arc::clone(&shared),
            HandshakeConfig {
                identity: Arc::clone(&handshake.identity),
                owner: handshake.owner.clone(),
                supported_features: handshake.supported_features.clone(),
                required_features: handshake.required_features.clone(),
            },
            Arc::clone(&sink),
            heartbeat,
            silence,
        );
        let end = link
            .run(
                sink_half,
                source_half,
                &mut outgoing,
                &mut shutdown,
                startup,
            )
            .await;
        match end {
            LinkEnd::Shutdown => return,
            LinkEnd::Revoked => {
                revoked.store(true, Ordering::SeqCst);
                // 撤销同样中止该 Owner 的在途附件连接（它们 select 同一个关闭通道）。
                shared.stop();
                tracing::info!(
                    event = "node_link_client.revoked",
                    owner = %owner,
                    "the owner revoked this access node; reconnect stopped"
                );
                return;
            }
            LinkEnd::Closed => {
                tracing::debug!(
                    event = "node_link_client.disconnected",
                    owner = %owner,
                    retry_in_ms = backoff.as_millis(),
                    "the node-link connection closed; reconnecting"
                );
            }
            LinkEnd::Failed(error) => {
                // `link.error(auth.node_revoked)` 由 `link_error` 收敛成 `Revoked`：它与
                // `node.trust.revoked` 是同一事实，必须停止重连，而不是按普通断线退避。
                if matches!(error, ClientError::Revoked) {
                    revoked.store(true, Ordering::SeqCst);
                    shared.stop();
                    tracing::info!(
                        event = "node_link_client.revoked",
                        owner = %owner,
                        "the owner revoked this access node via link.error; reconnect stopped"
                    );
                    return;
                }
                tracing::warn!(
                    event = "node_link_client.connection_failed",
                    owner = %owner,
                    error = %error,
                    retry_in_ms = backoff.as_millis(),
                    "the node-link connection failed; reconnecting"
                );
            }
        }
        if *shutdown.borrow() {
            return;
        }
        if wait_backoff(&mut shutdown, backoff).await {
            return;
        }
        backoff = reconnect.next(backoff);
    }
}

/// `transport.connect` 与关闭信号竞争：关闭时**立即**返回 `None`，而不是等 TLS 握手超时。
///
/// 这是「关闭顺序无 detached task」的前半：没有它，supervisor 可能卡在一次对不可达 Owner 的握手里，
/// 关闭方只能等 5 s 上限后 abort（后半在 [`OwnerLink::shutdown`]）。
async fn connect_or_shutdown(
    transport: &dyn crate::transport::Transport,
    target: &ConnectionTarget,
    shutdown: &mut watch::Receiver<bool>,
) -> Option<Result<Box<dyn crate::transport::Connection>, ClientError>> {
    tokio::select! {
        biased;
        changed = shutdown.changed() => {
            let _ = changed;
            let _ = *shutdown.borrow();
            None
        }
        result = transport.connect(target) => Some(result),
    }
}

/// 等待退避；返回 `true` 表示收到关闭信号。
async fn wait_backoff(shutdown: &mut watch::Receiver<bool>, backoff: Duration) -> bool {
    tokio::select! {
        changed = shutdown.changed() => {
            let _ = changed;
            true
        }
        () = tokio::time::sleep(backoff) => false,
    }
}

impl Drop for OwnerLink {
    fn drop(&mut self) {
        // 兜底：显式 shutdown 是正常路径；drop 只发信号，不阻塞（不能等待 runtime 之外的 join）。
        self.shared.stop();
    }
}
