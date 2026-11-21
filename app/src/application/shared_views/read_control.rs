//! One owned actual read operation per canonical target. Consumer waiters never own its RPC.
use super::{
    BoxReadFuture, RetainedRaw, SharedViews,
    budget::{
        NativeReadOptions, NativeReadOptionsProvider, NativeReadStartContext, ReadReason,
        ReadResponseWait,
    },
    tasks::{ViewTaskKind, ViewTaskSupervisor},
};
use crate::domain::{
    EntityId,
    native::NativeSessionKey,
    shared_view::{ReadEpoch, ReadMode, ViewEpoch, ViewError},
};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::{sync::watch, time::Instant};

#[derive(Clone, Eq, PartialEq, Hash)]
pub enum RpcScalar {
    Integer(i64),
    String(Arc<str>),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadTransportState {
    Reserved,
    Queued,
    Writing,
    Written,
    Unknown,
    Zero,
}
pub struct ReadCompletionProof<S, R> {
    pub(crate) scope: S,
    pub(crate) read: ReadEpoch,
    pub(crate) target: NativeSessionKey,
    pub(crate) rpc: RpcScalar,
    pub(crate) sequence: u64,
    pub(crate) order: u64,
    pub(crate) _retained: R,
}
impl<S, R> ReadCompletionProof<S, R> {
    #[must_use]
    pub const fn scope(&self) -> &S {
        &self.scope
    }
    #[must_use]
    pub const fn read_epoch(&self) -> ReadEpoch {
        self.read
    }
    #[must_use]
    pub const fn target(&self) -> &NativeSessionKey {
        &self.target
    }
    #[must_use]
    pub const fn rpc(&self) -> &RpcScalar {
        &self.rpc
    }
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    #[must_use]
    pub const fn receipt_order(&self) -> u64 {
        self.order
    }
}
pub enum ReplayReadStatus<'a, S, R> {
    Pending,
    Completed(&'a ReadCompletionProof<S, R>),
    Incomplete(ViewError),
}
pub struct NativeReadStatus<'a, S, R> {
    pub transport: ReadTransportState,
    pub completion: ReplayReadStatus<'a, S, R>,
}
pub enum ReadEnqueueError<R, O> {
    RetainedReservation { error: ViewError, reservation: R },
    RetainedOperation { error: ViewError, operation: O },
}
pub type ReadEnqueueResult<R, O> = Result<O, ReadEnqueueError<R, O>>;
pub type ReadObserveResult<'a, S, R> = Result<NativeReadStatus<'a, S, R>, ViewError>;
pub struct ReadCancelError<R> {
    pub(crate) error: ViewError,
    pub(crate) original: R,
}
impl<R> ReadCancelError<R> {
    #[must_use]
    pub const fn error(&self) -> ViewError {
        self.error
    }
    #[must_use]
    pub fn into_original(self) -> R {
        self.original
    }
}
pub enum ReadBindingOutcome<B> {
    Replay(B),
    CurrentOnlyWithoutReplay,
}
pub trait NativeHistoryPort: Send + Sync + 'static {
    type Scope: Clone + Eq + Send + Sync + 'static;
    type Raw: RetainedRaw;
    type Binding: Send + Sync + 'static;
    type Reservation: Send + 'static;
    type Operation: Send + 'static;
    fn acquire_read_binding<'a>(
        &'a self,
        target: &'a NativeSessionKey,
        reason: ReadReason,
    ) -> BoxReadFuture<'a, Result<ReadBindingOutcome<Self::Binding>, ViewError>>;
    fn scope<'a>(&self, binding: &'a Self::Binding) -> &'a Self::Scope;
    /// # Errors
    /// Rejects a stale original binding, unavailable configuration or protocol reservation.
    fn reserve_replay(
        &self,
        binding: &Self::Binding,
        read: ReadEpoch,
        options: NativeReadOptions,
    ) -> Result<Self::Reservation, ViewError>;
    fn enqueue_replay<'a>(
        &'a self,
        binding: &'a Self::Binding,
        reservation: Self::Reservation,
    ) -> BoxReadFuture<'a, ReadEnqueueResult<Self::Reservation, Self::Operation>>;
    fn observe<'a>(
        &'a self,
        operation: &'a mut Self::Operation,
    ) -> BoxReadFuture<'a, ReadObserveResult<'a, Self::Scope, Self::Raw>>;
    /// # Errors
    /// A cancellation failure retains the original unqueued reservation.
    fn cancel_reserved(
        &self,
        reservation: Self::Reservation,
    ) -> Result<ReadTransportState, ReadCancelError<Self::Reservation>>;
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeReadBudgetSelection {
    epoch: ReadEpoch,
    selected: Instant,
    options: NativeReadOptions,
}
impl NativeReadBudgetSelection {
    #[must_use]
    pub const fn read_epoch(self) -> ReadEpoch {
        self.epoch
    }
    #[must_use]
    pub const fn selected_at(self) -> Instant {
        self.selected
    }
    #[must_use]
    pub const fn write_deadline(self) -> Instant {
        self.options.write_deadline
    }
    #[must_use]
    pub const fn response_wait(self) -> ReadResponseWait {
        self.options.response_wait
    }
}
#[derive(Clone)]
pub struct NativeReadMetadata {
    operation: EntityId,
    read: ReadEpoch,
    mode: ReadMode,
    transport: ReadTransportState,
    options: Option<NativeReadBudgetSelection>,
    failure: Option<ViewError>,
    bound: bool,
}
impl NativeReadMetadata {
    #[must_use]
    pub const fn read_epoch(&self) -> Option<ReadEpoch> {
        if self.bound { Some(self.read) } else { None }
    }
    #[must_use]
    pub const fn read_mode(&self) -> ReadMode {
        self.mode
    }
    #[must_use]
    pub const fn transport(&self) -> ReadTransportState {
        self.transport
    }
    #[must_use]
    pub const fn failure(&self) -> Option<ViewError> {
        self.failure
    }
    #[must_use]
    pub const fn selected_options(&self) -> Option<NativeReadBudgetSelection> {
        self.options
    }
    pub(crate) const fn operation_id(&self) -> EntityId {
        self.operation
    }
}
pub(super) struct ReadJob {
    pub(super) confirmed: std::sync::OnceLock<ViewEpoch>,
    id: EntityId,
    pub(super) read: ReadEpoch,
    metadata: watch::Sender<NativeReadMetadata>,
    active: AtomicBool,
    desired: AtomicBool,
}
pub struct NativeReadRequest {
    id: EntityId,
    job: Arc<ReadJob>,
}
impl NativeReadRequest {
    pub(super) fn confirms_view(&self, epoch: ViewEpoch) -> bool {
        self.job.confirmed.get() == Some(&epoch)
    }
    #[must_use]
    pub const fn request_id(&self) -> EntityId {
        self.id
    }
    #[must_use]
    pub fn read_epoch(&self) -> Option<ReadEpoch> {
        self.job.metadata.borrow().read_epoch()
    }
    pub(crate) fn subscribe(&self) -> watch::Receiver<NativeReadMetadata> {
        self.job.metadata.subscribe()
    }
    pub(crate) fn pinned_operation(&self) -> EntityId {
        self.job.id
    }
}
pub trait NativeReadControl: Send + Sync + 'static {
    /// # Errors
    /// Reports actual scheduling or source lifetime failure without cancelling another read.
    fn request(
        &self,
        target: &NativeSessionKey,
        reason: ReadReason,
    ) -> Result<NativeReadRequest, ViewError>;
    fn status(&self, request: &NativeReadRequest) -> NativeReadMetadata;
}
pub struct NativeReadCoordinator<H: NativeHistoryPort> {
    core: Arc<CoordinatorCore<H>>,
}
struct CoordinatorCore<H: NativeHistoryPort> {
    bridge: Arc<H>,
    views: SharedViews<H::Scope, H::Raw>,
    supervisor: ViewTaskSupervisor,
    options: Arc<dyn NativeReadOptionsProvider>,
    jobs: Mutex<HashMap<NativeSessionKey, Arc<ReadJob>>>,
}
impl<H: NativeHistoryPort> NativeReadCoordinator<H> {
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    pub(crate) fn request_selected(
        &self,
        target: &NativeSessionKey,
        options: NativeReadOptions,
    ) -> Result<NativeReadRequest, ViewError> {
        self.core
            .request_inner(target, ReadReason::FirstOpen, Some(options))
    }
    pub(crate) fn new(
        bridge: Arc<H>,
        views: SharedViews<H::Scope, H::Raw>,
        supervisor: ViewTaskSupervisor,
        options: Arc<dyn NativeReadOptionsProvider>,
    ) -> Result<Arc<Self>, ViewError> {
        if views.core.probe.is_sealed() {
            return Err(ViewError::SourceReleased);
        }
        if *supervisor.subscribe_stop().borrow() {
            return Err(ViewError::NodeStopping);
        }
        Ok(Arc::new(Self {
            core: Arc::new(CoordinatorCore {
                bridge,
                views,
                supervisor,
                options,
                jobs: Mutex::new(HashMap::new()),
            }),
        }))
    }
}
impl<H: NativeHistoryPort> CoordinatorCore<H> {
    fn request(
        self: &Arc<Self>,
        target: &NativeSessionKey,
        reason: ReadReason,
    ) -> Result<NativeReadRequest, ViewError> {
        self.request_inner(target, reason, None)
    }
    fn request_inner(
        self: &Arc<Self>,
        target: &NativeSessionKey,
        reason: ReadReason,
        selected: Option<NativeReadOptions>,
    ) -> Result<NativeReadRequest, ViewError> {
        self.request_expected(target, reason, selected, None)
    }
    fn request_expected(
        self: &Arc<Self>,
        target: &NativeSessionKey,
        reason: ReadReason,
        selected: Option<NativeReadOptions>,
        previous: Option<&Arc<ReadJob>>,
    ) -> Result<NativeReadRequest, ViewError> {
        let id =
            EntityId::from_bytes(crate::random::bytes().map_err(|_| ViewError::RandomUnavailable)?);
        let mut jobs = self.jobs.lock().map_err(|_| ViewError::ReadFailure)?;
        if let (Some(previous), Some(current)) = (previous, jobs.get(target))
            && !Arc::ptr_eq(previous, current)
        {
            return Ok(NativeReadRequest {
                id,
                job: Arc::clone(current),
            });
        }
        if let Some(job) = jobs
            .get(target)
            .filter(|job| job.active.load(Ordering::Acquire))
        {
            if selected.is_some() {
                return Err(ViewError::InvalidStage);
            }
            if reason == ReadReason::ExplicitRefresh {
                job.desired.store(true, Ordering::Release);
            }
            return Ok(NativeReadRequest {
                id,
                job: Arc::clone(job),
            });
        }
        jobs.try_reserve(1)
            .map_err(|_| ViewError::AllocationFailure)?;
        #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
        self.views.core.controller.fault(
            super::controller::ViewFalliblePoint::NewReadEpochRandom,
            super::controller::HookContext {
                scope: Some(self.views.core.scope.clone()),
                target: Some(target.clone()),
                ..Default::default()
            },
        )?;
        let read = ReadEpoch::fresh()?;
        let operation =
            EntityId::from_bytes(crate::random::bytes().map_err(|_| ViewError::RandomUnavailable)?);
        let (metadata, _) = watch::channel(NativeReadMetadata {
            operation,
            read,
            mode: ReadMode::ReplayRequested,
            transport: ReadTransportState::Reserved,
            options: None,
            failure: None,
            bound: false,
        });
        let job = Arc::new(ReadJob {
            confirmed: std::sync::OnceLock::new(),
            id: operation,
            read,
            metadata,
            active: AtomicBool::new(true),
            desired: AtomicBool::new(false),
        });
        let core = Arc::clone(self);
        let running = Arc::clone(&job);
        let key = target.clone();
        let generation = self.views.core.target(target)?.send.clone();
        self.supervisor.spawn_target(
            ViewTaskKind::NativeRead,
            target,
            generation,
            move |context| async move {
                let result = core.run_operation(&key, reason, &running, selected).await;
                if let Err(error) = result {
                    running.metadata.send_modify(|metadata| {
                        if metadata.mode != ReadMode::ReplayComplete {
                            metadata.mode = ReadMode::ReplayIncomplete;
                            metadata.failure = Some(error);
                        }
                    });
                }
                let successor = {
                    let jobs = core.jobs.lock().map_err(|_| ViewError::ReadFailure)?;
                    running.active.store(false, Ordering::Release);
                    let same = jobs
                        .get(&key)
                        .is_some_and(|current| Arc::ptr_eq(current, &running));
                    drop(jobs);
                    same && running.desired.swap(false, Ordering::AcqRel)
                };
                if successor && !core.views.core.probe.is_sealed() {
                    // This is a new owned operation. The original request remains pinned to running.
                    let _ = core.request_expected(
                        &key,
                        ReadReason::ExplicitRefresh,
                        None,
                        Some(&running),
                    );
                }
                context.retire(running).await?;
                Ok(())
            },
        )?;
        jobs.insert(target.clone(), Arc::clone(&job));
        drop(jobs);
        Ok(NativeReadRequest { id, job })
    }
    fn register_replay_comparison(
        &self,
        target: &NativeSessionKey,
        reason: ReadReason,
        job: &Arc<ReadJob>,
    ) -> Result<(), ViewError> {
        if matches!(reason, ReadReason::Reopen | ReadReason::Reconnect) {
            match self.views.core.target_publication(target) {
                Ok(publication) => {
                    if let Some(publication) = publication.upgrade() {
                        *publication
                            .comparison
                            .lock()
                            .map_err(|_| ViewError::ReadFailure)? = Arc::downgrade(job);
                    }
                }
                Err(ViewError::InvalidTarget) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
    async fn run_operation(
        &self,
        target: &NativeSessionKey,
        reason: ReadReason,
        job: &Arc<ReadJob>,
        selected: Option<NativeReadOptions>,
    ) -> Result<(), ViewError> {
        let binding = match self.bridge.acquire_read_binding(target, reason).await? {
            ReadBindingOutcome::Replay(binding) => binding,
            ReadBindingOutcome::CurrentOnlyWithoutReplay => {
                job.metadata.send_modify(|metadata| {
                    metadata.mode = ReadMode::CurrentOnly;
                    metadata.failure = None;
                });
                return Ok(());
            }
        };
        if self.bridge.scope(&binding) != &self.views.core.scope
            || self.views.core.probe.is_sealed()
        {
            return Err(ViewError::SourceReplaced);
        }
        let context = NativeReadStartContext::sample(target, reason, job.read);
        let options = match selected {
            Some(options) => options,
            None => self.options.for_new_operation(&context)?,
        };
        job.metadata.send_modify(|metadata| {
            metadata.options = Some(NativeReadBudgetSelection {
                epoch: job.read,
                selected: context.now(),
                options,
            });
        });
        self.register_replay_comparison(target, reason, job)?;
        let reservation = self.bridge.reserve_replay(&binding, job.read, options)?;
        let mut operation = match self.bridge.enqueue_replay(&binding, reservation).await {
            Ok(operation) => operation,
            Err(ReadEnqueueError::RetainedReservation { error, reservation }) => {
                // Only a genuinely unqueued original reservation can be cancelled here.
                let transport = self
                    .bridge
                    .cancel_reserved(reservation)
                    .map_err(|error| error.error())?;
                job.metadata
                    .send_modify(|metadata| metadata.transport = transport);
                return Err(error);
            }
            Err(ReadEnqueueError::RetainedOperation { error, operation }) => {
                // An adopted operation is still observed to its actual terminal; no waiter cancellation.
                job.metadata
                    .send_modify(|metadata| metadata.failure = Some(error));
                operation
            }
        };
        job.metadata.send_modify(|metadata| metadata.bound = true);
        loop {
            let status = self.bridge.observe(&mut operation).await?;
            let mut completed = false;
            let mut incomplete = None;
            match status.completion {
                ReplayReadStatus::Pending => {}
                ReplayReadStatus::Completed(proof) => {
                    if proof.scope() != self.bridge.scope(&binding)
                        || proof.target() != target
                        || proof.read_epoch() != job.read
                    {
                        return Err(ViewError::InvalidScope);
                    }
                    completed = true;
                }
                ReplayReadStatus::Incomplete(error) => incomplete = Some(error),
            }
            job.metadata.send_modify(|metadata| {
                metadata.transport = status.transport;
                if completed {
                    metadata.mode = ReadMode::ReplayComplete;
                } else if metadata.mode != ReadMode::ReplayComplete {
                    metadata.mode = if incomplete.is_some() {
                        ReadMode::ReplayIncomplete
                    } else {
                        ReadMode::Replaying
                    };
                    metadata.failure = metadata.failure.or(incomplete);
                }
            });
            let terminal = matches!(
                status.transport,
                ReadTransportState::Written
                    | ReadTransportState::Unknown
                    | ReadTransportState::Zero
            );
            if terminal && (completed || incomplete.is_some()) {
                break;
            }
        }
        // The bridge operation's original owner retains its IO/Ticket beyond this observation.
        Ok(())
    }
}
impl<H: NativeHistoryPort> NativeReadControl for NativeReadCoordinator<H> {
    fn request(
        &self,
        target: &NativeSessionKey,
        reason: ReadReason,
    ) -> Result<NativeReadRequest, ViewError> {
        self.core.request(target, reason)
    }
    fn status(&self, request: &NativeReadRequest) -> NativeReadMetadata {
        request.job.metadata.borrow().clone()
    }
}
