//! Necessary native metadata on the original registry's physical connection owner.
use super::{
    registry_connection_owner::RegistryConnection, request_registry::SqliteRequestRegistry,
};
use crate::{
    application::{
        native_dispatch::{
            AttemptMetadata, AttemptProbe, BoxFuture, ControlInspection, ControlIntentKind,
            ControlKind, ControlMetadata, ControlPhase, ControlRecordProbe, ControlRecordProof,
            ConvergenceProbe, DispatchCloseRecord, DispatchInspection, DispatchLifecycle,
            DispatchProbe, FactKind, IoCloseKind, LoadTransportProof, LoadedResourceProof,
            LossTargetMetadata, MetadataBatch, NativeClosureRepository, NativeControlRepository,
            NativeDispatchRepository, NativeFactMetadata, NativeResourceRepository,
            ObservationMetadata, ObservationProbe, OwnerCloseKind, PreResourceProbe,
            PreSourceProbe, RecoverySummary, ReleaseKind, ReleaseProof, ReservedLoadProof,
            ResourceCloseBasis, ResourceCloseStage, ResourceClosureProof, ResourceLifecycle,
            ResourceRecord, ResourceStateProof, ResultKind, SelectedProof, SelectionStamp,
            SourceBoundRecordProof, SourceCloseBasis, SourceCloseStage, SourceClosureProof,
            SourceInspection, SourceLifecycle, SourceLossProbe, SourceRecord, SourceStateProof,
            SourceTerminalReason, StartupProof, StoredAttempt, StoredControl, StoredConvergence,
            StoredDispatch, StoredFacts, StoredResourceClosure, StoredSourceClosed,
            StoredSourceClosure, StoredSourceLoss, StoredTransport, TerminalTransportProof,
            TransitionKind, control_fingerprint, observation_fingerprint,
        },
        request_registry::RegistryDeadline,
    },
    domain::{
        EntityId,
        native::RuntimeEpoch,
        native_dispatch::{
            ContentOrigin, DispatchError, DispatchPlan, KnownStopReason, NativeOwnerKind,
            ProtocolVersion, ResourceMetadata, RpcScalar, SelectionKind, SourceMetadata,
            TransportFact, TransportReason,
        },
    },
};
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};
use std::{future::Future, sync::Arc};

mod source_sessions;

impl From<sqlx::Error> for DispatchError {
    fn from(_: sqlx::Error) -> Self {
        Self::StorageUnavailable
    }
}
#[derive(Clone)]
pub struct SqliteNativeDispatch {
    pub(super) registry: SqliteRequestRegistry,
}
impl SqliteNativeDispatch {
    pub(super) const fn new(registry: SqliteRequestRegistry) -> Self {
        Self { registry }
    }
    pub(super) async fn begin(
        &self,
        d: &RegistryDeadline,
    ) -> Result<RegistryConnection, DispatchError> {
        bounded(d, async { self.registry.begin().await.map_err(Into::into) }).await
    }
    pub(super) async fn finish<T>(
        &self,
        mut c: RegistryConnection,
        result: Result<T, DispatchError>,
        d: &RegistryDeadline,
    ) -> Result<T, DispatchError> {
        let result = match result {
            Ok(value) => match bounded(d, async {
                sqlx::query("COMMIT")
                    .execute(c.connection()?)
                    .await
                    .map_err(|_| DispatchError::CommitUnconfirmed)?;
                Ok(())
            })
            .await
            {
                Ok(()) => Ok(value),
                Err(_) => Err(DispatchError::CommitUnconfirmed),
            },
            Err(error) => {
                let _ = bounded(d, async {
                    sqlx::query("ROLLBACK").execute(c.connection()?).await?;
                    Ok(())
                })
                .await;
                Err(error)
            }
        };
        #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
        c.approval_commit_for_test(result.is_ok());
        let closed = bounded(d, async {
            match c.close().await {
                super::registry_connection_owner::CloseOutcome::Confirmed => Ok(()),
                super::registry_connection_owner::CloseOutcome::Unconfirmed => {
                    Err(DispatchError::CommitUnconfirmed)
                }
            }
        })
        .await;
        if closed.is_err() {
            return Err(DispatchError::SqlCloseUnconfirmed {
                commit_confirmed: result.is_ok(),
            });
        }
        result
    }
}
async fn bounded<T>(
    d: &RegistryDeadline,
    f: impl Future<Output = Result<T, DispatchError>>,
) -> Result<T, DispatchError> {
    tokio::select! {biased;()=d.cancelled()=>Err(DispatchError::StorageUnavailable),v=tokio::time::timeout_at(d.deadline(),f)=>v.map_err(|_|DispatchError::StorageUnavailable)?}
}
macro_rules! transaction {($s:ident,$d:ident,$c:ident,$body:block)=>{Box::pin(async move{let mut owned=$s.begin($d).await?;let result={let $c=owned.connection()?;bounded($d,async $body).await};$s.finish(owned,result,$d).await})};}
macro_rules! transaction_after_test_pause {($s:ident,$d:ident,$c:ident,$point:expr,$body:block)=>{Box::pin(async move{
    #[cfg(test)]
    bounded($d,$s.registry.native_metadata_hooks_for_test().hit($point)).await?;
    let mut owned=$s.begin($d).await?;let result={let $c=owned.connection()?;bounded($d,async $body).await};$s.finish(owned,result,$d).await
})};}
fn col<'a, T: sqlx::Decode<'a, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite>>(
    r: &'a SqliteRow,
    n: &str,
) -> Result<T, DispatchError> {
    r.try_get(n).map_err(|_| DispatchError::CorruptState)
}
fn id(v: &str) -> Result<EntityId, DispatchError> {
    EntityId::decode(v).map_err(|_| DispatchError::CorruptState)
}
fn entity(r: &SqliteRow, n: &str) -> Result<EntityId, DispatchError> {
    id(&col::<String>(r, n)?)
}
fn optional_id(r: &SqliteRow, n: &str) -> Result<Option<EntityId>, DispatchError> {
    col::<Option<String>>(r, n)?.as_deref().map(id).transpose()
}
fn u64_blob(v: &[u8]) -> Result<u64, DispatchError> {
    let b: [u8; 8] = v.try_into().map_err(|_| DispatchError::CorruptState)?;
    Ok(u64::from_be_bytes(b))
}
fn optional_u64(r: &SqliteRow, n: &str) -> Result<Option<u64>, DispatchError> {
    col::<Option<Vec<u8>>>(r, n)?
        .as_deref()
        .map(u64_blob)
        .transpose()
}
fn digest(r: &SqliteRow, n: &str) -> Result<Option<[u8; 32]>, DispatchError> {
    col::<Option<Vec<u8>>>(r, n)?
        .map(|v| v.try_into().map_err(|_| DispatchError::CorruptState))
        .transpose()
}
const fn wire(v: Option<i64>) -> Result<Option<ProtocolVersion>, DispatchError> {
    match v {
        None => Ok(None),
        Some(1) => Ok(Some(ProtocolVersion::V1)),
        Some(2) => Ok(Some(ProtocolVersion::V2)),
        _ => Err(DispatchError::CorruptState),
    }
}
fn rpc(row: &SqliteRow, kind: &str, bytes: &str) -> Result<Option<RpcScalar>, DispatchError> {
    match (
        col::<Option<String>>(row, kind)?,
        col::<Option<Vec<u8>>>(row, bytes)?,
    ) {
        (None, None) => Ok(None),
        (Some(kind_value), Some(encoded)) if kind_value == "integer" => {
            let integer_bytes: [u8; 8] = encoded
                .try_into()
                .map_err(|_| DispatchError::CorruptState)?;
            let integer = i64::from_be_bytes(integer_bytes);
            if !(-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&integer) {
                return Err(DispatchError::CorruptState);
            }
            Ok(Some(RpcScalar::Integer(integer)))
        }
        (Some(kind_value), Some(encoded)) if kind_value == "string" => {
            if encoded.len() > 1024 {
                return Err(DispatchError::CorruptState);
            }
            Ok(Some(RpcScalar::String(Arc::from(
                String::from_utf8(encoded).map_err(|_| DispatchError::CorruptState)?,
            ))))
        }
        _ => Err(DispatchError::CorruptState),
    }
}
fn rpc_parts(v: &RpcScalar) -> (&'static str, Vec<u8>) {
    match v {
        RpcScalar::Integer(n) => ("integer", n.to_be_bytes().to_vec()),
        RpcScalar::String(s) => ("string", s.as_bytes().to_vec()),
    }
}
fn reason(v: &str) -> Result<TransportReason, DispatchError> {
    match v {
        "cancelled" => Ok(TransportReason::Cancelled),
        "deadline" => Ok(TransportReason::Deadline),
        "fence_rejected" => Ok(TransportReason::FenceRejected),
        "connection_closed" => Ok(TransportReason::ConnectionClosed),
        "abandoned" => Ok(TransportReason::Abandoned),
        "io" => Ok(TransportReason::Io),
        "invalid_fence" => Ok(TransportReason::InvalidFence),
        "connection_lost" => Ok(TransportReason::ConnectionLost),
        "restart" => Ok(TransportReason::Restart),
        _ => Err(DispatchError::CorruptState),
    }
}
const fn transport_parts(
    f: TransportFact,
    len: u64,
) -> Result<(&'static str, u64, Option<&'static str>), DispatchError> {
    match f {
        TransportFact::Zero { reason: r }
            if matches!(
                r,
                TransportReason::Cancelled
                    | TransportReason::Deadline
                    | TransportReason::FenceRejected
                    | TransportReason::ConnectionClosed
                    | TransportReason::Abandoned
            ) =>
        {
            Ok(("zero", 0, Some(r.label())))
        }
        TransportFact::Unknown { accepted, cause }
            if accepted <= len
                && !matches!(
                    cause,
                    TransportReason::ConnectionClosed | TransportReason::Abandoned
                ) =>
        {
            Ok(("unknown", accepted, Some(cause.label())))
        }
        TransportFact::Written => Ok(("written", len, None)),
        _ => Err(DispatchError::CorruptState),
    }
}
fn source(r: &SqliteRow) -> Result<SourceRecord, DispatchError> {
    let owner_kind = match col::<String>(r, "owner_kind")?.as_str() {
        "windows_job" => NativeOwnerKind::WindowsJob,
        "test_owned_stream" => NativeOwnerKind::TestOwnedStream,
        _ => return Err(DispatchError::CorruptState),
    };
    let lifecycle = match col::<String>(r, "lifecycle")?.as_str() {
        "reserved" => SourceLifecycle::Reserved,
        "spawned" => SourceLifecycle::Spawned,
        "initialized" => SourceLifecycle::Initialized,
        "closing" => SourceLifecycle::Closing,
        "closed" => SourceLifecycle::Closed,
        "unknown" => SourceLifecycle::Unknown,
        _ => return Err(DispatchError::CorruptState),
    };
    let close = col::<Option<String>>(r, "close_basis")?
        .map(|basis| -> Result<_, DispatchError> {
            Ok(StoredSourceClosure {
                basis: Arc::from(basis),
                stage: Arc::from(col::<String>(r, "close_stage")?),
                io: Arc::from(col::<String>(r, "close_io_evidence")?),
                owner: Arc::from(col::<String>(r, "close_owner_evidence")?),
                active_processes: col::<Option<i64>>(r, "close_job_active")?
                    .map(|v| u32::try_from(v).map_err(|_| DispatchError::CorruptState))
                    .transpose()?,
                fingerprint: digest(r, "close_evidence_fingerprint")?
                    .ok_or(DispatchError::CorruptState)?,
            })
        })
        .transpose()?;
    Ok(SourceRecord {
        metadata: SourceMetadata {
            source_id: entity(r, "source_id")?,
            instance_id: entity(r, "instance_id")?,
            runtime: RuntimeEpoch(entity(r, "runtime_epoch")?),
            connection_epoch: entity(r, "connection_epoch")?,
            config_identity: entity(r, "config_identity")?,
            config_revision: u64_blob(&col::<Vec<u8>>(r, "config_revision")?)?,
            owner_kind,
            created_at_ms: col(r, "created_at_ms")?,
        },
        lifecycle,
        scope_binding: optional_id(r, "scope_binding_id")?,
        scope_bound_at: col(r, "scope_bound_at_ms")?,
        wire: wire(col(r, "protocol_version")?)?,
        initialize_rpc: rpc(r, "initialize_rpc_kind", "initialize_rpc_bytes")?,
        initialize_sequence: optional_u64(r, "initialize_sequence")?,
        initialize_receipt: optional_u64(r, "initialize_receipt")?,
        evidence: digest(r, "evidence_fingerprint")?,
        close,
        updated_at: col(r, "updated_at_ms")?,
    })
}
async fn read_source(
    c: &mut SqliteConnection,
    m: &SourceMetadata,
) -> Result<Option<SourceRecord>, DispatchError> {
    let r = sqlx::query("SELECT * FROM node_native_source_instances WHERE instance_id=?")
        .bind(m.instance_id.encode())
        .fetch_optional(c)
        .await?;
    let v = r.as_ref().map(source).transpose()?;
    if v.as_ref().is_some_and(|v| &v.metadata != m) {
        return Err(DispatchError::Conflict);
    }
    Ok(v)
}
async fn read_resource(
    connection: &mut SqliteConnection,
    metadata: &ResourceMetadata,
) -> Result<Option<ResourceRecord>, DispatchError> {
    read_source(connection, &metadata.source)
        .await?
        .ok_or(DispatchError::InvalidSource)?;
    let row = sqlx::query("SELECT * FROM node_native_resource_bindings WHERE load_epoch=?")
        .bind(metadata.load_epoch.encode())
        .fetch_optional(connection)
        .await?;
    let Some(record) = row else { return Ok(None) };
    if entity(&record, "target_id")? != metadata.target
        || entity(&record, "owner_id")? != metadata.owner
        || entity(&record, "instance_id")? != metadata.source.instance_id
        || entity(&record, "runtime_epoch")? != metadata.source.runtime.0
        || entity(&record, "connection_epoch")? != metadata.source.connection_epoch
        || entity(&record, "config_identity")? != metadata.source.config_identity
        || u64_blob(&col::<Vec<u8>>(&record, "config_revision")?)?
            != metadata.source.config_revision
        || col::<i64>(&record, "reserved_at_ms")? != metadata.reserved_at_ms
    {
        return Err(DispatchError::Conflict);
    }
    let lifecycle = match col::<String>(&record, "lifecycle")?.as_str() {
        "reserved" => ResourceLifecycle::Reserved,
        "loading" => ResourceLifecycle::Loading,
        "loaded" => ResourceLifecycle::Loaded,
        "releasing" => ResourceLifecycle::Releasing,
        "released" => ResourceLifecycle::Released,
        "unknown" => ResourceLifecycle::Unknown,
        _ => return Err(DispatchError::CorruptState),
    };
    let frame_length = optional_u64(&record, "load_frame_length")?;
    let transport = match col::<Option<String>>(&record, "load_transport_state")? {
        None => None,
        Some(state) => {
            let accepted = optional_u64(&record, "load_accepted_length")?
                .ok_or(DispatchError::CorruptState)?;
            let length = frame_length.ok_or(DispatchError::CorruptState)?;
            let why = col::<Option<String>>(&record, "load_transport_reason")?;
            let transport = match (state.as_str(), why) {
                ("armed", None) if accepted == 0 => StoredTransport::Armed,
                ("written", None) if accepted == length => {
                    StoredTransport::Terminal(TransportFact::Written)
                }
                ("zero", Some(record)) if accepted == 0 => {
                    StoredTransport::Terminal(TransportFact::Zero {
                        reason: reason(&record)?,
                    })
                }
                ("unknown", Some(record)) if accepted <= length => {
                    StoredTransport::Terminal(TransportFact::Unknown {
                        accepted,
                        cause: reason(&record)?,
                    })
                }
                _ => return Err(DispatchError::CorruptState),
            };
            Some(transport)
        }
    };
    let release = col::<Option<String>>(&record, "release_basis")?
        .map(|basis| -> Result<_, DispatchError> {
            Ok(StoredResourceClosure {
                basis: Arc::from(basis),
                stage: Arc::from(col::<String>(&record, "release_stage")?),
                fingerprint: digest(&record, "release_evidence_fingerprint")?
                    .ok_or(DispatchError::CorruptState)?,
            })
        })
        .transpose()?;
    Ok(Some(ResourceRecord {
        metadata: metadata.clone(),
        lifecycle,
        scope_binding: optional_id(&record, "scope_binding_id")?,
        wire: wire(col(&record, "protocol_version")?)?,
        load_rpc: rpc(&record, "load_rpc_kind", "load_rpc_bytes")?,
        frame_length,
        transport,
        loaded_sequence: optional_u64(&record, "loaded_sequence")?,
        loaded_receipt: optional_u64(&record, "loaded_receipt")?,
        evidence: digest(&record, "evidence_fingerprint")?,
        release,
        updated_at: col(&record, "updated_at_ms")?,
    }))
}

impl NativeResourceRepository for SqliteNativeDispatch {
    fn reserve_source<'a>(
        &'a self,
        p: &'a PreSourceProbe,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<SourceRecord, DispatchError>> {
        transaction!(self, d, c, {
            let m = p.metadata();
            if let Some(v) = read_source(c, m).await? {
                return Ok(v);
            }
            let unresolved:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_source_instances WHERE source_id=? AND config_identity=? AND config_revision=? AND lifecycle!='closed' AND (runtime_epoch!=? OR lifecycle IN('unknown','closing'))").bind(m.source_id.encode()).bind(m.config_identity.encode()).bind(m.config_revision.to_be_bytes().to_vec()).bind(m.runtime.0.encode()).fetch_one(&mut *c).await?;
            if unresolved != 0 {
                return Err(DispatchError::ResourceUnconfirmed);
            }
            sqlx::query("INSERT INTO node_native_source_instances(instance_id,source_id,runtime_epoch,connection_epoch,config_identity,config_revision,owner_kind,lifecycle,created_at_ms,updated_at_ms) VALUES(?,?,?,?,?,?,?,'reserved',?,?)").bind(m.instance_id.encode()).bind(m.source_id.encode()).bind(m.runtime.0.encode()).bind(m.connection_epoch.encode()).bind(m.config_identity.encode()).bind(m.config_revision.to_be_bytes().to_vec()).bind(m.owner_kind.label()).bind(m.created_at_ms).bind(m.created_at_ms).execute(&mut *c).await?;
            read_source(c, m).await?.ok_or(DispatchError::CorruptState)
        })
    }
    fn reserve_resource<'a>(
        &'a self,
        p: &'a PreResourceProbe,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<ResourceRecord, DispatchError>> {
        transaction!(self, d, c, {
            let m = p.metadata();
            if let Some(v) = read_resource(c, m).await? {
                return Ok(v);
            }
            let source = read_source(c, &m.source)
                .await?
                .ok_or(DispatchError::InvalidSource)?;
            if matches!(
                source.lifecycle,
                SourceLifecycle::Closed | SourceLifecycle::Closing | SourceLifecycle::Unknown
            ) {
                return Err(DispatchError::NotReady);
            }
            let target_source: String =
                sqlx::query_scalar("SELECT source_id FROM product_targets WHERE target_id=?")
                    .bind(m.target.encode())
                    .fetch_optional(&mut *c)
                    .await?
                    .ok_or(DispatchError::InvalidTarget)?;
            if id(&target_source)? != m.source.source_id {
                return Err(DispatchError::InvalidSource);
            }
            let old:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM product_resources WHERE target_id=? AND state!='released')+(SELECT count(*) FROM node_native_resource_bindings WHERE target_id=? AND lifecycle!='released')").bind(m.target.encode()).bind(m.target.encode()).fetch_one(&mut *c).await?;
            if old != 0 {
                return Err(DispatchError::ResourceUnconfirmed);
            }
            sqlx::query("INSERT INTO node_native_resource_bindings(load_epoch,target_id,owner_id,instance_id,runtime_epoch,connection_epoch,config_identity,config_revision,lifecycle,reserved_at_ms,updated_at_ms) VALUES(?,?,?,?,?,?,?,?,'reserved',?,?)").bind(m.load_epoch.encode()).bind(m.target.encode()).bind(m.owner.encode()).bind(m.source.instance_id.encode()).bind(m.source.runtime.0.encode()).bind(m.source.connection_epoch.encode()).bind(m.source.config_identity.encode()).bind(m.source.config_revision.to_be_bytes().to_vec()).bind(m.reserved_at_ms).bind(m.reserved_at_ms).execute(&mut *c).await?;
            read_resource(c, m)
                .await?
                .ok_or(DispatchError::CorruptState)
        })
    }
    fn record_source_bound<'a>(
        &'a self,
        p: &'a SourceBoundRecordProof,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<SourceRecord, DispatchError>> {
        transaction!(self, d, c, {
            let m = p.source().metadata();
            let v = read_source(c, m)
                .await?
                .ok_or(DispatchError::InvalidSource)?;
            if let Some(existing) = v.scope_binding {
                if existing == p.binding() && v.scope_bound_at == Some(p.at()) {
                    return Ok(v);
                }
                return Err(DispatchError::Conflict);
            }
            if !matches!(
                v.lifecycle,
                SourceLifecycle::Spawned | SourceLifecycle::Unknown | SourceLifecycle::Closing
            ) {
                return Err(DispatchError::Conflict);
            }
            sqlx::query("UPDATE node_native_source_instances SET scope_binding_id=?,scope_bound_at_ms=?,updated_at_ms=? WHERE instance_id=?").bind(p.binding().encode()).bind(p.at()).bind(p.at()).bind(m.instance_id.encode()).execute(&mut *c).await?;
            read_source(c, m).await?.ok_or(DispatchError::CorruptState)
        })
    }
    fn record_source_state<'a>(
        &'a self,
        proof: &'a SourceStateProof,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<SourceRecord, DispatchError>> {
        transaction_after_test_pause!(
            self,
            deadline,
            connection,
            super::native_metadata_test_hooks::NativeMetadataPoint::SourceStateBeforeBegin,
            {
                let metadata = proof.source().metadata();
                let current = read_source(connection, metadata)
                    .await?
                    .ok_or(DispatchError::InvalidSource)?;
                if current.lifecycle == SourceLifecycle::Closed {
                    return Err(DispatchError::Conflict);
                }
                if let Some(init) = proof.initialized() {
                    if proof.state() != SourceLifecycle::Initialized
                        || current.scope_binding.is_none()
                    {
                        return Err(DispatchError::Conflict);
                    }
                    if current.initialize_rpc.is_some() {
                        if current.initialize_rpc.as_ref() == Some(&init.rpc)
                            && current.wire == Some(init.wire)
                            && current.initialize_sequence == Some(init.sequence)
                            && current.initialize_receipt == Some(init.receipt)
                            && current.evidence == Some(init.fingerprint)
                        {
                            return Ok(current);
                        }
                        return Err(DispatchError::Conflict);
                    }
                    let (rpc_kind, rpc_bytes) = rpc_parts(&init.rpc);
                    let state = if matches!(
                        current.lifecycle,
                        SourceLifecycle::Closing | SourceLifecycle::Unknown
                    ) {
                        current.lifecycle
                    } else {
                        SourceLifecycle::Initialized
                    };
                    sqlx::query("UPDATE node_native_source_instances SET lifecycle=?,protocol_version=?,initialize_rpc_kind=?,initialize_rpc_bytes=?,initialize_sequence=?,initialize_receipt=?,evidence_fingerprint=?,updated_at_ms=? WHERE instance_id=?").bind(state.label()).bind(init.wire.number()).bind(rpc_kind).bind(rpc_bytes).bind(init.sequence.to_be_bytes().to_vec()).bind(init.receipt.to_be_bytes().to_vec()).bind(init.fingerprint.to_vec()).bind(proof.at()).bind(metadata.instance_id.encode()).execute(&mut *connection).await?;
                } else {
                    let valid = matches!(
                        (current.lifecycle, proof.state()),
                        (SourceLifecycle::Reserved, SourceLifecycle::Spawned)
                            | (_, SourceLifecycle::Closing | SourceLifecycle::Unknown)
                    ) || current.lifecycle == proof.state();
                    if !valid
                        || matches!(
                            proof.state(),
                            SourceLifecycle::Initialized | SourceLifecycle::Closed
                        )
                    {
                        return Err(DispatchError::Conflict);
                    }
                    sqlx::query("UPDATE node_native_source_instances SET lifecycle=?,updated_at_ms=? WHERE instance_id=?").bind(proof.state().label()).bind(proof.at()).bind(metadata.instance_id.encode()).execute(&mut *connection).await?;
                }
                read_source(connection, metadata)
                    .await?
                    .ok_or(DispatchError::CorruptState)
            }
        )
    }
    fn begin_load<'a>(
        &'a self,
        proof: &'a ReservedLoadProof,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<ResourceRecord, DispatchError>> {
        transaction!(self, deadline, connection, {
            let metadata = proof.resource().metadata();
            let current = read_resource(connection, metadata)
                .await?
                .ok_or(DispatchError::InvalidResource)?;
            if current.load_rpc.is_some() {
                if current.load_rpc.as_ref() == Some(proof.rpc())
                    && current.frame_length == Some(proof.frame_length())
                    && current.scope_binding == Some(proof.binding())
                    && current.wire == Some(proof.wire())
                {
                    return Ok(current);
                }
                return Err(DispatchError::Conflict);
            }
            let source = read_source(connection, &metadata.source)
                .await?
                .ok_or(DispatchError::InvalidSource)?;
            if current.lifecycle != ResourceLifecycle::Reserved
                || source.lifecycle != SourceLifecycle::Initialized
                || source.scope_binding != Some(proof.binding())
                || source.wire != Some(proof.wire())
                || proof.frame_length() == 0
            {
                return Err(DispatchError::NotReady);
            }
            let (rpc_kind, rpc_bytes) = rpc_parts(proof.rpc());
            sqlx::query("UPDATE node_native_resource_bindings SET lifecycle='loading',scope_binding_id=?,protocol_version=?,load_rpc_kind=?,load_rpc_bytes=?,load_frame_length=?,load_transport_state='armed',load_accepted_length=?,updated_at_ms=? WHERE load_epoch=?").bind(proof.binding().encode()).bind(proof.wire().number()).bind(rpc_kind).bind(rpc_bytes).bind(proof.frame_length().to_be_bytes().to_vec()).bind(0_u64.to_be_bytes().to_vec()).bind(proof.at()).bind(metadata.load_epoch.encode()).execute(&mut *connection).await?;
            read_resource(connection, metadata)
                .await?
                .ok_or(DispatchError::CorruptState)
        })
    }
    fn record_load_transport<'a>(
        &'a self,
        p: &'a LoadTransportProof,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<ResourceRecord, DispatchError>> {
        transaction!(self, d, c, {
            let m = p.resource().metadata();
            let v = read_resource(c, m)
                .await?
                .ok_or(DispatchError::InvalidResource)?;
            if v.load_rpc.as_ref() != Some(p.rpc()) || v.frame_length != Some(p.frame_length()) {
                return Err(DispatchError::Conflict);
            }
            if let Some(StoredTransport::Terminal(f)) = v.transport {
                if f == p.fact() {
                    return Ok(v);
                }
                return Err(DispatchError::Conflict);
            }
            if v.transport != Some(StoredTransport::Armed) {
                return Err(DispatchError::Conflict);
            }
            let (state, accepted, why) = transport_parts(p.fact(), p.frame_length())?;
            sqlx::query("UPDATE node_native_resource_bindings SET load_transport_state=?,load_accepted_length=?,load_transport_reason=?,updated_at_ms=? WHERE load_epoch=?").bind(state).bind(accepted.to_be_bytes().to_vec()).bind(why).bind(p.at()).bind(m.load_epoch.encode()).execute(&mut *c).await?;
            read_resource(c, m)
                .await?
                .ok_or(DispatchError::CorruptState)
        })
    }
    fn record_loaded<'a>(
        &'a self,
        p: &'a LoadedResourceProof,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<ResourceRecord, DispatchError>> {
        transaction!(self, d, c, {
            let m = p.resource().metadata();
            let v = read_resource(c, m)
                .await?
                .ok_or(DispatchError::InvalidResource)?;
            if v.load_rpc.as_ref() != Some(p.rpc()) {
                return Err(DispatchError::Conflict);
            }
            if v.loaded_sequence.is_some() {
                if v.loaded_sequence == Some(p.sequence())
                    && v.loaded_receipt == Some(p.receipt())
                    && v.evidence == Some(*p.fingerprint())
                {
                    return Ok(v);
                }
                return Err(DispatchError::Conflict);
            }
            if matches!(
                v.lifecycle,
                ResourceLifecycle::Reserved | ResourceLifecycle::Released
            ) || v.transport != Some(StoredTransport::Terminal(TransportFact::Written))
            {
                return Err(DispatchError::Conflict);
            }
            let state = if matches!(
                v.lifecycle,
                ResourceLifecycle::Releasing | ResourceLifecycle::Unknown
            ) {
                v.lifecycle
            } else {
                ResourceLifecycle::Loaded
            };
            sqlx::query("UPDATE node_native_resource_bindings SET lifecycle=?,loaded_sequence=?,loaded_receipt=?,evidence_fingerprint=?,updated_at_ms=? WHERE load_epoch=?").bind(state.label()).bind(p.sequence().to_be_bytes().to_vec()).bind(p.receipt().to_be_bytes().to_vec()).bind(p.fingerprint().to_vec()).bind(p.at()).bind(m.load_epoch.encode()).execute(&mut *c).await?;
            read_resource(c, m)
                .await?
                .ok_or(DispatchError::CorruptState)
        })
    }
    fn inspect_source<'a>(
        &'a self,
        p: &'a PreSourceProbe,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<Option<SourceRecord>, DispatchError>> {
        transaction!(self, d, c, { read_source(c, p.metadata()).await })
    }
    fn record_resource_state<'a>(
        &'a self,
        p: &'a ResourceStateProof,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<ResourceRecord, DispatchError>> {
        transaction!(self, d, c, {
            if !matches!(
                p.state(),
                ResourceLifecycle::Releasing | ResourceLifecycle::Unknown
            ) {
                return Err(DispatchError::Conflict);
            }
            let metadata = p.resource().metadata();
            let current = read_resource(c, metadata)
                .await?
                .ok_or(DispatchError::InvalidResource)?;
            if current.lifecycle == ResourceLifecycle::Released {
                return Err(DispatchError::Conflict);
            }
            if current.lifecycle == p.state() {
                return Ok(current);
            }
            sqlx::query("UPDATE node_native_resource_bindings SET lifecycle=?,updated_at_ms=? WHERE load_epoch=? AND instance_id=? AND owner_id=?").bind(p.state().label()).bind(p.at()).bind(metadata.load_epoch.encode()).bind(metadata.source.instance_id.encode()).bind(metadata.owner.encode()).execute(&mut *c).await?;
            read_resource(c, metadata)
                .await?
                .ok_or(DispatchError::CorruptState)
        })
    }
    fn inspect_resource<'a>(
        &'a self,
        p: &'a PreResourceProbe,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<Option<ResourceRecord>, DispatchError>> {
        transaction!(self, d, c, { read_resource(c, p.metadata()).await })
    }
}
#[cfg(test)]
mod close_outcome_tests;
mod closure;
mod control;
mod dispatch;
