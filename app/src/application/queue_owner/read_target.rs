//! A read admission witnesses the original running queue owner without creating an input.
use super::{
    Arc, EntityId, NativeQueueDriver, NativeQueueLink, NativeSessionKey, OwnerCell, OwnerIdentity,
    RuntimeCore, RuntimeEpoch, lock,
};
use crate::domain::native_dispatch::DispatchError;
use std::sync::Weak;

trait ReadFence: Send + Sync {
    fn current(&self) -> Result<(), DispatchError>;
}
struct OriginalReadOwner<T> {
    core: Weak<RuntimeCore<T>>,
    owner: Weak<OwnerCell<T>>,
}
impl<T: Send + 'static> ReadFence for OriginalReadOwner<T> {
    fn current(&self) -> Result<(), DispatchError> {
        let core = self.core.upgrade().ok_or(DispatchError::NodeStopping)?;
        core.running()?;
        let owner = self.owner.upgrade().ok_or(DispatchError::InvalidTarget)?;
        let owners = lock(&core.owners)?;
        if !owners
            .get(&owner.native)
            .is_some_and(|original| Arc::ptr_eq(original, &owner))
        {
            return Err(DispatchError::InvalidTarget);
        }
        drop(owners);
        Ok(())
    }
}

/// Produced only by the original service's authenticated read admission.
/// This does not witness an input, Loaded, execution or protocol completion.
pub struct ReadTargetProof {
    registry_owner: crate::application::request_registry::RegistryOwnerScope,
    target: EntityId,
    native: NativeSessionKey,
    identity: Arc<OwnerIdentity>,
    core_identity: Arc<()>,
    fence: Arc<dyn ReadFence>,
    pub(crate) link: Arc<dyn NativeQueueLink>,
}
impl ReadTargetProof {
    pub(super) fn from_owner<T: Send + 'static>(
        core: &Arc<RuntimeCore<T>>,
        owner: &Arc<OwnerCell<T>>,
        target: EntityId,
    ) -> Self {
        Self {
            registry_owner: core.registry_owner.clone(),
            target,
            native: owner.native.clone(),
            identity: Arc::clone(&owner.identity),
            core_identity: Arc::clone(&core.identity),
            fence: Arc::new(OriginalReadOwner {
                core: Arc::downgrade(core),
                owner: Arc::downgrade(owner),
            }),
            link: super::native::observation_link(Arc::clone(core), Arc::clone(owner)),
        }
    }
    #[must_use]
    pub const fn target(&self) -> EntityId {
        self.target
    }
    #[must_use]
    pub const fn native(&self) -> &NativeSessionKey {
        &self.native
    }
    pub(crate) fn owner(&self) -> EntityId {
        self.identity.id
    }
    pub(crate) fn runtime(&self) -> RuntimeEpoch {
        self.identity.runtime
    }
    pub(crate) fn verify_registry_owner(
        &self,
        scope: &crate::application::request_registry::RegistryOwnerScope,
    ) -> Result<(), DispatchError> {
        if !self.registry_owner.matches(scope) {
            return Err(DispatchError::InvalidScope);
        }
        self.registry_owner.require_open()?;
        Ok(())
    }
    pub(crate) fn require_current(&self) -> Result<(), DispatchError> {
        self.fence.current()
    }
}
impl<T: Send + 'static> NativeQueueDriver<T> {
    pub(crate) fn verify_read_target(&self, read: &ReadTargetProof) -> Result<(), DispatchError> {
        read.require_current()?;
        self.core.running()?;
        if !Arc::ptr_eq(&self.core.identity, &read.core_identity)
            || self.core.runtime != read.runtime()
        {
            return Err(DispatchError::InvalidTarget);
        }
        let owners = lock(&self.core.owners)?;
        if !owners
            .get(read.native())
            .is_some_and(|owner| Arc::ptr_eq(&owner.identity, &read.identity))
        {
            return Err(DispatchError::InvalidTarget);
        }
        drop(owners);
        Ok(())
    }
}
