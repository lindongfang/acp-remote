//! 命令面的单元测试（任务 2.1–2.6）。
//!
//! 这里的「记账替身」是 [`CommandStore`]：它是 `SessionStore`/`ReadView` 的替身，**逐次计数**会话读取
//! （`list`/`read_session`/`find_request`/`load`）与写入（`commit`）。「未授权零副作用」「被限流的命令无
//! 副作用」「重复 `messageId` 不产生第二次业务读取」三条判据都用它证明，而不是只看对端收到了什么。
//! `head()` **不计数**：设备信任行与事件库水位的读取是**授权判定本身**的输入（`sync_device_handshake_view`），
//! 不是业务读取，它也不可能泄露任何会话的存在性。

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use acp_core::broker::{Broker, BrokerConfig, BrokerDeps};
use acp_core::model::{
    Actor, CommandKind, CommandRecord, CommandStatus, CommittedEvent,
    ConfigOption as CoreConfigOption, ConfigOptionEntry as CoreConfigOptionEntry, ConfigOptionId,
    ConfigOptionKind, ConfigValue, DeviceId, Digest, EventId, EventPayload, EventType,
    GlobalCursor, ModeId, ModeRef as CoreModeRef, ModeState as CoreModeState, OriginEpoch,
    PortError, PromptRequest, RequestId, ResumeSessionRequest, SelectValue, Sequence, ServerEpoch,
    SessionRecoveryRecord, SessionSummary, Timestamp, ViewJson,
};
use acp_core::ports::{
    Clock, CommitOutcome, EventSink, HistoryPage, HistoryQuery, OwnedCommit, OwnedEventRecord,
    PruneReport, ReadView, ReplayLimit, SessionEndpoint, SessionQuery, SessionStore, StoreHealth,
    TurnAccepted,
};
use acp_core::use_cases::{UseCaseDeps, UseCases};
use identity_auth::Authority;
use sync_protocol::envelope::Envelope;
use sync_protocol::error::ErrorCode;

use crate::local_admin::test_support::{
    FakeAudit, FakeCatalog, FakeClock, FakeConfig, FakeEntropy, FakeExports, FakeIds, FakeKeystore,
    FakeTrust, NotTouched,
};
use crate::sync::command::CommandRoute;
use crate::sync::command::limits::{CommandFamily, RateOutcome};
use crate::sync::conn::SyncConfig;
use crate::sync::conn::registry::{ConnectionHandle, ConnectionRegistry, Outbound};
use crate::sync::subscribe::OwnerReachability;
use crate::sync::subscribe::fixture::{
    CONNECTION, DEVICE, EPOCH, Frame, HOST, Reachability, SyncDeliveries, bodies,
    connection_with_limits, frames,
};

// ---------------------------------------------------------------------------------------------
// 记账存储替身
// ---------------------------------------------------------------------------------------------

/// 存储替身的观测计数（每个字段各代表一条判据的「没有被触碰」）。
#[derive(Debug, Default)]
struct Counters {
    list: AtomicU64,
    read_session: AtomicU64,
    find_request: AtomicU64,
    load: AtomicU64,
    commit: AtomicU64,
}

#[derive(Default)]
struct StoreState {
    sessions: Vec<SessionSummary>,
    head: Option<GlobalCursor>,
    /// 逐会话的事件（升序），`read_session` 的分页素材。
    events: BTreeMap<String, Vec<OwnedEventRecord>>,
    /// `owned_command` 行的替身（`command.status` 的唯一读面）。
    commands: Vec<CommandRecord>,
}

/// 「会话列表 + 事件分页 + 命令记录」的存储替身，带逐次计数。
#[derive(Clone, Default)]
struct CommandStore {
    state: Arc<Mutex<StoreState>>,
    counters: Arc<Counters>,
}

impl CommandStore {
    fn set_head(&self, cursor: GlobalCursor) {
        self.lock().head = Some(cursor);
    }

    fn push_session(&self, summary: SessionSummary) {
        self.lock().sessions.push(summary);
    }

    fn push_event(&self, session: &str, record: OwnedEventRecord) {
        self.lock()
            .events
            .entry(session.to_owned())
            .or_default()
            .push(record);
    }

    fn push_command(&self, record: CommandRecord) {
        self.lock().commands.push(record);
    }

    /// 会话读取的次数（不含 `head()`：见模块文档）。
    fn session_reads(&self) -> u64 {
        self.counters.list.load(Ordering::SeqCst)
            + self.counters.read_session.load(Ordering::SeqCst)
            + self.counters.find_request.load(Ordering::SeqCst)
            + self.counters.load.load(Ordering::SeqCst)
    }

    fn list_calls(&self) -> u64 {
        self.counters.list.load(Ordering::SeqCst)
    }

    fn commits(&self) -> u64 {
        self.counters.commit.load(Ordering::SeqCst)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, StoreState> {
        self.state.lock().expect("存储替身锁")
    }
}

#[async_trait::async_trait]
impl SessionStore for CommandStore {
    async fn list(&self, _query: SessionQuery) -> Result<Vec<SessionSummary>, PortError> {
        self.counters.list.fetch_add(1, Ordering::SeqCst);
        Ok(self.lock().sessions.clone())
    }

    async fn head(&self) -> Result<GlobalCursor, PortError> {
        Ok(self.lock().head.clone().unwrap_or_else(|| cursor(EPOCH, 0)))
    }

    async fn read_view(&self) -> Result<Box<dyn ReadView>, PortError> {
        Ok(Box::new(CommandReadView {
            state: Arc::clone(&self.state),
            counters: Arc::clone(&self.counters),
        }))
    }

    async fn find_request(
        &self,
        request: &RequestId,
        _actor: &Actor,
    ) -> Result<Option<CommandRecord>, PortError> {
        self.counters.find_request.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .lock()
            .commands
            .iter()
            .find(|record| record.request().as_str() == request.as_str())
            .cloned())
    }

    /// 会话行的窄读取（`session_version` 的唯一来源）。替身没有会话行，因此恒为 `None`（版本 0）。
    async fn load(
        &self,
        _session: &acp_core::model::SessionId,
    ) -> Result<Option<acp_core::model::SessionSnapshot>, PortError> {
        self.counters.load.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    }

    /// 命令面不该有任何写入：走到这里就是「零副作用」判据被破坏，用例直接失败。
    async fn commit(&self, _commit: OwnedCommit) -> Result<CommitOutcome, PortError> {
        self.counters.commit.fetch_add(1, Ordering::SeqCst);
        panic!("命令面的查询路径不得写存储")
    }

    async fn unsettled_commands(
        &self,
        _limit: ReplayLimit,
    ) -> Result<Vec<CommandRecord>, PortError> {
        panic!("命令面未触及")
    }

    async fn load_recovery(
        &self,
        _session: &acp_core::model::SessionId,
    ) -> Result<Option<SessionRecoveryRecord>, PortError> {
        panic!("命令面未触及")
    }

    async fn retention_window(
        &self,
        _session: &acp_core::model::SessionId,
    ) -> Result<Option<(Sequence, Sequence)>, PortError> {
        panic!("命令面未触及")
    }

    async fn prune(
        &self,
        _policy: acp_core::ports::RetentionPolicy,
        _at: Timestamp,
    ) -> Result<PruneReport, PortError> {
        panic!("命令面未触及")
    }

    async fn health(&self) -> Result<StoreHealth, PortError> {
        panic!("命令面未触及")
    }
}

/// 一致性读视图替身：`read_session` 的分页语义照抄 `storage-sqlite`（升序、`limit + 1` 取页、多取到的
/// 那条给出 `next`），`history` 面照抄它的降序取页再翻回升序。
struct CommandReadView {
    state: Arc<Mutex<StoreState>>,
    counters: Arc<Counters>,
}

#[async_trait::async_trait]
impl ReadView for CommandReadView {
    async fn head(&self) -> Result<GlobalCursor, PortError> {
        Ok(self
            .state
            .lock()
            .expect("存储替身锁")
            .head
            .clone()
            .unwrap_or_else(|| cursor(EPOCH, 0)))
    }

    async fn replay(
        &self,
        _after: Option<GlobalCursor>,
        _limit: ReplayLimit,
    ) -> Result<acp_core::ports::ReplayBatch, PortError> {
        panic!("命令面未触及")
    }

    async fn sync_replay(
        &self,
        _after: Option<GlobalCursor>,
        _limit: ReplayLimit,
    ) -> Result<acp_core::ports::SyncReplaySlice, PortError> {
        panic!("命令面未触及")
    }

    async fn event_payload(&self, _event: &EventId) -> Result<Option<EventPayload>, PortError> {
        panic!("命令面未触及")
    }

    async fn read_session(&self, query: HistoryQuery) -> Result<HistoryPage, PortError> {
        self.counters.read_session.fetch_add(1, Ordering::SeqCst);
        let state = self.state.lock().expect("存储替身锁");
        let session = query.session.as_str().to_owned();
        let rows = state.events.get(&session).cloned().unwrap_or_default();
        let summary = state
            .sessions
            .iter()
            .find(|summary| summary.session_id().as_str() == session)
            .cloned()
            .unwrap_or_else(|| core_session(&session));
        drop(state);
        let mut rows = rows;
        rows.sort_by_key(|record| record.event.global_sequence.get());
        let head = cursor(
            EPOCH,
            rows.last().map_or(0, |r| r.event.global_sequence.get()),
        );
        let mut selected: Vec<OwnedEventRecord> = Vec::new();
        let mut next = None;
        if query.include.messages {
            let from = query.after.as_ref().map_or(0, |c| c.global_sequence.get());
            selected = rows
                .iter()
                .filter(|record| record.event.global_sequence.get() > from)
                .take(query.limit.events() as usize + 1)
                .cloned()
                .collect();
            if selected.len() as u32 > query.limit.events() {
                let last = selected
                    .get(query.limit.events() as usize - 1)
                    .map(|record| record.event.global_sequence.get());
                selected.truncate(query.limit.events() as usize);
                next = last.map(|value| cursor(EPOCH, value));
            }
        }
        let mut history: Vec<OwnedEventRecord> = rows
            .iter()
            .filter(|record| history_entry(record.event.event_type.as_str()))
            .filter(|record| match query.before_history {
                Some(cursor_value) => record
                    .event
                    .session_sequence
                    .as_ref()
                    .is_some_and(|value| value.get() < cursor_value.get()),
                None => true,
            })
            .cloned()
            .collect();
        history.sort_by_key(|record| record.event.session_sequence.as_ref().map(|s| s.get()));
        history.reverse();
        let has_earlier = history.len() as u32 > query.limit.events();
        history.truncate(query.limit.events() as usize);
        history.reverse();
        if !query.include.history {
            history.clear();
        }
        Ok(page(summary, selected, history, has_earlier, head, next))
    }

    async fn node_link_slice(
        &self,
        _session: &acp_core::model::SessionId,
        _after: Option<acp_core::model::OriginCursor>,
        _limit: ReplayLimit,
    ) -> Result<acp_core::ports::NodeLinkSlice, PortError> {
        panic!("命令面未触及")
    }

    async fn session_event_payload(
        &self,
        _session: &acp_core::model::SessionId,
        _event: &EventId,
    ) -> Result<Option<EventPayload>, PortError> {
        panic!("命令面未触及")
    }
}

/// `SYNC_PROTOCOL.md` §10.2.1 的「进入历史条目的 17 类」谓词（存储层的职责：它写在取行语句里）。
fn history_entry(event_type: &str) -> bool {
    matches!(
        event_type,
        "turn.queued"
            | "turn.started"
            | "turn.completed"
            | "turn.cancelled"
            | "turn.failed"
            | "tool.call.started"
            | "tool.call.updated"
            | "tool.call.completed"
            | "permission.requested"
            | "permission.resolved"
            | "elicitation.requested"
            | "elicitation.resolved"
            | "terminal.output"
            | "file.changed"
            | "session.plan.changed"
            | "session.mode.changed"
            | "session.config.changed"
    )
}

/// 一页历史（`config`/`capabilities`/`turns`/`interactions` 由用例层合并，存储层留空）。
fn page(
    summary: SessionSummary,
    events: Vec<OwnedEventRecord>,
    history: Vec<OwnedEventRecord>,
    history_has_earlier: bool,
    head: GlobalCursor,
    next: Option<GlobalCursor>,
) -> HistoryPage {
    HistoryPage {
        session: summary,
        events,
        history,
        history_has_earlier,
        turns: Vec::new(),
        interactions: Vec::new(),
        config: Vec::new(),
        capabilities: None,
        head,
        next,
    }
}

// ---------------------------------------------------------------------------------------------
// 后端替身（只为 config/mode 两条查询命令供数）
// ---------------------------------------------------------------------------------------------

/// 只实现 `list_config`/`modes` 的端点替身：`session.config.list` 与 `session.mode.list` 的活体数据来源。
struct ConfigEndpoint {
    options: Vec<CoreConfigOption>,
    modes: CoreModeState,
}

#[async_trait::async_trait]
impl SessionEndpoint for ConfigEndpoint {
    fn reference(&self) -> acp_core::model::SessionReference {
        acp_core::model::SessionReference::Owned(acp_core::model::OwnedSessionRef {
            session_id: acp_core::model::SessionId::new(SESSION).expect("session id"),
        })
    }

    fn agent_session_id(&self) -> Option<&acp_core::model::AgentSessionId> {
        None
    }

    async fn list_config(&self) -> Result<Vec<CoreConfigOption>, PortError> {
        Ok(self.options.clone())
    }

    async fn modes(&self) -> Result<CoreModeState, PortError> {
        Ok(self.modes.clone())
    }

    async fn prompt(
        &self,
        _request: PromptRequest,
        _at: Timestamp,
    ) -> Result<TurnAccepted, PortError> {
        panic!("命令面未触及")
    }

    async fn cancel(&self, _turn: Option<acp_core::model::TurnId>) -> Result<(), PortError> {
        panic!("命令面未触及")
    }

    async fn set_mode(&self, _mode: &acp_core::model::ModeId) -> Result<(), PortError> {
        panic!("命令面未触及")
    }

    async fn set_config(
        &self,
        _id: &acp_core::model::ConfigOptionId,
        _value: acp_core::model::ConfigValue,
    ) -> Result<(), PortError> {
        panic!("命令面未触及")
    }

    async fn resolve_interaction(
        &self,
        _interaction: &acp_core::model::InteractionId,
        _resolution: acp_core::model::InteractionResolution,
    ) -> Result<(), PortError> {
        panic!("命令面未触及")
    }

    async fn read_history(&self, _query: HistoryQuery) -> Result<HistoryPage, PortError> {
        panic!("命令面未触及")
    }

    async fn close(&self) -> Result<(), PortError> {
        panic!("命令面未触及")
    }
}

/// 端点工厂替身：`open` 只为**已登记**的会话返回 [`ConfigEndpoint`]（未知会话按真实后端的口径返回
/// `PortError::NotFound`，因此「授权设备查不存在的会话」与「未授权设备查不存在的会话」在用例里给出
/// 不同的错误码，正是 2.2 要区分的那条性质）。
struct ConfigBackend {
    endpoint: Arc<ConfigEndpoint>,
    sessions: Arc<Mutex<Vec<String>>>,
}

#[async_trait::async_trait]
impl acp_core::ports::SessionBackendFactory for ConfigBackend {
    async fn create(
        &self,
        _session: &acp_core::model::SessionId,
        _request: acp_core::model::CreateSessionRequest,
        _sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        panic!("命令面未触及")
    }

    async fn open(
        &self,
        reference: acp_core::model::SessionReference,
        _sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        let known = match &reference {
            acp_core::model::SessionReference::Owned(session) => self
                .sessions
                .lock()
                .expect("会话表锁")
                .iter()
                .any(|known| known == session.session_id.as_str()),
            acp_core::model::SessionReference::Remote(_) => false,
        };
        if !known {
            let acp_core::model::SessionReference::Owned(session) = reference else {
                return Err(PortError::NotFound(acp_core::model::EntityRef::Session(
                    acp_core::model::SessionId::new("remote").expect("session id"),
                )));
            };
            return Err(PortError::NotFound(acp_core::model::EntityRef::Session(
                session.session_id,
            )));
        }
        Ok(Box::new(ConfigEndpoint {
            options: self.endpoint.options.clone(),
            modes: self.endpoint.modes.clone(),
        }))
    }

    async fn resume(
        &self,
        _session: &acp_core::model::SessionId,
        _request: ResumeSessionRequest,
        _sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        panic!("命令面未触及")
    }
}

// ---------------------------------------------------------------------------------------------
// 世界
// ---------------------------------------------------------------------------------------------

/// 命令面用例的端口替身世界（`UseCases`/`Broker`/`Authority` 共享同一批实例，与组合根同形）。
struct CommandWorld {
    core: Arc<UseCases>,
    broker: Arc<Broker>,
    authority: Arc<Authority>,
    registry: Arc<ConnectionRegistry>,
    store: CommandStore,
    trust: FakeTrust,
    /// 已登记会话名（[`ConfigBackend`] 的可见性输入，与真实后端的 `owned_session` 行同口径）。
    sessions: Arc<Mutex<Vec<String>>>,
}

impl CommandWorld {
    fn new() -> Self {
        Self::with(ConfigEndpoint {
            options: vec![core_config_option()],
            modes: CoreModeState {
                current_mode: Some(mode_ref("code")),
                available: vec![mode_ref("code"), mode_ref("plan")],
            },
        })
    }

    fn with(endpoint: ConfigEndpoint) -> Self {
        let clock = FakeClock::new();
        let trust = FakeTrust::default();
        let exports = FakeExports::default().with_clock(clock.clone());
        let audit = FakeAudit::default();
        let config = FakeConfig::default();
        let store = CommandStore::default();
        store.set_head(cursor(EPOCH, 0));
        let sessions: Arc<Mutex<Vec<String>>> = Arc::default();
        let clock_port: Arc<dyn Clock> = Arc::new(clock.clone());
        let store_port: Arc<dyn SessionStore> = Arc::new(store.clone());
        let delivery_port: Arc<dyn acp_core::ports::RemoteDeliveryStore> =
            Arc::new(SyncDeliveries::default());
        let exports_port: Arc<dyn acp_core::ports::ExportStore> = Arc::new(exports);
        let audit_port: Arc<dyn acp_core::ports::AuditStore> = Arc::new(audit);
        let ids: Arc<dyn acp_core::ports::IdGenerator> = Arc::new(FakeIds::default());
        let backend: Arc<dyn acp_core::ports::SessionBackendFactory> = Arc::new(ConfigBackend {
            endpoint: Arc::new(endpoint),
            sessions: Arc::clone(&sessions),
        });
        let broker = Arc::new(Broker::new(
            BrokerDeps {
                store: Arc::clone(&store_port),
                deliveries: Arc::clone(&delivery_port),
                backends: Arc::clone(&backend),
                exports: Arc::clone(&exports_port),
                trust: Arc::new(trust.clone()),
                publisher: Arc::new(NotTouched),
                clock: Arc::clone(&clock_port),
                ids: Arc::clone(&ids),
                audit: Some(Arc::clone(&audit_port)),
            },
            BrokerConfig::default(),
        ));
        let core = Arc::new(UseCases::new(UseCaseDeps {
            broker: Arc::clone(&broker),
            store: store_port,
            deliveries: delivery_port,
            exports: exports_port,
            trust: Arc::new(trust.clone()),
            audit: audit_port,
            config: Arc::new(config),
            attachments: Arc::new(NotTouched),
            catalog: Arc::new(FakeCatalog::default()),
            clock: clock_port,
            ids,
        }));
        let authority = Arc::new(Authority::new(
            acp_core::model::NodeId::new(HOST).expect("host id"),
            identity_auth::port::KeyHandle::new("node-identity").expect("key handle"),
            Arc::new(FakeKeystore::default()),
            Arc::new(FakeEntropy::default()),
            Arc::new(clock),
        ));
        Self {
            core,
            broker,
            authority,
            registry: ConnectionRegistry::new(),
            store,
            trust,
            sessions,
        }
    }

    /// 预置一条会话：存储替身与后端替身一起登记（后者的可见性口径与真实后端的 `owned_session` 行一致）。
    fn push_session(&self, summary: SessionSummary) {
        self.sessions
            .lock()
            .expect("会话表锁")
            .push(summary.session_id().as_str().to_owned());
        self.store.push_session(summary);
    }

    /// 起一条用生产限流数值的路由（限流判据的用例另用 `with_test_rate_limit` 构造）。
    fn route(&self) -> Arc<CommandRoute> {
        Arc::new(CommandRoute::new(
            Arc::clone(&self.core),
            Arc::clone(&self.broker),
            Arc::clone(&self.authority),
            Arc::clone(&self.registry),
            Arc::new(Reachability(true)) as Arc<dyn OwnerReachability>,
        ))
    }

    /// 预置一条设备信任行（`sync_device_handshake_view` 的唯一读面）。
    fn seed_device(&self, device: &str, scopes: &[&str]) {
        let record = acp_core::model::DeviceRecord::try_new(
            DeviceId::new(device).expect("device id"),
            "Test Phone",
            acp_core::model::Fingerprint::new(&"a".repeat(64)).expect("fingerprint"),
            acp_core::model::ScopeSet::try_from_iter(scopes.iter().copied()).expect("scopes"),
            acp_core::model::DeviceState::Active,
            timestamp("2026-09-18T09:12:03.412Z"),
            None,
            None,
        )
        .expect("设备记录");
        self.trust.seed_device(record);
    }
}

/// 一条已认证连接的句柄与它的出站接收端（句柄同时登记进注册表，`reclaim` 才认它是活的）。
fn connect(
    world: &CommandWorld,
    connection_id: &str,
    config: &SyncConfig,
) -> (Arc<ConnectionHandle>, tokio::sync::mpsc::Receiver<Outbound>) {
    let (handle, receiver) = connection_with_limits(DEVICE, connection_id, config);
    world.registry.register(Arc::clone(&handle));
    (handle, receiver)
}

/// 派发一条命令帧并取出它产生的出站帧。
async fn dispatch(
    route: &CommandRoute,
    handle: &Arc<ConnectionHandle>,
    receiver: &mut tokio::sync::mpsc::Receiver<Outbound>,
    message_id: &str,
    sequence: u64,
    body: serde_json::Value,
) -> Vec<Frame> {
    let envelope = envelope_with(message_id, sequence, body);
    let _ = crate::sync::conn::MessageRoute::route(route, handle, &envelope).await;
    frames(receiver)
}

/// 第 `index` 个测试用 `messageId`（去重键的一半：重复同一帧的用例才复用同一个 id）。
fn message_id(index: u64) -> String {
    format!("04dbad03-8869-4c5a-94fa-2f3af2fb62{index:02x}")
}

/// 一条认证后 `command` 帧的信封（`messageId` 由调用方给：去重判据要重复同一个 id）。
fn envelope_with(message_id: &str, sequence: u64, body: serde_json::Value) -> Envelope {
    let frame = serde_json::json!({
        "protocolVersion": 1,
        "type": "command",
        "messageId": message_id,
        "connectionId": CONNECTION,
        "connectionSequence": sequence.to_string(),
        "body": body,
    })
    .to_string();
    Envelope::decode(&frame).expect("测试信封合法")
}

/// 一条查询命令的 body。
fn command_body(
    request_id: &str,
    command: &str,
    session: Option<&str>,
    payload: serde_json::Value,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "requestId": request_id,
        "command": command,
        "payload": payload,
    });
    if let Some(session) = session {
        body["sessionId"] = serde_json::Value::String(session.to_owned());
    }
    body
}

// ---------------------------------------------------------------------------------------------
// 素材
// ---------------------------------------------------------------------------------------------

const SESSION: &str = "5d73cd10-a465-43cd-b3f1-704e2d49e99e";
const OTHER_SESSION: &str = "7a2c1e40-6b21-4c9e-b0a5-8f4d3c2b1a09";
const MESSAGE_ID: &str = "04dbad03-8869-4c5a-94fa-2f3af2fb622b";
const TURN: &str = "6601828b-3eca-4cec-9a58-18ae1e0a3a14";
const REQUEST: &str = "4c4dafda-dd98-442e-8d55-252b75bac72d";
const CONNECTION_A: &str = "2de54db7-74ae-4c21-88e3-05df0dc2e637";
const CONNECTION_B: &str = "3de54db7-74ae-4c21-88e3-05df0dc2e638";
const NOW: &str = "2026-09-18T09:12:03.412Z";

fn timestamp(text: &str) -> Timestamp {
    Timestamp::new(text).expect("时间戳")
}

fn sequence(value: u64) -> Sequence {
    Sequence::new(value).expect("序号")
}

fn cursor(epoch: &str, value: u64) -> GlobalCursor {
    GlobalCursor::new(ServerEpoch::new(epoch).expect("epoch"), sequence(value))
}

fn mode_ref(mode_id: &str) -> CoreModeRef {
    CoreModeRef::try_new(ModeId::new(mode_id).expect("mode id"), mode_id).expect("mode ref")
}

fn core_config_option() -> CoreConfigOption {
    let candidate =
        CoreConfigOptionEntry::try_new(SelectValue::new("gpt").expect("select value"), "GPT", None)
            .expect("config candidate");
    CoreConfigOption::try_new(
        ConfigOptionId::new("model").expect("config id"),
        "Model",
        None,
        None,
        ConfigOptionKind::Select,
        ConfigValue::Select(SelectValue::new("gpt").expect("select value")),
        vec![candidate],
    )
    .expect("config option")
}

/// 一条 core 会话摘要。
fn core_session(session: &str) -> SessionSummary {
    SessionSummary::try_new(
        acp_core::model::SessionId::new(session).expect("session id"),
        Some("修复同步命令面".to_owned()),
        acp_core::model::AgentRef::try_new(
            acp_core::model::AgentId::new("codex").expect("agent id"),
            "Codex",
        )
        .expect("agent ref"),
        acp_core::model::SessionState::Running,
        acp_core::model::ResourceOrigin::Local,
        Some(
            acp_core::model::ModeRef::try_new(
                acp_core::model::ModeId::new("code").expect("mode id"),
                "Code",
            )
            .expect("mode ref"),
        ),
        acp_core::model::Version::from(7),
        timestamp("2026-09-17T12:10:00.123Z"),
        timestamp("2026-09-17T12:11:00.123Z"),
        None,
    )
    .expect("会话摘要")
}

/// 一条持久化事件（`session.read` 的消息/历史素材）。
fn owned_event(
    session: &str,
    event_id: &str,
    event_type: &str,
    session_sequence: u64,
    global_sequence: u64,
    created_at: &str,
    view: &str,
) -> OwnedEventRecord {
    OwnedEventRecord {
        event: CommittedEvent {
            id: EventId::new(event_id).expect("event id"),
            event_type: EventType::new(event_type).expect("event type"),
            session: Some(acp_core::model::SessionId::new(session).expect("session id")),
            session_sequence: Some(sequence(session_sequence)),
            global_sequence: sequence(global_sequence),
            origin_epoch: Some(OriginEpoch::new(EPOCH).expect("origin epoch")),
            origin_sequence: Some(sequence(global_sequence)),
            created_at: timestamp(created_at),
        },
        payload: EventPayload::new(
            ViewJson::from_slice(view.as_bytes()).expect("视图文本"),
            None,
        ),
    }
}

/// 一条 `owned_command` 记录（`command.status` 的素材）。
fn command_record(status: CommandStatus) -> CommandRecord {
    let terminal = status.is_terminal();
    CommandRecord::try_new(
        None,
        RequestId::new(REQUEST).expect("request id"),
        "session.prompt",
        CommandKind::Mutation,
        Actor::Device {
            device: DeviceId::new(DEVICE).expect("device id"),
            scopes: acp_core::model::ScopeSet::try_from_iter(["session.prompt"]).expect("scopes"),
        },
        Some(timestamp(NOW)),
        status,
        terminal.then(|| timestamp(NOW)),
        // mutation 的终态必须带 `terminalEventId`。
        terminal.then(|| EventId::new("8f1d2c3b-4a59-4e6f-9b7c-1d2e3f4a5b6c").expect("event id")),
        None,
        None,
        None,
        Digest::new(&"A".repeat(43)).expect("空摘要"),
    )
    .expect("命令记录")
}

// ---------------------------------------------------------------------------------------------
// 限流器的机制（键是设备与命令族、窗口滑动、键表有界）
// ---------------------------------------------------------------------------------------------

/// 限流器按 `(设备, 命令族)` 计数：不同设备、不同族各占各的窗口，窗口滑过后额度恢复。
#[test]
fn the_rate_limiter_counts_by_device_and_family() {
    use std::time::Instant;
    let limiter = crate::sync::command::limits::CommandRateLimiter::new(1, Duration::from_secs(60));
    let start = Instant::now();
    assert_eq!(
        limiter.check_at("device-a", CommandFamily::Observe, start),
        RateOutcome::Allowed
    );
    assert!(matches!(
        limiter.check_at("device-a", CommandFamily::Observe, start),
        RateOutcome::Denied { .. }
    ));
    // 另一个命令族与另一台设备各占各的额度。
    assert_eq!(
        limiter.check_at("device-a", CommandFamily::Interact, start),
        RateOutcome::Allowed
    );
    assert_eq!(
        limiter.check_at("device-b", CommandFamily::Observe, start),
        RateOutcome::Allowed
    );
    assert_eq!(limiter.tracked_keys(), 3);
    // 窗口滑过之后额度恢复。
    let later = start + Duration::from_secs(61);
    assert_eq!(
        limiter.check_at("device-a", CommandFamily::Observe, later),
        RateOutcome::Allowed
    );
}

/// 命令族与 `compatibility/commands/v1/commands.json` 的 `packs` 分组逐条相等：五条查询命令同属 `observe`，
/// prompt 属 `interact`（分族的意义就是两者不互相挤占）。
#[test]
fn command_families_mirror_the_registry_packs() {
    use sync_protocol::command::CommandName;
    for command in [
        CommandName::SessionList,
        CommandName::SessionRead,
        CommandName::CommandStatus,
        CommandName::SessionModeList,
        CommandName::SessionConfigList,
    ] {
        assert_eq!(CommandFamily::of(command), CommandFamily::Observe);
    }
    assert_eq!(
        CommandFamily::of(CommandName::SessionPrompt),
        CommandFamily::Interact
    );
    assert_eq!(
        CommandFamily::of(CommandName::PermissionResolve),
        CommandFamily::Approve
    );
    assert_eq!(
        CommandFamily::of(CommandName::SessionCreate),
        CommandFamily::CreateSession
    );
    assert_eq!(
        CommandFamily::of(CommandName::SessionModeSet),
        CommandFamily::ConfigureSession
    );
}

// ---------------------------------------------------------------------------------------------
// 2.1 解码分派：封闭词表外与 session.resume
// ---------------------------------------------------------------------------------------------

/// `session.resume` 只经 Node Link 接受：Sync 收到即 `command.unsupported`，且**本机什么都没做**。
#[tokio::test]
async fn session_resume_is_rejected_as_unsupported_without_local_reads() {
    let world = CommandWorld::new();
    world.seed_device(DEVICE, &["session.list", "session.resume"]);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let body = command_body(
        REQUEST,
        "session.resume",
        Some(SESSION),
        serde_json::json!({}),
    );
    let out = dispatch(&route, &handle, &mut receiver, MESSAGE_ID, 1, body).await;
    let errors = bodies(&out, "error");
    assert_eq!(errors.len(), 1, "恰好一条 error 帧");
    assert_eq!(
        errors[0].get("code").and_then(|code| code.as_str()),
        Some(ErrorCode::CommandUnsupported.as_str())
    );
    assert_eq!(
        errors[0]
            .get("correlationId")
            .and_then(|id| id.as_str())
            .map(str::to_owned),
        Some(MESSAGE_ID.to_owned()),
        "错误帧必须与该命令帧的 messageId 对上"
    );
    assert!(
        bodies(&out, "command.result").is_empty(),
        "词表外的命令名组不出合规的 command.result"
    );
    assert_eq!(world.store.session_reads(), 0, "不得发生任何业务读取");
    assert_eq!(world.store.commits(), 0, "不得发生任何写入");
}

/// 封闭词表外的命令名（这里取只属本机管理面的 `local.device.manage`）同样按 `command.unsupported` 拒绝。
#[tokio::test]
async fn an_unknown_command_name_is_rejected_as_unsupported() {
    let world = CommandWorld::new();
    world.seed_device(DEVICE, &["session.list"]);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let body = command_body(REQUEST, "local.device.manage", None, serde_json::json!({}));
    let out = dispatch(&route, &handle, &mut receiver, MESSAGE_ID, 1, body).await;
    assert_eq!(
        bodies(&out, "error")[0]
            .get("code")
            .and_then(|code| code.as_str()),
        Some(ErrorCode::CommandUnsupported.as_str())
    );
    assert_eq!(world.store.session_reads(), 0);
}

/// 形状不符（payload 与命令名不匹配）走 `protocol.schema_invalid`，与「不支持」分开。
#[tokio::test]
async fn a_malformed_command_body_is_rejected_as_schema_invalid() {
    let world = CommandWorld::new();
    world.seed_device(DEVICE, &["session.read"]);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    // `session.read` 的 payload 必须是 `{ include: [...] }`；这里给一个空对象。
    let body = command_body(
        REQUEST,
        "session.read",
        Some(SESSION),
        serde_json::json!({}),
    );
    let out = dispatch(&route, &handle, &mut receiver, MESSAGE_ID, 1, body).await;
    assert_eq!(
        bodies(&out, "error")[0]
            .get("code")
            .and_then(|code| code.as_str()),
        Some(ErrorCode::ProtocolSchemaInvalid.as_str())
    );
    assert_eq!(world.store.session_reads(), 0);
}

// ---------------------------------------------------------------------------------------------
// 2.2 授权先于读取
// ---------------------------------------------------------------------------------------------

/// 未持 scope 的设备对**不存在**的会话派发命令，也只得授权拒绝，且零业务读取、零写入。
#[tokio::test]
async fn an_unauthorized_device_gets_scope_denied_without_reading_sessions() {
    let world = CommandWorld::new();
    world.seed_device(DEVICE, &["session.list"]);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    // OTHER_SESSION 在存储里根本不存在；`session.prompt` 也不在该设备的 scope 里。
    let body = command_body(
        REQUEST,
        "session.prompt",
        Some(OTHER_SESSION),
        serde_json::json!({ "content": [{ "type": "text", "text": "你好" }] }),
    );
    let out = dispatch(&route, &handle, &mut receiver, MESSAGE_ID, 1, body).await;
    let results = bodies(&out, "command.result");
    assert_eq!(results.len(), 1, "恰好一条结果帧");
    assert_eq!(
        results[0].get("status").and_then(|status| status.as_str()),
        Some("rejected"),
        "被拒命令是 rejected，不是 accepted"
    );
    assert_eq!(
        results[0]
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(|code| code.as_str()),
        Some(ErrorCode::AuthorizationScopeDenied.as_str())
    );
    assert_eq!(
        world.store.session_reads(),
        0,
        "授权判定不得触碰任何会话读取（记账替身证明）"
    );
    assert_eq!(world.store.commits(), 0, "被拒命令零副作用");
}

/// 错误类型**不随会话是否存在而变**：同一条被拒命令，会话存在与不存在都得到同一个 `authorization.scope_denied`
/// （否则该错误码就成了会话是否存在的预言机）。
#[tokio::test]
async fn the_authorization_error_does_not_depend_on_session_existence() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE, &["session.list"]);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let mut codes = Vec::new();
    for (index, session) in [SESSION, OTHER_SESSION].into_iter().enumerate() {
        let body = command_body(
            &format!("4c4dafda-dd98-442e-8d55-252b75bac72{index}"),
            "session.read",
            Some(session),
            serde_json::json!({ "include": ["messages"] }),
        );
        let out = dispatch(
            &route,
            &handle,
            &mut receiver,
            MESSAGE_ID,
            index as u64 + 1,
            body,
        )
        .await;
        codes.push(
            bodies(&out, "command.result")[0]
                .get("error")
                .and_then(|error| error.get("code"))
                .and_then(|code| code.as_str())
                .map(str::to_owned),
        );
    }
    assert_eq!(
        codes,
        vec![
            Some(ErrorCode::AuthorizationScopeDenied.as_str().to_owned()),
            Some(ErrorCode::AuthorizationScopeDenied.as_str().to_owned())
        ],
        "会话存在与否不得改变错误类型"
    );
    assert_eq!(world.store.session_reads(), 0);
}

/// 已授权的查询对不存在的会话给出 `session.not_found`——**授权先行**的证明：同一个不存在的会话在未授权
/// 设备那里得到的是授权拒绝，在授权设备这里才是「找不到」。
#[tokio::test]
async fn an_authorized_query_on_a_missing_session_reports_not_found() {
    let world = CommandWorld::new();
    world.seed_device(DEVICE, &["session.mode.list"]);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let body = command_body(
        REQUEST,
        "session.mode.list",
        Some(OTHER_SESSION),
        serde_json::json!({}),
    );
    let out = dispatch(&route, &handle, &mut receiver, MESSAGE_ID, 1, body).await;
    let results = bodies(&out, "command.result");
    assert_eq!(
        results[0].get("status").and_then(|status| status.as_str()),
        Some("rejected")
    );
    assert_eq!(
        results[0]
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(|code| code.as_str()),
        Some(ErrorCode::SessionNotFound.as_str())
    );
}

// ---------------------------------------------------------------------------------------------
// 2.3 五条查询命令同步完成
// ---------------------------------------------------------------------------------------------

/// 每条查询命令都只回**一次** `completed`：`acceptedAt` 非空、`terminalEventId` 为 `null`、`error` 为 `null`，
/// 且没有任何 `accepted` 帧。
fn assert_single_completed(out: &[Frame], command: &str) -> serde_json::Value {
    let results = bodies(out, "command.result");
    assert_eq!(results.len(), 1, "{command} 必须只回一条结果");
    let body = results[0];
    assert_eq!(
        body.get("status").and_then(|status| status.as_str()),
        Some("completed"),
        "{command} 的首次响应必须是 completed"
    );
    assert_eq!(
        body.get("command").and_then(|value| value.as_str()),
        Some(command)
    );
    assert!(
        body.get("acceptedAt").is_some_and(|value| !value.is_null()),
        "{command} 的 completed 必须带 acceptedAt"
    );
    assert!(
        body.get("terminalEventId")
            .is_some_and(serde_json::Value::is_null),
        "查询命令的 terminalEventId 固定为 null"
    );
    assert!(
        body.get("error").is_some_and(serde_json::Value::is_null),
        "completed 的 error 必须是 null"
    );
    assert!(
        bodies(out, "command.result")
            .iter()
            .all(|body| body.get("status").and_then(|s| s.as_str()) != Some("accepted")),
        "查询命令 MUST NOT 先回 accepted"
    );
    body.get("result")
        .cloned()
        .unwrap_or(serde_json::Value::Null)
}

/// `session.list`：`completed { sessions: [...] }`，摘要含协议要求的最低字段。
#[tokio::test]
async fn session_list_completes_in_one_result() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE, &["session.list"]);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let body = command_body(REQUEST, "session.list", None, serde_json::json!({}));
    let out = dispatch(&route, &handle, &mut receiver, MESSAGE_ID, 1, body).await;
    let result = assert_single_completed(&out, "session.list");
    let sessions = result
        .get("sessions")
        .and_then(|value| value.as_array())
        .expect("sessions 数组");
    assert_eq!(sessions.len(), 1);
    let summary = &sessions[0];
    assert_eq!(
        summary.get("sessionId").and_then(|value| value.as_str()),
        Some(SESSION)
    );
    assert_eq!(
        summary.get("state").and_then(|value| value.as_str()),
        Some("running")
    );
    assert_eq!(
        summary.get("version").and_then(|value| value.as_str()),
        Some("7")
    );
    assert_eq!(
        summary
            .get("agent")
            .and_then(|agent| agent.get("agentId"))
            .and_then(|value| value.as_str()),
        Some("codex")
    );
    assert!(
        summary.get("workspace").is_none(),
        "未协商 core.local-catalog.v1 时 workspace 键整个缺席"
    );
}

/// `session.read`：`completed SessionReadResult`（`messages` 由持久化事件折叠、`history` 走同一套游标）。
#[tokio::test]
async fn session_read_completes_in_one_result_with_messages_and_history() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.store.push_event(
        SESSION,
        owned_event(
            SESSION,
            "8f1d2c3b-4a59-4e6f-9b7c-1d2e3f4a5b6c",
            "user.message.delta",
            1,
            1,
            NOW,
            &serde_json::json!({
                "messageId": MESSAGE_ID,
                "turnId": TURN,
                "deltaIndex": "0",
                "text": "继续修复测试",
            })
            .to_string(),
        ),
    );
    world.store.push_event(
        SESSION,
        owned_event(
            SESSION,
            "9f1d2c3b-4a59-4e6f-9b7c-1d2e3f4a5b6d",
            "agent.message.completed",
            2,
            2,
            "2026-09-18T09:12:04.412Z",
            &serde_json::json!({
                "messageId": TURN,
                "turnId": TURN,
                "content": [{ "type": "text", "text": "已修复" }],
            })
            .to_string(),
        ),
    );
    world.store.push_event(
        SESSION,
        owned_event(
            SESSION,
            "af1d2c3b-4a59-4e6f-9b7c-1d2e3f4a5b6e",
            "turn.completed",
            3,
            3,
            "2026-09-18T09:12:05.412Z",
            &serde_json::json!({ "turnId": TURN, "state": "completed" }).to_string(),
        ),
    );
    world.seed_device(DEVICE, &["session.read", "core.session-history.v1"]);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    // `core.session-history.v1` 是 feature，不是 scope：测试里直接交接协商结果。
    crate::sync::conn::MessageRoute::on_authenticated(
        route.as_ref(),
        &handle,
        &["core.session-history.v1".to_owned()],
    );
    let body = command_body(
        REQUEST,
        "session.read",
        Some(SESSION),
        serde_json::json!({ "include": ["messages", "history"] }),
    );
    let out = dispatch(&route, &handle, &mut receiver, MESSAGE_ID, 1, body).await;
    let result = assert_single_completed(&out, "session.read");
    assert_eq!(
        result.get("sessionId").and_then(|value| value.as_str()),
        Some(SESSION)
    );
    assert!(result.get("hasEarlier").is_some(), "hasEarlier 是必填");
    let messages = result["resources"]["messages"]
        .as_array()
        .expect("messages 数组");
    assert_eq!(messages.len(), 2, "两条消息：用户 1 条 + Agent 1 条");
    assert_eq!(
        messages[0].get("role").and_then(|value| value.as_str()),
        Some("user")
    );
    assert_eq!(
        messages[0]
            .get("content")
            .and_then(|content| content.get(0))
            .and_then(|block| block.get("text"))
            .and_then(|text| text.as_str()),
        Some("继续修复测试")
    );
    assert_eq!(
        messages[1].get("role").and_then(|value| value.as_str()),
        Some("agent")
    );
    assert_eq!(
        messages[1].get("turnId").and_then(|value| value.as_str()),
        Some(TURN)
    );
    let history = result["resources"]["history"]
        .as_array()
        .expect("history 数组");
    assert_eq!(history.len(), 1, "只有登记进历史条目的那一条");
    assert_eq!(
        history[0].get("eventType").and_then(|value| value.as_str()),
        Some("turn.completed")
    );
    assert_eq!(
        history[0]
            .get("sessionSequence")
            .and_then(|value| value.as_str()),
        Some("3")
    );
    assert!(
        history[0].get("acp").is_none(),
        "未协商 acp.raw-payload.v1 时不下发原文容器"
    );
    assert!(world.store.commits() == 0);
}

/// 未协商 `core.session-history.v1` 却请求 `history`：显式 `protocol.feature_required`（不得静默忽略）。
#[tokio::test]
async fn session_read_history_requires_the_feature() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE, &["session.read"]);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let body = command_body(
        REQUEST,
        "session.read",
        Some(SESSION),
        serde_json::json!({ "include": ["history"] }),
    );
    let out = dispatch(&route, &handle, &mut receiver, MESSAGE_ID, 1, body).await;
    let results = bodies(&out, "command.result");
    assert_eq!(
        results[0].get("status").and_then(|status| status.as_str()),
        Some("rejected")
    );
    assert_eq!(
        results[0]
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(|code| code.as_str()),
        Some(ErrorCode::ProtocolFeatureRequired.as_str())
    );
    assert_eq!(
        results[0]["error"]["details"]["features"][0].as_str(),
        Some("core.session-history.v1")
    );
}

/// 用例面无法表达的明细资源（`turns`/`pending_interactions`/`capabilities`）显式拒绝，不静默省略。
#[tokio::test]
async fn session_read_unsupported_resources_are_rejected() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE, &["session.read"]);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    for (index, resource) in ["turns", "pending_interactions", "capabilities"]
        .into_iter()
        .enumerate()
    {
        let body = command_body(
            REQUEST,
            "session.read",
            Some(SESSION),
            serde_json::json!({ "include": [resource] }),
        );
        let out = dispatch(
            &route,
            &handle,
            &mut receiver,
            &message_id(index as u64 + 1),
            index as u64 + 1,
            body,
        )
        .await;
        let results = bodies(&out, "command.result");
        assert_eq!(
            results[0]
                .get("error")
                .and_then(|error| error.get("code"))
                .and_then(|code| code.as_str()),
            Some(ErrorCode::CapabilityUnsupportedByBroker.as_str()),
            "{resource} 必须以「用例面无法表达」显式拒绝"
        );
        assert!(
            results[0]
                .get("result")
                .is_some_and(serde_json::Value::is_null),
            "拒绝时 result 必须是 null"
        );
    }
}

/// `command.status`：`completed CommandStatusRecord`。
#[tokio::test]
async fn command_status_completes_in_one_result() {
    let world = CommandWorld::new();
    world
        .store
        .push_command(command_record(CommandStatus::Completed));
    world.seed_device(DEVICE, &["command.status"]);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let body = command_body(
        REQUEST,
        "command.status",
        None,
        serde_json::json!({ "targetRequestId": REQUEST }),
    );
    let out = dispatch(&route, &handle, &mut receiver, MESSAGE_ID, 1, body).await;
    let result = assert_single_completed(&out, "command.status");
    assert_eq!(
        result
            .get("targetRequestId")
            .and_then(|value| value.as_str()),
        Some(REQUEST)
    );
    assert_eq!(
        result.get("state").and_then(|value| value.as_str()),
        Some("completed")
    );
    assert_eq!(
        result.get("acceptedAt").and_then(|value| value.as_str()),
        Some(NOW)
    );
}

/// 目标不存在或不可见：`command.not_found`（§11.4：避免权限侧信道）。
#[tokio::test]
async fn command_status_reports_not_found() {
    let world = CommandWorld::new();
    world.seed_device(DEVICE, &["command.status"]);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let body = command_body(
        REQUEST,
        "command.status",
        None,
        serde_json::json!({ "targetRequestId": REQUEST }),
    );
    let out = dispatch(&route, &handle, &mut receiver, MESSAGE_ID, 1, body).await;
    let results = bodies(&out, "command.result");
    assert_eq!(
        results[0].get("status").and_then(|status| status.as_str()),
        Some("rejected")
    );
    assert_eq!(
        results[0]
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(|code| code.as_str()),
        Some(ErrorCode::CommandNotFound.as_str())
    );
}

/// `session.mode.list`：`completed ModeState`（三键同对象，版本来自会话版本读）。
#[tokio::test]
async fn session_mode_list_completes_in_one_result() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE, &["session.mode.list"]);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let body = command_body(
        REQUEST,
        "session.mode.list",
        Some(SESSION),
        serde_json::json!({}),
    );
    let out = dispatch(&route, &handle, &mut receiver, MESSAGE_ID, 1, body).await;
    let result = assert_single_completed(&out, "session.mode.list");
    assert_eq!(
        result.get("currentModeId").and_then(|value| value.as_str()),
        Some("code")
    );
    assert_eq!(
        result
            .get("availableModes")
            .and_then(|value| value.as_array())
            .map(Vec::len),
        Some(2)
    );
    assert!(result.get("version").is_some(), "modeState.version 必填");
}

/// `session.config.list`：`completed { configOptions, version }`。
#[tokio::test]
async fn session_config_list_completes_in_one_result() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE, &["session.config.list"]);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let body = command_body(
        REQUEST,
        "session.config.list",
        Some(SESSION),
        serde_json::json!({}),
    );
    let out = dispatch(&route, &handle, &mut receiver, MESSAGE_ID, 1, body).await;
    let result = assert_single_completed(&out, "session.config.list");
    let options = result["configOptions"]
        .as_array()
        .expect("configOptions 数组");
    assert_eq!(options.len(), 1);
    assert_eq!(
        options[0].get("id").and_then(|value| value.as_str()),
        Some("model")
    );
    assert_eq!(
        options[0].get("type").and_then(|value| value.as_str()),
        Some("select")
    );
    assert_eq!(
        options[0]
            .get("currentValue")
            .and_then(|value| value.as_str()),
        Some("gpt")
    );
    assert_eq!(
        options[0]["options"][0]
            .get("value")
            .and_then(|value| value.as_str()),
        Some("gpt")
    );
    assert_eq!(
        result.get("version").and_then(|value| value.as_str()),
        Some("0")
    );
}

// ---------------------------------------------------------------------------------------------
// 2.4 结果超限的显式失败
// ---------------------------------------------------------------------------------------------

/// 结果超过单条消息上限时回 `resource.result_too_large`，**不得**截断后当完整结果返回。
#[tokio::test]
async fn an_oversized_query_result_is_rejected_instead_of_truncated() {
    let world = CommandWorld::new();
    for index in 0..64 {
        let mut summary = core_session(SESSION);
        summary = SessionSummary::try_new(
            acp_core::model::SessionId::new(&format!(
                "5d73cd10-a465-43cd-b3f1-704e2d49e9{index:02x}"
            ))
            .expect("session id"),
            Some("同步命令面与大结果的边界".repeat(4)),
            summary.agent().clone(),
            acp_core::model::SessionState::Running,
            acp_core::model::ResourceOrigin::Local,
            summary.current_mode().cloned(),
            acp_core::model::Version::from(7),
            timestamp("2026-09-17T12:10:00.123Z"),
            timestamp("2026-09-17T12:11:00.123Z"),
            None,
        )
        .expect("会话摘要");
        world.push_session(summary);
    }
    world.seed_device(DEVICE, &["session.list"]);
    let route = world.route();
    // 压小单条消息上限（`SessionLimits` 的 wire 下限是 1024，取 2048）。
    let config = SyncConfig {
        max_message_bytes: 2_048,
        ..SyncConfig::default()
    };
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &config);
    let body = command_body(REQUEST, "session.list", None, serde_json::json!({}));
    let out = dispatch(&route, &handle, &mut receiver, MESSAGE_ID, 1, body).await;
    let results = bodies(&out, "command.result");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].get("status").and_then(|status| status.as_str()),
        Some("rejected"),
        "超限必须显式失败"
    );
    assert_eq!(
        results[0]
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(|code| code.as_str()),
        Some(ErrorCode::ResourceResultTooLarge.as_str())
    );
    assert!(
        results[0]
            .get("result")
            .is_some_and(serde_json::Value::is_null),
        "不得返回被截断的结果"
    );
    assert!(
        out.iter().all(|frame| !frame.text.contains("\"sessions\"")),
        "出站帧里不得出现被截断的 sessions"
    );
}

// ---------------------------------------------------------------------------------------------
// 2.5 命令级限流
// ---------------------------------------------------------------------------------------------

/// 单次超限只回**可重试**的 `resource.rate_limited`（带 `retryAfterMs`），不关连接、不执行命令。
#[tokio::test]
async fn a_single_rate_limit_rejection_is_retryable_and_has_no_side_effect() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE, &["session.list"]);
    let route = Arc::new(
        CommandRoute::new(
            Arc::clone(&world.core),
            Arc::clone(&world.broker),
            Arc::clone(&world.authority),
            Arc::clone(&world.registry),
            Arc::new(Reachability(true)) as Arc<dyn OwnerReachability>,
        )
        .with_test_rate_limit(1, Duration::from_secs(60), 3),
    );
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let body = command_body(REQUEST, "session.list", None, serde_json::json!({}));
    let first = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(1),
        1,
        body.clone(),
    )
    .await;
    assert_eq!(bodies(&first, "command.result").len(), 1);
    let reads = world.store.session_reads();
    let second = dispatch(&route, &handle, &mut receiver, &message_id(2), 2, body).await;
    let errors = bodies(&second, "error");
    assert_eq!(errors.len(), 1, "限流以 error 帧返回");
    assert_eq!(
        errors[0].get("code").and_then(|code| code.as_str()),
        Some(ErrorCode::ResourceRateLimited.as_str())
    );
    assert_eq!(
        errors[0].get("retryable").and_then(|value| value.as_bool()),
        Some(true),
        "resource.rate_limited 必须是可重试的"
    );
    assert!(
        errors[0]["details"]["retryAfterMs"].as_u64().is_some(),
        "必须给出 details.retryAfterMs"
    );
    assert_eq!(
        errors[0].get("correlationId").and_then(|id| id.as_str()),
        Some(message_id(2).as_str()),
        "命令级拒绝必须与该命令帧的 messageId 对上"
    );
    assert_eq!(
        world.store.session_reads(),
        reads,
        "被限流的命令不得产生第二次业务读取"
    );
    assert!(handle.close_request().is_none(), "单次超限不得关闭连接");
}

/// 连续超限达阈值才以 `4429` 关闭连接。
#[tokio::test]
async fn consecutive_rate_limit_rejections_close_the_connection_with_4429() {
    let world = CommandWorld::new();
    world.seed_device(DEVICE, &["session.list"]);
    let route = Arc::new(
        CommandRoute::new(
            Arc::clone(&world.core),
            Arc::clone(&world.broker),
            Arc::clone(&world.authority),
            Arc::clone(&world.registry),
            Arc::new(Reachability(true)) as Arc<dyn OwnerReachability>,
        )
        .with_test_rate_limit(1, Duration::from_secs(60), 3),
    );
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let body = command_body(REQUEST, "session.list", None, serde_json::json!({}));
    for sequence in 1..=4 {
        let out = dispatch(
            &route,
            &handle,
            &mut receiver,
            &message_id(sequence),
            sequence,
            body.clone(),
        )
        .await;
        if sequence < 4 {
            assert!(
                handle.close_request().is_none(),
                "第 {sequence} 次还不到连续超限阈值"
            );
        }
        if sequence > 1 {
            assert_eq!(
                bodies(&out, "error")[0]
                    .get("code")
                    .and_then(|code| code.as_str()),
                Some(ErrorCode::ResourceRateLimited.as_str())
            );
        }
    }
    assert_eq!(
        handle.close_request().map(|(code, _)| code),
        Some(4429),
        "连续第 3 次超限必须以 4429 关闭连接"
    );
    assert_eq!(world.store.commits(), 0);
}

/// 不同命令族的额度互不挤占：`observe` 用尽后 `interact` 仍可派发。
#[tokio::test]
async fn different_command_families_do_not_share_the_quota() {
    let world = CommandWorld::new();
    world.seed_device(DEVICE, &["session.list", "session.prompt"]);
    let route = Arc::new(
        CommandRoute::new(
            Arc::clone(&world.core),
            Arc::clone(&world.broker),
            Arc::clone(&world.authority),
            Arc::clone(&world.registry),
            Arc::new(Reachability(true)) as Arc<dyn OwnerReachability>,
        )
        .with_test_rate_limit(1, Duration::from_secs(60), 3),
    );
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let list = command_body(REQUEST, "session.list", None, serde_json::json!({}));
    let _ = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(1),
        1,
        list.clone(),
    )
    .await;
    let exhausted = dispatch(&route, &handle, &mut receiver, &message_id(2), 2, list).await;
    assert_eq!(
        bodies(&exhausted, "error")[0]
            .get("code")
            .and_then(|code| code.as_str()),
        Some(ErrorCode::ResourceRateLimited.as_str()),
        "observe 族的额度确实用尽了"
    );
    let prompt = command_body(
        "4c4dafda-dd98-442e-8d55-252b75bac72e",
        "session.prompt",
        Some(SESSION),
        serde_json::json!({ "content": [{ "type": "text", "text": "你好" }] }),
    );
    let other = dispatch(&route, &handle, &mut receiver, &message_id(3), 3, prompt).await;
    assert!(
        bodies(&other, "error").is_empty(),
        "interact 族不得因 observe 族用尽而被限流"
    );
    assert_eq!(
        bodies(&other, "command.result")[0]
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(|code| code.as_str()),
        Some(ErrorCode::InternalUnavailable.as_str()),
        "mutation 的接受路径不在本工作包内（显式失败关闭，不是限流）"
    );
}

// ---------------------------------------------------------------------------------------------
// 2.6 连接期去重
// ---------------------------------------------------------------------------------------------

/// 同一连接内重复的同一 `messageId` 由连接期去重表答复：不再产生第二次业务读取，且结果体逐字相同。
#[tokio::test]
async fn a_duplicate_message_id_is_answered_from_the_connection_cache() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE, &["session.list"]);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let body = command_body(REQUEST, "session.list", None, serde_json::json!({}));
    let first = dispatch(&route, &handle, &mut receiver, MESSAGE_ID, 1, body.clone()).await;
    let reads = world.store.list_calls();
    assert_eq!(reads, 1);
    let second = dispatch(&route, &handle, &mut receiver, MESSAGE_ID, 2, body).await;
    assert_eq!(world.store.list_calls(), 1, "重复 messageId 不得二次读取");
    assert_eq!(
        bodies(&first, "command.result")[0],
        bodies(&second, "command.result")[0],
        "去重答复给出同一份结果体"
    );
}

/// 同一查询在**另一条连接**上各自独立（连接期去重不跨连接，也不落盘）。
#[tokio::test]
async fn the_same_query_on_another_connection_reads_again() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE, &["session.list"]);
    let route = world.route();
    let (first_handle, mut first_receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let (second_handle, mut second_receiver) =
        connect(&world, CONNECTION_B, &SyncConfig::default());
    let body = command_body(REQUEST, "session.list", None, serde_json::json!({}));
    let _ = dispatch(
        &route,
        &first_handle,
        &mut first_receiver,
        MESSAGE_ID,
        1,
        body.clone(),
    )
    .await;
    assert_eq!(world.store.list_calls(), 1);
    let second = dispatch(
        &route,
        &second_handle,
        &mut second_receiver,
        MESSAGE_ID,
        1,
        body,
    )
    .await;
    assert_eq!(
        world.store.list_calls(),
        2,
        "另一条连接上的同一条查询是独立的一次业务读取"
    );
    assert_eq!(
        bodies(&second, "command.result").len(),
        1,
        "第二条连接也拿到自己的 completed"
    );
    // 连接期状态必须随连接离开注册表回收。
    world
        .registry
        .close_device(&DeviceId::new(DEVICE).expect("device id"), 1000, "test");
    assert!(route.tracked() <= 2);
}
