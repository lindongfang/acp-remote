//! Opt-in observation and real-statement fault seams over the original registry/close slots.
#![allow(
    dead_code,
    clippy::struct_excessive_bools,
    clippy::significant_drop_tightening,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    reason = "Test-only approval fault and cut controller scaffolding"
)]
use crate::{
    application::native_dispatch::owned_job_registered_for_approval_test,
    domain::{EntityId, approval_owner::*},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tokio::{sync::Notify, time::Instant};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalCut {
    RequestBeforeBegin,
    RegisterBeforeBegin,
    DecideBeforeBegin,
    DecideAfterCommit,
    ReplyArmBeforeBegin,
    ReplyTerminalBeforeBegin,
    InvalidationBeforeBegin,
    ReconcileBeforeBegin,
    SourceApprovalManifestBeforeBegin,
    BeforeSelectedReserve,
    BeforeSelectedEnqueue,
    LeaseBeforeClose,
    LeaseAfterCloseAck,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalSqlOperation {
    Resolve,
    Observe,
    Register,
    Decide,
    Arm,
    Terminal,
    Invalidate,
    Recheck,
    Repair,
    SourceClose,
    QueueAudit,
    ApprovalAudit,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApprovalOperationTag {
    pub operation: EntityId,
    pub kind: ApprovalSqlOperation,
    pub instance: Option<EntityId>,
    pub approval: Option<ApprovalId>,
    pub registration: Option<ApprovalRegistrationId>,
    pub response: Option<ApprovalReplyId>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalCutSelector {
    Next,
    Operation(EntityId),
    Approval(ApprovalId),
    ApprovalOperation(ApprovalId, ApprovalSqlOperation),
    Registration(ApprovalRegistrationId),
    Response(ApprovalReplyId),
    SourceInstance(EntityId),
}
impl ApprovalCutSelector {
    fn matches(self, tag: &ApprovalOperationTag) -> bool {
        match self {
            Self::Next => true,
            Self::Operation(id) => tag.operation == id,
            Self::Approval(id) => tag.approval == Some(id),
            Self::ApprovalOperation(approval, operation) => {
                tag.approval == Some(approval) && tag.kind == operation
            }
            Self::Registration(id) => tag.registration == Some(id),
            Self::Response(id) => tag.response == Some(id),
            Self::SourceInstance(id) => tag.instance == Some(id),
        }
    }
    #[allow(
        clippy::match_same_arms,
        reason = "Distinct tuple payload types per variant"
    )]
    fn overlaps(self, other: Self) -> bool {
        match (self, other) {
            (Self::Operation(a), Self::Operation(b)) => a == b,
            (Self::Approval(a), Self::Approval(b)) => a == b,
            (Self::ApprovalOperation(a, _), Self::ApprovalOperation(b, _)) => a == b,
            (Self::ApprovalOperation(approval, _), Self::Approval(other))
            | (Self::Approval(other), Self::ApprovalOperation(approval, _)) => approval == other,
            (Self::Registration(a), Self::Registration(b)) => a == b,
            (Self::Response(a), Self::Response(b)) => a == b,
            (Self::SourceInstance(a), Self::SourceInstance(b)) => a == b,
            _ => true,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApprovalArmId(u64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalControlError {
    Occupied,
    Poisoned,
    InvalidPoint,
    InvalidGuard,
    Stopped,
    Exhausted,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalWaitError {
    TimedOut,
    Disarmed,
    Stopped,
}
#[derive(Clone, Debug)]
pub struct ApprovalCutArrival {
    pub event_sequence: u64,
    pub arm: ApprovalArmId,
    pub point: ApprovalCut,
    pub tag: ApprovalOperationTag,
    pub source_job_registered: bool,
    pub lease_attached: bool,
    pub begin_started: bool,
    pub commit: CommitState,
    pub actual_close_ack_observed: bool,
}
#[derive(Clone, Debug)]
pub struct ApprovalCutEvent {
    pub sequence: u64,
    pub arrival: ApprovalCutArrival,
}
#[derive(Clone, Copy)]
pub(in crate::session_storage) struct ApprovalCutState {
    pub lease_attached: bool,
    pub begin_started: bool,
    pub commit: CommitState,
    pub actual_close_ack_observed: bool,
    pub source_job_registered: bool,
}
impl ApprovalCutState {
    pub(in crate::session_storage) fn before_begin() -> Self {
        Self {
            lease_attached: false,
            begin_started: false,
            commit: CommitState::NotStarted,
            actual_close_ack_observed: false,
            source_job_registered: owned_job_registered_for_approval_test(),
        }
    }
}
#[derive(Default)]
pub(in crate::session_storage) struct ApprovalCutEffect {
    pub lose_commit_report: bool,
    pub fail_close_report: bool,
}
struct PauseState {
    arrival: Option<ApprovalCutArrival>,
    released: bool,
    disarmed: bool,
    stopped: bool,
    lose_commit_report: bool,
    fail_close_report: bool,
}
struct Pause {
    id: ApprovalArmId,
    point: ApprovalCut,
    selector: ApprovalCutSelector,
    state: Mutex<PauseState>,
    changed: Notify,
}
pub struct ApprovalPauseGuard {
    id: ApprovalArmId,
    controller: Arc<ApprovalTestController>,
    original: Arc<Pause>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalStatement {
    InsertRequest,
    InsertAssociation,
    InsertRegistration,
    InsertDecision,
    AcceptCommand,
    InsertArm,
    InsertTerminal,
    InsertInvalidation,
    AuditDeleteQueue,
    AuditDeleteApproval,
    AuditCountQueue,
    AuditCountApproval,
    AuditInsertQueue,
    AuditInsertApproval,
    Commit,
}
#[derive(Clone, Debug)]
pub struct ApprovalFaultObservation {
    pub tag: ApprovalOperationTag,
    pub point: ApprovalStatement,
    pub sqlite_primary_code: Option<i32>,
    pub sqlite_extended_code: Option<i32>,
    pub commit_attempted: bool,
    pub actual_sql_error: bool,
}
struct Fault {
    id: ApprovalArmId,
    point: ApprovalStatement,
    selector: ApprovalCutSelector,
    deferred: bool,
    taken: AtomicBool,
    disarmed: AtomicBool,
    observation: Mutex<Option<ApprovalFaultObservation>>,
    changed: Notify,
}
pub struct ApprovalFaultGuard {
    original: Arc<Fault>,
}
pub(in crate::session_storage) struct ApprovalStatementFault {
    original: Arc<Fault>,
    tag: ApprovalOperationTag,
}
struct State {
    next: u64,
    sequence: u64,
    pauses: Vec<Arc<Pause>>,
    faults: Vec<Arc<Fault>>,
    events: Vec<ApprovalCutEvent>,
    stopped: bool,
}
pub struct ApprovalTestController {
    state: Mutex<State>,
}
impl ApprovalTestController {
    pub(in crate::session_storage) const fn new_registry_controller() -> Self {
        Self {
            state: Mutex::new(State {
                next: 0,
                sequence: 0,
                pauses: Vec::new(),
                faults: Vec::new(),
                events: Vec::new(),
                stopped: false,
            }),
        }
    }
}
impl ApprovalTestController {
    pub fn pause_next(
        self: &Arc<Self>,
        point: ApprovalCut,
        selector: ApprovalCutSelector,
    ) -> Result<ApprovalPauseGuard, ApprovalControlError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ApprovalControlError::Poisoned)?;
        if state.stopped {
            return Err(ApprovalControlError::Stopped);
        }
        for arm in &state.pauses {
            let pending = arm
                .state
                .lock()
                .map_err(|_| ApprovalControlError::Poisoned)?;
            if arm.point == point
                && !pending.disarmed
                && (pending.arrival.is_none() || !pending.released)
                && arm.selector.overlaps(selector)
            {
                return Err(ApprovalControlError::Occupied);
            }
        }
        state.next = state
            .next
            .checked_add(1)
            .ok_or(ApprovalControlError::Exhausted)?;
        let original = Arc::new(Pause {
            id: ApprovalArmId(state.next),
            point,
            selector,
            state: Mutex::new(PauseState {
                arrival: None,
                released: false,
                disarmed: false,
                stopped: false,
                lose_commit_report: false,
                fail_close_report: false,
            }),
            changed: Notify::new(),
        });
        state.pauses.push(original.clone());
        Ok(ApprovalPauseGuard {
            id: original.id,
            controller: self.clone(),
            original,
        })
    }
    pub fn fail_result_after_ack(
        &self,
        guard: &ApprovalPauseGuard,
    ) -> Result<(), ApprovalControlError> {
        self.report_effect(guard, false)
    }
    pub fn lose_commit_report(
        &self,
        guard: &ApprovalPauseGuard,
    ) -> Result<(), ApprovalControlError> {
        self.report_effect(guard, true)
    }
    fn report_effect(
        &self,
        guard: &ApprovalPauseGuard,
        commit: bool,
    ) -> Result<(), ApprovalControlError> {
        if !std::ptr::eq(self, &raw const *guard.controller) {
            return Err(ApprovalControlError::InvalidGuard);
        }
        let mut state = guard
            .original
            .state
            .lock()
            .map_err(|_| ApprovalControlError::Poisoned)?;
        let arrival = state
            .arrival
            .as_ref()
            .ok_or(ApprovalControlError::InvalidGuard)?;
        let valid = if commit {
            arrival.point == ApprovalCut::DecideAfterCommit
                && arrival.commit == CommitState::Confirmed
        } else {
            arrival.point == ApprovalCut::LeaseAfterCloseAck && arrival.actual_close_ack_observed
        };
        if !valid
            || state.released
            || state.stopped
            || state.lose_commit_report
            || state.fail_close_report
        {
            return Err(ApprovalControlError::InvalidGuard);
        }
        if commit {
            state.lose_commit_report = true;
        } else {
            state.fail_close_report = true;
        }
        Ok(())
    }
    pub(in crate::session_storage) async fn hit(
        &self,
        point: ApprovalCut,
        tag: ApprovalOperationTag,
        facts: ApprovalCutState,
    ) -> Result<ApprovalCutEffect, ApprovalControlError> {
        let original = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| ApprovalControlError::Poisoned)?;
            if state.stopped {
                return Err(ApprovalControlError::Stopped);
            }
            let mut found = None;
            for arm in &state.pauses {
                let phase = arm
                    .state
                    .lock()
                    .map_err(|_| ApprovalControlError::Poisoned)?;
                if arm.point == point
                    && arm.selector.matches(&tag)
                    && phase.arrival.is_none()
                    && !phase.disarmed
                    && !phase.stopped
                {
                    found = Some(arm.clone());
                    break;
                }
            }
            let Some(arm) = found else {
                return Ok(ApprovalCutEffect::default());
            };
            state.sequence = state
                .sequence
                .checked_add(1)
                .ok_or(ApprovalControlError::Exhausted)?;
            let arrival = ApprovalCutArrival {
                event_sequence: state.sequence,
                arm: arm.id,
                point,
                tag,
                source_job_registered: facts.source_job_registered,
                lease_attached: facts.lease_attached,
                begin_started: facts.begin_started,
                commit: facts.commit,
                actual_close_ack_observed: facts.actual_close_ack_observed,
            };
            arm.state
                .lock()
                .map_err(|_| ApprovalControlError::Poisoned)?
                .arrival = Some(arrival.clone());
            state.events.push(ApprovalCutEvent {
                sequence: arrival.event_sequence,
                arrival,
            });
            arm.changed.notify_waiters();
            arm
        };
        loop {
            let changed = original.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let state = original
                    .state
                    .lock()
                    .map_err(|_| ApprovalControlError::Poisoned)?;
                if state.stopped {
                    return Err(ApprovalControlError::Stopped);
                }
                if state.released {
                    return Ok(ApprovalCutEffect {
                        lose_commit_report: state.lose_commit_report,
                        fail_close_report: state.fail_close_report,
                    });
                }
                if state.disarmed {
                    return Ok(ApprovalCutEffect::default());
                }
            }
            changed.await;
        }
    }
    pub fn events_after(&self, sequence: u64) -> Vec<ApprovalCutEvent> {
        self.state.lock().map_or_else(
            |_| Vec::new(),
            |state| {
                state
                    .events
                    .iter()
                    .filter(|event| event.sequence > sequence)
                    .cloned()
                    .collect()
            },
        )
    }
    pub(in crate::session_storage) fn stop_source(&self, instance: EntityId) {
        if let Ok(state) = self.state.lock() {
            for arm in &state.pauses {
                if let Ok(mut phase) = arm.state.lock()
                    && phase
                        .arrival
                        .as_ref()
                        .is_some_and(|arrival| arrival.tag.instance == Some(instance))
                {
                    phase.stopped = true;
                    arm.changed.notify_waiters();
                }
            }
        }
    }
    pub(in crate::session_storage) fn stop(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.stopped = true;
            for arm in &state.pauses {
                if let Ok(mut phase) = arm.state.lock() {
                    phase.stopped = true;
                    arm.changed.notify_waiters();
                }
            }
            for fault in &state.faults {
                fault.disarmed.store(true, Ordering::Release);
                fault.changed.notify_waiters();
            }
        }
    }
    fn arm_fault(
        &self,
        point: ApprovalStatement,
        selector: ApprovalCutSelector,
        deferred: bool,
    ) -> Result<ApprovalFaultGuard, ApprovalControlError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ApprovalControlError::Poisoned)?;
        if state.stopped {
            return Err(ApprovalControlError::Stopped);
        }
        if state.faults.iter().any(|arm| {
            arm.point == point
                && !arm.taken.load(Ordering::Acquire)
                && !arm.disarmed.load(Ordering::Acquire)
                && arm.selector.overlaps(selector)
        }) {
            return Err(ApprovalControlError::Occupied);
        }
        state.next = state
            .next
            .checked_add(1)
            .ok_or(ApprovalControlError::Exhausted)?;
        let original = Arc::new(Fault {
            id: ApprovalArmId(state.next),
            point,
            selector,
            deferred,
            taken: AtomicBool::new(false),
            disarmed: AtomicBool::new(false),
            observation: Mutex::new(None),
            changed: Notify::new(),
        });
        state.faults.push(original.clone());
        Ok(ApprovalFaultGuard { original })
    }
    pub fn abort_next_statement(
        &self,
        point: ApprovalStatement,
        selector: ApprovalCutSelector,
    ) -> Result<ApprovalFaultGuard, ApprovalControlError> {
        self.arm_fault(point, selector, false)
    }
    pub fn deferred_foreign_key_at_commit(
        &self,
        selector: ApprovalCutSelector,
    ) -> Result<ApprovalFaultGuard, ApprovalControlError> {
        self.arm_fault(ApprovalStatement::Commit, selector, true)
    }
    pub(in crate::session_storage) fn take_statement_fault(
        &self,
        point: ApprovalStatement,
        tag: ApprovalOperationTag,
    ) -> Result<Option<ApprovalStatementFault>, ApprovalControlError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ApprovalControlError::Poisoned)?;
        if state.stopped {
            return Err(ApprovalControlError::Stopped);
        }
        for arm in &state.faults {
            if arm.point == point
                && arm.selector.matches(&tag)
                && !arm.disarmed.load(Ordering::Acquire)
                && !arm.taken.swap(true, Ordering::AcqRel)
            {
                return Ok(Some(ApprovalStatementFault {
                    original: arm.clone(),
                    tag,
                }));
            }
        }
        Ok(None)
    }
}
impl ApprovalPauseGuard {
    pub const fn id(&self) -> ApprovalArmId {
        self.id
    }
    pub async fn wait_until_arrived(
        &self,
        until: Instant,
    ) -> Result<ApprovalCutArrival, ApprovalWaitError> {
        loop {
            let changed = self.original.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let state = self
                    .original
                    .state
                    .lock()
                    .map_err(|_| ApprovalWaitError::Stopped)?;
                if let Some(arrival) = &state.arrival {
                    return Ok(arrival.clone());
                }
                if state.stopped {
                    return Err(ApprovalWaitError::Stopped);
                }
                if state.disarmed {
                    return Err(ApprovalWaitError::Disarmed);
                }
            }
            tokio::time::timeout_at(until, changed)
                .await
                .map_err(|_| ApprovalWaitError::TimedOut)?;
        }
    }
    pub fn release(&self) {
        if let Ok(mut state) = self.original.state.lock() {
            state.released = true;
        }
        self.original.changed.notify_waiters();
    }
}
impl Drop for ApprovalPauseGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = self.original.state.lock() {
            if state.arrival.is_none() {
                state.disarmed = true;
            } else {
                state.released = true;
            }
        }
        self.original.changed.notify_waiters();
    }
}
impl ApprovalFaultGuard {
    pub fn id(&self) -> ApprovalArmId {
        self.original.id
    }
    pub async fn wait_until_fired(
        &self,
        until: Instant,
    ) -> Result<ApprovalFaultObservation, ApprovalWaitError> {
        loop {
            let changed = self.original.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let observation = self
                .original
                .observation
                .lock()
                .map_err(|_| ApprovalWaitError::Stopped)?
                .clone();
            if let Some(observation) = observation {
                return Ok(observation);
            }
            if self.original.disarmed.load(Ordering::Acquire) {
                return Err(ApprovalWaitError::Disarmed);
            }
            tokio::time::timeout_at(until, changed)
                .await
                .map_err(|_| ApprovalWaitError::TimedOut)?;
        }
    }
}
impl Drop for ApprovalFaultGuard {
    fn drop(&mut self) {
        if !self.original.taken.load(Ordering::Acquire) {
            self.original.disarmed.store(true, Ordering::Release);
            self.original.changed.notify_waiters();
        }
    }
}
impl ApprovalStatementFault {
    pub(in crate::session_storage) fn deferred(&self) -> bool {
        self.original.deferred
    }
    pub(in crate::session_storage) fn point(&self) -> ApprovalStatement {
        self.original.point
    }
    pub(in crate::session_storage) fn observe(&self, error: Option<&sqlx::Error>, attempted: bool) {
        let extended = error
            .and_then(sqlx::Error::as_database_error)
            .and_then(sqlx::error::DatabaseError::code)
            .and_then(|value| value.parse::<i32>().ok());
        let observation = ApprovalFaultObservation {
            tag: self.tag,
            point: self.original.point,
            sqlite_primary_code: extended.map(|code| code & 255),
            sqlite_extended_code: extended,
            commit_attempted: self.original.point == ApprovalStatement::Commit && attempted,
            actual_sql_error: attempted && error.is_some(),
        };
        if let Ok(mut value) = self.original.observation.lock() {
            *value = Some(observation);
        }
        self.original.changed.notify_waiters();
    }
}

#[derive(Clone)]
pub(in crate::session_storage) struct ApprovalConnectionContext {
    pub controller: Arc<ApprovalTestController>,
    pub tag: ApprovalOperationTag,
    pub source_job_registered: bool,
    pub commit_confirmed: bool,
}
tokio::task_local! {static APPROVAL_CONTEXT:ApprovalConnectionContext;}
pub(in crate::session_storage) async fn with_original_operation<T>(
    controller: Arc<ApprovalTestController>,
    tag: ApprovalOperationTag,
    future: impl std::future::Future<Output = T>,
) -> T {
    let context = ApprovalConnectionContext {
        controller,
        tag,
        source_job_registered: owned_job_registered_for_approval_test(),
        commit_confirmed: false,
    };
    APPROVAL_CONTEXT.scope(context, future).await
}
pub(in crate::session_storage) fn current_original_context() -> Option<ApprovalConnectionContext> {
    APPROVAL_CONTEXT.try_with(Clone::clone).ok()
}

pub use super::test_migration::*;

#[cfg(test)]
mod tests;
