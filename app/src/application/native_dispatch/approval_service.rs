//! Node-facing durable decision facade over the existing Source-owned tasks.
#![allow(
    clippy::match_wildcard_for_single_variants,
    clippy::option_if_let_else,
    clippy::significant_drop_tightening,
    clippy::too_many_lines,
    clippy::unused_self,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    ApprovalAdmissionProbe, ApprovalChoiceDisplay, ApprovalDisplay, ApprovalDisplayContent, Arc,
    ConfigurationPort, ControlPhase, DispatchError, EntityId, NativeApprovalPort,
    NativeApprovalRepository, NativeControlRepository, NativeCoordinator, NativeDispatchRepository,
    NativeResourceRepository, NativeRuntimeHandle, NativeRuntimeOptions, Ordering,
    OwnedNativeSource, PermissionPendingSnapshot, RegisteredApprovalProbe, SourceLaunchPort,
    TransportProgress,
    approval_inventory::{ApprovalSlot, digest_data, initial_confirmation},
    approval_records::ApprovalWriteCache,
    environment::SourcePermissionSlot,
    runtime::SourceSlot,
    tasks::TaskSlot,
};
use crate::{
    application::approval_owner::{ApprovalAuthority, CurrentApprovalActor},
    domain::{
        approval_owner::{
            ActualReplyKind, ApprovalContextDisplay, ApprovalId, ApprovalRegistrationId,
            ApprovalRejection, ApprovalSourceDisplay, ApprovalStatus, AssociationDisplay,
            AttemptSummary, CommitState, DecisionAcceptance, DecisionSummary, DeliverySummary,
            DisplayLimitation, InvalidationSummary, KnownOptionKind, LeaseCloseState,
            NecessaryWriteError, RegisterDecision, ReplyOrigin, RequestValidityDisplay,
            SourceCloseDisplay, SubmitDecision,
        },
        approval_records::{
            ApprovalCommandRecord, ApprovalDecisionRecord, StoredApprovalCommand,
            StoredApprovalCommandState, StoredApprovalDecision, StoredAssociationKind,
        },
    },
};
use std::sync::{Mutex, OnceLock};

pub struct NativeApprovalService<A, P, L, G, N>
where
    A: ApprovalAuthority,
    P: NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + NativeApprovalRepository,
{
    native: NativeRuntimeHandle<P, L, G, N>,
    authority: Arc<A>,
}
pub(super) struct ApprovalRegistrationOperation<P: NativeApprovalPort> {
    pub(super) probe: Arc<RegisteredApprovalProbe<P>>,
    pub(super) job: OnceLock<Arc<TaskSlot>>,
    pub(super) result: Mutex<Option<Result<StoredApprovalCommand, NecessaryWriteError>>>,
    pub(super) changed: tokio::sync::Notify,
}
pub(super) struct ApprovalDecisionOperation<P: NativeApprovalPort> {
    pub(super) probe: Arc<ApprovalAdmissionProbe<P>>,
    pub(super) registration: Arc<ApprovalRegistrationOperation<P>>,
    pub(super) choice: usize,
    pub(super) saved: OnceLock<Arc<super::approval_response::DecisionSavedCertificate<P>>>,
    pub(super) job: OnceLock<Arc<TaskSlot>>,
    pub(super) result: Mutex<Option<Result<StoredApprovalDecision, NecessaryWriteError>>>,
    pub(super) changed: tokio::sync::Notify,
}
type Located<P, L> = (
    Arc<SourceSlot<P, L>>,
    Arc<SourcePermissionSlot<P>>,
    Arc<ApprovalSlot<P>>,
);
const fn unavailable() -> ApprovalRejection {
    ApprovalRejection::NecessaryStorageUnavailable
}
const fn rejection(_: DispatchError) -> ApprovalRejection {
    ApprovalRejection::InvalidRequest
}
fn decision_summary(
    record: &StoredApprovalDecision,
    slot: &ApprovalSlot<impl NativeApprovalPort>,
) -> DecisionSummary {
    let data = record.record();
    DecisionSummary {
        approval: data.approval,
        registration: data.registration,
        operation: data.operation,
        actor: data.actor,
        admission_auth_generation: data.admission_auth_generation,
        selected_choice: slot
            .choices
            .iter()
            .find(|choice| choice.digest == data.option_digest)
            .map(|choice| choice.alias),
        option_digest: data.option_digest,
        known_kind: data.known_kind,
        accepted_at_ms: data.accepted_at_ms,
        fingerprint: data.fingerprint,
        save: *record.confirmation(),
    }
}
impl<A, P, L, G, N> NativeApprovalService<A, P, L, G, N>
where
    A: ApprovalAuthority,
    P: NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + NativeApprovalRepository,
{
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    pub(crate) fn approval_test_controller(
        &self,
    ) -> Arc<crate::session_storage::approval_owner::test_control::ApprovalTestController> {
        self.native.core.repository.approval_test_controller()
    }
    pub(crate) fn original_reconciler(
        &self,
    ) -> super::approval_recovery::OriginalApprovalReconciler<P, L, G, N> {
        super::approval_recovery::OriginalApprovalReconciler::from_native(&self.native)
    }
    pub(crate) fn create(
        native: NativeRuntimeHandle<P, L, G, N>,
        authority: Arc<A>,
    ) -> Result<Self, ApprovalRejection> {
        authority.verify_owner_scope(&native.core.repository.approval_owner_scope())?;
        Ok(Self { native, authority })
    }
    fn locate(&self, id: ApprovalId) -> Result<Located<P, L>, ApprovalRejection> {
        for source in self
            .native
            .core
            .sources
            .lock()
            .map_err(|_| unavailable())?
            .iter()
        {
            let retained = source.permissions.lock().map_err(|_| unavailable())?;
            for permission in retained.values() {
                if let Some(approval) = permission.approval.get()
                    && approval.request.metadata().approval == id
                {
                    return Ok((source.clone(), permission.clone(), approval.clone()));
                }
            }
        }
        Err(ApprovalRejection::InvalidRequest)
    }
    fn association_target(slot: &ApprovalSlot<P>) -> Result<EntityId, ApprovalRejection> {
        slot.association
            .lock()
            .map_err(|_| unavailable())?
            .as_ref()
            .map(|association| association.metadata().target)
            .ok_or(ApprovalRejection::UnassociatedTarget)
    }
    fn verify_actor(
        &self,
        actor: &CurrentApprovalActor,
        source: &SourceSlot<P, L>,
        permission: &SourcePermissionSlot<P>,
        slot: &ApprovalSlot<P>,
    ) -> Result<(), ApprovalRejection> {
        if !actor
            .owner
            .matches(&self.native.core.repository.approval_owner_scope())
            || actor.target != Self::association_target(slot)?
            || actor.native.source().0 != source.pre.metadata().source_id
            || actor.native.native_id() != &*permission.session
        {
            return Err(ApprovalRejection::InvalidActor);
        }
        Ok(())
    }
    fn verify_reader(
        &self,
        reader: &crate::application::approval_owner::CurrentApprovalReader,
    ) -> Result<(), ApprovalRejection> {
        if reader.context.auth_generation < 0 {
            return Err(ApprovalRejection::InvalidActor);
        }
        self.authority.verify_owner_scope(&reader.owner)
    }
    fn require_original_waiting(
        &self,
        source: &SourceSlot<P, L>,
        permission: &SourcePermissionSlot<P>,
    ) -> Result<(), ApprovalRejection> {
        let connection = source
            .source
            .get()
            .ok_or(ApprovalRejection::InvalidRequest)?;
        match self
            .native
            .core
            .protocol
            .inspect_permission(connection, &permission.request)
        {
            Ok(PermissionPendingSnapshot::Waiting) => Ok(()),
            _ => Err(ApprovalRejection::InvalidRequest),
        }
    }
    pub(super) fn status(
        &self,
        source: &SourceSlot<P, L>,
        permission: &SourcePermissionSlot<P>,
        slot: &ApprovalSlot<P>,
    ) -> Result<ApprovalStatus, ApprovalRejection> {
        let (
            change_sequence,
            request_save,
            decision,
            decision_confirmation,
            mut necessary_facts_missing,
            responses,
            owned_reconciliation_running,
        ) = {
            let state = slot.state.lock().map_err(|_| unavailable())?;
            let decision_confirmation = if let Some(record) = &state.decision {
                *record.confirmation()
            } else if let Some(operation) = &state.admission {
                operation
                    .probe
                    .write_cache()
                    .confirmation()
                    .map_err(|_| unavailable())?
            } else {
                initial_confirmation()
            };
            let running = state.admission.as_ref().is_some_and(|operation| {
                operation
                    .result
                    .lock()
                    .map_or(true, |result| result.is_none())
            });
            (
                state.change_sequence,
                state.request_save,
                state
                    .decision
                    .as_ref()
                    .map(|record| decision_summary(record, slot)),
                decision_confirmation,
                state.request_error.is_some(),
                state.responses.clone(),
                running,
            )
        };
        let association = slot.association.lock().map_err(|_| unavailable())?;
        let association = match association.as_ref().map(|a| a.metadata()) {
            None => AssociationDisplay::Unassociated,
            Some(a) => match a.kind {
                StoredAssociationKind::KnownPre => AssociationDisplay::KnownPre {
                    target: a.target,
                    load_epoch: a.load_epoch.ok_or_else(unavailable)?,
                    owner: a.owner.ok_or_else(unavailable)?,
                },
                StoredAssociationKind::RegisteredSession => {
                    AssociationDisplay::RegisteredSession { target: a.target }
                }
            },
        };
        let validity = if let Some(connection) = source.source.get() {
            match self
                .native
                .core
                .protocol
                .inspect_permission(connection, &permission.request)
            {
                Ok(PermissionPendingSnapshot::Waiting) => RequestValidityDisplay::CurrentPending,
                Ok(PermissionPendingSnapshot::Reserved) => RequestValidityDisplay::ResponseReserved,
                Ok(PermissionPendingSnapshot::SpentOrInvalid) => RequestValidityDisplay::Spent,
                Err(_) => RequestValidityDisplay::Unknown,
            }
        } else {
            RequestValidityDisplay::Unknown
        };
        let source_close = if matches!(source.metadata_close.result(), Some(Ok(()))) {
            SourceCloseDisplay::Confirmed
        } else if source
            .physical_closed
            .lock()
            .map_err(|_| unavailable())?
            .is_some()
        {
            SourceCloseDisplay::MetadataUnconfirmed
        } else if source.pre.authority.closed.load(Ordering::SeqCst) {
            SourceCloseDisplay::PhysicalUnconfirmed
        } else if source.pre.authority.closing.load(Ordering::SeqCst) {
            SourceCloseDisplay::Closing
        } else {
            SourceCloseDisplay::Open
        };
        let mut attempts = Vec::new();
        let mut delivery = DeliverySummary::NotAttempted;
        let mut owned_reconciliation_running = owned_reconciliation_running;
        for response in responses {
            let (arm, arm_stored, terminal, terminal_stored, probe, error) = {
                let state = response.state.lock().map_err(|_| unavailable())?;
                (
                    state.arm.clone(),
                    state.arm_stored.clone(),
                    state.terminal.clone(),
                    state.terminal_stored.clone(),
                    state.probe.clone(),
                    state.error,
                )
            };
            necessary_facts_missing |= error.is_some();
            owned_reconciliation_running |= terminal_stored.is_none() && error.is_none();
            delivery = if let Some(terminal) = &terminal {
                DeliverySummary::Terminal {
                    response: response.id,
                    actual: ActualReplyKind::Selected,
                    observation: terminal.metadata().observation,
                    record: terminal_stored
                        .as_ref()
                        .map_or_else(
                            || terminal.write_cache().confirmation(),
                            |stored| Ok(*stored.confirmation()),
                        )
                        .map_err(|_| unavailable())?,
                }
            } else {
                DeliverySummary::InProgress {
                    response: response.id,
                }
            };
            if let Some(arm) = arm {
                let metadata = arm.metadata();
                let terminal_save = terminal
                    .as_ref()
                    .map_or_else(
                        || Ok(initial_confirmation()),
                        |terminal| terminal.write_cache().confirmation(),
                    )
                    .map_err(|_| unavailable())?;
                attempts.push(AttemptSummary {
                    response: response.id,
                    approval: metadata.approval,
                    decision_operation: metadata.decision_operation,
                    serial: metadata.serial,
                    kind: metadata.kind,
                    origin: metadata.origin,
                    option_digest: metadata.option_digest,
                    frame_length: metadata.frame_length,
                    transport_progress: probe
                        .as_ref()
                        .map(|probe| self.native.core.protocol.probe_progress(probe)),
                    arm: arm_stored
                        .as_ref()
                        .map_or_else(
                            || arm.write_cache().confirmation(),
                            |stored| Ok(*stored.confirmation()),
                        )
                        .map_err(|_| unavailable())?,
                    terminal: terminal
                        .as_ref()
                        .map(|terminal| terminal.metadata().observation),
                    terminal_save,
                });
            }
        }
        let failures = slot
            .state
            .lock()
            .map_err(|_| unavailable())?
            .failure_responses
            .clone();
        for failure in failures {
            let progress = failure.actual_progress(&*self.native.core.protocol);
            let state = failure.state.lock().map_err(|_| unavailable())?;
            if let Some(arm) = &state.arm {
                let data = arm.metadata();
                let terminal = state.terminal.as_ref();
                attempts.push(AttemptSummary {
                    response: failure.id,
                    approval: data.approval,
                    decision_operation: None,
                    serial: data.serial,
                    kind: ActualReplyKind::Cancelled,
                    origin: ReplyOrigin::ManagementFailure,
                    option_digest: None,
                    frame_length: data.frame_length,
                    transport_progress: progress,
                    arm: state
                        .arm_stored
                        .as_ref()
                        .map_or_else(
                            || arm.write_cache().confirmation(),
                            |stored| Ok(*stored.confirmation()),
                        )
                        .map_err(|_| unavailable())?,
                    terminal: terminal.map(|p| p.metadata().observation),
                    terminal_save: terminal
                        .map_or_else(
                            || Ok(initial_confirmation()),
                            |p| p.write_cache().confirmation(),
                        )
                        .map_err(|_| unavailable())?,
                });
                necessary_facts_missing |=
                    state.arm_stored.is_none() || state.terminal_stored.is_none();
            }
        }
        let (controls, invalidation_proofs) = {
            let state = slot.state.lock().map_err(|_| unavailable())?;
            (
                state.controls.values().cloned().collect::<Vec<_>>(),
                state.invalidations.clone(),
            )
        };
        for facts in controls {
            let native = facts.original.approval_terminal().map_err(rejection)?;
            let (stored_arm, terminal, stored_terminal) = {
                let state = facts.state.lock().map_err(|_| unavailable())?;
                (
                    state.arm_stored.clone(),
                    state.terminal.clone(),
                    state.terminal_stored.clone(),
                )
            };
            if let Some(arm) = &facts.arm {
                let data = arm.metadata();
                let observation = terminal.as_ref().map(|proof| proof.metadata().observation);
                let terminal_save = terminal
                    .as_ref()
                    .map_or_else(
                        || Ok(initial_confirmation()),
                        |proof| proof.write_cache().confirmation(),
                    )
                    .map_err(|_| unavailable())?;
                attempts.push(AttemptSummary {
                    response: facts.response,
                    approval: data.approval,
                    decision_operation: data.decision_operation,
                    serial: data.serial,
                    kind: data.kind,
                    origin: data.origin,
                    option_digest: data.option_digest,
                    frame_length: data.frame_length,
                    transport_progress: native.as_ref().and_then(|value| match value.phase {
                        ControlPhase::Terminal(fact) => Some(TransportProgress::Finished(fact)),
                        _ => None,
                    }),
                    arm: stored_arm
                        .as_ref()
                        .map_or_else(
                            || arm.write_cache().confirmation(),
                            |stored| Ok(*stored.confirmation()),
                        )
                        .map_err(|_| unavailable())?,
                    terminal: observation,
                    terminal_save,
                });
                necessary_facts_missing |=
                    stored_arm.is_none() || terminal.is_some() && stored_terminal.is_none();
            }
        }
        attempts.sort_by_key(|attempt| attempt.serial);
        if let Some(latest) = attempts.last() {
            delivery = if let Some(observation) = latest.terminal {
                DeliverySummary::Terminal {
                    response: latest.response,
                    actual: latest.kind,
                    observation,
                    record: latest.terminal_save,
                }
            } else {
                DeliverySummary::InProgress {
                    response: latest.response,
                }
            };
        }
        let invalidations = invalidation_proofs
            .iter()
            .map(|proof| {
                let m = proof.metadata();
                InvalidationSummary {
                    id: m.id,
                    reason: m.reason,
                    fingerprint: m.fingerprint,
                    observed_sequence: m.observed_sequence,
                    observed_at_ms: m.observed_at_ms,
                    recorded_at_ms: m.recorded_at_ms,
                }
            })
            .collect();
        necessary_facts_missing |= source.pre.authority.required.load(Ordering::SeqCst) != 0;
        for original in source
            .approval_recoveries
            .lock()
            .map_err(|_| unavailable())?
            .iter()
        {
            owned_reconciliation_running |= original.is_running()?;
        }
        Ok(ApprovalStatus {
            approval: slot.request.metadata().approval,
            change_sequence,
            validity,
            request_save,
            association,
            decision,
            decision_confirmation,
            delivery,
            attempts,
            invalidations,
            source_close,
            necessary_facts_missing,
            owned_reconciliation_running,
        })
    }
    fn display(
        &self,
        source: &SourceSlot<P, L>,
        permission: &SourcePermissionSlot<P>,
        slot: &ApprovalSlot<P>,
    ) -> Result<ApprovalDisplay<P::RawEvent>, ApprovalRejection> {
        let request = slot.request.metadata();
        let status = self.status(source, permission, slot)?;
        let display = self
            .native
            .core
            .protocol
            .permission_display(&permission.request)
            .map_err(rejection)?;
        let mut limitations = Vec::new();
        if !display.supported_subject {
            limitations.push(DisplayLimitation::UnknownSubject);
        }
        if matches!(status.association, AssociationDisplay::Unassociated) {
            limitations.push(DisplayLimitation::TargetUnassociated);
        }
        if slot.malformed_advertisement {
            limitations.push(DisplayLimitation::MalformedAdvertisement);
        }
        let selectable = !slot.malformed_advertisement
            && status.validity == RequestValidityDisplay::CurrentPending
            && !matches!(status.association, AssociationDisplay::Unassociated)
            && status.request_save.commit == CommitState::Confirmed
            && status.decision.is_none()
            && status.source_close == SourceCloseDisplay::Open;
        let choices = slot
            .choices
            .iter()
            .map(|choice| {
                let kind = self
                    .native
                    .core
                    .protocol
                    .permission_choice_kind(&choice.choice);
                let known_kind = KnownOptionKind::classify(kind);
                if known_kind.is_none() {
                    limitations.push(DisplayLimitation::UnknownOptionKind);
                }
                ApprovalChoiceDisplay {
                    choice: choice.alias,
                    ordinal: choice.ordinal,
                    original_option_id: Arc::from(
                        self.native
                            .core
                            .protocol
                            .permission_choice_id(&choice.choice),
                    ),
                    original_kind: Arc::from(kind),
                    known_kind,
                    selectable,
                }
            })
            .collect();
        Ok(ApprovalDisplay {
            approval: request.approval,
            source: ApprovalSourceDisplay {
                source_id: request.source_id,
                runtime: request.runtime,
                instance: request.instance,
                connection_epoch: request.connection_epoch,
                scope_binding: request.scope_binding,
                protocol_version: request.protocol,
            },
            context: request
                .verified_dispatch
                .map_or(ApprovalContextDisplay::SessionRequest, |dispatch| {
                    ApprovalContextDisplay::VerifiedInput { dispatch }
                }),
            association: status.association.clone(),
            choices,
            content: ApprovalDisplayContent::LiveOriginal(display.raw_request),
            limitations,
            status,
            observed_sequence: request.observed_sequence,
            observed_receipt: request.observed_receipt,
        })
    }
    /// # Errors
    /// Returns an approval rejection when the actor cannot read this approval or its state is unavailable.
    pub async fn watch_status(
        &self,
        actor: &A::Actor,
        id: ApprovalId,
    ) -> Result<super::approval_watch::ApprovalStatusWatch<A, P, L, G, N>, ApprovalRejection> {
        let reader = self.authority.authorize_read(actor).await?;
        self.verify_reader(&reader)?;
        let (source, permission, slot) = self.locate(id)?;
        Ok(super::approval_watch::ApprovalStatusWatch::from_original(
            &self.native,
            self.authority.clone(),
            source,
            permission,
            slot,
            id,
        ))
    }
    /// # Errors
    /// Returns an approval rejection when the actor cannot read this approval or its audit state is unavailable.
    pub async fn audit_warnings(
        &self,
        actor: &A::Actor,
        id: ApprovalId,
    ) -> Result<Vec<crate::domain::approval_audit::ApprovalAuditWarning>, ApprovalRejection> {
        let reader = self.authority.authorize_read(actor).await?;
        self.verify_reader(&reader)?;
        let (_, _, slot) = self.locate(id)?;
        let warnings = slot
            .state
            .lock()
            .map_err(|_| unavailable())?
            .audit_warnings
            .clone();
        Ok(warnings)
    }
    /// # Errors
    /// Returns an approval rejection when the actor cannot read approvals or any observed state is unavailable.
    pub async fn list(
        &self,
        actor: &A::Actor,
    ) -> Result<Vec<ApprovalDisplay<P::RawEvent>>, ApprovalRejection> {
        let reader = self.authority.authorize_read(actor).await?;
        self.verify_reader(&reader)?;
        let sources = self
            .native
            .core
            .sources
            .lock()
            .map_err(|_| unavailable())?
            .clone();
        let mut result = Vec::new();
        for source in sources {
            let permissions = source
                .permissions
                .lock()
                .map_err(|_| unavailable())?
                .values()
                .cloned()
                .collect::<Vec<_>>();
            for permission in permissions {
                if let Some(slot) = permission.approval.get() {
                    result.push(self.display(&source, &permission, slot)?);
                }
            }
        }
        Ok(result)
    }
    /// # Errors
    /// Returns an approval rejection when the actor cannot read this approval or its state is unavailable.
    pub async fn read(
        &self,
        actor: &A::Actor,
        id: ApprovalId,
    ) -> Result<ApprovalDisplay<P::RawEvent>, ApprovalRejection> {
        let reader = self.authority.authorize_read(actor).await?;
        self.verify_reader(&reader)?;
        let (source, permission, slot) = self.locate(id)?;
        self.display(&source, &permission, &slot)
    }
    /// # Errors
    /// Returns an approval rejection when the actor cannot inspect this decision or its state is unavailable.
    pub async fn inspect_decision(
        &self,
        actor: &A::Actor,
        id: ApprovalId,
    ) -> Result<ApprovalStatus, ApprovalRejection> {
        let reader = self.authority.authorize_read(actor).await?;
        self.verify_reader(&reader)?;
        let (source, permission, slot) = self.locate(id)?;
        self.status(&source, &permission, &slot)
    }
    /// # Errors
    /// Returns a write error when authorization, persistence, or decision delivery fails.
    pub async fn decide(
        &self,
        actor: &A::Actor,
        input: SubmitDecision,
    ) -> Result<DecisionAcceptance, NecessaryWriteError> {
        let reader = self.authority.authorize_read(actor).await?;
        self.verify_reader(&reader)?;
        let sources = self
            .native
            .core
            .sources
            .lock()
            .map_err(|_| unavailable())?
            .clone();
        let mut located = None;
        for source in sources {
            let permissions = source
                .permissions
                .lock()
                .map_err(|_| unavailable())?
                .values()
                .cloned()
                .collect::<Vec<_>>();
            for permission in permissions {
                if let Some(slot) = permission.approval.get() {
                    let registered = slot
                        .state
                        .lock()
                        .map_err(|_| unavailable())?
                        .registered
                        .get(&input.registration())
                        .cloned();
                    if let Some(registered) = registered {
                        located =
                            Some((source.clone(), permission.clone(), slot.clone(), registered));
                        break;
                    }
                }
            }
            if located.is_some() {
                break;
            }
        }
        let (source, permission, slot, registration) =
            located.ok_or(ApprovalRejection::UnknownRegistration)?;
        loop {
            let changed = registration.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let result = registration
                .result
                .lock()
                .map_err(|_| unavailable())?
                .clone();
            if let Some(result) = result {
                result?;
                break;
            }
            changed.await;
        }
        if reader.context.client_id != registration.probe.metadata().actor {
            return Err(ApprovalRejection::InvalidActor.into());
        }
        let existing_decision = {
            let state = slot.state.lock().map_err(|_| unavailable())?;
            state.decision.as_ref().and_then(|stored| {
                if stored.record().actor == reader.context.client_id
                    && stored.record().registration == input.registration()
                {
                    Some(decision_summary(stored, &slot))
                } else {
                    None
                }
            })
        };
        if let Some(summary) = existing_decision {
            return Ok(DecisionAcceptance::Existing(summary));
        }
        let actor = self
            .authority
            .authorize_decision_with_epoch(
                actor,
                Self::association_target(&slot)?,
                input.expected_control_epoch(),
            )
            .await?;
        self.verify_actor(&actor, &source, &permission, &slot)?;
        self.authority.verify_approval_decide(
            &actor,
            input.registration(),
            input.expected_control_epoch(),
        )?;
        let choice = slot
            .choices
            .iter()
            .position(|choice| choice.digest == registration.probe.metadata().option_digest)
            .ok_or(ApprovalRejection::InvalidChoice)?;
        let original_waiting = self.require_original_waiting(&source, &permission).is_ok();
        let mut created = false;
        let operation = {
            let _action = permission.action.lock().map_err(|_| unavailable())?;
            let mut state = slot.state.lock().map_err(|_| unavailable())?;
            if let Some(stored) = &state.decision {
                return Ok(DecisionAcceptance::Existing(decision_summary(
                    stored, &slot,
                )));
            }
            if let Some(operation) = &state.admission {
                operation.clone()
            } else {
                self.authority.verify_approval_decide(
                    &actor,
                    input.registration(),
                    input.expected_control_epoch(),
                )?;
                if !original_waiting {
                    return Err(ApprovalRejection::InvalidRequest.into());
                }
                self.native
                    .core
                    .approval_current(&source, &permission, &state)?;
                if state.request_save.commit != CommitState::Confirmed
                    || state.request_save.lease_close != LeaseCloseState::Confirmed
                {
                    return Err(unavailable().into());
                }
                let id = NativeCoordinator::<P, L, G, N>::new_id().map_err(rejection)?;
                let at = self.native.core.clock.unix_millis();
                let authorization_fingerprint = digest_data(
                    b"acpr-approval-authorization-v1\0",
                    &[
                        actor.context.client_id.encode().as_bytes(),
                        &actor.context.auth_generation.to_be_bytes(),
                        actor.target.encode().as_bytes(),
                        registration.probe.fingerprint(),
                    ],
                )
                .map_err(rejection)?;
                let fingerprint = digest_data(
                    b"acpr-approval-decision-v1\0",
                    &[
                        id.encode().as_bytes(),
                        &authorization_fingerprint,
                        &registration.probe.metadata().option_digest,
                        &at.to_be_bytes(),
                    ],
                )
                .map_err(rejection)?;
                let metadata = ApprovalDecisionRecord {
                    approval: slot.request.metadata().approval,
                    registration: input.registration(),
                    operation: id,
                    actor: actor.context.client_id,
                    admission_auth_generation: actor.context.auth_generation,
                    option_digest: registration.probe.metadata().option_digest,
                    known_kind: KnownOptionKind::classify(
                        self.native
                            .core
                            .protocol
                            .permission_choice_kind(&slot.choices[choice].choice),
                    ),
                    authorization_fingerprint,
                    fingerprint,
                    accepted_at_ms: at,
                };
                let probe = Arc::new(ApprovalAdmissionProbe {
                    original: slot.request.original.clone(),
                    metadata,
                    fingerprint,
                    write: ApprovalWriteCache::default(),
                });
                let operation = Arc::new(ApprovalDecisionOperation {
                    probe,
                    registration: registration.clone(),
                    choice,
                    saved: OnceLock::new(),
                    job: OnceLock::new(),
                    result: Mutex::new(None),
                    changed: tokio::sync::Notify::new(),
                });
                state.admission = Some(operation.clone());
                created = true;
                operation
            }
        };
        if created {
            self.native
                .core
                .require_approval_fact(&source, operation.probe.metadata().operation)
                .map_err(rejection)?;
            let core = self.native.core.clone();
            let original_source = source.clone();
            let original_permission = permission.clone();
            let retained = operation.clone();
            let approval = slot.clone();
            let failed_core = core.clone();
            let failed_source = source.clone();
            let job = source
                .jobs
                .spawn_guarded(
                    async move {
                        let result = match core.deadline() {
                            Ok(deadline) => {
                                core.repository
                                    .decide_approval(&retained.probe, &deadline)
                                    .await
                            }
                            Err(error) => Err(rejection(error).into()),
                        };
                        let result = result.and_then(|stored| {
                            if !Arc::ptr_eq(
                                &retained.probe.original,
                                &retained.registration.probe.original,
                            ) || retained.probe.metadata().registration
                                != retained.registration.probe.metadata().registration
                                || retained.probe.metadata().actor
                                    != retained.registration.probe.metadata().actor
                                || retained.probe.metadata().option_digest
                                    != retained.registration.probe.metadata().option_digest
                            {
                                return Err(ApprovalRejection::Conflict.into());
                            }
                            let certificate = Arc::new(
                                super::approval_response::DecisionSavedCertificate::from_actual(
                                    retained.probe.clone(),
                                    stored.clone(),
                                )?,
                            );
                            retained
                                .saved
                                .set(certificate)
                                .map_err(|_| ApprovalRejection::Conflict)?;
                            Ok(stored)
                        });
                        if let Ok(mut state) = approval.state.lock() {
                            match &result {
                                Ok(stored) => state.decision = Some(stored.clone()),
                                Err(NecessaryWriteError::Rejected(
                                    ApprovalRejection::InvalidActor
                                    | ApprovalRejection::SessionNotControlled
                                    | ApprovalRejection::SessionTreeUnverified,
                                )) => {
                                    if state
                                        .admission
                                        .as_ref()
                                        .is_some_and(|current| Arc::ptr_eq(current, &retained))
                                    {
                                        state.admission = None;
                                        state.rejected_admissions.push(retained.clone());
                                    }
                                }
                                Err(_) => state.allow_disabled = true,
                            }
                        }
                        if result.is_ok()
                            || matches!(
                                result,
                                Err(NecessaryWriteError::Rejected(
                                    ApprovalRejection::InvalidActor
                                        | ApprovalRejection::SessionNotControlled
                                        | ApprovalRejection::SessionTreeUnverified
                                ))
                            )
                        {
                            let _ = core.confirm_approval_fact(
                                &original_source,
                                retained.probe.metadata().operation,
                            );
                        }
                        if let Ok(mut value) = retained.result.lock() {
                            *value = Some(result.clone());
                        }
                        retained.changed.notify_waiters();
                        approval.publish_change();
                        if let Ok(stored) = &result {
                            core.append_owned_approval_audit(
                                &original_source,
                                &approval,
                                super::approval_audit::ApprovalAuditBasis::decided(
                                    retained.probe.clone(),
                                    stored.clone(),
                                    retained.registration.probe.metadata().target,
                                ),
                            )
                            .await;
                        }
                        if result.is_ok() {
                            let _ = core
                                .deliver_approval_decision(
                                    &original_source,
                                    &original_permission,
                                    &approval,
                                    &retained,
                                )
                                .await;
                        } else if let Err(error) = result
                            && !matches!(
                                error,
                                NecessaryWriteError::Rejected(
                                    ApprovalRejection::SessionNotControlled
                                        | ApprovalRejection::SessionTreeUnverified
                                )
                            )
                        {
                            let _ = core
                                .cancel_actual_approval_save_failure(
                                    &original_source,
                                    &original_permission,
                                    &approval,
                                    super::approval_failure::ApprovalSaveFailure::Decision {
                                        original: retained.clone(),
                                        error,
                                    },
                                )
                                .await;
                        }
                    },
                    move || {
                        failed_core
                            .fail_source(&failed_source, DispatchError::RequiredStateUnavailable);
                    },
                )
                .map_err(rejection)?;
            operation
                .job
                .set(job)
                .map_err(|_| ApprovalRejection::Conflict)?;
        }
        loop {
            let changed = operation.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let result = operation.result.lock().map_err(|_| unavailable())?.clone();
            if let Some(result) = result {
                let stored = result?;
                let summary = decision_summary(&stored, &slot);
                return Ok(
                    if operation.probe.metadata().registration == input.registration() {
                        DecisionAcceptance::Accepted(summary)
                    } else {
                        DecisionAcceptance::Existing(summary)
                    },
                );
            }
            changed.await;
        }
    }
    /// # Errors
    /// Returns a write error when authorization, persistence, or decision registration fails.
    pub async fn register_decision(
        &self,
        actor: &A::Actor,
        input: RegisterDecision,
    ) -> Result<ApprovalRegistrationId, NecessaryWriteError> {
        self.authority.authorize_read(actor).await?;
        let (source, permission, slot) = self.locate(input.approval())?;
        if !slot
            .choices
            .iter()
            .any(|choice| choice.alias == input.choice())
        {
            return Err(ApprovalRejection::InvalidChoice.into());
        }
        self.native
            .core
            .ensure_approval_association(&source, &permission, &slot)
            .await?;
        let actor = self
            .authority
            .authorize_decision_with_epoch(
                actor,
                Self::association_target(&slot)?,
                input.expected_control_epoch(),
            )
            .await?;
        self.verify_actor(&actor, &source, &permission, &slot)?;
        let choice = slot
            .choices
            .iter()
            .find(|choice| choice.alias == input.choice())
            .ok_or(ApprovalRejection::InvalidChoice)?;
        let registration = ApprovalRegistrationId::from_entity(
            NativeCoordinator::<P, L, G, N>::new_id().map_err(rejection)?,
        );
        let at = self.native.core.clock.unix_millis();
        let fingerprint = digest_data(
            b"acpr-approval-intent-v1\0",
            &[
                registration.encode().as_bytes(),
                &choice.digest,
                actor.context.client_id.encode().as_bytes(),
                &actor.context.auth_generation.to_be_bytes(),
            ],
        )
        .map_err(rejection)?;
        let probe = Arc::new(RegisteredApprovalProbe {
            original: slot.request.original.clone(),
            metadata: ApprovalCommandRecord {
                registration,
                approval: input.approval(),
                target: actor.target,
                actor: actor.context.client_id,
                registration_auth_generation: actor.context.auth_generation,
                option_digest: choice.digest,
                intent_fingerprint: fingerprint,
                runtime: self.native.core.runtime.0,
                registered_at_ms: at,
                state: StoredApprovalCommandState::Registered,
                operation: None,
                admission_auth_generation: None,
                admitted_at_ms: None,
                rejection: None,
            },
            fingerprint,
            write: ApprovalWriteCache::default(),
        });
        self.require_original_waiting(&source, &permission)?;
        let operation = Arc::new(ApprovalRegistrationOperation {
            probe,
            job: OnceLock::new(),
            result: Mutex::new(None),
            changed: tokio::sync::Notify::new(),
        });
        {
            let _action = permission.action.lock().map_err(|_| unavailable())?;
            let mut state = slot.state.lock().map_err(|_| unavailable())?;
            self.native
                .core
                .approval_current(&source, &permission, &state)?;
            state.registered.insert(registration, operation.clone());
        }
        self.native
            .core
            .require_approval_fact(&source, registration.entity())
            .map_err(rejection)?;
        let core = self.native.core.clone();
        let retained = operation.clone();
        let approval = slot.clone();
        let original_source = source.clone();
        let failed_core = core.clone();
        let failed_source = source.clone();
        let job = source
            .jobs
            .spawn_guarded(
                async move {
                    let result = async {
                        let request_job = approval.request_job.get().ok_or_else(unavailable)?;
                        let until = NativeRuntimeOptions::at(core.options.metadata_wait)
                            .map_err(rejection)?;
                        if !request_job.join(until).await {
                            return Err(unavailable().into());
                        }
                        {
                            let state = approval.state.lock().map_err(|_| unavailable())?;
                            core.approval_current(&original_source, &permission, &state)?;
                            if state.request_save.commit != CommitState::Confirmed {
                                return Err(unavailable().into());
                            }
                        }
                        let deadline = core.deadline().map_err(rejection)?;
                        core.repository
                            .register_approval(&retained.probe, &deadline)
                            .await
                    }
                    .await;
                    if result.is_ok()
                        || matches!(
                            result,
                            Err(NecessaryWriteError::Rejected(
                                ApprovalRejection::InvalidActor
                            ))
                        )
                    {
                        let _ = core.confirm_approval_fact(
                            &original_source,
                            retained.probe.metadata().registration.entity(),
                        );
                    }
                    if let Ok(stored) = &result {
                        core.append_owned_approval_audit(
                            &original_source,
                            &approval,
                            super::approval_audit::ApprovalAuditBasis::registered(
                                retained.probe.clone(),
                                stored.clone(),
                            ),
                        )
                        .await;
                    }
                    if let Ok(mut value) = retained.result.lock() {
                        *value = Some(result.clone());
                    }
                    retained.changed.notify_waiters();
                    approval.publish_change();
                    if let Err(error) = result
                        && !matches!(
                            error,
                            NecessaryWriteError::Rejected(
                                ApprovalRejection::SessionNotControlled
                                    | ApprovalRejection::SessionTreeUnverified
                            )
                        )
                    {
                        let _ = core
                            .cancel_actual_approval_save_failure(
                                &original_source,
                                &permission,
                                &approval,
                                super::approval_failure::ApprovalSaveFailure::Registration {
                                    original: retained.clone(),
                                    error,
                                },
                            )
                            .await;
                    }
                },
                move || {
                    failed_core
                        .fail_source(&failed_source, DispatchError::RequiredStateUnavailable);
                },
            )
            .map_err(rejection)?;
        operation
            .job
            .set(job)
            .map_err(|_| ApprovalRejection::Conflict)?;
        loop {
            let changed = operation.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let result = operation.result.lock().map_err(|_| unavailable())?.clone();
            if let Some(result) = result {
                result?;
                self.authority
                    .record_approval_registration(&actor, registration);
                return Ok(registration);
            }
            changed.await;
        }
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
    pub(super) fn approval_current(
        &self,
        source: &SourceSlot<P, L>,
        permission: &SourcePermissionSlot<P>,
        state: &super::approval_inventory::ApprovalSlotState<P>,
    ) -> Result<(), ApprovalRejection> {
        if permission.association.as_ref().is_some_and(|resource| {
            resource.authority.closed.load(Ordering::SeqCst)
                || resource.authority.releasing.load(Ordering::SeqCst)
        }) {
            return Err(ApprovalRejection::InvalidRequest);
        }
        if source.pre.authority.closed.load(Ordering::SeqCst)
            || source.pre.authority.closing.load(Ordering::SeqCst)
            || state.allow_disabled
        {
            return Err(ApprovalRejection::InvalidRequest);
        }
        let snapshot = permission.snapshot().map_err(rejection)?;
        if snapshot.retired || snapshot.written || snapshot.claimed.is_some() {
            return Err(ApprovalRejection::InvalidRequest);
        }
        Ok(())
    }
}
