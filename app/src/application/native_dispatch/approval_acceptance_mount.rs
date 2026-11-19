//! Readonly weak references to the actual original Source, permission and task groups.
#![allow(
    dead_code,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::needless_collect,
    clippy::redundant_pub_crate,
    reason = "Read-only diagnostic mount for approval owner acceptance tests"
)]
use super::{
    environment::{PermissionResponseOwner, SourcePermissionSlot},
    runtime::SourceSlot,
    *,
};
use crate::domain::approval_owner::{ApprovalId, ApprovalRejection, ApprovalReplyId};
use std::sync::Weak;
#[derive(Debug)]
pub(crate) struct OriginalApprovalDiagnosticSnapshot {
    pub instance: EntityId,
    pub approval: ApprovalId,
    pub source_jobs_registered: usize,
    pub source_jobs_joined: usize,
    pub source_jobs_failed: usize,
    pub source_jobs_sealed: bool,
    pub cleanup_joined: bool,
    pub metadata_closer_joined: bool,
    pub physical_proof_cached: bool,
    pub approval_manifest_cached: bool,
    pub required_metadata_count: usize,
    pub required_metadata: Vec<(&'static str, EntityId)>,
    pub allow_disabled: bool,
    pub retry_nonce: bool,
    pub invalidations: usize,
    pub controls: usize,
    pub rejected_admissions: usize,
    pub pending_response: Option<ApprovalReplyId>,
}
trait OriginalObservation: Send + Sync {
    fn source_alive(&self) -> bool;
    fn request_alive(&self) -> bool;
    fn snapshot(&self) -> Result<OriginalApprovalDiagnosticSnapshot, ApprovalRejection>;
}
pub(crate) struct OriginalApprovalObservation {
    original: Box<dyn OriginalObservation>,
}
impl OriginalApprovalObservation {
    pub(crate) fn original_source_alive(&self) -> bool {
        self.original.source_alive()
    }
    pub(crate) fn original_request_alive(&self) -> bool {
        self.original.request_alive()
    }
    pub(crate) fn snapshot(&self) -> Result<OriginalApprovalDiagnosticSnapshot, ApprovalRejection> {
        self.original.snapshot()
    }
}
struct ActualOriginal<P, L>
where
    P: NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
{
    source: Weak<SourceSlot<P, L>>,
    permission: Weak<SourcePermissionSlot<P>>,
    id: ApprovalId,
}
impl<P, L> OriginalObservation for ActualOriginal<P, L>
where
    P: NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
{
    fn source_alive(&self) -> bool {
        self.source.strong_count() != 0
    }
    fn request_alive(&self) -> bool {
        self.permission.strong_count() != 0
    }
    fn snapshot(&self) -> Result<OriginalApprovalDiagnosticSnapshot, ApprovalRejection> {
        let source = self.source.upgrade().ok_or(ApprovalRejection::Stopped)?;
        let permission = self
            .permission
            .upgrade()
            .ok_or(ApprovalRejection::InvalidRequest)?;
        let count = source
            .jobs
            .approval_diagnostic_counts()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        let pending_response = match permission
            .response_owner
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .as_ref()
        {
            Some(PermissionResponseOwner::Selected(original)) => Some(original.id),
            Some(PermissionResponseOwner::Cancelled { id, .. }) => {
                Some(ApprovalReplyId::from_entity(*id))
            }
            Some(PermissionResponseOwner::ManagementFailure(original)) => Some(original.id),
            None => None,
        };
        let physical_proof_cached = source
            .physical_closed
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .is_some();
        let approval_manifest_cached = source
            .closure
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .as_ref()
            .is_some_and(|proof| {
                proof
                    .approvals()
                    .requests()
                    .iter()
                    .any(|request| request.approval == self.id)
            });
        let required_ids = source
            .memory
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .requirements
            .keys()
            .copied()
            .collect::<Vec<_>>();
        let approval = permission
            .approval
            .get()
            .ok_or(ApprovalRejection::InvalidRequest)?;
        let approval_state = approval
            .state
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        let required_metadata = required_ids
            .into_iter()
            .map(|id| {
                let role = if id == approval.request.metadata().approval.entity() {
                    "request"
                } else if approval_state
                    .registered
                    .values()
                    .any(|operation| id == operation.probe.metadata().registration.entity())
                {
                    "registration"
                } else if approval_state
                    .admission
                    .as_ref()
                    .is_some_and(|operation| id == operation.probe.metadata().operation)
                {
                    "decision"
                } else if approval_state
                    .responses
                    .iter()
                    .any(|operation| id == operation.id.entity())
                {
                    "response"
                } else if approval_state
                    .failure_responses
                    .iter()
                    .any(|operation| id == operation.id.entity())
                {
                    "failure-response"
                } else if approval_state
                    .invalidations
                    .iter()
                    .any(|proof| id == proof.metadata().id)
                {
                    "invalidation"
                } else if approval_state
                    .controls
                    .values()
                    .any(|facts| id == facts.response.entity())
                {
                    "control"
                } else if approval_state
                    .rejected_admissions
                    .iter()
                    .any(|operation| id == operation.probe.metadata().operation)
                {
                    "rejected-decision"
                } else {
                    "other"
                };
                (role, id)
            })
            .collect();
        Ok(OriginalApprovalDiagnosticSnapshot {
            instance: source.pre.metadata().instance_id,
            approval: self.id,
            source_jobs_registered: count.0,
            source_jobs_joined: count.1,
            source_jobs_failed: count.2,
            source_jobs_sealed: count.3,
            cleanup_joined: source
                .cleanup
                .get()
                .is_some_and(|job| job.confirmed_joined()),
            metadata_closer_joined: source
                .metadata_closer
                .get()
                .is_some_and(|job| job.confirmed_joined()),
            physical_proof_cached,
            approval_manifest_cached,
            required_metadata_count: source.pre.authority.required.load(Ordering::Acquire),
            required_metadata,
            allow_disabled: approval_state.allow_disabled,
            retry_nonce: approval_state.retry_nonce.is_some(),
            invalidations: approval_state.invalidations.len(),
            controls: approval_state.controls.len(),
            rejected_admissions: approval_state.rejected_admissions.len(),
            pending_response,
        })
    }
}
pub(crate) fn observe_approval_original<P, L, G, N>(
    native: &NativeRuntimeHandle<P, L, G, N>,
    id: ApprovalId,
) -> Result<OriginalApprovalObservation, ApprovalRejection>
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
    let (source, permission, _) =
        super::approval_recovery::OriginalApprovalReconciler::<P, L, G, N>::locate(
            &native.core,
            id,
        )?;
    Ok(OriginalApprovalObservation {
        original: Box::new(ActualOriginal::<P, L> {
            source: Arc::downgrade(&source),
            permission: Arc::downgrade(&permission),
            id,
        }),
    })
}
