//! Original-request permission reply ports. Data identities never construct write authority.
use super::{
    Arc, BoxFuture, DispatchError, DispatchFenceRejection, DispatchPollIo, DispatchWriteStep,
    EncodedPromptFrame, EnqueueRejected, NativeControlPort, PreSourceProbe, RpcScalar,
};
use crate::domain::{
    EntityId,
    approval_owner::{ApprovalId, ApprovalReplyId},
};
use std::task::Context;
use tokio::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionPendingSnapshot {
    Waiting,
    Reserved,
    SpentOrInvalid,
}

pub struct NativePermissionDisplay<R> {
    pub raw_request: R,
    pub supported_subject: bool,
}

pub struct PermissionSelectedBinding<S> {
    pub(super) scope: S,
    pub(super) source: Arc<PreSourceProbe>,
    pub(super) approval: ApprovalId,
    pub(super) decision: EntityId,
    pub(super) response: ApprovalReplyId,
    pub(super) rpc: RpcScalar,
    pub(super) option_digest: [u8; 32],
}
impl<S> PermissionSelectedBinding<S> {
    #[must_use]
    pub const fn scope(&self) -> &S {
        &self.scope
    }
    #[must_use]
    pub fn source(&self) -> &PreSourceProbe {
        &self.source
    }
    #[must_use]
    pub const fn approval(&self) -> ApprovalId {
        self.approval
    }
    #[must_use]
    pub const fn decision(&self) -> EntityId {
        self.decision
    }
    #[must_use]
    pub const fn response(&self) -> ApprovalReplyId {
        self.response
    }
    #[must_use]
    pub const fn rpc(&self) -> &RpcScalar {
        &self.rpc
    }
    #[must_use]
    pub const fn option_digest(&self) -> &[u8; 32] {
        &self.option_digest
    }
}

#[derive(Clone)]
pub struct PermissionReplyIdentity<S> {
    scope: S,
    rpc: RpcScalar,
    native_session: Arc<str>,
    option_digest: [u8; 32],
}
impl<S> PermissionReplyIdentity<S> {
    #[must_use]
    pub const fn scope(&self) -> &S {
        &self.scope
    }
    #[must_use]
    pub const fn rpc(&self) -> &RpcScalar {
        &self.rpc
    }
    #[must_use]
    pub fn native_session(&self) -> &str {
        &self.native_session
    }
    #[must_use]
    pub const fn option_digest(&self) -> &[u8; 32] {
        &self.option_digest
    }
    pub(crate) const fn from_actual(
        scope: S,
        rpc: RpcScalar,
        native_session: Arc<str>,
        option_digest: [u8; 32],
    ) -> Self {
        Self {
            scope,
            rpc,
            native_session,
            option_digest,
        }
    }
}

pub trait ReservedPermissionReply: Send + Sync + 'static {
    type Scope: Clone + Eq + Send + Sync + 'static;
    type Frame: EncodedPromptFrame;
    type Probe: Clone + Send + Sync + 'static;
    fn identity(&self) -> &PermissionReplyIdentity<Self::Scope>;
    fn probe(&self) -> Self::Probe;
    /// # Errors
    /// Encoding failure retains the original reservation with its existing owner.
    fn encode(&self) -> Result<Self::Frame, DispatchError>;
}

pub trait PermissionReplyBoundary<S>: Send + Sync {
    /// # Errors
    /// Rejects stale original ownership or request state before the same actual IO poll.
    fn check_and_poll(
        &self,
        identity: &PermissionReplyIdentity<S>,
        step: DispatchWriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn DispatchPollIo,
    ) -> Result<(), DispatchFenceRejection>;
}

pub struct CancelledPermissionReplyReservation<S, Q> {
    identity: PermissionReplyIdentity<S>,
    probe: Q,
}
impl<S, Q> CancelledPermissionReplyReservation<S, Q> {
    #[must_use]
    pub const fn identity(&self) -> &PermissionReplyIdentity<S> {
        &self.identity
    }
    #[must_use]
    pub const fn probe(&self) -> &Q {
        &self.probe
    }
    pub(crate) const fn from_actual(identity: PermissionReplyIdentity<S>, probe: Q) -> Self {
        Self { identity, probe }
    }
}

pub struct PermissionFailureCancelBinding<S> {
    pub(super) scope: S,
    pub(super) rpc: RpcScalar,
    pub(super) source: Arc<PreSourceProbe>,
    pub(super) obligation: EntityId,
}
impl<S> PermissionFailureCancelBinding<S> {
    pub const fn scope(&self) -> &S {
        &self.scope
    }
    pub const fn rpc(&self) -> &RpcScalar {
        &self.rpc
    }
    pub fn source(&self) -> &PreSourceProbe {
        &self.source
    }
    pub const fn obligation(&self) -> EntityId {
        self.obligation
    }
}
pub trait NativeApprovalPort: NativeControlPort {
    /// # Errors
    /// Rejects a mismatched original request; only the private actual-save-failure path constructs the binding.
    fn reserve_permission_failure_cancelled(
        &self,
        source: &Self::Source,
        original: &Self::PermissionRequest,
        binding: &PermissionFailureCancelBinding<Self::Scope>,
    ) -> Result<Self::ControlReservation, DispatchError>;
    type PermissionChoice: Clone + Send + Sync + 'static;
    type PermissionReplyReservation: ReservedPermissionReply<Scope = Self::Scope, Frame = Self::Frame, Probe = Self::Probe>;
    /// # Errors
    /// Rejects a request without its original branded source.
    fn permission_choices(
        &self,
        original: &Self::PermissionRequest,
    ) -> Result<Vec<Self::PermissionChoice>, DispatchError>;
    fn permission_choice_id<'a>(&self, choice: &'a Self::PermissionChoice) -> &'a str;
    fn permission_choice_kind<'a>(&self, choice: &'a Self::PermissionChoice) -> &'a str;
    fn permission_request_digest(&self, original: &Self::PermissionRequest) -> [u8; 32];
    /// # Errors
    /// Rejects invalid original source branding without changing the pending response slot.
    fn permission_display(
        &self,
        original: &Self::PermissionRequest,
    ) -> Result<NativePermissionDisplay<Self::RawEvent>, DispatchError>;
    /// # Errors
    /// Rejects foreign source/request identity. The returned snapshot performs no IO or reservation.
    fn inspect_permission(
        &self,
        source: &Self::Source,
        original: &Self::PermissionRequest,
    ) -> Result<PermissionPendingSnapshot, DispatchError>;
    /// # Errors
    /// Rejects any stale source, original pending slot or non-unique original advertised selection.
    fn reserve_permission_selected(
        &self,
        source: &Self::Source,
        original: &Self::PermissionRequest,
        original_choice: &Self::PermissionChoice,
        binding: &PermissionSelectedBinding<Self::Scope>,
    ) -> Result<Self::PermissionReplyReservation, DispatchError>;
    /// # Errors
    /// Returns both original inputs if the actual source writer rejects enqueue.
    fn enqueue_permission_reply(
        &self,
        source: &Self::Source,
        reservation: Self::PermissionReplyReservation,
        frame: Self::Frame,
        boundary: Arc<dyn PermissionReplyBoundary<Self::Scope>>,
        deadline: Instant,
    ) -> Result<Self::Ticket, EnqueueRejected<Self::PermissionReplyReservation, Self::Frame>>;
    fn cancel_reserved_permission_reply(
        &self,
        reservation: Self::PermissionReplyReservation,
    ) -> CancelledPermissionReplyReservation<Self::Scope, Self::Probe>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalCleanupObservation {
    Pending,
    ConfirmedJoined,
    UnconfirmedJoined,
}
pub trait ApprovalSqlCleanup: Send + Sync {
    fn original_owner_scope(&self) -> crate::application::request_registry::RegistryOwnerScope;
    fn confirm_original_cleanup(&self, until: Instant)
    -> BoxFuture<'_, ApprovalCleanupObservation>;
}
