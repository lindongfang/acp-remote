//! Genuine source-management reservations on the original ACP connection and sole receiver.
#![allow(
    dead_code,
    clippy::significant_drop_tightening,
    clippy::unused_self,
    reason = "Scaffolding and trait-shaped adapter methods preserve explicit lock scopes"
)]
use super::{
    AcpNativeAdapter, AcpSource, Arc, ConnectionScope, Context, EncodedFrame, ExtensionFields,
    FenceRejection, HashMap, Incoming, Instant, Mutex, ObservationLease, OutboundKind, PollBridge,
    PollIo, RequestKind, Reservation, Response, SessionSetup, WriteFence, WriteIdentity,
    WriteProbe, WriteStep, WriteTicket, d, error, n, raw_json, reject, scalar, step, wait,
};
use crate::application::native_dispatch::source_sessions as ss;
use crate::application::shared_views::RetainedRaw;
use std::sync::Weak;

pub(super) struct SourceReplyTracker {
    scope: ConnectionScope,
    pending: Mutex<HashMap<d::RpcScalar, ss::SourceRequestKind>>,
}
impl SourceReplyTracker {
    pub(super) fn new(scope: ConnectionScope) -> Self {
        Self {
            scope,
            pending: Mutex::new(HashMap::new()),
        }
    }
    fn register(
        self: &Arc<Self>,
        actual: &Reservation,
        kind: ss::SourceRequestKind,
    ) -> Result<SourceReplyTracking, d::DispatchError> {
        let rpc = actual
            .id()
            .map(scalar)
            .ok_or(d::DispatchError::InvalidPromptReservation)?;
        if actual.scope() != self.scope {
            return Err(d::DispatchError::InvalidScope);
        }
        let expected = match kind {
            ss::SourceRequestKind::Create => matches!(
                actual.kind(),
                OutboundKind::Request(RequestKind::NewSession)
            ),
            ss::SourceRequestKind::Discover => matches!(
                actual.kind(),
                OutboundKind::Request(RequestKind::ListSessions)
            ),
        };
        if !expected {
            return Err(d::DispatchError::InvalidScope);
        }
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| d::DispatchError::RequiredStateUnavailable)?;
        if pending.contains_key(&rpc) {
            return Err(d::DispatchError::Conflict);
        }
        pending
            .try_reserve(1)
            .map_err(|_| d::DispatchError::StorageUnavailable)?;
        pending.insert(rpc.clone(), kind);
        Ok(SourceReplyTracking {
            tracker: Arc::downgrade(self),
            scope: self.scope.clone(),
            rpc,
            kind,
        })
    }
}
#[derive(Clone)]
pub struct SourceReplyTracking {
    tracker: Weak<SourceReplyTracker>,
    scope: ConnectionScope,
    rpc: d::RpcScalar,
    kind: ss::SourceRequestKind,
}
impl SourceReplyTracking {
    fn remove(&self) {
        if let Some(tracker) = self.tracker.upgrade() {
            if tracker.scope != self.scope {
                return;
            }
            if let Ok(mut pending) = tracker.pending.lock()
                && pending.get(&self.rpc) == Some(&self.kind)
            {
                pending.remove(&self.rpc);
            }
        }
    }
}
pub struct AcpSourceSessionReservation {
    actual: Reservation,
    rpc: d::RpcScalar,
    tracking: SourceReplyTracking,
    kind: ss::SourceRequestKind,
}
impl n::ReservedPrompt for AcpSourceSessionReservation {
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
impl n::LifecycleReservation for AcpSourceSessionReservation {
    fn kind(&self) -> n::LifecycleKind {
        match self.kind {
            ss::SourceRequestKind::Create => n::LifecycleKind::Create,
            ss::SourceRequestKind::Discover => n::LifecycleKind::Discover,
        }
    }
}
struct SourceSessionFence {
    scope: ConnectionScope,
    rpc: d::RpcScalar,
    kind: ss::SourceRequestKind,
    boundary: Arc<dyn n::LifecycleWriteBoundary<ConnectionScope>>,
}
impl WriteFence for SourceSessionFence {
    fn check_and_poll(
        &self,
        write: &WriteIdentity,
        s: WriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn PollIo,
    ) -> Result<(), FenceRejection> {
        let expected = match self.kind {
            ss::SourceRequestKind::Create => {
                matches!(write.kind(), OutboundKind::Request(RequestKind::NewSession))
            }
            ss::SourceRequestKind::Discover => matches!(
                write.kind(),
                OutboundKind::Request(RequestKind::ListSessions)
            ),
        };
        if !expected
            || write.scope() != self.scope
            || write.id().map(scalar).as_ref() != Some(&self.rpc)
        {
            return Err(FenceRejection::StaleTarget);
        }
        let kind = match self.kind {
            ss::SourceRequestKind::Create => n::LifecycleKind::Create,
            ss::SourceRequestKind::Discover => n::LifecycleKind::Discover,
        };
        self.boundary
            .check_and_poll(
                &n::LifecycleWriteIdentity {
                    scope: self.scope.clone(),
                    rpc: self.rpc.clone(),
                    kind,
                },
                step(s),
                cx,
                &mut PollBridge(io),
            )
            .map_err(reject)
    }
}
/// Keeps the original Received (including the typed listing) and its exact original fragment.
#[derive(Clone)]
pub struct AcpSourceResult {
    received: super::super::Received,
    fragment: raw_json::OwnedRawValue,
}
impl std::fmt::Debug for AcpSourceResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AcpSourceResult(..)")
    }
}
impl RetainedRaw for AcpSourceResult {
    fn len(&self) -> usize {
        self.fragment.as_ref().fragment().len()
    }
    fn bytes(&self) -> &[u8] {
        self.fragment.as_ref().fragment().as_bytes()
    }
}
impl AcpNativeAdapter {
    fn reserve_source_request(
        &self,
        source: &AcpSource,
        request: super::super::Request,
        kind: ss::SourceRequestKind,
        w: n::ResponseWait,
    ) -> Result<AcpSourceSessionReservation, d::DispatchError> {
        let connection = source.connection()?;
        let actual = connection
            .reserve_request(request, wait(w))
            .map_err(error)?;
        let tracking = source.0.source_replies.register(&actual, kind)?;
        Ok(AcpSourceSessionReservation {
            rpc: tracking.rpc.clone(),
            actual,
            tracking,
            kind,
        })
    }
}
impl ss::NativeSourceSessionPort for AcpNativeAdapter {
    type SessionReservation = AcpSourceSessionReservation;
    type ResultRaw = AcpSourceResult;
    type ReplyTracking = SourceReplyTracking;
    fn reserve_discover(
        &self,
        source: &AcpSource,
        setup: &SessionSetup,
        cursor: Option<&str>,
        w: n::ResponseWait,
    ) -> Result<AcpSourceSessionReservation, d::DispatchError> {
        let connection = source.connection()?;
        let strategy = connection
            .strategy()
            .ok_or(d::DispatchError::ProtocolUnavailable)?;
        if strategy.version() != setup.wire() {
            return Err(d::DispatchError::InvalidConfiguration);
        }
        let request = strategy
            .list_sessions(
                Some(setup.cwd()),
                cursor,
                ExtensionFields::empty().map_err(|_| d::DispatchError::InvalidConfiguration)?,
            )
            .map_err(error)?;
        self.reserve_source_request(source, request, ss::SourceRequestKind::Discover, w)
    }
    fn reserve_create(
        &self,
        source: &AcpSource,
        setup: &SessionSetup,
        w: n::ResponseWait,
    ) -> Result<AcpSourceSessionReservation, d::DispatchError> {
        let connection = source.connection()?;
        let strategy = connection
            .strategy()
            .ok_or(d::DispatchError::ProtocolUnavailable)?;
        let request = strategy.new_session(setup).map_err(error)?;
        self.reserve_source_request(source, request, ss::SourceRequestKind::Create, w)
    }
    fn enqueue_source_session(
        &self,
        source: &AcpSource,
        r: AcpSourceSessionReservation,
        frame: EncodedFrame,
        boundary: Arc<dyn n::LifecycleWriteBoundary<ConnectionScope>>,
        deadline: Instant,
    ) -> Result<WriteTicket, n::EnqueueRejected<AcpSourceSessionReservation, EncodedFrame>> {
        let connection = match source.connection() {
            Ok(value) => value,
            Err(e) => {
                return Err(n::EnqueueRejected {
                    error: e,
                    reservation: r,
                    frame,
                });
            }
        };
        let fence = Arc::new(SourceSessionFence {
            scope: r.actual.scope(),
            rpc: r.rpc.clone(),
            kind: r.kind,
            boundary,
        });
        connection
            .enqueue(r.actual, frame, fence, deadline)
            .map_err(|rejected| {
                let (e, actual, frame) = rejected.into_parts();
                n::EnqueueRejected {
                    error: error(e),
                    reservation: AcpSourceSessionReservation {
                        actual,
                        rpc: r.rpc,
                        tracking: r.tracking,
                        kind: r.kind,
                    },
                    frame,
                }
            })
    }
    #[allow(
        clippy::suspicious_operation_groupings,
        reason = "Matches SourceReplyBinding fields against Received envelope metadata"
    )]
    fn retain_source_reply(
        &self,
        lease: &ObservationLease,
        b: &ss::SourceReplyBinding<ConnectionScope>,
    ) -> Result<ss::OriginalSourceReply<AcpSourceResult>, d::DispatchError> {
        let received = lease.received();
        if received.scope() != *b.scope()
            || received.sequence() != b.sequence()
            || received.receipt_order() != b.receipt()
        {
            return Err(d::DispatchError::InvalidScope);
        }
        let Incoming::Response { id, response, .. } = received.incoming() else {
            return Err(d::DispatchError::ProtocolRejected);
        };
        if scalar(id) != *b.rpc() {
            return Err(d::DispatchError::InvalidScope);
        }
        let mut deferred_decode = None;
        let response = match (b.kind(), response) {
            (ss::SourceRequestKind::Create, Response::NewSession(session)) => {
                ss::SourceObservedResponse::Created(session.shared_text())
            }
            (ss::SourceRequestKind::Discover, Response::Sessions(_)) => {
                ss::SourceObservedResponse::Sessions
            }
            (ss::SourceRequestKind::Discover, Response::DeferredSessions(strategy)) => {
                let strategy = strategy.clone();
                let raw_message = received.clone();
                deferred_decode = Some(Box::new(move || {
                    let result = raw_message
                        .raw()
                        .field("result")
                        .map_err(error)?
                        .ok_or(d::DispatchError::ProtocolRejected)?;
                    let listing = strategy.session_listing(result).map_err(error)?;
                    drop(listing);
                    Ok(())
                })
                    as Box<dyn FnOnce() -> Result<(), d::DispatchError> + Send>);
                ss::SourceObservedResponse::Sessions
            }
            (_, Response::RemoteError { code }) => ss::SourceObservedResponse::RemoteError(*code),
            _ => return Err(d::DispatchError::ProtocolRejected),
        };
        let field = if matches!(response, ss::SourceObservedResponse::RemoteError(_)) {
            "error"
        } else {
            "result"
        };
        let fragment = received
            .raw()
            .field(field)
            .map_err(error)?
            .ok_or(d::DispatchError::ProtocolRejected)?
            .to_owned();
        Ok(ss::OriginalSourceReply {
            raw: AcpSourceResult {
                received: received.clone(),
                fragment,
            },
            response,
            deferred_decode,
        })
    }
    fn source_reply_tracking(
        &self,
        reservation: &AcpSourceSessionReservation,
    ) -> SourceReplyTracking {
        reservation.tracking.clone()
    }
    fn acknowledge_source_reply(
        &self,
        tracking: &SourceReplyTracking,
        binding: &ss::SourceReplyBinding<ConnectionScope>,
    ) {
        if tracking.scope == *binding.scope()
            && tracking.rpc == *binding.rpc()
            && tracking.kind == binding.kind()
        {
            tracking.remove();
        }
    }
    fn finish_source_reply_tracking(
        &self,
        tracking: &SourceReplyTracking,
        end: &ss::SourceReplyEnd<ConnectionScope>,
    ) {
        if tracking.scope == *end.scope() {
            tracking.remove();
        }
    }
}
