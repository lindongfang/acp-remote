//! Composition under Storage's original runtime claim and physical `ConnectionOwner`.
use super::{Arc, AtomicBool, Duration, Ordering, Storage};
use crate::{
    application::{
        native_dispatch::{
            ConfigurationPort, NativeCoordinator, NativeEnvironment, NativeRuntimeHandle,
            NativeRuntimeOptions, OwnedNativeSource, RetainPrompt, SourceLaunchPort,
        },
        queue_owner::{QueueService, RuntimeSupervisor},
        request_registry::{AuditPort, NodeClock, RequestAuthority, ValidatedBodyPort, random_id},
    },
    domain::{native::RuntimeEpoch, native_dispatch::DispatchError},
    session_storage::{
        native_dispatch::SqliteNativeDispatch, request_registry::SqliteRequestRegistry,
    },
};
use std::{future::Future, pin::Pin};
type Service<A, V, C> = QueueService<A, V, SqliteRequestRegistry, C>;
struct CombinedSupervisor<A, V: ValidatedBodyPort, C, P, L, G>
where
    P: crate::application::native_dispatch::NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
{
    service: Service<A, V, C>,
    native: NativeRuntimeHandle<P, L, G, SqliteNativeDispatch>,
    claim: Arc<AtomicBool>,
    #[cfg(windows)]
    preparation: Arc<crate::application::prompt_preparation::PreparationShutdownSlot>,
    #[cfg(windows)]
    document: Arc<crate::application::document_disposal::DocumentShutdownSlot>,
    #[cfg(windows)]
    catalog: Arc<crate::application::catalog_preparation::CatalogShutdownSlot>,
}
impl<A, V, C, P, L, G> RuntimeSupervisor for CombinedSupervisor<A, V, C, P, L, G>
where
    A: RequestAuthority + 'static,
    V: ValidatedBodyPort + 'static,
    C: NodeClock + 'static,
    P: crate::application::native_dispatch::NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
{
    #[cfg(windows)]
    fn queue_runtime_identity(&self) -> crate::application::queue_owner::QueueRuntimeIdentity {
        self.service.queue_runtime_identity()
    }
    #[cfg(windows)]
    fn register_preparation(
        &self,
        expected: &crate::application::queue_owner::QueueRuntimeIdentity,
        install: &mut dyn FnMut() -> Result<(), crate::domain::request_registry::RegistryError>,
    ) -> Result<(), crate::domain::request_registry::RegistryError> {
        self.service.register_preparation(expected, install)
    }
    #[cfg(windows)]
    fn register_document(
        &self,
        expected: &crate::application::queue_owner::QueueRuntimeIdentity,
        install: &mut dyn FnMut() -> Result<(), crate::domain::request_registry::RegistryError>,
    ) -> Result<(), crate::domain::request_registry::RegistryError> {
        self.service.register_document(expected, install)
    }
    #[cfg(windows)]
    fn register_catalog(
        &self,
        expected: &crate::application::queue_owner::QueueRuntimeIdentity,
        install: &mut dyn FnMut() -> Result<(), crate::domain::request_registry::RegistryError>,
    ) -> Result<(), crate::domain::request_registry::RegistryError> {
        self.service.register_catalog(expected, install)
    }
    fn cancel(&self) {
        #[cfg(windows)]
        self.preparation.seal();
        #[cfg(windows)]
        self.document.seal();
        #[cfg(windows)]
        self.catalog.seal();
        self.native.core.request_shutdown();
        self.service.cancel();
    }
    fn stop_and_join(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            #[cfg(windows)]
            self.preparation.seal();
            #[cfg(windows)]
            self.document.seal();
            #[cfg(windows)]
            self.catalog.seal();
            let native_close = async {
                loop {
                    let progress = self.native.close_progress();
                    let report = self
                        .native
                        .shutdown(tokio::time::Instant::now() + Duration::from_secs(5))
                        .await;
                    if report.joined && report.metadata_confirmed {
                        break;
                    }
                    self.native.wait_close_progress(progress).await;
                }
            };
            #[cfg(windows)]
            {
                tokio::join!(
                    native_close,
                    self.preparation.join(),
                    self.document.join(),
                    self.catalog.join()
                );
            }
            #[cfg(not(windows))]
            native_close.await;
            self.service.stop_and_join().await;
            #[cfg(windows)]
            self.service.release_preparation_claim();
            self.claim.store(false, Ordering::SeqCst);
        })
    }
}
impl Storage {
    /// Starts native ownership with this Storage owner's SQL audit port.
    /// # Errors
    /// Preserves original composition and startup failure categories.
    #[expect(
        clippy::too_many_arguments,
        reason = "Keep the reviewed independent native ports explicit at composition."
    )]
    pub async fn start_native_service<A, V, C, P, L, G>(
        &self,
        authority: A,
        validator: V,
        clock: C,
        protocol: Arc<P>,
        launcher: Arc<L>,
        configuration: Arc<G>,
        retain: Arc<dyn RetainPrompt<V::Body, Retained = P::Retained>>,
        options: NativeRuntimeOptions,
    ) -> Result<
        (
            Service<A, V, C>,
            NativeRuntimeHandle<P, L, G, SqliteNativeDispatch>,
        ),
        DispatchError,
    >
    where
        A: RequestAuthority + 'static,
        V: ValidatedBodyPort + 'static,
        C: NodeClock + 'static,
        P: crate::application::native_dispatch::NativeApprovalPort,
        L: SourceLaunchPort,
        L::Owned: OwnedNativeSource<Streams = P::Streams>,
        G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    {
        self.start_native_service_composed(
            authority,
            validator,
            clock,
            self.management_audit_port(),
            protocol,
            launcher,
            configuration,
            retain,
            options,
        )
        .await
    }

    #[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
    #[expect(
        clippy::too_many_arguments,
        reason = "Keep the reviewed independent native ports explicit at composition."
    )]
    pub(crate) async fn start_native_service_with_audit_for_test<A, V, C, P, L, G>(
        &self,
        authority: A,
        validator: V,
        clock: C,
        audit: Arc<dyn AuditPort>,
        protocol: Arc<P>,
        launcher: Arc<L>,
        configuration: Arc<G>,
        retain: Arc<dyn RetainPrompt<V::Body, Retained = P::Retained>>,
        options: NativeRuntimeOptions,
    ) -> Result<
        (
            Service<A, V, C>,
            NativeRuntimeHandle<P, L, G, SqliteNativeDispatch>,
        ),
        DispatchError,
    >
    where
        A: RequestAuthority + 'static,
        V: ValidatedBodyPort + 'static,
        C: NodeClock + 'static,
        P: crate::application::native_dispatch::NativeApprovalPort,
        L: SourceLaunchPort,
        L::Owned: OwnedNativeSource<Streams = P::Streams>,
        G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    {
        self.start_native_service_composed(
            authority,
            validator,
            clock,
            audit,
            protocol,
            launcher,
            configuration,
            retain,
            options,
        )
        .await
    }

    /// Constructs the actual queue/SQLite/process/protocol combination before its first await.
    /// # Errors
    /// A second live runtime, invalid composition or unavailable startup ownership is rejected.
    #[expect(
        clippy::too_many_arguments,
        reason = "Keep separately owned protocol, persistence and lifecycle authorities explicit at this composition boundary."
    )]
    async fn start_native_service_composed<A, V, C, P, L, G>(
        &self,
        authority: A,
        validator: V,
        clock: C,
        audit: Arc<dyn AuditPort>,
        protocol: Arc<P>,
        launcher: Arc<L>,
        configuration: Arc<G>,
        retain: Arc<dyn RetainPrompt<V::Body, Retained = P::Retained>>,
        options: NativeRuntimeOptions,
    ) -> Result<
        (
            Service<A, V, C>,
            NativeRuntimeHandle<P, L, G, SqliteNativeDispatch>,
        ),
        DispatchError,
    >
    where
        A: RequestAuthority + 'static,
        V: ValidatedBodyPort + 'static,
        C: NodeClock + 'static,
        P: crate::application::native_dispatch::NativeApprovalPort,
        L: SourceLaunchPort,
        L::Owned: OwnedNativeSource<Streams = P::Streams>,
        G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    {
        options.validate()?;
        authority
            .verify_owner_scope(&self.request_registry.owner_scope())
            .map_err(|error| match error {
                crate::domain::request_registry::RegistryError::InvalidActor => {
                    DispatchError::InvalidScope
                }
                other => DispatchError::from(other),
            })?;
        let (service, native, startup) = {
            let _lifecycle = self
                .queue_lifecycle
                .lock()
                .map_err(|_| DispatchError::StorageUnavailable)?;
            if self.queue_closed.load(Ordering::SeqCst) {
                return Err(DispatchError::NodeStopping);
            }
            self.queue_runtime_claim
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .map_err(|_| DispatchError::RuntimeAlreadyStarted)?;
            let runtime = match random_id() {
                Ok(id) => RuntimeEpoch(id),
                Err(error) => {
                    self.queue_runtime_claim.store(false, Ordering::SeqCst);
                    return Err(error.into());
                }
            };
            let (environment, changes) = NativeEnvironment::new();
            let (service, driver) = QueueService::new(
                authority,
                validator,
                self.request_registry(),
                self.request_registry.owner_scope(),
                clock,
                runtime,
                environment.clone(),
                audit,
                self.queue_runtime_claim.clone(),
                #[cfg(test)]
                self.queue_randoms.clone(),
            );
            let core = NativeCoordinator::create(
                protocol,
                launcher,
                configuration,
                self.request_registry.native_dispatch_repository(),
                driver,
                retain,
                environment,
                service.native_clock(),
                runtime,
                self.queue_runtime_claim.clone(),
                options,
            );
            let native = NativeRuntimeHandle { core };
            let supervisor = Arc::new(CombinedSupervisor {
                service: service.clone(),
                native: native.clone(),
                claim: self.queue_runtime_claim.clone(),
                #[cfg(windows)]
                preparation: self.preparation_shutdown.clone(),
                #[cfg(windows)]
                document: self.document_shutdown.clone(),
                #[cfg(windows)]
                catalog: self.catalog_shutdown.clone(),
            });
            if let Ok(mut slot) = self.queue_supervisor.lock() {
                *slot = Some(supervisor);
            } else {
                self.queue_runtime_claim.store(false, Ordering::SeqCst);
                return Err(DispatchError::StorageUnavailable);
            }
            native.core.launch(changes)?;
            let startup = service.launch_startup()?;
            (service, native, startup)
        };
        let _ = startup.await;
        Ok((service, native))
    }
}
