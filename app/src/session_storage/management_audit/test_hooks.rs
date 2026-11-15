//! Default-off controls on the original connection; no fabricated SQL/close success.
use super::{Error, SqliteManagementAudit};
use crate::{
    application::request_registry::{AuditMetadata, NodeClock},
    domain::{management_audit::AuditReceipt, request_registry::RegistryError},
    session_storage::registry_connection_owner::RegistryConnection,
    test_support::AsyncBarrier,
};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::Notify;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditPoint {
    AfterLease,
    AfterAcquire,
    AfterBegin,
    AfterPrune,
    AfterInsert,
    BeforeCommit,
    AfterCommitAck,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditSqlPoint {
    Begin,
    Prune,
    Insert,
    Commit,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditFault {
    FailBeforeCommit,
    LoseReplyAfterCommitAck,
}
#[derive(Default)]
struct State {
    point: Option<(AuditPoint, Arc<AsyncBarrier>)>,
    fault: Option<AuditFault>,
    vm: Option<(AuditSqlPoint, Arc<GateState>)>,
    veto: bool,
}
#[derive(Default)]
pub(in crate::session_storage) struct AuditHooks {
    state: Mutex<State>,
}
impl AuditHooks {
    pub(in crate::session_storage) async fn point(&self, point: AuditPoint) -> Result<(), Error> {
        let barrier = {
            let mut state = self.state.lock().map_err(|_| Error::Sql)?;
            if state.point.as_ref().is_some_and(|(at, _)| *at == point) {
                state.point.take().map(|(_, b)| b)
            } else {
                None
            }
        };
        if let Some(barrier) = barrier {
            barrier.pause().await;
        }
        Ok(())
    }
    fn take_fault(&self, fault: AuditFault) -> Result<bool, Error> {
        let mut state = self.state.lock().map_err(|_| Error::Sql)?;
        let taken = state.fault == Some(fault);
        if taken {
            state.fault = None;
        }
        drop(state);
        Ok(taken)
    }
    pub(super) fn fail_before_commit(&self) -> Result<(), Error> {
        if self.take_fault(AuditFault::FailBeforeCommit)? {
            Err(Error::Sql)
        } else {
            Ok(())
        }
    }
    pub(super) fn lose_reply(&self) -> Result<(), Error> {
        if self.take_fault(AuditFault::LoseReplyAfterCommitAck)? {
            Err(Error::CommitUnconfirmed)
        } else {
            Ok(())
        }
    }
    pub(super) async fn install(
        &self,
        connection: &mut RegistryConnection,
        point: AuditSqlPoint,
    ) -> Result<(), Error> {
        let (gate, veto) = {
            let mut state = self.state.lock().map_err(|_| Error::Sql)?;
            let gate = if state.vm.as_ref().is_some_and(|(at, _)| *at == point) {
                state.vm.take().map(|(_, g)| g)
            } else {
                None
            };
            let veto = point == AuditSqlPoint::Commit && state.veto;
            if veto {
                state.veto = false;
            }
            drop(state);
            (gate, veto)
        };
        if gate.is_some() || veto {
            let mut handle = connection
                .connection()
                .map_err(super::registry_error)?
                .lock_handle()
                .await
                .map_err(|_| Error::Sql)?;
            if let Some(gate) = gate {
                let mut first = true;
                handle.set_progress_handler(1, move || {
                    if std::mem::take(&mut first) {
                        gate.enter_worker()
                    } else {
                        true
                    }
                });
            }
            if veto {
                let mut first = true;
                handle.set_commit_hook(move || !std::mem::take(&mut first));
            }
        }
        Ok(())
    }
    pub(super) async fn remove(&self, connection: &mut RegistryConnection) -> Result<(), Error> {
        let mut handle = connection
            .connection()
            .map_err(super::registry_error)?
            .lock_handle()
            .await
            .map_err(|_| Error::Sql)?;
        handle.remove_progress_handler();
        handle.remove_commit_hook();
        Ok(())
    }
}

#[derive(Default)]
struct GateState {
    entered: AtomicBool,
    notify: Notify,
    decision: Mutex<Option<bool>>,
    released: Condvar,
}
impl GateState {
    fn enter_worker(&self) -> bool {
        self.entered.store(true, Ordering::Release);
        self.notify.notify_waiters();
        let Ok(mut decision) = self.decision.lock() else {
            return false;
        };
        loop {
            if let Some(value) = *decision {
                return value;
            }
            decision = match self.released.wait(decision) {
                Ok(d) => d,
                Err(_) => return false,
            };
        }
    }
    fn release(&self, value: bool) {
        let (mut decision, value) = match self.decision.lock() {
            Ok(d) => (d, value),
            Err(p) => (p.into_inner(), false),
        };
        if decision.is_none() {
            *decision = Some(value);
        }
        self.released.notify_all();
    }
}
pub struct VmGate {
    state: Arc<GateState>,
}
impl VmGate {
    pub(crate) async fn entered(&self) {
        loop {
            let notified = self.state.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.state.entered.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }
    pub(crate) fn release_continue(&self) {
        self.state.release(true);
    }
    pub(crate) fn release_interrupt(&self) {
        self.state.release(false);
    }
}
impl Drop for VmGate {
    fn drop(&mut self) {
        self.release_interrupt();
    }
}

pub struct AuditTestPort {
    audit: SqliteManagementAudit,
}
impl AuditTestPort {
    pub(super) const fn new(audit: SqliteManagementAudit) -> Self {
        Self { audit }
    }
    pub(crate) fn with_clock(&self, clock: Arc<dyn NodeClock>) -> SqliteManagementAudit {
        let mut audit = self.audit.clone();
        audit.clock = clock;
        audit
    }
    pub(crate) fn pause_next(&self, point: AuditPoint) -> Result<Arc<AsyncBarrier>, RegistryError> {
        let mut state = self
            .audit
            .hooks
            .state
            .lock()
            .map_err(|_| RegistryError::StorageUnavailable)?;
        if state.point.is_some() {
            return Err(RegistryError::Conflict);
        }
        let barrier = Arc::new(AsyncBarrier::default());
        state.point = Some((point, barrier.clone()));
        drop(state);
        Ok(barrier)
    }
    pub(crate) fn fault_next(&self, fault: AuditFault) -> Result<(), RegistryError> {
        let mut state = self
            .audit
            .hooks
            .state
            .lock()
            .map_err(|_| RegistryError::StorageUnavailable)?;
        if state.fault.is_some() {
            return Err(RegistryError::Conflict);
        }
        state.fault = Some(fault);
        drop(state);
        Ok(())
    }
    pub(crate) fn pause_vm_next(&self, point: AuditSqlPoint) -> Result<VmGate, RegistryError> {
        let mut state = self
            .audit
            .hooks
            .state
            .lock()
            .map_err(|_| RegistryError::StorageUnavailable)?;
        if state.vm.is_some() {
            return Err(RegistryError::Conflict);
        }
        let gate = Arc::new(GateState::default());
        state.vm = Some((point, gate.clone()));
        drop(state);
        Ok(VmGate { state: gate })
    }
    pub(crate) fn veto_commit_once(&self) -> Result<(), RegistryError> {
        let mut state = self
            .audit
            .hooks
            .state
            .lock()
            .map_err(|_| RegistryError::StorageUnavailable)?;
        if state.veto {
            return Err(RegistryError::Conflict);
        }
        state.veto = true;
        drop(state);
        Ok(())
    }
    pub(crate) async fn append_detailed(
        &self,
        metadata: &AuditMetadata,
    ) -> Result<AuditReceipt, Error> {
        self.audit.append_detailed(metadata).await
    }
}
