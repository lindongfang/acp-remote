//! Access 侧运行期远程来源（`design.md` D2/D4/D5；`CORE_PORTS_AND_STORAGE.md` §5.9）。
//!
//! 本模块是 Owner 远程数据的**唯一**存在形式：进程内的有界内存目录、已校验事件的运行期缓冲、
//! 来源重放屏障的等待点，以及在线回源读取/命令查询的实现。它实现三个 core 端口
//! ([`RemoteRuntimeDirectory`]、[`RemoteSourceReads`]、[`RemoteCommandStatus`])，由 `app` 组合根接线。
//!
//! **生命期**：
//!
//! - 数据只在已认证连接期存在。`begin` 为每次连接尝试推进代际并清空上一代；`online` 之后目录才可用；
//!   断线（`offline`）、信任撤销（`revoked`）与 `invalidate` 都会**整代清除**条目、事件缓冲与屏障等待者。
//! - 不写数据库、临时文件或日志；重连恢复只依赖调用方显式提交的来源位置，本模块不读磁盘。
//! - 每个来源的目录最多 500 条（协议 `catalog.snapshot.sessions` 的 `maxItems`）；事件缓冲是
//!   [`EVENT_BUFFER_CAPACITY`] 条的 `broadcast` 通道，慢消费者只会收到 `Lagged` 并被要求重建，不会让
//!   队列无限增长。
//!
//! 这里没有收据、没有 local sequence、没有 ACK 落盘：`resource.ack` 只表达当前连接的运行期处理进度
//! （`design.md` D5）。

use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;

use acp_core::model::{
    AcpRaw, AgentId, AgentRef, CommandResult, CommandStatus, ConflictKind, Digest, EventId,
    EventPayload, EventType, JsonValueText, MessageChannel, MessageId, MessageRecord, MessageState,
    ModeId, ModeRef, NodeId, OriginCursor, OriginEpoch, OriginEventRef, PartialReason,
    PartialResult, PortError, RawUnavailableReason, RemoteSessionRef, RequestId, ResourceOrigin,
    Sequence, SessionId, SessionState, SessionSummary, SessionTitle, Timestamp, TurnId,
    UnavailableKind, Version, ViewJson,
};
use acp_core::ports::{
    HistoryInclude, MessageCoverage, RemoteCatalogRevision, RemoteCommandLookup,
    RemoteCommandRecord, RemoteCommandStatus, RemoteCommandTarget, RemoteDirectorySnapshot,
    RemoteEventStream, RemoteGeneration, RemoteHistoryRecord, RemoteNoticeReceiver,
    RemoteReadQuery, RemoteReadResult, RemoteRuntimeDirectory, RemoteSessionEntry,
    RemoteSourceBarrier, RemoteSourceNotice, RemoteSourceReads, RemoteSourceState,
    RemoteStreamEvent, RemoteStreamItem, RemoteStreamReceiver, RemoteStreamResetReason,
    RemoteTitleWrite,
};
use async_trait::async_trait;
use node_link_protocol::catalog::CatalogSnapshot;
use node_link_protocol::command::{
    CommandName, CommandPayload, CommandResultPayload, CommandStatus as StatusBody, CommandSubmit,
    IncludeResource, RemoteTarget, TerminalStatus,
};
use node_link_protocol::common::{
    DecimalString, Nullable, OriginCursor as WireOriginCursor, RawObject,
    RemoteSessionRef as WireRemoteSessionRef, SessionMeta, SessionState as WireSessionState, Uuid,
};
use node_link_protocol::resource::{
    Ack as WireAck, CaughtUp, Event, EventPayload as WireEventPayload, ResetReason, ResetRequired,
    Unavailable,
};
use sha2::{Digest as _, Sha256};
use tokio::sync::{broadcast, watch};
use uuid::Uuid as RandomUuid;

use crate::error::ClientError;
use crate::link::{CommandOutcome, Completion, SharedLink};
use crate::remote_session::wire_remote;

/// `resource.stateless-origin.v1`：运行期来源流/运行期 ACK 的协商 feature。
pub const STATELESS_ORIGIN_FEATURE: &str = "resource.stateless-origin.v1";

/// 每个会话的运行期事件缓冲上限（`broadcast` 容量）。
///
/// 超出的慢消费者收到 `Lagged`，由上层取消订阅并重新读取重建；绝不静默丢事件或落盘扩容。
pub const EVENT_BUFFER_CAPACITY: usize = 1024;

/// 单个来源的目录条目上限（`catalog.snapshot.sessions` 的 `maxItems`）。
const MAX_CATALOG_SESSIONS: usize = 500;

/// 等待来源屏障的上限：只有 Owner 明确发送 `resource.caught_up`/`resource.snapshot_end` 才算追平。
const BARRIER_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// 一条已校验的来源事件（原样 wire payload，逐字节保真）。
#[derive(Debug, Clone)]
pub struct RuntimeEvent {
    pub owner_node_id: NodeId,
    pub session_id: SessionId,
    pub origin_event_id: EventId,
    pub origin_epoch: OriginEpoch,
    pub origin_sequence: Sequence,
    pub event_type: EventType,
    pub payload_digest: Digest,
    pub created_at: Timestamp,
    pub payload: WireEventPayload,
}

/// `resource.event` 的处理结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventDisposition {
    /// 已交给运行期订阅；返回运行期游标（供 `resource.ack` 使用）。
    Forwarded(OriginCursor),
    /// 重复/过期/未知会话：不转发，也不关闭连接。
    Ignored,
}

/// 信任撤销的本地落库缝（由 `app` 注入；运行期不拥有持久信任）。
#[async_trait]
pub trait TrustRevocation: Send + Sync {
    async fn revoked(&self, owner: &NodeId) -> Result<(), PortError>;
}

struct OwnerState {
    generation: Option<RemoteGeneration>,
    counter: u64,
    source: RemoteSourceState,
    revision: u64,
    entries: BTreeMap<String, RemoteSessionEntry>,
    origin_epochs: BTreeMap<String, OriginEpoch>,
    sequences: BTreeMap<String, u64>,
    events: BTreeMap<String, broadcast::Sender<RuntimeEvent>>,
    barriers: BTreeMap<String, watch::Sender<Option<OriginCursor>>>,
    /// 每会话的流终止信号（`resource.reset_required`/`resource.unavailable`）。
    ///
    /// 与 [`Self::events`] 分开：事件通道关闭既可能是「Owner 要求重建」也可能是「连接断了」，只有这条
    /// 通道能区分两者，接收端因此不必猜。它**不随单会话 reset 清空**（清空会让接收端丢掉刚收到的原因）；
    /// 整代失效时 [`Self::clear_generation`] 一并丢弃。
    signals: BTreeMap<String, broadcast::Sender<RuntimeSignal>>,
    subscriptions: BTreeMap<String, Option<WireOriginCursor>>,
    /// 等待屏障期间缓冲的已校验事件（按 originSequence）；屏障到达后按序放行。
    pending: BTreeMap<String, BTreeMap<u64, RuntimeEvent>>,
    /// 仍处于「等待屏障」阶段的会话。
    replaying: std::collections::BTreeSet<String>,
    /// 缓冲溢出而失败的会话：必须等待客户端重建，屏障不得越过被丢弃的区间。
    failed: std::collections::BTreeSet<String>,
    /// 每会话已投递到的最高来源序号（只用于实时去重，不要求连续）。
    delivered: BTreeMap<String, u64>,
    /// 该会话当前活跃的扇出接收端数量；最后一个退出时才释放 Owner 订阅与屏障。
    subscribers: BTreeMap<String, usize>,
    /// 本代是否已收到 catalog 快照（空目录也算已建立）。
    catalog_seen: bool,
    /// 该来源是否曾经成功应用过 catalog 快照（连接期观测事实；撤销不清除）。
    catalog_ever: bool,
    link: Option<Arc<SharedLink>>,
}

impl Default for OwnerState {
    fn default() -> Self {
        Self {
            generation: None,
            counter: 0,
            source: RemoteSourceState::Offline,
            revision: 0,
            entries: BTreeMap::new(),
            origin_epochs: BTreeMap::new(),
            sequences: BTreeMap::new(),
            events: BTreeMap::new(),
            barriers: BTreeMap::new(),
            signals: BTreeMap::new(),
            subscriptions: BTreeMap::new(),
            pending: BTreeMap::new(),
            replaying: std::collections::BTreeSet::new(),
            failed: std::collections::BTreeSet::new(),
            delivered: BTreeMap::new(),
            subscribers: BTreeMap::new(),
            catalog_seen: false,
            catalog_ever: false,
            link: None,
        }
    }
}

impl OwnerState {
    fn clear_generation(&mut self) {
        self.entries.clear();
        self.origin_epochs.clear();
        self.sequences.clear();
        // drop senders => 订阅者收到 `Closed`，缓冲立即释放。
        self.events.clear();
        self.barriers.clear();
        self.signals.clear();
        self.subscriptions.clear();
        self.pending.clear();
        self.replaying.clear();
        self.failed.clear();
        self.delivered.clear();
        self.subscribers.clear();
        self.catalog_seen = false;
        self.link = None;
    }

    /// 屏障到达：按来源序号放行本会话缓冲的事件，返回已放行的最高序号（没有则为 0）。
    fn flush_pending(&mut self, key: &str) -> u64 {
        let pending = self.pending.remove(key).unwrap_or_default();
        self.replaying.remove(key);
        let sender = self
            .events
            .entry(key.to_owned())
            .or_insert_with(|| broadcast::channel(EVENT_BUFFER_CAPACITY).0)
            .clone();
        let mut last = 0;
        for (sequence, event) in pending {
            last = last.max(sequence);
            // 没有订阅者时 `send` 返回 `Err`：事件仍已校验，不构成错误。
            let _ = sender.send(event);
        }
        last
    }
}

/// 运行期来源流的终止原因（`apply_reset`/`apply_unavailable` 的落点）。
///
/// 它只描述**当前连接**上 Owner 对本次订阅给出的显式结论，不落盘、不跨连接复用（`design.md` D4/D6）。
#[derive(Debug, Clone)]
enum RuntimeSignal {
    /// `resource.reset_required`：客户端必须回源重建。
    Reset {
        reason: ResetReason,
        cursor: Option<OriginCursor>,
    },
    /// `resource.unavailable`：该来源会话在当前连接上不可用。
    Unavailable { error: acp_core::model::PublicError },
}

/// 运行期来源通知的通道容量（清单 revision/连接状态）；溢出由接收端重建，不扩容。
const NOTICE_CAPACITY: usize = 64;

/// 每会话流终止信号的通道容量（`reset_required`/`unavailable` 各一条足矣；通道满即视为终止）。
const SIGNAL_CAPACITY: usize = 8;

/// 运行期远程来源（见模块文档的生命期约束）。
pub struct RemoteRuntime {
    owners: std::sync::Mutex<BTreeMap<String, OwnerState>>,
    revocation: std::sync::Mutex<Option<Arc<dyn TrustRevocation>>>,
    /// 已通过 Owner/摘要校验的事件标识（只用于可观测与测试；不含正文）。
    observed: std::sync::Mutex<Vec<String>>,
    /// 来源清单/连接事实的变化通知（`sync.remote_catalog_changed`/`remote_source_state` 的唯一来源）。
    notices: broadcast::Sender<RemoteSourceNotice>,
}

impl Default for RemoteRuntime {
    fn default() -> Self {
        Self {
            owners: std::sync::Mutex::new(BTreeMap::new()),
            revocation: std::sync::Mutex::new(None),
            observed: std::sync::Mutex::new(Vec::new()),
            notices: broadcast::channel(NOTICE_CAPACITY).0,
        }
    }
}

impl std::fmt::Debug for RemoteRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RemoteRuntime")
            .field("owners", &lock(&self.owners).len())
            .finish_non_exhaustive()
    }
}

impl RemoteRuntime {
    /// 新建空的运行期（没有任何来源）。
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// 注入信任撤销的持久化落点（组合根调用一次）。
    pub fn set_trust_revocation(&self, sink: Arc<dyn TrustRevocation>) {
        *lock(&self.revocation) = Some(sink);
    }

    /// 开始一次新的连接尝试：推进代际并清空上一代。
    /// 已通过校验的来源事件标识（按到达顺序）。
    /// 该来源是否已经有非空运行期目录（同步可观测入口）。
    pub fn has_directory(&self, owner: &NodeId) -> bool {
        lock(&self.owners)
            .get(owner.as_str())
            .is_some_and(|state| state.catalog_ever)
    }

    pub fn observed_events(&self) -> Vec<String> {
        lock(&self.observed).clone()
    }

    pub fn begin(&self, owner: &NodeId) -> RemoteGeneration {
        let mut owners = lock(&self.owners);
        let state = owners.entry(owner.as_str().to_owned()).or_default();
        state.clear_generation();
        state.counter = state.counter.saturating_add(1);
        let generation = RemoteGeneration::new(state.counter);
        state.generation = Some(generation);
        state.source = RemoteSourceState::Offline;
        state.revision = 0;
        generation
    }

    /// 认证完成：目录进入在线状态并绑定连接句柄。
    pub fn online(&self, owner: &NodeId, generation: RemoteGeneration, link: Arc<SharedLink>) {
        let mut owners = lock(&self.owners);
        let Some(state) = owners.get_mut(owner.as_str()) else {
            return;
        };
        if state.generation != Some(generation) {
            return;
        }
        state.source = RemoteSourceState::Online;
        state.link = Some(link);
        drop(owners);
        self.notify(RemoteSourceNotice::State {
            owner_node_id: owner.clone(),
            state: RemoteSourceState::Online,
        });
    }

    /// 连接结束：整代清除（迟到结果不可再读到）。
    pub fn offline(&self, owner: &NodeId, generation: RemoteGeneration) {
        let mut owners = lock(&self.owners);
        let Some(state) = owners.get_mut(owner.as_str()) else {
            return;
        };
        if state.generation != Some(generation) {
            return;
        }
        state.clear_generation();
        state.generation = None;
        state.source = RemoteSourceState::Offline;
        state.revision = 0;
        drop(owners);
        self.notify(RemoteSourceNotice::State {
            owner_node_id: owner.clone(),
            state: RemoteSourceState::Offline,
        });
    }

    /// Owner 撤销了本节点（或本机撤销）：终态，条目清除且不再自动重连。
    pub async fn revoked(&self, owner: &NodeId) -> Result<(), PortError> {
        {
            let mut owners = lock(&self.owners);
            let state = owners.entry(owner.as_str().to_owned()).or_default();
            state.clear_generation();
            state.generation = None;
            state.source = RemoteSourceState::Revoked;
        }
        self.notify(RemoteSourceNotice::State {
            owner_node_id: owner.clone(),
            state: RemoteSourceState::Revoked,
        });
        let sink = lock(&self.revocation).clone();
        if let Some(sink) = sink {
            sink.revoked(owner).await?;
        }
        Ok(())
    }

    /// epoch 改变/重置：该会话的缓冲与屏障作废，等待重建。
    pub fn reset_session(&self, owner: &NodeId, session: &WireRemoteSessionRef) {
        let mut owners = lock(&self.owners);
        let Some(state) = owners.get_mut(owner.as_str()) else {
            return;
        };
        let key = session.session_id.as_str().to_owned();
        state.events.remove(&key);
        state.sequences.remove(&key);
        state.pending.remove(&key);
        state.delivered.remove(&key);
        state.origin_epochs.remove(&key);
        state.subscriptions.remove(&key);
        if let Some(sender) = state.barriers.get(&key) {
            let _ = sender.send(None);
        }
    }

    /// `catalog.snapshot`：用协议目录重建该来源的运行期条目（无标题、无正文）。
    pub fn apply_catalog(
        &self,
        owner: &NodeId,
        generation: RemoteGeneration,
        snapshot: &CatalogSnapshot,
    ) -> Result<(), PortError> {
        if snapshot.sessions.as_slice().len() > MAX_CATALOG_SESSIONS {
            return Err(PortError::Corrupt(
                "the owner catalog exceeds the runtime directory budget",
            ));
        }
        let mut names = BTreeMap::new();
        for agent in snapshot.agents.as_slice() {
            names.insert(
                agent.agent_id.as_str().to_owned(),
                agent.name.as_str().to_owned(),
            );
        }
        let revision = snapshot
            .revision
            .as_str()
            .parse::<u64>()
            .map_err(|_| PortError::Corrupt("the catalog revision is not a decimal string"))?;
        let mut owners = lock(&self.owners);
        let Some(state) = owners.get_mut(owner.as_str()) else {
            return Ok(());
        };
        if state.generation != Some(generation) || state.source != RemoteSourceState::Online {
            return Ok(());
        }
        state.catalog_seen = true;
        state.catalog_ever = true;
        let mut entries = BTreeMap::new();
        let mut epochs = BTreeMap::new();
        for session in snapshot.sessions.as_slice() {
            let session_id = SessionId::new(session.session_id.as_str())
                .map_err(|_| PortError::Corrupt("the catalog carries an invalid session id"))?;
            let origin_epoch = OriginEpoch::new(session.origin_epoch.as_str())
                .map_err(|_| PortError::Corrupt("the catalog carries an invalid origin epoch"))?;
            let agent_id = AgentId::new(session.agent_id.as_str())
                .map_err(|_| PortError::Corrupt("the catalog carries an invalid agent id"))?;
            let name = names
                .get(session.agent_id.as_str())
                .map(String::as_str)
                .unwrap_or_else(|| session.agent_id.as_str());
            let agent = AgentRef::try_new(agent_id, name)
                .map_err(|_| PortError::Corrupt("the catalog carries an invalid agent name"))?;
            let version = session
                .version
                .as_str()
                .parse::<u64>()
                .map_err(|_| PortError::Corrupt("the catalog carries an invalid version"))?;
            let created_at = Timestamp::new(session.created_at.as_str())
                .map_err(|_| PortError::Corrupt("the catalog carries an invalid timestamp"))?;
            let summary = SessionSummary::try_new(
                session_id.clone(),
                None,
                agent,
                core_state(session.state),
                ResourceOrigin::Remote {
                    owner_node_id: owner.clone(),
                    origin_epoch: origin_epoch.clone(),
                    online: true,
                },
                None,
                Version::new(version),
                created_at.clone(),
                created_at,
                None,
            )
            .map_err(|_| PortError::Corrupt("the catalog summary is not representable"))?;
            entries.insert(
                session_id.as_str().to_owned(),
                RemoteSessionEntry {
                    summary,
                    generation,
                    source: RemoteSourceState::Online,
                },
            );
            epochs.insert(session_id.as_str().to_owned(), origin_epoch);
        }
        state.revision = revision;
        state.entries = entries;
        state.origin_epochs = epochs;
        state
            .events
            .retain(|key, _| state.origin_epochs.contains_key(key));
        state
            .barriers
            .retain(|key, _| state.origin_epochs.contains_key(key));
        drop(owners);
        self.notify(RemoteSourceNotice::CatalogChanged {
            owner_node_id: owner.clone(),
            revision: RemoteCatalogRevision::new(revision),
        });
        Ok(())
    }

    /// 快照 `session_meta` item：只更新状态与版本。
    pub fn apply_session_meta(
        &self,
        owner: &NodeId,
        generation: RemoteGeneration,
        session: &WireRemoteSessionRef,
        meta: &SessionMeta,
    ) -> Result<(), PortError> {
        let mut owners = lock(&self.owners);
        let Some(state) = owners.get_mut(owner.as_str()) else {
            return Ok(());
        };
        if state.generation != Some(generation) || state.source != RemoteSourceState::Online {
            return Ok(());
        }
        let Some(entry) = state.entries.get_mut(session.session_id.as_str()) else {
            return Ok(());
        };
        let version = meta
            .version
            .as_str()
            .parse::<u64>()
            .map_err(|_| PortError::Corrupt("session_meta.version is not a decimal string"))?;
        let summary = &entry.summary;
        let rebuilt = SessionSummary::try_new(
            summary.session_id().clone(),
            summary.title().map(str::to_owned),
            summary.agent().clone(),
            core_state(meta.state),
            summary.origin().clone(),
            summary.current_mode().cloned(),
            Version::new(version),
            summary.created_at().clone(),
            summary.updated_at().clone(),
            summary.workspace().cloned(),
        )
        .map_err(|_| PortError::Corrupt("session_meta cannot be applied to the summary"))?;
        entry.summary = rebuilt;
        Ok(())
    }

    /// `resource.event`：校验 Owner/epoch/摘要/单调性，然后交给有界运行期缓冲。
    pub fn apply_event(
        &self,
        owner: &NodeId,
        generation: RemoteGeneration,
        event: &Event,
    ) -> Result<EventDisposition, PortError> {
        if event.session_ref.owner_node_id.as_str() != owner.as_str() {
            return Err(PortError::InvalidRequest(
                "the event belongs to a different owner node",
            ));
        }
        let expected = payload_digest(&event.payload).ok_or(PortError::InvalidRequest(
            "the event payload cannot be canonicalized",
        ))?;
        if expected != *event.payload_digest.as_bytes() {
            return Err(PortError::InvalidRequest(
                "the event payload digest does not match",
            ));
        }
        let session_id = SessionId::new(event.session_ref.session_id.as_str())
            .map_err(|_| PortError::Corrupt("the event carries an invalid session id"))?;
        let origin_epoch = OriginEpoch::new(event.origin_epoch.as_str())
            .map_err(|_| PortError::Corrupt("the event carries an invalid origin epoch"))?;
        let origin_event_id = EventId::new(event.origin_event_id.as_str())
            .map_err(|_| PortError::Corrupt("the event carries an invalid event id"))?;
        let sequence = event
            .origin_sequence
            .as_str()
            .parse::<u64>()
            .map_err(|_| PortError::Corrupt("the event origin sequence is not decimal"))?;
        let event_type = EventType::new(event.event_type.as_str())
            .map_err(|_| PortError::Corrupt("the event type is invalid"))?;
        let created_at = Timestamp::new(event.created_at.as_str())
            .map_err(|_| PortError::Corrupt("the event timestamp is invalid"))?;
        {
            let mut observed = lock(&self.observed);
            if observed.len() >= 512 {
                observed.remove(0);
            }
            observed.push(origin_event_id.as_str().to_owned());
        }
        let key = session_id.as_str().to_owned();
        let mut owners = lock(&self.owners);
        let Some(state) = owners.get_mut(owner.as_str()) else {
            return Ok(EventDisposition::Ignored);
        };
        if state.generation != Some(generation) || state.source != RemoteSourceState::Online {
            return Ok(EventDisposition::Ignored);
        }
        match state.origin_epochs.get(&key) {
            Some(epoch) if epoch == &origin_epoch => {}
            Some(_) => {
                return Err(PortError::Corrupt(
                    "the event origin epoch disagrees with the runtime catalog",
                ));
            }
            // 目录尚未到达：按迟到事件忽略（连接内的重放会补齐）。
            None => return Ok(EventDisposition::Ignored),
        }
        let runtime_event = RuntimeEvent {
            owner_node_id: owner.clone(),
            session_id,
            origin_event_id,
            origin_epoch: origin_epoch.clone(),
            origin_sequence: Sequence::new(sequence)
                .map_err(|_| PortError::Corrupt("the event sequence is out of range"))?,
            event_type,
            payload_digest: Digest::new(&base64url(&expected))
                .map_err(|_| PortError::Corrupt("the event digest is not canonical"))?,
            created_at,
            payload: event.payload.clone(),
        };
        if state.replaying.contains(&key) {
            // 等待屏障：重放与可能插队的实时帧一律先缓冲，屏障到达后再按来源序号放行。
            // 来源序号可能有合法空洞（压缩行不进入重放），因此**不**要求连续。
            let mut pending = state.pending.remove(&key).unwrap_or_default();
            if pending.len() >= EVENT_BUFFER_CAPACITY {
                if let Some(signal) = state.signals.get(&key) {
                    let _ = signal.send(RuntimeSignal::Unavailable {
                        error: backpressure_error(),
                    });
                }
                // 屏障不得越过被丢弃的区间：标记失败，等客户端重建。
                state.failed.insert(key.clone());
                state.pending.remove(&key);
                state.replaying.remove(&key);
                return Ok(EventDisposition::Ignored);
            }
            pending.insert(sequence, runtime_event);
            state.pending.insert(key, pending);
            return Ok(EventDisposition::Ignored);
        }
        let delivered = state.delivered.get(&key).copied().unwrap_or(0);
        if sequence <= delivered {
            return Ok(EventDisposition::Ignored);
        }
        let sender = state
            .events
            .entry(key.clone())
            .or_insert_with(|| broadcast::channel(EVENT_BUFFER_CAPACITY).0)
            .clone();
        let _ = sender.send(runtime_event);
        state.delivered.insert(key.clone(), sequence);
        state.sequences.insert(key, sequence);
        Ok(EventDisposition::Forwarded(OriginCursor::new(
            origin_epoch,
            Sequence::new(sequence)
                .map_err(|_| PortError::Corrupt("the event sequence is out of range"))?,
        )))
    }

    /// `resource.caught_up`：来源重放屏障到达。
    ///
    /// 屏障之前的事件必须已按序投递；有缺口时明确 backpressure（客户端重建），不得让
    /// `caught_up` 越过缺失的重放帧。`null` 订阅以快照内容建立屏障，因此把连续位置推进到屏障。
    pub fn apply_caught_up(&self, owner: &NodeId, session: &WireRemoteSessionRef, body: &CaughtUp) {
        let Ok(cursor) = core_cursor(&body.cursor) else {
            return;
        };
        let key = session.session_id.as_str().to_owned();
        let mut owners = lock(&self.owners);
        let Some(state) = owners.get_mut(owner.as_str()) else {
            return;
        };
        if state.failed.remove(&key) {
            state.pending.remove(&key);
            state.replaying.remove(&key);
            if let Some(signal) = state.signals.get(&key) {
                let _ = signal.send(RuntimeSignal::Unavailable {
                    error: backpressure_error(),
                });
            }
            return;
        }
        let flushed = state.flush_pending(&key);
        let barrier = cursor.origin_sequence.get();
        // Owner 已保证重放完成（含压缩造成的合法空洞）；`delivered` 只用于之后的实时去重。
        let delivered = state
            .delivered
            .get(&key)
            .copied()
            .unwrap_or(0)
            .max(flushed)
            .max(barrier);
        state.delivered.insert(key.clone(), delivered);
        state.sequences.insert(key.clone(), delivered);
        let sender = state
            .barriers
            .entry(key)
            .or_insert_with(|| watch::channel(None).0);
        let _ = sender.send(Some(cursor));
    }

    /// `resource.snapshot_end`：无 `caught_up` 的路径以快照结束游标作为来源屏障。
    pub fn apply_snapshot_end(
        &self,
        owner: &NodeId,
        session: &WireRemoteSessionRef,
        cursor: &WireOriginCursor,
    ) {
        let Ok(cursor) = core_cursor(cursor) else {
            return;
        };
        let key = session.session_id.as_str().to_owned();
        let mut owners = lock(&self.owners);
        let Some(state) = owners.get_mut(owner.as_str()) else {
            return;
        };
        if state.failed.remove(&key) {
            state.pending.remove(&key);
            state.replaying.remove(&key);
            if let Some(signal) = state.signals.get(&key) {
                let _ = signal.send(RuntimeSignal::Unavailable {
                    error: backpressure_error(),
                });
            }
            return;
        }
        let flushed = state.flush_pending(&key);
        let barrier = cursor.origin_sequence.get();
        let delivered = state
            .delivered
            .get(&key)
            .copied()
            .unwrap_or(0)
            .max(flushed)
            .max(barrier);
        state.delivered.insert(key.clone(), delivered);
        state.sequences.insert(key.clone(), delivered);
        let sender = state
            .barriers
            .entry(key)
            .or_insert_with(|| watch::channel(None).0);
        let _ = sender.send(Some(cursor));
    }

    /// `resource.reset_required`：旧订阅/游标不可续读。
    pub fn apply_reset(
        &self,
        owner: &NodeId,
        session: &WireRemoteSessionRef,
        body: &ResetRequired,
    ) {
        let cursor = body
            .cursor
            .as_ref()
            .and_then(|cursor| core_cursor(cursor).ok());
        if let Some(sender) = self.signal_sender(owner, session.session_id.as_str()) {
            let _ = sender.send(RuntimeSignal::Reset {
                reason: body.reason,
                cursor,
            });
        }
        self.reset_session(owner, session);
    }

    /// `resource.unavailable`：该来源会话在当前连接上不可用。
    ///
    /// 无法归属到具体会话（`attachmentId` 缺失/未知）时信号发给该 Owner 的**全部**会话：Owner 已明确
    /// 声明这条来源不可用，让订阅继续等一个不会再来的屏障会一直挂住。
    pub fn apply_unavailable(
        &self,
        owner: &NodeId,
        session: Option<&WireRemoteSessionRef>,
        body: &Unavailable,
    ) {
        let error = core_public_error(&body.error);
        let senders: Vec<broadcast::Sender<RuntimeSignal>> = {
            let owners = lock(&self.owners);
            match owners.get(owner.as_str()) {
                None => Vec::new(),
                Some(state) => match session {
                    Some(session) => state
                        .signals
                        .get(session.session_id.as_str())
                        .cloned()
                        .into_iter()
                        .collect(),
                    None => state.signals.values().cloned().collect(),
                },
            }
        };
        for sender in senders {
            let _ = sender.send(RuntimeSignal::Unavailable {
                error: error.clone(),
            });
        }
        if let Some(session) = session {
            self.reset_session(owner, session);
        }
    }

    /// 取（必要时建立）该会话的流终止信号发送端。
    fn signal_sender(
        &self,
        owner: &NodeId,
        session: &str,
    ) -> Option<broadcast::Sender<RuntimeSignal>> {
        let mut owners = lock(&self.owners);
        let state = owners.get_mut(owner.as_str())?;
        Some(
            state
                .signals
                .entry(session.to_owned())
                .or_insert_with(|| broadcast::channel(SIGNAL_CAPACITY).0)
                .clone(),
        )
    }

    /// 广播一条来源通知（没有接收者不是错误：通知只为当前连接服务）。
    fn notify(&self, notice: RemoteSourceNotice) {
        let _ = self.notices.send(notice);
    }

    /// 当前连接上的订阅：attach + `resource.subscribe`（幂等），返回运行期事件接收端。
    pub async fn subscribe(
        &self,
        remote: &RemoteSessionRef,
        cursor: Option<WireOriginCursor>,
    ) -> Result<broadcast::Receiver<RuntimeEvent>, ClientError> {
        let link = self.link_of(&remote.owner_node_id)?;
        let key = remote.session_id.as_str().to_owned();
        // 同一会话同一连接只维护一个 Owner 订阅；后续订阅者扇出同一个已校验事件流，
        // 不重发订阅、不清空在途缓冲。它们从当前屏障/之后的事件开始，明细由各自读取重建。
        {
            let mut owners = lock(&self.owners);
            let state = owners
                .get_mut(remote.owner_node_id.as_str())
                .ok_or(ClientError::OwnerOffline)?;
            if state.subscriptions.contains_key(&key) {
                *state.subscribers.entry(key.clone()).or_insert(0) += 1;
                return Ok(state
                    .events
                    .entry(key)
                    .or_insert_with(|| broadcast::channel(EVENT_BUFFER_CAPACITY).0)
                    .subscribe());
            }
        }
        let wire = wire_remote(remote)?;
        let ticket = link.attach(wire).await?;
        // 先进入 replaying 并创建接收端，再发订阅：屏障帧可能在 send 返回后立刻被读循环处理。
        let receiver = {
            let mut owners = lock(&self.owners);
            let state = owners
                .get_mut(remote.owner_node_id.as_str())
                .ok_or(ClientError::OwnerOffline)?;
            state.subscriptions.insert(key.clone(), cursor.clone());
            state.subscribers.insert(key.clone(), 1);
            state.replaying.insert(key.clone());
            state.pending.remove(&key);
            state.failed.remove(&key);
            state
                .events
                .entry(key.clone())
                .or_insert_with(|| broadcast::channel(EVENT_BUFFER_CAPACITY).0)
                .subscribe()
        };
        if let Err(error) = link.subscribe(&ticket, cursor).await {
            let mut owners = lock(&self.owners);
            if let Some(state) = owners.get_mut(remote.owner_node_id.as_str()) {
                state.subscriptions.remove(&key);
                state.subscribers.remove(&key);
                state.replaying.remove(&key);
                state.pending.remove(&key);
            }
            return Err(error);
        }
        Ok(receiver)
    }

    /// 当前连接的订阅接收端（未订阅时为 `None`）。
    pub fn events(&self, remote: &RemoteSessionRef) -> Option<broadcast::Receiver<RuntimeEvent>> {
        lock(&self.owners)
            .get(remote.owner_node_id.as_str())?
            .events
            .get(remote.session_id.as_str())
            .map(broadcast::Sender::subscribe)
    }

    /// 取消订阅：只影响本连接，不发送任何持久化 ACK。
    pub fn unsubscribe(&self, remote: &RemoteSessionRef) {
        let mut owners = lock(&self.owners);
        if let Some(state) = owners.get_mut(remote.owner_node_id.as_str()) {
            let key = remote.session_id.as_str();
            let remaining = state
                .subscribers
                .get_mut(key)
                .map(|count| count.saturating_sub(1))
                .unwrap_or(0);
            if remaining > 0 {
                state.subscribers.insert(key.to_owned(), remaining);
                return;
            }
            state.subscribers.remove(key);
            state.subscriptions.remove(key);
            state.replaying.remove(key);
            state.pending.remove(key);
            state.barriers.remove(key);
        }
    }

    fn link_of(&self, owner: &NodeId) -> Result<Arc<SharedLink>, ClientError> {
        let owners = lock(&self.owners);
        let state = owners
            .get(owner.as_str())
            .ok_or(ClientError::OwnerOffline)?;
        if state.source != RemoteSourceState::Online {
            return Err(ClientError::OwnerOffline);
        }
        state.link.clone().ok_or(ClientError::OwnerOffline)
    }

    fn current_generation(&self, owner: &NodeId) -> Option<RemoteGeneration> {
        let owners = lock(&self.owners);
        let state = owners.get(owner.as_str())?;
        (state.source == RemoteSourceState::Online)
            .then_some(state.generation)
            .flatten()
    }

    async fn wait_barrier(
        &self,
        remote: &RemoteSessionRef,
        generation: RemoteGeneration,
        cursor: Option<WireOriginCursor>,
    ) -> Result<RemoteSourceBarrier, PortError> {
        if self.current_generation(&remote.owner_node_id) != Some(generation) {
            return Err(PortError::Unavailable(UnavailableKind::OwnerOffline));
        }
        if !self.has_subscription(remote) {
            let _ = self
                .subscribe(remote, cursor)
                .await
                .map_err(|error| error.to_port_error())?;
        }
        let mut receiver = {
            let mut owners = lock(&self.owners);
            let state = owners
                .get_mut(remote.owner_node_id.as_str())
                .ok_or(PortError::Unavailable(UnavailableKind::OwnerOffline))?;
            state
                .barriers
                .entry(remote.session_id.as_str().to_owned())
                .or_insert_with(|| watch::channel(None).0)
                .subscribe()
        };
        let wait = async {
            loop {
                if let Some(value) = receiver.borrow_and_update().clone() {
                    return Ok(value);
                }
                if receiver.changed().await.is_err() {
                    return Err(PortError::Unavailable(UnavailableKind::OwnerOffline));
                }
            }
        };
        let value = tokio::time::timeout(BARRIER_WAIT, wait)
            .await
            .map_err(|_| PortError::Unavailable(UnavailableKind::OwnerOffline))??;
        if self.current_generation(&remote.owner_node_id) != Some(generation) {
            return Err(PortError::Unavailable(UnavailableKind::OwnerOffline));
        }
        Ok(RemoteSourceBarrier {
            cursor: value,
            generation,
        })
    }

    fn has_subscription(&self, remote: &RemoteSessionRef) -> bool {
        lock(&self.owners)
            .get(remote.owner_node_id.as_str())
            .is_some_and(|state| state.subscriptions.contains_key(remote.session_id.as_str()))
    }
}

#[async_trait]
impl RemoteRuntimeDirectory for RemoteRuntime {
    async fn list(&self) -> Result<Vec<RemoteSessionEntry>, PortError> {
        Ok(lock(&self.owners)
            .values()
            .filter(|state| state.source == RemoteSourceState::Online)
            .flat_map(|state| state.entries.values().cloned())
            .collect())
    }

    async fn snapshot(&self, owner: &NodeId) -> Result<Option<RemoteDirectorySnapshot>, PortError> {
        let owners = lock(&self.owners);
        let Some(state) = owners.get(owner.as_str()) else {
            return Ok(None);
        };
        if state.source != RemoteSourceState::Online {
            return Ok(None);
        }
        let Some(generation) = state.generation else {
            return Ok(None);
        };
        Ok(Some(RemoteDirectorySnapshot {
            owner_node_id: owner.clone(),
            generation,
            revision: RemoteCatalogRevision::new(state.revision),
            source: state.source,
            sessions: state.entries.values().cloned().collect(),
        }))
    }

    async fn lookup(
        &self,
        session: &RemoteSessionRef,
    ) -> Result<Option<RemoteSessionEntry>, PortError> {
        Ok(lock(&self.owners)
            .get(session.owner_node_id.as_str())
            .filter(|state| state.source == RemoteSourceState::Online)
            .and_then(|state| state.entries.get(session.session_id.as_str()).cloned()))
    }

    async fn find_by_session(
        &self,
        session: &SessionId,
    ) -> Result<Vec<RemoteSessionEntry>, PortError> {
        Ok(lock(&self.owners)
            .values()
            .filter(|state| state.source == RemoteSourceState::Online)
            .filter_map(|state| state.entries.get(session.as_str()).cloned())
            .collect())
    }

    async fn owner_state(&self, owner: &NodeId) -> Result<RemoteSourceState, PortError> {
        Ok(lock(&self.owners)
            .get(owner.as_str())
            .map_or(RemoteSourceState::Offline, |state| state.source))
    }

    async fn generation(&self, owner: &NodeId) -> Result<Option<RemoteGeneration>, PortError> {
        Ok(lock(&self.owners)
            .get(owner.as_str())
            .filter(|state| state.source == RemoteSourceState::Online)
            .and_then(|state| state.generation))
    }

    async fn invalidate(
        &self,
        owner: &NodeId,
        generation: RemoteGeneration,
    ) -> Result<(), PortError> {
        let mut owners = lock(&self.owners);
        let Some(state) = owners.get_mut(owner.as_str()) else {
            return Ok(());
        };
        if state.generation != Some(generation) {
            return Ok(());
        }
        state.clear_generation();
        state.generation = None;
        if state.source != RemoteSourceState::Revoked {
            state.source = RemoteSourceState::Offline;
        }
        let observed = state.source;
        drop(owners);
        self.notify(RemoteSourceNotice::State {
            owner_node_id: owner.clone(),
            state: observed,
        });
        Ok(())
    }
}

#[async_trait]
impl RemoteSourceReads for RemoteRuntime {
    async fn read(
        &self,
        session: &RemoteSessionRef,
        generation: RemoteGeneration,
        query: RemoteReadQuery,
    ) -> Result<RemoteReadResult, PortError> {
        let link = self
            .link_of(&session.owner_node_id)
            .map_err(|error| error.to_port_error())?;
        if !link.has_feature(STATELESS_ORIGIN_FEATURE) {
            return Err(PortError::Unavailable(UnavailableKind::BackendUnsupported));
        }
        let remote = crate::remote_session::RemoteSession::open(link, session.clone());
        let document = remote.read_document(query.clone()).await?;
        let result = map_read_document(session, &document)?;
        if self.current_generation(&session.owner_node_id) != Some(generation) {
            return Err(PortError::Unavailable(UnavailableKind::OwnerOffline));
        }
        Ok(result)
    }

    async fn replay_barrier(
        &self,
        session: &RemoteSessionRef,
        generation: RemoteGeneration,
        after: Option<OriginCursor>,
    ) -> Result<RemoteSourceBarrier, PortError> {
        let link = self
            .link_of(&session.owner_node_id)
            .map_err(|error| error.to_port_error())?;
        if !link.has_feature(STATELESS_ORIGIN_FEATURE) {
            return Err(PortError::Unavailable(UnavailableKind::BackendUnsupported));
        }
        let cursor = after.as_ref().map(wire_cursor).transpose()?;
        self.wait_barrier(session, generation, cursor).await
    }
}

#[async_trait]
impl RemoteCommandStatus for RemoteRuntime {
    async fn query(
        &self,
        target: &RemoteCommandTarget,
        request: &RequestId,
    ) -> Result<RemoteCommandLookup, PortError> {
        let link = self
            .link_of(&target.owner_node_id)
            .map_err(|error| error.to_port_error())?;
        if !link.has_feature(STATELESS_ORIGIN_FEATURE) {
            return Err(PortError::Unavailable(UnavailableKind::BackendUnsupported));
        }
        let request_id = parse_uuid(request.as_str())?;
        let target_request_id = Uuid::parse(request.as_str())
            .map_err(|_| PortError::InvalidRequest("the request id is not a uuid"))?;
        let submit = CommandSubmit {
            request_id,
            command: CommandName::CommandStatus,
            session_ref: Nullable::null(),
            attachment_id: Nullable::null(),
            attachment_generation: Nullable::null(),
            expected_version: Nullable::null(),
            payload: CommandPayload::CommandStatus(StatusBody {
                target_request_id,
                remote_target: Some(RemoteTarget {
                    owner_node_id: Uuid::parse(link.owner().as_str()).map_err(|_| {
                        PortError::Corrupt("the owner node id is not a canonical uuid")
                    })?,
                    session_id: match &target.session_id {
                        Some(session) => Nullable::value(parse_uuid(session.as_str())?),
                        None => Nullable::null(),
                    },
                }),
            }),
        };
        let outcome = link
            .submit(submit, Completion::Terminal)
            .await
            .map_err(|error| error.to_port_error())?;
        map_command_outcome(target, request, outcome)
    }
}

#[async_trait]
impl RemoteTitleWrite for RemoteRuntime {
    /// Access 侧向 Owner 提交 `session.title.set`（`design.md` D2/D3）。
    ///
    /// 顺序即合同：**在线**（`link_of`）→ **feature**（Owner 选中了 `core.session-title.v1`）→
    /// **代际**（调用方给的 `generation` 仍是当前代）→ 提交 → 提交返回后**再复核一次代际**。
    ///
    /// 三条否定都是显式分类，不互相替代：
    /// - Owner 未选中 feature / 不认识该命令 → `capability.unsupported_by_broker`（永久失败；**不**发副作用
    ///   命令，也不落到 `BackendUnsupported`——那个码在本进程里表示「没有远程写面」，Sync 层据此回
    ///   `session.not_found`）；
    /// - 连接不在线 → `OwnerOffline`；
    /// - 代际已变（断线/撤销/重连）→ `OwnerOffline`，迟到的成功一律丢弃。
    ///
    /// **不落盘**：本实现只在内存中转发，返回 Owner 的当前记录；标题、命令状态与请求引用都不写入任何
    /// 本地设施。`requestId` 原样使用（不换号、不自动重发）。
    async fn set_title(
        &self,
        session: &RemoteSessionRef,
        generation: RemoteGeneration,
        request: &RequestId,
        title: &SessionTitle,
    ) -> Result<RemoteCommandRecord, PortError> {
        let link = self
            .link_of(&session.owner_node_id)
            .map_err(|error| error.to_port_error())?;
        if !link.has_feature(crate::handshake::SESSION_TITLE_FEATURE) {
            return Err(PortError::InvalidRequest(
                "capability.unsupported_by_broker",
            ));
        }
        if self.current_generation(&session.owner_node_id) != Some(generation) {
            return Err(PortError::Unavailable(UnavailableKind::OwnerOffline));
        }
        let remote = crate::remote_session::RemoteSession::open(link, session.clone());
        let outcome = match remote.set_title(request.clone(), title).await {
            Ok(outcome) => outcome,
            // 帧已写出但结果丢失：Owner 可能已经提交，不能报成确定失败。返回 `uncertain` 记录，
            // 让客户端保留原 requestId 并在重连后走 `command.status` 回源（不自动换号重发）。
            Err(
                ClientError::Uncertain
                | ClientError::Timeout
                | ClientError::Transport(_)
                | ClientError::Closed,
            ) => {
                return Ok(uncertain_title_record(session, request));
            }
            Err(error) => return Err(error.to_port_error()),
        };
        // 迟到结果：写出期间代际已经变了（断线/撤销/重连）就不认这次成功，但也不能断言失败。
        if self.current_generation(&session.owner_node_id) != Some(generation) {
            return Ok(uncertain_title_record(session, request));
        }
        title_record(session, request, outcome)
    }
}

/// 结果未知的标题命令记录：没有 Owner 终态、没有终态事件标识，只说明「不能确定是否已提交」。
fn uncertain_title_record(session: &RemoteSessionRef, request: &RequestId) -> RemoteCommandRecord {
    RemoteCommandRecord {
        target: RemoteCommandTarget {
            owner_node_id: session.owner_node_id.clone(),
            session_id: Some(session.session_id.clone()),
        },
        request: request.clone(),
        status: CommandStatus::Uncertain,
        accepted_at: None,
        terminal_at: None,
        terminal_event_id: None,
        result: None,
        error: None,
    }
}

// ---------------------------------------------------------------------------------------------
// 运行期来源事件流（`RemoteEventStream`）：broadcast 缓冲 + 屏障 + 终止信号
// ---------------------------------------------------------------------------------------------

/// 订阅所处的阶段：屏障之前只允许把「已重放」的事件交给调用方，屏障之后才是实时事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamPhase {
    AwaitingBarrier,
    Live,
}

/// 一条运行期事件流的接收端（包装 runtime 的 broadcast 通道，`design.md` D3/D4）。
///
/// 三条处置都是**显式**的，不静默丢：
/// - 慢消费者（`Lagged`）→ [`RemoteStreamItem::Unavailable`] 并结束本次订阅（客户端必须重建）；
/// - `resource.reset_required`/`resource.unavailable` → 对应的 [`RemoteStreamItem`]；
/// - 连接断开/代际失效 → `Unavailable(OwnerOffline)`。
struct RuntimeStreamReceiver {
    generation: RemoteGeneration,
    events: broadcast::Receiver<RuntimeEvent>,
    signals: broadcast::Receiver<RuntimeSignal>,
    barrier: watch::Receiver<Option<OriginCursor>>,
    phase: StreamPhase,
    buffered: std::collections::VecDeque<RemoteStreamItem>,
    ended: bool,
}

impl RuntimeStreamReceiver {
    /// 屏障到达：把已缓冲的事件按「重放 / 实时」切开，插在 `CaughtUp` 两侧。
    fn open_barrier(&mut self, cursor: OriginCursor) {
        self.phase = StreamPhase::Live;
        let mut live = std::collections::VecDeque::new();
        let mut lagged = false;
        loop {
            match self.events.try_recv() {
                Ok(event) => {
                    let replayed = event.origin_epoch == cursor.origin_epoch
                        && event.origin_sequence <= cursor.origin_sequence;
                    let item = match stream_item(event) {
                        Some(item) => item,
                        None => continue,
                    };
                    if replayed {
                        self.buffered.push_back(item);
                    } else {
                        live.push_back(item);
                    }
                }
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(broadcast::error::TryRecvError::Closed) => break,
                Err(broadcast::error::TryRecvError::Lagged(_)) => {
                    lagged = true;
                    break;
                }
            }
        }
        self.buffered
            .push_back(RemoteStreamItem::CaughtUp(RemoteSourceBarrier {
                cursor,
                generation: self.generation,
            }));
        if lagged {
            self.buffered.push_back(backpressure_item());
            self.ended = true;
            return;
        }
        self.buffered.extend(live);
    }

    /// 取出已到达的终止信号（`try_recv`；通道关闭返回 `None`）。
    fn drain_signal(&mut self) -> Option<RemoteStreamItem> {
        loop {
            match self.signals.try_recv() {
                Ok(signal) => return Some(signal_item(signal)),
                Err(broadcast::error::TryRecvError::Empty) => return None,
                Err(broadcast::error::TryRecvError::Closed) => return None,
                Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
            }
        }
    }

    /// 事件通道的一次结果 → 下一项（`None` = 继续等待）。
    fn event_item(
        &mut self,
        received: Result<RuntimeEvent, broadcast::error::RecvError>,
    ) -> Option<RemoteStreamItem> {
        match received {
            Ok(event) => stream_item(event),
            Err(broadcast::error::RecvError::Lagged(_)) => {
                self.ended = true;
                Some(backpressure_item())
            }
            Err(broadcast::error::RecvError::Closed) => {
                self.ended = true;
                if let Some(item) = self.drain_signal() {
                    return Some(item);
                }
                Some(offline_item())
            }
        }
    }
}

#[async_trait]
impl RemoteStreamReceiver for RuntimeStreamReceiver {
    async fn next(&mut self) -> Option<RemoteStreamItem> {
        loop {
            if let Some(item) = self.buffered.pop_front() {
                return Some(item);
            }
            if self.ended {
                return None;
            }
            match self.phase {
                StreamPhase::AwaitingBarrier => {
                    let ready = self.barrier.borrow_and_update().clone();
                    if let Some(cursor) = ready {
                        self.open_barrier(cursor);
                        continue;
                    }
                    tokio::select! {
                        biased;
                        signal = self.signals.recv() => match signal {
                            Ok(signal) => {
                                self.ended = true;
                                return Some(signal_item(signal));
                            }
                            // 代际失效会丢弃发送端：事件通道随后也会关闭，下一轮即判离线。
                            Err(_) => continue,
                        },
                        changed = self.barrier.changed() => {
                            if changed.is_err() {
                                self.ended = true;
                                if let Some(item) = self.drain_signal() {
                                    return Some(item);
                                }
                                return Some(offline_item());
                            }
                        }
                        event = self.events.recv() => {
                            if let Some(item) = self.event_item(event) {
                                return Some(item);
                            }
                        }
                    }
                }
                StreamPhase::Live => {
                    tokio::select! {
                        biased;
                        signal = self.signals.recv() => match signal {
                            Ok(signal) => {
                                self.ended = true;
                                return Some(signal_item(signal));
                            }
                            Err(_) => continue,
                        },
                        event = self.events.recv() => {
                            if let Some(item) = self.event_item(event) {
                                return Some(item);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 运行期来源通知的接收端（`sync.remote_catalog_changed`/`remote_source_state` 的来源）。
struct RuntimeNoticeReceiver {
    receiver: broadcast::Receiver<RemoteSourceNotice>,
}

#[async_trait]
impl RemoteNoticeReceiver for RuntimeNoticeReceiver {
    async fn next(&mut self) -> Option<RemoteSourceNotice> {
        match self.receiver.recv().await {
            Ok(notice) => Some(notice),
            // 溢出：调用方重建接收端并从目录重新取一次事实，不在丢失的窗口上继续。
            Err(broadcast::error::RecvError::Lagged(_)) => None,
            Err(broadcast::error::RecvError::Closed) => None,
        }
    }
}

#[async_trait]
impl RemoteEventStream for RemoteRuntime {
    async fn subscribe(
        &self,
        session: &RemoteSessionRef,
        generation: RemoteGeneration,
        after: Option<OriginCursor>,
    ) -> Result<Box<dyn RemoteStreamReceiver>, PortError> {
        if self.current_generation(&session.owner_node_id) != Some(generation) {
            return Err(PortError::Unavailable(UnavailableKind::OwnerOffline));
        }
        let cursor = after.as_ref().map(wire_cursor).transpose()?;
        // 复用既有运行期订阅（attach + `resource.subscribe`）：本端口只做适配，不另造传输路径。
        let events = RemoteRuntime::subscribe(self, session, cursor)
            .await
            .map_err(|error| error.to_port_error())?;
        let (signals, barrier) = {
            let mut owners = lock(&self.owners);
            let state = owners
                .get_mut(session.owner_node_id.as_str())
                .ok_or(PortError::Unavailable(UnavailableKind::OwnerOffline))?;
            let key = session.session_id.as_str().to_owned();
            let signals = state
                .signals
                .entry(key.clone())
                .or_insert_with(|| broadcast::channel(SIGNAL_CAPACITY).0)
                .subscribe();
            let barrier = state
                .barriers
                .entry(key)
                .or_insert_with(|| watch::channel(None).0)
                .subscribe();
            (signals, barrier)
        };
        Ok(Box::new(RuntimeStreamReceiver {
            generation,
            events,
            signals,
            barrier,
            phase: StreamPhase::AwaitingBarrier,
            buffered: std::collections::VecDeque::new(),
            ended: false,
        }))
    }

    async fn unsubscribe(&self, session: &RemoteSessionRef, generation: RemoteGeneration) {
        if self.current_generation(&session.owner_node_id) != Some(generation) {
            return;
        }
        RemoteRuntime::unsubscribe(self, session);
    }

    async fn notices(&self) -> Result<Box<dyn RemoteNoticeReceiver>, PortError> {
        Ok(Box::new(RuntimeNoticeReceiver {
            receiver: self.notices.subscribe(),
        }))
    }
}

/// 一条运行期事件 → 流项；payload 无法投影时返回 `None`（调用方跳过该条并记录）。
fn stream_item(event: RuntimeEvent) -> Option<RemoteStreamItem> {
    let payload = core_payload(&event.payload)?;
    Some(RemoteStreamItem::Event(RemoteStreamEvent {
        owner_node_id: event.owner_node_id,
        session_id: event.session_id,
        origin_event_id: event.origin_event_id,
        origin_epoch: event.origin_epoch,
        origin_sequence: event.origin_sequence,
        event_type: event.event_type,
        payload_digest: event.payload_digest,
        created_at: event.created_at,
        payload,
    }))
}

/// 终止信号 → 流项（`Reset` 的 cursor 只在 Owner 给出有效 head 时非空）。
fn signal_item(signal: RuntimeSignal) -> RemoteStreamItem {
    match signal {
        RuntimeSignal::Reset { reason, cursor } => RemoteStreamItem::Reset {
            reason: match reason {
                ResetReason::EpochMismatch => RemoteStreamResetReason::EpochMismatch,
                ResetReason::CursorExpired => RemoteStreamResetReason::CursorExpired,
                ResetReason::CacheIncompatible => RemoteStreamResetReason::CacheIncompatible,
            },
            cursor,
        },
        RuntimeSignal::Unavailable { error } => RemoteStreamItem::Unavailable(error),
    }
}

/// 慢消费者/缓冲溢出的显式处置（`resource.backpressure`，可重试；客户端必须重建）。
fn backpressure_item() -> RemoteStreamItem {
    RemoteStreamItem::Unavailable(backpressure_error())
}

/// 缓冲溢出的结构化错误（`resource.backpressure`，可重试；客户端必须重建）。
fn backpressure_error() -> acp_core::model::PublicError {
    acp_core::model::PublicError::coded(
        "resource.backpressure",
        "the runtime event buffer overflowed; the subscription must be rebuilt",
        true,
    )
    .expect("the backpressure code is valid")
}

/// 连接断开/撤销/代际失效的显式处置（`resource.remote_unavailable`）。
fn offline_item() -> RemoteStreamItem {
    RemoteStreamItem::Unavailable(
        acp_core::model::PublicError::coded(
            "resource.remote_unavailable",
            "the source is no longer available on this connection",
            true,
        )
        .expect("the offline code is valid"),
    )
}

/// wire payload → core payload：`view` 与 `acp` 原文逐字节保留（不重新序列化、不重算摘要）。
fn core_payload(payload: &WireEventPayload) -> Option<EventPayload> {
    let view = payload.view()?;
    let view = ViewJson::new(view.get()).ok()?;
    let acp = match payload.acp() {
        Some(node_link_protocol::common::RawAcp::Complete {
            raw_json,
            byte_length,
            sha256,
        }) => {
            // `rawJson` 在 wire 上是 JSON **字符串字面量**（内容是 ACP 原文档文本）：取出字面量里的
            // 原文，不做任何解析或重新序列化。
            let text: String = serde_json::from_str(raw_json.get()).ok()?;
            let sha256 = Digest::new(&base64url(sha256.as_bytes())).ok()?;
            if byte_length.as_str().parse::<usize>().ok()? != text.len() {
                return None;
            }
            Some(AcpRaw::Available {
                media_type: "application/json".to_owned(),
                raw_json: text,
                byte_length: byte_length.as_str().parse().ok()?,
                sha256,
            })
        }
        Some(node_link_protocol::common::RawAcp::Unavailable {
            reason,
            byte_length,
            sha256,
        }) => Some(AcpRaw::Unavailable {
            reason: match reason {
                acpr_wire::RawAcpUnavailableReason::SizeLimit => RawUnavailableReason::SizeLimit,
                acpr_wire::RawAcpUnavailableReason::RetentionExpired => {
                    RawUnavailableReason::RetentionExpired
                }
                acpr_wire::RawAcpUnavailableReason::StorageFailure => {
                    RawUnavailableReason::StorageFailure
                }
            },
            byte_length: byte_length.as_str().parse().ok()?,
            sha256: sha256
                .as_ref()
                .and_then(|value| Digest::new(&base64url(value.as_bytes())).ok()),
        }),
        None => None,
    };
    Some(EventPayload::new(view, acp))
}

// ---------------------------------------------------------------------------------------------
// 映射与校验
// ---------------------------------------------------------------------------------------------

fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn core_state(state: WireSessionState) -> SessionState {
    match state {
        WireSessionState::Idle => SessionState::Idle,
        WireSessionState::Running => SessionState::Running,
        WireSessionState::Pending => SessionState::Pending,
    }
}

/// `payloadDigest` = `base64url(SHA-256(ACPR-CJ1(payload)))`（与 Owner 侧同一前像）。
fn payload_digest(payload: &WireEventPayload) -> Option<[u8; 32]> {
    let text = serde_json::to_string(payload).ok()?;
    let canonical = acpr_wire::cj1::canonicalize(&text).ok()?;
    Some(Sha256::digest(canonical.as_bytes()).into())
}

fn base64url(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn wire_cursor(cursor: &OriginCursor) -> Result<WireOriginCursor, PortError> {
    Ok(WireOriginCursor {
        origin_epoch: Uuid::parse(cursor.origin_epoch.as_str())
            .map_err(|_| PortError::InvalidRequest("the origin epoch is not a uuid"))?,
        origin_sequence: DecimalString::parse(&cursor.origin_sequence.get().to_string())
            .map_err(|_| PortError::InvalidRequest("the origin sequence is not encodable"))?,
    })
}

fn core_cursor(cursor: &WireOriginCursor) -> Result<OriginCursor, PortError> {
    Ok(OriginCursor::new(
        OriginEpoch::new(cursor.origin_epoch.as_str())
            .map_err(|_| PortError::Corrupt("the cursor epoch is invalid"))?,
        Sequence::new(
            cursor
                .origin_sequence
                .as_str()
                .parse::<u64>()
                .map_err(|_| PortError::Corrupt("the cursor sequence is invalid"))?,
        )
        .map_err(|_| PortError::Corrupt("the cursor sequence is out of range"))?,
    ))
}

fn parse_uuid(text: &str) -> Result<Uuid, PortError> {
    Uuid::parse(text).map_err(|_| PortError::InvalidRequest("the id is not a uuid"))
}

/// `command.terminal.terminalEventId` → core 的事件标识（`None` = 该终态没有终态事件，如实保留）。
///
/// 它是 **Owner** 的事件 id：Access 只透传，不用它伪造本地事件或落盘索引。
fn terminal_event_id(
    terminal: &node_link_protocol::command::Terminal,
) -> Result<Option<EventId>, PortError> {
    match terminal.terminal_event_id.as_ref() {
        Some(event) => EventId::new(event.as_str())
            .map(Some)
            .map_err(|_| PortError::Corrupt("command.terminal carries an invalid terminalEventId")),
        None => Ok(None),
    }
}

/// `session.title.set` 的 Owner 结果 → [`RemoteCommandRecord`]（`design.md` D3）。
///
/// 与 `command.status` 的投影只差一处：这里接受 `Rejected` 作为**错误**——未接受的命令没有业务记录，
/// 不能被投影成一条「完成过的」记录。
fn title_record(
    session: &RemoteSessionRef,
    request: &RequestId,
    outcome: CommandOutcome,
) -> Result<RemoteCommandRecord, PortError> {
    let target = RemoteCommandTarget {
        owner_node_id: session.owner_node_id.clone(),
        session_id: Some(session.session_id.clone()),
    };
    let build = |status: CommandStatus,
                 accepted_at: Option<Timestamp>,
                 terminal_at: Option<Timestamp>,
                 terminal_event_id: Option<EventId>,
                 result: Option<CommandResult>,
                 error: Option<acp_core::model::PublicError>| RemoteCommandRecord {
        target: target.clone(),
        request: request.clone(),
        status,
        accepted_at,
        terminal_at,
        terminal_event_id,
        result,
        error,
    };
    match outcome {
        CommandOutcome::Accepted {
            accepted_at,
            result,
        } => {
            let accepted_at = Timestamp::new(accepted_at.as_str())
                .map_err(|_| PortError::Corrupt("command.accepted carries an invalid timestamp"))?;
            Ok(build(
                CommandStatus::Accepted,
                Some(accepted_at),
                None,
                None,
                result.as_ref().and_then(object_result),
                None,
            ))
        }
        CommandOutcome::Terminal(terminal) => {
            let terminal_at = Timestamp::new(terminal.terminal_at.as_str())
                .map_err(|_| PortError::Corrupt("command.terminal carries an invalid timestamp"))?;
            let status = match terminal.status {
                TerminalStatus::Completed => CommandStatus::Completed,
                TerminalStatus::Failed => CommandStatus::Failed,
                TerminalStatus::Uncertain => CommandStatus::Uncertain,
                // 终态 `rejected` = 从未被接受：没有可投影的记录。
                TerminalStatus::Rejected => return Err(title_rejection(terminal.error.as_ref())),
            };
            Ok(build(
                status,
                // Node Link 的 `command.terminal` 不携带接受时间：不拿本地时钟冒充 Owner 的时间。
                None,
                Some(terminal_at),
                terminal_event_id(&terminal)?,
                terminal.result.as_ref().and_then(object_result),
                terminal.error.as_ref().map(core_public_error),
            ))
        }
        CommandOutcome::Rejected(error) => Err(title_rejection(Some(&error))),
    }
}

/// Owner 的拒绝 → 端口错误。能力类拒绝是**永久**失败（`capability.unsupported_by_broker`），
/// 其余保留结构化的 Owner 错误（`Backend`，不伪装成本地参数错误）。
fn title_rejection(error: Option<&node_link_protocol::common::PublicError>) -> PortError {
    let Some(error) = error else {
        return PortError::Backend(Box::new(std::io::Error::other(
            "the owner rejected the session title command",
        )));
    };
    if matches!(
        error.code.as_str(),
        "nodelink.protocol.feature_required" | "nodelink.command.unsupported"
    ) {
        return PortError::InvalidRequest("capability.unsupported_by_broker");
    }
    // 同一 requestId 换参是命令级冲突，不是本机故障：保留为可映射的 `Conflict`，让 Sync 客户端
    // 看到 `command.idempotency_conflict`（而不是可重试的 `internal.unavailable`）。
    if error.code.as_str() == "nodelink.command.idempotency_conflict" {
        return PortError::Conflict(ConflictKind::IdempotencyConflict);
    }
    ClientError::Rejected {
        code: error.code.as_str().to_owned(),
        retryable: error.retryable,
        error: core_public_error(error),
    }
    .to_port_error()
}

fn map_command_outcome(
    target: &RemoteCommandTarget,
    request: &RequestId,
    outcome: CommandOutcome,
) -> Result<RemoteCommandLookup, PortError> {
    let record =
        |status: CommandStatus,
         accepted_at: Option<Timestamp>,
         terminal_at: Option<Timestamp>,
         terminal_event_id: Option<EventId>,
         result: Option<CommandResult>,
         error: Option<acp_core::model::PublicError>| RemoteCommandRecord {
            target: target.clone(),
            request: request.clone(),
            status,
            accepted_at,
            terminal_at,
            terminal_event_id,
            result,
            error,
        };
    match outcome {
        CommandOutcome::Accepted {
            accepted_at,
            result,
        } => {
            let accepted_at = Timestamp::new(accepted_at.as_str())
                .map_err(|_| PortError::Corrupt("command.accepted carries an invalid timestamp"))?;
            let result = result.as_ref().and_then(object_result);
            Ok(RemoteCommandLookup::Accepted(record(
                CommandStatus::Accepted,
                Some(accepted_at),
                None,
                None,
                result,
                None,
            )))
        }
        CommandOutcome::Terminal(terminal) => {
            let terminal_at = Timestamp::new(terminal.terminal_at.as_str())
                .map_err(|_| PortError::Corrupt("command.terminal carries an invalid timestamp"))?;
            let error = terminal.error.as_ref().map(core_public_error);
            let result = terminal.result.as_ref().and_then(object_result);
            let event_id = terminal_event_id(&terminal)?;
            let status = match terminal.status {
                TerminalStatus::Completed => CommandStatus::Completed,
                TerminalStatus::Failed => CommandStatus::Failed,
                TerminalStatus::Rejected => return Ok(RemoteCommandLookup::NotFound),
                TerminalStatus::Uncertain => CommandStatus::Uncertain,
            };
            let record = record(status, None, Some(terminal_at), event_id, result, error);
            Ok(if status == CommandStatus::Uncertain {
                RemoteCommandLookup::Uncertain(record)
            } else {
                RemoteCommandLookup::Terminal(record)
            })
        }
        CommandOutcome::Rejected(error) => {
            if error.code.as_str() == "nodelink.command.not_found" {
                Ok(RemoteCommandLookup::NotFound)
            } else {
                Err(ClientError::Rejected {
                    code: error.code.as_str().to_owned(),
                    retryable: error.retryable,
                    error: core_public_error(&error),
                }
                .to_port_error())
            }
        }
    }
}

fn object_result(payload: &CommandResultPayload) -> Option<CommandResult> {
    match payload {
        CommandResultPayload::Object(object) => CommandResult::from_json_text(object.get()).ok(),
        _ => None,
    }
}

fn core_public_error(
    error: &node_link_protocol::common::PublicError,
) -> acp_core::model::PublicError {
    let details = ViewJson::new(error.details.get()).unwrap_or_else(|_| ViewJson::empty_object());
    acp_core::model::PublicError::try_new(
        error.code.as_str(),
        error.message.as_str(),
        error.retryable,
        details,
    )
    .unwrap_or_else(|_| {
        acp_core::model::PublicError::coded(
            "internal.unavailable",
            "the owner reported an invalid error",
            false,
        )
        .expect("the fallback error code is valid")
    })
}

/// `session.read` 结果的运行期投影（`resource.stateless-origin.v1` 的三个加性字段必需）。
fn map_read_document(
    expected: &RemoteSessionRef,
    document: &RawObject,
) -> Result<RemoteReadResult, PortError> {
    let value: serde_json::Value = serde_json::from_str(document.get())
        .map_err(|_| PortError::Corrupt("the owner session.read result is malformed"))?;
    let cursor = value
        .get("readCursor")
        .ok_or(PortError::Unavailable(UnavailableKind::BackendUnsupported))?;
    let read_cursor = json_cursor(cursor)?;
    let summary = value
        .get("sessionSummary")
        .ok_or(PortError::Unavailable(UnavailableKind::BackendUnsupported))?;
    let summary = json_summary(expected, summary, &read_cursor)?;
    let mut messages = Vec::new();
    if let Some(items) = value
        .get("resources")
        .and_then(|resources| resources.get("messages"))
        .and_then(|items| items.as_array())
    {
        for item in items {
            messages.push(json_message(item)?);
        }
    }
    let mut history = Vec::new();
    if let Some(items) = value
        .get("resources")
        .and_then(|resources| resources.get("history"))
        .and_then(|items| items.as_array())
    {
        for item in items {
            history.push(json_history(expected, &read_cursor, item)?);
        }
    }
    let mut coverage = Vec::new();
    if let Some(items) = value
        .get("messageCoverage")
        .and_then(|items| items.as_array())
    {
        for item in items {
            coverage.push(json_coverage(item)?);
        }
    }
    // 页标记是 Owner 原值：`hasEarlier` 是 `session.read` 结果的必填键，`historyHasEarlier` 只在请求了
    // 历史时出现（缺席 = 本次没请求历史，按 `false` 记）。本层不重新推断页边界。
    let messages_has_earlier = value
        .get("hasEarlier")
        .and_then(|flag| flag.as_bool())
        .ok_or(PortError::Corrupt(
            "the owner session.read result has no hasEarlier",
        ));
    let history_has_earlier = value
        .get("historyHasEarlier")
        .and_then(|flag| flag.as_bool())
        .unwrap_or(false);
    Ok(RemoteReadResult {
        summary,
        read_cursor,
        messages,
        history,
        coverage,
        messages_has_earlier: messages_has_earlier?,
        history_has_earlier,
    })
}

fn json_cursor(value: &serde_json::Value) -> Result<OriginCursor, PortError> {
    let epoch = value
        .get("originEpoch")
        .and_then(|value| value.as_str())
        .ok_or(PortError::Corrupt("the cursor epoch is missing"))?;
    let sequence = value
        .get("originSequence")
        .and_then(|value| value.as_str())
        .ok_or(PortError::Corrupt("the cursor sequence is missing"))?;
    Ok(OriginCursor::new(
        OriginEpoch::new(epoch).map_err(|_| PortError::Corrupt("the cursor epoch is invalid"))?,
        Sequence::new(
            sequence
                .parse::<u64>()
                .map_err(|_| PortError::Corrupt("the cursor sequence is invalid"))?,
        )
        .map_err(|_| PortError::Corrupt("the cursor sequence is out of range"))?,
    ))
}

fn json_summary(
    expected: &RemoteSessionRef,
    value: &serde_json::Value,
    read_cursor: &OriginCursor,
) -> Result<SessionSummary, PortError> {
    let session_id = value
        .get("sessionId")
        .and_then(|value| value.as_str())
        .ok_or(PortError::Corrupt("the summary session id is missing"))?;
    let session_id = SessionId::new(session_id)
        .map_err(|_| PortError::Corrupt("the summary session id is invalid"))?;
    if session_id != expected.session_id {
        return Err(PortError::Corrupt("the summary belongs to another session"));
    }
    let origin = value
        .get("origin")
        .ok_or(PortError::Corrupt("the summary origin is missing"))?;
    if origin.get("kind").and_then(|kind| kind.as_str()) != Some("remote") {
        return Err(PortError::Corrupt("the summary is not a remote origin"));
    }
    if origin.get("ownerNodeId").and_then(|id| id.as_str()) != Some(expected.owner_node_id.as_str())
    {
        return Err(PortError::Corrupt("the summary belongs to another owner"));
    }
    let agent = value
        .get("agent")
        .ok_or(PortError::Corrupt("the summary agent is missing"))?;
    let agent = AgentRef::try_new(
        AgentId::new(
            agent
                .get("agentId")
                .and_then(|id| id.as_str())
                .ok_or(PortError::Corrupt("the summary agent id is missing"))?,
        )
        .map_err(|_| PortError::Corrupt("the summary agent id is invalid"))?,
        agent
            .get("name")
            .and_then(|name| name.as_str())
            .ok_or(PortError::Corrupt("the summary agent name is missing"))?,
    )
    .map_err(|_| PortError::Corrupt("the summary agent is invalid"))?;
    let state = match value.get("state").and_then(|state| state.as_str()) {
        Some("idle") => SessionState::Idle,
        Some("running") => SessionState::Running,
        Some("pending") => SessionState::Pending,
        _ => return Err(PortError::Corrupt("the summary state is invalid")),
    };
    let version = value
        .get("version")
        .and_then(|version| version.as_str())
        .ok_or(PortError::Corrupt("the summary version is missing"))?
        .parse::<u64>()
        .map_err(|_| PortError::Corrupt("the summary version is invalid"))?;
    let created_at = value
        .get("createdAt")
        .and_then(|value| value.as_str())
        .ok_or(PortError::Corrupt("the summary createdAt is missing"))?;
    let updated_at = value
        .get("updatedAt")
        .and_then(|value| value.as_str())
        .ok_or(PortError::Corrupt("the summary updatedAt is missing"))?;
    let title = value
        .get("title")
        .and_then(|title| title.as_str())
        .map(str::to_owned);
    let current_mode = value.get("currentMode").and_then(|mode| {
        let id = ModeId::new(mode.get("modeId")?.as_str()?).ok()?;
        ModeRef::try_new(id, mode.get("name")?.as_str()?).ok()
    });
    SessionSummary::try_new(
        session_id,
        title,
        agent,
        state,
        ResourceOrigin::Remote {
            owner_node_id: expected.owner_node_id.clone(),
            origin_epoch: read_cursor.origin_epoch.clone(),
            online: true,
        },
        current_mode,
        Version::new(version),
        Timestamp::new(created_at)
            .map_err(|_| PortError::Corrupt("the summary createdAt is invalid"))?,
        Timestamp::new(updated_at)
            .map_err(|_| PortError::Corrupt("the summary updatedAt is invalid"))?,
        None,
    )
    .map_err(|_| PortError::Corrupt("the summary is not representable"))
}

fn json_message(value: &serde_json::Value) -> Result<MessageRecord, PortError> {
    let message_id = value
        .get("messageId")
        .and_then(|value| value.as_str())
        .ok_or(PortError::Corrupt("a message id is missing"))?;
    let session = value
        .get("sessionId")
        .and_then(|value| value.as_str())
        .ok_or(PortError::Corrupt("a message session id is missing"))?;
    let turn = value
        .get("turnId")
        .and_then(|value| value.as_str())
        .and_then(|id| TurnId::new(id).ok());
    // 角色来自 wire 的 `role`，**不是** `channel` 的缺省值：`role=user` 的导入用户消息没有 `channel`
    // （用户消息在 messages 里按 `role=user` 投影、`channel` 键缺席），旧实现会把它当成 Agent 回复。
    // Agent 消息未协商 `core.typed-media.v1` 时 `channel` 缺席，此时才是 Reply。
    let role = value
        .get("role")
        .and_then(|value| value.as_str())
        .ok_or(PortError::Corrupt("a message role is missing"))?;
    let channel = match role {
        "user" => MessageChannel::User,
        "agent" => match value.get("channel").and_then(|value| value.as_str()) {
            Some("thought") => MessageChannel::Thought,
            Some("reply") | None => MessageChannel::Reply,
            Some(_) => return Err(PortError::Corrupt("a message channel is invalid")),
        },
        _ => return Err(PortError::Corrupt("a message role is invalid")),
    };
    let status = value
        .get("status")
        .and_then(|value| value.as_str())
        .ok_or(PortError::Corrupt("a message status is missing"))?;
    let state = MessageState::from_str(status)
        .map_err(|_| PortError::Corrupt("a message status is invalid"))?;
    let reason = value
        .get("partial")
        .and_then(|partial| partial.get("reason"))
        .and_then(|value| value.as_str())
        .map(PartialReason::from_str)
        .transpose()
        .map_err(|_| PortError::Corrupt("a message partial reason is invalid"))?;
    let result = PartialResult::try_new(state, reason)
        .map_err(|_| PortError::Corrupt("a message partial result is invalid"))?;
    let content = value
        .get("content")
        .ok_or(PortError::Corrupt("a message content is missing"))?;
    let content = JsonValueText::new(&content.to_string())
        .map_err(|_| PortError::Corrupt("a message content is invalid"))?;
    let created_at = value
        .get("createdAt")
        .and_then(|value| value.as_str())
        .ok_or(PortError::Corrupt("a message createdAt is missing"))?;
    let created_at =
        Timestamp::new(created_at).map_err(|_| PortError::Corrupt("a message time is invalid"))?;
    let mut record = MessageRecord::try_new(
        MessageId::new(message_id).map_err(|_| PortError::Corrupt("a message id is invalid"))?,
        SessionId::new(session)
            .map_err(|_| PortError::Corrupt("a message session id is invalid"))?,
        turn,
        channel,
        result,
        content,
        Vec::new(),
        created_at.clone(),
        created_at,
    )
    .map_err(|_| PortError::Corrupt("a message is not representable"))?;
    record.first_origin_sequence = match value.get("sequence") {
        None => None,
        Some(value) => {
            let text = value
                .as_str()
                .ok_or(PortError::Corrupt("a message sequence is invalid"))?;
            if text.is_empty()
                || (text.len() > 1 && text.starts_with('0'))
                || !text.bytes().all(|byte| byte.is_ascii_digit())
            {
                return Err(PortError::Corrupt("a message sequence is invalid"));
            }
            Some(
                Sequence::new(
                    text.parse()
                        .map_err(|_| PortError::Corrupt("a message sequence is invalid"))?,
                )
                .map_err(|_| PortError::Corrupt("a message sequence is invalid"))?,
            )
        }
    };
    Ok(record)
}

fn json_history(
    expected: &RemoteSessionRef,
    read_cursor: &OriginCursor,
    value: &serde_json::Value,
) -> Result<RemoteHistoryRecord, PortError> {
    let event_id = value
        .get("eventId")
        .and_then(|value| value.as_str())
        .ok_or(PortError::Corrupt("a history event id is missing"))?;
    let event_type = value
        .get("eventType")
        .and_then(|value| value.as_str())
        .ok_or(PortError::Corrupt("a history event type is missing"))?;
    let created_at = value
        .get("createdAt")
        .and_then(|value| value.as_str())
        .ok_or(PortError::Corrupt("a history time is missing"))?;
    // `sessionSequence` 在 Owner 的 `session.read` 结果里就是这条事件的来源会话内序号（分页键）；
    // 原样保留，不在这里重编号。
    let origin_sequence = value
        .get("sessionSequence")
        .and_then(|value| value.as_str())
        .ok_or(PortError::Corrupt("a history sequence is missing"))?
        .parse::<u64>()
        .map_err(|_| PortError::Corrupt("a history sequence is not decimal"))?;
    let origin_sequence = Sequence::new(origin_sequence)
        .map_err(|_| PortError::Corrupt("a history sequence is out of range"))?;
    let view = value
        .get("view")
        .ok_or(PortError::Corrupt("a history view is missing"))?;
    let view = ViewJson::new(&view.to_string())
        .map_err(|_| PortError::Corrupt("a history view is invalid"))?;
    Ok(RemoteHistoryRecord {
        origin: OriginEventRef::new(
            expected.owner_node_id.clone(),
            read_cursor.origin_epoch.clone(),
            EventId::new(event_id).map_err(|_| PortError::Corrupt("a history id is invalid"))?,
        ),
        origin_sequence,
        event_type: EventType::new(event_type)
            .map_err(|_| PortError::Corrupt("a history event type is invalid"))?,
        payload: EventPayload::new(view, None),
        created_at: Timestamp::new(created_at)
            .map_err(|_| PortError::Corrupt("a history time is invalid"))?,
    })
}

fn json_coverage(value: &serde_json::Value) -> Result<MessageCoverage, PortError> {
    let message_id = value
        .get("messageId")
        .and_then(|value| value.as_str())
        .ok_or(PortError::Corrupt("a coverage message id is missing"))?;
    let through = value
        .get("through")
        .ok_or(PortError::Corrupt("a coverage cursor is missing"))?;
    let last_delta_index = value
        .get("lastDeltaIndex")
        .and_then(|value| value.as_str())
        .map(|text| {
            text.parse::<u64>()
                .map_err(|_| PortError::Corrupt("a coverage delta index is invalid"))
        })
        .transpose()?;
    Ok(MessageCoverage {
        message_id: MessageId::new(message_id)
            .map_err(|_| PortError::Corrupt("a coverage message id is invalid"))?,
        through: json_cursor(through)?,
        last_delta_index,
    })
}

/// `resource.ack` 的 body（运行期处理进度；只在 `stateless-origin` 协商后发送）。
pub fn wire_ack(session: &WireRemoteSessionRef, cursor: &OriginCursor) -> Option<WireAck> {
    Some(WireAck {
        session_ref: session.clone(),
        cursor: wire_cursor(cursor).ok()?,
    })
}

/// 一个随机 UUID（`command.status` 的查询请求 id；每次调用都不同，不用于幂等键）。
pub fn random_request_uuid() -> Uuid {
    Uuid::parse(&RandomUuid::new_v4().to_string()).expect("v4 uuid is valid")
}

/// [`IncludeResource`] 的请求列表（`RemoteSourceReads` 的 include 映射）。
pub fn include_list(include: &HistoryInclude) -> Result<Vec<IncludeResource>, PortError> {
    let mut items = Vec::new();
    if include.messages {
        items.push(IncludeResource::Messages);
    }
    if include.pending_interactions {
        items.push(IncludeResource::PendingInteractions);
    }
    if include.config_options {
        items.push(IncludeResource::ConfigOptions);
    }
    if include.history {
        items.push(IncludeResource::History);
    }
    if include.turns || include.capabilities {
        return Err(PortError::Unavailable(UnavailableKind::BackendUnsupported));
    }
    if items.is_empty() {
        return Err(PortError::InvalidRequest(
            "session.read requires at least one include",
        ));
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Owner 的幂等冲突必须保留为 `Conflict`，sync 层才能回 `command.idempotency_conflict`；
    /// 折成 `Backend` 会变成可重试的 `internal.unavailable`，客户端会错误地重试。
    #[test]
    fn owner_idempotency_conflict_keeps_its_identity() {
        let error: node_link_protocol::common::PublicError =
            serde_json::from_value(serde_json::json!({
                "code": "nodelink.command.idempotency_conflict",
                "message": "the requestId was used with a different payload",
                "retryable": false,
                "details": {}
            }))
            .expect("public error");
        assert!(matches!(
            title_rejection(Some(&error)),
            PortError::Conflict(ConflictKind::IdempotencyConflict)
        ));
    }

    /// `conversation-reliability` §2.2：imported 用户消息按 `role=user` 读取，**不**因 `channel`
    /// 缺席（未协商 typed-media）退化成 Agent 回复；非法的 role 仍失败关闭。
    #[test]
    fn imported_user_message_without_channel_reads_as_user() {
        let user = serde_json::json!({
            "messageId": "11111111-1111-4111-8111-111111111111",
            "sessionId": "22222222-2222-4222-8222-222222222222",
            "role": "user",
            "status": "completed",
            "content": [{"type": "text", "text": "继续"}],
            "createdAt": "2026-09-18T00:00:00.000Z"
        });
        let record = json_message(&user).expect("user message");
        assert_eq!(record.channel, MessageChannel::User);

        // Agent 消息在未协商 typed-media 时本来就缺 `channel`：默认 reply，不改成 user。
        let reply = serde_json::json!({
            "messageId": "11111111-1111-4111-8111-111111111112",
            "sessionId": "22222222-2222-4222-8222-222222222222",
            "role": "agent",
            "status": "completed",
            "content": [{"type": "text", "text": "好的"}],
            "createdAt": "2026-09-18T00:00:00.000Z"
        });
        let record = json_message(&reply).expect("agent message");
        assert_eq!(record.channel, MessageChannel::Reply);

        let invalid = serde_json::json!({
            "messageId": "11111111-1111-4111-8111-111111111113",
            "sessionId": "22222222-2222-4222-8222-222222222222",
            "role": "system",
            "status": "completed",
            "content": [],
            "createdAt": "2026-09-18T00:00:00.000Z"
        });
        assert!(matches!(json_message(&invalid), Err(PortError::Corrupt(_))));
    }
}
