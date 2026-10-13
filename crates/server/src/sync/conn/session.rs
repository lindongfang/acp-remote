//! 一条 WSS 连接的会话驱动：握手状态机、信封/序号、心跳与每连接发送队列
//! （`docs/SYNC_PROTOCOL.md` §3/§4/§8/§12/§13/§14，`design.md` D1/D2/D4/D5）。
//!
//! 一条连接 = 一个任务 = 一个 [`Session`]：握手与业务阶段都在同一任务里推进（不 spawn 子任务），因此
//! 取消路径只有一条——`peer.shutdown`、对端关闭、致命协议错误或关闭请求，任一发生即结束整个会话。
//!
//! 阶段划分（`auth.client_hello → auth.server_challenge → auth.client_proof → auth.authenticated`）：
//!
//! 1. **准入**：认证前先过三道路径无关的闸——单节点并发 WSS 连接上限 256（§14，最靠外的一道：它
//!    限制节点总量，不管来源）、单 IP 认证尝试 10 次/分钟（§14，键是接入层给出的真实 `client_ip`）与
//!    单 IP 在途握手配额（半开连接不占满接入层的握手池）；后两者的配额凭证只覆盖握手阶段，认证完成即
//!    释放，而并发连接凭证覆盖整条会话；
//! 2. **握手**：只允许 `auth.client_hello`/`auth.client_proof`（+ 双向的 `error`），其余消息以 4401
//!    关闭；首条消息（必须是 hello）必须在 5 秒内完整到达，整个认证在 15 秒内完成，两者都超时 4408
//!    （§14 的两条固定时限）；超限的 hello（body 超过 32 KiB）在进入握手状态机之前就被拒绝；
//! 3. **来源**：canonical origin 只来自升级请求的 `Origin` 头（[`RequestOrigin`]，§3.2），与设备记录里
//!    持久化的绑定逐字比较；缺失或非 canonical（`http://…` 等）一律按 `auth.origin_mismatch` 拒绝；
//! 4. **认证收尾**：写集（已批准配对的消费 + `last_seen_at` 推进与 `device.authenticated`）必须先提交
//!    成功，才登记连接并下发 `auth.authenticated`（`design.md` D3：不半认证）；
//! 5. **业务**：认证后的消息逐条判定阶段、信封、序号与 type，再交给 [`MessageRoute`]；连接的心跳、静默
//!    超时与慢消费者看门狗在同一 `select!` 里推进。心跳类消息（本机的 `control.ping`、回填的
//!    `control.pong`）撞上高水位时按 §14 的两级口径处置：一级是推迟/丢弃这一次，二级（持续过慢）才是
//!    `resource.backpressure` + 断开（[`heartbeat_outcome`] 与 [`ConnectionHandle::saturated_for`]）。
//!
//! **认证限流的接线口径**：接入层在 WS upgrade 之前没有每路径的准入钩子，因此本层把闸门放在**会话的
//! 第一步**：任何帧（包括 `auth.client_hello`）都不再被处理，连接以 4429 关闭。效果等价——未认证的连接
//! 无法产生任何业务副作用。
//!
//! **认证失败不写审计**：`identity_auth` 的 `HandshakeFailure::audit()` 给出 `device.auth_failed`，但
//! `core::use_cases` 目前没有设备侧失败留痕入口（`record_node_link_auth` 只接受三个 node 动作），因此
//! 本层只写结构化日志。该缺口由本变更的工作包范围之外（core 的写面）决定，已在交付报告里点名。

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use acp_core::model::{Actor, DeviceId, PeerIdentity};
use acp_core::use_cases::{SyncDeviceHandshakeView, UseCases};
use identity_auth::{
    Authority, CanonicalOrigin, ChallengeId, ChallengeIssue, ChallengeRequest, ConnectionKind,
    CredentialStatus, Nonce, P1363Signature, ProofSubmission,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sync_protocol::auth::{
    Authenticated as AuthenticatedBody, ClientHello, ClientProof, Scopes, ServerChallenge,
};
use sync_protocol::common::{Base64Url, BoundedU64, DecimalString, RawObject, UIntAtLeast, Uuid};
use sync_protocol::control::Body as ControlBody;
use sync_protocol::envelope::{Envelope, MessageType, Phase, encode_body};
use sync_protocol::error::ErrorCode;
use tokio::sync::mpsc;
use tokio::time::Instant as TokioInstant;
use tracing::{debug, info, warn};

use crate::sync::conn::handshake::{self, NegotiationFault};
use crate::sync::conn::limits::{
    DEFAULT_HEARTBEAT_INTERVAL_MS, FIRST_MESSAGE_TIMEOUT, HELLO_MAX_BYTES,
    MAX_CONCURRENT_CONNECTIONS, SessionLimits, SyncConfig,
};
use crate::sync::conn::registry::{ConnectionHandle, ConnectionRegistry, Outbound, SendFault};
use crate::sync::conn::wire::{self, Inbound};
use crate::sync::conn::{
    AUTH_ATTEMPTS_PER_MINUTE, AUTH_WINDOW, HANDSHAKE_TIMEOUT, MAX_TRACKED_IPS, close,
};
use crate::transport::net::{
    PeerInfo, RateLimit, RequestOrigin, SlidingWindowLimiter, WsConnection, WsError, WsHandler,
    WsMessage,
};

/// 心跳静默上限（§13：连续 90 秒没有入站消息即关闭；固定常量、不可配置）。
pub const HEARTBEAT_SILENCE_TIMEOUT: Duration = Duration::from_secs(90);

/// 单 IP 允许的在途握手数（§14 的「单 IP pending authentication：5 connections」）。
pub const MAX_IN_FLIGHT_HANDSHAKES_PER_IP: usize = 5;

/// 会话轮询间隔：驱动心跳、静默超时与慢消费者看门狗（不引入额外的计时任务）。
const SESSION_TICK: Duration = Duration::from_millis(500);

/// 慢消费者看门狗：队列持续处于高水位超过此时长即发 `resource.backpressure` 并断开（§14）。
const SLOW_CONSUMER_GRACE: Duration = Duration::from_secs(30);

/// 关闭时排空已入队消息的预算（撤销推送必须先送达再发 close 帧）。
const CLOSE_DRAIN_BUDGET: Duration = Duration::from_secs(2);

/// 会话看门狗的三个窗口（§13/§14 的固定常量：心跳 30 秒、静默 90 秒、慢消费者宽限 30 秒）。
///
/// 生产路径只有 [`SessionWindows::from_protocol`] 一个来源（`auth.authenticated.limits` 里没有这三个
/// 值，它们不可协商也不可配置）；`#[cfg(test)]` 的 [`SyncConn::with_test_windows`] 让用例把窗口调短：
/// 30 秒与 90 秒的真实等待在用例里不可接受，而这三个值本身由常量断言锁定。并发连接上限同理，由
/// [`SyncConn::with_test_connection_cap`] 调小（真起 257 条连接同样不可接受）。
///
/// **心跳的两个出现位置刻意分开**：`auth.authenticated.heartbeatIntervalMs` 永远是 §13 的固定常量
/// 30 秒（`SessionLimits::heartbeat_interval`），而 `heartbeat` 只是本会话实际发 `control.ping` 的节奏
/// ——生产路径上两者相等，用例里只有节奏被调短（对端被告知的仍是协议值）。
#[derive(Clone, Copy, Debug)]
struct SessionWindows {
    heartbeat: Duration,
    silence: Duration,
    slow_consumer: Duration,
}

impl SessionWindows {
    /// 协议约定的窗口（唯一的生成入口）。
    fn from_protocol() -> Self {
        Self {
            heartbeat: Duration::from_millis(DEFAULT_HEARTBEAT_INTERVAL_MS),
            silence: HEARTBEAT_SILENCE_TIMEOUT,
            slow_consumer: SLOW_CONSUMER_GRACE,
        }
    }
}

/// 认证后消息的分派口（WP4 的订阅/快照/ACK 各自实现，组合根组合它们）。
///
/// `conn` 只负责阶段、信封、序号、type 与限流判定；被路由看到的消息**已经**通过这些校验。本切片
/// （WP3）不注册任何路由：v1 的业务 type 因此会被显式拒绝为 `protocol.type_unsupported`（对端可见的
/// 显式拒绝，不是静默丢弃；WP4 注册后该分支不再命中）。
#[async_trait::async_trait]
pub trait MessageRoute: Send + Sync + 'static {
    /// 处理一条认证后消息。返回 [`RouteOutcome::Claimed`] 表示本路由认领该 type。
    async fn route(&self, session: &ConnectionHandle, message: &Envelope) -> RouteOutcome;

    /// 认证完成时的**连接上下文交接**：本次协商出的 feature 列表（§5.2 的 `selectedFeatures` 同值，
    /// 排序去重）。
    ///
    /// 默认实现为空操作——不需要连接上下文的路由不必实现它。需要它的理由：快照的目录资源
    /// （`workspaces`/`agents`）与已导入会话分别受 `core.local-catalog.v1`、`resource.remote-origin.v1`
    /// 门控，而这两个判定只认**当次握手**的协商结果，wire 上没有第二个承载它的字段（§9.4/§9.6）；
    /// 连接句柄本身只承载连接级事实（`connectionId`/`deviceId`/限额/出站账本），因此协商结果经本方法
    /// 一次性交接，而不是塞进句柄（那会让"连接级事实"与"协议状态"混在一处）。
    ///
    /// 调用时机：认证成功、连接已登记之后，业务阶段开始之前——路由在收到 `sync.subscribe` 时必定已经
    /// 拿到本连接的门控输入。
    fn on_authenticated(&self, _session: &ConnectionHandle, _features: &[String]) {}
}

/// 路由的处理结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteOutcome {
    /// 已处理（响应经会话句柄入队）。
    Claimed,
    /// 不属于本路由；没有路由认领时 `conn` 回 `type_unsupported`。
    Unclaimed,
}

/// Sync WSS 端点的装配件：组合根持有一个 `Arc<SyncConn>`。
pub struct SyncConn {
    core: Arc<UseCases>,
    authority: Arc<Authority>,
    config: SyncConfig,
    registry: Arc<ConnectionRegistry>,
    route: Option<Arc<dyn MessageRoute>>,
    /// 单 IP 的新认证尝试限流（§14：10 次/分钟，键是 `PeerInfo::client_ip`）。
    auth_limit: SlidingWindowLimiter,
    /// 单 IP 的在途握手配额。
    handshakes: HandshakeSlots,
    /// 单节点并发 WSS 连接配额（§14：256）。
    connections: Arc<ConnectionSlots>,
    /// 会话看门狗的窗口（§13/§14 的固定常量）。
    windows: SessionWindows,
}

impl std::fmt::Debug for SyncConn {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SyncConn")
            .field("routed", &self.route.is_some())
            .finish_non_exhaustive()
    }
}

impl SyncConn {
    /// 装配。`core` 是用例面、`authority` 是身份状态机、`config` 是配置快照、`registry` 是连接注册表
    /// （组合根与 WP4/WP6 共享同一个句柄）。
    pub fn new(
        core: Arc<UseCases>,
        authority: Arc<Authority>,
        config: SyncConfig,
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
            connections: Arc::new(ConnectionSlots::new(MAX_CONCURRENT_CONNECTIONS)),
            windows: SessionWindows::from_protocol(),
        }
    }

    /// 测试专用：把会话看门狗的窗口换成本次用例的值（§13/§14 的三个固定常量在生产路径上只有
    /// [`SessionWindows::from_protocol`] 一个来源；30 秒/90 秒的真实等待在用例里不可接受）。
    #[cfg(test)]
    pub(crate) fn with_test_windows(
        mut self,
        heartbeat: Duration,
        silence: Duration,
        slow_consumer: Duration,
    ) -> Self {
        self.windows = SessionWindows {
            heartbeat,
            silence,
            slow_consumer,
        };
        self
    }

    /// 注册认证后消息的路由（WP4 的组合根接线）。
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

    /// 测试专用：把并发连接配额换成本次用例的值（§14 的 256 在生产路径上只有
    /// [`MAX_CONCURRENT_CONNECTIONS`] 一个来源；真起 257 条连接在用例里不可接受）。
    #[cfg(test)]
    pub(crate) fn with_test_connection_cap(mut self, cap: usize) -> Self {
        self.connections = Arc::new(ConnectionSlots::new(cap));
        self
    }

    /// 测试专用：当前被占用的并发连接配额数。
    #[cfg(test)]
    pub(crate) fn concurrent_connections(&self) -> usize {
        self.connections.occupied()
    }

    /// 连接注册表句柄（WP4 的事件扇出、WP6 的撤销钩子与它共用）。
    pub fn registry(&self) -> &Arc<ConnectionRegistry> {
        &self.registry
    }

    /// 每帧唯一的消息标识（§4.1 的 `messageId`；只用于诊断，不代替业务 id）。
    fn message_id(&self) -> Option<Uuid> {
        Uuid::parse(self.core.ids().message_id().as_str()).ok()
    }
}

/// `WsHandler` 的实现：一条连接一个会话（组合根注册到 [`crate::sync::conn::WS_PATH`]）。
pub struct WsEndpoint {
    conn: Arc<SyncConn>,
}

impl WsEndpoint {
    /// 连接处理器（`NetListener::register_ws` 的入参）。
    pub fn handler(conn: Arc<SyncConn>) -> Arc<dyn WsHandler> {
        Arc::new(Self { conn })
    }
}

#[async_trait::async_trait]
impl WsHandler for WsEndpoint {
    /// Sync **依赖** HTTP 来源：设备信任绑定在 canonical origin 上（§3.2），来源因此必须进入
    /// `auth.server_challenge` 的 transcript 与绑定比较。接入层保证传入值已按 §3.2 归一化或为「无来源」，
    /// 本层只消费它，不重复实现归一化规则。
    async fn handle(
        self: Arc<Self>,
        connection: WsConnection,
        peer: PeerInfo,
        origin: RequestOrigin,
    ) {
        Session::new(Arc::clone(&self.conn), connection, peer, origin)
            .run()
            .await;
    }
}

/// 握手阶段的连接状态。
#[derive(Default)]
struct HandshakeState {
    /// 已签发的挑战（`server_nonce`/`client_nonce` 的回显校验由状态机承担）。
    challenge: Option<ChallengeIssue>,
    client_nonce: Option<Nonce>,
    view: Option<SyncDeviceHandshakeView>,
    device: Option<DeviceId>,
    /// 本次协商出的 feature（`auth.server_challenge.selectedFeatures` 的同值）：认证成功后经
    /// [`MessageRoute::on_authenticated`] 交接给路由，供 §9.4/§9.6 的两处门控使用。
    features: Vec<String>,
}

impl HandshakeState {
    /// 已经处理过 hello（握手进入「等 proof」阶段）。
    fn expecting_proof(&self) -> bool {
        self.view.is_some()
    }
}

/// 一条连接的会话。
struct Session {
    conn: Arc<SyncConn>,
    /// 连接本体：`WsConnection::close` 消费所有权，因此会话持有 `Option`，关闭即 `take`（绝不二次关闭）。
    connection: Option<WsConnection>,
    peer: PeerInfo,
    /// 升级请求里已取的来源（§3.2：`Known` = 归一化后的 canonical origin 文本，`Absent` = 无 `Origin` 头）。
    origin: RequestOrigin,
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
    fn new(
        conn: Arc<SyncConn>,
        connection: WsConnection,
        peer: PeerInfo,
        origin: RequestOrigin,
    ) -> Self {
        let limits = SessionLimits::negotiate(&conn.config);
        Self {
            conn,
            connection: Some(connection),
            peer,
            origin,
            limits,
            handshake: HandshakeState::default(),
            inbound_next: 1,
            last_inbound: Instant::now(),
        }
    }

    async fn run(mut self) {
        let client_ip = self.peer.client_ip;
        // ⓪ 单节点并发 WSS 连接上限（§14：256）。它排在两张按来源的闸门之前：节点级资源比来源级配额
        // 更靠外，而这条拒绝与「认证尝试」无关（连接根本没进认证，不该消耗那 10 次/分钟的额度）。
        // 超限一律显式拒绝：不排队、不静默丢弃、不挤掉既有连接。
        let Some(_connection_slot) = self.conn.connections.acquire() else {
            warn!(
                event = "sync.connection_cap_reached",
                client_ip = %client_ip,
                max = MAX_CONCURRENT_CONNECTIONS,
                "the node already serves the maximum number of concurrent sync connections"
            );
            self.refuse(
                close::RATE_LIMITED,
                ErrorCode::ResourceRateLimited,
                "the node cannot accept another connection right now",
                wire::rate_limit_details(Duration::from_secs(1)),
            )
            .await;
            return;
        };
        // ① 认证尝试限流（§14：10 次/分钟/IP，键是接入层给出的真实对端地址）。
        if let RateLimit::Denied { retry_after } = self.conn.auth_limit.check(client_ip) {
            warn!(
                event = "sync.auth_rate_limited",
                client_ip = %client_ip,
                retry_after_ms = retry_after.as_millis() as u64,
                "sync authentication attempts were rate limited"
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
        // ② 在途握手配额：凭证只覆盖**握手阶段**，认证完成（或握手结束）立即释放，因此本配额限制的是
        // 「同一源地址上尚未完成认证的连接数」，而不是「同源连接数」（§14 的取向只谈半开握手）。
        let handshake_slot = match self.conn.handshakes.acquire(client_ip) {
            Some(slot) => slot,
            None => {
                warn!(
                    event = "sync.handshake_slots_exhausted",
                    client_ip = %client_ip,
                    max = MAX_IN_FLIGHT_HANDSHAKES_PER_IP,
                    "too many handshakes in flight for this client address"
                );
                // 槽位会在某条在途握手结束（或 15 秒窗口到期）时释放，因此退避建议取握手窗口
                // （§14 允许 `resource.rate_limited` 的下调面之外的登记字段 `retryAfterMs`）。
                self.refuse(
                    close::RATE_LIMITED,
                    ErrorCode::ResourceRateLimited,
                    "too many handshakes in flight for this client address",
                    wire::rate_limit_details(HANDSHAKE_TIMEOUT),
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
        let started = TokioInstant::now();
        // §14 的两条时限（都是固定常量）：连接建立后第一条消息必须是 `auth.client_hello` 且必须在
        // **5 秒**内完整到达；整个认证必须在 15 秒内完成。5 秒窗口只在握手状态机确实处理完一条 hello
        // 之后结束——判据不是「收到任意一帧」，否则一条无关的帧就能把这条认证前固定上限推后到 15 秒。
        let handshake_deadline = started + HANDSHAKE_TIMEOUT;
        let first_message_deadline = started + FIRST_MESSAGE_TIMEOUT;
        let mut hello_received = false;
        loop {
            let shutdown = self.peer.shutdown.clone();
            let Some(connection) = self.connection.as_mut() else {
                return Flow::Finished;
            };
            let deadline = if hello_received {
                handshake_deadline
            } else {
                first_message_deadline
            };
            tokio::select! {
                biased;
                () = shutdown.wait() => {
                    debug!(event = "sync.shutdown", "the session ends because the daemon is shutting down");
                    return Flow::Finished;
                }
                () = tokio::time::sleep_until(deadline) => {
                    if hello_received {
                        warn!(event = "sync.handshake_timeout", "the handshake did not complete within 15 seconds");
                        self.close(close::TIMEOUT, "handshake timeout").await;
                    } else {
                        warn!(
                            event = "sync.first_message_timeout",
                            "the first auth.client_hello did not arrive within 5 seconds"
                        );
                        self.close(close::TIMEOUT, "the first message did not arrive in time")
                            .await;
                    }
                    return Flow::Finished;
                }
                frame = connection.recv() => {
                    match frame {
                        None => return Flow::Finished,
                        Some(Err(WsError::MessageTooLarge)) => return Flow::Finished,
                        Some(Err(WsError::Transport(detail))) => {
                            debug!(event = "sync.transport_closed", detail = %detail, "the connection ended");
                            return Flow::Finished;
                        }
                        Some(Ok(WsMessage::Binary(_))) => {
                            // §3.1：只接受 text frame；binary 帧必须关闭连接，且内容不得进入协议逻辑。
                            warn!(event = "sync.binary_frame", "a binary frame is not allowed on sync");
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
                            let flow = self.on_pre_auth(&text).await;
                            // 握手状态机确实处理完一条 `auth.client_hello`（挑战已签发）：5 秒的首条
                            // 消息窗口到此结束，余下的认证交给 15 秒整窗（§14 的两条时限）。
                            hello_received |= self.handshake.view.is_some();
                            match flow {
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
            // §8/§14：认证完成前的业务消息一律 4401 关闭，且认证阶段只接受 hello 与 proof 各一次。
            self.refuse(
                close::UNAUTHENTICATED,
                ErrorCode::AuthRequired,
                wire::AUTH_REQUIRED_MESSAGE,
                RawObject::empty(),
            )
            .await;
            return Flow::Finished;
        }
        // §14 的认证前固定上限：首条 `auth.client_hello` 的消息体最多 32 KiB。判据量的是信封 `body` 的
        // 原始 UTF-8 字节数；超限即拒绝，**不**进入握手状态机、不产生任何认证状态（注册表只在认证成功
        // 之后登记，因此这条路径也不占用设备槽位）。接入层在 1 MiB 处已按 1009 关掉更大的帧，因此这里
        // 处理的是「过了传输层、但越过认证前上限」的那一段，错误码用 §12.2 的 `protocol.message_too_large`。
        if envelope.message_type() == MessageType::AuthClientHello
            && envelope.body().get().len() as u64 > HELLO_MAX_BYTES
        {
            warn!(
                event = "sync.hello_too_large",
                bytes = envelope.body().get().len(),
                max = HELLO_MAX_BYTES,
                "the auth.client_hello exceeds the pre-authentication body limit"
            );
            self.refuse_with_details(
                close::PROTOCOL,
                ErrorCode::ProtocolMessageTooLarge,
                "the message exceeds the pre-authentication limit",
                RawObject::empty(),
                Some(envelope.message_id().clone()),
            )
            .await;
            return Flow::Finished;
        }
        match envelope.message_type() {
            MessageType::AuthClientHello => self.on_hello(&envelope).await,
            MessageType::AuthClientProof => self.on_proof(&envelope).await,
            // 对端报告了一条连接级错误：记录后继续（`error` 不改变本机状态）。
            _ => {
                debug!(
                    event = "sync.peer_error",
                    message_id = envelope.message_id().as_str(),
                    "the client reported a connection error"
                );
                Flow::Continue
            }
        }
    }

    /// `auth.client_hello`：版本交集 → feature 协商 → Host 身份 → 来源 → 组装 `PeerTrust` → 签发挑战。
    async fn on_hello(&mut self, envelope: &Envelope) -> Flow {
        if self.handshake.expecting_proof() {
            // 由 `pre_auth_allowed` 保证不可达：进入「等 proof」之后 `auth.client_hello` 不再在认证前
            // 白名单里（§14：认证前最多只接收 hello 与 proof 各一次），`on_pre_auth` 会以 4401 关闭连接。
            // 保留这条分支只为把状态机写全——重复 hello 若真的进来就会重签一次挑战，让已签发的那个悬空。
            self.refuse(
                close::UNAUTHENTICATED,
                ErrorCode::AuthRequired,
                wire::AUTH_REQUIRED_MESSAGE,
                RawObject::empty(),
            )
            .await;
            return Flow::Finished;
        }
        let Some(hello) = decode_body::<ClientHello>(envelope) else {
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
                    event = "sync.feature_negotiation_failed",
                    missing = ?missing,
                    "the client requires features this host does not support"
                );
                self.refuse_with_details(
                    close::VERSION,
                    code,
                    message,
                    wire::feature_details(&missing),
                    Some(envelope.message_id().clone()),
                )
                .await;
                return Flow::Finished;
            }
        };
        // Host 身份自报值必须等于本机 host id（§8.2：客户端同样核对 Host ID；`auth.host_mismatch`
        // 是 registry 为这条判定登记的码）。
        if hello.host_id.as_str() != self.conn.authority.local_node().as_str() {
            warn!(
                event = "sync.host_mismatch",
                reported = hello.host_id.as_str(),
                "the client hello declares another host identity"
            );
            self.refuse_with_details(
                close::UNAUTHENTICATED,
                ErrorCode::AuthHostMismatch,
                "the connection is bound to another host identity",
                RawObject::empty(),
                Some(envelope.message_id().clone()),
            )
            .await;
            return Flow::Finished;
        }
        // §3.2：canonical origin 只来自升级请求的 `Origin` 头。`Absent` 无可比较的绑定（§8.5 的已知
        // 偏差：v1 的设备信任绑定在 canonical origin 上），非 `https://` 的取值不是 canonical origin
        // ——两者都按 `auth.origin_mismatch` 拒绝，绝不以空字节串放行。
        let origin = match resolve_origin(&self.origin) {
            Some(origin) => origin,
            None => {
                warn!(
                    event = "sync.origin_missing",
                    present = self.origin.as_str().is_some(),
                    "the connection has no usable canonical origin"
                );
                self.refuse_with_details(
                    close::UNAUTHENTICATED,
                    ErrorCode::AuthOriginMismatch,
                    "the connection origin does not match the device binding",
                    RawObject::empty(),
                    Some(envelope.message_id().clone()),
                )
                .await;
                return Flow::Finished;
            }
        };
        let Ok(device_id) = DeviceId::new(hello.device_id.as_str()) else {
            self.reject(ErrorCode::ProtocolSchemaInvalid, wire::SCHEMA_MESSAGE)
                .await;
            return Flow::Continue;
        };
        // 每次握手都重新读一次持久事实（`design.md` D3：验签公钥只来自持久化信任的当次快照）。
        let view = match self.conn.core.sync_device_handshake_view(&device_id).await {
            Ok(view) => view,
            Err(error) => {
                warn!(
                    event = "sync.handshake_view_failed",
                    error = ?error,
                    "the sync handshake could not read the persisted trust snapshot"
                );
                return self.internal_fault(envelope).await;
            }
        };
        // 设备记录里的绑定逐字比较（§3.2）；未知设备没有配对行，因此跳过比较、照常签发挑战。
        let binding_mismatch = view
            .pairing
            .as_ref()
            .is_some_and(|pairing| pairing.host_binding() != origin.as_str());
        if binding_mismatch {
            warn!(
                event = "sync.origin_mismatch",
                device_id = device_id.as_str(),
                "the request origin does not match the persisted device binding"
            );
            self.refuse_with_details(
                close::UNAUTHENTICATED,
                ErrorCode::AuthOriginMismatch,
                "the connection origin does not match the device binding",
                RawObject::empty(),
                Some(envelope.message_id().clone()),
            )
            .await;
            return Flow::Finished;
        }
        let Ok(client_nonce) = Nonce::new(&base64url_text(&hello.client_nonce)) else {
            self.reject(ErrorCode::ProtocolSchemaInvalid, wire::SCHEMA_MESSAGE)
                .await;
            return Flow::Continue;
        };
        let trust = handshake::peer_trust(&view, &device_id);
        let request = ChallengeRequest {
            kind: ConnectionKind::SyncDevice,
            peer: PeerIdentity::Device(device_id.clone()),
            binding: handshake::connection_binding(&origin),
            client_nonce: client_nonce.clone(),
            negotiated_features: handshake::transcript_features(&selected),
            // Sync 侧没有 catalog 概念：`catalog_revision` 必须为 `None`（`identity-auth` 的 hello 只对
            // Node Link 分支要求它）。
            catalog_revision: None,
        };
        let issue = match self.conn.authority.hello(&request, &trust).await {
            Ok(issue) => issue,
            Err(error) => {
                // 绑定/信任类别/keystore 三类失败对端一律看到同一个 proof_invalid（不泄露差异）；
                // 细节只进结构化日志。来源不一致已在上面单独映射为 `auth.origin_mismatch`。
                warn!(
                    event = "sync.challenge_failed",
                    error = %error,
                    "the sync challenge could not be issued"
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
        let Some(body) = challenge_body(&issue, &self.conn.authority, &selected) else {
            return self.internal_fault(envelope).await;
        };
        if self
            .write_pre_auth(MessageType::AuthServerChallenge, &body)
            .await
            .is_err()
        {
            debug!(
                event = "sync.challenge_not_sent",
                "the connection ended before the challenge was sent"
            );
            return Flow::Finished;
        }
        info!(
            event = "sync.challenge_issued",
            device_id = device_id.as_str(),
            known = trust.public_key.is_some(),
            features = selected.len(),
            "the sync handshake issued a challenge"
        );
        self.handshake.challenge = Some(issue);
        self.handshake.client_nonce = Some(client_nonce);
        self.handshake.view = Some(view);
        self.handshake.device = Some(device_id);
        // 协商结果在**这里**落定（挑战已按它签发）：认证成功后原样交接给路由（§9.4/§9.6 的门控输入）。
        self.handshake.features = selected;
        Flow::Continue
    }

    /// `auth.client_proof`：验签 → 凭据状态 → 写集提交 → 注册连接 → `auth.authenticated`。
    async fn on_proof(&mut self, envelope: &Envelope) -> Flow {
        let (Some(challenge), Some(device_id), Some(view), Some(client_nonce)) = (
            self.handshake.challenge.clone(),
            self.handshake.device.clone(),
            self.handshake.view.clone(),
            self.handshake.client_nonce.clone(),
        ) else {
            // 由 `pre_auth_allowed` 保证不可达：不在「等 proof」阶段时 `auth.client_proof` 不在认证前
            // 白名单里。保留这条分支只为不 panic（也能挡住未来把状态机改回去时的悬空 proof）。
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
        let Some(proof) = decode_body::<ClientProof>(envelope) else {
            self.reject(ErrorCode::ProtocolSchemaInvalid, wire::SCHEMA_MESSAGE)
                .await;
            return Flow::Continue;
        };
        // proof body 里的 `deviceId` 必须与 hello 自报的同一个：transcript 用的是本机记住的 device id，
        // 不一致说明对端把它当成了别的身份（失败关闭，不透露差异）。
        if proof.device_id.as_str() != device_id.as_str() {
            warn!(
                event = "sync.proof_device_mismatch",
                "the proof declares another device than the client hello"
            );
            self.refuse_with_details(
                close::UNAUTHENTICATED,
                ErrorCode::AuthProofInvalid,
                "the proof is not valid",
                RawObject::empty(),
                Some(envelope.message_id().clone()),
            )
            .await;
            return Flow::Finished;
        }
        let trust = handshake::peer_trust(&view, &device_id);
        let Some(submission) = ProofInput {
            device: &device_id,
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
        let authenticated = match self.conn.authority.verify_proof(&submission, &trust) {
            Ok(authenticated) => authenticated,
            Err(failure) => {
                // 对端可见的映射统一为 `auth.proof_invalid`（§8.5）：未知设备（没有可用公钥）与签名
                // 无效因此不可区分。失败原因只进结构化日志；`failure.audit()` 给出的 `device.auth_failed`
                // 目前没有可用的 core 写面（见模块文档）。
                warn!(
                    event = "sync.proof_invalid",
                    device_id = device_id.as_str(),
                    reason = ?failure.reason,
                    "the sync proof was rejected"
                );
                self.refuse_with_details(
                    close::UNAUTHENTICATED,
                    ErrorCode::AuthProofInvalid,
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
            CredentialStatus::Revoked => Some((close::REVOKED, ErrorCode::AuthDeviceRevoked)),
            // 无信任行（`None`）与尚未批准的 `pending` 行都归 `Unknown`：与未知设备同一处置，
            // 不区分存在性（§8.5）。
            CredentialStatus::Unknown => {
                Some((close::UNAUTHENTICATED, ErrorCode::AuthProofInvalid))
            }
            // scope 缩减要求重新认证；设备侧 scopes 由信任行当次读出、不由本状态机下发，因此这里不可达
            // （保留分支只为失败关闭，不 panic）。
            CredentialStatus::ScopeReduced => {
                Some((close::STATE_CONFLICT, ErrorCode::AuthProofInvalid))
            }
        };
        if let Some((close_code, code)) = refuse_credential {
            warn!(
                event = "sync.credential_rejected",
                device_id = device_id.as_str(),
                credential = ?credential,
                "the device credential does not allow a connection"
            );
            self.refuse_with_details(
                close_code,
                code,
                "the device is not trusted by this host",
                RawObject::empty(),
                Some(envelope.message_id().clone()),
            )
            .await;
            return Flow::Finished;
        }

        // 认证收尾：写集必须先落库，再发 `auth.authenticated`（design D3）。
        let pending = handshake::pending_pairing(&view).map(|record| record.id().clone());
        let at = self.conn.authority.now();
        let completion =
            self.conn
                .authority
                .complete_auth(authenticated.fact, pending.as_ref(), &at);
        if let Some(pairing) = completion.consume_pairing {
            let scopes = view
                .device
                .as_ref()
                .map(|row| row.scopes().clone())
                .unwrap_or_default();
            let actor = Actor::Device {
                device: device_id.clone(),
                scopes,
            };
            if let Err(error) = self.conn.core.consume_pairing(&actor, &pairing).await {
                // 写集没提交成功就绝不进入业务阶段（不半认证）。
                warn!(
                    event = "sync.consume_pairing_failed",
                    pairing_id = pairing.as_str(),
                    error = ?error,
                    "the pairing consumption write set failed; the connection is closed"
                );
                return self.internal_fault(envelope).await;
            }
            info!(
                event = "sync.pairing_consumed",
                device_id = device_id.as_str(),
                pairing_id = pairing.as_str(),
                "the approved pairing was consumed by the first successful authentication"
            );
        }
        // `owned_device.last_seen_at` 的唯一生产者是本入口（`consume_pairing` 的写集只碰对端节点行），
        // 因此**每次**认证成功都要经它推进，包括有待消费配对的首次认证。
        if let Err(error) = self.conn.core.record_device_connected(&device_id).await {
            warn!(
                event = "sync.connected_write_failed",
                device_id = device_id.as_str(),
                error = ?error,
                "the authentication wrap-up write set could not be written; the connection is closed"
            );
            return self.internal_fault(envelope).await;
        }

        let Some(body) = authenticated_body(&view, &device_id, self.limits) else {
            warn!(
                event = "sync.authenticated_body_failed",
                "the auth.authenticated body cannot be assembled"
            );
            return self.internal_fault(envelope).await;
        };
        let Some(connection_id) = challenge_id_to_uuid(&challenge) else {
            return self.internal_fault(envelope).await;
        };
        let (handle, receiver) = ConnectionHandle::new(
            connection_id.clone(),
            self.conn.authority.local_node().clone(),
            device_id.clone(),
            self.peer.client_ip,
            self.limits,
            Arc::clone(self.conn.core.ids()),
        );
        // 先入队 `auth.authenticated`，成功后才登记并顶替旧连接：登记即意味着设备槽位易主，
        // 而下发失败的新连接不该把设备原有的连接赶走。
        if let Err(fault) = handle.send(MessageType::AuthAuthenticated, &body) {
            warn!(
                event = "sync.authenticated_not_sent",
                device_id = device_id.as_str(),
                fault = ?fault,
                "the auth.authenticated message could not be queued"
            );
            self.close(close::UNAVAILABLE, "the host cannot serve this connection")
                .await;
            return Flow::Finished;
        }
        if let Some(displaced) = self.conn.registry.register(Arc::clone(&handle)) {
            // §8.5：同一 `(hostId, deviceId)` 只保留一条 active 连接。旧连接只被**请求**关闭
            // （先排空已入队消息再发 close 帧），设备本身不撤销、它的授权也不变。
            info!(
                event = "sync.connection_replaced",
                device_id = device_id.as_str(),
                replaced = displaced.connection_id().as_str(),
                "a new connection replaced the previous one for this device"
            );
            displaced.request_close(close::REPLACED, "a new connection replaced this one");
        }
        // 连接上下文交接（认证成功、已登记之后）：把本次协商出的 feature 交给路由，供 §9.4 的目录资源
        // 与 §9.6 的已导入会话门控使用。放在登记之后是因为路由的状态表以注册表为连接活跃判据。
        if let Some(route) = self.conn.route.as_ref() {
            route.on_authenticated(&handle, &self.handshake.features);
        }
        info!(
            event = "sync.authenticated",
            device_id = device_id.as_str(),
            connection_id = connection_id.as_str(),
            "a sync connection completed authentication"
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
        // 发 `control.ping` 的节奏取会话窗口（生产 = §13 的固定 30 秒；下发对端的
        // `auth.authenticated.heartbeatIntervalMs` 永远是同一个常量，见 `SessionWindows`）。
        let heartbeat = self.conn.windows.heartbeat;
        let windows = self.conn.windows;
        let mut next_ping = Instant::now() + heartbeat;
        let mut last_ping: Option<Base64Url<16>> = None;
        // 看门狗 tick 的两条要求：**固定节拍**（deadline 跨轮次保留，不在每轮重建，否则持续的出站
        // 流量会把它一直推后）与**优先于出站投递**（`biased` 里排在 `outbound` 之前）。两者合起来保证
        // 心跳、静默超时与慢消费者看门狗在连接繁忙时也按固定节奏被检查——否则「队列一直是高水位」这条
        // 判据永远等不到检查它的时刻，慢消费者断开就成了死代码。
        let mut next_tick = TokioInstant::now() + SESSION_TICK;
        let ending = loop {
            // 关闭请求（撤销传播、单设备替换等）：先排空已入队消息，再发 close 帧。
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
                let tick = tokio::time::sleep_until(next_tick);
                tokio::select! {
                    biased;
                    () = requested => Event::CloseRequested,
                    () = shutdown.wait() => Event::Shutdown,
                    () = tick => Event::Tick,
                    Some(outbound) = outbound => Event::Outbound(outbound),
                    frame = connection.recv() => Event::Frame(frame),
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
                    None => break Ending::Close(close::NORMAL, "the device closed the connection"),
                    Some(Err(WsError::MessageTooLarge)) => {
                        // 传输层已按 §14 发过 1009 close 帧，这里不再发第二个：直接放弃连接。
                        break Ending::TransportClosed(
                            "a message exceeded the connection limit (1009)",
                        );
                    }
                    Some(Err(WsError::Transport(detail))) => {
                        debug!(event = "sync.transport_closed", detail = %detail, "the connection ended");
                        break Ending::TransportClosed("the connection ended");
                    }
                    Some(Ok(WsMessage::Binary(_))) => {
                        warn!(
                            event = "sync.binary_frame",
                            "a binary frame is not allowed on sync"
                        );
                        // §3.1：binary 帧必须关闭连接；`error` 仍尽力送出，帧内容不进协议逻辑。
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
                    next_tick = TokioInstant::now() + SESSION_TICK;
                    let now = Instant::now();
                    if now.duration_since(self.last_inbound) >= windows.silence {
                        warn!(
                            event = "sync.heartbeat_timeout",
                            device_id = handle.device_id().as_str(),
                            silence_seconds = windows.silence.as_secs(),
                            "no inbound message within the heartbeat window"
                        );
                        break Ending::Close(close::TIMEOUT, "heartbeat timeout");
                    }
                    if let Some(saturated) = handle.saturated_for() {
                        if saturated >= windows.slow_consumer {
                            warn!(
                                event = "sync.slow_consumer",
                                device_id = handle.device_id().as_str(),
                                pending_messages = handle.pending().messages,
                                "the connection stayed above the queue high water mark"
                            );
                            // §14：先发 `resource.backpressure` 再断开；客户端据此提示并按 cursor 重放。
                            self.refuse_post(
                                ErrorCode::ResourceBackpressure,
                                "the connection cannot consume messages fast enough",
                                RawObject::empty(),
                                &handle,
                            )
                            .await;
                            break Ending::Close(
                                close::NORMAL,
                                "the connection cannot consume messages fast enough",
                            );
                        }
                    } else if now >= next_ping {
                        let Some(body) = ping_body() else {
                            break Ending::Close(close::UNAVAILABLE, "entropy unavailable");
                        };
                        match heartbeat_outcome(handle.send(MessageType::ControlPing, &body)) {
                            HeartbeatOutcome::Queued => {
                                last_ping = Some(body.nonce.clone());
                                next_ping = now + heartbeat;
                            }
                            HeartbeatOutcome::Deferred => {
                                // `next_ping` 不推进：下一次 tick 会重试这条心跳；也**不**登记这条没发出去
                                // 的 nonce（否则对端之后回填它会命中 `sync.pong_mismatch`）。
                                warn!(
                                    event = "sync.ping_deferred",
                                    pending_messages = handle.pending().messages,
                                    "the heartbeat ping was deferred: the outbound queue is at its high water mark"
                                );
                            }
                            HeartbeatOutcome::Ended => {
                                warn!(
                                    event = "sync.ping_not_sent",
                                    "the heartbeat ping could not be queued"
                                );
                                break Ending::Close(close::NORMAL, "the connection ended");
                            }
                        }
                    }
                }
            }
        };
        let (close_code, close_reason) = match &ending {
            Ending::Close(code, reason) => (*code, *reason),
            Ending::TransportClosed(reason) => (0, *reason),
        };
        debug!(
            event = "sync.connection_closing",
            device_id = handle.device_id().as_str(),
            connection_id = handle.connection_id().as_str(),
            close_code,
            reason = close_reason,
            "the sync connection is closing"
        );
        // 先把已入队的消息写完（撤销推送必须先送达），再发 close 帧。
        self.drain(&mut receiver).await;
        self.conn.registry.unregister(&handle);
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
        // 序号先记账：`judge` 只在「信封合法且序号正好是下一个」时给出 `next`（§4.1 的序号属于对端发出的
        // 消息，body/type 是否被接受不影响它），因此被拒的消息（未知 type、未实现的 type、body 校验失败）
        // 也必须让本方向的期望序号前进，否则一次拒绝就让连接之后全部错位。
        if let Some(next) = next {
            self.inbound_next = next;
        }
        let envelope = match judged {
            Inbound::Message(envelope) => envelope,
            Inbound::Rejected { code, message } => {
                // 消息级协议错误：消息不生效、连接保持可用（只有致命的协议/连接错误才关闭）。
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
            // §13：`control.pong` 必须原样回填 nonce；对不上只记日志（协议未规定关闭，而任何有效入站
            // 消息都算连接活跃，因此不作 liveness 判定）。
            MessageType::ControlPong => {
                match decode_body::<ControlBody>(&envelope) {
                    Some(body) => {
                        if last_ping.as_ref() != Some(&body.nonce) {
                            debug!(
                                event = "sync.pong_mismatch",
                                "the control.pong nonce does not match the outstanding ping"
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
            MessageType::ControlPing => {
                let Some(body) = decode_body::<ControlBody>(&envelope) else {
                    self.reject_post(
                        ErrorCode::ProtocolSchemaInvalid,
                        wire::SCHEMA_MESSAGE,
                        Some(envelope.message_id().clone()),
                        handle,
                    )
                    .await;
                    return None;
                };
                // 原样回填 nonce（方向相反、形状相同）。
                let pong = ControlBody { nonce: body.nonce };
                match heartbeat_outcome(handle.send(MessageType::ControlPong, &pong)) {
                    HeartbeatOutcome::Queued => {}
                    HeartbeatOutcome::Deferred => {
                        // 一级背压：这一次回填直接丢掉（对端已经用它的入站 ping 证明过连接活着，§13 的
                        // liveness 判据看的是入站消息），但绝不因此以 1000 关连接。
                        warn!(
                            event = "sync.pong_deferred",
                            "the control.pong reply was dropped: the outbound queue is at its high water mark"
                        );
                    }
                    HeartbeatOutcome::Ended => {
                        warn!(
                            event = "sync.pong_not_sent",
                            "the control.pong reply could not be queued"
                        );
                        return Some((close::NORMAL, "the connection ended"));
                    }
                }
                None
            }
            MessageType::Error => {
                debug!(
                    event = "sync.peer_error",
                    message_id = envelope.message_id().as_str(),
                    "the client reported a connection error"
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
                    // WP4 注册路由后这条分支不再命中。
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

    /// 认证前的消息（`auth.server_challenge`/`error`）：信封必须省略连接字段（§4.1）。
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

    /// 消息级拒绝（握手阶段）：回 `error`，连接保持可用。
    async fn reject(&mut self, code: ErrorCode, message: &'static str) {
        let Some(body) = wire::error_body(code, message, RawObject::empty(), None) else {
            warn!(
                event = "sync.error_body_failed",
                code = code.as_str(),
                "the error body cannot be assembled"
            );
            return;
        };
        debug!(
            event = "sync.message_rejected",
            code = code.as_str(),
            "the message was rejected without closing the connection"
        );
        let _ = self.write_pre_auth(MessageType::Error, &body).await;
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
                event = "sync.error_body_failed",
                code = code.as_str(),
                "the error body cannot be assembled"
            );
            return;
        };
        debug!(
            event = "sync.message_rejected",
            code = code.as_str(),
            "the message was rejected without closing the connection"
        );
        let _ = handle.send(MessageType::Error, &body);
    }

    /// 致命拒绝（握手阶段）：回 `error`（尽力）后按 close code 关闭。
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
            event = "sync.connection_refused",
            code = code.as_str(),
            close_code,
            "the sync connection is being closed with a protocol error"
        );
        if let Some(body) = wire::error_body(code, message, details, correlation) {
            let _ = self.write_pre_auth(MessageType::Error, &body).await;
        }
        self.close(close_code, message).await;
    }

    /// 致命拒绝（认证后）：把 `error` 入队，由会话在关闭前**排空**（绝不直接关闭连接，否则错误根本
    /// 送不出去）；调用方随后以对应 close code 结束会话。
    async fn refuse_post(
        &mut self,
        code: ErrorCode,
        message: &'static str,
        details: RawObject,
        handle: &Arc<ConnectionHandle>,
    ) {
        debug!(
            event = "sync.connection_refused",
            code = code.as_str(),
            "the sync connection is being closed with a protocol error"
        );
        if let Some(body) = wire::error_body(code, message, details, None) {
            let _ = handle.send(MessageType::Error, &body);
        }
    }

    /// 内部不可用：回 `error`（internal.unavailable）并以 4500 关闭。
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

/// 心跳类消息投递失败的处置（§14 的两级背压口径）。
///
/// 本机发出的心跳有两处：定时器的 `control.ping`，以及对端 `control.ping` 的回填 `control.pong`。两者
/// 都是**心跳类**控制消息，撞上队列高水位时的处置必须一致：
///
/// - 高水位是**一级背压**（§14：「待发送队列达到高水位后先停止读取新的 replay batch」），因此不是致命
///   错误——把对端「暂时读得慢」当致命错误直接以 1000 断开，等于跳过一级处置直接执行二级；
/// - 二级处置由慢消费者看门狗承担（`saturated_for` 超过宽限才发 `resource.backpressure` 并断开）。
///
/// `Closed`/`Encoding` 不是背压：连接已结束或本机装配不出信封，会话按调用方给出的 close 结束。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeartbeatOutcome {
    /// 已入队。
    Queued,
    /// 高水位：本次未入队，但会话继续（不是错误）。
    Deferred,
    /// 结束会话。
    Ended,
}

/// [`HeartbeatOutcome`] 的判定表。
pub(crate) fn heartbeat_outcome(result: Result<(), SendFault>) -> HeartbeatOutcome {
    match result {
        Ok(()) => HeartbeatOutcome::Queued,
        Err(SendFault::HighWater) => HeartbeatOutcome::Deferred,
        Err(SendFault::Closed | SendFault::Encoding) => HeartbeatOutcome::Ended,
    }
}

/// 协商失败的错误映射（§5.1/§5.2）。
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

/// 把升级请求里已取的来源解析成 canonical origin（§3.2）。
///
/// `None` 覆盖两种同性质的拒绝：没有 `Origin` 头（`Absent`，§8.5 的已知偏差：v1 的设备信任绑定在
/// canonical origin 上，缺少来源即无可比较的绑定），以及取值不是 canonical origin（`CanonicalOrigin`
/// 只接受 `https://` 前缀，因此接入层如实产出的 `http://…` 也落到这里）。两者对端可见的映射都是
/// `auth.origin_mismatch`。
fn resolve_origin(origin: &RequestOrigin) -> Option<CanonicalOrigin> {
    let text = origin.as_str()?;
    CanonicalOrigin::parse(text).ok()
}

/// `auth.server_challenge` 的 body（§8.2）。
fn challenge_body(
    issue: &ChallengeIssue,
    authority: &Authority,
    selected: &[String],
) -> Option<ServerChallenge> {
    Some(ServerChallenge {
        selected_protocol_version: UIntAtLeast::<1>::new(handshake::PROTOCOL_VERSION).ok()?,
        host_id: Uuid::parse(authority.local_node().as_str()).ok()?,
        connection_id: Uuid::parse(issue.challenge_id.as_str()).ok()?,
        server_nonce: Base64Url::<32>::parse(issue.server_nonce.as_str()).ok()?,
        selected_features: handshake::wire_features(selected)?,
        host_proof: Base64Url::<64>::parse(&issue.host_proof.to_base64url()).ok()?,
    })
}

/// `auth.authenticated` 的 body（§8.4）。
///
/// `scopes` 取**持久化**设备行（与授权判定同源，不由对端自报）；`serverEpoch`/`headGlobalSequence` 取
/// 本机水位（与 `sync.subscribe` 的 cursor 判定同源）；`limits` 是协商结果。
fn authenticated_body(
    view: &SyncDeviceHandshakeView,
    device: &DeviceId,
    limits: SessionLimits,
) -> Option<AuthenticatedBody> {
    let scopes = view.device.as_ref()?.scopes();
    let heartbeat = limits.heartbeat_interval().as_millis();
    Some(AuthenticatedBody {
        device_id: Uuid::parse(device.as_str()).ok()?,
        scopes: wire_scopes(scopes)?,
        server_epoch: Uuid::parse(view.head.server_epoch.as_str()).ok()?,
        head_global_sequence: DecimalString::parse(&view.head.global_sequence.to_string()).ok()?,
        heartbeat_interval_ms: BoundedU64::<1_000, 300_000>::new(heartbeat as u64).ok()?,
        limits: limits.to_wire()?,
    })
}

/// `auth.authenticated.scopes` 的 wire 形状。
///
/// `Scopes` 只提供反序列化这一条构造路径（它自己承担去重与非空文本校验），因此这里经一次 JSON
/// 往返：失败即本机接线错误（例如 scope 名超过 128 字符），调用方按内部不可用失败关闭。
fn wire_scopes(scopes: &acp_core::model::ScopeSet) -> Option<Scopes> {
    let text = serde_json::to_string(&scopes.iter().collect::<Vec<&str>>()).ok()?;
    serde_json::from_str(&text).ok()
}

/// 挑战标识（wire 的 `connectionId`）的 UUID 视图。
fn challenge_id_to_uuid(issue: &ChallengeIssue) -> Option<Uuid> {
    Uuid::parse(issue.challenge_id.as_str()).ok()
}

/// `auth.client_proof` 到 `identity-auth` 输入的装配（回显字段来自本机签发的挑战，不取对端自报值）。
struct ProofInput<'a> {
    device: &'a DeviceId,
    server_nonce: &'a Nonce,
    client_nonce: &'a Nonce,
    proof: &'a ClientProof,
}

impl ProofInput<'_> {
    fn to_submission(&self) -> Option<ProofSubmission> {
        Some(ProofSubmission {
            kind: ConnectionKind::SyncDevice,
            challenge_id: ChallengeId::parse(self.proof.connection_id.as_str()).ok()?,
            server_nonce: self.server_nonce.clone(),
            peer: PeerIdentity::Device(self.device.clone()),
            client_nonce: self.client_nonce.clone(),
            signature: P1363Signature::try_from_bytes(self.proof.device_proof.as_bytes()).ok()?,
        })
    }
}

/// 一条 `control.ping` 的 body（16 字节随机 nonce，§13）。
///
/// 每次调用都新生成（`uuid::Uuid::new_v4` 的 122 位随机位），因此 v1 的同一条连接上不会复用 nonce。
fn ping_body() -> Option<ControlBody> {
    use base64::Engine as _;
    let bytes = uuid::Uuid::new_v4().into_bytes();
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    Some(ControlBody {
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

/// 单节点并发 WSS 连接配额（§14：256）。
///
/// 计数覆盖**已建立**的连接（含尚未认证的）；凭证随会话存续、会话结束（任何路径）即归还。超限的连接一律
/// 显式拒绝：不排队（那会把上限变成延迟）、不静默丢弃（对端会以为连上了）、也不挤掉既有连接（既有连接
/// 可能正是某台合法设备的那一条，§8.5）。
///
/// **口径边界**：§14 的这一行是「单节点」的 WSS 连接总量，而本层只看得见 Sync 端点（`/sync/v1`）——
/// Node Link 端点的连接由它自己的边界计数（`server::sync` 不得调用 `server::node_link`，`AGENTS.md` §5）。
/// 因此这里是「本节点 Sync 端点的并发连接数」；要不要把这条上限上移到接入层、让两个端点共用一个计数器，
/// 属组合根接线，留给主 Agent 复核（见交付报告）。
struct ConnectionSlots {
    in_use: AtomicUsize,
    cap: usize,
}

impl ConnectionSlots {
    fn new(cap: usize) -> Self {
        Self {
            in_use: AtomicUsize::new(0),
            cap,
        }
    }

    /// 当前占用的配额数（测试观测口，见 [`SyncConn::concurrent_connections`]）。
    #[cfg(test)]
    fn occupied(&self) -> usize {
        self.in_use.load(Ordering::SeqCst)
    }

    /// 占用一个配额；已满返回 `None`（调用方显式拒绝该连接）。
    fn acquire(self: &Arc<Self>) -> Option<ConnectionSlot> {
        let mut current = self.in_use.load(Ordering::SeqCst);
        loop {
            if current >= self.cap {
                return None;
            }
            match self.in_use.compare_exchange(
                current,
                current + 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => {
                    return Some(ConnectionSlot {
                        slots: Arc::clone(self),
                    });
                }
                Err(actual) => current = actual,
            }
        }
    }
}

/// 一条会话持有的并发连接凭证：会话结束（任何路径）即归还。
struct ConnectionSlot {
    slots: Arc<ConnectionSlots>,
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        self.slots.in_use.fetch_sub(1, Ordering::SeqCst);
    }
}

/// 单 IP 的在途握手配额。
///
/// 接入层的 TLS 握手池对每个连接都生效，但没有任何按源 IP 的准入闸门；本层因此限制「同一源地址上
/// 尚未完成认证的连接数」（§14 的固定上限 5）。取值即协议上限，不留额外余量。
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

    /// 当前被占用的配额数（源地址求和；测试观测口，见 [`SyncConn::in_flight_handshakes`]）。
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
