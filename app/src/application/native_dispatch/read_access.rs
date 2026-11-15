//! Attachment ports hold only weak links to the original native inventories.
#![allow(
    clippy::significant_drop_in_scrutinee,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    Arc, BoxFuture, ConfigurationPort, NativeControlRepository, NativeCoordinator,
    NativeDispatchRepository, NativeResourceRepository, NativeSessionKey, Ordering,
    OwnedNativeSource, SourceLaunchPort,
    read_source::{NativeReplayBinding, SourceReadCapability, wait_read_binding},
    runtime::SourceSlot,
    tasks::TaskSlot,
};
use crate::{application::shared_views::budget::ReadReason, domain::shared_view::ViewError};
use std::sync::Weak;

pub struct ReadWriteIdentity<S> {
    pub(crate) scope: S,
    pub(crate) rpc: super::RpcScalar,
    pub(crate) native: NativeSessionKey,
}
pub trait ReadWriteBoundary<S>: Send + Sync {
    fn check_and_poll(
        &self,
        identity: &ReadWriteIdentity<S>,
        step: super::DispatchWriteStep,
        cx: &mut std::task::Context<'_>,
        io: &mut dyn super::DispatchPollIo,
    ) -> Result<(), super::DispatchFenceRejection>;
}
pub(super) struct OriginalReadBoundary<S> {
    pub(super) scope: S,
    pub(super) rpc: super::RpcScalar,
    pub(super) native: NativeSessionKey,
    pub(super) source: Arc<super::PreSourceProbe>,
    pub(super) resource: Option<Arc<super::PreResourceProbe>>,
}
impl<S: Clone + Eq + Send + Sync> ReadWriteBoundary<S> for OriginalReadBoundary<S> {
    fn check_and_poll(
        &self,
        identity: &ReadWriteIdentity<S>,
        _step: super::DispatchWriteStep,
        cx: &mut std::task::Context<'_>,
        io: &mut dyn super::DispatchPollIo,
    ) -> Result<(), super::DispatchFenceRejection> {
        let _gate = self
            .source
            .authority
            .gate
            .lock()
            .map_err(|_| super::DispatchFenceRejection::RequiredStateUnavailable)?;
        if identity.scope != self.scope
            || identity.rpc != self.rpc
            || identity.native != self.native
        {
            return Err(super::DispatchFenceRejection::StaleTarget);
        }
        if self.source.authority.closed.load(Ordering::Acquire)
            || self.source.authority.closing.load(Ordering::Acquire)
            || self.resource.as_ref().is_some_and(|resource| {
                resource.authority.closed.load(Ordering::Acquire)
                    || resource.authority.releasing.load(Ordering::Acquire)
            })
        {
            return Err(super::DispatchFenceRejection::Stopped);
        }
        let _ = io.poll(cx);
        Ok(())
    }
}

pub trait NativeReadResourceAccess<P: crate::application::native_dispatch::NativeApprovalPort>:
    Send + Sync
{
    fn acquire_existing<'a>(
        &'a self,
        target: &'a NativeSessionKey,
        reason: ReadReason,
    ) -> BoxFuture<'a, Result<NativeReplayBinding<P>, ViewError>>;
}
pub struct OwnedReadTicketJob<P: crate::application::native_dispatch::NativeApprovalPort> {
    pub(crate) capability: Arc<SourceReadCapability<P>>,
    pub(crate) work: BoxFuture<'static, ()>,
    pub(crate) on_failure: Box<dyn FnOnce() + Send>,
}
pub struct ReadTicketTaskReceipt {
    _slot: Weak<TaskSlot>,
}
pub struct ReadTicketAdoptionError<P: crate::application::native_dispatch::NativeApprovalPort> {
    pub(crate) error: ViewError,
    pub(crate) job: OwnedReadTicketJob<P>,
}
pub trait NativeReadTaskRegistrar<P: crate::application::native_dispatch::NativeApprovalPort>:
    Send + Sync
{
    fn adopt(
        &self,
        job: OwnedReadTicketJob<P>,
    ) -> Result<ReadTicketTaskReceipt, ReadTicketAdoptionError<P>>;
}
pub struct ViewSourceContext<P: crate::application::native_dispatch::NativeApprovalPort> {
    pub(crate) source: P::Source,
    pub(crate) pre: Arc<super::PreSourceProbe>,
    pub(crate) resources: Arc<dyn NativeReadResourceAccess<P>>,
    pub(crate) registrar: Arc<dyn NativeReadTaskRegistrar<P>>,
}
struct OriginalReadAccess<P, L, G, N>
where
    P: crate::application::native_dispatch::NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + crate::application::native_dispatch::NativeApprovalRepository,
{
    core: Weak<NativeCoordinator<P, L, G, N>>,
    source: Weak<SourceSlot<P, L>>,
    scope: P::Scope,
}
impl<P, L, G, N> NativeReadResourceAccess<P> for OriginalReadAccess<P, L, G, N>
where
    P: crate::application::native_dispatch::NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + crate::application::native_dispatch::NativeApprovalRepository,
{
    fn acquire_existing<'a>(
        &'a self,
        target: &'a NativeSessionKey,
        _reason: ReadReason,
    ) -> BoxFuture<'a, Result<NativeReplayBinding<P>, ViewError>> {
        // No upgraded core/source or lock guard enters the returned future.
        let capability = (|| {
            let core = self.core.upgrade().ok_or(ViewError::NodeStopping)?;
            core.running().map_err(|_| ViewError::NodeStopping)?;
            let source = self.source.upgrade().ok_or(ViewError::CaptureUnavailable)?;
            if target.source().0 != source.pre.metadata.source_id {
                return Err(ViewError::InvalidScope);
            }
            let capability = if let Some(capability) = source
                .targets
                .lock()
                .map_err(|_| ViewError::ReadFailure)?
                .get(target)
                .and_then(|resource| resource.read_capability.get())
                .cloned()
            {
                capability
            } else {
                source
                    .external_reads
                    .lock()
                    .map_err(|_| ViewError::ReadFailure)?
                    .get(target)
                    .cloned()
                    .ok_or(ViewError::InvalidTarget)?
            };
            if capability.scope != self.scope || !Arc::ptr_eq(&capability.pre_source, &source.pre) {
                return Err(ViewError::InvalidScope);
            }
            capability.require_current()?;
            Ok(capability)
        })();
        Box::pin(async move { wait_read_binding(capability?).await })
    }
}
impl<P, L, G, N> NativeReadTaskRegistrar<P> for OriginalReadAccess<P, L, G, N>
where
    P: crate::application::native_dispatch::NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + crate::application::native_dispatch::NativeApprovalRepository,
{
    fn adopt(
        &self,
        job: OwnedReadTicketJob<P>,
    ) -> Result<ReadTicketTaskReceipt, ReadTicketAdoptionError<P>> {
        let Some(core) = self.core.upgrade() else {
            return Err(ReadTicketAdoptionError {
                error: ViewError::NodeStopping,
                job,
            });
        };
        if core.running().is_err() {
            return Err(ReadTicketAdoptionError {
                error: ViewError::NodeStopping,
                job,
            });
        }
        drop(core);
        let Some(source) = self.source.upgrade() else {
            return Err(ReadTicketAdoptionError {
                error: ViewError::CaptureUnavailable,
                job,
            });
        };
        let Ok(_gate) = source.pre.authority.gate.lock() else {
            return Err(ReadTicketAdoptionError {
                error: ViewError::ReadFailure,
                job,
            });
        };
        if source.pre.authority.closed.load(Ordering::Acquire)
            || source.pre.authority.closing.load(Ordering::Acquire)
        {
            return Err(ReadTicketAdoptionError {
                error: ViewError::SourceReleased,
                job,
            });
        }
        if !Arc::ptr_eq(&job.capability.pre_source, &source.pre)
            || job.capability.scope != self.scope
        {
            return Err(ReadTicketAdoptionError {
                error: ViewError::InvalidScope,
                job,
            });
        }
        if let Err(error) = job.capability.require_current() {
            return Err(ReadTicketAdoptionError { error, job });
        }
        let OwnedReadTicketJob {
            capability,
            work,
            on_failure,
        } = job;
        let original_capability = Arc::clone(&capability);
        let work: BoxFuture<'static, ()> = Box::pin(async move {
            work.await;
            // Original Ticket work retains its real connection/resource authority
            // independently of the caller and the attachment's weak inventory links.
            drop(original_capability);
        });
        match source.jobs.spawn_guarded_recover(work, on_failure) {
            Ok(slot) => Ok(ReadTicketTaskReceipt {
                _slot: Arc::downgrade(&slot),
            }),
            Err((_, work, on_failure)) => Err(ReadTicketAdoptionError {
                error: ViewError::QueueRejected,
                job: OwnedReadTicketJob {
                    capability,
                    work,
                    on_failure,
                },
            }),
        }
    }
}
impl<P, L, G, N> NativeCoordinator<P, L, G, N>
where
    P: crate::application::native_dispatch::NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + crate::application::native_dispatch::NativeApprovalRepository,
{
    pub(super) fn view_context(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        connection: &P::Source,
    ) -> ViewSourceContext<P> {
        let access = Arc::new(OriginalReadAccess {
            core: Arc::downgrade(self),
            source: Arc::downgrade(source),
            scope: self.protocol.source_scope(connection),
        });
        ViewSourceContext {
            source: connection.clone(),
            pre: Arc::clone(&source.pre),
            resources: access.clone(),
            registrar: access,
        }
    }
}
