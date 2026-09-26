//! 一条 WSS 连接的会话驱动：握手状态机、信封/序号、心跳与每连接发送队列
//! （`docs/NODE_LINK_PROTOCOL.md` §2/§12.2，`design.md` D2/D3/D9）。
//!
//! 一条连接 = 一个任务 = 一个 [`Session`]：握手与业务阶段都在同一任务里推进（不 spawn 子任务），
//! 因此取消路径只有一条——`peer.shutdown`、对端关闭、致命协议错误或关闭请求，任一发生即结束整个会话。
//!
//! 阶段划分（`node.hello → node.challenge → node.proof → node.ready`）：
//!
//! 1. **准入**：认证前先过两道闸——单 IP 认证尝试 10 次/分钟（§2.5，键是接入层给出的真实
//!    `client_ip`）与单 IP 在途握手配额（半开连接不占满接入层的 TLS 握手池）；配额凭证只覆盖握手
//!    阶段，认证完成即释放（已认证的连接不再占它）；
//! 2. **握手**：只允许 `node.hello`/`node.proof`（+ 双向的 `link.error`），其余消息以 4401 关闭；
//!    整个握手 15 秒内必须完成，超时 4408；
//! 3. **认证收尾**：`Authority::complete_auth` 的写集经 `UseCases::consume_pairing`（已批准配对 →
//!    `consumed` + 审计 + 对端节点行的 `last_connected_at`）或 `UseCases::record_node_connected`
//!    （没有待消费配对时的重复认证：`last_connected_at` + 审计）在**同一事务**提交成功后才发
//!    `node.ready`（`design.md` D3；`CORE_PORTS_AND_STORAGE.md` §11.6 第 8/9 条）；
//! 4. **业务**：认证后的消息逐条判定阶段、信封、序号与 type，再交给 [`MessageRoute`]；
//!    连接的心跳、静默超时与慢消费者看门狗在同一 `select!` 里推进。
//!
//! **认证限流的接线口径**：接入层（`transport::net`）在 WS upgrade 之前没有每路径的准入钩子
//! （WP2 冻结形状只提供 `WsHandler`），因此本层把闸门放在**会话的第一步**：任何帧（包括
//! `node.hello`）都不再被处理，连接以 4429 关闭。效果等价——未认证的连接无法产生任何业务副作用。
//!
//! **catalogRevision 的口径**：`node.challenge.catalogRevision` 与 `node.ready.catalogRevision` 同源，
//! 都取本机全局水位 `store.head()` 的 `globalSequence`（跨重启单调的十进制串）。握手两侧必须同源：
//! 该值进入 `node-link-challenge/v1`/`node-link-proof/v1` 的 tag 6，Access 用 `node.challenge` 给的值
//! 签 proof，Owner 用签发挑战时的同一个值验签。catalog 投影本体在 WP5，届时
//! `catalog.snapshot.revision` 必须与本值同源；这是本轮登记在案的临时口径（见 WP4 handoff），
//! 不是最终语义。

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use acp_core::model::{AuditAction, AuditOutcome, NodeId, PeerIdentity};
use acp_core::use_cases::{NodeLinkHandshakeView, UseCases};
use identity_auth::{
    Authority, ChallengeId, ChallengeIssue, ChallengeRequest, ConnectionKind, CredentialStatus,
    Nonce, P1363Signature, ProofSubmission,
};
use node_link_protocol::common::{Base64Url, DecimalString, RawObject, Uuid};
use node_link_protocol::envelope::{Envelope, MessageType, Phase, encode_body};
use node_link_protocol::error::{ErrorCode, Ping, Pong};
use node_link_protocol::handshake::{NodeChallenge, NodeHello, NodeProof, NodeReady};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::sync::mpsc;
use tokio::time::Instant as TokioInstant;
use tracing::{debug, info, warn};

use crate::node_link::conn::handshake::{self, NegotiationFault};
use crate::node_link::conn::limits::{NodeLinkConfig, SessionLimits};
use crate::node_link::conn::registry::{ConnectionHandle, ConnectionRegistry, Outbound, SendFault};
use crate::node_link::conn::wire::{self, Inbound};
use crate::node_link::conn::{
    AUTH_ATTEMPTS_PER_MINUTE, AUTH_WINDOW, HANDSHAKE_TIMEOUT, MAX_TRACKED_IPS, close,
};
use crate::transport::net::{
    PeerInfo, RateLimit, SlidingWindowLimiter, WsConnection, WsError, WsHandler, WsMessage,
};

/// 心跳静默上限（§2.5：连续 90 秒没有入站消息即关闭；固定常量、不可配置）。
pub const HEARTBEAT_SILENCE_TIMEOUT: Duration = Duration::from_secs(90);

/// 单 IP 允许的在途握手数（半开连接上限；见 [`HandshakeSlots`]）。
pub const MAX_IN_FLIGHT_HANDSHAKES_PER_IP: usize = 4;

/// 会话轮询间隔：驱动心跳、静默超时与慢消费者看门狗（不引入额外的计时任务）。
const SESSION_TICK: Duration = Duration::from_millis(500);

/// 慢消费者看门狗：队列持续处于高水位超过此时长即断开（§2.5：先停读快照批次，持续过慢时断开）。
const SLOW_CONSUMER_GRACE: Duration = Duration::from_secs(30);

/// 会话看门狗的两个窗口（§2.5 的固定常量：静默 90 秒、慢消费者宽限 30 秒）。
///
/// 生产路径只有 [`SessionWindows::from_protocol`] 一个来源（`node.ready.limits` 里没有这两个值，
/// 它们不可协商也不可配置）；`#[cfg(test)]` 的 [`NodeLinkConn::with_test_windows`] 让用例把窗口调短：
/// 90 秒/30 秒的真实等待在用例里不可接受，而这两个值本身由常量断言锁定。
#[derive(Clone, Copy, Debug)]
struct SessionWindows {
    silence: Duration,
    slow_consumer: Duration,
}

impl SessionWindows {
    /// 协议约定的窗口（唯一的生成入口）。
    fn from_protocol() -> Self {
        Self {
            silence: HEARTBEAT_SILENCE_TIMEOUT,
            slow_consumer: SLOW_CONSUMER_GRACE,
        }
    }
}

/// 关闭时排空已入队消息的预算（撤销推送必须先送达再发 close 帧）。
const CLOSE_DRAIN_BUDGET: Duration = Duration::from_secs(2);

/// 认证后消息的分派口（WP5 的 catalog/resource、WP6 的 command 各自实现，组合根组合它们）。
///
/// `conn` 只负责阶段、信封、序号、type 与限流判定；被路由看到的消息**已经**通过这些校验。
/// 本切片（WP4）不注册任何路由：v1 的业务 type 因此会被显式拒绝为
/// `nodelink.protocol.type_unsupported`（对端可见的显式拒绝，不是静默丢弃；WP5/WP6 注册后该分支不再命中）。
#[async_trait::async_trait]
pub trait MessageRoute: Send + Sync + 'static {
    /// 处理一条认证后消息。返回 [`RouteOutcome::Claimed`] 表示本路由认领该 type。
    async fn route(&self, session: &ConnectionHandle, message: &Envelope) -> RouteOutcome;
}

/// 路由的处理结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteOutcome {
    /// 已处理（响应经会话句柄入队）。
    Claimed,
    /// 不属于本路由；没有路由认领时 `conn` 回 `type_unsupported`。
    Unclaimed,
}

/// Node Link WSS 端点的装配件：组合根持有一个 `Arc<NodeLinkConn>`。
pub struct NodeLinkConn {
    core: Arc<UseCases>,
    authority: Arc<Authority>,
    config: NodeLinkConfig,
    registry: Arc<ConnectionRegistry>,
    route: Option<Arc<dyn MessageRoute>>,
    /// 单 IP 的新认证尝试限流（§2.5：10 次/分钟，键是 `PeerInfo::client_ip`）。
    auth_limit: SlidingWindowLimiter,
    /// 单 IP 的在途握手配额。
    handshakes: HandshakeSlots,
    /// 会话看门狗的窗口（§2.5 的固定常量）。
    windows: SessionWindows,
}

impl std::fmt::Debug for NodeLinkConn {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NodeLinkConn")
            .field("public_origin", &self.config.public_origin)
            .field("routed", &self.route.is_some())
            .finish_non_exhaustive()
    }
}

impl NodeLinkConn {
    /// 装配。`core` 是用例面、`authority` 是身份状态机、`config` 是配置快照（含 `public_origin`）、
    /// `registry` 是连接注册表（组合根与 WP5/WP6 共享同一个句柄）。
    pub fn new(
        core: Arc<UseCases>,
        authority: Arc<Authority>,
        config: NodeLinkConfig,
        registry: Arc<ConnectionRegistry>,
    ) -> Self {
        Self {
            core,
            authority,
            config,
            registry,
            route: None,
            auth_limit: SlidingWindowLimiter::new(AUTH_ATTEMPTS_PER_MINUTE, AUTH_WINDOW),
            handshakes: HandshakeSlots::new(MAX_IN_FLIGHT_HANDSHAKES_PER_IP),
            windows: SessionWindows::from_protocol(),
        }
    }

    /// 测试专用：把会话看门狗的窗口换成本次用例的值（§2.5 的两个固定常量在生产路径上只有
    /// [`SessionWindows::from_protocol`] 一个来源；90 秒/30 秒的真实等待在用例里不可接受）。
    #[cfg(test)]
    pub(crate) fn with_test_windows(mut self, silence: Duration, slow_consumer: Duration) -> Self {
        self.windows = SessionWindows {
            silence,
            slow_consumer,
        };
        self
    }

    /// 注册认证后消息的路由（WP5/WP6 的组合根接线）。
    pub fn with_route(mut self, route: Arc<dyn MessageRoute>) -> Self {
        self.route = Some(route);
        self
    }

    /// 测试专用：当前被占用的在途握手配额数（源地址无关；用例需要确定性地知道「配额已满/已释放」，
    /// 否则只能靠轮询等待，而配额本身是固定常量、不可从外部观测）。
    #[cfg(test)]
    pub(crate) fn in_flight_handshakes(&self) -> usize {
        self.handshakes.in_flight()
    }

    /// 连接注册表句柄（WP5/WP6 与撤销传播共用）。
    pub fn registry(&self) -> &Arc<ConnectionRegistry> {
        &self.registry
    }

    /// 每帧唯一的消息标识（§2.2 的 `messageId`；只用于诊断，不代替 `requestId`）。
    fn message_id(&self) -> Option<Uuid> {
        Uuid::parse(self.core.ids().message_id().as_str()).ok()
    }
}

/// `WsHandler` 的实现：一条连接一个会话（组合根注册到 [`crate::node_link::conn::WS_PATH`]）。
pub struct WsEndpoint {
    conn: Arc<NodeLinkConn>,
}

impl WsEndpoint {
    /// 连接处理器（`NetListener::register_ws` 的入参）。
    pub fn handler(conn: Arc<NodeLinkConn>) -> Arc<dyn WsHandler> {
        Arc::new(Self { conn })
    }
}

#[async_trait::async_trait]
impl WsHandler for WsEndpoint {
    async fn handle(self: Arc<Self>, connection: WsConnection, peer: PeerInfo) {
        Session::new(Arc::clone(&self.conn), connection, peer)
            .run()
            .await;
    }
}

/// 握手阶段的连接状态。
#[derive(Default)]
struct HandshakeState {
    /// 已签发的挑战（`server_nonce` 与 `client_nonce` 的回显校验由状态机承担）。
    challenge: Option<ChallengeIssue>,
    client_nonce: Option<Nonce>,
    view: Option<NodeLinkHandshakeView>,
    node: Option<NodeId>,
}

impl HandshakeState {
    /// 已经处理过 hello（握手进入「等 proof」阶段）。
    fn expecting_proof(&self) -> bool {
        self.view.is_some()
    }
}

/// 一条连接的会话。
struct Session {
    conn: Arc<NodeLinkConn>,
    /// 连接本体：`WsConnection::close` 消费所有权，因此会话持有 `Option`，关闭即 `take`（绝不二次关闭）。
    connection: Option<WsConnection>,
    peer: PeerInfo,
    limits: SessionLimits,
    handshake: HandshakeState,
    /// 本方向下一个应到的序号（认证后从 `1` 开始）。
    inbound_next: u64,
    /// 最近一次入站帧的时刻（静默超时判据）。
    last_inbound: Instant,
}

/// 握手的推进结果。
enum Flow {
    /// 继续读下一帧。
    Continue,
    /// 认证完成：进入业务阶段（会话句柄与出站队列）。
    Ready {
        handle: Arc<ConnectionHandle>,
        receiver: mpsc::Receiver<Outbound>,
    },
    /// 会话结束（已按 close code 关闭或对端已断开）。
    Finished,
}

impl Session {
    fn new(conn: Arc<NodeLinkConn>, connection: WsConnection, peer: PeerInfo) -> Self {
        let limits = SessionLimits::negotiate(&conn.config);
        Self {
            conn,
            connection: Some(connection),
            peer,
            limits,
            handshake: HandshakeState::default(),
            inbound_next: 1,
            last_inbound: Instant::now(),
        }
    }

    async fn run(mut self) {
        let client_ip = self.peer.client_ip;
        // ① 认证尝试限流（§2.5：10 次/分钟/IP，键是接入层给出的真实对端地址）。
        if let RateLimit::Denied { retry_after } = self.conn.auth_limit.check(client_ip) {
            warn!(
                event = "node_link.auth_rate_limited",
                client_ip = %client_ip,
                retry_after_ms = retry_after.as_millis() as u64,
                "node-link authentication attempts were rate limited"
            );
            self.refuse(
                close::RATE_LIMITED,
                ErrorCode::ResourceRateLimited,
                "too many authentication attempts",
                wire::rate_limit_details(retry_after),
            )
            .await;
            return;
        }
        // ② 在途握手配额（RV2-WP2FIX-F2 的取舍结论：便宜就做）。
        //
        // 凭证只覆盖**握手阶段**：认证完成（或握手结束）立即释放，因此本配额限制的是「同一源地址上
        // 尚未完成认证的连接数」，而不是「同源连接数」——已经认证的连接不再占配额（§2.5 与
        // [`MAX_IN_FLIGHT_HANDSHAKES_PER_IP`] 的取向都只谈半开握手）。
        let handshake_slot = match self.conn.handshakes.acquire(client_ip) {
            Some(slot) => slot,
            None => {
                warn!(
                    event = "node_link.handshake_slots_exhausted",
                    client_ip = %client_ip,
                    max = MAX_IN_FLIGHT_HANDSHAKES_PER_IP,
                    "too many handshakes in flight for this client address"
                );
                self.refuse(
                    close::RATE_LIMITED,
                    ErrorCode::ResourceRateLimited,
                    "too many handshakes in flight for this client address",
                    RawObject::empty(),
                )
                .await;
                return;
            }
        };
        let flow = self.handshake_phase().await;
        // 握手已结束（认证成功或连接关闭）：配额在这里释放，业务阶段不再持有它。
        drop(handshake_slot);

        match flow {
            Flow::Ready { handle, receiver } => self.business_phase(handle, receiver).await,
            Flow::Continue | Flow::Finished => {}
        }
    }

    // -----------------------------------------------------------------------------------------
    // 握手阶段
    // -----------------------------------------------------------------------------------------

    async fn handshake_phase(&mut self) -> Flow {
        let deadline = TokioInstant::now() + HANDSHAKE_TIMEOUT;
        loop {
            let shutdown = self.peer.shutdown.clone();
            let Some(connection) = self.connection.as_mut() else {
                return Flow::Finished;
            };
            tokio::select! {
                biased;
                () = shutdown.wait() => {
                    debug!(event = "node_link.shutdown", "the session ends because the daemon is shutting down");
                    return Flow::Finished;
                }
                () = tokio::time::sleep_until(deadline) => {
                    warn!(event = "node_link.handshake_timeout", "the handshake did not complete within 15 seconds");
                    self.audit_failure(AuditOutcome::Failed).await;
                    self.close(close::TIMEOUT, "handshake timeout").await;
                    return Flow::Finished;
                }
                frame = connection.recv() => {
                    match frame {
                        None => return Flow::Finished,
                        Some(Err(WsError::MessageTooLarge)) => return Flow::Finished,
                        Some(Err(WsError::Transport(detail))) => {
                            debug!(event = "node_link.transport_closed", detail = %detail, "the connection ended");
                            return Flow::Finished;
                        }
                        Some(Ok(WsMessage::Binary(_))) => {
                            // §2.1：只接受 text frame；binary 帧回 link.error（invalid_json）并以 4400 关闭。
                            warn!(event = "node_link.binary_frame", "a binary frame is not allowed on node-link");
                            self.refuse(
                                close::PROTOCOL,
                                ErrorCode::ProtocolInvalidJson,
                                "binary frames are not allowed on this endpoint",
                                RawObject::empty(),
                            ).await;
                            return Flow::Finished;
                        }
                        Some(Ok(WsMessage::Text(text))) => {
                            self.last_inbound = Instant::now();
                            match self.on_pre_auth(&text).await {
                                Flow::Continue => {}
                                other => return other,
                            }
                        }
                    }
                }
            }
        }
    }

    /// 处理一条认证前的消息。
    async fn on_pre_auth(&mut self, text: &str) -> Flow {
        let (judged, _) = wire::judge(text, Phase::PreAuth, None, None);
        let envelope = match judged {
            Inbound::Message(envelope) => envelope,
            Inbound::Rejected { code, message } => {
                self.reject(code, message).await;
                return Flow::Continue;
            }
            Inbound::Fatal {
                code,
                message,
                close_code,
            } => {
                self.refuse(close_code, code, message, RawObject::empty())
                    .await;
                return Flow::Finished;
            }
        };
        if !wire::pre_auth_allowed(envelope.message_type(), self.handshake.expecting_proof()) {
            // §2.1/§12.2：认证完成前的业务消息一律 4401 关闭。
            self.refuse(
                close::UNAUTHENTICATED,
                ErrorCode::ProtocolTypeUnsupported,
                "only node.hello and node.proof are allowed before authentication",
                RawObject::empty(),
            )
            .await;
            return Flow::Finished;
        }
        match envelope.message_type() {
            MessageType::NodeHello => self.on_hello(&envelope).await,
            MessageType::NodeProof => self.on_proof(&envelope).await,
            // 对端报告了一条连接级错误：记录后继续（`link.error` 不改变本机状态）。
            _ => {
                debug!(
                    event = "node_link.peer_error",
                    message_id = envelope.message_id().as_str(),
                    "the access node reported a link error"
                );
                Flow::Continue
            }
        }
    }

    /// `node.hello`：版本交集 → feature 协商 → 组装 `PeerTrust` → 签发挑战 → `node.challenge`。
    async fn on_hello(&mut self, envelope: &Envelope) -> Flow {
        if self.handshake.expecting_proof() {
            // 由 `pre_auth_allowed` 保证不可达：进入「等 proof」之后 `node.hello` 不再在认证前白名单
            // 里，`on_pre_auth` 会以 4401 关闭连接，根本到不了这里。保留这条分支只为把状态机写全——
            // 重复 hello 若真的进来就会重签一次挑战，让已签发的那个悬空（所以这里是失败关闭而不是忽略）。
            self.refuse(
                close::UNAUTHENTICATED,
                ErrorCode::ProtocolTypeUnsupported,
                "node.hello must be the first message of the connection",
                RawObject::empty(),
            )
            .await;
            return Flow::Finished;
        }
        let Some(hello) = decode_body::<NodeHello>(envelope) else {
            self.reject(ErrorCode::ProtocolSchemaInvalid, wire::SCHEMA_MESSAGE)
                .await;
            return Flow::Continue;
        };
        if let Err(fault) = handshake::negotiated_version(&hello) {
            let (code, message) = negotiation_error(&fault);
            self.refuse_with_details(
                close::VERSION,
                code,
                message,
                RawObject::empty(),
                Some(envelope.message_id().clone()),
            )
            .await;
            return Flow::Finished;
        }
        let selected = match handshake::negotiate_features(&hello) {
            Ok(selected) => selected,
            Err(fault) => {
                let (code, message) = negotiation_error(&fault);
                let missing = match &fault {
                    NegotiationFault::FeaturesMissing(missing) => missing.clone(),
                    NegotiationFault::VersionUnsupported => Vec::new(),
                };
                warn!(
                    event = "node_link.feature_negotiation_failed",
                    missing = ?missing,
                    "the access node requires features this owner does not support"
                );
                self.refuse_with_details(
                    close::PROTOCOL,
                    code,
                    message,
                    wire::feature_details(&missing),
                    Some(envelope.message_id().clone()),
                )
                .await;
                return Flow::Finished;
            }
        };

        let Ok(access_node) = NodeId::new(hello.access_node_id.as_str()) else {
            self.reject(ErrorCode::ProtocolSchemaInvalid, wire::SCHEMA_MESSAGE)
                .await;
            return Flow::Continue;
        };
        // 每次握手都重新读一次持久事实（design D3：验签公钥只来自持久化信任的当次快照）。
        let view = match self.conn.core.node_link_handshake_view(&access_node).await {
            Ok(view) => view,
            Err(error) => {
                warn!(
                    event = "node_link.handshake_view_failed",
                    error = ?error,
                    "the node-link handshake could not read the persisted trust snapshot"
                );
                return self.internal_fault(envelope).await;
            }
        };
        let trust = handshake::peer_trust(&view, &access_node);
        let Some(endpoint) =
            handshake::node_endpoint(&view, self.conn.config.public_origin.as_deref())
        else {
            warn!(
                event = "node_link.endpoint_unavailable",
                "no node-link endpoint is configured and no pairing binding exists"
            );
            return self.internal_fault(envelope).await;
        };
        let Ok(client_nonce) = Nonce::new(&base64url_text(&hello.client_nonce)) else {
            self.reject(ErrorCode::ProtocolSchemaInvalid, wire::SCHEMA_MESSAGE)
                .await;
            return Flow::Continue;
        };
        let catalog_revision = view.head.global_sequence.get();
        let request = ChallengeRequest {
            kind: ConnectionKind::NodeLink,
            peer: PeerIdentity::Node(access_node.clone()),
            binding: handshake::connection_binding(&endpoint),
            client_nonce: client_nonce.clone(),
            negotiated_features: handshake::transcript_features(&selected),
            catalog_revision: Some(catalog_revision),
        };
        let issue = match self.conn.authority.hello(&request, &trust).await {
            Ok(issue) => issue,
            Err(error) => {
                // 绑定/信任类别/keystore 三类失败对端一律看到同一个 proof_invalid（不泄露差异）；
                // 细节只进结构化日志。
                warn!(
                    event = "node_link.challenge_failed",
                    error = %error,
                    "the node-link challenge could not be issued"
                );
                self.refuse_with_details(
                    close::UNAUTHENTICATED,
                    ErrorCode::AuthProofInvalid,
                    "the handshake could not be served",
                    RawObject::empty(),
                    Some(envelope.message_id().clone()),
                )
                .await;
                return Flow::Finished;
            }
        };
        let Some(body) = challenge_body(&issue, &self.conn.authority, &selected, catalog_revision)
        else {
            return self.internal_fault(envelope).await;
        };
        if self
            .write_pre_auth(MessageType::NodeChallenge, &body)
            .await
            .is_err()
        {
            debug!(
                event = "node_link.challenge_not_sent",
                "the connection ended before the challenge was sent"
            );
            return Flow::Finished;
        }
        info!(
            event = "node_link.challenge_issued",
            access_node_id = access_node.as_str(),
            known = trust.public_key.is_some(),
            catalog_revision,
            "the node-link handshake issued a challenge"
        );
        self.handshake.challenge = Some(issue);
        self.handshake.client_nonce = Some(client_nonce);
        self.handshake.view = Some(view);
        self.handshake.node = Some(access_node);
        Flow::Continue
    }

    /// `node.proof`：验签 → 凭据状态 → 写集提交 → 注册连接 → `node.ready`。
    async fn on_proof(&mut self, envelope: &Envelope) -> Flow {
        let (Some(challenge), Some(access_node), Some(view), Some(client_nonce)) = (
            self.handshake.challenge.clone(),
            self.handshake.node.clone(),
            self.handshake.view.clone(),
            self.handshake.client_nonce.clone(),
        ) else {
            // 由 `pre_auth_allowed` 保证不可达：不在「等 proof」阶段时 `node.proof` 不在认证前白名单里，
            // `on_pre_auth` 会以 4401 关闭连接；挑战、快照与 client nonce 因此必然齐备。保留这条分支
            // 只为不 panic（也能挡住未来把状态机改回去时的悬空 proof）。
            self.refuse_with_details(
                close::UNAUTHENTICATED,
                ErrorCode::AuthProofInvalid,
                "the proof does not match an issued challenge",
                RawObject::empty(),
                Some(envelope.message_id().clone()),
            )
            .await;
            return Flow::Finished;
        };
        let Some(proof) = decode_body::<NodeProof>(envelope) else {
            self.reject(ErrorCode::ProtocolSchemaInvalid, wire::SCHEMA_MESSAGE)
                .await;
            return Flow::Continue;
        };
        let Some(submission) = ProofInput {
            access_node: &access_node,
            server_nonce: &challenge.server_nonce,
            client_nonce: &client_nonce,
            proof: &proof,
        }
        .to_submission() else {
            self.refuse_with_details(
                close::UNAUTHENTICATED,
                ErrorCode::AuthProofInvalid,
                "the proof is not valid",
                RawObject::empty(),
                Some(envelope.message_id().clone()),
            )
            .await;
            return Flow::Finished;
        };
        let trust = handshake::peer_trust(&view, &access_node);
        let authenticated = match self.conn.authority.verify_proof(&submission, &trust) {
            Ok(authenticated) => authenticated,
            Err(failure) => {
                // §14.2：握手失败必须留痕（Node Link 侧的动作是 `node.auth_failed`）。
                // 对端可见的映射只用**持久化快照**判定：快照里没有可用公钥（从未配对、或信任行已被清）
                // 就是 `node_unknown`（R43），其余证明类失败一律 `proof_invalid`（R41），不泄露失败细节；
                // 细节只进结构化日志。
                let unknown_node = trust.public_key.is_none();
                let (code, outcome) = if unknown_node {
                    (ErrorCode::AuthNodeUnknown, AuditOutcome::Denied)
                } else {
                    (ErrorCode::AuthProofInvalid, AuditOutcome::Failed)
                };
                warn!(
                    event = "node_link.proof_invalid",
                    access_node_id = access_node.as_str(),
                    code = code.as_str(),
                    reason = ?failure.reason,
                    "the node-link proof was rejected"
                );
                self.audit(&access_node, AuditAction::NodeAuthFailed, outcome)
                    .await;
                self.refuse_with_details(
                    close::UNAUTHENTICATED,
                    code,
                    "the proof is not valid",
                    RawObject::empty(),
                    Some(envelope.message_id().clone()),
                )
                .await;
                return Flow::Finished;
            }
        };

        // §5.1：`Revoked`/`Unknown` 不是握手失败，而是凭据状态 → adapter 映射为对应错误与 close code。
        let credential = authenticated.credential;
        let refuse_credential: Option<(u16, ErrorCode)> = match credential {
            CredentialStatus::Active => None,
            CredentialStatus::Revoked => Some((close::REVOKED, ErrorCode::AuthNodeRevoked)),
            CredentialStatus::Unknown => Some((close::UNAUTHENTICATED, ErrorCode::AuthNodeUnknown)),
            // 节点 grant 由持久记录逐消息复核，不需要「重连刷新」；状态机若从其它来源返回它，
            // 仍按失败关闭处理（v1 词表里没有更贴切的状态码），并留下结构化日志。
            CredentialStatus::ScopeReduced => {
                Some((close::STATE_CONFLICT, ErrorCode::AuthNodeUnknown))
            }
        };
        if let Some((close_code, code)) = refuse_credential {
            warn!(
                event = "node_link.credential_rejected",
                access_node_id = access_node.as_str(),
                credential = ?credential,
                "the node credential does not allow a connection"
            );
            self.audit(
                &access_node,
                AuditAction::NodeAuthFailed,
                AuditOutcome::Denied,
            )
            .await;
            self.refuse_with_details(
                close_code,
                code,
                "the node is not trusted by this owner",
                RawObject::empty(),
                Some(envelope.message_id().clone()),
            )
            .await;
            return Flow::Finished;
        }

        // 认证收尾：已批准配对 → consumed 的写集必须先落库，再发 node.ready（design D3）。
        let pending = handshake::pending_pairing(&view).map(|record| record.id().clone());
        let at = self.conn.authority.now();
        let completion =
            self.conn
                .authority
                .complete_auth(authenticated.fact, pending.as_ref(), &at);
        match completion.consume_pairing {
            Some(pairing) => {
                let actor = acp_core::model::Actor::Node {
                    node: access_node.clone(),
                    access_node: access_node.clone(),
                };
                if let Err(error) = self.conn.core.consume_pairing(&actor, &pairing).await {
                    // 写集没提交成功就绝不进入业务阶段（不半认证）。
                    warn!(
                        event = "node_link.consume_pairing_failed",
                        pairing_id = pairing.as_str(),
                        error = ?error,
                        "the pairing consumption write set failed; the connection is closed"
                    );
                    return self.internal_fault(envelope).await;
                }
                info!(
                    event = "node_link.pairing_consumed",
                    access_node_id = access_node.as_str(),
                    pairing_id = pairing.as_str(),
                    "the approved pairing was consumed by the first successful authentication"
                );
            }
            None => {
                // 首次认证之外的成功握手没有配对可消费，但收尾写集仍然存在：`last_connected_at` 与
                // `node.authenticated` 必须在同一事务提交（§11.6 第 9 条；`consume_pairing` 已写过一条的
                // 场合不重复）。
                if let Err(error) = self.conn.core.record_node_connected(&access_node).await {
                    warn!(
                        event = "node_link.connected_write_failed",
                        access_node_id = access_node.as_str(),
                        error = ?error,
                        "the authentication wrap-up write set could not be written; the connection is closed"
                    );
                    return self.internal_fault(envelope).await;
                }
            }
        }

        // 注册连接（撤销传播要能找到它），随后发 `node.ready`。
        let Some(ready) = ready_body(&self.conn.authority, &view, self.limits) else {
            warn!(
                event = "node_link.ready_body_failed",
                "the node.ready body cannot be assembled"
            );
            return self.internal_fault(envelope).await;
        };
        let Some(connection_id) = challenge_id_to_uuid(&challenge) else {
            return self.internal_fault(envelope).await;
        };
        let (handle, receiver) = ConnectionHandle::new(
            connection_id.clone(),
            access_node.clone(),
            self.peer.client_ip,
            self.limits,
            Arc::clone(self.conn.core.ids()),
        );
        self.conn.registry.register(Arc::clone(&handle));
        if let Err(fault) = handle.send(MessageType::NodeReady, &ready) {
            warn!(
                event = "node_link.ready_not_sent",
                access_node_id = access_node.as_str(),
                fault = ?fault,
                "the node.ready message could not be queued"
            );
            self.conn
                .registry
                .unregister(handle.connection_id().as_str());
            self.close(close::UNAVAILABLE, "the owner cannot serve this connection")
                .await;
            return Flow::Finished;
        }
        info!(
            event = "node_link.authenticated",
            access_node_id = access_node.as_str(),
            connection_id = connection_id.as_str(),
            "a node-link connection completed authentication"
        );
        Flow::Ready { handle, receiver }
    }

    // -----------------------------------------------------------------------------------------
    // 业务阶段
    // -----------------------------------------------------------------------------------------

    async fn business_phase(
        &mut self,
        handle: Arc<ConnectionHandle>,
        mut receiver: mpsc::Receiver<Outbound>,
    ) {
        let heartbeat = self.limits.heartbeat_interval();
        let windows = self.conn.windows;
        let mut next_ping = Instant::now() + heartbeat;
        let mut last_ping: Option<Base64Url<16>> = None;
        let ending = loop {
            // 关闭请求（撤销传播等）：先排空已入队消息，再发 close 帧。
            if let Some((code, reason)) = handle.close_request() {
                break Ending::Close(code, reason);
            }
            let event = {
                let shutdown = self.peer.shutdown.clone();
                let Some(connection) = self.connection.as_mut() else {
                    break Ending::Close(close::UNAVAILABLE, "the connection is already closed");
                };
                let requested = handle.close_signal.notified();
                let outbound = receiver.recv();
                let tick = tokio::time::sleep(SESSION_TICK);
                tokio::select! {
                    biased;
                    () = requested => Event::CloseRequested,
                    () = shutdown.wait() => Event::Shutdown,
                    Some(outbound) = outbound => Event::Outbound(outbound),
                    frame = connection.recv() => Event::Frame(frame),
                    () = tick => Event::Tick,
                }
            };
            match event {
                Event::CloseRequested => {
                    if let Some((code, reason)) = handle.close_request() {
                        break Ending::Close(code, reason);
                    }
                }
                Event::Shutdown => break Ending::Close(close::NORMAL, "daemon shutdown"),
                Event::Outbound(outbound) => {
                    handle.consumed(&outbound);
                    if self.write_now(&outbound.text).await.is_err() {
                        break Ending::Close(close::NORMAL, "the connection ended");
                    }
                }
                Event::Frame(frame) => match frame {
                    None => {
                        break Ending::Close(
                            close::NORMAL,
                            "the access node closed the connection",
                        );
                    }
                    Some(Err(WsError::MessageTooLarge)) => {
                        // 传输层已按 §2.5 发过 1009 close 帧，这里不再发第二个：直接放弃连接。
                        break Ending::TransportClosed(
                            "a message exceeded the connection limit (1009)",
                        );
                    }
                    Some(Err(WsError::Transport(detail))) => {
                        debug!(event = "node_link.transport_closed", detail = %detail, "the connection ended");
                        break Ending::TransportClosed("the connection ended");
                    }
                    Some(Ok(WsMessage::Binary(_))) => {
                        warn!(
                            event = "node_link.binary_frame",
                            "a binary frame is not allowed on node-link"
                        );
                        // binary 帧按 §2.1 关闭：它已经是帧级错误，`link.error` 仍尽力送出。
                        self.refuse_post(
                            ErrorCode::ProtocolInvalidJson,
                            "binary frames are not allowed on this endpoint",
                            RawObject::empty(),
                            &handle,
                        )
                        .await;
                        break Ending::Close(close::PROTOCOL, "binary frame");
                    }
                    Some(Ok(WsMessage::Text(text))) => {
                        self.last_inbound = Instant::now();
                        if let Some((code, reason)) =
                            self.on_post_auth(&text, &handle, &mut last_ping).await
                        {
                            break Ending::Close(code, reason);
                        }
                    }
                },
                Event::Tick => {
                    let now = Instant::now();
                    if now.duration_since(self.last_inbound) >= windows.silence {
                        warn!(
                            event = "node_link.heartbeat_timeout",
                            access_node_id = handle.node_id().as_str(),
                            silence_seconds = windows.silence.as_secs(),
                            "no inbound message within the heartbeat window"
                        );
                        break Ending::Close(close::TIMEOUT, "heartbeat timeout");
                    }
                    if let Some(saturated) = handle.saturated_for() {
                        if saturated >= windows.slow_consumer {
                            warn!(
                                event = "node_link.slow_consumer",
                                access_node_id = handle.node_id().as_str(),
                                pending_messages = handle.pending().messages,
                                "the connection stayed above the queue high water mark"
                            );
                            break Ending::Close(
                                close::NORMAL,
                                "the connection cannot consume messages fast enough",
                            );
                        }
                    } else if now >= next_ping {
                        let Some(body) = ping_body() else {
                            break Ending::Close(close::UNAVAILABLE, "entropy unavailable");
                        };
                        last_ping = Some(body.nonce.clone());
                        if let Err(fault) = handle.send(MessageType::LinkPing, &body) {
                            warn!(
                                event = "node_link.ping_not_sent",
                                fault = ?fault,
                                "the heartbeat ping could not be queued"
                            );
                            break Ending::Close(
                                close::NORMAL,
                                "the connection cannot consume messages fast enough",
                            );
                        }
                        next_ping = now + heartbeat;
                    }
                }
            }
        };
        let (close_code, close_reason) = match &ending {
            Ending::Close(code, reason) => (*code, *reason),
            Ending::TransportClosed(reason) => (0, *reason),
        };
        debug!(
            event = "node_link.connection_closing",
            access_node_id = handle.node_id().as_str(),
            connection_id = handle.connection_id().as_str(),
            close_code,
            reason = close_reason,
            "the node-link connection is closing"
        );
        // 先把已入队的消息写完（撤销推送必须先送达），再发 close 帧。
        self.drain(&mut receiver).await;
        self.conn
            .registry
            .unregister(handle.connection_id().as_str());
        if let Ending::Close(code, reason) = ending {
            self.close(code, reason).await;
        } else {
            // 传输层已关闭（1009 或流已终止）：放弃连接，不重复发 close 帧。
            self.connection.take();
        }
    }

    /// 处理一条认证后的消息；返回 `Some(_)` 表示会话应以该 close code 结束。
    async fn on_post_auth(
        &mut self,
        text: &str,
        handle: &Arc<ConnectionHandle>,
        last_ping: &mut Option<Base64Url<16>>,
    ) -> Option<(u16, &'static str)> {
        let (judged, next) = wire::judge(
            text,
            Phase::PostAuth,
            Some(self.inbound_next),
            Some(handle.connection_id()),
        );
        // 序号先记账：`judge` 只在「信封合法且序号正好是下一个」时给出 `next`（§2.2 的序号属于
        // 对端发出的消息，body/type 是否被接受不影响它），因此被拒的消息（post_mvp、未实现的 type、
        // body 校验失败）也必须让本方向的期望序号前进，否则一次拒绝就让连接之后全部错位。
        if let Some(next) = next {
            self.inbound_next = next;
        }
        let envelope = match judged {
            Inbound::Message(envelope) => envelope,
            Inbound::Rejected { code, message } => {
                // 消息级协议错误：消息不生效、连接保持可用（§14.1：只有致命的握手/连接错误才关闭）。
                self.reject_post(code, message, None, handle).await;
                return None;
            }
            Inbound::Fatal {
                code,
                message,
                close_code,
            } => {
                self.refuse_post(code, message, RawObject::empty(), handle)
                    .await;
                return Some((close_code, "fatal protocol error"));
            }
        };
        match envelope.message_type() {
            // §12.6：`link.pong` 必须原样回填 nonce；本机主动 ping 的回显对不上时只记日志。
            MessageType::LinkPong => {
                match decode_body::<Pong>(&envelope) {
                    Some(body) => {
                        if last_ping.as_ref() != Some(&body.nonce) {
                            debug!(
                                event = "node_link.pong_mismatch",
                                "the link.pong nonce does not match the outstanding ping"
                            );
                        }
                    }
                    None => {
                        self.reject_post(
                            ErrorCode::ProtocolSchemaInvalid,
                            wire::SCHEMA_MESSAGE,
                            Some(envelope.message_id().clone()),
                            handle,
                        )
                        .await;
                    }
                }
                None
            }
            MessageType::LinkPing => {
                let Some(body) = decode_body::<Ping>(&envelope) else {
                    self.reject_post(
                        ErrorCode::ProtocolSchemaInvalid,
                        wire::SCHEMA_MESSAGE,
                        Some(envelope.message_id().clone()),
                        handle,
                    )
                    .await;
                    return None;
                };
                // 原样回填 nonce（§12.6）。
                let pong = Pong { nonce: body.nonce };
                if handle.send(MessageType::LinkPong, &pong).is_err() {
                    return Some((
                        close::NORMAL,
                        "the connection cannot consume messages fast enough",
                    ));
                }
                None
            }
            MessageType::LinkError => {
                debug!(
                    event = "node_link.peer_error",
                    message_id = envelope.message_id().as_str(),
                    "the access node reported a link error"
                );
                None
            }
            _ => {
                let outcome = match self.conn.route.as_ref() {
                    Some(route) => route.route(handle, &envelope).await,
                    None => RouteOutcome::Unclaimed,
                };
                if outcome == RouteOutcome::Unclaimed {
                    // 已知但本切片尚无实现的 v1 类型也显式拒绝（绝不静默丢弃）；
                    // WP5/WP6 注册路由后这条分支不再命中。
                    self.reject_post(
                        ErrorCode::ProtocolTypeUnsupported,
                        wire::TYPE_MESSAGE,
                        Some(envelope.message_id().clone()),
                        handle,
                    )
                    .await;
                }
                None
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // 消息写出与失败关闭
    // -----------------------------------------------------------------------------------------

    /// 认证前的消息（`node.challenge`/`link.error`）：信封必须省略连接字段（§2.2）。
    async fn write_pre_auth<T: Serialize>(
        &mut self,
        message_type: MessageType,
        body: &T,
    ) -> Result<(), SendFault> {
        let Some(message_id) = self.conn.message_id() else {
            return Err(SendFault::Encoding);
        };
        let body = encode_body(body).map_err(|_| SendFault::Encoding)?;
        let envelope =
            Envelope::new(message_type, message_id, None, body).map_err(|_| SendFault::Encoding)?;
        let text = envelope.encode().map_err(|_| SendFault::Encoding)?;
        self.write_now(&text).await
    }

    /// 直接写一帧（连接已关闭时返回错误，不 panic）。
    async fn write_now(&mut self, text: &str) -> Result<(), SendFault> {
        match self.connection.as_mut() {
            Some(connection) => connection
                .send_text(text.to_owned())
                .await
                .map_err(|_| SendFault::Closed),
            None => Err(SendFault::Closed),
        }
    }

    /// 关闭连接（`WsConnection::close` 消费所有权，因此 `take` 一次；重复调用是 no-op）。
    async fn close(&mut self, code: u16, reason: &'static str) {
        if let Some(connection) = self.connection.take() {
            connection.close(code, reason).await;
        }
    }

    /// 排空已入队消息（预算内尽力而为；对端已断开时立即结束）。
    async fn drain(&mut self, receiver: &mut mpsc::Receiver<Outbound>) {
        let deadline = TokioInstant::now() + CLOSE_DRAIN_BUDGET;
        loop {
            match tokio::time::timeout_at(deadline, receiver.recv()).await {
                Ok(Some(outbound)) => {
                    if self.write_now(&outbound.text).await.is_err() {
                        return;
                    }
                }
                Ok(None) | Err(_) => return,
            }
        }
    }

    /// 消息级拒绝（握手阶段）：回 `link.error`，连接保持可用。
    async fn reject(&mut self, code: ErrorCode, message: &'static str) {
        let Some(body) = wire::error_body(code, message, RawObject::empty(), None) else {
            warn!(
                event = "node_link.error_body_failed",
                code = code.as_str(),
                "the link.error body cannot be assembled"
            );
            return;
        };
        debug!(
            event = "node_link.message_rejected",
            code = code.as_str(),
            "the message was rejected without closing the connection"
        );
        let _ = self.write_pre_auth(MessageType::LinkError, &body).await;
    }

    /// 消息级拒绝（认证后）：信封必须携带连接字段与序号。
    async fn reject_post(
        &mut self,
        code: ErrorCode,
        message: &'static str,
        correlation: Option<Uuid>,
        handle: &Arc<ConnectionHandle>,
    ) {
        let Some(body) = wire::error_body(code, message, RawObject::empty(), correlation) else {
            warn!(
                event = "node_link.error_body_failed",
                code = code.as_str(),
                "the link.error body cannot be assembled"
            );
            return;
        };
        debug!(
            event = "node_link.message_rejected",
            code = code.as_str(),
            "the message was rejected without closing the connection"
        );
        let _ = handle.send(MessageType::LinkError, &body);
    }

    /// 致命拒绝（握手阶段）：回 `link.error`（尽力）后按 close code 关闭。
    async fn refuse(
        &mut self,
        close_code: u16,
        code: ErrorCode,
        message: &'static str,
        details: RawObject,
    ) {
        self.refuse_with_details(close_code, code, message, details, None)
            .await;
    }

    async fn refuse_with_details(
        &mut self,
        close_code: u16,
        code: ErrorCode,
        message: &'static str,
        details: RawObject,
        correlation: Option<Uuid>,
    ) {
        debug!(
            event = "node_link.connection_refused",
            code = code.as_str(),
            close_code,
            "the node-link connection is being closed with a protocol error"
        );
        if let Some(body) = wire::error_body(code, message, details, correlation) {
            let _ = self.write_pre_auth(MessageType::LinkError, &body).await;
        }
        self.close(close_code, message).await;
    }

    /// 致命拒绝（认证后）：把 `link.error` 入队，由会话在关闭前**排空**（绝不直接关闭连接，
    /// 否则错误根本送不出去）；调用方随后以对应 close code 结束会话。
    async fn refuse_post(
        &mut self,
        code: ErrorCode,
        message: &'static str,
        details: RawObject,
        handle: &Arc<ConnectionHandle>,
    ) {
        debug!(
            event = "node_link.connection_refused",
            code = code.as_str(),
            "the node-link connection is being closed with a protocol error"
        );
        if let Some(body) = wire::error_body(code, message, details, None) {
            let _ = handle.send(MessageType::LinkError, &body);
        }
    }

    /// 内部不可用：回 `link.error`（internal.unavailable）并以 4500 关闭。
    async fn internal_fault(&mut self, envelope: &Envelope) -> Flow {
        self.refuse_with_details(
            close::UNAVAILABLE,
            ErrorCode::InternalUnavailable,
            wire::INTERNAL_MESSAGE,
            RawObject::empty(),
            Some(envelope.message_id().clone()),
        )
        .await;
        Flow::Finished
    }

    /// 认证失败留痕（`SECURITY_DESIGN.md` §14.2）：动作固定 `node.auth_failed`，写入失败只记日志。
    async fn audit(&mut self, node: &NodeId, action: AuditAction, outcome: AuditOutcome) {
        if let Err(error) = self
            .conn
            .core
            .record_node_link_auth(node, action, outcome)
            .await
        {
            warn!(
                event = "node_link.auth_audit_failed",
                action = action.as_str(),
                error = ?error,
                "the authentication audit row could not be written"
            );
        }
    }

    /// 握手超时/失败时的留痕：知道对端 id 时写 `node.auth_failed`，否则只记日志。
    async fn audit_failure(&mut self, outcome: AuditOutcome) {
        if let Some(node) = self.handshake.node.clone() {
            self.audit(&node, AuditAction::NodeAuthFailed, outcome)
                .await;
        }
    }
}

/// `select!` 的事件源（把借用收在一处，避免在 `select!` 里调用 `&mut self` 方法）。
enum Event {
    CloseRequested,
    Shutdown,
    Outbound(Outbound),
    Frame(Option<Result<WsMessage, WsError>>),
    Tick,
}

/// 会话的结束方式：本层发 close 帧，或传输层已经关闭（1009 超限/流终止）——后者不得再发 close 帧。
enum Ending {
    /// 本层以该 close code 与原因关闭。
    Close(u16, &'static str),
    /// 传输层已关闭连接（不重复发 close 帧）。
    TransportClosed(&'static str),
}

/// 协商失败的错误映射（§2.3/§11.3）。
fn negotiation_error(fault: &NegotiationFault) -> (ErrorCode, &'static str) {
    match fault {
        NegotiationFault::VersionUnsupported => (
            ErrorCode::ProtocolVersionUnsupported,
            "the protocol version is not supported",
        ),
        NegotiationFault::FeaturesMissing(_) => (
            ErrorCode::ProtocolFeatureRequired,
            "a required feature was not negotiated",
        ),
    }
}

/// `node.challenge` 的 body（§12.2）。
///
/// `catalog_revision` 由调用方传本机水位：同一个值进了 `ChallengeRequest`（签进挑战 transcript），
/// 也必须进 wire body，否则 Access 验不了 `nodeProof` 也签不了自己的 proof（2026-09-26 修订）。
fn challenge_body(
    issue: &ChallengeIssue,
    authority: &Authority,
    selected: &[String],
    catalog_revision: u64,
) -> Option<NodeChallenge> {
    Some(NodeChallenge {
        selected_protocol_version: node_link_protocol::common::ProtocolVersionV1::new(
            handshake::PROTOCOL_VERSION,
        )
        .ok()?,
        connection_id: Uuid::parse(issue.challenge_id.as_str()).ok()?,
        owner_node_id: Uuid::parse(authority.local_node().as_str()).ok()?,
        server_nonce: Base64Url::<32>::parse(issue.server_nonce.as_str()).ok()?,
        selected_features: handshake::wire_features(selected)?,
        catalog_revision: DecimalString::parse(&catalog_revision.to_string()).ok()?,
        node_proof: Base64Url::<64>::parse(&issue.host_proof.to_base64url()).ok()?,
    })
}

/// `node.ready` 的 body（§12.2）：`catalogRevision` 取本机全局水位（见模块文档的登记口径），
/// 与 `node.challenge.catalogRevision` 同源；`serverEpoch` 与 Sync 同义，`limits` 是协商结果。
fn ready_body(
    authority: &Authority,
    view: &NodeLinkHandshakeView,
    limits: SessionLimits,
) -> Option<NodeReady> {
    Some(NodeReady {
        owner_node_id: Uuid::parse(authority.local_node().as_str()).ok()?,
        catalog_revision: DecimalString::parse(&view.head.global_sequence.get().to_string())
            .ok()?,
        limits: limits.to_wire()?,
        server_epoch: Uuid::parse(view.head.server_epoch.as_str()).ok()?,
    })
}

/// 挑战标识（wire 的 `connectionId`）的 UUID 视图。
fn challenge_id_to_uuid(issue: &ChallengeIssue) -> Option<Uuid> {
    Uuid::parse(issue.challenge_id.as_str()).ok()
}

/// `node.proof` 到 `identity-auth` 输入的装配（回显字段来自本机签发的挑战，不取对端自报值）。
struct ProofInput<'a> {
    access_node: &'a NodeId,
    server_nonce: &'a Nonce,
    client_nonce: &'a Nonce,
    proof: &'a NodeProof,
}

impl ProofInput<'_> {
    fn to_submission(&self) -> Option<ProofSubmission> {
        Some(ProofSubmission {
            kind: ConnectionKind::NodeLink,
            challenge_id: ChallengeId::parse(self.proof.connection_id.as_str()).ok()?,
            server_nonce: self.server_nonce.clone(),
            peer: PeerIdentity::Node(self.access_node.clone()),
            client_nonce: self.client_nonce.clone(),
            signature: P1363Signature::try_from_bytes(self.proof.node_proof.as_bytes()).ok()?,
        })
    }
}

/// 一条 `link.ping` 的 body（16 字节随机 nonce，§12.6）。
fn ping_body() -> Option<Ping> {
    use base64::Engine as _;
    let bytes = uuid::Uuid::new_v4().into_bytes();
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    Some(Ping {
        nonce: Base64Url::<16>::parse(&encoded).ok()?,
    })
}

/// 解码消息 body（`Envelope` 的 body 是原始字节；解码失败按 schema 错误处理）。
fn decode_body<T: DeserializeOwned>(envelope: &Envelope) -> Option<T> {
    serde_json::from_str(envelope.body().get()).ok()
}

/// 定长 base64url 字段（`acpr-wire` 的值对象）的**规范**文本。
///
/// `Base64Url<BYTES>` 只在解码期保留字节，不保留文本；重新编码（无填充 base64url）得到的文本与 wire
/// 上的写法逐字一致，因为它能通过同一套规范校验。core 的 newtype（`Nonce` 等）只接受文本，因此握手
/// 回显字段经这里转一次。
fn base64url_text<const BYTES: usize>(value: &Base64Url<BYTES>) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value.as_bytes())
}

/// 单 IP 的在途握手配额。
///
/// 接入层的 TLS 握手池（64 个槽位、单次 10 秒）对每个连接都生效，但没有任何按源 IP 的准入闸门；
/// 本层因此限制「同一源地址上尚未完成认证的连接数」。取值 4 的依据：一次正常重连最多同时出现
/// 「旧连接尚未被对端确认关闭 + 新连接已建立 + 一次立刻重试」三条，留一条余量。
struct HandshakeSlots {
    inner: Arc<Mutex<BTreeMap<IpAddr, usize>>>,
    max_per_ip: usize,
}

impl HandshakeSlots {
    fn new(max_per_ip: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(BTreeMap::new())),
            max_per_ip,
        }
    }

    /// 当前被占用的配额数（源地址求和；测试观测口，见 [`NodeLinkConn::in_flight_handshakes`]）。
    #[cfg(test)]
    fn in_flight(&self) -> usize {
        lock(&self.inner).values().sum()
    }

    fn acquire(&self, ip: IpAddr) -> Option<HandshakeSlot> {
        let mut slots = lock(&self.inner);
        if slots.len() >= MAX_TRACKED_IPS && !slots.contains_key(&ip) {
            // 条目数有界（与限流器同口径）：新的源地址在表满时被拒绝，而不是无界增长。
            return None;
        }
        let used = slots.entry(ip).or_insert(0);
        if *used >= self.max_per_ip {
            return None;
        }
        *used += 1;
        Some(HandshakeSlot {
            inner: Arc::clone(&self.inner),
            ip,
        })
    }
}

/// 在途握手配额凭证：会话结束（任何路径）即释放。
struct HandshakeSlot {
    inner: Arc<Mutex<BTreeMap<IpAddr, usize>>>,
    ip: IpAddr,
}

impl Drop for HandshakeSlot {
    fn drop(&mut self) {
        let mut slots = lock(&self.inner);
        if let Some(used) = slots.get_mut(&self.ip) {
            *used = used.saturating_sub(1);
            if *used == 0 {
                slots.remove(&self.ip);
            }
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
