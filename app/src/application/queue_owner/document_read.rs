//! Branded original queue snapshots. The payload goes directly into its pre-registered root slot.
#![allow(
    clippy::too_many_arguments,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    Arc, EntityId, Future, OwnerCell, OwnerState, Pin, ProcessingPhase, RegistrationId,
    RegistryError, RuntimeCore, lock, teardown_lock,
};
use crate::application::{
    document_disposal::DocumentPayloadSlot,
    document_source::{DocumentEpoch, DocumentError, DocumentSourceLease, QueuedDocumentSource},
    shared_views::RetainedRaw,
};
use std::sync::Weak;
pub type QueueDocumentError = DocumentError;
#[derive(Clone, Copy, Debug)]
pub struct QueueSnapshotProcessing {
    pub(crate) phase: ProcessingPhase,
    pub(crate) applied: bool,
    pub(crate) request: RegistrationId,
}
pub struct QueuedBodySnapshot<B: RetainedRaw, P: RetainedRaw = B> {
    core: Weak<RuntimeCore<B>>,
    owner: Weak<OwnerCell<B>>,
    core_identity: Arc<()>,
    registry_owner: crate::application::request_registry::RegistryOwnerScope,
    target: EntityId,
    item: EntityId,
    revision: u64,
    total: u64,
    digest: [u8; 32],
    epoch: Arc<DocumentEpoch>,
    payload: DocumentPayloadSlot<P>,
    processing: Option<QueueSnapshotProcessing>,
}
impl<B: RetainedRaw, P: RetainedRaw> Clone for QueuedBodySnapshot<B, P> {
    fn clone(&self) -> Self {
        Self {
            core: self.core.clone(),
            owner: self.owner.clone(),
            core_identity: self.core_identity.clone(),
            registry_owner: self.registry_owner.clone(),
            target: self.target,
            item: self.item,
            revision: self.revision,
            total: self.total,
            digest: self.digest,
            epoch: self.epoch.clone(),
            payload: self.payload.clone(),
            processing: self.processing,
        }
    }
}
impl<B: RetainedRaw, P: RetainedRaw> QueuedBodySnapshot<B, P> {
    pub(super) fn from_original(
        core: &Arc<RuntimeCore<B>>,
        owner: &Arc<OwnerCell<B>>,
        target: EntityId,
        item: EntityId,
        revision: u64,
        total: u64,
        digest: [u8; 32],
        epoch: Arc<DocumentEpoch>,
        payload: DocumentPayloadSlot<P>,
        processing: Option<QueueSnapshotProcessing>,
    ) -> Self {
        Self {
            core: Arc::downgrade(core),
            owner: Arc::downgrade(owner),
            core_identity: core.identity.clone(),
            registry_owner: core.registry_owner.clone(),
            target,
            item,
            revision,
            total,
            digest,
            epoch,
            payload,
            processing,
        }
    }
    pub(crate) fn into_document_source(self) -> DocumentSourceLease {
        DocumentSourceLease::queued(Arc::new(self))
    }
    pub(crate) const fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
    pub(crate) const fn processing(&self) -> Option<QueueSnapshotProcessing> {
        self.processing
    }
}
impl<B: RetainedRaw, P: RetainedRaw> QueuedDocumentSource for QueuedBodySnapshot<B, P> {
    fn verify_scope(
        &self,
        scope: &crate::application::request_registry::RegistryOwnerScope,
    ) -> Result<(), DocumentError> {
        if !self.registry_owner.matches(scope) {
            return Err(RegistryError::InvalidActor.into());
        }
        self.registry_owner.require_open()?;
        scope.require_open()?;
        Ok(())
    }

    fn target(&self) -> EntityId {
        self.target
    }
    fn revision(&self) -> u64 {
        self.revision
    }
    fn total_bytes(&self) -> u64 {
        self.total
    }
    fn epoch(&self) -> &Arc<DocumentEpoch> {
        &self.epoch
    }
    fn require_current(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DocumentError>> + Send + '_>> {
        Box::pin(async move {
            self.registry_owner.require_open()?;
            self.payload.require_live()?;
            let core = self.core.upgrade().ok_or(RegistryError::NodeStopping)?;
            core.running()?;
            if !Arc::ptr_eq(&core.identity, &self.core_identity)
                || !core.registry_owner.matches(&self.registry_owner)
            {
                return Err(RegistryError::InvalidActor.into());
            }
            core.registry_owner.require_open()?;
            let owner = self.owner.upgrade().ok_or(RegistryError::InvalidTarget)?;
            {
                let owners = lock(&core.owners)?;
                if !owners
                    .get(&owner.native)
                    .is_some_and(|original| Arc::ptr_eq(original, &owner))
                {
                    return Err(RegistryError::InvalidTarget.into());
                }
            }
            let current = core.resolver.resolve(self.target).await?;
            if current != owner.native {
                return Err(RegistryError::InvalidTarget.into());
            }
            self.epoch.with_fence(|current| {
                if current {
                    Ok(())
                } else {
                    Err(crate::domain::shared_view::ViewError::StaleCursor)
                }
            })?;
            Ok(())
        })
    }
    fn copy_range(&self, offset: usize, length: usize) -> Result<Vec<u8>, DocumentError> {
        self.payload.copy_range(offset, length)
    }
    fn retire(&self) {
        self.payload.retire();
    }
}
pub(super) fn invalidate<T>(state: &mut OwnerState<T>, item: EntityId, revision: u64) {
    if let Some(epoch) = state
        .document_epochs
        .remove(&(item, revision))
        .and_then(|e| e.upgrade())
    {
        state.document_wakes.push(epoch.invalidate());
    }
}
impl<T> OwnerCell<T> {
    pub(super) fn notify_documents(&self) {
        let wakes = {
            let mut state = teardown_lock(&self.state);
            std::mem::take(&mut state.document_wakes)
        };
        for wake in wakes {
            wake.notify();
        }
    }
}

// Declared before the owner guard; reverse destruction releases the state lock before wakeup.
pub(super) struct DocumentWakeOnExit<'a, T>(&'a OwnerCell<T>);
impl<'a, T> DocumentWakeOnExit<'a, T> {
    pub(super) const fn new(owner: &'a OwnerCell<T>) -> Self {
        Self(owner)
    }
}
impl<T> Drop for DocumentWakeOnExit<'_, T> {
    fn drop(&mut self) {
        self.0.notify_documents();
    }
}
