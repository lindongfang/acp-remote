//! Session business values; these types do not authenticate network callers.
pub mod approvals;
pub mod history;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

/// A platform identifier, independent of native protocol identifiers.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EntityId([u8; 16]);
impl EntityId {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
    /// Parses the platform's canonical identifier representation.
    /// # Errors
    /// Rejects malformed or non-canonical identifiers.
    pub fn decode(value: &str) -> Result<Self, SessionError> {
        let bytes = URL_SAFE_NO_PAD
            .decode(value)
            .map_err(|_| SessionError::CorruptState)?;
        let bytes: [u8; 16] = bytes.try_into().map_err(|_| SessionError::CorruptState)?;
        let result = Self(bytes);
        if result.encode() != value {
            return Err(SessionError::CorruptState);
        }
        Ok(result)
    }
    #[must_use]
    pub fn encode(self) -> String {
        URL_SAFE_NO_PAD.encode(self.0)
    }
}

/// An internal repository context, never an authentication or approval grant.
/// Network application adapters must supply an already verified actor while holding its gate.
#[derive(Clone, Copy, Debug)]
pub struct ActorContext {
    pub client_id: EntityId,
    pub auth_generation: i64,
}

#[derive(Clone, Copy, Debug)]
pub struct InputRequest {
    pub request_id: EntityId,
    pub session_id: EntityId,
    pub content_ref: EntityId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedInput {
    pub command_id: String,
    pub run_id: String,
    pub turn_id: String,
    pub item_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandStatus {
    Accepted,
    Dispatching,
    Completed,
    Failed,
    DeliveryUnknown,
}
impl CommandStatus {
    #[must_use]
    pub fn from_storage(value: &str) -> Option<Self> {
        match value {
            "accepted" => Some(Self::Accepted),
            "dispatching" => Some(Self::Dispatching),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "delivery_unknown" => Some(Self::DeliveryUnknown),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandRecord {
    pub input: AcceptedInput,
    pub status: CommandStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Admission {
    pub command: CommandRecord,
    pub replayed: bool,
}

/// Not Clone: an execution owner can consume a single durable dispatch claim.
#[derive(Debug)]
pub struct DispatchPermit {
    pub(crate) input: AcceptedInput,
}
impl DispatchPermit {
    #[must_use]
    pub fn into_input(self) -> AcceptedInput {
        self.input
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("session actor is not current")]
    InvalidActor,
    #[error("session resource not found")]
    NotFound,
    #[error("request identifier conflicts with prior content")]
    RequestConflict,
    #[error("session has an active foreground run")]
    Busy,
    #[error("session does not accept input in its current state")]
    InvalidState,
    #[error("session command capacity is exhausted")]
    CapacityExhausted,
    #[error("session content is invalid or incomplete")]
    InvalidContent,
    #[error("session identifier generation is unavailable")]
    RandomUnavailable,
    #[error("session storage is unavailable")]
    StorageUnavailable,
    #[error("session stored state is invalid")]
    CorruptState,
}
