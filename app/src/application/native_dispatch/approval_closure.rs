//! Exact necessary approval collection attested by the original sealed Source producers.
#![allow(
    clippy::too_many_lines,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    ApprovalInvalidationProof, Arc, ConfigurationPort, DispatchError, NativeApprovalPort,
    NativeApprovalRepository, NativeControlRepository, NativeCoordinator, NativeDispatchRepository,
    NativeResourceRepository, OwnedNativeSource, SourceLaunchPort, TransportFact,
    TransportProgress,
    approval_control::native_error,
    approval_inventory::{ApprovalSlot, digest_data},
    approval_records::ApprovalWriteCache,
    environment::SourcePermissionSlot,
    runtime::SourceSlot,
};
use crate::domain::{
    approval_owner::{ApprovalInvalidationReason, ApprovalRejection},
    approval_records::{
        ApprovalAssociationRecord, ApprovalAttemptRecord, ApprovalCommandRecord,
        ApprovalDecisionRecord, ApprovalInvalidationRecord, ApprovalRequestRecord,
        ApprovalTerminalRecord, StoredApprovalCommandState,
    },
};
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalClosureManifest {
    pub(super) requests: Vec<ApprovalRequestRecord>,
    pub(super) associations: Vec<ApprovalAssociationRecord>,
    pub(super) commands: Vec<ApprovalCommandRecord>,
    pub(super) decisions: Vec<ApprovalDecisionRecord>,
    pub(super) attempts: Vec<ApprovalAttemptRecord>,
    pub(super) terminals: Vec<ApprovalTerminalRecord>,
    pub(super) invalidations: Vec<ApprovalInvalidationRecord>,
    pub(super) stages: Vec<[u8; 32]>,
    pub(super) fingerprint: [u8; 32],
}
impl ApprovalClosureManifest {
    #[must_use]
    pub fn requests(&self) -> &[ApprovalRequestRecord] {
        &self.requests
    }
    #[must_use]
    pub fn associations(&self) -> &[ApprovalAssociationRecord] {
        &self.associations
    }
    #[must_use]
    pub fn commands(&self) -> &[ApprovalCommandRecord] {
        &self.commands
    }
    #[must_use]
    pub fn decisions(&self) -> &[ApprovalDecisionRecord] {
        &self.decisions
    }
    #[must_use]
    pub fn attempts(&self) -> &[ApprovalAttemptRecord] {
        &self.attempts
    }
    #[must_use]
    pub fn terminals(&self) -> &[ApprovalTerminalRecord] {
        &self.terminals
    }
    #[must_use]
    pub fn invalidations(&self) -> &[ApprovalInvalidationRecord] {
        &self.invalidations
    }
    #[must_use]
    pub const fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
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
    pub(super) fn ensure_closed_approval_inventory(
        &self,
        source: &Arc<SourceSlot<P, L>>,
    ) -> Result<(), DispatchError> {
        Self::source_quiescence(source)?.flags(source)?;
        if source
            .physical_closed
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .is_none()
        {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        let permissions = source
            .permissions
            .lock()
            .map_err(|_| DispatchError::RequiredStateUnavailable)?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for permission in permissions {
            if permission.approval.get().is_none() {
                self.create_approval_inventory(source, &permission)?;
            }
        }
        Ok(())
    }
    pub(super) async fn reconcile_approval_requests(
        &self,
        source: &Arc<SourceSlot<P, L>>,
    ) -> Result<(), DispatchError> {
        let permissions = source
            .permissions
            .lock()
            .map_err(|_| DispatchError::RequiredStateUnavailable)?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for permission in permissions {
            let slot = permission
                .approval
                .get()
                .ok_or(DispatchError::RequiredStateUnavailable)?;
            self.repair_pending_approval_association(source, slot)
                .await
                .map_err(native_error)?;
            self.persist_approval_observation(source, slot).await;
            if slot
                .state
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?
                .request_error
                .is_some()
            {
                return Err(DispatchError::RequiredStateUnavailable);
            }
        }
        Ok(())
    }
    pub(super) async fn collect_approval_closure(
        &self,
        source: &Arc<SourceSlot<P, L>>,
    ) -> Result<Arc<ApprovalClosureManifest>, DispatchError> {
        Self::source_quiescence(source)?.flags(source)?;
        let until = self.deadline()?.deadline();
        let owner = self.repository.approval_owner_scope();
        let mut manifest = ApprovalClosureManifest {
            requests: Vec::new(),
            associations: Vec::new(),
            commands: Vec::new(),
            decisions: Vec::new(),
            attempts: Vec::new(),
            terminals: Vec::new(),
            invalidations: Vec::new(),
            stages: Vec::new(),
            fingerprint: [0; 32],
        };
        let mut permissions = source
            .permissions
            .lock()
            .map_err(|_| DispatchError::RequiredStateUnavailable)?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        permissions.sort_by_key(|permission| {
            permission
                .approval
                .get()
                .map(|slot| slot.request.metadata().approval.encode())
        });
        for permission in permissions {
            let slot = permission
                .approval
                .get()
                .ok_or(DispatchError::RequiredStateUnavailable)?
                .clone();
            if !slot
                .request
                .write_cache()
                .confirm_all_cleanup(&owner, until)
                .await
                .map_err(native_error)?
            {
                return Err(DispatchError::ResourceUnconfirmed);
            }
            if !slot.request.resolution_inventory_complete() {
                return Err(DispatchError::ResourceUnconfirmed);
            }
            if !slot
                .request
                .resolution_cache()
                .confirm_all_cleanup(&owner, until)
                .await
                .map_err(native_error)?
            {
                return Err(DispatchError::ResourceUnconfirmed);
            }
            let association_operation = {
                slot.association_resolution
                    .lock()
                    .map_err(|_| DispatchError::RequiredStateUnavailable)?
                    .clone()
            };
            if let Some(operation) = association_operation {
                let unpublished = {
                    operation
                        .candidate
                        .lock()
                        .map_err(|_| DispatchError::RequiredStateUnavailable)?
                        .clone()
                };
                if let Some(candidate) = unpublished {
                    if !candidate
                        .write_cache()
                        .confirm_all_cleanup(&owner, until)
                        .await
                        .map_err(native_error)?
                    {
                        return Err(DispatchError::ResourceUnconfirmed);
                    }
                    if slot
                        .association
                        .lock()
                        .map_err(|_| DispatchError::RequiredStateUnavailable)?
                        .is_none()
                        && !candidate.mapping_rejected_before_commit()
                    {
                        return Err(DispatchError::ResourceUnconfirmed);
                    }
                }
            }
            manifest.requests.push(slot.request.metadata().clone());
            let association = slot
                .association
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?
                .clone();
            if let Some(association) = association {
                if !association
                    .write_cache()
                    .confirm_all_cleanup(&owner, until)
                    .await
                    .map_err(native_error)?
                {
                    return Err(DispatchError::ResourceUnconfirmed);
                }
                manifest.associations.push(association.metadata().clone());
            }
            let (registrations, admission, responses, controls, rejected_admissions) = {
                let state = slot
                    .state
                    .lock()
                    .map_err(|_| DispatchError::RequiredStateUnavailable)?;
                (
                    state.registered.values().cloned().collect::<Vec<_>>(),
                    state.admission.clone(),
                    state.responses.clone(),
                    state.controls.values().cloned().collect::<Vec<_>>(),
                    state.rejected_admissions.clone(),
                )
            };
            for registration in &registrations {
                if let Some(rejection) = registration
                    .probe
                    .write_cache()
                    .final_rejection()
                    .map_err(native_error)?
                {
                    if !registration
                        .probe
                        .write_cache()
                        .confirm_all_cleanup(&owner, until)
                        .await
                        .map_err(native_error)?
                    {
                        return Err(DispatchError::ResourceUnconfirmed);
                    }
                    self.confirm_approval_fact(
                        source,
                        registration.probe.metadata().registration.entity(),
                    )?;
                    if rejection == ApprovalRejection::InvalidActor {
                        manifest.stages.push(digest_data(
                            b"acpr-approval-rejected-registration-v1\0",
                            &[registration.probe.fingerprint()],
                        )?);
                    } else {
                        let reason = match rejection {
                            ApprovalRejection::InvalidActor => "invalid_actor",
                            ApprovalRejection::UnknownRegistration => "unknown_registration",
                            ApprovalRejection::Conflict => "conflict",
                            ApprovalRejection::AlreadyDecided => "already_decided",
                            ApprovalRejection::InvalidRequest => "invalid_request",
                            ApprovalRejection::UnassociatedTarget => "unassociated_target",
                            ApprovalRejection::InvalidChoice => "invalid_choice",
                            ApprovalRejection::LengthUnrepresentable => "length_unrepresentable",
                            ApprovalRejection::Stopped => "stopped",
                            ApprovalRejection::NecessaryStorageUnavailable => {
                                "necessary_storage_unavailable"
                            }
                        };
                        manifest.stages.push(digest_data(
                            b"acpr-approval-final-registration-rejection-v1\0",
                            &[registration.probe.fingerprint(), reason.as_bytes()],
                        )?);
                    }
                    continue;
                }
                let known = registration
                    .result
                    .lock()
                    .map_err(|_| DispatchError::RequiredStateUnavailable)?
                    .clone();
                if !matches!(known, Some(Ok(_))) {
                    let stored = self
                        .repository
                        .register_approval(&registration.probe, &self.deadline()?)
                        .await
                        .map_err(native_error)?;
                    *registration
                        .result
                        .lock()
                        .map_err(|_| DispatchError::RequiredStateUnavailable)? = Some(Ok(stored));
                }
                if !registration
                    .probe
                    .write_cache()
                    .confirm_all_cleanup(&owner, until)
                    .await
                    .map_err(native_error)?
                {
                    return Err(DispatchError::ResourceUnconfirmed);
                }
                self.confirm_approval_fact(
                    source,
                    registration.probe.metadata().registration.entity(),
                )?;
                manifest
                    .commands
                    .push(registration.probe.metadata().clone());
            }
            let mut rejected_admissions = rejected_admissions;
            let admission = if let Some(admission) = admission {
                if admission
                    .probe
                    .write_cache()
                    .final_rejection()
                    .map_err(native_error)?
                    == Some(ApprovalRejection::InvalidActor)
                {
                    rejected_admissions.push(admission);
                    None
                } else {
                    Some(admission)
                }
            } else {
                None
            };
            for rejected in rejected_admissions {
                if !rejected
                    .probe
                    .write_cache()
                    .confirm_all_cleanup(&owner, until)
                    .await
                    .map_err(native_error)?
                {
                    return Err(DispatchError::ResourceUnconfirmed);
                }
                self.confirm_approval_fact(source, rejected.probe.metadata().operation)?;
                manifest.stages.push(digest_data(
                    b"acpr-approval-rejected-admission-v1\0",
                    &[rejected.probe.fingerprint()],
                )?);
            }
            if let Some(admission) = admission {
                let stored = self
                    .repository
                    .decide_approval(&admission.probe, &self.deadline()?)
                    .await
                    .map_err(native_error)?;
                if !admission
                    .probe
                    .write_cache()
                    .confirm_all_cleanup(&owner, until)
                    .await
                    .map_err(native_error)?
                {
                    return Err(DispatchError::ResourceUnconfirmed);
                }
                for command in &mut manifest.commands {
                    if command.registration == stored.record().registration {
                        command.state = StoredApprovalCommandState::Accepted;
                        command.operation = Some(stored.record().operation);
                        command.admission_auth_generation =
                            Some(stored.record().admission_auth_generation);
                        command.admitted_at_ms = Some(stored.record().accepted_at_ms);
                    }
                }
                manifest.decisions.push(stored.record().clone());
                slot.state
                    .lock()
                    .map_err(|_| DispatchError::RequiredStateUnavailable)?
                    .decision = Some(stored);
                self.confirm_approval_fact(source, admission.probe.metadata().operation)?;
            }
            for response in responses {
                self.cancel_approval_reservation(&response);
                let (arm, probe) = {
                    let state = response
                        .state
                        .lock()
                        .map_err(|_| DispatchError::RequiredStateUnavailable)?;
                    (state.arm.clone(), state.probe.clone())
                };
                let actual = if let Some(probe) = probe {
                    let TransportProgress::Finished(fact) = self.protocol.probe_progress(&probe)
                    else {
                        return Err(DispatchError::ResourceUnconfirmed);
                    };
                    Some(fact)
                } else {
                    None
                };
                let mut stage = Vec::new();
                stage.extend_from_slice(response.id.encode().as_bytes());
                stage.push(u8::from(arm.is_some()));
                stage.push(u8::from(actual.is_some()));
                if let Some(fact) = actual {
                    match fact {
                        TransportFact::Written => stage.extend_from_slice(b"written"),
                        TransportFact::Zero { reason } => {
                            stage.extend_from_slice(reason.label().as_bytes());
                        }
                        TransportFact::Unknown { accepted, cause } => {
                            stage.extend_from_slice(cause.label().as_bytes());
                            stage.extend_from_slice(&accepted.to_be_bytes());
                        }
                    }
                }
                manifest.stages.push(digest_data(
                    b"acpr-approval-closed-response-v1\0",
                    &[&stage],
                )?);
                if let Some(arm) = arm {
                    let stored = self
                        .repository
                        .arm_approval_reply(&arm, &self.deadline()?)
                        .await
                        .map_err(native_error)?;
                    response
                        .state
                        .lock()
                        .map_err(|_| DispatchError::RequiredStateUnavailable)?
                        .arm_stored = Some(stored);
                    self.record_approval_actual_terminal(source, &response, &arm)
                        .await
                        .map_err(native_error)?;
                    if !arm
                        .write_cache()
                        .confirm_all_cleanup(&owner, until)
                        .await
                        .map_err(native_error)?
                    {
                        return Err(DispatchError::ResourceUnconfirmed);
                    }
                    let terminal = response
                        .state
                        .lock()
                        .map_err(|_| DispatchError::RequiredStateUnavailable)?
                        .terminal
                        .clone()
                        .ok_or(DispatchError::ResourceUnconfirmed)?;
                    if !terminal
                        .write_cache()
                        .confirm_all_cleanup(&owner, until)
                        .await
                        .map_err(native_error)?
                    {
                        return Err(DispatchError::ResourceUnconfirmed);
                    }
                    manifest.attempts.push(arm.metadata().clone());
                    manifest.terminals.push(terminal.metadata().clone());
                } else {
                    self.confirm_approval_fact(source, response.id.entity())?;
                }
            }
            for control in controls {
                self.persist_approval_control_terminal(source, &control.original)
                    .await?;
                if let Some(arm) = &control.arm {
                    let terminal = control
                        .state
                        .lock()
                        .map_err(|_| DispatchError::RequiredStateUnavailable)?
                        .terminal
                        .clone()
                        .ok_or(DispatchError::ResourceUnconfirmed)?;
                    if !arm
                        .write_cache()
                        .confirm_all_cleanup(&owner, until)
                        .await
                        .map_err(native_error)?
                        || !terminal
                            .write_cache()
                            .confirm_all_cleanup(&owner, until)
                            .await
                            .map_err(native_error)?
                    {
                        return Err(DispatchError::ResourceUnconfirmed);
                    }
                    manifest.attempts.push(arm.metadata().clone());
                    manifest.terminals.push(terminal.metadata().clone());
                }
                let (_, _, reservation, _) = control
                    .original
                    .approval_reservation()?
                    .ok_or(DispatchError::InvalidScope)?;
                let terminal = control
                    .original
                    .approval_terminal()?
                    .ok_or(DispatchError::ResourceUnconfirmed)?;
                manifest.stages.push(digest_data(
                    b"acpr-approval-closed-control-v1\0",
                    &[
                        &reservation.fingerprint,
                        &terminal.fingerprint,
                        &control.serial.to_be_bytes(),
                    ],
                )?);
            }
            let (failure_arms, failure_terminals, failure_stages) =
                self.collect_approval_failure_records(source, &slot).await?;
            manifest.attempts.extend(failure_arms);
            manifest.terminals.extend(failure_terminals);
            manifest.stages.extend(failure_stages);
            self.close_approval_invalidation(source, &permission, &slot)
                .await?;
            let invalidations = slot
                .state
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?
                .invalidations
                .clone();
            for invalidation in invalidations {
                if !invalidation
                    .write_cache()
                    .confirm_all_cleanup(&owner, until)
                    .await
                    .map_err(native_error)?
                {
                    return Err(DispatchError::ResourceUnconfirmed);
                }
                manifest.invalidations.push(invalidation.metadata().clone());
            }
        }
        manifest.requests.sort_by_key(|r| r.approval.encode());
        manifest.associations.sort_by_key(|r| r.approval.encode());
        manifest.commands.sort_by_key(|r| r.registration.encode());
        manifest.decisions.sort_by_key(|r| r.approval.encode());
        manifest.attempts.sort_by_key(|r| r.response.encode());
        manifest.terminals.sort_by_key(|r| r.response.encode());
        manifest.invalidations.sort_by_key(|r| r.id.encode());
        manifest.stages.sort_unstable();
        let mut parts = Vec::new();
        for request in &manifest.requests {
            parts.extend_from_slice(request.approval.encode().as_bytes());
            parts.extend_from_slice(&request.request_digest);
            parts.extend_from_slice(&request.observed_sequence.to_be_bytes());
            parts.extend_from_slice(&request.observed_receipt.to_be_bytes());
        }
        for association in &manifest.associations {
            parts.extend_from_slice(&association.fingerprint);
        }
        for command in &manifest.commands {
            parts.extend_from_slice(command.registration.encode().as_bytes());
            parts.extend_from_slice(&command.intent_fingerprint);
        }
        for decision in &manifest.decisions {
            parts.extend_from_slice(&decision.fingerprint);
        }
        for attempt in &manifest.attempts {
            parts.extend_from_slice(&attempt.reserved_fingerprint);
        }
        for terminal in &manifest.terminals {
            parts.extend_from_slice(&terminal.fingerprint);
        }
        for invalidation in &manifest.invalidations {
            parts.extend_from_slice(&invalidation.fingerprint);
        }
        for stage in &manifest.stages {
            parts.extend_from_slice(stage);
        }
        manifest.fingerprint = digest_data(
            b"acpr-approval-source-closed-v1\0",
            &[
                source.pre.metadata().instance_id.encode().as_bytes(),
                &parts,
            ],
        )?;
        Ok(Arc::new(manifest))
    }
    async fn close_approval_invalidation(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        _permission: &Arc<SourcePermissionSlot<P>>,
        slot: &Arc<ApprovalSlot<P>>,
    ) -> Result<(), DispatchError> {
        let existing = slot
            .state
            .lock()
            .map_err(|_| DispatchError::RequiredStateUnavailable)?
            .invalidations
            .iter()
            .find(|proof| proof.metadata().reason == ApprovalInvalidationReason::SourceClosed)
            .cloned();
        let proof = if let Some(existing) = existing {
            existing
        } else {
            let id = Self::new_id()?;
            let at = self.clock.unix_millis();
            let fingerprint = digest_data(
                b"acpr-approval-source-invalidation-v1\0",
                &[
                    slot.request.fingerprint(),
                    source.pre.metadata().instance_id.encode().as_bytes(),
                    &at.to_be_bytes(),
                ],
            )?;
            let proof = Arc::new(ApprovalInvalidationProof {
                original: slot.request.original.clone(),
                metadata: ApprovalInvalidationRecord {
                    id,
                    approval: slot.request.metadata().approval,
                    reason: ApprovalInvalidationReason::SourceClosed,
                    fingerprint,
                    observed_sequence: None,
                    observed_at_ms: at,
                    recorded_at_ms: at,
                },
                fingerprint,
                write: ApprovalWriteCache::default(),
            });
            slot.state
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?
                .invalidations
                .push(proof.clone());
            proof
        };
        self.require_approval_fact(source, proof.metadata().id)?;
        let stored = self
            .repository
            .invalidate_approval(&proof, &self.deadline()?)
            .await
            .map_err(native_error)?;
        let target = slot
            .association
            .lock()
            .map_err(|_| DispatchError::RequiredStateUnavailable)?
            .as_ref()
            .map(|association| association.metadata().target);
        self.append_closing_approval_audit(
            slot,
            super::approval_audit::ApprovalAuditBasis::invalidated(proof.clone(), stored, target),
        )
        .await;
        self.confirm_approval_fact(source, proof.metadata().id)?;
        slot.publish_change();
        Ok(())
    }
}
