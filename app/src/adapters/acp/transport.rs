#![allow(
    clippy::significant_drop_tightening,
    reason = "Explicit protocol lock scopes define poll/receipt/Stop linearization; no guard spans async waiting"
)]
//! Public ownership handles. State, reader, writer and lifecycle live in separate modules.
use super::{
    AcpError, ConfigSourceStamp, ExtensionFields, OptionId, PermissionRequest, PromptResult,
    RawMessage, Request, RequestKind, RpcId, SessionEvent, SessionId, SessionListing, Strategy,
    UnsupportedRequest, Version, raw_json::OwnedRawValue, raw_model as raw,
};
use std::{
    fmt,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    time::Instant,
};
mod lifecycle;
mod observed;
pub use observed::*;
mod reader;
mod state;
mod writer;
use state::{Core, WriteCell};
#[derive(Clone)]
pub struct ConnectionScope(Arc<()>);
impl PartialEq for ConnectionScope {
    fn eq(&self, o: &Self) -> bool {
        Arc::ptr_eq(&self.0, &o.0)
    }
}
impl Eq for ConnectionScope {}
impl fmt::Debug for ConnectionScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ConnectionScope(..)")
    }
}
#[derive(Clone)]
pub struct Connection {
    core: Arc<Core>,
}
pub struct EventReceiver {
    core: Arc<Core>,
    ended: bool,
}
pub struct IoOwner {
    core: Arc<Core>,
}
#[derive(Clone, Copy, Debug)]
pub enum ResponseDeadline {
    At(Instant),
    Unbounded,
}
#[derive(Clone, Debug)]
pub enum OutboundKind {
    Request(RequestKind),
    Cancel(SessionId),
    PermissionReply { session: SessionId, id: RpcId },
    UnsupportedReply(RpcId),
}
#[derive(Clone)]
pub struct WriteIdentity {
    scope: ConnectionScope,
    id: Option<RpcId>,
    kind: OutboundKind,
    stamp: Option<ConfigSourceStamp>,
}
impl WriteIdentity {
    #[must_use]
    pub fn scope(&self) -> ConnectionScope {
        self.scope.clone()
    }
    #[must_use]
    pub const fn id(&self) -> Option<&RpcId> {
        self.id.as_ref()
    }
    #[must_use]
    pub const fn kind(&self) -> &OutboundKind {
        &self.kind
    }
    #[must_use]
    pub const fn setup_stamp(&self) -> Option<&ConfigSourceStamp> {
        self.stamp.as_ref()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteStep {
    Chunk { offset: usize, total: usize },
    Flush { total: usize },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IoProgress {
    Accepted(usize),
    Flushed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FenceRejection {
    Stopped,
    Cancelled,
    StaleTarget,
    RevisionChanged,
    NotReady,
    RequiredStateUnavailable,
}
pub trait PollIo {
    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Result<IoProgress, AcpError>>;
    /// Retains this same attempt without polling the provider; its owner must register a wake source.
    /// # Errors
    /// Rejects parking after this proxy has already attempted IO or recorded a park outcome.
    fn park(&mut self) -> Result<(), AcpError>;
}
pub trait WriteFence: Send + Sync {
    /// # Errors
    /// Rejects before polling when the private application fence is no longer valid.
    fn check_and_poll(
        &self,
        identity: &WriteIdentity,
        step: WriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn PollIo,
    ) -> Result<(), FenceRejection>;
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ZeroReason {
    Cancelled,
    Deadline,
    FenceRejected,
    ConnectionClosed,
    Abandoned,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteFailure {
    Io,
    Deadline,
    Cancelled,
    FenceRejected,
    InvalidFence,
    ConnectionLost,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteReport {
    Zero {
        reason: ZeroReason,
    },
    Written,
    Unknown {
        accepted: usize,
        cause: WriteFailure,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteProgress {
    Reserved,
    Queued,
    Writing { accepted: usize, total: usize },
    Flushing { total: usize },
    Finished(WriteReport),
}
#[derive(Clone)]
pub struct WriteProbe {
    cell: Arc<WriteCell>,
}
impl WriteProbe {
    #[must_use]
    pub fn scope(&self) -> ConnectionScope {
        self.cell.identity.scope()
    }
    #[must_use]
    pub fn id(&self) -> Option<&RpcId> {
        self.cell.identity.id()
    }
    #[must_use]
    pub fn progress(&self) -> WriteProgress {
        self.cell.lock().progress
    }
}
pub struct Reservation {
    core: Arc<Core>,
    cell: Arc<WriteCell>,
    payload: state::Payload,
    armed: bool,
}
impl Reservation {
    pub(crate) fn view_cell_key(&self) -> u64 {
        self.cell.key
    }
    #[must_use]
    pub fn scope(&self) -> ConnectionScope {
        self.cell.identity.scope()
    }
    #[must_use]
    pub fn id(&self) -> Option<&RpcId> {
        self.cell.identity.id()
    }
    #[must_use]
    pub fn kind(&self) -> &OutboundKind {
        self.cell.identity.kind()
    }
    #[must_use]
    pub fn setup_stamp(&self) -> Option<&ConfigSourceStamp> {
        self.cell.identity.setup_stamp()
    }
    #[must_use]
    pub fn probe(&self) -> WriteProbe {
        WriteProbe {
            cell: self.cell.clone(),
        }
    }
    /// # Errors
    /// Reports raw encoding or allocation failure without IO; borrowing failure retains this reservation and its slots.
    pub fn encode(&self) -> Result<EncodedFrame, AcpError> {
        let payload = match &self.payload {
            state::Payload::Raw(v) => v.clone(),
            state::Payload::Request { method, params } => {
                let id = self.id().ok_or(AcpError::InvalidMessage)?;
                let mut encoded = Vec::new();
                id.write_json(&mut encoded)?;
                let id = super::raw_json::RawDocument::parse(encoded)?.root_owned();
                raw::object(
                    [
                        ("jsonrpc", &raw::string("2.0")?),
                        ("id", &id),
                        ("method", &raw::string(method)?),
                        ("params", params),
                    ],
                    None,
                )?
            }
        };
        let mut bytes = payload.as_ref().compact()?.into_bytes();
        bytes.try_reserve(1).map_err(|_| AcpError::Allocation)?;
        bytes.push(b'\n');
        Ok(EncodedFrame {
            scope: self.scope(),
            key: self.cell.key,
            bytes,
        })
    }
    #[must_use]
    pub fn cancel(mut self) -> WriteProbe {
        self.core.cancel(&self.cell, ZeroReason::Cancelled);
        self.armed = false;
        self.probe()
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        if self.armed {
            self.core.cancel(&self.cell, ZeroReason::Abandoned);
        }
    }
}
pub struct EncodedFrame {
    scope: ConnectionScope,
    key: u64,
    bytes: Vec<u8>,
}
impl EncodedFrame {
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
pub struct EnqueueError {
    error: AcpError,
    reservation: Reservation,
    frame: EncodedFrame,
}
impl EnqueueError {
    #[must_use]
    pub const fn error(&self) -> AcpError {
        self.error
    }
    #[must_use]
    pub fn into_parts(self) -> (AcpError, Reservation, EncodedFrame) {
        (self.error, self.reservation, self.frame)
    }
}
pub struct WriteTicket {
    core: Arc<Core>,
    cell: Arc<WriteCell>,
}
impl WriteTicket {
    #[must_use]
    pub fn probe(&self) -> WriteProbe {
        WriteProbe {
            cell: self.cell.clone(),
        }
    }
    #[must_use]
    pub fn progress(&self) -> WriteProgress {
        self.cell.lock().progress
    }
    pub fn request_cancel(&self) {
        self.core.cancel(&self.cell, ZeroReason::Cancelled);
    }
    pub async fn wait(&self) -> WriteReport {
        loop {
            let notified = self.cell.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let WriteProgress::Finished(report) = self.progress() {
                return report;
            }
            notified.await;
        }
    }
    pub async fn cancel_and_wait(&self) -> WriteReport {
        self.request_cancel();
        self.wait().await
    }
}
impl Drop for WriteTicket {
    fn drop(&mut self) {
        self.core.cancel(&self.cell, ZeroReason::Abandoned);
    }
}
#[derive(Debug)]
pub struct IoShutdownReport {
    pub joined: bool,
    pub outstanding: Vec<WriteProbe>,
    pub unresolved_requests: Vec<RpcId>,
    pub error: Option<AcpError>,
}
#[derive(Clone, Debug)]
pub enum PermissionSelection {
    Selected(OptionId),
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionPendingSnapshot {
    Waiting,
    Reserved,
    SpentOrInvalid,
}
#[derive(Clone)]
pub enum Response {
    Initialized(Strategy),
    NewSession(SessionId),
    Prompt(PromptResult),
    Resumed { session: SessionId },
    Replayed { session: SessionId },
    Sessions(SessionListing),
    DeferredSessions(Strategy),
    Closed { session: SessionId },
    RemoteError { code: i64 },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateOrigin {
    Live,
    DuringResume,
    DuringReplay,
}
#[derive(Clone, Debug)]
pub enum Incoming {
    Response {
        id: RpcId,
        response: Response,
        raw: RawMessage,
    },
    Permission(PermissionRequest),
    Update {
        event: SessionEvent,
        origin: UpdateOrigin,
        raw: RawMessage,
    },
    UnsupportedRequest(UnsupportedRequest),
    UnknownNotification {
        raw: RawMessage,
    },
}
impl Incoming {
    const fn raw(&self) -> &RawMessage {
        match self {
            Self::Response { raw, .. }
            | Self::Update { raw, .. }
            | Self::UnknownNotification { raw } => raw,
            Self::Permission(p) => p.raw(),
            Self::UnsupportedRequest(r) => r.raw(),
        }
    }
}
#[derive(Clone)]
pub struct Received {
    scope: ConnectionScope,
    sequence: u64,
    receipt_order: u64,
    incoming: Incoming,
    view_context: ViewReadContextKind,
    view_window: Option<ReceivedReadWindow>,
}
/// A receipt observation, never a claim of native RPC causality or authorization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::enum_variant_names,
    reason = "The frozen receipt context ABI names explicitly distinguish observation-window kinds"
)]
pub enum ViewReadContextKind {
    OutsideReadWindow,
    UniqueReceiptWindow,
    AmbiguousReceiptWindow,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewReadWindowKind {
    Resume,
    Replay,
}
#[derive(Clone)]
pub struct ReceivedReadWindow {
    scope: ConnectionScope,
    session: SessionId,
    rpc: RpcId,
    kind: ViewReadWindowKind,
    cell_key: u64,
}
impl ReceivedReadWindow {
    pub(crate) const fn cell_key(&self) -> u64 {
        self.cell_key
    }
    pub(crate) fn scope(&self) -> ConnectionScope {
        self.scope.clone()
    }
    pub(crate) const fn session(&self) -> &SessionId {
        &self.session
    }
    pub(crate) const fn rpc(&self) -> &RpcId {
        &self.rpc
    }
    pub(crate) const fn kind(&self) -> ViewReadWindowKind {
        self.kind
    }
}
impl Received {
    pub(crate) const fn view_read_context(&self) -> ViewReadContextKind {
        self.view_context
    }
    pub(crate) const fn view_read_window(&self) -> Option<&ReceivedReadWindow> {
        self.view_window.as_ref()
    }
    #[must_use]
    pub fn scope(&self) -> ConnectionScope {
        self.scope.clone()
    }
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    #[must_use]
    pub const fn receipt_order(&self) -> u64 {
        self.receipt_order
    }
    #[must_use]
    pub const fn incoming(&self) -> &Incoming {
        &self.incoming
    }
    #[must_use]
    pub const fn raw(&self) -> &RawMessage {
        self.incoming.raw()
    }
}
impl Connection {
    /// # Errors
    /// Returns `RuntimeUnavailable` if no Tokio runtime can own the IO tasks.
    pub fn spawn<R, W>(reader: R, writer: W) -> Result<(Self, EventReceiver, IoOwner), AcpError>
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        lifecycle::spawn(reader, writer)
    }
    #[must_use]
    pub fn scope(&self) -> ConnectionScope {
        self.core.scope.clone()
    }
    #[must_use]
    pub fn strategy(&self) -> Option<Strategy> {
        self.core.lock().strategy.clone()
    }
    /// # Errors
    /// Rejects a repeated attempt, closed connection, elapsed response deadline or exhausted control identity/capacity.
    pub fn reserve_initialize(
        &self,
        offer: Version,
        response_deadline: Instant,
    ) -> Result<Reservation, AcpError> {
        let mut state = self.core.lock();
        if state.initialize_attempted {
            return Err(AcpError::AlreadyInitialized);
        }
        if state.stopped {
            return Err(AcpError::Closed);
        }
        if response_deadline <= Instant::now() {
            return Err(AcpError::Deadline);
        }
        let request = super::strategy::initialize(self.scope(), offer)?;
        let result =
            state.reserve_request(&self.core, request, ResponseDeadline::At(response_deadline))?;
        state.initialize_attempted = true;
        self.core.notify.notify_waiters();
        Ok(result)
    }
    /// # Errors
    /// Returns the same one-shot initialization, closed, deadline and control errors as `reserve_initialize`.
    pub fn reserve_preferred_initialize(
        &self,
        response_deadline: Instant,
    ) -> Result<Reservation, AcpError> {
        self.reserve_initialize(Version::preferred(), response_deadline)
    }
    /// # Errors
    /// Rejects foreign scope, closed/uninitialized state, elapsed response deadline, capacity or a busy same-session read.
    pub fn reserve_request(
        &self,
        request: Request,
        response_deadline: ResponseDeadline,
    ) -> Result<Reservation, AcpError> {
        if request.scope != self.scope() {
            return Err(AcpError::InvalidRequestScope);
        }
        let mut state = self.core.lock();
        state.expire(Instant::now());
        if state.stopped {
            return Err(AcpError::Closed);
        }
        if state.strategy.is_none() {
            return Err(AcpError::NotInitialized);
        }
        let result = state.reserve_request(&self.core, request, response_deadline);
        self.core.notify.notify_waiters();
        result
    }
    /// # Errors
    /// Rejects reserved extension fields, raw encoding/allocation failure or closed/uninitialized state.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "Frozen API consumes caller-owned extension snapshot"
    )]
    pub fn reserve_cancel(
        &self,
        session: SessionId,
        extra: ExtensionFields,
    ) -> Result<Reservation, AcpError> {
        extra.reject(&["sessionId"])?;
        let params = raw::object(
            [("sessionId", &raw::string(session.as_str())?)],
            Some(extra.as_raw()),
        )?;
        let payload = raw::object(
            [
                ("jsonrpc", &raw::string("2.0")?),
                ("method", &raw::string("session/cancel")?),
                ("params", &params),
            ],
            None,
        )?;
        self.core
            .lock()
            .reserve_control(&self.core, OutboundKind::Cancel(session), payload)
    }
    /// # Errors
    /// Rejects foreign/unbranded source, non-live original request, non-unique advertised selection or closed state.
    pub fn reserve_permission_response(
        &self,
        request: &PermissionRequest,
        choice: PermissionSelection,
    ) -> Result<Reservation, AcpError> {
        state::reserve_permission(&self.core, request, choice)
    }
    /// # Errors
    /// Rejects a foreign original request; observes the same pending slot without IO or reservation.
    pub fn inspect_permission_request(
        &self,
        request: &PermissionRequest,
    ) -> Result<PermissionPendingSnapshot, AcpError> {
        state::inspect_permission_request(&self.core, request)
    }
    /// # Errors
    /// Rejects foreign source, a non-live original request, closed state or raw encoding/allocation failure.
    pub fn reserve_unsupported(
        &self,
        request: &UnsupportedRequest,
    ) -> Result<Reservation, AcpError> {
        state::reserve_unsupported(&self.core, request)
    }
    /// # Errors
    /// Returns `EnqueueError` owning both original inputs on scope/frame/terminal/phase/deadline failure; success transfers responsibility to the ticket.
    fn enqueue_inner(
        &self,
        mut reservation: Reservation,
        frame: EncodedFrame,
        fence: Arc<dyn WriteFence>,
        write_deadline: Instant,
        observed_prompt: bool,
    ) -> Result<WriteTicket, EnqueueError> {
        let mut state = self.core.lock();
        let error = if (state.observed
            && matches!(
                reservation.kind(),
                OutboundKind::Request(RequestKind::Prompt(_))
            )
            && !observed_prompt)
            || reservation.scope() != self.scope()
        {
            Some(AcpError::InvalidRequestScope)
        } else if frame.scope != self.scope() || frame.key != reservation.cell.key {
            Some(AcpError::InvalidFrame)
        } else {
            state.expire(Instant::now());
            let progress = reservation.cell.lock().progress;
            if let WriteProgress::Finished(report) = progress {
                Some(state::report_error(report))
            } else if state.stopped {
                Some(AcpError::Closed)
            } else if write_deadline <= Instant::now() {
                state.finish_zero(&reservation.cell, ZeroReason::Deadline);
                Some(AcpError::Deadline)
            } else {
                None
            }
        };
        if let Some(error) = error {
            drop(state);
            return Err(EnqueueError {
                error,
                reservation,
                frame,
            });
        }
        reservation.cell.lock().progress = WriteProgress::Queued;
        state.queue.push_back(state::Command {
            cell: reservation.cell.clone(),
            bytes: frame.bytes,
            fence,
            deadline: write_deadline,
        });
        reservation.armed = false;
        let result = WriteTicket {
            core: self.core.clone(),
            cell: reservation.cell.clone(),
        };
        drop(state);
        self.core.notify.notify_waiters();
        Ok(result)
    }
    #[must_use]
    pub fn pending_count(&self) -> usize {
        let s = self.core.lock();
        if s.stopped { 0 } else { s.pending.len() }
    }
    #[must_use]
    pub fn server_pending_count(&self) -> usize {
        let s = self.core.lock();
        if s.stopped { 0 } else { s.server.len() }
    }
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.core.lock().stopped
    }
    pub(crate) fn stop_owned(&self) {
        self.core.stop(None);
    }
    #[must_use]
    pub fn take_rejected_message(&self) -> Option<RawMessage> {
        self.core.lock().rejected.pop_front()
    }
}
impl EventReceiver {
    /// # Errors
    /// Returns the first terminal protocol/IO error once, after queued facts and captured-frame drain; later calls return None.
    pub async fn next(&mut self) -> Result<Option<Received>, AcpError> {
        if self.ended {
            return Ok(None);
        }
        loop {
            let n = self.core.events_notify.notified();
            tokio::pin!(n);
            n.as_mut().enable();
            {
                let mut s = self.core.lock();
                if let Some(event) = s.events.pop_front() {
                    return Ok(Some(event));
                }
                if s.terminal {
                    self.ended = true;
                    return s.error.map_or(Ok(None), Err);
                }
            }
            n.await;
        }
    }
}
impl Drop for EventReceiver {
    fn drop(&mut self) {
        self.core.stop(None);
    }
}
impl IoOwner {
    pub fn request_stop(&self) {
        self.core.stop(None);
    }
    pub async fn shutdown(&self, deadline: Instant) -> IoShutdownReport {
        lifecycle::shutdown(&self.core, deadline).await
    }
}
impl Drop for IoOwner {
    fn drop(&mut self) {
        self.core.stop(None);
    }
}
macro_rules! redacted{($($t:ty),*)=>{$(impl fmt::Debug for $t{fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.write_str(concat!(stringify!($t),"(..)"))}})*}}
redacted!(
    Connection,
    EventReceiver,
    IoOwner,
    WriteIdentity,
    WriteProbe,
    Reservation,
    EncodedFrame,
    EnqueueError,
    WriteTicket,
    Received,
    Response
);
