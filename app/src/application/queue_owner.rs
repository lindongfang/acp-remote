//! One node-owned queue per native identity. SQL futures never hold a queue coordination lock.
use super::request_registry::{
    AuditWarning, ConsumeLease, CurrentRequestActor, OperationContext, OperationProbe,
    QueueOutcome, RegistrationId, RegistryDeadline, RegistryError, RequestFact, random_id,
};
pub use super::request_registry::{ValidatedBody, ValidatedMutation};
pub(crate) use crate::domain::input_queue::PriorityFailure;
use crate::domain::{
    EntityId,
    input_queue::{DispatchReadiness, DispatchToken, InputQueue, QueueDisposition, QueueError},
    native::{NativeSessionKey, ResourceEpoch, RuntimeEpoch},
};
use std::{
    collections::HashMap,
    fmt,
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
};
use tokio::{
    sync::{Notify, oneshot},
    task::JoinHandle,
};

#[cfg(windows)]
mod document_read;
mod native;
#[cfg(windows)]
pub(crate) use document_read::{QueueDocumentError, QueuedBodySnapshot};
mod read_target;
mod service;
#[cfg(all(test, windows))]
pub(crate) mod validation_test_hooks;
pub use native::{AcceptedTargetProof, DirectCapture, PreparedCapture, RetainPrompt};
pub(crate) use native::{NativeQueueLink, NativeQueuePolicy};
pub(crate) use native::{SelectedContext, SelectedFence};
pub use read_target::ReadTargetProof;
pub use service::QueueService;
#[cfg(test)]
pub(crate) use service::read_commands::ReadTaskSettlement;
pub(crate) use service::read_commands::{QueueReadFailure, QueueReadReceipt};

/// Read-only observation of the original pending installation, never an admission grant.
pub(crate) struct ConsumptionCustody {
    transferred: AtomicBool,
}
impl ConsumptionCustody {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            transferred: AtomicBool::new(false),
        })
    }
    pub(crate) fn transferred(&self) -> bool {
        self.transferred.load(Ordering::Acquire)
    }
}

pub enum ConsumeReply<T> {
    Resolved {
        fact: RequestFact,
        returned: Option<ValidatedMutation<T>>,
    },
    Pending {
        request: RegistrationId,
    },
    Refused {
        error: RegistryError,
        returned: ValidatedMutation<T>,
    },
}
impl<T> fmt::Debug for ConsumeReply<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resolved { fact, returned } => f
                .debug_struct("Resolved")
                .field("fact", fact)
                .field("returned", &returned.is_some())
                .finish(),
            Self::Pending { request } => f.debug_tuple("Pending").field(request).finish(),
            Self::Refused { error, .. } => f
                .debug_tuple("Refused")
                .field(error)
                .finish_non_exhaustive(),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessingPhase {
    AwaitBegin,
    BeginUnconfirmed,
    ReadyToApply,
    AwaitSettle,
    SettleUnconfirmed,
    Faulted,
}
pub enum QueueRead<'a, T> {
    Stable {
        queue: &'a InputQueue<ValidatedBody<T>>,
    },
    Processing {
        phase: ProcessingPhase,
        applied: bool,
        request: RegistrationId,
        queue: &'a InputQueue<ValidatedBody<T>>,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum StartupStatus {
    Starting = 0,
    Running = 1,
    Unconfirmed = 2,
    Stopping = 3,
    Stopped = 4,
}
#[derive(Debug)]
pub struct ShutdownReport {
    pub joined: bool,
    pub unresolved: Vec<RegistrationId>,
    pub audit_cancelled: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum DriverError {
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Queue(#[from] QueueError),
    #[error("external work sequence is exhausted")]
    SequenceExhausted,
}
pub trait QueueEnvironmentPort: Send + Sync {
    fn readiness(&self, target: &NativeSessionKey) -> DispatchReadiness;
    fn resource_epoch(&self, target: &NativeSessionKey) -> Option<ResourceEpoch>;
    fn poll_event(&self, target: &NativeSessionKey) -> Option<QueueOwnerEvent>;
    fn queue_changed(&self, _target: &NativeSessionKey) {}
    fn priority_accepted(&self, _target: &NativeSessionKey, _intent: EntityId) {}
    fn priority_failed(&self, _target: &NativeSessionKey, _intent: EntityId) {}
}
/// Production default until real protocol/dispatch qualifications have been independently verified.
#[derive(Default)]
pub struct BlockedEnvironment;
impl QueueEnvironmentPort for BlockedEnvironment {
    fn readiness(&self, _: &NativeSessionKey) -> DispatchReadiness {
        DispatchReadiness::Blocked
    }
    fn resource_epoch(&self, _: &NativeSessionKey) -> Option<ResourceEpoch> {
        None
    }
    fn poll_event(&self, _: &NativeSessionKey) -> Option<QueueOwnerEvent> {
        None
    }
}
struct OwnerIdentity {
    id: EntityId,
    runtime: RuntimeEpoch,
}
#[derive(Clone)]
pub struct ActiveKey {
    identity: Arc<OwnerIdentity>,
    item: EntityId,
    revision: u64,
    resource: ResourceEpoch,
    selection_order: u64,
}
impl ActiveKey {
    #[must_use]
    pub fn runtime(&self) -> RuntimeEpoch {
        self.identity.runtime
    }
    #[must_use]
    pub fn owner(&self) -> EntityId {
        self.identity.id
    }
    #[must_use]
    pub const fn item(&self) -> EntityId {
        self.item
    }
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    #[must_use]
    pub const fn resource(&self) -> ResourceEpoch {
        self.resource
    }
    #[must_use]
    pub const fn selection_order(&self) -> u64 {
        self.selection_order
    }
    fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.identity, &other.identity)
            && self.item == other.item
            && self.revision == other.revision
            && self.resource == other.resource
    }
}
impl fmt::Debug for ActiveKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ActiveKey")
            .field("owner", &self.owner())
            .field("item", &self.item)
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}
pub struct OwnedPreparation {
    identity: Arc<OwnerIdentity>,
    target: EntityId,
    token: DispatchToken,
    resource: ResourceEpoch,
}
impl fmt::Debug for OwnedPreparation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OwnedPreparation")
            .field("target", &self.target)
            .finish_non_exhaustive()
    }
}
#[derive(Clone)]
pub struct ExternalWorkKey {
    identity: Arc<OwnerIdentity>,
    resource: ResourceEpoch,
    serial: u64,
}
impl ExternalWorkKey {
    #[must_use]
    pub fn runtime(&self) -> RuntimeEpoch {
        self.identity.runtime
    }
    #[must_use]
    pub fn owner(&self) -> EntityId {
        self.identity.id
    }
    #[must_use]
    pub const fn resource(&self) -> ResourceEpoch {
        self.resource
    }
    #[must_use]
    pub const fn serial(&self) -> u64 {
        self.serial
    }
    fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.identity, &other.identity)
            && self.resource == other.resource
            && self.serial == other.serial
    }
}
impl fmt::Debug for ExternalWorkKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExternalWorkKey")
            .field("owner", &self.owner())
            .field("serial", &self.serial)
            .finish_non_exhaustive()
    }
}
pub enum QueueOwnerEvent {
    ActivityChanged,
    Settle {
        active: ActiveKey,
        disposition: QueueDisposition,
    },
    ExternalForegroundFailure {
        work: ExternalWorkKey,
    },
    ExternalForegroundCompleted {
        work: ExternalWorkKey,
    },
}

pub(crate) trait TargetResolver: Send + Sync {
    fn resolve(
        &self,
        target: EntityId,
    ) -> Pin<Box<dyn Future<Output = Result<NativeSessionKey, RegistryError>> + Send + '_>>;
}
struct Pending<T> {
    probe: Arc<OperationProbe>,
    actor: Arc<CurrentRequestActor>,
    context: OperationContext,
    phase: ProcessingPhase,
    applied: bool,
    worker: bool,
    worker_ticket: Arc<()>,
    mutation: Option<ValidatedMutation<T>>,
    returned: Option<ValidatedMutation<T>>,
    retired: Vec<ValidatedBody<T>>,
    outcome: Option<QueueOutcome>,
    lease: Option<Arc<ConsumeLease>>,
    response: Option<oneshot::Sender<ConsumeReply<T>>>,
}
struct OwnerState<T> {
    #[cfg(windows)]
    document_epochs: HashMap<
        (EntityId, u64),
        std::sync::Weak<crate::application::document_source::DocumentEpoch>,
    >,
    #[cfg(windows)]
    document_wakes: Vec<crate::application::document_source::DocumentWake>,
    queue: InputQueue<ValidatedBody<T>>,
    pending: Option<Pending<T>>,
    active: Option<(ActiveKey, DispatchToken)>,
    external: Option<ExternalWorkKey>,
    external_serial: u64,
    native_order: u64,
    canonical_target: Option<EntityId>,
    origins: HashMap<EntityId, crate::domain::native_dispatch::ContentOrigin>,
    priority_handoff: Option<PriorityHandoff>,
    selected_priority: Option<(EntityId, EntityId, u64)>,
}
impl<T> OwnerState<T> {
    fn fail_selected_priority(&mut self, failure: PriorityFailure) -> Option<EntityId> {
        let (operation, _, _) = self.selected_priority?;
        if self.queue.priority_failure().is_none() {
            self.queue.fail_priority(failure);
        }
        self.selected_priority = None;
        self.priority_handoff = None;
        Some(operation)
    }
}
struct OwnerCell<T> {
    identity: Arc<OwnerIdentity>,
    native: NativeSessionKey,
    state: Mutex<OwnerState<T>>,
    changed: Notify,
}
struct AuditTask {
    request: RegistrationId,
    cancelled: AtomicBool,
}
struct TaskRecord {
    handle: tokio::sync::Mutex<Option<JoinHandle<()>>>,
    abort: tokio::task::AbortHandle,
    audit: Option<Arc<AuditTask>>,
    startup: bool,
    failed: AtomicBool,
    failure_recorded: AtomicBool,
    read_completion: Option<Arc<service::read_commands::ReadTaskCompletion>>,
}
impl TaskRecord {
    fn new(handle: JoinHandle<()>, audit: Option<Arc<AuditTask>>, startup: bool) -> Arc<Self> {
        let abort = handle.abort_handle();
        Arc::new(Self {
            handle: tokio::sync::Mutex::new(Some(handle)),
            abort,
            audit,
            startup,
            failed: AtomicBool::new(false),
            failure_recorded: AtomicBool::new(false),
            read_completion: None,
        })
    }
    fn new_read(
        handle: JoinHandle<()>,
        completion: Arc<service::read_commands::ReadTaskCompletion>,
    ) -> Arc<Self> {
        let abort = handle.abort_handle();
        Arc::new(Self {
            handle: tokio::sync::Mutex::new(Some(handle)),
            abort,
            audit: None,
            startup: false,
            failed: AtomicBool::new(false),
            failure_recorded: AtomicBool::new(false),
            read_completion: Some(completion),
        })
    }
    fn record_join(&self, result: &Result<(), tokio::task::JoinError>) {
        if result.as_ref().is_err_and(tokio::task::JoinError::is_panic) {
            self.failed.store(true, Ordering::SeqCst);
        }
        if let Some(completion) = &self.read_completion {
            completion.complete(result);
        }
    }
    async fn join(&self, deadline: &RegistryDeadline) -> Result<(), RegistryError> {
        let mut slot = deadline.run(async { Ok(self.handle.lock().await) }).await?;
        if let Some(handle) = slot.as_mut() {
            let result = deadline.run(async { Ok(handle.await) }).await?;
            self.record_join(&result);
        }
        slot.take();
        drop(slot);
        Ok(())
    }
    fn reap_finished(&self) -> bool {
        use futures_util::FutureExt;
        let Ok(mut slot) = self.handle.try_lock() else {
            return false;
        };
        let Some(handle) = slot.as_mut() else {
            return true;
        };
        if !handle.is_finished() {
            return false;
        }
        let Some(result) = handle.now_or_never() else {
            return false;
        };
        self.record_join(&result);
        slot.take();
        true
    }
}
struct PendingWorker<T> {
    owner: Arc<OwnerCell<T>>,
    operation: EntityId,
    ticket: Arc<()>,
}
impl<T> Drop for PendingWorker<T> {
    fn drop(&mut self) {
        let mut state = match self.owner.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(p) = state.pending.as_mut().filter(|p| {
            p.probe.operation() == self.operation && Arc::ptr_eq(&p.worker_ticket, &self.ticket)
        }) {
            p.worker = false;
            if p.phase != ProcessingPhase::Faulted {
                p.phase = if p.applied {
                    ProcessingPhase::SettleUnconfirmed
                } else {
                    ProcessingPhase::BeginUnconfirmed
                };
            }
            if let Some(response) = p.response.take() {
                let _ = response.send(ConsumeReply::Pending {
                    request: p.probe.registration_id(),
                });
            }
        }
    }
}
#[cfg(windows)]
#[derive(Clone)]
pub(crate) struct QueueRuntimeIdentity(Arc<()>);
#[cfg(windows)]
impl QueueRuntimeIdentity {
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
pub(crate) trait RuntimeSupervisor: Send + Sync {
    #[cfg(windows)]
    fn queue_runtime_identity(&self) -> QueueRuntimeIdentity;
    #[cfg(windows)]
    fn register_preparation(
        &self,
        expected: &QueueRuntimeIdentity,
        install: &mut dyn FnMut() -> Result<(), RegistryError>,
    ) -> Result<(), RegistryError>;

    #[cfg(windows)]
    fn register_document(
        &self,
        _expected: &QueueRuntimeIdentity,
        _install: &mut dyn FnMut() -> Result<(), RegistryError>,
    ) -> Result<(), RegistryError> {
        Err(RegistryError::InvalidActor)
    }
    #[cfg(windows)]
    fn register_catalog(
        &self,
        _expected: &QueueRuntimeIdentity,
        _install: &mut dyn FnMut() -> Result<(), RegistryError>,
    ) -> Result<(), RegistryError> {
        Err(RegistryError::InvalidActor)
    }
    fn cancel(&self);
    fn stop_and_join(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}
pub(crate) struct RuntimeCore<T> {
    identity: Arc<()>,
    registry_owner: super::request_registry::RegistryOwnerScope,
    owners: Mutex<HashMap<NativeSessionKey, Arc<OwnerCell<T>>>>,
    resolver: Arc<dyn TargetResolver>,
    environment: Arc<dyn QueueEnvironmentPort>,
    runtime: RuntimeEpoch,
    native_enabled: AtomicBool,
    #[cfg(windows)]
    claim_release_deferred: AtomicBool,
    status: AtomicU8,
    claim: Arc<AtomicBool>,
    tasks: Mutex<Vec<Arc<TaskRecord>>>,
    shutdown: tokio::sync::Mutex<()>,
    stop: Notify,
    warnings: Mutex<Vec<AuditWarning>>,
    audit_cancelled: AtomicU64,
    task_failures: AtomicU64,
}
fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, RegistryError> {
    mutex.lock().map_err(|_| RegistryError::StorageUnavailable)
}
// Teardown only: recovering lifecycle metadata grants no new mutation/dispatch authority.
fn teardown_lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
fn release_waiters<T>(owners: &[Arc<OwnerCell<T>>]) {
    for owner in owners {
        let mut state = teardown_lock(&owner.state);
        if let Some(p) = state.pending.as_mut()
            && let Some(response) = p.response.take()
        {
            let _ = response.send(ConsumeReply::Pending {
                request: p.probe.registration_id(),
            });
        }
    }
}
fn pending_ids<T>(owners: &[Arc<OwnerCell<T>>]) -> Vec<RegistrationId> {
    owners
        .iter()
        .filter_map(|owner| {
            teardown_lock(&owner.state)
                .pending
                .as_ref()
                .map(|p| p.probe.registration_id())
        })
        .collect()
}
impl<T: Send + 'static> RuntimeCore<T> {
    fn owners_snapshot(&self) -> Result<Vec<Arc<OwnerCell<T>>>, RegistryError> {
        let owners = lock(&self.owners)?;
        let snapshot = owners.values().cloned().collect();
        drop(owners);
        Ok(snapshot)
    }
    fn record_task_failure(&self, task: &TaskRecord) {
        if task.failed.load(Ordering::SeqCst) && !task.failure_recorded.swap(true, Ordering::SeqCst)
        {
            let _ = self
                .task_failures
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(1));
        }
    }
    fn reap_completed(&self, tasks: &mut Vec<Arc<TaskRecord>>) {
        tasks.retain(|task| {
            if task.reap_finished() {
                self.record_task_failure(task);
                false
            } else {
                true
            }
        });
    }
    fn status(&self) -> StartupStatus {
        match self.status.load(Ordering::SeqCst) {
            0 => StartupStatus::Starting,
            1 => StartupStatus::Running,
            2 => StartupStatus::Unconfirmed,
            3 => StartupStatus::Stopping,
            _ => StartupStatus::Stopped,
        }
    }
    fn running(&self) -> Result<(), RegistryError> {
        match self.status() {
            StartupStatus::Running => Ok(()),
            StartupStatus::Starting | StartupStatus::Unconfirmed => {
                Err(RegistryError::NodeStarting)
            }
            _ => Err(RegistryError::NodeStopping),
        }
    }
    async fn owner(&self, target: EntityId) -> Result<Arc<OwnerCell<T>>, RegistryError> {
        let native = self.resolver.resolve(target).await?;
        self.owner_for(native)
    }
    fn owner_for(&self, native: NativeSessionKey) -> Result<Arc<OwnerCell<T>>, RegistryError> {
        let mut owners = lock(&self.owners)?;
        if let Some(owner) = owners.get(&native) {
            return Ok(owner.clone());
        }
        self.running()?;
        let owner = Arc::new(OwnerCell {
            identity: Arc::new(OwnerIdentity {
                id: random_id()?,
                runtime: self.runtime,
            }),
            native: native.clone(),
            state: Mutex::new(OwnerState {
                #[cfg(windows)]
                document_epochs: HashMap::new(),
                #[cfg(windows)]
                document_wakes: Vec::new(),
                queue: InputQueue::new(native.clone()),
                pending: None,
                active: None,
                external: None,
                external_serial: 0,
                native_order: 0,
                canonical_target: None,
                origins: HashMap::new(),
                priority_handoff: None,
                selected_priority: None,
            }),
            changed: Notify::new(),
        });
        owners.insert(native, owner.clone());
        drop(owners);
        Ok(owner)
    }
    fn readiness(&self, owner: &OwnerCell<T>) -> DispatchReadiness {
        let readiness = self.environment.readiness(&owner.native);
        match readiness {
            DispatchReadiness::Ready(e)
                if self.environment.resource_epoch(&owner.native) == Some(e) =>
            {
                readiness
            }
            _ => DispatchReadiness::Blocked,
        }
    }
    fn apply_event(
        &self,
        owner: &OwnerCell<T>,
        state: &mut OwnerState<T>,
        event: QueueOwnerEvent,
    ) -> Result<(), DriverError> {
        if self.status() == StartupStatus::Stopped {
            return Err(RegistryError::StaleObservation.into());
        }
        match event {
            QueueOwnerEvent::ActivityChanged => {
                let epoch = self.environment.resource_epoch(&owner.native);
                if state
                    .external
                    .as_ref()
                    .is_some_and(|w| Some(w.resource) != epoch)
                {
                    state.external = None;
                }
            }
            QueueOwnerEvent::Settle {
                active,
                disposition,
            } => {
                let (current, token) = state
                    .active
                    .as_ref()
                    .ok_or(RegistryError::StaleObservation)?;
                if !current.same(&active) {
                    return Err(RegistryError::StaleObservation.into());
                }
                #[cfg(windows)]
                let revision = active.revision;
                state.queue.settle(token, disposition)?;
                #[cfg(windows)]
                document_read::invalidate(state, active.item, revision);
                state.origins.remove(&active.item);
                state.active = None;
            }
            QueueOwnerEvent::ExternalForegroundFailure { work } => {
                self.require_external(owner, state, &work)?;
                state.queue.suspend_waiting();
                state.external = None;
            }
            QueueOwnerEvent::ExternalForegroundCompleted { work } => {
                self.require_external(owner, state, &work)?;
                state.external = None;
            }
        }
        Ok(())
    }
    fn require_external(
        &self,
        owner: &OwnerCell<T>,
        state: &OwnerState<T>,
        work: &ExternalWorkKey,
    ) -> Result<(), RegistryError> {
        if self.environment.resource_epoch(&owner.native) != Some(work.resource)
            || !state
                .external
                .as_ref()
                .is_some_and(|current| current.same(work))
        {
            return Err(RegistryError::StaleObservation);
        }
        Ok(())
    }
    fn drain(&self, owner: &OwnerCell<T>, state: &mut OwnerState<T>) -> Result<(), DriverError> {
        while let Some(event) = self.environment.poll_event(&owner.native) {
            match self.apply_event(owner, state, event) {
                Ok(()) | Err(DriverError::Registry(RegistryError::StaleObservation)) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

/// A node-owned handoff from this runtime's real D and reliable Prioritized E.
pub struct PriorityHandoff {
    identity: Arc<OwnerIdentity>,
    target: EntityId,
    item: EntityId,
    revision: u64,
    resource: Option<ResourceEpoch>,
    operation: EntityId,
    registration: RegistrationId,
}
impl PriorityHandoff {
    #[must_use]
    pub const fn operation(&self) -> EntityId {
        self.operation
    }
    #[must_use]
    pub const fn target(&self) -> EntityId {
        self.target
    }
    #[must_use]
    pub const fn item(&self) -> EntityId {
        self.item
    }
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    #[must_use]
    pub const fn resource(&self) -> Option<ResourceEpoch> {
        self.resource
    }
    #[must_use]
    pub const fn registration(&self) -> RegistrationId {
        self.registration
    }
}
pub struct NativeQueueDriver<T> {
    core: Arc<RuntimeCore<T>>,
}
impl<T> Clone for NativeQueueDriver<T> {
    fn clone(&self) -> Self {
        Self {
            core: self.core.clone(),
        }
    }
}
impl<T: Send + 'static> NativeQueueDriver<T> {
    /// # Errors
    /// Rejects pending mutations, inactive runtime and stale resource observations.
    pub async fn prepare(&self, target: EntityId) -> Result<Option<OwnedPreparation>, DriverError> {
        self.core.running()?;
        let owner = self.core.owner(target).await?;
        #[cfg(windows)]
        let _document_wake = document_read::DocumentWakeOnExit::new(&owner);
        let mut state = lock(&owner.state)?;
        self.core.running()?;
        self.core.drain(&owner, &mut state)?;
        if state.pending.is_some() {
            return Err(QueueError::Busy.into());
        }
        let readiness = self.core.readiness(&owner);
        let token = state.queue.prepare(readiness)?;
        drop(state);
        match (token, readiness) {
            (Some(token), DispatchReadiness::Ready(resource)) => Ok(Some(OwnedPreparation {
                identity: owner.identity.clone(),
                target,
                token,
                resource,
            })),
            (None, _) => Ok(None),
            _ => Err(RegistryError::CorruptState.into()),
        }
    }
    /// # Errors
    /// Rejects foreign or already invalidated preparations.
    pub async fn release(&self, preparation: &OwnedPreparation) -> Result<(), DriverError> {
        let owner = self.core.owner(preparation.target).await?;
        if !Arc::ptr_eq(&owner.identity, &preparation.identity) {
            return Err(QueueError::StaleToken.into());
        }
        lock(&owner.state)?
            .queue
            .release_preparation(&preparation.token)?;
        Ok(())
    }
    /// Local active selection only; this method cannot expose content or permit a native write.
    /// # Errors
    /// Rejects pending mutations, invalid tokens and changed readiness/resources.
    pub async fn begin_selection(
        &self,
        preparation: &OwnedPreparation,
    ) -> Result<ActiveKey, DriverError> {
        self.core.running()?;
        let owner = self.core.owner(preparation.target).await?;
        if !Arc::ptr_eq(&owner.identity, &preparation.identity) {
            return Err(QueueError::StaleToken.into());
        }
        #[cfg(windows)]
        let _document_wake = document_read::DocumentWakeOnExit::new(&owner);
        let mut state = lock(&owner.state)?;
        self.core.running()?;
        self.core.drain(&owner, &mut state)?;
        if state.pending.is_some() {
            return Err(QueueError::Busy.into());
        }
        state
            .queue
            .begin_send(&preparation.token, self.core.readiness(&owner))?;
        let entry = state.queue.dispatch_input(&preparation.token)?;
        let key = ActiveKey {
            identity: owner.identity.clone(),
            item: entry.id(),
            revision: entry.revision(),
            resource: preparation.resource,
            selection_order: state
                .native_order
                .checked_add(1)
                .ok_or(DriverError::SequenceExhausted)?,
        };
        state.native_order = key.selection_order;
        state.active = Some((key.clone(), preparation.token.clone()));
        drop(state);
        Ok(key)
    }
    /// # Errors
    /// Rejects a target that cannot be resolved within this node.
    pub async fn active_key(&self, target: EntityId) -> Result<Option<ActiveKey>, DriverError> {
        let owner = self.core.owner(target).await?;
        let key = lock(&owner.state)?
            .active
            .as_ref()
            .map(|(key, _)| key.clone());
        Ok(key)
    }
    /// # Errors
    /// Rejects old work capabilities without changing current queue state.
    pub async fn observe(
        &self,
        target: EntityId,
        event: QueueOwnerEvent,
    ) -> Result<(), DriverError> {
        let owner = self.core.owner(target).await?;
        #[cfg(windows)]
        let _document_wake = document_read::DocumentWakeOnExit::new(&owner);
        let mut state = lock(&owner.state)?;
        self.core.apply_event(&owner, &mut state, event)
    }
    /// Mints a node-local observation identity, not an ACP correlation proof.
    /// # Errors
    /// Rejects overlapping external work, stale resources and sequence exhaustion.
    pub async fn open_external_work(
        &self,
        target: EntityId,
        resource: ResourceEpoch,
    ) -> Result<ExternalWorkKey, DriverError> {
        self.core.running()?;
        let owner = self.core.owner(target).await?;
        #[cfg(windows)]
        let _document_wake = document_read::DocumentWakeOnExit::new(&owner);
        let mut state = lock(&owner.state)?;
        self.core.running()?;
        self.core.drain(&owner, &mut state)?;
        if self.core.environment.resource_epoch(&owner.native) != Some(resource) {
            return Err(RegistryError::StaleObservation.into());
        }
        if state.external.is_some() {
            return Err(QueueError::Busy.into());
        }
        let serial = state
            .external_serial
            .checked_add(1)
            .ok_or(DriverError::SequenceExhausted)?;
        let work = ExternalWorkKey {
            identity: owner.identity.clone(),
            resource,
            serial,
        };
        state.external_serial = serial;
        state.external = Some(work.clone());
        drop(state);
        Ok(work)
    }
}

#[cfg(test)]
mod tests;
