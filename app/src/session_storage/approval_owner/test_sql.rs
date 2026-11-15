//! Fixed same-connection SQLite faults. No arbitrary SQL or substitute operation result is accepted.
use super::test_control::{self, ApprovalStatement, ApprovalStatementFault};
use sqlx::SqliteConnection;
fn unavailable() -> sqlx::Error {
    sqlx::Error::Protocol("approval test controller unavailable".into())
}
pub(super) fn for_table(table: &str) -> Result<ApprovalStatement, sqlx::Error> {
    Ok(match table {
        "node_approval_requests" => ApprovalStatement::InsertRequest,
        "node_approval_associations" => ApprovalStatement::InsertAssociation,
        "node_approval_commands" => ApprovalStatement::InsertRegistration,
        "node_approval_decisions" => ApprovalStatement::InsertDecision,
        "node_approval_reply_attempts" => ApprovalStatement::InsertArm,
        "node_approval_reply_facts" => ApprovalStatement::InsertTerminal,
        "node_approval_invalidations" => ApprovalStatement::InsertInvalidation,
        _ => return Err(unavailable()),
    })
}
pub(in crate::session_storage) async fn before(
    connection: &mut SqliteConnection,
    point: ApprovalStatement,
) -> Result<Option<ApprovalStatementFault>, sqlx::Error> {
    let Some(context) = test_control::current_original_context() else {
        return Ok(None);
    };
    let Some(fault) = context
        .controller
        .take_statement_fault(point, context.tag)
        .map_err(|_| unavailable())?
    else {
        return Ok(None);
    };
    let setup=async{
  if fault.deferred(){for statement in ["CREATE TEMP TABLE _acpr_approval_parent(id INTEGER PRIMARY KEY)","CREATE TEMP TABLE _acpr_approval_child(parent INTEGER REFERENCES _acpr_approval_parent(id) DEFERRABLE INITIALLY DEFERRED)","INSERT INTO temp._acpr_approval_child(parent) VALUES(1)"]{sqlx::query(statement).execute(&mut *connection).await?;}return Ok(());}
  let statement=match point{
   ApprovalStatement::InsertRequest=>Some("CREATE TEMP TRIGGER _acpr_approval_abort BEFORE INSERT ON main.node_approval_requests BEGIN SELECT RAISE(ABORT,'approval synthetic statement fault'); END"),
   ApprovalStatement::InsertAssociation=>Some("CREATE TEMP TRIGGER _acpr_approval_abort BEFORE INSERT ON main.node_approval_associations BEGIN SELECT RAISE(ABORT,'approval synthetic statement fault'); END"),
   ApprovalStatement::InsertRegistration=>Some("CREATE TEMP TRIGGER _acpr_approval_abort BEFORE INSERT ON main.node_approval_commands BEGIN SELECT RAISE(ABORT,'approval synthetic statement fault'); END"),
   ApprovalStatement::InsertDecision=>Some("CREATE TEMP TRIGGER _acpr_approval_abort BEFORE INSERT ON main.node_approval_decisions BEGIN SELECT RAISE(ABORT,'approval synthetic statement fault'); END"),
   ApprovalStatement::AcceptCommand=>Some("CREATE TEMP TRIGGER _acpr_approval_abort BEFORE UPDATE ON main.node_approval_commands BEGIN SELECT RAISE(ABORT,'approval synthetic statement fault'); END"),
   ApprovalStatement::InsertArm=>Some("CREATE TEMP TRIGGER _acpr_approval_abort BEFORE INSERT ON main.node_approval_reply_attempts BEGIN SELECT RAISE(ABORT,'approval synthetic statement fault'); END"),
   ApprovalStatement::InsertTerminal=>Some("CREATE TEMP TRIGGER _acpr_approval_abort BEFORE INSERT ON main.node_approval_reply_facts BEGIN SELECT RAISE(ABORT,'approval synthetic statement fault'); END"),
   ApprovalStatement::InsertInvalidation=>Some("CREATE TEMP TRIGGER _acpr_approval_abort BEFORE INSERT ON main.node_approval_invalidations BEGIN SELECT RAISE(ABORT,'approval synthetic statement fault'); END"),
   ApprovalStatement::AuditInsertQueue=>Some("CREATE TEMP TRIGGER _acpr_approval_abort BEFORE INSERT ON main.node_management_audit BEGIN SELECT RAISE(ABORT,'approval synthetic statement fault'); END"),
   ApprovalStatement::AuditInsertApproval=>Some("CREATE TEMP TRIGGER _acpr_approval_abort BEFORE INSERT ON main.node_approval_audit BEGIN SELECT RAISE(ABORT,'approval synthetic statement fault'); END"),
   // DELETE/COUNT/COMMIT use the real SQLite VM interrupt because empty DELETE has no row trigger.
   _=>None,
  };
  if let Some(statement)=statement{sqlx::query(statement).execute(&mut *connection).await?;}else{let mut first=true;connection.lock_handle().await?.set_progress_handler(1,move||{let proceed=!first;first=false;proceed});}
  Ok::<(),sqlx::Error>(())
 }.await;
    if let Err(error) = setup {
        fault.observe(Some(&error), false);
        return Err(error);
    }
    Ok(Some(fault))
}
pub(in crate::session_storage) async fn after<T>(
    connection: &mut SqliteConnection,
    fault: Option<ApprovalStatementFault>,
    result: &Result<T, sqlx::Error>,
) -> Result<(), sqlx::Error> {
    if let Some(fault) = fault {
        fault.observe(result.as_ref().err(), true);
        if !fault.deferred() {
            connection.lock_handle().await?.remove_progress_handler();
        }
    }
    Ok(())
}
