//! Pure catalog data. None of these values authenticate callers or grant launch authority.
use super::EntityId;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CatalogRequestId(EntityId);
impl CatalogRequestId {
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
    /// # Errors
    /// Rejects noncanonical identifiers.
    pub fn decode(value: &str) -> Result<Self, CatalogError> {
        EntityId::decode(value)
            .map(Self)
            .map_err(|_| CatalogError::InvalidConfiguration)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProfileKey {
    id: EntityId,
    revision: u64,
}
impl ProfileKey {
    /// # Errors
    /// Rejects revision zero.
    pub const fn try_new(id: EntityId, revision: u64) -> Result<Self, CatalogError> {
        if revision == 0 {
            Err(CatalogError::InvalidConfiguration)
        } else {
            Ok(Self { id, revision })
        }
    }
    #[must_use]
    pub const fn id(self) -> EntityId {
        self.id
    }
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogAction {
    InstallHarness,
    PublishProfile,
    BindTargetInitial,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogState {
    Registered,
    Committed,
    Rejected,
}
pub enum SourceSelection {
    Existing(EntityId),
    New { name: String },
}
pub enum ProfilePublication {
    New,
    Next { expected: ProfileKey },
}
/// Complete owned format-one document. Intentionally has no Debug or mutable accessor.
pub struct LaunchProfileDraft {
    bytes: Vec<u8>,
}
impl LaunchProfileDraft {
    pub(crate) const fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
    #[must_use]
    pub fn document(&self) -> &[u8] {
        &self.bytes
    }
}
pub struct CatalogDraft {
    intent: DraftIntent,
}
pub(crate) enum DraftIntent {
    InstallHarness {
        name: String,
        source: SourceSelection,
        profile: LaunchProfileDraft,
    },
    PublishProfile {
        harness: EntityId,
        source: EntityId,
        publication: ProfilePublication,
        profile: LaunchProfileDraft,
    },
    BindTargetInitial {
        target: EntityId,
        source: EntityId,
        harness: EntityId,
        profile: ProfileKey,
        workspace: EntityId,
        launch_path: String,
        observed_canonical_path: String,
    },
}
impl CatalogDraft {
    /// # Errors
    /// Rejects names that cannot be stored in the current catalog representation.
    pub fn install_harness(
        name: String,
        source: SourceSelection,
        profile: LaunchProfileDraft,
    ) -> Result<Self, CatalogError> {
        validate_name(&name)?;
        if let SourceSelection::New { name } = &source {
            validate_name(name)?;
        }
        Ok(Self {
            intent: DraftIntent::InstallHarness {
                name,
                source,
                profile,
            },
        })
    }
    /// # Errors
    /// Rejects exhausted immutable profile revisions.
    pub fn publish_profile(
        harness: EntityId,
        source: EntityId,
        publication: ProfilePublication,
        profile: LaunchProfileDraft,
    ) -> Result<Self, CatalogError> {
        if let ProfilePublication::Next { expected } = &publication {
            expected
                .revision
                .checked_add(1)
                .ok_or(CatalogError::Conflict)?;
        }
        Ok(Self {
            intent: DraftIntent::PublishProfile {
                harness,
                source,
                publication,
                profile,
            },
        })
    }
    /// # Errors
    /// Rejects empty or NUL-bearing explicit path data; this does not check the filesystem.
    pub fn bind_target_initial(
        target: EntityId,
        source: EntityId,
        harness: EntityId,
        profile: ProfileKey,
        workspace: EntityId,
        launch_path: String,
        observed_canonical_path: String,
    ) -> Result<Self, CatalogError> {
        if [&launch_path, &observed_canonical_path]
            .iter()
            .any(|p| p.is_empty() || p.contains('\0'))
        {
            return Err(CatalogError::InvalidConfiguration);
        }
        Ok(Self {
            intent: DraftIntent::BindTargetInitial {
                target,
                source,
                harness,
                profile,
                workspace,
                launch_path,
                observed_canonical_path,
            },
        })
    }
    #[must_use]
    pub const fn action(&self) -> CatalogAction {
        match self.intent {
            DraftIntent::InstallHarness { .. } => CatalogAction::InstallHarness,
            DraftIntent::PublishProfile { .. } => CatalogAction::PublishProfile,
            DraftIntent::BindTargetInitial { .. } => CatalogAction::BindTargetInitial,
        }
    }
    pub(crate) fn into_intent(self) -> DraftIntent {
        self.intent
    }
}
fn validate_name(name: &str) -> Result<(), CatalogError> {
    if name.is_empty() || name.chars().count() > 80 || name.contains('\0') {
        Err(CatalogError::InvalidConfiguration)
    } else {
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditWarning {
    None,
    Unavailable,
    Full,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitKnowledge {
    NoWrite,
    WritePossible,
    Confirmed,
    RolledBack,
    Unknown,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanupKnowledge {
    NoDetachedConnection,
    PendingOriginal,
    ConfirmedOriginal,
    UnconfirmedOriginal,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogSqlRole {
    IdentityRead,
    RegisteredSnapshot,
    RegisterWrite,
    ApplyWrite,
    QueryRead,
    ConfigurationRead,
    RecoveryRead,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransactionKnowledge {
    NoTransaction,
    BegunRead,
    BegunWrite,
    CommitPolling,
    CommitConfirmed,
    RollbackConfirmed,
    EffectUnknown,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CatalogSqlObservation {
    role: CatalogSqlRole,
    transaction: TransactionKnowledge,
    cleanup: CleanupKnowledge,
}
impl CatalogSqlObservation {
    pub(crate) const fn new(
        role: CatalogSqlRole,
        transaction: TransactionKnowledge,
        cleanup: CleanupKnowledge,
    ) -> Self {
        Self {
            role,
            transaction,
            cleanup,
        }
    }
    #[must_use]
    pub const fn role(&self) -> CatalogSqlRole {
        self.role
    }
    #[must_use]
    pub const fn transaction(&self) -> TransactionKnowledge {
        self.transaction
    }
    #[must_use]
    pub const fn cleanup(&self) -> CleanupKnowledge {
        self.cleanup
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogOperationKind {
    Register,
    Apply,
    Query,
    Resolve,
    ResolveRead,
    Reconcile,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogSqlObservationSet {
    operation: CatalogOperationKind,
    slots: [Option<CatalogSqlObservation>; 2],
}
impl CatalogSqlObservationSet {
    pub(crate) const fn new(
        operation: CatalogOperationKind,
        slots: [Option<CatalogSqlObservation>; 2],
    ) -> Self {
        Self { operation, slots }
    }
    #[must_use]
    pub const fn operation_kind(&self) -> CatalogOperationKind {
        self.operation
    }
    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.slots.iter().flatten().count()
    }
    #[must_use]
    pub fn slot(&self, index: usize) -> Option<&CatalogSqlObservation> {
        self.slots.get(index).and_then(Option::as_ref)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CatalogError {
    #[error("catalog actor is not current")]
    InvalidActor,
    #[error("catalog source is invalid")]
    InvalidSource,
    #[error("catalog target is invalid")]
    InvalidTarget,
    #[error("catalog request is unknown")]
    UnknownRequest,
    #[error("catalog configuration is invalid")]
    InvalidConfiguration,
    #[error("catalog state conflicts")]
    Conflict,
    #[error("catalog stored state is invalid")]
    CorruptState,
    #[error("catalog random source is unavailable")]
    RandomUnavailable,
    #[error("catalog allocation failed")]
    Allocation,
    #[error("catalog protection is unavailable")]
    ProtectionUnavailable,
    #[error("catalog storage is unavailable")]
    StorageUnavailable,
    #[error("catalog operation was cancelled")]
    Cancelled,
    #[error("catalog node is stopping")]
    NodeStopping,
    #[error("catalog commit is unconfirmed")]
    CommitUnconfirmed,
    #[error("catalog original SQL close is unconfirmed")]
    SqlCloseUnconfirmed { committed: bool },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogFailure {
    kind: CatalogError,
    request: Option<CatalogRequestId>,
    commit: CommitKnowledge,
    cleanup: CleanupKnowledge,
    sql: CatalogSqlObservationSet,
}
impl CatalogFailure {
    pub(crate) const fn new(
        kind: CatalogError,
        request: Option<CatalogRequestId>,
        commit: CommitKnowledge,
        cleanup: CleanupKnowledge,
        sql: CatalogSqlObservationSet,
    ) -> Self {
        Self {
            kind,
            request,
            commit,
            cleanup,
            sql,
        }
    }
    #[must_use]
    pub const fn kind(&self) -> CatalogError {
        self.kind
    }
    #[must_use]
    pub const fn request_id(&self) -> Option<CatalogRequestId> {
        self.request
    }
    #[must_use]
    pub const fn commit(&self) -> CommitKnowledge {
        self.commit
    }
    #[must_use]
    pub const fn cleanup(&self) -> CleanupKnowledge {
        self.cleanup
    }
    #[must_use]
    pub const fn sql_observation(&self) -> &CatalogSqlObservationSet {
        &self.sql
    }
}
impl std::fmt::Display for CatalogFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.kind.fmt(f)
    }
}
impl std::error::Error for CatalogFailure {}
impl From<super::request_registry::RegistryError> for CatalogError {
    fn from(error: super::request_registry::RegistryError) -> Self {
        use super::request_registry::RegistryError as R;
        match error {
            R::InvalidActor => Self::InvalidActor,
            R::InvalidTarget => Self::InvalidTarget,
            R::UnknownRegistration => Self::UnknownRequest,
            R::Conflict => Self::Conflict,
            R::CorruptState => Self::CorruptState,
            R::RandomUnavailable => Self::RandomUnavailable,
            R::CommitUnconfirmed => Self::CommitUnconfirmed,
            R::Cancelled => Self::Cancelled,
            R::NodeStopping => Self::NodeStopping,
            _ => Self::StorageUnavailable,
        }
    }
}
impl From<CatalogError> for super::native_dispatch::DispatchError {
    fn from(error: CatalogError) -> Self {
        use super::native_dispatch::NativeProtocolFailure;
        match error {
            CatalogError::InvalidActor => Self::InvalidScope,
            CatalogError::InvalidSource => Self::InvalidSource,
            CatalogError::InvalidTarget => Self::InvalidTarget,
            CatalogError::Conflict => Self::Conflict,
            CatalogError::CorruptState => Self::CorruptState,
            CatalogError::Allocation => Self::ProtocolFault(NativeProtocolFailure::Allocation),
            CatalogError::Cancelled => Self::Stopped,
            CatalogError::NodeStopping => Self::NodeStopping,
            CatalogError::CommitUnconfirmed => Self::CommitUnconfirmed,
            CatalogError::SqlCloseUnconfirmed { committed } => Self::SqlCloseUnconfirmed {
                commit_confirmed: committed,
            },
            CatalogError::UnknownRequest | CatalogError::InvalidConfiguration => {
                Self::InvalidConfiguration
            }
            _ => Self::StorageUnavailable,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogResult {
    Harness {
        harness: EntityId,
        source: EntityId,
        profile: ProfileKey,
    },
    Profile {
        harness: EntityId,
        source: EntityId,
        profile: ProfileKey,
    },
    Target {
        target: EntityId,
        workspace: EntityId,
        harness: EntityId,
        source: EntityId,
        profile: ProfileKey,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogFact {
    request: CatalogRequestId,
    actor: EntityId,
    action: CatalogAction,
    state: CatalogState,
    registered_at_ms: i64,
    register_generation: i64,
    admission_generation: Option<i64>,
    result_at_ms: Option<i64>,
    result: Option<CatalogResult>,
}
impl CatalogFact {
    pub(crate) fn from_record(
        request: CatalogRequestId,
        actor: EntityId,
        action: CatalogAction,
        state: CatalogState,
        registration: (i64, i64),
        admission: Option<(i64, i64)>,
        result: Option<CatalogResult>,
    ) -> Self {
        Self {
            request,
            actor,
            action,
            state,
            registered_at_ms: registration.0,
            register_generation: registration.1,
            admission_generation: admission.map(|v| v.0),
            result_at_ms: admission.map(|v| v.1),
            result,
        }
    }
    #[must_use]
    pub const fn request_id(&self) -> CatalogRequestId {
        self.request
    }
    #[must_use]
    pub const fn actor_id(&self) -> EntityId {
        self.actor
    }
    #[must_use]
    pub const fn action(&self) -> CatalogAction {
        self.action
    }
    #[must_use]
    pub const fn state(&self) -> CatalogState {
        self.state
    }
    #[must_use]
    pub const fn registered_at_ms(&self) -> i64 {
        self.registered_at_ms
    }
    #[must_use]
    pub const fn register_auth_generation(&self) -> i64 {
        self.register_generation
    }
    #[must_use]
    pub const fn admission_auth_generation(&self) -> Option<i64> {
        self.admission_generation
    }
    #[must_use]
    pub const fn result_at_ms(&self) -> Option<i64> {
        self.result_at_ms
    }
    #[must_use]
    pub const fn result(&self) -> Option<&CatalogResult> {
        self.result.as_ref()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogReceipt {
    fact: CatalogFact,
    warning: AuditWarning,
    sql: CatalogSqlObservationSet,
}
impl CatalogReceipt {
    pub(crate) const fn new(
        fact: CatalogFact,
        warning: AuditWarning,
        sql: CatalogSqlObservationSet,
    ) -> Self {
        Self { fact, warning, sql }
    }
    #[must_use]
    pub const fn fact(&self) -> &CatalogFact {
        &self.fact
    }
    #[must_use]
    pub const fn audit_warning(&self) -> AuditWarning {
        self.warning
    }
    #[must_use]
    pub const fn sql_observation(&self) -> &CatalogSqlObservationSet {
        &self.sql
    }
}

/// Safe registered workspace record without secret or profile plaintext.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceRecord {
    pub workspace_id: EntityId,
    pub canonical_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductSourceSummary {
    pub source_id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessSummary {
    pub harness_id: EntityId,
    pub name: String,
    pub source_id: EntityId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSummary {
    pub profile_id: EntityId,
    pub revision: u64,
    pub harness_id: EntityId,
    pub source_id: EntityId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetBindingSummary {
    pub profile_id: EntityId,
    pub profile_revision: u64,
    pub workspace_id: EntityId,
    pub launch_path: String,
    pub observed_canonical_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductTargetRecord {
    pub target_id: EntityId,
    pub source_id: EntityId,
    pub native_session_id: String,
    pub workspace_id: Option<EntityId>,
    pub binding: Option<TargetBindingSummary>,
}

/// Closed catalog metadata snapshot. Never includes `protected_payload` or launch secrets.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogInventory {
    pub workspaces: Vec<WorkspaceRecord>,
    pub sources: Vec<ProductSourceSummary>,
    pub harnesses: Vec<HarnessSummary>,
    pub profiles: Vec<ProfileSummary>,
    pub targets: Vec<ProductTargetRecord>,
}
