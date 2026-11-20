//! Real management authentication and the original registry owner, without network actions.
use super::NodeRuntime;
use crate::{
    access_policy::{AccessPolicyError, ApplicationPolicy, AuthenticatedActor},
    application::{
        request_registry::{RegistryOwnerScope, RequestAuthority},
        session_control::SessionControlCoordinator,
    },
    domain::{
        ActorContext, EntityId,
        native::NativeSessionKey,
        request_registry::{QueueAction, RegistryError},
    },
    session_storage::request_registry::{ResolvedControlRoot, SqliteRequestRegistry},
};
use std::sync::Arc;

/// Trusted queue port bound only by its original `NodeRuntime` to policy and storage.
#[derive(Clone)]
pub struct NodeRequestAuthority {
    policy: Arc<ApplicationPolicy>,
    registry: SqliteRequestRegistry,
    session_control: Arc<SessionControlCoordinator>,
}

impl NodeRuntime {
    /// Obtains the node's real authentication/target adapter; this starts no runtime or action.
    /// # Errors
    /// Rejects a missing or physically sealed original storage owner.
    pub fn request_authority(&self) -> Result<NodeRequestAuthority, RegistryError> {
        let policy = self
            .application_policy
            .as_ref()
            .ok_or(RegistryError::NodeStopping)?;
        let storage = self.storage.as_ref().ok_or(RegistryError::NodeStopping)?;
        NodeRequestAuthority::from_node_parts(Arc::clone(policy), storage)
    }
}

impl NodeRequestAuthority {
    pub(super) fn from_node_parts(
        policy: Arc<ApplicationPolicy>,
        storage: &crate::storage::Storage,
    ) -> Result<Self, RegistryError> {
        let registry = storage.request_registry();
        registry.owner_scope().require_open()?;
        Ok(Self {
            policy,
            registry,
            session_control: storage.session_control(),
        })
    }
    pub(super) fn registry_scope(&self) -> RegistryOwnerScope {
        self.registry.owner_scope()
    }
    #[must_use]
    pub(crate) const fn session_control(&self) -> &Arc<SessionControlCoordinator> {
        &self.session_control
    }
    #[must_use]
    pub(crate) const fn registry(&self) -> &SqliteRequestRegistry {
        &self.registry
    }
    pub(crate) async fn resolve_control_root(
        &self,
        target: EntityId,
    ) -> Result<ResolvedControlRoot, RegistryError> {
        self.registry.resolve_control_root(target).await
    }
    pub(super) async fn current(
        &self,
        actor: &AuthenticatedActor,
    ) -> Result<ActorContext, RegistryError> {
        let scope = self.registry.owner_scope();
        scope
            .while_open(async {
                self.policy
                    .require_current_actor(actor)
                    .await
                    .map_err(|error| policy_error(&error))?;
                let client_id =
                    EntityId::decode(actor.client_id()).map_err(|_| RegistryError::InvalidActor)?;
                if actor.auth_generation < 0 {
                    return Err(RegistryError::InvalidActor);
                }
                Ok(ActorContext {
                    client_id,
                    auth_generation: actor.auth_generation,
                })
            })
            .await
    }
}

const fn policy_error(error: &AccessPolicyError) -> RegistryError {
    match error {
        AccessPolicyError::InvalidActor => RegistryError::InvalidActor,
        AccessPolicyError::NodeUnavailable => RegistryError::NodeStopping,
        _ => RegistryError::StorageUnavailable,
    }
}

impl RequestAuthority for NodeRequestAuthority {
    type Actor = AuthenticatedActor;

    fn verify_owner_scope(&self, owner: &RegistryOwnerScope) -> Result<(), RegistryError> {
        let own = self.registry.owner_scope();
        if !own.matches(owner) {
            return Err(RegistryError::InvalidActor);
        }
        own.require_open()?;
        owner.require_open()
    }

    async fn authorize(
        &self,
        actor: &AuthenticatedActor,
        target: EntityId,
        action: QueueAction,
    ) -> Result<ActorContext, RegistryError> {
        self.authorize_with_epoch(actor, target, action, None)
            .await
            .map(|(context, _, _)| context)
    }

    async fn authorize_with_epoch(
        &self,
        actor: &AuthenticatedActor,
        target: EntityId,
        _action: QueueAction,
        expected_control_epoch: Option<u64>,
    ) -> Result<(ActorContext, Option<NativeSessionKey>, Option<u64>), RegistryError> {
        let context = self.current(actor).await?;
        let resolved = self.resolve_control_root(target).await?;
        let epoch = self
            .session_control
            .authorize_controlled(&resolved.root, context.client_id, expected_control_epoch)
            .map_err(|_| RegistryError::SessionNotControlled)?;
        Ok((context, Some(resolved.root), Some(epoch)))
    }

    async fn authorize_input_consume(
        &self,
        actor: &AuthenticatedActor,
        target: EntityId,
        _action: QueueAction,
        registration: crate::domain::request_registry::RegistrationId,
        expected_control_epoch: Option<u64>,
    ) -> Result<(ActorContext, Option<NativeSessionKey>, Option<u64>), RegistryError> {
        let context = self.current(actor).await?;
        let resolved = self.resolve_control_root(target).await?;
        let epoch = self
            .session_control
            .verify_input_consume(
                registration.entity(),
                &resolved.root,
                context.client_id,
                expected_control_epoch,
            )
            .map_err(|_| RegistryError::SessionNotControlled)?;
        Ok((context, Some(resolved.root), Some(epoch)))
    }

    fn record_input_registration(
        &self,
        actor: &crate::application::request_registry::CurrentRequestActor,
        registration: crate::domain::request_registry::RegistrationId,
    ) {
        if let (Some(root), Some(epoch)) = (actor.control_root(), actor.control_epoch()) {
            self.session_control.bind_input_registration(
                registration.entity(),
                root,
                actor.context().client_id,
                epoch,
            );
        }
    }

    fn reverify_consume(
        &self,
        actor: &crate::application::request_registry::CurrentRequestActor,
        registration: crate::domain::request_registry::RegistrationId,
    ) -> Result<(), RegistryError> {
        let Some(root) = actor.control_root() else {
            return Err(RegistryError::SessionNotControlled);
        };
        self.session_control
            .verify_input_consume(
                registration.entity(),
                root,
                actor.context().client_id,
                actor.expected_control_epoch(),
            )
            .map(|_| ())
            .map_err(|_| RegistryError::SessionNotControlled)
    }

    async fn authorize_read(
        &self,
        actor: &AuthenticatedActor,
        _target: EntityId,
    ) -> Result<ActorContext, RegistryError> {
        self.current(actor).await
    }

    async fn resolve_target(&self, target: EntityId) -> Result<NativeSessionKey, RegistryError> {
        self.registry.resolve_native_target(target).await
    }
}
