#[cfg(test)]
use crate::session_storage::native_metadata_test_hooks::NativeMetadataPoint;
// Exact dispatch provenance and immutable attempt transitions in the original transaction owner.
use super::{
    Arc, AttemptMetadata, AttemptProbe, BoxFuture, ContentOrigin, DispatchCloseRecord,
    DispatchError, DispatchInspection, DispatchLifecycle, DispatchPlan, DispatchProbe, EntityId,
    FactKind, KnownStopReason, MetadataBatch, NativeDispatchRepository, NativeFactMetadata,
    ObservationMetadata, ObservationProbe, RecoverySummary, RegistryDeadline, ReleaseKind,
    ReleaseProof, ResourceLifecycle, ResultKind, RuntimeEpoch, SelectedProof, SelectionKind,
    SelectionStamp, SourceLifecycle, SqliteConnection, SqliteNativeDispatch, SqliteRow,
    StartupProof, StoredAttempt, StoredDispatch, StoredFacts, StoredTransport,
    TerminalTransportProof, TransitionKind, TransportFact, bounded, col, digest, entity,
    observation_fingerprint, optional_id, optional_u64, read_resource, read_source, reason, rpc,
    rpc_parts, transport_parts, u64_blob, wire,
};
use crate::{
    application::request_registry::OperationContext,
    domain::{
        native::{HarnessConnectionEpoch, LoadEpoch, NativeSessionKey, ResourceEpoch},
        request_registry::RegistrationId,
    },
};

fn selected(r: &SqliteRow) -> Result<Option<SelectionStamp>, DispatchError> {
    match (
        optional_id(r, "selection_id")?,
        optional_u64(r, "selection_owner_order")?,
        col::<Option<i64>>(r, "selected_at_ms")?,
    ) {
        (None, None, None) => Ok(None),
        (Some(selection_id), Some(owner_order), Some(at)) => Ok(Some(SelectionStamp {
            selection_id,
            owner_order,
            at,
        })),
        _ => Err(DispatchError::CorruptState),
    }
}
async fn read_dispatch(
    c: &mut SqliteConnection,
    p: &DispatchProbe,
) -> Result<Option<StoredDispatch>, DispatchError> {
    let row = sqlx::query("SELECT * FROM node_native_dispatches WHERE dispatch_id=?")
        .bind(p.plan().dispatch_id.encode())
        .fetch_optional(&mut *c)
        .await?;
    let Some(r) = row else { return Ok(None) };
    let source_id = entity(&r, "source_id")?;
    let native = NativeSessionKey::from_config(Some(source_id), &col::<String>(&r, "native_id")?)
        .map_err(|_| DispatchError::CorruptState)?;
    let runtime = RuntimeEpoch(entity(&r, "runtime_epoch")?);
    let plan = DispatchPlan {
        dispatch_id: entity(&r, "dispatch_id")?,
        planned_run_id: entity(&r, "planned_run_id")?,
        canonical_target: entity(&r, "target_id")?,
        native,
        runtime,
        owner_id: entity(&r, "owner_id")?,
        instance_id: entity(&r, "instance_id")?,
        resource: ResourceEpoch {
            runtime,
            connection: HarnessConnectionEpoch(entity(&r, "connection_epoch")?),
            load: LoadEpoch(entity(&r, "load_epoch")?),
        },
        item_id: entity(&r, "item_id")?,
        revision: u64_blob(&col::<Vec<u8>>(&r, "content_revision")?)?,
        origin: ContentOrigin {
            submit_request: RegistrationId::new(entity(&r, "submit_request_id")?),
            content_request: RegistrationId::new(entity(&r, "content_request_id")?),
            revision: u64_blob(&col::<Vec<u8>>(&r, "content_revision")?)?,
            digest: digest(&r, "content_digest")?.ok_or(DispatchError::CorruptState)?,
        },
        config_identity: entity(&r, "config_identity")?,
        config_revision: u64_blob(&col::<Vec<u8>>(&r, "config_revision")?)?,
        selection_kind: match col::<String>(&r, "selection_kind")?.as_str() {
            "waiting" => SelectionKind::Waiting,
            "existing_active" => SelectionKind::ExistingActive,
            _ => return Err(DispatchError::CorruptState),
        },
        prepared_at_ms: col(&r, "prepared_at_ms")?,
    };
    if &plan != p.plan() {
        return Err(DispatchError::Conflict);
    }
    let lifecycle = match col::<String>(&r, "lifecycle")?.as_str() {
        "prepared_zero" => DispatchLifecycle::PreparedZero,
        "selected" => DispatchLifecycle::Selected,
        "closed" => DispatchLifecycle::Closed,
        _ => return Err(DispatchError::CorruptState),
    };
    let close = match (
        col::<Option<String>>(&r, "close_reason")?,
        digest(&r, "close_evidence_fingerprint")?,
        col::<Option<i64>>(&r, "closed_at_ms")?,
    ) {
        (None, None, None) => None,
        (Some(reason), Some(fingerprint), Some(at)) => Some(DispatchCloseRecord {
            reason: Arc::from(reason),
            fingerprint,
            at,
        }),
        _ => return Err(DispatchError::CorruptState),
    };
    let slot: Option<String> =
        sqlx::query_scalar("SELECT dispatch_id FROM node_native_dispatch_slots WHERE target_id=?")
            .bind(plan.canonical_target.encode())
            .fetch_optional(c)
            .await?;
    let has_slot = slot.as_deref() == Some(plan.dispatch_id.encode().as_str());
    if (lifecycle == DispatchLifecycle::Closed && has_slot)
        || (lifecycle != DispatchLifecycle::Closed && !has_slot)
    {
        return Err(DispatchError::CorruptState);
    }
    Ok(Some(StoredDispatch {
        plan,
        lifecycle,
        selection: selected(&r)?,
        close,
        has_slot,
    }))
}
fn attempt(r: &SqliteRow) -> Result<StoredAttempt, DispatchError> {
    let frame_length = u64_blob(&col::<Vec<u8>>(r, "frame_length")?)?;
    if frame_length == 0 {
        return Err(DispatchError::CorruptState);
    }
    let accepted = u64_blob(&col::<Vec<u8>>(r, "accepted_length")?)?;
    let why = col::<Option<String>>(r, "transport_reason")?;
    let transport = match (col::<String>(r, "transport_state")?.as_str(), why) {
        ("armed", None) if accepted == 0 => StoredTransport::Armed,
        ("zero", Some(r)) if accepted == 0 => {
            let reason = reason(&r)?;
            transport_parts(TransportFact::Zero { reason }, frame_length)?;
            StoredTransport::Terminal(TransportFact::Zero { reason })
        }
        ("unknown", Some(r)) if accepted <= frame_length => {
            let cause = reason(&r)?;
            transport_parts(TransportFact::Unknown { accepted, cause }, frame_length)?;
            StoredTransport::Terminal(TransportFact::Unknown { accepted, cause })
        }
        ("written", None) if accepted == frame_length => {
            StoredTransport::Terminal(TransportFact::Written)
        }
        _ => return Err(DispatchError::CorruptState),
    };
    Ok(StoredAttempt {
        metadata: AttemptMetadata {
            attempt_id: entity(r, "attempt_id")?,
            dispatch_id: entity(r, "dispatch_id")?,
            selection: SelectionStamp {
                selection_id: entity(r, "selection_id")?,
                owner_order: u64_blob(&col::<Vec<u8>>(r, "selection_owner_order")?)?,
                at: col(r, "selected_at_ms")?,
            },
            item_id: entity(r, "item_id")?,
            revision: u64_blob(&col::<Vec<u8>>(r, "content_revision")?)?,
            connection_epoch: entity(r, "connection_epoch")?,
            wire: wire(col(r, "protocol_version")?)?.ok_or(DispatchError::CorruptState)?,
            rpc: rpc(r, "rpc_id_kind", "rpc_id_bytes")?.ok_or(DispatchError::CorruptState)?,
            frame_length,
            armed_at: col(r, "armed_at_ms")?,
        },
        transport,
        recorded_at: col(r, "recorded_at_ms")?,
    })
}
async fn read_attempt(
    c: &mut SqliteConnection,
    id: EntityId,
) -> Result<Option<StoredAttempt>, DispatchError> {
    sqlx::query("SELECT a.*,d.selection_owner_order,d.selected_at_ms FROM node_native_write_attempts a JOIN node_native_dispatches d USING(dispatch_id) WHERE a.attempt_id=?").bind(id.encode()).fetch_optional(c).await?.as_ref().map(attempt).transpose()
}
async fn content_origin(c: &mut SqliteConnection, p: &DispatchPlan) -> Result<(), DispatchError> {
    for (request, submit) in [
        (p.origin.submit_request, true),
        (p.origin.content_request, false),
    ] {
        let row=sqlx::query("SELECT r.*,o.phase,o.result_code,o.result_item_id,o.result_revision FROM node_request_registrations r JOIN node_request_operations o USING(request_id) WHERE request_id=?").bind(request.encode()).fetch_optional(&mut *c).await?.ok_or(DispatchError::Conflict)?;
        if col::<String>(&row, "phase")? != "settled"
            || entity(&row, "target_id")? != p.canonical_target
            || entity(&row, "result_item_id")? != p.item_id
        {
            return Err(DispatchError::Conflict);
        }
        let action: String = col(&row, "action")?;
        let result: String = col(&row, "result_code")?;
        let revision = u64_blob(&col::<Vec<u8>>(&row, "result_revision")?)?;
        if submit {
            if request.entity() != p.item_id
                || action != "input.submit"
                || !matches!(result.as_str(), "queued" | "direct_selected")
                || revision != 1
            {
                return Err(DispatchError::Conflict);
            }
        } else if request == p.origin.submit_request {
            if p.revision != 1
                || action != "input.submit"
                || !matches!(result.as_str(), "queued" | "direct_selected")
            {
                return Err(DispatchError::Conflict);
            }
        } else if action != "input.edit"
            || result != "edited"
            || optional_id(&row, "item_id")? != Some(p.item_id)
            || optional_u64(&row, "expected_revision")?.and_then(|n| n.checked_add(1))
                != Some(p.revision)
        {
            return Err(DispatchError::Conflict);
        }
        if !submit
            && (revision != p.revision || digest(&row, "body_digest")? != Some(p.origin.digest))
        {
            return Err(DispatchError::Conflict);
        }
    }
    if p.origin.revision != p.revision || p.origin.submit_request.entity() != p.item_id {
        return Err(DispatchError::Conflict);
    }
    Ok(())
}
fn attempt_matches(p: &AttemptProbe, v: &StoredAttempt) -> Result<(), DispatchError> {
    if p.metadata() != &v.metadata {
        return Err(DispatchError::Conflict);
    }
    Ok(())
}

impl NativeDispatchRepository for SqliteNativeDispatch {
    fn reserve<'a>(
        &'a self,
        p: &'a DispatchProbe,
        context: &'a OperationContext,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredDispatch, DispatchError>> {
        transaction_after_test_pause!(
            self,
            d,
            c,
            NativeMetadataPoint::DispatchReserveBeforeBegin,
            {
                let m = p.plan();
                if context.runtime != m.runtime {
                    return Err(DispatchError::Conflict);
                }
                if let Some(v) = read_dispatch(c, p).await? {
                    return Ok(v);
                }
                content_origin(c, m).await?;
                let resource = read_resource(c, p.resource().metadata())
                    .await?
                    .ok_or(DispatchError::InvalidResource)?;
                if resource.lifecycle != ResourceLifecycle::Loaded
                    || resource.transport != Some(StoredTransport::Terminal(TransportFact::Written))
                    || resource.loaded_sequence.is_none()
                    || resource.metadata.target != m.canonical_target
                    || resource.metadata.owner != m.owner_id
                    || resource.metadata.load_epoch != m.resource.load.0
                    || resource.metadata.source.instance_id != m.instance_id
                    || resource.metadata.source.runtime != m.runtime
                    || resource.metadata.source.connection_epoch != m.resource.connection.0
                    || resource.metadata.source.config_identity != m.config_identity
                    || resource.metadata.source.config_revision != m.config_revision
                {
                    return Err(DispatchError::NotReady);
                }
                let actual: Option<(String, String)> = sqlx::query_as(
                    "SELECT source_id,native_id FROM product_targets WHERE target_id=?",
                )
                .bind(m.canonical_target.encode())
                .fetch_optional(&mut *c)
                .await?;
                if actual.as_ref()
                    != Some(&(
                        m.native.source().0.encode(),
                        m.native.native_id().to_owned(),
                    ))
                {
                    return Err(DispatchError::InvalidTarget);
                }
                let occupied: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM node_native_dispatch_slots WHERE target_id=?",
                )
                .bind(m.canonical_target.encode())
                .fetch_one(&mut *c)
                .await?;
                if occupied != 0 {
                    return Err(DispatchError::Conflict);
                }
                sqlx::query("INSERT INTO node_native_dispatches(dispatch_id,planned_run_id,target_id,source_id,native_id,runtime_epoch,owner_id,instance_id,connection_epoch,load_epoch,item_id,content_revision,submit_request_id,content_request_id,content_digest,config_identity,config_revision,selection_kind,lifecycle,prepared_at_ms) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'prepared_zero',?)").bind(m.dispatch_id.encode()).bind(m.planned_run_id.encode()).bind(m.canonical_target.encode()).bind(m.native.source().0.encode()).bind(m.native.native_id()).bind(m.runtime.0.encode()).bind(m.owner_id.encode()).bind(m.instance_id.encode()).bind(m.resource.connection.0.encode()).bind(m.resource.load.0.encode()).bind(m.item_id.encode()).bind(m.revision.to_be_bytes().to_vec()).bind(m.origin.submit_request.encode()).bind(m.origin.content_request.encode()).bind(m.origin.digest.to_vec()).bind(m.config_identity.encode()).bind(m.config_revision.to_be_bytes().to_vec()).bind(match m.selection_kind{SelectionKind::Waiting=>"waiting",SelectionKind::ExistingActive=>"existing_active"}).bind(m.prepared_at_ms).execute(&mut *c).await?;
                sqlx::query(
                    "INSERT INTO node_native_dispatch_slots(target_id,dispatch_id) VALUES(?,?)",
                )
                .bind(m.canonical_target.encode())
                .bind(m.dispatch_id.encode())
                .execute(&mut *c)
                .await?;
                read_dispatch(c, p)
                    .await?
                    .ok_or(DispatchError::CorruptState)
            }
        )
    }
    fn arm<'a>(
        &'a self,
        attempt: &'a AttemptProbe,
        selection: &'a SelectedProof,
        context: &'a OperationContext,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredAttempt, DispatchError>> {
        transaction!(self, deadline, connection, {
            if !attempt.dispatch().same(selection.dispatch())
                || attempt.metadata().selection != selection.stamp()
                || context.runtime != attempt.dispatch().plan().runtime
            {
                return Err(DispatchError::Conflict);
            }
            let dispatch = read_dispatch(connection, attempt.dispatch())
                .await?
                .ok_or(DispatchError::Conflict)?;
            if dispatch.lifecycle == DispatchLifecycle::Closed {
                return Err(DispatchError::Conflict);
            }
            if let Some(v) = read_attempt(connection, attempt.metadata().attempt_id).await? {
                attempt_matches(attempt, &v)?;
                return Ok(v);
            }
            let metadata = attempt.metadata();
            let plan = attempt.dispatch().plan();
            if metadata.dispatch_id != plan.dispatch_id
                || metadata.item_id != plan.item_id
                || metadata.revision != plan.revision
                || metadata.connection_epoch != plan.resource.connection.0
                || metadata.frame_length == 0
            {
                return Err(DispatchError::Conflict);
            }
            if dispatch
                .selection
                .is_some_and(|stamp| stamp != selection.stamp())
            {
                return Err(DispatchError::Conflict);
            }
            let unresolved:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_write_attempts WHERE dispatch_id=? AND transport_state!='zero'").bind(metadata.dispatch_id.encode()).fetch_one(&mut *connection).await?;
            if unresolved != 0 {
                return Err(DispatchError::Conflict);
            }
            sqlx::query("UPDATE node_native_dispatches SET lifecycle='selected',selection_id=?,selection_owner_order=?,selected_at_ms=? WHERE dispatch_id=?").bind(selection.stamp().selection_id.encode()).bind(selection.stamp().owner_order.to_be_bytes().to_vec()).bind(selection.stamp().at).bind(metadata.dispatch_id.encode()).execute(&mut *connection).await?;
            let (rpc_kind, rpc_bytes) = rpc_parts(&metadata.rpc);
            sqlx::query("INSERT INTO node_native_write_attempts(attempt_id,dispatch_id,selection_id,item_id,content_revision,connection_epoch,protocol_version,rpc_id_kind,rpc_id_bytes,frame_length,accepted_length,transport_state,armed_at_ms) VALUES(?,?,?,?,?,?,?,?,?,?,?,'armed',?)").bind(metadata.attempt_id.encode()).bind(metadata.dispatch_id.encode()).bind(metadata.selection.selection_id.encode()).bind(metadata.item_id.encode()).bind(metadata.revision.to_be_bytes().to_vec()).bind(metadata.connection_epoch.encode()).bind(metadata.wire.number()).bind(rpc_kind).bind(rpc_bytes).bind(metadata.frame_length.to_be_bytes().to_vec()).bind(0_u64.to_be_bytes().to_vec()).bind(metadata.armed_at).execute(&mut *connection).await?;
            read_attempt(connection, metadata.attempt_id)
                .await?
                .ok_or(DispatchError::CorruptState)
        })
    }
    fn record_transport<'a>(
        &'a self,
        p: &'a AttemptProbe,
        f: &'a TerminalTransportProof,
        context: &'a OperationContext,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredAttempt, DispatchError>> {
        transaction!(self, d, c, {
            if !p.dispatch().same(f.attempt().dispatch())
                || p.metadata() != f.attempt().metadata()
                || context.runtime != p.dispatch().plan().runtime
            {
                return Err(DispatchError::Conflict);
            }
            let v = read_attempt(c, p.metadata().attempt_id)
                .await?
                .ok_or(DispatchError::Conflict)?;
            attempt_matches(p, &v)?;
            if let StoredTransport::Terminal(current) = v.transport {
                if current == f.fact() && v.recorded_at == Some(f.at()) {
                    return Ok(v);
                }
                return Err(DispatchError::Conflict);
            }
            let (state, accepted, why) = transport_parts(f.fact(), p.metadata().frame_length)?;
            sqlx::query("UPDATE node_native_write_attempts SET transport_state=?,accepted_length=?,transport_reason=?,recorded_at_ms=? WHERE attempt_id=?").bind(state).bind(accepted.to_be_bytes().to_vec()).bind(why).bind(f.at()).bind(p.metadata().attempt_id.encode()).execute(&mut *c).await?;
            read_attempt(c, p.metadata().attempt_id)
                .await?
                .ok_or(DispatchError::CorruptState)
        })
    }
    fn record_observation<'a>(
        &'a self,
        p: &'a ObservationProbe,
        b: &'a MetadataBatch,
        context: &'a OperationContext,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredFacts, DispatchError>> {
        transaction_after_test_pause!(self, d, c, NativeMetadataPoint::ObservationBeforeBegin, {
            record_batch(c, p, b, context).await
        })
    }
    fn close<'a>(
        &'a self,
        p: &'a DispatchProbe,
        f: &'a ReleaseProof,
        context: &'a OperationContext,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredDispatch, DispatchError>> {
        transaction!(self, d, c, {
            if !p.same(f.dispatch()) || context.runtime != p.plan().runtime {
                return Err(DispatchError::Conflict);
            }
            let v = read_dispatch(c, p).await?.ok_or(DispatchError::Conflict)?;
            if v.lifecycle == DispatchLifecycle::Closed {
                if v.close.as_ref()
                    == Some(&DispatchCloseRecord {
                        reason: Arc::from(f.reason().label()),
                        fingerprint: *f.fingerprint(),
                        at: f.at(),
                    })
                {
                    return Ok(v);
                }
                return Err(DispatchError::Conflict);
            }
            let rows=sqlx::query("SELECT a.*,d.selection_owner_order,d.selected_at_ms FROM node_native_write_attempts a JOIN node_native_dispatches d USING(dispatch_id) WHERE a.dispatch_id=?").bind(p.plan().dispatch_id.encode()).fetch_all(&mut *c).await?;
            let attempts = rows.iter().map(attempt).collect::<Result<Vec<_>, _>>()?;
            let resource = read_resource(c, p.resource().metadata())
                .await?
                .ok_or(DispatchError::InvalidResource)?;
            match f.reason() {
                ReleaseKind::KnownZero => {
                    if attempts.iter().any(|a| {
                        !matches!(
                            a.transport,
                            StoredTransport::Terminal(TransportFact::Zero { .. })
                        )
                    }) {
                        return Err(DispatchError::Conflict);
                    }
                }
                ReleaseKind::RestartZero => return Err(DispatchError::Conflict),
                ReleaseKind::NativeSettled => {
                    if f.requirements().is_empty() {
                        return Err(DispatchError::RequiredStateUnavailable);
                    }
                    if attempts.is_empty()
                        || attempts.iter().any(|a| {
                            matches!(
                                a.transport,
                                StoredTransport::Armed
                                    | StoredTransport::Terminal(TransportFact::Unknown { .. })
                            )
                        })
                    {
                        return Err(DispatchError::Conflict);
                    }
                    for requirement in f.requirements() {
                        let count:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_dispatch_facts WHERE requirement_id=? AND target_id=? AND load_epoch=? AND owner_id=?").bind(requirement.encode()).bind(p.plan().canonical_target.encode()).bind(p.plan().resource.load.0.encode()).bind(p.plan().owner_id.encode()).fetch_one(&mut *c).await?;
                        if count != 1 {
                            return Err(DispatchError::RequiredStateUnavailable);
                        }
                    }
                }
                ReleaseKind::ResourcesConverged => {
                    if resource.lifecycle != ResourceLifecycle::Released
                        || resource.release.is_none()
                    {
                        return Err(DispatchError::ResourceUnconfirmed);
                    }
                }
            }
            sqlx::query("UPDATE node_native_dispatches SET lifecycle='closed',close_reason=?,close_evidence_fingerprint=?,closed_at_ms=? WHERE dispatch_id=?").bind(f.reason().label()).bind(f.fingerprint().to_vec()).bind(f.at()).bind(p.plan().dispatch_id.encode()).execute(&mut *c).await?;
            sqlx::query(
                "DELETE FROM node_native_dispatch_slots WHERE target_id=? AND dispatch_id=?",
            )
            .bind(p.plan().canonical_target.encode())
            .bind(p.plan().dispatch_id.encode())
            .execute(&mut *c)
            .await?;
            read_dispatch(c, p)
                .await?
                .ok_or(DispatchError::CorruptState)
        })
    }
    fn inspect<'a>(
        &'a self,
        p: &'a DispatchProbe,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<Option<DispatchInspection>, DispatchError>> {
        transaction!(self, d, c, {
            let Some(dispatch) = read_dispatch(c, p).await? else {
                return Ok(None);
            };
            let rows=sqlx::query("SELECT a.*,d.selection_owner_order,d.selected_at_ms FROM node_native_write_attempts a JOIN node_native_dispatches d USING(dispatch_id) WHERE a.dispatch_id=? ORDER BY a.attempt_id").bind(p.plan().dispatch_id.encode()).fetch_all(c).await?;
            Ok(Some(DispatchInspection {
                dispatch,
                attempts: rows.iter().map(attempt).collect::<Result<Vec<_>, _>>()?,
            }))
        })
    }
    fn interrupt_previous_runtime<'a>(
        &'a self,
        p: &'a StartupProof,
        context: &'a OperationContext,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<RecoverySummary, DispatchError>> {
        transaction!(self, d, c, {
            if !p.owns_runtime(context.runtime) {
                return Err(DispatchError::NodeStopping);
            }
            let rows=sqlx::query("SELECT * FROM node_native_dispatches WHERE runtime_epoch!=? AND lifecycle='prepared_zero' AND NOT EXISTS(SELECT 1 FROM node_native_write_attempts a WHERE a.dispatch_id=node_native_dispatches.dispatch_id)").bind(context.runtime.0.encode()).fetch_all(&mut *c).await?;
            let mut zero = 0_u64;
            for row in rows {
                let fingerprint = restart_fingerprint(&row, context)?;
                let dispatch_id = entity(&row, "dispatch_id")?;
                zero=zero.checked_add(sqlx::query("UPDATE node_native_dispatches SET lifecycle='closed',close_reason='restart_zero',close_evidence_fingerprint=?,closed_at_ms=? WHERE dispatch_id=?").bind(fingerprint.to_vec()).bind(context.observed_at_ms).bind(dispatch_id.encode()).execute(&mut *c).await?.rows_affected()).ok_or(DispatchError::SequenceExhausted)?;
                sqlx::query("DELETE FROM node_native_dispatch_slots WHERE dispatch_id=?")
                    .bind(dispatch_id.encode())
                    .execute(&mut *c)
                    .await?;
            }
            let unknown=sqlx::query("UPDATE node_native_write_attempts SET transport_state='unknown',transport_reason='restart',recorded_at_ms=? WHERE transport_state='armed' AND dispatch_id IN(SELECT dispatch_id FROM node_native_dispatches WHERE runtime_epoch!=?)").bind(context.observed_at_ms).bind(context.runtime.0.encode()).execute(&mut *c).await?.rows_affected();
            Ok(RecoverySummary {
                zero_closed: zero,
                unknown_marked: unknown,
            })
        })
    }
}

fn restart_fingerprint(
    r: &SqliteRow,
    context: &OperationContext,
) -> Result<[u8; 32], DispatchError> {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"native-restart-zero-v1");
    for name in [
        "dispatch_id",
        "planned_run_id",
        "target_id",
        "source_id",
        "native_id",
        "runtime_epoch",
        "owner_id",
        "instance_id",
        "connection_epoch",
        "load_epoch",
        "item_id",
        "submit_request_id",
        "content_request_id",
        "config_identity",
        "selection_kind",
    ] {
        let v: String = col(r, name)?;
        h.update(
            u64::try_from(v.len())
                .map_err(|_| DispatchError::CorruptState)?
                .to_be_bytes(),
        );
        h.update(v.as_bytes());
    }
    for name in ["content_revision", "config_revision", "content_digest"] {
        let v: Vec<u8> = col(r, name)?;
        if v.len() != if name == "content_digest" { 32 } else { 8 } {
            return Err(DispatchError::CorruptState);
        }
        h.update(v);
    }
    h.update(col::<i64>(r, "prepared_at_ms")?.to_be_bytes());
    h.update(context.runtime.0.encode().as_bytes());
    h.update(context.observed_at_ms.to_be_bytes());
    Ok(h.finalize().into())
}
fn observation_header(r: &SqliteRow) -> Result<ObservationMetadata, DispatchError> {
    Ok(ObservationMetadata {
        observation_id: entity(r, "observation_id")?,
        source_id: entity(r, "source_id")?,
        runtime: RuntimeEpoch(entity(r, "runtime_epoch")?),
        instance_id: entity(r, "instance_id")?,
        connection_epoch: entity(r, "connection_epoch")?,
        sequence: u64_blob(&col::<Vec<u8>>(r, "observation_sequence")?)?,
        receipt: u64_blob(&col::<Vec<u8>>(r, "receipt_order")?)?,
        observed_at: col(r, "observed_at_ms")?,
        fingerprint: digest(r, "batch_fingerprint")?.ok_or(DispatchError::CorruptState)?,
    })
}
fn fact_kind(v: &str) -> Result<FactKind, DispatchError> {
    match v {
        "v1_prompt_result" => Ok(FactKind::V1PromptResult),
        "v2_inserted" => Ok(FactKind::V2Inserted),
        "prompt_rejected" => Ok(FactKind::PromptRejected),
        "foreground_running" => Ok(FactKind::ForegroundRunning),
        "foreground_requires_action" => Ok(FactKind::ForegroundRequiresAction),
        "foreground_idle" => Ok(FactKind::ForegroundIdle),
        "foreground_unknown" => Ok(FactKind::ForegroundUnknown),
        "foreground_terminal" => Ok(FactKind::ForegroundTerminal),
        "approval_present" => Ok(FactKind::ApprovalPresent),
        "approval_retired" => Ok(FactKind::ApprovalRetired),
        _ => Err(DispatchError::CorruptState),
    }
}
fn result_kind(v: &str) -> Result<ResultKind, DispatchError> {
    match v {
        "completed" => Ok(ResultKind::Completed),
        "stopped" => Ok(ResultKind::Stopped),
        "failed" => Ok(ResultKind::Failed),
        "rejected" => Ok(ResultKind::Rejected),
        "unknown" => Ok(ResultKind::Unknown),
        _ => Err(DispatchError::CorruptState),
    }
}
fn fact(r: &SqliteRow) -> Result<NativeFactMetadata, DispatchError> {
    let reason = col::<Option<String>>(r, "native_stop_reason")?
        .map(|v| {
            let reason = KnownStopReason::classify(Some(&v));
            if reason == KnownStopReason::Unknown && v != "unknown" {
                Err(DispatchError::CorruptState)
            } else {
                Ok(reason)
            }
        })
        .transpose()?;
    Ok(NativeFactMetadata {
        requirement: entity(r, "requirement_id")?,
        target: entity(r, "target_id")?,
        owner: entity(r, "owner_id")?,
        load_epoch: entity(r, "load_epoch")?,
        owner_order: u64_blob(&col::<Vec<u8>>(r, "owner_order")?)?,
        attempt: optional_id(r, "attempt_id")?,
        kind: fact_kind(&col::<String>(r, "fact_kind")?)?,
        message: col::<Option<String>>(r, "native_message_id")?.map(Arc::from),
        result: col::<Option<String>>(r, "result_kind")?
            .as_deref()
            .map(result_kind)
            .transpose()?,
        reason,
        transition: match col::<String>(r, "transition_kind")?.as_str() {
            "new" => TransitionKind::New,
            "snapshot" => TransitionKind::Snapshot,
            "repeated" => TransitionKind::Repeated,
            "unknown" => TransitionKind::Unknown,
            _ => return Err(DispatchError::CorruptState),
        },
        interval: optional_u64(r, "interval_serial")?,
        request: rpc(r, "native_request_id_kind", "native_request_id_bytes")?,
    })
}
async fn existing_batch(
    c: &mut SqliteConnection,
    header: &ObservationMetadata,
) -> Result<Option<MetadataBatch>, DispatchError> {
    let row=sqlx::query("SELECT * FROM node_native_observations WHERE observation_id=? OR (connection_epoch=? AND observation_sequence=?)").bind(header.observation_id.encode()).bind(header.connection_epoch.encode()).bind(header.sequence.to_be_bytes().to_vec()).fetch_all(&mut *c).await?;
    if row.len() > 1 {
        return Err(DispatchError::Conflict);
    }
    let Some(row) = row.first() else {
        return Ok(None);
    };
    let stored = observation_header(row)?;
    let rows = sqlx::query(
        "SELECT * FROM node_native_dispatch_facts WHERE observation_id=? ORDER BY fact_index",
    )
    .bind(stored.observation_id.encode())
    .fetch_all(c)
    .await?;
    let mut facts = Vec::new();
    for (index, r) in rows.iter().enumerate() {
        if u64_blob(&col::<Vec<u8>>(r, "fact_index")?)?
            != u64::try_from(index).map_err(|_| DispatchError::CorruptState)?
        {
            return Err(DispatchError::CorruptState);
        }
        facts.push(fact(r)?);
    }
    if observation_fingerprint(&stored, &facts)? != stored.fingerprint {
        return Err(DispatchError::CorruptState);
    }
    Ok(Some(MetadataBatch {
        header: stored,
        facts,
    }))
}
async fn record_batch(
    connection: &mut SqliteConnection,
    probe: &ObservationProbe,
    batch: &MetadataBatch,
    context: &OperationContext,
) -> Result<StoredFacts, DispatchError> {
    if probe.header() != &batch.header
        || context.runtime != batch.header.runtime
        || observation_fingerprint(&batch.header, &batch.facts)? != batch.header.fingerprint
    {
        return Err(DispatchError::Conflict);
    }
    let source = read_source(connection, probe.source().metadata())
        .await?
        .ok_or(DispatchError::InvalidSource)?;
    if source.metadata.source_id != batch.header.source_id
        || source.metadata.runtime != batch.header.runtime
        || source.metadata.instance_id != batch.header.instance_id
        || source.metadata.connection_epoch != batch.header.connection_epoch
        || source.scope_binding.is_none()
    {
        return Err(DispatchError::InvalidScope);
    }
    if let Some(stored) = existing_batch(connection, &batch.header).await? {
        if stored == *batch {
            return Ok(StoredFacts { batch: stored });
        }
        return Err(DispatchError::Conflict);
    }
    if source.lifecycle == SourceLifecycle::Closed {
        return Err(DispatchError::Conflict);
    }
    for fact in &batch.facts {
        let resource=sqlx::query("SELECT owner_id,target_id,instance_id,connection_epoch,runtime_epoch FROM node_native_resource_bindings WHERE load_epoch=?").bind(fact.load_epoch.encode()).fetch_optional(&mut *connection).await?.ok_or(DispatchError::InvalidResource)?;
        if entity(&resource, "owner_id")? != fact.owner
            || entity(&resource, "target_id")? != fact.target
            || entity(&resource, "instance_id")? != batch.header.instance_id
            || entity(&resource, "connection_epoch")? != batch.header.connection_epoch
            || entity(&resource, "runtime_epoch")? != batch.header.runtime.0
        {
            return Err(DispatchError::InvalidResource);
        }
        if let Some(attempt) = fact.attempt {
            let scope=sqlx::query("SELECT d.* FROM node_native_write_attempts a JOIN node_native_dispatches d USING(dispatch_id) WHERE a.attempt_id=?").bind(attempt.encode()).fetch_optional(&mut *connection).await?.ok_or(DispatchError::Conflict)?;
            if entity(&scope, "target_id")? != fact.target
                || entity(&scope, "load_epoch")? != fact.load_epoch
                || entity(&scope, "owner_id")? != fact.owner
                || entity(&scope, "source_id")? != batch.header.source_id
                || entity(&scope, "instance_id")? != batch.header.instance_id
                || entity(&scope, "connection_epoch")? != batch.header.connection_epoch
                || entity(&scope, "runtime_epoch")? != batch.header.runtime.0
            {
                return Err(DispatchError::Conflict);
            }
        }
        let used: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM node_native_dispatch_facts WHERE requirement_id=?",
        )
        .bind(fact.requirement.encode())
        .fetch_one(&mut *connection)
        .await?;
        if used != 0 {
            return Err(DispatchError::Conflict);
        }
    }
    let header = &batch.header;
    sqlx::query("INSERT INTO node_native_observations(observation_id,source_id,runtime_epoch,instance_id,connection_epoch,observation_sequence,receipt_order,batch_fingerprint,observed_at_ms) VALUES(?,?,?,?,?,?,?,?,?)").bind(header.observation_id.encode()).bind(header.source_id.encode()).bind(header.runtime.0.encode()).bind(header.instance_id.encode()).bind(header.connection_epoch.encode()).bind(header.sequence.to_be_bytes().to_vec()).bind(header.receipt.to_be_bytes().to_vec()).bind(header.fingerprint.to_vec()).bind(header.observed_at).execute(&mut *connection).await?;
    for (index, fact) in batch.facts.iter().enumerate() {
        let (kind, bytes) = fact.request.as_ref().map_or((None, None), |row| {
            let (k, batch) = rpc_parts(row);
            (Some(k), Some(batch))
        });
        sqlx::query("INSERT INTO node_native_dispatch_facts(observation_id,fact_index,requirement_id,target_id,owner_id,load_epoch,owner_order,attempt_id,fact_kind,native_message_id,native_request_id_kind,native_request_id_bytes,result_kind,native_stop_reason,transition_kind,interval_serial) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(header.observation_id.encode()).bind(u64::try_from(index).map_err(|_|DispatchError::LengthUnrepresentable)?.to_be_bytes().to_vec()).bind(fact.requirement.encode()).bind(fact.target.encode()).bind(fact.owner.encode()).bind(fact.load_epoch.encode()).bind(fact.owner_order.to_be_bytes().to_vec()).bind(fact.attempt.map(crate::domain::EntityId::encode)).bind(fact.kind.label()).bind(fact.message.as_deref()).bind(kind).bind(bytes).bind(fact.result.map(ResultKind::label)).bind(fact.reason.map(KnownStopReason::label)).bind(fact.transition.label()).bind(fact.interval.map(|v|v.to_be_bytes().to_vec())).execute(&mut *connection).await?;
    }
    let stored = existing_batch(connection, &batch.header)
        .await?
        .ok_or(DispatchError::CorruptState)?;
    if stored != *batch {
        return Err(DispatchError::CorruptState);
    }
    Ok(StoredFacts { batch: stored })
}
