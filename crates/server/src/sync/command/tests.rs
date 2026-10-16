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
    OwnedSessionRef, PortError, PromptRequest, RequestId, ResumeSessionRequest, SelectValue,
    Sequence, ServerEpoch, SessionRecoveryRecord, SessionSnapshot, SessionSummary, Timestamp,
    ViewJson,
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
    /// 逐会话的**持久快照**（`SessionStore::load` 的唯一读面；mutation 的提交前置读它）。
    ///
    /// 默认空：查询面零副作用用例因此仍然看不到任何会话（那正是它们要的）。
    /// mutation 的用例经 `push_session` 一并登记，于是 `submit_prompt` 的 `load` 前置能过。
    snapshots: BTreeMap<String, SessionSnapshot>,
}

/// 「会话列表 + 事件分页 + 命令记录」的存储替身，带逐次计数。
#[derive(Clone, Default)]
struct CommandStore {
    state: Arc<Mutex<StoreState>>,
    counters: Arc<Counters>,
}

/// 替身写下的终态事件 id（真实存储写一条 `command.*` 事件并把它的 id 记进命令行；终态回复的
/// `terminalEventId` 必须原样带出它）。
const STORE_TERMINAL_EVENT: &str = "9f3a0c21-7b4e-4d5f-8a6b-0c1d2e3f4a5b";

/// 终态提交里那条 `command.*` 事件的 `causation`（= 这条终态属于哪个请求）。
///
/// 真实存储用同一条线索定位要推进的 `owned_command` 行（终态提交只带事件与终态记录，不带 requestId），
/// 替身照抄它——不按「哪条记录还是 `accepted`」猜。
fn causation_request(commit: &OwnedCommit) -> Result<String, PortError> {
    commit
        .events
        .iter()
        .rev()
        .find(|event| event.event_type.as_str().starts_with("command."))
        .and_then(|event| event.causation.clone())
        .map(|request| request.as_str().to_owned())
        .ok_or(PortError::InvalidRequest("终态提交缺少 command.* 事件"))
}

impl CommandStore {
    fn set_head(&self, cursor: GlobalCursor) {
        self.lock().head = Some(cursor);
    }

    fn push_session(&self, summary: SessionSummary) {
        // 摘要与快照一并登记：`session.list` 读前者，`submit_prompt` 的 `load` 前置读后者。
        // 快照的状态取摘要自己的状态、版本固定 1（3.2 的版本冲突用例另用 `push_command` 等素材构造）。
        let id = summary.session_id().as_str().to_owned();
        let snapshot = SessionSnapshot {
            session: acp_core::model::Session::try_new(
                acp_core::model::SessionId::new(&id).expect("session id"),
                OwnedSessionRef::new(acp_core::model::SessionId::new(&id).expect("session id")),
                None,
                acp_core::model::AgentRef::try_new(
                    acp_core::model::AgentId::new("codex").expect("agent id"),
                    "Codex",
                )
                .expect("agent ref"),
                summary.state(),
                acp_core::model::ResourceOrigin::Local,
                None,
                acp_core::model::Version::new(0),
                acp_core::model::Timestamp::new("2026-09-17T12:10:00.123Z").expect("timestamp"),
                acp_core::model::Timestamp::new("2026-09-17T12:10:00.123Z").expect("timestamp"),
                None,
            )
            .expect("会话行合法"),
            origin_epoch: acp_core::model::OriginEpoch::new(EPOCH).expect("origin epoch"),
            head: GlobalCursor::new(
                acp_core::model::ServerEpoch::new(EPOCH).expect("server epoch"),
                acp_core::model::Sequence::new(1).expect("sequence"),
            ),
        };
        let mut state = self.lock();
        state.snapshots.insert(id, snapshot);
        state.sessions.push(summary);
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

    /// 把某条已落盘的命令推进到终态（替身：换掉那条记录，模拟 `settle_*` 与启动恢复写下的终态）。
    ///
    /// 只替换、不追加：`find_request` 按 requestId 找第一条，追加会读到旧的 `accepted` 行。
    fn settle_command(
        &self,
        request: &str,
        status: CommandStatus,
        terminal_event: &str,
        error: Option<acp_core::model::PublicError>,
    ) {
        let mut state = self.lock();
        for record in state.commands.iter_mut() {
            if record.request().as_str() != request {
                continue;
            }
            let next = CommandRecord::try_new(
                record.session().cloned(),
                record.request().clone(),
                record.command(),
                record.kind(),
                record.actor().clone(),
                record.accepted_at().cloned(),
                status,
                Some(timestamp("2026-09-18T09:12:04.000Z")),
                Some(EventId::new(terminal_event).expect("terminal event id")),
                None,
                error,
                record.expected_version(),
                record.request_fingerprint().clone(),
            )
            .expect("终态记录合法");
            *record = next;
            return;
        }
        panic!("没有可推进的记录：{request}");
    }

    /// 把某条已落盘的命令推进到终态（按 requestId 定位既有行；`session` 为 `Some` 时同时**回填**会话
    /// 标识——§6 第 20 条的落账提交里「回填」「建会话行」「写终态」是同一次提交的一部分）。
    fn settle_record(
        &self,
        request: &str,
        session: Option<acp_core::model::SessionId>,
        terminal: &acp_core::model::CommandTerminalRecord,
    ) -> Result<(), PortError> {
        let mut state = self.lock();
        let Some(index) = state
            .commands
            .iter()
            .position(|record| record.request().as_str() == request)
        else {
            return Err(PortError::InvalidRequest("终态提交找不到既有命令记录"));
        };
        let existing = state.commands[index].clone();
        let next = CommandRecord::try_new(
            session.or_else(|| existing.session().cloned()),
            existing.request().clone(),
            existing.command(),
            existing.kind(),
            existing.actor().clone(),
            existing.accepted_at().cloned(),
            terminal.status(),
            terminal.terminal_at().cloned(),
            Some(EventId::new(STORE_TERMINAL_EVENT).expect("terminal event id")),
            terminal.result().cloned(),
            terminal.error().cloned(),
            existing.expected_version(),
            existing.request_fingerprint().clone(),
        )?;
        state.commands[index] = next;
        Ok(())
    }

    /// 落账提交里的会话行（§6 第 20 条）：标识来自 core（存储层采用它），摘要与快照一起登记——与
    /// [`CommandWorld::push_session`] 同一读面口径，取值全部来自 `settled` 与提交时刻。
    fn push_settled_session(&self, settled: &acp_core::ports::SettledSession, at: &Timestamp) {
        let summary = SessionSummary::try_new(
            settled.session.clone(),
            settled.title.clone(),
            settled.agent.clone(),
            acp_core::model::SessionState::Idle,
            acp_core::model::ResourceOrigin::Local,
            None,
            acp_core::model::Version::from(1),
            at.clone(),
            at.clone(),
            None,
        )
        .expect("落账的会话摘要");
        let snapshot = SessionSnapshot {
            session: acp_core::model::Session::try_new(
                settled.session.clone(),
                OwnedSessionRef::new(settled.session.clone()),
                settled.title.clone(),
                settled.agent.clone(),
                acp_core::model::SessionState::Idle,
                acp_core::model::ResourceOrigin::Local,
                None,
                acp_core::model::Version::from(1),
                at.clone(),
                at.clone(),
                None,
            )
            .expect("落账的会话行"),
            origin_epoch: acp_core::model::OriginEpoch::new(EPOCH).expect("origin epoch"),
            head: GlobalCursor::new(
                acp_core::model::ServerEpoch::new(EPOCH).expect("server epoch"),
                acp_core::model::Sequence::new(1).expect("sequence"),
            ),
        };
        let mut state = self.lock();
        state
            .snapshots
            .insert(settled.session.as_str().to_owned(), snapshot);
        state.sessions.push(summary);
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
        session: &acp_core::model::SessionId,
    ) -> Result<Option<acp_core::model::SessionSnapshot>, PortError> {
        self.counters.load.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .state
            .lock()
            .expect("存储替身锁")
            .snapshots
            .get(session.as_str())
            .cloned())
    }

    /// §6 第 20 条的三笔时序（替身的 `commit` 本身是原子的，「同一次提交」由此成立）：
    ///
    /// - **受理提交**（`idempotency` 有、`state` 与 `command_terminal` 都为 `None`）只落幂等行：
    ///   `session_id` 为 `NULL`，**不**建会话行（会话标识由 core 在调用后端之前分配）；
    /// - **落账提交**（`StateChange::SettleCreate`）在**同一次**提交里建会话行（采用 core 已分配的标识）、
    ///   回填幂等行的 `session_id`、写终态；
    /// - **无会话终态**（`command_terminal` 有、`state` 没有：后端失败与窗口期超限）只终结既有幂等行，
    ///   不建会话行。
    ///
    /// 定位一条既有记录一律按 `command.*` 事件的 `causation`（真实存储读的是同一条线索），不按
    /// 「哪条记录还是 `accepted`」猜。
    async fn commit(&self, commit: OwnedCommit) -> Result<CommitOutcome, PortError> {
        self.counters.commit.fetch_add(1, Ordering::SeqCst);
        // 幂等命中（§6 第 6 条与第 20 条）：五项比对，`session = None` 不参与（会话标识由 core 分配、
        // 落账提交才回填）。同键同语义 ⇒ 回首次记录（core 据此回 `InFlight`/`Settled`，不派发第二次
        // 创建）；同键不同语义 ⇒ `IdempotencyConflict`。
        if let Some(idem) = commit.idempotency.as_ref()
            && let Some(existing) = self
                .lock()
                .commands
                .iter()
                .find(|record| record.request().as_str() == idem.request.as_str())
                .cloned()
        {
            let same = existing.command() == idem.command
                && existing.kind() == idem.kind
                && idem
                    .session
                    .as_ref()
                    .is_none_or(|session| existing.session() == Some(session))
                && existing.expected_version() == idem.expected_version
                && existing.request_fingerprint() == &idem.request_fingerprint;
            if !same {
                return Err(PortError::Conflict(
                    acp_core::model::ConflictKind::IdempotencyConflict,
                ));
            }
            return Ok(CommitOutcome {
                session_id: existing.session().cloned(),
                origin_epoch: None,
                version: acp_core::model::Version::from(0),
                appended: Vec::new(),
                replayed: Some(acp_core::ports::IdempotentReplay { record: existing }),
            });
        }
        let mut session_id = None;
        if let Some(record) = commit.idempotency.clone() {
            let stored = acp_core::model::CommandRecord::try_new(
                record.session.clone(),
                record.request.clone(),
                &record.command,
                record.kind,
                record.actor.clone(),
                Some(record.accepted_at.clone()),
                acp_core::model::CommandStatus::Accepted,
                None,
                None,
                None,
                None,
                record.expected_version,
                record.request_fingerprint.clone(),
            )
            .expect("幂等行合法");
            self.lock().commands.push(stored);
        }
        match &commit.state {
            Some(acp_core::ports::StateChange::SettleCreate(settled)) => {
                self.push_settled_session(settled, &commit.at);
                let terminal = commit
                    .command_terminal
                    .clone()
                    .ok_or(PortError::InvalidRequest("落账提交缺少 command_terminal"))?;
                self.settle_record(
                    &causation_request(&commit)?,
                    Some(settled.session.clone()),
                    &terminal,
                )?;
                session_id = Some(settled.session.clone());
            }
            Some(acp_core::ports::StateChange::Update(_)) => {
                // 已有会话的状态修改：替身只按同一批事件里的终态推进命令记录（`session.prompt` 的轮次
                // 结束），会话行本身的版本推进不在这里建模（用例直接种记录）。
                if let Some(terminal) = commit.command_terminal.clone() {
                    self.settle_record(&causation_request(&commit)?, None, &terminal)?;
                }
            }
            Some(acp_core::ports::StateChange::Create(_)) => {
                // `StateChange::Create` 只有**不经 core 用例**的提交方使用（`session.create` 的落账走
                // `SettleCreate`）：命令面没有这样的调用点，替身因此显式拒绝而不是静默接受。
                return Err(PortError::InvalidRequest(
                    "命令面不经 StateChange::Create 建会话",
                ));
            }
            None => {
                if let Some(terminal) = commit.command_terminal.clone() {
                    self.settle_record(&causation_request(&commit)?, None, &terminal)?;
                }
            }
        }
        Ok(CommitOutcome {
            session_id,
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
///
/// `created` 为 `Some` 时该端点属于 `SessionBackendFactory::create` 刚建出来的那个会话（`reference`
/// 因此指向它），查询面用例构造的端点取 `None`（引用固定的 [`SESSION`]）。
struct ConfigEndpoint {
    options: Vec<CoreConfigOption>,
    modes: CoreModeState,
    /// `prompt` 被调用几次：幂等用例的判据是「同一 requestId 只派发一次」。
    prompt_calls: Arc<std::sync::atomic::AtomicU64>,
    /// `create` 分配的会话（`None` = 查询面用例的端点）。
    created: Option<acp_core::model::SessionId>,
}

#[async_trait::async_trait]
impl SessionEndpoint for ConfigEndpoint {
    fn reference(&self) -> acp_core::model::SessionReference {
        let session = self
            .created
            .clone()
            .unwrap_or_else(|| acp_core::model::SessionId::new(SESSION).expect("session id"));
        acp_core::model::SessionReference::Owned(acp_core::model::OwnedSessionRef {
            session_id: session,
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
        // mutation 的接受链路真的会派发到后端：回一个确定的新 turn，让「已接受 + 带 turn」可断言。
        // 这不是"未触及"—— prompt 路径的终点就在这里，WP3 的 3.1 要的正是这一跳。
        self.prompt_calls.fetch_add(1, Ordering::SeqCst);
        Ok(TurnAccepted {
            turn: acp_core::model::TurnId::new(TEST_PROMPT_TURN).expect("turn id"),
        })
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
///
/// `create` 是 `session.create` 的唯一副作用点：`create_calls` 是「同一 requestId 不派发第二次创建」的
/// 判据，`create_step` 是失败类别的注入点。
struct ConfigBackend {
    endpoint: Arc<ConfigEndpoint>,
    sessions: Arc<Mutex<Vec<String>>>,
    /// `create` 被派发几次。
    create_calls: Arc<AtomicU64>,
    /// 下一次 `create` 的行为（`take` 语义：消费后回到正常建立）。
    create_step: Arc<Mutex<Option<CreateStep>>>,
}

/// 下一次 `create` 的行为。
enum CreateStep {
    /// 后端拒绝建立会话（类别由用例给，见 `create` 的注释）。
    Refused(PortError),
}

#[async_trait::async_trait]
impl acp_core::ports::SessionBackendFactory for ConfigBackend {
    async fn terminate_agent(
        &self,
        _session: &acp_core::model::SessionId,
    ) -> Result<(), PortError> {
        panic!("命令面未触及")
    }

    async fn create(
        &self,
        session: &acp_core::model::SessionId,
        _request: acp_core::model::CreateSessionRequest,
        _sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        self.create_calls.fetch_add(1, Ordering::SeqCst);
        // 失败类别由用例给：`Unavailable(SessionNotEstablished)` ⇒ 可证明 Agent 侧未产生会话（终态
        // `failed`），其余不可用取值（短请求超时、掉线一类）⇒ 说不清（终态 `uncertain`）。替身只回**那一个**
        // 类别，分类本身是 core 的事。
        match self.create_step.lock().expect("创建步骤锁").take() {
            Some(CreateStep::Refused(error)) => Err(error),
            None => Ok(Box::new(ConfigEndpoint {
                options: self.endpoint.options.clone(),
                modes: self.endpoint.modes.clone(),
                prompt_calls: Arc::clone(&self.endpoint.prompt_calls),
                created: Some(session.clone()),
            })),
        }
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
            prompt_calls: Arc::clone(&self.endpoint.prompt_calls),
            created: None,
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
    /// 后端 `prompt` 的调用计数（与 `ConfigEndpoint` 共享）。
    prompt_calls: Arc<std::sync::atomic::AtomicU64>,
    /// 后端替身本体：`create` 的失败注入点与调用计数都在它上面。
    backend: Arc<ConfigBackend>,
    /// 本机配置（`session.create` 的 `workspaceAlias` 要按这里的登记表解析）。
    config: FakeConfig,
}

impl CommandWorld {
    fn new() -> Self {
        Self::with(ConfigEndpoint {
            options: vec![core_config_option()],
            modes: CoreModeState {
                current_mode: Some(mode_ref("code")),
                available: vec![mode_ref("code"), mode_ref("plan")],
            },
            prompt_calls: Arc::default(),
            created: None,
        })
    }

    fn with(endpoint: ConfigEndpoint) -> Self {
        let prompt_calls = Arc::clone(&endpoint.prompt_calls);
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
        let backend = Arc::new(ConfigBackend {
            endpoint: Arc::new(endpoint),
            sessions: Arc::clone(&sessions),
            create_calls: Arc::default(),
            create_step: Arc::default(),
        });
        let backends: Arc<dyn acp_core::ports::SessionBackendFactory> =
            Arc::clone(&backend) as Arc<dyn acp_core::ports::SessionBackendFactory>;
        let broker = Arc::new(Broker::new(
            BrokerDeps {
                store: Arc::clone(&store_port),
                deliveries: Arc::clone(&delivery_port),
                backends: Arc::clone(&backends),
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
            prompt_calls,
            backend,
            config,
        }
    }

    /// 登记一个本机 workspace 别名（`session.create` 的 `workspaceAlias` 前提：core 按登记表解析，
    /// 未登记是**受理之前**的参数类失败）。
    fn seed_workspace(&self, alias: &str) {
        self.config.seed_workspace(
            acp_core::model::WorkspaceRecord::try_new(
                acp_core::model::WorkspaceAlias::new(alias).expect("alias"),
                "Project",
                std::env::temp_dir().to_str().expect("临时目录是 UTF-8"),
                timestamp("2026-09-18T09:00:00.000Z"),
                timestamp("2026-09-18T09:00:00.000Z"),
            )
            .expect("workspace 记录"),
        );
    }

    /// 后端 `prompt` 被派发几次（幂等用例的判据：同一 requestId 只一次）。
    fn prompt_calls(&self) -> u64 {
        self.prompt_calls.load(Ordering::SeqCst)
    }

    /// 下一次后端 `create` 回退为给定错误（消费一次）：`session.create` 的终态类别由**错误的类别**决定
    /// （`Unavailable(SessionNotEstablished)` ⇒ `failed`，其余 ⇒ `uncertain`）。
    fn refuse_next_create(&self, error: PortError) {
        *self.backend.create_step.lock().expect("创建步骤锁") = Some(CreateStep::Refused(error));
    }

    /// 后端 `create` 被派发几次（幂等判据：同一 requestId 不得第二次派发）。
    fn create_calls(&self) -> u64 {
        self.backend.create_calls.load(Ordering::SeqCst)
    }

    /// 存储替身里的会话行数（`session.create` 的「失败零会话行 / 成功一行」判据）。
    fn session_rows(&self) -> usize {
        self.store.lock().sessions.len()
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
    ///
    /// 设备不再有授权集合（`pairing-model` 删除设备权限层），因此没有 scopes 参数：命令的执行前提只有
    /// 「该设备在本机有一条信任行」。
    fn seed_device(&self, device: &str) {
        let record = acp_core::model::DeviceRecord::try_new(
            DeviceId::new(device).expect("device id"),
            "Test Phone",
            acp_core::model::Fingerprint::new(&"a".repeat(64)).expect("fingerprint"),
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
/// 后端派发返回的固定 turn id（替身 prompt 的产物）。
const TEST_PROMPT_TURN: &str = "9f4c2d10-6b1e-4a3f-8c7d-2e5b9a1c4f60";
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

/// 命令名这一词表以 `compatibility/commands/v1/commands.json` 为**唯一机器来源**；命令族是服务端自持
/// 的常量分组（`CommandFamily::of` 的 `match`）。本用例钉住两者之间的接缝：
///
/// - registry 里 `transport` 含 `sync` 的命令名与 `CommandName::ALL` **双向逐条相等**（新增一个 sync
///   命令名而不加进枚举 → 红；枚举里多出一个 registry 没有的名字 → 也红）；
/// - 每条命令**恰好**被一个族覆盖：族的取值域就是 `CommandFamily::ALL`，每个族都非空，且各族的成员
///   合起来正是全部命令（数量相等 + 每个名字在结果里只出现一次）。
#[test]
fn every_sync_command_is_covered_by_exactly_one_family() {
    use sync_protocol::command::CommandName;
    let registry: serde_json::Value =
        serde_json::from_str(COMMAND_CATALOG).expect("命令登记表是合法 JSON");
    let sync_commands = registry["commands"]
        .as_array()
        .expect("commands 数组")
        .iter()
        .filter(|entry| {
            entry["transport"]
                .as_array()
                .is_some_and(|transports| transports.iter().any(|it| it.as_str() == Some("sync")))
        })
        .map(|entry| entry["name"].as_str().expect("命令名").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        sync_commands.len(),
        CommandName::ALL.len(),
        "registry 的 sync 命令数必须等于命令枚举：{sync_commands:?}"
    );
    for command in CommandName::ALL {
        assert!(
            sync_commands.iter().any(|name| name == command.as_str()),
            "{} 必须登记在 registry 里（transport 含 sync）",
            command.as_str()
        );
    }
    for name in &sync_commands {
        assert!(
            CommandName::ALL
                .iter()
                .any(|command| command.as_str() == name),
            "{name} 不在命令枚举里：新增命令必须先进词表再归类"
        );
    }

    // 每族的成员（同一命令名不可能落进两个族：`of` 是值唯一的 `match`）。
    let mut members: BTreeMap<&'static str, Vec<&'static str>> = BTreeMap::new();
    for command in CommandName::ALL {
        members
            .entry(CommandFamily::of(command).as_str())
            .or_default()
            .push(command.as_str());
    }
    assert_eq!(
        members.len(),
        CommandFamily::ALL.len(),
        "每个族都必须至少覆盖一条命令"
    );
    assert_eq!(
        members.values().map(Vec::len).sum::<usize>(),
        CommandName::ALL.len(),
        "命令必须恰好落进一个族"
    );
    for family in CommandFamily::ALL {
        assert!(
            members.contains_key(family.as_str()),
            "族 {} 没有任何命令",
            family.as_str()
        );
    }
    // 没有族名重复（`as_str` 是限流键的一部分，重名会让两族共用一个额度）。
    let mut names = CommandFamily::ALL
        .iter()
        .map(|family| family.as_str())
        .collect::<Vec<_>>();
    names.sort_unstable();
    let unique = names.len();
    names.dedup();
    assert_eq!(names.len(), unique, "族名必须互不相同");
}

// ---------------------------------------------------------------------------------------------
// 2.1 解码分派：封闭词表外与 session.resume
// ---------------------------------------------------------------------------------------------

/// `session.resume` 只经 Node Link 接受：Sync 收到即 `command.unsupported`，且**本机什么都没做**。
#[tokio::test]
async fn session_resume_is_rejected_as_unsupported_without_local_reads() {
    let world = CommandWorld::new();
    world.seed_device(DEVICE);
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
    world.seed_device(DEVICE);
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
    world.seed_device(DEVICE);
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
// 2.2 前提先于读取（设备不再有命令级授权判定）
// ---------------------------------------------------------------------------------------------

/// 本机没有该设备的信任行：命令被认证类失败拒绝，且零业务读取、零写入。
///
/// 设备方向不再有 scope 判定（`pairing-model` 删除设备权限层），剩下的唯一前提是「持久信任行存在」；
/// 这条用例同时钉住「拒绝先于本机读取」——`session.prompt` 指向一个**不存在**的会话，记账替身证明
/// 没有任何会话读取发生。
#[tokio::test]
async fn an_unauthenticated_device_is_denied_without_reading_sessions() {
    let world = CommandWorld::new();
    // 故意不 seed_device：连接声称的设备在本机没有信任行。
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
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
        Some(ErrorCode::AuthDeviceUnknown.as_str())
    );
    assert_eq!(
        world.store.session_reads(),
        0,
        "前提判定不得触碰任何会话读取（记账替身证明）"
    );
    assert_eq!(world.store.commits(), 0, "被拒命令零副作用");
}

/// 错误类型**不随会话是否存在而变**：同一条被拒命令，会话存在与不存在都得到同一个 `auth.device_unknown`
/// （否则该错误码就成了会话是否存在的预言机）。
#[tokio::test]
async fn the_authentication_error_does_not_depend_on_session_existence() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
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
            Some(ErrorCode::AuthDeviceUnknown.as_str().to_owned()),
            Some(ErrorCode::AuthDeviceUnknown.as_str().to_owned())
        ],
        "会话存在与否不得改变错误类型"
    );
    assert_eq!(world.store.session_reads(), 0);
}

/// 设备方向不再有授权维度：一台**没有任何授权集合**的已认证设备照常执行命令（旧实现会以
/// `authorization.scope_denied` 拒绝）。这里用 `session.prompt`（旧实现的 `interact` scope）验证。
#[tokio::test]
async fn an_authenticated_device_executes_without_any_scope() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let body = command_body(
        REQUEST,
        "session.prompt",
        Some(SESSION),
        serde_json::json!({ "content": [{ "type": "text", "text": "你好" }] }),
    );
    let out = dispatch(&route, &handle, &mut receiver, MESSAGE_ID, 1, body).await;
    let results = bodies(&out, "command.result");
    assert_eq!(
        results[0].get("status").and_then(|status| status.as_str()),
        Some("accepted"),
        "已认证设备必须被受理：{results:?}"
    );
    // 落盘至少一次即证明「已受理」不是内存里的假成功。这条路径有两次结构性的提交（命令面的接受行 +
    // broker 的受理写集），因此这里只断言「确实发生了写入」，不钉内部记账的具体次数。
    assert!(
        world.store.commits() >= 1,
        "受理必须真的落盘，实际提交 {} 次",
        world.store.commits()
    );
}

/// 已认证的设备对不存在的会话给出 `session.not_found`（前提判定不读取会话行，因此这个错误来自业务层）。
#[tokio::test]
async fn an_authorized_query_on_a_missing_session_reports_not_found() {
    let world = CommandWorld::new();
    world.seed_device(DEVICE);
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
    world.seed_device(DEVICE);
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
    world.seed_device(DEVICE);
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
    world.seed_device(DEVICE);
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
    world.seed_device(DEVICE);
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
    world.seed_device(DEVICE);
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
    world.seed_device(DEVICE);
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
    world.seed_device(DEVICE);
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
    world.seed_device(DEVICE);
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
    world.seed_device(DEVICE);
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
    world.seed_device(DEVICE);
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
    world.seed_device(DEVICE);
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
    world.seed_device(DEVICE);
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
    world.seed_device(DEVICE);
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
    world.seed_device(DEVICE);
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
    world.seed_device(DEVICE);
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
    world.seed_device(DEVICE);
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
    world.seed_device(DEVICE);
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
    // 只断言「不是限流」——不绑死具体是哪种业务拒绝（3.1 落地后 prompt 会走真实链路，
    // 在替身环境下回 `session.not_found`）。绑死实现细节的断言在实现推进时必然碎。
    let code = bodies(&other, "command.result")[0]
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(|code| code.as_str());
    assert_ne!(
        code,
        Some(ErrorCode::ResourceRateLimited.as_str()),
        "两族额度互不挤占；被拒也不能是限流"
    );
}

/// 限流键是 `(deviceId, 命令族)` 而**不是**连接：同一设备的第二条连接不重置额度（§14「同一设备重连不
/// 重置额度」），另一台设备则各占各的额度。把 `admit_rate` 的 `handle.device_id()` 换成
/// `connection_id()` 会让这条失败。
#[tokio::test]
async fn the_command_quota_is_counted_per_device_not_per_connection() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE);
    world.seed_device(OTHER_DEVICE);
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
    world.seed_device(DEVICE);
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
    world.seed_device(DEVICE);
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
    world.seed_device(DEVICE);
    world.seed_device(OTHER_DEVICE);
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

/// **WP3 的硬门槛**：mutation 的首响应必须是 `accepted`，且 `result`/`terminalEventId` 均为 `null`。
///
/// 这条用例的存在本身就是教训：上一次交付（`SyncCmdWP3b`）实现了 30 条 mutation 相关用例，
/// 却**没有一条**断言过 `accepted` —— 接受路径一次都没被真正走过。因此本用例是本包的验收门槛，
/// 不是普通覆盖。
#[tokio::test]
async fn a_prompt_mutation_is_accepted_without_any_business_result() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());

    let out = dispatch(
        &route,
        &handle,
        &mut receiver,
        MESSAGE_ID,
        1,
        command_body(
            REQUEST,
            "session.prompt",
            Some(SESSION),
            serde_json::json!({ "content": [{ "type": "text", "text": "你好" }] }),
        ),
    )
    .await;

    let result = &bodies(&out, "command.result")[0];
    assert_eq!(
        result["status"], "accepted",
        "mutation 首响应必须是 accepted，实际 {result}"
    );
    // 3.1 的要求：业务结果与终态都不在首响应里。
    assert_eq!(result["result"], serde_json::Value::Null);
    assert_eq!(result["terminalEventId"], serde_json::Value::Null);
    assert_eq!(result["error"], serde_json::Value::Null);
}

// -----------------------------------------------------------------------------------------------
// WP3：幂等与冲突（3.2/3.3）、版本冲突（3.4）、终态观察者（3.5）、uncertain（3.6）
// -----------------------------------------------------------------------------------------------

/// mutation 的 `accepted` 必须带非空 `acceptedAt`（§11.2：只有 `rejected` 才允许 `null`）。
#[tokio::test]
async fn an_accepted_mutation_carries_the_acceptance_time() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());

    let out = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(1),
        1,
        command_body(
            REQUEST,
            "session.prompt",
            Some(SESSION),
            prompt_payload("你好"),
        ),
    )
    .await;

    let result = &bodies(&out, "command.result")[0];
    assert_eq!(result["status"], "accepted");
    assert!(
        result["acceptedAt"].is_string(),
        "accepted 必须带接受时间：{result}"
    );
}

/// 3.2 的硬判据：同一 `requestId` 重发只产生**一个**业务副作用，且该性质跨「重新打开存储」成立。
#[tokio::test]
async fn the_same_request_id_dispatches_only_once_even_after_a_reopen() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let payload = prompt_payload("你好");

    let first = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(1),
        1,
        command_body(REQUEST, "session.prompt", Some(SESSION), payload.clone()),
    )
    .await;
    assert_eq!(bodies(&first, "command.result")[0]["status"], "accepted");
    assert_eq!(world.prompt_calls(), 1, "首次提交派发一次");

    // 同一 requestId、同一语义，但换一条 messageId（连接期去重按**帧**，要打到的正是持久幂等）。
    let again = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(2),
        2,
        command_body(REQUEST, "session.prompt", Some(SESSION), payload.clone()),
    )
    .await;
    assert_eq!(
        bodies(&again, "command.result")[0]["status"],
        "accepted",
        "重放应回首次的已知接受"
    );
    assert_eq!(world.prompt_calls(), 1, "同一 requestId 不得二次派发");

    // 「重新打开存储」：同一份持久存储之上起一条新路由（进程重启视角）。
    let reopened = world.route();
    let (handle2, mut receiver2) = connect(&world, CONNECTION_B, &SyncConfig::default());
    let third = dispatch(
        &reopened,
        &handle2,
        &mut receiver2,
        &message_id(3),
        3,
        command_body(REQUEST, "session.prompt", Some(SESSION), payload),
    )
    .await;
    assert_eq!(bodies(&third, "command.result")[0]["status"], "accepted");
    assert_eq!(
        world.prompt_calls(),
        1,
        "重开存储后同一 requestId 仍不得二次派发（幂等由持久记录承担）"
    );
}

/// 3.3：冲突判据是**解码后 payload 的语义**，不是 JSON 键顺序。
///
/// 语义比较的机制是 `payload_fingerprint` 里的 ACPR-CJ1 规范化（先把原文规范成唯一的键序再取
/// SHA-256）。这里直接断言该机制：两条键顺序不同、语义相同的原文必须规范成同一份字节。
#[test]
fn the_payload_fingerprint_ignores_key_order() {
    let ordered = acpr_wire::cj1::canonicalize(r#"{"content":[{"type":"text","text":"你好"}]}"#)
        .expect("规范化");
    let reordered = acpr_wire::cj1::canonicalize(r#"{"content":[{"text":"你好","type":"text"}]}"#)
        .expect("规范化");

    assert_eq!(
        ordered, reordered,
        "键顺序不同、语义相同的原文必须规范成同一份字节（指纹因此相同）"
    );
}

/// 3.3：同一 `requestId` 携带不同语义 → `command.idempotency_conflict`，且**零副作用**。
#[tokio::test]
async fn the_same_request_id_with_a_different_payload_is_a_conflict() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());

    let first = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(1),
        1,
        command_body(
            REQUEST,
            "session.prompt",
            Some(SESSION),
            prompt_payload("你好"),
        ),
    )
    .await;
    assert_eq!(bodies(&first, "command.result")[0]["status"], "accepted");

    let conflict = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(2),
        2,
        command_body(
            REQUEST,
            "session.prompt",
            Some(SESSION),
            prompt_payload("换了内容"),
        ),
    )
    .await;
    let result = &bodies(&conflict, "command.result")[0];
    assert_eq!(
        result["error"]["code"], "command.idempotency_conflict",
        "键相同值不同必须判冲突：{result}"
    );
    assert_eq!(world.prompt_calls(), 1, "冲突不得派发第二次");
}

/// 3.4：`session.mode.set` 带着过期版本 → `state.version_conflict`，且给出**当前真实版本**、零副作用。
#[tokio::test]
async fn a_stale_expected_version_is_rejected_with_the_current_version() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE);
    let before = world.store.commits();
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());

    let mut body = command_body(
        REQUEST,
        "session.mode.set",
        Some(SESSION),
        serde_json::json!({ "modeId": "plan" }),
    );
    body["expectedVersion"] = serde_json::Value::String("999".to_owned());

    let out = dispatch(&route, &handle, &mut receiver, &message_id(1), 1, body).await;
    let result = &bodies(&out, "command.result")[0];
    assert_eq!(
        result["error"]["code"], "state.version_conflict",
        "过期版本必须回版本冲突：{result}"
    );
    assert_eq!(
        result["error"]["details"]["currentVersion"], "0",
        "必须给出当前真实版本（替身的会话版本固定 0）"
    );
    assert_eq!(world.store.commits(), before, "版本冲突必须零副作用");
}

/// 3.5：提交方在命令到达终态时收到一条终态 `command.result`，且之后观察表不再持有它。
#[tokio::test]
async fn the_submitter_receives_its_own_terminal_state() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());

    let out = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(1),
        1,
        command_body(
            REQUEST,
            "session.prompt",
            Some(SESSION),
            prompt_payload("你好"),
        ),
    )
    .await;
    assert_eq!(bodies(&out, "command.result")[0]["status"], "accepted");

    // 命令到达终态（真实路径由外部 Agent 的 turn 结束写下；这里直接把持久记录推进去）。
    world
        .store
        .settle_command(REQUEST, CommandStatus::Completed, TERMINAL_EVENT, None);
    route.poll_terminal_for_test().await;

    let first_frames = frames(&mut receiver);
    let terminal = bodies(&first_frames, "command.result");
    assert_eq!(terminal.len(), 1, "提交方应收到一条终态回复");
    assert_eq!(terminal[0]["status"], "completed");
    assert_eq!(
        terminal[0]["terminalEventId"], TERMINAL_EVENT,
        "终态必须带原 terminal event id"
    );

    // 已终结的观察项必须从表里移除（否则每一轮都会重复推）。
    route.poll_terminal_for_test().await;
    assert!(
        bodies(&frames(&mut receiver), "command.result").is_empty(),
        "终态只推一次"
    );
}

/// 3.5：连接断开后观察项被回收（观察表是连接期状态，不留无主条目）。
#[tokio::test]
async fn a_disconnected_submitter_leaves_no_watcher_behind() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());

    let out = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(1),
        1,
        command_body(
            REQUEST,
            "session.prompt",
            Some(SESSION),
            prompt_payload("你好"),
        ),
    )
    .await;
    assert_eq!(bodies(&out, "command.result")[0]["status"], "accepted");
    assert_eq!(route.watchers_for_test(), 1, "观察项已登记");

    // 连接离开注册表（真实的断开路径：句柄从注册表移除）。
    world.registry.unregister(&handle);
    route.poll_terminal_for_test().await;
    assert_eq!(route.watchers_for_test(), 0, "断开后不得留下观察项");
}

/// 3.6：`uncertain` 是终态——再查一次不会再派发，且 `command.status` 给出原终态标识。
#[tokio::test]
async fn an_uncertain_command_is_never_dispatched_again() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    // `command.status` 需要 `grant.observe` 覆盖的那条 scope：本用例要同时走提交流与重查流。
    world.seed_device(DEVICE);
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());

    let first = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(1),
        1,
        command_body(
            REQUEST,
            "session.prompt",
            Some(SESSION),
            prompt_payload("你好"),
        ),
    )
    .await;
    assert_eq!(bodies(&first, "command.result")[0]["status"], "accepted");
    assert_eq!(world.prompt_calls(), 1);

    // 崩溃恢复把它终结成 `uncertain`（§11.2：先持久化再广播，不自动重派）。
    world
        .store
        .settle_command(REQUEST, CommandStatus::Uncertain, TERMINAL_EVENT, None);

    // 重放同一 requestId：回的是那条终结态，不是第二次派发。
    let replay = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(2),
        2,
        command_body(
            REQUEST,
            "session.prompt",
            Some(SESSION),
            prompt_payload("你好"),
        ),
    )
    .await;
    let result = &bodies(&replay, "command.result")[0];
    assert_eq!(
        result["status"], "uncertain",
        "重放必须回已终结的终态：{result}"
    );
    assert_eq!(result["terminalEventId"], TERMINAL_EVENT);
    assert_eq!(world.prompt_calls(), 1, "uncertain 不得触发第二次派发");

    // `command.status` 给出同一终态与原 terminal event id。
    let status = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(3),
        3,
        command_body(
            STATUS_REQUEST,
            "command.status",
            None,
            serde_json::json!({ "targetRequestId": REQUEST }),
        ),
    )
    .await;
    let record = &bodies(&status, "command.result")[0];
    assert_eq!(record["status"], "completed");
    assert_eq!(record["result"]["state"], "uncertain");
    assert_eq!(record["result"]["terminalEventId"], TERMINAL_EVENT);
}

// -----------------------------------------------------------------------------------------------
// `session.create`：三笔时序的适配层（终态判据、失败分类、命令级日志）
// -----------------------------------------------------------------------------------------------

/// 协商 `core.session-create.v1`（未协商时 `session.create` 被 feature 门控拒绝）。
fn negotiate_session_create(route: &CommandRoute, handle: &ConnectionHandle) {
    crate::sync::conn::MessageRoute::on_authenticated(
        route,
        handle,
        &[crate::sync::command::SESSION_CREATE_FEATURE.to_owned()],
    );
}

/// `session.create` 的 body（Sync 面形状：payload 只有 `workspaceAlias` 与 `agentId`）。
fn create_body(request: &str, alias: &str) -> serde_json::Value {
    command_body(
        request,
        "session.create",
        None,
        serde_json::json!({ "workspaceAlias": alias, "agentId": "codex" }),
    )
}

/// 一个已认证、已登记 `project` 目录别名的世界（创建用例的公共前提）。
fn create_world() -> CommandWorld {
    let world = CommandWorld::new();
    world.seed_device(DEVICE);
    world.seed_workspace("project");
    world
}

/// §6 第 20 条的三笔时序：受理只落幂等行（会话行随落账提交才出现），客户端先收到
/// `accepted(result = null, terminalEventId = null)`，终态由连接期观察者推来；同键重发回首次终态。
#[tokio::test]
async fn a_session_create_is_accepted_then_completes_with_the_created_session() {
    let world = create_world();
    let before = world.session_rows();
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    negotiate_session_create(&route, &handle);

    let out = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(1),
        1,
        create_body(REQUEST, "project"),
    )
    .await;
    let accepted = bodies(&out, "command.result");
    assert_eq!(accepted.len(), 1, "受理只回一条：{accepted:?}");
    assert_eq!(accepted[0]["status"], "accepted");
    assert_eq!(accepted[0]["requestId"], REQUEST);
    assert_eq!(
        accepted[0]["result"],
        serde_json::Value::Null,
        "受理不带业务结果"
    );
    assert_eq!(
        accepted[0]["terminalEventId"],
        serde_json::Value::Null,
        "终态只经持久事件表达"
    );
    assert!(accepted[0]["acceptedAt"].is_string());
    assert_eq!(world.create_calls(), 1, "后端被派发一次");
    assert_eq!(
        world.session_rows(),
        before + 1,
        "落账提交建了一行会话行（受理阶段不建）"
    );

    // 终态由连接期观察者推：终态事件与落账在**同一次提交**里写下来。
    route.poll_terminal_for_test().await;
    let route_terminal_frames = frames(&mut receiver);
    let terminal = bodies(&route_terminal_frames, "command.result");
    assert_eq!(terminal.len(), 1, "终态一条：{terminal:?}");
    assert_eq!(terminal[0]["status"], "completed");
    assert_eq!(terminal[0]["terminalEventId"], STORE_TERMINAL_EVENT);
    let result = &terminal[0]["result"];
    let created = result["sessionId"].as_str().expect("会话标识");
    assert!(
        created.starts_with("00000000-0000-4000-8000-"),
        "会话标识由 core 分配（不是存储层现分配）：{created}"
    );
    assert_eq!(result["session"]["state"], "idle");
    assert_eq!(result["session"]["agent"]["agentId"], "codex");

    // 幂等：同键重发回首次终态，不再派发第二次创建、不产生第二个会话。
    let replay = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(2),
        2,
        create_body(REQUEST, "project"),
    )
    .await;
    let replayed = bodies(&replay, "command.result");
    assert_eq!(replayed.len(), 1, "重发回一条终态：{replayed:?}");
    assert_eq!(replayed[0]["status"], "completed");
    assert_eq!(replayed[0]["terminalEventId"], STORE_TERMINAL_EVENT);
    assert_eq!(world.create_calls(), 1, "同一 requestId 不得第二次派发");
    assert_eq!(world.session_rows(), before + 1, "不得产生第二个会话");
}

/// **可证明未产生会话**（进程未能启动一类）⇒ 终态 `failed` + `internal.unavailable`，零会话行；且
/// 已受理的命令**不会**被回成 `rejected`（那是「未产生任何副作用」的宣告）。
#[tokio::test]
async fn a_session_create_that_provably_did_not_establish_a_session_fails() {
    let world = create_world();
    let before = world.session_rows();
    world.refuse_next_create(PortError::Unavailable(
        acp_core::model::UnavailableKind::SessionNotEstablished,
    ));
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    negotiate_session_create(&route, &handle);

    let out = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(1),
        1,
        create_body(REQUEST, "project"),
    )
    .await;
    let results = bodies(&out, "command.result");
    assert!(
        results.iter().all(|result| result["status"] != "rejected"),
        "已受理的命令不得回 rejected：{results:?}"
    );
    assert_eq!(
        results.len(),
        2,
        "首次响应必须是受理态、随后才是终态（客户端要能观察到 accepted → 终态）：{results:?}"
    );
    assert_eq!(results[0]["status"], "accepted");
    assert_eq!(results[0]["result"], serde_json::Value::Null);
    assert_eq!(results[0]["terminalEventId"], serde_json::Value::Null);
    assert_eq!(results[0]["acceptedAt"], results[1]["acceptedAt"]);
    assert_eq!(results[1]["status"], "failed");
    assert_eq!(
        results[1]["error"]["code"], "internal.unavailable",
        "failed 复用既有错误码：{results:?}"
    );
    assert_eq!(
        results[1]["terminalEventId"], STORE_TERMINAL_EVENT,
        "终态以持久事件为权威：{results:?}"
    );
    assert_eq!(
        world.session_rows(),
        before,
        "可证明未建立会话时零会话行（受理阶段不建行，失败也不留孤儿行）"
    );

    // 同键重发：回同一 `failed` 终态，且不再派发第二次创建。
    let replay = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(2),
        2,
        create_body(REQUEST, "project"),
    )
    .await;
    let replayed = bodies(&replay, "command.result");
    assert_eq!(replayed[0]["status"], "failed");
    assert_eq!(replayed[0]["terminalEventId"], STORE_TERMINAL_EVENT);
    assert_eq!(world.create_calls(), 1, "重发不得第二次派发");
    assert_eq!(world.session_rows(), before, "重发不产生会话行");
}

/// **说不清**（短请求超时、发出后连接断开一类）⇒ 终态 `uncertain`（可重试的结论不得冒充「什么也没发生」）。
#[tokio::test]
async fn a_session_create_that_cannot_prove_the_outcome_is_uncertain() {
    let world = create_world();
    let before = world.session_rows();
    world.refuse_next_create(PortError::Unavailable(
        acp_core::model::UnavailableKind::IoError,
    ));
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    negotiate_session_create(&route, &handle);

    let out = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(1),
        1,
        create_body(REQUEST, "project"),
    )
    .await;
    let results = bodies(&out, "command.result");
    assert_eq!(
        results.len(),
        2,
        "首次响应必须是受理态、随后才是终态：{results:?}"
    );
    assert_eq!(results[0]["status"], "accepted");
    assert_eq!(
        results[1]["status"], "uncertain",
        "说不清时不得报 failed：{results:?}"
    );
    assert_eq!(results[1]["error"]["code"], "command.uncertain");
    assert_eq!(
        world.session_rows(),
        before,
        "说不清也不留会话行（可证明与否都不建行）"
    );
}

/// 同键不同语义：`command.idempotency_conflict` 且零副作用（**唯一**一条「已有记录也回 `rejected`」的
/// 失败——首次结果属于另一条语义的命令，重放它会让对端以为本次的不同 payload 被执行过）。
#[tokio::test]
async fn a_session_create_with_the_same_request_id_and_another_payload_conflicts() {
    let world = create_world();
    let before = world.session_rows();
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    negotiate_session_create(&route, &handle);
    world.seed_workspace("alt");

    let first = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(1),
        1,
        create_body(REQUEST, "project"),
    )
    .await;
    assert_eq!(bodies(&first, "command.result")[0]["status"], "accepted");

    let conflict = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(2),
        2,
        create_body(REQUEST, "alt"),
    )
    .await;
    let result = &bodies(&conflict, "command.result")[0];
    assert_eq!(result["status"], "rejected", "冲突回 rejected：{result}");
    assert_eq!(result["error"]["code"], "command.idempotency_conflict");
    assert_eq!(world.create_calls(), 1, "冲突不得派发第二次创建");
    assert_eq!(world.session_rows(), before + 1, "只有首次那一个会话");
}

/// 未协商 `core.session-create.v1` 时**不静默接受**（§11.5 的 feature 门控），且**零业务副作用**。
#[tokio::test]
async fn a_session_create_without_the_feature_is_rejected_before_any_create() {
    let world = create_world();
    let before = world.session_rows();
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());

    let out = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(1),
        1,
        create_body(REQUEST, "project"),
    )
    .await;
    let result = &bodies(&out, "command.result")[0];
    assert_eq!(result["status"], "rejected");
    assert_eq!(result["error"]["code"], "protocol.feature_required");
    assert_eq!(world.create_calls(), 0, "被拒命令不得派发创建");
    assert_eq!(world.session_rows(), before);
}

/// 别名未登记是**受理之前**的参数类失败：回 `rejected`（主 spec「引用失败与解析失败的分类」），
/// 不是 `failed`——那时连受理记录都没有。
#[tokio::test]
async fn a_session_create_with_an_unregistered_alias_is_rejected_without_a_record() {
    let world = create_world();
    let before = world.session_rows();
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    negotiate_session_create(&route, &handle);

    let out = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(1),
        1,
        create_body(REQUEST, "missing"),
    )
    .await;
    let result = &bodies(&out, "command.result")[0];
    assert_eq!(result["status"], "rejected", "{result}");
    assert_eq!(result["error"]["code"], "protocol.schema_invalid");
    assert_eq!(world.create_calls(), 0, "解析失败不得触碰后端");
    assert_eq!(world.session_rows(), before);
}

/// 命令级日志（`sync-command-handling` 的「命令级结构化日志」）：受理与终态各一条，字段集合恰好是登记
/// 的那一组（主体、命令名、资源标识、请求标识；终态再加类别与封闭词表里的成因）。
#[tokio::test]
async fn the_accepted_session_create_and_its_terminal_each_leave_one_log_line() {
    let world = create_world();
    let (logs, guard) = crate::command_log::capture::Logs::install();
    world.refuse_next_create(PortError::Unavailable(
        acp_core::model::UnavailableKind::SessionNotEstablished,
    ));
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    negotiate_session_create(&route, &handle);
    let _ = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(1),
        1,
        create_body(REQUEST, "project"),
    )
    .await;
    drop(guard);

    let accepted = logs.of("sync.command_accepted");
    assert_eq!(accepted.len(), 1, "受理一条：{accepted:?}");
    crate::command_log::capture::assert_field_set(
        &accepted[0],
        &[
            "event",
            "device_id",
            "command",
            "session_id",
            "request_id",
            "turn",
            "message",
        ],
    );
    assert_eq!(accepted[0]["device_id"], DEVICE);
    assert_eq!(accepted[0]["command"], "session.create");
    assert_eq!(accepted[0]["request_id"], REQUEST);
    assert_eq!(
        accepted[0]["session_id"], "None",
        "受理阶段还没有会话（资源标识为空）"
    );

    let terminal = logs.of("sync.command_terminal");
    assert_eq!(terminal.len(), 1, "终态一条：{terminal:?}");
    crate::command_log::capture::assert_field_set(
        &terminal[0],
        &[
            "event",
            "device_id",
            "command",
            "session_id",
            "request_id",
            "terminal",
            "cause",
            "message",
        ],
    );
    assert_eq!(terminal[0]["device_id"], DEVICE);
    assert_eq!(terminal[0]["command"], "session.create");
    assert_eq!(terminal[0]["request_id"], REQUEST);
    assert_eq!(terminal[0]["terminal"], "failed");
    assert_eq!(
        terminal[0]["cause"], "unavailable",
        "成因取自持久记录里的错误码（`failed` 那一档的码是 internal.unavailable）"
    );
    // 日志里没有任何路径/消息原文（只有标识、命令名与封闭类别）。
    let text = format!("{terminal:?}");
    assert!(!text.contains("后端"), "成因不得带出错误消息：{text}");
    assert!(!text.contains('\\'), "日志不得带出路径：{text}");
}

/// 已接受 mutation 的受理与终态同样各一条日志（**不只是** `session.create`：通用 mutation 路径也覆盖）。
#[tokio::test]
async fn a_prompt_mutation_logs_both_stages_with_the_target_session() {
    let world = CommandWorld::new();
    world.push_session(core_session(SESSION));
    world.seed_device(DEVICE);
    let (logs, guard) = crate::command_log::capture::Logs::install();
    let route = world.route();
    let (handle, mut receiver) = connect(&world, CONNECTION_A, &SyncConfig::default());
    let _ = dispatch(
        &route,
        &handle,
        &mut receiver,
        &message_id(1),
        1,
        command_body(
            REQUEST,
            "session.prompt",
            Some(SESSION),
            prompt_payload("你好"),
        ),
    )
    .await;
    world
        .store
        .settle_command(REQUEST, CommandStatus::Completed, TERMINAL_EVENT, None);
    route.poll_terminal_for_test().await;
    drop(guard);

    let accepted = logs.of("sync.command_accepted");
    assert_eq!(accepted.len(), 1, "受理一条：{accepted:?}");
    assert_eq!(accepted[0]["command"], "session.prompt");
    assert_eq!(
        accepted[0]["session_id"],
        format!("Some({SESSION:?})"),
        "受理日志带资源标识"
    );

    let terminal = logs.of("sync.command_terminal");
    assert_eq!(terminal.len(), 1, "终态一条：{terminal:?}");
    assert_eq!(terminal[0]["terminal"], "completed");
    assert_eq!(terminal[0]["cause"], "none");
    assert_eq!(terminal[0]["session_id"], format!("Some({SESSION:?})"));
}

/// `session.prompt` 的 payload（与既有用例同形）。
fn prompt_payload(text: &str) -> serde_json::Value {
    serde_json::json!({ "content": [{ "type": "text", "text": text }] })
}

/// 终态记录里的 command terminal event id（终态回复必须原样带出它）。
const TERMINAL_EVENT: &str = "7b3f9c21-4d5e-4a6b-9c8d-1e2f3a4b5c6d";
/// `command.status` 查询用的 requestId（与 mutation 的 requestId 不同）。
const STATUS_REQUEST: &str = "5e6f7a8b-9c0d-4e1f-8a2b-3c4d5e6f7a8b";
