//! Body-free approval data. Identifiers and snapshots never authorize native IO.
use super::{
    EntityId,
    native_dispatch::{ProtocolVersion, TransportProgress, TransportReason},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ApprovalDataError {
    #[error("invalid approval identifier")]
    InvalidIdentifier,
}

macro_rules! identifier {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub struct $name(EntityId);
        impl $name {
            #[must_use]
            pub const fn from_entity(value: EntityId) -> Self {
                Self(value)
            }
            #[must_use]
            pub const fn entity(self) -> EntityId {
                self.0
            }
            /// # Errors
            /// Rejects noncanonical platform identifier data; this performs no authorization.
            pub fn decode(value: &str) -> Result<Self, ApprovalDataError> {
                EntityId::decode(value)
                    .map(Self)
                    .map_err(|_| ApprovalDataError::InvalidIdentifier)
            }
            #[must_use]
            pub fn encode(self) -> String {
                self.0.encode()
            }
        }
    };
}
identifier!(ApprovalId);
identifier!(ChoiceId);
identifier!(ApprovalRegistrationId);
identifier!(ApprovalReplyId);

/// Exact original request and advertised option data; this hash is never an IO certificate.
/// # Errors
/// Rejects unrepresentable lengths without imposing a content or option-count quota.
pub fn choice_fingerprint(
    request_digest: [u8; 32],
    ordinal: usize,
    original_option_id: &str,
) -> Result<[u8; 32], ApprovalRejection> {
    use sha2::{Digest, Sha256};
    let ordinal = u64::try_from(ordinal).map_err(|_| ApprovalRejection::LengthUnrepresentable)?;
    let length = u64::try_from(original_option_id.len())
        .map_err(|_| ApprovalRejection::LengthUnrepresentable)?;
    let mut digest = Sha256::new();
    digest.update(b"acpr-approval-choice-v1\0");
    digest.update(request_digest);
    digest.update(ordinal.to_be_bytes());
    digest.update(length.to_be_bytes());
    digest.update(original_option_id.as_bytes());
    Ok(digest.finalize().into())
}

#[derive(Clone, Copy, Debug)]
pub struct RegisterDecision {
    approval: ApprovalId,
    choice: ChoiceId,
}
impl RegisterDecision {
    #[must_use]
    pub const fn new(approval: ApprovalId, choice: ChoiceId) -> Self {
        Self { approval, choice }
    }
    #[must_use]
    pub const fn approval(&self) -> ApprovalId {
        self.approval
    }
    #[must_use]
    pub const fn choice(&self) -> ChoiceId {
        self.choice
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SubmitDecision {
    registration: ApprovalRegistrationId,
}
impl SubmitDecision {
    #[must_use]
    pub const fn new(registration: ApprovalRegistrationId) -> Self {
        Self { registration }
    }
    #[must_use]
    pub const fn registration(&self) -> ApprovalRegistrationId {
        self.registration
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ApprovalRejection {
    #[error("approval actor is invalid")]
    InvalidActor,
    #[error("approval registration is unknown")]
    UnknownRegistration,
    #[error("approval metadata conflicts")]
    Conflict,
    #[error("approval already has a decision")]
    AlreadyDecided,
    #[error("original approval request is invalid")]
    InvalidRequest,
    #[error("approval target is not associated")]
    UnassociatedTarget,
    #[error("approval choice is not advertised")]
    InvalidChoice,
    #[error("approval metadata length cannot be represented")]
    LengthUnrepresentable,
    #[error("approval owner is stopped")]
    Stopped,
    #[error("necessary approval storage is unavailable")]
    NecessaryStorageUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum NecessaryWriteError {
    #[error(transparent)]
    Rejected(ApprovalRejection),
    #[error("approval commit confirmation is unavailable")]
    CommitUnconfirmed,
    #[error("approval SQL close is unconfirmed (commit acknowledged: {commit_confirmed})")]
    SqlCloseUnconfirmed { commit_confirmed: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnownOptionKind {
    AllowOnce,
    AllowAlways,
    RejectOnce,
    RejectAlways,
}
impl KnownOptionKind {
    #[must_use]
    pub fn classify(value: &str) -> Option<Self> {
        match value {
            "allow_once" => Some(Self::AllowOnce),
            "allow_always" => Some(Self::AllowAlways),
            "reject_once" => Some(Self::RejectOnce),
            "reject_always" => Some(Self::RejectAlways),
            _ => None,
        }
    }
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AllowOnce => "allow_once",
            Self::AllowAlways => "allow_always",
            Self::RejectOnce => "reject_once",
            Self::RejectAlways => "reject_always",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitState {
    NotStarted,
    Pending,
    NotCommitted,
    Confirmed,
    Unknown,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseCloseState {
    NotAcquired,
    Pending,
    Confirmed,
    Unconfirmed,
    Unavailable,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SqlTaskJoinState {
    NotStarted,
    Pending,
    Joined,
    Failed,
    Unavailable,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseReportState {
    NotReported,
    Pending,
    Confirmed,
    Unconfirmed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqlConfirmation {
    pub sql_operation: Option<EntityId>,
    pub commit: CommitState,
    pub lease_close: LeaseCloseState,
    pub close_report: CloseReportState,
    pub task_join: SqlTaskJoinState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssociationDisplay {
    Unassociated,
    KnownPre {
        target: EntityId,
        load_epoch: EntityId,
        owner: EntityId,
    },
    RegisteredSession {
        target: EntityId,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalSourceDisplay {
    pub source_id: EntityId,
    pub runtime: EntityId,
    pub instance: EntityId,
    pub connection_epoch: EntityId,
    pub scope_binding: EntityId,
    pub protocol_version: ProtocolVersion,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApprovalContextDisplay {
    SessionRequest,
    VerifiedInput { dispatch: EntityId },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayLimitation {
    UnknownSubject,
    UnknownOptionKind,
    TargetUnassociated,
    MalformedAdvertisement,
    OriginalUnavailable,
    RequestInvalid,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActualReplyKind {
    Selected,
    Cancelled,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplyOrigin {
    UserDecision,
    ProtocolCancel,
    ManagementFailure,
    SourceClosing,
    TargetClosing,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalReplyObservation {
    Zero {
        reason: TransportReason,
    },
    Written,
    Unknown {
        accepted: u64,
        cause: TransportReason,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SuppressionReason {
    RequestInvalid,
    SourceInvalid,
    OriginalUnavailable,
    SafetyWon,
}
#[derive(Clone, Debug)]
pub struct DecisionSummary {
    pub approval: ApprovalId,
    pub registration: ApprovalRegistrationId,
    pub operation: EntityId,
    pub actor: EntityId,
    pub admission_auth_generation: i64,
    pub selected_choice: Option<ChoiceId>,
    pub option_digest: [u8; 32],
    pub known_kind: Option<KnownOptionKind>,
    pub accepted_at_ms: i64,
    pub fingerprint: [u8; 32],
    pub save: SqlConfirmation,
}
#[derive(Clone, Debug)]
pub enum DecisionAcceptance {
    Accepted(DecisionSummary),
    Existing(DecisionSummary),
    Rejected(ApprovalRejection),
    ConfirmationUnknown { commit_confirmed: bool },
}
#[derive(Clone, Debug)]
pub enum DeliverySummary {
    NotAttempted,
    InProgress {
        response: ApprovalReplyId,
    },
    Terminal {
        response: ApprovalReplyId,
        actual: ActualReplyKind,
        observation: TerminalReplyObservation,
        record: SqlConfirmation,
    },
    Suppressed {
        reason: SuppressionReason,
        record: SqlConfirmation,
    },
}
#[derive(Clone, Debug)]
pub struct AttemptSummary {
    pub response: ApprovalReplyId,
    pub approval: ApprovalId,
    pub decision_operation: Option<EntityId>,
    pub serial: u64,
    pub kind: ActualReplyKind,
    pub origin: ReplyOrigin,
    pub option_digest: Option<[u8; 32]>,
    pub frame_length: u64,
    pub transport_progress: Option<TransportProgress>,
    pub arm: SqlConfirmation,
    pub terminal: Option<TerminalReplyObservation>,
    pub terminal_save: SqlConfirmation,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestValidityDisplay {
    CurrentPending,
    ResponseReserved,
    Spent,
    Invalid,
    Unknown,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalInvalidationReason {
    RequestCancelled,
    ResponseSpent,
    ConnectionLost,
    SourceClosed,
    TargetClosed,
    VerifiedNativeRequestEnded,
    NodeRestart,
}
#[derive(Clone, Debug)]
pub struct InvalidationSummary {
    pub id: EntityId,
    pub reason: ApprovalInvalidationReason,
    pub fingerprint: [u8; 32],
    pub observed_sequence: Option<u64>,
    pub observed_at_ms: i64,
    pub recorded_at_ms: i64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceCloseDisplay {
    Open,
    Closing,
    PhysicalUnconfirmed,
    MetadataUnconfirmed,
    Confirmed,
}
#[derive(Clone, Debug)]
pub struct ApprovalStatus {
    pub approval: ApprovalId,
    pub change_sequence: u64,
    pub validity: RequestValidityDisplay,
    pub request_save: SqlConfirmation,
    pub association: AssociationDisplay,
    pub decision: Option<DecisionSummary>,
    pub decision_confirmation: SqlConfirmation,
    pub delivery: DeliverySummary,
    pub attempts: Vec<AttemptSummary>,
    pub invalidations: Vec<InvalidationSummary>,
    pub source_close: SourceCloseDisplay,
    pub necessary_facts_missing: bool,
    pub owned_reconciliation_running: bool,
}
