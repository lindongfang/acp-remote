//! Body-free necessary approval records. Records are not native write capabilities.
use super::{
    EntityId,
    approval_owner::{
        ActualReplyKind, ApprovalId, ApprovalInvalidationReason, ApprovalRegistrationId,
        ApprovalReplyId, KnownOptionKind, ReplyOrigin, SqlConfirmation, TerminalReplyObservation,
    },
    native_dispatch::ProtocolVersion,
};
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalRequestRecord {
    pub approval: ApprovalId,
    pub obligation: EntityId,
    pub source_id: EntityId,
    pub runtime: EntityId,
    pub instance: EntityId,
    pub connection_epoch: EntityId,
    pub scope_binding: EntityId,
    pub rpc_kind: StoredRpcKind,
    pub rpc_digest: [u8; 32],
    pub native_session_digest: [u8; 32],
    pub request_digest: [u8; 32],
    pub protocol: ProtocolVersion,
    pub observed_sequence: u64,
    pub observed_receipt: u64,
    pub observed_at_ms: i64,
    pub verified_dispatch: Option<EntityId>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoredRpcKind {
    Integer,
    String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalAssociationRecord {
    pub approval: ApprovalId,
    pub target: EntityId,
    pub kind: StoredAssociationKind,
    pub load_epoch: Option<EntityId>,
    pub owner: Option<EntityId>,
    pub fingerprint: [u8; 32],
    pub associated_at_ms: i64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoredAssociationKind {
    KnownPre,
    RegisteredSession,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalCommandRecord {
    pub registration: ApprovalRegistrationId,
    pub approval: ApprovalId,
    pub target: EntityId,
    pub actor: EntityId,
    pub registration_auth_generation: i64,
    pub option_digest: [u8; 32],
    pub intent_fingerprint: [u8; 32],
    pub runtime: EntityId,
    pub registered_at_ms: i64,
    pub state: StoredApprovalCommandState,
    pub operation: Option<EntityId>,
    pub admission_auth_generation: Option<i64>,
    pub admitted_at_ms: Option<i64>,
    pub rejection: Option<StoredApprovalRejection>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoredApprovalCommandState {
    Registered,
    Accepted,
    Rejected,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoredApprovalRejection {
    AlreadyDecided,
    RequestInvalid,
    OptionInvalid,
    SourceInvalid,
    ActorInvalid,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalDecisionRecord {
    pub approval: ApprovalId,
    pub registration: ApprovalRegistrationId,
    pub operation: EntityId,
    pub actor: EntityId,
    pub admission_auth_generation: i64,
    pub option_digest: [u8; 32],
    pub known_kind: Option<KnownOptionKind>,
    pub authorization_fingerprint: [u8; 32],
    pub fingerprint: [u8; 32],
    pub accepted_at_ms: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalAttemptRecord {
    pub response: ApprovalReplyId,
    pub approval: ApprovalId,
    pub decision_operation: Option<EntityId>,
    pub serial: u64,
    pub kind: ActualReplyKind,
    pub origin: ReplyOrigin,
    pub option_digest: Option<[u8; 32]>,
    pub frame_length: u64,
    pub reserved_fingerprint: [u8; 32],
    pub observed_at_ms: i64,
    pub recorded_at_ms: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalTerminalRecord {
    pub response: ApprovalReplyId,
    pub approval: ApprovalId,
    pub observation: TerminalReplyObservation,
    pub accepted_length: u64,
    pub fingerprint: [u8; 32],
    pub observed_at_ms: i64,
    pub recorded_at_ms: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalInvalidationRecord {
    pub id: EntityId,
    pub approval: ApprovalId,
    pub reason: ApprovalInvalidationReason,
    pub fingerprint: [u8; 32],
    pub observed_sequence: Option<u64>,
    pub observed_at_ms: i64,
    pub recorded_at_ms: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredApproval<T> {
    record: T,
    confirmation: SqlConfirmation,
    record_fingerprint: [u8; 32],
}
impl<T> StoredApproval<T> {
    pub const fn record(&self) -> &T {
        &self.record
    }
    pub const fn confirmation(&self) -> &SqlConfirmation {
        &self.confirmation
    }
    pub const fn record_fingerprint(&self) -> &[u8; 32] {
        &self.record_fingerprint
    }
    pub(crate) const fn from_exact_owner_result(
        record: T,
        confirmation: SqlConfirmation,
        fingerprint: [u8; 32],
    ) -> Self {
        Self {
            record,
            confirmation,
            record_fingerprint: fingerprint,
        }
    }
}
pub type StoredApprovalRequest = StoredApproval<ApprovalRequestRecord>;
pub type StoredApprovalAssociation = StoredApproval<ApprovalAssociationRecord>;
pub type StoredApprovalCommand = StoredApproval<ApprovalCommandRecord>;
pub type StoredApprovalDecision = StoredApproval<ApprovalDecisionRecord>;
pub type StoredApprovalAttempt = StoredApproval<ApprovalAttemptRecord>;
pub type StoredApprovalTerminal = StoredApproval<ApprovalTerminalRecord>;
pub type StoredApprovalInvalidation = StoredApproval<ApprovalInvalidationRecord>;
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalStoredInspection {
    pub request: Option<ApprovalRequestRecord>,
    pub association: Option<ApprovalAssociationRecord>,
    pub commands: Vec<ApprovalCommandRecord>,
    pub decision: Option<ApprovalDecisionRecord>,
    pub attempts: Vec<ApprovalAttemptRecord>,
    pub terminals: Vec<ApprovalTerminalRecord>,
    pub invalidations: Vec<ApprovalInvalidationRecord>,
    pub read_confirmation: SqlConfirmation,
}
