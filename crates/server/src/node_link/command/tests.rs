//! `server::node_link::command` 的行为用例（`[PV3]` 的 WP6 部分）。
//!
//! 前半是纯函数的契约用例（禁带字段判定、绝对路径形态、终态映射、错误码映射）；后半用真实
//! `ConnectionHandle` 的出站队列驱动 [`CommandRoute`]：不看内部状态，只看对端真的收到的那几帧。
//!
//! 端口替身原则与 WP5 的用例一致：只实现本轮路由**真的**触及的方法，其余一律 `unreachable!`。
//! `CommandStore::commit` 只支持 create——命令 mutation 的落盘路径（`session.prompt` 的 turn 事件）由
//! [PV5] 的受控路径全链路测试（真实 SQLite）承担；本轮用幂等命中与 adapter 级拒绝覆盖路由行为，因此
//! 「没有第二次派发」这条断言由 `commit` 的 `unreachable!` 直接证明。

use std::net::{IpAddr, Ipv4Addr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use acp_core::broker::{Broker, BrokerConfig, BrokerDeps};
use acp_core::model::{
    CachePolicy, CommandResult, ConflictKind, Digest, EventId, EventPayload, ExportTemplate,
    GlobalCursor, GrantSet, NodeKind, NodeRecord, OriginCursor as CoreOriginCursor, OriginEpoch,
    OwnedSessionRef, PromptRequest, Sequence, ServerEpoch, Session, SessionReference, SessionState,
    SessionSummary, TemplateId, WorkspaceAlias, WorkspaceAliasEntry as CoreAliasEntry,
};
use acp_core::ports::{
    AgentCatalog, AuditQuery, CommitOutcome, EventSink, HistoryPage, HistoryQuery, NodeLinkSlice,
    OwnedCommit, ReplayBatch, ReplayLimit, SessionBackendFactory, SessionEndpoint, SessionQuery,
    SessionStore, StateChange, StoreHealth, TurnAccepted,
};
use acp_core::use_cases::{UseCaseDeps, UseCases};
use identity_auth::KeyHandle;
use node_link_protocol::command::CommandSubmit as WireSubmit;
use node_link_protocol::common::{DecimalString, Uuid};
use node_link_protocol::envelope::ConnectionFields;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::local_admin::test_support::{
    FakeAudit, FakeClock, FakeConfig, FakeEntropy, FakeExports, FakeIds, FakeKeystore, FakeTrust,
    NOW, NotTouched, TEST_SERVER_EPOCH, test_public_key,
};
use crate::node_link::conn::registry::Outbound;
use crate::node_link::conn::{ConnectionHandle, ConnectionRegistry, NodeLinkConfig, SessionLimits};

use super::*;

/// 测试扮演的 Access Node。
const ACCESS_NODE: &str = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";
/// 本机（Owner）node id：与 `Authority::local_node` 同源。
const OWNER_NODE: &str = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
/// Export（覆盖 `codex`）。
const EXPORT: &str = "11111111-1111-4111-8111-111111111111";
/// 已存在的会话。
const SESSION: &str = "3ae1c07c-9242-46e9-a9d2-4ec58c130f4a";
/// 本次连接。
const CONNECTION: &str = "5ae1c07c-9242-46e9-a9d2-4ec58c130f4c";
/// 终态事件（`command.terminal.terminalEventId` 的来源）。
const EVENT: &str = "4ae1c07c-9242-46e9-a9d2-4ec58c130f4b";
/// 一个稳定的 requestId（同一用例内重复提交用同一个）。
const REQUEST: &str = "7ae1c07c-9242-46e9-a9d2-4ec58c130f4e";
/// 另一个 requestId。
const REQUEST_2: &str = "7ae1c07c-9242-46e9-a9d2-4ec58c130f4f";
const ORIGIN_EPOCH: &str = "018f6f89-8a23-7a10-a0d3-f92e6a31d952";
const ORIGIN_EPOCH_2: &str = "018f6f89-8a23-7a10-a0d3-f92e6a31d953";

fn ts(text: &str) -> CoreTimestamp {
    CoreTimestamp::new(text).expect("固定时间戳")
}

fn session_id() -> SessionId {
    SessionId::new(SESSION).expect("session id")
}

fn export_id() -> CoreExportId {
    CoreExportId::new(EXPORT).expect("export id")
}

fn agent_ref() -> AgentRef {
    AgentRef::try_new(AgentId::new("codex").expect("agent id"), "Codex").expect("agent ref")
}

fn digest_of(text: &str) -> Digest {
    use base64::Engine as _;
    let bytes = Sha256::digest(text.as_bytes());
    Digest::new(&base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)).expect("摘要文本")
}

fn session_summary() -> SessionSummary {
    SessionSummary::try_new(
        session_id(),
        Some("Session".to_owned()),
        agent_ref(),
        SessionState::Idle,
        ResourceOrigin::Local,
        None,
        Version::new(3),
        ts("2026-09-18T09:00:00.000Z"),
        ts("2026-09-18T09:12:00.000Z"),
    )
    .expect("会话摘要")
}

fn session_of(summary: &SessionSummary) -> Session {
    Session::try_new(
        summary.session_id().clone(),
        OwnedSessionRef::new(summary.session_id().clone()),
        summary.title().map(str::to_owned),
        summary.agent().clone(),
        summary.state(),
        summary.origin().clone(),
        None,
        summary.version(),
        summary.created_at().clone(),
        summary.updated_at().clone(),
        None,
    )
    .expect("会话聚合")
}

/// 端口替身：命令记录表 + 会话表。`find_request` 只按 requestId 文本查（真实存储还含 actor 维度，
/// 本轮用例每条 request 只属于一个 actor）。
///
/// `commit` 按 §6 第 20 条模拟真实存储的 `session.create` 语义：创建提交 = 新会话 + 幂等行（`session_id`
/// 回填新建会话）、终态提交 = 把既有行推进到终态、同键重放 = 回首次结果、同键不同指纹 = 冲突。
#[derive(Clone, Default)]
struct CommandStore {
    commands: Arc<Mutex<BTreeMap<String, CommandRecord>>>,
    sessions: Arc<Mutex<Vec<SessionSummary>>>,
    commits: Arc<Mutex<Vec<OwnedCommit>>>,
    commit_calls: Arc<AtomicUsize>,
}

impl CommandStore {
    fn seed_command(&self, record: CommandRecord) {
        lock(&self.commands).insert(record.request().as_str().to_owned(), record);
    }

    fn seed_session(&self, summary: SessionSummary) {
        lock(&self.sessions).push(summary);
    }

    fn commit_calls(&self) -> usize {
        self.commit_calls.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl SessionStore for CommandStore {
    async fn commit(&self, commit: OwnedCommit) -> Result<CommitOutcome, PortError> {
        self.commit_calls.fetch_add(1, Ordering::SeqCst);
        lock(&self.commits).push(commit.clone());
        // 幂等命中（§6 第 6 条与第 20 条）：五项比对，`session = None` 不参与（`session.create` 的
        // 目标会话由存储层分配并回填）。
        if let Some(idem) = commit.idempotency.as_ref() {
            if let Some(existing) = lock(&self.commands).get(idem.request.as_str()).cloned() {
                let same = existing.command() == idem.command
                    && existing.kind() == idem.kind
                    && idem
                        .session
                        .as_ref()
                        .is_none_or(|session| existing.session() == Some(session))
                    && existing.expected_version() == idem.expected_version
                    && existing.request_fingerprint() == &idem.request_fingerprint;
                if !same {
                    return Err(PortError::Conflict(ConflictKind::IdempotencyConflict));
                }
                return Ok(CommitOutcome {
                    session_id: existing.session().cloned(),
                    origin_epoch: Some(OriginEpoch::new(ORIGIN_EPOCH_2).expect("epoch")),
                    version: Version::new(1),
                    appended: Vec::new(),
                    replayed: Some(acp_core::ports::IdempotentReplay { record: existing }),
                });
            }
        }
        let Some(StateChange::Create(new)) = &commit.state else {
            // 终态提交：按同一批 `command.*` 事件的 causation 定位行，推进到终态。
            let request = commit
                .events
                .iter()
                .rev()
                .find(|event| event.event_type.as_str().starts_with("command."))
                .and_then(|event| event.causation.clone())
                .ok_or(PortError::InvalidRequest("终态提交缺少 command.* 事件"))?;
            let terminal = commit
                .command_terminal
                .clone()
                .ok_or(PortError::InvalidRequest("终态提交缺少 command_terminal"))?;
            let existing =
                lock(&self.commands)
                    .get(request.as_str())
                    .cloned()
                    .ok_or(PortError::NotFound(EntityRef::Command {
                        session: commit.session.clone(),
                        request: request.clone(),
                    }))?;
            if existing.status().is_terminal() {
                return Ok(CommitOutcome {
                    session_id: existing.session().cloned(),
                    origin_epoch: Some(OriginEpoch::new(ORIGIN_EPOCH_2).expect("epoch")),
                    version: Version::new(1),
                    appended: Vec::new(),
                    replayed: Some(acp_core::ports::IdempotentReplay { record: existing }),
                });
            }
            let updated = CommandRecord::try_new(
                existing.session().cloned(),
                existing.request().clone(),
                existing.command(),
                existing.kind(),
                existing.actor().clone(),
                existing.accepted_at().cloned(),
                terminal.status(),
                terminal.terminal_at().cloned(),
                Some(EventId::new(EVENT).expect("终态事件 id")),
                terminal.result().cloned(),
                terminal.error().cloned(),
                existing.expected_version(),
                existing.request_fingerprint().clone(),
            )?;
            self.seed_command(updated);
            return Ok(CommitOutcome {
                session_id: commit.session.clone(),
                origin_epoch: Some(OriginEpoch::new(ORIGIN_EPOCH_2).expect("epoch")),
                version: Version::new(1),
                appended: Vec::new(),
                replayed: None,
            });
        };
        // 会话身份永远由 Owner（存储层）在提交事务内分配。
        let session = SessionId::new("8ae1c07c-9242-46e9-a9d2-4ec58c130f50").expect("session id");
        self.seed_session(
            SessionSummary::try_new(
                session.clone(),
                new.title.clone(),
                new.agent.clone(),
                SessionState::Idle,
                ResourceOrigin::Local,
                None,
                Version::new(1),
                commit.at.clone(),
                commit.at.clone(),
            )
            .expect("新建会话摘要"),
        );
        // 幂等行：`session` 由创建方的 `None` 回填为本次分配的会话 id（§6 第 20 条）。
        if let Some(idem) = commit.idempotency.as_ref() {
            self.seed_command(
                CommandRecord::try_new(
                    Some(session.clone()),
                    idem.request.clone(),
                    &idem.command,
                    idem.kind,
                    idem.actor.clone(),
                    Some(idem.accepted_at.clone()),
                    CoreStatus::Accepted,
                    None,
                    None,
                    None,
                    None,
                    idem.expected_version,
                    idem.request_fingerprint.clone(),
                )
                .expect("幂等行"),
            );
        }
        Ok(CommitOutcome {
            session_id: Some(session),
            origin_epoch: Some(OriginEpoch::new(ORIGIN_EPOCH_2).expect("epoch")),
            version: Version::new(1),
            appended: Vec::new(),
            replayed: None,
        })
    }

    async fn load(
        &self,
        session: &SessionId,
    ) -> Result<Option<acp_core::model::SessionSnapshot>, PortError> {
        let summaries = lock(&self.sessions).clone();
        Ok(summaries
            .iter()
            .find(|summary| summary.session_id() == session)
            .map(|summary| acp_core::model::SessionSnapshot {
                session: session_of(summary),
                origin_epoch: OriginEpoch::new(ORIGIN_EPOCH).expect("epoch"),
                head: global_cursor(),
            }))
    }

    async fn list(&self, query: SessionQuery) -> Result<Vec<SessionSummary>, PortError> {
        let mut summaries = lock(&self.sessions).clone();
        if let Some(only) = &query.only {
            summaries.retain(|summary| only.contains(summary.session_id()));
        }
        if let Some(limit) = query.limit {
            summaries.truncate(limit as usize);
        }
        Ok(summaries)
    }

    async fn head(&self) -> Result<GlobalCursor, PortError> {
        Ok(global_cursor())
    }

    async fn read_view(&self) -> Result<Box<dyn acp_core::ports::ReadView>, PortError> {
        Ok(Box::new(self.clone()))
    }

    async fn find_request(
        &self,
        request: &CoreRequestId,
        _actor: &Actor,
    ) -> Result<Option<CommandRecord>, PortError> {
        Ok(lock(&self.commands).get(request.as_str()).cloned())
    }

    async fn unsettled_commands(
        &self,
        _limit: ReplayLimit,
    ) -> Result<Vec<CommandRecord>, PortError> {
        Ok(lock(&self.commands)
            .values()
            .filter(|record| {
                record.status() == CoreStatus::Accepted
                    && record.kind() == CommandKind::Mutation
                    && record.terminal_event().is_none()
            })
            .cloned()
            .collect())
    }

    async fn retention_window(
        &self,
        _session: &SessionId,
    ) -> Result<Option<(Sequence, Sequence)>, PortError> {
        unreachable!("WP6 路由用例不查保留窗口")
    }

    async fn prune(
        &self,
        _policy: acp_core::ports::RetentionPolicy,
        _at: CoreTimestamp,
    ) -> Result<acp_core::ports::PruneReport, PortError> {
        unreachable!("WP6 路由用例不清扫")
    }

    async fn health(&self) -> Result<StoreHealth, PortError> {
        unreachable!("WP6 路由用例不查健康")
    }
}

#[async_trait::async_trait]
impl acp_core::ports::ReadView for CommandStore {
    async fn head(&self) -> Result<GlobalCursor, PortError> {
        Ok(global_cursor())
    }

    async fn replay(
        &self,
        _after: Option<GlobalCursor>,
        _limit: ReplayLimit,
    ) -> Result<ReplayBatch, PortError> {
        unreachable!("WP6 路由用例不读 Sync 重放面")
    }

    async fn read_session(&self, query: HistoryQuery) -> Result<HistoryPage, PortError> {
        // 启动恢复要读该会话的未终态 turn（`session.create` 没有 turn，因此这里只回空页面）。
        let summaries = lock(&self.sessions).clone();
        let summary = summaries
            .iter()
            .find(|summary| summary.session_id() == &query.session)
            .cloned()
            .ok_or_else(|| PortError::NotFound(EntityRef::Session(query.session.clone())))?;
        Ok(HistoryPage {
            session: summary,
            events: Vec::new(),
            turns: Vec::new(),
            interactions: Vec::new(),
            config: Vec::new(),
            capabilities: None,
            head: global_cursor(),
            next: None,
        })
    }

    async fn event_payload(&self, _event: &EventId) -> Result<Option<EventPayload>, PortError> {
        unreachable!("WP6 路由用例不按事件 id 取正文")
    }

    /// 快照元数据（`session.create` 的结果与 `resource.attach` 都经它）。
    async fn node_link_slice(
        &self,
        session: &SessionId,
        _after: Option<CoreOriginCursor>,
        _limit: ReplayLimit,
    ) -> Result<NodeLinkSlice, PortError> {
        let summaries = lock(&self.sessions).clone();
        let Some(summary) = summaries
            .iter()
            .find(|summary| summary.session_id() == session)
        else {
            return Err(PortError::NotFound(EntityRef::Session(session.clone())));
        };
        Ok(NodeLinkSlice {
            summary: summary.clone(),
            head: origin_head(),
            pending_interactions: Vec::new(),
            events: Vec::new(),
        })
    }

    async fn session_event_payload(
        &self,
        _session: &SessionId,
        _event: &EventId,
    ) -> Result<Option<EventPayload>, PortError> {
        unreachable!("WP6 路由用例不按会话取事件正文")
    }
}

fn global_cursor() -> GlobalCursor {
    GlobalCursor::new(
        ServerEpoch::new(TEST_SERVER_EPOCH).expect("server epoch"),
        Sequence::new(1).expect("sequence"),
    )
}

fn origin_head() -> CoreOriginCursor {
    CoreOriginCursor::new(
        OriginEpoch::new(ORIGIN_EPOCH).expect("epoch"),
        Sequence::new(0).expect("sequence"),
    )
}

/// 后端替身：`session.create` 只要求一个能给出自身引用的端点。
struct FakeBackends;

#[async_trait::async_trait]
impl SessionBackendFactory for FakeBackends {
    async fn create(
        &self,
        session: &SessionId,
        _request: CreateSessionRequest,
        _sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        Ok(Box::new(FakeEndpoint {
            reference: SessionReference::Owned(OwnedSessionRef::new(session.clone())),
        }))
    }

    async fn open(
        &self,
        reference: SessionReference,
        _sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        Ok(Box::new(FakeEndpoint { reference }))
    }
}

/// Agent 目录替身：只承担 `node_link_catalog_view` 的展示名来源（空目录也合法，名字回退到 agentId）。
struct TestCatalog;

#[async_trait::async_trait]
impl AgentCatalog for TestCatalog {
    async fn agents(&self) -> Result<Vec<acp_core::model::AgentDescriptor>, PortError> {
        Ok(vec![acp_core::model::AgentDescriptor::new(
            agent_ref(),
            true,
            ResourceOrigin::Local,
        )])
    }

    async fn agent_capabilities(
        &self,
        _agent: &AgentRef,
    ) -> Result<acp_core::model::CapabilitySet, PortError> {
        unreachable!("WP6 路由用例不查 agent 能力")
    }
}

struct FakeEndpoint {
    reference: SessionReference,
}

#[async_trait::async_trait]
impl SessionEndpoint for FakeEndpoint {
    fn reference(&self) -> SessionReference {
        self.reference.clone()
    }

    async fn prompt(
        &self,
        _request: PromptRequest,
        _at: CoreTimestamp,
    ) -> Result<TurnAccepted, PortError> {
        unreachable!("WP6 路由用例不派发 turn")
    }

    async fn cancel(&self, _turn: Option<TurnId>) -> Result<(), PortError> {
        unreachable!("WP6 路由用例不取消 turn")
    }

    async fn set_mode(&self, _mode: &ModeId) -> Result<(), PortError> {
        unreachable!("WP6 路由用例不切模式")
    }

    async fn list_config(&self) -> Result<Vec<acp_core::model::ConfigOption>, PortError> {
        unreachable!("WP6 路由用例不改配置")
    }

    async fn modes(&self) -> Result<acp_core::model::ModeState, PortError> {
        unreachable!("WP6 路由用例不改模式")
    }

    async fn set_config(
        &self,
        _id: &ConfigOptionId,
        _value: CoreConfigValue,
    ) -> Result<(), PortError> {
        unreachable!("WP6 路由用例不改配置")
    }

    async fn resolve_interaction(
        &self,
        _interaction: &InteractionId,
        _resolution: acp_core::model::InteractionResolution,
    ) -> Result<(), PortError> {
        unreachable!("WP6 路由用例不解析交互")
    }

    async fn read_history(&self, _query: HistoryQuery) -> Result<HistoryPage, PortError> {
        unreachable!("WP6 路由用例不读后端历史")
    }

    async fn close(&self) -> Result<(), PortError> {
        Ok(())
    }
}

/// 端口替身世界 + 已注册的连接句柄 + 真实路由。
struct Fixture {
    world: World,
    handle: Arc<ConnectionHandle>,
    outbound: mpsc::Receiver<Outbound>,
}

struct World {
    audit: FakeAudit,
    trust: FakeTrust,
    exports: FakeExports,
    authority: Arc<Authority>,
    store: Arc<CommandStore>,
    core: Arc<UseCases>,
    registry: Arc<ConnectionRegistry>,
    resource: Arc<ResourceRoute>,
    clock: FakeClock,
    config: FakeConfig,
}

impl Fixture {
    async fn new() -> Self {
        Self::with_grants(&["grant.observe", "grant.interact", "grant.remote-work"]).await
    }

    /// 该节点的信任记录 grants 与 Export scopes 取同一集合（授权交集的两支都成立或都不成立）。
    async fn with_grants(grants: &[&str]) -> Self {
        Self::build(grants, NodeLinkConfig::default(), true).await
    }

    async fn build(grants: &[&str], config: NodeLinkConfig, seed_workspace: bool) -> Self {
        let clock = FakeClock::new();
        let audit = FakeAudit::default();
        let config_store = FakeConfig::default();
        let trust = FakeTrust::default();
        let exports = FakeExports::default().with_clock(clock.clone());
        let authority = Arc::new(Authority::new(
            NodeId::new(OWNER_NODE).expect("本机 node id"),
            KeyHandle::new("node-identity").expect("key handle"),
            Arc::new(FakeKeystore::default()),
            Arc::new(FakeEntropy::default()),
            Arc::new(clock.clone()),
        ));
        let store = Arc::new(CommandStore::default());
        store.seed_session(session_summary());
        trust.seed_node(
            NodeRecord::try_new(
                NodeId::new(ACCESS_NODE).expect("node id"),
                "Office Access",
                NodeKind::Access,
                test_public_key().fingerprint(),
                GrantSet::try_from_iter(grants.iter().copied()).expect("grants"),
                NodeState::Paired,
                None,
                ts("2026-09-18T09:00:00.000Z"),
                None,
                None,
            )
            .expect("信任行"),
        );
        exports.seed_export(
            ExportRecord::try_new(
                export_id(),
                "Project Export",
                vec![AgentId::new("codex").expect("agent id")],
                vec![
                    CoreAliasEntry::try_new(
                        WorkspaceAlias::new("project").expect("alias"),
                        "Project",
                    )
                    .expect("alias entry"),
                    // 第二个合法 alias：幂等冲突用例要拿「同样合法、但语义不同」的 payload 对比。
                    CoreAliasEntry::try_new(WorkspaceAlias::new("alt").expect("alias"), "Alt")
                        .expect("alias entry"),
                ],
                WorkspaceAlias::new("project").expect("alias"),
                vec![
                    ExportTemplate::try_new(
                        TemplateId::new("template-1").expect("template id"),
                        "Template",
                        WorkspaceAlias::new("project").expect("alias"),
                        Vec::new(),
                    )
                    .expect("template"),
                ],
                TemplateId::new("template-1").expect("template id"),
                GrantSet::try_from_iter(grants.iter().copied()).expect("scopes"),
                CachePolicy::NoContentCache,
                ts("2026-09-18T09:00:00.000Z"),
                None,
            )
            .expect("Export 记录"),
        );
        if seed_workspace {
            // 本机必须登记这些 alias（core 把它们解析成规范化绝对路径），路径取真实存在的临时目录。
            let path = std::env::temp_dir();
            for alias in ["project", "alt"] {
                config_store.seed_workspace(
                    acp_core::model::WorkspaceRecord::try_new(
                        WorkspaceAlias::new(alias).expect("alias"),
                        "Project",
                        path.to_str().expect("临时目录是 UTF-8"),
                        ts("2026-09-18T09:00:00.000Z"),
                        ts("2026-09-18T09:00:00.000Z"),
                    )
                    .expect("workspace 记录"),
                );
            }
        }
        let broker = Arc::new(Broker::new(
            BrokerDeps {
                store: store.clone(),
                deliveries: Arc::new(NotTouched),
                backends: Arc::new(FakeBackends),
                exports: Arc::new(exports.clone()),
                trust: Arc::new(trust.clone()),
                publisher: Arc::new(NotTouched),
                clock: Arc::new(clock.clone()),
                ids: Arc::new(FakeIds::default()),
                audit: Some(Arc::new(audit.clone())),
            },
            BrokerConfig::default(),
        ));
        let core = Arc::new(UseCases::new(UseCaseDeps {
            broker,
            store: store.clone(),
            deliveries: Arc::new(NotTouched),
            exports: Arc::new(exports.clone()),
            trust: Arc::new(trust.clone()),
            audit: Arc::new(audit.clone()),
            config: Arc::new(config_store.clone()),
            attachments: Arc::new(NotTouched),
            catalog: Arc::new(TestCatalog),
            clock: Arc::new(clock.clone()),
            ids: Arc::new(FakeIds::default()),
        }));
        let registry = ConnectionRegistry::new();
        let (handle, outbound) = ConnectionHandle::new(
            Uuid::parse(CONNECTION).expect("connection id"),
            NodeId::new(ACCESS_NODE).expect("node id"),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            SessionLimits::negotiate(&config),
            Arc::new(FakeIds::default()),
        );
        registry.register(Arc::clone(&handle));
        let resource = Arc::new(ResourceRoute::new(
            Arc::clone(&core),
            Arc::clone(&registry),
            authority.local_node().clone(),
        ));
        Self {
            world: World {
                audit,
                trust,
                exports,
                authority,
                store,
                core,
                registry,
                resource,
                clock,
                config: config_store,
            },
            handle,
            outbound,
        }
    }

    fn route(&self) -> Arc<CommandRoute> {
        Arc::new(CommandRoute::new(
            Arc::clone(&self.world.core),
            Arc::clone(&self.world.registry),
            Arc::clone(&self.world.resource),
            Arc::clone(&self.world.authority),
        ))
    }

    /// 重启视角：用**同一份**端口状态重建 core 与路由。进程内的东西（观察表、内存缓存、后端端点）
    /// 全部丢弃，只有持久事实延续——这是「重启后同一 requestId 不重复创建」的观察点。
    fn restarted_route(&self) -> Arc<CommandRoute> {
        let owned = Arc::clone(&self.world.store);
        let store: Arc<dyn SessionStore> = owned.clone();
        let broker = Arc::new(Broker::new(
            BrokerDeps {
                store: Arc::clone(&store),
                deliveries: Arc::new(NotTouched),
                backends: Arc::new(FakeBackends),
                exports: Arc::new(self.world.exports.clone()),
                trust: Arc::new(self.world.trust.clone()),
                publisher: Arc::new(NotTouched),
                clock: Arc::new(self.world.clock.clone()),
                ids: Arc::new(FakeIds::default()),
                audit: Some(Arc::new(self.world.audit.clone())),
            },
            BrokerConfig::default(),
        ));
        let core = Arc::new(UseCases::new(UseCaseDeps {
            broker,
            store: Arc::clone(&store),
            deliveries: Arc::new(NotTouched),
            exports: Arc::new(self.world.exports.clone()),
            trust: Arc::new(self.world.trust.clone()),
            audit: Arc::new(self.world.audit.clone()),
            config: Arc::new(self.world.config.clone()),
            attachments: Arc::new(NotTouched),
            catalog: Arc::new(TestCatalog),
            clock: Arc::new(self.world.clock.clone()),
            ids: Arc::new(FakeIds::default()),
        }));
        let resource = Arc::new(ResourceRoute::new(
            Arc::clone(&core),
            Arc::clone(&self.world.registry),
            self.world.authority.local_node().clone(),
        ));
        Arc::new(CommandRoute::new(
            core,
            Arc::clone(&self.world.registry),
            resource,
            Arc::clone(&self.world.authority),
        ))
    }

    /// 认证后消息的信封（连接字段按 §2.2 必需）。
    fn envelope(&self, message_type: MessageType, body: Value) -> Envelope {
        Envelope::new(
            message_type,
            Uuid::parse("6ae1c07c-9242-46e9-a9d2-4ec58c130f4d").expect("message id"),
            Some(ConnectionFields {
                connection_id: Uuid::parse(CONNECTION).expect("connection id"),
                connection_sequence: DecimalString::parse("1").expect("sequence"),
            }),
            serde_json::value::RawValue::from_string(body.to_string()).expect("body"),
        )
        .expect("信封")
    }

    /// 走完 `resource.attach`，返回 `(attachmentId, attachmentGeneration)`。
    async fn attach(&mut self) -> (String, String) {
        let envelope = self.envelope(
            MessageType::ResourceAttach,
            json!({
                "remoteSessionRef": {
                    "ownerNodeId": OWNER_NODE,
                    "exportId": EXPORT,
                    "sessionId": SESSION,
                },
            }),
        );
        assert_eq!(
            self.world.resource.route(&self.handle, &envelope).await,
            RouteOutcome::Claimed
        );
        let frames = self.drain();
        let attached = of_type(&frames, "resource.attached");
        assert_eq!(attached.len(), 1, "attach 必须回一帧 resource.attached");
        let body = &attached[0]["body"];
        (
            body["attachmentId"]
                .as_str()
                .expect("attachment id")
                .to_owned(),
            body["attachmentGeneration"]
                .as_str()
                .expect("generation")
                .to_owned(),
        )
    }

    /// 排空出站队列（「只看本轮新增的那几帧」）。
    fn drain(&mut self) -> Vec<Frame> {
        let mut frames = Vec::new();
        while let Ok(outbound) = self.outbound.try_recv() {
            frames.push(Frame::new(outbound.text));
        }
        frames
    }

    /// 审计行（经 `AuditStore::query`，与真实实现同一读入口）。
    async fn audits(&self) -> Vec<acp_core::model::AuditRecord> {
        acp_core::ports::AuditStore::query(
            &self.world.audit,
            AuditQuery {
                since: None,
                until: None,
                actions: Vec::new(),
                actor: None,
                target: None,
                limit: None,
            },
        )
        .await
        .expect("审计查询")
    }
}

/// 一帧：原始文本 + 解析后的 JSON。
struct Frame {
    value: Value,
}

impl Frame {
    fn new(text: String) -> Self {
        Self {
            value: serde_json::from_str(text.as_str()).expect("帧是合法 JSON"),
        }
    }
}

impl std::ops::Deref for Frame {
    type Target = Value;

    fn deref(&self) -> &Value {
        &self.value
    }
}

/// 只取类型等于 `expected` 的帧。
fn of_type<'a>(frames: &'a [Frame], expected: &str) -> Vec<&'a Frame> {
    frames
        .iter()
        .filter(|frame| frame["type"] == expected)
        .collect()
}

fn error_code(frame: &Value) -> &str {
    frame["body"]["error"]["code"]
        .as_str()
        .or_else(|| frame["body"]["code"].as_str())
        .expect("错误码")
}

/// `command.submit` 的 body（顶层字段齐全；不适用的一律显式 `null`，§12.5）。
///
/// 是自由函数而不是 `Fixture` 的方法：调用点要同时借 `&mut Fixture`（提交会推进出站队列）与构造
/// body，写成方法会让两次借用重叠。
fn submit_body(request: &str, command: &str, payload: Value) -> Value {
    session_submit_body(request, command, payload, None)
}

/// 会话范围命令的 body（`sessionRef`/`attachmentId`/`attachmentGeneration` 由调用方给出）。
fn session_submit_body(
    request: &str,
    command: &str,
    payload: Value,
    attachment: Option<(&str, &str)>,
) -> Value {
    let scope = matches!(
        command,
        "session.read"
            | "session.mode.list"
            | "session.config.list"
            | "session.prompt"
            | "session.cancel"
            | "elicitation.respond"
            | "session.mode.set"
            | "session.config.set"
            | "permission.resolve"
    );
    let (attachment_id, generation) = match attachment {
        Some((id, generation)) => (json!(id), json!(generation)),
        None => (Value::Null, Value::Null),
    };
    let expected = if matches!(command, "session.mode.set" | "session.config.set") {
        json!("3")
    } else {
        Value::Null
    };
    json!({
        "requestId": request,
        "command": command,
        "sessionRef": if scope {
            json!({ "ownerNodeId": OWNER_NODE, "exportId": EXPORT, "sessionId": SESSION })
        } else {
            Value::Null
        },
        "attachmentId": if scope { attachment_id } else { Value::Null },
        "attachmentGeneration": if scope { generation } else { Value::Null },
        "expectedVersion": expected,
        "payload": payload,
    })
}

/// 从 body 解出 wire 的 `command.submit`（用例要用**同一份解码结果**算幂等指纹）。
fn wire_submit(body: &Value) -> WireSubmit {
    serde_json::from_value(body.clone()).expect("wire 形状合法")
}

/// 提交一条命令并返回本轮新增的帧。
async fn submit(fixture: &mut Fixture, route: &CommandRoute, body: Value) -> Vec<Frame> {
    let envelope = fixture.envelope(MessageType::CommandSubmit, body);
    assert_eq!(
        route.route(&fixture.handle, &envelope).await,
        RouteOutcome::Claimed
    );
    fixture.drain()
}

/// 一条命令记录（`CommandRecord` 的 13 字段，按 §11.2 的相容性填齐）。
struct RecordSpec<'a> {
    request: &'a str,
    status: CoreStatus,
    command: &'static str,
    kind: CommandKind,
    result: Option<&'static str>,
    error: Option<&'static str>,
    terminal_event: bool,
    fingerprint: Digest,
}

fn record(spec: RecordSpec<'_>) -> CommandRecord {
    let at = ts("2026-09-18T09:12:03.412Z");
    let accepted_at = match spec.status {
        CoreStatus::Rejected => None,
        _ => Some(at.clone()),
    };
    let terminal_at = match spec.status {
        CoreStatus::Accepted => None,
        CoreStatus::Rejected => None,
        _ => Some(at.clone()),
    };
    let terminal_event = match (spec.terminal_event, spec.status) {
        (true, CoreStatus::Completed | CoreStatus::Failed | CoreStatus::Uncertain) => {
            Some(EventId::new(EVENT).expect("event id"))
        }
        _ => None,
    };
    let result = spec
        .result
        .map(|view| CommandResult::from_json_text(view).expect("命令结果"));
    let error = spec
        .error
        .map(|code| acp_core::model::PublicError::coded(code, "失败", false).expect("公开错误"));
    CommandRecord::try_new(
        Some(session_id()),
        CoreRequestId::new(spec.request).expect("request id"),
        spec.command,
        spec.kind,
        node_actor(&NodeId::new(ACCESS_NODE).expect("node id")),
        accepted_at,
        spec.status,
        terminal_at,
        terminal_event,
        result,
        error,
        None,
        spec.fingerprint,
    )
    .expect("命令记录")
}

// ---------------------------------------------------------------------------------------------
// 纯函数契约
// ---------------------------------------------------------------------------------------------

/// [R74]/[R72] 的前半：`session.create` 的禁带字段与绝对路径在**严格解码之前**就被识别。
#[test]
fn forbidden_session_create_fields_are_recognised_before_the_typed_decode() {
    let body = |payload: Value| {
        json!({
            "requestId": REQUEST,
            "command": "session.create",
            "sessionRef": null,
            "attachmentId": null,
            "attachmentGeneration": null,
            "expectedVersion": null,
            "payload": payload,
        })
        .to_string()
    };
    let allowed = json!({
        "agentId": "codex",
        "exportId": EXPORT,
        "workspaceAlias": "project",
    });

    for (payload, expected) in [
        (json!({ "cwd": "/tmp", "agentId": "codex" }), Some("cwd")),
        (
            json!({ "agentId": "codex", "mcpServers": {} }),
            Some("mcpServers"),
        ),
        (json!({ "agentId": "codex", "apiKey": "x" }), Some("apiKey")),
        (
            json!({ "agentId": "codex", "exportId": EXPORT, "workspaceAlias": "C:/tmp" }),
            Some("workspaceAlias"),
        ),
        (
            json!({
                "agentId": "codex",
                "exportId": EXPORT,
                "workspaceAlias": "project",
                "templateParams": { "k": 1 },
            }),
            Some("templateParams"),
        ),
        (allowed.clone(), None),
    ] {
        let found = forbidden_session_create_field(&body(payload));
        assert_eq!(
            found.as_ref().map(|(_, _, field)| field.as_str()),
            expected,
            "被拒字段必须与 §12.7 的白名单一致"
        );
        if let Some((request, command, _)) = found {
            assert_eq!(request.as_str(), REQUEST);
            assert_eq!(command, CommandName::SessionCreate);
        }
    }

    // 其他命令不受该白名单约束（它们的形状由各自的 schema 决定）。
    let other = json!({
        "requestId": REQUEST,
        "command": "session.list",
        "sessionRef": null,
        "attachmentId": null,
        "attachmentGeneration": null,
        "expectedVersion": null,
        "payload": {},
    })
    .to_string();
    assert!(forbidden_session_create_field(&other).is_none());

    // 超长键不冒充字段名（`details.field` 的上限是 128）。
    let long_key = "k".repeat(MAX_DETAILS_FIELD + 1);
    let found = forbidden_session_create_field(&body(json!({ long_key.clone(): 1 })));
    assert_eq!(
        found.map(|(_, _, field)| field),
        Some(UNKNOWN_FIELD.to_owned())
    );
}

#[test]
fn absolute_path_shapes_cover_posix_windows_and_unc() {
    for text in [
        "/etc/passwd",
        r"\\server\share",
        "C:/tmp",
        r"D:\tmp",
        "c:\\tmp",
    ] {
        assert!(is_absolute_path(text), "{text} 是绝对路径");
    }
    for text in ["project", "a/b", "C:", "1:/x", ""] {
        assert!(!is_absolute_path(text), "{text} 不是绝对路径");
    }
}

/// [R66] 的终态形状：`completed` 带非空 `result`、其余 status 带 `error`，
/// 且 `command` 取该 request 提交时的命令名。
#[test]
fn terminal_mapping_keeps_the_command_name_and_the_terminal_contract() {
    let at = ts("2026-09-18T09:12:03.412Z");
    let completed = record(RecordSpec {
        request: REQUEST,
        status: CoreStatus::Completed,
        command: "session.prompt",
        kind: CommandKind::Mutation,
        result: Some(r#"{"kind":"turn.completed"}"#),
        error: None,
        terminal_event: true,
        fingerprint: digest_of("a"),
    });
    let body = terminal_body(&completed, &at).expect("可映射");
    assert_eq!(body.command, CommandName::SessionPrompt);
    assert_eq!(body.terminal.status, TerminalStatus::Completed);
    assert_eq!(
        body.terminal.terminal_event_id.as_ref().map(Uuid::as_str),
        Some(EVENT)
    );
    match body.terminal.result.as_ref() {
        Some(CommandResultPayload::Object(object)) => {
            assert_eq!(object.get(), r#"{"kind":"turn.completed"}"#)
        }
        other => panic!("completed 必须带 object result：{other:?}"),
    }
    assert!(body.terminal.error.is_null());

    // 未终结的记录不能映射成终态。
    let accepted = record(RecordSpec {
        request: REQUEST_2,
        status: CoreStatus::Accepted,
        command: "session.prompt",
        kind: CommandKind::Mutation,
        result: None,
        error: None,
        terminal_event: false,
        fingerprint: digest_of("a"),
    });
    assert!(terminal_body(&accepted, &at).is_none());

    // `rejected` 行没有持久化终态时间与事件：时间回退到本次应答，`terminalEventId` 保持 null。
    let rejected = record(RecordSpec {
        request: REQUEST_2,
        status: CoreStatus::Rejected,
        command: "session.prompt",
        kind: CommandKind::Mutation,
        result: None,
        error: Some("authorization.scope_denied"),
        terminal_event: false,
        fingerprint: digest_of("a"),
    });
    let body = terminal_body(&rejected, &at).expect("可映射");
    assert_eq!(body.terminal.status, TerminalStatus::Rejected);
    assert_eq!(
        body.terminal.error.as_ref().map(|error| error.code),
        Some(ErrorCode::ExportNotGranted)
    );
    assert!(body.terminal.terminal_event_id.is_null());
}

#[test]
fn the_error_registry_maps_core_codes_without_inventing_new_ones() {
    for (core_code, expected) in [
        ("authorization.scope_denied", ErrorCode::ExportNotGranted),
        ("export.not_granted", ErrorCode::ExportNotGranted),
        (
            "command.idempotency_conflict",
            ErrorCode::CommandIdempotencyConflict,
        ),
        (
            "nodelink.command.unsupported",
            ErrorCode::CommandUnsupported,
        ),
        ("nodelink.command.uncertain", ErrorCode::CommandUncertain),
        ("nodelink.command.not_found", ErrorCode::CommandNotFound),
        (
            "nodelink.protocol.schema_invalid",
            ErrorCode::ProtocolSchemaInvalid,
        ),
        ("internal.unavailable", ErrorCode::InternalUnavailable),
    ] {
        assert_eq!(error_info(core_code).code, expected, "{core_code}");
    }
    assert!(
        error_info("nodelink.resource.rate_limited").retryable,
        "retryable 取 registry 登记的语义"
    );
    assert!(!error_info("authorization.scope_denied").retryable);
    assert_eq!(
        field_details("cwd").get(),
        r#"{"field":"cwd"}"#,
        "details.field 是 `nodelink.command.unsupported_field` 的必需字段"
    );
    assert_eq!(
        parameter_details("workspaceAlias").get(),
        r#"{"parameter":"workspaceAlias"}"#
    );
    assert_eq!(
        rate_limit_details(Duration::from_millis(1_500)).get(),
        r#"{"retryAfterMs":1500}"#
    );
}

/// [R66] 的幂等键：指纹按 ACPR-CJ1 对**解码后**的 payload 取（键序无关，语义相关）。
#[test]
fn the_request_fingerprint_is_acpr_cj1_over_the_decoded_payload() {
    let first: WireSubmit = serde_json::from_value(json!({
        "requestId": REQUEST,
        "command": "session.prompt",
        "sessionRef": { "ownerNodeId": OWNER_NODE, "exportId": EXPORT, "sessionId": SESSION },
        "attachmentId": CONNECTION,
        "attachmentGeneration": "1",
        "expectedVersion": null,
        "payload": { "content": [{ "type": "text", "text": "hi" }] },
    }))
    .expect("submit");
    let reordered: WireSubmit = serde_json::from_value(json!({
        "requestId": REQUEST,
        "command": "session.prompt",
        "sessionRef": { "ownerNodeId": OWNER_NODE, "exportId": EXPORT, "sessionId": SESSION },
        "attachmentId": CONNECTION,
        "attachmentGeneration": "1",
        "expectedVersion": null,
        "payload": { "content": [{ "text": "hi", "type": "text" }] },
    }))
    .expect("submit");
    let different: WireSubmit = serde_json::from_value(json!({
        "requestId": REQUEST,
        "command": "session.prompt",
        "sessionRef": { "ownerNodeId": OWNER_NODE, "exportId": EXPORT, "sessionId": SESSION },
        "attachmentId": CONNECTION,
        "attachmentGeneration": "1",
        "expectedVersion": null,
        "payload": { "content": [{ "type": "text", "text": "bye" }] },
    }))
    .expect("submit");

    let first = fingerprint_of(&first.payload).expect("指纹");
    let reordered = fingerprint_of(&reordered.payload).expect("指纹");
    let different = fingerprint_of(&different.payload).expect("指纹");
    assert_eq!(first, reordered, "ACPR-CJ1 只规范化键序与转义");
    assert_ne!(first, different, "语义不同必须得到不同指纹");
    assert_eq!(first.as_str().len(), 43, "规范 base64url 的 32 字节摘要");
}

// ---------------------------------------------------------------------------------------------
// 路由层用例
// ---------------------------------------------------------------------------------------------

/// [R66]：`session.create` 的四个字段必须显式为 `null`，`session.mode.set` 必须带 `expectedVersion`。
#[tokio::test]
async fn session_create_null_rules_and_expected_version_are_enforced_at_the_wire_boundary() {
    let mut fixture = Fixture::new().await;
    let route = fixture.route();

    // `sessionRef` 非 null：schema 拒绝（不是命令级拒绝）。
    let mut body = submit_body(
        REQUEST,
        "session.create",
        json!({
            "agentId": "codex",
            "exportId": EXPORT,
            "workspaceAlias": "project",
        }),
    );
    body["sessionRef"] = json!({
        "ownerNodeId": OWNER_NODE,
        "exportId": EXPORT,
        "sessionId": SESSION,
    });
    let frames = submit(&mut fixture, &route, body).await;
    let errors = of_type(&frames, "link.error");
    assert_eq!(errors.len(), 1);
    assert_eq!(error_code(errors[0]), "nodelink.protocol.schema_invalid");

    // `session.mode.set` 缺 `expectedVersion`：同样是 schema 拒绝。
    let (attachment, generation) = fixture.attach().await;
    let mut body = session_submit_body(
        REQUEST_2,
        "session.mode.set",
        json!({ "modeId": "code" }),
        Some((&attachment, &generation)),
    );
    body["expectedVersion"] = Value::Null;
    let frames = submit(&mut fixture, &route, body).await;
    let errors = of_type(&frames, "link.error");
    assert_eq!(errors.len(), 1);
    assert_eq!(error_code(errors[0]), "nodelink.protocol.schema_invalid");
    assert_eq!(
        fixture.world.store.commit_calls(),
        0,
        "被 schema 拒绝的命令不得产生任何副作用"
    );
}

/// [R69]/[R82]/[R83]：越权命令回 `command.rejected`、无副作用、且留下 `authorization.denied` 审计。
#[tokio::test]
async fn an_unauthorized_command_is_rejected_audited_and_has_no_side_effect() {
    // 信任记录只有 `grant.observe`：`session.prompt` 需要 `grant.interact`。
    let mut fixture = Fixture::with_grants(&["grant.observe"]).await;
    let route = fixture.route();
    let (attachment, generation) = fixture.attach().await;

    let body = session_submit_body(
        REQUEST,
        "session.prompt",
        json!({ "content": [{ "type": "text", "text": "hi" }] }),
        Some((&attachment, &generation)),
    );
    let frames = submit(&mut fixture, &route, body).await;
    let rejected = of_type(&frames, "command.rejected");
    assert_eq!(rejected.len(), 1, "越权必须回 command.rejected");
    let body = &rejected[0]["body"];
    assert_eq!(body["command"], "session.prompt");
    assert_eq!(error_code(rejected[0]), "nodelink.export.not_granted");
    assert!(
        body.get("acceptedAt").is_none(),
        "command.rejected 不携带 acceptedAt"
    );
    assert_eq!(
        fixture.world.store.commit_calls(),
        0,
        "越权命令不得有副作用"
    );

    let audits = fixture.audits().await;
    let denials: Vec<_> = audits
        .iter()
        .filter(|record| record.action() == AuditAction::AuthorizationDenied)
        .collect();
    assert_eq!(denials.len(), 1, "越权必须留痕");
    assert_eq!(denials[0].outcome(), AuditOutcome::Denied);
    assert_eq!(
        denials[0].via_node().map(NodeId::as_str),
        Some(ACCESS_NODE),
        "Owner 侧只记 viaNodeId"
    );
    assert!(
        denials[0].local_principal_ref().is_none(),
        "节点级信任模型：localPrincipalRef 为 null"
    );
}

/// [R74]：禁带字段被拒、`details.field` 指明字段、且**不创建会话**。
#[tokio::test]
async fn session_create_forbidden_fields_are_rejected_without_creating_a_session() {
    let mut fixture = Fixture::new().await;
    let route = fixture.route();

    for (payload, field) in [
        (json!({ "cwd": "/tmp", "agentId": "codex" }), "cwd"),
        (
            json!({ "agentId": "codex", "mcpServers": { "x": {} } }),
            "mcpServers",
        ),
    ] {
        let frames = submit(
            &mut fixture,
            &route,
            submit_body(REQUEST, "session.create", payload),
        )
        .await;
        let rejected = of_type(&frames, "command.rejected");
        assert_eq!(rejected.len(), 1, "禁带字段必须回 command.rejected");
        assert_eq!(
            error_code(rejected[0]),
            "nodelink.command.unsupported_field"
        );
        assert_eq!(rejected[0]["body"]["error"]["details"]["field"], field);
    }
    assert_eq!(
        fixture.world.store.commit_calls(),
        0,
        "禁带字段不得创建会话或部分应用参数"
    );
}

/// [R75]：未知 `workspaceAlias` 回 `not_granted` 并带 `details.parameter`，不创建会话。
#[tokio::test]
async fn session_create_with_an_unknown_workspace_alias_is_rejected_with_the_parameter() {
    let mut fixture = Fixture::new().await;
    let route = fixture.route();

    let frames = submit(
        &mut fixture,
        &route,
        submit_body(
            REQUEST,
            "session.create",
            json!({
                "agentId": "codex",
                "exportId": EXPORT,
                "workspaceAlias": "other",
            }),
        ),
    )
    .await;
    let rejected = of_type(&frames, "command.rejected");
    assert_eq!(rejected.len(), 1);
    assert_eq!(error_code(rejected[0]), "nodelink.export.not_granted");
    assert_eq!(
        rejected[0]["body"]["error"]["details"]["parameter"],
        "workspaceAlias"
    );

    // 未导出的 agent 走同一条判定。
    let frames = submit(
        &mut fixture,
        &route,
        submit_body(
            REQUEST_2,
            "session.create",
            json!({
                "agentId": "claude",
                "exportId": EXPORT,
                "workspaceAlias": "project",
            }),
        ),
    )
    .await;
    let rejected = of_type(&frames, "command.rejected");
    assert_eq!(rejected.len(), 1);
    assert_eq!(error_code(rejected[0]), "nodelink.export.not_granted");
    assert_eq!(
        rejected[0]["body"]["error"]["details"]["parameter"],
        "agentId"
    );
    assert_eq!(fixture.world.store.commit_calls(), 0);
}

/// [R72]：首切片 template 零参数——带 `templateParams` 的请求必须明确拒绝。
#[tokio::test]
async fn session_create_with_template_parameters_is_rejected() {
    let mut fixture = Fixture::new().await;
    let route = fixture.route();

    let frames = submit(
        &mut fixture,
        &route,
        submit_body(
            REQUEST,
            "session.create",
            json!({
                "agentId": "codex",
                "exportId": EXPORT,
                "workspaceAlias": "project",
                "templateParams": { "name": "value" },
            }),
        ),
    )
    .await;
    let rejected = of_type(&frames, "command.rejected");
    assert_eq!(rejected.len(), 1);
    assert_eq!(
        error_code(rejected[0]),
        "nodelink.command.unsupported_field"
    );
    assert_eq!(
        rejected[0]["body"]["error"]["details"]["field"],
        "templateParams"
    );
    assert_eq!(fixture.world.store.commit_calls(), 0);

    // 省略 `templateParams` 时正常路径不受影响（同样的显式空对象也允许）。
    let frames = submit(
        &mut fixture,
        &route,
        submit_body(
            REQUEST_2,
            "session.create",
            json!({
                "agentId": "codex",
                "exportId": EXPORT,
                "workspaceAlias": "project",
                "templateParams": {},
            }),
        ),
    )
    .await;
    assert!(
        of_type(&frames, "command.rejected").is_empty(),
        "空 templateParams 合法：{:?}",
        frames
            .iter()
            .map(|frame| frame["type"].clone())
            .collect::<Vec<_>>()
    );
}

/// [R73]/[R72]：正常创建先回 `accepted(result = null)`，再回带复合引用的 `completed` 终态。
#[tokio::test]
async fn session_create_returns_accepted_then_the_composite_result() {
    let mut fixture = Fixture::new().await;
    let route = fixture.route();

    let frames = submit(
        &mut fixture,
        &route,
        submit_body(
            REQUEST,
            "session.create",
            json!({
                "agentId": "codex",
                "exportId": EXPORT,
                "workspaceAlias": "project",
            }),
        ),
    )
    .await;
    assert!(
        of_type(&frames, "command.rejected").is_empty(),
        "合法参数不得被拒：{:?}",
        frames
            .iter()
            .map(|frame| frame["type"].clone())
            .collect::<Vec<_>>()
    );

    let accepted = of_type(&frames, "command.accepted");
    assert_eq!(accepted.len(), 1, "必须先回 command.accepted");
    assert_eq!(accepted[0]["body"]["command"], "session.create");
    assert_eq!(accepted[0]["body"]["acceptedAt"], NOW);
    assert_eq!(
        accepted[0]["body"]["result"],
        Value::Null,
        "session.create 的 accepted 不携带结果"
    );

    let terminal = of_type(&frames, "command.terminal");
    assert_eq!(terminal.len(), 1, "创建完成后必须回 command.terminal");
    let body = &terminal[0]["body"];
    assert_eq!(body["command"], "session.create");
    assert_eq!(body["terminal"]["status"], "completed");
    assert!(body["terminal"]["error"].is_null());
    let result = &body["terminal"]["result"];
    assert_eq!(result["remoteSessionRef"]["ownerNodeId"], OWNER_NODE);
    assert_eq!(result["remoteSessionRef"]["exportId"], EXPORT);
    assert_eq!(result["sessionMeta"]["state"], "idle");
    assert_eq!(result["sessionMeta"]["version"], "1");
    let generated = result["remoteSessionRef"]["sessionId"]
        .as_str()
        .expect("Owner 生成的 sessionId");
    assert_ne!(
        generated, SESSION,
        "sessionId 由 Owner 在提交事务内分配（不是请求带来的）"
    );
    assert_eq!(
        body["terminal"]["terminalEventId"], EVENT,
        "终态以持久记录为唯一权威：terminalEventId 非 null"
    );
    assert_eq!(
        fixture.world.store.commit_calls(),
        2,
        "创建一次（幂等行 + 新会话），终态一次（终态事件 + 命令行终结）"
    );
    // 持久事实：`command.status` 重查回同一终态（`terminalEventId` 非 null）。
    let status = submit(
        &mut fixture,
        &route,
        submit_body(
            REQUEST_2,
            "command.status",
            json!({ "targetRequestId": REQUEST }),
        ),
    )
    .await;
    let reread = of_type(&status, "command.terminal");
    assert_eq!(reread.len(), 1);
    assert_eq!(reread[0]["body"]["command"], "session.create");
    assert_eq!(reread[0]["body"]["terminal"]["terminalEventId"], EVENT);
    assert_eq!(reread[0]["body"]["terminal"]["result"], result.clone());
}

/// [R66]/§6 第 20 条：`session.create` 的幂等键 `(accessNodeId, requestId)` 是**持久事实**——同键
/// 同语义回首次结果（含重启后），同键不同语义 `nodelink.command.idempotency_conflict`，都不重复创建。
#[tokio::test]
async fn a_repeated_session_create_request_replays_the_first_result() {
    let mut fixture = Fixture::new().await;
    let route = fixture.route();
    let body = submit_body(
        REQUEST,
        "session.create",
        json!({
            "agentId": "codex",
            "exportId": EXPORT,
            "workspaceAlias": "project",
        }),
    );

    let first = submit(&mut fixture, &route, body.clone()).await;
    let first_terminal = of_type(&first, "command.terminal");
    assert_eq!(first_terminal.len(), 1);
    let commits_after_create = fixture.world.store.commit_calls();
    assert_eq!(commits_after_create, 2);

    // 同一路由（进程内状态仍在）：回第一次的终态，不再创建第二个会话。
    let second = submit(&mut fixture, &route, body.clone()).await;
    let replayed = of_type(&second, "command.terminal");
    assert_eq!(replayed.len(), 1, "重复提交必须回首次结果");
    assert_eq!(replayed[0]["body"], first_terminal[0]["body"]);
    assert_eq!(
        fixture.world.store.commit_calls(),
        commits_after_create,
        "幂等命中不得再落盘"
    );

    // 重启：core 与路由重建（观察表、内存缓存全丢），持久事实延续——仍回首次结果且零新提交。
    let restarted = fixture.restarted_route();
    let again = submit(&mut fixture, &restarted, body).await;
    let replayed = of_type(&again, "command.terminal");
    assert_eq!(replayed.len(), 1, "重启后重复提交必须回首次结果");
    assert_eq!(replayed[0]["body"], first_terminal[0]["body"]);
    assert_eq!(
        fixture.world.store.commit_calls(),
        commits_after_create,
        "重启后重试不得重复创建"
    );

    // 同键不同语义：`idempotency_conflict`（两个 payload 都合法，只是语义不同）。
    let conflict = submit(
        &mut fixture,
        &restarted,
        submit_body(
            REQUEST,
            "session.create",
            json!({
                "agentId": "codex",
                "exportId": EXPORT,
                "workspaceAlias": "alt",
            }),
        ),
    )
    .await;
    let rejected = of_type(&conflict, "command.rejected");
    assert_eq!(rejected.len(), 1);
    assert_eq!(
        error_code(rejected[0]),
        "nodelink.command.idempotency_conflict"
    );
    assert_eq!(
        fixture.world.store.commit_calls(),
        commits_after_create,
        "冲突不得产生副作用"
    );
}

/// [R70]/§6 第 20 条：创建的崩溃窗口（幂等行已落盘、终态从未提交）由启动恢复终结为 `uncertain`，
/// 重查与重试都回该持久终态，且不重复创建。
#[tokio::test]
async fn a_session_create_crash_window_becomes_uncertain_after_recovery() {
    let mut fixture = Fixture::new().await;
    let body = submit_body(
        REQUEST,
        "session.create",
        json!({
            "agentId": "codex",
            "exportId": EXPORT,
            "workspaceAlias": "project",
        }),
    );
    // 直接提交**创建**提交（不经过适配层的终态落盘），模拟崩溃后的现场。
    let request = core_request(&Uuid::parse(REQUEST).expect("uuid")).expect("request id");
    let actor = node_actor(&NodeId::new(ACCESS_NODE).expect("node id"));
    let fingerprint = fingerprint_of(&wire_submit(&body).payload).expect("指纹");
    let session = fixture
        .world
        .core
        .create_session(
            &actor,
            request.clone(),
            fingerprint,
            CreateSessionRequest {
                agent: agent_ref(),
                workspace: None,
                template: None,
                origin: ResourceOrigin::Local,
            },
            None,
        )
        .await
        .expect("创建会话");

    assert_eq!(
        fixture.world.store.commit_calls(),
        1,
        "崩溃前只落了创建提交（终态从未提交）"
    );
    // 重启：组合根先跑 §6 第 16 条的启动恢复，再开始服务。
    fixture
        .world
        .core
        .recover_unsettled(&Actor::LocalCli, ReplayLimit::new(16))
        .await
        .expect("启动恢复");
    let restarted = fixture.restarted_route();

    // `command.status` 重查：持久终态是 `uncertain`，带 `terminalEventId` 与结构化错误。
    let status = submit(
        &mut fixture,
        &restarted,
        submit_body(
            REQUEST_2,
            "command.status",
            json!({ "targetRequestId": REQUEST }),
        ),
    )
    .await;
    let terminal = of_type(&status, "command.terminal");
    assert_eq!(terminal.len(), 1);
    assert_eq!(terminal[0]["body"]["command"], "session.create");
    assert_eq!(terminal[0]["body"]["terminal"]["status"], "uncertain");
    assert_eq!(
        terminal[0]["body"]["terminal"]["terminalEventId"], EVENT,
        "恢复必须写持久终态事件"
    );

    // 同键重试：回同一 `uncertain` 终态（不猜成功/失败），且不重复创建。
    let commits_after_recovery = fixture.world.store.commit_calls();
    assert_eq!(commits_after_recovery, 2, "恢复本身写一次终态提交");
    let resubmitted = submit(&mut fixture, &restarted, body).await;
    let replayed = of_type(&resubmitted, "command.terminal");
    assert_eq!(replayed.len(), 1, "重试回持久终态而不是新建");
    assert_eq!(replayed[0]["body"], terminal[0]["body"]);
    assert_eq!(
        fixture.world.store.commit_calls(),
        commits_after_recovery,
        "重试不得重复创建第二个会话"
    );
    assert_eq!(
        lock(&fixture.world.store.sessions).len(),
        2,
        "夹具预置一个会话 + 崩溃前创建的一个"
    );
    assert_ne!(session.as_str(), "");
}

/// [R67]：相同 `(requestId, command, payload)` 的重复 mutation 回首次结果且不二次派发。
#[tokio::test]
async fn a_repeated_request_id_replays_the_first_terminal() {
    let mut fixture = Fixture::new().await;
    let route = fixture.route();
    let (attachment, generation) = fixture.attach().await;
    let body = session_submit_body(
        REQUEST,
        "session.prompt",
        json!({ "content": [{ "type": "text", "text": "hi" }] }),
        Some((&attachment, &generation)),
    );
    let fingerprint = fingerprint_of(&wire_submit(&body).payload).expect("指纹");
    fixture.world.store.seed_command(record(RecordSpec {
        request: REQUEST,
        status: CoreStatus::Completed,
        command: "session.prompt",
        kind: CommandKind::Mutation,
        result: Some(r#"{"kind":"turn.completed"}"#),
        error: None,
        terminal_event: true,
        fingerprint,
    }));

    let frames = submit(&mut fixture, &route, body).await;
    let terminal = of_type(&frames, "command.terminal");
    assert_eq!(terminal.len(), 1, "重复提交回首次的 terminal 结果");
    assert_eq!(terminal[0]["body"]["command"], "session.prompt");
    assert_eq!(terminal[0]["body"]["terminal"]["status"], "completed");
    assert_eq!(terminal[0]["body"]["terminal"]["terminalEventId"], EVENT);
    assert!(
        of_type(&frames, "command.rejected").is_empty(),
        "命中幂等不得回拒绝"
    );
    assert_eq!(
        fixture.world.store.commit_calls(),
        0,
        "幂等命中不得产生第二次派发"
    );
}

/// [R68]：同键不同语义的 mutation 回 `idempotency_conflict` 且不执行第二次副作用。
#[tokio::test]
async fn the_same_request_id_with_a_different_payload_conflicts() {
    let mut fixture = Fixture::new().await;
    let route = fixture.route();
    let (attachment, generation) = fixture.attach().await;
    let body = session_submit_body(
        REQUEST,
        "session.prompt",
        json!({ "content": [{ "type": "text", "text": "hi" }] }),
        Some((&attachment, &generation)),
    );
    // 首次落盘的指纹与本次不同（例如同 requestId 换了 prompt 正文）。
    fixture.world.store.seed_command(record(RecordSpec {
        request: REQUEST,
        status: CoreStatus::Completed,
        command: "session.prompt",
        kind: CommandKind::Mutation,
        result: Some(r#"{"kind":"turn.completed"}"#),
        error: None,
        terminal_event: true,
        fingerprint: digest_of("another payload"),
    }));

    let frames = submit(&mut fixture, &route, body).await;
    let rejected = of_type(&frames, "command.rejected");
    assert_eq!(rejected.len(), 1);
    assert_eq!(
        error_code(rejected[0]),
        "nodelink.command.idempotency_conflict"
    );
    assert_eq!(fixture.world.store.commit_calls(), 0);
}

/// [R66]：`command.status` 重查 mutation 的终态——带 `command` 名、`terminalEventId` 非 null。
#[tokio::test]
async fn command_status_replies_the_terminal_of_the_mutation() {
    let mut fixture = Fixture::new().await;
    let route = fixture.route();
    fixture.world.store.seed_command(record(RecordSpec {
        request: REQUEST,
        status: CoreStatus::Completed,
        command: "session.prompt",
        kind: CommandKind::Mutation,
        result: Some(r#"{"kind":"turn.completed"}"#),
        error: None,
        terminal_event: true,
        fingerprint: digest_of("a"),
    }));

    // 形式一：`command.submit{command:"command.status"}`。
    let frames = submit(
        &mut fixture,
        &route,
        submit_body(
            REQUEST_2,
            "command.status",
            json!({ "targetRequestId": REQUEST }),
        ),
    )
    .await;
    let submit_form = of_type(&frames, "command.terminal");
    assert_eq!(submit_form.len(), 1);
    assert_eq!(submit_form[0]["body"]["requestId"], REQUEST);

    // 形式二：独立的 `command.status` 消息。
    let envelope = fixture.envelope(
        MessageType::CommandStatus,
        json!({ "targetRequestId": REQUEST }),
    );
    assert_eq!(
        route.route(&fixture.handle, &envelope).await,
        RouteOutcome::Claimed
    );
    let frames = fixture.drain();
    let standalone = of_type(&frames, "command.terminal");
    assert_eq!(standalone.len(), 1);
    assert_eq!(
        standalone[0]["body"], submit_form[0]["body"],
        "两种形式必须产生相同的 command.terminal 形状回复"
    );

    let body = &standalone[0]["body"];
    assert_eq!(body["command"], "session.prompt");
    assert_eq!(body["terminal"]["status"], "completed");
    assert_eq!(
        body["terminal"]["terminalEventId"], EVENT,
        "重查必须给出事件 id"
    );
    assert_eq!(body["terminal"]["result"]["kind"], "turn.completed");
}

/// [R66]：未终结的 mutation 在两种 `command.status` 形式下都回同形的 `command.accepted`
/// （`acceptedAt` 取首次接受时间）。
#[tokio::test]
async fn command_status_replies_accepted_for_an_unfinished_mutation() {
    let mut fixture = Fixture::new().await;
    let route = fixture.route();
    fixture.world.store.seed_command(record(RecordSpec {
        request: REQUEST,
        status: CoreStatus::Accepted,
        command: "session.prompt",
        kind: CommandKind::Mutation,
        result: None,
        error: None,
        terminal_event: false,
        fingerprint: digest_of("a"),
    }));

    let frames = submit(
        &mut fixture,
        &route,
        submit_body(
            REQUEST_2,
            "command.status",
            json!({ "targetRequestId": REQUEST }),
        ),
    )
    .await;
    let accepted = of_type(&frames, "command.accepted");
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0]["body"]["command"], "session.prompt");
    assert_eq!(accepted[0]["body"]["acceptedAt"], NOW);
    assert!(accepted[0]["body"]["result"].is_null());

    let envelope = fixture.envelope(
        MessageType::CommandStatus,
        json!({ "targetRequestId": REQUEST }),
    );
    assert_eq!(
        route.route(&fixture.handle, &envelope).await,
        RouteOutcome::Claimed
    );
    let frames = fixture.drain();
    let standalone = of_type(&frames, "command.accepted");
    assert_eq!(standalone.len(), 1);
    assert_eq!(standalone[0]["body"], accepted[0]["body"]);
}

/// [R70]：崩溃窗口写入的 `uncertain` 原样透传，不猜测成功或失败。
#[tokio::test]
async fn an_uncertain_terminal_is_passed_through() {
    let mut fixture = Fixture::new().await;
    let route = fixture.route();
    fixture.world.store.seed_command(record(RecordSpec {
        request: REQUEST,
        status: CoreStatus::Uncertain,
        command: "session.prompt",
        kind: CommandKind::Mutation,
        result: None,
        error: None,
        terminal_event: true,
        fingerprint: digest_of("a"),
    }));

    let frames = submit(
        &mut fixture,
        &route,
        submit_body(
            REQUEST_2,
            "command.status",
            json!({ "targetRequestId": REQUEST }),
        ),
    )
    .await;
    let terminal = of_type(&frames, "command.terminal");
    assert_eq!(terminal.len(), 1);
    let body = &terminal[0]["body"];
    assert_eq!(body["command"], "session.prompt");
    assert_eq!(body["terminal"]["status"], "uncertain");
    assert_eq!(
        body["terminal"]["error"]["code"], "nodelink.command.uncertain",
        "非 completed 必须给出 error，但不编造业务原因"
    );
    assert!(body["terminal"]["result"].is_null());
}

/// [R66]：未知 `targetRequestId` 不猜命令名——连接级 `command.not_found`。
#[tokio::test]
async fn command_status_for_an_unknown_request_is_a_link_error() {
    let mut fixture = Fixture::new().await;
    let route = fixture.route();
    let frames = submit(
        &mut fixture,
        &route,
        submit_body(
            REQUEST,
            "command.status",
            json!({ "targetRequestId": REQUEST_2 }),
        ),
    )
    .await;
    let errors = of_type(&frames, "link.error");
    assert_eq!(errors.len(), 1);
    assert_eq!(error_code(errors[0]), "nodelink.command.not_found");
    assert_eq!(
        errors[0]["body"]["correlationId"],
        "6ae1c07c-9242-46e9-a9d2-4ec58c130f4d"
    );
    assert!(
        of_type(&frames, "command.terminal").is_empty(),
        "未知请求不得伪造成终态"
    );
}

/// 终态观察：记录一旦终结就推出 `command.terminal`，并从在途表里摘掉。
#[tokio::test]
async fn the_watcher_pushes_the_terminal_once_the_record_is_terminal() {
    let mut fixture = Fixture::new().await;
    let route = fixture.route();
    let request = CoreRequestId::new(REQUEST).expect("request id");
    let node = NodeId::new(ACCESS_NODE).expect("node id");

    // 先挂一条观察项：此刻还没有任何帧。
    fixture.world.store.seed_command(record(RecordSpec {
        request: REQUEST,
        status: CoreStatus::Accepted,
        command: "session.prompt",
        kind: CommandKind::Mutation,
        result: None,
        error: None,
        terminal_event: false,
        fingerprint: digest_of("a"),
    }));
    route.watch(&fixture.handle, &request);
    assert_eq!(route.pending_of(&fixture.handle).len(), 1);
    route.poll_pending().await;
    assert!(fixture.drain().is_empty(), "未终结时不得推终态");

    // 记录终结后，同一轮询把它推出去并摘掉观察项。
    fixture.world.store.seed_command(record(RecordSpec {
        request: REQUEST,
        status: CoreStatus::Completed,
        command: "session.prompt",
        kind: CommandKind::Mutation,
        result: Some(r#"{"kind":"turn.completed"}"#),
        error: None,
        terminal_event: true,
        fingerprint: digest_of("a"),
    }));
    route.poll_pending().await;
    let frames = fixture.drain();
    let terminal = of_type(&frames, "command.terminal");
    assert_eq!(terminal.len(), 1);
    assert_eq!(terminal[0]["body"]["terminal"]["status"], "completed");
    assert!(route.pending_of(&fixture.handle).is_empty());
    let _ = node;
}

/// [R66]/[R45] 后半：单连接 in-flight 上限（已接受未终结的 mutation 数）生效并给出退避提示。
#[tokio::test]
async fn the_in_flight_limit_is_enforced_per_connection() {
    // [R45] 后半：`node_link.max_in_flight_commands` 下调为 8 时，该连接第 9 个并发命令被按上限规则拒绝。
    let mut fixture = Fixture::build(
        &["grant.observe", "grant.interact", "grant.remote-work"],
        NodeLinkConfig {
            max_in_flight_commands: 8,
            ..NodeLinkConfig::default()
        },
        true,
    )
    .await;
    let route = fixture.route();
    let live: Vec<CoreRequestId> = (0..8)
        .map(|index| {
            CoreRequestId::new(&format!("00000000-0000-4000-8000-{index:012x}"))
                .expect("request id")
        })
        .collect();
    for request in &live {
        fixture.world.store.seed_command(record(RecordSpec {
            request: request.as_str(),
            status: CoreStatus::Accepted,
            command: "session.prompt",
            kind: CommandKind::Mutation,
            result: None,
            error: None,
            terminal_event: false,
            fingerprint: digest_of("a"),
        }));
        route.watch(&fixture.handle, request);
    }

    assert!(
        !route.admit_in_flight(&fixture.handle).await,
        "第 9 个并发命令必须按上限规则拒绝"
    );
    let frames = fixture.drain();
    let errors = of_type(&frames, "link.error");
    assert_eq!(errors.len(), 1);
    assert_eq!(error_code(errors[0]), "nodelink.resource.rate_limited");
    assert!(
        errors[0]["body"]["details"]["retryAfterMs"].is_number(),
        "限流必须给出 retryAfterMs"
    );

    // 其中一条终结后配额释放（在途数按持久化记录判定，不看进程内计时）。
    let first = live[0].clone();
    fixture.world.store.seed_command(record(RecordSpec {
        request: first.as_str(),
        status: CoreStatus::Completed,
        command: "session.prompt",
        kind: CommandKind::Mutation,
        result: Some(r#"{"kind":"turn.completed"}"#),
        error: None,
        terminal_event: true,
        fingerprint: digest_of("a"),
    }));
    assert!(
        route.admit_in_flight(&fixture.handle).await,
        "终态之后必须重新接受命令"
    );
}

/// [R71]：单连接 120/分钟；连续超限以 4429 关闭（`retryable = true` + `retryAfterMs`）。
#[tokio::test]
async fn the_command_rate_limit_replies_rate_limited_and_then_closes() {
    // 只有 `grant.observe`：被拒的命令因此在授权交集处失败，成本与真实越权命令同量级。
    let mut fixture = Fixture::with_grants(&["grant.observe"]).await;
    let route = fixture.route();
    let (attachment, generation) = fixture.attach().await;
    let body = session_submit_body(
        REQUEST,
        "session.prompt",
        json!({ "content": [{ "type": "text", "text": "hi" }] }),
        Some((&attachment, &generation)),
    );

    for _ in 0..COMMANDS_PER_MINUTE {
        let frames = submit(&mut fixture, &route, body.clone()).await;
        assert!(
            of_type(&frames, "command.rejected").len() == 1,
            "限额内的命令按越权拒绝（不是限流）"
        );
    }
    let frames = submit(&mut fixture, &route, body.clone()).await;
    let errors = of_type(&frames, "link.error");
    assert_eq!(errors.len(), 1, "超限必须回 link.error");
    assert_eq!(error_code(errors[0]), "nodelink.resource.rate_limited");
    assert_eq!(errors[0]["body"]["retryable"], true);
    assert!(errors[0]["body"]["details"]["retryAfterMs"].is_number());
    assert_eq!(fixture.handle.close_request(), None, "首次超限不关闭连接");

    // 连续超限达到阈值即 4429。
    let mut closed = false;
    for _ in 1..MAX_CONSECUTIVE_RATE_LIMIT_REJECTIONS {
        submit(&mut fixture, &route, body.clone()).await;
        if let Some((code, _)) = fixture.handle.close_request() {
            assert_eq!(code, 4429);
            closed = true;
        }
    }
    assert!(closed, "持续超限必须关闭连接（4429）");
}

/// [R78]：节点撤销推送 `node.trust.revoked` 并以 4410 关闭。
#[tokio::test]
async fn node_revocation_notifies_then_closes_with_4410() {
    let mut fixture = Fixture::new().await;
    let route = fixture.route();
    let node = NodeId::new(ACCESS_NODE).expect("node id");

    let revoked_at = ts("2026-09-18T09:20:00.000Z");
    fixture.world.trust.seed_node(
        NodeRecord::try_new(
            node.clone(),
            "Office Access",
            NodeKind::Access,
            test_public_key().fingerprint(),
            GrantSet::try_from_iter(["grant.observe"]).expect("grants"),
            NodeState::Revoked,
            None,
            ts("2026-09-18T09:00:00.000Z"),
            None,
            Some(revoked_at.clone()),
        )
        .expect("已撤销信任行"),
    );

    assert_eq!(route.node_revoked(&node).await, 1);
    let frames = fixture.drain();
    let revoked = of_type(&frames, "node.trust.revoked");
    assert_eq!(revoked.len(), 1);
    assert_eq!(revoked[0]["body"]["revokedNodeId"], ACCESS_NODE);
    assert_eq!(revoked[0]["body"]["revokedAt"], revoked_at.as_str());
    assert_eq!(
        fixture.handle.close_request().map(|(code, _)| code),
        Some(4410)
    );
    assert!(
        route.pending_of(&fixture.handle).is_empty(),
        "撤销后该连接的观察表随连接消失"
    );
}

/// [R77]/[R76]：Export 撤销推 `export.revoked`、清内存 attachment，且此后的命令按持久化记录被拒。
#[tokio::test]
async fn export_revocation_clears_attachments_and_rejects_later_commands() {
    let mut fixture = Fixture::new().await;
    let route = fixture.route();
    let (attachment, generation) = fixture.attach().await;

    // 撤销：已撤销的 Export 与已撤销的节点行都从持久化记录读（`FakeExports` 按同一语义换掉记录）。
    fixture.world.exports.seed_revoked(EXPORT);
    assert_eq!(route.export_revoked(&export_id()).await, 1);
    let frames = fixture.drain();
    let revoked = of_type(&frames, "export.revoked");
    assert_eq!(revoked.len(), 1);
    assert_eq!(revoked[0]["body"]["exportId"], EXPORT);
    assert_eq!(revoked[0]["body"]["revokedAt"], "2026-01-01T00:00:00.000Z");

    // 内存 attachment 已清除：旧代际不再是「当前 attachment」。
    assert!(
        fixture
            .world
            .resource
            .current_attachment_session_ref(
                &fixture.handle,
                &Uuid::parse(&attachment).expect("attachment id"),
                generation.parse::<u64>().expect("generation"),
            )
            .is_none(),
        "撤销必须清掉内存订阅与 attachment"
    );

    // 该 Export 上的新命令被拒（授权判定读当次持久化记录，与推送无关）。
    let frames = submit(
        &mut fixture,
        &route,
        session_submit_body(
            REQUEST,
            "session.prompt",
            json!({ "content": [{ "type": "text", "text": "hi" }] }),
            Some((&attachment, &generation)),
        ),
    )
    .await;
    let rejected = of_type(&frames, "command.rejected");
    assert_eq!(rejected.len(), 1);
    assert_eq!(error_code(rejected[0]), "nodelink.export.not_granted");
}

/// [R66]：`session.list` 只返回 agent 属于该节点可见 Export 的会话（D14 的唯一判定点）。
#[tokio::test]
async fn session_list_only_returns_sessions_of_visible_exports() {
    let mut fixture = Fixture::new().await;
    let route = fixture.route();
    // 第二个会话属于 Export 未覆盖的 agent。
    fixture.world.store.seed_session(
        SessionSummary::try_new(
            SessionId::new("9ae1c07c-9242-46e9-a9d2-4ec58c130f51").expect("session id"),
            None,
            AgentRef::try_new(AgentId::new("claude").expect("agent id"), "Claude")
                .expect("agent ref"),
            SessionState::Idle,
            ResourceOrigin::Local,
            None,
            Version::new(1),
            ts("2026-09-18T09:00:00.000Z"),
            ts("2026-09-18T09:00:00.000Z"),
        )
        .expect("会话摘要"),
    );

    let frames = submit(
        &mut fixture,
        &route,
        submit_body(REQUEST, "session.list", json!({})),
    )
    .await;
    let terminal = of_type(&frames, "command.terminal");
    assert_eq!(terminal.len(), 1);
    let body = &terminal[0]["body"];
    assert_eq!(body["command"], "session.list");
    assert_eq!(body["terminal"]["status"], "completed");
    let sessions = body["terminal"]["result"]["sessions"]
        .as_array()
        .expect("sessions 数组");
    assert_eq!(sessions.len(), 1, "未导出的 agent 的会话不得出现");
    assert_eq!(sessions[0]["sessionId"], SESSION);
    assert_eq!(sessions[0]["agent"]["agentId"], "codex");
    assert_eq!(sessions[0]["origin"]["kind"], "local");
    assert_eq!(sessions[0]["version"], "3");
    // 查询命令同步完成，没有持久化记录因此没有终态事件。
    assert!(body["terminal"]["terminalEventId"].is_null());
}

/// 本切片没有 v1 结果投影的查询必须**显式**回不支持，不返回被裁剪的结果。
#[tokio::test]
async fn queries_without_a_v1_projection_are_rejected_explicitly() {
    let mut fixture = Fixture::new().await;
    let route = fixture.route();
    let (attachment, generation) = fixture.attach().await;

    for (command, payload) in [
        ("session.read", json!({ "include": ["messages"] })),
        ("session.mode.list", json!({})),
        ("session.config.list", json!({})),
    ] {
        let frames = submit(
            &mut fixture,
            &route,
            session_submit_body(REQUEST, command, payload, Some((&attachment, &generation))),
        )
        .await;
        let rejected = of_type(&frames, "command.rejected");
        assert_eq!(rejected.len(), 1, "{command} 必须显式拒绝");
        assert_eq!(error_code(rejected[0]), "nodelink.command.unsupported");
        assert!(
            of_type(&frames, "command.terminal").is_empty(),
            "{command} 不得回被裁剪的结果"
        );
    }
}
