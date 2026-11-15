//! Original safe Cancelled responses share the original permission slot and mandatory facts.
#![allow(
    clippy::significant_drop_tightening,
    clippy::unused_self,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    ApprovalReplyArmProof, ApprovalReplyTerminalProof, Arc, ConfigurationPort, ControlPhase,
    DispatchError, NativeApprovalPort, NativeApprovalRepository, NativeControlRepository,
    NativeCoordinator, NativeDispatchRepository, NativeResourceRepository, OwnedNativeSource,
    SourceLaunchPort, TransportFact,
    approval_inventory::{ApprovalSlot, digest_data},
    approval_records::ApprovalWriteCache,
    control::ControlOperation,
    runtime::SourceSlot,
};
use crate::domain::{
    approval_owner::{
        ActualReplyKind, ApprovalReplyId, NecessaryWriteError, TerminalReplyObservation,
    },
    approval_records::{
        ApprovalAttemptRecord, ApprovalTerminalRecord, StoredApprovalAttempt,
        StoredApprovalTerminal,
    },
};
use std::sync::Mutex;

pub(super) struct ApprovalControlFacts<P: NativeApprovalPort> {
    pub(super) original: Arc<ControlOperation<P>>,
    pub(super) response: ApprovalReplyId,
    pub(super) serial: u64,
    pub(super) approval: std::sync::Weak<ApprovalSlot<P>>,
    pub(super) arm: Option<Arc<ApprovalReplyArmProof<P>>>,
    pub(super) state: Mutex<ApprovalControlFactState<P>>,
}
pub(super) struct ApprovalControlFactState<P: NativeApprovalPort> {
    pub(super) arm_stored: Option<StoredApprovalAttempt>,
    pub(super) terminal: Option<Arc<ApprovalReplyTerminalProof<P>>>,
    pub(super) terminal_stored: Option<StoredApprovalTerminal>,
}
pub(super) const fn native_error(error: NecessaryWriteError) -> DispatchError {
    match error {
        NecessaryWriteError::CommitUnconfirmed => DispatchError::CommitUnconfirmed,
        NecessaryWriteError::SqlCloseUnconfirmed { commit_confirmed } => {
            DispatchError::SqlCloseUnconfirmed { commit_confirmed }
        }
        NecessaryWriteError::Rejected(_) => DispatchError::RequiredStateUnavailable,
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
    pub(super) fn retain_approval_control(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        operation: &Arc<ControlOperation<P>>,
    ) -> Result<Option<Arc<ApprovalControlFacts<P>>>, DispatchError> {
        let Some((scope, obligation, metadata, origin)) = operation.approval_reservation()? else {
            return Ok(None);
        };
        let permission = metadata
            .rpc
            .as_ref()
            .and_then(|rpc| source.permissions.lock().ok()?.get(rpc).cloned())
            .ok_or(DispatchError::InvalidScope)?;
        let approval = permission
            .approval
            .get()
            .ok_or(DispatchError::RequiredStateUnavailable)?
            .clone();
        if scope != permission.scope
            || permission.snapshot()?.obligation != Some(obligation)
            || metadata.source != *source.pre.metadata()
            || metadata.scope_binding != approval.request.metadata().scope_binding
        {
            return Err(DispatchError::InvalidScope);
        }
        let mut state = approval
            .state
            .lock()
            .map_err(|_| DispatchError::RequiredStateUnavailable)?;
        if let Some(existing) = state.controls.get(&metadata.control_id) {
            if !Arc::ptr_eq(&existing.original, operation) {
                return Err(DispatchError::Conflict);
            }
            return Ok(Some(existing.clone()));
        }
        let serial = state.next_reply_serial;
        state.next_reply_serial = serial
            .checked_add(1)
            .ok_or(DispatchError::SequenceExhausted)?;
        let response = ApprovalReplyId::from_entity(metadata.control_id);
        let arm = if let Some(length) = metadata.frame_length {
            if length == 0 {
                return Err(DispatchError::Conflict);
            }
            let fingerprint = digest_data(
                b"acpr-approval-cancel-arm-v1\0",
                &[
                    &metadata.fingerprint,
                    &serial.to_be_bytes(),
                    &length.to_be_bytes(),
                ],
            )?;
            Some(Arc::new(ApprovalReplyArmProof {
                original: approval.request.original.clone(),
                metadata: ApprovalAttemptRecord {
                    response,
                    approval: approval.request.metadata().approval,
                    decision_operation: None,
                    serial,
                    kind: ActualReplyKind::Cancelled,
                    origin,
                    option_digest: None,
                    frame_length: length,
                    reserved_fingerprint: fingerprint,
                    observed_at_ms: metadata.at,
                    recorded_at_ms: metadata.at,
                },
                fingerprint,
                write: ApprovalWriteCache::default(),
            }))
        } else {
            None
        };
        let facts = Arc::new(ApprovalControlFacts {
            original: operation.clone(),
            response,
            serial,
            approval: Arc::downgrade(&approval),
            arm,
            state: Mutex::new(ApprovalControlFactState {
                arm_stored: None,
                terminal: None,
                terminal_stored: None,
            }),
        });
        state.controls.insert(metadata.control_id, facts.clone());
        Ok(Some(facts))
    }
    pub(super) async fn persist_approval_control_arm(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        operation: &Arc<ControlOperation<P>>,
    ) -> Result<(), DispatchError> {
        let Some(facts) = self.retain_approval_control(source, operation)? else {
            return Ok(());
        };
        let Some(arm) = &facts.arm else {
            return Ok(());
        };
        let stored = self
            .repository
            .arm_approval_reply(arm, &self.deadline()?)
            .await
            .map_err(native_error)?;
        facts
            .state
            .lock()
            .map_err(|_| DispatchError::RequiredStateUnavailable)?
            .arm_stored = Some(stored);
        if let Some(approval) = facts.approval.upgrade() {
            approval.publish_change();
        }
        Ok(())
    }
    pub(super) async fn persist_approval_control_terminal(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        operation: &Arc<ControlOperation<P>>,
    ) -> Result<(), DispatchError> {
        let Some(facts) = self.retain_approval_control(source, operation)? else {
            return Ok(());
        };
        let Some(arm) = &facts.arm else {
            return Ok(());
        };
        self.persist_approval_control_arm(source, operation).await?;
        let native = operation
            .approval_terminal()?
            .ok_or(DispatchError::ResourceUnconfirmed)?;
        let ControlPhase::Terminal(fact) = native.phase else {
            return Err(DispatchError::ResourceUnconfirmed);
        };
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
        let existing = facts
            .state
            .lock()
            .map_err(|_| DispatchError::RequiredStateUnavailable)?
            .terminal
            .clone();
        let terminal = if let Some(existing) = existing {
            if existing.metadata().observation != observation {
                return Err(DispatchError::Conflict);
            }
            existing
        } else {
            let fingerprint = digest_data(
                b"acpr-approval-cancel-terminal-v1\0",
                &[
                    arm.fingerprint(),
                    &native.fingerprint,
                    &accepted.to_be_bytes(),
                ],
            )?;
            Arc::new(ApprovalReplyTerminalProof {
                original: arm.original.clone(),
                metadata: ApprovalTerminalRecord {
                    response: facts.response,
                    approval: arm.metadata().approval,
                    observation,
                    accepted_length: accepted,
                    fingerprint,
                    observed_at_ms: native.at,
                    recorded_at_ms: native.at,
                },
                fingerprint,
                write: ApprovalWriteCache::default(),
            })
        };
        facts
            .state
            .lock()
            .map_err(|_| DispatchError::RequiredStateUnavailable)?
            .terminal = Some(terminal.clone());
        let stored = self
            .repository
            .record_approval_reply(&terminal, &self.deadline()?)
            .await
            .map_err(native_error)?;
        facts
            .state
            .lock()
            .map_err(|_| DispatchError::RequiredStateUnavailable)?
            .terminal_stored = Some(stored.clone());
        if let Some(approval) = facts.approval.upgrade() {
            let target = approval
                .association
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?
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
        if let Some(approval) = facts.approval.upgrade() {
            approval.publish_change();
        }
        Ok(())
    }
}
