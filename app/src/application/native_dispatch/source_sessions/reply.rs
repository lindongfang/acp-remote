//! The same original Received is installed once and kept through its actual ACK consumption.
#![allow(
    clippy::option_if_let_else,
    clippy::significant_drop_tightening,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::super::{DispatchError, NativeApprovalPort, NativeIncoming};
use super::{
    Arc, Mutex, NativeSessionKey, NativeSourceSessionPort, OriginalCreatedClaim,
    OriginalSourceResult, PreSourceProbe, QueueRuntimeIdentity, RegistryOwnerScope, RetainedRaw,
    RpcScalar, SourceAckState, SourceObservedResponse, SourceReplyBinding, SourceReplyEnd,
    SourceRequestKind, SourceResponseFact, SourceResponseKind, SourceResultPublisher,
    SourceSqlInventory, TransportFact,
};
use crate::application::document_disposal::{InstalledDocumentHandoff, PendingDocumentHandoff};
use std::sync::Weak;
use tokio::sync::Notify;
mod sealed {
    pub trait Sealed {}
}
pub(super) trait SourceCpuSpawner: Send + Sync {
    fn spawn(
        &self,
        work: Box<dyn FnOnce() + Send>,
        on_failure: Box<dyn FnOnce() + Send>,
    ) -> Result<(), DispatchError>;
}
pub(in super::super) trait SourceReplyAckUse: Send + Sync {
    fn acknowledged(&self);
    fn rejected(&self);
}
pub(in super::super) trait SourceReplyOperation<S, L>:
    sealed::Sealed + Send + Sync
{
    fn matches_incoming(&self, scope: &S, incoming: &NativeIncoming) -> bool;
    fn apply(
        self: Arc<Self>,
        lease: &L,
        sequence: u64,
        receipt: u64,
    ) -> Result<Box<dyn SourceReplyAckUse>, DispatchError>;
    fn receiver_ended(&self);
    fn source_closed(&self, proof: &Arc<super::super::SourceClosureProof>);
    fn close_stamp(&self) -> Result<[u8; 32], DispatchError>;
}
struct ReplyState<R: RetainedRaw> {
    handoff: Option<PendingDocumentHandoff<R>>,
    response: Option<SourceResponseFact>,
    result: Weak<OriginalSourceResult<R>>,
    ack: SourceAckState,
    decode_pending: bool,
    decode_failed: bool,
    receiver_ended: bool,
    ownership_failed: bool,
    never_enqueued: bool,
    confirmed_zero: bool,
}
pub(super) struct SourceReplyCell<P: NativeApprovalPort + NativeSourceSessionPort> {
    pub(super) protocol: Arc<P>,
    pub(super) source: Arc<PreSourceProbe>,
    pub(super) scope: P::Scope,
    pub(super) rpc: RpcScalar,
    pub(super) kind: SourceRequestKind,
    pub(super) frame_length: u64,
    tracking: P::ReplyTracking,
    environment: Arc<super::super::NativeEnvironment<P>>,
    owner: RegistryOwnerScope,
    queue: QueueRuntimeIdentity,
    publisher: Arc<dyn SourceResultPublisher<P::ResultRaw>>,
    cpu: Arc<dyn SourceCpuSpawner>,
    sql: Arc<SourceSqlInventory>,
    state: Mutex<ReplyState<P::ResultRaw>>,
    pub(super) changed: Arc<Notify>,
}
impl<P: NativeApprovalPort + NativeSourceSessionPort> SourceReplyCell<P> {
    #[expect(
        clippy::too_many_arguments,
        reason = "SourceReplyCell binds all original scope, disposal, CPU, SQL and publisher owners once at cell creation."
    )]
    pub(super) fn new(
        protocol: Arc<P>,
        source: Arc<PreSourceProbe>,
        scope: P::Scope,
        rpc: RpcScalar,
        kind: SourceRequestKind,
        frame_length: u64,
        tracking: P::ReplyTracking,
        environment: Arc<super::super::NativeEnvironment<P>>,
        owner: RegistryOwnerScope,
        queue: QueueRuntimeIdentity,
        publisher: Arc<dyn SourceResultPublisher<P::ResultRaw>>,
        cpu: Arc<dyn SourceCpuSpawner>,
        sql: Arc<SourceSqlInventory>,
        handoff: PendingDocumentHandoff<P::ResultRaw>,
        changed: Arc<Notify>,
    ) -> Self {
        Self {
            protocol,
            source,
            scope,
            rpc,
            kind,
            frame_length,
            tracking,
            environment,
            owner,
            queue,
            publisher,
            cpu,
            sql,
            state: Mutex::new(ReplyState {
                handoff: Some(handoff),
                response: None,
                result: Weak::new(),
                ack: SourceAckState::Pending,
                decode_pending: false,
                decode_failed: false,
                receiver_ended: false,
                ownership_failed: false,
                never_enqueued: false,
                confirmed_zero: false,
            }),
            changed,
        }
    }
    pub(super) fn response(&self) -> Option<SourceResponseFact> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .response
            .clone()
    }
    pub(super) fn result(&self) -> Weak<OriginalSourceResult<P::ResultRaw>> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .result
            .clone()
    }
    pub(super) fn abandon_unenqueued(
        &self,
        reservation: &P::SessionReservation,
    ) -> Result<(), DispatchError> {
        use super::super::ReservedPrompt;
        if reservation.scope() != self.scope || reservation.rpc() != &self.rpc {
            return Err(DispatchError::InvalidScope);
        }
        let handoff = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.never_enqueued = true;
            state.handoff.take()
        };
        drop(handoff);
        self.changed.notify_waiters();
        Ok(())
    }
    pub(super) fn has_pending_handoff(&self) -> bool {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.handoff.is_some() || state.decode_pending
    }
    pub(super) fn release_after_original_zero(
        &self,
        ticket: &P::Ticket,
    ) -> Result<(), DispatchError> {
        use super::super::PromptTicket;
        if !matches!(
            ticket.progress(),
            super::super::TransportProgress::Finished(TransportFact::Zero { .. })
        ) {
            return Err(DispatchError::RequiredStateUnavailable);
        }
        let handoff = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.confirmed_zero = true;
            state.handoff.take()
        };
        drop(handoff);
        self.changed.notify_waiters();
        Ok(())
    }
    pub(super) fn ended(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .receiver_ended
    }
    pub(super) fn decode_pending(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .decode_pending
    }
    pub(super) fn decode_failed(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .decode_failed
    }
    pub(super) fn ownership_failed(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .ownership_failed
    }
    /// Only after the original receiver terminal and actual IO join can an empty reservation retire.
    pub(super) fn release_empty_after_io_join(&self) {
        let handoff = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .handoff
            .take();
        drop(handoff);
    }
    fn mark_ack(&self, ack: SourceAckState, binding: &SourceReplyBinding<P::Scope>) {
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.ack = ack;
            if let Some(response) = state.response.as_mut() {
                response.ack = ack;
            }
        }
        if ack == SourceAckState::Confirmed {
            self.protocol
                .acknowledge_source_reply(&self.tracking, binding);
        }
        self.changed.notify_waiters();
    }
    fn mark_decode_failed(&self) {
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.decode_pending = false;
            state.decode_failed = true;
        }
        self.changed.notify_waiters();
    }
    fn finish_install(
        &self,
        handoff: PendingDocumentHandoff<P::ResultRaw>,
        raw: P::ResultRaw,
        observed_response: SourceObservedResponse,
        sequence: u64,
        receipt: u64,
    ) -> Result<InstalledDocumentHandoff<P::ResultRaw>, DispatchError> {
        let total_bytes =
            u64::try_from(raw.len()).map_err(|_| DispatchError::LengthUnrepresentable)?;
        let installed = handoff.install(raw);
        let root = OriginalSourceResult::from_original(
            installed.payload_slot(),
            self.source.clone(),
            self.owner.clone(),
            self.queue.clone(),
            total_bytes,
        );
        let (kind, native, code) = match observed_response {
            SourceObservedResponse::Created(session) => {
                (SourceResponseKind::Created, Some(session), None)
            }
            SourceObservedResponse::Sessions => (SourceResponseKind::Sessions, None, None),
            SourceObservedResponse::RemoteError(code) => {
                (SourceResponseKind::RemoteError, None, Some(code))
            }
        };
        let key = native.as_ref().and_then(|sid| {
            NativeSessionKey::from_config(Some(self.source.metadata().source_id), sid).ok()
        });
        let ownership_failed = if let Some(key) = &key {
            self.environment
                .claim_created(&OriginalCreatedClaim {
                    source: self.source.clone(),
                    native: key.clone(),
                })
                .is_err()
        } else {
            false
        };
        let response_evidence = super::super::source::metadata_stamp(
            b"native-source-session-reply",
            self.source.metadata().instance_id,
            sequence,
            receipt,
        );
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let ack = state.ack;
            state.response = Some(SourceResponseFact {
                kind,
                native_session_id: native,
                native_key_available: key.is_some(),
                remote_error_code: code,
                sequence,
                receipt,
                evidence: response_evidence,
                result_bytes: total_bytes,
                ack,
            });
            state.result = Arc::downgrade(&root);
            state.ownership_failed = ownership_failed;
            state.decode_pending = false;
        }
        self.publisher.publish(root);
        self.changed.notify_waiters();
        Ok(installed)
    }
}
impl<P: NativeApprovalPort + NativeSourceSessionPort> sealed::Sealed for SourceReplyCell<P> {}
struct AckUse<P: NativeApprovalPort + NativeSourceSessionPort> {
    cell: Arc<SourceReplyCell<P>>,
    binding: SourceReplyBinding<P::Scope>,
    _installed: Option<InstalledDocumentHandoff<P::ResultRaw>>,
}
impl<P: NativeApprovalPort + NativeSourceSessionPort> SourceReplyAckUse for AckUse<P> {
    fn acknowledged(&self) {
        self.cell.mark_ack(SourceAckState::Confirmed, &self.binding);
    }
    fn rejected(&self) {
        self.cell.mark_ack(SourceAckState::Rejected, &self.binding);
    }
}
impl<P: NativeApprovalPort + NativeSourceSessionPort> SourceReplyOperation<P::Scope, P::Lease>
    for SourceReplyCell<P>
{
    fn matches_incoming(&self, scope: &P::Scope, incoming: &NativeIncoming) -> bool {
        if scope != &self.scope {
            return false;
        }
        match (self.kind, incoming) {
            (SourceRequestKind::Create, NativeIncoming::Created { rpc, .. })
            | (SourceRequestKind::Discover, NativeIncoming::Sessions { rpc })
            | (_, NativeIncoming::RemoteError { rpc, .. }) => rpc == &self.rpc,
            _ => false,
        }
    }
    fn apply(
        self: Arc<Self>,
        lease: &P::Lease,
        sequence: u64,
        receipt: u64,
    ) -> Result<Box<dyn SourceReplyAckUse>, DispatchError> {
        let binding = SourceReplyBinding {
            scope: self.scope.clone(),
            rpc: self.rpc.clone(),
            kind: self.kind,
            sequence,
            receipt,
        };
        let handoff = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?;
            if state.response.is_some() || state.decode_pending || state.decode_failed {
                return Err(DispatchError::ObservationOrder);
            }
            state
                .handoff
                .take()
                .ok_or(DispatchError::ObservationOrder)?
        };
        let original = match self.protocol.retain_source_reply(lease, &binding) {
            Ok(value) => value,
            Err(error) => {
                self.state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .handoff = Some(handoff);
                return Err(error);
            }
        };
        if let Some(decode) = original.deferred_decode {
            {
                let mut state = self
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state.decode_pending = true;
                state.ack = SourceAckState::Pending;
            }
            let cell = self.clone();
            let failed_cell = self.clone();
            let raw = original.raw;
            let response = original.response;
            let handoff_slot = Arc::new(Mutex::new(Some(handoff)));
            let handoff_work = handoff_slot.clone();
            let spawned = self.cpu.spawn(
                Box::new(move || {
                    #[cfg(test)]
                    if let Some(barrier) = cell.sql.take_cpu_decode_barrier() {
                        barrier.pause_blocking();
                    }
                    let handoff = handoff_work
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .take();
                    if decode() == Ok(()) {
                        if let Some(handoff) = handoff {
                            match cell.finish_install(handoff, raw, response, sequence, receipt) {
                                Ok(installed) => drop(installed),
                                Err(_) => cell.mark_decode_failed(),
                            }
                        } else {
                            drop(raw);
                            cell.mark_decode_failed();
                        }
                    } else {
                        #[cfg(test)]
                        if let Some(barrier) = cell.sql.take_cpu_retire_barrier() {
                            barrier.pause_blocking();
                        }
                        drop(raw);
                        drop(handoff);
                        cell.mark_decode_failed();
                    }
                }),
                Box::new(move || {
                    failed_cell.mark_decode_failed();
                }),
            );
            if let Err(error) = spawned {
                let mut state = self
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state.decode_pending = false;
                state.handoff = handoff_slot
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take();
                return Err(error);
            }
            return Ok(Box::new(AckUse {
                cell: self,
                binding,
                _installed: None,
            }));
        }
        let installed =
            self.finish_install(handoff, original.raw, original.response, sequence, receipt)?;
        Ok(Box::new(AckUse {
            cell: self,
            binding,
            _installed: Some(installed),
        }))
    }
    fn receiver_ended(&self) {
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.receiver_ended = true;
            if state.ack == SourceAckState::Pending {
                state.ack = SourceAckState::Unknown;
            }
            if let Some(response) = state.response.as_mut()
                && response.ack == SourceAckState::Pending
            {
                response.ack = SourceAckState::Unknown;
            }
        }
        self.changed.notify_waiters();
    }
    fn source_closed(&self, proof: &Arc<super::super::SourceClosureProof>) {
        if self.source.same(&proof.source) {
            self.protocol.finish_source_reply_tracking(
                &self.tracking,
                &SourceReplyEnd {
                    scope: self.scope.clone(),
                    original: proof.clone(),
                },
            );
        }
    }
    fn close_stamp(&self) -> Result<[u8; 32], DispatchError> {
        use sha2::{Digest, Sha256};
        let state = self
            .state
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?;
        if state.handoff.is_some()
            || state.decode_pending
            || state
                .response
                .as_ref()
                .is_some_and(|response| response.ack == SourceAckState::Pending)
        {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        let mut hash = Sha256::new();
        hash.update(b"source-session-close-v1");
        hash.update(self.source.metadata().instance_id.encode());
        hash.update(self.frame_length.to_be_bytes());
        hash.update([match self.kind {
            SourceRequestKind::Create => 1,
            SourceRequestKind::Discover => 2,
        }]);
        match &self.rpc {
            RpcScalar::Integer(value) => {
                hash.update([0]);
                hash.update(value.to_be_bytes());
            }
            RpcScalar::String(value) => {
                hash.update([1]);
                hash.update(value.as_bytes());
            }
        }
        if let Some(response) = &state.response {
            hash.update([1]);
            hash.update(response.evidence);
            hash.update([match response.ack {
                SourceAckState::Pending => 0,
                SourceAckState::Confirmed => 1,
                SourceAckState::Rejected => 2,
                SourceAckState::Unknown => 3,
            }]);
        } else {
            if !state.receiver_ended
                && !state.never_enqueued
                && !state.confirmed_zero
                && !state.decode_failed
            {
                return Err(DispatchError::ResourceUnconfirmed);
            }
            hash.update([
                0,
                u8::from(state.never_enqueued),
                u8::from(state.confirmed_zero),
                u8::from(state.decode_failed),
            ]);
        }
        Ok(hash.finalize().into())
    }
}
