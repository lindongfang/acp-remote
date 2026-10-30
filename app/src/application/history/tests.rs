//! Pure clock/output-port fixtures. These tests do not claim a real Harness was stopped.
use super::*;
use crate::domain::{
    AcceptedInput, CommandRecord, CommandStatus,
    history::{RunId, TurnId},
};
use std::sync::atomic::{AtomicBool, AtomicUsize};
use tokio::sync::Semaphore;
async fn bounded<F: Future>(future: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(10), future)
        .await
        .expect("fixture wait exceeded10s")
}
#[derive(Clone, Default)]
struct Clock(Arc<AtomicU64>);
impl Clock {
    fn set(&self, value: u64) {
        self.0.store(value, Ordering::Release);
    }
}
impl HistoryClock for Clock {
    fn millis(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }
}
struct Gate {
    entered: Semaphore,
    release: Semaphore,
}
impl Gate {
    fn new() -> Self {
        Self {
            entered: Semaphore::new(0),
            release: Semaphore::new(0),
        }
    }
}
#[derive(Default)]
struct RepositoryState {
    next: Mutex<HashMap<OutputScope, u64>>,
    calls: AtomicUsize,
    active: AtomicUsize,
    peak: AtomicUsize,
    fail: AtomicBool,
    block: Mutex<Option<Arc<Gate>>>,
    versions: Mutex<HashMap<ItemId, u64>>,
}
#[derive(Clone, Default)]
struct Repository(Arc<RepositoryState>);
struct Active(Arc<RepositoryState>);
impl Drop for Active {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::AcqRel);
    }
}
impl HistoryRepository for Repository {
    fn reserve(
        &self,
        scope: OutputScope,
        count: u32,
    ) -> impl Future<Output = Result<PositionReservation, HistoryError>> {
        let mut map = self.0.next.lock().expect("fixture position map");
        let first = *map.entry(scope).or_insert(0);
        let end = first + u64::from(count);
        map.insert(scope, end);
        drop(map);
        std::future::ready(Ok(PositionReservation { next: first, end }))
    }
    async fn save(
        &self,
        _: OutputScope,
        items: &[HistoryItem],
        _: IntegrityChange,
    ) -> Result<Vec<CommitAck>, HistoryError> {
        self.0.calls.fetch_add(1, Ordering::AcqRel);
        let active = self.0.active.fetch_add(1, Ordering::AcqRel) + 1;
        self.0.peak.fetch_max(active, Ordering::AcqRel);
        let _active = Active(Arc::clone(&self.0));
        let gate = self.0.block.lock().expect("fixture gate").take();
        if let Some(gate) = gate {
            gate.entered.add_permits(1);
            gate.release
                .acquire()
                .await
                .expect("fixture release")
                .forget();
        }
        if self.0.fail.load(Ordering::Acquire) {
            return Err(HistoryError::StorageUnavailable);
        }
        let mut stored = self.0.versions.lock().expect("fixture versions");
        Ok(items
            .iter()
            .map(|item| {
                let version = stored.entry(item.identity.item).or_insert(0);
                *version = (*version).max(item.version.value());
                CommitAck {
                    item: item.identity.item,
                    version: ContentVersion::new(*version).expect("version"),
                }
            })
            .collect())
    }
}
#[derive(Default)]
struct OutputState {
    requests: Mutex<Vec<(OutputScope, IntegrityFault)>>,
    retained: AtomicBool,
    finished: AtomicUsize,
}
#[derive(Clone, Default)]
struct Output(Arc<OutputState>);
impl OutputControl for Output {
    fn request_stop(&self, scope: OutputScope, fault: IntegrityFault) -> StopReceipt {
        self.0
            .requests
            .lock()
            .expect("stop fixture")
            .push((scope, fault));
        if self.0.retained.load(Ordering::Acquire) {
            StopReceipt::ResponsibilityRetained
        } else {
            StopReceipt::Requested
        }
    }
    fn finish_stop(&self, _: OutputScope) -> impl Future<Output = Result<(), HistoryError>> {
        self.0.finished.fetch_add(1, Ordering::AcqRel);
        std::future::ready(if self.0.retained.load(Ordering::Acquire) {
            Err(HistoryError::OwnerUnavailable)
        } else {
            Ok(())
        })
    }
}
fn id(value: u64) -> EntityId {
    let mut bytes = [0; 16];
    bytes[..8].copy_from_slice(&value.to_le_bytes());
    EntityId::from_bytes(bytes)
}
fn scope(value: u64) -> OutputScope {
    OutputScope {
        session: SessionId(id(value)),
        run: RunId(id(value + 1000)),
        turn: TurnId(id(value + 2000)),
    }
}
type Owner = HistoryCoordinator<Repository, Clock, Output>;
fn fixture() -> (Owner, Repository, Clock, Output) {
    let repository = Repository::default();
    let clock = Clock::default();
    let output = Output::default();
    (
        HistoryCoordinator::new(repository.clone(), clock.clone(), output.clone()),
        repository,
        clock,
        output,
    )
}
async fn item(owner: &Owner, scope: OutputScope, source: u64, bytes: usize) -> ItemHandle {
    owner
        .begin_item(
            scope,
            SourceId(id(source)),
            Arc::from("text"),
            Arc::from("acp"),
            Arc::from("1"),
            vec![b'x'; bytes],
        )
        .await
        .expect("begin item")
}
const ITEM_OVERHEAD: usize = 1024 + 4 + 3 + 1;
fn payload_at_retained(target: usize) -> usize {
    target - SCOPE_OVERHEAD - ITEM_OVERHEAD
}

#[tokio::test]
async fn time_and_dirty_bytes_use_first_boundary_without_waiting_for_both() {
    let (owner, repository, clock, _) = fixture();
    let scope = scope(1);
    owner.attach(scope).await.expect("attach");
    item(&owner, scope, 3, 1).await;
    clock.set(499);
    assert_eq!(
        owner
            .flush(scope, FlushReason::Periodic)
            .await
            .expect("499"),
        FlushOutcome::NotDue
    );
    assert_eq!(repository.0.calls.load(Ordering::Acquire), 0);
    clock.set(500);
    assert_eq!(
        owner
            .flush(scope, FlushReason::Periodic)
            .await
            .expect("500"),
        FlushOutcome::Committed
    );
    assert_eq!(repository.0.calls.load(Ordering::Acquire), 1);
    let (owner, repository, clock, _) = fixture();
    owner.attach(scope).await.expect("attach");
    let handle = item(&owner, scope, 3, SAVE_BYTES - 1).await;
    clock.set(1);
    assert_eq!(
        owner
            .flush(scope, FlushReason::Periodic)
            .await
            .expect("size minus1"),
        FlushOutcome::NotDue
    );
    owner
        .update_item(&handle, vec![b'y'; 1])
        .await
        .expect("one more dirty byte");
    assert_eq!(
        owner
            .flush(scope, FlushReason::Periodic)
            .await
            .expect("size boundary"),
        FlushOutcome::Committed
    );
    assert_eq!(repository.0.calls.load(Ordering::Acquire), 1);
    let (owner, repository, _, _) = fixture();
    owner.attach(scope).await.expect("attach");
    item(&owner, scope, 3, SAVE_BYTES).await;
    owner.tick().await.expect("exact64KiB");
    assert_eq!(repository.0.calls.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn every_forced_boundary_flushes_without_a_periodic_delay() {
    for reason in [
        FlushReason::ItemEnd,
        FlushReason::RunEnd,
        FlushReason::BeforeApproval,
        FlushReason::Release,
        FlushReason::Shutdown,
    ] {
        let (owner, repository, _, _) = fixture();
        let scope = scope(1);
        owner.attach(scope).await.expect("attach");
        let handle = item(&owner, scope, 3, 1).await;
        assert_eq!(handle.committed_version(), 0);
        assert_eq!(
            owner.flush(scope, reason).await.expect("forced"),
            FlushOutcome::Committed
        );
        assert_eq!(handle.committed_version(), 1);
        assert_eq!(repository.0.calls.load(Ordering::Acquire), 1);
    }
}

#[tokio::test]
async fn in_flight_old_version_remains_charged_and_its_ack_does_not_clear_new_dirty() {
    let (owner, repository, _, _) = fixture();
    let scope = scope(1);
    owner.attach(scope).await.expect("attach");
    let handle = item(&owner, scope, 3, 100).await;
    let gate = Arc::new(Gate::new());
    *repository.0.block.lock().expect("block") = Some(Arc::clone(&gate));
    let saving = {
        let owner = owner.clone();
        tokio::spawn(async move { owner.flush(scope, FlushReason::ItemEnd).await })
    };
    bounded(gate.entered.acquire())
        .await
        .expect("entered")
        .forget();
    owner
        .update_item(&handle, vec![b'y'; 200])
        .await
        .expect("new snapshot");
    let status = owner.status(scope).expect("status");
    assert!(status.in_flight);
    assert_eq!(
        status.retained_bytes,
        SCOPE_OVERHEAD + 2 * ITEM_OVERHEAD + 100 + 200
    );
    assert_eq!(
        (handle.current_version(), handle.committed_version()),
        (2, 0)
    );
    assert_eq!(
        owner
            .flush(scope, FlushReason::Periodic)
            .await
            .expect("single flight"),
        FlushOutcome::NotDue
    );
    assert_eq!(repository.0.calls.load(Ordering::Acquire), 1);
    gate.release.add_permits(1);
    bounded(saving).await.expect("join").expect("v1 commit");
    assert_eq!(
        (handle.current_version(), handle.committed_version()),
        (2, 1)
    );
    assert_eq!(
        owner.status(scope).expect("remaining").retained_bytes,
        SCOPE_OVERHEAD + ITEM_OVERHEAD + 200
    );
    owner
        .flush(scope, FlushReason::RunEnd)
        .await
        .expect("v2 commit");
    assert_eq!(handle.committed_version(), 2);
    assert_eq!(
        owner.status(scope).expect("drained").retained_bytes,
        SCOPE_OVERHEAD
    );
    assert_eq!(repository.0.peak.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn a_forced_flush_waits_for_the_existing_flight_then_saves_its_newer_version() {
    let (owner, repository, _, _) = fixture();
    let scope = scope(1);
    owner.attach(scope).await.expect("attach");
    let handle = item(&owner, scope, 3, 1).await;
    let gate = Arc::new(Gate::new());
    *repository.0.block.lock().expect("block") = Some(Arc::clone(&gate));
    let first = {
        let owner = owner.clone();
        tokio::spawn(async move { owner.flush(scope, FlushReason::ItemEnd).await })
    };
    gate.entered.acquire().await.expect("entered").forget();
    owner.update_item(&handle, vec![b'z'; 2]).await.expect("v2");
    let second = {
        let owner = owner.clone();
        tokio::spawn(async move { owner.flush(scope, FlushReason::Release).await })
    };
    tokio::task::yield_now().await;
    assert_eq!(repository.0.calls.load(Ordering::Acquire), 1);
    gate.release.add_permits(1);
    bounded(first).await.expect("first").expect("v1");
    bounded(second).await.expect("second").expect("v2");
    assert_eq!(handle.committed_version(), 2);
    assert_eq!(repository.0.peak.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn retry_249_250_and_three_failures_stop_once_and_success_resets_the_counter() {
    let (owner, repository, clock, output) = fixture();
    let scope = scope(1);
    owner.attach(scope).await.expect("attach");
    item(&owner, scope, 3, SAVE_BYTES).await;
    repository.0.fail.store(true, Ordering::Release);
    assert!(owner.tick().await.is_err());
    assert!(!owner.status(scope).expect("first").stopped);
    clock.set(249);
    owner.tick().await.expect("not retrying");
    assert_eq!(repository.0.calls.load(Ordering::Acquire), 1);
    clock.set(250);
    assert!(owner.tick().await.is_err());
    assert!(!owner.status(scope).expect("second").stopped);
    clock.set(500);
    assert!(owner.tick().await.is_err());
    assert!(owner.status(scope).expect("third").stopped);
    assert_eq!(output.0.requests.lock().expect("requests").len(), 1);
    let (owner, repository, clock, _) = fixture();
    owner.attach(scope).await.expect("attach");
    let handle = item(&owner, scope, 3, SAVE_BYTES).await;
    repository.0.fail.store(true, Ordering::Release);
    assert!(owner.tick().await.is_err());
    clock.set(250);
    assert!(owner.tick().await.is_err());
    repository.0.fail.store(false, Ordering::Release);
    clock.set(500);
    owner.tick().await.expect("successful retry");
    assert!(owner.status(scope).expect("reset").fault.is_none());
    repository.0.fail.store(true, Ordering::Release);
    owner
        .update_item(&handle, vec![b'q'; SAVE_BYTES])
        .await
        .expect("new dirty");
    clock.set(750);
    assert!(owner.tick().await.is_err());
    assert!(!owner.status(scope).expect("new first failure").stopped);
}

#[tokio::test]
async fn five_second_failure_watchdog_does_not_wait_for_a_blocked_retry() {
    let (owner, repository, clock, output) = fixture();
    let scope = scope(1);
    owner.attach(scope).await.expect("attach");
    item(&owner, scope, 3, SAVE_BYTES).await;
    repository.0.fail.store(true, Ordering::Release);
    assert!(owner.tick().await.is_err());
    let gate = Arc::new(Gate::new());
    *repository.0.block.lock().expect("block") = Some(Arc::clone(&gate));
    clock.set(250);
    let blocked = {
        let owner = owner.clone();
        tokio::spawn(async move { owner.flush(scope, FlushReason::Periodic).await })
    };
    gate.entered.acquire().await.expect("entered").forget();
    clock.set(4999);
    owner.check_deadlines().expect("4999");
    assert!(!owner.status(scope).expect("not yet").stopped);
    clock.set(5000);
    owner.check_deadlines().expect("5000");
    let status = owner.status(scope).expect("stopped");
    assert!(status.stopped && status.in_flight);
    assert_eq!(
        status.fault.expect("fault").reason,
        IntegrityReason::PersistentFailure
    );
    assert_eq!(output.0.requests.lock().expect("request").len(), 1);
    blocked.abort();
    let _ = bounded(blocked).await;
    assert!(!owner.status(scope).expect("cancelled future").in_flight);
    assert_eq!(
        owner
            .status(scope)
            .expect("preserved fault")
            .fault
            .expect("fault")
            .reason,
        IntegrityReason::PersistentFailure
    );
}

#[derive(Clone, Default)]
struct InputFixture(Arc<AtomicUsize>);
impl InputRepository for InputFixture {
    fn admit(
        &self,
        _: ActorContext,
        _: InputRequest,
    ) -> impl Future<Output = Result<Admission, SessionError>> {
        self.0.fetch_add(1, Ordering::AcqRel);
        std::future::ready(Ok(Admission {
            command: CommandRecord {
                input: AcceptedInput {
                    command_id: "fixture".into(),
                    run_id: "fixture".into(),
                    turn_id: "fixture".into(),
                    item_id: "fixture".into(),
                },
                status: CommandStatus::Accepted,
            },
            replayed: false,
        }))
    }
}
#[tokio::test]
async fn session_high_water_and_hard_limit_have_exact_retained_byte_boundaries() {
    let (owner, _, _, output) = fixture();
    let scope = scope(1);
    owner.attach(scope).await.expect("attach");
    let high = high(SESSION_CACHE_LIMIT);
    let handle = item(&owner, scope, 3, payload_at_retained(high - 1)).await;
    assert!(owner.status(scope).expect("before high").input_allowed);
    owner
        .update_item(&handle, vec![b'y'; payload_at_retained(high)])
        .await
        .expect("at high");
    assert!(!owner.status(scope).expect("at high").input_allowed);
    let input = InputFixture::default();
    let service = InputService::new(input.clone());
    let actor = ActorContext {
        client_id: id(8),
        auth_generation: 1,
    };
    let request = InputRequest {
        request_id: id(9),
        session_id: scope.session.0,
        content_ref: id(10),
    };
    assert!(matches!(
        owner.submit_input(&service, actor, request).await,
        Err(SessionError::Busy)
    ));
    assert_eq!(input.0.load(Ordering::Acquire), 0);
    assert_eq!(
        owner
            .flush(scope, FlushReason::Periodic)
            .await
            .expect("high flush"),
        FlushOutcome::Committed
    );
    assert!(owner.status(scope).expect("recovered budget").input_allowed);
    owner
        .update_item(
            &handle,
            vec![b'y'; payload_at_retained(SESSION_CACHE_LIMIT - 1)],
        )
        .await
        .expect("below hard");
    assert!(!owner.status(scope).expect("not stopped").stopped);
    owner
        .update_item(
            &handle,
            vec![b'z'; payload_at_retained(SESSION_CACHE_LIMIT)],
        )
        .await
        .expect("at hard retained");
    let status = owner.status(scope).expect("hard");
    assert_eq!(status.retained_bytes, SESSION_CACHE_LIMIT);
    assert!(status.stopped);
    assert_eq!(output.0.requests.lock().expect("stop once").len(), 1);
    assert!(matches!(
        owner.update_item(&handle, vec![b'!'; 7]).await,
        Err(HistoryError::Stopped)
    ));
    assert_eq!(
        owner
            .status(scope)
            .expect("dropped")
            .fault
            .expect("fault")
            .possible_loss
            .expect("range")
            .dropped_bytes,
        7
    );
}

#[tokio::test]
async fn in_flight_snapshot_and_new_version_cannot_bypass_the_session_hard_cap() {
    let (owner, repository, _, output) = fixture();
    let scope = scope(1);
    owner.attach(scope).await.expect("attach");
    let handle = item(&owner, scope, 3, 9 * 1024 * 1024).await;
    let gate = Arc::new(Gate::new());
    *repository.0.block.lock().expect("block") = Some(Arc::clone(&gate));
    let blocked = {
        let owner = owner.clone();
        tokio::spawn(async move { owner.flush(scope, FlushReason::Periodic).await })
    };
    gate.entered.acquire().await.expect("entered").forget();
    assert!(matches!(
        owner
            .update_item(&handle, vec![b'z'; 8 * 1024 * 1024])
            .await,
        Err(HistoryError::Capacity)
    ));
    let status = owner.status(scope).expect("bound");
    assert!(status.retained_bytes < SESSION_CACHE_LIMIT);
    assert!(status.stopped);
    assert_eq!(output.0.requests.lock().expect("stop").len(), 1);
    assert_eq!(
        status
            .fault
            .expect("fault")
            .possible_loss
            .expect("range")
            .dropped_bytes,
        8 * 1024 * 1024
    );
    blocked.abort();
    let _ = blocked.await;
}

#[tokio::test]
async fn node_budget_is_shared_across_sessions_and_exact_node_high_water_flushes() {
    let (owner, _, _, _) = fixture();
    let high = high(NODE_CACHE_LIMIT);
    let scopes: Vec<_> = (1..=16).map(scope).collect();
    let mut handles = Vec::new();
    let target = (high - 1) / 16;
    let extra = (high - 1) % 16;
    for (index, scope) in scopes.iter().enumerate() {
        owner.attach(*scope).await.expect("attach");
        handles.push(
            item(
                &owner,
                *scope,
                100 + u64::try_from(index).expect("index"),
                payload_at_retained(target + usize::from(index < extra)),
            )
            .await,
        );
    }
    let before = owner.status(scopes[0]).expect("before");
    assert_eq!(before.node_retained_bytes, high - 1);
    assert!(before.input_allowed);
    let last = handles.last().expect("last");
    let old_len = owner
        .core
        .state
        .lock()
        .expect("state")
        .buffers
        .get(&last.identity.scope)
        .expect("buffer")
        .pending
        .get(&last.identity.item)
        .expect("entry")
        .item
        .content
        .0
        .len();
    owner
        .update_item(last, vec![b'y'; old_len + 1])
        .await
        .expect("one node byte");
    let at = owner.status(scopes[0]).expect("at");
    assert_eq!(at.node_retained_bytes, high);
    assert!(!at.input_allowed);
    assert!(!at.stopped);
    assert_eq!(
        owner
            .flush(scopes[0], FlushReason::Periodic)
            .await
            .expect("node high flush"),
        FlushOutcome::Committed
    );
}

#[tokio::test]
async fn exact_node_hard_limit_requests_all_owners_and_concurrent_reservations_remain_bounded() {
    let (owner, _, _, output) = fixture();
    let scopes: Vec<_> = (1..=16).map(scope).collect();
    let each = NODE_CACHE_LIMIT / 16;
    let mut handles = Vec::new();
    for (index, scope) in scopes.iter().enumerate() {
        owner.attach(*scope).await.expect("attach");
        handles.push(
            item(
                &owner,
                *scope,
                100 + u64::try_from(index).expect("index"),
                payload_at_retained(each - usize::from(index == 15)),
            )
            .await,
        );
    }
    assert_eq!(
        owner.status(scopes[0]).expect("below").node_retained_bytes,
        NODE_CACHE_LIMIT - 1
    );
    let last = handles.last().expect("last");
    owner
        .update_item(last, vec![b'y'; payload_at_retained(each)])
        .await
        .expect("node hard");
    for scope in &scopes {
        let status = owner.status(*scope).expect("status");
        assert!(status.stopped);
        assert_eq!(status.node_retained_bytes, NODE_CACHE_LIMIT);
        assert!(status.retained_bytes < SESSION_CACHE_LIMIT);
    }
    assert_eq!(output.0.requests.lock().expect("all scopes").len(), 16);
    drop(handles);
    drop(owner);
    let (owner, _, _, _) = fixture();
    let mut tasks = tokio::task::JoinSet::new();
    let mut handles = Vec::new();
    for (index, scope) in scopes.iter().enumerate() {
        owner.attach(*scope).await.expect("attach");
        handles.push(
            item(
                &owner,
                *scope,
                100 + u64::try_from(index).expect("index"),
                7 * 1024 * 1024,
            )
            .await,
        );
    }
    for handle in handles {
        let owner = owner.clone();
        tasks.spawn(async move {
            owner
                .update_item(&handle, vec![b'z'; 9 * 1024 * 1024])
                .await
        });
    }
    let mut rejected = 0;
    while let Some(result) = bounded(tasks.join_next()).await {
        if result.expect("join").is_err() {
            rejected += 1;
        }
    }
    assert!(rejected > 0);
    assert!(
        owner
            .status(scopes[0])
            .expect("bounded")
            .node_retained_bytes
            <= NODE_CACHE_LIMIT
    );
}

#[tokio::test]
async fn empty_items_and_metadata_are_bounded_even_when_payload_bytes_are_zero() {
    let (owner, _, _, _) = fixture();
    let scope = scope(1);
    owner.attach(scope).await.expect("attach");
    let mut count = 0u64;
    loop {
        let result = owner
            .begin_item(
                scope,
                SourceId(id(count + 100)),
                Arc::from("text"),
                Arc::from("acp"),
                Arc::from("1"),
                Vec::new(),
            )
            .await;
        match result {
            Ok(_) => count += 1,
            Err(HistoryError::Capacity | HistoryError::Stopped) => break,
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(count < 16384);
    let status = owner.status(scope).expect("metadata bound");
    assert!(status.retained_bytes <= SESSION_CACHE_LIMIT);
    assert!(status.stopped);
}

#[tokio::test]
async fn forced_failure_returns_error_keeps_resource_responsibility_and_reports_the_range() {
    let (owner, repository, _, output) = fixture();
    output.0.retained.store(true, Ordering::Release);
    let scope = scope(1);
    owner.attach(scope).await.expect("attach");
    let handle = item(&owner, scope, 3, 4).await;
    repository.0.fail.store(true, Ordering::Release);
    assert!(matches!(
        owner.flush(scope, FlushReason::BeforeApproval).await,
        Err(HistoryError::StorageUnavailable)
    ));
    let status = owner.status(scope).expect("failure");
    assert!(status.stopped);
    assert_eq!(
        status.stop_receipt,
        Some(StopReceipt::ResponsibilityRetained)
    );
    let range = status.fault.expect("error").possible_loss.expect("range");
    assert_eq!(
        (range.first, range.last),
        (handle.identity.position, handle.identity.position)
    );
    assert!(owner.close(Duration::from_millis(100)).await.is_err());
    assert_eq!(output.0.finished.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn owned_worker_flushes_size_immediately_and_drop_cancels_its_pending_future() {
    let (owner, repository, _, output) = fixture();
    let scope = scope(1);
    owner.attach(scope).await.expect("attach");
    item(&owner, scope, 3, SAVE_BYTES).await;
    let gate = Arc::new(Gate::new());
    *repository.0.block.lock().expect("block") = Some(Arc::clone(&gate));
    let task = HistoryTask::start(owner.clone());
    tokio::time::timeout(Duration::from_secs(1), gate.entered.acquire())
        .await
        .expect("scheduled before500ms")
        .expect("entered")
        .forget();
    drop(task);
    tokio::time::timeout(Duration::from_secs(1), async {
        while owner.status(scope).expect("status").in_flight {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cancelled child joined by runtime");
    assert!(owner.status(scope).expect("fault").stopped);
    assert_eq!(output.0.requests.lock().expect("requested").len(), 1);
    assert_eq!(repository.0.active.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn close_timeout_does_not_report_success_and_clock_reversal_is_not_hidden() {
    let (owner, repository, clock, output) = fixture();
    let scope = scope(1);
    owner.attach(scope).await.expect("attach");
    clock.set(10);
    let handle = item(&owner, scope, 3, 1).await;
    clock.set(9);
    assert!(matches!(
        owner.update_item(&handle, vec![1]).await,
        Err(HistoryError::ClockReversed)
    ));
    clock.set(10);
    let gate = Arc::new(Gate::new());
    *repository.0.block.lock().expect("block") = Some(Arc::clone(&gate));
    let blocked = {
        let owner = owner.clone();
        tokio::spawn(async move { owner.flush(scope, FlushReason::ItemEnd).await })
    };
    gate.entered.acquire().await.expect("entered").forget();
    assert!(matches!(
        owner.close(Duration::from_millis(5)).await,
        Err(HistoryError::CloseTimeout)
    ));
    assert_eq!(output.0.requests.lock().expect("stop").len(), 1);
    blocked.abort();
    let _ = blocked.await;
}

#[tokio::test]
async fn old_ack_does_not_clear_new_dirty_bytes_or_its_first_dirty_clock() {
    let (owner, repository, clock, _) = fixture();
    let scope = scope(1);
    owner.attach(scope).await.expect("attach");
    let handle = item(&owner, scope, 3, 1).await;
    let gate = Arc::new(Gate::new());
    *repository.0.block.lock().expect("gate") = Some(Arc::clone(&gate));
    let first = {
        let owner = owner.clone();
        tokio::spawn(async move { owner.flush(scope, FlushReason::ItemEnd).await })
    };
    gate.entered.acquire().await.expect("entered").forget();
    clock.set(10);
    owner
        .update_item(&handle, vec![b'b'; 65535])
        .await
        .expect("new dirty65535");
    gate.release.add_permits(1);
    first.await.expect("join").expect("old commit");
    clock.set(11);
    assert_eq!(
        owner
            .flush(scope, FlushReason::Periodic)
            .await
            .expect("below64KiB"),
        FlushOutcome::NotDue
    );
    assert_eq!(repository.0.calls.load(Ordering::Acquire), 1);
    owner
        .update_item(&handle, vec![b'c'; 1])
        .await
        .expect("cumulative one byte");
    assert_eq!(
        owner
            .flush(scope, FlushReason::Periodic)
            .await
            .expect("64KiB survived oldack"),
        FlushOutcome::Committed
    );
    assert_eq!(handle.committed_version(), 3);
    let handle = item(&owner, scope, 4, 1).await;
    clock.set(510);
    assert_eq!(
        owner
            .flush(scope, FlushReason::Periodic)
            .await
            .expect("499ms new dirty"),
        FlushOutcome::NotDue
    );
    clock.set(511);
    owner.tick().await.expect("500ms new dirty");
    assert_eq!(handle.committed_version(), 1);
}
#[tokio::test]
async fn reliable_run_end_retires_only_clean_history_owner_metadata() {
    let (owner, _, _, _) = fixture();
    let scope = scope(1);
    owner.attach(scope).await.expect("attach");
    let handle = item(&owner, scope, 3, 1).await;
    owner
        .finish_run(scope)
        .await
        .expect("confirmed terminal fixture");
    assert_eq!(handle.committed_version(), 1);
    assert!(matches!(
        owner.status(scope),
        Err(HistoryError::InvalidState)
    ));
    assert_eq!(owner.core.state.lock().expect("state").retained(), 0);
}

#[tokio::test]
async fn handles_cannot_cross_history_owners_and_scope_costs_aggregate_by_session() {
    let (first, _, _, _) = fixture();
    let (second, _, _, _) = fixture();
    let scope = scope(1);
    first.attach(scope).await.expect("first");
    second.attach(scope).await.expect("second");
    let handle = item(&first, scope, 3, 1).await;
    assert!(matches!(
        second.update_item(&handle, vec![1]).await,
        Err(HistoryError::InvalidState)
    ));
    assert_eq!(handle.current_version(), 1);
    let (owner, _, _, _) = fixture();
    let other = OutputScope {
        run: RunId(id(77)),
        turn: TurnId(id(78)),
        ..scope
    };
    owner.attach(scope).await.expect("scope1");
    owner.attach(other).await.expect("scope2");
    let first = item(&owner, scope, 4, 8 * 1024 * 1024).await;
    assert!(first.current_version() > 0);
    assert!(matches!(
        owner
            .begin_item(
                other,
                SourceId(id(5)),
                Arc::from("text"),
                Arc::from("acp"),
                Arc::from("1"),
                vec![1; 8 * 1024 * 1024]
            )
            .await,
        Err(HistoryError::Capacity)
    ));
    assert!(
        owner
            .status(other)
            .expect("shared session cap")
            .retained_bytes
            <= SESSION_CACHE_LIMIT
    );
}
#[tokio::test]
async fn background_watchdog_keeps_running_while_a_storage_job_is_blocked() {
    let (owner, repository, clock, output) = fixture();
    let scope = scope(1);
    owner.attach(scope).await.expect("attach");
    item(&owner, scope, 3, SAVE_BYTES).await;
    repository.0.fail.store(true, Ordering::Release);
    owner.tick().await.expect_err("first actual failure");
    let gate = Arc::new(Gate::new());
    *repository.0.block.lock().expect("block") = Some(Arc::clone(&gate));
    clock.set(250);
    let task = HistoryTask::start(owner.clone());
    tokio::time::timeout(Duration::from_secs(1), gate.entered.acquire())
        .await
        .expect("retry scheduled")
        .expect("entered")
        .forget();
    clock.set(5000);
    owner.core.changed.notify_waiters();
    tokio::time::timeout(Duration::from_secs(1), async {
        while output.0.requests.lock().expect("requests").is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("watchdog independent of storage job");
    assert!(owner.status(scope).expect("stopped while inflight").stopped);
    drop(task);
}

#[tokio::test]
async fn oversized_payload_is_rejected_without_retaining_it_and_new_stopped_sources_record_loss() {
    let (owner, _, _, _) = fixture();
    let scope = scope(1);
    owner.attach(scope).await.expect("attach");
    let result = owner
        .begin_item(
            scope,
            SourceId(id(3)),
            Arc::from("text"),
            Arc::from("acp"),
            Arc::from("1"),
            vec![0; SESSION_CACHE_LIMIT + 1],
        )
        .await;
    assert!(matches!(result, Err(HistoryError::Capacity)));
    let before = owner.status(scope).expect("bounded");
    assert_eq!(before.retained_bytes, SCOPE_OVERHEAD);
    assert_eq!(
        before
            .fault
            .expect("fault")
            .possible_loss
            .expect("range")
            .dropped_bytes,
        u64::try_from(SESSION_CACHE_LIMIT + 1).expect("bytes")
    );
    assert!(matches!(
        owner
            .begin_item(
                scope,
                SourceId(id(4)),
                Arc::from("text"),
                Arc::from("acp"),
                Arc::from("1"),
                vec![0; 9]
            )
            .await,
        Err(HistoryError::Stopped)
    ));
    let after = owner.status(scope).expect("recorded rejection");
    assert_eq!(
        after
            .fault
            .expect("fault")
            .possible_loss
            .expect("range")
            .dropped_bytes,
        u64::try_from(SESSION_CACHE_LIMIT + 10).expect("bytes")
    );
}

// Each case forces the first observation into the former idle-schedule/registration gap.
// The second barrier must be reached from that broadcast alone, before any clock wake is sent.
async fn first_observation_crosses_idle_schedule_window(
    payload_bytes: usize,
    expected_delay: Duration,
) {
    let (owner, repository, clock, _) = fixture();
    let scope = scope(1);
    owner.attach(scope).await.expect("attach empty scope");
    let first = Arc::new(WorkerScheduleBarrier::new());
    *owner.core.after_schedule.lock().expect("hook") = Some(Arc::clone(&first));
    let task = HistoryTask::start(owner.clone());
    bounded(first.reached.wait()).await;
    assert_eq!(
        *first.scheduled.lock().expect("initial schedule"),
        Some((Duration::from_secs(60), 0)),
        "worker must already hold a stale idle schedule"
    );
    let handle = item(&owner, scope, 3, payload_bytes).await;
    if payload_bytes < SAVE_BYTES {
        clock.set(499);
    }
    let second = Arc::new(WorkerScheduleBarrier::new());
    *owner.core.after_schedule.lock().expect("second hook") = Some(Arc::clone(&second));
    bounded(first.resume.wait()).await;
    // No sleep, additional output, manual tick or notification can rescue a lost first wake.
    bounded(second.reached.wait()).await;
    assert_eq!(
        *second.scheduled.lock().expect("rescheduled deadline"),
        Some((expected_delay, usize::from(payload_bytes >= SAVE_BYTES)))
    );
    assert_eq!(repository.0.calls.load(Ordering::Acquire), 0);
    assert_eq!(handle.committed_version(), 0);
    if payload_bytes < SAVE_BYTES {
        // The first wake was already proved at 499ms. Advance the controlled clock to its 500ms boundary.
        clock.set(500);
        owner.core.changed.notify_waiters();
    }
    bounded(second.resume.wait()).await;
    bounded(async {
        while handle.committed_version() != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert_eq!(repository.0.calls.load(Ordering::Acquire), 1);
    assert_eq!(
        clock.millis(),
        if payload_bytes < SAVE_BYTES { 500 } else { 0 }
    );
    bounded(task.shutdown()).await.expect("owned task shutdown");
}
#[tokio::test]
async fn first_small_payload_in_idle_registration_window_keeps_its_500ms_deadline() {
    first_observation_crosses_idle_schedule_window(1, Duration::from_millis(1)).await;
}
#[tokio::test]
async fn first_65536_bytes_in_idle_registration_window_schedule_immediately() {
    first_observation_crosses_idle_schedule_window(65536, Duration::ZERO).await;
}

#[tokio::test]
async fn hist_r001_broadcast_completion_wakes_all_registered_forced_flush_waiters() {
    let (owner, repository, _, _) = fixture();
    let scope = scope(1);
    owner.attach(scope).await.expect("attach");
    let handle = item(&owner, scope, 3, 1).await;
    let gate = Arc::new(Gate::new());
    *repository.0.block.lock().expect("gate") = Some(Arc::clone(&gate));
    let first = {
        let owner = owner.clone();
        tokio::spawn(async move { owner.flush(scope, FlushReason::ItemEnd).await })
    };
    bounded(gate.entered.acquire())
        .await
        .expect("entered")
        .forget();
    owner
        .update_item(&handle, vec![2; 2])
        .await
        .expect("new version");
    let mut waiters: Vec<_> = (0..3)
        .map(|_| Box::pin(owner.flush(scope, FlushReason::Release)))
        .collect();
    // Poll each once to guarantee all three broadcast waiters are registered before completion.
    for waiter in &mut waiters {
        std::future::poll_fn(|context| {
            assert!(waiter.as_mut().poll(context).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
    }
    gate.release.add_permits(1);
    bounded(first)
        .await
        .expect("owned first flight")
        .expect("first commit");
    for waiter in waiters {
        bounded(waiter)
            .await
            .expect("every forced waiter completes");
    }
    assert_eq!(handle.committed_version(), 2);
    assert_eq!(repository.0.calls.load(Ordering::Acquire), 2);
    assert_eq!(repository.0.peak.load(Ordering::Acquire), 1);
}
