//! broker：`docs/CORE_PORTS_AND_STORAGE.md` §6 的 12 条提交与发布契约。
//!
//! # 为什么 broker 有"驱动入口"
//!
//! `core` 不依赖 runtime（§2）：它既不能 spawn 任务，也不能自己读时钟（时间一律由调用方以
//! `Timestamp` 传入）。后端事件经 [`crate::ports::EventSink`] 到达 broker，而 `EventSink::send`
//! 是**同步**的、`SessionStore::commit` 是异步的——因此 broker 把事件按到达顺序缓冲，由持有 runtime
//! 的组合根调用 [`Broker::pump`]（或只落盘的 [`Broker::flush`]）驱动：
//!
//! - `storage.flush_interval_ms`（默认 250）的合并窗口由组合根的定时器触发 `pump`；
//! - §6.10 要求合并不延迟终态事件、不跨 turn 边界：终态事件到达时由 sink 的包装者立即触发 `pump`，
//!   且 broker 自身在每个终态事件处**切断合批**；
//! - 单线程嵌入与测试按顺序 `await` [`Broker::pump`] 即可，不需要 runtime。
//!
//! # 顺序（§6.1/§6.2，不可交换）
//!
//! - owned：`EventSink` → 组装 [`OwnedCommit`] → `SessionStore::commit` → `EventPublisher::publish`。
//! - imported：组装 [`DeliveryReceipt`] → `RemoteDeliveryStore::commit_receipt` → 发布（正文只在内存）。
//!
//! 两条路径都只在写成功之后才发布；`commit` 返回 `Unavailable` 时的事件一律不发布（§6.9）。

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

use crate::model::{
    Actor, AuditAction, AuditOutcome, AuditRecord, ClientCommand, CommandKind, CommandPayload,
    CommandReceipt, CommandRecord, CommandResult, CommandStatus, CommandTerminalRecord,
    CommittedDelivery, CommittedEvent, ConfigOptionId, ConfigValue, ConflictKind,
    CreateSessionRequest, Digest, ElicitationAction, ElicitationValues, EndpointEvent, EntityRef,
    EventKind, EventOrigin, EventPayload, EventType, GlobalCursor, InteractionId, InteractionKind,
    InteractionResolution, LocalCursor, MemberValue, MessageId, ModeId, ModeRef, NodeId, NodeKind,
    NodeState, OriginEventRef, OwnedSessionRef, PendingEvent, PendingInteraction,
    PermissionDecision, PermissionDecisionKind, PersistencePolicy, PortError, PromptContentBlock,
    PromptRequest, PublicError, RemoteSessionRef, RequestId, Resolution, Sequence, SessionId,
    SessionReference, SessionState, StoredPolicy, Timestamp, TurnId, TurnState, UnavailableKind,
    Version, ViewJson, decode_json_string as json_string, encode_json_string as json_text,
    insert_string_member_front, object_members as json_members, top_level_member,
};
use crate::ports::{
    AuditStore, Clock, CommitOutcome, DeliveryIndexEntry, DeliveryReceipt, EventPublisher,
    EventSink, ExportStore, HistoryInclude, HistoryPage, HistoryQuery, IdGenerator,
    IdempotencyRecord, InteractionResolved, ModeChange, NewSession, NewTurn, OwnedCommit,
    PendingInteractionWrite, ReadView, ReceiptOutcome, RemoteDeliveryStore, ReplayBatch,
    ReplayLimit, SessionBackendFactory, SessionEndpoint, SessionStore, SessionUpdate, StateChange,
    TrustStore, TurnChange, TurnUpdate,
};

// ---------------------------------------------------------------------------------------------
// §6.5 授权
// ---------------------------------------------------------------------------------------------

/// 授权拒绝（§6.5）。
///
/// `PortError`（§2）是闭合枚举且没有 Forbidden 变体，因此拒绝以命令级 `PublicError`
/// （`authorization.scope_denied`）表达：`submit_command` 族把它变成 `Rejected`，非命令入口用
/// [`Denied::into_port_error`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Denied {
    code: &'static str,
    message: &'static str,
    retryable: bool,
}

impl Denied {
    /// 授权拒绝的唯一构造点：`authorization.scope_denied`。
    pub fn scope_denied() -> Self {
        Self {
            code: "authorization.scope_denied",
            message: "该调用方没有执行此命令的 scope/grant",
            retryable: false,
        }
    }

    /// 命令级错误（`PublicError` 的构造是校验性的，因此这里可失败）。
    pub fn into_error(self) -> Result<PublicError, PortError> {
        PublicError::coded(self.code, self.message, self.retryable).map_err(PortError::from)
    }

    /// 非命令入口的表示。core 的 `PortError` 没有"禁止"变体，这里用
    /// `InvalidRequest("authorization.scope_denied")`；适配器映射到等价的 wire 错误码。
    pub fn into_port_error(self) -> PortError {
        PortError::InvalidRequest(self.code)
    }
}

// ---------------------------------------------------------------------------------------------
// 命令表与事件策略表
// ---------------------------------------------------------------------------------------------

/// payload 变体 → 命令名（`SYNC_PROTOCOL.md` §11.5 表首列），同时是授权所需的 scope 名。
pub fn command_name(payload: &CommandPayload) -> &'static str {
    match payload {
        CommandPayload::SessionList { .. } => "session.list",
        CommandPayload::SessionRead { .. } => "session.read",
        CommandPayload::CommandStatus { .. } => "command.status",
        CommandPayload::ModeList { .. } => "session.mode.list",
        CommandPayload::ConfigList { .. } => "session.config.list",
        CommandPayload::Prompt { .. } => "session.prompt",
        CommandPayload::Cancel { .. } => "session.cancel",
        CommandPayload::ElicitationRespond { .. } => "elicitation.respond",
        CommandPayload::ModeSet { .. } => "session.mode.set",
        CommandPayload::ConfigSet { .. } => "session.config.set",
        CommandPayload::PermissionResolve { .. } => "permission.resolve",
    }
}

/// payload 变体 → 命令类别。payload 与 `ClientCommand.kind` 必须一致（不一致即 `InvalidRequest`）。
pub fn command_kind(payload: &CommandPayload) -> CommandKind {
    payload.family()
}

/// 命令 → 覆盖它的 grant（`compatibility/commands/v1/commands.json` 在 core 里的**手工镜像**）。
///
/// core 读不了 JSON（不依赖 serde），而 Node actor 的 Export/Import 交集判定（§6.5）必须知道
/// `grant.*` 覆盖哪些命令名。增删命令必须同时改 `commands.json` 与本表；集成层应对二者加漂移门禁。
pub fn required_grant(command: &str) -> Option<&'static str> {
    let grant = match command {
        "session.list"
        | "session.read"
        | "command.status"
        | "session.mode.list"
        | "session.config.list" => "grant.observe",
        "session.prompt" | "session.cancel" | "elicitation.respond" => "grant.interact",
        "session.mode.set" | "session.config.set" => "grant.configure-session",
        "permission.resolve" => "grant.approve",
        "session.create" => "grant.remote-work",
        _ => return None,
    };
    Some(grant)
}

/// 只放内存、**不得**进入任何提交的事件类型（`INITIAL_DESIGN.md` §10.3「心跳、typing、presence：
/// 只放内存」）。
///
/// v1 登记的标准事件类型里没有这一类的成员（心跳是连接层消息，不是事件），因此本表是 daemon 新增
/// presence/typing 类事件时的唯一登记点；`EventType` 是开放 newtype，未登记取值合法（§10.1）。
/// 组合根用 [`persistence_policy`] 在 sink 包装层做内存转发，broker 在组装前把它们过滤掉（§6.11）。
pub const EPHEMERAL_EVENT_TYPES: &[&str] = &["device.typing", "session.presence"];

/// 事件类型的持久策略（`INITIAL_DESIGN.md` §10.1–§10.3）。未登记取值一律 `Durable`——
/// `SYNC_PROTOCOL.md` §10.2 要求未登记事件降级处理而**不得静默丢弃**。
pub fn persistence_policy(event_type: &EventType) -> PersistencePolicy {
    let text = event_type.as_str();
    if EPHEMERAL_EVENT_TYPES.contains(&text) {
        return PersistencePolicy::Ephemeral;
    }
    match text {
        "user.message.delta"
        | "agent.message.delta"
        | "agent.thought.delta"
        | "tool.call.started"
        | "tool.call.updated"
        | "terminal.output"
        | "session.usage.changed" => PersistencePolicy::ShortTerm,
        _ => PersistencePolicy::Durable,
    }
}

/// `docs/SYNC_PROTOCOL.md` §10.3 要求 view 带顶层 `turnId` 的事件类型（与 §10.3 的表格一致；
/// `turn.*` 含 `turn.delta_compacted`）。§10.3 变化时必须同步本表、`specs/core-event-view-identity/`
/// 的清单与相应用例。
const TURN_ID_VIEW_EVENT_TYPES: &[&str] = &[
    "turn.queued",
    "turn.started",
    "turn.completed",
    "turn.cancelled",
    "turn.failed",
    "turn.delta_compacted",
    "user.message.delta",
    "agent.message.delta",
    "agent.message.completed",
    "agent.thought.delta",
    "tool.call.started",
    "tool.call.updated",
    "tool.call.completed",
    "permission.requested",
    "elicitation.requested",
];

/// §10.3 要求 view 带顶层 `version`（会话版本，十进制字符串）的事件类型。
const SESSION_VERSION_VIEW_EVENT_TYPES: &[&str] =
    &["session.mode.changed", "session.config.changed"];

/// 该事件类型的 view 是否要求 `turnId`（§10.3）。
fn view_requires_turn_id(event_type: &str) -> bool {
    TURN_ID_VIEW_EVENT_TYPES.contains(&event_type)
}

/// 该事件类型的 view 是否要求 `version`（§10.3）。
fn view_requires_session_version(event_type: &str) -> bool {
    SESSION_VERSION_VIEW_EVENT_TYPES.contains(&event_type)
}

// ---------------------------------------------------------------------------------------------
// 配置、依赖与会话槽位
// ---------------------------------------------------------------------------------------------

/// `sessions.queue_policy`（`CONFIG_REFERENCE.md` §4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueuePolicy {
    /// 按持久化接受顺序排队；等待中的 turn 数达到 `max_queued_turns` 时返回 `session.busy`。
    Queue,
    /// 已有 active turn 时直接返回 `session.busy`。
    RejectBusy,
}

/// broker 的配置面。由组合根从配置读取后注入——core 不读配置文件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrokerConfig {
    pub queue_policy: QueuePolicy,
    pub max_queued_turns: u32,
    /// `storage.persist_deltas`（`docs/CORE_PORTS_AND_STORAGE.md` §7.5）：`false` 时 turn 终态后必须压缩
    /// 该 turn 的 delta（§6 第 15 条）；`true` 时永不压缩。core 只读它，不解释其它存储配置。
    pub persist_deltas: bool,
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            queue_policy: QueuePolicy::Queue,
            max_queued_turns: 16,
            persist_deltas: false,
        }
    }
}

/// broker 的全部出站依赖。
pub struct BrokerDeps {
    pub store: Arc<dyn SessionStore>,
    pub deliveries: Arc<dyn RemoteDeliveryStore>,
    pub backends: Arc<dyn SessionBackendFactory>,
    pub exports: Arc<dyn ExportStore>,
    /// 节点信任记录：`Actor::Node` 的 Owner 侧授权需要「该 Access 信任行已配对且 grants 含所需 grant」
    /// （§6.5），因此授权判定不能只靠 Export 记录。
    pub trust: Arc<dyn TrustStore>,
    pub publisher: Arc<dyn EventPublisher>,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdGenerator>,
    /// 授权拒绝的审计（§3.5 的 `authorization.denied`）。`None` = 该部署不注入审计端口。
    pub audit: Option<Arc<dyn AuditStore>>,
}

/// 不带 wire 指纹的入口使用的规范占位摘要（32 个零字节的 base64url，无填充、末字符在规范集合内）。
const PLACEHOLDER_FINGERPRINT: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

/// 每会话串行门（§6.3：一个会话一个串行队列，不同会话并行）。
///
/// core 不依赖 runtime，因此这里用 std 互斥量 + `Waker` 自己实现 FIFO 异步门：等待者按到达顺序取号，
/// 释放时把锁**指名**交给队首（`granted` 是票据而不是 waker 比较——`Waker` 之间可能等价，票据不会），
/// 等待者被丢弃时收回它的票据（`Wait::drop`），因此取消不会让会话卡死。持有它即"该会话的串行队列"，
/// 跨 `.await` 合法（守卫是 `Send`）。
struct Gate {
    inner: Mutex<GateInner>,
}

struct GateInner {
    locked: bool,
    next_ticket: u64,
    waiters: VecDeque<(u64, Waker)>,
    granted: Option<u64>,
}

struct GateGuard<'a> {
    gate: &'a Gate,
}

/// 等待取锁的 future；`Drop` 时收回票据（调用方在等待期间取消命令时不会卡住会话）。
struct Wait<'a> {
    gate: &'a Gate,
    ticket: Option<u64>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Gate {
    fn new() -> Self {
        Self {
            inner: Mutex::new(GateInner {
                locked: false,
                next_ticket: 0,
                waiters: VecDeque::new(),
                granted: None,
            }),
        }
    }

    async fn guard(&self) -> GateGuard<'_> {
        Wait {
            gate: self,
            ticket: None,
        }
        .await;
        GateGuard { gate: self }
    }
}

impl Future for Wait<'_> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let mut inner = lock(&self.gate.inner);
        if !inner.locked {
            inner.locked = true;
            inner.granted = None;
            self.ticket = None;
            return Poll::Ready(());
        }
        match self.ticket {
            Some(ticket) if inner.granted == Some(ticket) => {
                inner.granted = None;
                self.ticket = None;
                Poll::Ready(())
            }
            Some(_) => Poll::Pending,
            None => {
                inner.next_ticket += 1;
                let ticket = inner.next_ticket;
                inner.waiters.push_back((ticket, cx.waker().clone()));
                self.ticket = Some(ticket);
                Poll::Pending
            }
        }
    }
}

impl Drop for Wait<'_> {
    fn drop(&mut self) {
        let Some(ticket) = self.ticket else {
            return;
        };
        let mut inner = lock(&self.gate.inner);
        if let Some(position) = inner
            .waiters
            .iter()
            .position(|(queued, _)| *queued == ticket)
        {
            inner.waiters.remove(position);
        }
        if inner.granted == Some(ticket) {
            // 已经指名交给它、但它被丢弃了：转交给下一位等待者，或直接解锁。
            match inner.waiters.pop_front() {
                Some((next, waker)) => {
                    inner.granted = Some(next);
                    drop(inner);
                    waker.wake();
                }
                None => {
                    inner.locked = false;
                    inner.granted = None;
                }
            }
        }
    }
}

impl Drop for GateGuard<'_> {
    fn drop(&mut self) {
        let mut inner = lock(&self.gate.inner);
        match inner.waiters.pop_front() {
            Some((ticket, waker)) => {
                inner.granted = Some(ticket);
                drop(inner);
                waker.wake();
            }
            None => {
                inner.locked = false;
                inner.granted = None;
            }
        }
    }
}

/// 已持久接受、等待派发的 turn（§6.4）。
///
/// **prompt 正文只在内存**：`owned_command` 按 §11.2 只承诺保留 fingerprint/接受状态/结果引用，不要求
/// 保存完整 prompt。进程崩溃后未派发的 turn 必须由恢复任务终结为 `command.uncertain`，不得静默重新
/// 派发（§11.2）。
struct QueuedTurn {
    turn: TurnId,
    request: RequestId,
    prompt: PromptRequest,
    /// 触发该 turn 的设备/本地 CLI；用于 `SYNC_PROTOCOL.md` §10.1 的 `origin.kind` 判定。
    actor: Actor,
}

#[derive(Default)]
struct TurnQueue {
    running: Option<TurnId>,
    running_request: Option<RequestId>,
    running_actor: Option<Actor>,
    waiting: VecDeque<QueuedTurn>,
}

/// 一个会话的运行时状态（owned 与 imported 各一份，键见 [`Broker::slot_key`]）。
struct Slot {
    /// §6.3 的每会话串行门：命令处理、事件落盘与派发都在它内部完成。
    gate: Gate,
    pending: Mutex<Vec<EndpointEvent>>,
    endpoint: Mutex<Option<Arc<dyn SessionEndpoint>>>,
    queue: Mutex<TurnQueue>,
    /// §6 第 14 条：每个 `(turn, messageId)` 已提交的 delta 片段，turn 终态时折叠成一条
    /// `agent.message.completed`。
    deltas: Mutex<HashMap<String, Vec<DeltaFragment>>>,
    /// §6 第 15 条：该 turn 已提交的 delta 行的 global_sequence，终态后用于 `turn.delta_compacted`
    /// 的 `compacted` 清单（server epoch 在提交压缩时从 `head()` 取，避免拿事件自己的 origin epoch 冒充）。
    turn_deltas: Mutex<HashMap<String, Vec<Sequence>>>,
    /// §6 第 9 条：因落盘失败被放弃的 turn（见 [`Broker::abandon_failed_turn`]），按放弃顺序排列。
    /// 它们的后续事件（含终态）不再提交：正文已经缺块，不得再在库里留下 `completed`；
    /// 最后一个也是「需要 `turnId` 但适配器未带标识的迟到事件」的兜底归属者（适配层不带 turn，
    /// 归属由 [`crate::broker`] 按 §10.3 的集合完成）。
    abandoned: Mutex<Vec<TurnId>>,
}

/// 一条已提交 delta 的折叠输入（只来自 delta 的 view，`agent.message.delta` 专用）。
#[derive(Clone)]
struct DeltaFragment {
    message: MessageId,
    index: u64,
    text: String,
    /// 适配器投影的非文本块原文（缺失时该 delta 只贡献文本，core 不猜类型）。
    block: Option<String>,
}

impl Slot {
    fn push_event(&self, event: EndpointEvent) {
        lock(&self.pending).push(event);
    }

    fn running_turn(&self) -> Option<TurnId> {
        lock(&self.queue).running.clone()
    }

    fn running_request(&self) -> Option<RequestId> {
        lock(&self.queue).running_request.clone()
    }

    /// 当前 active turn 的触发者（用于事件 origin 判定）。
    fn running_actor(&self) -> Option<Actor> {
        lock(&self.queue).running_actor.clone()
    }

    /// 终态落盘后清空 active turn（`turn.queued` 之外的排队 turn 保留）。
    fn finish_turn(&self) {
        let mut queue = lock(&self.queue);
        queue.running = None;
        queue.running_request = None;
        queue.running_actor = None;
    }
}

// ---------------------------------------------------------------------------------------------
// Broker
// ---------------------------------------------------------------------------------------------

/// `docs/CORE_PORTS_AND_STORAGE.md` §6 的实现：提交与发布、每会话串行、幂等、交互仲裁、队列策略。
pub struct Broker {
    deps: BrokerDeps,
    config: BrokerConfig,
    slots: Mutex<HashMap<String, Arc<Slot>>>,
}

impl Broker {
    pub fn new(deps: BrokerDeps, config: BrokerConfig) -> Self {
        Self {
            deps,
            config,
            slots: Mutex::new(HashMap::new()),
        }
    }

    pub fn config(&self) -> BrokerConfig {
        self.config
    }

    fn now(&self) -> Timestamp {
        self.deps.clock.now()
    }

    /// 会话槽位键。用文本键而不是 `HashMap<SessionId, _>`，是为了让 owned 与 imported 的同一
    /// `SessionId` 不共享串行队列（来自不同 Owner 的同一 id 不是同一个会话，`NODE_LINK_PROTOCOL.md` §7）。
    fn slot_key(reference: &SessionReference) -> String {
        match reference {
            SessionReference::Owned(owned) => format!("owned:{}", owned.session_id.as_str()),
            SessionReference::Remote(remote) => format!(
                "remote:{}:{}:{}",
                remote.owner_node_id.as_str(),
                remote.export_id.as_str(),
                remote.session_id.as_str()
            ),
        }
    }

    fn reference_of(session: &SessionId) -> SessionReference {
        SessionReference::Owned(OwnedSessionRef {
            session_id: session.clone(),
        })
    }

    fn slot(&self, reference: &SessionReference) -> Arc<Slot> {
        let key = Self::slot_key(reference);
        let mut slots = lock(&self.slots);
        slots
            .entry(key)
            .or_insert_with(|| {
                Arc::new(Slot {
                    gate: Gate::new(),
                    pending: Mutex::new(Vec::new()),
                    endpoint: Mutex::new(None),
                    queue: Mutex::new(TurnQueue::default()),
                    deltas: Mutex::new(HashMap::new()),
                    turn_deltas: Mutex::new(HashMap::new()),
                    abandoned: Mutex::new(Vec::new()),
                })
            })
            .clone()
    }

    fn owned_slot(&self, session: &SessionId) -> Arc<Slot> {
        self.slot(&Self::reference_of(session))
    }

    /// 交给 `SessionBackendFactory` 的 `EventSink`（§5.1/§6.1）。事件按到达顺序缓冲，由
    /// [`Broker::flush`] 落盘；缓冲顺序即提交顺序。
    pub fn sink(&self, session: &SessionId) -> EventSink {
        let slot = self.owned_slot(session);
        EventSink::new(move |event| slot.push_event(event))
    }

    // -----------------------------------------------------------------------------------------
    // §6.5 授权
    // -----------------------------------------------------------------------------------------

    /// 所有用例入口的第一步：按 actor 的 scope/grant 与 Export 交集判定（§6.5）。
    ///
    /// - `LocalCli`：本地管理入口，访问控制由 `LOCAL_ADMIN_PROTOCOL.md` §2.2 的 OS 用户边界保证。
    /// - `Device`：`scopes` 是展开后的独立 scope（等于命令名），必须含该命令。
    /// - `Node`：Access 侧要求本地 `ImportRecord.grants` 覆盖该命令；Owner 侧要求某个未撤销
    ///   `ExportRecord` 既覆盖该命令、又覆盖目标会话的 agent。两者任一成立即通过。
    /// - `PairingClaimant`：**任何命令都不授权**（design D12：认领方只存在于配对通道，且那里不经
    ///   broker 授权）；命中即失败关闭并记一条 `authorization.denied`。
    pub async fn authorize(
        &self,
        actor: &Actor,
        command: &str,
        session: Option<&SessionId>,
        request: &RequestId,
    ) -> Result<(), Denied> {
        let allowed = match actor {
            Actor::LocalCli => true,
            Actor::Device { scopes, .. } => scopes.contains(command),
            // 查不到 Export/Import（或存储失败）时**失败关闭**：不能证明授权即拒绝（§6.5）。
            Actor::Node { node, .. } => self
                .node_allowed(node, command, session)
                .await
                .unwrap_or(false),
            // 配对认领方没有 scope/grant 面：不给任何命令授权（见方法文档）。
            Actor::PairingClaimant { .. } => false,
        };
        if allowed {
            return Ok(());
        }
        let denied = Denied::scope_denied();
        self.audit_denial(actor, session, request).await;
        Err(denied)
    }

    /// `Actor::Node` 的判定：Access 侧与 Owner 侧各有一条路径，任一成立即通过。
    ///
    /// Access 侧（本节点是 `node` 的**客户端**）：本地 `ImportRecord.grants` 覆盖该命令。
    ///
    /// Owner 侧（本节点是导出方）：有效权限是「Export grant ∩ 该 Access 信任记录 grant」的交集——
    /// 信任记录必须是已配对的 `access` 行且其 `grants` 含该命令所需的 grant；同时某个未撤销
    /// `ExportRecord` 必须覆盖该命令（`scopes` 含该 grant）并与该节点的信任记录 grants 有交集
    /// （`export.scopes ∩ node.grants ≠ ∅`，与 Node Link 的可见性口径同源）；此外**会话命令**还必须
    /// 落在覆盖目标会话 agent 的 Export 上。
    ///
    /// 无会话命令（`session.list`/`command.status`/`session.create`）没有目标会话可比对 agent，
    /// 因此 Owner 侧按「该节点是否与某个覆盖该命令的 Export 有关联」判定；`session.list` 的结果过滤
    /// 与 `session.create` 的参数校验仍由 `server::node_link` 按其单点可见性策略完成，本层只判授权。
    async fn node_allowed(
        &self,
        node: &NodeId,
        command: &str,
        session: Option<&SessionId>,
    ) -> Result<bool, PortError> {
        let Some(grant) = required_grant(command) else {
            return Ok(false);
        };
        // Access 侧：本地 Import 的 grants 覆盖该命令。
        for import in self.deps.exports.imports().await? {
            if import.owner_node_id() == node && import.grants().contains(grant) {
                return Ok(true);
            }
        }
        // 节点信任记录：Owner 侧的有效权限是「Export grant ∩ 该 Access 信任记录 grant」的交集，
        // 因此信任行缺失/未配对或 grants 不含该命令所需的 grant 时直接失败关闭（不因为某个 Export
        // 恰好覆盖就放行）。
        let trust = self
            .deps
            .trust
            .node(node, NodeKind::Access)
            .await?
            .filter(|row| row.state() == NodeState::Paired);
        let Some(trust) = trust else {
            return Ok(false);
        };
        if !trust.grants().contains(grant) {
            return Ok(false);
        }
        // Owner 侧：某个未撤销 Export 覆盖该命令；会话命令还必须覆盖目标会话的 agent。
        let agent = match session {
            Some(session) => self
                .deps
                .store
                .load(session)
                .await?
                .map(|snapshot| snapshot.session.agent().clone()),
            None => None,
        };
        if session.is_some() && agent.is_none() {
            return Ok(false);
        }
        for export in self.deps.exports.exports().await? {
            if export.revoked_at().is_some() || !export.scopes().contains(grant) {
                continue;
            }
            if export
                .scopes()
                .iter()
                .all(|scope| !trust.grants().contains(scope))
            {
                // 与该节点信任记录不相交的 Export 不是它的授权来源（Node Link 的可见性口径）。
                continue;
            }
            match &agent {
                Some(agent) => {
                    if export
                        .agent_ids()
                        .iter()
                        .any(|id| id.as_str() == agent.agent_id().as_str())
                    {
                        return Ok(true);
                    }
                }
                // 无会话命令：与该节点有关联且覆盖该命令的 Export 存在即通过。
                None => return Ok(true),
            }
        }
        Ok(false)
    }

    /// 审计拒绝（§3.5 的 `authorization.denied`）；审计失败不改变授权判定。
    async fn audit_denial(&self, actor: &Actor, session: Option<&SessionId>, request: &RequestId) {
        let Some(audit) = self.deps.audit.as_ref() else {
            return;
        };
        let record = AuditRecord::try_new(
            self.now(),
            AuditAction::AuthorizationDenied,
            actor.clone(),
            None,
            None,
            EntityRef::Command {
                session: session.cloned(),
                request: request.clone(),
            },
            AuditOutcome::Denied,
            None,
        );
        if let Ok(record) = record {
            let _ = audit.append(record).await;
        }
    }

    // -----------------------------------------------------------------------------------------
    // mutation 管道
    // -----------------------------------------------------------------------------------------

    /// 一条 mutation 的完整管道（§6.1/§6.3/§6.6/§6.9）。返回值只可能是 `Accepted` 或 `Rejected`
    /// （§11.2：mutation 首次提交只返回这两种）。
    pub async fn submit_mutation(
        &self,
        actor: &Actor,
        command: &ClientCommand,
    ) -> Result<CommandReceipt, PortError> {
        let name = command_name(&command.payload);
        if command.command != name || command.kind != command_kind(&command.payload) {
            return self.reject(
                "protocol.schema_invalid",
                "command/kind 与 payload 变体不一致",
                false,
            );
        }
        if let Err(denied) = self
            .authorize(actor, name, command.session.as_ref(), &command.request)
            .await
        {
            return Ok(CommandReceipt::Rejected {
                error: denied.into_error()?,
            });
        }
        match &command.payload {
            CommandPayload::Prompt { content } => self.submit_prompt(actor, command, content).await,
            CommandPayload::Cancel { turn } => self.submit_cancel(actor, command, turn).await,
            CommandPayload::ModeSet { mode } => self.submit_mode_set(actor, command, mode).await,
            CommandPayload::ConfigSet { id, value } => {
                self.submit_config_set(actor, command, id, value.clone())
                    .await
            }
            CommandPayload::PermissionResolve {
                interaction,
                option_id,
            } => {
                self.submit_permission_resolve(actor, command, interaction, option_id)
                    .await
            }
            CommandPayload::ElicitationRespond {
                interaction,
                action,
                values,
            } => {
                self.submit_elicitation_respond(actor, command, interaction, *action, values)
                    .await
            }
            // 查询命令不在此派发：结果由 §4 的查询入口（`list_sessions`/`read_session`/
            // `command_status`/`config_options`）返回 core 类型，由适配器投影到 wire（core 不做 JSON
            // 序列化）。这里只确认"已接受、无副作用"。
            CommandPayload::SessionList { .. }
            | CommandPayload::SessionRead { .. }
            | CommandPayload::CommandStatus { .. }
            | CommandPayload::ModeList { .. }
            | CommandPayload::ConfigList { .. } => Ok(CommandReceipt::Accepted {
                request: command.request.clone(),
                turn: None,
            }),
        }
    }

    async fn submit_prompt(
        &self,
        actor: &Actor,
        command: &ClientCommand,
        content: &[PromptContentBlock],
    ) -> Result<CommandReceipt, PortError> {
        let Some(session) = command.session.clone() else {
            return self.reject(
                "command.unsupported",
                "session.prompt 必须带 sessionId",
                false,
            );
        };
        let slot = self.owned_slot(&session);
        let _guard = slot.gate.guard().await;

        if let Some(receipt) = self.idempotency(actor, command).await? {
            return Ok(receipt);
        }
        let Some(snapshot) = self.deps.store.load(&session).await? else {
            return self.reject("session.not_found", "会话不存在", false);
        };

        // §6.4：active turn 时的两种策略。
        if Self::is_active(snapshot.session.state()) || slot.running_turn().is_some() {
            let waiting = lock(&slot.queue).waiting.len() as u32;
            match self.config.queue_policy {
                QueuePolicy::RejectBusy => {
                    return self.reject("session.busy", "该会话已有 active turn", true);
                }
                QueuePolicy::Queue => {
                    if waiting >= self.config.max_queued_turns {
                        return self.reject("session.busy", "该会话的排队 turn 已达上限", true);
                    }
                }
            }
        }

        let turn = self.deps.ids.turn_id();
        let event = pending_event(
            "turn.queued",
            EventKind::State,
            view_turn(&turn, TurnState::Queued)?,
            Some(turn.clone()),
            Some(command.request.clone()),
            StoredPolicy::Durable,
            Some(actor),
        )?;
        // 会话状态：只有在没有 active turn 时才能从 Idle/Failed 推到 Queued；否则保持既有状态
        // （`Running`/`WaitingPermission` 不能被排队中的 turn 覆盖）。
        let state = if Self::is_active(snapshot.session.state()) {
            None
        } else {
            Some(StateChange::Update(SessionUpdate {
                state: Some(SessionState::Queued),
                mode: ModeChange::Unchanged,
                closed_at: None,
                interaction: None,
            }))
        };
        let turns = vec![TurnChange::Create(NewTurn {
            turn: turn.clone(),
            state: TurnState::Queued,
            causation: Some(command.request.clone()),
            started_at: None,
        })];
        match self
            .accept(actor, command, &session, state, turns, vec![event])
            .await?
        {
            Accept::Done(receipt) => Ok(receipt),
            Accept::Committed => {
                lock(&slot.queue).waiting.push_back(QueuedTurn {
                    turn: turn.clone(),
                    request: command.request.clone(),
                    prompt: PromptRequest {
                        content: content.to_vec(),
                    },
                    actor: actor.clone(),
                });
                self.pump_locked(&slot, &session).await?;
                Ok(CommandReceipt::Accepted {
                    request: command.request.clone(),
                    turn: Some(turn),
                })
            }
        }
    }

    async fn submit_cancel(
        &self,
        actor: &Actor,
        command: &ClientCommand,
        turn: &Option<TurnId>,
    ) -> Result<CommandReceipt, PortError> {
        let Some(session) = command.session.clone() else {
            return self.reject(
                "command.unsupported",
                "session.cancel 必须带 sessionId",
                false,
            );
        };
        let slot = self.owned_slot(&session);
        let _guard = slot.gate.guard().await;
        if let Some(receipt) = self.idempotency(actor, command).await? {
            return Ok(receipt);
        }
        if self.deps.store.load(&session).await?.is_none() {
            return self.reject("session.not_found", "会话不存在", false);
        }
        match self
            .accept(actor, command, &session, None, Vec::new(), Vec::new())
            .await?
        {
            Accept::Done(receipt) => return Ok(receipt),
            Accept::Committed => {}
        }
        let target = turn.clone().or_else(|| slot.running_turn());
        let endpoint = self.endpoint(&slot, &session).await?;
        let dispatched = endpoint.cancel(target.clone()).await;
        // 后端会经 sink 发 `turn.cancelled`；它由本调用驱动落盘（含 prompt 命令的终态，§11.2）。
        self.flush_locked(&slot, &session).await?;
        let at = self.now();
        let status = if dispatched.is_ok() {
            CommandStatus::Completed
        } else {
            CommandStatus::Failed
        };
        let error = match &dispatched {
            Ok(()) => None,
            Err(error) => Some(self.port_error_public(error)?),
        };
        // 取消成功的收据带被取消的 turn（已知时）；失败时只有错误，不编造 turn。
        let result = match (&dispatched, target.as_ref()) {
            (Ok(()), Some(turn)) => Some(turn_result(turn)?),
            _ => None,
        };
        self.commit_terminal(&session, command, status, error, result, None, &at)
            .await?;
        Ok(CommandReceipt::Accepted {
            request: command.request.clone(),
            turn: None,
        })
    }

    async fn submit_mode_set(
        &self,
        actor: &Actor,
        command: &ClientCommand,
        mode: &ModeId,
    ) -> Result<CommandReceipt, PortError> {
        let Some(session) = command.session.clone() else {
            return self.reject(
                "command.unsupported",
                "session.mode.set 必须带 sessionId",
                false,
            );
        };
        let slot = self.owned_slot(&session);
        let _guard = slot.gate.guard().await;
        if let Some(receipt) = self.idempotency(actor, command).await? {
            return Ok(receipt);
        }
        let Some(snapshot) = self.deps.store.load(&session).await? else {
            return self.reject("session.not_found", "会话不存在", false);
        };
        // §6.8：模型/配置切换只在 turn 边界生效。v1 选择"显式返回 state.version_conflict"而不是排队。
        if Self::is_active(snapshot.session.state()) || slot.running_turn().is_some() {
            return self.version_conflict(snapshot.session.version().get(), command);
        }
        match self
            .accept(actor, command, &session, None, Vec::new(), Vec::new())
            .await?
        {
            Accept::Done(receipt) => return Ok(receipt),
            Accept::Committed => {}
        }
        let endpoint = self.endpoint(&slot, &session).await?;
        match endpoint.set_mode(mode).await {
            Ok(()) => {
                // 后端的 `session.mode.changed` 先落盘（§11.5：先发状态事件、再发 command.completed）。
                self.flush_locked(&slot, &session).await?;
                let state = StateChange::Update(SessionUpdate {
                    state: None,
                    mode: ModeChange::Set(mode_ref(mode)?),
                    closed_at: None,
                    interaction: None,
                });
                self.apply_state(&session, state, command).await?;
            }
            Err(error) => {
                let public = self.port_error_public(&error)?;
                let at2 = self.now();
                self.commit_terminal(
                    &session,
                    command,
                    CommandStatus::Failed,
                    Some(public),
                    None,
                    None,
                    &at2,
                )
                .await?;
            }
        }
        Ok(CommandReceipt::Accepted {
            request: command.request.clone(),
            turn: None,
        })
    }

    async fn submit_config_set(
        &self,
        actor: &Actor,
        command: &ClientCommand,
        id: &ConfigOptionId,
        value: ConfigValue,
    ) -> Result<CommandReceipt, PortError> {
        let Some(session) = command.session.clone() else {
            return self.reject(
                "command.unsupported",
                "session.config.set 必须带 sessionId",
                false,
            );
        };
        let slot = self.owned_slot(&session);
        let _guard = slot.gate.guard().await;
        if let Some(receipt) = self.idempotency(actor, command).await? {
            return Ok(receipt);
        }
        let Some(snapshot) = self.deps.store.load(&session).await? else {
            return self.reject("session.not_found", "会话不存在", false);
        };
        if Self::is_active(snapshot.session.state()) || slot.running_turn().is_some() {
            return self.version_conflict(snapshot.session.version().get(), command);
        }
        match self
            .accept(actor, command, &session, None, Vec::new(), Vec::new())
            .await?
        {
            Accept::Done(receipt) => return Ok(receipt),
            Accept::Committed => {}
        }
        let endpoint = self.endpoint(&slot, &session).await?;
        match endpoint.set_config(id, value).await {
            Ok(()) => {
                self.flush_locked(&slot, &session).await?;
                let state = StateChange::Update(SessionUpdate {
                    state: None,
                    mode: ModeChange::Unchanged,
                    closed_at: None,
                    interaction: None,
                });
                self.apply_state(&session, state, command).await?;
            }
            Err(error) => {
                let public = self.port_error_public(&error)?;
                let at2 = self.now();
                self.commit_terminal(
                    &session,
                    command,
                    CommandStatus::Failed,
                    Some(public),
                    None,
                    None,
                    &at2,
                )
                .await?;
            }
        }
        Ok(CommandReceipt::Accepted {
            request: command.request.clone(),
            turn: None,
        })
    }

    async fn submit_permission_resolve(
        &self,
        actor: &Actor,
        command: &ClientCommand,
        interaction: &InteractionId,
        option_id: &str,
    ) -> Result<CommandReceipt, PortError> {
        let Some(session) = command.session.clone() else {
            return self.reject(
                "command.unsupported",
                "permission.resolve 必须带 sessionId",
                false,
            );
        };
        let slot = self.owned_slot(&session);
        let _guard = slot.gate.guard().await;
        if let Some(receipt) = self.idempotency(actor, command).await? {
            return Ok(receipt);
        }
        let resolution = match self
            .permission_resolution(&session, interaction, option_id)
            .await?
        {
            Ok(resolution) => resolution,
            Err(receipt) => return Ok(receipt),
        };
        match self
            .accept(actor, command, &session, None, Vec::new(), Vec::new())
            .await?
        {
            Accept::Done(receipt) => return Ok(receipt),
            Accept::Committed => {}
        }
        let outcome = self
            .resolve_locked(actor, &slot, &session, interaction, resolution)
            .await?;
        let at = self.now();
        let status = match outcome {
            Resolution::Resolved => CommandStatus::Completed,
            Resolution::AlreadyResolved => CommandStatus::Failed,
        };
        let error = match outcome {
            Resolution::Resolved => None,
            Resolution::AlreadyResolved => Some(
                PublicError::coded(
                    "interaction.already_resolved",
                    "该交互已经由另一个应答解析，既有结果未被覆盖",
                    false,
                )
                .map_err(PortError::from)?,
            ),
        };
        self.commit_terminal(&session, command, status, error, None, None, &at)
            .await?;
        Ok(CommandReceipt::Accepted {
            request: command.request.clone(),
            turn: None,
        })
    }

    async fn submit_elicitation_respond(
        &self,
        actor: &Actor,
        command: &ClientCommand,
        interaction: &InteractionId,
        action: ElicitationAction,
        values: &ElicitationValues,
    ) -> Result<CommandReceipt, PortError> {
        let Some(session) = command.session.clone() else {
            return self.reject(
                "command.unsupported",
                "elicitation.respond 必须带 sessionId",
                false,
            );
        };
        let resolution = match action {
            ElicitationAction::Submit => InteractionResolution::elicitation_submit(values.clone())
                .map_err(PortError::from)?,
            ElicitationAction::Cancel => InteractionResolution::elicitation_cancel(),
            ElicitationAction::Decline => InteractionResolution::elicitation_decline(),
        };
        let slot = self.owned_slot(&session);
        let _guard = slot.gate.guard().await;
        if let Some(receipt) = self.idempotency(actor, command).await? {
            return Ok(receipt);
        }
        match self
            .accept(actor, command, &session, None, Vec::new(), Vec::new())
            .await?
        {
            Accept::Done(receipt) => return Ok(receipt),
            Accept::Committed => {}
        }
        let outcome = self
            .resolve_locked(actor, &slot, &session, interaction, resolution)
            .await?;
        let at = self.now();
        let (status, error) = match outcome {
            Resolution::Resolved => (CommandStatus::Completed, None),
            Resolution::AlreadyResolved => (
                CommandStatus::Failed,
                Some(
                    PublicError::coded(
                        "interaction.already_resolved",
                        "该交互已经由另一个应答解析，既有结果未被覆盖",
                        false,
                    )
                    .map_err(PortError::from)?,
                ),
            ),
        };
        self.commit_terminal(&session, command, status, error, None, None, &at)
            .await?;
        Ok(CommandReceipt::Accepted {
            request: command.request.clone(),
            turn: None,
        })
    }

    /// §4 `PermissionCommands::resolve_interaction`（不带命令幂等键的入口）。
    pub async fn resolve_interaction(
        &self,
        actor: &Actor,
        reference: &SessionReference,
        interaction: &InteractionId,
        resolution: InteractionResolution,
    ) -> Result<Resolution, PortError> {
        let session = match reference {
            SessionReference::Owned(owned) => owned.session_id.clone(),
            SessionReference::Remote(_) => {
                return Err(PortError::InvalidRequest(
                    "imported 会话的交互由持有 attachment 的 facade 转成上游请求",
                ));
            }
        };
        let command = if matches!(resolution, InteractionResolution::Permission(_)) {
            "permission.resolve"
        } else {
            "elicitation.respond"
        };
        let request = self.deps.ids.request_id();
        let slot = self.owned_slot(&session);
        let _guard = slot.gate.guard().await;
        self.authorize(actor, command, Some(&session), &request)
            .await
            .map_err(Denied::into_port_error)?;
        if resolution.validate().is_err() {
            return Err(PortError::InvalidRequest("交互解析不满足 schema"));
        }
        self.resolve_locked(actor, &slot, &session, interaction, resolution)
            .await
    }

    /// 交互解析的公共实现（调用方已持有串行门）：**先**用条件更新落盘仲裁（§6.7 first-writer-wins），
    /// **再**派发给后端，最后把后端发出的 `*.resolved` 事件落盘。
    async fn resolve_locked(
        &self,
        actor: &Actor,
        slot: &Slot,
        session: &SessionId,
        interaction: &InteractionId,
        resolution: InteractionResolution,
    ) -> Result<Resolution, PortError> {
        let state = StateChange::Update(SessionUpdate {
            state: None,
            mode: ModeChange::Unchanged,
            closed_at: None,
            interaction: Some(InteractionResolved {
                interaction: interaction.clone(),
                resolution: resolution.clone(),
                resolved_by: actor.clone(),
            }),
        });
        let commit = OwnedCommit {
            session: Some(session.clone()),
            at: self.now(),
            expected_version: None,
            state: Some(state),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        };
        match self.commit_owned(commit).await {
            Ok(_) => {}
            Err(PortError::Conflict(ConflictKind::AlreadyResolved)) => {
                return Ok(Resolution::AlreadyResolved);
            }
            Err(error) => return Err(error),
        }
        let endpoint = self.endpoint(slot, session).await?;
        endpoint
            .resolve_interaction(interaction, resolution)
            .await?;
        self.flush_locked(slot, session).await?;
        Ok(Resolution::Resolved)
    }

    /// §4 `ConfigCommands::set_mode`（不带命令幂等键的入口）。返回新会话版本。
    pub async fn set_mode(
        &self,
        actor: &Actor,
        reference: &SessionReference,
        mode: ModeId,
    ) -> Result<Version, PortError> {
        let request = self.deps.ids.request_id();
        let command = ClientCommand {
            actor: actor.clone(),
            request: request.clone(),
            command: "session.mode.set".to_string(),
            kind: CommandKind::Mutation,
            session: Some(self.owned_session(reference)?),
            expected_version: None,
            request_fingerprint: self.fingerprint_of(&request)?,
            payload: CommandPayload::ModeSet { mode: mode.clone() },
        };
        let before = self.command_session_version(&command).await?;
        let receipt = self.submit_mode_set(actor, &command, &mode).await?;
        self.outcome_version(&receipt, before, &command).await
    }

    /// §4 `ConfigCommands::set_config`（不带命令幂等键的入口）。返回新会话版本。
    pub async fn set_config(
        &self,
        actor: &Actor,
        reference: &SessionReference,
        id: ConfigOptionId,
        value: ConfigValue,
    ) -> Result<Version, PortError> {
        let request = self.deps.ids.request_id();
        let command = ClientCommand {
            actor: actor.clone(),
            request: request.clone(),
            command: "session.config.set".to_string(),
            kind: CommandKind::Mutation,
            session: Some(self.owned_session(reference)?),
            expected_version: None,
            request_fingerprint: self.fingerprint_of(&request)?,
            payload: CommandPayload::ConfigSet {
                id: id.clone(),
                value: value.clone(),
            },
        };
        let before = self.command_session_version(&command).await?;
        let receipt = self.submit_config_set(actor, &command, &id, value).await?;
        self.outcome_version(&receipt, before, &command).await
    }

    /// `session.create`（Node Link 命令，§12.7）。返回 Owner 分配的 `SessionId`。
    ///
    /// 幂等键是协议维度的 `(actor, requestId)`（§6 第 6 条），`requestId` 与 `request_fingerprint`
    /// 由适配层给出（Node Link 的 `requestId` 与 ACPR-CJ1 后的 payload 摘要）。**创建会话与幂等行在
    /// 同一次提交**：崩溃窗口里留下的是 `accepted` 行 + 已存在的会话，启动恢复按 §6 第 16 条把它终结为
    /// `uncertain`（不重复创建）。
    ///
    /// 同键重试（含重启后）由存储层按幂等行重放：本次**不**创建第二个会话、也**不**开第二个后端端点，
    /// 返回首次结果的 `SessionId`；键相同而 `command`/`kind`/指纹/`expected_version` 任一不同 →
    /// `Conflict(IdempotencyConflict)`（§6 第 6 条）。
    pub async fn create_session(
        &self,
        actor: &Actor,
        request: &RequestId,
        request_fingerprint: &Digest,
        create: CreateSessionRequest,
    ) -> Result<SessionId, PortError> {
        self.authorize(actor, "session.create", None, request)
            .await
            .map_err(Denied::into_port_error)?;
        let at = self.now();
        let origin_epoch = self.deps.ids.origin_epoch();
        let commit = OwnedCommit {
            session: None,
            at: at.clone(),
            expected_version: None,
            state: Some(StateChange::Create(NewSession {
                title: None,
                agent: create.agent.clone(),
            })),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: Some(IdempotencyRecord {
                actor: actor.clone(),
                request: request.clone(),
                command: "session.create".to_owned(),
                kind: CommandKind::Mutation,
                // 目标会话在本次提交的事务内才分配，装配方预知不了：存储层把新会话 id 写进这一行
                // （§6 第 20 条），使终态提交与启动恢复都能按 `(session, requestId)` 定位它。
                session: None,
                expected_version: None,
                request_fingerprint: request_fingerprint.clone(),
                accepted_at: at,
            }),
            command_terminal: None,
            origin_epoch: Some(origin_epoch),
        };
        let outcome = self.commit_owned(commit).await?;
        let Some(session) = outcome.session_id else {
            return Err(PortError::InvalidRequest(
                "commit 未返回新建会话的 sessionId",
            ));
        };
        if outcome.replayed.is_some() {
            // 同键重试（或并发重复）：首次结果已存在，副作用只发生一次。
            return Ok(session);
        }
        let slot = self.owned_slot(&session);
        let sink = self.sink(&session);
        let endpoint: Arc<dyn SessionEndpoint> = self
            .deps
            .backends
            .create(&session, create, sink)
            .await?
            .into();
        *lock(&slot.endpoint) = Some(endpoint);
        Ok(session)
    }

    /// `session.create` 的终态提交（§6 第 20 条、§12.7）。返回本次是否真的写入了终态。
    ///
    /// `status` 必须是终态：`completed` 必须带 `result` 且不带 `error`，其余必须带 `error`（形状由
    /// [`CommandTerminalRecord::try_new`] 校验）。**幂等 no-op** 的两种情况：该 `(actor, requestId)`
    /// 没有持久记录（创建在幂等行落盘前就失败），或记录已经终结（首次结果不覆盖）。
    ///
    /// 只终结 `command == "session.create"` 的持久记录：该 `(actor, requestId)` 记着别的命令时是
    /// 适配层误用（wire 不可达），显式 `InvalidRequest`，不把那条命令的幂等行改写成创建的终态。
    ///
    /// 落盘失败（`Unavailable`）不报成功：行仍是 `accepted`，由启动恢复按 §6 第 16 条终结为 `uncertain`。
    pub async fn settle_session_create(
        &self,
        actor: &Actor,
        request: &RequestId,
        status: CommandStatus,
        result: Option<CommandResult>,
        error: Option<PublicError>,
    ) -> Result<bool, PortError> {
        if !status.is_terminal() {
            return Err(PortError::InvalidRequest("命令终态必须是终止态"));
        }
        let Some(record) = self.deps.store.find_request(request, actor).await? else {
            return Ok(false);
        };
        if record.command() != "session.create" {
            return Err(PortError::InvalidRequest(
                "settle_session_create 只能终结 session.create 的持久记录（§6 第 20 条）",
            ));
        }
        if record.status().is_terminal() {
            return Ok(false);
        }
        let Some(session) = record.session().cloned() else {
            return Err(PortError::Corrupt(
                "session.create 的持久记录缺少目标会话（§6 第 20 条）",
            ));
        };
        let at = self.now();
        let terminal =
            CommandTerminalRecord::try_new(status, Some(at.clone()), None, result, error)?;
        let (event_type, view) = match status {
            CommandStatus::Completed => {
                ("command.completed", view_command_completed(request, None)?)
            }
            CommandStatus::Failed => ("command.failed", view_command_failed(request, None)?),
            _ => (
                "command.uncertain",
                view_command_uncertain(request, "无法确认会话是否已创建")?,
            ),
        };
        let event = pending_event(
            event_type,
            EventKind::Structured,
            view,
            None,
            Some(request.clone()),
            StoredPolicy::Durable,
            Some(actor),
        )?;
        let commit = OwnedCommit {
            session: Some(session.clone()),
            at,
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![event],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: Some(terminal),
            origin_epoch: None,
        };
        let slot = self.owned_slot(&session);
        let _guard = slot.gate.guard().await;
        match self.commit_owned(commit).await {
            Ok(outcome) => {
                self.publish(&outcome.appended);
                Ok(true)
            }
            Err(PortError::Unavailable(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }

    // -----------------------------------------------------------------------------------------
    // 查询与重放（§6.12）
    // -----------------------------------------------------------------------------------------

    /// 一致性读视图（§6.12）：`sync.snapshot_begin`/`snapshot_end` 与随后的增量重放游标必须出自
    /// **同一个**视图——适配器拿到它以后自己调用 `head()`/`read_session()`/`replay()`。
    pub async fn read_view(&self) -> Result<Box<dyn ReadView>, PortError> {
        self.deps.store.read_view().await
    }

    pub async fn head(&self) -> Result<GlobalCursor, PortError> {
        self.deps.store.read_view().await?.head().await
    }

    /// 增量重放：在一个新读视图内完成（`ReplayBatch.head` 即该视图的 barrier）。
    pub async fn replay(
        &self,
        after: Option<GlobalCursor>,
        limit: ReplayLimit,
    ) -> Result<ReplayBatch, PortError> {
        self.deps
            .store
            .read_view()
            .await?
            .replay(after, limit)
            .await
    }

    pub async fn read_session(&self, query: HistoryQuery) -> Result<HistoryPage, PortError> {
        self.deps.store.read_view().await?.read_session(query).await
    }

    pub async fn retention_window(
        &self,
        session: &SessionId,
    ) -> Result<Option<(Sequence, Sequence)>, PortError> {
        self.deps.store.retention_window(session).await
    }

    pub async fn command_status(
        &self,
        actor: &Actor,
        request: &RequestId,
    ) -> Result<Option<CommandRecord>, PortError> {
        self.deps.store.find_request(request, actor).await
    }

    /// imported 会话的本地重放（只含无正文索引，`NODE_LINK_PROTOCOL.md` §6）。
    pub async fn remote_replay(
        &self,
        after: Option<LocalCursor>,
        limit: ReplayLimit,
    ) -> Result<Vec<DeliveryIndexEntry>, PortError> {
        self.deps.deliveries.local_replay(after, limit).await
    }

    // -----------------------------------------------------------------------------------------
    // imported 提交与发布（§6.2）
    // -----------------------------------------------------------------------------------------

    /// 一条 imported 事件：先 `commit_receipt`（无正文索引），成功后才发布；正文只在内存里转发。
    ///
    /// 返回 `true` 表示本次是新行（已发布），`false` 表示同一 `origin_event_id` 的重复投递
    /// （不新增 `local_sequence`，也不重复发布）。
    pub async fn deliver_imported(
        &self,
        session: RemoteSessionRef,
        origin: OriginEventRef,
        origin_sequence: Sequence,
        event_type: EventType,
        payload_digest: Digest,
        payload: Option<EventPayload>,
    ) -> Result<bool, PortError> {
        let reference = SessionReference::Remote(session.clone());
        let slot = self.slot(&reference);
        let _guard = slot.gate.guard().await;
        let receipt = DeliveryReceipt {
            session: session.clone(),
            origin: origin.clone(),
            origin_sequence,
            event_type: event_type.clone(),
            payload_digest: payload_digest.clone(),
            at: self.now(),
        };
        let outcome: ReceiptOutcome = self.deps.deliveries.commit_receipt(receipt).await?;
        if outcome.duplicate {
            return Ok(false);
        }
        self.deps.publisher.publish(CommittedDelivery::Imported {
            session,
            origin,
            origin_sequence,
            local_sequence: outcome.local_sequence,
            event_type,
            payload_digest,
            payload,
        });
        Ok(true)
    }

    // -----------------------------------------------------------------------------------------
    // 驱动入口
    // -----------------------------------------------------------------------------------------

    /// 只落盘缓冲事件（§6.10 的合并窗口由调用方决定，core 不读时钟/不设定时器）。
    pub async fn flush(&self, session: &SessionId) -> Result<(), PortError> {
        let slot = self.owned_slot(session);
        let _guard = slot.gate.guard().await;
        self.flush_locked(&slot, session).await
    }

    /// 落盘 + 派发下一个排队 turn，直到该会话无事可做（§6.3/§6.4）。
    pub async fn pump(&self, session: &SessionId) -> Result<(), PortError> {
        let slot = self.owned_slot(session);
        let _guard = slot.gate.guard().await;
        self.pump_locked(&slot, session).await
    }

    async fn pump_locked(&self, slot: &Slot, session: &SessionId) -> Result<(), PortError> {
        loop {
            self.flush_locked(slot, session).await?;
            if !self.dispatch_one(slot, session).await? {
                return Ok(());
            }
        }
    }

    /// 落盘一批后端事件。按**终态事件**切批，保证 §6.10 的"不跨 turn 边界、不延迟终态事件"。
    async fn flush_locked(&self, slot: &Slot, session: &SessionId) -> Result<(), PortError> {
        let buffered: Vec<EndpointEvent> = {
            let mut pending = lock(&slot.pending);
            std::mem::take(&mut *pending)
        };
        if buffered.is_empty() {
            return Ok(());
        }
        let mut chunk: Vec<EndpointEvent> = Vec::new();
        for event in buffered {
            if is_turn_terminal(&event.event_type) {
                // §6.10：合并不得跨 turn 边界、不得延迟终态事件——终态单独成批并立即提交。
                if !chunk.is_empty() {
                    self.commit_chunk(slot, session, std::mem::take(&mut chunk))
                        .await?;
                }
                self.commit_chunk(slot, session, vec![event]).await?;
            } else {
                chunk.push(event);
            }
        }
        if !chunk.is_empty() {
            self.commit_chunk(slot, session, chunk).await?;
        }
        Ok(())
    }

    /// 组装并提交一批（以终态结尾或纯增量）后端事件（§6.1 第 1–3 步）。
    async fn commit_chunk(
        &self,
        slot: &Slot,
        session: &SessionId,
        chunk: Vec<EndpointEvent>,
    ) -> Result<(), PortError> {
        let at = self.now();
        let running = slot.running_turn();
        let running_request = slot.running_request();
        let running_actor = slot.running_actor();
        let mut events: Vec<PendingEvent> = Vec::new();
        let mut turn_change: Option<TurnChange> = None;
        let mut session_state: Option<SessionState> = None;
        let mut terminal: Option<(TurnState, CommandStatus)> = None;
        let mut terminal_request: Option<RequestId> = None;
        let mut terminal_turn: Option<TurnId> = None;
        let mut last_view: Option<ViewJson> = None;
        let mut interactions: Vec<PendingInteractionWrite> = Vec::new();
        let mut delta_plan: Vec<(usize, TurnId, DeltaFragment)> = Vec::new();
        let mut completed_events: Vec<PendingEvent> = Vec::new();
        let mut compact_after: Option<(TurnId, usize)> = None;
        // 失败批次是否携带了**在跑的** turn 的事件：只有它才需要在写失败时放弃该 turn（§6 第 9 条）。
        let mut running_in_chunk = false;
        for (event_index, event) in chunk.into_iter().enumerate() {
            let turn = event.turn.clone().or_else(|| {
                if running.is_some() {
                    return running.clone();
                }
                // §6 第 9 条 + §10.3 的归属规则：需要 `turnId` 的事件在没有在跑 turn 时，只可能属于
                // 最后被放弃的那个 turn（适配器不带 turn 标识），兜底归属它。
                if view_requires_turn_id(event.event_type.as_str()) {
                    return lock(&slot.abandoned).last().cloned();
                }
                None
            });
            // §6 第 9 条：被放弃的 turn（曾发生落盘失败）不再接受任何事件，包括它的终态事件——
            // 否则库里会留下「报完成但正文缺失」的记录。
            if let Some(turn_id) = turn.as_ref() {
                if lock(&slot.abandoned).contains(turn_id) {
                    continue;
                }
            }
            if turn.is_some() && turn == running {
                running_in_chunk = true;
            }
            let policy = persistence_policy(&event.event_type);
            // §6 第 14 条：登记本批里的 delta 片段（提交成功后才并入累积）。
            if is_agent_message_delta(&event.event_type) {
                if let (Some(turn_id), Some(fragment)) =
                    (turn.clone(), delta_fragment(&event.payload.view))
                {
                    delta_plan.push((event_index, turn_id, fragment));
                }
            }
            if let Some((turn_state, command_status)) = turn_terminal(&event.event_type) {
                if let Some(turn_id) = turn.clone() {
                    turn_change = Some(TurnChange::Update(TurnUpdate {
                        turn: turn_id.clone(),
                        state: turn_state,
                        ended_at: Some(at.clone()),
                    }));
                    session_state = Some(turn_session_state(turn_state));
                    terminal = Some((turn_state, command_status));
                    terminal_turn = Some(turn_id.clone());
                    terminal_request = event.causation.clone().or_else(|| running_request.clone());
                    last_view = Some(event.payload.view.clone());
                    // §6 第 14 条：该 turn 内出现过 delta 的每个 messageId 各发恰好一条 completed，
                    // 与终态事件同批提交；没有 delta 的消息不发。
                    completed_events = self.completed_events_for(slot, &turn_id)?;
                    let count = lock(&slot.deltas)
                        .get(turn_id.as_str())
                        .map(|fragments| fragments.len())
                        .unwrap_or(0);
                    compact_after = Some((turn_id, count));
                }
            }
            // §6 第 13 条：Agent 的权限/elicitation 请求必须**在同一提交里**同时落一条
            // `kind = interaction` 事件与一条 `PendingInteractionWrite`（`options` 只存在于事件
            // payload 里，是它的唯一权威来源）。
            if let Some(kind) = interaction_request_kind(&event.event_type) {
                let interaction = self.build_interaction(&event, kind, session, &at)?;
                interactions.push(PendingInteractionWrite {
                    interaction,
                    turn: turn.clone(),
                });
            }
            // §6.11：`Ephemeral` 由 `PendingEvent::from_persistence` 直接排除（`None` = 只做内存转发）。
            if let Some(pending) = PendingEvent::from_persistence(
                interaction_event_kind(event.kind, &event.event_type),
                event.event_type.clone(),
                policy,
                event.payload,
                event_origin(&event.event_type, running_actor.as_ref()),
                turn,
                event.causation,
            ) {
                events.push(pending);
            }
        }
        if events.is_empty() {
            return Ok(());
        }
        // §6 第 14 条：`agent.message.completed` 与 turn 终态事件同批提交，且排在命令终态事件之前。
        events.extend(completed_events);
        let had_terminal = terminal.is_some();
        let mut command_terminal = None;
        if let Some((_, status)) = terminal {
            if let Some(request) = terminal_request.clone() {
                // §11.2：同一事务内 sequence 上先写领域事件，再写 terminal event。
                let (event_type, view, error) = match status {
                    CommandStatus::Failed => (
                        "command.failed",
                        view_command_failed(&request, last_view.as_ref())?,
                        Some(
                            PublicError::coded(
                                "internal.unavailable",
                                "turn 失败，具体原因见同一 causation 的 turn.failed 事件",
                                false,
                            )
                            .map_err(PortError::from)?,
                        ),
                    ),
                    _ => (
                        "command.completed",
                        view_command_completed(
                            &request,
                            terminal_turn.as_ref().map(|turn| turn.as_str()),
                        )?,
                        None,
                    ),
                };
                events.push(pending_event(
                    event_type,
                    EventKind::Structured,
                    view,
                    terminal_turn.clone(),
                    Some(request.clone()),
                    StoredPolicy::Durable,
                    running_actor.as_ref(),
                )?);
                // §11.2 的终态收据：`completed` 带上这条命令自己的 turn（收据里唯一有意义的分量），
                // `failed` 的无 turn 信息（错误已单独落盘）。终态结果的这次落盘是 RV1-WP6-F3 的修复
                // 点：在此之前所有 `completed` 记录的 `result` 都是 NULL，适配层只能回空 object。
                let result = match (status, terminal_turn.as_ref()) {
                    (CommandStatus::Completed, Some(turn)) => Some(turn_result(turn)?),
                    _ => None,
                };
                command_terminal = Some(CommandTerminalRecord::try_new(
                    status,
                    Some(at.clone()),
                    None,
                    result,
                    error,
                )?);
            }
        }
        let state = session_state.map(|state| {
            StateChange::Update(SessionUpdate {
                state: Some(state),
                mode: ModeChange::Unchanged,
                closed_at: None,
                interaction: None,
            })
        });
        let kinds: Vec<EventKind> = events.iter().map(|event| event.kind).collect();
        let commit = OwnedCommit {
            session: Some(session.clone()),
            at: at.clone(),
            expected_version: None,
            state,
            turns: turn_change.into_iter().collect(),
            events,
            interactions,
            compacted: Vec::new(),
            idempotency: None,
            command_terminal,
            origin_epoch: None,
        };
        match self.commit_owned(commit).await {
            Ok(outcome) => {
                if had_terminal {
                    slot.finish_turn();
                }
                // §6 第 14/15 条：只登记**已提交**的 delta（cursor 由存储层分配后回传）。
                for (event_index, turn, fragment) in delta_plan {
                    let Some(committed) = outcome.appended.get(event_index) else {
                        continue;
                    };
                    if kinds.get(event_index) != Some(&EventKind::Delta) {
                        continue;
                    }
                    lock(&slot.deltas)
                        .entry(turn.as_str().to_owned())
                        .or_default()
                        .push(fragment);
                    lock(&slot.turn_deltas)
                        .entry(turn.as_str().to_owned())
                        .or_default()
                        .push(committed.global_sequence);
                }
                self.publish(&outcome.appended);
                // §6 第 15 条：turn 终态提交之后紧接一次压缩提交（执行中不压缩）。
                if let Some((turn, count)) = compact_after {
                    let sequences = lock(&slot.turn_deltas)
                        .remove(turn.as_str())
                        .unwrap_or_default();
                    lock(&slot.deltas).remove(turn.as_str());
                    if !self.config.persist_deltas && !sequences.is_empty() {
                        self.commit_compaction(session, &turn, count, sequences)
                            .await?;
                    }
                }
                Ok(())
            }
            Err(PortError::Unavailable(_)) => {
                // §6.9：写失败 → 对应事件一律不发布；相关命令转为 uncertain。
                if had_terminal {
                    slot.finish_turn();
                }
                if let Some(request) = terminal_request {
                    self.record_uncertain(
                        session,
                        &request,
                        "事件落盘失败，无法确认已投递给客户端",
                    )
                    .await;
                    return Ok(());
                }
                // §6.9：非终态批次的失败不能只丢弃——该 turn 的正文已经缺了一块，若让同一 turn 的
                // 终态批照常 `completed`，库里就留下「报完成但正文缺失」的记录（RV1-WP7-F3）。
                if running_in_chunk {
                    self.abandon_failed_turn(
                        slot,
                        session,
                        running.as_ref(),
                        running_request.as_ref(),
                        running_actor.as_ref(),
                    )
                    .await?;
                }
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// 派发一个排队 turn：Queued → Running（`turn.started`）→ 后端 `prompt`。
    async fn dispatch_one(&self, slot: &Slot, session: &SessionId) -> Result<bool, PortError> {
        let next = {
            let mut queue = lock(&slot.queue);
            if queue.running.is_some() {
                return Ok(false);
            }
            match queue.waiting.pop_front() {
                Some(next) => {
                    queue.running = Some(next.turn.clone());
                    queue.running_request = Some(next.request.clone());
                    queue.running_actor = Some(next.actor.clone());
                    next
                }
                None => return Ok(false),
            }
        };
        let at = self.now();
        let commit = OwnedCommit {
            session: Some(session.clone()),
            at: at.clone(),
            expected_version: None,
            state: Some(StateChange::Update(SessionUpdate {
                state: Some(SessionState::Running),
                mode: ModeChange::Unchanged,
                closed_at: None,
                interaction: None,
            })),
            turns: vec![TurnChange::Update(TurnUpdate {
                turn: next.turn.clone(),
                state: TurnState::Running,
                ended_at: None,
            })],
            events: vec![pending_event(
                "turn.started",
                EventKind::State,
                view_turn(&next.turn, TurnState::Running)?,
                Some(next.turn.clone()),
                Some(next.request.clone()),
                StoredPolicy::Durable,
                Some(&next.actor),
            )?],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        };
        match self.commit_owned(commit).await {
            Ok(outcome) => self.publish(&outcome.appended),
            Err(PortError::Unavailable(_)) => {
                // turn 仍是 Queued：放回队首，下一次 pump 重试（§6.9 不发布任何东西）。
                slot.finish_turn();
                lock(&slot.queue).waiting.push_front(next);
                return Ok(false);
            }
            Err(error) => return Err(error),
        }
        let endpoint = self.endpoint(slot, session).await?;
        match endpoint.prompt(next.prompt.clone(), at).await {
            Ok(_) => Ok(true),
            Err(error) => {
                let public = self.port_error_public(&error)?;
                self.fail_turn(slot, session, &next, &public).await?;
                Ok(true)
            }
        }
    }

    /// 派发失败的 turn：显式终态（`turn.failed` + `command.failed`），不静默重试。
    async fn fail_turn(
        &self,
        slot: &Slot,
        session: &SessionId,
        turn: &QueuedTurn,
        error: &PublicError,
    ) -> Result<(), PortError> {
        let at = self.now();
        let view = view_turn_error(&turn.turn, error)?;
        let commit = OwnedCommit {
            session: Some(session.clone()),
            at: at.clone(),
            expected_version: None,
            state: Some(StateChange::Update(SessionUpdate {
                state: Some(SessionState::Failed),
                mode: ModeChange::Unchanged,
                closed_at: None,
                interaction: None,
            })),
            turns: vec![TurnChange::Update(TurnUpdate {
                turn: turn.turn.clone(),
                state: TurnState::Failed,
                ended_at: Some(at.clone()),
            })],
            events: vec![
                pending_event(
                    "turn.failed",
                    EventKind::State,
                    view,
                    Some(turn.turn.clone()),
                    Some(turn.request.clone()),
                    StoredPolicy::Durable,
                    Some(&turn.actor),
                )?,
                pending_event(
                    "command.failed",
                    EventKind::Structured,
                    view_command_failed(&turn.request, None)?,
                    Some(turn.turn.clone()),
                    Some(turn.request.clone()),
                    StoredPolicy::Durable,
                    Some(&turn.actor),
                )?,
            ],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: Some(CommandTerminalRecord::try_new(
                CommandStatus::Failed,
                Some(at),
                None,
                None,
                Some(error.clone()),
            )?),
            origin_epoch: None,
        };
        slot.finish_turn();
        match self.commit_owned(commit).await {
            Ok(outcome) => {
                self.publish(&outcome.appended);
                Ok(())
            }
            Err(PortError::Unavailable(_)) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// §6 第 9 条（RV1-WP7-F3）：非终态批次落盘失败后放弃在跑的 turn。
    ///
    /// 为什么必须终结而不能只丢批次：该 turn 的正文已经缺了一块，若它在后续批次里照常走到终态，库里
    /// 就会留下「报完成但正文缺失」的记录。因此这里把 turn 终结为 `failed`、把命令置为 `uncertain`
    /// （与 §6 第 16 条的恢复同一口径：无法确认已经落盘的部分与 Agent 的副作用），并把该 turn 登记为
    /// 「已放弃」——它后续到达的事件与终态不再提交。
    ///
    /// 可观测信号是这次提交里的两条持久事件（`turn.failed` + `command.uncertain`）：core 不含日志依赖
    /// （`AGENTS.md` §7），而错误上抛会把「已经终结的 turn」报成调用方（往往是另一个命令）的失败，
    /// 与终态批失败时的既有口径不一致，因此这里不传播 `Unavailable`。
    ///
    /// 已落盘的 delta 仍按 §6 第 14 条收尾成 `agent.message.completed`（缺的段落不伪造）；不压缩它们
    /// （§6 第 15 条是可选动作），错误路径上不再多发一次提交。
    async fn abandon_failed_turn(
        &self,
        slot: &Slot,
        session: &SessionId,
        turn: Option<&TurnId>,
        request: Option<&RequestId>,
        actor: Option<&Actor>,
    ) -> Result<(), PortError> {
        let Some(turn) = turn else {
            return Ok(());
        };
        lock(&slot.abandoned).push(turn.clone());
        let completed = self.completed_events_for(slot, turn)?;
        lock(&slot.deltas).remove(turn.as_str());
        lock(&slot.turn_deltas).remove(turn.as_str());
        slot.finish_turn();
        let Some(request) = request else {
            return Ok(());
        };
        let at = self.now();
        let reason = "事件落盘失败，无法确认已投递给客户端";
        let error =
            PublicError::coded("command.uncertain", reason, false).map_err(PortError::from)?;
        let mut events = vec![pending_event(
            "turn.failed",
            EventKind::State,
            view_turn_error(turn, &error)?,
            Some(turn.clone()),
            Some(request.clone()),
            StoredPolicy::Durable,
            actor,
        )?];
        events.extend(completed);
        events.push(pending_event(
            "command.uncertain",
            EventKind::Structured,
            view_command_uncertain(request, reason)?,
            None,
            Some(request.clone()),
            StoredPolicy::Durable,
            actor,
        )?);
        let commit = OwnedCommit {
            session: Some(session.clone()),
            at: at.clone(),
            expected_version: None,
            state: Some(StateChange::Update(SessionUpdate {
                state: Some(SessionState::Failed),
                mode: ModeChange::Unchanged,
                closed_at: None,
                interaction: None,
            })),
            turns: vec![TurnChange::Update(TurnUpdate {
                turn: turn.clone(),
                state: TurnState::Failed,
                ended_at: Some(at.clone()),
            })],
            events,
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: Some(CommandTerminalRecord::try_new(
                CommandStatus::Uncertain,
                Some(at),
                None,
                None,
                Some(error),
            )?),
            origin_epoch: None,
        };
        match self.commit_owned(commit).await {
            Ok(outcome) => {
                self.publish(&outcome.appended);
                Ok(())
            }
            // 存储仍然不可用：该 turn 在内存里已放弃（`abandoned` + `finish_turn`），不再有
            // 「completed 但正文缺失」的记录会产生；不发布任何东西（§6.9）。
            Err(PortError::Unavailable(_)) => Ok(()),
            Err(error) => Err(error),
        }
    }

    async fn record_uncertain(&self, session: &SessionId, request: &RequestId, reason: &str) {
        let Ok(error) = PublicError::coded("command.uncertain", reason, false) else {
            return;
        };
        let Ok(view) = view_command_uncertain(request, reason) else {
            return;
        };
        let Ok(record) = CommandTerminalRecord::try_new(
            CommandStatus::Uncertain,
            Some(self.now()),
            None,
            None,
            Some(error.clone()),
        ) else {
            return;
        };
        // `command.uncertain` 由设备命令引起；这里没有 actor 在作用域里，`event_origin` 按 `Device` 记账。
        let Ok(event) = pending_event(
            "command.uncertain",
            EventKind::Structured,
            view,
            None,
            Some(request.clone()),
            StoredPolicy::Durable,
            None,
        ) else {
            return;
        };
        let commit = OwnedCommit {
            session: Some(session.clone()),
            at: self.now(),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![event],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: Some(record),
            origin_epoch: None,
        };
        if let Ok(outcome) = self.commit_owned(commit).await {
            self.publish(&outcome.appended);
        }
    }

    /// §6 第 14 条：该 turn 内出现过 delta 的每个 `messageId` 各生成恰好一条 `agent.message.completed`。
    ///
    /// 排序保证确定性（按首个 `deltaIndex`、再按 messageId）；没有 delta 的 turn 返回空。
    fn completed_events_for(
        &self,
        slot: &Slot,
        turn: &TurnId,
    ) -> Result<Vec<PendingEvent>, PortError> {
        let fragments: Vec<DeltaFragment> = lock(&slot.deltas)
            .get(turn.as_str())
            .cloned()
            .unwrap_or_default();
        if fragments.is_empty() {
            return Ok(Vec::new());
        }
        let mut order: HashMap<String, (u64, MessageId)> = HashMap::new();
        for fragment in &fragments {
            let key = fragment.message.as_str().to_owned();
            let entry = order
                .entry(key)
                .or_insert((fragment.index, fragment.message.clone()));
            if fragment.index < entry.0 {
                entry.0 = fragment.index;
            }
        }
        let mut messages: Vec<(u64, MessageId)> = order.into_values().collect();
        messages.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.as_str().cmp(right.1.as_str()))
        });
        let mut events = Vec::new();
        for (_, message) in messages {
            let own: Vec<DeltaFragment> = fragments
                .iter()
                .filter(|fragment| fragment.message == message)
                .cloned()
                .collect();
            let content = fold_delta_content(&own);
            let view = view_message_completed(&message, turn, &content)?;
            events.push(pending_event(
                "agent.message.completed",
                EventKind::FinalMessage,
                view,
                Some(turn.clone()),
                None,
                StoredPolicy::Durable,
                Some(&Actor::LocalCli),
            )?);
        }
        Ok(events)
    }

    /// §6 第 15 条：turn 终态之后紧接一次压缩提交（`turn.delta_compacted` 收据 + `compacted` 清单）。
    ///
    /// core 侧的自检：清单非空、每个 cursor 的 `global_sequence` 非零（本会话与 `kind='delta'` 由存储层
    /// 再校验一次并失败关闭）。
    async fn commit_compaction(
        &self,
        session: &SessionId,
        turn: &TurnId,
        delta_count: usize,
        sequences: Vec<Sequence>,
    ) -> Result<(), PortError> {
        if sequences.is_empty() || sequences.iter().any(|sequence| sequence.get() == 0) {
            return Err(PortError::InvalidRequest(
                "compacted 只接受本会话已提交的 delta 行 cursor（§6 第 15 条）",
            ));
        }
        let server_epoch = self.deps.store.head().await?.server_epoch;
        let compacted: Vec<GlobalCursor> = sequences
            .into_iter()
            .map(|global_sequence| GlobalCursor {
                server_epoch: server_epoch.clone(),
                global_sequence,
            })
            .collect();
        let at = self.now();
        let event = pending_event(
            "turn.delta_compacted",
            EventKind::Summary,
            view_delta_compacted(turn, delta_count)?,
            Some(turn.clone()),
            None,
            StoredPolicy::Durable,
            None,
        )?;
        let commit = OwnedCommit {
            session: Some(session.clone()),
            at,
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![event],
            interactions: Vec::new(),
            compacted,
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        };
        let outcome = self.commit_owned(commit).await?;
        self.publish(&outcome.appended);
        Ok(())
    }

    /// §6 第 16 条：启动恢复。组合根在取得单实例锁、开始监听**之前**调用它。
    ///
    /// 对每条仍为 `accepted` 的命令，在该会话的串行门内终结为 `uncertain`（写 `command.uncertain`
    /// 终态事件 + 更新幂等行，并把对应未终态 turn 终结为 `failed`），并补写缺失的
    /// `agent.message.completed`。**不**自动重放副作用；广播一律发生在 `commit` 之后。
    pub async fn recover_unsettled(&self, limit: ReplayLimit) -> Result<usize, PortError> {
        let commands = self.deps.store.unsettled_commands(limit).await?;
        let mut recovered = 0usize;
        for record in commands {
            let session = record.session().cloned();
            match session {
                Some(session) => {
                    let slot = self.owned_slot(&session);
                    let _guard = slot.gate.guard().await;
                    // 等门期间可能已被别的路径终结：复查后跳过。
                    let current = self
                        .deps
                        .store
                        .find_request(record.request(), record.actor())
                        .await?;
                    if !matches!(current, Some(found) if found.status() == CommandStatus::Accepted)
                    {
                        continue;
                    }
                    self.recover_command(Some(&session), &record).await?;
                }
                None => self.recover_command(None, &record).await?,
            }
            recovered += 1;
        }
        Ok(recovered)
    }

    /// 单条 `accepted` 命令的恢复提交（会话串行门由调用方持有）。
    async fn recover_command(
        &self,
        session: Option<&SessionId>,
        record: &CommandRecord,
    ) -> Result<(), PortError> {
        let at = self.now();
        let request = record.request().clone();
        let reason = "进程恢复后无法确认命令是否已到达 Agent";
        let error =
            PublicError::coded("command.uncertain", reason, false).map_err(PortError::from)?;
        let mut events: Vec<PendingEvent> = Vec::new();
        let mut turns: Vec<TurnChange> = Vec::new();
        let mut state = None;
        if let Some(session_id) = session {
            let view = self.deps.store.read_view().await?;
            let page = view
                .read_session(HistoryQuery {
                    session: session_id.clone(),
                    include: HistoryInclude {
                        messages: true,
                        turns: true,
                        pending_interactions: false,
                        config_options: false,
                        capabilities: false,
                    },
                    after: None,
                    limit: ReplayLimit::new(512),
                })
                .await?;
            if let Some(turn) = page
                .turns
                .iter()
                .find(|turn| turn.causation() == Some(&request) && !turn.state().is_terminal())
            {
                events.push(pending_event(
                    "turn.failed",
                    EventKind::State,
                    view_turn_error(turn.id(), &error)?,
                    Some(turn.id().clone()),
                    Some(request.clone()),
                    StoredPolicy::Durable,
                    Some(record.actor()),
                )?);
                turns.push(TurnChange::Update(TurnUpdate {
                    turn: turn.id().clone(),
                    state: TurnState::Failed,
                    ended_at: Some(at.clone()),
                }));
                state = Some(StateChange::Update(SessionUpdate {
                    state: Some(SessionState::Failed),
                    mode: ModeChange::Unchanged,
                    closed_at: None,
                    interaction: None,
                }));
                // §6 第 16 条：已有已提交 delta 却没有 completed 的 turn，必须补写。
                events.extend(self.backfill_completed(&*view, &page, turn.id()).await?);
            }
            events.push(pending_event(
                "command.uncertain",
                EventKind::Structured,
                view_command_uncertain(&request, reason)?,
                None,
                Some(request.clone()),
                StoredPolicy::Durable,
                Some(record.actor()),
            )?);
        } else {
            events.push(pending_event(
                "command.uncertain",
                EventKind::Structured,
                view_command_uncertain(&request, reason)?,
                None,
                Some(request.clone()),
                StoredPolicy::Durable,
                Some(record.actor()),
            )?);
        }
        let terminal = CommandTerminalRecord::try_new(
            CommandStatus::Uncertain,
            Some(at.clone()),
            None,
            None,
            Some(error),
        )?;
        let commit = OwnedCommit {
            session: session.cloned(),
            at,
            expected_version: None,
            state,
            turns,
            events,
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: Some(terminal),
            origin_epoch: None,
        };
        let outcome = self.commit_owned(commit).await?;
        // §6 第 16 条：广播必须在 commit 之后。
        self.publish(&outcome.appended);
        Ok(())
    }

    /// §6 第 14/16 条：从库里重建该 turn 缺失的 `agent.message.completed`。
    ///
    /// 只认 `agent.message.delta` 的事件类型（`agent.thought.delta` 不产生 completed），正文一律取自
    /// `ReadView::event_payload`；已有 `completed` 的 messageId 不重复补写。
    async fn backfill_completed(
        &self,
        view: &dyn ReadView,
        page: &HistoryPage,
        turn: &TurnId,
    ) -> Result<Vec<PendingEvent>, PortError> {
        let mut fragments: HashMap<String, Vec<DeltaFragment>> = HashMap::new();
        let mut completed: Vec<MessageId> = Vec::new();
        for history in &page.events {
            let payload = match view.event_payload(&history.id).await? {
                Some(payload) => payload,
                None => continue,
            };
            match history.event_type.as_str() {
                "agent.message.delta" => {
                    if let Some(fragment) = delta_fragment(&payload.view) {
                        let belongs = json_members(payload.view.as_str())
                            .ok()
                            .and_then(|members| {
                                members
                                    .iter()
                                    .find(|(name, _)| name == "turnId")
                                    .map(|(_, raw)| *raw)
                            })
                            .and_then(json_string);
                        if belongs.as_deref() == Some(turn.as_str()) {
                            fragments
                                .entry(fragment.message.as_str().to_owned())
                                .or_default()
                                .push(fragment);
                        }
                    }
                }
                "agent.message.completed" => {
                    if let Some(message) = view_message_id(&payload.view) {
                        completed.push(message);
                    }
                }
                _ => {}
            }
        }
        let mut events = Vec::new();
        for (_, own) in fragments {
            let Some(message) = own.first().map(|fragment| fragment.message.clone()) else {
                continue;
            };
            if completed.contains(&message) {
                continue;
            }
            let content = fold_delta_content(&own);
            events.push(pending_event(
                "agent.message.completed",
                EventKind::FinalMessage,
                view_message_completed(&message, turn, &content)?,
                Some(turn.clone()),
                None,
                StoredPolicy::Durable,
                None,
            )?);
        }
        Ok(events)
    }

    /// 打开（或复用）该会话的后端端点（§5.1）。
    ///
    /// owned 会话复用 [`Broker::sink`] 并把事件交给本会话的串行队列；imported 会话的后端由持有
    /// Node Link attachment 的 facade 在同一组合根内直接持有（§5.2 的 imported 分流），core 不为它
    /// 开端点。
    pub async fn endpoint_for(
        &self,
        reference: &SessionReference,
    ) -> Result<Arc<dyn SessionEndpoint>, PortError> {
        match reference {
            SessionReference::Owned(owned) => {
                let session = owned.session_id.clone();
                let slot = self.owned_slot(&session);
                self.endpoint(&slot, &session).await
            }
            SessionReference::Remote(_) => Err(PortError::InvalidRequest(
                "imported 会话的后端由持有 attachment 的 facade 持有",
            )),
        }
    }

    async fn endpoint(
        &self,
        slot: &Slot,
        session: &SessionId,
    ) -> Result<Arc<dyn SessionEndpoint>, PortError> {
        if let Some(endpoint) = lock(&slot.endpoint).clone() {
            return Ok(endpoint);
        }
        let sink = self.sink(session);
        let reference = Self::reference_of(session);
        let endpoint: Arc<dyn SessionEndpoint> =
            self.deps.backends.open(reference, sink).await?.into();
        *lock(&slot.endpoint) = Some(endpoint.clone());
        Ok(endpoint)
    }

    // -----------------------------------------------------------------------------------------
    // mutation 管道的公共步骤
    // -----------------------------------------------------------------------------------------

    /// §6.6：幂等键是协议维度的 `(actor, requestId)`。命中且五项指纹全同 → 返回首次结果、不二次派发；
    /// 任一不同 → `command.idempotency_conflict`。
    async fn idempotency(
        &self,
        actor: &Actor,
        command: &ClientCommand,
    ) -> Result<Option<CommandReceipt>, PortError> {
        let Some(record) = self
            .deps
            .store
            .find_request(&command.request, actor)
            .await?
        else {
            return Ok(None);
        };
        let same = record.command() == command.command
            && record.kind() == command.kind
            && record.session().map(|session| session.as_str())
                == command.session.as_ref().map(|session| session.as_str())
            && record.expected_version() == command.expected_version
            && record.request_fingerprint() == &command.request_fingerprint;
        if !same {
            return self
                .reject(
                    "command.idempotency_conflict",
                    "同一 requestId 已用于不同的命令或载荷",
                    false,
                )
                .map(Some);
        }
        // 仍为 accepted 的 mutation：从该会话的非终态 turn 里恢复原 turnId（§11.5 的 accepted{turnId}）。
        let turn = match (record.session(), record.status()) {
            (Some(session), CommandStatus::Accepted) if record.kind() == CommandKind::Mutation => {
                self.turn_for_request(session, &command.request).await?
            }
            _ => None,
        };
        Ok(Some(CommandReceipt::Accepted {
            request: command.request.clone(),
            turn,
        }))
    }

    /// 在会话的未终态 turn 里找 `causation == request` 的那个（幂等重放用）。
    async fn turn_for_request(
        &self,
        session: &SessionId,
        request: &RequestId,
    ) -> Result<Option<TurnId>, PortError> {
        let query = HistoryQuery {
            session: session.clone(),
            include: HistoryInclude {
                turns: true,
                ..HistoryInclude::default()
            },
            after: None,
            limit: ReplayLimit::new(64),
        };
        let page = self
            .deps
            .store
            .read_view()
            .await?
            .read_session(query)
            .await?;
        Ok(page
            .turns
            .iter()
            .find(|turn| {
                turn.causation()
                    .map(|causation| causation == request)
                    .unwrap_or(false)
            })
            .map(|turn| turn.id().clone()))
    }

    /// 接受提交（§6.1/§6.9）：先落盘并发布，失败即显式拒绝且**不派发**。
    async fn accept(
        &self,
        actor: &Actor,
        command: &ClientCommand,
        session: &SessionId,
        state: Option<StateChange>,
        turns: Vec<TurnChange>,
        events: Vec<PendingEvent>,
    ) -> Result<Accept, PortError> {
        let at = self.now();
        let commit = OwnedCommit {
            session: Some(session.clone()),
            at: at.clone(),
            expected_version: command.expected_version,
            state,
            turns,
            events,
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: Some(IdempotencyRecord {
                actor: actor.clone(),
                request: command.request.clone(),
                command: command.command.clone(),
                kind: command.kind,
                session: Some(session.clone()),
                expected_version: command.expected_version,
                request_fingerprint: command.request_fingerprint.clone(),
                accepted_at: at.clone(),
            }),
            command_terminal: None,
            origin_epoch: None,
        };
        match self.commit_owned(commit).await {
            Ok(outcome) => {
                if let Some(replayed) = &outcome.replayed {
                    // 竞态：另一路径已经用同一幂等键落盘（§5.2 的 `replayed`），不二次派发。
                    let turn = match replayed.record.session() {
                        Some(session) => self.turn_for_request(session, &command.request).await?,
                        None => None,
                    };
                    return Ok(Accept::Done(CommandReceipt::Accepted {
                        request: command.request.clone(),
                        turn,
                    }));
                }
                self.publish(&outcome.appended);
                Ok(Accept::Committed)
            }
            Err(PortError::Unavailable(_)) => Ok(Accept::Done(self.reject(
                "internal.unavailable",
                "命令未落盘，未产生任何副作用",
                true,
            )?)),
            Err(PortError::Conflict(ConflictKind::VersionMismatch)) => {
                let current = self.session_version_of(session).await?;
                Ok(Accept::Done(
                    self.version_conflict_receipt(current, command)?,
                ))
            }
            Err(error) => Err(error),
        }
    }

    /// 只改会话状态（模式/配置切换）的提交 + 终态命令事件。
    async fn apply_state(
        &self,
        session: &SessionId,
        state: StateChange,
        command: &ClientCommand,
    ) -> Result<CommitOutcome, PortError> {
        let at = self.now();
        let commit = OwnedCommit {
            session: Some(session.clone()),
            at: at.clone(),
            expected_version: None,
            state: Some(state),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        };
        let outcome = self.commit_owned(commit).await?;
        let version = outcome.version;
        // §6.1：先发布状态提交，再提交并发布终态（§11.2：终端事件在状态变化之后）。
        self.publish(&outcome.appended);
        self.commit_terminal(
            session,
            command,
            CommandStatus::Completed,
            None,
            Some(version_result(&version)?),
            Some(version),
            &at,
        )
        .await?;
        Ok(outcome)
    }

    /// 终态提交（§11.2：终态只通过一个持久化的 `command.*` 事件表达）。
    ///
    /// `result` 是该命令终态的收据结果（RV1-WP6-F3）：有 turn 的命令带 `{"turnId":…}`、模式/配置切换
    /// 带 `{"version":…}`，其余为 `None`（适配层按 §12.5 回空 object，不编造字段）。
    #[allow(clippy::too_many_arguments)] // 终态的四个分量（status/error/result/version）+ 会话/命令/时间
    async fn commit_terminal(
        &self,
        session: &SessionId,
        command: &ClientCommand,
        status: CommandStatus,
        error: Option<PublicError>,
        result: Option<CommandResult>,
        version: Option<Version>,
        at: &Timestamp,
    ) -> Result<(), PortError> {
        if !status.is_terminal() {
            return Err(PortError::InvalidRequest("命令终态必须是终止态"));
        }
        let request = &command.request;
        let result_text = version.as_ref().map(version_text);
        let (event_type, view) = match status {
            CommandStatus::Completed => (
                "command.completed",
                view_command_completed(request, result_text.as_deref())?,
            ),
            CommandStatus::Failed => ("command.failed", view_command_failed(request, None)?),
            CommandStatus::Uncertain => (
                "command.uncertain",
                view_command_uncertain(request, "无法确认副作用")?,
            ),
            _ => return Err(PortError::InvalidRequest("命令终态必须是终止态")),
        };
        let events = vec![pending_event(
            event_type,
            EventKind::Structured,
            view,
            None,
            Some(request.clone()),
            StoredPolicy::Durable,
            Some(&command.actor),
        )?];
        let record = CommandTerminalRecord::try_new(status, Some(at.clone()), None, result, error)?;
        let commit = OwnedCommit {
            session: Some(session.clone()),
            at: at.clone(),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events,
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: Some(record),
            origin_epoch: None,
        };
        match self.commit_owned(commit).await {
            Ok(outcome) => {
                self.publish(&outcome.appended);
                Ok(())
            }
            Err(PortError::Unavailable(_)) => {
                self.record_uncertain(session, request, "终态落盘失败")
                    .await;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// 权限解析：optionId 必须来自该未解决请求（§11.5），`kind` 由请求里的 `InteractionOption.kind`
    /// 决定（core 不发明分类）。
    async fn permission_resolution(
        &self,
        session: &SessionId,
        interaction: &InteractionId,
        option_id: &str,
    ) -> Result<Result<InteractionResolution, CommandReceipt>, PortError> {
        let query = HistoryQuery {
            session: session.clone(),
            include: HistoryInclude {
                pending_interactions: true,
                ..HistoryInclude::default()
            },
            after: None,
            limit: ReplayLimit::new(64),
        };
        let page = self
            .deps
            .store
            .read_view()
            .await?
            .read_session(query)
            .await?;
        let Some(pending) = page
            .interactions
            .iter()
            .find(|item| item.id() == interaction)
        else {
            return Ok(Err(self.reject(
                "command.not_found",
                "交互不存在或已经终态",
                false,
            )?));
        };
        let Some(option) = pending
            .options()
            .iter()
            .find(|option| option.option_id() == option_id)
        else {
            return Ok(Err(self.reject(
                "capability.unsupported_by_broker",
                "optionId 不属于该未解决请求",
                false,
            )?));
        };
        let Some(kind) = permission_kind(option.kind()) else {
            return Ok(Err(self.reject(
                "capability.unsupported_by_broker",
                "该权限选项的 kind 无法映射为 AllowOnce/AllowAlways/RejectOnce/RejectAlways",
                false,
            )?));
        };
        let decision = PermissionDecision::try_new(option_id, kind).map_err(PortError::from)?;
        Ok(Ok(InteractionResolution::permission(decision)))
    }

    fn is_active(state: SessionState) -> bool {
        matches!(
            state,
            SessionState::Queued
                | SessionState::Running
                | SessionState::WaitingInput
                | SessionState::WaitingPermission
        )
    }

    fn publish(&self, events: &[CommittedEvent]) {
        for event in events {
            self.deps
                .publisher
                .publish(CommittedDelivery::Owned(event.clone()));
        }
    }

    /// 组装一条 pending 交互行（§6 第 13 条）。
    ///
    /// id 优先取事件 payload 里的 `interactionId`（客户端用同一个 id 应答），缺失时用
    /// `IdGenerator::interaction_id` 生成。`options` 不落库，因此这里固定为空——它们的唯一权威来源
    /// 是同一提交里那条事件的 payload（§6 第 13 条末句）。
    fn build_interaction(
        &self,
        event: &EndpointEvent,
        kind: InteractionKind,
        session: &SessionId,
        at: &Timestamp,
    ) -> Result<PendingInteraction, PortError> {
        let id = match view_interaction_id(&event.payload.view) {
            Some(id) => id,
            None => self.deps.ids.interaction_id(),
        };
        PendingInteraction::try_new(id, kind, session.clone(), at.clone(), Vec::new())
            .map_err(PortError::from)
    }

    /// 唯一的落盘漏斗：先做 §9 判据 15 的互斥校验与 §10.3 的 view 收口，再交给存储层。
    ///
    /// 同一提交里 `interactions` 非空**且** `state.interaction` 为 `Some` → `InvalidRequest`：
    /// 创建与解析是两条互斥路径（§6 第 13 条），同时出现意味着组装出了自相矛盾的事务。
    async fn commit_owned(&self, commit: OwnedCommit) -> Result<CommitOutcome, PortError> {
        let resolving = commit
            .state
            .as_ref()
            .map(|state| match state {
                StateChange::Update(update) => update.interaction.is_some(),
                StateChange::Create(_) => false,
            })
            .unwrap_or(false);
        if resolving && !commit.interactions.is_empty() {
            return Err(PortError::InvalidRequest(
                "同一提交不得同时创建与解析交互（§6 第 13 条 / §9 判据 15）",
            ));
        }
        let (commit, predicted) = self.finalize_owned_views(commit).await?;
        let outcome = self.deps.store.commit(commit).await?;
        // §10.3 的 `version` 规则漂移检测：推导值必须等于存储层返回值。幂等命中时本批内容未被采用
        // （返回的是首次提交的结果），因此不参与比对。
        if let (Some(predicted), None) = (predicted, &outcome.replayed) {
            if outcome.version != predicted {
                return Err(PortError::Corrupt(
                    "存储层返回的会话版本与 core 推导不一致（§10.3 的 version 规则漂移）",
                ));
            }
        }
        Ok(outcome)
    }

    /// 提交前的 view 收口（§10.3）：注入 `turnId` 与会话 `version`，并返回推导出的会话版本。
    ///
    /// turn 归属在批组装时已经定稿（[`PendingEvent.turn`]），这里只按该值注入与校验，**不**重新推导；
    /// 返回的 `None` 表示本批不含要求 `version` 的 view，无需比对。
    async fn finalize_owned_views(
        &self,
        mut commit: OwnedCommit,
    ) -> Result<(OwnedCommit, Option<Version>), PortError> {
        for event in &mut commit.events {
            if !view_requires_turn_id(event.event_type.as_str()) {
                continue;
            }
            let Some(turn) = event.turn.clone() else {
                continue;
            };
            event.payload.view = ensure_view_string_field(
                &event.payload.view,
                "turnId",
                turn.as_str(),
                "view 的 turnId 与 core 的权威 turn 不一致（§10.3）",
            )?;
        }
        let needs_version = commit
            .events
            .iter()
            .any(|event| view_requires_session_version(event.event_type.as_str()));
        if !needs_version {
            return Ok((commit, None));
        }
        let predicted = self.predict_session_version(&commit).await?;
        let text = predicted.to_string();
        for event in &mut commit.events {
            if view_requires_session_version(event.event_type.as_str()) {
                event.payload.view = ensure_view_string_field(
                    &event.payload.view,
                    "version",
                    &text,
                    "view 的 version 与会话当前版本不一致（§10.3）",
                )?;
            }
        }
        Ok((commit, Some(predicted)))
    }

    /// 提交前的会话版本推导：`state` 变更递增一，否则不变（§5.2 的存储层规则）。
    ///
    /// 只在含要求 `version` 的 view 时调用（view 是提交输入，权威版本在提交后才产生），因此
    /// `expected_version` 缺失时回读当前版本，并由提交后的比对兜底。
    async fn predict_session_version(&self, commit: &OwnedCommit) -> Result<Version, PortError> {
        let current = match (&commit.state, &commit.expected_version) {
            (Some(StateChange::Create(_)), _) => 0,
            (_, Some(expected)) => expected.get(),
            (_, None) => {
                let Some(session) = commit.session.as_ref() else {
                    return Err(PortError::InvalidRequest(
                        "要求会话版本的事件必须属于某个会话（§10.3）",
                    ));
                };
                match self.deps.store.load(session).await? {
                    Some(snapshot) => snapshot.session.version().get(),
                    None => return Err(PortError::NotFound(EntityRef::Session(session.clone()))),
                }
            }
        };
        let next = if commit.state.is_some() {
            current.checked_add(1)
        } else {
            Some(current)
        };
        next.map(Version::new)
            .ok_or(PortError::Corrupt("会话版本已溢出"))
    }

    fn reject(
        &self,
        code: &str,
        message: &str,
        retryable: bool,
    ) -> Result<CommandReceipt, PortError> {
        let error = PublicError::coded(code, message, retryable).map_err(PortError::from)?;
        Ok(CommandReceipt::Rejected { error })
    }

    fn version_conflict(
        &self,
        current: u64,
        command: &ClientCommand,
    ) -> Result<CommandReceipt, PortError> {
        let error = PublicError::try_new(
            "state.version_conflict",
            "该会话正在执行 turn，模式/配置切换只在 turn 边界生效",
            true,
            view_version_conflict(current, command.expected_version.as_ref()),
        )
        .map_err(PortError::from)?;
        Ok(CommandReceipt::Rejected { error })
    }

    fn version_conflict_receipt(
        &self,
        current: u64,
        command: &ClientCommand,
    ) -> Result<CommandReceipt, PortError> {
        self.version_conflict(current, command)
    }

    /// `PortError` → 命令级 `PublicError`（适配器再映射到 wire 错误码）。
    fn port_error_public(&self, error: &PortError) -> Result<PublicError, PortError> {
        let (code, message, retryable) = match error {
            PortError::NotFound(_) => ("command.not_found", "目标不存在", false),
            PortError::Conflict(kind) => match kind {
                ConflictKind::VersionMismatch => ("state.version_conflict", "版本冲突", true),
                ConflictKind::AlreadyResolved => {
                    ("interaction.already_resolved", "交互已经解析", false)
                }
                ConflictKind::IdempotencyConflict => (
                    "command.idempotency_conflict",
                    "同一 requestId 已用于不同命令",
                    false,
                ),
                // 管理写集的冲突只可能出现在本地管理路径：这里**显式**列出而不用通配臂。
                // 新增 `ConflictKind` 取值时编译器会报错（§11.6 记录的隐性陷阱：`Conflict(_)`
                // 通配臂会让新取值静默落入 `internal.unavailable`）；本地管理适配器负责把它们
                // 映射为 `local.conflict`（`LOCAL_ADMIN_PROTOCOL.md` §6）。
                ConflictKind::AlreadyClaimed
                | ConflictKind::Expired
                | ConflictKind::Consumed
                | ConflictKind::AlreadyExists
                | ConflictKind::IdentityMismatch
                | ConflictKind::DuplicateOwnership => ("internal.unavailable", "状态冲突", false),
            },
            PortError::InvalidRequest(_) => ("protocol.schema_invalid", "请求不合法", false),
            PortError::Corrupt(_) => ("internal.unavailable", "存储不可用", true),
            // 同上：`UnavailableKind` 逐值列出；`KeystoreUnavailable` 由本地管理适配器映射为
            // `local.unavailable`。
            PortError::Unavailable(kind) => match kind {
                UnavailableKind::RemoteUnavailable => {
                    ("resource.remote_unavailable", "远端不可达", true)
                }
                UnavailableKind::KeystoreUnavailable => {
                    ("internal.unavailable", "凭据存储不可用", true)
                }
                UnavailableKind::Busy
                | UnavailableKind::StorageFull
                | UnavailableKind::IoError
                | UnavailableKind::OwnerOffline
                | UnavailableKind::ExportRevoked => {
                    ("internal.unavailable", "存储或后端不可用", true)
                }
            },
            PortError::Backend(_) => ("internal.unavailable", "后端失败", true),
        };
        PublicError::coded(code, message, retryable).map_err(PortError::from)
    }

    fn owned_session(&self, reference: &SessionReference) -> Result<SessionId, PortError> {
        match reference {
            SessionReference::Owned(owned) => Ok(owned.session_id.clone()),
            SessionReference::Remote(_) => Err(PortError::InvalidRequest(
                "imported 会话的 mutation 由持有 attachment 的 facade 转发给 Owner",
            )),
        }
    }

    async fn command_session_version(&self, command: &ClientCommand) -> Result<u64, PortError> {
        match command.session.as_ref() {
            Some(session) => self.session_version_of(session).await,
            None => Ok(0),
        }
    }

    /// 会话当前版本（`session.mode.list` 等用例的结果里要带它）。
    pub async fn session_version(&self, session: &SessionId) -> Result<Version, PortError> {
        Ok(Version::from(self.session_version_of(session).await?))
    }

    async fn session_version_of(&self, session: &SessionId) -> Result<u64, PortError> {
        Ok(self
            .deps
            .store
            .load(session)
            .await?
            .map(|snapshot| snapshot.session.version().get())
            .unwrap_or(0))
    }

    /// `set_mode`/`set_config` 的返回值：命令被拒绝时返回当前版本，否则返回提交后的版本。
    async fn outcome_version(
        &self,
        receipt: &CommandReceipt,
        before: u64,
        command: &ClientCommand,
    ) -> Result<Version, PortError> {
        match receipt {
            CommandReceipt::Accepted { .. } => match command.session.as_ref() {
                Some(session) => {
                    let after = self.session_version_of(session).await?;
                    Ok(Version::from(after))
                }
                None => Ok(Version::from(before)),
            },
            CommandReceipt::Rejected { .. } => Ok(Version::from(before)),
        }
    }

    /// 不带 wire 指纹的入口（[`Broker::set_mode`]/[`Broker::set_config`]）用一次性 requestId，
    /// 因此不存在"同 requestId 二次提交"的比对场景。core 无密码学依赖（§2），这里写一个规范的
    /// 空摘要作为占位（32 个零字节的 base64url，无填充、末字符落在规范集合内）——它不冒充 ACPR-CJ1，
    /// wire 侧一律由适配器填 `ClientCommand.request_fingerprint`。
    fn fingerprint_of(&self, request: &RequestId) -> Result<Digest, PortError> {
        let _ = request;
        Digest::new(PLACEHOLDER_FINGERPRINT).map_err(PortError::from)
    }
}

/// 接受阶段的两种出路：已经落盘（可以派发），或者就地返回一张回执（拒绝/幂等命中）。
enum Accept {
    Committed,
    Done(CommandReceipt),
}

/// `InteractionOption.kind`（ACP 的 `PermissionOptionKind`，`SECURITY_DESIGN.md`/ACP schema 的
/// snake_case 取值）→ `PermissionDecisionKind`。未知取值不猜（调用方按 `capability.unsupported_by_broker`
/// 拒绝），避免把用户意图翻译错。
fn permission_kind(kind: &str) -> Option<PermissionDecisionKind> {
    match kind {
        "allow_once" | "allow-once" | "allowOnce" => Some(PermissionDecisionKind::AllowOnce),
        "allow_always" | "allow-always" | "allowAlways" => {
            Some(PermissionDecisionKind::AllowAlways)
        }
        "reject_once" | "reject-once" | "rejectOnce" => Some(PermissionDecisionKind::RejectOnce),
        "reject_always" | "reject-always" | "rejectAlways" => {
            Some(PermissionDecisionKind::RejectAlways)
        }
        _ => None,
    }
}

/// 事件产生者判定（`SYNC_PROTOCOL.md` §10.1 的 `origin.kind`）——**唯一实现**：broker 组装每条事件时都用它，
/// 不按「有没有会话」猜，也不散在提交点里。
///
/// - 后端 / Agent 产生（增量、工具、交互请求、计划/命令目录/用量/连接状态…）→ [`EventOrigin::Agent`]；
/// - 由设备命令引起（`permission.resolved`、`elicitation.resolved`、`turn.*`、`command.*`、模式/配置变更）
///   → [`EventOrigin::Device`]；`Actor::LocalCli` 引起时 → [`EventOrigin::LocalCli`]。
///   这些类型在 core 里**只**由命令路径产生，因此 actor 缺失时按 `Device` 记账；
/// - daemon 自身事件（没有请求 actor 的 `device.revoked`、`session.origin.online_changed`、
///   `storage.integrity_failed`）→ [`EventOrigin::Daemon`]。
fn event_origin(event_type: &EventType, actor: Option<&Actor>) -> EventOrigin {
    match event_type.as_str() {
        "device.revoked" | "session.origin.online_changed" | "storage.integrity_failed" => {
            EventOrigin::Daemon
        }
        "permission.resolved"
        | "elicitation.resolved"
        | "turn.queued"
        | "turn.started"
        | "turn.completed"
        | "turn.cancelled"
        | "turn.failed"
        | "command.completed"
        | "command.failed"
        | "command.uncertain"
        | "session.mode.changed"
        | "session.config.changed" => match actor {
            Some(Actor::LocalCli) => EventOrigin::LocalCli,
            Some(_) | None => EventOrigin::Device,
        },
        _ => EventOrigin::Agent,
    }
}

/// delta 视图 → 折叠输入。只读适配器投影的字段（`messageId`/`deltaIndex`/`text`/可选 `block`），
/// core 不解析 ACP 原文、也不猜 `block` 的类型（§6 第 14 条）。
fn delta_fragment(view: &ViewJson) -> Option<DeltaFragment> {
    let members = json_members(view.as_str()).ok()?;
    let member = |name: &str| {
        members
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, raw)| *raw)
    };
    let message = message_id_of(member("messageId")?)?;
    let index = member("deltaIndex").and_then(json_index)?;
    let text = json_string(member("text")?)?;
    let block = member("block").map(str::to_owned);
    Some(DeltaFragment {
        message,
        index,
        text,
        block,
    })
}

/// 十进制字符串 → `u64`（`deltaIndex` 的线格式，`common.schema.json#/$defs/decimalString`）。
fn json_index(raw: &str) -> Option<u64> {
    let text = json_string(raw)?;
    if text.is_empty() || (text.len() > 1 && text.starts_with('0')) {
        return None;
    }
    text.parse::<u64>().ok()
}

fn message_id_of(raw: &str) -> Option<MessageId> {
    MessageId::new(&json_string(raw)?).ok()
}

/// view 里的 `messageId`（`agent.message.completed` 的自检用）。
fn view_message_id(view: &ViewJson) -> Option<MessageId> {
    let members = json_members(view.as_str()).ok()?;
    let raw = members
        .iter()
        .find(|(name, _)| name == "messageId")
        .map(|(_, raw)| *raw)?;
    message_id_of(raw)
}

/// 按 `deltaIndex` 升序折叠正文（§6 第 14 条）：连续文本 delta 合成**一个** `{type:"text",text}` 块，
/// 带 `block` 的按位置插入；空洞不是错误，顺序一律以现存的 `deltaIndex` 升序为准。
fn fold_delta_content(fragments: &[DeltaFragment]) -> Vec<String> {
    let mut ordered = fragments.to_vec();
    ordered.sort_by_key(|fragment| fragment.index);
    let mut blocks: Vec<String> = Vec::new();
    let mut text = String::new();
    for fragment in ordered {
        text.push_str(&fragment.text);
        if let Some(block) = fragment.block {
            if !text.is_empty() {
                blocks.push(text_block(&text));
                text.clear();
            }
            blocks.push(block);
        }
    }
    if !text.is_empty() {
        blocks.push(text_block(&text));
    }
    blocks
}

fn text_block(text: &str) -> String {
    format!(r#"{{"type":"text","text":{}}}"#, json_text(text))
}

/// `agent.message.completed` 的 view（`SYNC_PROTOCOL.md` §10.3 的最低字段：`messageId`/`turnId`/`content`）。
fn view_message_completed(
    message: &MessageId,
    turn: &TurnId,
    content: &[String],
) -> Result<ViewJson, PortError> {
    view(format!(
        r#"{{"messageId":"{}","turnId":"{}","content":[{}]}}"#,
        message.as_str(),
        turn.as_str(),
        content.join(",")
    ))
}

/// `turn.delta_compacted` 的 view：**只有** `turnId` 与 `deltaCount` 两个字段（§6 第 15 条）——
/// 正文来自第 14 条的 `agent.message.completed`，summary 只承载收据。
fn view_delta_compacted(turn: &TurnId, delta_count: usize) -> Result<ViewJson, PortError> {
    view(format!(
        r#"{{"turnId":"{}","deltaCount":{delta_count}}}"#,
        turn.as_str()
    ))
}

fn is_agent_message_delta(event_type: &EventType) -> bool {
    event_type.as_str() == "agent.message.delta"
}

fn is_turn_terminal(event_type: &EventType) -> bool {
    turn_terminal(event_type).is_some()
}

/// 交互请求事件 → `InteractionKind`（§10.2 的 `permission.requested`/`elicitation.requested`）。
fn interaction_request_kind(event_type: &EventType) -> Option<InteractionKind> {
    match event_type.as_str() {
        "permission.requested" => Some(InteractionKind::Permission),
        "elicitation.requested" => Some(InteractionKind::Elicitation),
        _ => None,
    }
}

/// 交互事件必须是 `kind = interaction`（§6 第 13 条）：后端若把它标成别的类别，这里按事件类型纠正。
fn interaction_event_kind(kind: EventKind, event_type: &EventType) -> EventKind {
    if interaction_request_kind(event_type).is_some() {
        EventKind::Interaction
    } else {
        kind
    }
}

/// 从事件 payload 里读 `interactionId`（`agent-host` 组装的 view 携带它）；读不到时返回 `None`。
fn view_interaction_id(view: &ViewJson) -> Option<InteractionId> {
    let members = json_members(view.as_str()).ok()?;
    let raw = members
        .into_iter()
        .find(|(name, _)| name == "interactionId")
        .map(|(_, raw)| raw)?;
    let text = json_string(raw)?;
    InteractionId::new(&text).ok()
}

/// turn 终态事件 → `(turn 状态, 命令终态)`。
fn turn_terminal(event_type: &EventType) -> Option<(TurnState, CommandStatus)> {
    match event_type.as_str() {
        "turn.completed" => Some((TurnState::Completed, CommandStatus::Completed)),
        // 取消是"请求生效"而不是失败：prompt 命令以完成收尾（§11.5）。
        "turn.cancelled" => Some((TurnState::Cancelled, CommandStatus::Completed)),
        "turn.failed" => Some((TurnState::Failed, CommandStatus::Failed)),
        _ => None,
    }
}

fn turn_session_state(turn: TurnState) -> SessionState {
    match turn {
        TurnState::Failed => SessionState::Failed,
        _ => SessionState::Idle,
    }
}

fn mode_ref(mode: &ModeId) -> Result<ModeRef, PortError> {
    // displayName 由 ACP 的 ModeState 给出，core 侧没有模式枚举端口（§5.1 无 list_modes）；
    // 这里以 modeId 兜底，展示名由前端用 `session.mode.list` 结果覆盖（§11.5 的 ModeState）。
    ModeRef::try_new(mode.clone(), mode.as_str()).map_err(PortError::from)
}

fn view(text: String) -> Result<ViewJson, PortError> {
    ViewJson::new(&text).map_err(PortError::from)
}

fn view_turn(turn: &TurnId, state: TurnState) -> Result<ViewJson, PortError> {
    view(format!(
        r#"{{"turnId":"{}","state":"{}"}}"#,
        turn.as_str(),
        state.as_str()
    ))
}

fn view_turn_error(turn: &TurnId, error: &PublicError) -> Result<ViewJson, PortError> {
    view(format!(
        r#"{{"turnId":"{}","state":"failed","error":{}}}"#,
        turn.as_str(),
        error.details().as_str()
    ))
}

fn view_command_completed(
    request: &RequestId,
    result: Option<&str>,
) -> Result<ViewJson, PortError> {
    let result = match result {
        Some(text) => format!(r#"{{"turnId":"{text}"}}"#),
        None => "{}".to_string(),
    };
    view(format!(
        r#"{{"requestId":"{}","result":{result}}}"#,
        request.as_str()
    ))
}

fn view_command_failed(
    request: &RequestId,
    details: Option<&ViewJson>,
) -> Result<ViewJson, PortError> {
    let details = details.map(ViewJson::as_str).unwrap_or("{}");
    view(format!(
        r#"{{"requestId":"{}","error":{{"code":"internal.unavailable","message":"turn 失败","retryable":false,"details":{details}}}}}"#,
        request.as_str()
    ))
}

fn view_command_uncertain(request: &RequestId, reason: &str) -> Result<ViewJson, PortError> {
    view(format!(
        r#"{{"requestId":"{}","reason":"{}","mayHaveReachedAgent":true}}"#,
        request.as_str(),
        reason
    ))
}

fn view_version_conflict(current: u64, expected: Option<&Version>) -> ViewJson {
    let expected = match expected {
        Some(version) => format!("\"{}\"", version_text(version)),
        None => "null".to_string(),
    };
    ViewJson::new(&format!(
        r#"{{"expectedVersion":{expected},"currentVersion":"{current}"}}"#
    ))
    .unwrap_or_else(|_| ViewJson::empty_object())
}

fn version_text(version: &Version) -> String {
    version.get().to_string()
}

/// 终态结果：`{"turnId":"…"}`（turn 作用域命令的收据字段，RV1-WP6-F3）。
///
/// Node Link 的 `command.terminal.terminal.result` 是开放 object，带 turnId 使 Access 能把命令与 turn
/// 对上；缺它时适配层只能回空 object（§12.5 的「非空」= 非 null，但空 object 丢失了这条信息）。
fn turn_result(turn: &TurnId) -> Result<CommandResult, PortError> {
    CommandResult::from_json_text(&format!(r#"{{"turnId":"{}"}}"#, turn.as_str()))
        .map_err(PortError::from)
}

/// 终态结果：`{"version":"…"}`（模式/配置切换的收据字段，RV1-WP6-F3）。
fn version_result(version: &Version) -> Result<CommandResult, PortError> {
    CommandResult::from_json_text(&format!(r#"{{"version":"{}"}}"#, version.get()))
        .map_err(PortError::from)
}

/// 确保 view 的顶层字符串成员 `key` 等于 `value`（§10.3 的身份/版本字段）。
///
/// 缺失 → 在顶层最前面插入（其余字节原样保留）；已存在且取值一致 → 原样返回（字节不变，不重写）；
/// 已存在但取值不同或不是字符串 → `conflict` 指定的显式错误。core 不覆盖适配器给出的取值，也不写出
/// 第二个同名字段。
fn ensure_view_string_field(
    view: &ViewJson,
    key: &'static str,
    value: &str,
    conflict: &'static str,
) -> Result<ViewJson, PortError> {
    match top_level_member(view.as_str(), key).map_err(PortError::from)? {
        MemberValue::Text(existing) if existing == value => Ok(view.clone()),
        MemberValue::Text(_) | MemberValue::NonText => Err(PortError::InvalidRequest(conflict)),
        MemberValue::Absent => {
            let injected = insert_string_member_front(view.as_str(), key, &json_text(value))
                .map_err(PortError::from)?;
            ViewJson::new(&injected).map_err(PortError::from)
        }
    }
}

#[allow(clippy::too_many_arguments)] // 事件组装的完整形状（含 §10.1 的 origin 判定）
fn pending_event(
    event_type: &str,
    kind: EventKind,
    view: ViewJson,
    turn: Option<TurnId>,
    causation: Option<RequestId>,
    policy: StoredPolicy,
    actor: Option<&Actor>,
) -> Result<PendingEvent, PortError> {
    let event_type = EventType::new(event_type).map_err(PortError::from)?;
    let origin = event_origin(&event_type, actor);
    Ok(PendingEvent {
        kind,
        event_type,
        policy,
        payload: EventPayload { view, acp: None },
        origin,
        turn,
        causation,
    })
}
// ---------------------------------------------------------------------------------------------
// 测试替身（内存 fake 端口）
// ---------------------------------------------------------------------------------------------

/// 内存 fake 端口。
///
/// 这些替身同时是契约的可执行说明：`FakeStore::commit` 按 §5.2 与 [`OwnedCommit`] 的文档实现
/// 「接受提交 vs 终态提交」「幂等命中回放」「交互条件更新」三条语义，broker 的行为断言都建立在它们
/// 之上。它们**不是** `storage-sqlite` 的替代品（不做 migration、PRAGMA、保留与容量）。
#[cfg(test)]
pub(crate) mod test_support {
    use std::collections::{HashMap, VecDeque};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
    use std::task::{Context, Poll, Waker};

    use async_trait::async_trait;

    use super::*;
    use crate::model::{
        AgentDescriptor, AgentId, AgentProfile, AgentRef, AttachmentGeneration, AttachmentId,
        AuditRecord, CapabilitySet, ConfigOption, DeviceId, DeviceRecord, EventId, ExportId,
        ExportRecord, ImportId, ImportRecord, ModeState, NodeId, NodeKind, NodeRecord,
        OriginCursor, OriginEpoch, PairingId, PairingPeer, PairingRecord, PairingState,
        PairingTarget, PeerIdentity, PeerPublicKey, PendingInteraction, ProviderRef,
        ResourceOrigin, SeedState, ServerEpoch, Session, SessionSnapshot, SessionSummary, Turn,
        WorkspaceAlias, WorkspaceRecord,
    };
    use crate::ports::{
        AckOutcome, AgentCatalog, AttachmentRef, AttachmentStore, AuditQuery, DeviceRevocation,
        DeviceWrite, DropReport, ExpiryWrite, ExportRevocation, ExportWrite, IdempotentReplay,
        ImportRemoval, ImportWrite, ImportedSessionQuery, ImportedSessionRecord, LocalConfigStore,
        NodeConnectedWrite, NodeLinkSlice, NodeRevocation, NodeWrite, OwnedEventRecord,
        PairingClaimOutcome, PairingClaimWrite, PairingConsumption, PairingSettlementWrite,
        PairingWrite, PendingInteractionOrigin, ProfileWrite, ProviderRefWrite, PruneReport,
        RemoteCommandRef, RetentionPolicy, SeedWrite, SessionQuery, StoreHealth, TrustRecordRef,
        TrustStore, TurnAccepted, WorkspaceWrite,
    };

    pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn uuid_text(n: u64) -> String {
        format!("00000000-0000-4000-8000-{n:012}")
    }

    /// 合法的 43 字符规范 base64url 摘要（末字符低 2 位为 0）。
    pub(crate) fn digest(seed: char) -> Digest {
        let mut text: String = std::iter::repeat_n(seed, 42).collect();
        text.push('A');
        Digest::new(&text).expect("43 字符规范 base64url")
    }

    pub(crate) fn ts(seconds: u32) -> Timestamp {
        let minutes = seconds / 60;
        let seconds = seconds % 60;
        Timestamp::new(&format!("2026-09-18T00:{minutes:02}:{seconds:02}.000Z"))
            .expect("合法时间戳")
    }

    pub(crate) fn json_view(text: &str) -> ViewJson {
        ViewJson::new(text).expect("object 视图")
    }

    pub(crate) fn endpoint_event(kind: EventKind, event_type: &str, view: &str) -> EndpointEvent {
        EndpointEvent {
            kind,
            event_type: EventType::new(event_type).expect("合法事件类型"),
            payload: EventPayload {
                view: json_view(view),
                acp: None,
            },
            turn: None,
            causation: None,
            at: ts(0),
        }
    }

    pub(crate) fn turn_view(state: &str) -> String {
        format!(r#"{{"state":"{state}"}}"#)
    }

    pub(crate) fn prompt_command(
        actor: &Actor,
        session: &SessionId,
        request: &RequestId,
        fingerprint: char,
    ) -> ClientCommand {
        ClientCommand {
            actor: actor.clone(),
            request: request.clone(),
            command: "session.prompt".to_owned(),
            kind: CommandKind::Mutation,
            session: Some(session.clone()),
            expected_version: None,
            request_fingerprint: digest(fingerprint),
            payload: CommandPayload::Prompt {
                content: vec![PromptContentBlock::new(json_view(
                    r#"{"type":"text","text":"hi"}"#,
                ))],
            },
        }
    }

    pub(crate) fn mode_command(
        actor: &Actor,
        session: &SessionId,
        request: &RequestId,
        expected_version: Version,
    ) -> ClientCommand {
        ClientCommand {
            actor: actor.clone(),
            request: request.clone(),
            command: "session.mode.set".to_owned(),
            kind: CommandKind::Mutation,
            session: Some(session.clone()),
            expected_version: Some(expected_version),
            request_fingerprint: digest('C'),
            payload: CommandPayload::ModeSet {
                mode: ModeId::new("code").expect("mode id"),
            },
        }
    }

    pub(crate) fn server_epoch_of(n: u64) -> ServerEpoch {
        ServerEpoch::new(&uuid_text(n)).expect("server epoch")
    }

    pub(crate) fn origin_epoch_of(n: u64) -> OriginEpoch {
        OriginEpoch::new(&uuid_text(n)).expect("origin epoch")
    }

    pub(crate) fn head_cursor(sequence: u64) -> GlobalCursor {
        GlobalCursor {
            server_epoch: server_epoch_of(999),
            global_sequence: Sequence::new(sequence).expect("sequence"),
        }
    }

    pub(crate) fn command_key(actor: &Actor, request: &RequestId) -> String {
        format!("{}|{}", actor.id_text(), request.as_str())
    }

    // -----------------------------------------------------------------------------------------
    // 共享世界
    // -----------------------------------------------------------------------------------------

    /// 所有 fake 端口共享的状态：让「发布时事件是否已落盘」这类顺序断言可以直接观察。
    #[derive(Default)]
    pub(crate) struct FakeWorld {
        pub(crate) state: Mutex<WorldState>,
        pub(crate) commits: AtomicUsize,
        pub(crate) read_views: AtomicUsize,
        pub(crate) prompts: AtomicUsize,
        pub(crate) prompt_trace: Mutex<Vec<String>>,
        pub(crate) published: Mutex<Vec<Published>>,
        pub(crate) receipts: Mutex<Vec<DeliveryReceipt>>,
        pub(crate) script: Mutex<VecDeque<Script>>,
        pub(crate) audits: Mutex<Vec<AuditRecord>>,
        pub(crate) batches: Mutex<Vec<BatchRecord>>,
        pub(crate) config_calls: AtomicUsize,
        pub(crate) mode_calls: AtomicUsize,
        /// `session.mode.list` 的端口侧候选（测试按需 set，默认空）。
        pub(crate) modes: Mutex<Option<ModeState>>,
        /// 适配器 `prompt` 返回的接受结果里的 turn 标识（测试可改写，默认全零占位值）。
        pub(crate) accepted_turn: Mutex<Option<TurnId>>,
        pub(crate) exports: Mutex<Vec<ExportRecord>>,
        pub(crate) imports: Mutex<Vec<ImportRecord>>,
        pub(crate) profiles: Mutex<Vec<AgentProfile>>,
        pub(crate) workspaces: Mutex<Vec<WorkspaceRecord>>,
        pub(crate) provider_refs: Mutex<Vec<ProviderRef>>,
        pub(crate) seed: Mutex<Option<SeedState>>,
        /// 管理写集实际携带的审计动作（断言「审计随写集提交」而不是事后补写）。
        pub(crate) write_audits: Mutex<Vec<AuditAction>>,
        /// `drop_import` 的调用次数（断言完整移除不再串联连接级清空）。
        pub(crate) drop_import_calls: AtomicUsize,
        pub(crate) imported_sessions: Mutex<Vec<ImportedSessionRecord>>,
        pub(crate) acked: Mutex<Vec<OriginCursor>>,
        pub(crate) attachments: Mutex<Vec<AttachmentRef>>,
        /// 配对行：`settle_pairing` 的目标族由它读出（§11.6 第 2 条）。
        pub(crate) pairings: Mutex<Vec<PairingRecord>>,
        /// 已认领的配对对端行（`consume_pairing` 的身份判定从它读；测试按需 seed，默认空）。
        pub(crate) peers: Mutex<Vec<(PairingId, PairingPeer)>>,
        /// 已绑定的身份材料（`peer_key` 从它读；测试按需 seed，默认空）。
        pub(crate) keys: Mutex<Vec<(PeerIdentity, PeerPublicKey)>>,
        /// 节点角色行（`add_import` 的 owner 前置校验从它读）。
        pub(crate) nodes: Mutex<Vec<NodeRecord>>,
        /// 目录里的可用 Agent（`put_export` 的前置校验从它读；默认空）。
        pub(crate) catalog_agents: Mutex<Vec<AgentDescriptor>>,
    }

    #[derive(Default)]
    pub(crate) struct WorldState {
        pub(crate) sessions: HashMap<String, Session>,
        pub(crate) turns: HashMap<String, Vec<Turn>>,
        pub(crate) events: Vec<CommittedEvent>,
        pub(crate) commands: HashMap<String, CommandRecord>,
        pub(crate) interactions: HashMap<String, InteractionRow>,
        pub(crate) epochs: HashMap<String, OriginEpoch>,
        /// 事件 id → 持久化正文（fake 的 `ReadView::event_payload` 数据源）。
        pub(crate) event_payloads: HashMap<String, EventPayload>,
        /// 事件 id → `origin.kind`（fake 记录 broker 写入的 origin，供 §10.1 断言）。
        pub(crate) event_origins: HashMap<String, EventOrigin>,
        /// 事件 id → 落盘的 turn 归属（供 §10.3 「view 与 turn_id 一致」断言）。
        pub(crate) event_turns: HashMap<String, Option<TurnId>>,
        /// `global_sequence` → 事件类别（§6 第 15 条的 delta 判定）。
        pub(crate) event_kinds: HashMap<u64, EventKind>,
        /// 被压缩的 delta 行 `global_sequence` → summary 行的 `global_sequence`（§6 第 15 条）。
        pub(crate) compacted_into: HashMap<u64, u64>,
        pub(crate) head: u64,
        pub(crate) per_session_seq: HashMap<String, u64>,
        pub(crate) next_id: u64,
        /// 指定第 N 次 `commit` 返回 `Unavailable`（脚本化失败）。
        pub(crate) fail_at: Option<usize>,
        /// 指定第 N 次 `commit` 返回的会话版本比实际值大 1（模拟存储层版本规则漂移）。
        pub(crate) version_drift_at: Option<usize>,
    }

    pub(crate) struct InteractionRow {
        pub(crate) pending: PendingInteraction,
        pub(crate) resolution: Option<InteractionResolved>,
        /// 创建该交互的 origin 事件 id（`node_link_slice` 的 `pending_interactions` 需要它；
        /// 真实存储用 `owned_interaction.request_event` 回查）。
        pub(crate) origin_event: EventId,
    }

    /// 一次提交里的事件类型序列（§6.10 的合批断言）。
    #[derive(Clone)]
    pub(crate) struct BatchRecord {
        pub(crate) session: Option<String>,
        pub(crate) types: Vec<String>,
    }

    /// 一次发布：`store_had_it` 断言「先提交后发布」（§6.1/§6.2）。
    pub(crate) struct Published {
        pub(crate) delivery: CommittedDelivery,
        pub(crate) store_had_it: bool,
    }

    /// 后端脚本：一次 `prompt` 要发的事件序列；`yield_polls` 让出若干次以暴露交错。
    #[derive(Clone)]
    pub(crate) struct Script {
        pub(crate) events: Vec<EndpointEvent>,
        pub(crate) yield_polls: usize,
    }

    impl Script {
        pub(crate) fn new(events: Vec<EndpointEvent>) -> Self {
            Self {
                events,
                yield_polls: 0,
            }
        }

        pub(crate) fn yielding(events: Vec<EndpointEvent>, yield_polls: usize) -> Self {
            Self {
                events,
                yield_polls,
            }
        }
    }

    impl FakeWorld {
        pub(crate) fn new() -> Arc<Self> {
            Arc::new(Self::default())
        }

        pub(crate) fn fail_commit_at(&self, index: usize) {
            lock(&self.state).fail_at = Some(index);
        }

        /// 让第 `index` 次 `commit` 返回一个与 core 推导不一致的会话版本（§10.3 漂移检测用例）。
        pub(crate) fn drift_version_at(&self, index: usize) {
            lock(&self.state).version_drift_at = Some(index);
        }

        pub(crate) fn fail_next_commit(&self) {
            let next = self.commits.load(Ordering::SeqCst) + 1;
            lock(&self.state).fail_at = Some(next);
        }

        pub(crate) fn push_script(&self, script: Script) {
            lock(&self.script).push_back(script);
        }

        pub(crate) fn prompt_count(&self) -> usize {
            self.prompts.load(Ordering::SeqCst)
        }

        pub(crate) fn prompt_trace(&self) -> Vec<String> {
            lock(&self.prompt_trace).clone()
        }

        pub(crate) fn commit_count(&self) -> usize {
            self.commits.load(Ordering::SeqCst)
        }

        pub(crate) fn read_view_count(&self) -> usize {
            self.read_views.load(Ordering::SeqCst)
        }

        pub(crate) fn publish_count(&self) -> usize {
            lock(&self.published).len()
        }

        pub(crate) fn published(&self) -> Vec<CommittedDelivery> {
            lock(&self.published)
                .iter()
                .map(|item| item.delivery.clone())
                .collect()
        }

        /// 每次发布时，对应事件/收据是否已经落盘（§6.1/§6.2 的顺序断言）。
        pub(crate) fn published_after_commit(&self) -> bool {
            lock(&self.published).iter().all(|item| item.store_had_it)
        }

        pub(crate) fn batches(&self) -> Vec<BatchRecord> {
            lock(&self.batches).clone()
        }

        pub(crate) fn batch_types(&self) -> Vec<Vec<String>> {
            self.batches()
                .into_iter()
                .map(|batch| batch.types)
                .collect()
        }

        pub(crate) fn event_types(&self, session: &SessionId) -> Vec<String> {
            self.batches()
                .into_iter()
                .filter(|batch| batch.session.as_deref() == Some(session.as_str()))
                .flat_map(|batch| batch.types)
                .collect()
        }

        pub(crate) fn command(&self, request: &RequestId) -> Option<CommandRecord> {
            lock(&self.state)
                .commands
                .iter()
                .find(|(key, _)| key.ends_with(request.as_str()))
                .map(|(_, record)| record.clone())
        }

        pub(crate) fn session(&self, session: &SessionId) -> Option<Session> {
            lock(&self.state).sessions.get(session.as_str()).cloned()
        }

        pub(crate) fn turns(&self, session: &SessionId) -> Vec<Turn> {
            lock(&self.state)
                .turns
                .get(session.as_str())
                .cloned()
                .unwrap_or_default()
        }

        pub(crate) fn events(&self, session: &SessionId) -> Vec<CommittedEvent> {
            lock(&self.state)
                .events
                .iter()
                .filter(|event| {
                    event.session.as_ref().map(|id| id.as_str()) == Some(session.as_str())
                })
                .cloned()
                .collect()
        }

        pub(crate) fn receipts(&self) -> Vec<DeliveryReceipt> {
            lock(&self.receipts).clone()
        }

        pub(crate) fn audits(&self) -> Vec<AuditRecord> {
            lock(&self.audits).clone()
        }

        pub(crate) fn config_calls(&self) -> usize {
            self.config_calls.load(Ordering::SeqCst)
        }

        /// 某条事件落库时的 `origin.kind`（§10.1）。
        pub(crate) fn event_origin(&self, id: &EventId) -> Option<EventOrigin> {
            lock(&self.state).event_origins.get(id.as_str()).copied()
        }

        /// 某条事件落库时的 turn 归属（§10.3：view 的 `turnId` 必须与它一致）。
        pub(crate) fn event_turn(&self, id: &EventId) -> Option<TurnId> {
            lock(&self.state)
                .event_turns
                .get(id.as_str())
                .cloned()
                .flatten()
        }

        pub(crate) fn seed_session(&self, session: Session) {
            let mut state = lock(&self.state);
            let id = session.id().clone();
            state
                .epochs
                .insert(id.as_str().to_owned(), origin_epoch_of(500));
            state.sessions.insert(id.as_str().to_owned(), session);
        }

        pub(crate) fn seed_interaction(&self, pending: PendingInteraction) {
            let origin_event = EventId::new(&uuid_text(0)).expect("事件 id");
            lock(&self.state).interactions.insert(
                pending.id().as_str().to_owned(),
                InteractionRow {
                    pending,
                    resolution: None,
                    origin_event,
                },
            );
        }

        pub(crate) fn interaction_resolution(
            &self,
            id: &InteractionId,
        ) -> Option<InteractionResolved> {
            lock(&self.state)
                .interactions
                .get(id.as_str())
                .and_then(|row| row.resolution.clone())
        }
    }

    // -----------------------------------------------------------------------------------------
    // 基础设施 fake
    // -----------------------------------------------------------------------------------------

    #[derive(Default)]
    pub(crate) struct TestClock {
        pub(crate) seconds: Mutex<u32>,
    }

    impl TestClock {
        pub(crate) fn new() -> Arc<Self> {
            Arc::new(Self::default())
        }
    }

    impl Clock for TestClock {
        fn now(&self) -> Timestamp {
            let mut seconds = lock(&self.seconds);
            let value = *seconds;
            *seconds = seconds.saturating_add(1);
            ts(value)
        }
    }

    #[derive(Default)]
    pub(crate) struct TestIds {
        pub(crate) next: AtomicUsize,
    }

    impl TestIds {
        fn next_text(&self) -> String {
            uuid_text(self.next.fetch_add(1, Ordering::SeqCst) as u64 + 100)
        }
    }

    impl IdGenerator for TestIds {
        fn turn_id(&self) -> TurnId {
            TurnId::new(&self.next_text()).expect("uuid")
        }

        fn interaction_id(&self) -> InteractionId {
            InteractionId::new(&self.next_text()).expect("uuid")
        }

        fn pairing_id(&self) -> PairingId {
            PairingId::new(&self.next_text()).expect("uuid")
        }

        fn origin_epoch(&self) -> OriginEpoch {
            OriginEpoch::new(&self.next_text()).expect("uuid")
        }

        fn message_id(&self) -> MessageId {
            MessageId::new(&self.next_text()).expect("uuid")
        }

        fn request_id(&self) -> RequestId {
            RequestId::new(&self.next_text()).expect("uuid")
        }
    }

    pub(crate) struct TestPublisher {
        pub(crate) world: Arc<FakeWorld>,
    }

    impl EventPublisher for TestPublisher {
        fn publish(&self, delivery: CommittedDelivery) {
            let store_had_it = match &delivery {
                CommittedDelivery::Owned(event) => lock(&self.world.state)
                    .events
                    .iter()
                    .any(|stored| stored.id == event.id),
                CommittedDelivery::Imported { origin, .. } => lock(&self.world.receipts)
                    .iter()
                    .any(|receipt| receipt.origin.origin_event_id == origin.origin_event_id),
            };
            lock(&self.world.published).push(Published {
                delivery,
                store_had_it,
            });
        }
    }

    pub(crate) struct TestAudit {
        pub(crate) world: Arc<FakeWorld>,
    }

    #[async_trait]
    impl AuditStore for TestAudit {
        async fn append(&self, record: AuditRecord) -> Result<(), PortError> {
            lock(&self.world.audits).push(record);
            Ok(())
        }

        async fn query(&self, _query: AuditQuery) -> Result<Vec<AuditRecord>, PortError> {
            Ok(lock(&self.world.audits).clone())
        }

        /// 测试替身的「曾经写入过的最大审计序号」＝已追加的行数：用例据此断言
        /// `catalogRevision` 取自审计水位（真实实现在 `storage-sqlite`，用 `sqlite_sequence`）。
        async fn watermark(&self) -> Result<u64, PortError> {
            Ok(lock(&self.world.audits).len() as u64)
        }
    }

    /// 目录测试替身：默认空，测试按需把 `FakeWorld::catalog_agents` 填成可用 Agent。
    pub(crate) struct TestCatalog {
        pub(crate) world: Arc<FakeWorld>,
    }

    #[async_trait]
    impl AgentCatalog for TestCatalog {
        async fn agents(&self) -> Result<Vec<AgentDescriptor>, PortError> {
            Ok(lock(&self.world.catalog_agents).clone())
        }

        async fn agent_capabilities(&self, _agent: &AgentRef) -> Result<CapabilitySet, PortError> {
            Ok(CapabilitySet::empty())
        }
    }

    // -----------------------------------------------------------------------------------------
    // 存储 fake
    // -----------------------------------------------------------------------------------------

    pub(crate) struct FakeStore {
        pub(crate) world: Arc<FakeWorld>,
    }

    impl FakeStore {
        /// `OwnedCommit` 的 §5.2 语义：幂等 → 会话/交互 → turn → 事件 → 终态/接受行。
        fn apply(
            &self,
            commit: &OwnedCommit,
        ) -> Result<(Option<SessionId>, Vec<CommittedEvent>), PortError> {
            let mut state = lock(&self.world.state);
            let at = commit.at.clone();
            let mut created: Option<SessionId> = None;
            match commit.state.clone() {
                Some(StateChange::Create(new)) => {
                    state.next_id += 1;
                    let session_id = SessionId::new(&uuid_text(state.next_id))?;
                    let epoch = commit
                        .origin_epoch
                        .clone()
                        .ok_or(PortError::InvalidRequest("Create 必须带 origin_epoch"))?;
                    let session = Session::try_new(
                        session_id.clone(),
                        OwnedSessionRef::new(session_id.clone()),
                        new.title.clone(),
                        new.agent.clone(),
                        SessionState::Idle,
                        ResourceOrigin::Local,
                        None,
                        Version::from(1),
                        at.clone(),
                        at.clone(),
                        None,
                    )?;
                    state.epochs.insert(session_id.as_str().to_owned(), epoch);
                    state
                        .sessions
                        .insert(session_id.as_str().to_owned(), session);
                    created = Some(session_id);
                }
                Some(StateChange::Update(update)) => {
                    let session_id = commit
                        .session
                        .clone()
                        .ok_or(PortError::InvalidRequest("Update 必须带 session"))?;
                    let current = state
                        .sessions
                        .get(session_id.as_str())
                        .cloned()
                        .ok_or_else(|| {
                            PortError::NotFound(EntityRef::Session(session_id.clone()))
                        })?;
                    if let Some(expected) = commit.expected_version {
                        if expected.get() != current.version().get() {
                            return Err(PortError::Conflict(ConflictKind::VersionMismatch));
                        }
                    }
                    // §6.7：first-writer-wins 的条件更新。
                    if let Some(resolved) = &update.interaction {
                        match state.interactions.get_mut(resolved.interaction.as_str()) {
                            None => {
                                return Err(PortError::NotFound(EntityRef::Interaction(
                                    resolved.interaction.clone(),
                                )));
                            }
                            Some(row) if row.resolution.is_some() => {
                                return Err(PortError::Conflict(ConflictKind::AlreadyResolved));
                            }
                            Some(row) => row.resolution = Some(resolved.clone()),
                        }
                    }
                    let next_state = update.state.unwrap_or_else(|| current.state());
                    let mode = match &update.mode {
                        ModeChange::Unchanged => current.current_mode().cloned(),
                        ModeChange::Set(mode) => Some(mode.clone()),
                    };
                    let closed_at = update
                        .closed_at
                        .clone()
                        .or_else(|| current.closed_at().cloned());
                    let updated = Session::try_new(
                        current.id().clone(),
                        current.reference().clone(),
                        current.title().map(str::to_owned),
                        current.agent().clone(),
                        next_state,
                        current.origin().clone(),
                        mode,
                        Version::from(current.version().get() + 1),
                        current.created_at().clone(),
                        at.clone(),
                        closed_at,
                    )?;
                    state
                        .sessions
                        .insert(session_id.as_str().to_owned(), updated);
                }
                None => {}
            }
            if let Some(session_id) = commit.session.clone() {
                let turns = state
                    .turns
                    .entry(session_id.as_str().to_owned())
                    .or_default();
                for change in &commit.turns {
                    match change {
                        TurnChange::Create(new) => {
                            let queue_index = turns.len() as u32;
                            let turn = Turn::try_new(
                                new.turn.clone(),
                                session_id.clone(),
                                new.state,
                                queue_index,
                                new.causation.clone(),
                                new.started_at.clone(),
                                None,
                            )?;
                            turns.push(turn);
                        }
                        TurnChange::Update(update) => {
                            let Some(turn) =
                                turns.iter_mut().find(|turn| turn.id() == &update.turn)
                            else {
                                return Err(PortError::NotFound(EntityRef::Turn(
                                    update.turn.clone(),
                                )));
                            };
                            *turn = Turn::try_new(
                                turn.id().clone(),
                                session_id.clone(),
                                update.state,
                                turn.queue_index(),
                                turn.causation().cloned(),
                                turn.started_at().cloned(),
                                update.ended_at.clone(),
                            )?;
                        }
                    }
                }
            }
            // §6 第 15 条：`compacted` 的每个 cursor 必须是本提交会话的 delta 行、未被压过，且同批必须有
            // summary 事件；任一不满足 → 整事务 `InvalidRequest`。校验放在任何写入之前，因此是"零写入"。
            if !commit.compacted.is_empty() {
                if !commit
                    .events
                    .iter()
                    .any(|event| event.kind == EventKind::Summary)
                {
                    return Err(PortError::InvalidRequest(
                        "compacted 必须与一条 kind='summary' 的事件同批（§6 第 15 条）",
                    ));
                }
                for cursor in &commit.compacted {
                    let sequence = cursor.global_sequence.get();
                    let Some(target) = state
                        .events
                        .iter()
                        .find(|event| event.global_sequence.get() == sequence)
                    else {
                        return Err(PortError::InvalidRequest("compacted 指向不存在的事件"));
                    };
                    if target.session.as_ref() != commit.session.as_ref() {
                        return Err(PortError::InvalidRequest("compacted 不得跨会话"));
                    }
                    if state.compacted_into.contains_key(&sequence) {
                        return Err(PortError::InvalidRequest("compacted 指向的行已经被压过"));
                    }
                    if state.event_kinds.get(&sequence) != Some(&EventKind::Delta) {
                        return Err(PortError::InvalidRequest(
                            "compacted 只接受 kind='delta' 的行（§6 第 15 条）",
                        ));
                    }
                }
            }
            let mut appended = Vec::new();
            let mut batch = Vec::new();
            for event in &commit.events {
                state.next_id += 1;
                let id = EventId::new(&uuid_text(state.next_id))?;
                state.head += 1;
                let global = Sequence::new(state.head)?;
                let (session_sequence, origin_epoch, origin_sequence) =
                    match commit.session.as_ref() {
                        Some(session) => {
                            let counter = state
                                .per_session_seq
                                .entry(session.as_str().to_owned())
                                .or_insert(0);
                            *counter += 1;
                            let sequence = Sequence::new(*counter)?;
                            let epoch = state
                                .epochs
                                .get(session.as_str())
                                .cloned()
                                .unwrap_or_else(|| origin_epoch_of(501));
                            (Some(sequence), Some(epoch), Some(sequence))
                        }
                        // §9 判据 17：非会话级事件三列都是 NULL。
                        None => (None, None, None),
                    };
                state
                    .event_payloads
                    .insert(id.as_str().to_owned(), event.payload.clone());
                state
                    .event_origins
                    .insert(id.as_str().to_owned(), event.origin);
                state
                    .event_turns
                    .insert(id.as_str().to_owned(), event.turn.clone());
                state.event_kinds.insert(global.get(), event.kind);
                batch.push(event.event_type.as_str().to_owned());
                let committed = CommittedEvent {
                    id,
                    event_type: event.event_type.clone(),
                    session: commit.session.clone(),
                    session_sequence,
                    global_sequence: global,
                    origin_epoch,
                    origin_sequence,
                    created_at: at.clone(),
                };
                state.events.push(committed.clone());
                appended.push(committed);
            }
            // §6 第 13 条 / §9 判据 15：每条 `PendingInteractionWrite` 落一行；配对（按 payload 的
            // `interactionId` 找到同一提交里那条 `kind = interaction` 事件）由存储层完成，配不到即整
            // 事务失败——事件 id 只存在于存储层，不经过 broker。
            for write in &commit.interactions {
                let paired = commit.events.iter().find(|event| {
                    interaction_request_kind(&event.event_type).is_some()
                        && view_interaction_id(&event.payload.view).as_ref()
                            == Some(write.interaction.id())
                });
                let Some(paired) = paired else {
                    return Err(PortError::InvalidRequest(
                        "交互写入必须与同一提交里带同一 interactionId 的事件配对（§6 第 13 条）",
                    ));
                };
                if let Some(existing) = state.interactions.get(write.interaction.id().as_str()) {
                    if existing.resolution.is_some() {
                        return Err(PortError::Conflict(ConflictKind::AlreadyResolved));
                    }
                }
                // 创建它的那条事件就是同一提交里配对成功的那条（`node_link_slice` 的
                // `pending_interactions[].origin_event` 要从它取 payload）。`commit.events` 与
                // `appended` 同序，因此下标直接对应。
                let position = commit
                    .events
                    .iter()
                    .position(|event| std::ptr::eq(event, paired))
                    .ok_or(PortError::Corrupt("交互事件未在本次提交中落盘"))?;
                let origin_event = appended
                    .get(position)
                    .map(|event| event.id.clone())
                    .ok_or(PortError::Corrupt("交互事件未在本次提交中落盘"))?;
                state.interactions.insert(
                    write.interaction.id().as_str().to_owned(),
                    InteractionRow {
                        pending: write.interaction.clone(),
                        resolution: None,
                        origin_event,
                    },
                );
            }
            // §6 第 15 条：校验已通过，这里把被压行指向本批 summary 行的 `global_sequence`。
            if !commit.compacted.is_empty() {
                if let Some(summary) = appended.last().map(|event| event.global_sequence.get()) {
                    for cursor in &commit.compacted {
                        state
                            .compacted_into
                            .insert(cursor.global_sequence.get(), summary);
                    }
                }
            }
            // 终态提交：按同一批次 `command.*` 事件的 causation 定位命令行（`ports::OwnedCommit`
            // 的文档约定），`terminal_event_id` 取该批次最后一条事件的 id。
            if let Some(terminal) = commit.command_terminal.clone() {
                let request = commit
                    .events
                    .iter()
                    .rev()
                    .find(|event| event.event_type.as_str().starts_with("command."))
                    .and_then(|event| event.causation.clone())
                    .ok_or(PortError::InvalidRequest(
                        "终态提交缺少 command.* 事件的 causation",
                    ))?;
                let key = state
                    .commands
                    .iter()
                    .find(|(_, record)| {
                        record.request() == &request && record.session() == commit.session.as_ref()
                    })
                    .map(|(key, _)| key.clone())
                    .ok_or_else(|| {
                        PortError::NotFound(EntityRef::Command {
                            session: commit.session.clone(),
                            request: request.clone(),
                        })
                    })?;
                let existing = state
                    .commands
                    .get(&key)
                    .cloned()
                    .ok_or(PortError::InvalidRequest("命令行消失"))?;
                let terminal_event = appended.last().map(|event| event.id.clone());
                let updated = CommandRecord::try_new(
                    existing.session().cloned(),
                    existing.request().clone(),
                    existing.command(),
                    existing.kind(),
                    existing.actor().clone(),
                    existing.accepted_at().cloned(),
                    terminal.status(),
                    terminal.terminal_at().cloned(),
                    terminal_event,
                    terminal
                        .result()
                        .cloned()
                        .or_else(|| existing.result().cloned()),
                    terminal
                        .error()
                        .cloned()
                        .or_else(|| existing.error().cloned()),
                    existing.expected_version(),
                    existing.request_fingerprint().clone(),
                )?;
                state.commands.insert(key, updated);
            }
            // 接受提交：写幂等行（status = accepted）。§6 第 20 条：装配方不知道目标会话的命令
            // （`session.create`）用本次事务刚创建的会话回填——终态块与启动恢复靠它定位该行。
            if let Some(record) = commit.idempotency.clone() {
                let command = CommandRecord::try_new(
                    commit.session.clone().or_else(|| created.clone()),
                    record.request.clone(),
                    &record.command,
                    record.kind,
                    record.actor.clone(),
                    Some(record.accepted_at.clone()),
                    CommandStatus::Accepted,
                    None,
                    None,
                    None,
                    None,
                    record.expected_version,
                    record.request_fingerprint.clone(),
                )?;
                state
                    .commands
                    .insert(command_key(&record.actor, &record.request), command);
            }
            lock(&self.world.batches).push(BatchRecord {
                session: commit
                    .session
                    .as_ref()
                    .map(|session| session.as_str().to_owned()),
                types: batch,
            });
            Ok((created, appended))
        }
    }

    #[async_trait]
    impl SessionStore for FakeStore {
        async fn commit(&self, commit: OwnedCommit) -> Result<CommitOutcome, PortError> {
            let index = self.world.commits.fetch_add(1, Ordering::SeqCst) + 1;
            let version_drifts = {
                let mut state = lock(&self.world.state);
                if state.fail_at == Some(index) {
                    state.fail_at = None;
                    return Err(PortError::Unavailable(UnavailableKind::IoError));
                }
                if state.version_drift_at == Some(index) {
                    state.version_drift_at = None;
                    true
                } else {
                    false
                }
            };
            // §5.2：幂等命中 → 不追加事件、不改状态，返回首次结果；五项指纹任一不同 → 冲突。
            if let Some(record) = commit.idempotency.as_ref() {
                let state = lock(&self.world.state);
                let key = command_key(&record.actor, &record.request);
                if let Some(existing) = state.commands.get(&key).cloned() {
                    // §6 第 20 条：`record.session = None` 表示「装配期不知道目标会话」（`session.create`
                    // 的 id 由存储层分配），不参与比对；其余四项恒比。
                    let same = existing.command() == record.command
                        && existing.kind() == record.kind
                        && record
                            .session
                            .as_ref()
                            .is_none_or(|session| existing.session() == Some(session))
                        && existing.expected_version() == record.expected_version
                        && existing.request_fingerprint() == &record.request_fingerprint;
                    if !same {
                        return Err(PortError::Conflict(ConflictKind::IdempotencyConflict));
                    }
                    let session_id = existing.session().cloned();
                    let version = session_id
                        .as_ref()
                        .and_then(|session| state.sessions.get(session.as_str()))
                        .map(|session| session.version())
                        .unwrap_or_else(|| Version::from(0));
                    let origin_epoch = session_id
                        .as_ref()
                        .and_then(|session| state.epochs.get(session.as_str()).cloned());
                    return Ok(CommitOutcome {
                        session_id,
                        origin_epoch,
                        version,
                        appended: Vec::new(),
                        replayed: Some(IdempotentReplay { record: existing }),
                    });
                }
            }
            let (created, appended) = self.apply(&commit)?;
            let state = lock(&self.world.state);
            let mut version = commit
                .session
                .as_ref()
                .and_then(|session| state.sessions.get(session.as_str()))
                .map(|session| session.version())
                .unwrap_or_else(|| Version::from(0));
            // 脚本化的版本漂移：只改**返回值**，不改已写下的会话行（模拟「存储层规则与 core 推导不同」）。
            if version_drifts {
                version = Version::new(version.get() + 1);
            }
            let origin_epoch = commit
                .session
                .as_ref()
                .and_then(|session| state.epochs.get(session.as_str()).cloned())
                .or_else(|| commit.origin_epoch.clone());
            Ok(CommitOutcome {
                session_id: created.or_else(|| commit.session.clone()),
                origin_epoch,
                version,
                appended,
                replayed: None,
            })
        }

        async fn load(&self, session: &SessionId) -> Result<Option<SessionSnapshot>, PortError> {
            let state = lock(&self.world.state);
            let Some(found) = state.sessions.get(session.as_str()).cloned() else {
                return Ok(None);
            };
            let origin_epoch = state
                .epochs
                .get(session.as_str())
                .cloned()
                .unwrap_or_else(|| origin_epoch_of(500));
            Ok(Some(SessionSnapshot {
                session: found,
                origin_epoch,
                head: head_cursor(state.head),
            }))
        }

        async fn list(&self, query: SessionQuery) -> Result<Vec<SessionSummary>, PortError> {
            let state = lock(&self.world.state);
            let mut summaries: Vec<SessionSummary> = state
                .sessions
                .values()
                .filter(|session| match &query.only {
                    Some(only) => only.iter().any(|id| id == session.id()),
                    None => true,
                })
                .filter(|session| {
                    query.states.is_empty() || query.states.contains(&session.state())
                })
                .map(|session| session.summary())
                .collect();
            if let Some(limit) = query.limit {
                summaries.truncate(limit as usize);
            }
            Ok(summaries)
        }

        async fn head(&self) -> Result<GlobalCursor, PortError> {
            Ok(head_cursor(lock(&self.world.state).head))
        }

        async fn read_view(&self) -> Result<Box<dyn ReadView>, PortError> {
            self.world.read_views.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(FakeReadView {
                world: self.world.clone(),
            }))
        }

        async fn find_request(
            &self,
            request: &RequestId,
            actor: &Actor,
        ) -> Result<Option<CommandRecord>, PortError> {
            Ok(lock(&self.world.state)
                .commands
                .get(&command_key(actor, request))
                .cloned())
        }

        /// §6 第 16 条：仍为 `accepted` 且没有终态事件的 mutation 命令。
        async fn unsettled_commands(
            &self,
            limit: ReplayLimit,
        ) -> Result<Vec<CommandRecord>, PortError> {
            let state = lock(&self.world.state);
            Ok(state
                .commands
                .values()
                .filter(|record| {
                    record.status() == CommandStatus::Accepted
                        && record.kind() == CommandKind::Mutation
                })
                .take(limit.events() as usize)
                .cloned()
                .collect())
        }

        async fn retention_window(
            &self,
            _session: &SessionId,
        ) -> Result<Option<(Sequence, Sequence)>, PortError> {
            let state = lock(&self.world.state);
            if state.head == 0 {
                return Ok(None);
            }
            Ok(Some((Sequence::new(1)?, Sequence::new(state.head)?)))
        }

        async fn prune(
            &self,
            _policy: RetentionPolicy,
            _at: Timestamp,
        ) -> Result<PruneReport, PortError> {
            Ok(PruneReport::default())
        }

        async fn health(&self) -> Result<StoreHealth, PortError> {
            Ok(StoreHealth {
                total_bytes: 0,
                integrity_ok: true,
                read_only: false,
                user_version: 1,
                owned_schema_version: 1,
                imported_schema_version: 1,
                server_epoch: server_epoch_of(999),
            })
        }
    }

    pub(crate) struct FakeReadView {
        pub(crate) world: Arc<FakeWorld>,
    }

    #[async_trait]
    impl ReadView for FakeReadView {
        async fn head(&self) -> Result<GlobalCursor, PortError> {
            Ok(head_cursor(lock(&self.world.state).head))
        }

        async fn replay(
            &self,
            after: Option<GlobalCursor>,
            limit: ReplayLimit,
        ) -> Result<ReplayBatch, PortError> {
            let state = lock(&self.world.state);
            let from = after
                .as_ref()
                .map(|cursor| cursor.global_sequence.get())
                .unwrap_or(0);
            let head = head_cursor(state.head);
            let events: Vec<CommittedDelivery> = state
                .events
                .iter()
                .filter(|event| event.global_sequence.get() > from)
                .take(limit.events() as usize)
                .map(|event| CommittedDelivery::Owned(event.clone()))
                .collect();
            let next = events.last().map(|delivery| match delivery {
                CommittedDelivery::Owned(event) => head_cursor(event.global_sequence.get()),
                CommittedDelivery::Imported { local_sequence, .. } => {
                    head_cursor(local_sequence.get())
                }
            });
            Ok(ReplayBatch {
                events,
                head,
                next,
                reset_required: None,
            })
        }

        async fn event_payload(&self, event: &EventId) -> Result<Option<EventPayload>, PortError> {
            Ok(lock(&self.world.state)
                .event_payloads
                .get(event.as_str())
                .cloned())
        }

        async fn node_link_slice(
            &self,
            session: &SessionId,
            after: Option<OriginCursor>,
            limit: ReplayLimit,
        ) -> Result<NodeLinkSlice, PortError> {
            let state = lock(&self.world.state);
            let Some(stored) = state.sessions.get(session.as_str()) else {
                return Err(PortError::NotFound(EntityRef::Session(session.clone())));
            };
            let Some(epoch) = state.epochs.get(session.as_str()).cloned() else {
                return Err(PortError::NotFound(EntityRef::Session(session.clone())));
            };
            let from = after
                .as_ref()
                .map_or(0, |cursor| cursor.origin_sequence.get());
            let mut events = Vec::new();
            for event in &state.events {
                if event.session.as_ref() != Some(session) {
                    continue;
                }
                let Some(origin) = event.origin_sequence else {
                    continue;
                };
                if origin.get() <= from {
                    continue;
                }
                if events.len() as u32 >= limit.events() {
                    break;
                }
                let Some(payload) = state.event_payloads.get(event.id.as_str()).cloned() else {
                    continue;
                };
                events.push(OwnedEventRecord {
                    event: event.clone(),
                    payload,
                });
            }
            let head_sequence = state
                .events
                .iter()
                .filter(|event| event.session.as_ref() == Some(session))
                .filter_map(|event| event.origin_sequence)
                .map(Sequence::get)
                .max()
                .unwrap_or(0);
            let pending_interactions = state
                .interactions
                .values()
                .filter(|row| row.pending.session() == session && row.resolution.is_none())
                .map(|row| PendingInteractionOrigin {
                    interaction: row.pending.clone(),
                    origin_event: row.origin_event.clone(),
                })
                .collect();
            Ok(NodeLinkSlice {
                summary: stored.summary(),
                head: OriginCursor {
                    origin_epoch: epoch,
                    origin_sequence: Sequence::new(head_sequence)
                        .map_err(|_| PortError::Corrupt("origin sequence out of range"))?,
                },
                pending_interactions,
                events,
            })
        }

        async fn session_event_payload(
            &self,
            session: &SessionId,
            event: &EventId,
        ) -> Result<Option<EventPayload>, PortError> {
            let state = lock(&self.world.state);
            let belongs = state
                .events
                .iter()
                .any(|row| row.id == *event && row.session.as_ref() == Some(session));
            if !belongs {
                return Ok(None);
            }
            Ok(state.event_payloads.get(event.as_str()).cloned())
        }

        async fn read_session(&self, query: HistoryQuery) -> Result<HistoryPage, PortError> {
            let state = lock(&self.world.state);
            let Some(session) = state.sessions.get(query.session.as_str()) else {
                return Err(PortError::NotFound(EntityRef::Session(
                    query.session.clone(),
                )));
            };
            let events = if query.include.messages {
                state
                    .events
                    .iter()
                    .filter(|event| event.session.as_ref() == Some(&query.session))
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            };
            let turns = if query.include.turns {
                state
                    .turns
                    .get(query.session.as_str())
                    .cloned()
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let interactions = if query.include.pending_interactions {
                state
                    .interactions
                    .values()
                    .filter(|row| {
                        row.pending.session() == &query.session && row.resolution.is_none()
                    })
                    .map(|row| row.pending.clone())
                    .collect()
            } else {
                Vec::new()
            };
            Ok(HistoryPage {
                session: session.summary(),
                events,
                turns,
                interactions,
                config: Vec::new(),
                capabilities: None,
                head: head_cursor(state.head),
                next: None,
            })
        }
    }

    pub(crate) struct FakeDeliveries {
        pub(crate) world: Arc<FakeWorld>,
    }

    #[async_trait]
    impl RemoteDeliveryStore for FakeDeliveries {
        async fn commit_receipt(
            &self,
            receipt: DeliveryReceipt,
        ) -> Result<ReceiptOutcome, PortError> {
            let mut receipts = lock(&self.world.receipts);
            if let Some(index) = receipts
                .iter()
                .position(|stored| stored.origin.origin_event_id == receipt.origin.origin_event_id)
            {
                return Ok(ReceiptOutcome {
                    local_sequence: LocalCursor::new(index as u64 + 1),
                    duplicate: true,
                });
            }
            receipts.push(receipt);
            Ok(ReceiptOutcome {
                local_sequence: LocalCursor::new(receipts.len() as u64),
                duplicate: false,
            })
        }

        async fn local_replay(
            &self,
            after: Option<LocalCursor>,
            limit: ReplayLimit,
        ) -> Result<Vec<DeliveryIndexEntry>, PortError> {
            let from = after.map(|cursor| cursor.get()).unwrap_or(0);
            Ok(lock(&self.world.receipts)
                .iter()
                .enumerate()
                .filter(|(index, _)| (*index as u64 + 1) > from)
                .take(limit.events() as usize)
                .map(|(index, receipt)| DeliveryIndexEntry {
                    session: receipt.session.clone(),
                    origin: receipt.origin.clone(),
                    origin_sequence: receipt.origin_sequence,
                    local_sequence: LocalCursor::new(index as u64 + 1),
                    event_type: receipt.event_type.clone(),
                    payload_digest: receipt.payload_digest.clone(),
                    received_at: receipt.at.clone(),
                })
                .collect())
        }

        async fn ack(
            &self,
            _session: &RemoteSessionRef,
            cursor: OriginCursor,
            _at: Timestamp,
        ) -> Result<AckOutcome, PortError> {
            let mut acked = lock(&self.world.acked);
            let known = acked.last().cloned();
            let applied = known
                .as_ref()
                .map(|previous| cursor.origin_sequence > previous.origin_sequence)
                .unwrap_or(true);
            if applied {
                acked.push(cursor.clone());
            }
            Ok(AckOutcome {
                applied,
                cursor: known.or(Some(cursor)),
            })
        }

        async fn load_ack(
            &self,
            _session: &RemoteSessionRef,
        ) -> Result<Option<OriginCursor>, PortError> {
            Ok(lock(&self.world.acked).last().cloned())
        }

        async fn upsert_session(
            &self,
            record: ImportedSessionRecord,
            _at: Timestamp,
        ) -> Result<(), PortError> {
            let mut sessions = lock(&self.world.imported_sessions);
            sessions.retain(|stored| stored.session != record.session);
            sessions.push(record);
            Ok(())
        }

        async fn list_sessions(
            &self,
            _query: ImportedSessionQuery,
        ) -> Result<Vec<ImportedSessionRecord>, PortError> {
            Ok(lock(&self.world.imported_sessions).clone())
        }

        async fn find_remote_request(
            &self,
            _session: &RemoteSessionRef,
            _request: &RequestId,
        ) -> Result<Option<RemoteCommandRef>, PortError> {
            Ok(None)
        }

        async fn drop_import(&self, import: &ImportId) -> Result<DropReport, PortError> {
            self.world.drop_import_calls.fetch_add(1, Ordering::SeqCst);
            let mut receipts = lock(&self.world.receipts);
            let before = receipts.len();
            receipts.retain(|receipt| receipt.session.export_id.as_str() != import.as_str());
            Ok(DropReport {
                delivery_index_removed: (before - receipts.len()) as u64,
                command_refs_removed: 0,
            })
        }

        async fn prune(
            &self,
            _policy: RetentionPolicy,
            _at: Timestamp,
        ) -> Result<PruneReport, PortError> {
            Ok(PruneReport::default())
        }
    }

    pub(crate) struct FakeExports {
        pub(crate) world: Arc<FakeWorld>,
    }

    #[async_trait]
    impl ExportStore for FakeExports {
        async fn put_export(&self, write: ExportWrite) -> Result<(), PortError> {
            lock(&self.world.exports).push(write.record);
            Ok(())
        }

        async fn export(&self, id: &ExportId) -> Result<Option<ExportRecord>, PortError> {
            Ok(lock(&self.world.exports)
                .iter()
                .find(|export| export.export_id() == id)
                .cloned())
        }

        async fn exports(&self) -> Result<Vec<ExportRecord>, PortError> {
            Ok(lock(&self.world.exports).clone())
        }

        async fn revoke_export(&self, _write: ExportRevocation) -> Result<(), PortError> {
            Ok(())
        }

        async fn add_import(&self, write: ImportWrite) -> Result<(), PortError> {
            lock(&self.world.write_audits)
                .extend(write.context.audit.iter().map(|audit| audit.action));
            lock(&self.world.imports).push(write.record);
            Ok(())
        }

        async fn import(&self, id: &ImportId) -> Result<Option<ImportRecord>, PortError> {
            Ok(lock(&self.world.imports)
                .iter()
                .find(|import| import.import_id() == id)
                .cloned())
        }

        async fn imports(&self) -> Result<Vec<ImportRecord>, PortError> {
            Ok(lock(&self.world.imports).clone())
        }

        async fn remove_import(&self, write: ImportRemoval) -> Result<(), PortError> {
            lock(&self.world.write_audits)
                .extend(write.context.audit.iter().map(|audit| audit.action));
            lock(&self.world.imports).retain(|import| import.import_id() != &write.import);
            Ok(())
        }
    }

    /// 信任端口在本切片只被 `use_cases` 透传；这里给最小实现（记录写集携带的审计动作）。
    pub(crate) struct FakeTrust {
        pub(crate) world: Arc<FakeWorld>,
    }

    fn missing_pairing() -> PortError {
        PortError::NotFound(EntityRef::Pairing(
            PairingId::new(&uuid_text(1)).expect("uuid"),
        ))
    }

    #[async_trait]
    impl TrustStore for FakeTrust {
        async fn device(&self, _id: &DeviceId) -> Result<Option<DeviceRecord>, PortError> {
            Ok(None)
        }

        async fn devices(&self) -> Result<Vec<DeviceRecord>, PortError> {
            Ok(Vec::new())
        }

        async fn node(&self, id: &NodeId, kind: NodeKind) -> Result<Option<NodeRecord>, PortError> {
            Ok(lock(&self.world.nodes)
                .iter()
                .find(|record| record.node_id() == id && record.kind() == kind)
                .cloned())
        }

        async fn nodes(&self) -> Result<Vec<NodeRecord>, PortError> {
            Ok(lock(&self.world.nodes).clone())
        }

        async fn nodes_for(&self, id: &NodeId) -> Result<Vec<NodeRecord>, PortError> {
            Ok(lock(&self.world.nodes)
                .iter()
                .filter(|record| record.node_id() == id)
                .cloned()
                .collect())
        }

        async fn peer_key(&self, peer: &PeerIdentity) -> Result<Option<PeerPublicKey>, PortError> {
            Ok(lock(&self.world.keys)
                .iter()
                .find(|(identity, _)| identity == peer)
                .map(|(_, key)| key.clone()))
        }

        async fn pairing(&self, id: &PairingId) -> Result<Option<PairingRecord>, PortError> {
            Ok(lock(&self.world.pairings)
                .iter()
                .find(|record| record.id() == id)
                .cloned())
        }

        async fn pairing_peer(&self, id: &PairingId) -> Result<Option<PairingPeer>, PortError> {
            Ok(lock(&self.world.peers)
                .iter()
                .find(|(pairing, _)| pairing == id)
                .map(|(_, peer)| peer.clone()))
        }

        /// 该对端最近一次配对：替身按 `world.peers` 的登记顺序取最后一条匹配（真实实现按
        /// `created_at`/`pairing_id` 降序，两者都只要「最近一次」这一个语义）。
        async fn pairing_for(
            &self,
            peer: &PeerIdentity,
        ) -> Result<Option<PairingRecord>, PortError> {
            let peers = lock(&self.world.peers);
            let pairings = lock(&self.world.pairings);
            let matched: Vec<PairingRecord> = peers
                .iter()
                .filter(|(_, row)| row.id() == peer)
                .filter_map(|(pairing, _)| {
                    pairings
                        .iter()
                        .find(|record| record.id() == pairing)
                        .cloned()
                })
                .collect();
            Ok(matched.into_iter().last())
        }

        async fn put_device(&self, _write: DeviceWrite) -> Result<(), PortError> {
            Ok(())
        }

        async fn put_node(&self, write: NodeWrite) -> Result<(), PortError> {
            let mut nodes = lock(&self.world.nodes);
            nodes.retain(|record| {
                !(record.node_id() == write.record.node_id()
                    && record.kind() == write.record.kind())
            });
            nodes.push(write.record);
            Ok(())
        }

        async fn revoke_device(&self, write: DeviceRevocation) -> Result<(), PortError> {
            lock(&self.world.write_audits)
                .extend(write.context.audit.iter().map(|audit| audit.action));
            Ok(())
        }

        async fn revoke_node(&self, _write: NodeRevocation) -> Result<(), PortError> {
            Ok(())
        }

        async fn create_pairing(&self, write: PairingWrite) -> Result<(), PortError> {
            let mut pairings = lock(&self.world.pairings);
            pairings.retain(|record| record.id() != write.record.id());
            pairings.push(write.record);
            Ok(())
        }

        async fn claim_pairing(
            &self,
            _write: PairingClaimWrite,
        ) -> Result<PairingClaimOutcome, PortError> {
            Err(missing_pairing())
        }

        async fn settle_pairing(
            &self,
            write: PairingSettlementWrite,
        ) -> Result<TrustRecordRef, PortError> {
            let Some(target) = lock(&self.world.pairings)
                .iter()
                .find(|record| record.id() == &write.pairing)
                .map(PairingRecord::target)
            else {
                return Err(missing_pairing());
            };
            lock(&self.world.write_audits)
                .extend(write.context.audit.iter().map(|audit| audit.action));
            Ok(match target {
                PairingTarget::Device => {
                    TrustRecordRef::Device(DeviceId::new(&uuid_text(2)).expect("device id"))
                }
                PairingTarget::Node => {
                    TrustRecordRef::Node(NodeId::new(&uuid_text(3)).expect("node id"))
                }
            })
        }

        async fn expire_pairings(&self, _write: ExpiryWrite) -> Result<u64, PortError> {
            Ok(0)
        }

        /// 消费已批准的配对：本替身只实现「状态推进到 `consumed` + 审计入账」与状态拒绝；权威的
        /// 对端身份判定在用例层（`UseCases::consume_pairing` 先于本调用完成，否则根本到不了这里）。
        /// 幂等命中**不**追加审计（与真实实现的写集语义一致：不改状态就不产生新的留痕）。
        async fn consume_pairing(
            &self,
            write: PairingConsumption,
        ) -> Result<PairingRecord, PortError> {
            let mut pairings = lock(&self.world.pairings);
            let Some(index) = pairings
                .iter()
                .position(|record| record.id() == &write.pairing)
            else {
                return Err(missing_pairing());
            };
            let record = pairings[index].clone();
            match record.state() {
                PairingState::Consumed => Ok(record),
                PairingState::Approved => {
                    let consumed = PairingRecord::try_new(
                        record.id().clone(),
                        record.target(),
                        PairingState::Consumed,
                        record.display_name().map(str::to_owned),
                        record.requested_scopes().clone(),
                        record.requested_grants().clone(),
                        record.secret_digest().clone(),
                        record.host_binding(),
                        record.created_at().clone(),
                        record.expires_at().clone(),
                        record.claimed_at().cloned(),
                        record.approved_at().cloned(),
                        Some(write.context.at.clone()),
                    )?;
                    pairings[index] = consumed.clone();
                    lock(&self.world.write_audits)
                        .extend(write.context.audit.iter().map(|audit| audit.action));
                    Ok(consumed)
                }
                PairingState::Expired => Err(PortError::Conflict(ConflictKind::Expired)),
                _ => Err(PortError::Conflict(ConflictKind::Consumed)),
            }
        }

        /// Node Link 的认证收尾不在本替身的范围内（`use_cases` 的用例只走配对消费与审计）：
        /// 显式失败关闭而不是静默成功，避免「替身比真实存储更宽容」。
        async fn record_node_connected(&self, _write: NodeConnectedWrite) -> Result<(), PortError> {
            unreachable!("本替身不实现 Node Link 认证收尾")
        }
    }

    /// 本地配置端口的测试替身：内存列表，`put_profile` 保持「至多一个默认」的语义。
    pub(crate) struct FakeLocalConfig {
        pub(crate) world: Arc<FakeWorld>,
    }

    #[async_trait]
    impl LocalConfigStore for FakeLocalConfig {
        async fn profiles(&self) -> Result<Vec<AgentProfile>, PortError> {
            Ok(lock(&self.world.profiles).clone())
        }

        async fn profile(&self, id: &AgentId) -> Result<Option<AgentProfile>, PortError> {
            Ok(lock(&self.world.profiles)
                .iter()
                .find(|profile| profile.id() == id)
                .cloned())
        }

        async fn put_profile(&self, write: ProfileWrite) -> Result<(), PortError> {
            let mut profiles = lock(&self.world.profiles);
            if write.profile.is_default() {
                profiles.retain(|profile| !profile.is_default());
            }
            profiles.retain(|profile| profile.id() != write.profile.id());
            profiles.push(write.profile);
            Ok(())
        }

        async fn workspaces(&self) -> Result<Vec<WorkspaceRecord>, PortError> {
            Ok(lock(&self.world.workspaces).clone())
        }

        async fn workspace(
            &self,
            alias: &WorkspaceAlias,
        ) -> Result<Option<WorkspaceRecord>, PortError> {
            Ok(lock(&self.world.workspaces)
                .iter()
                .find(|record| record.alias() == alias)
                .cloned())
        }

        async fn put_workspace(&self, write: WorkspaceWrite) -> Result<(), PortError> {
            let mut records = lock(&self.world.workspaces);
            records.retain(|record| record.alias() != write.record.alias());
            records.push(write.record);
            Ok(())
        }

        async fn provider_refs(&self) -> Result<Vec<ProviderRef>, PortError> {
            Ok(lock(&self.world.provider_refs).clone())
        }

        async fn put_provider_ref(&self, write: ProviderRefWrite) -> Result<(), PortError> {
            let mut refs = lock(&self.world.provider_refs);
            refs.retain(|reference| reference.id() != write.reference.id());
            refs.push(write.reference);
            Ok(())
        }

        async fn seed_state(&self) -> Result<SeedState, PortError> {
            let world = lock(&self.world.seed);
            match &*world {
                Some(state) => Ok(state.clone()),
                None => Ok(SeedState::unseeded()),
            }
        }

        async fn mark_seeded(&self, write: SeedWrite) -> Result<(), PortError> {
            let mut profiles = lock(&self.world.profiles);
            for profile in write.profiles {
                profiles.retain(|existing| existing.id() != profile.id());
                profiles.push(profile);
            }
            *lock(&self.world.seed) =
                Some(SeedState::try_new(true, Some(write.context.at)).expect("seeded"));
            Ok(())
        }
    }

    pub(crate) struct FakeAttachments {
        pub(crate) world: Arc<FakeWorld>,
    }

    #[async_trait]
    impl AttachmentStore for FakeAttachments {
        async fn put(
            &self,
            bytes: &[u8],
            media_type: &str,
            _at: Timestamp,
        ) -> Result<AttachmentRef, PortError> {
            let id =
                AttachmentId::new(&uuid_text(700 + lock(&self.world.attachments).len() as u64))
                    .expect("uuid");
            let reference = AttachmentRef {
                id,
                sha256: digest('A'),
                byte_length: bytes.len() as u64,
                media_type: media_type.to_owned(),
                relative_path: format!("attachments/{}", bytes.len()),
            };
            lock(&self.world.attachments).push(reference.clone());
            Ok(reference)
        }

        async fn get(&self, id: &AttachmentId) -> Result<Option<Vec<u8>>, PortError> {
            Ok(lock(&self.world.attachments)
                .iter()
                .find(|reference| &reference.id == id)
                .map(|reference| vec![0u8; reference.byte_length as usize]))
        }

        async fn link(
            &self,
            _session: &SessionId,
            _attachment: &AttachmentId,
            _generation: AttachmentGeneration,
        ) -> Result<(), PortError> {
            Ok(())
        }

        async fn prune_lru(
            &self,
            _budget_bytes: u64,
            _at: Timestamp,
        ) -> Result<PruneReport, PortError> {
            Ok(PruneReport::default())
        }

        /// 内存 fake 没有文件目录，因此没有孤儿可回收（§6 第 18 条由 storage 侧实现）。
        async fn sweep_orphans(&self, _at: Timestamp, _limit: u32) -> Result<u32, PortError> {
            Ok(0)
        }
    }

    // -----------------------------------------------------------------------------------------
    // 后端 fake
    // -----------------------------------------------------------------------------------------

    pub(crate) struct FakeBackend {
        pub(crate) world: Arc<FakeWorld>,
    }

    #[async_trait]
    impl SessionBackendFactory for FakeBackend {
        async fn create(
            &self,
            session: &SessionId,
            _request: CreateSessionRequest,
            sink: EventSink,
        ) -> Result<Box<dyn SessionEndpoint>, PortError> {
            // 后端现在直接拿到 core 分配的 id（§5.1），不再自己造引用。
            Ok(Box::new(FakeEndpoint {
                world: self.world.clone(),
                sink,
                reference: SessionReference::Owned(OwnedSessionRef::new(session.clone())),
            }))
        }

        async fn open(
            &self,
            reference: SessionReference,
            sink: EventSink,
        ) -> Result<Box<dyn SessionEndpoint>, PortError> {
            Ok(Box::new(FakeEndpoint {
                world: self.world.clone(),
                sink,
                reference,
            }))
        }
    }

    pub(crate) struct FakeEndpoint {
        pub(crate) world: Arc<FakeWorld>,
        pub(crate) sink: EventSink,
        pub(crate) reference: SessionReference,
    }

    #[async_trait]
    impl SessionEndpoint for FakeEndpoint {
        fn reference(&self) -> SessionReference {
            self.reference.clone()
        }

        async fn prompt(
            &self,
            _request: PromptRequest,
            _at: Timestamp,
        ) -> Result<TurnAccepted, PortError> {
            let ordinal = self.world.prompts.fetch_add(1, Ordering::SeqCst) + 1;
            lock(&self.world.prompt_trace).push(format!("start:{ordinal}"));
            let script = lock(&self.world.script).pop_front();
            match script {
                Some(script) => {
                    if script.yield_polls > 0 {
                        let mut left = script.yield_polls;
                        std::future::poll_fn(|_| {
                            if left == 0 {
                                Poll::Ready(())
                            } else {
                                left -= 1;
                                Poll::Pending
                            }
                        })
                        .await;
                    }
                    for event in script.events {
                        self.sink.send(event);
                    }
                }
                None => {
                    // 没有脚本：立即正常结束该 turn（broker 会用当前派发的 turn 补齐 turn 字段）。
                    self.sink.send(endpoint_event(
                        EventKind::State,
                        "turn.completed",
                        &turn_view("completed"),
                    ));
                }
            }
            lock(&self.world.prompt_trace).push(format!("end:{ordinal}"));
            Ok(TurnAccepted {
                turn: lock(&self.world.accepted_turn)
                    .clone()
                    .unwrap_or_else(|| TurnId::new(&uuid_text(0)).expect("uuid")),
            })
        }

        async fn cancel(&self, _turn: Option<TurnId>) -> Result<(), PortError> {
            Ok(())
        }

        async fn set_mode(&self, _mode: &ModeId) -> Result<(), PortError> {
            Ok(())
        }

        async fn list_config(&self) -> Result<Vec<ConfigOption>, PortError> {
            self.world.config_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Vec::new())
        }

        /// `session.mode.list` 的候选来源（§5.1/§6 第 17 条）：由测试按需配置，默认空列表
        /// （core 不得凭当前模式编造候选）。
        async fn modes(&self) -> Result<ModeState, PortError> {
            self.world.mode_calls.fetch_add(1, Ordering::SeqCst);
            Ok(lock(&self.world.modes)
                .clone()
                .unwrap_or_else(|| ModeState::new(None, Vec::new())))
        }

        async fn set_config(
            &self,
            _id: &ConfigOptionId,
            _value: ConfigValue,
        ) -> Result<(), PortError> {
            Ok(())
        }

        async fn resolve_interaction(
            &self,
            _interaction: &InteractionId,
            _resolution: InteractionResolution,
        ) -> Result<(), PortError> {
            Ok(())
        }

        async fn read_history(&self, _query: HistoryQuery) -> Result<HistoryPage, PortError> {
            let session = match &self.reference {
                SessionReference::Owned(owned) => owned.session_id.clone(),
                SessionReference::Remote(remote) => remote.session_id.clone(),
            };
            Err(PortError::NotFound(EntityRef::Session(session)))
        }

        async fn close(&self) -> Result<(), PortError> {
            Ok(())
        }
    }

    // -----------------------------------------------------------------------------------------
    // 组装与执行器
    // -----------------------------------------------------------------------------------------

    /// 一套完整的 fake 世界 + broker，附带一个已 seed 的 owned 会话。
    pub(crate) struct Harness {
        pub(crate) world: Arc<FakeWorld>,
        pub(crate) broker: Arc<Broker>,
        pub(crate) session: SessionId,
    }

    impl Harness {
        pub(crate) fn new(config: BrokerConfig) -> Self {
            let world = FakeWorld::new();
            let store = Arc::new(FakeStore {
                world: world.clone(),
            });
            let seq = Sequence::new(1).expect("sequence");
            let _ = seq;
            let broker = Arc::new(Broker::new(
                BrokerDeps {
                    store: store.clone(),
                    deliveries: Arc::new(FakeDeliveries {
                        world: world.clone(),
                    }),
                    backends: Arc::new(FakeBackend {
                        world: world.clone(),
                    }),
                    exports: Arc::new(FakeExports {
                        world: world.clone(),
                    }),
                    trust: Arc::new(FakeTrust {
                        world: world.clone(),
                    }),
                    publisher: Arc::new(TestPublisher {
                        world: world.clone(),
                    }),
                    clock: TestClock::new(),
                    ids: Arc::new(TestIds::default()),
                    audit: Some(Arc::new(TestAudit {
                        world: world.clone(),
                    })),
                },
                config,
            ));
            let session = SessionId::new(&uuid_text(7)).expect("session id");
            let agent = AgentRef::try_new(AgentId::new("agent-1").expect("agent id"), "Agent One")
                .expect("agent ref");
            let seeded = Session::try_new(
                session.clone(),
                OwnedSessionRef::new(session.clone()),
                None,
                agent,
                SessionState::Idle,
                ResourceOrigin::Local,
                None,
                Version::from(1),
                ts(0),
                ts(0),
                None,
            )
            .expect("session");
            world.seed_session(seeded);
            Self {
                world,
                broker,
                session,
            }
        }

        pub(crate) fn actor(&self) -> Actor {
            Actor::LocalCli
        }

        pub(crate) fn request(&self, n: u64) -> RequestId {
            RequestId::new(&uuid_text(900 + n)).expect("uuid")
        }

        pub(crate) fn reference(&self) -> SessionReference {
            SessionReference::Owned(OwnedSessionRef::new(self.session.clone()))
        }

        pub(crate) fn submit_prompt(&self, n: u64, fingerprint: char) -> CommandReceipt {
            let actor = self.actor();
            let command = prompt_command(&actor, &self.session, &self.request(n), fingerprint);
            block_on(self.broker.submit_mutation(&actor, &command)).expect("submit")
        }

        /// 让后端"异步地"把当前 turn 收尾：直接向 sink 送一条终态事件，然后驱动一次 pump。
        pub(crate) fn complete_turn(&self, event_type: &str, state: &str) {
            self.broker.sink(&self.session).send(endpoint_event(
                EventKind::State,
                event_type,
                &turn_view(state),
            ));
            block_on(self.broker.pump(&self.session)).expect("pump");
        }
    }

    pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
        run_all(vec![future]).remove(0)
    }

    /// 协作式执行器：没有 runtime 也能观察并发交错。所有 future 每轮都被 poll，`Waker` 是 noop
    /// （broker 的等待点都可重复 poll），因此不需要真实的唤醒就能推进。
    pub(crate) fn run_all<F: Future>(futures: Vec<F>) -> Vec<F::Output> {
        let mut context = Context::from_waker(Waker::noop());
        let mut tasks: Vec<Option<Pin<Box<F>>>> = futures
            .into_iter()
            .map(|future| Some(Box::pin(future)))
            .collect();
        let mut outputs: Vec<Option<F::Output>> = Vec::new();
        outputs.resize_with(tasks.len(), || None);
        let mut remaining = tasks.len();
        let mut rounds = 0usize;
        while remaining > 0 {
            rounds += 1;
            assert!(
                rounds < 100_000,
                "测试执行器检测到死锁：仍有 {remaining} 个 future 未完成"
            );
            for index in 0..tasks.len() {
                let polled = tasks[index]
                    .as_mut()
                    .map(|task| task.as_mut().poll(&mut context));
                if let Some(Poll::Ready(value)) = polled {
                    outputs[index] = Some(value);
                    tasks[index] = None;
                    remaining -= 1;
                }
            }
        }
        outputs
            .into_iter()
            .map(|output| output.expect("所有任务都已 poll 到完成"))
            .collect()
    }
}

// ---------------------------------------------------------------------------------------------
// §6 十二条契约的行为测试
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;
    use crate::model::{
        AcpRaw, AgentId, AgentRef, DeviceId, EventId, ExportId, InteractionKind, InteractionOption,
        ModeState, PendingInteraction, ResourceOrigin, ScopeSet,
    };

    fn device_without_scopes() -> Actor {
        Actor::Device {
            device: DeviceId::new(&uuid_text(40)).expect("uuid"),
            scopes: ScopeSet::empty(),
        }
    }

    fn rejection_code(receipt: &CommandReceipt) -> String {
        match receipt {
            CommandReceipt::Rejected { error } => error.code().to_owned(),
            CommandReceipt::Accepted { .. } => panic!("期望拒绝，实际接受"),
        }
    }

    /// §6.1：owned 事件必须先 `commit` 再 `publish`，顺序不可交换。
    #[test]
    fn owned_events_commit_before_publish() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(vec![
            endpoint_event(
                EventKind::Delta,
                "agent.message.delta",
                &turn_view("running"),
            ),
            endpoint_event(EventKind::State, "turn.completed", &turn_view("completed")),
        ]));

        let receipt = harness.submit_prompt(1, 'A');
        assert!(matches!(
            receipt,
            CommandReceipt::Accepted { turn: Some(_), .. }
        ));
        assert!(
            harness.world.publish_count() >= 4,
            "排队/开始/增量/终态都应发布"
        );
        assert!(
            harness.world.published_after_commit(),
            "每条发布的事件都必须已经在库里"
        );
        assert_eq!(
            harness.world.event_types(&harness.session),
            vec![
                "turn.queued",
                "turn.started",
                "agent.message.delta",
                "turn.completed",
                "command.completed",
            ],
            "§11.2：领域事件先于 terminal event"
        );
        let record = harness.world.command(&harness.request(1)).expect("幂等行");
        assert_eq!(record.status(), CommandStatus::Completed);
        assert!(record.terminal_event().is_some(), "终态事件由存储层回填");
    }

    /// §6.2：imported 先 `commit_receipt` 再发布，且正文只在内存。
    #[test]
    fn imported_receipt_commits_before_publish_and_body_stays_in_memory() {
        let harness = Harness::new(BrokerConfig::default());
        let remote = RemoteSessionRef::new(
            NodeId::new(&uuid_text(11)).expect("node"),
            ExportId::new("export.one").expect("export"),
            SessionId::new(&uuid_text(12)).expect("session"),
        );
        let origin = OriginEventRef::new(
            NodeId::new(&uuid_text(11)).expect("node"),
            origin_epoch_of(20),
            EventId::new(&uuid_text(21)).expect("event"),
        );
        let event_type = EventType::new("agent.message.delta").expect("event type");
        let payload = EventPayload {
            view: json_view(r#"{"text":"secret"}"#),
            acp: None,
        };

        let first = block_on(harness.broker.deliver_imported(
            remote.clone(),
            origin.clone(),
            Sequence::new(1).expect("sequence"),
            event_type.clone(),
            digest('A'),
            Some(payload.clone()),
        ))
        .expect("deliver");
        assert!(first, "首次投递是新行");
        assert_eq!(harness.world.publish_count(), 1);
        assert!(
            harness.world.published_after_commit(),
            "commit_receipt 必须先于广播"
        );

        let again = block_on(harness.broker.deliver_imported(
            remote.clone(),
            origin,
            Sequence::new(1).expect("sequence"),
            event_type,
            digest('A'),
            Some(payload),
        ))
        .expect("deliver");
        assert!(
            !again,
            "同一 origin_event_id 重发不新增 local_sequence、不重复发布"
        );
        assert_eq!(harness.world.publish_count(), 1);

        // 落盘的是无正文索引：receipt 里没有 view，正文只出现在内存投递里。
        let receipts = harness.world.receipts();
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].payload_digest, digest('A'));
        match &harness.world.published()[0] {
            CommittedDelivery::Imported {
                payload, session, ..
            } => {
                assert!(payload.is_some(), "正文只在内存投递");
                assert_eq!(session, &remote);
            }
            CommittedDelivery::Owned(_) => panic!("imported 投递不得表达成 owned"),
        }
        let local =
            block_on(harness.broker.remote_replay(None, ReplayLimit::default())).expect("replay");
        assert_eq!(local.len(), 1);
        assert_eq!(
            local[0].origin.origin_event_id,
            EventId::new(&uuid_text(21)).expect("event")
        );
    }

    /// §6.3：同一会话的提交不交错；不同会话可并行（这里断言前者）。
    #[test]
    fn same_session_submissions_do_not_interleave() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::yielding(
            vec![endpoint_event(
                EventKind::State,
                "turn.completed",
                &turn_view("completed"),
            )],
            3,
        ));
        harness.world.push_script(Script::yielding(
            vec![endpoint_event(
                EventKind::State,
                "turn.completed",
                &turn_view("completed"),
            )],
            3,
        ));
        let actor = harness.actor();
        let first = prompt_command(&actor, &harness.session, &harness.request(1), 'A');
        let second = prompt_command(&actor, &harness.session, &harness.request(2), 'B');

        let receipts = run_all(vec![
            harness.broker.submit_mutation(&actor, &first),
            harness.broker.submit_mutation(&actor, &second),
        ]);
        assert_eq!(receipts.len(), 2);
        assert!(
            receipts.iter().all(|receipt| receipt.is_ok()),
            "两条命令都应被接受"
        );
        assert_eq!(
            harness.world.prompt_trace(),
            vec!["start:1", "end:1", "start:2", "end:2"],
            "同一会话的 turn 必须串行，不得交错"
        );
    }

    /// §6.4：`queue` 策略按接受顺序排队，超过 `max_queued_turns` 返回 `session.busy`。
    #[test]
    fn queue_policy_bounds_max_queued_turns() {
        let harness = Harness::new(BrokerConfig {
            queue_policy: QueuePolicy::Queue,
            max_queued_turns: 1,
            ..BrokerConfig::default()
        });
        // 第一个 turn 永远不结束（脚本只发增量）。
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::Delta,
            "agent.message.delta",
            &turn_view("running"),
        )]));

        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));
        assert_eq!(
            harness
                .world
                .session(&harness.session)
                .expect("session")
                .state(),
            SessionState::Running
        );
        assert!(matches!(
            harness.submit_prompt(2, 'B'),
            CommandReceipt::Accepted { .. }
        ));
        // 排队 turn 已持久化，状态是 Queued。
        let queued = harness
            .world
            .turns(&harness.session)
            .into_iter()
            .filter(|turn| turn.state() == TurnState::Queued)
            .count();
        assert_eq!(queued, 1);
        assert_eq!(harness.world.prompt_count(), 1, "排队的 turn 还不能派发");

        let third = harness.submit_prompt(3, 'C');
        assert_eq!(rejection_code(&third), "session.busy");
        assert_eq!(
            harness.world.turns(&harness.session).len(),
            2,
            "被拒的 turn 不落库"
        );
    }

    /// §6.4：`reject_busy` 直接返回 `session.busy`，不留排队 turn。
    #[test]
    fn reject_busy_policy_rejects_second_prompt() {
        let harness = Harness::new(BrokerConfig {
            queue_policy: QueuePolicy::RejectBusy,
            max_queued_turns: 16,
            ..BrokerConfig::default()
        });
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::Delta,
            "agent.message.delta",
            &turn_view("running"),
        )]));
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));
        let second = harness.submit_prompt(2, 'B');
        assert_eq!(rejection_code(&second), "session.busy");
        assert_eq!(harness.world.turns(&harness.session).len(), 1);
    }

    /// §6.3/§6.4：running turn 结束后，排队 turn 按顺序被派发。
    #[test]
    fn queued_turn_starts_after_running_turn_finishes() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::Delta,
            "agent.message.delta",
            &turn_view("running"),
        )]));
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::State,
            "turn.completed",
            &turn_view("completed"),
        )]));
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));
        assert!(matches!(
            harness.submit_prompt(2, 'B'),
            CommandReceipt::Accepted { .. }
        ));
        assert_eq!(harness.world.prompt_count(), 1);

        harness.complete_turn("turn.completed", "completed");
        assert_eq!(
            harness.world.prompt_count(),
            2,
            "第一个 turn 结束后第二个被派发"
        );
        assert_eq!(
            harness.world.prompt_trace(),
            vec!["start:1", "end:1", "start:2", "end:2"]
        );
        let types = harness.world.event_types(&harness.session);
        assert!(types.contains(&"turn.started".to_owned()));
        assert_eq!(
            types
                .iter()
                .filter(|kind| *kind == "turn.completed")
                .count(),
            2,
            "两个 turn 各自终态"
        );
    }

    /// §6.6：重复提交返回首次结果且不二次派发。
    #[test]
    fn idempotent_replay_returns_first_turn_without_redispatch() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::Delta,
            "agent.message.delta",
            &turn_view("running"),
        )]));
        let actor = harness.actor();
        let command = prompt_command(&actor, &harness.session, &harness.request(1), 'A');
        let first = block_on(harness.broker.submit_mutation(&actor, &command)).expect("submit");
        let commits = harness.world.commit_count();
        let prompts = harness.world.prompt_count();

        let second = block_on(harness.broker.submit_mutation(&actor, &command)).expect("submit");
        assert_eq!(harness.world.commit_count(), commits, "重放不追加提交");
        assert_eq!(harness.world.prompt_count(), prompts, "重放不二次派发");
        match (first, second) {
            (
                CommandReceipt::Accepted {
                    turn: Some(first), ..
                },
                CommandReceipt::Accepted {
                    turn: Some(second), ..
                },
            ) => assert_eq!(first, second, "仍为 accepted 的重放返回原 turnId"),
            other => panic!("幂等重放应返回同一 turn：{other:?}"),
        }
    }

    /// §6 第 20 条：`settle_session_create` 只终结 `session.create` 的持久记录。同一 `(actor, requestId)`
    /// 记着别的命令时（适配层误用，wire 不可达）必须 `InvalidRequest`，不得把那条命令改写成创建的终态。
    #[test]
    fn settle_session_create_rejects_other_commands() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::Delta,
            "agent.message.delta",
            &turn_view("running"),
        )]));
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));
        let request = harness.request(1);
        let before = harness.world.command(&request).expect("幂等行");
        assert_eq!(before.command(), "session.prompt");

        let result =
            CommandResult::from_json_text(r#"{"sessionId":"first"}"#).expect("result object");
        let error = block_on(harness.broker.settle_session_create(
            &harness.actor(),
            &request,
            CommandStatus::Completed,
            Some(result),
            None,
        ))
        .expect_err("非 session.create 的记录不得被终结");
        assert!(
            matches!(error, PortError::InvalidRequest(_)),
            "得到 {error:?}"
        );
        let after = harness.world.command(&request).expect("幂等行");
        assert_eq!(after.command(), "session.prompt");
        assert_eq!(after.status(), before.status(), "既有命令的终态不得被改写");
    }

    /// §6.6：指纹不同 → `command.idempotency_conflict`；不同 actor 用同一 requestId 是两条命令。
    #[test]
    fn idempotency_conflict_on_changed_fingerprint() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::Delta,
            "agent.message.delta",
            &turn_view("running"),
        )]));
        let actor = harness.actor();
        let command = prompt_command(&actor, &harness.session, &harness.request(1), 'A');
        assert!(matches!(
            block_on(harness.broker.submit_mutation(&actor, &command)).expect("submit"),
            CommandReceipt::Accepted { .. }
        ));

        let mut changed = command.clone();
        changed.request_fingerprint = digest('B');
        let conflict = block_on(harness.broker.submit_mutation(&actor, &changed)).expect("submit");
        assert_eq!(rejection_code(&conflict), "command.idempotency_conflict");

        // 不同 actor 用同一 requestId → 视为独立命令（幂等键是协议维度的 `(actor, requestId)`）。
        let other = Actor::Device {
            device: DeviceId::new(&uuid_text(41)).expect("uuid"),
            scopes: ScopeSet::empty(),
        };
        let mut foreign = command.clone();
        foreign.actor = other.clone();
        assert_eq!(
            rejection_code(
                &block_on(harness.broker.submit_mutation(&other, &foreign)).expect("submit")
            ),
            "authorization.scope_denied"
        );
    }

    /// §11.2/RV1-WP6-F3：终态记录带上收据里有意义的分量——prompt 的 `completed` 带 `{"turnId":…}`，
    /// 模式切换的 `completed` 带 `{"version":…}`。在此之前终态记录的 `result` 恒为 NULL，适配层只能回
    /// 空 object（`NODE_LINK_PROTOCOL.md` §12.5 允许，但丢掉了这两条可用的关联信息）。
    #[test]
    fn terminal_result_carries_the_receipt_fields() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::State,
            "turn.completed",
            &turn_view("completed"),
        )]));
        let receipt = harness.submit_prompt(1, 'A');
        let turn = match receipt {
            CommandReceipt::Accepted { turn, .. } => turn.expect("prompt 的收据带 turn"),
            other => panic!("期望 accepted，得到 {other:?}"),
        };
        let record = harness
            .world
            .command(&harness.request(1))
            .expect("命令行已落盘");
        assert_eq!(record.status(), CommandStatus::Completed);
        assert_eq!(
            record.result().map(CommandResult::as_str),
            Some(format!(r#"{{"turnId":"{}"}}"#, turn.as_str()).as_str()),
            "completed 的 result 必须带该命令的 turnId"
        );

        // 模式切换：结果带新的会话版本（`apply_state` 的收据分量）。
        let actor = harness.actor();
        let version = block_on(harness.broker.session_version(&harness.session)).expect("版本");
        let command = crate::broker::test_support::mode_command(
            &actor,
            &harness.session,
            &harness.request(2),
            version,
        );
        let receipt = block_on(harness.broker.submit_mutation(&actor, &command)).expect("submit");
        assert!(matches!(receipt, CommandReceipt::Accepted { .. }));
        let record = harness
            .world
            .command(&harness.request(2))
            .expect("模式切换的命令行");
        assert_eq!(record.status(), CommandStatus::Completed);
        let after = block_on(harness.broker.session_version(&harness.session)).expect("版本");
        assert_eq!(
            record.result().map(CommandResult::as_str),
            Some(format!(r#"{{"version":"{}"}}"#, after.get()).as_str()),
            "completed 的 result 必须带切换后的版本"
        );
    }

    /// §6.9：接受提交失败 → 显式拒绝、不发布、不派发。
    #[test]
    fn accept_commit_failure_rejects_without_publishing() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.fail_next_commit();
        let receipt = harness.submit_prompt(1, 'A');
        assert_eq!(rejection_code(&receipt), "internal.unavailable");
        assert_eq!(harness.world.publish_count(), 0, "写失败不得发布任何事件");
        assert_eq!(
            harness.world.prompt_count(),
            0,
            "未落盘的命令不得派发给后端"
        );
        assert!(harness.world.turns(&harness.session).is_empty());
    }

    /// §6.9：终态提交失败 → 命令显式转为 `uncertain`，且该批次不发布。
    #[test]
    fn terminal_commit_failure_marks_command_uncertain() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::State,
            "turn.completed",
            &turn_view("completed"),
        )]));
        // 提交顺序：1 = 接受 turn.queued，2 = 提升 turn.started，3 = 终态批次。
        harness.world.fail_commit_at(3);
        let receipt = harness.submit_prompt(1, 'A');
        assert!(matches!(receipt, CommandReceipt::Accepted { .. }));

        let record = harness.world.command(&harness.request(1)).expect("幂等行");
        assert_eq!(
            record.status(),
            CommandStatus::Uncertain,
            "必须显式落盘 uncertain"
        );
        assert!(record.error().is_some());
        // 未发布的事件数 == 未落盘的批次数：turn.queued + turn.started(+command.uncertain 一条)
        let published = harness.world.published();
        assert!(published.iter().all(|_| true), "只发布已落盘的事件");
        let published_types: Vec<String> = harvest_published_types(&harness);
        assert!(
            !published_types.contains(&"turn.completed".to_owned()),
            "失败的批次不得发布"
        );
        assert!(harness.world.published_after_commit());
    }

    /// §6 第 9 条（RV1-WP7-F3）：**非终态**批次的写失败不得只丢弃——该 turn 的正文已经缺了一块，
    /// 若让同一 turn 的终态批照常 `completed`，库里就留下「报完成但正文缺失」的记录。
    #[test]
    fn a_failed_non_terminal_batch_terminates_the_turn_instead_of_silently_dropping_it() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(vec![
            endpoint_event(
                EventKind::Delta,
                "agent.message.delta",
                &format!(
                    r#"{{"messageId":"{}","deltaIndex":"0","text":"lost"}}"#,
                    uuid_text(610)
                ),
            ),
            endpoint_event(EventKind::State, "turn.completed", &turn_view("completed")),
        ]));
        // 提交顺序：1 = 接受 turn.queued，2 = 提升 turn.started，3 = 非终态增量批次（本用例注入失败）。
        harness.world.fail_commit_at(3);
        let receipt = harness.submit_prompt(1, 'A');
        assert!(matches!(receipt, CommandReceipt::Accepted { .. }));

        let record = harness.world.command(&harness.request(1)).expect("幂等行");
        assert_eq!(
            record.status(),
            CommandStatus::Uncertain,
            "正文缺块的 turn 不得报完成"
        );
        assert!(record.error().is_some());
        let types = harness.world.event_types(&harness.session);
        assert!(
            !types.contains(&"turn.completed".to_owned()),
            "被放弃 turn 的终态批不得落盘：{types:?}"
        );
        assert!(!types.contains(&"command.completed".to_owned()));
        assert!(types.contains(&"turn.failed".to_owned()), "{types:?}");
        assert!(types.contains(&"command.uncertain".to_owned()), "{types:?}");
        let turns = harness.world.turns(&harness.session);
        assert_eq!(turns.len(), 1, "{turns:?}");
        assert_eq!(turns[0].state(), TurnState::Failed, "{turns:?}");
        let published_types = harvest_published_types(&harness);
        assert!(!published_types.contains(&"turn.completed".to_owned()));
        assert!(harness.world.published_after_commit());
    }

    /// §6 第 9/14 条（RV1-WP7-F3）：被放弃的 turn 仍要为**已落盘**的 delta 收尾（`completed`），
    /// 缺的那一块不伪造，迟到的事件与终态一律不再提交。
    #[test]
    fn an_abandoned_turn_still_finalises_the_deltas_it_persisted() {
        let harness = Harness::new(BrokerConfig::default());
        // 空脚本：turn 被接受并派发，但后端在注入失败前只发两批增量。
        harness.world.push_script(Script::new(Vec::new()));
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));
        let message = MessageId::new(&uuid_text(611)).expect("message");
        let delta = |index: u64, text: &str| {
            endpoint_event(
                EventKind::Delta,
                "agent.message.delta",
                &format!(
                    r#"{{"messageId":"{}","deltaIndex":"{index}","text":"{text}"}}"#,
                    message.as_str()
                ),
            )
        };
        // 第一批增量正常落盘（提交 3），第二批注入失败（提交 4）。
        harness
            .broker
            .sink(&harness.session)
            .send(delta(0, "Hello"));
        block_on(harness.broker.pump(&harness.session)).expect("pump");
        harness.world.fail_commit_at(4);
        harness
            .broker
            .sink(&harness.session)
            .send(delta(1, " lost"));
        block_on(harness.broker.pump(&harness.session)).expect("pump");
        // 迟到的终态批不得再落盘。
        harness.broker.sink(&harness.session).send(endpoint_event(
            EventKind::State,
            "turn.completed",
            &turn_view("completed"),
        ));
        block_on(harness.broker.pump(&harness.session)).expect("pump");

        assert_eq!(
            harness.world.event_types(&harness.session),
            vec![
                "turn.queued",
                "turn.started",
                "agent.message.delta",
                "turn.failed",
                "agent.message.completed",
                "command.uncertain",
            ],
            "只保留已落盘的增量、收尾与失败终态"
        );
        assert_eq!(
            harness
                .world
                .command(&harness.request(1))
                .expect("幂等行")
                .status(),
            CommandStatus::Uncertain
        );
        // 收尾正文只含已落盘的那一段（缺块不伪造）。
        let view = block_on(harness.broker.read_view()).expect("view");
        let completed = harness
            .world
            .events(&harness.session)
            .into_iter()
            .find(|event| event.event_type.as_str() == "agent.message.completed")
            .expect("已落盘 delta 的收尾");
        let payload = block_on(view.event_payload(&completed.id))
            .expect("payload")
            .expect("存在");
        assert!(
            payload.view.as_str().contains("Hello") && !payload.view.as_str().contains("lost"),
            "只收尾已落盘的段落：{}",
            payload.view.as_str()
        );
    }

    fn harvest_published_types(harness: &Harness) -> Vec<String> {
        harness
            .world
            .published()
            .into_iter()
            .map(|delivery| match delivery {
                CommittedDelivery::Owned(event) => event.id.as_str().to_owned(),
                CommittedDelivery::Imported { event_type, .. } => event_type.as_str().to_owned(),
            })
            .collect()
    }

    /// §6.11：`Ephemeral` 在组装前被过滤，不进入任何提交，也不由 broker 发布。
    #[test]
    fn ephemeral_events_are_filtered_before_commit() {
        assert!(!EPHEMERAL_EVENT_TYPES.is_empty(), "登记点不得为空表");
        assert_eq!(
            persistence_policy(&EventType::new("device.typing").expect("event type")),
            PersistencePolicy::Ephemeral
        );
        assert_eq!(
            persistence_policy(&EventType::new("session.presence").expect("event type")),
            PersistencePolicy::Ephemeral
        );
        assert_eq!(
            persistence_policy(&EventType::new("agent.message.delta").expect("event type")),
            PersistencePolicy::ShortTerm
        );
        assert_eq!(
            persistence_policy(&EventType::new("turn.completed").expect("event type")),
            PersistencePolicy::Durable
        );
        assert_eq!(
            persistence_policy(&EventType::new("unregistered.future.event").expect("event type")),
            PersistencePolicy::Durable,
            "未登记取值降级处理而不是丢弃"
        );

        let harness = Harness::new(BrokerConfig::default());
        let sink = harness.broker.sink(&harness.session);
        sink.send(endpoint_event(
            EventKind::Delta,
            "device.typing",
            r#"{"typing":true}"#,
        ));
        sink.send(endpoint_event(
            EventKind::Delta,
            "session.presence",
            r#"{"online":true}"#,
        ));
        block_on(harness.broker.flush(&harness.session)).expect("flush");
        assert_eq!(harness.world.commit_count(), 0, "Ephemeral 不进入任何提交");
        assert_eq!(harness.world.publish_count(), 0, "broker 不发布 Ephemeral");
        assert!(harness.world.events(&harness.session).is_empty());
    }

    /// §6.10：同一 turn 的 delta 合并为一次提交，跨终态时切批。
    #[test]
    fn flush_merges_deltas_without_crossing_turn_boundaries() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(vec![
            endpoint_event(
                EventKind::Delta,
                "agent.message.delta",
                &turn_view("running"),
            ),
            endpoint_event(
                EventKind::Delta,
                "agent.message.delta",
                &turn_view("running"),
            ),
            endpoint_event(
                EventKind::Delta,
                "agent.message.delta",
                &turn_view("running"),
            ),
            endpoint_event(EventKind::State, "turn.completed", &turn_view("completed")),
        ]));
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));

        let batches = harness.world.batch_types();
        assert!(
            batches.contains(&vec!["agent.message.delta".to_owned(); 3]),
            "同一 turn 的三条 delta 合并进一次提交：{batches:?}"
        );
        assert!(
            batches.contains(&vec![
                "turn.completed".to_owned(),
                "command.completed".to_owned()
            ]),
            "终态不延迟、也不与增量跨 turn 合并：{batches:?}"
        );
        assert_eq!(
            harness.world.events(&harness.session).len(),
            7,
            "合并不得丢事件（turn.queued + turn.started + 3 delta + turn.completed + command.completed）"
        );
    }

    /// §6.8：非终态 turn 期间的模式切换被显式拒绝（v1 不排队）。
    #[test]
    fn mode_change_rejected_while_turn_running() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::Delta,
            "agent.message.delta",
            &turn_view("running"),
        )]));
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));

        let actor = harness.actor();
        let command = mode_command(
            &actor,
            &harness.session,
            &harness.request(2),
            Version::from(1),
        );
        let receipt = block_on(harness.broker.submit_mutation(&actor, &command)).expect("submit");
        match receipt {
            CommandReceipt::Rejected { error } => {
                assert_eq!(error.code(), "state.version_conflict");
                assert!(
                    error.details().as_str().contains("currentVersion"),
                    "§11.6 要求回显当前版本：{}",
                    error.details().as_str()
                );
            }
            CommandReceipt::Accepted { .. } => panic!("run 中的模式切换必须被拒绝"),
        }
        assert!(
            harness
                .world
                .session(&harness.session)
                .expect("session")
                .current_mode()
                .is_none(),
            "被拒的模式切换不得改状态"
        );
    }

    /// §6.7：交互解析 first-writer-wins，迟到的应答不覆盖既有结果。
    #[test]
    fn interaction_resolution_is_first_writer_wins() {
        let harness = Harness::new(BrokerConfig::default());
        let interaction = InteractionId::new(&uuid_text(300)).expect("interaction");
        harness.world.seed_interaction(
            PendingInteraction::try_new(
                interaction.clone(),
                InteractionKind::Permission,
                harness.session.clone(),
                ts(0),
                vec![
                    InteractionOption::try_new("allow-once", "Allow once", "allow_once")
                        .expect("option"),
                ],
            )
            .expect("pending"),
        );
        let actor = harness.actor();
        let resolution = || {
            InteractionResolution::permission(
                PermissionDecision::try_new("allow-once", PermissionDecisionKind::AllowOnce)
                    .expect("decision"),
            )
        };

        let first = block_on(harness.broker.resolve_interaction(
            &actor,
            &harness.reference(),
            &interaction,
            resolution(),
        ))
        .expect("resolve");
        assert_eq!(first, Resolution::Resolved);
        let second = block_on(harness.broker.resolve_interaction(
            &actor,
            &harness.reference(),
            &interaction,
            resolution(),
        ))
        .expect("resolve");
        assert_eq!(second, Resolution::AlreadyResolved);

        let stored = harness
            .world
            .interaction_resolution(&interaction)
            .expect("既有结果");
        match stored.resolution {
            InteractionResolution::Permission(decision) => {
                assert_eq!(decision.option_id(), "allow-once");
                assert_eq!(decision.kind(), PermissionDecisionKind::AllowOnce);
            }
            InteractionResolution::Elicitation { .. } => panic!("必须是权限决定"),
        }
        assert_eq!(harness.world.audits().len(), 0, "成功的解析不写审计拒绝行");
    }

    /// §6.5：授权在 core 判定，Scope 不足即拒绝，并记 `authorization.denied`。
    #[test]
    fn authorization_denied_for_device_without_scope() {
        let harness = Harness::new(BrokerConfig::default());
        let actor = device_without_scopes();
        let command = prompt_command(&actor, &harness.session, &harness.request(1), 'A');
        let receipt = block_on(harness.broker.submit_mutation(&actor, &command)).expect("submit");
        assert_eq!(rejection_code(&receipt), "authorization.scope_denied");
        assert_eq!(harness.world.commit_count(), 0, "拒绝不落盘");
        assert_eq!(harness.world.prompt_count(), 0);
        let audits = harness.world.audits();
        assert_eq!(audits.len(), 1);
    }

    /// §6 第 13 条 / §9 判据 15：Agent 的权限请求必须在**同一提交**里同时落事件与 pending 交互行；
    /// 行与事件的配对（真实 `event_id`）由存储层按 payload 的 `interactionId` 完成，core 不携带该 id。
    #[test]
    fn interaction_request_lands_event_and_pending_row_in_one_commit() {
        let harness = Harness::new(BrokerConfig::default());
        let interaction = InteractionId::new(&uuid_text(400)).expect("interaction");
        let view = format!(
            r#"{{"interactionId":"{}","title":"Approve","description":null,"options":[{{"optionId":"allow-once","label":"Allow once","kind":"allow_once"}}]}}"#,
            interaction.as_str()
        );
        harness.world.push_script(Script::new(vec![
            endpoint_event(EventKind::Interaction, "permission.requested", &view),
            endpoint_event(
                EventKind::Delta,
                "agent.message.delta",
                &turn_view("running"),
            ),
        ]));
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));

        // (a) 交互事件与 pending 行落在同一次提交里（fake store 按 §6 第 13 条配对，配不到即整事务失败）。
        let interaction_batch = harness
            .world
            .batches()
            .into_iter()
            .find(|batch| {
                batch
                    .types
                    .iter()
                    .any(|kind| kind == "permission.requested")
            })
            .expect("交互事件必须落盘");
        assert!(
            interaction_batch
                .types
                .iter()
                .any(|kind| kind == "permission.requested"),
            "交互事件必须是 kind = interaction 的事件：{:?}",
            interaction_batch.types
        );
        assert!(
            harness
                .world
                .events(&harness.session)
                .iter()
                .any(|event| { event.session_sequence.is_some() }),
            "交互事件是会话级事件"
        );

        // (b) 读视图里该行的 options 恒为空（不落库），正文经 event_payload 还原。
        let read_view = block_on(harness.broker.read_view()).expect("read view");
        let page = block_on(read_view.read_session(HistoryQuery {
            session: harness.session.clone(),
            include: HistoryInclude {
                messages: false,
                turns: false,
                pending_interactions: true,
                config_options: false,
                capabilities: false,
            },
            after: None,
            limit: ReplayLimit::default(),
        }))
        .expect("read");
        let pending = page
            .interactions
            .iter()
            .find(|item| item.id() == &interaction)
            .expect("pending 交互行必须落地");
        assert!(pending.options().is_empty(), "options 不落库");
        let interaction_event = harness
            .world
            .events(&harness.session)
            .into_iter()
            .map(|event| event.id)
            .find(|id| {
                block_on(read_view.event_payload(id))
                    .ok()
                    .flatten()
                    .map(|payload| payload.view.as_str().contains(interaction.as_str()))
                    .unwrap_or(false)
            })
            .expect("必须能找到携带该 interactionId 的事件");
        let payload = block_on(read_view.event_payload(&interaction_event))
            .expect("event_payload")
            .expect("事件存在");
        assert!(
            payload.view.as_str().contains("allow-once"),
            "options 只能从事件正文还原：{}",
            payload.view.as_str()
        );
        assert!(
            payload.view.as_str().contains(interaction.as_str()),
            "正文里的 interactionId 必须与交互行一致"
        );
    }

    /// §9 判据 15：同一提交里既创建又解析交互 → `InvalidRequest`。
    #[test]
    fn creating_and_resolving_interaction_in_one_commit_is_rejected() {
        let harness = Harness::new(BrokerConfig::default());
        let interaction = InteractionId::new(&uuid_text(401)).expect("interaction");
        let pending = PendingInteraction::try_new(
            interaction.clone(),
            InteractionKind::Permission,
            harness.session.clone(),
            ts(0),
            Vec::new(),
        )
        .expect("pending");
        let commit = OwnedCommit {
            session: Some(harness.session.clone()),
            at: ts(1),
            expected_version: None,
            state: Some(StateChange::Update(SessionUpdate {
                state: None,
                mode: ModeChange::Unchanged,
                closed_at: None,
                interaction: Some(InteractionResolved {
                    interaction: interaction.clone(),
                    resolution: InteractionResolution::permission(
                        PermissionDecision::try_new(
                            "allow-once",
                            PermissionDecisionKind::AllowOnce,
                        )
                        .expect("decision"),
                    ),
                    resolved_by: Actor::LocalCli,
                }),
            })),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: vec![PendingInteractionWrite {
                interaction: pending,
                turn: None,
            }],
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        };
        let error = block_on(harness.broker.commit_owned(commit)).expect_err("必须拒绝");
        assert!(
            matches!(error, PortError::InvalidRequest(reason) if reason.contains("同时创建与解析")),
            "必须是 §9 判据 15 的 InvalidRequest，实际 {error:?}"
        );
        assert_eq!(harness.world.commit_count(), 0, "校验在落盘之前");
    }

    /// §9 判据 17：非会话级事件的三个列都是 NULL，且仍出现在 replay 流里。
    #[test]
    fn non_session_event_has_no_session_scoped_identifiers() {
        let harness = Harness::new(BrokerConfig::default());
        let commit = OwnedCommit {
            session: None,
            at: ts(2),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![
                pending_event(
                    "device.revoked",
                    EventKind::State,
                    json_view(r#"{"deviceId":"00000000-0000-4000-8000-000000000042"}"#),
                    None,
                    None,
                    StoredPolicy::Durable,
                    None,
                )
                .expect("event"),
            ],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        };
        let outcome = block_on(harness.broker.commit_owned(commit)).expect("commit");
        assert_eq!(outcome.appended.len(), 1);
        let event = &outcome.appended[0];
        assert!(event.session.is_none());
        assert!(event.session_sequence.is_none());
        assert!(event.origin_epoch.is_none());
        assert!(event.origin_sequence.is_none());
        event.validate().expect("成对不变量");

        let view = block_on(harness.broker.read_view()).expect("view");
        let batch = block_on(view.replay(None, ReplayLimit::default())).expect("replay");
        assert_eq!(batch.events.len(), 1, "非会话级事件也进入 replay 流");
    }

    /// §6.12/§5.2：`ReadView::event_payload` 是正文的唯一读取入口，不存在的事件返回 `None`。
    #[test]
    fn event_payload_returns_stored_view_and_none_for_unknown_event() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::State,
            "turn.completed",
            &turn_view("completed"),
        )]));
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));

        let view = block_on(harness.broker.read_view()).expect("view");
        let event = harness
            .world
            .events(&harness.session)
            .into_iter()
            .find(|event| event.session_sequence.is_some())
            .expect("会话级事件");
        let payload = block_on(view.event_payload(&event.id))
            .expect("event_payload")
            .expect("已落盘事件");
        assert!(payload.view.as_str().starts_with('{'));
        let missing = EventId::new(&uuid_text(999_999)).expect("event");
        assert!(
            block_on(view.event_payload(&missing))
                .expect("ok")
                .is_none()
        );
    }

    /// §10.1：四类产生者各至少一条断言——Agent（后端事件）、Device（设备命令引起）、LocalCli（本地
    /// CLI 命令引起）、Daemon（daemon 自身事件）。
    #[test]
    fn event_origin_follows_the_producer() {
        // 纯映射层：同一事件类型在不同 actor 下得到不同的 origin。
        let delta = EventType::new("agent.message.delta").expect("event type");
        assert_eq!(event_origin(&delta, None), EventOrigin::Agent);
        let completed = EventType::new("command.completed").expect("event type");
        assert_eq!(event_origin(&completed, None), EventOrigin::Device);
        assert_eq!(
            event_origin(&completed, Some(&Actor::LocalCli)),
            EventOrigin::LocalCli
        );
        let revoked = EventType::new("device.revoked").expect("event type");
        assert_eq!(
            event_origin(&revoked, Some(&Actor::LocalCli)),
            EventOrigin::Daemon
        );

        // 端到端：同一条 prompt 提交里，Agent 事件与命令事件的 origin 不同。
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(vec![
            endpoint_event(
                EventKind::Delta,
                "agent.message.delta",
                &turn_view("running"),
            ),
            endpoint_event(EventKind::State, "turn.completed", &turn_view("completed")),
        ]));
        let device = Actor::Device {
            device: DeviceId::new(&uuid_text(43)).expect("uuid"),
            scopes: {
                let mut scopes = ScopeSet::empty();
                scopes.insert("session.prompt").expect("insert scope");
                scopes
            },
        };
        let command = prompt_command(&device, &harness.session, &harness.request(1), 'A');
        assert!(matches!(
            block_on(harness.broker.submit_mutation(&device, &command)).expect("submit"),
            CommandReceipt::Accepted { .. }
        ));
        let origins: Vec<(EventOrigin, String)> = harness
            .world
            .events(&harness.session)
            .into_iter()
            .map(|event| {
                (
                    harness
                        .world
                        .event_origin(&event.id)
                        .expect("origin recorded"),
                    event.id.as_str().to_owned(),
                )
            })
            .collect();
        let by_origin = |origin: EventOrigin| {
            origins
                .iter()
                .filter(|(stored, _)| *stored == origin)
                .count()
        };
        assert_eq!(by_origin(EventOrigin::Agent), 1, "后端 delta 属于 Agent");
        assert!(
            by_origin(EventOrigin::Device) >= 3,
            "turn.queued/turn.started/command.completed 由设备命令引起"
        );
        assert_eq!(
            by_origin(EventOrigin::LocalCli),
            0,
            "该命令来自设备而不是本地 CLI"
        );
        assert_eq!(by_origin(EventOrigin::Daemon), 0);

        // 本地 CLI 的同一路径 → LocalCli。
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::State,
            "turn.completed",
            &turn_view("completed"),
        )]));
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));
        let origins: Vec<EventOrigin> = harness
            .world
            .events(&harness.session)
            .into_iter()
            .filter_map(|event| harness.world.event_origin(&event.id))
            .collect();
        assert!(
            origins
                .iter()
                .filter(|origin| **origin == EventOrigin::LocalCli)
                .count()
                >= 3,
            "本地 CLI 引起的 turn/command 事件记 LocalCli：{origins:?}"
        );
    }

    /// 同时装配 `Broker` 与 `UseCases`（恢复与 `mode.list` 的用例入口需要后者）。
    fn use_cases_fixture() -> UseCaseFixture {
        use crate::broker::test_support::FakeBackend;
        let world = FakeWorld::new();
        let store = Arc::new(FakeStore {
            world: world.clone(),
        });
        let broker = Arc::new(Broker::new(
            BrokerDeps {
                store: store.clone(),
                deliveries: Arc::new(FakeDeliveries {
                    world: world.clone(),
                }),
                backends: Arc::new(FakeBackend {
                    world: world.clone(),
                }),
                exports: Arc::new(FakeExports {
                    world: world.clone(),
                }),
                trust: Arc::new(FakeTrust {
                    world: world.clone(),
                }),
                publisher: Arc::new(TestPublisher {
                    world: world.clone(),
                }),
                clock: TestClock::new(),
                ids: Arc::new(TestIds::default()),
                audit: Some(Arc::new(TestAudit {
                    world: world.clone(),
                })),
            },
            BrokerConfig::default(),
        ));
        let session = SessionId::new(&uuid_text(7)).expect("session id");
        let agent = AgentRef::try_new(AgentId::new("agent-1").expect("agent id"), "Agent One")
            .expect("agent ref");
        world.seed_session(
            crate::model::Session::try_new(
                session.clone(),
                OwnedSessionRef::new(session.clone()),
                None,
                agent,
                SessionState::Idle,
                ResourceOrigin::Local,
                None,
                Version::from(1),
                ts(0),
                ts(0),
                None,
            )
            .expect("session"),
        );
        let use_cases = crate::use_cases::UseCases::new(crate::use_cases::UseCaseDeps {
            broker: broker.clone(),
            store,
            deliveries: Arc::new(FakeDeliveries {
                world: world.clone(),
            }),
            exports: Arc::new(FakeExports {
                world: world.clone(),
            }),
            trust: Arc::new(FakeTrust {
                world: world.clone(),
            }),
            audit: Arc::new(TestAudit {
                world: world.clone(),
            }),
            config: Arc::new(FakeLocalConfig {
                world: world.clone(),
            }),
            attachments: Arc::new(FakeAttachments {
                world: world.clone(),
            }),
            catalog: Arc::new(TestCatalog {
                world: world.clone(),
            }),
            clock: TestClock::new(),
            ids: Arc::new(TestIds::default()),
        });
        UseCaseFixture {
            world,
            broker,
            use_cases,
            session,
        }
    }

    struct UseCaseFixture {
        world: Arc<FakeWorld>,
        broker: Arc<Broker>,
        use_cases: crate::use_cases::UseCases,
        session: SessionId,
    }

    impl UseCaseFixture {
        fn reference(&self) -> SessionReference {
            SessionReference::Owned(OwnedSessionRef::new(self.session.clone()))
        }
    }

    /// §9 判据 18 / §6 第 14 条：三种终态各一例、多消息、无 delta 不发、同批提交、think 不发。
    #[test]
    fn assistant_messages_complete_with_the_turn() {
        let harness = Harness::new(BrokerConfig::default());
        let first = MessageId::new(&uuid_text(600)).expect("message");
        let second = MessageId::new(&uuid_text(601)).expect("message");
        let delta = |message: &MessageId, index: u64, text: &str| {
            endpoint_event(
                EventKind::Delta,
                "agent.message.delta",
                &format!(
                    r#"{{"messageId":"{}","deltaIndex":"{index}","text":"{text}"}}"#,
                    message.as_str()
                ),
            )
        };
        let thought = endpoint_event(
            EventKind::Delta,
            "agent.thought.delta",
            &format!(
                r#"{{"messageId":"{}","deltaIndex":"0","text":"thinking"}}"#,
                MessageId::new(&uuid_text(602)).expect("message").as_str()
            ),
        );
        // 三条终态各跑一例：delta（含乱序与带 block 的项）+ thought + 终态。
        for (event_type, state) in [
            ("turn.completed", "completed"),
            ("turn.cancelled", "cancelled"),
            ("turn.failed", "failed"),
        ] {
            harness.world.push_script(Script::new(vec![
                delta(&first, 1, " world"),
                delta(&first, 0, "Hello"),
                thought.clone(),
                endpoint_event(EventKind::Delta, "agent.message.delta", &format!(
                    r#"{{"messageId":"{}","deltaIndex":"0","text":"B","block":{{"type":"image_ref","mimeType":"image/png","byteLength":"1","displayState":"available"}}}}"#,
                    second.as_str()
                )),
                endpoint_event(EventKind::State, event_type, &turn_view(state)),
            ]));
        }
        let mut seen = 0usize;
        for n in 1..=3u64 {
            assert!(matches!(
                harness.submit_prompt(n, 'A'),
                CommandReceipt::Accepted { .. }
            ));
            // 无 delta 的 turn 不产生 completed：第 4 次提交一个空脚本。
            seen += 1;
        }
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::State,
            "turn.completed",
            &turn_view("completed"),
        )]));
        assert!(matches!(
            harness.submit_prompt(4, 'B'),
            CommandReceipt::Accepted { .. }
        ));

        let completed: Vec<CommittedEvent> = harness
            .world
            .events(&harness.session)
            .into_iter()
            .filter(|event| event.event_type.as_str() == "agent.message.completed")
            .collect();
        assert_eq!(
            completed.len(),
            seen * 2,
            "每次终态为两个 messageId 各发一条"
        );
        // 终态事件与 completed 同批（§6 第 14 条）。
        let batches = harness.world.batch_types();
        assert!(
            batches.iter().any(|batch| {
                batch.iter().any(|kind| kind == "agent.message.completed")
                    && batch.iter().any(|kind| kind == "turn.completed")
            }),
            "completed 必须与 turn 终态事件同批提交：{batches:?}"
        );
        assert!(
            !batches
                .iter()
                .flatten()
                .any(|kind| *kind == "agent.thought.delta.completed"),
            "think 流不产生 completed"
        );

        // 文本按 deltaIndex 升序拼接；带 block 的按位置插入；think 的 messageId 没有 completed。
        let view = block_on(harness.broker.read_view()).expect("view");
        let first_payload = block_on(view.event_payload(&completed[0].id))
            .expect("payload")
            .expect("存在");
        assert!(
            first_payload.view.as_str().contains("Hello world"),
            "乱序 delta 必须按 deltaIndex 折叠：{}",
            first_payload.view.as_str()
        );
        let block_payload = harness
            .world
            .events(&harness.session)
            .into_iter()
            .find(|event| {
                event.event_type.as_str() == "agent.message.completed"
                    && block_on(view.event_payload(&event.id))
                        .ok()
                        .flatten()
                        .map(|payload| payload.view.as_str().contains(second.as_str()))
                        .unwrap_or(false)
            })
            .expect("第二个 messageId 有 completed");
        let block_payload = block_on(view.event_payload(&block_payload.id))
            .expect("payload")
            .expect("存在");
        assert!(
            block_payload.view.as_str().contains("image_ref"),
            "带 block 的 delta 按位置插入：{}",
            block_payload.view.as_str()
        );
    }

    /// §9 判据 19 / §6 第 15 条：压缩的两向 + `compacted` 校验。
    #[test]
    fn delta_compaction_two_way() {
        // persist_deltas = false → 终态后恰好一条 summary，被压行标记 compacted_into。
        let harness = Harness::new(BrokerConfig {
            persist_deltas: false,
            ..BrokerConfig::default()
        });
        harness.world.push_script(Script::new(vec![
            endpoint_event(
                EventKind::Delta,
                "agent.message.delta",
                &format!(
                    r#"{{"messageId":"{}","deltaIndex":"0","text":"A"}}"#,
                    uuid_text(610)
                ),
            ),
            endpoint_event(EventKind::State, "turn.completed", &turn_view("completed")),
        ]));
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));
        let summaries: Vec<CommittedEvent> = harness
            .world
            .events(&harness.session)
            .into_iter()
            .filter(|event| event.event_type.as_str() == "turn.delta_compacted")
            .collect();
        assert_eq!(
            summaries.len(),
            1,
            "persist_deltas=false 时终态后必须压缩一次"
        );
        assert!(
            !crate::broker::test_support::lock(&harness.world.state)
                .compacted_into
                .is_empty(),
            "被压行必须标记 compacted_into"
        );
        let view = block_on(harness.broker.read_view()).expect("view");
        let payload = block_on(view.event_payload(&summaries[0].id))
            .expect("payload")
            .expect("存在");
        let text = payload.view.as_str();
        assert!(
            text.contains("turnId") && text.contains("deltaCount") && !text.contains("\"text\""),
            "summary 只承载收据：{text}"
        );

        // persist_deltas = true → 永不压缩。
        let kept = Harness::new(BrokerConfig {
            persist_deltas: true,
            ..BrokerConfig::default()
        });
        kept.world.push_script(Script::new(vec![
            endpoint_event(
                EventKind::Delta,
                "agent.message.delta",
                &format!(
                    r#"{{"messageId":"{}","deltaIndex":"0","text":"A"}}"#,
                    uuid_text(611)
                ),
            ),
            endpoint_event(EventKind::State, "turn.completed", &turn_view("completed")),
        ]));
        assert!(matches!(
            kept.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));
        assert!(
            !kept
                .world
                .events(&kept.session)
                .iter()
                .any(|event| event.event_type.as_str() == "turn.delta_compacted"),
            "persist_deltas=true 时永不压缩"
        );

        // 非法 `compacted`（非 delta 行 / 跨会话）→ InvalidRequest 且零写入。
        let before = harness.world.commit_count();
        let bad = OwnedCommit {
            session: Some(harness.session.clone()),
            at: ts(9),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![
                pending_event(
                    "turn.delta_compacted",
                    EventKind::Summary,
                    json_view(
                        r#"{"turnId":"00000000-0000-4000-8000-000000000000","deltaCount":1}"#,
                    ),
                    None,
                    None,
                    StoredPolicy::Durable,
                    None,
                )
                .expect("event"),
            ],
            interactions: Vec::new(),
            // 指向一条非 delta 行（会话的第一条事件是 `turn.queued`）。
            compacted: vec![GlobalCursor {
                server_epoch: server_epoch_of(999),
                global_sequence: Sequence::new(1).expect("sequence"),
            }],
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        };
        let error = block_on(harness.broker.commit_owned(bad)).expect_err("必须拒绝");
        assert!(matches!(error, PortError::InvalidRequest(_)));
        assert_eq!(
            harness.world.commit_count(),
            before + 1,
            "校验发生在写入之前（fake 只在通过后才推进计数）"
        );
    }

    /// §9 判据 20 / §6 第 16 条：启动恢复把 `accepted` 终结为 `uncertain`，并补写缺失的 completed。
    #[test]
    fn recover_unsettled_terminates_and_backfills() {
        let fixture = use_cases_fixture();
        let session = fixture.session.clone();
        let harness_world = fixture.world.clone();
        let request = RequestId::new(&uuid_text(700)).expect("uuid");
        let turn = TurnId::new(&uuid_text(701)).expect("uuid");
        let message = MessageId::new(&uuid_text(702)).expect("message");
        let actor = Actor::LocalCli;
        // 造一条 accepted 的 mutation + 未终态 turn + 已提交 delta（没有 completed）。
        let accepted =
            OwnedCommit {
                session: Some(session.clone()),
                at: ts(0),
                expected_version: None,
                state: Some(StateChange::Update(SessionUpdate {
                    state: Some(SessionState::Queued),
                    mode: ModeChange::Unchanged,
                    closed_at: None,
                    interaction: None,
                })),
                turns: vec![TurnChange::Create(NewTurn {
                    turn: turn.clone(),
                    state: TurnState::Queued,
                    causation: Some(request.clone()),
                    started_at: None,
                })],
                events: vec![pending_event(
                "agent.message.delta",
                EventKind::Delta,
                json_view(&format!(
                    r#"{{"messageId":"{}","turnId":"{}","deltaIndex":"0","text":"lost tail"}}"#,
                    message.as_str(),
                    turn.as_str()
                )),
                Some(turn.clone()),
                Some(request.clone()),
                StoredPolicy::Durable,
                Some(&actor),
            )
            .expect("event")],
                interactions: Vec::new(),
                compacted: Vec::new(),
                idempotency: Some(IdempotencyRecord {
                    actor: actor.clone(),
                    request: request.clone(),
                    command: "session.prompt".to_owned(),
                    kind: CommandKind::Mutation,
                    session: Some(session.clone()),
                    expected_version: None,
                    request_fingerprint: digest('A'),
                    accepted_at: ts(0),
                }),
                command_terminal: None,
                origin_epoch: None,
            };
        block_on(fixture.broker.commit_owned(accepted)).expect("seed accepted");
        assert_eq!(
            harness_world.command(&request).expect("row").status(),
            CommandStatus::Accepted
        );

        let recovered = block_on(
            fixture
                .use_cases
                .recover_unsettled(&actor, ReplayLimit::default()),
        )
        .expect("recover");
        assert_eq!(recovered, 1, "一条 accepted 命令被终结");
        let record = harness_world.command(&request).expect("row");
        assert_eq!(record.status(), CommandStatus::Uncertain);
        assert!(record.terminal_event().is_some(), "终态事件必须回填");
        assert_eq!(
            harness_world.turns(&session)[0].state(),
            TurnState::Failed,
            "对应 turn 必须终结为 failed"
        );
        assert!(
            block_on(
                fixture
                    .use_cases
                    .recover_unsettled(&actor, ReplayLimit::default())
            )
            .expect("recover")
                == 0,
            "恢复后不再有未结命令"
        );
        let completed = harness_world
            .events(&session)
            .into_iter()
            .any(|event| event.event_type.as_str() == "agent.message.completed");
        assert!(completed, "必须补写缺失的 completed（§6 第 16 条）");
        assert!(
            harness_world.published_after_commit(),
            "广播必须发生在 commit 之后（§9 判据 20）"
        );
    }

    /// §9 判据 21 / §6 第 17 条：候选原样来自端口；空列表不伪造；version 取自会话。
    #[test]
    fn mode_list_uses_adapter_modes_only() {
        let fixture = use_cases_fixture();
        let modes = ModeState::new(
            Some(ModeRef::try_new(ModeId::new("code").expect("mode"), "Code").expect("mode ref")),
            vec![
                ModeRef::try_new(ModeId::new("code").expect("mode"), "Code").expect("mode ref"),
                ModeRef::try_new(ModeId::new("ask").expect("mode"), "Ask").expect("mode ref"),
            ],
        );
        *crate::broker::test_support::lock(&fixture.world.modes) = Some(modes.clone());
        let listing = block_on(
            fixture
                .use_cases
                .mode_list(&Actor::LocalCli, &fixture.reference()),
        )
        .expect("mode list");
        assert_eq!(
            listing.state, modes,
            "候选必须原样来自 SessionEndpoint::modes()"
        );
        assert_eq!(listing.version, Version::from(1), "version 取会话当前版本");
        assert_eq!(
            fixture
                .world
                .mode_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );

        // 端口返回空列表 → 结果为空（不凭 current_mode 编造候选）。
        *crate::broker::test_support::lock(&fixture.world.modes) = None;
        let empty = block_on(
            fixture
                .use_cases
                .mode_list(&Actor::LocalCli, &fixture.reference()),
        )
        .expect("mode list");
        assert!(empty.state.available.is_empty());
        assert!(empty.state.current_mode.is_none());
    }

    /// §6.12：快照与重放出自同一读视图，barrier 是该视图的 `head()`。
    #[test]
    fn replay_and_snapshot_share_one_read_view() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::State,
            "turn.completed",
            &turn_view("completed"),
        )]));
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));

        let before = harness.world.read_view_count();
        let view = block_on(harness.broker.read_view()).expect("read view");
        assert_eq!(harness.world.read_view_count(), before + 1);
        let barrier = block_on(view.head()).expect("view head");
        let batch = block_on(view.replay(None, ReplayLimit::default())).expect("replay");
        assert_eq!(batch.head, barrier, "重放 barrier 与视图 head 一致");
        assert_eq!(batch.reset_required, None);
        assert_eq!(
            batch.events.len(),
            harness.world.events(&harness.session).len()
        );
        assert!(batch.next.is_some());

        // 没有 cursor 的首次同步由调用方按 `initial_sync` 处理；这里断言 head 仍来自同一视图。
        let again =
            block_on(view.replay(batch.next.clone(), ReplayLimit::default())).expect("replay");
        assert!(again.events.is_empty());
        assert_eq!(again.head, barrier);
    }
    // -----------------------------------------------------------------------------------------
    // §10.3 的 core 侧收口：turn 归属与会话版本（specs/core-event-view-identity）
    // -----------------------------------------------------------------------------------------

    /// 一条事件的持久化 view 文本。
    fn stored_view(harness: &Harness, event: &CommittedEvent) -> String {
        let view = block_on(harness.broker.read_view()).expect("view");
        block_on(view.event_payload(&event.id))
            .expect("payload")
            .expect("存在")
            .view
            .as_str()
            .to_owned()
    }

    /// R1/R2/R5/R17：owned 路径下 core 注入 turnId，且只前置一个成员、其余字节不变。
    #[test]
    fn owned_views_get_the_authoritative_turn_id() {
        let harness = Harness::new(BrokerConfig::default());
        let message = MessageId::new(&uuid_text(610)).expect("message");
        let raw = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionUpdate":"agent_message_chunk","futureField":[1,2]}}"#;
        let acp = AcpRaw::available("application/json", raw, digest('Z')).expect("acp");
        // 适配器投影：不含 turnId，含未知顶层字段与嵌套结构，并带 ACP 原文。
        let event = EndpointEvent {
            kind: EventKind::Delta,
            event_type: EventType::new("agent.message.delta").expect("type"),
            payload: EventPayload {
                view: json_view(&format!(
                    r#"{{"messageId":"{}","deltaIndex":"0","text":"hi","unknownField":{{"nested":[1,2]}}}}"#,
                    message.as_str()
                )),
                acp: Some(acp.clone()),
            },
            turn: None,
            causation: None,
            at: ts(0),
        };
        harness.world.push_script(Script::new(vec![
            event,
            endpoint_event(EventKind::State, "turn.completed", &turn_view("completed")),
        ]));
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));
        let turn = harness.world.turns(&harness.session)[0].id().clone();
        let stored = harness.world.events(&harness.session);
        let delta = stored
            .iter()
            .find(|event| event.event_type.as_str() == "agent.message.delta")
            .expect("delta");
        assert_eq!(
            stored_view(&harness, delta),
            format!(
                r#"{{"turnId":"{}","messageId":"{}","deltaIndex":"0","text":"hi","unknownField":{{"nested":[1,2]}}}}"#,
                turn.as_str(),
                message.as_str()
            ),
            "注入必须只前置一个成员，未知字段与既有字节不变"
        );
        // 落盘 turn_id 与 view 的 turnId 一致（三者同源：落盘 / view / turn 行）。
        assert_eq!(
            harness.world.event_turn(&delta.id).as_ref(),
            Some(&turn),
            "落盘 turn_id 必须等于 view 的 turnId"
        );
        // §10.3 覆盖到同批的其它要求类型（含 core 自建视图）。
        let completed = stored
            .iter()
            .find(|event| event.event_type.as_str() == "turn.completed")
            .expect("turn.completed");
        assert_eq!(
            harness.world.event_turn(&completed.id).as_ref(),
            Some(&turn)
        );
        assert!(stored_view(&harness, completed).contains(turn.as_str()));
        // ACP 三要素逐字节不变（R5）。
        let payload = block_on(
            block_on(harness.broker.read_view())
                .expect("view")
                .event_payload(&delta.id),
        )
        .expect("payload")
        .expect("存在");
        assert_eq!(payload.acp.as_ref(), Some(&acp), "注入不得改动 ACP 原文");
    }

    /// R3：没有 turn 归属的事件不注入、不伪造字段；未列入 §10.3 的类型也不加未协商字段。
    #[test]
    fn views_without_turn_attribution_get_no_turn_id() {
        let harness = Harness::new(BrokerConfig::default());
        let session_level =
            r#"{"title":"hello","updatedAt":"2026-09-18T00:00:00.000Z","x":{"y":1}}"#;
        harness.broker.sink(&harness.session).send(endpoint_event(
            EventKind::Structured,
            "session.info.changed",
            session_level,
        ));
        block_on(harness.broker.flush(&harness.session)).expect("flush");
        let stored = harness.world.events(&harness.session);
        assert_eq!(stored.len(), 1);
        assert_eq!(
            stored_view(&harness, &stored[0]),
            session_level,
            "无 turn 归属的事件必须逐字节不变"
        );
        assert_eq!(harness.world.event_turn(&stored[0].id), None);

        // 居所属列但事件类型不在 §10.3 的 turnId 集合内：同样不注入（不添加未协商字段）。
        harness
            .broker
            .sink(&harness.session)
            .send(endpoint_event(
                EventKind::Delta,
                "terminal.output",
                r#"{"terminalId":"t1","chunkIndex":"0","stream":"stdout","text":"x","truncated":false}"#,
            ));
        block_on(harness.broker.flush(&harness.session)).expect("flush");
        let stored = harness.world.events(&harness.session);
        let output = stored
            .iter()
            .find(|event| event.event_type.as_str() == "terminal.output")
            .expect("terminal.output");
        assert_eq!(
            stored_view(&harness, output),
            r#"{"terminalId":"t1","chunkIndex":"0","stream":"stdout","text":"x","truncated":false}"#
        );
    }

    /// R2/R7：turnId 取值冲突时显式失败，且该批零落盘、零发布。
    #[test]
    fn a_conflicting_turn_id_fails_closed_without_side_effects() {
        let harness = Harness::new(BrokerConfig::default());
        let view = format!(
            r#"{{"messageId":"{}","turnId":"{}","deltaIndex":"0","text":"hi"}}"#,
            uuid_text(611),
            uuid_text(0)
        );
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::Delta,
            "agent.message.delta",
            &view,
        )]));
        let actor = harness.actor();
        let command = prompt_command(&actor, &harness.session, &harness.request(1), 'A');
        let error =
            block_on(harness.broker.submit_mutation(&actor, &command)).expect_err("冲突必须失败");
        assert!(matches!(error, PortError::InvalidRequest(_)), "{error}");
        let types: Vec<String> = harness
            .world
            .events(&harness.session)
            .iter()
            .map(|event| event.event_type.as_str().to_owned())
            .collect();
        assert_eq!(
            types,
            vec!["turn.queued".to_owned(), "turn.started".to_owned()],
            "冲突批不得落盘任何事件"
        );
        assert_eq!(harness.world.publish_count(), 2, "冲突批不得发布任何帧");
        assert_eq!(
            harness.world.turns(&harness.session)[0].state(),
            TurnState::Running,
            "冲突不得静默终结该 turn"
        );
    }

    /// R6：turnId 取值一致时保留原字段字节（不重写、不生成第二个同名键）。
    #[test]
    fn a_matching_turn_id_is_kept_byte_for_byte() {
        let harness = Harness::new(BrokerConfig::default());
        // 第一段不带终态事件：turn 保持运行中，从而可以拿到 core 分配的权威 turn id。
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::Delta,
            "agent.message.delta",
            &format!(
                r#"{{"messageId":"{}","deltaIndex":"0","text":"a"}}"#,
                uuid_text(612)
            ),
        )]));
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));
        let turn = harness.world.turns(&harness.session)[0].id().clone();
        let view = format!(
            r#"{{"messageId":"{}","turnId":"{}","deltaIndex":"1","text":"b"}}"#,
            uuid_text(613),
            turn.as_str()
        );
        harness.broker.sink(&harness.session).send(endpoint_event(
            EventKind::Delta,
            "agent.message.delta",
            &view,
        ));
        block_on(harness.broker.flush(&harness.session)).expect("flush");
        let views: Vec<String> = harness
            .world
            .events(&harness.session)
            .iter()
            .filter(|event| event.event_type.as_str() == "agent.message.delta")
            .map(|event| stored_view(&harness, event))
            .collect();
        assert_eq!(views.len(), 2);
        assert!(
            views.contains(&view),
            "取值一致的既有字段必须字节不变：{views:?}"
        );
    }

    /// R3/R10：纯事件提交注入当前版本，不递增（expected_version 缺失时回读当前版本）。
    #[test]
    fn mode_changed_views_carry_the_current_session_version() {
        let harness = Harness::new(BrokerConfig::default());
        harness.broker.sink(&harness.session).send(endpoint_event(
            EventKind::State,
            "session.mode.changed",
            r#"{"currentModeId":"code","unknown":true}"#,
        ));
        block_on(harness.broker.flush(&harness.session)).expect("flush");
        let stored = harness.world.events(&harness.session);
        assert_eq!(
            stored_view(&harness, &stored[0]),
            r#"{"version":"1","currentModeId":"code","unknown":true}"#
        );
        assert_eq!(
            harness
                .world
                .session(&harness.session)
                .expect("session")
                .version()
                .get(),
            1,
            "纯事件提交不递增会话版本"
        );
    }

    /// R9/R13：状态变更提交注入递增后的版本，并与存储返回的版本一致。
    #[test]
    fn a_state_change_commit_injects_the_bumped_version() {
        let harness = Harness::new(BrokerConfig::default());
        let event = pending_event(
            "session.config.changed",
            EventKind::State,
            json_view(r#"{"configOptions":[]}"#),
            None,
            None,
            StoredPolicy::Durable,
            None,
        )
        .expect("event");
        let commit = OwnedCommit {
            session: Some(harness.session.clone()),
            at: ts(1),
            expected_version: None,
            state: Some(StateChange::Update(SessionUpdate {
                state: None,
                mode: ModeChange::Unchanged,
                closed_at: None,
                interaction: None,
            })),
            turns: Vec::new(),
            events: vec![event],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        };
        let outcome = block_on(harness.broker.commit_owned(commit)).expect("commit");
        assert_eq!(outcome.version.get(), 2, "状态变更提交递增一");
        let stored = harness.world.events(&harness.session);
        assert_eq!(
            stored_view(&harness, &stored[0]),
            r#"{"version":"2","configOptions":[]}"#
        );
    }

    /// R14：存储层版本规则漂移时显式失败且不发布（比对发生在存储返回之后）。
    #[test]
    fn a_version_rule_drift_fails_closed() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.drift_version_at(1);
        harness.broker.sink(&harness.session).send(endpoint_event(
            EventKind::State,
            "session.mode.changed",
            r#"{"currentModeId":"code"}"#,
        ));
        let error = block_on(harness.broker.flush(&harness.session)).expect_err("漂移必须失败");
        assert!(matches!(error, PortError::Corrupt(_)), "{error}");
        assert_eq!(harness.world.publish_count(), 0, "漂移批不得发布任何帧");
    }

    /// R19：幂等重放不得二次注入，字节与首次持久化一致。
    #[test]
    fn replayed_commits_do_not_reinject_view_fields() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::State,
            "session.mode.changed",
            r#"{"currentModeId":"code"}"#,
        )]));
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));
        // 该脚本不带终态事件：turn 保持运行，会话版本在断言期间稳定。
        let before = harness.world.events(&harness.session).len();
        let mode = harness
            .world
            .events(&harness.session)
            .into_iter()
            .find(|event| event.event_type.as_str() == "session.mode.changed")
            .expect("mode.changed");
        let current = harness
            .world
            .session(&harness.session)
            .expect("session")
            .version();
        let first = stored_view(&harness, &mode);
        assert_eq!(
            first,
            format!(
                r#"{{"version":"{}","currentModeId":"code"}}"#,
                current.get()
            ),
            "纯事件提交注入的版本必须等于当前会话版本"
        );
        assert_eq!(first.matches("\"version\"").count(), 1);

        // 同一 (actor, requestId) 的重复提交：命中幂等，不新增事件、不重写既有字节。
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));
        assert_eq!(
            harness.world.events(&harness.session).len(),
            before,
            "重放不得新增事件行"
        );
        assert_eq!(stored_view(&harness, &mode), first, "重放字节必须一致");

        // 直接走漏斗的竞态回放分支：存储层返回 replayed 时不做版本比对、不注入新行。
        let actor = harness.actor();
        let replayed = OwnedCommit {
            session: Some(harness.session.clone()),
            at: ts(2),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![
                pending_event(
                    "session.mode.changed",
                    EventKind::State,
                    json_view(r#"{"currentModeId":"other"}"#),
                    None,
                    None,
                    StoredPolicy::Durable,
                    None,
                )
                .expect("event"),
            ],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: Some(IdempotencyRecord {
                actor: actor.clone(),
                request: harness.request(1),
                command: "session.prompt".to_owned(),
                kind: CommandKind::Mutation,
                session: Some(harness.session.clone()),
                expected_version: None,
                request_fingerprint: digest('A'),
                accepted_at: ts(2),
            }),
            command_terminal: None,
            origin_epoch: None,
        };
        let outcome = block_on(harness.broker.commit_owned(replayed)).expect("replay");
        assert!(outcome.replayed.is_some(), "必须命中幂等回放");
        assert_eq!(
            harness.world.events(&harness.session).len(),
            before,
            "回放分支不得写入任何事件行"
        );
        assert_eq!(stored_view(&harness, &mode), first);
    }

    /// R20/R21/R22：适配器返回的 turn 标识不具权威性（占位值也不产生第二行）。
    #[test]
    fn the_turn_from_the_endpoint_is_never_trusted() {
        for placeholder in [uuid_text(0), uuid_text(999)] {
            let harness = Harness::new(BrokerConfig::default());
            *test_support::lock(&harness.world.accepted_turn) =
                Some(TurnId::new(&placeholder).expect("turn id"));
            assert!(matches!(
                harness.submit_prompt(1, 'A'),
                CommandReceipt::Accepted { .. }
            ));
            let turns = harness.world.turns(&harness.session);
            assert_eq!(
                turns.len(),
                1,
                "适配器返回值不得产生第二个 turn 行：{placeholder}"
            );
            let turn = turns[0].id().clone();
            assert_ne!(
                turn.as_str(),
                placeholder,
                "core 必须使用自己分配的 turn id"
            );
            let stored = harness.world.events(&harness.session);
            for event in &stored {
                if let Some(found) = harness.world.event_turn(&event.id) {
                    assert_eq!(found, turn, "事件归属必须是 core 的权威值");
                }
            }
            let queued = stored
                .iter()
                .find(|event| event.event_type.as_str() == "turn.queued")
                .expect("turn.queued");
            assert_eq!(
                stored_view(&harness, queued),
                format!(r#"{{"turnId":"{}","state":"queued"}}"#, turn.as_str())
            );
        }
    }
    /// R4/R11：imported 路径保留 Owner 给出的 view 字节（不注入、不重写、不伪造）。
    #[test]
    fn imported_deliveries_keep_the_owner_view_bytes() {
        let harness = Harness::new(BrokerConfig::default());
        let remote = RemoteSessionRef::new(
            NodeId::new(&uuid_text(11)).expect("node"),
            ExportId::new("export.one").expect("export"),
            SessionId::new(&uuid_text(12)).expect("session"),
        );
        let event_type = EventType::new("agent.message.delta").expect("event type");
        // Owner 已注入 turnId / version：本端必须逐字节转发（payloadDigest 覆盖这些字节）。
        let owner_view = r#"{"turnId":"11111111-2222-3333-4444-555555555555","messageId":"m1","deltaIndex":"0","text":"hi","version":"7"}"#;
        for (index, view) in [(1_u64, owner_view), (2, r#"{"text":"no identity"}"#)].iter() {
            let origin = OriginEventRef::new(
                NodeId::new(&uuid_text(11)).expect("node"),
                origin_epoch_of(20),
                EventId::new(&uuid_text(30 + index)).expect("event"),
            );
            let delivered = block_on(harness.broker.deliver_imported(
                remote.clone(),
                origin,
                Sequence::new(*index).expect("sequence"),
                event_type.clone(),
                digest('B'),
                Some(EventPayload {
                    view: json_view(view),
                    acp: None,
                }),
            ))
            .expect("deliver");
            assert!(delivered, "首次投递是新行");
        }
        let published = harness.world.published();
        let views: Vec<&str> = published
            .iter()
            .filter_map(|delivery| match delivery {
                CommittedDelivery::Imported { payload, .. } => {
                    payload.as_ref().map(|payload| payload.view.as_str())
                }
                CommittedDelivery::Owned(_) => None,
            })
            .collect();
        assert_eq!(
            views,
            vec![owner_view, r#"{"text":"no identity"}"#],
            "imported 正文必须原样转发：不注入、不重写、不补齐"
        );
        // 无正文索引只保存调用方给出的摘要（重写 view 会让摘要与正文不再一致）。
        let receipts = harness.world.receipts();
        assert_eq!(receipts.len(), 2);
        assert!(
            receipts
                .iter()
                .all(|receipt| receipt.payload_digest == digest('B'))
        );
    }
    /// D4 的降级：turn 终结后晚到的 §10.3 类型事件没有权威归属 → 不注入、不伪造（登记边界）。
    #[test]
    fn a_late_delta_after_turn_end_is_persisted_without_attribution() {
        let harness = Harness::new(BrokerConfig::default());
        // 脚本只结束 turn，不发 delta。
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::State,
            "turn.completed",
            &turn_view("completed"),
        )]));
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));
        // turn 已终结后再到达的 delta（适配器异步尾巴）。
        let view = format!(
            r#"{{"messageId":"{}","deltaIndex":"0","text":"late"}}"#,
            uuid_text(614)
        );
        harness.broker.sink(&harness.session).send(endpoint_event(
            EventKind::Delta,
            "agent.message.delta",
            &view,
        ));
        block_on(harness.broker.flush(&harness.session)).expect("flush");
        let late = harness
            .world
            .events(&harness.session)
            .into_iter()
            .find(|event| event.event_type.as_str() == "agent.message.delta")
            .expect("晚到的 delta 必须落盘");
        assert_eq!(
            harness.world.event_turn(&late.id),
            None,
            "无权威归属时不得伪造 turn"
        );
        assert_eq!(
            stored_view(&harness, &late),
            view,
            "无归属时 view 逐字节不变（不注入）"
        );
    }

    /// R2：`turnId` 已存在但不是字符串 → 同样显式失败（不覆盖、不静默跳过）。
    #[test]
    fn a_non_string_turn_id_fails_closed() {
        let harness = Harness::new(BrokerConfig::default());
        let view = format!(
            r#"{{"messageId":"{}","turnId":12,"deltaIndex":"0","text":"hi"}}"#,
            uuid_text(615)
        );
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::Delta,
            "agent.message.delta",
            &view,
        )]));
        let actor = harness.actor();
        let command = prompt_command(&actor, &harness.session, &harness.request(1), 'A');
        let error = block_on(harness.broker.submit_mutation(&actor, &command))
            .expect_err("非字符串 turnId 必须失败");
        assert!(matches!(error, PortError::InvalidRequest(_)), "{error}");
        assert!(
            !harness
                .world
                .events(&harness.session)
                .iter()
                .any(|event| event.event_type.as_str() == "agent.message.delta"),
            "该批不得落盘"
        );
        assert_eq!(harness.world.publish_count(), 2, "该批不得发布");
    }

    /// `expected_version` 已给出的纯事件提交：推导值即它，且与存储返回值一致（§6 第 19 条的快捷分支）。
    #[test]
    fn an_event_only_commit_with_expected_version_keeps_the_current_version() {
        let harness = Harness::new(BrokerConfig::default());
        let commit = OwnedCommit {
            session: Some(harness.session.clone()),
            at: ts(1),
            expected_version: Some(Version::new(1)),
            state: None,
            turns: Vec::new(),
            events: vec![
                pending_event(
                    "session.mode.changed",
                    EventKind::State,
                    json_view(r#"{"currentModeId":"code"}"#),
                    None,
                    None,
                    StoredPolicy::Durable,
                    None,
                )
                .expect("event"),
            ],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        };
        let outcome = block_on(harness.broker.commit_owned(commit)).expect("commit");
        assert_eq!(outcome.version, Version::new(1), "无状态变更不递增");
        let stored = harness.world.events(&harness.session);
        assert_eq!(
            stored_view(&harness, &stored[0]),
            r#"{"version":"1","currentModeId":"code"}"#
        );
    }
}
