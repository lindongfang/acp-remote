#[cfg(test)]
use crate::session_storage::native_metadata_test_hooks::NativeMetadataPoint;
// Control metadata is phase-immutable and cannot reconstruct an original protocol request.
use super::{
    BoxFuture, ControlInspection, ControlIntentKind, ControlKind, ControlMetadata, ControlPhase,
    ControlRecordProbe, ControlRecordProof, DispatchError, NativeControlRepository,
    RegistryDeadline, ResourceLifecycle, SourceLifecycle, SqliteConnection, SqliteNativeDispatch,
    SqliteRow, StoredControl, TransportFact, bounded, col, control_fingerprint, digest, entity,
    optional_id, optional_u64, read_resource, read_source, reason, rpc, rpc_parts, transport_parts,
    u64_blob,
};
use crate::application::request_registry::OperationContext;

fn control_kind(v: &str) -> Result<ControlKind, DispatchError> {
    match v {
        "native_cancel" => Ok(ControlKind::NativeCancel),
        "permission_cancelled" => Ok(ControlKind::PermissionCancelled),
        "unsupported" => Ok(ControlKind::Unsupported),
        _ => Err(DispatchError::CorruptState),
    }
}
fn intent_kind(v: &str) -> Result<ControlIntentKind, DispatchError> {
    match v {
        "priority" => Ok(ControlIntentKind::Priority),
        "shutdown" => Ok(ControlIntentKind::Shutdown),
        "unsupported" => Ok(ControlIntentKind::Unsupported),
        _ => Err(DispatchError::CorruptState),
    }
}
async fn decode_control(
    c: &mut SqliteConnection,
    r: &SqliteRow,
    expected: &ControlMetadata,
) -> Result<StoredControl, DispatchError> {
    let source = read_source(c, &expected.source)
        .await?
        .ok_or(DispatchError::InvalidSource)?;
    if entity(r, "source_id")? != source.metadata.source_id
        || entity(r, "runtime_epoch")? != source.metadata.runtime.0
        || entity(r, "instance_id")? != source.metadata.instance_id
        || entity(r, "connection_epoch")? != source.metadata.connection_epoch
    {
        return Err(DispatchError::Conflict);
    }
    let resource = match (
        optional_id(r, "target_id")?,
        optional_id(r, "load_epoch")?,
        optional_id(r, "owner_id")?,
    ) {
        (None, None, None) => None,
        (Some(target), Some(load), Some(owner)) => {
            let expected = expected.resource.as_ref().ok_or(DispatchError::Conflict)?;
            if expected.target != target || expected.load_epoch != load || expected.owner != owner {
                return Err(DispatchError::Conflict);
            }
            Some(
                read_resource(c, expected)
                    .await?
                    .ok_or(DispatchError::InvalidResource)?
                    .metadata,
            )
        }
        _ => return Err(DispatchError::CorruptState),
    };
    let frame_length = optional_u64(r, "frame_length")?;
    let accepted = u64_blob(&col::<Vec<u8>>(r, "accepted_length")?)?;
    let state: String = col(r, "transport_state")?;
    let reason_string: Option<String> = col(r, "transport_reason")?;
    let phase = match (
        col::<String>(r, "phase")?.as_str(),
        state.as_str(),
        reason_string,
    ) {
        ("reserved", "reserved", None) if accepted == 0 && frame_length.is_some() => {
            ControlPhase::Reserved
        }
        ("terminal", "zero", Some(v)) if accepted == 0 => {
            let fact = TransportFact::Zero {
                reason: reason(&v)?,
            };
            transport_parts(fact, frame_length.unwrap_or(0))?;
            ControlPhase::Terminal(fact)
        }
        ("terminal", "unknown", Some(v)) => {
            let frame_length = frame_length.ok_or(DispatchError::CorruptState)?;
            let fact = TransportFact::Unknown {
                accepted,
                cause: reason(&v)?,
            };
            transport_parts(fact, frame_length)?;
            ControlPhase::Terminal(fact)
        }
        ("terminal", "written", None) if frame_length == Some(accepted) && accepted > 0 => {
            ControlPhase::Terminal(TransportFact::Written)
        }
        _ => return Err(DispatchError::CorruptState),
    };
    let metadata = ControlMetadata {
        control_id: entity(r, "control_id")?,
        obligation_id: entity(r, "obligation_id")?,
        attempt_serial: u64_blob(&col::<Vec<u8>>(r, "attempt_serial")?)?,
        requirement: entity(r, "requirement_id")?,
        source: source.metadata,
        scope_binding: entity(r, "scope_binding_id")?,
        resource,
        work_serial: optional_u64(r, "work_serial")?,
        intent: intent_kind(&col::<String>(r, "intent_kind")?)?,
        intent_reference: entity(r, "intent_reference")?,
        kind: control_kind(&col::<String>(r, "control_kind")?)?,
        rpc: rpc(r, "rpc_id_kind", "rpc_id_bytes")?,
        frame_length,
        phase,
        fingerprint: digest(r, "metadata_fingerprint")?.ok_or(DispatchError::CorruptState)?,
        at: col(r, "observed_at_ms")?,
    };
    if control_fingerprint(&metadata)? != metadata.fingerprint {
        return Err(DispatchError::CorruptState);
    }
    Ok(StoredControl { metadata })
}
fn same_operation(a: &ControlMetadata, b: &ControlMetadata) -> bool {
    a.control_id == b.control_id
        && a.obligation_id == b.obligation_id
        && a.attempt_serial == b.attempt_serial
        && a.source == b.source
        && a.scope_binding == b.scope_binding
        && a.resource == b.resource
        && a.work_serial == b.work_serial
        && a.intent == b.intent
        && a.intent_reference == b.intent_reference
        && a.kind == b.kind
        && a.rpc == b.rpc
        && a.frame_length == b.frame_length
}
async fn control_inspection(
    c: &mut SqliteConnection,
    m: &ControlMetadata,
) -> Result<ControlInspection, DispatchError> {
    let rows =
        sqlx::query("SELECT * FROM node_native_control_facts WHERE control_id=? ORDER BY phase")
            .bind(m.control_id.encode())
            .fetch_all(&mut *c)
            .await?;
    let mut result = ControlInspection {
        reserved: None,
        terminal: None,
    };
    for row in rows {
        let fact = decode_control(c, &row, m).await?;
        if !same_operation(&fact.metadata, m) {
            return Err(DispatchError::Conflict);
        }
        match fact.metadata.phase {
            ControlPhase::Reserved => result.reserved = Some(fact),
            ControlPhase::Terminal(_) => result.terminal = Some(fact),
        }
    }
    Ok(result)
}
impl NativeControlRepository for SqliteNativeDispatch {
    #[expect(
        clippy::too_many_lines,
        reason = "All original control identity and phase checks precede the single transactional fact insert."
    )]
    fn record_control<'a>(
        &'a self,
        proof: &'a ControlRecordProof,
        context: &'a OperationContext,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredControl, DispatchError>> {
        transaction_after_test_pause!(
            self,
            deadline,
            connection,
            if matches!(proof.metadata().phase, ControlPhase::Reserved) {
                NativeMetadataPoint::ControlReservedBeforeBegin
            } else {
                NativeMetadataPoint::ControlTerminalBeforeBegin
            },
            {
                let metadata = proof.metadata();
                if context.runtime != metadata.source.runtime
                    || control_fingerprint(metadata)? != metadata.fingerprint
                {
                    return Err(DispatchError::Conflict);
                }
                let current = control_inspection(connection, metadata).await?;
                let same_phase = match metadata.phase {
                    ControlPhase::Reserved => current.reserved,
                    ControlPhase::Terminal(_) => current.terminal,
                };
                if let Some(stored) = same_phase {
                    if &stored.metadata == metadata {
                        return Ok(stored);
                    }
                    return Err(DispatchError::Conflict);
                }
                let source = read_source(connection, &metadata.source)
                    .await?
                    .ok_or(DispatchError::InvalidSource)?;
                if source.scope_binding != Some(metadata.scope_binding)
                    || source.lifecycle == SourceLifecycle::Closed
                {
                    return Err(DispatchError::InvalidScope);
                }
                match (
                    metadata.kind,
                    metadata.intent,
                    &metadata.resource,
                    &metadata.rpc,
                ) {
                    (ControlKind::Unsupported, ControlIntentKind::Unsupported, None, Some(_))
                    | (
                        ControlKind::PermissionCancelled,
                        ControlIntentKind::Shutdown,
                        None,
                        Some(_),
                    ) if metadata.work_serial.is_none() => {}
                    (
                        ControlKind::NativeCancel,
                        ControlIntentKind::Priority | ControlIntentKind::Shutdown,
                        Some(resource),
                        None,
                    )
                    | (
                        ControlKind::PermissionCancelled,
                        ControlIntentKind::Priority | ControlIntentKind::Shutdown,
                        Some(resource),
                        Some(_),
                    ) => {
                        let actual = read_resource(connection, resource)
                            .await?
                            .ok_or(DispatchError::InvalidResource)?;
                        if actual.lifecycle == ResourceLifecycle::Released
                            || resource.source != metadata.source
                        {
                            return Err(DispatchError::InvalidResource);
                        }
                    }
                    _ => return Err(DispatchError::Conflict),
                }
                if metadata.attempt_serial > 0 {
                    let previous=sqlx::query("SELECT * FROM node_native_control_facts WHERE obligation_id=? AND attempt_serial=? AND phase='terminal'").bind(metadata.obligation_id.encode()).bind((metadata.attempt_serial-1).to_be_bytes().to_vec()).fetch_optional(&mut *connection).await?.ok_or(DispatchError::RequiredStateUnavailable)?;
                    let previous = decode_control(connection, &previous, metadata)
                        .await?
                        .metadata;
                    if !matches!(
                        previous.phase,
                        ControlPhase::Terminal(TransportFact::Zero { .. })
                    ) || previous.source != metadata.source
                        || previous.scope_binding != metadata.scope_binding
                        || previous.resource != metadata.resource
                        || previous.work_serial != metadata.work_serial
                        || previous.intent != metadata.intent
                        || previous.intent_reference != metadata.intent_reference
                        || previous.kind != metadata.kind
                        || previous.rpc != metadata.rpc
                    {
                        return Err(DispatchError::Conflict);
                    }
                }
                let (state, accepted, why) = match metadata.phase {
                    ControlPhase::Reserved => {
                        if metadata.frame_length.is_none_or(|n| n == 0) {
                            return Err(DispatchError::Conflict);
                        }
                        ("reserved", 0, None)
                    }
                    ControlPhase::Terminal(fact) => {
                        if metadata.frame_length.is_none()
                            && !matches!(fact, TransportFact::Zero { .. })
                        {
                            return Err(DispatchError::Conflict);
                        }
                        transport_parts(fact, metadata.frame_length.unwrap_or(0))?
                    }
                };
                let duplicate:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_control_facts WHERE requirement_id=? OR (obligation_id=? AND attempt_serial=? AND phase=?)").bind(metadata.requirement.encode()).bind(metadata.obligation_id.encode()).bind(metadata.attempt_serial.to_be_bytes().to_vec()).bind(metadata.phase.label()).fetch_one(&mut *connection).await?;
                if duplicate != 0 {
                    return Err(DispatchError::Conflict);
                }
                let (rpc_kind, rpc_bytes) = metadata.rpc.as_ref().map_or((None, None), |rpc| {
                    let (rpc_kind, rpc_bytes) = rpc_parts(rpc);
                    (Some(rpc_kind), Some(rpc_bytes))
                });
                sqlx::query("INSERT INTO node_native_control_facts(control_id,phase,obligation_id,attempt_serial,requirement_id,source_id,runtime_epoch,instance_id,connection_epoch,scope_binding_id,target_id,owner_id,load_epoch,work_serial,intent_kind,intent_reference,control_kind,rpc_id_kind,rpc_id_bytes,frame_length,transport_state,transport_reason,accepted_length,metadata_fingerprint,observed_at_ms) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(metadata.control_id.encode()).bind(metadata.phase.label()).bind(metadata.obligation_id.encode()).bind(metadata.attempt_serial.to_be_bytes().to_vec()).bind(metadata.requirement.encode()).bind(metadata.source.source_id.encode()).bind(metadata.source.runtime.0.encode()).bind(metadata.source.instance_id.encode()).bind(metadata.source.connection_epoch.encode()).bind(metadata.scope_binding.encode()).bind(metadata.resource.as_ref().map(|r|r.target.encode())).bind(metadata.resource.as_ref().map(|r|r.owner.encode())).bind(metadata.resource.as_ref().map(|r|r.load_epoch.encode())).bind(metadata.work_serial.map(|v|v.to_be_bytes().to_vec())).bind(metadata.intent.label()).bind(metadata.intent_reference.encode()).bind(metadata.kind.label()).bind(rpc_kind).bind(rpc_bytes).bind(metadata.frame_length.map(|v|v.to_be_bytes().to_vec())).bind(state).bind(why).bind(accepted.to_be_bytes().to_vec()).bind(metadata.fingerprint.to_vec()).bind(metadata.at).execute(&mut *connection).await?;
                let stored = control_inspection(connection, metadata).await?;
                match metadata.phase {
                    ControlPhase::Reserved => stored.reserved,
                    ControlPhase::Terminal(_) => stored.terminal,
                }
                .ok_or(DispatchError::CorruptState)
            }
        )
    }
    fn inspect_control<'a>(
        &'a self,
        p: &'a ControlRecordProbe,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<ControlInspection, DispatchError>> {
        transaction!(self, d, c, { control_inspection(c, p.metadata()).await })
    }
}
