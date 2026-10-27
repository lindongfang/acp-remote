//! 本机（owned）会话生命周期：运行期目录、五个端口的实现与 owned 提交顺序。
//!
//! 权威：`docs/CORE_PORTS_AND_STORAGE.md` §3.3/§4/§5.10/§6 第 23 条、`SYNC_PROTOCOL.md` §9.8/§11.8、
//! `openspec/changes/session-activation-lifecycle/specs/local-session-lifecycle/spec.md`。
//!
//! 设计约束（逐条对应合同）：
//!
//! - **运行期**：资源代、查看租约、最后保护时刻与保留起点只存在于本模块的 [`Mutex`] 状态里，**不落库**、
//!   不进快照或事件；时间一律来自注入的 [`Clock`] 或端口参数。
//! - **归约唯一**：[`LocalActivity::reduce`] 是唯一取值规则；本模块只负责把「有效交互/未结束任务/资源代」
//!   投影成它的输入。
//! - **不交错**：激活与回收由进程内的目录锁加资源代隔离；`reclaiming` 期间新的激活返回
//!   [`ActivationPlan::InFlight`]，不会启动第二次建立。
//! - **先持久化后发布**：`session.activity.changed` 只在该事件**实际改变**归约结果时提交；提交成功后才
//!   经 [`EventPublisher`] 广播，提交失败不广播（§6 第 23 条）。
//!
//! 本模块不依赖 Tokio、文件系统、进程、wire DTO 或 SQLite；它只组合 [`Clock`]/[`SessionStore`]/
//! [`EventPublisher`]/[`SessionRelease`] 这些既有的 core 端口。

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::task::{Context, Poll, Waker};

use async_trait::async_trait;

use crate::broker::{pending_event, session_summary_view};
use crate::model::{
    Actor, CommandRecord, CommandStatus, ConflictKind, ConnectionId, EntityRef, EventKind,
    GlobalCursor, InvalidValue, LocalActivity, LocalActivityFacts, PortError, PublicError,
    RequestId, ResourceGeneration, RestartEmptySessionDeadline, SessionDeletionOutcome,
    SessionDeletionWrite, SessionId, SessionSummary, StoredPolicy, Timestamp, UnavailableKind,
    ViewJson,
};

use crate::ports::{
    ActivationAdmission, ActivationOutcome, ActivationRequest, Admission, CapacityAdmission, Clock,
    EventPublisher, LifecycleQuery, OwnedCommit, ReclaimCandidate, ReleaseOutcome, ReplayLimit,
    SessionActivation, SessionActivityFacts, SessionContentFacts, SessionQuery, SessionRelease,
    SessionStore, ViewDecision, ViewRegistry,
};
use crate::use_cases::UseCases;

/// 查看租约的固定 v1 时长（`SYNC_PROTOCOL.md` §11.8 的「查看控制预算」：30 秒）。
///
/// 这是固定 v1 常量，不由 `auth.authenticated.limits` 下发，也不得通过配置放宽。
pub const VIEW_LEASE_MS: u64 = 30_000;

/// owned 生命周期变化事件（`SYNC_PROTOCOL.md` §9.8/§10.3）。
pub const LOCAL_ACTIVITY_EVENT: &str = "session.activity.changed";

/// `sessions.idle_timeout_ms` 与 `sessions.max_active_sessions` 的运行期视图。
///
/// 配置解析（正整数校验、默认值）由组合根/`CONFIG_REFERENCE.md` §4 拥有；core 只要求：
/// `idle_timeout_ms = 0` **只**关闭按时间回收、不关闭容量策略；`max_active_sessions` 没有「无限」取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifecycleConfig {
    idle_timeout_ms: u64,
    max_active_sessions: u32,
}

impl LifecycleConfig {
    /// `max_active_sessions` 必须是正整数（`0` = 无限是**非法**取值，失败关闭）。
    pub fn new(idle_timeout_ms: u64, max_active_sessions: u32) -> Result<Self, InvalidValue> {
        if max_active_sessions == 0 {
            return Err(InvalidValue::Field);
        }
        Ok(Self {
            idle_timeout_ms,
            max_active_sessions,
        })
    }

    pub const fn idle_timeout_ms(self) -> u64 {
        self.idle_timeout_ms
    }

    pub const fn max_active_sessions(self) -> u32 {
        self.max_active_sessions
    }

    /// 按时间回收是否启用（显式 `0` = 只关闭这一部分）。
    pub const fn time_based_reclaim_enabled(self) -> bool {
        self.idle_timeout_ms > 0
    }
}

/// 一次激活的分流结果（`LocalSessionLifecycle::activate_session`）。
///
/// 契约的 [`ActivationAdmission`] 与它一一对应，但本类型额外回答「本次是否需要启动后端」——ready 复用
/// **不触发 ACP**，因此适配层不能只看 `Prepared` 就无条件恢复。
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // `Settled` 承载完整的 `CommandRecord`（与契约一致）
pub enum ActivationPlan {
    /// 需要启动后端建立/恢复；容量名额已预占、资源代已分配；完成后必须 `settle`。
    Establish { generation: ResourceGeneration },
    /// 资源代已 `ready`：复用现有资源，**不得**触发 ACP；直接按本次请求的收据收口。
    Reused { generation: ResourceGeneration },
    /// 同会话已有激活在建立中，或回收尚未确认：不得重复启动，等同一份终态。
    InFlight,
    /// 没有可回收的安全候选：明确容量拒绝（适配层映射 `session.capacity_exceeded`，retryable）。
    CapacityExceeded,
    /// 已有持久终态记录（幂等重试，或先前调用已收口）。
    Settled(CommandRecord),
}

impl ActivationPlan {
    /// 契约三态的投影。`CapacityExceeded` 在契约里没有变体：适配层 MUST 用本类型（或先调用
    /// [`CapacityAdmission::reserve`]）取得明确的容量拒绝，不得把它折叠成 `Prepared`/`InFlight`。
    pub fn admission(&self) -> ActivationAdmission {
        match self {
            Self::Establish { .. } | Self::Reused { .. } => ActivationAdmission::Prepared,
            Self::InFlight | Self::CapacityExceeded => ActivationAdmission::InFlight,
            Self::Settled(record) => ActivationAdmission::Settled(record.clone()),
        }
    }

    /// 本次资源代（`InFlight`/`Settled`/容量拒绝时为 `None`）。
    pub fn generation(&self) -> Option<ResourceGeneration> {
        match self {
            Self::Establish { generation } | Self::Reused { generation } => Some(*generation),
            Self::InFlight | Self::CapacityExceeded | Self::Settled(_) => None,
        }
    }

    /// 是否可以在不触碰 Agent 的情况下按本次请求收口。
    pub fn reuses_ready_resource(&self) -> bool {
        matches!(self, Self::Reused { .. })
    }
}

/// owned 会话的只读投影（`localActivity` 与快照读同一代目录，§6 第 23 条）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalSessionView {
    pub summary: SessionSummary,
    pub activity: LocalActivity,
}

/// 单个 owned 会话的 Owner 生命周期门。
///
/// 该值只证明调用方当前持有某条会话的串行权。生产入口在内部获取；
/// [`LocalSessionLifecycle::acquire_owner_lifecycle_gate`] 仅供需要把「释放 → 最终删除事务」作为一个
/// 不可插入区间的适配层使用。
pub struct SessionLifecycleGate {
    session: SessionId,
    inner: Arc<SessionGateClaim>,
}

struct SessionGateClaim {
    inner: Arc<Mutex<SessionGateInner>>,
}

pub fn clone_gate(gate: &SessionLifecycleGate) -> SessionLifecycleGate {
    gate.clone_lifecycle_handle()
}

impl std::fmt::Debug for SessionLifecycleGate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionLifecycleGate")
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

impl SessionLifecycleGate {
    pub fn session(&self) -> &SessionId {
        &self.session
    }

    /// Hand the already-held lifecycle claim to an asynchronous settlement
    /// without taking a second ticket for the same session.
    /// Extends the same ownership claim to another future. Dropping either
    /// handle keeps the claim until the remaining handle is dropped.
    pub fn clone_lifecycle_handle(&self) -> Self {
        Self {
            session: self.session.clone(),
            inner: Arc::clone(&self.inner),
        }
    }
}

/// 扫描器生成删除意图时读取的保留状态快照。
///
/// 快照只在生成候选时用于建立预期；实际删除必须持有会话 gate 后再次读取并重新执行到期检查，
/// 不能把这个一次性快照当作授权本身。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionSnapshot {
    pub generation: ResourceGeneration,
    pub retention_start: Option<Timestamp>,
    pub delete_pending: bool,
}

/// 激活/回收失败的运行期事实。它只服务于 UI/诊断：失败**不覆盖**真实任务或交互，也不改变容量占位规则
/// （`uncertain`/关闭失败保持占位与 `Active`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleFailure {
    pub error: PublicError,
    /// `true` = 结果不确定（可能已经产生外部资源）；`false` = 可证明没有建立/释放。
    pub uncertain: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// 没有运行资源，也没有容量占位。
    Inactive,
    /// 正在建立/恢复：占位，但归约仍是 `Idle`（不提前声称可用）。
    Activating,
    /// 已确认可用的运行资源。
    Ready,
    /// 已开始回收、尚未确认释放：保持 `Active` 与占位。
    Reclaiming,
    /// 资源已确认释放，但持久删除事务尚未成功；保留会话行供扫描器用新请求重试。
    DeletePending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReleaseTarget {
    Inactive,
    DeletePending,
}

#[derive(Debug, Clone)]
struct Entry {
    generation: ResourceGeneration,
    /// Identifies this runtime incarnation independently of its numeric
    /// generation, so a late async result cannot bind to a successor.
    epoch: Option<u64>,
    /// Epoch of the exact delete intent. A later activation can start a new
    /// epoch on the same entry while the old request is unwinding.
    delete_epoch: Option<u64>,
    phase: Phase,
    /// 容量名额是否被本会话占用（ready/activating/reclaiming/uncertain 都算）。
    reserved: bool,
    /// 本代是否已经 `settle`（重复 settle 是幂等无操作）。
    settled: Option<ResourceGeneration>,
    /// 外部资源可能存在、但结果不确定（`ActivationOutcome::Uncertain` 或关闭结果不确定）。
    resource_unconfirmed: bool,
    failure: Option<LifecycleFailure>,
    /// The runtime resource exists although creation has not yet persisted the
    /// session row. A prompt on it is safe and must not be treated as an empty
    /// session pending cleanup.
    established: bool,
    /// 启动发现时为空会话播种的资源回收标记。它不占容量，只让缺失运行资源也能进入
    /// 到期释放与删除路径；用户重新建立该会话时清除。
    restart_seeded: bool,
    /// 最后观察到的「全部保护解除」的时刻；`None` = 仍受保护。
    retention_start: Option<Timestamp>,
    /// 最后一次仍受保护的时刻（容量 LRU 的选择依据）。
    last_protected_at: Timestamp,
    /// 最近一次已发布/已观察的归约值（任务驱动的刷新据此判断「取值是否实际变化」）。
    last_activity: LocalActivity,
    /// A prompt which began before its durable write may have committed.
    /// This remains set across message-dispatch errors.
    active_prompt: Option<RequestId>,
    /// A durable deletion has been admitted while the lifecycle gate is held.
    /// It blocks later prompts before their acceptance transaction.
    delete_intent: Option<(ResourceGeneration, RequestId)>,
    /// A retention scanner has picked this exact generation. Any later user
    /// activity revokes this lease synchronously, before its I/O starts.
    delete_lease: Option<ResourceGeneration>,
}

impl Entry {
    fn new(now: Timestamp) -> Self {
        Self {
            generation: ResourceGeneration::new(0),
            epoch: None,
            delete_epoch: None,
            phase: Phase::Inactive,
            reserved: false,
            settled: None,
            resource_unconfirmed: false,
            failure: None,
            established: false,
            restart_seeded: false,
            retention_start: None,
            last_protected_at: now,
            last_activity: LocalActivity::Idle,
            active_prompt: None,
            delete_intent: None,
            delete_lease: None,
        }
    }
}

#[derive(Debug, Clone)]
struct ViewState {
    /// 当前查看的会话；`None` = 已离页，但保留版本水位。
    session: Option<SessionId>,
    /// 该连接已见过的最大 `viewVersion`（离页/过期后仍保留，迟到的低版本不得复活页面）。
    view_version: u64,
    /// 当前租约的到期时刻（`session = None` 时无意义）。
    expires_at: Option<Timestamp>,
}

impl ViewState {
    fn is_live(&self, now: &Timestamp) -> bool {
        match (&self.session, &self.expires_at) {
            (Some(_), Some(expires_at)) => now < expires_at,
            _ => false,
        }
    }
}

#[derive(Debug, Default)]
struct DirectoryState {
    entries: HashMap<SessionId, Entry>,
    views: HashMap<ConnectionId, ViewState>,
}

/// 按会话分配 FIFO 门。映射只保留弱引用，避免删除后的会话永久占用进程内存。
#[derive(Default)]
struct SessionGates {
    gates: Mutex<HashMap<SessionId, Weak<Mutex<SessionGateInner>>>>,
}

struct SessionGateInner {
    locked: bool,
    next_ticket: u64,
    waiters: VecDeque<(u64, Waker)>,
    granted: Option<u64>,
}

struct SessionGateWait {
    inner: Arc<Mutex<SessionGateInner>>,
    ticket: Option<u64>,
}

impl SessionGates {
    async fn acquire(&self, session: SessionId) -> SessionLifecycleGate {
        let inner = self.acquire_raw(session.clone()).await;
        SessionLifecycleGate {
            session,
            inner: Arc::new(SessionGateClaim { inner }),
        }
    }

    async fn acquire_raw(&self, session: SessionId) -> Arc<Mutex<SessionGateInner>> {
        let inner = {
            let mut gates = lock(&self.gates);
            if gates.len() > 256 {
                gates.retain(|_, gate| gate.strong_count() > 0);
            }
            match gates.get(&session).and_then(Weak::upgrade) {
                Some(inner) => inner,
                None => {
                    let inner = Arc::new(Mutex::new(SessionGateInner {
                        locked: false,
                        next_ticket: 0,
                        waiters: VecDeque::new(),
                        granted: None,
                    }));
                    gates.insert(session.clone(), Arc::downgrade(&inner));
                    inner
                }
            }
        };
        SessionGateWait {
            inner: inner.clone(),
            ticket: None,
        }
        .await;
        inner
    }
}

impl Future for SessionGateWait {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let inner_handle = Arc::clone(&self.inner);
        let mut inner = lock(&inner_handle);
        if !inner.locked {
            inner.locked = true;
            inner.granted = None;
            self.ticket = None;
            return Poll::Ready(());
        }
        match self.ticket {
            Some(ticket) if inner.granted == Some(ticket) => {
                inner.granted = None;
                self.ticket = None;
                Poll::Ready(())
            }
            Some(_) => Poll::Pending,
            None => {
                inner.next_ticket += 1;
                let ticket = inner.next_ticket;
                inner.waiters.push_back((ticket, cx.waker().clone()));
                self.ticket = Some(ticket);
                Poll::Pending
            }
        }
    }
}

impl Drop for SessionGateWait {
    fn drop(&mut self) {
        let Some(ticket) = self.ticket else {
            return;
        };
        let mut inner = lock(&self.inner);
        if let Some(position) = inner
            .waiters
            .iter()
            .position(|(queued, _)| *queued == ticket)
        {
            inner.waiters.remove(position);
        }
        if inner.granted == Some(ticket) {
            match inner.waiters.pop_front() {
                Some((next, waker)) => {
                    inner.granted = Some(next);
                    drop(inner);
                    waker.wake();
                }
                None => {
                    inner.locked = false;
                    inner.granted = None;
                }
            }
        }
    }
}

impl Drop for SessionGateClaim {
    fn drop(&mut self) {
        let mut inner = lock(&self.inner);
        match inner.waiters.pop_front() {
            Some((ticket, waker)) => {
                inner.granted = Some(ticket);
                drop(inner);
                waker.wake();
            }
            None => {
                inner.locked = false;
                inner.granted = None;
            }
        }
    }
}

/// 本机生命周期的运行期目录 + 五个端口的 core 实现。
///
/// 组合根在启动收敛完成后才开放入站查询/订阅；本类型的查询/准入入口在收敛完成前返回
/// [`PortError::Unavailable(UnavailableKind::Busy)`]（`docs/MODULE_ARCHITECTURE.md` §4.10 的关闭顺序）。
pub struct LocalSessionLifecycle {
    clock: Arc<dyn Clock>,
    store: Arc<dyn SessionStore>,
    content: Arc<dyn SessionContentFacts>,
    publisher: Arc<dyn EventPublisher>,
    release: std::sync::OnceLock<Arc<dyn SessionRelease>>,
    config: Mutex<LifecycleConfig>,
    state: Mutex<DirectoryState>,
    gates: SessionGates,
    delete_barriers: DeleteBarriers,
    converged: AtomicBool,
    next_epoch: AtomicU64,
}

/// A lifecycle gate plus an immutable snapshot of the session's runtime
/// directory. A delete held on the gate can prove that no activation was
/// granted its own gate before the tombstone transaction.
pub struct SessionLifecycleStateGuard {
    gate: SessionLifecycleGate,
    generation: Option<ResourceGeneration>,
    delete_pending: bool,
}

pub struct DeleteLease {
    session: SessionId,
    generation: ResourceGeneration,
    delete_epoch: Option<u64>,
    epoch: Option<u64>,
    gate: Option<SessionLifecycleGate>,
}

/// Deletion processes in the order accepted, before the write executor
/// acquires the session lifecycle gate. This preserves a user's deletion
/// intent when a create or prompt settlement is also still pending.
#[derive(Clone, Default)]
pub struct DeleteBarriers {
    barriers: Arc<Mutex<HashMap<SessionId, Arc<Mutex<DeleteBarrierState>>>>>,
}

struct DeleteBarrierState {
    accepted: bool,
    active: bool,
    waiters: Vec<Waker>,
}

pub struct DeleteBarrierGuard {
    barrier: Arc<Mutex<DeleteBarrierState>>,
    active: bool,
}

impl DeleteBarriers {
    pub fn accept(&self, session: &SessionId) {
        let mut barriers = lock(&self.barriers);
        if barriers.len() > 256 {
            barriers.retain(|_, barrier| {
                let state = lock(barrier);
                state.accepted || state.active
            });
        }
        match barriers.get(session) {
            Some(barrier) => lock(barrier).accepted = true,
            None => {
                barriers.insert(
                    session.clone(),
                    Arc::new(Mutex::new(DeleteBarrierState {
                        accepted: true,
                        active: false,
                        waiters: Vec::new(),
                    })),
                );
            }
        }
    }

    pub async fn enter(&self, session: &SessionId) -> Option<DeleteBarrierGuard> {
        let barrier = {
            let barriers = lock(&self.barriers);
            barriers
                .get(session)
                .filter(|barrier| lock(barrier).accepted)
                .cloned()
        }?;
        let guard = DeleteBarrierGuard {
            barrier,
            active: false,
        };
        let mut guard = guard;
        std::future::poll_fn(|cx| {
            let mut state = lock(&guard.barrier);
            if !state.accepted {
                return Poll::Ready(());
            }
            if state.active {
                state.waiters.push(cx.waker().clone());
                return Poll::Pending;
            }
            state.active = true;
            Poll::Ready(())
        })
        .await;
        guard.active = true;
        Some(guard)
    }
}

impl LocalSessionLifecycle {
    /// Establishes ordering for a deletion which has already been accepted
    /// durably. Its write executor will wait for an earlier create or prompt
    /// settlement without holding the session's lifecycle gate.
    pub fn accept_delete(&self, session: &SessionId) {
        self.delete_barriers.accept(session);
    }

    pub fn finish_accepted_delete(&self, session: &SessionId) {
        let Some(barrier) = lock(&self.delete_barriers.barriers).get(session).cloned() else {
            return;
        };
        let wake = {
            let mut state = lock(&barrier);
            state.accepted = false;
            std::mem::take(&mut state.waiters)
        };
        for waker in wake {
            waker.wake();
        }
    }

    pub async fn enter_accepted_delete(&self, session: &SessionId) -> Option<DeleteBarrierGuard> {
        self.delete_barriers.enter(session).await
    }

    /// After the exact deletion failure is durable, releases only this
    /// request's runtime claim. The command gate remains held by the caller
    /// through this transition, so a retried request cannot race it.
    pub fn fail_delete_release_current_in_gate(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
    ) -> Result<(), PortError> {
        self.ensure_gate(gate, session)?;
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return Ok(());
        };
        if entry.phase == Phase::Reclaiming && !entry.resource_unconfirmed {
            entry.delete_lease = None;
            entry.resource_unconfirmed = false;
            entry.restart_seeded = false;
            entry.established = false;
            entry.phase = Phase::Inactive;
            entry.reserved = false;
            entry.retention_start = None;
            entry.last_protected_at = self.now();
        }
        Ok(())
    }
}

impl DeleteBarrierGuard {
    pub fn release(mut self) {
        self.active = false;
        let wake = {
            let mut state = lock(&self.barrier);
            state.active = false;
            std::mem::take(&mut state.waiters)
        };
        for waker in wake {
            waker.wake();
        }
    }
}

impl Drop for DeleteBarrierGuard {
    fn drop(&mut self) {
        if self.active {
            self.active = false;
            let wake = {
                let mut state = lock(&self.barrier);
                if state.active {
                    state.active = false;
                }
                std::mem::take(&mut state.waiters)
            };
            for waker in wake {
                waker.wake();
            }
        }
    }
}

/// A prompt claim which must be released by every failed acceptance path.
pub struct PromptClaim<'a> {
    lifecycle: &'a LocalSessionLifecycle,
    session: SessionId,
    generation: ResourceGeneration,
    request: RequestId,
    armed: bool,
    committed: bool,
    newly_reserved: bool,
}

impl PromptClaim<'_> {
    pub fn new(
        lifecycle: &LocalSessionLifecycle,
        session: SessionId,
        generation: ResourceGeneration,
        request: RequestId,
        newly_reserved: bool,
    ) -> PromptClaim<'_> {
        PromptClaim {
            lifecycle,
            session,
            generation,
            request,
            armed: true,
            committed: false,
            newly_reserved,
        }
    }

    pub fn disarm(&mut self) {
        self.armed = false;
    }

    pub fn generation(&self) -> ResourceGeneration {
        self.generation
    }

    pub fn committed(&mut self) {
        self.committed = true;
    }

    pub fn is_committed(&self) -> bool {
        self.committed
    }
}

impl Drop for PromptClaim<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.lifecycle
            .clear_prompt_claim(&self.session, self.generation, &self.request);
        if self.newly_reserved && !self.committed {
            let mut state = lock(&self.lifecycle.state);
            if let Some(entry) = state.entries.get_mut(&self.session)
                && entry.generation == self.generation
                && entry.phase == Phase::Inactive
                && !entry.resource_unconfirmed
            {
                entry.reserved = false;
            }
        }
    }
}

impl SessionLifecycleStateGuard {
    pub fn generation(&self) -> Option<ResourceGeneration> {
        self.generation
    }

    pub fn delete_pending(&self) -> bool {
        self.delete_pending
    }

    pub fn gate(&self) -> &SessionLifecycleGate {
        &self.gate
    }
}

impl LocalSessionLifecycle {
    /// 构造。目录初始为空且**未收敛**：组合根必须先调用 [`Self::converge_on_startup`]。
    pub fn new(
        clock: Arc<dyn Clock>,
        store: Arc<dyn SessionStore>,
        content: Arc<dyn SessionContentFacts>,
        publisher: Arc<dyn EventPublisher>,
        config: LifecycleConfig,
    ) -> Self {
        Self {
            clock,
            store,
            content,
            publisher,
            release: std::sync::OnceLock::new(),
            config: Mutex::new(config),
            state: Mutex::new(DirectoryState::default()),
            gates: SessionGates::default(),
            delete_barriers: DeleteBarriers::default(),
            converged: AtomicBool::new(false),
            next_epoch: AtomicU64::new(1),
        }
    }

    /// 接线逐会话释放端口（`agent-host` 的真实 ACP close 实现）。重复接线保留首次实现。
    pub fn set_session_release(&self, release: Arc<dyn SessionRelease>) -> bool {
        self.release.set(release).is_ok()
    }

    /// 当前运行期配置。
    pub fn config(&self) -> LifecycleConfig {
        *lock(&self.config)
    }

    /// 运行期调整。调低上限**不强杀**受保护会话：只让后续准入被拒，直到计数回落（§5.10）。
    pub fn set_config(&self, config: LifecycleConfig) {
        *lock(&self.config) = config;
    }

    pub fn is_converged(&self) -> bool {
        self.converged.load(Ordering::SeqCst)
    }

    fn ensure_converged(&self) -> Result<(), PortError> {
        if self.is_converged() {
            Ok(())
        } else {
            Err(PortError::Unavailable(UnavailableKind::Busy))
        }
    }

    fn now(&self) -> Timestamp {
        self.clock.now()
    }

    /// 取得某条 owned 会话的 Owner 生命周期门。
    ///
    /// 组合层的「真实释放 + 最终删除事务」必须持有同一个 [`SessionLifecycleGate`] 直到事务返回，
    /// 否则释放完成后到达的 `session.view` 或 `session.resume` 可能看到尚可读的空会话并重新占用资源。
    pub async fn acquire_owner_lifecycle_gate(&self, session: &SessionId) -> SessionLifecycleGate {
        self.gates.acquire(session.clone()).await
    }

    /// 取得删除门和同一个运行时目录快照。调用方必须把这个 guard 一直持有到删除事务结束，
    /// 才能防止资源释放之后的新激活穿插进来。
    pub async fn acquire_owner_lifecycle_state(
        &self,
        session: &SessionId,
    ) -> SessionLifecycleStateGuard {
        let gate = self.acquire_owner_lifecycle_gate(session).await;
        let entry = lock(&self.state).entries.get(session).cloned();
        SessionLifecycleStateGuard {
            gate,
            generation: entry
                .as_ref()
                .filter(|entry| entry.reserved || entry.generation.get() > 0)
                .map(|entry| entry.generation),
            delete_pending: entry
                .as_ref()
                .is_some_and(|entry| entry.phase == Phase::DeletePending),
        }
    }

    async fn ensure_session_visible(&self, session: &SessionId) -> Result<(), PortError> {
        if self.store.load(session).await?.is_some() {
            Ok(())
        } else {
            Err(PortError::NotFound(EntityRef::Session(session.clone())))
        }
    }

    // -------------------------------------------------------------------------------------
    // §4 用例面：每个入口先取 actor，再委托同名端口方法（授权仍由命令路径的 broker 判定）。
    // -------------------------------------------------------------------------------------

    /// `session.resume` 的激活入口（同会话并发合并、ready 复用、稳定 requestId）。
    pub async fn activate_session(
        &self,
        actor: &Actor,
        request: ActivationRequest,
    ) -> Result<ActivationPlan, PortError> {
        let gate = self.acquire_owner_lifecycle_gate(&request.session).await;
        self.ensure_session_visible(&request.session).await?;
        let plan = self.plan_activation(&request).await?;
        if !matches!(plan, ActivationPlan::CapacityExceeded) {
            return Ok(plan);
        }
        drop(gate);
        // 满额先按 LRU 回收一个**未受保护**的 ready 会话；只有适配器确认 `Released` 才重试准入一次。
        // 拒绝前回收是容量策略的一半（`sessions.idle_timeout_ms = 0` 只关闭按时间回收，不影响这里）。
        if !self.reclaim_capacity_once(actor).await? {
            return Ok(plan);
        }
        let _gate = self.acquire_owner_lifecycle_gate(&request.session).await;
        self.ensure_session_visible(&request.session).await?;
        self.plan_activation(&request).await
    }

    /// 当前是否已占满容量上限（周期回收只在满额时消费 LRU 容量候选；有空位时不得提前释放会话）。
    pub fn has_capacity_pressure(&self) -> bool {
        let max = self.config().max_active_sessions() as usize;
        lock(&self.state)
            .entries
            .values()
            .filter(|entry| entry.reserved)
            .count()
            >= max
    }

    /// Read facts outside the lifecycle gate so a yielding store operation
    /// cannot deadlock against acceptance on the same session.
    pub async fn session_activity_facts(
        &self,
        session: &SessionId,
    ) -> Result<SessionActivityFacts, PortError> {
        self.store.session_activity(session).await
    }

    /// Snapshot the generation for diagnostics and tests without exposing runtime
    /// mutation through an unlockable accessor.
    pub fn resource_generation(&self, session: &SessionId) -> Option<ResourceGeneration> {
        lock(&self.state)
            .entries
            .get(session)
            .filter(|entry| entry.reserved || entry.generation.get() > 0)
            .map(|entry| entry.generation)
    }

    pub fn claim_delete_lease_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        generation: ResourceGeneration,
    ) -> Result<DeleteLease, PortError> {
        self.ensure_gate(gate, session)?;
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return Err(PortError::NotFound(EntityRef::Session(session.clone())));
        };
        if entry.generation != generation
            || entry.phase != Phase::Reclaiming
            || entry.delete_intent.is_none()
            || entry.delete_lease.is_some()
            || entry.active_prompt.is_some()
        {
            return Err(PortError::Unavailable(UnavailableKind::Busy));
        }
        entry.delete_lease = Some(generation);
        entry.active_prompt = None;
        Ok(DeleteLease {
            session: session.clone(),
            generation,
            delete_epoch: entry.delete_epoch,
            epoch: entry.epoch,
            gate: None,
        })
    }

    pub fn has_delete_lease_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        generation: ResourceGeneration,
    ) -> Result<bool, PortError> {
        self.ensure_gate(gate, session)?;
        Ok(lock(&self.state).entries.get(session).is_some_and(|entry| {
            entry.generation == generation
                && entry.phase == Phase::Reclaiming
                && entry.delete_lease == Some(generation)
        }))
    }

    pub async fn release_session_for_delete(
        &self,
        lease: &DeleteLease,
        actor: &Actor,
        session: SessionId,
        generation: ResourceGeneration,
        reserved_at_claim: bool,
        now: Timestamp,
    ) -> Result<ReleaseOutcome, PortError> {
        let gate = match lease.gate.as_ref() {
            Some(gate) => gate.clone_lifecycle_handle(),
            None => self.acquire_owner_lifecycle_gate(&session).await,
        };
        self.ensure_gate(&gate, &session)?;
        if lease.session != session
            || lease.generation != generation
            || lock(&self.state).entries.get(&session).is_none_or(|entry| {
                entry.epoch != lease.epoch || entry.delete_epoch != lease.delete_epoch
            })
        {
            return Ok(ReleaseOutcome::Failed);
        }
        if !self.has_delete_lease_locked(&gate, &session, generation)? {
            return Ok(ReleaseOutcome::Failed);
        }
        if !reserved_at_claim
            && let Some(entry) = lock(&self.state).entries.get_mut(&session)
            && entry.generation == generation
            && entry.phase == Phase::Inactive
            && !entry.reserved
            && entry.restart_seeded
            && entry.delete_lease == Some(generation)
        {
            entry.phase = Phase::Reclaiming;
            entry.reserved = true;
        }
        let outcome = self
            .release_session_for_delete_locked(&gate, actor, session, generation, now)
            .await?;
        drop(gate);
        Ok(outcome)
    }

    pub fn continue_deletion_after_release_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        generation: ResourceGeneration,
        outcome: ReleaseOutcome,
        now: Timestamp,
    ) -> Result<(), PortError> {
        self.ensure_gate(gate, session)?;
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return Ok(());
        };
        if entry.generation != generation || entry.delete_lease != Some(generation) {
            return Ok(());
        }
        entry.delete_lease = None;
        entry.last_protected_at = now;
        if !matches!(
            outcome,
            ReleaseOutcome::Released | ReleaseOutcome::AlreadyReleased
        ) {
            entry.phase = match outcome {
                ReleaseOutcome::Failed => Phase::Inactive,
                ReleaseOutcome::Released => Phase::DeletePending,
                ReleaseOutcome::AlreadyReleased => entry.phase,
                ReleaseOutcome::Uncertain => Phase::Ready,
            };
            entry.resource_unconfirmed = outcome == ReleaseOutcome::Uncertain;
            entry.reserved = outcome == ReleaseOutcome::Uncertain;
            entry.restart_seeded = false;
            entry.retention_start = None;
        }
        Ok(())
    }

    /// Ends a failed prompt claim without changing content or runtime
    /// capacity. Activation and deletion retry after they observe the lease.
    pub fn discard_delete_lease_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        generation: ResourceGeneration,
    ) -> Result<(), PortError> {
        self.ensure_gate(gate, session)?;
        if let Some(entry) = lock(&self.state).entries.get_mut(session)
            && entry.generation == generation
            && entry.phase == Phase::Reclaiming
            && !entry.reserved
            && entry.resource_unconfirmed
        {
            entry.phase = Phase::Inactive;
            entry.reserved = false;
            entry.delete_lease = None;
        }
        Ok(())
    }

    pub async fn finish_delete_in_lifecycle_gate(
        &self,
        core: &UseCases,
        gate: SessionLifecycleGate,
        write: SessionDeletionWrite,
    ) -> Result<SessionDeletionOutcome, PortError> {
        self.ensure_gate(&gate, write.session())?;
        core.finish_delete_in_held_lifecycle_gate_locked(self, gate, write)
            .await
    }

    /// 容量满时回收一个安全候选并交还名额；返回是否真的释放成功。
    ///
    /// 没有释放端口（未接线/纯目录测试）、没有安全候选、或释放结果不是 `Released` 时返回 `false`：
    /// 调用方据此保留原来的容量拒绝，不伪造「已腾出名额」。
    pub(crate) async fn reclaim_capacity_once(&self, actor: &Actor) -> Result<bool, PortError> {
        if self.release.get().is_none() {
            return Ok(false);
        }
        let now = self.now();
        let Some(candidate) = self.reclaim_candidate(actor, now).await? else {
            return Ok(false);
        };
        Ok(matches!(
            self.release_capacity_candidate(actor, &candidate, self.now())
                .await?,
            ReleaseOutcome::Released
        ))
    }

    /// `session.create` 的资源入口：与恢复**共用**容量名额与资源代。
    ///
    /// 创建路径的幂等记录由 `session.create` 自己的受理提交承担（本入口只查运行期目录，不读
    /// `(actor, requestId)`），因此这里与 [`Self::activate_session`] 的差别只有「不查持久记录」。
    /// 调用方拿到 [`ActivationPlan::Establish`] 后启动后端，再用 [`Self::settle_session`] 收口；
    /// 满额返回 [`ActivationPlan::CapacityExceeded`]（适配层映射 `session.capacity_exceeded`）。
    pub async fn begin_created_session(
        &self,
        actor: &Actor,
        session: SessionId,
        now: Timestamp,
    ) -> Result<ActivationPlan, PortError> {
        let gate = self.acquire_owner_lifecycle_gate(&session).await;
        let plan = self.begin_created_session_in_gate(&gate, &session, now)?;
        if !matches!(plan, ActivationPlan::CapacityExceeded) {
            return Ok(plan);
        }
        // 与恢复同一口径：满额先回收一个未受保护的 ready 会话，`Released` 后重试准入一次。
        drop(gate);
        if !self.reclaim_capacity_once(actor).await? {
            return Ok(plan);
        }
        let gate = self.acquire_owner_lifecycle_gate(&session).await;
        self.begin_created_session_in_gate(&gate, &session, self.now())
    }

    /// 完成一次激活（`session.resume` 的终态/恢复适配器回调）。
    pub async fn settle_session(
        &self,
        _actor: &Actor,
        session: SessionId,
        generation: ResourceGeneration,
        outcome: ActivationOutcome,
    ) -> Result<(), PortError> {
        let _gate = self.acquire_owner_lifecycle_gate(&session).await;
        self.settle_if_visible(session, generation, outcome).await
    }

    async fn settle_if_visible(
        &self,
        session: SessionId,
        generation: ResourceGeneration,
        outcome: ActivationOutcome,
    ) -> Result<(), PortError> {
        if self.store.load(&session).await?.is_none() {
            // 删除事务提交后，旧 generation 的迟到结果只能清理运行期残留，不能复活 tombstone。
            lock(&self.state).entries.remove(&session);
            return Ok(());
        }
        self.settle_impl(session, generation, outcome).await
    }

    /// 在 `session.create` 的事务提交前标记本代资源为 Ready。
    ///
    /// 创建命令的会话行与终态在同一事务里落盘；适配层需要在那次提交成功后立即广播结果。若等到广播
    /// 之后才把生命周期改为 Ready，实时摘要会在两件事实之间短暂投影为 `Idle`。本入口只更新内存，
    /// 不写事件；事务失败时调用方必须用 [`Self::rollback_created_session_ready`] 恢复本代。
    pub fn settle_created_session_ready(
        &self,
        _actor: &Actor,
        session: &SessionId,
        generation: ResourceGeneration,
    ) -> Result<(), PortError> {
        self.settle_created_session_ready_locked(session, generation)
    }

    /// 在已持有的同一会话生命周期门内，于创建最终事务前标记 Ready。
    pub fn settle_created_session_ready_locked(
        &self,
        session: &SessionId,
        generation: ResourceGeneration,
    ) -> Result<(), PortError> {
        self.ensure_converged()?;
        let now = self.now();
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return Ok(());
        };
        if entry.generation != generation || entry.phase != Phase::Activating {
            return Ok(());
        }
        entry.phase = Phase::Ready;
        entry.established = true;
        entry.reserved = true;
        entry.resource_unconfirmed = false;
        entry.failure = None;
        entry.settled = Some(generation);
        entry.last_protected_at = now.clone();
        entry.retention_start = Some(now);
        Ok(())
    }

    /// 删除最终事务已接受本代：同时结束删除态与资源占位。
    /// 调用方持有同一会话的 [`SessionLifecycleGate`]。
    pub fn complete_deleted_session_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        generation: ResourceGeneration,
    ) -> Result<(), PortError> {
        self.ensure_gate(gate, session)?;
        let mut state = lock(&self.state);
        if state.entries.get(session).is_some_and(|entry| {
            entry.phase == Phase::DeletePending && entry.generation == generation
        }) {
            state.entries.remove(session);
        }
        Ok(())
    }

    /// 撤销尚未落盘成功的创建 Ready 标记；只处理同一资源代，迟到的回滚不得影响新代。
    /// 仅撤销同一代、尚未落库的创建 Ready 标记。调用方须持有创建会话的门。
    pub fn rollback_created_session_ready_locked(
        &self,
        session: &SessionId,
        generation: ResourceGeneration,
    ) {
        let now = self.now();
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return;
        };
        if entry.generation != generation || entry.phase != Phase::Ready {
            return;
        }
        entry.phase = Phase::Activating;
        entry.established = false;
        entry.reserved = true;
        entry.resource_unconfirmed = false;
        entry.failure = None;
        entry.settled = None;
        entry.retention_start = None;
        entry.last_protected_at = now;
    }

    pub async fn declare_view(
        &self,
        _actor: &Actor,
        connection: ConnectionId,
        session: Option<SessionId>,
        view_version: u64,
        now: Timestamp,
    ) -> Result<ViewDecision, PortError> {
        loop {
            let locked_session = match session.as_ref() {
                Some(session) => Some(session.clone()),
                None => lock(&self.state)
                    .views
                    .get(&connection)
                    .and_then(|view| view.session.clone()),
            };
            let gate = match locked_session.as_ref() {
                Some(session) => Some(self.acquire_owner_lifecycle_gate(session).await),
                None => None,
            };
            if let Some(session) = session.as_ref() {
                self.ensure_session_visible(session).await?;
            } else if let Some(locked_session) = locked_session.as_ref() {
                // 等待门期间连接可能切到别的会话；只让仍与取得门时一致的声明继续。
                let unchanged = lock(&self.state)
                    .views
                    .get(&connection)
                    .and_then(|view| view.session.as_ref())
                    == Some(locked_session);
                if !unchanged {
                    drop(gate);
                    continue;
                }
            }
            let decision = self
                .declare_impl(connection, session, view_version, now)
                .await?;
            drop(gate);
            return Ok(decision);
        }
    }

    pub async fn renew_view(
        &self,
        _actor: &Actor,
        connection: ConnectionId,
        view_version: u64,
        now: Timestamp,
    ) -> Result<ViewDecision, PortError> {
        loop {
            let Some(session) = lock(&self.state)
                .views
                .get(&connection)
                .and_then(|view| view.session.clone())
            else {
                return self.renew_impl(connection, view_version, now).await;
            };
            let gate = self.acquire_owner_lifecycle_gate(&session).await;
            let unchanged = lock(&self.state)
                .views
                .get(&connection)
                .and_then(|view| view.session.as_ref())
                == Some(&session);
            if !unchanged {
                drop(gate);
                continue;
            }
            self.ensure_session_visible(&session).await?;
            let decision = self.renew_impl(connection, view_version, now).await;
            drop(gate);
            return decision;
        }
    }

    pub async fn release_view(
        &self,
        _actor: &Actor,
        connection: ConnectionId,
        now: Timestamp,
    ) -> Result<(), PortError> {
        loop {
            let session = lock(&self.state)
                .views
                .get(&connection)
                .and_then(|view| view.session.clone());
            let Some(session) = session else {
                lock(&self.state).views.remove(&connection);
                return Ok(());
            };
            let gate = self.acquire_owner_lifecycle_gate(&session).await;
            let unchanged = lock(&self.state)
                .views
                .get(&connection)
                .and_then(|view| view.session.as_ref())
                == Some(&session);
            if !unchanged {
                drop(gate);
                continue;
            }
            let result = self.release_viewer_impl(connection, now).await;
            drop(gate);
            return result;
        }
    }

    /// 会话存在正在收尾的删除事务或输入尚在受理时拒绝激活。调用方须持有该会话的生命周期门。
    pub fn ensure_no_delete_in_progress_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
    ) -> Result<(), PortError> {
        self.ensure_gate(gate, session)?;
        let mut state = lock(&self.state);
        if state.entries.get(session).is_some_and(|entry| {
            entry.phase == Phase::DeletePending
                || entry.delete_intent.is_some()
                || entry.active_prompt.is_some()
        }) {
            return Err(PortError::Unavailable(UnavailableKind::Busy));
        }
        if let Some(entry) = state.entries.get_mut(session) {
            entry.delete_lease = None;
        }
        Ok(())
    }

    /// Marks this claim as the one-time prompt which takes an empty session
    /// out of its deletion grace period. The marker is cleared only by the
    /// durable prompt commit, so a failed acceptance cannot later remove a
    /// concurrently protected session.
    pub(crate) fn begin_prompt_in_gate(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        request: RequestId,
    ) -> Result<ResourceGeneration, PortError> {
        self.ensure_gate(gate, session)?;
        self.ensure_no_delete_in_progress_locked(gate, session)?;
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return Err(PortError::NotFound(EntityRef::Session(session.clone())));
        };
        entry.active_prompt = Some(request);
        entry.last_protected_at = self.now();
        entry.retention_start = None;
        Ok(entry.generation)
    }

    /// Applies protection only when the prompt commit which observed this
    /// generation is the pending one. A late commit cannot claim a newer one.
    pub(crate) fn complete_prompt_in_gate(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        request: &RequestId,
    ) -> Result<(), PortError> {
        self.ensure_gate(gate, session)?;
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return Ok(());
        };
        if entry.active_prompt.as_ref() == Some(request) {
            entry.active_prompt = None;
            entry.last_protected_at = self.now();
            entry.retention_start = None;
        }
        Ok(())
    }

    /// Finishes a committed prompt only if the exact delete intent it
    /// replaced is still current. A newer deletion request keeps its result.
    pub fn complete_prompt_after_delete_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        request: &RequestId,
        delete_generation: Option<ResourceGeneration>,
    ) -> Result<(), PortError> {
        self.ensure_gate(gate, session)?;
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return Ok(());
        };
        if let Some(generation) = delete_generation
            && entry
                .delete_intent
                .as_ref()
                .is_some_and(|(intent_generation, _)| *intent_generation == generation)
        {
            entry.delete_intent = None;
            entry.delete_epoch = None;
            if entry.phase == Phase::DeletePending && entry.generation == generation {
                entry.phase = Phase::Inactive;
                entry.reserved = false;
                entry.restart_seeded = false;
                entry.retention_start = None;
            }
        }
        if entry.active_prompt.as_ref() == Some(request) {
            entry.active_prompt = None;
            entry.last_protected_at = self.now();
            entry.retention_start = None;
        }
        Ok(())
    }

    /// A prompt rejected by deletion owns only its provisional runtime claim;
    /// end that claim and its exact delete intent together.
    pub fn abandon_prompt_after_delete_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        request: &RequestId,
        delete_generation: Option<ResourceGeneration>,
    ) -> Result<(), PortError> {
        self.clear_prompt_claim(
            session,
            delete_generation.ok_or(PortError::InvalidRequest(
                "prompt cleanup is only for a lifecycle prompt claim",
            ))?,
            request,
        );
        if let Some(generation) = delete_generation {
            self.complete_prompt_after_delete_locked(gate, session, request, Some(generation))?;
        }
        Ok(())
    }

    pub fn clear_prompt_claim(
        &self,
        session: &SessionId,
        generation: ResourceGeneration,
        request: &RequestId,
    ) {
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return;
        };
        if entry.generation != generation {
            return;
        }
        if entry.active_prompt.as_ref() == Some(request) {
            entry.active_prompt = None;
        }
        if entry.phase == Phase::Reclaiming
            && !entry.reserved
            && entry.resource_unconfirmed
            && entry.delete_lease.is_none()
        {
            entry.phase = Phase::Inactive;
            entry.reserved = false;
        }
    }

    /// Returns true when a delayed release is still allowed to close exactly
    /// this runtime incarnation.
    pub fn delete_release_lease_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        generation: ResourceGeneration,
        epoch: Option<u64>,
        delete_epoch: u64,
    ) -> Result<bool, PortError> {
        self.ensure_gate(gate, session)?;
        Ok(lock(&self.state).entries.get(session).is_some_and(|entry| {
            entry.generation == generation
                && entry.epoch == epoch
                && entry.delete_epoch == Some(delete_epoch)
                && entry.phase == Phase::Reclaiming
                && entry.delete_lease == Some(generation)
        }))
    }

    pub fn delete_epoch_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        generation: ResourceGeneration,
        request: &RequestId,
    ) -> Result<Option<u64>, PortError> {
        self.ensure_gate(gate, session)?;
        Ok(lock(&self.state).entries.get(session).and_then(|entry| {
            entry
                .delete_intent
                .as_ref()
                .and_then(|(intent_generation, intent_request)| {
                    (*intent_generation == generation && intent_request == request)
                        .then_some(entry.delete_epoch)
                        .flatten()
                })
        }))
    }

    pub fn delete_epoch_is_current_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        generation: ResourceGeneration,
        request: &RequestId,
        delete_epoch: u64,
    ) -> Result<bool, PortError> {
        self.ensure_gate(gate, session)?;
        Ok(lock(&self.state).entries.get(session).is_some_and(|entry| {
            entry.generation == generation
                && entry.delete_epoch == Some(delete_epoch)
                && entry.delete_intent.as_ref().is_some_and(
                    |(intent_generation, intent_request)| {
                        *intent_generation == generation && intent_request == request
                    },
                )
        }))
    }

    /// Passes an already-held lifecycle claim to every step of one deletion.
    /// The adapter may await Agent I/O while holding it; activation therefore
    /// waits for the final tombstone instead of entering the release window.
    pub fn hold_delete_lease_gate(
        &self,
        lease: &mut DeleteLease,
        gate: SessionLifecycleGate,
    ) -> Result<(), PortError> {
        self.ensure_gate(&gate, &lease.session)?;
        if lease.gate.is_some() {
            return Err(PortError::InvalidRequest(
                "the delete lease already holds a lifecycle gate",
            ));
        }
        lease.gate = Some(gate);
        Ok(())
    }

    /// Releases a failed deletion's runtime claim while the adapter still
    /// owns the lifecycle gate, and publishes its durable failed terminal.
    #[allow(clippy::too_many_arguments)] // Binds the exact request to the resource generation under the held gate.
    pub async fn finish_failed_delete_in_held_lifecycle_gate_locked(
        &self,
        core: &UseCases,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        actor: &Actor,
        request: &RequestId,
        _generation: ResourceGeneration,
        _delete_epoch: Option<u64>,
        error: PublicError,
    ) -> Result<(), PortError> {
        self.ensure_gate(gate, session)?;
        core.settle_delete_in_held_lifecycle_gate_locked(
            self,
            gate.clone_lifecycle_handle(),
            session,
            actor,
            request,
            CommandStatus::Failed,
            Some(error),
        )
        .await?;
        Ok(())
    }

    pub fn runtime_epoch_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        generation: ResourceGeneration,
    ) -> Result<Option<u64>, PortError> {
        self.ensure_gate(gate, session)?;
        Ok(lock(&self.state).entries.get(session).and_then(|entry| {
            (entry.generation == generation)
                .then_some(entry.epoch)
                .flatten()
        }))
    }

    pub fn bind_delete_epoch_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        generation: ResourceGeneration,
        request: RequestId,
    ) -> Result<u64, PortError> {
        self.ensure_gate(gate, session)?;
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return Err(PortError::NotFound(EntityRef::Session(session.clone())));
        };
        if let Some((intent_generation, intent_request)) = &entry.delete_intent {
            if *intent_generation == generation && intent_request == &request {
                return entry.delete_epoch.ok_or(PortError::Corrupt(
                    "a bound delete intent must retain its delete epoch",
                ));
            }
            return Err(PortError::Unavailable(UnavailableKind::Busy));
        }
        if entry.phase == Phase::DeletePending || entry.generation != generation {
            return Err(PortError::Unavailable(UnavailableKind::Busy));
        }
        entry.active_prompt = None;
        entry.delete_intent = Some((generation, request));
        let epoch = self.next_epoch.fetch_add(1, Ordering::Relaxed);
        entry.delete_epoch = Some(epoch);
        entry.last_protected_at = self.now();
        entry.retention_start = None;
        Ok(epoch)
    }

    pub fn has_delete_intent(&self, session: &SessionId) -> bool {
        lock(&self.state)
            .entries
            .get(session)
            .is_some_and(|entry| entry.delete_intent.is_some())
    }

    pub fn release_prompt_claim(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        generation: ResourceGeneration,
        request: &RequestId,
    ) -> Result<(), PortError> {
        self.ensure_gate(gate, session)?;
        self.clear_prompt_claim(session, generation, request);
        Ok(())
    }

    /// Checks deletion and capacity. The broker rechecks the delete intent
    /// after its asynchronous read and before the durable prompt commit.
    pub fn reserve_owner_prompt_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        now: Timestamp,
    ) -> Result<Option<(ResourceGeneration, bool)>, PortError> {
        self.ensure_gate(gate, session)?;
        self.ensure_no_delete_in_progress_locked(gate, session)?;
        let max = self.config().max_active_sessions() as usize;
        let mut state = lock(&self.state);
        let reserved = state
            .entries
            .values()
            .filter(|entry| entry.reserved)
            .count();
        let Some(entry) = state.entries.get_mut(session) else {
            return Ok(None);
        };
        let newly_reserved = !entry.reserved;
        if !entry.reserved {
            if reserved >= max {
                return Err(PortError::Unavailable(UnavailableKind::Busy));
            }
            entry.reserved = true;
        }
        entry.last_protected_at = now;
        entry.retention_start = None;
        Ok(Some((entry.generation, newly_reserved)))
    }

    pub async fn reserve_capacity(
        &self,
        _actor: &Actor,
        session: SessionId,
        now: Timestamp,
    ) -> Result<Admission, PortError> {
        self.reserve(session, now).await
    }

    pub async fn release_capacity(
        &self,
        _actor: &Actor,
        session: SessionId,
        generation: ResourceGeneration,
        now: Timestamp,
    ) -> Result<(), PortError> {
        self.release_capacity_impl(session, generation, now).await
    }

    pub async fn reclaim_candidate(
        &self,
        _actor: &Actor,
        now: Timestamp,
    ) -> Result<Option<ReclaimCandidate>, PortError> {
        CapacityAdmission::reclaim_candidate(self, now).await
    }

    /// 该会话当前资源代（运行期事实；`None` = 目录里没有该会话，或本进程从未建立过资源）。
    ///
    /// 组合根持有容量候选/释放路径时用它取得与候选同一代的标识；查看者、租约与资源代都不出现在
    /// 快照或事件里，因此这是唯一的读取入口。
    pub fn resource_generation_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
    ) -> Result<Option<ResourceGeneration>, PortError> {
        self.ensure_gate(gate, session)?;
        Ok(lock(&self.state)
            .entries
            .get(session)
            .filter(|entry| entry.reserved || entry.generation.get() > 0)
            .map(|entry| entry.generation))
    }

    /// 读取保留状态快照。调用方仍必须在取得同一会话 gate 后通过
    /// [`Self::retention_snapshot_locked`] 复核。
    pub fn retention_snapshot(&self, session: &SessionId) -> Option<RetentionSnapshot> {
        lock(&self.state)
            .entries
            .get(session)
            .map(|entry| RetentionSnapshot {
                generation: entry.generation,
                retention_start: entry.retention_start.clone(),
                delete_pending: entry.phase == Phase::DeletePending,
            })
    }

    /// 在调用方已取得的 gate 内复核保留状态快照。
    pub fn retention_snapshot_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
    ) -> Result<Option<RetentionSnapshot>, PortError> {
        self.ensure_gate(gate, session)?;
        Ok(self.retention_snapshot(session))
    }

    /// 在调用方已取得的 gate 内重新判断保留候选是否仍然到期。
    ///
    /// 该检查同时观察查看者、未结束任务与未解决交互；因此候选生成后的激活或保护变化会使旧删除
    /// 意图失效。`Reclaiming` 会直接成为重试候选，后续每次扫描都必须使用新的命令 requestId。
    pub async fn retention_candidate_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        now: &Timestamp,
    ) -> Result<Option<ReclaimCandidate>, PortError> {
        self.ensure_gate(gate, session)?;
        self.observe_and_due(session, now, Some(self.config()))
            .await
    }

    /// 按保留期候选执行一次逐会话释放。
    ///
    /// 候选只是扫描时的快照；进入 Owner 门后必须重新判断当前保留期仍未到期、查看者与任务保护仍全部
    /// 解除。这样候选生成后被激活、查看或写入任务时，旧候选不会越权释放。
    pub async fn release_retention_candidate(
        &self,
        actor: &Actor,
        candidate: &ReclaimCandidate,
        now: Timestamp,
    ) -> Result<ReleaseOutcome, PortError> {
        self.release_candidate_checked(actor, candidate, now, Some(self.config()))
            .await
    }

    /// 按容量候选执行一次逐会话释放。容量回收不受保留期限制，但仍必须在门内复核查看者与任务保护。
    pub async fn release_capacity_candidate(
        &self,
        actor: &Actor,
        candidate: &ReclaimCandidate,
        now: Timestamp,
    ) -> Result<ReleaseOutcome, PortError> {
        self.release_candidate_checked(actor, candidate, now, None)
            .await
    }

    async fn release_candidate_checked(
        &self,
        actor: &Actor,
        candidate: &ReclaimCandidate,
        now: Timestamp,
        timeout: Option<LifecycleConfig>,
    ) -> Result<ReleaseOutcome, PortError> {
        let gate = self.acquire_owner_lifecycle_gate(&candidate.session).await;
        let Some(current) = self
            .observe_and_due(&candidate.session, &now, timeout)
            .await?
        else {
            return Ok(ReleaseOutcome::Failed);
        };
        if current.session != candidate.session {
            return Ok(ReleaseOutcome::Failed);
        }
        let Some(generation) = self.resource_generation_locked(&gate, &candidate.session)? else {
            return Ok(ReleaseOutcome::Failed);
        };
        self.release_session_locked(&gate, actor, candidate.session.clone(), generation, now)
            .await
    }

    /// 逐会话释放确认：只有适配器返回 `Released` 才交还名额并把归约收敛为 `Idle`。
    ///
    /// 并发的重复释放共享同一 `reclaiming`：适配器返回后只有当前仍为 `reclaiming` 且资源代未变时才应用
    /// `Failed`/`Uncertain`；已被另一次调用确认 `Released` 的会话不会因迟到的失败结果回到占位状态。
    pub async fn release_session(
        &self,
        actor: &Actor,
        session: SessionId,
        generation: ResourceGeneration,
        now: Timestamp,
    ) -> Result<ReleaseOutcome, PortError> {
        let _gate = self.acquire_owner_lifecycle_gate(&session).await;
        self.release_session_locked(&_gate, actor, session, generation, now)
            .await
    }

    /// 在调用方已取得的 Owner 生命周期门内执行释放。
    ///
    /// `gate` 必须属于同一条会话。删除适配层用它把真实释放和后续 tombstone 事务放在同一个不可插入区间。
    pub async fn release_session_locked(
        &self,
        gate: &SessionLifecycleGate,
        actor: &Actor,
        session: SessionId,
        generation: ResourceGeneration,
        now: Timestamp,
    ) -> Result<ReleaseOutcome, PortError> {
        if gate.session() != &session {
            return Err(PortError::InvalidRequest(
                "the lifecycle gate belongs to another session",
            ));
        }
        self.release_session_in_gate(actor, session, generation, now)
            .await
    }

    /// 删除路径的专用释放入口。
    ///
    /// 首次调用在适配器确认 `Released` 后进入 [`Phase::DeletePending`]；后续调用看到同一代
    /// `DeletePending` 时直接返回 `Released`，不会再次关闭已经释放的资源。
    /// Establishes the durable deletion intent while the same lifecycle gate
    /// covers eligibility, command admission and block on all new prompts.
    pub fn begin_delete_intent_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        generation: ResourceGeneration,
        request: RequestId,
        allow_existing: bool,
    ) -> Result<(), PortError> {
        self.bind_delete_intent_locked(gate, session, generation, request, allow_existing)
            .map(|_| ())
    }

    /// Binds a delete request to the exact runtime generation or returns the
    /// existing intent only when it belongs to the same request.
    ///
    /// A local lifecycle is runtime-only. Startup recovery cannot match an
    /// in-memory intent from a previous process, so each new request receives
    /// its own intent and delete epoch.
    pub fn bind_delete_intent_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        generation: ResourceGeneration,
        request: RequestId,
        _allow_existing: bool,
    ) -> Result<(ResourceGeneration, RequestId), PortError> {
        self.ensure_gate(gate, session)?;
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return Err(PortError::NotFound(EntityRef::Session(session.clone())));
        };
        if let Some((intent_generation, intent_request)) = &entry.delete_intent {
            if *intent_generation == generation && intent_request == &request {
                return Ok((*intent_generation, intent_request.clone()));
            }
            return Err(PortError::Unavailable(UnavailableKind::Busy));
        }
        if entry.generation != generation {
            return Err(PortError::Unavailable(UnavailableKind::Busy));
        }
        entry.delete_intent = Some((generation, request.clone()));
        entry.delete_epoch = Some(self.next_epoch.fetch_add(1, Ordering::Relaxed));
        Ok((generation, request))
    }

    pub fn delete_intent_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
    ) -> Result<Option<(ResourceGeneration, RequestId)>, PortError> {
        self.ensure_gate(gate, session)?;
        Ok(lock(&self.state)
            .entries
            .get(session)
            .and_then(|entry| entry.delete_intent.as_ref())
            .cloned())
    }

    pub fn delete_intent_generation_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
    ) -> Result<Option<ResourceGeneration>, PortError> {
        Ok(self
            .delete_intent_locked(gate, session)?
            .map(|(generation, _)| generation))
    }

    /// Ends one deletion intent. An older intent cannot clear a newer one.
    pub fn clear_delete_intent_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        generation: ResourceGeneration,
    ) -> Result<(), PortError> {
        self.clear_exact_delete_intent_locked(gate, session, generation, None)
    }

    /// Clears only the request which owns this intent. A same-generation
    /// successor remains protected while an older execution unwinds.
    pub fn clear_exact_delete_intent_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        generation: ResourceGeneration,
        request: Option<&RequestId>,
    ) -> Result<(), PortError> {
        self.ensure_gate(gate, session)?;
        let mut state = lock(&self.state);
        if let Some(entry) = state.entries.get_mut(session)
            && entry
                .delete_intent
                .as_ref()
                .is_some_and(|(intent_generation, intent_request)| {
                    *intent_generation == generation
                        && request.is_none_or(|request| request == intent_request)
                })
        {
            entry.delete_intent = None;
            entry.delete_epoch = None;
        }
        Ok(())
    }

    pub async fn release_session_for_delete_locked(
        &self,
        gate: &SessionLifecycleGate,
        actor: &Actor,
        session: SessionId,
        generation: ResourceGeneration,
        now: Timestamp,
    ) -> Result<ReleaseOutcome, PortError> {
        self.ensure_gate(gate, &session)?;
        if self
            .delete_pending_generation_locked(gate, &session)?
            .is_some_and(|pending| pending == generation)
        {
            return Ok(ReleaseOutcome::AlreadyReleased);
        }
        self.release_session_in_gate_with(
            actor,
            session,
            generation,
            now,
            ReleaseTarget::DeletePending,
        )
        .await
    }

    /// 在生命周期 gate 内读取待删除资源代。扫描器用它证明候选仍对应同一删除意图。
    pub fn delete_pending_generation_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
    ) -> Result<Option<ResourceGeneration>, PortError> {
        self.ensure_gate(gate, session)?;
        let state = lock(&self.state);
        Ok(state
            .entries
            .get(session)
            .and_then(|entry| (entry.phase == Phase::DeletePending).then_some(entry.generation)))
    }

    /// tombstone 已提交：清除运行期目录项。
    pub fn complete_delete_pending_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        generation: ResourceGeneration,
    ) -> Result<(), PortError> {
        self.ensure_gate(gate, session)?;
        let mut state = lock(&self.state);
        if state.entries.get(session).is_some_and(|entry| {
            entry.phase == Phase::DeletePending && entry.generation == generation
        }) {
            state.entries.remove(session);
        }
        Ok(())
    }

    /// 删除事务明确失败或资格已变化：退出待删除态，后续激活可正常建立新资源。
    pub fn cancel_delete_pending_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        generation: ResourceGeneration,
    ) -> Result<bool, PortError> {
        self.ensure_gate(gate, session)?;
        let mut state = lock(&self.state);
        if let Some(entry) = state.entries.get_mut(session) {
            if entry.phase == Phase::DeletePending && entry.generation == generation {
                entry.phase = Phase::Inactive;
                entry.reserved = false;
                entry.restart_seeded = false;
                entry.retention_start = None;
                if entry
                    .delete_intent
                    .as_ref()
                    .is_some_and(|(intent_generation, _)| *intent_generation == generation)
                {
                    entry.delete_intent = None;
                    entry.delete_epoch = None;
                }
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn ensure_gate(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
    ) -> Result<(), PortError> {
        if gate.session() != session {
            return Err(PortError::InvalidRequest(
                "the lifecycle gate belongs to another session",
            ));
        }
        Ok(())
    }

    /// Records a deletion candidate as reclaiming after task and content
    /// eligibility were checked in this gate. The durable delete intent keeps
    /// every activation and later prompt waiting until release settles.
    pub fn begin_delete_release_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        generation: ResourceGeneration,
    ) -> Result<(), PortError> {
        self.ensure_gate(gate, session)?;
        self.ensure_converged()?;
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return Err(PortError::NotFound(EntityRef::Session(session.clone())));
        };
        if entry.generation == generation
            && entry.phase == Phase::Inactive
            && !entry.reserved
            && entry.restart_seeded
            && entry.delete_intent.is_some()
        {
            entry.reserved = true;
        }
        if entry.generation != generation || !entry.reserved {
            return Err(PortError::Unavailable(UnavailableKind::Busy));
        }
        if entry.delete_intent.is_none() {
            return Err(PortError::Unavailable(UnavailableKind::Busy));
        }
        let restart_seeded = entry.restart_seeded && entry.phase == Phase::Inactive;
        match entry.phase {
            Phase::Ready | Phase::Reclaiming => entry.phase = Phase::Reclaiming,
            Phase::Activating if entry.resource_unconfirmed => entry.phase = Phase::Reclaiming,
            Phase::Inactive if restart_seeded => {}
            Phase::Activating | Phase::Inactive | Phase::DeletePending => {
                return Err(PortError::Unavailable(UnavailableKind::Busy));
            }
        }
        Ok(())
    }

    /// Releases the runtime-generation claim and publishes the failed delete
    /// terminal while the same lifecycle gate is held. This keeps a prompt
    /// that was blocked during preflight from entering between the two steps.
    #[allow(clippy::too_many_arguments)] // Binds the exact request to the resource generation under the held gate.
    pub async fn fail_delete_release_in_gate(
        &self,
        core: &UseCases,
        gate: SessionLifecycleGate,
        session: &SessionId,
        _generation: ResourceGeneration,
        _delete_epoch: Option<u64>,
        actor: &Actor,
        request: &RequestId,
        error: PublicError,
    ) -> Result<(), PortError> {
        let gate_for_settlement = gate.clone_lifecycle_handle();
        core.settle_delete_in_held_lifecycle_gate_locked(
            self,
            gate_for_settlement,
            session,
            actor,
            request,
            CommandStatus::Failed,
            Some(error),
        )
        .await?;
        // The command is now terminal. A later retry is a distinct request
        // and must not remain permanently blocked by this failed attempt.
        if self.has_delete_intent(session) {
            self.fail_delete_release_current_in_gate(&gate, session)?;
        }
        Ok(())
    }

    pub fn begin_seeded_delete_release_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        generation: ResourceGeneration,
    ) -> Result<(), PortError> {
        self.ensure_gate(gate, session)?;
        self.ensure_converged()?;
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return Err(PortError::NotFound(EntityRef::Session(session.clone())));
        };
        if entry.generation != generation
            || entry.phase != Phase::Inactive
            || entry.reserved
            || !entry.restart_seeded
        {
            return Err(PortError::Unavailable(UnavailableKind::Busy));
        }
        entry.phase = Phase::Reclaiming;
        entry.reserved = true;
        Ok(())
    }

    /// Confirms that the read-only eligibility and activity checks still
    /// describe the generation whose delete lease was claimed.
    pub fn confirm_release_in_gate(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        generation: ResourceGeneration,
        release_observed: bool,
        now: &Timestamp,
    ) -> Result<bool, PortError> {
        self.ensure_gate(gate, session)?;
        let state = lock(&self.state);
        Ok(state.entries.get(session).is_some_and(|entry| {
            entry.generation == generation
                && entry.phase == Phase::Reclaiming
                && entry.delete_lease == Some(generation)
                && entry.delete_intent.is_some()
                && entry.active_prompt.is_none()
                && (release_observed || !self.has_live_viewer(session, now))
        }))
    }

    /// Converts the deletion intent into the released state only when the
    /// released resource is exactly the one that was admitted for deletion.
    pub fn enter_delete_pending_locked(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        generation: ResourceGeneration,
    ) -> Result<(), PortError> {
        self.ensure_gate(gate, session)?;
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return Err(PortError::NotFound(EntityRef::Session(session.clone())));
        };
        if entry.generation != generation
            || entry
                .delete_intent
                .as_ref()
                .is_none_or(|(intent_generation, _)| *intent_generation != generation)
        {
            return Err(PortError::Unavailable(UnavailableKind::Busy));
        }
        entry.phase = Phase::DeletePending;
        entry.reserved = false;
        entry.resource_unconfirmed = false;
        entry.failure = None;
        entry.restart_seeded = false;
        entry.retention_start = None;
        Ok(())
    }

    async fn release_session_in_gate(
        &self,
        actor: &Actor,
        session: SessionId,
        generation: ResourceGeneration,
        now: Timestamp,
    ) -> Result<ReleaseOutcome, PortError> {
        self.release_session_in_gate_with(actor, session, generation, now, ReleaseTarget::Inactive)
            .await
    }

    async fn release_session_in_gate_with(
        &self,
        _actor: &Actor,
        session: SessionId,
        generation: ResourceGeneration,
        now: Timestamp,
        target: ReleaseTarget,
    ) -> Result<ReleaseOutcome, PortError> {
        self.ensure_converged()?;
        let Some(release) = self.release.get().cloned() else {
            return Err(PortError::Unavailable(UnavailableKind::BackendUnsupported));
        };
        let facts = match self.store.session_activity(&session).await {
            Ok(facts) => facts,
            Err(PortError::NotFound(_)) => {
                lock(&self.state).entries.remove(&session);
                return Ok(ReleaseOutcome::Failed);
            }
            Err(error) => return Err(error),
        };
        if self.has_live_viewer(&session, &now)
            || facts.unfinished_turns > 0
            || facts.unresolved_interactions > 0
        {
            return Ok(ReleaseOutcome::Failed);
        }
        // 先进入 `reclaiming`：关闭中的资源不得再被新的激活当成可复用资源（禁止交错产生两个绑定）。
        {
            let mut state = lock(&self.state);
            let Some(entry) = state.entries.get_mut(&session) else {
                return Ok(ReleaseOutcome::Failed);
            };
            if entry.generation != generation {
                // 旧代结果不得绑定到新代资源（§3.3）。
                return Ok(ReleaseOutcome::Failed);
            }
            let restart_seeded = entry.restart_seeded && entry.phase == Phase::Inactive;
            match entry.phase {
                Phase::Ready | Phase::Reclaiming => entry.phase = Phase::Reclaiming,
                Phase::Activating if entry.resource_unconfirmed => entry.phase = Phase::Reclaiming,
                // 启动种子没有本进程可复用的运行时，但仍必须请求释放端口确认「没有资源可关」。
                // 释放完成前后都保持 Inactive，避免空会话在静默删除前短暂投影为 active。
                Phase::Inactive if restart_seeded => {}
                Phase::DeletePending => return Ok(ReleaseOutcome::Failed),
                Phase::Activating | Phase::Inactive => return Ok(ReleaseOutcome::Failed),
            }
            entry.reserved = true;
        }
        let before = self.activity_for(&session).await?;
        let outcome = release
            .release_session(session.clone(), generation, now.clone())
            .await?;
        // 适配器 await 期间可能已有另一次调用完成释放（或新一代资源已建立）：迟到的
        // `Failed`/`Uncertain` 不得把已确认释放的会话改回 `reclaiming`，也不得重新占位。
        // 只有当前仍处于本次 `reclaiming` 且资源代未变时才能应用失败/不确定结果。
        let applied = {
            let mut state = lock(&self.state);
            let Some(entry) = state.entries.get_mut(&session) else {
                return Ok(outcome);
            };
            if entry.generation != generation {
                false
            } else {
                match outcome {
                    ReleaseOutcome::Released | ReleaseOutcome::AlreadyReleased => {
                        let has_delete_intent = entry
                            .delete_intent
                            .as_ref()
                            .is_some_and(|(intent_generation, _)| *intent_generation == generation);
                        let released_phase = match target {
                            ReleaseTarget::Inactive => Phase::Inactive,
                            ReleaseTarget::DeletePending if has_delete_intent => {
                                Phase::DeletePending
                            }
                            ReleaseTarget::DeletePending => entry.phase,
                        };
                        let changed = entry.phase != released_phase
                            || entry.reserved
                            || entry.resource_unconfirmed;
                        if target == ReleaseTarget::DeletePending && !has_delete_intent {
                            entry.phase = Phase::Reclaiming;
                        } else {
                            entry.phase = released_phase;
                        }
                        entry.reserved = false;
                        entry.resource_unconfirmed = false;
                        entry.failure = None;
                        entry.restart_seeded = false;
                        entry.retention_start = None;
                        entry.last_protected_at = now.clone();
                        changed
                    }
                    ReleaseOutcome::Failed | ReleaseOutcome::Uncertain => {
                        if entry.phase != Phase::Reclaiming {
                            // 已被另一次调用置为 `Inactive`（或新一代已开始建立）：保持现状。
                            false
                        } else {
                            entry.phase = Phase::Reclaiming;
                            entry.reserved = true;
                            entry.resource_unconfirmed = true;
                            entry.failure = None;
                            true
                        }
                    }
                }
            }
        };
        if !applied {
            // 归约值不因迟到的结果而改变：不重复提交 `session.activity.changed`。
            return Ok(outcome);
        }
        let after = self.activity_for(&session).await?;
        self.sync_activity(&session, before, after, now).await?;
        Ok(outcome)
    }

    pub async fn current_activity(
        &self,
        _actor: &Actor,
        session: SessionId,
    ) -> Result<LocalActivity, PortError> {
        LifecycleQuery::local_activity(self, session).await
    }

    pub async fn lifecycle_snapshot(
        &self,
        _actor: &Actor,
    ) -> Result<Vec<SessionSummary>, PortError> {
        LifecycleQuery::snapshot(self).await
    }

    /// `localActivity` 与快照**同一代目录**的批量投影（适配层据此补键，不得逐条另开目录快照）。
    pub async fn activity_snapshot(
        &self,
        _actor: &Actor,
    ) -> Result<Vec<LocalSessionView>, PortError> {
        self.ensure_converged()?;
        let summaries = self.store.list(SessionQuery::default()).await?;
        let phases = {
            let state = lock(&self.state);
            summaries
                .iter()
                .map(|summary| {
                    state
                        .entries
                        .get(summary.session_id())
                        .map(|entry| (entry.phase, entry.resource_unconfirmed))
                        .unwrap_or((Phase::Inactive, false))
                })
                .collect::<Vec<_>>()
        };
        let mut out = Vec::with_capacity(summaries.len());
        for (summary, (phase, unconfirmed)) in summaries.into_iter().zip(phases) {
            let facts = self.store.session_activity(summary.session_id()).await?;
            let activity = reduce(phase, unconfirmed, &facts);
            out.push(LocalSessionView { summary, activity });
        }
        Ok(out)
    }

    /// 保留期到期的候选（`sessions.idle_timeout_ms`；显式 `0` 时为空）。
    ///
    /// 候选只来自「未受保护且已 ready」的会话：无查看者、无未结束/排队任务、无有效交互、无停止未确认
    /// 的执行（后者仍表现为未结束 turn）。返回按 `last_protected_at` 升序、同一时刻按稳定会话标识。
    pub async fn retention_due(
        &self,
        _actor: &Actor,
        now: Timestamp,
    ) -> Result<Vec<ReclaimCandidate>, PortError> {
        self.ensure_converged()?;
        let ready = {
            let state = lock(&self.state);
            state
                .entries
                .iter()
                .filter(|(_, entry)| {
                    entry.phase == Phase::DeletePending
                        || (entry.phase == Phase::Reclaiming && entry.reserved)
                        || entry.restart_seeded
                        || (entry.phase == Phase::Ready && entry.reserved)
                })
                .map(|(session, entry)| (session.clone(), entry.retention_start.clone()))
                .collect::<Vec<_>>()
        };
        let config = self.config();
        if !config.time_based_reclaim_enabled() {
            let mut pending = Vec::new();
            for (session, _) in ready {
                let state = lock(&self.state);
                let Some(entry) = state.entries.get(&session) else {
                    continue;
                };
                let delete_pending =
                    entry.phase == Phase::DeletePending && entry.retention_start.is_some();
                let reclaiming = entry.phase == Phase::Reclaiming && entry.reserved;
                if delete_pending || reclaiming {
                    pending.push(ReclaimCandidate {
                        session: session.clone(),
                        last_protected_at: entry.last_protected_at.clone(),
                        delete_pending,
                    });
                }
            }
            sort_candidates(&mut pending);
            return Ok(pending);
        }
        let mut due = Vec::new();
        for (session, _) in ready {
            if let Some(candidate) = self.observe_and_due(&session, &now, Some(config)).await? {
                due.push(candidate);
            }
        }
        sort_candidates(&mut due);
        Ok(due)
    }

    /// 归约结果可能因任务事实变化而改变时由组合根调用（例如 broker 提交了新的 pending 交互）。
    ///
    /// 只在归约值**实际变化**时写 `session.activity.changed` 并发布；返回值是当前值。
    pub async fn refresh_activity(&self, session: &SessionId) -> Result<LocalActivity, PortError> {
        self.ensure_converged()?;
        let after = self.activity_for(session).await?;
        let before = {
            let state = lock(&self.state);
            state
                .entries
                .get(session)
                .map(|entry| entry.last_activity)
                .unwrap_or(LocalActivity::Idle)
        };
        let now = self.now();
        self.sync_activity(session, before, after, now).await?;
        Ok(after)
    }

    /// 启动收敛（`docs/CORE_PORTS_AND_STORAGE.md` §6 第 23 条）。
    ///
    /// 顺序：① 清空所有运行资源、资源代与查看租约（本进程一个都不信任）；② 读取每个会话最近一次**已
    /// 发布**的 `localActivity`；③ 按当前任务事实为「曾发布非 `Idle`、现已失效」的会话提交一次收敛事件；
    /// ④ 标记收敛完成，之后才开放查询/准入。返回提交的收敛事件数。
    ///
    /// 收敛**不**触碰任何 Agent、不重派 `uncertain` 的恢复：它只做状态收敛（§6 第 23 条）。
    pub async fn converge_on_startup(&self) -> Result<usize, PortError> {
        self.converged.store(false, Ordering::SeqCst);
        {
            let mut state = lock(&self.state);
            state.entries.clear();
            state.views.clear();
        }
        let now = self.now();
        let config = self.config();
        let seed_empty_sessions = config.time_based_reclaim_enabled();
        let published = self.published_activities().await?;
        let summaries = self.store.list(SessionQuery::default()).await?;
        let mut emitted = 0_usize;
        for summary in summaries {
            let session = summary.session_id();
            let empty = match self.content.has_content(session).await {
                Ok(has_content) => !has_content,
                Err(PortError::NotFound(_)) => continue,
                Err(error) => return Err(error),
            };
            let restart_deadline = if seed_empty_sessions && empty {
                Some(RestartEmptySessionDeadline::from_restart(
                    now.clone(),
                    config.idle_timeout_ms(),
                )?)
            } else {
                None
            };
            let facts = self.store.session_activity(session).await?;
            let activity = reduce(Phase::Inactive, false, &facts);
            {
                let mut state = lock(&self.state);
                let entry = state
                    .entries
                    .entry(session.clone())
                    .or_insert_with(|| Entry::new(now.clone()));
                entry.phase = Phase::Inactive;
                entry.generation = if restart_deadline.is_some() {
                    ResourceGeneration::new(1)
                } else {
                    ResourceGeneration::new(0)
                };
                entry.reserved = false;
                entry.resource_unconfirmed = false;
                entry.failure = None;
                entry.restart_seeded = restart_deadline.is_some();
                entry.retention_start =
                    restart_deadline.map(|deadline| deadline.started_at().clone());
                entry.last_protected_at = now.clone();
                entry.last_activity = activity;
            }
            if let Some(previous) = published.get(session) {
                if *previous != activity {
                    self.emit_activity(session, activity, now.clone()).await?;
                    emitted += 1;
                }
            }
        }
        self.converged.store(true, Ordering::SeqCst);
        Ok(emitted)
    }

    // -------------------------------------------------------------------------------------
    // 端口实现（目录 + 准入 + 查看租约）
    // -------------------------------------------------------------------------------------

    async fn plan_activation(
        &self,
        request: &ActivationRequest,
    ) -> Result<ActivationPlan, PortError> {
        self.ensure_converged()?;
        if let Some(admission) = self.durable_admission(request).await? {
            return Ok(admission);
        }
        self.begin_establishment(&request.session, self.now())
    }

    /// 运行期目录上的建立权分配（同步：只改内存、不持锁等待 I/O）。
    ///
    /// 恢复与创建共用：ready 复用、同会话合并、容量预占与资源代分配都只在这里发生一次。
    fn begin_establishment(
        &self,
        session: &SessionId,
        now: Timestamp,
    ) -> Result<ActivationPlan, PortError> {
        let mut state = lock(&self.state);
        self.begin_establishment_locked(&mut state, session, now)
    }

    /// Runtime admission when the caller already holds the same lifecycle gate.
    fn begin_establishment_locked(
        &self,
        state: &mut DirectoryState,
        session: &SessionId,
        now: Timestamp,
    ) -> Result<ActivationPlan, PortError> {
        let max = self.config().max_active_sessions();
        let reserved = state
            .entries
            .values()
            .filter(|entry| entry.reserved)
            .count();
        let entry = state
            .entries
            .entry(session.clone())
            .or_insert_with(|| Entry::new(now.clone()));
        if entry.delete_intent.is_some()
            && matches!(entry.phase, Phase::Reclaiming | Phase::DeletePending)
        {
            return Err(PortError::Unavailable(UnavailableKind::Busy));
        }
        match entry.phase {
            // An accepted prompt reserves the runtime before its durable commit.
            // Do not report an activation as complete during that window.
            Phase::Ready if entry.active_prompt.is_some() => {
                return Ok(ActivationPlan::InFlight);
            }
            // ready 复用：不新建资源代、不预占新名额、不触发 ACP。
            Phase::Ready => {
                entry.last_protected_at = now;
                entry.retention_start = None;
                return Ok(ActivationPlan::Reused {
                    generation: entry.generation,
                });
            }
            // 建立中/回收中：不得启动第二次建立（关闭中禁止恢复与回收交错）。
            Phase::Activating | Phase::Reclaiming => return Ok(ActivationPlan::InFlight),
            // 删除事务尚未提交时，激活在同一生命周期 gate 内先取得所有权并取消 pending；
            // 此时资源已经确认释放，因此分配新代是安全的，也不会重复 close。
            Phase::Inactive | Phase::DeletePending => {}
        }
        if !entry.reserved {
            if reserved >= max as usize {
                return Ok(ActivationPlan::CapacityExceeded);
            }
            entry.reserved = true;
        }
        let Some(next) = entry.generation.checked_next() else {
            return Err(PortError::Corrupt("资源代已溢出"));
        };
        entry.generation = next;
        entry.epoch = Some(self.next_epoch.fetch_add(1, Ordering::Relaxed));
        entry.phase = Phase::Activating;
        entry.settled = None;
        entry.resource_unconfirmed = false;
        entry.failure = None;
        entry.restart_seeded = false;
        entry.retention_start = None;
        entry.last_protected_at = now;
        Ok(ActivationPlan::Establish { generation: next })
    }

    /// 创建入口在同一个生命周期门内取得资源代，删除请求在此期间无法插入。
    pub fn begin_created_session_in_gate(
        &self,
        gate: &SessionLifecycleGate,
        session: &SessionId,
        now: Timestamp,
    ) -> Result<ActivationPlan, PortError> {
        self.ensure_converged()?;
        self.ensure_gate(gate, session)?;
        let mut state = lock(&self.state);
        self.begin_establishment_locked(&mut state, session, now)
    }

    /// 先查持久幂等记录：accepted（含窗口期）→ `InFlight`；终态 → `Settled`；命令/指纹/会话冲突 →
    /// `command.idempotency_conflict`。`uncertain` 是终态，因此绝不会被本路径换号重派。
    async fn durable_admission(
        &self,
        request: &ActivationRequest,
    ) -> Result<Option<ActivationPlan>, PortError> {
        let Some(record) = self
            .store
            .find_request(&request.request_id, &request.actor)
            .await?
        else {
            return Ok(None);
        };
        if record.command() != "session.resume"
            || record.request_fingerprint() != &request.request_fingerprint
            || record.session() != Some(&request.session)
        {
            return Err(PortError::Conflict(ConflictKind::IdempotencyConflict));
        }
        Ok(Some(match record.status() {
            CommandStatus::Accepted => ActivationPlan::InFlight,
            _ => ActivationPlan::Settled(record),
        }))
    }

    async fn settle_impl(
        &self,
        session: SessionId,
        generation: ResourceGeneration,
        outcome: ActivationOutcome,
    ) -> Result<(), PortError> {
        self.ensure_converged()?;
        let now = self.now();
        let phases = {
            let state = lock(&self.state);
            let Some(entry) = state.entries.get(&session) else {
                // 未知/更早代：不得把结果绑定到新代资源。
                return Ok(());
            };
            if entry.generation != generation || entry.settled == Some(generation) {
                return Ok(());
            }
            (entry.phase, entry.resource_unconfirmed)
        };
        let facts = self.store.session_activity(&session).await?;
        let before = reduce(phases.0, phases.1, &facts);
        let after = {
            let mut state = lock(&self.state);
            let Some(entry) = state.entries.get_mut(&session) else {
                return Ok(());
            };
            if entry.generation != generation || entry.settled == Some(generation) {
                return Ok(());
            }
            match outcome {
                ActivationOutcome::Ready => {
                    entry.phase = Phase::Ready;
                    entry.established = true;
                    entry.reserved = true;
                    entry.resource_unconfirmed = false;
                    entry.failure = None;
                    entry.settled = Some(generation);
                    entry.last_protected_at = now.clone();
                    // 「恢复完成时若已无人查看则从完成时开始完整保留期」：查看者/任务仍会取消它。
                    entry.retention_start = Some(now.clone());
                }
                ActivationOutcome::Failed { error } => {
                    entry.phase = Phase::Inactive;
                    entry.established = false;
                    entry.reserved = false;
                    entry.resource_unconfirmed = false;
                    entry.failure = Some(LifecycleFailure {
                        error,
                        uncertain: false,
                    });
                    entry.settled = Some(generation);
                    entry.retention_start = None;
                }
                ActivationOutcome::Uncertain { error } => {
                    // 结果不确定：外部资源可能存在，保留容量占位与 `Active` 事实；后续显式核实/释放前
                    // 新的激活一律 `InFlight`（`uncertain` 不自动换号重派）。
                    entry.phase = Phase::Activating;
                    entry.established = true;
                    entry.reserved = true;
                    entry.resource_unconfirmed = true;
                    entry.failure = Some(LifecycleFailure {
                        error,
                        uncertain: true,
                    });
                    entry.settled = Some(generation);
                }
            }
            reduce(entry.phase, entry.resource_unconfirmed, &facts)
        };
        self.sync_activity(&session, before, after, now.clone())
            .await?;
        // Ready 之后按「此刻是否仍受保护」修正保留起点（查看者/任务会取消计时）。
        if matches!(self.entry_phase(&session), Some(Phase::Ready)) {
            self.observe_protection(&session, &now).await?;
        }
        Ok(())
    }

    async fn declare_impl(
        &self,
        connection: ConnectionId,
        session: Option<SessionId>,
        view_version: u64,
        now: Timestamp,
    ) -> Result<ViewDecision, PortError> {
        let Some(expires_at) = now.checked_add_millis(VIEW_LEASE_MS) else {
            return Err(PortError::InvalidRequest("查看租约到期时刻溢出"));
        };
        let mut lost: Option<SessionId> = None;
        let decision = {
            let mut state = lock(&self.state);
            match state.views.get(&connection).cloned() {
                None => {
                    state.views.insert(
                        connection,
                        ViewState {
                            session: session.clone(),
                            view_version,
                            expires_at: session.as_ref().map(|_| expires_at.clone()),
                        },
                    );
                    ViewDecision::accepted(expires_at)
                }
                Some(mut current) => {
                    let live = current.is_live(&now);
                    if view_version < current.view_version {
                        // 过期版本不复活旧页面，也不清除现有查看者。
                        ViewDecision::stale()
                    } else if view_version == current.view_version {
                        if session.is_none() && current.session.is_none() {
                            // 同版本离页（重复的离页声明）→ 幂等接受；不再表示仍在查看，
                            // 因此也不建立/刷新任何保护。
                            current.expires_at = None;
                            state.views.insert(connection, current);
                            ViewDecision::accepted(expires_at)
                        } else if live && current.session.as_ref() == session.as_ref() {
                            // 同版本、同载荷（仍在查看同一会话）→ 幂等续租。
                            current.expires_at = Some(expires_at.clone());
                            state.views.insert(connection, current);
                            ViewDecision::accepted(expires_at)
                        } else {
                            // 同版本、载荷已变化（含 A→null、Some→None、已离开又挑同一会话）→
                            // 状态机已分叉，不猜、不替换、不替换租期；过期租约上的同版本声明
                            // 同样不能复活页面。真正的离页/切换必须先递增版本。
                            ViewDecision::stale()
                        }
                    } else {
                        // 更高版本：替换选择（A→B、A→null、null→A 都必须递增版本）。
                        lost = current.session.clone();
                        state.views.insert(
                            connection,
                            ViewState {
                                session: session.clone(),
                                view_version,
                                expires_at: session.as_ref().map(|_| expires_at.clone()),
                            },
                        );
                        ViewDecision::accepted(expires_at)
                    }
                }
            }
        };
        if let Some(lost) = lost {
            self.observe_protection(&lost, &now).await?;
        }
        // 只有被接受的声明才建立/刷新查看保护：`stale` 表示本连接的状态机已分叉，请求方
        // 并未真正成为该会话的查看者，因此不得延长保留期或取消它的计时。
        if decision.is_accepted() {
            if let Some(session) = &session {
                self.note_viewer(session, &now);
            }
        }
        Ok(decision)
    }

    async fn renew_impl(
        &self,
        connection: ConnectionId,
        view_version: u64,
        now: Timestamp,
    ) -> Result<ViewDecision, PortError> {
        let Some(expires_at) = now.checked_add_millis(VIEW_LEASE_MS) else {
            return Err(PortError::InvalidRequest("查看租约到期时刻溢出"));
        };
        let session = {
            let mut state = lock(&self.state);
            let Some(mut current) = state.views.get(&connection).cloned() else {
                return Ok(ViewDecision::stale());
            };
            if view_version != current.view_version || !current.is_live(&now) {
                // 低于/高于当前版本、或租约已过期：都不续租，更不能复活页面。
                return Ok(ViewDecision::stale());
            }
            let Some(session) = current.session.clone() else {
                return Ok(ViewDecision::stale());
            };
            current.expires_at = Some(expires_at.clone());
            state.views.insert(connection, current);
            session
        };
        self.note_viewer(&session, &now);
        Ok(ViewDecision::accepted(expires_at))
    }

    async fn release_viewer_impl(
        &self,
        connection: ConnectionId,
        now: Timestamp,
    ) -> Result<(), PortError> {
        let lost = {
            let mut state = lock(&self.state);
            match state.views.remove(&connection) {
                Some(current) => current.session,
                None => None,
            }
        };
        if let Some(session) = lost {
            self.observe_protection(&session, &now).await?;
        }
        Ok(())
    }

    async fn viewers_impl(
        &self,
        session: SessionId,
        now: Timestamp,
    ) -> Result<Vec<ConnectionId>, PortError> {
        let state = lock(&self.state);
        let mut viewers = state
            .views
            .iter()
            .filter(|(_, view)| view.is_live(&now) && view.session.as_ref() == Some(&session))
            .map(|(connection, _)| connection.clone())
            .collect::<Vec<_>>();
        viewers.sort();
        Ok(viewers)
    }

    async fn reserve_impl(
        &self,
        session: SessionId,
        now: Timestamp,
    ) -> Result<Admission, PortError> {
        self.ensure_converged()?;
        let max = self.config().max_active_sessions();
        let mut state = lock(&self.state);
        let reserved = state
            .entries
            .values()
            .filter(|entry| entry.reserved)
            .count();
        let entry = state
            .entries
            .entry(session)
            .or_insert_with(|| Entry::new(now.clone()));
        if entry.reserved {
            return Ok(Admission::Reserved);
        }
        if reserved >= max as usize {
            return Ok(Admission::CapacityExceeded);
        }
        entry.reserved = true;
        entry.last_protected_at = now;
        Ok(Admission::Reserved)
    }

    async fn release_capacity_impl(
        &self,
        session: SessionId,
        generation: ResourceGeneration,
        _now: Timestamp,
    ) -> Result<(), PortError> {
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(&session) else {
            return Ok(());
        };
        if entry.generation != generation {
            // 旧代不得释放新代名额（§3.3 的资源代隔离）。
            return Ok(());
        }
        match entry.phase {
            Phase::Ready | Phase::Reclaiming | Phase::DeletePending => Err(
                PortError::InvalidRequest("已建立/回收中的资源必须先经释放确认才能交还名额"),
            ),
            // `Uncertain` 的激活可能已经在外部建立了资源：名额必须保留到显式
            // `SessionRelease::release_session` 确认释放，不能在这里当作回滚放掉。
            Phase::Activating if entry.resource_unconfirmed => Err(PortError::InvalidRequest(
                "结果不确定的激活必须先经会话释放确认才能交还名额",
            )),
            Phase::Inactive | Phase::Activating => {
                entry.reserved = false;
                entry.phase = Phase::Inactive;
                entry.resource_unconfirmed = false;
                entry.failure = None;
                // 回滚后该代的迟到 settle 必须被忽略，不能在未重新预占的情况下把资源标成 ready
                // 并突破容量上限。
                entry.settled = Some(generation);
                Ok(())
            }
        }
    }

    async fn reclaim_candidate_impl(
        &self,
        now: Timestamp,
    ) -> Result<Option<ReclaimCandidate>, PortError> {
        self.ensure_converged()?;
        let ready = {
            let state = lock(&self.state);
            state
                .entries
                .iter()
                .filter(|(_, entry)| {
                    (entry.phase == Phase::Ready && entry.reserved)
                        || (entry.phase == Phase::Reclaiming && entry.reserved)
                })
                .filter(|(_, entry)| entry.established)
                .map(|(session, _)| session.clone())
                .collect::<Vec<_>>()
        };
        let mut candidates = Vec::new();
        for session in ready {
            // 容量驱逐可以早于保留期：不受 idle_timeout 限制，只看「是否受保护」。
            if let Some(candidate) = self.observe_and_due(&session, &now, None).await? {
                candidates.push(candidate);
            }
        }
        sort_candidates(&mut candidates);
        Ok(candidates.into_iter().next())
    }

    async fn local_activity_impl(&self, session: SessionId) -> Result<LocalActivity, PortError> {
        self.ensure_converged()?;
        self.activity_for(&session).await
    }

    async fn snapshot_impl(&self) -> Result<Vec<SessionSummary>, PortError> {
        self.ensure_converged()?;
        self.store.list(SessionQuery::default()).await
    }

    // -------------------------------------------------------------------------------------
    // 内部：事实读取、保护观察、事件提交
    // -------------------------------------------------------------------------------------

    async fn activity_for(&self, session: &SessionId) -> Result<LocalActivity, PortError> {
        let (phase, unconfirmed) = {
            let state = lock(&self.state);
            state
                .entries
                .get(session)
                .map(|entry| (entry.phase, entry.resource_unconfirmed))
                .unwrap_or((Phase::Inactive, false))
        };
        let facts = self.store.session_activity(session).await?;
        Ok(reduce(phase, unconfirmed, &facts))
    }

    fn entry_phase(&self, session: &SessionId) -> Option<Phase> {
        lock(&self.state)
            .entries
            .get(session)
            .map(|entry| entry.phase)
    }

    fn has_live_viewer(&self, session: &SessionId, now: &Timestamp) -> bool {
        lock(&self.state)
            .views
            .values()
            .any(|view| view.is_live(now) && view.session.as_ref() == Some(session))
    }

    fn note_viewer(&self, session: &SessionId, now: &Timestamp) {
        let mut state = lock(&self.state);
        let entry = state
            .entries
            .entry(session.clone())
            .or_insert_with(|| Entry::new(now.clone()));
        entry.last_protected_at = now.clone();
        entry.retention_start = None;
    }

    /// 观察一次保护状态并写入保留起点的变化（查看者增减、任务事实变化、激活完成）。
    async fn observe_protection(
        &self,
        session: &SessionId,
        now: &Timestamp,
    ) -> Result<(), PortError> {
        let facts = self.store.session_activity(session).await?;
        let has_viewer = self.has_live_viewer(session, now);
        let protected =
            has_viewer || facts.unresolved_interactions > 0 || facts.unfinished_turns > 0;
        let mut state = lock(&self.state);
        let entry = state
            .entries
            .entry(session.clone())
            .or_insert_with(|| Entry::new(now.clone()));
        if protected {
            entry.retention_start = None;
            entry.last_protected_at = now.clone();
        } else if entry.retention_start.is_none() {
            // 「所有保护解除后才开始完整保留期」：观察到的第一次无保护时刻就是起点。
            entry.retention_start = Some(now.clone());
        }
        Ok(())
    }

    /// 观察保护状态；`timeout = Some(config)` 时只在保留期已满后返回候选，`None` 时是容量驱逐
    /// （不受保留期限制）。
    async fn observe_and_due(
        &self,
        session: &SessionId,
        now: &Timestamp,
        timeout: Option<LifecycleConfig>,
    ) -> Result<Option<ReclaimCandidate>, PortError> {
        self.observe_protection(session, now).await?;
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return Ok(None);
        };
        let delete_pending = entry.phase == Phase::DeletePending;
        let reclaiming = entry.phase == Phase::Reclaiming && entry.reserved;
        if !delete_pending
            && !reclaiming
            && !entry.restart_seeded
            && (entry.phase != Phase::Ready || !entry.reserved)
        {
            return Ok(None);
        }
        // 已开始的释放重试不受保留期重新计时；每次扫描用新 requestId 尝试确认关闭，成功后才删除。
        if reclaiming {
            if entry.retention_start.is_none() {
                return Ok(None);
            }
            return Ok(Some(ReclaimCandidate {
                session: session.clone(),
                last_protected_at: entry.last_protected_at.clone(),
                delete_pending: false,
            }));
        }
        if delete_pending && entry.retention_start.is_none() {
            return Ok(None);
        }
        if let Some(config) = timeout.filter(|_| !delete_pending) {
            let Some(start) = entry.retention_start.clone() else {
                return Ok(None);
            };
            let Some(deadline) = start.checked_add_millis(config.idle_timeout_ms()) else {
                return Ok(None);
            };
            if now < &deadline {
                return Ok(None);
            }
        } else if entry.retention_start.is_none() {
            // 仍受保护：不是安全候选。
            return Ok(None);
        }
        Ok(Some(ReclaimCandidate {
            session: session.clone(),
            last_protected_at: entry.last_protected_at.clone(),
            delete_pending,
        }))
    }

    /// 若归约值实际变化：写一条持久 `session.activity.changed`，提交成功后才广播。
    async fn sync_activity(
        &self,
        session: &SessionId,
        before: LocalActivity,
        after: LocalActivity,
        at: Timestamp,
    ) -> Result<(), PortError> {
        if before == after {
            let mut state = lock(&self.state);
            if let Some(entry) = state.entries.get_mut(session) {
                entry.last_activity = after;
            }
            return Ok(());
        }
        // 先提交、后广播；失败时保留旧的 `last_activity`，让下一次观测仍能补发这一次变化。
        self.emit_activity(session, after, at).await?;
        let mut state = lock(&self.state);
        if let Some(entry) = state.entries.get_mut(session) {
            entry.last_activity = after;
        }
        Ok(())
    }

    async fn emit_activity(
        &self,
        session: &SessionId,
        activity: LocalActivity,
        at: Timestamp,
    ) -> Result<(), PortError> {
        let mut summaries = self
            .store
            .list(SessionQuery {
                only: Some(vec![session.clone()]),
                states: Vec::new(),
                limit: Some(1),
            })
            .await?;
        let Some(summary) = summaries.pop() else {
            // 会话行已不存在：没有可投影的摘要，不写悬空事件。
            return Ok(());
        };
        let view = session_summary_view(
            &summary,
            summary.state(),
            &summary.version(),
            summary.updated_at(),
            Some(activity),
        )?;
        let event = pending_event(
            LOCAL_ACTIVITY_EVENT,
            EventKind::State,
            view,
            None,
            None,
            StoredPolicy::Durable,
            None,
        )?;
        let commit = OwnedCommit {
            session: Some(session.clone()),
            at,
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![event],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
            input: None,
        };
        let outcome = self.store.commit(commit).await?;
        for event in outcome.appended {
            self.publisher.publish(event);
        }
        Ok(())
    }

    /// 每个会话最近一次**已发布**的 `localActivity`（启动收敛的唯一来源）。
    ///
    /// 只读 `sync_replay` 的持久正文；事件被保留策略裁掉时该会话不参与收敛（不猜、不推断）。
    async fn published_activities(&self) -> Result<HashMap<SessionId, LocalActivity>, PortError> {
        let view = self.store.read_view().await?;
        let limit = ReplayLimit::default();
        let mut after: Option<GlobalCursor> = None;
        let mut published = HashMap::new();
        loop {
            let slice = view.sync_replay(after.clone(), limit).await?;
            if slice.events.is_empty() {
                break;
            }
            let mut last_sequence = None;
            for record in &slice.events {
                last_sequence = Some(record.event.global_sequence);
                if record.event.event_type.as_str() != LOCAL_ACTIVITY_EVENT {
                    continue;
                }
                let (Some(session), Some(activity)) = (
                    &record.event.session,
                    local_activity_of(&record.payload.view),
                ) else {
                    continue;
                };
                published.insert(session.clone(), activity);
            }
            let Some(last_sequence) = last_sequence else {
                break;
            };
            if slice.events.len() < limit.events() as usize {
                break;
            }
            // 续读游标：本批最后一条的全局序号；epoch 取视图 head 的 epoch（同一只读视图内不变）。
            let next = GlobalCursor::new(slice.head.server_epoch.clone(), last_sequence);
            if after.as_ref() == Some(&next) {
                break;
            }
            after = Some(next);
        }
        Ok(published)
    }
}

fn reduce(phase: Phase, resource_unconfirmed: bool, facts: &SessionActivityFacts) -> LocalActivity {
    LocalActivity::reduce(LocalActivityFacts {
        pending_interaction: facts.unresolved_interactions > 0,
        unfinished_task: facts.unfinished_turns > 0,
        ready_resource: phase == Phase::Ready,
        release_unconfirmed: phase == Phase::Reclaiming || resource_unconfirmed,
    })
}

fn sort_candidates(candidates: &mut [ReclaimCandidate]) {
    candidates.sort_by(|left, right| {
        left.last_protected_at
            .cmp(&right.last_protected_at)
            .then_with(|| left.session.as_str().cmp(right.session.as_str()))
    });
}

/// 从一条 `session.activity.changed` 持久视图里读回 `localActivity`（只认 `view.session.localActivity`）。
fn local_activity_of(view: &ViewJson) -> Option<LocalActivity> {
    let members = crate::model::json_object_members(view.as_str()).ok()?;
    let session = members
        .iter()
        .find(|(name, _)| name == "session")
        .map(|(_, raw)| *raw)?;
    let members = crate::model::json_object_members(session).ok()?;
    let raw = members
        .iter()
        .find(|(name, _)| name == "localActivity")
        .map(|(_, raw)| *raw)?;
    let text = crate::model::decode_json_string(raw)?;
    text.parse().ok()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[async_trait]
impl SessionActivation for LocalSessionLifecycle {
    async fn activate(&self, request: ActivationRequest) -> Result<ActivationAdmission, PortError> {
        let _gate = self.acquire_owner_lifecycle_gate(&request.session).await;
        self.ensure_session_visible(&request.session).await?;
        match self.plan_activation(&request).await? {
            // `ActivationAdmission` 没有容量变体：该端口调用方 MUST 先经
            // `CapacityAdmission::reserve`/`reclaim_candidate` 取得明确拒绝；这里是防御性失败关闭
            // （不得静默当成 `Prepared`）。
            ActivationPlan::CapacityExceeded => Err(PortError::Unavailable(UnavailableKind::Busy)),
            plan => Ok(plan.admission()),
        }
    }

    async fn settle(
        &self,
        session: SessionId,
        generation: ResourceGeneration,
        outcome: ActivationOutcome,
    ) -> Result<(), PortError> {
        let _gate = self.acquire_owner_lifecycle_gate(&session).await;
        self.settle_if_visible(session, generation, outcome).await
    }
}

#[async_trait]
impl ViewRegistry for LocalSessionLifecycle {
    async fn declare(
        &self,
        connection: ConnectionId,
        session: Option<SessionId>,
        view_version: u64,
        now: Timestamp,
    ) -> Result<ViewDecision, PortError> {
        self.declare_view(&Actor::LocalCli, connection, session, view_version, now)
            .await
    }

    async fn renew(
        &self,
        connection: ConnectionId,
        view_version: u64,
        now: Timestamp,
    ) -> Result<ViewDecision, PortError> {
        self.renew_view(&Actor::LocalCli, connection, view_version, now)
            .await
    }

    async fn release(&self, connection: ConnectionId, now: Timestamp) -> Result<(), PortError> {
        self.release_view(&Actor::LocalCli, connection, now).await
    }

    async fn viewers(
        &self,
        session: SessionId,
        now: Timestamp,
    ) -> Result<Vec<ConnectionId>, PortError> {
        self.viewers_impl(session, now).await
    }
}

#[async_trait]
impl CapacityAdmission for LocalSessionLifecycle {
    async fn reserve(&self, session: SessionId, now: Timestamp) -> Result<Admission, PortError> {
        self.reserve_impl(session, now).await
    }

    async fn release(
        &self,
        session: SessionId,
        generation: ResourceGeneration,
        now: Timestamp,
    ) -> Result<(), PortError> {
        let _gate = self.acquire_owner_lifecycle_gate(&session).await;
        self.release_capacity_impl(session, generation, now).await
    }

    async fn reclaim_candidate(
        &self,
        now: Timestamp,
    ) -> Result<Option<ReclaimCandidate>, PortError> {
        self.reclaim_candidate_impl(now).await
    }
}

#[async_trait]
impl LifecycleQuery for LocalSessionLifecycle {
    async fn local_activity(&self, session: SessionId) -> Result<LocalActivity, PortError> {
        self.local_activity_impl(session).await
    }

    async fn snapshot(&self) -> Result<Vec<SessionSummary>, PortError> {
        self.snapshot_impl().await
    }
}

#[cfg(test)]
mod tests;
