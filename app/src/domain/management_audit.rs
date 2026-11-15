//! Supplemental facts only; none of these values authorize an action or delivery.
use super::{
    EntityId,
    native::RuntimeEpoch,
    request_registry::{QueueAction, QueueOutcome, RegistrationId},
};

pub const DEFAULT_RETENTION_MS: i64 = 2_592_000_000;
pub const AUDIT_LIMIT: i64 = 100_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuditRecordId(RegistrationId);
impl AuditRecordId {
    pub(crate) const fn settled(request: RegistrationId) -> Self {
        Self(request)
    }
    pub(crate) fn encode(self) -> String {
        format!("queue.settled:{}", self.0.encode())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditReceipt {
    pub(crate) id: AuditRecordId,
    pub(crate) recorded_at_ms: i64,
    pub(crate) retain_until_ms: i64,
    pub(crate) duplicate: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditStoreError {
    Full,
    Cancelled,
    InvalidSource,
    Conflict,
    Clock,
    Layout,
    Sql,
    NecessaryIntegrity,
    CommitUnconfirmed,
    CloseUnconfirmed { committed: bool },
}
impl std::fmt::Display for AuditStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "supplemental audit: {self:?}")
    }
}
impl std::error::Error for AuditStoreError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettledAuditBasis {
    pub(crate) request: RegistrationId,
    pub(crate) operation: EntityId,
    pub(crate) actor: EntityId,
    pub(crate) action: QueueAction,
    pub(crate) target: EntityId,
    pub(crate) register_generation: i64,
    pub(crate) admission_generation: i64,
    pub(crate) runtime: RuntimeEpoch,
    pub(crate) outcome: QueueOutcome,
    pub(crate) event_at_ms: i64,
}

pub fn retention(recorded: i64) -> Result<i64, AuditStoreError> {
    if recorded < 0 {
        return Err(AuditStoreError::Clock);
    }
    recorded
        .checked_add(DEFAULT_RETENTION_MS)
        .ok_or(AuditStoreError::Clock)
}
