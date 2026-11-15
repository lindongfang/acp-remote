//! Concrete native ports. Every effect uses the same actual ACP connection and writer.
use super::{
    AcpError, Connection, ConnectionScope, EncodedFrame, ExtensionFields, FenceRejection, Incoming,
    IoAdoptionError, IoOwner, IoProgress, Known, McpServer, ObservationAcknowledger,
    ObservationLease, ObservationReceiver, ObservedPreparePhase, ObservedStart, ObservedWriteGate,
    OutboundKind, PermissionRequest, PermissionSelection, PollIo, PromptResult, RawMessage,
    RequestKind, Reservation, Response, ResponseDeadline, RpcId, SessionEvent, SessionId,
    SessionSetup, SetupSnapshot, UnsupportedRequest, UpdateOrigin, Version, WriteFailure,
    WriteFence, WriteIdentity, WriteProbe, WriteProgress, WriteReport, WriteStep, WriteTicket,
    ZeroReason, raw_json, raw_model,
};
use crate::{
    application::native_dispatch as n,
    domain::{EntityId, native::NativeSessionKey, native_dispatch as d},
};
use std::{
    collections::HashMap,
    marker::PhantomData,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};
use tokio::time::Instant;
#[cfg(windows)]
#[path = "source_sessions.rs"]
mod source_sessions;
#[cfg(windows)]
pub(crate) use source_sessions::AcpSourceResult;
mod approval_reply;
pub use approval_reply::AcpPermissionReplyReservation;

const fn error(e: AcpError) -> d::DispatchError {
    match e {
        AcpError::InvalidRequestScope => d::DispatchError::InvalidScope,
        AcpError::Closed | AcpError::Cancelled => d::DispatchError::Stopped,
        AcpError::ConfigurationIncomplete
        | AcpError::ConfigurationVersionMismatch
        | AcpError::UnencodableConfiguration => d::DispatchError::InvalidConfiguration,
        AcpError::NotInitialized => {
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::NotInitialized)
        }
        AcpError::AlreadyInitialized => {
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::AlreadyInitialized)
        }
        AcpError::InvalidMessage => {
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::InvalidMessage)
        }
        AcpError::IncompatibleVersion => {
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::IncompatibleVersion)
        }
        AcpError::UnsupportedCapability => {
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::UnsupportedCapability)
        }
        AcpError::Capacity => d::DispatchError::ProtocolFault(d::NativeProtocolFailure::Capacity),
        AcpError::UnmatchedResponse => {
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::UnmatchedResponse)
        }
        AcpError::DeliveryUnknown => {
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::DeliveryUnknown)
        }
        AcpError::Deadline => d::DispatchError::ProtocolFault(d::NativeProtocolFailure::Deadline),
        AcpError::Io => d::DispatchError::ProtocolFault(d::NativeProtocolFailure::Io),
        AcpError::InvalidOption => {
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::InvalidOption)
        }
        AcpError::MissingSnapshot => {
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::MissingSnapshot)
        }
        AcpError::SessionReadBusy => {
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::SessionReadBusy)
        }
        AcpError::RuntimeUnavailable => {
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::RuntimeUnavailable)
        }
        AcpError::Allocation => {
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::Allocation)
        }
        AcpError::InvalidFence => {
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::InvalidFence)
        }
        AcpError::InvalidFrame => {
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::InvalidFrame)
        }
    }
}
fn scalar(id: &RpcId) -> d::RpcScalar {
    match id {
        RpcId::Integer(v) => d::RpcScalar::Integer(*v),
        RpcId::String(v) => d::RpcScalar::String(v.clone()),
    }
}
const fn wire(v: Version) -> d::ProtocolVersion {
    match v {
        Version::V1 => d::ProtocolVersion::V1,
        Version::V2 => d::ProtocolVersion::V2,
    }
}
const fn version(v: d::ProtocolVersion) -> Version {
    match v {
        d::ProtocolVersion::V1 => Version::V1,
        d::ProtocolVersion::V2 => Version::V2,
    }
}
#[allow(
    clippy::cast_possible_truncation,
    reason = "Rust's supported Windows and portable target usize widths are at most u64; frame creation still uses checked length"
)]
const fn count(n: usize) -> u64 {
    n as u64
}
const fn why(v: ZeroReason) -> d::TransportReason {
    match v {
        ZeroReason::Cancelled => d::TransportReason::Cancelled,
        ZeroReason::Deadline => d::TransportReason::Deadline,
        ZeroReason::FenceRejected => d::TransportReason::FenceRejected,
        ZeroReason::ConnectionClosed => d::TransportReason::ConnectionClosed,
        ZeroReason::Abandoned => d::TransportReason::Abandoned,
    }
}
const fn failure(v: WriteFailure) -> d::TransportReason {
    match v {
        WriteFailure::Io => d::TransportReason::Io,
        WriteFailure::Deadline => d::TransportReason::Deadline,
        WriteFailure::Cancelled => d::TransportReason::Cancelled,
        WriteFailure::FenceRejected => d::TransportReason::FenceRejected,
        WriteFailure::InvalidFence => d::TransportReason::InvalidFence,
        WriteFailure::ConnectionLost => d::TransportReason::ConnectionLost,
    }
}
const fn report(v: WriteReport) -> d::TransportFact {
    match v {
        WriteReport::Written => d::TransportFact::Written,
        WriteReport::Zero { reason } => d::TransportFact::Zero {
            reason: why(reason),
        },
        WriteReport::Unknown { accepted, cause } => d::TransportFact::Unknown {
            accepted: count(accepted),
            cause: failure(cause),
        },
    }
}
const fn progress(v: WriteProgress) -> d::TransportProgress {
    match v {
        WriteProgress::Reserved => d::TransportProgress::Reserved,
        WriteProgress::Queued => d::TransportProgress::Queued,
        WriteProgress::Writing { accepted, total } => d::TransportProgress::Writing {
            accepted: count(accepted),
            total: count(total),
        },
        WriteProgress::Flushing { total } => d::TransportProgress::Flushing {
            total: count(total),
        },
        WriteProgress::Finished(v) => d::TransportProgress::Finished(report(v)),
    }
}
const fn wait(v: n::ResponseWait) -> ResponseDeadline {
    match v {
        n::ResponseWait::At(v) => ResponseDeadline::At(v),
        n::ResponseWait::Unbounded => ResponseDeadline::Unbounded,
    }
}
const fn step(v: WriteStep) -> n::DispatchWriteStep {
    match v {
        WriteStep::Chunk { offset, total } => n::DispatchWriteStep::Chunk { offset, total },
        WriteStep::Flush { total } => n::DispatchWriteStep::Flush { total },
    }
}
pub(super) const fn reject(v: n::DispatchFenceRejection) -> FenceRejection {
    match v {
        n::DispatchFenceRejection::Stopped => FenceRejection::Stopped,
        n::DispatchFenceRejection::Cancelled => FenceRejection::Cancelled,
        n::DispatchFenceRejection::StaleTarget => FenceRejection::StaleTarget,
        n::DispatchFenceRejection::RevisionChanged => FenceRejection::RevisionChanged,
        n::DispatchFenceRejection::NotReady => FenceRejection::NotReady,
        n::DispatchFenceRejection::RequiredStateUnavailable => {
            FenceRejection::RequiredStateUnavailable
        }
    }
}
pub(super) struct PollBridge<'a>(pub(super) &'a mut dyn PollIo);
impl n::DispatchPollIo for PollBridge<'_> {
    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<n::DispatchIoProgress, n::DispatchIoError>> {
        self.0.poll(cx).map(|r| {
            r.map(|p| match p {
                IoProgress::Accepted(n) => n::DispatchIoProgress::Accepted(n),
                IoProgress::Flushed => n::DispatchIoProgress::Flushed,
            })
            .map_err(|_| n::DispatchIoError)
        })
    }
    fn park(&mut self) -> Result<(), n::DispatchIoError> {
        self.0.park().map_err(|_| n::DispatchIoError)
    }
}
impl n::EncodedPromptFrame for EncodedFrame {
    fn byte_len_checked(&self) -> Result<u64, d::DispatchError> {
        u64::try_from(self.bytes().len()).map_err(|_| d::DispatchError::LengthUnrepresentable)
    }
}
impl n::PromptTicket for WriteTicket {
    type Probe = WriteProbe;
    fn probe(&self) -> WriteProbe {
        self.probe()
    }
    fn progress(&self) -> d::TransportProgress {
        progress(self.progress())
    }
    fn request_cancel(&self) {
        self.request_cancel();
    }
    fn wait(&self) -> n::BoxFuture<'_, d::TransportFact> {
        Box::pin(async { report(self.wait().await) })
    }
}
impl n::NativeIoOwner for IoOwner {
    fn request_stop(&self) {
        self.request_stop();
    }
    fn shutdown(&self, deadline: Instant) -> n::BoxFuture<'_, n::IoShutdownObservation> {
        Box::pin(async move {
            let r = self.shutdown(deadline).await;
            n::IoShutdownObservation {
                joined: r.joined,
                outstanding: r
                    .outstanding
                    .iter()
                    .map(|p| progress(p.progress()))
                    .collect(),
                unresolved: r.unresolved_requests.iter().map(scalar).collect(),
                failed: r.error.is_some(),
            }
        })
    }
}
pub struct AcpStreams {
    reader: Box<dyn tokio::io::AsyncRead + Unpin + Send>,
    writer: Box<dyn tokio::io::AsyncWrite + Unpin + Send>,
}
impl AcpStreams {
    #[must_use]
    pub fn from_owned<R, W>(reader: R, writer: W) -> Self
    where
        R: tokio::io::AsyncRead + Unpin + Send + 'static,
        W: tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        Self {
            reader: Box::new(reader),
            writer: Box::new(writer),
        }
    }
}
#[cfg(test)]
impl AcpStreams {
    #[allow(
        dead_code,
        reason = "Test-only writer decorator used by optional dev and independent acceptance fixtures."
    )]
    pub(crate) fn map_writer_for_test(
        self,
        map: impl FnOnce(
            Box<dyn tokio::io::AsyncWrite + Unpin + Send>,
        ) -> Box<dyn tokio::io::AsyncWrite + Unpin + Send>,
    ) -> Self {
        Self {
            reader: self.reader,
            writer: map(self.writer),
        }
    }
}
struct SourceState {
    connection: std::sync::OnceLock<Connection>,
    scope: ConnectionScope,
    terminal_id: EntityId,
    terminal: Mutex<Option<n::SourceTerminalReason>>,
    #[cfg(windows)]
    source_replies: Arc<source_sessions::SourceReplyTracker>,
    #[cfg(windows)]
    views: std::sync::OnceLock<std::sync::Weak<super::native_views::AcpSourceViews>>,
}
#[derive(Clone)]
pub struct AcpSource(Arc<SourceState>);
impl AcpSource {
    pub(super) fn connection(&self) -> Result<Connection, d::DispatchError> {
        self.0
            .connection
            .get()
            .cloned()
            .ok_or(d::DispatchError::ProtocolUnavailable)
    }
    #[cfg(windows)]
    pub(super) fn install_views(
        &self,
        views: &Arc<super::native_views::AcpSourceViews>,
    ) -> Result<(), crate::domain::shared_view::ViewError> {
        self.0
            .views
            .set(Arc::downgrade(views))
            .map_err(|_| crate::domain::shared_view::ViewError::AlreadyAttached)
    }
    #[cfg(windows)]
    pub(crate) fn views(
        &self,
    ) -> Result<Arc<super::native_views::AcpSourceViews>, crate::domain::shared_view::ViewError>
    {
        self.0
            .views
            .get()
            .and_then(std::sync::Weak::upgrade)
            .ok_or(crate::domain::shared_view::ViewError::CaptureUnavailable)
    }
    fn terminal(&self, reason: n::SourceTerminalReason) -> n::SourceTerminal<ConnectionScope> {
        let reason = self
            .0
            .terminal
            .lock()
            .map_or(n::SourceTerminalReason::ObservationFault, |mut value| {
                *value.get_or_insert(reason)
            });
        n::SourceTerminal::from_actual(self.0.scope.clone(), self.0.terminal_id, reason)
    }
}
#[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
impl AcpSource {
    pub(crate) fn replay_for_test(
        &self,
        native: &str,
        setup: &SessionSetup,
        deadline: Instant,
    ) -> Result<WriteTicket, d::DispatchError> {
        let connection = self.connection()?;
        let request = connection
            .strategy()
            .ok_or(d::DispatchError::ProtocolUnavailable)?
            .replay(session(native).map_err(error)?, setup)
            .map_err(error)?;
        let reservation = connection
            .reserve_request(request, ResponseDeadline::At(deadline))
            .map_err(error)?;
        let frame = reservation.encode().map_err(error)?;
        connection
            .enqueue(
                reservation,
                frame,
                Arc::new(ReplayFixtureFence(connection.scope())),
                deadline,
            )
            .map_err(|rejected| error(rejected.error()))
    }
}
#[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
struct ReplayFixtureFence(ConnectionScope);
#[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
impl WriteFence for ReplayFixtureFence {
    fn check_and_poll(
        &self,
        identity: &WriteIdentity,
        _: WriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn PollIo,
    ) -> Result<(), FenceRejection> {
        if identity.scope() != self.0
            || !matches!(
                identity.kind(),
                OutboundKind::Request(RequestKind::Replay(_))
            )
        {
            return Err(FenceRejection::StaleTarget);
        }
        let _ = io.poll(cx);
        Ok(())
    }
}
pub struct AcpStart {
    actual: ObservedStart,
    source: AcpSource,
}
struct LoadedTarget {
    source: AcpSource,
    gate: ObservedWriteGate,
    scope: ConnectionScope,
    resource: crate::domain::native::ResourceEpoch,
    configuration: (EntityId, u64),
}
#[derive(Default)]
pub struct AcpNativeAdapter {
    targets: Mutex<HashMap<NativeSessionKey, LoadedTarget>>,
}
impl AcpNativeAdapter {
    fn require_loaded_control(
        &self,
        source: &AcpSource,
        native: &NativeSessionKey,
        resource: &n::PreResourceProbe,
    ) -> Result<(), d::DispatchError> {
        if resource.native() != native {
            return Err(d::DispatchError::InvalidResource);
        }
        let targets = self
            .targets
            .lock()
            .map_err(|_| d::DispatchError::ProtocolUnavailable)?;
        let loaded = targets
            .get(native)
            .ok_or(d::DispatchError::InvalidResource)?;
        let metadata = resource.metadata();
        if !Arc::ptr_eq(&loaded.source.0, &source.0)
            || loaded.scope != source.0.scope
            || loaded.resource.runtime != metadata.source.runtime
            || loaded.resource.connection.0 != metadata.source.connection_epoch
            || loaded.resource.load.0 != metadata.load_epoch
            || loaded.configuration
                != (
                    metadata.source.config_identity,
                    metadata.source.config_revision,
                )
        {
            return Err(d::DispatchError::InvalidResource);
        }
        drop(targets);
        Ok(())
    }
}
pub struct AcpPromptReservation {
    actual: Reservation,
    rpc: d::RpcScalar,
    source: AcpSource,
    gate: ObservedWriteGate,
    bound: n::BoundWrite<ConnectionScope>,
}
impl n::ReservedPrompt for AcpPromptReservation {
    type Scope = ConnectionScope;
    type Frame = EncodedFrame;
    type Probe = WriteProbe;
    fn scope(&self) -> ConnectionScope {
        self.actual.scope()
    }
    fn rpc(&self) -> &d::RpcScalar {
        &self.rpc
    }
    fn probe(&self) -> WriteProbe {
        self.actual.probe()
    }
    fn encode(&self) -> Result<EncodedFrame, d::DispatchError> {
        self.actual.encode().map_err(error)
    }
}
struct PromptFence {
    boundary: Arc<dyn n::DispatchWriteBoundary<ConnectionScope>>,
    bound: n::BoundWrite<ConnectionScope>,
}
impl WriteFence for PromptFence {
    fn check_and_poll(
        &self,
        w: &WriteIdentity,
        s: WriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn PollIo,
    ) -> Result<(), FenceRejection> {
        if w.scope() != self.bound.scope
            || w.id().map(scalar).as_ref() != Some(&self.bound.rpc)
            || !matches!(w.kind(),OutboundKind::Request(RequestKind::Prompt(session)) if session.as_str()==self.bound.native.native_id())
        {
            return Err(FenceRejection::StaleTarget);
        }
        self.boundary
            .check_and_poll(&self.bound, step(s), cx, &mut PollBridge(io))
            .map_err(reject)
    }
}
impl n::NativeProtocolPort for AcpNativeAdapter {
    type Scope = ConnectionScope;
    type Retained = raw_json::OwnedRawValue;
    type Frame = EncodedFrame;
    type Probe = WriteProbe;
    type Reservation = AcpPromptReservation;
    type Ticket = WriteTicket;
    fn reserve_prompt(
        &self,
        b: &n::SessionBinding<ConnectionScope>,
        _: EntityId,
        body: Self::Retained,
        response: n::ResponseWait,
    ) -> Result<AcpPromptReservation, d::DispatchError> {
        let targets = self.targets.lock().map_err(|_| d::DispatchError::Stopped)?;
        let target = targets.get(b.native()).ok_or(d::DispatchError::NotReady)?;
        if target.scope != *b.scope()
            || target.resource != b.resource()
            || target.configuration != b.configuration()
        {
            return Err(d::DispatchError::InvalidResource);
        }
        let source = target.source.clone();
        let gate = target.gate.clone();
        drop(targets);
        let c = source.connection()?;
        let session = session(b.native().native_id()).map_err(error)?;
        let request = c
            .strategy()
            .ok_or(d::DispatchError::ProtocolUnavailable)?
            .prompt(
                session,
                body,
                ExtensionFields::empty().map_err(|_| d::DispatchError::InvalidConfiguration)?,
            )
            .map_err(error)?;
        let actual = c.reserve_request(request, wait(response)).map_err(error)?;
        let rpc = actual
            .id()
            .map(scalar)
            .ok_or(d::DispatchError::InvalidPromptReservation)?;
        if actual.scope() != *b.scope()
            || !matches!(actual.kind(), OutboundKind::Request(RequestKind::Prompt(_)))
        {
            return Err(d::DispatchError::InvalidPromptReservation);
        }
        let (config_identity, config_revision) = b.configuration();
        let bound = n::BoundWrite {
            scope: actual.scope(),
            rpc: rpc.clone(),
            native: b.native().clone(),
            resource: b.resource(),
            config_identity,
            config_revision,
        };
        Ok(AcpPromptReservation {
            actual,
            rpc,
            source,
            gate,
            bound,
        })
    }
    fn enqueue(
        &self,
        r: AcpPromptReservation,
        f: EncodedFrame,
        boundary: Arc<dyn n::DispatchWriteBoundary<ConnectionScope>>,
        deadline: Instant,
    ) -> Result<WriteTicket, n::EnqueueRejected<AcpPromptReservation, EncodedFrame>> {
        let AcpPromptReservation {
            actual,
            rpc,
            source,
            gate,
            bound,
        } = r;
        let connection = match source.connection() {
            Ok(v) => v,
            Err(e) => {
                return Err(n::EnqueueRejected {
                    error: e,
                    reservation: AcpPromptReservation {
                        actual,
                        rpc,
                        source,
                        gate,
                        bound,
                    },
                    frame: f,
                });
            }
        };
        let fence = Arc::new(PromptFence {
            boundary,
            bound: n::BoundWrite {
                scope: bound.scope.clone(),
                rpc: bound.rpc.clone(),
                native: bound.native.clone(),
                resource: bound.resource,
                config_identity: bound.config_identity,
                config_revision: bound.config_revision,
            },
        });
        connection
            .enqueue_observed(actual, f, fence, deadline, &gate)
            .map_err(|e| {
                let (e, actual, frame) = e.into_parts();
                n::EnqueueRejected {
                    error: error(e),
                    reservation: AcpPromptReservation {
                        actual,
                        rpc,
                        source,
                        gate,
                        bound,
                    },
                    frame,
                }
            })
    }
}

fn session(s: &str) -> Result<SessionId, AcpError> {
    SessionId::parse(raw_model::string(s)?.as_ref())
}
pub trait LifecycleMarker: Send + Sync + 'static {
    const KIND: n::LifecycleKind;
}
pub struct InitializeMarker;
pub struct ResumeMarker;
pub struct CloseMarker;
impl LifecycleMarker for InitializeMarker {
    const KIND: n::LifecycleKind = n::LifecycleKind::Initialize;
}
impl LifecycleMarker for ResumeMarker {
    const KIND: n::LifecycleKind = n::LifecycleKind::Resume;
}
impl LifecycleMarker for CloseMarker {
    const KIND: n::LifecycleKind = n::LifecycleKind::Close;
}
pub struct AcpLifecycleReservation<K: LifecycleMarker> {
    actual: Reservation,
    rpc: d::RpcScalar,
    marker: PhantomData<K>,
}
impl<K: LifecycleMarker> AcpLifecycleReservation<K> {
    fn checked(actual: Reservation, scope: &ConnectionScope) -> Result<Self, d::DispatchError> {
        let valid = matches!(
            (K::KIND, actual.kind()),
            (
                n::LifecycleKind::Initialize,
                OutboundKind::Request(RequestKind::Initialize(_))
            ) | (
                n::LifecycleKind::Resume,
                OutboundKind::Request(RequestKind::Resume(_))
            ) | (
                n::LifecycleKind::Close,
                OutboundKind::Request(RequestKind::Close(_))
            )
        );
        if actual.scope() != *scope || !valid {
            return Err(d::DispatchError::InvalidScope);
        }
        let rpc = actual
            .id()
            .map(scalar)
            .ok_or(d::DispatchError::ProtocolRejected)?;
        Ok(Self {
            actual,
            rpc,
            marker: PhantomData,
        })
    }
}
impl<K: LifecycleMarker> n::ReservedPrompt for AcpLifecycleReservation<K> {
    type Scope = ConnectionScope;
    type Frame = EncodedFrame;
    type Probe = WriteProbe;
    fn scope(&self) -> ConnectionScope {
        self.actual.scope()
    }
    fn rpc(&self) -> &d::RpcScalar {
        &self.rpc
    }
    fn probe(&self) -> WriteProbe {
        self.actual.probe()
    }
    fn encode(&self) -> Result<EncodedFrame, d::DispatchError> {
        self.actual.encode().map_err(error)
    }
}
impl<K: LifecycleMarker> n::LifecycleReservation for AcpLifecycleReservation<K> {
    fn kind(&self) -> n::LifecycleKind {
        K::KIND
    }
}
struct LifecycleFence {
    scope: ConnectionScope,
    rpc: d::RpcScalar,
    kind: n::LifecycleKind,
    boundary: Arc<dyn n::LifecycleWriteBoundary<ConnectionScope>>,
}
impl WriteFence for LifecycleFence {
    fn check_and_poll(
        &self,
        w: &WriteIdentity,
        s: WriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn PollIo,
    ) -> Result<(), FenceRejection> {
        let valid = matches!(
            (self.kind, w.kind()),
            (
                n::LifecycleKind::Initialize,
                OutboundKind::Request(RequestKind::Initialize(_))
            ) | (
                n::LifecycleKind::Resume,
                OutboundKind::Request(RequestKind::Resume(_))
            ) | (
                n::LifecycleKind::Close,
                OutboundKind::Request(RequestKind::Close(_))
            )
        );
        if !valid || w.scope() != self.scope || w.id().map(scalar).as_ref() != Some(&self.rpc) {
            return Err(FenceRejection::StaleTarget);
        }
        self.boundary
            .check_and_poll(
                &n::LifecycleWriteIdentity {
                    scope: w.scope(),
                    rpc: self.rpc.clone(),
                    kind: self.kind,
                },
                step(s),
                cx,
                &mut PollBridge(io),
            )
            .map_err(reject)
    }
}
#[expect(
    clippy::result_large_err,
    reason = "Return the same original linear objects on rejection without a new fallible allocation or loss of cleanup ownership."
)]
fn enqueue_lifecycle<K: LifecycleMarker>(
    source: &AcpSource,
    r: AcpLifecycleReservation<K>,
    f: EncodedFrame,
    boundary: Arc<dyn n::LifecycleWriteBoundary<ConnectionScope>>,
    deadline: Instant,
) -> Result<WriteTicket, n::EnqueueRejected<AcpLifecycleReservation<K>, EncodedFrame>> {
    let connection = match source.connection() {
        Ok(v) => v,
        Err(e) => {
            return Err(n::EnqueueRejected {
                error: e,
                reservation: r,
                frame: f,
            });
        }
    };
    let fence = Arc::new(LifecycleFence {
        scope: r.actual.scope(),
        rpc: r.rpc.clone(),
        kind: K::KIND,
        boundary,
    });
    connection
        .enqueue(r.actual, f, fence, deadline)
        .map_err(|e| {
            let (e, actual, frame) = e.into_parts();
            n::EnqueueRejected {
                error: error(e),
                reservation: AcpLifecycleReservation {
                    actual,
                    rpc: r.rpc,
                    marker: PhantomData,
                },
                frame,
            }
        })
}
fn raw_equal(
    a: raw_json::RawValueRef<'_>,
    b: raw_json::RawValueRef<'_>,
) -> Result<bool, d::DispatchError> {
    Ok(a.compact()
        .map_err(|_| d::DispatchError::InvalidConfiguration)?
        == b.compact()
            .map_err(|_| d::DispatchError::InvalidConfiguration)?)
}
fn known_equal<T>(
    a: &Known<T>,
    b: &Known<T>,
    equal: impl FnOnce(&T, &T) -> Result<bool, d::DispatchError>,
) -> Result<bool, d::DispatchError> {
    match (a, b) {
        (Known::Unknown, Known::Unknown) => Ok(true),
        (Known::Known(a), Known::Known(b)) => equal(a, b),
        _ => Ok(false),
    }
}
fn mcp_equal(a: &[McpServer], b: &[McpServer]) -> Result<bool, d::DispatchError> {
    if a.len() != b.len() {
        return Ok(false);
    }
    for (a, b) in a.iter().zip(b) {
        if !raw_equal(a.as_raw(), b.as_raw())? {
            return Ok(false);
        }
    }
    Ok(true)
}
fn setup_equal(a: &SessionSetup, b: &SessionSetup) -> Result<bool, d::DispatchError> {
    Ok(a.wire() == b.wire()
        && a.source().reference == b.source().reference
        && a.source().revision == b.source().revision
        && a.cwd() == b.cwd()
        && a.additional_directories() == b.additional_directories()
        && mcp_equal(a.mcp_servers(), b.mcp_servers())?
        && raw_equal(a.extra().as_raw(), b.extra().as_raw())?)
}

impl n::NativeLifecyclePort for AcpNativeAdapter {
    type Streams = AcpStreams;
    type Source = AcpSource;
    type Start = AcpStart;
    type Receiver = ObservationReceiver;
    type Lease = ObservationLease;
    type Acknowledger = ObservationAcknowledger;
    type Gate = ObservedWriteGate;
    type RawEvent = RawMessage;
    type IoOwner = IoOwner;
    type Setup = SetupSnapshot;
    type BoundSetup = SessionSetup;
    type InitReservation = AcpLifecycleReservation<InitializeMarker>;
    type LoadReservation = AcpLifecycleReservation<ResumeMarker>;
    type CloseReservation = AcpLifecycleReservation<CloseMarker>;
    fn prepare_source<A>(
        &self,
        streams: AcpStreams,
        adopt: A,
    ) -> Result<n::PreparedProtocol<Self>, n::PrepareFailure>
    where
        A: FnOnce(IoOwner) -> Result<(), n::IoAdoptionRejected<IoOwner>>,
    {
        let terminal_id =
            crate::application::request_registry::random_id().map_err(|_| n::PrepareFailure {
                phase: n::PrepareFailurePhase::BeforeAdopt,
                error: d::DispatchError::ProtocolUnavailable,
            })?;
        let (start, receiver, acknowledger, gate) =
            Connection::prepare_observed(streams.reader, streams.writer, |owner| {
                adopt(owner).map_err(|e| IoAdoptionError::new(AcpError::Io, e.owner))
            })
            .map_err(|e| n::PrepareFailure {
                phase: match e.phase() {
                    ObservedPreparePhase::BeforeAdopt => n::PrepareFailurePhase::BeforeAdopt,
                    ObservedPreparePhase::AfterAdopt => n::PrepareFailurePhase::AfterAdopt,
                },
                error: error(e.error()),
            })?;
        let source = AcpSource(Arc::new(SourceState {
            scope: start.scope(),
            connection: std::sync::OnceLock::new(),
            terminal_id,
            terminal: Mutex::new(None),
            #[cfg(windows)]
            source_replies: Arc::new(source_sessions::SourceReplyTracker::new(start.scope())),
            #[cfg(windows)]
            views: std::sync::OnceLock::new(),
        }));
        Ok(n::PreparedProtocol {
            start: AcpStart {
                actual: start,
                source,
            },
            receiver,
            acknowledger,
            gate,
        })
    }
    fn prepared_scope(&self, start: &AcpStart) -> ConnectionScope {
        start.actual.scope()
    }
    fn start_source(&self, start: AcpStart) -> AcpSource {
        let actual = start.actual.start();
        let _ = start.source.0.connection.set(actual);
        start.source
    }
    fn source_scope(&self, source: &AcpSource) -> ConnectionScope {
        source.0.scope.clone()
    }
    fn reserve_initialize(
        &self,
        source: &AcpSource,
        deadline: Instant,
    ) -> Result<Self::InitReservation, d::DispatchError> {
        AcpLifecycleReservation::checked(
            source
                .connection()?
                .reserve_preferred_initialize(deadline)
                .map_err(error)?,
            &source.0.scope,
        )
    }
    fn setups_equal(&self, a: &SetupSnapshot, b: &SetupSnapshot) -> Result<bool, d::DispatchError> {
        Ok(a.wire == b.wire
            && a.source.reference == b.source.reference
            && a.source.revision == b.source.revision
            && known_equal(&a.cwd, &b.cwd, |a, b| Ok(a == b))?
            && known_equal(
                &a.additional_directories,
                &b.additional_directories,
                |a, b| Ok(a == b),
            )?
            && known_equal(&a.mcp_servers, &b.mcp_servers, |a, b| mcp_equal(a, b))?
            && raw_equal(a.extra.as_raw(), b.extra.as_raw())?)
    }
    fn bind_setup(
        &self,
        source: &AcpSource,
        wire: d::ProtocolVersion,
        key: &n::ConfigurationKey,
        setup: SetupSnapshot,
    ) -> Result<SessionSetup, d::DispatchError> {
        if source
            .connection()?
            .strategy()
            .ok_or(d::DispatchError::ProtocolUnavailable)?
            .version()
            != version(wire)
            || setup.wire != version(wire)
            || setup.source.revision != key.revision
            || setup.source.reference.as_ref() != key.identity.encode()
        {
            return Err(d::DispatchError::InvalidConfiguration);
        }
        SessionSetup::try_from_snapshot(setup).map_err(|_| d::DispatchError::InvalidConfiguration)
    }
    fn setup_matches(
        &self,
        source: &AcpSource,
        current: &SessionSetup,
        candidate: &SetupSnapshot,
        key: &n::ConfigurationKey,
    ) -> Result<bool, d::DispatchError> {
        let bound = self.bind_setup(source, wire(current.wire()), key, candidate.clone())?;
        setup_equal(current, &bound)
    }
    fn reserve_resume(
        &self,
        source: &AcpSource,
        resource: &n::BoundResourceProbe<ConnectionScope>,
        setup: &SessionSetup,
        w: n::ResponseWait,
    ) -> Result<Self::LoadReservation, d::DispatchError> {
        if *resource.scope() != source.0.scope || resource.wire() != wire(setup.wire()) {
            return Err(d::DispatchError::InvalidScope);
        }
        let c = source.connection()?;
        let request = c
            .strategy()
            .ok_or(d::DispatchError::ProtocolUnavailable)?
            .resume(
                session(resource.resource().native().native_id()).map_err(error)?,
                setup,
            )
            .map_err(error)?;
        let reservation = AcpLifecycleReservation::checked(
            c.reserve_request(request, wait(w)).map_err(error)?,
            &source.0.scope,
        )?;
        #[cfg(windows)]
        if let Ok(views) = source.views() {
            let _ = views.bind_resume(&reservation.actual, resource.resource().native().clone());
        }
        Ok(reservation)
    }
    fn enqueue_initialize(
        &self,
        source: &AcpSource,
        reservation: Self::InitReservation,
        frame: EncodedFrame,
        boundary: Arc<dyn n::LifecycleWriteBoundary<ConnectionScope>>,
        deadline: Instant,
    ) -> Result<WriteTicket, n::EnqueueRejected<Self::InitReservation, EncodedFrame>> {
        enqueue_lifecycle(source, reservation, frame, boundary, deadline)
    }
    fn enqueue_resume(
        &self,
        source: &AcpSource,
        reservation: Self::LoadReservation,
        frame: EncodedFrame,
        boundary: Arc<dyn n::LifecycleWriteBoundary<ConnectionScope>>,
        deadline: Instant,
    ) -> Result<WriteTicket, n::EnqueueRejected<Self::LoadReservation, EncodedFrame>> {
        enqueue_lifecycle(source, reservation, frame, boundary, deadline)
    }
    fn reserve_target_close(
        &self,
        s: &AcpSource,
        b: &n::CurrentTargetCloseBinding<ConnectionScope>,
        w: n::ResponseWait,
    ) -> Result<Self::CloseReservation, d::DispatchError> {
        if *b.scope() != s.0.scope || b.resource().native() != b.native() {
            return Err(d::DispatchError::InvalidScope);
        }
        let c = s.connection()?;
        let request = c
            .strategy()
            .ok_or(d::DispatchError::ProtocolUnavailable)?
            .close(
                session(b.native().native_id()).map_err(error)?,
                ExtensionFields::empty().map_err(|_| d::DispatchError::InvalidConfiguration)?,
            )
            .map_err(error)?;
        AcpLifecycleReservation::checked(
            c.reserve_request(request, wait(w)).map_err(error)?,
            &s.0.scope,
        )
    }
    fn enqueue_target_close(
        &self,
        source: &AcpSource,
        reservation: Self::CloseReservation,
        frame: EncodedFrame,
        boundary: Arc<dyn n::LifecycleWriteBoundary<ConnectionScope>>,
        deadline: Instant,
    ) -> Result<WriteTicket, n::EnqueueRejected<Self::CloseReservation, EncodedFrame>> {
        enqueue_lifecycle(source, reservation, frame, boundary, deadline)
    }
    fn next<'a>(
        &'a self,
        s: &'a AcpSource,
        r: &'a mut ObservationReceiver,
    ) -> n::BoxFuture<'a, n::ProtocolRead<ObservationLease, ConnectionScope>> {
        Box::pin(async move {
            match r.next().await {
                Ok(Some(lease)) => {
                    #[cfg(windows)]
                    if let Ok(views) = s.views() {
                        let _ = views.receive(lease.received());
                    }
                    n::ProtocolRead::Received(lease)
                }
                Ok(None) => n::ProtocolRead::Terminal(s.terminal(n::SourceTerminalReason::Eof)),
                Err(AcpError::Io) => {
                    n::ProtocolRead::Terminal(s.terminal(n::SourceTerminalReason::Io))
                }
                Err(_) => n::ProtocolRead::Terminal(s.terminal(n::SourceTerminalReason::Protocol)),
            }
        })
    }
    fn view<'a>(
        &self,
        l: &'a ObservationLease,
    ) -> Result<n::ObservationView<'a, ConnectionScope, RawMessage>, d::DispatchError> {
        let r = l.received();
        let incoming = match r.incoming() {
            Incoming::Response { id, response, .. } => {
                let rpc = scalar(id);
                match response {
                    Response::NewSession(session) => n::NativeIncoming::Created {
                        rpc,
                        session: session.shared_text(),
                    },
                    Response::Sessions(_) | Response::DeferredSessions(_) => {
                        n::NativeIncoming::Sessions { rpc }
                    }
                    Response::Initialized(s) => n::NativeIncoming::Initialized {
                        rpc,
                        wire: wire(s.version()),
                    },
                    Response::Resumed { session } => n::NativeIncoming::Resumed {
                        rpc,
                        session: Arc::from(session.as_str()),
                    },
                    Response::Closed { session } => n::NativeIncoming::Closed {
                        rpc,
                        session: Arc::from(session.as_str()),
                    },
                    Response::Prompt(PromptResult::Completed { stop_reason }) => {
                        n::NativeIncoming::PromptV1 {
                            rpc,
                            reason: stop_reason.clone(),
                        }
                    }
                    Response::Prompt(PromptResult::Inserted { message }) => {
                        n::NativeIncoming::PromptV2 {
                            rpc,
                            message: Arc::from(message.as_str()),
                        }
                    }
                    Response::RemoteError { code } => {
                        n::NativeIncoming::RemoteError { rpc, code: *code }
                    }
                    _ => n::NativeIncoming::OtherResponse { rpc },
                }
            }
            Incoming::Permission(p) => n::NativeIncoming::Permission {
                session: Arc::from(p.session().as_str()),
                rpc: scalar(p.id()),
            },
            Incoming::UnsupportedRequest(p) => n::NativeIncoming::Unsupported {
                rpc: scalar(p.id()),
            },
            Incoming::Update { event, origin, .. } => {
                let origin = match origin {
                    UpdateOrigin::Live => n::ObservationOrigin::Live,
                    UpdateOrigin::DuringResume => n::ObservationOrigin::DuringResume,
                    UpdateOrigin::DuringReplay => n::ObservationOrigin::DuringReplay,
                };
                match event {
                    SessionEvent::State {
                        session,
                        state,
                        stop_reason,
                    } => n::NativeIncoming::State {
                        session: Arc::from(session.as_str()),
                        state: state.clone(),
                        reason: stop_reason.clone(),
                        origin,
                    },
                    _ => n::NativeIncoming::OtherSession {
                        session: Arc::from(event.session().as_str()),
                    },
                }
            }
            Incoming::UnknownNotification { .. } => n::NativeIncoming::UnknownNotification,
        };
        Ok(n::ObservationView {
            scope: r.scope(),
            sequence: r.sequence(),
            receipt_order: r.receipt_order(),
            raw: r.raw(),
            incoming,
        })
    }
    fn cancel_resume(
        &self,
        r: Self::LoadReservation,
    ) -> n::CancelledLifecycleReservation<ConnectionScope, WriteProbe> {
        let scope = r.actual.scope();
        let probe = r.actual.cancel();
        n::CancelledLifecycleReservation::from_actual(scope, r.rpc, probe, n::LifecycleKind::Resume)
    }
    fn probe_progress(&self, p: &WriteProbe) -> d::TransportProgress {
        progress(p.progress())
    }
    fn fail_observation(
        &self,
        s: &AcpSource,
        l: ObservationLease,
        _: d::DispatchError,
    ) -> n::SourceTerminal<ConnectionScope> {
        drop(l);
        if let Ok(c) = s.connection() {
            c.stop_owned();
        }
        s.terminal(n::SourceTerminalReason::ObservationFault)
    }
    fn acknowledge(
        &self,
        a: &ObservationAcknowledger,
        l: ObservationLease,
        p: &n::AppliedObservation<ConnectionScope>,
    ) -> Result<(), n::AckRejected<ObservationLease>> {
        let r = l.received();
        if r.scope() != *p.scope()
            || r.sequence() != p.sequence()
            || r.receipt_order() != p.receipt_order()
        {
            return Err(n::AckRejected {
                error: d::DispatchError::ObservationUnapplied,
                lease: l,
            });
        }
        a.acknowledge(l).map_err(|e| n::AckRejected {
            error: d::DispatchError::ObservationOrder,
            lease: e.into_lease(),
        })
    }
    fn attach_prompt_binding(
        &self,
        s: &AcpSource,
        p: &n::VerifiedLoadedBinding<ConnectionScope, SessionSetup>,
        gate: &ObservedWriteGate,
    ) -> Result<(), d::DispatchError> {
        let b = p.binding();
        if *b.scope() != s.0.scope
            || s.connection()?
                .strategy()
                .ok_or(d::DispatchError::ProtocolUnavailable)?
                .version()
                != p.setup().wire()
        {
            return Err(d::DispatchError::InvalidScope);
        }
        self.targets
            .lock()
            .map_err(|_| d::DispatchError::Stopped)?
            .insert(
                b.native().clone(),
                LoadedTarget {
                    source: s.clone(),
                    gate: gate.clone(),
                    scope: b.scope().clone(),
                    resource: b.resource(),
                    configuration: b.configuration(),
                },
            );
        Ok(())
    }
}

pub struct AcpControlReservation {
    actual: Reservation,
    identity: n::ControlIdentity<ConnectionScope>,
}
impl n::ReservedControl for AcpControlReservation {
    type Scope = ConnectionScope;
    type Frame = EncodedFrame;
    type Probe = WriteProbe;
    fn identity(&self) -> &n::ControlIdentity<ConnectionScope> {
        &self.identity
    }
    fn probe(&self) -> WriteProbe {
        self.actual.probe()
    }
    fn encode(&self) -> Result<EncodedFrame, d::DispatchError> {
        self.actual.encode().map_err(error)
    }
}
fn copy_control(v: &n::ControlIdentity<ConnectionScope>) -> n::ControlIdentity<ConnectionScope> {
    match v {
        n::ControlIdentity::NativeCancel {
            scope,
            native_session,
        } => n::ControlIdentity::NativeCancel {
            scope: scope.clone(),
            native_session: native_session.clone(),
        },
        n::ControlIdentity::PermissionCancelled {
            scope,
            native_session,
            rpc,
        } => n::ControlIdentity::PermissionCancelled {
            scope: scope.clone(),
            native_session: native_session.clone(),
            rpc: rpc.clone(),
        },
        n::ControlIdentity::Unsupported { scope, rpc } => n::ControlIdentity::Unsupported {
            scope: scope.clone(),
            rpc: rpc.clone(),
        },
    }
}
struct ControlFence {
    identity: n::ControlIdentity<ConnectionScope>,
    boundary: Arc<dyn n::ControlWriteBoundary<ConnectionScope>>,
}
impl WriteFence for ControlFence {
    fn check_and_poll(
        &self,
        w: &WriteIdentity,
        s: WriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn PollIo,
    ) -> Result<(), FenceRejection> {
        let valid = match (&self.identity, w.kind()) {
            (
                n::ControlIdentity::NativeCancel { native_session, .. },
                OutboundKind::Cancel(session),
            ) => w.id().is_none() && session.as_str() == native_session.as_ref(),
            (
                n::ControlIdentity::PermissionCancelled {
                    native_session,
                    rpc,
                    ..
                },
                OutboundKind::PermissionReply { session, id },
            ) => session.as_str() == native_session.as_ref() && scalar(id) == *rpc,
            (n::ControlIdentity::Unsupported { rpc, .. }, OutboundKind::UnsupportedReply(id)) => {
                scalar(id) == *rpc
            }
            _ => false,
        };
        if !valid
            || w.scope() != *self.identity.scope()
            || w.id().map(scalar).as_ref() != self.identity.rpc()
        {
            return Err(FenceRejection::StaleTarget);
        }
        self.boundary
            .check_and_poll(&self.identity, step(s), cx, &mut PollBridge(io))
            .map_err(reject)
    }
}
impl n::NativeControlPort for AcpNativeAdapter {
    type PermissionRequest = PermissionRequest;
    type UnsupportedRequest = UnsupportedRequest;
    type ControlReservation = AcpControlReservation;
    fn permission_from_lease(
        &self,
        l: &ObservationLease,
    ) -> Result<Option<PermissionRequest>, d::DispatchError> {
        Ok(match l.received().incoming() {
            Incoming::Permission(p) => Some(p.clone()),
            _ => None,
        })
    }
    fn unsupported_from_lease(
        &self,
        l: &ObservationLease,
    ) -> Result<Option<UnsupportedRequest>, d::DispatchError> {
        Ok(match l.received().incoming() {
            Incoming::UnsupportedRequest(p) => Some(p.clone()),
            _ => None,
        })
    }
    fn permission_identity(
        &self,
        p: &PermissionRequest,
    ) -> Result<n::PermissionIdentity<ConnectionScope>, d::DispatchError> {
        Ok(n::PermissionIdentity {
            scope: p.scope().ok_or(d::DispatchError::InvalidScope)?,
            rpc: scalar(p.id()),
            native_session: Arc::from(p.session().as_str()),
        })
    }
    fn unsupported_identity(
        &self,
        p: &UnsupportedRequest,
    ) -> n::UnsupportedIdentity<ConnectionScope> {
        n::UnsupportedIdentity {
            scope: p.scope(),
            rpc: scalar(p.id()),
        }
    }
    fn reserve_native_cancel(
        &self,
        s: &AcpSource,
        b: &n::CurrentCancelBinding<ConnectionScope>,
    ) -> Result<AcpControlReservation, d::DispatchError> {
        self.require_loaded_control(s, b.native(), b.resource())?;
        if *b.scope() != s.0.scope {
            return Err(d::DispatchError::InvalidScope);
        }
        let actual = s
            .connection()?
            .reserve_cancel(
                session(b.native().native_id()).map_err(error)?,
                ExtensionFields::empty().map_err(|_| d::DispatchError::InvalidConfiguration)?,
            )
            .map_err(error)?;
        if actual.id().is_some() || !matches!(actual.kind(), OutboundKind::Cancel(_)) {
            return Err(d::DispatchError::ProtocolRejected);
        }
        Ok(AcpControlReservation {
            identity: n::ControlIdentity::NativeCancel {
                scope: actual.scope(),
                native_session: Arc::from(b.native().native_id()),
            },
            actual,
        })
    }
    fn reserve_permission_cancelled(
        &self,
        s: &AcpSource,
        p: &PermissionRequest,
        b: &n::PermissionCancelBinding<ConnectionScope>,
    ) -> Result<AcpControlReservation, d::DispatchError> {
        if b.resource().native() != b.native() {
            return Err(d::DispatchError::InvalidResource);
        }
        if *b.scope() != s.0.scope
            || p.scope().as_ref() != Some(b.scope())
            || p.session().as_str() != b.native().native_id()
            || scalar(p.id()) != *b.rpc()
        {
            return Err(d::DispatchError::InvalidScope);
        }
        let actual = s
            .connection()?
            .reserve_permission_response(p, PermissionSelection::Cancelled)
            .map_err(error)?;
        if actual.id().map(scalar).as_ref() != Some(b.rpc())
            || !matches!(actual.kind(), OutboundKind::PermissionReply { .. })
        {
            return Err(d::DispatchError::ProtocolRejected);
        }
        Ok(AcpControlReservation {
            identity: n::ControlIdentity::PermissionCancelled {
                scope: actual.scope(),
                native_session: Arc::from(p.session().as_str()),
                rpc: scalar(p.id()),
            },
            actual,
        })
    }
    fn reserve_source_permission_cancelled(
        &self,
        source: &AcpSource,
        original: &PermissionRequest,
        binding: &n::SourcePermissionCancelBinding<ConnectionScope>,
    ) -> Result<AcpControlReservation, d::DispatchError> {
        if *binding.scope() != source.0.scope
            || original.scope().as_ref() != Some(binding.scope())
            || scalar(original.id()) != *binding.rpc()
        {
            return Err(d::DispatchError::InvalidScope);
        }
        let actual = source
            .connection()?
            .reserve_permission_response(original, PermissionSelection::Cancelled)
            .map_err(error)?;
        if actual.scope() != source.0.scope
            || actual.id().map(scalar).as_ref() != Some(binding.rpc())
            || !matches!(actual.kind(), OutboundKind::PermissionReply { .. })
        {
            return Err(d::DispatchError::ProtocolRejected);
        }
        Ok(AcpControlReservation {
            identity: n::ControlIdentity::PermissionCancelled {
                scope: actual.scope(),
                native_session: Arc::from(original.session().as_str()),
                rpc: scalar(original.id()),
            },
            actual,
        })
    }
    fn reserve_unsupported_reply(
        &self,
        s: &AcpSource,
        p: &UnsupportedRequest,
        b: &n::UnsupportedReplyBinding<ConnectionScope>,
    ) -> Result<AcpControlReservation, d::DispatchError> {
        if *b.scope() != s.0.scope || p.scope() != *b.scope() || scalar(p.id()) != *b.rpc() {
            return Err(d::DispatchError::InvalidScope);
        }
        let actual = s.connection()?.reserve_unsupported(p).map_err(error)?;
        if actual.id().map(scalar).as_ref() != Some(b.rpc())
            || !matches!(actual.kind(), OutboundKind::UnsupportedReply(_))
        {
            return Err(d::DispatchError::ProtocolRejected);
        }
        Ok(AcpControlReservation {
            identity: n::ControlIdentity::Unsupported {
                scope: actual.scope(),
                rpc: scalar(p.id()),
            },
            actual,
        })
    }
    fn enqueue_control(
        &self,
        source: &AcpSource,
        reservation: AcpControlReservation,
        frame: EncodedFrame,
        boundary: Arc<dyn n::ControlWriteBoundary<ConnectionScope>>,
        deadline: Instant,
    ) -> Result<WriteTicket, n::EnqueueRejected<AcpControlReservation, EncodedFrame>> {
        let connection = match source.connection() {
            Ok(connection) => connection,
            Err(rejected) => {
                return Err(n::EnqueueRejected {
                    error: rejected,
                    reservation,
                    frame,
                });
            }
        };
        let AcpControlReservation { actual, identity } = reservation;
        let fence = Arc::new(ControlFence {
            identity: copy_control(&identity),
            boundary,
        });
        connection
            .enqueue(actual, frame, fence, deadline)
            .map_err(|rejected| {
                let (rejected, actual, frame) = rejected.into_parts();
                n::EnqueueRejected {
                    error: error(rejected),
                    reservation: AcpControlReservation { actual, identity },
                    frame,
                }
            })
    }
    fn cancel_reserved_control(
        &self,
        r: AcpControlReservation,
    ) -> n::CancelledControlReservation<ConnectionScope, WriteProbe> {
        let probe = r.actual.cancel();
        n::CancelledControlReservation::from_actual(r.identity, probe)
    }
}

#[cfg(test)]
mod cause_tests;
