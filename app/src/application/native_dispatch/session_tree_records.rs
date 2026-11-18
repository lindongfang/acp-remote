use super::{Arc, BoxFuture, DispatchError, NativeSessionKey, PreSourceProbe};
use crate::application::request_registry::RegistryDeadline;

pub struct SessionParentProof {
    pub(super) source: Arc<PreSourceProbe>,
    pub(super) parent: NativeSessionKey,
    pub(super) child: NativeSessionKey,
    pub(super) sequence: u64,
    pub(super) receipt: u64,
    pub(super) fingerprint: [u8; 32],
    pub(super) observed_at_ms: i64,
}

impl SessionParentProof {
    pub(crate) fn source(&self) -> &PreSourceProbe {
        &self.source
    }

    pub(crate) const fn parent(&self) -> &NativeSessionKey {
        &self.parent
    }

    pub(crate) const fn child(&self) -> &NativeSessionKey {
        &self.child
    }

    pub(crate) const fn sequence(&self) -> u64 {
        self.sequence
    }

    pub(crate) const fn receipt(&self) -> u64 {
        self.receipt
    }

    pub(crate) const fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
    }

    pub(crate) const fn observed_at_ms(&self) -> i64 {
        self.observed_at_ms
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionTreePath {
    pub subject: NativeSessionKey,
    pub ancestors: Vec<NativeSessionKey>,
    pub root: Option<NativeSessionKey>,
}

pub trait SessionTreeRepository: Send + Sync + 'static {
    fn record_session_parent<'a>(
        &'a self,
        proof: &'a SessionParentProof,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<SessionTreePath, DispatchError>>;

    fn resolve_session_tree<'a>(
        &'a self,
        native: &'a NativeSessionKey,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<SessionTreePath, DispatchError>>;

    fn session_tree_members<'a>(
        &'a self,
        root: &'a NativeSessionKey,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<Vec<SessionTreePath>, DispatchError>>;
}

#[cfg(test)]
pub const fn session_parent_development_fixture(
    source: Arc<PreSourceProbe>,
    parent: NativeSessionKey,
    child: NativeSessionKey,
    sequence: u64,
) -> SessionParentProof {
    SessionParentProof {
        source,
        parent,
        child,
        sequence,
        receipt: sequence,
        fingerprint: [3; 32],
        observed_at_ms: 1,
    }
}
