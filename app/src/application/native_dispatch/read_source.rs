//! Read installation stays in the original runtime task registry after a caller leaves.
use super::{
    Arc, DispatchError, EntityId, NativeControlRepository, NativeDispatchRepository,
    NativeResourceRepository, NativeRuntimeHandle, Ordering, OwnedNativeSource, PreResourceProbe,
    PreSourceProbe, ProtocolVersion, ReadConfigurationPort, SourceLaunchPort,
    SourceReadSelectionProof,
    runtime::{LifecycleOperation, LifecycleResponseValue},
};
use crate::{
    application::{queue_owner::ReadTargetProof, shared_views::budget::ReadReason},
    domain::shared_view::ViewError,
};
use std::sync::{Mutex, OnceLock};
use tokio::sync::{Notify, watch};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadActivationSnapshot {
    #[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
    AwaitSource,
    Resuming,
    Ready,
    Failed(DispatchError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadTargetKind {
    Target,
    SourceOnly,
}

/// A current route on the original resource, independent of reliable Loaded metadata.
pub enum SourceReadCapability<P: crate::application::native_dispatch::NativeApprovalPort> {
    RootResource(RootReadCapability<P>),
    ChildAutoDelivered(ChildReadCapability<P>),
}
pub struct ChildReadCapability<P: crate::application::native_dispatch::NativeApprovalPort> {
    view_subject: super::NativeSessionKey,
    owner_resource: Arc<SourceReadCapability<P>>,
}
pub struct RootReadCapability<P: crate::application::native_dispatch::NativeApprovalPort> {
    pub(super) kind: ReadTargetKind,
    pub(super) source: P::Source,
    pub(super) pre_source: Arc<PreSourceProbe>,
    pub(super) pre_resource: Option<Arc<PreResourceProbe>>,
    pub(super) capacity: Arc<super::capacity::CapacityResource<P>>,
    pub(super) capacity_admission: Arc<dyn super::capacity::ReadCapacityAdmission>,
    pub(super) native: super::NativeSessionKey,
    pub(super) scope: P::Scope,
    pub(super) binding: EntityId,
    pub(super) wire: ProtocolVersion,
    pub(super) setup: Arc<P::BoundSetup>,
    pub(super) resume: OnceLock<Arc<LifecycleOperation<P>>>,
    pub(super) activation_lock: tokio::sync::Mutex<()>,
    pub(super) changed: Arc<Notify>,
    pub(super) view_handoff: std::sync::Weak<super::view_attachment::ViewAttachmentHandoff>,
}
impl<P: crate::application::native_dispatch::NativeApprovalPort> std::ops::Deref
    for SourceReadCapability<P>
{
    type Target = RootReadCapability<P>;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::RootResource(root) => root,
            Self::ChildAutoDelivered(child) => &child.owner_resource,
        }
    }
}
impl<P: crate::application::native_dispatch::NativeApprovalPort> SourceReadCapability<P> {
    pub(crate) const fn is_child_auto_delivered(&self) -> bool {
        matches!(self, Self::ChildAutoDelivered(_))
    }
    pub(crate) async fn reserve_replay_capacity(&self) -> Result<(), ViewError> {
        self.require_current()?;
        if self.is_child_auto_delivered() {
            return Err(ViewError::InvalidTarget);
        }
        self.capacity_admission.reserve().await?;
        let resource = self.pre_resource.as_ref().ok_or(ViewError::InvalidTarget)?;
        super::capacity::restore_display(&self.pre_source, resource, || {
            if let Some(view) = self.view_handoff.upgrade() {
                view.restore_target(&self.native)?;
            }
            Ok(())
        })
    }
    pub(crate) fn is_source_only(&self) -> bool {
        matches!(self.kind, ReadTargetKind::SourceOnly)
    }
    pub(crate) fn view_failure(&self) -> Option<ViewError> {
        self.view_handoff
            .upgrade()
            .and_then(|handoff| handoff.unavailable_reason())
    }
    pub(crate) fn source(&self) -> &P::Source {
        &self.source
    }
    pub(crate) fn scope(&self) -> &P::Scope {
        &self.scope
    }
    pub(crate) const fn native(&self) -> &super::NativeSessionKey {
        match self {
            Self::RootResource(root) => &root.native,
            Self::ChildAutoDelivered(child) => &child.view_subject,
        }
    }
    pub(crate) fn setup(&self) -> &P::BoundSetup {
        &self.setup
    }
    pub(crate) fn configuration_binding(&self) -> (EntityId, ProtocolVersion) {
        (self.binding, self.wire)
    }
    pub(crate) fn write_boundary(
        &self,
        rpc: super::RpcScalar,
    ) -> Arc<dyn super::read_access::ReadWriteBoundary<P::Scope>> {
        Arc::new(super::read_access::OriginalReadBoundary {
            scope: self.scope.clone(),
            rpc,
            native: self.native.clone(),
            source: self.pre_source.clone(),
            resource: self.pre_resource.clone(),
        })
    }
    pub(crate) fn require_current(&self) -> Result<(), ViewError> {
        if self.pre_source.authority.closed.load(Ordering::Acquire)
            || self.pre_source.authority.closing.load(Ordering::Acquire)
            || self.pre_resource.as_ref().is_some_and(|resource| {
                resource.authority.closed.load(Ordering::Acquire)
                    || resource.authority.releasing.load(Ordering::Acquire)
            })
        {
            return Err(ViewError::SourceReleased);
        }
        Ok(())
    }
    fn activation(&self) -> Result<ReadActivationSnapshot, ViewError> {
        self.require_current()?;
        let Some(resume) = self.resume.get() else {
            return Ok(ReadActivationSnapshot::Ready);
        };
        let response = resume.response.lock().map_err(|_| ViewError::ReadFailure)?;
        match response.as_ref().map(|response| &response.value) {
            Some(LifecycleResponseValue::Resumed(session))
                if session.as_ref() == self.native.native_id() =>
            {
                Ok(ReadActivationSnapshot::Ready)
            }
            Some(_) => Ok(ReadActivationSnapshot::Failed(
                DispatchError::ProtocolRejected,
            )),
            None => Ok(resume
                .read_failure
                .lock()
                .map_err(|_| ViewError::ReadFailure)?
                .map_or(
                    ReadActivationSnapshot::Resuming,
                    ReadActivationSnapshot::Failed,
                )),
        }
    }
}
pub struct NativeReplayBinding<P: crate::application::native_dispatch::NativeApprovalPort> {
    pub(super) capability: Arc<SourceReadCapability<P>>,
}
impl<P: crate::application::native_dispatch::NativeApprovalPort> NativeReplayBinding<P> {
    pub(crate) fn capability(&self) -> Arc<SourceReadCapability<P>> {
        Arc::clone(&self.capability)
    }
}
pub struct NativeReadAcquisition<P: crate::application::native_dispatch::NativeApprovalPort> {
    state: Arc<ReadAcquisitionState<P>>,
}
pub(super) struct ReadAcquisitionState<P: crate::application::native_dispatch::NativeApprovalPort> {
    capability: OnceLock<Arc<SourceReadCapability<P>>>,
    completed: Mutex<Option<Result<(), DispatchError>>>,
    changes: watch::Sender<()>,
}
impl<P: crate::application::native_dispatch::NativeApprovalPort> ReadAcquisitionState<P> {
    fn new() -> Self {
        let (changes, _) = watch::channel(());
        Self {
            capability: OnceLock::new(),
            completed: Mutex::new(None),
            changes,
        }
    }
    pub(super) fn publish(
        &self,
        capability: Arc<SourceReadCapability<P>>,
    ) -> Result<(), DispatchError> {
        if let Some(current) = self.capability.get() {
            if !Arc::ptr_eq(current, &capability) {
                return Err(DispatchError::Conflict);
            }
        } else {
            self.capability
                .set(capability)
                .map_err(|_| DispatchError::Conflict)?;
        }
        self.changes.send_replace(());
        Ok(())
    }
    fn finish(&self, result: Result<(), DispatchError>) {
        match self.completed.lock() {
            Ok(mut completed) => *completed = Some(result),
            Err(error) => *error.into_inner() = Some(Err(DispatchError::ResourceUnconfirmed)),
        }
        self.changes.send_replace(());
    }
}
impl<P: crate::application::native_dispatch::NativeApprovalPort> NativeReadAcquisition<P> {
    #[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
    pub(crate) fn activation(&self) -> Result<ReadActivationSnapshot, ViewError> {
        if let Some(capability) = self.state.capability.get() {
            let activation = capability.activation()?;
            if activation == ReadActivationSnapshot::Ready {
                return Ok(activation);
            }
        }
        let completed = *self
            .state
            .completed
            .lock()
            .map_err(|_| ViewError::ReadFailure)?;
        if let Some(Err(error)) = completed {
            return Ok(ReadActivationSnapshot::Failed(error));
        }
        Ok(if self.state.capability.get().is_some() {
            ReadActivationSnapshot::Resuming
        } else {
            ReadActivationSnapshot::AwaitSource
        })
    }
    pub(crate) async fn source_capability(
        &self,
    ) -> Result<Arc<SourceReadCapability<P>>, ViewError> {
        let mut changes = self.state.changes.subscribe();
        loop {
            if let Some(capability) = self.state.capability.get() {
                capability.require_current()?;
                return Ok(Arc::clone(capability));
            }
            let completed = *self
                .state
                .completed
                .lock()
                .map_err(|_| ViewError::ReadFailure)?;
            if let Some(result) = completed {
                return Err(match result {
                    Err(error) => ViewError::NativeFailure(error),
                    Ok(()) => ViewError::CaptureUnavailable,
                });
            }
            changes
                .changed()
                .await
                .map_err(|_| ViewError::ReadFailure)?;
        }
    }
    #[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
    pub(crate) async fn replay_binding(&self) -> Result<NativeReplayBinding<P>, ViewError> {
        let capability = self.source_capability().await?;
        wait_read_binding(capability).await
    }
}
pub(super) async fn wait_read_binding<
    P: crate::application::native_dispatch::NativeApprovalPort,
>(
    capability: Arc<SourceReadCapability<P>>,
) -> Result<NativeReplayBinding<P>, ViewError> {
    let Some(resume) = capability.resume.get() else {
        capability.require_current()?;
        return Ok(NativeReplayBinding { capability });
    };
    loop {
        let response = resume.changed.notified();
        let stopped = capability.changed.notified();
        tokio::pin!(response, stopped);
        response.as_mut().enable();
        stopped.as_mut().enable();
        match capability.activation()? {
            ReadActivationSnapshot::Ready => {
                return Ok(NativeReplayBinding {
                    capability: Arc::clone(&capability),
                });
            }
            ReadActivationSnapshot::Failed(error) => return Err(ViewError::NativeFailure(error)),
            #[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
            ReadActivationSnapshot::AwaitSource => {}
            ReadActivationSnapshot::Resuming => {}
        }
        tokio::select! {
            () = &mut response => {},
            () = &mut stopped => {},
        }
    }
}
impl<P, L, G, N> NativeRuntimeHandle<P, L, G, N>
where
    P: crate::application::native_dispatch::NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ReadConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + crate::application::native_dispatch::NativeApprovalRepository,
{
    pub(crate) fn install_view_factory(
        &self,
        factory: Arc<dyn super::view_attachment::NativeSourceViewFactory<P>>,
    ) -> Result<(), ViewError> {
        let sources = self
            .core
            .sources
            .lock()
            .map_err(|_| ViewError::ReadFailure)?;
        if !sources.is_empty() || self.core.status.load(Ordering::Acquire) > 1 {
            return Err(ViewError::AlreadyAttached);
        }
        self.core
            .view_factory
            .set(factory)
            .map_err(|_| ViewError::AlreadyAttached)?;
        drop(sources);
        Ok(())
    }
    pub(crate) fn acquire_read_source(
        &self,
        proof: ReadTargetProof,
        _reason: ReadReason,
    ) -> Result<NativeReadAcquisition<P>, DispatchError> {
        self.core.running()?;
        self.core.queue.verify_read_target(&proof)?;
        let state = Arc::new(ReadAcquisitionState::new());
        let completed = Arc::clone(&state);
        let failed = Arc::clone(&state);
        let core = Arc::clone(&self.core);
        self.core.tasks.spawn_guarded(
            async move {
                let result = core
                    .ensure_read_resource(proof, Arc::clone(&completed))
                    .await;
                completed.finish(result);
            },
            move || failed.finish(Err(DispatchError::RequiredStateUnavailable)),
        )?;
        Ok(NativeReadAcquisition { state })
    }
    pub(crate) fn acquire_source_read(
        &self,
        proof: SourceReadSelectionProof,
        _reason: ReadReason,
    ) -> Result<NativeReadAcquisition<P>, DispatchError> {
        self.core.running()?;
        proof.require_current()?;
        let state = Arc::new(ReadAcquisitionState::new());
        let completed = Arc::clone(&state);
        let failed = Arc::clone(&state);
        let core = Arc::clone(&self.core);
        self.core.tasks.spawn_guarded(
            async move {
                let result = core.ensure_source_read(proof, Arc::clone(&completed)).await;
                completed.finish(result.map(|_| ()));
            },
            move || failed.finish(Err(DispatchError::RequiredStateUnavailable)),
        )?;
        Ok(NativeReadAcquisition { state })
    }
}

#[cfg(test)]
pub(super) mod cause_tests;

mod child;
