//! Exactly the two supplemental audit families share one caller-owned BEGIN IMMEDIATE.
#[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
use super::approval_owner::{test_control::ApprovalStatement, test_sql};
use crate::domain::management_audit::{AUDIT_LIMIT, AuditStoreError};
use sqlx::SqliteConnection;
pub(super) async fn validate(c: &mut SqliteConnection) -> Result<(), AuditStoreError> {
    super::schema::validate_management_audit(c).await?;
    super::schema::validate_approval_audit(c).await
}
pub(super) async fn prune_approval(
    c: &mut SqliteConnection,
    now: i64,
) -> Result<(), AuditStoreError> {
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    let fault = test_sql::before(c, ApprovalStatement::AuditDeleteApproval)
        .await
        .map_err(|_| AuditStoreError::Sql)?;
    let result = sqlx::query(
        "DELETE FROM main.node_approval_audit WHERE retention_hold=0 AND retain_until_ms<=?",
    )
    .bind(now)
    .execute(&mut *c)
    .await;
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    test_sql::after(c, fault, &result)
        .await
        .map_err(|_| AuditStoreError::Sql)?;
    result.map_err(|_| AuditStoreError::Sql)?;
    Ok(())
}
pub(super) async fn prune_both(c: &mut SqliteConnection, now: i64) -> Result<(), AuditStoreError> {
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    let fault = test_sql::before(c, ApprovalStatement::AuditDeleteQueue)
        .await
        .map_err(|_| AuditStoreError::Sql)?;
    let result = sqlx::query(
        "DELETE FROM main.node_management_audit WHERE retention_hold=0 AND retain_until_ms<=?",
    )
    .bind(now)
    .execute(&mut *c)
    .await;
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    test_sql::after(c, fault, &result)
        .await
        .map_err(|_| AuditStoreError::Sql)?;
    result.map_err(|_| AuditStoreError::Sql)?;
    prune_approval(c, now).await
}
pub(super) async fn require_room(c: &mut SqliteConnection) -> Result<(), AuditStoreError> {
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    let fault = test_sql::before(c, ApprovalStatement::AuditCountQueue)
        .await
        .map_err(|_| AuditStoreError::Sql)?;
    let result = sqlx::query_scalar("SELECT COUNT(*) FROM main.node_management_audit")
        .fetch_one(&mut *c)
        .await;
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    test_sql::after(c, fault, &result)
        .await
        .map_err(|_| AuditStoreError::Sql)?;
    let queue: i64 = result.map_err(|_| AuditStoreError::Sql)?;
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    let fault = test_sql::before(c, ApprovalStatement::AuditCountApproval)
        .await
        .map_err(|_| AuditStoreError::Sql)?;
    let result = sqlx::query_scalar("SELECT COUNT(*) FROM main.node_approval_audit")
        .fetch_one(&mut *c)
        .await;
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    test_sql::after(c, fault, &result)
        .await
        .map_err(|_| AuditStoreError::Sql)?;
    let approval: i64 = result.map_err(|_| AuditStoreError::Sql)?;
    let total = queue.checked_add(approval).ok_or(AuditStoreError::Full)?;
    if total >= AUDIT_LIMIT {
        return Err(AuditStoreError::Full);
    }
    Ok(())
}
