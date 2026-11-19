//! Document cleanup is installed under the original Storage -> Queue runtime registration fence.
#![allow(
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{Arc, Storage};
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
        self.inner.coordinator.register_document(expected, receipt)
    }
    pub(crate) fn document_shutdown_receipt(&self) -> Option<Arc<dyn DocumentShutdownPort>> {
        self.inner.coordinator.document_receipt()
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
