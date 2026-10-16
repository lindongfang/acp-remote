//! 用例面：`docs/CORE_PORTS_AND_STORAGE.md` §4。
//!
//! 每个入口的第一个参数都是 `actor: &Actor`：**授权只在 core 判定**（§4/§6.5），适配器不得传入
//! "已授权"标志。全部 mutation 只同步接受（返回 `CommandReceipt`），终态一律经事件与
//! `command.status` 表达（`SYNC_PROTOCOL.md` §11.2）。
//!
//! 查询类命令（`session.list`/`session.read`/`command.status`/`session.mode.list`/
//! `session.config.list`）的结果在本层以 **core 类型**返回，由适配器投影到 wire——core 不做 JSON
//! 序列化（无 serde，`MODULE_ARCHITECTURE.md` §8 的边界）。因此适配器对查询命令调用这里对应的
//! 查询入口，而不是从 `submit_command` 的返回值里取结果。
//!
//! 设备/配对/Export/Import 管理族（§4 的 `DeviceManagement`/`ExportManagement`）是端口之上的薄层：
//! 本地访问控制由 `LOCAL_ADMIN_PROTOCOL.md` §2.2 的 OS 用户边界保证，因此这里要求 `Actor::LocalCli`，
//! 其余 actor 一律 `authorization.scope_denied`。配对通道（claim/status/consume，design D12）是唯一
//! 的例外：`claim_pairing`/`pairing` 另接受绑定该配对的 `Actor::PairingClaimant`，`consume_pairing`
//! 只接受与该配对已批准对端一致的 `Actor::Node`/`Actor::Device`。

use std::sync::Arc;

use crate::broker::{Broker, Denied, command_name};
use crate::model::{
    Actor, AgentDescriptor, AgentId, AgentProfile, AgentRef, AttachmentGeneration, AttachmentId,
    AuditAction, AuditOutcome, AuditRecord, CapabilitySet, ClientCommand, CommandKind,
    CommandReceipt, CommandRecord, CommandResult, CommandStatus, ConfigOption, ConfigOptionId,
    ConfigValue, ConflictKind, CreateSessionRequest, DeviceId, DeviceRecord, Digest,
    ElicitationAction, ElicitationValues, EntityRef, EventId, EventPayload, EventType, ExportId,
    ExportRecord, GlobalCursor, ImportId, ImportRecord, InteractionId, InteractionResolution,
    LocalCursor, ModeId, ModeState, NodeId, NodeKind, NodeRecord, NodeState, OriginCursor,
    OriginEpoch, OwnedSessionRef, PairingClaim, PairingId, PairingRecord, PairingSettlement,
    PairingState, PairingTarget, PeerIdentity, PeerPublicKey, PendingInteraction, PortError,
    ProviderRef, PublicError, RemoteSessionRef, RequestId, Resolution, ResolvedWorkspace,
    ResourceOrigin, SeedState, Sequence, SessionId, SessionReference, SessionSummary, Timestamp,
    UnavailableKind, Version, WorkspaceAlias, WorkspaceCatalogEntry, WorkspaceRecord, WorkspaceRef,
    WorkspaceSource,
};
use crate::ports::{
    AgentCatalog, AttachmentRef, AttachmentStore, AuditQuery, AuditStore, Clock,
    DeliveryIndexEntry, DeviceRevocation, DeviceWrite, DirectoryBrowser, DirectoryListing,
    ExpiryWrite, ExportRevocation, ExportStore, ExportWrite, HistoryPage, HistoryQuery,
    IdGenerator, ImportRemoval, ImportWrite, ImportedSessionQuery, ImportedSessionRecord,
    LocalConfigStore, NodeConnectedWrite, NodeRevocation, NodeWrite, PairingClaimOutcome,
    PairingClaimWrite, PairingConsumption, PairingSettlementWrite, PairingWrite, PendingAudit,
    ProfileWrite, ProviderRefWrite, PruneReport, RemoteDeliveryStore, ReplayBatch, ReplayLimit,
    RetentionPolicy, RevokeReason, SeedWrite, SessionCreateAdmission, SessionQuery, SessionStore,
    StoreHealth, SyncReplaySlice, TrustRecordRef, TrustStore, WorkspaceWrite, WriteContext,
};

/// `session.mode.list` 的结果：端口返回的 `ModeState` + 会话当前 `Version`（§6 第 17 条）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeListing {
    pub state: ModeState,
    pub version: Version,
}

/// 配对通道的只读视图（claim/status HTTP 端点；design D12 的 seam 补全）。
///
/// 三个字段都是端点判定与回包所需的**持久事实**：记录（状态/绑定/过期/登记集合）、已固定的对端行
/// （幂等重试的比对输入）与已批准后的对端节点行（`grant.*` 的唯一来源；`PairingRecord` 不带
/// `granted_*`）。视图本身不包含任何秘密材料：pairing secret 只在状态机内存里。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingChannelView {
    /// 配对记录。
    pub record: PairingRecord,
    /// 已固定的对端行；claim 之前为 `None`。
    pub peer: Option<crate::model::PairingPeer>,
    /// 已批准后该对端的节点角色行（`owned_node` 的 `access` 行）；未批准、非节点对端或行缺失时为 `None`。
    pub node: Option<NodeRecord>,
}

/// Node Link 握手准入的只读视图（`design.md` D3；`IDENTITY_AND_AUTH_CONTRACT.md` §5.1 的 `PeerTrust` 与
/// `ChallengeRequest` 所需的持久事实）。
///
/// 四个字段都是握手本次调用需要且只需要的持久事实：该对端的 `access` 信任行（凭据状态与 grant）、
/// 已绑定的验签公钥、最近一次配对（登记方宣告的 `host_binding` 与是否待消费）、本机全局水位
/// （`serverEpoch` 的来源）。不含任何秘密材料：pairing secret 只在状态机内存。
///
/// `catalog_revision` 与 [`NodeLinkCatalogView::revision`] **同源**（都是 `AuditStore::watermark()`）：
/// `node.challenge`/`node.ready` 与 `catalog.snapshot` 必须报同一个目录修订号（D4/G5 的口径结论）。
#[derive(Debug, Clone, PartialEq)]
pub struct NodeLinkHandshakeView {
    /// 该对端的 `access` 角色行；未配对/未批准时为 `None`（未知节点照常签发挑战）。
    pub node: Option<NodeRecord>,
    /// 已绑定的身份材料（验签公钥的唯一来源）；未知节点为 `None`。
    pub public_key: Option<PeerPublicKey>,
    /// 该对端最近一次配对；`None` = 该对端从未配对。
    pub pairing: Option<PairingRecord>,
    /// 本机全局水位（`store.head()`）：`serverEpoch` 的来源。
    pub head: GlobalCursor,
    /// 管理写集水位（`AuditStore::watermark()`）：`catalogRevision` 的唯一来源。
    pub catalog_revision: u64,
}

/// Sync 设备握手准入的只读视图（`design.md` D2；`IDENTITY_AND_AUTH_CONTRACT.md` §5.1 的 `PeerTrust` 与
/// `ChallengeRequest` 所需的持久事实）。
///
/// 与 [`NodeLinkHandshakeView`] **同形但少一项**：Sync 设备握手不带 `catalogRevision`（`identity-auth`
/// 的 `hello` 只对 Node Link 分支要求它），因此本视图也不读 `AuditStore::watermark()`。
///
/// 四个字段都是本次握手需要且只需要的持久事实：该设备的信任行（凭据状态的来源，即 `PeerTrust`
/// 的 `credential`；设备没有授权维度，因此没有集合字段）、已绑定的验签公钥（`TrustStore::peer_key`，
/// `verify_proof` 唯一接受的密钥来源）、最近一次配对（登记方宣告的 `host_binding` 与是否待消费）、
/// 本机全局水位（`SessionStore::head()`，同时携带 `server_epoch` 与 `global_sequence`）。视图不包含任何
/// 秘密材料：pairing secret 只在身份状态机内存里。
#[derive(Debug, Clone, PartialEq)]
pub struct SyncDeviceHandshakeView {
    /// 该设备的信任行；未知设备为 `None`（未知设备照常签发挑战，见 §8.2/§8.3：挑战照常签发、
    /// 存在性判定推迟到 proof 阶段）。
    pub device: Option<DeviceRecord>,
    /// 已绑定的身份材料（验签公钥的唯一来源）；未知设备为 `None`。
    pub public_key: Option<PeerPublicKey>,
    /// 该设备最近一次配对；`None` = 该设备从未配对。
    pub pairing: Option<PairingRecord>,
    /// 本机全局水位（`SessionStore::head()`）：`serverEpoch`/`globalSequence` 的唯一来源。
    pub head: GlobalCursor,
}

/// `sync_visible_sessions` 的入参（`SYNC_PROTOCOL.md` §9.6 的 `sessions` 资源；spec
/// `sync-event-delivery`「设备可见的会话清单范围」）。
///
/// 三个字段各承载一件**调用方**的判定，core 不替它决定：两侧的分页/过滤条件按各自存储的既有语义透传，
/// `include_imported` 是 feature `resource.remote-origin.v1` 的**协商结果**（feature 协商在各协议边界，
/// core 不读 feature 表、也不缓存它）。
///
/// `Default` = 两侧都不过滤 + **不**纳入已导入会话：未协商 feature 时一行 imported 摘要都不得出现，
/// 因此保守取值就是 `false`。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncSessionQuery {
    /// 自有会话的过滤/分页（与 [`UseCases::list_sessions`] 同款，原样透传给 `SessionStore::list`）。
    pub owned: SessionQuery,
    /// 已导入会话的过滤/分页（原样透传给 `RemoteDeliveryStore::list_sessions`）。
    pub imported: ImportedSessionQuery,
    /// 是否把已导入会话纳入结果（调用方传入的 `resource.remote-origin.v1` 协商结果）。
    pub include_imported: bool,
}

/// `workspace.browse` 的单层条目上限（`SYNC_PROTOCOL.md` §14 的固定 v1 常量：128）。
///
/// 它是**固定常量**而不是可下调配置键：§14 只允许 `maxMessageBytes`/`maxPromptBytes`/
/// `maxReplayEventsPerBatch` 三项经 `auth.authenticated.limits` 调整，其余上限都写死在协议里。
const WORKSPACE_BROWSE_ENTRY_LIMIT: usize = 128;

/// `workspace.browse` 结果里的一条子目录（`SYNC_PROTOCOL.md` §11.5 的 `WorkspaceBrowseEntry`）。
///
/// 只有三个字段——**没有**文件条目、文件内容、字节大小、时间戳或权限位：浏览是逐层选目录用的，不下发
/// 目录里有什么文件（spec `sync-workspace-browse`「结果不含文件内容与元数据」）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceBrowseEntry {
    /// 该子目录的名称（单层，不含路径片段）。
    pub name: String,
    /// 该子目录的绝对路径（下发形式：`canonicalize` 结果剥掉平台 verbatim 前缀）。
    pub path: String,
    /// 它是否就是本机某个**已登记** workspace 的目录：与已登记目录的规范化路径（同样剥掉平台
    /// verbatim 前缀后）逐字相等时为真，判定始终走规范化路径，与请求原文的形态无关。
    pub registered: bool,
}

/// `workspace.browse` 的结果：请求的父路径回显 + 该层的直接子目录 + 截断标记。
///
/// `path` 是**请求的父路径原文**（`None` = 根列表请求）的**逐字回显**——不是规范化后的另一份取值：
/// `SYNC_PROTOCOL.md` §11.5 要求它与请求里的 `path` 逐字相同，`command.schema.json` 的
/// `workspaceBrowsePath` 也把它定义成「请求字段与结果回显字段」同一个形状。`truncated` 为真表示该层还有
/// 超出 [`WORKSPACE_BROWSE_ENTRY_LIMIT`] 的条目未随本次结果下发——客户端据此告知用户还有更多，而
/// **不能**把「没看到」理解成「不存在」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceBrowse {
    pub path: Option<String>,
    pub entries: Vec<WorkspaceBrowseEntry>,
    pub truncated: bool,
}

/// Node Link 的 catalog 投影源（`design.md` D4；`NODE_LINK_PROTOCOL.md` §12.3）。
///
/// 只承载投影需要的持久事实：管理写集水位（`revision`，取自 `AuditStore::watermark`）、该对端的
/// `access` 信任行（可见性策略与 grant 判定的输入，未配对时为 `None`）与本机全部 Export 记录
/// （含已撤销；撤销由条目自己的 `revoked` 标记表达）。不含任何秘密材料。
///
/// 这是**无 actor** 的窄入口（与 [`NodeLinkHandshakeView`] 同一模式）：授权面收窄到「单个已认证的
/// accessNodeId」，节点绑定由连接层保证；`exportIds` 一类可见性策略是调用方的单点边界。
#[derive(Debug, Clone, PartialEq)]
pub struct NodeLinkCatalogView {
    /// 管理写集水位（`AuditStore::watermark`）：`catalogRevision` 的唯一来源。
    pub revision: u64,
    /// 该对端的 `access` 角色行；未配对/未批准时为 `None`。
    pub node: Option<NodeRecord>,
    /// 本机全部 Export 记录（含已撤销）。
    pub exports: Vec<ExportRecord>,
    /// 本机 Agent 目录（投影 `agents[].name` 的唯一来源；名字不进存储合同）。
    pub agents: Vec<AgentDescriptor>,
}

/// Node Link 会话视图（`NODE_LINK_PROTOCOL.md` §12.4 的快照元数据）。
///
/// 只承载快照确实需要的元数据：会话状态/版本、origin epoch 与 origin head、未决交互。正文（消息、
/// turn、diff、终端输出、ACP raw）**不在这里**，也不得进快照。
#[derive(Debug, Clone, PartialEq)]
pub struct NodeLinkSessionView {
    /// 会话摘要（`state`/`version` 是 `sessionMeta` 的两个字段）。
    pub session: SessionSummary,
    /// 该会话当前 origin cursor（`resource.snapshot_begin.cursor` 的来源）。
    pub head: OriginCursor,
    /// 未决交互的元数据 + 创建它的 origin 事件 id（正文由调用方按 id 取）。
    pub pending_interactions: Vec<NodeLinkPendingInteraction>,
}

/// 快照里的一个未决交互条目（`NODE_LINK_PROTOCOL.md` §12.4 的 `pending_interactions` item）。
///
/// `origin_event` 是**创建该交互的 origin 事件**：wire 的 `payloadDigest` 由调用方用该事件的 payload
/// 按 ACPR-CJ1 规则现算（core 不依赖 `acpr-wire`），因此这里给出事件定位而不是摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeLinkPendingInteraction {
    pub interaction: PendingInteraction,
    pub origin_event: EventId,
}

/// Node Link 的 origin 事件投递单元（`resource.event` 与增量重放共用的形状）。
#[derive(Debug, Clone, PartialEq)]
pub struct NodeLinkEvent {
    pub event_id: EventId,
    pub event_type: EventType,
    pub origin_epoch: OriginEpoch,
    pub origin_sequence: Sequence,
    pub created_at: Timestamp,
    pub payload: EventPayload,
}
/// `node_link_replay` 的结果：本批事件 + 该会话当前的 origin head。
#[derive(Debug, Clone, PartialEq)]
pub struct NodeLinkReplay {
    pub events: Vec<NodeLinkEvent>,
    /// 该会话当前的 origin cursor（`resource.snapshot_end.cursor` / `resource.event` 之后的续读点）。
    pub head: OriginCursor,
}

/// §4 的用例面实现。组合根持有一个 `Arc<UseCases>`。
pub struct UseCases {
    broker: Arc<Broker>,
    store: Arc<dyn SessionStore>,
    deliveries: Arc<dyn RemoteDeliveryStore>,
    exports: Arc<dyn ExportStore>,
    trust: Arc<dyn TrustStore>,
    audit: Arc<dyn AuditStore>,
    config: Arc<dyn LocalConfigStore>,
    attachments: Arc<dyn AttachmentStore>,
    catalog: Arc<dyn AgentCatalog>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    /// 目录浏览端口（`workspace.browse`）。默认是本机文件系统实现（[`SystemDirectories`]）；
    /// 需要替身或另一份实现时用 [`UseCases::with_directory_browser`] 换掉。
    ///
    /// 为什么不在 [`UseCaseDeps`] 里：那份依赖结构的字面量构造点分布在本 crate 之外（组合根与多个
    /// 适配器的测试装配），加一个必填字段会让它们全部编译失败；而目录浏览是本机文件系统事实，
    /// 默认实现不依赖任何外部 crate（§2 的依赖 allow-list 不变），因此默认可用、按需替换。
    browser: Arc<dyn DirectoryBrowser>,
}

/// [`UseCases`] 的依赖。
pub struct UseCaseDeps {
    pub broker: Arc<Broker>,
    pub store: Arc<dyn SessionStore>,
    pub deliveries: Arc<dyn RemoteDeliveryStore>,
    pub exports: Arc<dyn ExportStore>,
    pub trust: Arc<dyn TrustStore>,
    pub audit: Arc<dyn AuditStore>,
    pub config: Arc<dyn LocalConfigStore>,
    pub attachments: Arc<dyn AttachmentStore>,
    pub catalog: Arc<dyn AgentCatalog>,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdGenerator>,
}

impl UseCases {
    pub fn new(deps: UseCaseDeps) -> Self {
        Self {
            broker: deps.broker,
            store: deps.store,
            deliveries: deps.deliveries,
            exports: deps.exports,
            trust: deps.trust,
            audit: deps.audit,
            config: deps.config,
            attachments: deps.attachments,
            catalog: deps.catalog,
            clock: deps.clock,
            ids: deps.ids,
            browser: Arc::new(SystemDirectories),
        }
    }

    /// 换用另一份目录浏览实现（组合根注入别的实现，或测试注入记录调用的替身）。
    ///
    /// 消耗 `self` 并返回它：组合根在 `Arc::new(UseCases::new(..))` 之前接一次即可，与
    /// [`UseCases::new`] 的「一次装配」语义一致（`&mut self` 的 setter 在 `Arc` 之后就用不了了）。
    pub fn with_directory_browser(self, browser: Arc<dyn DirectoryBrowser>) -> Self {
        Self { browser, ..self }
    }

    // ---------------------------------------------------------------------------------------
    // SessionCommands（§4）
    // ---------------------------------------------------------------------------------------

    /// 全部命令的唯一提交入口。mutation 走 [`Broker::submit_mutation`]；
    /// 查询命令在这里授权并确认"已接受、无副作用"，结果由对应查询入口返回。
    pub async fn submit_command(
        &self,
        actor: &Actor,
        command: ClientCommand,
    ) -> Result<CommandReceipt, PortError> {
        command.validate().map_err(PortError::from)?;
        if command.kind == CommandKind::Query {
            let name = command_name(&command.payload);
            if command.command != name {
                return Err(PortError::InvalidRequest("command 与 payload 变体不一致"));
            }
            self.broker
                .authorize(actor, name, command.session.as_ref(), &command.request)
                .await
                .map_err(Denied::into_port_error)?;
            return Ok(CommandReceipt::Accepted {
                request: command.request,
                turn: None,
            });
        }
        self.broker.submit_mutation(actor, &command).await
    }

    /// `session.create`（Node Link §12.7 / Sync §11.5）：先把目录来源解析成本机规范化绝对路径，
    /// 再以客户端 `requestId` 为幂等键走 core 的三笔时序——**受理 → 执行 → 落账**（§6 第 20 条）。
    ///
    /// `request_id`/`request_fingerprint` 由适配层给出：幂等键是
    /// `(actor, requestId)`（Node Link 的 actor 即本进程的 `Actor::Node` 对端），指纹是 ACPR-CJ1
    /// 之后的解码 payload 摘要（§6 第 6 条）。core 不自造 requestId——否则同一次重试会得到第二个
    /// 幂等键，重启后就会重复创建。
    ///
    /// `workspace` 是本次的目录来源（[`WorkspaceSource`]），三种来源的解析与失败分类：
    ///
    /// - [`WorkspaceSource::Alias`]：该请求声明的是别名——「是否在该 Export 的别名集合内」由
    ///   `server::node_link` 校验（`nodelink.export.not_granted`，参数类）；本层只负责本机解析，
    ///   **未在本机登记**是参数类错误，**已登记但解析失败** MUST 返回
    ///   [`UnavailableKind::IoError`]、**不得**降级为参数错误；
    /// - [`WorkspaceSource::Path`]（仅 Sync 面）：客户端提供的目录，见 [`resolve_client_workspace`]。
    ///   未给出建目录开关时该目录必须已存在且是目录；
    /// - [`WorkspaceSource::PathWithCreate`]（仅 Sync 面）：客户端提供的目录 + 「目录不存在则先创建」
    ///   开关，见 [`create_client_workspace`]。`sync-session-create` 要求该开关与别名**互斥**——这一点
    ///   由类型保证（别名没有对应的变体），不需要另做运行时判定；被创建的目录**不登记**为别名。
    ///
    /// 开关为真时的创建发生在**启动后端之前**，且**不回滚**：`SessionBackendFactory::create` 或落账
    /// 提交无论以何种方式失败，已创建的目录都留在主机上（`sync-session-create`「后续失败不删除已创建的
    /// 目录」）——删除要区分「哪些层是这次建的」，而误删用户目录的代价远高于留下一个空目录。
    ///
    /// UNC/网络路径允许使用；core 不持日志设施，因此「网络路径」的结构化警告由接入层在解析成功
    /// 后记录，本层不因它改变授权模型（`design.md` D6）。
    ///
    /// 解析成功时，**别名原文（若有）与该次解析得到的规范化路径一起**落进该会话行（`design.md`
    /// D1/D6、`workspace-resolution`）：别名是目录归属的权威投影来源，路径是恢复流程的权威取值，
    /// 二者独立保存、互不推导；目录来源是客户端提供的目录时**没有别名**，该会话呈现为「未分组」。
    /// 目录归属的读取与展示名关联都在读路径（`SessionStore::list`）。本层只把**别名与展示名的
    /// 投影**（`WorkspaceRef`）交给 broker 落进会话行与终态结果——展示名只有本层拿得到（它持有
    /// `LocalConfigStore`），读取时按别名关联目录记录取得的也是同一个值。
    ///
    /// 返回 [`SessionCreateAdmission`]：首次受理且后端建立成功时是 `Prepared`（适配层据此投影终态
    /// 结果，再调 [`UseCases::settle_session_create`] 落账）；窗口期重发是 `InFlight`（回受理态、
    /// 不再派发第二次创建）；已终结时是 `Settled`（回首次结果）。
    pub async fn create_session(
        &self,
        actor: &Actor,
        request_id: RequestId,
        request_fingerprint: Digest,
        mut request: CreateSessionRequest,
        workspace: Option<WorkspaceSource>,
    ) -> Result<SessionCreateAdmission, PortError> {
        // 授权先于任何本机读取与文件系统访问（与其余用例入口同款；broker 内部还会再授权一次，
        // 对本地 actor 恒成功、不重复写审计）。否则未授权调用方能借解析结果的差异探测「别名是否
        // 已登记、目录当前是否存在」——那是一个本机状态预言机。
        self.broker
            .authorize(actor, "session.create", None, &request_id)
            .await
            .map_err(Denied::into_port_error)?;
        let mut ownership = None;
        match workspace {
            Some(WorkspaceSource::Alias(alias)) => {
                let record =
                    self.config
                        .workspace(&alias)
                        .await?
                        .ok_or(PortError::InvalidRequest(
                            "workspace alias is not registered on this node",
                        ))?;
                request.workspace = Some(resolve_workspace(&alias, record.canonical_path())?);
                ownership = Some(
                    WorkspaceRef::try_new(alias, record.display_name()).map_err(PortError::from)?,
                );
            }
            Some(WorkspaceSource::Path(path)) => {
                // 客户端提供的目录没有别名：`ResolvedWorkspace` 的别名是 `None`，因此这里**不**
                // 造归属投影——该会话落库即「未分组」（`design.md` D1 的 `NULL` 语义）。
                request.workspace = Some(resolve_client_workspace(&path)?);
            }
            Some(WorkspaceSource::PathWithCreate(path)) => {
                // 建目录开关（`design.md` D4）：先创建（连同缺失的父层）再启动 Agent；创建出来的目录
                // **不登记**为别名，因此同样没有归属投影，该会话呈现为「未分组」。
                request.workspace = Some(create_client_workspace(&path)?);
            }
            None => {
                // 调用方已经交来解析结果（内部入口）：解析出别名的请求**必须**带上归属投影，否则投影给
                // 适配层的摘要与读路径看到的行会不一致（`check_workspace_ownership` 在那条边界上失败
                // 关闭）。展示名按读路径同一条规则取——目录记录在册时用记录的展示名，不在册时回退为
                // 别名本身（`CORE_PORTS_AND_STORAGE.md` §7.2 的 `LEFT JOIN` 回退）。
                if let Some(alias) = request
                    .workspace
                    .as_ref()
                    .and_then(|workspace| workspace.alias())
                {
                    let display_name = match self.config.workspace(alias).await? {
                        Some(record) => record.display_name().to_owned(),
                        None => alias.as_str().to_owned(),
                    };
                    ownership = Some(
                        WorkspaceRef::try_new(alias.clone(), &display_name)
                            .map_err(PortError::from)?,
                    );
                }
            }
        }
        self.broker
            .create_session(actor, &request_id, &request_fingerprint, request, ownership)
            .await
    }

    /// `session.create` 的**落账提交**（§6 第 20 条、§12.7）：把 [`UseCases::create_session`] 返回的
    /// `Prepared` 落成持久事实——会话行（含 core 分配的标识、ACP 会话标识、创建时 cwd、目录别名）、
    /// 幂等行的 `session_id` 回填与终态在**同一次提交**里完成，随后把窗口期排队的事件按序写库再广播。
    /// 结果由适配层投影（`completed` 携带 `SessionCreateResult` 的原文），失败/不确定携带适配层构造的
    /// 结构化错误。
    ///
    /// 没有持久记录（创建在幂等行落盘前失败）或记录已终结时是**幂等 no-op**（返回 `false`）；没有窗口期
    /// 状态、只有一条尚无会话的 `accepted` 记录时（后端失败那条路径的兜底）写「无会话」形状的终态。
    pub async fn settle_session_create(
        &self,
        actor: &Actor,
        request_id: &RequestId,
        status: CommandStatus,
        result: Option<CommandResult>,
        error: Option<PublicError>,
    ) -> Result<bool, PortError> {
        self.broker
            .settle_session_create(actor, request_id, status, result, error)
            .await
    }

    /// `session.resume`（Node Link，§12.7、§5.1）：恢复**进程不在**的 owned 会话，成功后返回该会话。
    ///
    /// 输入全部取自 Owner 自身的持久化记录（客户端不得提供 agent/cwd）：`request_id` 与
    /// `request_fingerprint` 由适配层给出（与 [`UseCases::create_session`] 同一幂等口径），`session`
    /// 由适配层从 `sessionRef` 解析。授权先于一切本机读取与文件系统访问；调用方拿到的只是
    /// `SessionId`——终态由适配层投影 `SessionResumeResult` 后经
    /// [`UseCases::settle_session_resume`] 提交。
    pub async fn resume_session(
        &self,
        actor: &Actor,
        request_id: RequestId,
        request_fingerprint: Digest,
        session: SessionId,
    ) -> Result<SessionId, PortError> {
        self.broker
            .resume_session(actor, &request_id, &request_fingerprint, &session)
            .await
    }

    /// `session.resume` 的终态（§5.1）：把已持久化的 `accepted` 行推进到终态。与
    /// [`UseCases::settle_session_create`] **同形**，但只终结 `session.resume` 的持久记录。
    ///
    /// 结果由适配层投影（`remoteSessionRef.exportId` 只有适配层有），core 的恢复用例只返回
    /// `SessionId`；没有持久记录或记录已终结时是**幂等 no-op**（返回 `false`）。
    pub async fn settle_session_resume(
        &self,
        actor: &Actor,
        request_id: &RequestId,
        status: CommandStatus,
        result: Option<CommandResult>,
        error: Option<PublicError>,
    ) -> Result<bool, PortError> {
        self.broker
            .settle_session_resume(actor, request_id, status, result, error)
            .await
    }

    // ---------------------------------------------------------------------------------------
    // SessionQueries（§4）
    // ---------------------------------------------------------------------------------------

    /// `session.list`：owned 摘要。授权过滤在本层完成（存储层只按 `query` 取行）。
    pub async fn list_sessions(
        &self,
        actor: &Actor,
        query: SessionQuery,
    ) -> Result<Vec<SessionSummary>, PortError> {
        let request = self.ids.request_id();
        self.broker
            .authorize(actor, "session.list", None, &request)
            .await
            .map_err(Denied::into_port_error)?;
        self.store.list(query).await
    }

    /// Sync 快照的 `sessions` 资源来源：**自有 + 已导入合并**后的设备可见清单
    /// （`SYNC_PROTOCOL.md` §9.6；spec `sync-event-delivery`「设备可见的会话清单范围」）。
    ///
    /// 为什么合并必须发生在 core（`design.md` D3）：可见性判定（`Broker::authorize`）在 core；若适配层
    /// 分别调 `SessionStore::list` 与 `RemoteDeliveryStore::list_sessions` 再自行拼接，就会出现**第二条
    /// 可见性判定路径**（`MODULE_ARCHITECTURE.md` §5 的单一判定点原则）。
    ///
    /// **为什么也不走 [`UseCases::require_local`]**：本入口服务的是入站设备的快照读取，它带 `actor`；
    /// `require_local` 只服务本机 CLI 的管理读面，把设备读取塞进那条守卫会放开本机管理面。设备方向已经
    /// 没有可见性过滤（`pairing-model`：设备不再有授权维度），因此 `actor` 在这里只用来区分「已认证设备」
    /// 与其它主体（节点仍要按 Export/grant 判定）。
    ///
    /// 本入口因此：
    ///
    /// 1. 先按 `session.list` 走 `Broker::authorize`——与 [`UseCases::list_sessions`] **逐字同口径**
    ///    （同一个命令名、同一个 broker 判定、同一个 `authorization.scope_denied`）：本层不写任何授权
    ///    判定。设备主体恒通过（设备不再有 scope 面），节点主体仍按 `grant.observe` 判定；授权先于本层的
    ///    任何读取，未授权时不读会话/索引存储、也不返回任何会话；
    /// 2. 自有条目直接取 `SessionStore::list`：`workspace` 因此取自该会话行**持久化**的
    ///    `workspace_alias`（§3.6 的决定原文，含「不得按 `workspace_cwd` 反查」）——本层不重算归属；
    /// 3. `include_imported` 为真时才取 `RemoteDeliveryStore::list_sessions` 的行并投影成**同一形状**的
    ///    `SessionSummary`（§9.6：imported 摘要不得另造字段）；为假时一行都不取——未协商
    ///    `resource.remote-origin.v1` 的客户端 MUST NOT 看到任何 `origin.kind = remote` 的会话，连空占位
    ///    条目也不得出现。
    ///
    /// 顺序是「自有在前、已导入在后」，两侧各自的顺序由存储层给出、本层不重排（`SessionSummary` 没有排序
    /// 语义字段，快照是集合语义）：同一次读取的输入决定同一次读取的输出。`query` 两侧的过滤/分页同样只是
    /// 调用方给出的确定性收窄，不构成第二个可见性判定点。
    ///
    /// `owner_reachable` 给出 imported 条目的 `origin.online`（Owner 可达性）：它是**运行时事实**，core
    /// 不缓存（`MODULE_ARCHITECTURE.md` §4.1：可达性变化经 `EventPublisher` 的
    /// `session.origin.online_changed` 暴露，「不在 core 里维护独立的在线状态缓存」），因此由调用方按自己
    /// 的连接事实传入，本层只做投影、不判定也不记忆。刻意**不**取固定保守值：§9.6 规定 Owner 离线期间
    /// 客户端不得把远程会话的输入标记为已发送，把一个确实可达的 Owner 一律标成 `false` 会让客户端错误地
    /// 封禁可用会话——那比缺席更糟。
    ///
    /// 闭包要求 `Send + Sync`：适配器会把本调用放进可被 `spawn` 的 future。
    pub async fn sync_visible_sessions(
        &self,
        actor: &Actor,
        query: SyncSessionQuery,
        owner_reachable: impl Fn(&RemoteSessionRef) -> bool + Send + Sync,
    ) -> Result<Vec<SessionSummary>, PortError> {
        let request = self.ids.request_id();
        self.broker
            .authorize(actor, "session.list", None, &request)
            .await
            .map_err(Denied::into_port_error)?;
        let SyncSessionQuery {
            owned,
            imported,
            include_imported,
        } = query;
        let mut sessions = self.store.list(owned).await?;
        if !include_imported {
            return Ok(sessions);
        }
        let rows = self.deliveries.list_sessions(imported).await?;
        for record in &rows {
            if let Some(summary) = imported_summary(record, &owner_reachable)? {
                sessions.push(summary);
            }
        }
        Ok(sessions)
    }

    /// Sync 增量重放的事件来源（`SYNC_PROTOCOL.md` §10.1 的 `event` 帧；spec `sync-event-delivery`
    /// 「增量重放与实时投递的无缝衔接」）：`after` 之后的 owned 事件，每条带 §10.1 要求的来源字段，外加
    /// 这批数据的一致性 head（`sync.caught_up` 的屏障游标）。
    ///
    /// **守卫与 [`UseCases::sync_visible_sessions`] 逐字同口径**：先按 `session.list` 走
    /// `Broker::authorize`（同一个命令名、同一个 broker 判定、同一个 `authorization.scope_denied`——本层
    /// 不写任何授权判定）。授权**先于**本层的任何读取，因此无权主体连一次只读视图都不会开，一条事件
    /// 也读不出（不是「读出来再丢掉」）。设备主体恒通过（设备不再有授权维度）、节点主体仍按
    /// `grant.observe` 判定。**为什么在 [`UseCases::require_local`] 之外**：与
    /// `sync_visible_sessions` 同一条理由——本入口服务的是**入站对端**的读取
    /// （`require_local` 保护的是本机 CLI/桌面入口的管理读面，放宽它会连带打开 `devices`/
    /// `profiles`/`workspaces` 那一批，风险面远大于需求）。
    ///
    /// 本层**不**投影：`event` 帧的每个字段都由 [`SyncReplaySlice`] 的条目承载（core 不做 JSON
    /// 序列化，`MODULE_ARCHITECTURE.md` §8），因此不另立用例级类型；`remoteOrigin` 是唯一例外——本读面
    /// 只返回 owned 事件，按 §9.6 它恒为 `null`（imported 事件的跳数与来源由投递层的 imported 路径另行
    /// 投影）。**设备方向的投递集合就是本层给出的这一页全部**：不再有按授权的可见性过滤
    /// （`pairing-model`），因此也不会再由过滤产生序号空洞；四类 cursor 判定仍是投递层的事：
    /// `server::sync` 的投递面负责把本层给出的这一页变成「事件 + `sync.caught_up`」，本层只如实给出
    /// 存储上按 `global_sequence` 升序的一页。
    ///
    /// **返回错误即要求投递层显式失败关闭**：存储行得不出唯一设备/节点 id 时（见
    /// [`SyncEventRecord::origin_device_id`]）读面返回 [`PortError::Corrupt`]；投递层必须记诊断并让该次
    /// 订阅失败关闭，MUST NOT 伪造 `kind`/`deviceId`/`nodeId`。
    pub async fn sync_replay(
        &self,
        actor: &Actor,
        after: Option<GlobalCursor>,
        limit: ReplayLimit,
    ) -> Result<SyncReplaySlice, PortError> {
        let request = self.ids.request_id();
        self.broker
            .authorize(actor, "session.list", None, &request)
            .await
            .map_err(Denied::into_port_error)?;
        self.store
            .read_view()
            .await?
            .sync_replay(after, limit)
            .await
    }

    // ---------------------------------------------------------------------------------------
    // 目录浏览与目录资源投影（§4 的 `SyncSnapshotQueries` / `workspace.browse`）
    // ---------------------------------------------------------------------------------------

    /// `workspace.browse`（`SYNC_PROTOCOL.md` §11.5；spec `sync-workspace-browse`）：逐层浏览本机目录。
    ///
    /// `path` 是请求的父路径原文（绝对路径），`None` = 请求本机的根列表。执行顺序即契约的顺序：
    ///
    /// 1. **授权**（`workspace.browse` 走 [`crate::broker::Broker::authorize`] 与 `required_grant` 的
    ///    `grant.observe`）——授权先于任何本机读取与文件系统访问；
    /// 2. **形状校验**（[`browse_parent`]）：不是绝对路径或含 `..` 是参数类错误，且**不触碰文件系统**；
    /// 3. **列举**（[`DirectoryBrowser`]）：形状合法但该层不存在、不可读或不是目录是服务端不可用类
    ///    （[`UnavailableKind::IoError`]），不返回任何条目，也不回退到父层或根列表；
    /// 4. **投影**（[`project_browse`]）：只保留名称与绝对路径 + 是否已登记，按名称的 Unicode 码点升序
    ///    排列，超过 [`WORKSPACE_BROWSE_ENTRY_LIMIT`] 时只返回上限内的条目并置截断标记。
    ///
    /// 只读：不登记、不缓存、不写会话状态，也不产生审计记录；被查看的目录不因此获得任何后续权限（它成为
    /// 会话归属的唯一路径仍是创建会话时按**已登记别名**解析）。
    ///
    /// 路径的**下发形式**是「`canonicalize` 的结果剥掉平台 verbatim 前缀」（`design.md` D2）：每个
    /// **条目**的 `path` 取该形式，客户端回传它时本层重新 `canonicalize` 并以其为权威。结果的 `path`
    /// **不**取该形式——它**逐字回显请求的父路径原文**（`None` = 根列表）：`SYNC_PROTOCOL.md` §11.5 与
    /// `command.schema.json#/$defs/workspaceBrowsePath` 要求请求字段与该回显字段逐字相同，规范化结果只
    /// 用于调用端口与 `registered` 判定。
    /// 未被浏览命令授权查看的层不因此泄漏——条目只覆盖请求的那一层（`sync-workspace-catalog`）。
    pub async fn workspace_browse(
        &self,
        actor: &Actor,
        path: Option<&str>,
    ) -> Result<WorkspaceBrowse, PortError> {
        let request = self.ids.request_id();
        self.broker
            .authorize(actor, "workspace.browse", None, &request)
            .await
            .map_err(Denied::into_port_error)?;
        // 形状校验先于一切文件系统访问；`None`（根列表）没有可校验的路径。规范化结果**只**用于调用
        // 端口：结果的 `path` 必须**逐字回显请求原文**（`SYNC_PROTOCOL.md` §11.5 与
        // `command.schema.json#/$defs/workspaceBrowsePath` 要求请求字段与结果回显字段逐字相同），
        // 因此原文原样留在 `echo` 里，不换成任何规范化形态。
        let (echo, canonical) = match path {
            Some(path) => (Some(path.to_owned()), Some(browse_parent(path)?)),
            None => (None, None),
        };
        let listings = match canonical.as_deref() {
            Some(parent) => self.browser.directories(parent).await?,
            None => self.browser.roots().await?,
        };
        // `registered` 标记是**本条命令唯一需要登记表的地方**：登记表的路径不下发，只用来算一个布尔值。
        let registered: Vec<String> = self
            .config
            .workspaces()
            .await?
            .iter()
            .map(|record| delivered_path(record.canonical_path()))
            .collect();
        Ok(project_browse(echo, listings, &registered))
    }

    /// Sync 快照 `workspaces` 资源的投影（`SYNC_PROTOCOL.md` §9.4；spec `sync-workspace-catalog`）。
    ///
    /// `include_path` 是**调用方**对 feature `core.workspace-path.v1` 的协商结果——core 不读 feature
    /// 表、也不缓存它（与 [`UseCases::sync_visible_sessions`] 的 `include_imported` 同一模式）。为假时
    /// 每个条目的 `path` 都是 `None`：对端看到的对象因此仍是「只有 `alias` 与 `displayName` 两个键」的
    /// 封闭形状，已协商 `core.local-catalog.v1` 的旧客户端不会因为多一个键拒绝整条快照。
    ///
    /// 路径的取值是**已登记目录**的规范化结果剥掉平台 verbatim 前缀后的形式（`design.md` D2），只出现在
    /// 这些条目上——会话摘要的 `workspace` 引用不带它，客户端按别名在同一份快照内查表。这里只投影，不
    /// 判定可见性：设备方向没有可见性过滤（`pairing-model`），本入口与本机 `workspace.list` 走同一份
    /// 登记表读取（[`UseCases::workspaces`] 的 `require_local` 守卫照旧）。
    pub async fn sync_workspace_catalog(
        &self,
        actor: &Actor,
        include_path: bool,
    ) -> Result<Vec<WorkspaceCatalogEntry>, PortError> {
        self.require_local(actor)?;
        let records = self.config.workspaces().await?;
        records
            .iter()
            .map(|record| {
                let path = include_path.then(|| delivered_path(record.canonical_path()));
                WorkspaceCatalogEntry::try_new(
                    record.alias().clone(),
                    record.display_name(),
                    path.as_deref(),
                )
                .map_err(PortError::from)
            })
            .collect()
    }

    /// `session.read`：owned 走事件日志；`include` 里的活体数据（config/capabilities）由本层从
    /// 后端与 `AgentCatalog` 合并——存储层实现必须让那两个字段保持空值。
    pub async fn read_session(
        &self,
        actor: &Actor,
        query: HistoryQuery,
    ) -> Result<HistoryPage, PortError> {
        let request = self.ids.request_id();
        self.broker
            .authorize(actor, "session.read", Some(&query.session), &request)
            .await
            .map_err(Denied::into_port_error)?;
        let mut page = self.broker.read_session(query.clone()).await?;
        if query.include.config_options {
            page.config = self.live_config(&query.session).await?;
        }
        if query.include.capabilities {
            page.capabilities = Some(
                self.catalog
                    .agent_capabilities(page.session.agent())
                    .await?,
            );
        }
        Ok(page)
    }

    /// `session.mode.list` / `session.config.list` 的 config 部分（§11.5 的 `SessionConfigOptionView`）。
    pub async fn config_options(
        &self,
        actor: &Actor,
        reference: &SessionReference,
    ) -> Result<Vec<ConfigOption>, PortError> {
        let request = self.ids.request_id();
        let session = owned_session(reference)?;
        self.broker
            .authorize(actor, "session.config.list", Some(&session), &request)
            .await
            .map_err(Denied::into_port_error)?;
        self.live_config(&session).await
    }

    /// `command.status`（§11.4）：只返回该 actor 自己提交的命令；不存在或不可见一律 `None`。
    pub async fn command_status(
        &self,
        actor: &Actor,
        request: RequestId,
    ) -> Result<Option<CommandRecord>, PortError> {
        let audit_request = self.ids.request_id();
        self.broker
            .authorize(actor, "command.status", None, &audit_request)
            .await
            .map_err(Denied::into_port_error)?;
        self.broker.command_status(actor, &request).await
    }

    /// 远端目录投影（§4 的 `RemoteCatalogQueries`）：本节点的 Agent 目录。
    pub async fn agents(&self, actor: &Actor) -> Result<Vec<AgentDescriptor>, PortError> {
        let request = self.ids.request_id();
        self.broker
            .authorize(actor, "session.list", None, &request)
            .await
            .map_err(Denied::into_port_error)?;
        self.catalog.agents().await
    }

    pub async fn agent_capabilities(
        &self,
        actor: &Actor,
        agent: &AgentRef,
    ) -> Result<CapabilitySet, PortError> {
        let request = self.ids.request_id();
        self.broker
            .authorize(actor, "session.mode.list", None, &request)
            .await
            .map_err(Denied::into_port_error)?;
        self.catalog.agent_capabilities(agent).await
    }

    // ---------------------------------------------------------------------------------------
    // ConfigCommands（§4）
    // ---------------------------------------------------------------------------------------

    pub async fn set_mode(
        &self,
        actor: &Actor,
        reference: SessionReference,
        mode: ModeId,
    ) -> Result<Version, PortError> {
        self.broker.set_mode(actor, &reference, mode).await
    }

    pub async fn set_config(
        &self,
        actor: &Actor,
        reference: SessionReference,
        id: ConfigOptionId,
        value: ConfigValue,
    ) -> Result<Version, PortError> {
        self.broker.set_config(actor, &reference, id, value).await
    }

    // ---------------------------------------------------------------------------------------
    // PermissionCommands（§4）
    // ---------------------------------------------------------------------------------------

    pub async fn resolve_interaction(
        &self,
        actor: &Actor,
        reference: SessionReference,
        interaction: InteractionId,
        resolution: InteractionResolution,
    ) -> Result<Resolution, PortError> {
        self.broker
            .resolve_interaction(actor, &reference, &interaction, resolution)
            .await
    }

    /// elicitation 的便捷入口（`elicitation.respond` 的 §4 形态）。
    pub async fn respond_elicitation(
        &self,
        actor: &Actor,
        reference: SessionReference,
        interaction: InteractionId,
        action: ElicitationAction,
        values: ElicitationValues,
    ) -> Result<Resolution, PortError> {
        let resolution = match action {
            ElicitationAction::Submit => {
                InteractionResolution::elicitation_submit(values).map_err(PortError::from)?
            }
            ElicitationAction::Cancel => InteractionResolution::elicitation_cancel(),
            ElicitationAction::Decline => InteractionResolution::elicitation_decline(),
        };
        self.resolve_interaction(actor, reference, interaction, resolution)
            .await
    }

    // ---------------------------------------------------------------------------------------
    // SubscriptionQueries（§4）
    // ---------------------------------------------------------------------------------------

    /// 增量重放（§6.12：快照与重放必须出自同一读视图；需要同一视图时用
    /// [`Broker::read_view`]）。
    pub async fn replay(
        &self,
        actor: &Actor,
        reference: &SessionReference,
        after: Option<GlobalCursor>,
        limit: ReplayLimit,
    ) -> Result<ReplayBatch, PortError> {
        let request = self.ids.request_id();
        let session = owned_session(reference)?;
        self.broker
            .authorize(actor, "session.read", Some(&session), &request)
            .await
            .map_err(Denied::into_port_error)?;
        self.broker.replay(after, limit).await
    }

    /// imported 会话的本地重放（只含无正文索引）。
    pub async fn remote_replay(
        &self,
        actor: &Actor,
        reference: &SessionReference,
        after: Option<LocalCursor>,
        limit: ReplayLimit,
    ) -> Result<Vec<DeliveryIndexEntry>, PortError> {
        let request = self.ids.request_id();
        match reference {
            SessionReference::Remote(_) => {
                self.broker
                    .authorize(actor, "session.read", None, &request)
                    .await
                    .map_err(Denied::into_port_error)?;

                self.broker.remote_replay(after, limit).await
            }
            SessionReference::Owned(_) => {
                Err(PortError::InvalidRequest("owned 会话的重放请使用 replay()"))
            }
        }
    }

    /// §6 第 17 条：`session.mode.list`。候选**只**来自 `SessionEndpoint::modes()`，版本取会话当前版本；
    /// 端口返回空列表时结果就是空列表（不凭 `current_mode` 编造候选）。
    pub async fn mode_list(
        &self,
        actor: &Actor,
        reference: &SessionReference,
    ) -> Result<ModeListing, PortError> {
        let request = self.ids.request_id();
        let session = owned_session(reference)?;
        self.broker
            .authorize(actor, "session.mode.list", Some(&session), &request)
            .await
            .map_err(Denied::into_port_error)?;
        let endpoint = self.broker.endpoint_for(reference).await?;
        let state = endpoint.modes().await?;
        let version = self.broker.session_version(&session).await?;
        Ok(ModeListing { state, version })
    }

    /// §6 第 16 条：启动恢复（`LocalCli`）。组合根在取得单实例锁、开始监听**之前**调用；返回被终结的
    /// 命令数。**不**自动重放副作用；广播一律发生在 `commit` 之后。
    pub async fn recover_unsettled(
        &self,
        actor: &Actor,
        limit: ReplayLimit,
    ) -> Result<usize, PortError> {
        self.require_local(actor)?;
        self.broker.recover_unsettled(limit).await
    }

    pub async fn retention_window(
        &self,
        actor: &Actor,
        session: SessionId,
    ) -> Result<Option<(Sequence, Sequence)>, PortError> {
        let request = self.ids.request_id();
        self.broker
            .authorize(actor, "session.read", Some(&session), &request)
            .await
            .map_err(Denied::into_port_error)?;
        self.broker.retention_window(&session).await
    }

    // ---------------------------------------------------------------------------------------
    // 本地管理族（§4 的 DeviceManagement/ExportManagement/RemoteCatalogQueries）
    // ---------------------------------------------------------------------------------------

    pub async fn devices(&self, actor: &Actor) -> Result<Vec<DeviceRecord>, PortError> {
        self.require_local(actor)?;
        self.trust.devices().await
    }

    pub async fn device(
        &self,
        actor: &Actor,
        id: &DeviceId,
    ) -> Result<Option<DeviceRecord>, PortError> {
        self.require_local(actor)?;
        self.trust.device(id).await
    }

    /// 写入设备记录（`device.add` 与身份层共用）：状态与审计同一事务（§11.2 第 6 条）。
    ///
    /// **不写任何授权相关审计**：设备记录不再有授权集合（`pairing-model` 删除设备权限层），因此
    /// `device.scopes_changed` 这条安全动作不可能再发生（`AuditAction` 里保留它只为读回升级前的库里的
    /// 历史行）。设备新建也没有对应的安全动作——它由 `device.add` 这类本机管理操作发起，而「涉及已登记
    /// 安全动作的写集才必须带审计」；与身份建立相关的那两条留痕分别是
    /// [`UseCases::record_device_connected`]（`device.authenticated`）与配对落定
    /// （`pairing.approved`）。
    pub async fn put_device(&self, actor: &Actor, record: DeviceRecord) -> Result<(), PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        self.trust
            .put_device(DeviceWrite {
                record,
                context: WriteContext {
                    at,
                    audit: Vec::new(),
                },
            })
            .await
    }

    /// `device.revoke`（`LOCAL_ADMIN_PROTOCOL.md` §5.3）：撤销与审计同一事务；提交后才由组合根关连接。
    pub async fn revoke_device(&self, actor: &Actor, id: &DeviceId) -> Result<(), PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        let audits = vec![self.pending_audit(actor, AuditAction::DeviceRevoked, k_device(id))];
        self.trust
            .revoke_device(DeviceRevocation {
                device: id.clone(),
                reason: RevokeReason::UserRequested,
                context: WriteContext { at, audit: audits },
            })
            .await
    }

    pub async fn nodes(&self, actor: &Actor) -> Result<Vec<NodeRecord>, PortError> {
        self.require_local(actor)?;
        self.trust.nodes().await
    }

    /// 按 `(NodeId, NodeKind)` 取行；禁止「找不到就取第一行」（§11.5）。
    pub async fn node(
        &self,
        actor: &Actor,
        id: &NodeId,
        kind: NodeKind,
    ) -> Result<Option<NodeRecord>, PortError> {
        self.require_local(actor)?;
        self.trust.node(id, kind).await
    }

    /// 该对端的全部角色行。
    pub async fn nodes_for(
        &self,
        actor: &Actor,
        id: &NodeId,
    ) -> Result<Vec<NodeRecord>, PortError> {
        self.require_local(actor)?;
        self.trust.nodes_for(id).await
    }

    /// 已绑定的身份材料（验签公钥的唯一来源）。
    pub async fn peer_key(
        &self,
        actor: &Actor,
        peer: &PeerIdentity,
    ) -> Result<Option<PeerPublicKey>, PortError> {
        self.require_local(actor)?;
        self.trust.peer_key(peer).await
    }

    /// 写入节点角色行与身份材料；配对确认路径负责写 `node.paired`（§11.6 第 2 条）。
    pub async fn put_node(
        &self,
        actor: &Actor,
        record: NodeRecord,
        public_key: PeerPublicKey,
    ) -> Result<(), PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        self.trust
            .put_node(NodeWrite {
                record,
                public_key,
                context: WriteContext {
                    at,
                    audit: Vec::new(),
                },
            })
            .await
    }

    /// `node.revoke`（`LOCAL_ADMIN_PROTOCOL.md` §5.4）：按 NodeId 撤销，覆盖两种角色。
    pub async fn revoke_node(&self, actor: &Actor, id: &NodeId) -> Result<(), PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        let audits = vec![self.pending_audit(actor, AuditAction::NodeTrustRevoked, k_node(id))];
        self.trust
            .revoke_node(NodeRevocation {
                node: id.clone(),
                reason: RevokeReason::UserRequested,
                context: WriteContext { at, audit: audits },
            })
            .await
    }

    /// `device.pair.begin` / `node.pair.begin`：登记一次性配对（状态 + 审计同一事务）。
    pub async fn create_pairing(
        &self,
        actor: &Actor,
        pairing: PairingRecord,
    ) -> Result<(), PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        let audits =
            vec![self.pending_audit(actor, AuditAction::PairingCreated, k_pairing(&pairing))];
        self.trust
            .create_pairing(PairingWrite {
                record: pairing,
                context: WriteContext { at, audit: audits },
            })
            .await
    }

    /// 原子认领（HMAC 由调用方验证，`SYNC_PROTOCOL.md` §7.2）。
    ///
    /// 除本机入口外接受**绑定同一配对**的 `Actor::PairingClaimant`（配对 HTTP 通道，design D12）；
    /// 绑定不一致与其它 actor 一律 `authorization.scope_denied`。
    pub async fn claim_pairing(
        &self,
        actor: &Actor,
        claim: PairingClaim,
    ) -> Result<PairingClaimOutcome, PortError> {
        require_pairing_access(actor, claim.pairing())?;
        let at = self.clock.now();
        let target = EntityRef::Pairing(claim.pairing().clone());
        let audits = vec![self.pending_audit(actor, AuditAction::PairingClaimed, target)];
        self.trust
            .claim_pairing(PairingClaimWrite {
                claim,
                context: WriteContext { at, audit: audits },
            })
            .await
    }

    /// 配对状态读取（claim/status 两条通道共用）。
    ///
    /// 与 [`UseCases::claim_pairing`] 同一套访问规则：本机入口，或绑定该配对的 `PairingClaimant`。
    pub async fn pairing(
        &self,
        actor: &Actor,
        id: &PairingId,
    ) -> Result<Option<PairingRecord>, PortError> {
        require_pairing_access(actor, id)?;
        self.trust.pairing(id).await
    }

    /// 首次认证成功后的配对消费（`IDENTITY_AND_AUTH_CONTRACT.md` §5.1 的收尾副作用，design D12）。
    ///
    /// 授权：只接受与该配对**已批准对端一致**的 `Actor::Node`/`Actor::Device`；claimant、本机入口与
    /// 对端错配一律 `authorization.scope_denied`/`Conflict(IdentityMismatch)`。状态推进、`terminal_at`
    /// 与审计（节点 `node.authenticated`、设备 `device.authenticated`）在一个写集里提交；节点对端的
    /// 那一次还会在同一事务推进该对端节点行的 `last_connected_at`（§11.6 第 9 条）；已是 `consumed`
    /// 且对端一致时幂等成功。存储层在同一事务内重做对端比对与状态守卫（并发权威）。
    pub async fn consume_pairing(
        &self,
        actor: &Actor,
        id: &PairingId,
    ) -> Result<PairingRecord, PortError> {
        let action = match actor {
            Actor::Node { .. } => AuditAction::NodeAuthenticated,
            Actor::Device { .. } => AuditAction::DeviceAuthenticated,
            _ => return Err(PortError::InvalidRequest("authorization.scope_denied")),
        };
        let record = self
            .trust
            .pairing(id)
            .await?
            .ok_or_else(|| PortError::NotFound(EntityRef::Pairing(id.clone())))?;
        let peer = self
            .trust
            .pairing_peer(id)
            .await?
            .ok_or(PortError::Corrupt("approved pairing has no peer row"))?;
        if !peer_matches_actor(&peer, actor) {
            return Err(PortError::Conflict(ConflictKind::IdentityMismatch));
        }
        match record.state() {
            PairingState::Approved | PairingState::Consumed => {}
            state if state.is_terminal() => return Err(terminal_pairing_conflict(state)),
            _ => return Err(PortError::InvalidRequest("pairing has not been approved")),
        }
        let at = self.clock.now();
        let audits = vec![self.pending_audit(actor, action, EntityRef::Pairing(id.clone()))];
        self.trust
            .consume_pairing(PairingConsumption {
                pairing: id.clone(),
                actor: actor.clone(),
                context: WriteContext { at, audit: audits },
            })
            .await
    }

    /// Node Link 握手准入的**未认证**只读入口（`design.md` D3；合同扩展见 §10 的 2026-09-26 裁定条目）。
    ///
    /// 这是用例面唯一没有 `actor` 的入口：握手完成前不存在已验证主体——`node.hello` 里的
    /// `accessNodeId` 只是**待验证的自报身份**（`NODE_LINK_PROTOCOL.md` §12.2 的第 1 步）。授权面因此
    /// 收窄到「单个自报 node id」：
    ///
    /// - 只读该 id 自己的 `access` 信任行、身份材料、最近一次配对与本机水位，不返回任何别的节点、设备、
    ///   Export 的事实，也不返回秘密材料；
    /// - 未知 id 返回**空视图**（`node`/`public_key`/`pairing` 都是 `None`）而不是错误：调用方照常签发
    ///   挑战，对端可见的失败映射发生在 proof 阶段（§12.2「不得用错误区分节点是否存在」）；
    /// - 零写入、不产生审计；撤销由调用方按 `node.state()` 自行判定（§5.1：`Revoked`/`Unknown` 不是
    ///   握手失败，而是凭据状态）。
    pub async fn node_link_handshake_view(
        &self,
        access_node: &NodeId,
    ) -> Result<NodeLinkHandshakeView, PortError> {
        let peer = PeerIdentity::Node(access_node.clone());
        let node = self.trust.node(access_node, NodeKind::Access).await?;
        let public_key = self.trust.peer_key(&peer).await?;
        let pairing = self.trust.pairing_for(&peer).await?;
        let head = self.store.head().await?;
        let catalog_revision = self.audit.watermark().await?;
        Ok(NodeLinkHandshakeView {
            node,
            public_key,
            pairing,
            head,
            catalog_revision,
        })
    }

    /// Sync 设备握手准入的**未认证**只读入口（`design.md` D2；`IDENTITY_AND_AUTH_CONTRACT.md` §5.1）。
    ///
    /// **为什么在 [`UseCases::require_local`] 之外**：`require_local` 保护的是「本机 CLI/桌面入口才能看的
    /// 管理读面」（`devices`/`device`/`peer_key`/`profiles`/`workspaces`），而本入口服务的是**尚未认证**的
    /// 入站连接——握手完成前不存在已验证主体，`sync.hello` 里的 `deviceId` 只是**待验证的自报身份**
    /// （`SYNC_PROTOCOL.md` §8.1）。放宽 `require_local` 会连带打开一批本机管理读面，风险面远大于需求
    /// （`design.md` D2 的否决理由），因此这里照 [`UseCases::node_link_handshake_view`] 的模式新增窄入口，
    /// 把授权面收窄到「单个自报 device id」：
    ///
    /// - 只读该 id 自己的信任行、身份材料、最近一次配对与本机水位，不返回任何别的设备/节点/Export 的事实，
    ///   也不返回秘密材料；
    /// - 未知 id 返回**结构完整的空视图**（`device`/`public_key`/`pairing` 都是 `None`，`head` 照常给出）
    ///   而不是错误：`SYNC_PROTOCOL.md` §8.2/§8.3 与 `identity-auth` 的既有口径一致
    ///   （`IDENTITY_AND_AUTH_CONTRACT.md` §5.1：未知 id 也必须生成挑战，不得用错误区分设备是否存在）
    ///   ——未知对端照常签发挑战，对端可见的失败映射发生在 proof 阶段。**不得**用错误类型区分设备
    ///   是否存在，否则本入口就成了设备 id 预言机；
    /// - 零写入、不产生审计；撤销由调用方按 `device.state()` 自行判定（`revoked` 是凭据状态而不是读取
    ///   失败，由身份层映射）。
    ///
    /// 本入口不承担认证收尾：认证成功后的 `last_seen_at` 推进与 `device.authenticated` 留痕见
    /// [`UseCases::record_device_connected`]。
    pub async fn sync_device_handshake_view(
        &self,
        device_id: &DeviceId,
    ) -> Result<SyncDeviceHandshakeView, PortError> {
        let peer = PeerIdentity::Device(device_id.clone());
        let device = self.trust.device(device_id).await?;
        let public_key = self.trust.peer_key(&peer).await?;
        let pairing = self.trust.pairing_for(&peer).await?;
        let head = self.store.head().await?;
        Ok(SyncDeviceHandshakeView {
            device,
            public_key,
            pairing,
            head,
        })
    }

    /// Node Link 的审计留痕（`SECURITY_DESIGN.md` §14.2 的 `node.authenticated`/`node.auth_failed`
    /// 与 `authorization.denied`）。
    ///
    /// 只接受这三个动作（其余一律 `authorization.scope_denied`）：握手与授权拒绝都是安全动作、都要
    /// 留痕，但这个入口不是通用审计写面。归因一律按 Node Link 对端——`actor` 与 `via_node` 都是该对端
    /// id，`target = Node(对端)`，`localPrincipalRef` 为 `None`（节点级信任模型，§8.3），`detailDigest`
    /// 为空（细分失败原因只在结构化日志里，不进审计行）。
    ///
    /// `authorization.denied` 服务 `server::node_link` 的命令管线：它的可见性/授权交集（Export grant
    /// ∩ 信任记录 grant）比 `Broker::authorize` 的 Owner 侧判定更严，被它拒的命令不会到达 broker，
    /// 因此拒绝留痕只能经本入口（`design.md` D8 第 2 步）。
    ///
    /// 首次认证成功（已批准配对 → `consumed`）的 `node.authenticated` 由
    /// [`UseCases::consume_pairing`] 的写集提交；重复认证成功（没有待消费配对）的留痕与
    /// `last_connected_at` 的推进由 [`UseCases::record_node_connected`] 的写集提交——这两处适配器都
    /// **不要**再补一条。本入口服务「认证失败」这类没有状态推进的留痕（以及调用方确实只需要一条独立
    /// 审计行的场合）。
    pub async fn record_node_link_auth(
        &self,
        access_node: &NodeId,
        action: AuditAction,
        outcome: AuditOutcome,
    ) -> Result<(), PortError> {
        if !matches!(
            action,
            AuditAction::NodeAuthenticated
                | AuditAction::NodeAuthFailed
                | AuditAction::AuthorizationDenied
        ) {
            return Err(PortError::InvalidRequest("authorization.scope_denied"));
        }
        let at = self.clock.now();
        let record = AuditRecord::try_new(
            at,
            action,
            Actor::Node {
                node: access_node.clone(),
                access_node: access_node.clone(),
            },
            Some(access_node.clone()),
            None,
            EntityRef::Node(access_node.clone()),
            outcome,
            None,
        )
        .map_err(PortError::from)?;
        self.audit.append(record).await
    }

    /// 重复认证成功的收尾写集（§11.6 第 9 条；`IDENTITY_AND_AUTH_CONTRACT.md` §5.1 的收尾副作用）。
    ///
    /// 认证成功但没有待消费配对时，`last_connected_at` 与 `node.authenticated` 必须与首次认证一样在
    /// **同一事务**提交：该列是「最近一次认证成功时间」，只在首次认证写会让它对重复连接说谎。归因与
    /// [`UseCases::record_node_link_auth`] 的 `node.authenticated` 逐字一致（`actor`/`via_node` 都是该
    /// 对端、`target = Node(对端)`、`localPrincipalRef` 为 `None`）。
    ///
    /// 角色恒取 `NodeKind::Access`：Node Link 的入站对端只可能是配对批准写下的 `access` 行
    /// （`LOCAL_ADMIN_PROTOCOL.md` §5.4 的 `--mode access` 没有 `confirm` 调用）。
    pub async fn record_node_connected(&self, access_node: &NodeId) -> Result<(), PortError> {
        let at = self.clock.now();
        let actor = Actor::Node {
            node: access_node.clone(),
            access_node: access_node.clone(),
        };
        let audits = vec![self.pending_audit(
            &actor,
            AuditAction::NodeAuthenticated,
            EntityRef::Node(access_node.clone()),
        )];
        self.trust
            .record_node_connected(NodeConnectedWrite {
                node: access_node.clone(),
                kind: NodeKind::Access,
                context: WriteContext { at, audit: audits },
            })
            .await
    }

    /// 设备认证成功的收尾写集（`SYNC_PROTOCOL.md` §8.4；`SECURITY_DESIGN.md` §14.2 的
    /// `device.authenticated`）。
    ///
    /// 与 [`UseCases::record_node_connected`] 同形：把 `owned_device.last_seen_at` 推进到本次时间，并把
    /// 该设备的 `device.authenticated` 成功行在**同一事务**提交（`WriteContext` 携带两者，§11.2 第 6 条）。
    ///
    /// **为什么在 [`UseCases::require_local`] 之外**：本入口服务的是**入站对端**的连接收尾——此刻设备已
    /// 由握手证明持有该设备持久化的私钥（`identity-auth` 的 `verify_proof` 只用 `TrustStore::peer_key`
    /// 的密钥验签），它的身份就是 `device_id` 本身，因此本入口与 `record_node_connected` 一样**没有
    /// `actor` 参数**、归因由 id 唯一确定。`require_local` 保护的是本机 CLI 的**管理**读面
    /// （`devices`/`device`/…），把它套在连接收尾上会迫使适配器伪造一个本机主体。
    ///
    /// **只前进语义不在本层实现**：`last_seen_at` 的三分支（旧值为空→写入新值、新值为空→保留旧值、两者
    /// 非空取较大者）由存储层的 `put_device` 保证（`crates/storage-sqlite/src/admin/trust.rs` 的
    /// `upsert_device`；其判据是 `storage-sqlite` 的 `timestamps_are_advanced_never_erased`）。因此本层
    /// 只读出既有行、把 `last_seen_at` 置为 `Some(now)`、把**其余每一列原样写回**：它不复制也不替代那条
    /// 规则，更不得用 `None` 抹掉已存值。撤销优先同样由存储层判定（`state = revoked` 的记录会被
    /// `put_device` 拒绝，唯一复活路径是重新配对）：本层**不预先**判定撤销，也不把该行改成别的状态。
    ///
    /// 设备行不存在时返回 `NotFound(EntityRef::Device)` 且零写入：`DeviceWrite` 携带的是**整行**，行不存在
    /// 就无法只推进一列（编造一行等于伪造信任），也没有任何状态变更需要留痕——与
    /// [`UseCases::record_node_connected`] 的行缺失口径一致。
    ///
    /// 注意与配对消费的分工：`consume_pairing` 的写集只推进**节点**行的 `last_connected_at`，设备配对
    /// 没有节点行、也不碰设备行（`storage-sqlite` 的 `consume_pairing`），因此 `owned_device.last_seen_at`
    /// 的**唯一生产者是本入口**——首次认证（有待消费配对）也必须经它推进，否则 `lastSeenAt` 会一直为空。
    pub async fn record_device_connected(&self, device_id: &DeviceId) -> Result<(), PortError> {
        let existing = self
            .trust
            .device(device_id)
            .await?
            .ok_or_else(|| PortError::NotFound(k_device(device_id)))?;
        let at = self.clock.now();
        // 归因只由**持久化**身份给出（本入口没有 actor 参数）：设备行里的 id 即主体，设备方向不再有
        // 任何授权集合，因此审计行的 actor 就是一个裸 `Actor::Device`。
        let actor = Actor::Device {
            device: device_id.clone(),
        };
        let record = DeviceRecord::try_new(
            existing.device_id().clone(),
            existing.display_name(),
            existing.public_key_fingerprint().clone(),
            existing.state(),
            existing.created_at().clone(),
            Some(at.clone()),
            existing.revoked_at().cloned(),
        )
        .map_err(PortError::from)?;
        let audits = vec![self.pending_audit(
            &actor,
            AuditAction::DeviceAuthenticated,
            k_device(device_id),
        )];
        self.trust
            .put_device(DeviceWrite {
                record,
                context: WriteContext { at, audit: audits },
            })
            .await
    }

    /// 设备**认证失败**的审计留痕（`SECURITY_DESIGN.md` §14.2 的 `device.auth_failed`；
    /// `SYNC_PROTOCOL.md` §8 的握手失败路径）。
    ///
    /// `identity-auth` 的握手失败分类自带审计动作要求（`HandshakeFailure::audit()` 对
    /// `ConnectionKind::SyncDevice` 返回 `AuditAction::DeviceAuthFailed`），但它的返回值只是**动作**而不是
    /// 审计行：行由调用方按本次连接的对端身份组装（见 `IDENTITY_AND_AUTH_CONTRACT.md` 关于 `audit()` 的
    /// 条款）。Node Link 侧由 [`UseCases::record_node_link_auth`] 承接那三个 node 动作，本入口是设备侧的
    /// 对应物：动作**恒为** `AuditAction::DeviceAuthFailed`（不接受动作参数，因此结构上不可能经本入口写
    /// 出别的动作，比那个入口更窄）、目标恒为 `Device(device_id)`、结果恒为失败
    /// （`AuditOutcome::Failed`）。`via_node` 为 `None`（那是 Node Link 路径的列）、
    /// `local_principal_ref` 为 `None`、`detail_digest` 为空：细分失败原因只进结构化日志，不进审计行
    /// （`SECURITY_DESIGN.md` §14.1/§14.2 的 no-content 取舍）。
    ///
    /// **为什么在 [`UseCases::require_local`] 之外、也不带 `actor` 参数**：本入口服务的是**尚未认证**的
    /// 入站连接——失败恰恰意味着本次连接没有取得已验证主体，`sync.hello` 里的 `deviceId` 只是**待验证的
    /// 自报身份**（`SYNC_PROTOCOL.md` §8.1）。套上 `require_local` 会迫使适配器伪造一个本机主体；而把
    /// `actor` 收成参数就等于把审计归因交给握手消息自报的取值。两者都与「归因取持久化身份」相悖，因此本
    /// 入口与 [`UseCases::sync_device_handshake_view`] 同一模式：只收 `device_id`，归因只能由 core 从
    /// **持久化**记录读出。`require_local` 保护的是本机 CLI 的**管理读面**（`devices`/`device`/…），这里
    /// 不需要它、也不得为它放宽它。
    ///
    /// **已知设备**：读出该设备的信任行，归因取 `Actor::Device { device: 该行 id }`——身份取**持久化**
    /// 值，**不得**取握手自报值（`sync-device-auth`「认证失败的审计留痕」要求 actor 归因取持久化身份）。
    /// 设备方向不再有授权集合，因此 actor 里也没有任何集合字段可填。除该行外零读取，且**零写入**：留痕不
    /// 推进 `last_seen_at`、不改状态，该行原样留在存储里。
    ///
    /// **未知设备**：**同样写出这一行**，归因取 `Actor::Device { device: device_id.clone() }`（没有任何
    /// 集合字段可以从自报值里编造出来）。**设备未知时不得返回 `NotFound`**（这一点与
    /// [`UseCases::record_device_connected`] 刻意不同，那个入口的整行写集要求行存在）：未配对设备发起握手
    /// 并失败是最常见的失败场景，正是本入口存在的主要理由；返回 `NotFound` 会让调用方无处留痕，而
    /// 「未知 → 错误 / 已知 → 成功」的响应差异还会变成一个**设备存在性预言机**（`sync-device-auth`「未知
    /// 与已撤销设备的可见行为」与「认证失败的审计留痕」都要求不得用错误类别区分设备是否存在）。因此两种
    /// 情形都返回 `Ok(())`，且都恰好写出一行形状相同的审计，响应里没有任何存在性信号。
    ///
    /// 不新增端口方法：本入口没有关联的状态变更（`CORE_PORTS_AND_STORAGE.md` §11.2 第 6 条的后半句），
    /// 直接用 `AuditStore::append` 追加即可。成功路径的 `device.authenticated` 由
    /// [`UseCases::record_device_connected`] 的收尾写集承担，两者**不得互相替代**。
    pub async fn record_device_auth_failure(&self, device_id: &DeviceId) -> Result<(), PortError> {
        // 归因只由 id 给出：设备方向没有集合字段，因此这里的 actor 形状与「该 id 有没有持久行」无关，
        // 也就不必读那一行——审计行的 actor_kind/actor_id 两列在两种情形下逐字相同（未知设备不得因
        // 差异被区分出来）。
        let actor = Actor::Device {
            device: device_id.clone(),
        };
        let at = self.clock.now();
        let record = AuditRecord::try_new(
            at,
            AuditAction::DeviceAuthFailed,
            actor,
            None,
            None,
            k_device(device_id),
            AuditOutcome::Failed,
            None,
        )
        .map_err(PortError::from)?;
        self.audit.append(record).await
    }

    // ---------------------------------------------------------------------------------------
    // Node Link 资源读面（WP5 的窄 seam；design D6）
    // ---------------------------------------------------------------------------------------

    /// Node Link 的 catalog 投影源（`design.md` D4；`NODE_LINK_PROTOCOL.md` §12.3）。
    ///
    /// **无 actor**（与 [`UseCases::node_link_handshake_view`] 同一模式）：连接层在认证完成后以已认证
    /// 的 `access_node` 调用，授权面因此收窄到「单个已认证 node id」。零写入、零审计。未知 id 返回
    /// `node = None` 的空视图（不用错误区分存在性）；本机 Export 记录与撤销状态一律**当场从持久化记
    /// 录读**，不做缓存副本（因此撤销天然即时生效）。
    pub async fn node_link_catalog_view(
        &self,
        access_node: &NodeId,
    ) -> Result<NodeLinkCatalogView, PortError> {
        let node = self.trust.node(access_node, NodeKind::Access).await?;
        let exports = self.exports.exports().await?;
        let agents = self.catalog.agents().await?;
        let revision = self.audit.watermark().await?;
        Ok(NodeLinkCatalogView {
            revision,
            node,
            exports,
            agents,
        })
    }

    /// Node Link 快照的元数据视图（`NODE_LINK_PROTOCOL.md` §12.4）：会话状态/版本、origin head 与未决
    /// 交互（不含任何正文）。
    pub async fn node_link_session_view(
        &self,
        access_node: &NodeId,
        export: &ExportId,
        session: &SessionId,
    ) -> Result<NodeLinkSessionView, PortError> {
        self.node_link_session_access(access_node, export, session)
            .await?;
        let slice = self
            .store
            .read_view()
            .await?
            .node_link_slice(session, None, ReplayLimit::new(0))
            .await?;
        Ok(NodeLinkSessionView {
            session: slice.summary,
            head: slice.head,
            pending_interactions: slice
                .pending_interactions
                .into_iter()
                .map(|row| NodeLinkPendingInteraction {
                    interaction: row.interaction,
                    origin_event: row.origin_event,
                })
                .collect(),
        })
    }

    /// Node Link 的 origin 游标重放（`NODE_LINK_PROTOCOL.md` §12.4）：`after` 之后的会话事件含正文。
    ///
    /// `after` 为 `None` 时从该会话开头取本批（快照结束点由返回的 `head` 表达）；`after` 的
    /// `origin_epoch` 与该会话当前 epoch 不一致时返回 `InvalidRequest("nodelink.origin_epoch_mismatch")`，
    /// 由适配器映射为 `nodelink.protocol.sequence_invalid`（不从错误位置重放）。
    pub async fn node_link_replay(
        &self,
        access_node: &NodeId,
        export: &ExportId,
        session: &SessionId,
        after: Option<OriginCursor>,
        limit: ReplayLimit,
    ) -> Result<NodeLinkReplay, PortError> {
        self.node_link_session_access(access_node, export, session)
            .await?;
        let slice = self
            .store
            .read_view()
            .await?
            .node_link_slice(session, after.clone(), limit)
            .await?;
        if let Some(cursor) = &after
            && cursor.origin_epoch != slice.head.origin_epoch
        {
            return Err(PortError::InvalidRequest("nodelink.origin_epoch_mismatch"));
        }
        let mut events = Vec::with_capacity(slice.events.len());
        for record in slice.events {
            let origin_epoch = record
                .event
                .origin_epoch
                .ok_or(PortError::Corrupt("session event without an origin epoch"))?;
            let origin_sequence = record.event.origin_sequence.ok_or(PortError::Corrupt(
                "session event without an origin sequence",
            ))?;
            events.push(NodeLinkEvent {
                event_id: record.event.id,
                event_type: record.event.event_type,
                origin_epoch,
                origin_sequence,
                created_at: record.event.created_at,
                payload: record.payload,
            });
        }
        Ok(NodeLinkReplay {
            events,
            head: slice.head,
        })
    }

    /// 某会话内一条事件的持久化正文（事件扇出的正文来源）。
    ///
    /// 与 [`UseCases::node_link_replay`] 同一套归属前置：调用方必须给出该连接 attachment 上的
    /// `(access_node, export, session)`，因此本入口不是按任意 event id 的通用读取。
    pub async fn node_link_event_payload(
        &self,
        access_node: &NodeId,
        export: &ExportId,
        session: &SessionId,
        event: &EventId,
    ) -> Result<Option<EventPayload>, PortError> {
        self.node_link_session_access(access_node, export, session)
            .await?;
        self.store
            .read_view()
            .await?
            .session_event_payload(session, event)
            .await
    }

    /// Node Link 会话读的归属前置：对端是已配对的 `access` 行、Export 存在且未撤销、会话存在且其
    /// Agent 属于该 Export。
    ///
    /// 「这个 Export 是否对该节点可见」（`design.md` D1 的三条件：未撤销 ∧ `scopes ∩ grants ≠ ∅`
    /// ∧ `exportId ∈ node.export_ids`）不在本入口判定：它是适配器的**单点可见性策略**
    /// （`node_link::catalog::visible_exports`），命令授权则由 `Broker::node_allowed` 按同一份信任
    /// 记录判定；core 这里只守住「sessionRef 确实属于该 Export」这条硬底线，避免把
    /// `(exportId, sessionId)` 当成可任意组合的读取钥匙。
    async fn node_link_session_access(
        &self,
        access_node: &NodeId,
        export: &ExportId,
        session: &SessionId,
    ) -> Result<(), PortError> {
        if self
            .trust
            .node(access_node, NodeKind::Access)
            .await?
            .filter(|row| row.state() == NodeState::Paired)
            .is_none()
        {
            return Err(PortError::InvalidRequest("authorization.scope_denied"));
        }
        let Some(record) = self.exports.export(export).await? else {
            return Err(PortError::NotFound(EntityRef::Export(export.clone())));
        };
        let Some(snapshot) = self.store.load(session).await? else {
            return Err(PortError::NotFound(EntityRef::Session(session.clone())));
        };
        let agent = snapshot.session.agent().agent_id();
        if record.is_revoked()
            || !record
                .agent_ids()
                .iter()
                .any(|candidate| candidate == agent)
        {
            return Err(PortError::InvalidRequest("export.not_granted"));
        }
        Ok(())
    }

    /// 配对通道的只读视图（claim/status HTTP 端点；design D12 的 seam 补全）。
    ///
    /// 与 [`UseCases::claim_pairing`]/[`UseCases::pairing`] 同一套访问规则：本机入口，或绑定该配对的
    /// `PairingClaimant`。认领路径允许在 proof 校验**之前**读取（端点必须拿到记录才能校验 HMAC），
    /// 但**不含任何写入**：状态推进仍只能经 `claim_pairing` 的写集，且只在 proof 通过后提交；
    /// 拒绝形状与其它配对通道入口逐字相同（不让该错误变成配对 id 预言机）。
    pub async fn pairing_channel_view(
        &self,
        actor: &Actor,
        id: &PairingId,
    ) -> Result<Option<PairingChannelView>, PortError> {
        require_pairing_access(actor, id)?;
        let Some(record) = self.trust.pairing(id).await? else {
            return Ok(None);
        };
        let peer = self.trust.pairing_peer(id).await?;
        // `grant.*` 只在已批准后的节点行上（§11.2 第 3 条：确认事务创建 `owned_node`）。Node Link 的
        // claim 恒为 `access`（`NODE_LINK_PROTOCOL.md` §13.2 的 `nodeKind` 是常量），因此这里读 `access` 行；
        // 同一对端的 `owner` 行属于另一个方向，不属于本次配对。
        let node = match (
            record.state(),
            peer.as_ref().map(crate::model::PairingPeer::id),
        ) {
            (
                PairingState::Approved | PairingState::Consumed,
                Some(PeerIdentity::Node(node_id)),
            ) => self
                .trust
                .nodes_for(node_id)
                .await?
                .into_iter()
                .find(|row| row.kind() == NodeKind::Access),
            _ => None,
        };
        Ok(Some(PairingChannelView { record, peer, node }))
    }

    /// 已认领的对端行（读回公钥是确认事务的前置输入，§11.5）。
    pub async fn pairing_peer(
        &self,
        actor: &Actor,
        id: &PairingId,
    ) -> Result<Option<crate::model::PairingPeer>, PortError> {
        self.require_local(actor)?;
        self.trust.pairing_peer(id).await
    }

    /// `device.pair.confirm` / `node.pair.confirm`：落定并创建信任记录；批准与拒绝各自的审计与状态同事务。
    ///
    /// 「信任建立」的审计动作按目标族区分（`SECURITY_DESIGN.md` §14.2 的最小集合）：设备是
    /// `pairing.approved`，节点是 `node.paired`——与 `device.revoked`/`node.trust_revoked` 同款对称。
    pub async fn settle_pairing(
        &self,
        actor: &Actor,
        id: &PairingId,
        settlement: PairingSettlement,
    ) -> Result<TrustRecordRef, PortError> {
        self.require_local(actor)?;
        settlement.validate().map_err(PortError::from)?;
        // 目标族从配对行读出：存储层只落库写集携带的审计，不自行决定动作（§11.6 第 2 条）。
        let record = self
            .trust
            .pairing(id)
            .await?
            .ok_or_else(|| PortError::NotFound(EntityRef::Pairing(id.clone())))?;
        let at = self.clock.now();
        let action = match (settlement.is_approved(), record.target()) {
            (false, _) => AuditAction::PairingRejected,
            (true, PairingTarget::Device) => AuditAction::PairingApproved,
            (true, PairingTarget::Node) => AuditAction::NodePaired,
        };
        let audits = vec![self.pending_audit(actor, action, EntityRef::Pairing(id.clone()))];
        self.trust
            .settle_pairing(PairingSettlementWrite {
                pairing: id.clone(),
                settlement,
                context: WriteContext { at, audit: audits },
            })
            .await
    }

    /// 过期扫描：`pairing.expired` 由存储层为每条被终结的配对写入（actor 取 `context.audit` 首条）。
    pub async fn expire_pairings(&self, actor: &Actor) -> Result<u64, PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        self.trust
            .expire_pairings(ExpiryWrite {
                context: WriteContext {
                    at,
                    audit: Vec::new(),
                },
            })
            .await
    }

    pub async fn exports(&self, actor: &Actor) -> Result<Vec<ExportRecord>, PortError> {
        self.require_local(actor)?;
        self.exports.exports().await
    }

    pub async fn export(
        &self,
        actor: &Actor,
        id: &ExportId,
    ) -> Result<Option<ExportRecord>, PortError> {
        self.require_local(actor)?;
        self.exports.export(id).await
    }

    /// `export.create`/`export.update`：Export 与 `export.created` 审计同一事务。
    pub async fn put_export(&self, actor: &Actor, export: ExportRecord) -> Result<(), PortError> {
        self.require_local(actor)?;
        // §11.2 第 4 条 / §5.1：创建前验证 Export 引用的 workspace 与 Agent 都存在于本机
        // （`LOCAL_ADMIN_PROTOCOL.md` §5.5 把它定为 `local.not_found` 类失败，由适配器映射）。
        for entry in export.workspace_aliases() {
            if self.config.workspace(entry.alias()).await?.is_none() {
                return Err(PortError::InvalidRequest(
                    "export references a workspace alias that is not registered on this node",
                ));
            }
        }
        let known_agents = self.catalog.agents().await?;
        if export.agent_ids().iter().any(|agent| {
            !known_agents
                .iter()
                .any(|descriptor| descriptor.agent.agent_id() == agent)
        }) {
            return Err(PortError::InvalidRequest(
                "export references an agent that is not available on this node",
            ));
        }
        let at = self.clock.now();
        let target = EntityRef::Export(export.export_id().clone());
        let audits = vec![self.pending_audit(actor, AuditAction::ExportCreated, target)];
        self.exports
            .put_export(ExportWrite {
                record: export,
                context: WriteContext { at, audit: audits },
            })
            .await
    }

    /// `export.revoke`（`LOCAL_ADMIN_PROTOCOL.md` §5.5）：先提交再发送 `export.revoked`（发送属组合根）。
    pub async fn revoke_export(&self, actor: &Actor, id: &ExportId) -> Result<(), PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        let audits = vec![self.pending_audit(
            actor,
            AuditAction::ExportRevoked,
            EntityRef::Export(id.clone()),
        )];
        self.exports
            .revoke_export(ExportRevocation {
                export: id.clone(),
                context: WriteContext { at, audit: audits },
            })
            .await
    }

    pub async fn imports(&self, actor: &Actor) -> Result<Vec<ImportRecord>, PortError> {
        self.require_local(actor)?;
        self.exports.imports().await
    }

    pub async fn import(
        &self,
        actor: &Actor,
        id: &ImportId,
    ) -> Result<Option<ImportRecord>, PortError> {
        self.require_local(actor)?;
        self.exports.import(id).await
    }

    /// `import.add`：管理行与全部关联行一次提交（§11.2 第 5 条）。
    pub async fn add_import(&self, actor: &Actor, record: ImportRecord) -> Result<(), PortError> {
        self.require_local(actor)?;
        if record.export_ids().is_empty() {
            return Err(PortError::InvalidRequest("import 必须关联至少一个 export"));
        }
        // §11.7 / §11.2 第 5 条：`owner_node_id` 指向 owned 家族的节点记录，存在性与角色由用例层在
        // 写集内校验（`import.add` 的前置条件，`LOCAL_ADMIN_PROTOCOL.md` §5.5：必须是已配对且
        // `kind = owner` 的节点）。
        // 缺席与「存在但角色/状态不对」分开报：缺席是 `NotFound`（适配器映射 `local.not_found`），
        // 其余是参数类错误（§5.5 的 `local.invalid_params` 一侧）。
        match self
            .trust
            .node(record.owner_node_id(), NodeKind::Owner)
            .await?
        {
            Some(node) if node.state() == NodeState::Paired => {}
            Some(_) => {
                return Err(PortError::InvalidRequest(
                    "import owner node must be a paired owner node on this node",
                ));
            }
            None => {
                if !self
                    .trust
                    .nodes_for(record.owner_node_id())
                    .await?
                    .is_empty()
                {
                    return Err(PortError::InvalidRequest(
                        "import owner node must hold the owner role on this node",
                    ));
                }
                return Err(PortError::NotFound(EntityRef::Node(
                    record.owner_node_id().clone(),
                )));
            }
        }
        let at = self.clock.now();
        let exports = record.export_ids().to_vec();
        let audits = vec![self.pending_audit(
            actor,
            AuditAction::ImportAdded,
            EntityRef::Import(record.import_id().clone()),
        )];
        self.exports
            .add_import(ImportWrite {
                record,
                exports,
                context: WriteContext { at, audit: audits },
            })
            .await
    }

    /// `import.remove`：一次调用完成完整移除（管理行 + 关联行 + 交付索引 + 命令引用），审计保留。
    ///
    /// 撤回 v0.6 以前「先 `drop_import` 再 `remove_import`」的两次调用：那要么暴露中间态，
    /// 要么留下 `imported_session` 残留（§11.6）。
    pub async fn remove_import(&self, actor: &Actor, id: &ImportId) -> Result<(), PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        let audits = vec![self.pending_audit(
            actor,
            AuditAction::ImportRemoved,
            EntityRef::Import(id.clone()),
        )];
        self.exports
            .remove_import(ImportRemoval {
                import: id.clone(),
                context: WriteContext { at, audit: audits },
            })
            .await
    }

    // ---------------------------------------------------------------------------------------
    // 本地配置（§11.6 的 LocalConfigStore）
    // ---------------------------------------------------------------------------------------

    /// `agent.list`：本地 Agent profile。
    pub async fn profiles(&self, actor: &Actor) -> Result<Vec<AgentProfile>, PortError> {
        self.require_local(actor)?;
        self.config.profiles().await
    }

    pub async fn profile(
        &self,
        actor: &Actor,
        id: &AgentId,
    ) -> Result<Option<AgentProfile>, PortError> {
        self.require_local(actor)?;
        self.config.profile(id).await
    }

    /// `agent.configure`：写入 profile；切换默认是同一写集（至多一个默认，§7.3 的部分唯一索引）。
    ///
    /// `agent.configure` 不是已登记的安全动作（其凭据经 `provider.configure` 登记），因此审计留空。
    pub async fn put_profile(&self, actor: &Actor, profile: AgentProfile) -> Result<(), PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        self.config
            .put_profile(ProfileWrite {
                profile,
                context: WriteContext {
                    at,
                    audit: Vec::new(),
                },
            })
            .await
    }

    /// `workspace.list`：本机 workspace 记录（路径不进 Node Link catalog）。
    pub async fn workspaces(&self, actor: &Actor) -> Result<Vec<WorkspaceRecord>, PortError> {
        self.require_local(actor)?;
        self.config.workspaces().await
    }

    pub async fn workspace(
        &self,
        actor: &Actor,
        alias: &WorkspaceAlias,
    ) -> Result<Option<WorkspaceRecord>, PortError> {
        self.require_local(actor)?;
        self.config.workspace(alias).await
    }

    /// `workspace.select`：不是已登记的安全动作，因此审计留空（§11.6）。
    pub async fn put_workspace(
        &self,
        actor: &Actor,
        record: WorkspaceRecord,
    ) -> Result<(), PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        self.config
            .put_workspace(WorkspaceWrite {
                record,
                context: WriteContext {
                    at,
                    audit: Vec::new(),
                },
            })
            .await
    }

    /// `provider.list`：只有字段名、引用与版本，没有凭据值。
    pub async fn provider_refs(&self, actor: &Actor) -> Result<Vec<ProviderRef>, PortError> {
        self.require_local(actor)?;
        self.config.provider_refs().await
    }

    /// `provider.configure`：已登记的安全动作，写集必须带 `provider.configured` 审计（§11.6）。
    pub async fn put_provider_ref(
        &self,
        actor: &Actor,
        reference: ProviderRef,
    ) -> Result<(), PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        // `EntityRef` 没有 Provider 变体时无法表达审计目标；本变更新增 `EntityRef::Provider`
        // （§3.1），target 指向 Provider 引用 id，摘要仍由 `detail_digest` 承载。
        let target = EntityRef::Provider(reference.id().to_owned());
        let audits = vec![self.pending_audit(actor, AuditAction::ProviderConfigured, target)];
        self.config
            .put_provider_ref(ProviderRefWrite {
                reference,
                context: WriteContext { at, audit: audits },
            })
            .await
    }

    /// 首次初始化状态（`CONFIG_REFERENCE.md` 的「配置与管理状态的权威」）。
    pub async fn seed_state(&self, actor: &Actor) -> Result<SeedState, PortError> {
        self.require_local(actor)?;
        self.config.seed_state().await
    }

    /// 种子导入与「已初始化」标记同一事务提交（空种子也写标记）。
    pub async fn mark_seeded(
        &self,
        actor: &Actor,
        profiles: Vec<AgentProfile>,
    ) -> Result<(), PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        self.config
            .mark_seeded(SeedWrite {
                profiles,
                context: WriteContext {
                    at,
                    audit: Vec::new(),
                },
            })
            .await
    }

    /// 交付索引存储：`import.remove` 的完整移除走 [`ExportStore::remove_import`]，而连接级清空
    /// （[`RemoteDeliveryStore::drop_import`]）仍由接入层直接调用——两者不是同一件事（§11.6）。
    pub fn deliveries(&self) -> &Arc<dyn RemoteDeliveryStore> {
        &self.deliveries
    }

    /// id 分配器：入站适配器需要为**非命令**的标识取值（例如 Node Link 每条消息的 `messageId`、
    /// `session.create` 会话 id 之外的说明性标识）时经它分配，保持「id 分配收敛到两处权威」（§3.1）。
    /// 它不参与授权，也不暴露任何持久事实。
    pub fn ids(&self) -> &Arc<dyn IdGenerator> {
        &self.ids
    }

    pub async fn audit(
        &self,
        actor: &Actor,
        query: AuditQuery,
    ) -> Result<Vec<AuditRecord>, PortError> {
        self.require_local(actor)?;
        self.audit.query(query).await
    }

    // ---------------------------------------------------------------------------------------
    // 维护入口（存储/附件）
    // ---------------------------------------------------------------------------------------

    pub async fn prune(
        &self,
        actor: &Actor,
        policy: RetentionPolicy,
    ) -> Result<PruneReport, PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        self.store.prune(policy, at).await
    }

    pub async fn store_health(&self, actor: &Actor) -> Result<StoreHealth, PortError> {
        self.require_local(actor)?;
        self.store.health().await
    }

    pub async fn put_attachment(
        &self,
        actor: &Actor,
        bytes: &[u8],
        media_type: &str,
    ) -> Result<AttachmentRef, PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        self.attachments.put(bytes, media_type, at).await
    }

    pub async fn get_attachment(
        &self,
        actor: &Actor,
        id: &AttachmentId,
    ) -> Result<Option<Vec<u8>>, PortError> {
        self.require_local(actor)?;
        self.attachments.get(id).await
    }

    pub async fn link_attachment(
        &self,
        actor: &Actor,
        session: &SessionId,
        attachment: &AttachmentId,
        generation: AttachmentGeneration,
    ) -> Result<(), PortError> {
        self.require_local(actor)?;
        self.attachments.link(session, attachment, generation).await
    }

    pub async fn prune_attachments(
        &self,
        actor: &Actor,
        budget_bytes: u64,
    ) -> Result<PruneReport, PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        self.attachments.prune_lru(budget_bytes, at).await
    }

    // ---------------------------------------------------------------------------------------
    // 内部
    // ---------------------------------------------------------------------------------------

    async fn live_config(&self, session: &SessionId) -> Result<Vec<ConfigOption>, PortError> {
        let reference = SessionReference::Owned(OwnedSessionRef {
            session_id: session.clone(),
        });
        let endpoint = self.broker.endpoint_for(&reference).await?;
        endpoint.list_config().await
    }

    fn require_local(&self, actor: &Actor) -> Result<(), PortError> {
        match actor {
            Actor::LocalCli => Ok(()),
            _ => Err(PortError::InvalidRequest("authorization.scope_denied")),
        }
    }

    /// 写集用的待写审计行：成功结果、首个 `at` 由 [`WriteContext`] 提供。
    fn pending_audit(&self, actor: &Actor, action: AuditAction, target: EntityRef) -> PendingAudit {
        let via_node = match actor {
            Actor::Node { node, .. } => Some(node.clone()),
            _ => None,
        };
        PendingAudit {
            action,
            actor: actor.clone(),
            via_node,
            local_principal_ref: None,
            target,
            outcome: AuditOutcome::Success,
            detail_digest: None,
        }
    }

    /// 独立追加一条审计（没有关联状态变更的场合，§11.2 第 6 条）。
    ///
    /// 管理写集不再用它：审计随 `WriteContext` 与状态同事务提交（§11.6）。
    #[allow(dead_code)] // 保留给尚无关联状态变更的审计入口，后续 server 适配器使用
    async fn audit_action(
        &self,
        actor: &Actor,
        action: AuditAction,
        target: EntityRef,
        at: &Timestamp,
    ) {
        let pending = self.pending_audit(actor, action, target);
        let record = AuditRecord::try_new(
            at.clone(),
            pending.action,
            pending.actor,
            pending.via_node,
            pending.local_principal_ref,
            pending.target,
            pending.outcome,
            pending.detail_digest,
        );
        if let Ok(record) = record {
            let _ = self.audit.append(record).await;
        }
    }
}

fn owned_session(reference: &SessionReference) -> Result<SessionId, PortError> {
    match reference {
        SessionReference::Owned(owned) => Ok(owned.session_id.clone()),
        SessionReference::Remote(_) => Err(PortError::InvalidRequest(
            "imported 会话的正文与交互由 node-link-client 在线回源 Owner",
        )),
    }
}

/// [`ImportedSessionRecord`] → [`SessionSummary`] 投影（`SYNC_PROTOCOL.md` §9.6）。
///
/// 仓库里没有任何 `ImportedSessionRecord → SessionSummary` 的转换函数（`upsert_session` 目前只有测试
/// 调用方，索引行的生产写入方是待落地的 `node-link-client`），本函数是它的**唯一**实现，供
/// [`UseCases::sync_visible_sessions`] 使用。返回 `None` 表示该索引行**还不能**投影成摘要，调用方跳过它，
/// 而不是替它编造字段：
///
/// - `origin_epoch`/`agent`/`state`/`version`/`created_at` 在 `imported_session` 表里可空，而
///   `SessionSummary` 与 §9.4 的 `sessions` 资源都要求这几个字段有值。为 NULL 时恰好没有可投影的内容，
///   而 §3.6 对 `NULL` 的既有口径是「不是错误，但也不得被推导或补齐」：该会话在 Owner 侧的元数据补齐前不
///   出现在结果里，而不是以假值（例如把 `state` 说成 `idle`、把 `created_at` 说成 `updated_at`）出现。
/// - `current_mode` 恒为 `None`：正文类字段不进 Access 的索引（§9.6 的 no-content-cache）。
/// - `workspace` 恒为 `None`：`imported_session` **没有** `workspace_alias` 列（§7.4 的黄金列清单），而
///   §9.6 明确 imported 会话的 `workspace` 固定为 `null`，直到 Owner 侧投影真的带来目录引用为止；Access
///   MUST NOT 用 `owner_node_id`/`export_id`/本机路径或任何其它列推导归属。
///
/// `owner_reachable` 是调用方给出的 Owner 可达性事实（见 [`UseCases::sync_visible_sessions`]）。
///
/// 构造失败（例如索引行里的标题超过 512 字符）按**列值损坏**上抛：那不是「不完整」，而是存储里出现了不满足
/// 值对象约束的数据，静默跳过会让损坏看不见。
fn imported_summary(
    record: &ImportedSessionRecord,
    owner_reachable: &impl Fn(&RemoteSessionRef) -> bool,
) -> Result<Option<SessionSummary>, PortError> {
    let (Some(origin_epoch), Some(agent), Some(state), Some(version), Some(created_at)) = (
        record.origin_epoch.clone(),
        record.agent.clone(),
        record.state,
        record.version,
        record.created_at.clone(),
    ) else {
        return Ok(None);
    };
    let summary = SessionSummary::try_new(
        record.session.session_id.clone(),
        record.title.clone(),
        agent,
        state,
        ResourceOrigin::Remote {
            owner_node_id: record.session.owner_node_id.clone(),
            export_id: record.session.export_id.clone(),
            origin_epoch,
            online: owner_reachable(&record.session),
        },
        None,
        version,
        created_at,
        record.updated_at.clone(),
        None,
    )
    .map_err(PortError::from)?;
    Ok(Some(summary))
}

/// 配对通道（claim/status）的访问判定（design D12）：本机入口恒可；`Actor::PairingClaimant` 只在绑定
/// **同一**配对时可访问。
///
/// 拒绝形状与 [`UseCases::require_local`] 逐字相同（同一个 `authorization.scope_denied`）：不区分
/// 「不是本机入口」与「claimant 绑定了别的配对」，否则该错误会变成可探测的配对 id 预言机。
fn require_pairing_access(actor: &Actor, pairing: &PairingId) -> Result<(), PortError> {
    match actor {
        Actor::LocalCli => Ok(()),
        Actor::PairingClaimant { pairing: bound } if bound == pairing => Ok(()),
        _ => Err(PortError::InvalidRequest("authorization.scope_denied")),
    }
}

/// 配对的对端行与本次认证主体的身份比对（`consume_pairing`）：类别或 id 任一不同都不算一致。
fn peer_matches_actor(peer: &crate::model::PairingPeer, actor: &Actor) -> bool {
    match (peer.id(), actor) {
        (PeerIdentity::Device(id), Actor::Device { device, .. }) => id == device,
        (PeerIdentity::Node(id), Actor::Node { node, .. }) => id == node,
        _ => false,
    }
}

/// 未批准且已终态的配对上的消费请求：`expired` → `Expired`，其余终态 → `Consumed`
/// （与 `storage-sqlite` 的 `terminal_conflict` 同口径，两层对同一请求给出同一个具名分类）。
fn terminal_pairing_conflict(state: PairingState) -> PortError {
    if state == PairingState::Expired {
        PortError::Conflict(ConflictKind::Expired)
    } else {
        PortError::Conflict(ConflictKind::Consumed)
    }
}

fn k_device(id: &DeviceId) -> EntityRef {
    EntityRef::Device(id.clone())
}

fn k_node(id: &NodeId) -> EntityRef {
    EntityRef::Node(id.clone())
}

fn k_pairing(pairing: &PairingRecord) -> EntityRef {
    EntityRef::Pairing(pairing.id().clone())
}

/// alias → 规范化本机绝对路径（§11.9）。
///
/// 只接受绝对路径、存在且为目录的输入，并以 `canonicalize` 的结果（解析 symlink/junction/大小写/
/// `.`与`..`）作为权威值；相对路径与含 `..` 组件的输入一律拒绝。任一失败都属于「本机配置问题」，
/// 统一返回 [`UnavailableKind::IoError`]。
fn resolve_workspace(alias: &WorkspaceAlias, path: &str) -> Result<ResolvedWorkspace, PortError> {
    let io = || PortError::Unavailable(UnavailableKind::IoError);
    let candidate = std::path::Path::new(path);
    if !candidate.is_absolute()
        || candidate
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(io());
    }
    let metadata = std::fs::metadata(candidate).map_err(|_| io())?;
    if !metadata.is_dir() {
        return Err(io());
    }
    let canonical = std::fs::canonicalize(candidate).map_err(|_| io())?;
    let text = canonical.to_str().ok_or_else(io)?;
    ResolvedWorkspace::try_new(alias.clone(), text.to_owned()).map_err(PortError::from)
}

/// 客户端提供的目录 → 已解析的 workspace（**没有别名**，仅 Sync 面）。
///
/// 失败分类是 `sync-session-create`「目录校验与失败分类」的增量要求，与 [`resolve_workspace`]
/// 刻意不同（那里的输入是本机**已登记**的目录，因此任何失败都是本机配置问题）：
///
/// - **形状不合法**（空、相对路径、含 `..` 组件）是**参数类**错误（`InvalidRequest`）且判定顺序
///   在文件系统访问**之前**——形状非法的输入 MUST NOT 触碰文件系统；
/// - **形式合法但解析不了**（不存在、不是目录、`canonicalize` 失败、权限不足）是**服务端不可用类**
///   （[`UnavailableKind::IoError`]），不得降级为参数错误。
///
/// 两类都不创建会话、不启动进程、不回退到任何替代目录。形状校验与 [`create_client_workspace`] 共用
/// 同一实现（[`shape_checked_client_path`]），因此「先拒 `..` 再 `canonicalize`」的顺序在两条路径上
/// 一致，不依赖 `canonicalize` 归一化 `..`（Windows 上它不会）。本函数**不查**别名登记表、也**不登记**
/// 新目录：未登记目录不得因此被接受为别名（`workspace-resolution`），解析结果因此没有别名，该会话呈现为
/// 「未分组」。返回值只带规范化路径，路径本身不得进事件、错误 `details` 或审计前像。
fn resolve_client_workspace(path: &str) -> Result<ResolvedWorkspace, PortError> {
    let candidate = shape_checked_client_path(path)?;
    let io = || PortError::Unavailable(UnavailableKind::IoError);
    let metadata = std::fs::metadata(candidate).map_err(|_| io())?;
    if !metadata.is_dir() {
        return Err(io());
    }
    canonical_client_path(candidate)
}

/// 客户端提供的目录 → 已解析的 workspace，**目录不存在时先创建**（`createIfMissing: true`）。
///
/// 与 [`resolve_client_workspace`] 共用同一条形状校验（[`shape_checked_client_path`]）：形状非法是
/// **参数类**错误且判定在文件系统访问**之前**——开关为真也不例外，形状非法的输入 MUST NOT 触碰文件
/// 系统（否则一次笔误就会在没有校验的情况下落到磁盘上）。
///
/// 形状合法时的顺序（`sync-session-create`「开关为真且目录不存在时先创建」）：
///
/// 1. `create_dir_all`：目录已存在时是幂等成功，缺失的父层一并创建；
/// 2. `canonicalize`：创建结果（或被创建的目录）重新规范化为权威取值。
///
/// 两类失败都归**服务端不可用类**（[`UnavailableKind::IoError`]）：路径上已有成分不是目录（`create_dir_all`
/// 会因它失败，本层**不覆盖、不删除**既有内容）、父层不可达、无写权限，以及随后的规范化失败。任一失败
/// 都 MUST NOT 启动后端、MUST NOT 留会话行，也 MUST NOT 删除已经建出来的目录。
fn create_client_workspace(path: &str) -> Result<ResolvedWorkspace, PortError> {
    let candidate = shape_checked_client_path(path)?;
    let io = || PortError::Unavailable(UnavailableKind::IoError);
    std::fs::create_dir_all(candidate).map_err(|_| io())?;
    canonical_client_path(candidate)
}

/// 客户端提供的目录的形状校验（两种 Path 来源共用）：返回**已通过形状检查**的原样候选路径。
///
/// 形状不合法（空、相对路径、含 `..` 组件）是**参数类**错误（`InvalidRequest`），判定顺序在文件系统
/// 访问**之前**：形状非法的输入 MUST NOT 触碰文件系统。不查别名登记表、也不登记新目录——未登记目录
/// 不得因此被接受为别名（`workspace-resolution`）。
fn shape_checked_client_path(path: &str) -> Result<&std::path::Path, PortError> {
    let candidate = std::path::Path::new(path);
    if path.is_empty()
        || !candidate.is_absolute()
        || candidate
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(PortError::InvalidRequest(
            "the workspace path must be an absolute path without parent references",
        ));
    }
    Ok(candidate)
}

/// 已存在（或刚创建）的目录 → `canonicalize` 的权威取值（两个 Path 来源共用的收尾一步）。
fn canonical_client_path(candidate: &std::path::Path) -> Result<ResolvedWorkspace, PortError> {
    let io = || PortError::Unavailable(UnavailableKind::IoError);
    let canonical = std::fs::canonicalize(candidate).map_err(|_| io())?;
    let text = canonical.to_str().ok_or_else(io)?;
    ResolvedWorkspace::at_path(text.to_owned()).map_err(PortError::from)
}

/// 下发给设备的路径形式（`design.md` D2）：规范化结果**剥掉平台 verbatim 前缀**（`\\?\D:\…` →
/// `D:\…`、`\\?\UNC\server\share\…` → `\\server\share\…`，非 Windows 逐字节直通）。
///
/// 前缀规则只有一处实现（[`crate::strip_verbatim`]，`agent-host` 的出站 ACP 参数也在用）；本函数只把
/// `&str` 与 `PathBuf` 之间来回一趟，不另写前缀判断。**只用于下发形式**：持久化取值与本机解析结果都
/// 保持 `canonicalize` 的原样，客户端回传该形式时本层重新规范化（[`browse_parent`]）。
fn delivered_path(path: &str) -> String {
    crate::strip_verbatim(std::path::PathBuf::from(path))
        .to_string_lossy()
        .into_owned()
}

/// `workspace.browse` 的父路径形状校验与规范化，顺序与既有路径口径**逐条一致**
/// （[`shape_checked_client_path`] → 存在 → 是目录 → `canonicalize`）：
///
/// - 不是绝对路径、含父目录引用或为空 → **参数类**错误（`InvalidRequest`）且**不触碰文件系统**；
/// - 形式合法但不存在、不是目录、`canonicalize` 失败或权限不足 → **服务端不可用类**
///   （[`UnavailableKind::IoError`]）。
///
/// 两类都不返回任何条目，也**不回退**到父层或根列表（spec `sync-workspace-browse`「不回退到替代目录」）。
/// 先拒 `..` 再 `canonicalize` 的顺序是关键：Windows 的 `canonicalize` **不**归一化 `..`，依赖它来
/// 消除回退层级会让 `..` 逃逸被静默接受。
///
/// 返回值是**下发形式**（[`delivered_path`]），调用方只用它列举这一层；结果的 `path` **不**取它——
/// 那是请求原文的逐字回显（[`UseCases::workspace_browse`]）。
fn browse_parent(path: &str) -> Result<String, PortError> {
    let candidate = shape_checked_client_path(path)?;
    let io = || PortError::Unavailable(UnavailableKind::IoError);
    let metadata = std::fs::metadata(candidate).map_err(|_| io())?;
    if !metadata.is_dir() {
        return Err(io());
    }
    let canonical = std::fs::canonicalize(candidate).map_err(|_| io())?;
    let text = canonical.to_str().ok_or_else(io)?;
    Ok(delivered_path(text))
}

/// 浏览结果的有序、有界投影（纯函数，便于单独断言顺序与截断口径）。
///
/// `requested_parent` 是**请求父路径原文**的逐字回显（`None` = 根列表），原样写进结果的 `path`：规范化
/// 只作用于每个条目的下发路径与 `registered`，`path` 的取值不由本函数改写（`SYNC_PROTOCOL.md` §11.5）。
///
/// - **确定顺序**：按名称的 Unicode 码点升序（`str` 的序即 UTF-8 字节序，也就是码点序）；名称相同时
///   按路径升序兜底，使同一层在两次调用中给出相同顺序（spec「顺序确定」）——端口自己的返回顺序因此
///   只影响性能，不影响结果；
/// - **有界**：超过 [`WORKSPACE_BROWSE_ENTRY_LIMIT`] 时只取前 `limit` 条并置 `truncated`，**不**静默
///   丢弃（也不因超限整次失败）；
/// - **只保留两种事实**：名称与绝对路径（+ 是否已登记），不构造任何文件/元数据字段。
fn project_browse(
    requested_parent: Option<String>,
    listings: Vec<DirectoryListing>,
    registered: &[String],
) -> WorkspaceBrowse {
    let mut entries: Vec<WorkspaceBrowseEntry> = listings
        .into_iter()
        .map(|listing| {
            let path = delivered_path(&listing.path);
            let is_registered = registered.contains(&path);
            WorkspaceBrowseEntry {
                name: listing.name,
                path,
                registered: is_registered,
            }
        })
        .collect();
    entries.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.path.cmp(&right.path))
    });
    let truncated = entries.len() > WORKSPACE_BROWSE_ENTRY_LIMIT;
    entries.truncate(WORKSPACE_BROWSE_ENTRY_LIMIT);
    WorkspaceBrowse {
        path: requested_parent,
        entries,
        truncated,
    }
}

/// 随 core 提供的本机文件系统目录浏览实现（[`DirectoryBrowser`] 的默认实现）。
///
/// 它是**默认值**而不是唯一实现：组合根可以用 [`UseCases::with_directory_browser`] 换掉（另一份实现或
/// 测试替身）。三处约束与端口文档逐条对应：
///
/// - 只列**目录**（[`std::fs::DirEntry::file_type`] 判定，不跟随符号链接；文件与其它类型一律跳过）；
/// - 只读（不创建、不删除、不写任何缓存）；
/// - 不可读的一层与不存在的一层对调用方是同一类事实（`Unavailable(IoError)`），不回退到父层。
///
/// 不读系统时间、不读文档外的事实，也不引入任何新依赖（`std::fs` 早已在 `use_cases` 里被使用）。
struct SystemDirectories;

#[async_trait::async_trait]
impl DirectoryBrowser for SystemDirectories {
    async fn roots(&self) -> Result<Vec<DirectoryListing>, PortError> {
        // 平台差异只有一处：Windows 有盘符根，其余平台只有一个 `/`。两者都是「列出这个节点的根」的
        // 直接答案，不猜测只读卷、网络挂载或驱动器类型（`design.md` 的 Non-Goals）。
        let roots: Vec<DirectoryListing> = if cfg!(windows) {
            ('A'..='Z')
                .filter_map(|letter| {
                    let path = format!("{letter}:\\");
                    // 只下发**存在**的盘符根：不存在的盘符（例如没有对应卷的 `E:`）不是这一层的一个条目。
                    std::fs::metadata(&path).ok().map(|_| DirectoryListing {
                        name: format!("{letter}:"),
                        path,
                    })
                })
                .collect()
        } else {
            vec![DirectoryListing {
                name: "/".to_owned(),
                path: "/".to_owned(),
            }]
        };
        Ok(roots)
    }

    async fn directories(&self, parent: &str) -> Result<Vec<DirectoryListing>, PortError> {
        let io = || PortError::Unavailable(UnavailableKind::IoError);
        let entries = std::fs::read_dir(parent).map_err(|_| io())?;
        let mut listings = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|_| io())?;
            // `file_type` 不跟随符号链接：指向目录的链接不被当成目录下发（避免把别处的一棵树暴露成
            // 「这一层有一个子目录」）。
            let file_type = entry.file_type().map_err(|_| io())?;
            if !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path().to_string_lossy().into_owned();
            listings.push(DirectoryListing { name, path });
        }
        Ok(listings)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::broker::test_support::{
        FakeAttachments, FakeDeliveries, FakeExports, FakeLocalConfig, FakeStore, FakeTrust,
        FakeWorld, TestAudit, TestCatalog, TestClock, TestIds, TestPublisher, block_on, digest,
        lock, prompt_command, ts, uuid_text,
    };
    use crate::broker::{Broker, BrokerConfig, BrokerDeps, QueuePolicy};
    use crate::model::{
        AgentId, AgentRef, AgentSessionId, CommandKind, CommandPayload, CommittedEvent,
        DeviceState, EventKind, EventOrigin, NodeId, Nonce, PairingState, PendingEvent, ScopeSet,
        StoredPolicy, ViewJson,
    };
    use crate::ports::{HistoryInclude, IdempotencyRecord, OwnedCommit};

    /// 用例自建临时目录的守卫：析构时尽力删除（正常结束与 panic 展开两条路径都生效）。
    ///
    /// 选 `Deref<Target = Path>` 而不是把路径交回调用方：`&dir`、`dir.join(..)`、`dir.to_str()` 照常
    /// 工作，调用点无需改名；`AsRef<Path>` 让 `std::fs::remove_dir_all(&dir)` 这类泛型入参也直接收。
    struct TempDir(std::path::PathBuf);

    impl std::ops::Deref for TempDir {
        type Target = std::path::Path;

        fn deref(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl AsRef<std::path::Path> for TempDir {
        fn as_ref(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            // 尽力而为，且**不 panic**（展开中 panic 会 abort）：目录可能已被用例自己删掉（`NotFound`
            // 立即返回），Windows 上也可能因句柄释放/扫描瞬时占用而失败——此时重试若干次（与 `app` 测试
            // 的 `TempRoot` 同一口径）。
            for _ in 0..10 {
                match std::fs::remove_dir_all(&self.0) {
                    Ok(()) => return,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(50)),
                }
            }
        }
    }

    /// 在系统临时目录下新建一个测试目录（`name` 必须已含 uuid/pid 等唯一化成分）。
    #[must_use]
    fn temp_dir(name: &str) -> TempDir {
        let path = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create dir");
        TempDir(path)
    }

    struct Fixture {
        use_cases: UseCases,
        world: Arc<FakeWorld>,
        session: SessionId,
    }

    fn fixture() -> Fixture {
        fixture_with_trust(FakeWorld::new(), None)
    }

    /// [`fixture`] 的信任端口可替换版本：`broker::test_support::FakeTrust` 不读写设备行，而本工作包的两个
    /// Sync 设备入口正是 `TrustStore::device`/`put_device` 的生产调用方（见 [`DeviceTrust`]）。
    fn fixture_with_trust(world: Arc<FakeWorld>, trust: Option<Arc<dyn TrustStore>>) -> Fixture {
        let store = Arc::new(FakeStore {
            world: world.clone(),
        });
        // 一个替身同时服务 broker 与用例层：`FakeTrust` 除 `world` 外没有状态，分成两个实例只是历史写法。
        let trust_port: Arc<dyn TrustStore> = match trust {
            Some(port) => port,
            None => Arc::new(FakeTrust {
                world: world.clone(),
            }),
        };
        let broker = Arc::new(Broker::new(
            BrokerDeps {
                store: store.clone(),
                deliveries: Arc::new(FakeDeliveries {
                    world: world.clone(),
                }),
                backends: Arc::new(crate::broker::test_support::FakeBackend {
                    world: world.clone(),
                }),
                exports: Arc::new(FakeExports {
                    world: world.clone(),
                }),
                trust: trust_port.clone(),
                publisher: Arc::new(TestPublisher {
                    world: world.clone(),
                }),
                clock: TestClock::new(),
                ids: Arc::new(TestIds::default()),
                audit: Some(Arc::new(TestAudit {
                    world: world.clone(),
                })),
            },
            BrokerConfig {
                queue_policy: QueuePolicy::Queue,
                max_queued_turns: 16,
                persist_deltas: false,
            },
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
                crate::model::SessionState::Idle,
                ResourceOrigin::Local,
                None,
                Version::from(1),
                crate::broker::test_support::ts(0),
                crate::broker::test_support::ts(0),
                None,
            )
            .expect("session"),
        );
        let use_cases = UseCases::new(UseCaseDeps {
            broker,
            store,
            deliveries: Arc::new(FakeDeliveries {
                world: world.clone(),
            }),
            exports: Arc::new(FakeExports {
                world: world.clone(),
            }),
            trust: trust_port,
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

        Fixture {
            use_cases,
            world,
            session,
        }
    }

    /// 设备用例的装配：信任端口换成 [`DeviceTrust`]，并把替身交给调用方断言 core 产出的写集。
    fn fixture_with_device_trust() -> (Fixture, Arc<DeviceTrust>) {
        let world = FakeWorld::new();
        let trust = Arc::new(DeviceTrust {
            inner: FakeTrust {
                world: world.clone(),
            },
            devices: std::sync::Mutex::new(Vec::new()),
            writes: std::sync::Mutex::new(Vec::new()),
        });
        let dyn_trust: Arc<dyn TrustStore> = trust.clone();
        let fixture = fixture_with_trust(world, Some(dyn_trust));
        (fixture, trust)
    }

    /// 只补**设备行**读写的信任替身：`FakeTrust` 的 `device` 恒返回 `None`、`put_device` 无动作（它服务的
    /// 切片不读写设备行）。其余方法逐条委托给 [`FakeTrust`]，因此行为与默认夹具逐字一致；替身不得比真实
    /// 存储更宽容（同一取舍见 `FakeTrust::record_node_connected`）。
    struct DeviceTrust {
        inner: FakeTrust,
        devices: std::sync::Mutex<Vec<DeviceRecord>>,
        /// 逐次提交的写集：断言 core 产出的**整行**，而不是替身自己算出来的结果。
        writes: std::sync::Mutex<Vec<DeviceWrite>>,
    }

    #[async_trait::async_trait]
    impl TrustStore for DeviceTrust {
        async fn device(&self, id: &DeviceId) -> Result<Option<DeviceRecord>, PortError> {
            Ok(lock(&self.devices)
                .iter()
                .find(|record| record.device_id() == id)
                .cloned())
        }

        async fn devices(&self) -> Result<Vec<DeviceRecord>, PortError> {
            Ok(lock(&self.devices).clone())
        }

        /// 与真实存储同款的两件事：记录本次写集，并复制「撤销优先」的拒绝（`state = revoked` 的记录只能
        /// 经 `revoke_device` 写；复活路径是重新配对）。替身若在这里静默成功，就比真实存储更宽容。
        async fn put_device(&self, write: DeviceWrite) -> Result<(), PortError> {
            let revoked = write.record.state() == DeviceState::Revoked;
            let record = write.record.clone();
            lock(&self.devices).retain(|stored| stored.device_id() != record.device_id());
            lock(&self.devices).push(record);
            lock(&self.writes).push(write);
            if revoked {
                return Err(PortError::InvalidRequest(
                    "use device.revoke to revoke a device",
                ));
            }
            Ok(())
        }

        async fn node(&self, id: &NodeId, kind: NodeKind) -> Result<Option<NodeRecord>, PortError> {
            self.inner.node(id, kind).await
        }

        async fn nodes(&self) -> Result<Vec<NodeRecord>, PortError> {
            self.inner.nodes().await
        }

        async fn nodes_for(&self, id: &NodeId) -> Result<Vec<NodeRecord>, PortError> {
            self.inner.nodes_for(id).await
        }

        async fn peer_key(&self, peer: &PeerIdentity) -> Result<Option<PeerPublicKey>, PortError> {
            self.inner.peer_key(peer).await
        }

        async fn pairing(&self, id: &PairingId) -> Result<Option<PairingRecord>, PortError> {
            self.inner.pairing(id).await
        }

        async fn pairing_peer(
            &self,
            id: &PairingId,
        ) -> Result<Option<crate::model::PairingPeer>, PortError> {
            self.inner.pairing_peer(id).await
        }

        async fn pairing_for(
            &self,
            peer: &PeerIdentity,
        ) -> Result<Option<PairingRecord>, PortError> {
            self.inner.pairing_for(peer).await
        }

        async fn put_node(&self, write: NodeWrite) -> Result<(), PortError> {
            self.inner.put_node(write).await
        }

        async fn revoke_device(&self, write: DeviceRevocation) -> Result<(), PortError> {
            self.inner.revoke_device(write).await
        }

        async fn revoke_node(&self, write: NodeRevocation) -> Result<(), PortError> {
            self.inner.revoke_node(write).await
        }

        async fn create_pairing(&self, write: PairingWrite) -> Result<(), PortError> {
            self.inner.create_pairing(write).await
        }

        async fn claim_pairing(
            &self,
            write: PairingClaimWrite,
        ) -> Result<PairingClaimOutcome, PortError> {
            self.inner.claim_pairing(write).await
        }

        async fn settle_pairing(
            &self,
            write: PairingSettlementWrite,
        ) -> Result<TrustRecordRef, PortError> {
            self.inner.settle_pairing(write).await
        }

        async fn expire_pairings(&self, write: ExpiryWrite) -> Result<u64, PortError> {
            self.inner.expire_pairings(write).await
        }

        async fn consume_pairing(
            &self,
            write: PairingConsumption,
        ) -> Result<PairingRecord, PortError> {
            self.inner.consume_pairing(write).await
        }

        /// 与 `FakeTrust` 同一取舍：显式失败关闭而不是静默成功。
        async fn record_node_connected(&self, _write: NodeConnectedWrite) -> Result<(), PortError> {
            unreachable!("DeviceTrust 只补设备行的读写：Node Link 认证收尾不在本替身范围内")
        }
    }

    /// §4/§6.5：查询命令也走同一个入口，且不产生副作用；`LocalCli` 是本地管理入口，设备方向则没有命令级
    /// 授权判定（`pairing-model`：通过认证即可执行）。
    #[test]
    fn query_command_is_authorized_without_side_effects() {
        let fixture = fixture();
        let command = ClientCommand {
            actor: Actor::LocalCli,
            request: RequestId::new(&uuid_text(950)).expect("uuid"),
            command: "session.list".to_owned(),
            kind: CommandKind::Query,
            session: None,
            expected_version: None,
            request_fingerprint: digest('D'),
            payload: CommandPayload::SessionList {},
        };
        let receipt = block_on(
            fixture
                .use_cases
                .submit_command(&Actor::LocalCli, command.clone()),
        )
        .expect("submit");
        assert!(matches!(
            receipt,
            CommandReceipt::Accepted { turn: None, .. }
        ));
        assert_eq!(fixture.world.commit_count(), 0, "查询不落盘");

        // 设备方向没有命令级授权判定（`pairing-model`）：同一个查询对一台从未被授予任何 scope 的已认证
        // 设备照常被接受，同样不落盘、也不写任何拒绝审计。旧实现会以 `authorization.scope_denied` 拒绝它。
        let device = Actor::Device {
            device: DeviceId::new(&uuid_text(41)).expect("uuid"),
        };
        let accepted =
            block_on(fixture.use_cases.submit_command(&device, command)).expect("submit");
        assert!(matches!(
            accepted,
            CommandReceipt::Accepted { turn: None, .. }
        ));
        assert_eq!(fixture.world.commit_count(), 0, "查询不落盘");
        assert!(fixture.world.audits().is_empty(), "没有拒绝就没有审计行");
    }

    /// §4：`session.list` 返回 core 类型（wire 投影留给适配器）。
    #[test]
    fn list_sessions_returns_core_summaries() {
        let fixture = fixture();
        let summaries = block_on(fixture.use_cases.list_sessions(
            &Actor::LocalCli,
            SessionQuery {
                only: None,
                states: Vec::new(),
                limit: None,
            },
        ))
        .expect("list");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].session_id(), &fixture.session);
        assert_eq!(summaries[0].state(), crate::model::SessionState::Idle);
    }

    /// 只播种一条 imported 索引行（§7.4 的 `imported_session`，无正文）；`None` 的字段就是表里的 NULL。
    fn imported_session_record(
        owner: &NodeId,
        export: &ExportId,
        session: &SessionId,
        complete: bool,
    ) -> ImportedSessionRecord {
        // 半成品行对应真实表里确实可空的列（Owner 侧元数据还没到），不是替身漏填。
        let (origin_epoch, title, agent, state, version, created_at, last_origin_sequence) =
            if complete {
                (
                    Some(OriginEpoch::new(&uuid_text(777)).expect("epoch")),
                    Some("Remote one".to_owned()),
                    Some(
                        AgentRef::try_new(AgentId::new("agent-2").expect("agent id"), "Agent Two")
                            .expect("agent ref"),
                    ),
                    Some(crate::model::SessionState::Running),
                    Some(Version::from(4)),
                    Some(crate::broker::test_support::ts(10)),
                    Some(Sequence::new(3).expect("sequence")),
                )
            } else {
                (None, None, None, None, None, None, None)
            };
        ImportedSessionRecord {
            session: RemoteSessionRef::new(owner.clone(), export.clone(), session.clone()),
            origin_epoch,
            title,
            agent,
            state,
            version,
            created_at,
            last_origin_sequence,
            acked: None,
            attachment: None,
            updated_at: crate::broker::test_support::ts(11),
        }
    }

    /// Sync WP1（任务 1.3）：快照的 `sessions` 资源来源把**自有与已导入合并**后交给设备——自有条目排在
    /// 前、`workspace` 取会话行持久化的别名；imported 条目按调用方传入的 feature 协商结果纳入、`workspace`
    /// 恒为 `None`（表里没有该列，不得推导）、`online` 取调用方传入的可达性事实。设备不再有授权维度，
    /// 因此任何已认证设备都拿得到自己的全部会话（`pairing-model`）。
    #[test]
    fn sync_visible_sessions_merges_owned_and_imported() {
        let fixture = fixture();
        let actor = Actor::Device {
            device: DeviceId::new(&uuid_text(241)).expect("device"),
        };

        // 第二条自有会话：带一条**持久化**的目录别名（`workspace` 的唯一来源）。
        let owned = SessionId::new(&uuid_text(242)).expect("session id");
        let agent = AgentRef::try_new(AgentId::new("agent-1").expect("agent id"), "Agent One")
            .expect("agent ref");
        fixture.world.seed_session(
            crate::model::Session::try_new(
                owned.clone(),
                OwnedSessionRef::new(owned.clone()),
                None,
                agent,
                crate::model::SessionState::Running,
                ResourceOrigin::Local,
                None,
                Version::from(2),
                crate::broker::test_support::ts(0),
                crate::broker::test_support::ts(0),
                None,
            )
            .expect("session"),
        );
        let alias = WorkspaceAlias::new("project").expect("alias");
        lock(&fixture.world.state)
            .workspace_aliases
            .insert(owned.as_str().to_owned(), alias.clone());
        fixture.world.workspaces.lock().expect("lock").push(
            WorkspaceRecord::try_new(
                alias.clone(),
                "Project",
                std::env::temp_dir().to_str().expect("temp dir is utf-8"),
                crate::broker::test_support::ts(0),
                crate::broker::test_support::ts(0),
            )
            .expect("workspace record"),
        );

        // imported 索引：一条完整行 + 一条缺 `agent`/`state`/… 的半成品行（表里这些列可空）。
        let owner = NodeId::new(&uuid_text(243)).expect("owner node");
        let export = ExportId::new("export-1").expect("export id");
        let remote_session = SessionId::new(&uuid_text(244)).expect("session id");
        let incomplete = SessionId::new(&uuid_text(245)).expect("session id");
        let remote = RemoteSessionRef::new(owner.clone(), export.clone(), remote_session.clone());
        lock(&fixture.world.imported_sessions).push(imported_session_record(
            &owner,
            &export,
            &remote_session,
            true,
        ));
        lock(&fixture.world.imported_sessions).push(imported_session_record(
            &owner,
            &export,
            &incomplete,
            false,
        ));

        let query = SyncSessionQuery {
            include_imported: true,
            ..SyncSessionQuery::default()
        };
        let sessions = block_on(fixture.use_cases.sync_visible_sessions(
            &actor,
            query.clone(),
            |reference| reference == &remote,
        ))
        .expect("merged list");

        assert_eq!(
            sessions.len(),
            3,
            "两条自有 + 一条可投影的 imported；半成品行不可投影（缺失字段不得被编造）"
        );
        assert_eq!(
            sessions.last().map(SessionSummary::session_id),
            Some(&remote_session),
            "自有在前、已导入在后"
        );
        // 自有两条的顺序由存储层给出（`FakeStore` 用 HashMap，不保证顺序），因此按集合比较。
        let mut owned_ids: Vec<&str> = sessions[..2]
            .iter()
            .map(|summary| summary.session_id().as_str())
            .collect();
        owned_ids.sort_unstable();
        let mut expected_owned = vec![fixture.session.as_str(), owned.as_str()];
        expected_owned.sort_unstable();
        assert_eq!(owned_ids, expected_owned, "两条自有会话都在结果里");

        let merged_owned = sessions
            .iter()
            .find(|summary| summary.session_id() == &owned)
            .expect("自有条目");
        assert_eq!(
            merged_owned.workspace().map(|workspace| (
                workspace.alias().clone(),
                workspace.display_name().to_owned()
            )),
            Some((alias.clone(), "Project".to_owned())),
            "workspace 取自会话行持久化的别名（展示名按别名关联目录记录）"
        );
        assert!(merged_owned.origin().is_local());

        let merged_remote = sessions
            .iter()
            .find(|summary| summary.session_id() == &remote_session)
            .expect("imported 条目");
        let ResourceOrigin::Remote {
            owner_node_id,
            export_id,
            origin_epoch,
            online,
        } = merged_remote.origin()
        else {
            unreachable!("本层投影的 imported 条目恒为 remote origin");
        };
        assert_eq!(owner_node_id, &owner);
        assert_eq!(export_id, &export);
        assert_eq!(
            origin_epoch,
            &OriginEpoch::new(&uuid_text(777)).expect("epoch"),
            "来源 epoch 来自索引行"
        );
        assert!(*online, "可达性取调用方传入的运行时事实（core 不缓存它）");
        assert_eq!(
            merged_remote.state(),
            crate::model::SessionState::Running,
            "状态取自索引行的对应列"
        );
        assert_eq!(
            merged_remote.version(),
            Version::from(4),
            "版本取自索引行的对应列"
        );
        assert_eq!(
            merged_remote.agent().name(),
            "Agent Two",
            "Agent 引用取自索引行的 agent 列"
        );
        assert_eq!(
            merged_remote.workspace(),
            None,
            "`imported_session` 没有 workspace_alias 列：不得由 owner/export 或本机路径推导归属"
        );
        assert_eq!(merged_remote.current_mode(), None, "索引不落正文类字段");

        // 未协商 `resource.remote-origin.v1`：一行 imported 都不出现（连占位也没有）。
        let gated = block_on(fixture.use_cases.sync_visible_sessions(
            &actor,
            SyncSessionQuery {
                include_imported: false,
                ..query.clone()
            },
            |_| true,
        ))
        .expect("feature 未协商时照常返回自有会话");
        assert_eq!(gated.len(), 2);
        assert!(gated.iter().all(|summary| summary.origin().is_local()));

        // 调用方给出的收窄照常生效（本层只透传，不构成第二个可见性判定点）。
        let narrowed = block_on(fixture.use_cases.sync_visible_sessions(
            &actor,
            SyncSessionQuery {
                owned: SessionQuery {
                    only: Some(vec![owned.clone()]),
                    ..SessionQuery::default()
                },
                include_imported: false,
                ..query.clone()
            },
            |_| true,
        ))
        .expect("收窄");
        assert_eq!(narrowed.len(), 1);
        assert_eq!(narrowed[0].session_id(), &owned);

        // 设备方向不再有按授权的可见性过滤（`pairing-model`）：一台从未被授予任何 scope 的已认证设备
        // 拿到的仍是同一份完整清单（自有 + 已导入），一行不少——这与旧实现「无 scope 即失败关闭」相反，
        // 因此本断言可独立变红。
        let same_device = Actor::Device {
            device: DeviceId::new(&uuid_text(246)).expect("device"),
        };
        let all = block_on(fixture.use_cases.sync_visible_sessions(
            &same_device,
            query.clone(),
            |reference| reference == &remote,
        ))
        .expect("已认证设备必须读得到全部会话");
        let mut all_ids: Vec<&str> = all
            .iter()
            .map(|summary| summary.session_id().as_str())
            .collect();
        let mut sessions_ids: Vec<&str> = sessions
            .iter()
            .map(|summary| summary.session_id().as_str())
            .collect();
        all_ids.sort_unstable();
        sessions_ids.sort_unstable();
        assert_eq!(all_ids, sessions_ids, "设备维度不得再过滤任何会话：{all:?}");
    }

    /// 1.6（Sync 事件读面）：`SYNC_PROTOCOL.md` §10.1 的 `event` 帧所需的字段由
    /// [`UseCases::sync_replay`] 如实带回——**五个** `origin.kind` 原样往返、`origin.deviceId` 只经
    /// `causation` 回查**设备命令行**得到、`origin.nodeId` 只经同一条 `causation` 回查**节点命令行**
    /// 得到（两者互斥，其余来源即便带同一个 causation 也不回填）、`causationRequestId` 原样（含
    /// `None`）；窗口按 `global_sequence` 升序、严格在 `after` 之后、受 `limit` 限制。设备不再有授权维度
    /// （`pairing-model`），因此这一页对任何已认证设备都照常给出；未授权的主体（节点方向）仍失败关闭且
    /// **一次读都不发生**。
    #[test]
    fn sync_replay_roundtrips_origin_and_requires_session_list_scope() {
        let fixture = fixture();
        let device = DeviceId::new(&uuid_text(301)).expect("device");
        let request = RequestId::new(&uuid_text(302)).expect("request");
        let node = NodeId::new(&uuid_text(304)).expect("node");
        let access = NodeId::new(&uuid_text(305)).expect("access node");
        let node_request = RequestId::new(&uuid_text(306)).expect("node request");
        let store = FakeStore {
            world: fixture.world.clone(),
        };
        // 五条事件覆盖五种 `origin.kind`：device 那条由设备命令引起（`causation` = 该命令的
        // requestId）、node 那条由节点命令引起（另一条 requestId）；local_cli 那条**故意**带与 device
        // 相同的 causation，用来固定「非本类不回填身份 id」。
        let events = vec![
            sync_event("agent.message.delta", EventOrigin::Agent, None),
            sync_event("turn.completed", EventOrigin::Device, Some(request.clone())),
            sync_event("storage.integrity_failed", EventOrigin::Daemon, None),
            sync_event(
                "session.mode.changed",
                EventOrigin::LocalCli,
                Some(request.clone()),
            ),
            sync_event(
                "session.created",
                EventOrigin::Node,
                Some(node_request.clone()),
            ),
        ];
        block_on(store.commit(OwnedCommit {
            session: Some(fixture.session.clone()),
            at: ts(1),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events,
            interactions: Vec::new(),
            compacted: Vec::new(),
            // 命令行的唯一落盘入口（`owned_command` 的 actor 就是两个身份字段的来源）：一次提交只带
            // 一条幂等行，因此 node 命令行分第二次提交写入。
            idempotency: Some(IdempotencyRecord {
                actor: Actor::Device {
                    device: device.clone(),
                },
                request: request.clone(),
                command: "session.prompt".to_owned(),
                kind: CommandKind::Mutation,
                session: Some(fixture.session.clone()),
                expected_version: None,
                request_fingerprint: digest('a'),
                accepted_at: ts(1),
            }),
            command_terminal: None,
            origin_epoch: Some(OriginEpoch::new(&uuid_text(303)).expect("origin epoch")),
        }))
        .expect("五条事件 + 一条设备命令行");
        block_on(store.commit(OwnedCommit {
            session: Some(fixture.session.clone()),
            at: ts(2),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: Some(IdempotencyRecord {
                actor: Actor::Node {
                    node: node.clone(),
                    access_node: access.clone(),
                },
                request: node_request.clone(),
                command: "session.create".to_owned(),
                kind: CommandKind::Mutation,
                session: Some(fixture.session.clone()),
                expected_version: None,
                request_fingerprint: digest('b'),
                accepted_at: ts(2),
            }),
            command_terminal: None,
            origin_epoch: Some(OriginEpoch::new(&uuid_text(303)).expect("origin epoch")),
        }))
        .expect("一条节点命令行");

        let actor = Actor::Device {
            device: DeviceId::new(&uuid_text(311)).expect("device"),
        };
        let slice = block_on(
            fixture
                .use_cases
                .sync_replay(&actor, None, ReplayLimit::new(10)),
        )
        .expect("Sync 事件读面");
        assert_eq!(slice.events.len(), 5, "五条事件都在一页里");
        assert_eq!(
            slice
                .events
                .iter()
                .map(|record| record.origin_kind)
                .collect::<Vec<_>>(),
            vec![
                EventOrigin::Agent,
                EventOrigin::Device,
                EventOrigin::Daemon,
                EventOrigin::LocalCli,
                EventOrigin::Node,
            ],
            "五个 origin.kind 原样往返（顺序即 global_sequence 升序）"
        );
        assert_eq!(
            slice.events[1].origin_device_id.as_ref(),
            Some(&device),
            "device 类事件的 origin.deviceId 等于诱发该事件的命令的 actor_id"
        );
        assert_eq!(
            slice.events[4].origin_node_id.as_ref(),
            Some(&node),
            "node 类事件的 origin.nodeId 等于诱发该事件的命令的主 principal（ownerNodeId）"
        );
        assert!(
            slice.events[4].origin_device_id.is_none() && slice.events[1].origin_node_id.is_none(),
            "两个身份字段互斥（§10.1 的配对规则）"
        );
        assert!(
            slice.events.iter().enumerate().all(|(index, record)| {
                match record.origin_kind {
                    EventOrigin::Device => {
                        record.origin_device_id.is_some()
                            && record.origin_node_id.is_none()
                            && index == 1
                    }
                    EventOrigin::Node => {
                        record.origin_node_id.is_some()
                            && record.origin_device_id.is_none()
                            && index == 4
                    }
                    _ => record.origin_device_id.is_none() && record.origin_node_id.is_none(),
                }
            }),
            "两个身份字段只在对应 kind 上非空；其余来源（含带同一 causation 的 local_cli）恒为 None"
        );
        assert_eq!(
            slice.events[0].causation, None,
            "没有命令直接导致的事件 causationRequestId 为 null"
        );
        assert_eq!(
            slice.events[1].causation.as_ref(),
            Some(&request),
            "causation 原样回传"
        );
        assert_eq!(
            slice.events[0].payload.view.as_str(),
            "{\"probe\":\"agent.message.delta\"}",
            "条目同时带该事件的持久化正文"
        );
        assert_eq!(
            slice.head.global_sequence,
            Sequence::new(5).expect("sequence"),
            "head 是这批数据的水位（sync.caught_up 的屏障游标）"
        );

        // 窗口：严格在 `after` 之后、升序、受 `limit` 限制。
        let resumed = block_on(fixture.use_cases.sync_replay(
            &actor,
            Some(GlobalCursor::new(
                slice.head.server_epoch.clone(),
                Sequence::new(2).expect("sequence"),
            )),
            ReplayLimit::new(10),
        ))
        .expect("续读");
        assert_eq!(
            resumed
                .events
                .iter()
                .map(|record| record.event.global_sequence.get())
                .collect::<Vec<_>>(),
            vec![3, 4, 5],
            "after 之后的三条，升序"
        );
        let limited = block_on(
            fixture
                .use_cases
                .sync_replay(&actor, None, ReplayLimit::new(2)),
        )
        .expect("限长");
        assert_eq!(limited.events.len(), 2, "limit 生效");
        assert_eq!(
            limited.events[1].event.global_sequence.get(),
            2,
            "一页取满 limit 条即停，续读由调用方按最后一条的 globalSequence 发起"
        );

        // 授权**先于**读取：未获授权的主体连一次只读视图都不开、一条事件也读不出（不是「读出来再丢掉」）。
        // 设备方向已无授权判定，因此这条守卫的判别力现在落在节点面——设备侧的对应判据是上面的「同一个
        // 读面对任何已认证设备都照常给出这一页」（`pairing-model`）。
        let reads = fixture
            .world
            .read_views
            .load(std::sync::atomic::Ordering::SeqCst);
        let denied = Actor::Node {
            node: NodeId::new(&uuid_text(312)).expect("node"),
            access_node: NodeId::new(&uuid_text(313)).expect("access node"),
        };
        let error = block_on(
            fixture
                .use_cases
                .sync_replay(&denied, None, ReplayLimit::new(10)),
        )
        .expect_err("未授权的节点主体不得读取任何事件");
        assert!(
            matches!(&error, PortError::InvalidRequest(code) if *code == "authorization.scope_denied"),
            "与 sync_visible_sessions/list_sessions 同一授权口径，得到 {error:?}"
        );
        assert_eq!(
            fixture
                .world
                .read_views
                .load(std::sync::atomic::Ordering::SeqCst),
            reads,
            "未授权时不开读视图：一条事件也没读出"
        );
    }

    /// 1.6 的**历史行失败关闭**（`SYNC_PROTOCOL.md` §10.1 的配对规则 + 4.8 的处置要求）：设备/节点类
    /// 事件得不出**唯一**身份时读面返回 `Corrupt`，而不是给 `None`——`{kind:"device", deviceId:null}`
    /// 是 §10.1 禁止发送方产生的形状，`None` 会让投递层无声地违反它。
    ///
    /// 三类「无法归因」各有一条：`causation` 为空、回查不到对应命令行、命令行的 actor 类别不对
    /// （设备事件带的是 CLI 命令）。它们**不是**假想的边角——1.7 之前 agent-host 产出的每条
    /// `EndpointEvent` 都不带 `causation`，broker 又原样落盘，于是升级前写入的设备驱动事件全部落在
    /// 第一类；改写/复用同一 requestId 的事件与命令落在第二、三类。
    #[test]
    fn sync_replay_fails_closed_when_an_identity_event_cannot_be_attributed() {
        /// 一条会话内事件 + 可选的命令行（`None` = 这次提交不带 `owned_command` 行）。
        fn seed(fixture: &Fixture, causation: Option<RequestId>, command: Option<Actor>) {
            block_on(
                FakeStore {
                    world: fixture.world.clone(),
                }
                .commit(OwnedCommit {
                    session: Some(fixture.session.clone()),
                    at: ts(1),
                    expected_version: None,
                    state: None,
                    turns: Vec::new(),
                    events: vec![sync_event("turn.completed", EventOrigin::Device, causation)],
                    interactions: Vec::new(),
                    compacted: Vec::new(),
                    idempotency: command.map(|actor| IdempotencyRecord {
                        actor,
                        request: RequestId::new(&uuid_text(330)).expect("request"),
                        command: "session.prompt".to_owned(),
                        kind: CommandKind::Mutation,
                        session: Some(fixture.session.clone()),
                        expected_version: None,
                        request_fingerprint: digest('a'),
                        accepted_at: ts(1),
                    }),
                    command_terminal: None,
                    origin_epoch: Some(OriginEpoch::new(&uuid_text(321)).expect("origin epoch")),
                }),
            )
            .expect("一条设备事件");
        }

        let authorized = Actor::Device {
            device: DeviceId::new(&uuid_text(324)).expect("device"),
        };

        // ① `causation` 为空：连回查的键都没有。
        let missing_causation = fixture();
        seed(&missing_causation, None, None);

        // ② 命令行不存在：该 requestId 没有对应命令（不在本机历史窗口内，或该命令由别的入口发起）。
        let missing_command = fixture();
        seed(
            &missing_command,
            Some(RequestId::new(&uuid_text(322)).expect("request")),
            None,
        );

        // ③ 命令行存在但 actor 类别不对：设备事件带的是本机 CLI 命令。
        let wrong_actor = fixture();
        seed(
            &wrong_actor,
            Some(RequestId::new(&uuid_text(322)).expect("request")),
            Some(Actor::LocalCli),
        );

        for (label, fixture) in [
            ("causation 为空", missing_causation),
            ("命令不在本机", missing_command),
            ("命令行 actor 类别不对", wrong_actor),
        ] {
            let error = block_on(fixture.use_cases.sync_replay(
                &authorized,
                None,
                ReplayLimit::new(10),
            ))
            .expect_err("无法归因的设备事件必须失败关闭");
            assert!(
                matches!(&error, PortError::Corrupt(_)),
                "{label}：得到 {error:?}（不得给 None 让投递层发出 kind=device/deviceId=null 的帧）"
            );
        }
    }

    /// [`sync_replay_roundtrips_origin_and_requires_session_list_scope`] 的事件素材：正文的 `view` 文本
    /// 带上事件类型，便于断言「条目带的是它自己那条的正文」。
    fn sync_event(
        event_type: &str,
        origin: EventOrigin,
        causation: Option<RequestId>,
    ) -> PendingEvent {
        PendingEvent::new(
            EventKind::State,
            EventType::new(event_type).expect("event type"),
            StoredPolicy::Durable,
            EventPayload::new(
                ViewJson::new(&format!("{{\"probe\":\"{event_type}\"}}")).expect("view json"),
                None,
            ),
            origin,
            None,
            causation,
        )
    }

    /// §4：`session.read` 的活体字段（config/capabilities）由用例层从后端合并。
    #[test]
    fn read_session_merges_live_config_from_the_endpoint() {
        let fixture = fixture();
        let page = block_on(fixture.use_cases.read_session(
            &Actor::LocalCli,
            HistoryQuery {
                session: fixture.session.clone(),
                include: HistoryInclude {
                    messages: true,
                    turns: true,
                    pending_interactions: true,
                    config_options: true,
                    capabilities: true,
                    history: true,
                },
                before_history: None,
                after: None,
                limit: ReplayLimit::default(),
            },
        ))
        .expect("read");
        assert_eq!(page.session.session_id(), &fixture.session);
        assert_eq!(
            fixture.world.config_calls(),
            1,
            "config_options 必须回源后端"
        );
        assert!(page.config.is_empty(), "fake 后端返回空配置");
        assert!(
            page.capabilities.is_some(),
            "capabilities 来自 AgentCatalog"
        );
    }

    /// §4/§6.6：`command.status` 只能查询该 actor 自己提交的命令。
    #[test]
    fn command_status_is_scoped_to_the_submitting_actor() {
        let fixture = fixture();
        fixture
            .world
            .push_script(crate::broker::test_support::Script::new(vec![
                crate::broker::test_support::endpoint_event(
                    crate::model::EventKind::State,
                    "turn.completed",
                    &crate::broker::test_support::turn_view("completed"),
                ),
            ]));
        let actor = Actor::LocalCli;
        let request = RequestId::new(&uuid_text(960)).expect("uuid");
        let command = prompt_command(&actor, &fixture.session, &request, 'E');
        let receipt = block_on(fixture.use_cases.submit_command(&actor, command)).expect("submit");
        assert!(matches!(
            receipt,
            CommandReceipt::Accepted { turn: Some(_), .. }
        ));

        let record = block_on(fixture.use_cases.command_status(&actor, request.clone()))
            .expect("status")
            .expect("自己的命令可见");
        assert_eq!(record.status(), crate::model::CommandStatus::Completed);

        // 幂等键是协议维度的 `(actor, requestId)`：别的设备看不到这条命令。设备方向不再有命令级授权
        // 判定，因此这里的差别只来自**归因**（而不是「未授权即拒绝」）。
        let other = Actor::Device {
            device: DeviceId::new(&uuid_text(42)).expect("uuid"),
        };
        let hidden = block_on(fixture.use_cases.command_status(&other, request)).expect("status");
        assert!(
            hidden.is_none(),
            "别的设备不得看到这条命令，得到 {hidden:?}"
        );
    }

    /// §11.9：别名 → 规范化绝对路径；相对路径、含 `..` 的输入、不存在路径与非目录一律 `IoError`。
    #[test]
    fn workspace_resolution_canonicalizes_and_rejects_invalid_inputs() {
        let alias = WorkspaceAlias::new("repo").expect("alias");
        let root = temp_dir(&format!("acpr-ws-{}", uuid_text(11)));
        let resolved = resolve_workspace(&alias, root.to_str().expect("path")).expect("resolve");
        assert_eq!(resolved.alias(), Some(&alias));
        assert!(std::path::Path::new(resolved.canonical_path()).is_absolute());

        let file = root.join("notes.txt");
        std::fs::write(&file, b"x").expect("write file");
        assert!(matches!(
            resolve_workspace(&alias, file.to_str().expect("path")),
            Err(PortError::Unavailable(UnavailableKind::IoError))
        ));
        assert!(matches!(
            resolve_workspace(&alias, "relative/path"),
            Err(PortError::Unavailable(UnavailableKind::IoError))
        ));
        let separator = std::path::MAIN_SEPARATOR;
        let mut dotted = root.to_str().expect("path").to_owned();
        dotted.push(separator);
        dotted.push_str("..");
        dotted.push(separator);
        dotted.push('x');
        assert!(matches!(
            resolve_workspace(&alias, &dotted),
            Err(PortError::Unavailable(UnavailableKind::IoError))
        ));
        let missing = root.join("gone");
        assert!(matches!(
            resolve_workspace(&alias, missing.to_str().expect("path")),
            Err(PortError::Unavailable(UnavailableKind::IoError))
        ));
        let _ = std::fs::remove_dir_all(&root);
    }

    fn create_request() -> CreateSessionRequest {
        CreateSessionRequest::new(
            crate::model::AgentRef::try_new(
                crate::model::AgentId::new("codex").expect("agent id"),
                "Codex CLI",
            )
            .expect("agent ref"),
            None,
            None,
            ResourceOrigin::Local,
        )
    }

    /// `session.create` 的幂等键（§6 第 6 条）：requestId 由调用方给出，指纹是 ACPR-CJ1 摘要。
    fn create_session_request_id() -> crate::model::RequestId {
        crate::model::RequestId::new(&uuid_text(70)).expect("request id")
    }

    /// 已创建会话的条数（断言「不重复创建」用）。
    fn session_count(fixture: &Fixture) -> usize {
        lock(&fixture.world.state).sessions.len()
    }

    /// 走完一次完整的 `session.create`：受理 → 执行 → **落账**。
    ///
    /// `Prepared` 之后必须结算一次（`sync-session-create` 的时序），结算用适配层会给的形状：
    /// `completed` + 一个**投影结果原文**（core 不解释它的内容，只原样落进终态）。
    /// 返回新建会话的标识。
    fn create_and_settle(
        fixture: &Fixture,
        request: RequestId,
        fingerprint: Digest,
        create: CreateSessionRequest,
        workspace: Option<WorkspaceSource>,
    ) -> crate::model::SessionId {
        let prepared = match block_on(fixture.use_cases.create_session(
            &Actor::LocalCli,
            request.clone(),
            fingerprint,
            create,
            workspace,
        ))
        .expect("create")
        {
            SessionCreateAdmission::Prepared(prepared) => prepared,
            other => panic!("首次受理必须返回 Prepared，得到 {other:?}"),
        };
        let result = crate::model::CommandResult::from_json_text(r#"{"sessionId":"projected"}"#)
            .expect("result object");
        assert!(
            block_on(fixture.use_cases.settle_session_create(
                &Actor::LocalCli,
                &request,
                CommandStatus::Completed,
                Some(result),
                None,
            ))
            .expect("settle"),
            "落账必须真的写入"
        );
        prepared.session
    }

    /// §6 第 20 条 / design D2：ACP 会话标识、创建时规范化 cwd 与目录别名由**落账那一次提交**写进会话行，
    /// 与终态同批——不存在「创建提交 + 紧接着的两列提交」两笔。
    ///
    /// 会话版本因此停在 1（不是 2）：落账提交建行（v1）与终态在同一个批次里完成，此后没有第二次状态提交。
    #[test]
    fn the_settlement_commit_writes_the_recovery_columns_together_with_the_terminal() {
        let fixture = fixture();
        let agent_session_id = AgentSessionId::new("acp-session-1").expect("agent session id");
        fixture.world.set_agent_session_id(agent_session_id.clone());
        let workspace = temp_dir(&format!("acpr-resume-{}-a", std::process::id()));
        let alias = WorkspaceAlias::new("resume").expect("alias");
        let record = WorkspaceRecord::try_new(
            alias.clone(),
            "Resume",
            workspace.to_str().expect("path"),
            crate::broker::test_support::ts(0),
            crate::broker::test_support::ts(0),
        )
        .expect("workspace record");
        block_on(fixture.use_cases.put_workspace(&Actor::LocalCli, record)).expect("put workspace");

        let request = create_session_request_id();
        let created = create_and_settle(
            &fixture,
            request.clone(),
            digest('A'),
            create_request(),
            Some(WorkspaceSource::Alias(alias)),
        );

        let expected = std::fs::canonicalize(&*workspace)
            .expect("canonicalize")
            .to_str()
            .expect("path")
            .to_owned();
        let state = lock(&fixture.world.state);
        let columns = state
            .recoveries
            .get(created.as_str())
            .cloned()
            .expect("三列随落账提交一起落盘");
        assert_eq!(columns.agent_session_id, Some(agent_session_id.clone()));
        assert_eq!(columns.workspace_cwd, Some(expected.clone()));
        assert_eq!(
            state
                .workspace_aliases
                .get(created.as_str())
                .map(WorkspaceAlias::as_str),
            Some("resume"),
            "目录归属的别名与两列同批落盘"
        );
        drop(state);
        assert_eq!(
            fixture
                .world
                .session(&created)
                .expect("会话行")
                .version()
                .get(),
            1,
            "落账提交只建一次行：没有第二次状态提交"
        );

        // 该会话的批次里只有一条终态事件——会话行、三个列与终态出自**同一次提交**。
        let batches: Vec<Vec<String>> = fixture
            .world
            .batches()
            .into_iter()
            .filter(|batch| batch.session.as_deref() == Some(created.as_str()))
            .map(|batch| batch.types)
            .collect();
        assert_eq!(
            batches,
            vec![vec!["command.completed".to_owned()]],
            "落账提交是唯一一次带该会话的提交，且它带着终态事件"
        );

        // 恢复流程读到的就是这两个持久化取值（可读回、不推导）。
        let record = block_on(fixture.use_cases.store.load_recovery(&created))
            .expect("load recovery")
            .expect("恢复数据");
        assert_eq!(record.workspace_cwd.as_deref(), Some(expected.as_str()));
        assert_eq!(record.agent_session_id, Some(agent_session_id));
    }

    /// 登记一个目录记录（`local.workspace.select` 的形状），返回其别名与目录守卫（创建流程要
    /// `canonicalize` 该目录，因此它必须在整个用例期间真实存在）。
    fn register_workspace(
        fixture: &Fixture,
        unique: u64,
        alias_text: &str,
        display: &str,
    ) -> (WorkspaceAlias, TempDir) {
        let alias = WorkspaceAlias::new(alias_text).expect("alias");
        let path = temp_dir(&format!("acpr-ws-{unique}-{}", std::process::id()));
        let record = WorkspaceRecord::try_new(
            alias.clone(),
            display,
            path.to_str().expect("path"),
            crate::broker::test_support::ts(0),
            crate::broker::test_support::ts(0),
        )
        .expect("workspace record");
        block_on(fixture.use_cases.put_workspace(&Actor::LocalCli, record)).expect("put workspace");
        (alias, path)
    }

    /// 一个带目录的会话（登记目录 + 打开后端并落账），返回会话 id、别名与目录守卫。
    ///
    /// 守卫必须交回调用方：恢复路径会重新校验持久化 cwd，目录提前消失就不是「别名不被恢复改写」
    /// 而是「目录已删除」了。
    fn create_session_in(
        fixture: &Fixture,
        unique: u64,
    ) -> (crate::model::SessionId, WorkspaceAlias, TempDir) {
        let (alias, dir) = register_workspace(fixture, unique, "repo", "Repository");
        let session = create_and_settle(
            fixture,
            create_session_request_id(),
            digest('A'),
            create_request(),
            Some(WorkspaceSource::Alias(alias.clone())),
        );
        (session, alias, dir)
    }

    /// 读路径投影出的目录归属：`别名|展示名`（`None` = 未分组）。
    fn projected_workspace(fixture: &Fixture, session: &crate::model::SessionId) -> Option<String> {
        let summaries = block_on(fixture.use_cases.list_sessions(
            &Actor::LocalCli,
            SessionQuery {
                only: None,
                states: Vec::new(),
                limit: None,
            },
        ))
        .expect("list");
        summaries
            .into_iter()
            .find(|summary| summary.session_id() == session)
            .expect("会话在列表里")
            .workspace()
            .map(|workspace| {
                format!(
                    "{}|{}",
                    workspace.alias().as_str(),
                    workspace.display_name()
                )
            })
    }

    /// `design.md` D1/§3.6：创建时把**别名原文**与该次解析得到的规范化路径**一起**写进会话行，二者
    /// 独立保存；归属投影取自持久化的别名（展示名按别名关联目录记录）。
    #[test]
    fn create_session_persists_the_resolved_alias_next_to_the_canonical_path() {
        let fixture = fixture();
        fixture.world.set_agent_session_id(
            crate::model::AgentSessionId::new("acp-session-alias").expect("agent session id"),
        );
        let (created, alias, _dir) = create_session_in(&fixture, 201);

        let state = lock(&fixture.world.state);
        assert_eq!(
            state.workspace_aliases.get(created.as_str()),
            Some(&alias),
            "别名原文按创建时的解析结果落盘"
        );
        assert!(
            state.recoveries[created.as_str()].workspace_cwd.is_some(),
            "路径与别名是同一次提交里的两列，互不推导"
        );
        drop(state);
        assert_eq!(
            projected_workspace(&fixture, &created).as_deref(),
            Some("repo|Repository"),
            "读路径按持久化别名投影展示名"
        );
    }

    /// `design.md` D1/`workspace-resolution`：别名为 `NULL` 就是「未分组」，**不得**按 `workspace_cwd`
    /// 反查别名解析表补齐——即使那条路径当前确实能匹配某个已登记目录。
    #[test]
    fn a_null_workspace_alias_is_never_backfilled_from_the_canonical_path() {
        let fixture = fixture();
        let (_alias, dir) = register_workspace(&fixture, 202, "repo", "Repository");
        let canonical = std::fs::canonicalize(&*dir)
            .expect("canonicalize")
            .to_str()
            .expect("path")
            .to_owned();
        // 只有 cwd、别名列为 `NULL` 的既有行（v6 之前创建的会话就是这个形状）。
        fixture.world.seed_recovery(
            &fixture.session,
            Some(crate::model::AgentSessionId::new("acp-legacy").expect("agent session id")),
            Some(canonical),
        );

        assert_eq!(
            projected_workspace(&fixture, &fixture.session),
            None,
            "路径能匹配到已登记目录也不得补出归属"
        );
    }

    /// `design.md` D1：别名被改指到另一个目录并改名之后，既有会话的归属仍是那个别名、规范化路径列
    /// 保持创建时的原值（展示名随目录记录更新——归属不漂移、路径不被改写）。
    #[test]
    fn repointing_a_workspace_keeps_the_persisted_ownership() {
        let fixture = fixture();
        fixture.world.set_agent_session_id(
            crate::model::AgentSessionId::new("acp-session-repoint").expect("agent session id"),
        );
        let (created, alias, _dir) = create_session_in(&fixture, 203);
        let original_cwd = lock(&fixture.world.state).recoveries[created.as_str()]
            .workspace_cwd
            .clone()
            .expect("cwd");

        let moved = temp_dir(&format!("acpr-ws-moved-{}", std::process::id()));
        block_on(
            fixture.use_cases.put_workspace(
                &Actor::LocalCli,
                WorkspaceRecord::try_new(
                    alias,
                    "Moved",
                    moved.to_str().expect("path"),
                    crate::broker::test_support::ts(1),
                    crate::broker::test_support::ts(1),
                )
                .expect("workspace record"),
            ),
        )
        .expect("put workspace");

        assert_eq!(
            projected_workspace(&fixture, &created).as_deref(),
            Some("repo|Moved"),
            "归属仍是原别名，展示名跟随目录记录"
        );
        assert_eq!(
            lock(&fixture.world.state).recoveries[created.as_str()]
                .workspace_cwd
                .as_deref(),
            Some(original_cwd.as_str()),
            "规范化路径列保持创建时的原值，不被改写"
        );
    }

    /// §3.6/`design.md` D6：恢复路径**不读也不写** `workspace_alias`——恢复输入仍只来自两列窄读取，
    /// 落盘的别名在恢复前后逐字不变。
    #[test]
    fn resume_session_neither_reads_nor_rewrites_the_workspace_alias() {
        let fixture = fixture();
        fixture.world.set_agent_session_id(
            crate::model::AgentSessionId::new("acp-session-resume-alias")
                .expect("agent session id"),
        );
        let (created, _alias, _dir) = create_session_in(&fixture, 204);
        let before = lock(&fixture.world.state).workspace_aliases.clone();

        block_on(fixture.use_cases.resume_session(
            &Actor::LocalCli,
            resume_request_id(90),
            digest('B'),
            created.clone(),
        ))
        .expect("resume");

        assert_eq!(
            lock(&fixture.world.state).workspace_aliases,
            before,
            "恢复不得改写别名列"
        );
        let record = block_on(fixture.use_cases.store.load_recovery(&created))
            .expect("load recovery")
            .expect("恢复数据");
        assert!(record.workspace_cwd.is_some(), "恢复输入仍只来自两列窄读取");
    }

    /// §3.6：`agent_session_id()` 为 `None`（未取得标识）时**不写占位值**——该会话不被当作可恢复会话，
    /// `load_recovery` 因此返回 `None`（`NULL` 就是「没有可用于恢复的数据」）。
    #[test]
    fn create_session_writes_no_recovery_columns_without_an_agent_session_id() {
        let fixture = fixture();
        let request = create_session_request_id();
        let created = create_and_settle(&fixture, request, digest('A'), create_request(), None);

        assert!(
            lock(&fixture.world.state).recoveries.is_empty(),
            "未取得标识时不得写入占位取值"
        );
        assert_eq!(
            fixture
                .world
                .session(&created)
                .expect("会话行")
                .version()
                .get(),
            1,
            "落账提交只建一次行"
        );
        assert!(
            block_on(fixture.use_cases.store.load_recovery(&created))
                .expect("load recovery")
                .is_none(),
            "没有恢复数据（两列都是 NULL）"
        );
    }

    /// 一个已带两列恢复数据的夹具：临时目录 + `agent_session_id`。
    fn resume_fixture(name: &str) -> (Fixture, TempDir, crate::model::AgentSessionId) {
        let fixture = fixture();
        let workspace = temp_dir(&format!("acpr-{name}-{}", std::process::id()));
        let canonical = std::fs::canonicalize(&*workspace)
            .expect("canonicalize")
            .to_str()
            .expect("path")
            .to_owned();
        let agent_session_id = crate::model::AgentSessionId::new("acp-session-9").expect("id");
        fixture.world.seed_recovery(
            &fixture.session,
            Some(agent_session_id.clone()),
            Some(canonical),
        );
        (fixture, workspace, agent_session_id)
    }

    fn resume_request_id(n: u64) -> crate::model::RequestId {
        crate::model::RequestId::new(&uuid_text(80 + n)).expect("request id")
    }

    /// §5.1：`resume_session` 的输入全部取自持久化记录（`agent`/`agent_session_id`/
    /// 持久化 cwd 原文），成功后写 `accepted` + 幂等行（`session` 指向目标会话）并只返回 `SessionId`。
    #[test]
    fn resume_session_uses_the_persisted_values_and_commits_an_accepted_row() {
        let (fixture, _workspace, agent_session_id) = resume_fixture("resume-ok");
        let actor = Actor::LocalCli;
        let request = resume_request_id(1);
        let canonical = lock(&fixture.world.state)
            .recoveries
            .get(fixture.session.as_str())
            .expect("两列")
            .workspace_cwd
            .clone()
            .expect("cwd");

        let resumed = block_on(fixture.use_cases.resume_session(
            &actor,
            request.clone(),
            digest('R'),
            fixture.session.clone(),
        ))
        .expect("resume");
        assert_eq!(resumed, fixture.session, "只返回目标会话标识");

        let received = fixture.world.resume_requests.lock().expect("lock").clone();
        assert_eq!(received.len(), 1, "后端只被调用一次");
        assert_eq!(received[0].agent.agent_id().as_str(), "agent-1");
        assert_eq!(received[0].agent_session_id, agent_session_id);
        assert_eq!(
            received[0].workspace_cwd, canonical,
            "传给后端的就是持久化原文，不按别名重新解析"
        );

        let record = fixture.world.command(&request).expect("幂等行");
        assert_eq!(record.command(), "session.resume");
        assert_eq!(record.status(), CommandStatus::Accepted);
        assert_eq!(record.session(), Some(&fixture.session));
    }

    /// §6 第 6 条：同键重试不重复派发——幂等行已存在时早退，不再调用后端。
    #[test]
    fn resume_session_replay_does_not_spawn_a_second_endpoint() {
        let (fixture, _workspace, _) = resume_fixture("resume-replay");
        let actor = Actor::LocalCli;
        for _ in 0..2 {
            block_on(fixture.use_cases.resume_session(
                &actor,
                resume_request_id(2),
                digest('R'),
                fixture.session.clone(),
            ))
            .expect("resume");
        }
        assert_eq!(
            fixture.world.resume_requests.lock().expect("lock").len(),
            1,
            "同键重试只发生一次副作用"
        );
    }

    /// cwd 复校验失败在**调用后端之前**发生，且是服务端不可用类；
    /// 持久化取值不被改写，也不回退到任何新解析结果。
    #[test]
    fn resume_session_revalidates_the_persisted_directory_before_calling_the_backend() {
        // 目录被删除。
        let (fixture, workspace, _) = resume_fixture("resume-missing");
        drop(workspace);
        let error = block_on(fixture.use_cases.resume_session(
            &Actor::LocalCli,
            resume_request_id(3),
            digest('R'),
            fixture.session.clone(),
        ))
        .expect_err("目录不存在必须失败");
        assert!(matches!(
            error,
            PortError::Unavailable(UnavailableKind::IoError)
        ));
        assert!(
            fixture
                .world
                .resume_requests
                .lock()
                .expect("lock")
                .is_empty(),
            "复校验失败时不得调用后端（不启动进程）"
        );
        assert!(
            fixture.world.command(&resume_request_id(3)).is_none(),
            "提交前失败不留 accepted 行"
        );

        // 规范化结果与持久化取值不同（附加 `.` 的等价路径只能经库被篡改到达）。
        let (fixture, workspace, _) = resume_fixture("resume-dotted");
        let canonical = std::fs::canonicalize(&*workspace)
            .expect("canonicalize")
            .to_str()
            .expect("path")
            .to_owned();
        let dotted = format!("{canonical}{}.", std::path::MAIN_SEPARATOR);
        fixture.world.seed_recovery(
            &fixture.session,
            Some(crate::model::AgentSessionId::new("acp-session-9").expect("id")),
            Some(dotted.clone()),
        );
        let error = block_on(fixture.use_cases.resume_session(
            &Actor::LocalCli,
            resume_request_id(4),
            digest('R'),
            fixture.session.clone(),
        ))
        .expect_err("未规范化的持久化值必须被拒");
        assert!(matches!(
            error,
            PortError::Unavailable(UnavailableKind::IoError)
        ));
        assert!(
            fixture
                .world
                .resume_requests
                .lock()
                .expect("lock")
                .is_empty()
        );
        assert_eq!(
            lock(&fixture.world.state)
                .recoveries
                .get(fixture.session.as_str())
                .expect("两列")
                .workspace_cwd,
            Some(dotted),
            "持久化取值不得被改写"
        );

        // 取值无法通过值对象构造（这里用含 NUL 的篡改值）：服务端不可用类，不是「不支持」。
        let (fixture, _workspace, _) = resume_fixture("resume-nul");
        fixture.world.seed_recovery(
            &fixture.session,
            Some(crate::model::AgentSessionId::new("acp-session-9").expect("id")),
            Some("C:\\work\0x".to_owned()),
        );
        let error = block_on(fixture.use_cases.resume_session(
            &Actor::LocalCli,
            resume_request_id(5),
            digest('R'),
            fixture.session.clone(),
        ))
        .expect_err("非法持久化取值必须被拒");
        assert!(matches!(
            error,
            PortError::Unavailable(UnavailableKind::IoError)
        ));
    }

    /// §3.6：两列为 `NULL`（没有可用恢复数据）时与「能力不支持」同一条路径——
    /// `BackendUnsupported`、**不启动进程**、不降级为新建会话。
    #[test]
    fn resume_session_reports_missing_recovery_data_as_unsupported() {
        let fixture = fixture();
        let request = resume_request_id(6);
        let error = block_on(fixture.use_cases.resume_session(
            &Actor::LocalCli,
            request.clone(),
            digest('R'),
            fixture.session.clone(),
        ))
        .expect_err("两列 NULL 必须显式失败");
        assert!(matches!(
            error,
            PortError::Unavailable(UnavailableKind::BackendUnsupported)
        ));
        assert!(
            fixture
                .world
                .resume_requests
                .lock()
                .expect("lock")
                .is_empty(),
            "不得启动进程"
        );
        assert_eq!(
            session_count(&fixture),
            1,
            "不得降级为新建会话（会话数不变）"
        );
        assert!(fixture.world.command(&request).is_none());
    }

    /// §5.1：授权先于一切本机读取——未授权时既不读会话行也不触碰文件系统，且与会话是否存在
    /// 不可区分。设备方向已无授权判定（`pairing-model`），因此这条守卫的判别力落在节点面。
    #[test]
    fn resume_session_authorizes_before_reading_the_session_row() {
        let (fixture, _workspace, _) = resume_fixture("resume-denied");
        let node = Actor::Node {
            node: crate::model::NodeId::new(&uuid_text(52)).expect("node"),
            access_node: crate::model::NodeId::new(&uuid_text(53)).expect("access node"),
        };
        let error = block_on(fixture.use_cases.resume_session(
            &node,
            resume_request_id(7),
            digest('R'),
            fixture.session.clone(),
        ))
        .expect_err("越权恢复必须被拒");
        assert!(
            matches!(
                error,
                PortError::InvalidRequest("authorization.scope_denied")
            ),
            "得到 {error:?}"
        );
        assert_eq!(
            fixture.world.recovery_read_count(),
            0,
            "拒绝不得读取该会话行（因此也无法区分会话是否存在）"
        );
        assert!(
            fixture
                .world
                .resume_requests
                .lock()
                .expect("lock")
                .is_empty()
        );
    }

    /// §5.1：后端报告「不支持」（Agent 未宣告能力）时原样上抛，且 `accepted` 行仍在——
    /// 由适配层结 `failed` 终态（`command.uncertain` 只留给「已 accepted、尚未确认副作用」的窗口）。
    #[test]
    fn resume_session_propagates_the_backend_unsupported_error() {
        let (fixture, _workspace, _) = resume_fixture("resume-unsupported");
        fixture
            .world
            .resume_errors
            .lock()
            .expect("lock")
            .push_back(PortError::Unavailable(UnavailableKind::BackendUnsupported));
        let request = resume_request_id(8);
        let error = block_on(fixture.use_cases.resume_session(
            &Actor::LocalCli,
            request.clone(),
            digest('R'),
            fixture.session.clone(),
        ))
        .expect_err("后端不支持必须上抛");
        assert!(matches!(
            error,
            PortError::Unavailable(UnavailableKind::BackendUnsupported)
        ));
        assert_eq!(
            fixture.world.command(&request).expect("幂等行").status(),
            CommandStatus::Accepted,
            "先 accepted 再工作：后端失败时行仍为 accepted，等适配层结终态"
        );
        // 终态由适配层结：结过之后不再覆盖。
        let result = crate::model::CommandResult::from_json_text(r#"{"sessionId":"acp-9"}"#)
            .expect("result object");
        assert!(
            block_on(fixture.use_cases.settle_session_resume(
                &Actor::LocalCli,
                &request,
                CommandStatus::Completed,
                Some(result),
                None,
            ))
            .expect("settle")
        );
        assert_eq!(
            fixture.world.command(&request).expect("终态行").status(),
            CommandStatus::Completed
        );
    }

    /// §6 第 6/20 条：`session.create` 的幂等键是 `(actor, requestId)`，**落盘**在
    /// `owned_command` 里；同键重试回首次结果且不重复派发创建，同键不同语义是 `idempotency_conflict`。
    ///
    /// 三个时点的语义各不同（这也是本变更的 core 侧结论）：
    /// ① 首次受理且后端已建立 → `Prepared`；
    /// ② **窗口期**（受理已落盘、尚未落账）重发 → `InFlight`：回受理态，**不再派发第二次创建**；
    /// ③ 已终结后重发 → `Settled`：回首次终态，不覆盖。
    #[test]
    fn session_create_replay_does_not_create_a_second_session() {
        let fixture = fixture();
        let actor = Actor::LocalCli;
        let request = create_session_request_id();
        let fingerprint = digest('A');
        let before = session_count(&fixture);

        let prepared = match block_on(fixture.use_cases.create_session(
            &actor,
            request.clone(),
            fingerprint.clone(),
            create_request(),
            None,
        ))
        .expect("create")
        {
            SessionCreateAdmission::Prepared(prepared) => prepared,
            other => panic!("首次受理必须返回 Prepared，得到 {other:?}"),
        };
        assert_eq!(fixture.world.create_calls(), 1, "首次受理派发一次创建");

        // ② 窗口期重发：受理态，不再派发第二次创建，也不建第二行。
        assert_eq!(
            block_on(fixture.use_cases.create_session(
                &actor,
                request.clone(),
                fingerprint.clone(),
                create_request(),
                None,
            ))
            .expect("in-flight replay"),
            SessionCreateAdmission::InFlight,
            "窗口期的同键重发必须回受理态"
        );
        assert_eq!(fixture.world.create_calls(), 1, "不得派发第二次创建");
        assert_eq!(session_count(&fixture), before, "落账之前没有会话行");

        // 落账。
        let result = crate::model::CommandResult::from_json_text(r#"{"sessionId":"first"}"#)
            .expect("result object");
        assert!(
            block_on(fixture.use_cases.settle_session_create(
                &actor,
                &request,
                CommandStatus::Completed,
                Some(result.clone()),
                None,
            ))
            .expect("settle")
        );
        assert_eq!(session_count(&fixture), before + 1, "落账创建一个会话");

        // ③ 已终结后重发：回首次结果，不覆盖、不重复创建。
        let replayed = block_on(fixture.use_cases.create_session(
            &actor,
            request.clone(),
            fingerprint.clone(),
            create_request(),
            None,
        ))
        .expect("settled replay");
        match replayed {
            SessionCreateAdmission::Settled(record) => {
                assert_eq!(record.status(), CommandStatus::Completed);
                assert_eq!(record.session(), Some(&prepared.session));
                assert_eq!(
                    record.result().map(crate::model::CommandResult::as_str),
                    Some(r#"{"sessionId":"first"}"#)
                );
            }
            other => panic!("已终结的同键重发必须回首次终态，得到 {other:?}"),
        }
        assert_eq!(fixture.world.create_calls(), 1, "重发不得再派发创建");
        assert_eq!(session_count(&fixture), before + 1, "重试不得重复创建");

        // 同键不同语义：`command.idempotency_conflict`，同样零副作用。
        let conflict = block_on(fixture.use_cases.create_session(
            &actor,
            request,
            digest('B'),
            create_request(),
            None,
        ))
        .expect_err("同键不同指纹必须冲突");
        assert!(
            matches!(
                conflict,
                PortError::Conflict(ConflictKind::IdempotencyConflict)
            ),
            "得到 {conflict:?}"
        );
        assert_eq!(session_count(&fixture), before + 1);
        assert_eq!(fixture.world.create_calls(), 1);
    }

    /// §6 第 20 条（`sync-session-create`「受理不创建会话行」）：受理只落幂等记录——后端调用之前的
    /// 每一刻，本机都**查不到**这个会话；落账之后它才作为一行出现（并带上 ACP 会话标识与目录归属）。
    #[test]
    fn the_admission_leaves_no_session_row_until_the_settlement() {
        let fixture = fixture();
        let actor = Actor::LocalCli;
        let request = create_session_request_id();
        let agent_session_id =
            AgentSessionId::new("acp-session-admission").expect("agent session id");
        fixture.world.set_agent_session_id(agent_session_id.clone());
        let before = session_count(&fixture);

        let prepared = match block_on(fixture.use_cases.create_session(
            &actor,
            request.clone(),
            digest('A'),
            create_request(),
            None,
        ))
        .expect("create")
        {
            SessionCreateAdmission::Prepared(prepared) => prepared,
            other => panic!("首次受理必须返回 Prepared，得到 {other:?}"),
        };
        let session = prepared.session.clone();

        // 受理 + 执行之后、落账之前：幂等行在（`session_id` 为空），会话行**不在**。
        let accepted = block_on(fixture.use_cases.command_status(&actor, request.clone()))
            .expect("status")
            .expect("受理记录已落盘");
        assert_eq!(accepted.status(), CommandStatus::Accepted);
        assert_eq!(accepted.session(), None, "受理行的 session_id 必须为空");
        assert_eq!(accepted.terminal_event(), None);
        assert_eq!(session_count(&fixture), before, "受理不得创建会话行");
        assert!(
            block_on(fixture.use_cases.store.load(&session))
                .expect("load")
                .is_none(),
            "受理之后本机查不到该会话"
        );
        assert!(
            !block_on(fixture.use_cases.list_sessions(
                &actor,
                SessionQuery {
                    only: None,
                    states: Vec::new(),
                    limit: None,
                },
            ))
            .expect("list")
            .iter()
            .any(|summary| summary.session_id() == &session),
            "会话清单里也不得有它"
        );

        // 落账（终态与行同批）。
        let result = crate::model::CommandResult::from_json_text(r#"{"sessionId":"projected"}"#)
            .expect("result object");
        assert!(
            block_on(fixture.use_cases.settle_session_create(
                &actor,
                &request,
                CommandStatus::Completed,
                Some(result),
                None,
            ))
            .expect("settle")
        );

        assert_eq!(session_count(&fixture), before + 1);
        assert!(
            block_on(fixture.use_cases.store.load(&session))
                .expect("load")
                .is_some(),
            "落账之后会话行才出现"
        );
        let record = block_on(fixture.use_cases.command_status(&actor, request))
            .expect("status")
            .expect("记录");
        assert_eq!(record.status(), CommandStatus::Completed);
        assert_eq!(record.session(), Some(&session), "终态回填了会话标识");
        assert!(
            record.terminal_event().is_some(),
            "completed 必须有终态事件"
        );
    }

    /// §6 第 20 条（`sync-session-create`「可证明未产生会话时以 failed 终结且零会话行」）：后端失败时
    /// **零会话行**，命令必须到达终态。
    #[test]
    fn a_provable_backend_failure_terminates_as_failed_with_no_session_row() {
        let fixture = fixture();
        let actor = Actor::LocalCli;
        let request = create_session_request_id();
        let before = session_count(&fixture);
        fixture.world.push_create_error(PortError::Unavailable(
            UnavailableKind::SessionNotEstablished,
        ));

        let error = block_on(fixture.use_cases.create_session(
            &actor,
            request.clone(),
            digest('A'),
            create_request(),
            None,
        ))
        .expect_err("后端未能建立会话时必须失败");
        assert!(
            matches!(
                error,
                PortError::Unavailable(UnavailableKind::SessionNotEstablished)
            ),
            "原错误原样上抛给适配层，得到 {error:?}"
        );

        assert_eq!(session_count(&fixture), before, "失败零会话行");
        let record = block_on(fixture.use_cases.command_status(&actor, request))
            .expect("status")
            .expect("终态已落盘");
        assert_eq!(record.status(), CommandStatus::Failed);
        assert_eq!(record.session(), None);
        assert!(
            record.terminal_event().is_some(),
            "failed 必须有终态事件（不得停在只有受理记录的状态）"
        );
        assert_eq!(
            record.error().map(crate::model::PublicError::code),
            Some("internal.unavailable"),
            "可证明失败复用既有错误码（design D4 不新增码）"
        );
    }

    /// §6 第 20 条（`sync-session-create`「无法证明时以 uncertain 终结」）：短请求超时这类说不清的失败
    /// 结 `uncertain` 并标记可能已到达 Agent，同样**零会话行**。
    #[test]
    fn an_unprovable_backend_failure_terminates_as_uncertain_with_no_session_row() {
        let fixture = fixture();
        let actor = Actor::LocalCli;
        let request = create_session_request_id();
        let before = session_count(&fixture);
        fixture
            .world
            .push_create_error(PortError::Unavailable(UnavailableKind::Busy));

        block_on(fixture.use_cases.create_session(
            &actor,
            request.clone(),
            digest('A'),
            create_request(),
            None,
        ))
        .expect_err("短请求超时必须失败");

        assert_eq!(session_count(&fixture), before, "失败零会话行");
        let record = block_on(fixture.use_cases.command_status(&actor, request))
            .expect("status")
            .expect("终态已落盘");
        assert_eq!(record.status(), CommandStatus::Uncertain);
        assert_eq!(record.session(), None);
        assert_eq!(
            record.error().map(crate::model::PublicError::code),
            Some("command.uncertain"),
            "说不清的成因由终态的 message 承担，码取 core 的「无法确认」"
        );
        assert!(
            record
                .error()
                .is_some_and(|error| !error.message().is_empty()),
            "成因必须可读"
        );
    }

    /// §6 第 16/20 条：**新的**崩溃窗口形状——受理已落盘、会话行从未落盘（后端已建立、落账前崩溃）。
    /// 启动恢复把它终结为 `uncertain`，且本机**没有**该会话的会话行。
    ///
    /// 旧口径里的「恢复走有会话分支」因此不再成立（`CORE_PORTS` §6 第 20 条同步改写）。
    #[test]
    fn the_crash_window_recovers_as_uncertain_without_a_session_row() {
        let fixture = fixture();
        let actor = Actor::LocalCli;
        let request = create_session_request_id();
        let before = session_count(&fixture);
        let prepared = match block_on(fixture.use_cases.create_session(
            &actor,
            request.clone(),
            digest('A'),
            create_request(),
            None,
        ))
        .expect("create")
        {
            SessionCreateAdmission::Prepared(prepared) => prepared,
            other => panic!("首次受理必须返回 Prepared，得到 {other:?}"),
        };
        // 崩溃：`settle_session_create` 从未被调用，行仍是 accepted 且 `session_id` 仍为空
        // ——这正是重启时的现场。
        let accepted = block_on(fixture.use_cases.command_status(&actor, request.clone()))
            .expect("status")
            .expect("记录已落盘");
        assert_eq!(accepted.status(), CommandStatus::Accepted);
        assert_eq!(accepted.session(), None);

        let recovered = block_on(
            fixture
                .use_cases
                .recover_unsettled(&Actor::LocalCli, ReplayLimit::new(16)),
        )
        .expect("recover");
        assert_eq!(recovered, 1, "崩溃窗口里的创建命令必须被终结");
        let record = block_on(fixture.use_cases.command_status(&actor, request.clone()))
            .expect("status")
            .expect("记录");
        assert_eq!(record.status(), CommandStatus::Uncertain);
        assert!(
            record.terminal_event().is_some(),
            "恢复必须写持久终态事件（terminalEventId 的来源）"
        );
        assert_eq!(
            record.error().map(crate::model::PublicError::code),
            Some("command.uncertain")
        );
        assert_eq!(session_count(&fixture), before, "崩溃窗口不留会话行");
        assert!(
            block_on(fixture.use_cases.store.load(&prepared.session))
                .expect("load")
                .is_none(),
            "本机没有该会话的会话行"
        );

        // 重试：不再派发创建，回首次终态（不重建、不覆盖）。
        let replayed = block_on(fixture.use_cases.create_session(
            &actor,
            request.clone(),
            digest('A'),
            create_request(),
            None,
        ))
        .expect("replay");
        match replayed {
            SessionCreateAdmission::Settled(record) => {
                assert_eq!(record.status(), CommandStatus::Uncertain)
            }
            other => panic!("崩溃窗口终结之后的重发必须回首次终态，得到 {other:?}"),
        }
        assert_eq!(
            fixture.world.create_calls(),
            1,
            "崩溃窗口之后的重试不得再派发创建"
        );
        assert_eq!(session_count(&fixture), before);
        // 终态不被重试覆盖。
        let after = block_on(fixture.use_cases.command_status(&actor, request))
            .expect("status")
            .expect("记录");
        assert_eq!(after.status(), CommandStatus::Uncertain);
    }

    /// §6 第 20 条：终态提交把结果写进持久记录（`terminalEventId` 非空），且**不覆盖**已终结的记录。
    #[test]
    fn settle_session_create_persists_the_terminal_result() {
        let fixture = fixture();
        let actor = Actor::LocalCli;
        let request = create_session_request_id();
        let prepared = match block_on(fixture.use_cases.create_session(
            &actor,
            request.clone(),
            digest('A'),
            create_request(),
            None,
        ))
        .expect("create")
        {
            SessionCreateAdmission::Prepared(prepared) => prepared,
            other => panic!("首次受理必须返回 Prepared，得到 {other:?}"),
        };

        let result = crate::model::CommandResult::from_json_text(r#"{"sessionId":"first"}"#)
            .expect("result object");
        let written = block_on(fixture.use_cases.settle_session_create(
            &actor,
            &request,
            CommandStatus::Completed,
            Some(result),
            None,
        ))
        .expect("settle");
        assert!(written, "第一次终结必须真的写入");
        let record = block_on(fixture.use_cases.command_status(&actor, request.clone()))
            .expect("status")
            .expect("记录");
        assert_eq!(record.status(), CommandStatus::Completed);
        assert_eq!(record.session(), Some(&prepared.session));
        assert!(
            record.terminal_event().is_some(),
            "completed 必须有终态事件"
        );
        assert_eq!(
            record.result().map(crate::model::CommandResult::as_str),
            Some(r#"{"sessionId":"first"}"#)
        );

        // 已终结的记录不被第二次 settle 覆盖（首次结果优先）。
        let overwrite = crate::model::CommandResult::from_json_text(r#"{"sessionId":"second"}"#)
            .expect("result object");
        let rewritten = block_on(fixture.use_cases.settle_session_create(
            &actor,
            &request,
            CommandStatus::Completed,
            Some(overwrite),
            None,
        ))
        .expect("settle");
        assert!(!rewritten, "已终结的记录不覆盖");
        let after = block_on(fixture.use_cases.command_status(&actor, request))
            .expect("status")
            .expect("记录");
        assert_eq!(
            after.result().map(crate::model::CommandResult::as_str),
            Some(r#"{"sessionId":"first"}"#)
        );
    }

    /// §11.9：未登记的别名是参数类错误，且不得触达后端。
    #[test]
    fn create_session_rejects_unregistered_workspace_alias() {
        let fixture = fixture();
        let error = block_on(fixture.use_cases.create_session(
            &Actor::LocalCli,
            create_session_request_id(),
            digest('A'),
            create_request(),
            Some(WorkspaceSource::Alias(
                WorkspaceAlias::new("ghost").expect("alias"),
            )),
        ))
        .expect_err("unregistered alias must be rejected");
        assert!(matches!(error, PortError::InvalidRequest(_)));
    }

    /// §11.9：已登记但本机解析失败是 `Unavailable(IoError)`，**不得**降级为参数错误。
    #[test]
    fn create_session_reports_local_resolution_failure_as_unavailable() {
        let fixture = fixture();
        let alias = WorkspaceAlias::new("ghost").expect("alias");
        let missing = std::env::temp_dir().join(format!("acpr-missing-{}", uuid_text(12)));
        let record = WorkspaceRecord::try_new(
            alias.clone(),
            "Ghost",
            missing.to_str().expect("path"),
            crate::broker::test_support::ts(0),
            crate::broker::test_support::ts(0),
        )
        .expect("workspace record");
        block_on(fixture.use_cases.put_workspace(&Actor::LocalCli, record)).expect("put workspace");

        let error = block_on(fixture.use_cases.create_session(
            &Actor::LocalCli,
            create_session_request_id(),
            digest('A'),
            create_request(),
            Some(WorkspaceSource::Alias(alias)),
        ))
        .expect_err("a missing workspace directory must fail");
        assert!(matches!(
            error,
            PortError::Unavailable(UnavailableKind::IoError)
        ));
    }

    /// Sync 面的目录来源分类（`sync-session-create`「目录校验与失败分类」）：**形状**不合法是参数类
    /// 错误且不触碰文件系统（用一个**不存在**但形状合法的路径反证「先形状、后 IO」的顺序——它得到
    /// 的是不可用类而不是参数类），形式合法但解析不了才是服务端不可用类。
    #[test]
    fn resolve_client_workspace_classifies_shape_and_resolution_failures_apart() {
        // 形状不合法：相对路径、含 `..`、空串——都在文件系统访问之前拒绝，因此与路径是否存在无关。
        for shape in ["relative/repo", "../repo", ""] {
            assert!(
                matches!(
                    resolve_client_workspace(shape),
                    Err(PortError::InvalidRequest(_))
                ),
                "{shape} 必须以参数类错误拒绝"
            );
        }
        // 含 `..` 的**绝对**路径同样属于形状错误（不能靠「先看存在性」蒙混过去：它确实存在）。
        let parent_ref = std::env::temp_dir().join("sub").join("..");
        assert!(matches!(
            resolve_client_workspace(parent_ref.to_str().expect("path")),
            Err(PortError::InvalidRequest(_))
        ));
        // 形式合法但不存在：不可用类（若实现顺序颠倒成「先 metadata」，上面那条断言仍会绿，而这条会红）。
        let missing = std::env::temp_dir().join(format!("acpr-absent-{}", uuid_text(13)));
        assert!(matches!(
            resolve_client_workspace(missing.to_str().expect("path")),
            Err(PortError::Unavailable(UnavailableKind::IoError))
        ));
        // 形式合法但是文件而不是目录：同样是不可用类。
        let file = std::env::temp_dir().join(format!("acpr-file-{}", uuid_text(14)));
        std::fs::write(&file, b"not a directory").expect("write file");
        assert!(matches!(
            resolve_client_workspace(file.to_str().expect("path")),
            Err(PortError::Unavailable(UnavailableKind::IoError))
        ));
        let _ = std::fs::remove_file(&file);
        // 形式合法且是目录：解析成功，且**没有别名**（未登记目录不得因此被接受为别名）。
        let dir = temp_dir(&format!("acpr-client-path-{}", std::process::id()));
        let resolved = resolve_client_workspace(dir.to_str().expect("path")).expect("resolve");
        assert!(resolved.alias().is_none());
        assert!(std::path::Path::new(resolved.canonical_path()).is_absolute());
        let _ = std::fs::remove_dir_all(&*dir);
    }

    /// Sync 面的目录来源**不落盘别名**（`workspace-resolution`：别名只在解析出别名时写入）：会话行
    /// 的规范化路径照常落盘，归属则呈现为「未分组」，且目录**不被登记**进 workspace 目录表。
    #[test]
    fn create_session_with_a_client_directory_persists_the_path_without_an_alias() {
        let fixture = fixture();
        fixture.world.set_agent_session_id(
            crate::model::AgentSessionId::new("acp-session-client-path").expect("agent session id"),
        );
        let dir = temp_dir(&format!("acpr-client-create-{}", std::process::id()));
        let canonical = std::fs::canonicalize(&*dir)
            .expect("canonicalize")
            .to_str()
            .expect("path")
            .to_owned();
        let before = block_on(fixture.use_cases.workspaces(&Actor::LocalCli))
            .expect("workspaces")
            .len();

        let created = create_and_settle(
            &fixture,
            create_session_request_id(),
            digest('A'),
            create_request(),
            Some(WorkspaceSource::Path(canonical.clone())),
        );

        let state = lock(&fixture.world.state);
        assert_eq!(
            state
                .recoveries
                .get(created.as_str())
                .and_then(|columns| columns.workspace_cwd.clone()),
            Some(canonical),
            "规范化路径按本次解析结果落盘"
        );
        assert!(
            !state.workspace_aliases.contains_key(created.as_str()),
            "客户端提供的目录不得落盘别名（未分组）"
        );
        drop(state);
        assert!(
            projected_workspace(&fixture, &created).is_none(),
            "归属投影是「未分组」，MUST NOT 按路径反查别名补齐"
        );
        assert_eq!(
            block_on(fixture.use_cases.workspaces(&Actor::LocalCli))
                .expect("workspaces")
                .len(),
            before,
            "客户端提供的目录不得被登记进 workspace 目录表"
        );
        let _ = std::fs::remove_dir_all(&*dir);
    }

    // -----------------------------------------------------------------------------------------
    // `session.create` 的建目录开关（`sync-session-create` 的四条新场景）
    // -----------------------------------------------------------------------------------------

    /// 建目录开关与别名**互斥**在类型上成立：开关只可能挂在客户端提供的目录上，而别名没有对应变体
    /// （别名解析出的目录必然已存在）——因此这里没有「别名 + 开关」的组合可表达，不需要运行时判定。
    #[test]
    fn the_create_switch_is_structurally_exclusive_with_an_alias() {
        let alias = WorkspaceAlias::new("project").expect("alias");
        // 开关就是 `PathWithCreate` 这个变体本身：别名与「客户端提供但不建目录」的来源都不可能是它。
        for without_switch in [
            WorkspaceSource::Alias(alias.clone()),
            WorkspaceSource::Path("/work/api".to_owned()),
        ] {
            assert!(
                !matches!(without_switch, WorkspaceSource::PathWithCreate(_)),
                "只有客户端目录 + 显式开关那一种变体带开关：{without_switch:?}"
            );
        }
        // 客户端提供的两种来源各自携带路径原文；别名的路径由登记表解析，来源本身不带路径。
        assert!(matches!(
            WorkspaceSource::Path("/work/api".to_owned()),
            WorkspaceSource::Path(path) if path == "/work/api"
        ));
        assert!(matches!(
            WorkspaceSource::PathWithCreate("/work/api".to_owned()),
            WorkspaceSource::PathWithCreate(path) if path == "/work/api"
        ));
        assert!(matches!(
            WorkspaceSource::Alias(alias),
            WorkspaceSource::Alias(kept) if kept.as_str() == "project"
        ));
    }

    /// 开关为真且目录不存在：先创建（**连同缺失的父层**）再启动 Agent；创建出来的目录不是别名，
    /// 该会话呈现为「未分组」。
    #[test]
    fn the_create_switch_creates_missing_parents_before_starting_the_agent() {
        let fixture = fixture();
        fixture
            .world
            .set_agent_session_id(AgentSessionId::new("acp-session-created").expect("session id"));
        let root = temp_dir(&format!("acpr-create-switch-{}", std::process::id()));
        // 目标目录与其两层父目录都不存在。
        let target = root.join("a").join("b").join("c");
        assert!(!target.exists(), "前提：目标目录尚未存在");
        let before = block_on(fixture.use_cases.workspaces(&Actor::LocalCli))
            .expect("workspaces")
            .len();

        let created = create_and_settle(
            &fixture,
            create_session_request_id(),
            digest('A'),
            create_request(),
            Some(WorkspaceSource::PathWithCreate(
                target.to_str().expect("path").to_owned(),
            )),
        );

        assert!(target.is_dir(), "缺失的父层必须一起创建");
        let canonical = std::fs::canonicalize(&target)
            .expect("canonicalize")
            .to_str()
            .expect("path")
            .to_owned();
        let state = lock(&fixture.world.state);
        assert_eq!(
            state
                .recoveries
                .get(created.as_str())
                .and_then(|columns| columns.workspace_cwd.clone()),
            Some(canonical),
            "落盘的是创建后 canonicalize 的权威取值"
        );
        assert!(
            !state.workspace_aliases.contains_key(created.as_str()),
            "被创建的目录不成为已登记别名"
        );
        drop(state);
        assert!(
            projected_workspace(&fixture, &created).is_none(),
            "归属是「未分组」，MUST NOT 为它补出别名"
        );
        assert_eq!(
            block_on(fixture.use_cases.workspaces(&Actor::LocalCli))
                .expect("workspaces")
                .len(),
            before,
            "被创建的目录不进 workspace 登记表"
        );
        let _ = std::fs::remove_dir_all(&*root);
    }

    /// 路径上已有成分不是目录（目标本身是文件、或某个父层是文件）：服务端不可用类错误，**不覆盖、
    /// 不删除**既有内容，也不启动 Agent、不留会话行。
    #[test]
    fn the_create_switch_refuses_when_a_component_is_not_a_directory() {
        let fixture = fixture();
        let root = temp_dir(&format!("acpr-create-switch-file-{}", std::process::id()));
        let file = root.join("blocker");
        std::fs::write(&file, b"not a directory").expect("write file");
        let before = session_count(&fixture);

        // 目标本身就是文件。
        let error = block_on(fixture.use_cases.create_session(
            &Actor::LocalCli,
            create_session_request_id(),
            digest('A'),
            create_request(),
            Some(WorkspaceSource::PathWithCreate(
                file.to_str().expect("path").to_owned(),
            )),
        ))
        .expect_err("文件不能当目录");
        assert!(matches!(
            error,
            PortError::Unavailable(UnavailableKind::IoError)
        ));
        assert_eq!(
            std::fs::read(&file).expect("read"),
            b"not a directory".as_slice(),
            "不得覆盖既有文件"
        );

        // 某个父层成分是文件。
        let nested = file.join("child").join("leaf");
        let error = block_on(fixture.use_cases.create_session(
            &Actor::LocalCli,
            create_session_request_id(),
            digest('B'),
            create_request(),
            Some(WorkspaceSource::PathWithCreate(
                nested.to_str().expect("path").to_owned(),
            )),
        ))
        .expect_err("父层是文件时创建必须失败");
        assert!(matches!(
            error,
            PortError::Unavailable(UnavailableKind::IoError)
        ));
        assert_eq!(session_count(&fixture), before, "失败零会话行");
        assert!(!nested.exists());
        assert_eq!(
            std::fs::read(&file).expect("read"),
            b"not a directory".as_slice(),
            "不得删除既有文件"
        );
        let _ = std::fs::remove_dir_all(&*root);
    }

    /// 形状非法（相对路径、含 `..`）在**开关为真**时同样是参数类错误，且 MUST NOT 触碰文件系统：
    /// 目录不会被（部分）创建出来。
    #[test]
    fn the_create_switch_still_rejects_an_illegal_shape_without_touching_the_filesystem() {
        let fixture = fixture();
        let root = temp_dir(&format!("acpr-create-switch-shape-{}", std::process::id()));
        let created = root.join("escape");
        let escaped = format!(
            "{}{}..{}escape",
            created.display(),
            std::path::MAIN_SEPARATOR,
            std::path::MAIN_SEPARATOR
        );
        for shape in ["relative/dir", &escaped] {
            let error = block_on(fixture.use_cases.create_session(
                &Actor::LocalCli,
                create_session_request_id(),
                digest('A'),
                create_request(),
                Some(WorkspaceSource::PathWithCreate(shape.to_owned())),
            ))
            .expect_err("形状非法必须拒绝");
            assert!(
                matches!(error, PortError::InvalidRequest(_)),
                "{shape} 必须是参数类错误，实际 {error:?}"
            );
        }
        assert!(!created.exists(), "形状非法时不得创建任何目录");
        let _ = std::fs::remove_dir_all(&*root);
    }

    /// 目录已按开关创建，但随后的后端启动失败：命令失败，**已创建的目录仍在主机上**（不回滚、
    /// 不删除）。
    #[test]
    fn a_created_directory_survives_a_later_backend_failure() {
        let fixture = fixture();
        let root = temp_dir(&format!("acpr-create-switch-fail-{}", std::process::id()));
        let target = root.join("keep").join("me");
        fixture
            .world
            .push_create_error(PortError::Unavailable(UnavailableKind::Busy));
        let before = session_count(&fixture);

        block_on(fixture.use_cases.create_session(
            &Actor::LocalCli,
            create_session_request_id(),
            digest('A'),
            create_request(),
            Some(WorkspaceSource::PathWithCreate(
                target.to_str().expect("path").to_owned(),
            )),
        ))
        .expect_err("后端失败");

        assert!(target.is_dir(), "已创建的目录不得被删除或回滚");
        assert_eq!(session_count(&fixture), before, "失败零会话行");
        let _ = std::fs::remove_dir_all(&*root);
    }

    // -----------------------------------------------------------------------------------------
    // `workspace.browse` 与目录资源投影
    // -----------------------------------------------------------------------------------------

    /// 目录浏览替身：记录 `directories()` 的每次入参（用来观察「形状非法时不触碰文件系统」），
    /// 并按预置内容返回一层；`unreachable` 为真时按不可达的一层失败。
    struct RecordingBrowser {
        roots: Vec<DirectoryListing>,
        layer: Vec<DirectoryListing>,
        unreachable: bool,
        calls: std::sync::Mutex<Vec<String>>,
    }

    impl RecordingBrowser {
        fn new(layer: Vec<DirectoryListing>) -> Self {
            Self {
                roots: Vec::new(),
                layer,
                unreachable: false,
                calls: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<String> {
            lock(&self.calls).clone()
        }
    }

    #[async_trait::async_trait]
    impl DirectoryBrowser for RecordingBrowser {
        async fn roots(&self) -> Result<Vec<DirectoryListing>, PortError> {
            lock(&self.calls).push("<roots>".to_owned());
            Ok(self.roots.clone())
        }

        async fn directories(&self, parent: &str) -> Result<Vec<DirectoryListing>, PortError> {
            lock(&self.calls).push(parent.to_owned());
            if self.unreachable {
                return Err(PortError::Unavailable(UnavailableKind::IoError));
            }
            Ok(self.layer.clone())
        }
    }

    /// [`fixture`] 的目录浏览端口可替换版本（注入替身以观察是否触碰文件系统）。
    fn fixture_with_browser(browser: Arc<dyn DirectoryBrowser>) -> Fixture {
        let Fixture {
            use_cases,
            world,
            session,
        } = fixture();
        Fixture {
            use_cases: use_cases.with_directory_browser(browser),
            world,
            session,
        }
    }

    /// 一层里的一条目录条目（名称 + 绝对路径）。
    fn listing(parent: &std::path::Path, name: &str) -> DirectoryListing {
        DirectoryListing {
            name: name.to_owned(),
            path: parent.join(name).to_string_lossy().into_owned(),
        }
    }

    /// 形状非法（相对路径、含 `..`）是参数类错误，且**端口一次都没被调用**——目录浏览的判定顺序
    /// 必须是「形状 → 文件系统」，与既有的路径口径一致（不依赖 `canonicalize` 归一化 `..`）。
    #[test]
    fn workspace_browse_rejects_an_illegal_shape_without_touching_the_filesystem() {
        let browser = Arc::new(RecordingBrowser::new(Vec::new()));
        let fixture = fixture_with_browser(browser.clone());
        let parent_ref = std::env::temp_dir().join("sub").join("..");
        for shape in [
            "relative/dir".to_owned(),
            parent_ref.to_string_lossy().into_owned(),
            String::new(),
        ] {
            let error = block_on(
                fixture
                    .use_cases
                    .workspace_browse(&Actor::LocalCli, Some(&shape)),
            )
            .expect_err("形状非法必须拒绝");
            assert!(
                matches!(error, PortError::InvalidRequest(_)),
                "{shape} 必须是参数类错误，实际 {error:?}"
            );
        }
        assert!(
            browser.calls().is_empty(),
            "形状非法时不得触碰文件系统，实际调用 {}",
            browser.calls().len()
        );
    }

    /// 一层不可达（不存在/不可读/不是目录）是服务端不可用类错误，且**不返回任何条目**，也不回退到
    /// 父层或根列表。
    #[test]
    fn workspace_browse_reports_an_unreachable_layer_as_unavailable() {
        // 用随 core 提供的默认实现：真实临时目录下不存在的一层。
        let fixture = fixture();
        let missing = std::env::temp_dir().join(format!("acpr-browse-missing-{}", uuid_text(31)));
        let error = block_on(
            fixture
                .use_cases
                .workspace_browse(&Actor::LocalCli, Some(missing.to_str().expect("path"))),
        )
        .expect_err("不存在的一层必须失败");
        assert!(matches!(
            error,
            PortError::Unavailable(UnavailableKind::IoError)
        ));

        // 形式合法但是文件：同样是不可用类（不是「这一层没有目录」）。
        let file = std::env::temp_dir().join(format!("acpr-browse-file-{}", uuid_text(32)));
        std::fs::write(&file, b"x").expect("write");
        let error = block_on(
            fixture
                .use_cases
                .workspace_browse(&Actor::LocalCli, Some(file.to_str().expect("path"))),
        )
        .expect_err("文件不是一层");
        assert!(matches!(
            error,
            PortError::Unavailable(UnavailableKind::IoError)
        ));
        let _ = std::fs::remove_file(&file);

        // 端口的不可达（读目录中途失败）同样映射成不可用类，而不是空列表。
        let browser = Arc::new(RecordingBrowser {
            unreachable: true,
            ..RecordingBrowser::new(Vec::new())
        });
        let fixture = fixture_with_browser(browser);
        let dir = temp_dir(&format!("acpr-browse-unreadable-{}", std::process::id()));
        let error = block_on(
            fixture
                .use_cases
                .workspace_browse(&Actor::LocalCli, Some(dir.to_str().expect("path"))),
        )
        .expect_err("端口的失败必须上抛");
        assert!(matches!(
            error,
            PortError::Unavailable(UnavailableKind::IoError)
        ));
        let _ = std::fs::remove_dir_all(&*dir);
    }

    /// 默认实现只返回**目录**：同一层里的普通文件不出现在条目里，条目也不带任何大小/时间戳字段。
    #[test]
    fn workspace_browse_lists_directories_only() {
        let fixture = fixture();
        let root = temp_dir(&format!("acpr-browse-layer-{}", std::process::id()));
        std::fs::create_dir_all(root.join("nested")).expect("nested dir");
        std::fs::write(root.join("note.txt"), b"content").expect("file");
        let result = block_on(
            fixture
                .use_cases
                .workspace_browse(&Actor::LocalCli, Some(root.to_str().expect("path"))),
        )
        .expect("browse");
        assert_eq!(
            result.entries.len(),
            1,
            "只列直接子目录：{:?}",
            result.entries
        );
        assert_eq!(result.entries[0].name, "nested");
        assert!(result.entries[0].path.ends_with("nested"));
        assert!(!result.truncated);
        assert!(
            !result
                .entries
                .iter()
                .any(|entry| entry.name.contains("note.txt")),
            "文件条目不得出现"
        );
        let _ = std::fs::remove_dir_all(&*root);
    }

    /// 条目按名称以确定的顺序排列（端口给出的顺序不影响结果），并标记「是否已登记」。
    #[test]
    fn workspace_browse_sorts_entries_and_marks_registered_directories() {
        let root = temp_dir(&format!("acpr-browse-order-{}", std::process::id()));
        let canonical = std::fs::canonicalize(&*root).expect("canonicalize");
        let alias = WorkspaceAlias::new("api").expect("alias");
        let browser = Arc::new(RecordingBrowser::new(vec![
            listing(&canonical, "zeta"),
            listing(&canonical, "Alpha"),
            listing(&canonical, "beta"),
        ]));
        let fixture = fixture_with_browser(browser.clone());
        fixture.world.workspaces.lock().expect("lock").push(
            WorkspaceRecord::try_new(
                alias,
                "API",
                canonical.join("beta").to_str().expect("path"),
                crate::broker::test_support::ts(0),
                crate::broker::test_support::ts(0),
            )
            .expect("workspace record"),
        );

        let result = block_on(
            fixture
                .use_cases
                .workspace_browse(&Actor::LocalCli, Some(root.to_str().expect("path"))),
        )
        .expect("browse");
        assert_eq!(
            result
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Alpha", "beta", "zeta"],
            "按名称的 Unicode 码点升序，端口给出的顺序不影响结果"
        );
        assert_eq!(
            result
                .entries
                .iter()
                .map(|entry| entry.registered)
                .collect::<Vec<_>>(),
            vec![false, true, false],
            "只有与某个已登记目录的规范化路径相同的那条为真"
        );
        // 结果里的 `path` 是**请求原文的逐字回显**（这里请求的就是 temp 目录的原文，规范化结果在
        // Windows 上多出 verbatim 前缀，因此两者不是同一个取值）。
        assert_eq!(result.path.as_deref(), root.to_str());
        assert_eq!(browser.calls().len(), 1, "每次浏览只查询一层");
        let _ = std::fs::remove_dir_all(&*root);
    }

    /// 结果里的 `path` 与请求原文**逐字相同**，即使它与规范化结果不是同一个取值；条目的 `path` 仍取
    /// 下发形式，`registered` 仍按规范化路径判定。
    ///
    /// 用**尾分隔符**制造「原文 ≠ 规范化结果」：形状校验接受它，`canonicalize` 会去掉它，这条差异在
    /// Windows 与 Unix 上都成立（Windows 上还有 verbatim 前缀这一层差异）。
    #[test]
    fn workspace_browse_echoes_the_requested_parent_verbatim() {
        let root = temp_dir(&format!("acpr-browse-echo-{}", std::process::id()));
        let canonical = std::fs::canonicalize(&*root).expect("canonicalize");
        let requested = format!(
            "{}{}",
            root.to_str().expect("path"),
            std::path::MAIN_SEPARATOR
        );
        let alias = WorkspaceAlias::new("api").expect("alias");
        let browser = Arc::new(RecordingBrowser::new(vec![
            listing(&canonical, "sub"),
            listing(&canonical, "registered"),
        ]));
        let fixture = fixture_with_browser(browser);
        // 已登记目录取 `registered` 这一条：`registered` 必须按**规范化路径**判定，与请求形态无关。
        fixture.world.workspaces.lock().expect("lock").push(
            WorkspaceRecord::try_new(
                alias,
                "API",
                canonical.join("registered").to_str().expect("path"),
                crate::broker::test_support::ts(0),
                crate::broker::test_support::ts(0),
            )
            .expect("workspace record"),
        );

        let result = block_on(
            fixture
                .use_cases
                .workspace_browse(&Actor::LocalCli, Some(requested.as_str())),
        )
        .expect("browse");

        assert_eq!(
            result.path.as_deref(),
            Some(requested.as_str()),
            "结果里的 `path` 必须是请求原文"
        );
        assert_ne!(
            result.path.as_deref(),
            Some(canonical.to_str().expect("path")),
            "回显的不是规范化结果——否则这条断言在斜杠/尾分隔符形态下就会漏过"
        );
        // 条目的 `path` 仍是**下发形式**：规范化结果剥掉平台 verbatim 前缀。
        assert_eq!(
            result
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["registered", "sub"],
            "条目按名称升序"
        );
        assert_eq!(
            result.entries[1].path,
            crate::strip_verbatim(canonical.join("sub"))
                .to_string_lossy()
                .into_owned(),
            "条目的 `path` 仍是下发形式（规范化结果剥掉 verbatim 前缀）"
        );
        assert!(
            result.entries[0].registered,
            "registered 按规范化路径判定，不受请求形态影响"
        );
        assert!(!result.entries[1].registered);

        // Windows 上再钉住一种「请求原文与规范化结果不同」的形态：正斜杠。`canonicalize` 会把它换成
        // 反斜杠（并补上 verbatim 前缀），回显则必须保留客户端提交的那一份文本。
        #[cfg(windows)]
        {
            let forward = requested.replace('\\', "/");
            let echoed = block_on(
                fixture
                    .use_cases
                    .workspace_browse(&Actor::LocalCli, Some(forward.as_str())),
            )
            .expect("browse");
            assert_eq!(echoed.path.as_deref(), Some(forward.as_str()));
            assert_ne!(
                echoed.path.as_deref(),
                Some(canonical.to_str().expect("path"))
            );
        }
        let _ = std::fs::remove_dir_all(&*root);
    }

    /// 根列表请求（`path = null`）走端口的 `roots()`，结果条目的父路径是 `None`。
    #[test]
    fn workspace_browse_without_a_parent_returns_the_root_list() {
        let browser = Arc::new(RecordingBrowser {
            roots: vec![
                DirectoryListing {
                    name: "C:".to_owned(),
                    path: "C:\\".to_owned(),
                },
                DirectoryListing {
                    name: "D:".to_owned(),
                    path: "D:\\".to_owned(),
                },
            ],
            ..RecordingBrowser::new(Vec::new())
        });
        let fixture = fixture_with_browser(browser.clone());
        let result =
            block_on(fixture.use_cases.workspace_browse(&Actor::LocalCli, None)).expect("roots");
        assert_eq!(result.path, None);
        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.entries[0].name, "C:");
        assert!(!result.truncated);
        assert_eq!(browser.calls(), vec!["<roots>".to_owned()]);
    }

    /// 超过固定上限时只返回上限内的条目并置截断标记：不静默丢弃，也不因超限整次失败。
    #[test]
    fn workspace_browse_truncates_over_the_fixed_limit() {
        let root = temp_dir(&format!("acpr-browse-limit-{}", std::process::id()));
        let canonical = std::fs::canonicalize(&*root).expect("canonicalize");
        let layer: Vec<DirectoryListing> = (0..WORKSPACE_BROWSE_ENTRY_LIMIT + 12)
            .map(|index| listing(&canonical, &format!("dir-{index:03}")))
            .collect();
        let browser = Arc::new(RecordingBrowser::new(layer));
        let fixture = fixture_with_browser(browser);
        let result = block_on(
            fixture
                .use_cases
                .workspace_browse(&Actor::LocalCli, Some(root.to_str().expect("path"))),
        )
        .expect("browse");
        assert!(result.truncated, "超限必须置截断标记");
        assert_eq!(result.entries.len(), WORKSPACE_BROWSE_ENTRY_LIMIT);
        assert_eq!(
            result.entries[0].name, "dir-000",
            "保留的是排序后的前若干条"
        );
        let _ = std::fs::remove_dir_all(&*root);
    }

    /// 目录资源投影：协商 `core.workspace-path.v1` 时每条带规范化路径（`strip_verbatim` 之后的形式），
    /// 未协商时该键**缺席**（`None`，不是空串）——对端因此仍看到只有两个键的封闭对象。
    #[test]
    fn the_workspace_catalog_carries_paths_only_when_negotiated() {
        let fixture = fixture();
        let alias = WorkspaceAlias::new("project").expect("alias");
        fixture.world.workspaces.lock().expect("lock").push(
            WorkspaceRecord::try_new(
                alias.clone(),
                "Project",
                std::env::temp_dir().to_str().expect("temp dir is utf-8"),
                crate::broker::test_support::ts(0),
                crate::broker::test_support::ts(0),
            )
            .expect("workspace record"),
        );

        let with_path = block_on(
            fixture
                .use_cases
                .sync_workspace_catalog(&Actor::LocalCli, true),
        )
        .expect("catalog");
        assert_eq!(with_path.len(), 1);
        assert_eq!(with_path[0].alias(), &alias);
        assert_eq!(with_path[0].display_name(), "Project");
        let path = with_path[0].path().expect("协商后必须带路径");
        assert!(
            std::path::Path::new(path).is_absolute(),
            "路径必须是绝对路径形状：{path}"
        );
        assert!(
            !path.starts_with(r"\\?\"),
            "下发形式必须剥掉 verbatim 前缀：{path}"
        );

        let without_path = block_on(
            fixture
                .use_cases
                .sync_workspace_catalog(&Actor::LocalCli, false),
        )
        .expect("catalog");
        assert_eq!(without_path.len(), 1);
        assert_eq!(
            without_path[0].path(),
            None,
            "未协商路径能力时该键缺席（不是空串、不是 null 占位）"
        );
        assert_eq!(without_path[0].alias(), &alias);
        assert_eq!(without_path[0].display_name(), "Project");
    }

    /// §11.6：`import.remove` 只走一次写集（管理行 + 关联行 + 交付索引 + 命令引用），
    /// 不再串联 `drop_import`；审计随写集提交。
    #[test]
    fn remove_import_is_one_atomic_write_set_with_audit() {
        let fixture = fixture();
        let import = ImportId::new("remote-a").expect("import id");
        let record = ImportRecord::try_new(
            import.clone(),
            "wss://owner.example/acp",
            NodeId::new(&uuid_text(21)).expect("node"),
            vec![ExportId::new("exp-a").expect("export")],
            crate::model::GrantSet::try_from_iter(["grant.observe"]).expect("grants"),
        )
        .expect("import record");
        // §11.2 第 5 条 / §11.7：owner 节点必须已配对且是 owner 角色，先种一行。
        fixture.world.nodes.lock().expect("lock").push(node_record(
            record.owner_node_id(),
            NodeKind::Owner,
            NodeState::Paired,
        ));
        block_on(fixture.use_cases.add_import(&Actor::LocalCli, record)).expect("add import");

        block_on(fixture.use_cases.remove_import(&Actor::LocalCli, &import)).expect("remove");

        assert_eq!(
            fixture
                .world
                .drop_import_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            0,
            "完整移除不得串联连接级 drop_import"
        );
        assert!(
            fixture.world.imports.lock().expect("lock").is_empty(),
            "管理行必须被删除"
        );
        assert_eq!(
            *fixture.world.write_audits.lock().expect("lock"),
            vec![AuditAction::ImportAdded, AuditAction::ImportRemoved],
            "两条写集各自携带自己的审计"
        );
    }

    /// 测试用的节点角色行（`NodeRecord` 的构造不变式：`revoked_at` 与状态成对、owner 必须有 endpoint）。
    fn node_record(id: &NodeId, kind: NodeKind, state: NodeState) -> NodeRecord {
        NodeRecord::try_new(
            id.clone(),
            "peer node",
            kind,
            crate::model::Fingerprint::new(&"a".repeat(64)).expect("fingerprint"),
            crate::model::GrantSet::try_from_iter(["grant.remote-work"]).expect("grants"),
            Vec::new(),
            state,
            match kind {
                NodeKind::Owner => Some("wss://owner.example/acpr".to_owned()),
                NodeKind::Access => None,
            },
            crate::broker::test_support::ts(0),
            None,
            if state == NodeState::Revoked {
                Some(crate::broker::test_support::ts(1))
            } else {
                None
            },
        )
        .expect("node record")
    }

    /// 与 [`node_record`] 相同，但 grants 与点名的 Export 清单由调用方给出（Owner 侧授权的查询用例）。
    fn paired_node(id: &NodeId, grants: &[&str], export_ids: &[&str]) -> NodeRecord {
        NodeRecord::try_new(
            id.clone(),
            "office access",
            NodeKind::Access,
            crate::model::Fingerprint::new(&"a".repeat(64)).expect("fingerprint"),
            crate::model::GrantSet::try_from_iter(grants.iter().copied()).expect("grants"),
            export_ids
                .iter()
                .map(|id| ExportId::new(id).expect("export id"))
                .collect(),
            NodeState::Paired,
            None,
            crate::broker::test_support::ts(0),
            None,
            None,
        )
        .expect("node record")
    }

    /// §11.2 第 4/5 条 + §11.7：`export.create` 与 `import.add` 引用的本机事实必须在用例层校验
    /// （`LOCAL_ADMIN_PROTOCOL.md` §5.5 把它定为 `local.not_found` / 参数类失败）。
    #[test]
    fn export_and_import_preconditions_are_enforced() {
        let fixture = fixture();
        let alias = WorkspaceAlias::new("project").expect("alias");
        let agent_ref = AgentRef::try_new(AgentId::new("codex").expect("agent id"), "Codex")
            .expect("agent ref");
        let export = |alias: &WorkspaceAlias| {
            ExportRecord::try_new(
                ExportId::new("exp-pre").expect("export id"),
                "team export",
                vec![agent_ref.agent_id().clone()],
                vec![
                    crate::model::WorkspaceAliasEntry::try_new(alias.clone(), "Project")
                        .expect("alias entry"),
                ],
                alias.clone(),
                vec![
                    crate::model::ExportTemplate::try_new(
                        crate::model::TemplateId::new("coding").expect("template id"),
                        "Coding",
                        alias.clone(),
                        vec![
                            crate::model::TemplateParam::try_new(
                                crate::model::ParamName::new("model").expect("param name"),
                                crate::model::TemplateParamType::String,
                                true,
                                None,
                                None,
                            )
                            .expect("param"),
                        ],
                    )
                    .expect("template"),
                ],
                crate::model::TemplateId::new("coding").expect("template id"),
                crate::model::GrantSet::try_from_iter(["grant.remote-work"]).expect("grants"),
                crate::model::CachePolicy::NoContentCache,
                crate::broker::test_support::ts(0),
                None,
            )
            .expect("export record")
        };

        // 先把该 Agent 放进目录：这一步之后「别名未登记」必须由别名前置单独拦下（否则空目录下的
        // Agent 前置会顶掉它，测试就守不住别名校验）。
        fixture
            .world
            .catalog_agents
            .lock()
            .expect("lock")
            .push(AgentDescriptor {
                agent: agent_ref.clone(),
                available: true,
                origin: ResourceOrigin::Local,
            });

        // 别名未在本机登记 → 拒绝。
        let error = block_on(
            fixture
                .use_cases
                .put_export(&Actor::LocalCli, export(&alias)),
        )
        .expect_err("an unregistered workspace alias must be refused");
        assert!(matches!(error, PortError::InvalidRequest(_)), "{error:?}");

        // 登记别名但目录里没有该 Agent → 拒绝（清空目录后同一份 Export 必须仍被拒）。
        fixture.world.catalog_agents.lock().expect("lock").clear();
        let record = WorkspaceRecord::try_new(
            alias.clone(),
            "Repo",
            if cfg!(windows) {
                "C:\\\\Project\\\\acp-remote"
            } else {
                "/srv/acp-remote"
            },
            crate::broker::test_support::ts(0),
            crate::broker::test_support::ts(0),
        )
        .expect("workspace record");
        block_on(fixture.use_cases.put_workspace(&Actor::LocalCli, record)).expect("put workspace");
        let error = block_on(
            fixture
                .use_cases
                .put_export(&Actor::LocalCli, export(&alias)),
        )
        .expect_err("an unknown agent must be refused");
        assert!(matches!(error, PortError::InvalidRequest(_)), "{error:?}");

        // 别名已登记且 Agent 在目录里 → 通过。
        fixture
            .world
            .catalog_agents
            .lock()
            .expect("lock")
            .push(AgentDescriptor {
                agent: agent_ref.clone(),
                available: true,
                origin: ResourceOrigin::Local,
            });
        block_on(
            fixture
                .use_cases
                .put_export(&Actor::LocalCli, export(&alias)),
        )
        .expect("a fully resolvable export must be accepted");

        // `import.add`：owner 节点必须已配对且角色是 owner。
        let owner = NodeId::new(&uuid_text(21)).expect("node");
        let import = |owner: NodeId| {
            ImportRecord::try_new(
                ImportId::new("remote-pre").expect("import id"),
                "wss://owner.example/acp",
                owner,
                vec![ExportId::new("exp-pre").expect("export")],
                crate::model::GrantSet::try_from_iter(["grant.observe"]).expect("grants"),
            )
            .expect("import record")
        };
        let error = block_on(
            fixture
                .use_cases
                .add_import(&Actor::LocalCli, import(owner.clone())),
        )
        .expect_err("an absent owner node must be refused");
        assert!(
            matches!(error, PortError::NotFound(_)),
            "缺席必须报 NotFound：{error:?}"
        );
        fixture.world.nodes.lock().expect("lock").push(node_record(
            &owner,
            NodeKind::Access,
            NodeState::Paired,
        ));
        let error = block_on(
            fixture
                .use_cases
                .add_import(&Actor::LocalCli, import(owner.clone())),
        )
        .expect_err("an access-role node must not own an import");
        assert!(matches!(error, PortError::InvalidRequest(_)), "{error:?}");
        // 角色正确但还没配对上（`pending`）→ 仍必须拒绝（§5.5 要求「已配对」）。
        fixture.world.nodes.lock().expect("lock").push(node_record(
            &owner,
            NodeKind::Owner,
            NodeState::Pending,
        ));
        let error = block_on(
            fixture
                .use_cases
                .add_import(&Actor::LocalCli, import(owner.clone())),
        )
        .expect_err("an unpaired owner node must be refused");
        assert!(matches!(error, PortError::InvalidRequest(_)), "{error:?}");
        fixture
            .world
            .nodes
            .lock()
            .expect("lock")
            .retain(|record| record.node_id() != &owner);
        fixture.world.nodes.lock().expect("lock").push(node_record(
            &owner,
            NodeKind::Owner,
            NodeState::Paired,
        ));
        block_on(
            fixture
                .use_cases
                .add_import(&Actor::LocalCli, import(owner.clone())),
        )
        .expect("a paired owner node must be accepted");
    }

    /// §11.6：撤销设备时审计与状态同一写集（不再是「先写状态再补审计」）。
    #[test]
    fn revoke_device_carries_its_audit_in_the_write_set() {
        let fixture = fixture();
        let device = DeviceId::new(&uuid_text(31)).expect("device");
        block_on(fixture.use_cases.revoke_device(&Actor::LocalCli, &device)).expect("revoke");
        assert_eq!(
            *fixture.world.write_audits.lock().expect("lock"),
            vec![AuditAction::DeviceRevoked]
        );
    }

    /// §11.6 第 2 条 + `SECURITY_DESIGN.md` §14.2：落定的审计动作按目标族区分——节点配对的**批准**
    /// 写 `node.paired`（此前该动作在实现里没有任何写入方），设备配对的批准写 `pairing.approved`，
    /// 拒绝两族都写 `pairing.rejected`。
    #[test]
    fn pairing_settlement_carries_the_target_family_audit() {
        let fixture = fixture();
        let grants = crate::model::GrantSet::try_from_iter(["grant.observe"]).expect("grants");
        let node = PairingId::new(&uuid_text(41)).expect("pairing");
        let device = PairingId::new(&uuid_text(42)).expect("pairing");
        let rejected = PairingId::new(&uuid_text(43)).expect("pairing");
        for (id, target, requested) in [
            (&node, PairingTarget::Node, grants.clone()),
            (
                &device,
                PairingTarget::Device,
                crate::model::GrantSet::empty(),
            ),
            (
                &rejected,
                PairingTarget::Device,
                crate::model::GrantSet::empty(),
            ),
        ] {
            fixture.world.pairings.lock().expect("lock").push(
                PairingRecord::try_new(
                    id.clone(),
                    target,
                    PairingState::PendingConfirmation,
                    None,
                    ScopeSet::empty(),
                    requested,
                    digest('p'),
                    "https://node.example",
                    crate::broker::test_support::ts(0),
                    crate::broker::test_support::ts(5),
                    Some(crate::broker::test_support::ts(0)),
                    None,
                    None,
                )
                .expect("pairing record"),
            );
        }

        block_on(fixture.use_cases.settle_pairing(
            &Actor::LocalCli,
            &node,
            PairingSettlement::approved(ScopeSet::empty(), grants.clone()),
        ))
        .expect("approve node pairing");
        block_on(fixture.use_cases.settle_pairing(
            &Actor::LocalCli,
            &device,
            PairingSettlement::approved(ScopeSet::empty(), crate::model::GrantSet::empty()),
        ))
        .expect("approve device pairing");
        block_on(fixture.use_cases.settle_pairing(
            &Actor::LocalCli,
            &rejected,
            PairingSettlement::rejected(Some("user said no")).expect("rejection"),
        ))
        .expect("reject pairing");

        assert_eq!(
            *fixture.world.write_audits.lock().expect("lock"),
            vec![
                AuditAction::NodePaired,
                AuditAction::PairingApproved,
                AuditAction::PairingRejected
            ],
            "节点批准写 node.paired，设备批准写 pairing.approved"
        );
    }

    /// §5.1：授权先于 workspace 解析。未授权的 `session.create` 必须得到授权类错误，而不是
    /// 「别名已登记但目录缺失」的 `Unavailable(IoError)`——否则本机登记状态与文件系统成为预言机。
    /// 设备方向已无授权判定（`pairing-model`），因此这条守卫的判别力落在节点面。
    #[test]
    fn create_session_authorizes_before_resolving_the_workspace() {
        let fixture = fixture();
        let alias = WorkspaceAlias::new("repo").expect("alias");
        // 已登记但目录不存在：只要解析被触发，错误就会先变成 `Unavailable(IoError)`。
        // 用临时目录下不存在的子路径而不是固定的盘符路径：`WorkspaceRecord` 只校验绝对路径形状，
        // 而 `Z:\\…` 只在 Windows 上是绝对路径，在 Unix 上会先被值对象拒绝。
        let missing =
            std::env::temp_dir().join(format!("acpr-missing-workspace-{}", uuid_text(52)));
        let record = WorkspaceRecord::try_new(
            alias.clone(),
            "Repo",
            missing.to_str().expect("path"),
            crate::broker::test_support::ts(0),
            crate::broker::test_support::ts(0),
        )
        .expect("workspace record");
        block_on(fixture.use_cases.put_workspace(&Actor::LocalCli, record)).expect("put workspace");

        let actor = Actor::Node {
            node: NodeId::new(&uuid_text(51)).expect("node"),
            access_node: NodeId::new(&uuid_text(53)).expect("access node"),
        };
        let error = block_on(fixture.use_cases.create_session(
            &actor,
            create_session_request_id(),
            digest('A'),
            create_request(),
            Some(WorkspaceSource::Alias(alias.clone())),
        ))
        .expect_err("an unauthorized actor must be denied");
        match error {
            PortError::InvalidRequest(code) => {
                assert!(code.starts_with("authorization."), "得到 {code}");
            }
            other => panic!("期望授权类错误，得到 {other:?}"),
        }
    }

    // ---------------------------------------------------------------------------------------
    // 配对通道（design D12）
    // ---------------------------------------------------------------------------------------

    const HOST_BINDING: &str = "https://owner.example";

    /// 造一个已认领的对端（配对通道用例共用；公钥用 model 的基点 G 测试常量）。
    fn peer(identity: PeerIdentity) -> crate::model::PairingPeer {
        crate::model::PairingPeer::try_new(
            identity,
            "Pair One",
            crate::model::test_peer_public_key(),
            HOST_BINDING,
            Nonce::new(&"E".repeat(43)).expect("nonce"),
        )
        .expect("pairing peer")
    }

    /// 播种一条配对行与它的对端行：`claimed_at`/`approved_at`/`terminal_at` 与状态成对给出
    /// （`PairingRecord` 的构造校验会拒绝自相矛盾的时间戳）。
    fn seed_pairing(
        world: &Arc<FakeWorld>,
        pairing: &PairingId,
        target: PairingTarget,
        state: PairingState,
        peer: crate::model::PairingPeer,
    ) {
        let grants = match target {
            PairingTarget::Device => crate::model::GrantSet::empty(),
            PairingTarget::Node => {
                crate::model::GrantSet::try_from_iter(["grant.observe"]).expect("grants")
            }
        };
        let claimed_at =
            (state != PairingState::Created).then(|| crate::broker::test_support::ts(0));
        let approved_at = matches!(state, PairingState::Approved | PairingState::Consumed)
            .then(|| crate::broker::test_support::ts(1));
        let terminal_at = state
            .is_terminal()
            .then(|| crate::broker::test_support::ts(2));
        world.pairings.lock().expect("lock").push(
            PairingRecord::try_new(
                pairing.clone(),
                target,
                state,
                Some("Pair One".to_owned()),
                ScopeSet::empty(),
                grants,
                digest('p'),
                HOST_BINDING,
                crate::broker::test_support::ts(0),
                crate::broker::test_support::ts(600),
                claimed_at,
                approved_at,
                terminal_at,
            )
            .expect("pairing record"),
        );
        world
            .peers
            .lock()
            .expect("lock")
            .push((pairing.clone(), peer));
    }

    /// design D3 / 任务 2.28：Node Link 握手准入的只读视图绑定到单个自报 `accessNodeId`——已知节点带回信任行、
    /// 身份材料、最近一次配对与本机水位；未知节点得到空视图（不是错误）；全过程零写入。
    #[test]
    fn node_link_handshake_view_is_bound_to_one_reported_node_id() {
        let fixture = fixture();
        let node_id = NodeId::new(&uuid_text(101)).expect("node");
        let pairing = PairingId::new(&uuid_text(102)).expect("pairing");
        seed_pairing(
            &fixture.world,
            &pairing,
            PairingTarget::Node,
            PairingState::Approved,
            peer(PeerIdentity::Node(node_id.clone())),
        );
        fixture.world.nodes.lock().expect("lock").push(node_record(
            &node_id,
            NodeKind::Access,
            NodeState::Paired,
        ));
        let key = crate::model::test_peer_public_key();
        fixture
            .world
            .keys
            .lock()
            .expect("lock")
            .push((PeerIdentity::Node(node_id.clone()), key.clone()));

        let view =
            block_on(fixture.use_cases.node_link_handshake_view(&node_id)).expect("handshake view");
        let node = view
            .node
            .as_ref()
            .expect("已配对的 access 节点必须带回信任行");
        assert_eq!(node.kind(), NodeKind::Access);
        assert_eq!(node.state(), NodeState::Paired);
        assert_eq!(view.public_key, Some(key), "验签公钥只能来自持久化信任");
        assert_eq!(
            view.pairing.as_ref().map(PairingRecord::id),
            Some(&pairing),
            "待消费配对与登记方宣告的 host_binding 都来自最近一次配对行"
        );
        assert_eq!(
            view.head,
            crate::broker::test_support::head_cursor(0),
            "本机水位来自 store.head()"
        );

        // 未知节点：空视图，不是错误（调用方照常签发挑战，失败映射在 proof 阶段）。
        let unknown = NodeId::new(&uuid_text(103)).expect("node");
        let empty = block_on(fixture.use_cases.node_link_handshake_view(&unknown))
            .expect("未知节点不能报错");
        assert!(empty.node.is_none() && empty.public_key.is_none() && empty.pairing.is_none());

        // 只有 owner 角色行、没有 access 行时同样按未知处理（禁止「找不到就取第一行」）。
        fixture.world.nodes.lock().expect("lock").push(node_record(
            &node_id,
            NodeKind::Owner,
            NodeState::Paired,
        ));
        let owner_only =
            block_on(fixture.use_cases.node_link_handshake_view(&node_id)).expect("view");
        assert_eq!(
            owner_only.node.as_ref().map(NodeRecord::kind),
            Some(NodeKind::Access),
            "同一 nodeId 的 owner 行不得被当成 access 行"
        );

        assert!(
            fixture.world.write_audits.lock().expect("lock").is_empty()
                && fixture.world.audits.lock().expect("lock").is_empty(),
            "握手准入读取零写入、无审计"
        );
    }

    /// 设备行的测试夹具（§5.3）：`created_at` 取 `ts(30)`，与 `TestClock` 从 0 起的推进值不撞号。
    fn device_record(
        id: &DeviceId,
        state: DeviceState,
        last_seen_at: Option<Timestamp>,
        revoked_at: Option<Timestamp>,
    ) -> DeviceRecord {
        DeviceRecord::try_new(
            id.clone(),
            "phone",
            crate::model::Fingerprint::new(&"b".repeat(64)).expect("fingerprint"),
            state,
            crate::broker::test_support::ts(30),
            last_seen_at,
            revoked_at,
        )
        .expect("device record")
    }

    /// Sync WP1（任务 1.1）：设备握手准入的只读视图绑定到单个自报 `deviceId`——已知设备带回信任行、
    /// 身份材料、最近一次配对与本机水位；未知设备得到**结构完整**的空视图（不是错误）；零写入。
    #[test]
    fn sync_device_handshake_view_is_bound_to_one_reported_device_id() {
        let (fixture, trust) = fixture_with_device_trust();
        let device = DeviceId::new(&uuid_text(201)).expect("device");
        let other = DeviceId::new(&uuid_text(202)).expect("device");
        let pairing = PairingId::new(&uuid_text(203)).expect("pairing");
        seed_pairing(
            &fixture.world,
            &pairing,
            PairingTarget::Device,
            PairingState::Approved,
            peer(PeerIdentity::Device(device.clone())),
        );
        trust.devices.lock().expect("lock").push(device_record(
            &device,
            DeviceState::Active,
            None,
            None,
        ));
        let key = crate::model::test_peer_public_key();
        fixture
            .world
            .keys
            .lock()
            .expect("lock")
            .push((PeerIdentity::Device(device.clone()), key.clone()));

        let view = block_on(fixture.use_cases.sync_device_handshake_view(&device))
            .expect("handshake view");
        assert_eq!(
            view.device.as_ref().map(DeviceRecord::state),
            Some(DeviceState::Active),
            "信任行（凭据状态的来源）只能来自该设备自己的行"
        );
        assert_eq!(view.public_key, Some(key), "验签公钥只能来自持久化信任");
        assert_eq!(
            view.pairing.as_ref().map(PairingRecord::id),
            Some(&pairing),
            "host_binding 与待消费配对的来源是该设备最近一次配对"
        );
        assert_eq!(
            view.head,
            crate::broker::test_support::head_cursor(0),
            "本机水位来自 store.head()，同时携带 serverEpoch"
        );

        // 未知设备：结构完整的空视图，不是错误（调用方照常签发挑战，失败映射在 proof 阶段）。
        let unknown = DeviceId::new(&uuid_text(204)).expect("device");
        let empty = block_on(fixture.use_cases.sync_device_handshake_view(&unknown))
            .expect("未知设备不能报错");
        assert!(
            empty.device.is_none() && empty.public_key.is_none() && empty.pairing.is_none(),
            "未知设备得到空视图"
        );
        assert_eq!(empty.head, view.head, "空视图照样带本机水位与 epoch");

        // 另一个已有信任行的设备：只带回它自己的事实，不得漏出第一个设备的公钥/配对。
        trust.devices.lock().expect("lock").push(device_record(
            &other,
            DeviceState::Pending,
            None,
            None,
        ));
        let other_view =
            block_on(fixture.use_cases.sync_device_handshake_view(&other)).expect("view");
        assert_eq!(
            other_view.device.as_ref().map(DeviceRecord::device_id),
            Some(&other)
        );
        assert!(
            other_view.public_key.is_none() && other_view.pairing.is_none(),
            "入口不返回任何其它设备的事实"
        );

        assert!(
            fixture.world.write_audits.lock().expect("lock").is_empty()
                && fixture.world.audits.lock().expect("lock").is_empty(),
            "握手准入读取零写入、无审计"
        );
    }

    /// Sync WP1（任务 1.2）：设备认证收尾把 `last_seen_at` 置为本次时间（**只前进**由存储层的
    /// `CASE` 保证，本层既不改写该规则也不抹掉已存值）、其余每一列原样透传，并把
    /// `device.authenticated` 与状态推进放进**同一个写集**。
    #[test]
    fn record_device_connected_advances_last_seen_and_audits() {
        let (fixture, trust) = fixture_with_device_trust();
        let device = DeviceId::new(&uuid_text(211)).expect("device");
        trust.devices.lock().expect("lock").push(device_record(
            &device,
            DeviceState::Active,
            None,
            None,
        ));

        block_on(fixture.use_cases.record_device_connected(&device)).expect("record connected");

        let writes = trust.writes.lock().expect("lock").clone();
        assert_eq!(writes.len(), 1, "一次认证收尾只提交一个写集");
        let first = &writes[0];
        let now = crate::broker::test_support::ts(0);
        assert_eq!(
            first.record.last_seen_at(),
            Some(&now),
            "旧值为空时写入本次时间（旧值→新值这一支由存储层落库）"
        );
        assert_eq!(&first.context.at, &now, "状态列与写集 at 同源");
        // 除 `last_seen_at` 外的每一列都必须原样透传：本层不重造记录、不改状态、不碰撤销时间。
        assert_eq!(first.record.display_name(), "phone");
        assert_eq!(first.record.state(), DeviceState::Active);
        assert_eq!(
            first.record.created_at(),
            &crate::broker::test_support::ts(30)
        );
        assert_eq!(first.record.revoked_at(), None);
        // 审计：动作、目标与归因（设备方向没有授权集合，actor 因此只有 id）。
        assert_eq!(first.context.audit.len(), 1, "收尾写集恰好一条审计");
        let audit = &first.context.audit[0];
        assert_eq!(audit.action, AuditAction::DeviceAuthenticated);
        assert_eq!(audit.target, EntityRef::Device(device.clone()));
        assert!(
            matches!(&audit.actor, Actor::Device { device: id } if id == &device),
            "归因是该设备本身"
        );

        // 已有更早的 `last_seen_at`：本层照常提交 `Some(now)`（第二次调用拿 `ts(1)`），**不得**改用
        // `None` 抹掉已存值、也不得回填已存值；「取较大者」由存储层判定（`storage-sqlite` 的
        // `timestamps_are_advanced_never_erased` 覆盖三分支）。
        trust.devices.lock().expect("lock")[0] = device_record(
            &device,
            DeviceState::Active,
            Some(crate::broker::test_support::ts(9)),
            None,
        );
        block_on(fixture.use_cases.record_device_connected(&device)).expect("record connected");
        let writes = trust.writes.lock().expect("lock").clone();
        assert_eq!(writes.len(), 2);
        assert_eq!(
            writes[1].record.last_seen_at(),
            Some(&crate::broker::test_support::ts(1)),
            "每次认证都提交本次时间，是否前进由存储层取较大者"
        );
    }

    /// Sync WP1（任务 1.2）：没有信任行、或该行已撤销时失败关闭——前者零写入（`DeviceWrite` 携带整行，
    /// 行不存在无法只推进一列），后者把原状态原样交给存储层，由存储层的「撤销优先」拒绝，本层不预判。
    #[test]
    fn record_device_connected_fails_closed_without_a_writable_row() {
        let (fixture, trust) = fixture_with_device_trust();
        let missing = DeviceId::new(&uuid_text(221)).expect("device");
        let error = block_on(fixture.use_cases.record_device_connected(&missing))
            .expect_err("没有信任行就不能静默成功");
        assert!(
            matches!(&error, PortError::NotFound(EntityRef::Device(id)) if id == &missing),
            "得到 {error:?}"
        );
        assert!(
            trust.writes.lock().expect("lock").is_empty(),
            "零写入：没有可用整行、也没有状态变更需要留痕"
        );

        let revoked = DeviceId::new(&uuid_text(222)).expect("device");
        trust.devices.lock().expect("lock").push(device_record(
            &revoked,
            DeviceState::Revoked,
            None,
            Some(crate::broker::test_support::ts(3)),
        ));
        let error = block_on(fixture.use_cases.record_device_connected(&revoked))
            .expect_err("已撤销的设备行不得被推进");
        assert!(
            matches!(&error, PortError::InvalidRequest(code) if *code == "use device.revoke to revoke a device"),
            "撤销优先由存储层判定，得到 {error:?}"
        );
        let writes = trust.writes.lock().expect("lock").clone();
        assert_eq!(
            writes.last().expect("写集已到达存储层").record.state(),
            DeviceState::Revoked,
            "本层原样透传撤销状态：不静默复活、也不把已撤销行说成 active"
        );
    }

    /// Sync WP1（任务 1.5）：**已知**设备认证失败写出 `device.auth_failed`——动作/目标/结果正确、归因是该
    /// 设备本身，且这条留痕零写入（既不推进 `last_seen_at`、也不改状态）。
    ///
    /// 设备方向不再有授权集合（`pairing-model`），归因因此只剩 id：本入口不读设备行、也不依赖行里的任何
    /// 其它内容（这正是它不必读那一行的原因），写出的一行与「已知/未知」无关。
    #[test]
    fn record_device_auth_failure_writes_one_row_attributed_to_the_device() {
        let (fixture, trust) = fixture_with_device_trust();
        let device = DeviceId::new(&uuid_text(231)).expect("device");
        trust.devices.lock().expect("lock").push(device_record(
            &device,
            DeviceState::Active,
            None,
            None,
        ));

        block_on(fixture.use_cases.record_device_auth_failure(&device)).expect("留痕");

        let written = fixture.world.audits.lock().expect("lock").clone();
        assert_eq!(written.len(), 1, "一次失败恰好一条审计");
        let record = &written[0];
        assert_eq!(record.action(), AuditAction::DeviceAuthFailed);
        assert_eq!(*record.target(), EntityRef::Device(device.clone()));
        assert_eq!(record.outcome(), AuditOutcome::Failed);
        assert_eq!(record.via_node(), None, "via_node 是 Node Link 路径的列");
        assert_eq!(record.local_principal_ref(), None);
        assert_eq!(record.detail_digest(), None, "细分失败原因不进审计行");
        assert!(
            matches!(record.actor(), Actor::Device { device: id } if id == &device),
            "归因是该设备本身，得到 {:?}",
            record.actor()
        );
        assert!(
            trust.writes.lock().expect("lock").is_empty(),
            "留痕没有关联状态变更：不写设备行"
        );
    }

    /// Sync WP1（任务 1.5）：**未知**设备的认证失败同样留痕——不是 `NotFound`、不是零写入；归因只按
    /// `deviceId`（设备方向没有授权集合，所以不存在「取自握手的自报能力」的余地）。它与已知设备返回
    /// **同一个** `Ok`、写出形状相同的一行，因此响应里没有可用于探测设备是否存在的差异；这是本入口与
    /// [`UseCases::record_device_connected`]「行不存在即 `NotFound`」的刻意差异（那个入口要写整行，本入口
    /// 只追加一行审计）。
    #[test]
    fn record_device_auth_failure_records_an_unknown_device_without_self_reported_values() {
        let (fixture, trust) = fixture_with_device_trust();
        let known = DeviceId::new(&uuid_text(232)).expect("device");
        trust.devices.lock().expect("lock").push(device_record(
            &known,
            DeviceState::Active,
            None,
            None,
        ));
        let unknown = DeviceId::new(&uuid_text(233)).expect("device");

        let known_result = block_on(fixture.use_cases.record_device_auth_failure(&known));
        let unknown_result = block_on(fixture.use_cases.record_device_auth_failure(&unknown));
        assert!(
            known_result.is_ok() && unknown_result.is_ok(),
            "两种情形都必须返回 Ok：未知设备不得以 NotFound 收场（那会让调用方无处留痕），也不得给出可探测存在性的差异"
        );

        let written = fixture.world.audits.lock().expect("lock").clone();
        assert_eq!(written.len(), 2, "未知设备同样留痕，而不是零写入");
        let record = &written[1];
        assert_eq!(record.action(), AuditAction::DeviceAuthFailed);
        assert_eq!(*record.target(), EntityRef::Device(unknown.clone()));
        assert_eq!(record.outcome(), AuditOutcome::Failed);
        assert_eq!(record.via_node(), None);
        assert_eq!(record.local_principal_ref(), None);
        assert_eq!(record.detail_digest(), None);
        assert!(
            matches!(record.actor(), Actor::Device { device: id } if id == &unknown),
            "未知设备只能按自报 id 归因，得到 {:?}",
            record.actor()
        );
        assert!(
            trust.writes.lock().expect("lock").is_empty(),
            "留痕没有关联状态变更：不写设备行"
        );
    }

    /// WP5（任务 2.13）：catalog 投影源只读持久事实：信任行、全部 Export 与**审计水位**（`catalogRevision`
    /// 的来源，不是 `store.head()`）；未知节点是空视图而不是错误；零写入。
    #[test]
    fn node_link_catalog_view_reads_the_audit_watermark() {
        let fixture = fixture();
        let node = NodeId::new(&uuid_text(121)).expect("node");
        fixture.world.nodes.lock().expect("lock").push(node_record(
            &node,
            NodeKind::Access,
            NodeState::Paired,
        ));
        fixture
            .world
            .exports
            .lock()
            .expect("lock")
            .push(export_record(
                "export-visible",
                &["agent-1"],
                &["grant.observe"],
                false,
            ));
        for _ in 0..3 {
            fixture.world.audits.lock().expect("lock").push(
                AuditRecord::try_new(
                    crate::broker::test_support::ts(0),
                    AuditAction::ExportCreated,
                    Actor::LocalCli,
                    None,
                    None,
                    EntityRef::Export(ExportId::new("export-visible").expect("export id")),
                    AuditOutcome::Success,
                    None,
                )
                .expect("审计行"),
            );
        }

        let view = block_on(fixture.use_cases.node_link_catalog_view(&node)).expect("catalog view");
        assert_eq!(view.revision, 3, "revision 取自审计水位");
        assert_eq!(
            view.node.as_ref().map(NodeRecord::state),
            Some(NodeState::Paired)
        );
        assert_eq!(view.exports.len(), 1);
        assert_eq!(view.exports[0].export_id().as_str(), "export-visible");

        let unknown = NodeId::new(&uuid_text(122)).expect("node");
        let empty = block_on(fixture.use_cases.node_link_catalog_view(&unknown)).expect("空视图");
        assert!(empty.node.is_none(), "未知节点不用错误区分存在性");
        assert_eq!(empty.revision, 3, "水位与节点无关");

        assert!(
            fixture.world.write_audits.lock().expect("lock").is_empty(),
            "catalog 投影是零写入的"
        );
    }

    /// WP5（任务 2.14）：会话读的归属前置——对端必须是已配对的 `access` 行、Export 必须存在且未撤销、
    /// 会话的 Agent 必须属于该 Export；epoch 不一致的重放一律拒绝（不从错误位置重放）。
    #[test]
    fn node_link_session_reads_are_bound_to_the_node_and_export() {
        let fixture = fixture();
        let node = NodeId::new(&uuid_text(123)).expect("node");
        fixture.world.nodes.lock().expect("lock").push(node_record(
            &node,
            NodeKind::Access,
            NodeState::Paired,
        ));
        fixture
            .world
            .exports
            .lock()
            .expect("lock")
            .push(export_record(
                "export-a",
                &["agent-1"],
                &["grant.observe"],
                false,
            ));
        fixture
            .world
            .exports
            .lock()
            .expect("lock")
            .push(export_record(
                "export-revoked",
                &["agent-1"],
                &["grant.observe"],
                true,
            ));
        fixture
            .world
            .exports
            .lock()
            .expect("lock")
            .push(export_record(
                "export-other-agent",
                &["agent-2"],
                &["grant.observe"],
                false,
            ));
        let session = fixture.session.clone();
        let export = ExportId::new("export-a").expect("export id");

        let view = block_on(
            fixture
                .use_cases
                .node_link_session_view(&node, &export, &session),
        )
        .expect("会话视图");
        assert_eq!(view.session.session_id(), &session);
        assert_eq!(view.head.origin_sequence.get(), 0, "尚无事件的会话序列为 0");
        assert!(view.pending_interactions.is_empty());

        // 未知 Export / 未知会话 → NotFound（适配器映射为 export.not_found）。
        let unknown_export = ExportId::new("export-none").expect("export id");
        assert!(matches!(
            block_on(
                fixture
                    .use_cases
                    .node_link_session_view(&node, &unknown_export, &session)
            ),
            Err(PortError::NotFound(EntityRef::Export(_)))
        ));
        let unknown_session = SessionId::new(&uuid_text(999)).expect("session");
        assert!(matches!(
            block_on(
                fixture
                    .use_cases
                    .node_link_session_view(&node, &export, &unknown_session)
            ),
            Err(PortError::NotFound(EntityRef::Session(_)))
        ));

        // 已撤销 Export 与「Agent 不属于该 Export」→ 拒绝且不改语义。
        for rejected in ["export-revoked", "export-other-agent"] {
            let export = ExportId::new(rejected).expect("export id");
            assert!(
                matches!(
                    block_on(
                        fixture
                            .use_cases
                            .node_link_session_view(&node, &export, &session)
                    ),
                    Err(PortError::InvalidRequest("export.not_granted"))
                ),
                "{rejected} 必须被拒"
            );
        }

        // 没有 `access` 行（或行未配对）时一律 `authorization.scope_denied`。
        let unknown_node = NodeId::new(&uuid_text(124)).expect("node");
        assert!(matches!(
            block_on(
                fixture
                    .use_cases
                    .node_link_session_view(&unknown_node, &export, &session)
            ),
            Err(PortError::InvalidRequest("authorization.scope_denied"))
        ));

        // 重放的 epoch 不一致 → 显式拒绝。
        let wrong_epoch = OriginCursor {
            origin_epoch: OriginEpoch::new(&uuid_text(777)).expect("epoch"),
            origin_sequence: Sequence::new(0).expect("sequence"),
        };
        assert!(matches!(
            block_on(fixture.use_cases.node_link_replay(
                &node,
                &export,
                &session,
                Some(wrong_epoch),
                ReplayLimit::new(10),
            )),
            Err(PortError::InvalidRequest("nodelink.origin_epoch_mismatch"))
        ));
    }

    /// WP5（任务 2.14/2.15）：origin 重放按 `origin_sequence > after` 取事件含正文，head 是该会话的
    /// 最大 origin 序列；事件正文读取按会话归属收窄。
    #[test]
    fn node_link_replay_returns_origin_events_with_payloads() {
        let fixture = fixture();
        let node = NodeId::new(&uuid_text(131)).expect("node");
        fixture.world.nodes.lock().expect("lock").push(node_record(
            &node,
            NodeKind::Access,
            NodeState::Paired,
        ));
        fixture
            .world
            .exports
            .lock()
            .expect("lock")
            .push(export_record(
                "export-replay",
                &["agent-1"],
                &["grant.observe"],
                false,
            ));
        let session = fixture.session.clone();
        let epoch = {
            let state = crate::broker::test_support::lock(&fixture.world.state);
            state
                .epochs
                .get(session.as_str())
                .cloned()
                .expect("fixture 必须为会话写入 origin epoch")
        };
        let first = EventId::new(&uuid_text(141)).expect("event");
        let second = EventId::new(&uuid_text(142)).expect("event");
        {
            let mut state = crate::broker::test_support::lock(&fixture.world.state);
            for (index, id) in [&first, &second].into_iter().enumerate() {
                let sequence = Sequence::new(index as u64 + 1).expect("sequence");
                state.events.push(CommittedEvent {
                    id: id.clone(),
                    event_type: EventType::new("session.created").expect("event type"),
                    session: Some(session.clone()),
                    session_sequence: Some(sequence),
                    global_sequence: sequence,
                    origin_epoch: Some(epoch.clone()),
                    origin_sequence: Some(sequence),
                    created_at: crate::broker::test_support::ts(index as u32),
                });
                state.event_payloads.insert(
                    id.as_str().to_owned(),
                    EventPayload::new(
                        ViewJson::new(&format!(r#"{{"index":{index}}}"#)).expect("view"),
                        None,
                    ),
                );
            }
        }

        let export = ExportId::new("export-replay").expect("export id");
        let replay = block_on(fixture.use_cases.node_link_replay(
            &node,
            &export,
            &session,
            None,
            ReplayLimit::new(10),
        ))
        .expect("重放");
        assert_eq!(replay.events.len(), 2);
        assert_eq!(replay.events[0].origin_sequence.get(), 1);
        assert_eq!(replay.events[1].event_id, second);
        assert_eq!(replay.head.origin_epoch, epoch);
        assert_eq!(replay.head.origin_sequence.get(), 2);

        // 非空 cursor：只取该点之后的事件，且按 origin 序列比较。
        let after = OriginCursor {
            origin_epoch: epoch.clone(),
            origin_sequence: Sequence::new(1).expect("sequence"),
        };
        let resumed = block_on(fixture.use_cases.node_link_replay(
            &node,
            &export,
            &session,
            Some(after),
            ReplayLimit::new(10),
        ))
        .expect("增量重放");
        assert_eq!(resumed.events.len(), 1);
        assert_eq!(resumed.events[0].event_id, second);

        // 正文读取按会话归属收窄：事件存在但不属于该会话时为 `None`。
        let other_session = SessionId::new(&uuid_text(998)).expect("session");
        assert!(
            block_on(fixture.use_cases.node_link_event_payload(
                &node,
                &export,
                &other_session,
                &first
            ))
            .is_err(),
            "会话不存在时正文读取失败关闭（先过归属前置）"
        );
        assert!(
            block_on(
                fixture
                    .use_cases
                    .node_link_event_payload(&node, &export, &session, &first)
            )
            .expect("正文读取")
            .is_some()
        );
    }

    /// 测试用 Export 记录（可见性策略在适配器，core 只校验「会话属于该 Export」）。
    fn export_record(id: &str, agents: &[&str], grants: &[&str], revoked: bool) -> ExportRecord {
        let alias = WorkspaceAlias::new("project").expect("alias");
        ExportRecord::try_new(
            ExportId::new(id).expect("export id"),
            "team export",
            agents
                .iter()
                .map(|agent| AgentId::new(agent).expect("agent id"))
                .collect(),
            vec![
                crate::model::WorkspaceAliasEntry::try_new(alias.clone(), "Project")
                    .expect("alias entry"),
            ],
            alias.clone(),
            vec![
                crate::model::ExportTemplate::try_new(
                    crate::model::TemplateId::new("coding").expect("template id"),
                    "Coding",
                    alias.clone(),
                    Vec::new(),
                )
                .expect("template"),
            ],
            crate::model::TemplateId::new("coding").expect("template id"),
            crate::model::GrantSet::try_from_iter(grants).expect("grants"),
            crate::model::CachePolicy::NoContentCache,
            crate::broker::test_support::ts(0),
            if revoked {
                Some(crate::broker::test_support::ts(1))
            } else {
                None
            },
        )
        .expect("export record")
    }

    /// 任务 2.28：握手审计入口只追加 `node.authenticated`/`node.auth_failed`，归因与目标都是该对端；
    /// 其它动作一律 `authorization.scope_denied` 且不写任何行。
    #[test]
    fn node_link_auth_audit_is_narrow() {
        let fixture = fixture();
        let node = NodeId::new(&uuid_text(111)).expect("node");
        for (action, outcome) in [
            (AuditAction::NodeAuthenticated, AuditOutcome::Success),
            (AuditAction::NodeAuthFailed, AuditOutcome::Failed),
        ] {
            block_on(
                fixture
                    .use_cases
                    .record_node_link_auth(&node, action, outcome),
            )
            .expect("握手两个动作必须可写");
        }
        let written = fixture.world.audits.lock().expect("lock").clone();
        assert_eq!(
            written.iter().map(AuditRecord::action).collect::<Vec<_>>(),
            vec![AuditAction::NodeAuthenticated, AuditAction::NodeAuthFailed]
        );
        let record = &written[1];
        assert_eq!(
            record.actor(),
            &Actor::Node {
                node: node.clone(),
                access_node: node.clone()
            },
            "归因按本次连接声明的对端"
        );
        assert_eq!(record.via_node(), Some(&node));
        assert_eq!(
            *record.target(),
            crate::model::EntityRef::Node(node.clone())
        );
        assert_eq!(record.outcome(), AuditOutcome::Failed);
        assert_eq!(
            record.local_principal_ref(),
            None,
            "节点级信任模型下为 None"
        );

        let before = written.len();
        let error = block_on(fixture.use_cases.record_node_link_auth(
            &node,
            AuditAction::NodeTrustRevoked,
            AuditOutcome::Denied,
        ))
        .expect_err("其它动作不得经本入口写入");
        assert!(matches!(
            &error,
            PortError::InvalidRequest(code) if *code == "authorization.scope_denied"
        ));
        assert_eq!(fixture.world.audits.lock().expect("lock").len(), before);

        // WP6（任务 2.17）：适配层比 broker 更严的越权拒绝路径需要留痕，因此该入口也接受
        // `authorization.denied`（仍不开口成通用审计写面）。
        block_on(fixture.use_cases.record_node_link_auth(
            &node,
            AuditAction::AuthorizationDenied,
            AuditOutcome::Denied,
        ))
        .expect("越权拒绝留痕必须可写");
        let denied = fixture.world.audits.lock().expect("lock").clone();
        assert_eq!(denied.len(), before + 1);
        assert_eq!(denied[before].action(), AuditAction::AuthorizationDenied);
        assert_eq!(denied[before].outcome(), AuditOutcome::Denied);
        assert_eq!(denied[before].via_node(), Some(&node));
    }

    /// WP6（任务 2.17）：Owner 侧 `Actor::Node` 的**无会话命令**（`session.list`/`command.status`/
    /// `session.create`）必须有可判定的授权路径——原先 `node_allowed` 在 `session = None` 时恒拒，
    /// 这些命令在 Owner 侧永远不可用（§6 第 5 条的裁决）。
    #[test]
    fn owner_side_node_commands_without_a_session_use_the_trust_record_and_a_covering_export() {
        let fixture = fixture();
        let node = NodeId::new(&uuid_text(124)).expect("node");
        let actor = Actor::Node {
            node: node.clone(),
            access_node: node.clone(),
        };
        // 信任行 grants 与 Export scopes 的交集只有 `grant.observe`：观察类命令可用，
        // `grant.remote-work` 类命令不可用；该行点名的 Export 清单同时是授权的第三个条件。
        fixture.world.nodes.lock().expect("lock").push(paired_node(
            &node,
            &["grant.observe"],
            &["export-observe"],
        ));
        fixture
            .world
            .exports
            .lock()
            .expect("lock")
            .push(export_record(
                "export-observe",
                &["agent-1"],
                &["grant.observe"],
                false,
            ));

        // 无会话命令：已配对 + grants 含所需 grant + 存在覆盖该命令的 Export → 通过。
        let summaries = block_on(fixture.use_cases.list_sessions(
            &actor,
            SessionQuery {
                only: None,
                states: Vec::new(),
                limit: None,
            },
        ))
        .expect("session.list 必须对 Owner 侧节点可用");
        assert_eq!(
            summaries.len(),
            1,
            "返回本机 owned 摘要（适配层再按可见 Export 过滤）"
        );
        assert!(
            block_on(
                fixture
                    .use_cases
                    .command_status(&actor, RequestId::new(&uuid_text(950)).expect("uuid"),)
            )
            .expect("command.status 必须对 Owner 侧节点可用")
            .is_none(),
            "未提交过的 requestId 没有记录"
        );

        // 信任记录 grants 不含该 grant → 拒绝（`grant.observe` 不在 remote-work 节点的 grants 里）。
        let remote_work = NodeId::new(&uuid_text(125)).expect("node");
        fixture.world.nodes.lock().expect("lock").push(paired_node(
            &remote_work,
            &["grant.remote-work"],
            &["export-observe"],
        ));
        let error = block_on(fixture.use_cases.list_sessions(
            &Actor::Node {
                node: remote_work.clone(),
                access_node: remote_work,
            },
            SessionQuery {
                only: None,
                states: Vec::new(),
                limit: None,
            },
        ))
        .expect_err("授予面不含该命令时失败关闭");
        assert!(matches!(
            error,
            PortError::InvalidRequest(reason) if reason == "authorization.scope_denied"
        ));

        // 没有信任行 / 行已撤销 / 没有覆盖该命令的 Export：三种情况都失败关闭。
        let unknown = NodeId::new(&uuid_text(126)).expect("node");
        let revoked = NodeId::new(&uuid_text(127)).expect("node");
        fixture.world.nodes.lock().expect("lock").push(node_record(
            &revoked,
            NodeKind::Access,
            NodeState::Revoked,
        ));
        for candidate in [unknown, revoked] {
            assert!(
                block_on(fixture.use_cases.list_sessions(
                    &Actor::Node {
                        node: candidate.clone(),
                        access_node: candidate,
                    },
                    SessionQuery {
                        only: None,
                        states: Vec::new(),
                        limit: None,
                    },
                ))
                .is_err(),
                "未知与已撤销的节点都不得通过"
            );
        }
        fixture.world.exports.lock().expect("lock").clear();
        assert!(
            block_on(fixture.use_cases.list_sessions(
                &actor,
                SessionQuery {
                    only: None,
                    states: Vec::new(),
                    limit: None,
                },
            ))
            .is_err(),
            "没有任何覆盖该命令的 Export 时失败关闭"
        );
    }

    /// 用户 2026-09-27 裁决的收窄型白名单（`design.md` D1 的条件③）：Owner 侧授权必须要求目标 Export
    /// 在该信任记录的 `export_ids` 内，且**清单为空时即使 `scopes ∩ grants` 非空也不可用**
    /// （与 `node_link::catalog::visible_exports` 同口径）。
    #[test]
    fn owner_side_node_authorization_requires_the_export_to_be_nominated() {
        let fixture = fixture();
        let node = NodeId::new(&uuid_text(131)).expect("node");
        let actor = Actor::Node {
            node: node.clone(),
            access_node: node.clone(),
        };
        // 两个 Export 的 scopes 都与信任行 grants 相交（条件②恒成立），差别只在清单（条件③）
        // 与它们覆盖的 agent：两个不可用都只能是条件③造成的。
        fixture
            .world
            .exports
            .lock()
            .expect("lock")
            .push(export_record(
                "export-covered",
                &["agent-1"],
                &["grant.observe"],
                false,
            ));
        fixture
            .world
            .exports
            .lock()
            .expect("lock")
            .push(export_record(
                "export-other-agent",
                &["agent-2"],
                &["grant.observe"],
                false,
            ));
        let read = |actor: &Actor| {
            block_on(fixture.use_cases.read_session(
                actor,
                HistoryQuery {
                    session: fixture.session.clone(),
                    include: HistoryInclude {
                        messages: false,
                        turns: false,
                        pending_interactions: false,
                        config_options: false,
                        capabilities: false,
                        history: false,
                    },
                    before_history: None,
                    after: None,
                    limit: ReplayLimit::default(),
                },
            ))
        };
        let seed_node = |export_ids: &[&str]| {
            let mut nodes = fixture.world.nodes.lock().expect("lock");
            nodes.retain(|record| record.node_id() != &node);
            nodes.push(paired_node(&node, &["grant.observe"], export_ids));
        };

        // ① 清单点名覆盖该会话 agent 的 Export → 通过。
        seed_node(&["export-covered"]);
        read(&actor).expect("清单内且 scopes ∩ grants 非空时必须可用");

        // ② 清单不含该 Export（只列了另一个）→ 不可用，即使它的 scopes ∩ grants 非空。
        seed_node(&["export-other-agent"]);
        let error = read(&actor).expect_err("清单外的 Export 不得成为授权来源");
        assert!(
            matches!(
                error,
                PortError::InvalidRequest(reason) if reason == "authorization.scope_denied"
            ),
            "与目录层同口径：不可见即不可用"
        );

        // ③ 空清单 → 不可用（方案 A：空清单 = 看不到任何 Export，不得默认放权）。
        seed_node(&[]);
        let error = read(&actor).expect_err("空清单时即使 scopes ∩ grants 非空也不得授权");
        assert!(matches!(
            error,
            PortError::InvalidRequest(reason) if reason == "authorization.scope_denied"
        ));
    }

    /// design D12：claim/status 在 `LocalCli` 之外只接受**绑定该配对**的 `Actor::PairingClaimant`；
    /// 绑定不一致与其它 actor 都是同一种 `authorization.scope_denied`（不泄露配对存在性）。
    #[test]
    fn pairing_channel_accepts_only_the_bound_claimant() {
        let fixture = fixture();
        let pairing = PairingId::new(&uuid_text(61)).expect("pairing");
        let bound = Actor::PairingClaimant {
            pairing: pairing.clone(),
        };
        let other = Actor::PairingClaimant {
            pairing: PairingId::new(&uuid_text(62)).expect("pairing"),
        };
        let device = Actor::Device {
            device: DeviceId::new(&uuid_text(63)).expect("device"),
        };
        seed_pairing(
            &fixture.world,
            &pairing,
            PairingTarget::Device,
            PairingState::PendingConfirmation,
            peer(PeerIdentity::Device(
                DeviceId::new(&uuid_text(63)).expect("device"),
            )),
        );

        // 绑定一致：授权通过，读回该配对的状态行。
        let status = block_on(fixture.use_cases.pairing(&bound, &pairing))
            .expect("claimant status")
            .expect("seeded pairing");
        assert_eq!(status.state(), PairingState::PendingConfirmation);
        assert_eq!(status.id(), &pairing);

        // 本机入口保持原行为。
        assert!(
            block_on(fixture.use_cases.pairing(&Actor::LocalCli, &pairing))
                .expect("local status")
                .is_some()
        );

        for denied in [&other, &device] {
            let error = block_on(fixture.use_cases.pairing(denied, &pairing))
                .expect_err("未绑定该配对的 actor 必须被拒");
            assert!(
                matches!(
                    &error,
                    PortError::InvalidRequest(code) if *code == "authorization.scope_denied"
                ),
                "拒绝形状必须与 require_local 一致，得到 {error:?}"
            );
        }

        // claim 走同一套访问规则：绑定一致时由端口给出结果（fake 的 claim 恒为 NotFound），
        // 绑定不一致时在授权层就被挡住，连端口都不会调用。
        let claim = PairingClaim::try_new(
            pairing.clone(),
            peer(PeerIdentity::Device(
                DeviceId::new(&uuid_text(63)).expect("device"),
            )),
            ScopeSet::empty(),
            crate::model::GrantSet::empty(),
        )
        .expect("claim");
        assert!(matches!(
            block_on(fixture.use_cases.claim_pairing(&bound, claim.clone())),
            Err(PortError::NotFound(_))
        ));
        let error = block_on(fixture.use_cases.claim_pairing(&other, claim.clone()))
            .expect_err("绑定不一致的 claimant 必须被拒");
        assert!(matches!(
            &error,
            PortError::InvalidRequest(code) if *code == "authorization.scope_denied"
        ));
        assert!(
            fixture.world.write_audits.lock().expect("lock").is_empty(),
            "被拒的认领不得产生任何写集审计"
        );
    }

    /// design D12（seam 补全）：配对通道的只读视图只对绑定该配对的 claimant 开放，不含任何写入；
    /// 未认领的配对没有对端行，未知配对 id 返回 `None`（而不是错误）。
    #[test]
    fn pairing_channel_view_is_bound_to_the_claimant() {
        let fixture = fixture();
        let node_id = NodeId::new(&uuid_text(66)).expect("node");
        let pairing = PairingId::new(&uuid_text(67)).expect("pairing");
        seed_pairing(
            &fixture.world,
            &pairing,
            PairingTarget::Node,
            PairingState::PendingConfirmation,
            peer(PeerIdentity::Node(node_id.clone())),
        );
        let bound = Actor::PairingClaimant {
            pairing: pairing.clone(),
        };
        let view = block_on(fixture.use_cases.pairing_channel_view(&bound, &pairing))
            .expect("claimant view")
            .expect("seeded pairing");
        assert_eq!(view.record.state(), PairingState::PendingConfirmation);
        assert_eq!(
            view.peer.as_ref().map(crate::model::PairingPeer::id),
            Some(&PeerIdentity::Node(node_id))
        );
        assert!(view.node.is_none(), "未批准时没有授权集可读");

        // 本机入口保持可读；其它 actor 与绑定别的配对的 claimant 都是同一种拒绝。
        assert!(
            block_on(
                fixture
                    .use_cases
                    .pairing_channel_view(&Actor::LocalCli, &pairing)
            )
            .expect("local view")
            .is_some()
        );
        let other = Actor::PairingClaimant {
            pairing: PairingId::new(&uuid_text(68)).expect("pairing"),
        };
        let device = Actor::Device {
            device: DeviceId::new(&uuid_text(69)).expect("device"),
        };
        for denied in [&other, &device] {
            let error = block_on(fixture.use_cases.pairing_channel_view(denied, &pairing))
                .expect_err("未绑定该配对的 actor 必须被拒");
            assert!(
                matches!(&error, PortError::InvalidRequest(code) if *code == "authorization.scope_denied"),
                "拒绝形状必须与 require_local 一致，得到 {error:?}"
            );
        }

        // 未知配对 id：`None`，不是错误（端点据此区分 404 与 401）。
        let unknown = PairingId::new(&uuid_text(70)).expect("pairing");
        let bound_unknown = Actor::PairingClaimant {
            pairing: unknown.clone(),
        };
        assert!(
            block_on(
                fixture
                    .use_cases
                    .pairing_channel_view(&bound_unknown, &unknown)
            )
            .expect("unknown pairing")
            .is_none()
        );
        assert!(
            fixture.world.write_audits.lock().expect("lock").is_empty(),
            "只读视图不产生任何写集审计"
        );
    }

    /// design D12（seam 补全）：`approved`/`consumed` 时视图带上该对端的 `access` 节点行
    /// （`grant.*` 的唯一来源）；同一对端的 `owner` 行属于另一个方向，不被选中。
    #[test]
    fn pairing_channel_view_exposes_the_granted_grants_after_approval() {
        let fixture = fixture();
        let node_id = NodeId::new(&uuid_text(76)).expect("node");
        let pairing = PairingId::new(&uuid_text(77)).expect("pairing");
        seed_pairing(
            &fixture.world,
            &pairing,
            PairingTarget::Node,
            PairingState::Approved,
            peer(PeerIdentity::Node(node_id.clone())),
        );
        fixture.world.nodes.lock().expect("lock").push(node_record(
            &node_id,
            NodeKind::Access,
            NodeState::Paired,
        ));
        fixture.world.nodes.lock().expect("lock").push(node_record(
            &node_id,
            NodeKind::Owner,
            NodeState::Paired,
        ));

        let bound = Actor::PairingClaimant {
            pairing: pairing.clone(),
        };
        let view = block_on(fixture.use_cases.pairing_channel_view(&bound, &pairing))
            .expect("claimant view")
            .expect("seeded pairing");
        let node = view
            .node
            .expect("已批准的配对必须带出该对端的 access 节点行");
        assert_eq!(node.kind(), NodeKind::Access);
        assert_eq!(
            node.grants().iter().collect::<Vec<&str>>(),
            vec!["grant.remote-work"]
        );
    }

    /// design D12：`consume_pairing` 把 `approved` 推进到 `consumed`（`terminal_at` = 当次写入时刻）并
    /// 把认证留痕写进同一写集；对端一致的重复调用幂等成功，且不覆盖首次 `terminal_at`、不重复写审计。
    #[test]
    fn consume_pairing_advances_approved_and_is_idempotent() {
        let fixture = fixture();
        let device = DeviceId::new(&uuid_text(71)).expect("device");
        let actor = Actor::Device {
            device: device.clone(),
        };
        let pairing = PairingId::new(&uuid_text(72)).expect("pairing");
        seed_pairing(
            &fixture.world,
            &pairing,
            PairingTarget::Device,
            PairingState::Approved,
            peer(PeerIdentity::Device(device.clone())),
        );

        let consumed = block_on(fixture.use_cases.consume_pairing(&actor, &pairing))
            .expect("consume approved pairing");
        assert_eq!(consumed.state(), PairingState::Consumed);
        let consumed_at = crate::broker::test_support::ts(0);
        assert_eq!(
            consumed.terminal_at(),
            Some(&consumed_at),
            "terminal_at 取本次写集的 at"
        );
        assert_eq!(
            *fixture.world.write_audits.lock().expect("lock"),
            vec![AuditAction::DeviceAuthenticated],
            "设备路径写 device.authenticated"
        );

        let again =
            block_on(fixture.use_cases.consume_pairing(&actor, &pairing)).expect("幂等重试成功");
        assert_eq!(again.state(), PairingState::Consumed);
        assert_eq!(
            again.terminal_at(),
            consumed.terminal_at(),
            "不覆盖首次时间"
        );
        assert_eq!(
            *fixture.world.write_audits.lock().expect("lock"),
            vec![AuditAction::DeviceAuthenticated],
            "幂等命中不重复写审计"
        );
    }

    /// design D12：节点路径写 `node.authenticated`。
    #[test]
    fn consume_pairing_audits_node_authentication() {
        let fixture = fixture();
        let node = NodeId::new(&uuid_text(81)).expect("node");
        let actor = Actor::Node {
            node: node.clone(),
            access_node: NodeId::new(&uuid_text(82)).expect("access"),
        };
        let pairing = PairingId::new(&uuid_text(83)).expect("pairing");
        seed_pairing(
            &fixture.world,
            &pairing,
            PairingTarget::Node,
            PairingState::Approved,
            peer(PeerIdentity::Node(node)),
        );

        let consumed =
            block_on(fixture.use_cases.consume_pairing(&actor, &pairing)).expect("consume");
        assert_eq!(consumed.state(), PairingState::Consumed);
        assert_eq!(
            *fixture.world.write_audits.lock().expect("lock"),
            vec![AuditAction::NodeAuthenticated]
        );
    }

    /// design D12：只接受与该配对已批准对端一致的设备/节点 actor；claimant、本机入口、对端错配一律拒绝。
    #[test]
    fn consume_pairing_rejects_other_actors_and_peers() {
        let fixture = fixture();
        let device = DeviceId::new(&uuid_text(91)).expect("device");
        let pairing = PairingId::new(&uuid_text(92)).expect("pairing");
        seed_pairing(
            &fixture.world,
            &pairing,
            PairingTarget::Device,
            PairingState::Approved,
            peer(PeerIdentity::Device(device.clone())),
        );
        let claimant = Actor::PairingClaimant {
            pairing: pairing.clone(),
        };
        for denied in [&Actor::LocalCli, &claimant] {
            let error = block_on(fixture.use_cases.consume_pairing(denied, &pairing))
                .expect_err("非设备/节点主体必须被拒");
            assert!(matches!(
                &error,
                PortError::InvalidRequest(code) if *code == "authorization.scope_denied"
            ));
        }

        // 类别对但 id 不同 / 类别不同：对端不符。
        let wrong_device = Actor::Device {
            device: DeviceId::new(&uuid_text(93)).expect("device"),
        };
        assert!(matches!(
            block_on(fixture.use_cases.consume_pairing(&wrong_device, &pairing)),
            Err(PortError::Conflict(ConflictKind::IdentityMismatch))
        ));
        let wrong_node = Actor::Node {
            node: NodeId::new(&uuid_text(91)).expect("node"),
            access_node: NodeId::new(&uuid_text(94)).expect("access"),
        };
        assert!(matches!(
            block_on(fixture.use_cases.consume_pairing(&wrong_node, &pairing)),
            Err(PortError::Conflict(ConflictKind::IdentityMismatch))
        ));
        assert!(
            fixture.world.write_audits.lock().expect("lock").is_empty(),
            "被拒的消费不得产生任何写集审计"
        );
    }

    /// design D12：状态机——未批准拒、已终结具名拒、配对或对端行缺失各自具名。
    #[test]
    fn consume_pairing_rejects_unapproved_and_terminal_states() {
        let fixture = fixture();
        let device = DeviceId::new(&uuid_text(101)).expect("device");
        let actor = Actor::Device {
            device: device.clone(),
        };
        let pending = PairingId::new(&uuid_text(102)).expect("pairing");
        seed_pairing(
            &fixture.world,
            &pending,
            PairingTarget::Device,
            PairingState::PendingConfirmation,
            peer(PeerIdentity::Device(device.clone())),
        );
        let error = block_on(fixture.use_cases.consume_pairing(&actor, &pending))
            .expect_err("未批准的配对不可消费");
        assert!(
            matches!(
                &error,
                PortError::InvalidRequest(code) if *code == "pairing has not been approved"
            ),
            "得到 {error:?}"
        );

        let expired = PairingId::new(&uuid_text(103)).expect("pairing");
        seed_pairing(
            &fixture.world,
            &expired,
            PairingTarget::Device,
            PairingState::Expired,
            peer(PeerIdentity::Device(device.clone())),
        );
        assert!(matches!(
            block_on(fixture.use_cases.consume_pairing(&actor, &expired)),
            Err(PortError::Conflict(ConflictKind::Expired))
        ));

        let missing = PairingId::new(&uuid_text(104)).expect("pairing");
        assert!(matches!(
            block_on(fixture.use_cases.consume_pairing(&actor, &missing)),
            Err(PortError::NotFound(EntityRef::Pairing(id))) if id == missing
        ));

        // 配对在但没有对端行：库被外部改写（§8 的失败关闭）。
        let headless = PairingId::new(&uuid_text(105)).expect("pairing");
        fixture.world.pairings.lock().expect("lock").push(
            PairingRecord::try_new(
                headless.clone(),
                PairingTarget::Device,
                PairingState::Approved,
                None,
                ScopeSet::empty(),
                crate::model::GrantSet::empty(),
                digest('p'),
                HOST_BINDING,
                crate::broker::test_support::ts(0),
                crate::broker::test_support::ts(600),
                Some(crate::broker::test_support::ts(0)),
                Some(crate::broker::test_support::ts(1)),
                None,
            )
            .expect("pairing record"),
        );
        assert!(matches!(
            block_on(fixture.use_cases.consume_pairing(&actor, &headless)),
            Err(PortError::Corrupt(_))
        ));
        assert!(
            fixture.world.write_audits.lock().expect("lock").is_empty(),
            "没有任何一条被拒路径可以产生写集审计"
        );
    }
}
