//! 出站端口与端口层数据结构。
//!
//! 形状冻结于 `docs/CORE_PORTS_AND_STORAGE.md` §5：实现侧可以增加私有辅助方法，但不得改变这里的
//! 语义与参数；新增公共方法必须先改合同。端口一律是可 dyn 化的异步 trait（`#[async_trait]` 只是
//! proc-macro，不引入 runtime，§2）＋ `Send + Sync`，由组合根以 `Arc<dyn Port>` 持有。
//!
//! 本模块只使用 [`crate::model`] 的值对象，不出现 `sqlx`、`serde_json::Value`、HTTP、JSON-RPC、
//! wire discriminator 或数据库列名（§2）。
//!
//! §5 里被签名引用、但 §3 未列表的类型分两类：
//!
//! - §3 已冻结的值对象（含 `PublicError`、`CommandResult`、`PromptContentBlock` 等 §3 表格引用的
//!   形状）在 [`crate::model`]。
//! - 只服务端口的查询/结果结构（`OwnedCommit`、`SessionQuery`、`HistoryPage`、`ReplayBatch`……）
//!   在本模块，与它们的 trait 放在一起。

use std::sync::Arc;

use async_trait::async_trait;

use crate::model::{
    Actor, AgentDescriptor, AgentId, AgentProfile, AgentRef, AgentSessionId, AttachmentGeneration,
    AttachmentId, AuditAction, AuditOutcome, AuditRecord, CapabilitySet, CommandKind,
    CommandRecord, CommandStatus, CommandTerminalRecord, CommittedDelivery, CommittedEvent,
    ConfigOption, ConfigOptionId, ConfigValue, CreateSessionRequest, DeviceId, DeviceRecord,
    Digest, DirectoryId, DirectoryRecord, EndpointEvent, EntityRef, EventId, EventOrigin,
    EventPayload, EventType, GlobalCursor, InteractionId, InteractionResolution, LocalCursor,
    MessageId, ModeId, ModeRef, ModeState, NodeId, NodeKind, NodeRecord, OriginCursor, OriginEpoch,
    OriginEventRef, PairingClaim, PairingId, PairingPeer, PairingRecord, PairingSettlement,
    PeerIdentity, PeerPublicKey, PendingEvent, PendingInteraction, PortError, PromptRequest,
    PublicError, RemoteSessionRef, RequestId, ResumeSessionRequest, Sequence, ServerEpoch,
    SessionId, SessionRecoveryRecord, SessionReference, SessionSnapshot, SessionState,
    SessionSummary, Timestamp, Turn, TurnId, TurnState, Version,
};

/// `replay`/`local_replay` 的单批上限（`SYNC_PROTOCOL.md` §14：`maxReplayEventsPerBatch` 默认 500）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReplayLimit(u32);

impl ReplayLimit {
    /// v1 默认单批事件数（`SYNC_PROTOCOL.md` §14）。
    pub const DEFAULT_EVENTS: u32 = 500;

    pub const fn new(max_events: u32) -> Self {
        Self(max_events)
    }

    pub const fn events(self) -> u32 {
        self.0
    }
}

impl Default for ReplayLimit {
    fn default() -> Self {
        Self(Self::DEFAULT_EVENTS)
    }
}

// ---------------------------------------------------------------------------------------------
// §5.1 会话后端
// ---------------------------------------------------------------------------------------------

/// 后端接受 `prompt` 后的同步结果。
///
/// `turn` 是**适配器侧**对本次提交的标识／占位值，仅用于适配器自身的审计与日志：turn 归属一律由 core
/// 在提交前用自己的 `TurnId`（`IdGenerator::turn_id`）定稿（`CORE_PORTS_AND_STORAGE.md` §6 第 19 条、
/// §9 判据 31），本字段**不得**参与归属决策、不得产生第二个 turn 行，也不得影响事件顺序。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnAccepted {
    pub turn: TurnId,
}

#[async_trait]
pub trait AgentCatalog: Send + Sync {
    async fn agents(&self) -> Result<Vec<AgentDescriptor>, PortError>;
    async fn profile(&self, id: &AgentId) -> Result<Option<AgentProfile>, PortError>;
    async fn agent_capabilities(&self, agent: &AgentRef) -> Result<CapabilitySet, PortError>;
}

#[async_trait]
pub trait SessionBackendFactory: Send + Sync {
    /// `session` 是 core 已在存储里分配好的 `SessionId`（§3.1：由 `commit` 的事务内分配），因此后端
    /// 可以直接构造 `SessionEndpoint::reference()`，不必自己造引用或维护旁表。
    async fn create(
        &self,
        session: &SessionId,
        request: CreateSessionRequest,
        sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError>;

    async fn open(
        &self,
        reference: SessionReference,
        sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError>;

    /// 进程不在的会话恢复（`session.resume`）：按 [`ResumeSessionRequest::agent`] 重新拉起 Agent 子
    /// 进程并发送 `session/resume { sessionId, cwd }`，成功后返回可接受 turn 的端点。
    ///
    /// 与 `open` 的分工：`open` 只重新绑定**还活着**的进程内会话，本方法是唯一会为恢复而 spawn
    /// 的入口。能力门控（目标 Agent 未宣告时**不得发送** `session/resume`、并在返回前回收本次拉起
    /// 的子进程）与「同一会话只有一条活跃绑定」都由实现负责；能力不支持与「后端不支持该操作」
    /// 一律返回 [`PortError::Unavailable`] 的 `BackendUnsupported`。
    async fn resume(
        &self,
        session: &SessionId,
        request: ResumeSessionRequest,
        sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError>;

    /// 结束该会话所属的 Agent 运行时（进程树与映射一并回收），此后该运行时**不得再被复用**。
    ///
    /// 唯一调用方是「窗口期事件队列超限」的失败关闭（`SYNC_PROTOCOL.md` §11.5 的
    /// `session.create`：超限复用 `local-agent-host` 的「超限结束的失败关闭顺序」）：那时端点已经
    /// 不可信任，光关端点不足以让调用方看到「这个 Agent 已经不在运行」——事件源还在、进程树还在。
    /// 顺序与 [`Self::resume`] 的「本次拉起才回收」相反，这里**就是**要结束它：先把运行时从运行目录
    /// 里摘掉（此后按 `AgentId` 复用取不到它），再让未完成请求以明确结果结束，最后回收进程树。
    ///
    /// 幂等：该会话没有任何运行时（已结束、从未建立、或本实现不持有进程）时是成功的空操作。
    async fn terminate_agent(&self, session: &SessionId) -> Result<(), PortError>;
}

#[async_trait]
pub trait SessionEndpoint: Send + Sync {
    fn reference(&self) -> SessionReference;

    /// 本次绑定对应的 Agent 侧会话标识（ACP 会话标识）：创建成功后由 core 落盘，恢复得到的端点同样
    /// 可读（§3.6）。未取得标识（启动失败、超时、Agent 返回错误）时返回 `None`——实现**不得**编造
    /// 占位值，返回的取值必须来自 Agent 的实际响应。
    fn agent_session_id(&self) -> Option<&AgentSessionId>;

    async fn prompt(
        &self,
        request: PromptRequest,
        at: Timestamp,
    ) -> Result<TurnAccepted, PortError>;

    async fn cancel(&self, turn: Option<TurnId>) -> Result<(), PortError>;

    async fn set_mode(&self, mode: &ModeId) -> Result<(), PortError>;

    async fn list_config(&self) -> Result<Vec<ConfigOption>, PortError>;

    /// `SessionModeState.availableModes` 的唯一来源（§6 第 17 条）；core 不用它做授权或状态迁移，
    /// 端口返回空列表时结果就是空列表（不得凭当前模式编造候选）。
    async fn modes(&self) -> Result<ModeState, PortError>;

    async fn set_config(&self, id: &ConfigOptionId, value: ConfigValue) -> Result<(), PortError>;

    /// 权限与 elicitation 共用的唯一解析入口。
    async fn resolve_interaction(
        &self,
        interaction: &InteractionId,
        resolution: InteractionResolution,
    ) -> Result<(), PortError>;

    async fn read_history(&self, query: HistoryQuery) -> Result<HistoryPage, PortError>;

    async fn close(&self) -> Result<(), PortError>;
}

// ---------------------------------------------------------------------------------------------
// §4 用例面：`session.create` 的受理结果
// ---------------------------------------------------------------------------------------------

/// `session.create` 的**准备结果**：后端已经建立会话，但**会话行尚未落账**（§6 第 20 条）。
///
/// 它的存在是为了解开「终态结果读会话行、而会话行与终态必须同一次提交」这个环：会话行要落盘的取值
/// 在**调后端之前**就已定稿（标识、cwd、别名），`agent_session_id` 在后端成功返回后补齐，因此适配层
/// 不必回读存储就能投影自己的结果（Node Link 的 `SessionCreateResult`、Sync 的会话摘要）；投影完成后
/// 由适配层调 `settle_session_create` **一次**落账（会话行 + 幂等行回填 + 终态）。
///
/// 拿到本值的调用方**必须**把本次命令结算掉（`Completed` 或 `Uncertain`）：窗口期到达的后端事件在
/// [`WINDOW_EVENT_QUEUE_LIMIT`](crate::broker::WINDOW_EVENT_QUEUE_LIMIT) 的界内排队，落账后按序写库
/// 再广播；不结算就意味着这条命令停在 `accepted`，只能由启动恢复终结为 `uncertain`。
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedSessionCreate {
    /// 本次创建的会话标识（core 分配；落账提交采用它）。
    pub session: SessionId,
    /// 落账后的会话摘要——与 `SessionStore::list` **同一投影口径**（`state`/`version`/目录归属的取值
    /// 都是落账提交将写入的那些），因此适配层的终态结果与之后的读路径必然一致。
    pub summary: SessionSummary,
    /// ACP 会话标识；`None` = 后端未取得标识（不写占位值，§3.6）。
    pub agent_session_id: Option<AgentSessionId>,
}

/// `create_session` 的受理结果（§6 第 20 条、`sync-session-create`「重复 requestId 不产生第二个会话」）。
#[derive(Debug, Clone, PartialEq)]
pub enum SessionCreateAdmission {
    /// 首次受理、且后端已成功建立：调用方据此投影终态结果，再调 `settle_session_create` 落账。
    Prepared(Box<PreparedSessionCreate>),
    /// 同 `(actor, requestId)` 的受理记录已存在、尚未落账（窗口期与并发重发）。
    ///
    /// 调用方**不得**再派发第二次创建、**不得**改写首次记录：这正是「重复 requestId 不产生第二个会话」
    /// 的 core 侧语义——受理行的 `session_id` 此刻仍为 `NULL`，因此重放不能靠行里的会话标识判断，而是
    /// 靠本节这个结论。它对应的持久事实是「行仍是 `accepted`」；进程崩溃时该行由启动恢复终结为
    /// `uncertain`（§6 第 16 条）。
    InFlight,
    /// 同键的命令已经终结：返回**首次结果**（幂等重放），不得覆盖。
    Settled(Box<CommandRecord>),
}

// ---------------------------------------------------------------------------------------------
// §5.2 持久化
// ---------------------------------------------------------------------------------------------

/// 一次提交里对会话行的修改。`OwnedCommit::state` 为 `None` 时只追加事件与 turn 变更。
///
/// `Update` 的载荷（`SessionUpdate`）比 `Create` 大得多，但两者的形状是已冻结的合同、调用点全部按值
/// 构造：装箱会改动全部调用点与匹配点的写法，收益只是省下每次提交一次的枚举尺寸，因此这里显式保留
/// 按值载荷。
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum StateChange {
    /// 新建 owned 会话：`OwnedCommit.session` 必须为 `None`、`expected_version` 必须为 `None`、
    /// `origin_epoch` 必须为 `Some`；`SessionId` 由存储层在创建事务内分配并回传。
    ///
    /// 只有**不经 core 用例**的提交方使用它。`session.create` 的落账走
    /// [`StateChange::SettleCreate`]：会话标识必须由 core 在调用后端**之前**分配（§3.1、§6 第 20 条），
    /// 因此存储层不得再为那条路径现分配。
    Create(NewSession),
    /// `session.create` 的**落账提交**（§6 第 20 条）：会话行（含 core 分配的标识、ACP 会话标识、
    /// 创建时的规范化 cwd 与目录别名）、幂等行 `session_id` 的**回填**与终态在**同一次事务**里完成。
    ///
    /// 提交形状（存储层逐条校验，任一不满足即整事务 `InvalidRequest`）：`OwnedCommit.session` 必须等于
    /// [`SettledSession::session`]；`expected_version` 必须为 `None`；`origin_epoch` 必须为 `Some`；
    /// `idempotency` 必须为 `None`（幂等行已在受理提交里写过，本次只回填它的 `session_id`）；
    /// `command_terminal` 必须为 `Some`，且 `events` 里带 `causation = requestId` 的那条
    /// `command.completed`/`command.uncertain` 就是它的终态事件。
    ///
    /// 后端失败时**不得**用本形状：那时没有会话行可建，终态走 [`OwnedCommit::command_terminal`] 的
    /// 「尚无会话」形状（`session` 为 `None`、无 `state`）。
    SettleCreate(SettledSession),
    /// 已有会话的状态修改：`version` 在同一事务内 +1。
    Update(SessionUpdate),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSession {
    pub title: Option<String>,
    pub agent: AgentRef,
}

/// `session.create` 落账提交的会话行取值（§6 第 20 条、`local-agent-host`「会话创建时暴露 ACP
/// 会话标识」）。四个可选/必填分量都来自 core：标识由 [`crate::ports::IdGenerator::session_id`] 分配
/// （**不是**存储层现分配），三个列取自 core 自己的解析结果与后端回报（存储层只落列）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettledSession {
    /// core 已分配的会话标识；存储层**采用**它建行（`CommitOutcome.session_id` 原样回传它）。
    pub session: SessionId,
    pub title: Option<String>,
    pub agent: AgentRef,
    /// `owned_session.agent_session_id`：ACP 会话标识（`SessionEndpoint::agent_session_id()`）。
    /// `None` = 未取得标识（启动失败、超时、Agent 返回错误），此时不写占位值（§3.6）。
    pub agent_session_id: Option<AgentSessionId>,
    /// `owned_session.workspace_cwd`：core **自己**已解析的规范化绝对路径（不依赖后端回报）。
    pub workspace_cwd: Option<String>,
    /// `owned_session.workspace_directory_id`：同一次解析用掉的**别名原文**（目录归属的唯一投影来源）；
    /// 目录来源是客户端提供的路径时没有别名，写 `None` 即「未分组」。
    pub workspace_directory_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionUpdate {
    /// `None` = 不改会话状态（例如只解析一个交互）。
    pub state: Option<SessionState>,
    /// `owned_session.title`：**两层可选**。
    ///
    /// - `None` = 不改该列（ACP `SessionInfoUpdate` 是所有字段可选的部分更新：通知只携带
    ///   `updatedAt` 时标题必须保持不变）；
    /// - `Some(None)` = 把标题显式置空（通知把 `<title>` 显式置空 → 客户端呈现为未命名会话）；
    /// - `Some(Some(text))` = 写入该标题（上限 512 字符，与 `Session::title` 同一约束）。
    ///
    /// 唯一来源是 Agent 的 `session_info_update` 通知经 broker 投影的 `session.info.changed`
    /// （`design.md` D7）：不存在客户端发起的重命名命令，也没有绕过该通知直接写标题的路径。
    pub title: Option<Option<String>>,
    pub mode: ModeChange,
    /// `Some` 时会话进入 `Closed` 并写 `closed_at`。
    pub closed_at: Option<Timestamp>,
    /// 一次交互解析：由存储层用**条件更新**实现 first-writer-wins（§5.2、§6.7）。
    ///
    /// 受影响行数为 0 时存储层在同一事务内回读：行存在且 `state <> 'pending'` →
    /// `PortError::Conflict(AlreadyResolved)`；无行 → `PortError::NotFound`。
    pub interaction: Option<InteractionResolved>,
    /// `owned_session.agent_session_id`：`None` = 不改该列。
    ///
    /// `session.create` 不再经本列写入（它在 [`StateChange::SettleCreate`] 那条落账提交里一次建全），
    /// 此后只读（恢复流程只读这两列）；保留本列是为了不经 core 的提交方仍能按列语义写它。
    pub agent_session_id: Option<AgentSessionId>,
    /// `owned_session.workspace_cwd`：`None` = 不改该列。语义同
    /// [`SessionUpdate::agent_session_id`]：`session.create` 走落账提交，本列只服务其它提交方。
    pub workspace_cwd: Option<String>,
    /// `owned_session.workspace_directory_id`：`None` = 不改该列。语义同
    /// [`SessionUpdate::agent_session_id`]：`session.create` 走落账提交，本列只服务其它提交方。
    ///
    /// 来源是客户端提供的目录时该次解析**没有别名**，本列因此保持 `NULL`（「未分组」）。
    /// 写入后**只有读取路径**消费它（`SessionStore::list` 投影出 `SessionSummary::workspace`）；
    /// 恢复流程既不读也不改它（`SessionRecoveryRecord` 里没有这一项，§3.6）。`None` 语义为「未分组」，
    /// MUST NOT 由 `workspace_cwd` 反查别名补齐（`design.md` D1）。
    pub workspace_directory_id: Option<String>,
}

/// 交互的一次解析（`InteractionResolution` + 解析者，供 `owned_interaction` 的
/// `decision_*`/`elicitation_*`/`resolved_by_*` 列）。
#[derive(Debug, Clone, PartialEq)]
pub struct InteractionResolved {
    pub interaction: InteractionId,
    pub resolution: InteractionResolution,
    pub resolved_by: Actor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModeChange {
    /// 不改当前模式。
    Unchanged,
    Set(ModeRef),
}

/// 一次提交里对 turn 行的修改。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnChange {
    Create(NewTurn),
    Update(TurnUpdate),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTurn {
    /// 由 core 用 `IdGenerator::turn_id` 在提交前分配。
    pub turn: TurnId,
    pub state: TurnState,
    pub causation: Option<RequestId>,
    pub started_at: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnUpdate {
    pub turn: TurnId,
    pub state: TurnState,
    pub ended_at: Option<Timestamp>,
}

/// 幂等记录（`owned_command` 行）。幂等键是协议维度的 `(actor, request_id)`，本结构承载键的全部
/// 分量与 §6 第 6 条要比对的五项指纹输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyRecord {
    pub actor: Actor,
    pub request: RequestId,
    pub command: String,
    pub kind: CommandKind,
    pub session: Option<SessionId>,
    pub expected_version: Option<Version>,
    pub request_fingerprint: Digest,
    pub accepted_at: Timestamp,
}

/// 幂等命中：返回首次结果，不追加事件、不改状态（§5.2 约束）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotentReplay {
    pub record: CommandRecord,
}

/// 新建一条 pending 交互行（权限 / elicitation 请求）。
///
/// 与**同一提交**里那条 `kind = interaction` 的事件成对（§6 第 13 条、§9 判据 15）；`options` 不落库
/// （`owned_interaction` 无该列），读视图里 [`PendingInteraction::options`] 恒为空，调用方用
/// [`ReadView::event_payload`] 还原。
///
/// 配对由**存储层**完成：在同一提交里找 `kind = interaction` 且 payload 的 `interactionId` 等于
/// `interaction.id()` 的那条事件，把它的真实 `event_id` 写进 `owned_interaction.request_event`
/// （`event_id` 由存储层分配，broker 拿不到，因此这里不带该字段）；配不到（事件缺失或该事件被
/// `Ephemeral` 过滤掉）→ 整事务 `InvalidRequest`，不得写悬空引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingInteractionWrite {
    pub interaction: PendingInteraction,
    pub turn: Option<TurnId>,
}

/// owned 家族的唯一写入口载荷（§5.2）。
#[derive(Debug, Clone, PartialEq)]
pub struct OwnedCommit {
    /// `None` = 非会话级事件（如 `device.revoked`）：`session_sequence` 与 origin cursor 均为 NULL，
    /// 只分配 `global_sequence`。`session.create` 的**失败终态**提交也是这个形状——那时没有会话行，
    /// 终态事件只拿全局序号（§6 第 20 条）。
    ///
    /// [`StateChange::SettleCreate`] 时必须为 `Some`，且等于载荷里的 `session`（本提交的事件按该会话
    /// 归属，因此与「会话行同批落盘」不矛盾：行在同一事务里先建）。
    pub session: Option<SessionId>,
    pub at: Timestamp,
    pub expected_version: Option<Version>,
    pub state: Option<StateChange>,
    pub turns: Vec<TurnChange>,
    pub events: Vec<PendingEvent>,
    /// 新建 pending 交互行（权限 / elicitation 请求）。每条都必须与同一提交里那条 `interaction` 事件
    /// 一一对应；解析既有行走 `state.interaction`，两条路径互斥（§6 第 13 条）。
    pub interactions: Vec<PendingInteractionWrite>,
    /// delta 压缩的替代清单（§6 第 15 条）：本提交里那条 `kind='summary'` 事件所替代的 delta 行的
    /// `global_sequence`。每个 cursor 必须属于本提交的会话、对应行必须是 `kind='delta'` 且
    /// `compacted_into IS NULL`；任一不满足 → 整事务 `InvalidRequest`（存储层校验，失败关闭）。
    pub compacted: Vec<GlobalCursor>,
    /// 含指纹与 `expected_version`。
    pub idempotency: Option<IdempotencyRecord>,
    /// 命令终态。**接受提交**（`idempotency` 为 `Some`）时本字段为 `None`——`CommandTerminalRecord`
    /// 只能表达终止态（§3.3），accepted 行的 `status`/`accepted_at` 由 `idempotency` 决定。
    ///
    /// **终态提交**（本字段为 `Some`）不含 `requestId`，因此存储层用同一批次里 `command.*`
    /// 终态事件的 `causation` 定位 `owned_command` 行；`terminal_event` 由存储层回填为该事件的
    /// `event_id`，并把 `terminal_at`/`result`/`error` 写进该行（`None` = 不改既有列）。
    ///
    /// 定位规则按提交形状分三条（§6 第 20 条）：`session` 为 `Some` 且 `state` 是
    /// [`StateChange::SettleCreate`] 时，行仍是受理提交写的**尚无会话**那一行（`session_id IS NULL`），
    /// 存储层先按 `request_id + session_id IS NULL + status = 'accepted'` 定位并**回填**本次的会话标识，
    /// 再写终态；`session` 为 `Some` 的其它形状按 `(session_id, request_id)` 定位；`session` 为 `None`
    /// 时按 `request_id + session_id IS NULL + status = 'accepted'` 定位（`session.create` 的失败终态与
    /// 启动恢复都走这一条）。命中多行即整事务 `InvalidRequest`（不得猜），命中 0 行即 `NotFound`。
    pub command_terminal: Option<CommandTerminalRecord>,
    /// 新建会话时由 core 生成并传入；存储层只校验“已有 epoch 时必须一致”。
    pub origin_epoch: Option<OriginEpoch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitOutcome {
    /// 新建会话时返回分配到的 id。
    pub session_id: Option<SessionId>,
    pub origin_epoch: Option<OriginEpoch>,
    pub version: Version,
    pub appended: Vec<CommittedEvent>,
    pub replayed: Option<IdempotentReplay>,
}

/// `session.list` 的过滤条件。授权过滤由用例层完成，存储层只按这里的条件取行。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionQuery {
    /// 只返回这些会话；`None` = 不按 id 过滤。
    pub only: Option<Vec<SessionId>>,
    /// 只返回这些状态；空 = 不按状态过滤。
    pub states: Vec<SessionState>,
    /// 返回行数上限；`None` = 不限。
    pub limit: Option<u32>,
}

/// `session.read` 的 `include` 集合（`SYNC_PROTOCOL.md` §11.5）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HistoryInclude {
    pub messages: bool,
    pub turns: bool,
    pub pending_interactions: bool,
    pub config_options: bool,
    pub capabilities: bool,
    /// 结构化历史条目（`SYNC_PROTOCOL.md` §11.5 的 `history` 取值）：开启时 [`HistoryPage::history`]
    /// 给出该会话的一页条目，关闭时它为空。空**只**由本开关决定，不代表「该会话没有历史」。
    pub history: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryQuery {
    pub session: SessionId,
    pub include: HistoryInclude,
    /// 分页起点：只返回该 cursor 之后的事件；`None` = 从头。cursor 的合法性由用例层判定。
    pub after: Option<GlobalCursor>,
    /// 结构化历史条目的分页游标：**会话内**事件序号（`SYNC_PROTOCOL.md` §11.5 的 `beforeHistory`），
    /// 只返回会话内序号**严格小于**它的条目；`None` = 该会话的最新一页。
    ///
    /// 它只作用于 [`HistoryPage::history`]，与作用于 `events` 的 `after`（全局 cursor）各自独立分页，
    /// 一次请求里互不改变对方的返回内容与页边界。取值只能来自上一页最早条目的
    /// `CommittedEvent.session_sequence`：用事件 id、全局序号或其它标识代替**不是**本字段的语义，实现
    /// 不得据此猜测或补全游标。
    ///
    /// 游标早于该会话仍可读条目的下界（更早的行已被保留策略压缩或清理）时，实现 MUST 返回
    /// [`PortError::InvalidRequest`]，MUST NOT 静默返回更少的条目让调用方误以为已到最早一条；游标正好
    /// 等于该下界时是合法的一页边界（空页 + 无更早条目，不是错误）。
    pub before_history: Option<Sequence>,
    pub limit: ReplayLimit,
}

/// owned 事件 + 其持久化正文（Node Link 的 origin 重放、会话历史读与事件扇出共用；定位与正文严格
/// 同源——都来自**同一条行**/同一次读视图，因此调用方不需要逐条再取一次正文）。
#[derive(Debug, Clone, PartialEq)]
pub struct OwnedEventRecord {
    pub event: CommittedEvent,
    pub payload: EventPayload,
}

/// 未决交互 + 创建它的 origin 事件 id（Node Link 快照的 `pending_interactions` item 需要那个事件的
/// payload 才能算出 wire 的 `payloadDigest`；交互行本身不落库正文）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingInteractionOrigin {
    pub interaction: PendingInteraction,
    pub origin_event: EventId,
}

/// 一个会话的 Node Link 读视图（[`ReadView::node_link_slice`] 的返回值）：快照需要的元数据与
/// `after` 之后的事件切片，在**同一次只读事务**内取得（快照 barrier 与正文同源）。
#[derive(Debug, Clone, PartialEq)]
pub struct NodeLinkSlice {
    /// 会话摘要：快照的 `sessionMeta` 只取它的 `state`/`version`。
    pub summary: SessionSummary,
    /// 该会话当前的 origin cursor（`origin_epoch` 与该会话已提交的最大 `origin_sequence`）。
    /// 会话尚无事件时序列为 0（origin cursor 的空值，不是「不存在」）。
    pub head: OriginCursor,
    /// 未决交互（快照的 `pending_interactions`）。
    pub pending_interactions: Vec<PendingInteractionOrigin>,
    /// `after` 之后的事件，按 `origin_sequence` 升序。
    pub events: Vec<OwnedEventRecord>,
}

/// Sync 事件读面的条目（`SYNC_PROTOCOL.md` §10.1 的 `event` 帧）：定位 + 持久化正文 + 三个**只有存储
/// 行能回答**的来源字段。
///
/// 为什么不复用 [`OwnedEventRecord`]：`origin_kind` 与 `causation` 是 `owned_event` 的两个列（§7.3），
/// 而 `CommittedEvent` 是 §3.4 冻结的**只带定位**的值对象、`EventPayload` 是正文，两者都不承载它们；
/// 于是 `schemas/sync/v1/event.schema.json` 列为 required 的 `origin`/`causationRequestId` 在既有读面上
/// **无法组装**。本条目是它们的载体，`OwnedEventRecord`/`CommittedEvent` 的形状因此不变（它们有大量
/// 现存调用方）。
///
/// `remoteOrigin`（同属 §10.1 的 required 清单）**没有**载体：本读面只读 `owned_event`，而 imported
/// 事件的正文与 `remoteOrigin` 属另一套读路径（imported 家族无正文，§7.4），因此调用方按「本读面返回的
/// 都是 owned 事件 ⇒ `remoteOrigin` 恒为 `null`」投影（§9.6）。
#[derive(Debug, Clone, PartialEq)]
pub struct SyncEventRecord {
    /// 定位；与 [`OwnedEventRecord::event`] 同源（同一条行、同一次取行）。
    pub event: CommittedEvent,
    /// 持久化正文；与 [`OwnedEventRecord::payload`] 同源（同一条行，不逐条回查）。
    pub payload: EventPayload,
    /// `SYNC_PROTOCOL.md` §10.1 的 `origin.kind`：取 `owned_event.origin_kind`。
    ///
    /// 取值**只能**来自该列——按「这条事件有没有 `session_id`」推断来源不是本字段的语义，实现不得据此
    /// 猜测或补全（§5.2 关于 `PendingEvent.origin` 的条款同口径）。**五个**取值
    /// （`agent`/`device`/`daemon`/`local_cli`/`node`）原样往返（§7.3 的 CHECK 与 wire 词表同集）；
    /// 词表外取值是列值损坏：实现**必须**返回 [`PortError::Corrupt`]，不得静默降级或挑一个近义取值。
    pub origin_kind: EventOrigin,
    /// `SYNC_PROTOCOL.md` §10.1 的 `origin.deviceId`：**仅** `origin_kind = device` 时非空，其余
    /// 恒为 `None`。
    ///
    /// 取值来自 `owned_event.causation`（= 诱发该事件的命令 `requestId`）到 `owned_command.actor_id` 的
    /// 回查（要求 `actor_kind = 'device'`）。§10.1 对 `kind = device` 的语义要求是「事件来自已授权
    /// Sync 设备经命令产生 ⇒ 带非空 `deviceId`」，而 schema 侧的 `{kind:"device", deviceId:null}` 又
    /// 合法（见 `SYNC_PROTOCOL.md` §10.1 的配对规则与 §16.2）——因此**发送方** MUST NOT 产生那种
    /// 帧。本读面据此**失败关闭**：`origin_kind = device` 而设备 id 得不出唯一取值时，实现 MUST 返回
    /// [`PortError::Corrupt`]，MUST NOT 以 `None` 交差了事（那等于把「投递层无法组装合法帧」改写
    /// 成一条合法取值，逼它静默发出 §10.1 禁止的形状），也 MUST NOT 从旁路（会话归属、actor 瞬时
    /// 状态……）推断一个 id。
    ///
    /// 得不出唯一取值的三类情形（真实库里的产生路径见 §5.2 的 `[决定]`）：`causation` 为空、
    /// 回查不到对应命令行（该 requestId 不在本机 `owned_command` 里）、命令行的 `actor_kind`
    /// 不是 `device`（`node`/`cli`）。第四类是库被外部改写：同一 `requestId` 命中**多台**设备
    /// （`owned_command` 的唯一键是 `(actor_kind, actor_id, request_id)`，跨设备复用同一 requestId
    /// 在 DDL 上合法）——那同样没有唯一答案，同样失败关闭。
    pub origin_device_id: Option<DeviceId>,
    /// `SYNC_PROTOCOL.md` §10.1 的 `origin.nodeId`（feature `core.event-origin.v1`）：**仅**
    /// `origin_kind = node` 时非空，其余恒为 `None`。
    ///
    /// 与 [`SyncEventRecord::origin_device_id`] 同源同口径，只是回查要求 `actor_kind = 'node'`，
    /// 取回的 `actor_id` 是该命令的 `"{ownerNodeId}/{accessNodeId}"` 复合键（§7.3），本字段取其中的
    /// **主 principal** `ownerNodeId`（§10.1 的 `nodeId` 就是产生该事件的另一个节点；access 端点
    /// 只经命令幂等键存在）。§10.1 的配对规则「`kind = node` ⇒ `nodeId` 非空且 `deviceId` 为
    /// `null`」由本条与上一条的分工保证：两者互斥、且都得不出唯一取值时失败关闭。
    pub origin_node_id: Option<NodeId>,
    /// `SYNC_PROTOCOL.md` §10.1 的 `causationRequestId`，取 `owned_event.causation` 原样：没有命令直接
    /// 导致的事件为 `None`（§10.1 的规则原文）。
    pub causation: Option<RequestId>,
}

/// Sync 事件读面的一个批次（[`ReadView::sync_replay`] 的返回值）：`after` 之后的一页事件 + 生成这批
/// 数据时的一致性视图 head（`sync.caught_up` 的屏障游标），同一次只读事务内取得。
#[derive(Debug, Clone, PartialEq)]
pub struct SyncReplaySlice {
    /// `after` 之后的事件，按 `global_sequence` 升序，最多 `limit` 条。
    pub events: Vec<SyncEventRecord>,
    /// 本批数据对应的一致性视图 head（barrier）。
    pub head: GlobalCursor,
}

/// 历史页面。`config`/`capabilities` 是**用例层**按 `include` 合并的活体数据（来自
/// `SessionEndpoint`/`AgentCatalog`）；存储层实现必须让它们保持空值——持久层不保存活体能力。
///
/// 两个事件集合各自分页、**各自带持久化正文**（都复用 [`OwnedEventRecord`]，不新增值对象）：
/// `events` 是 `messages` 面的原料（`after`/`next` 的全局 cursor 升序一页），`history` 是结构化历史
/// 条目（`before_history`/`history_has_earlier` 的会话内序号一页）。同一请求里两者互不影响。
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryPage {
    pub session: SessionSummary,
    /// `messages` 面的原料：`after` 之后的一页事件（含持久化正文），按 `global_sequence` 升序；
    /// `include.messages` 为假时为空。
    pub events: Vec<OwnedEventRecord>,
    /// 结构化历史条目，按 `session_sequence` **升序**；`include.history` 为假时为空。
    ///
    /// 每个元素的 `event.session_sequence` 必然为 `Some`（条目由会话内序号定位）且互不相同。
    pub history: Vec<OwnedEventRecord>,
    /// [`HistoryPage::history`] 之前是否仍有更早条目（`SYNC_PROTOCOL.md` §11.5 的 `historyHasEarlier`）；
    /// `include.history` 为假时恒为假。为假只表示已到该会话**仍可读**的最早一条。
    pub history_has_earlier: bool,
    pub turns: Vec<Turn>,
    pub interactions: Vec<PendingInteraction>,
    pub config: Vec<ConfigOption>,
    pub capabilities: Option<CapabilitySet>,
    /// 本页数据对应的一致性视图 head（barrier）。
    pub head: GlobalCursor,
    /// 续读游标；`None` = 已到 `head`。
    pub next: Option<GlobalCursor>,
}

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn commit(&self, commit: OwnedCommit) -> Result<CommitOutcome, PortError>;

    async fn load(&self, session: &SessionId) -> Result<Option<SessionSnapshot>, PortError>;

    /// 会话列表。返回的每个 [`SessionSummary`] 的 `workspace` **必须**取自该会话行持久化的
    /// `owned_session.workspace_directory_id`（展示名按别名关联目录记录取得，取不到时回退为别名本身）；
    /// 别名为 `NULL` 时就是 `None`（未分组）。
    ///
    /// 实现 MUST NOT 按 `owned_session.workspace_cwd` 反查别名解析表来补出归属：`NULL` 就是未分组，
    /// 「不反查」是恢复与归属稳定（别名重指向不改变既有会话归属）的前提（§3.6、`design.md` D1/D6）。
    async fn list(&self, query: SessionQuery) -> Result<Vec<SessionSummary>, PortError>;

    async fn head(&self) -> Result<GlobalCursor, PortError>;

    /// 一致性读视图：`sync.snapshot_*` 必须在本方法返回的视图内完成（barrier 依据）。
    async fn read_view(&self) -> Result<Box<dyn ReadView>, PortError>;

    async fn find_request(
        &self,
        request: &RequestId,
        actor: &Actor,
    ) -> Result<Option<CommandRecord>, PortError>;

    /// 该会话的恢复数据（§3.6）：`agent` 取自会话行，两个 `Option` 直接对应 `owned_session` 的
    /// `agent_session_id`/`workspace_cwd`。
    ///
    /// **窄读取**：这两列不进 `Session`/`SessionSummary`（`workspace_cwd` 是本机规范化路径，不得进入
    /// 可投影形状，§3.6）。会话行不存在，或任一列为 `NULL`（该会话没有可用于恢复的数据）时返回
    /// `Ok(None)`——`NULL` 不是错误，也不得被推导或补齐。
    ///
    /// 本方法**不读 `workspace_directory_id`**：恢复一律以持久化的 cwd 原文为权威，不按别名重解析，目录归属
    /// 是投影期的事（§3.6、`design.md` D1）。
    async fn load_recovery(
        &self,
        session: &SessionId,
    ) -> Result<Option<SessionRecoveryRecord>, PortError>;

    /// 启动恢复用（§6 第 16 条）：仍为 `accepted` 且没有终态事件的 mutation 命令，最多 `limit` 条。
    async fn unsettled_commands(&self, limit: ReplayLimit)
    -> Result<Vec<CommandRecord>, PortError>;

    async fn retention_window(
        &self,
        session: &SessionId,
    ) -> Result<Option<(Sequence, Sequence)>, PortError>;

    async fn prune(&self, policy: RetentionPolicy, at: Timestamp)
    -> Result<PruneReport, PortError>;

    async fn health(&self) -> Result<StoreHealth, PortError>;
}

#[async_trait]
pub trait ReadView: Send + Sync {
    async fn head(&self) -> Result<GlobalCursor, PortError>;

    async fn replay(
        &self,
        after: Option<GlobalCursor>,
        limit: ReplayLimit,
    ) -> Result<ReplayBatch, PortError>;

    /// Sync 增量重放的事件来源（`SYNC_PROTOCOL.md` §10.1 的 `event` 帧）：`after` 之后的 owned 事件
    /// （定位 + 持久化正文 + [`SyncEventRecord`] 的三个来源字段），按 `global_sequence` 升序，并给出
    /// 生成这批数据时的 head（`sync.caught_up` 的屏障游标）。
    ///
    /// 与本 trait 的 [`ReadView::replay`] 的分工：`replay` 服务**本机**客户端的全局重放（owned 与
    /// imported 合并成 `CommittedDelivery`，并给出 `reset_required`/`next`）；本方法只服务入站设备的
    /// `event` 帧，回传 §10.1 要求的来源字段，且不做任何 cursor 解释。
    ///
    /// - 窗口：严格在 `after` 之后（`None` = 从头）、升序、最多 `limit` 条；已压缩的 delta 不出现
    ///   （与 [`ReadView::replay`] 同口径，§6 第 15 条的「被压行由 summary 事件替代」）。
    /// - `after.server_epoch` 与当前 epoch 是否一致**不由本方法判定**：四类 cursor 拒绝原因
    ///   （`SYNC_PROTOCOL.md` §9.2）与 `sync.reset_required` 是投递层的判定，本方法只按
    ///   `global_sequence > after` 取行。
    /// - `origin_device_id`/`origin_node_id` 的回查必须与事件读在**同一次只读事务**内完成，且不得
    ///   逐条回查（见 [`SyncEventRecord::origin_device_id`] 的取值规则）。
    /// - **历史行失败关闭**：`origin_kind = device`（`node`）而设备（节点）id 得不出唯一取值时，实现
    ///   MUST 返回 [`PortError::Corrupt`] 而不是给 `None`——升级前写入的行在 `causation`/actor 分类上
    ///   本来就可能不可归因，交付层因此拿不到可组装的合法帧；读面必须把这个状态显式暴露出来，而不是
    ///   伪装成合法取值（`SYNC_PROTOCOL.md` §10.1）。
    async fn sync_replay(
        &self,
        after: Option<GlobalCursor>,
        limit: ReplayLimit,
    ) -> Result<SyncReplaySlice, PortError>;

    /// 会话的一页读（`session.read`）：`messages` 面的事件原料与结构化历史条目各自按 `include`
    /// 开关**分页**（`HistoryQuery::after` 与 `HistoryQuery::before_history` 互不影响）。
    ///
    /// 两个集合都必须**带持久化正文**、且正文与定位来自同一条行：实现必须一次取行（`payload_json`
    /// 与 ACP 列同批取出），MUST NOT 为逐条事件再取一次正文（`design.md` D4）。
    ///
    /// 结构化历史条目按**会话内序号**分页：省略游标时给最新一页；带游标时给紧接该游标之前的一页；
    /// 分页只跳过已按保留策略压缩的行（`compacted_into IS NOT NULL`），游标早于仍可读条目的下界时返回
    /// [`PortError::InvalidRequest`] 而不是更少条目。
    async fn read_session(&self, query: HistoryQuery) -> Result<HistoryPage, PortError>;

    /// 取回一条 owned 事件的持久化正文——重放、`sync.snapshot_*` 与 `PendingInteraction.options`
    /// 补全的唯一入口（复用 [`EventPayload`]/`AcpRaw`，不新增类型）。
    ///
    /// `view` 必须是库里 `payload_json` 的**原样文本**（不得经通用 JSON 解析后重新序列化）；`acp`
    /// 可用时 `raw_json` 必须字节保真，不可用时按行里的 `acp_raw_unavailable_reason` 还原为
    /// `AcpRaw::Unavailable`。事件不存在返回 `None`。
    async fn event_payload(&self, event: &EventId) -> Result<Option<EventPayload>, PortError>;

    /// Node Link 的会话读视图（`docs/NODE_LINK_PROTOCOL.md` §12.4，`design.md` D6）：快照元数据
    /// （会话摘要、origin head、未决交互）与 `after` 之后的会话事件**含持久化正文**，在同一次只读
    /// 事务内取得。
    ///
    /// `after` 的 `origin_epoch` 与该会话当前 epoch 是否一致由**调用方**判定（本方法只按
    /// `origin_sequence > after` 取行，不替调用方解释 epoch 不符）；会话不存在返回 `NotFound(Session)`。
    async fn node_link_slice(
        &self,
        session: &SessionId,
        after: Option<OriginCursor>,
        limit: ReplayLimit,
    ) -> Result<NodeLinkSlice, PortError>;

    /// 指定会话内某条 owned 事件的正文；事件 id 与会话必须同时匹配，否则返回 `None`。
    ///
    /// 还原规则与 [`ReadView::event_payload`] 逐条相同，额外把读取面收窄到「该会话的事件」：
    /// Node Link 的事件扇出只按会话归属取正文，不提供按任意 event id 的通用读取。
    async fn session_event_payload(
        &self,
        session: &SessionId,
        event: &EventId,
    ) -> Result<Option<EventPayload>, PortError>;
}

/// imported 家族的唯一写入口载荷：**无正文**（§7.4）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryReceipt {
    pub session: RemoteSessionRef,
    pub origin: OriginEventRef,
    pub origin_sequence: Sequence,
    pub event_type: EventType,
    pub payload_digest: Digest,
    pub at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptOutcome {
    pub local_sequence: LocalCursor,
    /// 同一 `origin_event_id` 重发：既有行不变，也不新增 `local_sequence`（§9.5）。
    pub duplicate: bool,
}

/// 无正文交付索引条目（`NODE_LINK_PROTOCOL.md` §6 白名单）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryIndexEntry {
    pub session: RemoteSessionRef,
    pub origin: OriginEventRef,
    pub origin_sequence: Sequence,
    pub local_sequence: LocalCursor,
    pub event_type: EventType,
    pub payload_digest: Digest,
    pub received_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AckOutcome {
    /// `true` = 游标前进并已持久化；`false` = 回退请求被忽略，既有游标不变。
    pub applied: bool,
    /// 存储当前的已确认游标（可能是本次之前的旧值）。
    pub cursor: Option<OriginCursor>,
}

/// 当前连接上的 attachment 凭据；断开即清空，不跨重启保留。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionAttachment {
    pub id: AttachmentId,
    pub generation: AttachmentGeneration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedSessionRecord {
    pub session: RemoteSessionRef,
    pub origin_epoch: Option<OriginEpoch>,
    pub title: Option<String>,
    pub agent: Option<AgentRef>,
    pub state: Option<SessionState>,
    pub version: Option<Version>,
    pub created_at: Option<Timestamp>,
    pub last_origin_sequence: Option<Sequence>,
    pub acked: Option<OriginCursor>,
    pub attachment: Option<SessionAttachment>,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportedSessionQuery {
    /// 只返回这些远程会话；`None` = 不按会话过滤。
    pub only: Option<Vec<RemoteSessionRef>>,
    /// 返回行数上限；`None` = 不限。
    pub limit: Option<u32>,
}

/// imported 命令终态引用（`imported_command_ref` 行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteCommandRef {
    pub request: RequestId,
    pub command: String,
    pub status: CommandStatus,
    pub accepted_at: Option<Timestamp>,
    pub terminal_at: Option<Timestamp>,
    pub terminal_event: Option<EventId>,
    pub error: Option<PublicError>,
}

#[async_trait]
pub trait RemoteDeliveryStore: Send + Sync {
    async fn commit_receipt(&self, receipt: DeliveryReceipt) -> Result<ReceiptOutcome, PortError>;

    async fn local_replay(
        &self,
        after: Option<LocalCursor>,
        limit: ReplayLimit,
    ) -> Result<Vec<DeliveryIndexEntry>, PortError>;

    async fn ack(
        &self,
        session: &RemoteSessionRef,
        cursor: OriginCursor,
        at: Timestamp,
    ) -> Result<AckOutcome, PortError>;

    async fn load_ack(&self, session: &RemoteSessionRef)
    -> Result<Option<OriginCursor>, PortError>;

    async fn upsert_session(
        &self,
        record: ImportedSessionRecord,
        at: Timestamp,
    ) -> Result<(), PortError>;

    async fn list_sessions(
        &self,
        query: ImportedSessionQuery,
    ) -> Result<Vec<ImportedSessionRecord>, PortError>;

    /// imported 命令终态引用查询（与 `SessionStore::find_request` 同名会迫使调用点写 UFCS，故改名）。
    async fn find_remote_request(
        &self,
        session: &RemoteSessionRef,
        request: &RequestId,
    ) -> Result<Option<RemoteCommandRef>, PortError>;

    async fn prune(&self, policy: RetentionPolicy, at: Timestamp)
    -> Result<PruneReport, PortError>;
}

/// 需要重建快照的原因（`SYNC_PROTOCOL.md` §9.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetReason {
    InitialSync,
    EpochMismatch,
    CursorExpired,
    CacheIncompatible,
}

/// 一批重放结果。`reset_required` 是**服务端主动**要求重建快照的唯一依据（`SYNC_PROTOCOL.md` §9.2）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayBatch {
    pub events: Vec<CommittedDelivery>,
    /// 生成这批数据时视图的 head（barrier）：客户端据此发 `sync.caught_up`。
    pub head: GlobalCursor,
    /// 续读游标；`None` = 已到 `head`。
    pub next: Option<GlobalCursor>,
    pub reset_required: Option<ResetReason>,
}

/// 保留与容量的配置集合（`docs/CORE_PORTS_AND_STORAGE.md` §7.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionPolicy {
    pub transcript_retention_days: u32,
    pub sync_event_retention_days: u32,
    pub audit_retention_days: u32,
    pub max_total_size_bytes: u64,
    pub max_session_size_bytes: u64,
    pub persist_deltas: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PruneReport {
    pub removed_events: u64,
    pub removed_interactions: u64,
    pub removed_attachments: u64,
    pub removed_audit: u64,
    pub freed_bytes: u64,
    /// 清理后仍超出容量上限：写路径必须返回 `PortError::Unavailable(UnavailableKind::StorageFull)`。
    pub still_over_limit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreHealth {
    /// §7.5 的容量度量现值（owned 与 imported 两家族 TEXT 列 + 附件字节）；调用方据此解释 `still_over_limit`。
    pub total_bytes: u64,
    /// `PRAGMA quick_check` 的结果；`false` 即只读失败关闭。
    pub integrity_ok: bool,
    /// 进入只读失败关闭（写路径全部返回 `PortError::Corrupt`）。
    pub read_only: bool,
    pub user_version: u32,
    pub owned_schema_version: u32,
    pub imported_schema_version: u32,
    pub server_epoch: ServerEpoch,
}

// ---------------------------------------------------------------------------------------------
// §5.3 信任、审计与附件
// ---------------------------------------------------------------------------------------------

/// 撤销原因。v1 的 `device.revoke`/`node.revoke` 只传 id，因此调用方填 `UserRequested`；
/// `node.identity_changed` 填 `KeyChanged`；带外撤销（丢失/泄露）填 `Compromised`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevokeReason {
    UserRequested,
    KeyChanged,
    Compromised,
}

/// 原子认领的结果：认领后的记录（状态已推进到 claimed 一档）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingClaimOutcome {
    pub pairing: PairingRecord,
}

/// 配对落定后创建的信任记录引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustRecordRef {
    Device(DeviceId),
    Node(NodeId),
}

/// 待写入的审计行（§11.6）：与状态变更同事务；审计写失败则整事务失败（§11.2 第 6 条）。
///
/// 字段与 `AuditRecord` 完全一致，只是没有 `at`（`at` 由 [`WriteContext`] 提供）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingAudit {
    pub action: AuditAction,
    pub actor: Actor,
    pub via_node: Option<NodeId>,
    pub local_principal_ref: Option<String>,
    pub target: EntityRef,
    pub outcome: AuditOutcome,
    pub detail_digest: Option<Digest>,
}

/// 管理写集的公共上下文（§11.6）。`at` 由调用方从 `Clock` 取（§2：存储层不读系统时间）。
///
/// `audit` 为空只允许用于不作为 `AuditAction` 已登记安全动作的操作（例如 `workspace.select`、
/// `agent.configure`）；涉及安全动作的写集必须至少带一条成功或失败审计。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteContext {
    pub at: Timestamp,
    pub audit: Vec<PendingAudit>,
}

/// 写入一个设备记录（§11.6）：同 ID 不得换绑公钥，也不得把 `revoked` 改回 `active`。
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceWrite {
    pub record: DeviceRecord,
    pub context: WriteContext,
}

/// 写入一个节点角色行并绑定身份材料（§11.6）：同一 `nodeId` 的两种角色必须指纹一致。
#[derive(Debug, Clone, PartialEq)]
pub struct NodeWrite {
    pub record: NodeRecord,
    pub public_key: PeerPublicKey,
    pub context: WriteContext,
}

/// 撤销一个设备（§11.6）：单事务写撤销时间、状态与审计；提交后才由组合根关闭连接。
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceRevocation {
    pub device: DeviceId,
    pub reason: RevokeReason,
    pub context: WriteContext,
}

/// 按 NodeId 撤销一个节点（§11.6）：同一事务令两种角色一起进入 `revoked`。
#[derive(Debug, Clone, PartialEq)]
pub struct NodeRevocation {
    pub node: NodeId,
    pub reason: RevokeReason,
    pub context: WriteContext,
}

/// 登记一次性配对（§11.6）。
#[derive(Debug, Clone, PartialEq)]
pub struct PairingWrite {
    pub record: PairingRecord,
    pub context: WriteContext,
}

/// 原子认领（§11.6）：单事务内检查「存在、未过期、仍为 `created`、本机绑定一致」，插入唯一 peer 行
/// 并推进到 `pending_confirmation`；HMAC/proof 由调用方验证。
#[derive(Debug, Clone, PartialEq)]
pub struct PairingClaimWrite {
    pub claim: PairingClaim,
    pub context: WriteContext,
}

/// 落定配对（§11.6）：单事务完成状态/过期检查、固定 peer、创建信任记录、
/// 更新配对状态并写审计。peer 公钥从配对的对端行读回，不由调用方重复提供。
#[derive(Debug, Clone, PartialEq)]
pub struct PairingSettlementWrite {
    pub pairing: PairingId,
    pub settlement: PairingSettlement,
    pub context: WriteContext,
}

/// 过期扫描（§11.6）：只终结**未确认**（`created`/`claimed`/`pending_confirmation`）的配对，返回终结行数。
///
/// `only = None` 是时间扫描：只终结 `expires_at <= context.at` 的行（既有语义）。`only = Some(ids)` 是
/// **按标识终结**：只处理列出的行，且**不**再判 `expires_at`——它服务两个调用方：过期扫描已经从
/// `Authority::due_pairings` 得到「哪些配对的 secret 已被清除」，以及进程重启时
/// `Authority::unrecoverable_after_restart` 给出的「已无法继续验密、必须立即终结」的未确认配对。两种
/// 形态都不触碰 `approved`/`consumed`/`rejected`/`expired` 的行。
#[derive(Debug, Clone, PartialEq)]
pub struct ExpiryWrite {
    pub context: WriteContext,
    /// `None` = 按时间扫描；`Some(ids)` = 只终结这些标识（空列表是零写入）。
    pub only: Option<Vec<PairingId>>,
}

/// 消费一个已批准的配对（§11.6 第 8 条）。
///
/// `actor` 是本次认证的主体：只接受与该配对**已批准对端**一致的 `Actor::Node`/`Actor::Device`（存储层
/// 在同一事务内与 `owned_pairing_peer` 比对）；`context.audit` 必须携带与 `actor` 同主的审计行
/// （`node.authenticated`/`device.authenticated`，`SECURITY_DESIGN.md` §14.2）。
///
/// 写集在同一个事务里还推进「本次认证的对端节点行」的 `last_connected_at`（§11.6 第 9 条）：
/// `actor` 为 `Actor::Node` 时该行是 `(actor.node, NodeKind::Access)`——节点配对行只由
/// `node.pair.begin --mode owner` 创建，批准时写下的角色恒为 `access`（§11.6 第 3 条），因此这一行
/// 由写集本身唯一确定；`Actor::Device` 的配对没有节点行，不做这一步。
#[derive(Debug, Clone, PartialEq)]
pub struct PairingConsumption {
    pub pairing: PairingId,
    pub actor: Actor,
    pub context: WriteContext,
}

/// 认证成功的收尾写集（§11.6 第 9 条）：把 `(node, kind)` 角色行的 `last_connected_at` 推进到
/// `context.at`，并把 `context.audit`（该对端的 `node.authenticated` 成功行）在**同一事务**提交。
///
/// 它服务「认证成功但没有待消费配对」的重复认证；首次认证（已批准配对 → `consumed`）的同一职责由
/// [`PairingConsumption`] 在消费写集里承担。Node Link 的连接对端在本机的角色恒为 `NodeKind::Access`。
#[derive(Debug, Clone, PartialEq)]
pub struct NodeConnectedWrite {
    pub node: NodeId,
    pub kind: NodeKind,
    pub context: WriteContext,
}

/// 写入一个 workspace 记录（§11.6）。
#[derive(Debug, Clone, PartialEq)]
pub struct DirectoryWrite {
    pub record: DirectoryRecord,
    pub context: WriteContext,
}

/// 信任存储（§5.3/§11.6）：**读**面按角色/身份材料取值，**写**面一调用一个事务一个完整写集。
#[async_trait]
pub trait TrustStore: Send + Sync {
    async fn device(&self, id: &DeviceId) -> Result<Option<DeviceRecord>, PortError>;

    async fn devices(&self) -> Result<Vec<DeviceRecord>, PortError>;

    /// 按 `(NodeId, NodeKind)` 取值；同一对端可同时存在两种角色（禁止「取第一行」）。
    async fn node(&self, id: &NodeId, kind: NodeKind) -> Result<Option<NodeRecord>, PortError>;

    async fn nodes(&self) -> Result<Vec<NodeRecord>, PortError>;

    /// 该对端的全部角色行。
    async fn nodes_for(&self, id: &NodeId) -> Result<Vec<NodeRecord>, PortError>;

    /// 已绑定的身份材料（验签公钥的唯一来源）。
    async fn peer_key(&self, peer: &PeerIdentity) -> Result<Option<PeerPublicKey>, PortError>;

    async fn pairing(&self, id: &PairingId) -> Result<Option<PairingRecord>, PortError>;

    /// **全部**配对行（本机维护路径的只读列表）。
    ///
    /// 只服务「过期 / 重启终结」这类需要遍历的维护入口：不含 secret、只读、零写入；顺序按
    /// `created_at`、`pairing_id` 升序（定宽文本，字典序即时间序），因此同一快照的读取结果确定。
    /// 业务读面仍走 [`TrustStore::pairing`]/[`TrustStore::pairing_for`]（按标识或对端精确取值）。
    async fn pairings(&self) -> Result<Vec<PairingRecord>, PortError>;

    /// 已认领的对端行（确认事务从它读回公钥，§11.5）。
    async fn pairing_peer(&self, id: &PairingId) -> Result<Option<PairingPeer>, PortError>;

    /// 该对端**最近一次**配对（`design.md` D3：Node Link 握手准入手里的绑定与待消费配对来源）。
    ///
    /// 配对行本身是「登记方宣告的 `host_binding`」的唯一权威（`PairingRecord`，§11.2 第 1 条），
    /// 而节点信任行不带该列；同一对端先后可能有多条配对行，因此这里按 `created_at`、`pairing_id`
    /// 降序取一条，结果确定（同一次读取的输入决定同一次读取的输出）。没有任何配对行时返回 `None`。
    async fn pairing_for(&self, peer: &PeerIdentity) -> Result<Option<PairingRecord>, PortError>;

    async fn put_device(&self, write: DeviceWrite) -> Result<(), PortError>;

    async fn put_node(&self, write: NodeWrite) -> Result<(), PortError>;

    async fn revoke_device(&self, write: DeviceRevocation) -> Result<(), PortError>;

    async fn revoke_node(&self, write: NodeRevocation) -> Result<(), PortError>;

    async fn create_pairing(&self, write: PairingWrite) -> Result<(), PortError>;

    async fn claim_pairing(
        &self,
        write: PairingClaimWrite,
    ) -> Result<PairingClaimOutcome, PortError>;

    async fn settle_pairing(
        &self,
        write: PairingSettlementWrite,
    ) -> Result<TrustRecordRef, PortError>;

    async fn expire_pairings(&self, write: ExpiryWrite) -> Result<u64, PortError>;

    /// 消费一个已批准的配对（§11.6 第 8 条）：单事务把状态推进到 `consumed` 并写 `terminal_at`、追加
    /// `context.audit`；已是 `consumed` 且 `actor` 与对端一致时幂等成功（不覆盖首次 `terminal_at`，也
    /// 不重复写审计）。写集还推进对端节点行的 `last_connected_at`（见 [`PairingConsumption`]）。
    async fn consume_pairing(&self, write: PairingConsumption) -> Result<PairingRecord, PortError>;

    /// 认证成功的收尾写集（§11.6 第 9 条）：单事务把 `(node, kind)` 行的 `last_connected_at` 推进到
    /// `context.at`（只前进不倒退、不抹掉已存值）并追加 `context.audit`；该行不存在 →
    /// `NotFound(EntityRef::Node)`。
    async fn record_node_connected(&self, write: NodeConnectedWrite) -> Result<(), PortError>;
}

/// 本地配置存储（§11.6）：只承载目录记录；Agent 声明来自启动配置。
#[async_trait]
pub trait LocalConfigStore: Send + Sync {
    async fn directories(&self) -> Result<Vec<DirectoryRecord>, PortError>;

    async fn directory(
        &self,
        directory_id: &DirectoryId,
    ) -> Result<Option<DirectoryRecord>, PortError>;
}

/// 审计查询条件；`actions` 为空 = 不按动作过滤。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditQuery {
    pub since: Option<Timestamp>,
    pub until: Option<Timestamp>,
    pub actions: Vec<AuditAction>,
    pub actor: Option<Actor>,
    pub target: Option<EntityRef>,
    /// 返回行数上限；`None` = 不限。
    pub limit: Option<u32>,
}

#[async_trait]
pub trait AuditStore: Send + Sync {
    async fn append(&self, record: AuditRecord) -> Result<(), PortError>;

    async fn query(&self, query: AuditQuery) -> Result<Vec<AuditRecord>, PortError>;

    /// 管理写集的变更水位（`docs/NODE_LINK_PROTOCOL.md` §12.3 的 `catalogRevision`）：只增不减的
    /// 审计自增序号。
    ///
    /// 取值必须是该库**曾经写入过的最大审计序号**，不得随清理回退——信任写集各自追加一行审计，
    /// 因此它是写集的变更点，且在重启与保留期清理之后仍然单调。
    async fn watermark(&self) -> Result<u64, PortError>;
}

/// 内容寻址附件的引用。`id` 由存储层分配（`AttachmentStore::put` 不接受 id 参数）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentRef {
    pub id: AttachmentId,
    pub sha256: Digest,
    pub byte_length: u64,
    pub media_type: String,
    pub relative_path: String,
}

#[async_trait]
pub trait AttachmentStore: Send + Sync {
    /// 写入内容寻址的附件字节，返回其 sha256 与相对路径。
    async fn put(
        &self,
        bytes: &[u8],
        media_type: &str,
        at: Timestamp,
    ) -> Result<AttachmentRef, PortError>;

    async fn get(&self, id: &AttachmentId) -> Result<Option<Vec<u8>>, PortError>;

    async fn link(
        &self,
        session: &SessionId,
        attachment: &AttachmentId,
        generation: AttachmentGeneration,
    ) -> Result<(), PortError>;

    /// 按 LRU 清理到给定字节预算以下；返回被删除的附件。
    async fn prune_lru(&self, budget_bytes: u64, at: Timestamp) -> Result<PruneReport, PortError>;

    /// §7.5 的孤儿回收（§6 第 18 条）：删除 `storage.attachment_dir` 下**不在 `owned_attachment` 表里**
    /// 且 `mtime` 早于 `at` 的文件，每次最多 `limit` 个，返回实际删除数（`limit = 0` 不删）。
    ///
    /// 一律在事务之外运行（文件删除只在行事务提交之后）；失败不阻止启动。
    async fn sweep_orphans(&self, at: Timestamp, limit: u32) -> Result<u32, PortError>;
}

// ---------------------------------------------------------------------------------------------
// §5.4 发布与基础设施
// ---------------------------------------------------------------------------------------------

/// 后端事件交付通道。
///
/// §5.1 冻结其表示为「`Arc<dyn Fn(EndpointEvent) + Send + Sync>` 的包装类型」，§5.4 冻结其接口为
/// `send(&self, event)`；本类型同时满足两者（值语义 + `Clone`，因此 `SessionBackendFactory` 可按值
/// 接收并交给后端持有）。`EventSink` 的调用顺序即提交顺序（§6 第 1/3 条）。
#[derive(Clone)]
pub struct EventSink(Arc<dyn Fn(EndpointEvent) + Send + Sync>);

impl EventSink {
    pub fn new(f: impl Fn(EndpointEvent) + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    pub fn send(&self, event: EndpointEvent) {
        (self.0)(event)
    }
}

impl std::fmt::Debug for EventSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EventSink")
    }
}

/// **节点级**事件的交付通道（`design.md` D6、R8）：与 [`EventSink`] 同形（同一个 `Clone` + `send` 契约），
/// 但语义不同——它承载的是**不属于任何会话**的事件（`agent.connected`/`agent.disconnected`）。
///
/// 单独成一个类型而不是复用 `EventSink`，是为了让「节点级事件不得进入会话槽位」在类型上可见：持有一个
/// `EventSink` 意味着「这是某个会话的事件」，而本类型的消费者只能是 [`crate::broker::Broker::commit_node_event`]。
#[derive(Clone)]
pub struct NodeEventSink(Arc<dyn Fn(EndpointEvent) + Send + Sync>);

impl NodeEventSink {
    pub fn new(f: impl Fn(EndpointEvent) + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    pub fn send(&self, event: EndpointEvent) {
        (self.0)(event)
    }
}

impl std::fmt::Debug for NodeEventSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NodeEventSink")
    }
}

/// 发布已提交的投递。`publish` 不返回结果，且不得回滚已提交事务（§5.4）。
pub trait EventPublisher: Send + Sync {
    fn publish(&self, delivery: CommittedDelivery);
}

pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

/// id 分配（§3.1）：`SessionId`/`TurnId`/`InteractionId`/`PairingId`/`OriginEpoch`/`RequestId`/
/// `MessageId` 由本 trait 分配；`EventId` 由存储层在提交事务内分配（`SessionStore::commit`），
/// `AttachmentId` 由 `AttachmentStore::put` 分配并返回——它们**不**在这里，以免出现两个来源。
///
/// `SessionId` 必须由 core 在调用 `SessionBackendFactory::create` **之前**分配（§6 第 20 条）：后端
/// 要拿它构造引用，而落账提交那时才把会话行落盘——存储层因此**采用**这个 id，不再自己现分配。
pub trait IdGenerator: Send + Sync {
    /// 新建 owned 会话的标识（`session.create` 的落账提交采用它建行，§5.2 的
    /// [`StateChange::SettleCreate`]、§6 第 20 条）。
    fn session_id(&self) -> SessionId;
    fn turn_id(&self) -> TurnId;
    fn interaction_id(&self) -> InteractionId;
    fn pairing_id(&self) -> PairingId;
    /// 助手消息标识：由生产 delta 的适配器在该消息第一条 delta 提交前分配（§6 第 14 条）。
    fn message_id(&self) -> MessageId;
    fn origin_epoch(&self) -> OriginEpoch;
    fn request_id(&self) -> RequestId;
}

// ---------------------------------------------------------------------------------------------
// §5.5 目录浏览
// ---------------------------------------------------------------------------------------------

/// 目录浏览端口（§5.5/§11.9）：`workspace.browse` 的只读、逐层目录列举。
///
/// 三条约束（`sync-workspace-browse` 的「结果形状」与「无副作用」）：
///
/// - **只列目录**：只返回**直接子目录**，MUST NOT 返回文件条目，也不返回文件内容、大小、时间戳或权限
///   位——`workspace.browse` 的条目只承载名称与绝对路径，因此本端口没有承载这些事实的字段；
/// - **只读**：不写任何登记、缓存或会话状态，也不让被查看的目录因此获得任何后续权限；
/// - **顺序不作要求**：确定顺序（名称的 Unicode 码点升序）由用例层在投影时施加，端口按自己的自然顺序
///   返回即可；同一层不可读、不存在或不是目录对调用方是**同一类**事实（[`PortError::Unavailable`] 的
///   `IoError`），不得用错误区分它们，也不得回退到父层或根列表。
#[async_trait]
pub trait DirectoryBrowser: Send + Sync {
    /// 本机的根列表（`workspace.browse` 的父路径为 `null` 的分支）：每个条目是一个根挂载点/盘符。
    async fn roots(&self) -> Result<Vec<DirectoryListing>, PortError>;

    /// `parent` 的**直接**子目录。`parent` 是**已规范化**的绝对路径：形状校验（绝对、无 `..`）与
    /// 存在性/目录性判定由用例层在调用本方法**之前**完成（§5.1 的同一口径），实现只负责列举。
    async fn directories(&self, parent: &str) -> Result<Vec<DirectoryListing>, PortError>;
}

/// 目录浏览端口的一条子目录：只含名称与该目录的绝对路径。
///
/// 刻意没有大小、时间戳、权限或「是否是文件」这类字段——那些事实不在 `workspace.browse` 的下发范围内
/// （`sync-workspace-browse` 的「结果不含文件内容与元数据」），类型上不给可能性比运行时过滤更可靠。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryListing {
    /// 该子目录的名称（单层，不含任何路径分隔符）。
    pub name: String,
    /// 该子目录的绝对路径（实现侧的规范化结果；下发给客户端的形式还要经 `strip_verbatim`，见
    /// §11.9 与 `design.md` D2）。
    pub path: String,
}
