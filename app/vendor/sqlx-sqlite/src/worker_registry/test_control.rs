//! Private per-scope controls. No product feature or external state setter.
use super::*;
use std::{
    collections::BTreeMap,
    sync::{Condvar, Weak},
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum Point {
    AfterReserveBeforeSpawn,
    AfterSpawnBeforeInstall,
    WorkerBeforeEstablish,
    EstablishReplyBeforeSend,
    EstablishReplyAfterEnqueue,
    BeforeExecute,
    BeforeUnlockDb,
    BeforeBeginReply,
    BeforePing,
    BeforeShutdownAck,
    AfterShutdownAckBeforeThreadReturn,
    JoinResultBeforeWriteback,
    SnapshotStateRead,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Visit {
    Continue,
    SpawnIoError,
    Unavailable,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Action {
    Pause,
    SpawnIoErrorOnce,
    PanicAtRustPoint,
    StateUnavailableOnce,
    AttemptSelfJoin,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Selection {
    pub worker: usize,
    pub point: Point,
    pub hit: usize,
}
#[derive(Clone, Debug)]
pub(crate) struct TestPlan {
    pub rules: Vec<(Selection, Action)>,
}
impl Default for TestPlan {
    fn default() -> Self {
        Self { rules: Vec::new() }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Event {
    pub selection: Selection,
    pub self_join: Option<Result<WorkerJoinReport, WorkerRegistryError>>,
}
#[derive(Default)]
pub(crate) struct Controls {
    state: Mutex<Option<Arc<Control>>>,
}
struct Control {
    registry: Weak<Shared>,
    state: Mutex<ControlState>,
    changed: Condvar,
}
struct ControlState {
    rules: Vec<Rule>,
    hits: BTreeMap<(usize, Point), usize>,
    events: Vec<Event>,
}
struct Rule {
    selection: Selection,
    action: Action,
    released: bool,
    reached: bool,
}
#[derive(Clone)]
pub(crate) struct TestController {
    control: Arc<Control>,
}
impl Controls {
    fn get(&self) -> Option<Arc<Control>> {
        match self.state.lock() {
            Ok(state) => state.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
    pub(super) fn visit(&self, id: &WorkerId, point: Point) -> Visit {
        let Some(control) = self.get() else {
            return Visit::Continue;
        };
        let mut state = control.lock();
        let counter = state.hits.entry((id.ordinal, point)).or_default();
        *counter += 1;
        let selection = Selection {
            worker: id.ordinal,
            point,
            hit: *counter,
        };
        state.events.push(Event {
            selection,
            self_join: None,
        });
        let action = state
            .rules
            .iter_mut()
            .find(|rule| rule.selection == selection)
            .map(|rule| {
                rule.reached = true;
                rule.action
            });
        control.changed.notify_all();
        if action == Some(Action::Pause) {
            while state
                .rules
                .iter()
                .any(|rule| rule.selection == selection && !rule.released)
            {
                state = match control.changed.wait(state) {
                    Ok(state) => state,
                    Err(poisoned) => poisoned.into_inner(),
                };
            }
        }
        drop(state);
        match action {
            Some(Action::SpawnIoErrorOnce) if point == Point::AfterReserveBeforeSpawn => {
                Visit::SpawnIoError
            }
            Some(Action::StateUnavailableOnce)
                if matches!(
                    point,
                    Point::SnapshotStateRead | Point::JoinResultBeforeWriteback
                ) =>
            {
                Visit::Unavailable
            }
            Some(Action::PanicAtRustPoint)
                if matches!(
                    point,
                    Point::WorkerBeforeEstablish
                        | Point::BeforeExecute
                        | Point::BeforeUnlockDb
                        | Point::BeforeBeginReply
                        | Point::BeforePing
                ) =>
            {
                panic!("CLDB_TEST_WORKER_PANIC")
            }
            Some(Action::AttemptSelfJoin) => {
                if let Some(registry) = control.registry.upgrade() {
                    registry.observers.fetch_add(1, Ordering::Relaxed);
                    let registry = SqliteWorkerRegistry { shared: registry };
                    let result = registry.join_worker_blocking(id.clone());
                    let mut state = control.lock();
                    state.events.push(Event {
                        selection,
                        self_join: Some(result),
                    });
                    control.changed.notify_all();
                }
                Visit::Continue
            }
            _ => Visit::Continue,
        }
    }
    pub(super) fn snapshot_unavailable(&self) -> bool {
        let Some(control) = self.get() else {
            return false;
        };
        let Some(shared) = control.registry.upgrade() else {
            return false;
        };
        let id = WorkerId {
            scope: shared.scope.clone(),
            ordinal: 0,
        };
        self.visit(&id, Point::SnapshotStateRead) == Visit::Unavailable
    }
}
impl Control {
    fn lock(&self) -> MutexGuard<'_, ControlState> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}
impl TestController {
    pub(crate) fn install(registry: &SqliteWorkerRegistry, plan: TestPlan) -> Self {
        let control = Arc::new(Control {
            registry: Arc::downgrade(&registry.shared),
            state: Mutex::new(ControlState {
                rules: plan
                    .rules
                    .into_iter()
                    .map(|(selection, action)| Rule {
                        selection,
                        action,
                        released: false,
                        reached: false,
                    })
                    .collect(),
                hits: BTreeMap::new(),
                events: Vec::new(),
            }),
            changed: Condvar::new(),
        });
        let mut slot = match registry.shared.control.state.lock() {
            Ok(slot) => slot,
            Err(poisoned) => poisoned.into_inner(),
        };
        *slot = Some(Arc::clone(&control));
        Self { control }
    }
    pub(crate) fn events(&self) -> Vec<Event> {
        self.control.lock().events.clone()
    }
    pub(crate) fn wait_reached(&self, selection: Selection, budget: Duration) -> bool {
        let until = Instant::now() + budget;
        let mut state = self.control.lock();
        loop {
            if state
                .rules
                .iter()
                .any(|rule| rule.selection == selection && rule.reached)
            {
                return true;
            }
            let Some(left) = until.checked_duration_since(Instant::now()) else {
                return false;
            };
            state = match self.control.changed.wait_timeout(state, left) {
                Ok((state, _)) => state,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
    }
    pub(crate) fn release(&self, selection: Selection) {
        let mut state = self.control.lock();
        for rule in &mut state.rules {
            if rule.selection == selection {
                rule.released = true;
            }
        }
        self.control.changed.notify_all();
    }
    /// Emergency recovery of the same scope for a test's cleanup only. It cannot
    /// clear Unknown or turn retention into an automatic cleanup mechanism.
    pub(crate) fn recover_original_scope(&self) -> Option<SqliteWorkerRegistry> {
        let shared = self.control.registry.upgrade()?;
        shared.observers.fetch_add(1, Ordering::Relaxed);
        Some(SqliteWorkerRegistry { shared })
    }
    pub(crate) fn public_observers(&self) -> usize {
        self.control
            .registry
            .upgrade()
            .map_or(0, |shared| shared.observers.load(Ordering::Acquire))
    }
    pub(crate) fn pin_present(&self) -> bool {
        self.control
            .registry
            .upgrade()
            .is_some_and(|shared| match shared.state.lock() {
                Ok(state) => state.retention_pin.is_some(),
                Err(poisoned) => poisoned.into_inner().retention_pin.is_some(),
            })
    }
}
