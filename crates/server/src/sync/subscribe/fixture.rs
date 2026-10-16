//! 订阅面与快照面用例的端口替身世界（仅测试代码）。
//!
//! `#![cfg(test)]` 是**自我声明**：本文件只在 `cargo test` 下编译。
//!
//! 三个替身与它们的口径：
//!
//! - [`SyncStore`]：会话列表、水位与**一致性读视图**。`ReadView::replay` 的语义逐条照抄
//!   `storage-sqlite` 的 `resolve_cursor`（`session_store.rs:2352`）与窗口读（`:2410`）——先按
//!   `server_epoch` 判 epoch、再判越界（返回 `InvalidRequest("cursor is beyond head")`）、再判保留窗口
//!   下界，命中时把 `reset_required` 一并给出，并且**从窗口下界开始读**（真实存储的同款回填）。用例因此
//!   能确定性地覆盖四种 cursor 结果，而不必起真实 SQLite（真实存储的同口径由 `storage-sqlite` 自己的
//!   用例与 WP8 的端到端用例承担）。
//! - [`SyncDeliveries`]：imported 会话索引（`list_sessions`）。
//! - `crate::local_admin::test_support` 的既有 fake（时钟/审计/配置/信任/id）：只补本工作包真正触及的
//!   方法，其余方法保持它们的 `unreachable!` 口径，避免替身比真实实现更宽容。
//!
//! 观测口：读视图记录**每一次** `replay` 的 `after`（[`World::replay_calls`]）——"恢复位置取客户端提交
//! 的 cursor、而不是服务端保存的最大 ACK"因此是可断言的事实，而不是口头承诺。
#![cfg(test)]

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};

use acp_core::broker::{Broker, BrokerConfig, BrokerDeps};
use acp_core::model::{
    AgentProfile, CommittedEvent, DeviceRecord, DeviceState, EventId,
    EventOrigin as CoreEventOrigin, EventPayload, EventType, GlobalCursor, LocalCursor, PortError,
    RemoteSessionRef, Sequence, ServerEpoch, SessionId, SessionRecoveryRecord, SessionSummary,
    Timestamp, Version,
};
use acp_core::ports::{
    CommitOutcome, DeliveryIndexEntry, DropReport, ImportedSessionQuery, ImportedSessionRecord,
    OwnedCommit, ReadView, RemoteCommandRef, RemoteDeliveryStore, ReplayBatch, ReplayLimit,
    ResetReason, SessionQuery, SessionStore, StoreHealth, SyncEventRecord,
};
use acp_core::use_cases::{UseCaseDeps, UseCases};
use sync_protocol::common::Uuid;
use sync_protocol::envelope::Envelope;

use crate::local_admin::test_support::{
    FakeAudit, FakeCatalog, FakeClock, FakeConfig, FakeExports, FakeIds, FakeTrust, NotTouched,
};
use crate::sync::conn::registry::{ConnectionHandle, ConnectionRegistry, Outbound};
use crate::sync::conn::{SessionLimits, SyncConfig};
use crate::sync::subscribe::{OwnerReachability, SubscribeRoute};

/// 未触及方法的统一标记（与 `local_admin::test_support` 同口径）。
const NOT_TOUCHED: &str = "订阅面用例未触及";

/// 本机 host id（与 `TestWorld` 的 `Authority` 同一素材）。
pub(crate) const HOST: &str = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
/// 事件库 epoch（`server_epoch` 的唯一来源）。
pub(crate) const EPOCH: &str = "018f6f89-8a23-7a10-a0d3-f92e6a31d952";
/// 另一个事件库 epoch（跨 epoch 用例）。
pub(crate) const OTHER_EPOCH: &str = "028f6f89-8a23-7a10-a0d3-f92e6a31d952";
/// 测试扮演的 Sync 设备。
pub(crate) const DEVICE: &str = "2ae1c07c-0000-4000-8000-0000000000a1";
/// 本次连接的 `connectionId`。
pub(crate) const CONNECTION: &str = "2de54db7-74ae-4c21-88e3-05df0dc2e637";
/// 一条自有会话。
pub(crate) const SESSION: &str = "5d73cd10-a465-43cd-b3f1-704e2d49e99e";
/// imported 会话的 Owner 节点。
pub(crate) const OWNER: &str = "1f0d5c4b-3a29-4877-9615-0e1d2c3b4a59";
/// imported 会话的 Export。
pub(crate) const EXPORT: &str = "export-laptop-zed";
/// imported 会话。
pub(crate) const REMOTE_SESSION: &str = "6e84de21-b576-44de-a4c2-815e3e5faa77";
/// imported 会话的 origin epoch。
pub(crate) const ORIGIN_EPOCH: &str = "2a1b0c9d-8e7f-4a6b-b5c4-3d2e1f0a9b87";

pub(crate) fn timestamp(text: &str) -> Timestamp {
    Timestamp::new(text).expect("测试时间戳合法")
}

pub(crate) fn epoch(text: &str) -> ServerEpoch {
    ServerEpoch::new(text).expect("测试 epoch 合法")
}

pub(crate) fn sequence(value: u64) -> Sequence {
    Sequence::new(value).expect("测试序号合法")
}

pub(crate) fn cursor(text_epoch: &str, value: u64) -> GlobalCursor {
    GlobalCursor::new(epoch(text_epoch), sequence(value))
}

pub(crate) fn core_session(
    id: &str,
    title: &str,
    workspace: Option<(&str, &str)>,
) -> SessionSummary {
    SessionSummary::try_new(
        SessionId::new(id).expect("session id"),
        Some(title.to_owned()),
        acp_core::model::AgentRef::try_new(
            acp_core::model::AgentId::new("codex").expect("agent id"),
            "Codex",
        )
        .expect("agent ref"),
        acp_core::model::SessionState::Running,
        acp_core::model::ResourceOrigin::Local,
        None,
        Version::from(3),
        timestamp("2026-09-17T12:10:00.123Z"),
        timestamp("2026-09-17T12:11:00.123Z"),
        workspace.map(|(alias, display_name)| {
            acp_core::model::WorkspaceRef::try_new(
                acp_core::model::WorkspaceAlias::new(alias).expect("alias"),
                display_name,
            )
            .expect("workspace ref")
        }),
    )
    .expect("session summary")
}

/// 一条已提交事件（`ReadView::replay` 的行；会话级定位四项成对）。
pub(crate) fn committed_event(
    id: &str,
    event_type: &str,
    global: u64,
    session_sequence: u64,
) -> CommittedEvent {
    CommittedEvent {
        id: EventId::new(id).expect("event id"),
        event_type: EventType::new(event_type).expect("event type"),
        session: Some(SessionId::new(SESSION).expect("session id")),
        session_sequence: Some(sequence(session_sequence)),
        global_sequence: sequence(global),
        origin_epoch: Some(acp_core::model::OriginEpoch::new(ORIGIN_EPOCH).expect("origin epoch")),
        origin_sequence: Some(sequence(global)),
        created_at: timestamp("2026-09-17T12:10:00.123Z"),
    }
}

/// 按序号给一条合法 UUID 的 Sync 事件读面素材（`agent` 来源、无 causation）。
///
/// 事件 id 由序号派生（末段两位十进制，`…-a7744dfedbANN`，36 字符整），因此用例无需手写一叠 id
/// 字面量；序号上限 99 由末段宽度决定。
pub(crate) fn agent_event(index: u64) -> SyncEventRecord {
    assert!(index < 100, "测试素材序号上限 99（UUID 末段宽度）");
    sync_event(
        &format!("da702ba1-f49f-4e16-a677-a7744dfedb{index:02}"),
        index,
        CoreEventOrigin::Agent,
        None,
        None,
        None,
    )
}

/// 一条 Sync 事件读面的素材：定位 + 正文 + `origin` 三个来源字段（`event` 帧的全部输入）。
///
/// 默认 `origin = agent`、无 causation、正文是一个单键 `view`（便于断言"发的是它自己那条"）。
pub(crate) fn sync_event(
    id: &str,
    global: u64,
    origin: CoreEventOrigin,
    device_id: Option<&str>,
    node_id: Option<&str>,
    causation: Option<&str>,
) -> SyncEventRecord {
    SyncEventRecord {
        event: committed_event(id, "turn.started", global, global),
        payload: EventPayload::new(
            acp_core::model::ViewJson::new(&format!(r#"{{"global":{global}}}"#))
                .expect("测试 view 合法"),
            None,
        ),
        origin_kind: origin,
        origin_device_id: device_id
            .map(|value| acp_core::model::DeviceId::new(value).expect("测试 device id")),
        origin_node_id: node_id
            .map(|value| acp_core::model::NodeId::new(value).expect("测试 node id")),
        causation: causation
            .map(|value| acp_core::model::RequestId::new(value).expect("测试 request id")),
    }
}

// ---------------------------------------------------------------------------------------------
// 存储替身
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct StoreState {
    head: Option<GlobalCursor>,
    sessions: Vec<SessionSummary>,
    events: Vec<CommittedEvent>,
    payloads: BTreeMap<String, EventPayload>,
    /// 保留窗口下界（`MIN(global_sequence)`）：低于它的游标判 `cursor_expired`。
    window_low: Option<u64>,
    /// 每次 `replay` 的 `after` 实参（观测口）。
    replay_calls: Vec<Option<GlobalCursor>>,
    /// Sync 事件读面的条目素材（`push_sync_event`）：与 `events` 分开存，因为 `sync_replay` 还要带
    /// `origin_kind`/`origin_device_id`/`origin_node_id`/`causation` 四个 `owned_event` 列。
    sync_events: Vec<SyncEventRecord>,
    /// 每条 `sync_replay` 的 `after` 实参（观测口：逐页续读位置可断言）。
    sync_replay_calls: Vec<GlobalCursor>,
    /// `sync_replay` 的整批失败注入（历史行无法归因的 `Corrupt`）。
    sync_replay_failure: Option<PortError>,
}

/// 「会话列表 + 水位 + 一致性读视图」的存储替身。
#[derive(Clone, Default)]
pub(crate) struct SyncStore {
    state: Arc<Mutex<StoreState>>,
}

impl SyncStore {
    pub(crate) fn set_head(&self, cursor: GlobalCursor) {
        self.lock().head = Some(cursor);
    }

    pub(crate) fn push_session(&self, summary: SessionSummary) {
        self.lock().sessions.push(summary);
    }

    /// 预置一条 Sync 事件读面的素材（`event` 帧的全部来源字段）。
    pub(crate) fn push_sync_event(&self, record: SyncEventRecord) {
        self.lock().sync_events.push(record);
    }

    /// 让 `sync_replay` 整批失败关闭（历史行无法归因的注入点）。
    pub(crate) fn fail_sync_replay(&self, error: PortError) {
        self.lock().sync_replay_failure = Some(error);
    }

    /// 每次 `ReadView::sync_replay` 的 `after` 实参（顺序即调用顺序；`None` 未被本工作包使用）。
    pub(crate) fn sync_replay_calls(&self) -> Vec<GlobalCursor> {
        self.lock().sync_replay_calls.clone()
    }

    /// 设保留窗口下界（`MIN(global_sequence)`）。
    pub(crate) fn set_window_low(&self, low: u64) {
        self.lock().window_low = Some(low);
    }

    /// 每一次 `ReadView::replay` 的 `after` 实参（顺序即调用顺序）。
    pub(crate) fn replay_calls(&self) -> Vec<Option<GlobalCursor>> {
        self.lock().replay_calls.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, StoreState> {
        self.state.lock().expect("存储替身锁")
    }
}

#[async_trait::async_trait]
impl SessionStore for SyncStore {
    async fn list(&self, _query: SessionQuery) -> Result<Vec<SessionSummary>, PortError> {
        Ok(self.lock().sessions.clone())
    }

    async fn head(&self) -> Result<GlobalCursor, PortError> {
        Ok(self.lock().head.clone().unwrap_or_else(|| cursor(EPOCH, 0)))
    }

    async fn read_view(&self) -> Result<Box<dyn ReadView>, PortError> {
        Ok(Box::new(SyncReadView {
            state: Arc::clone(&self.state),
        }))
    }

    async fn commit(&self, _commit: OwnedCommit) -> Result<CommitOutcome, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn load(
        &self,
        _session: &SessionId,
    ) -> Result<Option<acp_core::model::SessionSnapshot>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn find_request(
        &self,
        _request: &acp_core::model::RequestId,
        _actor: &acp_core::model::Actor,
    ) -> Result<Option<acp_core::model::CommandRecord>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn unsettled_commands(
        &self,
        _limit: ReplayLimit,
    ) -> Result<Vec<acp_core::model::CommandRecord>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn load_recovery(
        &self,
        _session: &SessionId,
    ) -> Result<Option<SessionRecoveryRecord>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn retention_window(
        &self,
        _session: &SessionId,
    ) -> Result<Option<(Sequence, Sequence)>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn prune(
        &self,
        _policy: acp_core::ports::RetentionPolicy,
        _at: Timestamp,
    ) -> Result<acp_core::ports::PruneReport, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn health(&self) -> Result<StoreHealth, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }
}

/// 只读一致性视图：语义照抄 `storage-sqlite` 的 `SqlReadView`。
struct SyncReadView {
    state: Arc<Mutex<StoreState>>,
}

#[async_trait::async_trait]
impl ReadView for SyncReadView {
    async fn head(&self) -> Result<GlobalCursor, PortError> {
        let state = self.state.lock().expect("存储替身锁");
        Ok(state.head.clone().unwrap_or_else(|| cursor(EPOCH, 0)))
    }

    /// Sync 事件读面（`SYNC_PROTOCOL.md` §10.1）：`after` 之后的一页事件 + 这批数据的 head。
    ///
    /// 语义照抄 `storage-sqlite` 的 `SqlReadView::sync_replay`：`global_sequence > after` 严格升序、
    /// `LIMIT limit`、head 与本批同一次读；`sync_replay_failure` 是历史行无法归因的注入点（真实存储
    /// 在那里返回 `Corrupt`，见 `session_store.rs` 的同名方法）。
    async fn sync_replay(
        &self,
        after: Option<GlobalCursor>,
        limit: ReplayLimit,
    ) -> Result<acp_core::ports::SyncReplaySlice, PortError> {
        let mut state = self.state.lock().expect("存储替身锁");
        let head = state.head.clone().unwrap_or_else(|| cursor(EPOCH, 0));
        if let Some(after) = after.as_ref() {
            state.sync_replay_calls.push(after.clone());
        }
        if let Some(error) = state.sync_replay_failure.take() {
            return Err(error);
        }
        let from = after
            .as_ref()
            .map_or(0, |cursor| cursor.global_sequence.get());
        let mut events: Vec<SyncEventRecord> = state
            .sync_events
            .iter()
            .filter(|record| record.event.global_sequence.get() > from)
            .take(limit.events() as usize)
            .cloned()
            .collect();
        events.sort_by_key(|record| record.event.global_sequence);
        Ok(acp_core::ports::SyncReplaySlice { events, head })
    }

    async fn replay(
        &self,
        after: Option<GlobalCursor>,
        limit: ReplayLimit,
    ) -> Result<ReplayBatch, PortError> {
        let mut state = self.state.lock().expect("存储替身锁");
        let head = state.head.clone().unwrap_or_else(|| cursor(EPOCH, 0));
        state.replay_calls.push(after.clone());
        // `resolve_cursor`（storage-sqlite:2352）：epoch → 越界（错误）→ 保留窗口下界。
        let reset_required = match after.as_ref() {
            None => Some(ResetReason::InitialSync),
            Some(requested) if requested.server_epoch != head.server_epoch => {
                Some(ResetReason::EpochMismatch)
            }
            Some(requested) if requested.global_sequence > head.global_sequence => {
                return Err(PortError::InvalidRequest("cursor is beyond head"));
            }
            Some(requested) => match state.window_low {
                Some(low) if requested.global_sequence.get() + 1 < low => {
                    Some(ResetReason::CursorExpired)
                }
                _ => None,
            },
        };
        // 需要重建快照时从窗口下界开始读（真实存储的同款回填）——调用方必须**忽略**这批事件，
        // 那正是"把越界 cursor 钳制到 head"的等价物，协议禁止。
        let from = match reset_required {
            Some(_) => 0,
            None => after.as_ref().map_or(0, |c| c.global_sequence.get()),
        };
        let rows: Vec<CommittedEvent> = state
            .events
            .iter()
            .filter(|event| event.global_sequence.get() > from)
            .cloned()
            .collect();
        let mut events = Vec::new();
        let mut next = None;
        for (index, row) in rows.iter().enumerate() {
            if index as u32 >= limit.events() {
                break;
            }
            events.push(acp_core::model::CommittedDelivery::Owned(row.clone()));
        }
        if rows.len() as u32 > limit.events() {
            if let Some(last) = rows.get(limit.events() as usize - 1) {
                next = Some(GlobalCursor::new(
                    head.server_epoch.clone(),
                    last.global_sequence,
                ));
            }
        }
        Ok(ReplayBatch {
            events,
            head,
            next,
            reset_required,
        })
    }

    async fn event_payload(&self, event: &EventId) -> Result<Option<EventPayload>, PortError> {
        Ok(self
            .state
            .lock()
            .expect("存储替身锁")
            .payloads
            .get(event.as_str())
            .cloned())
    }

    async fn read_session(
        &self,
        _query: acp_core::ports::HistoryQuery,
    ) -> Result<acp_core::ports::HistoryPage, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn node_link_slice(
        &self,
        _session: &SessionId,
        _after: Option<acp_core::model::OriginCursor>,
        _limit: ReplayLimit,
    ) -> Result<acp_core::ports::NodeLinkSlice, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn session_event_payload(
        &self,
        _session: &SessionId,
        _event: &EventId,
    ) -> Result<Option<EventPayload>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }
}

/// imported 会话索引替身（只有 `list_sessions` 被本工作包触及）。
#[derive(Clone, Default)]
pub(crate) struct SyncDeliveries {
    sessions: Arc<Mutex<Vec<ImportedSessionRecord>>>,
}

impl SyncDeliveries {
    pub(crate) fn push(&self, record: ImportedSessionRecord) {
        self.sessions.lock().expect("交付锁").push(record);
    }

    /// 一条可投影的 imported 索引行（字段齐全）。
    pub(crate) fn record(title: &str) -> ImportedSessionRecord {
        ImportedSessionRecord {
            session: RemoteSessionRef::new(
                acp_core::model::NodeId::new(OWNER).expect("owner"),
                acp_core::model::ExportId::new(EXPORT).expect("export"),
                SessionId::new(REMOTE_SESSION).expect("session id"),
            ),
            origin_epoch: Some(acp_core::model::OriginEpoch::new(ORIGIN_EPOCH).expect("epoch")),
            title: Some(title.to_owned()),
            agent: Some(
                acp_core::model::AgentRef::try_new(
                    acp_core::model::AgentId::new("codex").expect("agent id"),
                    "Codex",
                )
                .expect("agent ref"),
            ),
            state: Some(acp_core::model::SessionState::Running),
            version: Some(Version::from(4)),
            created_at: Some(timestamp("2026-09-17T12:10:00.123Z")),
            last_origin_sequence: Some(sequence(900)),
            acked: None,
            attachment: None,
            updated_at: timestamp("2026-09-17T12:11:00.123Z"),
        }
    }
}

#[async_trait::async_trait]
impl RemoteDeliveryStore for SyncDeliveries {
    async fn list_sessions(
        &self,
        _query: ImportedSessionQuery,
    ) -> Result<Vec<ImportedSessionRecord>, PortError> {
        Ok(self.sessions.lock().expect("交付锁").clone())
    }

    async fn commit_receipt(
        &self,
        _receipt: acp_core::ports::DeliveryReceipt,
    ) -> Result<acp_core::ports::ReceiptOutcome, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn local_replay(
        &self,
        _after: Option<LocalCursor>,
        _limit: ReplayLimit,
    ) -> Result<Vec<DeliveryIndexEntry>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn ack(
        &self,
        _session: &RemoteSessionRef,
        _cursor: acp_core::model::OriginCursor,
        _at: Timestamp,
    ) -> Result<acp_core::ports::AckOutcome, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn load_ack(
        &self,
        _session: &RemoteSessionRef,
    ) -> Result<Option<acp_core::model::OriginCursor>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn upsert_session(
        &self,
        _record: ImportedSessionRecord,
        _at: Timestamp,
    ) -> Result<(), PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn find_remote_request(
        &self,
        _session: &RemoteSessionRef,
        _request: &acp_core::model::RequestId,
    ) -> Result<Option<RemoteCommandRef>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn drop_import(
        &self,
        _import: &acp_core::model::ImportId,
    ) -> Result<DropReport, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn prune(
        &self,
        _policy: acp_core::ports::RetentionPolicy,
        _at: Timestamp,
    ) -> Result<acp_core::ports::PruneReport, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }
}

// ---------------------------------------------------------------------------------------------
// 世界
// ---------------------------------------------------------------------------------------------

/// 端口替身世界：`UseCases` 与 `Broker` 共享同一批实例（与生产组合根同形）。
pub(crate) struct World {
    pub(crate) core: Arc<UseCases>,
    pub(crate) broker: Arc<Broker>,
    pub(crate) store: SyncStore,
    pub(crate) deliveries: SyncDeliveries,
    pub(crate) trust: FakeTrust,
    pub(crate) config: FakeConfig,
    pub(crate) registry: Arc<ConnectionRegistry>,
}

impl World {
    pub(crate) fn new() -> Self {
        let clock = FakeClock::new();
        let trust = FakeTrust::default();
        let exports = FakeExports::default().with_clock(clock.clone());
        let audit = FakeAudit::default();
        let config = FakeConfig::default();
        let store = SyncStore::default();
        store.set_head(cursor(EPOCH, 0));
        let deliveries = SyncDeliveries::default();
        let store_port: Arc<dyn SessionStore> = Arc::new(store.clone());
        let delivery_port: Arc<dyn RemoteDeliveryStore> = Arc::new(deliveries.clone());
        let exports_port: Arc<dyn acp_core::ports::ExportStore> = Arc::new(exports);
        let audit_port: Arc<dyn acp_core::ports::AuditStore> = Arc::new(audit);
        let clock_port: Arc<dyn acp_core::ports::Clock> = Arc::new(clock.clone());
        let ids: Arc<dyn acp_core::ports::IdGenerator> = Arc::new(FakeIds::default());
        let broker = Arc::new(Broker::new(
            BrokerDeps {
                store: Arc::clone(&store_port),
                deliveries: Arc::clone(&delivery_port),
                backends: Arc::new(NotTouched),
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
            config: Arc::new(config.clone()),
            attachments: Arc::new(NotTouched),
            catalog: Arc::new(FakeCatalog::default()),
            clock: clock_port,
            ids,
        }));
        Self {
            core,
            broker,
            store,
            deliveries,
            trust,
            config,
            registry: ConnectionRegistry::new(),
        }
    }

    /// 路由（`owner` 由用例注入；`registry` 与世界共享，便于用例断言状态回收）。
    pub(crate) fn route(&self, owner: Arc<dyn OwnerReachability>) -> SubscribeRoute {
        SubscribeRoute::new(
            Arc::clone(&self.core),
            Arc::clone(&self.broker),
            Arc::clone(&self.registry),
            owner,
        )
    }

    /// 预置一条设备信任行（`sync_device_handshake_view` 的唯一读面）。
    ///
    /// 设备不再有授权集合（`pairing-model` 删除设备权限层），因此没有 scopes 参数。
    pub(crate) fn seed_device(&self, device: &str) {
        let record = DeviceRecord::try_new(
            acp_core::model::DeviceId::new(device).expect("device id"),
            "Test Phone",
            acp_core::model::Fingerprint::new(&"a".repeat(64)).expect("fingerprint"),
            DeviceState::Active,
            timestamp("2026-09-18T09:12:03.412Z"),
            None,
            None,
        )
        .expect("设备记录");
        self.trust.seed_device(record);
    }

    /// 预置一条 workspace 记录（`workspaces` 资源的唯一来源；`canonical_path` 只在快照条目上、且只在
    /// 客户端协商了 `core.workspace-path.v1` 时进快照）。
    pub(crate) fn seed_workspace(&self, alias: &str, display_name: &str, path: &str) {
        self.config.seed_workspace(
            acp_core::model::WorkspaceRecord::try_new(
                acp_core::model::WorkspaceAlias::new(alias).expect("alias"),
                display_name,
                path,
                timestamp("2026-09-17T12:10:00.123Z"),
                timestamp("2026-09-17T12:10:00.123Z"),
            )
            .expect("workspace 记录"),
        );
    }
    /// 预置一条 Agent profile（`agents` 资源的唯一来源）。
    pub(crate) fn seed_agent(&self, id: &str, display_name: &str, default: bool) {
        self.config.seed_profile(
            AgentProfile::try_new(
                acp_core::model::AgentId::new(id).expect("agent id"),
                display_name,
                "/usr/bin/codex",
                Vec::new(),
                Vec::new(),
                Vec::new(),
                default,
                timestamp("2026-09-17T12:10:00.123Z"),
                timestamp("2026-09-17T12:10:00.123Z"),
            )
            .expect("agent profile"),
        );
    }
}

/// 一个**本机绝对**的目录文本（`WorkspaceRecord::canonical_path` 要求绝对路径形状；Windows 上
/// `/home/...` 不是绝对路径）。用例用它断言「未协商 `core.workspace-path.v1` 时本机路径不进快照」
/// 与「协商后该条目带这个取值」。
pub(crate) fn workspace_path() -> String {
    std::env::temp_dir()
        .join("acpr-sync-wp4-workspace")
        .to_string_lossy()
        .into_owned()
}

/// 固定可达性的 Owner 可达性替身。
pub(crate) struct Reachability(pub(crate) bool);

impl OwnerReachability for Reachability {
    fn reachable(&self, _owner: &acp_core::model::NodeId) -> bool {
        self.0
    }
}

/// 一条已入队的出站帧（解码后的形状 + 原始文本）。
pub(crate) struct Frame {
    pub(crate) kind: String,
    pub(crate) body: serde_json::Value,
    pub(crate) text: String,
}

impl Frame {
    /// `details.reason`（`sync.cursor_invalid` 的唯一可断言取值）。
    pub(crate) fn reason(&self) -> Option<&str> {
        self.body.get("details")?.get("reason")?.as_str()
    }

    /// `body.code`（`error` 帧）。
    pub(crate) fn code(&self) -> Option<&str> {
        self.body.get("code")?.as_str()
    }
}

/// 建立一条已认证连接的句柄（出站队列的接收端交回用例）。
pub(crate) fn connection(
    device: &str,
) -> (Arc<ConnectionHandle>, tokio::sync::mpsc::Receiver<Outbound>) {
    connection_with(device, CONNECTION)
}

/// 指定 `connectionId` 的句柄（同一设备的两条连接的用例需要不同的连接标识）。
pub(crate) fn connection_with(
    device: &str,
    connection_id: &str,
) -> (Arc<ConnectionHandle>, tokio::sync::mpsc::Receiver<Outbound>) {
    connection_with_limits(device, connection_id, &SyncConfig::default())
}

/// 指定连接限额的句柄（分批投递的用例用它压小 `maxReplayEventsPerBatch`）。
pub(crate) fn connection_with_limits(
    device: &str,
    connection_id: &str,
    config: &SyncConfig,
) -> (Arc<ConnectionHandle>, tokio::sync::mpsc::Receiver<Outbound>) {
    ConnectionHandle::new(
        Uuid::parse(connection_id).expect("connection id"),
        acp_core::model::NodeId::new(HOST).expect("host id"),
        acp_core::model::DeviceId::new(device).expect("device id"),
        IpAddr::from(Ipv4Addr::LOCALHOST),
        SessionLimits::negotiate(config),
        Arc::new(FakeIds::default()),
    )
}

/// 一条认证后消息的信封（`connectionId`/`connectionSequence` 与本次连接一致）。
pub(crate) fn envelope(kind: &str, sequence: u64, body: serde_json::Value) -> Envelope {
    let frame = serde_json::json!({
        "protocolVersion": 1,
        "type": kind,
        "messageId": Uuid::parse("04dbad03-8869-4c5a-94fa-2f3af2fb622b").expect("message id").as_str(),
        "connectionId": CONNECTION,
        "connectionSequence": sequence.to_string(),
        "body": body,
    })
    .to_string();
    Envelope::decode(&frame).expect("测试信封合法")
}

/// 排空出站队列（用例只看"对端真的收到了哪几帧"）。
pub(crate) fn frames(receiver: &mut tokio::sync::mpsc::Receiver<Outbound>) -> Vec<Frame> {
    let mut collected = Vec::new();
    while let Ok(outbound) = receiver.try_recv() {
        let text = outbound.text;
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("出站帧是合法 JSON");
        collected.push(Frame {
            kind: parsed
                .get("type")
                .and_then(serde_json::Value::as_str)
                .expect("出站帧有 type")
                .to_owned(),
            body: parsed.get("body").cloned().expect("出站帧有 body"),
            text,
        });
    }
    collected
}

/// 某一类帧的 body 列表（按入队顺序）。
pub(crate) fn bodies<'a>(frames: &'a [Frame], kind: &str) -> Vec<&'a serde_json::Value> {
    frames
        .iter()
        .filter(|frame| frame.kind == kind)
        .map(|frame| &frame.body)
        .collect()
}
