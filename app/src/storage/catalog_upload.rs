//! Catalog preparation is installed under the original Storage -> Queue runtime registration fence.
#![allow(
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{Arc, Storage};
use crate::application::{
    catalog_preparation::{CatalogPreparationRegistration, CatalogUploadShutdownPort},
    queue_owner::QueueRuntimeIdentity,
    request_registry::RegistryOwnerScope,
};
use crate::domain::request_registry::RegistryError;
impl Storage {
    pub(crate) fn register_catalog_preparation(
        &self,
        expected: &QueueRuntimeIdentity,
        receipt: Arc<dyn CatalogUploadShutdownPort>,
    ) -> Result<(), RegistryError> {
        self.inner.coordinator.register_catalog(expected, receipt)
    }
    pub(crate) fn catalog_upload_shutdown_receipt(
        &self,
    ) -> Option<Arc<dyn CatalogUploadShutdownPort>> {
        self.inner.coordinator.catalog_receipt()
    }
}

impl CatalogPreparationRegistration for Storage {
    fn owner_scope(&self) -> RegistryOwnerScope {
        self.request_registry().owner_scope()
    }
    fn register(
        &self,
        expected: &QueueRuntimeIdentity,
        receipt: Arc<dyn CatalogUploadShutdownPort>,
    ) -> Result<(), RegistryError> {
        self.register_catalog_preparation(expected, receipt)
    }
}
