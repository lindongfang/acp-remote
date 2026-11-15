//! Exact SQL set comparison is subordinate to the original physical/quiescence proof.
use super::{
    ApprovalAssociationRecord, ApprovalAttemptRecord, ApprovalCommandRecord,
    ApprovalDecisionRecord, ApprovalInvalidationReason, ApprovalInvalidationRecord,
    ApprovalRequestRecord, ApprovalTerminalRecord, SqliteConnection, rows::ApprovalRow,
};
use crate::application::native_dispatch::{ApprovalClosureManifest, PreSourceProbe};
use crate::domain::native_dispatch::DispatchError;
async fn source_rows<T: ApprovalRow>(
    c: &mut SqliteConnection,
    instance: String,
) -> Result<Vec<T>, DispatchError> {
    let query = if T::TABLE == "node_approval_requests" {
        format!(
            "SELECT t.* FROM main.{} t WHERE t.instance_id=? ORDER BY t.{}",
            T::TABLE,
            T::PRIMARY
        )
    } else {
        format!(
            "SELECT t.* FROM main.{} t JOIN main.node_approval_requests r ON r.approval_id=t.approval_id WHERE r.instance_id=? ORDER BY t.{}",
            T::TABLE,
            T::PRIMARY
        )
    };
    sqlx::query(&query)
        .bind(instance)
        .fetch_all(c)
        .await?
        .iter()
        .map(|row| T::decode(row).map_err(|_| DispatchError::CorruptState))
        .collect()
}
pub(in crate::session_storage) async fn verify(
    c: &mut SqliteConnection,
    source: &PreSourceProbe,
    manifest: &ApprovalClosureManifest,
) -> Result<(), DispatchError> {
    super::super::schema::validate_approval_necessary(c)
        .await
        .map_err(|_| DispatchError::CorruptState)?;
    let instance = source.metadata().instance_id.encode();
    if source_rows::<ApprovalRequestRecord>(c, instance.clone()).await? != manifest.requests()
        || source_rows::<ApprovalAssociationRecord>(c, instance.clone()).await?
            != manifest.associations()
        || source_rows::<ApprovalCommandRecord>(c, instance.clone()).await? != manifest.commands()
        || source_rows::<ApprovalDecisionRecord>(c, instance.clone()).await? != manifest.decisions()
        || source_rows::<ApprovalAttemptRecord>(c, instance.clone()).await? != manifest.attempts()
        || source_rows::<ApprovalTerminalRecord>(c, instance.clone()).await? != manifest.terminals()
        || source_rows::<ApprovalInvalidationRecord>(c, instance).await? != manifest.invalidations()
    {
        return Err(DispatchError::Conflict);
    }
    for request in manifest.requests() {
        if request.source_id != source.metadata().source_id
            || request.runtime != source.metadata().runtime.0
            || request.connection_epoch != source.metadata().connection_epoch
            || !manifest.invalidations().iter().any(|fact| {
                fact.approval == request.approval
                    && fact.reason == ApprovalInvalidationReason::SourceClosed
            })
        {
            return Err(DispatchError::Conflict);
        }
    }
    Ok(())
}
