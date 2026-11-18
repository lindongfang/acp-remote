//! Real Replay objects use the original source job registrar and original ACP writer.
use super::{
    AcpNativeAdapter, AcpSourceViews, Arc, ConnectionScope, Mutex, NativeSessionKey, Received,
    Reservation, RetainedAcpRaw, ViewError,
};
use crate::{
    adapters::acp::{
        AcpError, EncodedFrame, FenceRejection, Incoming, OutboundKind, PollIo, RequestKind,
        Response, ResponseDeadline, RpcId, SessionId, WriteFence, WriteIdentity, WriteProgress,
        WriteReport, WriteStep, WriteTicket,
        native_dispatch::{PollBridge, reject},
        raw_json::RawDocument,
        shared_view_read::{
            AcpReplayRead, ReplayCompletionState, ReplayReadObservation, ReplayTransportState,
            request_error,
        },
    },
    application::{
        native_dispatch::{
            SourceReadCapability,
            read_access::{
                OwnedReadTicketJob, ReadTicketTaskReceipt, ReadWriteBoundary, ReadWriteIdentity,
            },
        },
        shared_views::{
            BoxReadFuture,
            budget::{NativeReadOptions, ReadReason, ReadResponseWait},
            ingress::ReadFailureEvidence,
            read_control::{
                NativeHistoryPort, NativeReadStatus, ReadBindingOutcome, ReadCancelError,
                ReadCompletionProof, ReadEnqueueError, ReadTransportState, ReplayReadStatus,
                RpcScalar,
            },
        },
    },
    domain::shared_view::ReadEpoch,
};
use std::{sync::Weak, task::Context};

pub(super) struct NativeHistoryBridge {
    pub(super) attachment: Weak<AcpSourceViews>,
}
pub(super) struct NativeReadBinding {
    capability: Arc<SourceReadCapability<AcpNativeAdapter>>,
}
pub(super) struct NativeReplayReservation {
    objects: Arc<Mutex<Option<(Reservation, EncodedFrame)>>>,
    probe: crate::adapters::acp::WriteProbe,
    capability: Arc<SourceReadCapability<AcpNativeAdapter>>,
    read: ReadEpoch,
    options: NativeReadOptions,
    operation: Option<Arc<ReplayOperation>>,
}
pub(super) struct NativeReplayOperation {
    original: Arc<ReplayOperation>,
    last: Option<(ReplayReadObservation, bool)>,
    proof: Option<Arc<ReadCompletionProof<ConnectionScope, RetainedAcpRaw>>>,
}
pub(super) struct ReplayOperation {
    objects: Arc<Mutex<Option<(Reservation, EncodedFrame)>>>,
    rpc: RpcId,
    key: NativeSessionKey,
    state: Mutex<ReplayState>,
    ticket: Mutex<Option<Arc<WriteTicket>>>,
    task: Mutex<Option<ReadTicketTaskReceipt>>,
}
struct ReplayState {
    read: AcpReplayRead,
    marked: bool,
    proof: Option<Arc<ReadCompletionProof<ConnectionScope, RetainedAcpRaw>>>,
}
struct ReplayFence {
    scope: ConnectionScope,
    rpc: RpcId,
    key: NativeSessionKey,
    native: Arc<dyn ReadWriteBoundary<ConnectionScope>>,
}
impl WriteFence for ReplayFence {
    fn check_and_poll(
        &self,
        identity: &WriteIdentity,
        step: WriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn PollIo,
    ) -> Result<(), FenceRejection> {
        if identity.scope() != self.scope
            || identity.id() != Some(&self.rpc)
            || !matches!(identity.kind(), OutboundKind::Request(RequestKind::Replay(session)) if session.as_str()==self.key.native_id())
        {
            return Err(FenceRejection::StaleTarget);
        }
        let step = match step {
            WriteStep::Chunk { offset, total } => {
                crate::application::native_dispatch::DispatchWriteStep::Chunk { offset, total }
            }
            WriteStep::Flush { total } => {
                crate::application::native_dispatch::DispatchWriteStep::Flush { total }
            }
        };
        self.native
            .check_and_poll(
                &ReadWriteIdentity {
                    scope: self.scope.clone(),
                    rpc: scalar(&self.rpc),
                    native: self.key.clone(),
                },
                step,
                cx,
                &mut PollBridge(io),
            )
            .map_err(reject)
    }
}
fn scalar(rpc: &RpcId) -> crate::domain::native_dispatch::RpcScalar {
    match rpc {
        RpcId::Integer(value) => crate::domain::native_dispatch::RpcScalar::Integer(*value),
        RpcId::String(value) => crate::domain::native_dispatch::RpcScalar::String(value.clone()),
    }
}
fn read_scalar(rpc: &RpcId) -> RpcScalar {
    match rpc {
        RpcId::Integer(value) => RpcScalar::Integer(*value),
        RpcId::String(value) => RpcScalar::String(value.clone()),
    }
}
fn session(key: &NativeSessionKey) -> Result<SessionId, ViewError> {
    let raw = RawDocument::parse(
        serde_json::to_vec(key.native_id()).map_err(|_| ViewError::AllocationFailure)?,
    )
    .map_err(|_| ViewError::InvalidTarget)?;
    SessionId::parse(raw.root()).map_err(|_| ViewError::InvalidTarget)
}
pub async fn admit_replay_capacity(
    capability: &SourceReadCapability<AcpNativeAdapter>,
) -> Result<(), ViewError> {
    capability.require_current()?;
    if capability.is_child_auto_delivered() {
        return Ok(());
    }
    let connection = capability
        .source()
        .connection()
        .map_err(ViewError::NativeFailure)?;
    let strategy = connection.strategy().ok_or(ViewError::ProtocolFailure)?;
    match strategy.replay(session(capability.native())?, capability.setup()) {
        Ok(_) => capability.reserve_replay_capacity().await,
        Err(AcpError::UnsupportedCapability) if capability.is_source_only() => Ok(()),
        Err(error) => Err(request_error(error)),
    }
}
const fn transport(progress: WriteProgress) -> ReadTransportState {
    match progress {
        WriteProgress::Reserved => ReadTransportState::Reserved,
        WriteProgress::Queued => ReadTransportState::Queued,
        WriteProgress::Writing { .. } | WriteProgress::Flushing { .. } => {
            ReadTransportState::Writing
        }
        WriteProgress::Finished(WriteReport::Written) => ReadTransportState::Written,
        WriteProgress::Finished(WriteReport::Zero { .. }) => ReadTransportState::Zero,
        WriteProgress::Finished(WriteReport::Unknown { .. }) => ReadTransportState::Unknown,
    }
}
impl NativeHistoryPort for NativeHistoryBridge {
    type Scope = ConnectionScope;
    type Raw = RetainedAcpRaw;
    type Binding = NativeReadBinding;
    type Reservation = NativeReplayReservation;
    type Operation = NativeReplayOperation;
    fn acquire_read_binding<'a>(
        &'a self,
        target: &'a NativeSessionKey,
        reason: ReadReason,
    ) -> BoxReadFuture<'a, Result<ReadBindingOutcome<Self::Binding>, ViewError>> {
        let resources = self
            .attachment
            .upgrade()
            .map(|attachment| Arc::clone(&attachment.resources));
        Box::pin(async move {
            let resources = resources.ok_or(ViewError::CaptureUnavailable)?;
            let capability = resources
                .acquire_existing(target, reason)
                .await?
                .capability();
            capability.require_current()?;
            if capability.is_child_auto_delivered() {
                return Ok(ReadBindingOutcome::CurrentOnlyWithoutReplay);
            }
            let connection = capability
                .source()
                .connection()
                .map_err(|_| ViewError::SourceReleased)?;
            if connection.scope() != *capability.scope() {
                return Err(ViewError::InvalidScope);
            }
            let strategy = connection.strategy().ok_or(ViewError::ProtocolFailure)?;
            let (_, configured_wire) = capability.configuration_binding();
            if strategy.version().number()
                != u64::try_from(configured_wire.number())
                    .map_err(|_| ViewError::InvalidConfiguration)?
            {
                return Err(ViewError::InvalidConfiguration);
            }
            match strategy.replay(session(target)?, capability.setup()) {
                Ok(_) => {
                    capability.reserve_replay_capacity().await?;
                    Ok(ReadBindingOutcome::Replay(NativeReadBinding { capability }))
                }
                Err(AcpError::UnsupportedCapability) if capability.is_source_only() => {
                    Ok(ReadBindingOutcome::CurrentOnlyWithoutReplay)
                }
                Err(error) => Err(request_error(error)),
            }
        })
    }
    fn scope<'a>(&self, binding: &'a Self::Binding) -> &'a ConnectionScope {
        binding.capability.scope()
    }
    fn reserve_replay(
        &self,
        binding: &Self::Binding,
        read: ReadEpoch,
        options: NativeReadOptions,
    ) -> Result<Self::Reservation, ViewError> {
        binding.capability.require_current()?;
        let connection = binding
            .capability
            .source()
            .connection()
            .map_err(|_| ViewError::SourceReleased)?;
        let request = connection
            .strategy()
            .ok_or(ViewError::ProtocolFailure)?
            .replay(
                session(binding.capability.native())?,
                binding.capability.setup(),
            )
            .map_err(request_error)?;
        let response = match options.response_wait {
            ReadResponseWait::At(deadline) => ResponseDeadline::At(deadline),
            ReadResponseWait::Unbounded => ResponseDeadline::Unbounded,
        };
        let reservation = connection
            .reserve_request(request, response)
            .map_err(request_error)?;
        let frame = reservation.encode().map_err(request_error)?;
        let probe = reservation.probe();
        Ok(NativeReplayReservation {
            objects: Arc::new(Mutex::new(Some((reservation, frame)))),
            probe,
            capability: Arc::clone(&binding.capability),
            read,
            options,
            operation: None,
        })
    }
    #[expect(
        clippy::too_many_lines,
        reason = "Keep linear reservation, original task adoption and actual enqueue handoff together; each error returns its original owner"
    )]
    fn enqueue_replay<'a>(
        &'a self,
        binding: &'a Self::Binding,
        mut reserved: Self::Reservation,
    ) -> BoxReadFuture<
        'a,
        Result<Self::Operation, ReadEnqueueError<Self::Reservation, Self::Operation>>,
    > {
        Box::pin(async move {
            let Some(attachment) = self.attachment.upgrade() else {
                return Err(ReadEnqueueError::RetainedReservation {
                    error: ViewError::CaptureUnavailable,
                    reservation: reserved,
                });
            };
            if !Arc::ptr_eq(&reserved.capability, &binding.capability) {
                return Err(ReadEnqueueError::RetainedReservation {
                    error: ViewError::InvalidScope,
                    reservation: reserved,
                });
            }
            let operation = match attachment.prepare_replay(&reserved) {
                Ok(operation) => operation,
                Err(error) => {
                    return Err(ReadEnqueueError::RetainedReservation {
                        error,
                        reservation: reserved,
                    });
                }
            };
            reserved.operation = Some(operation.clone());
            let (ready, started) = tokio::sync::oneshot::channel::<()>();
            let original = operation.clone();
            let observed = Arc::downgrade(&attachment);
            let failed = Arc::downgrade(&attachment);
            let failed_operation = operation.clone();
            let job = OwnedReadTicketJob {
                capability: Arc::clone(&reserved.capability),
                view_generation: None,
                work: Box::pin(async move {
                    let _ = started.await;
                    let ticket = match original.ticket.lock() {
                        Ok(ticket) => ticket.clone(),
                        Err(poison) => poison.into_inner().clone(),
                    };
                    let report = if let Some(ticket) = ticket {
                        Some(ticket.wait().await)
                    } else {
                        let objects = match original.objects.lock() {
                            Ok(mut objects) => objects.take(),
                            Err(poison) => poison.into_inner().take(),
                        };
                        if let Some((reservation, frame)) = objects {
                            let probe = reservation.cancel();
                            drop(frame);
                            if let WriteProgress::Finished(report) = probe.progress() {
                                Some(report)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };
                    if let (Some(attachment), Some(report)) = (observed.upgrade(), report) {
                        attachment.replay_transport(&original, report);
                    }
                }),
                on_failure: Box::new(move || {
                    if let Some(attachment) = failed.upgrade() {
                        attachment.replay_failed(&failed_operation, ViewError::ReadFailure);
                    }
                }),
            };
            let receipt = match attachment.registrar.adopt(job) {
                Ok(receipt) => receipt,
                Err(rejected) => {
                    let crate::application::native_dispatch::read_access::ReadTicketAdoptionError {
                        error,
                        job,
                    } = rejected;
                    // Adoption did not start this job. Drop only its unpolled future;
                    // the returned reservation retains the original objects slot.
                    drop(job);
                    return Err(ReadEnqueueError::RetainedReservation {
                        error,
                        reservation: reserved,
                    });
                }
            };
            match operation.task.lock() {
                Ok(mut task) => *task = Some(receipt),
                Err(poison) => *poison.into_inner() = Some(receipt),
            }
            if let Err(error) = attachment.register_replay(&operation, &reserved) {
                attachment.replay_failed(&operation, error);
                return Err(ReadEnqueueError::RetainedOperation {
                    error,
                    operation: NativeReplayOperation {
                        original: operation,
                        last: None,
                        proof: None,
                    },
                });
            }
            let Ok(connection) = reserved.capability.source().connection() else {
                return Err(ReadEnqueueError::RetainedOperation {
                    error: ViewError::SourceReleased,
                    operation: NativeReplayOperation {
                        original: operation,
                        last: None,
                        proof: None,
                    },
                });
            };
            let fence = Arc::new(ReplayFence {
                scope: reserved.probe.scope(),
                rpc: operation.rpc.clone(),
                key: operation.key.clone(),
                native: reserved.capability.write_boundary(scalar(&operation.rpc)),
            });
            let objects = match operation.objects.lock() {
                Ok(mut objects) => objects.take(),
                Err(poison) => poison.into_inner().take(),
            };
            let Some((reservation, frame)) = objects else {
                return Err(ReadEnqueueError::RetainedOperation {
                    error: ViewError::InvalidStage,
                    operation: NativeReplayOperation {
                        original: operation,
                        last: None,
                        proof: None,
                    },
                });
            };
            let options = reserved.options;
            let ticket = match connection.enqueue(reservation, frame, fence, options.write_deadline)
            {
                Ok(ticket) => Arc::new(ticket),
                Err(rejected) => {
                    let (error, reservation, frame) = rejected.into_parts();
                    attachment.replay_failed(&operation, request_error(error));
                    match operation.objects.lock() {
                        Ok(mut objects) => *objects = Some((reservation, frame)),
                        Err(poison) => *poison.into_inner() = Some((reservation, frame)),
                    }
                    return Err(ReadEnqueueError::RetainedOperation {
                        error: request_error(error),
                        operation: NativeReplayOperation {
                            original: operation,
                            last: None,
                            proof: None,
                        },
                    });
                }
            };
            match operation.ticket.lock() {
                Ok(mut original) => *original = Some(ticket),
                Err(poison) => *poison.into_inner() = Some(ticket),
            }
            let _ = ready.send(());
            Ok(NativeReplayOperation {
                original: operation,
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
        let views = self
            .attachment
            .upgrade()
            .ok_or(ViewError::SourceReleased)
            .and_then(|attachment| attachment.views());
        Box::pin(async move {
            let views = views?;
            loop {
                let changed = views.core.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                let (observation, proof) = {
                    let mut original = operation
                        .original
                        .state
                        .lock()
                        .map_err(|_| ViewError::ReadFailure)?;
                    (original.read.observation(), original.proof.clone())
                };
                let published =
                    observation.completion() == ReplayCompletionState::Complete && proof.is_some();
                if operation.last != Some((observation, published)) {
                    operation.last = Some((observation, published));
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
                    let completion = if published {
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
                changed.await;
            }
        })
    }
    fn cancel_reserved(
        &self,
        reservation: Self::Reservation,
    ) -> Result<ReadTransportState, ReadCancelError<Self::Reservation>> {
        let probe = reservation.probe.clone();
        if probe.scope() != *reservation.capability.scope() {
            return Err(ReadCancelError {
                error: ViewError::InvalidScope,
                original: reservation,
            });
        }
        let operation = reservation.operation.clone();
        let objects = match reservation.objects.lock() {
            Ok(mut objects) => objects.take(),
            Err(poison) => poison.into_inner().take(),
        };
        if let Some((original, frame)) = objects {
            let _ = original.cancel();
            drop(frame);
        }
        drop(reservation);
        if let (Some(attachment), Some(operation)) = (self.attachment.upgrade(), operation)
            && let WriteProgress::Finished(report) = probe.progress()
        {
            attachment.replay_transport(&operation, report);
        }
        Ok(transport(probe.progress()))
    }
}
impl AcpSourceViews {
    fn prepare_replay(
        &self,
        reserved: &NativeReplayReservation,
    ) -> Result<Arc<ReplayOperation>, ViewError> {
        reserved.capability.require_current()?;
        let objects = reserved
            .objects
            .lock()
            .map_err(|_| ViewError::ReadFailure)?;
        let (reservation, _) = objects.as_ref().ok_or(ViewError::InvalidStage)?;
        let read = self
            .capture
            .lock()
            .map_err(|_| ViewError::ReadFailure)?
            .track_replay(reservation, reserved.read)?;
        let rpc = reservation.id().ok_or(ViewError::ProtocolFailure)?.clone();
        drop(objects);
        Ok(Arc::new(ReplayOperation {
            objects: Arc::clone(&reserved.objects),
            rpc,
            key: reserved.capability.native().clone(),
            state: Mutex::new(ReplayState {
                read,
                marked: false,
                proof: None,
            }),
            ticket: Mutex::new(None),
            task: Mutex::new(None),
        }))
    }
    fn register_replay(
        &self,
        operation: &Arc<ReplayOperation>,
        reserved: &NativeReplayReservation,
    ) -> Result<(), ViewError> {
        let mut operations = self.operations.lock().map_err(|_| ViewError::ReadFailure)?;
        operations.retain(|operation| operation.strong_count() != 0);
        operations
            .try_reserve(1)
            .map_err(|_| ViewError::AllocationFailure)?;
        let objects = reserved
            .objects
            .lock()
            .map_err(|_| ViewError::ReadFailure)?;
        let (reservation, _) = objects.as_ref().ok_or(ViewError::InvalidStage)?;
        self.capture
            .lock()
            .map_err(|_| ViewError::ReadFailure)?
            .bind_module_reservation(reservation, operation.key.clone(), Some(reserved.read))?;
        drop(objects);
        operations.push(Arc::downgrade(operation));
        drop(operations);
        Ok(())
    }
    pub(super) fn capture_replay_received(&self, event: &Received) -> Result<(), ViewError> {
        let mut operations = self.operations.lock().map_err(|_| ViewError::ReadFailure)?;
        let mut capture = self.capture.lock().map_err(|_| ViewError::ReadFailure)?;
        let captured = capture.capture_received(event);
        operations.retain(|operation| operation.strong_count() != 0);
        for operation in operations.iter().filter_map(Weak::upgrade) {
            let mut state = operation.state.lock().map_err(|_| ViewError::ReadFailure)?;
            let observed = state.read.observe_received(event)?;
            if observed && !state.marked && state.read.response_boundary().is_some() {
                let boundary = state
                    .read
                    .response_boundary()
                    .ok_or(ViewError::ReadFailure)?;
                state.proof = Some(Arc::new(ReadCompletionProof {
                    scope: boundary.scope,
                    read: boundary.epoch,
                    target: operation.key.clone(),
                    rpc: read_scalar(&operation.rpc),
                    sequence: boundary.sequence,
                    order: boundary.receipt_order,
                    _retained: RetainedAcpRaw::from_received(event),
                }));
                match capture.complete_module_read(operation.key.clone(), &state.read) {
                    Ok(_) => state.marked = true,
                    Err(error) => {
                        let _ = capture.fail_module_read(
                            &operation.key,
                            &state.read,
                            error,
                            ReadFailureEvidence::Response {
                                raw: RetainedAcpRaw::from_received(event),
                                sequence: event.sequence(),
                                order: event.receipt_order(),
                            },
                        );
                        state.marked = true;
                    }
                }
            }
            if matches!(event.incoming(), Incoming::Response { id, response: Response::RemoteError { .. }, .. } if *id==operation.rpc)
                && !state.marked
            {
                let _ = capture.fail_module_read(
                    &operation.key,
                    &state.read,
                    ViewError::ReadFailure,
                    ReadFailureEvidence::Response {
                        raw: RetainedAcpRaw::from_received(event),
                        sequence: event.sequence(),
                        order: event.receipt_order(),
                    },
                );
                state.marked = true;
            }
        }
        drop(capture);
        drop(operations);
        if let Ok(views) = self.views() {
            views.core.changed.notify_waiters();
        }
        captured
    }
    fn replay_failed(&self, operation: &ReplayOperation, error: ViewError) {
        if let (Ok(mut capture), Ok(mut state)) = (self.capture.lock(), operation.state.lock())
            && !state.marked
        {
            let _ = capture.fail_module_read(
                &operation.key,
                &state.read,
                error,
                ReadFailureEvidence::Transport(Arc::new(state.read.original_write_probe())),
            );
            state.marked = true;
        }
        if let Ok(views) = self.views() {
            views.core.changed.notify_waiters();
        }
    }
    fn replay_transport(&self, operation: &ReplayOperation, report: WriteReport) {
        if matches!(
            report,
            WriteReport::Zero { .. } | WriteReport::Unknown { .. }
        ) {
            self.replay_failed(operation, ViewError::TransportFailure);
        }
        if let Ok(views) = self.views() {
            views.core.changed.notify_waiters();
        }
    }
}
