//! Default approval audit: exact original basis and both-family budget under the same owner.
use super::{
    ApprovalCommandRecord, ApprovalId, BoxFuture, CloseOutcome, Future, RegistryDeadline,
    SqliteConnection, SqliteNativeDispatch, TerminalReplyObservation, rows::ApprovalRow,
};
use crate::{
    application::{
        native_dispatch::{ApprovalAuditBasis, ApprovalAuditNecessary, ApprovalAuditPort},
        request_registry::{NodeClock, SystemNodeClock},
    },
    domain::{
        approval_audit::{
            ApprovalAuditEventKind, ApprovalAuditOutcome, ApprovalAuditReceipt,
            ApprovalAuditWarning,
        },
        management_audit::{AuditStoreError, retention},
    },
};
use sqlx::Row;
use std::sync::Arc;
const fn warning(error: AuditStoreError) -> ApprovalAuditWarning {
    match error {
        AuditStoreError::Full => ApprovalAuditWarning::Full,
        AuditStoreError::CommitUnconfirmed => ApprovalAuditWarning::CommitUnconfirmed,
        AuditStoreError::CloseUnconfirmed { .. } => ApprovalAuditWarning::CloseUnconfirmed,
        AuditStoreError::Cancelled => ApprovalAuditWarning::Stopped,
        _ => ApprovalAuditWarning::Sql,
    }
}
async fn audit_bound<T>(
    d: &RegistryDeadline,
    f: impl Future<Output = Result<T, ApprovalAuditWarning>>,
) -> Result<T, ApprovalAuditWarning> {
    tokio::select! {biased;()=d.cancelled()=>Err(ApprovalAuditWarning::Stopped),v=tokio::time::timeout_at(d.deadline(),f)=>v.map_err(|_|ApprovalAuditWarning::Sql)?}
}
async fn exact_audit<T: ApprovalRow>(
    c: &mut SqliteConnection,
    record: &T,
) -> Result<(), ApprovalAuditWarning> {
    super::exact(c, record)
        .await
        .map(|_| ())
        .map_err(|_| ApprovalAuditWarning::InvalidBasis)
}
async fn association_target(
    c: &mut SqliteConnection,
    approval: ApprovalId,
) -> Result<Option<crate::domain::EntityId>, ApprovalAuditWarning> {
    let value: Option<String> = sqlx::query_scalar(
        "SELECT target_id FROM main.node_approval_associations WHERE approval_id=?",
    )
    .bind(approval.encode())
    .fetch_optional(c)
    .await
    .map_err(|_| ApprovalAuditWarning::Sql)?;
    value
        .as_deref()
        .map(|value| {
            crate::domain::EntityId::decode(value).map_err(|_| ApprovalAuditWarning::InvalidBasis)
        })
        .transpose()
}
async fn validate_basis(
    c: &mut SqliteConnection,
    basis: &ApprovalAuditBasis,
    owner: &crate::application::request_registry::RegistryOwnerScope,
) -> Result<(), ApprovalAuditWarning> {
    let expected = basis.verify_original(owner)?;
    let m = basis.metadata();
    let target = association_target(c, m.approval).await?;
    if target != m.target {
        return Err(ApprovalAuditWarning::InvalidBasis);
    }
    match expected {
        ApprovalAuditNecessary::Registered(record) => {
            let rows = super::list::<ApprovalCommandRecord>(c, record.approval)
                .await
                .map_err(|_| ApprovalAuditWarning::InvalidBasis)?;
            let actual = rows
                .iter()
                .find(|row| row.registration == record.registration)
                .ok_or(ApprovalAuditWarning::InvalidBasis)?;
            let mut immutable = actual.clone();
            immutable.state = record.state;
            immutable.operation = record.operation;
            immutable.admission_auth_generation = record.admission_auth_generation;
            immutable.admitted_at_ms = record.admitted_at_ms;
            immutable.rejection = record.rejection;
            if immutable != record
                || m.event != ApprovalAuditEventKind::Registered
                || m.outcome != ApprovalAuditOutcome::Registered
                || m.actor != Some(record.actor)
                || m.basis_fingerprint != record.intent_fingerprint
                || m.occurred_at_ms != record.registered_at_ms
            {
                return Err(ApprovalAuditWarning::InvalidBasis);
            }
        }
        ApprovalAuditNecessary::Decided(record) => {
            exact_audit(c, &record).await?;
            if m.event != ApprovalAuditEventKind::Decided
                || m.outcome != ApprovalAuditOutcome::Accepted
                || m.actor != Some(record.actor)
                || m.basis_fingerprint != record.fingerprint
                || m.occurred_at_ms != record.accepted_at_ms
            {
                return Err(ApprovalAuditWarning::InvalidBasis);
            }
        }
        ApprovalAuditNecessary::Delivery { arm, terminal } => {
            exact_audit(c, &arm).await?;
            exact_audit(c, &terminal).await?;
            let outcome = match terminal.observation {
                TerminalReplyObservation::Zero { .. } => ApprovalAuditOutcome::Zero,
                TerminalReplyObservation::Written => ApprovalAuditOutcome::Written,
                TerminalReplyObservation::Unknown { .. } => ApprovalAuditOutcome::Unknown,
            };
            if m.event != ApprovalAuditEventKind::Delivery
                || m.outcome != outcome
                || m.actor.is_some()
                || m.basis_fingerprint != terminal.fingerprint
                || m.occurred_at_ms != terminal.observed_at_ms
            {
                return Err(ApprovalAuditWarning::InvalidBasis);
            }
        }
        ApprovalAuditNecessary::Invalidated(record) => {
            exact_audit(c, &record).await?;
            if m.event != ApprovalAuditEventKind::Invalidated
                || m.outcome != ApprovalAuditOutcome::Invalidated
                || m.actor.is_some()
                || m.basis_fingerprint != record.fingerprint
                || m.occurred_at_ms != record.observed_at_ms
            {
                return Err(ApprovalAuditWarning::InvalidBasis);
            }
        }
    }
    Ok(())
}
fn duplicate(
    row: &sqlx::sqlite::SqliteRow,
    id: Arc<str>,
    basis: &ApprovalAuditBasis,
) -> Result<ApprovalAuditReceipt, ApprovalAuditWarning> {
    let m = basis.metadata();
    let decode = || -> Result<ApprovalAuditReceipt, sqlx::Error> {
        let approval: String = row.try_get("approval_id")?;
        let event: String = row.try_get("event_kind")?;
        let fp: Vec<u8> = row.try_get("basis_fingerprint")?;
        let actor: Option<String> = row.try_get("actor_id")?;
        let target: Option<String> = row.try_get("target_id")?;
        let outcome: String = row.try_get("outcome")?;
        let occurred: i64 = row.try_get("occurred_at_ms")?;
        let recorded: i64 = row.try_get("recorded_at_ms")?;
        let retained: i64 = row.try_get("retain_until_ms")?;
        if approval != m.approval.encode()
            || event != m.event.label()
            || fp != m.basis_fingerprint
            || actor != m.actor.map(crate::domain::EntityId::encode)
            || target != m.target.map(crate::domain::EntityId::encode)
            || outcome != m.outcome.label()
            || occurred != m.occurred_at_ms
            || recorded < 0
            || retention(recorded).is_err()
            || retention(recorded).is_ok_and(|minimum| retained < minimum)
        {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(ApprovalAuditReceipt {
            record_id: id,
            approval: m.approval,
            event: m.event,
            basis_fingerprint: m.basis_fingerprint,
            outcome: m.outcome,
            recorded_at_ms: recorded,
            retain_until_ms: retained,
            duplicate: true,
        })
    };
    decode().map_err(|_| ApprovalAuditWarning::InvalidBasis)
}
impl ApprovalAuditPort for SqliteNativeDispatch {
    fn append_approval<'a>(
        &'a self,
        basis: &'a ApprovalAuditBasis,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<ApprovalAuditReceipt, ApprovalAuditWarning>> {
        Box::pin(super::approval_operation(
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            self.registry.approval_test_controller(),
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            super::test_control::ApprovalOperationTag {
                operation: basis.metadata().approval.entity(),
                kind: super::test_control::ApprovalSqlOperation::ApprovalAudit,
                instance: basis.original_instance_for_test(),
                approval: Some(basis.metadata().approval),
                registration: None,
                response: None,
            },
            async move {
                self.registry
                    .owner_scope()
                    .require_open()
                    .map_err(|_| ApprovalAuditWarning::Stopped)?;
                basis.verify_original(&self.registry.owner_scope())?;
                let mut connection = audit_bound(d, async {
                    self.registry
                        .begin()
                        .await
                        .map_err(|_| ApprovalAuditWarning::Sql)
                })
                .await?;
                #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
                connection
                    .attach_approval_test_context(super::test_control::current_original_context());
                let mut open = true;
                let mut commit_attempted = false;
                let result=audit_bound(d,async{
   let c=connection.connection().map_err(|_|ApprovalAuditWarning::Sql)?;
   super::super::management_audit_budget::validate(c).await.map_err(warning)?;
   validate_basis(c,basis,&self.registry.owner_scope()).await?;
   let m=basis.metadata();let id:Arc<str>=Arc::from(format!("{}:{}:{}",m.event.label(),m.approval.encode(),hex::encode(m.basis_fingerprint)));
   if let Some(row)=sqlx::query("SELECT * FROM main.node_approval_audit WHERE record_id=?").bind(&*id).fetch_optional(&mut *c).await.map_err(|_|ApprovalAuditWarning::Sql)? {
    let receipt=duplicate(&row,id,basis)?;sqlx::query("ROLLBACK").execute(&mut *c).await.map_err(|_|ApprovalAuditWarning::Sql)?;open=false;return Ok(receipt);
   }
   let now=SystemNodeClock.unix_millis();let until=retention(now).map_err(warning)?;
   super::super::management_audit_budget::prune_both(c,now).await.map_err(warning)?;
   super::super::management_audit_budget::require_room(c).await.map_err(warning)?;
   #[cfg(all(test,feature="approval-owner-acceptance-fixtures"))]
   let fault=super::test_sql::before(c,super::test_control::ApprovalStatement::AuditInsertApproval).await.map_err(|_|ApprovalAuditWarning::Sql)?;
   let inserted=sqlx::query("INSERT INTO main.node_approval_audit(record_id,approval_id,basis_fingerprint,event_kind,actor_id,target_id,outcome,occurred_at_ms,recorded_at_ms,retain_until_ms,retention_hold) VALUES(?,?,?,?,?,?,?,?,?,?,0)")
    .bind(&*id).bind(m.approval.encode()).bind(m.basis_fingerprint.to_vec()).bind(m.event.label()).bind(m.actor.map(crate::domain::EntityId::encode)).bind(m.target.map(crate::domain::EntityId::encode)).bind(m.outcome.label()).bind(m.occurred_at_ms).bind(now).bind(until).execute(&mut *c).await;
   #[cfg(all(test,feature="approval-owner-acceptance-fixtures"))]
   super::test_sql::after(c,fault,&inserted).await.map_err(|_|ApprovalAuditWarning::Sql)?;
   inserted.map_err(|_|ApprovalAuditWarning::Sql)?;
   #[cfg(all(test,feature="approval-owner-acceptance-fixtures"))]
   let fault=super::test_sql::before(c,super::test_control::ApprovalStatement::Commit).await.map_err(|_|ApprovalAuditWarning::Sql)?;
   commit_attempted=true;let committed=sqlx::query("COMMIT").execute(&mut *c).await;
   #[cfg(all(test,feature="approval-owner-acceptance-fixtures"))]
   super::test_sql::after(c,fault,&committed).await.map_err(|_|ApprovalAuditWarning::CommitUnconfirmed)?;
   committed.map_err(|_|ApprovalAuditWarning::CommitUnconfirmed)?;open=false;
   #[cfg(all(test,feature="approval-owner-acceptance-fixtures"))]
   connection.approval_commit_for_test(true);
   Ok(ApprovalAuditReceipt{record_id:id,approval:m.approval,event:m.event,basis_fingerprint:m.basis_fingerprint,outcome:m.outcome,recorded_at_ms:now,retain_until_ms:until,duplicate:false})
  }).await;
                if open && let Ok(c) = connection.connection() {
                    let _ = audit_bound(d, async {
                        sqlx::query("ROLLBACK")
                            .execute(c)
                            .await
                            .map_err(|_| ApprovalAuditWarning::Sql)?;
                        Ok(())
                    })
                    .await;
                }
                let closed = audit_bound(d, async { Ok(connection.close().await) }).await;
                if !matches!(closed, Ok(CloseOutcome::Confirmed)) {
                    return Err(ApprovalAuditWarning::CloseUnconfirmed);
                }
                if commit_attempted && result == Err(ApprovalAuditWarning::Sql) {
                    return Err(ApprovalAuditWarning::CommitUnconfirmed);
                }
                result
            },
        ))
    }
}
