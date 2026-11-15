//! Original queue witnesses share resource installation without sharing admission semantics.
use super::{AcceptedTargetProof, Arc, DispatchError, EntityId, NativeSessionKey};
use crate::application::queue_owner::{NativeQueueLink, ReadTargetProof};

pub(super) enum ResourceTargetOrigin {
    Dispatch(AcceptedTargetProof),
    Read(ReadTargetProof),
}
impl ResourceTargetOrigin {
    pub(super) const fn target(&self) -> EntityId {
        match self {
            Self::Dispatch(proof) => proof.target(),
            Self::Read(proof) => proof.target(),
        }
    }
    pub(super) const fn native(&self) -> &NativeSessionKey {
        match self {
            Self::Dispatch(proof) => proof.native(),
            Self::Read(proof) => proof.native(),
        }
    }
    pub(super) fn owner(&self) -> EntityId {
        match self {
            Self::Dispatch(proof) => proof.owner(),
            Self::Read(proof) => proof.owner(),
        }
    }
    pub(super) fn link(&self) -> Arc<dyn NativeQueueLink> {
        match self {
            Self::Dispatch(proof) => Arc::clone(&proof.link),
            Self::Read(proof) => Arc::clone(&proof.link),
        }
    }
    pub(super) fn require_current(&self) -> Result<(), DispatchError> {
        match self {
            Self::Dispatch(proof) => proof.require_current(),
            Self::Read(proof) => proof.require_current(),
        }
    }
}
