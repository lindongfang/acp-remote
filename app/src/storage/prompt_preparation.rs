//! Preparation receipt is installed under Storage -> original Queue task fence.
#![allow(
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{Arc, Ordering, Storage};
use crate::application::{
    prompt_preparation::{PreparationShutdownSlot, PromptPreparationShutdownPort},
    queue_owner::{QueueRuntimeIdentity, RuntimeSupervisor},
    request_registry::RegistryError,
};
use std::{future::Future, pin::Pin};
impl Storage {
    pub(crate) fn register_prompt_preparation(
        &self,
        expected: &QueueRuntimeIdentity,
        receipt: Arc<dyn PromptPreparationShutdownPort>,
    ) -> Result<(), RegistryError> {
        let _lifecycle = self
            .queue_lifecycle
            .lock()
            .map_err(|_| RegistryError::StorageUnavailable)?;
        if self.queue_closed.load(Ordering::SeqCst) {
            return Err(RegistryError::NodeStopping);
        }
        let supervisor = self
            .queue_supervisor
            .lock()
            .map_err(|_| RegistryError::StorageUnavailable)?
            .clone()
            .ok_or(RegistryError::NodeStarting)?;
        let mut receipt = Some(receipt);
        supervisor.register_preparation(expected, &mut || {
            self.preparation_shutdown
                .install(receipt.take().ok_or(RegistryError::StorageUnavailable)?)
        })
    }
    pub(crate) fn prompt_preparation_receipt(
        &self,
    ) -> Option<Arc<dyn PromptPreparationShutdownPort>> {
        self.preparation_shutdown.get()
    }
}
pub(super) struct QueuePreparationSupervisor<
    A,
    V: crate::application::request_registry::ValidatedBodyPort,
    R,
    C,
> {
    pub(super) service: crate::application::queue_owner::QueueService<A, V, R, C>,
    pub(super) preparation: Arc<PreparationShutdownSlot>,
    pub(super) document: Arc<crate::application::document_disposal::DocumentShutdownSlot>,
    #[cfg(windows)]
    pub(super) catalog: Arc<crate::application::catalog_preparation::CatalogShutdownSlot>,
}
impl<A, V, R, C> RuntimeSupervisor for QueuePreparationSupervisor<A, V, R, C>
where
    A: crate::application::request_registry::RequestAuthority + 'static,
    V: crate::application::request_registry::ValidatedBodyPort + 'static,
    R: crate::application::request_registry::RequestRegistryRepository + 'static,
    C: crate::application::request_registry::NodeClock + 'static,
{
    fn queue_runtime_identity(&self) -> QueueRuntimeIdentity {
        self.service.queue_runtime_identity()
    }
    fn register_preparation(
        &self,
        expected: &QueueRuntimeIdentity,
        install: &mut dyn FnMut() -> Result<(), RegistryError>,
    ) -> Result<(), RegistryError> {
        self.service.register_preparation(expected, install)
    }
    fn register_document(
        &self,
        expected: &QueueRuntimeIdentity,
        install: &mut dyn FnMut() -> Result<(), RegistryError>,
    ) -> Result<(), RegistryError> {
        self.service.register_document(expected, install)
    }
    fn register_catalog(
        &self,
        expected: &QueueRuntimeIdentity,
        install: &mut dyn FnMut() -> Result<(), RegistryError>,
    ) -> Result<(), RegistryError> {
        self.service.register_catalog(expected, install)
    }
    fn cancel(&self) {
        self.preparation.seal();
        self.document.seal();
        #[cfg(windows)]
        self.catalog.seal();
        self.service.cancel();
    }
    fn stop_and_join(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            self.preparation.seal();
            self.document.seal();
            #[cfg(windows)]
            self.catalog.seal();
            self.service.cancel();
            tokio::join!(
                self.preparation.join(),
                self.document.join(),
                self.catalog.join()
            );
            self.service.stop_and_join().await;
            self.service.release_preparation_claim();
        })
    }
}
