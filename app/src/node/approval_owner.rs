//! Approval assembly uses this Node's original authenticated issuer, registry and Native owner.
use super::{NodeRequestAuthority, NodeRuntime};
use crate::{
    access_policy::AuthenticatedActor,
    application::{
        approval_owner::{ApprovalAuthority, CurrentApprovalActor, CurrentApprovalReader},
        native_dispatch::{
            BoxFuture, ConfigurationPort, NativeApprovalPort, NativeApprovalService,
            NativeRuntimeHandle, OwnedNativeSource, SourceLaunchPort,
        },
        request_registry::{RegistryOwnerScope, RequestAuthority},
    },
    domain::{
        EntityId,
        approval_owner::{ApprovalRegistrationId, ApprovalRejection},
        request_registry::RegistryError,
    },
    session_storage::native_dispatch::SqliteNativeDispatch,
};
use std::sync::Arc;
impl ApprovalAuthority for NodeRequestAuthority {
    type Actor = AuthenticatedActor;
    fn verify_owner_scope(&self, owner: &RegistryOwnerScope) -> Result<(), ApprovalRejection> {
        RequestAuthority::verify_owner_scope(self, owner).map_err(|_| ApprovalRejection::Stopped)
    }
    fn authorize_decision<'a>(
        &'a self,
        actor: &'a Self::Actor,
        target: EntityId,
    ) -> BoxFuture<'a, Result<CurrentApprovalActor, ApprovalRejection>> {
        self.authorize_decision_with_epoch(actor, target, None)
    }
    fn authorize_decision_with_epoch<'a>(
        &'a self,
        actor: &'a Self::Actor,
        target: EntityId,
        expected_control_epoch: Option<u64>,
    ) -> BoxFuture<'a, Result<CurrentApprovalActor, ApprovalRejection>> {
        Box::pin(async move {
            let context = self
                .current(actor)
                .await
                .map_err(|_| ApprovalRejection::InvalidActor)?;
            let native = self
                .resolve_target(target)
                .await
                .map_err(|_| ApprovalRejection::UnassociatedTarget)?;
            let resolved = self
                .resolve_control_root(target)
                .await
                .map_err(|err| match err {
                    RegistryError::SessionTreeUnverified => {
                        ApprovalRejection::SessionTreeUnverified
                    }
                    RegistryError::InvalidActor => ApprovalRejection::InvalidActor,
                    _ => ApprovalRejection::UnassociatedTarget,
                })?;
            let control_epoch = self
                .session_control()
                .authorize_controlled(&resolved.root, context.client_id, expected_control_epoch)
                .map_err(|_| ApprovalRejection::SessionNotControlled)?;
            Ok(CurrentApprovalActor {
                context,
                target,
                native,
                owner: self.registry_scope(),
                control_root: Some(resolved.root),
                control_epoch: Some(control_epoch),
            })
        })
    }
    fn record_approval_registration(
        &self,
        actor: &CurrentApprovalActor,
        registration: ApprovalRegistrationId,
    ) {
        if let (Some(root), Some(epoch)) = (actor.control_root.as_ref(), actor.control_epoch) {
            self.session_control().bind_approval_registration(
                registration.entity(),
                root,
                actor.context.client_id,
                epoch,
            );
        }
    }
    fn verify_approval_decide(
        &self,
        actor: &CurrentApprovalActor,
        registration: ApprovalRegistrationId,
        expected_control_epoch: Option<u64>,
    ) -> Result<(), ApprovalRejection> {
        let Some(root) = actor.control_root.as_ref() else {
            return Err(ApprovalRejection::SessionNotControlled);
        };
        self.session_control()
            .verify_approval_decide(
                registration.entity(),
                root,
                actor.context.client_id,
                expected_control_epoch,
            )
            .map(|_| ())
            .map_err(|_| ApprovalRejection::SessionNotControlled)
    }
    fn authorize_read<'a>(
        &'a self,
        actor: &'a Self::Actor,
    ) -> BoxFuture<'a, Result<CurrentApprovalReader, ApprovalRejection>> {
        Box::pin(async move {
            let context = self
                .current(actor)
                .await
                .map_err(|_| ApprovalRejection::InvalidActor)?;
            Ok(CurrentApprovalReader {
                context,
                owner: self.registry_scope(),
            })
        })
    }
}
pub type NodeApprovalService<P, L, G, N> = NativeApprovalService<NodeRequestAuthority, P, L, G, N>;
impl NodeRuntime {
    /// Binds this Node's authenticated issuer to the same original Native SQL owner.
    /// # Errors
    /// Rejects a foreign or stopped owner without constructing a service.
    pub fn approval_owner<P, L, G>(
        &self,
        native: &NativeRuntimeHandle<P, L, G, SqliteNativeDispatch>,
    ) -> Result<NodeApprovalService<P, L, G, SqliteNativeDispatch>, ApprovalRejection>
    where
        P: NativeApprovalPort,
        L: SourceLaunchPort,
        L::Owned: OwnedNativeSource<Streams = P::Streams>,
        G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    {
        let authority = self
            .request_authority()
            .map_err(|_| ApprovalRejection::Stopped)?;
        NativeApprovalService::create(native.clone(), Arc::new(authority))
    }
}
