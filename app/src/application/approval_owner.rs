//! Current actor admission and readonly visibility, separate from original native request authority.
use crate::{
    application::{native_dispatch::BoxFuture, request_registry::RegistryOwnerScope},
    domain::{
        ActorContext, EntityId,
        approval_owner::{ApprovalRegistrationId, ApprovalRejection},
        native::NativeSessionKey,
    },
};

#[derive(Clone)]
pub struct CurrentApprovalActor {
    pub(crate) context: ActorContext,
    pub(crate) target: EntityId,
    pub(crate) native: NativeSessionKey,
    pub(crate) owner: RegistryOwnerScope,
    pub(crate) control_root: Option<NativeSessionKey>,
    pub(crate) control_epoch: Option<u64>,
}
pub struct CurrentApprovalReader {
    pub(crate) context: ActorContext,
    pub(crate) owner: RegistryOwnerScope,
}
pub trait ApprovalAuthority: Send + Sync + 'static {
    type Actor: Send + Sync;
    /// # Errors
    /// Returns an approval rejection when the node no longer owns the actor scope.
    fn verify_owner_scope(&self, owner: &RegistryOwnerScope) -> Result<(), ApprovalRejection>;
    fn authorize_decision<'a>(
        &'a self,
        actor: &'a Self::Actor,
        target: EntityId,
    ) -> BoxFuture<'a, Result<CurrentApprovalActor, ApprovalRejection>>;
    fn authorize_decision_with_epoch<'a>(
        &'a self,
        actor: &'a Self::Actor,
        target: EntityId,
        _expected_control_epoch: Option<u64>,
    ) -> BoxFuture<'a, Result<CurrentApprovalActor, ApprovalRejection>> {
        self.authorize_decision(actor, target)
    }
    fn record_approval_registration(
        &self,
        _actor: &CurrentApprovalActor,
        _registration: ApprovalRegistrationId,
    ) {
    }
    /// # Errors
    /// Returns an approval rejection when session control changed before commit.
    fn verify_approval_decide(
        &self,
        _actor: &CurrentApprovalActor,
        _registration: ApprovalRegistrationId,
        _expected_control_epoch: Option<u64>,
    ) -> Result<(), ApprovalRejection> {
        Ok(())
    }
    fn authorize_read<'a>(
        &'a self,
        actor: &'a Self::Actor,
    ) -> BoxFuture<'a, Result<CurrentApprovalReader, ApprovalRejection>>;
}
