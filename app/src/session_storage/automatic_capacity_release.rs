//! Internal single-native system operations; production tree capture is a separate gate.
use super::session_capacity::decode;
use crate::{
    application::native_dispatch::{
        AutomaticReleaseOutcome, AutomaticReleasePhase, AutomaticReleaseReason,
        AutomaticReleaseRecord, CapacityRecord,
    },
    domain::{EntityId, native_dispatch::DispatchError},
};
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

const fn reason(value: AutomaticReleaseReason) -> &'static str {
    match value {
        AutomaticReleaseReason::IdleTimeout => "idle_timeout",
        AutomaticReleaseReason::CapacityPressure => "capacity_pressure",
    }
}
const fn outcome(value: AutomaticReleaseOutcome) -> &'static str {
    match value {
        AutomaticReleaseOutcome::Requested => "requested",
        AutomaticReleaseOutcome::Failed => "failed",
        AutomaticReleaseOutcome::Unknown => "unknown",
        AutomaticReleaseOutcome::Withdrawn => "withdrawn",
    }
}
fn decode_operation(row: &SqliteRow) -> Result<AutomaticReleaseRecord, DispatchError> {
    Ok(AutomaticReleaseRecord {
        operation: EntityId::decode(&row.try_get::<String, _>("operation_id")?)
            .map_err(|_| DispatchError::CorruptState)?,
        original: decode(row)?,
        reason: match row.try_get::<String, _>("reason")?.as_str() {
            "idle_timeout" => AutomaticReleaseReason::IdleTimeout,
            "capacity_pressure" => AutomaticReleaseReason::CapacityPressure,
            _ => return Err(DispatchError::CorruptState),
        },
        phase: match row.try_get::<String, _>("phase")?.as_str() {
            "pending" => AutomaticReleasePhase::Pending,
            "finished" => AutomaticReleasePhase::Finished,
            "withdrawn" => AutomaticReleasePhase::Withdrawn,
            _ => return Err(DispatchError::CorruptState),
        },
        outcome: match row.try_get::<Option<String>, _>("outcome")?.as_deref() {
            None => None,
            Some("requested") => Some(AutomaticReleaseOutcome::Requested),
            Some("failed") => Some(AutomaticReleaseOutcome::Failed),
            Some("unknown") => Some(AutomaticReleaseOutcome::Unknown),
            Some("withdrawn") => Some(AutomaticReleaseOutcome::Withdrawn),
            _ => return Err(DispatchError::CorruptState),
        },
    })
}
async fn read(
    c: &mut SqliteConnection,
    operation: EntityId,
) -> Result<Option<AutomaticReleaseRecord>, DispatchError> {
    sqlx::query("SELECT o.operation_id,o.reason,o.phase,o.outcome,c.* FROM main.node_session_automatic_releases o JOIN main.node_session_capacity c ON c.reservation_id=o.anchor_reservation WHERE o.operation_id=?")
        .bind(operation.encode()).fetch_optional(c).await?.as_ref().map(decode_operation).transpose()
}
async fn require_original(
    c: &mut SqliteConnection,
    record: &CapacityRecord,
    operation: EntityId,
) -> Result<AutomaticReleaseRecord, DispatchError> {
    let original = read(c, operation).await?.ok_or(DispatchError::Conflict)?;
    if !original.original.same_reservation(record) {
        return Err(DispatchError::Conflict);
    }
    require_members(c, &original).await?;
    Ok(original)
}
async fn require_members(
    c: &mut SqliteConnection,
    original: &AutomaticReleaseRecord,
) -> Result<(), DispatchError> {
    let anchor = &original.original;
    let rows = sqlx::query("SELECT c.* FROM main.node_session_automatic_release_members m JOIN main.node_session_capacity c ON c.reservation_id=m.reservation_id WHERE m.operation_id=?")
        .bind(original.operation.encode()).fetch_all(c).await?;
    let mut contains_anchor = false;
    for row in &rows {
        let member = decode(row)?;
        contains_anchor |= member.same_reservation(anchor);
        if member.native != anchor.native
            || member.source != anchor.source
            || member.instance != anchor.instance
            || member.runtime != anchor.runtime
        {
            return Err(DispatchError::CorruptState);
        }
        if original.phase == AutomaticReleasePhase::Finished
            && (!member.physically_closed || row.try_get::<i64, _>("released")? != 1)
        {
            return Err(DispatchError::CorruptState);
        }
    }
    if !contains_anchor || anchor.native.is_none() {
        return Err(DispatchError::CorruptState);
    }
    Ok(())
}

pub(super) async fn begin(
    c: &mut SqliteConnection,
    record: &CapacityRecord,
    operation: EntityId,
    why: AutomaticReleaseReason,
) -> Result<AutomaticReleaseRecord, DispatchError> {
    if let Some(original) = read(c, operation).await? {
        if !original.original.same_reservation(record) || original.reason != why {
            return Err(DispatchError::Conflict);
        }
        require_members(c, &original).await?;
        return Ok(original);
    }
    let row = sqlx::query("SELECT c.* FROM main.node_session_capacity c JOIN main.node_native_source_instances s ON s.instance_id=c.instance_id AND s.source_id=c.source_id AND s.runtime_epoch=c.runtime_epoch WHERE c.reservation_id=? AND c.released=0 AND c.physically_closed=0 AND c.native_id IS NOT NULL AND s.lifecycle!='closed'")
        .bind(record.id.encode()).fetch_optional(&mut *c).await?.ok_or(DispatchError::Conflict)?;
    let anchor = decode(&row)?;
    if !anchor.same_reservation(record) {
        return Err(DispatchError::Conflict);
    }
    let native = anchor.native.as_ref().ok_or(DispatchError::Conflict)?;
    let root: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_session_capacity WHERE source_id=? AND native_id=? AND root_verified=1)")
        .bind(anchor.source.encode()).bind(native.native_id()).fetch_one(&mut *c).await?;
    if root != 1 {
        return Err(DispatchError::SessionTreeUnverified);
    }
    let conflicting: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_session_capacity WHERE source_id=? AND native_id=? AND released=0 AND (instance_id!=? OR runtime_epoch!=? OR physically_closed!=0)) OR EXISTS(SELECT 1 FROM main.node_native_resource_bindings b JOIN main.product_targets t ON t.target_id=b.target_id WHERE t.source_id=? AND t.native_id=? AND b.lifecycle!='released' AND (b.instance_id!=? OR b.runtime_epoch!=?))")
        .bind(anchor.source.encode()).bind(native.native_id()).bind(anchor.instance.encode()).bind(anchor.runtime.0.encode())
        .bind(anchor.source.encode()).bind(native.native_id()).bind(anchor.instance.encode()).bind(anchor.runtime.0.encode())
        .fetch_one(&mut *c).await?;
    if conflicting != 0 {
        return Err(DispatchError::Conflict);
    }
    sqlx::query("INSERT INTO main.node_session_automatic_releases(operation_id,anchor_reservation,reason,phase,outcome) VALUES(?,?,?,'pending',NULL)")
        .bind(operation.encode()).bind(anchor.id.encode()).bind(reason(why)).execute(&mut *c).await?;
    sqlx::query("INSERT INTO main.node_session_automatic_release_members(operation_id,reservation_id) SELECT ?,reservation_id FROM main.node_session_capacity WHERE source_id=? AND native_id=? AND instance_id=? AND runtime_epoch=? AND released=0")
        .bind(operation.encode()).bind(anchor.source.encode()).bind(native.native_id()).bind(anchor.instance.encode()).bind(anchor.runtime.0.encode()).execute(&mut *c).await?;
    require_create_converged(c, operation).await?;
    require_original(c, record, operation).await
}

pub(super) async fn record_outcome(
    c: &mut SqliteConnection,
    record: &CapacityRecord,
    operation: EntityId,
    observation: AutomaticReleaseOutcome,
) -> Result<AutomaticReleaseRecord, DispatchError> {
    let current = require_original(c, record, operation).await?;
    if current.outcome == Some(observation) {
        return Ok(current);
    }
    if current.phase != AutomaticReleasePhase::Pending
        || !matches!(
            (current.outcome, observation),
            (None, _)
                | (
                    Some(AutomaticReleaseOutcome::Requested),
                    AutomaticReleaseOutcome::Failed | AutomaticReleaseOutcome::Unknown
                )
        )
    {
        return Err(DispatchError::Conflict);
    }
    if observation == AutomaticReleaseOutcome::Withdrawn {
        // The original ticket's Zero/not-enqueued evidence belongs to the caller.
        // SQL only verifies that this intent has not observed resource closure.
        let closed: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_session_automatic_release_members m JOIN main.node_session_capacity c ON c.reservation_id=m.reservation_id JOIN main.node_native_source_instances s ON s.instance_id=c.instance_id WHERE m.operation_id=? AND (c.released!=0 OR c.physically_closed!=0 OR s.lifecycle='closed'))")
            .bind(operation.encode()).fetch_one(&mut *c).await?;
        if closed != 0 {
            return Err(DispatchError::Conflict);
        }
    }
    let phase = if observation == AutomaticReleaseOutcome::Withdrawn {
        "withdrawn"
    } else {
        "pending"
    };
    sqlx::query(
        "UPDATE main.node_session_automatic_releases SET phase=?,outcome=? WHERE operation_id=?",
    )
    .bind(phase)
    .bind(outcome(observation))
    .bind(operation.encode())
    .execute(&mut *c)
    .await?;
    require_original(c, record, operation).await
}

async fn require_create_converged(
    c: &mut SqliteConnection,
    operation: EntityId,
) -> Result<(), DispatchError> {
    let pending: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_session_automatic_release_members m JOIN main.node_session_capacity c ON c.reservation_id=m.reservation_id LEFT JOIN main.node_source_session_operations o ON o.operation_id=c.create_operation WHERE m.operation_id=? AND c.create_operation IS NOT NULL AND (o.operation_state IS NOT 'terminal' OR o.outcome IS NOT 'succeeded' OR o.response_kind IS NOT 'created' OR o.ack_state IS NOT 'confirmed' OR o.native_session_id IS NOT c.native_id OR o.source_instance_id IS NOT c.instance_id OR o.runtime_epoch IS NOT c.runtime_epoch))")
        .bind(operation.encode()).fetch_one(c).await?;
    if pending != 0 {
        return Err(DispatchError::ResourceUnconfirmed);
    }
    Ok(())
}

pub(super) async fn finish(
    c: &mut SqliteConnection,
    record: &CapacityRecord,
    operation: EntityId,
) -> Result<AutomaticReleaseRecord, DispatchError> {
    let current = require_original(c, record, operation).await?;
    match current.phase {
        AutomaticReleasePhase::Finished => return Ok(current),
        AutomaticReleasePhase::Withdrawn => return Err(DispatchError::Conflict),
        AutomaticReleasePhase::Pending => {}
    }
    let pending: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_session_automatic_release_members m JOIN main.node_session_capacity c ON c.reservation_id=m.reservation_id WHERE m.operation_id=? AND (c.physically_closed!=1 OR (c.released=0 AND EXISTS(SELECT 1 FROM main.node_native_resource_bindings b JOIN main.product_targets t ON t.target_id=b.target_id WHERE t.source_id=c.source_id AND t.native_id=c.native_id AND b.instance_id=c.instance_id AND b.runtime_epoch=c.runtime_epoch AND b.lifecycle!='released'))))")
        .bind(operation.encode()).fetch_one(&mut *c).await?;
    if pending != 0 {
        return Err(DispatchError::ResourceUnconfirmed);
    }
    require_create_converged(c, operation).await?;
    sqlx::query(
        "UPDATE main.node_session_automatic_releases SET phase='finished' WHERE operation_id=?",
    )
    .bind(operation.encode())
    .execute(&mut *c)
    .await?;
    sqlx::query("UPDATE main.node_session_capacity SET released=1 WHERE released=0 AND EXISTS(SELECT 1 FROM main.node_session_automatic_release_members m WHERE m.operation_id=? AND m.reservation_id=node_session_capacity.reservation_id)")
        .bind(operation.encode()).execute(&mut *c).await?;
    require_original(c, record, operation).await
}
