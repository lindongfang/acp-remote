//! Per-native-target coordination facts. These values are not authentication or resource proofs.
use super::{
    EntityId,
    native::{NativeSessionKey, ResourceEpoch},
};
use std::{collections::HashMap, num::NonZeroUsize};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkActivity {
    Unknown,
    Idle,
    Active,
}

/// A local ordering fence, never an execution ID claimed to have come from ACP.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkToken {
    target: NativeSessionKey,
    epoch: ResourceEpoch,
    serial: u64,
}
impl WorkToken {
    #[must_use]
    pub(crate) const fn serial(&self) -> u64 {
        self.serial
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformInput {
    pub request: EntityId,
    pub run: EntityId,
    pub awaiting_insertion: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForegroundOutcome {
    Completed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinishedWork {
    pub input: Option<PlatformInput>,
    pub outcome: ForegroundOutcome,
}

/// Captures only the work and permissions present when the owner serialized the cancellation.
#[derive(Clone, Debug)]
pub struct CancelScope {
    target: NativeSessionKey,
    epoch: ResourceEpoch,
    serial: u64,
    work: Option<WorkToken>,
    permissions: Vec<EntityId>,
}

impl CancelScope {
    #[must_use]
    pub const fn work(&self) -> Option<&WorkToken> {
        self.work.as_ref()
    }

    #[must_use]
    pub fn permissions(&self) -> &[EntityId] {
        &self.permissions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopEvidence {
    Unknown,
    OwnedResourcesConverged,
    NativeStoppedAndResumeCannotRedispatch,
}

/// The application supplies independently verified facts; clients cannot supply these as grants.
#[allow(
    clippy::struct_excessive_bools,
    reason = "Independent recovery prerequisites must all be verified; they are not lifecycle states"
)]
pub struct RecoveryFacts {
    pub target: NativeSessionKey,
    pub old_epoch: ResourceEpoch,
    pub stop: StopEvidence,
    pub identity_verified: bool,
    pub unique_owner_verified: bool,
    pub restore_succeeded: bool,
    pub management_reconciled: bool,
}

pub struct SessionActivity {
    target: NativeSessionKey,
    epoch: ResourceEpoch,
    serial: u64,
    connected: bool,
    foreground: WorkActivity,
    background: WorkActivity,
    work: Option<WorkToken>,
    input: Option<PlatformInput>,
    // Only current permission references: no request, tool or conversation bodies.
    permissions: HashMap<EntityId, Option<EntityId>>,
}

impl SessionActivity {
    #[must_use]
    pub fn new(target: NativeSessionKey, epoch: ResourceEpoch) -> Self {
        Self {
            target,
            epoch,
            serial: 0,
            connected: true,
            foreground: WorkActivity::Unknown,
            background: WorkActivity::Unknown,
            work: None,
            input: None,
            permissions: HashMap::new(),
        }
    }

    #[must_use]
    pub const fn epoch(&self) -> ResourceEpoch {
        self.epoch
    }

    #[must_use]
    pub const fn foreground(&self) -> WorkActivity {
        self.foreground
    }

    #[must_use]
    pub const fn background(&self) -> WorkActivity {
        self.background
    }

    #[must_use]
    pub const fn input(&self) -> Option<&PlatformInput> {
        self.input.as_ref()
    }

    #[must_use]
    pub const fn current_work(&self) -> Option<&WorkToken> {
        self.work.as_ref()
    }

    /// Additional authorization, durable admission and resource checks belong to the owner.
    #[must_use]
    pub fn can_accept_input(&self) -> bool {
        self.connected && self.foreground == WorkActivity::Idle && self.input.is_none()
    }

    /// Local facts only; the resource policy must also verify complete observation/close capability.
    #[must_use]
    pub fn locally_quiescent(&self) -> bool {
        self.can_accept_input()
            && self.background == WorkActivity::Idle
            && self.permissions.is_empty()
    }

    /// Records reliable native state or a snapshot. Idle alone never settles an accepted input.
    /// # Errors
    /// Rejects stale/disconnected sources and exhausted local ordering fences.
    pub fn observe_foreground(
        &mut self,
        epoch: ResourceEpoch,
        state: WorkActivity,
    ) -> Result<(), ActivityError> {
        self.require_current(epoch)?;
        if state == WorkActivity::Active && self.work.is_none() {
            self.work = Some(self.next_work()?);
        }
        self.foreground = state;
        if state == WorkActivity::Idle && self.input.is_none() {
            self.work = None;
        }
        Ok(())
    }

    /// # Errors
    /// Rejects stale or disconnected sources; unknown is retained, not interpreted as idle.
    pub fn observe_background(
        &mut self,
        epoch: ResourceEpoch,
        state: WorkActivity,
    ) -> Result<(), ActivityError> {
        self.require_current(epoch)?;
        self.background = state;
        Ok(())
    }

    /// Reserves a single foreground slot after the owner has validated durable admission.
    /// # Errors
    /// Unknown/native active/pending platform work cannot accept another ordinary input.
    pub fn reserve_input(
        &mut self,
        request: EntityId,
        run: EntityId,
    ) -> Result<WorkToken, ActivityError> {
        if !self.can_accept_input() {
            return Err(ActivityError::Busy);
        }
        let token = self.next_work()?;
        self.input = Some(PlatformInput {
            request,
            run,
            awaiting_insertion: true,
        });
        self.work = Some(token.clone());
        Ok(token)
    }

    /// # Errors
    /// Only the current input's insertion response may remove its insertion uncertainty.
    pub fn confirm_insertion(&mut self, work: &WorkToken) -> Result<(), ActivityError> {
        self.require_work(work)?;
        self.input
            .as_mut()
            .ok_or(ActivityError::WrongWork)?
            .awaiting_insertion = false;
        Ok(())
    }

    /// A reliable rejection resolves only the corresponding input, not unrelated native activity.
    /// # Errors
    /// Rejects stale, cross-target and already settled input responses.
    pub fn reject_input(&mut self, work: &WorkToken) -> Result<PlatformInput, ActivityError> {
        self.require_work(work)?;
        let input = self.input.take().ok_or(ActivityError::WrongWork)?;
        if self.foreground == WorkActivity::Idle {
            self.work = None;
        }
        Ok(input)
    }

    /// Consumes reliable work completion, never a bare idle snapshot or a cancel write report.
    /// # Errors
    /// A late completion cannot settle a newer work slot or another target/connection.
    pub fn finish_work(
        &mut self,
        work: &WorkToken,
        outcome: ForegroundOutcome,
    ) -> Result<FinishedWork, ActivityError> {
        self.require_work(work)?;
        self.work = None;
        self.foreground = WorkActivity::Idle;
        Ok(FinishedWork {
            input: self.input.take(),
            outcome,
        })
    }
    /// Releases only the inserted-input coordination slot; this records no Run outcome.
    /// # Errors
    /// The current input must be inserted and the current native foreground reliably idle.
    pub fn release_inserted_slot(
        &mut self,
        work: &WorkToken,
    ) -> Result<PlatformInput, ActivityError> {
        self.require_work(work)?;
        if self.foreground != WorkActivity::Idle
            || self
                .input
                .as_ref()
                .is_none_or(|input| input.awaiting_insertion)
        {
            return Err(ActivityError::Busy);
        }
        let input = self.input.take().ok_or(ActivityError::WrongWork)?;
        self.work = None;
        Ok(input)
    }
    /// Retires only the old coordination slot after the application verifies real resource closure.
    /// This records no native completion and leaves the disconnected target unknown.
    /// # Errors
    /// The original target, epoch and current work token must still match.
    pub(crate) fn release_converged_input(
        &mut self,
        work: &WorkToken,
    ) -> Result<PlatformInput, ActivityError> {
        if work.epoch != self.epoch
            || work.target != self.target
            || self.work.as_ref() != Some(work)
        {
            return Err(ActivityError::WrongWork);
        }
        let input = self.input.take().ok_or(ActivityError::WrongWork)?;
        self.work = None;
        self.connected = false;
        self.foreground = WorkActivity::Unknown;
        self.background = WorkActivity::Unknown;
        Ok(input)
    }

    /// No new Run, foreground work, or waiting-for-idle flag is created by cancellation.
    /// # Errors
    /// A disconnected owner must use its separate, verified convergence path.
    pub fn cancel_scope(&self) -> Result<CancelScope, ActivityError> {
        self.require_current(self.epoch)?;
        Ok(CancelScope {
            target: self.target.clone(),
            epoch: self.epoch,
            serial: self.serial,
            work: self.work.clone(),
            permissions: self.permissions.keys().copied().collect(),
        })
    }

    /// Returns whether this captured cancellation still refers to the same foreground slot.
    #[must_use]
    pub fn cancellation_is_current(&self, scope: &CancelScope) -> bool {
        self.connected
            && scope.target == self.target
            && scope.epoch == self.epoch
            && scope.serial == self.serial
            && scope.work == self.work
    }

    /// Called only for a current, verified native permission request; Run correlation is optional.
    /// # Errors
    /// Bounded current references cannot cross a connection or change their optional Run association.
    pub fn observe_permission(
        &mut self,
        epoch: ResourceEpoch,
        permission: EntityId,
        run: Option<EntityId>,
    ) -> Result<(), ActivityError> {
        self.require_current(epoch)?;
        if let Some(existing) = self.permissions.get(&permission) {
            return if *existing == run {
                Ok(())
            } else {
                Err(ActivityError::PermissionConflict)
            };
        }
        if self.permissions.len() >= 64 {
            return Err(ActivityError::Capacity);
        }
        self.permissions.insert(permission, run);
        Ok(())
    }

    /// An explicit protocol invalidation/response retires a request; Run completion does not.
    /// # Errors
    /// Rejects a source from a disconnected or replaced connection.
    pub fn retire_permission(
        &mut self,
        epoch: ResourceEpoch,
        permission: EntityId,
    ) -> Result<bool, ActivityError> {
        self.require_current(epoch)?;
        Ok(self.permissions.remove(&permission).is_some())
    }

    /// Marks uncertainty without releasing the input slot or inferring that native work stopped.
    /// # Errors
    /// A stale connection cannot interrupt the current owner.
    pub fn interrupt(
        &mut self,
        epoch: ResourceEpoch,
    ) -> Result<Option<PlatformInput>, ActivityError> {
        if epoch != self.epoch {
            return Err(ActivityError::StaleEpoch);
        }
        self.connected = false;
        self.foreground = WorkActivity::Unknown;
        self.background = WorkActivity::Unknown;
        self.permissions.clear();
        Ok(self.input.clone())
    }

    /// Clears old execution blocking only after all independent recovery facts were verified.
    /// The returned prior input remains interrupted/unknown in the management repository.
    /// # Errors
    /// A new handshake, timeout or changed epoch alone does not authorize new input.
    pub fn recover(
        &mut self,
        epoch: ResourceEpoch,
        facts: &RecoveryFacts,
    ) -> Result<Option<PlatformInput>, ActivityError> {
        if self.connected || epoch == self.epoch {
            return Err(ActivityError::RecoveryUnverified);
        }
        if facts.old_epoch != self.epoch {
            return Err(ActivityError::RecoveryUnverified);
        }
        if facts.target != self.target {
            return Err(ActivityError::RecoveryUnverified);
        }
        if facts.stop == StopEvidence::Unknown
            || !facts.identity_verified
            || !facts.unique_owner_verified
            || !facts.restore_succeeded
            || !facts.management_reconciled
        {
            return Err(ActivityError::RecoveryUnverified);
        }
        let prior = self.input.take();
        self.epoch = epoch;
        self.connected = true;
        self.foreground = WorkActivity::Unknown;
        self.background = WorkActivity::Unknown;
        self.work = None;
        self.permissions.clear();
        Ok(prior)
    }

    fn require_current(&self, epoch: ResourceEpoch) -> Result<(), ActivityError> {
        if epoch != self.epoch {
            Err(ActivityError::StaleEpoch)
        } else if !self.connected {
            Err(ActivityError::Disconnected)
        } else {
            Ok(())
        }
    }

    fn require_work(&self, work: &WorkToken) -> Result<(), ActivityError> {
        self.require_current(work.epoch)?;
        if self.work.as_ref() == Some(work) {
            Ok(())
        } else {
            Err(ActivityError::WrongWork)
        }
    }

    fn next_work(&mut self) -> Result<WorkToken, ActivityError> {
        self.serial = self.serial.checked_add(1).ok_or(ActivityError::Capacity)?;
        Ok(WorkToken {
            target: self.target.clone(),
            epoch: self.epoch,
            serial: self.serial,
        })
    }
}

/// A bounded identity registry: configuration aliases cannot create independent execution slots.
pub struct ActivityRegistry {
    targets: HashMap<NativeSessionKey, SessionActivity>,
    limit: NonZeroUsize,
}

impl ActivityRegistry {
    #[must_use]
    pub fn new(limit: NonZeroUsize) -> Self {
        Self {
            targets: HashMap::new(),
            limit,
        }
    }

    /// # Errors
    /// Rejects a second live owner and new targets beyond this registry's explicit budget.
    pub fn claim(
        &mut self,
        target: NativeSessionKey,
        epoch: ResourceEpoch,
    ) -> Result<&mut SessionActivity, ActivityError> {
        if !self.targets.contains_key(&target) && self.targets.len() >= self.limit.get() {
            return Err(ActivityError::Capacity);
        }
        let activity = self
            .targets
            .entry(target)
            .or_insert_with_key(|key| SessionActivity::new(key.clone(), epoch));
        if activity.epoch != epoch {
            return Err(ActivityError::OwnerConflict);
        }
        Ok(activity)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ActivityError {
    #[error("native foreground is not available for ordinary input")]
    Busy,
    #[error("native activity source belongs to a stale resource epoch")]
    StaleEpoch,
    #[error("native connection is not current")]
    Disconnected,
    #[error("native work result does not match the current foreground slot")]
    WrongWork,
    #[error("native target already has another resource owner")]
    OwnerConflict,
    #[error("native recovery facts are incomplete or do not match the old owner")]
    RecoveryUnverified,
    #[error("native activity coordination capacity is exhausted")]
    Capacity,
    #[error("native permission reference conflicts with an earlier observation")]
    PermissionConflict,
}

#[cfg(test)]
mod tests;
