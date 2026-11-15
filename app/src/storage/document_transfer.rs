//! Document cleanup is installed under the original Storage -> Queue runtime registration fence.
#![allow(
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{Arc, Ordering, Storage};
use crate::application::{
    document_disposal::{DocumentDisposalRegistration, DocumentShutdownPort},
    queue_owner::QueueRuntimeIdentity,
    request_registry::RegistryOwnerScope,
};
use crate::domain::request_registry::RegistryError;
impl Storage {
    pub(crate) fn register_document_shutdown(
        &self,
        expected: &QueueRuntimeIdentity,
        receipt: Arc<dyn DocumentShutdownPort>,
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
        supervisor.register_document(expected, &mut || {
            self.document_shutdown
                .install(receipt.take().ok_or(RegistryError::StorageUnavailable)?)
        })
    }
    pub(crate) fn document_shutdown_receipt(&self) -> Option<Arc<dyn DocumentShutdownPort>> {
        self.document_shutdown.get()
    }
}

impl DocumentDisposalRegistration for Storage {
    fn owner_scope(&self) -> RegistryOwnerScope {
        self.request_registry().owner_scope()
    }
    fn register(
        &self,
        expected: &QueueRuntimeIdentity,
        receipt: Arc<dyn DocumentShutdownPort>,
    ) -> Result<(), RegistryError> {
        self.register_document_shutdown(expected, receipt)
    }
}
