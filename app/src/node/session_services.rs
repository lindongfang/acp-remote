//! Concrete production handles from this Node's original registry and runtime claim.
#![allow(
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{NodeRequestAuthority, catalog::NodeCatalogCore, native_views::NodeNativeViewsService};
use crate::{
    access_policy::{ApplicationPolicy, AuthenticatedActor},
    adapters::acp::{RawMessage, native_dispatch::AcpNativeAdapter},
    application::{
        native_dispatch::{
            ApprovalDisplay, NativeRuntimeHandle, NativeRuntimeOptions, ResponseBudget,
        },
        queue_owner::QueueService,
        request_registry::{
            NodeClock, RegistryDeadline, RequestAuthority, SystemNodeClock, random_id,
        },
        shared_views::{
            budget::{MonotonicReadOptionsProvider, ReadResponseBudget},
            socket_authority::SocketViewAuthority,
        },
    },
    domain::{
        EntityId,
        approval_owner::{
            ApprovalId, ApprovalRegistrationId, ApprovalRejection, ApprovalStatus,
            DecisionAcceptance, NecessaryWriteError, RegisterDecision, SubmitDecision,
        },
        native_catalog::{CatalogFailure, CatalogInventory, ProductTargetRecord, WorkspaceRecord},
        native_dispatch::DispatchError,
        request_registry::RegistryError,
    },
    error::AppError,
    identity::NodeIdentity,
    platform::{SecretProtector, native_source::WindowsSourceLauncher},
    session_storage::{
        SqliteNodeConfiguration,
        native_dispatch::SqliteNativeDispatch,
        native_release::{
            NativeReleaseRecord, NativeReleaseStoredPhase, SqliteNativeReleaseOperations,
        },
        request_registry::SqliteRequestRegistry,
    },
    storage::Storage,
};
use std::{num::NonZeroUsize, sync::Arc, time::Duration};
pub(super) mod queue_reads;

pub(super) type ProductQueue = QueueService<
    NodeRequestAuthority,
    Arc<AcpNativeAdapter>,
    SqliteRequestRegistry,
    SystemNodeClock,
>;
pub(super) type ProductNative = NativeRuntimeHandle<
    AcpNativeAdapter,
    WindowsSourceLauncher,
    SqliteNodeConfiguration,
    SqliteNativeDispatch,
>;
pub(super) type ProductApproval = super::approval_owner::NodeApprovalService<
    AcpNativeAdapter,
    WindowsSourceLauncher,
    SqliteNodeConfiguration,
    SqliteNativeDispatch,
>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeReleasePhase {
    Registered,
    Releasing,
    Released,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeReleaseSnapshot {
    pub(crate) operation: EntityId,
    pub(crate) target: EntityId,
    pub(crate) phase: NativeReleasePhase,
    pub(crate) error: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum NativeReleaseError {
    #[error("native release authority failed")]
    Registry(RegistryError),
    #[error("native release operation conflicts")]
    Conflict,
    #[error("native release failed")]
    Native(DispatchError),
}

const fn native_release_error_code(error: DispatchError) -> &'static str {
    match error {
        DispatchError::NodeStopping | DispatchError::Stopped => "node_stopping",
        DispatchError::StorageUnavailable
        | DispatchError::CommitUnconfirmed
        | DispatchError::SqlCloseUnconfirmed { .. } => "necessary_storage_unavailable",
        DispatchError::InvalidTarget
        | DispatchError::InvalidResource
        | DispatchError::InvalidSource
        | DispatchError::InvalidScope => "invalid_target",
        DispatchError::Conflict
        | DispatchError::RevisionChanged
        | DispatchError::StaleSelection => "request_conflict",
        DispatchError::ResourceUnconfirmed => "release_unconfirmed",
        DispatchError::ProtocolRejected
        | DispatchError::ProtocolUnavailable
        | DispatchError::ProtocolFault(_) => "protocol_failure",
        _ => "release_failed",
    }
}

fn native_release_snapshot(
    record: NativeReleaseRecord,
) -> Result<NativeReleaseSnapshot, NativeReleaseError> {
    let phase = match record.phase {
        NativeReleaseStoredPhase::Registered => NativeReleasePhase::Registered,
        NativeReleaseStoredPhase::Releasing => NativeReleasePhase::Releasing,
        NativeReleaseStoredPhase::Released => NativeReleasePhase::Released,
        NativeReleaseStoredPhase::Failed => NativeReleasePhase::Failed,
    };
    let error = match record.error_code.as_deref() {
        None => None,
        Some("node_stopping") => Some("node_stopping"),
        Some("necessary_storage_unavailable") => Some("necessary_storage_unavailable"),
        Some("invalid_target") => Some("invalid_target"),
        Some("request_conflict") => Some("request_conflict"),
        Some("release_unconfirmed") => Some("release_unconfirmed"),
        Some("protocol_failure") => Some("protocol_failure"),
        Some("release_failed") => Some("release_failed"),
        Some(_) => return Err(NativeReleaseError::Registry(RegistryError::CorruptState)),
    };
    Ok(NativeReleaseSnapshot {
        operation: record.operation,
        target: record.target,
        phase,
        error,
    })
}

fn native_release_deadline() -> RegistryDeadline {
    RegistryDeadline::new(tokio::time::Instant::now() + SessionServiceTuning::IO_WAIT)
}

/// Scheduling/transport waits only. Neither response path caps model execution.
struct SessionServiceTuning;
impl SessionServiceTuning {
    const IO_WAIT: Duration = Duration::from_secs(30);
    const YIELD_EVERY: NonZeroUsize = NonZeroUsize::MIN.saturating_add(63);
    const fn native() -> NativeRuntimeOptions {
        NativeRuntimeOptions {
            spawn_wait: Self::IO_WAIT,
            initialize_write_wait: Self::IO_WAIT,
            initialize_response_wait: Self::IO_WAIT,
            load_write_wait: Self::IO_WAIT,
            load_response: ResponseBudget::Unbounded,
            prompt_write_wait: Self::IO_WAIT,
            prompt_response: ResponseBudget::Unbounded,
            metadata_wait: Self::IO_WAIT,
            close_wait: Self::IO_WAIT,
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FactoryStage {
    Acquiring = 0,
    BeforeServices = 1,
    BeforeViewInstall = 2,
    BeforeReady = 3,
    Ready = 4,
}
#[cfg(test)]
#[derive(Clone, Default)]
pub(super) struct FactoryTestHooks {
    pub(super) before_services: Option<Arc<crate::test_support::AsyncBarrier>>,
    pub(super) before_view_install: Option<Arc<crate::test_support::AsyncBarrier>>,
    pub(super) before_ready: Option<Arc<crate::test_support::AsyncBarrier>>,
    pub(super) body: super::prompt_preparation::test_hooks::Hooks,
    pub(super) reject_services: bool,
    stage: Arc<std::sync::atomic::AtomicU8>,
}
#[cfg(test)]
impl FactoryTestHooks {
    pub(super) fn mark(&self, stage: FactoryStage) {
        self.stage
            .store(stage as u8, std::sync::atomic::Ordering::Release);
    }
    pub(super) fn stage(&self) -> FactoryStage {
        match self.stage.load(std::sync::atomic::Ordering::Acquire) {
            1 => FactoryStage::BeforeServices,
            2 => FactoryStage::BeforeViewInstall,
            3 => FactoryStage::BeforeReady,
            4 => FactoryStage::Ready,
            _ => FactoryStage::Acquiring,
        }
    }
}
pub(super) struct OwnedNodeSessionServices {
    pub(super) queue_reads: queue_reads::NodeQueueReadService,
    pub(super) catalog: Arc<NodeCatalogCore<SqliteNodeConfiguration>>,
    pub(super) catalog_management: super::catalog_upload::NodeCatalogManagement,
    pub(super) configuration: Arc<SqliteNodeConfiguration>,
    pub(super) queue: ProductQueue,
    pub(super) native: ProductNative,
    pub(super) approval: Arc<ProductApproval>,
    pub(super) release_authority: NodeRequestAuthority,
    pub(super) native_release_operations: SqliteNativeReleaseOperations,
    pub(super) views: NodeNativeViewsService,
    pub(super) body: super::prompt_preparation::PreparedBodyHandle,
    pub(super) uploads: Arc<super::input_upload::NodeInputUploads>,
    pub(super) queue_management: super::queue_management::NodeQueueManagement,
    pub(super) documents: super::document_transfer::NodeDocumentService,
    pub(super) socket_authority: Arc<SocketViewAuthority>,
    pub(super) source_sessions: super::source_sessions::NodeSourceSessionManagement,
}
#[derive(Clone)]
pub struct NodeManagementSessionServices {
    approval: Arc<ProductApproval>,
    native: ProductNative,
    release_authority: NodeRequestAuthority,
    native_release_operations: SqliteNativeReleaseOperations,
    queue_reads: queue_reads::NodeQueueReadService,
    queue_management: super::queue_management::NodeQueueManagement,
    catalog: Arc<NodeCatalogCore<SqliteNodeConfiguration>>,
    catalog_management: super::catalog_upload::NodeCatalogManagement,
    uploads: Arc<super::input_upload::NodeInputUploads>,
    documents: super::document_transfer::NodeDocumentService,
    views: NodeNativeViewsService,
    socket_authority: Arc<SocketViewAuthority>,
    source_sessions: super::source_sessions::NodeSourceSessionManagement,
}
impl NodeManagementSessionServices {
    pub(crate) fn belongs_to(&self, authority: &Arc<SocketViewAuthority>) -> bool {
        Arc::ptr_eq(&self.socket_authority, authority)
    }
    pub(crate) fn queue_reads(&self) -> queue_reads::NodeQueueReadService {
        self.queue_reads.clone()
    }
    pub(crate) fn queue_management(&self) -> super::queue_management::NodeQueueManagement {
        self.queue_management.clone()
    }
    pub(crate) fn catalog_management(&self) -> super::catalog_upload::NodeCatalogManagement {
        self.catalog_management.clone()
    }
    pub(crate) fn source_sessions(&self) -> super::source_sessions::NodeSourceSessionManagement {
        self.source_sessions.clone()
    }
    pub(crate) async fn register_workspace(
        &self,
        actor: &AuthenticatedActor,
        path: &str,
    ) -> Result<WorkspaceRecord, CatalogFailure> {
        self.catalog
            .register_workspace(actor, path, &native_release_deadline())
            .await
    }
    pub(crate) async fn register_target(
        &self,
        actor: &AuthenticatedActor,
        source: EntityId,
        native_session_id: &str,
        workspace: EntityId,
    ) -> Result<ProductTargetRecord, CatalogFailure> {
        self.catalog
            .register_target(
                actor,
                source,
                native_session_id,
                workspace,
                &native_release_deadline(),
            )
            .await
    }
    pub(crate) async fn catalog_inventory(
        &self,
        actor: &AuthenticatedActor,
    ) -> Result<CatalogInventory, CatalogFailure> {
        self.catalog
            .catalog_inventory(actor, &native_release_deadline())
            .await
    }
    pub(crate) fn open_catalog_upload(
        &self,
        client: &crate::application::shared_views::socket_authority::SocketViewClient,
    ) -> Result<
        super::catalog_upload::CatalogUploadOwner,
        crate::management_ws::input_upload::UploadError,
    > {
        self.catalog_management.open_upload(client)
    }
    pub(crate) async fn approval_list(
        &self,
        actor: &AuthenticatedActor,
    ) -> Result<Vec<ApprovalDisplay<RawMessage>>, ApprovalRejection> {
        self.approval.list(actor).await
    }
    pub(crate) async fn approval_read(
        &self,
        actor: &AuthenticatedActor,
        approval: ApprovalId,
    ) -> Result<ApprovalDisplay<RawMessage>, ApprovalRejection> {
        self.approval.read(actor, approval).await
    }
    pub(crate) async fn approval_register(
        &self,
        actor: &AuthenticatedActor,
        input: RegisterDecision,
    ) -> Result<ApprovalRegistrationId, NecessaryWriteError> {
        self.approval.register_decision(actor, input).await
    }
    pub(crate) async fn approval_decide(
        &self,
        actor: &AuthenticatedActor,
        input: SubmitDecision,
    ) -> Result<DecisionAcceptance, NecessaryWriteError> {
        self.approval.decide(actor, input).await
    }
    pub(crate) async fn approval_status(
        &self,
        actor: &AuthenticatedActor,
        approval: ApprovalId,
    ) -> Result<ApprovalStatus, ApprovalRejection> {
        self.approval.inspect_decision(actor, approval).await
    }
    pub(crate) async fn register_native_release(
        &self,
        actor: &AuthenticatedActor,
        request: EntityId,
        target: EntityId,
    ) -> Result<EntityId, NativeReleaseError> {
        let actor = self
            .release_authority
            .current(actor)
            .await
            .map_err(NativeReleaseError::Registry)?;
        RequestAuthority::resolve_target(&self.release_authority, target)
            .await
            .map_err(NativeReleaseError::Registry)?;
        let operation = random_id().map_err(NativeReleaseError::Registry)?;
        let deadline = native_release_deadline();
        let registered = self
            .native_release_operations
            .register(
                actor,
                target,
                request,
                operation,
                SystemNodeClock.unix_millis(),
                &deadline,
            )
            .await
            .map_err(NativeReleaseError::Registry)?;
        Ok(registered.operation)
    }
    pub(crate) async fn native_release(
        &self,
        actor: &AuthenticatedActor,
        operation: EntityId,
    ) -> Result<NativeReleaseSnapshot, NativeReleaseError> {
        let actor = self
            .release_authority
            .current(actor)
            .await
            .map_err(NativeReleaseError::Registry)?;
        let deadline = native_release_deadline();
        let release = self
            .native_release_operations
            .begin(actor, operation, SystemNodeClock.unix_millis(), &deadline)
            .await
            .map_err(NativeReleaseError::Registry)?;
        match release.phase {
            NativeReleaseStoredPhase::Releasing => {}
            NativeReleaseStoredPhase::Registered => return Err(NativeReleaseError::Conflict),
            NativeReleaseStoredPhase::Released | NativeReleaseStoredPhase::Failed => {
                return native_release_snapshot(release);
            }
        }
        RequestAuthority::resolve_target(&self.release_authority, release.target)
            .await
            .map_err(NativeReleaseError::Registry)?;
        match self.native.release_target(release.target).await {
            Ok(()) => {
                let finished = self
                    .native_release_operations
                    .finish(
                        actor,
                        operation,
                        None,
                        SystemNodeClock.unix_millis(),
                        &native_release_deadline(),
                    )
                    .await
                    .map_err(NativeReleaseError::Registry)?;
                native_release_snapshot(finished)
            }
            Err(error) => {
                let error_code = native_release_error_code(error);
                self.native_release_operations
                    .finish(
                        actor,
                        operation,
                        Some(error_code),
                        SystemNodeClock.unix_millis(),
                        &native_release_deadline(),
                    )
                    .await
                    .map_err(NativeReleaseError::Registry)?;
                Err(NativeReleaseError::Native(error))
            }
        }
    }
    pub(crate) async fn native_release_status(
        &self,
        actor: &AuthenticatedActor,
        operation: EntityId,
    ) -> Result<NativeReleaseSnapshot, NativeReleaseError> {
        let actor = self
            .release_authority
            .current(actor)
            .await
            .map_err(NativeReleaseError::Registry)?;
        let release = self
            .native_release_operations
            .inspect(actor, operation, &native_release_deadline())
            .await
            .map_err(NativeReleaseError::Registry)?;
        RequestAuthority::resolve_target(&self.release_authority, release.target)
            .await
            .map_err(NativeReleaseError::Registry)?;
        native_release_snapshot(release)
    }
    pub(crate) async fn open_queued_body(
        &self,
        client: &crate::application::shared_views::socket_authority::SocketViewClient,
        target: crate::domain::EntityId,
        item: crate::domain::EntityId,
        expected_revision: u64,
    ) -> Result<
        crate::application::document_transfer::DocumentTransfer,
        crate::application::document_source::DocumentError,
    > {
        self.documents
            .open_queued_body(client, target, item, expected_revision)
            .await
    }
    pub(crate) fn open_upload(
        &self,
        client: &crate::application::shared_views::socket_authority::SocketViewClient,
    ) -> Result<super::input_upload::UploadOwner, crate::management_ws::input_upload::UploadError>
    {
        self.uploads.open(client)
    }
    pub(crate) async fn open_view(
        &self,
        client: &crate::application::shared_views::socket_authority::SocketViewClient,
        target: crate::domain::EntityId,
        reason: crate::application::shared_views::budget::ReadReason,
    ) -> Result<super::native_views::NativeSocketViewRead, crate::domain::shared_view::ViewError>
    {
        self.views.open_socket(client, target, reason).await
    }
    pub(crate) async fn open_source_view(
        &self,
        client: &crate::application::shared_views::socket_authority::SocketViewClient,
        selection: crate::domain::source_sessions::SourceLaunchSelection,
        native_session_id: String,
        reason: crate::application::shared_views::budget::ReadReason,
    ) -> Result<super::native_views::NativeSocketViewRead, crate::domain::shared_view::ViewError>
    {
        self.views
            .open_socket_source(client, selection, native_session_id, reason)
            .await
    }
    #[cfg(test)]
    pub(crate) fn arm_route_fault_for_test(
        &self,
        client: &crate::application::shared_views::socket_authority::SocketViewClient,
        fault: super::input_upload::UploadRouteFault,
    ) -> Result<
        super::input_upload::UploadRouteFaultReceipt,
        crate::management_ws::input_upload::UploadError,
    > {
        self.uploads.arm_route_fault_for_test(client, fault)
    }
}
impl OwnedNodeSessionServices {
    pub(super) async fn start(
        storage: &Storage,
        identity: &NodeIdentity,
        policy: Arc<ApplicationPolicy>,
        protector: Arc<dyn SecretProtector>,
        socket_authority: Arc<SocketViewAuthority>,
        #[cfg(test)] hooks: &FactoryTestHooks,
    ) -> Result<Self, AppError> {
        let authority = NodeRequestAuthority::from_node_parts(policy.clone(), storage)
            .map_err(|_| AppError::Startup("product authority unavailable".into()))?;
        let catalog = super::catalog::from_node_parts(
            authority.clone(),
            storage,
            identity,
            protector,
            SessionServiceTuning::IO_WAIT,
        )
        .map_err(|_| AppError::Startup("product configuration unavailable".into()))?;
        catalog
            .reconcile(&RegistryDeadline::new(
                tokio::time::Instant::now() + SessionServiceTuning::IO_WAIT,
            ))
            .await
            .map_err(|_| {
                AppError::Startup("product configuration reconciliation unconfirmed".into())
            })?;
        let catalog = Arc::new(catalog);
        let configuration = catalog.configuration();
        let protocol = Arc::new(AcpNativeAdapter::default());
        let started = super::native_views::start_from_node_parts(
            storage,
            &policy,
            authority.clone(),
            protocol.clone(),
            SystemNodeClock,
            protocol.clone(),
            Arc::new(WindowsSourceLauncher),
            configuration.clone(),
            protocol,
            SessionServiceTuning::native(),
            Arc::new(MonotonicReadOptionsProvider::new(
                SessionServiceTuning::IO_WAIT,
                ReadResponseBudget::Unbounded,
            )),
            SessionServiceTuning::YIELD_EVERY,
            Some(socket_authority.clone()),
            #[cfg(test)]
            Some(hooks),
        )
        .await
        .map_err(|_| AppError::Startup("product native assembly unavailable".into()))?;
        let approval = crate::application::native_dispatch::NativeApprovalService::create(
            started.native.clone(),
            Arc::new(authority.clone()),
        )
        .map_err(|_| AppError::Startup("product approval assembly unavailable".into()))?;
        let (body, catalog_preparation) = Self::attach_upload_preparation(
            storage,
            &started.queue,
            #[cfg(test)]
            hooks,
        )?;
        let release_authority = authority.clone();
        let uploads = Arc::new(super::input_upload::NodeInputUploads::new(
            body.clone(),
            catalog_preparation.clone(),
            authority,
            socket_authority.clone(),
        ));
        let catalog_management = super::catalog_upload::NodeCatalogManagement::new(
            catalog.clone(),
            uploads.clone(),
            catalog_preparation,
            socket_authority.clone(),
            storage.request_registry().owner_scope(),
        );
        let queue_management = super::queue_management::NodeQueueManagement::new(
            started.queue.clone(),
            uploads.clone(),
            socket_authority.clone(),
        );
        let documents = super::document_transfer::NodeDocumentService::new(
            storage,
            started.queue.clone(),
            socket_authority.clone(),
        )
        .map_err(|_| AppError::Startup("product document assembly unavailable".into()))?;
        let source_sessions = super::source_sessions::NodeSourceSessionManagement::new(
            started.native.clone(),
            release_authority.clone(),
            socket_authority.clone(),
            documents.clone(),
        )
        .await
        .map_err(|_| AppError::Startup("product source session assembly unavailable".into()))?;
        let queue_reads =
            queue_reads::NodeQueueReadService::new(started.queue.clone(), socket_authority.clone());
        Ok(Self {
            queue_reads,
            catalog,
            catalog_management,
            configuration,
            queue: started.queue,
            native: started.native,
            approval: Arc::new(approval),
            release_authority,
            native_release_operations: storage.native_release_operations(),
            views: started.views,
            body,
            uploads,
            queue_management,
            documents,
            socket_authority,
            source_sessions,
        })
    }
    fn attach_upload_preparation(
        storage: &Storage,
        queue: &ProductQueue,
        #[cfg(test)] hooks: &FactoryTestHooks,
    ) -> Result<
        (
            super::prompt_preparation::PreparedBodyHandle,
            crate::application::catalog_preparation::CatalogPreparationOwner,
        ),
        AppError,
    > {
        #[cfg(not(test))]
        let body = super::prompt_preparation::attach_prompt_preparation(storage, queue.clone())
            .map_err(|_| AppError::Startup("product body preparation unavailable".into()))?;
        #[cfg(test)]
        let body = super::prompt_preparation::attach_with_hooks(
            storage,
            queue.clone(),
            hooks.body.clone(),
        )
        .map_err(|_| AppError::Startup("product body preparation unavailable".into()))?;
        let catalog_preparation =
            crate::application::catalog_preparation::CatalogPreparationOwner::attach(
                storage,
                crate::application::queue_owner::RuntimeSupervisor::queue_runtime_identity(queue),
            )
            .map_err(|_| AppError::Startup("product catalog preparation unavailable".into()))?;
        Ok((body, catalog_preparation))
    }
    pub(super) fn seal(&self) {
        self.source_sessions.seal();
        self.catalog_management.seal();
        self.documents.seal();
        self.queue_management.seal();
        self.uploads.seal();
        self.body.seal();
    }
    pub(super) async fn shutdown(&self, deadline: tokio::time::Instant) -> bool {
        let report = self.native.shutdown(deadline).await;
        #[cfg(test)]
        eprintln!(
            "NODE_SESSION_SHUTDOWN_DIAGNOSTIC_V1 joined={} metadata_confirmed={} sources={:?}",
            report.joined,
            report.metadata_confirmed,
            self.native.source_close_diagnostics()
        );
        report.joined && report.metadata_confirmed
    }
    pub(crate) fn management_services(&self) -> NodeManagementSessionServices {
        NodeManagementSessionServices {
            approval: self.approval.clone(),
            native: self.native.clone(),
            release_authority: self.release_authority.clone(),
            native_release_operations: self.native_release_operations.clone(),
            queue_reads: self.queue_reads.clone(),
            queue_management: self.queue_management.clone(),
            catalog: self.catalog.clone(),
            catalog_management: self.catalog_management.clone(),
            uploads: self.uploads.clone(),
            documents: self.documents.clone(),
            views: self.views.clone(),
            socket_authority: self.socket_authority.clone(),
            source_sessions: self.source_sessions.clone(),
        }
    }
}
