//! 准备运行期目录（`docs/CORE_PORTS_AND_STORAGE.md` §3.10、`design.md` D1/D3/D9）。
//!
//! 这是**纯运行期**实现：目录、资源代、租约、释放事实都只存在于内存；不写任何 owned 行、命令收据、
//! 事件或目录记录。容量经既有的 [`CapacityAdmission`] 端口计入同一 `max_active_sessions` 预算，
//! 首次受理成功后名额**转移**给正式会话（不调用 `release`），可证明未派发的离开/失败才交还名额。
//!
//! 并发规则：
//!
//! - **不交错**：首次提交冻结（[`PreparationDirectory::freeze_for_submission`]）期间，释放返回失败并
//!   继续占位；重复冻结返回 `Busy`。
//! - **释放失败/未知继续占位**：只有容量端口 `Ok` 才把名额交还；失败时保留名额并记录
//!   `release_uncertain`，不能靠删除内存项假装腾出名额。
//! - **有界目录**：释放成功与首次提交成功都把条目从目录移除，只留下有界 tombstone，因此长期运行的
//!   daemon 不会为每个已结束的准备保留一条内存项；操作结果仍由 tombstone 给出 `stale(released)`。
//! - **旧代隔离**：`RuntimeEpoch` + `ResourceGeneration` 决定操作是否仍指向当前资源；释放后的有界
//!   tombstone 让旧代操作得到 `stale(released)` 而不是创建替代资源。

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;

use crate::model::{
    AgentRef, AllocatePreparation, AllocatePreparationOutcome, DirectoryId, PortError,
    PreparationDecision, PreparationFailure, PreparationFailureReason, PreparationFreeze,
    PreparationGrant, PreparationOwner, PreparationReleaseOutcome, PreparationStaleReason,
    PreparationState, PreparationView, ResourceGeneration, RuntimeEpoch, SessionId, Timestamp,
};
use crate::ports::{Admission, CapacityAdmission, IdGenerator};

use crate::model::{PREPARATION_LEASE_MS, PREPARATION_RELEASED_TOMBSTONES};

#[derive(Debug, Clone)]
struct Entry {
    generation: ResourceGeneration,
    /// 容量端口的预占标识。`reserve` 不分配资源代时按 `ResourceGeneration::new(0)` 归还（§5.10 的
    /// `release` 只放“尚未建立资源”的预占）。
    capacity_generation: ResourceGeneration,
    state: PreparationState,
    failure: Option<PreparationFailure>,
    agent: AgentRef,
    directory: Option<DirectoryId>,
    owner: PreparationOwner,
    lease_expires_at: Option<Timestamp>,
    /// 首次提交已冻结：期间释放必须失败并继续占位。
    frozen: bool,
    /// 正在调用容量端口的释放（防止与冻结/另一次释放交错）。
    releasing: bool,
    updated_at: Timestamp,
}

#[derive(Debug, Default)]
struct DirectoryState {
    entries: HashMap<SessionId, Entry>,
    /// 有界的释放事实：`(session, generation, released_at)`；超出上界时遗忘最旧的。
    released: VecDeque<(SessionId, ResourceGeneration, Timestamp)>,
}

impl DirectoryState {
    fn remember_released(
        &mut self,
        session: SessionId,
        generation: ResourceGeneration,
        at: Timestamp,
    ) {
        self.released.push_back((session, generation, at));
        while self.released.len() > PREPARATION_RELEASED_TOMBSTONES {
            self.released.pop_front();
        }
    }

    fn released_generation(&self, session: &SessionId) -> Option<ResourceGeneration> {
        self.released
            .iter()
            .rev()
            .find(|(candidate, _, _)| candidate == session)
            .map(|(_, generation, _)| *generation)
    }
}

/// 本进程的准备运行期目录。
pub struct PreparationDirectory {
    ids: Arc<dyn IdGenerator>,
    capacity: Arc<dyn CapacityAdmission>,
    runtime_epoch: RuntimeEpoch,
    state: Mutex<DirectoryState>,
}

impl std::fmt::Debug for PreparationDirectory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = lock(&self.state);
        f.debug_struct("PreparationDirectory")
            .field("runtime_epoch", &self.runtime_epoch)
            .field("entries", &state.entries.len())
            .finish()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl PreparationDirectory {
    /// 构造。`runtime_epoch` 由组合根在进程启动时分配一次（正常重启改变）；`ids` 是会话 ID 的唯一
    /// 分配权威，`capacity` 与创建/恢复共用同一预算。
    pub fn new(
        ids: Arc<dyn IdGenerator>,
        capacity: Arc<dyn CapacityAdmission>,
        runtime_epoch: RuntimeEpoch,
    ) -> Self {
        Self {
            ids,
            capacity,
            runtime_epoch,
            state: Mutex::new(DirectoryState::default()),
        }
    }

    /// 本进程的准备运行期 epoch。
    pub fn runtime_epoch(&self) -> &RuntimeEpoch {
        &self.runtime_epoch
    }

    /// 分配一个新的准备 ID 并预占容量。
    ///
    /// ID 只由 `IdGenerator` 产生：客户端 UUID 永远不被采用。容量满返回
    /// [`AllocatePreparationOutcome::CapacityExceeded`]，不插入任何条目。
    pub async fn allocate(
        &self,
        request: AllocatePreparation,
        now: Timestamp,
    ) -> Result<AllocatePreparationOutcome, PortError> {
        let session = self.ids.session_id();
        if self.capacity.reserve(session.clone(), now.clone()).await? == Admission::CapacityExceeded
        {
            return Ok(AllocatePreparationOutcome::CapacityExceeded);
        }
        let Some(expires_at) = now.checked_add_millis(PREPARATION_LEASE_MS) else {
            // 时间戳溢出：立即交还预占，不留下无租期的条目。
            let _ = self
                .capacity
                .release(session, ResourceGeneration::new(0), now)
                .await;
            return Err(PortError::InvalidRequest(
                "preparation lease expiry is outside the supported window",
            ));
        };
        let view = {
            let mut state = lock(&self.state);
            state.entries.insert(
                session.clone(),
                Entry {
                    generation: ResourceGeneration::new(1),
                    capacity_generation: ResourceGeneration::new(0),
                    state: PreparationState::Preparing,
                    failure: None,
                    agent: request.agent,
                    directory: request.directory,
                    owner: request.owner,
                    lease_expires_at: Some(expires_at),
                    frozen: false,
                    releasing: false,
                    updated_at: now,
                },
            );
            let entry = state.entries.get(&session).ok_or(PortError::Corrupt(
                "preparation entry vanished after insert",
            ))?;
            self.view_of(&session, entry)
        };
        Ok(AllocatePreparationOutcome::Allocated(view))
    }

    fn view_of(&self, session: &SessionId, entry: &Entry) -> PreparationView {
        PreparationView {
            session: session.clone(),
            runtime_epoch: self.runtime_epoch.clone(),
            generation: entry.generation,
            state: entry.state,
            agent: entry.agent.clone(),
            directory: entry.directory.clone(),
            owner: entry.owner.clone(),
            lease_expires_at: entry.lease_expires_at.clone(),
            failure: entry.failure.clone(),
        }
    }

    /// 读取当前准备（不改变租约/状态）。
    pub fn view(&self, session: &SessionId) -> Option<PreparationView> {
        let state = lock(&self.state);
        state
            .entries
            .get(session)
            .map(|entry| self.view_of(session, entry))
    }

    /// 恢复：同一 ID 的未提交准备在同一运行期返回当前资源；租约到期则分配**新资源代**并回到
    /// `preparing`（调用方随后重建端点并 `mark_ready`）。已提交/已释放/未知返回 stale 或 unknown。
    pub fn restore(
        &self,
        session: &SessionId,
        owner: &PreparationOwner,
        now: Timestamp,
    ) -> PreparationDecision {
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return match state.released_generation(session) {
                Some(_) => PreparationDecision::Stale(PreparationStaleReason::Released),
                None => PreparationDecision::Unknown,
            };
        };
        if entry.owner != *owner {
            return PreparationDecision::Stale(PreparationStaleReason::Released);
        }
        let expired = entry
            .lease_expires_at
            .as_ref()
            .is_none_or(|expires| now >= *expires);
        if expired {
            let Some(next) = entry.generation.checked_next() else {
                return PreparationDecision::Stale(PreparationStaleReason::GenerationMismatch);
            };
            let Some(expires_at) = now.checked_add_millis(PREPARATION_LEASE_MS) else {
                return PreparationDecision::Stale(PreparationStaleReason::Released);
            };
            entry.generation = next;
            entry.state = PreparationState::Preparing;
            entry.failure = None;
            entry.frozen = false;
            entry.lease_expires_at = Some(expires_at);
            entry.updated_at = now.clone();
        }
        PreparationDecision::Ready(self.view_of(session, entry))
    }

    /// 端点与真实设置已确认：`preparing → ready`。资源代不符返回 `stale`。
    pub fn mark_ready(
        &self,
        session: &SessionId,
        generation: ResourceGeneration,
        now: Timestamp,
    ) -> PreparationDecision {
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return PreparationDecision::Unknown;
        };
        if entry.generation != generation {
            return PreparationDecision::Stale(PreparationStaleReason::GenerationMismatch);
        }
        let Some(expires_at) = now.checked_add_millis(PREPARATION_LEASE_MS) else {
            return PreparationDecision::Stale(PreparationStaleReason::Released);
        };
        entry.state = PreparationState::Ready;
        entry.failure = None;
        entry.lease_expires_at = Some(expires_at);
        entry.updated_at = now;
        PreparationDecision::Ready(self.view_of(session, entry))
    }

    /// 记录一次可证明的失败（端点建立/配置复核）：保留资源与正文，不伪造 ready。
    pub fn mark_failed(
        &self,
        session: &SessionId,
        generation: ResourceGeneration,
        reason: PreparationFailureReason,
        now: Timestamp,
    ) -> PreparationDecision {
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return PreparationDecision::Unknown;
        };
        if entry.generation != generation {
            return PreparationDecision::Stale(PreparationStaleReason::GenerationMismatch);
        }
        entry.failure = Some(PreparationFailure { reason });
        entry.updated_at = now;
        PreparationDecision::Ready(self.view_of(session, entry))
    }

    /// 续租：只接受当前资源代且未过期的条目；过期返回 `stale(released)`（由 `sweep_expired` 或
    /// `restore` 重新分配资源代）。
    pub fn renew(
        &self,
        session: &SessionId,
        generation: ResourceGeneration,
        now: Timestamp,
    ) -> PreparationDecision {
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return match state.released_generation(session) {
                Some(_) => PreparationDecision::Stale(PreparationStaleReason::Released),
                None => PreparationDecision::Unknown,
            };
        };
        if entry.generation != generation {
            return PreparationDecision::Stale(PreparationStaleReason::GenerationMismatch);
        }
        let expired = entry
            .lease_expires_at
            .as_ref()
            .is_none_or(|expires| now >= *expires);
        if expired {
            return PreparationDecision::Stale(PreparationStaleReason::Released);
        }
        let Some(expires_at) = now.checked_add_millis(PREPARATION_LEASE_MS) else {
            return PreparationDecision::Stale(PreparationStaleReason::Released);
        };
        entry.lease_expires_at = Some(expires_at);
        entry.updated_at = now;
        PreparationDecision::Ready(self.view_of(session, entry))
    }

    /// 显式释放。只有容量端口确认 `Ok` 才把名额交还；失败/未知保留占位与真实资源事实。
    pub async fn release(
        &self,
        session: &SessionId,
        generation: ResourceGeneration,
        now: Timestamp,
    ) -> PreparationReleaseOutcome {
        let (capacity_generation, proceed) = {
            let mut state = lock(&self.state);
            let Some(entry) = state.entries.get_mut(session) else {
                return PreparationReleaseOutcome::Released;
            };
            if entry.generation != generation {
                return PreparationReleaseOutcome::Failed;
            }
            if entry.frozen || entry.releasing {
                // 正在首条提交或另一次释放：不得回收正要派发的端点。
                return PreparationReleaseOutcome::Failed;
            }
            entry.releasing = true;
            (entry.capacity_generation, true)
        };
        if !proceed {
            return PreparationReleaseOutcome::Failed;
        }
        let outcome = self
            .capacity
            .release(session.clone(), capacity_generation, now.clone())
            .await;
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return PreparationReleaseOutcome::Uncertain;
        };
        if entry.generation != generation {
            // 释放期间资源代已变化：迟到结果不得影响新代。
            entry.releasing = false;
            return PreparationReleaseOutcome::Uncertain;
        }
        match outcome {
            Ok(()) => {
                // 名额已交还：条目立即移除，释放事实只留在有界 tombstone 里（否则长期运行的目录会为
                // 每个已结束的准备保留一条内存项）。
                state.entries.remove(session);
                state.remember_released(session.clone(), generation, now);
                PreparationReleaseOutcome::Released
            }
            Err(_) => {
                // 失败/未知继续占位：条目必须保留，不能靠删除它假装腾出名额。
                entry.releasing = false;
                entry.failure = Some(PreparationFailure {
                    reason: PreparationFailureReason::ReleaseUncertain,
                });
                PreparationReleaseOutcome::Uncertain
            }
        }
    }

    /// 回收已过租约且未冻结的准备：容量确认释放后保留有界 tombstone；失败继续占位。
    pub async fn sweep_expired(&self, now: Timestamp) -> usize {
        let expired: Vec<(SessionId, ResourceGeneration)> = {
            let state = lock(&self.state);
            state
                .entries
                .iter()
                .filter(|(_, entry)| {
                    !entry.frozen
                        && !entry.releasing
                        && entry
                            .lease_expires_at
                            .as_ref()
                            .is_some_and(|expires| now >= *expires)
                })
                .map(|(session, entry)| (session.clone(), entry.generation))
                .collect()
        };
        let mut released = 0;
        for (session, generation) in expired {
            if self.release(&session, generation, now.clone()).await
                == PreparationReleaseOutcome::Released
            {
                released += 1;
            }
        }
        released
    }

    /// 首次提交冻结当前资源代与 Agent 身份：提交/派发期间释放失败并继续占位。
    pub fn freeze_for_submission(
        &self,
        session: &SessionId,
        generation: ResourceGeneration,
        runtime_epoch: &RuntimeEpoch,
        now: Timestamp,
    ) -> PreparationFreeze {
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return match state.released_generation(session) {
                Some(_) => PreparationFreeze::Stale(PreparationStaleReason::Released),
                None => PreparationFreeze::Unknown,
            };
        };
        if runtime_epoch != &self.runtime_epoch {
            return PreparationFreeze::Stale(PreparationStaleReason::RuntimeEpochMismatch);
        }
        if entry.generation != generation {
            return PreparationFreeze::Stale(PreparationStaleReason::GenerationMismatch);
        }
        let expired = entry
            .lease_expires_at
            .as_ref()
            .is_none_or(|expires| now >= *expires);
        if expired {
            return PreparationFreeze::Stale(PreparationStaleReason::Released);
        }
        if entry.state != PreparationState::Ready {
            return PreparationFreeze::Busy;
        }
        if entry.frozen || entry.releasing {
            return PreparationFreeze::Busy;
        }
        entry.frozen = true;
        entry.updated_at = now;
        PreparationFreeze::Granted(PreparationGrant {
            session: session.clone(),
            runtime_epoch: self.runtime_epoch.clone(),
            generation: entry.generation,
            agent: entry.agent.clone(),
            directory: entry.directory.clone(),
            owner: entry.owner.clone(),
        })
    }

    /// 首次提交成功：名额**转移**给正式会话，准备目录不再释放它。
    ///
    /// 条目随即从目录移除：已提交的准备不再有可观察的运行期资源，留下的只有有界 tombstone
    /// （`restore`/`renew` 因此得到 `stale(released)`，与 D1 的状态机一致）。
    pub fn finish_submission(
        &self,
        session: &SessionId,
        generation: ResourceGeneration,
        now: Timestamp,
    ) -> bool {
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get(session) else {
            return false;
        };
        if entry.generation != generation || !entry.frozen {
            return false;
        }
        state.entries.remove(session);
        state.remember_released(session.clone(), generation, now);
        true
    }

    /// 首次提交失败回滚：恢复 `ready` 与租约，保留同一资源代供显式重试。
    pub fn abort_submission(
        &self,
        session: &SessionId,
        generation: ResourceGeneration,
        now: Timestamp,
    ) -> bool {
        let mut state = lock(&self.state);
        let Some(entry) = state.entries.get_mut(session) else {
            return false;
        };
        if entry.generation != generation || !entry.frozen {
            return false;
        }
        let Some(expires_at) = now.checked_add_millis(PREPARATION_LEASE_MS) else {
            return false;
        };
        entry.frozen = false;
        entry.state = PreparationState::Ready;
        entry.failure = None;
        entry.lease_expires_at = Some(expires_at);
        entry.updated_at = now;
        true
    }

    /// 当前占用的准备名额（含失败/释放中的占位；已交还/已转移的条目已从目录移除）。
    pub fn occupied(&self) -> usize {
        lock(&self.state)
            .entries
            .values()
            .filter(|entry| entry.state.occupies_capacity())
            .count()
    }

    /// 该会话的当前运行期资源代（诊断/继续接线；不落库）。
    pub fn generation(&self, session: &SessionId) -> Option<ResourceGeneration> {
        lock(&self.state)
            .entries
            .get(session)
            .map(|entry| entry.generation)
    }
}

/// 纯 `CapacityAdmission` 适配：准备目录自己只消费 `reserve`/`release`。
#[async_trait]
impl CapacityAdmission for PreparationDirectory {
    async fn reserve(&self, session: SessionId, now: Timestamp) -> Result<Admission, PortError> {
        self.capacity.reserve(session, now).await
    }

    async fn release(
        &self,
        session: SessionId,
        generation: ResourceGeneration,
        now: Timestamp,
    ) -> Result<(), PortError> {
        self.capacity.release(session, generation, now).await
    }

    async fn reclaim_candidate(
        &self,
        now: Timestamp,
    ) -> Result<Option<crate::ports::ReclaimCandidate>, PortError> {
        self.capacity.reclaim_candidate(now).await
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::task::{Context, Poll};

    use super::*;
    use crate::broker::test_support::{block_on, run_all};
    use crate::model::{AgentId, DeviceId, Nonce, OriginEpoch, PairingId, RequestId, TurnId};
    use crate::ports::ReclaimCandidate;

    /// 测试时间戳：固定格式、逐毫秒递增。
    fn at(offset: u64) -> Timestamp {
        let base = Timestamp::new("2026-10-22T00:00:00.000Z").expect("timestamp");
        base.checked_add_millis(offset).expect("timestamp")
    }

    fn ids(seed: u64) -> SessionId {
        SessionId::new(&{
            let text = format!("{seed:08x}-1111-4111-8111-{:012x}", seed);
            text
        })
        .expect("session id")
    }

    struct TestIds(AtomicU64);

    impl IdGenerator for TestIds {
        fn session_id(&self) -> SessionId {
            let next = self.0.fetch_add(1, Ordering::SeqCst) + 1;
            ids(next)
        }
        fn turn_id(&self) -> TurnId {
            TurnId::new(&format!("{0:08x}-1111-4111-8111-{0:012x}", 0)).expect("turn")
        }
        fn interaction_id(&self) -> crate::model::InteractionId {
            crate::model::InteractionId::new(&format!("{0:08x}-1111-4111-8111-{0:012x}", 0))
                .expect("interaction")
        }
        fn pairing_id(&self) -> PairingId {
            PairingId::new(&format!("{0:08x}-1111-4111-8111-{0:012x}", 0)).expect("pairing")
        }
        fn message_id(&self) -> crate::model::MessageId {
            crate::model::MessageId::new(&format!("{0:08x}-1111-4111-8111-{0:012x}", 0))
                .expect("message")
        }
        fn origin_epoch(&self) -> OriginEpoch {
            OriginEpoch::new(&format!("{0:08x}-1111-4111-8111-{0:012x}", 0)).expect("epoch")
        }
        fn request_id(&self) -> RequestId {
            RequestId::new(&format!("{0:08x}-1111-4111-8111-{0:012x}", 0)).expect("request")
        }
    }

    #[derive(Default)]
    struct RecordingCapacity {
        reservations: Mutex<Vec<SessionId>>,
        releases: Mutex<Vec<(SessionId, ResourceGeneration)>>,
        full: std::sync::atomic::AtomicBool,
        release_fails: std::sync::atomic::AtomicBool,
    }

    /// 让出一次执行权的 future：`run_all` 的协作式执行器因此在同一个等待点上真正交错两个任务。
    struct YieldOnce(bool);

    impl Future for YieldOnce {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
            if self.0 {
                Poll::Ready(())
            } else {
                self.0 = true;
                context.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    async fn pause() {
        YieldOnce(false).await;
    }

    /// 暂停一次再冻结：两个任务因此都先让出执行权，随后在同一资源代上竞争 `Granted`。
    async fn freeze_after_pause(
        preparations: &PreparationDirectory,
        session: &SessionId,
        generation: ResourceGeneration,
        runtime_epoch: &RuntimeEpoch,
    ) -> PreparationFreeze {
        pause().await;
        preparations.freeze_for_submission(session, generation, runtime_epoch, at(2))
    }

    #[async_trait]
    impl CapacityAdmission for RecordingCapacity {
        async fn reserve(
            &self,
            session: SessionId,
            _now: Timestamp,
        ) -> Result<Admission, PortError> {
            // 真实容量端口会 await 底层预算；这里用一次让出模拟同一个等待点。
            pause().await;
            if self.full.load(Ordering::SeqCst) {
                return Ok(Admission::CapacityExceeded);
            }
            lock(&self.reservations).push(session);
            Ok(Admission::Reserved)
        }
        async fn release(
            &self,
            session: SessionId,
            generation: ResourceGeneration,
            _now: Timestamp,
        ) -> Result<(), PortError> {
            // 释放必须先让出一次：并发的第二个 `release` 才能在 `releasing` 窗口内观察并失败。
            pause().await;
            if self.release_fails.load(Ordering::SeqCst) {
                return Err(PortError::Unavailable(
                    crate::model::UnavailableKind::IoError,
                ));
            }
            lock(&self.releases).push((session, generation));
            Ok(())
        }
        async fn reclaim_candidate(
            &self,
            _now: Timestamp,
        ) -> Result<Option<ReclaimCandidate>, PortError> {
            Ok(None)
        }
    }

    fn owner() -> PreparationOwner {
        PreparationOwner::new(
            DeviceId::new("11111111-1111-4111-8111-111111111111").expect("device"),
            Nonce::new(&"A".repeat(43)).expect("nonce"),
        )
    }

    fn request() -> AllocatePreparation {
        AllocatePreparation {
            agent: AgentRef::try_new(AgentId::new("codex").expect("agent"), "Codex")
                .expect("agent"),
            directory: None,
            owner: owner(),
        }
    }

    fn directory(capacity: Arc<RecordingCapacity>) -> (PreparationDirectory, Arc<TestIds>) {
        let ids = Arc::new(TestIds(AtomicU64::new(0)));
        let runtime_epoch =
            RuntimeEpoch::new("22222222-2222-4222-8222-222222222222").expect("epoch");
        (
            PreparationDirectory::new(ids.clone(), capacity, runtime_epoch),
            ids,
        )
    }

    #[test]
    fn allocate_is_memory_only_and_uses_the_owner_id() {
        block_on(async {
            let capacity = Arc::new(RecordingCapacity::default());
            let (preparations, _ids) = directory(capacity.clone());
            let outcome = preparations.allocate(request(), at(0)).await;
            let Ok(AllocatePreparationOutcome::Allocated(view)) = outcome else {
                panic!("expected allocation");
            };
            assert_eq!(view.state, PreparationState::Preparing);
            assert_eq!(view.generation, ResourceGeneration::new(1));
            assert_eq!(view.lease_expires_at, Some(at(PREPARATION_LEASE_MS)));
            assert_eq!(lock(&capacity.reservations).len(), 1);
            assert_eq!(preparations.occupied(), 1);
            // 没有 store/publisher 依赖：本类型无法写 owned 行/事件/命令收据。
            assert!(preparations.view(&view.session).is_some());
        });
    }

    #[test]
    fn capacity_exceeded_does_not_create_an_entry() {
        block_on(async {
            let capacity = Arc::new(RecordingCapacity::default());
            capacity.full.store(true, Ordering::SeqCst);
            let (preparations, _ids) = directory(capacity);
            let outcome = preparations
                .allocate(request(), at(0))
                .await
                .expect("allocate");
            assert_eq!(outcome, AllocatePreparationOutcome::CapacityExceeded);
            assert_eq!(preparations.occupied(), 0);
        });
    }

    #[test]
    fn lease_expiry_renews_or_rebuilds_with_a_new_generation() {
        block_on(async {
            let capacity = Arc::new(RecordingCapacity::default());
            let (preparations, _ids) = directory(capacity.clone());
            let Ok(AllocatePreparationOutcome::Allocated(view)) =
                preparations.allocate(request(), at(0)).await
            else {
                panic!("expected allocation");
            };
            // 10 秒续租节奏内仍是同一代。
            let renewed = preparations.renew(&view.session, view.generation, at(10_000));
            assert!(matches!(renewed, PreparationDecision::Ready(_)));
            // 到期后续租 stale(released)，restore 分配新代并回到 preparing。
            let expired = preparations.renew(&view.session, view.generation, at(40_000));
            assert_eq!(
                expired,
                PreparationDecision::Stale(PreparationStaleReason::Released)
            );
            let restored = preparations.restore(&view.session, &owner(), at(40_000));
            let PreparationDecision::Ready(next) = restored else {
                panic!("restore rebuilds the same id");
            };
            assert_eq!(next.generation, ResourceGeneration::new(2));
            assert_eq!(next.state, PreparationState::Preparing);
            assert_eq!(next.session, view.session);
            assert_eq!(
                lock(&capacity.releases).len(),
                0,
                "restore must not release"
            );
        });
    }

    #[test]
    fn release_failure_keeps_the_slot_and_the_entry() {
        block_on(async {
            let capacity = Arc::new(RecordingCapacity::default());
            capacity.release_fails.store(true, Ordering::SeqCst);
            let (preparations, _ids) = directory(capacity.clone());
            let Ok(AllocatePreparationOutcome::Allocated(view)) =
                preparations.allocate(request(), at(0)).await
            else {
                panic!("expected allocation");
            };
            assert_eq!(
                preparations
                    .release(&view.session, view.generation, at(1))
                    .await,
                PreparationReleaseOutcome::Uncertain
            );
            assert_eq!(preparations.occupied(), 1, "失败必须继续占位");
            let kept = preparations.view(&view.session).expect("entry kept");
            assert_eq!(
                kept.failure.map(|failure| failure.reason),
                Some(PreparationFailureReason::ReleaseUncertain)
            );
            // 失败后仍然可重试释放；成功后才交还名额并留下有界 tombstone。
            capacity.release_fails.store(false, Ordering::SeqCst);
            assert_eq!(
                preparations
                    .release(&view.session, view.generation, at(2))
                    .await,
                PreparationReleaseOutcome::Released
            );
            assert_eq!(preparations.occupied(), 0);
            assert_eq!(
                preparations.renew(&view.session, view.generation, at(3)),
                PreparationDecision::Stale(PreparationStaleReason::Released)
            );
        });
    }

    #[test]
    fn concurrent_freezes_grant_exactly_one_and_finish_transfers_the_slot() {
        block_on(async {
            let capacity = Arc::new(RecordingCapacity::default());
            let (preparations, _ids) = directory(capacity.clone());
            let Ok(AllocatePreparationOutcome::Allocated(view)) =
                preparations.allocate(request(), at(0)).await
            else {
                panic!("expected allocation");
            };
            let epoch = preparations.runtime_epoch().clone();
            // 只有 ready 的资源可被首次受理冻结。
            assert!(matches!(
                preparations.freeze_for_submission(&view.session, view.generation, &epoch, at(1)),
                PreparationFreeze::Busy
            ));
            let PreparationDecision::Ready(ready) =
                preparations.mark_ready(&view.session, view.generation, at(1))
            else {
                panic!("ready");
            };
            assert_eq!(ready.state, PreparationState::Ready);
            // 两个任务在同一资源代上并发冻结：CAS 语义下恰好一个 `Granted`，另一个 `Busy`。
            let grants = run_all(vec![
                freeze_after_pause(&preparations, &view.session, view.generation, &epoch),
                freeze_after_pause(&preparations, &view.session, view.generation, &epoch),
            ]);
            assert_eq!(
                grants
                    .iter()
                    .filter(|grant| matches!(grant, PreparationFreeze::Granted(_)))
                    .count(),
                1,
                "并发冻结只能有一个 Granted"
            );
            assert_eq!(
                grants
                    .iter()
                    .filter(|grant| matches!(grant, PreparationFreeze::Busy))
                    .count(),
                1
            );
            // 冻结期间释放必须失败并继续占位（并发提交不得被回收）。
            assert_eq!(
                preparations
                    .release(&view.session, view.generation, at(3))
                    .await,
                PreparationReleaseOutcome::Failed
            );
            assert_eq!(preparations.occupied(), 1);
            // 提交成功：名额转移，不调用容量 release，条目随有界 tombstone 一起消失。
            assert!(preparations.finish_submission(&view.session, view.generation, at(4)));
            assert_eq!(lock(&capacity.releases).len(), 0, "名额必须转移给正式会话");
            assert_eq!(preparations.occupied(), 0);
            assert!(preparations.view(&view.session).is_none());
            assert_eq!(
                preparations.restore(&view.session, &owner(), at(5)),
                PreparationDecision::Stale(PreparationStaleReason::Released)
            );
        });
    }

    #[test]
    fn concurrent_release_hands_the_slot_back_exactly_once() {
        block_on(async {
            // 容量端口每次调用都先让出一次执行权：两个 `release` 因此真的会在释放窗口内交错。
            let capacity = Arc::new(RecordingCapacity::default());
            let (preparations, _ids) = directory(capacity.clone());
            let Ok(AllocatePreparationOutcome::Allocated(view)) =
                preparations.allocate(request(), at(0)).await
            else {
                panic!("expected allocation");
            };
            preparations.mark_ready(&view.session, view.generation, at(1));
            let outcomes = run_all(vec![
                preparations.release(&view.session, view.generation, at(2)),
                preparations.release(&view.session, view.generation, at(2)),
            ]);
            assert_eq!(
                outcomes
                    .iter()
                    .filter(|outcome| **outcome == PreparationReleaseOutcome::Released)
                    .count(),
                1,
                "并发释放只能有一个 Released"
            );
            assert_eq!(
                outcomes
                    .iter()
                    .filter(|outcome| **outcome == PreparationReleaseOutcome::Failed)
                    .count(),
                1,
                "第二个并发释放必须看到 releasing 并失败（不得二次交还名额）"
            );
            assert_eq!(lock(&capacity.releases).len(), 1, "容量只能交还一次");
            assert_eq!(preparations.occupied(), 0);
            assert!(
                preparations.view(&view.session).is_none(),
                "已释放条目被移除"
            );
            assert_eq!(
                preparations.renew(&view.session, view.generation, at(3)),
                PreparationDecision::Stale(PreparationStaleReason::Released)
            );
        });
    }

    #[test]
    fn aborted_submission_returns_to_ready_with_the_same_generation() {
        block_on(async {
            let capacity = Arc::new(RecordingCapacity::default());
            let (preparations, _ids) = directory(capacity);
            let Ok(AllocatePreparationOutcome::Allocated(view)) =
                preparations.allocate(request(), at(0)).await
            else {
                panic!("expected allocation");
            };
            let epoch = preparations.runtime_epoch().clone();
            preparations.mark_ready(&view.session, view.generation, at(1));
            assert!(matches!(
                preparations.freeze_for_submission(&view.session, view.generation, &epoch, at(2)),
                PreparationFreeze::Granted(_)
            ));
            assert!(preparations.abort_submission(&view.session, view.generation, at(3)));
            let restored = preparations.view(&view.session).expect("entry");
            assert_eq!(restored.state, PreparationState::Ready);
            assert_eq!(restored.generation, view.generation);
        });
    }
}
