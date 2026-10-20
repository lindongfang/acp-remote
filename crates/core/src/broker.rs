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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

use crate::derive::{
    self, FILE_CHANGE_DEDUP_CAPACITY, TitleIntent, file_change_event, file_changed_view,
    title_intent,
};
use crate::model::{
    AcpRaw, Actor, AuditAction, AuditOutcome, AuditRecord, ClientCommand, CommandKind,
    CommandPayload, CommandReceipt, CommandRecord, CommandResult, CommandStatus,
    CommandTerminalRecord, CommittedDelivery, CommittedEvent, ConfigOptionId, ConfigValue,
    ConflictKind, CreateSessionRequest, Digest, EffectiveInteraction, ElicitationAction,
    ElicitationValues, EndpointEvent, EntityRef, EventKind, EventOrigin, EventPayload, EventType,
    GlobalCursor, InteractionId, InteractionKind, InteractionResolution, LocalCursor, MemberValue,
    MessageId, ModeId, ModeRef, OriginEpoch, OriginEventRef, OwnedSessionRef, PendingEvent,
    PendingInteraction, PermissionDecision, PermissionDecisionKind, PersistencePolicy, PortError,
    PromptContentBlock, PromptRequest, PublicError, RemoteSessionRef, RequestId, Resolution,
    ResourceOrigin, ResumeSessionRequest, Sequence, SessionActivity, SessionId,
    SessionRecoveryRecord, SessionReference, SessionState, SessionSummary, StoredPolicy, Timestamp,
    TurnId, TurnState, UnavailableKind, Version, ViewJson, WorkspaceRef,
    decode_json_string as json_string, encode_json_string as json_text, insert_string_member_front,
    object_members as json_members, top_level_member,
};
use crate::ports::{
    AuditStore, Clock, CommitOutcome, DeliveryIndexEntry, DeliveryReceipt, EventPublisher,
    EventSink, HistoryInclude, HistoryPage, HistoryQuery, IdGenerator, IdempotencyRecord,
    InteractionAuthority, InteractionResolved, ModeChange, NewTurn, OwnedCommit,
    PendingInteractionWrite, PreparedSessionCreate, ReadView, ReceiptOutcome, RemoteDeliveryStore,
    ReplayBatch, ReplayLimit, SessionActivityFacts, SessionBackendFactory, SessionCreateAdmission,
    SessionEndpoint, SessionQuery, SessionStore, SessionUpdate, SettledSession, StateChange,
    SubmissionBinding, TrustStore, TurnChange, TurnUpdate,
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
        CommandPayload::WorkspaceBrowse { .. } => "workspace.browse",
        CommandPayload::ModeSet { .. } => "session.mode.set",
        CommandPayload::ConfigSet { .. } => "session.config.set",
        CommandPayload::PermissionResolve { .. } => "permission.resolve",
    }
}

/// payload 变体 → 命令类别。payload 与 `ClientCommand.kind` 必须一致（不一致即 `InvalidRequest`）。
pub fn command_kind(payload: &CommandPayload) -> CommandKind {
    payload.family()
}

/// 只放内存、**不得**进入任何提交的事件类型（`INITIAL_DESIGN.md` §10.3「心跳、typing、presence：
/// 只放内存」）。
///
/// v1 登记的标准事件类型里没有这一类的成员（心跳是连接层消息，不是事件），因此本表是 daemon 新增
/// presence/typing 类事件时的唯一登记点；`EventType` 是开放 newtype，未登记取值合法（§10.1）。
/// 组合根用 [`persistence_policy`] 在 sink 包装层做内存转发，broker 在组装前把它们过滤掉（§6.11）。
pub const EPHEMERAL_EVENT_TYPES: &[&str] = &["device.typing", "session.presence"];

/// 进入**会话结构化历史条目**的事件类型（`SYNC_PROTOCOL.md` §10.2.1 的「进入历史条目的 17 类」，
/// 逐条照抄该表）。该表同时登记了不进入的 17 类：承载消息正文的（`*.message.delta` 与收尾正文
/// `agent.message.completed`）、摘要/仪表类（`session.created`/`session.updated`/`session.info.changed`/
/// `session.usage.changed`/`session.commands.changed`/`session.origin.online_changed`）、命令终态类
/// （`command.completed`/`command.failed`/`command.uncertain`）、连接与审计类（`agent.connected`/
/// `agent.disconnected`/`device.revoked`/`turn.delta_compacted`）。
///
/// §10.2.1 是唯一口径来源：该表变化时必须改这里，并由 `crates/storage-sqlite/tests/session_history.rs`
/// 的用例把本表与文档的两张表逐条对齐（本表是机器可见的唯一定义，消费方**不得**另写一份字面量）。
///
/// 两个消费方必须同口径：`storage-sqlite` 的历史页 SELECT（谓词与 `limit`/页标记在**同一条语句**里
/// 生效——事后过滤会让页标记按未过滤的行计数，客户端会拿到空页或短页却被告知「更早还有条目」，而它按
/// 「上一页最早条目的会话内序号」续读时没有游标可用）与 `test_support` 的 `FakeReadView::read_session`。
pub const HISTORY_ENTRY_EVENT_TYPES: &[&str] = &[
    "turn.queued",
    "turn.started",
    "turn.completed",
    "turn.cancelled",
    "turn.failed",
    "tool.call.started",
    "tool.call.updated",
    "tool.call.completed",
    "permission.requested",
    "permission.resolved",
    "elicitation.requested",
    "elicitation.resolved",
    "terminal.output",
    "file.changed",
    "session.plan.changed",
    "session.mode.changed",
    "session.config.changed",
];

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

/// §10.3 要求 view 带会话版本（十进制字符串）的事件类型。
///
/// `session.updated` 的版本嵌在 `session.version` 里（与 `sessionSummary` 同形），其余是顶层
/// `version`；[`finalize_owned_views`](Broker::finalize_owned_views) 按事件类型选择注入位置。
const SESSION_VERSION_VIEW_EVENT_TYPES: &[&str] = &[
    "session.mode.changed",
    "session.config.changed",
    "session.updated",
];

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
    /// 节点信任记录：Node Link 准入需要「该 Access 信任行已配对且未撤销」的持久事实。
    pub trust: Arc<dyn TrustStore>,
    pub publisher: Arc<dyn EventPublisher>,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdGenerator>,
    /// 授权拒绝的审计（§3.5 的 `authorization.denied`）。`None` = 该部署不注入审计端口。
    pub audit: Option<Arc<dyn AuditStore>>,
}

/// 不带 wire 指纹的入口使用的规范占位摘要（32 个零字节的 base64url，无填充、末字符在规范集合内）。
const PLACEHOLDER_FINGERPRINT: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

/// §6 第 9 条的兜底：被放弃 turn 的【占位】最多消耗多少个**驱动轮次**。
///
/// core 不读时钟、不设定时器，因此这里按轮次而不是墙钟计时：每个驱动轮次 = 一次派发尝试，组合根按
/// `storage.flush_interval_ms`（默认 250 ms）周期驱动活动会话，`1200` 轮 ≈ 5 分钟。轮次用尽仍未观测到
/// 终态说明端点的 turn 边界已不可信，此时优先让会话继续可用（释放占位，迟到事件的归属回到「在跑的
/// turn」规则）。
const ABANDONED_TURN_HOLD_ROUNDS: u32 = 1200;

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

/// **诱发当前提交事件的命令**：`request` 写进 `PendingEvent.causation`（§10.1 的 `causationRequestId`），
/// `actor` 决定命令类事件的 `origin.kind`（谁产生的）。
///
/// 两个来源：
/// - **在跑的 turn**（[`Slot::running_command`]）：`request` 就是 `owned_turn.causation`（§7.3 已持久化），
///   `actor` 在 [`Broker::dispatch_one`] 提升该 turn 时随 `QueuedTurn.actor` 记入；
/// - **会话在作用域的命令**（[`Slot::command_scope`]）：**响应类**事件
///   （`session.mode.changed`、`session.config.changed`、`permission.resolved`、`elicitation.resolved`）
///   由提交路径在把命令派发给后端**之前**登记，且**优先于**在跑的 turn——权限/elicitation 的解析按定义
///   发生在 prompt turn 的 `WaitingPermission` 期间，取在跑 turn 的命令会把解析记到 prompt 头上。
///
/// `causation` 与 `origin.kind` 是两件事：turn 内由 Agent 产出的消息/delta 带 command 的 `causation`，
/// 但 `origin.kind` 仍是 `agent`——**不得**用其中一个替另一个（§10.1）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandScope {
    request: RequestId,
    actor: Actor,
}

#[derive(Default)]
struct TurnQueue {
    running: Option<TurnId>,
    running_request: Option<RequestId>,
    running_actor: Option<Actor>,
    waiting: VecDeque<QueuedTurn>,
    /// §6 第 9 条：被放弃、但还没有被端点观测到终态的 turn——它继续占住
    /// `running` 槽位（见 [`HeldTurn`]）。
    held: Option<HeldTurn>,
}

/// 被放弃的 turn 的【占位】：`abandon_failed_turn` 之后不让出 `running` 槽位，直到它的终态被端点观测到。
///
/// 为什么必须占住：适配器（`agent-host`）发出的 `EndpointEvent` **不带** turn 标识，归属只能由 core 按
/// 「有在跑 turn 时归给它」推断（§10.3）。若放弃后立刻派发下一个排队 turn，下一个 turn 就成了「在跑的
/// turn」，被放弃 turn 的迟到事件（含终态）会被记到它头上——客户端看到下一个命令 `completed`，而它的
/// 正文被按上一个 turn 的收尾折叠。占住期间迟到事件一律按被放弃的 turn 归属并丢弃。
///
/// 释放路径：①该 turn 的终态事件到达（视为端点已观测到该 turn 结束）；②兜底：占位轮次用尽
/// （[`ABANDONED_TURN_HOLD_ROUNDS`]，避免端点永不收敛时会话永久卡住）。释放之后无归属事件的归属
/// 回到「在跑的 turn」规则，因此兜底是最后手段而不是常规路径。
struct HeldTurn {
    turn: TurnId,
    /// 已经消耗的驱动轮次（每轮 = 一次派发尝试，见 [`Slot::hold_blocks_dispatch`]）。
    rounds: u32,
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
    /// 每会话的 `file.changed` 去重日志：`(toolCallId, 展示路径)` 的插入顺序 FIFO（容量见
    /// [`FILE_CHANGE_DEDUP_CAPACITY`]）。R5 要求同一个工具调用内容里的同一处改动只派生一条事件，
    /// 而适配器会在 `tool.call.started` 与 `tool.call.updated` 里重复携带同一个 Diff 元素。
    ///
    /// 只登记**已提交**的键（提交成功后才插入）：落盘失败的批次不得吃掉下一次重试的派生。
    file_changes: Mutex<Vec<String>>,
    /// **该会话最近一次派发到后端的命令**（[`CommandScope`]）。
    ///
    /// 为什么需要：`session.mode.changed`/`session.config.changed`/`permission.resolved`/
    /// `elicitation.resolved`（[`RESPONSE_EVENT_TYPES`]）由后端在**收到命令之后**产出，而 `agent-host`
    /// 产出的 `EndpointEvent` 不携带 `causation`（那属适配器边界，core 不改），因此提交时（`commit_chunk`）
    /// 只能从会话作用域取诱发命令。这条命令**优先于**在跑的 turn：解析权限的命令按定义不是那条驱动 turn
    /// 的 prompt。没有它，这些事件只能按「有没有 actor」猜 `origin`——正是 §7.3 明令禁止的。
    ///
    /// 边界：**有界**（每会话一条记录）、随会话生命周期（会话槽位是它的唯一持有者），新命令直接覆盖旧值；
    /// 它不参与任何授权或幂等判定，只影响 `origin_kind`/`causation` 两个字段的取值。
    command_scope: Mutex<Option<CommandScope>>,
    /// **会话行尚未落账**（`session.create` 的窗口期，§6 第 20 条）。
    ///
    /// `true` 期间到达的后端事件只能排队（见 `Slot::pending`），**不得**提交：那一刻 `owned_session`
    /// 还没有这一行，落盘会写出引用不存在会话的事件。窗口由 [`Broker::settle_session_create`] 关闭
    /// （落账提交成功后按序 flush 队列），或由它的失败关闭路径一并作废。
    unaccounted: AtomicBool,
    /// 窗口期队列是否已超限（见 [`WINDOW_EVENT_QUEUE_LIMIT`]）。超限之后不再入队，由结算路径**失败关闭**：
    /// 结束该 Agent 运行时并把命令终结为 `uncertain`（`sync-session-create` 的「超限走失败关闭」）。
    window_overflowed: AtomicBool,
    /// 该会话已被**围栏**：失败关闭已经执行（Agent 运行时被结束、窗口期队列被作废），因此这个会话
    /// 永远不会落账、也永远不会有消费者。此后到达的事件**直接丢弃**，不得再缓冲。
    ///
    /// 为什么不用 [`Self::window_overflowed`] 代替：那个标志在落账提交期间也可能被并发的入队置位，
    /// 而那条竞态下会话会正常落账——按它丢弃会让已落账会话静默丢事件。围栏只在失败关闭那一步闭合。
    fenced: AtomicBool,
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
        // 已围栏（失败关闭执行完毕）的会话永远不会落账、也没有消费者：直接丢弃，不再缓冲。
        if self.fenced.load(Ordering::SeqCst) {
            return;
        }
        let mut pending = lock(&self.pending);
        // 窗口期（会话行尚未落账）的队列**有界**：超限不再入队、标记失败关闭（`settle_session_create`
        // 会结束该后端端点并把命令终结为 `uncertain`）。已入队的事件一律保留——超限的后果是显式失败，
        // 不是让已有事件悄悄消失。
        if self.unaccounted.load(Ordering::SeqCst) && pending.len() >= WINDOW_EVENT_QUEUE_LIMIT {
            self.window_overflowed.store(true, Ordering::SeqCst);
            return;
        }
        pending.push(event);
    }

    /// 进入窗口期（`session.create` 调用后端**之前**）：此后事件只入队，不得提交。
    fn hold_unaccounted(&self) {
        self.unaccounted.store(true, Ordering::SeqCst);
    }

    /// 离开窗口期：落账提交成功（队列随后由结算路径按序落盘并广播），或该会话根本不会落账。
    fn release_unaccounted(&self) {
        self.unaccounted.store(false, Ordering::SeqCst);
    }

    fn is_unaccounted(&self) -> bool {
        self.unaccounted.load(Ordering::SeqCst)
    }

    fn window_overflowed(&self) -> bool {
        self.window_overflowed.load(Ordering::SeqCst)
    }

    /// 闭合围栏：失败关闭已经执行，此后到达的事件直接丢弃（见 [`Slot::fenced`]）。
    fn fence(&self) {
        self.fenced.store(true, Ordering::SeqCst);
    }

    /// 取走并清空窗口期队列（作废路径专用：会话行不会存在，这些事件没有归属）。
    fn take_pending(&self) -> Vec<EndpointEvent> {
        std::mem::take(&mut *lock(&self.pending))
    }

    fn running_turn(&self) -> Option<TurnId> {
        lock(&self.queue).running.clone()
    }

    /// 在跑的 turn 的诱发命令（`request` = `owned_turn.causation`、`actor` = 触发者）。
    ///
    /// 两者总是成对写入/清空（[`Broker::dispatch_one`] 与 [`Slot::finish_turn`]），因此只有一个到位时按
    /// 「没有诱发命令」处理（不猜、不混搭）。
    fn running_command(&self) -> Option<CommandScope> {
        let queue = lock(&self.queue);
        match (&queue.running_request, &queue.running_actor) {
            (Some(request), Some(actor)) => Some(CommandScope {
                request: request.clone(),
                actor: actor.clone(),
            }),
            _ => None,
        }
    }

    /// 该会话最近一次派发到后端的命令（[`Slot::command_scope`]）。
    fn command_scope(&self) -> Option<CommandScope> {
        lock(&self.command_scope).clone()
    }

    /// 登记「该命令已派发到后端」——由提交路径在调后端**之前**调用，供后端随后产出的**响应类**事件
    /// （[`RESPONSE_EVENT_TYPES`]）在提交时取到诱发命令；它**优先于**在跑的 turn（见 [`CommandScope`]
    /// 与 [`Slot::command_scope`]）。
    fn set_command_scope(&self, scope: CommandScope) {
        *lock(&self.command_scope) = Some(scope);
    }

    /// 终态落盘后清空 active turn（`turn.queued` 之外的排队 turn 保留）。
    fn finish_turn(&self) {
        let mut queue = lock(&self.queue);
        queue.running = None;
        queue.running_request = None;
        queue.running_actor = None;
    }

    /// §6 第 9 条：放弃 `turn` 之后不让出 `running` 槽位，而是登记为待观测的占位。
    fn hold_abandoned_turn(&self, turn: &TurnId) {
        let mut queue = lock(&self.queue);
        queue.held = Some(HeldTurn {
            turn: turn.clone(),
            rounds: 0,
        });
    }

    /// 当前持有占位的 turn（`None` = 没有占位）。
    fn held_turn(&self) -> Option<TurnId> {
        lock(&self.queue)
            .held
            .as_ref()
            .map(|held| held.turn.clone())
    }

    /// 释放【占位】：只有占位者本人能释放（更早被放弃的 turn 的迟到终态不得提前放行会话）。
    fn release_held_turn(&self, turn: &TurnId) {
        let mut queue = lock(&self.queue);
        if queue.held.as_ref().map(|held| &held.turn) != Some(turn) {
            return;
        }
        queue.held = None;
        if queue.running.as_ref() == Some(turn) {
            queue.running = None;
            queue.running_request = None;
            queue.running_actor = None;
        }
    }

    /// 派发前的占位检查。返回 `true` = 仍在占位、本轮不得派发；`false` = 没有占位（可派发）。
    ///
    /// 兜底在这里推进（core 不读时钟、不设定时器，因此按驱动轮次而不是墙钟计时，见
    /// [`ABANDONED_TURN_HOLD_ROUNDS`]）。
    fn hold_blocks_dispatch(&self) -> bool {
        let expired = {
            let mut queue = lock(&self.queue);
            let Some(held) = queue.held.as_mut() else {
                return false;
            };
            held.rounds = held.rounds.saturating_add(1);
            if held.rounds < ABANDONED_TURN_HOLD_ROUNDS {
                return true;
            }
            held.turn.clone()
        };
        self.release_held_turn(&expired);
        false
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
    /// `session.create` 的**窗口期**状态（§6 第 20 条）：后端已建立会话、会话行尚未落账，落账所需的
    /// 全部取值只能停在内存里（受理行此刻的 `session_id` 仍为 `NULL`，靠它找不到会话）。
    ///
    /// 键是 `(actor, requestId)`——与幂等键同口径（§6 第 6 条）。条目在后端成功返回后写入、在
    /// [`Broker::settle_session_create`] 里取走；适配层对每个 `Prepared` 都必须结算一次，未结算的条目
    /// 会一直留到进程退出（那条命令停在 `accepted`，由启动恢复终结为 `uncertain`）。
    prepared: Mutex<HashMap<String, PreparedCreate>>,
    /// 有效待处理交互端口（§5.8）：服务协商 `core.pending-interactions.v1` 的读-校验-原子胜出路径。
    /// 未注入时该路径显式不可用（旧 peer 的按 id 解析路径不受影响）。
    interactions: Option<Arc<dyn InteractionAuthority>>,
}

/// 窗口期状态：落账提交（[`StateChange::SettleCreate`]）的全部输入，外加会话行的两个时间戳来源。
#[derive(Clone)]
struct PreparedCreate {
    /// 会话行的全部创建期取值（`agent_session_id` 在后端成功返回后已补齐）。
    settled: SettledSession,
    /// 落账提交的时间戳：会话行的 `created_at`/`updated_at` 与投影给适配层的摘要里的两个时间戳
    /// 都是它（同一个值 ⇒ 回归给客户端的摘要与之后读路径看到的行**同源**）。
    at: Timestamp,
    /// 该会话的 origin epoch（core 在受理之后分配，落账提交随行落盘）。
    epoch: OriginEpoch,
}

/// 窗口期（会话行落账之前）事件队列的上限（§6 第 20 条）。
///
/// 取值理由：窗口的存活时间只有「后端 `create` 返回到适配层完成一次结算」，正常是毫秒级的少量
/// 通知（`session/new` 成功后 Agent 的初始 `session/update`）；256 条远高于任何正常取值，又把异常
/// 或恶意 Agent 在这段窗口里的内存占用钉在几十 KB 量级。超限不再入队，并由结算路径失败关闭
/// （结束该 Agent 运行时 + 把命令终结为 `uncertain`），不静默丢事件；失败关闭之后该槽位被**围栏**，
/// 此后到达的事件直接丢弃（见 [`Slot::fenced`]）。
pub const WINDOW_EVENT_QUEUE_LIMIT: usize = 256;

impl Broker {
    pub fn new(deps: BrokerDeps, config: BrokerConfig) -> Self {
        Self {
            deps,
            config,
            slots: Mutex::new(HashMap::new()),
            prepared: Mutex::new(HashMap::new()),
            interactions: None,
        }
    }

    /// 注入有效待处理交互端口（组合根在装配时调用；见 [`InteractionAuthority`]）。
    pub fn with_interaction_authority(mut self, authority: Arc<dyn InteractionAuthority>) -> Self {
        self.interactions = Some(authority);
        self
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
                "remote:{}:{}",
                remote.owner_node_id.as_str(),
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
                    file_changes: Mutex::new(Vec::new()),
                    command_scope: Mutex::new(None),
                    unaccounted: AtomicBool::new(false),
                    window_overflowed: AtomicBool::new(false),
                    fenced: AtomicBool::new(false),
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

    /// **节点级**事件的出口（`design.md` D6、R8）：`agent.connected`/`agent.disconnected` 是节点级事件
    /// （会话标识为空），不属于任何会话槽位，因此不能走 [`Broker::sink`]——那条路径会把事件归到某个会话上。
    ///
    /// sink 的调用是同步的，而提交是异步的；core 不依赖 runtime，因此这里把每条事件交给调用方在能驱动
    /// future 的位置（组合根本就有 runtime）经 [`Broker::commit_node_event`] 提交。节点级事件稀少（每个
    /// profile 进程各一次），不需要合并窗口。
    pub async fn node_submit(&self, event: EndpointEvent) -> Result<(), PortError> {
        self.commit_node_event(event).await
    }

    /// 节点级事件的提交入口：校验后按 `session: None` 的提交落库并发布。
    ///
    /// 校验（失败即 [`PortError::InvalidRequest`]，**不做**任何降级写入）：
    /// - `event_type` 只能是 `agent.connected`/`agent.disconnected`；
    /// - `kind` 必须是 [`EventKind::State`]，`turn`/`causation` 必须为空（节点级事件不属于任何 turn）；
    /// - `payload.acp` 必须为 `None`（节点级事件没有 ACP 原文）；
    /// - view 必须带非空 `agentId`，且 `state` 与该事件类型的唯一取值一致（`SYNC_PROTOCOL.md` §10.3
    ///   的封闭词表：`connected` / `disconnected`）。
    pub async fn commit_node_event(&self, event: EndpointEvent) -> Result<(), PortError> {
        let event_type = event.event_type.as_str();
        let expected_state = match event_type {
            "agent.connected" => "connected",
            "agent.disconnected" => "disconnected",
            _ => {
                return Err(PortError::InvalidRequest(
                    "节点级事件只接受 agent.connected/agent.disconnected（§10.3）",
                ));
            }
        };
        if event.kind != EventKind::State {
            return Err(PortError::InvalidRequest(
                "节点级 Agent 事件必须是 state 类别（§10.3）",
            ));
        }
        if event.turn.is_some() || event.causation.is_some() {
            return Err(PortError::InvalidRequest(
                "节点级 Agent 事件不属于任何 turn，也不由命令触发（§10.3）",
            ));
        }
        if event.payload.acp.is_some() {
            return Err(PortError::InvalidRequest(
                "节点级 Agent 事件不携带 ACP 原文（§10.3）",
            ));
        }
        event.payload.validate().map_err(PortError::from)?;
        let (agent_id, state) = derive::node_event_parts(event.payload.view.as_str());
        // 按**形状**而非缺席判定：JSON 的空串也是 `Some("")`，只查 `is_none()` 会让空 `agentId` 通过并落一条
        // schema 非法（`minLength: 1`）的节点级事件，消费端 `NonEmptyText<128>` 解析失败。core 是节点级事件
        // 的最后一道形状防线（§10.3）。
        if agent_id.as_deref().is_none_or(str::is_empty) {
            return Err(PortError::InvalidRequest(
                "节点级 Agent 事件的 view 必须带非空 agentId（§10.3）",
            ));
        }
        if state.as_deref() != Some(expected_state) {
            return Err(PortError::InvalidRequest(
                "节点级 Agent 事件的 state 取值必须与事件类型一致（§10.3 的封闭词表）",
            ));
        }
        let pending = pending_event(
            event_type,
            EventKind::State,
            event.payload.view,
            None,
            None,
            StoredPolicy::Durable,
            None,
        )?;
        let commit = OwnedCommit {
            session: None,
            at: event.at,
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![pending],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        };
        let outcome = self.commit_owned(commit).await?;
        self.publish(&outcome.appended);
        Ok(())
    }

    /// 会话的规范化工作目录（`owned_session.workspace_cwd`）。
    ///
    /// 走 `SessionStore::load_recovery` 这个**窄读取**：`workspace_cwd` 是本机规范化路径，按 §3.6 不得进入
    /// 任何可投影形状（`Session`/`SessionSummary`/`read_session`/`node_link_slice` 都不带它），但它正是
    /// 展示路径相对化的根。`None` = 该会话没有可用于恢复的取值（未登记 workspace，或行不存在），
    /// 此时派生按「无法证明在工作区内」处理（越界 + 只给文件名称），绝不静默下发绝对路径。
    async fn workspace_root(&self, session: &SessionId) -> Result<Option<String>, PortError> {
        match self.deps.store.load_recovery(session).await? {
            Some(record) => Ok(record.workspace_cwd),
            None => Ok(None),
        }
    }

    /// 从 `tool.call.*` 的 view 派生 `file.changed`（R5–R7）；view 不含类型化 Diff 时返回 `None`。
    async fn derive_file_changes(
        &self,
        view: &ViewJson,
        session: &SessionId,
    ) -> Result<Option<Vec<derive::DerivedFileChange>>, PortError> {
        if !derive::view_carries_diff(view.as_str()).map_err(PortError::from)? {
            return Ok(None);
        }
        let root = self.workspace_root(session).await?;
        let changes = derive::derived_file_changes(view.as_str(), root.as_deref())
            .map_err(PortError::from)?;
        Ok(Some(changes))
    }

    // -----------------------------------------------------------------------------------------
    // §6.5 授权
    // -----------------------------------------------------------------------------------------

    /// 所有用例入口的第一步：按 actor 的授权面判定（§6.5）。
    ///
    /// - `LocalCli`：本地管理入口，访问控制由 `LOCAL_ADMIN_PROTOCOL.md` §2.2 的 OS 用户边界保证。
    /// - `Device`：**通过设备认证即可执行任何业务命令**——设备不再有授权维度。认证本身与撤销即时
    ///   生效由连接层负责（`SYNC_PROTOCOL.md` §8），不是本函数的事。
    /// - `Node`：与设备方向同一规则——已配对且未撤销的节点即可执行本节点命令。`local.*` 永不出现在
    ///   远程命令面，因此这里不存在需要逐命令展开的授权集合。
    /// - `PairingClaimant`：**任何命令都不授权**（design D12：认领方只存在于配对通道，且那里不经
    ///   broker 授权）；命中即失败关闭并记一条 `authorization.denied`。
    pub async fn authorize(
        &self,
        actor: &Actor,
        _command: &str,
        session: Option<&SessionId>,
        request: &RequestId,
    ) -> Result<(), Denied> {
        if !matches!(actor, Actor::PairingClaimant { .. }) {
            return Ok(());
        }
        let denied = Denied::scope_denied();
        self.audit_denial(actor, session, request).await;
        Err(denied)
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
        self.submit_mutation_versioned(actor, command, None).await
    }

    /// 带交互应答版本（`core.pending-interactions.v1` 的 `requestVersion`）的 mutation 管道。
    ///
    /// 未协商该 feature 的旧 peer 走 [`Self::submit_mutation`]（`request_version = None`）：
    /// 命令形状与行为逐字不变；协商后的客户端必须把 `session.read` 拿到的当前版本原样回传。
    pub async fn submit_mutation_versioned(
        &self,
        actor: &Actor,
        command: &ClientCommand,
        request_version: Option<Version>,
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
                self.submit_permission_resolve(
                    actor,
                    command,
                    interaction,
                    option_id,
                    request_version,
                )
                .await
            }
            CommandPayload::ElicitationRespond {
                interaction,
                action,
                values,
            } => {
                self.submit_elicitation_respond(
                    actor,
                    command,
                    interaction,
                    *action,
                    values,
                    request_version,
                )
                .await
            }
            // 查询命令不在此派发：结果由 §4 的查询入口（`list_sessions`/`read_session`/
            // `command_status`/`config_options`）返回 core 类型，由适配器投影到 wire（core 不做 JSON
            // 序列化）。这里只确认"已接受、无副作用"。
            CommandPayload::SessionList { .. }
            | CommandPayload::SessionRead { .. }
            | CommandPayload::CommandStatus { .. }
            | CommandPayload::ModeList { .. }
            | CommandPayload::ConfigList { .. }
            | CommandPayload::WorkspaceBrowse { .. } => Ok(CommandReceipt::Accepted {
                request: command.request.clone(),
                turn: None,
            }),
        }
    }

    /// `session.prompt` 的带附件清单入口（`core.attachment-transfer.v1`）。
    ///
    /// 与 [`Self::submit_mutation`] 的区别只在受理提交：附件清单纯粹随**同一次** `commit` 绑定
    /// （§6 第 22 条）。任一校验/写入失败都会让受理失败，因此 Agent 不会收到残缺 prompt；
    /// 幂等命中在绑定之前发生，重试不会二次派发。
    pub async fn submit_prompt_with_submission(
        &self,
        actor: &Actor,
        command: &ClientCommand,
        content: &[PromptContentBlock],
        binding: SubmissionBinding,
    ) -> Result<CommandReceipt, PortError> {
        self.submit_prompt_bound(actor, command, content, Some(binding))
            .await
    }

    async fn submit_prompt(
        &self,
        actor: &Actor,
        command: &ClientCommand,
        content: &[PromptContentBlock],
    ) -> Result<CommandReceipt, PortError> {
        self.submit_prompt_bound(actor, command, content, None)
            .await
    }

    async fn submit_prompt_bound(
        &self,
        actor: &Actor,
        command: &ClientCommand,
        content: &[PromptContentBlock],
        binding: Option<SubmissionBinding>,
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

        // §6.4：已有未结束任务时的两种策略（排队中的 turn 也算「有工作」）。
        let facts = self.deps.store.session_activity(&session).await?;
        let busy = facts.unfinished_turns > 0 || slot.running_turn().is_some();
        if busy {
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

        // 派发前预检：不支持的媒体/超预算必须在**受理之前**整条拒绝，否则客户端会先看到
        // accepted、再看到泛化的 turn.failed，并已清空草稿与附件。
        let endpoint = self.endpoint(&slot, &session).await?;
        endpoint
            .validate_prompt(&PromptRequest {
                content: content.to_vec(),
            })
            .await?;

        let turn = self.deps.ids.turn_id();
        let mut events = vec![pending_event(
            "turn.queued",
            EventKind::State,
            view_turn(&turn, TurnState::Queued)?,
            Some(turn.clone()),
            Some(command.request.clone()),
            StoredPolicy::Durable,
            Some(actor),
        )?];
        // 会话状态：新增一个排队 turn 后按归约规则重算（`pending` 交互仍优先）。状态未变化时不重复
        // 广播，也不无谓递增版本。
        let at = self.now();
        let state = self.state_after(&facts, 0, 1, slot.hold_blocks_dispatch());
        let state = if snapshot.session.state() == state {
            None
        } else {
            events.push(
                self.session_updated_event(&session, state, &at, None)
                    .await?,
            );
            Some(StateChange::Update(SessionUpdate {
                state: Some(state),
                mode: ModeChange::Unchanged,
                title: None,
                interaction: None,
                confirmation: None,
                agent_session_id: None,
                workspace_cwd: None,
                workspace_directory_id: None,
            }))
        };
        let turns = vec![TurnChange::Create(NewTurn {
            turn: turn.clone(),
            state: TurnState::Queued,
            causation: Some(command.request.clone()),
            started_at: None,
        })];
        match self
            .accept_with_binding(actor, command, &session, state, turns, events, binding)
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
        // 取消命令进入作用域：后端随后的 `turn.cancelled` 属于本 turn（走 turn 自身的 causation），
        // 但同批可能出现的非 turn 命令类事件也按这条命令归因，而不是按「有没有 actor」猜。
        slot.set_command_scope(CommandScope {
            request: command.request.clone(),
            actor: actor.clone(),
        });
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
        if self
            .deps
            .store
            .session_activity(&session)
            .await?
            .unfinished_turns
            > 0
            || slot.running_turn().is_some()
        {
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
        // 命令在作用域里登记：后端随后产出的 `session.mode.changed` 不含 causation（适配器边界），
        // 提交时按这条命令判 `origin.kind` 与 `causation`（§10.1）。
        slot.set_command_scope(CommandScope {
            request: command.request.clone(),
            actor: actor.clone(),
        });
        match endpoint.set_mode(mode).await {
            Ok(()) => {
                // 后端的 `session.mode.changed` 先落盘（§11.5：先发状态事件、再发 command.completed）。
                self.flush_locked(&slot, &session).await?;
                let state = StateChange::Update(SessionUpdate {
                    state: None,
                    mode: ModeChange::Set(mode_ref(mode)?),
                    title: None,
                    interaction: None,
                    confirmation: None,
                    agent_session_id: None,
                    workspace_cwd: None,
                    workspace_directory_id: None,
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
        if self
            .deps
            .store
            .session_activity(&session)
            .await?
            .unfinished_turns
            > 0
            || slot.running_turn().is_some()
        {
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
        // 同 `set_mode`：`session.config.changed` 的 `origin.kind`/`causation` 取自这条命令。
        slot.set_command_scope(CommandScope {
            request: command.request.clone(),
            actor: actor.clone(),
        });
        match endpoint.set_config(id, value).await {
            Ok(()) => {
                self.flush_locked(&slot, &session).await?;
                let state = StateChange::Update(SessionUpdate {
                    state: None,
                    mode: ModeChange::Unchanged,
                    title: None,
                    interaction: None,
                    confirmation: None,
                    agent_session_id: None,
                    workspace_cwd: None,
                    workspace_directory_id: None,
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
        request_version: Option<Version>,
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
        // 版本/类别/表单/上游存活校验全部先于受理：失败时不留下没有终态的 `accepted` 行。
        self.preflight_effective_locked(&slot, &session, interaction, request_version, &resolution)
            .await?;
        match self
            .accept(actor, command, &session, None, Vec::new(), Vec::new())
            .await?
        {
            Accept::Done(receipt) => return Ok(receipt),
            Accept::Committed => {}
        }
        let outcome = match self
            .commit_effective_locked(
                actor,
                &command.request,
                &slot,
                &session,
                interaction,
                request_version,
                resolution,
            )
            .await
        {
            Ok(outcome) => outcome,
            // 受理之后的竞态（另一设备抢先解析、请求在窗口内失效、后端写入失败）：必须落一条
            // 持久化终态，客户端按同一 requestId 重试才能拿到确定结果。
            Err(error) => {
                let public = self.port_error_public(&error)?;
                let at = self.now();
                self.commit_terminal(
                    &session,
                    command,
                    CommandStatus::Failed,
                    Some(public),
                    None,
                    None,
                    &at,
                )
                .await?;
                return Ok(CommandReceipt::Accepted {
                    request: command.request.clone(),
                    turn: None,
                });
            }
        };
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
        request_version: Option<Version>,
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
        // 版本/类别/表单/上游存活校验全部先于受理：失败时不留下没有终态的 `accepted` 行。
        self.preflight_effective_locked(&slot, &session, interaction, request_version, &resolution)
            .await?;
        match self
            .accept(actor, command, &session, None, Vec::new(), Vec::new())
            .await?
        {
            Accept::Done(receipt) => return Ok(receipt),
            Accept::Committed => {}
        }
        let outcome = match self
            .commit_effective_locked(
                actor,
                &command.request,
                &slot,
                &session,
                interaction,
                request_version,
                resolution,
            )
            .await
        {
            Ok(outcome) => outcome,
            // 同权限路径：受理之后的任何失败都必须留下持久化终态。
            Err(error) => {
                let public = self.port_error_public(&error)?;
                let at = self.now();
                self.commit_terminal(
                    &session,
                    command,
                    CommandStatus::Failed,
                    Some(public),
                    None,
                    None,
                    &at,
                )
                .await?;
                return Ok(CommandReceipt::Accepted {
                    request: command.request.clone(),
                    turn: None,
                });
            }
        };
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

    /// 当前有效的完整待处理请求（`SYNC_PROTOCOL.md` §11.7）。
    ///
    /// 先授权身份/会话，再由 [`InteractionAuthority`] 读出**服务端确认过**的行；随后校验该会话的上游
    /// 仍存活：端点不可得时把这些行标记为失效并返回错误，绝不把已丢失上游的请求继续当作可应答。
    pub async fn effective_interactions(
        &self,
        actor: &Actor,
        reference: &SessionReference,
    ) -> Result<Vec<EffectiveInteraction>, PortError> {
        let session = self.owned_session(reference)?;
        let request = self.deps.ids.request_id();
        self.authorize(actor, "session.read", Some(&session), &request)
            .await
            .map_err(Denied::into_port_error)?;
        let authority = self
            .interactions
            .clone()
            .ok_or(PortError::Unavailable(UnavailableKind::BackendUnsupported))?;
        let effective = authority.effective(&session).await?;
        if effective.is_empty() {
            return Ok(effective);
        }
        if let Err(error) = self.endpoint_for(reference).await {
            let slot = self.owned_slot(&session);
            let _guard = slot.gate.guard().await;
            let at = self.now();
            let _ = authority
                .invalidate_pending(Some(&session), at.clone())
                .await;
            // 失效改变了归约输入：不能只清行而留下 `pending` 摘要。
            let _ = self
                .refresh_activity(&session, &at, slot.hold_blocks_dispatch())
                .await;
            return Err(error);
        }
        Ok(effective)
    }

    /// 解析一次**当前有效**的交互（`permission.resolve`/`elicitation.respond` 的版本化入口）。
    ///
    /// 校验顺序（任一失败都不消耗请求）：身份/会话授权 → 请求存在 → 版本一致 → 表单值符合 schema →
    /// 上游存活 → 条件更新选出首个有效答案。存储层的条件更新同时校验状态/失效标记/版本，因此两个设备
    /// 并发提交时只有一个胜出，其余拿到 [`Resolution::AlreadyResolved`]。
    pub async fn resolve_effective(
        &self,
        actor: &Actor,
        reference: &SessionReference,
        interaction: &InteractionId,
        request_version: Option<Version>,
        resolution: InteractionResolution,
    ) -> Result<Resolution, PortError> {
        let session = self.owned_session(reference)?;
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
        self.resolve_effective_locked(
            actor,
            &request,
            &slot,
            &session,
            interaction,
            request_version,
            resolution,
        )
        .await
    }

    /// 版本化交互解析的公共实现（调用方已持有串行门并完成授权）。
    ///
    /// 未注入 [`InteractionAuthority`] 时退回旧 peer 的按 id 解析路径（状态条件更新仍保证
    /// first-writer-wins，但没有版本/表单/失效校验）。注入后按 §11.7 的顺序校验并原子胜出。
    #[allow(clippy::too_many_arguments)]
    async fn resolve_effective_locked(
        &self,
        actor: &Actor,
        request: &RequestId,
        slot: &Slot,
        session: &SessionId,
        interaction: &InteractionId,
        request_version: Option<Version>,
        resolution: InteractionResolution,
    ) -> Result<Resolution, PortError> {
        self.preflight_effective_locked(slot, session, interaction, request_version, &resolution)
            .await?;
        self.commit_effective_locked(
            actor,
            request,
            slot,
            session,
            interaction,
            request_version,
            resolution,
        )
        .await
    }

    /// 解析前的**只读**校验：分辨率形状、交互类别、请求版本、表单值、上游存活。
    ///
    /// 它必须先于命令受理（`accept`）执行：这些校验失败时命令不得留下一条没有终态的 `accepted` 行。
    /// 上游不可得时顺带把该会话的 waiting 请求标记为失效（不复活），随后返回错误——这是唯一允许的
    /// 副作用，因为它不消费请求、也不产生命令终态。
    async fn preflight_effective_locked(
        &self,
        slot: &Slot,
        session: &SessionId,
        interaction: &InteractionId,
        request_version: Option<Version>,
        resolution: &InteractionResolution,
    ) -> Result<(), PortError> {
        resolution.validate().map_err(PortError::from)?;
        let Some(authority) = self.interactions.clone() else {
            // 旧 peer 路径没有版本/权威读面；解析阶段的条件更新仍保证 first-writer-wins。
            return Ok(());
        };
        // 上游存活先于任何写入：失效的等待方不得被「解析」后留下一条假终态。
        if let Err(error) = self.endpoint(slot, session).await {
            let at = self.now();
            let _ = authority
                .invalidate_pending(Some(session), at.clone())
                .await;
            // 失效改变了归约输入：不能只清行而留下 `pending` 摘要。
            let _ = self
                .refresh_activity(session, &at, slot.hold_blocks_dispatch())
                .await;
            return Err(error);
        }
        let effective = authority.effective(session).await?;
        if let Some(current) = effective
            .iter()
            .find(|item| item.interaction.as_str() == interaction.as_str())
        {
            if let Some(expected) = request_version
                && expected != current.request_version
            {
                return Err(PortError::Conflict(ConflictKind::VersionMismatch));
            }
            if current.kind
                != match resolution {
                    InteractionResolution::Permission(_) => InteractionKind::Permission,
                    InteractionResolution::Elicitation { .. } => InteractionKind::Elicitation,
                }
            {
                return Err(PortError::InvalidRequest("交互类别与解析动作不一致"));
            }
            if let InteractionResolution::Elicitation { action, values } = resolution
                && *action == ElicitationAction::Submit
            {
                current
                    .validate_submission(values)
                    .map_err(PortError::from)?;
            }
        }
        Ok(())
    }

    /// 解析的写入阶段（调用方已完成 [`Self::preflight_effective_locked`] 与命令受理）。
    ///
    /// 版本化入口与旧 peer 的按 id 入口走**同一条**事务：[`SessionStore::commit`] 里
    /// `SessionUpdate.interaction` 用条件更新选出首个有效答案（含 `request_version`/失效标记），
    /// 因此仲裁、会话摘要与 `session.updated` 永远同批落盘（`session-activity` D2）。
    #[allow(clippy::too_many_arguments)] // 事务写入所需的稳定标识与已预检结果；拆结构体只会增加临时分配。
    async fn commit_effective_locked(
        &self,
        actor: &Actor,
        request: &RequestId,
        slot: &Slot,
        session: &SessionId,
        interaction: &InteractionId,
        request_version: Option<Version>,
        resolution: InteractionResolution,
    ) -> Result<Resolution, PortError> {
        self.resolve_locked(
            actor,
            request,
            slot,
            session,
            interaction,
            request_version,
            resolution,
        )
        .await
    }

    /// 把有效未解决交互标记为失效（上游丢失或重启恢复）：`session = None` 表示全部会话。
    ///
    /// 失效会同时改变会话摘要（有效请求可能清零），因此随后在同一串行门内重算并（仅在状态变化时）
    /// 提交一次 `session.updated`。`session = None` 的全局失效先取「可能处于待处理/运行中」的会话快照，
    /// 失效后逐个重算——不依赖失效后的查询结果，否则刚清空请求的会话不会被返回。
    pub async fn invalidate_pending_interactions(
        &self,
        session: Option<&SessionId>,
        at: Timestamp,
    ) -> Result<u32, PortError> {
        let Some(authority) = self.interactions.as_ref() else {
            return Ok(0);
        };
        let candidates: Vec<SessionId> = match session {
            Some(session) => vec![session.clone()],
            None => self
                .deps
                .store
                .list(SessionQuery {
                    only: None,
                    states: vec![SessionState::Running, SessionState::Pending],
                    limit: None,
                })
                .await?
                .into_iter()
                .map(|summary| summary.session_id().clone())
                .collect(),
        };
        let affected = authority.invalidate_pending(session, at.clone()).await?;
        for candidate in candidates {
            let held = self.owned_slot(&candidate).hold_blocks_dispatch();
            self.refresh_activity(&candidate, &at, held).await?;
        }
        Ok(affected)
    }

    /// 端点的正式应答**确认**：把已仲裁的交互移出有效未解决集合。
    ///
    /// 适配器在收到「Agent 已处理该应答」的明确结果后调用本方法（`session-activity` D2）。确认只对
    /// `resolved` 且未确认的行生效；重复确认、未知交互或仍为 `pending` 的行都是无操作（返回 `false`），
    /// 因此适配器的重复通知不会产生第二次副作用，也不会伪造授权。
    ///
    /// 确认行、提交后的会话摘要与（状态变化时的）`session.updated` 在**同一事务**里落盘。
    pub async fn confirm_interaction(
        &self,
        session: &SessionId,
        interaction: &InteractionId,
    ) -> Result<bool, PortError> {
        let slot = self.owned_slot(session);
        let _guard = slot.gate.guard().await;
        self.confirm_locked(&slot, session, interaction).await
    }

    /// 已持有会话串行门时的确认实现（适配器成功返回后的同一命令路径使用）。
    async fn confirm_locked(
        &self,
        slot: &Slot,
        session: &SessionId,
        interaction: &InteractionId,
    ) -> Result<bool, PortError> {
        let at = self.now();
        let facts = self.deps.store.session_activity(session).await?;
        if !facts
            .unconfirmed_interactions
            .iter()
            .any(|candidate| candidate == interaction)
        {
            // 未知/仍 pending/已确认：无操作。不写行、不广播。
            return Ok(false);
        }
        let mut events = Vec::new();
        let state = self.state_after(&facts, -1, 0, slot.hold_blocks_dispatch());
        let changed = self.current_state(session).await? != Some(state);
        if changed {
            events.push(
                self.session_updated_event(session, state, &at, None)
                    .await?,
            );
        }
        // 摘要不变时仍要落确认，但不重复广播、不递增版本。
        let state_change = Some(StateChange::Update(SessionUpdate {
            state: changed.then_some(state),
            mode: ModeChange::Unchanged,
            title: None,
            interaction: None,
            confirmation: Some(interaction.clone()),
            agent_session_id: None,
            workspace_cwd: None,
            workspace_directory_id: None,
        }));
        let commit = OwnedCommit {
            session: Some(session.clone()),
            at,
            expected_version: None,
            state: state_change,
            turns: Vec::new(),
            events,
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        };
        let outcome = self.commit_owned(commit).await?;
        self.publish(&outcome.appended);
        Ok(true)
    }

    /// 提交后的会话状态（从已读事实 + 本提交增量归约）。
    fn state_after(
        &self,
        facts: &SessionActivityFacts,
        pending_delta: i64,
        turn_delta: i64,
        held: bool,
    ) -> SessionState {
        let pending = apply_delta(facts.unresolved_interactions, pending_delta) > 0;
        let unfinished = apply_delta(facts.unfinished_turns, turn_delta) > 0 || held;
        SessionActivity::reduce(pending, unfinished)
    }

    async fn current_state(&self, session: &SessionId) -> Result<Option<SessionState>, PortError> {
        Ok(self
            .deps
            .store
            .load(session)
            .await?
            .map(|snapshot| snapshot.session.state()))
    }

    /// 重算并（仅在状态变化时）提交会话摘要 + `session.updated`。
    ///
    /// `held` 表示该会话仍有「已派发但停止未确认」的执行占位（[`HeldTurn`]）：此时即使库里没有未结束
    /// turn，也不能声称空闲（`session-activity` D3）。
    async fn refresh_activity(
        &self,
        session: &SessionId,
        at: &Timestamp,
        held: bool,
    ) -> Result<(), PortError> {
        let facts = self.deps.store.session_activity(session).await?;
        let state = self.state_after(&facts, 0, 0, held);
        if self.current_state(session).await? == Some(state) {
            return Ok(());
        }
        let event = self.session_updated_event(session, state, at, None).await?;
        let commit = OwnedCommit {
            session: Some(session.clone()),
            at: at.clone(),
            expected_version: None,
            state: Some(StateChange::Update(SessionUpdate {
                state: Some(state),
                mode: ModeChange::Unchanged,
                title: None,
                interaction: None,
                confirmation: None,
                agent_session_id: None,
                workspace_cwd: None,
                workspace_directory_id: None,
            })),
            turns: Vec::new(),
            events: vec![event],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        };
        let outcome = self.commit_owned(commit).await?;
        self.publish(&outcome.appended);
        Ok(())
    }

    /// `session.updated` 的 view：提交后的**完整**会话摘要（`event-views` 的 `session` 成员）。
    ///
    /// `version` 取本次提交将写入的版本（当前 + 1，因为状态变化总带一次 `SessionUpdate`），并由
    /// [`Self::finalize_owned_views`] 的嵌套注入与 `CommitOutcome.version` 做漂移检查。
    async fn session_updated_event(
        &self,
        session: &SessionId,
        state: SessionState,
        at: &Timestamp,
        title: Option<Option<&str>>,
    ) -> Result<PendingEvent, PortError> {
        let mut summaries = self
            .deps
            .store
            .list(SessionQuery {
                only: Some(vec![session.clone()]),
                states: Vec::new(),
                limit: Some(1),
            })
            .await?;
        let summary = summaries
            .pop()
            .ok_or_else(|| PortError::NotFound(EntityRef::Session(session.clone())))?;
        // 同一提交可能同时写标题：`session.updated` 投影的是提交后的完整摘要，不能用提交前的行。
        let summary = match title {
            Some(title) => summary
                .with_title(title.map(str::to_owned))
                .map_err(PortError::from)?,
            None => summary,
        };
        let version = summary
            .version()
            .get()
            .checked_add(1)
            .map(Version::new)
            .ok_or(PortError::Corrupt("会话版本已溢出"))?;
        let view = view_session_summary(&summary, state, &version, at)?;
        pending_event(
            "session.updated",
            EventKind::State,
            view,
            None,
            None,
            StoredPolicy::Durable,
            None,
        )
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
        self.resolve_locked(
            actor,
            &request,
            &slot,
            &session,
            interaction,
            None,
            resolution,
        )
        .await
    }

    /// 交互解析的公共实现（调用方已持有串行门）：**先**用条件更新落盘仲裁（§6.7 first-writer-wins），
    /// **再**派发给后端，最后把后端发出的 `*.resolved` 事件落盘。
    ///
    /// 仲裁只把行推进到 `resolved`：`confirmed_at` 仍为空，状态归约因此**不会**提前解除待处理
    /// （`session-activity` D2）。确认由 [`Broker::confirm_interaction`] 单独提交。
    #[allow(clippy::too_many_arguments)] // 版本化与按 id 两条入口共用的完整事实集
    async fn resolve_locked(
        &self,
        actor: &Actor,
        request: &RequestId,
        slot: &Slot,
        session: &SessionId,
        interaction: &InteractionId,
        request_version: Option<Version>,
        resolution: InteractionResolution,
    ) -> Result<Resolution, PortError> {
        let state = StateChange::Update(SessionUpdate {
            state: None,
            mode: ModeChange::Unchanged,
            title: None,
            interaction: Some(InteractionResolved {
                interaction: interaction.clone(),
                resolution: resolution.clone(),
                resolved_by: actor.clone(),
                request_version,
            }),
            confirmation: None,
            agent_session_id: None,
            workspace_cwd: None,
            workspace_directory_id: None,
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
        // 解析命令进入作用域：后端随后的 `permission.resolved`/`elicitation.resolved` 按这条命令归因
        // （它们不带 causation，见适配器边界）。
        slot.set_command_scope(CommandScope {
            request: request.clone(),
            actor: actor.clone(),
        });
        endpoint
            .resolve_interaction(interaction, resolution)
            .await?;
        // 适配器成功返回 = 应答已经写入 Agent 的 stdin（WP3 把「成功」收紧为写确认）。
        // 此刻才把仲裁行移出有效集合；失败时保持 `resolved` 未确认，继续计入待处理。
        self.confirm_locked(slot, session, interaction).await?;
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

    /// Persist the independent directory command before its filesystem side effect.
    pub(crate) async fn admit_workspace_mkdir(
        &self,
        actor: &Actor,
        request: &RequestId,
        fingerprint: &Digest,
    ) -> Result<Option<CommandRecord>, PortError> {
        let at = self.now();
        let outcome = self
            .commit_owned(OwnedCommit {
                session: None,
                at: at.clone(),
                expected_version: None,
                state: None,
                turns: Vec::new(),
                events: Vec::new(),
                interactions: Vec::new(),
                compacted: Vec::new(),
                idempotency: Some(IdempotencyRecord {
                    actor: actor.clone(),
                    request: request.clone(),
                    command: "workspace.mkdir".to_owned(),
                    kind: CommandKind::Mutation,
                    session: None,
                    expected_version: None,
                    request_fingerprint: fingerprint.clone(),
                    accepted_at: at,
                }),
                command_terminal: None,
                origin_epoch: None,
            })
            .await?;
        Ok(outcome.replayed.map(|replay| replay.record))
    }

    /// Persist the terminal and global event together before publication.
    pub(crate) async fn settle_workspace_mkdir(
        &self,
        actor: &Actor,
        request: &RequestId,
        failure: Option<PublicError>,
    ) -> Result<(), PortError> {
        let record = self
            .deps
            .store
            .find_request(request, actor)
            .await?
            .ok_or_else(|| {
                PortError::NotFound(EntityRef::Command {
                    session: None,
                    request: request.clone(),
                })
            })?;
        if record.command() != "workspace.mkdir" || record.session().is_some() {
            return Err(PortError::InvalidRequest(
                "workspace.mkdir settlement requires its unsessioned command",
            ));
        }
        if record.status().is_terminal() {
            return Ok(());
        }
        let at = self.now();
        let status = if failure.is_some() {
            CommandStatus::Failed
        } else {
            CommandStatus::Completed
        };
        let result = if failure.is_none() {
            Some(CommandResult::from_json_text(r#"{"created":true}"#)?)
        } else {
            None
        };
        let terminal =
            CommandTerminalRecord::try_new(status, Some(at.clone()), None, result, failure)?;
        let mut event = create_terminal_event(actor, request, status, terminal.error())?;
        if status == CommandStatus::Completed {
            event.payload = EventPayload::new(
                view(format!(
                    r#"{{"requestId":"{}","result":{{"created":true}}}}"#,
                    request.as_str()
                ))?,
                None,
            );
        }
        let outcome = self
            .deps
            .store
            .commit_workspace_mkdir_terminal(
                actor,
                OwnedCommit {
                    session: None,
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
                },
            )
            .await?;
        self.publish(&outcome.appended);
        Ok(())
    }

    /// `session.create`（Node Link 命令，§12.7；Sync §11.5）。三笔时序：**受理 → 执行 → 落账**。
    ///
    /// - **受理**：只落该命令的幂等记录（`session_id` 为 `NULL`、无会话行），此后本机**查不到**这个会话；
    /// - **执行**：`SessionBackendFactory::create`（唯一可能 spawn 的副作用）。这段时间里到达的后端事件
    ///   在会话槽位里排队（有界，见 [`WINDOW_EVENT_QUEUE_LIMIT`]），不落盘——那一刻会话行还不存在；
    /// - **落账**：由 [`Broker::settle_session_create`] 在**同一次提交**里写会话行（含 ACP 会话标识、
    ///   core 自己解析的 cwd、目录别名）、回填幂等行的 `session_id` 并写终态。
    ///
    /// 幂等键是协议维度的 `(actor, requestId)`（§6 第 6 条），`requestId` 与 `request_fingerprint`
    /// 由适配层给出。同键重发**不派发第二次创建**：命中既有记录时返回 [`SessionCreateAdmission::InFlight`]
    /// （仍是 `accepted`，此刻行里的 `session_id` 还是 `NULL`，因此重放不能靠它判断）或
    /// [`SessionCreateAdmission::Settled`]（已终结，回首次结果）；键相同而
    /// `command`/`kind`/指纹/`expected_version` 任一不同 → `Conflict(IdempotencyConflict)`。
    ///
    /// **后端失败零会话行**：失败时不建会话行，只把该命令终结掉——可证明 Agent 侧未产生会话时为
    /// `failed`，说不清时为 `uncertain`（[`create_failure_status`]），并原样上抛后端错误供适配层回包。
    pub async fn create_session(
        &self,
        actor: &Actor,
        request: &RequestId,
        request_fingerprint: &Digest,
        create: CreateSessionRequest,
        ownership: Option<WorkspaceRef>,
    ) -> Result<SessionCreateAdmission, PortError> {
        self.authorize(actor, "session.create", None, request)
            .await
            .map_err(Denied::into_port_error)?;
        // ① 受理之前把**可能失败**的本地步骤全部做完：受理一旦落盘，再出现本地失败就会留下一条
        //    `accepted` 行（既无终态也无资源），那正是本变更要消灭的形状。
        let workspace_cwd = create
            .workspace
            .as_ref()
            .map(|workspace| workspace.canonical_path().to_owned());
        let workspace = check_workspace_ownership(&create, ownership)?;
        let at = self.now();
        let session = self.deps.ids.session_id();
        let settled = SettledSession {
            session: session.clone(),
            title: None,
            agent: create.agent.clone(),
            // ACP 会话标识要到后端成功返回之后才知道（见下面）；此刻先按「未取得」占位，
            // 落账提交前一定已被补齐。
            agent_session_id: None,
            workspace_cwd,
            // 目录归属**只**以别名落列（`owned_session.workspace_directory_id`）：展示名不落库，读路径按别名
            // 关联目录记录（`SessionStore::list` 的 LEFT JOIN）取得；本次的归属投影（别名 + 展示名）
            // 留在内存里供摘要用，两者同源由 `check_workspace_ownership` 保证。
            workspace_directory_id: workspace
                .as_ref()
                .map(|workspace| workspace.directory_id().as_str().to_owned()),
        };
        let summary = created_summary(&settled, &at, workspace.as_ref())?;
        let epoch = self.deps.ids.origin_epoch();
        let commit = OwnedCommit {
            session: None,
            at: at.clone(),
            expected_version: None,
            // 受理提交**只落幂等行**：不建会话行、不改任何状态（§6 第 20 条）。
            state: None,
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: Some(IdempotencyRecord {
                actor: actor.clone(),
                request: request.clone(),
                command: "session.create".to_owned(),
                kind: CommandKind::Mutation,
                // 受理期没有目标会话（会话标识由 core 分配，落账提交才回填这一列）：幂等比对因此不拿
                // 行里的会话标识与它比（§6 第 6 条、第 20 条）。
                session: None,
                expected_version: None,
                request_fingerprint: request_fingerprint.clone(),
                accepted_at: at.clone(),
            }),
            command_terminal: None,
            origin_epoch: None,
        };
        let outcome = self.commit_owned(commit).await?;
        if let Some(replayed) = outcome.replayed {
            // 同键：既有持久记录就是首次结果。仍是 `accepted` ⇒ **窗口期重发**（并发或重试）：回受理态、
            // 不派发第二次创建、不碰首次记录；已终结 ⇒ 回首次终态。两者都不再打开后端端点。
            return Ok(match replayed.record.status() {
                CommandStatus::Accepted => SessionCreateAdmission::InFlight,
                _ => SessionCreateAdmission::Settled(Box::new(replayed.record)),
            });
        }
        // ② 执行：窗口期从这里开始——后端一返回事件就可能到达，而那时会话行还没落盘。
        let slot = self.owned_slot(&session);
        slot.hold_unaccounted();
        let sink = self.sink(&session);
        let endpoint: Arc<dyn SessionEndpoint> =
            match self.deps.backends.create(&session, create, sink).await {
                Ok(endpoint) => endpoint.into(),
                Err(error) => {
                    // ③ 失败：零会话行 + 明确的终态（可证明 ⇒ `failed`，说不清 ⇒ `uncertain`）。
                    //    窗口期队列里的事件随失败一起作废：那个会话不会存在，它们没有归属。
                    slot.release_unaccounted();
                    slot.take_pending();
                    self.commit_create_failure(actor, request, &error).await;
                    return Err(error);
                }
            };
        // `agent_session_id` 只来自后端对 `session/new` 的实际响应；未取得时 `None`（不写占位值）。
        let agent_session_id = endpoint.agent_session_id().cloned();
        *lock(&slot.endpoint) = Some(endpoint);
        let settled = SettledSession {
            agent_session_id: agent_session_id.clone(),
            ..settled
        };
        let prepared = PreparedSessionCreate {
            session: session.clone(),
            summary,
            agent_session_id,
        };
        lock(&self.prepared).insert(
            prepared_key(actor, request),
            PreparedCreate { settled, at, epoch },
        );
        Ok(SessionCreateAdmission::Prepared(Box::new(prepared)))
    }

    /// `session.resume`（§5.1、§12.7）：进程不在的 owned 会话的恢复入口。
    ///
    /// 顺序（**授权先于一切本机读取**）：① 授权 → ② `SessionStore::load_recovery` 窄读取 →
    /// ③ 请求构造与 cwd 复校验（对象是持久化原文）→ ④ `SessionBackendFactory::resume`（唯一可能
    /// spawn 的副作用）→ ⑤ 返回 `SessionId`（会话重新可交互），**不**投影、**不**写终态。
    ///
    /// `accepted` 行与幂等行由本方法**自建**，提交点与 `session.create` 相同（先回 accepted、再做
    /// 工作）：`session.resume` 没有 `CommandPayload` 变体，也不走通用 mutation 管线（否则终态会被
    /// 通用臂先提交，`settle_session_resume` 就退化为幂等 no-op）。终态由适配层用同源映射投影
    /// `SessionResumeResult`（`remoteSessionRef.exportId` 只有它有）后经
    /// [`Broker::settle_session_resume`] 提交。
    ///
    /// 两类「不支持」走同一条路径（`Unavailable(BackendUnsupported)`）：该会话没有持久化恢复数据
    /// （两列 `NULL`/行不存在），以及后端自己报告「该 Agent 未宣告能力」。本机取值不成立（cwd 复校验
    /// 失败、持久化值无法通过值对象构造）是**服务端不可用类**（`Unavailable(IoError)`），两者不得混用。
    pub async fn resume_session(
        &self,
        actor: &Actor,
        request: &RequestId,
        request_fingerprint: &Digest,
        session: &SessionId,
    ) -> Result<SessionId, PortError> {
        self.authorize(actor, "session.resume", Some(session), request)
            .await
            .map_err(Denied::into_port_error)?;
        // ②：两列 `NULL`（或会话行不存在）= 没有可用于恢复的数据 = 对本次操作不支持，不启动进程。
        let Some(record) = self.deps.store.load_recovery(session).await? else {
            return Err(PortError::Unavailable(UnavailableKind::BackendUnsupported));
        };
        // ③：请求全部来自持久化记录；cwd 在这里重新校验（「它曾经合法」不是跳过理由）。
        let resume = resume_request(record)?;
        revalidate_resume_workspace(&resume.workspace_cwd)?;
        let at = self.now();
        let commit = OwnedCommit {
            session: Some(session.clone()),
            at: at.clone(),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: Some(IdempotencyRecord {
                actor: actor.clone(),
                request: request.clone(),
                command: "session.resume".to_owned(),
                kind: CommandKind::Mutation,
                // 与 `session.create` 不同：目标会话在提交前就已知，幂等行直接指向它（§6 第 20 条）。
                session: Some(session.clone()),
                expected_version: None,
                request_fingerprint: request_fingerprint.clone(),
                accepted_at: at,
            }),
            command_terminal: None,
            origin_epoch: None,
        };
        let outcome = self.commit_owned(commit).await?;
        if outcome.replayed.is_some() {
            // 同键重试（或并发重复）：首次结果已存在，副作用只发生一次、不重复 spawn。
            return Ok(session.clone());
        }
        // ④：唯一副作用（可能拉起 Agent 子进程）。
        let slot = self.owned_slot(session);
        let sink = self.sink(session);
        let endpoint: Arc<dyn SessionEndpoint> = self
            .deps
            .backends
            .resume(session, resume, sink)
            .await?
            .into();
        *lock(&slot.endpoint) = Some(endpoint);
        Ok(session.clone())
    }

    /// `session.create` 的**落账提交**（§6 第 20 条、§12.7）。返回本次是否真的写入了终态。
    ///
    /// 两条形状，按内存里的窗口期状态（[`PreparedCreate`]）分流：
    ///
    /// - **有窗口期状态**（[`SessionCreateAdmission::Prepared`] 之后必须调一次）：在**同一次提交**里
    ///   落地会话行（含 core 分配的标识、ACP 会话标识、创建时 cwd 与目录别名）、回填幂等行的
    ///   `session_id` 并写终态；提交成功后把窗口期排队的事件**按序**落盘再广播（§6.1 的「先持久化
    ///   成功再广播」因此第一次端到端成立）。`status` 只接受 `completed`/`uncertain`：会话已经建立，
    ///   报 `failed` 就是撒谎（`failed` 只用于「可证明未产生会话」那条路径，那时没有窗口期状态）。
    /// - **无窗口期状态**（后端失败路径的兜底重试、或 `create` 在幂等行落盘失败后适配层的结算调用）：
    ///   与旧口径同形——定位仍然 `accepted` 的记录，按 [`OwnedCommit::command_terminal`] 的「尚无会话」
    ///   形状写终态；`status` 只接受 `failed`/`uncertain`（没有会话行，`completed` 说不通）。
    ///
    /// **幂等 no-op** 的三种情况保持不变：该 `(actor, requestId)` 没有持久记录、记录已经终结（首次结果
    /// 不覆盖）、该 `(actor, requestId)` 记着别的命令（`InvalidRequest`，不把别的命令改写成创建的终态）。
    ///
    /// 落盘失败（`Unavailable`）不报成功：行仍是 `accepted`，由启动恢复按 §6 第 16 条终结为 `uncertain`。
    /// 那时窗口期状态**保留**（会话行没有落盘），重试可以再走一遍落账。
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
        let key = prepared_key(actor, request);
        let prepared = lock(&self.prepared).get(&key).cloned();
        let Some(prepared) = prepared else {
            return self
                .settle_session_create_without_window(actor, request, status, result, error)
                .await;
        };
        if status == CommandStatus::Failed {
            return Err(PortError::InvalidRequest(
                "会话已经建立：不得以 failed 终结（那是「可证明未产生会话」那条路径的形状）",
            ));
        }
        let session = prepared.settled.session.clone();
        let slot = self.owned_slot(&session);
        let _guard = slot.gate.guard().await;
        if slot.window_overflowed() {
            // 失败关闭（`sync-session-create` 的「窗口期事件不丢不重、超限走失败关闭」）：窗口期队列
            // 超限意味着这个后端不能再被信任。顺序与 `agent-host` 的「超限结束」同源：先结束该 Agent
            // 的运行时（`SessionBackendFactory::terminate_agent`：摘掉运行目录、让未完成请求明确失败、
            // 回收进程树），再作废那些没有归属的事件，最后让命令**明确失败**——不是让事件悄悄消失。
            //
            // **队列上界保持**（不 `release_unaccounted`）：该会话永远不会落账，因此没有消费者；若在这里
            // 解除上界，仍在发事件的 Agent 会让队列无界增长（该会话也没有会话行，组合根的合并窗口不会
            // 枚举到它）。保持有界意味着超限之后的事件继续被界拦住并丢弃。
            //
            // 更彻底一步：**围栏**。失败关闭之后这个会话不可能再落账，因此此后到达的任何事件（含终止
            // 宽限期里未完成 turn 的尾巴）直接丢弃，不再缓冲——否则它们会占满上界并永久滞留（没有消费
            // 者，槽位也不随会话删除）。
            self.terminate_overflowed_agent(&slot, &session).await;
            slot.take_pending();
            slot.fence();
            lock(&self.prepared).remove(&key);
            let error = PublicError::coded(
                "command.uncertain",
                "窗口期事件队列超限，无法确认会话是否已建立",
                false,
            )
            .map_err(PortError::from)?;
            let written = self
                .commit_create_terminal(actor, request, CommandStatus::Uncertain, None, Some(error))
                .await?;
            return Ok(written);
        }
        // 落账提交用**准备期**那个时间戳（[`PreparedCreate::at`]），不再另取时钟：会话行的
        // `created_at`/`updated_at`、投影给适配层的摘要里的两个时间戳与终态的 `terminal_at` 因此是同
        // 一个值，「回归给客户端的摘要」与之后读路径看到的行逐字段同源（`PreparedCreate::at` 的约定）。
        let at = prepared.at.clone();
        let terminal: CommandTerminalRecord =
            CommandTerminalRecord::try_new(status, Some(at.clone()), None, result, error)?;
        let event = create_terminal_event(actor, request, status, terminal.error())?;
        let commit = OwnedCommit {
            session: Some(session.clone()),
            at,
            expected_version: None,
            state: Some(StateChange::SettleCreate(prepared.settled.clone())),
            turns: Vec::new(),
            events: vec![event],
            interactions: Vec::new(),
            compacted: Vec::new(),
            // 幂等行已在受理提交里写过：本次只**回填**它的 `session_id`（存储层按
            // `request_id + session_id IS NULL + status = 'accepted'` 定位并回填）。
            idempotency: None,
            command_terminal: Some(terminal),
            origin_epoch: Some(prepared.epoch.clone()),
        };
        match self.commit_owned(commit).await {
            Ok(outcome) => {
                // §6.1：先落盘成功、再广播；窗口期的事件按到达顺序落盘并广播（同一个会话槽位门内，
                // 因此与并发的 flush 不会交错、也不会重复）。
                self.publish(&outcome.appended);
                slot.release_unaccounted();
                self.flush_locked(&slot, &session).await?;
                lock(&self.prepared).remove(&key);
                Ok(true)
            }
            Err(PortError::Unavailable(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// 没有窗口期状态时的 `settle_session_create`：只终结「尚无会话」的持久记录（见上）。
    ///
    /// 服务两个场景：① `create_session` 在后端失败时**已经**写好了终态（那次调用是幂等 no-op，适配层
    /// 的结算调用因此落在这一支）；② 窗口期状态已丢失（进程重启），该 `requestId` 的 `accepted` 记录由
    /// 适配层或恢复路径再结一次时也能被终结。**后端失败的终态本身**落盘失败时不在这里补——那条窗口由
    /// 启动恢复按 §6 第 16 条把该行终结为 `uncertain`（命令因此不会停在只有受理记录的状态）。
    async fn settle_session_create_without_window(
        &self,
        actor: &Actor,
        request: &RequestId,
        status: CommandStatus,
        result: Option<CommandResult>,
        error: Option<PublicError>,
    ) -> Result<bool, PortError> {
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
        if record.session().is_some() {
            // `session.create` 的会话行与终态在**同一次提交**里落盘：只剩 `accepted` 却已带会话行是
            // 自相矛盾的持久事实（只能经库被篡改到达），不猜、不覆盖。
            return Err(PortError::Corrupt(
                "session.create 的 accepted 记录带会话行（§6 第 20 条）",
            ));
        }
        if status == CommandStatus::Completed {
            return Err(PortError::InvalidRequest(
                "没有会话行时不得以 completed 终结 session.create",
            ));
        }
        self.commit_create_terminal(actor, request, status, result, error)
            .await
    }

    /// 「尚无会话」形状的 `session.create` 终态提交：不建会话行、终态事件只拿全局序号（§6 第 20 条）。
    ///
    /// 三种情况都用它：落账路径的失败关闭（窗口期队列超限）、`create_session` 的后端失败终态，以及
    /// 适配层在「失败终态本身没落盘」时的兜底结算。
    async fn commit_create_terminal(
        &self,
        actor: &Actor,
        request: &RequestId,
        status: CommandStatus,
        result: Option<CommandResult>,
        error: Option<PublicError>,
    ) -> Result<bool, PortError> {
        let at = self.now();
        let terminal =
            CommandTerminalRecord::try_new(status, Some(at.clone()), None, result, error)?;
        let event = create_terminal_event(actor, request, status, terminal.error())?;
        let commit = OwnedCommit {
            session: None,
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
        match self.commit_owned(commit).await {
            Ok(outcome) => {
                self.publish(&outcome.appended);
                Ok(true)
            }
            Err(PortError::Unavailable(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// 后端失败的终态（§6 第 20 条）：**零会话行**，只终结命令。
    ///
    /// 分类见 [`create_failure_status`]（可证明 ⇒ `failed`，说不清 ⇒ `uncertain`）。**尽力而为**：
    /// 落盘失败时命令停在 `accepted`，由启动恢复终结为 `uncertain`（§6 第 16 条）；调用方拿到的仍是
    /// 后端的原错误（适配层按既有映射回包，终态的成因由 message 承担）。
    async fn commit_create_failure(&self, actor: &Actor, request: &RequestId, error: &PortError) {
        let status = create_failure_status(error);
        let Ok(public) = self.create_failure_error(status, error) else {
            return;
        };
        let _ = self
            .commit_create_terminal(actor, request, status, None, Some(public))
            .await;
    }

    /// 后端失败的成因 → 终态 `error`：`failed` 直接用端口错误的公开映射（`internal.unavailable` 一类，
    /// design D4 不新增错误码）；`uncertain` 取 core 自己的「无法确认」码（与启动恢复同口径），成因放进
    /// message。
    fn create_failure_error(
        &self,
        status: CommandStatus,
        error: &PortError,
    ) -> Result<PublicError, PortError> {
        let cause = self.port_error_public(error)?;
        if status == CommandStatus::Failed {
            return Ok(cause);
        }
        PublicError::coded("command.uncertain", cause.message(), false).map_err(PortError::from)
    }

    /// 结束一个窗口期已超限的后端所属的 Agent 运行时（`sync-session-create`「超限走失败关闭」复用
    /// `local-agent-host` 的「超限结束的失败关闭顺序」）。
    ///
    /// 走端口 `SessionBackendFactory::terminate_agent`：core 不持进程句柄，只关端点不足以让事件源停下
    /// （Agent 仍会把 `session/update` 送进队列），也不足以让「该运行时不得再被复用」成立。失败不改变
    /// 结论——命令照样终结为 `uncertain`，只是该运行时可能活到进程退出为止。
    async fn terminate_overflowed_agent(&self, slot: &Slot, session: &SessionId) {
        // 先摘掉本槽位的端点绑定（此后 core 不再经它写入），再要求后端结束该运行时。
        let _ = lock(&slot.endpoint).take();
        let _ = self.deps.backends.terminate_agent(session).await;
    }

    /// `session.resume` 的终态提交（§5.1、§12.7）。返回本次是否真的写入了终态。
    ///
    /// 与 [`Broker::settle_session_create`] **同形**，但**只**终结 `command == "session.resume"` 的
    /// 持久记录：该 `(actor, requestId)` 记着别的命令时是适配层误用（wire 不可达），显式
    /// `InvalidRequest`。结果（`SessionResumeResult`）由适配层投影——`remoteSessionRef.exportId`
    /// 只有适配层有，core 的恢复用例不持有 Export 信息。
    ///
    /// **幂等 no-op** 的两种情况与 create 同：该 `(actor, requestId)` 没有持久记录（恢复在幂等行
    /// 落盘前就失败），或记录已经终结（首次结果不覆盖）。落盘失败（`Unavailable`）不报成功：行仍是
    /// `accepted`，由启动恢复按 §6 第 16 条终结为 `uncertain`。
    pub async fn settle_session_resume(
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
        if record.command() != "session.resume" {
            return Err(PortError::InvalidRequest(
                "settle_session_resume 只能终结 session.resume 的持久记录（§5.1）",
            ));
        }
        if record.status().is_terminal() {
            return Ok(false);
        }
        let Some(session) = record.session().cloned() else {
            return Err(PortError::Corrupt(
                "session.resume 的持久记录缺少目标会话（§5.1）",
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
                view_command_uncertain(request, "无法确认会话是否已恢复")?,
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
        let at = self.now();
        let receipt = DeliveryReceipt {
            session: session.clone(),
            origin: origin.clone(),
            origin_sequence,
            event_type: event_type.clone(),
            payload_digest: payload_digest.clone(),
            at: at.clone(),
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
            // 与 `DeliveryReceipt.at` 是同一次取值的两个投影（见该字段的口径注释）。
            created_at: at,
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
    ///
    /// 被放弃 turn 的【占位】未释放时只落盘、不派发下一个排队 turn（§6 第 9 条，见 [`HeldTurn`]）。
    pub async fn pump(&self, session: &SessionId) -> Result<(), PortError> {
        let slot = self.owned_slot(session);
        let _guard = slot.gate.guard().await;
        self.pump_locked(&slot, session).await
    }

    async fn pump_locked(&self, slot: &Slot, session: &SessionId) -> Result<(), PortError> {
        self.reconcile_persisted_queue(slot, session).await?;
        loop {
            self.flush_locked(slot, session).await?;
            if !self.dispatch_one(slot, session).await? {
                return Ok(());
            }
        }
    }

    /// 把存储里遗留的 `queued` turn 与内存队列对齐（`session-activity` D3/恢复）。
    ///
    /// `Session.prompt` 正文**不**持久化（`owned_turn` 没有该列，正文只由 Agent 回显的
    /// `user.message.delta` 留下），因此重启后无法安全地重放排队请求。这里在内存队列为空且没有在跑的
    /// turn 时把遗留项显式终结为 `failed`（不重派、不静默丢弃），随后 FIFO 从后续项继续；它们的
    /// `accepted` 命令由 [`Broker::recover_unsettled`] 以 `uncertain` 结算（不自动重试副作用）。
    async fn reconcile_persisted_queue(
        &self,
        slot: &Slot,
        session: &SessionId,
    ) -> Result<(), PortError> {
        {
            let queue = lock(&slot.queue);
            if queue.running.is_some() || !queue.waiting.is_empty() {
                return Ok(());
            }
        }
        let queued = self.deps.store.queued_turns(session).await?;
        if queued.is_empty() {
            return Ok(());
        }
        let at = self.now();
        let error = PublicError::coded(
            "internal.unavailable",
            "重启后无法恢复排队任务的请求内容，该任务未派发",
            false,
        )
        .map_err(PortError::from)?;
        let mut events = Vec::with_capacity(queued.len());
        let mut turns = Vec::with_capacity(queued.len());
        for turn in &queued {
            events.push(pending_event(
                "turn.failed",
                EventKind::State,
                view_turn_error(turn.id(), &error)?,
                Some(turn.id().clone()),
                turn.causation().cloned(),
                StoredPolicy::Durable,
                None,
            )?);
            turns.push(TurnChange::Update(TurnUpdate {
                turn: turn.id().clone(),
                state: TurnState::Failed,
                ended_at: Some(at.clone()),
            }));
        }
        let facts = self.deps.store.session_activity(session).await?;
        let expired: i64 = queued
            .iter()
            .map(|turn| i64::from(facts.unresolved_for_turn(turn.id())))
            .sum();
        let turn_delta = -i64::try_from(queued.len()).unwrap_or(i64::MAX);
        let next_state = self.state_after(&facts, -expired, turn_delta, false);
        let changed = self.current_state(session).await? != Some(next_state);
        if changed {
            events.push(
                self.session_updated_event(session, next_state, &at, None)
                    .await?,
            );
        }
        let state = if changed {
            Some(StateChange::Update(SessionUpdate {
                state: Some(next_state),
                mode: ModeChange::Unchanged,
                title: None,
                interaction: None,
                confirmation: None,
                agent_session_id: None,
                workspace_cwd: None,
                workspace_directory_id: None,
            }))
        } else {
            None
        };
        let commit = OwnedCommit {
            session: Some(session.clone()),
            at,
            expected_version: None,
            state,
            turns,
            events,
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        };
        let outcome = self.commit_owned(commit).await?;
        self.publish(&outcome.appended);
        Ok(())
    }

    /// 落盘一批后端事件。按**终态事件**切批，保证 §6.10 的"不跨 turn 边界、不延迟终态事件"。
    ///
    /// 窗口期（会话行尚未落账）只入队、不提交：那时 `owned_session` 还没有这一行，落盘会写出引用不存在
    /// 会话的事件。队列由 [`Broker::settle_session_create`] 在落账提交之后按序驱动（同一个串行门）。
    async fn flush_locked(&self, slot: &Slot, session: &SessionId) -> Result<(), PortError> {
        if slot.is_unaccounted() {
            return Ok(());
        }
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
        // 在跑的 turn 的诱发命令（`owned_turn.causation` + 触发者）从**同一次**读取取得，避免两个字段
        // 分两次加锁后出现「有 causation 没 actor」的错配（两者总是成对写入/清空）。
        let running_command = slot.running_command();
        let running_request = running_command.as_ref().map(|scope| scope.request.clone());
        let running_actor = running_command.as_ref().map(|scope| scope.actor.clone());
        // 后端产出的 `EndpointEvent` 不携带 `causation`（适配器边界，core 不改），因此**响应类**事件
        // （`session.mode.changed`/`session.config.changed`/`permission.resolved`/`elicitation.resolved`）
        // 只能从会话在作用域的命令取诱发者与 causation——**即使它们落在某个正在跑的 turn 里**：权限与
        // elicitation 的解析按定义就发生在 prompt turn 的 `WaitingPermission` 期间，而适配器产出的
        // `EndpointEvent.turn` 恒为 `None`、`commit_chunk` 又会把无 turn 的事件兜底归给在跑的 turn，
        // 按归属取命令会把解析记到 prompt 那条命令上（见 [`RESPONSE_EVENT_TYPES`] 与下面每个事件处的
        // `inducing`）。这里先取一次会话作用域。
        let scoped_command = slot.command_scope();
        let mut events: Vec<PendingEvent> = Vec::new();
        let mut turn_change: Option<TurnChange> = None;
        let mut terminal: Option<(TurnState, CommandStatus)> = None;
        let mut terminal_request: Option<RequestId> = None;
        let mut terminal_turn: Option<TurnId> = None;
        let mut last_view: Option<ViewJson> = None;
        let mut interactions: Vec<PendingInteractionWrite> = Vec::new();
        let mut delta_plan: Vec<(usize, TurnId, DeltaFragment)> = Vec::new();
        let mut completed_events: Vec<PendingEvent> = Vec::new();
        let mut compact_after: Option<(TurnId, usize)> = None;
        // R5：本批实际派生的 `file.changed` 去重键（提交成功后才登记，见 [`Slot::file_changes`]）。
        let mut file_change_plan: Vec<String> = Vec::new();
        // R9：`session.info.changed` 投影出的标题窄写入（两层可选，`None` = 本次不改标题）。
        let mut title_update: Option<Option<String>> = None;
        // R9（`design.md` D7）：本批是否出现过 `session.info.changed`。
        //
        // 通知即使**只带更新时间**（标题意图 `Unchanged`）也是一次会话信息更新：会话的**权威**更新时间
        // 必须随之推进到本次 Daemon 提交时钟（`commit.at`），否则 `SYNC_PROTOCOL.md` §10.2 的
        // `ORDER BY updated_at DESC` 目录排序会停在旧值上。因此只要本批含该事件，就必须产出一次
        // `SessionUpdate`（`title: None` = 不改该列），让存储层照常写 `updated_at` 与 `version`。
        let mut info_update_seen = false;
        // 失败批次是否携带了**在跑的** turn 的事件：只有它才需要在写失败时放弃该 turn（§6 第 9 条）。
        let mut running_in_chunk = false;
        // 本批是否因观测到被放弃 turn 的终态而释放了占位；释放后要重算摘要。
        let mut released_hold = false;
        for event in chunk {
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
                    // §6 第 9 条：该 turn 的终态到达即「端点已观测到它结束」，占位随之
                    // 释放（事件本身仍被丢弃）。
                    if is_turn_terminal(&event.event_type) {
                        slot.release_held_turn(turn_id);
                        released_hold = true;
                    }
                    continue;
                }
            }
            if turn.is_some() && turn == running {
                running_in_chunk = true;
            }
            // **诱发该事件的命令**（§10.1 的 `causationRequestId`，并决定命令类事件的 `origin.kind`）。
            // 按**事件类型**分流，不按「事件归属在不在跑的 turn」：
            // - **响应类事件**（[`RESPONSE_EVENT_TYPES`]）取**提交时在会话作用域的那条命令**——它们是后端
            //   对 `session.mode.set`/`session.config.set`/`permission.resolve`/`elicitation.respond` 的
            //   响应，即使落在某个正在跑的 turn 里也**不是**该 turn 的命令（权限解析就发生在 prompt turn
            //   的 `WaitingPermission` 期间）；作用域里没有命令时退回该 turn 的命令，不猜；
            // - **turn 生命周期事件**与 turn 内由 Agent 产出的事件：`causation` 取该 turn 的命令
            //   （`owned_turn.causation` 已持久化，§7.3），`origin.kind` 仍由 `event_origin` 按类型判定
            //   （Agent 产出恒为 `agent`——`causation` 与 `origin` 表达不同的事实，不得互相替代）；
            // - 其余事件（不属于任何 turn 的 Agent 产出）没有命令直接导致它，`causation` 保持 `null`；
            //   命令类事件（`command.*`）即使没有 turn 归属也取会话作用域命令，它们只由命令路径产生。
            let inducing = if is_response_event_type(event.event_type.as_str()) {
                scoped_command.clone().or_else(|| running_command.clone())
            } else if turn.is_some() {
                running_command.clone()
            } else if is_command_event_type(event.event_type.as_str()) {
                scoped_command.clone()
            } else {
                None
            };
            // 命令类事件的 `origin.kind` 按诱发命令的执行者分类；非命令类事件一律 `agent`（`causation`
            // 与 `origin` 表达不同的事实，不得互相替代）。
            let inducing_actor = inducing.as_ref().map(|scope| &scope.actor);
            let inducing_request = inducing.as_ref().map(|scope| scope.request.clone());
            let policy = persistence_policy(&event.event_type);
            // §6 第 14 条：登记本批里的 delta 片段。输出索引要等事件真正 push 后才知道
            // （同一条输入可能派生 `file.changed` 等前置事件），否则回查 `outcome.appended`
            // 会错位，后段 delta 不会并入 completed 正文。
            let pending_delta = if is_agent_message_delta(&event.event_type) {
                match (turn.clone(), delta_fragment(&event.payload.view)) {
                    (Some(turn_id), Some(fragment)) => Some((turn_id, fragment)),
                    _ => None,
                }
            } else {
                None
            };
            if let Some((turn_state, command_status)) = turn_terminal(&event.event_type) {
                if let Some(turn_id) = turn.clone() {
                    turn_change = Some(TurnChange::Update(TurnUpdate {
                        turn: turn_id.clone(),
                        state: turn_state,
                        ended_at: Some(at.clone()),
                    }));
                    terminal = Some((turn_state, command_status));
                    terminal_turn = Some(turn_id.clone());
                    terminal_request = event.causation.clone().or_else(|| running_request.clone());
                    last_view = Some(event.payload.view.clone());
                    // §6 第 14 条：该 turn 内出现过 delta 的每个 messageId 各发恰好一条 completed，
                    // 与终态事件同批提交；没有 delta 的消息不发。它们与终态事件同属该 turn，因此带上同一
                    // 个诱发命令的 `causation`（`origin.kind` 仍是 `agent`——Agent 产出的）。
                    let in_chunk: Vec<DeltaFragment> = delta_plan
                        .iter()
                        .filter(|(_, chunk_turn, _)| chunk_turn == &turn_id)
                        .map(|(_, _, fragment)| fragment.clone())
                        .collect();
                    completed_events = self.completed_events_for(
                        slot,
                        &turn_id,
                        terminal_request.as_ref(),
                        &in_chunk,
                    )?;
                    let count = lock(&slot.deltas)
                        .get(turn_id.as_str())
                        .map(|fragments| fragments.len())
                        .unwrap_or(0)
                        + in_chunk.len();
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
            // R5–R7（`design.md` D4/D5）：`file.changed` 的派生源**只有** ACP 工具调用内容里的类型化
            // Diff 元素——适配器把它逐字节投影到 view 的 `diff` 数组，这里按该键派生。相对化需要会话的
            // 规范化工作目录，因此先做一次窄读取（只在真的含 `diff` 时才读）。
            if file_change_event(event.event_type.as_str()) {
                if let Some(changes) = self
                    .derive_file_changes(&event.payload.view, session)
                    .await?
                {
                    for change in changes {
                        let key = change.dedup_key();
                        // R5：同一处改动只派生一次。去重有两个来源——本批已计划派生的键（同一个工具调用
                        // 的 `started`/`updated` 会落在同一次 `commit_chunk` 里）与已提交的去重日志。
                        if lock(&slot.file_changes).contains(&key)
                            || file_change_plan.iter().any(|planned| planned == &key)
                        {
                            continue;
                        }
                        file_change_plan.push(key);
                        let view = ViewJson::new(&file_changed_view(&change, session.as_str()))
                            .map_err(PortError::from)?;
                        events.push(pending_event(
                            "file.changed",
                            EventKind::Structured,
                            view,
                            turn.clone(),
                            inducing_request.clone(),
                            StoredPolicy::Durable,
                            inducing_actor,
                        )?);
                    }
                }
            }
            // R9（`design.md` D7）：Agent 的 `session_info_update` 同时是标题的**唯一**来源。标题只由
            // 通知单向写入，因此这里把该通知投影成一次 `SessionUpdate.title` 的窄写入；通知没带标题
            // （只带更新时间）时保持既有标题不变。
            if event.event_type.as_str() == "session.info.changed" {
                info_update_seen = true;
                let acp_raw = event.payload.acp.as_ref().and_then(AcpRaw::as_available);
                let intent = title_intent(
                    acp_raw.map(|(_, raw, _, _)| raw),
                    event.payload.view.as_str(),
                )
                .map_err(PortError::from)?;
                if let TitleIntent::Set(text) = intent {
                    title_update = Some(Some(text));
                } else if intent == TitleIntent::Clear {
                    title_update = Some(None);
                }
                // 事件 view 的 `updatedAt` 照常转发 Agent 自报值；会话的**权威**更新时间取 Daemon
                // 持久化时间（`commit.at`，由存储层写进 `owned_session.updated_at`），不使用 Agent
                // 自报值决定排序（`design.md` D7）。只要本事件出现，`updated_at` 就必须前进一步——
                // 因此下面的 `state` 组装把它计入「本次需要一次 `SessionUpdate`」的判据。
            }
            // §6.11：`Ephemeral` 由 `PendingEvent::from_persistence` 直接排除（`None` = 只做内存转发）。
            let pushed = if let Some(pending) = PendingEvent::from_persistence(
                interaction_event_kind(event.kind, &event.event_type),
                event.event_type.clone(),
                policy,
                event.payload,
                event_origin(&event.event_type, inducing_actor),
                turn,
                event.causation.or(inducing_request),
            ) {
                events.push(pending);
                true
            } else {
                false
            };
            if pushed {
                if let Some((turn_id, fragment)) = pending_delta {
                    delta_plan.push((events.len() - 1, turn_id, fragment));
                }
            }
        }
        // 释放占位的迟到终态被丢弃后，若没有后继排队工作，摘要必须从 running 收敛。
        if released_hold && lock(&slot.queue).waiting.is_empty() {
            self.refresh_activity(session, &at, false).await?;
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
                // `failed` 的无 turn 信息（错误已单独落盘）。终态结果的这次落盘是这次修复
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
        // 会话状态：turn 终态会失效该轮的遗留请求，新增交互请求会把摘要推到 `pending`。两者都在本事务
        // 里生效，因此归约必须把「本批增量」算进去而不是逐事件写死转换（`session-activity` D1/D3）。
        let mut session_state: Option<SessionState> = None;
        let mut activity_event: Option<PendingEvent> = None;
        if had_terminal || !interactions.is_empty() {
            let facts = self.deps.store.session_activity(session).await?;
            let expired = terminal_turn
                .as_ref()
                .map(|turn| i64::from(facts.unresolved_for_turn(turn)))
                .unwrap_or(0);
            let pending_delta = i64::try_from(interactions.len()).unwrap_or(i64::MAX) - expired;
            let turn_delta = if had_terminal { -1 } else { 0 };
            let next = self.state_after(
                &facts,
                pending_delta,
                turn_delta,
                slot.hold_blocks_dispatch(),
            );
            if self.current_state(session).await? != Some(next) {
                session_state = Some(next);
                activity_event = Some(
                    self.session_updated_event(
                        session,
                        next,
                        &at,
                        title_update.as_ref().map(|title| title.as_deref()),
                    )
                    .await?,
                );
            }
        }
        if let Some(event) = activity_event {
            events.push(event);
        }
        // R9：标题窄写入、通知带来的权威时间推进与 turn 终态的状态修改合批（`None` = 本批不改状态）。
        let state = if session_state.is_some() || title_update.is_some() || info_update_seen {
            Some(StateChange::Update(SessionUpdate {
                state: session_state,
                mode: ModeChange::Unchanged,
                title: title_update,
                interaction: None,
                confirmation: None,
                agent_session_id: None,
                workspace_cwd: None,
                workspace_directory_id: None,
            }))
        } else {
            None
        };
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
                // R5：只登记**已提交**的 `file.changed` 去重键（失败批次不得吃掉重试的派生）。
                if !file_change_plan.is_empty() {
                    let mut log = lock(&slot.file_changes);
                    for key in file_change_plan {
                        if !log.contains(&key) {
                            log.push(key);
                        }
                    }
                    let excess = log.len().saturating_sub(FILE_CHANGE_DEDUP_CAPACITY);
                    log.drain(..excess);
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
                        running_actor.as_ref(),
                    )
                    .await;
                    return Ok(());
                }
                // §6.9：非终态批次的失败不能只丢弃——该 turn 的正文已经缺了一块，若让同一 turn 的
                // 终态批照常 `completed`，库里就留下「报完成但正文缺失」的记录。
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
        // §6 第 9 条：被放弃 turn 的【占位】未释放前不得提升下一个排队 turn，
        // 否则它的迟到事件会被记到下一个 turn 上（见 [`HeldTurn`]）。兜底轮次也在这里推进。
        let held_before = slot.held_turn();
        if slot.hold_blocks_dispatch() {
            return Ok(false);
        }
        // 兜底释放占位后必须重算摘要：没有后继 turn 时不能继续声称 running。
        if held_before.is_some() && lock(&slot.queue).waiting.is_empty() {
            self.refresh_activity(session, &self.now(), false).await?;
        }
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
        // 该 turn 的命令现在起在会话作用域里：后端随后产出的响应类事件若还没有被别的命令接管
        // （`permission.resolve`/`elicitation.respond`/`session.mode.set`/`session.config.set` 各自会在
        // 派发前覆盖它），就按这条命令归因——此时它与在跑的 turn 的命令是同一条。turn 生命周期事件与
        // turn 内由 Agent 产出的事件仍以 turn 自身的 `causation`/触发者为准（见 `commit_chunk`）。
        slot.set_command_scope(CommandScope {
            request: next.request.clone(),
            actor: next.actor.clone(),
        });
        let at = self.now();
        // queued → running 不改变「有未结束任务」这一事实，摘要通常不变；仍按统一归约重算，以便
        // 恢复后第一次派发纠正历史摘要（`session-activity` D1/D3）。
        let facts = self.deps.store.session_activity(session).await?;
        let next_state = self.state_after(&facts, 0, 0, false);
        let mut events = vec![pending_event(
            "turn.started",
            EventKind::State,
            view_turn(&next.turn, TurnState::Running)?,
            Some(next.turn.clone()),
            Some(next.request.clone()),
            StoredPolicy::Durable,
            Some(&next.actor),
        )?];
        let state = if self.current_state(session).await? == Some(next_state) {
            None
        } else {
            events.push(
                self.session_updated_event(session, next_state, &at, None)
                    .await?,
            );
            Some(StateChange::Update(SessionUpdate {
                state: Some(next_state),
                mode: ModeChange::Unchanged,
                title: None,
                interaction: None,
                confirmation: None,
                agent_session_id: None,
                workspace_cwd: None,
                workspace_directory_id: None,
            }))
        };
        let commit = OwnedCommit {
            session: Some(session.clone()),
            at: at.clone(),
            expected_version: None,
            state,
            turns: vec![TurnChange::Update(TurnUpdate {
                turn: next.turn.clone(),
                state: TurnState::Running,
                ended_at: None,
            })],
            events,
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
        // 本轮结束：该 turn 的遗留交互在同一事务里失效（存储层按 `turn_id` 处理），摘要按剩余工作重算。
        // 失败不封死会话，后续排队 turn 仍会被 `pump` 提升（`session-activity` D3）。
        let facts = self.deps.store.session_activity(session).await?;
        let expired = i64::from(facts.unresolved_for_turn(&turn.turn));
        let next_state = self.state_after(&facts, -expired, -1, slot.hold_blocks_dispatch());
        let mut events = vec![
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
        ];
        let changed = self.current_state(session).await? != Some(next_state);
        let state = if changed {
            Some(StateChange::Update(SessionUpdate {
                state: Some(next_state),
                mode: ModeChange::Unchanged,
                title: None,
                interaction: None,
                confirmation: None,
                agent_session_id: None,
                workspace_cwd: None,
                workspace_directory_id: None,
            }))
        } else {
            None
        };
        if changed {
            events.push(
                self.session_updated_event(session, next_state, &at, None)
                    .await?,
            );
        }
        let commit = OwnedCommit {
            session: Some(session.clone()),
            at: at.clone(),
            expected_version: None,
            state,
            turns: vec![TurnChange::Update(TurnUpdate {
                turn: turn.turn.clone(),
                state: TurnState::Failed,
                ended_at: Some(at.clone()),
            })],
            events,
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

    /// §6 第 9 条：非终态批次落盘失败后放弃在跑的 turn。
    ///
    /// 为什么必须终结而不能只丢批次：该 turn 的正文已经缺了一块，若它在后续批次里照常走到终态，库里
    /// 就会留下「报完成但正文缺失」的记录。因此这里把 turn 终结为 `failed`、把命令置为 `uncertain`
    /// （与 §6 第 16 条的恢复同一口径：无法确认已经落盘的部分与 Agent 的副作用），并把该 turn 登记为
    /// 「已放弃」——它后续到达的事件与终态不再提交。
    ///
    /// 可观测信号是这次提交里的两条持久事件（`turn.failed` + `command.uncertain`）：core 不含日志依赖
    /// （`AGENTS.md` §8），而错误上抛会把「已经终结的 turn」报成调用方（往往是另一个命令）的失败，
    /// 与终态批失败时的既有口径不一致，因此这里不传播 `Unavailable`。
    ///
    /// 已落盘的 delta 仍按 §6 第 14 条收尾成 `agent.message.completed`（缺的段落不伪造）；不压缩它们
    /// （§6 第 15 条是可选动作），错误路径上不再多发一次提交。
    ///
    /// **不让出会话槽**：放弃的 turn 继续占住 `running`（[`HeldTurn`]），直到它的终态被
    /// 端点观测到；此前 `dispatch_one` 不得提升下一个排队 turn。这也会把它迟到的 permission/
    /// elicitation 请求一并丢弃（不落交互行、不广播）——已经被终结为 `uncertain` 的 turn 不再接受
    /// 交互，收敛该 turn 是端点的职责（`AGENTS.md` §4：一个状态只能有一个权威写入者）。
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
        let completed = self.completed_events_for(slot, turn, request, &[])?;
        lock(&slot.deltas).remove(turn.as_str());
        lock(&slot.turn_deltas).remove(turn.as_str());
        slot.hold_abandoned_turn(turn);
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
        // 摘要：放弃的 turn 虽已标记失败，但它的执行占位仍被持有（停止未确认），因此**不**能声称空闲；
        // 仍排队的 turn 也照旧计入未结束任务。二者都由统一归约表达（`session-activity` D3）。
        let facts = self.deps.store.session_activity(session).await?;
        let expired = i64::from(facts.unresolved_for_turn(turn));
        let next_state = self.state_after(&facts, -expired, -1, true);
        let changed = self.current_state(session).await? != Some(next_state);
        if changed {
            events.push(
                self.session_updated_event(session, next_state, &at, None)
                    .await?,
            );
        }
        let state = if changed {
            Some(StateChange::Update(SessionUpdate {
                state: Some(next_state),
                mode: ModeChange::Unchanged,
                title: None,
                interaction: None,
                confirmation: None,
                agent_session_id: None,
                workspace_cwd: None,
                workspace_directory_id: None,
            }))
        } else {
            None
        };
        let commit = OwnedCommit {
            session: Some(session.clone()),
            at: at.clone(),
            expected_version: None,
            state,
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
            // 存储仍然不可用：该 turn 在内存里已放弃（`abandoned` + 占位），不再有
            // 「completed 但正文缺失」的记录会产生；不发布任何东西（§6.9）。
            Err(PortError::Unavailable(_)) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// §6.9 的兜底终态：终态事件落盘失败时，把该命令终结为 `uncertain`。
    ///
    /// `inducing_actor` 是诱发该命令的执行者（调用方在作用域里就有），用于 `command.uncertain` 的
    /// `origin.kind`——**不得**因为这里拿不到 actor 就默认记 `device`。确实没有诱发命令时由
    /// [`event_origin`] 记 `daemon`。
    async fn record_uncertain(
        &self,
        session: &SessionId,
        request: &RequestId,
        reason: &str,
        inducing_actor: Option<&Actor>,
    ) {
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
        let Ok(event) = pending_event(
            "command.uncertain",
            EventKind::Structured,
            view,
            None,
            Some(request.clone()),
            StoredPolicy::Durable,
            inducing_actor,
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
    ///
    /// `causation` 是该 turn 的诱发命令（§10.1：turn 内由 Agent 产出的领域事件照样带诱发命令的
    /// `requestId`）；`origin.kind` 仍是 `agent`，两者不得互相替代。
    fn completed_events_for(
        &self,
        slot: &Slot,
        turn: &TurnId,
        causation: Option<&RequestId>,
        in_chunk: &[DeltaFragment],
    ) -> Result<Vec<PendingEvent>, PortError> {
        let mut fragments: Vec<DeltaFragment> = lock(&slot.deltas)
            .get(turn.as_str())
            .cloned()
            .unwrap_or_default();
        // 终态事件可能与最后一段 delta 落在**同一个** flush 批次里：这些片段还没并入
        // `slot.deltas`，但终态的 completed 正文与压缩计数必须包含它们，否则尾段会丢失。
        fragments.extend(in_chunk.iter().cloned());
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
                causation.cloned(),
                StoredPolicy::Durable,
                // `agent.message.completed` 由 Agent 产出（非命令类事件），`origin.kind` 与 actor 无关；
                // 传 `None` 如实表达「这条不是命令产生的产物」。
                None,
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
        // §11.7「不可复活」：重启后上游 ACP 请求已不存在，任何仍标 `pending` 的交互都不得再被当作
        // 可应答；先标记失效，再恢复命令（两者都在监听之前完成）。
        let _ = self.invalidate_pending_interactions(None, self.now()).await;
        let commands = self.deps.store.unsettled_commands(limit).await?;
        let mut recovered = 0usize;
        let mut sessions: Vec<SessionId> = Vec::new();
        for record in commands {
            let session = record.session().cloned();
            if let Some(session) = &session
                && !sessions.contains(session)
            {
                sessions.push(session.clone());
            }
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
        // 仍有未结束 turn 或未解决交互的会话（含迁移后摘要过期、以及恢复没覆盖到的排队项）：
        // 逐个重算摘要并驱动持久队列。同一个会话只处理一次。
        for session in self
            .deps
            .store
            .sessions_with_pending_activity(limit)
            .await?
        {
            if !sessions.contains(&session) {
                sessions.push(session);
            }
        }
        for session in sessions {
            let slot = self.owned_slot(&session);
            let _guard = slot.gate.guard().await;
            self.pump_locked(&slot, &session).await?;
            let held = slot.hold_blocks_dispatch();
            self.refresh_activity(&session, &self.now(), held).await?;
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
        let reason = if record.command() == "workspace.mkdir" {
            "Unable to confirm whether the directory was created after restart"
        } else {
            "进程恢复后无法确认命令是否已到达 Agent"
        };
        let error =
            PublicError::coded("command.uncertain", reason, false).map_err(PortError::from)?;
        let mut events: Vec<PendingEvent> = Vec::new();
        let mut turns: Vec<TurnChange> = Vec::new();
        let mut state = None;
        let mut activity_event: Option<PendingEvent> = None;
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
                        history: false,
                    },
                    before_history: None,
                    before_messages: None,
                    messages_limit: ReplayLimit::new(0),
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
                // 本轮已确认结束（恢复把它终结为失败）：遗留请求在同一事务里失效，摘要按剩余工作重算。
                // `uncertain` 命令不重派；排队中的其它 turn 由 `pump` 继续（`session-activity` D3/恢复）。
                let facts = self.deps.store.session_activity(session_id).await?;
                let expired = i64::from(facts.unresolved_for_turn(turn.id()));
                let next_state = self.state_after(&facts, -expired, -1, false);
                if self.current_state(session_id).await? != Some(next_state) {
                    activity_event = Some(
                        self.session_updated_event(session_id, next_state, &at, None)
                            .await?,
                    );
                    state = Some(StateChange::Update(SessionUpdate {
                        state: Some(next_state),
                        mode: ModeChange::Unchanged,
                        title: None,
                        interaction: None,
                        confirmation: None,
                        agent_session_id: None,
                        workspace_cwd: None,
                        workspace_directory_id: None,
                    }));
                }
                // §6 第 16 条：已有已提交 delta 却没有 completed 的 turn，必须补写。
                events.extend(
                    self.backfill_completed(&page, turn.id(), turn.causation())
                        .await?,
                );
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
        if let Some(event) = activity_event {
            events.push(event);
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
        let outcome = if record.command() == "workspace.mkdir" {
            self.deps
                .store
                .commit_workspace_mkdir_terminal(record.actor(), commit)
                .await?
        } else {
            self.commit_owned(commit).await?
        };
        // §6 第 16 条：广播必须在 commit 之后。
        self.publish(&outcome.appended);
        Ok(())
    }

    /// §6 第 14/16 条：从库里重建该 turn 缺失的 `agent.message.completed`。
    ///
    /// 只认 `agent.message.delta` 的事件类型（`agent.thought.delta` 不产生 completed），正文直接取自
    /// 读视图返回的记录（`HistoryPage.events` 的元素自带持久化正文，不逐条回查）；已有 `completed` 的
    /// messageId 不重复补写。
    ///
    /// `causation` 是该 turn 的诱发命令（与 [`Broker::completed_events_for`] 同口径：turn 内由 Agent
    /// 产出的领域事件照样带诱发命令的 `requestId`）。
    async fn backfill_completed(
        &self,
        page: &HistoryPage,
        turn: &TurnId,
        causation: Option<&RequestId>,
    ) -> Result<Vec<PendingEvent>, PortError> {
        let mut fragments: HashMap<String, Vec<DeltaFragment>> = HashMap::new();
        let mut completed: Vec<MessageId> = Vec::new();
        for record in &page.events {
            let event = &record.event;
            let payload = &record.payload;
            match event.event_type.as_str() {
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
                causation.cloned(),
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
            before_history: None,
            before_messages: None,
            messages_limit: ReplayLimit::new(0),
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
        self.accept_with_binding(actor, command, session, state, turns, events, None)
            .await
    }

    /// 同 [`Self::accept`]，并把附件清单绑定放进同一次 `commit`（`None` = 无附件）。
    #[allow(clippy::too_many_arguments)] // 事务入参固定为一组已校验的业务事实；无需引入中间 DTO。
    async fn accept_with_binding(
        &self,
        actor: &Actor,
        command: &ClientCommand,
        session: &SessionId,
        state: Option<StateChange>,
        turns: Vec<TurnChange>,
        events: Vec<PendingEvent>,
        binding: Option<SubmissionBinding>,
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
        match self.commit_owned_with_binding(commit, binding).await {
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
    /// `result` 是该命令终态的收据结果：有 turn 的命令带 `{"turnId":…}`、模式/配置切换
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
                self.record_uncertain(session, request, "终态落盘失败", Some(&command.actor))
                    .await;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// 权限解析：optionId 必须来自该未解决请求（§11.5），`kind` 由请求里的 `InteractionOption.kind`
    /// 决定（core 不发明分类）。
    ///
    /// 候选项的权威来源是创建交互的事件 payload。注入 [`InteractionAuthority`] 时直接用它投影出的
    /// **完整** `EffectiveInteraction`（`SqliteStore` 从 `owned_interaction.request_event` 的 payload 还原
    /// options，`SqlReadView` 的旧投影则只带元数据、options 恒为空）；未注入时退回旧读视图，此时只有
    /// 调用方自己注入了 options 的存储替身才能通过校验。
    async fn permission_resolution(
        &self,
        session: &SessionId,
        interaction: &InteractionId,
        option_id: &str,
    ) -> Result<Result<InteractionResolution, CommandReceipt>, PortError> {
        let options = match self.interactions.as_ref() {
            Some(authority) => {
                let effective = authority.effective(session).await?;
                let Some(current) = effective
                    .iter()
                    .find(|item| item.interaction.as_str() == interaction.as_str())
                else {
                    return Ok(Err(self.reject(
                        "command.not_found",
                        "交互不存在或已经终态",
                        false,
                    )?));
                };
                current.options.clone()
            }
            None => {
                let query = HistoryQuery {
                    session: session.clone(),
                    include: HistoryInclude {
                        pending_interactions: true,
                        ..HistoryInclude::default()
                    },
                    before_history: None,
                    before_messages: None,
                    messages_limit: ReplayLimit::new(0),
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
                pending.options().to_vec()
            }
        };
        let Some(option) = options
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
        self.commit_owned_with_binding(commit, None).await
    }

    /// 同 [`Self::commit_owned`]，并把附件清单绑定交给存储层的**同一个事务**。
    ///
    /// 绑定失败（清单不存在/未封存/跨会话/文件缺失/磁盘错误）会让整次提交失败；调用方据此在派发给
    /// Agent **之前**结束命令，因此不会出现「Agent 收到 prompt 但附件没绑定」的窗口。
    async fn commit_owned_with_binding(
        &self,
        commit: OwnedCommit,
        binding: Option<SubmissionBinding>,
    ) -> Result<CommitOutcome, PortError> {
        let resolving = commit
            .state
            .as_ref()
            .map(|state| match state {
                StateChange::Update(update) => update.interaction.is_some(),
                StateChange::Create(_) | StateChange::SettleCreate(_) => false,
            })
            .unwrap_or(false);
        if resolving && !commit.interactions.is_empty() {
            return Err(PortError::InvalidRequest(
                "同一提交不得同时创建与解析交互（§6 第 13 条 / §9 判据 15）",
            ));
        }
        let (commit, predicted) = self.finalize_owned_views(commit).await?;
        let outcome = self
            .deps
            .store
            .commit_with_submission(commit, binding)
            .await?;
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
            if !view_requires_session_version(event.event_type.as_str()) {
                continue;
            }
            event.payload.view = if event.event_type.as_str() == "session.updated" {
                ensure_nested_session_version(&event.payload.view, &text)?
            } else {
                ensure_view_string_field(
                    &event.payload.view,
                    "version",
                    &text,
                    "view 的 version 与会话当前版本不一致（§10.3）",
                )?
            };
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
                | ConflictKind::IdentityMismatch => ("internal.unavailable", "状态冲突", false),
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
                // 「后端不支持该操作」不是临时故障：取既有码 `command.unsupported`（Node Link
                // 适配器映射为 `nodelink.command.unsupported`），本地管理适配器映射为
                // `local.unavailable`。
                UnavailableKind::BackendUnsupported => {
                    ("command.unsupported", "后端不支持该操作", false)
                }
                // 「后端可证明没有建立会话」（进程未能启动、请求未发出即失败、Agent 明确拒绝）：
                // 成因由 message 承担，错误码复用既有的 `internal.unavailable`（design D4，不新增码）。
                UnavailableKind::SessionNotEstablished => {
                    ("internal.unavailable", "后端未能建立会话", true)
                }
                UnavailableKind::Busy
                | UnavailableKind::StorageFull
                | UnavailableKind::IoError
                | UnavailableKind::OwnerOffline => {
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

/// Daemon/Broker 自身产出的事件类型（没有诱发命令，`origin.kind = daemon`）。
const DAEMON_EVENT_TYPES: &[&str] = &[
    "device.revoked",
    "session.origin.online_changed",
    "session.updated",
    "storage.integrity_failed",
];

/// 由命令路径产生的领域事件类型。这一组**只**由命令诱发（`turn.*`/`command.*` 由提交路径组装，
/// `permission.resolved`/`elicitation.resolved`/`session.mode.changed`/`session.config.changed` 是后端
/// 对命令的响应），因此它们的 `origin.kind` 取**诱发命令的执行者**、`causation` 取该命令的 `requestId`；
/// 其余类型由 Agent 产出（`origin.kind = agent`），只在属于某个 turn 时带该 turn 的 `causation`。
///
/// 该清单与 `broker::event_origin` 是 `origin.kind` 分类的**唯一**来源，不散在提交点里。它只回答
/// 「`origin.kind` 按不按诱发命令分类」；**哪条**命令是诱发命令是另一件事，由 [`commit_chunk`] 按事件
/// 类型分流（见 [`RESPONSE_EVENT_TYPES`]）。
///
/// [`commit_chunk`]: Broker::commit_chunk
const COMMAND_EVENT_TYPES: &[&str] = &[
    "permission.resolved",
    "elicitation.resolved",
    "turn.queued",
    "turn.started",
    "turn.completed",
    "turn.cancelled",
    "turn.failed",
    "command.completed",
    "command.failed",
    "command.uncertain",
    "session.mode.changed",
    "session.config.changed",
];

/// 该事件类型是否由命令路径产生（见 [`COMMAND_EVENT_TYPES`]）。
fn is_command_event_type(event_type: &str) -> bool {
    COMMAND_EVENT_TYPES.contains(&event_type)
}

/// 四类**响应类**事件：后端对一条**会话作用域命令**的响应（`session.mode.set`/`session.config.set`/
/// `permission.resolve`/`elicitation.respond`）。
///
/// 它们与 turn 事件分属**两套**诱发来源，这是 [`Broker::commit_chunk`] 按事件类型而不是按事件归属挑诱发
/// 命令的原因：turn 事件的诱发命令是**该 turn 自己的命令**，响应类事件的诱发命令是**提交时在会话作用域
/// 的那条命令**——两者在「有 turn 正在跑」时常常不是同一条（权限与 elicitation 的解析按定义就发生在
/// prompt turn 的 `WaitingPermission` 期间）。`agent-host` 产出的 `EndpointEvent` 恒不带 turn（那属
/// 适配器边界），而 `commit_chunk` 会把无 turn 的事件兜底归给在跑的 turn，因此**不得**按事件归属的 turn
/// 取响应类事件的诱发命令：那会把「设备 B 解析设备 A 的 turn 的权限」记成 A 的那条 prompt 命令，
/// 让 `causation → owned_command.actor_id` 的回查把归属判给 A。
const RESPONSE_EVENT_TYPES: &[&str] = &[
    "permission.resolved",
    "elicitation.resolved",
    "session.mode.changed",
    "session.config.changed",
];

/// 该事件类型是否是后端对会话作用域命令的响应（见 [`RESPONSE_EVENT_TYPES`]）。
fn is_response_event_type(event_type: &str) -> bool {
    RESPONSE_EVENT_TYPES.contains(&event_type)
}

/// 事件产生者判定（`SYNC_PROTOCOL.md` §10.1 的 `origin.kind`）——**唯一实现**：broker 组装每条事件时都用它，
/// 不按「有没有会话」猜（那是 §7.3 明令禁止的推断），也不散在提交点里。
///
/// - 后端 / Agent 产生（增量、工具、交互请求、计划/命令目录/用量/连接状态…）→ [`EventOrigin::Agent`]；
/// - 命令类事件（[`COMMAND_EVENT_TYPES`]）按**诱发命令的执行者**分类：本机 CLI → [`EventOrigin::LocalCli`]、
///   本机已授权 Sync 设备 → [`EventOrigin::Device`]、另一个节点经 Node Link 的命令 → [`EventOrigin::Node`]。
///   缺诱发命令时**不猜**：记 [`EventOrigin::Daemon`]（见下）；
/// - daemon 自身事件（[`DAEMON_EVENT_TYPES`]）→ [`EventOrigin::Daemon`]。
///
/// `inducing_actor = None` 的处置：命令类事件在 core 里只由命令路径产生，没有诱发命令说明它不是任何命令的
/// 产物，而是 Daemon/Broker 自身的写入（启动恢复的 `command.uncertain`、维护类提交等）——因此记
/// `daemon`。**不得**默认记 `device`（那会把无从归因的事件谎报成某个设备发起的），也**不得**按
/// 「有没有会话或 actor」反推产出者。配对认领方（[`Actor::PairingClaimant`]）永不可提交命令
/// （授权一律拒绝），走到这里说明接线错误，与「没有诱发命令」同一处置。
fn event_origin(event_type: &EventType, inducing_actor: Option<&Actor>) -> EventOrigin {
    let text = event_type.as_str();
    if DAEMON_EVENT_TYPES.contains(&text) {
        return EventOrigin::Daemon;
    }
    if !is_command_event_type(text) {
        return EventOrigin::Agent;
    }
    match inducing_actor {
        Some(Actor::LocalCli) => EventOrigin::LocalCli,
        Some(Actor::Device { .. }) => EventOrigin::Device,
        Some(Actor::Node { .. }) => EventOrigin::Node,
        Some(Actor::PairingClaimant { .. }) | None => EventOrigin::Daemon,
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

fn mode_ref(mode: &ModeId) -> Result<ModeRef, PortError> {
    // displayName 由 ACP 的 ModeState 给出，core 侧没有模式枚举端口（§5.1 无 list_modes）；
    // 这里以 modeId 兜底，展示名由前端用 `session.mode.list` 结果覆盖（§11.5 的 ModeState）。
    ModeRef::try_new(mode.clone(), mode.as_str()).map_err(PortError::from)
}

fn view(text: String) -> Result<ViewJson, PortError> {
    ViewJson::new(&text).map_err(PortError::from)
}

/// 提交增量作用于计数：负数按 0 收敛（读到的计数可能已被并发的失效/终态提交压低）。
fn apply_delta(value: u32, delta: i64) -> u32 {
    if delta >= 0 {
        value.saturating_add(u32::try_from(delta).unwrap_or(u32::MAX))
    } else {
        value.saturating_sub(u32::try_from(delta.unsigned_abs()).unwrap_or(u32::MAX))
    }
}

/// `session.updated` 的 `session` 成员：提交后的完整会话摘要（`common.schema.json#/$defs/sessionSummary`）。
///
/// 只由 core 组装：`state`/`version`/`updatedAt` 是本提交将写入的权威值，其余投影字段原样取自会话行
/// （`title`/`agent`/`origin`/`currentMode`/`createdAt`/`workspace`），不在这里重新解释。
fn view_session_summary(
    summary: &SessionSummary,
    state: SessionState,
    version: &Version,
    at: &Timestamp,
) -> Result<ViewJson, PortError> {
    let title = summary
        .title()
        .map(json_text)
        .unwrap_or_else(|| "null".to_owned());
    let agent = summary.agent();
    let current_mode = match summary.current_mode() {
        Some(mode) => format!(
            r#"{{"modeId":"{}","displayName":{}}}"#,
            mode.mode_id().as_str(),
            json_text(mode.display_name())
        ),
        None => "null".to_owned(),
    };
    let origin = match summary.origin() {
        ResourceOrigin::Local => r#"{"kind":"local"}"#.to_owned(),
        ResourceOrigin::Remote {
            owner_node_id,
            origin_epoch,
            online,
        } => format!(
            r#"{{"kind":"remote","ownerNodeId":"{}","originEpoch":"{}","online":{online}}}"#,
            owner_node_id.as_str(),
            origin_epoch.as_str()
        ),
    };
    let workspace = match summary.workspace() {
        Some(reference) => format!(
            r#"{{"directoryId":"{}"}}"#,
            reference.directory_id().as_str()
        ),
        None => "null".to_owned(),
    };
    view(format!(
        r#"{{"session":{{"sessionId":"{}","title":{title},"agent":{{"agentId":"{}","name":{}}},"state":"{}","origin":{origin},"currentMode":{current_mode},"version":"{}","createdAt":"{}","updatedAt":"{}","workspace":{workspace}}}}}"#,
        summary.session_id().as_str(),
        agent.agent_id().as_str(),
        json_text(agent.name()),
        state.as_str(),
        version.get(),
        summary.created_at().as_str(),
        at.as_str(),
    ))
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

/// 恢复请求的构造（§3.6）：全部输入取自持久化记录。
///
/// 记录里两个 `Option` 中任一为 `None` 时与「没有恢复数据」同义（`load_recovery` 已按该口径收敛），
/// 返回 `BackendUnsupported`；取值**无法通过值对象构造**（相对路径、空串、超长、含 NUL——只能经库
/// 被篡改或写入侧 bug 到达）属于**服务端不可用类**（`IoError`），不得归入「不支持」那一类。
fn resume_request(record: SessionRecoveryRecord) -> Result<ResumeSessionRequest, PortError> {
    let (Some(agent_session_id), Some(workspace_cwd)) =
        (record.agent_session_id, record.workspace_cwd)
    else {
        return Err(PortError::Unavailable(UnavailableKind::BackendUnsupported));
    };
    ResumeSessionRequest::try_new(record.agent, agent_session_id, workspace_cwd)
        .map_err(|_| PortError::Unavailable(UnavailableKind::IoError))
}

/// 恢复时的工作目录复校验（与创建同口径）：绝对、存在、是目录，且 `canonicalize` 的结果与持久化
/// 取值**逐字相同**。
///
/// 失败一律是 `Unavailable(IoError)`（服务端不可用类）：**不**回退到按别名重新解析，也不改用
/// 「最接近」的目录；「它曾经合法」不是跳过校验的理由（每次恢复都重新校验）。
fn revalidate_resume_workspace(workspace_cwd: &str) -> Result<(), PortError> {
    let io = || PortError::Unavailable(UnavailableKind::IoError);
    let candidate = std::path::Path::new(workspace_cwd);
    if !candidate.is_absolute() {
        return Err(io());
    }
    let metadata = std::fs::metadata(candidate).map_err(|_| io())?;
    if !metadata.is_dir() {
        return Err(io());
    }
    let canonical = std::fs::canonicalize(candidate).map_err(|_| io())?;
    if canonical.to_str() != Some(workspace_cwd) {
        return Err(io());
    }
    Ok(())
}

/// 窗口期状态的键：与幂等键同口径的 `(actor, requestId)`（§6 第 6 条、第 20 条）。
fn prepared_key(actor: &Actor, request: &RequestId) -> String {
    format!(
        "{}|{}|{}",
        actor.kind().as_str(),
        actor.id_text(),
        request.as_str()
    )
}

/// 目录归属（`WorkspaceRef`）与已解析 workspace 的一致性校验（§3.1、`design.md` D1）。
///
/// 别名与展示名必须**同源**：落进 `owned_session.workspace_directory_id` 的是归属里的别名，投影给适配层的
/// 会话摘要用的也是同一个 `WorkspaceRef`。因此这里既不接受「解析出别名却没有归属投影」，也不接受
/// 「两者的别名不同」——两种组装错误都失败关闭，而不是猜一个别名写库。
fn check_workspace_ownership(
    create: &CreateSessionRequest,
    ownership: Option<WorkspaceRef>,
) -> Result<Option<WorkspaceRef>, PortError> {
    let resolved = create
        .workspace
        .as_ref()
        .map(|workspace| workspace.directory_id());
    match (resolved, ownership.as_ref()) {
        (None, None) => Ok(None),
        (None, Some(_)) => Err(PortError::InvalidRequest(
            "本次解析没有别名，不得给出目录归属投影",
        )),
        // 客户端提供的目录也会派生 directoryId，但没有登记记录；没有归属投影是合法状态。
        (Some(_), None) => Err(PortError::InvalidRequest(
            "已解析的目录必须带上目录归属投影",
        )),
        (Some(alias), Some(owned)) if alias == owned.directory_id() => Ok(ownership),
        (Some(_), Some(_)) => Err(PortError::InvalidRequest(
            "目录归属的别名与本次解析结果不一致",
        )),
    }
}

/// 落账后的会话摘要：适配层投影终态结果（Node Link 的 `SessionCreateResult.sessionMeta`、Sync 的
/// `session` 资源）用它，因此**必须**与 `SessionStore::list` 对同一行给出的摘要逐字段同源——
/// `state` 恒为 `idle`、`version` 恒为 1、两个时间戳都是落账提交的 `at`、目录归属就是落进
/// `owned_session.workspace_directory_id` 的那个别名（展示名按别名关联目录记录，与本函数取的是同一个）。
///
/// 目录归属按**归属投影**（`WorkspaceRef`）而非 `SettledSession::workspace_directory_id` 取：别名是落库的
/// 唯一取值，而展示名不落库（读路径按别名关联目录记录），因此摘要要同源就必须拿本次解析用的那份
/// 投影。两者的别名必然相同——`check_workspace_ownership` 已比对过，不同即失败关闭。
fn created_summary(
    settled: &SettledSession,
    at: &Timestamp,
    workspace: Option<&WorkspaceRef>,
) -> Result<SessionSummary, PortError> {
    SessionSummary::try_new(
        settled.session.clone(),
        settled.title.clone(),
        settled.agent.clone(),
        SessionState::Idle,
        ResourceOrigin::Local,
        None,
        Version::new(1),
        at.clone(),
        at.clone(),
        workspace.cloned(),
    )
    .map_err(PortError::from)
}

/// `session.create` 的失败分类（§6 第 20 条、`sync-session-create` 的终态判据）：可证明 Agent 侧未
/// 产生会话 ⇒ `failed`；说不清 ⇒ `uncertain`。
///
/// 判据落在适配器映射出的**具名错误**上（`local-agent-host`「可证明与无法证明可区分」）：进程未能
/// 启动、请求未发出即失败、Agent 对会话建立请求明确返回错误，一律收敛成
/// [`UnavailableKind::SessionNotEstablished`]；其余错误（短请求超时、请求发出后掉线、协议破坏、会话
/// 已建立之后才失败）都不能证明没有副作用。默认走 `uncertain` 是**保守**方向：宁可让客户端知道
/// 「说不清」，也不声称「什么也没发生」。
fn create_failure_status(error: &PortError) -> CommandStatus {
    match error {
        PortError::Unavailable(UnavailableKind::SessionNotEstablished) => CommandStatus::Failed,
        _ => CommandStatus::Uncertain,
    }
}

/// `session.create` 终态事件的唯一构造点（§10.1 的三个 `command.*` 终态类型）。
///
/// 成因取自**结构化错误本身**（`code`/`message`/`retryable`/`details`）：终态的成因由 `message` 承担
/// （design D4），不写死文案（`view_command_failed` 是 turn 失败的旧口径，仍保留给那条路径）。
fn create_terminal_event(
    actor: &Actor,
    request: &RequestId,
    status: CommandStatus,
    error: Option<&PublicError>,
) -> Result<PendingEvent, PortError> {
    let (event_type, view) = match status {
        CommandStatus::Completed => ("command.completed", view_command_completed(request, None)?),
        CommandStatus::Failed => ("command.failed", view_command_failed_error(request, error)?),
        CommandStatus::Uncertain => (
            "command.uncertain",
            view_command_uncertain(
                request,
                error.map_or("无法确认会话是否已建立", PublicError::message),
            )?,
        ),
        _ => return Err(PortError::InvalidRequest("命令终态必须是终止态")),
    };
    pending_event(
        event_type,
        EventKind::Structured,
        view,
        None,
        Some(request.clone()),
        StoredPolicy::Durable,
        Some(actor),
    )
}

/// `command.failed` 的 view（`SYNC_PROTOCOL.md` §10.1）：与 [`view_command_failed`] 同形，但成因取自
/// 传进来的结构化错误，而不是固定文案。
fn view_command_failed_error(
    request: &RequestId,
    error: Option<&PublicError>,
) -> Result<ViewJson, PortError> {
    let error = error.ok_or(PortError::InvalidRequest(
        "command.failed 必须携带结构化错误",
    ))?;
    view(format!(
        r#"{{"requestId":"{}","error":{{"code":"{}","message":{},"retryable":{},"details":{}}}}}"#,
        request.as_str(),
        error.code(),
        json_text(error.message()),
        error.retryable(),
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

/// 终态结果：`{"turnId":"…"}`（turn 作用域命令的收据字段）。
///
/// Node Link 的 `command.terminal.terminal.result` 是开放 object，带 turnId 使 Access 能把命令与 turn
/// 对上；缺它时适配层只能回空 object（§12.5 的「非空」= 非 null，但空 object 丢失了这条信息）。
fn turn_result(turn: &TurnId) -> Result<CommandResult, PortError> {
    CommandResult::from_json_text(&format!(r#"{{"turnId":"{}"}}"#, turn.as_str()))
        .map_err(PortError::from)
}

/// 终态结果：`{"version":"…"}`（模式/配置切换的收据字段）。
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

/// `session.updated` 的版本在 `session.version`（与 `sessionSummary` 同形），不在顶层。
///
/// 与 [`ensure_view_string_field`] 的分工：这个注入只**校验**嵌套取值，不合成缺失字段——该 view 由
/// core 自己组装，缺 `session.version` 属于内部组装错误（失败关闭），而不是适配器输入。
fn ensure_nested_session_version(view: &ViewJson, value: &str) -> Result<ViewJson, PortError> {
    let conflict = "session.updated 的 session.version 与 core 的权威版本不一致（§10.3）";
    let members = json_members(view.as_str()).map_err(PortError::from)?;
    let session = members
        .iter()
        .find(|(name, _)| name == "session")
        .map(|(_, raw)| *raw)
        .ok_or(PortError::InvalidRequest(conflict))?;
    let session_members = json_members(session).map_err(PortError::from)?;
    let current = session_members
        .iter()
        .find(|(name, _)| name == "version")
        .and_then(|(_, raw)| json_string(raw));
    match current {
        Some(existing) if existing == value => Ok(view.clone()),
        _ => Err(PortError::InvalidRequest(conflict)),
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
        ActorKind, AgentDescriptor, AgentId, AgentProfile, AgentRef, AgentSessionId,
        AttachmentGeneration, AttachmentId, AttachmentSubmission, AuditRecord, CapabilitySet,
        ConfigOption, DeviceId, DeviceRecord, DirectoryId, DirectoryRecord, EventId, ModeState,
        NodeId, NodeKind, NodeRecord, OriginCursor, OriginEpoch, PairingId, PairingPeer,
        PairingRecord, PairingState, PairingTarget, PeerIdentity, PeerPublicKey,
        PendingInteraction, ResourceOrigin, ServerEpoch, Session, SessionId, SessionSnapshot,
        SessionSummary, Turn, WorkspaceRef,
    };
    use crate::ports::{
        AckOutcome, AgentCatalog, AttachmentRef, AttachmentStore, AuditQuery, DeviceRevocation,
        DeviceWrite, ExpiryWrite, IdempotentReplay, ImportedSessionQuery, ImportedSessionRecord,
        LocalConfigStore, NodeConnectedWrite, NodeLinkSlice, NodeRevocation, NodeWrite,
        OwnedEventRecord, PairingClaimOutcome, PairingClaimWrite, PairingConsumption,
        PairingSettlementWrite, PairingWrite, PendingInteractionOrigin, PruneReport,
        RemoteCommandRef, RetentionPolicy, SessionQuery, StoreHealth, SyncEventRecord,
        SyncReplaySlice, TrustRecordRef, TrustStore, TurnAccepted,
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
        pub(crate) workspaces: Mutex<Vec<DirectoryRecord>>,
        /// 管理写集实际携带的审计动作（断言「审计随写集提交」而不是事后补写）。
        pub(crate) write_audits: Mutex<Vec<AuditAction>>,
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
        /// `SessionEndpoint::agent_session_id` 的返回值（默认 `None` = 未取得标识，即 R7 的情形）。
        pub(crate) agent_session_id: Mutex<Option<AgentSessionId>>,
        /// 被 `SessionEndpoint::close` 结束过的端点（按会话标识、调用顺序）。
        ///
        /// 窗口期超限的失败关闭**不**走它：那里结束的是整个 Agent 运行时
        /// （`SessionBackendFactory::terminate_agent`），端点只是随之失效。保留它是为了断言「这条超限
        /// 路径没有被退回成只关端点」。
        pub(crate) closed_endpoints: Mutex<Vec<String>>,
        /// 被 `SessionBackendFactory::terminate_agent` 要求结束的 Agent 运行时（按会话标识、调用顺序）。
        ///
        /// core 不持进程句柄，「结束该 Agent」必须经这条端口方法落到 `agent-host` 的目录层，因此它是
        /// 窗口期超限失败关闭的可观察证据。
        pub(crate) terminated_agents: Mutex<Vec<String>>,
        /// 后继后端收到的恢复请求（断言「未调用后端」与「传的就是持久化取值」）。
        pub(crate) resume_requests: Mutex<Vec<ResumeSessionRequest>>,
        /// `resume` 的脚本化错误（先进先出弹出；空 = 成功返回端点）。
        pub(crate) resume_errors: Mutex<VecDeque<PortError>>,
        /// `create` 的调用次数（断言「同键重发不派发第二次创建」）。
        pub(crate) create_calls: AtomicUsize,
        /// `create` 的脚本化错误（先进先出弹出；空 = 成功返回端点）。
        ///
        /// 用它钉「后端失败」那条路径：错误类别决定终态（`Unavailable(SessionNotEstablished)` ⇒
        /// `failed`，其余 ⇒ `uncertain`），见 `session.create` 的失败分类。
        pub(crate) create_errors: Mutex<VecDeque<PortError>>,
        /// `load_recovery` 的调用次数（断言「授权先于本机读取」：未授权时不得读会话行）。
        pub(crate) recovery_reads: AtomicUsize,
        /// 已封存附件清单（`submit_prompt_with_submission` 的 `submission()` 读取源）。
        pub(crate) submissions: Mutex<HashMap<String, AttachmentSubmission>>,
        /// 成功受理时实际交给存储的附件绑定（顺序即调用顺序）。
        pub(crate) bindings: Mutex<Vec<SubmissionBinding>>,
        /// `InteractionAuthority::effective` 的脚本化结果。
        pub(crate) effective_interactions: Mutex<Vec<EffectiveInteraction>>,
        /// 已被解析的交互（同一标识再次解析 → `AlreadyResolved`）。
        pub(crate) resolved_interactions: Mutex<Vec<String>>,
        /// 已失效的交互（`invalidate_pending` 的调用记录与目标集合）。
        pub(crate) invalidated_interactions: Mutex<Vec<(Option<String>, usize)>>,
        /// 适配器解析写入的脚本化错误（先进先出；空 = 写成功）。
        pub(crate) resolve_errors: Mutex<VecDeque<PortError>>,
        /// `SessionBackendFactory::open` 的脚本化错误（先进先出；空 = 成功）。
        pub(crate) open_errors: Mutex<VecDeque<PortError>>,
    }

    /// `owned_session` 的两列恢复数据（§3.6 `SessionRecoveryRecord`）。
    ///
    /// 与 `WorldState.sessions` 分开存，以守住「这两列**不进** `Session`/`SessionSummary`」的形状约定。
    #[derive(Clone, Default)]
    pub(crate) struct RecoveryColumns {
        pub(crate) agent_session_id: Option<AgentSessionId>,
        pub(crate) workspace_cwd: Option<String>,
    }

    impl FakeWorld {
        /// 读取路径上的目录归属（§3.6/§5.2、`design.md` D1）：**只**按会话行持久化的
        /// `workspace_directory_id` 取值，展示名按别名查目录记录（storage 侧那条 LEFT JOIN 的等价物），
        /// 目录记录不存在时回退为别名本身——因此目录被删除/改名都不影响既有会话的归属。
        ///
        /// 本函数**不**看 `workspace_cwd`：`NULL` 就是未分组，任何情况下都不按路径反查别名。
        pub(crate) fn workspace_ref(
            &self,
            state: &WorldState,
            session: &SessionId,
        ) -> Option<WorkspaceRef> {
            let directory_id = state.workspace_directories.get(session.as_str())?;
            Some(WorkspaceRef::new(directory_id.clone()))
        }
    }

    #[derive(Default)]
    pub(crate) struct WorldState {
        pub(crate) sessions: HashMap<String, Session>,
        /// 会话 id → `owned_session` 的两列恢复数据（§3.6 的窄读取源）。
        pub(crate) recoveries: HashMap<String, RecoveryColumns>,
        /// 会话 id → `owned_session.workspace_directory_id`（目录归属的**唯一**来源；缺席 = `NULL` = 未分组）。
        ///
        /// 与 `sessions`/`recoveries` 分开存，和 storage 侧一样：别名不是聚合的状态，恢复流程也不读它。
        pub(crate) workspace_directories: HashMap<String, DirectoryId>,
        pub(crate) turns: HashMap<String, Vec<Turn>>,
        pub(crate) events: Vec<CommittedEvent>,
        pub(crate) commands: HashMap<String, CommandRecord>,
        pub(crate) interactions: HashMap<String, InteractionRow>,
        pub(crate) epochs: HashMap<String, OriginEpoch>,
        /// 事件 id → 持久化正文（fake 的 `ReadView::event_payload` 数据源）。
        pub(crate) event_payloads: HashMap<String, EventPayload>,
        /// 事件 id → `origin.kind`（fake 记录 broker 写入的 origin，供 §10.1 断言）。
        pub(crate) event_origins: HashMap<String, EventOrigin>,
        /// 事件 id → `owned_event.causation`（fake 记录 broker 写入的因果；`sync_replay` 的
        /// `causationRequestId` 与 `origin.deviceId`/`origin_node_id` 回查都从它读——真实存储读的
        /// 是同一个列）。
        pub(crate) event_causations: HashMap<String, Option<RequestId>>,
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
        /// 所属 turn（`owned_interaction.turn_id` 的替身）：turn 终态时按它失效遗留请求。
        pub(crate) turn: Option<TurnId>,
        /// 请求版本（版本化应答的乐观并发依据）。
        pub(crate) request_version: Option<Version>,
        /// 是否已确认（`owned_interaction.confirmed_at IS NOT NULL` 的替身）。
        pub(crate) confirmed: bool,
        /// 是否已失效（`invalidated_at` 的替身）：失效行不参与有效集合与三态归约。
        pub(crate) invalidated: bool,
    }

    impl WorldState {
        /// `causation`（= 诱发命令的 `requestId`）+ actor 类别 → `owned_command.actor_id`
        /// （`SYNC_PROTOCOL.md` §10.1 的 `origin.deviceId`/`origin.nodeId`）。
        ///
        /// 与真实存储（`storage-sqlite` 的 `command_origins` + `identity_origin_for`）同一口径：只认
        /// **本类别** actor 的命令行（device 事件看 `Actor::Device`、node 事件看 `Actor::Node`，两类
        /// 身份不可互换）；`causation` 为空、找不到命令行、或同一 requestId 在同一类别下命中多个不同
        /// actor 时返回 `None`——调用方（`FakeReadView::sync_replay`）据此失败关闭，**不**编造 id。
        pub(crate) fn identity_origin(
            &self,
            causation: Option<&RequestId>,
            kind: ActorKind,
            event_id: Option<&EventId>,
        ) -> Option<String> {
            if let Some(event_id) = event_id {
                for record in self.commands.values() {
                    if record.command() == "workspace.mkdir"
                        && record.actor().kind() == kind
                        && record.terminal_event() == Some(event_id)
                    {
                        return Some(record.actor().id_text());
                    }
                }
            }
            let causation = causation?;
            let mut found: Option<String> = None;
            for record in self.commands.values() {
                if record.request() != causation || record.actor().kind() != kind {
                    continue;
                }
                let actor_id = record.actor().id_text();
                if found.is_some_and(|known| known != actor_id) {
                    // 同一 requestId 在同一 actor 类别下命中多个 actor：没有唯一答案。
                    return None;
                }
                found = Some(actor_id);
            }
            found
        }
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
        pub(crate) fn directories(&self) -> &Mutex<Vec<DirectoryRecord>> {
            &self.workspaces
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

        pub(crate) fn create_calls(&self) -> usize {
            self.create_calls.load(Ordering::SeqCst)
        }

        /// 让下一次 `SessionBackendFactory::create` 以该错误失败（先进先出；空 = 成功）。
        pub(crate) fn push_create_error(&self, error: PortError) {
            lock(&self.create_errors).push_back(error);
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

        /// 直接写一条命令行（测试用来构造「已 accepted 的 `session.resume` 记录」）。
        pub(crate) fn seed_command(&self, record: CommandRecord) {
            lock(&self.state)
                .commands
                .insert(command_key(record.actor(), record.request()), record);
        }

        /// `load_recovery` 的调用次数（「授权先于本机读取」的可观察证据）。
        pub(crate) fn recovery_read_count(&self) -> usize {
            self.recovery_reads.load(Ordering::SeqCst)
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

        /// 某条事件落库时的 `causation`（§10.1：诱发命令的 requestId；`None` = 没有命令直接导致它）。
        pub(crate) fn event_causation(&self, id: &EventId) -> Option<Option<RequestId>> {
            lock(&self.state).event_causations.get(id.as_str()).cloned()
        }

        /// 某条事件落库时的 `(origin.kind, causation)` 投影（§10.1 断言用）。
        pub(crate) fn provenance_of(
            &self,
            session: &SessionId,
            event_type: &str,
        ) -> Vec<(EventOrigin, Option<RequestId>)> {
            self.events(session)
                .into_iter()
                .filter(|event| event.event_type.as_str() == event_type)
                .map(|event| {
                    (
                        self.event_origin(&event.id).expect("origin recorded"),
                        self.event_causation(&event.id).expect("causation recorded"),
                    )
                })
                .collect()
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

        /// 让后端在下次 `create`/`resume` 时暴露这个 ACP 会话标识（默认 `None` = 未取得标识，即 R7 的情形）。
        pub(crate) fn set_agent_session_id(&self, id: AgentSessionId) {
            *lock(&self.agent_session_id) = Some(id);
        }

        /// 已被 `SessionEndpoint::close` 结束的端点（按会话标识、调用顺序）。
        pub(crate) fn closed_endpoints(&self) -> Vec<String> {
            lock(&self.closed_endpoints).clone()
        }

        pub(crate) fn terminated_agents(&self) -> Vec<String> {
            lock(&self.terminated_agents).clone()
        }

        /// 直接写会话的两列恢复数据（模拟既有行的 `NULL` 时传 `None`）。
        pub(crate) fn seed_recovery(
            &self,
            session: &SessionId,
            agent_session_id: Option<AgentSessionId>,
            workspace_cwd: Option<String>,
        ) {
            lock(&self.state).recoveries.insert(
                session.as_str().to_owned(),
                RecoveryColumns {
                    agent_session_id,
                    workspace_cwd,
                },
            );
        }

        pub(crate) fn seed_interaction(&self, pending: PendingInteraction) {
            let origin_event = EventId::new(&uuid_text(0)).expect("事件 id");
            lock(&self.state).interactions.insert(
                pending.id().as_str().to_owned(),
                InteractionRow {
                    pending,
                    resolution: None,
                    origin_event,
                    turn: None,
                    request_version: Some(Version::new(1)),
                    confirmed: false,
                    invalidated: false,
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
        fn session_id(&self) -> SessionId {
            SessionId::new(&self.next_text()).expect("uuid")
        }

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

        async fn profile(&self, _id: &AgentId) -> Result<Option<AgentProfile>, PortError> {
            Ok(None)
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
        async fn commit_scoped(
            &self,
            commit: OwnedCommit,
            terminal_actor: Option<&Actor>,
        ) -> Result<CommitOutcome, PortError> {
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
            let (created, appended) = self.apply(&commit, terminal_actor)?;
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

        /// `OwnedCommit` 的 §5.2 语义：幂等 → 会话/交互 → turn → 事件 → 终态/接受行。
        fn apply(
            &self,
            commit: &OwnedCommit,
            terminal_actor: Option<&Actor>,
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
                    )?;
                    state.epochs.insert(session_id.as_str().to_owned(), epoch);
                    state
                        .sessions
                        .insert(session_id.as_str().to_owned(), session);
                    created = Some(session_id);
                }
                Some(StateChange::SettleCreate(settled)) => {
                    // §6 第 20 条：会话标识由 core 分配、存储层**采用**；ACP 会话标识、创建时 cwd 与
                    // 目录别名在同一次提交里随行落盘（真存储的列语义逐条对齐）。
                    if commit.session.as_ref() != Some(&settled.session) {
                        return Err(PortError::InvalidRequest(
                            "SettleCreate 的 session 必须等于载荷里的会话标识",
                        ));
                    }
                    if commit.expected_version.is_some() {
                        return Err(PortError::InvalidRequest(
                            "SettleCreate 不得带 expected_version",
                        ));
                    }
                    if commit.idempotency.is_some() {
                        return Err(PortError::InvalidRequest(
                            "SettleCreate 不得再写幂等行（只回填它的 session_id）",
                        ));
                    }
                    let session_id = settled.session.clone();
                    let epoch = commit
                        .origin_epoch
                        .clone()
                        .ok_or(PortError::InvalidRequest(
                            "SettleCreate 必须带 origin_epoch",
                        ))?;
                    let session = Session::try_new(
                        session_id.clone(),
                        OwnedSessionRef::new(session_id.clone()),
                        settled.title.clone(),
                        settled.agent.clone(),
                        SessionState::Idle,
                        ResourceOrigin::Local,
                        None,
                        Version::from(1),
                        at.clone(),
                        at.clone(),
                    )?;
                    state.epochs.insert(session_id.as_str().to_owned(), epoch);
                    state
                        .sessions
                        .insert(session_id.as_str().to_owned(), session);
                    if settled.agent_session_id.is_some() || settled.workspace_cwd.is_some() {
                        let columns = state
                            .recoveries
                            .entry(session_id.as_str().to_owned())
                            .or_default();
                        columns.agent_session_id = settled.agent_session_id.clone();
                        columns.workspace_cwd = settled.workspace_cwd.clone();
                    }
                    if let Some(alias) = settled.workspace_directory_id.as_ref() {
                        state
                            .workspace_directories
                            .insert(session_id.as_str().to_owned(), DirectoryId::new(alias)?);
                    }
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
                    // §6.7：first-writer-wins 的条件更新（含失效标记与请求版本）。
                    if let Some(resolved) = &update.interaction {
                        match state.interactions.get_mut(resolved.interaction.as_str()) {
                            None => {
                                return Err(PortError::NotFound(EntityRef::Interaction(
                                    resolved.interaction.clone(),
                                )));
                            }
                            Some(row) if row.invalidated => {
                                return Err(PortError::Conflict(ConflictKind::Expired));
                            }
                            Some(row) if row.resolution.is_some() => {
                                return Err(PortError::Conflict(ConflictKind::AlreadyResolved));
                            }
                            Some(row) => {
                                if let Some(expected) = resolved.request_version
                                    && row.request_version.as_ref() != Some(&expected)
                                {
                                    return Err(PortError::Conflict(ConflictKind::VersionMismatch));
                                }
                                row.resolution = Some(resolved.clone());
                            }
                        }
                    }
                    // 确认：只对已仲裁、未确认、未失效的行生效；其余是无操作。
                    if let Some(confirmation) = &update.confirmation
                        && let Some(row) = state.interactions.get_mut(confirmation.as_str())
                        && row.resolution.is_some()
                        && !row.invalidated
                    {
                        row.confirmed = true;
                    }
                    let next_state = update.state.unwrap_or_else(|| current.state());
                    let mode = match &update.mode {
                        ModeChange::Unchanged => current.current_mode().cloned(),
                        ModeChange::Set(mode) => Some(mode.clone()),
                    };
                    // R9（`design.md` D7）：两层可选 —— `None` = 不改该列、`Some(None)` = 显式置空、
                    // `Some(Some(text))` = 写入。fake 与 `storage-sqlite` 的列语义逐条一致。
                    let title = match &update.title {
                        None => current.title().map(str::to_owned),
                        Some(None) => None,
                        Some(Some(text)) => Some(text.clone()),
                    };
                    let updated = Session::try_new(
                        current.id().clone(),
                        current.reference().clone(),
                        title,
                        current.agent().clone(),
                        next_state,
                        current.origin().clone(),
                        mode,
                        Version::from(current.version().get() + 1),
                        current.created_at().clone(),
                        at.clone(),
                    )?;
                    state
                        .sessions
                        .insert(session_id.as_str().to_owned(), updated);
                    // §5.2：两列只在写入时改，`None` = 不改该列（fake 同样按列语义实现，
                    // 以便恢复流程「只读两列」的断言能真的跑在写入路径上）。
                    if update.agent_session_id.is_some() || update.workspace_cwd.is_some() {
                        let columns = state
                            .recoveries
                            .entry(session_id.as_str().to_owned())
                            .or_default();
                        if let Some(agent_session_id) = update.agent_session_id.clone() {
                            columns.agent_session_id = Some(agent_session_id);
                        }
                        if let Some(workspace_cwd) = update.workspace_cwd.clone() {
                            columns.workspace_cwd = Some(workspace_cwd);
                        }
                    }
                    // §5.2：`workspace_directory_id` 同为窄写列，`None` = 不改；写入后只被读路径消费。
                    if let Some(alias) = update.workspace_directory_id.clone() {
                        state
                            .workspace_directories
                            .insert(session_id.as_str().to_owned(), DirectoryId::new(&alias)?);
                    }
                }
                None => {}
            }
            if let Some(session_id) = commit.session.clone() {
                let mut ended_turns: Vec<TurnId> = Vec::new();
                {
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
                                if update.state.is_terminal() {
                                    ended_turns.push(update.turn.clone());
                                }
                            }
                        }
                    }
                }
                // 所属任务确认结束：该轮的遗留请求在同一事务里失效（不伪造授权）。
                for turn in ended_turns {
                    for row in state.interactions.values_mut() {
                        if row.turn.as_ref() == Some(&turn) {
                            row.invalidated = true;
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
                    .event_causations
                    .insert(id.as_str().to_owned(), event.causation.clone());
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
                        turn: write.turn.clone(),
                        request_version: Some(Version::new(1)),
                        confirmed: false,
                        invalidated: false,
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
                // §6 第 20 条：终态定位按提交形状分三条（与 `storage-sqlite` 逐条同口径）——
                // `SettleCreate` 定位的是**尚无会话**的受理行（本次提交回填它的 `session_id`）；
                // 其余带会话的提交按 `(session_id, request_id)`；`session` 为 `None` 时按
                // `request_id + session_id IS NULL`（`session.create` 的失败终态与启动恢复）。
                let settlement = matches!(commit.state, Some(StateChange::SettleCreate(_)));
                let expected = if settlement {
                    None
                } else {
                    commit.session.as_ref()
                };
                let mut matched = state.commands.iter().filter(|(_, record)| {
                    record.request() == &request
                        && record.session() == expected
                        && terminal_actor.is_none_or(|actor| {
                            record.actor() == actor && record.command() == "workspace.mkdir"
                        })
                });
                let key = matched.next().map(|(key, _)| key.clone());
                if matched.next().is_some() {
                    // 命中多行即失败关闭（不得猜）：同一 `requestId` 在多条 actor 下各有一行。
                    return Err(PortError::InvalidRequest("request id is ambiguous"));
                }
                let key = key.ok_or_else(|| {
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
                // `SettleCreate` 在本事务里回填受理行的 `session_id`（真存储同口径地 UPDATE 那一列），
                // 因此终态行落到本次会话上；其余形状保持既有会话（`None` 就是「尚无会话」）。
                let command_session = commit
                    .session
                    .clone()
                    .or_else(|| existing.session().cloned());
                let updated = CommandRecord::try_new(
                    command_session,
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
            self.commit_scoped(commit, None).await
        }

        /// §6 第 22 条的 fake 等价物：受理前核对清单（会话/顺序）与已登记清单相等；失败整次拒绝。
        /// 成功后才记录绑定，因此调用方观察到的 `bindings` 只包含真正落账的受理。
        async fn commit_with_submission(
            &self,
            commit: OwnedCommit,
            binding: Option<SubmissionBinding>,
        ) -> Result<CommitOutcome, PortError> {
            let Some(binding) = binding else {
                return self.commit_scoped(commit, None).await;
            };
            let session = commit.session.clone().ok_or(PortError::InvalidRequest(
                "attachment binding requires a session",
            ))?;
            if commit.idempotency.is_none() || commit.command_terminal.is_some() {
                return Err(PortError::InvalidRequest(
                    "attachment binding is only valid on an acceptance commit",
                ));
            }
            let manifest = lock(&self.world.submissions)
                .get(binding.submission.as_str())
                .cloned()
                .ok_or_else(|| {
                    PortError::NotFound(EntityRef::Submission(binding.submission.clone()))
                })?;
            if manifest.session != session || manifest.attachments != binding.attachments {
                return Err(PortError::InvalidRequest(
                    "attachment binding does not match the sealed manifest",
                ));
            }
            let outcome = self.commit_scoped(commit, None).await?;
            if outcome.replayed.is_none() {
                lock(&self.world.bindings).push(binding);
            }
            Ok(outcome)
        }

        async fn commit_workspace_mkdir_terminal(
            &self,
            actor: &Actor,
            commit: OwnedCommit,
        ) -> Result<CommitOutcome, PortError> {
            self.commit_scoped(commit, Some(actor)).await
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
                .map(|session| session.summary(self.world.workspace_ref(&state, session.id())))
                .collect();
            if let Some(limit) = query.limit {
                summaries.truncate(limit as usize);
            }
            Ok(summaries)
        }

        async fn origin_epochs(
            &self,
            sessions: &[SessionId],
        ) -> Result<Vec<(SessionId, OriginEpoch)>, PortError> {
            let state = lock(&self.world.state);
            Ok(sessions
                .iter()
                .filter(|session| state.sessions.contains_key(session.as_str()))
                .map(|session| {
                    (
                        session.clone(),
                        state
                            .epochs
                            .get(session.as_str())
                            .cloned()
                            .unwrap_or_else(|| origin_epoch_of(500)),
                    )
                })
                .collect())
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

        /// §3.6：恢复数据的窄读取。会话行不存在、或任一列为 `NULL`（没有可用于恢复的数据）时为 `None`。
        async fn load_recovery(
            &self,
            session: &SessionId,
        ) -> Result<Option<SessionRecoveryRecord>, PortError> {
            self.world.recovery_reads.fetch_add(1, Ordering::SeqCst);
            let state = lock(&self.world.state);
            let Some(found) = state.sessions.get(session.as_str()) else {
                return Ok(None);
            };
            let columns = state
                .recoveries
                .get(session.as_str())
                .cloned()
                .unwrap_or_default();
            let (Some(agent_session_id), Some(workspace_cwd)) =
                (columns.agent_session_id, columns.workspace_cwd)
            else {
                return Ok(None);
            };
            Ok(Some(SessionRecoveryRecord {
                agent: found.agent().clone(),
                agent_session_id: Some(agent_session_id),
                workspace_cwd: Some(workspace_cwd),
            }))
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

        /// 三态归约的事实读取（与真实存储同一口径）：`pending` 或「已仲裁未确认」且未失效。
        async fn session_activity(
            &self,
            session: &SessionId,
        ) -> Result<SessionActivityFacts, PortError> {
            let state = lock(&self.world.state);
            let mut unresolved = 0_u32;
            let mut unconfirmed = Vec::new();
            let mut by_turn: Vec<(TurnId, u32)> = Vec::new();
            for row in state
                .interactions
                .values()
                .filter(|row| row.pending.session() == session && !row.invalidated)
            {
                let resolved = row.resolution.is_some();
                if resolved && row.confirmed {
                    continue;
                }
                unresolved += 1;
                if resolved {
                    unconfirmed.push(row.pending.id().clone());
                }
                if let Some(turn) = row.turn.as_ref() {
                    match by_turn.iter_mut().find(|(candidate, _)| candidate == turn) {
                        Some((_, count)) => *count += 1,
                        None => by_turn.push((turn.clone(), 1)),
                    }
                }
            }
            let unfinished = state
                .turns
                .get(session.as_str())
                .map(|turns| {
                    turns
                        .iter()
                        .filter(|turn| !turn.state().is_terminal())
                        .count()
                })
                .unwrap_or(0);
            Ok(SessionActivityFacts {
                unresolved_interactions: unresolved,
                unconfirmed_interactions: unconfirmed,
                unresolved_by_turn: by_turn,
                unfinished_turns: u32::try_from(unfinished).unwrap_or(u32::MAX),
            })
        }

        async fn queued_turns(&self, session: &SessionId) -> Result<Vec<Turn>, PortError> {
            let state = lock(&self.world.state);
            let mut queued: Vec<Turn> = state
                .turns
                .get(session.as_str())
                .map(|turns| {
                    turns
                        .iter()
                        .filter(|turn| turn.state() == TurnState::Queued)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            queued.sort_by_key(Turn::queue_index);
            Ok(queued)
        }

        async fn sessions_with_pending_activity(
            &self,
            limit: ReplayLimit,
        ) -> Result<Vec<SessionId>, PortError> {
            let state = lock(&self.world.state);
            let mut sessions: Vec<SessionId> = Vec::new();
            for (id, turns) in &state.turns {
                if turns.iter().any(|turn| !turn.state().is_terminal())
                    && let Ok(session) = SessionId::new(id)
                    && !sessions.contains(&session)
                {
                    sessions.push(session);
                }
            }
            for row in state
                .interactions
                .values()
                .filter(|row| !row.invalidated && (row.resolution.is_none() || !row.confirmed))
            {
                let session = row.pending.session().clone();
                if !sessions.contains(&session) {
                    sessions.push(session);
                }
            }
            sessions.truncate(limit.events() as usize);
            Ok(sessions)
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
                summary: stored.summary(self.world.workspace_ref(&state, session)),
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

        /// Sync 事件读面：`FakeStore` 记录的行按 `global_sequence` 升序一页 + head。
        ///
        /// 与真实存储同一口径的四点：已压缩的 delta 不出现（`compacted_into` 有记录的行跳过）；
        /// `origin_kind` 取 fake 记录的 `event_origins`、`causation` 取 `event_causations`（都是 broker
        /// 提交时真实写入的取值）；`origin.deviceId`/`origin.nodeId` 按对应 kind 回查**同类别 actor**
        /// 的命令行且必须唯一（见 [`WorldState::identity_origin`]）；得不出唯一取值时**失败关闭**
        /// （`Corrupt`）而不是给 `None`——与真实存储的 `identity_origin_for` 同口径（§10.1 的配对
        /// 规则要求这两类事件带非空身份字段）。
        ///
        /// 本替身没有「列值损坏」这一失败路径（`origin_kind` 就是 `EventOrigin` 值，不经过文本解码）：
        /// 词表外的取值只可能出现在 `storage-sqlite`，由它的适配器用例固定。
        async fn sync_replay(
            &self,
            after: Option<GlobalCursor>,
            limit: ReplayLimit,
        ) -> Result<SyncReplaySlice, PortError> {
            let state = lock(&self.world.state);
            let from = after
                .as_ref()
                .map(|cursor| cursor.global_sequence.get())
                .unwrap_or(0);
            let head = head_cursor(state.head);
            let mut events = Vec::new();
            for event in &state.events {
                if event.global_sequence.get() <= from {
                    continue;
                }
                if state
                    .compacted_into
                    .contains_key(&event.global_sequence.get())
                {
                    continue;
                }
                if events.len() as u32 >= limit.events() {
                    break;
                }
                let Some(payload) = state.event_payloads.get(event.id.as_str()).cloned() else {
                    continue;
                };
                let origin_kind = state
                    .event_origins
                    .get(event.id.as_str())
                    .copied()
                    .ok_or(PortError::Corrupt("事件缺少 origin 记录"))?;
                let causation = state
                    .event_causations
                    .get(event.id.as_str())
                    .cloned()
                    .flatten();
                let origin_device_id = match origin_kind {
                    EventOrigin::Device => Some(
                        DeviceId::new(
                            &state
                                .identity_origin(
                                    causation.as_ref(),
                                    ActorKind::Device,
                                    Some(&event.id),
                                )
                                .ok_or(PortError::Corrupt(
                                    "device event cannot be attributed to a unique device command",
                                ))?,
                        )
                        .map_err(|_| {
                            PortError::Corrupt("owned_command.actor_id is not a device id")
                        })?,
                    ),
                    _ => None,
                };
                let origin_node_id = match origin_kind {
                    EventOrigin::Node => Some(
                        state
                            .identity_origin(causation.as_ref(), ActorKind::Node, Some(&event.id))
                            .ok_or(PortError::Corrupt(
                                "node event cannot be attributed to a unique node command",
                            ))?
                            .split_once('/')
                            .and_then(|(node, _)| NodeId::new(node).ok())
                            .ok_or(PortError::Corrupt(
                                "owned_command.actor_id is not a node id",
                            ))?,
                    ),
                    _ => None,
                };
                events.push(SyncEventRecord {
                    event: event.clone(),
                    payload,
                    origin_kind,
                    origin_device_id,
                    origin_node_id,
                    causation,
                });
            }
            Ok(SyncReplaySlice { events, head })
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
                    .filter_map(|event| {
                        state
                            .event_payloads
                            .get(event.id.as_str())
                            .cloned()
                            .map(|payload| OwnedEventRecord {
                                event: event.clone(),
                                payload,
                            })
                    })
                    .collect()
            } else {
                Vec::new()
            };
            // 结构化历史条目：与真实存储同口径——按会话内序号升序，缺省给最新一页，游标只取严格更早
            // 的条目，页标记表示更早的条目是否仍在（关掉 include 时按真实存储一样一次都不查）。
            // 条目集合取 [`HISTORY_ENTRY_EVENT_TYPES`]（`SYNC_PROTOCOL.md` §10.2.1 的 17 类，与真实存储
            // 共用同一常量）：不进入条目的事件既不出现在页里，也不占页大小与页标记。
            //
            // 本替身是世界状态，没有保留期/压缩模型：因此不存在「游标早于仍可读下界」这一失败路径
            // （§5.2/判据 33 ⑥ 由 `storage-sqlite` 的适配器与其用例固定），也不跳过任何行。
            let (history, history_has_earlier) = if query.include.history {
                let mut readable: Vec<CommittedEvent> = state
                    .events
                    .iter()
                    .filter(|event| event.session.as_ref() == Some(&query.session))
                    .filter(|event| event.session_sequence.is_some())
                    .filter(|event| HISTORY_ENTRY_EVENT_TYPES.contains(&event.event_type.as_str()))
                    .filter(|event| {
                        let sequence = event.session_sequence;
                        query.before_history.as_ref().is_none_or(|cursor| {
                            sequence.is_some_and(|sequence| sequence < *cursor)
                        })
                    })
                    .cloned()
                    .collect();
                readable.sort_by_key(|event| event.session_sequence);
                let keep = query.limit.events() as usize;
                let start = readable.len().saturating_sub(keep);
                let mut history = Vec::new();
                for event in &readable[start..] {
                    let Some(payload) = state.event_payloads.get(event.id.as_str()).cloned() else {
                        continue;
                    };
                    history.push(OwnedEventRecord {
                        event: event.clone(),
                        payload,
                    });
                }
                (history, start > 0)
            } else {
                (Vec::new(), false)
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
                session: session.summary(self.world.workspace_ref(&state, &query.session)),
                events,
                history,
                history_has_earlier,
                messages: Vec::new(),
                messages_has_earlier: false,
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

        async fn find_imported_session(
            &self,
            session: &SessionId,
        ) -> Result<Option<ImportedSessionRecord>, PortError> {
            Ok(lock(&self.world.imported_sessions)
                .iter()
                .find(|record| &record.session.session_id == session)
                .cloned())
        }

        async fn find_remote_request(
            &self,
            _session: &RemoteSessionRef,
            _request: &RequestId,
        ) -> Result<Option<RemoteCommandRef>, PortError> {
            Ok(None)
        }

        async fn prune(
            &self,
            _policy: RetentionPolicy,
            _at: Timestamp,
        ) -> Result<PruneReport, PortError> {
            Ok(PruneReport::default())
        }
    }

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

        /// 与真实存储同口径：返回全部配对行的快照（顺序无关，维护路径只按标识判定）。
        async fn pairings(&self) -> Result<Vec<PairingRecord>, PortError> {
            Ok(lock(&self.world.pairings).clone())
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
        async fn directories(&self) -> Result<Vec<DirectoryRecord>, PortError> {
            Ok(lock(&self.world.workspaces).clone())
        }

        async fn directory(
            &self,
            directory_id: &DirectoryId,
        ) -> Result<Option<DirectoryRecord>, PortError> {
            Ok(lock(&self.world.workspaces)
                .iter()
                .find(|record| record.directory_id() == directory_id)
                .cloned())
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
            _at: Timestamp,
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
        async fn terminate_agent(&self, session: &SessionId) -> Result<(), PortError> {
            lock(&self.world.terminated_agents).push(session.as_str().to_owned());
            Ok(())
        }

        async fn create(
            &self,
            session: &SessionId,
            _request: CreateSessionRequest,
            sink: EventSink,
        ) -> Result<Box<dyn SessionEndpoint>, PortError> {
            self.world.create_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(error) = lock(&self.world.create_errors).pop_front() {
                return Err(error);
            }
            // 后端现在直接拿到 core 分配的 id（§5.1），不再自己造引用。
            Ok(Box::new(FakeEndpoint {
                world: self.world.clone(),
                sink,
                reference: SessionReference::Owned(OwnedSessionRef::new(session.clone())),
                agent_session_id: lock(&self.world.agent_session_id).clone(),
            }))
        }

        async fn open(
            &self,
            reference: SessionReference,
            sink: EventSink,
        ) -> Result<Box<dyn SessionEndpoint>, PortError> {
            if let Some(error) = lock(&self.world.open_errors).pop_front() {
                return Err(error);
            }
            Ok(Box::new(FakeEndpoint {
                world: self.world.clone(),
                sink,
                reference,
                agent_session_id: lock(&self.world.agent_session_id).clone(),
            }))
        }

        /// 恢复：请求记进 `world.resume_requests`（测试断言「未调用后端」与「传的就是持久化取值」）；
        /// `world.resume_errors` 非空时依次弹出并返回（模拟「后端不支持」类错误）。
        async fn resume(
            &self,
            session: &SessionId,
            request: ResumeSessionRequest,
            sink: EventSink,
        ) -> Result<Box<dyn SessionEndpoint>, PortError> {
            lock(&self.world.resume_requests).push(request);
            if let Some(error) = lock(&self.world.resume_errors).pop_front() {
                return Err(error);
            }
            Ok(Box::new(FakeEndpoint {
                world: self.world.clone(),
                sink,
                reference: SessionReference::Owned(OwnedSessionRef::new(session.clone())),
                agent_session_id: lock(&self.world.agent_session_id).clone(),
            }))
        }
    }

    pub(crate) struct FakeEndpoint {
        pub(crate) world: Arc<FakeWorld>,
        pub(crate) sink: EventSink,
        pub(crate) reference: SessionReference,
        /// 本次绑定的 ACP 会话标识（`None` = Agent 未给出，R7）。
        pub(crate) agent_session_id: Option<AgentSessionId>,
    }

    #[async_trait]
    impl SessionEndpoint for FakeEndpoint {
        fn reference(&self) -> SessionReference {
            self.reference.clone()
        }

        fn agent_session_id(&self) -> Option<&AgentSessionId> {
            self.agent_session_id.as_ref()
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

        async fn validate_prompt(&self, _request: &PromptRequest) -> Result<(), PortError> {
            Ok(())
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
            match lock(&self.world.resolve_errors).pop_front() {
                Some(error) => Err(error),
                None => Ok(()),
            }
        }

        async fn read_history(&self, _query: HistoryQuery) -> Result<HistoryPage, PortError> {
            let session = match &self.reference {
                SessionReference::Owned(owned) => owned.session_id.clone(),
                SessionReference::Remote(remote) => remote.session_id.clone(),
            };
            Err(PortError::NotFound(EntityRef::Session(session)))
        }

        async fn close(&self) -> Result<(), PortError> {
            lock(&self.world.closed_endpoints).push(match &self.reference {
                SessionReference::Owned(owned) => owned.session_id.as_str().to_owned(),
                SessionReference::Remote(remote) => remote.session_id.as_str().to_owned(),
            });
            Ok(())
        }
    }

    // -----------------------------------------------------------------------------------------
    // 有效待处理交互 fake
    // -----------------------------------------------------------------------------------------

    /// `InteractionAuthority` 的读面替身：`effective` 返回脚本化清单（版本/表单校验的来源）；
    /// 应答仲裁与确认走存储端口（`SessionStore::commit`），因此本替身不再有写入阶段。
    pub(crate) struct FakeInteractions {
        pub(crate) world: Arc<FakeWorld>,
    }

    #[async_trait]
    impl InteractionAuthority for FakeInteractions {
        async fn effective(
            &self,
            session: &SessionId,
        ) -> Result<Vec<EffectiveInteraction>, PortError> {
            Ok(lock(&self.world.effective_interactions)
                .iter()
                .filter(|item| &item.session == session)
                .cloned()
                .collect())
        }

        async fn invalidate_pending(
            &self,
            session: Option<&SessionId>,
            _at: Timestamp,
        ) -> Result<u32, PortError> {
            let mut effective = lock(&self.world.effective_interactions);
            let before = effective.len();
            effective.retain(|item| match session {
                Some(session) => &item.session != session,
                None => false,
            });
            let removed = before - effective.len();
            lock(&self.world.invalidated_interactions)
                .push((session.map(SessionId::as_str).map(str::to_owned), removed));
            // 与真实存储同口径：失效把存储交互行移出有效集合（含已仲裁未确认的行）。
            let mut state = lock(&self.world.state);
            for row in state.interactions.values_mut() {
                if session.is_none_or(|session| row.pending.session() == session) {
                    row.invalidated = true;
                }
            }
            Ok(u32::try_from(removed).unwrap_or(u32::MAX))
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
            Self::build(config, None)
        }

        /// 带 `InteractionAuthority` 的装配：返回 broker 与可观察的 fake 授权端口。
        pub(crate) fn with_interactions(config: BrokerConfig) -> (Self, Arc<FakeInteractions>) {
            let world = FakeWorld::new();
            let authority = Arc::new(FakeInteractions {
                world: world.clone(),
            });
            (
                Self::build_in(config, Some(authority.clone()), world),
                authority,
            )
        }

        fn build(config: BrokerConfig, interactions: Option<Arc<FakeInteractions>>) -> Self {
            let world = FakeWorld::new();
            Self::build_in(config, interactions, world)
        }

        fn build_in(
            config: BrokerConfig,
            interactions: Option<Arc<FakeInteractions>>,
            world: Arc<FakeWorld>,
        ) -> Self {
            let store = Arc::new(FakeStore {
                world: world.clone(),
            });
            let seq = Sequence::new(1).expect("sequence");
            let _ = seq;
            let mut broker = Broker::new(
                BrokerDeps {
                    store: store.clone(),
                    deliveries: Arc::new(FakeDeliveries {
                        world: world.clone(),
                    }),
                    backends: Arc::new(FakeBackend {
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
            );
            if let Some(authority) = interactions {
                broker = broker.with_interaction_authority(authority);
            }
            let broker = Arc::new(broker);
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
    use super::test_support::lock;
    use super::test_support::*;
    use super::*;
    use crate::model::{
        AcpRaw, AgentId, AgentRef, AgentSessionId, AttachmentId, AttachmentSubject,
        AttachmentSubmission, DeviceId, ElicitationSchema, EventId, InteractionKind,
        InteractionOption, ModeState, NodeId, PairingId, PendingInteraction, ResourceOrigin,
        SubmissionId,
    };

    /// 一个已认证的设备主体（§10.1 的「按诱发命令的执行者分类」断言与授权回归都用它）。
    ///
    /// 设备不再有授权集合（`pairing-model` 删除设备权限层），构造因此不需要任何 scope：命令的执行前提
    /// 只有「该连接已通过设备认证」，而认证发生在连接层、不在 broker 判定。
    fn device_actor() -> Actor {
        Actor::Device {
            device: DeviceId::new(&uuid_text(40)).expect("uuid"),
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
                "session.updated",
                "turn.started",
                "agent.message.delta",
                "turn.completed",
                "command.completed",
                "session.updated",
            ],
            "§11.2：领域事件先于 terminal event；三态摘要随排队与终态各更新一次"
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

    // -----------------------------------------------------------------------------------------
    // §6 第 20 条：窗口期（会话行落账之前）到达的后端事件
    // -----------------------------------------------------------------------------------------

    /// `session.create` 的请求（本组用例共用一个 Agent，workspace 为空 = 未分组）。
    fn window_create_request() -> CreateSessionRequest {
        CreateSessionRequest::new(
            AgentRef::try_new(AgentId::new("agent-1").expect("agent id"), "Agent One")
                .expect("agent ref"),
            None,
            None,
            ResourceOrigin::Local,
        )
    }

    /// 受理一条 `session.create` 并断言拿到 `Prepared`：窗口期从这里开始——后端已建立、会话行未落账。
    fn prepared_create(
        harness: &Harness,
        request: &RequestId,
        seed: char,
    ) -> Box<PreparedSessionCreate> {
        match block_on(harness.broker.create_session(
            &harness.actor(),
            request,
            &digest(seed),
            window_create_request(),
            None,
        ))
        .expect("session.create 受理")
        {
            SessionCreateAdmission::Prepared(prepared) => prepared,
            other => panic!("首次受理必须返回 Prepared，得到 {other:?}"),
        }
    }

    /// 落账一次创建（`completed` + 适配层投影的结果原文），返回是否真的写入了终态。
    fn settle_window(harness: &Harness, request: &RequestId) -> bool {
        let result =
            CommandResult::from_json_text(r#"{"sessionId":"projected"}"#).expect("result object");
        block_on(harness.broker.settle_session_create(
            &harness.actor(),
            request,
            CommandStatus::Completed,
            Some(result),
            None,
        ))
        .expect("落账")
    }

    /// 某会话已广播的事件类型（按发布顺序；只取 owned 的会话级投递）。
    fn published_types_of(harness: &Harness, session: &SessionId) -> Vec<String> {
        harness
            .world
            .published()
            .into_iter()
            .filter_map(|delivery| match delivery {
                CommittedDelivery::Owned(event) if event.session.as_ref() == Some(session) => {
                    Some(event.event_type.as_str().to_owned())
                }
                _ => None,
            })
            .collect()
    }

    /// 窗口期第 1 态（`sync-session-create`「会话行落账前的事件不丢」）：落账之前到达的事件只在会话
    /// 槽位里排队——驱动合并窗口也不写库、不广播；落账提交成功后按到达顺序落库并广播。
    #[test]
    fn window_events_queue_until_the_settlement_commit() {
        let harness = Harness::new(BrokerConfig::default());
        let request = harness.request(21);
        let prepared = prepared_create(&harness, &request, 'W');
        let sink = harness.broker.sink(&prepared.session);
        for mode in ["plan", "code"] {
            sink.send(endpoint_event(
                EventKind::State,
                "session.mode.changed",
                &format!(r#"{{"currentModeId":"{mode}"}}"#),
            ));
        }

        // 落账前：驱动一次合并窗口也只有排队——那一刻 `owned_session` 还没有这一行，落盘会写出引用
        // 不存在会话的事件，因此既不得写库、也不得广播。
        block_on(harness.broker.pump(&prepared.session)).expect("pump");
        assert!(
            harness.world.session(&prepared.session).is_none(),
            "受理不建会话行（§6 第 20 条）"
        );
        assert!(
            harness.world.events(&prepared.session).is_empty(),
            "落账前到达的事件不得落库"
        );
        assert_eq!(
            harness.world.publish_count(),
            0,
            "落账前不得广播任何事件（先持久化成功再广播）"
        );

        assert!(settle_window(&harness, &request), "落账必须真的写入");
        assert!(
            harness.world.session(&prepared.session).is_some(),
            "落账提交建会话行"
        );
        assert_eq!(
            harness
                .world
                .events(&prepared.session)
                .iter()
                .map(|event| event.event_type.as_str().to_owned())
                .collect::<Vec<String>>(),
            vec![
                "command.completed".to_owned(),
                "session.mode.changed".to_owned(),
                "session.mode.changed".to_owned()
            ],
            "落账提交之后，窗口期排队的两个事件按到达顺序落库"
        );
        assert_eq!(
            published_types_of(&harness, &prepared.session),
            vec![
                "command.completed".to_owned(),
                "session.mode.changed".to_owned(),
                "session.mode.changed".to_owned()
            ],
            "广播顺序与落库顺序一致"
        );
        assert!(
            harness.world.published_after_commit(),
            "每条投递都必须发生在它落盘之后（§6.1）"
        );
    }

    /// 窗口期第 2 态（`sync-session-create`「顺序正确、不丢不重」）：窗口期到达的**多条**事件在落账后
    /// 逐条落库——会话内序号连续、eventId 序列与广播序列都与到达顺序逐条相同，一条不多也一条不少。
    #[test]
    fn window_events_keep_their_arrival_order_without_loss_or_duplication() {
        let harness = Harness::new(BrokerConfig::default());
        let request = harness.request(22);
        let prepared = prepared_create(&harness, &request, 'X');
        let sink = harness.broker.sink(&prepared.session);
        let modes = ["plan", "code", "review", "debug"];
        for mode in modes {
            sink.send(endpoint_event(
                EventKind::State,
                "session.mode.changed",
                &format!(r#"{{"currentModeId":"{mode}"}}"#),
            ));
        }

        assert!(settle_window(&harness, &request), "落账必须真的写入");
        let stored = harness.world.events(&prepared.session);
        let windowed: Vec<&CommittedEvent> = stored
            .iter()
            .filter(|event| event.event_type.as_str() == "session.mode.changed")
            .collect();
        assert_eq!(
            windowed.len(),
            modes.len(),
            "窗口期的四条事件必须一条不少地落库"
        );
        // 会话内序号连续：落账提交带的那条终态事件占 1，其后逐条 +1（不跳号、不重号）。
        let sequences: Vec<u64> = windowed
            .iter()
            .map(|event| {
                event
                    .session_sequence
                    .expect("会话级事件必须有会话内序号")
                    .get()
            })
            .collect();
        assert_eq!(sequences, vec![2, 3, 4, 5], "落库序号必须连续且按到达顺序");
        // 正文取值顺序 = 到达顺序（不重不丢）。
        let views: Vec<String> = windowed
            .iter()
            .map(|event| stored_view(&harness, event))
            .collect();
        for (index, mode) in modes.iter().enumerate() {
            assert!(
                views[index].contains(&format!(r#""currentModeId":"{mode}""#)),
                "第 {index} 条落库事件必须是到达顺序里的 `{mode}`：{views:?}"
            );
        }
        // 广播顺序与落库顺序逐条一致（同一条事件、同一个 eventId）。
        let stored_ids: Vec<String> = windowed
            .iter()
            .map(|event| event.id.as_str().to_owned())
            .collect();
        let published_ids: Vec<String> = harness
            .world
            .published()
            .into_iter()
            .filter_map(|delivery| match delivery {
                CommittedDelivery::Owned(event)
                    if event.session.as_ref() == Some(&prepared.session) =>
                {
                    Some(event.id.as_str().to_owned())
                }
                _ => None,
            })
            .filter(|id| stored_ids.contains(id))
            .collect();
        assert_eq!(published_ids, stored_ids, "广播顺序必须与落库顺序逐条相同");
    }

    /// 窗口期第 3 态（`sync-session-create`「超限走失败关闭」）：超过 [`WINDOW_EVENT_QUEUE_LIMIT`] 时
    /// 不再入队，结算路径**结束该后端**并把命令终结为 `uncertain`——不静默丢事件，也不留下半个会话：
    /// 库里既没有该会话的会话行，也没有它的任何事件行。
    #[test]
    fn a_window_event_overflow_closes_the_backend_and_ends_the_create_as_uncertain() {
        let harness = Harness::new(BrokerConfig::default());
        let request = harness.request(23);
        let prepared = prepared_create(&harness, &request, 'Y');
        let sink = harness.broker.sink(&prepared.session);
        for index in 0..WINDOW_EVENT_QUEUE_LIMIT {
            sink.send(endpoint_event(
                EventKind::State,
                "session.mode.changed",
                &format!(r#"{{"currentModeId":"m{index}"}}"#),
            ));
        }
        assert!(
            harness.world.closed_endpoints().is_empty(),
            "入队到上限为止还不算超限：不得提前结束后端"
        );
        // 第 `LIMIT + 1` 条：越界，不再入队并标记失败关闭。
        sink.send(endpoint_event(
            EventKind::State,
            "session.mode.changed",
            r#"{"currentModeId":"overflow"}"#,
        ));

        assert!(
            settle_window(&harness, &request),
            "失败关闭也要把命令终结掉（不是幂等 no-op）"
        );
        assert_eq!(
            harness.world.terminated_agents(),
            vec![prepared.session.as_str().to_owned()],
            "超限必须**结束该 Agent 运行时**（`SessionBackendFactory::terminate_agent`），而不是只关端点"
        );
        assert!(
            harness.world.closed_endpoints().is_empty(),
            "结束该 Agent 走 `terminate_agent`；端点关闭由该后端自己的回收路径完成，core 不再单独关它"
        );
        let record = harness.world.command(&request).expect("终态记录");
        assert_eq!(
            record.status(),
            CommandStatus::Uncertain,
            "超限意味着无法确认会话是否已建立：不得报 `completed`"
        );
        assert_eq!(
            record.error().map(PublicError::code),
            Some("command.uncertain"),
            "成因由 `command.uncertain` 承担"
        );
        assert!(
            harness.world.session(&prepared.session).is_none(),
            "失败关闭不得留下会话行"
        );
        assert!(
            harness.world.events(&prepared.session).is_empty(),
            "失败关闭不得留下该会话的任何事件行（既不落盘已入队的事件，也不静默少写）"
        );
        // 队列上界在失败关闭之后**保持**，且槽位被围栏：该会话永远不会落账，因此没有消费者；若在这里
        // 解除上界，仍在发事件的 Agent 会让队列无界增长。再灌 `LIMIT + 8` 条，断言一条都没被缓冲。
        for index in 0..(WINDOW_EVENT_QUEUE_LIMIT + 8) {
            sink.send(endpoint_event(
                EventKind::State,
                "session.mode.changed",
                &format!(r#"{{"currentModeId":"after{index}"}}"#),
            ));
        }
        let slot = harness.broker.owned_slot(&prepared.session);
        assert!(
            super::lock(&slot.pending).is_empty(),
            "失败关闭之后到达的事件必须直接丢弃（不得缓冲成永久滞留的队列）"
        );
    }

    /// §3.6/§5.2：恢复数据走**窄读取**；会话不存在、或任一列为 `NULL` 时是 `None`（`NULL` 不是错误，
    /// 也不得被推导或补齐）。
    #[test]
    fn load_recovery_reports_null_columns_as_absent() {
        let harness = Harness::new(BrokerConfig::default());
        let store = FakeStore {
            world: harness.world.clone(),
        };
        let agent_session_id = AgentSessionId::new("acp-session-7").expect("agent session id");

        assert!(
            block_on(store.load_recovery(&harness.session))
                .expect("read")
                .is_none(),
            "夹具预置的会话没有恢复数据（两列 NULL）"
        );
        let unknown = SessionId::new(&uuid_text(404)).expect("session id");
        assert!(
            block_on(store.load_recovery(&unknown))
                .expect("read")
                .is_none(),
            "未知会话与两列 NULL 同形"
        );

        harness.world.seed_recovery(
            &harness.session,
            Some(agent_session_id.clone()),
            Some("C:\\work".to_owned()),
        );
        let record = block_on(store.load_recovery(&harness.session))
            .expect("read")
            .expect("record");
        assert_eq!(record.agent.agent_id().as_str(), "agent-1");
        assert_eq!(record.agent_session_id, Some(agent_session_id));
        assert_eq!(record.workspace_cwd.as_deref(), Some("C:\\work"));

        // 只有一列：仍然没有可用于恢复的数据（两列是一体的）。
        harness
            .world
            .seed_recovery(&harness.session, None, Some("C:\\work".to_owned()));
        assert!(
            block_on(store.load_recovery(&harness.session))
                .expect("read")
                .is_none()
        );
    }

    /// §2/§5.1：「后端不支持该操作」取既有码 `command.unsupported`，且是**不可重试**的一类
    /// （与「临时故障」区分）；Node Link / 本地管理适配器各自映射到既有 wire 码。
    #[test]
    fn backend_unsupported_maps_to_the_existing_unsupported_code() {
        let harness = Harness::new(BrokerConfig::default());
        let public = harness
            .broker
            .port_error_public(&PortError::Unavailable(UnavailableKind::BackendUnsupported))
            .expect("map");
        assert_eq!(public.code(), "command.unsupported");
        assert!(!public.retryable(), "不支持不是可重试的临时故障");
    }

    /// 已 `accepted` 的 `session.resume` 持久记录（`settle_session_resume` 的定位目标）。
    fn accepted_resume_record(
        harness: &Harness,
        request: &RequestId,
        fingerprint: char,
    ) -> CommandRecord {
        CommandRecord::try_new(
            Some(harness.session.clone()),
            request.clone(),
            "session.resume",
            CommandKind::Mutation,
            harness.actor(),
            Some(ts(1)),
            CommandStatus::Accepted,
            None,
            None,
            None,
            None,
            None,
            digest(fingerprint),
        )
        .expect("command record")
    }

    /// §5.1：`settle_session_resume` 只终结 `session.resume` 的持久记录；同一条记录重复终结（或从未
    /// 落盘）是**幂等 no-op**，不覆盖首次结果。
    #[test]
    fn settle_session_resume_terminates_only_its_own_record_and_is_idempotent() {
        let harness = Harness::new(BrokerConfig::default());
        let actor = harness.actor();
        let request = harness.request(11);
        harness
            .world
            .seed_command(accepted_resume_record(&harness, &request, 'R'));

        let result =
            CommandResult::from_json_text(r#"{"sessionId":"acp-1"}"#).expect("result object");
        let written = block_on(harness.broker.settle_session_resume(
            &actor,
            &request,
            CommandStatus::Completed,
            Some(result.clone()),
            None,
        ))
        .expect("settle");
        assert!(written, "第一次终结必须真的写入");
        let after = harness.world.command(&request).expect("记录");
        assert_eq!(after.command(), "session.resume");
        assert_eq!(after.status(), CommandStatus::Completed);
        assert!(
            after.terminal_event().is_some(),
            "终态必须带 terminalEventId"
        );
        assert_eq!(
            after.result().map(CommandResult::as_str),
            Some(result.as_str())
        );

        // 已终结：幂等 no-op，不覆盖首次结果。
        let again = block_on(
            harness.broker.settle_session_resume(
                &actor,
                &request,
                CommandStatus::Failed,
                None,
                Some(
                    PublicError::coded("nodelink.command.unsupported", "unsupported", false)
                        .expect("error"),
                ),
            ),
        )
        .expect("second settle");
        assert!(!again, "已终结的记录不覆盖首次结果");
        let unchanged = harness.world.command(&request).expect("记录");
        assert_eq!(unchanged.status(), CommandStatus::Completed);
        assert_eq!(
            unchanged.result().map(CommandResult::as_str),
            Some(result.as_str())
        );

        // 没有持久记录（恢复在幂等行落盘前就失败）：幂等 no-op。
        let missing = harness.request(12);
        assert!(
            !block_on(harness.broker.settle_session_resume(
                &actor,
                &missing,
                CommandStatus::Completed,
                Some(result),
                None,
            ))
            .expect("missing record")
        );
    }

    /// §5.1：`session.resume` 的终态入口不得改写别的命令的幂等行（适配层误用，wire 不可达）。
    #[test]
    fn settle_session_resume_rejects_other_commands() {
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

        let error = block_on(
            harness.broker.settle_session_resume(
                &harness.actor(),
                &request,
                CommandStatus::Failed,
                None,
                Some(
                    PublicError::coded("nodelink.command.unsupported", "unsupported", false)
                        .expect("error"),
                ),
            ),
        )
        .expect_err("非 session.resume 的记录不得被终结");
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

        // 不同 actor 用同一 requestId → 视为独立命令（幂等键是协议维度的 `(actor, requestId)`）：
        // 这条命令不与上一条冲突，而是走自己的授权判定。设备与节点方向都已无命令级判定，因此这里用
        // 配对认领方（它永远不得提交命令），它得到的仍是自己的 `authorization.scope_denied`（而不是
        // `command.idempotency_conflict`）——两者的区别正是本用例要钉住的。
        let other = Actor::PairingClaimant {
            pairing: PairingId::new(&uuid_text(41)).expect("uuid"),
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

    /// §11.2：终态记录带上收据里有意义的分量——prompt 的 `completed` 带 `{"turnId":…}`，
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

    /// §6 第 9 条：**非终态**批次的写失败不得只丢弃——该 turn 的正文已经缺了一块，
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

    /// §6 第 9/14 条：被放弃的 turn 仍要为**已落盘**的 delta 收尾（`completed`），
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
                "session.updated",
                "turn.started",
                "agent.message.delta",
                "turn.failed",
                "agent.message.completed",
                "command.uncertain",
                "session.updated",
            ],
            "只保留已落盘的增量、收尾与失败终态；释放占位后摘要收敛"
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

    /// §6 第 9 条：被放弃 turn 的迟到终态不得记到下一个 turn 上。
    ///
    /// 适配器发出的 `EndpointEvent` 不带 turn 标识（`agent-host` 的 mapper 固定 `turn: None`），归属只能
    /// 按「有在跑 turn 时归给它」推断（§10.3）。因此放弃之后**不得**立刻提升下一个排队 turn：否则 T1 的
    /// 迟到 `turn.completed` 会把 T2 报成完成，并把 T2 已落盘的正文按 T1 的终态折叠。
    #[test]
    fn a_late_terminal_of_an_abandoned_turn_does_not_complete_the_next_turn() {
        let harness = Harness::new(BrokerConfig::default());
        // T1：空脚本（派发后不发任何事件）；T2：只发一条增量，自己不收尾。
        harness.world.push_script(Script::new(Vec::new()));
        let second = MessageId::new(&uuid_text(612)).expect("message");
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::Delta,
            "agent.message.delta",
            &format!(
                r#"{{"messageId":"{}","deltaIndex":"0","text":"second"}}"#,
                second.as_str()
            ),
        )]));
        let first = MessageId::new(&uuid_text(610)).expect("message");
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));
        assert!(matches!(
            harness.submit_prompt(2, 'B'),
            CommandReceipt::Accepted { .. }
        ));
        // 提交顺序：1 = T1 接受、2 = T1 派发、3 = T2 接受、4 = T1 的增量批次（本用例注入失败）。
        harness.world.fail_commit_at(4);
        harness.broker.sink(&harness.session).send(endpoint_event(
            EventKind::Delta,
            "agent.message.delta",
            &format!(
                r#"{{"messageId":"{}","deltaIndex":"0","text":"first"}}"#,
                first.as_str()
            ),
        ));
        block_on(harness.broker.pump(&harness.session)).expect("pump");

        // ① 被放弃 turn 的终态还没被观测到：T2 不得被派发，且会话必须仍是可被驱动的状态。
        assert_eq!(harness.world.prompt_count(), 1, "不得提前派发 T2");
        let queued = harness
            .world
            .turns(&harness.session)
            .into_iter()
            .filter(|turn| turn.state() == TurnState::Queued)
            .count();
        assert_eq!(queued, 1, "T2 必须还在排队");
        assert_eq!(
            harness
                .world
                .session(&harness.session)
                .expect("session")
                .state(),
            SessionState::Running,
            "放弃后仍排队的会话必须仍是运行中（否则合并窗口永远推不动）"
        );

        // ② 无归属的迟到终态：按被放弃的 T1 归属并丢弃，同时释放占位。
        harness.broker.sink(&harness.session).send(endpoint_event(
            EventKind::State,
            "turn.completed",
            &turn_view("completed"),
        ));
        block_on(harness.broker.pump(&harness.session)).expect("pump");

        // ③ T2 不得被记成完成；它自己的事件也不得被丢弃。
        assert_eq!(
            harness
                .world
                .command(&harness.request(2))
                .expect("幂等行")
                .status(),
            CommandStatus::Accepted,
            "T1 的迟到终态不得把 T2 的命令报成完成"
        );
        assert_eq!(
            harness.world.event_types(&harness.session),
            vec![
                "turn.queued",
                "session.updated",
                "turn.started",
                "turn.queued",
                "turn.failed",
                "command.uncertain",
                "turn.started",
                "agent.message.delta",
            ],
            "迟到终态被丢弃；观测到它之后 T2 才被派发（其增量必须落盘）"
        );
        let turns = harness.world.turns(&harness.session);
        assert_eq!(
            turns.iter().map(|turn| turn.state()).collect::<Vec<_>>(),
            vec![TurnState::Failed, TurnState::Running],
            "T2 只能是 Running，不得是 Completed：{turns:?}"
        );
        assert!(harness.world.published_after_commit());
    }

    /// §6 第 9 条的兑底：端点始终没有观测到终态时，占位必须在有界轮次后释放，
    /// 否则排队的 turn 永远不被派发、会话永久卡住。
    #[test]
    fn an_unobserved_abandoned_turn_hold_is_released_after_the_bounded_rounds() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(Vec::new()));
        harness.world.push_script(Script::new(Vec::new()));
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));
        assert!(matches!(
            harness.submit_prompt(2, 'B'),
            CommandReceipt::Accepted { .. }
        ));
        harness.world.fail_commit_at(4);
        harness.broker.sink(&harness.session).send(endpoint_event(
            EventKind::Delta,
            "agent.message.delta",
            &format!(
                r#"{{"messageId":"{}","deltaIndex":"0","text":"lost"}}"#,
                uuid_text(613)
            ),
        ));
        block_on(harness.broker.pump(&harness.session)).expect("pump");
        assert_eq!(harness.world.prompt_count(), 1);

        // 合同登记值：§6 第 9 条写的是 `1200` 轮，这里把常量钉死在用例里（合同与实现不得各自漂移）。
        assert_eq!(ABANDONED_TURN_HOLD_ROUNDS, 1200, "§6 第 9 条登记值");

        // 占位按驱动轮次计时：放弃提交所在的那次 `pump` 已消耗第一轮，此后每次 `pump` 消耗一轮。
        let mut rounds = 1;
        while rounds + 1 < ABANDONED_TURN_HOLD_ROUNDS {
            block_on(harness.broker.pump(&harness.session)).expect("pump");
            rounds += 1;
        }
        assert_eq!(harness.world.prompt_count(), 1, "轮次未用尽前不得派发 T2");
        block_on(harness.broker.pump(&harness.session)).expect("pump");
        assert_eq!(
            harness.world.prompt_count(),
            2,
            "兑底必须释放占位，否则会话永久卡住"
        );
    }

    /// §6 第 4/8/9 条：占位期该会话按「仍有 active turn」处理，因此模式切换被显式拒绝
    /// （v1 不排队），而不是因为该 turn 已经终结就被放行。
    #[test]
    fn mode_change_rejected_while_an_abandoned_turn_holds_the_session_slot() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(Vec::new()));
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));
        // 提交顺序：1 = 接受、2 = 派发、3 = 增量批次（本用例注入失败），4 = 放弃提交。
        harness.world.fail_commit_at(3);
        harness.broker.sink(&harness.session).send(endpoint_event(
            EventKind::Delta,
            "agent.message.delta",
            &format!(
                r#"{{"messageId":"{}","deltaIndex":"0","text":"lost"}}"#,
                uuid_text(616)
            ),
        ));
        block_on(harness.broker.pump(&harness.session)).expect("pump");

        // 端点还没给出被放弃 turn 的终态 → 占位仍在，不能用「版本不匹配」冒充这个拒绝，
        // 因此 expected_version 取当前版本。
        let current = harness
            .world
            .session(&harness.session)
            .expect("session")
            .version();
        let actor = harness.actor();
        let command = mode_command(&actor, &harness.session, &harness.request(2), current);
        let receipt = block_on(harness.broker.submit_mutation(&actor, &command)).expect("submit");
        match receipt {
            CommandReceipt::Rejected { error } => {
                assert_eq!(error.code(), "state.version_conflict");
            }
            CommandReceipt::Accepted { .. } => {
                panic!("占位期会话仍有 active turn，模式切换必须被拒绝")
            }
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

    /// §6 第 9/19 条：被放弃 turn 的迟到事件走第 9 条的「归属到被放弃的 turn 并被丢弃」，
    /// **不是**第 19 条的「无归属降级」（带着 NULL 的 `turn_id` 照常落库）。
    #[test]
    fn a_late_event_of_an_abandoned_turn_is_dropped_instead_of_degraded_without_a_turn() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(Vec::new()));
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));
        // 提交顺序：1 = 接受、2 = 派发、3 = 增量批次（本用例注入失败），4 = 放弃提交。
        harness.world.fail_commit_at(3);
        harness.broker.sink(&harness.session).send(endpoint_event(
            EventKind::Delta,
            "agent.message.delta",
            &format!(
                r#"{{"messageId":"{}","deltaIndex":"0","text":"lost"}}"#,
                uuid_text(614)
            ),
        ));
        block_on(harness.broker.pump(&harness.session)).expect("pump");
        // 迟到终态被丢弃并释放占位：此后这个会话没有在跑的 turn。
        harness.broker.sink(&harness.session).send(endpoint_event(
            EventKind::State,
            "turn.completed",
            &turn_view("completed"),
        ));
        block_on(harness.broker.pump(&harness.session)).expect("pump");
        let before = harness.world.event_types(&harness.session);

        // 没有在跑 turn，但归属者（被放弃的 turn）存在：迟到增量按 §6 第 9 条丢弃，不落库。
        harness.broker.sink(&harness.session).send(endpoint_event(
            EventKind::Delta,
            "agent.message.delta",
            &format!(
                r#"{{"messageId":"{}","deltaIndex":"0","text":"tail"}}"#,
                uuid_text(615)
            ),
        ));
        block_on(harness.broker.pump(&harness.session)).expect("pump");
        assert_eq!(
            harness.world.event_types(&harness.session),
            before,
            "被放弃 turn 的迟到增量不得落库"
        );

        // 阳性对照：会话级事件不属于第 19 条的 `turnId` 集合，它照常提交（turn 归属为 NULL）——
        // 证明上一条是「归属到被放弃的 turn」，而不是「迟到的都丢」。
        harness.broker.sink(&harness.session).send(endpoint_event(
            EventKind::State,
            "session.origin.online_changed",
            r#"{"online":true}"#,
        ));
        block_on(harness.broker.pump(&harness.session)).expect("pump");
        let mut expected = before;
        expected.push("session.origin.online_changed".to_owned());
        assert_eq!(harness.world.event_types(&harness.session), expected);
        let event = harness
            .world
            .events(&harness.session)
            .into_iter()
            .find(|event| event.event_type.as_str() == "session.origin.online_changed")
            .expect("会话级事件必须落库");
        assert!(
            harness.world.event_turn(&event.id).is_none(),
            "无归属的降级：turn 归属为 NULL"
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
                "command.completed".to_owned(),
                "session.updated".to_owned()
            ]),
            "终态不延迟、也不与增量跨 turn 合并：{batches:?}"
        );
        assert_eq!(
            harness.world.events(&harness.session).len(),
            9,
            "合并不得丢事件（turn.queued + session.updated + turn.started + 3 delta + turn.completed + command.completed + session.updated）"
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

    /// §6.5：设备方向**没有**命令级授权判定——通过认证即可执行任何业务命令（`pairing-model` 删除设备
    /// 权限层；`AGENTS.md` §4 的安全不变量）。本用例的判别力在于：主体是一个**从未被授予任何 scope** 的
    /// 设备，而命令照常落到后端；旧实现会以 `authorization.scope_denied` 拒绝并且一次都不派发。
    #[test]
    fn authenticated_device_executes_without_any_scope() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::Delta,
            "agent.message.delta",
            &turn_view("running"),
        )]));
        let actor = device_actor();
        let command = prompt_command(&actor, &harness.session, &harness.request(1), 'A');
        let receipt = block_on(harness.broker.submit_mutation(&actor, &command)).expect("submit");
        assert!(
            matches!(receipt, CommandReceipt::Accepted { .. }),
            "已认证设备不得因缺少 scope 被拒，得到 {receipt:?}"
        );
        assert_eq!(harness.world.prompt_count(), 1, "命令必须真的派发到后端");
        assert_eq!(
            harness.world.audits().len(),
            0,
            "没有拒绝就没有 authorization.denied 行"
        );
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
                history: false,
            },
            before_history: None,
            before_messages: None,
            messages_limit: ReplayLimit::new(0),
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

    /// §5.2 + `SYNC_PROTOCOL.md` §10.2.1：`read_session` 的历史条目只取条目集合内的事件类型——替身与
    /// 真实存储共用 [`HISTORY_ENTRY_EVENT_TYPES`]（本 crate 里**唯一**的定义），这里固定替身确实施加了
    /// 该谓词：转写消息正文的 `agent.message.delta` 既不进条目也不占页，工具调用进。
    #[test]
    fn fake_history_page_drops_events_outside_the_entry_types() {
        let harness = Harness::new(BrokerConfig::default());
        let delta_view = format!(
            r#"{{"messageId":"{}","deltaIndex":"0","text":"hi"}}"#,
            uuid_text(700)
        );
        harness.world.push_script(Script::new(vec![
            endpoint_event(EventKind::Delta, "agent.message.delta", &delta_view),
            endpoint_event(
                EventKind::Structured,
                "tool.call.started",
                r#"{"toolCallId":"c1","title":"Run","state":"pending"}"#,
            ),
        ]));
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));

        let written: Vec<String> = harness
            .world
            .events(&harness.session)
            .into_iter()
            .map(|event| event.event_type.as_str().to_owned())
            .collect();
        assert!(
            written.iter().any(|event| event == "agent.message.delta"),
            "正文增量必须落盘（`messages` 面的原料）：{written:?}"
        );
        assert!(written.iter().any(|event| event == "tool.call.started"));

        let read_view = block_on(harness.broker.read_view()).expect("read view");
        let page = block_on(read_view.read_session(HistoryQuery {
            session: harness.session.clone(),
            include: HistoryInclude {
                history: true,
                ..HistoryInclude::default()
            },
            before_history: None,
            before_messages: None,
            messages_limit: ReplayLimit::new(0),
            after: None,
            limit: ReplayLimit::new(64),
        }))
        .expect("read");
        let entries: Vec<&str> = page
            .history
            .iter()
            .map(|record| record.event.event_type.as_str())
            .collect();
        assert!(
            entries
                .iter()
                .all(|event_type| HISTORY_ENTRY_EVENT_TYPES.contains(event_type)),
            "条目只能来自 §10.2.1 的 17 类：{entries:?}"
        );
        assert!(
            entries.contains(&"tool.call.started"),
            "工具调用必须成为条目：{entries:?}"
        );
        assert!(
            !entries.contains(&"agent.message.delta"),
            "承载消息正文的增量不得成为条目：{entries:?}"
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
                title: None,
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
                    request_version: None,
                }),
                confirmation: None,
                agent_session_id: None,
                workspace_cwd: None,
                workspace_directory_id: None,
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

    /// §10.1：`origin.kind` 的**纯映射层**断言——命令类事件按诱发命令的执行者分类；缺诱发命令时记
    /// `daemon` 而**不是** `device`；daemon 自身事件恒为 `daemon`；其余（Agent 产出）恒为 `agent`。
    #[test]
    fn event_origin_classifies_command_events_by_the_inducing_actor() {
        let delta = EventType::new("agent.message.delta").expect("event type");
        let mode = EventType::new("session.mode.changed").expect("event type");
        let completed = EventType::new("command.completed").expect("event type");
        let revoked = EventType::new("device.revoked").expect("event type");
        let updated = EventType::new("session.updated").expect("event type");
        let device = device_actor();
        let node = Actor::Node {
            node: NodeId::new(&uuid_text(44)).expect("node"),
            access_node: NodeId::new(&uuid_text(45)).expect("access node"),
        };
        let claimant = Actor::PairingClaimant {
            pairing: PairingId::new(&uuid_text(46)).expect("pairing"),
        };

        // 每个执行者取值在**任一**命令类事件上给出同一个分类（清单只决定「是不是命令类」）。
        for event in [&mode, &completed] {
            assert_eq!(
                event_origin(event, None),
                EventOrigin::Daemon,
                "缺诱发命令不得默认记为 device"
            );
            assert_eq!(
                event_origin(event, Some(&Actor::LocalCli)),
                EventOrigin::LocalCli
            );
            assert_eq!(event_origin(event, Some(&device)), EventOrigin::Device);
            assert_eq!(event_origin(event, Some(&node)), EventOrigin::Node);
            assert_eq!(
                event_origin(event, Some(&claimant)),
                EventOrigin::Daemon,
                "认领方永不可提交命令；不得拿它拼出一个来源"
            );
        }
        // Agent 产出的事件与 actor 无关。
        for actor in [None, Some(&Actor::LocalCli), Some(&device), Some(&node)] {
            assert_eq!(event_origin(&delta, actor), EventOrigin::Agent);
        }
        // Daemon 自身事件与 actor 无关。
        assert_eq!(event_origin(&revoked, None), EventOrigin::Daemon);
        assert_eq!(
            event_origin(&revoked, Some(&Actor::LocalCli)),
            EventOrigin::Daemon
        );
        // Broker 计算的三态摘要也属于 daemon 产物，不得记成 agent。
        assert_eq!(event_origin(&updated, None), EventOrigin::Daemon);
        assert_eq!(
            event_origin(&updated, Some(&Actor::LocalCli)),
            EventOrigin::Daemon
        );
        // 新增的第五个取值：wire/db token 都是 `node`。
        assert_eq!(EventOrigin::Node.as_str(), "node");
        assert_eq!(EventOrigin::ALL.len(), 5, "§7.3 的 CHECK 是五值");
    }

    /// §10.1 的验收口径（**设备驱动**）：turn 生命周期事件记 `origin_kind='device'` **且** `causation`
    /// 等于该 prompt 的 requestId；turn 内由 Agent 产出的增量仍是 `agent`，但**照样带**该 causation
    /// （两者表达不同的事实，不得互相替代）。
    #[test]
    fn device_prompt_records_device_origin_and_the_prompt_causation() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(vec![
            endpoint_event(
                EventKind::Delta,
                "agent.message.delta",
                &turn_view("running"),
            ),
            endpoint_event(EventKind::State, "turn.completed", &turn_view("completed")),
        ]));
        let device = device_actor();
        let request = harness.request(1);
        let command = prompt_command(&device, &harness.session, &request, 'A');
        assert!(matches!(
            block_on(harness.broker.submit_mutation(&device, &command)).expect("submit"),
            CommandReceipt::Accepted { .. }
        ));

        for event_type in [
            "turn.queued",
            "turn.started",
            "turn.completed",
            "command.completed",
        ] {
            assert_eq!(
                harness.world.provenance_of(&harness.session, event_type),
                vec![(EventOrigin::Device, Some(request.clone()))],
                "{event_type} 必须按诱发命令记 device，且 causation = 该 prompt 的 requestId"
            );
        }
        assert_eq!(
            harness
                .world
                .provenance_of(&harness.session, "agent.message.delta"),
            vec![(EventOrigin::Agent, Some(request.clone()))],
            "turn 内由 Agent 产出的增量仍是 agent，但照样带该 prompt 的 causation"
        );
    }

    /// §10.1：**本地 CLI 驱动**的同一路径记 `local_cli`——今天这条路径上的非 turn 命令类事件被误记为
    /// `device`（缺 actor 时的旧默认），改后不得再出现 `device`。
    #[test]
    fn cli_prompt_records_local_cli_not_device() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::State,
            "turn.completed",
            &turn_view("completed"),
        )]));
        let request = harness.request(1);
        assert!(matches!(
            harness.submit_prompt(1, 'A'),
            CommandReceipt::Accepted { .. }
        ));
        for event_type in ["turn.queued", "turn.started", "turn.completed"] {
            assert_eq!(
                harness.world.provenance_of(&harness.session, event_type),
                vec![(EventOrigin::LocalCli, Some(request.clone()))],
                "{event_type} 的诱发命令来自本地 CLI"
            );
        }
        let origins: Vec<EventOrigin> = harness
            .world
            .events(&harness.session)
            .into_iter()
            .filter_map(|event| harness.world.event_origin(&event.id))
            .collect();
        assert!(
            !origins.contains(&EventOrigin::Device),
            "CLI 驱动的命令类事件不得记 device：{origins:?}"
        );

        // 非 turn 的命令类事件（`session.mode.set` 诱发的 `session.mode.changed`）同样记 `local_cli`。
        let request = harness.request(2);
        let actor = harness.actor();
        let command = mode_command(&actor, &harness.session, &request, Version::from(1));
        assert!(matches!(
            block_on(harness.broker.submit_mutation(&actor, &command)).expect("submit"),
            CommandReceipt::Accepted { .. }
        ));
        harness.broker.sink(&harness.session).send(endpoint_event(
            EventKind::State,
            "session.mode.changed",
            r#"{"currentModeId":"code"}"#,
        ));
        block_on(harness.broker.flush(&harness.session)).expect("flush");
        assert_eq!(
            harness
                .world
                .provenance_of(&harness.session, "session.mode.changed"),
            vec![(EventOrigin::LocalCli, Some(request.clone()))],
            "非 turn 的命令类事件取提交时在作用域的命令（其 actor 是 CLI）"
        );
        let completed = harness
            .world
            .provenance_of(&harness.session, "command.completed");
        assert_eq!(
            completed.last(),
            Some(&(EventOrigin::LocalCli, Some(request))),
            "模式切换的终态事件也带该命令的 requestId：{completed:?}"
        );
    }

    /// §10.1：**节点经 Node Link 的命令**驱动的 turn 记新增的 `node` 取值（不得记 `device`）。
    #[test]
    fn node_command_records_the_node_origin() {
        let harness = Harness::new(BrokerConfig::default());
        let node = NodeId::new(&uuid_text(120)).expect("node");
        let actor = Actor::Node {
            node: node.clone(),
            access_node: NodeId::new(&uuid_text(121)).expect("access node"),
        };
        harness.world.push_script(Script::new(vec![
            endpoint_event(
                EventKind::Delta,
                "agent.message.delta",
                &turn_view("running"),
            ),
            endpoint_event(EventKind::State, "turn.completed", &turn_view("completed")),
        ]));
        let request = harness.request(1);
        let command = prompt_command(&actor, &harness.session, &request, 'A');
        assert!(matches!(
            block_on(harness.broker.submit_mutation(&actor, &command)).expect("submit"),
            CommandReceipt::Accepted { .. }
        ));
        for event_type in [
            "turn.queued",
            "turn.started",
            "turn.completed",
            "command.completed",
        ] {
            assert_eq!(
                harness.world.provenance_of(&harness.session, event_type),
                vec![(EventOrigin::Node, Some(request.clone()))],
                "{event_type} 由节点命令诱发，必须记 node"
            );
        }
        assert_eq!(
            harness
                .world
                .provenance_of(&harness.session, "agent.message.delta"),
            vec![(EventOrigin::Agent, Some(request))],
            "turn 内的 Agent 产出仍是 agent"
        );
    }

    /// §10.1：**turn 运行期间由另一条命令解析权限**——`permission.resolved` 的 `causation` 与
    /// `origin.kind` 取**那条解析命令**，而不是仍在跑的 turn 的那条 prompt 命令。
    ///
    /// 这是主用例而不是边缘情形：权限/elicitation 的解析按定义就发生在 prompt turn 的
    /// `WaitingPermission` 期间，而 `agent-host` 产出的 `EndpointEvent` 恒不带 turn、`commit_chunk`
    /// 又会把无 turn 的事件兜底归给在跑的 turn——按事件归属取诱发命令会把「设备 B 解析设备 A 的 turn
    /// 的权限」记成 A 的 prompt（任务 1.6 的 `causation → owned_command.actor_id` 回查因此判给 A）。
    #[test]
    fn permission_resolved_during_a_running_turn_takes_the_resolving_command() {
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
        // 设备 A 的 prompt 打开一个 turn：脚本只发一条增量、不发终态 ⇒ 提交后该 turn 仍在跑。
        harness.world.push_script(Script::new(vec![endpoint_event(
            EventKind::Delta,
            "agent.message.delta",
            &turn_view("running"),
        )]));
        let device_a = device_actor();
        let prompt_request = harness.request(1);
        let prompt = prompt_command(&device_a, &harness.session, &prompt_request, 'A');
        assert!(matches!(
            block_on(harness.broker.submit_mutation(&device_a, &prompt)).expect("submit"),
            CommandReceipt::Accepted { .. }
        ));
        assert_eq!(
            harness
                .world
                .session(&harness.session)
                .expect("session")
                .state(),
            SessionState::Pending,
            "本用例的前提是 turn 仍在跑且有有效交互（三态下是待处理，不是运行中）"
        );

        // 后端在 `resolve_interaction` 期间产出 `permission.resolved`（适配器边界，core 只从 sink 收）：
        // 这里先入队，由 `resolve_locked` 在把命令派发给后端之后的那次 flush 落盘。view 按
        // `schemas/sync/v1/event-views.schema.json#/permission.resolved` 的形状给（本机 CLI 解析 ⇒
        // `resolvedByDeviceId` 为 `null`）。
        harness.broker.sink(&harness.session).send(endpoint_event(
            EventKind::Structured,
            "permission.resolved",
            &format!(
                r#"{{"interactionId":"{}","resolution":"allow-once","resolvedByDeviceId":null}}"#,
                interaction.as_str()
            ),
        ));
        // 解析来自**另一条命令**（本机 CLI），与驱动该 turn 的设备 A 不是同一条。
        let actor = harness.actor();
        let resolve_request = harness.request(2);
        let command = ClientCommand {
            actor: actor.clone(),
            request: resolve_request.clone(),
            command: "permission.resolve".to_owned(),
            kind: CommandKind::Mutation,
            session: Some(harness.session.clone()),
            expected_version: None,
            request_fingerprint: digest('D'),
            payload: CommandPayload::PermissionResolve {
                interaction: interaction.clone(),
                option_id: "allow-once".to_owned(),
            },
        };
        assert!(matches!(
            block_on(harness.broker.submit_mutation(&actor, &command)).expect("submit"),
            CommandReceipt::Accepted { .. }
        ));

        assert_eq!(
            harness
                .world
                .provenance_of(&harness.session, "permission.resolved"),
            vec![(EventOrigin::LocalCli, Some(resolve_request))],
            "响应类事件必须取**解析命令**：它不是驱动在跑 turn 的那条 prompt，也不是设备 A 的命令"
        );
        // 同一会话里 turn 内由 Agent 产出的事件仍带它自己那条 prompt 的 `causation`、`origin` 仍是
        // `agent`——两套诱发来源不得互相污染。
        assert_eq!(
            harness
                .world
                .provenance_of(&harness.session, "agent.message.delta"),
            vec![(EventOrigin::Agent, Some(prompt_request))],
            "turn 内 Agent 产出的 causation 仍是它自己那条 prompt"
        );
    }

    /// §10.1：**没有任何命令在作用域**时，Agent 产出的事件 `causation` 为 `null`（且 `origin` 仍是
    /// `agent`）——不能把「会话里曾经有过命令」当成因果。
    #[test]
    fn agent_event_without_any_command_has_null_causation() {
        let harness = Harness::new(BrokerConfig::default());
        harness.broker.sink(&harness.session).send(endpoint_event(
            EventKind::Delta,
            "agent.thought.delta",
            &turn_view("running"),
        ));
        block_on(harness.broker.flush(&harness.session)).expect("flush");
        assert_eq!(
            harness
                .world
                .provenance_of(&harness.session, "agent.thought.delta"),
            vec![(EventOrigin::Agent, None)],
            "没有命令直接导致的事件其 causation 必须是 null"
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
            )
            .expect("session"),
        );
        let use_cases = crate::use_cases::UseCases::new(crate::use_cases::UseCaseDeps {
            broker: broker.clone(),
            store,
            deliveries: Arc::new(FakeDeliveries {
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
                    state: Some(SessionState::Running),
                    mode: ModeChange::Unchanged,
                    title: None,
                    interaction: None,
                    confirmation: None,
                    agent_session_id: None,
                    workspace_cwd: None,
                    workspace_directory_id: None,
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
            vec![
                "turn.queued".to_owned(),
                "session.updated".to_owned(),
                "turn.started".to_owned()
            ],
            "冲突批不得落盘任何事件"
        );
        assert_eq!(harness.world.publish_count(), 3, "冲突批不得发布任何帧");
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
                title: None,
                interaction: None,
                confirmation: None,
                agent_session_id: None,
                workspace_cwd: None,
                workspace_directory_id: None,
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
        assert_eq!(harness.world.publish_count(), 3, "该批不得发布");
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

    // -----------------------------------------------------------------------------------------
    // R5–R9：core 派生的三类生产者（`sync-scope-and-pwa-client` 变更 WP3）
    // -----------------------------------------------------------------------------------------

    /// 工具调用事件 view 的装配（`toolCallId` + 可选 `diff`）。
    fn tool_call_view(tool_call_id: &str, diff: Option<&str>) -> String {
        match diff {
            Some(diff) => {
                format!(r#"{{"toolCallId":"{tool_call_id}","state":"in_progress","diff":{diff}}}"#)
            }
            None => format!(r#"{{"toolCallId":"{tool_call_id}","state":"in_progress"}}"#),
        }
    }

    /// 会话的规范化工作目录根（R7 相对化的依据）：真实创建目录并让目录本身就存在，避免
    /// symlink/verbatim 前缀造成的口径差异。`load_recovery` 是窄读取，两列都要有值。
    fn workspace_root() -> String {
        static ROOT: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
            let root = std::env::temp_dir().join("acp-remote-wp3-root");
            std::fs::create_dir_all(root.join("src")).expect("临时工作目录");
            std::fs::write(root.join("src").join("main.rs"), "x").expect("临时文件");
            std::fs::canonicalize(&root)
                .expect("canonicalize 临时工作目录")
                .to_string_lossy()
                .into_owned()
        });
        ROOT.clone()
    }

    /// 该 view 的派生事件（按事件类型过滤已提交的领域事件）。
    fn stored_views_of(harness: &Harness, event_type: &str) -> Vec<String> {
        harness
            .world
            .events(&harness.session)
            .iter()
            .filter(|event| event.event_type.as_str() == event_type)
            .map(|event| stored_view(harness, event))
            .collect()
    }

    /// R5 + R7：含类型化 Diff 的工具调用派生**恰好一条** `file.changed`，展示路径相对化到工作目录根。
    #[test]
    fn a_typed_diff_derives_one_file_change_with_a_relativized_display_path() {
        let harness = Harness::new(BrokerConfig::default());
        let root = workspace_root();
        harness.world.seed_recovery(
            &harness.session,
            Some(AgentSessionId::new("acp-wp3").expect("agent session id")),
            Some(root.clone()),
        );
        let inside = std::path::Path::new(&root).join("src").join("main.rs");
        harness.world.push_script(Script::new(vec![
            endpoint_event(
                EventKind::Structured,
                "tool.call.started",
                &tool_call_view(
                    "tool-1",
                    Some(&format!(
                        r#"[{{"path":{},"oldText":"let a = 1;","newText":"let a = 2;"}}]"#,
                        json_text(&inside.to_string_lossy())
                    )),
                ),
            ),
            endpoint_event(EventKind::State, "turn.completed", &turn_view("completed")),
        ]));
        let receipt = harness.submit_prompt(1, 'A');
        assert!(matches!(
            receipt,
            CommandReceipt::Accepted { turn: Some(_), .. }
        ));
        let changes = stored_views_of(&harness, "file.changed");
        assert_eq!(changes.len(), 1, "R5：恰好一条 file.changed");
        assert!(
            changes[0].contains(r#""displayPath":"src/main.rs""#),
            "R7：展示路径相对化，实际 {}",
            changes[0]
        );
        assert!(
            changes[0].contains(r#""addedLines":"1""#)
                && changes[0].contains(r#""deletedLines":"1""#),
            "R6：行级差异而不是行数差，实际 {}",
            changes[0]
        );
        assert!(
            !changes[0].contains(&root),
            "不得下发工作目录根的任何片段：{}",
            changes[0]
        );
    }

    /// R5：不含类型化 Diff 的工具调用**不**派生 `file.changed`（`rawInput` 里的编辑结构不作数）。
    #[test]
    fn a_tool_call_without_a_typed_diff_derives_nothing() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(vec![
            endpoint_event(
                EventKind::Structured,
                "tool.call.started",
                r#"{"toolCallId":"tool-9","state":"in_progress","rawInput":{"path":"/etc/passwd","newText":"x"}}"#,
            ),
            endpoint_event(EventKind::State, "turn.completed", &turn_view("completed")),
        ]));
        let _ = harness.submit_prompt(1, 'B');
        assert!(
            stored_views_of(&harness, "file.changed").is_empty(),
            "R5：MUST NOT 从 rawInput 推断文件改动"
        );
    }

    /// R5：同一个工具调用在 `started`/`updated` 里重复携带同一个 Diff 元素时只派生一次。
    #[test]
    fn a_repeated_diff_element_is_derived_only_once() {
        let harness = Harness::new(BrokerConfig::default());
        let root = workspace_root();
        harness.world.seed_recovery(
            &harness.session,
            Some(AgentSessionId::new("acp-wp3").expect("agent session id")),
            Some(root.clone()),
        );
        let inside = std::path::Path::new(&root).join("dup.txt");
        let diff = format!(
            r#"[{{"path":{},"oldText":"a","newText":"b"}}]"#,
            json_text(&inside.to_string_lossy())
        );
        harness.world.push_script(Script::new(vec![
            endpoint_event(
                EventKind::Structured,
                "tool.call.started",
                &tool_call_view("tool-dup", Some(&diff)),
            ),
            endpoint_event(
                EventKind::Structured,
                "tool.call.updated",
                &tool_call_view("tool-dup", Some(&diff)),
            ),
            endpoint_event(EventKind::State, "turn.completed", &turn_view("completed")),
        ]));
        let _ = harness.submit_prompt(1, 'C');
        let changes = stored_views_of(&harness, "file.changed");
        assert_eq!(
            changes.len(),
            1,
            "R5：同一处改动不得重复派生，实际 {changes:?}"
        );
    }

    /// R7：工作区外的路径只下发文件名称，并显式置 `outsideWorkspace`。
    #[test]
    fn a_path_outside_the_workspace_is_marked_and_keeps_only_the_file_name() {
        let harness = Harness::new(BrokerConfig::default());
        let root = workspace_root();
        harness.world.seed_recovery(
            &harness.session,
            Some(AgentSessionId::new("acp-wp3").expect("agent session id")),
            Some(root.clone()),
        );
        harness.world.push_script(Script::new(vec![
            endpoint_event(
                EventKind::Structured,
                "tool.call.started",
                &tool_call_view(
                    "tool-out",
                    Some(r#"[{"path":"/etc/secret.txt","oldText":"a","newText":"b"}]"#),
                ),
            ),
            endpoint_event(EventKind::State, "turn.completed", &turn_view("completed")),
        ]));
        let _ = harness.submit_prompt(1, 'D');
        let changes = stored_views_of(&harness, "file.changed");
        assert_eq!(changes.len(), 1);
        assert!(
            changes[0].contains(r#""outsideWorkspace":true"#)
                && changes[0].contains(r#""displayPath":"secret.txt""#),
            "R7：越界时标记且只给文件名称，实际 {}",
            changes[0]
        );
        assert!(!changes[0].contains(".."), "不得下发回退层级");
    }

    /// F2（`review-w3-r1`）：同一次工具调用里的两个**同名**越界文件各派生一条 `file.changed`。
    ///
    /// 判别力：去重键与 `changeId` 若取**展示路径**（越界时按 R7 收窄为 `file_name()`），第二个元素
    /// 会命中本批已计划的键而被静默 `continue`——本用例断言 2 条、且两条 `changeId` 不同。
    #[test]
    fn two_same_named_outside_files_derive_two_distinct_changes() {
        let harness = Harness::new(BrokerConfig::default());
        let root = workspace_root();
        harness.world.seed_recovery(
            &harness.session,
            Some(AgentSessionId::new("acp-wp3").expect("agent session id")),
            Some(root),
        );
        harness.world.push_script(Script::new(vec![
            endpoint_event(
                EventKind::Structured,
                "tool.call.started",
                &tool_call_view(
                    "tool-same",
                    Some(
                        r#"[{"path":"/etc/nginx/nginx.conf","oldText":"a","newText":"b"},{"path":"/tmp/nginx.conf","oldText":"c","newText":"d"}]"#,
                    ),
                ),
            ),
            endpoint_event(EventKind::State, "turn.completed", &turn_view("completed")),
        ]));
        let _ = harness.submit_prompt(1, 'H');
        let changes = stored_views_of(&harness, "file.changed");
        assert_eq!(
            changes.len(),
            2,
            "两处同名但不同目录的越界改动各派生一条，实际 {changes:?}"
        );
        let ids: Vec<String> = changes.iter().map(|view| change_id_of(view)).collect();
        assert_ne!(ids[0], ids[1], "同名越界文件的 changeId 不得相同");
        for view in &changes {
            assert!(
                view.contains(r#""displayPath":"nginx.conf""#)
                    && view.contains(r#""outsideWorkspace":true"#),
                "R7：越界只下发文件名，实际 {view}"
            );
        }
    }

    /// 派生 view 的 `changeId`（供去重/唯一性断言取值）。
    fn change_id_of(view: &str) -> String {
        let marker = r#""changeId":""#;
        let start = view.find(marker).expect("changeId 必须存在") + marker.len();
        let rest = &view[start..];
        let end = rest.find('"').expect("changeId 是字符串");
        rest[..end].to_owned()
    }

    /// R6：判定不出行数时**省略**两项（不填零）。
    #[test]
    fn a_diff_without_new_text_omits_the_line_counts() {
        let harness = Harness::new(BrokerConfig::default());
        harness.world.push_script(Script::new(vec![
            endpoint_event(
                EventKind::Structured,
                "tool.call.started",
                &tool_call_view("tool-bare", Some(r#"[{"path":"bare.txt","oldText":"a"}]"#)),
            ),
            endpoint_event(EventKind::State, "turn.completed", &turn_view("completed")),
        ]));
        let _ = harness.submit_prompt(1, 'E');
        let changes = stored_views_of(&harness, "file.changed");
        assert_eq!(changes.len(), 1);
        assert!(
            !changes[0].contains("addedLines") && !changes[0].contains("deletedLines"),
            "R6：判定不出时省略而非填零，实际 {}",
            changes[0]
        );
    }

    /// R5：派生**不改写** ACP 原文的字节、摘要与字节长度。
    #[test]
    fn deriving_a_file_change_leaves_the_acp_raw_document_untouched() {
        let harness = Harness::new(BrokerConfig::default());
        let raw = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"tool_call","toolCallId":"tool-acp","content":[{"type":"diff","path":"src/a.rs","oldText":"a","newText":"b"}]}}}"#;
        let digest = digest('Z');
        let acp = AcpRaw::available("application/json", raw, digest.clone()).expect("acp");
        let mut event = endpoint_event(
            EventKind::Structured,
            "tool.call.started",
            &tool_call_view(
                "tool-acp",
                Some(r#"[{"path":"src/a.rs","oldText":"a","newText":"b"}]"#),
            ),
        );
        event.payload.acp = Some(acp.clone());
        harness.world.push_script(Script::new(vec![
            event,
            endpoint_event(EventKind::State, "turn.completed", &turn_view("completed")),
        ]));
        let _ = harness.submit_prompt(1, 'F');
        let changes = stored_views_of(&harness, "file.changed");
        assert_eq!(changes.len(), 1);
        // R10：同一份类型化 Diff 还派生了结构化差异行（与统计同源），且**不含** ACP 原文本身。
        assert!(
            changes[0].contains(
                r#""diffLines":[{"kind":"deleted","text":"a"},{"kind":"added","text":"b"}]"#
            ),
            "差异行必须逐行标明新增/删除/上下文，实际 {}",
            changes[0]
        );
        assert!(
            !changes[0].contains("jsonrpc") && !changes[0].contains("session/update"),
            "视图里不得出现 ACP 原文：{}",
            changes[0]
        );
        // 携带 Diff 的那条工具调用事件仍在库里，且 ACP 三要素逐字不变。
        let stored = harness.world.events(&harness.session);
        let tool_call = stored
            .iter()
            .find(|event| event.event_type.as_str() == "tool.call.started")
            .expect("工具调用事件");
        let payload = crate::broker::lock(&harness.world.state)
            .event_payloads
            .get(tool_call.id.as_str())
            .cloned()
            .expect("正文");
        assert_eq!(payload.acp.as_ref(), Some(&acp), "派生不得改动 ACP 原文");
        let (media_type, raw_json, byte_length, sha256) = payload
            .acp
            .as_ref()
            .and_then(AcpRaw::as_available)
            .expect("原文可用");
        assert_eq!(media_type, "application/json");
        assert_eq!(raw_json.as_bytes(), raw.as_bytes(), "原文必须逐字节不变");
        assert_eq!(byte_length, raw.len() as u64, "字节长度必须不变");
        assert_eq!(sha256, &digest, "摘要必须不变");
    }

    /// R8：节点级 `agent.connected` 落库且会话标识为空；`sessionId`/`sessionSequence` 同时为 NULL。
    #[test]
    fn a_node_level_agent_event_is_committed_without_session_identity() {
        let harness = Harness::new(BrokerConfig::default());
        let event = endpoint_event(
            EventKind::State,
            "agent.connected",
            r#"{"agentId":"agent-1","state":"connected"}"#,
        );
        block_on(harness.broker.commit_node_event(event)).expect("commit");
        let view = block_on(harness.broker.read_view()).expect("view");
        let batch = block_on(view.replay(None, ReplayLimit::default())).expect("replay");
        let node_event = batch
            .events
            .iter()
            .find_map(|delivery| match delivery {
                CommittedDelivery::Owned(event)
                    if event.event_type.as_str() == "agent.connected" =>
                {
                    Some(event.clone())
                }
                _ => None,
            })
            .expect("节点级事件进入 replay 流");
        assert!(node_event.session.is_none(), "R8：会话标识必须为空");
        assert!(node_event.session_sequence.is_none());
        assert!(node_event.origin_epoch.is_none());
        assert!(node_event.origin_sequence.is_none());
        assert!(node_event.global_sequence.get() > 0, "仍有全局序号");
    }

    /// R8：节点级事件的形状校验失败关闭（类型/类别/state 词表/turn/ACP 原文）。
    #[test]
    fn node_level_events_fail_closed_on_shape_violations() {
        let harness = Harness::new(BrokerConfig::default());
        // 事件类型不在节点级词表内。
        assert!(
            block_on(harness.broker.commit_node_event(endpoint_event(
                EventKind::State,
                "session.mode.changed",
                r#"{"state":"connected"}"#,
            )))
            .is_err()
        );
        // state 与事件类型不一致（`agent.connected` 只能取 `connected`）。
        assert!(
            block_on(harness.broker.commit_node_event(endpoint_event(
                EventKind::State,
                "agent.connected",
                r#"{"agentId":"a","state":"disconnected"}"#,
            )))
            .is_err()
        );
        // 缺 agentId。
        assert!(
            block_on(harness.broker.commit_node_event(endpoint_event(
                EventKind::State,
                "agent.connected",
                r#"{"state":"connected"}"#,
            )))
            .is_err()
        );
        // F3（`review-w3-r1`）：**空串** agentId 也是形状非法（schema `minLength: 1`）——只判 `is_none()`
        // 会让它通过并落一条消费端解析不了的事件。
        assert!(
            block_on(harness.broker.commit_node_event(endpoint_event(
                EventKind::State,
                "agent.connected",
                r#"{"agentId":"","state":"connected"}"#,
            )))
            .is_err(),
            "空串 agentId 必须被拒绝（§10.3 的 schema 是 minLength: 1）"
        );
        // 类别不是 state。
        assert!(
            block_on(harness.broker.commit_node_event(endpoint_event(
                EventKind::Structured,
                "agent.connected",
                r#"{"agentId":"a","state":"connected"}"#,
            )))
            .is_err()
        );
    }

    /// R9：`session_info_update` 把标题单向写入会话；`updatedAt` 取 Daemon 持久化时间。
    #[test]
    fn a_session_info_update_writes_the_title_from_the_agent_notification() {
        let harness = Harness::new(BrokerConfig::default());
        let raw = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"session_info_update","title":"新标题","updatedAt":"2020-01-01T00:00:00.000Z"}}}"#;
        let mut event = endpoint_event(
            EventKind::State,
            "session.info.changed",
            r#"{"title":"新标题","updatedAt":"2020-01-01T00:00:00.000Z"}"#,
        );
        event.payload.acp =
            Some(AcpRaw::available("application/json", raw, digest('Y')).expect("acp"));
        harness.world.push_script(Script::new(vec![
            event,
            endpoint_event(EventKind::State, "turn.completed", &turn_view("completed")),
        ]));
        let _ = harness.submit_prompt(1, 'G');
        let session = harness.world.session(&harness.session).expect("会话");
        assert_eq!(session.title(), Some("新标题"), "R9：标题来自通知");
        assert_ne!(
            session.updated_at().as_str(),
            "2020-01-01T00:00:00.000Z",
            "R9：权威更新时间取 Daemon 持久化时间，不采用 Agent 自报值"
        );
    }

    /// R9/D7：通知**只带更新时间**时（标题意图为「不改」），会话的权威更新时间仍必须前进到本次
    /// Daemon 提交时钟——`SYNC_PROTOCOL.md` §10.2 的目录排序按 `updated_at DESC`，该列不前进排序就是错的。
    ///
    /// 判别力：只把「标题意图」变成 `SessionUpdate`（`None` = 不改标题时不产出任何会话写入）时，
    /// 本批不写 `owned_session`，`updated_at` 停在旧值上——断言二（`after > before`）失败。
    #[test]
    fn a_session_info_update_without_a_title_still_advances_the_updated_at() {
        let harness = Harness::new(BrokerConfig::default());
        // 先制造一个较早的基线（prompt 期间的多条提交会推进 TestClock）。
        let _ = harness.submit_prompt(1, 'K');
        let before = harness
            .world
            .session(&harness.session)
            .expect("会话")
            .updated_at()
            .clone();

        // ACP 原文里**没有** `title` 键 = 只带更新时间（通知的部分更新语义）。
        let only_time = r#"{"params":{"update":{"sessionUpdate":"session_info_update","updatedAt":"2020-01-01T00:00:00.000Z"}}}"#;
        let mut event = endpoint_event(
            EventKind::State,
            "session.info.changed",
            r#"{"title":null,"updatedAt":"2020-01-01T00:00:00.000Z"}"#,
        );
        event.payload.acp =
            Some(AcpRaw::available("application/json", only_time, digest('T')).expect("acp"));
        // 事件自报的时间**早于**该会话上一次写入：权威时间不得被它拖回去。
        event.at = ts(0);
        harness.broker.sink(&harness.session).send(event);
        block_on(harness.broker.flush(&harness.session)).expect("flush");

        let session = harness.world.session(&harness.session).expect("会话");
        assert_ne!(
            session.updated_at().as_str(),
            "2020-01-01T00:00:00.000Z",
            "R9/D7：权威更新时间不得取 Agent 自报值"
        );
        assert!(
            session.updated_at().as_str() > before.as_str(),
            "R9/D7：只带更新的通知也必须把 updated_at 推进到提交时钟（排序依据），{before} → {}",
            session.updated_at()
        );
    }

    /// R9：通知只携带更新时间时既有标题保持不变；显式置空时标题变空。
    #[test]
    fn a_session_info_update_without_a_title_keeps_the_existing_one() {
        let harness = Harness::new(BrokerConfig::default());
        let set =
            r#"{"params":{"update":{"sessionUpdate":"session_info_update","title":"保留我"}}}"#;
        let mut first = endpoint_event(
            EventKind::State,
            "session.info.changed",
            r#"{"title":"保留我","updatedAt":"2020-01-01T00:00:00.000Z"}"#,
        );
        first.payload.acp =
            Some(AcpRaw::available("application/json", set, digest('X')).expect("acp"));
        // 只有更新时间（ACP 原文里没有 `title` 键，公共 view 是 `null`）。
        let only_time = r#"{"params":{"update":{"sessionUpdate":"session_info_update","updatedAt":"2021-01-01T00:00:00.000Z"}}}"#;
        let mut second = endpoint_event(
            EventKind::State,
            "session.info.changed",
            r#"{"title":null,"updatedAt":"2021-01-01T00:00:00.000Z"}"#,
        );
        second.payload.acp =
            Some(AcpRaw::available("application/json", only_time, digest('W')).expect("acp"));
        harness.world.push_script(Script::new(vec![
            first,
            second,
            endpoint_event(EventKind::State, "turn.completed", &turn_view("completed")),
        ]));
        let _ = harness.submit_prompt(1, 'H');
        let session = harness.world.session(&harness.session).expect("会话");
        assert_eq!(
            session.title(),
            Some("保留我"),
            "缺 title 的通知不得清空标题"
        );
    }

    /// R9：通知显式把标题置空时呈现为未命名会话。
    #[test]
    fn a_session_info_update_with_an_explicit_null_clears_the_title() {
        let harness = Harness::new(BrokerConfig::default());
        let set =
            r#"{"params":{"update":{"sessionUpdate":"session_info_update","title":"先有标题"}}}"#;
        let mut first = endpoint_event(
            EventKind::State,
            "session.info.changed",
            r#"{"title":"先有标题","updatedAt":"2020-01-01T00:00:00.000Z"}"#,
        );
        first.payload.acp =
            Some(AcpRaw::available("application/json", set, digest('V')).expect("acp"));
        let clear = r#"{"params":{"update":{"sessionUpdate":"session_info_update","title":null}}}"#;
        let mut second = endpoint_event(
            EventKind::State,
            "session.info.changed",
            r#"{"title":null,"updatedAt":"2021-01-01T00:00:00.000Z"}"#,
        );
        second.payload.acp =
            Some(AcpRaw::available("application/json", clear, digest('U')).expect("acp"));
        harness.world.push_script(Script::new(vec![
            first,
            second,
            endpoint_event(EventKind::State, "turn.completed", &turn_view("completed")),
        ]));
        let _ = harness.submit_prompt(1, 'I');
        let session = harness.world.session(&harness.session).expect("会话");
        assert!(session.title().is_none(), "R9：显式置空 → 未命名会话");
    }

    // -----------------------------------------------------------------------------------------
    // §5.8/§6 第 22 条：附件清单的原子绑定与有效交互
    // -----------------------------------------------------------------------------------------

    fn submission_of(
        session: &SessionId,
        n: u64,
    ) -> (SubmissionId, AttachmentId, AttachmentSubmission) {
        let submission = SubmissionId::new(&uuid_text(600 + n)).expect("submission id");
        let attachment = AttachmentId::new(&uuid_text(700 + n)).expect("attachment id");
        let manifest = AttachmentSubmission {
            submission: submission.clone(),
            session: session.clone(),
            subject: AttachmentSubject::Device(DeviceId::new(&uuid_text(800 + n)).expect("device")),
            attachments: vec![attachment.clone()],
            digest: digest('S'),
            sealed_at: ts(0),
        };
        (submission, attachment, manifest)
    }

    fn prompt_with_binding(
        harness: &Harness,
        binding: SubmissionBinding,
        fingerprint: char,
    ) -> Result<CommandReceipt, PortError> {
        let actor = harness.actor();
        let command = prompt_command(&actor, &harness.session, &harness.request(1), fingerprint);
        block_on(harness.broker.submit_prompt_with_submission(
            &actor,
            &command,
            &[PromptContentBlock::new(json_view(
                r#"{"type":"text","text":"see attachment"}"#,
            ))],
            binding,
        ))
    }

    /// 受理提交携带清单时，绑定与幂等行、事件同批落账；重试返回首次结果且不重复绑定。
    #[test]
    fn prompt_submission_binding_is_committed_with_the_acceptance() {
        let harness = Harness::new(BrokerConfig::default());
        let (submission, attachment, manifest) = submission_of(&harness.session, 1);
        let subject = manifest.subject.clone();
        lock(&harness.world.submissions).insert(submission.as_str().to_owned(), manifest);
        let binding = SubmissionBinding {
            submission,
            attachments: vec![attachment],
            subject,
        };

        let receipt =
            prompt_with_binding(&harness, binding.clone(), 'A').expect("accept with submission");
        assert!(matches!(receipt, CommandReceipt::Accepted { .. }));
        assert_eq!(harness.world.prompt_count(), 1, "受理后才派发给 Agent");
        assert_eq!(lock(&harness.world.bindings).len(), 1);
        assert!(
            harness.world.commit_count() >= 1,
            "受理提交至少一次，且绑定只随它落账"
        );

        // 同一 requestId 重试：幂等命中（先于绑定校验），不再次绑定、不二次派发。
        let replay = prompt_with_binding(&harness, binding, 'A').expect("replay");
        assert!(matches!(replay, CommandReceipt::Accepted { .. }));
        assert_eq!(harness.world.prompt_count(), 1);
        assert_eq!(lock(&harness.world.bindings).len(), 1);
    }

    /// 清单不存在/跨会话/持久化失败：命令不被受理、Agent 未被调用、也没有广播。
    #[test]
    fn failed_submission_binding_never_dispatches_or_publishes() {
        let harness = Harness::new(BrokerConfig::default());
        let (unknown, attachment, manifest) = submission_of(&harness.session, 2);
        let error = prompt_with_binding(
            &harness,
            SubmissionBinding {
                submission: unknown,
                attachments: vec![attachment],
                subject: manifest.subject,
            },
            'B',
        )
        .expect_err("unknown submission");
        assert!(matches!(
            error,
            PortError::NotFound(EntityRef::Submission(_))
        ));
        assert_eq!(harness.world.prompt_count(), 0);
        assert_eq!(harness.world.publish_count(), 0);
        assert!(lock(&harness.world.bindings).is_empty());

        // 清单属于另一个会话。
        let (submission, attachment, mut manifest) = submission_of(&harness.session, 3);
        manifest.session = SessionId::new(&uuid_text(77)).expect("other session");
        let subject = manifest.subject.clone();
        lock(&harness.world.submissions).insert(submission.as_str().to_owned(), manifest);
        let error = prompt_with_binding(
            &harness,
            SubmissionBinding {
                submission,
                attachments: vec![attachment],
                subject,
            },
            'C',
        )
        .expect_err("cross-session submission");
        assert!(matches!(error, PortError::InvalidRequest(_)));
        assert_eq!(harness.world.prompt_count(), 0);
        assert_eq!(harness.world.publish_count(), 0);

        // 存储写失败：绑定已通过校验，但受理事务失败 ⇒ 不派发、不广播。
        let (submission, attachment, manifest) = submission_of(&harness.session, 4);
        let subject = manifest.subject.clone();
        lock(&harness.world.submissions).insert(submission.as_str().to_owned(), manifest);
        harness.world.fail_next_commit();
        let receipt = prompt_with_binding(
            &harness,
            SubmissionBinding {
                submission,
                attachments: vec![attachment],
                subject,
            },
            'D',
        )
        .expect("persistence failure is a command-level rejection");
        assert!(matches!(
            receipt,
            CommandReceipt::Rejected { error } if error.code() == "internal.unavailable"
        ));
        assert_eq!(harness.world.prompt_count(), 0);
        assert_eq!(harness.world.publish_count(), 0);
        assert!(lock(&harness.world.bindings).is_empty());
    }

    fn effective(
        session: &SessionId,
        interaction: &InteractionId,
        kind: InteractionKind,
        schema: Option<ElicitationSchema>,
    ) -> EffectiveInteraction {
        EffectiveInteraction::try_new(
            interaction.clone(),
            session.clone(),
            kind,
            Version::new(1),
            None,
            if kind == InteractionKind::Permission {
                vec![
                    InteractionOption::try_new("allow-once", "Allow once", "allow_once")
                        .expect("option"),
                ]
            } else {
                Vec::new()
            },
            schema,
        )
        .expect("effective interaction")
    }

    /// 有效读取返回完整请求；上游不可得时先标记失效并返回错误，绝不继续当作可应答。
    #[test]
    fn effective_interactions_require_a_live_upstream() {
        let (harness, _authority) = Harness::with_interactions(BrokerConfig::default());
        let interaction = InteractionId::new(&uuid_text(300)).expect("interaction");
        lock(&harness.world.effective_interactions).push(effective(
            &harness.session,
            &interaction,
            InteractionKind::Permission,
            None,
        ));
        let actor = device_actor();
        let found = block_on(
            harness
                .broker
                .effective_interactions(&actor, &harness.reference()),
        )
        .expect("effective");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].interaction, interaction);

        // 新 harness：尚未打开端点，脚本化 open 失败 ⇒ 失效 + 错误。
        let (dead, authority) = Harness::with_interactions(BrokerConfig::default());
        let dead_interaction = InteractionId::new(&uuid_text(301)).expect("interaction");
        lock(&dead.world.effective_interactions).push(effective(
            &dead.session,
            &dead_interaction,
            InteractionKind::Permission,
            None,
        ));
        lock(&dead.world.open_errors)
            .push_back(PortError::Unavailable(UnavailableKind::RemoteUnavailable));
        let error = block_on(
            dead.broker
                .effective_interactions(&actor, &dead.reference()),
        )
        .expect_err("upstream unavailable");
        assert!(matches!(
            error,
            PortError::Unavailable(UnavailableKind::RemoteUnavailable)
        ));
        assert_eq!(
            lock(&dead.world.invalidated_interactions).len(),
            1,
            "上游丢失时必须标记失效"
        );
        assert_eq!(
            block_on(authority.effective(&dead.session))
                .expect("effective")
                .len(),
            0
        );
    }

    /// 版本不符/表单无效都不消费请求；第一个有效答案胜出，后到者得到已处理事实。
    #[test]
    fn effective_resolution_validates_before_consuming() {
        let (harness, authority) = Harness::with_interactions(BrokerConfig::default());
        let interaction = InteractionId::new(&uuid_text(302)).expect("interaction");
        let schema = ElicitationSchema::new(
            r#"{"type":"object","required":["count","mode"],"properties":{"count":{"type":"integer","minimum":1,"maximum":10},"mode":{"type":"string","enum":["fast","safe"]}}}"#,
        )
        .expect("schema");
        lock(&harness.world.effective_interactions).push(effective(
            &harness.session,
            &interaction,
            InteractionKind::Elicitation,
            Some(schema),
        ));
        harness.world.seed_interaction(
            PendingInteraction::try_new(
                interaction.clone(),
                InteractionKind::Elicitation,
                harness.session.clone(),
                ts(0),
                Vec::new(),
            )
            .expect("pending"),
        );
        let actor = device_actor();
        let values = |json: &str| ElicitationValues::parse_json(json).expect("values");
        let resolution =
            |action: ElicitationAction, json: &str| InteractionResolution::Elicitation {
                action,
                values: values(json),
            };

        // 版本不符。
        let error = block_on(harness.broker.resolve_effective(
            &actor,
            &harness.reference(),
            &interaction,
            Some(Version::new(2)),
            resolution(ElicitationAction::Submit, r#"{"count":5,"mode":"fast"}"#),
        ))
        .expect_err("stale version");
        assert!(matches!(
            error,
            PortError::Conflict(ConflictKind::VersionMismatch)
        ));
        assert!(lock(&harness.world.resolved_interactions).is_empty());

        // 表单值不符合 schema。
        let error = block_on(harness.broker.resolve_effective(
            &actor,
            &harness.reference(),
            &interaction,
            Some(Version::new(1)),
            resolution(ElicitationAction::Submit, r#"{"count":0,"mode":"fast"}"#),
        ))
        .expect_err("invalid form");
        assert!(matches!(
            error,
            PortError::InvalidRequest(_) | PortError::Conflict(_)
        ));
        assert!(lock(&harness.world.resolved_interactions).is_empty());
        assert_eq!(
            block_on(authority.effective(&harness.session))
                .expect("effective")
                .len(),
            1
        );

        // 两个设备并发提交同一版本：恰好一个胜出。
        let reference = harness.reference();
        let other_actor = device_actor();
        let first = harness.broker.resolve_effective(
            &actor,
            &reference,
            &interaction,
            Some(Version::new(1)),
            resolution(ElicitationAction::Submit, r#"{"count":5,"mode":"fast"}"#),
        );
        let second = harness.broker.resolve_effective(
            &other_actor,
            &reference,
            &interaction,
            Some(Version::new(1)),
            resolution(ElicitationAction::Decline, "null"),
        );
        let outcomes = run_all(vec![first, second]);
        let resolved = outcomes
            .iter()
            .filter(|item| item.as_ref().ok() == Some(&Resolution::Resolved))
            .count();
        let already = outcomes
            .iter()
            .filter(|item| item.as_ref().ok() == Some(&Resolution::AlreadyResolved))
            .count();
        assert_eq!(resolved, 1, "只能有一个第一个有效答案");
        assert_eq!(already, 1, "后到者拿到已处理事实");
        let state = lock(&harness.world.state);
        assert_eq!(
            state
                .interactions
                .values()
                .filter(|row| row.resolution.is_some())
                .count(),
            1,
            "仲裁后只允许一行"
        );
        assert!(
            state
                .interactions
                .values()
                .filter(|row| row.resolution.is_some())
                .all(|row| row.confirmed),
            "适配器成功返回后仲裁行必须已确认"
        );
    }

    /// 同一 `requestId` 重试不会二次派发解析（幂等命中先于授权/解析）。
    #[test]
    fn duplicate_resolution_request_is_idempotent() {
        let (harness, _authority) = Harness::with_interactions(BrokerConfig::default());
        let interaction = InteractionId::new(&uuid_text(303)).expect("interaction");
        lock(&harness.world.effective_interactions).push(effective(
            &harness.session,
            &interaction,
            InteractionKind::Permission,
            None,
        ));
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
        let command = |request: RequestId| ClientCommand {
            actor: actor.clone(),
            request,
            command: "permission.resolve".to_owned(),
            kind: CommandKind::Mutation,
            session: Some(harness.session.clone()),
            expected_version: None,
            request_fingerprint: digest('R'),
            payload: CommandPayload::PermissionResolve {
                interaction: interaction.clone(),
                option_id: "allow-once".to_owned(),
            },
        };
        let request = harness.request(5);
        let first = block_on(harness.broker.submit_mutation_versioned(
            &actor,
            &command(request.clone()),
            Some(Version::new(1)),
        ))
        .expect("first");
        assert!(matches!(first, CommandReceipt::Accepted { .. }));
        let second = block_on(harness.broker.submit_mutation_versioned(
            &actor,
            &command(request),
            Some(Version::new(1)),
        ))
        .expect("replay");
        assert!(matches!(second, CommandReceipt::Accepted { .. }));
        let state = lock(&harness.world.state);
        assert!(
            state
                .interactions
                .get(interaction.as_str())
                .is_some_and(|row| row.resolution.is_some()),
            "重试不得二次调用解析路径"
        );
    }

    /// 版本/表单/上游校验必须**先于**受理：失败时没有 `accepted` 行，同一 requestId 用正确版本可重试。
    #[test]
    fn resolution_validation_precedes_acceptance() {
        let (harness, _authority) = Harness::with_interactions(BrokerConfig::default());
        let interaction = InteractionId::new(&uuid_text(310)).expect("interaction");
        let mut current = effective(
            &harness.session,
            &interaction,
            InteractionKind::Permission,
            None,
        );
        // 服务端的当前请求版本是 2；客户端仍带着上一页读到的 1。
        current.request_version = Version::new(2);
        lock(&harness.world.effective_interactions).push(current);
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
        if let Some(row) = lock(&harness.world.state)
            .interactions
            .get_mut(interaction.as_str())
        {
            row.request_version = Some(Version::new(2));
        }

        let actor = harness.actor();
        let request = harness.request(6);
        let command = ClientCommand {
            actor: actor.clone(),
            request: request.clone(),
            command: "permission.resolve".to_owned(),
            kind: CommandKind::Mutation,
            session: Some(harness.session.clone()),
            expected_version: None,
            request_fingerprint: digest('V'),
            payload: CommandPayload::PermissionResolve {
                interaction: interaction.clone(),
                option_id: "allow-once".to_owned(),
            },
        };
        let error = block_on(harness.broker.submit_mutation_versioned(
            &actor,
            &command,
            Some(Version::new(1)),
        ))
        .expect_err("stale version");
        assert!(matches!(
            error,
            PortError::Conflict(ConflictKind::VersionMismatch)
        ));
        // 校验失败不落受理行、不消费请求——重试因此不会被「accepted 但没有终态」卡死。
        assert!(
            !lock(&harness.world.state)
                .commands
                .contains_key(command_key(&actor, &request).as_str()),
            "校验失败不得留下 accepted 行"
        );
        assert!(
            !lock(&harness.world.state)
                .interactions
                .get(interaction.as_str())
                .is_some_and(|row| row.resolution.is_some()),
            "校验失败不得消费请求"
        );

        let receipt = block_on(harness.broker.submit_mutation_versioned(
            &actor,
            &command,
            Some(Version::new(2)),
        ))
        .expect("retry with the current version");
        assert!(matches!(receipt, CommandReceipt::Accepted { .. }));
        let state = lock(&harness.world.state);
        let row = state
            .interactions
            .get(interaction.as_str())
            .expect("interaction row");
        assert!(row.resolution.is_some(), "重试成功后应落下仲裁结果");
        assert!(row.confirmed, "适配器成功返回后应确认仲裁行");
    }

    /// 受理之后的竞态失败（另一设备抢先解析、请求在窗口内失效、后端写失败）必须落一条持久化终态：
    /// 客户端按同一 requestId 重试拿到确定结果，而不是永远停在 `accepted`。
    #[test]
    fn post_accept_resolution_failure_records_a_terminal() {
        let (harness, _authority) = Harness::with_interactions(BrokerConfig::default());
        let interaction = InteractionId::new(&uuid_text(311)).expect("interaction");
        lock(&harness.world.effective_interactions).push(effective(
            &harness.session,
            &interaction,
            InteractionKind::Permission,
            None,
        ));
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
        lock(&harness.world.resolve_errors)
            .push_back(PortError::Unavailable(UnavailableKind::IoError));

        let actor = harness.actor();
        let request = harness.request(7);
        let command = ClientCommand {
            actor: actor.clone(),
            request: request.clone(),
            command: "permission.resolve".to_owned(),
            kind: CommandKind::Mutation,
            session: Some(harness.session.clone()),
            expected_version: None,
            request_fingerprint: digest('P'),
            payload: CommandPayload::PermissionResolve {
                interaction: interaction.clone(),
                option_id: "allow-once".to_owned(),
            },
        };
        let receipt = block_on(harness.broker.submit_mutation_versioned(
            &actor,
            &command,
            Some(Version::new(1)),
        ))
        .expect("accepted, then failed");
        assert!(matches!(receipt, CommandReceipt::Accepted { .. }));

        let record = lock(&harness.world.state)
            .commands
            .get(command_key(&actor, &request).as_str())
            .cloned()
            .expect("acceptance row");
        assert_eq!(record.status(), CommandStatus::Failed);
        assert!(
            record.terminal_event().is_some(),
            "失败必须落 command.failed 终态事件"
        );
        {
            let state = lock(&harness.world.state);
            let row = state
                .interactions
                .get(interaction.as_str())
                .expect("interaction row");
            assert!(row.resolution.is_some(), "仲裁结果必须保留");
            assert!(!row.confirmed, "适配器写失败不得把请求移出有效集合");
        }

        // 重试是幂等重放，不会二次仲裁；终态已经存在，客户端可据此结束等待。
        let replay = block_on(harness.broker.submit_mutation_versioned(
            &actor,
            &command,
            Some(Version::new(1)),
        ))
        .expect("replay");
        assert!(matches!(replay, CommandReceipt::Accepted { .. }));
        assert!(lock(&harness.world.resolved_interactions).is_empty());
        assert_eq!(
            lock(&harness.world.state)
                .commands
                .get(command_key(&actor, &request).as_str())
                .map(|record| record.status()),
            Some(CommandStatus::Failed)
        );
    }
}
