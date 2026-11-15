#![allow(
    clippy::significant_drop_tightening,
    reason = "Original-operation and capture registration are published together before enqueue; no guard spans IO or an async wait"
)]
//! Test-owned source assembly. Its bindings cannot be converted to Native resource proofs.
use super::{
    Connection, ConnectionScope, FenceRejection, Incoming, OutboundKind, PollIo, RequestKind,
    Reservation, Response, ResponseDeadline, RpcId, SessionId, SessionSetup, WriteFence,
    WriteIdentity, WriteStep, WriteTicket,
    shared_view_capture::{AcpViewCapture, RetainedAcpRaw},
    shared_view_projection::AcpViewProjector,
    shared_view_read::AcpReplayRead,
};
use crate::{
    application::shared_views::local_authority::AuthenticatedLocalViewClient,
    application::shared_views::{
        SharedViewOwner, SharedViews,
        budget::{
            MonotonicReadOptionsProvider, NativeReadOptions, NativeReadOptionsProvider,
            NativeReadStartContext, ReadResponseBudget, ReadResponseWait,
        },
        read_control::{NativeReadCoordinator, NativeReadRequest, ReadCompletionProof, RpcScalar},
        reader::ViewReadService,
    },
    domain::{
        native::{NativeSessionKey, NativeSourceId},
        shared_view::{ReadEpoch, ViewError},
    },
    shared_view_test_support::{
        FixtureOwnerHandle, FixturePeerWire, FixtureResourceLease, LocalFixtureViewAuthority,
        LocalViewAuthFixture,
    },
};
use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::Context,
    time::Duration,
};
use tokio::{
    sync::{Notify, watch},
    time::{Instant, timeout_at},
};

pub struct FixtureSourceConfig {
    pub source: NativeSourceId,
    pub entry_alias: Arc<str>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixtureTargetOrigin {
    ModuleFixture,
}
pub struct FixtureTarget {
    key: NativeSessionKey,
    scope: ConnectionScope,
    rpc: RpcId,
}
impl FixtureTarget {
    #[must_use]
    pub const fn key(&self) -> &NativeSessionKey {
        &self.key
    }
    #[must_use]
    pub fn scope(&self) -> ConnectionScope {
        self.scope.clone()
    }
    #[must_use]
    pub const fn origin(&self) -> FixtureTargetOrigin {
        FixtureTargetOrigin::ModuleFixture
    }
    #[must_use]
    pub const fn original_rpc(&self) -> &RpcId {
        &self.rpc
    }
}
#[derive(Clone)]
enum OperationResult {
    Initialized,
    Target(NativeSessionKey),
    Replayed,
}
struct Operation {
    rpc: RpcId,
    kind: RequestKind,
    key: Option<NativeSessionKey>,
    replay: Option<AcpReplayRead>,
    completed: watch::Sender<Option<Result<OperationResult, ViewError>>>,
    completion_marked: bool,
    ticket: Option<Arc<WriteTicket>>,
    proof: Option<Arc<ReadCompletionProof<ConnectionScope, RetainedAcpRaw>>>,
}
struct ModuleCore {
    connection: Connection,
    source: NativeSourceId,
    capture: Mutex<AcpViewCapture>,
    operations: Mutex<Vec<Operation>>,
    terminal: AtomicBool,
    pump_joined: AtomicBool,
    changed: Notify,
    enqueues: AtomicU64,
    targets: RwLock<HashMap<NativeSessionKey, SessionSetup>>,
}
pub struct AcpViewModuleFixture {
    core: Arc<ModuleCore>,
    owner: FixtureOwnerHandle,
    lease: FixtureResourceLease,
    view_owner: SharedViewOwner<ConnectionScope, RetainedAcpRaw>,
    reads: Arc<
        ViewReadService<
            ConnectionScope,
            RetainedAcpRaw,
            LocalFixtureViewAuthority,
            AuthenticatedLocalViewClient,
        >,
    >,
    native_reads: Arc<NativeReadCoordinator<history::ModuleHistoryBridge>>,
    #[cfg(test)]
    policy: Arc<ModuleReadPolicy>,
    auth: Arc<LocalViewAuthFixture>,
}
struct ModuleReadPolicy {
    inner: MonotonicReadOptionsProvider,
    calls: AtomicU64,
    #[cfg(test)]
    last_reason: std::sync::atomic::AtomicU8,
}
impl NativeReadOptionsProvider for ModuleReadPolicy {
    fn for_new_operation(
        &self,
        context: &NativeReadStartContext<'_>,
    ) -> Result<NativeReadOptions, ViewError> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        #[cfg(test)]
        self.last_reason.store(
            match context.reason() {
                crate::application::shared_views::budget::ReadReason::ExplicitRefresh => 1,
                _ => 2,
            },
            Ordering::Release,
        );
        self.inner.for_new_operation(context)
    }
}
pub struct FixtureShutdownReport {
    io: bool,
    pump: bool,
    view: bool,
    failure: Option<ViewError>,
}
impl FixtureShutdownReport {
    #[must_use]
    pub const fn io_joined(&self) -> bool {
        self.io
    }
    #[must_use]
    pub const fn pump_joined(&self) -> bool {
        self.pump
    }
    #[must_use]
    pub const fn views_joined(&self) -> bool {
        self.view
    }
    #[must_use]
    pub const fn failure(&self) -> Option<ViewError> {
        self.failure
    }
}
struct ModuleWriteFence {
    scope: ConnectionScope,
    rpc: RpcId,
}
impl WriteFence for ModuleWriteFence {
    fn check_and_poll(
        &self,
        identity: &WriteIdentity,
        _step: WriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn PollIo,
    ) -> Result<(), FenceRejection> {
        if identity.scope() != self.scope || identity.id() != Some(&self.rpc) {
            return Err(FenceRejection::StaleTarget);
        }
        let _ = io.poll(cx);
        Ok(())
    }
}
impl ModuleCore {
    fn register(
        &self,
        reservation: &Reservation,
        key: Option<NativeSessionKey>,
        read: Option<ReadEpoch>,
    ) -> Result<watch::Receiver<Option<Result<OperationResult, ViewError>>>, ViewError> {
        if reservation.scope() != self.connection.scope() || self.terminal.load(Ordering::Acquire) {
            return Err(ViewError::SourceReleased);
        }
        let OutboundKind::Request(kind) = reservation.kind() else {
            return Err(ViewError::InvalidStage);
        };
        let rpc = reservation.id().ok_or(ViewError::ProtocolFailure)?.clone();
        let mut operations = self.operations.lock().map_err(|_| ViewError::ReadFailure)?;
        operations
            .try_reserve(1)
            .map_err(|_| ViewError::AllocationFailure)?;
        let mut capture = self.capture.lock().map_err(|_| ViewError::ReadFailure)?;
        let replay = match kind {
            RequestKind::NewSession => {
                capture.register_module_create(reservation, self.source)?;
                None
            }
            RequestKind::Resume(_) => {
                capture.bind_module_reservation(
                    reservation,
                    key.clone().ok_or(ViewError::InvalidTarget)?,
                    None,
                )?;
                None
            }
            RequestKind::Replay(_) => {
                let read = read.ok_or(ViewError::InvalidStage)?;
                let replay = capture.track_replay(reservation, read)?;
                capture.bind_module_reservation(
                    reservation,
                    key.clone().ok_or(ViewError::InvalidTarget)?,
                    Some(read),
                )?;
                Some(replay)
            }
            RequestKind::Initialize(_) => None,
            _ => return Err(ViewError::Unsupported),
        };
        let (completed, receiver) = watch::channel(None);
        operations.push(Operation {
            rpc,
            kind: kind.clone(),
            key,
            replay,
            completed,
            completion_marked: false,
            ticket: None,
            proof: None,
        });
        Ok(receiver)
    }
    fn receive(&self, event: &super::Received) -> Result<(), ViewError> {
        // This fixture is the one actual receiver owner. Capture failure is display-only:
        // a legitimate response is still observed rather than withdrawn by a view fault.
        let mut operations = self.operations.lock().map_err(|_| ViewError::ReadFailure)?;
        let mut capture = self.capture.lock().map_err(|_| ViewError::ReadFailure)?;
        let _ = capture.capture_received(event);
        let created = capture.resolve_module_created(event);
        for operation in operations.iter_mut() {
            let read_observed = if let Some(replay) = operation.replay.as_mut() {
                match replay.observe_received(event) {
                    Ok(observed) => observed,
                    Err(error) => {
                        operation.completed.send_replace(Some(Err(error)));
                        false
                    }
                }
            } else {
                false
            };
            if let Some(replay) = operation.replay.as_mut()
                && read_observed
                && !operation.completion_marked
                && replay.response_boundary().is_some()
                && let Some(key) = operation.key.clone()
            {
                if operation.proof.is_none() {
                    let boundary = replay.response_boundary().ok_or(ViewError::ReadFailure)?;
                    operation.proof = Some(Arc::new(ReadCompletionProof {
                        scope: boundary.scope,
                        read: boundary.epoch,
                        target: key.clone(),
                        rpc: match &operation.rpc {
                            RpcId::Integer(value) => RpcScalar::Integer(*value),
                            RpcId::String(value) => RpcScalar::String(Arc::clone(value)),
                        },
                        sequence: boundary.sequence,
                        order: boundary.receipt_order,
                        _retained: RetainedAcpRaw::from_received(event),
                    }));
                }
                match capture.complete_module_read(key.clone(), replay) {
                    Ok(_) => operation.completion_marked = true,
                    Err(error) => {
                        let _=capture.fail_module_read(&key,replay,error,crate::application::shared_views::ingress::ReadFailureEvidence::Response{raw:RetainedAcpRaw::from_received(event),sequence:event.sequence(),order:event.receipt_order()});
                        operation.completion_marked = true;
                        operation.completed.send_replace(Some(Err(error)));
                    }
                }
            }
            let Incoming::Response { id, response, .. } = event.incoming() else {
                continue;
            };
            if *id != operation.rpc {
                continue;
            }
            if matches!(response, Response::RemoteError { .. })
                && !operation.completion_marked
                && let (Some(replay), Some(key)) = (&operation.replay, &operation.key)
            {
                let _ = capture.fail_module_read(
                    key,
                    replay,
                    ViewError::ReadFailure,
                    crate::application::shared_views::ingress::ReadFailureEvidence::Response {
                        raw: RetainedAcpRaw::from_received(event),
                        sequence: event.sequence(),
                        order: event.receipt_order(),
                    },
                );
                operation.completion_marked = true;
            }
            let result = match (&operation.kind, response) {
                (RequestKind::Initialize(_), Response::Initialized(_)) => {
                    Ok(OperationResult::Initialized)
                }
                (RequestKind::NewSession, Response::NewSession(_)) => match &created {
                    Ok(Some((rpc, key))) if *rpc == operation.rpc => {
                        Ok(OperationResult::Target(key.clone()))
                    }
                    Ok(_) => Err(ViewError::ProtocolFailure),
                    Err(error) => Err(*error),
                },
                (RequestKind::Resume(session), Response::Resumed { session: actual })
                    if session == actual =>
                {
                    operation
                        .key
                        .clone()
                        .map(OperationResult::Target)
                        .ok_or(ViewError::InvalidTarget)
                }
                (RequestKind::Replay(session), Response::Replayed { session: actual })
                    if session == actual =>
                {
                    Ok(OperationResult::Replayed)
                }
                (_, Response::RemoteError { .. }) => Err(ViewError::ReadFailure),
                _ => Err(ViewError::ProtocolFailure),
            };
            operation.completed.send_replace(Some(result));
        }
        self.changed.notify_waiters();
        Ok(())
    }
    fn finish(&self) {
        self.terminal.store(true, Ordering::Release);
        if let (Ok(mut operations), Ok(mut capture)) = (self.operations.lock(), self.capture.lock())
        {
            for operation in operations.iter_mut() {
                if !operation.completion_marked
                    && let (Some(replay), Some(key)) = (&operation.replay, &operation.key)
                {
                    let _ = capture.fail_module_read(
                        key,
                        replay,
                        ViewError::SourceReleased,
                        crate::application::shared_views::ingress::ReadFailureEvidence::Lifecycle(
                            Arc::new(self.connection.clone()),
                        ),
                    );
                    operation.completion_marked = true;
                }
                if operation.completed.borrow().is_none() {
                    operation
                        .completed
                        .send_replace(Some(Err(ViewError::SourceReleased)));
                }
            }
            capture.seal();
        }
        self.changed.notify_waiters();
    }
    fn observe_write_terminal(&self, rpc: &RpcId, report: super::WriteReport) {
        if matches!(
            report,
            super::WriteReport::Zero { .. } | super::WriteReport::Unknown { .. }
        ) && let (Ok(mut operations), Ok(mut capture)) =
            (self.operations.lock(), self.capture.lock())
            && let Some(operation) = operations
                .iter_mut()
                .find(|operation| &operation.rpc == rpc && !operation.completion_marked)
            && let (Some(replay), Some(key)) = (&mut operation.replay, &operation.key)
        {
            let _ = replay.observation();
            let _ = capture.fail_module_read(
                key,
                replay,
                ViewError::TransportFailure,
                crate::application::shared_views::ingress::ReadFailureEvidence::Transport(
                    Arc::new(replay.original_write_probe()),
                ),
            );
            operation.completion_marked = true;
        }
        self.changed.notify_waiters();
    }
}
impl AcpViewModuleFixture {
    /// # Errors
    /// Uses only the adopted original IO and real pairing authority; failures retain IO ownership.
    pub async fn start(
        config: FixtureSourceConfig,
        wire: FixturePeerWire,
        auth: Arc<LocalViewAuthFixture>,
    ) -> Result<Self, ViewError> {
        Self::start_inner(config, wire, auth, None).await
    }
    /// # Errors
    /// Selects only the operation-local IO options provider before original assembly starts.
    pub async fn start_with_read_options_provider(
        config: FixtureSourceConfig,
        wire: FixturePeerWire,
        auth: Arc<LocalViewAuthFixture>,
        provider: Arc<dyn NativeReadOptionsProvider>,
    ) -> Result<Self, ViewError> {
        Self::start_inner(config, wire, auth, Some(provider)).await
    }
    #[allow(
        clippy::too_many_lines,
        reason = "The real source/IO/pump/view owners are registered before initialization can start; keep the handoff order explicit"
    )]
    async fn start_inner(
        config: FixtureSourceConfig,
        mut wire: FixturePeerWire,
        auth: Arc<LocalViewAuthFixture>,
        options: Option<Arc<dyn NativeReadOptionsProvider>>,
    ) -> Result<Self, ViewError> {
        if config.entry_alias.is_empty() {
            return Err(ViewError::InvalidConfiguration);
        }
        let owner = wire.resource_owner();
        let (node, lease) = wire.take_node().map_err(|_| ViewError::ReadFailure)?;
        let (reader, writer) = tokio::io::split(node);
        let (connection, mut events, io) =
            Connection::spawn(reader, writer).map_err(|_| ViewError::ProtocolFailure)?;
        lease
            .install_io(io)
            .map_err(|_| ViewError::ShutdownUnconfirmed)?;
        let (mut capture, consumer, probe) = AcpViewCapture::for_connection(&connection);
        let view_owner = SharedViewOwner::start_source(
            connection.scope(),
            config.source,
            consumer,
            probe,
            Arc::new(AcpViewProjector::new(connection.clone())),
            NonZeroUsize::new(64).ok_or(ViewError::InvalidConfiguration)?,
            tokio::runtime::Handle::current(),
        )?;
        let authority = auth.authority();
        capture.install_controller(view_owner.test_controller())?;
        capture.install_source_seal(&view_owner.handle())?;
        let mut reads = ViewReadService::new(
            view_owner.handle(),
            Arc::clone(&authority),
            view_owner.supervisor.clone(),
        );
        let core = Arc::new(ModuleCore {
            connection,
            source: config.source,
            capture: Mutex::new(capture),
            operations: Mutex::new(Vec::new()),
            terminal: AtomicBool::new(false),
            pump_joined: AtomicBool::new(false),
            changed: Notify::new(),
            enqueues: AtomicU64::new(0),
            targets: RwLock::new(HashMap::new()),
        });
        let pump = Arc::clone(&core);
        owner
            .spawn(async move {
                let result = loop {
                    match events.next().await {
                        Ok(Some(event)) => {
                            if let Err(error) = pump.receive(&event) {
                                break Err(error);
                            }
                        }
                        Ok(None) => break Ok(()),
                        Err(_) => break Err(ViewError::ProtocolFailure),
                    }
                };
                pump.finish();
                pump.pump_joined.store(true, Ordering::Release);
                pump.changed.notify_waiters();
                result.map_err(|_| crate::shared_view_test_support::FixtureError::Io)
            })
            .map_err(|_| ViewError::ShutdownUnconfirmed)?;
        let policy = Arc::new(ModuleReadPolicy {
            inner: MonotonicReadOptionsProvider::new(
                Duration::from_secs(30),
                ReadResponseBudget::Unbounded,
            ),
            calls: AtomicU64::new(0),
            #[cfg(test)]
            last_reason: std::sync::atomic::AtomicU8::new(0),
        });
        let bridge = Arc::new(history::ModuleHistoryBridge {
            core: Arc::clone(&core),
            owner: owner.clone(),
            views: view_owner.handle(),
        });
        let native_reads = NativeReadCoordinator::new(
            bridge,
            view_owner.handle(),
            view_owner.supervisor.clone(),
            options.unwrap_or_else(|| policy.clone()),
        )?;
        reads.set_read_control(native_reads.clone());
        let module = Self {
            core,
            owner,
            lease,
            view_owner,
            reads: Arc::new(reads),
            native_reads,
            #[cfg(test)]
            policy,
            auth,
        };
        let deadline = Instant::now()
            .checked_add(Duration::from_secs(30))
            .ok_or(ViewError::ReadFailure)?;
        let reservation = module
            .core
            .connection
            .reserve_preferred_initialize(deadline)
            .map_err(|_| ViewError::ProtocolFailure)?;
        let (_, result) = module.submit(reservation, None, None, deadline).await?;
        if !matches!(result, OperationResult::Initialized) {
            return Err(ViewError::ProtocolFailure);
        }
        authority.register_module_source(&module.view_owner.handle())?;
        Ok(module)
    }
    async fn submit(
        &self,
        reservation: Reservation,
        key: Option<NativeSessionKey>,
        read: Option<ReadEpoch>,
        deadline: Instant,
    ) -> Result<(RpcId, OperationResult), ViewError> {
        let (rpc, mut result) =
            enqueue_owned(&self.core, &self.owner, reservation, key, read, deadline)?;
        let outcome = timeout_at(deadline, async {
            loop {
                let completed = result.borrow().clone();
                if let Some(result) = completed {
                    return result;
                }
                result
                    .changed()
                    .await
                    .map_err(|_| ViewError::SourceReleased)?;
            }
        })
        .await
        .map_err(|_| ViewError::ReadFailure)??;
        Ok((rpc, outcome))
    }
    /// # Errors
    /// Binds only the actual Resume reservation and its real response on this connection.
    pub async fn resume_existing(
        &self,
        session: SessionId,
        setup: SessionSetup,
    ) -> Result<FixtureTarget, ViewError> {
        let key = NativeSessionKey::from_config(Some(self.core.source.0), session.as_str())
            .map_err(|_| ViewError::InvalidTarget)?;
        let request = self
            .core
            .connection
            .strategy()
            .ok_or(ViewError::ProtocolFailure)?
            .resume(session, &setup)
            .map_err(|_| ViewError::Unsupported)?;
        let reservation = self
            .core
            .connection
            .reserve_request(request, ResponseDeadline::Unbounded)
            .map_err(|_| ViewError::ReadFailure)?;
        let deadline = Instant::now()
            .checked_add(Duration::from_secs(30))
            .ok_or(ViewError::ReadFailure)?;
        let (rpc, outcome) = self.submit(reservation, Some(key), None, deadline).await?;
        let OperationResult::Target(key) = outcome else {
            return Err(ViewError::ProtocolFailure);
        };
        self.remember_setup(&key, setup)?;
        Ok(FixtureTarget {
            key,
            scope: self.scope(),
            rpc,
        })
    }
    /// # Errors
    /// No session identity exists until the original creation receives its real typed response.
    pub async fn create(&self, setup: SessionSetup) -> Result<FixtureTarget, ViewError> {
        let request = self
            .core
            .connection
            .strategy()
            .ok_or(ViewError::ProtocolFailure)?
            .new_session(&setup)
            .map_err(|_| ViewError::Unsupported)?;
        let reservation = self
            .core
            .connection
            .reserve_request(request, ResponseDeadline::Unbounded)
            .map_err(|_| ViewError::ReadFailure)?;
        let deadline = Instant::now()
            .checked_add(Duration::from_secs(30))
            .ok_or(ViewError::ReadFailure)?;
        let (rpc, outcome) = self.submit(reservation, None, None, deadline).await?;
        let OperationResult::Target(key) = outcome else {
            return Err(ViewError::ProtocolFailure);
        };
        self.remember_setup(&key, setup)?;
        Ok(FixtureTarget {
            key,
            scope: self.scope(),
            rpc,
        })
    }
    fn remember_setup(&self, key: &NativeSessionKey, setup: SessionSetup) -> Result<(), ViewError> {
        let mut targets = self
            .core
            .targets
            .write()
            .map_err(|_| ViewError::ReadFailure)?;
        targets
            .try_reserve(1)
            .map_err(|_| ViewError::AllocationFailure)?;
        targets.insert(key.clone(), setup);
        Ok(())
    }
    #[must_use]
    pub fn scope(&self) -> ConnectionScope {
        self.core.connection.scope()
    }
    #[must_use]
    pub fn resource_owner(&self) -> FixtureOwnerHandle {
        self.owner.clone()
    }
    #[must_use]
    pub fn views(&self) -> SharedViews<ConnectionScope, RetainedAcpRaw> {
        self.view_owner.handle()
    }
    #[must_use]
    pub fn probe(
        &self,
    ) -> crate::application::shared_views::probe::SharedViewProbe<ConnectionScope> {
        self.view_owner.test_probe()
    }
    #[must_use]
    pub fn test_controller(
        &self,
    ) -> crate::application::shared_views::controller::ViewTestController<ConnectionScope> {
        self.view_owner.test_controller()
    }
    #[must_use]
    pub fn reads(
        &self,
    ) -> &ViewReadService<
        ConnectionScope,
        RetainedAcpRaw,
        LocalFixtureViewAuthority,
        AuthenticatedLocalViewClient,
    > {
        &self.reads
    }
    /// # Errors
    /// Rejects foreign targets, changed configuration or an occupied original operation.
    #[allow(
        clippy::unused_async_trait_impl,
        clippy::unused_async,
        reason = "Frozen A6 async signature; the actual operation executes in its already registered owner"
    )]
    pub async fn replay(
        &self,
        target: &FixtureTarget,
        setup: SessionSetup,
        options: NativeReadOptions,
    ) -> Result<NativeReadRequest, ViewError> {
        if target.scope != self.scope() || target.key.source() != self.core.source {
            return Err(ViewError::InvalidScope);
        }
        let targets = self
            .core
            .targets
            .read()
            .map_err(|_| ViewError::ReadFailure)?;
        let original = targets.get(&target.key).ok_or(ViewError::InvalidTarget)?;
        if original.source().reference != setup.source().reference
            || original.source().revision != setup.source().revision
            || original.wire() != setup.wire()
        {
            return Err(ViewError::InvalidConfiguration);
        }
        let session_raw = super::raw_json::RawDocument::parse(
            serde_json::to_vec(target.key.native_id()).map_err(|_| ViewError::AllocationFailure)?,
        )
        .map_err(|_| ViewError::InvalidTarget)?;
        let session = SessionId::parse(session_raw.root()).map_err(|_| ViewError::InvalidTarget)?;
        let strategy = self
            .core
            .connection
            .strategy()
            .ok_or(ViewError::ProtocolFailure)?;
        let current = strategy
            .replay(session.clone(), original)
            .map_err(super::shared_view_read::request_error)?;
        let supplied = strategy
            .replay(session, &setup)
            .map_err(super::shared_view_read::request_error)?;
        if current.params().fragment() != supplied.params().fragment() {
            return Err(ViewError::InvalidConfiguration);
        }
        drop(targets);
        self.native_reads.request_selected(&target.key, options)
    }
    pub async fn shutdown(&self, deadline: Instant) -> FixtureShutdownReport {
        self.core.finish();
        let io = self.lease.shutdown(deadline).await.all_joined();
        let pump = timeout_at(deadline, async {
            loop {
                let changed = self.core.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if self.core.pump_joined.load(Ordering::Acquire) {
                    return;
                }
                changed.await;
            }
        })
        .await
        .is_ok();
        let view = self.view_owner.shutdown(deadline).await;
        let joined = view.worker_joined() && view.readers_joined() && view.cleanup_joined();
        FixtureShutdownReport {
            io,
            pump,
            view: joined,
            failure: view
                .failure()
                .or_else(|| (!io || !pump || !joined).then_some(ViewError::ShutdownUnconfirmed)),
        }
    }
}
impl Drop for AcpViewModuleFixture {
    fn drop(&mut self) {
        self.core.finish();
    }
}
type OperationReceiver = watch::Receiver<Option<Result<OperationResult, ViewError>>>;
fn enqueue_owned(
    core: &Arc<ModuleCore>,
    owner: &FixtureOwnerHandle,
    reservation: Reservation,
    key: Option<NativeSessionKey>,
    read: Option<ReadEpoch>,
    deadline: Instant,
) -> Result<(RpcId, OperationReceiver), ViewError> {
    let observed_rpc = reservation.id().ok_or(ViewError::ProtocolFailure)?.clone();
    let (transfer, adopted) = tokio::sync::oneshot::channel::<Arc<WriteTicket>>();
    let original = Arc::clone(core);
    owner
        .spawn(async move {
            if let Ok(ticket) = adopted.await {
                let report = ticket.wait().await;
                original.observe_write_terminal(&observed_rpc, report);
            }
            Ok(())
        })
        .map_err(|_| ViewError::ShutdownUnconfirmed)?;
    let frame = reservation
        .encode()
        .map_err(|_| ViewError::ProtocolFailure)?;
    let rpc = reservation.id().ok_or(ViewError::ProtocolFailure)?.clone();
    let result = core.register(&reservation, key, read)?;
    let ticket = Arc::new(
        core.connection
            .enqueue(
                reservation,
                frame,
                Arc::new(ModuleWriteFence {
                    scope: core.connection.scope(),
                    rpc: rpc.clone(),
                }),
                deadline,
            )
            .map_err(|_| ViewError::QueueRejected)?,
    );
    {
        let mut operations = core.operations.lock().map_err(|_| ViewError::ReadFailure)?;
        let operation = operations
            .iter_mut()
            .find(|operation| operation.rpc == rpc)
            .ok_or(ViewError::InvalidStage)?;
        operation.ticket = Some(Arc::clone(&ticket));
    }
    core.enqueues.fetch_add(1, Ordering::AcqRel);
    let _ = transfer.send(ticket);
    Ok((rpc, result))
}
#[cfg(test)]
mod controller_tests;
mod history;
#[cfg(test)]
mod tests;
mod transfer;
pub use transfer::{CollectedModuleRecord, ModuleOriginalReadProgress, ModuleViewTransfer};
