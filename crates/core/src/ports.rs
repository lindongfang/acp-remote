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
    AttachmentId, AttachmentSubject, AttachmentSubmission, AttachmentTransfer, AuditAction,
    AuditOutcome, AuditRecord, CapabilitySet, ClaimedAttachment, CommandKind, CommandRecord,
    CommandResult, CommandStatus, CommandTerminalRecord, CommittedEvent, ConfigOption,
    ConfigOptionId, ConfigValue, ConnectionId, ContentSnapshot, ContentVersion,
    CreateSessionRequest, DeviceId, DeviceRecord, Digest, DirectoryId, DirectoryRecord,
    DispatchAdmission, DispatchContext, DispatchFence, EffectiveInteraction, EndpointEvent,
    EntityRef, EventId, EventOrigin, EventPayload, EventType, FileReadPurpose,
    FirstSubmissionAdmission, FirstSubmissionConvergence, FirstSubmissionRecord,
    FirstSubmissionSettlement, FirstSubmissionWrite, FirstUserMessage, GitDiff, GitStatusPage,
    GlobalCursor, InteractionId, InteractionResolution, LocalActivity, MessageId, MessageRecord,
    ModeId, ModeRef, ModeState, NodeId, NodeKind, NodeRecord, OriginCursor, OriginEpoch,
    OriginEventRef, PairingClaim, PairingId, PairingPeer, PairingRecord, PairingSettlement,
    PeerIdentity, PeerPublicKey, PendingEvent, PendingInteraction, PortError, PromptRequest,
    PublicError, RemoteSessionRef, RequestId, ResourceGeneration, ResumeSessionRequest,
    SealedAttachment, Sequence, ServerEpoch, SessionFileChunk, SessionFilePage, SessionFilePath,
    SessionId, SessionPageToken, SessionRecoveryRecord, SessionReference, SessionSnapshot,
    SessionState, SessionSummary, SessionTitle, StagedAbortReason, StagedChunk, StagedChunkOutcome,
    StagedClaim, StagedManifestBegin, StagedManifestPage, StagedManifestSeal, StagedManifestState,
    StagedSeal, StagedSealOutcome, StagedTransferBegin, StagedTransferState, SubmissionId,
    Timestamp, Turn, TurnId, TurnState, UnavailableKind, Version,
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

    /// 带**本次派发上下文**的 prompt（`design.md` D7）。
    ///
    /// `context` 只属于这一次调用：它让适配器把该次 prompt 引起的用户输入回显（`user.message.delta`）
    /// 关联到已保存的首条消息（`session`/`firstSubmission`/`userMessage`/`request`/Agent 身份/资源代/
    /// 输入摘要），而不是按相同文本或 Agent 的 `messageId` 猜测。它**不是**持久凭据，也不进入任何
    /// wire DTO；普通 prompt（旧路径、第二条消息）传 `None`。
    ///
    /// 默认实现丢弃上下文并委托 [`SessionEndpoint::prompt`]：既有实现与测试因此无需改动，也没有任何
    /// 行为变化。需要回显关联的适配器覆盖本方法（`agent-host` 的实现把上下文随该调用的事件 sink
    /// 交给 mapper，由适配器内部调用 `associate_user_echo`：可证明重复的回显不再产出第二条
    /// `user.message.delta`；上下文本身**不**写入事件视图）。
    async fn prompt_with_context(
        &self,
        request: PromptRequest,
        at: Timestamp,
        context: Option<DispatchContext>,
    ) -> Result<TurnAccepted, PortError> {
        let _ = context;
        self.prompt(request, at).await
    }

    /// 派发前的只读能力/帧预算预检。本地 Agent 端点据此在命令受理前拒绝不支持的媒体或
    /// 超预算的 prompt，避免「先 accepted 再 turn.failed」并丢失客户端草稿。
    async fn validate_prompt(&self, request: &PromptRequest) -> Result<(), PortError>;

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
    /// （`design.md` D7）以及 `session.create` 的创建提交流程。**只**写 Agent 标题列，自定义标题
    /// （[`SessionUpdate::custom_title`]）与它互不覆盖。
    pub title: Option<Option<String>>,
    /// `owned_session.custom_title`：用户显式设置的自定义标题（`design.md` D1/D2）。
    ///
    /// `None` = 不改该列（**不是**清空：合同没有清除/恢复语义，一旦写入就只能再次改名）；
    /// `Some(title)` = 写入 trim 后的标题值。唯一写入方是 `session.title.set` 的
    /// [`crate::broker::Broker`] 管道；Agent 通知永远不碰这一列。
    pub custom_title: Option<SessionTitle>,
    pub mode: ModeChange,
    /// 一次交互解析：由存储层用**条件更新**实现 first-writer-wins（§5.2、§6.7）。
    ///
    /// 受影响行数为 0 时存储层在同一事务内回读：行存在且 `state <> 'pending'` →
    /// `PortError::Conflict(AlreadyResolved)`；无行 → `PortError::NotFound`。
    ///
    /// 仲裁只把行推进到 `resolved` 并写 `resolved_at`，**不**写 `confirmed_at`：在端点确认处理结果之前，
    /// 该行仍计入有效未解决交互（`session-activity` D2）。
    pub interaction: Option<InteractionResolved>,
    /// 一次应答**确认**：把已是 `resolved`、尚未确认的行写入 `confirmed_at`。这是该交互退出有效集合的
    /// 唯一途径（仲裁不算解除）。行不存在/不是 `resolved`/已确认时按无操作处理，不产生第二次副作用。
    pub confirmation: Option<InteractionId>,
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
    /// 版本化入口（`SYNC_PROTOCOL.md` §11.7）携带的 `expectedVersion`：`Some` 时必须等于行内
    /// `request_version`，否则整事务 `Conflict(VersionMismatch)` 且不消费请求；`None` = 旧 peer 未携带。
    pub request_version: Option<Version>,
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
    /// 命令终态。**接受提交**（`idempotency` 为 `Some`）通常本字段为 `None`——`CommandTerminalRecord`
    /// 只能表达终止态（§3.3），accepted 行的 `status`/`accepted_at` 由 `idempotency` 决定。
    ///
    /// **同一提交既是接受也是终态**（`session.title.set`，`design.md` D2）：`idempotency` 与
    /// `command_terminal` 同时为 `Some` 时，存储层在**同一事务**里写幂等行（`status` 直接取终态）、
    /// 该批次的 `session.updated`/`command.completed` 事件，并把 `terminal_event` 回填为批次里
    /// `causation = requestId` 的那条终态事件；不再定位既有 `accepted` 行（本次提交才建它）。
    /// 这种形状要求批次里**必须**有该请求的终态事件，否则整事务 `InvalidRequest`。
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

/// prompt 受理时需要与命令同事务绑定的附件清单（`SYNC_PROTOCOL.md` §11.5 的 `submissionId`）。
///
/// `attachments` 是清单的**权威顺序**（与 `AttachmentSubmission::attachments` 逐项相同）；存储层在同一
/// 事务里重新核对「清单已封存、会话/主体一致、每个附件仍存在且文件可读」，任一失败整事务回滚，
/// 因此命令不会被受理，也不会派发给 Agent。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmissionBinding {
    pub submission: SubmissionId,
    pub attachments: Vec<AttachmentId>,
    /// 认证主体（`device`/`node`）：存储层在**同一事务**里与清单行的 `subject_kind`/`subject_id`
    /// 逐项比较，任一不符整事务回滚。主体因此不只由调用方在事务外校验（`UseCases` 也只做预检）——
    /// 任何直接调用 `commit_with_submission` 的路径都不能绕过它。
    pub subject: AttachmentSubject,
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

/// 会话消息的分页游标：`(createdAt, messageId)` 复合游标。
///
/// `owned_message` 没有会话内序号，正文页按「创建时间 + 标识」稳定排序（同毫秒的消息靠标识决定先后）。
/// 它只作用于 [`HistoryPage::messages`]，与 `events` 的 `after`（全局游标）和 `history` 的
/// `before_history`（会话内序号）各自独立分页；取值只能来自上一页最早一条
/// [`MessageRecord::created_at`]/[`MessageRecord::message_id`]，实现不得据此猜测或补全。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageCursor {
    pub created_at: Timestamp,
    pub message_id: MessageId,
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
    /// 消息正文页的游标：只返回 `(created_at, message_id)` **严格早于**它的 `owned_message` 行；
    /// `None` = 最新一页。与 `before_history` 相同，只影响 [`HistoryPage::messages`].
    pub before_messages: Option<MessageCursor>,
    /// 消息正文页大小（与 `limit` 各自独立分页）：实现最多返回这么多条 `HistoryPage::messages`
    /// （并据多取一条决定页标记），`0` 表示本请求不取正文。
    pub messages_limit: ReplayLimit,
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
    /// 该会话的权威消息正文（`owned_message`；`include.messages` 为假或 `messages_limit` 为 0 时为空）。
    ///
    /// 回复与思考分别持久化（`channel` 区分），停止/失败/断线保留已收到正文并标记 `result`；
    /// 正文只从这里取，**不**从 `events`/`history` 重复拼接（§7.6）。
    ///
    /// **一页**：按 `(created_at, message_id)` 升序返回，最多 `HistoryQuery::messages_limit` 条；
    /// 省略 `before_messages` 时是最新一页，带游标时是严格更早的一页。
    pub messages: Vec<MessageRecord>,
    /// [`HistoryPage::messages`] 之前是否仍有更早消息；关闭 `include.messages` 或 `messages_limit = 0`
    /// 时恒为假（本页不是消息页，不能据此推断「没有更早消息」）。
    pub messages_has_earlier: bool,
    pub turns: Vec<Turn>,
    pub interactions: Vec<PendingInteraction>,
    pub config: Vec<ConfigOption>,
    pub capabilities: Option<CapabilitySet>,
    /// 本页数据对应的一致性视图 head（barrier）。
    pub head: GlobalCursor,
    /// 续读游标；`None` = 已到 `head`。
    pub next: Option<GlobalCursor>,
}

/// [`SessionStore::session_activity`] 的读取结果：会话三态归约的全部输入。
///
/// 计数口径（与 `owned_interaction`/`owned_turn` 的落库语义一致）：
/// - `unresolved_interactions`：未失效、且 `state = 'pending'` 或（`resolved` 且未确认）的行数；
/// - `unconfirmed_interactions`：其中已仲裁未确认的行标识（确认后减法只作用于这些行）；
/// - `unresolved_by_turn`：未失效未解决交互按 `turn_id` 的分组计数（`turn_id IS NULL` 的历史行不出现）；
/// - `unfinished_turns`：`state IN ('queued','running','waiting_input','waiting_permission')` 的 turn 数。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionActivityFacts {
    pub unresolved_interactions: u32,
    pub unconfirmed_interactions: Vec<InteractionId>,
    pub unresolved_by_turn: Vec<(TurnId, u32)>,
    pub unfinished_turns: u32,
}

impl SessionActivityFacts {
    /// 该 turn 结束时会被一并失效的未解决交互数。
    pub fn unresolved_for_turn(&self, turn: &TurnId) -> u32 {
        self.unresolved_by_turn
            .iter()
            .find(|(candidate, _)| candidate == turn)
            .map(|(_, count)| *count)
            .unwrap_or(0)
    }
}

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn commit(&self, commit: OwnedCommit) -> Result<CommitOutcome, PortError>;

    /// 会话活跃程度的权威事实（`session-activity` D1/D2）：由存储层在**同一事务口径**下读出，
    /// 调用方据此计算 [`SessionActivity::reduce`](crate::model::SessionActivity::reduce) 的输入。
    ///
    /// 「有效未解决交互」= `state = 'pending'` 或 `state = 'resolved'` 但 `confirmed_at IS NULL`，
    /// 且未失效（`invalidated_at IS NULL`）。`unconfirmed_interactions` 是其中已仲裁未确认的子集，
    /// 供确认路径计算「确认后是否仍有待处理」。`unresolved_by_turn` 按 `turn_id` 分组，供 turn 终态
    /// 提交计算同一事务里被失效的那些请求。
    async fn session_activity(
        &self,
        session: &SessionId,
    ) -> Result<SessionActivityFacts, PortError>;

    /// 该会话的持久队列（`owned_turn.state = 'queued'`），按 `queue_index` 升序。
    ///
    /// 只承载 turn 元数据：`Session.prompt` 的正文不在 `owned_turn` 里（它只由 Agent 的
    /// `user.message.delta` 回显持久化），因此重启后不能凭本方法重放 Agent 副作用；恢复流程据它
    /// 识别并结算遗留排队项，再继续 FIFO（`session-activity` D3）。
    async fn queued_turns(&self, session: &SessionId) -> Result<Vec<Turn>, PortError>;

    /// 启动恢复用：仍有未结束 turn 或有效未解决交互的会话，最多 `limit` 个。
    ///
    /// 恢复据此在这些会话上重算摘要并驱动持久队列（`session-activity` 迁移计划的第 3 步）。
    async fn sessions_with_pending_activity(
        &self,
        limit: ReplayLimit,
    ) -> Result<Vec<SessionId>, PortError>;

    /// 与 [`SessionStore::commit`] 相同的原子提交，并在**同一事务**里绑定 prompt 的附件清单
    /// （§5.8/§6 第 22 条）：重新核对清单已封存、会话/主体一致、每个附件仍存在且文件可读，
    /// 再把附件引用写入会话绑定。
    ///
    /// 默认实现只支持 `binding = None`：不实现该能力的存储（测试替身或旧适配器）收到清单时显式
    /// `InvalidRequest`，绝不静默丢掉绑定后继续受理。
    async fn commit_with_submission(
        &self,
        commit: OwnedCommit,
        binding: Option<SubmissionBinding>,
    ) -> Result<CommitOutcome, PortError> {
        match binding {
            None => self.commit(commit).await,
            Some(_) => Err(PortError::InvalidRequest(
                "this SessionStore does not implement attachment submission binding",
            )),
        }
    }

    /// Actor-scoped terminal commit for workspace.mkdir only. Same atomicity as commit,
    /// but locates the unsessioned command by the full (actor, requestId) key.
    async fn commit_workspace_mkdir_terminal(
        &self,
        _actor: &Actor,
        _commit: OwnedCommit,
    ) -> Result<CommitOutcome, PortError> {
        Err(PortError::InvalidRequest(
            "workspace.mkdir terminal commits are unsupported",
        ))
    }

    async fn load(&self, session: &SessionId) -> Result<Option<SessionSnapshot>, PortError>;

    /// 会话列表。返回的每个 [`SessionSummary`] 的 `workspace` **必须**取自该会话行持久化的
    /// `owned_session.workspace_directory_id`（展示名按别名关联目录记录取得，取不到时回退为别名本身）；
    /// 别名为 `NULL` 时就是 `None`（未分组）。
    ///
    /// 实现 MUST NOT 按 `owned_session.workspace_cwd` 反查别名解析表来补出归属：`NULL` 就是未分组，
    /// 「不反查」是恢复与归属稳定（别名重指向不改变既有会话归属）的前提（§3.6、`design.md` D1/D6）。
    async fn list(&self, query: SessionQuery) -> Result<Vec<SessionSummary>, PortError>;

    /// 批量读取会话的 origin epoch（`owned_session.origin_epoch`）。
    ///
    /// 唯一生产调用方是 Node Link 的 catalog 投影（`NODE_LINK_PROTOCOL.md` §12.3 的
    /// `catalog.session.originEpoch`）：catalog 是 Access 在收到第一个 `resource.event` **之前**唯一
    /// 能取得该会话 origin epoch 的地方，缺它会让 imported 会话在本地不可发现。读面只返回存在的会话；
    /// 不存在的标识不出现在结果里（调用方按「未知」处置，不补值）。本方法只读、无正文、不改 cursor。
    async fn origin_epochs(
        &self,
        sessions: &[SessionId],
    ) -> Result<Vec<(SessionId, OriginEpoch)>, PortError>;

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

/// 一次带来源位置的会话读取（`ReadView::read_session_origin`）：页、来源位置 R 与逐消息覆盖。
#[derive(Debug, Clone)]
pub struct OriginReadPage {
    pub page: HistoryPage,
    pub read_cursor: OriginCursor,
    pub coverage: Vec<MessageCoverage>,
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
    /// 与本 trait 的 [`ReadView::replay`] 的分工：`replay` 服务**本机**客户端的 owned 全局重放
    /// （给出 `reset_required`/`next`）；本方法只服务入站设备的
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

    /// Owner 侧一次原子读取：在**同一个只读事务**里取得页数据、来源位置与逐消息增量覆盖
    /// （`design.md` D4）。实现必须先捕获来源位置 R，再在同一事务内取页，最后按 R 之前的 delta 计算
    /// 每条返回消息的连续覆盖；不得读取结束后另开事务采样 head。
    ///
    /// 没有来源位置的只读视图必须显式返回不支持，不得伪造 R。
    async fn read_session_origin(&self, query: HistoryQuery) -> Result<OriginReadPage, PortError>;

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
    pub events: Vec<CommittedEvent>,
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
    /// 随正文保留期清理的消息行（`owned_message`；媒体引用随外键级联删除）。
    pub removed_messages: u64,
    pub freed_bytes: u64,
    /// 清理后仍超出容量上限：写路径必须返回 `PortError::Unavailable(UnavailableKind::StorageFull)`。
    pub still_over_limit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreHealth {
    /// §7.5 的容量度量现值（owned 家族 TEXT 列 + 附件字节）；调用方据此解释 `still_over_limit`。
    pub total_bytes: u64,
    /// `PRAGMA quick_check` 的结果；`false` 即只读失败关闭。
    pub integrity_ok: bool,
    /// 进入只读失败关闭（写路径全部返回 `PortError::Corrupt`）。
    pub read_only: bool,
    pub user_version: u32,
    pub owned_schema_version: u32,
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

    /// 建立会话对附件的归属（附件授权与 GC 根的唯一来源）。
    ///
    /// `at` 是本链接的绑定时刻：实现必须把附件的 `last_used_at` 刷新到它。按引用 GC 与消息保留期
    /// 回收都以「最近一次使用」为界（§7.5），内容寻址去重让旧字节复用同一个附件行（行上保留**首次**
    /// 上传的 `created_at`），所以刚绑定的旧内容只能靠这次刷新免于下一轮清理。
    async fn link(
        &self,
        session: &SessionId,
        attachment: &AttachmentId,
        generation: AttachmentGeneration,
        at: Timestamp,
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
// §5.8 分块附件与有效交互（运行时无关）
// ---------------------------------------------------------------------------------------------

/// 开始一次附件传输（`attachment.begin`）。
///
/// `transfer` 由客户端给出（该次传输的稳定标识）：`chunk`/`seal`/`status`/`abort` 复用它。
/// `total_length` 是无损整数（wire 上是十进制字符串）；`at` 由调用方传入，存储层不读系统时钟。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferBegin {
    pub transfer: AttachmentId,
    pub session: SessionId,
    pub subject: AttachmentSubject,
    pub total_length: u64,
    pub media_type: String,
    pub file_name: String,
    pub at: Timestamp,
}

/// 追加一块原始数据（`attachment.chunk`）。`data.len()` 不得超过
/// [`ATTACHMENT_CHUNK_SIZE`](crate::model::ATTACHMENT_CHUNK_SIZE)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferChunk {
    pub transfer: AttachmentId,
    pub offset: u64,
    pub data: Vec<u8>,
    pub at: Timestamp,
}

/// 块确认：`next_offset` 是该传输已确认的下一偏移。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkOutcome {
    pub next_offset: u64,
    pub duplicate: bool,
}

/// 封存传输（`attachment.seal`）：`length` 是完整长度、`sha256` 是整段原始字节的流式摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferSeal {
    pub transfer: AttachmentId,
    pub length: u64,
    pub sha256: Digest,
    pub at: Timestamp,
}

/// 开始登记一次提交清单（`attachment.manifest_begin`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmissionBegin {
    pub submission: SubmissionId,
    pub session: SessionId,
    pub subject: AttachmentSubject,
    pub attachment_count: u64,
    pub at: Timestamp,
}

/// 一页清单（`attachment.manifest_page`）。`start_position` 是该页第一项的清单位置；
/// `attachments` 的长度不得超过 [`ATTACHMENT_MANIFEST_PAGE`](crate::model::ATTACHMENT_MANIFEST_PAGE)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmissionPage {
    pub submission: SubmissionId,
    pub start_position: u64,
    pub attachments: Vec<AttachmentId>,
    pub at: Timestamp,
}

/// 清单登记进度（`begin_submission`/`append_submission_page` 的返回值）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmissionProgress {
    pub submission: SubmissionId,
    /// 已登记项数。
    pub registered: u64,
    /// 客户端声明的总数。
    pub attachment_count: u64,
    pub sealed: bool,
}

/// 封存清单（`attachment.manifest_sealed`）：整批复核（会话/主体/封存/数量/摘要）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmissionSeal {
    pub submission: SubmissionId,
    pub session: SessionId,
    pub subject: AttachmentSubject,
    /// 与分页登记的内容**逐项相同**的整批清单（顺序即清单顺序）。
    pub attachments: Vec<AttachmentId>,
    pub digest: Digest,
    pub at: Timestamp,
}

/// 读取已封存附件的一段（`attachment.read`）。`max_length` 上限为 32 KiB。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentRead {
    pub attachment: AttachmentId,
    pub session: SessionId,
    pub offset: u64,
    pub max_length: u64,
}

/// 一段附件数据（`attachment.read_chunk`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentChunk {
    pub offset: u64,
    pub data: Vec<u8>,
    pub total_length: u64,
    pub eof: bool,
}

/// 分块附件端口（`docs/CORE_PORTS_AND_STORAGE.md` §5.8）：临时文件 + 元数据、流式摘要、清单与读取。
///
/// 与 [`AttachmentStore`]（整段字节的旧路径）的分工：新路径**不接受整文件字节**，内存工作集只与单块
/// 大小有关，不随正文增大。两个端口都由 `storage-sqlite` 实现，组合根负责装配。
#[async_trait]
pub trait AttachmentTransferStore: Send + Sync {
    /// 建临时文件与传输行（只创建，不写入任何正文）。
    async fn begin_transfer(&self, write: TransferBegin) -> Result<AttachmentTransfer, PortError>;

    /// 按期望偏移追加一块；重复同内容返回 `duplicate: true`，不同内容返回
    /// [`ConflictKind::AlreadyExists`](crate::model::ConflictKind::AlreadyExists) 语义的冲突。
    async fn append_chunk(&self, write: TransferChunk) -> Result<ChunkOutcome, PortError>;

    /// 校验长度与流式摘要后封存：文件先写临时名，校验通过后 rename，再允许后续业务事务引用。
    async fn seal_transfer(&self, write: TransferSeal) -> Result<SealedAttachment, PortError>;

    async fn transfer(
        &self,
        transfer: &AttachmentId,
    ) -> Result<Option<AttachmentTransfer>, PortError>;

    /// 取消一次**仍在接收**的传输；已封存/已提交的附件不可取消。
    async fn abort_transfer(
        &self,
        transfer: &AttachmentId,
        at: Timestamp,
    ) -> Result<AttachmentTransfer, PortError>;

    /// 开始登记清单（`attachment_count` 是客户端声明的总数，封存时整批核对）。
    async fn begin_submission(
        &self,
        write: SubmissionBegin,
    ) -> Result<SubmissionProgress, PortError>;

    /// 追加一页清单（幂等：同一位置重复同值是 `duplicate` 语义，不同内容拒绝）。
    async fn append_submission_page(
        &self,
        write: SubmissionPage,
    ) -> Result<SubmissionProgress, PortError>;

    /// 封存整份清单：所有附件均已封存、同一会话/主体、顺序与分页登记一致、摘要与数量相符。
    async fn finalize_submission(
        &self,
        write: SubmissionSeal,
    ) -> Result<AttachmentSubmission, PortError>;

    async fn submission(
        &self,
        submission: &SubmissionId,
    ) -> Result<Option<AttachmentSubmission>, PortError>;

    /// 读取一段已封存、且绑定到该会话的附件数据（`data.len() <= min(max_length, 32 KiB)`）。
    async fn read_chunk(&self, request: &AttachmentRead) -> Result<AttachmentChunk, PortError>;

    /// 清理闲置超过 24 小时的 `receiving` 传输（条件更新 + 事务提交后删文件），返回清理数。
    async fn prune_transfers(&self, at: Timestamp, limit: u32) -> Result<u32, PortError>;
}

/// 有效待处理交互端口（`docs/CORE_PORTS_AND_STORAGE.md` §5.8、`SYNC_PROTOCOL.md` §11.7）。
///
/// 只承载**读**与**失效**：应答仲裁与确认都走 `SessionStore::commit`（`SessionUpdate.interaction` /
/// `SessionUpdate.confirmation`），因此交互阶段、会话摘要与 `session.updated` 事件永远在同一事务里。
/// 首个有效答案仍由存储层的**条件更新**选出（版本/状态/失效标记同一条 `WHERE`），失败校验不消费请求。
#[async_trait]
pub trait InteractionAuthority: Send + Sync {
    /// 当前有效（`pending` 且未失效）的完整请求；`options`/`schema` 来自创建它的交互事件。
    ///
    /// 已仲裁但未确认的行**不**在这里出现：它们阻止其他设备再次应答（条件更新会得到
    /// `AlreadyResolved`），但仍计入会话摘要的待处理数（`SessionStore::session_activity`）。
    async fn effective(&self, session: &SessionId) -> Result<Vec<EffectiveInteraction>, PortError>;

    /// 把有效未解决行（`pending`，或 `resolved` 但未确认）标记为失效（上游丢失/重启恢复）：
    /// `session = None` 表示全部会话。失效行不可复活，也不参与 `effective` 与三态归约。
    async fn invalidate_pending(
        &self,
        session: Option<&SessionId>,
        at: Timestamp,
    ) -> Result<u32, PortError>;
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
    fn publish(&self, event: CommittedEvent);
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

/// Independent directory creation command boundary.
#[async_trait]
pub trait DirectoryCreator: Send + Sync {
    async fn create_directory(&self, parent: &str, name: &str) -> Result<(), PortError>;
}

/// Independent directory creation command boundary.
#[derive(Debug)]
pub enum WorkspaceMkdirAdmission {
    Accepted(WorkspaceMkdirExecution),
    InFlight,
    Settled(Box<CommandRecord>),
}

/// One-time execution ticket; Debug omits host paths and names.
pub struct WorkspaceMkdirExecution {
    pub(crate) actor: Actor,
    pub(crate) request: RequestId,
    pub(crate) parent: String,
    pub(crate) name: String,
}

impl std::fmt::Debug for WorkspaceMkdirExecution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkspaceMkdirExecution")
            .finish_non_exhaustive()
    }
}

/// 会话文件与 Git 的**只读**出站端口（`docs/CORE_PORTS_AND_STORAGE.md` §5.7）。
///
/// 三条约束与 `workspace.browse` 的目录浏览端口同源，但作用域是**当前会话的实际工作目录**：
///
/// - **根由调用方给出**：`root` 是用例层从 `SessionStore::load_recovery().workspace_cwd` 取得的
///   规范化绝对路径；实现 MUST NOT 按 workspace 别名反查、猜测或回退到其它目录。
/// - **路径只接受根内相对形状**：类型 `SessionFilePath` 已在 core 构造期拒绝绝对路径、盘符、UNC、
///   父引用、NUL、ADS 冒号与 `.git`；实现仍必须以实际打开的对象验证最终路径位于规范根内（D3），
///   不能只用字符串前缀比较。
/// - **只读、无正文落盘**：不写索引、工作树、缓存或会话状态；内容快照只在内存中存在，连接断开、
///   取消、设备撤销或空闲到期即释放，不写临时正文文件。整个实现不得把 Tokio、文件系统、Git 子进程
///   或 wire DTO 带进 core。
#[async_trait]
pub trait SessionFiles: Send + Sync {
    /// 列出 `path` 的直接子项（`None` = 会话根），按目录优先、名称码点、路径打破并列的确定顺序。
    /// `hide_ignored` 为真时按 Git 有效忽略规则过滤；实现必须在分页前完成过滤。
    async fn list(
        &self,
        root: &str,
        path: Option<&SessionFilePath>,
        hide_ignored: bool,
        after: Option<&SessionPageToken>,
    ) -> Result<SessionFilePage, PortError>;

    /// 打开一个连接绑定的只读内容快照。`preview` 只取得该用途的预算并给出截断标记；`download` 在
    /// 超过下载预算（`SESSION_FILE_DOWNLOAD_MAX_BYTES`，64 MiB）时 MUST 返回
    /// `PortError::InvalidRequest(RESULT_TOO_LARGE)`，不得悄悄只取前一段。`preview` 不设原始大小上限：
    /// 文件更大时仍如实给出 `size_bytes` 并置 `preview_truncated`。
    async fn open_content(
        &self,
        root: &str,
        path: &SessionFilePath,
        purpose: FileReadPurpose,
    ) -> Result<ContentSnapshot, PortError>;

    /// 从既有快照读取一个字节块。`version` 必须匹配同一连接/设备/会话/路径建立的快照；文件版本
    /// 变化时实现必须废弃快照并明确要求重新读取，不得拼接两个版本。
    async fn read_content(
        &self,
        version: &ContentVersion,
        offset: u64,
    ) -> Result<SessionFileChunk, PortError>;

    /// 释放一个内容快照（幂等）。连接断开、取消、撤销与空闲到期都走同一入口。
    async fn release_content(&self, version: &ContentVersion) -> Result<(), PortError>;

    /// Git status（相对 HEAD 与工作树合并），只覆盖会话根内文件。
    async fn git_status(
        &self,
        root: &str,
        after: Option<&SessionPageToken>,
    ) -> Result<GitStatusPage, PortError>;

    /// Git 文本差异（HEAD 与当前工作树；无 HEAD 时以空基准）。实现不得运行仓库自定义命令。
    async fn git_diff(&self, root: &str, path: &SessionFilePath) -> Result<GitDiff, PortError>;
}

/// `SessionFiles` 的默认实现：未装配原生适配器时一律失败关闭。
///
/// 与 `SystemDirectories` 不同，core 不能自带文件/Git 实现（`AGENTS.md` §5：core 不依赖文件系统、
/// 子进程或 Tokio）；组合根必须注入 `app::session_files` 的实现。默认值只保证 `UseCases::new` 的
/// 既有字面量构造点不被打断，并让「忘了装配」显式失败而不是返回空结果。
pub(crate) struct UnavailableSessionFiles;

#[async_trait]
impl SessionFiles for UnavailableSessionFiles {
    async fn list(
        &self,
        _root: &str,
        _path: Option<&SessionFilePath>,
        _hide_ignored: bool,
        _after: Option<&SessionPageToken>,
    ) -> Result<SessionFilePage, PortError> {
        Err(PortError::Unavailable(UnavailableKind::IoError))
    }

    async fn open_content(
        &self,
        _root: &str,
        _path: &SessionFilePath,
        _purpose: FileReadPurpose,
    ) -> Result<ContentSnapshot, PortError> {
        Err(PortError::Unavailable(UnavailableKind::IoError))
    }

    async fn read_content(
        &self,
        _version: &ContentVersion,
        _offset: u64,
    ) -> Result<SessionFileChunk, PortError> {
        Err(PortError::Unavailable(UnavailableKind::IoError))
    }

    async fn release_content(&self, _version: &ContentVersion) -> Result<(), PortError> {
        Ok(())
    }

    async fn git_status(
        &self,
        _root: &str,
        _after: Option<&SessionPageToken>,
    ) -> Result<GitStatusPage, PortError> {
        Err(PortError::Unavailable(UnavailableKind::IoError))
    }

    async fn git_diff(&self, _root: &str, _path: &SessionFilePath) -> Result<GitDiff, PortError> {
        Err(PortError::Unavailable(UnavailableKind::IoError))
    }
}

// ---------------------------------------------------------------------------------------------
// §5.9 远程运行期目录与在线回源
// ---------------------------------------------------------------------------------------------

/// 一个远程来源（Owner 节点）在当前已认证连接期的状态（`design.md` D2/D6）。
///
/// 它只描述**连接事实**，不是持久化的资源状态：`Online` 只在当前连接通过认证且目录已取得时成立；
/// 断开、撤销与 Owner epoch 改变都会让整代目录失效（[`RemoteRuntimeDirectory::invalidate`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RemoteSourceState {
    /// 当前连接可用，目录与回源读取有效。
    Online,
    /// 当前没有已认证连接（运行期条目已按代清除）。
    Offline,
    /// 本机已撤销对该 Owner 的信任（连接必须关闭，条目不得继续渲染）。
    Revoked,
}

impl RemoteSourceState {
    /// 是否可继续提供远程资源。
    pub const fn is_online(self) -> bool {
        matches!(self, Self::Online)
    }
}

/// 远程运行期目录的代际：每次重新认证/重新取得目录都推进，旧的异步结果据此丢弃。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RemoteGeneration(u64);

impl RemoteGeneration {
    /// 构造。
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// 底层数值。
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Owner 侧目录 revision（`catalogRevision` 的 core 侧取值）。
///
/// 只用于判定「同一 Owner 的目录是否发生了变化」；比较在同一 Owner 内进行，跨 Owner 无意义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RemoteCatalogRevision(u64);

impl RemoteCatalogRevision {
    /// 构造。
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// 底层数值。
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// 运行期目录中的一条远程会话摘要。
///
/// `summary.origin` 必须是 [`crate::model::ResourceOrigin::Remote`]，且 `online` 必须与 `source`
/// 一致；`generation` 是取得该条目时的目录代际（用于丢弃旧连接留下的异步结果）。core 在
/// [`crate::use_cases::UseCases::sync_visible_sessions`] 与回源读取前校验这些来源一致性，不信任
/// 适配器把本地会话混进远程目录。
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteSessionEntry {
    pub summary: SessionSummary,
    pub generation: RemoteGeneration,
    pub source: RemoteSourceState,
}

/// 一个 Owner 的目录快照（同一已认证连接期内的一致性视图）。
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteDirectorySnapshot {
    pub owner_node_id: NodeId,
    pub generation: RemoteGeneration,
    pub revision: RemoteCatalogRevision,
    pub source: RemoteSourceState,
    pub sessions: Vec<RemoteSessionEntry>,
}

/// 远程运行期目录端口（`design.md` D2/D6；spec `stateless-access`、`sync-event-delivery`）。
///
/// **生命期**：实现只在已认证连接期持有数据；连接断开、信任撤销或 Owner epoch 改变时调用
/// [`RemoteRuntimeDirectory::invalidate`] 整代清除，并取消该代的所有读写任务。目录与其中的摘要
/// **不得**写入数据库、临时文件或日志，也不得跨重启恢复。
///
/// **有界**：目录、快照与列表沿用协议既有的目录/帧/队列/超时限额，不新增无界缓存；`list` 返回的
/// 条目数量必须受这些限额约束，超出时由实现显式失败或截断并报告，而不是静默增长。
#[async_trait]
pub trait RemoteRuntimeDirectory: Send + Sync {
    /// 当前所有在线 Owner 的会话条目（连接期视图；本进程没有远程运行期时为空）。
    async fn list(&self) -> Result<Vec<RemoteSessionEntry>, PortError>;

    /// 指定 Owner 的一致性目录快照；未认证或已失效时为 `None`。
    async fn snapshot(&self, owner: &NodeId) -> Result<Option<RemoteDirectorySnapshot>, PortError>;

    /// 按 `(ownerNodeId, sessionId)` 精确查找一条条目。
    async fn lookup(
        &self,
        session: &RemoteSessionRef,
    ) -> Result<Option<RemoteSessionEntry>, PortError>;

    /// 只按会话标识查找**所有** Owner 下的匹配条目。
    ///
    /// 调用方（core）必须自己处理 0/1/多条的歧义：不同 Owner 的同名 `SessionId` 不是同一会话，
    /// 多条命中时不得挑选其中任意一条。本方法不用于可见性判定。
    async fn find_by_session(
        &self,
        session: &SessionId,
    ) -> Result<Vec<RemoteSessionEntry>, PortError>;

    /// 指定 Owner 的当前状态；从未建立过连接时返回 `Offline`（不虚构 `Revoked`）。
    async fn owner_state(&self, owner: &NodeId) -> Result<RemoteSourceState, PortError>;

    /// 指定 Owner 的当前代际；没有任何有效目录时为 `None`。
    async fn generation(&self, owner: &NodeId) -> Result<Option<RemoteGeneration>, PortError>;

    /// 使指定代际失效（断开/撤销/epoch 改变）。只有 `generation` 与当前代际相等时才清除，因此旧连接
    /// 迟到的失效不会误删新连接建立的目录；返回后该代的所有条目与未完成结果都不可再被读到。
    async fn invalidate(
        &self,
        owner: &NodeId,
        generation: RemoteGeneration,
    ) -> Result<(), PortError>;
}

/// 一条已返回消息在某个读取边界上的增量覆盖（`design.md` D4）。
///
/// `through` 恒等于该次读取结果的 `read_cursor`；`last_delta_index` 是消息在当前完整内容中已覆盖的
/// **最后连续**增量索引（没有增量时为 `None`）。覆盖只适用于本次返回的消息，不得用来丢弃未返回消息
/// 的缓冲事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageCoverage {
    pub message_id: MessageId,
    pub through: OriginCursor,
    pub last_delta_index: Option<u64>,
}

/// 远程历史的一项：来源稳定标识 + 事件类型 + 原样正文。
///
/// 不复用 [`CommittedEvent`]：那是 Access 本地事件定位（带 `global_sequence`/`session_sequence`），
/// 远程历史只认 Owner 的 `originEventId` 与来源序号，不得冒用本地序号。
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteHistoryRecord {
    pub origin: OriginEventRef,
    /// Owner 的**来源会话内序号**（`originSequence` 的同一数值域）。
    ///
    /// 它只是从 Owner 的结果里原样取出的分页键（`beforeHistory` 用它续读），**不是** Access 分配的
    /// 本地 `session_sequence`：跨重启后它仍指向 Owner 的同一条事件。
    pub origin_sequence: Sequence,
    pub event_type: EventType,
    pub payload: EventPayload,
    pub created_at: Timestamp,
}

/// 远程会话的一页权威读取结果（`session.read` 的回源形状；`design.md` D4）。
///
/// `messages`/`history`/`summary`/`read_cursor`/`coverage` 必须来自 Owner 的**同一次**只读边界：
/// `read_cursor` 是该次读取捕获的来源位置，`coverage` 只覆盖本次返回的消息。正文只在调用栈与有界
/// 运行期内存中存在，不写数据库、临时文件或日志。
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteReadResult {
    pub summary: SessionSummary,
    pub read_cursor: OriginCursor,
    pub messages: Vec<MessageRecord>,
    pub history: Vec<RemoteHistoryRecord>,
    pub coverage: Vec<MessageCoverage>,
    /// `messages` 这一页之前是否仍有更早消息（Owner `session.read` 的 `hasEarlier` 原值）。
    ///
    /// 它与 [`Self::history_has_earlier`] 各自分页、互不影响；本端口不重新推断页标记，缺了它调用方
    /// 只能猜「这一页是不是最后一页」。
    pub messages_has_earlier: bool,
    /// `history` 这一页之前是否仍有更早条目（Owner `session.read` 的 `historyHasEarlier` 原值）；
    /// 未请求 `history` 时为 `false`。
    pub history_has_earlier: bool,
}

/// 远程读取的入参（与 [`ImportedReadQuery`] 同形，去掉只对本地 owned 事件日志有意义的字段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteReadQuery {
    pub include: HistoryInclude,
    pub before_history: Option<Sequence>,
    pub before_messages: Option<MessageCursor>,
    pub messages_limit: ReplayLimit,
    pub limit: ReplayLimit,
}

/// 来源重放/追平屏障：Owner 已把 `after` 之后的事件重放到 `cursor`，之后才允许发实时事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteSourceBarrier {
    pub cursor: OriginCursor,
    pub generation: RemoteGeneration,
}

/// 远程来源事件流与回源读取端口（`design.md` D2/D4；spec `sync-event-delivery`、
/// `sync-session-history`）。
///
/// **生命期**：调用方必须先经 [`RemoteRuntimeDirectory`] 校验来源身份与在线状态，并传入该条目的
/// `generation`；连接断开/撤销后实现必须让未完成的读取以 `Unavailable` 结束，且结果不得继续被返回。
/// 读取结果只在内存中存在，不落盘。
#[async_trait]
pub trait RemoteSourceReads: Send + Sync {
    /// 读取 Owner 当前的权威页（摘要 + 来源位置 + 消息 + 历史 + 覆盖）。
    async fn read(
        &self,
        session: &RemoteSessionRef,
        generation: RemoteGeneration,
        query: RemoteReadQuery,
    ) -> Result<RemoteReadResult, PortError>;

    /// 取得来源重放屏障：`after` 为 `None` 时从 Owner 当前元数据快照取得屏障（首次重建）。
    async fn replay_barrier(
        &self,
        session: &RemoteSessionRef,
        generation: RemoteGeneration,
        after: Option<OriginCursor>,
    ) -> Result<RemoteSourceBarrier, PortError>;
}

/// 来源流重置的原因（`sync.remote_reset_required.reason` 的 core 侧取值，`design.md` D4）。
///
/// `CacheIncompatible` 只用于「首次重建」：客户端当前不持有可兼容的远程状态，必须回源
/// [`RemoteSourceReads::read`] 重建明细；它不是 `cursor_invalid` 的一种。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RemoteStreamResetReason {
    EpochMismatch,
    CursorExpired,
    CacheIncompatible,
}

/// 来源流里的一条事件（Owner 的 origin 事件，正文逐字节保真，`design.md` D3）。
///
/// `payload` 直接承载 core 的 [`EventPayload`]（`view` 是 Owner 的原文、`acp` 是原始 ACP 文本），
/// 因此适配层不需要、也不得重新序列化正文；`payload_digest` 是 Owner 对该 payload 的
/// `payloadDigest`，逐字保留，不重新计算。
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteStreamEvent {
    pub owner_node_id: NodeId,
    pub session_id: SessionId,
    pub origin_event_id: EventId,
    pub origin_epoch: OriginEpoch,
    pub origin_sequence: Sequence,
    pub event_type: EventType,
    pub payload_digest: Digest,
    pub created_at: Timestamp,
    pub payload: EventPayload,
}

/// 来源流的下一项（与 `sync.remote_*` 的帧一一对应，`design.md` D3/D4）。
///
/// 顺序不变量：`CaughtUp` 之前的 `Event` 都是 Owner 的重放，之后才是实时事件；`Reset`/`Unavailable`
/// 终止本次订阅（调用方必须取消该订阅，不得继续消费）。
#[derive(Debug, Clone, PartialEq)]
pub enum RemoteStreamItem {
    Event(RemoteStreamEvent),
    CaughtUp(RemoteSourceBarrier),
    Reset {
        reason: RemoteStreamResetReason,
        cursor: Option<OriginCursor>,
    },
    Unavailable(PublicError),
}

/// 来源清单/连接事实的变化通知（`sync.remote_catalog_changed`/`sync.remote_source_state` 的来源，
/// `design.md` D6）。
///
/// 它是**连接事实**的推送，不是 owned 业务事件：调用方只能把它投影成 `sync.remote_*` 通知，不得写入
/// 本地事件流或磁盘。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteSourceNotice {
    CatalogChanged {
        owner_node_id: NodeId,
        revision: RemoteCatalogRevision,
    },
    State {
        owner_node_id: NodeId,
        state: RemoteSourceState,
    },
}

/// 一条来源事件流的接收端（`RemoteEventStream::subscribe` 的返回值）。
///
/// `next` 返回 `None` 表示本次订阅已结束（Owner 断线、撤销、退订或调用方 drop）；实现必须在 drop 时
/// 取消底层观察者，不留 detached 订阅。
#[async_trait]
pub trait RemoteStreamReceiver: Send {
    async fn next(&mut self) -> Option<RemoteStreamItem>;
}

/// 来源通知的接收端（`RemoteEventStream::notices` 的返回值）；`None` = 运行期结束。
#[async_trait]
pub trait RemoteNoticeReceiver: Send {
    async fn next(&mut self) -> Option<RemoteSourceNotice>;
}

/// 运行期远程来源事件流端口（`design.md` D3/D4/D6；spec `sync-event-delivery`）。
///
/// **与 [`RemoteSourceReads`] 的分工**：读取给出一次权威页（含读取边界与覆盖），本端口给出**有序的
/// 来源事件流**（重放 → 屏障 → 实时）。两者都必须先经 [`RemoteRuntimeDirectory`] 校验来源身份与代际。
///
/// **生命期**：只服务当前已认证连接。连接断开/撤销/代际失效后实现必须让 [`RemoteStreamReceiver::next`]
/// 以 `Unavailable`/`None` 结束；缓冲只在内存中，不落盘。
///
/// **有界**：实现只在协议既有预算内维护每会话缓冲；慢消费者不得被静默丢事件——缓冲溢出必须投影成
/// [`RemoteStreamItem::Unavailable`]（backpressure），由调用方取消该订阅并要求客户端重建。
#[async_trait]
pub trait RemoteEventStream: Send + Sync {
    /// 订阅一个来源会话：`after` 为 `None` 时从 Owner 当前元数据屏障开始（首次重建），非空时先重放
    /// `after` 之后的事件、再发 `CaughtUp` 屏障。
    async fn subscribe(
        &self,
        session: &RemoteSessionRef,
        generation: RemoteGeneration,
        after: Option<OriginCursor>,
    ) -> Result<Box<dyn RemoteStreamReceiver>, PortError>;

    /// 取消本连接上该来源会话的订阅（客户端退订/订阅结束时调用）。代际不匹配是迟到调用，忽略。
    async fn unsubscribe(&self, session: &RemoteSessionRef, generation: RemoteGeneration);

    /// 当前连接全部来源的清单/状态变化通知；接收端关闭 = 运行期结束。
    async fn notices(&self) -> Result<Box<dyn RemoteNoticeReceiver>, PortError>;
}

/// 远程命令查询的显式来源（`design.md` D5；spec `sync-command-handling`）。
///
/// `session_id = None` 只用于尚未建成会话的 `session.create`；其余操作必须给出原来源会话。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RemoteCommandTarget {
    pub owner_node_id: NodeId,
    pub session_id: Option<SessionId>,
}

/// Owner 侧的命令记录（只承载 `command.status` 需要的字段；不进入本地请求表）。
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteCommandRecord {
    pub target: RemoteCommandTarget,
    pub request: RequestId,
    pub status: CommandStatus,
    pub accepted_at: Option<Timestamp>,
    pub terminal_at: Option<Timestamp>,
    /// Owner 为这条命令落盘的**终态事件标识**（`terminalEventId`）；`accepted` 恒为 `None`。
    ///
    /// 它是 Owner 的事件标识，不是 Access 本地序号：Access 只把它原样投影成 wire 的
    /// `terminalEventId`（`SYNC_PROTOCOL.md` §11.2），**不**据此伪造本地事件或落盘任何索引进而
    /// 声称「可本地恢复」。
    pub terminal_event_id: Option<EventId>,
    pub result: Option<CommandResult>,
    pub error: Option<PublicError>,
}

/// 远程命令查询的结果分类（`design.md` D5）。
///
/// Owner 不可达由端口返回 [`PortError::Unavailable`]；本枚举把「原请求不存在」与「执行结果不确定」
/// 与已接受/已终结区分开，避免调用方把不确定误报成成功或失败。
#[derive(Debug, Clone, PartialEq)]
pub enum RemoteCommandLookup {
    /// Owner 已接受、仍在执行。
    Accepted(RemoteCommandRecord),
    /// Owner 已给出终态（含 `completed`/`failed`/`rejected`/`uncertain`）。
    Terminal(RemoteCommandRecord),
    /// Owner 证明没有该 `(ownerNodeId, accessNodeId, requestId)` 记录。
    NotFound,
    /// Owner 无法确定本次执行是否发生（不得据此判定失败或成功）。
    Uncertain(RemoteCommandRecord),
}

/// Owner 远程命令状态查询端口（`design.md` D5；spec `sync-command-handling`）。
///
/// Access **不**保存远程命令状态：本端口按原 `(remoteTarget, requestId)` 直接向 Owner 查询，同一
/// `requestId` 的幂等与参数冲突判定完全由 Owner 负责。实现必须在 Owner 不可达时返回
/// [`PortError::Unavailable`]，不得用本地记录或空结果代替。
///
/// **生命期**：只服务当前已认证连接；断开/撤销后未完成查询以 `Unavailable` 结束，不留下可跨重启
/// 恢复的请求或结果引用。
#[async_trait]
pub trait RemoteCommandStatus: Send + Sync {
    async fn query(
        &self,
        target: &RemoteCommandTarget,
        request: &RequestId,
    ) -> Result<RemoteCommandLookup, PortError>;
}

/// Access 侧向 Owner 提交 `session.title.set` 的窄端口（`design.md` D2/D3；spec
/// `session-custom-title`「imported 会话修改」）。
///
/// **同一 requestId**：调用方给出、实现原样使用——不换号、不自动重发；结果丢失只能由客户端用原
/// `requestId` 走 `command.status`（[`RemoteCommandStatus`]）回源。
///
/// **不落盘**：实现只在已认证连接期持有运行期事实，标题、命令状态、收据与请求引用一律不写数据库、
/// 临时文件或日志；Access 不是远程标题的第二权威。
///
/// **代际**：调用方传入目录条目的 `generation`；实现必须在连接断开/撤销/代际变化后丢弃迟到的
/// **成功**结果，不得把跨代结果当作本次成功。若帧已写出但结果无法确认（断线、超时、提交后代际
/// 变化），实现返回 `status = Uncertain` 的记录而不是错误——它不声称成功或失败，调用方据此保留
/// 原 `requestId` 供客户端回源；只有确定没有发出副作用时才返回
/// [`PortError::Unavailable`]（`OwnerOffline`/`RemoteUnavailable`）。
///
/// 返回 Owner 的当前记录：Owner 已给出终态时是终态记录（`terminal_event_id` 可缺），否则是
/// `accepted` 记录，结果未知时是 `Uncertain` 记录（`terminal_at`/`terminal_event_id` 均为 `None`）。
#[async_trait]
pub trait RemoteTitleWrite: Send + Sync {
    async fn set_title(
        &self,
        session: &RemoteSessionRef,
        generation: RemoteGeneration,
        request: &RequestId,
        title: &SessionTitle,
    ) -> Result<RemoteCommandRecord, PortError>;
}

// ---------------------------------------------------------------------------------------------
// §5.10 本机生命周期端口（`session-activation-lifecycle`）
// ---------------------------------------------------------------------------------------------

/// 激活请求（`CORE_PORTS_AND_STORAGE.md` §5.10）。
///
/// 四个字段都是稳定输入：同一 `(actor, request_id)` 的重试必须逐字相同；指纹不一致即
/// `command.idempotency_conflict`。激活路径**不得**自造 `request_id` 或指纹（否则一次重试会得到第二个
/// 幂等键，§6 第 6/20 条）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationRequest {
    pub actor: Actor,
    pub request_id: RequestId,
    pub request_fingerprint: Digest,
    pub session: SessionId,
}

/// 激活受理三态，互斥。**受理不是建立成功**：`Prepared`/`InFlight` 之后仍要
/// [`SessionActivation::settle`] 收口。
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // 契约形状：`Settled` 承载完整的 `CommandRecord`
pub enum ActivationAdmission {
    /// 本次调用获得建立权：容量名额已预占，资源代已分配；仍须 `settle` 收口。
    Prepared,
    /// 同会话已有激活在建立中（或回收尚未确认）：**不**重复启动后端，等同一份终态。
    InFlight,
    /// 已有**持久终态**（幂等重试，或先前调用已收口）：原样返回记录，不再派发、不新建资源代。
    Settled(CommandRecord),
}

/// 建立结果三态。只有 `Ready` 让资源代进入 `ready`。`Failed` 表示适配器**可证明**没有建立资源：
/// 实现释放容量预占并把 `LocalActivity` 收敛为 `Idle`；`Uncertain` 表示外部资源可能存在：保留容量
/// 占位与 `Active` 事实，直到经 [`SessionRelease::release_session`] 确认释放。说不清的一律用
/// `Uncertain`（`docs/CORE_PORTS_AND_STORAGE.md` §5.10）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationOutcome {
    Ready,
    Failed { error: PublicError },
    Uncertain { error: PublicError },
}

/// 查看声明/续租的判定：`Accepted` 必须给出未来时刻，`Stale` 必须没有租期（`SYNC_PROTOCOL.md` §11.8）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewStatus {
    Accepted,
    Stale,
}

/// 一次查看声明/续租的判定结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewDecision {
    pub status: ViewStatus,
    /// 只有 `Accepted` 才为 `Some`，且必须晚于调用方的 `now`。
    pub lease_expires_at: Option<Timestamp>,
}

impl ViewDecision {
    pub fn accepted(lease_expires_at: Timestamp) -> Self {
        Self {
            status: ViewStatus::Accepted,
            lease_expires_at: Some(lease_expires_at),
        }
    }

    pub fn stale() -> Self {
        Self {
            status: ViewStatus::Stale,
            lease_expires_at: None,
        }
    }

    pub fn is_accepted(&self) -> bool {
        self.status == ViewStatus::Accepted
    }
}

/// 容量准入：只有 `Reserved` 占住名额；`CapacityExceeded` 映射 `session.capacity_exceeded`（retryable）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Reserved,
    CapacityExceeded,
}

/// 安全回收候选。「没有安全候选」是 `None`，不是空占位、哨兵会话或「随便挑一个」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReclaimCandidate {
    pub session: SessionId,
    /// 该会话最后一次仍受保护（查看租约/未结束任务/有效交互）的时刻；回收按此值最久优先。
    pub last_protected_at: Timestamp,
}

/// 逐会话释放确认：只有 `Released` 交还名额；`Failed`/`Uncertain` 继续占位并保持 `Active`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseOutcome {
    Released,
    Failed,
    Uncertain,
}

/// 激活/恢复的合并入口：同一会话的并发激活共享一次建立过程。
#[async_trait]
pub trait SessionActivation: Send + Sync {
    /// 先查稳定 requestId 的持久记录，再判 ready 复用或启动恢复；`uncertain` 不自动换号重派。
    async fn activate(&self, request: ActivationRequest) -> Result<ActivationAdmission, PortError>;
    /// 完成一次激活：成功才把资源代置为 ready；`Failed`（可证明未建立）释放容量预占并收敛为 `Idle`，
    /// `Uncertain` 保留占位与 `Active` 事实。
    async fn settle(
        &self,
        session: SessionId,
        generation: ResourceGeneration,
        outcome: ActivationOutcome,
    ) -> Result<(), PortError>;
}

/// 查看声明、续租与释放。每连接最多一个当前查看者，租约不持久化。
#[async_trait]
pub trait ViewRegistry: Send + Sync {
    async fn declare(
        &self,
        connection: ConnectionId,
        session: Option<SessionId>,
        view_version: u64,
        now: Timestamp,
    ) -> Result<ViewDecision, PortError>;
    async fn renew(
        &self,
        connection: ConnectionId,
        view_version: u64,
        now: Timestamp,
    ) -> Result<ViewDecision, PortError>;
    async fn release(&self, connection: ConnectionId, now: Timestamp) -> Result<(), PortError>;
    async fn viewers(
        &self,
        session: SessionId,
        now: Timestamp,
    ) -> Result<Vec<ConnectionId>, PortError>;
}

/// 容量准入与安全候选选择：创建与恢复共用，预占也计入上限。
#[async_trait]
pub trait CapacityAdmission: Send + Sync {
    async fn reserve(&self, session: SessionId, now: Timestamp) -> Result<Admission, PortError>;
    /// 只释放**尚未建立**资源的预占（失败回滚）。已 ready/回收中的资源必须经
    /// [`SessionRelease::release_session`] 确认真实释放后才能交还名额；`Uncertain` 的激活同样必须先经
    /// 释放确认（它可能已在外部建立了资源），本方法对它返回 `InvalidRequest` 而不是放掉占位。
    async fn release(
        &self,
        session: SessionId,
        generation: ResourceGeneration,
        now: Timestamp,
    ) -> Result<(), PortError>;
    /// 只在「未受保护」的候选中选最久未使用者；同一时刻以稳定会话标识打破并列。
    async fn reclaim_candidate(
        &self,
        now: Timestamp,
    ) -> Result<Option<ReclaimCandidate>, PortError>;
}

/// 逐会话释放确认：只有适配器确认真实释放后才能交还名额。由 `agent-host` 实现（真实 ACP close），
/// core 只消费结果。
#[async_trait]
pub trait SessionRelease: Send + Sync {
    async fn release_session(
        &self,
        session: SessionId,
        generation: ResourceGeneration,
        now: Timestamp,
    ) -> Result<ReleaseOutcome, PortError>;
}

/// 当前生命周期的只读查询：owned 当前查询与快照必须读同一代目录。
#[async_trait]
pub trait LifecycleQuery: Send + Sync {
    async fn local_activity(&self, session: SessionId) -> Result<LocalActivity, PortError>;
    async fn snapshot(&self) -> Result<Vec<SessionSummary>, PortError>;
}

// ---------------------------------------------------------------------------------------------
// §5.11 首条提交与准备附件暂存
// ---------------------------------------------------------------------------------------------

/// 首次提交的原子持久入口（`design.md` D7/D8）。
///
/// `commit_first_submission` 在**同一事务**里写正式会话、完整首条输入、稳定消息/请求身份、命令受理、
/// turn、首条关联与附件正式引用，阶段固定为 `saved_not_dispatched`。任一步失败全回滚，不派发、不留
/// 部分提交。`begin_dispatch` 是唯一把阶段推进到 `dispatch_started` 的入口，且必须在调用 Agent
/// **之前**提交。
#[async_trait]
pub trait FirstSubmission: Send + Sync {
    /// 首次受理。`write.session` 必须是由准备凭据证明的 Owner 分配 ID；重复调用返回
    /// [`FirstSubmissionAdmission::AlreadyCommitted`]，不重建准备、不追加首条、不重传附件。
    async fn commit_first_submission(
        &self,
        write: FirstSubmissionWrite,
    ) -> Result<FirstSubmissionAdmission, PortError>;

    /// 读阶段与关联（`command.status` 投影、`restore` 的 committed、显式继续的准入都只读这里）。
    async fn first_submission(
        &self,
        session: SessionId,
    ) -> Result<Option<FirstSubmissionRecord>, PortError>;

    /// 持久首条用户消息（`owned_message` 的 user 通道）：**不需要** Agent 回显即可完整读取。
    async fn first_user_message(
        &self,
        session: SessionId,
    ) -> Result<Option<FirstUserMessage>, PortError>;

    /// 派发围栏：仅在阶段可证明未派发、且继续引用合法时 CAS 到 `dispatch_started` 并固定 Agent
    /// 会话标识/资源代/本次 requestId。并发继续最多一个 `Granted`。
    async fn begin_dispatch(&self, fence: DispatchFence) -> Result<DispatchAdmission, PortError>;

    /// 写本次尝试的终态。`dispatch_started` 之后无法确认结果一律 `uncertain`，绝不自动重派；
    /// 可证明未派发时的 `failed` 保留正文并等待显式继续。
    async fn settle_first_submission(
        &self,
        settlement: FirstSubmissionSettlement,
    ) -> Result<FirstSubmissionRecord, PortError>;

    /// 启动收敛：只收敛到可证明的事实，不自动派发、不自动新建 Agent 身份。
    async fn converge_first_submission(
        &self,
        session: SessionId,
        at: Timestamp,
    ) -> Result<FirstSubmissionConvergence, PortError>;
}

/// 准备附件暂存（`design.md` D6.4）。
///
/// 只维护运行期传输/清单元数据与 Owner 私有内容区中的字节，**不写**带 `owned_session` 外键的行；
/// 分块接收、流式摘要、封存后不可变、24 小时/启动回收与配额复用既有预算。`claim` 只核验已封存事实并
/// 原子认领元数据，不再次上传、不复制或全量重哈希。
#[async_trait]
pub trait PreparationAttachmentStaging: Send + Sync {
    /// 开始/重绑一次运行期传输：同 transferId 同声明返回原状态（保留偏移），不同声明冲突。
    async fn begin(&self, begin: StagedTransferBegin) -> Result<StagedTransferState, PortError>;

    /// 按期望偏移追加一块（重复同内容返回 `Duplicate`，不同内容返回 `ChunkConflict`）。
    async fn append(&self, chunk: StagedChunk) -> Result<StagedChunkOutcome, PortError>;

    /// 校验长度与流式 SHA-256 后封存；封存后字节不可变。
    async fn seal(&self, seal: StagedSeal) -> Result<StagedSealOutcome, PortError>;

    /// 运行期传输状态。
    async fn status(
        &self,
        transfer: AttachmentId,
    ) -> Result<Option<StagedTransferState>, PortError>;

    /// 取消一次仍在接收/已封存但未认领的传输（只清运行期元数据与未认领字节）。
    async fn abort(
        &self,
        transfer: AttachmentId,
        reason: StagedAbortReason,
    ) -> Result<(), PortError>;

    /// 开始登记一份准备清单。
    async fn begin_manifest(
        &self,
        begin: StagedManifestBegin,
    ) -> Result<StagedManifestState, PortError>;

    /// 追加一页清单（顺序登记；同页重放幂等）。
    async fn append_manifest_page(
        &self,
        page: StagedManifestPage,
    ) -> Result<StagedManifestState, PortError>;

    /// 封存清单：所有项已封存、同一绑定/主体、数量一致。
    async fn seal_manifest(
        &self,
        seal: StagedManifestSeal,
    ) -> Result<StagedManifestState, PortError>;

    /// 首次提交认领：返回已封存事实并清除运行期清单元数据。
    async fn claim(&self, claim: StagedClaim) -> Result<Vec<ClaimedAttachment>, PortError>;

    /// 24 小时/启动清理：回收无引用的孤儿字节；活动传输与冻结待认领内容不得被误删。
    async fn release_orphans(&self, before: Timestamp) -> Result<usize, PortError>;
}
