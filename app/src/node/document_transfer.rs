//! Same-node Queue document producer; snapshots and disposals stay under the original Storage claim.
#![allow(
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{NodeRuntime, document_payload::NodeDocumentPayload, session_services::ProductQueue};
use crate::{
    application::{
        document_disposal::DocumentDisposalOwner,
        document_source::DocumentError,
        document_transfer::{DocumentReadState, DocumentTransfer},
        queue_owner::RuntimeSupervisor,
        request_registry::RegistryOwnerScope,
        shared_views::socket_authority::{SocketViewAuthority, SocketViewClient},
    },
    domain::EntityId,
    storage::Storage,
};
use std::sync::{Arc, Mutex, Weak};
struct Registry {
    closed: bool,
    reads: Vec<Weak<DocumentReadState>>,
}
struct Core {
    queue: ProductQueue,
    authority: Arc<SocketViewAuthority>,
    scope: RegistryOwnerScope,
    disposal: DocumentDisposalOwner<NodeDocumentPayload>,
    registry: Mutex<Registry>,
}
#[derive(Clone)]
pub struct NodeDocumentService {
    core: Arc<Core>,
}
impl NodeDocumentService {
    pub(super) fn new(
        storage: &Storage,
        queue: ProductQueue,
        authority: Arc<SocketViewAuthority>,
    ) -> Result<Self, DocumentError> {
        let disposal = DocumentDisposalOwner::attach(storage, queue.queue_runtime_identity())?;
        Ok(Self {
            core: Arc::new(Core {
                queue,
                authority,
                scope: storage.request_registry().owner_scope(),
                disposal,
                registry: Mutex::new(Registry {
                    closed: false,
                    reads: Vec::new(),
                }),
            }),
        })
    }
    pub(crate) async fn open_queued_body(
        &self,
        client: &SocketViewClient,
        target: EntityId,
        item: EntityId,
        expected_revision: u64,
    ) -> Result<DocumentTransfer, DocumentError> {
        if !Arc::ptr_eq(client.context.authority(), &self.core.authority) {
            return Err(crate::domain::request_registry::RegistryError::InvalidActor.into());
        }
        self.core.scope.require_open()?;
        {
            if self
                .core
                .registry
                .lock()
                .map_err(|_| DocumentError::Closed)?
                .closed
            {
                return Err(DocumentError::Closed);
            }
        }
        let destination = self.core.disposal.reserve_payload()?;
        let snapshot = self
            .core
            .queue
            .snapshot_queued_body(
                client.context.actor(),
                target,
                item,
                expected_revision,
                &destination,
            )
            .await?;
        let source = snapshot
            .into_document_source()
            .bind_registry(self.core.scope.clone())?;
        source.require_current().await?;
        let (_, guard) = self.core.authority.current_actor_guard(client).await?;
        let transfer = DocumentTransfer::open(source, client)?;
        let mut registry = self
            .core
            .registry
            .lock()
            .map_err(|_| DocumentError::Closed)?;
        if registry.closed {
            drop(registry);
            transfer.detach()?;
            return Err(DocumentError::Closed);
        }
        registry.reads.retain(|read| read.strong_count() > 0);
        if registry.reads.try_reserve(1).is_err() {
            drop(registry);
            transfer.detach()?;
            return Err(DocumentError::Allocation);
        }
        registry.reads.push(transfer.weak_state());
        drop(registry);
        drop(guard);
        drop(destination);
        Ok(transfer)
    }
    pub(super) fn seal(&self) {
        let reads = {
            let mut registry = self
                .core
                .registry
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            registry.closed = true;
            std::mem::take(&mut registry.reads)
        };
        for read in reads.into_iter().filter_map(|read| read.upgrade()) {
            read.close();
        }
        self.core.disposal.seal();
    }
    pub(super) fn source_parts<R: crate::application::shared_views::RetainedRaw>(
        &self,
    ) -> (
        DocumentDisposalOwner<R>,
        RegistryOwnerScope,
        crate::application::queue_owner::QueueRuntimeIdentity,
    ) {
        (
            self.core.disposal.retype(),
            self.core.scope.clone(),
            self.core.queue.queue_runtime_identity(),
        )
    }
    pub(super) async fn open_source_result<R: crate::application::shared_views::RetainedRaw>(
        &self,
        client: &SocketViewClient,
        root: &Arc<crate::application::native_dispatch::source_sessions::OriginalSourceResult<R>>,
    ) -> Result<DocumentTransfer, DocumentError> {
        if !Arc::ptr_eq(client.context.authority(), &self.core.authority) {
            return Err(crate::domain::request_registry::RegistryError::InvalidActor.into());
        }
        root.verify_destination(&self.core.scope, &self.core.queue.queue_runtime_identity())?;
        let (_, guard) = self.core.authority.current_actor_guard(client).await?;
        let source = root.open_read()?;
        let transfer = DocumentTransfer::open(source, client)?;
        let mut registry = self
            .core
            .registry
            .lock()
            .map_err(|_| DocumentError::Closed)?;
        if registry.closed {
            drop(registry);
            transfer.detach()?;
            return Err(DocumentError::Closed);
        }
        registry.reads.retain(|read| read.strong_count() > 0);
        if registry.reads.try_reserve(1).is_err() {
            drop(registry);
            transfer.detach()?;
            return Err(DocumentError::Allocation);
        }
        registry.reads.push(transfer.weak_state());
        drop(registry);
        drop(guard);
        Ok(transfer)
    }
    #[cfg(test)]
    pub(super) fn reserve_payload_for_test(
        &self,
    ) -> Result<
        crate::application::document_disposal::DocumentPayloadSlot<NodeDocumentPayload>,
        DocumentError,
    > {
        self.core.disposal.reserve_payload()
    }
    #[cfg(test)]
    pub(crate) fn install_disposal_observer_for_test(
        &self,
        observer: crate::application::document_disposal::DisposalObserver,
    ) -> Result<(), DocumentError> {
        self.core.disposal.install_observer_for_test(observer)
    }
}
impl NodeRuntime {
    pub(crate) fn document_transfer_service(&self) -> Result<NodeDocumentService, DocumentError> {
        self.session_services
            .as_ref()
            .map(|services| services.documents.clone())
            .ok_or(DocumentError::Closed)
    }
}
