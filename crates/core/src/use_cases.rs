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
//! 其余 actor 一律 `authorization.scope_denied`。

use std::sync::Arc;

use crate::broker::{Broker, Denied, command_name};
use crate::model::{
    Actor, AgentDescriptor, AgentId, AgentProfile, AgentRef, AttachmentGeneration, AttachmentId,
    AuditAction, AuditOutcome, AuditRecord, CapabilitySet, ClientCommand, CommandKind,
    CommandReceipt, CommandRecord, ConfigOption, ConfigOptionId, ConfigValue, CreateSessionRequest,
    DeviceId, DeviceRecord, ElicitationAction, ElicitationValues, EntityRef, ExportId,
    ExportRecord, GlobalCursor, ImportId, ImportRecord, InteractionId, InteractionResolution,
    LocalCursor, ModeId, ModeState, NodeId, NodeKind, NodeRecord, NodeState, OwnedSessionRef,
    PairingClaim, PairingId, PairingRecord, PairingSettlement, PairingTarget, PeerIdentity,
    PeerPublicKey, PortError, ProviderRef, RequestId, Resolution, ResolvedWorkspace, SeedState,
    Sequence, SessionId, SessionReference, SessionSummary, Timestamp, UnavailableKind, Version,
    WorkspaceAlias, WorkspaceRecord,
};
use crate::ports::{
    AgentCatalog, AttachmentRef, AttachmentStore, AuditQuery, AuditStore, Clock,
    DeliveryIndexEntry, DeviceRevocation, DeviceWrite, ExpiryWrite, ExportRevocation, ExportStore,
    ExportWrite, HistoryPage, HistoryQuery, IdGenerator, ImportRemoval, ImportWrite,
    LocalConfigStore, NodeRevocation, NodeWrite, PairingClaimOutcome, PairingClaimWrite,
    PairingSettlementWrite, PairingWrite, PendingAudit, ProfileWrite, ProviderRefWrite,
    PruneReport, RemoteDeliveryStore, ReplayBatch, ReplayLimit, RetentionPolicy, RevokeReason,
    SeedWrite, SessionQuery, SessionStore, StoreHealth, TrustRecordRef, TrustStore, WorkspaceWrite,
    WriteContext,
};

/// `session.mode.list` 的结果：端口返回的 `ModeState` + 会话当前 `Version`（§6 第 17 条）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeListing {
    pub state: ModeState,
    pub version: Version,
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
    pub async fn claim_pairing(
        &self,
        actor: &Actor,
        claim: PairingClaim,
    ) -> Result<PairingClaimOutcome, PortError> {
        self.require_local(actor)?;
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

    pub async fn pairing(
        &self,
        actor: &Actor,
        id: &PairingId,
    ) -> Result<Option<PairingRecord>, PortError> {
        self.require_local(actor)?;
        self.trust.pairing(id).await
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
        AgentId, AgentRef, CommandKind, CommandPayload, PairingState, ResourceOrigin, ScopeSet,
    };
    use crate::ports::HistoryInclude;

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
        let root = std::env::temp_dir().join(format!("acpr-ws-{}", uuid_text(11)));
        std::fs::create_dir_all(&root).expect("create dir");
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
}
