//! Shared synchronous generation fences. They contain no native resource/Loaded proof.
use crate::domain::native::NativeSessionKey;
use crate::domain::shared_view::{ViewEpoch, ViewError};
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
pub(super) struct Group {
    pub(super) sealed: bool,
    targets: HashMap<NativeSessionKey, Arc<AtomicBool>>,
}
#[derive(Clone)]
pub struct OwnedDisplaySendGroup {
    pub(super) inner: Arc<Mutex<Group>>,
}
#[derive(Clone)]
pub struct OwnedDisplaySendView {
    pub(super) group: OwnedDisplaySendGroup,
    pub(super) epoch: ViewEpoch,
    pub(super) sealed: Arc<AtomicBool>,
    target: Option<Arc<AtomicBool>>,
}
impl OwnedDisplaySendGroup {
    pub(super) fn target_live_in(group: &Group, target: &NativeSessionKey) -> bool {
        !group.sealed
            && group
                .targets
                .get(target)
                .is_some_and(|target| !target.load(Ordering::Acquire))
    }
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Group {
                sealed: false,
                targets: HashMap::new(),
            })),
        }
    }
    /// # Errors
    /// Rejects a poisoned short generation fence.
    pub fn seal(&self) -> Result<(), ViewError> {
        self.inner
            .lock()
            .map_err(|_| ViewError::ReadFailure)?
            .sealed = true;
        Ok(())
    }
    #[must_use]
    pub fn view(&self, epoch: ViewEpoch) -> OwnedDisplaySendView {
        OwnedDisplaySendView {
            group: self.clone(),
            epoch,
            sealed: Arc::new(AtomicBool::new(false)),
            target: None,
        }
    }
    pub(crate) fn view_for_target(
        &self,
        epoch: ViewEpoch,
        target: &NativeSessionKey,
    ) -> Result<OwnedDisplaySendView, ViewError> {
        let mut group = self.inner.lock().map_err(|_| ViewError::ReadFailure)?;
        let target = if let Some(target) = group.targets.get(target) {
            Arc::clone(target)
        } else {
            group
                .targets
                .try_reserve(1)
                .map_err(|_| ViewError::AllocationFailure)?;
            let flag = Arc::new(AtomicBool::new(false));
            group.targets.insert(target.clone(), flag.clone());
            flag
        };
        drop(group);
        Ok(OwnedDisplaySendView {
            group: self.clone(),
            epoch,
            sealed: Arc::new(AtomicBool::new(false)),
            target: Some(target),
        })
    }
    /// Denies this original source's target, including generations published later.
    /// This display fence grants no native qualification and is never reset.
    pub(crate) fn seal_target(&self, target: &NativeSessionKey) -> Result<(), ViewError> {
        let mut group = self.inner.lock().map_err(|_| ViewError::ReadFailure)?;
        if let Some(target) = group.targets.get(target) {
            target.store(true, Ordering::Release);
        } else {
            // Closure may precede the worker's first projection for this target.
            // Retain the deny fact before a later generation can be published.
            if group.targets.try_reserve(1).is_err() {
                group.sealed = true;
                return Err(ViewError::AllocationFailure);
            }
            group
                .targets
                .insert(target.clone(), Arc::new(AtomicBool::new(true)));
        }
        drop(group);
        Ok(())
    }
}
impl Default for OwnedDisplaySendGroup {
    fn default() -> Self {
        Self::new()
    }
}
impl OwnedDisplaySendView {
    /// # Errors
    /// Rejects a poisoned short generation fence.
    pub fn seal(&self) -> Result<(), ViewError> {
        let _fence = self
            .group
            .inner
            .lock()
            .map_err(|_| ViewError::ReadFailure)?;
        self.sealed.store(true, Ordering::Release);
        Ok(())
    }
    #[must_use]
    pub const fn epoch(&self) -> ViewEpoch {
        self.epoch
    }
    pub(crate) fn current(&self) -> Result<bool, ViewError> {
        let fence = self
            .group
            .inner
            .lock()
            .map_err(|_| ViewError::ReadFailure)?;
        Ok(self.current_in(&fence))
    }
    pub(super) fn current_in(&self, group: &Group) -> bool {
        !group.sealed
            && !self.sealed.load(Ordering::Acquire)
            && !self
                .target
                .as_ref()
                .is_some_and(|target| target.load(Ordering::Acquire))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::EntityId;

    fn target(id: u8) -> NativeSessionKey {
        NativeSessionKey::from_config(
            Some(EntityId::from_bytes([1; 16])),
            &format!("synthetic-{id}"),
        )
        .unwrap()
    }

    #[test]
    fn target_close_before_projection_denies_later_generations_only_on_that_source() {
        let group = OwnedDisplaySendGroup::new();
        let first = target(1);
        let other = target(2);
        group.seal_target(&first).unwrap();
        let late = group
            .view_for_target(ViewEpoch::fresh().unwrap(), &first)
            .unwrap();
        let healthy = group
            .view_for_target(ViewEpoch::fresh().unwrap(), &other)
            .unwrap();
        let another_source = OwnedDisplaySendGroup::new()
            .view_for_target(ViewEpoch::fresh().unwrap(), &first)
            .unwrap();
        assert!(!late.current().unwrap());
        assert!(healthy.current().unwrap());
        assert!(another_source.current().unwrap());
    }

    #[test]
    fn target_close_seals_every_existing_and_future_view_without_clearing_the_fact() {
        let group = OwnedDisplaySendGroup::new();
        let first = target(1);
        let old = group
            .view_for_target(ViewEpoch::fresh().unwrap(), &first)
            .unwrap();
        let next = group
            .view_for_target(ViewEpoch::fresh().unwrap(), &first)
            .unwrap();
        assert!(old.current().unwrap() && next.current().unwrap());
        group.seal_target(&first).unwrap();
        assert!(!old.current().unwrap() && !next.current().unwrap());
        group.seal_target(&first).unwrap();
        assert!(
            !group
                .view_for_target(ViewEpoch::fresh().unwrap(), &first)
                .unwrap()
                .current()
                .unwrap()
        );
    }
}
