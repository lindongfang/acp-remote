//! Necessary approvals use the original registry and original physical close slots.
#![allow(
    clippy::too_many_lines,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    native_dispatch::SqliteNativeDispatch,
    registry_connection_owner::{CloseOutcome, RegistryConnection},
};
use crate::{
    application::{
        native_dispatch::{
            ApprovalAdmissionProbe, ApprovalInvalidationProof, ApprovalReplyArmProof,
            ApprovalReplyTerminalProof, ApprovalWriteCache, BoxFuture, NativeApprovalPort,
            NativeApprovalRepository, OriginalRequestProbe, RegisteredApprovalProbe,
            StoredApprovalObservation, VerifiedApprovalAssociation,
        },
        request_registry::RegistryDeadline,
    },
    domain::{
        approval_owner::{
            ActualReplyKind, ApprovalId, ApprovalInvalidationReason, ApprovalRegistrationId,
            ApprovalRejection, ApprovalReplyId, CloseReportState, CommitState, KnownOptionKind,
            LeaseCloseState, NecessaryWriteError, ReplyOrigin, SqlConfirmation, SqlTaskJoinState,
            TerminalReplyObservation,
        },
        approval_records::{
            ApprovalAssociationRecord, ApprovalAttemptRecord, ApprovalCommandRecord,
            ApprovalDecisionRecord, ApprovalInvalidationRecord, ApprovalRequestRecord,
            ApprovalStoredInspection, ApprovalTerminalRecord, StoredApproval,
            StoredApprovalAttempt, StoredApprovalCommand, StoredApprovalCommandState,
            StoredApprovalDecision, StoredApprovalInvalidation, StoredApprovalRejection,
            StoredApprovalTerminal, StoredAssociationKind, StoredRpcKind,
        },
    },
};
use sqlx::SqliteConnection;
use std::{future::Future, sync::Arc};
mod association;
mod audit;
pub(super) mod closure;
mod rows;
#[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
pub(crate) mod test_control;
#[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
mod test_migration;
#[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
pub(in crate::session_storage) mod test_sql;
use rows::ApprovalRow;
#[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
use test_control::{ApprovalCut, ApprovalCutState, ApprovalOperationTag, ApprovalSqlOperation};
async fn approval_operation<T>(
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))] controller: Arc<
        test_control::ApprovalTestController,
    >,
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))] tag: ApprovalOperationTag,
    future: impl Future<Output = T>,
) -> T {
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    {
        test_control::with_original_operation(controller, tag, future).await
    }
    #[cfg(not(all(test, feature = "approval-owner-acceptance-fixtures")))]
    {
        future.await
    }
}

fn unavailable() -> NecessaryWriteError {
    ApprovalRejection::NecessaryStorageUnavailable.into()
}
async fn bounded<T>(
    deadline: &RegistryDeadline,
    work: impl Future<Output = Result<T, NecessaryWriteError>>,
) -> Result<T, NecessaryWriteError> {
    tokio::select! {biased;()=deadline.cancelled()=>Err(unavailable()),value=tokio::time::timeout_at(deadline.deadline(),work)=>value.map_err(|_|unavailable())?}
}
const fn confirmation() -> SqlConfirmation {
    SqlConfirmation {
        sql_operation: None,
        commit: CommitState::Confirmed,
        lease_close: LeaseCloseState::Confirmed,
        close_report: CloseReportState::Confirmed,
        task_join: SqlTaskJoinState::Joined,
    }
}
async fn exact<T: ApprovalRow>(
    c: &mut SqliteConnection,
    row: &T,
) -> Result<T, NecessaryWriteError> {
    let query = format!("SELECT * FROM main.{} WHERE {}=?", T::TABLE, T::PRIMARY);
    let actual = sqlx::query(&query)
        .bind(row.key())
        .fetch_optional(c)
        .await
        .map_err(|_| unavailable())?
        .ok_or(ApprovalRejection::Conflict)?;
    let decoded = T::decode(&actual)?;
    if &decoded != row {
        return Err(ApprovalRejection::Conflict.into());
    }
    Ok(decoded)
}
async fn insert<T: ApprovalRow>(
    c: &mut SqliteConnection,
    row: &T,
) -> Result<T, NecessaryWriteError> {
    let lookup = format!("SELECT * FROM main.{} WHERE {}=?", T::TABLE, T::PRIMARY);
    if let Some(actual) = sqlx::query(&lookup)
        .bind(row.key())
        .fetch_optional(&mut *c)
        .await
        .map_err(|_| unavailable())?
    {
        let actual = T::decode(&actual)?;
        if &actual != row {
            return Err(ApprovalRejection::Conflict.into());
        }
        return Ok(actual);
    }
    row.validate_relationships(c).await?;
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    let fault = test_sql::before(c, test_sql::for_table(T::TABLE).map_err(|_| unavailable())?)
        .await
        .map_err(|_| unavailable())?;
    let result = row.insert().build().execute(&mut *c).await;
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    test_sql::after(c, fault, &result)
        .await
        .map_err(|_| unavailable())?;
    result.map_err(|_| unavailable())?;
    exact(c, row).await
}
async fn list<T: ApprovalRow>(
    c: &mut SqliteConnection,
    approval: ApprovalId,
) -> Result<Vec<T>, NecessaryWriteError> {
    let query = format!(
        "SELECT * FROM main.{} WHERE approval_id=? ORDER BY {}",
        T::TABLE,
        T::PRIMARY
    );
    sqlx::query(&query)
        .bind(approval.encode())
        .fetch_all(c)
        .await
        .map_err(|_| unavailable())?
        .iter()
        .map(T::decode)
        .collect()
}
async fn current_client(
    c: &mut SqliteConnection,
    actor: crate::domain::EntityId,
    generation: i64,
) -> Result<(), NecessaryWriteError> {
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM main.clients WHERE client_id=? AND status='trusted' AND auth_generation=?").bind(actor.encode()).bind(generation).fetch_one(c).await.map_err(|_|unavailable())?;
    if count != 1 {
        return Err(ApprovalRejection::InvalidActor.into());
    }
    Ok(())
}
impl SqliteNativeDispatch {
    async fn approval_begin(
        &self,
        d: &RegistryDeadline,
        caches: &[&ApprovalWriteCache],
    ) -> Result<RegistryConnection, NecessaryWriteError> {
        #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
        if let Some(context) = test_control::current_original_context() {
            let point = match context.tag.kind {
                ApprovalSqlOperation::Resolve => ApprovalCut::ReconcileBeforeBegin,
                ApprovalSqlOperation::Observe => ApprovalCut::RequestBeforeBegin,
                ApprovalSqlOperation::Register => ApprovalCut::RegisterBeforeBegin,
                ApprovalSqlOperation::Decide => ApprovalCut::DecideBeforeBegin,
                ApprovalSqlOperation::Arm => ApprovalCut::ReplyArmBeforeBegin,
                ApprovalSqlOperation::Terminal => ApprovalCut::ReplyTerminalBeforeBegin,
                ApprovalSqlOperation::Invalidate => ApprovalCut::InvalidationBeforeBegin,
                _ => ApprovalCut::ReconcileBeforeBegin,
            };
            bounded(d, async {
                context
                    .controller
                    .hit(point, context.tag, ApprovalCutState::before_begin())
                    .await
                    .map_err(|_| unavailable())?;
                Ok(())
            })
            .await?;
        }
        for cache in caches {
            if !cache
                .confirm_all_cleanup(&self.registry.owner_scope(), d.deadline())
                .await?
            {
                return Err(NecessaryWriteError::SqlCloseUnconfirmed {
                    commit_confirmed: cache.close_state()?.1,
                });
            }
        }
        let mut original = bounded(d, async {
            self.registry
                .detached_owned_connection()
                .await
                .map_err(|_| unavailable())
        })
        .await?;
        #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
        original.attach_approval_test_context(test_control::current_original_context());
        // The actual original connection stays outside cancellable statement futures. Every
        // post-acquisition failure hands it to the same owner and retains its real close receipt.
        let begun = bounded(d, async {
            let c = original.connection().map_err(|_| unavailable())?;
            sqlx::query("BEGIN IMMEDIATE")
                .execute(&mut *c)
                .await
                .map_err(|_| unavailable())?;
            super::schema::validate_approval_necessary(c)
                .await
                .map_err(|_| unavailable())?;
            Ok(())
        })
        .await;
        if let Err(error) = begun {
            let _ = bounded(d, async {
                sqlx::query("ROLLBACK")
                    .execute(original.connection().map_err(|_| unavailable())?)
                    .await
                    .map_err(|_| unavailable())?;
                Ok(())
            })
            .await;
            let waiter = original.begin_owned_close().map_err(|_| unavailable())?;
            for cache in caches {
                cache.retain_ancillary_close(Arc::new(waiter.receipt()))?;
            }
            let _ = bounded(d, async { Ok(waiter.wait_report().await) }).await;
            return Err(error);
        }
        Ok(original)
    }
    async fn approval_finish<T>(
        &self,
        mut c: RegistryConnection,
        result: Result<T, NecessaryWriteError>,
        caches: &[&ApprovalWriteCache],
        d: &RegistryDeadline,
    ) -> Result<T, NecessaryWriteError> {
        if let Err(NecessaryWriteError::Rejected(
            rejection @ (ApprovalRejection::InvalidActor | ApprovalRejection::UnassociatedTarget),
        )) = result
        {
            for cache in caches {
                cache.note_final_rejection(rejection)?;
            }
        }
        let committed = if result.is_ok() {
            bounded(d, async {
                let connection = c.connection().map_err(|_| unavailable())?;
                #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
                let fault = test_sql::before(connection, test_control::ApprovalStatement::Commit)
                    .await
                    .map_err(|_| NecessaryWriteError::CommitUnconfirmed)?;
                let result = sqlx::query("COMMIT").execute(&mut *connection).await;
                #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
                test_sql::after(connection, fault, &result)
                    .await
                    .map_err(|_| NecessaryWriteError::CommitUnconfirmed)?;
                result.map_err(|_| NecessaryWriteError::CommitUnconfirmed)?;
                Ok(())
            })
            .await
            .is_ok()
        } else {
            let _ = bounded(d, async {
                sqlx::query("ROLLBACK")
                    .execute(c.connection().map_err(|_| unavailable())?)
                    .await
                    .map_err(|_| unavailable())?;
                Ok(())
            })
            .await;
            false
        };
        #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
        c.approval_commit_for_test(committed);
        #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
        let report_lost = if committed && !caches.is_empty() {
            if let Some(context) = test_control::current_original_context() {
                if context.tag.kind == ApprovalSqlOperation::Decide {
                    bounded(d, async {
                        context
                            .controller
                            .hit(
                                ApprovalCut::DecideAfterCommit,
                                context.tag,
                                ApprovalCutState {
                                    lease_attached: true,
                                    begin_started: true,
                                    commit: CommitState::Confirmed,
                                    actual_close_ack_observed: false,
                                    source_job_registered: context.source_job_registered,
                                },
                            )
                            .await
                            .map(|effect| effect.lose_commit_report)
                            .map_err(|_| unavailable())
                    })
                    .await
                    .unwrap_or(true)
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };
        let waiter =
            c.begin_owned_close()
                .map_err(|_| NecessaryWriteError::SqlCloseUnconfirmed {
                    commit_confirmed: committed,
                })?;
        for cache in caches {
            cache.retain_actual_close(committed, Arc::new(waiter.receipt()))?;
        }
        let report = bounded(d, async { Ok(waiter.wait_report().await) }).await;
        for cache in caches {
            cache.observe_report(if matches!(report, Ok(CloseOutcome::Confirmed)) {
                CloseReportState::Confirmed
            } else {
                CloseReportState::Unconfirmed
            })?;
        }
        if !matches!(report, Ok(CloseOutcome::Confirmed)) {
            return Err(NecessaryWriteError::SqlCloseUnconfirmed {
                commit_confirmed: committed,
            });
        }
        for cache in caches {
            if committed
                && !cache
                    .confirm_actual_close(&self.registry.owner_scope(), d.deadline())
                    .await?
            {
                return Err(NecessaryWriteError::SqlCloseUnconfirmed {
                    commit_confirmed: true,
                });
            }
        }
        #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
        if report_lost {
            return Err(NecessaryWriteError::CommitUnconfirmed);
        }
        match result {
            Ok(value) if committed => Ok(value),
            Ok(_) => Err(NecessaryWriteError::CommitUnconfirmed),
            Err(error) => Err(error),
        }
    }
    async fn approval_save<T: ApprovalRow>(
        &self,
        row: &T,
        fingerprint: [u8; 32],
        cache: &ApprovalWriteCache,
        d: &RegistryDeadline,
    ) -> Result<StoredApproval<T>, NecessaryWriteError> {
        let _sql_owner = cache.sql_owner().lock().await;
        let (cached, known_commit) = cache.close_state()?;
        if cached && known_commit {
            if !cache
                .confirm_actual_close(&self.registry.owner_scope(), d.deadline())
                .await?
            {
                let committed = cache.close_state()?.1;
                return Err(NecessaryWriteError::SqlCloseUnconfirmed {
                    commit_confirmed: committed,
                });
            }
            let mut c = self.approval_begin(d, &[cache]).await?;
            let result = bounded(d, exact(c.connection().map_err(|_| unavailable())?, row)).await;
            return self
                .approval_finish(c, result, &[], d)
                .await
                .and_then(|record| {
                    Ok(StoredApproval::from_exact_owner_result(
                        record,
                        cache.confirmation()?,
                        fingerprint,
                    ))
                });
        }
        if cached {
            cache
                .prepare_missing_repair(&self.registry.owner_scope(), d.deadline())
                .await?;
        }
        let mut c = self.approval_begin(d, &[cache]).await?;
        let result = bounded(d, insert(c.connection().map_err(|_| unavailable())?, row)).await;
        self.approval_finish(c, result, &[cache], d)
            .await
            .and_then(|record| {
                Ok(StoredApproval::from_exact_owner_result(
                    record,
                    cache.confirmation()?,
                    fingerprint,
                ))
            })
    }
}
impl NativeApprovalRepository for SqliteNativeDispatch {
    fn resolve_approval_target<'a, P: NativeApprovalPort>(
        &'a self,
        original: &'a OriginalRequestProbe<P>,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<Option<crate::domain::EntityId>, NecessaryWriteError>> {
        Box::pin(approval_operation(
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            self.registry.approval_test_controller(),
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            ApprovalOperationTag {
                operation: original.metadata().approval.entity(),
                kind: ApprovalSqlOperation::Resolve,
                instance: Some(original.instance_for_approval_test()),
                approval: Some(original.metadata().approval),
                registration: None,
                response: None,
            },
            async move { self.resolve_original_approval_target(original, d).await },
        ))
    }

    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    fn stop_approval_test_source(&self, instance: crate::domain::EntityId) {
        self.registry
            .approval_test_controller()
            .stop_source(instance);
    }
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    fn approval_native_test_cut<'a, P: NativeApprovalPort>(
        &'a self,
        proof: &'a ApprovalAdmissionProbe<P>,
        point: ApprovalCut,
        response: ApprovalReplyId,
        until: tokio::time::Instant,
    ) -> BoxFuture<'a, Result<(), crate::domain::native_dispatch::DispatchError>> {
        Box::pin(async move {
            use crate::domain::native_dispatch::DispatchError;
            proof
                .verify_owner(&self.registry.owner_scope())
                .map_err(|_| DispatchError::InvalidScope)?;
            if !matches!(
                point,
                ApprovalCut::BeforeSelectedReserve | ApprovalCut::BeforeSelectedEnqueue
            ) {
                return Err(DispatchError::InvalidScope);
            }
            let actual = proof
                .write_cache()
                .confirmation()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?;
            let context = ApprovalCutState {
                lease_attached: false,
                begin_started: false,
                commit: actual.commit,
                actual_close_ack_observed: false,
                source_job_registered:
                    crate::application::native_dispatch::owned_job_registered_for_approval_test(),
            };
            let tag = ApprovalOperationTag {
                operation: response.entity(),
                kind: ApprovalSqlOperation::Arm,
                instance: Some(proof.instance_for_approval_test()),
                approval: Some(proof.metadata().approval),
                registration: Some(proof.metadata().registration),
                response: Some(response),
            };
            tokio::time::timeout_at(
                until,
                self.registry
                    .approval_test_controller()
                    .hit(point, tag, context),
            )
            .await
            .map_err(|_| DispatchError::RequiredStateUnavailable)?
            .map_err(|_| DispatchError::Stopped)?;
            Ok(())
        })
    }
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    fn approval_test_controller(&self) -> Arc<test_control::ApprovalTestController> {
        self.registry.approval_test_controller()
    }
    fn approval_owner_scope(&self) -> crate::application::request_registry::RegistryOwnerScope {
        self.registry.owner_scope()
    }
    fn observe_approval<'a, P: NativeApprovalPort>(
        &'a self,
        probe: &'a OriginalRequestProbe<P>,
        association: Option<&'a VerifiedApprovalAssociation<P>>,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredApprovalObservation, NecessaryWriteError>> {
        Box::pin(approval_operation(
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            self.registry.approval_test_controller(),
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            ApprovalOperationTag {
                operation: probe.metadata().approval.entity(),
                kind: ApprovalSqlOperation::Observe,
                instance: Some(probe.instance_for_approval_test()),
                approval: Some(probe.metadata().approval),
                registration: None,
                response: None,
            },
            async move {
                probe.verify_owner(&self.registry.owner_scope())?;
                if let Some(association) = association {
                    association.verify_owner(&self.registry.owner_scope())?;
                    if association.metadata().approval != probe.metadata().approval {
                        return Err(ApprovalRejection::Conflict.into());
                    }
                }
                if let Some(association) = association
                    && association.metadata().kind == StoredAssociationKind::RegisteredSession
                {
                    return self
                        .observe_registered_approval(probe, association, d)
                        .await;
                }
                if matches!(probe.write_cache().close_state()?, (true, true)) {
                    let request = self
                        .approval_save(
                            probe.metadata(),
                            *probe.fingerprint(),
                            probe.write_cache(),
                            d,
                        )
                        .await?;
                    let association = if let Some(association) = association {
                        Some(
                            self.approval_save(
                                association.metadata(),
                                *association.fingerprint(),
                                association.write_cache(),
                                d,
                            )
                            .await?,
                        )
                    } else {
                        None
                    };
                    return Ok(StoredApprovalObservation {
                        request,
                        association,
                    });
                }
                let _request_owner = probe.write_cache().sql_owner().lock().await;
                let _association_owner = if let Some(association) = association {
                    Some(association.write_cache().sql_owner().lock().await)
                } else {
                    None
                };
                if probe.write_cache().close_state()?.0 {
                    probe
                        .write_cache()
                        .prepare_missing_repair(&self.registry.owner_scope(), d.deadline())
                        .await?;
                }
                if let Some(association) = association
                    && association.write_cache().close_state()?.0
                {
                    association
                        .write_cache()
                        .prepare_missing_repair(&self.registry.owner_scope(), d.deadline())
                        .await?;
                }
                let mut c = self.approval_begin(d, &[probe.write_cache()]).await?;
                let result = bounded(d, async {
                    let c = c.connection().map_err(|_| unavailable())?;
                    let request = insert(c, probe.metadata()).await?;
                    let associated = if let Some(association) = association {
                        Some(insert(c, association.metadata()).await?)
                    } else {
                        None
                    };
                    Ok((request, associated))
                })
                .await;
                let mut caches = vec![probe.write_cache()];
                if let Some(association) = association {
                    caches.push(association.write_cache());
                }
                let (request, associated) = self.approval_finish(c, result, &caches, d).await?;
                let request = StoredApproval::from_exact_owner_result(
                    request,
                    probe.write_cache().confirmation()?,
                    *probe.fingerprint(),
                );
                let association = match (association, associated) {
                    (Some(proof), Some(record)) => Some(StoredApproval::from_exact_owner_result(
                        record,
                        proof.write_cache().confirmation()?,
                        *proof.fingerprint(),
                    )),
                    (None, None) => None,
                    _ => return Err(ApprovalRejection::Conflict.into()),
                };
                Ok(StoredApprovalObservation {
                    request,
                    association,
                })
            },
        ))
    }

    fn register_approval<'a, P: NativeApprovalPort>(
        &'a self,
        proof: &'a RegisteredApprovalProbe<P>,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredApprovalCommand, NecessaryWriteError>> {
        Box::pin(approval_operation(
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            self.registry.approval_test_controller(),
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            ApprovalOperationTag {
                operation: proof.metadata().registration.entity(),
                kind: ApprovalSqlOperation::Register,
                instance: Some(proof.instance_for_approval_test()),
                approval: Some(proof.metadata().approval),
                registration: Some(proof.metadata().registration),
                response: None,
            },
            async move {
                proof.verify_owner(&self.registry.owner_scope())?;
                if let Some(reason) = proof.write_cache().final_rejection()? {
                    return Err(reason.into());
                }
                let (cached, known) = proof.write_cache().close_state()?;
                if cached && known {
                    return self
                        .approval_save(
                            proof.metadata(),
                            *proof.fingerprint(),
                            proof.write_cache(),
                            d,
                        )
                        .await;
                }
                let _owner = proof.write_cache().sql_owner().lock().await;
                if proof.write_cache().close_state()?.0 {
                    proof
                        .write_cache()
                        .prepare_missing_repair(&self.registry.owner_scope(), d.deadline())
                        .await?;
                }
                let mut c = self.approval_begin(d, &[proof.write_cache()]).await?;
                let expected = proof.metadata();
                let native = proof.resolution_native(&self.registry.owner_scope())?;
                let result=bounded(d,async{let c=c.connection().map_err(|_|unavailable())?;
            let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_approval_commands WHERE registration_id=?)").bind(expected.registration.encode()).fetch_one(&mut *c).await.map_err(|_|unavailable())?;
            if exists{return exact(c,expected).await;}
            if association::lookup(c,native.source().0,native.native_id()).await?!=Some(expected.target){return Err(ApprovalRejection::UnassociatedTarget.into());}
            current_client(c,expected.actor,expected.registration_auth_generation).await?;
            insert(c,expected).await
        }).await;
                self.approval_finish(c, result, &[proof.write_cache()], d)
                    .await
                    .and_then(|record| {
                        Ok(StoredApproval::from_exact_owner_result(
                            record,
                            proof.write_cache().confirmation()?,
                            *proof.fingerprint(),
                        ))
                    })
            },
        ))
    }
    fn decide_approval<'a, P: NativeApprovalPort>(
        &'a self,
        proof: &'a ApprovalAdmissionProbe<P>,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredApprovalDecision, NecessaryWriteError>> {
        Box::pin(approval_operation(
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            self.registry.approval_test_controller(),
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            ApprovalOperationTag {
                operation: proof.metadata().operation,
                kind: ApprovalSqlOperation::Decide,
                instance: Some(proof.instance_for_approval_test()),
                approval: Some(proof.metadata().approval),
                registration: Some(proof.metadata().registration),
                response: None,
            },
            async move {
                proof.verify_owner(&self.registry.owner_scope())?;
                let expected = proof.metadata();
                if let Some(reason) = proof.write_cache().final_rejection()? {
                    return Err(reason.into());
                }
                let (cached, known_commit) = proof.write_cache().close_state()?;
                if cached && known_commit {
                    return self
                        .approval_save(expected, *proof.fingerprint(), proof.write_cache(), d)
                        .await;
                }
                let _sql_owner = proof.write_cache().sql_owner().lock().await;
                if proof.write_cache().close_state()?.0 {
                    proof
                        .write_cache()
                        .prepare_missing_repair(&self.registry.owner_scope(), d.deadline())
                        .await?;
                }
                let mut c = self.approval_begin(d, &[proof.write_cache()]).await?;
                let result=bounded(d,async{let c=c.connection().map_err(|_|unavailable())?;
 let existing=list::<ApprovalDecisionRecord>(c,expected.approval).await?;
 if let Some(other)=existing.first(){if other!=expected{return Err(ApprovalRejection::AlreadyDecided.into());}}else{current_client(c,expected.actor,expected.admission_auth_generation).await?;let invalid:i64=sqlx::query_scalar("SELECT count(*) FROM main.node_approval_invalidations WHERE approval_id=?").bind(expected.approval.encode()).fetch_one(&mut *c).await.map_err(|_|unavailable())?;if invalid!=0{return Err(ApprovalRejection::InvalidRequest.into());}}
 let registered=proof.original_registration_metadata()?;
 let commands=list::<ApprovalCommandRecord>(c,expected.approval).await?;
 let actual=commands.iter().find(|row|row.registration==expected.registration).ok_or(ApprovalRejection::Conflict)?;
 let mut immutable=actual.clone();immutable.state=registered.state;immutable.operation=registered.operation;immutable.admission_auth_generation=registered.admission_auth_generation;immutable.admitted_at_ms=registered.admitted_at_ms;immutable.rejection=registered.rejection;
 if immutable!=registered || registered.actor!=expected.actor || registered.option_digest!=expected.option_digest{return Err(ApprovalRejection::Conflict.into());}
 #[cfg(all(test,feature="approval-owner-acceptance-fixtures"))]
 let fault=test_sql::before(c,test_control::ApprovalStatement::AcceptCommand).await.map_err(|_|unavailable())?;
 let affected_result=sqlx::query("UPDATE main.node_approval_commands SET state='accepted',operation_id=?,admission_auth_generation=?,admitted_at_ms=? WHERE registration_id=? AND approval_id=? AND actor_id=? AND option_digest=? AND state='registered'")
 .bind(expected.operation.encode()).bind(expected.admission_auth_generation).bind(expected.accepted_at_ms).bind(expected.registration.encode()).bind(expected.approval.encode()).bind(expected.actor.encode()).bind(expected.option_digest.to_vec()).execute(&mut *c).await;
 #[cfg(all(test,feature="approval-owner-acceptance-fixtures"))]
 test_sql::after(c,fault,&affected_result).await.map_err(|_|unavailable())?;
 let affected=affected_result.map_err(|_|unavailable())?.rows_affected();
 if affected!=1 && existing.is_empty(){return Err(ApprovalRejection::Conflict.into());}
 insert(c,expected).await}).await;
                self.approval_finish(c, result, &[proof.write_cache()], d)
                    .await
                    .and_then(|record| {
                        Ok(StoredApproval::from_exact_owner_result(
                            record,
                            proof.write_cache().confirmation()?,
                            *proof.fingerprint(),
                        ))
                    })
            },
        ))
    }
    fn arm_approval_reply<'a, P: NativeApprovalPort>(
        &'a self,
        proof: &'a ApprovalReplyArmProof<P>,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredApprovalAttempt, NecessaryWriteError>> {
        Box::pin(approval_operation(
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            self.registry.approval_test_controller(),
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            ApprovalOperationTag {
                operation: proof.metadata().response.entity(),
                kind: ApprovalSqlOperation::Arm,
                instance: Some(proof.instance_for_approval_test()),
                approval: Some(proof.metadata().approval),
                registration: None,
                response: Some(proof.metadata().response),
            },
            async move {
                proof.verify_owner(&self.registry.owner_scope())?;
                self.approval_save(
                    proof.metadata(),
                    *proof.fingerprint(),
                    proof.write_cache(),
                    d,
                )
                .await
            },
        ))
    }
    fn record_approval_reply<'a, P: NativeApprovalPort>(
        &'a self,
        proof: &'a ApprovalReplyTerminalProof<P>,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredApprovalTerminal, NecessaryWriteError>> {
        Box::pin(approval_operation(
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            self.registry.approval_test_controller(),
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            ApprovalOperationTag {
                operation: proof.metadata().response.entity(),
                kind: ApprovalSqlOperation::Terminal,
                instance: Some(proof.instance_for_approval_test()),
                approval: Some(proof.metadata().approval),
                registration: None,
                response: Some(proof.metadata().response),
            },
            async move {
                proof.verify_owner(&self.registry.owner_scope())?;
                self.approval_save(
                    proof.metadata(),
                    *proof.fingerprint(),
                    proof.write_cache(),
                    d,
                )
                .await
            },
        ))
    }
    fn invalidate_approval<'a, P: NativeApprovalPort>(
        &'a self,
        proof: &'a ApprovalInvalidationProof<P>,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredApprovalInvalidation, NecessaryWriteError>> {
        Box::pin(approval_operation(
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            self.registry.approval_test_controller(),
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            ApprovalOperationTag {
                operation: proof.metadata().id,
                kind: ApprovalSqlOperation::Invalidate,
                instance: Some(proof.instance_for_approval_test()),
                approval: Some(proof.metadata().approval),
                registration: None,
                response: None,
            },
            async move {
                proof.verify_owner(&self.registry.owner_scope())?;
                self.approval_save(
                    proof.metadata(),
                    *proof.fingerprint(),
                    proof.write_cache(),
                    d,
                )
                .await
            },
        ))
    }
    fn inspect_approval<'a, P: NativeApprovalPort>(
        &'a self,
        original: &'a OriginalRequestProbe<P>,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<ApprovalStoredInspection, NecessaryWriteError>> {
        Box::pin(approval_operation(
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            self.registry.approval_test_controller(),
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            ApprovalOperationTag {
                operation: original.metadata().approval.entity(),
                kind: ApprovalSqlOperation::Recheck,
                instance: Some(original.instance_for_approval_test()),
                approval: Some(original.metadata().approval),
                registration: None,
                response: None,
            },
            async move {
                original.verify_owner(&self.registry.owner_scope())?;
                let approval = original.metadata().approval;
                let mut c = self.approval_begin(d, &[original.write_cache()]).await?;
                let result = bounded(d, async {
                    let c = c.connection().map_err(|_| unavailable())?;
                    let requests = list::<ApprovalRequestRecord>(c, approval).await?;
                    let associations = list::<ApprovalAssociationRecord>(c, approval).await?;
                    let decisions = list::<ApprovalDecisionRecord>(c, approval).await?;
                    Ok(ApprovalStoredInspection {
                        request: requests.into_iter().next(),
                        association: associations.into_iter().next(),
                        commands: list(c, approval).await?,
                        decision: decisions.into_iter().next(),
                        attempts: list(c, approval).await?,
                        terminals: list(c, approval).await?,
                        invalidations: list(c, approval).await?,
                        read_confirmation: confirmation(),
                    })
                })
                .await;
                self.approval_finish(c, result, &[], d).await
            },
        ))
    }
}
