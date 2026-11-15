//! Cleanup capabilities distinguish never-entered, physical-owned and isolated target closure.
use super::{
    Arc, BoxFuture, DispatchError, EntityId, PreResourceProbe, PreSourceProbe, ResourceAuthority,
    ResourceMetadata, ResourceRecord, SourceRecord, SourceTerminalReason,
};
use crate::application::request_registry::{OperationContext, RegistryDeadline};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceInspection {
    pub source: Option<SourceRecord>,
    pub resources: Vec<ResourceRecord>,
}
pub struct SourceClosureProof {
    pub(super) approvals: Arc<super::ApprovalClosureManifest>,
    pub(super) source: Arc<PreSourceProbe>,
    pub(super) expected: SourceInspection,
    pub(super) basis: SourceCloseBasis,
    pub(super) stage: SourceCloseStage,
    pub(super) io: IoCloseKind,
    pub(super) owner: OwnerCloseKind,
    pub(super) owner_witness: Option<super::source_ports::OwnerClosureWitness>,
    pub(super) quiescence: Arc<super::source_jobs::SourceQuiescence>,
    pub(super) fingerprint: [u8; 32],
    pub(super) at: i64,
}
/// Opaque same-owner retirement authority; only the original closed-source actor constructs it.
pub struct RetiredResourceProof {
    pub(super) resource: Arc<PreResourceProbe>,
    pub(super) source: Arc<SourceClosureProof>,
    pub(super) convergence: Arc<super::ConvergenceProbe>,
    pub(super) epoch: crate::domain::native::ResourceEpoch,
}
impl RetiredResourceProof {
    pub(crate) fn require_current(
        &self,
    ) -> Result<(EntityId, crate::domain::native::ResourceEpoch), DispatchError> {
        let metadata = self.resource.metadata();
        if !self.source.quiescence.confirms(&self.source.source)
            || !Arc::ptr_eq(
                &self.source.source.authority,
                &self.resource.authority.source,
            )
            || !self
                .resource
                .authority
                .closed
                .load(super::Ordering::Acquire)
            || !Arc::ptr_eq(&self.convergence.authority, &self.resource.authority)
            || self.convergence.metadata.resource != *metadata
            || self.epoch.runtime != metadata.source.runtime
            || self.epoch.connection.0 != metadata.source.connection_epoch
            || self.epoch.load.0 != metadata.load_epoch
        {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        Ok((metadata.owner, self.epoch))
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceCloseBasis {
    NeverLaunched,
    OwnedConverged,
    EnteredNeverCreated,
}
impl SourceCloseBasis {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NeverLaunched => "never_launched",
            Self::OwnedConverged => "owned_converged",
            Self::EnteredNeverCreated => "entered_never_created",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceCloseStage {
    NeverEntered,
    EnteredBeforeCreate,
    CreateRejected,
    OwnedBeforeIo,
    IoUnstarted,
    SourceBound,
    Started,
    Initialized,
    LoadPending,
    Loaded,
}
impl SourceCloseStage {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NeverEntered => "never_entered",
            Self::EnteredBeforeCreate => "entered_before_create",
            Self::CreateRejected => "create_rejected",
            Self::OwnedBeforeIo => "owned_before_io",
            Self::IoUnstarted => "io_unstarted",
            Self::SourceBound => "source_bound",
            Self::Started => "started",
            Self::Initialized => "initialized",
            Self::LoadPending => "load_pending",
            Self::Loaded => "loaded",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IoCloseKind {
    NotApplicable,
    NotPrepared,
    BeforeAdoptRejected,
    AdoptedUnstartedJoined,
    StartedJoined,
}
impl IoCloseKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NotApplicable => "not_applicable",
            Self::NotPrepared => "not_prepared",
            Self::BeforeAdoptRejected => "before_adopt_rejected",
            Self::AdoptedUnstartedJoined => "adopted_unstarted_joined",
            Self::StartedJoined => "started_joined",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OwnerCloseKind {
    NotApplicable,
    WindowsJob { active_processes: u32 },
    TestOwnedStream,
    NoProcessCreated,
    SuspendedRootExited,
}
impl OwnerCloseKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NotApplicable => "not_applicable",
            Self::WindowsJob { .. } => "windows_job",
            Self::TestOwnedStream => "test_owned_stream",
            Self::NoProcessCreated => "no_process_created",
            Self::SuspendedRootExited => "suspended_root_exited",
        }
    }
    #[must_use]
    pub const fn active_processes(self) -> Option<u32> {
        match self {
            Self::WindowsJob { active_processes } => Some(active_processes),
            _ => None,
        }
    }
}
impl SourceClosureProof {
    #[must_use]
    pub fn approvals(&self) -> &super::ApprovalClosureManifest {
        &self.approvals
    }
    pub(crate) const fn owner_witness(&self) -> Option<&super::source_ports::OwnerClosureWitness> {
        self.owner_witness.as_ref()
    }
    #[must_use]
    pub fn source(&self) -> &PreSourceProbe {
        &self.source
    }
    #[must_use]
    pub const fn expected(&self) -> &SourceInspection {
        &self.expected
    }
    #[must_use]
    pub const fn basis(&self) -> SourceCloseBasis {
        self.basis
    }
    #[must_use]
    pub const fn stage(&self) -> SourceCloseStage {
        self.stage
    }
    #[must_use]
    pub const fn io(&self) -> IoCloseKind {
        self.io
    }
    #[must_use]
    pub const fn owner(&self) -> OwnerCloseKind {
        self.owner
    }
    #[must_use]
    pub const fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
    }
    #[must_use]
    pub const fn at(&self) -> i64 {
        self.at
    }
}
pub struct ResourceClosureProof {
    pub(super) resource: Arc<PreResourceProbe>,
    pub(super) expected: ResourceRecord,
    pub(super) basis: ResourceCloseBasis,
    pub(super) stage: ResourceCloseStage,
    pub(super) fingerprint: [u8; 32],
    pub(super) at: i64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceCloseBasis {
    UnloadedReservationAbandoned,
    TargetNativeConverged,
}
impl ResourceCloseBasis {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::UnloadedReservationAbandoned => "unloaded_reservation_abandoned",
            Self::TargetNativeConverged => "target_native_converged",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceCloseStage {
    ReservedNoTargetIo,
    BoundNoTargetIo,
    Loading,
    Loaded,
    Unknown,
}
impl ResourceCloseStage {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ReservedNoTargetIo => "reserved_no_target_io",
            Self::BoundNoTargetIo => "bound_no_target_io",
            Self::Loading => "loading",
            Self::Loaded => "loaded",
            Self::Unknown => "unknown",
        }
    }
}
impl ResourceClosureProof {
    #[must_use]
    pub fn resource(&self) -> &PreResourceProbe {
        &self.resource
    }
    #[must_use]
    pub const fn expected(&self) -> &ResourceRecord {
        &self.expected
    }
    #[must_use]
    pub const fn basis(&self) -> ResourceCloseBasis {
        self.basis
    }
    #[must_use]
    pub const fn stage(&self) -> ResourceCloseStage {
        self.stage
    }
    #[must_use]
    pub const fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
    }
    #[must_use]
    pub const fn at(&self) -> i64 {
        self.at
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredSourceClosed {
    pub source: SourceRecord,
    pub resources: Vec<ResourceRecord>,
}
pub struct SourceLossProbe {
    pub(super) source: Arc<PreSourceProbe>,
    pub(super) binding: EntityId,
    pub(super) loss_id: EntityId,
    pub(super) requirement: EntityId,
    pub(super) reason: SourceTerminalReason,
    pub(super) targets: Vec<LossTargetMetadata>,
    pub(super) fingerprint: [u8; 32],
    pub(super) at: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LossTargetMetadata {
    pub target: EntityId,
    pub load_epoch: EntityId,
    pub owner: EntityId,
    pub owner_order: u64,
    pub requirement: EntityId,
    pub fingerprint: [u8; 32],
}
impl SourceLossProbe {
    #[must_use]
    pub fn source(&self) -> &PreSourceProbe {
        &self.source
    }
    #[must_use]
    pub const fn binding(&self) -> EntityId {
        self.binding
    }
    #[must_use]
    pub const fn loss_id(&self) -> EntityId {
        self.loss_id
    }
    #[must_use]
    pub const fn requirement(&self) -> EntityId {
        self.requirement
    }
    #[must_use]
    pub const fn reason(&self) -> SourceTerminalReason {
        self.reason
    }
    #[must_use]
    pub fn targets(&self) -> &[LossTargetMetadata] {
        &self.targets
    }
    #[must_use]
    pub const fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
    }
    #[must_use]
    pub const fn at(&self) -> i64 {
        self.at
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredSourceLoss {
    pub loss_id: EntityId,
    pub requirement: EntityId,
    pub targets: Vec<LossTargetMetadata>,
    pub fingerprint: [u8; 32],
    pub at: i64,
}
pub trait NativeClosureRepository: Send + Sync + 'static {
    fn inspect_source_resources<'a>(
        &'a self,
        p: &'a PreSourceProbe,
        resources: &'a [Arc<PreResourceProbe>],
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<SourceInspection, DispatchError>>;
    fn record_source_closed<'a>(
        &'a self,
        p: &'a PreSourceProbe,
        f: &'a SourceClosureProof,
        c: &'a OperationContext,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredSourceClosed, DispatchError>>;
    fn record_resource_closed<'a>(
        &'a self,
        p: &'a PreResourceProbe,
        f: &'a ResourceClosureProof,
        c: &'a OperationContext,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<ResourceRecord, DispatchError>>;
    fn record_source_loss<'a>(
        &'a self,
        p: &'a SourceLossProbe,
        c: &'a OperationContext,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredSourceLoss, DispatchError>>;
    fn record_convergence<'a>(
        &'a self,
        p: &'a ConvergenceProbe,
        c: &'a OperationContext,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredConvergence, DispatchError>>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConvergenceKind {
    JoinedOwnedSource,
    NativeTargetClosedVerified,
    OldRuntimeStopped,
}
impl ConvergenceKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::JoinedOwnedSource => "joined_owned_source",
            Self::NativeTargetClosedVerified => "native_target_closed_verified",
            Self::OldRuntimeStopped => "old_runtime_stopped",
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvergenceMetadata {
    pub id: EntityId,
    pub requirement: EntityId,
    pub resource: ResourceMetadata,
    pub owner_order: u64,
    pub kind: ConvergenceKind,
    pub fingerprint: [u8; 32],
    pub at: i64,
}
pub struct ConvergenceProbe {
    pub(super) metadata: ConvergenceMetadata,
    pub(super) authority: Arc<ResourceAuthority>,
}
impl ConvergenceProbe {
    #[must_use]
    pub const fn metadata(&self) -> &ConvergenceMetadata {
        &self.metadata
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredConvergence {
    pub metadata: ConvergenceMetadata,
}
