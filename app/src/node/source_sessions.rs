//! Thin same-node source facade. Only the original authenticated socket can admit a new operation.
#![allow(
    clippy::option_if_let_else,
    clippy::significant_drop_tightening,
    clippy::type_complexity,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    NodeRequestAuthority, NodeRuntime, document_transfer::NodeDocumentService,
    session_services::ProductNative,
};
#[cfg(test)]
use crate::domain::source_sessions::{SourceSqlObservation, SourceSqlRole, SourceTaskObservation};
use crate::{
    access_policy::AuthenticatedActor,
    adapters::acp::native_dispatch::{AcpNativeAdapter, AcpSourceResult},
    application::{
        native_dispatch::{
            BoxFuture,
            source_sessions::{
                OriginalSourceResult, SourceResultPublisher, SourceSessionAuthority,
                SourceSessionManager,
            },
        },
        shared_views::socket_authority::{
            SocketLifetimeParticipant, SocketViewAuthority, SocketViewClient,
        },
    },
    domain::{
        ActorContext, EntityId,
        source_sessions::{
            SourceLaunchSelection, SourceSessionAction, SourceSessionError, SourceSessionFact,
            SourceSessionFailure, SourceSessionOperationId, SourceSessionRegistrationMetadata,
        },
    },
    platform::native_source::WindowsSourceLauncher,
    session_storage::{SqliteNodeConfiguration, native_dispatch::SqliteNativeDispatch},
};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
};
type Manager = SourceSessionManager<
    AcpNativeAdapter,
    WindowsSourceLauncher,
    SqliteNodeConfiguration,
    SqliteNativeDispatch,
>;
pub type NodeSourceSessionReceipt =
    crate::application::native_dispatch::source_sessions::SourceSessionReceipt<AcpNativeAdapter>;
struct OfferEntry {
    token: Arc<()>,
    _participant: Arc<OfferPublisher>,
    root: Option<Arc<OriginalSourceResult<AcpSourceResult>>>,
}
struct OfferState {
    closed: bool,
    entries: HashMap<SourceSessionOperationId, OfferEntry>,
}
struct Offers {
    state: Mutex<OfferState>,
}
struct OfferPublisher {
    owner: Weak<Offers>,
    id: SourceSessionOperationId,
    token: Arc<()>,
    sealed: AtomicBool,
}
impl Offers {
    fn remove(&self, id: SourceSessionOperationId, token: &Arc<()>) {
        let removed = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state
                .entries
                .get(&id)
                .is_some_and(|entry| Arc::ptr_eq(&entry.token, token))
            {
                state.entries.remove(&id)
            } else {
                None
            }
        };
        if let Some(entry) = removed
            && let Some(root) = entry.root
        {
            root.withdraw_offer();
        }
    }
    fn consume(
        &self,
        id: SourceSessionOperationId,
        root: &Arc<OriginalSourceResult<AcpSourceResult>>,
    ) {
        let removed = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state
                .entries
                .get(&id)
                .and_then(|entry| entry.root.as_ref())
                .is_some_and(|original| Arc::ptr_eq(original, root))
            {
                state.entries.remove(&id)
            } else {
                None
            }
        };
        root.claimed();
        drop(removed);
    }
    fn seal(&self) {
        let entries = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.closed = true;
            std::mem::take(&mut state.entries)
        };
        for (_, entry) in entries {
            if let Some(root) = entry.root {
                root.withdraw_offer();
            }
        }
    }
}
impl OfferPublisher {
    fn reserve(self: &Arc<Self>) -> Result<(), SourceSessionFailure> {
        let owner = self
            .owner
            .upgrade()
            .ok_or(SourceSessionError::NodeStopping)?;
        let mut state = owner
            .state
            .lock()
            .map_err(|_| SourceSessionError::NodeStopping)?;
        if state.closed || self.sealed.load(Ordering::Acquire) {
            return Err(SourceSessionError::NodeStopping.into());
        }
        if state.entries.contains_key(&self.id) {
            return Ok(());
        }
        state
            .entries
            .try_reserve(1)
            .map_err(|_| SourceSessionError::Allocation)?;
        state.entries.insert(
            self.id,
            OfferEntry {
                token: self.token.clone(),
                _participant: self.clone(),
                root: None,
            },
        );
        Ok(())
    }
}
impl SocketLifetimeParticipant for OfferPublisher {
    fn seal(&self) {
        self.sealed.store(true, Ordering::Release);
        self.no_result();
    }
}
impl SourceResultPublisher<AcpSourceResult> for OfferPublisher {
    fn publish(&self, root: Arc<OriginalSourceResult<AcpSourceResult>>) {
        let weak = self.owner.clone();
        let token = self.token.clone();
        let id = self.id;
        root.on_offer_retired(Box::new(move || {
            if let Some(owner) = weak.upgrade() {
                owner.remove(id, &token);
            }
        }));
        let retained = if let Some(owner) = self.owner.upgrade() {
            let mut state = owner
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !state.closed && !self.sealed.load(Ordering::Acquire) && root.available() {
                if let Some(entry) = state.entries.get_mut(&self.id) {
                    if Arc::ptr_eq(&entry.token, &self.token) && entry.root.is_none() {
                        entry.root = Some(root.clone());
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };
        if !retained {
            root.withdraw_offer();
        }
    }
    fn no_result(&self) {
        if let Some(owner) = self.owner.upgrade() {
            owner.remove(self.id, &self.token);
        }
    }
}
#[derive(Clone)]
pub struct NodeSourceSessionManagement {
    manager: Manager,
    authority: NodeRequestAuthority,
    socket: Arc<SocketViewAuthority>,
    documents: NodeDocumentService,
    offers: Arc<Offers>,
}
impl SourceSessionAuthority<AuthenticatedActor> for NodeRequestAuthority {
    fn verify_owner_scope(
        &self,
        scope: &crate::application::request_registry::RegistryOwnerScope,
    ) -> Result<(), SourceSessionError> {
        let original = self.registry_scope();
        if !original.matches(scope) {
            return Err(SourceSessionError::InvalidScope);
        }
        scope
            .require_open()
            .map_err(|_| SourceSessionError::NodeStopping)
    }
    fn current<'a>(
        &'a self,
        actor: &'a AuthenticatedActor,
    ) -> BoxFuture<'a, Result<ActorContext, SourceSessionError>> {
        Box::pin(async move {
            Self::current(self, actor)
                .await
                .map_err(|error| match error {
                    crate::domain::request_registry::RegistryError::InvalidActor => {
                        SourceSessionError::InvalidActor
                    }
                    crate::domain::request_registry::RegistryError::NodeStopping => {
                        SourceSessionError::NodeStopping
                    }
                    _ => SourceSessionError::StorageUnavailable,
                })
        })
    }
}
impl NodeSourceSessionManagement {
    pub(super) async fn new(
        native: ProductNative,
        authority: NodeRequestAuthority,
        socket: Arc<SocketViewAuthority>,
        documents: NodeDocumentService,
    ) -> Result<Self, SourceSessionFailure> {
        let (disposal, scope, queue) = documents.source_parts();
        SourceSessionAuthority::verify_owner_scope(&authority, &scope)?;
        let manager = Manager::new(native, disposal, scope, queue)?;
        manager.recover().await?;
        Ok(Self {
            manager,
            authority,
            socket,
            documents,
            offers: Arc::new(Offers {
                state: Mutex::new(OfferState {
                    closed: false,
                    entries: HashMap::new(),
                }),
            }),
        })
    }
    fn check(&self, client: &SocketViewClient) -> Result<(), SourceSessionFailure> {
        if !Arc::ptr_eq(client.context.authority(), &self.socket) {
            return Err(SourceSessionError::InvalidScope.into());
        }
        client
            .context
            .lifetime
            .with_live(|| Ok(()))
            .map_err(|_| SourceSessionError::InvalidActor)?;
        Ok(())
    }
    pub(crate) async fn register(
        &self,
        client: &SocketViewClient,
        selection: SourceLaunchSelection,
        action: SourceSessionAction,
    ) -> Result<SourceSessionRegistrationMetadata, SourceSessionFailure> {
        self.check(client)?;
        let registered = self
            .manager
            .register(&self.authority, client.context.actor(), selection, action)
            .await?;
        let (_, guard) = self
            .socket
            .current_actor_guard(client)
            .await
            .map_err(|_| SourceSessionError::InvalidActor)?;
        drop(guard);
        Ok(registered)
    }
    pub(crate) async fn register_with_id(
        &self,
        client: &SocketViewClient,
        id: SourceSessionOperationId,
        selection: SourceLaunchSelection,
        action: SourceSessionAction,
    ) -> Result<SourceSessionRegistrationMetadata, SourceSessionFailure> {
        self.check(client)?;
        let registered = self
            .manager
            .register_with_id(
                &self.authority,
                client.context.actor(),
                id,
                selection,
                action,
            )
            .await?;
        let (_, guard) = self
            .socket
            .current_actor_guard(client)
            .await
            .map_err(|_| SourceSessionError::InvalidActor)?;
        drop(guard);
        Ok(registered)
    }
    pub(crate) async fn execute(
        &self,
        client: &SocketViewClient,
        id: SourceSessionOperationId,
    ) -> Result<NodeSourceSessionReceipt, SourceSessionFailure> {
        self.check(client)?;
        let prepared = self
            .manager
            .prepare_execute(&self.authority, client.context.actor(), id)
            .await?;
        let publisher = Arc::new(OfferPublisher {
            owner: Arc::downgrade(&self.offers),
            id,
            token: Arc::new(()),
            sealed: AtomicBool::new(false),
        });
        // Registration takes the original lifetime lock itself: it must precede the final fence.
        let participant: Arc<dyn SocketLifetimeParticipant> = publisher.clone();
        client
            .context
            .lifetime
            .register_participant(Arc::downgrade(&participant))
            .map_err(|_| SourceSessionError::InvalidActor)?;
        let (actor, guard) = self
            .socket
            .current_actor_guard(client)
            .await
            .map_err(|_| SourceSessionError::InvalidActor)?;
        prepared.require_actor(ActorContext {
            client_id: EntityId::decode(actor.client_id())
                .map_err(|_| SourceSessionError::InvalidActor)?,
            auth_generation: actor.auth_generation,
        })?;
        let admitted = client
            .context
            .lifetime
            .with_live(|| {
                Ok(publisher
                    .reserve()
                    .and_then(|()| prepared.begin(publisher.clone())))
            })
            .map_err(|_| SourceSessionError::InvalidActor)?;
        drop(guard);
        if admitted.is_err() {
            publisher.no_result();
        }
        admitted
    }
    pub(crate) async fn query(
        &self,
        client: &SocketViewClient,
        id: SourceSessionOperationId,
    ) -> Result<SourceSessionFact, SourceSessionFailure> {
        self.check(client)?;
        let fact = self
            .manager
            .query(&self.authority, client.context.actor(), id)
            .await?;
        let (_, guard) = self
            .socket
            .current_actor_guard(client)
            .await
            .map_err(|_| SourceSessionError::InvalidActor)?;
        drop(guard);
        Ok(fact)
    }
    pub(crate) async fn reconcile(
        &self,
        client: &SocketViewClient,
        id: SourceSessionOperationId,
    ) -> Result<SourceSessionFact, SourceSessionFailure> {
        self.check(client)?;
        let fact = self
            .manager
            .reconcile(&self.authority, client.context.actor(), id)
            .await?;
        let (_, guard) = self
            .socket
            .current_actor_guard(client)
            .await
            .map_err(|_| SourceSessionError::InvalidActor)?;
        drop(guard);
        Ok(fact)
    }
    #[cfg(test)]
    pub(crate) fn pause_next_sql_for_test(
        &self,
        role: SourceSqlRole,
    ) -> Result<crate::test_support::SharedAsyncBarrier, SourceSessionFailure> {
        self.manager.pause_next_sql_for_test(role)
    }
    #[cfg(test)]
    pub(crate) fn observe_next_invocation_for_test(
        &self,
        role: SourceSqlRole,
    ) -> Result<
        tokio::sync::oneshot::Receiver<
            Arc<crate::application::native_dispatch::source_sessions::SourceSqlInvocation>,
        >,
        SourceSessionFailure,
    > {
        self.manager.observe_next_invocation_for_test(role)
    }
    #[cfg(test)]
    pub(crate) fn reject_next_dispatch_for_test(
        &self,
        role: SourceSqlRole,
        error: SourceSessionError,
    ) -> Result<(), SourceSessionFailure> {
        self.manager.reject_next_dispatch_for_test(role, error)
    }
    #[cfg(test)]
    pub(crate) fn arm_next_sql_fault_for_test(
        &self,
        role: SourceSqlRole,
        fault: crate::application::native_dispatch::source_sessions::SourceSqlTestFault,
    ) -> Result<(), SourceSessionFailure> {
        self.manager.arm_next_sql_fault_for_test(role, fault)
    }
    #[cfg(test)]
    pub(crate) fn pause_next_cpu_decode_for_test(
        &self,
    ) -> Result<
        Arc<crate::application::native_dispatch::source_sessions::BlockingTestBarrier>,
        SourceSessionFailure,
    > {
        self.manager.pause_next_cpu_decode_for_test()
    }
    #[cfg(test)]
    pub(crate) fn pause_next_cpu_retire_for_test(
        &self,
    ) -> Result<
        Arc<crate::application::native_dispatch::source_sessions::BlockingTestBarrier>,
        SourceSessionFailure,
    > {
        self.manager.pause_next_cpu_retire_for_test()
    }
    #[cfg(test)]
    pub(crate) fn cpu_tasks_for_test(
        &self,
        id: SourceSessionOperationId,
    ) -> Result<Vec<(usize, SourceTaskObservation)>, SourceSessionFailure> {
        self.manager.cpu_tasks_for_test(id)
    }
    #[cfg(test)]
    pub(crate) fn registered_native_tasks_for_test(&self) -> Result<usize, SourceSessionFailure> {
        self.manager.registered_native_tasks_for_test()
    }
    #[cfg(test)]
    pub(crate) fn original_sql_for_test(
        &self,
        id: SourceSessionOperationId,
    ) -> Result<Vec<(Option<SourceSqlObservation>, bool)>, SourceSessionFailure> {
        self.manager.original_sql_for_test(id)
    }
    #[cfg(test)]
    pub(crate) fn original_sql_cleanup_for_test(
        &self,
        id: SourceSessionOperationId,
    ) -> Result<
        Vec<
            Option<crate::application::native_dispatch::source_sessions::SourceSqlOriginalSnapshot>,
        >,
        SourceSessionFailure,
    > {
        self.manager.original_sql_cleanup_for_test(id)
    }
    #[cfg(test)]
    pub(crate) fn source_close_for_test(
        &self,
        id: SourceSessionOperationId,
    ) -> Result<
        Option<(
            bool,
            Option<Result<(), crate::domain::native_dispatch::DispatchError>>,
            usize,
        )>,
        SourceSessionFailure,
    > {
        self.manager.source_close_for_test(id)
    }
    pub(crate) async fn open_result(
        &self,
        client: &SocketViewClient,
        id: SourceSessionOperationId,
    ) -> Result<crate::application::document_transfer::DocumentTransfer, SourceSessionFailure> {
        self.check(client)?;
        self.manager
            .query(&self.authority, client.context.actor(), id)
            .await?;
        let root = self
            .manager
            .result(id)
            .ok_or(SourceSessionError::ResultUnavailable)?;
        let transfer = self
            .documents
            .open_source_result(client, &root)
            .await
            .map_err(|_| SourceSessionError::ResultUnavailable)?;
        self.offers.consume(id, &root);
        Ok(transfer)
    }
    pub(crate) async fn release_result(
        &self,
        client: &SocketViewClient,
        id: SourceSessionOperationId,
    ) -> Result<(), SourceSessionFailure> {
        self.check(client)?;
        self.manager
            .query(&self.authority, client.context.actor(), id)
            .await?;
        let (_, guard) = self
            .socket
            .current_actor_guard(client)
            .await
            .map_err(|_| SourceSessionError::InvalidActor)?;
        let root = client
            .context
            .lifetime
            .with_live(|| Ok(self.manager.result(id)))
            .map_err(|_| SourceSessionError::InvalidActor)?;
        drop(guard);
        if let Some(root) = root {
            root.withdraw_offer();
        }
        Ok(())
    }
    pub(super) fn seal(&self) {
        self.offers.seal();
    }
}
impl NodeRuntime {
    pub(crate) fn source_session_management(
        &self,
    ) -> Result<NodeSourceSessionManagement, SourceSessionFailure> {
        self.session_services
            .as_ref()
            .map(|services| services.source_sessions.clone())
            .ok_or_else(|| SourceSessionError::NodeStopping.into())
    }
}
