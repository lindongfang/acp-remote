//! Original SQL receipts remain owned after the caller and its wait disappear.
#![allow(
    clippy::option_if_let_else,
    clippy::redundant_pub_crate,
    clippy::significant_drop_tightening,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::super::tasks::TaskSlot;
use super::{
    Arc, BoxFuture, EntityId, RegistryDeadline, SourceReconcileProof, SourceSessionError,
    SourceSessionFailure, SourceSessionOperationId, SourceSqlObservation, SourceSqlReceipt,
    SourceSqlRole, SourceUpdate, SourceWriteProof,
};
#[cfg(test)]
use super::{SourceSqlOriginalSnapshot, SourceSqlTestFault, SourceTaskObservation};
use std::{
    collections::HashMap,
    sync::{
        Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::Notify;

pub type OriginalSourceTask = Arc<OnceLock<Weak<TaskSlot>>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceSqlScheduleState {
    Pending,
    Scheduled,
    NeverScheduled,
}
struct SourceSqlInvocationState {
    schedule: SourceSqlScheduleState,
    rejection: Option<SourceSessionError>,
}
pub struct SourceSqlInvocation {
    pub(crate) invocation_id: u64,
    pub(crate) operation_id: Option<SourceSessionOperationId>,
    pub(crate) role: SourceSqlRole,
    receipt: Arc<dyn SourceSqlReceipt>,
    task: OriginalSourceTask,
    state: Mutex<SourceSqlInvocationState>,
    pub(crate) changed: Arc<Notify>,
}
impl SourceSqlInvocation {
    pub(crate) fn schedule_state(&self) -> SourceSqlScheduleState {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .schedule
    }
    pub(crate) fn rejection(&self) -> Option<SourceSessionError> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .rejection
    }
    pub(crate) fn sql_observation(&self) -> Option<SourceSqlObservation> {
        self.receipt.observe()
    }
    pub(super) fn mark_scheduled(&self, slot: &Arc<TaskSlot>) {
        let _ = self.task.set(Arc::downgrade(slot));
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .schedule = SourceSqlScheduleState::Scheduled;
        self.changed.notify_waiters();
    }
    pub(super) fn mark_never_scheduled(&self, reason: SourceSessionError) {
        let _ = self.task.set(Weak::new());
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.schedule = SourceSqlScheduleState::NeverScheduled;
        state.rejection = Some(reason);
        drop(state);
        self.changed.notify_waiters();
    }
    #[cfg(test)]
    pub(crate) fn sql_started_for_test(&self) -> usize {
        self.receipt.sql_started_for_test()
    }
    #[cfg(test)]
    pub(crate) fn task_observation(&self) -> SourceTaskObservation {
        match self.task.get().and_then(Weak::upgrade) {
            Some(slot) => SourceTaskObservation {
                installed: true,
                joined: slot.confirmed_joined(),
                failed: slot.failed(),
            },
            None => SourceTaskObservation {
                installed: false,
                joined: false,
                failed: false,
            },
        }
    }
    #[cfg(test)]
    pub(crate) fn cleanup_for_test(&self) -> Option<SourceSqlOriginalSnapshot> {
        self.receipt.original_cleanup_for_test()
    }
    #[cfg(test)]
    pub(crate) async fn join_original_task(&self, deadline: tokio::time::Instant) -> bool {
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.schedule_state() != SourceSqlScheduleState::Pending {
                break;
            }
            notified.await;
        }
        let Some(slot) = self.task.get().and_then(Weak::upgrade) else {
            return false;
        };
        slot.join(deadline).await
    }
}
#[cfg(test)]
#[derive(Default)]
pub(crate) struct BlockingTestBarrier {
    entered: std::sync::atomic::AtomicBool,
    entered_notify: Notify,
    released: Mutex<bool>,
    condvar: std::sync::Condvar,
    thread_id: Mutex<Option<std::thread::ThreadId>>,
}
#[cfg(test)]
impl BlockingTestBarrier {
    pub(crate) fn pause_blocking(&self) {
        *self
            .thread_id
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(std::thread::current().id());
        self.entered.store(true, Ordering::Release);
        self.entered_notify.notify_waiters();
        let mut released = self
            .released
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while !*released {
            released = self
                .condvar
                .wait(released)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }
    pub(crate) async fn wait_until_entered(&self) {
        loop {
            let notified = self.entered_notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.entered.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }
    pub(crate) fn release(&self) {
        *self
            .released
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
        self.condvar.notify_all();
    }
    pub(crate) fn thread_id(&self) -> Option<std::thread::ThreadId> {
        *self
            .thread_id
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

pub struct SourceSqlEntry {
    pub(super) receipt: Arc<dyn SourceSqlReceipt>,
    pub(super) task: OriginalSourceTask,
    pub(super) update: Option<Arc<SourceWriteProof>>,
    source_required: bool,
}
impl SourceSqlEntry {
    pub(super) async fn confirm(&self, d: &RegistryDeadline) -> Result<(), SourceSessionFailure> {
        if let Some(original) = self.task.get() {
            let Some(task) = original.upgrade() else {
                return if self.receipt.observe().is_none() {
                    Ok(())
                } else {
                    Err(SourceSessionError::WorkerUnconfirmed.into())
                };
            };
            if !task.join(d.deadline()).await {
                return Err(SourceSessionError::WorkerUnconfirmed.into());
            }
            if task.failed() && self.receipt.observe().is_none() {
                return Err(SourceSessionError::WorkerUnconfirmed.into());
            }
        } else {
            return Err(SourceSessionError::WorkerUnconfirmed.into());
        }
        self.receipt.confirm_original(d).await?;
        Ok(())
    }
}
struct GroupState {
    source: Option<EntityId>,
    sealed: bool,
    metadata_closed: bool,
    entries: Vec<Arc<SourceSqlEntry>>,
}
pub struct SourceSqlGroup {
    pub(super) id: Option<SourceSessionOperationId>,
    pub(super) reconcile: tokio::sync::Mutex<()>,
    state: Mutex<GroupState>,
}
impl SourceSqlGroup {
    pub(super) fn metadata_closed(&self) -> Result<bool, SourceSessionFailure> {
        Ok(self
            .state
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
            .metadata_closed)
    }
    pub(super) fn freeze_metadata(&self) -> Result<(), SourceSessionFailure> {
        use crate::domain::native_catalog::CleanupKnowledge;
        let mut state = self
            .state
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?;
        if !state.sealed
            || state
                .entries
                .iter()
                .filter(|entry| entry.source_required)
                .any(|entry| {
                    entry.task.get().is_none()
                        || !entry
                            .task
                            .get()
                            .and_then(Weak::upgrade)
                            .is_none_or(|task| task.confirmed_joined())
                        || entry
                            .receipt
                            .observe()
                            .is_some_and(|sql| sql.cleanup != CleanupKnowledge::ConfirmedOriginal)
                })
        {
            return Err(SourceSessionError::SqlCloseUnconfirmed.into());
        }
        state.metadata_closed = true;
        Ok(())
    }
    pub(super) fn source_entries(&self) -> Result<Vec<Arc<SourceSqlEntry>>, SourceSessionFailure> {
        Ok(self
            .state
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
            .entries
            .iter()
            .filter(|entry| entry.source_required)
            .cloned()
            .collect())
    }
    pub(super) fn entries(&self) -> Result<Vec<Arc<SourceSqlEntry>>, SourceSessionFailure> {
        Ok(self
            .state
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
            .entries
            .clone())
    }
    pub(super) fn seal(&self) -> Result<(), SourceSessionFailure> {
        self.state
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
            .sealed = true;
        Ok(())
    }
}
#[derive(Default)]
pub struct SourceSqlInventory {
    groups: Mutex<HashMap<Option<SourceSessionOperationId>, Arc<SourceSqlGroup>>>,
    next_invocation: AtomicU64,
    #[cfg(test)]
    before: Mutex<Vec<(SourceSqlRole, crate::test_support::SharedAsyncBarrier)>>,
    #[cfg(test)]
    decode_hooks: Mutex<Vec<Arc<BlockingTestBarrier>>>,
    #[cfg(test)]
    retire_hooks: Mutex<Vec<Arc<BlockingTestBarrier>>>,
    #[cfg(test)]
    faults: Mutex<Vec<(SourceSqlRole, SourceSqlTestFault)>>,
    #[cfg(test)]
    reject_dispatch: Mutex<Vec<(SourceSqlRole, SourceSessionError)>>,
    #[cfg(test)]
    invocation_observers: Mutex<
        Vec<(
            SourceSqlRole,
            tokio::sync::oneshot::Sender<Arc<SourceSqlInvocation>>,
        )>,
    >,
}
impl SourceSqlInventory {
    pub(super) fn begin_invocation(
        &self,
        operation_id: Option<SourceSessionOperationId>,
        receipt: Arc<dyn SourceSqlReceipt>,
        task: OriginalSourceTask,
    ) -> Arc<SourceSqlInvocation> {
        let role = receipt.role();
        let invocation = Arc::new(SourceSqlInvocation {
            invocation_id: self.next_invocation.fetch_add(1, Ordering::Relaxed) + 1,
            operation_id,
            role,
            receipt,
            task,
            state: Mutex::new(SourceSqlInvocationState {
                schedule: SourceSqlScheduleState::Pending,
                rejection: None,
            }),
            changed: Arc::new(Notify::new()),
        });
        #[cfg(test)]
        {
            let observer = {
                let mut observers = self
                    .invocation_observers
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                observers
                    .iter()
                    .position(|(wanted, _)| *wanted == role)
                    .map(|index| observers.remove(index).1)
            };
            if let Some(observer) = observer {
                let _ = observer.send(invocation.clone());
            }
        }
        invocation
    }
    pub(super) fn bound_source(
        &self,
        id: SourceSessionOperationId,
    ) -> Result<Option<EntityId>, SourceSessionFailure> {
        Ok(self
            .group(Some(id))?
            .state
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
            .source)
    }
    #[cfg(test)]
    pub(crate) fn pause_next_for_test(
        &self,
        role: SourceSqlRole,
    ) -> Result<crate::test_support::SharedAsyncBarrier, SourceSessionFailure> {
        let barrier = Arc::new(crate::test_support::AsyncBarrier::default());
        self.before
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
            .push((role, barrier.clone()));
        Ok(barrier)
    }
    #[cfg(test)]
    pub(crate) async fn pause_before(&self, role: SourceSqlRole) {
        let barrier = {
            let mut hooks = self
                .before
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            hooks
                .iter()
                .position(|(wanted, _)| *wanted == role)
                .map(|index| hooks.remove(index).1)
        };
        if let Some(barrier) = barrier {
            barrier.pause().await;
        }
    }
    #[cfg(test)]
    pub(crate) fn observe_next_invocation_for_test(
        &self,
        role: SourceSqlRole,
    ) -> Result<tokio::sync::oneshot::Receiver<Arc<SourceSqlInvocation>>, SourceSessionFailure>
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.invocation_observers
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
            .push((role, tx));
        Ok(rx)
    }
    #[cfg(test)]
    pub(crate) fn reject_next_dispatch_for_test(
        &self,
        role: SourceSqlRole,
        error: SourceSessionError,
    ) -> Result<(), SourceSessionFailure> {
        self.reject_dispatch
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
            .push((role, error));
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn take_rejected_dispatch_for_test(
        &self,
        role: SourceSqlRole,
    ) -> Option<SourceSessionError> {
        let mut list = self
            .reject_dispatch
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        list.iter()
            .position(|(wanted, _)| *wanted == role)
            .map(|index| list.remove(index).1)
    }
    #[cfg(test)]
    pub(crate) fn arm_next_sql_fault_for_test(
        &self,
        role: SourceSqlRole,
        fault: SourceSqlTestFault,
    ) -> Result<(), SourceSessionFailure> {
        self.faults
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
            .push((role, fault));
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn pause_next_cpu_decode_for_test(
        &self,
    ) -> Result<Arc<BlockingTestBarrier>, SourceSessionFailure> {
        let barrier = Arc::new(BlockingTestBarrier::default());
        self.decode_hooks
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
            .push(barrier.clone());
        Ok(barrier)
    }
    #[cfg(test)]
    pub(crate) fn pause_next_cpu_retire_for_test(
        &self,
    ) -> Result<Arc<BlockingTestBarrier>, SourceSessionFailure> {
        let barrier = Arc::new(BlockingTestBarrier::default());
        self.retire_hooks
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
            .push(barrier.clone());
        Ok(barrier)
    }
    #[cfg(test)]
    pub(super) fn take_cpu_decode_barrier(&self) -> Option<Arc<BlockingTestBarrier>> {
        let mut hooks = self
            .decode_hooks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if hooks.is_empty() {
            None
        } else {
            Some(hooks.remove(0))
        }
    }
    #[cfg(test)]
    pub(super) fn take_cpu_retire_barrier(&self) -> Option<Arc<BlockingTestBarrier>> {
        let mut hooks = self
            .retire_hooks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if hooks.is_empty() {
            None
        } else {
            Some(hooks.remove(0))
        }
    }
    #[cfg(test)]
    pub(crate) fn observe_for_test(
        &self,
        id: SourceSessionOperationId,
    ) -> Result<Vec<(Option<SourceSqlObservation>, bool)>, SourceSessionFailure> {
        let group = self
            .groups
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
            .get(&Some(id))
            .cloned();
        let Some(group) = group else {
            return Ok(Vec::new());
        };
        Ok(group
            .entries()?
            .iter()
            .map(|entry| {
                (
                    entry.receipt.observe(),
                    entry
                        .task
                        .get()
                        .and_then(Weak::upgrade)
                        .is_some_and(|task| task.confirmed_joined()),
                )
            })
            .collect())
    }
    #[cfg(test)]
    pub(crate) fn cleanup_for_test(
        &self,
        id: SourceSessionOperationId,
    ) -> Result<Vec<Option<SourceSqlOriginalSnapshot>>, SourceSessionFailure> {
        let group = self
            .groups
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
            .get(&Some(id))
            .cloned();
        let Some(group) = group else {
            return Ok(Vec::new());
        };
        Ok(group
            .entries()?
            .iter()
            .map(|entry| entry.receipt.original_cleanup_for_test())
            .collect())
    }
    pub(super) fn group(
        &self,
        id: Option<SourceSessionOperationId>,
    ) -> Result<Arc<SourceSqlGroup>, SourceSessionFailure> {
        let mut groups = self
            .groups
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?;
        if let Some(group) = groups.get(&id) {
            return Ok(group.clone());
        }
        groups
            .try_reserve(1)
            .map_err(|_| SourceSessionError::Allocation)?;
        let group = Arc::new(SourceSqlGroup {
            id,
            reconcile: tokio::sync::Mutex::new(()),
            state: Mutex::new(GroupState {
                source: None,
                sealed: false,
                metadata_closed: false,
                entries: Vec::new(),
            }),
        });
        groups.insert(id, group.clone());
        Ok(group)
    }
    pub(super) fn remember(
        &self,
        id: Option<SourceSessionOperationId>,
        receipt: Arc<dyn SourceSqlReceipt>,
        task: OriginalSourceTask,
        update: Option<Arc<SourceWriteProof>>,
    ) -> Result<(), SourceSessionFailure> {
        self.insert(id, receipt, task, update, false)
    }
    pub(super) fn remember_repair(
        &self,
        id: SourceSessionOperationId,
        receipt: Arc<dyn SourceSqlReceipt>,
        task: OriginalSourceTask,
        original: Option<Arc<SourceWriteProof>>,
        permit: &SourceReconcileProof,
    ) -> Result<(), SourceSessionFailure> {
        if permit.id != id || !Arc::ptr_eq(&self.group(Some(id))?, &permit.original) {
            return Err(SourceSessionError::InvalidScope.into());
        }
        if let Some(original) = &original {
            if original.operation_id() != id
                || !original.access.scope.matches(&permit.scope)
                || !matches!(
                    original.update,
                    SourceUpdate::Transport { .. }
                        | SourceUpdate::Response { .. }
                        | SourceUpdate::Finish { .. }
                )
            {
                return Err(SourceSessionError::InvalidScope.into());
            }
        } else if receipt.role() != SourceSqlRole::Reconcile {
            return Err(SourceSessionError::InvalidScope.into());
        }
        self.insert(Some(id), receipt, task, original, true)
    }
    fn insert(
        &self,
        id: Option<SourceSessionOperationId>,
        receipt: Arc<dyn SourceSqlReceipt>,
        task: OriginalSourceTask,
        update: Option<Arc<SourceWriteProof>>,
        repair: bool,
    ) -> Result<(), SourceSessionFailure> {
        let group = self.group(id)?;
        let mut state = group
            .state
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?;
        let read = matches!(
            receipt.role(),
            SourceSqlRole::Query | SourceSqlRole::Reconcile
        );
        if (repair && state.metadata_closed) || (!repair && state.sealed && !read) {
            return Err(SourceSessionError::NodeStopping.into());
        }
        #[cfg(test)]
        {
            let mut faults = self
                .faults
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(index) = faults
                .iter()
                .position(|(wanted, _)| *wanted == receipt.role())
            {
                let fault = faults.remove(index).1;
                receipt.arm_fault_for_test(fault);
            }
        }
        let source_required = !state.sealed || repair;
        state
            .entries
            .try_reserve(1)
            .map_err(|_| SourceSessionError::Allocation)?;
        state.entries.push(Arc::new(SourceSqlEntry {
            receipt,
            task,
            update,
            source_required,
        }));
        Ok(())
    }
    pub(super) fn bind(
        &self,
        id: SourceSessionOperationId,
        source: EntityId,
    ) -> Result<(), SourceSessionFailure> {
        let group = self.group(Some(id))?;
        let mut state = group
            .state
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?;
        if state.sealed || state.source.is_some_and(|original| original != source) {
            return Err(SourceSessionError::Conflict.into());
        }
        state.source = Some(source);
        Ok(())
    }
    pub(super) fn source_groups(
        &self,
        source: EntityId,
    ) -> Result<Vec<Arc<SourceSqlGroup>>, SourceSessionFailure> {
        let groups = self
            .groups
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let mut selected = Vec::new();
        for group in groups {
            if group
                .state
                .lock()
                .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
                .source
                == Some(source)
            {
                selected.push(group);
            }
        }
        selected.sort_unstable_by_key(|group| group.id.map(SourceSessionOperationId::encode));
        Ok(selected)
    }
    pub(crate) fn source_stamp(
        &self,
        source: EntityId,
    ) -> Result<[u8; 32], super::super::DispatchError> {
        use crate::domain::native_catalog::CleanupKnowledge;
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        hash.update(b"source-original-sql-v2");
        let groups = self
            .source_groups(source)
            .map_err(|_| super::super::DispatchError::ResourceUnconfirmed)?;
        hash.update((groups.len() as u64).to_be_bytes());
        for group in groups {
            let state = group
                .state
                .lock()
                .map_err(|_| super::super::DispatchError::ResourceUnconfirmed)?;
            if !state.sealed || !state.metadata_closed {
                return Err(super::super::DispatchError::ResourceUnconfirmed);
            }
            if let Some(id) = group.id {
                hash.update(id.encode());
            }
            for entry in state.entries.iter().filter(|entry| entry.source_required) {
                if entry.task.get().is_none() {
                    return Err(super::super::DispatchError::ResourceUnconfirmed);
                }
                if !entry
                    .task
                    .get()
                    .and_then(Weak::upgrade)
                    .is_none_or(|task| task.confirmed_joined())
                {
                    return Err(super::super::DispatchError::ResourceUnconfirmed);
                }
                if let Some(observed) = entry.receipt.observe() {
                    if observed.cleanup != CleanupKnowledge::ConfirmedOriginal {
                        return Err(super::super::DispatchError::ResourceUnconfirmed);
                    }
                    hash.update([
                        observed.role as u8,
                        observed.transaction as u8,
                        observed.cleanup as u8,
                    ]);
                } else {
                    hash.update([0xff]);
                }
            }
        }
        Ok(hash.finalize().into())
    }
}

pub trait SourceSqlClosurePort: Send + Sync {
    fn reconcile_source(
        &self,
        source: EntityId,
    ) -> BoxFuture<'_, Result<(), super::super::DispatchError>>;
}
