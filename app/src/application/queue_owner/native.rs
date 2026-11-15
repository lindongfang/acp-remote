//! Private same-owner content/selection captures. SQL data cannot construct these capabilities.
#![allow(
    clippy::significant_drop_tightening,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    ActiveKey, Arc, DispatchReadiness, DispatchToken, EntityId, ExternalWorkKey, NativeQueueDriver,
    NativeSessionKey, Ordering, OwnedPreparation, OwnerCell, OwnerState, PriorityFailure,
    PriorityHandoff, QueueDisposition, ResourceEpoch, RuntimeCore, RuntimeEpoch, ValidatedBody,
    lock,
};
use crate::domain::native_dispatch::{ContentOrigin, DispatchError, KnownStopReason};

pub trait RetainPrompt<T>: Send + Sync {
    type Retained: Clone + Send + Sync + 'static;
    fn retain(&self, body: &ValidatedBody<T>) -> Self::Retained;
}
trait AcceptedFence: Send + Sync {
    fn current(&self) -> Result<(), DispatchError>;
}
struct AcceptedOwner<T> {
    owner: Arc<OwnerCell<T>>,
    item: EntityId,
    origin: ContentOrigin,
}
impl<T: Send + 'static> AcceptedFence for AcceptedOwner<T> {
    fn current(&self) -> Result<(), DispatchError> {
        let s = lock(&self.owner.state)?;
        if s.pending.is_some() || s.origins.get(&self.item) != Some(&self.origin) {
            return Err(DispatchError::StaleSelection);
        }
        let present = s
            .queue
            .active()
            .is_some_and(|e| e.id() == self.item && e.revision() == self.origin.revision)
            || s.queue.waiting().any(|e| {
                e.id() == self.item && e.revision() == self.origin.revision && !e.suspended()
            });
        drop(s);
        if present {
            Ok(())
        } else {
            Err(DispatchError::StaleSelection)
        }
    }
}
pub struct AcceptedTargetProof {
    registry_owner: crate::application::request_registry::RegistryOwnerScope,
    target: EntityId,
    native: NativeSessionKey,
    owner: EntityId,
    runtime: RuntimeEpoch,
    item: EntityId,
    origin: ContentOrigin,
    fence: Arc<dyn AcceptedFence>,
    pub(crate) link: Arc<dyn NativeQueueLink>,
}
impl AcceptedTargetProof {
    #[must_use]
    pub const fn target(&self) -> EntityId {
        self.target
    }
    #[must_use]
    pub const fn native(&self) -> &NativeSessionKey {
        &self.native
    }
    #[must_use]
    pub const fn owner(&self) -> EntityId {
        self.owner
    }
    #[must_use]
    pub const fn runtime(&self) -> RuntimeEpoch {
        self.runtime
    }
    #[must_use]
    pub const fn item(&self) -> EntityId {
        self.item
    }
    #[must_use]
    pub const fn origin(&self) -> ContentOrigin {
        self.origin
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
trait PreparationRelease: Send {
    fn disarm(&mut self);
}
struct PreparationGuard<T> {
    owner: Arc<OwnerCell<T>>,
    token: DispatchToken,
    armed: bool,
}
impl<T: Send> PreparationRelease for PreparationGuard<T> {
    fn disarm(&mut self) {
        self.armed = false;
    }
}
impl<T> Drop for PreparationGuard<T> {
    fn drop(&mut self) {
        if self.armed
            && let Ok(mut state) = self.owner.state.lock()
        {
            let _ = state.queue.release_preparation(&self.token);
        }
    }
}
pub struct PreparedCapture<B> {
    preparation: OwnedPreparation,
    retained: B,
    origin: ContentOrigin,
    native: NativeSessionKey,
    release: Box<dyn PreparationRelease>,
}
pub struct DirectCapture<B> {
    active: ActiveKey,
    retained: B,
    origin: ContentOrigin,
    native: NativeSessionKey,
    target: EntityId,
}
pub trait SelectedFence: Send + Sync {
    fn with_current(
        &self,
        action: &mut dyn FnMut() -> Result<(), DispatchError>,
    ) -> Result<(), DispatchError>;
}
struct SelectedOwner<T> {
    owner: Arc<OwnerCell<T>>,
    active: ActiveKey,
    core: Arc<RuntimeCore<T>>,
}
impl<T: Send + 'static> SelectedFence for SelectedOwner<T> {
    fn with_current(
        &self,
        action: &mut dyn FnMut() -> Result<(), DispatchError>,
    ) -> Result<(), DispatchError> {
        let s = lock(&self.owner.state)?;
        self.core.running()?;
        if !s.active.as_ref().is_some_and(|(a, _)| a.same(&self.active)) {
            return Err(DispatchError::StaleSelection);
        }
        let result = action();
        drop(s);
        result
    }
}
pub struct SelectedContext<B> {
    pub(crate) active: ActiveKey,
    pub(crate) retained: B,
    pub(crate) origin: ContentOrigin,
    pub(crate) native: NativeSessionKey,
    pub(crate) target: EntityId,
    pub(crate) fence: Arc<dyn SelectedFence>,
}
impl<B> SelectedContext<B> {
    pub(crate) fn with_current(
        &self,
        f: &mut dyn FnMut() -> Result<(), DispatchError>,
    ) -> Result<(), DispatchError> {
        self.fence.with_current(f)
    }
}
impl<B> PreparedCapture<B> {
    #[must_use]
    pub const fn origin(&self) -> ContentOrigin {
        self.origin
    }
    #[must_use]
    pub const fn body(&self) -> &B {
        &self.retained
    }
    #[must_use]
    pub const fn native(&self) -> &NativeSessionKey {
        &self.native
    }
    #[must_use]
    pub const fn target(&self) -> EntityId {
        self.preparation.target
    }
    #[must_use]
    pub fn owner(&self) -> EntityId {
        self.preparation.identity.id
    }
    #[must_use]
    pub const fn resource(&self) -> ResourceEpoch {
        self.preparation.resource
    }
}
impl<B> DirectCapture<B> {
    #[must_use]
    pub const fn origin(&self) -> ContentOrigin {
        self.origin
    }
    #[must_use]
    pub const fn body(&self) -> &B {
        &self.retained
    }
    #[must_use]
    pub const fn native(&self) -> &NativeSessionKey {
        &self.native
    }
    #[must_use]
    pub const fn target(&self) -> EntityId {
        self.target
    }
    #[must_use]
    pub const fn active(&self) -> &ActiveKey {
        &self.active
    }
}
impl<T: Send + 'static> NativeQueueDriver<T> {
    pub(crate) fn enable_native_extension(&self) {
        self.core.native_enabled.store(true, Ordering::SeqCst);
    }
    /// Captures a stable accepted source before resource installation; this does not grant IO.
    /// # Errors
    /// Requires the original E to be confirmed and the same content origin to remain current.
    pub async fn accepted_target(
        &self,
        target: EntityId,
    ) -> Result<Option<AcceptedTargetProof>, DispatchError> {
        self.core.running()?;
        let owner = self.core.owner(target).await?;
        #[cfg(windows)]
        let _document_wake = super::document_read::DocumentWakeOnExit::new(&owner);
        let mut s = lock(&owner.state)?;
        self.core
            .drain(&owner, &mut s)
            .map_err(|_| DispatchError::NotReady)?;
        if s.pending.is_some() {
            return Err(DispatchError::RequiredStateUnavailable);
        }
        let entry = s.queue.active().or_else(|| s.queue.next_runnable());
        let Some(entry) = entry else { return Ok(None) };
        let item = entry.id();
        let origin = *s.origins.get(&item).ok_or(DispatchError::CorruptState)?;
        if origin.revision != entry.revision() || origin.digest != *entry.input().body.digest() {
            return Err(DispatchError::CorruptState);
        }
        let result = AcceptedTargetProof {
            registry_owner: self.core.registry_owner.clone(),
            target,
            native: owner.native.clone(),
            owner: owner.identity.id,
            runtime: owner.identity.runtime,
            item,
            origin,
            link: Arc::new(ObservationOwner {
                core: self.core.clone(),
                owner: owner.clone(),
            }),
            fence: Arc::new(AcceptedOwner {
                owner: owner.clone(),
                item,
                origin,
            }),
        };
        drop(s);
        Ok(Some(result))
    }
    pub(crate) async fn prepare_capture<R: RetainPrompt<T> + ?Sized>(
        &self,
        target: EntityId,
        retain: &R,
    ) -> Result<Option<PreparedCapture<R::Retained>>, DispatchError> {
        self.core.running()?;
        let owner = self.core.owner(target).await?;
        #[cfg(windows)]
        let _document_wake = super::document_read::DocumentWakeOnExit::new(&owner);
        let mut s = lock(&owner.state)?;
        self.core
            .drain(&owner, &mut s)
            .map_err(|_| DispatchError::NotReady)?;
        if s.pending.is_some() || s.priority_handoff.is_some() {
            return Err(DispatchError::RequiredStateUnavailable);
        }
        let readiness = self.core.readiness(&owner);
        let token = s
            .queue
            .prepare(readiness)
            .map_err(|_| DispatchError::NotReady)?;
        let Some(token) = token else { return Ok(None) };
        let entry = s
            .queue
            .prepared_input(&token)
            .map_err(|_| DispatchError::StaleSelection)?;
        let origin = *s
            .origins
            .get(&entry.id())
            .ok_or(DispatchError::CorruptState)?;
        let retained = retain.retain(&entry.input().body);
        drop(s);
        let DispatchReadiness::Ready(resource) = readiness else {
            return Err(DispatchError::NotReady);
        };
        Ok(Some(PreparedCapture {
            preparation: OwnedPreparation {
                identity: owner.identity.clone(),
                target,
                token: token.clone(),
                resource,
            },
            retained,
            origin,
            native: owner.native.clone(),
            release: Box::new(PreparationGuard {
                owner: owner.clone(),
                token: token.clone(),
                armed: true,
            }),
        }))
    }
    #[expect(
        clippy::result_large_err,
        reason = "Return the same original linear objects on rejection without a new fallible allocation or loss of cleanup ownership."
    )]
    pub(crate) async fn select_capture<B: Send + 'static>(
        &self,
        mut capture: PreparedCapture<B>,
    ) -> Result<SelectedContext<B>, (DispatchError, PreparedCapture<B>)> {
        let owner = match self.core.owner(capture.preparation.target).await {
            Ok(v) => v,
            Err(e) => return Err((e.into(), capture)),
        };
        let outcome = (|| {
            #[cfg(windows)]
            let _document_wake = super::document_read::DocumentWakeOnExit::new(&owner);
            let mut s = lock(&owner.state)?;
            if s.pending.is_some()
                || s.priority_handoff.is_some()
                || !Arc::ptr_eq(&owner.identity, &capture.preparation.identity)
                || s.origins.get(&capture.origin.submit_request.entity()) != Some(&capture.origin)
            {
                return Err(DispatchError::StaleSelection);
            }
            self.core
                .drain(&owner, &mut s)
                .map_err(|_| DispatchError::NotReady)?;
            let selection_order = s
                .native_order
                .checked_add(1)
                .ok_or(DispatchError::SequenceExhausted)?;
            s.queue
                .begin_send(&capture.preparation.token, self.core.readiness(&owner))
                .map_err(|_| DispatchError::StaleSelection)?;
            s.native_order = selection_order;
            s.selected_priority = None;
            let active = ActiveKey {
                identity: owner.identity.clone(),
                item: capture.origin.submit_request.entity(),
                revision: capture.origin.revision,
                resource: capture.preparation.resource,
                selection_order,
            };
            s.active = Some((active.clone(), capture.preparation.token.clone()));
            drop(s);
            Ok(active)
        })();
        match outcome {
            Ok(active) => {
                capture.release.disarm();
                Ok(SelectedContext {
                    fence: Arc::new(SelectedOwner {
                        owner,
                        active: active.clone(),
                        core: self.core.clone(),
                    }),
                    active,
                    retained: capture.retained,
                    origin: capture.origin,
                    native: capture.native,
                    target: capture.preparation.target,
                })
            }
            Err(e) => Err((e, capture)),
        }
    }
    pub(crate) async fn capture_direct<R: RetainPrompt<T> + ?Sized>(
        &self,
        target: EntityId,
        retain: &R,
    ) -> Result<Option<SelectedContext<R::Retained>>, DispatchError> {
        self.core.running()?;
        let owner = self.core.owner(target).await?;
        #[cfg(windows)]
        let _document_wake = super::document_read::DocumentWakeOnExit::new(&owner);
        let mut s = lock(&owner.state)?;
        self.core
            .drain(&owner, &mut s)
            .map_err(|_| DispatchError::NotReady)?;
        if s.pending.is_some() || s.priority_handoff.is_some() {
            return Err(DispatchError::RequiredStateUnavailable);
        }
        let Some((active, token)) = s.active.as_ref() else {
            return Ok(None);
        };
        let entry = s
            .queue
            .dispatch_input(token)
            .map_err(|_| DispatchError::StaleSelection)?;
        let origin = *s
            .origins
            .get(&entry.id())
            .ok_or(DispatchError::CorruptState)?;
        let selected = SelectedContext {
            active: active.clone(),
            retained: retain.retain(&entry.input().body),
            origin,
            native: owner.native.clone(),
            target,
            fence: Arc::new(SelectedOwner {
                owner: owner.clone(),
                active: active.clone(),
                core: self.core.clone(),
            }),
        };
        drop(s);
        Ok(Some(selected))
    }
    pub(crate) async fn take_priority_handoff(
        &self,
        target: EntityId,
    ) -> Result<Option<PriorityHandoff>, DispatchError> {
        let owner = self.core.owner(target).await?;
        let value = lock(&owner.state)?.priority_handoff.take();
        Ok(value)
    }
    #[cfg(test)]
    pub(crate) async fn priority_current_for_test(
        &self,
        target: EntityId,
        handoff: &PriorityHandoff,
    ) -> Result<(), DispatchError> {
        let owner = self.core.owner(target).await?;
        observation_link(self.core.clone(), owner).priority_current(handoff)
    }
}

pub enum NativeQueuePolicy {
    None,
    BeginExternal,
    ClearExternal,
    EndExternal {
        disposition: QueueDisposition,
        reason: KnownStopReason,
    },
    Settle {
        active: ActiveKey,
        disposition: QueueDisposition,
        reason: KnownStopReason,
    },
}
type RetirementWait =
    super::Pin<Box<dyn super::Future<Output = Result<(), DispatchError>> + Send + 'static>>;
pub trait NativeQueueLink: Send + Sync {
    fn pending_retirement(&self) -> Result<Option<RetirementWait>, DispatchError>;
    /// # Errors
    /// Rejects unmatched original owner/resource closure and an in-flight R3 management transaction.
    fn retire_closed_resource(
        &self,
        proof: &crate::application::native_dispatch::RetiredResourceProof,
    ) -> Result<(), DispatchError>;
    fn priority_current(&self, handoff: &PriorityHandoff) -> Result<(), DispatchError>;
    fn priority_with_current(
        &self,
        handoff: &PriorityHandoff,
        action: &mut dyn FnMut() -> Result<(), DispatchError>,
    ) -> Result<(), DispatchError>;
    fn apply(
        &self,
        resource: ResourceEpoch,
        update: &mut dyn FnMut(u64) -> Result<NativeQueuePolicy, DispatchError>,
    ) -> Result<Option<ExternalWorkKey>, DispatchError>;
}
struct ObservationOwner<T> {
    core: Arc<RuntimeCore<T>>,
    owner: Arc<OwnerCell<T>>,
}
pub(super) fn observation_link<T: Send + 'static>(
    core: Arc<RuntimeCore<T>>,
    owner: Arc<OwnerCell<T>>,
) -> Arc<dyn NativeQueueLink> {
    Arc::new(ObservationOwner { core, owner })
}
impl<T: Send + 'static> NativeQueueLink for ObservationOwner<T> {
    fn pending_retirement(&self) -> Result<Option<RetirementWait>, DispatchError> {
        if lock(&self.owner.state)?.pending.is_none() {
            return Ok(None);
        }
        let owner = self.owner.clone();
        Ok(Some(Box::pin(async move {
            loop {
                let changed = owner.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                let pending = { lock(&owner.state)?.pending.is_some() };
                if !pending {
                    return Ok(());
                }
                changed.await;
            }
        })))
    }
    fn retire_closed_resource(
        &self,
        proof: &crate::application::native_dispatch::RetiredResourceProof,
    ) -> Result<(), DispatchError> {
        let (owner, resource) = proof.require_current()?;
        let mut state = lock(&self.owner.state)?;
        if owner != self.owner.identity.id || resource.runtime != self.owner.identity.runtime {
            return Err(DispatchError::InvalidResource);
        }
        if state.pending.is_some() {
            return Err(DispatchError::RequiredStateUnavailable);
        }
        if let Some((active, token)) = &state.active {
            if active.resource != resource {
                return Err(DispatchError::InvalidResource);
            }
            let item = active.item;
            #[cfg(windows)]
            let revision = active.revision;
            let token = token.clone();
            state
                .queue
                .settle(&token, QueueDisposition::Advance)
                .map_err(|_| DispatchError::StaleSelection)?;
            #[cfg(windows)]
            super::document_read::invalidate(&mut state, item, revision);
            state.active = None;
            state.origins.remove(&item);
        }
        drop(state);
        #[cfg(windows)]
        self.owner.notify_documents();
        self.owner.changed.notify_waiters();
        Ok(())
    }
    fn priority_with_current(
        &self,
        handoff: &PriorityHandoff,
        action: &mut dyn FnMut() -> Result<(), DispatchError>,
    ) -> Result<(), DispatchError> {
        let state = lock(&self.owner.state)?;
        if !Arc::ptr_eq(&self.owner.identity, &handoff.identity)
            || state.selected_priority != Some((handoff.operation, handoff.item, handoff.revision))
            || !state.queue.priority_matches(handoff.item, handoff.revision)
            || (handoff.resource.is_some()
                && self.core.environment.resource_epoch(&self.owner.native) != handoff.resource)
        {
            return Err(DispatchError::StaleSelection);
        }
        let result = action();
        drop(state);
        result
    }
    fn priority_current(&self, handoff: &PriorityHandoff) -> Result<(), DispatchError> {
        let state = lock(&self.owner.state)?;
        let identity_matches = Arc::ptr_eq(&self.owner.identity, &handoff.identity);
        let selected_matches =
            state.selected_priority == Some((handoff.operation, handoff.item, handoff.revision));
        let priority_matches = state.queue.priority_matches(handoff.item, handoff.revision);
        let resource_matches = handoff.resource.is_none()
            || self.core.environment.resource_epoch(&self.owner.native) == handoff.resource;
        if identity_matches && selected_matches && priority_matches && resource_matches {
            Ok(())
        } else {
            Err(DispatchError::StaleSelection)
        }
    }
    fn apply(
        &self,
        resource: ResourceEpoch,
        update: &mut dyn FnMut(u64) -> Result<NativeQueuePolicy, DispatchError>,
    ) -> Result<Option<ExternalWorkKey>, DispatchError> {
        let mut state = lock(&self.owner.state)?;
        if resource.runtime != self.owner.identity.runtime
            || self
                .core
                .environment
                .resource_epoch(&self.owner.native)
                .is_some_and(|epoch| epoch != resource)
        {
            return Err(DispatchError::InvalidResource);
        }
        // The callback takes only the corresponding native-resource short lock, then releases it.
        let order = state
            .native_order
            .checked_add(1)
            .ok_or(DispatchError::SequenceExhausted)?;
        state.native_order = order;
        let action = update(order)?;
        match action {
            NativeQueuePolicy::None => {}
            NativeQueuePolicy::ClearExternal => {
                if state
                    .external
                    .as_ref()
                    .is_some_and(|work| work.resource == resource)
                {
                    state.external = None;
                }
            }
            NativeQueuePolicy::BeginExternal => {
                if state.external.is_none() {
                    let serial = state
                        .external_serial
                        .checked_add(1)
                        .ok_or(DispatchError::SequenceExhausted)?;
                    state.external_serial = serial;
                    state.external = Some(ExternalWorkKey {
                        identity: self.owner.identity.clone(),
                        resource,
                        serial,
                    });
                }
            }
            NativeQueuePolicy::EndExternal {
                disposition,
                reason,
            } => {
                if disposition == QueueDisposition::Suspend {
                    // v2 can persist the live terminal before the external marker is
                    // projected into this owner. The terminal transition itself is the
                    // reliable pause evidence, while the marker only proves which
                    // resource may be cleared.
                    suspend_native_waiting(&mut state, reason);
                }
                if state
                    .external
                    .as_ref()
                    .is_some_and(|work| work.resource == resource)
                {
                    state.external = None;
                }
            }
            NativeQueuePolicy::Settle {
                active,
                disposition,
                reason,
            } => {
                let Some((current, token)) = state.active.as_ref() else {
                    return Err(DispatchError::StaleSelection);
                };
                if !current.same(&active) {
                    return Err(DispatchError::StaleSelection);
                }
                let token = token.clone();
                state
                    .queue
                    .settle(&token, QueueDisposition::Advance)
                    .map_err(|_| DispatchError::StaleSelection)?;
                if disposition == QueueDisposition::Suspend {
                    suspend_native_waiting(&mut state, reason);
                }
                #[cfg(windows)]
                super::document_read::invalidate(&mut state, active.item, active.revision);
                state.active = None;
                state.origins.remove(&active.item);
            }
        }
        let external = state.external.clone();
        drop(state);
        #[cfg(windows)]
        self.owner.notify_documents();
        self.owner.changed.notify_waiters();
        Ok(external)
    }
}

impl<T: Send + 'static> NativeQueueDriver<T> {
    pub(crate) fn target_for_native(
        &self,
        native: &NativeSessionKey,
    ) -> Result<Option<EntityId>, DispatchError> {
        let owners = self
            .core
            .owners
            .lock()
            .map_err(|_| DispatchError::Stopped)?;
        let Some(owner) = owners.get(native) else {
            return Ok(None);
        };
        let result = lock(&owner.state)?.canonical_target;
        drop(owners);
        Ok(result)
    }

    pub(crate) async fn fail_priority_for_native(
        &self,
        native: &NativeSessionKey,
        failure: PriorityFailure,
    ) -> Result<(), DispatchError> {
        let Some(target) = self.target_for_native(native)? else {
            return Ok(());
        };
        let owner = self.core.owner(target).await?;
        let failed = {
            let mut state = lock(&owner.state)?;
            state.fail_selected_priority(failure)
        };
        if let Some(intent) = failed {
            self.core.environment.priority_failed(native, intent);
            owner.changed.notify_waiters();
        }
        Ok(())
    }
}

fn suspend_native_waiting<T>(state: &mut OwnerState<T>, reason: KnownStopReason) {
    if let Some((_, item, revision)) = state.selected_priority {
        state
            .queue
            .suspend_except_priority_with_reason(item, revision, reason);
    } else {
        state.queue.suspend_waiting_with_reason(reason);
    }
}
