//! Exact original-request target lookup and atomically validated new association.
#![allow(
    clippy::match_same_arms,
    clippy::too_many_lines,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    ApprovalRejection, Arc, CloseOutcome, NativeApprovalPort, NecessaryWriteError,
    OriginalRequestProbe, RegistryDeadline, SqliteConnection, SqliteNativeDispatch, StoredApproval,
    StoredApprovalObservation, VerifiedApprovalAssociation, bounded, exact, insert, unavailable,
};
use crate::domain::EntityId;
use sqlx::Row;
impl SqliteNativeDispatch {
    pub(super) async fn resolve_original_approval_target<P: NativeApprovalPort>(
        &self,
        original: &OriginalRequestProbe<P>,
        d: &RegistryDeadline,
    ) -> Result<Option<EntityId>, NecessaryWriteError> {
        let native = original.resolution_native(&self.registry.owner_scope())?;
        let cache = original.resolution_cache();
        let _owner = cache.sql_owner().lock().await;
        if !cache
            .confirm_all_cleanup(&self.registry.owner_scope(), d.deadline())
            .await?
        {
            return Err(NecessaryWriteError::SqlCloseUnconfirmed {
                commit_confirmed: false,
            });
        }
        original.begin_resolution_acquisition();
        let mut c = self
            .registry
            .detached_owned_connection()
            .await
            .map_err(|_| unavailable())?;
        #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
        c.attach_approval_test_context(super::test_control::current_original_context());
        let result = bounded(d, async {
            super::super::schema::validate_approval_necessary(
                c.connection().map_err(|_| unavailable())?,
            )
            .await
            .map_err(|_| unavailable())?;
            lookup(
                c.connection().map_err(|_| unavailable())?,
                native.source().0,
                native.native_id(),
            )
            .await
        })
        .await;
        let waiter = c.begin_owned_close().map_err(|_| unavailable())?;
        original.retain_resolution_close(Arc::new(waiter.receipt()))?;
        let report = bounded(d, async { Ok(waiter.wait_report().await) }).await;
        if !matches!(report, Ok(CloseOutcome::Confirmed))
            || !cache
                .confirm_all_cleanup(&self.registry.owner_scope(), d.deadline())
                .await?
        {
            return Err(NecessaryWriteError::SqlCloseUnconfirmed {
                commit_confirmed: false,
            });
        }
        result
    }
    pub(super) async fn observe_registered_approval<P: NativeApprovalPort>(
        &self,
        request: &OriginalRequestProbe<P>,
        proof: &VerifiedApprovalAssociation<P>,
        d: &RegistryDeadline,
    ) -> Result<StoredApprovalObservation, NecessaryWriteError> {
        // All registered-session paths, including the formerly cached-request fast path, use this helper.
        let native = request.resolution_native(&self.registry.owner_scope())?;
        if proof.write_cache().close_state()?.1 {
            let request = self
                .approval_save(
                    request.metadata(),
                    *request.fingerprint(),
                    request.write_cache(),
                    d,
                )
                .await?;
            let association = self
                .approval_save(
                    proof.metadata(),
                    *proof.fingerprint(),
                    proof.write_cache(),
                    d,
                )
                .await?;
            return Ok(StoredApprovalObservation {
                request,
                association: Some(association),
            });
        }
        let _request_owner = request.write_cache().sql_owner().lock().await;
        let _association_owner = proof.write_cache().sql_owner().lock().await;
        for cache in [request.write_cache(), proof.write_cache()] {
            let (cached, committed) = cache.close_state()?;
            if cached && !committed {
                cache
                    .prepare_missing_repair(&self.registry.owner_scope(), d.deadline())
                    .await?;
            }
        }
        let mut c = self.approval_begin(d, &[proof.write_cache()]).await?;
        let result = bounded(d, async {
            let c = c.connection().map_err(|_| unavailable())?;
            let existing: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM main.node_approval_associations WHERE approval_id=?",
            )
            .bind(proof.metadata().approval.encode())
            .fetch_one(&mut *c)
            .await
            .map_err(|_| unavailable())?;
            if existing == 0 {
                if lookup(c, native.source().0, native.native_id()).await?
                    != Some(proof.metadata().target)
                {
                    return Err(ApprovalRejection::UnassociatedTarget.into());
                }
            } else if existing == 1 {
                exact(c, proof.metadata()).await?;
            } else {
                return Err(unavailable());
            }
            let request = insert(c, request.metadata()).await?;
            let association = insert(c, proof.metadata()).await?;
            Ok((request, association))
        })
        .await;
        let mismatch = matches!(
            result,
            Err(NecessaryWriteError::Rejected(
                ApprovalRejection::UnassociatedTarget
            ))
        );
        let rollback_confirmed = if mismatch {
            bounded(d, async {
                sqlx::query("ROLLBACK")
                    .execute(c.connection().map_err(|_| unavailable())?)
                    .await
                    .map_err(|_| unavailable())?;
                Ok(())
            })
            .await
            .is_ok()
        } else {
            false
        };
        // Request may already have a prior reliable receipt; never overwrite that original receipt.
        let request_cached = request.write_cache().close_state()?.0;
        let caches = if request_cached {
            vec![proof.write_cache()]
        } else {
            vec![request.write_cache(), proof.write_cache()]
        };
        let finished = self.approval_finish(c, result, &caches, d).await;
        if mismatch
            && rollback_confirmed
            && proof
                .write_cache()
                .confirm_all_cleanup(&self.registry.owner_scope(), d.deadline())
                .await?
        {
            proof.reject_mapping_before_commit();
        }
        let (request_row, association_row) = finished?;
        Ok(StoredApprovalObservation {
            request: StoredApproval::from_exact_owner_result(
                request_row,
                request.write_cache().confirmation()?,
                *request.fingerprint(),
            ),
            association: Some(StoredApproval::from_exact_owner_result(
                association_row,
                proof.write_cache().confirmation()?,
                *proof.fingerprint(),
            )),
        })
    }
}
pub(super) async fn lookup(
    c: &mut SqliteConnection,
    source: EntityId,
    native: &str,
) -> Result<Option<EntityId>, NecessaryWriteError> {
    let rows = sqlx::query(
        "SELECT target_id FROM main.product_targets WHERE source_id=? AND native_id=? LIMIT 2",
    )
    .bind(source.encode())
    .bind(native)
    .fetch_all(c)
    .await
    .map_err(|_| unavailable())?;
    match rows.as_slice() {
        [] => Ok(None),
        [row] => {
            let value: String = row.try_get("target_id").map_err(|_| unavailable())?;
            EntityId::decode(&value)
                .map(Some)
                .map_err(|_| unavailable())
        }
        _ => Ok(None),
    }
}
