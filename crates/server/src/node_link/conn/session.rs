//! 一条 WSS 连接的会话驱动：握手状态机、信封/序号、心跳与每连接发送队列
//! （`docs/NODE_LINK_PROTOCOL.md` §2/§12.2，`design.md` D2/D3/D9）。
//!
//! 一条连接 = 一个 [`Session`]（读半边 + 判据）+ 一个写任务（写半边）：握手与业务阶段都在会话任务里
//! 推进，写半边则交给一个由会话**拥有**的写任务（[`WriterTask`]）。写任务存在的唯一理由是让判据不被
//! 写路径拖住——见下节「写路径与写任务」。取消路径因此有两条：会话自身由 `peer.shutdown`、对端关闭、
//! 致命协议错误或关闭请求结束；写任务由会话的关闭请求结束，超预算仍未结束则被取消（`abort`）——它由
//! [`Session`] 持有并关闭，绝不是 detached 任务。
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
//! **看门狗 tick 的调度口径**：业务阶段的 `select!` 用 `biased` 固定分支优先级，tick 分支**排在出站
//! 投递之前**，且它的 deadline（`next_tick`）**跨轮次保留**（`sleep_until`，不在每轮循环里重建）。
//! 两条缺一不可：只把 tick 提前而每轮重建 deadline，持续的出站流量会让它「每轮被唤醒都重新睡满一个
//! 周期」，永远凑不满一个周期；只保留 deadline 而把它排在出站之后，只要出站队列每轮都就绪，它连被
//! 轮询的机会都没有。合起来才保证心跳（`link.ping`）、90 秒静默判定与慢消费者看门狗在连接繁忙时仍
//! 按固定节拍被检查——否则「待发送队列持续处于高水位」这条判据永远等不到检查它的时刻，慢消费者断开
//! 就成了死代码。`sync/conn/session.rs` 的同构实现是同一口径。
//!
//! **饱和（慢消费者）与保活的口径**：每连接待发送队列的水位判定完全归 [`ConnectionHandle`]
//! （进入用高水位、退出用低水位迟滞，见 `conn::registry` 的模块文档），会话只消费它的两个结论：
//! [`ConnectionHandle::saturated_for`] 给出**当前**饱和段已持续的时长，`send` 给出 [`SendFault`]。
//! 因此 Tick 分支里心跳与看门狗必须各自独立判定——饱和**不是**静默：
//!
//! - 饱和期间 `link.ping` 仍按协商的 `heartbeatIntervalMs` 发出（心跳是唯一「没有业务消息也要产生
//!   入站 pong」的来源，停发它会让对端也不再产生入站消息，最终撞上 90 秒静默关闭）；
//! - ping 因队列顶满而被拒（[`SendFault::HighWater`]）不算「心跳停摆」：这一轮不发出、下个 tick 重试，
//!   生死由看门狗按 `SLOW_CONSUMER_GRACE` 判定（瞬时突发因此有机会自行排空）；
//! - 看门狗只看**当前**饱和段的时长：曾被拒过一次但随后把队列排到低水位之下的连接会清掉该计时，
//!   不会被判死。
//!
//! 上面两组口径缺一不可：水位判定（本节）保证判据的语义，tick 的调度口径（上一节）保证判据真的被按
//! 节拍轮询到。「持续饱和 → 断开」因此在真实连接上可达：队列持续顶在高水位、出站分支每轮都就绪时，
//! 看门狗仍在宽限后断开——用例
//! `a_permanently_saturated_queue_still_reaches_the_slow_consumer_watchdog` 在真实连接上锁定了它。
//!
//! **写路径与写任务**：判据要真的有机会执行，光有 tick 的调度口径还不够——写路径本身也必须**不会**把
//! 会话任务卡住。因此连接在 [`Session`] 构造时就经 [`WsConnection::split`] 拆成读/写半边
//! （`transport/net/ws.rs`）：读半边与判据留在会话任务里，写半边交给每连接一个写任务（[`WriterTask`]，
//! `tokio::spawn` 出来的任务其所有者就是 [`Session`]）。写任务串行消费一个命令通道（帧 / close），
//! 同一时刻至多一帧在写、帧序由单写者保证；出站帧由 `Event::Outbound` 交给它，会话**不再** await
//! socket 写。于是对端一个字节都不读、socket 缓冲被填满时：写任务停在它那一帧上，而会话的 `select!`
//! 照常轮询读半边与 tick——判据在最需要它的时候仍然成立。
//!
//! **每帧写期限**：写任务对每一帧用与慢消费者看门狗**同一个窗口**（`SLOW_CONSUMER_GRACE`）的
//! `timeout`。到期即该连接的最终处置：半截帧不可恢复，因此不重试、不续写，**也不再发任何后续帧**
//! （含 close 帧——在一个可能已经残缺的数据帧之后再发帧是畸形帧），记一条
//! `event = "node_link.write_deadline"` 后直接丢弃写半边并结束写任务。会话侧据此以
//! `Ending::TransportClosed` 收尾（不再发 close 帧）。这只是兜底：窗口内看门狗/静默判定仍会先给出
//! 分级处置。
//!
//! **关闭顺序**：关闭 = 有界的三步。①会话按 `CLOSE_DRAIN_BUDGET` 把已入队消息交给写任务（撤销推送
//! 必须先送达）；②发关闭请求（带 close code/reason），写任务按 FIFO 写完已入队的帧后**在预算内**尽力
//! 发 close 帧并结束；③会话有界地 join 写任务（`CLOSE_DRAIN_BUDGET`），超时则放弃等待并 `abort` 它
//! ——写半边随任务一起被丢弃，socket 随之关闭。取消路径不依赖某个特定的关闭顺序：命令发送端被丢弃
//! 时，写任务写完已入队的帧后同样会结束。
//!
//! 关闭请求因此**不会**插队到「正在推进的帧」之前：它按 FIFO 排在已交给写任务的帧之后，撤销推送才
//! 能真的先送达。写卡住时的有界性由第③步保证——`abort` 是比「放弃那一帧」更强的放弃：之后不再发任何
//! 帧（与写期限到期同一口径，避免在一个可能残缺的数据帧之后再发帧）。
//!
//! 另注：`crate::sync::conn::session` 的同一处分支仍是 `} else if now >= next_ping {`——饱和期间它连自己
//! `control.ping` 也不发（只对撞上高水位的投递做推迟）。本模块的规格明确要求饱和期间心跳照发，因此
//! `node_link` 在此**有意**比 sync 严格；两侧口径若需对齐，属另一个变更的决策。
//!
//! **认证限流的接线口径**：接入层（`transport::net`）在 WS upgrade 之前没有每路径的准入钩子
//! （WP2 冻结形状只提供 `WsHandler`），因此本层把闸门放在**会话的第一步**：任何帧（包括
//! `node.hello`）都不再被处理，连接以 4429 关闭。效果等价——未认证的连接无法产生任何业务副作用。
//!
//! **catalogRevision 的口径**：`node.challenge.catalogRevision` 与 `node.ready.catalogRevision` 同源，
//! 都取本机**管理写集水位**（`AuditStore::watermark()`）：Export 与信任写集各自追加一行审计，因此它
//! 正是这三个值要表达的「目录修订号」，且跨重启与保留期清理单调（`store.head()` 会随裁剪回退，已弃
//! 用——design D4 的实现期结论，见 WP4/WP5 handoff）。握手两侧必须同源：该值进入
//! `node-link-challenge/v1`/`node-link-proof/v1` 的 tag 6，Access 用 `node.challenge` 给的值签 proof，
//! Owner 用签发挑战时的同一个值验签。`catalog.snapshot.revision` 与本值同源（WP5 的
//! [`crate::node_link::catalog`]）。

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
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::Instant as TokioInstant;
use tracing::{debug, info, warn};

use crate::node_link::conn::handshake::{self, NegotiationFault};
use crate::node_link::conn::limits::{NodeLinkConfig, SessionLimits};
use crate::node_link::conn::registry::{ConnectionHandle, ConnectionRegistry, Outbound, SendFault};
use crate::node_link::conn::wire::{self, Inbound};
use crate::node_link::conn::{
    AUTH_ATTEMPTS_PER_MINUTE, AUTH_WINDOW, HANDSHAKE_TIMEOUT, MAX_TRACKED_IPS, close,
};
use crate::transport::net::ws::{MESSAGE_TOO_LARGE_CLOSE, WsReader, WsWriter};
use crate::transport::net::{
    PeerInfo, RateLimit, RequestOrigin, SlidingWindowLimiter, WsConnection, WsError, WsHandler,
    WsMessage,
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

/// 一个 Tick 的动作（§2.5 的静默判定之外，剩下的两个判定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TickAction {
    /// 当前饱和段已超过宽限：断开（`SLOW_CONSUMER_GRACE`）。
    Disconnect,
    /// 该发心跳（`now >= next_ping`）。
    Ping,
    /// 什么都不做。
    Idle,
}

/// Tick 的动作判定：慢消费者看门狗与心跳是**并列**的两件事，不是二选一。
///
/// 写成 `if saturated { … } else if due { ping }` 会让饱和分支一旦命中就把心跳变成永久不可达：饱和不是
/// 静默，对端收不到 `link.ping` 也就不会回 `link.pong`，本机最终以「连续 90 秒无入站消息」关掉一条其实
/// 只是暂时落后的连接。只有「当前饱和段已超过宽限」才短路心跳——那一刻连接马上要被断开，发不发 ping
/// 已无意义（且断开优先于发心跳）。
///
/// 提取成纯函数是为了让这条不变量可被用例直接锁定（会话 Tick 里只剩分派；对照 `sync::conn::session` 的
/// 同构处理）。
pub(crate) fn tick_action(
    saturated_for: Option<std::time::Duration>,
    slow_consumer: Duration,
    ping_due: bool,
) -> TickAction {
    if let Some(saturated) = saturated_for
        && saturated >= slow_consumer
    {
        return TickAction::Disconnect;
    }
    if ping_due {
        TickAction::Ping
    } else {
        TickAction::Idle
    }
}

/// 一次心跳投递的结果（§2.5 的两级处置，与 `sync::conn::session::heartbeat_outcome` 同口径）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeartbeatOutcome {
    /// 已入队：登记 nonce 并推进下一个心跳周期。
    Queued,
    /// 队列顶满：推迟这一次（不推进周期、不登记 nonce），交给慢消费者看门狗按宽限做二级处置。
    Deferred,
    /// 连接已结束或本机装不出信封：不是背压，按调用方给出的 close 结束会话。
    Ended,
}

/// 心跳投递结果的分类：**一级处置是推迟**，把对端「暂时读得慢」当致命错误直接断开等于跳过一级处置。
///
/// 二级处置（断开）由慢消费者看门狗承担（[`TickAction::Disconnect`]，判据是 `saturated_for` 超过宽限）。
/// 高水位这一条因此绝不能映射成 [`HeartbeatOutcome::Ended`]。
pub(crate) fn heartbeat_outcome(result: Result<(), SendFault>) -> HeartbeatOutcome {
    match result {
        Ok(()) => HeartbeatOutcome::Queued,
        Err(SendFault::HighWater) => HeartbeatOutcome::Deferred,
        Err(SendFault::Closed | SendFault::Encoding) => HeartbeatOutcome::Ended,
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

    /// 注册认证后消息的路由（WP5/WP6 的组合根接线）。
    pub fn with_route(mut self, route: Arc<dyn MessageRoute>) -> Self {
        self.route = Some(route);
        self
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
    /// Node Link 不依赖 HTTP 来源：`node.hello` 自报的 endpoint 与本机持久记录决定身份，绑定比较在
    /// `identity-auth` 里完成（`NODE_LINK_PROTOCOL.md` §2/§12.2）。因此这里**如实接收即忽略**该参数，
    /// 不因它的有无改变任何行为（接入层保证传入值已按 `SYNC_PROTOCOL.md` §3.2 归一化或为「无来源」）。
    async fn handle(
        self: Arc<Self>,
        connection: WsConnection,
        peer: PeerInfo,
        _origin: RequestOrigin,
    ) {
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
    /// 连接读半边：帧级读与判据都靠它，因此它留在会话任务里（写半边在 [`WriterTask`] 里）。
    /// `WsReader::recv` 不自发 close 帧，会话持有 `Option` 只为在收尾时「丢弃即关闭」。
    reader: Option<WsReader>,
    /// 每连接写任务（写半边 + 所有者凭证，见模块文档「写路径与写任务」）。
    writer: Option<WriterTask>,
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
        // 读写半边在此拆开：写半边交给写任务，会话只留读半边与判据（模块文档「写路径与写任务」）。
        let (reader, writer) = connection.split();
        let writer = WriterTask::spawn(writer, conn.windows.slow_consumer);
        Self {
            conn,
            reader: Some(reader),
            writer: Some(writer),
            peer,
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

    /// 会话主体（握手 → 业务）；结束方式由各阶段自己决定，收尾统一交给 [`Session::run`]。
    async fn serve(&mut self) {
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
        // ② 在途握手配额（取舍结论：便宜就做）。
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
            let Some(reader) = self.reader.as_mut() else {
                return Flow::Finished;
            };
            let Some(writer) = self.writer.as_mut() else {
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
                end = &mut writer.done => {
                    // 写任务已结束（写期限到期或写半边报错）：连接不再可用，会话随之结束。原因由写任务
                    // 自己记日志，这里只登记「会话是被这条路结束的」。
                    debug!(event = "node_link.write_task_ended", reason = ?end, "the write task ended during the handshake");
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
        let catalog_revision = view.catalog_revision;
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
        // 看门狗 tick 的两条要求（见模块文档「看门狗 tick 的调度口径」）：**固定节拍**（deadline
        // 跨轮次保留，不在每轮重建，否则持续的出站流量会让它「每轮被唤醒都重新睡满一个周期」，
        // 永远凑不满一个周期）与**优先于出站投递**（`biased` 里排在 `outbound` 之前，否则只要出站
        // 队列每轮都就绪，它连被轮询的机会都没有）。两者合起来才保证心跳、静默超时与慢消费者
        // 看门狗在连接繁忙时仍按固定节奏被检查。写路径已不在这条 `select!` 里（模块文档「写路径与
        // 写任务」）：出站帧只是投给写任务，会话因此不会被写卡住。
        let mut next_tick = TokioInstant::now() + SESSION_TICK;
        // 写任务从这里开始记账：`node.ready` 已入队，账本句柄必须先于它到达写任务（同一条命令通道，
        // FIFO 保证顺序）。
        self.attach_writer(&handle);
        let ending = loop {
            // 关闭请求（撤销传播等）：先排空已入队消息，再发 close 帧。
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
                        event = "node_link.write_task_ended",
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
                    None => {
                        break Ending::Close(
                            close::NORMAL,
                            "the access node closed the connection",
                        );
                    }
                    Some(Err(WsError::MessageTooLarge)) => {
                        // §2.5：读半边只判定超限（它没有写能力），1009 的关闭帧经写任务发出——它排在
                        // 已交给写任务的帧之后（FIFO），因此这里只入队、不再发第二个 close 帧。
                        let (code, reason) = MESSAGE_TOO_LARGE_CLOSE;
                        self.close(code, reason).await;
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
                    // 下一拍的 deadline 从**本次 tick 的时刻**起算（而不是从这轮 select 构造起算）：
                    // 这样即便这一拍被别的就绪分支挤晚了一点，节拍也不会漂移。
                    next_tick = TokioInstant::now() + SESSION_TICK;
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
                    match tick_action(
                        handle.saturated_for(),
                        windows.slow_consumer,
                        now >= next_ping,
                    ) {
                        TickAction::Disconnect => {
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
                        TickAction::Ping => {
                            let Some(body) = ping_body() else {
                                break Ending::Close(close::UNAVAILABLE, "entropy unavailable");
                            };
                            match heartbeat_outcome(handle.send(MessageType::LinkPing, &body)) {
                                HeartbeatOutcome::Queued => {
                                    last_ping = Some(body.nonce);
                                    next_ping = now + heartbeat;
                                }
                                // 队列此刻仍然顶满：这一轮心跳挤不进去。这不是「心跳停摆」，也不该由心跳
                                // 决定生死——持续过慢由上面的看门狗按宽限判定。不推进 `next_ping`：下一个
                                // tick 立刻重试，队列一腾出位置就补发；也不登记这条没发出去的 nonce（否则
                                // 对端之后回填它会命中 `pong_mismatch`）。
                                HeartbeatOutcome::Deferred => warn!(
                                    event = "node_link.ping_deferred",
                                    connection_id = handle.connection_id().as_str(),
                                    pending_messages = handle.pending().messages,
                                    "the heartbeat ping was deferred: the outbound queue is at its high water mark"
                                ),
                                HeartbeatOutcome::Ended => {
                                    warn!(
                                        event = "node_link.ping_not_sent",
                                        "the heartbeat ping could not be queued"
                                    );
                                    break Ending::Close(
                                        close::NORMAL,
                                        "the connection cannot consume messages fast enough",
                                    );
                                }
                            }
                        }
                        TickAction::Idle => {}
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
        // 关闭顺序（模块文档「关闭顺序」）：先按预算把已入队消息交给写任务（撤销推送必须先送达），再发
        // close 命令；写任务由 `Session::run` 按预算有界 join。
        match ending {
            Ending::Close(code, reason) => {
                self.drain(&mut receiver).await;
                self.conn
                    .registry
                    .unregister(handle.connection_id().as_str());
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
                self.conn
                    .registry
                    .unregister(handle.connection_id().as_str());
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
                event = "node_link.write_task_abandoned",
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
    /// 起一个写任务（每连接一个）。`frame_deadline` 是每帧的写期限（§2.5 的慢消费者宽限，见模块文档
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
                event = "node_link.transport_closed",
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
                event = "node_link.write_deadline",
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
        catalog_revision: DecimalString::parse(&view.catalog_revision.to_string()).ok()?,
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
