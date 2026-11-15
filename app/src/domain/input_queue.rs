//! Shared, volatile waiting inputs. Trusted owners supply readiness and settlement facts.
//! This model performs no authentication, durable deduplication or protocol I/O.
#![allow(
    clippy::option_if_let_else,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    EntityId,
    native::{NativeSessionKey, ResourceEpoch},
    native_dispatch::KnownStopReason,
};
use std::{collections::VecDeque, fmt, sync::Arc};

pub const MAX_WAITING_INPUTS: usize = 3;

pub struct QueueInput<T> {
    pub id: EntityId,
    pub submitter: EntityId,
    pub body: T,
}
impl<T> fmt::Debug for QueueInput<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QueueInput")
            .field("id", &self.id)
            .field("submitter", &self.submitter)
            .finish_non_exhaustive()
    }
}
pub struct Rejected<T> {
    pub error: QueueError,
    pub value: T,
}
impl<T> fmt::Debug for Rejected<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Rejected")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
pub struct QueueEntry<T> {
    input: QueueInput<T>,
    revision: u64,
    editor: EntityId,
    suspended: bool,
}
impl<T> QueueEntry<T> {
    const fn new(input: QueueInput<T>) -> Self {
        let editor = input.submitter;
        Self {
            input,
            revision: 1,
            editor,
            suspended: false,
        }
    }
    #[must_use]
    pub const fn id(&self) -> EntityId {
        self.input.id
    }
    #[must_use]
    pub const fn input(&self) -> &QueueInput<T> {
        &self.input
    }
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    #[must_use]
    pub const fn editor(&self) -> EntityId {
        self.editor
    }
    #[must_use]
    pub const fn suspended(&self) -> bool {
        self.suspended
    }
    #[must_use]
    pub fn into_input(self) -> QueueInput<T> {
        self.input
    }
}

/// Internal observations, never a client-supplied permission or ACP correlation proof.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchReadiness {
    Blocked,
    Ready(ResourceEpoch),
}

/// Local instance/version fence. Cloning it cannot duplicate a state transition.
#[derive(Clone)]
pub struct DispatchToken {
    instance: Arc<()>,
    serial: u64,
    id: EntityId,
    revision: u64,
    epoch: ResourceEpoch,
}
impl fmt::Debug for DispatchToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DispatchToken")
            .field("id", &self.id)
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}
#[derive(Debug)]
pub enum Submission {
    Queued { id: EntityId, revision: u64 },
    Dispatching(DispatchToken),
}
/// An owner's verified scheduling decision, not a native success/failure result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueDisposition {
    Advance,
    Suspend,
}

/// Why an accepted interrupt-and-send selection can no longer be honored.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PriorityFailure {
    SelectionChanged,
    InterruptUnconfirmed,
}
impl PriorityFailure {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::SelectionChanged => "selection_changed",
            Self::InterruptUnconfirmed => "interrupt_unconfirmed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum QueueError {
    #[error("shared waiting queue is full")]
    Full,
    #[error("input identity already exists in this queue")]
    DuplicateId,
    #[error("waiting input was not found")]
    NotFound,
    #[error("waiting input revision changed")]
    RevisionChanged,
    #[error("waiting input revision is exhausted")]
    RevisionExhausted,
    #[error("input dispatch has already started")]
    DispatchStarted,
    #[error("waiting input requires explicit resume")]
    Suspended,
    #[error("a dispatch is active or already prepared")]
    Busy,
    #[error("dispatch token is no longer current")]
    StaleToken,
    #[error("native target is not ready for dispatch")]
    Blocked,
    #[error("native resource generation changed")]
    ResourceChanged,
    #[error("local dispatch sequence is exhausted")]
    SequenceExhausted,
}

struct Active<T> {
    entry: QueueEntry<T>,
    token: DispatchToken,
}

/// One trusted application owner must reuse this instance for a native target.
/// Methods only serialize local state; they do not wait for SQL, writers or native work.
pub struct InputQueue<T> {
    target: NativeSessionKey,
    instance: Arc<()>,
    serial: u64,
    waiting: VecDeque<QueueEntry<T>>,
    prepared: Option<DispatchToken>,
    active: Option<Active<T>>,
    priority: Option<EntityId>,
    failure_suspended: bool,
    last_stop_reason: Option<KnownStopReason>,
    priority_failure: Option<PriorityFailure>,
}
impl<T> InputQueue<T> {
    #[must_use]
    pub fn new(target: NativeSessionKey) -> Self {
        Self {
            target,
            instance: Arc::new(()),
            serial: 0,
            waiting: VecDeque::new(),
            prepared: None,
            active: None,
            priority: None,
            failure_suspended: false,
            last_stop_reason: None,
            priority_failure: None,
        }
    }
    #[must_use]
    pub const fn target(&self) -> &NativeSessionKey {
        &self.target
    }
    pub fn waiting(&self) -> impl ExactSizeIterator<Item = &QueueEntry<T>> {
        self.waiting.iter()
    }
    #[must_use]
    pub fn active(&self) -> Option<&QueueEntry<T>> {
        self.active.as_ref().map(|a| &a.entry)
    }
    #[must_use]
    pub const fn priority(&self) -> Option<EntityId> {
        self.priority
    }
    #[must_use]
    pub const fn failure_suspended(&self) -> bool {
        self.failure_suspended
    }
    #[must_use]
    pub const fn last_stop_reason(&self) -> Option<KnownStopReason> {
        self.last_stop_reason
    }
    #[must_use]
    pub const fn priority_failure(&self) -> Option<PriorityFailure> {
        self.priority_failure
    }

    /// Accepts an already registered input; historical IDs must be rejected by its registry.
    /// A direct handoff requires the owner to be ready to start transmission immediately.
    /// # Errors
    /// Returns the untouched input on duplicate identity, full queue or exhausted sequence.
    pub fn submit(
        &mut self,
        input: QueueInput<T>,
        readiness: DispatchReadiness,
    ) -> Result<Submission, Rejected<QueueInput<T>>> {
        if self.active().is_some_and(|e| e.id() == input.id)
            || self.waiting.iter().any(|e| e.id() == input.id)
        {
            return Err(Rejected {
                error: QueueError::DuplicateId,
                value: input,
            });
        }
        if let DispatchReadiness::Ready(epoch) = readiness
            && self.failure_suspended
            && self.active.is_none()
            && self.prepared.is_none()
            && self.waiting.iter().all(|e| e.suspended)
        {
            let token = match self.next_token(input.id, 1, epoch) {
                Ok(token) => token,
                Err(error) => {
                    return Err(Rejected {
                        error,
                        value: input,
                    });
                }
            };
            self.active = Some(Active {
                entry: QueueEntry::new(input),
                token: token.clone(),
            });
            return Ok(Submission::Dispatching(token));
        }
        if self.waiting.len() == MAX_WAITING_INPUTS {
            return Err(Rejected {
                error: QueueError::Full,
                value: input,
            });
        }
        let id = input.id;
        self.waiting.push_back(QueueEntry::new(input));
        Ok(Submission::Queued { id, revision: 1 })
    }

    /// # Errors
    /// Returns the proposed body unchanged if the item/version is no longer editable.
    pub fn edit(
        &mut self,
        id: EntityId,
        revision: u64,
        editor: EntityId,
        body: T,
    ) -> Result<T, Rejected<T>> {
        let index = match self.waiting_index(id, revision) {
            Ok(index) => index,
            Err(error) => return Err(Rejected { error, value: body }),
        };
        let selected = self.priority == Some(id);
        let entry = &mut self.waiting[index];
        let Some(next) = entry.revision.checked_add(1) else {
            return Err(Rejected {
                error: QueueError::RevisionExhausted,
                value: body,
            });
        };
        let old = std::mem::replace(&mut entry.input.body, body);
        entry.revision = next;
        entry.editor = editor;
        self.invalidate_preparation(id);
        if selected {
            self.fail_priority(PriorityFailure::SelectionChanged);
        }
        Ok(old)
    }
    /// # Errors
    /// Rejects a missing, revised or already dispatching input without interrupting native work.
    pub fn cancel(&mut self, id: EntityId, revision: u64) -> Result<QueueEntry<T>, QueueError> {
        let index = self.waiting_index(id, revision)?;
        let selected = self.priority == Some(id);
        let entry = self.waiting.remove(index).ok_or(QueueError::NotFound)?;
        self.invalidate_preparation(id);
        if selected {
            self.fail_priority(PriorityFailure::SelectionChanged);
        }
        Ok(entry)
    }
    /// Selects an existing runnable item. The application separately interrupts old work.
    /// # Errors
    /// Rejects a stale, active or suspended item; never resumes other waiting items.
    pub fn prioritize(&mut self, id: EntityId, revision: u64) -> Result<(), QueueError> {
        let index = self.waiting_index(id, revision)?;
        if self.waiting[index].suspended {
            return Err(QueueError::Suspended);
        }
        if self.priority != Some(id) {
            self.prepared = None;
        }
        self.priority = Some(id);
        self.priority_failure = None;
        Ok(())
    }
    /// Existing runnable inputs precede resumed old items; neither group is internally reordered.
    /// Returns the number actually resumed. An ordinary no-op leaves preparation valid.
    pub fn resume(&mut self) -> usize {
        let count = self.waiting.iter().filter(|e| e.suspended).count();
        if !self.failure_suspended && count == 0 && self.priority_failure.is_none() {
            return 0;
        }
        self.prepared = None;
        self.waiting.make_contiguous().sort_by_key(|e| e.suspended);
        for entry in &mut self.waiting {
            entry.suspended = false;
        }
        self.failure_suspended = false;
        self.last_stop_reason = None;
        self.priority_failure = None;
        count
    }

    /// Preparation keeps the input editable and charged to its original waiting slot.
    /// # Errors
    /// Rejects overlapping preparation/active dispatch and sequence exhaustion.
    pub fn prepare(
        &mut self,
        readiness: DispatchReadiness,
    ) -> Result<Option<DispatchToken>, QueueError> {
        if self.active.is_some() || self.prepared.is_some() {
            return Err(QueueError::Busy);
        }
        let DispatchReadiness::Ready(epoch) = readiness else {
            return Ok(None);
        };
        let chosen = if let Some(id) = self.priority {
            self.waiting
                .iter()
                .find(|entry| entry.id() == id && !entry.suspended)
        } else {
            self.waiting.iter().find(|entry| !entry.suspended)
        };
        let Some(entry) = chosen else {
            return Ok(None);
        };
        let token = self.next_token(entry.id(), entry.revision, epoch)?;
        self.prepared = Some(token.clone());
        Ok(Some(token))
    }
    /// Borrows editable prepared content without selecting it.
    /// # Errors
    /// Rejects a stale or foreign preparation.
    pub(crate) fn prepared_input(
        &self,
        token: &DispatchToken,
    ) -> Result<&QueueEntry<T>, QueueError> {
        let index = self.require_prepared(token)?;
        self.waiting.get(index).ok_or(QueueError::StaleToken)
    }
    pub(crate) fn next_runnable(&self) -> Option<&QueueEntry<T>> {
        if let Some(id) = self.priority {
            self.waiting
                .iter()
                .find(|entry| entry.id() == id && !entry.suspended())
        } else {
            self.waiting.iter().find(|entry| !entry.suspended())
        }
    }
    pub(crate) fn priority_matches(&self, item: EntityId, revision: u64) -> bool {
        self.priority == Some(item)
            && self
                .waiting
                .iter()
                .any(|e| e.id() == item && e.revision() == revision && !e.suspended())
    }
    /// # Errors
    /// Rejects a token from another instance or an invalidated preparation.
    pub fn release_preparation(&mut self, token: &DispatchToken) -> Result<(), QueueError> {
        self.require_prepared(token)?;
        self.prepared = None;
        Ok(())
    }
    /// Final synchronous handoff, to be performed by the owner at its transmission boundary.
    /// # Errors
    /// A stale token, blocked target or changed resource leaves preparation/waiting untouched.
    pub fn begin_send(
        &mut self,
        token: &DispatchToken,
        readiness: DispatchReadiness,
    ) -> Result<(), QueueError> {
        let index = self.require_prepared(token)?;
        let DispatchReadiness::Ready(epoch) = readiness else {
            return Err(QueueError::Blocked);
        };
        if epoch != token.epoch {
            return Err(QueueError::ResourceChanged);
        }
        let entry = self.waiting.remove(index).ok_or(QueueError::StaleToken)?;
        self.prepared = None;
        if self.priority == Some(token.id) {
            self.priority = None;
        }
        self.active = Some(Active {
            entry,
            token: token.clone(),
        });
        Ok(())
    }
    /// # Errors
    /// Only the current active token can borrow dispatch content.
    pub fn dispatch_input(&self, token: &DispatchToken) -> Result<&QueueEntry<T>, QueueError> {
        let active = self.active.as_ref().ok_or(QueueError::StaleToken)?;
        if !Self::same_token(&active.token, token) {
            return Err(QueueError::StaleToken);
        }
        Ok(&active.entry)
    }
    /// The owner must first resolve insertion/activity/resource uncertainty independently.
    /// Returning content never authorizes an automatic re-admission or retry.
    /// # Errors
    /// Rejects stale/foreign/already settled tokens without releasing current ownership.
    pub fn settle(
        &mut self,
        token: &DispatchToken,
        disposition: QueueDisposition,
    ) -> Result<QueueEntry<T>, QueueError> {
        self.dispatch_input(token)?;
        let active = self.active.take().ok_or(QueueError::StaleToken)?;
        if disposition == QueueDisposition::Suspend {
            self.suspend_waiting();
        }
        Ok(active.entry)
    }

    /// Suspends exactly the waiting set at this processing boundary, without settling active work.
    /// Returns only the number of newly suspended items; no native completion is implied.
    pub fn suspend_waiting(&mut self) -> usize {
        self.suspend_waiting_with_reason(KnownStopReason::Unknown)
    }
    pub fn suspend_waiting_with_reason(&mut self, reason: KnownStopReason) -> usize {
        let mut changed = 0;
        for entry in &mut self.waiting {
            if !entry.suspended {
                changed += 1;
            }
            entry.suspended = true;
        }
        self.prepared = None;
        self.priority = None;
        self.failure_suspended = true;
        self.last_stop_reason = Some(reason);
        changed
    }

    /// Rejects the current interrupt-and-send selection and pauses every remaining item.
    ///
    /// This is not a native stop. Explicit resume is the only operation that clears the
    /// selection failure and makes the retained items runnable again.
    pub(crate) fn fail_priority(&mut self, failure: PriorityFailure) {
        self.suspend_waiting();
        self.last_stop_reason = None;
        self.priority_failure = Some(failure);
    }
    pub(crate) fn suspend_except_priority(&mut self, item: EntityId, revision: u64) -> usize {
        self.suspend_except_priority_with_reason(item, revision, KnownStopReason::Unknown)
    }
    pub(crate) fn suspend_except_priority_with_reason(
        &mut self,
        item: EntityId,
        revision: u64,
        reason: KnownStopReason,
    ) -> usize {
        if !self.priority_matches(item, revision) {
            return self.suspend_waiting_with_reason(reason);
        }
        let mut changed = 0;
        for entry in &mut self.waiting {
            if entry.id() != item {
                if !entry.suspended {
                    changed += 1;
                }
                entry.suspended = true;
            }
        }
        self.prepared = None;
        self.failure_suspended = true;
        self.last_stop_reason = Some(reason);
        changed
    }

    fn waiting_index(&self, id: EntityId, revision: u64) -> Result<usize, QueueError> {
        if self.active().is_some_and(|e| e.id() == id) {
            return Err(QueueError::DispatchStarted);
        }
        let index = self
            .waiting
            .iter()
            .position(|e| e.id() == id)
            .ok_or(QueueError::NotFound)?;
        if self.waiting[index].revision != revision {
            return Err(QueueError::RevisionChanged);
        }
        Ok(index)
    }
    fn invalidate_preparation(&mut self, id: EntityId) {
        if self.prepared.as_ref().is_some_and(|token| token.id == id) {
            self.prepared = None;
        }
    }
    fn next_token(
        &mut self,
        id: EntityId,
        revision: u64,
        epoch: ResourceEpoch,
    ) -> Result<DispatchToken, QueueError> {
        let serial = self
            .serial
            .checked_add(1)
            .ok_or(QueueError::SequenceExhausted)?;
        self.serial = serial;
        Ok(DispatchToken {
            instance: Arc::clone(&self.instance),
            serial,
            id,
            revision,
            epoch,
        })
    }
    fn same_token(left: &DispatchToken, right: &DispatchToken) -> bool {
        Arc::ptr_eq(&left.instance, &right.instance)
            && left.serial == right.serial
            && left.id == right.id
            && left.revision == right.revision
            && left.epoch == right.epoch
    }
    fn require_prepared(&self, token: &DispatchToken) -> Result<usize, QueueError> {
        let prepared = self.prepared.as_ref().ok_or(QueueError::StaleToken)?;
        if !Self::same_token(prepared, token) {
            return Err(QueueError::StaleToken);
        }
        self.waiting_index(token.id, token.revision)
            .map_err(|_| QueueError::StaleToken)
    }
}

#[cfg(test)]
mod tests;
