//! Catalog-only serialization and original SQL-slot responsibility, on the existing registry owner.
use super::registry_connection_owner::{
    CloseOutcome, OriginalCleanupObservation, OriginalSqlCloseReceipt, RegistryConnection,
};
use crate::application::native_dispatch::BoxFuture;
use crate::domain::native_catalog::CatalogRequestId;
use crate::{
    application::request_registry::{RegistryDeadline, RegistryOwnerScope},
    domain::native_catalog::{
        CatalogError, CatalogFailure, CatalogOperationKind, CatalogSqlObservation,
        CatalogSqlObservationSet, CatalogSqlRole, CleanupKnowledge, CommitKnowledge,
        TransactionKnowledge,
    },
};
use sqlx::SqliteConnection;
pub(super) trait ReconcileEvidence: Send + Sync {
    fn request_id(&self) -> CatalogRequestId;
    fn verify<'a>(
        &'a self,
        sql: &'a mut SqliteConnection,
    ) -> BoxFuture<'a, Result<(), CatalogError>>;
}
use std::sync::{Arc, Mutex, MutexGuard};

pub(super) struct CatalogSqlFence {
    serial: Arc<tokio::sync::Mutex<()>>,
    retained: Mutex<Option<Arc<CatalogOperation>>>,
    #[cfg(test)]
    waiting: std::sync::atomic::AtomicUsize,
}
impl CatalogSqlFence {
    pub(super) fn new() -> Self {
        Self {
            serial: Arc::new(tokio::sync::Mutex::new(())),
            retained: Mutex::new(None),
            #[cfg(test)]
            waiting: std::sync::atomic::AtomicUsize::new(0),
        }
    }
    pub(super) async fn enter(
        self: &Arc<Self>,
        kind: CatalogOperationKind,
        scope: &RegistryOwnerScope,
        d: &RegistryDeadline,
    ) -> Result<CatalogGuard, CatalogFailure> {
        let serial = d
            .run(scope.while_open(async {
                #[cfg(test)]
                let _waiting = GateWaitObservation::new(&self.waiting);
                Ok(self.serial.clone().lock_owned().await)
            }))
            .await
            .map_err(|e| empty_failure(kind, e.into()))?;
        let previous = self
            .retained
            .lock()
            .map_err(|_| empty_failure(kind, CatalogError::StorageUnavailable))?
            .clone();
        if let Some(operation) = previous {
            if !operation.interrupted_configuration_read(scope) {
                let cleanup = operation.failure(CatalogError::CommitUnconfirmed).cleanup();
                return Err(
                    operation.failure(if cleanup == CleanupKnowledge::ConfirmedOriginal {
                        CatalogError::CommitUnconfirmed
                    } else {
                        CatalogError::SqlCloseUnconfirmed { committed: false }
                    }),
                );
            }
            // A disconnected reader can leave its original close receipt with the fence.
            // Keep the serial guard until that specific SQL worker has actually joined.
            operation.confirm_cleanup(d).await?;
            scope
                .require_open()
                .map_err(|e| operation.failure(e.into()))?;
        }
        let operation = Arc::new(CatalogOperation {
            kind,
            state: Mutex::new(OperationState {
                slots: [None, None],
                scope: None,
                effect: None,
                request: None,
                recovered: false,
                reconciled: false,
                recovery_verified: false,
                archived: [None, None],
            }),
        });
        *self
            .retained
            .lock()
            .map_err(|_| empty_failure(kind, CatalogError::StorageUnavailable))? =
            Some(operation.clone());
        Ok(CatalogGuard {
            fence: self.clone(),
            operation,
            _serial: serial,
        })
    }
}
#[cfg(test)]
struct GateWaitObservation<'a>(&'a std::sync::atomic::AtomicUsize);
#[cfg(test)]
impl<'a> GateWaitObservation<'a> {
    fn new(waiting: &'a std::sync::atomic::AtomicUsize) -> Self {
        waiting.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Self(waiting)
    }
}
#[cfg(test)]
impl Drop for GateWaitObservation<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}
pub(super) struct CatalogGuard {
    fence: Arc<CatalogSqlFence>,
    pub(super) operation: Arc<CatalogOperation>,
    _serial: tokio::sync::OwnedMutexGuard<()>,
}
impl CatalogSqlFence {
    async fn recovery_guard(
        self: &Arc<Self>,
        d: &RegistryDeadline,
    ) -> Result<Option<CatalogGuard>, CatalogFailure> {
        let serial = d
            .run(async { Ok(self.serial.clone().lock_owned().await) })
            .await
            .map_err(|e| empty_failure(CatalogOperationKind::Reconcile, e.into()))?;
        let operation = {
            self.retained
                .lock()
                .map_err(|_| {
                    empty_failure(
                        CatalogOperationKind::Reconcile,
                        CatalogError::StorageUnavailable,
                    )
                })?
                .clone()
        };
        let Some(operation) = operation else {
            drop(serial);
            return Ok(None);
        };
        Ok(Some(CatalogGuard {
            fence: self.clone(),
            operation,
            _serial: serial,
        }))
    }
    pub(super) async fn recover(
        self: &Arc<Self>,
        registry: &super::request_registry::SqliteRequestRegistry,
        d: &RegistryDeadline,
    ) -> Result<(), CatalogFailure> {
        let Some(guard) = self.recovery_guard(d).await? else {
            return registry
                .owner_scope()
                .require_open()
                .map_err(|e| empty_failure(CatalogOperationKind::Reconcile, e.into()));
        };
        let operation = guard.operation.clone();
        let own = registry.owner_scope();
        let same_scope = {
            operation
                .state()
                .scope
                .as_ref()
                .is_some_and(|original| original.matches(&own))
        };
        if !same_scope {
            return Err(operation.failure(CatalogError::InvalidActor));
        }
        operation.confirm_cleanup(d).await?;
        let needs_reconcile = {
            let s = operation.state();
            matches!(
                business_commit(&s),
                CommitKnowledge::WritePossible | CommitKnowledge::Unknown
            ) && !s.reconciled
        };
        if !needs_reconcile {
            operation.state().recovered = true;
            drop(guard);
            return registry
                .owner_scope()
                .require_open()
                .map_err(|e| operation.failure(e.into()));
        }
        if operation.state().recovery_verified {
            let mut s = operation.state();
            s.recovered = true;
            s.reconciled = true;
            drop(s);
            drop(guard);
            return registry
                .owner_scope()
                .require_open()
                .map_err(|e| operation.failure(e.into()));
        }
        registry
            .owner_scope()
            .require_open()
            .map_err(|e| operation.failure(e.into()))?;
        let _lease = registry
            .catalog_operation_lease()
            .map_err(|e| operation.failure(e.into()))?;
        let effect = { operation.state().effect.clone() };
        let effect = effect.ok_or_else(|| operation.failure(CatalogError::CorruptState))?;
        {
            let mut s = operation.state();
            for i in 0..2 {
                if s.slots[i]
                    .as_ref()
                    .is_some_and(|v| !is_write(v.observation.role()))
                {
                    s.archived[i] = s.slots[i].as_ref().map(|v| v.observation);
                    s.slots[i] = None;
                }
            }
        }
        let scope = registry.owner_scope();
        let read = d
            .run(scope.while_open(async {
                Ok(async {
                    let mut c = registry
                        .acquire_catalog_connection(operation.clone(), CatalogSqlRole::RecoveryRead)
                        .await?;
                    c.begin_read().await?;
                    let result = effect.verify(c.sql()?).await;
                    if result.is_ok() {
                        operation.state().recovery_verified = true;
                    }
                    c.finish_catalog(result, d).await
                }
                .await)
            }))
            .await;
        match read {
            Ok(Ok(())) => {
                let mut s = operation.state();
                s.recovered = true;
                s.reconciled = true;
            }
            Ok(Err(e)) => return Err(operation.failure(e)),
            Err(e) => return Err(operation.failure(e.into())),
        }
        drop(guard);
        registry
            .owner_scope()
            .require_open()
            .map_err(|e| operation.failure(e.into()))
    }
}
impl Drop for CatalogGuard {
    fn drop(&mut self) {
        if self.operation.clean()
            && let Ok(mut retained) = self.fence.retained.lock()
        {
            *retained = None;
        }
    }
}
struct SlotState {
    observation: CatalogSqlObservation,
    receipt: Option<OriginalSqlCloseReceipt>,
}
struct OperationState {
    slots: [Option<SlotState>; 2],
    scope: Option<crate::application::request_registry::RegistryOwnerScope>,
    effect: Option<Arc<dyn ReconcileEvidence>>,
    request: Option<CatalogRequestId>,
    recovered: bool,
    reconciled: bool,
    recovery_verified: bool,
    archived: [Option<CatalogSqlObservation>; 2],
}
pub(super) struct CatalogOperation {
    kind: CatalogOperationKind,
    state: Mutex<OperationState>,
}
impl CatalogOperation {
    async fn confirm_cleanup(&self, d: &RegistryDeadline) -> Result<(), CatalogFailure> {
        for index in 0..2 {
            let pending = {
                let s = self.state();
                s.slots[index]
                    .as_ref()
                    .filter(|v| v.observation.cleanup() != CleanupKnowledge::ConfirmedOriginal)
                    .map(|v| v.receipt.clone())
            };
            if let Some(receipt) = pending {
                let receipt = receipt.ok_or_else(|| {
                    self.failure(CatalogError::SqlCloseUnconfirmed { committed: false })
                })?;
                let observation = d
                    .run(async { Ok(receipt.confirm_actual_cleanup(d.deadline()).await) })
                    .await
                    .map_err(|e| self.failure(e.into()))?;
                if observation != OriginalCleanupObservation::ConfirmedJoined {
                    return Err(
                        self.failure(CatalogError::SqlCloseUnconfirmed { committed: false })
                    );
                }
                self.update(index, None, Some(CleanupKnowledge::ConfirmedOriginal), None);
                if let Some(v) = self.state().slots[index].as_mut() {
                    v.receipt = None;
                }
            }
        }
        Ok(())
    }
    pub(super) fn bind_scope(
        &self,
        scope: crate::application::request_registry::RegistryOwnerScope,
    ) -> Result<(), CatalogError> {
        let mut s = self
            .state
            .lock()
            .map_err(|_| CatalogError::StorageUnavailable)?;
        if s.scope
            .as_ref()
            .is_some_and(|original| !original.matches(&scope))
        {
            return Err(CatalogError::InvalidActor);
        }
        if s.scope.is_none() {
            s.scope = Some(scope);
        }
        drop(s);
        Ok(())
    }
    fn state(&self) -> MutexGuard<'_, OperationState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    pub(super) fn attach(&self, role: CatalogSqlRole) -> Result<usize, CatalogError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| CatalogError::StorageUnavailable)?;
        let index = state
            .slots
            .iter()
            .position(Option::is_none)
            .ok_or(CatalogError::CorruptState)?;
        state.slots[index] = Some(SlotState {
            observation: CatalogSqlObservation::new(
                role,
                TransactionKnowledge::NoTransaction,
                CleanupKnowledge::PendingOriginal,
            ),
            receipt: None,
        });
        drop(state);
        Ok(index)
    }
    fn update(
        &self,
        index: usize,
        transaction: Option<TransactionKnowledge>,
        cleanup: Option<CleanupKnowledge>,
        receipt: Option<OriginalSqlCloseReceipt>,
    ) {
        if let Some(slot) = self.state().slots.get_mut(index).and_then(Option::as_mut) {
            slot.observation = CatalogSqlObservation::new(
                slot.observation.role(),
                transaction.unwrap_or_else(|| slot.observation.transaction()),
                cleanup.unwrap_or_else(|| slot.observation.cleanup()),
            );
            if receipt.is_some() {
                slot.receipt = receipt;
            }
        }
    }
    fn interrupted_configuration_read(&self, scope: &RegistryOwnerScope) -> bool {
        let state = self.state();
        self.kind == CatalogOperationKind::ResolveRead
            && state
                .scope
                .as_ref()
                .is_some_and(|owner| owner.matches(scope))
            && matches!(&state.slots, [Some(slot), None]
                if slot.observation.role() == CatalogSqlRole::ConfigurationRead
                    && slot.observation.cleanup() == CleanupKnowledge::PendingOriginal
                    && slot.receipt.is_some())
    }
    fn clean(&self) -> bool {
        self.state.lock().is_ok_and(|s| {
            s.slots.iter().flatten().all(|v| {
                v.observation.cleanup() == CleanupKnowledge::ConfirmedOriginal
                    && (s.recovered
                        || !matches!(
                            v.observation.transaction(),
                            TransactionKnowledge::EffectUnknown
                                | TransactionKnowledge::CommitPolling
                        ))
            }) && (s.reconciled
                || !matches!(
                    business_commit(&s),
                    CommitKnowledge::Unknown | CommitKnowledge::WritePossible
                ))
        })
    }
    pub(super) fn set_request(&self, id: CatalogRequestId) {
        self.state().request = Some(id);
    }
    pub(super) fn set_effect(&self, effect: Arc<dyn ReconcileEvidence>) {
        let mut s = self.state();
        s.request = Some(effect.request_id());
        s.effect = Some(effect);
    }
    pub(super) fn observation(&self) -> CatalogSqlObservationSet {
        let s = self.state();
        CatalogSqlObservationSet::new(
            self.kind,
            std::array::from_fn(|i| s.slots[i].as_ref().map(|v| v.observation)),
        )
    }
    pub(super) fn failure(&self, kind: CatalogError) -> CatalogFailure {
        let state = self.state();
        let slots = std::array::from_fn(|i| state.slots[i].as_ref().map(|s| s.observation));
        let cleanup = if slots
            .iter()
            .flatten()
            .any(|s| s.cleanup() != CleanupKnowledge::ConfirmedOriginal)
        {
            CleanupKnowledge::UnconfirmedOriginal
        } else if slots.iter().any(Option::is_some) {
            CleanupKnowledge::ConfirmedOriginal
        } else {
            CleanupKnowledge::NoDetachedConnection
        };
        let kind = if matches!(kind, CatalogError::SqlCloseUnconfirmed { .. }) {
            CatalogError::SqlCloseUnconfirmed {
                committed: business_commit(&state) == CommitKnowledge::Confirmed,
            }
        } else {
            kind
        };
        CatalogFailure::new(
            kind,
            state.request,
            business_commit(&state),
            cleanup,
            CatalogSqlObservationSet::new(self.kind, slots),
        )
    }
}
const fn is_write(role: CatalogSqlRole) -> bool {
    matches!(
        role,
        CatalogSqlRole::RegisterWrite | CatalogSqlRole::ApplyWrite
    )
}
fn business_commit(s: &OperationState) -> CommitKnowledge {
    s.slots
        .iter()
        .flatten()
        .find(|v| is_write(v.observation.role()))
        .map_or(CommitKnowledge::NoWrite, |v| {
            match v.observation.transaction() {
                TransactionKnowledge::NoTransaction => CommitKnowledge::NoWrite,
                TransactionKnowledge::BegunWrite => CommitKnowledge::WritePossible,
                TransactionKnowledge::CommitConfirmed => CommitKnowledge::Confirmed,
                TransactionKnowledge::RollbackConfirmed => CommitKnowledge::RolledBack,
                _ => CommitKnowledge::Unknown,
            }
        })
}
const fn empty_failure(kind: CatalogOperationKind, error: CatalogError) -> CatalogFailure {
    CatalogFailure::new(
        error,
        None,
        CommitKnowledge::NoWrite,
        CleanupKnowledge::NoDetachedConnection,
        CatalogSqlObservationSet::new(kind, [None, None]),
    )
}
/// Acquired before any BEGIN await; Drop preserves the same original slot's receipt.
pub(super) struct CatalogConnection {
    inner: Option<RegistryConnection>,
    operation: Arc<CatalogOperation>,
    index: usize,
}
impl CatalogConnection {
    pub(super) const fn new(
        inner: RegistryConnection,
        operation: Arc<CatalogOperation>,
        index: usize,
    ) -> Self {
        Self {
            inner: Some(inner),
            operation,
            index,
        }
    }
    pub(super) fn sql(&mut self) -> Result<&mut SqliteConnection, CatalogError> {
        self.inner
            .as_mut()
            .ok_or(CatalogError::CorruptState)?
            .connection()
            .map_err(Into::into)
    }
    pub(super) async fn begin_read(&mut self) -> Result<(), CatalogError> {
        self.operation.update(
            self.index,
            Some(TransactionKnowledge::EffectUnknown),
            None,
            None,
        );
        sqlx::query("BEGIN IMMEDIATE")
            .execute(self.sql()?)
            .await
            .map_err(|_| CatalogError::StorageUnavailable)?;
        self.operation.update(
            self.index,
            Some(TransactionKnowledge::BegunRead),
            None,
            None,
        );
        verify_namespace(self.sql()?).await
    }
    pub(super) async fn begin_write(&mut self) -> Result<(), CatalogError> {
        self.operation.update(
            self.index,
            Some(TransactionKnowledge::EffectUnknown),
            None,
            None,
        );
        sqlx::query("BEGIN IMMEDIATE")
            .execute(self.sql()?)
            .await
            .map_err(|_| CatalogError::StorageUnavailable)?;
        self.operation.update(
            self.index,
            Some(TransactionKnowledge::BegunWrite),
            None,
            None,
        );
        verify_namespace(self.sql()?).await
    }
    pub(super) async fn finish_catalog<T>(
        mut self,
        result: Result<T, CatalogError>,
        d: &RegistryDeadline,
    ) -> Result<T, CatalogError> {
        let result = match result {
            Ok(value) => {
                self.operation.update(
                    self.index,
                    Some(TransactionKnowledge::CommitPolling),
                    None,
                    None,
                );
                if sqlx::query("COMMIT").execute(self.sql()?).await.is_ok() {
                    self.operation.update(
                        self.index,
                        Some(TransactionKnowledge::CommitConfirmed),
                        None,
                        None,
                    );
                    Ok(value)
                } else {
                    self.operation.update(
                        self.index,
                        Some(TransactionKnowledge::EffectUnknown),
                        None,
                        None,
                    );
                    Err(CatalogError::CommitUnconfirmed)
                }
            }
            Err(error) => {
                match sqlx::query("ROLLBACK").execute(self.sql()?).await {
                    Ok(_) => self.operation.update(
                        self.index,
                        Some(TransactionKnowledge::RollbackConfirmed),
                        None,
                        None,
                    ),
                    Err(_) => self.operation.update(
                        self.index,
                        Some(TransactionKnowledge::EffectUnknown),
                        None,
                        None,
                    ),
                }
                Err(error)
            }
        };
        let waiter = self.handoff()?;
        let outcome = d
            .run(async { Ok(waiter.wait_report().await) })
            .await
            .map_err(CatalogError::from)?;
        match outcome {
            CloseOutcome::Confirmed => {
                self.operation.update(
                    self.index,
                    None,
                    Some(CleanupKnowledge::ConfirmedOriginal),
                    None,
                );
                result
            }
            CloseOutcome::Unconfirmed => {
                self.operation.update(
                    self.index,
                    None,
                    Some(CleanupKnowledge::UnconfirmedOriginal),
                    None,
                );
                Err(CatalogError::SqlCloseUnconfirmed {
                    committed: self
                        .operation
                        .failure(CatalogError::CommitUnconfirmed)
                        .commit()
                        == CommitKnowledge::Confirmed,
                })
            }
        }
    }
    fn handoff(
        &mut self,
    ) -> Result<super::registry_connection_owner::OriginalSqlCloseWaiter, CatalogError> {
        let inner = self.inner.take().ok_or(CatalogError::CorruptState)?;
        match inner.begin_owned_close() {
            Ok(waiter) => {
                self.operation.update(
                    self.index,
                    None,
                    Some(CleanupKnowledge::PendingOriginal),
                    Some(waiter.receipt()),
                );
                Ok(waiter)
            }
            Err(e) => {
                self.operation.update(
                    self.index,
                    None,
                    Some(CleanupKnowledge::UnconfirmedOriginal),
                    None,
                );
                Err(e.into())
            }
        }
    }
}
impl Drop for CatalogConnection {
    fn drop(&mut self) {
        if self.inner.is_some() {
            let _ = self.handoff();
        }
    }
}

#[cfg(test)]
mod tests;

async fn verify_namespace(sql: &mut SqliteConnection) -> Result<(), CatalogError> {
    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA main.foreign_keys")
        .fetch_one(&mut *sql)
        .await
        .map_err(|_| CatalogError::StorageUnavailable)?;
    if foreign_keys != 1 {
        return Err(CatalogError::CorruptState);
    }
    let shadows:i64=sqlx::query_scalar("SELECT count(*) FROM temp.sqlite_schema WHERE substr(lower(name),1,7)='pragma_' OR name COLLATE NOCASE IN ('node_catalog_commands','node_launch_profiles','node_target_launch_selection','clients','node_meta','product_sources','product_harness_sources','product_targets','harnesses','workspaces') OR (type='trigger' AND tbl_name COLLATE NOCASE IN ('node_catalog_commands','node_launch_profiles','node_target_launch_selection','clients','node_meta','product_sources','product_harness_sources','product_targets','harnesses','workspaces'))").fetch_one(&mut *sql).await.map_err(|_|CatalogError::StorageUnavailable)?;
    let triggers:i64=sqlx::query_scalar("SELECT count(*) FROM main.sqlite_schema WHERE substr(lower(name),1,7)='pragma_' OR (type='trigger' AND tbl_name COLLATE NOCASE IN ('node_catalog_commands','node_launch_profiles','node_target_launch_selection','clients','node_meta','product_sources','product_harness_sources','product_targets','harnesses','workspaces'))").fetch_one(sql).await.map_err(|_|CatalogError::StorageUnavailable)?;
    if shadows != 0 || triggers != 0 {
        return Err(CatalogError::CorruptState);
    }
    Ok(())
}
