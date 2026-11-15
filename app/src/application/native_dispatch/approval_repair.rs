//! Original probes only: explicit SQL repair does not reserve, enqueue or create a source.
#![allow(
    clippy::manual_let_else,
    clippy::match_wildcard_for_single_variants,
    clippy::significant_drop_tightening,
    clippy::too_many_lines,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    Arc, ConfigurationPort, DispatchError, Instant, NativeApprovalPort, NativeApprovalRepository,
    NativeControlRepository, NativeCoordinator, NativeDispatchRepository, NativeResourceRepository,
    Ordering, OwnedNativeSource, SourceLaunchPort,
    approval_inventory::ApprovalSlot,
    approval_records::ApprovalWriteCache,
    approval_recovery::{
        ApprovalReconcileOutcome, ApprovalReconcileProgress, ApprovalReconcileWaiter,
        ApprovalRecoveryBlock, ApprovalRecoveryOperation, ApprovalRepairMode, ApprovalRepairScope,
        OriginalApprovalReconciler, RecoveryKey,
    },
    approval_response::DecisionSavedCertificate,
    environment::SourcePermissionSlot,
    runtime::SourceSlot,
};
use crate::{
    application::request_registry::RegistryDeadline,
    domain::{
        approval_owner::{ApprovalId, ApprovalRejection, NecessaryWriteError},
        approval_records::StoredApproval,
    },
};
const fn blocked(reason: ApprovalRecoveryBlock) -> ApprovalReconcileOutcome {
    ApprovalReconcileOutcome::StillBlocked { reason }
}
const fn sql_error(error: NecessaryWriteError) -> ApprovalRecoveryBlock {
    match error {
        NecessaryWriteError::CommitUnconfirmed => ApprovalRecoveryBlock::CommitUnconfirmed,
        NecessaryWriteError::SqlCloseUnconfirmed { .. } => {
            ApprovalRecoveryBlock::CleanupUnconfirmed
        }
        _ => ApprovalRecoveryBlock::NecessaryStorageUnavailable,
    }
}
impl<P, L, G, N> OriginalApprovalReconciler<P, L, G, N>
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
    pub(crate) fn start_repair(
        &self,
        scope: ApprovalRepairScope,
        mode: ApprovalRepairMode,
        until: Instant,
    ) -> Result<ApprovalReconcileWaiter, ApprovalRejection> {
        let core = self.core()?;
        if until <= Instant::now() {
            return Err(ApprovalRejection::Stopped);
        }
        let source = match scope {
            ApprovalRepairScope::Request(id) => Self::locate(&core, id)?.0,
            ApprovalRepairScope::SourceInstance(instance) => core
                .sources
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
                .iter()
                .find(|source| source.pre.metadata().instance_id == instance)
                .cloned()
                .ok_or(ApprovalRejection::InvalidRequest)?,
        };
        let (operation, new) = Self::adopt(&source, RecoveryKey::Repair(scope, mode))?;
        let waiter = ApprovalReconcileWaiter {
            original: operation.clone(),
        };
        if !new {
            return Ok(waiter);
        }
        if source.pre.authority.closing.load(Ordering::SeqCst)
            || source.pre.authority.closed.load(Ordering::SeqCst)
        {
            NativeCoordinator::<P, L, G, N>::request_source_metadata_retry(&source);
            operation.complete(
                blocked(ApprovalRecoveryBlock::SourceClosing),
                match scope {
                    ApprovalRepairScope::Request(id) => Some(id),
                    _ => None,
                },
            );
            return Ok(waiter);
        }
        let task_core = core.clone();
        let task_source = source.clone();
        let op = operation.clone();
        let failed = operation.clone();
        let failed_core = core;
        let failed_source = source.clone();
        let job = if let Ok(job) = source.jobs.spawn_guarded(
            async move {
                op.advance(ApprovalReconcileProgress::WaitingForOriginalSql);
                let result = task_core
                    .reconcile_original_approvals(&task_source, scope, mode, until, &op)
                    .await;
                op.complete(
                    result,
                    match scope {
                        ApprovalRepairScope::Request(id) => Some(id),
                        _ => None,
                    },
                );
            },
            move || {
                failed_core.fail_source(&failed_source, DispatchError::RequiredStateUnavailable);
                failed.complete(ApprovalReconcileOutcome::OriginalUnavailable, None);
            },
        ) {
            job
        } else {
            operation.complete(ApprovalReconcileOutcome::OriginalUnavailable, None);
            return Err(ApprovalRejection::Stopped);
        };
        let _ = operation.job.set(job);
        Ok(waiter)
    }
    pub(crate) fn start_resume_saved_decision(
        &self,
        id: ApprovalId,
        until: Instant,
    ) -> Result<ApprovalReconcileWaiter, ApprovalRejection> {
        let core = self.core()?;
        let (source, permission, slot) = Self::locate(&core, id)?;
        Self::live(&core, &source, &permission)?;
        if until <= Instant::now() {
            return Err(ApprovalRejection::Stopped);
        }
        let (operation, new) = Self::adopt(&source, RecoveryKey::Resume(id))?;
        let waiter = ApprovalReconcileWaiter {
            original: operation.clone(),
        };
        if !new {
            return Ok(waiter);
        }
        let (decision, existing, required_metadata) = {
            let state = slot
                .state
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            if state.allow_disabled
                || state.retry_nonce.is_some()
                || !state.invalidations.is_empty()
            {
                operation.complete(blocked(ApprovalRecoveryBlock::RequestInvalid), Some(id));
                return Ok(waiter);
            }
            let decision = state
                .admission
                .clone()
                .filter(|op| op.saved.get().is_some())
                .ok_or(ApprovalRejection::NecessaryStorageUnavailable)?;
            let existing = state.responses.last().cloned();
            let required_metadata = source
                .memory
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
                .requirements
                .keys()
                .copied()
                .collect::<Vec<_>>();
            (decision, existing, required_metadata)
        };
        let resumable_response = existing.as_ref().map(|response| response.id.entity());
        if required_metadata
            .iter()
            .any(|required| Some(*required) != resumable_response)
        {
            operation.complete(blocked(ApprovalRecoveryBlock::RequestInvalid), Some(id));
            return Ok(waiter);
        }
        if let Some(previous) = &existing {
            let state = previous
                .state
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            if state.probe.is_some()
                || state.arm.is_some()
                || state.ticket.is_some()
                || state.reservation.is_some()
                || state.frame.is_some()
            {
                operation.complete(
                    blocked(ApprovalRecoveryBlock::AttemptAlreadyOwned),
                    Some(id),
                );
                return Ok(waiter);
            }
        }
        let task_core = core.clone();
        let task_source = source.clone();
        let task_permission = permission;
        let task_slot = slot;
        let op = operation.clone();
        let failed = operation.clone();
        let failed_core = core;
        let failed_source = source.clone();
        let job = if let Ok(job) = source.jobs.spawn_guarded(
            async move {
                if let Some(previous) = decision.job.get()
                    && (!previous.join(until).await || previous.failed())
                {
                    op.complete(
                        blocked(ApprovalRecoveryBlock::OriginalSqlNotJoined),
                        Some(id),
                    );
                    return;
                }
                op.advance(ApprovalReconcileProgress::ReplyPending);
                let result = if let Some(existing) = existing {
                    task_core
                        .resume_original_unreserved_reply(
                            &task_source,
                            &task_permission,
                            &task_slot,
                            &decision,
                            &existing,
                            until,
                        )
                        .await
                        .map(|()| existing.id)
                } else {
                    task_core
                        .deliver_approval_with_deadline(
                            &task_source,
                            &task_permission,
                            &task_slot,
                            &decision,
                            until,
                        )
                        .await
                };
                op.complete(
                    match result {
                        Ok(response) => ApprovalReconcileOutcome::ReplyStarted { response },
                        Err(error) => blocked(sql_error(error)),
                    },
                    Some(id),
                );
            },
            move || {
                failed_core.fail_source(&failed_source, DispatchError::RequiredStateUnavailable);
                failed.complete(ApprovalReconcileOutcome::OriginalUnavailable, Some(id));
            },
        ) {
            job
        } else {
            operation.complete(ApprovalReconcileOutcome::OriginalUnavailable, Some(id));
            return Err(ApprovalRejection::Stopped);
        };
        let _ = operation.job.set(job);
        Ok(waiter)
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
    pub(super) async fn reconcile_original_approvals(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        scope: ApprovalRepairScope,
        mode: ApprovalRepairMode,
        until: Instant,
        operation: &ApprovalRecoveryOperation,
    ) -> ApprovalReconcileOutcome {
        let permissions = match source.permissions.lock() {
            Ok(all) => all
                .values()
                .filter(|permission| match scope {
                    ApprovalRepairScope::Request(id) => permission
                        .approval
                        .get()
                        .is_some_and(|slot| slot.request.metadata().approval == id),
                    ApprovalRepairScope::SourceInstance(_) => true,
                })
                .cloned()
                .collect::<Vec<_>>(),
            Err(_) => return blocked(ApprovalRecoveryBlock::MissingOriginalProof),
        };
        for permission in permissions {
            if source.pre.authority.closing.load(Ordering::SeqCst)
                || source.pre.authority.closed.load(Ordering::SeqCst)
            {
                return blocked(ApprovalRecoveryBlock::SourceClosing);
            }
            let slot = match permission.approval.get() {
                Some(slot) => slot.clone(),
                None if mode == ApprovalRepairMode::RepairMissingFacts => {
                    match self.create_approval_inventory(source, &permission) {
                        Ok(slot) => slot,
                        Err(_) => return blocked(ApprovalRecoveryBlock::MissingOriginalProof),
                    }
                }
                None => return blocked(ApprovalRecoveryBlock::MissingOriginalProof),
            };
            if let Err(reason) = self.join_original_approval_operations(&slot, until).await {
                return blocked(reason);
            }
            operation.advance(ApprovalReconcileProgress::Repairing);
            if let Err(error) = self.reconcile_source_observations(source).await {
                return blocked(match error {
                    DispatchError::CommitUnconfirmed => ApprovalRecoveryBlock::CommitUnconfirmed,
                    DispatchError::SqlCloseUnconfirmed { .. } => {
                        ApprovalRecoveryBlock::CleanupUnconfirmed
                    }
                    DispatchError::Stopped | DispatchError::NodeStopping => {
                        ApprovalRecoveryBlock::SourceClosing
                    }
                    _ => ApprovalRecoveryBlock::NecessaryStorageUnavailable,
                });
            }
            if mode == ApprovalRepairMode::RepairMissingFacts {
                if let Err(error) = self
                    .repair_pending_approval_association(source, &slot)
                    .await
                {
                    return blocked(sql_error(error));
                }
                if slot
                    .association
                    .lock()
                    .map_or(true, |association| association.is_none())
                    && let Err(error) = self
                        .ensure_approval_association(source, &permission, &slot)
                        .await
                    && !matches!(
                        error,
                        NecessaryWriteError::Rejected(ApprovalRejection::UnassociatedTarget)
                    )
                {
                    return blocked(sql_error(error));
                }
            }
            if mode == ApprovalRepairMode::RepairMissingFacts
                && let Err(error) = self
                    .repair_original_approval_facts(source, &slot, until)
                    .await
            {
                return blocked(sql_error(error));
            }
            if let Err(reason) = self
                .recheck_original_approval_facts(source, &slot, until)
                .await
            {
                return blocked(reason);
            }
        }
        if mode == ApprovalRepairMode::RepairMissingFacts {
            ApprovalReconcileOutcome::FactsRepaired
        } else {
            ApprovalReconcileOutcome::FactsConfirmed
        }
    }
    async fn join_original_approval_operations(
        &self,
        slot: &ApprovalSlot<P>,
        until: Instant,
    ) -> Result<(), ApprovalRecoveryBlock> {
        let mut jobs = Vec::new();
        if let Some(job) = slot.request_job.get() {
            jobs.push(job.clone());
        }
        {
            let state = slot
                .state
                .lock()
                .map_err(|_| ApprovalRecoveryBlock::MissingOriginalProof)?;
            for registration in state.registered.values() {
                if let Some(job) = registration.job.get() {
                    jobs.push(job.clone());
                }
            }
            for decision in state
                .admission
                .iter()
                .chain(state.rejected_admissions.iter())
            {
                if let Some(job) = decision.job.get() {
                    jobs.push(job.clone());
                }
            }
        }
        for job in jobs {
            if !job.join(until).await || job.failed() {
                return Err(ApprovalRecoveryBlock::OriginalSqlNotJoined);
            }
        }
        Ok(())
    }
    async fn original_cache_ready(
        &self,
        cache: &ApprovalWriteCache,
        until: Instant,
    ) -> Result<bool, ApprovalRecoveryBlock> {
        let owner = self.repository.approval_owner_scope();
        if !cache
            .confirm_all_cleanup(&owner, until)
            .await
            .map_err(sql_error)?
        {
            return Err(ApprovalRecoveryBlock::CleanupUnconfirmed);
        }
        cache
            .confirm_actual_close(&owner, until)
            .await
            .map_err(sql_error)
    }
    async fn repair_original_approval_facts(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        slot: &Arc<ApprovalSlot<P>>,
        until: Instant,
    ) -> Result<(), NecessaryWriteError> {
        let d = RegistryDeadline::new(until);
        let association = slot
            .association
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .clone();
        let observed = self
            .repository
            .observe_approval(&slot.request, association.as_deref(), &d)
            .await?;
        {
            let mut state = slot
                .state
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            state.request_save = *observed.request.confirmation();
            state.request_error = None;
        }
        self.confirm_approval_fact(source, slot.request.metadata().approval.entity())
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        let (registrations, admission, responses, controls, invalidations) = {
            let state = slot
                .state
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            (
                state.registered.values().cloned().collect::<Vec<_>>(),
                state.admission.clone(),
                state.responses.clone(),
                state.controls.values().cloned().collect::<Vec<_>>(),
                state.invalidations.clone(),
            )
        };
        for registration in registrations {
            if registration
                .probe
                .write_cache()
                .final_rejection()?
                .is_some()
            {
                continue;
            }
            if registration.probe.write_cache().close_state()?.1 {
                continue;
            }
            let stored = self
                .repository
                .register_approval(&registration.probe, &d)
                .await?;
            *registration
                .result
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)? = Some(Ok(stored));
        }
        if let Some(admission) = admission
            && admission.probe.write_cache().final_rejection()?.is_none()
        {
            let stored = self
                .repository
                .decide_approval(&admission.probe, &d)
                .await?;
            let saved = Arc::new(DecisionSavedCertificate::from_actual(
                admission.probe.clone(),
                stored.clone(),
            )?);
            if admission.saved.get().is_none() {
                let _ = admission.saved.set(saved);
            }
            *admission
                .result
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)? =
                Some(Ok(stored.clone()));
            slot.state
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
                .decision = Some(stored.clone());
            self.append_owned_approval_audit(
                source,
                slot,
                super::approval_audit::ApprovalAuditBasis::decided(
                    admission.probe.clone(),
                    stored,
                    admission.registration.probe.metadata().target,
                ),
            )
            .await;
        }
        for response in responses {
            let arm = response
                .state
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
                .arm
                .clone();
            if let Some(arm) = arm {
                let stored = self.repository.arm_approval_reply(&arm, &d).await?;
                response
                    .state
                    .lock()
                    .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
                    .arm_stored = Some(stored);
                self.record_approval_actual_terminal(source, &response, &arm)
                    .await?;
            }
        }
        for facts in controls {
            self.persist_approval_control_arm(source, &facts.original)
                .await
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            self.persist_approval_control_terminal(source, &facts.original)
                .await
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        }
        let failures = slot
            .state
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .failure_responses
            .clone();
        for failure in failures {
            self.persist_approval_failure_facts(source, &failure)
                .await
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        }
        for proof in invalidations {
            self.repository.invalidate_approval(&proof, &d).await?;
        }
        Ok(())
    }
    async fn recheck_original_approval_facts(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        slot: &Arc<ApprovalSlot<P>>,
        until: Instant,
    ) -> Result<(), ApprovalRecoveryBlock> {
        let actual = self
            .repository
            .inspect_approval(&slot.request, &RegistryDeadline::new(until))
            .await
            .map_err(sql_error)?;
        if actual.request.as_ref() != Some(slot.request.metadata()) {
            return Err(ApprovalRecoveryBlock::MissingOriginalProof);
        }
        if !self
            .original_cache_ready(slot.request.write_cache(), until)
            .await?
        {
            return Err(ApprovalRecoveryBlock::CommitUnconfirmed);
        }
        let association = slot
            .association
            .lock()
            .map_err(|_| ApprovalRecoveryBlock::MissingOriginalProof)?
            .clone();
        if let Some(proof) = association {
            if actual.association.as_ref() != Some(proof.metadata()) {
                return Err(ApprovalRecoveryBlock::MissingOriginalProof);
            }
            if !self
                .original_cache_ready(proof.write_cache(), until)
                .await?
            {
                return Err(ApprovalRecoveryBlock::CommitUnconfirmed);
            }
        }
        let (registrations, admission, responses, controls, invalidations) = {
            let state = slot
                .state
                .lock()
                .map_err(|_| ApprovalRecoveryBlock::MissingOriginalProof)?;
            (
                state.registered.values().cloned().collect::<Vec<_>>(),
                state.admission.clone(),
                state.responses.clone(),
                state.controls.values().cloned().collect::<Vec<_>>(),
                state.invalidations.clone(),
            )
        };
        for registration in registrations {
            let cache = registration.probe.write_cache();
            if cache.final_rejection().map_err(sql_error)?.is_some() {
                if !cache
                    .confirm_all_cleanup(&self.repository.approval_owner_scope(), until)
                    .await
                    .map_err(sql_error)?
                {
                    return Err(ApprovalRecoveryBlock::CleanupUnconfirmed);
                }
                continue;
            }
            let original = registration.probe.metadata();
            let row = actual
                .commands
                .iter()
                .find(|row| row.registration == original.registration)
                .ok_or(ApprovalRecoveryBlock::MissingOriginalProof)?;
            let mut immutable = row.clone();
            immutable.state = original.state;
            immutable.operation = original.operation;
            immutable.admission_auth_generation = original.admission_auth_generation;
            immutable.admitted_at_ms = original.admitted_at_ms;
            immutable.rejection = original.rejection;
            if &immutable != original {
                return Err(ApprovalRecoveryBlock::MissingOriginalProof);
            }
            if !self.original_cache_ready(cache, until).await? {
                return Err(ApprovalRecoveryBlock::CommitUnconfirmed);
            }
            *registration
                .result
                .lock()
                .map_err(|_| ApprovalRecoveryBlock::MissingOriginalProof)? =
                Some(Ok(StoredApproval::from_exact_owner_result(
                    original.clone(),
                    cache.confirmation().map_err(sql_error)?,
                    *registration.probe.fingerprint(),
                )));
            self.confirm_approval_fact(source, original.registration.entity())
                .map_err(|_| ApprovalRecoveryBlock::NecessaryStorageUnavailable)?;
        }
        if let Some(admission) = admission
            && admission
                .probe
                .write_cache()
                .final_rejection()
                .map_err(sql_error)?
                .is_none()
        {
            let proof = &admission.probe;
            if actual.decision.as_ref() != Some(proof.metadata()) {
                return Err(ApprovalRecoveryBlock::MissingOriginalProof);
            }
            if !self
                .original_cache_ready(proof.write_cache(), until)
                .await?
            {
                return Err(ApprovalRecoveryBlock::CommitUnconfirmed);
            }
            let stored = StoredApproval::from_exact_owner_result(
                proof.metadata().clone(),
                proof.write_cache().confirmation().map_err(sql_error)?,
                *proof.fingerprint(),
            );
            let saved = Arc::new(
                DecisionSavedCertificate::from_actual(proof.clone(), stored.clone())
                    .map_err(|_| ApprovalRecoveryBlock::CleanupUnconfirmed)?,
            );
            if admission.saved.get().is_none() {
                let _ = admission.saved.set(saved);
            }
            *admission
                .result
                .lock()
                .map_err(|_| ApprovalRecoveryBlock::MissingOriginalProof)? =
                Some(Ok(stored.clone()));
            slot.state
                .lock()
                .map_err(|_| ApprovalRecoveryBlock::MissingOriginalProof)?
                .decision = Some(stored.clone());
            self.append_owned_approval_audit(
                source,
                slot,
                super::approval_audit::ApprovalAuditBasis::decided(
                    admission.probe.clone(),
                    stored,
                    admission.registration.probe.metadata().target,
                ),
            )
            .await;
            self.confirm_approval_fact(source, proof.metadata().operation)
                .map_err(|_| ApprovalRecoveryBlock::NecessaryStorageUnavailable)?;
        }
        for response in responses {
            let (arm, terminal) = {
                let state = response
                    .state
                    .lock()
                    .map_err(|_| ApprovalRecoveryBlock::MissingOriginalProof)?;
                (state.arm.clone(), state.terminal.clone())
            };
            if let Some(arm) = arm {
                if !actual.attempts.iter().any(|row| row == arm.metadata())
                    || !self.original_cache_ready(arm.write_cache(), until).await?
                {
                    return Err(ApprovalRecoveryBlock::NotReliablyRecorded);
                }
                response
                    .state
                    .lock()
                    .map_err(|_| ApprovalRecoveryBlock::MissingOriginalProof)?
                    .arm_stored = Some(StoredApproval::from_exact_owner_result(
                    arm.metadata().clone(),
                    arm.write_cache().confirmation().map_err(sql_error)?,
                    *arm.fingerprint(),
                ));
            }
            if let Some(terminal) = terminal {
                if !actual
                    .terminals
                    .iter()
                    .any(|row| row == terminal.metadata())
                    || !self
                        .original_cache_ready(terminal.write_cache(), until)
                        .await?
                {
                    return Err(ApprovalRecoveryBlock::NotReliablyRecorded);
                }
                response
                    .state
                    .lock()
                    .map_err(|_| ApprovalRecoveryBlock::MissingOriginalProof)?
                    .terminal_stored = Some(StoredApproval::from_exact_owner_result(
                    terminal.metadata().clone(),
                    terminal.write_cache().confirmation().map_err(sql_error)?,
                    *terminal.fingerprint(),
                ));
                self.confirm_approval_fact(source, response.id.entity())
                    .map_err(|_| ApprovalRecoveryBlock::NecessaryStorageUnavailable)?;
            }
        }
        for facts in controls {
            if let Some(arm) = &facts.arm
                && (!actual.attempts.iter().any(|row| row == arm.metadata())
                    || !self.original_cache_ready(arm.write_cache(), until).await?)
            {
                return Err(ApprovalRecoveryBlock::NotReliablyRecorded);
            }
            let terminal = facts
                .state
                .lock()
                .map_err(|_| ApprovalRecoveryBlock::MissingOriginalProof)?
                .terminal
                .clone();
            if let Some(terminal) = terminal
                && (!actual
                    .terminals
                    .iter()
                    .any(|row| row == terminal.metadata())
                    || !self
                        .original_cache_ready(terminal.write_cache(), until)
                        .await?)
            {
                return Err(ApprovalRecoveryBlock::NotReliablyRecorded);
            }
        }
        for proof in invalidations {
            if !actual
                .invalidations
                .iter()
                .any(|row| row == proof.metadata())
                || !self
                    .original_cache_ready(proof.write_cache(), until)
                    .await?
            {
                return Err(ApprovalRecoveryBlock::NotReliablyRecorded);
            }
        }
        let mut state = slot
            .state
            .lock()
            .map_err(|_| ApprovalRecoveryBlock::MissingOriginalProof)?;
        state.request_save = slot
            .request
            .write_cache()
            .confirmation()
            .map_err(sql_error)?;
        state.request_error = None;
        state.allow_disabled = slot.malformed_advertisement || !state.failure_responses.is_empty();
        drop(state);
        self.confirm_approval_fact(source, slot.request.metadata().approval.entity())
            .map_err(|_| ApprovalRecoveryBlock::NecessaryStorageUnavailable)?;
        slot.publish_change();
        Ok(())
    }
    pub(super) async fn resume_original_unreserved_reply(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        permission: &Arc<SourcePermissionSlot<P>>,
        slot: &Arc<ApprovalSlot<P>>,
        decision: &Arc<super::approval_service::ApprovalDecisionOperation<P>>,
        existing: &Arc<super::approval_response::ApprovalResponseOperation<P>>,
        until: Instant,
    ) -> Result<(), NecessaryWriteError> {
        *existing
            .write_until
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)? = until;
        {
            let state = existing
                .state
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            if state.probe.is_some()
                || state.arm.is_some()
                || state.ticket.is_some()
                || state.reservation.is_some()
                || state.frame.is_some()
            {
                return Err(ApprovalRejection::InvalidRequest.into());
            }
        }
        self.drive_approval_reply(source, permission, slot, decision, existing)
            .await
    }
}
