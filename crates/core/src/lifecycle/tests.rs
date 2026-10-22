//! 本机生命周期的纯端口/并发/事件顺序测试（`docs/CORE_PORTS_AND_STORAGE.md` §5.10/§6 第 23 条，
//! spec `local-session-lifecycle` 与 `session-activity`）。
//!
//! 时间全部由 [`FakeClock`] 提供（不睡眠、不读系统时间）；存储与发布使用 broker 的内存替身
//! （`crate::broker::test_support`），因此这里断言的是 core 的判定与提交顺序，不是 SQLite 行为。

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use async_trait::async_trait;

use super::lock;
use super::*;
use crate::broker::test_support::*;
use crate::broker::{EPHEMERAL_EVENT_TYPES, HISTORY_ENTRY_EVENT_TYPES, persistence_policy};
use crate::model::{
    Actor, AgentId, AgentRef, ConnectionId, EventOrigin, EventType, InteractionId, InteractionKind,
    LocalActivity, LocalActivityFacts, OwnedSessionRef, PendingInteraction, PersistencePolicy,
    PortError, PublicError, RequestId, ResourceGeneration, ResourceOrigin, Session, SessionId,
    SessionState, Timestamp, Turn, TurnId, TurnState, Version,
};
use crate::ports::{
    ActivationOutcome, ActivationRequest, Admission, Clock, ReleaseOutcome, ReplayLimit,
    SessionActivityFacts, SessionRelease, SessionStore, ViewRegistry,
};

const BASE: &str = "2026-09-18T00:00:00.000Z";
const MINUTE_MS: u64 = 60_000;

fn at(millis: u64) -> Timestamp {
    Timestamp::new(BASE)
        .expect("valid base timestamp")
        .checked_add_millis(millis)
        .expect("timestamp in range")
}

struct FakeClock {
    millis: AtomicU64,
}

impl FakeClock {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            millis: AtomicU64::new(0),
        })
    }

    fn advance(&self, millis: u64) {
        self.millis.fetch_add(millis, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Timestamp {
        at(self.millis.load(Ordering::SeqCst))
    }
}

struct Fixture {
    world: Arc<FakeWorld>,
    store: Arc<FakeStore>,
    publisher: Arc<TestPublisher>,
    clock: Arc<FakeClock>,
}

impl Fixture {
    fn new() -> Self {
        let world = FakeWorld::new();
        Self {
            store: Arc::new(FakeStore {
                world: world.clone(),
            }),
            publisher: Arc::new(TestPublisher {
                world: world.clone(),
            }),
            clock: FakeClock::new(),
            world,
        }
    }

    fn lifecycle(&self, config: LifecycleConfig) -> LocalSessionLifecycle {
        LocalSessionLifecycle::new(
            self.clock.clone(),
            self.store.clone(),
            self.publisher.clone(),
            config,
        )
    }

    fn converged(&self, config: LifecycleConfig) -> LocalSessionLifecycle {
        let lifecycle = self.lifecycle(config);
        block_on(lifecycle.converge_on_startup()).expect("startup convergence");
        lifecycle
    }

    fn seed_session(&self, n: u64) -> SessionId {
        let session = SessionId::new(&uuid_text(n)).expect("session id");
        let agent = AgentRef::try_new(AgentId::new("agent-1").expect("agent id"), "Agent One")
            .expect("agent ref");
        let row = Session::try_new(
            session.clone(),
            OwnedSessionRef::new(session.clone()),
            None,
            None,
            agent,
            SessionState::Idle,
            ResourceOrigin::Local,
            None,
            Version::from(1),
            ts(0),
            ts(0),
        )
        .expect("session");
        self.world.seed_session(row);
        session
    }

    fn activation(&self, session: &SessionId, n: u64) -> ActivationRequest {
        ActivationRequest {
            actor: Actor::LocalCli,
            request_id: RequestId::new(&uuid_text(n)).expect("request id"),
            request_fingerprint: digest('A'),
            session: session.clone(),
        }
    }

    fn seed_turn(&self, session: &SessionId, n: u64, state: TurnState) {
        let turn = Turn::try_new(
            TurnId::new(&uuid_text(n)).expect("turn id"),
            session.clone(),
            state,
            0,
            None,
            Some(ts(0)),
            None,
        )
        .expect("turn");
        lock(&self.world.state)
            .turns
            .entry(session.as_str().to_owned())
            .or_default()
            .push(turn);
    }

    fn finish_turns(&self, session: &SessionId) {
        lock(&self.world.state).turns.remove(session.as_str());
    }

    fn seed_permission(&self, session: &SessionId, n: u64) {
        let pending = PendingInteraction::try_new(
            InteractionId::new(&uuid_text(n)).expect("interaction id"),
            InteractionKind::Permission,
            session.clone(),
            ts(0),
            Vec::new(),
        )
        .expect("pending interaction");
        self.world.seed_interaction(pending);
    }
}

fn config() -> LifecycleConfig {
    LifecycleConfig::new(30 * MINUTE_MS, 4).expect("config")
}

fn expect_establish(plan: ActivationPlan) -> ResourceGeneration {
    match plan {
        ActivationPlan::Establish { generation } => generation,
        other => panic!("expected a new establishment, got {other:?}"),
    }
}

fn ready(
    lifecycle: &LocalSessionLifecycle,
    fixture: &Fixture,
    session: &SessionId,
) -> ResourceGeneration {
    let plan =
        block_on(lifecycle.activate_session(&Actor::LocalCli, fixture.activation(session, 1)))
            .expect("activation");
    let generation = expect_establish(plan);
    block_on(lifecycle.settle_session(
        &Actor::LocalCli,
        session.clone(),
        generation,
        ActivationOutcome::Ready,
    ))
    .expect("settle ready");
    generation
}

struct FakeRelease {
    outcome: Mutex<ReleaseOutcome>,
    calls: AtomicUsize,
}

impl FakeRelease {
    fn new(outcome: ReleaseOutcome) -> Arc<Self> {
        Arc::new(Self {
            outcome: Mutex::new(outcome),
            calls: AtomicUsize::new(0),
        })
    }

    fn set(&self, outcome: ReleaseOutcome) {
        *lock(&self.outcome) = outcome;
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl SessionRelease for FakeRelease {
    async fn release_session(
        &self,
        _session: SessionId,
        _generation: ResourceGeneration,
        _now: Timestamp,
    ) -> Result<ReleaseOutcome, PortError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(*lock(&self.outcome))
    }
}

// ---------------------------------------------------------------------------------------------
// 激活合并 / ready 复用 / 资源代
// ---------------------------------------------------------------------------------------------

#[test]
fn two_devices_viewing_one_session_share_a_single_establishment() {
    let fixture = Fixture::new();
    let session = fixture.seed_session(1);
    let lifecycle = fixture.converged(config());
    let first = ConnectionId::new(&uuid_text(10)).expect("connection");
    let second = ConnectionId::new(&uuid_text(11)).expect("connection");
    let now = at(0);

    block_on(lifecycle.declare_view(
        &Actor::LocalCli,
        first.clone(),
        Some(session.clone()),
        1,
        now.clone(),
    ))
    .expect("declare first viewer");
    block_on(lifecycle.declare_view(
        &Actor::LocalCli,
        second.clone(),
        Some(session.clone()),
        1,
        now.clone(),
    ))
    .expect("declare second viewer");

    let plan =
        block_on(lifecycle.activate_session(&Actor::LocalCli, fixture.activation(&session, 1)))
            .expect("first activation");
    assert!(matches!(plan, ActivationPlan::Establish { generation } if generation.get() == 1));

    // 第二个并发调用者不启动第二次建立，也不自造第二个资源代。
    let concurrent =
        block_on(lifecycle.activate_session(&Actor::LocalCli, fixture.activation(&session, 2)))
            .expect("merged activation");
    assert_eq!(concurrent, ActivationPlan::InFlight);

    // 目录里的查看者按连接去重。
    let viewers = block_on(lifecycle.viewers(session.clone(), now)).expect("viewers");
    assert_eq!(viewers, vec![first, second]);
}

#[test]
fn ready_reuse_does_not_touch_the_backend_or_write_an_activity_event() {
    let fixture = Fixture::new();
    let session = fixture.seed_session(1);
    let lifecycle = fixture.converged(config());
    let generation = ready(&lifecycle, &fixture, &session);
    let commits = fixture.world.commit_count();
    let published = fixture.world.publish_count();
    assert_eq!(published, 1, "idle -> active writes exactly one event");

    let plan =
        block_on(lifecycle.activate_session(&Actor::LocalCli, fixture.activation(&session, 2)))
            .expect("reuse");
    assert_eq!(plan, ActivationPlan::Reused { generation });
    assert!(plan.reuses_ready_resource());
    assert_eq!(
        fixture.world.commit_count(),
        commits,
        "ready reuse writes nothing"
    );
    assert_eq!(
        fixture.world.publish_count(),
        published,
        "ready reuse publishes nothing"
    );
}

#[test]
fn stale_generation_results_never_bind_to_the_new_resource() {
    let fixture = Fixture::new();
    let session = fixture.seed_session(1);
    let lifecycle = fixture.converged(LifecycleConfig::new(0, 1).expect("config"));
    let release = FakeRelease::new(ReleaseOutcome::Released);
    assert!(lifecycle.set_session_release(release.clone()));
    let first = ready(&lifecycle, &fixture, &session);

    let outcome =
        block_on(lifecycle.release_session(&Actor::LocalCli, session.clone(), first, at(1_000)))
            .expect("release");
    assert_eq!(outcome, ReleaseOutcome::Released);
    assert_eq!(
        block_on(lifecycle.current_activity(&Actor::LocalCli, session.clone())).expect("activity"),
        LocalActivity::Idle
    );

    let plan =
        block_on(lifecycle.activate_session(&Actor::LocalCli, fixture.activation(&session, 2)))
            .expect("second activation");
    let second = expect_establish(plan);
    assert_eq!(second.get(), first.get() + 1);

    // 旧代的迟到 settle 不得把新代标成 ready，也不得产生事件。
    block_on(lifecycle.settle_session(
        &Actor::LocalCli,
        session.clone(),
        first,
        ActivationOutcome::Ready,
    ))
    .expect("stale settle");
    assert_eq!(
        block_on(lifecycle.current_activity(&Actor::LocalCli, session.clone())).expect("activity"),
        LocalActivity::Idle,
        "the stale generation did not become ready"
    );

    // 旧代的容量释放也不得交还新代名额。
    block_on(lifecycle.release_capacity(&Actor::LocalCli, session.clone(), first, at(2_000)))
        .expect("stale release");
    let other = fixture.seed_session(2);
    assert_eq!(
        block_on(lifecycle.reserve_capacity(&Actor::LocalCli, other, at(2_000))).expect("reserve"),
        Admission::CapacityExceeded,
        "the second generation still holds the only slot"
    );
}

// ---------------------------------------------------------------------------------------------
// 释放确认与恢复/回收互斥
// ---------------------------------------------------------------------------------------------

#[test]
fn reclaiming_and_failed_release_keep_active_and_the_slot() {
    let fixture = Fixture::new();
    let session = fixture.seed_session(1);
    let lifecycle = fixture.converged(LifecycleConfig::new(0, 1).expect("config"));
    let release = FakeRelease::new(ReleaseOutcome::Failed);
    assert!(lifecycle.set_session_release(release.clone()));
    let generation = ready(&lifecycle, &fixture, &session);
    assert_eq!(release.calls(), 0);

    let outcome = block_on(lifecycle.release_session(
        &Actor::LocalCli,
        session.clone(),
        generation,
        at(1_000),
    ))
    .expect("release attempt");
    assert_eq!(outcome, ReleaseOutcome::Failed);
    assert_eq!(
        block_on(lifecycle.current_activity(&Actor::LocalCli, session.clone())).expect("activity"),
        LocalActivity::Active,
        "a failed close keeps the session active"
    );
    let other = fixture.seed_session(2);
    assert_eq!(
        block_on(lifecycle.reserve_capacity(&Actor::LocalCli, other.clone(), at(1_000)))
            .expect("reserve"),
        Admission::CapacityExceeded,
        "a failed close keeps the capacity slot"
    );
    assert!(
        block_on(lifecycle.reclaim_candidate(&Actor::LocalCli, at(1_000)))
            .expect("candidate")
            .is_none()
    );

    // 关闭中禁止恢复与回收交错：不会启动第二个绑定。
    assert_eq!(
        block_on(lifecycle.activate_session(&Actor::LocalCli, fixture.activation(&session, 2),))
            .expect("activation while reclaiming"),
        ActivationPlan::InFlight
    );

    // 适配器确认释放后才转 idle 并交还名额。
    release.set(ReleaseOutcome::Released);
    let outcome = block_on(lifecycle.release_session(
        &Actor::LocalCli,
        session.clone(),
        generation,
        at(2_000),
    ))
    .expect("second release attempt");
    assert_eq!(outcome, ReleaseOutcome::Released);
    assert_eq!(
        block_on(lifecycle.current_activity(&Actor::LocalCli, session.clone())).expect("activity"),
        LocalActivity::Idle
    );
    assert_eq!(
        block_on(lifecycle.reserve_capacity(&Actor::LocalCli, other, at(2_000))).expect("reserve"),
        Admission::Reserved
    );
}

// ---------------------------------------------------------------------------------------------
// 查看租约
// ---------------------------------------------------------------------------------------------

#[test]
fn view_lease_versions_leave_expiry_and_disconnect() {
    let fixture = Fixture::new();
    let session_a = fixture.seed_session(1);
    let session_b = fixture.seed_session(2);
    let lifecycle = fixture.converged(config());
    let actor = Actor::LocalCli;
    let connection = ConnectionId::new(&uuid_text(10)).expect("connection");
    let other = ConnectionId::new(&uuid_text(11)).expect("connection");

    let decision = block_on(lifecycle.declare_view(
        &actor,
        connection.clone(),
        Some(session_a.clone()),
        1,
        at(0),
    ))
    .expect("declare");
    assert!(decision.is_accepted());
    assert_eq!(decision.lease_expires_at, Some(at(VIEW_LEASE_MS)));
    assert_eq!(
        block_on(lifecycle.viewers(session_a.clone(), at(0))).expect("viewers"),
        vec![connection.clone()]
    );

    // 同版本同载荷的重试幂等；普通心跳不经过本端口，因此这里只断言续租路径。
    let renewed =
        block_on(lifecycle.renew_view(&actor, connection.clone(), 1, at(10_000))).expect("renew");
    assert_eq!(renewed.lease_expires_at, Some(at(10_000 + VIEW_LEASE_MS)));

    // 过期后不续租、不复活。
    let expired = block_on(lifecycle.renew_view(&actor, connection.clone(), 1, at(41_000)))
        .expect("renew after expiry");
    assert!(!expired.is_accepted());
    assert_eq!(expired.lease_expires_at, None);
    assert!(
        block_on(lifecycle.viewers(session_a.clone(), at(41_000)))
            .expect("viewers")
            .is_empty()
    );
    let resurrect = block_on(lifecycle.declare_view(
        &actor,
        connection.clone(),
        Some(session_a.clone()),
        1,
        at(42_000),
    ))
    .expect("same version after expiry");
    assert!(!resurrect.is_accepted());

    // 更高版本切换选择；旧会话不再有查看者。
    let switched = block_on(lifecycle.declare_view(
        &actor,
        connection.clone(),
        Some(session_b.clone()),
        2,
        at(43_000),
    ))
    .expect("switch");
    assert!(switched.is_accepted());
    assert!(
        block_on(lifecycle.viewers(session_a.clone(), at(43_000)))
            .expect("viewers")
            .is_empty()
    );
    assert_eq!(
        block_on(lifecycle.viewers(session_b.clone(), at(43_000))).expect("viewers"),
        vec![connection.clone()]
    );

    // 同版本不同选择 → stale，且不替换现有查看者。
    assert!(
        !block_on(lifecycle.declare_view(
            &actor,
            connection.clone(),
            Some(session_a.clone()),
            2,
            at(44_000),
        ))
        .expect("same version")
        .is_accepted()
    );
    assert_eq!(
        block_on(lifecycle.viewers(session_b.clone(), at(44_000))).expect("viewers"),
        vec![connection.clone()]
    );

    // 第二条连接：首声明任意版本；低于水位的离页不得清除查看者。
    block_on(lifecycle.declare_view(
        &actor,
        other.clone(),
        Some(session_a.clone()),
        5,
        at(45_000),
    ))
    .expect("other declare");
    assert!(
        !block_on(lifecycle.declare_view(&actor, other.clone(), None, 4, at(46_000)))
            .expect("stale leave")
            .is_accepted()
    );
    assert_eq!(
        block_on(lifecycle.viewers(session_a.clone(), at(46_000))).expect("viewers"),
        vec![other.clone()]
    );
    assert!(
        block_on(lifecycle.declare_view(&actor, other.clone(), None, 6, at(47_000)))
            .expect("leave")
            .is_accepted()
    );
    assert!(
        block_on(lifecycle.viewers(session_a.clone(), at(47_000)))
            .expect("viewers")
            .is_empty()
    );

    // 断开/撤销：立即清除查看者。
    block_on(lifecycle.release_view(&actor, other, at(48_000))).expect("disconnect");
}

#[test]
fn read_only_viewing_keeps_the_session_alive_across_the_retention_window() {
    let fixture = Fixture::new();
    let session = fixture.seed_session(1);
    let lifecycle = fixture.converged(config());
    let actor = Actor::LocalCli;
    let connection = ConnectionId::new(&uuid_text(10)).expect("connection");
    ready(&lifecycle, &fixture, &session);
    block_on(lifecycle.declare_view(&actor, connection.clone(), Some(session.clone()), 1, at(0)))
        .expect("declare");

    // 可见页面每 10 秒续租：整个 30 分钟保留窗口内都不应成为候选。
    for step in 1..=179_u64 {
        fixture.clock.advance(10_000);
        assert!(
            block_on(lifecycle.renew_view(&actor, connection.clone(), 1, at(step * 10_000)))
                .expect("renew")
                .is_accepted()
        );
    }
    assert!(
        block_on(lifecycle.retention_due(&actor, at(1_790_000)))
            .expect("due")
            .is_empty()
    );

    // 离开后必须重新计满完整保留期。
    let leave_at = 1_800_000;
    block_on(lifecycle.release_view(&actor, connection, at(leave_at))).expect("leave");
    assert!(
        block_on(lifecycle.retention_due(&actor, at(leave_at + 29 * MINUTE_MS)))
            .expect("due")
            .is_empty()
    );
    let due =
        block_on(lifecycle.retention_due(&actor, at(leave_at + 30 * MINUTE_MS))).expect("due");
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].session, session);
}

// ---------------------------------------------------------------------------------------------
// 保留期与保护事实
// ---------------------------------------------------------------------------------------------

#[test]
fn tasks_queue_and_pending_interactions_block_reclaim_until_they_end() {
    let fixture = Fixture::new();
    let session = fixture.seed_session(1);
    let lifecycle = fixture.converged(LifecycleConfig::new(60_000, 2).expect("config"));
    fixture.seed_turn(&session, 900, TurnState::Running);
    fixture.seed_turn(&session, 902, TurnState::Queued);
    ready(&lifecycle, &fixture, &session);
    assert_eq!(
        block_on(lifecycle.current_activity(&Actor::LocalCli, session.clone())).expect("activity"),
        LocalActivity::Active
    );

    // 任务仍在执行（含排队与停止未确认：它们都仍是未结束 turn）。
    for step in 0..5_u64 {
        assert!(
            block_on(lifecycle.retention_due(&Actor::LocalCli, at(step * 10 * MINUTE_MS)))
                .expect("due")
                .is_empty()
        );
    }
    assert!(
        block_on(lifecycle.reclaim_candidate(&Actor::LocalCli, at(2_000_000)))
            .expect("candidate")
            .is_none()
    );
    fixture.finish_turns(&session);

    // 待处理交互同样阻止回收，且归约为 Pending。
    fixture.seed_permission(&session, 901);
    assert_eq!(
        block_on(lifecycle.current_activity(&Actor::LocalCli, session.clone())).expect("activity"),
        LocalActivity::Pending
    );
    assert!(
        block_on(lifecycle.retention_due(&Actor::LocalCli, at(3_000_000)))
            .expect("due")
            .is_empty()
    );
}

#[test]
fn protection_end_restarts_the_full_retention_window() {
    let fixture = Fixture::new();
    let session = fixture.seed_session(1);
    let lifecycle = fixture.converged(LifecycleConfig::new(30 * MINUTE_MS, 2).expect("config"));
    ready(&lifecycle, &fixture, &session);
    fixture.clock.advance(29 * MINUTE_MS);

    let connection = ConnectionId::new(&uuid_text(10)).expect("connection");
    block_on(lifecycle.declare_view(
        &Actor::LocalCli,
        connection.clone(),
        Some(session.clone()),
        1,
        at(29 * MINUTE_MS),
    ))
    .expect("declare");
    // 租约仍有效：不能被回收。
    assert!(
        block_on(lifecycle.retention_due(&Actor::LocalCli, at(29 * MINUTE_MS + 20_000)))
            .expect("due")
            .is_empty()
    );
    block_on(lifecycle.release_view(&Actor::LocalCli, connection, at(30 * MINUTE_MS)))
        .expect("leave");
    // 保护解除后重新计满 30 分钟。
    assert!(
        block_on(lifecycle.retention_due(&Actor::LocalCli, at(30 * MINUTE_MS + 29 * MINUTE_MS)))
            .expect("due")
            .is_empty()
    );
    assert_eq!(
        block_on(lifecycle.retention_due(&Actor::LocalCli, at(30 * MINUTE_MS + 30 * MINUTE_MS)))
            .expect("due")
            .len(),
        1
    );
}

#[test]
fn explicit_zero_disables_time_based_reclaim_but_not_capacity() {
    let fixture = Fixture::new();
    let first = fixture.seed_session(1);
    let second = fixture.seed_session(2);
    let lifecycle = fixture.converged(LifecycleConfig::new(0, 1).expect("config"));
    ready(&lifecycle, &fixture, &first);

    assert!(
        block_on(lifecycle.retention_due(&Actor::LocalCli, at(u32::MAX as u64 * 1_000)))
            .expect("due")
            .is_empty()
    );

    // 容量策略仍然生效：满额且第二会话要占位时被拒绝。
    assert_eq!(
        block_on(lifecycle.reserve_capacity(&Actor::LocalCli, second.clone(), at(10_000)))
            .expect("reserve"),
        Admission::CapacityExceeded
    );
    // 但存在未保护候选时可以提前回收它（容量驱逐不受保留期限制）。
    let candidate = block_on(lifecycle.reclaim_candidate(&Actor::LocalCli, at(10_000)))
        .expect("candidate")
        .expect("first is an unprotected candidate");
    assert_eq!(candidate.session, first);
}

// ---------------------------------------------------------------------------------------------
// 容量、LRU 与动态调整
// ---------------------------------------------------------------------------------------------

#[test]
fn concurrent_admissions_never_exceed_the_limit() {
    let fixture = Fixture::new();
    let sessions = (1..=2).map(|n| fixture.seed_session(n)).collect::<Vec<_>>();
    let lifecycle = fixture.converged(LifecycleConfig::new(0, 2).expect("config"));

    assert_eq!(
        block_on(lifecycle.reserve_capacity(&Actor::LocalCli, sessions[0].clone(), at(0)))
            .expect("reserve"),
        Admission::Reserved
    );
    assert_eq!(
        block_on(lifecycle.reserve_capacity(&Actor::LocalCli, sessions[1].clone(), at(0)))
            .expect("reserve"),
        Admission::Reserved
    );
    // 重复预占同一会话不重复计数。
    assert_eq!(
        block_on(lifecycle.reserve_capacity(&Actor::LocalCli, sessions[0].clone(), at(0)))
            .expect("reserve"),
        Admission::Reserved
    );
    let third = fixture.seed_session(3);
    assert_eq!(
        block_on(lifecycle.reserve_capacity(&Actor::LocalCli, third, at(0))).expect("reserve"),
        Admission::CapacityExceeded
    );
}

#[test]
fn reclaim_candidate_is_lru_and_breaks_ties_with_the_stable_session_id() {
    let fixture = Fixture::new();
    let low = fixture.seed_session(1);
    let high = fixture.seed_session(2);
    let lifecycle = fixture.converged(LifecycleConfig::new(0, 2).expect("config"));
    ready(&lifecycle, &fixture, &low);
    ready_hidden(&lifecycle, &fixture, &high, 2);

    // 两者的 last_protected_at 相同（都从未受查看/任务保护）→ 稳定会话标识打破并列。
    let candidate = block_on(lifecycle.reclaim_candidate(&Actor::LocalCli, at(100_000)))
        .expect("candidate")
        .expect("unprotected candidate");
    assert_eq!(candidate.session, low);

    // 让 low 更早结束保护：给 high 一个查看者再离开，high 的 last_protected_at 更晚。
    let connection = ConnectionId::new(&uuid_text(10)).expect("connection");
    block_on(lifecycle.declare_view(
        &Actor::LocalCli,
        connection.clone(),
        Some(high.clone()),
        1,
        at(200_000),
    ))
    .expect("declare");
    block_on(lifecycle.release_view(&Actor::LocalCli, connection, at(200_000))).expect("leave");
    let candidate = block_on(lifecycle.reclaim_candidate(&Actor::LocalCli, at(300_000)))
        .expect("candidate")
        .expect("unprotected candidate");
    assert_eq!(candidate.session, low);
}

fn ready_hidden(
    lifecycle: &LocalSessionLifecycle,
    fixture: &Fixture,
    session: &SessionId,
    request: u64,
) -> ResourceGeneration {
    let plan = block_on(
        lifecycle.activate_session(&Actor::LocalCli, fixture.activation(session, request)),
    )
    .expect("activation");
    let generation = expect_establish(plan);
    block_on(lifecycle.settle_session(
        &Actor::LocalCli,
        session.clone(),
        generation,
        ActivationOutcome::Ready,
    ))
    .expect("settle ready");
    generation
}

#[test]
fn no_safe_candidate_returns_an_explicit_capacity_rejection() {
    let fixture = Fixture::new();
    let active = fixture.seed_session(1);
    let waiting = fixture.seed_session(2);
    let lifecycle = fixture.converged(LifecycleConfig::new(0, 1).expect("config"));
    ready(&lifecycle, &fixture, &active);
    fixture.seed_permission(&active, 900);

    assert!(
        block_on(lifecycle.reclaim_candidate(&Actor::LocalCli, at(1_000)))
            .expect("candidate")
            .is_none()
    );
    assert_eq!(
        block_on(lifecycle.activate_session(&Actor::LocalCli, fixture.activation(&waiting, 2),))
            .expect("activation"),
        ActivationPlan::CapacityExceeded
    );
}

/// 周期回收只在满额时消费 LRU 容量候选：有空位时不得提前释放仍可保留的会话。
#[test]
fn capacity_pressure_is_reported_only_at_the_limit() {
    let fixture = Fixture::new();
    let first = fixture.seed_session(1);
    let second = fixture.seed_session(2);
    let lifecycle = fixture.converged(LifecycleConfig::new(30 * MINUTE_MS, 2).expect("config"));
    assert!(!lifecycle.has_capacity_pressure());
    ready(&lifecycle, &fixture, &first);
    assert!(!lifecycle.has_capacity_pressure(), "还有一个空位");
    ready_hidden(&lifecycle, &fixture, &second, 2);
    assert!(lifecycle.has_capacity_pressure(), "名额已占满");
}

/// 容量满时先按 LRU 回收**未受保护**的 ready 会话，`Released` 后重试准入一次并成功；
/// `sessions.idle_timeout_ms = 0` 只关闭按时间回收，不影响容量回收。
#[test]
fn capacity_is_reclaimed_before_rejecting_even_with_time_based_reclaim_disabled() {
    let fixture = Fixture::new();
    let old = fixture.seed_session(1);
    let waiting = fixture.seed_session(2);
    let lifecycle = fixture.converged(LifecycleConfig::new(0, 1).expect("容量 1"));
    let release = FakeRelease::new(ReleaseOutcome::Released);
    assert!(lifecycle.set_session_release(release.clone()));
    ready(&lifecycle, &fixture, &old);

    let plan =
        block_on(lifecycle.activate_session(&Actor::LocalCli, fixture.activation(&waiting, 2)))
            .expect("activation");
    let generation = expect_establish(plan);
    assert_eq!(release.calls(), 1, "拒绝前必须先释放安全候选");
    assert_eq!(
        block_on(lifecycle.current_activity(&Actor::LocalCli, old)).expect("activity"),
        LocalActivity::Idle,
        "确认 Released 后候选交还名额并收敛为 Idle"
    );

    // 回收后的名额属于新会话：它可以收口为 ready，而不会再次超限。
    block_on(lifecycle.settle_session(
        &Actor::LocalCli,
        waiting.clone(),
        generation,
        ActivationOutcome::Ready,
    ))
    .expect("settle ready");
    assert_eq!(
        block_on(lifecycle.current_activity(&Actor::LocalCli, waiting)).expect("activity"),
        LocalActivity::Active
    );
}

/// 所有候选都仍受保护（未结束任务/有效交互/查看租约）时**不得**回收，保留显式容量拒绝。
#[test]
fn a_fully_protected_capacity_still_returns_an_explicit_rejection() {
    let fixture = Fixture::new();
    let active = fixture.seed_session(1);
    let waiting = fixture.seed_session(2);
    let lifecycle = fixture.converged(LifecycleConfig::new(0, 1).expect("容量 1"));
    let release = FakeRelease::new(ReleaseOutcome::Released);
    assert!(lifecycle.set_session_release(release.clone()));
    ready(&lifecycle, &fixture, &active);
    fixture.seed_permission(&active, 900);

    assert_eq!(
        block_on(lifecycle.activate_session(&Actor::LocalCli, fixture.activation(&waiting, 2)))
            .expect("activation"),
        ActivationPlan::CapacityExceeded
    );
    assert_eq!(release.calls(), 0, "受保护会话不得被回收");
    assert_eq!(
        block_on(lifecycle.current_activity(&Actor::LocalCli, active)).expect("activity"),
        LocalActivity::Pending,
        "保护中的会话保持可用（有效交互优先归约为 Pending）"
    );
}

/// `session.create` 与恢复共用同一容量账：满额时同样先回收再准入。
#[test]
fn create_capacity_is_reclaimed_before_rejecting() {
    let fixture = Fixture::new();
    let old = fixture.seed_session(1);
    let created = fixture.seed_session(2);
    let lifecycle = fixture.converged(LifecycleConfig::new(0, 1).expect("容量 1"));
    let release = FakeRelease::new(ReleaseOutcome::Released);
    assert!(lifecycle.set_session_release(release.clone()));
    ready(&lifecycle, &fixture, &old);

    let generation = expect_establish(
        block_on(lifecycle.begin_created_session(&Actor::LocalCli, created.clone(), at(10_000)))
            .expect("create admission"),
    );
    assert_eq!(release.calls(), 1);
    assert_eq!(
        block_on(lifecycle.current_activity(&Actor::LocalCli, old)).expect("activity"),
        LocalActivity::Idle
    );
    block_on(lifecycle.settle_session(
        &Actor::LocalCli,
        created,
        generation,
        ActivationOutcome::Ready,
    ))
    .expect("settle ready");
}

#[test]
fn lowering_the_limit_does_not_kill_protected_sessions() {
    let fixture = Fixture::new();
    let first = fixture.seed_session(1);
    let second = fixture.seed_session(2);
    let third = fixture.seed_session(3);
    let lifecycle = fixture.converged(LifecycleConfig::new(0, 2).expect("config"));
    ready(&lifecycle, &fixture, &first);
    ready_hidden(&lifecycle, &fixture, &second, 2);

    lifecycle.set_config(LifecycleConfig::new(0, 1).expect("config"));
    // 既有会话不被强杀；只拒绝新增。
    assert_eq!(
        block_on(lifecycle.current_activity(&Actor::LocalCli, first.clone())).expect("activity"),
        LocalActivity::Active
    );
    assert_eq!(
        block_on(lifecycle.reserve_capacity(&Actor::LocalCli, third, at(10_000))).expect("reserve"),
        Admission::CapacityExceeded
    );
}

// ---------------------------------------------------------------------------------------------
// 事件顺序、启动收敛与快照一致
// ---------------------------------------------------------------------------------------------

#[test]
fn activity_event_is_committed_before_it_is_published() {
    let fixture = Fixture::new();
    let session = fixture.seed_session(1);
    let lifecycle = fixture.converged(config());
    ready(&lifecycle, &fixture, &session);

    assert_eq!(fixture.world.commit_count(), 1);
    assert_eq!(fixture.world.publish_count(), 1);
    assert!(
        fixture.world.published_after_commit(),
        "the publisher must only see committed events"
    );
    let events = fixture.world.events(&session);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type.as_str(), LOCAL_ACTIVITY_EVENT);
    let view = block_on(fixture.store.read_view()).expect("read view");
    let payload = block_on(view.event_payload(&events[0].id))
        .expect("payload")
        .expect("event exists");
    assert!(
        payload
            .view
            .as_str()
            .contains(r#""localActivity":"active""#)
    );
    assert_eq!(
        fixture.world.event_origin(&events[0].id),
        Some(EventOrigin::Daemon)
    );
}

#[test]
fn failed_activity_commit_is_not_broadcast() {
    let fixture = Fixture::new();
    let session = fixture.seed_session(1);
    let lifecycle = fixture.converged(config());
    let plan =
        block_on(lifecycle.activate_session(&Actor::LocalCli, fixture.activation(&session, 1)))
            .expect("activation");
    let generation = expect_establish(plan);
    fixture.world.fail_next_commit();
    let result = block_on(lifecycle.settle_session(
        &Actor::LocalCli,
        session.clone(),
        generation,
        ActivationOutcome::Ready,
    ));
    assert!(result.is_err());
    assert_eq!(
        fixture.world.publish_count(),
        0,
        "a failed commit must not broadcast"
    );
}

#[test]
fn restart_converges_published_activity_without_trusting_old_ready_resources() {
    let fixture = Fixture::new();
    let session = fixture.seed_session(1);
    let first_process = fixture.converged(config());
    ready(&first_process, &fixture, &session);
    assert_eq!(
        block_on(first_process.current_activity(&Actor::LocalCli, session.clone()))
            .expect("activity"),
        LocalActivity::Active
    );
    let before = fixture.world.publish_count();

    // 新进程：目录、租约与资源代都必须清空，旧 ready 不得被当作资源仍存活。
    let second_process = fixture.lifecycle(config());
    assert!(!second_process.is_converged());
    assert!(block_on(second_process.current_activity(&Actor::LocalCli, session.clone())).is_err());
    let emitted = block_on(second_process.converge_on_startup()).expect("converge");
    assert_eq!(
        emitted, 1,
        "the previously active session converges to idle"
    );
    assert_eq!(
        block_on(second_process.current_activity(&Actor::LocalCli, session.clone()))
            .expect("activity"),
        LocalActivity::Idle
    );
    assert!(fixture.world.publish_count() > before);
    // 新的激活从新代开始，不复用旧代的 `ready`。
    let plan = block_on(
        second_process.activate_session(&Actor::LocalCli, fixture.activation(&session, 2)),
    )
    .expect("activation after restart");
    let next = expect_establish(plan);
    assert_eq!(
        next.get(),
        1,
        "the new process has a fresh runtime directory (no old ready generation)"
    );
}

#[test]
fn uncertain_activation_keeps_active_and_is_not_redelivered_automatically() {
    let fixture = Fixture::new();
    let session = fixture.seed_session(1);
    let lifecycle = fixture.converged(LifecycleConfig::new(0, 1).expect("config"));
    let plan =
        block_on(lifecycle.activate_session(&Actor::LocalCli, fixture.activation(&session, 1)))
            .expect("activation");
    let generation = expect_establish(plan);
    let error = PublicError::coded("internal.unavailable", "unknown", true).expect("error");
    block_on(lifecycle.settle_session(
        &Actor::LocalCli,
        session.clone(),
        generation,
        ActivationOutcome::Uncertain { error },
    ))
    .expect("settle uncertain");

    assert_eq!(
        block_on(lifecycle.current_activity(&Actor::LocalCli, session.clone())).expect("activity"),
        LocalActivity::Active
    );
    // 新的请求不会换号重派：仍然 `InFlight`，且容量名额保持占用。
    assert_eq!(
        block_on(lifecycle.activate_session(&Actor::LocalCli, fixture.activation(&session, 2),))
            .expect("activation"),
        ActivationPlan::InFlight
    );
    let other = fixture.seed_session(2);
    assert_eq!(
        block_on(lifecycle.reserve_capacity(&Actor::LocalCli, other, at(1_000))).expect("reserve"),
        Admission::CapacityExceeded
    );
}

#[test]
fn query_snapshot_and_replay_read_the_same_activity_value() {
    let fixture = Fixture::new();
    let session = fixture.seed_session(1);
    let lifecycle = fixture.converged(config());
    ready(&lifecycle, &fixture, &session);

    let direct =
        block_on(lifecycle.current_activity(&Actor::LocalCli, session.clone())).expect("activity");
    assert_eq!(direct, LocalActivity::Active);
    let snapshot = block_on(lifecycle.activity_snapshot(&Actor::LocalCli)).expect("snapshot");
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].summary.session_id(), &session);
    assert_eq!(snapshot[0].activity, direct);

    // 重放里的视图与查询同值。
    let view = block_on(fixture.store.read_view()).expect("read view");
    let slice = block_on(view.sync_replay(None, ReplayLimit::default())).expect("replay");
    let record = slice
        .events
        .iter()
        .find(|record| record.event.event_type.as_str() == LOCAL_ACTIVITY_EVENT)
        .expect("activity event");
    assert_eq!(
        local_activity_of(&record.payload.view),
        Some(direct),
        "replay and query must agree"
    );
}

#[test]
fn activity_event_is_not_a_history_entry_and_its_policy_is_durable() {
    let event_type = EventType::new(LOCAL_ACTIVITY_EVENT).expect("event type");
    assert!(!HISTORY_ENTRY_EVENT_TYPES.contains(&LOCAL_ACTIVITY_EVENT));
    assert_eq!(persistence_policy(&event_type), PersistencePolicy::Durable);
    assert!(!EPHEMERAL_EVENT_TYPES.contains(&LOCAL_ACTIVITY_EVENT));
}

#[test]
fn activity_event_only_on_actual_value_change() {
    let fixture = Fixture::new();
    let session = fixture.seed_session(1);
    let lifecycle = fixture.converged(config());
    let generation = ready(&lifecycle, &fixture, &session);
    let after_ready = fixture.world.commit_count();

    // 重复 settle（同一代）是幂等无操作。
    block_on(lifecycle.settle_session(
        &Actor::LocalCli,
        session.clone(),
        generation,
        ActivationOutcome::Ready,
    ))
    .expect("idempotent settle");
    assert_eq!(fixture.world.commit_count(), after_ready);

    // 查看者增减不改变归约结果，因此也不写事件。
    let connection = ConnectionId::new(&uuid_text(10)).expect("connection");
    block_on(lifecycle.declare_view(
        &Actor::LocalCli,
        connection.clone(),
        Some(session.clone()),
        1,
        at(1_000),
    ))
    .expect("declare");
    block_on(lifecycle.release_view(&Actor::LocalCli, connection, at(2_000))).expect("leave");
    assert_eq!(fixture.world.commit_count(), after_ready);

    // 任务事实驱动刷新：同一归约值不写。
    block_on(lifecycle.refresh_activity(&session)).expect("refresh");
    assert_eq!(fixture.world.commit_count(), after_ready);
}

#[test]
fn session_activity_facts_reduce_pending_over_active() {
    // 归约本身的边界：待处理优先，其次是任务/资源，否则空闲。
    assert_eq!(
        LocalActivity::reduce(LocalActivityFacts {
            pending_interaction: true,
            unfinished_task: true,
            ready_resource: true,
            release_unconfirmed: true,
        }),
        LocalActivity::Pending
    );
    assert_eq!(
        LocalActivity::reduce(LocalActivityFacts {
            pending_interaction: false,
            unfinished_task: false,
            ready_resource: false,
            release_unconfirmed: true,
        }),
        LocalActivity::Active
    );
    assert_eq!(
        LocalActivity::reduce(LocalActivityFacts::default()),
        LocalActivity::Idle
    );
    // 首次 activating / 首次失败之外的空目录不产生 Active。
    let facts = SessionActivityFacts::default();
    assert_eq!(
        LocalActivity::reduce(LocalActivityFacts {
            pending_interaction: false,
            unfinished_task: facts.unfinished_turns > 0,
            ready_resource: false,
            release_unconfirmed: false,
        }),
        LocalActivity::Idle
    );
}

#[test]
fn restart_does_not_redispatch_or_reuse_an_uncertain_activation() {
    let fixture = Fixture::new();
    let session = fixture.seed_session(1);
    let release = FakeRelease::new(ReleaseOutcome::Released);
    let first_process = fixture.converged(config());
    assert!(first_process.set_session_release(release.clone()));
    let plan =
        block_on(first_process.activate_session(&Actor::LocalCli, fixture.activation(&session, 1)))
            .expect("activation");
    let generation = expect_establish(plan);
    let error = PublicError::coded("internal.unavailable", "unknown", true).expect("error");
    block_on(first_process.settle_session(
        &Actor::LocalCli,
        session.clone(),
        generation,
        ActivationOutcome::Uncertain { error },
    ))
    .expect("settle uncertain");
    assert_eq!(
        block_on(first_process.current_activity(&Actor::LocalCli, session.clone()))
            .expect("activity"),
        LocalActivity::Active
    );

    // 新进程的启动收敛只清空运行期事实并提交收敛事件：不触碰 Agent、不调用释放端口、不重派。
    let second_process = fixture.lifecycle(config());
    assert!(second_process.set_session_release(release.clone()));
    block_on(second_process.converge_on_startup()).expect("converge");
    assert_eq!(
        release.calls(),
        0,
        "startup convergence never drives a release"
    );
    assert_eq!(
        block_on(second_process.current_activity(&Actor::LocalCli, session.clone()))
            .expect("activity"),
        LocalActivity::Idle,
        "an old uncertain attempt is not treated as a live resource"
    );
    // 新的显式激活仍然是新建立（从空目录开始），不是对旧 uncertain 的自动重派。
    let plan = block_on(
        second_process.activate_session(&Actor::LocalCli, fixture.activation(&session, 2)),
    )
    .expect("explicit activation");
    let _ = expect_establish(plan);
}

#[test]
fn proven_activation_failure_releases_the_reservation_and_shows_idle() {
    // `ActivationOutcome::Failed` 的语义是「适配器可证明没有建立资源」（spec
    // `local-session-lifecycle`「同时创建和恢复」：失败释放预占；「本机展示」：首次恢复失败且无真实资源
    // 时显示空闲）。说不清的一律用 `Uncertain`，它保留占位与 `Active`。
    let fixture = Fixture::new();
    let session = fixture.seed_session(1);
    let lifecycle = fixture.converged(LifecycleConfig::new(0, 1).expect("config"));
    let plan =
        block_on(lifecycle.activate_session(&Actor::LocalCli, fixture.activation(&session, 1)))
            .expect("activation");
    let generation = expect_establish(plan);
    assert_eq!(
        block_on(lifecycle.current_activity(&Actor::LocalCli, session.clone())).expect("activity"),
        LocalActivity::Idle,
        "activating does not claim availability"
    );

    let error = PublicError::coded("command.unsupported", "not supported", false).expect("error");
    block_on(lifecycle.settle_session(
        &Actor::LocalCli,
        session.clone(),
        generation,
        ActivationOutcome::Failed { error },
    ))
    .expect("settle failed");
    assert_eq!(
        block_on(lifecycle.current_activity(&Actor::LocalCli, session.clone())).expect("activity"),
        LocalActivity::Idle
    );
    let other = fixture.seed_session(2);
    assert_eq!(
        block_on(lifecycle.reserve_capacity(&Actor::LocalCli, other, at(1_000))).expect("reserve"),
        Admission::Reserved,
        "a proven failure releases the reservation"
    );
}

#[test]
fn create_and_resume_share_the_same_capacity_and_generation_flow() {
    let fixture = Fixture::new();
    let created = fixture.seed_session(1);
    let resumed = fixture.seed_session(2);
    let lifecycle = fixture.converged(LifecycleConfig::new(0, 1).expect("config"));

    let plan = block_on(lifecycle.begin_created_session(&Actor::LocalCli, created.clone(), at(0)))
        .expect("create admission");
    let generation = expect_establish(plan);
    block_on(lifecycle.settle_session(
        &Actor::LocalCli,
        created.clone(),
        generation,
        ActivationOutcome::Ready,
    ))
    .expect("settle created resource");
    assert_eq!(
        block_on(lifecycle.current_activity(&Actor::LocalCli, created)).expect("activity"),
        LocalActivity::Active
    );
    // 同一节点名额：创建占用的名额也被恢复路径看到。
    assert_eq!(
        block_on(lifecycle.activate_session(&Actor::LocalCli, fixture.activation(&resumed, 2),))
            .expect("resume admission"),
        ActivationPlan::CapacityExceeded
    );
}

// ---------------------------------------------------------------------------------------------
// 修复轮：stale 保护、不确定占位与并发释放
// ---------------------------------------------------------------------------------------------

/// 释放适配器的并发替身：第一次调用挂起一轮，让第二次调用先完成 `Released`；随后第一次调用
/// 返回 `late`，模拟「已确认释放之后才到达的失败/不确定结果」。
struct RacingRelease {
    calls: AtomicUsize,
    late: ReleaseOutcome,
}

#[async_trait]
impl SessionRelease for RacingRelease {
    async fn release_session(
        &self,
        _session: SessionId,
        _generation: ResourceGeneration,
        _now: Timestamp,
    ) -> Result<ReleaseOutcome, PortError> {
        let index = self.calls.fetch_add(1, Ordering::SeqCst);
        if index == 0 {
            YieldOnce { yielded: false }.await;
            Ok(self.late)
        } else {
            Ok(ReleaseOutcome::Released)
        }
    }
}

/// 只让出一次执行权的 future（`run_all` 的协作式执行器据此交错两个调用）。
struct YieldOnce {
    yielded: bool,
}

impl Future for YieldOnce {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            Poll::Pending
        }
    }
}

#[test]
fn stale_declarations_do_not_refresh_viewer_protection() {
    let fixture = Fixture::new();
    let session_a = fixture.seed_session(1);
    let session_b = fixture.seed_session(2);
    let lifecycle = fixture.converged(LifecycleConfig::new(30 * MINUTE_MS, 4).expect("config"));
    let actor = Actor::LocalCli;
    let connection = ConnectionId::new(&uuid_text(10)).expect("connection");
    ready(&lifecycle, &fixture, &session_a);
    ready_hidden(&lifecycle, &fixture, &session_b, 2);

    // a 从激活完成时刻开始无人保护的保留期；此刻连接查看的是 b。
    block_on(lifecycle.declare_view(
        &actor,
        connection.clone(),
        Some(session_b.clone()),
        1,
        at(10 * MINUTE_MS),
    ))
    .expect("declare b");

    // 同版本把选择改成 a：状态机已分叉 → stale，且不得给 a 建立或刷新保护。
    let stale = block_on(lifecycle.declare_view(
        &actor,
        connection.clone(),
        Some(session_a.clone()),
        1,
        at(20 * MINUTE_MS),
    ))
    .expect("stale declare");
    assert!(!stale.is_accepted());
    assert_eq!(stale.lease_expires_at, None);
    assert!(
        block_on(lifecycle.viewers(session_a.clone(), at(20 * MINUTE_MS)))
            .expect("viewers")
            .is_empty(),
        "a stale declaration never registers the connection as a viewer"
    );

    // a 的保留期仍在激活完成后的 30 分钟处到期；stale 没有延长它。
    let due = block_on(lifecycle.retention_due(&actor, at(30 * MINUTE_MS))).expect("due");
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].session, session_a);
    assert!(
        due.iter().all(|candidate| candidate.session != session_b),
        "b lost its viewer just now, so its full retention window has only started"
    );
}

#[test]
fn same_version_payload_change_is_stale_and_keeps_the_lease() {
    let fixture = Fixture::new();
    let session_a = fixture.seed_session(1);
    let session_b = fixture.seed_session(2);
    let lifecycle = fixture.converged(config());
    let actor = Actor::LocalCli;
    let connection = ConnectionId::new(&uuid_text(10)).expect("connection");

    let declared = block_on(lifecycle.declare_view(
        &actor,
        connection.clone(),
        Some(session_a.clone()),
        1,
        at(0),
    ))
    .expect("declare a");
    assert!(declared.is_accepted());

    // 同版本 A→null：载荷已变化，必须 stale；不替换当前查看者、不清租期。
    let leave = block_on(lifecycle.declare_view(&actor, connection.clone(), None, 1, at(1_000)))
        .expect("same version leave");
    assert!(!leave.is_accepted());
    assert_eq!(leave.lease_expires_at, None);
    assert_eq!(
        block_on(lifecycle.viewers(session_a.clone(), at(1_000))).expect("viewers"),
        vec![connection.clone()]
    );

    // 租期仍在：同版本续租成功（若 stale 清掉了 expires_at，这里会失败）。
    let renewed = block_on(lifecycle.renew_view(&actor, connection.clone(), 1, at(2_000)))
        .expect("renew after stale leave");
    assert!(renewed.is_accepted());
    assert_eq!(renewed.lease_expires_at, Some(at(2_000 + VIEW_LEASE_MS)));

    // 同版本 Some(A)→Some(B) 同样是载荷变化 → stale。
    assert!(
        !block_on(lifecycle.declare_view(
            &actor,
            connection.clone(),
            Some(session_b.clone()),
            1,
            at(3_000),
        ))
        .expect("same version switch")
        .is_accepted()
    );
    assert_eq!(
        block_on(lifecycle.viewers(session_a.clone(), at(3_000))).expect("viewers"),
        vec![connection.clone()]
    );
    assert!(
        block_on(lifecycle.viewers(session_b.clone(), at(3_000)))
            .expect("viewers")
            .is_empty()
    );

    // 真正的离页必须先递增版本；null→null 同版本才是幂等接受。
    assert!(
        block_on(lifecycle.declare_view(&actor, connection.clone(), None, 2, at(4_000)))
            .expect("versioned leave")
            .is_accepted()
    );
    assert!(
        block_on(lifecycle.viewers(session_a.clone(), at(4_000)))
            .expect("viewers")
            .is_empty()
    );
    let repeated = block_on(lifecycle.declare_view(&actor, connection, None, 2, at(5_000)))
        .expect("idempotent leave");
    assert!(repeated.is_accepted());
    assert!(
        block_on(lifecycle.viewers(session_a.clone(), at(5_000)))
            .expect("viewers")
            .is_empty()
    );
}

#[test]
fn capacity_release_never_frees_an_uncertain_reservation() {
    let fixture = Fixture::new();
    let session = fixture.seed_session(1);
    let lifecycle = fixture.converged(LifecycleConfig::new(0, 1).expect("config"));
    let plan =
        block_on(lifecycle.activate_session(&Actor::LocalCli, fixture.activation(&session, 1)))
            .expect("activation");
    let generation = expect_establish(plan);
    let error = PublicError::coded("internal.unavailable", "unknown", true).expect("error");
    block_on(lifecycle.settle_session(
        &Actor::LocalCli,
        session.clone(),
        generation,
        ActivationOutcome::Uncertain { error },
    ))
    .expect("settle uncertain");

    // 结果不确定 = 外部资源可能存在：容量回滚不得放掉名额，必须经会话释放确认。
    let rejected = block_on(lifecycle.release_capacity(
        &Actor::LocalCli,
        session.clone(),
        generation,
        at(1_000),
    ));
    assert!(
        matches!(rejected, Err(PortError::InvalidRequest(_))),
        "an uncertain activation must not return its slot: {rejected:?}"
    );
    assert_eq!(
        block_on(lifecycle.current_activity(&Actor::LocalCli, session.clone())).expect("activity"),
        LocalActivity::Active,
        "the unconfirmed resource is still reported active"
    );
    let other = fixture.seed_session(2);
    assert_eq!(
        block_on(lifecycle.reserve_capacity(&Actor::LocalCli, other.clone(), at(1_000)))
            .expect("reserve"),
        Admission::CapacityExceeded,
        "the uncertain reservation still holds the only slot"
    );

    // 只有适配器确认释放后才交还名额。
    let release = FakeRelease::new(ReleaseOutcome::Released);
    assert!(lifecycle.set_session_release(release));
    let outcome = block_on(lifecycle.release_session(
        &Actor::LocalCli,
        session.clone(),
        generation,
        at(2_000),
    ))
    .expect("confirmed release");
    assert_eq!(outcome, ReleaseOutcome::Released);
    assert_eq!(
        block_on(lifecycle.current_activity(&Actor::LocalCli, session)).expect("activity"),
        LocalActivity::Idle
    );
    assert_eq!(
        block_on(lifecycle.reserve_capacity(&Actor::LocalCli, other, at(2_000))).expect("reserve"),
        Admission::Reserved
    );
}

#[test]
fn capacity_release_still_rolls_back_a_plain_pending_activation() {
    let fixture = Fixture::new();
    let session = fixture.seed_session(1);
    let lifecycle = fixture.converged(LifecycleConfig::new(0, 1).expect("config"));
    let plan =
        block_on(lifecycle.activate_session(&Actor::LocalCli, fixture.activation(&session, 1)))
            .expect("activation");
    let generation = expect_establish(plan);

    // 尚未 settle、也没有不确定结果：这才是 `CapacityAdmission::release` 的唯一合法用途。
    block_on(lifecycle.release_capacity(&Actor::LocalCli, session.clone(), generation, at(1_000)))
        .expect("roll back a pending activation");
    let other = fixture.seed_session(2);
    assert_eq!(
        block_on(lifecycle.reserve_capacity(&Actor::LocalCli, other, at(1_000))).expect("reserve"),
        Admission::Reserved
    );
    // 回滚之后该代的迟到 settle 仍必须被忽略。
    block_on(lifecycle.settle_session(
        &Actor::LocalCli,
        session.clone(),
        generation,
        ActivationOutcome::Ready,
    ))
    .expect("stale settle");
    assert_eq!(
        block_on(lifecycle.current_activity(&Actor::LocalCli, session)).expect("activity"),
        LocalActivity::Idle
    );
}

#[test]
fn late_release_failure_never_reopens_an_already_released_session() {
    for late in [ReleaseOutcome::Failed, ReleaseOutcome::Uncertain] {
        let fixture = Fixture::new();
        let session = fixture.seed_session(1);
        let lifecycle = fixture.converged(LifecycleConfig::new(0, 1).expect("config"));
        let release = Arc::new(RacingRelease {
            calls: AtomicUsize::new(0),
            late,
        });
        assert!(lifecycle.set_session_release(release));
        let generation = ready(&lifecycle, &fixture, &session);
        let commits_before = fixture.world.commit_count();

        // 两个并发释放：第二个先确认 `Released`，第一个的失败结果迟到返回。
        let results = run_all(vec![
            lifecycle.release_session(&Actor::LocalCli, session.clone(), generation, at(1_000)),
            lifecycle.release_session(&Actor::LocalCli, session.clone(), generation, at(1_001)),
        ]);
        assert_eq!(results[0].as_ref().ok(), Some(&late));
        assert_eq!(results[1].as_ref().ok(), Some(&ReleaseOutcome::Released));

        // 迟到的失败不得把已 Released 的会话改回 `reclaiming`，也不得重新占位。
        assert_eq!(
            block_on(lifecycle.current_activity(&Actor::LocalCli, session.clone()))
                .expect("activity"),
            LocalActivity::Idle,
            "an already released session stays idle after a late {late:?}"
        );
        let other = fixture.seed_session(2);
        assert_eq!(
            block_on(lifecycle.reserve_capacity(&Actor::LocalCli, other, at(2_000)))
                .expect("reserve"),
            Admission::Reserved,
            "the slot released by the confirmed call stays free"
        );
        assert_eq!(
            fixture.world.commit_count(),
            commits_before + 1,
            "the ignored late result must not commit a second activity event"
        );
    }
}
