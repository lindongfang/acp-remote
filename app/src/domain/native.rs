//! Native identity is independent of executable paths, credentials and connection lifetimes.
use super::EntityId;
use std::{fmt, sync::Arc};

macro_rules! epoch {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub struct $name(pub EntityId);
    };
}
epoch!(RequestEpoch);
epoch!(RuntimeEpoch);
epoch!(HarnessConnectionEpoch);
epoch!(LoadEpoch);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ResourceEpoch {
    pub runtime: RuntimeEpoch,
    pub connection: HarnessConnectionEpoch,
    pub load: LoadEpoch,
}

/// An explicitly configured namespace, not proof of physical storage equivalence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeSourceId(pub EntityId);

#[derive(Clone, Eq, Hash, PartialEq)]
pub struct NativeSessionKey {
    source: NativeSourceId,
    native_id: Arc<str>,
}

impl NativeSessionKey {
    /// # Errors
    /// Requires an explicit source and an opaque, nonempty bounded protocol identifier.
    pub fn from_config(
        source: Option<EntityId>,
        native_id: &str,
    ) -> Result<Self, NativeIdentityError> {
        let source = NativeSourceId(source.ok_or(NativeIdentityError::MissingSource)?);
        if native_id.is_empty() || native_id.len() > 4096 {
            return Err(NativeIdentityError::InvalidNativeId);
        }
        Ok(Self {
            source,
            native_id: Arc::from(native_id),
        })
    }

    #[must_use]
    pub const fn source(&self) -> NativeSourceId {
        self.source
    }

    #[must_use]
    pub fn native_id(&self) -> &str {
        &self.native_id
    }
}

impl fmt::Debug for NativeSessionKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeSessionKey")
            .field("source", &self.source)
            .field("native_id_bytes", &self.native_id.len())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum NativeIdentityError {
    #[error("native source must be explicitly configured")]
    MissingSource,
    #[error("native session identifier is invalid")]
    InvalidNativeId,
}
