//! Only an actual original necessary-save failure can own this Cancelled-only path.
#![allow(
    clippy::significant_drop_tightening,
    clippy::similar_names,
    clippy::too_many_lines,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    ApprovalReplyArmProof, ApprovalReplyTerminalProof, Arc, CancelledControlReservation,
    ConfigurationPort, ControlIdentity, ControlWriteBoundary, DispatchError,
    DispatchFenceRejection, DispatchPollIo, DispatchWriteStep, EncodedPromptFrame,
    NativeApprovalPort, NativeApprovalRepository, NativeControlRepository, NativeCoordinator,
    NativeDispatchRepository, NativeResourceRepository, NativeRuntimeOptions, Ordering,
    OriginalRequestProbe, OwnedNativeSource, PermissionFailureCancelBinding,
    PermissionPendingSnapshot, PreSourceProbe, PromptTicket, ReservedControl, SourceLaunchPort,
    TransportFact, TransportProgress,
    approval_inventory::{ApprovalSlot, digest_data},
    approval_records::ApprovalWriteCache,
    environment::{PermissionResponseOwner, SourcePermissionSlot},
    runtime::SourceSlot,
};
use crate::domain::{
    approval_owner::{
        ActualReplyKind, ApprovalRejection, ApprovalReplyId, NecessaryWriteError, ReplyOrigin,
        TerminalReplyObservation,
    },
    approval_records::{
        ApprovalAttemptRecord, ApprovalTerminalRecord, StoredApprovalAttempt,
        StoredApprovalTerminal,
    },
};
use std::{
    sync::{Mutex, Weak},
    task::Context,
};
pub(super) enum ApprovalSaveFailure<P: NativeApprovalPort> {
    Request {
        original: Arc<OriginalRequestProbe<P>>,
        error: NecessaryWriteError,
    },
    Registration {
        original: Arc<super::approval_service::ApprovalRegistrationOperation<P>>,
        error: NecessaryWriteError,
    },
    Decision {
        original: Arc<super::approval_service::ApprovalDecisionOperation<P>>,
        error: NecessaryWriteError,
    },
    Reply {
        original: Arc<super::approval_response::ApprovalResponseOperation<P>>,
        error: NecessaryWriteError,
    },
}
const fn is_storage_failure(error: NecessaryWriteError) -> bool {
    matches!(
        error,
        NecessaryWriteError::Rejected(ApprovalRejection::NecessaryStorageUnavailable)
    )
}
impl<P: NativeApprovalPort> ApprovalSaveFailure<P> {
    fn verify_actual(&self, slot: &ApprovalSlot<P>) -> Result<bool, DispatchError> {
        let state = slot
            .state
            .lock()
            .map_err(|_| DispatchError::RequiredStateUnavailable)?;
        let (error, same) = match self {
            Self::Request { original, error } => (
                *error,
                Arc::ptr_eq(original, &slot.request) && state.request_error == Some(*error),
            ),
            Self::Registration { original, error } => (
                *error,
                state
                    .registered
                    .get(&original.probe.metadata().registration)
                    .is_some_and(|op| Arc::ptr_eq(op, original))
                    && matches!(*original.result.lock().map_err(|_|DispatchError::RequiredStateUnavailable)?,Some(Err(actual)) if actual==*error),
            ),
            Self::Decision { original, error } => (
                *error,
                state
                    .admission
                    .as_ref()
                    .is_some_and(|op| Arc::ptr_eq(op, original))
                    && matches!(*original.result.lock().map_err(|_|DispatchError::RequiredStateUnavailable)?,Some(Err(actual)) if actual==*error),
            ),
            Self::Reply { original, error } => (
                *error,
                state.responses.iter().any(|op| Arc::ptr_eq(op, original))
                    && original
                        .state
                        .lock()
                        .map_err(|_| DispatchError::RequiredStateUnavailable)?
                        .error
                        == Some(*error),
            ),
        };
        Ok(is_storage_failure(error) && same)
    }
}
pub(super) struct ApprovalFailureResponse<P: NativeApprovalPort> {
    pub(super) id: ApprovalReplyId,
    pub(super) serial: u64,
    permission: Weak<SourcePermissionSlot<P>>,
    approval: Weak<ApprovalSlot<P>>,
    // The immutable original failing operation stays retained even if later SQL repair confirms it.
    cause: ApprovalSaveFailure<P>,
    pub(super) state: Mutex<ApprovalFailureState<P>>,
}
pub(super) struct ApprovalFailureState<P: NativeApprovalPort> {
    reservation: Option<P::ControlReservation>,
    frame: Option<P::Frame>,
    probe: Option<P::Probe>,
    cancelled: Option<CancelledControlReservation<P::Scope, P::Probe>>,
    ticket: Option<Arc<P::Ticket>>,
    pub(super) arm: Option<Arc<ApprovalReplyArmProof<P>>>,
    pub(super) arm_stored: Option<StoredApprovalAttempt>,
    pub(super) terminal: Option<Arc<ApprovalReplyTerminalProof<P>>>,
    pub(super) terminal_stored: Option<StoredApprovalTerminal>,
    error: Option<DispatchError>,
}
impl<P: NativeApprovalPort> ApprovalFailureResponse<P> {
    pub(super) fn actual_progress(&self, protocol: &P) -> Option<TransportProgress> {
        let probe = self.state.lock().ok()?.probe.clone()?;
        Some(protocol.probe_progress(&probe))
    }
}
struct FailureReplyBoundary<P: NativeApprovalPort> {
    original: Weak<ApprovalFailureResponse<P>>,
    source: Arc<PreSourceProbe>,
}
impl<P: NativeApprovalPort> ControlWriteBoundary<P::Scope> for FailureReplyBoundary<P> {
    fn check_and_poll(
        &self,
        identity: &ControlIdentity<P::Scope>,
        _: DispatchWriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn DispatchPollIo,
    ) -> Result<(), DispatchFenceRejection> {
        let original = self
            .original
            .upgrade()
            .ok_or(DispatchFenceRejection::Stopped)?;
        let permission = original
            .permission
            .upgrade()
            .ok_or(DispatchFenceRejection::Stopped)?;
        let approval = original
            .approval
            .upgrade()
            .ok_or(DispatchFenceRejection::Stopped)?;
        if !matches!(identity,ControlIdentity::PermissionCancelled{scope,native_session,rpc} if scope==&permission.scope && native_session.as_ref()==permission.session.as_ref() && rpc==&permission.rpc)
        {
            return Err(DispatchFenceRejection::StaleTarget);
        }
        let _gate = self
            .source
            .authority
            .gate
            .lock()
            .map_err(|_| DispatchFenceRejection::Stopped)?;
        let _action = permission
            .action
            .lock()
            .map_err(|_| DispatchFenceRejection::Stopped)?;
        if self.source.authority.closed.load(Ordering::SeqCst) {
            return Err(DispatchFenceRejection::Stopped);
        }
        let state = permission
            .state
            .lock()
            .map_err(|_| DispatchFenceRejection::Stopped)?;
        if state.claimed != Some(original.id.entity()) || state.retired || state.written {
            return Err(DispatchFenceRejection::StaleTarget);
        }
        let owner = permission
            .response_owner
            .lock()
            .map_err(|_| DispatchFenceRejection::Stopped)?;
        if !matches!(owner.as_ref(),Some(PermissionResponseOwner::ManagementFailure(current)) if Arc::ptr_eq(current,&original))
        {
            return Err(DispatchFenceRejection::StaleTarget);
        }
        let state = approval
            .state
            .lock()
            .map_err(|_| DispatchFenceRejection::Stopped)?;
        if !state.allow_disabled
            || !state
                .failure_responses
                .iter()
                .any(|current| Arc::ptr_eq(current, &original))
        {
            return Err(DispatchFenceRejection::Stopped);
        }
        let _ = &original.cause;
        let _ = io.poll(cx);
        Ok(())
    }
}
impl<P, L, G, N> NativeCoordinator<P, L, G, N>
where
    P: NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + NativeApprovalRepository,
{
    pub(super) async fn cancel_actual_approval_save_failure(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        permission: &Arc<SourcePermissionSlot<P>>,
        slot: &Arc<ApprovalSlot<P>>,
        cause: ApprovalSaveFailure<P>,
    ) -> Result<(), DispatchError> {
        if !cause.verify_actual(slot)? {
            return Ok(());
        }
        // All protocol/WriteCell observations precede local action locks. A prior actual Zero is immutable.
        let prior = {
            let owner = permission
                .response_owner
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?;
            match owner.as_ref() {
                Some(PermissionResponseOwner::Selected(previous)) => Some(previous.clone()),
                Some(_) => return Ok(()),
                None => None,
            }
        };
        if let Some(previous) = &prior {
            if !matches!(&cause,ApprovalSaveFailure::Reply{original,..} if Arc::ptr_eq(original,previous))
            {
                return Ok(());
            }
            let (probe, unreserved) = {
                let state = previous
                    .state
                    .lock()
                    .map_err(|_| DispatchError::RequiredStateUnavailable)?;
                (
                    state.probe.clone(),
                    state.reservation.is_none()
                        && state.frame.is_none()
                        && state.ticket.is_none()
                        && state.arm.is_none(),
                )
            };
            match probe
                .as_ref()
                .map(|probe| self.protocol.probe_progress(probe))
            {
                Some(TransportProgress::Finished(TransportFact::Zero { .. })) => {}
                None if unreserved => {}
                _ => return Ok(()),
            }
        }
        let connection = source.source.get().ok_or(DispatchError::InvalidScope)?;
        if self
            .protocol
            .inspect_permission(connection, &permission.request)?
            != PermissionPendingSnapshot::Waiting
        {
            return Ok(());
        }
        let id = ApprovalReplyId::from_entity(Self::new_id()?);
        let operation = {
            let _gate = source
                .pre
                .authority
                .gate
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?;
            let _action = permission
                .action
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?;
            if source.pre.authority.closed.load(Ordering::SeqCst) {
                return Err(DispatchError::Stopped);
            }
            let mut state = permission
                .state
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?;
            let mut owner = permission
                .response_owner
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?;
            if state.written || state.retired {
                return Ok(());
            }
            match (owner.as_ref(), prior.as_ref()) {
                (None, None) => {}
                (Some(PermissionResponseOwner::Selected(current)), Some(previous))
                    if Arc::ptr_eq(current, previous) => {}
                _ => return Ok(()),
            }
            let mut approval = slot
                .state
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?;
            let serial = approval.next_reply_serial;
            approval.next_reply_serial = serial
                .checked_add(1)
                .ok_or(DispatchError::LengthUnrepresentable)?;
            approval.allow_disabled = true;
            let operation = Arc::new(ApprovalFailureResponse {
                id,
                serial,
                permission: Arc::downgrade(permission),
                approval: Arc::downgrade(slot),
                cause,
                state: Mutex::new(ApprovalFailureState {
                    reservation: None,
                    frame: None,
                    probe: None,
                    cancelled: None,
                    ticket: None,
                    arm: None,
                    arm_stored: None,
                    terminal: None,
                    terminal_stored: None,
                    error: None,
                }),
            });
            approval.failure_responses.push(operation.clone());
            state.claimed = Some(id.entity());
            *owner = Some(PermissionResponseOwner::ManagementFailure(
                operation.clone(),
            ));
            operation
        };
        self.require_approval_fact(source, id.entity())?;
        let result = self
            .drive_approval_failure_cancelled(source, permission, slot, &operation)
            .await;
        if let Err(error) = result {
            match operation.state.lock() {
                Ok(mut state) => state.error = Some(error),
                Err(poisoned) => poisoned.into_inner().error = Some(error),
            }
        }
        slot.publish_change();
        result
    }
    async fn drive_approval_failure_cancelled(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        permission: &Arc<SourcePermissionSlot<P>>,
        slot: &Arc<ApprovalSlot<P>>,
        operation: &Arc<ApprovalFailureResponse<P>>,
    ) -> Result<(), DispatchError> {
        let connection = source.source.get().ok_or(DispatchError::InvalidScope)?;
        let obligation = permission
            .snapshot()?
            .obligation
            .ok_or(DispatchError::RequiredStateUnavailable)?;
        let reservation = self.protocol.reserve_permission_failure_cancelled(
            connection,
            &permission.request,
            &PermissionFailureCancelBinding {
                scope: permission.scope.clone(),
                rpc: permission.rpc.clone(),
                source: source.pre.clone(),
                obligation,
            },
        )?;
        {
            let mut state = match operation.state.lock() {
                Ok(state) => state,
                Err(poisoned) => {
                    let mut state = poisoned.into_inner();
                    state.probe = Some(reservation.probe());
                    state.reservation = Some(reservation);
                    return Err(DispatchError::RequiredStateUnavailable);
                }
            };
            state.probe = Some(reservation.probe());
            state.reservation = Some(reservation);
        }
        let encoded = {
            let state = operation
                .state
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?;
            state
                .reservation
                .as_ref()
                .ok_or(DispatchError::InvalidPromptReservation)?
                .encode()
        };
        let frame = match encoded {
            Ok(frame) => frame,
            Err(error) => {
                self.cancel_failure_reservation(operation);
                return Err(error);
            }
        };
        let length = match frame.byte_len_checked() {
            Ok(length) if length > 0 => length,
            _ => {
                self.cancel_failure_reservation(operation);
                return Err(DispatchError::LengthUnrepresentable);
            }
        };
        let at = self.clock.unix_millis();
        let fingerprint = digest_data(
            b"acpr-approval-save-failure-arm-v1\0",
            &[
                operation.id.encode().as_bytes(),
                slot.request.fingerprint(),
                &length.to_be_bytes(),
            ],
        )?;
        let arm = Arc::new(ApprovalReplyArmProof {
            original: slot.request.original.clone(),
            metadata: ApprovalAttemptRecord {
                response: operation.id,
                approval: slot.request.metadata().approval,
                decision_operation: None,
                serial: operation.serial,
                kind: ActualReplyKind::Cancelled,
                origin: ReplyOrigin::ManagementFailure,
                option_digest: None,
                frame_length: length,
                reserved_fingerprint: fingerprint,
                observed_at_ms: at,
                recorded_at_ms: at,
            },
            fingerprint,
            write: ApprovalWriteCache::default(),
        });
        {
            let mut state = operation
                .state
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?;
            state.arm = Some(arm);
            state.frame = Some(frame);
        }
        // This Cancelled-only safety channel deliberately does not wait for database recovery.
        let (reservation, frame) = {
            let mut state = operation
                .state
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?;
            (
                state
                    .reservation
                    .take()
                    .ok_or(DispatchError::InvalidPromptReservation)?,
                state
                    .frame
                    .take()
                    .ok_or(DispatchError::InvalidPromptReservation)?,
            )
        };
        let boundary = Arc::new(FailureReplyBoundary {
            original: Arc::downgrade(operation),
            source: source.pre.clone(),
        });
        let ticket = match self.protocol.enqueue_control(
            connection,
            reservation,
            frame,
            boundary,
            NativeRuntimeOptions::at(self.options.prompt_write_wait)?,
        ) {
            Ok(ticket) => Arc::new(ticket),
            Err(rejected) => {
                {
                    let mut state = match operation.state.lock() {
                        Ok(state) => state,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    state.reservation = Some(rejected.reservation);
                    state.frame = Some(rejected.frame);
                }
                self.cancel_failure_reservation(operation);
                return self.persist_approval_failure_facts(source, operation).await;
            }
        };
        {
            let mut state = match operation.state.lock() {
                Ok(state) => state,
                Err(poisoned) => poisoned.into_inner(),
            };
            state.ticket = Some(ticket.clone());
        }
        let fact = ticket.wait().await;
        if fact == TransportFact::Written {
            permission
                .state
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?
                .written = true;
        }
        self.persist_approval_failure_facts(source, operation).await
    }
    fn cancel_failure_reservation(&self, operation: &ApprovalFailureResponse<P>) {
        let reservation = {
            let mut state = match operation.state.lock() {
                Ok(state) => state,
                Err(poisoned) => poisoned.into_inner(),
            };
            state.frame.take();
            state.reservation.take()
        };
        if let Some(reservation) = reservation {
            let receipt = self.protocol.cancel_reserved_control(reservation);
            match operation.state.lock() {
                Ok(mut state) => state.cancelled = Some(receipt),
                Err(poisoned) => poisoned.into_inner().cancelled = Some(receipt),
            }
        }
    }
    pub(super) async fn persist_approval_failure_facts(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        operation: &Arc<ApprovalFailureResponse<P>>,
    ) -> Result<(), DispatchError> {
        let (arm, probe, existing) = {
            let state = operation
                .state
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?;
            (
                state.arm.clone(),
                state.probe.clone(),
                state.terminal.clone(),
            )
        };
        let Some(arm) = arm else {
            return Ok(());
        };
        let probe = probe.ok_or(DispatchError::ResourceUnconfirmed)?;
        let TransportProgress::Finished(fact) = self.protocol.probe_progress(&probe) else {
            return Err(DispatchError::ResourceUnconfirmed);
        };
        let (observation, accepted, label) = match fact {
            TransportFact::Zero { reason } => {
                (TerminalReplyObservation::Zero { reason }, 0, reason.label())
            }
            TransportFact::Written => (
                TerminalReplyObservation::Written,
                arm.metadata().frame_length,
                "written",
            ),
            TransportFact::Unknown { accepted, cause } => (
                TerminalReplyObservation::Unknown { accepted, cause },
                accepted,
                cause.label(),
            ),
        };
        let terminal = if let Some(existing) = existing {
            if existing.metadata().observation != observation
                || existing.metadata().accepted_length != accepted
            {
                return Err(DispatchError::Conflict);
            }
            existing
        } else {
            let at = self.clock.unix_millis();
            let fingerprint = digest_data(
                b"acpr-approval-save-failure-terminal-v1\0",
                &[
                    arm.fingerprint(),
                    label.as_bytes(),
                    &accepted.to_be_bytes(),
                    &at.to_be_bytes(),
                ],
            )?;
            Arc::new(ApprovalReplyTerminalProof {
                original: arm.original.clone(),
                metadata: ApprovalTerminalRecord {
                    response: operation.id,
                    approval: arm.metadata().approval,
                    observation,
                    accepted_length: accepted,
                    fingerprint,
                    observed_at_ms: at,
                    recorded_at_ms: at,
                },
                fingerprint,
                write: ApprovalWriteCache::default(),
            })
        };
        operation
            .state
            .lock()
            .map_err(|_| DispatchError::RequiredStateUnavailable)?
            .terminal = Some(terminal.clone());
        let stored_arm = self
            .repository
            .arm_approval_reply(&arm, &self.deadline()?)
            .await
            .map_err(super::approval_control::native_error)?;
        operation
            .state
            .lock()
            .map_err(|_| DispatchError::RequiredStateUnavailable)?
            .arm_stored = Some(stored_arm);
        let stored = self
            .repository
            .record_approval_reply(&terminal, &self.deadline()?)
            .await
            .map_err(super::approval_control::native_error)?;
        operation
            .state
            .lock()
            .map_err(|_| DispatchError::RequiredStateUnavailable)?
            .terminal_stored = Some(stored.clone());
        if let Some(slot) = operation.approval.upgrade() {
            let target = slot
                .association
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?
                .as_ref()
                .map(|a| a.metadata().target);
            self.append_owned_approval_audit(
                source,
                &slot,
                super::approval_audit::ApprovalAuditBasis::delivery(arm, terminal, stored, target),
            )
            .await;
            slot.publish_change();
        }
        self.confirm_approval_fact(source, operation.id.entity())?;
        Ok(())
    }
    pub(super) async fn collect_approval_failure_records(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        slot: &Arc<ApprovalSlot<P>>,
    ) -> Result<
        (
            Vec<ApprovalAttemptRecord>,
            Vec<ApprovalTerminalRecord>,
            Vec<[u8; 32]>,
        ),
        DispatchError,
    > {
        let originals = slot
            .state
            .lock()
            .map_err(|_| DispatchError::RequiredStateUnavailable)?
            .failure_responses
            .clone();
        let owner = self.repository.approval_owner_scope();
        let until = self.deadline()?.deadline();
        let mut arms = Vec::new();
        let mut terminals = Vec::new();
        let mut stages = Vec::new();
        for original in originals {
            let has_arm = original
                .state
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?
                .arm
                .is_some();
            if has_arm {
                self.persist_approval_failure_facts(source, &original)
                    .await?;
                let (arm, terminal) = {
                    let state = original
                        .state
                        .lock()
                        .map_err(|_| DispatchError::RequiredStateUnavailable)?;
                    (
                        state
                            .arm
                            .clone()
                            .ok_or(DispatchError::ResourceUnconfirmed)?,
                        state
                            .terminal
                            .clone()
                            .ok_or(DispatchError::ResourceUnconfirmed)?,
                    )
                };
                for cache in [arm.write_cache(), terminal.write_cache()] {
                    if !cache
                        .confirm_all_cleanup(&owner, until)
                        .await
                        .map_err(super::approval_control::native_error)?
                        || !cache
                            .confirm_actual_close(&owner, until)
                            .await
                            .map_err(super::approval_control::native_error)?
                    {
                        return Err(DispatchError::ResourceUnconfirmed);
                    }
                }
                arms.push(arm.metadata().clone());
                terminals.push(terminal.metadata().clone());
            } else {
                // Called only after the same Source's physical and producer quiescence proof. Consume any
                // original unqueued reservation explicitly; Drop is not cancellation evidence.
                self.cancel_failure_reservation(&original);
                let state = original
                    .state
                    .lock()
                    .map_err(|_| DispatchError::RequiredStateUnavailable)?;
                if state.reservation.is_some() || state.frame.is_some() || state.ticket.is_some() {
                    return Err(DispatchError::ResourceUnconfirmed);
                }
                let stage = if let Some(probe) = &state.probe {
                    let Some(cancelled) = &state.cancelled else {
                        return Err(DispatchError::ResourceUnconfirmed);
                    };
                    let permission = original
                        .permission
                        .upgrade()
                        .ok_or(DispatchError::ResourceUnconfirmed)?;
                    if cancelled.identity().scope() != &permission.scope
                        || cancelled.identity().rpc() != Some(&permission.rpc)
                        || !matches!(
                            self.protocol.probe_progress(probe),
                            TransportProgress::Finished(TransportFact::Zero { .. })
                        )
                    {
                        return Err(DispatchError::ResourceUnconfirmed);
                    }
                    b"cancelled_no_frame".as_slice()
                } else {
                    b"no_reservation".as_slice()
                };
                stages.push(digest_data(
                    b"acpr-approval-failure-stage-v1\0",
                    &[
                        original.id.encode().as_bytes(),
                        slot.request.fingerprint(),
                        stage,
                    ],
                )?);
            }
            self.confirm_approval_fact(source, original.id.entity())?;
        }
        Ok((arms, terminals, stages))
    }
}
