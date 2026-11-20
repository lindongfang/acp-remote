//! Necessary, body-free request facts. Every detached connection has a tracked close owner.
#[cfg(all(test, windows))]
pub(crate) use super::registry_connection_owner::authority_slot_observation;
#[cfg(test)]
pub(crate) use super::registry_connection_owner::test_hooks as connection_close_test_hooks;
use super::registry_connection_owner::{
    CloseOutcome, ConnectionCloseStatus, ConnectionOwner, RegistryConnection,
};
use crate::{
    application::request_registry::{
        BeginRecord, ConsumeLease, CurrentRequestActor, OperationContext, OperationProbe,
        RegistryDeadline, RegistryOwnerPort, RegistryOwnerScope, RequestRegistryRepository,
    },
    domain::{
        EntityId,
        native::{NativeSessionKey, RuntimeEpoch},
        request_registry::{
            QueueAction, QueueOutcome, QueueRejection, RegisteredIntent, RegistrationFact,
            RegistrationId, RegistryError, RequestFact, StoredApplying,
        },
    },
};
use sqlx::{Decode, Row, Sqlite, SqliteConnection, SqlitePool, Type, sqlite::SqliteRow};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedControlRoot {
    pub(crate) root: NativeSessionKey,
    pub(crate) root_target_id: Option<EntityId>,
}

impl From<sqlx::Error> for RegistryError {
    fn from(_: sqlx::Error) -> Self {
        Self::StorageUnavailable
    }
}
#[derive(Clone)]
pub struct SqliteRequestRegistry {
    pool: SqlitePool,
    connections: std::sync::Arc<ConnectionOwner>,
    catalog_fence: std::sync::Arc<super::catalog_sql::CatalogSqlFence>,
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    approval_hooks: std::sync::Arc<super::approval_owner::test_control::ApprovalTestController>,
    #[cfg(test)]
    hooks: std::sync::Arc<test_hooks::FaultController>,
    #[cfg(test)]
    native_hooks: std::sync::Arc<super::native_metadata_test_hooks::NativeMetadataHooks>,
}
impl SqliteRequestRegistry {
    pub(super) fn catalog_fence(&self) -> std::sync::Arc<super::catalog_sql::CatalogSqlFence> {
        self.catalog_fence.clone()
    }
    pub(super) fn catalog_operation_lease(
        &self,
    ) -> Result<super::registry_connection_owner::ConnectionLease, RegistryError> {
        self.connections.lease()
    }
    pub(super) async fn acquire_catalog_connection(
        &self,
        operation: std::sync::Arc<super::catalog_sql::CatalogOperation>,
        role: crate::domain::native_catalog::CatalogSqlRole,
    ) -> Result<super::catalog_sql::CatalogConnection, crate::domain::native_catalog::CatalogError>
    {
        operation.bind_scope(self.owner_scope())?;
        let acquired = self
            .connections
            .acquire(self.pool.clone())
            .await
            .map_err(|_| crate::domain::native_catalog::CatalogError::StorageUnavailable)?;
        let index = operation.attach(role)?;
        // No await between detaching the pool connection and installing the catalog Drop owner.
        Ok(super::catalog_sql::CatalogConnection::new(
            acquired, operation, index,
        ))
    }
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    pub(crate) fn approval_test_controller(
        &self,
    ) -> std::sync::Arc<super::approval_owner::test_control::ApprovalTestController> {
        self.approval_hooks.clone()
    }
    pub(crate) fn management_audit(&self) -> super::management_audit::SqliteManagementAudit {
        super::management_audit::SqliteManagementAudit::from_registry(self.clone())
    }
    pub(super) async fn audit_connection(
        &self,
        #[cfg(all(test, windows))] hooks: &super::management_audit::test_hooks::AuditHooks,
    ) -> Result<RegistryConnection, RegistryError> {
        let lease = self.connections.lease()?;
        #[cfg(all(test, windows))]
        hooks
            .point(super::management_audit::AuditPoint::AfterLease)
            .await
            .map_err(|_| RegistryError::StorageUnavailable)?;
        let connection = lease.acquire(self.pool.clone()).await?;
        #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
        let mut connection = connection;
        #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
        connection.attach_approval_test_context(
            super::approval_owner::test_control::current_original_context(),
        );
        Ok(connection)
    }
    #[cfg(all(test, windows))]
    pub(crate) fn observe_authority_slots_for_test(
        &self,
    ) -> authority_slot_observation::OwnerObservation {
        authority_slot_observation::capture(&self.connections)
    }
    pub(crate) fn owner_scope(&self) -> RegistryOwnerScope {
        let owner: std::sync::Arc<dyn RegistryOwnerPort> = self.connections.clone();
        RegistryOwnerScope::from_owner(owner)
    }

    pub(crate) async fn resolve_native_target(
        &self,
        target: EntityId,
    ) -> Result<NativeSessionKey, RegistryError> {
        let scope = self.owner_scope();
        scope
            .while_open(async {
                let mut connection = self.detached_owned_connection().await?;
                let selected = async {
                    let row = sqlx::query(
                        "SELECT source_id,native_id FROM product_targets WHERE target_id=?",
                    )
                    .bind(target.encode())
                    .fetch_optional(connection.connection()?)
                    .await?
                    .ok_or(RegistryError::InvalidTarget)?;
                    let source: String = row
                        .try_get("source_id")
                        .map_err(|_| RegistryError::CorruptState)?;
                    let native: String = row
                        .try_get("native_id")
                        .map_err(|_| RegistryError::CorruptState)?;
                    let source =
                        EntityId::decode(&source).map_err(|_| RegistryError::CorruptState)?;
                    NativeSessionKey::from_config(Some(source), &native)
                        .map_err(|_| RegistryError::CorruptState)
                }
                .await;
                match connection.close().await {
                    CloseOutcome::Confirmed => selected,
                    CloseOutcome::Unconfirmed => Err(RegistryError::StorageUnavailable),
                }
            })
            .await
    }

    pub(crate) async fn resolve_control_root(
        &self,
        target: EntityId,
    ) -> Result<ResolvedControlRoot, RegistryError> {
        let scope = self.owner_scope();
        scope
            .while_open(async {
                let mut connection = self.detached_owned_connection().await?;
                let selected = resolve_control_root_inner(connection.connection()?, target).await;
                match connection.close().await {
                    CloseOutcome::Confirmed => selected,
                    CloseOutcome::Unconfirmed => Err(RegistryError::StorageUnavailable),
                }
            })
            .await
    }

    pub(crate) async fn trusted_client_display_name(
        &self,
        client_id: EntityId,
    ) -> Result<Option<String>, RegistryError> {
        let scope = self.owner_scope();
        scope
            .while_open(async {
                let mut connection = self.detached_owned_connection().await?;
                let selected = sqlx::query_scalar::<_, String>(
                    "SELECT display_name FROM main.clients WHERE client_id=? AND status='trusted'",
                )
                .bind(client_id.encode())
                .fetch_optional(connection.connection()?)
                .await
                .map_err(RegistryError::from);
                match connection.close().await {
                    CloseOutcome::Confirmed => selected,
                    CloseOutcome::Unconfirmed => Err(RegistryError::StorageUnavailable),
                }
            })
            .await
    }

    pub(crate) async fn append_control_security_audit(
        &self,
        actor_id: Option<&str>,
        action: &str,
        target_id: &str,
        outcome: &str,
        occurred_at: i64,
    ) -> Result<(), RegistryError> {
        let scope = self.owner_scope();
        scope
            .while_open(async {
                let mut connection = self.detached_owned_connection().await?;
                let res = async {
                    sqlx::query("BEGIN IMMEDIATE")
                        .execute(connection.connection()?)
                        .await?;
                    let insert = sqlx::query(
                        "INSERT INTO main.security_audit(actor_id, action, target_id, outcome, occurred_at) VALUES (?, ?, ?, ?, ?)",
                    )
                    .bind(actor_id)
                    .bind(action)
                    .bind(target_id)
                    .bind(outcome)
                    .bind(occurred_at)
                    .execute(connection.connection()?)
                    .await;
                    match insert {
                        Ok(_) => {
                            sqlx::query("COMMIT")
                                .execute(connection.connection()?)
                                .await
                                .map_err(|_| RegistryError::CommitUnconfirmed)?;
                            Ok(())
                        }
                        Err(err) => {
                            let _ = sqlx::query("ROLLBACK")
                                .execute(connection.connection()?)
                                .await;
                            Err(RegistryError::from(err))
                        }
                    }
                }
                .await;
                match connection.close().await {
                    CloseOutcome::Confirmed => res,
                    CloseOutcome::Unconfirmed => Err(RegistryError::StorageUnavailable),
                }
            })
            .await
    }

    #[cfg(test)]
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            connections: ConnectionOwner::new(),
            catalog_fence: std::sync::Arc::new(super::catalog_sql::CatalogSqlFence::new()),
            #[cfg(all(test,feature="approval-owner-acceptance-fixtures"))]
            approval_hooks:std::sync::Arc::new(super::approval_owner::test_control::ApprovalTestController::new_registry_controller()),
            #[cfg(test)]
            hooks: std::sync::Arc::default(),
            #[cfg(test)]
            native_hooks: std::sync::Arc::default(),
        }
    }
    #[cfg(not(test))]
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            connections: ConnectionOwner::new(),
            catalog_fence: std::sync::Arc::new(super::catalog_sql::CatalogSqlFence::new()),
        }
    }
    pub(crate) fn new_scoped(
        pool: SqlitePool,
        scope: std::sync::Weak<super::sqlite_lifecycle::SqliteOpenState>,
    ) -> Self {
        let mut registry = Self::new(pool);
        registry.connections = ConnectionOwner::new_scoped(scope);
        registry
    }
    pub(crate) async fn detached_owned_connection(
        &self,
    ) -> Result<RegistryConnection, RegistryError> {
        self.connections.acquire(self.pool.clone()).await
    }
    pub(crate) async fn trust_owned_connection(&self) -> Result<RegistryConnection, RegistryError> {
        self.connections.acquire_trust(self.pool.clone()).await
    }
    pub(super) async fn begin(&self) -> Result<RegistryConnection, RegistryError> {
        let mut connection = self.detached_owned_connection().await?;
        #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
        connection.attach_approval_test_context(
            super::approval_owner::test_control::current_original_context(),
        );
        sqlx::query("BEGIN IMMEDIATE")
            .execute(connection.connection()?)
            .await?;
        Ok(connection)
    }
    async fn finish<T>(
        &self,
        mut c: RegistryConnection,
        result: Result<T, RegistryError>,
        #[cfg(test)] kind: test_hooks::Operation,
    ) -> Result<T, RegistryError> {
        let transaction_result = async {
            match result {
                Ok(value) => {
                    #[cfg(test)]
                    if let Err(error) = self.hooks.hit(test_hooks::Point::BeforeCommit(kind)).await
                    {
                        sqlx::query("ROLLBACK").execute(c.connection()?).await?;
                        return Err(error);
                    }
                    sqlx::query("COMMIT")
                        .execute(c.connection()?)
                        .await
                        .map_err(|_| RegistryError::CommitUnconfirmed)?;
                    #[cfg(test)]
                    self.hooks.hit(test_hooks::Point::AfterCommit(kind)).await?;
                    Ok(value)
                }
                Err(error) => {
                    sqlx::query("ROLLBACK").execute(c.connection()?).await?;
                    Err(error)
                }
            }
        }
        .await;
        // Cleanup is independently owned. Its outcome cannot undo a confirmed COMMIT.
        c.close().await;
        transaction_result
    }
    pub(crate) async fn close_connections(&self) {
        self.connections.shutdown().await;
    }
    pub(crate) fn native_dispatch_repository(
        &self,
    ) -> super::native_dispatch::SqliteNativeDispatch {
        super::native_dispatch::SqliteNativeDispatch::new(self.clone())
    }
    #[cfg(test)]
    #[must_use]
    pub fn native_metadata_hooks_for_test(
        &self,
    ) -> std::sync::Arc<super::native_metadata_test_hooks::NativeMetadataHooks> {
        self.native_hooks.clone()
    }
    pub(crate) fn seal_connections(&self) {
        self.connections.seal();
    }
    pub(crate) fn close_status(&self) -> ConnectionCloseStatus {
        self.connections.status()
    }
    #[cfg(test)]
    pub(crate) fn acquisitions_for_test(
        &self,
    ) -> Vec<std::sync::Arc<super::AcquisitionObservation>> {
        self.connections.acquisitions_for_test()
    }
    #[cfg(test)]
    pub(crate) fn connection_closer_for_test(
        &self,
    ) -> std::sync::Arc<super::registry_connection_owner::test_hooks::ConnectionCloseController>
    {
        self.connections.hooks()
    }
    #[cfg(test)]
    pub(crate) fn faults_for_test(&self) -> std::sync::Arc<test_hooks::FaultController> {
        self.hooks.clone()
    }
}

async fn resolve_control_root_inner(
    connection: &mut SqliteConnection,
    target: EntityId,
) -> Result<ResolvedControlRoot, RegistryError> {
    let row = sqlx::query("SELECT source_id,native_id FROM main.product_targets WHERE target_id=?")
        .bind(target.encode())
        .fetch_optional(&mut *connection)
        .await?
        .ok_or(RegistryError::InvalidTarget)?;
    let source: String = row
        .try_get("source_id")
        .map_err(|_| RegistryError::CorruptState)?;
    let native: String = row
        .try_get("native_id")
        .map_err(|_| RegistryError::CorruptState)?;
    let source = EntityId::decode(&source).map_err(|_| RegistryError::CorruptState)?;
    let subject = NativeSessionKey::from_config(Some(source), &native)
        .map_err(|_| RegistryError::CorruptState)?;

    let mut current = subject;
    let mut ancestors = Vec::new();
    let mut seen = std::collections::HashSet::new();
    loop {
        if !seen.insert(current.clone()) {
            return Err(RegistryError::CorruptState);
        }
        let parent_value: Option<String> = sqlx::query_scalar(
            "SELECT parent_native_id FROM main.node_session_parents WHERE source_id=? AND child_native_id=?",
        )
        .bind(current.source().0.encode())
        .bind(current.native_id())
        .fetch_optional(&mut *connection)
        .await?;
        let next = parent_value
            .as_deref()
            .map(|value| {
                NativeSessionKey::from_config(Some(current.source().0), value)
                    .map_err(|_| RegistryError::CorruptState)
            })
            .transpose()?;
        let verified_root: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM main.node_session_capacity WHERE source_id=? AND native_id=? AND root_verified=1)",
        )
        .bind(current.source().0.encode())
        .bind(current.native_id())
        .fetch_one(&mut *connection)
        .await?;
        if verified_root != 0 && next.is_some() {
            return Err(RegistryError::CorruptState);
        }
        if let Some(parent) = next {
            ancestors.push(parent.clone());
            current = parent;
            continue;
        }
        if !ancestors.is_empty() {
            if verified_root == 0 {
                return Err(RegistryError::SessionTreeUnverified);
            }
        } else if verified_root == 0 {
            let has_capacity_row: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM main.node_session_capacity WHERE source_id=? AND native_id=?)",
            )
            .bind(current.source().0.encode())
            .bind(current.native_id())
            .fetch_one(&mut *connection)
            .await?;
            if has_capacity_row != 0 {
                return Err(RegistryError::SessionTreeUnverified);
            }
        }
        let root_target_id = if ancestors.is_empty() {
            Some(target)
        } else {
            let root_target: Option<String> = sqlx::query_scalar(
                "SELECT target_id FROM main.product_targets WHERE source_id=? AND native_id=?",
            )
            .bind(current.source().0.encode())
            .bind(current.native_id())
            .fetch_optional(&mut *connection)
            .await?;
            root_target
                .as_deref()
                .map(EntityId::decode)
                .transpose()
                .map_err(|_| RegistryError::CorruptState)?
        };
        return Ok(ResolvedControlRoot {
            root: current,
            root_target_id,
        });
    }
}

const READ: &str = "SELECT r.*,o.operation_id,o.runtime_epoch,o.admission_auth_generation,o.phase,o.begun_at_ms,o.resolved_at_ms,o.result_code,o.result_item_id,o.result_revision,o.restored_count,o.rejection_code FROM node_request_registrations r LEFT JOIN node_request_operations o USING(request_id) WHERE r.request_id=?";
pub(super) async fn settled_audit_basis(
    connection: &mut SqliteConnection,
    id: RegistrationId,
) -> Result<crate::domain::management_audit::SettledAuditBasis, RegistryError> {
    let row = sqlx::query("SELECT r.*,o.operation_id,o.runtime_epoch,o.admission_auth_generation,o.phase,o.begun_at_ms,o.resolved_at_ms,o.result_code,o.result_item_id,o.result_revision,o.restored_count,o.rejection_code FROM main.node_request_registrations r LEFT JOIN main.node_request_operations o ON o.request_id=r.request_id WHERE r.request_id=?")
        .bind(id.encode()).fetch_optional(connection).await?.ok_or(RegistryError::UnknownRegistration)?;
    if column::<Option<String>>(&row, "phase")?.as_deref() != Some("settled") {
        return Err(RegistryError::InvalidIntent);
    }
    let registration = registration(&row)?;
    let record = applying(&row, registration)?;
    let result = outcome(&row)?;
    record
        .registration
        .intent()
        .validate_outcome(id, result)
        .map_err(|_| RegistryError::CorruptState)?;
    Ok(crate::domain::management_audit::SettledAuditBasis {
        request: id,
        operation: record.operation,
        actor: record.registration.intent().actor(),
        action: record.registration.intent().action(),
        target: record.registration.intent().target(),
        register_generation: record.registration.register_auth_generation(),
        admission_generation: record.admission_auth_generation,
        runtime: record.runtime,
        outcome: result,
        event_at_ms: column::<Option<i64>>(&row, "resolved_at_ms")?
            .ok_or(RegistryError::CorruptState)?,
    })
}
fn column<'a, T: Decode<'a, Sqlite> + Type<Sqlite>>(
    row: &'a SqliteRow,
    name: &str,
) -> Result<T, RegistryError> {
    row.try_get(name).map_err(|_| RegistryError::CorruptState)
}
fn entity(value: &str) -> Result<EntityId, RegistryError> {
    EntityId::decode(value).map_err(|_| RegistryError::CorruptState)
}
fn revision(bytes: &[u8]) -> Result<u64, RegistryError> {
    let bytes: [u8; 8] = bytes.try_into().map_err(|_| RegistryError::CorruptState)?;
    Ok(u64::from_be_bytes(bytes))
}
fn registration(row: &SqliteRow) -> Result<RegistrationFact, RegistryError> {
    if column::<i64>(row, "intent_version")? != 1 {
        return Err(RegistryError::CorruptState);
    }
    let item: Option<String> = column(row, "item_id")?;
    let version: Option<Vec<u8>> = column(row, "expected_revision")?;
    let digest: Option<Vec<u8>> = column(row, "body_digest")?;
    let intent = RegisteredIntent::new(
        entity(&column::<String>(row, "actor_id")?)?,
        QueueAction::from_label(&column::<String>(row, "action")?)
            .ok_or(RegistryError::CorruptState)?,
        entity(&column::<String>(row, "target_id")?)?,
        item.as_deref().map(entity).transpose()?,
        version.as_deref().map(revision).transpose()?,
        digest
            .map(|v| <[u8; 32]>::try_from(v).map_err(|_| RegistryError::CorruptState))
            .transpose()?,
    )
    .map_err(|_| RegistryError::CorruptState)?;
    RegistrationFact::new(
        RegistrationId::new(entity(&column::<String>(row, "request_id")?)?),
        intent,
        column(row, "register_auth_generation")?,
        column(row, "registered_at_ms")?,
    )
    .map_err(|_| RegistryError::CorruptState)
}
fn applying(
    row: &SqliteRow,
    registration: RegistrationFact,
) -> Result<StoredApplying, RegistryError> {
    let generation = column::<i64>(row, "admission_auth_generation")?;
    if generation < 0 {
        return Err(RegistryError::CorruptState);
    }
    Ok(StoredApplying {
        registration,
        operation: entity(&column::<String>(row, "operation_id")?)?,
        runtime: RuntimeEpoch(entity(&column::<String>(row, "runtime_epoch")?)?),
        admission_auth_generation: generation,
        begun_at_ms: column(row, "begun_at_ms")?,
    })
}
fn outcome(row: &SqliteRow) -> Result<QueueOutcome, RegistryError> {
    let code: String = column(row, "result_code")?;
    let item: Option<String> = column(row, "result_item_id")?;
    let version: Option<Vec<u8>> = column(row, "result_revision")?;
    let count: Option<i64> = column(row, "restored_count")?;
    let reject: Option<String> = column(row, "rejection_code")?;
    if code == "resumed" {
        if item.is_some() || version.is_some() || reject.is_some() {
            return Err(RegistryError::CorruptState);
        }
        let restored = count
            .and_then(|v| u8::try_from(v).ok())
            .filter(|v| *v <= 3)
            .ok_or(RegistryError::CorruptState)?;
        return Ok(QueueOutcome::Resumed { restored });
    }
    if code == "rejected" {
        if item.is_some() || version.is_some() || count.is_some() {
            return Err(RegistryError::CorruptState);
        }
        return reject
            .as_deref()
            .and_then(QueueRejection::from_label)
            .map(QueueOutcome::Rejected)
            .ok_or(RegistryError::CorruptState);
    }
    if count.is_some() || reject.is_some() {
        return Err(RegistryError::CorruptState);
    }
    let item = entity(item.as_deref().ok_or(RegistryError::CorruptState)?)?;
    let revision = revision(version.as_deref().ok_or(RegistryError::CorruptState)?)?;
    match code.as_str() {
        "queued" => Ok(QueueOutcome::Queued { item, revision }),
        "direct_selected" => Ok(QueueOutcome::DirectSelected { item, revision }),
        "edited" => Ok(QueueOutcome::Edited { item, revision }),
        "cancelled" => Ok(QueueOutcome::Cancelled { item, revision }),
        "prioritized" => Ok(QueueOutcome::Prioritized { item, revision }),
        _ => Err(RegistryError::CorruptState),
    }
}
fn fact(row: &SqliteRow) -> Result<RequestFact, RegistryError> {
    let r = registration(row)?;
    match column::<i64>(row, "retired")? {
        1 => return Ok(RequestFact::Retired),
        0 => {}
        _ => return Err(RegistryError::CorruptState),
    }
    let operation: Option<String> = column(row, "operation_id")?;
    if operation.is_none() {
        return Ok(RequestFact::Registered(r));
    }
    let record = applying(row, r)?;
    let resolved: Option<i64> = column(row, "resolved_at_ms")?;
    match column::<String>(row, "phase")?.as_str() {
        "applying" if resolved.is_none() => Ok(RequestFact::Applying(record)),
        "settled" if resolved.is_some() => {
            let outcome = outcome(row)?;
            record
                .registration
                .intent()
                .validate_outcome(record.registration.id(), outcome)
                .map_err(|_| RegistryError::CorruptState)?;
            Ok(RequestFact::Settled {
                registration: record.registration,
                outcome,
            })
        }
        "interrupted" if resolved.is_some() => Ok(RequestFact::Interrupted {
            registration: record.registration,
        }),
        _ => Err(RegistryError::CorruptState),
    }
}
async fn read_row(
    c: &mut SqliteConnection,
    id: RegistrationId,
) -> Result<SqliteRow, RegistryError> {
    sqlx::query(READ)
        .bind(id.encode())
        .fetch_optional(c)
        .await?
        .ok_or(RegistryError::UnknownRegistration)
}
async fn current(
    c: &mut SqliteConnection,
    actor: &CurrentRequestActor,
    action: Option<QueueAction>,
) -> Result<(), RegistryError> {
    if actor.action() != action || actor.context().auth_generation < 0 {
        return Err(RegistryError::InvalidActor);
    }
    let found: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM clients WHERE client_id=? AND auth_generation=? AND status='trusted'",
    )
    .bind(actor.context().client_id.encode())
    .bind(actor.context().auth_generation)
    .fetch_one(&mut *c)
    .await?;
    if found != 1 {
        return Err(RegistryError::InvalidActor);
    }
    let found: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM product_targets WHERE target_id=? AND source_id=? AND native_id=?",
    )
    .bind(actor.target().encode())
    .bind(actor.native().source().0.encode())
    .bind(actor.native().native_id())
    .fetch_one(c)
    .await?;
    if found != 1 {
        return Err(RegistryError::InvalidTarget);
    }
    Ok(())
}
fn scope(actor: &CurrentRequestActor, intent: &RegisteredIntent) -> Result<(), RegistryError> {
    if actor.context().client_id != intent.actor() {
        return Err(RegistryError::InvalidActor);
    }
    if actor.target() != intent.target() || actor.action() != Some(intent.action()) {
        return Err(RegistryError::InvalidTarget);
    }
    Ok(())
}
struct ResultColumns {
    code: &'static str,
    item: Option<String>,
    revision: Option<Vec<u8>>,
    count: Option<i64>,
    rejection: Option<&'static str>,
}
fn result_columns(outcome: QueueOutcome) -> ResultColumns {
    let (code, item, version, count, rejection) = match outcome {
        QueueOutcome::Queued { item, revision } => {
            ("queued", Some(item), Some(revision), None, None)
        }
        QueueOutcome::DirectSelected { item, revision } => {
            ("direct_selected", Some(item), Some(revision), None, None)
        }
        QueueOutcome::Edited { item, revision } => {
            ("edited", Some(item), Some(revision), None, None)
        }
        QueueOutcome::Cancelled { item, revision } => {
            ("cancelled", Some(item), Some(revision), None, None)
        }
        QueueOutcome::Prioritized { item, revision } => {
            ("prioritized", Some(item), Some(revision), None, None)
        }
        QueueOutcome::Resumed { restored } => {
            ("resumed", None, None, Some(i64::from(restored)), None)
        }
        QueueOutcome::Rejected(error) => ("rejected", None, None, None, Some(error.label())),
    };
    ResultColumns {
        code,
        item: item.map(EntityId::encode),
        revision: version.map(|v| v.to_be_bytes().to_vec()),
        count,
        rejection,
    }
}

impl SqliteRequestRegistry {
    async fn register_inner(
        &self,
        actor: &CurrentRequestActor,
        intent: &RegisteredIntent,
        id: RegistrationId,
        at: &OperationContext,
        deadline: &RegistryDeadline,
        generated_id: bool,
    ) -> Result<RegistrationFact, RegistryError> {
        scope(actor, intent)?;
        deadline.run(async {
            let mut c = self.begin().await?;
            let result = async {
                current(c.connection()?, actor, Some(intent.action())).await?;
                #[cfg(test)] self.hooks.hit(test_hooks::Point::RegisterInsert).await?;
                let result = sqlx::query("INSERT INTO node_request_registrations(request_id,intent_version,actor_id,register_auth_generation,action,target_id,item_id,expected_revision,body_digest,registered_at_ms) VALUES(?,1,?,?,?,?,?,?,?,?)")
                    .bind(id.encode()).bind(intent.actor().encode()).bind(actor.context().auth_generation)
                    .bind(intent.action().label()).bind(intent.target().encode()).bind(intent.item().map(EntityId::encode))
                    .bind(intent.expected_revision().map(|v| v.to_be_bytes().to_vec())).bind(intent.body_digest().map(|v| v.to_vec()))
                    .bind(at.observed_at_ms).execute(c.connection()?).await;
                if let Err(error) = result {
                    if error
                        .as_database_error()
                        .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
                    {
                        let row = read_row(c.connection()?, id).await?;
                        let existing = registration(&row)?;
                        return if existing.intent() == intent && !generated_id {
                            Ok(existing)
                        } else {
                            Err(RegistryError::Conflict)
                        };
                    }
                    return Err(RegistryError::StorageUnavailable);
                }
                RegistrationFact::new(
                    id,
                    intent.clone(),
                    actor.context().auth_generation,
                    at.observed_at_ms,
                )
            }
            .await;
            self.finish(
                c,
                result,
                #[cfg(test)]
                test_hooks::Operation::Register,
            )
            .await
        })
        .await
    }
}

impl RequestRegistryRepository for SqliteRequestRegistry {
    async fn register(
        &self,
        actor: &CurrentRequestActor,
        intent: &RegisteredIntent,
        id: RegistrationId,
        at: &OperationContext,
        deadline: &RegistryDeadline,
    ) -> Result<RegistrationFact, RegistryError> {
        self.register_inner(actor, intent, id, at, deadline, false)
            .await
    }
    async fn register_generated(
        &self,
        actor: &CurrentRequestActor,
        intent: &RegisteredIntent,
        id: RegistrationId,
        at: &OperationContext,
        deadline: &RegistryDeadline,
    ) -> Result<RegistrationFact, RegistryError> {
        self.register_inner(actor, intent, id, at, deadline, true)
            .await
    }
    async fn begin_consume(
        &self,
        actor: &CurrentRequestActor,
        id: RegistrationId,
        intent: &RegisteredIntent,
        operation: EntityId,
        at: &OperationContext,
        deadline: &RegistryDeadline,
    ) -> Result<BeginRecord, RegistryError> {
        scope(actor, intent)?;
        deadline.run(async {
            let mut c = self.begin().await?;
            let result = async {
                current(c.connection()?,actor,Some(intent.action())).await?;
                let row = read_row(c.connection()?,id).await?;
                let original = registration(&row)?;
                if original.intent() != intent { return Err(RegistryError::Conflict); }
                let original_fact = fact(&row)?;
                match original_fact {
                    RequestFact::Retired => return Err(RegistryError::Retired),
                    RequestFact::Registered(_) => {},
                    other => return Ok(BeginRecord::Existing(other)),
                }
                #[cfg(test)] self.hooks.hit(test_hooks::Point::BeginInsert).await?;
                sqlx::query("INSERT INTO node_request_operations(request_id,operation_id,runtime_epoch,admission_auth_generation,phase,begun_at_ms) VALUES(?,?,?,?,'applying',?)")
                    .bind(id.encode()).bind(operation.encode()).bind(at.runtime.0.encode()).bind(actor.context().auth_generation)
                    .bind(at.observed_at_ms).execute(c.connection()?).await?;
                Ok(BeginRecord::Started(StoredApplying { registration:original,operation,runtime:at.runtime,
                    admission_auth_generation:actor.context().auth_generation,begun_at_ms:at.observed_at_ms }))
            }.await;
            self.finish(c,result, #[cfg(test)] test_hooks::Operation::Begin).await
        }).await
    }
    async fn settle(
        &self,
        lease: &ConsumeLease,
        outcome: &QueueOutcome,
        at: &OperationContext,
        deadline: &RegistryDeadline,
    ) -> Result<RequestFact, RegistryError> {
        lease
            .intent()
            .validate_outcome(lease.registration_id(), *outcome)?;
        if at.runtime != lease.runtime() {
            return Err(RegistryError::StaleObservation);
        }
        deadline.run(async {
            let mut c = self.begin().await?;
            let result = async {
                let row = read_row(c.connection()?,lease.registration_id()).await?;
                let r = registration(&row)?;
                if applying(&row,r.clone())? != *lease.stored() { return Err(RegistryError::Conflict); }
                match fact(&row)? {
                    RequestFact::Settled {outcome:old,..} if old == *outcome => return fact(&row),
                    RequestFact::Applying(_) => {},
                    _ => return Err(RegistryError::Conflict),
                }
                #[cfg(test)] self.hooks.hit(test_hooks::Point::SettleUpdate).await?;
                let v = result_columns(*outcome);
                let changed = sqlx::query("UPDATE node_request_operations SET phase='settled',resolved_at_ms=?,result_code=?,result_item_id=?,result_revision=?,restored_count=?,rejection_code=? WHERE request_id=? AND operation_id=? AND runtime_epoch=? AND phase='applying'")
                    .bind(at.observed_at_ms).bind(v.code).bind(v.item).bind(v.revision).bind(v.count).bind(v.rejection)
                    .bind(lease.registration_id().encode()).bind(lease.operation().encode()).bind(lease.runtime().0.encode())
                    .execute(c.connection()?).await?;
                if changed.rows_affected() != 1 { return Err(RegistryError::Conflict); }
                Ok(RequestFact::Settled { registration:r, outcome:*outcome })
            }.await;
            self.finish(c,result, #[cfg(test)] test_hooks::Operation::Settle).await
        }).await
    }
    async fn read(
        &self,
        reader: &CurrentRequestActor,
        target: EntityId,
        id: RegistrationId,
        deadline: &RegistryDeadline,
    ) -> Result<RequestFact, RegistryError> {
        if reader.target() != target {
            return Err(RegistryError::InvalidTarget);
        }
        deadline
            .run(async {
                let mut c = self.begin().await?;
                let result = async {
                    current(c.connection()?, reader, None).await?;
                    let row = read_row(c.connection()?, id).await?;
                    if registration(&row)?.intent().target() != target {
                        return Err(RegistryError::InvalidTarget);
                    }
                    fact(&row)
                }
                .await;
                self.finish(
                    c,
                    result,
                    #[cfg(test)]
                    test_hooks::Operation::Read,
                )
                .await
            })
            .await
    }
    async fn inspect_operation(
        &self,
        probe: &OperationProbe,
        deadline: &RegistryDeadline,
    ) -> Result<RequestFact, RegistryError> {
        deadline
            .run(async {
                let mut c = self.begin().await?;
                let result = async {
                    let row = read_row(c.connection()?, probe.registration_id()).await?;
                    let r = registration(&row)?;
                    if r.intent() != probe.intent() {
                        return Err(RegistryError::Conflict);
                    }
                    if column::<Option<String>>(&row, "operation_id")?.is_some()
                        && !probe.matches(&applying(&row, r)?)
                    {
                        return Err(RegistryError::CorruptState);
                    }
                    fact(&row)
                }
                .await;
                self.finish(
                    c,
                    result,
                    #[cfg(test)]
                    test_hooks::Operation::Inspect,
                )
                .await
            })
            .await
    }
    async fn interrupt_previous_runtime(
        &self,
        current: RuntimeEpoch,
        at: &OperationContext,
        deadline: &RegistryDeadline,
    ) -> Result<u64, RegistryError> {
        if at.runtime != current {
            return Err(RegistryError::InvalidIntent);
        }
        deadline.run(async {
            let mut c = self.begin().await?;
            let result = async {
                #[cfg(test)] self.hooks.hit(test_hooks::Point::StartupUpdate).await?;
                Ok(sqlx::query("UPDATE node_request_operations SET phase='interrupted',resolved_at_ms=? WHERE phase='applying' AND runtime_epoch!=?")
                    .bind(at.observed_at_ms).bind(current.0.encode()).execute(c.connection()?).await?.rows_affected())
            }.await;
            self.finish(c,result, #[cfg(test)] test_hooks::Operation::Startup).await
        }).await
    }
}

#[cfg(test)]
pub(crate) mod test_hooks {
    use super::RegistryError;
    use std::sync::{Arc, Mutex};
    use tokio::sync::Semaphore;
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum Operation {
        Register,
        Begin,
        Settle,
        Read,
        Inspect,
        Startup,
    }
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum Point {
        RegisterInsert,
        BeginInsert,
        SettleUpdate,
        StartupUpdate,
        BeforeCommit(Operation),
        AfterCommit(Operation),
    }
    pub enum Action {
        Fail,
        Unconfirmed,
        Pause(Arc<Barrier>),
    }
    pub struct Barrier {
        arrived: Semaphore,
        released: Semaphore,
    }
    impl Barrier {
        pub fn new() -> Arc<Self> {
            Arc::new(Self {
                arrived: Semaphore::new(0),
                released: Semaphore::new(0),
            })
        }
        pub async fn arrived(&self) {
            if let Ok(p) = self.arrived.acquire().await {
                p.forget();
            }
        }
        pub fn release(&self) {
            self.released.add_permits(1);
        }
        async fn pause(&self) {
            self.arrived.add_permits(1);
            if let Ok(p) = self.released.acquire().await {
                p.forget();
            }
        }
    }
    #[derive(Default)]
    pub struct FaultController {
        next: Mutex<Option<(Point, Action)>>,
    }
    impl FaultController {
        pub fn arm(&self, point: Point, action: Action) -> Result<(), RegistryError> {
            let mut next = self
                .next
                .lock()
                .map_err(|_| RegistryError::StorageUnavailable)?;
            if next.is_some() {
                return Err(RegistryError::Conflict);
            }
            if matches!(action, Action::Unconfirmed) && !matches!(point, Point::AfterCommit(_)) {
                return Err(RegistryError::InvalidIntent);
            }
            *next = Some((point, action));
            drop(next);
            Ok(())
        }
        pub(super) async fn hit(&self, point: Point) -> Result<(), RegistryError> {
            let action = {
                let mut next = self
                    .next
                    .lock()
                    .map_err(|_| RegistryError::StorageUnavailable)?;
                if next.as_ref().is_some_and(|(p, _)| *p == point) {
                    next.take().map(|(_, a)| a)
                } else {
                    None
                }
            };
            match action {
                Some(Action::Fail) => Err(RegistryError::StorageUnavailable),
                Some(Action::Unconfirmed) => Err(RegistryError::CommitUnconfirmed),
                Some(Action::Pause(b)) => {
                    b.pause().await;
                    Ok(())
                }
                None => Ok(()),
            }
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "../../tests/registration_acceptance/f/registry_sql_fault_acceptance.rs"]
mod independent_registry_sql;

#[cfg(test)]
#[path = "../../tests/registration_acceptance/f/factory_waiter_sql_acceptance.rs"]
mod independent_factory_sql;

#[cfg(all(test, windows))]
#[path = "../../tests/registration_acceptance/close_owner/storage_physical_close_acceptance.rs"]
mod independent_physical_close;
