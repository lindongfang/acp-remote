//! Private assembly from one real Node; integration into the default factory is separately gated.
#![allow(
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{NodeRuntime, request_authority::NodeRequestAuthority};
use crate::{
    access_policy::AuthenticatedActor,
    application::{
        native_catalog::{CatalogAuthority, CatalogRepository, CatalogService},
        request_registry::{RegistryDeadline, RegistryOwnerScope, SystemNodeClock},
    },
    domain::{
        ActorContext, EntityId,
        native_catalog::{
            CatalogDraft, CatalogError, CatalogFact, CatalogFailure, CatalogInventory,
            CatalogOperationKind, CatalogReceipt, CatalogRequestId, ProductTargetRecord,
            WorkspaceRecord,
        },
    },
    platform::SecretProtector,
    session_storage::SqliteNodeConfiguration,
};
use std::{sync::Arc, time::Duration};
struct NodeCatalogAuthority {
    original: NodeRequestAuthority,
}
impl CatalogAuthority for NodeCatalogAuthority {
    type Actor = AuthenticatedActor;
    fn verify_owner(&self, scope: &RegistryOwnerScope) -> Result<(), CatalogError> {
        let original = self.original.registry_scope();
        if !original.matches(scope) {
            return Err(CatalogError::InvalidActor);
        }
        original.require_open()?;
        scope.require_open()?;
        Ok(())
    }
    async fn current(&self, actor: &AuthenticatedActor) -> Result<ActorContext, CatalogError> {
        self.original.current(actor).await.map_err(Into::into)
    }
}
/// Minted only here from this original concrete catalog read. It is neither DTO nor filesystem proof.
pub(super) struct RegisteredWorkspaceObservation<R> {
    configuration: Arc<R>,
    scope: RegistryOwnerScope,
    target: crate::domain::EntityId,
    source: crate::domain::EntityId,
    workspace: crate::domain::EntityId,
    canonical: String,
}
impl<R: CatalogRepository> RegisteredWorkspaceObservation<R> {
    pub(super) fn bind(
        self,
        harness: crate::domain::EntityId,
        profile: crate::domain::native_catalog::ProfileKey,
        launch_path: String,
    ) -> Result<CatalogDraft, CatalogError> {
        let current = self.configuration.owner_scope();
        if !self.scope.matches(&current) {
            return Err(CatalogError::InvalidActor);
        }
        self.scope.require_open()?;
        current.require_open()?;
        CatalogDraft::bind_target_initial(
            self.target,
            self.source,
            harness,
            profile,
            self.workspace,
            launch_path,
            self.canonical,
        )
    }
}
pub(super) struct NodeCatalogCore<R> {
    service: CatalogService<NodeCatalogAuthority, R, SystemNodeClock>,
    configuration: Arc<R>,
}
impl<R: CatalogRepository> NodeCatalogCore<R> {
    pub(super) async fn observe_workspace(
        &self,
        actor: &AuthenticatedActor,
        target: crate::domain::EntityId,
        source: crate::domain::EntityId,
        workspace: crate::domain::EntityId,
        d: &RegistryDeadline,
    ) -> Result<RegisteredWorkspaceObservation<R>, CatalogFailure> {
        let row = self
            .service
            .observe_workspace(actor, target, source, workspace, d)
            .await?;
        let scope = self.configuration.owner_scope();
        scope.require_open().map_err(|e| {
            crate::application::native_catalog::failure(CatalogOperationKind::Query, e.into())
        })?;
        Ok(RegisteredWorkspaceObservation {
            configuration: self.configuration.clone(),
            scope,
            target: row.target,
            source: row.source,
            workspace: row.workspace,
            canonical: row.canonical,
        })
    }
    pub(super) fn configuration(&self) -> Arc<R> {
        Arc::clone(&self.configuration)
    }
    pub(super) async fn register(
        &self,
        actor: &AuthenticatedActor,
        draft: CatalogDraft,
        d: &RegistryDeadline,
    ) -> Result<CatalogRequestId, CatalogFailure> {
        self.service.register(actor, draft, d).await
    }
    pub(super) async fn apply(
        &self,
        actor: &AuthenticatedActor,
        id: CatalogRequestId,
        d: &RegistryDeadline,
    ) -> Result<CatalogReceipt, CatalogFailure> {
        self.service.apply(actor, id, d).await
    }
    pub(super) async fn query(
        &self,
        actor: &AuthenticatedActor,
        id: CatalogRequestId,
        d: &RegistryDeadline,
    ) -> Result<CatalogFact, CatalogFailure> {
        self.service.query(actor, id, d).await
    }
    pub(super) async fn register_workspace(
        &self,
        actor: &AuthenticatedActor,
        path: &str,
        d: &RegistryDeadline,
    ) -> Result<WorkspaceRecord, CatalogFailure> {
        self.service.register_workspace(actor, path, d).await
    }
    pub(super) async fn register_target(
        &self,
        actor: &AuthenticatedActor,
        source: EntityId,
        native_session_id: &str,
        workspace: EntityId,
        d: &RegistryDeadline,
    ) -> Result<ProductTargetRecord, CatalogFailure> {
        self.service
            .register_target(actor, source, native_session_id, workspace, d)
            .await
    }
    pub(super) async fn catalog_inventory(
        &self,
        actor: &AuthenticatedActor,
        d: &RegistryDeadline,
    ) -> Result<CatalogInventory, CatalogFailure> {
        self.service.catalog_inventory(actor, d).await
    }
    pub(super) async fn reconcile(&self, d: &RegistryDeadline) -> Result<(), CatalogFailure> {
        self.service.reconcile(d).await
    }
}
pub(super) fn from_node_parts(
    original: NodeRequestAuthority,
    storage: &crate::storage::Storage,
    identity: &crate::identity::NodeIdentity,
    protector: Arc<dyn SecretProtector>,
    metadata_wait: Duration,
) -> Result<NodeCatalogCore<SqliteNodeConfiguration>, CatalogFailure> {
    let map = |e| crate::application::native_catalog::failure(CatalogOperationKind::Query, e);
    let authority = NodeCatalogAuthority { original };
    let registry = storage.request_registry();
    authority
        .verify_owner(&registry.owner_scope())
        .map_err(map)?;
    let configuration = Arc::new(
        crate::session_storage::node_catalog_repository(
            registry,
            protector,
            identity.node_id().to_owned(),
            identity.public_key().to_vec(),
            metadata_wait,
        )
        .map_err(map)?,
    );
    let service = CatalogService::trusted(authority, configuration.clone(), SystemNodeClock)?;
    Ok(NodeCatalogCore {
        service,
        configuration,
    })
}
impl NodeRuntime {
    pub(super) fn catalog_core(
        &self,
        protector: Arc<dyn SecretProtector>,
        metadata_wait: Duration,
    ) -> Result<NodeCatalogCore<SqliteNodeConfiguration>, CatalogFailure> {
        let map = |e| crate::application::native_catalog::failure(CatalogOperationKind::Query, e);
        from_node_parts(
            self.request_authority().map_err(|e| map(e.into()))?,
            self.storage
                .as_ref()
                .ok_or_else(|| map(CatalogError::NodeStopping))?,
            self.identity
                .as_ref()
                .ok_or_else(|| map(CatalogError::NodeStopping))?,
            protector,
            metadata_wait,
        )
    }
}
