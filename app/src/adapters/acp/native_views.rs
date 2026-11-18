//! Display attachment for the original native observation pump; no receiver or IO owner.
use super::{
    ConnectionScope, Received, Reservation,
    native_dispatch::{AcpNativeAdapter, AcpSource},
    shared_view_capture::{AcpViewCapture, RetainedAcpRaw},
    shared_view_projection::AcpViewProjector,
};
use crate::{
    application::{
        native_dispatch::{
            AttachmentBuildGuard, BoxFuture, NativeReadResourceAccess, NativeReadTaskRegistrar,
            NativeSourceViewFactory, NativeSourceViewOwner, PreSourceProbe, ViewAttachmentClose,
            ViewSourceContext,
        },
        shared_views::{SharedViewOwner, SharedViews},
    },
    domain::{
        native::{NativeSessionKey, NativeSourceId},
        shared_view::ViewError,
    },
};
use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};
mod replay;
pub use replay::admit_replay_capacity;
pub type AcpReadService = crate::application::shared_views::reader::ViewReadService<
    ConnectionScope,
    RetainedAcpRaw,
    crate::application::shared_views::local_authority::LocalViewAuthority,
    crate::application::shared_views::local_authority::AuthenticatedLocalViewClient,
>;

pub type AcpSocketReadService = crate::application::shared_views::reader::ViewReadService<
    ConnectionScope,
    RetainedAcpRaw,
    crate::application::shared_views::socket_authority::SocketViewAuthority,
    crate::application::shared_views::socket_authority::SocketViewClient,
>;
pub struct AcpSourceViews {
    source: AcpSource,
    pre: Arc<PreSourceProbe>,
    resources: Arc<dyn NativeReadResourceAccess<AcpNativeAdapter>>,
    registrar: Arc<dyn NativeReadTaskRegistrar<AcpNativeAdapter>>,
    capture: Mutex<AcpViewCapture>,
    owner: Mutex<Option<Arc<SharedViewOwner<ConnectionScope, RetainedAcpRaw>>>>,
    operations: Mutex<Vec<std::sync::Weak<replay::ReplayOperation>>>,
    reads: std::sync::OnceLock<
        Arc<
            crate::application::shared_views::read_control::NativeReadCoordinator<
                replay::NativeHistoryBridge,
            >,
        >,
    >,
    reader: std::sync::OnceLock<Arc<AcpReadService>>,
    socket_reader: std::sync::OnceLock<Arc<AcpSocketReadService>>,
}
pub struct AcpNativeViewFactory {
    socket_authority:
        Option<Arc<crate::application::shared_views::socket_authority::SocketViewAuthority>>,
    yield_every: NonZeroUsize,
    read_options: Arc<dyn crate::application::shared_views::budget::NativeReadOptionsProvider>,
    authority: Option<Arc<crate::application::shared_views::local_authority::LocalViewAuthority>>,
}
impl AcpNativeViewFactory {
    pub(crate) fn with_socket_authority(
        mut self,
        authority: Arc<crate::application::shared_views::socket_authority::SocketViewAuthority>,
    ) -> Self {
        self.socket_authority = Some(authority);
        self
    }
    pub(crate) const fn new(
        yield_every: NonZeroUsize,
        read_options: Arc<dyn crate::application::shared_views::budget::NativeReadOptionsProvider>,
        authority: Arc<crate::application::shared_views::local_authority::LocalViewAuthority>,
    ) -> Self {
        Self {
            yield_every,
            socket_authority: None,
            read_options,
            authority: Some(authority),
        }
    }
    #[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
    pub(crate) const fn replay_only_for_test(
        yield_every: NonZeroUsize,
        read_options: Arc<dyn crate::application::shared_views::budget::NativeReadOptionsProvider>,
    ) -> Self {
        Self {
            yield_every,
            socket_authority: None,
            read_options,
            authority: None,
        }
    }
}
impl NativeSourceViewFactory<AcpNativeAdapter> for AcpNativeViewFactory {
    fn attach(
        &self,
        context: ViewSourceContext<AcpNativeAdapter>,
        guard: AttachmentBuildGuard,
    ) -> Result<(), ViewError> {
        let connection = context
            .source
            .connection()
            .map_err(|_| ViewError::CaptureUnavailable)?;
        let (capture, consumer, probe) = AcpViewCapture::for_connection(&connection);
        let attached = Arc::new(AcpSourceViews {
            source: context.source,
            pre: context.pre,
            resources: context.resources,
            registrar: context.registrar,
            capture: Mutex::new(capture),
            owner: Mutex::new(None),
            operations: Mutex::new(Vec::new()),
            reads: std::sync::OnceLock::new(),
            reader: std::sync::OnceLock::new(),
            socket_reader: std::sync::OnceLock::new(),
        });
        // This stable partial owner is in H before the first worker is started.
        guard.adopt_partial(attached.clone())?;
        let (owner, worker) = SharedViewOwner::prepare_source(
            connection.scope(),
            NativeSourceId(attached.pre.metadata().source_id),
            consumer,
            probe,
            Arc::new(AcpViewProjector::new(connection)),
            self.yield_every,
            tokio::runtime::Handle::try_current().map_err(|_| ViewError::ReadFailure)?,
        )?;
        let owner = Arc::new(owner);
        owner
            .supervisor
            .install_target_owner(Arc::new(NativeViewTaskOwnership {
                resources: attached.resources.clone(),
                registrar: attached.registrar.clone(),
            }))?;
        *attached.owner.lock().map_err(|_| ViewError::ReadFailure)? = Some(owner.clone());
        attached
            .capture
            .lock()
            .map_err(|_| ViewError::ReadFailure)?
            .install_source_seal(&owner.handle())?;
        #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
        attached
            .capture
            .lock()
            .map_err(|_| ViewError::ReadFailure)?
            .install_controller(owner.test_controller())?;
        let reads = crate::application::shared_views::read_control::NativeReadCoordinator::new(
            Arc::new(replay::NativeHistoryBridge {
                attachment: Arc::downgrade(&attached),
            }),
            owner.handle(),
            owner.supervisor.clone(),
            Arc::clone(&self.read_options),
        )?;
        if let Some(authority) = &self.authority {
            let mut reader = AcpReadService::new(
                owner.handle(),
                Arc::clone(authority),
                owner.supervisor.clone(),
            );
            reader.set_read_control(reads.clone());
            attached
                .reader
                .set(Arc::new(reader))
                .map_err(|_| ViewError::AlreadyAttached)?;
        }
        if let Some(authority) = &self.socket_authority {
            let mut reader = AcpSocketReadService::new(
                owner.handle(),
                authority.clone(),
                owner.supervisor.clone(),
            );
            reader.set_read_control(reads.clone());
            attached
                .socket_reader
                .set(Arc::new(reader))
                .map_err(|_| ViewError::AlreadyAttached)?;
        }
        attached
            .reads
            .set(reads)
            .map_err(|_| ViewError::AlreadyAttached)?;
        worker.start(&owner)?;
        attached.source.install_views(&attached)?;
        guard.publish()
    }
}
struct NativeViewTaskOwnership {
    resources: Arc<dyn NativeReadResourceAccess<AcpNativeAdapter>>,
    registrar: Arc<dyn NativeReadTaskRegistrar<AcpNativeAdapter>>,
}
impl crate::application::shared_views::tasks::TargetTaskOwnership for NativeViewTaskOwnership {
    fn adopt(
        &self,
        target: &NativeSessionKey,
        generation: crate::application::shared_views::send_epoch::OwnedDisplaySendView,
        joined: crate::application::shared_views::BoxReadFuture<'static, Result<(), ViewError>>,
    ) -> Result<(), ViewError> {
        use crate::application::native_dispatch::read_access::{
            OwnedReadTicketJob, ReadTicketTaskReceipt,
        };
        let capability = self.resources.current_capability(target)?;
        let (started, receipt) = tokio::sync::oneshot::channel::<ReadTicketTaskReceipt>();
        let job = OwnedReadTicketJob {
            capability,
            view_generation: Some(generation),
            work: Box::pin(async move {
                if let Ok(receipt) = receipt.await
                    && joined.await.is_err()
                {
                    receipt.mark_failed();
                }
            }),
            on_failure: Box::new(|| {}),
        };
        let receipt = self
            .registrar
            .adopt(job)
            .map_err(|rejected| rejected.error)?;
        started.send(receipt).map_err(|_| ViewError::ReadFailure)
    }
}
impl AcpSourceViews {
    pub(crate) fn ensure_external_target(&self, target: NativeSessionKey) -> Result<(), ViewError> {
        if target.source().0 != self.pre.metadata().source_id {
            return Err(ViewError::InvalidScope);
        }
        self.capture
            .lock()
            .map_err(|_| ViewError::ReadFailure)?
            .ensure_external_route(target)?;
        if let Ok(views) = self.views() {
            views.core.changed.notify_waiters();
        }
        Ok(())
    }
    pub(crate) fn socket_reader(
        &self,
        capability: &crate::application::native_dispatch::SourceReadCapability<AcpNativeAdapter>,
        authority: &crate::application::shared_views::socket_authority::SocketViewAuthority,
    ) -> Result<Arc<AcpSocketReadService>, ViewError> {
        capability.require_current()?;
        let connection = self
            .source
            .connection()
            .map_err(|_| ViewError::SourceReleased)?;
        if *capability.scope() != connection.scope()
            || capability.native().source().0 != self.pre.metadata().source_id
        {
            return Err(ViewError::InvalidScope);
        }
        let reader = self
            .socket_reader
            .get()
            .cloned()
            .ok_or(ViewError::CaptureUnavailable)?;
        if !reader.uses_authority(authority) {
            return Err(ViewError::InvalidClient);
        }
        authority.register_native_source(capability)?;
        Ok(reader)
    }
    pub(crate) fn reader(
        &self,
        capability: &crate::application::native_dispatch::SourceReadCapability<AcpNativeAdapter>,
    ) -> Result<Arc<AcpReadService>, ViewError> {
        capability.require_current()?;
        let connection = self
            .source
            .connection()
            .map_err(|_| ViewError::SourceReleased)?;
        if *capability.scope() != connection.scope()
            || capability.native().source().0 != self.pre.metadata().source_id
        {
            return Err(ViewError::InvalidScope);
        }
        self.reader
            .get()
            .cloned()
            .ok_or(ViewError::CaptureUnavailable)
    }
    pub(super) fn receive(&self, received: &Received) -> Result<(), ViewError> {
        self.capture_replay_received(received)
    }
    #[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
    pub(crate) fn test_controller(
        &self,
    ) -> Result<
        crate::application::shared_views::controller::ViewTestController<ConnectionScope>,
        ViewError,
    > {
        let owner = self.owner.lock().map_err(|_| ViewError::ReadFailure)?;
        owner
            .as_ref()
            .map(|owner| owner.test_controller())
            .ok_or(ViewError::CaptureUnavailable)
    }
    #[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
    pub(crate) fn spawn_target_task_for_test(
        &self,
        target: &NativeSessionKey,
        work: crate::application::shared_views::BoxReadFuture<'static, Result<(), ViewError>>,
    ) -> Result<(), ViewError> {
        let owner = self
            .owner
            .lock()
            .map_err(|_| ViewError::ReadFailure)?
            .clone()
            .ok_or(ViewError::SourceReleased)?;
        let generation = owner.handle().core.target(target)?.send.clone();
        owner.supervisor.spawn_target(
            crate::application::shared_views::tasks::ViewTaskKind::Reader,
            target,
            generation,
            move |_| work,
        )
    }
    #[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
    pub(crate) fn read_control(
        &self,
    ) -> Result<Arc<dyn crate::application::shared_views::read_control::NativeReadControl>, ViewError>
    {
        self.reads
            .get()
            .cloned()
            .map(|reads| {
                reads as Arc<dyn crate::application::shared_views::read_control::NativeReadControl>
            })
            .ok_or(ViewError::CaptureUnavailable)
    }
    pub(super) fn bind_resume(
        &self,
        reservation: &Reservation,
        target: NativeSessionKey,
    ) -> Result<(), ViewError> {
        self.capture
            .lock()
            .map_err(|_| ViewError::ReadFailure)?
            .bind_module_reservation(reservation, target, None)?;
        Ok(())
    }
    pub(crate) fn views(&self) -> Result<SharedViews<ConnectionScope, RetainedAcpRaw>, ViewError> {
        let owner = self.owner.lock().map_err(|_| ViewError::ReadFailure)?;
        owner
            .as_ref()
            .map(|owner| owner.handle())
            .ok_or(ViewError::CaptureUnavailable)
    }
}
impl NativeSourceViewOwner for AcpSourceViews {
    fn bind_target(&self, target: &NativeSessionKey) -> Result<(), ViewError> {
        self.restore_target(target)?;
        self.ensure_external_target(target.clone())
    }
    fn restore_target(&self, target: &NativeSessionKey) -> Result<(), ViewError> {
        let views = self.views()?;
        if let Some(restored) = views.core.send.restore_target(target)? {
            let result = self
                .capture
                .lock()
                .map_err(|_| ViewError::ReadFailure)
                .and_then(|mut capture| capture.restore_external_route(target.clone(), restored));
            if let Err(error) = result {
                views.core.send.seal_target(target)?;
                return Err(error);
            }
        }
        Ok(())
    }
    fn seal_target(&self, target: &NativeSessionKey) -> Result<(), ViewError> {
        let views = self.views()?;
        let result = views.core.send.seal_target(target);
        views.core.changed.notify_waiters();
        result
    }
    fn seal(&self) -> Result<(), ViewError> {
        match self.capture.lock() {
            Ok(capture) => capture.seal(),
            Err(poison) => poison.into_inner().seal(),
        }
        Ok(())
    }
    fn join_stopped(&self) -> BoxFuture<'_, ViewAttachmentClose> {
        let owner = match self.owner.lock() {
            Ok(owner) => owner.clone(),
            Err(poison) => poison.into_inner().clone(),
        };
        Box::pin(async move {
            if let Some(owner) = owner {
                let report = owner.join_stopped().await;
                ViewAttachmentClose {
                    workers: report.workers,
                    readers: report.readers,
                    cleanup: report.cleanup,
                    native_reads: report.native_reads,
                    failure: report.failure,
                }
            } else {
                // prepare_source failed before any task existed.
                ViewAttachmentClose {
                    workers: 0,
                    readers: 0,
                    cleanup: 0,
                    native_reads: 0,
                    failure: None,
                }
            }
        })
    }
}

#[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
#[path = "runtime_child_task_kind_probe.rs"]
mod runtime_child_task_kind_probe;

#[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
#[path = "capacity_registration_race_probe.rs"]
mod capacity_registration_race_probe;
