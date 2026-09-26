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
    CommandReceipt, CommandRecord, ConfigOption, ConfigOptionId, ConfigValue, ConflictKind,
    CreateSessionRequest, DeviceId, DeviceRecord, ElicitationAction, ElicitationValues, EntityRef,
    EventId, EventPayload, EventType, ExportId, ExportRecord, GlobalCursor, ImportId, ImportRecord,
    InteractionId, InteractionResolution, LocalCursor, ModeId, ModeState, NodeId, NodeKind,
    NodeRecord, NodeState, OriginCursor, OriginEpoch, OwnedSessionRef, PairingClaim, PairingId,
    PairingRecord, PairingSettlement, PairingState, PairingTarget, PeerIdentity, PeerPublicKey,
    PendingInteraction, PortError, ProviderRef, RequestId, Resolution, ResolvedWorkspace,
    SeedState, Sequence, SessionId, SessionReference, SessionSummary, Timestamp, UnavailableKind,
    Version, WorkspaceAlias, WorkspaceRecord,
};
use crate::ports::{
    AgentCatalog, AttachmentRef, AttachmentStore, AuditQuery, AuditStore, Clock,
    DeliveryIndexEntry, DeviceRevocation, DeviceWrite, ExpiryWrite, ExportRevocation, ExportStore,
    ExportWrite, HistoryPage, HistoryQuery, IdGenerator, ImportRemoval, ImportWrite,
    LocalConfigStore, NodeConnectedWrite, NodeRevocation, NodeWrite, PairingClaimOutcome,
    PairingClaimWrite, PairingConsumption, PairingSettlementWrite, PairingWrite, PendingAudit,
    ProfileWrite, ProviderRefWrite, PruneReport, RemoteDeliveryStore, ReplayBatch, ReplayLimit,
    RetentionPolicy, RevokeReason, SeedWrite, SessionQuery, SessionStore, StoreHealth,
    TrustRecordRef, TrustStore, WorkspaceWrite, WriteContext,
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
        }
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

    /// `session.create`（Node Link，§12.7）：先把 workspace 别名解析成本机规范化绝对路径，
    /// 再创建 owned 会话并打开其后端端点（§11.9）。
    ///
    /// `workspace_alias` 是该请求在 Export 中声明的别名——「是否在该 Export 的别名集合内」由
    /// `server::node_link` 校验（`nodelink.export.not_granted`，参数类）；本层只负责本机解析：
    /// 已声明但本机解析失败 MUST 返回 [`UnavailableKind::IoError`]，**不得**降级为参数错误。
    ///
    /// UNC/网络路径允许使用；core 不持日志设施，因此「网络路径」的结构化警告由接入层在解析成功
    /// 后记录，本层不因它改变授权模型（`design.md` D6）。
    pub async fn create_session(
        &self,
        actor: &Actor,
        mut request: CreateSessionRequest,
        workspace_alias: Option<WorkspaceAlias>,
    ) -> Result<SessionId, PortError> {
        // 授权先于任何本机读取与文件系统访问（与其余用例入口同款；broker 内部还会再授权一次，
        // 对本地 actor 恒成功、不重复写审计）。否则未授权调用方能借解析结果的差异探测「别名是否
        // 已登记、目录当前是否存在」——那是一个本机状态预言机。
        let request_id = self.ids.request_id();
        self.broker
            .authorize(actor, "session.create", None, &request_id)
            .await
            .map_err(Denied::into_port_error)?;
        if let Some(alias) = workspace_alias {
            let record = self
                .config
                .workspace(&alias)
                .await?
                .ok_or(PortError::InvalidRequest(
                    "workspace alias is not registered on this node",
                ))?;
            request.workspace = Some(resolve_workspace(&alias, record.canonical_path())?);
        }
        self.broker.create_session(actor, request).await
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
    /// 只有「已存行的 scopes 发生变化」才写 `device.scopes_changed`：没有登记「设备新建」类安全动作，
    /// 由配对确认路径负责写 `pairing.approved`。
    pub async fn put_device(&self, actor: &Actor, record: DeviceRecord) -> Result<(), PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        let existing = self.trust.device(record.device_id()).await?;
        let audits = match &existing {
            Some(old) if old.scopes() != record.scopes() => vec![self.pending_audit(
                actor,
                AuditAction::DeviceScopesChanged,
                k_device(record.device_id()),
            )],
            _ => Vec::new(),
        };
        self.trust
            .put_device(DeviceWrite {
                record,
                context: WriteContext { at, audit: audits },
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

    /// Node Link 握手的审计留痕（`SECURITY_DESIGN.md` §14.2 的 `node.authenticated`/`node.auth_failed`）。
    ///
    /// 只接受这两个动作（其余一律 `authorization.scope_denied`）：握手是安全动作、成败都要留痕，但这个
    /// 入口不是通用审计写面。归因一律按 Node Link 对端——`actor` 与 `via_node` 都是该对端 id，
    /// `target = Node(对端)`，`localPrincipalRef` 为 `None`（节点级信任模型，§8.3），`detailDigest` 为空
    /// （握手的细分失败原因只在结构化日志里，不进审计行）。
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
            AuditAction::NodeAuthenticated | AuditAction::NodeAuthFailed
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
    /// 「这个 Export 是否对该节点可见」（`design.md` D4 的 `exportIds` 口径）不在 core 判定：它是
    /// 适配器的**单点可见性策略**（待用户裁决，见 WP5 handoff）；core 只守住「sessionRef 确实属于该
    /// Export」这条硬底线，避免把 `(exportId, sessionId)` 当成可任意组合的读取钥匙。
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::broker::test_support::{
        FakeAttachments, FakeDeliveries, FakeExports, FakeLocalConfig, FakeStore, FakeTrust,
        FakeWorld, TestAudit, TestCatalog, TestClock, TestIds, TestPublisher, block_on, digest,
        prompt_command, uuid_text,
    };
    use crate::broker::{Broker, BrokerConfig, BrokerDeps, QueuePolicy};
    use crate::model::{
        AgentId, AgentRef, CommandKind, CommandPayload, CommittedEvent, Nonce, PairingState,
        ResourceOrigin, ScopeSet, ViewJson,
    };
    use crate::ports::HistoryInclude;

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
                backends: Arc::new(crate::broker::test_support::FakeBackend {
                    world: world.clone(),
                }),
                exports: Arc::new(FakeExports {
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

        Fixture {
            use_cases,
            world,
            session,
        }
    }

    /// §4/§6.5：查询命令也走授权，且不产生副作用；`LocalCli` 是本地管理入口。
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

        let denied = Actor::Device {
            device: DeviceId::new(&uuid_text(41)).expect("uuid"),
            scopes: ScopeSet::empty(),
        };
        let error = block_on(fixture.use_cases.submit_command(&denied, command)).expect_err("拒绝");
        assert!(
            matches!(error, PortError::InvalidRequest(reason) if reason == "authorization.scope_denied"),
            "拒绝必须来自 core 的 scope 判定"
        );
        let audits = fixture.world.audits();
        assert_eq!(audits.len(), 1, "拒绝记一条审计");
        assert_eq!(
            audits[0].action(),
            crate::model::AuditAction::AuthorizationDenied
        );
        assert_eq!(audits[0].outcome(), crate::model::AuditOutcome::Denied);
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
                },
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

        let other = Actor::Device {
            device: DeviceId::new(&uuid_text(42)).expect("uuid"),
            scopes: ScopeSet::empty(),
        };
        let hidden = block_on(fixture.use_cases.command_status(&other, request)).expect_err("拒绝");
        assert!(matches!(hidden, PortError::InvalidRequest(_)));
    }

    /// §11.9：别名 → 规范化绝对路径；相对路径、含 `..` 的输入、不存在路径与非目录一律 `IoError`。
    #[test]
    fn workspace_resolution_canonicalizes_and_rejects_invalid_inputs() {
        let alias = WorkspaceAlias::new("repo").expect("alias");
        let root = temp_dir(&format!("acpr-ws-{}", uuid_text(11)));
        let resolved = resolve_workspace(&alias, root.to_str().expect("path")).expect("resolve");
        assert_eq!(resolved.alias(), &alias);
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

    /// §11.9：未登记的别名是参数类错误，且不得触达后端。
    #[test]
    fn create_session_rejects_unregistered_workspace_alias() {
        let fixture = fixture();
        let error = block_on(fixture.use_cases.create_session(
            &Actor::LocalCli,
            create_request(),
            Some(WorkspaceAlias::new("ghost").expect("alias")),
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
            create_request(),
            Some(alias),
        ))
        .expect_err("a missing workspace directory must fail");
        assert!(matches!(
            error,
            PortError::Unavailable(UnavailableKind::IoError)
        ));
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

        let actor = Actor::Device {
            device: DeviceId::new(&uuid_text(51)).expect("device"),
            scopes: ScopeSet::empty(),
        };
        let error = block_on(fixture.use_cases.create_session(
            &actor,
            create_request(),
            Some(alias.clone()),
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
            scopes: ScopeSet::empty(),
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
            scopes: ScopeSet::empty(),
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
            scopes: ScopeSet::empty(),
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
            scopes: ScopeSet::empty(),
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
            scopes: ScopeSet::empty(),
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
