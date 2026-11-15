//! Native-owned approval inventory. Raw requests remain in their original Source slot.
#![allow(
    clippy::significant_drop_tightening,
    clippy::too_many_lines,
    clippy::unused_self,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    ApprovalInvalidationProof, Arc, AtomicBool, ConfigurationPort, DispatchError, EntityId,
    NativeApprovalPort, NativeApprovalRepository, NativeControlRepository, NativeCoordinator,
    NativeDispatchRepository, NativeResourceRepository, NativeSessionKey, Ordering,
    OriginalRequestProbe, OwnedNativeSource, RpcScalar, SourceLaunchPort,
    VerifiedApprovalAssociation,
    approval_records::{ApprovalOriginal, ApprovalWriteCache},
    environment::SourcePermissionSlot,
    runtime::{LifecycleResponseValue, SourceSlot},
    tasks::TaskSlot,
};
use crate::domain::{
    approval_owner::{
        ApprovalContextDisplay, ApprovalId, ApprovalRegistrationId, ApprovalRejection,
        ApprovalSourceDisplay, ApprovalStatus, AssociationDisplay, ChoiceId, CloseReportState,
        CommitState, DisplayLimitation, KnownOptionKind, LeaseCloseState, NecessaryWriteError,
        SqlConfirmation, SqlTaskJoinState, choice_fingerprint,
    },
    approval_records::{
        ApprovalAssociationRecord, ApprovalRequestRecord, StoredApprovalDecision,
        StoredAssociationKind, StoredRpcKind,
    },
};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, OnceLock},
};

pub struct ApprovalChoiceDisplay {
    pub(super) choice: ChoiceId,
    pub(super) ordinal: usize,
    pub(super) original_option_id: Arc<str>,
    pub(super) original_kind: Arc<str>,
    pub(super) known_kind: Option<KnownOptionKind>,
    pub(super) selectable: bool,
}
impl ApprovalChoiceDisplay {
    #[must_use]
    pub const fn id(&self) -> ChoiceId {
        self.choice
    }
    #[must_use]
    pub const fn ordinal(&self) -> usize {
        self.ordinal
    }
    #[must_use]
    pub const fn known_kind(&self) -> Option<KnownOptionKind> {
        self.known_kind
    }
    #[must_use]
    pub fn kind_text(&self) -> &str {
        &self.original_kind
    }
    #[must_use]
    pub fn native_option_id(&self) -> &str {
        &self.original_option_id
    }
    #[must_use]
    pub const fn selectable(&self) -> bool {
        self.selectable
    }
}
pub enum ApprovalDisplayContent<R> {
    LiveOriginal(R),
    Unavailable,
}
pub struct ApprovalDisplay<R> {
    pub(super) approval: ApprovalId,
    pub(super) source: ApprovalSourceDisplay,
    pub(super) context: ApprovalContextDisplay,
    pub(super) association: AssociationDisplay,
    pub(super) choices: Vec<ApprovalChoiceDisplay>,
    pub(super) content: ApprovalDisplayContent<R>,
    pub(super) limitations: Vec<DisplayLimitation>,
    pub(super) status: ApprovalStatus,
    pub(super) observed_sequence: u64,
    pub(super) observed_receipt: u64,
}
impl<R> ApprovalDisplay<R> {
    pub const fn id(&self) -> ApprovalId {
        self.approval
    }
    pub const fn source(&self) -> &ApprovalSourceDisplay {
        &self.source
    }
    pub const fn association(&self) -> &AssociationDisplay {
        &self.association
    }
    pub const fn context(&self) -> &ApprovalContextDisplay {
        &self.context
    }
    pub fn choices(&self) -> &[ApprovalChoiceDisplay] {
        &self.choices
    }
    pub fn limitations(&self) -> &[DisplayLimitation] {
        &self.limitations
    }
    pub const fn content(&self) -> &ApprovalDisplayContent<R> {
        &self.content
    }
    pub const fn observed_sequence(&self) -> u64 {
        self.observed_sequence
    }
    pub const fn observed_receipt(&self) -> u64 {
        self.observed_receipt
    }
    pub const fn status(&self) -> &ApprovalStatus {
        &self.status
    }
}
pub(super) struct OriginalApprovalChoice<P: NativeApprovalPort> {
    pub(super) choice: P::PermissionChoice,
    pub(super) alias: ChoiceId,
    pub(super) ordinal: usize,
    pub(super) digest: [u8; 32],
}
pub(super) struct ApprovalSlot<P: NativeApprovalPort> {
    pub(super) request: Arc<OriginalRequestProbe<P>>,
    pub(super) association: Mutex<Option<Arc<VerifiedApprovalAssociation<P>>>>,
    pub(super) association_resolution:
        Mutex<Option<Arc<super::approval_association::ApprovalAssociationOperation<P>>>>,
    pub(super) choices: Vec<OriginalApprovalChoice<P>>,
    pub(super) malformed_advertisement: bool,
    pub(super) request_job: OnceLock<Arc<TaskSlot>>,
    pub(super) state: Mutex<ApprovalSlotState<P>>,
    pub(super) changed: tokio::sync::Notify,
}
impl<P: NativeApprovalPort> ApprovalSlot<P> {
    pub(super) fn publish_change(&self) {
        match self.state.lock() {
            Ok(mut state) => match state.change_sequence.checked_add(1) {
                Some(next) => state.change_sequence = next,
                None => state.allow_disabled = true,
            },
            Err(poisoned) => poisoned.into_inner().allow_disabled = true,
        }
        self.changed.notify_waiters();
    }
}
pub(super) struct ApprovalSlotState<P: NativeApprovalPort> {
    pub(super) change_sequence: u64,
    pub(super) request_save: SqlConfirmation,
    pub(super) request_error: Option<NecessaryWriteError>,
    pub(super) registered: HashMap<
        ApprovalRegistrationId,
        Arc<super::approval_service::ApprovalRegistrationOperation<P>>,
    >,
    pub(super) admission: Option<Arc<super::approval_service::ApprovalDecisionOperation<P>>>,
    pub(super) decision: Option<StoredApprovalDecision>,
    pub(super) rejected_admissions: Vec<Arc<super::approval_service::ApprovalDecisionOperation<P>>>,
    pub(super) responses: Vec<Arc<super::approval_response::ApprovalResponseOperation<P>>>,
    pub(super) controls: HashMap<EntityId, Arc<super::approval_control::ApprovalControlFacts<P>>>,
    pub(super) failure_responses: Vec<Arc<super::approval_failure::ApprovalFailureResponse<P>>>,
    pub(super) next_reply_serial: u64,
    pub(super) retry_nonce: Option<Arc<super::approval_recovery::ApprovalRetryNonce>>,
    pub(super) invalidations: Vec<Arc<ApprovalInvalidationProof<P>>>,
    pub(super) audit_warnings: Vec<crate::domain::approval_audit::ApprovalAuditWarning>,
    pub(super) allow_disabled: bool,
}
pub(super) const fn initial_confirmation() -> SqlConfirmation {
    SqlConfirmation {
        sql_operation: None,
        commit: CommitState::NotStarted,
        lease_close: LeaseCloseState::NotAcquired,
        close_report: CloseReportState::NotReported,
        task_join: SqlTaskJoinState::NotStarted,
    }
}
pub(super) fn digest_data(tag: &[u8], parts: &[&[u8]]) -> Result<[u8; 32], DispatchError> {
    let mut digest = Sha256::new();
    digest.update(tag);
    for part in parts {
        let length = u64::try_from(part.len()).map_err(|_| DispatchError::LengthUnrepresentable)?;
        digest.update(length.to_be_bytes());
        digest.update(part);
    }
    Ok(digest.finalize().into())
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
    pub(super) fn notify_approval_source_change(source: &SourceSlot<P, L>) {
        let slots = match source.permissions.lock() {
            Ok(all) => all
                .values()
                .filter_map(|permission| permission.approval.get().cloned())
                .collect::<Vec<_>>(),
            Err(_) => return,
        };
        for slot in slots {
            slot.publish_change();
        }
    }
    pub(super) fn require_approval_fact(
        &self,
        source: &SourceSlot<P, L>,
        id: EntityId,
    ) -> Result<(), DispatchError> {
        let _gate = source
            .pre
            .authority
            .gate
            .lock()
            .map_err(|_| DispatchError::RequiredStateUnavailable)?;
        if source
            .memory
            .lock()
            .map_err(|_| DispatchError::RequiredStateUnavailable)?
            .requirements
            .insert(id, DispatchError::RequiredStateUnavailable)
            .is_none()
        {
            source.pre.authority.required.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    }
    pub(super) fn confirm_approval_fact(
        &self,
        source: &SourceSlot<P, L>,
        id: EntityId,
    ) -> Result<(), DispatchError> {
        {
            let _gate = source
                .pre
                .authority
                .gate
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?;
            if source
                .memory
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?
                .requirements
                .remove(&id)
                .is_some()
            {
                source.pre.authority.required.fetch_sub(1, Ordering::SeqCst);
            }
        }
        Self::notify_approval_source_change(source);
        source.changed.notify_waiters();
        if let Ok(resources) = source.all_resources.lock() {
            for resource in resources.iter() {
                resource.target.wake_writer();
                resource.target.changed.notify_waiters();
                self.environment.wake(resource.pre.native());
            }
        }
        Ok(())
    }
    pub(super) fn create_approval_inventory(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        slot: &Arc<SourcePermissionSlot<P>>,
    ) -> Result<Arc<ApprovalSlot<P>>, DispatchError> {
        if let Some(existing) = slot.approval.get() {
            return Ok(existing.clone());
        }
        {
            let mut state = slot
                .state
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?;
            if state.obligation.is_none() {
                state.obligation = Some(Self::new_id()?);
            }
        }
        let source_metadata = source.pre.metadata();
        let (binding, protocol) = {
            let memory = source.memory.lock().map_err(|_| DispatchError::Stopped)?;
            let binding = memory.binding.ok_or(DispatchError::InvalidScope)?;
            let protocol = if let Some(initialized) = &memory.initialized {
                initialized.metadata.wire
            } else {
                let initialize = source
                    .initialize
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .clone()
                    .ok_or(DispatchError::ProtocolRejected)?;
                let response = initialize
                    .response
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?;
                match response.as_ref().map(|r| &r.value) {
                    Some(LifecycleResponseValue::Initialized(wire)) => *wire,
                    _ => return Err(DispatchError::ProtocolRejected),
                }
            };
            (binding, protocol)
        };
        let approval = ApprovalId::from_entity(Self::new_id()?);
        let obligation = slot
            .snapshot()?
            .obligation
            .ok_or(DispatchError::RequiredStateUnavailable)?;
        let request_digest = self.protocol.permission_request_digest(&slot.request);
        let (rpc_kind, rpc_digest) = match &slot.rpc {
            RpcScalar::Integer(value) => (
                StoredRpcKind::Integer,
                digest_data(b"acpr-approval-rpc-integer-v1\0", &[&value.to_be_bytes()])?,
            ),
            RpcScalar::String(value) => (
                StoredRpcKind::String,
                digest_data(b"acpr-approval-rpc-string-v1\0", &[value.as_bytes()])?,
            ),
        };
        let metadata = ApprovalRequestRecord {
            approval,
            obligation,
            source_id: source_metadata.source_id,
            runtime: self.runtime.0,
            instance: source_metadata.instance_id,
            connection_epoch: source_metadata.connection_epoch,
            scope_binding: binding,
            rpc_kind,
            rpc_digest,
            native_session_digest: digest_data(
                b"acpr-approval-session-v1\0",
                &[slot.session.as_bytes()],
            )?,
            request_digest,
            protocol,
            observed_sequence: slot.sequence,
            observed_receipt: slot.receipt,
            observed_at_ms: self.clock.unix_millis(),
            verified_dispatch: None,
        };
        let fingerprint = digest_data(
            b"acpr-approval-request-v1\0",
            &[
                approval.encode().as_bytes(),
                &request_digest,
                &slot.sequence.to_be_bytes(),
                &slot.receipt.to_be_bytes(),
                binding.encode().as_bytes(),
            ],
        )?;
        let original = Arc::new(ApprovalOriginal {
            slot: Arc::downgrade(slot),
            source: source.pre.clone(),
            owner: self.repository.approval_owner_scope(),
        });
        let request = Arc::new(OriginalRequestProbe {
            original: original.clone(),
            metadata,
            fingerprint,
            write: ApprovalWriteCache::default(),
            resolution: ApprovalWriteCache::default(),
            resolution_untracked: AtomicBool::new(false),
        });
        let mut choices = Vec::new();
        let mut ids = HashSet::new();
        let mut ambiguous = false;
        for (ordinal, choice) in self
            .protocol
            .permission_choices(&slot.request)?
            .into_iter()
            .enumerate()
        {
            let id = self.protocol.permission_choice_id(&choice);
            ambiguous |= !ids.insert(id.to_owned());
            let digest = choice_fingerprint(request_digest, ordinal, id)
                .map_err(|_| DispatchError::LengthUnrepresentable)?;
            choices.push(OriginalApprovalChoice {
                choice,
                alias: ChoiceId::from_entity(Self::new_id()?),
                ordinal,
                digest,
            });
        }
        let association = if let Some(pre) = &slot.association {
            let fingerprint = digest_data(
                b"acpr-approval-known-pre-v1\0",
                &[
                    &fingerprint,
                    pre.metadata().target.encode().as_bytes(),
                    pre.metadata().load_epoch.encode().as_bytes(),
                    pre.metadata().owner.encode().as_bytes(),
                ],
            )?;
            Some(Arc::new(VerifiedApprovalAssociation {
                original,
                metadata: ApprovalAssociationRecord {
                    approval,
                    target: pre.metadata().target,
                    kind: StoredAssociationKind::KnownPre,
                    load_epoch: Some(pre.metadata().load_epoch),
                    owner: Some(pre.metadata().owner),
                    fingerprint,
                    associated_at_ms: self.clock.unix_millis(),
                },
                fingerprint,
                write: ApprovalWriteCache::default(),
                mapping_rejected_before_commit: AtomicBool::new(false),
            }))
        } else {
            let native =
                NativeSessionKey::from_config(Some(source_metadata.source_id), &slot.session)
                    .map_err(|_| DispatchError::InvalidTarget)?;
            if let Some(target) = self.queue.target_for_native(&native)? {
                let fingerprint = digest_data(
                    b"acpr-approval-registered-session-v1\0",
                    &[&fingerprint, target.encode().as_bytes()],
                )?;
                Some(Arc::new(VerifiedApprovalAssociation {
                    original,
                    metadata: ApprovalAssociationRecord {
                        approval,
                        target,
                        kind: StoredAssociationKind::RegisteredSession,
                        load_epoch: None,
                        owner: None,
                        fingerprint,
                        associated_at_ms: self.clock.unix_millis(),
                    },
                    fingerprint,
                    write: ApprovalWriteCache::default(),
                    mapping_rejected_before_commit: AtomicBool::new(false),
                }))
            } else {
                None
            }
        };
        let extension = Arc::new(ApprovalSlot {
            request,
            association: Mutex::new(association),
            association_resolution: Mutex::new(None),
            choices,
            malformed_advertisement: ambiguous,
            request_job: OnceLock::new(),
            state: Mutex::new(ApprovalSlotState {
                change_sequence: 0,
                request_save: initial_confirmation(),
                request_error: None,
                registered: HashMap::new(),
                admission: None,
                decision: None,
                rejected_admissions: Vec::new(),
                responses: Vec::new(),
                controls: HashMap::new(),
                failure_responses: Vec::new(),
                next_reply_serial: 0,
                retry_nonce: None,
                invalidations: Vec::new(),
                audit_warnings: Vec::new(),
                allow_disabled: ambiguous,
            }),
            changed: tokio::sync::Notify::new(),
        });
        slot.approval
            .set(extension.clone())
            .map_err(|_| DispatchError::Conflict)?;
        self.require_approval_fact(source, approval.entity())?;
        Ok(extension)
    }
    pub(super) fn retain_approval(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        slot: &Arc<SourcePermissionSlot<P>>,
    ) -> Result<(), DispatchError> {
        let extension = self.create_approval_inventory(source, slot)?;
        let core = self.clone();
        let source_task = source.clone();
        let retained = extension.clone();
        let failed_core = self.clone();
        let failed_source = source.clone();
        let task = source.tasks.spawn_guarded(
            async move {
                core.persist_approval_observation(&source_task, &retained)
                    .await;
            },
            move || {
                failed_core.fail_source(&failed_source, DispatchError::RequiredStateUnavailable);
            },
        )?;
        extension
            .request_job
            .set(task)
            .map_err(|_| DispatchError::Conflict)?;
        Ok(())
    }
    pub(super) async fn persist_approval_observation(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        slot: &Arc<ApprovalSlot<P>>,
    ) {
        let association = match slot.association.lock() {
            Ok(value) => value.clone(),
            Err(_) => return,
        };
        let result = match self.deadline() {
            Ok(deadline) => {
                self.repository
                    .observe_approval(&slot.request, association.as_deref(), &deadline)
                    .await
            }
            Err(_) => Err(ApprovalRejection::NecessaryStorageUnavailable.into()),
        };
        let reliable = result.is_ok();
        let failure = result.as_ref().err().copied();
        if let Ok(mut state) = slot.state.lock() {
            match result {
                Ok(stored) => {
                    state.request_save = *stored.request.confirmation();
                    state.request_error = None;
                }
                Err(error) => {
                    state.request_error = Some(error);
                    if let Ok(confirmation) = slot.request.write_cache().confirmation() {
                        state.request_save = confirmation;
                    }
                    state.allow_disabled = true;
                }
            }
        }
        if reliable {
            let _ = self.confirm_approval_fact(source, slot.request.metadata.approval.entity());
        }
        slot.publish_change();
        if let Some(error) = failure
            && let Some(permission) = slot.request.original.slot.upgrade()
        {
            let _ = self
                .cancel_actual_approval_save_failure(
                    source,
                    &permission,
                    slot,
                    super::approval_failure::ApprovalSaveFailure::Request {
                        original: slot.request.clone(),
                        error,
                    },
                )
                .await;
        }
        source.changed.notify_waiters();
        if let Ok(resources) = source.all_resources.lock() {
            for resource in resources.iter() {
                resource.target.wake_writer();
                resource.target.changed.notify_waiters();
                self.environment.wake(resource.pre.native());
            }
        }
    }
}
