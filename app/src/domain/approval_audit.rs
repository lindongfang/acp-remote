//! Supplemental body-free receipts are data and never approval or delivery capabilities.
use super::{EntityId, approval_owner::ApprovalId};
use std::sync::Arc;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalAuditEventKind {
    Registered,
    Decided,
    Delivery,
    Invalidated,
}
impl ApprovalAuditEventKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Registered => "approval.registered",
            Self::Decided => "approval.decided",
            Self::Delivery => "approval.delivery",
            Self::Invalidated => "approval.invalidated",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalAuditWarning {
    Full,
    Sql,
    CommitUnconfirmed,
    CloseUnconfirmed,
    InvalidBasis,
    Stopped,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalAuditOutcome {
    Registered,
    Accepted,
    Rejected,
    Zero,
    Written,
    Unknown,
    Suppressed,
    Invalidated,
}
impl ApprovalAuditOutcome {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Registered => "registered",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Zero => "zero",
            Self::Written => "written",
            Self::Unknown => "unknown",
            Self::Suppressed => "suppressed",
            Self::Invalidated => "invalidated",
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalAuditBasisMetadata {
    pub approval: ApprovalId,
    pub event: ApprovalAuditEventKind,
    pub basis_fingerprint: [u8; 32],
    pub actor: Option<EntityId>,
    pub target: Option<EntityId>,
    pub outcome: ApprovalAuditOutcome,
    pub occurred_at_ms: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalAuditReceipt {
    pub(crate) record_id: Arc<str>,
    pub(crate) approval: ApprovalId,
    pub(crate) event: ApprovalAuditEventKind,
    pub(crate) basis_fingerprint: [u8; 32],
    pub(crate) outcome: ApprovalAuditOutcome,
    pub(crate) recorded_at_ms: i64,
    pub(crate) retain_until_ms: i64,
    pub(crate) duplicate: bool,
}
impl ApprovalAuditReceipt {
    #[must_use]
    pub fn record_id(&self) -> &str {
        &self.record_id
    }
    #[must_use]
    pub const fn approval(&self) -> ApprovalId {
        self.approval
    }
    #[must_use]
    pub const fn event(&self) -> &ApprovalAuditEventKind {
        &self.event
    }
    #[must_use]
    pub const fn basis_fingerprint(&self) -> &[u8; 32] {
        &self.basis_fingerprint
    }
    #[must_use]
    pub const fn outcome(&self) -> &ApprovalAuditOutcome {
        &self.outcome
    }
    #[must_use]
    pub const fn recorded_at_ms(&self) -> i64 {
        self.recorded_at_ms
    }
    #[must_use]
    pub const fn retain_until_ms(&self) -> i64 {
        self.retain_until_ms
    }
    #[must_use]
    pub const fn duplicate(&self) -> bool {
        self.duplicate
    }
}
