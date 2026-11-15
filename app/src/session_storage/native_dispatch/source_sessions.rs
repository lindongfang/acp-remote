//! Source-operation facts on the original physical registry owner, with original close receipts.
#![allow(
    clippy::items_after_statements,
    clippy::significant_drop_in_scrutinee,
    clippy::too_many_lines,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    Arc, BoxFuture, EntityId, Future, ProtocolVersion, RegistryConnection, RegistryDeadline, Row,
    RpcScalar, RuntimeEpoch, SqliteNativeDispatch, SqliteRow, TransportFact, TransportReason,
};
#[cfg(test)]
use crate::application::native_dispatch::source_sessions::{
    SourceSqlOriginalSnapshot, SourceSqlTestFault,
};
use crate::{
    application::{
        native_dispatch::source_sessions::{
            SourceAdmissionProof, SourceAdmissionResult, SourceReaderProof, SourceReconcileProof,
            SourceRecoveryProof, SourceRegistrationProof, SourceSessionRepository,
            SourceSqlReceipt, SourceStoredOperation, SourceUpdateView, SourceWriteProof,
            StoredNativeClaimBarrier, source_intent_digest,
        },
        request_registry::RegistryOwnerScope,
    },
    domain::{
        native::NativeSessionKey,
        native_catalog::{CleanupKnowledge, ProfileKey, TransactionKnowledge},
        source_sessions::{
            SourceAckState, SourceFailureClass, SourceLaunchSelection, SourceResponseFact,
            SourceResponseKind, SourceSessionAction, SourceSessionError, SourceSessionFact,
            SourceSessionFailure, SourceSessionOperationId, SourceSessionOutcome,
            SourceSessionRegistration, SourceSessionSqlFailure, SourceSessionState,
            SourceSqlObservation, SourceSqlRole,
        },
    },
    session_storage::registry_connection_owner::{
        CloseOutcome, OriginalCleanupObservation, OriginalSqlCloseReceipt,
    },
};
use std::sync::Mutex;

struct SqlState {
    observation: Option<SourceSqlObservation>,
    close: Option<OriginalSqlCloseReceipt>,
}
pub struct OriginalSourceSqlOperation {
    scope: RegistryOwnerScope,
    role: SourceSqlRole,
    id: Option<SourceSessionOperationId>,
    state: Mutex<SqlState>,
    #[cfg(test)]
    fault: Mutex<Option<SourceSqlTestFault>>,
    #[cfg(test)]
    started: std::sync::atomic::AtomicUsize,
}
impl OriginalSourceSqlOperation {
    fn failure(&self, kind: SourceSessionError) -> SourceSessionFailure {
        SourceSessionFailure::NecessarySql(SourceSessionSqlFailure {
            kind,
            operation: self.id,
            observation: self.observe(),
        })
    }
    fn update(&self, transaction: TransactionKnowledge, cleanup: CleanupKnowledge) {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .observation = Some(SourceSqlObservation {
            role: self.role,
            transaction,
            cleanup,
        });
    }
    fn cleanup(&self, cleanup: CleanupKnowledge) {
        if let Some(observation) = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .observation
            .as_mut()
        {
            observation.cleanup = cleanup;
        }
    }
    fn verify(
        &self,
        scope: &RegistryOwnerScope,
        id: Option<SourceSessionOperationId>,
    ) -> Result<(), SourceSessionFailure> {
        if !self.scope.matches(scope) || self.id != id {
            return Err(self.failure(SourceSessionError::InvalidScope));
        }
        scope
            .require_open()
            .map_err(|_| self.failure(SourceSessionError::NodeStopping))
    }
}
impl SourceSqlReceipt for OriginalSourceSqlOperation {
    fn role(&self) -> SourceSqlRole {
        self.role
    }
    #[cfg(test)]
    fn arm_fault_for_test(&self, fault: SourceSqlTestFault) {
        *self
            .fault
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(fault);
    }
    #[cfg(test)]
    fn sql_started_for_test(&self) -> usize {
        self.started.load(std::sync::atomic::Ordering::SeqCst)
    }
    #[cfg(test)]
    fn original_cleanup_for_test(&self) -> Option<SourceSqlOriginalSnapshot> {
        self.state.lock().ok()?.close.as_ref().map(|receipt| {
            let snapshot = receipt.snapshot_for_test();
            SourceSqlOriginalSnapshot {
                owner: snapshot.owner.value(),
                close_slot: snapshot.operation.value(),
                actual_ack: snapshot.sql_actual,
                close_task_join: snapshot.sql_task_join,
                worker_registered: snapshot.worker.is_some(),
                worker_joined: snapshot.worker.as_ref().and_then(|worker| {
                    if worker.unknown {
                        None
                    } else {
                        Some(matches!(
                            worker.handle,
                            sqlx::sqlite::WorkerHandleState::Joined
                        ))
                    }
                }),
                app_join: snapshot.app_join,
            }
        })
    }
    fn observe(&self) -> Option<SourceSqlObservation> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .observation
    }
    fn confirm_original<'a>(
        &'a self,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<Option<SourceSqlObservation>, SourceSessionFailure>> {
        Box::pin(async move {
            let receipt = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .close
                .clone();
            let Some(receipt) = receipt else {
                return if self.observe().is_none() {
                    Ok(None)
                } else {
                    Err(self.failure(SourceSessionError::SqlCloseUnconfirmed))
                };
            };
            let observation = receipt.confirm_actual_cleanup(d.deadline()).await;
            match observation {
                OriginalCleanupObservation::ConfirmedJoined => {
                    self.cleanup(CleanupKnowledge::ConfirmedOriginal);
                }
                OriginalCleanupObservation::Pending => {
                    self.cleanup(CleanupKnowledge::PendingOriginal);
                }
                OriginalCleanupObservation::UnconfirmedJoined => {
                    self.cleanup(CleanupKnowledge::UnconfirmedOriginal);
                }
            }
            if observation != OriginalCleanupObservation::ConfirmedJoined {
                return Err(self.failure(SourceSessionError::SqlCloseUnconfirmed));
            }
            Ok(self.observe())
        })
    }
}
async fn wait_sql<T>(
    d: &RegistryDeadline,
    future: impl Future<Output = Result<T, SourceSessionError>>,
) -> Result<T, SourceSessionError> {
    tokio::select! { biased; ()=d.cancelled()=>Err(SourceSessionError::StorageUnavailable), result=tokio::time::timeout_at(d.deadline(),future)=>result.map_err(|_| SourceSessionError::StorageUnavailable)? }
}
impl SqliteNativeDispatch {
    async fn begin_source_sql(
        &self,
        slot: &OriginalSourceSqlOperation,
        write: bool,
        d: &RegistryDeadline,
    ) -> Result<RegistryConnection, SourceSessionFailure> {
        #[cfg(test)]
        slot.started
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut c = wait_sql(d, async {
            self.registry
                .detached_owned_connection()
                .await
                .map_err(|_| SourceSessionError::StorageUnavailable)
        })
        .await
        .map_err(|e| slot.failure(e))?;
        slot.update(
            TransactionKnowledge::NoTransaction,
            CleanupKnowledge::PendingOriginal,
        );
        let result = wait_sql(d, async {
            sqlx::query(if write { "BEGIN IMMEDIATE" } else { "BEGIN" })
                .execute(
                    c.connection()
                        .map_err(|_| SourceSessionError::StorageUnavailable)?,
                )
                .await
                .map_err(|_| SourceSessionError::StorageUnavailable)?;
            Ok(())
        })
        .await;
        if let Err(error) = result {
            return self.finish_source_sql(c, Err(error), slot, d).await;
        }
        slot.update(
            if write {
                TransactionKnowledge::BegunWrite
            } else {
                TransactionKnowledge::BegunRead
            },
            CleanupKnowledge::PendingOriginal,
        );
        Ok(c)
    }
    async fn finish_source_sql<T>(
        &self,
        mut c: RegistryConnection,
        result: Result<T, SourceSessionError>,
        slot: &OriginalSourceSqlOperation,
        d: &RegistryDeadline,
    ) -> Result<T, SourceSessionFailure> {
        #[cfg(test)]
        let test_fault = slot
            .fault
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        #[cfg(test)]
        let result = match (result, test_fault) {
            (Ok(_), Some(SourceSqlTestFault::StatementError)) => {
                Err(SourceSessionError::StorageUnavailable)
            }
            (Ok(value), Some(SourceSqlTestFault::DeferredCommit)) => {
                let fk_arm = wait_sql(d, async {
                    let conn = c
                        .connection()
                        .map_err(|_| SourceSessionError::StorageUnavailable)?;
                    sqlx::query("PRAGMA defer_foreign_keys = ON")
                        .execute(&mut *conn)
                        .await
                        .map_err(|_| SourceSessionError::StorageUnavailable)?;
                    sqlx::query(
                        "UPDATE main.node_source_session_operations SET source_instance_id='AAAAAAAAAAAAAAAAAAAAAA', connection_epoch='AAAAAAAAAAAAAAAAAAAAAA' WHERE operation_id=?",
                    )
                    .bind(
                        slot.id
                            .map(SourceSessionOperationId::encode)
                            .unwrap_or_default(),
                    )
                    .execute(&mut *conn)
                    .await
                    .map_err(|_| SourceSessionError::StorageUnavailable)?;
                    Ok(())
                })
                .await;
                fk_arm.map(|()| value)
            }
            (other, _) => other,
        };
        let result = match result {
            Ok(value) => {
                slot.update(
                    TransactionKnowledge::CommitPolling,
                    CleanupKnowledge::PendingOriginal,
                );
                enum CommitOutcome {
                    Confirmed,
                    RejectedRollback,
                    Unknown,
                }
                let committed = wait_sql(d, async {
                    let conn = c
                        .connection()
                        .map_err(|_| SourceSessionError::CommitUnconfirmed)?;
                    match sqlx::query("COMMIT").execute(conn).await {
                        Ok(_) => Ok(CommitOutcome::Confirmed),
                        Err(sqlx::Error::Database(_)) => Ok(CommitOutcome::RejectedRollback),
                        Err(_) => Ok(CommitOutcome::Unknown),
                    }
                })
                .await;
                match committed {
                    Ok(CommitOutcome::Confirmed) => {
                        slot.update(
                            TransactionKnowledge::CommitConfirmed,
                            CleanupKnowledge::PendingOriginal,
                        );
                        Ok(value)
                    }
                    Ok(CommitOutcome::RejectedRollback) => {
                        let _ = wait_sql(d, async {
                            if let Ok(conn) = c.connection() {
                                let _ = sqlx::query("ROLLBACK").execute(conn).await;
                            }
                            Ok(())
                        })
                        .await;
                        slot.update(
                            TransactionKnowledge::RollbackConfirmed,
                            CleanupKnowledge::PendingOriginal,
                        );
                        Err(SourceSessionError::StorageUnavailable)
                    }
                    Ok(CommitOutcome::Unknown) | Err(_) => {
                        slot.update(
                            TransactionKnowledge::EffectUnknown,
                            CleanupKnowledge::PendingOriginal,
                        );
                        Err(SourceSessionError::CommitUnconfirmed)
                    }
                }
            }
            Err(error) => {
                let rollback = wait_sql(d, async {
                    sqlx::query("ROLLBACK")
                        .execute(
                            c.connection()
                                .map_err(|_| SourceSessionError::StorageUnavailable)?,
                        )
                        .await
                        .map_err(|_| SourceSessionError::StorageUnavailable)?;
                    Ok(())
                })
                .await;
                slot.update(
                    if rollback.is_ok() {
                        TransactionKnowledge::RollbackConfirmed
                    } else {
                        TransactionKnowledge::EffectUnknown
                    },
                    CleanupKnowledge::PendingOriginal,
                );
                Err(error)
            }
        };
        let waiter = c
            .begin_owned_close()
            .map_err(|_| slot.failure(SourceSessionError::SqlCloseUnconfirmed))?;
        slot.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .close = Some(waiter.receipt());
        let report = tokio::time::timeout_at(d.deadline(), waiter.wait_report()).await;
        if !matches!(report, Ok(CloseOutcome::Confirmed)) {
            slot.cleanup(if report.is_err() {
                CleanupKnowledge::PendingOriginal
            } else {
                CleanupKnowledge::UnconfirmedOriginal
            });
            return Err(slot.failure(SourceSessionError::SqlCloseUnconfirmed));
        }
        slot.confirm_original(d).await?;
        result.map_err(|e| slot.failure(e))
    }
}
const READ: &str = "SELECT r.*,o.runtime_epoch,o.admission_auth_generation,o.operation_state,o.outcome,o.source_instance_id,o.connection_epoch,o.protocol_version,o.rpc_id_kind,o.rpc_id_bytes,o.frame_length,o.accepted_length,o.transport_state,o.transport_reason,o.response_kind,o.native_session_id,o.remote_error_code,o.response_sequence,o.response_receipt,o.response_evidence,o.result_bytes,o.ack_state,o.failure_class FROM main.node_source_session_registrations r LEFT JOIN main.node_source_session_operations o ON o.operation_id=r.operation_id WHERE r.operation_id=?";
fn bad(_: impl std::fmt::Debug) -> SourceSessionError {
    SourceSessionError::CorruptState
}
fn col<T: for<'a> sqlx::Decode<'a, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite>>(
    row: &SqliteRow,
    name: &str,
) -> Result<T, SourceSessionError> {
    row.try_get(name).map_err(bad)
}
fn id(row: &SqliteRow, name: &str) -> Result<EntityId, SourceSessionError> {
    EntityId::decode(&col::<String>(row, name)?).map_err(bad)
}
fn u64_blob(bytes: Vec<u8>) -> Result<u64, SourceSessionError> {
    Ok(u64::from_be_bytes(bytes.try_into().map_err(bad)?))
}
fn rpc(row: &SqliteRow) -> Result<Option<RpcScalar>, SourceSessionError> {
    let kind: Option<String> = col(row, "rpc_id_kind")?;
    let bytes: Option<Vec<u8>> = col(row, "rpc_id_bytes")?;
    match (kind.as_deref(), bytes) {
        (None, None) => Ok(None),
        (Some("integer"), Some(v)) => Ok(Some(RpcScalar::Integer(i64::from_be_bytes(
            v.try_into().map_err(bad)?,
        )))),
        (Some("string"), Some(v)) => Ok(Some(RpcScalar::String(
            String::from_utf8(v).map_err(bad)?.into(),
        ))),
        _ => Err(SourceSessionError::CorruptState),
    }
}
fn reason(value: &str) -> Result<TransportReason, SourceSessionError> {
    Ok(match value {
        "cancelled" => TransportReason::Cancelled,
        "deadline" => TransportReason::Deadline,
        "fence_rejected" => TransportReason::FenceRejected,
        "connection_closed" => TransportReason::ConnectionClosed,
        "abandoned" => TransportReason::Abandoned,
        "io" => TransportReason::Io,
        "invalid_fence" => TransportReason::InvalidFence,
        "connection_lost" => TransportReason::ConnectionLost,
        "restart" => TransportReason::Restart,
        _ => return Err(SourceSessionError::CorruptState),
    })
}
fn decode(row: &SqliteRow) -> Result<SourceStoredOperation, SourceSessionError> {
    let action = match col::<String>(row, "action")?.as_str() {
        "discover" => SourceSessionAction::Discover {
            cursor: col(row, "cursor")?,
        },
        "create" => SourceSessionAction::Create,
        _ => return Err(SourceSessionError::CorruptState),
    };
    let selection = SourceLaunchSelection {
        source: id(row, "source_id")?,
        harness: id(row, "harness_id")?,
        profile: ProfileKey::try_new(
            id(row, "profile_id")?,
            u64_blob(col(row, "profile_revision")?)?,
        )
        .map_err(bad)?,
        workspace: id(row, "workspace_id")?,
        launch_path: col(row, "launch_path")?,
    };
    let digest: [u8; 32] = col::<Vec<u8>>(row, "intent_digest")?
        .try_into()
        .map_err(bad)?;
    if source_intent_digest(&selection, &action)? != digest {
        return Err(SourceSessionError::CorruptState);
    }
    let registration = SourceSessionRegistration {
        id: SourceSessionOperationId::new(id(row, "operation_id")?),
        actor: id(row, "actor_id")?,
        auth_generation: col(row, "register_auth_generation")?,
        selection,
        action,
        intent_digest: digest,
        registered_at_ms: col(row, "registered_at_ms")?,
    };
    let runtime: Option<String> = col(row, "runtime_epoch")?;
    let state = match col::<Option<String>>(row, "operation_state")?.as_deref() {
        None if runtime.is_none() => SourceSessionState::Registered,
        Some("owned") => SourceSessionState::Owned,
        Some("terminal") => SourceSessionState::Terminal,
        Some("interrupted") => SourceSessionState::Interrupted,
        _ => return Err(SourceSessionError::CorruptState),
    };
    let outcome = match col::<Option<String>>(row, "outcome")?.as_deref() {
        None if runtime.is_none() => SourceSessionOutcome::Pending,
        Some("pending") => SourceSessionOutcome::Pending,
        Some("succeeded") => SourceSessionOutcome::Succeeded,
        Some("refused") => SourceSessionOutcome::Refused,
        Some("unknown") => SourceSessionOutcome::Unknown,
        _ => return Err(SourceSessionError::CorruptState),
    };
    let wire = match col::<Option<i64>>(row, "protocol_version")? {
        None => None,
        Some(1) => Some(ProtocolVersion::V1),
        Some(2) => Some(ProtocolVersion::V2),
        _ => return Err(SourceSessionError::CorruptState),
    };
    let frame_length = col::<Option<Vec<u8>>>(row, "frame_length")?
        .map(u64_blob)
        .transpose()?;
    let transport = match col::<Option<String>>(row, "transport_state")?.as_deref() {
        None | Some("reserved" | "armed") => None,
        Some("written") => Some(TransportFact::Written),
        Some("zero") => Some(TransportFact::Zero {
            reason: reason(&col::<String>(row, "transport_reason")?)?,
        }),
        Some("unknown") => Some(TransportFact::Unknown {
            accepted: u64_blob(col(row, "accepted_length")?)?,
            cause: reason(&col::<String>(row, "transport_reason")?)?,
        }),
        _ => return Err(SourceSessionError::CorruptState),
    };
    let response = match col::<Option<String>>(row, "response_kind")?.as_deref() {
        None => None,
        Some(kind) => {
            let kind = match kind {
                "created" => SourceResponseKind::Created,
                "sessions" => SourceResponseKind::Sessions,
                "remote_error" => SourceResponseKind::RemoteError,
                _ => return Err(SourceSessionError::CorruptState),
            };
            let native: Option<String> = col(row, "native_session_id")?;
            let available = native.as_ref().is_some_and(|v| {
                NativeSessionKey::from_config(Some(registration.selection.source), v).is_ok()
            });
            let ack = match col::<String>(row, "ack_state")?.as_str() {
                "pending" => SourceAckState::Pending,
                "confirmed" => SourceAckState::Confirmed,
                "rejected" => SourceAckState::Rejected,
                "unknown" => SourceAckState::Unknown,
                _ => return Err(SourceSessionError::CorruptState),
            };
            Some(SourceResponseFact {
                kind,
                native_session_id: native.map(Arc::from),
                native_key_available: available,
                remote_error_code: col(row, "remote_error_code")?,
                sequence: u64_blob(col(row, "response_sequence")?)?,
                receipt: u64_blob(col(row, "response_receipt")?)?,
                evidence: col::<Vec<u8>>(row, "response_evidence")?
                    .try_into()
                    .map_err(bad)?,
                result_bytes: u64_blob(col(row, "result_bytes")?)?,
                ack,
            })
        }
    };
    let failure = match col::<Option<String>>(row, "failure_class")?.as_deref() {
        None => None,
        Some("configuration") => Some(SourceFailureClass::Configuration),
        Some("protocol") => Some(SourceFailureClass::Protocol),
        Some("source") => Some(SourceFailureClass::Source),
        Some("metadata") => Some(SourceFailureClass::Metadata),
        Some("ownership") => Some(SourceFailureClass::Ownership),
        Some("allocation") => Some(SourceFailureClass::Allocation),
        Some("interrupted") => Some(SourceFailureClass::Interrupted),
        _ => return Err(SourceSessionError::CorruptState),
    };
    let optional_id = |name| -> Result<Option<EntityId>, SourceSessionError> {
        col::<Option<String>>(row, name)?
            .map(|v| EntityId::decode(&v).map_err(bad))
            .transpose()
    };
    let fact = SourceSessionFact {
        registration: registration.metadata(),
        result_available: false,
        state,
        outcome,
        runtime: runtime
            .map(|v| EntityId::decode(&v).map(RuntimeEpoch).map_err(bad))
            .transpose()?,
        admission_auth_generation: col(row, "admission_auth_generation")?,
        source_instance: optional_id("source_instance_id")?,
        connection_epoch: optional_id("connection_epoch")?,
        wire,
        rpc: rpc(row)?,
        frame_length,
        transport,
        response,
        failure,
    };
    Ok(SourceStoredOperation { registration, fact })
}
async fn read(
    c: &mut RegistryConnection,
    id: SourceSessionOperationId,
) -> Result<SourceStoredOperation, SourceSessionError> {
    let row = sqlx::query(READ)
        .bind(id.encode())
        .fetch_optional(
            c.connection()
                .map_err(|_| SourceSessionError::StorageUnavailable)?,
        )
        .await
        .map_err(|_| SourceSessionError::StorageUnavailable)?
        .ok_or(SourceSessionError::UnknownOperation)?;
    decode(&row)
}
fn sql_error(_: sqlx::Error) -> SourceSessionError {
    SourceSessionError::StorageUnavailable
}
fn rpc_parts(rpc: &RpcScalar) -> (&'static str, Vec<u8>) {
    match rpc {
        RpcScalar::Integer(v) => ("integer", v.to_be_bytes().to_vec()),
        RpcScalar::String(v) => ("string", v.as_bytes().to_vec()),
    }
}

impl SourceSessionRepository for SqliteNativeDispatch {
    type SqlOperation = OriginalSourceSqlOperation;
    fn source_owner_scope(&self) -> RegistryOwnerScope {
        self.registry.owner_scope()
    }
    fn source_sql_operation(
        &self,
        role: SourceSqlRole,
        id: Option<SourceSessionOperationId>,
    ) -> Result<Arc<OriginalSourceSqlOperation>, SourceSessionFailure> {
        let scope = self.registry.owner_scope();
        scope
            .require_open()
            .map_err(|_| SourceSessionError::NodeStopping)?;
        Ok(Arc::new(OriginalSourceSqlOperation {
            scope,
            role,
            id,
            state: Mutex::new(SqlState {
                observation: None,
                close: None,
            }),
            #[cfg(test)]
            fault: Mutex::new(None),
            #[cfg(test)]
            started: std::sync::atomic::AtomicUsize::new(0),
        }))
    }
    fn register_source_session<'a>(
        &'a self,
        proof: &'a SourceRegistrationProof,
        slot: Arc<OriginalSourceSqlOperation>,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<SourceStoredOperation, SourceSessionFailure>> {
        Box::pin(async move {
            let r = proof.registration();
            proof.verify_owner_scope(&self.registry.owner_scope())?;
            slot.verify(&self.registry.owner_scope(), Some(r.id))?;
            let mut c = self.begin_source_sql(&slot, true, d).await?;
            let result=wait_sql(d,async{
                let canonical_path:Option<String>=sqlx::query_scalar("SELECT w.canonical_path FROM main.node_launch_profiles p JOIN main.node_catalog_commands c ON c.request_id=p.created_by_request_id AND c.profile_id=p.profile_id AND c.profile_revision=p.revision AND c.state='committed' AND ((c.action='harness.install' AND c.result_kind='harness' AND c.result_id=p.harness_id) OR (c.action='profile.publish' AND c.result_kind='profile' AND c.result_id=p.profile_id)) JOIN main.product_harness_sources hs ON hs.harness_id=p.harness_id AND hs.source_id=p.source_id JOIN main.workspaces w ON w.workspace_id=? WHERE p.profile_id=? AND p.revision=? AND p.source_id=? AND p.harness_id=?")
                    .bind(r.selection.workspace.encode()).bind(r.selection.profile.id().encode()).bind(r.selection.profile.revision().to_be_bytes().to_vec()).bind(r.selection.source.encode()).bind(r.selection.harness.encode()).fetch_optional(c.connection().map_err(|_|SourceSessionError::StorageUnavailable)?).await.map_err(sql_error)?;
                let Some(canonical_path)=canonical_path else {return Err(SourceSessionError::InvalidSelection);};
                if r.selection.launch_path.is_empty() || r.selection.launch_path.contains('\0') || r.selection.launch_path.len() > 4096 || source_intent_digest(&r.selection,&r.action)?!=r.intent_digest || !crate::session_storage::node_configuration::source_sessions::launch_path_within_workspace(&r.selection.launch_path, &canonical_path).await {return Err(SourceSessionError::InvalidSelection);}
                let inserted=sqlx::query("INSERT INTO main.node_source_session_registrations(operation_id,action,actor_id,register_auth_generation,source_id,harness_id,profile_id,profile_revision,workspace_id,launch_path,cursor,intent_digest,registered_at_ms) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(operation_id) DO NOTHING")
                    .bind(r.id.encode()).bind(r.action.label()).bind(r.actor.encode()).bind(r.auth_generation).bind(r.selection.source.encode()).bind(r.selection.harness.encode()).bind(r.selection.profile.id().encode()).bind(r.selection.profile.revision().to_be_bytes().to_vec()).bind(r.selection.workspace.encode()).bind(&r.selection.launch_path).bind(r.action.cursor()).bind(r.intent_digest.to_vec()).bind(r.registered_at_ms).execute(c.connection().map_err(|_|SourceSessionError::StorageUnavailable)?).await.map_err(sql_error)?.rows_affected()>0;
                let stored=read(&mut c,r.id).await?;
                if !inserted {
                    if proof.generated_id() {return Err(SourceSessionError::Conflict);}
                    if stored.registration.id!=r.id || stored.registration.actor!=r.actor || stored.registration.selection!=r.selection || stored.registration.action!=r.action || stored.registration.intent_digest!=r.intent_digest {return Err(SourceSessionError::Conflict);}
                } else if stored.registration!=*r {
                    return Err(SourceSessionError::Conflict);
                }
                Ok(stored)
            }).await;
            self.finish_source_sql(c, result, &slot, d).await
        })
    }
    fn admit_source_session<'a>(
        &'a self,
        proof: &'a SourceAdmissionProof,
        slot: Arc<OriginalSourceSqlOperation>,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<SourceAdmissionResult, SourceSessionFailure>> {
        Box::pin(async move {
            let r = proof.registration();
            proof.verify_owner_scope(&self.registry.owner_scope())?;
            slot.verify(&self.registry.owner_scope(), Some(r.id))?;
            let mut c = self.begin_source_sql(&slot, true, d).await?;
            let result=wait_sql(d,async{
                let original=read(&mut c,r.id).await?;
                if original.registration!=*r {return Err(SourceSessionError::Conflict);}
                if original.fact.state!=SourceSessionState::Registered {return Ok(SourceAdmissionResult::Existing(original));}
                sqlx::query("INSERT INTO main.node_source_session_operations(operation_id,action,runtime_epoch,admission_auth_generation,admitted_at_ms,operation_state,outcome,updated_at_ms) VALUES(?,?,?,?,?,'owned','pending',?)")
                    .bind(r.id.encode()).bind(r.action.label()).bind(proof.runtime().0.encode()).bind(proof.actor().auth_generation).bind(proof.at()).bind(proof.at()).execute(c.connection().map_err(|_|SourceSessionError::StorageUnavailable)?).await.map_err(sql_error)?;
                Ok(SourceAdmissionResult::Admitted(read(&mut c,r.id).await?))
            }).await;
            self.finish_source_sql(c, result, &slot, d).await
        })
    }
    fn query_source_session<'a>(
        &'a self,
        proof: &'a SourceReaderProof,
        id: SourceSessionOperationId,
        slot: Arc<OriginalSourceSqlOperation>,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<SourceStoredOperation, SourceSessionFailure>> {
        Box::pin(async move {
            proof.verify_owner_scope(&self.registry.owner_scope())?;
            slot.verify(&self.registry.owner_scope(), Some(id))?;
            let mut c = self.begin_source_sql(&slot, false, d).await?;
            let result = wait_sql(d, read(&mut c, id)).await;
            self.finish_source_sql(c, result, &slot, d).await
        })
    }
    fn update_source_session<'a>(
        &'a self,
        proof: &'a SourceWriteProof,
        slot: Arc<OriginalSourceSqlOperation>,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<SourceStoredOperation, SourceSessionFailure>> {
        Box::pin(async move {
            proof.verify_owner_scope(&self.registry.owner_scope())?;
            slot.verify(&self.registry.owner_scope(), Some(proof.operation_id()))?;
            let mut c = self.begin_source_sql(&slot, true, d).await?;
            let result=wait_sql(d,async{
                let id=proof.operation_id().encode();let epoch=proof.runtime().0.encode();let at=proof.at();
                let original=read(&mut c,proof.operation_id()).await?;
                if original.fact.runtime!=Some(proof.runtime()) {return Err(SourceSessionError::Conflict);}
                let changed=match proof.view(){
                    SourceUpdateView::Attach{source,wire}=>{
                        let m=source.metadata();
                        sqlx::query("UPDATE main.node_source_session_operations SET source_instance_id=?,connection_epoch=?,protocol_version=?,updated_at_ms=? WHERE operation_id=? AND runtime_epoch=? AND (source_instance_id IS NULL OR (source_instance_id=? AND connection_epoch=?))")
                            .bind(m.instance_id.encode()).bind(m.connection_epoch.encode()).bind(wire.number()).bind(at).bind(&id).bind(&epoch).bind(m.instance_id.encode()).bind(m.connection_epoch.encode()).execute(c.connection().map_err(|_|SourceSessionError::StorageUnavailable)?).await.map_err(sql_error)?.rows_affected()
                    }
                    SourceUpdateView::Reserve{source,wire,rpc,frame_length}=>{
                        let m=source.metadata();let (kind,bytes)=rpc_parts(rpc);
                        sqlx::query("UPDATE main.node_source_session_operations SET rpc_id_kind=?,rpc_id_bytes=?,frame_length=?,accepted_length=X'0000000000000000',transport_state='reserved',updated_at_ms=? WHERE operation_id=? AND runtime_epoch=? AND source_instance_id=? AND connection_epoch=? AND protocol_version=? AND rpc_id_kind IS NULL")
                            .bind(kind).bind(bytes).bind(frame_length.to_be_bytes().to_vec()).bind(at).bind(&id).bind(&epoch).bind(m.instance_id.encode()).bind(m.connection_epoch.encode()).bind(wire.number()).execute(c.connection().map_err(|_|SourceSessionError::StorageUnavailable)?).await.map_err(sql_error)?.rows_affected()
                    }
                    SourceUpdateView::Arm{rpc}=>{
                        let(kind,bytes)=rpc_parts(rpc);sqlx::query("UPDATE main.node_source_session_operations SET transport_state='armed',updated_at_ms=? WHERE operation_id=? AND runtime_epoch=? AND rpc_id_kind=? AND rpc_id_bytes=? AND transport_state='reserved'")
                            .bind(at).bind(&id).bind(&epoch).bind(kind).bind(bytes).execute(c.connection().map_err(|_|SourceSessionError::StorageUnavailable)?).await.map_err(sql_error)?.rows_affected()
                    }
                    SourceUpdateView::Transport{rpc,frame_length,fact}=>{
                        let(kind,bytes)=rpc_parts(rpc);let(state,accepted,reason)=match fact{TransportFact::Written=>("written",frame_length,None),TransportFact::Zero{reason}=>("zero",0,Some(reason.label())),TransportFact::Unknown{accepted,cause}=>("unknown",accepted,Some(cause.label()))};
                        sqlx::query("UPDATE main.node_source_session_operations SET transport_state=?,accepted_length=?,transport_reason=?,transport_recorded_at_ms=?,updated_at_ms=? WHERE operation_id=? AND runtime_epoch=? AND rpc_id_kind=? AND rpc_id_bytes=? AND frame_length=? AND transport_state IN ('armed','unknown','written')")
                            .bind(state).bind(accepted.to_be_bytes().to_vec()).bind(reason).bind(at).bind(at).bind(&id).bind(&epoch).bind(kind).bind(bytes).bind(frame_length.to_be_bytes().to_vec()).execute(c.connection().map_err(|_|SourceSessionError::StorageUnavailable)?).await.map_err(sql_error)?.rows_affected()
                    }
                    SourceUpdateView::Response{rpc,fact}=>{
                        let(kind,bytes)=rpc_parts(rpc);let ack=match fact.ack{SourceAckState::Pending=>"pending",SourceAckState::Confirmed=>"confirmed",SourceAckState::Rejected=>"rejected",SourceAckState::Unknown=>"unknown"};
                        sqlx::query("UPDATE main.node_source_session_operations SET response_kind=?,native_session_id=?,remote_error_code=?,response_sequence=?,response_receipt=?,response_evidence=?,result_bytes=?,ack_state=?,updated_at_ms=? WHERE operation_id=? AND runtime_epoch=? AND rpc_id_kind=? AND rpc_id_bytes=? AND (response_kind IS NULL OR (response_kind=? AND response_sequence=? AND response_receipt=? AND response_evidence=?))")
                            .bind(fact.kind.label()).bind(fact.native_session_id.as_deref()).bind(fact.remote_error_code).bind(fact.sequence.to_be_bytes().to_vec()).bind(fact.receipt.to_be_bytes().to_vec()).bind(fact.evidence.to_vec()).bind(fact.result_bytes.to_be_bytes().to_vec()).bind(ack).bind(at).bind(&id).bind(&epoch).bind(kind).bind(bytes).bind(fact.kind.label()).bind(fact.sequence.to_be_bytes().to_vec()).bind(fact.receipt.to_be_bytes().to_vec()).bind(fact.evidence.to_vec()).execute(c.connection().map_err(|_|SourceSessionError::StorageUnavailable)?).await.map_err(sql_error)?.rows_affected()
                    }
                    SourceUpdateView::Finish{outcome,failure}=>{
                        let outcome=match outcome{SourceSessionOutcome::Succeeded=>"succeeded",SourceSessionOutcome::Refused=>"refused",SourceSessionOutcome::Unknown=>"unknown",SourceSessionOutcome::Pending=>return Err(SourceSessionError::CorruptState)};
                        sqlx::query("UPDATE main.node_source_session_operations SET operation_state='terminal',outcome=?,failure_class=?,terminal_at_ms=?,updated_at_ms=? WHERE operation_id=? AND runtime_epoch=? AND operation_state='owned' AND outcome='pending'")
                            .bind(outcome).bind(failure.map(SourceFailureClass::label)).bind(at).bind(at).bind(&id).bind(&epoch).execute(c.connection().map_err(|_|SourceSessionError::StorageUnavailable)?).await.map_err(sql_error)?.rows_affected()
                    }
                };
                if changed!=1{return Err(SourceSessionError::Conflict);}
                read(&mut c,proof.operation_id()).await
            }).await;
            self.finish_source_sql(c, result, &slot, d).await
        })
    }
    fn reconcile_source_session<'a>(
        &'a self,
        proof: &'a SourceReconcileProof,
        slot: Arc<OriginalSourceSqlOperation>,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<SourceStoredOperation, SourceSessionFailure>> {
        Box::pin(async move {
            proof.verify_owner_scope(&self.registry.owner_scope())?;
            slot.verify(&self.registry.owner_scope(), Some(proof.operation_id()))?;
            let mut c = self.begin_source_sql(&slot, false, d).await?;
            let result = wait_sql(d, read(&mut c, proof.operation_id())).await;
            self.finish_source_sql(c, result, &slot, d).await
        })
    }
    fn recover_source_sessions<'a>(
        &'a self,
        proof: &'a SourceRecoveryProof,
        slot: Arc<OriginalSourceSqlOperation>,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<Vec<StoredNativeClaimBarrier>, SourceSessionFailure>> {
        Box::pin(async move {
            proof.verify_owner_scope(&self.registry.owner_scope())?;
            slot.verify(&self.registry.owner_scope(), None)?;
            let mut c = self.begin_source_sql(&slot, true, d).await?;
            let result=wait_sql(d,async{
                sqlx::query("UPDATE main.node_source_session_operations SET operation_state='interrupted',outcome='unknown',failure_class='interrupted',terminal_at_ms=?,updated_at_ms=?,transport_state=CASE WHEN transport_state IN ('armed','reserved') THEN 'unknown' ELSE transport_state END,transport_reason=CASE WHEN transport_state IN ('armed','reserved') THEN 'restart' ELSE transport_reason END,transport_recorded_at_ms=CASE WHEN transport_state IN ('armed','reserved') THEN ? ELSE transport_recorded_at_ms END,ack_state=CASE WHEN ack_state='pending' THEN 'unknown' ELSE ack_state END WHERE runtime_epoch!=? AND operation_state='owned'")
                    .bind(proof.at()).bind(proof.at()).bind(proof.at()).bind(proof.runtime().0.encode()).execute(c.connection().map_err(|_|SourceSessionError::StorageUnavailable)?).await.map_err(sql_error)?;
                let rows=sqlx::query("SELECT r.source_id,o.native_session_id AS native_id,o.source_instance_id AS instance_id FROM main.node_source_session_operations o JOIN main.node_source_session_registrations r ON r.operation_id=o.operation_id JOIN main.node_native_source_instances s ON s.instance_id=o.source_instance_id WHERE o.response_kind='created' AND s.lifecycle!='closed' UNION SELECT t.source_id,t.native_id,b.instance_id FROM main.node_native_resource_bindings b JOIN main.product_targets t ON t.target_id=b.target_id JOIN main.node_native_source_instances s ON s.instance_id=b.instance_id WHERE s.lifecycle!='closed'")
                    .fetch_all(c.connection().map_err(|_|SourceSessionError::StorageUnavailable)?).await.map_err(sql_error)?;
                let mut barriers=Vec::new();barriers.try_reserve_exact(rows.len()).map_err(|_|SourceSessionError::Allocation)?;
                for row in rows {let source=id(&row,"source_id")?;let instance=id(&row,"instance_id")?;let native:String=col(&row,"native_id")?;if let Ok(native)=NativeSessionKey::from_config(Some(source),&native){barriers.push(StoredNativeClaimBarrier::from_necessary_row(native,instance));}}
                Ok(barriers)
            }).await;
            self.finish_source_sql(c, result, &slot, d).await
        })
    }
}
