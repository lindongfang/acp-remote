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
    Actor, AgentDescriptor, AgentId, AgentProfile, AgentRef, AttachmentGeneration, AttachmentId,
    AuditAction, AuditOutcome, AuditRecord, CapabilitySet, CommandKind, CommandRecord,
    CommandStatus, CommandTerminalRecord, CommittedDelivery, CommittedEvent, ConfigOption,
    ConfigOptionId, ConfigValue, CreateSessionRequest, DeviceId, DeviceRecord, Digest,
    EndpointEvent, EntityRef, EventId, EventPayload, EventType, ExportId, ExportRecord,
    GlobalCursor, ImportId, ImportRecord, InteractionId, InteractionResolution, LocalCursor,
    MessageId, ModeId, ModeRef, ModeState, NodeId, NodeKind, NodeRecord, OriginCursor, OriginEpoch,
    OriginEventRef, PairingClaim, PairingId, PairingPeer, PairingRecord, PairingSettlement,
    PeerIdentity, PeerPublicKey, PendingEvent, PendingInteraction, PortError, PromptRequest,
    ProviderRef, PublicError, RemoteSessionRef, RequestId, SecretValue, SeedState, Sequence,
    ServerEpoch, SessionId, SessionReference, SessionSnapshot, SessionState, SessionSummary,
    Timestamp, Turn, TurnId, TurnState, Version, WorkspaceAlias, WorkspaceRecord,
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
}

#[async_trait]
pub trait SessionEndpoint: Send + Sync {
    fn reference(&self) -> SessionReference;

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
// §5.2 持久化
// ---------------------------------------------------------------------------------------------

/// 一次提交里对会话行的修改。`OwnedCommit::state` 为 `None` 时只追加事件与 turn 变更。
#[derive(Debug, Clone, PartialEq)]
pub enum StateChange {
    /// 新建 owned 会话：`OwnedCommit.session` 必须为 `None`、`expected_version` 必须为 `None`、
    /// `origin_epoch` 必须为 `Some`；`SessionId` 由存储层在创建事务内分配并回传。
    Create(NewSession),
    /// 已有会话的状态修改：`version` 在同一事务内 +1。
    Update(SessionUpdate),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSession {
    pub title: Option<String>,
    pub agent: AgentRef,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionUpdate {
    /// `None` = 不改会话状态（例如只解析一个交互）。
    pub state: Option<SessionState>,
    pub mode: ModeChange,
    /// `Some` 时会话进入 `Closed` 并写 `closed_at`。
    pub closed_at: Option<Timestamp>,
    /// 一次交互解析：由存储层用**条件更新**实现 first-writer-wins（§5.2、§6.7）。
    ///
    /// 受影响行数为 0 时存储层在同一事务内回读：行存在且 `state <> 'pending'` →
    /// `PortError::Conflict(AlreadyResolved)`；无行 → `PortError::NotFound`。
    pub interaction: Option<InteractionResolved>,
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
    /// 只分配 `global_sequence`。
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryQuery {
    pub session: SessionId,
    pub include: HistoryInclude,
    /// 分页起点：只返回该 cursor 之后的事件；`None` = 从头。cursor 的合法性由用例层判定。
    pub after: Option<GlobalCursor>,
    pub limit: ReplayLimit,
}

/// 历史页面。`config`/`capabilities` 是**用例层**按 `include` 合并的活体数据（来自
/// `SessionEndpoint`/`AgentCatalog`）；存储层实现必须让它们保持空值——持久层不保存活体能力。
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryPage {
    pub session: SessionSummary,
    pub events: Vec<CommittedEvent>,
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

    async fn list(&self, query: SessionQuery) -> Result<Vec<SessionSummary>, PortError>;

    async fn head(&self) -> Result<GlobalCursor, PortError>;

    /// 一致性读视图：`sync.snapshot_*` 必须在本方法返回的视图内完成（barrier 依据）。
    async fn read_view(&self) -> Result<Box<dyn ReadView>, PortError>;

    async fn find_request(
        &self,
        request: &RequestId,
        actor: &Actor,
    ) -> Result<Option<CommandRecord>, PortError>;

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

    async fn read_session(&self, query: HistoryQuery) -> Result<HistoryPage, PortError>;

    /// 取回一条 owned 事件的持久化正文——重放、`sync.snapshot_*` 与 `PendingInteraction.options`
    /// 补全的唯一入口（复用 [`EventPayload`]/`AcpRaw`，不新增类型）。
    ///
    /// `view` 必须是库里 `payload_json` 的**原样文本**（不得经通用 JSON 解析后重新序列化）；`acp`
    /// 可用时 `raw_json` 必须字节保真，不可用时按行里的 `acp_raw_unavailable_reason` 还原为
    /// `AcpRaw::Unavailable`。事件不存在返回 `None`。
    async fn event_payload(&self, event: &EventId) -> Result<Option<EventPayload>, PortError>;
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
    /// 只返回这些 import 名下的会话；`None` = 不按 import 过滤。
    pub imports: Option<Vec<ImportId>>,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DropReport {
    pub delivery_index_removed: u64,
    pub command_refs_removed: u64,
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

    /// 删除该 import 名下的交付索引与命令引用；**不删除审计行**（`SECURITY_DESIGN.md` §11.5）。
    async fn drop_import(&self, import: &ImportId) -> Result<DropReport, PortError>;

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
// §5.3 信任、Export、审计与附件
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

/// 落定配对（§11.6）：单事务完成状态/过期检查、固定 peer 与最终 scopes/grants 校验、创建信任记录、
/// 更新配对状态并写审计。peer 公钥从配对的对端行读回，不由调用方重复提供。
#[derive(Debug, Clone, PartialEq)]
pub struct PairingSettlementWrite {
    pub pairing: PairingId,
    pub settlement: PairingSettlement,
    pub context: WriteContext,
}

/// 过期扫描（§11.6）：只终结未确认且已过期的配对，返回终结行数。
#[derive(Debug, Clone, PartialEq)]
pub struct ExpiryWrite {
    pub context: WriteContext,
}

/// 写入/更新一个 Export（§11.6）。
#[derive(Debug, Clone, PartialEq)]
pub struct ExportWrite {
    pub record: ExportRecord,
    pub context: WriteContext,
}

/// 撤销一个 Export（§11.6）：先提交再发送 `export.revoked`，发送失败不撤销数据库决定。
#[derive(Debug, Clone, PartialEq)]
pub struct ExportRevocation {
    pub export: ExportId,
    pub context: WriteContext,
}

/// 添加一个 Import（§11.6）：管理行 + 全部关联行一次提交。
///
/// `exports` 是本次写入的关联集合，**必须**等于 `record.export_ids()`（两处不得分歧，否则存储层返回
/// `InvalidRequest`）；同一 `(owner_node_id, export_id)` 只能属于一个 Import，冲突返回
/// `PortError::Conflict(DuplicateOwnership)`。
#[derive(Debug, Clone, PartialEq)]
pub struct ImportWrite {
    pub record: ImportRecord,
    pub exports: Vec<ExportId>,
    pub context: WriteContext,
}

/// 完整移除一个 Import（§11.6）：同一事务删除管理行、关联行、`imported_session` 及其级联
/// （交付索引、命令引用），**审计保留**。提交后由组合根停止连接/重连并清空内存正文。
#[derive(Debug, Clone, PartialEq)]
pub struct ImportRemoval {
    pub import: ImportId,
    pub context: WriteContext,
}

/// 写入一个 Agent profile（§11.6）。`put_profile` 是唯一写入默认 profile 的入口：至多一个
/// `default = true`，切换默认必须是一次调用的原子写集。
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileWrite {
    pub profile: AgentProfile,
    pub context: WriteContext,
}

/// 写入一个 workspace 记录（§11.6）。
#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceWrite {
    pub record: WorkspaceRecord,
    pub context: WriteContext,
}

/// 写入一个 Provider 引用（§11.6）：只记字段名、keystore 引用与版本。
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderRefWrite {
    pub reference: ProviderRef,
    pub context: WriteContext,
}

/// 种子导入（§11.6）：`profiles` 与「已初始化」标记在同一事务里提交（空列表也写标记）。
#[derive(Debug, Clone, PartialEq)]
pub struct SeedWrite {
    pub profiles: Vec<AgentProfile>,
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

    /// 已认领的对端行（确认事务从它读回公钥，§11.5）。
    async fn pairing_peer(&self, id: &PairingId) -> Result<Option<PairingPeer>, PortError>;

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
}

/// Export/Import 存储（§5.3/§11.6）：读取面不变，写入面全部走写集。
#[async_trait]
pub trait ExportStore: Send + Sync {
    async fn export(&self, id: &ExportId) -> Result<Option<ExportRecord>, PortError>;

    async fn exports(&self) -> Result<Vec<ExportRecord>, PortError>;

    async fn import(&self, id: &ImportId) -> Result<Option<ImportRecord>, PortError>;

    async fn imports(&self) -> Result<Vec<ImportRecord>, PortError>;

    async fn put_export(&self, write: ExportWrite) -> Result<(), PortError>;

    async fn revoke_export(&self, write: ExportRevocation) -> Result<(), PortError>;

    async fn add_import(&self, write: ImportWrite) -> Result<(), PortError>;

    /// 完整移除（§11.6）：管理行 + 关联行 + 交付索引 + 命令引用；审计保留。
    ///
    /// 与 `RemoteDeliveryStore::drop_import`（连接级清空交付索引）不是同一件事，两者不得串联充当完整删除。
    async fn remove_import(&self, write: ImportRemoval) -> Result<(), PortError>;
}

/// 本地配置存储（§11.6）：profile、workspace、Provider 引用与首次初始化标记。
#[async_trait]
pub trait LocalConfigStore: Send + Sync {
    async fn profiles(&self) -> Result<Vec<AgentProfile>, PortError>;

    async fn profile(&self, id: &AgentId) -> Result<Option<AgentProfile>, PortError>;

    async fn put_profile(&self, write: ProfileWrite) -> Result<(), PortError>;

    async fn workspaces(&self) -> Result<Vec<WorkspaceRecord>, PortError>;

    async fn workspace(&self, alias: &WorkspaceAlias)
    -> Result<Option<WorkspaceRecord>, PortError>;

    async fn put_workspace(&self, write: WorkspaceWrite) -> Result<(), PortError>;

    async fn provider_refs(&self) -> Result<Vec<ProviderRef>, PortError>;

    async fn put_provider_ref(&self, write: ProviderRefWrite) -> Result<(), PortError>;

    /// 首次初始化标记：`seeded = false` 时启动流程才能导入种子。
    async fn seed_state(&self) -> Result<SeedState, PortError>;

    /// 种子导入与「已初始化」标记同一事务提交（空列表也写标记）。
    async fn mark_seeded(&self, write: SeedWrite) -> Result<(), PortError>;
}

/// 凭据解析（§11.6）：把 profile 的凭据绑定解析成子进程环境变量。
///
/// 由组合根用平台 keystore 实现并注入 `agent-host`（`agent-host` 不依赖 `identity-auth`）。
#[async_trait]
pub trait CredentialResolver: Send + Sync {
    /// 解析启动子进程所需的全部环境变量。只允许解析 profile 的 `env` 绑定与 `env_allowlist` 的交集；
    /// 未绑定、未列入白名单或引用失效（keystore 不可用 / 字段不存在）→
    /// `Unavailable(KeystoreUnavailable)`，**失败关闭**：不得静默跳过该变量后继续启动。
    async fn resolve_env(
        &self,
        profile: &AgentProfile,
    ) -> Result<Vec<(String, SecretValue)>, PortError>;
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

/// 发布已提交的投递。`publish` 不返回结果，且不得回滚已提交事务（§5.4）。
pub trait EventPublisher: Send + Sync {
    fn publish(&self, delivery: CommittedDelivery);
}

pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

/// id 分配（§3.1）：`TurnId`/`InteractionId`/`PairingId`/`OriginEpoch`/`RequestId` 由本 trait 分配；
/// `SessionId`/`EventId` 由存储层在提交事务内分配（`SessionStore::commit`），`AttachmentId` 由
/// `AttachmentStore::put` 分配并返回——它们**不**在这里，以免出现两个来源。
pub trait IdGenerator: Send + Sync {
    fn turn_id(&self) -> TurnId;
    fn interaction_id(&self) -> InteractionId;
    fn pairing_id(&self) -> PairingId;
    /// 助手消息标识：由生产 delta 的适配器在该消息第一条 delta 提交前分配（§6 第 14 条）。
    fn message_id(&self) -> MessageId;
    fn origin_epoch(&self) -> OriginEpoch;
    fn request_id(&self) -> RequestId;
}
