//! The single native runtime shares the original queue owner and its physical SQL closer.
#![allow(
    clippy::type_complexity,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    AcceptedTargetProof, Arc, AtomicBool, BoxFuture, CancelledLifecycleReservation,
    ConfigurationPort, ConvergenceProbe, DispatchError, EntityId, InitializeMetadata, Instant,
    IoShutdownObservation, LaunchPermit, LifecycleKind, LoadTransportProof, LoadedResourceProof,
    NativeControlRepository, NativeDispatchRepository, NativeEnvironment, NativeIoOwner,
    NativeResourceRepository, NativeSessionKey, Ordering, OwnedNativeSource, OwnerCloseKind,
    PreResourceProbe, PreSourceProbe, PreparedProtocol, ProtocolVersion, ReservedLoadProof,
    ResourceClosureProof, ResourceStateProof, ResponseWait, RetainPrompt, RpcScalar,
    SourceBoundRecordProof, SourceClosureProof, SourceLaunchController, SourceLaunchPort,
    SourceLossProbe, SourceRecord, SourceStateProof, VerifiedLoadedBinding,
};
use super::{environment::NativeTarget, tasks::TaskRegistry};
use crate::{
    application::{
        queue_owner::{
            NativeQueueDriver, NativeQueueLink, PreparedCapture, PriorityFailure, PriorityHandoff,
            SelectedContext,
        },
        request_registry::{NodeClock, OperationContext, RegistryDeadline, random_id},
    },
    domain::native::RuntimeEpoch,
};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock, atomic::AtomicU8},
    time::Duration,
};
use tokio::sync::Notify;

#[derive(Clone, Copy)]
pub enum ResponseBudget {
    Wait(Duration),
    Unbounded,
}
pub struct NativeRuntimeOptions {
    pub spawn_wait: Duration,
    pub initialize_write_wait: Duration,
    pub initialize_response_wait: Duration,
    pub load_write_wait: Duration,
    pub load_response: ResponseBudget,
    pub prompt_write_wait: Duration,
    pub prompt_response: ResponseBudget,
    pub metadata_wait: Duration,
    pub close_wait: Duration,
}
impl NativeRuntimeOptions {
    pub(crate) fn validate(&self) -> Result<(), DispatchError> {
        for budget in [
            self.spawn_wait,
            self.initialize_write_wait,
            self.initialize_response_wait,
            self.load_write_wait,
            self.prompt_write_wait,
            self.metadata_wait,
            self.close_wait,
        ] {
            Self::at(budget)?;
        }
        for response in [self.load_response, self.prompt_response] {
            if let ResponseBudget::Wait(budget) = response {
                Self::at(budget)?;
            }
        }
        Ok(())
    }
    pub(super) fn at(wait: Duration) -> Result<Instant, DispatchError> {
        Instant::now()
            .checked_add(wait)
            .ok_or(DispatchError::InvalidConfiguration)
    }
    pub(super) fn response(wait: ResponseBudget) -> Result<ResponseWait, DispatchError> {
        match wait {
            ResponseBudget::Wait(d) => Ok(ResponseWait::At(Self::at(d)?)),
            ResponseBudget::Unbounded => Ok(ResponseWait::Unbounded),
        }
    }
}
type CapturedSelection<B> = Result<SelectedContext<B>, (DispatchError, PreparedCapture<B>)>;
pub(super) trait QueueBridge<B: Clone + Send + Sync + 'static>: Send + Sync {
    fn automatic_close_tree_guard(
        &self,
        natives: &[NativeSessionKey],
    ) -> Result<Arc<dyn crate::application::queue_owner::AutomaticCloseGuard>, DispatchError>;
    #[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
    fn automatic_close_guard(
        &self,
        native: &NativeSessionKey,
    ) -> Result<Arc<dyn crate::application::queue_owner::AutomaticCloseGuard>, DispatchError>;
    fn verify_read_target(
        &self,
        read: &crate::application::queue_owner::ReadTargetProof,
    ) -> Result<(), DispatchError>;
    fn target_for_native(&self, key: &NativeSessionKey) -> Result<Option<EntityId>, DispatchError>;
    fn accepted(
        &self,
        target: EntityId,
    ) -> BoxFuture<'_, Result<Option<AcceptedTargetProof>, DispatchError>>;
    fn prepare(
        &self,
        target: EntityId,
    ) -> BoxFuture<'_, Result<Option<PreparedCapture<B>>, DispatchError>>;
    fn select(&self, capture: PreparedCapture<B>) -> BoxFuture<'_, CapturedSelection<B>>;
    fn direct(
        &self,
        target: EntityId,
    ) -> BoxFuture<'_, Result<Option<SelectedContext<B>>, DispatchError>>;
    fn priority(
        &self,
        target: EntityId,
    ) -> BoxFuture<'_, Result<Option<PriorityHandoff>, DispatchError>>;
    fn fail_priority_for_native<'a>(
        &'a self,
        key: &'a NativeSessionKey,
        failure: PriorityFailure,
    ) -> BoxFuture<'a, Result<(), DispatchError>>;
}
struct DriverBridge<T, B> {
    driver: NativeQueueDriver<T>,
    retain: Arc<dyn RetainPrompt<T, Retained = B>>,
}
impl<T: Send + 'static, B: Clone + Send + Sync + 'static> QueueBridge<B> for DriverBridge<T, B> {
    fn automatic_close_tree_guard(
        &self,
        natives: &[NativeSessionKey],
    ) -> Result<Arc<dyn crate::application::queue_owner::AutomaticCloseGuard>, DispatchError> {
        self.driver.automatic_close_tree_guard(natives)
    }
    #[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
    fn automatic_close_guard(
        &self,
        native: &NativeSessionKey,
    ) -> Result<Arc<dyn crate::application::queue_owner::AutomaticCloseGuard>, DispatchError> {
        self.driver.automatic_close_guard(native)
    }
    fn verify_read_target(
        &self,
        read: &crate::application::queue_owner::ReadTargetProof,
    ) -> Result<(), DispatchError> {
        self.driver.verify_read_target(read)
    }
    fn target_for_native(&self, key: &NativeSessionKey) -> Result<Option<EntityId>, DispatchError> {
        self.driver.target_for_native(key)
    }
    fn accepted(
        &self,
        target: EntityId,
    ) -> BoxFuture<'_, Result<Option<AcceptedTargetProof>, DispatchError>> {
        Box::pin(self.driver.accepted_target(target))
    }
    fn prepare(
        &self,
        target: EntityId,
    ) -> BoxFuture<'_, Result<Option<PreparedCapture<B>>, DispatchError>> {
        Box::pin(self.driver.prepare_capture(target, &*self.retain))
    }
    fn select(&self, capture: PreparedCapture<B>) -> BoxFuture<'_, CapturedSelection<B>> {
        Box::pin(self.driver.select_capture(capture))
    }
    fn direct(
        &self,
        target: EntityId,
    ) -> BoxFuture<'_, Result<Option<SelectedContext<B>>, DispatchError>> {
        Box::pin(self.driver.capture_direct(target, &*self.retain))
    }
    fn priority(
        &self,
        target: EntityId,
    ) -> BoxFuture<'_, Result<Option<PriorityHandoff>, DispatchError>> {
        Box::pin(self.driver.take_priority_handoff(target))
    }
    fn fail_priority_for_native<'a>(
        &'a self,
        key: &'a NativeSessionKey,
        failure: PriorityFailure,
    ) -> BoxFuture<'a, Result<(), DispatchError>> {
        Box::pin(self.driver.fail_priority_for_native(key, failure))
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeRuntimeStatus {
    Starting,
    Running,
    Unconfirmed,
    Stopping,
    Stopped,
}
pub struct NativeRuntimeHandle<P, L, G, N>
where
    P: crate::application::native_dispatch::NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + crate::application::native_dispatch::NativeApprovalRepository,
{
    pub(crate) core: Arc<NativeCoordinator<P, L, G, N>>,
}
impl<P, L, G, N> Clone for NativeRuntimeHandle<P, L, G, N>
where
    P: crate::application::native_dispatch::NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + crate::application::native_dispatch::NativeApprovalRepository,
{
    fn clone(&self) -> Self {
        Self {
            core: self.core.clone(),
        }
    }
}
pub struct NativeCoordinator<P, L, G, N>
where
    P: crate::application::native_dispatch::NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + crate::application::native_dispatch::NativeApprovalRepository,
{
    pub(super) protocol: Arc<P>,
    pub(super) launcher: Arc<L>,
    pub(super) configuration: Arc<G>,
    pub(super) view_factory: OnceLock<Arc<dyn super::view_attachment::NativeSourceViewFactory<P>>>,
    #[cfg(windows)]
    pub(super) source_facade: OnceLock<()>,
    #[cfg(windows)]
    pub(super) source_admission: Mutex<()>,
    #[cfg(windows)]
    pub(super) source_sql: Arc<super::source_sessions::SourceSqlInventory>,
    #[cfg(windows)]
    pub(super) source_sql_closer: OnceLock<Arc<dyn super::source_sessions::SourceSqlClosurePort>>,
    pub(super) repository: Arc<N>,
    pub(super) queue: Arc<dyn QueueBridge<P::Retained>>,
    pub(super) environment: Arc<NativeEnvironment<P>>,
    pub(super) clock: Arc<dyn NodeClock>,
    pub(super) runtime: RuntimeEpoch,
    pub(super) options: NativeRuntimeOptions,
    pub(super) sources: Mutex<Vec<Arc<SourceSlot<P, L>>>>,
    pub(super) capacity_routes:
        tokio::sync::Mutex<HashMap<NativeSessionKey, std::sync::Weak<SourceSlot<P, L>>>>,
    pub(super) capacity_recycling: tokio::sync::Mutex<()>,
    pub(super) capacity_scan_started: AtomicBool,
    pub(super) capacity_scan: OnceLock<Arc<super::tasks::TaskSlot>>,
    pub(super) tasks: TaskRegistry,
    #[cfg(windows)]
    pub(super) source_close_tasks: TaskRegistry,
    pub(super) status: AtomicU8,
    pub(super) stop: Notify,
    pub(super) shutdown: tokio::sync::Mutex<()>,
    pub(super) shutdown_operation: Mutex<Option<Arc<super::shutdown::ShutdownOperation>>>,
    pub(super) shutdown_tasks: TaskRegistry,
    pub(super) close_progress: Arc<super::tasks::CloseProgress>,
    pub(super) last_shutdown: Mutex<Option<super::NativeShutdownReport>>,
    pub(super) claim: Arc<AtomicBool>,
    pub(super) workers: Mutex<HashMap<NativeSessionKey, Arc<Notify>>>,
    pub(super) runs: Mutex<HashMap<NativeSessionKey, Arc<super::dispatch::RunningDispatch<P>>>>,
}
pub(super) struct SourceSlot<P, L>
where
    P: crate::application::native_dispatch::NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
{
    pub(super) pre: Arc<PreSourceProbe>,
    pub(super) plan: Arc<super::SourceInitializationPlan<L::Plan, P::Setup>>,
    pub(super) recorded: AtomicBool,
    pub(super) controller: Arc<SourceLaunchController>,
    pub(super) launch_outcome: Mutex<Option<super::source_ports::LaunchOutcome<L::Running>>>,
    pub(super) permit: Mutex<Option<LaunchPermit>>,
    pub(super) owned: Mutex<Option<Arc<tokio::sync::Mutex<L::Owned>>>>,
    pub(super) io: Mutex<Option<Arc<P::IoOwner>>>,
    pub(super) source: OnceLock<P::Source>,
    pub(super) gate: OnceLock<P::Gate>,
    pub(super) memory: Mutex<SourceMemory<P::Scope>>,
    pub(super) targets: Mutex<HashMap<NativeSessionKey, Arc<ResourceSlot<P>>>>,
    pub(super) capacity:
        Mutex<HashMap<NativeSessionKey, Arc<super::capacity::CapacityResource<P>>>>,
    pub(super) capacity_reservations: Arc<Mutex<Vec<EntityId>>>,
    pub(super) capacity_activity: Mutex<super::capacity_activity::CapacityActivity>,
    pub(super) external_reads:
        Mutex<HashMap<NativeSessionKey, Arc<super::SourceReadCapability<P>>>>,
    pub(super) all_resources: Mutex<Vec<Arc<ResourceSlot<P>>>>,
    pub(super) close_intent: OnceLock<EntityId>,
    pub(super) close_deadline: Mutex<Option<Instant>>,
    pub(super) operations: Mutex<HashMap<RpcScalar, Arc<LifecycleOperation<P>>>>,
    #[cfg(windows)]
    pub(super) source_replies: Mutex<
        HashMap<
            RpcScalar,
            Arc<dyn super::source_sessions::SourceReplyOperation<P::Scope, P::Lease>>,
        >,
    >,
    pub(super) tasks: TaskRegistry,
    pub(super) jobs: TaskRegistry,
    pub(super) approval_recoveries:
        Mutex<Vec<Arc<super::approval_recovery::ApprovalRecoveryOperation>>>,
    pub(super) metadata_closer: OnceLock<Arc<super::tasks::TaskSlot>>,
    pub(super) metadata_close: super::source_jobs::SourceCloseOperation,
    pub(super) progress: Arc<super::tasks::CloseProgress>,
    pub(super) prepared: Mutex<Option<PreparedProtocol<P>>>,
    pub(super) prepare_failure: Mutex<Option<super::PrepareFailurePhase>>,
    pub(super) bound_record: Mutex<Option<Arc<SourceBoundRecordProof>>>,
    pub(super) spawned_record: Mutex<Option<Arc<SourceStateProof>>>,
    pub(super) failure_record: Mutex<Option<Arc<SourceStateProof>>>,
    pub(super) initialize: Mutex<Option<Arc<LifecycleOperation<P>>>>,
    pub(super) changed: Arc<Notify>,
    pub(super) view_attachment: Arc<super::view_attachment::ViewAttachmentHandoff>,
    pub(super) install: tokio::sync::Mutex<()>,
    pub(super) close: tokio::sync::Mutex<()>,
    pub(super) raw: Mutex<Vec<P::RawEvent>>,
    pub(super) loss: Mutex<Option<Arc<SourceLossProbe>>>,
    pub(super) pump: OnceLock<Arc<super::tasks::TaskSlot>>,
    pub(super) cleanup: OnceLock<Arc<super::tasks::TaskSlot>>,
    pub(super) observations: Mutex<Vec<Arc<super::observation::PendingObservation>>>,
    pub(super) tree_observations: Mutex<Vec<Arc<super::source_tree::PendingTreeObservation>>>,
    pub(super) physical_closed: Mutex<Option<super::shutdown::SourcePhysicalClosure>>,
    pub(super) closure: Mutex<Option<Arc<SourceClosureProof>>>,
    pub(super) owner_closed: Mutex<Option<OwnerCloseKind>>,
    pub(super) owner_witness: Mutex<Option<super::source_ports::OwnerClosureWitness>>,
    pub(super) io_closed: Mutex<Option<IoShutdownObservation>>,
    pub(super) controls: Mutex<Vec<Arc<super::control::ControlOperation<P>>>>,
    pub(super) control_staging: Mutex<Vec<Arc<super::control::ControlStaging<P>>>>,
    pub(super) retired_runs: Mutex<Vec<Arc<super::dispatch::RunningDispatch<P>>>>,
    pub(super) dispatches: Mutex<Vec<Arc<super::dispatch::RunningDispatch<P>>>>,
    pub(super) dispatch_closures: Mutex<Option<Vec<[u8; 32]>>>,
    pub(super) permissions:
        Mutex<HashMap<RpcScalar, Arc<super::environment::SourcePermissionSlot<P>>>>,
}
pub(super) struct SourceMemory<S> {
    pub(super) phase: SourcePhase,
    pub(super) scope: Option<S>,
    pub(super) binding: Option<EntityId>,
    pub(super) initialized: Option<InitializedSource>,
    pub(super) error: Option<DispatchError>,
    pub(super) requirements: HashMap<EntityId, DispatchError>,
    pub(super) observed_sequence: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SourcePhase {
    Reserved,
    Owned,
    Prepared,
    Bound,
    Started,
    Initialized,
    Closing,
    Closed,
    Unknown,
}
#[derive(Clone)]
pub(super) struct InitializedSource {
    pub(super) metadata: InitializeMetadata,
    pub(super) record: SourceRecord,
}
pub(super) struct ResourceSlot<P: crate::application::native_dispatch::NativeApprovalPort> {
    pub(super) target: Arc<NativeTarget<P>>,
    pub(super) pre: Arc<PreResourceProbe>,
    pub(super) queue: Arc<dyn NativeQueueLink>,
    pub(super) loaded: OnceLock<Arc<VerifiedLoadedBinding<P::Scope, P::BoundSetup>>>,
    pub(super) read_capability: OnceLock<Arc<super::SourceReadCapability<P>>>,
    pub(super) load: Mutex<Option<Arc<LifecycleOperation<P>>>>,
    pub(super) setup: OnceLock<Arc<P::BoundSetup>>,
    pub(super) recorded: AtomicBool,
    pub(super) install: tokio::sync::Mutex<()>,
    pub(super) close: tokio::sync::Mutex<()>,
    pub(super) close_intent: OnceLock<EntityId>,
    pub(super) close_deadline: Mutex<Option<Instant>>,
    pub(super) close_operation: Mutex<Option<Arc<LifecycleOperation<P>>>>,
    pub(super) convergence: Mutex<Option<Arc<ConvergenceProbe>>>,
    pub(super) release: Mutex<Option<Arc<ResourceClosureProof>>>,
    pub(super) releasing_record: Mutex<Option<Arc<ResourceStateProof>>>,
}
pub(super) struct LifecycleOperation<P: crate::application::native_dispatch::NativeApprovalPort> {
    pub(super) rpc: RpcScalar,
    pub(super) kind: LifecycleKind,
    pub(super) response: Mutex<Option<LifecycleResponse>>,
    pub(super) read_failure: Mutex<Option<DispatchError>>,
    pub(super) changed: Notify,
    pub(super) initialize: Mutex<Option<(P::InitReservation, P::Frame)>>,
    pub(super) resume: Mutex<Option<(P::LoadReservation, P::Frame)>>,
    pub(super) close_objects: Mutex<Option<(P::CloseReservation, P::Frame)>>,
    pub(super) ticket: Mutex<Option<Arc<P::Ticket>>>,
    pub(super) frame_length: u64,
    pub(super) at: i64,
    pub(super) initialize_record: Mutex<Option<Arc<SourceStateProof>>>,
    pub(super) load_record: Mutex<Option<Arc<ReservedLoadProof>>>,
    pub(super) transport_record: Mutex<Option<Arc<LoadTransportProof>>>,
    pub(super) loaded_record: Mutex<Option<Arc<LoadedResourceProof>>>,
    pub(super) cancelled_resume: Mutex<Option<CancelledLifecycleReservation<P::Scope, P::Probe>>>,
    pub(super) retired_probe: Mutex<Option<P::Probe>>,
}
#[derive(Clone)]
pub(super) struct LifecycleResponse {
    pub(super) sequence: u64,
    pub(super) receipt: u64,
    pub(super) value: LifecycleResponseValue,
}
#[derive(Clone)]
pub(super) enum LifecycleResponseValue {
    Initialized(ProtocolVersion),
    Resumed(Arc<str>),
    Closed(Arc<str>),
    Error,
}
impl<P: crate::application::native_dispatch::NativeApprovalPort> LifecycleOperation<P> {
    pub(super) fn new(rpc: RpcScalar, kind: LifecycleKind, frame_length: u64, at: i64) -> Self {
        Self {
            rpc,
            kind,
            response: Mutex::new(None),
            read_failure: Mutex::new(None),
            changed: Notify::new(),
            initialize: Mutex::new(None),
            resume: Mutex::new(None),
            close_objects: Mutex::new(None),
            ticket: Mutex::new(None),
            frame_length,
            at,
            initialize_record: Mutex::new(None),
            load_record: Mutex::new(None),
            transport_record: Mutex::new(None),
            loaded_record: Mutex::new(None),
            cancelled_resume: Mutex::new(None),
            retired_probe: Mutex::new(None),
        }
    }
}

impl<P, L, G, N> NativeCoordinator<P, L, G, N>
where
    P: crate::application::native_dispatch::NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + crate::application::native_dispatch::NativeApprovalRepository,
{
    #[expect(
        clippy::too_many_arguments,
        reason = "Keep separately owned protocol, persistence and lifecycle authorities explicit at this composition boundary."
    )]
    pub(crate) fn create<T: Send + 'static>(
        protocol: Arc<P>,
        launcher: Arc<L>,
        configuration: Arc<G>,
        repository: N,
        driver: NativeQueueDriver<T>,
        retain: Arc<dyn RetainPrompt<T, Retained = P::Retained>>,
        environment: Arc<NativeEnvironment<P>>,
        clock: Arc<dyn NodeClock>,
        runtime: RuntimeEpoch,
        claim: Arc<AtomicBool>,
        options: NativeRuntimeOptions,
    ) -> Arc<Self> {
        driver.enable_native_extension();
        let close_progress = Arc::new(super::tasks::CloseProgress::new());
        Arc::new(Self {
            protocol,
            launcher,
            configuration,
            view_factory: OnceLock::new(),
            #[cfg(windows)]
            source_facade: OnceLock::new(),
            #[cfg(windows)]
            source_admission: Mutex::new(()),
            #[cfg(windows)]
            source_sql: Arc::new(super::source_sessions::SourceSqlInventory::default()),
            #[cfg(windows)]
            source_sql_closer: OnceLock::new(),
            repository: Arc::new(repository),
            queue: Arc::new(DriverBridge { driver, retain }),
            environment,
            clock,
            runtime,
            options,
            sources: Mutex::new(Vec::new()),
            capacity_routes: tokio::sync::Mutex::new(HashMap::new()),
            capacity_recycling: tokio::sync::Mutex::new(()),
            capacity_scan_started: AtomicBool::new(false),
            capacity_scan: OnceLock::new(),
            tasks: TaskRegistry::with_progress(close_progress.clone()),
            #[cfg(windows)]
            source_close_tasks: TaskRegistry::new(),
            status: AtomicU8::new(0),
            stop: Notify::new(),
            shutdown: tokio::sync::Mutex::new(()),
            shutdown_operation: Mutex::new(None),
            shutdown_tasks: TaskRegistry::new(),
            close_progress,
            last_shutdown: Mutex::new(None),
            claim,
            workers: Mutex::new(HashMap::new()),
            runs: Mutex::new(HashMap::new()),
        })
    }
    pub(super) fn context(&self) -> OperationContext {
        OperationContext {
            runtime: self.runtime,
            observed_at_ms: self.clock.unix_millis(),
        }
    }
    pub(super) fn deadline(&self) -> Result<RegistryDeadline, DispatchError> {
        Ok(RegistryDeadline::new(NativeRuntimeOptions::at(
            self.options.metadata_wait,
        )?))
    }
    pub(super) fn running(&self) -> Result<(), DispatchError> {
        if self.status.load(Ordering::SeqCst) == 1 {
            Ok(())
        } else {
            Err(DispatchError::NodeStopping)
        }
    }
    pub(super) fn new_id() -> Result<EntityId, DispatchError> {
        random_id().map_err(Into::into)
    }
    pub fn status(&self) -> NativeRuntimeStatus {
        match self.status.load(Ordering::SeqCst) {
            0 => NativeRuntimeStatus::Starting,
            1 => NativeRuntimeStatus::Running,
            2 => NativeRuntimeStatus::Unconfirmed,
            3 => NativeRuntimeStatus::Stopping,
            _ => NativeRuntimeStatus::Stopped,
        }
    }
    pub(crate) fn request_shutdown(&self) {
        self.status.store(3, Ordering::SeqCst);
        self.environment.stopping.store(true, Ordering::SeqCst);
        self.stop.notify_waiters();
        #[cfg(windows)]
        let mut wakes = Vec::new();
        if let Ok(sources) = self.sources.lock() {
            for source in &*sources {
                {
                    let _gate = match source.pre.authority.gate.lock() {
                        Ok(guard) => guard,
                        Err(error) => error.into_inner(),
                    };
                    source.pre.authority.closing.store(true, Ordering::SeqCst);
                    #[cfg(windows)]
                    wakes.extend(source.pre.authority.invalidate_results());
                    source.view_attachment.seal();
                }
                source.changed.notify_waiters();
                if let Ok(targets) = source.targets.lock() {
                    for resource in targets.values() {
                        resource.target.wake_writer();
                        resource.target.changed.notify_waiters();
                    }
                }
            }
        }
        #[cfg(windows)]
        for wake in wakes {
            wake.notify();
        }
    }
    pub(crate) fn cancel(&self) {
        self.status.store(3, Ordering::SeqCst);
        self.environment.stopping.store(true, Ordering::SeqCst);
        self.stop.notify_waiters();
        #[cfg(windows)]
        let mut wakes = Vec::new();
        if let Ok(sources) = self.sources.lock() {
            for source in &*sources {
                {
                    let _gate = match source.pre.authority.gate.lock() {
                        Ok(v) => v,
                        Err(e) => e.into_inner(),
                    };
                    source.pre.authority.closed.store(true, Ordering::SeqCst);
                    #[cfg(windows)]
                    wakes.extend(source.pre.authority.invalidate_results());
                    source.view_attachment.seal();
                    if let Ok(owned) = source.owned.lock()
                        && let Some(owned) = &*owned
                        && let Ok(owner) = owned.try_lock()
                    {
                        owner.request_stop();
                    }
                    if let Ok(io) = source.io.lock()
                        && let Some(io) = &*io
                    {
                        io.request_stop();
                    }
                }
                source.changed.notify_waiters();
                if let Ok(targets) = source.targets.lock() {
                    for resource in targets.values() {
                        resource.target.wake_writer();
                        resource.target.changed.notify_waiters();
                    }
                }
            }
        }
        #[cfg(windows)]
        for wake in wakes {
            wake.notify();
        }
    }
    pub(super) async fn wait_response(
        &self,
        source: &SourceSlot<P, L>,
        operation: &LifecycleOperation<P>,
    ) -> Result<LifecycleResponse, DispatchError> {
        loop {
            let changed = operation.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let stopped = source.changed.notified();
            tokio::pin!(stopped);
            stopped.as_mut().enable();
            let value = operation
                .response
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .clone();
            if let Some(value) = value {
                return Ok(value);
            }
            if source.pre.authority.closed.load(Ordering::SeqCst) {
                return Err(DispatchError::Stopped);
            }
            tokio::select! { () = &mut changed => {}, () = &mut stopped => {} }
        }
    }
}
