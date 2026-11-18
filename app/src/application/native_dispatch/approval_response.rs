//! Selected replies retain the original reservation, frame, ticket and source-owned facts.
#![allow(
    clippy::manual_let_else,
    clippy::significant_drop_tightening,
    clippy::too_many_lines,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    ApprovalAdmissionProbe, ApprovalReplyArmProof, ApprovalReplyTerminalProof, Arc,
    CancelledPermissionReplyReservation, ConfigurationPort, DispatchFenceRejection, DispatchPollIo,
    DispatchWriteStep, EncodedPromptFrame, Instant, NativeApprovalPort, NativeApprovalRepository,
    NativeControlRepository, NativeCoordinator, NativeDispatchRepository, NativeResourceRepository,
    NativeRuntimeOptions, Ordering, OwnedNativeSource, PermissionReplyBoundary,
    PermissionReplyIdentity, PermissionSelectedBinding, PreSourceProbe, PromptTicket,
    ReservedPermissionReply, SourceLaunchPort, TransportFact, TransportProgress,
    approval_inventory::{ApprovalSlot, digest_data},
    approval_records::ApprovalWriteCache,
    approval_service::ApprovalDecisionOperation,
    environment::SourcePermissionSlot,
    runtime::SourceSlot,
};
use crate::domain::{
    approval_owner::{
        ActualReplyKind, ApprovalId, ApprovalRejection, ApprovalReplyId, CommitState,
        LeaseCloseState, NecessaryWriteError, ReplyOrigin, SqlTaskJoinState,
        TerminalReplyObservation,
    },
    approval_records::{
        ApprovalAttemptRecord, ApprovalTerminalRecord, StoredApprovalAttempt,
        StoredApprovalDecision, StoredApprovalTerminal,
    },
};
use std::{
    sync::{Mutex, Weak},
    task::Context,
};

pub(super) struct DecisionSavedCertificate<P: NativeApprovalPort> {
    admission: Arc<ApprovalAdmissionProbe<P>>,
    stored: StoredApprovalDecision,
}
impl<P: NativeApprovalPort> DecisionSavedCertificate<P> {
    pub(super) const fn approval(&self) -> ApprovalId {
        self.stored.record().approval
    }
    pub(super) fn from_actual(
        admission: Arc<ApprovalAdmissionProbe<P>>,
        stored: StoredApprovalDecision,
    ) -> Result<Self, ApprovalRejection> {
        let actual = admission
            .write_cache()
            .confirmation()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        if stored.record() != admission.metadata()
            || stored.record_fingerprint() != admission.fingerprint()
            || stored.confirmation().commit != CommitState::Confirmed
            || stored.confirmation().lease_close != LeaseCloseState::Confirmed
            || stored.confirmation().task_join != SqlTaskJoinState::Joined
            || actual.commit != CommitState::Confirmed
            || actual.lease_close != LeaseCloseState::Confirmed
            || actual.task_join != SqlTaskJoinState::Joined
        {
            return Err(ApprovalRejection::NecessaryStorageUnavailable);
        }
        Ok(Self { admission, stored })
    }
}
pub(super) struct ApprovalResponseOperation<P: NativeApprovalPort> {
    pub(super) id: ApprovalReplyId,
    pub(super) serial: u64,
    pub(super) write_until: Mutex<Instant>,
    pub(super) permission: Weak<SourcePermissionSlot<P>>,
    pub(super) approval: Weak<ApprovalSlot<P>>,
    pub(super) saved: Arc<DecisionSavedCertificate<P>>,
    pub(super) scope: P::Scope,
    pub(super) choice_digest: [u8; 32],
    pub(super) state: Mutex<ApprovalResponseState<P>>,
    pub(super) changed: tokio::sync::Notify,
}
#[derive(Clone, Copy, Debug)]
pub(super) enum ApprovalBoundaryObservation {
    BeforeBoundary,
    Closing,
    Required(usize),
    Identity,
    Saved,
    ActualPoll,
}
pub(super) struct ApprovalResponseState<P: NativeApprovalPort> {
    pub(super) last_boundary: ApprovalBoundaryObservation,
    pub(super) reservation: Option<P::PermissionReplyReservation>,
    pub(super) frame: Option<P::Frame>,
    pub(super) probe: Option<P::Probe>,
    pub(super) cancelled: Option<CancelledPermissionReplyReservation<P::Scope, P::Probe>>,
    pub(super) ticket: Option<Arc<P::Ticket>>,
    pub(super) arm: Option<Arc<ApprovalReplyArmProof<P>>>,
    pub(super) arm_stored: Option<StoredApprovalAttempt>,
    pub(super) terminal: Option<Arc<ApprovalReplyTerminalProof<P>>>,
    pub(super) terminal_stored: Option<StoredApprovalTerminal>,
    pub(super) error: Option<NecessaryWriteError>,
}
struct SelectedReplyBoundary<P: NativeApprovalPort> {
    operation: Weak<ApprovalResponseOperation<P>>,
    source: Arc<PreSourceProbe>,
}
impl<P: NativeApprovalPort> PermissionReplyBoundary<P::Scope> for SelectedReplyBoundary<P> {
    fn check_and_poll(
        &self,
        identity: &PermissionReplyIdentity<P::Scope>,
        _: DispatchWriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn DispatchPollIo,
    ) -> Result<(), DispatchFenceRejection> {
        let operation = self
            .operation
            .upgrade()
            .ok_or(DispatchFenceRejection::Stopped)?;
        let permission = operation
            .permission
            .upgrade()
            .ok_or(DispatchFenceRejection::Stopped)?;
        let approval = operation
            .approval
            .upgrade()
            .ok_or(DispatchFenceRejection::Stopped)?;
        let _source = self
            .source
            .authority
            .gate
            .lock()
            .map_err(|_| DispatchFenceRejection::Stopped)?;
        let _action = permission
            .action
            .lock()
            .map_err(|_| DispatchFenceRejection::Stopped)?;
        let permission_state = permission
            .state
            .lock()
            .map_err(|_| DispatchFenceRejection::Stopped)?;
        let mut state = operation
            .state
            .lock()
            .map_err(|_| DispatchFenceRejection::Stopped)?;
        if self.source.authority.closed.load(Ordering::SeqCst)
            || self.source.authority.closing.load(Ordering::SeqCst)
        {
            state.last_boundary = ApprovalBoundaryObservation::Closing;
            return Err(DispatchFenceRejection::Stopped);
        }
        if permission.association.as_ref().is_some_and(|resource| {
            resource.authority.closed.load(Ordering::SeqCst)
                || resource.authority.releasing.load(Ordering::SeqCst)
        }) {
            state.last_boundary = ApprovalBoundaryObservation::Identity;
            return Err(DispatchFenceRejection::StaleTarget);
        }
        if self.source.authority.required.load(Ordering::SeqCst) != 0 {
            state.last_boundary = ApprovalBoundaryObservation::Required(
                self.source.authority.required.load(Ordering::SeqCst),
            );
            return Err(DispatchFenceRejection::RequiredStateUnavailable);
        }
        if identity.scope() != &operation.scope
            || identity.rpc() != &permission.rpc
            || identity.native_session() != &*permission.session
            || identity.option_digest() != &operation.choice_digest
            || permission_state.claimed != Some(operation.id.entity())
            || permission_state.retired
            || permission_state.written
        {
            state.last_boundary = ApprovalBoundaryObservation::Identity;
            return Err(DispatchFenceRejection::StaleTarget);
        }
        if !Arc::ptr_eq(
            &operation.saved.admission.original,
            &approval.request.original,
        ) || operation.saved.stored.record().approval != approval.request.metadata().approval
            || operation.saved.stored.record().option_digest != operation.choice_digest
            || state.arm_stored.is_none()
        {
            state.last_boundary = ApprovalBoundaryObservation::Saved;
            return Err(DispatchFenceRejection::RequiredStateUnavailable);
        }
        state.last_boundary = ApprovalBoundaryObservation::ActualPoll;
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
    pub(super) async fn deliver_approval_decision(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        permission: &Arc<SourcePermissionSlot<P>>,
        approval: &Arc<ApprovalSlot<P>>,
        decision: &Arc<ApprovalDecisionOperation<P>>,
    ) -> Result<(), NecessaryWriteError> {
        let until = NativeRuntimeOptions::at(self.options.prompt_write_wait)
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        self.deliver_approval_with_deadline(source, permission, approval, decision, until)
            .await
            .map(|_| ())
    }
    pub(super) async fn deliver_approval_with_deadline(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        permission: &Arc<SourcePermissionSlot<P>>,
        approval: &Arc<ApprovalSlot<P>>,
        decision: &Arc<ApprovalDecisionOperation<P>>,
        write_until: Instant,
    ) -> Result<ApprovalReplyId, NecessaryWriteError> {
        // No reservation or attempt exists while another original necessary fact is pending.
        // Actual producer completion and source shutdown wake this owned actor; no synthetic IO Pending.
        loop {
            let stamp = source.progress.current();
            let changed = source.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if source.pre.authority.closed.load(Ordering::SeqCst)
                || source.pre.authority.closing.load(Ordering::SeqCst)
            {
                return Err(ApprovalRejection::InvalidRequest.into());
            }
            if source.pre.authority.required.load(Ordering::SeqCst) == 0 {
                break;
            }
            tokio::select! {()=changed=>{},()=source.progress.wait_after(stamp)=>{}}
        }
        let saved = decision
            .saved
            .get()
            .ok_or(ApprovalRejection::NecessaryStorageUnavailable)?
            .clone();
        let choice = approval
            .choices
            .get(decision.choice)
            .ok_or(ApprovalRejection::InvalidChoice)?;
        let id = ApprovalReplyId::from_entity(
            Self::new_id().map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?,
        );
        let serial = {
            let mut state = approval
                .state
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            let serial = state.next_reply_serial;
            state.next_reply_serial = serial
                .checked_add(1)
                .ok_or(ApprovalRejection::LengthUnrepresentable)?;
            serial
        };
        let operation = Arc::new(ApprovalResponseOperation {
            id,
            serial,
            write_until: Mutex::new(write_until),
            permission: Arc::downgrade(permission),
            approval: Arc::downgrade(approval),
            saved,
            scope: permission.scope.clone(),
            choice_digest: choice.digest,
            state: Mutex::new(ApprovalResponseState {
                last_boundary: ApprovalBoundaryObservation::BeforeBoundary,
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
            changed: tokio::sync::Notify::new(),
        });
        {
            let _action = permission
                .action
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            let mut permission_state = permission
                .state
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            if source.pre.authority.closed.load(Ordering::SeqCst)
                || source.pre.authority.closing.load(Ordering::SeqCst)
                || permission_state.retired
                || permission_state.written
                || permission_state.claimed.is_some()
            {
                return Err(ApprovalRejection::InvalidRequest.into());
            }
            let mut state = approval
                .state
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            state.responses.push(operation.clone());
            *permission
                .response_owner
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)? = Some(
                super::environment::PermissionResponseOwner::Selected(operation.clone()),
            );
            permission_state.claimed = Some(id.entity());
        }
        let result = self
            .drive_approval_reply(source, permission, approval, decision, &operation)
            .await;
        if let Err(error) = result {
            match operation.state.lock() {
                Ok(mut state) => state.error = Some(error),
                Err(poisoned) => poisoned.into_inner().error = Some(error),
            }
        }
        operation.changed.notify_waiters();
        approval.publish_change();
        if let Err(error) = result {
            let _ = self
                .cancel_actual_approval_save_failure(
                    source,
                    permission,
                    approval,
                    super::approval_failure::ApprovalSaveFailure::Reply {
                        original: operation.clone(),
                        error,
                    },
                )
                .await;
        }
        result.map(|()| operation.id)
    }
    pub(super) async fn drive_approval_reply(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        permission: &Arc<SourcePermissionSlot<P>>,
        approval: &Arc<ApprovalSlot<P>>,
        decision: &Arc<ApprovalDecisionOperation<P>>,
        operation: &Arc<ApprovalResponseOperation<P>>,
    ) -> Result<(), NecessaryWriteError> {
        let connection = source
            .source
            .get()
            .ok_or(ApprovalRejection::InvalidRequest)?;
        let choice = approval
            .choices
            .get(decision.choice)
            .ok_or(ApprovalRejection::InvalidChoice)?;
        let binding = PermissionSelectedBinding {
            scope: permission.scope.clone(),
            source: source.pre.clone(),
            approval: approval.request.metadata().approval,
            decision: decision.probe.metadata().operation,
            response: operation.id,
            rpc: permission.rpc.clone(),
            option_digest: choice.digest,
        };
        #[cfg(all(test,feature="approval-owner-acceptance-fixtures"))]
        self.repository.approval_native_test_cut(&decision.probe,crate::session_storage::approval_owner::test_control::ApprovalCut::BeforeSelectedReserve,operation.id,NativeRuntimeOptions::at(self.options.metadata_wait).map_err(|_|ApprovalRejection::NecessaryStorageUnavailable)?).await.map_err(|_|ApprovalRejection::NecessaryStorageUnavailable)?;
        let reservation = self
            .protocol
            .reserve_permission_selected(connection, &permission.request, &choice.choice, &binding)
            .map_err(|_| ApprovalRejection::InvalidRequest)?;
        // Publish the same original object before encoding or any await. Poison recovery retains it.
        {
            let mut state = match operation.state.lock() {
                Ok(state) => state,
                Err(poisoned) => {
                    let mut state = poisoned.into_inner();
                    state.probe = Some(reservation.probe());
                    state.reservation = Some(reservation);
                    return Err(ApprovalRejection::NecessaryStorageUnavailable.into());
                }
            };
            state.probe = Some(reservation.probe());
            state.reservation = Some(reservation);
        }
        let frame = {
            let state = operation
                .state
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            state
                .reservation
                .as_ref()
                .ok_or(ApprovalRejection::InvalidRequest)?
                .encode()
        };
        let frame = if let Ok(frame) = frame {
            frame
        } else {
            self.cancel_approval_reservation(operation);
            return Err(ApprovalRejection::InvalidRequest.into());
        };
        let frame_length = match frame.byte_len_checked() {
            Ok(length) if length > 0 => length,
            _ => {
                self.cancel_approval_reservation(operation);
                return Err(ApprovalRejection::LengthUnrepresentable.into());
            }
        };
        {
            let mut state = match operation.state.lock() {
                Ok(state) => state,
                Err(poisoned) => poisoned.into_inner(),
            };
            state.frame = Some(frame);
        }
        let at = self.clock.unix_millis();
        let fingerprint = digest_data(
            b"acpr-approval-arm-v1\0",
            &[
                operation.id.encode().as_bytes(),
                decision.probe.fingerprint(),
                &choice.digest,
                &frame_length.to_be_bytes(),
            ],
        )
        .map_err(|_| ApprovalRejection::LengthUnrepresentable)?;
        let arm = Arc::new(ApprovalReplyArmProof {
            original: approval.request.original.clone(),
            metadata: ApprovalAttemptRecord {
                response: operation.id,
                approval: approval.request.metadata().approval,
                decision_operation: Some(decision.probe.metadata().operation),
                serial: operation.serial,
                kind: ActualReplyKind::Selected,
                origin: ReplyOrigin::UserDecision,
                option_digest: Some(choice.digest),
                frame_length,
                reserved_fingerprint: fingerprint,
                observed_at_ms: at,
                recorded_at_ms: at,
            },
            fingerprint,
            write: ApprovalWriteCache::default(),
        });
        operation
            .state
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .arm = Some(arm.clone());
        self.require_approval_fact(source, operation.id.entity())
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        let deadline = self
            .deadline()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        let stored = match self.repository.arm_approval_reply(&arm, &deadline).await {
            Ok(stored) => stored,
            Err(error) => {
                self.cancel_approval_reservation(operation);
                return Err(error);
            }
        };
        operation
            .state
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .arm_stored = Some(stored);
        self.confirm_approval_fact(source, operation.id.entity())
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        #[cfg(all(test,feature="approval-owner-acceptance-fixtures"))]
        self.repository.approval_native_test_cut(&decision.probe,crate::session_storage::approval_owner::test_control::ApprovalCut::BeforeSelectedEnqueue,operation.id,NativeRuntimeOptions::at(self.options.metadata_wait).map_err(|_|ApprovalRejection::NecessaryStorageUnavailable)?).await.map_err(|_|ApprovalRejection::NecessaryStorageUnavailable)?;
        let (reservation, frame) = {
            let mut state = operation
                .state
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            (
                state
                    .reservation
                    .take()
                    .ok_or(ApprovalRejection::InvalidRequest)?,
                state
                    .frame
                    .take()
                    .ok_or(ApprovalRejection::InvalidRequest)?,
            )
        };
        let boundary = Arc::new(SelectedReplyBoundary {
            operation: Arc::downgrade(operation),
            source: source.pre.clone(),
        });
        let write_until = *operation
            .write_until
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        let ticket = match self.protocol.enqueue_permission_reply(
            connection,
            reservation,
            frame,
            boundary,
            write_until,
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
                self.cancel_approval_reservation(operation);
                return self
                    .record_approval_actual_terminal(source, operation, &arm)
                    .await;
            }
        };
        {
            let mut state = match operation.state.lock() {
                Ok(state) => state,
                Err(poisoned) => poisoned.into_inner(),
            };
            state.ticket = Some(ticket.clone());
        }
        let _ = ticket.wait().await;
        self.record_approval_actual_terminal(source, operation, &arm)
            .await
    }
    pub(super) fn cancel_approval_reservation(&self, operation: &ApprovalResponseOperation<P>) {
        let mut state = match operation.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(reservation) = state.reservation.take() {
            state.cancelled = Some(self.protocol.cancel_reserved_permission_reply(reservation));
        }
        state.frame.take();
    }
    pub(super) async fn record_approval_actual_terminal(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        operation: &Arc<ApprovalResponseOperation<P>>,
        arm: &Arc<ApprovalReplyArmProof<P>>,
    ) -> Result<(), NecessaryWriteError> {
        let progress = {
            let state = operation
                .state
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            self.protocol.probe_progress(
                state
                    .probe
                    .as_ref()
                    .ok_or(ApprovalRejection::InvalidRequest)?,
            )
        };
        let TransportProgress::Finished(fact) = progress else {
            return Err(ApprovalRejection::NecessaryStorageUnavailable.into());
        };
        #[cfg(test)]
        if matches!(fact, TransportFact::Zero { .. }) {
            let state = operation
                .state
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            let source = &operation.saved.admission.original.source;
            let flags = (
                source.authority.closed.load(Ordering::SeqCst),
                source.authority.closing.load(Ordering::SeqCst),
                source.authority.quarantined.load(Ordering::SeqCst),
                source.authority.required.load(Ordering::SeqCst),
            );
            eprintln!(
                "approval developer actual Zero boundary={:?}; source closed/closing/quarantine/required={flags:?}",
                state.last_boundary
            );
        }
        self.require_approval_fact(source, operation.id.entity())
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        let (observation, accepted) = match fact {
            TransportFact::Zero { reason } => (TerminalReplyObservation::Zero { reason }, 0),
            TransportFact::Written => (
                TerminalReplyObservation::Written,
                arm.metadata().frame_length,
            ),
            TransportFact::Unknown { accepted, cause } => (
                TerminalReplyObservation::Unknown { accepted, cause },
                accepted,
            ),
        };
        let existing = operation
            .state
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .terminal
            .clone();
        let terminal = if let Some(existing) = existing {
            if existing.metadata().observation != observation
                || existing.metadata().accepted_length != accepted
            {
                return Err(ApprovalRejection::Conflict.into());
            }
            existing
        } else {
            let at = self.clock.unix_millis();
            let label = match fact {
                TransportFact::Zero { reason } => reason.label(),
                TransportFact::Written => "written",
                TransportFact::Unknown { cause, .. } => cause.label(),
            };
            let fingerprint = digest_data(
                b"acpr-approval-terminal-v1\0",
                &[
                    arm.fingerprint(),
                    label.as_bytes(),
                    &accepted.to_be_bytes(),
                    &at.to_be_bytes(),
                ],
            )
            .map_err(|_| ApprovalRejection::LengthUnrepresentable)?;

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
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .terminal = Some(terminal.clone());
        let deadline = self
            .deadline()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        let stored = self
            .repository
            .record_approval_reply(&terminal, &deadline)
            .await?;
        operation
            .state
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .terminal_stored = Some(stored.clone());
        if let Some(approval) = operation.approval.upgrade() {
            let target = approval
                .association
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
                .as_ref()
                .map(|association| association.metadata().target);
            self.append_owned_approval_audit(
                source,
                &approval,
                super::approval_audit::ApprovalAuditBasis::delivery(
                    arm.clone(),
                    terminal.clone(),
                    stored,
                    target,
                ),
            )
            .await;
        }
        if fact == TransportFact::Written
            && let Some(permission) = operation.permission.upgrade()
        {
            source
                .capacity_activity
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
                .touch(&permission.session, std::time::Instant::now())
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            permission
                .state
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
                .written = true;
        }
        self.confirm_approval_fact(source, operation.id.entity())
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        operation.changed.notify_waiters();
        Ok(())
    }
}
