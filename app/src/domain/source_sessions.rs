//! Small source-management facts. These values never authorize IO or mint native bindings.
use super::native_dispatch::{ProtocolVersion, RpcScalar, TransportFact};
use super::{
    EntityId,
    native::RuntimeEpoch,
    native_catalog::{CleanupKnowledge, ProfileKey, TransactionKnowledge},
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SourceSessionOperationId(EntityId);
impl SourceSessionOperationId {
    #[must_use]
    pub const fn new(id: EntityId) -> Self {
        Self(id)
    }
    #[must_use]
    pub const fn entity(self) -> EntityId {
        self.0
    }
    #[must_use]
    pub fn encode(self) -> String {
        self.0.encode()
    }
    /// Decodes a serialized operation ID.
    ///
    /// # Errors
    /// Returns an error if the encoded identifier is invalid.
    pub fn decode(value: &str) -> Result<Self, SourceSessionError> {
        EntityId::decode(value)
            .map(Self)
            .map_err(|_| SourceSessionError::CorruptState)
    }
}
#[derive(Clone, Eq, PartialEq)]
pub struct SourceLaunchSelection {
    pub source: EntityId,
    pub harness: EntityId,
    pub profile: ProfileKey,
    pub workspace: EntityId,
    pub launch_path: String,
}
impl std::fmt::Debug for SourceLaunchSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceLaunchSelection")
            .field("source", &self.source)
            .field("harness", &self.harness)
            .field("profile", &self.profile)
            .field("workspace", &self.workspace)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Eq, PartialEq)]
pub enum SourceSessionAction {
    Discover { cursor: Option<String> },
    Create,
}
impl SourceSessionAction {
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Discover { .. } => "discover",
            Self::Create => "create",
        }
    }
    #[must_use]
    pub fn cursor(&self) -> Option<&str> {
        match self {
            Self::Discover { cursor } => cursor.as_deref(),
            Self::Create => None,
        }
    }
}
impl std::fmt::Debug for SourceSessionAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSessionRegistration {
    pub id: SourceSessionOperationId,
    pub actor: EntityId,
    pub auth_generation: i64,
    pub selection: SourceLaunchSelection,
    pub action: SourceSessionAction,
    pub intent_digest: [u8; 32],
    pub registered_at_ms: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSessionRegistrationMetadata {
    pub id: SourceSessionOperationId,
    pub actor: EntityId,
    pub source: EntityId,
    pub harness: EntityId,
    pub profile: ProfileKey,
    pub workspace: EntityId,
    pub action: &'static str,
    pub registered_at_ms: i64,
}
impl SourceSessionRegistration {
    #[must_use]
    pub const fn metadata(&self) -> SourceSessionRegistrationMetadata {
        SourceSessionRegistrationMetadata {
            id: self.id,
            actor: self.actor,
            source: self.selection.source,
            harness: self.selection.harness,
            profile: self.selection.profile,
            workspace: self.selection.workspace,
            action: self.action.label(),
            registered_at_ms: self.registered_at_ms,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceSessionState {
    Registered,
    Owned,
    Terminal,
    Interrupted,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceSessionOutcome {
    Pending,
    Succeeded,
    Refused,
    Unknown,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceResponseKind {
    Created,
    Sessions,
    RemoteError,
}
impl SourceResponseKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Sessions => "sessions",
            Self::RemoteError => "remote_error",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceAckState {
    Pending,
    Confirmed,
    Rejected,
    Unknown,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceFailureClass {
    Configuration,
    Protocol,
    Source,
    Metadata,
    Ownership,
    Allocation,
    Interrupted,
}
impl SourceFailureClass {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::Protocol => "protocol",
            Self::Source => "source",
            Self::Metadata => "metadata",
            Self::Ownership => "ownership",
            Self::Allocation => "allocation",
            Self::Interrupted => "interrupted",
        }
    }
}
/// Necessary response metadata only. No raw response, Listing or Document lease.
#[derive(Clone, Eq, PartialEq)]
pub struct SourceResponseFact {
    pub kind: SourceResponseKind,
    pub native_session_id: Option<Arc<str>>,
    pub native_key_available: bool,
    pub remote_error_code: Option<i64>,
    pub sequence: u64,
    pub receipt: u64,
    pub evidence: [u8; 32],
    pub result_bytes: u64,
    pub ack: SourceAckState,
}
impl std::fmt::Debug for SourceResponseFact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceResponseFact")
            .field("kind", &self.kind)
            .field("native_key_available", &self.native_key_available)
            .field("remote_error_code", &self.remote_error_code)
            .field("sequence", &self.sequence)
            .field("receipt", &self.receipt)
            .field("result_bytes", &self.result_bytes)
            .field("ack", &self.ack)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSessionFact {
    pub registration: SourceSessionRegistrationMetadata,
    pub result_available: bool,
    pub state: SourceSessionState,
    pub outcome: SourceSessionOutcome,
    pub runtime: Option<RuntimeEpoch>,
    pub admission_auth_generation: Option<i64>,
    pub source_instance: Option<EntityId>,
    pub connection_epoch: Option<EntityId>,
    pub wire: Option<ProtocolVersion>,
    pub rpc: Option<RpcScalar>,
    pub frame_length: Option<u64>,
    pub transport: Option<TransportFact>,
    pub response: Option<SourceResponseFact>,
    pub failure: Option<SourceFailureClass>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceSqlRole {
    Register,
    Admit,
    SourceAttach,
    RpcReserve,
    Arm,
    Transport,
    Response,
    Finish,
    Query,
    Reconcile,
    Recover,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceSqlObservation {
    pub role: SourceSqlRole,
    pub transaction: TransactionKnowledge,
    pub cleanup: CleanupKnowledge,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceTaskObservation {
    pub installed: bool,
    pub joined: bool,
    pub failed: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSessionSqlFailure {
    pub kind: SourceSessionError,
    pub operation: Option<SourceSessionOperationId>,
    pub observation: Option<SourceSqlObservation>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceSessionFailure {
    Configuration(super::native_catalog::CatalogFailure),
    Native(super::native_dispatch::DispatchError),
    NecessarySql(SourceSessionSqlFailure),
    WorkerUnconfirmed {
        operation: Option<SourceSessionOperationId>,
        observation: SourceTaskObservation,
        sql: Option<SourceSqlObservation>,
    },
    Rejected(SourceSessionError),
}
impl std::fmt::Display for SourceSessionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Configuration(error) => std::fmt::Display::fmt(error, f),
            Self::Native(error) => std::fmt::Display::fmt(error, f),
            Self::NecessarySql(error) => std::fmt::Display::fmt(&error.kind, f),
            Self::WorkerUnconfirmed { .. } => f.write_str("source session worker is unconfirmed"),
            Self::Rejected(error) => std::fmt::Display::fmt(error, f),
        }
    }
}
impl std::error::Error for SourceSessionFailure {}
impl From<SourceSessionError> for SourceSessionFailure {
    fn from(error: SourceSessionError) -> Self {
        Self::Rejected(error)
    }
}
impl From<super::native_catalog::CatalogFailure> for SourceSessionFailure {
    fn from(error: super::native_catalog::CatalogFailure) -> Self {
        Self::Configuration(error)
    }
}
impl From<super::native_dispatch::DispatchError> for SourceSessionFailure {
    fn from(error: super::native_dispatch::DispatchError) -> Self {
        Self::Native(error)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SourceSessionError {
    #[error("source session actor is not current")]
    InvalidActor,
    #[error("source session identity or scope is invalid")]
    InvalidScope,
    #[error("source session selection is invalid")]
    InvalidSelection,
    #[error("source session operation is unknown")]
    UnknownOperation,
    #[error("source session operation conflicts")]
    Conflict,
    #[error("source session state is corrupt")]
    CorruptState,
    #[error("source session storage is unavailable")]
    StorageUnavailable,
    #[error("source session commit is unconfirmed")]
    CommitUnconfirmed,
    #[error("source session SQL close is unconfirmed")]
    SqlCloseUnconfirmed,
    #[error("source session worker is unconfirmed")]
    WorkerUnconfirmed,
    #[error("source session protocol failed")]
    Protocol,
    #[error("source session configuration failed")]
    Configuration,
    #[error("source session native ownership is unconfirmed")]
    OwnershipUnconfirmed,
    #[error("source session result is unavailable")]
    ResultUnavailable,
    #[error("source session allocation failed")]
    Allocation,
    #[error("source session random source is unavailable")]
    RandomUnavailable,
    #[error("source session node is stopping")]
    NodeStopping,
}
