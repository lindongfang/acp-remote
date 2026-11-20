//! The original registry connection owns every necessary capacity transaction and close.
use super::native_dispatch::SqliteNativeDispatch;
use crate::{
    application::{
        native_dispatch::{
            AutomaticReleaseOutcome, AutomaticReleaseReason, AutomaticReleaseRecord, BoxFuture,
            CapacityRecord, CapacityReleaseBasis, CapacityRepository, CapacityReservation,
            CapacitySnapshot, PreSourceProbe,
        },
        request_registry::RegistryDeadline,
    },
    domain::{
        EntityId,
        native::{NativeSessionKey, RuntimeEpoch},
        native_dispatch::DispatchError,
        request_registry::RegistryError,
    },
};
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

const DEFAULT_LIMIT: u32 = crate::domain::session_capacity::DEFAULT_SESSION_LIMIT;

macro_rules! transaction {
    ($self:ident, $deadline:ident, $c:ident, $work:block) => {
        Box::pin(async move {
            let mut owned = $self.begin($deadline).await?;
            let result = {
                let $c = owned.connection()?;
                tokio::select! {
                    biased;
                    () = $deadline.cancelled() => Err(DispatchError::StorageUnavailable),
                    result = tokio::time::timeout_at($deadline.deadline(), async $work) =>
                        result.unwrap_or(Err(DispatchError::StorageUnavailable)),
                }
            };
            $self.finish(owned, result, $deadline).await
        })
    };
}

impl CapacityRepository for SqliteNativeDispatch {
    fn begin_automatic_release<'a>(
        &'a self,
        record: &'a CapacityRecord,
        operation: EntityId,
        reason: AutomaticReleaseReason,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<AutomaticReleaseRecord, DispatchError>> {
        let transaction = transaction!(self, d, c, {
            super::automatic_capacity_release::begin(c, record, operation, reason).await
        });
        #[cfg(not(test))]
        {
            transaction
        }
        #[cfg(test)]
        Box::pin(async move {
            {
                let hooks = self.registry.native_metadata_hooks_for_test();
                let paused = hooks.hit(
                    super::native_metadata_test_hooks::NativeMetadataPoint::AutomaticReleaseBeforeBegin,
                );
                tokio::select! {
                    biased;
                    () = d.cancelled() => return Err(DispatchError::StorageUnavailable),
                    result = tokio::time::timeout_at(d.deadline(), paused) =>
                        result.map_err(|_| DispatchError::StorageUnavailable)??,
                }
            }
            transaction.await
        })
    }

    fn record_automatic_release_outcome<'a>(
        &'a self,
        record: &'a CapacityRecord,
        operation: EntityId,
        outcome: AutomaticReleaseOutcome,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<AutomaticReleaseRecord, DispatchError>> {
        transaction!(self, d, c, {
            super::automatic_capacity_release::record_outcome(c, record, operation, outcome).await
        })
    }

    fn finish_automatic_release<'a>(
        &'a self,
        record: &'a CapacityRecord,
        operation: EntityId,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<AutomaticReleaseRecord, DispatchError>> {
        let transaction = transaction!(self, d, c, {
            super::automatic_capacity_release::finish(c, record, operation).await
        });
        Box::pin(async move {
            let stored = transaction.await?;
            #[cfg(test)]
            {
                let hooks = self.registry.native_metadata_hooks_for_test();
                let report = hooks.hit(
                    super::native_metadata_test_hooks::NativeMetadataPoint::AutomaticReleaseFinishedAfterCommit,
                );
                tokio::select! {
                    biased;
                    () = d.cancelled() => return Err(DispatchError::CommitUnconfirmed),
                    result = tokio::time::timeout_at(d.deadline(), report) =>
                        result.map_err(|_| DispatchError::CommitUnconfirmed)??,
                }
            }
            Ok(stored)
        })
    }

    fn reserve_capacity<'a>(
        &'a self,
        p: &'a CapacityReservation,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<CapacityRecord, DispatchError>> {
        let transaction = transaction!(self, d, c, {
            let source = p.source.metadata();
            verify_source(c, &p.source).await?;
            if let Some(existing) = read(c, p.id).await? {
                if existing.source != source.source_id
                    || existing.instance != source.instance_id
                    || existing.runtime != source.runtime
                    || (p.native.is_some() && existing.native != p.native)
                    || create_operation(c, p.id).await? != p.create_operation
                {
                    return Err(DispatchError::Conflict);
                }
                if let Some(native) = &p.native {
                    require_root(c, native).await?;
                    if existing.physically_closed {
                        return Err(DispatchError::Conflict);
                    }
                }
                return Ok(existing);
            }
            match (&p.native, p.create_operation) {
                (Some(native), None) if native.source().0 == source.source_id => {
                    if let Some(owner) = owner(c, native).await? {
                        let same_instance = owner.instance == source.instance_id;
                        if !same_instance
                            || owner.runtime != source.runtime
                            || owner.physically_closed
                        {
                            return Err(DispatchError::Conflict);
                        }
                        require_root(c, native).await?;
                        return Ok(owner);
                    }
                    require_root(c, native).await?;
                }
                (None, Some(operation)) if operation == p.id => {
                    let valid: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_source_session_operations o JOIN main.node_source_session_registrations r ON r.operation_id=o.operation_id WHERE o.operation_id=? AND o.action='create' AND o.runtime_epoch=? AND r.source_id=? AND (o.source_instance_id IS NULL OR o.source_instance_id=?) AND r.profile_id=? AND r.profile_revision=?)")
                        .bind(operation.encode()).bind(source.runtime.0.encode())
                        .bind(source.source_id.encode()).bind(source.instance_id.encode())
                        .bind(source.config_identity.encode()).bind(source.config_revision.to_be_bytes().to_vec())
                        .fetch_one(&mut *c).await?;
                    if valid != 1 {
                        return Err(DispatchError::Conflict);
                    }
                }
                _ => return Err(DispatchError::Conflict),
            }
            if snapshot(c).await?.occupied >= DEFAULT_LIMIT {
                return Err(DispatchError::SessionCapacityFull);
            }
            sqlx::query("INSERT INTO main.node_session_capacity(reservation_id,source_id,native_id,instance_id,runtime_epoch,create_operation,root_verified,physically_closed,released) VALUES(?,?,?,?,?,?,0,0,0)")
                .bind(p.id.encode()).bind(source.source_id.encode())
                .bind(p.native.as_ref().map(NativeSessionKey::native_id))
                .bind(source.instance_id.encode()).bind(source.runtime.0.encode())
                .bind(p.create_operation.map(EntityId::encode))
                .execute(&mut *c).await?;
            read(c, p.id).await?.ok_or(DispatchError::CorruptState)
        });
        Box::pin(async move {
            let stored = transaction.await?;
            #[cfg(test)]
            if p.native.is_none() {
                // The real unbound reservation has committed; the registry
                // connection is released before the test holds its return.
                let hooks = self.registry.native_metadata_hooks_for_test();
                let report = hooks.hit(
                    super::native_metadata_test_hooks::NativeMetadataPoint::CapacityReservedAfterCommit,
                );
                tokio::select! {
                    biased;
                    () = d.cancelled() => return Err(DispatchError::CommitUnconfirmed),
                    result = tokio::time::timeout_at(d.deadline(), report) =>
                        result.map_err(|_| DispatchError::CommitUnconfirmed)??,
                }
            }
            Ok(stored)
        })
    }

    fn capacity_owner<'a>(
        &'a self,
        native: &'a NativeSessionKey,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<Option<CapacityRecord>, DispatchError>> {
        transaction!(self, d, c, { owner(c, native).await })
    }

    fn capacity_release_basis<'a>(
        &'a self,
        operation: EntityId,
        native: &'a NativeSessionKey,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<CapacityReleaseBasis, DispatchError>> {
        let query = transaction!(self, d, c, { release_basis(c, operation, native).await });
        Box::pin(async move {
            let basis = query.await?;
            #[cfg(test)]
            if matches!(&basis, CapacityReleaseBasis::Owned(_)) {
                let hooks = self.registry.native_metadata_hooks_for_test();
                let paused = hooks.hit(
                    super::native_metadata_test_hooks::NativeMetadataPoint::CapacityReleaseOwnedAfterCommit,
                );
                tokio::select! {
                    biased;
                    () = d.cancelled() => return Err(DispatchError::StorageUnavailable),
                    result = tokio::time::timeout_at(d.deadline(), paused) =>
                        result.map_err(|_| DispatchError::StorageUnavailable)??,
                }
            }
            Ok(basis)
        })
    }

    fn inspect_capacity<'a>(
        &'a self,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<CapacitySnapshot, DispatchError>> {
        transaction!(self, d, c, { snapshot(c).await })
    }

    fn bind_created_capacity<'a>(
        &'a self,
        reservation: EntityId,
        native: &'a NativeSessionKey,
        source: &'a PreSourceProbe,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<CapacityRecord, DispatchError>> {
        transaction!(self, d, c, {
            verify_source(c, source).await?;
            let metadata = source.metadata();
            let current = read(c, reservation).await?.ok_or(DispatchError::Conflict)?;
            if current.source != metadata.source_id
                || current.instance != metadata.instance_id
                || current.runtime != metadata.runtime
                || native.source().0 != current.source
                || current.physically_closed
                || current.native.as_ref().is_some_and(|value| value != native)
            {
                return Err(DispatchError::Conflict);
            }
            let child: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_session_parents WHERE source_id=? AND child_native_id=?)")
                .bind(native.source().0.encode()).bind(native.native_id())
                .fetch_one(&mut *c).await?;
            if child != 0 {
                return Err(DispatchError::Conflict);
            }
            // A raw Created DTO is insufficient: this exact original operation, response,
            // acknowledgement, source and launch selection must already be durable.
            let valid: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_session_capacity c JOIN main.node_source_session_operations o ON o.operation_id=c.create_operation JOIN main.node_source_session_registrations r ON r.operation_id=o.operation_id WHERE c.reservation_id=? AND o.action='create' AND o.response_kind='created' AND o.ack_state='confirmed' AND o.native_session_id=? AND o.source_instance_id=? AND o.connection_epoch=? AND o.runtime_epoch=? AND r.source_id=? AND r.profile_id=? AND r.profile_revision=?)")
                .bind(reservation.encode()).bind(native.native_id()).bind(metadata.instance_id.encode())
                .bind(metadata.connection_epoch.encode()).bind(metadata.runtime.0.encode())
                .bind(metadata.source_id.encode()).bind(metadata.config_identity.encode())
                .bind(metadata.config_revision.to_be_bytes().to_vec())
                .fetch_one(&mut *c).await?;
            if valid != 1 {
                return Err(DispatchError::RequiredStateUnavailable);
            }
            // Even a same-instance duplicate Created cannot silently discard the second
            // possible resource responsibility.
            let conflict: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_session_capacity WHERE source_id=? AND native_id=? AND released=0 AND reservation_id!=?)")
                .bind(native.source().0.encode()).bind(native.native_id()).bind(reservation.encode())
                .fetch_one(&mut *c).await?;
            if conflict != 0 {
                return Err(DispatchError::Conflict);
            }
            sqlx::query("UPDATE main.node_session_capacity SET native_id=?,root_verified=1 WHERE reservation_id=? AND released=0")
                .bind(native.native_id()).bind(reservation.encode()).execute(&mut *c).await?;
            read(c, reservation)
                .await?
                .ok_or(DispatchError::CorruptState)
        })
    }

    fn confirm_capacity_closed<'a>(
        &'a self,
        record: &'a CapacityRecord,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<(), DispatchError>> {
        transaction!(self, d, c, {
            let current = read(c, record.id).await?.ok_or(DispatchError::Conflict)?;
            if current.id != record.id
                || current.native != record.native
                || current.source != record.source
                || current.instance != record.instance
                || current.runtime != record.runtime
            {
                return Err(DispatchError::Conflict);
            }
            sqlx::query("UPDATE main.node_session_capacity SET physically_closed=1 WHERE reservation_id=? AND released=0")
                .bind(record.id.encode()).execute(c).await?;
            Ok(())
        })
    }

    fn abandon_capacity<'a>(
        &'a self,
        reservation: EntityId,
        source: &'a PreSourceProbe,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<(), DispatchError>> {
        transaction!(self, d, c, {
            let Some(current) = read(c, reservation).await? else {
                // A released row can only be repeated by its original owner.
                let valid: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_session_capacity WHERE reservation_id=? AND instance_id=? AND runtime_epoch=? AND released=1)")
                    .bind(reservation.encode()).bind(source.metadata().instance_id.encode())
                    .bind(source.metadata().runtime.0.encode()).fetch_one(&mut *c).await?;
                let present: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_session_capacity WHERE reservation_id=?)")
                    .bind(reservation.encode()).fetch_one(&mut *c).await?;
                return if valid == 1
                    || present == 0 && source_converged(c, source, reservation).await?
                {
                    Ok(())
                } else {
                    Err(DispatchError::Conflict)
                };
            };
            if current.source != source.metadata().source_id
                || current.instance != source.metadata().instance_id
                || current.runtime != source.metadata().runtime
            {
                return Err(DispatchError::Conflict);
            }
            let zero: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_session_capacity c JOIN main.node_source_session_operations o ON o.operation_id=c.create_operation WHERE c.reservation_id=? AND c.native_id IS NULL AND o.operation_state='terminal' AND o.outcome='refused' AND o.response_kind IS NULL AND (o.transport_state IS NULL OR o.transport_state='reserved' OR o.transport_state='zero'))")
                .bind(reservation.encode()).fetch_one(&mut *c).await?;
            if zero != 1 && !source_converged(c, source, reservation).await? {
                return Err(DispatchError::ResourceUnconfirmed);
            }
            sqlx::query("UPDATE main.node_session_capacity SET physically_closed=1,released=1 WHERE reservation_id=?")
                .bind(reservation.encode()).execute(c).await?;
            Ok(())
        })
    }

    fn trusted_client_display_name<'a>(
        &'a self,
        client: EntityId,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<Option<String>, DispatchError>> {
        Box::pin(async move {
            deadline
                .run(self.registry.trusted_client_display_name(client))
                .await
                .map_err(DispatchError::from)
        })
    }

    fn record_control_security_audit<'a>(
        &'a self,
        actor: Option<EntityId>,
        action: &'static str,
        root: &'a NativeSessionKey,
        outcome_json: String,
        occurred_at: i64,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<(), DispatchError>> {
        Box::pin(async move {
            let actor_encoded = actor.map(EntityId::encode);
            let target_id = format!("{}:{}", root.source().0.encode(), root.native_id());
            deadline
                .run(self.registry.append_control_security_audit(
                    actor_encoded.as_deref(),
                    action,
                    &target_id,
                    &outcome_json,
                    occurred_at,
                ))
                .await
                .map_err(DispatchError::from)
        })
    }
}

async fn release_basis(
    c: &mut SqliteConnection,
    operation: EntityId,
    native: &NativeSessionKey,
) -> Result<CapacityReleaseBasis, DispatchError> {
    let valid: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_native_release_operations o JOIN main.product_targets t ON t.target_id=o.target_id WHERE o.operation_id=? AND o.phase='releasing' AND t.source_id=? AND t.native_id=?)")
        .bind(operation.encode()).bind(native.source().0.encode()).bind(native.native_id())
        .fetch_one(&mut *c).await?;
    if valid != 1 {
        return Err(DispatchError::ResourceUnconfirmed);
    }
    let rows = sqlx::query("SELECT c.*,CASE WHEN s.source_id=c.source_id AND s.runtime_epoch=c.runtime_epoch AND s.lifecycle='closed' AND s.close_evidence_fingerprint IS NOT NULL THEN 1 ELSE 0 END AS source_closed FROM main.node_session_capacity_releases r JOIN main.node_session_capacity c ON c.reservation_id=r.reservation_id LEFT JOIN main.node_native_source_instances s ON s.instance_id=c.instance_id WHERE r.operation_id=? ORDER BY c.reservation_id")
        .bind(operation.encode()).fetch_all(&mut *c).await?;
    if rows.is_empty() {
        return Err(DispatchError::ResourceUnconfirmed);
    }
    let mut original = Vec::with_capacity(rows.len());
    let mut source_closed = true;
    for row in &rows {
        let record = decode(row)?;
        if record.native.as_ref() != Some(native) || record.source != native.source().0 {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        let released = row.try_get::<i64, _>("released")? == 1;
        source_closed &=
            released && record.physically_closed && row.try_get::<i64, _>("source_closed")? == 1;
        if !released {
            original.push(record);
        }
    }
    if source_closed {
        return Ok(CapacityReleaseBasis::SourceClosed);
    }
    let current = owner(c, native)
        .await?
        .ok_or(DispatchError::ResourceUnconfirmed)?;
    if !original
        .iter()
        .any(|record| record.same_reservation(&current))
    {
        return Err(DispatchError::ResourceUnconfirmed);
    }
    Ok(CapacityReleaseBasis::Owned(current))
}

async fn source_converged(
    c: &mut SqliteConnection,
    source: &PreSourceProbe,
    reservation: EntityId,
) -> Result<bool, DispatchError> {
    let source = source.metadata();
    let closed: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_native_source_instances s WHERE s.instance_id=? AND s.runtime_epoch=? AND s.source_id=? AND s.connection_epoch=? AND s.lifecycle='closed' AND s.close_evidence_fingerprint IS NOT NULL) AND NOT EXISTS(SELECT 1 FROM main.node_native_resource_bindings b WHERE b.instance_id=? AND b.runtime_epoch=? AND b.lifecycle!='released') AND NOT EXISTS(SELECT 1 FROM main.node_source_session_operations o WHERE o.action='create' AND (o.source_instance_id=? OR o.operation_id=?) AND o.runtime_epoch=? AND o.operation_state='owned')")
        .bind(source.instance_id.encode()).bind(source.runtime.0.encode())
        .bind(source.source_id.encode()).bind(source.connection_epoch.encode())
        .bind(source.instance_id.encode()).bind(source.runtime.0.encode())
        .bind(source.instance_id.encode()).bind(reservation.encode()).bind(source.runtime.0.encode())
        .fetch_one(c).await?;
    Ok(closed == 1)
}

async fn verify_source(
    c: &mut SqliteConnection,
    source: &PreSourceProbe,
) -> Result<(), DispatchError> {
    let s = source.metadata();
    let valid: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_native_source_instances WHERE instance_id=? AND source_id=? AND runtime_epoch=? AND connection_epoch=? AND config_identity=? AND config_revision=? AND lifecycle!='closed')")
        .bind(s.instance_id.encode()).bind(s.source_id.encode()).bind(s.runtime.0.encode())
        .bind(s.connection_epoch.encode()).bind(s.config_identity.encode())
        .bind(s.config_revision.to_be_bytes().to_vec()).fetch_one(c).await?;
    if valid != 1 {
        return Err(DispatchError::Conflict);
    }
    Ok(())
}
pub(super) async fn require_root(
    c: &mut SqliteConnection,
    native: &NativeSessionKey,
) -> Result<(), DispatchError> {
    let root: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_session_capacity WHERE source_id=? AND native_id=? AND root_verified=1)")
        .bind(native.source().0.encode()).bind(native.native_id()).fetch_one(c).await?;
    if root != 1 {
        return Err(DispatchError::SessionTreeUnverified);
    }
    Ok(())
}
async fn create_operation(
    c: &mut SqliteConnection,
    reservation: EntityId,
) -> Result<Option<EntityId>, DispatchError> {
    let value: Option<String> = sqlx::query_scalar(
        "SELECT create_operation FROM main.node_session_capacity WHERE reservation_id=?",
    )
    .bind(reservation.encode())
    .fetch_one(c)
    .await?;
    value.as_deref().map(entity).transpose()
}
fn entity(value: &str) -> Result<EntityId, DispatchError> {
    EntityId::decode(value).map_err(|_| DispatchError::CorruptState)
}
pub(super) fn decode(row: &SqliteRow) -> Result<CapacityRecord, DispatchError> {
    let source = entity(&row.try_get::<String, _>("source_id")?)?;
    let native: Option<String> = row.try_get("native_id")?;
    Ok(CapacityRecord {
        id: entity(&row.try_get::<String, _>("reservation_id")?)?,
        native: native
            .as_deref()
            .map(|value| {
                NativeSessionKey::from_config(Some(source), value)
                    .map_err(|_| DispatchError::CorruptState)
            })
            .transpose()?,
        source,
        instance: entity(&row.try_get::<String, _>("instance_id")?)?,
        runtime: RuntimeEpoch(entity(&row.try_get::<String, _>("runtime_epoch")?)?),
        physically_closed: row.try_get::<i64, _>("physically_closed")? == 1,
    })
}
async fn read(
    c: &mut SqliteConnection,
    reservation: EntityId,
) -> Result<Option<CapacityRecord>, DispatchError> {
    sqlx::query("SELECT * FROM main.node_session_capacity WHERE reservation_id=? AND released=0")
        .bind(reservation.encode())
        .fetch_optional(c)
        .await?
        .as_ref()
        .map(decode)
        .transpose()
}
async fn owner(
    c: &mut SqliteConnection,
    native: &NativeSessionKey,
) -> Result<Option<CapacityRecord>, DispatchError> {
    let rows = sqlx::query("SELECT * FROM main.node_session_capacity WHERE source_id=? AND native_id=? AND released=0 ORDER BY reservation_id")
        .bind(native.source().0.encode()).bind(native.native_id()).fetch_all(c).await?;
    let records = rows.iter().map(decode).collect::<Result<Vec<_>, _>>()?;
    let Some(first) = records.first() else {
        return Ok(None);
    };
    if records
        .iter()
        .any(|record| record.instance != first.instance || record.runtime != first.runtime)
    {
        return Err(DispatchError::Conflict);
    }
    let mut result = first.clone();
    result.physically_closed = records.iter().all(|record| record.physically_closed);
    Ok(Some(result))
}
async fn snapshot(c: &mut SqliteConnection) -> Result<CapacitySnapshot, DispatchError> {
    let row = sqlx::query("SELECT COUNT(*) AS occupied,COALESCE(SUM(unbound),0) AS reserved,COALESCE(SUM(closed),0) AS closed_pending FROM (SELECT native_id IS NULL AS unbound,MIN(physically_closed) AS closed FROM main.node_session_capacity WHERE released=0 GROUP BY source_id,native_id,CASE WHEN native_id IS NULL THEN reservation_id ELSE '' END)")
        .fetch_one(c).await?;
    let count =
        |name| u32::try_from(row.try_get::<i64, _>(name)?).map_err(|_| DispatchError::CorruptState);
    Ok(CapacitySnapshot {
        limit: DEFAULT_LIMIT,
        occupied: count("occupied")?,
        reserved: count("reserved")?,
        closed_pending: count("closed_pending")?,
    })
}

/// Captures original responsibilities once, in Registered -> Releasing's transaction.
pub(super) async fn bind_capacity_release(
    c: &mut SqliteConnection,
    operation: EntityId,
    target: EntityId,
) -> Result<(), RegistryError> {
    sqlx::query("INSERT INTO main.node_session_capacity_releases(operation_id,reservation_id) SELECT ?,c.reservation_id FROM main.node_session_capacity c JOIN main.product_targets t ON t.source_id=c.source_id AND t.native_id=c.native_id WHERE t.target_id=? AND c.released=0")
        .bind(operation.encode()).bind(target.encode()).execute(&mut *c).await
        .map_err(|_| RegistryError::StorageUnavailable)?;
    let wrong_owner: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_native_resource_bindings b WHERE b.target_id=? AND b.lifecycle!='released' AND EXISTS(SELECT 1 FROM main.node_session_capacity_releases r WHERE r.operation_id=?) AND NOT EXISTS(SELECT 1 FROM main.node_session_capacity_releases r JOIN main.node_session_capacity c ON c.reservation_id=r.reservation_id WHERE r.operation_id=? AND c.instance_id=b.instance_id AND c.runtime_epoch=b.runtime_epoch))")
        .bind(target.encode()).bind(operation.encode()).bind(operation.encode())
        .fetch_one(c).await.map_err(|_| RegistryError::StorageUnavailable)?;
    if wrong_owner != 0 {
        return Err(RegistryError::Conflict);
    }
    Ok(())
}

/// Released retries can only reconcile the immutable set captured by their original begin.
pub(super) async fn release_capacity_for_operation(
    c: &mut SqliteConnection,
    operation: EntityId,
    target: EntityId,
) -> Result<(), RegistryError> {
    let live: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM main.node_session_capacity_releases r JOIN main.node_session_capacity c ON c.reservation_id=r.reservation_id WHERE r.operation_id=? AND c.released=0")
        .bind(operation.encode()).fetch_one(&mut *c).await
        .map_err(|_| RegistryError::StorageUnavailable)?;
    // Legacy operations have no generation proof. Their retry keeps the original
    // result but cannot adopt a responsibility created before or after migration.
    if live == 0 {
        return Ok(());
    }
    // Original live resource ownership was checked by begin. Historical released
    // resource rows from older generations must not become current owners.
    let pending: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_session_capacity_releases r JOIN main.node_session_capacity c ON c.reservation_id=r.reservation_id WHERE r.operation_id=? AND c.released=0 AND c.physically_closed=0) OR EXISTS(SELECT 1 FROM main.node_native_resource_bindings b WHERE b.target_id=? AND b.lifecycle!='released')")
        .bind(operation.encode()).bind(target.encode())
        .fetch_one(&mut *c).await.map_err(|_| RegistryError::StorageUnavailable)?;
    if pending != 0 {
        return Err(RegistryError::Conflict);
    }
    let unfinished_create: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_session_capacity_releases r JOIN main.node_session_capacity c ON c.reservation_id=r.reservation_id LEFT JOIN main.node_source_session_operations o ON o.operation_id=c.create_operation WHERE r.operation_id=? AND c.released=0 AND c.create_operation IS NOT NULL AND (o.operation_state IS NULL OR o.operation_state!='terminal' OR o.outcome IS NOT 'succeeded' OR o.response_kind IS NOT 'created' OR o.ack_state IS NOT 'confirmed' OR o.native_session_id IS NOT c.native_id OR o.source_instance_id IS NOT c.instance_id OR o.runtime_epoch IS NOT c.runtime_epoch))")
        .bind(operation.encode()).fetch_one(&mut *c).await.map_err(|_| RegistryError::StorageUnavailable)?;
    if unfinished_create != 0 {
        return Err(RegistryError::Conflict);
    }
    sqlx::query("UPDATE main.node_session_capacity SET released=1 WHERE released=0 AND physically_closed=1 AND EXISTS(SELECT 1 FROM main.node_session_capacity_releases r WHERE r.operation_id=? AND r.reservation_id=node_session_capacity.reservation_id)")
        .bind(operation.encode()).execute(c).await.map_err(|_| RegistryError::StorageUnavailable)?;
    Ok(())
}

#[cfg(test)]
mod development_tests;
