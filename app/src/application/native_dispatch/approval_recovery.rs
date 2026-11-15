//! Explicit Node continuation. Waiters own data, never task handles or reply capabilities.
#![allow(
    clippy::manual_let_else,
    clippy::needless_pass_by_ref_mut,
    clippy::option_if_let_else,
    clippy::significant_drop_tightening,
    clippy::too_many_lines,
    clippy::type_complexity,
    clippy::used_underscore_binding,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    Arc, ConfigurationPort, DispatchError, Instant, NativeApprovalPort, NativeApprovalRepository,
    NativeControlRepository, NativeCoordinator, NativeDispatchRepository, NativeResourceRepository,
    NativeRuntimeHandle, Ordering, OwnedNativeSource, PermissionPendingSnapshot, PreSourceProbe,
    SourceLaunchPort, TransportFact, TransportProgress,
    approval_inventory::ApprovalSlot,
    approval_response::ApprovalResponseOperation,
    approval_service::ApprovalDecisionOperation,
    environment::{PermissionResponseOwner, SourcePermissionSlot},
    runtime::SourceSlot,
    tasks::TaskSlot,
};
use crate::domain::{
    EntityId,
    approval_owner::{
        ApprovalId, ApprovalRejection, ApprovalReplyId, ApprovalStatus, CommitState,
        LeaseCloseState, SqlTaskJoinState, TerminalReplyObservation,
    },
};
use std::sync::{Mutex, OnceLock, Weak, atomic::AtomicBool};
use tokio::sync::Notify;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalRepairScope {
    Request(ApprovalId),
    SourceInstance(EntityId),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalRepairMode {
    RecheckOnly,
    RepairMissingFacts,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalReconcileProgress {
    Owned,
    WaitingForOriginalSql,
    Repairing,
    ReplyPending,
    Complete,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalRecoveryBlock {
    OriginalSqlNotJoined,
    CleanupUnconfirmed,
    CommitUnconfirmed,
    NecessaryStorageUnavailable,
    MissingOriginalProof,
    RequestInvalid,
    SafetyWon,
    SourceClosing,
    SourceClosed,
    NotZero,
    NotReliablyRecorded,
    AttemptAlreadyOwned,
    RetryConsumed,
    SerialExhausted,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApprovalReconcileOutcome {
    FactsConfirmed,
    FactsRepaired,
    ReplyStarted { response: ApprovalReplyId },
    StillBlocked { reason: ApprovalRecoveryBlock },
    OriginalUnavailable,
}
#[derive(Clone)]
pub struct ApprovalReconcileReport {
    pub outcome: ApprovalReconcileOutcome,
    pub approval: Option<ApprovalId>,
    pub operation: EntityId,
    pub status: Option<ApprovalStatus>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalReconcileWaitError {
    TimedOut,
    OwnerUnavailable,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RecoveryKey {
    Repair(ApprovalRepairScope, ApprovalRepairMode),
    Resume(ApprovalId),
    Zero(ApprovalId, ApprovalReplyId),
}
pub(super) struct ApprovalRecoveryOperation {
    pub(super) id: EntityId,
    pub(super) key: RecoveryKey,
    state: Mutex<(ApprovalReconcileProgress, Option<ApprovalReconcileReport>)>,
    changed: Notify,
    pub(super) job: OnceLock<Arc<TaskSlot>>,
}
impl ApprovalRecoveryOperation {
    fn new(id: EntityId, key: RecoveryKey) -> Self {
        Self {
            id,
            key,
            state: Mutex::new((ApprovalReconcileProgress::Owned, None)),
            changed: Notify::new(),
            job: OnceLock::new(),
        }
    }
    pub(super) fn advance(&self, value: ApprovalReconcileProgress) {
        if let Ok(mut state) = self.state.lock() {
            state.0 = value;
        }
        self.changed.notify_waiters();
    }
    pub(super) fn complete(&self, outcome: ApprovalReconcileOutcome, approval: Option<ApprovalId>) {
        if let Ok(mut state) = self.state.lock() {
            state.0 = ApprovalReconcileProgress::Complete;
            state.1 = Some(ApprovalReconcileReport {
                outcome,
                approval,
                operation: self.id,
                status: None,
            });
        }
        self.changed.notify_waiters();
    }
    pub(super) fn is_running(&self) -> Result<bool, ApprovalRejection> {
        Ok(self
            .state
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .1
            .is_none())
    }
}
pub struct ApprovalReconcileWaiter {
    pub(super) original: Arc<ApprovalRecoveryOperation>,
}
impl ApprovalReconcileWaiter {
    pub(crate) fn latest(&self) -> ApprovalReconcileProgress {
        self.original
            .state
            .lock()
            .map_or(ApprovalReconcileProgress::Complete, |state| state.0)
    }
    pub(crate) async fn wait(
        &mut self,
        until: Instant,
    ) -> Result<ApprovalReconcileReport, ApprovalReconcileWaitError> {
        loop {
            let changed = self.original.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let report = self
                .original
                .state
                .lock()
                .map_err(|_| ApprovalReconcileWaitError::OwnerUnavailable)?
                .1
                .clone();
            if let Some(report) = report {
                return Ok(report);
            }
            tokio::time::timeout_at(until, changed)
                .await
                .map_err(|_| ApprovalReconcileWaitError::TimedOut)?;
        }
    }
}
pub(super) struct ApprovalRetryNonce {
    consumed: AtomicBool,
}
pub struct ZeroApprovalRetryPermit<P: NativeApprovalPort> {
    permission: Weak<SourcePermissionSlot<P>>,
    approval: Weak<ApprovalSlot<P>>,
    source: Arc<PreSourceProbe>,
    decision: Arc<ApprovalDecisionOperation<P>>,
    previous: Arc<ApprovalResponseOperation<P>>,
    nonce: Arc<ApprovalRetryNonce>,
    next_serial: u64,
}
impl<P: NativeApprovalPort> Drop for ZeroApprovalRetryPermit<P> {
    fn drop(&mut self) {
        if self.nonce.consumed.load(Ordering::Acquire) {
            return;
        }
        if let Some(slot) = self.approval.upgrade()
            && let Ok(mut state) = slot.state.lock()
            && state
                .retry_nonce
                .as_ref()
                .is_some_and(|nonce| Arc::ptr_eq(nonce, &self.nonce))
        {
            state.retry_nonce = None;
        }
    }
}
pub struct ZeroApprovalRetryStartRejected<P: NativeApprovalPort> {
    pub error: ApprovalRejection,
    pub permit: ZeroApprovalRetryPermit<P>,
}
pub struct OriginalApprovalReconciler<P, L, G, N>
where
    P: NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + NativeApprovalRepository,
{
    pub(super) core: Weak<NativeCoordinator<P, L, G, N>>,
}
impl<P, L, G, N> OriginalApprovalReconciler<P, L, G, N>
where
    P: NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + NativeApprovalRepository,
{
    pub(super) fn from_native(native: &NativeRuntimeHandle<P, L, G, N>) -> Self {
        Self {
            core: Arc::downgrade(&native.core),
        }
    }
    pub(super) fn core(&self) -> Result<Arc<NativeCoordinator<P, L, G, N>>, ApprovalRejection> {
        self.core.upgrade().ok_or(ApprovalRejection::Stopped)
    }
    pub(super) fn locate(
        core: &NativeCoordinator<P, L, G, N>,
        id: ApprovalId,
    ) -> Result<
        (
            Arc<SourceSlot<P, L>>,
            Arc<SourcePermissionSlot<P>>,
            Arc<ApprovalSlot<P>>,
        ),
        ApprovalRejection,
    > {
        for source in core
            .sources
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .iter()
        {
            for permission in source
                .permissions
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
                .values()
            {
                if let Some(slot) = permission.approval.get()
                    && slot.request.metadata().approval == id
                {
                    return Ok((source.clone(), permission.clone(), slot.clone()));
                }
            }
        }
        Err(ApprovalRejection::InvalidRequest)
    }
    pub(super) fn live(
        core: &NativeCoordinator<P, L, G, N>,
        source: &SourceSlot<P, L>,
        permission: &SourcePermissionSlot<P>,
    ) -> Result<(), ApprovalRejection> {
        if source.pre.authority.closed.load(Ordering::SeqCst)
            || source.pre.authority.closing.load(Ordering::SeqCst)
        {
            return Err(ApprovalRejection::InvalidRequest);
        }
        if permission.association.as_ref().is_some_and(|pre| {
            pre.authority.closed.load(Ordering::SeqCst)
                || pre.authority.releasing.load(Ordering::SeqCst)
        }) {
            return Err(ApprovalRejection::InvalidRequest);
        }
        let connection = source
            .source
            .get()
            .ok_or(ApprovalRejection::InvalidRequest)?;
        if core
            .protocol
            .inspect_permission(connection, &permission.request)
            .map_err(|_| ApprovalRejection::InvalidRequest)?
            != PermissionPendingSnapshot::Waiting
        {
            return Err(ApprovalRejection::InvalidRequest);
        }
        Ok(())
    }
    pub(super) fn adopt(
        source: &SourceSlot<P, L>,
        key: RecoveryKey,
    ) -> Result<(Arc<ApprovalRecoveryOperation>, bool), ApprovalRejection> {
        let mut all = source
            .approval_recoveries
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        for original in all.iter() {
            if original.is_running()? {
                if original.key == key {
                    return Ok((original.clone(), false));
                }
                return Err(ApprovalRejection::Conflict);
            }
        }
        let id = NativeCoordinator::<P, L, G, N>::new_id()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        let original = Arc::new(ApprovalRecoveryOperation::new(id, key));
        all.push(original.clone());
        Ok((original, true))
    }
    pub(crate) fn prepare_zero_retry(
        &self,
        id: ApprovalId,
    ) -> Result<ZeroApprovalRetryPermit<P>, ApprovalRejection> {
        let core = self.core()?;
        let (source, permission, slot) = Self::locate(&core, id)?;
        Self::live(&core, &source, &permission)?;
        if source.pre.authority.required.load(Ordering::SeqCst) != 0 {
            return Err(ApprovalRejection::InvalidRequest);
        }
        let previous = slot
            .state
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?
            .responses
            .last()
            .cloned()
            .ok_or(ApprovalRejection::InvalidRequest)?;
        let probe = {
            let response = previous
                .state
                .lock()
                .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
            let arm = response
                .arm
                .as_ref()
                .ok_or(ApprovalRejection::NecessaryStorageUnavailable)?;
            let terminal = response
                .terminal
                .as_ref()
                .ok_or(ApprovalRejection::NecessaryStorageUnavailable)?;
            let saved_arm = response
                .arm_stored
                .as_ref()
                .ok_or(ApprovalRejection::NecessaryStorageUnavailable)?;
            let saved_terminal = response
                .terminal_stored
                .as_ref()
                .ok_or(ApprovalRejection::NecessaryStorageUnavailable)?;
            if saved_arm.record() != arm.metadata()
                || saved_terminal.record() != terminal.metadata()
                || !matches!(
                    terminal.metadata().observation,
                    TerminalReplyObservation::Zero { .. }
                )
                || saved_terminal.record_fingerprint() != terminal.fingerprint()
            {
                return Err(ApprovalRejection::Conflict);
            }
            for value in [
                arm.write_cache().confirmation(),
                terminal.write_cache().confirmation(),
            ] {
                let v = value.map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
                if v.commit != CommitState::Confirmed
                    || v.lease_close != LeaseCloseState::Confirmed
                    || v.task_join != SqlTaskJoinState::Joined
                {
                    return Err(ApprovalRejection::NecessaryStorageUnavailable);
                }
            }
            response
                .probe
                .clone()
                .ok_or(ApprovalRejection::InvalidRequest)?
        };
        // Probe/Core observation is outside every local Native lock. The original terminal is immutable;
        // atomic adoption below still rechecks the same latest original and all local closing fences.
        if !matches!(
            core.protocol.probe_progress(&probe),
            TransportProgress::Finished(TransportFact::Zero { .. })
        ) {
            return Err(ApprovalRejection::InvalidRequest);
        }
        let _gate = source
            .pre
            .authority
            .gate
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        let _action = permission
            .action
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        let mut state = slot
            .state
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        if source.pre.authority.closed.load(Ordering::SeqCst)
            || source.pre.authority.closing.load(Ordering::SeqCst)
            || source.pre.authority.required.load(Ordering::SeqCst) != 0
            || state.retry_nonce.is_some()
            || state.allow_disabled
            || !state.invalidations.is_empty()
            || !state
                .responses
                .last()
                .is_some_and(|v| Arc::ptr_eq(v, &previous))
        {
            return Err(ApprovalRejection::Conflict);
        }
        let decision = state
            .admission
            .clone()
            .filter(|op| op.saved.get().is_some())
            .ok_or(ApprovalRejection::NecessaryStorageUnavailable)?;
        let owner = permission
            .response_owner
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        if !matches!(owner.as_ref(),Some(PermissionResponseOwner::Selected(current)) if Arc::ptr_eq(current,&previous))
        {
            return Err(ApprovalRejection::InvalidRequest);
        }
        let next_serial = previous
            .serial
            .checked_add(1)
            .ok_or(ApprovalRejection::LengthUnrepresentable)?;
        if state.next_reply_serial != next_serial {
            return Err(ApprovalRejection::Conflict);
        }
        let nonce = Arc::new(ApprovalRetryNonce {
            consumed: AtomicBool::new(false),
        });
        state.retry_nonce = Some(nonce.clone());
        drop(owner);
        drop(state);
        drop(_action);
        drop(_gate);
        Ok(ZeroApprovalRetryPermit {
            permission: Arc::downgrade(&permission),
            approval: Arc::downgrade(&slot),
            source: source.pre.clone(),
            decision,
            previous,
            nonce,
            next_serial,
        })
    }
    pub(crate) fn start_zero_retry(
        &self,
        permit: ZeroApprovalRetryPermit<P>,
        write_deadline: Instant,
    ) -> Result<ApprovalReconcileWaiter, ZeroApprovalRetryStartRejected<P>> {
        match self.adopt_zero(&permit, write_deadline) {
            Ok(waiter) => Ok(waiter),
            Err(error) => Err(ZeroApprovalRetryStartRejected { error, permit }),
        }
    }
    fn adopt_zero(
        &self,
        permit: &ZeroApprovalRetryPermit<P>,
        until: Instant,
    ) -> Result<ApprovalReconcileWaiter, ApprovalRejection> {
        let core = self.core()?;
        let id = permit.previous.saved.approval();
        let (source, permission, slot) = Self::locate(&core, id)?;
        if until <= Instant::now()
            || !Arc::ptr_eq(&source.pre, &permit.source)
            || !permit
                .permission
                .upgrade()
                .is_some_and(|p| Arc::ptr_eq(&p, &permission))
            || !permit
                .approval
                .upgrade()
                .is_some_and(|p| Arc::ptr_eq(&p, &slot))
        {
            return Err(ApprovalRejection::InvalidRequest);
        }
        Self::live(&core, &source, &permission)?;
        let _gate = source
            .pre
            .authority
            .gate
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        let _action = permission
            .action
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        let mut state = slot
            .state
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        if source.pre.authority.closed.load(Ordering::SeqCst)
            || source.pre.authority.closing.load(Ordering::SeqCst)
            || source.pre.authority.required.load(Ordering::SeqCst) != 0
            || state.allow_disabled
            || !state.invalidations.is_empty()
        {
            return Err(ApprovalRejection::InvalidRequest);
        }
        let mut claimed = permission
            .state
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        let mut owner = permission
            .response_owner
            .lock()
            .map_err(|_| ApprovalRejection::NecessaryStorageUnavailable)?;
        if !state
            .retry_nonce
            .as_ref()
            .is_some_and(|n| Arc::ptr_eq(n, &permit.nonce))
            || state.next_reply_serial != permit.next_serial
            || !matches!(owner.as_ref(),Some(PermissionResponseOwner::Selected(current)) if Arc::ptr_eq(current,&permit.previous))
            || claimed.claimed != Some(permit.previous.id.entity())
            || permit.nonce.consumed.load(Ordering::Acquire)
        {
            return Err(ApprovalRejection::Conflict);
        }
        let (operation, new) = Self::adopt(&source, RecoveryKey::Zero(id, permit.previous.id))?;
        if !new {
            return Err(ApprovalRejection::Conflict);
        }
        // The original group owns the job before the one-use nonce is spent. Hold local action only;
        // the job's delivery must acquire it before any protocol reserve, so cannot race this commit.
        let task_core = core.clone();
        let task_source = source.clone();
        let task_permission = permission.clone();
        let task_slot = slot.clone();
        let decision = permit.decision.clone();
        let op = operation.clone();
        let failed = operation.clone();
        let failed_core = core;
        let failed_source = source.clone();
        let job = if let Ok(job) = source.jobs.spawn_guarded(
            async move {
                op.advance(ApprovalReconcileProgress::ReplyPending);
                let outcome = match task_core
                    .deliver_approval_with_deadline(
                        &task_source,
                        &task_permission,
                        &task_slot,
                        &decision,
                        until,
                    )
                    .await
                {
                    Ok(response) => ApprovalReconcileOutcome::ReplyStarted { response },
                    Err(_) => ApprovalReconcileOutcome::StillBlocked {
                        reason: ApprovalRecoveryBlock::NecessaryStorageUnavailable,
                    },
                };
                op.complete(outcome, Some(id));
            },
            move || {
                failed_core.fail_source(&failed_source, DispatchError::RequiredStateUnavailable);
                failed.complete(ApprovalReconcileOutcome::OriginalUnavailable, Some(id));
            },
        ) {
            job
        } else {
            operation.complete(ApprovalReconcileOutcome::OriginalUnavailable, Some(id));
            return Err(ApprovalRejection::Stopped);
        };
        let _ = operation.job.set(job);
        permit.nonce.consumed.store(true, Ordering::Release);
        state.retry_nonce = None;
        claimed.claimed = None;
        owner.take();
        Ok(ApprovalReconcileWaiter {
            original: operation,
        })
    }
}
