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
    targets: HashMap<NativeSessionKey, TargetFlags>,
}
struct TargetFlags {
    original: Arc<AtomicBool>,
    current: Arc<AtomicBool>,
}
#[derive(Clone)]
pub struct OwnedDisplaySendGroup {
    pub(super) inner: Arc<Mutex<Group>>,
}
#[derive(Clone)]
pub(crate) struct RestoredDisplayTarget {
    group: OwnedDisplaySendGroup,
    flag: Arc<AtomicBool>,
}
impl RestoredDisplayTarget {
    pub(crate) fn seal(&self) -> Result<(), ViewError> {
        let _group = self
            .group
            .inner
            .lock()
            .map_err(|_| ViewError::ReadFailure)?;
        self.flag.store(true, Ordering::Release);
        Ok(())
    }
    pub(crate) fn view(
        &self,
        group: &OwnedDisplaySendGroup,
        epoch: ViewEpoch,
    ) -> Result<OwnedDisplaySendView, ViewError> {
        if !Arc::ptr_eq(&self.group.inner, &group.inner) {
            return Err(ViewError::InvalidScope);
        }
        Ok(OwnedDisplaySendView {
            group: group.clone(),
            epoch,
            sealed: Arc::new(AtomicBool::new(false)),
            confirmed_against: Arc::default(),
            target: Some(self.flag.clone()),
        })
    }
}
#[derive(Clone)]
pub struct OwnedDisplaySendView {
    pub(super) group: OwnedDisplaySendGroup,
    pub(super) epoch: ViewEpoch,
    pub(super) sealed: Arc<AtomicBool>,
    confirmed_against: Arc<std::sync::OnceLock<Arc<AtomicBool>>>,
    target: Option<Arc<AtomicBool>>,
}
impl OwnedDisplaySendGroup {
    pub(super) fn target_live_in(group: &Group, target: &NativeSessionKey) -> bool {
        !group.sealed
            && group
                .targets
                .get(target)
                .is_some_and(|target| !target.current.load(Ordering::Acquire))
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
            confirmed_against: Arc::default(),
            target: None,
        }
    }
    #[cfg(test)]
    pub(crate) fn view_for_target(
        &self,
        epoch: ViewEpoch,
        target: &NativeSessionKey,
    ) -> Result<OwnedDisplaySendView, ViewError> {
        self.target_view(epoch, target, false)
    }
    pub(crate) fn original_view_for_target(
        &self,
        epoch: ViewEpoch,
        target: &NativeSessionKey,
    ) -> Result<OwnedDisplaySendView, ViewError> {
        self.target_view(epoch, target, true)
    }
    fn target_view(
        &self,
        epoch: ViewEpoch,
        target: &NativeSessionKey,
        original: bool,
    ) -> Result<OwnedDisplaySendView, ViewError> {
        let mut group = self.inner.lock().map_err(|_| ViewError::ReadFailure)?;
        let target = if let Some(target) = group.targets.get(target) {
            Arc::clone(if original {
                &target.original
            } else {
                &target.current
            })
        } else {
            group
                .targets
                .try_reserve(1)
                .map_err(|_| ViewError::AllocationFailure)?;
            let flag = Arc::new(AtomicBool::new(false));
            group.targets.insert(
                target.clone(),
                TargetFlags {
                    original: flag.clone(),
                    current: flag.clone(),
                },
            );
            flag
        };
        drop(group);
        Ok(OwnedDisplaySendView {
            group: self.clone(),
            epoch,
            sealed: Arc::new(AtomicBool::new(false)),
            confirmed_against: Arc::default(),
            target: Some(target),
        })
    }
    /// Seals the current resource generation, including projections published later.
    /// Only a separately admitted restore can create a new flag; old flags remain sealed.
    pub(crate) fn seal_target(&self, target: &NativeSessionKey) -> Result<(), ViewError> {
        let mut group = self.inner.lock().map_err(|_| ViewError::ReadFailure)?;
        if let Some(target) = group.targets.get(target) {
            target.current.store(true, Ordering::Release);
        } else {
            // Closure may precede the worker's first projection for this target.
            // Retain the deny fact before a later generation can be published.
            if group.targets.try_reserve(1).is_err() {
                group.sealed = true;
                return Err(ViewError::AllocationFailure);
            }
            group.targets.insert(target.clone(), {
                let flag = Arc::new(AtomicBool::new(true));
                TargetFlags {
                    original: flag.clone(),
                    current: flag,
                }
            });
        }
        drop(group);
        Ok(())
    }
    /// Starts a new explicitly admitted resource generation; old view flags remain sealed.
    pub(crate) fn restore_target(
        &self,
        target: &NativeSessionKey,
    ) -> Result<Option<RestoredDisplayTarget>, ViewError> {
        let mut group = self.inner.lock().map_err(|_| ViewError::ReadFailure)?;
        if group.sealed {
            return Err(ViewError::SourceReleased);
        }
        let replaced = group
            .targets
            .get(target)
            .is_some_and(|old| old.current.load(Ordering::Acquire));
        let restored = replaced.then(|| {
            let flag = Arc::new(AtomicBool::new(false));
            if let Some(target) = group.targets.get_mut(target) {
                target.current = flag.clone();
            }
            RestoredDisplayTarget {
                group: self.clone(),
                flag,
            }
        });
        drop(group);
        Ok(restored)
    }
}
impl Default for OwnedDisplaySendGroup {
    fn default() -> Self {
        Self::new()
    }
}
impl OwnedDisplaySendView {
    // A follower can drain its own completed candidate until a genuinely new
    // publication seals the byte-identical current view.
    pub(crate) fn retain_until_replaced(&self, current: &Self) -> Result<(), ViewError> {
        self.confirmed_against
            .set(Arc::clone(&current.sealed))
            .map_err(|_| ViewError::InvalidStage)
    }
    pub(crate) fn next_epoch(&self, epoch: ViewEpoch) -> Self {
        Self {
            group: self.group.clone(),
            epoch,
            sealed: Arc::new(AtomicBool::new(false)),
            confirmed_against: Arc::default(),
            target: self.target.clone(),
        }
    }
    /// # Errors
    /// Rejects a poisoned short generation fence.
    pub fn seal(&self) -> Result<(), ViewError> {
        self.seal_with(|| {})
    }
    pub(super) fn seal_with(&self, publish: impl FnOnce()) -> Result<(), ViewError> {
        let _fence = self
            .group
            .inner
            .lock()
            .map_err(|_| ViewError::ReadFailure)?;
        self.sealed.store(true, Ordering::Release);
        publish();
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
        !self.sealed.load(Ordering::Acquire)
            && !self
                .confirmed_against
                .get()
                .is_some_and(|flag| flag.load(Ordering::Acquire))
            && self.target_generation_current_in(group)
    }
    /// A normal replay may replace a projection while retaining the original native generation.
    pub(super) fn target_generation_current_in(&self, group: &Group) -> bool {
        !group.sealed
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
    #[test]
    fn explicitly_restored_generation_never_reopens_old_views_or_a_closed_source() {
        let group = OwnedDisplaySendGroup::new();
        let native = target(3);
        let old = group
            .view_for_target(ViewEpoch::fresh().unwrap(), &native)
            .unwrap();
        group.seal_target(&native).unwrap();
        group.restore_target(&native).unwrap();
        let new = group
            .view_for_target(ViewEpoch::fresh().unwrap(), &native)
            .unwrap();
        assert!(!old.current().unwrap());
        assert!(new.current().unwrap());
        group.seal_target(&native).unwrap();
        assert!(!old.current().unwrap() && !new.current().unwrap());
        group.seal().unwrap();
        assert!(matches!(
            group.restore_target(&native),
            Err(ViewError::SourceReleased)
        ));
    }
}
