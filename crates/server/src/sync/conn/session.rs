//! 一条 WSS 连接的会话驱动：握手状态机、信封/序号、心跳与每连接发送队列
//! （`docs/SYNC_PROTOCOL.md` §3/§4/§8/§12/§13/§14，`design.md` D1/D2/D4/D5）。
//!
//! 一条连接 = 一个 [`Session`]（读半边 + 判据）+ 一个写任务（写半边）：准入、握手与业务阶段都在会话
//! 任务里推进，写半边则交给一个由会话**拥有**的写任务（[`WriterTask`]）。写任务存在的唯一理由是让判据
//! 不被写路径拖住——见下节「写路径与写任务」。取消路径因此有两条：会话自身由 `peer.shutdown`、对端
//! 关闭、致命协议错误或关闭请求结束；写任务由会话的关闭请求结束，超预算仍未结束则被取消（`abort`）
//! ——它由 [`Session`] 持有并关闭，绝不是 detached 任务。
//!
//! 阶段划分（`auth.client_hello → auth.server_challenge → auth.client_proof → auth.authenticated`）：
//!
//! 1. **准入**：认证前先过三道路径无关的闸——单节点并发 WSS 连接上限 256（§14，最靠外的一道：它
//!    限制节点总量，不管来源）、单 IP 认证尝试 100 次/分钟（§14，键是接入层给出的真实 `client_ip`）与
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
//!
//! **写路径与写任务**：判据要真的有机会执行，光有 tick 的调度口径还不够——写路径本身也必须**不会**把
//! 会话任务卡住。因此连接在 [`Session`] 构造时就经 [`WsConnection::split`] 拆成读/写半边
//! （`transport/net/ws.rs`）：读半边与判据留在会话任务里，写半边交给每连接一个写任务（[`WriterTask`]，
//! `tokio::spawn` 出来的任务其所有者就是 [`Session`]）。写任务串行消费一个命令通道（帧 / close），
//! 同一时刻至多一帧在写、帧序由单写者保证；出站帧由 `Event::Outbound` 交给它，会话**不再** await
//! socket 写。于是对端一个字节都不读、socket 缓冲被填满时：写任务停在它那一帧上，而会话的 `select!`
//! 照常轮询读半边与 tick——判据在最需要它的时候仍然成立。本模块的**写路径**（拆半、每连接写任务、
//! 写期限、关闭顺序）与 `node_link/conn/session.rs` 的同构实现**逐条一致**（`design.md` D6）。
//!
//! **每帧写期限**：写任务对每一帧用与慢消费者看门狗**同一个窗口**（`SLOW_CONSUMER_GRACE`）的
//! `timeout`。到期即该连接的最终处置：半截帧不可恢复，因此不重试、不续写，**也不再发任何后续帧**
//! （含 close 帧——在一个可能已经残缺的数据帧之后再发帧是畸形帧），记一条
//! `event = "sync.write_deadline"` 后直接丢弃写半边并结束写任务。会话侧据此以 `Ending::TransportClosed`
//! 收尾（不再发 close 帧）。这只是兜底：窗口内看门狗/静默判定仍会先给出分级处置。
//!
//! **关闭顺序**：关闭 = 有界的三步。①会话按 `CLOSE_DRAIN_BUDGET` 把已入队消息交给写任务（撤销推送
//! 必须先送达）；②发关闭请求（带 close code/reason），写任务按 FIFO 写完已入队的帧后**在预算内**尽力
//! 发 close 帧并结束；③会话有界地 join 写任务（`CLOSE_DRAIN_BUDGET`），超时则放弃等待并 `abort` 它
//! ——写半边随任务一起被丢弃，socket 随之关闭。取消路径不依赖某个特定的关闭顺序：命令发送端被丢弃
//! 时，写任务写完已入队的帧后同样会结束。
//!
//! **与 `node_link` 的刻意差异（本变更不动）**：业务阶段 Tick 分支的饱和期心跳口径仍是
//! `if saturated { … } else if now >= next_ping { … }`——饱和期间本机会连自己的 `control.ping` 也不发
//! （只对撞上高水位的投递做推迟）。`node_link` 的规格要求饱和期间心跳照发，因此它在这一点上**有意**
//! 比本模块严格；两侧口径若需对齐，属另一个变更的决策（`design.md` 的 Non-Goals）。本变更只镜像
//! 「写路径有期限」这一件事，不改这条差异。

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
    Authenticated as AuthenticatedBody, ClientHello, ClientProof, ServerChallenge,
};
use sync_protocol::common::{Base64Url, BoundedU64, DecimalString, RawObject, UIntAtLeast, Uuid};
use sync_protocol::control::Body as ControlBody;
use sync_protocol::envelope::{Envelope, MessageType, Phase, encode_body};
use sync_protocol::error::ErrorCode;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
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
use crate::transport::net::ws::{MESSAGE_TOO_LARGE_CLOSE, WsReader, WsWriter};
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
    /// 单 IP 的新认证尝试限流（§14：100 次/分钟，键是 `PeerInfo::client_ip`）。
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

    /// 注册认证后消息的路由（WP4 的组合根接线）。
    pub fn with_route(mut self, route: Arc<dyn MessageRoute>) -> Self {
        self.route = Some(route);
        self
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
    /// 连接读半边：帧级读与判据都靠它，因此它留在会话任务里（写半边在 [`WriterTask`] 里）。
    /// `WsReader::recv` 不自发 close 帧，会话持有 `Option` 只为在收尾时「丢弃即关闭」。
    reader: Option<WsReader>,
    /// 每连接写任务（写半边 + 所有者凭证，见模块文档「写路径与写任务」）。
    writer: Option<WriterTask>,
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
        // 读写半边在此拆开：写半边交给写任务，会话只留读半边与判据（模块文档「写路径与写任务」）。
        let (reader, writer) = connection.split();
        let writer = WriterTask::spawn(writer, conn.windows.slow_consumer);
        Self {
            conn,
            reader: Some(reader),
            writer: Some(writer),
            peer,
            origin,
            limits,
            handshake: HandshakeState::default(),
            inbound_next: 1,
            last_inbound: Instant::now(),
        }
    }

    async fn run(mut self) {
        self.serve().await;
        // 写任务由本会话持有：无论会话从哪条路径结束，都在这里按预算有界地收尾（拥有关闭顺序与取消
        // 路径，见模块文档「关闭顺序」）。
        self.finish_writer().await;
    }

    /// 会话主体（准入 → 握手 → 业务）；结束方式由各阶段自己决定，收尾统一交给 [`Session::run`]。
    async fn serve(&mut self) {
        let client_ip = self.peer.client_ip;
        // ⓪ 单节点并发 WSS 连接上限（§14：256）。它排在两张按来源的闸门之前：节点级资源比来源级配额
        // 更靠外，而这条拒绝与「认证尝试」无关（连接根本没进认证，不该消耗那 100 次/分钟的额度）。
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
        // ① 认证尝试限流（§14：100 次/分钟/IP，键是接入层给出的真实对端地址）。
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
            let Some(reader) = self.reader.as_mut() else {
                return Flow::Finished;
            };
            let Some(writer) = self.writer.as_mut() else {
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
                end = &mut writer.done => {
                    // 写任务已结束（写期限到期或写半边报错）：连接不再可用，会话随之结束。原因由写任务
                    // 自己记日志，这里只登记「会话是被这条路结束的」。
                    debug!(event = "sync.write_task_ended", reason = ?end, "the write task ended during the handshake");
                    return Flow::Finished;
                }
                frame = reader.recv() => {
                    match frame {
                        None => return Flow::Finished,
                        Some(Err(WsError::MessageTooLarge)) => {
                            // 读半边只判定超限（它没有写能力），1009 的关闭帧经写任务发出。
                            let (code, reason) = MESSAGE_TOO_LARGE_CLOSE;
                            self.close(code, reason).await;
                            return Flow::Finished;
                        }
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
            // scope 缩减要求重新认证：设备方向已没有授权集合可缩减（`pairing-model` 删除设备权限层），
            // 因此设备侧不可能产生该状态；保留分支只为让判定穷尽并失败关闭，不 panic。
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
            let actor = Actor::Device {
                device: device_id.clone(),
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
        // 判据永远等不到检查它的时刻，慢消费者断开就成了死代码。写路径已不在这条 `select!` 里（模块
        // 文档「写路径与写任务」）：出站帧只是投给写任务，会话因此不会被写卡住。
        let mut next_tick = TokioInstant::now() + SESSION_TICK;
        // 写任务从这里开始记账：`auth.authenticated` 已入队，账本句柄必须先于它到达写任务（同一条命令
        // 通道，FIFO 保证顺序）。
        self.attach_writer(&handle);
        let ending = loop {
            // 关闭请求（撤销传播、单设备替换等）：先排空已入队消息，再发 close 帧。
            if let Some((code, reason)) = handle.close_request() {
                break Ending::Close(code, reason);
            }
            let event = {
                let shutdown = self.peer.shutdown.clone();
                let Some(reader) = self.reader.as_mut() else {
                    break Ending::Close(close::UNAVAILABLE, "the connection is already closed");
                };
                let Some(writer) = self.writer.as_mut() else {
                    break Ending::Close(close::UNAVAILABLE, "the write task already ended");
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
                    end = &mut writer.done => Event::WriterEnded(end.ok()),
                    frame = reader.recv() => Event::Frame(frame),
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
                    // 帧交给写任务（不再 await socket 写）：写卡住只影响写任务，判据照常被轮询。
                    if !self.hand_outbound(outbound) {
                        break Ending::TransportClosed("the write task ended");
                    }
                }
                Event::WriterEnded(end) => {
                    debug!(
                        event = "sync.write_task_ended",
                        reason = ?end,
                        "the write task ended while the session was running"
                    );
                    // 写任务不是被本会话的关闭请求结束的（那条路径只在收尾时走，且会话已退出循环）：
                    // 写期限到期时它已经丢弃写半边，因此这里只结束会话、不再发任何帧。
                    break Ending::TransportClosed(match end {
                        Some(WriteEnd::Deadline) => "the write path exceeded its deadline",
                        _ => "the connection ended",
                    });
                }
                Event::Frame(frame) => match frame {
                    None => break Ending::Close(close::NORMAL, "the device closed the connection"),
                    Some(Err(WsError::MessageTooLarge)) => {
                        // §14：读半边只判定超限（它没有写能力），1009 的关闭帧经写任务发出——它排在
                        // 已交给写任务的帧之后（FIFO），因此这里只入队、不再发第二个 close 帧。
                        let (code, reason) = MESSAGE_TOO_LARGE_CLOSE;
                        self.close(code, reason).await;
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
        // 关闭顺序（模块文档「关闭顺序」）：先按预算把已入队消息交给写任务（撤销推送必须先送达），再发
        // close 命令；写任务由 `Session::run` 按预算有界 join。
        match ending {
            Ending::Close(code, reason) => {
                self.drain(&mut receiver).await;
                self.conn.registry.unregister(&handle);
                self.close(code, reason).await;
            }
            Ending::TransportClosed(_) => {
                // 这条分支只表示「本层不再自己发 close 帧」，两种来路要分开看：
                //
                // - 1009 超限：close 帧（`WriteCommand::Close(1009, …)`）在 break **之前**已由
                //   [`Session::close`] 入队，写任务会先写完**已交给它的**帧、再在 join 预算内尽力把这一帧
                //   发出去（FIFO，排在已入队帧之后）。socket 因此要等写任务结束，耗时 ≤ `CLOSE_DRAIN_BUDGET`；
                // - 流已终止（读半边报错/对端结束）与写期限到期：写半边已结束或已不可信，**不再发任何 close
                //   帧**，排空也没有意义（写半边已丢弃）。读半边在这里被丢弃，socket 随之关闭。
                self.conn.registry.unregister(&handle);
                self.reader.take();
            }
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

    /// 把一帧交给写任务（连接/写任务已结束时返回错误，不 panic）。
    ///
    /// 命令通道是无界的：投递**不会**阻塞会话——写卡住只体现在写任务那一侧（模块文档「写路径与写
    /// 任务」）。
    async fn write_now(&mut self, text: &str) -> Result<(), SendFault> {
        match self.writer.as_ref() {
            Some(writer) => writer
                .commands
                .send(WriteCommand::Frame(text.to_owned()))
                .map_err(|_| SendFault::Closed),
            None => Err(SendFault::Closed),
        }
    }

    /// 把一条已入队的出站帧交给写任务（写任务写出后递减队列账本）。
    ///
    /// 返回 `false` 表示写任务已结束（它的命令接收端被丢弃）。
    fn hand_outbound(&self, outbound: Outbound) -> bool {
        match self.writer.as_ref() {
            Some(writer) => writer
                .commands
                .send(WriteCommand::Outbound(outbound))
                .is_ok(),
            None => false,
        }
    }

    /// 把队列账本句柄交给写任务（认证后调用一次；必须先于任何 `Outbound` 命令到达）。
    fn attach_writer(&self, handle: &Arc<ConnectionHandle>) {
        if let Some(writer) = self.writer.as_ref() {
            let _ = writer
                .commands
                .send(WriteCommand::Attach(Arc::clone(handle)));
        }
    }

    /// 向写任务发出关闭请求（重复调用不会改变目标：第一条到达的关闭请求即结束写任务）。
    ///
    /// 写任务按 FIFO 写完已入队的帧后**在预算内**尽力发 close 帧并结束；本方法不等待（有界 join 在
    /// [`Session::finish_writer`]）。
    async fn close(&mut self, code: u16, reason: &'static str) {
        if let Some(writer) = self.writer.as_ref() {
            let _ = writer.commands.send(WriteCommand::Close { code, reason });
        }
    }

    /// 有界地收尾写任务：先在预算内 join（写任务收到关闭请求后会尽力发 close 帧并结束），超时则放弃
    /// 等待并 `abort` 它——写半边随任务一起被丢弃，socket 随之关闭（取消路径，绝不留 detached 任务）。
    async fn finish_writer(&mut self) {
        let Some(mut writer) = self.writer.take() else {
            return;
        };
        // 先丢掉命令发送端：即使关闭路径没来得及发关闭请求，写任务写完已入队的帧后也会因通道关闭而
        // 结束（取消路径不依赖某个特定的关闭顺序）。
        drop(writer.commands);
        let joined = tokio::time::timeout(CLOSE_DRAIN_BUDGET, &mut writer.handle).await;
        if joined.is_err() {
            warn!(
                event = "sync.write_task_abandoned",
                "the write task did not end within the close drain budget; it is cancelled"
            );
            writer.handle.abort();
        }
    }

    /// 排空已入队消息（预算内尽力而为）：把它们交给写任务（写任务按 FIFO 写出），对端/写任务已结束
    /// 时立即结束。
    async fn drain(&mut self, receiver: &mut mpsc::Receiver<Outbound>) {
        let deadline = TokioInstant::now() + CLOSE_DRAIN_BUDGET;
        loop {
            match tokio::time::timeout_at(deadline, receiver.recv()).await {
                Ok(Some(outbound)) => {
                    if !self.hand_outbound(outbound) {
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

impl Drop for Session {
    /// 会话被取消时连带取消写任务：所有者消失即取消是**必要条件**——会话 future 未必跑到
    /// [`Session::run`] 的 [`Session::finish_writer`]（`listener.rs` 的 `abort`、以及 panic 展开都会在
    /// `run` 完成前丢掉会话 future），而 `tokio::task::JoinHandle` 一旦被 drop 就 **detach**，写任务会
    /// 带着一个已经没有所有者的连接继续跑下去（违反 `AGENTS.md` §8「异步任务必须有所有者、不能遗留
    /// detached task」）。
    ///
    /// 正常路径下 [`Session::finish_writer`] 已经 `take` 过 `writer`（无论走完有界 join 还是超时
    /// `abort`），因此这里拿到的通常是 `None`，是 no-op——不改变既有的有界 join 行为。
    fn drop(&mut self) {
        if let Some(writer) = self.writer.take() {
            writer.handle.abort();
        }
    }
}

/// `select!` 的事件源（把借用收在一处，避免在 `select!` 里调用 `&mut self` 方法）。
enum Event {
    CloseRequested,
    Shutdown,
    Outbound(Outbound),
    /// 写任务已结束（`None` = 任务没来得及报告就结束了）。
    WriterEnded(Option<WriteEnd>),
    Frame(Option<Result<WsMessage, WsError>>),
    Tick,
}

/// 会话的结束方式：本层发 close 帧，或传输层已经关闭（1009 超限/流终止/写期限到期）——后者不得再发
/// close 帧。
enum Ending {
    /// 本层以该 close code 与原因关闭。
    Close(u16, &'static str),
    /// 传输层（或写任务）已关闭连接（不重复发 close 帧）。
    TransportClosed(&'static str),
}

/// 写任务的一条命令（同一时刻至多一帧在写、帧序由单写者保证：命令通道是 FIFO 的）。
enum WriteCommand {
    /// 握手阶段的帧（不含队列账本）。
    Frame(String),
    /// 认证后从待发送队列取出的帧：写任务在写出前记账（[`ConnectionHandle::consumed`]）。
    Outbound(Outbound),
    /// 队列账本句柄（认证完成时投递一次，**先于**任何 [`WriteCommand::Outbound`] 到达）。
    Attach(Arc<ConnectionHandle>),
    /// 关闭请求：写任务写完已入队的帧后**在预算内**尽力发 close 帧并结束。
    Close { code: u16, reason: &'static str },
}

/// 写任务的结束原因（会话据此决定收尾方式）。
#[derive(Debug, Clone, Copy)]
enum WriteEnd {
    /// 已按关闭请求发出 close 帧（或对端早已断开）后正常结束。
    Closed,
    /// 某一帧超过写期限：整条连接的最终处置——写半边已被丢弃，不再重试、也不再有后续帧（含 close）。
    Deadline,
    /// 写半边报错（对端已断开等）。
    TransportClosed,
    /// 命令通道被关闭，写任务在没有关闭请求的情况下结束（会话侧的取消路径）。
    Abandoned,
}

/// 每连接的写任务：拥有写半边，串行消费命令通道。
///
/// 所有者是 [`Session`]（[`Session::finish_writer`] 有界 join，超时 `abort`）；取消路径有两条——关闭
/// 请求命令，或命令发送端被丢弃。写任务绝不 detach（`AGENTS.md` §8）。
struct WriterTask {
    /// 命令通道（无界：投递永远不会阻塞会话，积压由队列账本的水位判定体现）。
    commands: mpsc::UnboundedSender<WriteCommand>,
    /// 结束原因；写任务结束时必然被填充（`Err` = 任务没报告就结束了）。
    done: oneshot::Receiver<WriteEnd>,
    /// 所有者凭证（join/abort 用）。
    handle: JoinHandle<()>,
}

impl WriterTask {
    /// 起一个写任务（每连接一个）。`frame_deadline` 是每帧的写期限（§14 的慢消费者宽限，见模块文档
    /// 「每帧写期限」）。
    fn spawn(writer: WsWriter, frame_deadline: Duration) -> Self {
        let (commands, receiver) = mpsc::unbounded_channel();
        let (done_sender, done) = oneshot::channel();
        let handle = tokio::spawn(async move {
            let end = write_loop(writer, receiver, frame_deadline).await;
            let _ = done_sender.send(end);
        });
        Self {
            commands,
            done,
            handle,
        }
    }
}

/// 写任务主体：串行消费命令通道（帧 / 关闭请求），见 [`WriterTask`] 与模块文档的「写路径与写任务」。
async fn write_loop(
    mut writer: WsWriter,
    mut commands: mpsc::UnboundedReceiver<WriteCommand>,
    frame_deadline: Duration,
) -> WriteEnd {
    // 队列账本句柄：`Attach` 到达前不记账（认证前的帧本来就不在队列里）。
    let mut handle: Option<Arc<ConnectionHandle>> = None;
    while let Some(command) = commands.recv().await {
        match command {
            WriteCommand::Attach(attached) => handle = Some(attached),
            WriteCommand::Frame(text) => {
                if let Some(end) =
                    write_frame(&mut writer, text, frame_deadline, handle.as_ref()).await
                {
                    return end;
                }
            }
            WriteCommand::Outbound(outbound) => {
                // 记账口径与改造前一致：会话（现在由写任务代劳）**取出即记账**，不等写出完成。
                if let Some(attached) = handle.as_ref() {
                    attached.consumed(&outbound);
                }
                if let Some(end) =
                    write_frame(&mut writer, outbound.text, frame_deadline, handle.as_ref()).await
                {
                    return end;
                }
            }
            WriteCommand::Close { code, reason } => {
                // 尽力发 close 帧：预算内没发出去（对端不读）也直接放弃——连接随写半边一起关闭。
                let _ = tokio::time::timeout(CLOSE_DRAIN_BUDGET, writer.close(code, reason)).await;
                return WriteEnd::Closed;
            }
        }
    }
    WriteEnd::Abandoned
}

/// 写一帧（带写期限）。返回 `Some(end)` 表示写任务应就此结束。
///
/// 期限到期是**最终处置**：半截帧不可恢复，因此不重试、不续写，也**不再发任何后续帧**（含 close 帧
/// ——在可能残缺的数据帧之后再发帧是畸形帧）；写半边随写任务结束被丢弃。
async fn write_frame(
    writer: &mut WsWriter,
    text: String,
    frame_deadline: Duration,
    handle: Option<&Arc<ConnectionHandle>>,
) -> Option<WriteEnd> {
    let started = Instant::now();
    match tokio::time::timeout(frame_deadline, writer.send_text(text)).await {
        Ok(Ok(())) => None,
        Ok(Err(error)) => {
            debug!(
                event = "sync.transport_closed",
                detail = %error,
                "the connection ended while writing a frame"
            );
            Some(WriteEnd::TransportClosed)
        }
        Err(_) => {
            let connection_id = match handle {
                Some(handle) => handle.connection_id().as_str(),
                None => "unattached",
            };
            warn!(
                event = "sync.write_deadline",
                connection_id = connection_id,
                waited_ms = started.elapsed().as_millis() as u64,
                deadline_ms = frame_deadline.as_millis() as u64,
                reason = "the frame could not be flushed within the slow-consumer grace",
                "the write path exceeded its deadline; the connection is closed"
            );
            Some(WriteEnd::Deadline)
        }
    }
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
/// 只写**已验证事实**与本次连接的参数：`serverEpoch`/`headGlobalSequence` 取本机水位（与
/// `sync.subscribe` 的 cursor 判定同源）、`limits` 是协商结果。设备方向**没有授权维度**（`pairing-model`
/// 删除设备权限层），因此这里不计算、也不填充任何授权集合字段——通过认证即具备执行业务命令的前提。
fn authenticated_body(
    view: &SyncDeviceHandshakeView,
    device: &DeviceId,
    limits: SessionLimits,
) -> Option<AuthenticatedBody> {
    let heartbeat = limits.heartbeat_interval().as_millis();
    Some(AuthenticatedBody {
        device_id: Uuid::parse(device.as_str()).ok()?,
        server_epoch: Uuid::parse(view.head.server_epoch.as_str()).ok()?,
        head_global_sequence: DecimalString::parse(&view.head.global_sequence.to_string()).ok()?,
        heartbeat_interval_ms: BoundedU64::<1_000, 300_000>::new(heartbeat as u64).ok()?,
        limits: limits.to_wire()?,
    })
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
