//! Optional audit writes never constitute a request, permission or dispatch grant.
use super::{
    registry_connection_owner::RegistryConnection,
    request_registry::{SqliteRequestRegistry, settled_audit_basis},
};
use crate::{
    application::request_registry::{
        AuditFailure, AuditMetadata, AuditPort, NodeClock, SystemNodeClock,
    },
    domain::{
        management_audit::{
            AuditReceipt, AuditRecordId, AuditStoreError as Error, SettledAuditBasis, retention,
        },
        request_registry::{QueueOutcome, RegistryError},
    },
};
use sqlx::{Row, sqlite::SqliteRow};
use std::{future::Future, pin::Pin, sync::Arc};

#[cfg(all(test, windows))]
pub(super) mod test_hooks;
#[cfg(all(test, windows))]
pub use test_hooks::{AuditFault, AuditPoint, AuditSqlPoint, AuditTestPort, VmGate};
#[cfg(all(test, windows))]
mod dev_tests;

#[derive(Clone)]
pub struct SqliteManagementAudit {
    registry: SqliteRequestRegistry,
    clock: Arc<dyn NodeClock>,
    #[cfg(all(test, windows))]
    hooks: Arc<test_hooks::AuditHooks>,
}
impl SqliteManagementAudit {
    pub(super) fn from_registry(registry: SqliteRequestRegistry) -> Self {
        Self {
            registry,
            clock: Arc::new(SystemNodeClock),
            #[cfg(all(test, windows))]
            hooks: Arc::default(),
        }
    }
    #[cfg(all(test, windows))]
    pub(crate) fn test_port(&self) -> AuditTestPort {
        AuditTestPort::new(self.clone())
    }

    async fn append_detailed(&self, metadata: &AuditMetadata) -> Result<AuditReceipt, Error> {
        #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
        {
            use super::approval_owner::test_control::{
                self, ApprovalOperationTag, ApprovalSqlOperation,
            };
            let operation = crate::domain::EntityId::decode(&metadata.request.encode())
                .map_err(|_| Error::InvalidSource)?;
            let tag = ApprovalOperationTag {
                operation,
                kind: ApprovalSqlOperation::QueueAudit,
                instance: None,
                approval: None,
                registration: None,
                response: None,
            };
            return test_control::with_original_operation(
                self.registry.approval_test_controller(),
                tag,
                self.append_detailed_unscoped(metadata),
            )
            .await;
        }
        #[cfg(not(all(test, feature = "approval-owner-acceptance-fixtures")))]
        self.append_detailed_unscoped(metadata).await
    }
    async fn append_detailed_unscoped(
        &self,
        metadata: &AuditMetadata,
    ) -> Result<AuditReceipt, Error> {
        self.registry
            .owner_scope()
            .while_open(async { Ok(self.append_inner(metadata).await) })
            .await
            .map_err(|error| match error {
                RegistryError::NodeStopping | RegistryError::Cancelled => Error::Cancelled,
                _ => Error::Sql,
            })?
    }
    async fn append_inner(&self, metadata: &AuditMetadata) -> Result<AuditReceipt, Error> {
        let mut connection = self
            .registry
            .audit_connection(
                #[cfg(all(test, windows))]
                &self.hooks,
            )
            .await
            .map_err(registry_error)?;
        let mut open = false;
        let mut committed = false;
        let mut result = async {
            #[cfg(all(test, windows))] self.hooks.point(AuditPoint::AfterAcquire).await?;
            self.execute(&mut connection, "BEGIN IMMEDIATE", #[cfg(all(test, windows))] AuditSqlPoint::Begin).await?;
            open = true;
            #[cfg(all(test, windows))] self.hooks.point(AuditPoint::AfterBegin).await?;
            super::management_audit_budget::validate(connection.connection().map_err(registry_error)?).await?;
            let basis = settled_audit_basis(connection.connection().map_err(registry_error)?, metadata.request).await.map_err(source_error)?;
            if basis.actor != metadata.actor || basis.action != metadata.action || basis.target != metadata.target || basis.outcome != metadata.outcome { return Err(Error::InvalidSource); }
            let id = AuditRecordId::settled(metadata.request);
            if let Some(row) = sqlx::query("SELECT * FROM main.node_management_audit WHERE record_id=?").bind(id.encode())
                .fetch_optional(connection.connection().map_err(registry_error)?).await.map_err(|_| Error::Sql)? {
                let receipt = duplicate(&row, basis)?;
                sqlx::query("ROLLBACK").execute(connection.connection().map_err(registry_error)?).await.map_err(|_| Error::Sql)?;
                open = false;
                return Ok(receipt);
            }
            let now = self.clock.unix_millis();
            let until = retention(now)?;
            #[cfg(all(test, windows))] self.hooks.install(&mut connection, AuditSqlPoint::Prune).await?;
            #[cfg(all(test,feature="approval-owner-acceptance-fixtures"))]
            let approval_fault=super::approval_owner::test_sql::before(connection.connection().map_err(registry_error)?,super::approval_owner::test_control::ApprovalStatement::AuditDeleteQueue).await.map_err(|_|Error::Sql)?;
            let deleted = sqlx::query("DELETE FROM main.node_management_audit WHERE retention_hold=0 AND retain_until_ms<=?").bind(now)
                .execute(connection.connection().map_err(registry_error)?).await;
            #[cfg(all(test, windows))] self.hooks.remove(&mut connection).await?;
            #[cfg(all(test,feature="approval-owner-acceptance-fixtures"))]
            super::approval_owner::test_sql::after(connection.connection().map_err(registry_error)?,approval_fault,&deleted).await.map_err(|_|Error::Sql)?;
            deleted.map_err(|_| Error::Sql)?;
            super::management_audit_budget::prune_approval(connection.connection().map_err(registry_error)?,now).await?;
            #[cfg(all(test, windows))] self.hooks.point(AuditPoint::AfterPrune).await?;
            super::management_audit_budget::require_room(connection.connection().map_err(registry_error)?).await?;
            let parts = outcome_parts(basis.outcome);
            #[cfg(all(test, windows))] self.hooks.install(&mut connection, AuditSqlPoint::Insert).await?;
            #[cfg(all(test,feature="approval-owner-acceptance-fixtures"))]
            let approval_fault=super::approval_owner::test_sql::before(connection.connection().map_err(registry_error)?,super::approval_owner::test_control::ApprovalStatement::AuditInsertQueue).await.map_err(|_|Error::Sql)?;
            let inserted = sqlx::query("INSERT INTO main.node_management_audit(record_id,event_version,event_kind,request_id,operation_id,actor_id,action,target_id,basis_kind,register_auth_generation,admission_auth_generation,runtime_epoch,outcome,result_item_id,result_revision,restored_count,rejection_code,event_at_ms,observed_at_ms,recorded_at_ms,retain_until_ms,retention_hold) VALUES(?,1,'queue.settled',?,?,?,?,?,'registered_operation',?,?,?,?,?,?,?,?,?,?,?,?,0)")
                .bind(id.encode()).bind(basis.request.encode()).bind(basis.operation.encode()).bind(basis.actor.encode()).bind(basis.action.label()).bind(basis.target.encode())
                .bind(basis.register_generation).bind(basis.admission_generation).bind(basis.runtime.0.encode()).bind(parts.code)
                .bind(parts.item).bind(parts.revision).bind(parts.restored).bind(parts.rejection).bind(basis.event_at_ms).bind(metadata.observed_at_ms).bind(now).bind(until)
                .execute(connection.connection().map_err(registry_error)?).await;
            #[cfg(all(test, windows))] self.hooks.remove(&mut connection).await?;
            #[cfg(all(test,feature="approval-owner-acceptance-fixtures"))]
            super::approval_owner::test_sql::after(connection.connection().map_err(registry_error)?,approval_fault,&inserted).await.map_err(|_|Error::Sql)?;
            inserted.map_err(|_| Error::Sql)?;
            #[cfg(all(test, windows))] {
                self.hooks.point(AuditPoint::AfterInsert).await?;
                self.hooks.point(AuditPoint::BeforeCommit).await?;
                self.hooks.fail_before_commit()?;
            }
            self.execute(&mut connection, "COMMIT", #[cfg(all(test, windows))] AuditSqlPoint::Commit).await.map_err(|e| if e == Error::Sql { Error::CommitUnconfirmed } else { e })?;
            open = false;
            committed = true;
            #[cfg(all(test, windows))] {
                self.hooks.point(AuditPoint::AfterCommitAck).await?;
                self.hooks.lose_reply()?;
            }
            Ok(AuditReceipt { id, recorded_at_ms: now, retain_until_ms: until, duplicate: false })
        }.await;
        if open {
            let rollback = match connection.connection() {
                Ok(raw) => sqlx::query("ROLLBACK")
                    .execute(raw)
                    .await
                    .map(|_| ())
                    .map_err(|_| Error::Sql),
                Err(_) => Err(Error::Sql),
            };
            if rollback.is_err() && result != Err(Error::CommitUnconfirmed) {
                result = Err(Error::Sql);
            }
        }
        if connection.close().await == super::registry_connection_owner::CloseOutcome::Unconfirmed {
            return Err(Error::CloseUnconfirmed { committed });
        }
        result
    }
    async fn execute(
        &self,
        connection: &mut RegistryConnection,
        sql: &'static str,
        #[cfg(all(test, windows))] point: AuditSqlPoint,
    ) -> Result<(), Error> {
        #[cfg(all(test, windows))]
        self.hooks.install(connection, point).await?;
        #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
        let approval_fault = if sql == "COMMIT" {
            super::approval_owner::test_sql::before(
                connection.connection().map_err(registry_error)?,
                super::approval_owner::test_control::ApprovalStatement::Commit,
            )
            .await
            .map_err(|_| Error::Sql)?
        } else {
            None
        };
        let result = sqlx::query(sql)
            .execute(connection.connection().map_err(registry_error)?)
            .await;
        #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
        super::approval_owner::test_sql::after(
            connection.connection().map_err(registry_error)?,
            approval_fault,
            &result,
        )
        .await
        .map_err(|_| Error::Sql)?;
        #[cfg(all(test, windows))]
        self.hooks.remove(connection).await?;
        result.map(|_| ()).map_err(|_| Error::Sql)
    }
}
impl AuditPort for SqliteManagementAudit {
    fn append<'a>(
        &'a self,
        metadata: &'a AuditMetadata,
    ) -> Pin<Box<dyn Future<Output = Result<(), AuditFailure>> + Send + 'a>> {
        Box::pin(async move {
            self.append_detailed(metadata)
                .await
                .map(|_| ())
                .map_err(|e| match e {
                    Error::Full => AuditFailure::Full,
                    Error::Cancelled => AuditFailure::Cancelled,
                    _ => AuditFailure::Unavailable,
                })
        })
    }
}
const fn registry_error(error: RegistryError) -> Error {
    match error {
        RegistryError::NodeStopping | RegistryError::Cancelled => Error::Cancelled,
        RegistryError::InvalidIntent | RegistryError::UnknownRegistration => Error::InvalidSource,
        RegistryError::CorruptState => Error::NecessaryIntegrity,
        _ => Error::Sql,
    }
}
const fn source_error(error: RegistryError) -> Error {
    match error {
        RegistryError::NodeStopping | RegistryError::Cancelled => Error::Cancelled,
        RegistryError::InvalidIntent | RegistryError::UnknownRegistration => Error::InvalidSource,
        _ => Error::NecessaryIntegrity,
    }
}
struct OutcomeParts {
    code: &'static str,
    item: Option<String>,
    revision: Option<Vec<u8>>,
    restored: Option<i64>,
    rejection: Option<&'static str>,
}
fn outcome_parts(outcome: QueueOutcome) -> OutcomeParts {
    let mut parts = OutcomeParts {
        code: "",
        item: None,
        revision: None,
        restored: None,
        rejection: None,
    };
    let ordinary = match outcome {
        QueueOutcome::Queued { item, revision } => Some(("queued", item, revision)),
        QueueOutcome::DirectSelected { item, revision } => {
            Some(("direct_selected", item, revision))
        }
        QueueOutcome::Edited { item, revision } => Some(("edited", item, revision)),
        QueueOutcome::Cancelled { item, revision } => Some(("cancelled", item, revision)),
        QueueOutcome::Prioritized { item, revision } => Some(("prioritized", item, revision)),
        QueueOutcome::Resumed { restored } => {
            parts.code = "resumed";
            parts.restored = Some(i64::from(restored));
            None
        }
        QueueOutcome::Rejected(rejection) => {
            parts.code = "rejected";
            parts.rejection = Some(rejection.label());
            None
        }
    };
    if let Some((code, item, revision)) = ordinary {
        parts.code = code;
        parts.item = Some(item.encode());
        parts.revision = Some(revision.to_be_bytes().to_vec());
    }
    parts
}
fn duplicate(row: &SqliteRow, basis: SettledAuditBasis) -> Result<AuditReceipt, Error> {
    let parts = outcome_parts(basis.outcome);
    let text = |name| row.try_get::<String, _>(name).map_err(|_| Error::Conflict);
    if text("request_id")? != basis.request.encode()
        || text("operation_id")? != basis.operation.encode()
        || text("actor_id")? != basis.actor.encode()
        || text("action")? != basis.action.label()
        || text("target_id")? != basis.target.encode()
        || text("runtime_epoch")? != basis.runtime.0.encode()
        || text("basis_kind")? != "registered_operation"
        || text("event_kind")? != "queue.settled"
        || text("outcome")? != parts.code
        || row
            .try_get::<i64, _>("event_version")
            .map_err(|_| Error::Conflict)?
            != 1
        || row
            .try_get::<i64, _>("register_auth_generation")
            .map_err(|_| Error::Conflict)?
            != basis.register_generation
        || row
            .try_get::<i64, _>("admission_auth_generation")
            .map_err(|_| Error::Conflict)?
            != basis.admission_generation
        || row
            .try_get::<i64, _>("event_at_ms")
            .map_err(|_| Error::Conflict)?
            != basis.event_at_ms
        || row
            .try_get::<Option<String>, _>("result_item_id")
            .map_err(|_| Error::Conflict)?
            != parts.item
        || row
            .try_get::<Option<Vec<u8>>, _>("result_revision")
            .map_err(|_| Error::Conflict)?
            != parts.revision
        || row
            .try_get::<Option<i64>, _>("restored_count")
            .map_err(|_| Error::Conflict)?
            != parts.restored
        || row
            .try_get::<Option<String>, _>("rejection_code")
            .map_err(|_| Error::Conflict)?
            .as_deref()
            != parts.rejection
    {
        return Err(Error::Conflict);
    }
    let recorded = row
        .try_get::<i64, _>("recorded_at_ms")
        .map_err(|_| Error::Conflict)?;
    let until = row
        .try_get::<i64, _>("retain_until_ms")
        .map_err(|_| Error::Conflict)?;
    if until < retention(recorded).map_err(|_| Error::Conflict)? {
        return Err(Error::Conflict);
    }
    Ok(AuditReceipt {
        id: AuditRecordId::settled(basis.request),
        recorded_at_ms: recorded,
        retain_until_ms: until,
        duplicate: true,
    })
}
