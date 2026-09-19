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
    Actor, AgentDescriptor, AgentRef, AttachmentGeneration, AttachmentId, AuditAction,
    AuditOutcome, AuditRecord, CapabilitySet, ClientCommand, CommandKind, CommandReceipt,
    CommandRecord, ConfigOption, ConfigOptionId, ConfigValue, CreateSessionRequest, DeviceId,
    DeviceRecord, ElicitationAction, ElicitationValues, EntityRef, ExportId, ExportRecord,
    GlobalCursor, ImportId, ImportRecord, InteractionId, InteractionResolution, LocalCursor,
    ModeId, ModeState, NodeId, NodeRecord, OwnedSessionRef, PairingClaim, PairingId, PairingRecord,
    PairingSettlement, PortError, RequestId, Resolution, Sequence, SessionId, SessionReference,
    SessionSummary, Timestamp, Version,
};
use crate::ports::{
    AgentCatalog, AttachmentRef, AttachmentStore, AuditQuery, AuditStore, Clock,
    DeliveryIndexEntry, ExportStore, HistoryPage, HistoryQuery, IdGenerator, PairingClaimOutcome,
    PruneReport, RemoteDeliveryStore, ReplayBatch, ReplayLimit, RetentionPolicy, RevokeReason,
    SessionQuery, SessionStore, StoreHealth, TrustRecordRef, TrustStore,
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

    /// `session.create`（Node Link，§12.7）：创建 owned 会话并打开其后端端点。
    pub async fn create_session(
        &self,
        actor: &Actor,
        request: CreateSessionRequest,
    ) -> Result<SessionId, PortError> {
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

    pub async fn upsert_device(
        &self,
        actor: &Actor,
        record: DeviceRecord,
    ) -> Result<(), PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        self.trust.upsert_device(record, at).await
    }

    /// `device.revoke`（`LOCAL_ADMIN_PROTOCOL.md` §5.3）：撤销 + 按 §3.5 记审计。
    pub async fn revoke_device(&self, actor: &Actor, id: &DeviceId) -> Result<(), PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        self.trust
            .revoke_device(id, at.clone(), RevokeReason::UserRequested)
            .await?;
        self.audit_action(actor, AuditAction::DeviceRevoked, k_device(id), &at)
            .await;
        Ok(())
    }

    pub async fn nodes(&self, actor: &Actor) -> Result<Vec<NodeRecord>, PortError> {
        self.require_local(actor)?;
        self.trust.nodes().await
    }

    pub async fn node(&self, actor: &Actor, id: &NodeId) -> Result<Option<NodeRecord>, PortError> {
        self.require_local(actor)?;
        self.trust.node(id).await
    }

    pub async fn upsert_node(&self, actor: &Actor, record: NodeRecord) -> Result<(), PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        self.trust.upsert_node(record, at).await
    }

    /// `node.revoke`（`LOCAL_ADMIN_PROTOCOL.md` §5.4）。
    pub async fn revoke_node(&self, actor: &Actor, id: &NodeId) -> Result<(), PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        self.trust
            .revoke_node(id, at.clone(), RevokeReason::UserRequested)
            .await?;
        self.audit_action(actor, AuditAction::NodeTrustRevoked, k_node(id), &at)
            .await;
        Ok(())
    }

    /// `device.pair.begin` / `node.pair.begin`：登记一次性配对。
    pub async fn create_pairing(
        &self,
        actor: &Actor,
        pairing: PairingRecord,
    ) -> Result<(), PortError> {
        self.require_local(actor)?;
        let target = k_pairing(&pairing);
        self.trust.create_pairing(pairing).await?;
        let at = self.clock.now();
        self.audit_action(actor, AuditAction::PairingCreated, target, &at)
            .await;
        Ok(())
    }

    /// 原子认领（HMAC 由调用方验证，`SYNC_PROTOCOL.md` §7.2）。
    pub async fn claim_pairing(
        &self,
        actor: &Actor,
        claim: PairingClaim,
    ) -> Result<PairingClaimOutcome, PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        let outcome = self.trust.claim_pairing(claim, at.clone()).await?;
        self.audit_action(
            actor,
            AuditAction::PairingClaimed,
            k_pairing(&outcome.pairing),
            &at,
        )
        .await;
        Ok(outcome)
    }

    pub async fn pairing(
        &self,
        actor: &Actor,
        id: &PairingId,
    ) -> Result<Option<PairingRecord>, PortError> {
        self.require_local(actor)?;
        self.trust.pairing(id).await
    }

    /// `device.pair.confirm` / `node.pair.confirm`：落定并创建信任记录。
    pub async fn settle_pairing(
        &self,
        actor: &Actor,
        id: &PairingId,
        settlement: PairingSettlement,
    ) -> Result<TrustRecordRef, PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        let settled = self
            .trust
            .settle_pairing(id, settlement, at.clone())
            .await?;
        let target = match &settled {
            TrustRecordRef::Device(device) => k_device(device),
            TrustRecordRef::Node(node) => k_node(node),
        };
        self.audit_action(actor, AuditAction::PairingApproved, target, &at)
            .await;
        Ok(settled)
    }

    pub async fn expire_pairings(&self, actor: &Actor) -> Result<u64, PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        self.trust.expire_pairings(at).await
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

    pub async fn upsert_export(
        &self,
        actor: &Actor,
        export: ExportRecord,
    ) -> Result<(), PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        self.exports.upsert_export(export, at).await
    }

    /// `export.revoke`（`LOCAL_ADMIN_PROTOCOL.md` §5.5）：立即拒绝该 Export 的新命令与订阅。
    pub async fn revoke_export(&self, actor: &Actor, id: &ExportId) -> Result<(), PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        self.exports.revoke_export(id, at).await
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

    pub async fn upsert_import(
        &self,
        actor: &Actor,
        import: ImportRecord,
    ) -> Result<(), PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        self.exports.upsert_import(import, at).await
    }

    /// `import.remove`：删本地 Import 引用；交付索引与命令引用由
    /// [`crate::ports::RemoteDeliveryStore::drop_import`] 清掉，审计行保留。
    pub async fn remove_import(&self, actor: &Actor, id: &ImportId) -> Result<(), PortError> {
        self.require_local(actor)?;
        let at = self.clock.now();
        self.deliveries.drop_import(id).await?;
        self.exports.remove_import(id, at).await
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

    async fn audit_action(
        &self,
        actor: &Actor,
        action: AuditAction,
        target: EntityRef,
        at: &Timestamp,
    ) {
        let via_node = match actor {
            Actor::Node { node, .. } => Some(node.clone()),
            _ => None,
        };
        let record = AuditRecord::try_new(
            at.clone(),
            action,
            actor.clone(),
            via_node,
            None,
            target,
            AuditOutcome::Success,
            None,
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::broker::test_support::{
        FakeAttachments, FakeDeliveries, FakeExports, FakeStore, FakeTrust, FakeWorld, TestAudit,
        TestCatalog, TestClock, TestIds, TestPublisher, block_on, digest, prompt_command,
        uuid_text,
    };
    use crate::broker::{Broker, BrokerConfig, BrokerDeps, QueuePolicy};
    use crate::model::{AgentId, AgentRef, CommandKind, CommandPayload, ResourceOrigin, ScopeSet};
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
            trust: Arc::new(FakeTrust),
            audit: Arc::new(TestAudit {
                world: world.clone(),
            }),
            attachments: Arc::new(FakeAttachments {
                world: world.clone(),
            }),
            catalog: Arc::new(TestCatalog),
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
}
