//! Metadata-only input commands. None of these values authenticate a network caller.
use super::{
    EntityId,
    native::{NativeSessionKey, ResourceEpoch},
    requests::{MAX_REQUEST_TIME, ProductAction, ProductTarget, ProductTargetKind, RequestIntent},
};
use sha2::{Digest, Sha256};

pub const COMMAND_LIMIT: u64 = 10_000;
pub const AUDIT_LIMIT: u64 = 100_000;
pub const AUDIT_RETENTION_MS: u64 = 7_776_000_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputTarget {
    pub target: EntityId,
    pub native: NativeSessionKey,
    pub epoch: ResourceEpoch,
    pub instance: EntityId,
    pub revision: u64,
}

/// Fixed field array, independent of the gateway object's field order.
/// # Errors
/// Rejects failure to encode the closed field array.
pub fn input_intent(
    actor: EntityId,
    target: &InputTarget,
    content_digest: [u8; 32],
) -> Result<RequestIntent, ProductCommandError> {
    let fields = [
        target.target.encode(),
        target.native.source().0.encode(),
        target.native.native_id().to_owned(),
        hex::encode(content_digest),
    ];
    let encoded = serde_json::to_vec(&fields).map_err(|_| ProductCommandError::InvalidInput)?;
    Ok(RequestIntent {
        actor,
        action: ProductAction::SessionInput,
        target: ProductTarget {
            kind: ProductTargetKind::Session,
            id: target.target,
        },
        payload_digest: Sha256::digest(encoded).into(),
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductCommandStatus {
    Accepted,
    Dispatching,
    Completed,
    Failed,
    Unknown,
    Expired,
}
impl ProductCommandStatus {
    pub(crate) fn parse(s: &str) -> Result<Self, ProductCommandError> {
        match s {
            "accepted" => Ok(Self::Accepted),
            "dispatching" => Ok(Self::Dispatching),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "unknown" => Ok(Self::Unknown),
            "expired" => Ok(Self::Expired),
            _ => Err(ProductCommandError::CorruptState),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductCommandFact {
    pub command: EntityId,
    pub run: EntityId,
    pub target: EntityId,
    pub status: ProductCommandStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputResult {
    Completed,
    Cancelled,
    Rejected,
    Unknown,
    Expired,
    NotSent,
}
impl InputResult {
    pub(crate) const fn labels(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Completed => ("completed", "completed", "completed"),
            Self::Cancelled => ("completed", "cancelled", "cancelled"),
            Self::Rejected | Self::NotSent => ("failed", "failed", "failed"),
            Self::Unknown => ("unknown", "unknown", "unknown"),
            Self::Expired => ("expired", "failed", "expired"),
        }
    }
}

/// Only an execution owner can mint this receipt, not a query or a request DTO.
pub struct InputReceipt {
    pub(crate) command: EntityId,
    pub(crate) run: EntityId,
    pub(crate) claim: EntityId,
    pub(crate) target: InputTarget,
    pub(crate) owner_nonce: EntityId,
}
impl InputReceipt {
    #[must_use]
    pub const fn command(&self) -> EntityId {
        self.command
    }
    #[must_use]
    pub const fn run(&self) -> EntityId {
        self.run
    }
}

/// Checked arithmetic is independently testable; SQL always reserves exactly four stages.
#[must_use]
pub const fn audit_room(count: u64, limit: u64) -> bool {
    match count.checked_add(4) {
        Some(next) => next <= limit,
        None => false,
    }
}

pub(crate) fn retention(now: u64) -> Result<u64, ProductCommandError> {
    now.checked_add(AUDIT_RETENTION_MS)
        .filter(|v| *v <= MAX_REQUEST_TIME)
        .ok_or(ProductCommandError::ClockUnreliable)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProductCommandError {
    #[error("product actor is not current")]
    InvalidActor,
    #[error("product target is not current")]
    InvalidTarget,
    #[error("native foreground is unavailable")]
    Busy,
    #[error("product request conflicts with an earlier identity")]
    Conflict,
    #[error("product request identity is invalid")]
    InvalidIdentity,
    #[error("product request has expired")]
    Expired,
    #[error("product request epoch is no longer current")]
    EpochMismatch,
    #[error("product request time is unreliable")]
    ClockUnreliable,
    #[error("product command capacity is exhausted")]
    Capacity,
    #[error("input is incomplete or exceeds its budget")]
    InvalidInput,
    #[error("temporary input has expired")]
    InputExpired,
    #[error("product storage is unavailable or unconfirmed")]
    StorageUnavailable,
    #[error("product stored state is invalid")]
    CorruptState,
    #[error("product operation was cancelled or timed out")]
    Cancelled,
    #[error("product dispatch qualification is not current")]
    InvalidTicket,
    #[error("product random source is unavailable")]
    RandomUnavailable,
}
