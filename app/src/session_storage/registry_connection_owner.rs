//! Storage-scoped connection acquisition and closure. SQL business work stays in its adapter.
#![allow(
    clippy::too_many_lines,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
pub(super) use super::RegistryConnection;
use crate::{
    application::request_registry::RegistryOwnerPort, domain::request_registry::RegistryError,
};
use futures_util::FutureExt;
use sqlx::{Connection, SqliteConnection, SqlitePool};
use std::sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicBool, AtomicU8, Ordering},
};
use std::{future::Future, pin::Pin};
use tokio::{sync::Notify, task::JoinHandle};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionCloseStatus {
    Pending,
    Complete,
    Unconfirmed,
}
impl ConnectionCloseStatus {
    pub(crate) const fn is_complete(self) -> bool {
        matches!(self, Self::Complete)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CloseOutcome {
    Confirmed,
    Unconfirmed,
}
pub(super) struct CloseSlot {
    failed: Arc<AtomicBool>,
    outcome: AtomicU8,
    actual: Arc<AtomicU8>,
    joined: AtomicU8,
    changed: Notify,
    abort: tokio::task::AbortHandle,
    worker: Option<sqlx::sqlite::SqliteWorkerRegistration>,
    lifecycle: Option<std::sync::Weak<super::sqlite_lifecycle::SqliteOpenState>>,
}
impl CloseSlot {
    fn worker_status(&self) -> Option<bool> {
        match (&self.lifecycle, &self.worker) {
            (None, _) => Some(true),
            (Some(scope), Some(worker)) => scope.upgrade().and_then(|s| s.worker_status(worker)),
            _ => Some(false),
        }
    }
    async fn wait_worker(&self) -> bool {
        match (&self.lifecycle, &self.worker) {
            (None, _) => true,
            (Some(scope), Some(worker)) => {
                if let Some(scope) = scope.upgrade() {
                    scope.wait_worker(worker).await
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    async fn wait(&self) -> CloseOutcome {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.joined.load(Ordering::Acquire) != 0 {
                return match self.outcome.load(Ordering::Acquire) {
                    1 => CloseOutcome::Confirmed,
                    _ => CloseOutcome::Unconfirmed,
                };
            }
            changed.await;
        }
    }
    fn observe(&self, result: &Result<CloseOutcome, tokio::task::JoinError>) {
        self.outcome.store(
            if matches!(result, Ok(CloseOutcome::Confirmed)) {
                1
            } else {
                2
            },
            Ordering::Release,
        );
        self.joined
            .store(if result.is_ok() { 1 } else { 2 }, Ordering::Release);
        // An unconfirmed report is not evidence that the physical close or
        // worker failed. Only a JoinError loses the close task receipt itself.
        if result.is_err() {
            self.failed.store(true, Ordering::SeqCst);
        }
        self.changed.notify_waiters();
    }
    fn reap_joined(&self) -> bool {
        if self.worker_status() == Some(false) {
            self.failed.store(true, Ordering::SeqCst);
        }
        self.joined.load(Ordering::Acquire) != 0 && self.worker_status().is_some()
    }
}

#[derive(Clone)]
pub(super) struct OriginalSqlCloseReceipt {
    owner: Arc<ConnectionOwner>,
    slot: Arc<CloseSlot>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OriginalCleanupObservation {
    Pending,
    ConfirmedJoined,
    UnconfirmedJoined,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqlCloseKey(usize);
#[cfg(test)]
impl SqlCloseKey {
    pub(crate) const fn value(self) -> usize {
        self.0
    }
}
#[cfg(test)]
pub struct SqlCloseTestSnapshot {
    pub(crate) scope: Option<sqlx::sqlite::WorkerScopeId>,
    pub(crate) owner: SqlCloseKey,
    pub(crate) operation: SqlCloseKey,
    pub(crate) sql_actual: u8,
    pub(crate) sql_task_join: u8,
    pub(crate) sql_report: u8,
    pub(crate) worker: Option<sqlx::sqlite::WorkerSnapshot>,
    pub(crate) app_join: Option<bool>,
}
impl OriginalSqlCloseReceipt {
    #[cfg(test)]
    pub(super) fn belongs_to(&self, owner: &Arc<ConnectionOwner>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
    }

    #[cfg(test)]
    pub(super) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner) && Arc::ptr_eq(&self.slot, &other.slot)
    }

    #[cfg(test)]
    pub(crate) fn snapshot_for_test(&self) -> SqlCloseTestSnapshot {
        SqlCloseTestSnapshot {
            scope: self
                .slot
                .lifecycle
                .as_ref()
                .and_then(std::sync::Weak::upgrade)
                .and_then(|scope| scope.registry().snapshot().ok())
                .map(|snapshot| snapshot.scope),
            owner: SqlCloseKey(Arc::as_ptr(&self.owner) as usize),
            operation: SqlCloseKey(Arc::as_ptr(&self.slot) as usize),
            sql_actual: self.slot.actual.load(Ordering::Acquire),
            sql_task_join: self.slot.joined.load(Ordering::Acquire),
            sql_report: self.slot.outcome.load(Ordering::Acquire),
            worker: self.slot.worker.as_ref().and_then(|w| w.snapshot().ok()),
            app_join: self.slot.worker_status(),
        }
    }
    pub(super) async fn confirm_actual_cleanup(
        &self,
        until: tokio::time::Instant,
    ) -> OriginalCleanupObservation {
        if tokio::time::timeout_at(until, self.slot.wait())
            .await
            .is_err()
        {
            return OriginalCleanupObservation::Pending;
        }
        let Ok(worker) = tokio::time::timeout_at(until, self.slot.wait_worker()).await else {
            return OriginalCleanupObservation::Pending;
        };
        if self.slot.joined.load(Ordering::Acquire) == 1
            && self.slot.actual.load(Ordering::Acquire) == 1
            && worker
        {
            OriginalCleanupObservation::ConfirmedJoined
        } else {
            OriginalCleanupObservation::UnconfirmedJoined
        }
    }
}

impl crate::application::native_dispatch::ApprovalSqlCleanup for OriginalSqlCloseReceipt {
    fn original_owner_scope(&self) -> crate::application::request_registry::RegistryOwnerScope {
        crate::application::request_registry::RegistryOwnerScope::from_owner(self.owner.clone())
    }

    fn confirm_original_cleanup(
        &self,
        until: tokio::time::Instant,
    ) -> crate::application::native_dispatch::BoxFuture<
        '_,
        crate::application::native_dispatch::ApprovalCleanupObservation,
    > {
        Box::pin(async move {
            use crate::application::native_dispatch::ApprovalCleanupObservation as Observation;
            match self.confirm_actual_cleanup(until).await {
                OriginalCleanupObservation::Pending => Observation::Pending,
                OriginalCleanupObservation::ConfirmedJoined => Observation::ConfirmedJoined,
                OriginalCleanupObservation::UnconfirmedJoined => Observation::UnconfirmedJoined,
            }
        })
    }
}

pub(super) struct OriginalSqlCloseWaiter {
    receipt: OriginalSqlCloseReceipt,
}

impl OriginalSqlCloseWaiter {
    pub(super) fn receipt(&self) -> OriginalSqlCloseReceipt {
        self.receipt.clone()
    }

    pub(super) async fn wait_report(&self) -> CloseOutcome {
        self.receipt.slot.wait().await
    }
}

// Retain the original acquisition task even when its receiver or shutdown waiter is dropped.
struct AcquisitionSlot {
    #[cfg(test)]
    observation: Arc<super::AcquisitionObservation>,
    task: tokio::sync::Mutex<Option<JoinHandle<()>>>,
    failed: Arc<AtomicBool>,
}
impl AcquisitionSlot {
    async fn wait(&self) {
        let mut task = self.task.lock().await;
        if let Some(handle) = task.as_mut() {
            if handle.await.is_err() {
                self.failed.store(true, Ordering::SeqCst);
            }
            task.take();
        }
        drop(task);
        #[cfg(test)]
        self.observation.joined.store(true, Ordering::Release);
    }
    fn reap_joined(&self) -> bool {
        let Ok(mut task) = self.task.try_lock() else {
            return false;
        };
        let Some(handle) = task.as_mut() else {
            return true;
        };
        if !handle.is_finished() {
            return false;
        }
        let Some(outcome) = handle.now_or_never() else {
            return false;
        };
        if outcome.is_err() {
            self.failed.store(true, Ordering::SeqCst);
        }
        task.take();
        #[cfg(test)]
        self.observation.joined.store(true, Ordering::Release);
        true
    }
}

#[derive(Default)]
struct State {
    sealed: bool,
    leases: usize,
    slots: Vec<Arc<CloseSlot>>,
    acquisitions: Vec<Arc<AcquisitionSlot>>,
    idle_trust: Option<RegistryConnection>,
}
pub(super) struct ConnectionOwner {
    runtime: Option<tokio::runtime::Handle>,
    lifecycle: Option<std::sync::Weak<super::sqlite_lifecycle::SqliteOpenState>>,
    state: Mutex<State>,
    changed: Notify,
    failed: Arc<AtomicBool>,
    report_unconfirmed: Arc<AtomicBool>,
    #[cfg(test)]
    hooks: Arc<test_hooks::ConnectionCloseController>,
}
impl RegistryOwnerPort for ConnectionOwner {
    fn is_sealed(&self) -> bool {
        self.state.lock().map_or(true, |state| state.sealed)
    }

    fn wait_sealed(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            loop {
                let notified = self.changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if self.is_sealed() {
                    return;
                }
                notified.await;
            }
        })
    }
}

// Poison recovery here only retains cleanup responsibility; it never grants SQL or product work.
fn cleanup_state(state: &Mutex<State>) -> MutexGuard<'_, State> {
    match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
impl ConnectionOwner {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            runtime: tokio::runtime::Handle::try_current().ok(),
            lifecycle: None,
            state: Mutex::default(),
            changed: Notify::new(),
            failed: Arc::default(),
            report_unconfirmed: Arc::default(),
            #[cfg(test)]
            hooks: Arc::default(),
        })
    }
    pub(super) fn new_scoped(
        scope: std::sync::Weak<super::sqlite_lifecycle::SqliteOpenState>,
    ) -> Arc<Self> {
        Arc::new(Self {
            runtime: tokio::runtime::Handle::try_current().ok(),
            lifecycle: Some(scope),
            state: Mutex::default(),
            changed: Notify::new(),
            failed: Arc::default(),
            report_unconfirmed: Arc::default(),
            #[cfg(test)]
            hooks: Arc::default(),
        })
    }
    pub(super) fn lease(self: &Arc<Self>) -> Result<ConnectionLease, RegistryError> {
        let runtime = self
            .runtime
            .clone()
            .ok_or(RegistryError::StorageUnavailable)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| RegistryError::StorageUnavailable)?;
        state.slots.retain(|slot| !slot.reap_joined());
        if state.sealed {
            return Err(RegistryError::NodeStopping);
        }
        state.leases = state
            .leases
            .checked_add(1)
            .ok_or(RegistryError::StorageUnavailable)?;
        drop(state);
        Ok(ConnectionLease {
            owner: self.clone(),
            active: true,
            runtime,
        })
    }
    pub(super) async fn acquire(
        self: &Arc<Self>,
        pool: SqlitePool,
    ) -> Result<RegistryConnection, RegistryError> {
        self.acquire_with_lease(self.lease()?, pool, false).await
    }
    pub(super) async fn acquire_trust(
        self: &Arc<Self>,
        pool: SqlitePool,
    ) -> Result<RegistryConnection, RegistryError> {
        self.acquire_with_lease(self.lease()?, pool, true).await
    }
    // Called only by the registered acquisition task after its start gate.
    async fn acquire_original(
        self: &Arc<Self>,
        lease: ConnectionLease,
        pool: SqlitePool,
        trust_scope: bool,
        #[cfg(test)] observation: &super::AcquisitionObservation,
    ) -> Result<RegistryConnection, RegistryError> {
        let retained = if trust_scope {
            #[cfg(test)]
            self.hooks
                .pause(test_hooks::ClosePoint::BeforeTrustTake)
                .await;
            let mut state = self
                .state
                .lock()
                .map_err(|_| RegistryError::StorageUnavailable)?;
            if state.sealed {
                None
            } else {
                state.idle_trust.take()
            }
        } else {
            None
        };
        let mut connection = if let Some(connection) = retained {
            // Preserve the original physical lease and drop only the new temporary
            // acquisition lease outside the state fence.
            drop(lease);
            #[cfg(test)]
            self.hooks.pause(test_hooks::ClosePoint::AfterAcquire).await;
            connection
        } else {
            let acquired = pool.acquire();
            #[cfg(test)]
            let acquired = observation.observe_pool_acquire(acquired).await;
            #[cfg(not(test))]
            let acquired = acquired.await;
            let acquired = acquired.map_err(|error| {
                // SQLx can lose its original worker receipt on any acquire error.
                self.failed.store(true, Ordering::SeqCst);
                RegistryError::from(error)
            })?;
            #[cfg(test)]
            self.hooks.pause(test_hooks::ClosePoint::AfterAcquire).await;
            lease.attach(acquired.detach())
        };
        connection.trust_scope = trust_scope;
        #[cfg(test)]
        connection
            .physical_close
            .get_or_insert_with(|| Arc::new(super::PhysicalCloseLink::default()));
        Ok(connection)
    }
    async fn acquire_with_lease(
        self: &Arc<Self>,
        lease: ConnectionLease,
        pool: SqlitePool,
        trust_scope: bool,
    ) -> Result<RegistryConnection, RegistryError> {
        let runtime = lease.runtime.clone();
        let owner = self.clone();
        // A one-shot channel transfers the registry connection exactly once.
        // Its mutable receiver also proves that delivery has not happened yet:
        // if the caller is cancelled before the send, the sender observes the
        // closed channel and owns the connection for cleanup.
        let (send, receive) =
            tokio::sync::oneshot::channel::<Result<RegistryConnection, RegistryError>>();
        // Start gate prevents even the first acquire poll until its task is registered.
        let (start, started) = tokio::sync::oneshot::channel();
        #[cfg(test)]
        let observation = Arc::new(super::AcquisitionObservation::default());
        #[cfg(test)]
        let task_observation = observation.clone();
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| RegistryError::StorageUnavailable)?;
            if state.sealed {
                drop(state);
                return Err(RegistryError::NodeStopping);
            }
            state.acquisitions.retain(|slot| !slot.reap_joined());
            let task = runtime.spawn(async move {
                if started.await.is_err() {
                    return;
                }
                match owner
                    .acquire_original(
                        lease,
                        pool,
                        trust_scope,
                        #[cfg(test)]
                        &task_observation,
                    )
                    .await
                {
                    Ok(connection) => {
                        #[cfg(test)]
                        {
                            *task_observation
                                .physical_close
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                connection.physical_close.clone();
                            task_observation.transferred.store(true, Ordering::Release);
                        }
                        if owner.is_sealed() || send.is_closed() {
                            connection.close().await;
                        } else {
                            // `is_closed` has excluded receiver cancellation; the
                            // successful send transfers the connection exactly once.
                            let _ = send.send(Ok(connection));
                        }
                    }
                    Err(error) => {
                        let _ = send.send(Err(error));
                    }
                }
                owner.changed.notify_waiters();
            });
            state.acquisitions.push(Arc::new(AcquisitionSlot {
                #[cfg(test)]
                observation,
                task: tokio::sync::Mutex::new(Some(task)),
                failed: self.failed.clone(),
            }));
            drop(state);
            let _ = start.send(());
        }
        receive.await.map_err(|_| {
            if self.is_sealed() {
                RegistryError::NodeStopping
            } else {
                RegistryError::StorageUnavailable
            }
        })?
    }
    pub(super) async fn shutdown(&self) {
        self.seal();
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let (slots, acquisitions) = {
                let mut state = cleanup_state(&self.state);
                state.slots.retain(|slot| !slot.reap_joined());
                state.acquisitions.retain(|slot| !slot.reap_joined());
                if state.leases == 0 && state.slots.is_empty() && state.acquisitions.is_empty() {
                    return;
                }
                (state.slots.clone(), state.acquisitions.clone())
            };
            if slots.is_empty() && acquisitions.is_empty() {
                notified.await;
            } else {
                // Close tasks already run independently, releasing held resources while
                // acquisitions wait for permits. No task is aborted when this waiter drops.
                for slot in acquisitions {
                    slot.wait().await;
                }
                for slot in slots {
                    slot.wait().await;
                    if !slot.wait_worker().await {
                        self.failed.store(true, Ordering::SeqCst);
                    }
                }
            }
        }
    }
    pub(super) fn seal(&self) {
        let idle = {
            let mut state = cleanup_state(&self.state);
            state.sealed = true;
            state.idle_trust.take()
        };
        // Break the idle -> lease -> owner cycle and register its original close
        // outside the state lock. The active lease covers the handoff interval.
        drop(idle);
        self.changed.notify_waiters();
    }
    pub(super) fn status(&self) -> ConnectionCloseStatus {
        if self.state.is_poisoned() {
            return ConnectionCloseStatus::Unconfirmed;
        }
        let state = cleanup_state(&self.state);
        let pending = !state.sealed
            || state.leases != 0
            || !state.slots.is_empty()
            || !state.acquisitions.is_empty()
            || state.idle_trust.is_some();
        drop(state);
        if pending {
            ConnectionCloseStatus::Pending
        } else if self.failed.load(Ordering::SeqCst)
            || self.report_unconfirmed.load(Ordering::SeqCst)
        {
            ConnectionCloseStatus::Unconfirmed
        } else {
            ConnectionCloseStatus::Complete
        }
    }
    #[cfg(test)]
    pub(super) fn hooks(&self) -> Arc<test_hooks::ConnectionCloseController> {
        self.hooks.clone()
    }
}

pub(super) struct ConnectionLease {
    owner: Arc<ConnectionOwner>,
    active: bool,
    runtime: tokio::runtime::Handle,
}
impl ConnectionLease {
    pub(super) async fn acquire(
        self,
        pool: SqlitePool,
    ) -> Result<RegistryConnection, RegistryError> {
        let owner = self.owner.clone();
        owner.acquire_with_lease(self, pool, false).await
    }
    pub(super) const fn attach(self, connection: SqliteConnection) -> RegistryConnection {
        RegistryConnection {
            connection: Some(connection),
            lease: self,
            trust_scope: false,
            #[cfg(test)]
            physical_close: None,
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            approval_context: None,
        }
    }
    fn transfer_to_close_slot(active: &mut bool, state: &mut State, slot: Arc<CloseSlot>) {
        state.slots.push(slot);
        if *active {
            state.leases -= 1;
            *active = false;
        }
    }
    fn handoff(
        &mut self,
        connection: SqliteConnection,
        #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))] approval_context: Option<
            super::approval_owner::test_control::ApprovalConnectionContext,
        >,
    ) -> Arc<CloseSlot> {
        let worker = connection.worker_registration();
        let lifecycle = self.owner.lifecycle.clone();
        if lifecycle.is_some() && worker.is_none() {
            self.owner.failed.store(true, Ordering::SeqCst);
        }
        let mut state = cleanup_state(&self.owner.state);
        state.slots.retain(|slot| !slot.reap_joined());
        let owner = self.owner.clone();
        let failed = self.owner.failed.clone();
        let report_unconfirmed = self.owner.report_unconfirmed.clone();
        let actual = Arc::new(AtomicU8::new(0));
        let task_actual = actual.clone();
        // Registration and lease transfer share this fence, including a job that completes early.
        // A Send operation may be dropped on a thread with no ambient Tokio context.
        let task = self.runtime.spawn(async move {
            #[cfg(test)]
            owner
                .hooks
                .pause(test_hooks::ClosePoint::BeforeClose)
                .await;
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            let mut approval_report_failed = if let Some(context) = &approval_context {
                use super::approval_owner::test_control::{ApprovalCut, ApprovalCutState};
                context
                    .controller
                    .hit(
                        ApprovalCut::LeaseBeforeClose,
                        context.tag,
                        ApprovalCutState {
                            lease_attached: true,
                            begin_started: true,
                            commit: match (context.tag.kind, context.commit_confirmed) {
                                (
                                    super::approval_owner::test_control::ApprovalSqlOperation::Resolve,
                                    false,
                                ) => crate::domain::approval_owner::CommitState::NotStarted,
                                (_, true) => crate::domain::approval_owner::CommitState::Confirmed,
                                (_, false) => crate::domain::approval_owner::CommitState::Unknown,
                            },
                            actual_close_ack_observed: false,
                            source_job_registered: context.source_job_registered,
                        },
                    )
                    .await
                    .map_or(true, |effect| effect.fail_close_report)
            } else {
                false
            };
            let actual_confirmed = connection.close().await.is_ok();
            task_actual.store(
                if actual_confirmed { 1 } else { 2 },
                Ordering::Release,
            );
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            if actual_confirmed && let Some(context) = &approval_context {
                use super::approval_owner::test_control::{ApprovalCut, ApprovalCutState};
                approval_report_failed |= context
                    .controller
                    .hit(
                        ApprovalCut::LeaseAfterCloseAck,
                        context.tag,
                        ApprovalCutState {
                            lease_attached: true,
                            begin_started: true,
                            commit: match (context.tag.kind, context.commit_confirmed) {
                                (
                                    super::approval_owner::test_control::ApprovalSqlOperation::Resolve,
                                    false,
                                ) => crate::domain::approval_owner::CommitState::NotStarted,
                                (_, true) => crate::domain::approval_owner::CommitState::Confirmed,
                                (_, false) => crate::domain::approval_owner::CommitState::Unknown,
                            },
                            actual_close_ack_observed: true,
                            source_job_registered: context.source_job_registered,
                        },
                    )
                    .await
                    .map_or(true, |effect| effect.fail_close_report);
            }
            #[cfg(test)]
            if actual_confirmed {
                owner
                    .hooks
                    .pause(test_hooks::ClosePoint::AfterCloseAck)
                    .await;
            }
            #[cfg(test)]
            let report_confirmed = actual_confirmed && !owner.hooks.take_failure();
            #[cfg(not(test))]
            let report_confirmed = actual_confirmed;
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            let report_confirmed = report_confirmed && !approval_report_failed;
            let outcome = if report_confirmed {
                CloseOutcome::Confirmed
            } else {
                report_unconfirmed.store(true, Ordering::SeqCst);
                if !actual_confirmed {
                    owner.failed.store(true, Ordering::SeqCst);
                }
                CloseOutcome::Unconfirmed
            };
            owner.changed.notify_waiters();
            outcome
        });
        let slot = Arc::new(CloseSlot {
            failed,
            outcome: AtomicU8::new(0),
            actual,
            joined: AtomicU8::new(0),
            changed: Notify::new(),
            abort: task.abort_handle(),
            worker,
            lifecycle,
        });
        let observer = slot.clone();
        self.runtime.spawn(async move {
            let result = task.await;
            observer.observe(&result);
        });
        Self::transfer_to_close_slot(&mut self.active, &mut state, slot.clone());
        drop(state);
        self.owner.changed.notify_waiters();
        slot
    }
}
impl Drop for ConnectionLease {
    fn drop(&mut self) {
        if self.active {
            let mut state = cleanup_state(&self.owner.state);
            state.leases -= 1;
            drop(state);
            self.owner.changed.notify_waiters();
        }
    }
}

impl RegistryConnection {
    // Caller invokes this only after reliable SQL/row processing or COMMIT,
    // with no active transaction or queued rollback. Errors keep the full value.
    pub(crate) fn retain_for_trust(self) -> Result<(), Self> {
        if !self.trust_scope || self.connection.is_none() || !self.lease.active {
            return Err(self);
        }
        let owner = self.lease.owner.clone();
        let Ok(mut state) = owner.state.lock() else {
            return Err(self);
        };
        if state.sealed || state.idle_trust.is_some() {
            drop(state);
            return Err(self);
        }
        state.idle_trust = Some(self);
        drop(state);
        owner.changed.notify_waiters();
        Ok(())
    }
    #[cfg(test)]
    fn link_original_close(&self, slot: &Arc<CloseSlot>) {
        if let Some(physical) = &self.physical_close {
            *physical
                .close
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(slot.clone());
        }
    }
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    pub(super) fn attach_approval_test_context(
        &mut self,
        context: Option<super::approval_owner::test_control::ApprovalConnectionContext>,
    ) {
        self.approval_context = context.map(Box::new);
    }
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    pub(super) fn approval_commit_for_test(&mut self, confirmed: bool) {
        if let Some(context) = &mut self.approval_context {
            context.commit_confirmed = confirmed;
        }
    }
    pub(crate) fn connection(&mut self) -> Result<&mut SqliteConnection, RegistryError> {
        self.connection.as_mut().ok_or(RegistryError::CorruptState)
    }
    pub(super) fn begin_owned_close(mut self) -> Result<OriginalSqlCloseWaiter, RegistryError> {
        if let Some(connection) = self.connection.take() {
            let owner = self.lease.owner.clone();
            let slot = self.lease.handoff(
                connection,
                #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
                self.approval_context.take().map(|context| *context),
            );
            #[cfg(test)]
            self.link_original_close(&slot);
            Ok(OriginalSqlCloseWaiter {
                receipt: OriginalSqlCloseReceipt { owner, slot },
            })
        } else {
            self.lease.owner.failed.store(true, Ordering::SeqCst);
            Err(RegistryError::CorruptState)
        }
    }

    pub(super) async fn close(self) -> CloseOutcome {
        match self.begin_owned_close() {
            Ok(waiter) => waiter.wait_report().await,
            Err(_) => CloseOutcome::Unconfirmed,
        }
    }
}
impl Drop for RegistryConnection {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            let slot = self.lease.handoff(
                connection,
                #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
                self.approval_context.take().map(|context| *context),
            );
            #[cfg(test)]
            self.link_original_close(&slot);
            #[cfg(not(test))]
            drop(slot);
        }
    }
}

#[cfg(test)]
mod acquire_observation;

#[cfg(test)]
pub mod test_hooks {
    use crate::domain::request_registry::RegistryError;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };
    use tokio::sync::Semaphore;

    #[derive(Clone, Copy, Eq, PartialEq)]
    pub enum ClosePoint {
        BeforeClose,
        AfterCloseAck,
        AfterAcquire,
        BeforeTrustTake,
    }
    pub struct CloseBarrier {
        arrived: Semaphore,
        released: Semaphore,
    }
    impl CloseBarrier {
        pub(crate) fn new() -> Arc<Self> {
            Arc::new(Self {
                arrived: Semaphore::new(0),
                released: Semaphore::new(0),
            })
        }
        pub(crate) async fn arrived(&self) {
            if let Ok(permit) = self.arrived.acquire().await {
                permit.forget();
            }
        }
        pub(crate) fn release(&self) {
            self.released.add_permits(1);
        }
        async fn pause(&self) {
            self.arrived.add_permits(1);
            if let Ok(permit) = self.released.acquire().await {
                permit.forget();
            }
        }
    }
    #[derive(Default)]
    pub struct ConnectionCloseController {
        next: Mutex<Option<(ClosePoint, Arc<CloseBarrier>)>>,
        fail: AtomicBool,
    }
    impl ConnectionCloseController {
        pub(crate) fn pause_next(
            &self,
            point: ClosePoint,
            barrier: Arc<CloseBarrier>,
        ) -> Result<(), RegistryError> {
            let mut next = self
                .next
                .lock()
                .map_err(|_| RegistryError::StorageUnavailable)?;
            if next.is_some() {
                return Err(RegistryError::Conflict);
            }
            *next = Some((point, barrier));
            drop(next);
            Ok(())
        }
        pub(crate) fn fail_next_result_after_ack(&self) -> Result<(), RegistryError> {
            if self.fail.swap(true, Ordering::SeqCst) {
                return Err(RegistryError::Conflict);
            }
            Ok(())
        }
        pub(super) async fn pause(&self, point: ClosePoint) -> bool {
            let barrier = {
                let mut next = self
                    .next
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if next.as_ref().is_some_and(|(at, _)| *at == point) {
                    next.take().map(|(_, b)| b)
                } else {
                    None
                }
            };
            if let Some(barrier) = barrier {
                barrier.pause().await;
            }
            false
        }
        pub(super) fn take_failure(&self) -> bool {
            self.fail.swap(false, Ordering::SeqCst)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn poisoned_retain_returns_original_and_closes_outside_state_fence() {
        let owner = ConnectionOwner::new();
        let raw = SqliteConnection::connect(":memory:")
            .await
            .expect("original worker");
        let mut original = owner.lease().expect("lease").attach(raw);
        original.trust_scope = true;
        let poisoned = owner.clone();
        assert!(
            std::thread::spawn(move || {
                let _state = poisoned.state.lock().expect("fixture state");
                panic!("intentional owned poison fixture");
            })
            .join()
            .is_err()
        );
        let returned = original
            .retain_for_trust()
            .expect_err("poison returns full original");
        assert_eq!(cleanup_state(&owner.state).leases, 1);
        let waiter = returned.begin_owned_close().expect("same original handoff");
        assert_eq!(waiter.wait_report().await, CloseOutcome::Confirmed);
        owner.shutdown().await;
        assert_eq!(owner.status(), ConnectionCloseStatus::Unconfirmed);
    }
    #[test]
    fn registry_connection_owner_without_runtime_rejects_before_acquire() {
        let owner = std::thread::spawn(ConnectionOwner::new).join().unwrap();
        assert!(matches!(
            owner.lease(),
            Err(RegistryError::StorageUnavailable)
        ));
        assert_eq!(cleanup_state(&owner.state).leases, 0);
        assert!(cleanup_state(&owner.state).slots.is_empty());
    }
}

#[cfg(test)]
mod original_close_receipt_tests;

#[cfg(all(test, windows))]
#[path = "../../tests/registration_acceptance/close_owner/private_slot_acceptance.rs"]
mod independent_private_close;

#[cfg(all(test, windows))]
#[path = "../../tests/request_authority_development/slot_observation.rs"]
pub mod authority_slot_observation;
