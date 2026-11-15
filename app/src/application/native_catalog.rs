//! Trusted catalog application port. Only a real Node authority can supply current actors.
use super::request_registry::{NodeClock, RegistryDeadline, RegistryOwnerScope};
use crate::domain::{
    ActorContext, EntityId,
    native_catalog::{
        CatalogDraft, CatalogError, CatalogFact, CatalogFailure, CatalogInventory,
        CatalogOperationKind, CatalogReceipt, CatalogRequestId, CatalogSqlObservationSet,
        CleanupKnowledge, CommitKnowledge, ProductTargetRecord, WorkspaceRecord,
    },
};
use std::{future::Future, sync::Arc};
pub(crate) trait CatalogAuthority: Send + Sync {
    type Actor: Sync;
    fn verify_owner(&self, scope: &RegistryOwnerScope) -> Result<(), CatalogError>;
    fn current(
        &self,
        actor: &Self::Actor,
    ) -> impl Future<Output = Result<ActorContext, CatalogError>> + Send;
}
pub(crate) struct CurrentCatalogActor {
    context: ActorContext,
    scope: RegistryOwnerScope,
}
impl CurrentCatalogActor {
    pub(crate) const fn context(&self) -> &ActorContext {
        &self.context
    }
    pub(crate) const fn owner_scope(&self) -> &RegistryOwnerScope {
        &self.scope
    }
}
/// Read row data only. Node wraps a genuine same-core read in its private observation.
pub(crate) struct CatalogWorkspaceRow {
    pub(crate) target: EntityId,
    pub(crate) source: EntityId,
    pub(crate) workspace: EntityId,
    pub(crate) canonical: String,
}
pub(crate) trait CatalogRepository: Send + Sync {
    fn observe_workspace(
        &self,
        _actor: &CurrentCatalogActor,
        _target: EntityId,
        _source: EntityId,
        _workspace: EntityId,
        _d: &RegistryDeadline,
    ) -> impl Future<Output = Result<CatalogWorkspaceRow, CatalogFailure>> + Send {
        async {
            Err(failure(
                CatalogOperationKind::Query,
                CatalogError::InvalidTarget,
            ))
        }
    }
    fn register_workspace(
        &self,
        _actor: Option<&CurrentCatalogActor>,
        _path: &str,
        _d: &RegistryDeadline,
    ) -> impl Future<Output = Result<WorkspaceRecord, CatalogFailure>> + Send {
        async {
            Err(failure(
                CatalogOperationKind::Register,
                CatalogError::InvalidConfiguration,
            ))
        }
    }
    fn register_target(
        &self,
        _actor: Option<&CurrentCatalogActor>,
        _source: EntityId,
        _native_session_id: &str,
        _workspace: EntityId,
        _d: &RegistryDeadline,
    ) -> impl Future<Output = Result<ProductTargetRecord, CatalogFailure>> + Send {
        async {
            Err(failure(
                CatalogOperationKind::Register,
                CatalogError::InvalidTarget,
            ))
        }
    }
    fn catalog_inventory(
        &self,
        _actor: Option<&CurrentCatalogActor>,
        _d: &RegistryDeadline,
    ) -> impl Future<Output = Result<CatalogInventory, CatalogFailure>> + Send {
        async {
            Err(failure(
                CatalogOperationKind::Query,
                CatalogError::StorageUnavailable,
            ))
        }
    }
    fn owner_scope(&self) -> RegistryOwnerScope;
    fn register(
        &self,
        actor: &CurrentCatalogActor,
        draft: CatalogDraft,
        at: i64,
        d: &RegistryDeadline,
    ) -> impl Future<Output = Result<CatalogRequestId, CatalogFailure>> + Send;
    fn apply(
        &self,
        actor: &CurrentCatalogActor,
        id: CatalogRequestId,
        at: i64,
        d: &RegistryDeadline,
    ) -> impl Future<Output = Result<CatalogReceipt, CatalogFailure>> + Send;
    fn query(
        &self,
        actor: &CurrentCatalogActor,
        id: CatalogRequestId,
        d: &RegistryDeadline,
    ) -> impl Future<Output = Result<CatalogFact, CatalogFailure>> + Send;
    fn reconcile(
        &self,
        d: &RegistryDeadline,
    ) -> impl Future<Output = Result<(), CatalogFailure>> + Send;
}
pub(crate) struct CatalogService<A, R, C> {
    authority: A,
    repository: Arc<R>,
    clock: C,
}
impl<A: CatalogAuthority, R: CatalogRepository, C: NodeClock> CatalogService<A, R, C> {
    pub(crate) fn trusted(
        authority: A,
        repository: Arc<R>,
        clock: C,
    ) -> Result<Self, CatalogFailure> {
        authority
            .verify_owner(&repository.owner_scope())
            .map_err(|e| failure(CatalogOperationKind::Query, e))?;
        Ok(Self {
            authority,
            repository,
            clock,
        })
    }
    async fn current(
        &self,
        actor: &A::Actor,
        kind: CatalogOperationKind,
        d: &RegistryDeadline,
    ) -> Result<CurrentCatalogActor, CatalogFailure> {
        let scope = self.repository.owner_scope();
        self.authority
            .verify_owner(&scope)
            .map_err(|e| failure(kind, e))?;
        let context = d
            .run(scope.while_open(async { Ok(self.authority.current(actor).await) }))
            .await
            .map_err(|e| failure(kind, e.into()))?
            .map_err(|e| failure(kind, e))?;
        self.authority
            .verify_owner(&scope)
            .map_err(|e| failure(kind, e))?;
        Ok(CurrentCatalogActor { context, scope })
    }
    pub(crate) async fn register(
        &self,
        actor: &A::Actor,
        draft: CatalogDraft,
        d: &RegistryDeadline,
    ) -> Result<CatalogRequestId, CatalogFailure> {
        let actor = self
            .current(actor, CatalogOperationKind::Register, d)
            .await?;
        self.repository
            .register(&actor, draft, self.clock.unix_millis(), d)
            .await
    }
    pub(crate) async fn apply(
        &self,
        actor: &A::Actor,
        id: CatalogRequestId,
        d: &RegistryDeadline,
    ) -> Result<CatalogReceipt, CatalogFailure> {
        let actor = self.current(actor, CatalogOperationKind::Apply, d).await?;
        self.repository
            .apply(&actor, id, self.clock.unix_millis(), d)
            .await
    }
    pub(crate) async fn query(
        &self,
        actor: &A::Actor,
        id: CatalogRequestId,
        d: &RegistryDeadline,
    ) -> Result<CatalogFact, CatalogFailure> {
        let actor = self.current(actor, CatalogOperationKind::Query, d).await?;
        self.repository.query(&actor, id, d).await
    }
    pub(crate) async fn observe_workspace(
        &self,
        actor: &A::Actor,
        target: EntityId,
        source: EntityId,
        workspace: EntityId,
        d: &RegistryDeadline,
    ) -> Result<CatalogWorkspaceRow, CatalogFailure> {
        let actor = self.current(actor, CatalogOperationKind::Query, d).await?;
        self.repository
            .observe_workspace(&actor, target, source, workspace, d)
            .await
    }
    pub(crate) async fn register_workspace(
        &self,
        actor: &A::Actor,
        path: &str,
        d: &RegistryDeadline,
    ) -> Result<WorkspaceRecord, CatalogFailure> {
        let actor = self
            .current(actor, CatalogOperationKind::Register, d)
            .await?;
        self.repository
            .register_workspace(Some(&actor), path, d)
            .await
    }
    pub(crate) async fn register_target(
        &self,
        actor: &A::Actor,
        source: EntityId,
        native_session_id: &str,
        workspace: EntityId,
        d: &RegistryDeadline,
    ) -> Result<ProductTargetRecord, CatalogFailure> {
        let actor = self
            .current(actor, CatalogOperationKind::Register, d)
            .await?;
        self.repository
            .register_target(Some(&actor), source, native_session_id, workspace, d)
            .await
    }
    pub(crate) async fn catalog_inventory(
        &self,
        actor: &A::Actor,
        d: &RegistryDeadline,
    ) -> Result<CatalogInventory, CatalogFailure> {
        let actor = self.current(actor, CatalogOperationKind::Query, d).await?;
        self.repository.catalog_inventory(Some(&actor), d).await
    }
    pub(crate) async fn reconcile(&self, d: &RegistryDeadline) -> Result<(), CatalogFailure> {
        self.repository.reconcile(d).await
    }
}
pub(crate) const fn failure(kind: CatalogOperationKind, error: CatalogError) -> CatalogFailure {
    CatalogFailure::new(
        error,
        None,
        CommitKnowledge::NoWrite,
        CleanupKnowledge::NoDetachedConnection,
        CatalogSqlObservationSet::new(kind, [None, None]),
    )
}
