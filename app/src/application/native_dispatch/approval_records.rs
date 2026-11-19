//! Opaque facts retain the same original permission slot and registry owner.
#![allow(
    clippy::significant_drop_tightening,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::environment::SourcePermissionSlot;
use super::{
    ApprovalCleanupObservation, ApprovalSqlCleanup, Arc, BoxFuture, NativeApprovalPort,
    PreSourceProbe,
};
use crate::{
    application::request_registry::{RegistryDeadline, RegistryOwnerScope},
    domain::{
        approval_owner::{
            ApprovalId, ApprovalRejection, CloseReportState, CommitState, LeaseCloseState,
            NecessaryWriteError, SqlConfirmation, SqlTaskJoinState,
        },
        approval_records::{
            ApprovalAssociationRecord, ApprovalAttemptRecord, ApprovalCommandRecord,
            ApprovalDecisionRecord, ApprovalInvalidationRecord, ApprovalRequestRecord,
            ApprovalStoredInspection, ApprovalTerminalRecord, StoredApprovalAssociation,
            StoredApprovalAttempt, StoredApprovalCommand, StoredApprovalDecision,
            StoredApprovalInvalidation, StoredApprovalRequest, StoredApprovalTerminal,
        },
    },
};
use std::sync::{Mutex, Weak};
use tokio::time::Instant;

pub(super) struct ApprovalOriginal<P: NativeApprovalPort> {
    pub(super) slot: Weak<SourcePermissionSlot<P>>,
    pub(super) source: Arc<PreSourceProbe>,
    pub(super) owner: RegistryOwnerScope,
}
impl<P: NativeApprovalPort> ApprovalOriginal<P> {
    fn validate(&self, owner: &RegistryOwnerScope) -> Result<ApprovalId, NecessaryWriteError> {
        if !self.owner.matches(owner) {
            return Err(ApprovalRejection::InvalidRequest.into());
        }
        let slot = self
            .slot
            .upgrade()
            .ok_or(ApprovalRejection::InvalidRequest)?;
        let extension = slot
            .approval
            .get()
            .ok_or(ApprovalRejection::InvalidRequest)?;
        let request = extension.request.metadata();
        let source = self.source.metadata();
        if !std::ptr::eq(&raw const *extension.request.original, self)
            || request.source_id != source.source_id
            || request.runtime != source.runtime.0
            || request.instance != source.instance_id
            || request.connection_epoch != source.connection_epoch
            || slot
                .snapshot()
                .map_err(|_| ApprovalRejection::InvalidRequest)?
                .obligation
                != Some(request.obligation)
        {
            return Err(ApprovalRejection::InvalidRequest.into());
        }
        Ok(request.approval)
    }
}
impl From<ApprovalRejection> for NecessaryWriteError {
    fn from(value: ApprovalRejection) -> Self {
        Self::Rejected(value)
    }
}
/// Original operation cache remains owned by its original Source permission slot.
#[derive(Default)]
pub struct ApprovalWriteCache {
    sql_owner: tokio::sync::Mutex<()>,
    previous: Mutex<Vec<ApprovalWriteState>>,
    pub(super) result: Mutex<ApprovalWriteState>,
}
#[derive(Default)]
pub(super) struct ApprovalWriteState {
    pub(super) commit_confirmed: bool,
    pub(super) cleanup: Option<Arc<dyn ApprovalSqlCleanup>>,
    pub(super) cleanup_observation: Option<ApprovalCleanupObservation>,
    pub(super) report: Option<CloseReportState>,
    pub(super) final_rejection: Option<ApprovalRejection>,
}
impl ApprovalWriteCache {
    pub(super) fn has_ancillary_close(&self) -> Result<bool, NecessaryWriteError> {
        Ok(self
            .previous
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .iter()
            .any(|state| state.cleanup.is_some()))
    }
    pub(crate) fn retain_ancillary_close(
        &self,
        cleanup: Arc<dyn ApprovalSqlCleanup>,
    ) -> Result<(), NecessaryWriteError> {
        let mut previous = self
            .previous
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        previous
            .try_reserve(1)
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        previous.push(ApprovalWriteState {
            cleanup: Some(cleanup),
            ..ApprovalWriteState::default()
        });
        Ok(())
    }
    pub(crate) fn note_final_rejection(
        &self,
        reason: ApprovalRejection,
    ) -> Result<(), NecessaryWriteError> {
        self.result
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .final_rejection = Some(reason);
        Ok(())
    }
    pub(crate) fn final_rejection(&self) -> Result<Option<ApprovalRejection>, NecessaryWriteError> {
        Ok(self
            .result
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .final_rejection)
    }

    pub(crate) const fn sql_owner(&self) -> &tokio::sync::Mutex<()> {
        &self.sql_owner
    }
    pub(crate) fn observe_report(
        &self,
        report: CloseReportState,
    ) -> Result<(), NecessaryWriteError> {
        let mut state = self
            .result
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        if state.report.is_none() {
            state.report = Some(report);
        }
        Ok(())
    }
    pub(crate) fn confirmation(&self) -> Result<SqlConfirmation, NecessaryWriteError> {
        let state = self
            .result
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        let acquired = state.cleanup.is_some();
        Ok(SqlConfirmation {
            sql_operation: None,
            commit: if state.commit_confirmed {
                CommitState::Confirmed
            } else if acquired {
                CommitState::Unknown
            } else {
                CommitState::NotStarted
            },
            lease_close: match state.cleanup_observation {
                Some(ApprovalCleanupObservation::ConfirmedJoined) => LeaseCloseState::Confirmed,
                Some(ApprovalCleanupObservation::UnconfirmedJoined) => LeaseCloseState::Unconfirmed,
                Some(ApprovalCleanupObservation::Pending) => LeaseCloseState::Pending,
                None if acquired => LeaseCloseState::Pending,
                None => LeaseCloseState::NotAcquired,
            },
            close_report: state.report.unwrap_or(if acquired {
                CloseReportState::Pending
            } else {
                CloseReportState::NotReported
            }),
            task_join: match state.cleanup_observation {
                Some(
                    ApprovalCleanupObservation::ConfirmedJoined
                    | ApprovalCleanupObservation::UnconfirmedJoined,
                ) => SqlTaskJoinState::Joined,
                Some(ApprovalCleanupObservation::Pending) => SqlTaskJoinState::Pending,
                None if acquired => SqlTaskJoinState::Pending,
                None => SqlTaskJoinState::NotStarted,
            },
        })
    }
    pub(crate) async fn confirm_all_cleanup(
        &self,
        owner: &RegistryOwnerScope,
        until: Instant,
    ) -> Result<bool, NecessaryWriteError> {
        let mut receipts = {
            let previous = self
                .previous
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            previous
                .iter()
                .filter_map(|state| state.cleanup.clone())
                .collect::<Vec<_>>()
        };
        let receipt = self
            .result
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .cleanup
            .clone();
        if let Some(receipt) = receipt {
            receipts.push(receipt);
        }
        for receipt in receipts {
            if !receipt.original_owner_scope().matches(owner)
                || receipt.confirm_original_cleanup(until).await
                    != ApprovalCleanupObservation::ConfirmedJoined
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub(crate) async fn prepare_missing_repair(
        &self,
        owner: &RegistryOwnerScope,
        until: Instant,
    ) -> Result<(), NecessaryWriteError> {
        if !self.confirm_all_cleanup(owner, until).await? {
            return Err(NecessaryWriteError::SqlCloseUnconfirmed {
                commit_confirmed: false,
            });
        }
        let mut current = self
            .result
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        if current.commit_confirmed {
            return Err(ApprovalRejection::Conflict.into());
        }
        if current.cleanup.is_some() {
            let mut previous = self
                .previous
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            previous
                .try_reserve(1)
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            let old = std::mem::take(&mut *current);
            previous.push(old);
        }
        Ok(())
    }
    pub(crate) fn close_state(&self) -> Result<(bool, bool), NecessaryWriteError> {
        let state = self
            .result
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        Ok((state.cleanup.is_some(), state.commit_confirmed))
    }
    pub(crate) fn retain_actual_close(
        &self,
        commit_confirmed: bool,
        cleanup: Arc<dyn ApprovalSqlCleanup>,
    ) -> Result<(), NecessaryWriteError> {
        let mut state = self
            .result
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        if state.cleanup.is_some() {
            return Err(ApprovalRejection::Conflict.into());
        }
        state.commit_confirmed = commit_confirmed;
        state.cleanup = Some(cleanup);
        Ok(())
    }
    pub(crate) async fn confirm_actual_close(
        &self,
        owner: &RegistryOwnerScope,
        until: Instant,
    ) -> Result<bool, NecessaryWriteError> {
        let (commit, cleanup) = {
            let state = self
                .result
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            (state.commit_confirmed, state.cleanup.clone())
        };
        let Some(cleanup) = cleanup else {
            return Ok(false);
        };
        if !cleanup.original_owner_scope().matches(owner) {
            return Err(ApprovalRejection::Conflict.into());
        }
        let observed = cleanup.confirm_original_cleanup(until).await;
        self.result
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .cleanup_observation = Some(observed);
        Ok(commit && observed == ApprovalCleanupObservation::ConfirmedJoined)
    }
}

pub struct OriginalRequestProbe<P: NativeApprovalPort> {
    pub(super) resolution: ApprovalWriteCache,
    pub(super) resolution_untracked: super::AtomicBool,
    pub(super) original: Arc<ApprovalOriginal<P>>,
    pub(super) metadata: ApprovalRequestRecord,
    pub(super) fingerprint: [u8; 32],
    pub(super) write: ApprovalWriteCache,
}
impl<P: NativeApprovalPort> OriginalRequestProbe<P> {
    pub(crate) fn resolution_native(
        &self,
        owner: &RegistryOwnerScope,
    ) -> Result<crate::domain::native::NativeSessionKey, NecessaryWriteError> {
        self.verify_owner(owner)?;
        let slot = self
            .original
            .slot
            .upgrade()
            .ok_or(ApprovalRejection::InvalidRequest)?;
        crate::domain::native::NativeSessionKey::from_config(
            Some(self.original.source.metadata().source_id),
            &slot.session,
        )
        .map_err(|_| ApprovalRejection::InvalidRequest.into())
    }
    pub(crate) const fn resolution_cache(&self) -> &ApprovalWriteCache {
        &self.resolution
    }
    pub(crate) fn begin_resolution_acquisition(&self) {
        self.resolution_untracked
            .store(true, super::Ordering::SeqCst);
    }
    pub(crate) fn retain_resolution_close(
        &self,
        receipt: Arc<dyn ApprovalSqlCleanup>,
    ) -> Result<(), NecessaryWriteError> {
        self.resolution.retain_ancillary_close(receipt)?;
        self.resolution_untracked
            .store(false, super::Ordering::SeqCst);
        Ok(())
    }
    pub(super) fn resolution_inventory_complete(&self) -> bool {
        !self.resolution_untracked.load(super::Ordering::SeqCst)
    }

    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    pub(crate) fn instance_for_approval_test(&self) -> crate::domain::EntityId {
        self.original.source.metadata().instance_id
    }
    pub const fn metadata(&self) -> &ApprovalRequestRecord {
        &self.metadata
    }
    pub const fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
    }
    pub(crate) fn verify_owner(
        &self,
        owner: &RegistryOwnerScope,
    ) -> Result<(), NecessaryWriteError> {
        if self.original.validate(owner)? != self.metadata.approval {
            return Err(ApprovalRejection::InvalidRequest.into());
        }
        Ok(())
    }
    pub(crate) const fn write_cache(&self) -> &ApprovalWriteCache {
        &self.write
    }
}

pub struct VerifiedApprovalAssociation<P: NativeApprovalPort> {
    pub(super) mapping_rejected_before_commit: super::AtomicBool,
    pub(super) original: Arc<ApprovalOriginal<P>>,
    pub(super) metadata: ApprovalAssociationRecord,
    pub(super) fingerprint: [u8; 32],
    pub(super) write: ApprovalWriteCache,
}
impl<P: NativeApprovalPort> VerifiedApprovalAssociation<P> {
    pub(crate) fn reject_mapping_before_commit(&self) {
        self.mapping_rejected_before_commit
            .store(true, super::Ordering::SeqCst);
    }
    pub(crate) fn mapping_rejected_before_commit(&self) -> bool {
        self.mapping_rejected_before_commit
            .load(super::Ordering::SeqCst)
    }

    pub const fn metadata(&self) -> &ApprovalAssociationRecord {
        &self.metadata
    }
    pub const fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
    }
    pub(crate) fn verify_owner(
        &self,
        owner: &RegistryOwnerScope,
    ) -> Result<(), NecessaryWriteError> {
        if self.original.validate(owner)? != self.metadata.approval {
            return Err(ApprovalRejection::InvalidRequest.into());
        }
        Ok(())
    }
    pub(crate) const fn write_cache(&self) -> &ApprovalWriteCache {
        &self.write
    }
}

pub struct RegisteredApprovalProbe<P: NativeApprovalPort> {
    pub(super) original: Arc<ApprovalOriginal<P>>,
    pub(super) metadata: ApprovalCommandRecord,
    pub(super) fingerprint: [u8; 32],
    pub(super) write: ApprovalWriteCache,
}
impl<P: NativeApprovalPort> RegisteredApprovalProbe<P> {
    pub(crate) fn resolution_native(
        &self,
        owner: &RegistryOwnerScope,
    ) -> Result<crate::domain::native::NativeSessionKey, NecessaryWriteError> {
        self.verify_owner(owner)?;
        let slot = self
            .original
            .slot
            .upgrade()
            .ok_or(ApprovalRejection::InvalidRequest)?;
        crate::domain::native::NativeSessionKey::from_config(
            Some(self.original.source.metadata().source_id),
            &slot.session,
        )
        .map_err(|_| ApprovalRejection::InvalidRequest.into())
    }
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    pub(crate) fn instance_for_approval_test(&self) -> crate::domain::EntityId {
        self.original.source.metadata().instance_id
    }
    pub const fn metadata(&self) -> &ApprovalCommandRecord {
        &self.metadata
    }
    pub const fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
    }
    pub(crate) fn verify_owner(
        &self,
        owner: &RegistryOwnerScope,
    ) -> Result<(), NecessaryWriteError> {
        if self.original.validate(owner)? != self.metadata.approval {
            return Err(ApprovalRejection::InvalidRequest.into());
        }
        Ok(())
    }
    pub(crate) const fn write_cache(&self) -> &ApprovalWriteCache {
        &self.write
    }
}

pub struct ApprovalAdmissionProbe<P: NativeApprovalPort> {
    pub(super) original: Arc<ApprovalOriginal<P>>,
    pub(super) metadata: ApprovalDecisionRecord,
    pub(super) fingerprint: [u8; 32],
    pub(super) write: ApprovalWriteCache,
}
impl<P: NativeApprovalPort> ApprovalAdmissionProbe<P> {
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    pub(crate) fn instance_for_approval_test(&self) -> crate::domain::EntityId {
        self.original.source.metadata().instance_id
    }
    pub(crate) fn original_registration_metadata(
        &self,
    ) -> Result<ApprovalCommandRecord, NecessaryWriteError> {
        let permission = self
            .original
            .slot
            .upgrade()
            .ok_or(ApprovalRejection::InvalidRequest)?;
        let approval = permission
            .approval
            .get()
            .ok_or(ApprovalRejection::InvalidRequest)?;
        let state = approval
            .state
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        let admission = state
            .admission
            .as_ref()
            .filter(|operation| std::ptr::eq(&raw const *operation.probe, self))
            .or_else(|| {
                state
                    .rejected_admissions
                    .iter()
                    .find(|operation| std::ptr::eq(&raw const *operation.probe, self))
            })
            .ok_or(ApprovalRejection::InvalidRequest)?;
        if !Arc::ptr_eq(&admission.registration.probe.original, &self.original) {
            return Err(ApprovalRejection::InvalidRequest.into());
        }
        Ok(admission.registration.probe.metadata().clone())
    }

    pub const fn metadata(&self) -> &ApprovalDecisionRecord {
        &self.metadata
    }
    pub const fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
    }
    pub(crate) fn verify_owner(
        &self,
        owner: &RegistryOwnerScope,
    ) -> Result<(), NecessaryWriteError> {
        if self.original.validate(owner)? != self.metadata.approval {
            return Err(ApprovalRejection::InvalidRequest.into());
        }
        Ok(())
    }
    pub(crate) const fn write_cache(&self) -> &ApprovalWriteCache {
        &self.write
    }
}

pub struct ApprovalReplyArmProof<P: NativeApprovalPort> {
    pub(super) original: Arc<ApprovalOriginal<P>>,
    pub(super) metadata: ApprovalAttemptRecord,
    pub(super) fingerprint: [u8; 32],
    pub(super) write: ApprovalWriteCache,
}
impl<P: NativeApprovalPort> ApprovalReplyArmProof<P> {
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    pub(crate) fn instance_for_approval_test(&self) -> crate::domain::EntityId {
        self.original.source.metadata().instance_id
    }
    pub const fn metadata(&self) -> &ApprovalAttemptRecord {
        &self.metadata
    }
    pub const fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
    }
    pub(crate) fn verify_owner(
        &self,
        owner: &RegistryOwnerScope,
    ) -> Result<(), NecessaryWriteError> {
        if self.original.validate(owner)? != self.metadata.approval {
            return Err(ApprovalRejection::InvalidRequest.into());
        }
        Ok(())
    }
    pub(crate) const fn write_cache(&self) -> &ApprovalWriteCache {
        &self.write
    }
}

pub struct ApprovalReplyTerminalProof<P: NativeApprovalPort> {
    pub(super) original: Arc<ApprovalOriginal<P>>,
    pub(super) metadata: ApprovalTerminalRecord,
    pub(super) fingerprint: [u8; 32],
    pub(super) write: ApprovalWriteCache,
}
impl<P: NativeApprovalPort> ApprovalReplyTerminalProof<P> {
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    pub(crate) fn instance_for_approval_test(&self) -> crate::domain::EntityId {
        self.original.source.metadata().instance_id
    }
    pub const fn metadata(&self) -> &ApprovalTerminalRecord {
        &self.metadata
    }
    pub const fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
    }
    pub(crate) fn verify_owner(
        &self,
        owner: &RegistryOwnerScope,
    ) -> Result<(), NecessaryWriteError> {
        if self.original.validate(owner)? != self.metadata.approval {
            return Err(ApprovalRejection::InvalidRequest.into());
        }
        Ok(())
    }
    pub(crate) const fn write_cache(&self) -> &ApprovalWriteCache {
        &self.write
    }
}

pub struct ApprovalInvalidationProof<P: NativeApprovalPort> {
    pub(super) original: Arc<ApprovalOriginal<P>>,
    pub(super) metadata: ApprovalInvalidationRecord,
    pub(super) fingerprint: [u8; 32],
    pub(super) write: ApprovalWriteCache,
}
impl<P: NativeApprovalPort> ApprovalInvalidationProof<P> {
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    pub(crate) fn instance_for_approval_test(&self) -> crate::domain::EntityId {
        self.original.source.metadata().instance_id
    }
    pub const fn metadata(&self) -> &ApprovalInvalidationRecord {
        &self.metadata
    }
    pub const fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
    }
    pub(crate) fn verify_owner(
        &self,
        owner: &RegistryOwnerScope,
    ) -> Result<(), NecessaryWriteError> {
        if self.original.validate(owner)? != self.metadata.approval {
            return Err(ApprovalRejection::InvalidRequest.into());
        }
        Ok(())
    }
    pub(crate) const fn write_cache(&self) -> &ApprovalWriteCache {
        &self.write
    }
}

pub struct StoredApprovalObservation {
    pub request: StoredApprovalRequest,
    pub association: Option<StoredApprovalAssociation>,
}
pub trait NativeApprovalRepository: super::ApprovalAuditPort + Send + Sync {
    fn approval_owner_scope(&self) -> RegistryOwnerScope;
    fn resolve_approval_target<'a, P: NativeApprovalPort>(
        &'a self,
        original: &'a OriginalRequestProbe<P>,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<Option<crate::domain::EntityId>, NecessaryWriteError>>;
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    fn approval_native_test_cut<'a, P: NativeApprovalPort>(
        &'a self,
        proof: &'a ApprovalAdmissionProbe<P>,
        point: crate::session_storage::approval_owner::test_control::ApprovalCut,
        response: crate::domain::approval_owner::ApprovalReplyId,
        until: Instant,
    ) -> BoxFuture<'a, Result<(), super::DispatchError>>;
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    fn stop_approval_test_source(&self, instance: crate::domain::EntityId);
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    fn approval_test_controller(
        &self,
    ) -> Arc<crate::session_storage::approval_owner::test_control::ApprovalTestController>;
    fn observe_approval<'a, P: NativeApprovalPort>(
        &'a self,
        probe: &'a OriginalRequestProbe<P>,
        association: Option<&'a VerifiedApprovalAssociation<P>>,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredApprovalObservation, NecessaryWriteError>>;
    fn register_approval<'a, P: NativeApprovalPort>(
        &'a self,
        proof: &'a RegisteredApprovalProbe<P>,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredApprovalCommand, NecessaryWriteError>>;
    fn decide_approval<'a, P: NativeApprovalPort>(
        &'a self,
        proof: &'a ApprovalAdmissionProbe<P>,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredApprovalDecision, NecessaryWriteError>>;
    fn arm_approval_reply<'a, P: NativeApprovalPort>(
        &'a self,
        proof: &'a ApprovalReplyArmProof<P>,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredApprovalAttempt, NecessaryWriteError>>;
    fn record_approval_reply<'a, P: NativeApprovalPort>(
        &'a self,
        proof: &'a ApprovalReplyTerminalProof<P>,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredApprovalTerminal, NecessaryWriteError>>;
    fn invalidate_approval<'a, P: NativeApprovalPort>(
        &'a self,
        proof: &'a ApprovalInvalidationProof<P>,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredApprovalInvalidation, NecessaryWriteError>>;
    fn inspect_approval<'a, P: NativeApprovalPort>(
        &'a self,
        original: &'a OriginalRequestProbe<P>,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<ApprovalStoredInspection, NecessaryWriteError>>;
}
