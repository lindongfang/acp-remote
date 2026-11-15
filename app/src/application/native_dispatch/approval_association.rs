//! Same-original Source-owned association resolution. Reads never invoke this writer.
#![allow(
    clippy::or_fun_call,
    clippy::significant_drop_tightening,
    clippy::too_many_lines,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    Arc, AtomicBool, ConfigurationPort, DispatchError, NativeApprovalPort,
    NativeApprovalRepository, NativeControlRepository, NativeCoordinator, NativeDispatchRepository,
    NativeResourceRepository, Ordering, OwnedNativeSource, PermissionPendingSnapshot,
    SourceLaunchPort, VerifiedApprovalAssociation,
    approval_inventory::{ApprovalSlot, digest_data},
    approval_records::ApprovalWriteCache,
    environment::SourcePermissionSlot,
    runtime::SourceSlot,
    tasks::TaskSlot,
};
use crate::domain::{
    approval_owner::{ApprovalRejection, NecessaryWriteError},
    approval_records::{ApprovalAssociationRecord, StoredAssociationKind},
};
use std::sync::{Mutex, OnceLock};
pub(super) struct ApprovalAssociationOperation<P: NativeApprovalPort> {
    pub(super) candidate: Mutex<Option<Arc<VerifiedApprovalAssociation<P>>>>,
    job: OnceLock<Arc<TaskSlot>>,
    result: Mutex<Option<Result<(), NecessaryWriteError>>>,
    joined: AtomicBool,
    pre_candidate_cleanup_confirmed: AtomicBool,
    changed: tokio::sync::Notify,
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
    fn association_current(
        &self,
        source: &SourceSlot<P, L>,
        permission: &SourcePermissionSlot<P>,
        slot: &ApprovalSlot<P>,
    ) -> Result<(), NecessaryWriteError> {
        if !Arc::ptr_eq(&slot.request.original.source, &source.pre)
            || !slot
                .request
                .original
                .slot
                .upgrade()
                .is_some_and(|original| std::ptr::eq(&raw const *original, permission))
        {
            return Err(ApprovalRejection::InvalidRequest.into());
        }
        let state = slot
            .state
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        self.approval_current(source, permission, &state)?;
        if state.admission.is_some()
            || state.decision.is_some()
            || !state.registered.is_empty()
            || !state.responses.is_empty()
            || !state.invalidations.is_empty()
        {
            return Err(ApprovalRejection::InvalidRequest.into());
        }
        Ok(())
    }
    pub(super) async fn ensure_approval_association(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        permission: &Arc<SourcePermissionSlot<P>>,
        slot: &Arc<ApprovalSlot<P>>,
    ) -> Result<(), NecessaryWriteError> {
        if slot
            .association
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .is_some()
        {
            return Ok(());
        }
        let connection = source
            .source
            .get()
            .ok_or(ApprovalRejection::InvalidRequest)?;
        if self
            .protocol
            .inspect_permission(connection, &permission.request)
            .map_err(|_| ApprovalRejection::InvalidRequest)?
            != PermissionPendingSnapshot::Waiting
        {
            return Err(ApprovalRejection::InvalidRequest.into());
        }
        let operation = {
            let _source = source
                .pre
                .authority
                .gate
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            let _action = permission
                .action
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            if slot
                .association
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
                .is_some()
            {
                return Ok(());
            }
            self.association_current(source, permission, slot)?;
            let mut current = slot
                .association_resolution
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            if let Some(old) = current.as_ref() {
                let result = *old
                    .result
                    .lock()
                    .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
                let discard = old.job.get().is_some_and(|job| job.confirmed_joined())
                    && result.is_some()
                    && old
                        .candidate
                        .lock()
                        .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
                        .as_ref()
                        .map_or(
                            matches!(
                                result,
                                Some(Err(NecessaryWriteError::Rejected(
                                    ApprovalRejection::UnassociatedTarget
                                )))
                            ) || (matches!(
                                result,
                                Some(Err(NecessaryWriteError::SqlCloseUnconfirmed {
                                    commit_confirmed: false
                                }))
                            ) && old.pre_candidate_cleanup_confirmed.load(Ordering::Acquire)),
                            |proof| proof.mapping_rejected_before_commit(),
                        );
                if discard {
                    current.take();
                }
            }
            if let Some(existing) = current.as_ref() {
                existing.clone()
            } else {
                let operation = Arc::new(ApprovalAssociationOperation {
                    candidate: Mutex::new(None),
                    job: OnceLock::new(),
                    result: Mutex::new(None),
                    joined: AtomicBool::new(false),
                    pre_candidate_cleanup_confirmed: AtomicBool::new(false),
                    changed: tokio::sync::Notify::new(),
                });
                let core = self.clone();
                let original_source = source.clone();
                let original_permission = permission.clone();
                let approval = slot.clone();
                let owned = operation.clone();
                let failed_core = self.clone();
                let failed_source = source.clone();
                let job = source
                    .jobs
                    .spawn_guarded(
                        async move {
                            let result = core
                                .run_approval_association(
                                    &original_source,
                                    &original_permission,
                                    &approval,
                                    &owned,
                                )
                                .await;
                            if matches!(
                                result,
                                Err(NecessaryWriteError::SqlCloseUnconfirmed {
                                    commit_confirmed: false
                                })
                            ) {
                                // Keep the first report Unknown; only this original owned task
                                // may establish that its read-only resolver debt actually converged.
                                let _ = core.confirm_pre_candidate_cleanup(&approval, &owned).await;
                            }
                            if let Ok(mut state) = owned.result.lock() {
                                *state = Some(result);
                            }
                            approval.publish_change();
                        },
                        move || {
                            failed_core.fail_source(
                                &failed_source,
                                DispatchError::RequiredStateUnavailable,
                            );
                        },
                    )
                    .map_err(|_| ApprovalRejection::Stopped)?;
                let _ = operation.job.set(job.clone());
                *current = Some(operation.clone());
                // Joining belongs to the original Source, never to a caller's read/await future.
                // The Source closer may join this same slot; an unpolled caller holds no join lock.
                let joined_operation = operation.clone();
                let failed_core = self.clone();
                let failed_source = source.clone();
                source
                    .tasks
                    .spawn_guarded(
                        async move {
                            job.join_unbounded().await;
                            joined_operation
                                .joined
                                .store(job.confirmed_joined(), Ordering::Release);
                            joined_operation.changed.notify_waiters();
                        },
                        move || {
                            failed_core.fail_source(
                                &failed_source,
                                DispatchError::RequiredStateUnavailable,
                            );
                        },
                    )
                    .map_err(|_| ApprovalRejection::Stopped)?;
                operation
            }
        };
        let until = self
            .deadline()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .deadline();
        loop {
            let changed = operation.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if operation.joined.load(Ordering::Acquire) {
                break;
            }
            tokio::time::timeout_at(until, changed)
                .await
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        }
        if operation.job.get().is_none_or(|job| job.failed()) {
            return Err(ApprovalRejection::NecessaryStorageUnavailable.into());
        }
        operation
            .result
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .as_ref()
            .copied()
            .ok_or(ApprovalRejection::NecessaryStorageUnavailable)?
    }
    async fn run_approval_association(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        permission: &Arc<SourcePermissionSlot<P>>,
        slot: &Arc<ApprovalSlot<P>>,
        operation: &ApprovalAssociationOperation<P>,
    ) -> Result<(), NecessaryWriteError> {
        let d = self
            .deadline()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        if let Some(job) = slot.request_job.get()
            && (!job.join(d.deadline()).await || job.failed())
        {
            return Err(ApprovalRejection::NecessaryStorageUnavailable.into());
        }
        let target = self
            .repository
            .resolve_approval_target(&slot.request, &d)
            .await?
            .ok_or(ApprovalRejection::UnassociatedTarget)?;
        let connection = source
            .source
            .get()
            .ok_or(ApprovalRejection::InvalidRequest)?;
        if self
            .protocol
            .inspect_permission(connection, &permission.request)
            .map_err(|_| ApprovalRejection::InvalidRequest)?
            != PermissionPendingSnapshot::Waiting
        {
            return Err(ApprovalRejection::InvalidRequest.into());
        }
        let fingerprint = digest_data(
            b"acpr-approval-registered-session-v1\0",
            &[&slot.request.fingerprint, target.encode().as_bytes()],
        )
        .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        let candidate = Arc::new(VerifiedApprovalAssociation {
            original: slot.request.original.clone(),
            metadata: ApprovalAssociationRecord {
                approval: slot.request.metadata().approval,
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
        });
        {
            let _gate = source
                .pre
                .authority
                .gate
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            let _action = permission
                .action
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            self.association_current(source, permission, slot)?;
            *operation
                .candidate
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)? =
                Some(candidate.clone());
        }
        self.require_approval_fact(source, slot.request.metadata().approval.entity())
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        self.finish_approval_association(source, slot, &candidate, &d)
            .await
    }
    pub(super) async fn repair_pending_approval_association(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        slot: &Arc<ApprovalSlot<P>>,
    ) -> Result<(), NecessaryWriteError> {
        if slot
            .association
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .is_some()
        {
            return Ok(());
        }
        let operation = {
            slot.association_resolution
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
                .clone()
        };
        if let Some(operation) = operation {
            let candidate = {
                operation
                    .candidate
                    .lock()
                    .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
                    .clone()
            };
            if candidate.is_none() {
                if operation
                    .job
                    .get()
                    .is_none_or(|job| !job.confirmed_joined() || job.failed())
                {
                    return Err(ApprovalRejection::NecessaryStorageUnavailable.into());
                }
                if matches!(
                    *operation
                        .result
                        .lock()
                        .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?,
                    Some(Err(NecessaryWriteError::SqlCloseUnconfirmed {
                        commit_confirmed: false
                    }))
                ) && !self.confirm_pre_candidate_cleanup(slot, &operation).await?
                {
                    return Err(NecessaryWriteError::SqlCloseUnconfirmed {
                        commit_confirmed: false,
                    });
                }
            }
            if let Some(candidate) = candidate
                && !candidate.mapping_rejected_before_commit()
            {
                let d = self
                    .deadline()
                    .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
                if let Some(job) = operation.job.get()
                    && (!job.join(d.deadline()).await || job.failed())
                {
                    return Err(ApprovalRejection::NecessaryStorageUnavailable.into());
                }
                return self
                    .finish_approval_association(source, slot, &candidate, &d)
                    .await;
            }
        }
        Ok(())
    }
    async fn confirm_pre_candidate_cleanup(
        &self,
        slot: &ApprovalSlot<P>,
        operation: &ApprovalAssociationOperation<P>,
    ) -> Result<bool, NecessaryWriteError> {
        if operation
            .candidate
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .is_some()
            || !slot.request.resolution_inventory_complete()
            || !slot.request.resolution_cache().has_ancillary_close()?
        {
            return Ok(false);
        }
        let d = self
            .deadline()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        let cache = slot.request.resolution_cache();
        let _sql_owner = cache.sql_owner().lock().await;
        let confirmed = slot.request.resolution_inventory_complete()
            && cache
                .confirm_all_cleanup(&slot.request.original.owner, d.deadline())
                .await?;
        if confirmed {
            operation
                .pre_candidate_cleanup_confirmed
                .store(true, Ordering::Release);
        }
        Ok(confirmed)
    }
    async fn finish_approval_association(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        slot: &Arc<ApprovalSlot<P>>,
        candidate: &Arc<VerifiedApprovalAssociation<P>>,
        d: &crate::application::request_registry::RegistryDeadline,
    ) -> Result<(), NecessaryWriteError> {
        let saved = self
            .repository
            .observe_approval(&slot.request, Some(candidate), d)
            .await;
        match saved {
            Ok(stored) => {
                let mut association = slot
                    .association
                    .lock()
                    .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
                if let Some(existing) = association.as_ref() {
                    if !Arc::ptr_eq(existing, candidate) {
                        return Err(ApprovalRejection::Conflict.into());
                    }
                } else {
                    *association = Some(candidate.clone());
                }
                drop(association);
                let mut state = slot
                    .state
                    .lock()
                    .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
                state.request_save = *stored.request.confirmation();
                state.request_error = None;
                drop(state);
                self.confirm_approval_fact(source, slot.request.metadata().approval.entity())
                    .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
                Ok(())
            }
            Err(error) => {
                if candidate.mapping_rejected_before_commit() {
                    self.confirm_approval_fact(source, slot.request.metadata().approval.entity())
                        .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
                }
                Err(error)
            }
        }
    }
}
