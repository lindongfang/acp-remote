//! Session business values; these types do not authenticate network callers.
pub mod activity;
pub mod approval_audit;
pub mod approval_owner;
pub mod approval_records;
pub mod history_page;
pub mod input_queue;
pub(crate) mod management_audit;
pub mod native;
pub mod native_catalog;
pub mod native_dispatch;
#[cfg(test)]
pub mod product_commands;
pub mod request_registry;
pub mod requests;
pub mod session_capacity;
pub mod shared_projection;
pub mod shared_view;
pub(crate) mod shared_view_log;
pub mod source_sessions;
pub mod views;
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

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("session stored state is invalid")]
    CorruptState,
}
