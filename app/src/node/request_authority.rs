//! Real management authentication and the original registry owner, without network actions.
use super::NodeRuntime;
use crate::{
    access_policy::{AccessPolicyError, ApplicationPolicy, AuthenticatedActor},
    application::request_registry::{RegistryOwnerScope, RequestAuthority},
    domain::{
        ActorContext, EntityId,
        native::NativeSessionKey,
        request_registry::{QueueAction, RegistryError},
    },
    session_storage::request_registry::SqliteRequestRegistry,
};
use std::sync::Arc;

/// Trusted queue port bound only by its original `NodeRuntime` to policy and storage.
#[derive(Clone)]
pub struct NodeRequestAuthority {
    policy: Arc<ApplicationPolicy>,
    registry: SqliteRequestRegistry,
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
        Ok(Self { policy, registry })
    }
    pub(super) fn registry_scope(&self) -> RegistryOwnerScope {
        self.registry.owner_scope()
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
        _target: EntityId,
        _action: QueueAction,
    ) -> Result<ActorContext, RegistryError> {
        self.current(actor).await
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
