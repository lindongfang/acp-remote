//! History port backed by the module's one original connection/receiver/operation registry.
use super::{
    ConnectionScope, FixtureOwnerHandle, ModuleCore, ModuleWriteFence, NativeReadOptions,
    NativeSessionKey, ReadCompletionProof, ReadEpoch, ReadResponseWait, Reservation,
    ResponseDeadline, RetainedAcpRaw, RpcId, SessionId, SessionSetup, SharedViews, ViewError,
    WriteTicket,
};
use crate::adapters::acp::{
    EncodedFrame, WriteProgress, WriteReport,
    raw_json::RawDocument,
    shared_view_read::{ReplayCompletionState, ReplayReadObservation, ReplayTransportState},
};
use crate::application::shared_views::{
    BoxReadFuture,
    budget::ReadReason,
    read_control::{
        NativeHistoryPort, NativeReadStatus, ReadBindingOutcome, ReadCancelError, ReadEnqueueError,
        ReadTransportState, ReplayReadStatus,
    },
};
use std::sync::{Arc, atomic::Ordering};

use super::super::shared_view_read::request_error as replay_request_error;
pub(super) struct ModuleHistoryBridge {
    pub core: Arc<ModuleCore>,
    pub owner: FixtureOwnerHandle,
    pub views: SharedViews<ConnectionScope, RetainedAcpRaw>,
}
pub struct ModuleReadBinding {
    scope: ConnectionScope,
    key: NativeSessionKey,
    setup: SessionSetup,
}
pub struct ModuleReadReservation {
    reservation: Reservation,
    frame: EncodedFrame,
    key: NativeSessionKey,
    read: ReadEpoch,
    options: NativeReadOptions,
}
pub struct ModuleReadOperation {
    rpc: RpcId,
    key: NativeSessionKey,
    last: Option<(ReplayReadObservation, bool)>,
    proof: Option<Arc<ReadCompletionProof<ConnectionScope, RetainedAcpRaw>>>,
}
impl NativeHistoryPort for ModuleHistoryBridge {
    type Scope = ConnectionScope;
    type Raw = RetainedAcpRaw;
    type Binding = ModuleReadBinding;
    type Reservation = ModuleReadReservation;
    type Operation = ModuleReadOperation;
    fn acquire_read_binding<'a>(
        &'a self,
        target: &'a NativeSessionKey,
        _reason: ReadReason,
    ) -> BoxReadFuture<'a, Result<ReadBindingOutcome<Self::Binding>, ViewError>> {
        Box::pin(async move {
            if self.core.terminal.load(Ordering::Acquire) || self.core.connection.is_closed() {
                return Err(ViewError::SourceReleased);
            }
            if target.source() != self.core.source
                || self.views.core.scope != self.core.connection.scope()
            {
                return Err(ViewError::InvalidScope);
            }
            let setup = self
                .core
                .targets
                .read()
                .map_err(|_| ViewError::ReadFailure)?
                .get(target)
                .cloned()
                .ok_or(ViewError::InvalidTarget)?;
            let strategy = self
                .core
                .connection
                .strategy()
                .ok_or(ViewError::ProtocolFailure)?;
            let raw = RawDocument::parse(
                serde_json::to_vec(target.native_id()).map_err(|_| ViewError::AllocationFailure)?,
            )
            .map_err(|_| ViewError::InvalidTarget)?;
            let session = SessionId::parse(raw.root()).map_err(|_| ViewError::InvalidTarget)?;
            // This actual negotiated constructor is the capability/configuration check, before budget selection.
            let _ = strategy
                .replay(session, &setup)
                .map_err(replay_request_error)?;
            Ok(ReadBindingOutcome::Replay(ModuleReadBinding {
                scope: self.core.connection.scope(),
                key: target.clone(),
                setup,
            }))
        })
    }
    fn scope<'a>(&self, binding: &'a Self::Binding) -> &'a ConnectionScope {
        &binding.scope
    }
    fn reserve_replay(
        &self,
        binding: &Self::Binding,
        read: ReadEpoch,
        options: NativeReadOptions,
    ) -> Result<Self::Reservation, ViewError> {
        if binding.scope != self.core.connection.scope()
            || self.core.terminal.load(Ordering::Acquire)
        {
            return Err(ViewError::SourceReplaced);
        }
        let raw = RawDocument::parse(
            serde_json::to_vec(binding.key.native_id())
                .map_err(|_| ViewError::AllocationFailure)?,
        )
        .map_err(|_| ViewError::InvalidTarget)?;
        let session = SessionId::parse(raw.root()).map_err(|_| ViewError::InvalidTarget)?;
        let request = self
            .core
            .connection
            .strategy()
            .ok_or(ViewError::ProtocolFailure)?
            .replay(session, &binding.setup)
            .map_err(replay_request_error)?;
        let response = match options.response_wait {
            ReadResponseWait::At(deadline) => ResponseDeadline::At(deadline),
            ReadResponseWait::Unbounded => ResponseDeadline::Unbounded,
        };
        let reservation = self
            .core
            .connection
            .reserve_request(request, response)
            .map_err(|_| ViewError::ReadFailure)?;
        let frame = reservation
            .encode()
            .map_err(|_| ViewError::ProtocolFailure)?;
        Ok(ModuleReadReservation {
            reservation,
            frame,
            key: binding.key.clone(),
            read,
            options,
        })
    }
    #[allow(
        clippy::too_many_lines,
        reason = "Original reservation/frame retention and pre-enqueue owner registration remain one linear handoff"
    )]
    fn enqueue_replay<'a>(
        &'a self,
        binding: &'a Self::Binding,
        reservation: Self::Reservation,
    ) -> BoxReadFuture<
        'a,
        Result<Self::Operation, ReadEnqueueError<Self::Reservation, Self::Operation>>,
    > {
        Box::pin(async move {
            if binding.scope != self.core.connection.scope() {
                return Err(ReadEnqueueError::RetainedReservation {
                    error: ViewError::InvalidScope,
                    reservation,
                });
            }
            let ModuleReadReservation {
                reservation,
                frame,
                key,
                read,
                options,
            } = reservation;
            let rpc = reservation.id().ok_or(ViewError::ProtocolFailure);
            let rpc = match rpc {
                Ok(rpc) => rpc.clone(),
                Err(error) => {
                    return Err(ReadEnqueueError::RetainedReservation {
                        error,
                        reservation: ModuleReadReservation {
                            reservation,
                            frame,
                            key,
                            read,
                            options,
                        },
                    });
                }
            };
            // Register the real operation before enqueue; the receiver never sees a synthetic receipt.
            let (transfer, adopted) = tokio::sync::oneshot::channel::<Arc<WriteTicket>>();
            let original = Arc::clone(&self.core);
            let observed_rpc = rpc.clone();
            if self
                .owner
                .spawn(async move {
                    if let Ok(ticket) = adopted.await {
                        let report = ticket.wait().await;
                        original.observe_write_terminal(&observed_rpc, report);
                    }
                    Ok(())
                })
                .is_err()
            {
                return Err(ReadEnqueueError::RetainedReservation {
                    error: ViewError::ShutdownUnconfirmed,
                    reservation: ModuleReadReservation {
                        reservation,
                        frame,
                        key,
                        read,
                        options,
                    },
                });
            }
            if let Err(error) = self
                .core
                .register(&reservation, Some(key.clone()), Some(read))
            {
                return Err(ReadEnqueueError::RetainedReservation {
                    error,
                    reservation: ModuleReadReservation {
                        reservation,
                        frame,
                        key,
                        read,
                        options,
                    },
                });
            }
            let ticket = match self.core.connection.enqueue(
                reservation,
                frame,
                Arc::new(ModuleWriteFence {
                    scope: binding.scope.clone(),
                    rpc: rpc.clone(),
                }),
                options.write_deadline,
            ) {
                Ok(ticket) => Arc::new(ticket),
                Err(error) => {
                    let (_, reservation, frame) = error.into_parts();
                    return Err(ReadEnqueueError::RetainedReservation {
                        error: ViewError::QueueRejected,
                        reservation: ModuleReadReservation {
                            reservation,
                            frame,
                            key,
                            read,
                            options,
                        },
                    });
                }
            };
            // This map cannot lose the registered slot: only the single pump mutates its facts.
            match self.core.operations.lock() {
                Ok(mut operations) => {
                    if let Some(operation) =
                        operations.iter_mut().find(|operation| operation.rpc == rpc)
                    {
                        operation.ticket = Some(Arc::clone(&ticket));
                    }
                }
                Err(poison) => {
                    if let Some(operation) = poison
                        .into_inner()
                        .iter_mut()
                        .find(|operation| operation.rpc == rpc)
                    {
                        operation.ticket = Some(Arc::clone(&ticket));
                    }
                }
            }
            self.core.enqueues.fetch_add(1, Ordering::AcqRel);
            let _ = transfer.send(ticket);
            Ok(ModuleReadOperation {
                rpc,
                key,
                last: None,
                proof: None,
            })
        })
    }
    fn observe<'a>(
        &'a self,
        operation: &'a mut Self::Operation,
    ) -> BoxReadFuture<'a, Result<NativeReadStatus<'a, ConnectionScope, RetainedAcpRaw>, ViewError>>
    {
        Box::pin(async move {
            loop {
                let changed = self.core.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                let projected = self.views.core.changed.notified();
                tokio::pin!(projected);
                projected.as_mut().enable();
                let (observation, proof) = {
                    let mut operations = self
                        .core
                        .operations
                        .lock()
                        .map_err(|_| ViewError::ReadFailure)?;
                    let original = operations
                        .iter_mut()
                        .find(|original| original.rpc == operation.rpc)
                        .ok_or(ViewError::InvalidStage)?;
                    let replay = original.replay.as_mut().ok_or(ViewError::InvalidStage)?;
                    (replay.observation(), original.proof.clone())
                };
                // Completion is the original real Replay receipt plus its processed raw
                // prefix. Cache release/publication and later IO loss cannot withdraw it.
                let published =
                    observation.completion() == ReplayCompletionState::Complete && proof.is_some();
                if let Some(proof) = proof.as_ref()
                    && proof.target() != &operation.key
                {
                    return Err(ViewError::InvalidTarget);
                }
                let state = (observation, published);
                if operation.last != Some(state) {
                    operation.last = Some(state);
                    if operation.proof.is_none() {
                        operation.proof = proof;
                    }
                    let transport = match observation.transport() {
                        ReplayTransportState::Reserved => ReadTransportState::Reserved,
                        ReplayTransportState::Queued => ReadTransportState::Queued,
                        ReplayTransportState::Writing => ReadTransportState::Writing,
                        ReplayTransportState::Written => ReadTransportState::Written,
                        ReplayTransportState::Unknown => ReadTransportState::Unknown,
                        ReplayTransportState::Zero => ReadTransportState::Zero,
                    };
                    let completion = if published
                        && observation.completion() == ReplayCompletionState::Complete
                    {
                        ReplayReadStatus::Completed(
                            operation
                                .proof
                                .as_deref()
                                .ok_or(ViewError::ProtocolFailure)?,
                        )
                    } else if observation.completion() == ReplayCompletionState::Incomplete {
                        ReplayReadStatus::Incomplete(
                            observation.failure().unwrap_or(ViewError::ReadFailure),
                        )
                    } else {
                        ReplayReadStatus::Pending
                    };
                    return Ok(NativeReadStatus {
                        transport,
                        completion,
                    });
                }
                // The original ACP response deadline owns its held-receipt/expiry rules.
                // A view timer must never retire a pre-deadline receipt still being decoded.
                tokio::select! {()=changed=>{},()=projected=>{}}
            }
        })
    }
    fn cancel_reserved(
        &self,
        reservation: Self::Reservation,
    ) -> Result<ReadTransportState, ReadCancelError<Self::Reservation>> {
        if reservation.reservation.scope() != self.core.connection.scope() {
            return Err(ReadCancelError {
                error: ViewError::InvalidScope,
                original: reservation,
            });
        }
        let probe = reservation.reservation.probe();
        let rpc = reservation.reservation.id().cloned();
        // This original unqueued reservation's Drop invokes its real cancel path. Retain its
        // actual probe rather than fabricate Zero or construct a replacement reservation.
        drop(reservation);
        if let (Some(rpc), WriteProgress::Finished(report)) = (rpc, probe.progress()) {
            self.core.observe_write_terminal(&rpc, report);
        }
        Ok(match probe.progress() {
            WriteProgress::Reserved => ReadTransportState::Reserved,
            WriteProgress::Queued => ReadTransportState::Queued,
            WriteProgress::Writing { .. } | WriteProgress::Flushing { .. } => {
                ReadTransportState::Writing
            }
            WriteProgress::Finished(WriteReport::Zero { .. }) => ReadTransportState::Zero,
            WriteProgress::Finished(WriteReport::Unknown { .. }) => ReadTransportState::Unknown,
            WriteProgress::Finished(WriteReport::Written) => ReadTransportState::Written,
        })
    }
}
