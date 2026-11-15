#![allow(
    clippy::too_many_lines,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
#[cfg(test)]
use crate::session_storage::native_metadata_test_hooks::NativeMetadataPoint;
// Stage-exact closure updates only the original source/resource set; loss is separate.
use super::{
    Arc, BoxFuture, ConvergenceProbe, DispatchError, EntityId, IoCloseKind, LossTargetMetadata,
    NativeClosureRepository, NativeOwnerKind, OwnerCloseKind, PreResourceProbe, PreSourceProbe,
    RegistryDeadline, ResourceCloseBasis, ResourceCloseStage, ResourceClosureProof,
    ResourceLifecycle, ResourceRecord, SourceCloseBasis, SourceCloseStage, SourceClosureProof,
    SourceInspection, SourceLifecycle, SourceLossProbe, SourceTerminalReason, SqliteConnection,
    SqliteNativeDispatch, StoredConvergence, StoredResourceClosure, StoredSourceClosed,
    StoredSourceClosure, StoredSourceLoss, StoredTransport, TransportFact, bounded, col, digest,
    entity, read_resource, read_source, u64_blob,
};
use crate::application::native_dispatch::{OwnerClosureWitness, SourceCreationCut};
use crate::application::request_registry::OperationContext;

fn validate_pre_io_closure(
    p: &PreSourceProbe,
    f: &SourceClosureProof,
) -> Result<(), DispatchError> {
    let source = f
        .expected()
        .source
        .as_ref()
        .ok_or(DispatchError::InvalidSource)?;
    let witness_matches = match (f.basis(), f.owner(), f.owner_witness()) {
        (
            SourceCloseBasis::EnteredNeverCreated,
            OwnerCloseKind::NoProcessCreated,
            Some(OwnerClosureWitness::NeverCreated(proof)),
        ) => {
            matches!(
                (proof.creation_cut(), f.stage()),
                (
                    SourceCreationCut::BeforeCreateCall,
                    SourceCloseStage::EnteredBeforeCreate
                ) | (
                    SourceCreationCut::CreateCallRejected,
                    SourceCloseStage::CreateRejected
                )
            )
        }
        (
            SourceCloseBasis::OwnedConverged,
            OwnerCloseKind::SuspendedRootExited,
            Some(OwnerClosureWitness::SuspendedUnassigned(_)),
        ) => f.stage() == SourceCloseStage::OwnedBeforeIo,
        _ => false,
    };
    if !witness_matches
        || p.metadata().owner_kind != NativeOwnerKind::WindowsJob
        || f.io() != IoCloseKind::NotPrepared
        || !matches!(
            source.lifecycle,
            SourceLifecycle::Reserved | SourceLifecycle::Unknown
        )
        || source.scope_binding.is_some()
        || source.scope_bound_at.is_some()
        || source.wire.is_some()
        || source.initialize_rpc.is_some()
        || source.initialize_sequence.is_some()
        || source.initialize_receipt.is_some()
        || source.evidence.is_some()
    {
        return Err(DispatchError::ResourceUnconfirmed);
    }
    if f.expected().resources.iter().any(|row| {
        !matches!(
            row.lifecycle,
            ResourceLifecycle::Reserved | ResourceLifecycle::Unknown
        ) || row.scope_binding.is_some()
            || row.wire.is_some()
            || row.load_rpc.is_some()
            || row.frame_length.is_some()
            || row.transport.is_some()
            || row.loaded_sequence.is_some()
            || row.loaded_receipt.is_some()
            || row.evidence.is_some()
    }) {
        return Err(DispatchError::Conflict);
    }
    Ok(())
}
async fn ensure_no_native_effects(
    c: &mut SqliteConnection,
    instance: EntityId,
) -> Result<(), DispatchError> {
    let effects:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM node_native_dispatches WHERE instance_id=?)+(SELECT count(*) FROM node_native_write_attempts w JOIN node_native_dispatches d ON d.dispatch_id=w.dispatch_id WHERE d.instance_id=?)+(SELECT count(*) FROM node_native_observations WHERE instance_id=?)+(SELECT count(*) FROM node_native_dispatch_facts f JOIN node_native_observations o ON o.observation_id=f.observation_id WHERE o.instance_id=?)+(SELECT count(*) FROM node_native_source_losses WHERE instance_id=?)+(SELECT count(*) FROM node_native_loss_targets t JOIN node_native_source_losses l ON l.loss_id=t.loss_id WHERE l.instance_id=?)+(SELECT count(*) FROM node_native_control_facts WHERE instance_id=?)+(SELECT count(*) FROM node_native_convergences WHERE instance_id=?)")
        .bind(instance.encode()).bind(instance.encode()).bind(instance.encode()).bind(instance.encode())
        .bind(instance.encode()).bind(instance.encode()).bind(instance.encode()).bind(instance.encode()).fetch_one(c).await?;
    if effects != 0 {
        return Err(DispatchError::Conflict);
    }
    Ok(())
}

async fn inspect_set(
    c: &mut SqliteConnection,
    p: &PreSourceProbe,
    resources: &[Arc<PreResourceProbe>],
) -> Result<SourceInspection, DispatchError> {
    let source = read_source(c, p.metadata()).await?;
    let ids:Vec<String>=sqlx::query_scalar("SELECT load_epoch FROM node_native_resource_bindings WHERE instance_id=? ORDER BY load_epoch").bind(p.metadata().instance_id.encode()).fetch_all(&mut *c).await?;
    let mut records = Vec::new();
    for load in ids {
        let pre = resources
            .iter()
            .find(|r| r.metadata().load_epoch.encode() == load)
            .ok_or(DispatchError::Conflict)?;
        if pre.metadata().source != *p.metadata() {
            return Err(DispatchError::InvalidSource);
        }
        records.push(
            read_resource(c, pre.metadata())
                .await?
                .ok_or(DispatchError::CorruptState)?,
        );
    }
    Ok(SourceInspection {
        source,
        resources: records,
    })
}
async fn exact_expected_set(
    c: &mut SqliteConnection,
    p: &PreSourceProbe,
    expected: &SourceInspection,
) -> Result<SourceInspection, DispatchError> {
    let source = read_source(c, p.metadata()).await?;
    let ids:Vec<String>=sqlx::query_scalar("SELECT load_epoch FROM node_native_resource_bindings WHERE instance_id=? ORDER BY load_epoch").bind(p.metadata().instance_id.encode()).fetch_all(&mut *c).await?;
    let mut records = Vec::new();
    for load in ids {
        let m = expected
            .resources
            .iter()
            .find(|r| r.metadata.load_epoch.encode() == load)
            .ok_or(DispatchError::Conflict)?;
        records.push(
            read_resource(c, &m.metadata)
                .await?
                .ok_or(DispatchError::CorruptState)?,
        );
    }
    Ok(SourceInspection {
        source,
        resources: records,
    })
}
const fn resource_stage(
    v: &ResourceRecord,
    basis: SourceCloseBasis,
    stage: SourceCloseStage,
) -> &'static str {
    if matches!(basis, SourceCloseBasis::EnteredNeverCreated) {
        stage.label()
    } else if matches!(basis, SourceCloseBasis::NeverLaunched) {
        "never_entered"
    } else if v.loaded_sequence.is_some() {
        "loaded"
    } else if v.load_rpc.is_some() {
        "loading"
    } else if v.scope_binding.is_some() {
        "bound_no_target_io"
    } else {
        "reserved_no_target_io"
    }
}
fn projected_source_close(f: &SourceClosureProof) -> Result<SourceInspection, DispatchError> {
    let mut expected = f.expected().clone();
    let source = expected
        .source
        .as_mut()
        .ok_or(DispatchError::InvalidSource)?;
    source.lifecycle = SourceLifecycle::Closed;
    source.close = Some(StoredSourceClosure {
        basis: Arc::from(f.basis().label()),
        stage: Arc::from(f.stage().label()),
        io: Arc::from(f.io().label()),
        owner: Arc::from(f.owner().label()),
        active_processes: f.owner().active_processes(),
        fingerprint: *f.fingerprint(),
    });
    source.updated_at = f.at();
    for row in &mut expected.resources {
        if row.lifecycle != ResourceLifecycle::Released {
            let stage = resource_stage(row, f.basis(), f.stage());
            row.lifecycle = ResourceLifecycle::Released;
            row.release = Some(StoredResourceClosure {
                basis: Arc::from(f.basis().label()),
                stage: Arc::from(stage),
                fingerprint: *f.fingerprint(),
            });
            row.updated_at = f.at();
        }
    }
    Ok(expected)
}
impl NativeClosureRepository for SqliteNativeDispatch {
    fn inspect_source_resources<'a>(
        &'a self,
        p: &'a PreSourceProbe,
        resources: &'a [Arc<PreResourceProbe>],
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<SourceInspection, DispatchError>> {
        transaction!(self, d, c, { inspect_set(c, p, resources).await })
    }
    fn record_source_closed<'a>(
        &'a self,
        p: &'a PreSourceProbe,
        f: &'a SourceClosureProof,
        context: &'a OperationContext,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredSourceClosed, DispatchError>> {
        source_approval_transaction(
            self,
            p,
            d,
            transaction_after_test_pause!(
                self,
                d,
                c,
                NativeMetadataPoint::SourceClosedBeforeBegin,
                {
                    if !p.same(f.source()) || context.runtime != p.metadata().runtime {
                        return Err(DispatchError::Conflict);
                    }
                    crate::session_storage::approval_owner::closure::verify(c, p, f.approvals())
                        .await?;
                    let actual = exact_expected_set(c, p, f.expected()).await?;
                    let source = actual.source.as_ref().ok_or(DispatchError::InvalidSource)?;
                    let pre_io = f.basis() == SourceCloseBasis::EnteredNeverCreated
                        || f.owner() == OwnerCloseKind::SuspendedRootExited;
                    if pre_io {
                        validate_pre_io_closure(p, f)?;
                        ensure_no_native_effects(c, p.metadata().instance_id).await?;
                    }
                    if source.lifecycle == SourceLifecycle::Closed {
                        if actual != projected_source_close(f)? {
                            return Err(DispatchError::Conflict);
                        }
                        return Ok(StoredSourceClosed {
                            source: source.clone(),
                            resources: actual.resources,
                        });
                    }
                    if &actual != f.expected() {
                        return Err(DispatchError::Conflict);
                    }
                    let never = f.basis() == SourceCloseBasis::NeverLaunched;
                    if never {
                        if f.stage() != SourceCloseStage::NeverEntered
                            || f.io() != IoCloseKind::NotApplicable
                            || f.owner() != OwnerCloseKind::NotApplicable
                            || source.lifecycle != SourceLifecycle::Reserved
                            || source.scope_binding.is_some()
                            || source.initialize_rpc.is_some()
                            || source.wire.is_some()
                        {
                            return Err(DispatchError::Conflict);
                        }
                        if actual.resources.iter().any(|r| {
                            r.lifecycle != ResourceLifecycle::Reserved
                                || r.scope_binding.is_some()
                                || r.load_rpc.is_some()
                                || r.transport.is_some()
                                || r.loaded_sequence.is_some()
                        }) {
                            return Err(DispatchError::Conflict);
                        }
                        let effects:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM node_native_dispatches WHERE instance_id=?)+(SELECT count(*) FROM node_native_source_losses WHERE instance_id=?)+(SELECT count(*) FROM node_native_control_facts WHERE instance_id=?)").bind(p.metadata().instance_id.encode()).bind(p.metadata().instance_id.encode()).bind(p.metadata().instance_id.encode()).fetch_one(&mut *c).await?;
                        if effects != 0 {
                            return Err(DispatchError::Conflict);
                        }
                    } else if !pre_io {
                        let valid = matches!(
                            (p.metadata().owner_kind, f.owner()),
                            (
                                NativeOwnerKind::WindowsJob,
                                OwnerCloseKind::WindowsJob {
                                    active_processes: 0,
                                },
                            ) | (
                                NativeOwnerKind::TestOwnedStream,
                                OwnerCloseKind::TestOwnedStream
                            )
                        );
                        if !valid
                            || f.owner_witness().is_some()
                            || matches!(f.stage(), SourceCloseStage::NeverEntered)
                            || matches!(f.io(), IoCloseKind::NotApplicable)
                        {
                            return Err(DispatchError::ResourceUnconfirmed);
                        }
                    }
                    let mut resources = Vec::new();
                    for row in &actual.resources {
                        if row.lifecycle == ResourceLifecycle::Released {
                            // A previously isolated target keeps its original independently verified basis.
                            resources.push(row.clone());
                            continue;
                        }
                        sqlx::query("UPDATE node_native_resource_bindings SET lifecycle='released',release_basis=?,release_stage=?,release_evidence_fingerprint=?,updated_at_ms=? WHERE load_epoch=? AND instance_id=?").bind(f.basis().label()).bind(resource_stage(row,f.basis(),f.stage())).bind(f.fingerprint().to_vec()).bind(f.at()).bind(row.metadata.load_epoch.encode()).bind(p.metadata().instance_id.encode()).execute(&mut *c).await?;
                        resources.push(
                            read_resource(c, &row.metadata)
                                .await?
                                .ok_or(DispatchError::CorruptState)?,
                        );
                    }
                    sqlx::query("UPDATE node_native_source_instances SET lifecycle='closed',close_basis=?,close_stage=?,close_io_evidence=?,close_owner_evidence=?,close_job_active=?,close_evidence_fingerprint=?,updated_at_ms=? WHERE instance_id=?").bind(f.basis().label()).bind(f.stage().label()).bind(f.io().label()).bind(f.owner().label()).bind(f.owner().active_processes().map(i64::from)).bind(f.fingerprint().to_vec()).bind(f.at()).bind(p.metadata().instance_id.encode()).execute(&mut *c).await?;
                    Ok(StoredSourceClosed {
                        source: read_source(c, p.metadata())
                            .await?
                            .ok_or(DispatchError::CorruptState)?,
                        resources,
                    })
                }
            ),
        )
    }
    fn record_resource_closed<'a>(
        &'a self,
        p: &'a PreResourceProbe,
        f: &'a ResourceClosureProof,
        context: &'a OperationContext,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<ResourceRecord, DispatchError>> {
        transaction!(self, d, c, {
            if !p.same(f.resource()) || context.runtime != p.metadata().source.runtime {
                return Err(DispatchError::Conflict);
            }
            let row = read_resource(c, p.metadata())
                .await?
                .ok_or(DispatchError::InvalidResource)?;
            if row.lifecycle == ResourceLifecycle::Released {
                let mut expected = f.expected().clone();
                expected.lifecycle = ResourceLifecycle::Released;
                expected.release = Some(StoredResourceClosure {
                    basis: Arc::from(f.basis().label()),
                    stage: Arc::from(f.stage().label()),
                    fingerprint: *f.fingerprint(),
                });
                expected.updated_at = f.at();
                if row == expected {
                    return Ok(row);
                }
                return Err(DispatchError::Conflict);
            }
            if &row != f.expected() {
                return Err(DispatchError::Conflict);
            }
            match f.basis() {
                ResourceCloseBasis::UnloadedReservationAbandoned => {
                    let source = read_source(c, &p.metadata().source)
                        .await?
                        .ok_or(DispatchError::InvalidSource)?;
                    if matches!(
                        source.lifecycle,
                        SourceLifecycle::Reserved
                            | SourceLifecycle::Unknown
                            | SourceLifecycle::Closed
                    ) {
                        return Err(DispatchError::ResourceUnconfirmed);
                    }
                    if row.loaded_sequence.is_some()
                        || row.loaded_receipt.is_some()
                        || !matches!(
                            row.transport,
                            None | Some(StoredTransport::Terminal(TransportFact::Zero { .. }))
                        )
                        || !matches!(
                            f.stage(),
                            ResourceCloseStage::ReservedNoTargetIo
                                | ResourceCloseStage::BoundNoTargetIo
                        )
                    {
                        return Err(DispatchError::ResourceUnconfirmed);
                    }
                    let activity:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM node_native_dispatches WHERE load_epoch=? AND lifecycle!='closed')+(SELECT count(*) FROM node_native_dispatch_facts p WHERE p.load_epoch=? AND p.fact_kind='approval_present' AND NOT EXISTS(SELECT 1 FROM node_native_dispatch_facts r WHERE r.load_epoch=p.load_epoch AND r.fact_kind='approval_retired' AND r.native_request_id_kind=p.native_request_id_kind AND r.native_request_id_bytes=p.native_request_id_bytes AND r.owner_order>p.owner_order) AND NOT EXISTS(SELECT 1 FROM node_native_control_facts r WHERE r.load_epoch=p.load_epoch AND r.control_kind='permission_cancelled' AND r.phase='terminal' AND r.transport_state='written' AND r.rpc_id_kind=p.native_request_id_kind AND r.rpc_id_bytes=p.native_request_id_bytes))").bind(p.metadata().load_epoch.encode()).bind(p.metadata().load_epoch.encode()).fetch_one(&mut *c).await?;
                    if activity != 0 {
                        return Err(DispatchError::ResourceUnconfirmed);
                    }
                    let last_state:Option<String>=sqlx::query_scalar("SELECT fact_kind FROM node_native_dispatch_facts WHERE load_epoch=? AND fact_kind IN('foreground_running','foreground_requires_action','foreground_idle','foreground_unknown','foreground_terminal') ORDER BY owner_order DESC,fact_index DESC LIMIT 1").bind(p.metadata().load_epoch.encode()).fetch_optional(&mut *c).await?;
                    if last_state.as_deref().is_some_and(|s| {
                        matches!(
                            s,
                            "foreground_running"
                                | "foreground_requires_action"
                                | "foreground_unknown"
                        )
                    }) {
                        return Err(DispatchError::ResourceUnconfirmed);
                    }
                }
                ResourceCloseBasis::TargetNativeConverged => {
                    let count:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_convergences WHERE target_id=? AND load_epoch=? AND owner_id=? AND instance_id=? AND evidence_kind='native_target_closed_verified' AND evidence_fingerprint=?").bind(p.metadata().target.encode()).bind(p.metadata().load_epoch.encode()).bind(p.metadata().owner.encode()).bind(p.metadata().source.instance_id.encode()).bind(f.fingerprint().to_vec()).fetch_one(&mut *c).await?;
                    if count != 1 {
                        return Err(DispatchError::ResourceUnconfirmed);
                    }
                }
            }
            sqlx::query("UPDATE node_native_resource_bindings SET lifecycle='released',release_basis=?,release_stage=?,release_evidence_fingerprint=?,updated_at_ms=? WHERE load_epoch=?").bind(f.basis().label()).bind(f.stage().label()).bind(f.fingerprint().to_vec()).bind(f.at()).bind(p.metadata().load_epoch.encode()).execute(&mut *c).await?;
            read_resource(c, p.metadata())
                .await?
                .ok_or(DispatchError::CorruptState)
        })
    }
    fn record_source_loss<'a>(
        &'a self,
        p: &'a SourceLossProbe,
        context: &'a OperationContext,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredSourceLoss, DispatchError>> {
        transaction!(self, d, c, {
            if context.runtime != p.source().metadata().runtime {
                return Err(DispatchError::Conflict);
            }
            let s = read_source(c, p.source().metadata())
                .await?
                .ok_or(DispatchError::InvalidSource)?;
            if s.scope_binding != Some(p.binding()) {
                return Err(DispatchError::InvalidScope);
            }
            let reason = match p.reason() {
                SourceTerminalReason::Eof => "eof",
                SourceTerminalReason::Io => "io",
                SourceTerminalReason::Protocol => "protocol",
                SourceTerminalReason::Stopped => "stopped",
                SourceTerminalReason::ObservationFault => "observation_fault",
            };
            let existing = sqlx::query("SELECT * FROM node_native_source_losses WHERE loss_id=?")
                .bind(p.loss_id().encode())
                .fetch_optional(&mut *c)
                .await?;
            if let Some(row) = existing {
                if entity(&row, "source_requirement_id")? != p.requirement()
                    || entity(&row, "source_id")? != s.metadata.source_id
                    || entity(&row, "runtime_epoch")? != s.metadata.runtime.0
                    || entity(&row, "connection_epoch")? != s.metadata.connection_epoch
                    || entity(&row, "instance_id")? != p.source().metadata().instance_id
                    || entity(&row, "scope_binding_id")? != p.binding()
                    || col::<String>(&row, "reason")? != reason
                    || digest(&row, "metadata_fingerprint")? != Some(*p.fingerprint())
                    || col::<i64>(&row, "observed_at_ms")? != p.at()
                {
                    return Err(DispatchError::Conflict);
                }
                let targets = loss_targets(c, p.loss_id()).await?;
                if targets != p.targets() {
                    return Err(DispatchError::Conflict);
                }
                return Ok(StoredSourceLoss {
                    loss_id: p.loss_id(),
                    requirement: p.requirement(),
                    targets,
                    fingerprint: *p.fingerprint(),
                    at: p.at(),
                });
            }
            if s.lifecycle == SourceLifecycle::Closed {
                return Err(DispatchError::Conflict);
            }
            sqlx::query("INSERT INTO node_native_source_losses(loss_id,source_requirement_id,source_id,runtime_epoch,instance_id,connection_epoch,scope_binding_id,reason,metadata_fingerprint,observed_at_ms) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(p.loss_id().encode()).bind(p.requirement().encode()).bind(s.metadata.source_id.encode()).bind(s.metadata.runtime.0.encode()).bind(s.metadata.instance_id.encode()).bind(s.metadata.connection_epoch.encode()).bind(p.binding().encode()).bind(reason).bind(p.fingerprint().to_vec()).bind(p.at()).execute(&mut *c).await?;
            for target in p.targets() {
                let r=sqlx::query("SELECT owner_id,instance_id,target_id FROM node_native_resource_bindings WHERE load_epoch=?").bind(target.load_epoch.encode()).fetch_one(&mut *c).await?;
                if entity(&r, "owner_id")? != target.owner
                    || entity(&r, "instance_id")? != s.metadata.instance_id
                    || entity(&r, "target_id")? != target.target
                {
                    return Err(DispatchError::Conflict);
                }
                sqlx::query("INSERT INTO node_native_loss_targets(loss_id,target_id,load_epoch,owner_id,owner_order,requirement_id,metadata_fingerprint) VALUES(?,?,?,?,?,?,?)").bind(p.loss_id().encode()).bind(target.target.encode()).bind(target.load_epoch.encode()).bind(target.owner.encode()).bind(target.owner_order.to_be_bytes().to_vec()).bind(target.requirement.encode()).bind(target.fingerprint.to_vec()).execute(&mut *c).await?;
                sqlx::query("UPDATE node_native_resource_bindings SET lifecycle='unknown',updated_at_ms=? WHERE load_epoch=? AND lifecycle!='released'").bind(p.at()).bind(target.load_epoch.encode()).execute(&mut *c).await?;
            }
            sqlx::query("UPDATE node_native_source_instances SET lifecycle='unknown',updated_at_ms=? WHERE instance_id=?").bind(p.at()).bind(s.metadata.instance_id.encode()).execute(&mut *c).await?;
            Ok(StoredSourceLoss {
                loss_id: p.loss_id(),
                requirement: p.requirement(),
                targets: p.targets().to_vec(),
                fingerprint: *p.fingerprint(),
                at: p.at(),
            })
        })
    }
    fn record_convergence<'a>(
        &'a self,
        p: &'a ConvergenceProbe,
        context: &'a OperationContext,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredConvergence, DispatchError>> {
        transaction!(self, d, c, {
            let m = p.metadata();
            if m.resource.source.runtime != context.runtime {
                return Err(DispatchError::Conflict);
            }
            read_resource(c, &m.resource)
                .await?
                .ok_or(DispatchError::InvalidResource)?;
            let existing =
                sqlx::query("SELECT * FROM node_native_convergences WHERE convergence_id=?")
                    .bind(m.id.encode())
                    .fetch_optional(&mut *c)
                    .await?;
            if let Some(row) = existing {
                if entity(&row, "requirement_id")? != m.requirement
                    || entity(&row, "target_id")? != m.resource.target
                    || entity(&row, "source_id")? != m.resource.source.source_id
                    || entity(&row, "runtime_epoch")? != m.resource.source.runtime.0
                    || entity(&row, "owner_id")? != m.resource.owner
                    || entity(&row, "instance_id")? != m.resource.source.instance_id
                    || entity(&row, "connection_epoch")? != m.resource.source.connection_epoch
                    || entity(&row, "load_epoch")? != m.resource.load_epoch
                    || u64_blob(&col::<Vec<u8>>(&row, "owner_order")?)? != m.owner_order
                    || col::<String>(&row, "evidence_kind")? != m.kind.label()
                    || digest(&row, "evidence_fingerprint")? != Some(m.fingerprint)
                    || col::<i64>(&row, "verified_at_ms")? != m.at
                {
                    return Err(DispatchError::Conflict);
                }
                return Ok(StoredConvergence {
                    metadata: m.clone(),
                });
            }
            sqlx::query("INSERT INTO node_native_convergences(convergence_id,requirement_id,target_id,source_id,runtime_epoch,owner_id,instance_id,connection_epoch,load_epoch,owner_order,evidence_kind,evidence_fingerprint,verified_at_ms) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(m.id.encode()).bind(m.requirement.encode()).bind(m.resource.target.encode()).bind(m.resource.source.source_id.encode()).bind(m.resource.source.runtime.0.encode()).bind(m.resource.owner.encode()).bind(m.resource.source.instance_id.encode()).bind(m.resource.source.connection_epoch.encode()).bind(m.resource.load_epoch.encode()).bind(m.owner_order.to_be_bytes().to_vec()).bind(m.kind.label()).bind(m.fingerprint.to_vec()).bind(m.at).execute(&mut *c).await?;
            Ok(StoredConvergence {
                metadata: m.clone(),
            })
        })
    }
}
async fn loss_targets(
    c: &mut SqliteConnection,
    loss: EntityId,
) -> Result<Vec<LossTargetMetadata>, DispatchError> {
    let rows = sqlx::query(
        "SELECT * FROM node_native_loss_targets WHERE loss_id=? ORDER BY target_id,load_epoch",
    )
    .bind(loss.encode())
    .fetch_all(c)
    .await?;
    rows.iter()
        .map(|r| {
            Ok(LossTargetMetadata {
                target: entity(r, "target_id")?,
                load_epoch: entity(r, "load_epoch")?,
                owner: entity(r, "owner_id")?,
                owner_order: u64_blob(&col::<Vec<u8>>(r, "owner_order")?)?,
                requirement: entity(r, "requirement_id")?,
                fingerprint: digest(r, "metadata_fingerprint")?
                    .ok_or(DispatchError::CorruptState)?,
            })
        })
        .collect()
}

fn source_approval_transaction<'a, T: Send + 'a>(
    store: &'a SqliteNativeDispatch,
    source: &'a PreSourceProbe,
    deadline: &'a RegistryDeadline,
    future: BoxFuture<'a, Result<T, DispatchError>>,
) -> BoxFuture<'a, Result<T, DispatchError>> {
    Box::pin(async move {
        #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
        {
            use crate::session_storage::approval_owner::test_control::{
                self, ApprovalCut, ApprovalCutState, ApprovalOperationTag, ApprovalSqlOperation,
            };
            let tag = ApprovalOperationTag {
                operation: source.metadata().instance_id,
                kind: ApprovalSqlOperation::SourceClose,
                instance: Some(source.metadata().instance_id),
                approval: None,
                registration: None,
                response: None,
            };
            let controller = store.registry.approval_test_controller();
            bounded(deadline, async {
                controller
                    .hit(
                        ApprovalCut::SourceApprovalManifestBeforeBegin,
                        tag,
                        ApprovalCutState::before_begin(),
                    )
                    .await
                    .map_err(|_| DispatchError::StorageUnavailable)?;
                Ok(())
            })
            .await?;
            return test_control::with_original_operation(controller, tag, future).await;
        }
        #[cfg(not(all(test, feature = "approval-owner-acceptance-fixtures")))]
        {
            let _ = (store, source, deadline);
            future.await
        }
    })
}
