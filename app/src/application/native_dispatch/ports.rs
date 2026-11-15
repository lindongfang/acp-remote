//! Inward-facing trusted protocol and process ports. DTOs never implement authority.
use super::{
    Arc, BoxFuture, DispatchError, EntityId, Instant, NativeSessionKey, PreResourceProbe,
    ProtocolVersion, ResourceAuthority, RpcScalar, TransportFact, TransportProgress,
};
use crate::domain::native::ResourceEpoch;
use std::task::{Context, Poll};

#[derive(Clone, Copy, Debug)]
pub enum ResponseWait {
    At(Instant),
    Unbounded,
}
pub struct DispatchIoOptions {
    pub write_deadline: Instant,
    pub response_wait: ResponseWait,
}
pub trait EncodedPromptFrame: Send + 'static {
    /// # Errors
    /// Returns `LengthUnrepresentable` if the complete encoded frame length cannot be represented.
    fn byte_len_checked(&self) -> Result<u64, DispatchError>;
}
pub trait ReservedPrompt: Send + Sync {
    type Scope: Clone + Eq + Send + Sync + 'static;
    type Frame: EncodedPromptFrame;
    type Probe: Clone + Send + Sync + 'static;
    fn scope(&self) -> Self::Scope;
    fn rpc(&self) -> &RpcScalar;
    fn probe(&self) -> Self::Probe;
    /// # Errors
    /// Returns a protocol encoding error while leaving the original reservation owned by the caller.
    fn encode(&self) -> Result<Self::Frame, DispatchError>;
}
pub trait PromptTicket: Send + Sync {
    type Probe: Clone + Send + Sync + 'static;
    fn probe(&self) -> Self::Probe;
    fn progress(&self) -> TransportProgress;
    fn request_cancel(&self);
    fn wait(&self) -> BoxFuture<'_, TransportFact>;
}
pub struct EnqueueRejected<R, F> {
    pub error: DispatchError,
    pub reservation: R,
    pub frame: F,
}
pub enum DispatchIoProgress {
    Accepted(usize),
    Flushed,
}
pub struct DispatchIoError;
pub trait DispatchPollIo {
    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Result<DispatchIoProgress, DispatchIoError>>;
    /// # Errors
    /// Rejects parking after this proxy has already attempted IO or recorded a park outcome.
    fn park(&mut self) -> Result<(), DispatchIoError>;
}
#[derive(Clone, Copy, Debug)]
pub enum DispatchWriteStep {
    Chunk { offset: usize, total: usize },
    Flush { total: usize },
}
#[derive(Clone, Copy, Debug)]
pub enum DispatchFenceRejection {
    Stopped,
    Cancelled,
    StaleTarget,
    RevisionChanged,
    NotReady,
    RequiredStateUnavailable,
}
pub struct BoundWrite<S> {
    pub scope: S,
    pub rpc: RpcScalar,
    pub native: NativeSessionKey,
    pub resource: ResourceEpoch,
    pub config_identity: EntityId,
    pub config_revision: u64,
}
pub trait DispatchWriteBoundary<S>: Send + Sync {
    /// # Errors
    /// Rejects stale ownership, scope or stage before invoking the original IO proxy.
    fn check_and_poll(
        &self,
        write: &BoundWrite<S>,
        step: DispatchWriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn DispatchPollIo,
    ) -> Result<(), DispatchFenceRejection>;
}
pub struct SessionBinding<S> {
    pub(super) scope: S,
    pub(super) native: NativeSessionKey,
    pub(super) target: EntityId,
    pub(super) resource: ResourceEpoch,
    pub(super) instance: EntityId,
    pub(super) owner: EntityId,
    pub(super) config_identity: EntityId,
    pub(super) config_revision: u64,
    pub(super) authority: Arc<ResourceAuthority>,
}
impl<S> SessionBinding<S> {
    #[must_use]
    pub const fn scope(&self) -> &S {
        &self.scope
    }
    #[must_use]
    pub const fn native(&self) -> &NativeSessionKey {
        &self.native
    }
    #[must_use]
    pub const fn target(&self) -> EntityId {
        self.target
    }
    #[must_use]
    pub const fn resource(&self) -> ResourceEpoch {
        self.resource
    }
    #[must_use]
    pub const fn instance(&self) -> EntityId {
        self.instance
    }
    #[must_use]
    pub const fn owner(&self) -> EntityId {
        self.owner
    }
    #[must_use]
    pub const fn configuration(&self) -> (EntityId, u64) {
        (self.config_identity, self.config_revision)
    }
}
pub struct VerifiedLoadedBinding<S, B> {
    pub(super) binding: Arc<SessionBinding<S>>,
    pub(super) setup: Arc<B>,
}
impl<S, B> VerifiedLoadedBinding<S, B> {
    #[must_use]
    pub const fn binding(&self) -> &Arc<SessionBinding<S>> {
        &self.binding
    }
    #[must_use]
    pub fn setup(&self) -> &B {
        &self.setup
    }
}
pub trait NativeProtocolPort: Send + Sync + 'static {
    type Scope: Clone + Eq + Send + Sync + 'static;
    type Retained: Clone + Send + Sync + 'static;
    type Frame: EncodedPromptFrame;
    type Probe: Clone + Send + Sync + 'static;
    type Reservation: ReservedPrompt<Scope = Self::Scope, Frame = Self::Frame, Probe = Self::Probe>;
    type Ticket: PromptTicket<Probe = Self::Probe>;
    /// # Errors
    /// Rejects stale source/resource bindings or a protocol reservation failure; no frame is enqueued.
    fn reserve_prompt(
        &self,
        binding: &SessionBinding<Self::Scope>,
        attempt: EntityId,
        body: Self::Retained,
        response: ResponseWait,
    ) -> Result<Self::Reservation, DispatchError>;
    /// # Errors
    /// Returns the original reservation and frame on rejection; rejection alone is not a transport receipt.
    fn enqueue(
        &self,
        reservation: Self::Reservation,
        frame: Self::Frame,
        boundary: Arc<dyn DispatchWriteBoundary<Self::Scope>>,
        deadline: Instant,
    ) -> Result<Self::Ticket, EnqueueRejected<Self::Reservation, Self::Frame>>;
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleKind {
    Create,
    Discover,
    Initialize,
    Resume,
    Close,
}
pub trait LifecycleReservation: ReservedPrompt {
    fn kind(&self) -> LifecycleKind;
}
pub struct LifecycleWriteIdentity<S> {
    pub scope: S,
    pub rpc: RpcScalar,
    pub kind: LifecycleKind,
}
pub trait LifecycleWriteBoundary<S>: Send + Sync {
    /// # Errors
    /// Rejects stale ownership, scope or stage before invoking the original IO proxy.
    fn check_and_poll(
        &self,
        write: &LifecycleWriteIdentity<S>,
        step: DispatchWriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn DispatchPollIo,
    ) -> Result<(), DispatchFenceRejection>;
}
pub trait NativeIoOwner: Send + Sync + 'static {
    fn request_stop(&self);
    fn shutdown(&self, deadline: Instant) -> BoxFuture<'_, IoShutdownObservation>;
}
#[derive(Clone)]
pub struct IoShutdownObservation {
    pub joined: bool,
    pub outstanding: Vec<TransportProgress>,
    pub unresolved: Vec<RpcScalar>,
    pub failed: bool,
}
pub struct IoAdoptionRejected<I> {
    pub error: DispatchError,
    pub owner: I,
}
pub struct PrepareFailure {
    pub phase: PrepareFailurePhase,
    pub error: DispatchError,
}
#[derive(Clone, Copy, Debug)]
pub enum PrepareFailurePhase {
    BeforeAdopt,
    AfterAdopt,
}
pub struct PreparedProtocol<P: NativeLifecyclePort> {
    pub start: P::Start,
    pub receiver: P::Receiver,
    pub acknowledger: P::Acknowledger,
    pub gate: P::Gate,
}
pub struct ConfigurationKey {
    pub identity: EntityId,
    pub revision: u64,
}
pub struct BoundResourceProbe<S> {
    pub(super) pre: Arc<PreResourceProbe>,
    pub(super) scope: S,
    pub(super) binding: EntityId,
    pub(super) wire: ProtocolVersion,
}
impl<S> BoundResourceProbe<S> {
    #[must_use]
    pub fn resource(&self) -> &PreResourceProbe {
        &self.pre
    }
    #[must_use]
    pub const fn scope(&self) -> &S {
        &self.scope
    }
    #[must_use]
    pub const fn binding(&self) -> EntityId {
        self.binding
    }
    #[must_use]
    pub const fn wire(&self) -> ProtocolVersion {
        self.wire
    }
}
pub struct CurrentTargetCloseBinding<S> {
    pub(super) scope: S,
    pub(super) native: NativeSessionKey,
    pub(super) resource: Arc<PreResourceProbe>,
}
impl<S> CurrentTargetCloseBinding<S> {
    #[must_use]
    pub fn resource(&self) -> &PreResourceProbe {
        &self.resource
    }
    #[must_use]
    pub const fn scope(&self) -> &S {
        &self.scope
    }
    #[must_use]
    pub const fn native(&self) -> &NativeSessionKey {
        &self.native
    }
}
pub struct CancelledLifecycleReservation<S, P> {
    pub(super) scope: S,
    pub(super) rpc: RpcScalar,
    pub(super) probe: P,
    pub(super) kind: LifecycleKind,
}
impl<S, P> CancelledLifecycleReservation<S, P> {
    pub(crate) const fn from_actual(
        scope: S,
        rpc: RpcScalar,
        probe: P,
        kind: LifecycleKind,
    ) -> Self {
        Self {
            scope,
            rpc,
            probe,
            kind,
        }
    }
}
pub struct SourceTerminal<S> {
    pub(super) scope: S,
    pub(super) id: EntityId,
    pub(super) reason: SourceTerminalReason,
}
impl<S> SourceTerminal<S> {
    #[must_use]
    pub const fn scope(&self) -> &S {
        &self.scope
    }
    #[must_use]
    pub const fn id(&self) -> EntityId {
        self.id
    }
    #[must_use]
    pub const fn reason(&self) -> SourceTerminalReason {
        self.reason
    }
}
impl<S> SourceTerminal<S> {
    pub(crate) const fn from_actual(scope: S, id: EntityId, reason: SourceTerminalReason) -> Self {
        Self { scope, id, reason }
    }
}
#[derive(Clone, Copy, Debug)]
pub enum SourceTerminalReason {
    Eof,
    Io,
    Protocol,
    Stopped,
    ObservationFault,
}
pub enum ProtocolRead<L, S> {
    Received(L),
    Terminal(SourceTerminal<S>),
}
pub struct AppliedObservation<S> {
    pub(super) scope: S,
    pub(super) sequence: u64,
    pub(super) receipt: u64,
    #[cfg(windows)]
    pub(super) source_reply_ack: Option<Box<dyn super::source_sessions::SourceReplyAckUse>>,
}
impl<S> AppliedObservation<S> {
    #[must_use]
    pub const fn scope(&self) -> &S {
        &self.scope
    }
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
}
impl<S> AppliedObservation<S> {
    #[must_use]
    pub const fn receipt_order(&self) -> u64 {
        self.receipt
    }

    #[cfg(windows)]
    pub(super) fn source_reply_acknowledged(&mut self) {
        if let Some(ack) = self.source_reply_ack.take() {
            ack.acknowledged();
        }
    }

    #[cfg(windows)]
    pub(super) fn source_reply_ack_rejected(&mut self) {
        if let Some(ack) = self.source_reply_ack.take() {
            ack.rejected();
        }
    }
}
pub struct AckRejected<L> {
    pub error: DispatchError,
    pub lease: L,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationOrigin {
    Live,
    DuringResume,
    DuringReplay,
}
#[derive(Clone)]
pub enum NativeIncoming {
    Created {
        rpc: RpcScalar,
        session: Arc<str>,
    },
    Sessions {
        rpc: RpcScalar,
    },
    Initialized {
        rpc: RpcScalar,
        wire: ProtocolVersion,
    },
    Resumed {
        rpc: RpcScalar,
        session: Arc<str>,
    },
    Closed {
        rpc: RpcScalar,
        session: Arc<str>,
    },
    PromptV1 {
        rpc: RpcScalar,
        reason: Arc<str>,
    },
    PromptV2 {
        rpc: RpcScalar,
        message: Arc<str>,
    },
    RemoteError {
        rpc: RpcScalar,
        code: i64,
    },
    State {
        session: Arc<str>,
        state: Arc<str>,
        reason: Option<Arc<str>>,
        origin: ObservationOrigin,
    },
    Permission {
        session: Arc<str>,
        rpc: RpcScalar,
    },
    Unsupported {
        rpc: RpcScalar,
    },
    OtherResponse {
        rpc: RpcScalar,
    },
    OtherSession {
        session: Arc<str>,
    },
    UnknownNotification,
}
pub struct ObservationView<'a, S, R> {
    pub scope: S,
    pub sequence: u64,
    pub receipt_order: u64,
    pub raw: &'a R,
    pub incoming: NativeIncoming,
}
pub trait NativeLifecyclePort: NativeProtocolPort + Sized {
    type Streams: Send + 'static;
    type Source: Clone + Send + Sync + 'static;
    type Start: Send + 'static;
    type Receiver: Send + 'static;
    type Lease: Send + 'static;
    type Acknowledger: Clone + Send + Sync + 'static;
    type Gate: Clone + Send + Sync + 'static;
    type RawEvent: Clone + Send + Sync + 'static;
    type IoOwner: NativeIoOwner;
    type Setup: Clone + Send + Sync + 'static;
    type BoundSetup: Send + Sync + 'static;
    type InitReservation: LifecycleReservation<Scope = Self::Scope, Frame = Self::Frame, Probe = Self::Probe>;
    type LoadReservation: LifecycleReservation<Scope = Self::Scope, Frame = Self::Frame, Probe = Self::Probe>;
    type CloseReservation: LifecycleReservation<Scope = Self::Scope, Frame = Self::Frame, Probe = Self::Probe>;
    /// # Errors
    /// Returns the exact preparation phase; any adopted IO owner remains with its original supervisor.
    fn prepare_source<A>(
        &self,
        streams: Self::Streams,
        adopt: A,
    ) -> Result<PreparedProtocol<Self>, PrepareFailure>
    where
        A: FnOnce(Self::IoOwner) -> Result<(), IoAdoptionRejected<Self::IoOwner>>;
    fn prepared_scope(&self, start: &Self::Start) -> Self::Scope;
    fn start_source(&self, start: Self::Start) -> Self::Source;
    fn source_scope(&self, source: &Self::Source) -> Self::Scope;
    /// # Errors
    /// Rejects an invalid source or protocol reservation without enqueueing.
    fn reserve_initialize(
        &self,
        source: &Self::Source,
        deadline: Instant,
    ) -> Result<Self::InitReservation, DispatchError>;
    /// # Errors
    /// Returns an invalid-configuration error if either complete setup cannot be compared.
    fn setups_equal(&self, left: &Self::Setup, right: &Self::Setup) -> Result<bool, DispatchError>;
    /// # Errors
    /// Rejects mismatched wire, source or configuration; it does not perform a load.
    fn bind_setup(
        &self,
        source: &Self::Source,
        wire: ProtocolVersion,
        key: &ConfigurationKey,
        setup: Self::Setup,
    ) -> Result<Self::BoundSetup, DispatchError>;
    /// # Errors
    /// Rejects invalid configuration/source context instead of treating unknown fields as empty.
    fn setup_matches(
        &self,
        source: &Self::Source,
        current: &Self::BoundSetup,
        candidate: &Self::Setup,
        key: &ConfigurationKey,
    ) -> Result<bool, DispatchError>;
    /// # Errors
    /// Rejects mismatched source, setup or original resource scope before any enqueue.
    fn reserve_resume(
        &self,
        source: &Self::Source,
        resource: &BoundResourceProbe<Self::Scope>,
        setup: &Self::BoundSetup,
        wait: ResponseWait,
    ) -> Result<Self::LoadReservation, DispatchError>;
    /// # Errors
    /// Returns the original initialize reservation and frame if enqueue is rejected.
    fn enqueue_initialize(
        &self,
        source: &Self::Source,
        reservation: Self::InitReservation,
        frame: Self::Frame,
        boundary: Arc<dyn LifecycleWriteBoundary<Self::Scope>>,
        deadline: Instant,
    ) -> Result<Self::Ticket, EnqueueRejected<Self::InitReservation, Self::Frame>>;
    /// # Errors
    /// Returns the original resume reservation and frame if enqueue is rejected.
    fn enqueue_resume(
        &self,
        source: &Self::Source,
        reservation: Self::LoadReservation,
        frame: Self::Frame,
        boundary: Arc<dyn LifecycleWriteBoundary<Self::Scope>>,
        deadline: Instant,
    ) -> Result<Self::Ticket, EnqueueRejected<Self::LoadReservation, Self::Frame>>;
    /// # Errors
    /// Rejects a stale target/source binding or unsupported native close capability.
    fn reserve_target_close(
        &self,
        source: &Self::Source,
        binding: &CurrentTargetCloseBinding<Self::Scope>,
        wait: ResponseWait,
    ) -> Result<Self::CloseReservation, DispatchError>;
    /// # Errors
    /// Returns the original close reservation and frame on rejection.
    fn enqueue_target_close(
        &self,
        source: &Self::Source,
        reservation: Self::CloseReservation,
        frame: Self::Frame,
        boundary: Arc<dyn LifecycleWriteBoundary<Self::Scope>>,
        deadline: Instant,
    ) -> Result<Self::Ticket, EnqueueRejected<Self::CloseReservation, Self::Frame>>;
    fn next<'a>(
        &'a self,
        source: &'a Self::Source,
        receiver: &'a mut Self::Receiver,
    ) -> BoxFuture<'a, ProtocolRead<Self::Lease, Self::Scope>>;
    /// # Errors
    /// Returns an error for an invalid original lease; this method performs no protocol IO.
    fn view<'a>(
        &self,
        lease: &'a Self::Lease,
    ) -> Result<ObservationView<'a, Self::Scope, Self::RawEvent>, DispatchError>;
    fn cancel_resume(
        &self,
        reservation: Self::LoadReservation,
    ) -> CancelledLifecycleReservation<Self::Scope, Self::Probe>;
    fn probe_progress(&self, probe: &Self::Probe) -> TransportProgress;
    fn fail_observation(
        &self,
        source: &Self::Source,
        lease: Self::Lease,
        error: DispatchError,
    ) -> SourceTerminal<Self::Scope>;
    /// # Errors
    /// Returns the original lease if its applied receipt, source scope or ordering does not match.
    fn acknowledge(
        &self,
        ack: &Self::Acknowledger,
        lease: Self::Lease,
        applied: &AppliedObservation<Self::Scope>,
    ) -> Result<(), AckRejected<Self::Lease>>;
    /// # Errors
    /// Rejects any mismatch in the actual loaded source/resource/configuration proof.
    fn attach_prompt_binding(
        &self,
        source: &Self::Source,
        proof: &VerifiedLoadedBinding<Self::Scope, Self::BoundSetup>,
        gate: &Self::Gate,
    ) -> Result<(), DispatchError>;
}
pub enum ControlIdentity<S> {
    NativeCancel {
        scope: S,
        native_session: Arc<str>,
    },
    PermissionCancelled {
        scope: S,
        native_session: Arc<str>,
        rpc: RpcScalar,
    },
    Unsupported {
        scope: S,
        rpc: RpcScalar,
    },
}
impl<S> ControlIdentity<S> {
    #[must_use]
    pub const fn scope(&self) -> &S {
        match self {
            Self::NativeCancel { scope, .. }
            | Self::PermissionCancelled { scope, .. }
            | Self::Unsupported { scope, .. } => scope,
        }
    }
    #[must_use]
    pub const fn rpc(&self) -> Option<&RpcScalar> {
        match self {
            Self::NativeCancel { .. } => None,
            Self::PermissionCancelled { rpc, .. } | Self::Unsupported { rpc, .. } => Some(rpc),
        }
    }
    #[must_use]
    pub fn native_session(&self) -> Option<&str> {
        match self {
            Self::NativeCancel { native_session, .. }
            | Self::PermissionCancelled { native_session, .. } => Some(native_session),
            Self::Unsupported { .. } => None,
        }
    }
}
pub struct CancelledControlReservation<S, P> {
    pub(super) identity: ControlIdentity<S>,
    pub(super) probe: P,
}
impl<S, P> CancelledControlReservation<S, P> {
    #[must_use]
    pub const fn identity(&self) -> &ControlIdentity<S> {
        &self.identity
    }
    #[must_use]
    pub const fn probe(&self) -> &P {
        &self.probe
    }
}
impl<S, P> CancelledControlReservation<S, P> {
    pub(crate) const fn from_actual(identity: ControlIdentity<S>, probe: P) -> Self {
        Self { identity, probe }
    }
}
pub trait ReservedControl: Send + Sync + 'static {
    type Scope: Clone + Eq + Send + Sync + 'static;
    type Frame: EncodedPromptFrame;
    type Probe: Clone + Send + Sync + 'static;
    fn identity(&self) -> &ControlIdentity<Self::Scope>;
    fn probe(&self) -> Self::Probe;
    /// # Errors
    /// Returns a protocol encoding error while leaving the original reservation owned by the caller.
    fn encode(&self) -> Result<Self::Frame, DispatchError>;
}
pub struct CurrentCancelBinding<S> {
    pub(super) scope: S,
    pub(super) native: NativeSessionKey,
    pub(super) resource: Arc<PreResourceProbe>,
}
pub struct PermissionCancelBinding<S> {
    pub(super) scope: S,
    pub(super) native: NativeSessionKey,
    pub(super) rpc: RpcScalar,
    pub(super) resource: Arc<PreResourceProbe>,
}
pub struct UnsupportedReplyBinding<S> {
    pub(super) scope: S,
    pub(super) rpc: RpcScalar,
}
pub struct SourcePermissionCancelBinding<S> {
    pub(super) scope: S,
    pub(super) rpc: RpcScalar,
    pub(super) source: Arc<super::PreSourceProbe>,
    pub(super) intent_reference: EntityId,
}
impl<S> SourcePermissionCancelBinding<S> {
    #[must_use]
    pub const fn scope(&self) -> &S {
        &self.scope
    }
    #[must_use]
    pub const fn rpc(&self) -> &RpcScalar {
        &self.rpc
    }
    #[must_use]
    pub fn source(&self) -> &super::PreSourceProbe {
        &self.source
    }
    #[must_use]
    pub const fn intent_reference(&self) -> EntityId {
        self.intent_reference
    }
}
impl<S> CurrentCancelBinding<S> {
    #[must_use]
    pub fn resource(&self) -> &PreResourceProbe {
        &self.resource
    }
    #[must_use]
    pub const fn scope(&self) -> &S {
        &self.scope
    }
    #[must_use]
    pub const fn native(&self) -> &NativeSessionKey {
        &self.native
    }
}
impl<S> PermissionCancelBinding<S> {
    #[must_use]
    pub fn resource(&self) -> &PreResourceProbe {
        &self.resource
    }
    #[must_use]
    pub const fn scope(&self) -> &S {
        &self.scope
    }
    #[must_use]
    pub const fn native(&self) -> &NativeSessionKey {
        &self.native
    }
    #[must_use]
    pub const fn rpc(&self) -> &RpcScalar {
        &self.rpc
    }
}
impl<S> UnsupportedReplyBinding<S> {
    #[must_use]
    pub const fn scope(&self) -> &S {
        &self.scope
    }
    #[must_use]
    pub const fn rpc(&self) -> &RpcScalar {
        &self.rpc
    }
}
pub struct PermissionIdentity<S> {
    pub scope: S,
    pub rpc: RpcScalar,
    pub native_session: Arc<str>,
}
pub struct UnsupportedIdentity<S> {
    pub scope: S,
    pub rpc: RpcScalar,
}
pub trait ControlWriteBoundary<S>: Send + Sync {
    /// # Errors
    /// Rejects stale ownership, scope or stage before invoking the original IO proxy.
    fn check_and_poll(
        &self,
        identity: &ControlIdentity<S>,
        step: DispatchWriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn DispatchPollIo,
    ) -> Result<(), DispatchFenceRejection>;
}
pub trait NativeControlPort: NativeLifecyclePort {
    type PermissionRequest: Clone + Send + Sync + 'static;
    type UnsupportedRequest: Clone + Send + Sync + 'static;
    type ControlReservation: ReservedControl<Scope = Self::Scope, Frame = Self::Frame, Probe = Self::Probe>;
    /// # Errors
    /// Rejects an invalid original lease; an unrelated event yields None without minting a request.
    fn permission_from_lease(
        &self,
        lease: &Self::Lease,
    ) -> Result<Option<Self::PermissionRequest>, DispatchError>;
    /// # Errors
    /// Rejects an invalid original lease; an unrelated event yields None without minting a request.
    fn unsupported_from_lease(
        &self,
        lease: &Self::Lease,
    ) -> Result<Option<Self::UnsupportedRequest>, DispatchError>;
    /// # Errors
    /// Rejects a request without its original valid source scope.
    fn permission_identity(
        &self,
        request: &Self::PermissionRequest,
    ) -> Result<PermissionIdentity<Self::Scope>, DispatchError>;
    fn unsupported_identity(
        &self,
        request: &Self::UnsupportedRequest,
    ) -> UnsupportedIdentity<Self::Scope>;
    /// # Errors
    /// Rejects stale native resource scope; the returned reservation is not a native cancellation result.
    fn reserve_native_cancel(
        &self,
        source: &Self::Source,
        binding: &CurrentCancelBinding<Self::Scope>,
    ) -> Result<Self::ControlReservation, DispatchError>;
    /// # Errors
    /// Rejects stale request scope, original pending-response state or duplicate response reservation.
    fn reserve_permission_cancelled(
        &self,
        source: &Self::Source,
        request: &Self::PermissionRequest,
        binding: &PermissionCancelBinding<Self::Scope>,
    ) -> Result<Self::ControlReservation, DispatchError>;
    /// # Errors
    /// Rejects a stale source, original pending request or mismatched whole-source closing binding.
    fn reserve_source_permission_cancelled(
        &self,
        source: &Self::Source,
        original: &Self::PermissionRequest,
        binding: &SourcePermissionCancelBinding<Self::Scope>,
    ) -> Result<Self::ControlReservation, DispatchError>;
    /// # Errors
    /// Rejects a stale original request or an already reserved/spent response slot.
    fn reserve_unsupported_reply(
        &self,
        source: &Self::Source,
        request: &Self::UnsupportedRequest,
        binding: &UnsupportedReplyBinding<Self::Scope>,
    ) -> Result<Self::ControlReservation, DispatchError>;
    /// # Errors
    /// Returns the original control reservation and frame on rejection.
    fn enqueue_control(
        &self,
        source: &Self::Source,
        reservation: Self::ControlReservation,
        frame: Self::Frame,
        boundary: Arc<dyn ControlWriteBoundary<Self::Scope>>,
        deadline: Instant,
    ) -> Result<Self::Ticket, EnqueueRejected<Self::ControlReservation, Self::Frame>>;
    fn cancel_reserved_control(
        &self,
        reservation: Self::ControlReservation,
    ) -> CancelledControlReservation<Self::Scope, Self::Probe>;
}
