//! Explicit ownership of the original SQLite threads. Observers never own joins.
use futures_channel::oneshot;
use std::{
    fmt,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, MutexGuard,
    },
    thread::{self, JoinHandle},
};
mod facts;
pub(crate) use facts::Fact;
pub use facts::*;
#[cfg(test)]
mod development_tests;
#[cfg(test)]
pub(crate) mod test_control;

pub struct SqliteWorkerRegistry {
    shared: Arc<Shared>,
}
pub struct SqliteWorkerRegistration {
    shared: Arc<Shared>,
    id: WorkerId,
    public_observer: bool,
}
struct Shared {
    scope: WorkerScopeId,
    observers: AtomicUsize,
    state: Mutex<State>,
    #[cfg(test)]
    control: test_control::Controls,
}
struct State {
    generation: u64,
    sealed: bool,
    finalized: bool,
    unknown: bool,
    workers: Vec<Slot>,
    listeners: Vec<oneshot::Sender<()>>,
    retention_pin: Option<Arc<Shared>>,
    /// Defensive retention on a violated install invariant; never reported complete.
    unexpected_handles: Vec<JoinHandle<()>>,
}
struct Slot {
    snapshot: WorkerSnapshot,
    actual: Option<JoinHandle<()>>,
}
impl Clone for SqliteWorkerRegistry {
    fn clone(&self) -> Self {
        self.shared.observers.fetch_add(1, Ordering::Relaxed);
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}
impl Clone for SqliteWorkerRegistration {
    fn clone(&self) -> Self {
        if self.public_observer {
            self.shared.observers.fetch_add(1, Ordering::Relaxed);
        }
        Self {
            shared: Arc::clone(&self.shared),
            id: self.id.clone(),
            public_observer: self.public_observer,
        }
    }
}
fn release_observer(shared: &Arc<Shared>) {
    if shared.observers.fetch_sub(1, Ordering::AcqRel) != 1 {
        return;
    }
    // No spawn, join or implicit successful finalization in Drop.
    let mut state = match shared.state.lock() {
        Ok(state) => state,
        Err(poisoned) => {
            let mut state = poisoned.into_inner();
            state.unknown = true;
            state
        }
    };
    state.sealed = true;
    if state.workers.iter().any(|slot| !slot.terminal()) {
        state.unknown = true;
    }
    state.publish();
}
impl Drop for SqliteWorkerRegistry {
    fn drop(&mut self) {
        release_observer(&self.shared);
    }
}
impl Drop for SqliteWorkerRegistration {
    fn drop(&mut self) {
        if self.public_observer {
            release_observer(&self.shared);
        }
    }
}
impl fmt::Debug for SqliteWorkerRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SqliteWorkerRegistry")
            .field("scope", &self.shared.scope)
            .finish_non_exhaustive()
    }
}
impl fmt::Debug for SqliteWorkerRegistration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SqliteWorkerRegistration")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}
impl Default for SqliteWorkerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
impl SqliteWorkerRegistry {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared {
                scope: WorkerScopeId(Arc::new(())),
                observers: AtomicUsize::new(1),
                state: Mutex::new(State {
                    generation: 0,
                    sealed: false,
                    finalized: false,
                    unknown: false,
                    workers: Vec::new(),
                    listeners: Vec::new(),
                    retention_pin: None,
                    unexpected_handles: Vec::new(),
                }),
                #[cfg(test)]
                control: test_control::Controls::default(),
            }),
        }
    }
    pub fn snapshot(&self) -> Result<RegistrySnapshot, WorkerRegistryError> {
        self.shared.snapshot()
    }
    pub fn seal(&self) -> Result<RegistrySnapshot, WorkerRegistryError> {
        let mut state = self.shared.lock()?;
        if !state.sealed {
            state.sealed = true;
            state.publish();
        }
        Ok(state.snapshot(&self.shared.scope))
    }
    /// Subscribe under the same lock as the generation read. After subscribing,
    /// reread snapshot and compare generations before awaiting this one-shot.
    pub fn subscribe(&self) -> Result<WorkerChangeSubscription, WorkerRegistryError> {
        let mut state = self.shared.lock()?;
        let (sender, receiver) = oneshot::channel();
        state.listeners.retain(|listener| !listener.is_canceled());
        state.listeners.push(sender);
        Ok(WorkerChangeSubscription {
            generation: state.generation,
            receiver,
        })
    }
    pub fn finalize(&self) -> Result<RegistrySnapshot, WorkerRegistryError> {
        let mut state = self.shared.lock()?;
        if !state.sealed {
            return Err(WorkerRegistryError::NotSealed);
        }
        if state.unknown {
            return Err(WorkerRegistryError::StateUnavailable);
        }
        if !state.unexpected_handles.is_empty() || state.workers.iter().any(|slot| !slot.terminal())
        {
            return Err(WorkerRegistryError::Incomplete);
        }
        if !state.finalized {
            state.finalized = true;
            state.retention_pin = None;
            state.publish();
        }
        Ok(state.snapshot(&self.shared.scope))
    }
    /// Call only from an externally owned blocking operation. It does not own or
    /// supervise that operation's runtime, roots or Tokio JoinHandle.
    pub fn join_worker_blocking(
        &self,
        id: WorkerId,
    ) -> Result<WorkerJoinReport, WorkerRegistryError> {
        if id.scope != self.shared.scope {
            return Err(WorkerRegistryError::WrongScope);
        }
        let actual = {
            let mut state = self.shared.lock()?;
            let slot = state.slot_mut(&id)?;
            if let Some(report) = slot.snapshot.join_report {
                return Ok(report);
            }
            if slot.snapshot.handle == WorkerHandleState::Joining {
                return Err(WorkerRegistryError::AlreadyJoining);
            }
            let Some(handle) = slot.actual.as_ref() else {
                return Err(WorkerRegistryError::SpawnPending);
            };
            if handle.thread().id() == thread::current().id() {
                return Err(WorkerRegistryError::WouldJoinSelf);
            }
            slot.snapshot.handle = WorkerHandleState::Joining;
            // Notify while the handle is still in its stable slot. Even a
            // panicking observer waker cannot detach the original handle.
            state.publish();
            let Some(handle) = state.slot_mut(&id)?.actual.take() else {
                return Err(WorkerRegistryError::InvariantViolation);
            };
            handle
        };
        // No callback/await between claiming the original handle and actual join.
        let joined = actual.join();
        let report = if joined.is_ok() {
            WorkerJoinReport::Ok
        } else {
            WorkerJoinReport::WorkerPanicked
        };
        #[cfg(test)]
        if self
            .shared
            .control
            .visit(&id, test_control::Point::JoinResultBeforeWriteback)
            == test_control::Visit::Unavailable
        {
            self.shared.unconfirmed_join(&id, report);
            return Err(WorkerRegistryError::StateUnavailable);
        }
        let mut state = match self.shared.state.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                state.unknown = true;
                if let Ok(slot) = state.slot_mut(&id) {
                    slot.snapshot.unknown = true;
                    slot.snapshot.unconfirmed_join_report = Some(report);
                }
                state.publish();
                return Err(WorkerRegistryError::StateUnavailable);
            }
        };
        let slot = state.slot_mut(&id)?;
        slot.snapshot.handle = WorkerHandleState::Joined;
        slot.snapshot.join_report = Some(report);
        state.publish();
        Ok(report)
    }
    pub(crate) fn reserve(&self) -> Result<SqliteWorkerRegistration, WorkerRegistryError> {
        let mut state = self.shared.lock()?;
        if state.sealed {
            return Err(WorkerRegistryError::Sealed);
        }
        let ordinal = state
            .workers
            .len()
            .checked_add(1)
            .ok_or(WorkerRegistryError::StateUnavailable)?;
        let id = WorkerId {
            scope: self.shared.scope.clone(),
            ordinal,
        };
        if state.retention_pin.is_none() {
            state.retention_pin = Some(Arc::clone(&self.shared));
        }
        let generation = state
            .generation
            .checked_add(1)
            .ok_or(WorkerRegistryError::StateUnavailable)?;
        state.workers.push(Slot {
            snapshot: WorkerSnapshot {
                id: id.clone(),
                reservation_generation: generation,
                spawn: WorkerSpawn::Reserved,
                spawn_error_kind: None,
                handle: WorkerHandleState::NotInstalled,
                join_report: None,
                unconfirmed_join_report: None,
                exit_path: WorkerExitPath::Unknown,
                shutdown: WorkerShutdownFacts::default(),
                close: WorkerCloseFacts::default(),
                unknown: false,
                thread_is_finished: false,
            },
            actual: None,
        });
        state.publish();
        Ok(SqliteWorkerRegistration {
            shared: Arc::clone(&self.shared),
            id,
            public_observer: false,
        })
    }
}
pub struct WorkerChangeSubscription {
    pub generation: u64,
    receiver: oneshot::Receiver<()>,
}
impl WorkerChangeSubscription {
    pub async fn changed(self) -> Result<(), WorkerRegistryError> {
        self.receiver
            .await
            .map_err(|_| WorkerRegistryError::StateUnavailable)
    }
}
impl Shared {
    fn lock(&self) -> Result<MutexGuard<'_, State>, WorkerRegistryError> {
        self.state
            .lock()
            .map_err(|_| WorkerRegistryError::StateUnavailable)
    }
    fn snapshot(&self) -> Result<RegistrySnapshot, WorkerRegistryError> {
        #[cfg(test)]
        if self.control.snapshot_unavailable() {
            let mut state = match self.state.lock() {
                Ok(state) => state,
                Err(poisoned) => poisoned.into_inner(),
            };
            state.unknown = true;
            state.publish();
            return Err(WorkerRegistryError::StateUnavailable);
        }
        Ok(self.lock()?.snapshot(&self.scope))
    }
    #[cfg(test)]
    fn unconfirmed_join(&self, id: &WorkerId, report: WorkerJoinReport) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.unknown = true;
        if let Ok(slot) = state.slot_mut(id) {
            slot.snapshot.unknown = true;
            slot.snapshot.unconfirmed_join_report = Some(report);
        }
        state.publish();
    }
}
impl State {
    fn slot_mut(&mut self, id: &WorkerId) -> Result<&mut Slot, WorkerRegistryError> {
        id.ordinal
            .checked_sub(1)
            .and_then(|index| self.workers.get_mut(index))
            .filter(|slot| slot.snapshot.id == *id)
            .ok_or(WorkerRegistryError::UnknownWorker)
    }
    fn publish(&mut self) {
        if let Some(next) = self.generation.checked_add(1) {
            self.generation = next;
        } else {
            self.unknown = true;
        }
        for listener in self.listeners.drain(..) {
            let _ = listener.send(());
        }
    }
    fn snapshot(&self, scope: &WorkerScopeId) -> RegistrySnapshot {
        RegistrySnapshot {
            scope: scope.clone(),
            generation: self.generation,
            sealed: self.sealed,
            finalized: self.finalized,
            unknown: self.unknown,
            workers: self
                .workers
                .iter()
                .map(|slot| {
                    let mut snapshot = slot.snapshot.clone();
                    snapshot.thread_is_finished =
                        slot.actual.as_ref().is_some_and(JoinHandle::is_finished);
                    snapshot
                })
                .collect(),
        }
    }
}
impl Slot {
    fn terminal(&self) -> bool {
        !self.snapshot.unknown
            && (self.snapshot.spawn == WorkerSpawn::SpawnFailedNoThread
                || (self.snapshot.handle == WorkerHandleState::Joined
                    && self.snapshot.join_report.is_some()))
    }
}
impl SqliteWorkerRegistration {
    pub fn id(&self) -> WorkerId {
        self.id.clone()
    }
    pub fn snapshot(&self) -> Result<WorkerSnapshot, WorkerRegistryError> {
        self.shared
            .snapshot()?
            .workers
            .into_iter()
            .find(|worker| worker.id == self.id)
            .ok_or(WorkerRegistryError::UnknownWorker)
    }
    pub(crate) fn observer(&self) -> Self {
        self.shared.observers.fetch_add(1, Ordering::Relaxed);
        Self {
            shared: Arc::clone(&self.shared),
            id: self.id.clone(),
            public_observer: true,
        }
    }
    pub(crate) fn install(&self, actual: JoinHandle<()>) -> Result<(), WorkerRegistryError> {
        // Retain the actual handle even when mutex poisoning makes success unknown.
        let (mut state, poisoned) = match self.shared.state.lock() {
            Ok(state) => (state, false),
            Err(poisoned) => (poisoned.into_inner(), true),
        };
        if poisoned {
            state.unknown = true;
        }
        match state.slot_mut(&self.id) {
            Ok(slot) if slot.actual.is_none() && slot.snapshot.spawn == WorkerSpawn::Reserved => {
                slot.actual = Some(actual);
                slot.snapshot.spawn = WorkerSpawn::Installed;
                slot.snapshot.handle = WorkerHandleState::VendorOwned;
                if poisoned {
                    slot.snapshot.unknown = true;
                }
            }
            _ => {
                state.unknown = true;
                state.unexpected_handles.push(actual);
                state.publish();
                return Err(WorkerRegistryError::InvariantViolation);
            }
        }
        state.publish();
        if poisoned {
            Err(WorkerRegistryError::StateUnavailable)
        } else {
            Ok(())
        }
    }
    pub(crate) fn spawn_failed(&self, error: &std::io::Error) {
        let mut state = match self.shared.state.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                state.unknown = true;
                state
            }
        };
        if let Ok(slot) = state.slot_mut(&self.id) {
            if slot.snapshot.spawn == WorkerSpawn::Reserved {
                slot.snapshot.spawn = WorkerSpawn::SpawnFailedNoThread;
                slot.snapshot.spawn_error_kind = Some(error.kind());
            } else {
                slot.snapshot.unknown = true;
                state.unknown = true;
            }
        } else {
            state.unknown = true;
        }
        state.publish();
    }
    pub(crate) fn record(&self, fact: Fact) {
        let mut state = match self.shared.state.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                state.unknown = true;
                state
            }
        };
        if let Ok(slot) = state.slot_mut(&self.id) {
            let facts = &mut slot.snapshot;
            match fact {
                Fact::Exit(path) => {
                    if facts.exit_path == WorkerExitPath::Unknown {
                        facts.exit_path = path;
                    }
                }
                Fact::ShutdownRequested => facts.shutdown.requested = true,
                Fact::ShutdownSendFailed => facts.shutdown.command_send_failed = true,
                Fact::AckAttempted => facts.shutdown.ack_send_attempted = true,
                Fact::AckEnqueued => facts.shutdown.ack_enqueued = true,
                Fact::AckReceiverGone => facts.shutdown.ack_receiver_gone = true,
                Fact::AckObserved => facts.shutdown.ack_observed = true,
                Fact::AckReceiveFailed => facts.shutdown.ack_receive_failed = true,
                Fact::GracefulStarted => facts.close.graceful_started = true,
                Fact::OptimizeFailed => facts.close.optimize_failed = true,
                Fact::ShutdownError => facts.close.shutdown_error = true,
                Fact::GracefulReturnedOk => facts.close.graceful_returned_ok = true,
                Fact::HardDropRequested => facts.close.hard_drop_requested = true,
            }
        } else {
            state.unknown = true;
        }
        state.publish();
    }
    #[cfg(test)]
    pub(crate) fn visit(&self, point: test_control::Point) -> test_control::Visit {
        self.shared.control.visit(&self.id, point)
    }
}
/// Keeps a just-spawned handle owned during the synchronous install interval,
/// including an unwind at a private test barrier. Drop never joins or detaches it.
pub(crate) struct PendingWorkerInstall {
    registration: SqliteWorkerRegistration,
    actual: Option<JoinHandle<()>>,
}
impl PendingWorkerInstall {
    pub(crate) fn new(registration: SqliteWorkerRegistration, actual: JoinHandle<()>) -> Self {
        Self {
            registration,
            actual: Some(actual),
        }
    }
    pub(crate) fn install(mut self) -> Result<(), WorkerRegistryError> {
        let Some(actual) = self.actual.take() else {
            return Err(WorkerRegistryError::InvariantViolation);
        };
        self.registration.install(actual)
    }
}
impl Drop for PendingWorkerInstall {
    fn drop(&mut self) {
        if let Some(actual) = self.actual.take() {
            let _ = self.registration.install(actual);
        }
    }
}
pub(crate) struct WorkerExitGuard(pub(crate) Option<SqliteWorkerRegistration>);
impl Drop for WorkerExitGuard {
    fn drop(&mut self) {
        if let Some(registration) = &self.0 {
            registration.record(Fact::Exit(if thread::panicking() {
                WorkerExitPath::WorkerUnwinding
            } else {
                WorkerExitPath::CommandChannelClosed
            }));
        }
    }
}
