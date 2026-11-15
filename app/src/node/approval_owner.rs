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
    domain::{EntityId, approval_owner::ApprovalRejection},
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
        Box::pin(async move {
            let context = self
                .current(actor)
                .await
                .map_err(|_| ApprovalRejection::InvalidActor)?;
            let native = self
                .resolve_target(target)
                .await
                .map_err(|_| ApprovalRejection::UnassociatedTarget)?;
            Ok(CurrentApprovalActor {
                context,
                target,
                native,
                owner: self.registry_scope(),
            })
        })
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
