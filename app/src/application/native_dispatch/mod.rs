//! Node-owned dispatch: protocol, metadata and queue capabilities remain separate.
use crate::domain::{
    EntityId,
    native::NativeSessionKey,
    native_dispatch::{
        DispatchError, DispatchPlan, KnownStopReason, NativeOwnerKind, ProtocolVersion,
        ResourceMetadata, RpcScalar, SelectionKind, SourceMetadata, TransportFact,
        TransportProgress,
    },
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::time::Instant;
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub use crate::application::queue_owner::{AcceptedTargetProof, RetainPrompt};
mod ports;
pub use ports::*;
mod approval_audit;
pub(crate) use approval_audit::ApprovalAuditNecessary;
pub use approval_audit::{ApprovalAuditBasis, ApprovalAuditPort};
mod approval_closure;
pub use approval_closure::ApprovalClosureManifest;
mod approval_control;
mod approval_failure;
mod approval_recovery;
#[cfg(all(
    test,
    any(
        feature = "approval-owner-dev-fixtures",
        feature = "approval-owner-acceptance-fixtures"
    )
))]
pub(crate) use approval_recovery::{
    ApprovalReconcileOutcome, ApprovalRepairMode, ApprovalRepairScope,
};
mod approval_repair;
mod approval_response;
mod approval_safety;
mod approval_watch;
pub use approval_watch::{ApprovalStatusWatch, ApprovalWatchError};
mod approval_service;
pub use approval_service::NativeApprovalService;
mod approval_association;
mod approval_inventory;
pub use approval_inventory::{ApprovalChoiceDisplay, ApprovalDisplay, ApprovalDisplayContent};
mod approval_records;
pub use approval_records::*;
mod approval_ports;
pub use approval_ports::*;
mod source_ports;
pub(crate) mod source_sessions;
pub use source_ports::*;
mod environment;
mod tasks;
pub use environment::NativeEnvironment;
mod dispatch_records;
pub use dispatch_records::*;
mod control_records;
pub use control_records::*;
mod closure_records;
pub use closure_records::*;
mod runtime;
pub use runtime::{
    NativeCoordinator, NativeRuntimeHandle, NativeRuntimeOptions, NativeRuntimeStatus,
    ResponseBudget,
};
mod control;
mod dispatch;
mod observation;
mod resource_close;
mod shutdown;
pub use shutdown::NativeShutdownReport;
pub(crate) mod read_access;
pub(crate) mod read_source;
mod resource_origin;
mod source;
pub(crate) mod view_attachment;
pub(crate) use read_access::{
    NativeReadResourceAccess, NativeReadTaskRegistrar, ViewSourceContext,
};
pub(crate) use read_source::SourceReadCapability;
pub(crate) use view_attachment::{
    AttachmentBuildGuard, NativeSourceViewFactory, NativeSourceViewOwner, ViewAttachmentClose,
};
mod source_jobs;
#[cfg(test)]
mod test_inspection;
#[cfg(test)]
pub use test_inspection::NativeParkSnapshot;
#[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
pub(crate) mod dev_tests;

pub struct PreSourceProbe {
    metadata: SourceMetadata,
    authority: Arc<SourceAuthority>,
}
pub struct PreResourceProbe {
    metadata: ResourceMetadata,
    native: NativeSessionKey,
    authority: Arc<ResourceAuthority>,
}
struct SourceAuthority {
    closed: AtomicBool,
    closing: AtomicBool,
    quarantined: AtomicBool,
    required: std::sync::atomic::AtomicUsize,
    gate: std::sync::Mutex<()>,
    #[cfg(windows)]
    result_epochs:
        std::sync::Mutex<Vec<std::sync::Weak<crate::application::document_source::DocumentEpoch>>>,
}
#[cfg(windows)]
impl SourceAuthority {
    // Caller holds the original Source gate. Only weak epochs are retained here.
    pub(super) fn invalidate_results(
        &self,
    ) -> Vec<crate::application::document_source::DocumentWake> {
        let epochs = std::mem::take(
            &mut *self
                .result_epochs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        epochs
            .into_iter()
            .filter_map(|epoch| epoch.upgrade())
            .map(|epoch| epoch.invalidate())
            .collect()
    }
}
struct ResourceAuthority {
    source: Arc<SourceAuthority>,
    closed: AtomicBool,
    releasing: AtomicBool,
}
impl PreSourceProbe {
    #[must_use]
    pub const fn metadata(&self) -> &SourceMetadata {
        &self.metadata
    }
}
impl PreResourceProbe {
    #[must_use]
    pub const fn metadata(&self) -> &ResourceMetadata {
        &self.metadata
    }
}
impl PreResourceProbe {
    #[must_use]
    pub const fn native(&self) -> &NativeSessionKey {
        &self.native
    }
}
impl PreSourceProbe {
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.authority, &other.authority) && self.metadata == other.metadata
    }
}
impl PreResourceProbe {
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.authority, &other.authority)
            && self.metadata == other.metadata
            && self.native == other.native
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceRecord {
    pub metadata: SourceMetadata,
    pub lifecycle: SourceLifecycle,
    pub scope_binding: Option<EntityId>,
    pub scope_bound_at: Option<i64>,
    pub wire: Option<ProtocolVersion>,
    pub initialize_rpc: Option<RpcScalar>,
    pub initialize_sequence: Option<u64>,
    pub initialize_receipt: Option<u64>,
    pub evidence: Option<[u8; 32]>,
    pub close: Option<StoredSourceClosure>,
    pub updated_at: i64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceLifecycle {
    Reserved,
    Spawned,
    Initialized,
    Closing,
    Closed,
    Unknown,
}
impl SourceLifecycle {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Reserved => "reserved",
            Self::Spawned => "spawned",
            Self::Initialized => "initialized",
            Self::Closing => "closing",
            Self::Closed => "closed",
            Self::Unknown => "unknown",
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredSourceClosure {
    pub basis: Arc<str>,
    pub stage: Arc<str>,
    pub io: Arc<str>,
    pub owner: Arc<str>,
    pub active_processes: Option<u32>,
    pub fingerprint: [u8; 32],
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceRecord {
    pub metadata: ResourceMetadata,
    pub lifecycle: ResourceLifecycle,
    pub scope_binding: Option<EntityId>,
    pub wire: Option<ProtocolVersion>,
    pub load_rpc: Option<RpcScalar>,
    pub frame_length: Option<u64>,
    pub transport: Option<StoredTransport>,
    pub loaded_sequence: Option<u64>,
    pub loaded_receipt: Option<u64>,
    pub evidence: Option<[u8; 32]>,
    pub release: Option<StoredResourceClosure>,
    pub updated_at: i64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceLifecycle {
    Reserved,
    Loading,
    Loaded,
    Releasing,
    Released,
    Unknown,
}
impl ResourceLifecycle {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Reserved => "reserved",
            Self::Loading => "loading",
            Self::Loaded => "loaded",
            Self::Releasing => "releasing",
            Self::Released => "released",
            Self::Unknown => "unknown",
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredResourceClosure {
    pub basis: Arc<str>,
    pub stage: Arc<str>,
    pub fingerprint: [u8; 32],
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoredTransport {
    Armed,
    Terminal(TransportFact),
}
pub struct SourceBoundRecordProof {
    source: Arc<PreSourceProbe>,
    binding: EntityId,
    at: i64,
}
pub struct SourceStateProof {
    source: Arc<PreSourceProbe>,
    state: SourceLifecycle,
    initialized: Option<InitializeMetadata>,
    at: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitializeMetadata {
    pub rpc: RpcScalar,
    pub wire: ProtocolVersion,
    pub sequence: u64,
    pub receipt: u64,
    pub fingerprint: [u8; 32],
}
pub struct ReservedLoadProof {
    resource: Arc<PreResourceProbe>,
    binding: EntityId,
    wire: ProtocolVersion,
    rpc: RpcScalar,
    frame_length: u64,
    at: i64,
}
pub struct LoadTransportProof {
    resource: Arc<PreResourceProbe>,
    rpc: RpcScalar,
    frame_length: u64,
    fact: TransportFact,
    at: i64,
}
pub struct LoadedResourceProof {
    resource: Arc<PreResourceProbe>,
    rpc: RpcScalar,
    sequence: u64,
    receipt: u64,
    fingerprint: [u8; 32],
    at: i64,
}
pub struct ResourceStateProof {
    resource: Arc<PreResourceProbe>,
    state: ResourceLifecycle,
    at: i64,
}
impl ResourceStateProof {
    #[must_use]
    pub fn resource(&self) -> &PreResourceProbe {
        &self.resource
    }
    #[must_use]
    pub const fn state(&self) -> ResourceLifecycle {
        self.state
    }
    #[must_use]
    pub const fn at(&self) -> i64 {
        self.at
    }
}
impl SourceBoundRecordProof {
    #[must_use]
    pub fn source(&self) -> &PreSourceProbe {
        &self.source
    }
    #[must_use]
    pub const fn binding(&self) -> EntityId {
        self.binding
    }
    #[must_use]
    pub const fn at(&self) -> i64 {
        self.at
    }
}
impl SourceStateProof {
    #[must_use]
    pub fn source(&self) -> &PreSourceProbe {
        &self.source
    }
    #[must_use]
    pub const fn state(&self) -> SourceLifecycle {
        self.state
    }
    #[must_use]
    pub const fn initialized(&self) -> Option<&InitializeMetadata> {
        self.initialized.as_ref()
    }
    #[must_use]
    pub const fn at(&self) -> i64 {
        self.at
    }
}
impl ReservedLoadProof {
    #[must_use]
    pub fn resource(&self) -> &PreResourceProbe {
        &self.resource
    }
    #[must_use]
    pub const fn binding(&self) -> EntityId {
        self.binding
    }
    #[must_use]
    pub const fn wire(&self) -> ProtocolVersion {
        self.wire
    }
    #[must_use]
    pub const fn rpc(&self) -> &RpcScalar {
        &self.rpc
    }
    #[must_use]
    pub const fn frame_length(&self) -> u64 {
        self.frame_length
    }
    #[must_use]
    pub const fn at(&self) -> i64 {
        self.at
    }
}
impl LoadTransportProof {
    #[must_use]
    pub fn resource(&self) -> &PreResourceProbe {
        &self.resource
    }
    #[must_use]
    pub const fn rpc(&self) -> &RpcScalar {
        &self.rpc
    }
    #[must_use]
    pub const fn frame_length(&self) -> u64 {
        self.frame_length
    }
    #[must_use]
    pub const fn fact(&self) -> TransportFact {
        self.fact
    }
    #[must_use]
    pub const fn at(&self) -> i64 {
        self.at
    }
}
impl LoadedResourceProof {
    #[must_use]
    pub fn resource(&self) -> &PreResourceProbe {
        &self.resource
    }
    #[must_use]
    pub const fn rpc(&self) -> &RpcScalar {
        &self.rpc
    }
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    #[must_use]
    pub const fn receipt(&self) -> u64 {
        self.receipt
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
pub trait NativeResourceRepository: NativeClosureRepository + Send + Sync + 'static {
    fn reserve_source<'a>(
        &'a self,
        p: &'a PreSourceProbe,
        d: &'a crate::application::request_registry::RegistryDeadline,
    ) -> BoxFuture<'a, Result<SourceRecord, DispatchError>>;
    fn reserve_resource<'a>(
        &'a self,
        p: &'a PreResourceProbe,
        d: &'a crate::application::request_registry::RegistryDeadline,
    ) -> BoxFuture<'a, Result<ResourceRecord, DispatchError>>;
    fn record_source_bound<'a>(
        &'a self,
        p: &'a SourceBoundRecordProof,
        d: &'a crate::application::request_registry::RegistryDeadline,
    ) -> BoxFuture<'a, Result<SourceRecord, DispatchError>>;
    fn record_source_state<'a>(
        &'a self,
        p: &'a SourceStateProof,
        d: &'a crate::application::request_registry::RegistryDeadline,
    ) -> BoxFuture<'a, Result<SourceRecord, DispatchError>>;
    fn begin_load<'a>(
        &'a self,
        p: &'a ReservedLoadProof,
        d: &'a crate::application::request_registry::RegistryDeadline,
    ) -> BoxFuture<'a, Result<ResourceRecord, DispatchError>>;
    fn record_load_transport<'a>(
        &'a self,
        p: &'a LoadTransportProof,
        d: &'a crate::application::request_registry::RegistryDeadline,
    ) -> BoxFuture<'a, Result<ResourceRecord, DispatchError>>;
    fn record_loaded<'a>(
        &'a self,
        p: &'a LoadedResourceProof,
        d: &'a crate::application::request_registry::RegistryDeadline,
    ) -> BoxFuture<'a, Result<ResourceRecord, DispatchError>>;
    fn record_resource_state<'a>(
        &'a self,
        p: &'a ResourceStateProof,
        d: &'a crate::application::request_registry::RegistryDeadline,
    ) -> BoxFuture<'a, Result<ResourceRecord, DispatchError>>;
    fn inspect_source<'a>(
        &'a self,
        p: &'a PreSourceProbe,
        d: &'a crate::application::request_registry::RegistryDeadline,
    ) -> BoxFuture<'a, Result<Option<SourceRecord>, DispatchError>>;
    fn inspect_resource<'a>(
        &'a self,
        p: &'a PreResourceProbe,
        d: &'a crate::application::request_registry::RegistryDeadline,
    ) -> BoxFuture<'a, Result<Option<ResourceRecord>, DispatchError>>;
}

#[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
pub(crate) use tasks::owned_job_registered_for_approval_test;

#[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
mod approval_acceptance_mount;
#[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
pub(crate) use approval_acceptance_mount::observe_approval_original;
