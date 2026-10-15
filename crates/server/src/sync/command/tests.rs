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
use sync_protocol::common::{DecimalString, Uuid as WireUuid};
use sync_protocol::envelope::{ConnectionFields, Envelope, MessageType, encode_body};
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

/// 命令名与分组的唯一机器来源（`crates/identity-auth/tests/authorization.rs` 同款做法：`include_str!`
/// 的路径相对本源文件）。
const COMMAND_CATALOG: &str =
    include_str!("../../../../../compatibility/commands/v1/commands.json");

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
    /// 逐会话**可读窗口的下界**（会话内序号）：低于它的行已被保留策略清理，读面看不到它们。
    /// 默认 0 = 什么都没被清理（§9.7 的前缀清理语义）。
    window: BTreeMap<String, u64>,
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

    /// 把某会话的可读窗口下界设到 `lowest`：低于它的行**在存储里已不存在**（§9.7 的前缀清理），
    /// `read_session` 的三种面（messages/history/正文）都看不到它们。用例据此构造「保留窗口之前的游标」。
    fn prune_below(&self, session: &str, lowest: u64) {
        self.lock().window.insert(session.to_owned(), lowest);
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

    /// 命令面不该有任何写入：「零副作用」判据由用例的 `commits() == 0` 断言承担。
    ///
    /// 这里**计数后返回平凡的成功**，而不是 panic：panic 会让断言永远到不了（检视指出的「不可证伪」），
    /// 误写它的路径现在会继续跑到用例的断言处，断言因此真的能失败。
    async fn commit(&self, _commit: OwnedCommit) -> Result<CommitOutcome, PortError> {
        self.counters.commit.fetch_add(1, Ordering::SeqCst);
        Ok(CommitOutcome {
            session_id: None,
            origin_epoch: None,
            version: acp_core::model::Version::from(0),
            appended: Vec::new(),
            replayed: None,
        })
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
        let window = state.window.get(&session).copied().unwrap_or(0);
        // 保留窗口：低于下界的行已按 §9.7 清理，读面看不到（三种面共用这条过滤）。
        let rows: Vec<OwnedEventRecord> = state
            .events
            .get(&session)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|record| {
                record
                    .event
                    .session_sequence
                    .as_ref()
                    .is_none_or(|sequence| sequence.get() >= window)
            })
            .collect();
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
        // 保留窗口之内、且属于条目集合的可读行（§10.2.1 的 17 类）：既是分页素材，也是「仍可读的下界」
        // 的来源（与 `storage-sqlite` 的 `MIN(session_sequence)` 同一口径）。
        let entries: Vec<&OwnedEventRecord> = rows
            .iter()
            .filter(|record| history_entry(record.event.event_type.as_str()))
            .collect();
        let mut history: Vec<OwnedEventRecord> = entries
            .iter()
            .filter(|record| match query.before_history {
                Some(cursor_value) => record
                    .event
                    .session_sequence
                    .as_ref()
                    .is_some_and(|value| value.get() < cursor_value.get()),
                None => true,
            })
            .map(|record| (*record).clone())
            .collect();
        history.sort_by_key(|record| record.event.session_sequence.as_ref().map(|s| s.get()));
        history.reverse();
        let has_earlier = history.len() as u32 > query.limit.events();
        history.truncate(query.limit.events() as usize);
        history.reverse();
        // 空页 + 游标：游标正好指向仍可读的最早一条（合法，页标记为假）还是落在已按保留策略清理的区间
        // ——后者必须显式报保留期类错误（§11.5 的「保留窗口之前的游标显式不可读」）。与 `storage-sqlite`
        // 的 `read_session` 同一条判定：严格小于，因此「到最早一条」不被误报。
        if query.include.history
            && let Some(before) = query.before_history
            && !has_earlier
            && history.is_empty()
        {
            let low = entries
                .iter()
                .filter_map(|record| record.event.session_sequence.as_ref().map(|s| s.get()))
                .min();
            if low.is_none_or(|low| before.get() < low) {
                return Err(PortError::InvalidRequest(
                    crate::sync::command::query::RETAINED_WINDOW_EXCEEDED,
                ));
            }
        }
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
    connect_device(world, DEVICE, connection_id, config)
}

/// 指定设备标识的连接（限流按 `deviceId` 计数：同一设备的两条连接与两台设备各占各的额度）。
fn connect_device(
    world: &CommandWorld,
    device: &str,
    connection_id: &str,
    config: &SyncConfig,
) -> (Arc<ConnectionHandle>, tokio::sync::mpsc::Receiver<Outbound>) {
    let (handle, receiver) = connection_with_limits(device, connection_id, config);
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
/// 已被保留策略清理的那条用户消息（消息游标用例的游标所指）。
const PRUNED_MESSAGE_ID: &str = "04dbad03-8869-4c5a-94fa-2f3af2fb6230";
/// 仍可读的第二条用户消息。
const SECOND_MESSAGE_ID: &str = "04dbad03-8869-4c5a-94fa-2f3af2fb6231";
const TURN: &str = "6601828b-3eca-4cec-9a58-18ae1e0a3a14";
const REQUEST: &str = "4c4dafda-dd98-442e-8d55-252b75bac72d";
/// 另一个 `requestId`（连接期去重的键含 `requestId`：换它是一条新命令）。
const OTHER_REQUEST: &str = "4c4dafda-dd98-442e-8d55-252b75bac72e";
const CONNECTION_A: &str = "2de54db7-74ae-4c21-88e3-05df0dc2e637";
const CONNECTION_B: &str = "3de54db7-74ae-4c21-88e3-05df0dc2e638";
/// 第三条连接（限流按设备计数：它属于另一台设备）。
const CONNECTION_C: &str = "4de54db7-74ae-4c21-88e3-05df0dc2e639";
/// 另一台设备（限流用例：与 [`DEVICE`] 各占各的额度）。
const OTHER_DEVICE: &str = "2ae1c07c-0000-4000-8000-0000000000a2";
const NOW: &str = "2026-09-18T09:12:03.412Z";
/// 三条升序的持久化时间（消息页的素材：`createdAt` 定宽，字典序即时间序）。
const T1: &str = "2026-09-18T09:12:01.412Z";
const T2: &str = "2026-09-18T09:12:02.412Z";

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

/// 一条用户输入（`session.read` 消息页的载体：正文的权威只在 `messages`；一次 delta 即完整正文）。
fn push_user_message(
    world: &CommandWorld,
    message: &str,
    session_sequence: u64,
    at: &str,
    text: &str,
) {
    world.store.push_event(
        SESSION,
        owned_event(
            SESSION,
            // 事件 id 由会话内序号派生（末段两位十进制 ⇒ 长度仍是 36），用例不必手写一叠 id 字面量。
            &format!("8f1d2c3b-4a59-4e6f-9b7c-1d2e3f4a5b{session_sequence:02x}"),
            "user.message.delta",
            session_sequence,
            session_sequence,
            at,
            &serde_json::json!({
                "messageId": message,
                "turnId": TURN,
                "deltaIndex": "0",
                "text": text,
            })
            .to_string(),
        ),
    );
}

/// 一条 `turn.completed` 历史条目（条目集合里的一个，用于 `beforeHistory` 的三种游标结果）。
fn push_history_entry(world: &CommandWorld, event_id: &str, session_sequence: u64, at: &str) {
    world.store.push_event(
        SESSION,
        owned_event(
            SESSION,
            event_id,
            "turn.completed",
            session_sequence,
            session_sequence,
            at,
            &serde_json::json!({ "turnId": TURN, "state": "completed" }).to_string(),
        ),
    );
}

/// 与出站同一条装配路径（`sync_protocol` 的信封 + `encode_body`）装配一帧，量出信封开销
/// （帧长 − body 字节数）：序号位数不同的帧长度不同，因此这里由调用方给出序号。
fn envelope_overhead(sequence: &str) -> u64 {
    let body = encode_body(&serde_json::json!({})).expect("空 body");
    let envelope = Envelope::new(
        MessageType::CommandResult,
        WireUuid::parse(MESSAGE_ID).expect("message id"),
        Some(ConnectionFields {
            connection_id: WireUuid::parse(CONNECTION_A).expect("connection id"),
            connection_sequence: DecimalString::parse(sequence).expect("序号"),
        }),
        body.clone(),
    )
    .expect("信封");
    (envelope.encode().expect("编码").len() - body.get().len()) as u64
}

/// `index` 号会话摘要（`session.list` 的大结果素材：标题定长，便于把真实帧长卡到确定的字节数上）。
fn fat_session(index: u32) -> SessionSummary {
    SessionSummary::try_new(
        acp_core::model::SessionId::new(&format!("5d73cd10-a465-43cd-b3f1-704e2d49e9{index:02x}"))
            .expect("session id"),
        Some("同步命令面与大结果的边界".to_owned()),
        acp_core::model::AgentRef::try_new(
            acp_core::model::AgentId::new("codex").expect("agent id"),
            "Codex",
        )
        .expect("agent ref"),
        acp_core::model::SessionState::Running,
        acp_core::model::ResourceOrigin::Local,
        None,
        acp_core::model::Version::from(7),
        timestamp("2026-09-17T12:10:00.123Z"),
        timestamp("2026-09-17T12:11:00.123Z"),
        None,
    )
    .expect("会话摘要")
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

/// 命令族与 `compatibility/commands/v1/commands.json` 的 `packs` 分组逐条相等（该登记表是命令名分组的
/// **唯一机器来源**，本文件按同仓其它契约测试的做法直接读它，不再在用例里维护第二份期望值）：
/// 12 条 sync 命令（`CommandName::ALL`）**逐条**都比一遍，`session.cancel`/`elicitation.respond`/
/// `session.config.set` 这类此前没有断言的命令因此也在覆盖内。
#[test]
fn command_families_mirror_the_registry_packs() {
    use sync_protocol::command::CommandName;
    let registry: serde_json::Value =
        serde_json::from_str(COMMAND_CATALOG).expect("命令登记表是合法 JSON");
    // pack → 成员命令名（登记表的 packs 分组）。
    let mut pack_of: BTreeMap<String, String> = BTreeMap::new();
    for (pack, names) in registry["packs"].as_object().expect("packs 对象") {
        for name in names.as_array().expect("pack 成员数组") {
            let name = name.as_str().expect("命令名是字符串");
            assert!(
                pack_of.insert(name.to_owned(), pack.clone()).is_none(),
                "{name} 不得出现在两个 pack 里"
            );
        }
    }
    // 逐条：命令名的族必须就是它登记的 pack（`CommandFamily::as_str` 是 pack 去掉 `pack.` 前缀）。
    for command in CommandName::ALL {
        let name = command.as_str();
        let pack = pack_of
            .get(name)
            .unwrap_or_else(|| panic!("{name} 不在登记表的任何 pack 里"));
        assert_eq!(
            &format!("pack.{}", CommandFamily::of(command).as_str()),
            pack,
            "{name} 的族必须等于登记表的 pack"
        );
    }
    // 反向与计数：登记表里带 pack 的命令恰好是这 12 条 sync 命令（`session.resume` 的 pack 为 null，
    // 它不在 `CommandName::ALL` 里），`transport` 含 sync 的集合也与它逐条相等。
    assert_eq!(pack_of.len(), CommandName::ALL.len());
    let sync_commands = registry["commands"]
        .as_array()
        .expect("commands 数组")
        .iter()
        .filter(|entry| {
            entry["transport"]
                .as_array()
                .is_some_and(|transports| transports.iter().any(|it| it.as_str() == Some("sync")))
        })
        .map(|entry| entry["name"].as_str().expect("命令名"))
        .collect::<Vec<_>>();
    assert_eq!(sync_commands.len(), CommandName::ALL.len());
    for name in &sync_commands {
        assert!(pack_of.contains_key(*name), "{name} 必须有 pack");
    }
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

/// `session.read` 的 `beforeHistory` 早于保留窗口：core 的保留期类错误必须映射成 §11.5 要求的
/// `sync.cursor_invalid`（`details.reason = "cursor_expired"`），不得落进通配臂变成
/// `protocol.schema_invalid`；而游标**恰好**指向仍可读的最早一条时，同样的空页是合法的。
#[tokio::test]
async fn session_read_rejects_a_history_cursor_before_the_retained_window() {
    const EVENT_ONE: &str = "8f1d2c3b-4a59-4e6f-9b7c-1d2e3f4a5b6c";
    const EVENT_TWO: &str = "9f1d2c3b-4a59-4e6f-9b7c-1d2e3f4a5b6d";
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    push_history_entry(&world, EVENT_ONE, 5, T1);
    push_history_entry(&world, EVENT_TWO, 6, T2);
    // 序号 1..=4 的行已按保留策略清理：可读窗口从序号 5 开始。
    world.store.prune_below(SESSION, 5);
    world.seed_device(DEVICE, &["session.read"]);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    crate::sync::conn::MessageRoute::on_authenticated(
        route.as_ref(),
        &handle,
        &["core.session-history.v1".to_owned()],
    );
    let read = |before: &str| {
        command_body(
            REQUEST,
            "session.read",
            Some(SESSION),
            serde_json::json!({ "include": ["history"], "beforeHistory": before }),
        )
    };
    // 游标 3 落在已清理的区间：显式拒绝，且原因必须是 cursor_expired。
    let out = dispatch(&route, &handle, &mut receiver, MESSAGE_ID, 1, read("3")).await;
    let results = bodies(&out, "command.result");
    assert_eq!(
        results[0].get("status").and_then(|value| value.as_str()),
        Some("rejected")
    );
    assert_eq!(
        results[0]["error"]["code"].as_str(),
        Some(ErrorCode::SyncCursorInvalid.as_str()),
        "保留窗口之前的游标不得被报成 protocol.schema_invalid"
    );
    assert_eq!(
        results[0]["error"]["details"]["reason"].as_str(),
        Some("cursor_expired"),
        "reason 是该码的必需字段"
    );
    assert!(results[0]["result"].is_null(), "拒绝时 result 必须是 null");
    // 游标 5 恰好是仍可读的最早一条：合法空页（严格比较，不得误报过期）。
    let out = dispatch(&route, &handle, &mut receiver, &message_id(2), 2, read("5")).await;
    let result = assert_single_completed(&out, "session.read");
    assert_eq!(
        result["resources"]["history"]
            .as_array()
            .expect("history 数组")
            .len(),
        0
    );
    assert_eq!(
        result
            .get("historyHasEarlier")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    // 游标 6（窗口之内）：给出序号 5 那一条。
    let out = dispatch(&route, &handle, &mut receiver, &message_id(3), 3, read("6")).await;
    let result = assert_single_completed(&out, "session.read");
    let history = result["resources"]["history"]
        .as_array()
        .expect("history 数组");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0]["sessionSequence"].as_str(), Some("5"));
}

/// 消息页的 `before` 指向**已被保留策略清掉**的消息：§11.5 明令不得静默返回一页空 `messages` +
/// `hasEarlier: false`（那会让客户端误以为已到最早一条），必须回 `sync.cursor_invalid` +
/// `details.reason = "cursor_expired"`；游标恰好指向仍可读的最早一条时，空页是**合法**的。
#[tokio::test]
async fn session_read_rejects_a_message_cursor_before_the_retained_window() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    push_user_message(&world, PRUNED_MESSAGE_ID, 1, T1, "已被清理的一条");
    push_user_message(&world, MESSAGE_ID, 2, T2, "仍可读的一条");
    push_user_message(&world, SECOND_MESSAGE_ID, 3, NOW, "最新的一条");
    // 序号 1 的那条（`PRUNED_MESSAGE_ID`）已被清理。
    world.store.prune_below(SESSION, 2);
    world.seed_device(DEVICE, &["session.read"]);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let read = |before: serde_json::Value| {
        command_body(
            REQUEST,
            "session.read",
            Some(SESSION),
            serde_json::json!({ "include": ["messages"], "before": before }),
        )
    };
    // 已清理那条消息的 `(createdAt, messageId)`：过期游标必须显式拒绝。
    let out = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(1),
        1,
        read(serde_json::json!({ "createdAt": T1, "messageId": PRUNED_MESSAGE_ID })),
    )
    .await;
    let results = bodies(&out, "command.result");
    assert_eq!(
        results[0].get("status").and_then(|value| value.as_str()),
        Some("rejected"),
        "过期的消息游标不得静默返回空页"
    );
    assert_eq!(
        results[0]["error"]["code"].as_str(),
        Some(ErrorCode::SyncCursorInvalid.as_str())
    );
    assert_eq!(
        results[0]["error"]["details"]["reason"].as_str(),
        Some("cursor_expired")
    );
    // 仍可读的最早一条本身：合法空页（客户端翻到最早一条后再问一页），不得误报过期。
    let out = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(2),
        2,
        read(serde_json::json!({ "createdAt": T2, "messageId": MESSAGE_ID })),
    )
    .await;
    let result = assert_single_completed(&out, "session.read");
    assert_eq!(
        result["resources"]["messages"]
            .as_array()
            .expect("messages 数组")
            .len(),
        0
    );
    assert_eq!(
        result.get("hasEarlier").and_then(|value| value.as_bool()),
        Some(false)
    );
    // 不带游标：最新一页给出两条仍可读的消息，被清理的那条不出现。
    let out = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(3),
        3,
        command_body(
            REQUEST,
            "session.read",
            Some(SESSION),
            serde_json::json!({ "include": ["messages"] }),
        ),
    )
    .await;
    let result = assert_single_completed(&out, "session.read");
    let messages = result["resources"]["messages"]
        .as_array()
        .expect("messages 数组");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["messageId"].as_str(), Some(MESSAGE_ID));
    assert_eq!(messages[1]["messageId"].as_str(), Some(SECOND_MESSAGE_ID));
    assert_eq!(
        result.get("hasEarlier").and_then(|value| value.as_bool()),
        Some(false)
    );
}

/// `limit` 超过服务端上限时收敛到上限并照常返回（§11.5）：`2^32` 这样的取值必须**饱和**转换到上限，
/// 不得回绕成 0（`as u32` 会那样）再被 clamp 抬成 1，把一页 200 条变成一页 1 条。
#[tokio::test]
async fn session_read_converges_an_oversized_limit_to_the_server_cap() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    push_user_message(&world, PRUNED_MESSAGE_ID, 1, T1, "第一条");
    push_user_message(&world, MESSAGE_ID, 2, T2, "第二条");
    push_user_message(&world, SECOND_MESSAGE_ID, 3, NOW, "第三条");
    world.seed_device(DEVICE, &["session.read"]);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let read = |limit: u64| {
        command_body(
            REQUEST,
            "session.read",
            Some(SESSION),
            serde_json::json!({ "include": ["messages"], "limit": limit }),
        )
    };
    // 2^32：`as u32` 会回绕成 0 ⇒ clamp 成 1（只给最新一条）。收敛到上限时三条都在一页里。
    let out = dispatch(
        &route,
        &handle,
        &mut receiver,
        MESSAGE_ID,
        1,
        read(4_294_967_296),
    )
    .await;
    let result = assert_single_completed(&out, "session.read");
    assert_eq!(
        result["resources"]["messages"]
            .as_array()
            .expect("messages 数组")
            .len(),
        3,
        "超过上限的 limit 必须收敛到服务端上限（200），而不是回绕成 1"
    );
    // 对照：`limit: 1` 是有效的一页一条（说明上面那一页不是因为 limit 被忽略）。
    let out = dispatch(&route, &handle, &mut receiver, &message_id(2), 2, read(1)).await;
    let result = assert_single_completed(&out, "session.read");
    let messages = result["resources"]["messages"]
        .as_array()
        .expect("messages 数组");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["messageId"].as_str(), Some(SECOND_MESSAGE_ID));
    assert_eq!(
        result.get("hasEarlier").and_then(|value| value.as_bool()),
        Some(true),
        "limit 1 时更早还有消息"
    );
}

/// 消息页的游标翻页：`before` 给出**紧接其之前**的一页（不与上一页重叠、也不跳条），走到最早一条时
/// `hasEarlier: false` 且不返回空占位（§11.5）。每一页都用上一页最早一条的 `(createdAt, messageId)`。
#[tokio::test]
async fn session_read_pages_backwards_without_gaps_or_overlap() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    push_user_message(&world, PRUNED_MESSAGE_ID, 1, T1, "第一条");
    push_user_message(&world, MESSAGE_ID, 2, T2, "第二条");
    push_user_message(&world, SECOND_MESSAGE_ID, 3, NOW, "第三条");
    world.seed_device(DEVICE, &["session.read"]);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let read = |before: Option<serde_json::Value>| {
        let mut payload = serde_json::json!({ "include": ["messages"], "limit": 1 });
        if let Some(before) = before {
            payload["before"] = before;
        }
        command_body(REQUEST, "session.read", Some(SESSION), payload)
    };
    let steps = [
        (None, vec![SECOND_MESSAGE_ID], true),
        (Some((NOW, SECOND_MESSAGE_ID)), vec![MESSAGE_ID], true),
        (Some((T2, MESSAGE_ID)), vec![PRUNED_MESSAGE_ID], false),
        (Some((T1, PRUNED_MESSAGE_ID)), vec![], false),
    ];
    for (index, (cursor, expected, has_earlier)) in steps.into_iter().enumerate() {
        let before = cursor.map(|(at, id)| serde_json::json!({ "createdAt": at, "messageId": id }));
        let out = dispatch(
            &route,
            &handle,
            &mut receiver,
            &message_id(index as u64 + 1),
            index as u64 + 1,
            read(before),
        )
        .await;
        let result = assert_single_completed(&out, "session.read");
        let messages = result["resources"]["messages"]
            .as_array()
            .expect("messages 数组");
        let ids = messages
            .iter()
            .map(|message| message["messageId"].as_str().expect("messageId"))
            .collect::<Vec<_>>();
        assert_eq!(ids, expected, "第 {} 页的成员", index + 1);
        assert_eq!(
            result.get("hasEarlier").and_then(|value| value.as_bool()),
            Some(has_earlier),
            "第 {} 页的页标记",
            index + 1
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

/// 信封开销常量是**测出来的**最坏上界：与出站同一条装配路径（`sync_protocol` 的信封 + `encode_body`）
/// 装配一帧，帧长减去 body 的字节数就是开销。序号取 u64 的最长十进制（20 位）、两个标识都是 36 字符。
#[test]
fn the_envelope_overhead_is_the_measured_worst_case() {
    assert_eq!(
        envelope_overhead(&u64::MAX.to_string()),
        super::ENVELOPE_OVERHEAD_BYTES,
        "常量必须等于实测的最坏开销：放大它就会把装得下的结果判成超限"
    );
    assert!(
        envelope_overhead("1") < super::ENVELOPE_OVERHEAD_BYTES,
        "短序号的帧更短：常量确实是上界，而不是随手取大的余量"
    );
}

/// 「装得下」的结果不得被判超限：上限取「body + 最坏信封开销」，因此这一帧**恰好**落在判定的边界上，
/// 必须照常 `completed`，且真实帧长不超过上限。信封开销留余量（旧常量 320）会在这里把
/// `[max - 319, max - 202]` 区间里装得下的结果打成 `resource.result_too_large`。
#[tokio::test]
async fn an_almost_full_query_result_still_fits() {
    let world = CommandWorld::new();
    for index in 0..16 {
        world.push_session(fat_session(index));
    }
    world.seed_device(DEVICE, &["session.list"]);
    let route = world.route();
    let body = command_body(REQUEST, "session.list", None, serde_json::json!({}));
    // ① 先用宽松上限量出这份结果真实出站帧的长度。
    let (measure_handle, mut measure_receiver) =
        connect(&world, CONNECTION_A, &SyncConfig::default());
    let measured = dispatch(
        &route,
        &measure_handle,
        &mut measure_receiver,
        MESSAGE_ID,
        1,
        body.clone(),
    )
    .await;
    assert_single_completed(&measured, "session.list");
    // ② 从帧文本里量出 body 的字节数（信封 = 固定头 + body + `}`），把上限卡到「body + 实测的最坏
    //    信封开销」上——这个上限**不**读被测常量，否则常量变了测试会跟着变，判不出真假。
    const MARKER: &str = "\"body\":";
    let measured_frame = &measured[0];
    let start = measured_frame.text.find(MARKER).expect("信封里有 body 键") + MARKER.len();
    assert!(measured_frame.text.ends_with('}'));
    let body_bytes = (measured_frame.text.len() - start - 1) as u64;
    let max = body_bytes + envelope_overhead(&u64::MAX.to_string());
    assert!(
        max > 1_024,
        "上限的 wire 下限是 1024，量出来的结果必须比它大才谈得上「卡在上限上」"
    );
    let config = SyncConfig {
        max_message_bytes: max,
        ..SyncConfig::default()
    };
    let (handle, mut receiver) = connect(&world, CONNECTION_B, &config);
    let out = dispatch(&route, &handle, &mut receiver, &message_id(2), 1, body).await;
    let result = assert_single_completed(&out, "session.list");
    assert!(
        result.get("sessions").is_some_and(|value| value.is_array()),
        "装得下的结果必须原样返回，不得截断"
    );
    let frame = out
        .iter()
        .find(|frame| frame.kind == "command.result")
        .expect("结果帧");
    assert_eq!(
        frame.text.len() as u64,
        measured_frame.text.len() as u64,
        "同一份 body、同为新连接的第一次发送 ⇒ 逐字节相同的帧"
    );
    assert!(
        frame.text.len() as u64 <= max,
        "真实帧长 {} 不得超过本连接的单条消息上限 {max}",
        frame.text.len()
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

/// 限流键是 `(deviceId, 命令族)` 而**不是**连接：同一设备的第二条连接不重置额度（§14「同一设备重连不
/// 重置额度」），另一台设备则各占各的额度。把 `admit_rate` 的 `handle.device_id()` 换成
/// `connection_id()` 会让这条失败。
#[tokio::test]
async fn the_command_quota_is_counted_per_device_not_per_connection() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE, &["session.list"]);
    world.seed_device(OTHER_DEVICE, &["session.list"]);
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
    let (first, mut first_receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let (second, mut second_receiver) = connect(&world, CONNECTION_B, &SyncConfig::default());
    let (other, mut other_receiver) =
        connect_device(&world, OTHER_DEVICE, CONNECTION_C, &SyncConfig::default());
    let body = command_body(REQUEST, "session.list", None, serde_json::json!({}));
    // 第一条连接用掉该设备这一分钟的唯一额度。
    let first_out = dispatch(
        &route,
        &first,
        &mut first_receiver,
        &message_id(1),
        1,
        body.clone(),
    )
    .await;
    assert_single_completed(&first_out, "session.list");
    // 同一设备的第二条连接：额度已被该设备用掉，必须被限流（而不是「新连接 = 新额度」）。
    let second_out = dispatch(
        &route,
        &second,
        &mut second_receiver,
        &message_id(2),
        1,
        body.clone(),
    )
    .await;
    assert!(
        bodies(&second_out, "command.result").is_empty(),
        "同一设备的第二条连接不得拿到自己的额度"
    );
    let errors = bodies(&second_out, "error");
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors[0].get("code").and_then(|code| code.as_str()),
        Some(ErrorCode::ResourceRateLimited.as_str())
    );
    // 另一台设备：各占各的额度，照常执行。
    let other_out = dispatch(&route, &other, &mut other_receiver, &message_id(3), 1, body).await;
    assert_single_completed(&other_out, "session.list");
    assert_eq!(
        world.store.list_calls(),
        2,
        "两台设备各产生一次业务读取：被限流的第二条连接零副作用"
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

/// 连接期去重的键是 `(messageId, requestId)` 这一**对**：换掉任一半都是一条新帧，不得命中缓存
/// （§11.1「`messageId` 重复不自动重放业务结果；命令幂等只看 `requestId`」）。键退化成
/// `messageId` 或 `requestId` 单字段都会让这条失败。
#[tokio::test]
async fn the_connection_dedup_key_is_the_frame_pair() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE, &["session.list"]);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let frame = command_body(REQUEST, "session.list", None, serde_json::json!({}));
    let _ = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(1),
        1,
        frame.clone(),
    )
    .await;
    assert_eq!(world.store.list_calls(), 1);
    // 同一 `messageId`、换 `requestId`：新命令，必须再见一次业务读取。
    let _ = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(1),
        2,
        command_body(OTHER_REQUEST, "session.list", None, serde_json::json!({})),
    )
    .await;
    assert_eq!(
        world.store.list_calls(),
        2,
        "去重键含 requestId：同 messageId 的另一条命令不得命中缓存"
    );
    // 同一 `requestId`、换 `messageId`：也是一条新帧（响应丢失后的重试由客户端复用同一个
    // `messageId` 表达，见 §11.1）。
    let _ = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(2),
        3,
        frame.clone(),
    )
    .await;
    assert_eq!(
        world.store.list_calls(),
        3,
        "去重键含 messageId：同 requestId 的另一帧不得命中缓存"
    );
    // 原帧再来一次：这一对才是缓存键。
    let _ = dispatch(&route, &handle, &mut receiver, &message_id(1), 4, frame).await;
    assert_eq!(
        world.store.list_calls(),
        3,
        "同一对 (messageId, requestId) 才命中连接期缓存"
    );
}

/// 同一查询在**另一条连接**上各自独立（连接期去重不跨连接，也不落盘），且连接期状态随连接离开注册表
/// 回收。
///
/// 两条连接分属两台设备：注册表的键是 `(hostId, deviceId)`（§8.5 的单设备替换），同一设备的两条连接
/// 不会同时在注册表里，因此「两条连接同时活着」的场景只能用两台设备构造。
#[tokio::test]
async fn the_same_query_on_another_connection_reads_again() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE, &["session.list"]);
    world.seed_device(OTHER_DEVICE, &["session.list"]);
    let route = world.route();
    let (first_handle, mut first_receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let (second_handle, mut second_receiver) =
        connect_device(&world, OTHER_DEVICE, CONNECTION_B, &SyncConfig::default());
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
    assert_eq!(route.tracked(), 2, "两条连接各有一份连接期状态");
    // 连接期状态必须随连接**离开注册表**回收（`conn::session` 在连接结束时 `unregister`）：下一次命令
    // 处理只留下正在派发的那一条。断言必须是精确值——`<= 2` 之类恒真的写法连「reclaim 变成空操作」
    // 都发现不了。
    world.registry.unregister(&first_handle);
    world.registry.unregister(&second_handle);
    assert_eq!(world.registry.active(), 0);
    let _ = dispatch(
        &route,
        &first_handle,
        &mut first_receiver,
        &message_id(7),
        7,
        command_body(REQUEST, "session.list", None, serde_json::json!({})),
    )
    .await;
    assert_eq!(
        route.tracked(),
        1,
        "离开注册表的连接的限流窗口与去重表必须被回收，只留下正在派发的这一条"
    );
}
