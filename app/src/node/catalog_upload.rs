//! Thin catalog custody over the original socket, catalog core and supervised workers.
#![allow(
    clippy::result_large_err,
    clippy::significant_drop_tightening,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    NodeRuntime,
    catalog::NodeCatalogCore,
    input_upload::{
        NodeInputUploads, UploadOwner, UploadPhase, UploadPreparing, UploadRoutingError,
        UploadStarted,
    },
};
use crate::{
    adapters::acp::catalog_draft_document::{ParsedCatalogDocument, parse_catalog_document},
    application::{
        catalog_preparation::{
            CatalogBufferError, CatalogBufferPhase, CatalogJobResult, CatalogPreparationFailure,
            CatalogPreparationOwner, CatalogTaskObservation, CatalogWorkReceipt,
        },
        request_registry::{RegistryDeadline, RegistryOwnerScope},
        shared_views::socket_authority::{
            LiveSocketContext, SocketLifetimeParticipant, SocketViewAuthority, SocketViewClient,
        },
    },
    claims::Id128,
    domain::native_catalog::{
        CatalogError, CatalogFact, CatalogFailure, CatalogOperationKind, CatalogReceipt,
        CatalogRequestId,
    },
    management_ws::input_upload::{InputAck, InputData, UploadError},
    session_storage::SqliteNodeConfiguration,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
// Necessary metadata/SQL waits start inside the worker, not while the upload or scheduling waits.
const CATALOG_IO_WAIT: Duration = Duration::from_secs(30);
fn deadline() -> RegistryDeadline {
    RegistryDeadline::new(tokio::time::Instant::now() + CATALOG_IO_WAIT)
}
const fn failure(kind: CatalogOperationKind, e: CatalogError) -> CatalogFailure {
    crate::application::native_catalog::failure(kind, e)
}
const fn view_failure(
    kind: CatalogOperationKind,
    e: crate::domain::shared_view::ViewError,
) -> CatalogFailure {
    failure(
        kind,
        match e {
            crate::domain::shared_view::ViewError::AllocationFailure => CatalogError::Allocation,
            crate::domain::shared_view::ViewError::NodeStopping => CatalogError::NodeStopping,
            crate::domain::shared_view::ViewError::ReadFailure => CatalogError::StorageUnavailable,
            _ => CatalogError::InvalidActor,
        },
    )
}
const fn buffer_view(e: CatalogBufferError) -> crate::domain::shared_view::ViewError {
    match e {
        CatalogBufferError::Allocation => crate::domain::shared_view::ViewError::AllocationFailure,
        CatalogBufferError::NodeStopping => crate::domain::shared_view::ViewError::NodeStopping,
        _ => crate::domain::shared_view::ViewError::ReadFailure,
    }
}
const fn upload_failure(kind: CatalogOperationKind, e: UploadError) -> CatalogFailure {
    failure(
        kind,
        match e {
            UploadError::InvalidOwner => CatalogError::InvalidActor,
            UploadError::InvalidTarget => CatalogError::InvalidTarget,
            UploadError::Allocation => CatalogError::Allocation,
            UploadError::NodeStopping => CatalogError::NodeStopping,
            UploadError::StorageUnavailable => CatalogError::StorageUnavailable,
            _ => CatalogError::InvalidConfiguration,
        },
    )
}
#[derive(Clone)]
pub struct CatalogUploadOwner {
    upload: UploadOwner,
    catalog: Arc<NodeCatalogCore<SqliteNodeConfiguration>>,
}
#[derive(Clone)]
pub struct CatalogUploadStatus {
    phase: UploadPhase,
    next_offset: u64,
    error: Option<CatalogPreparationFailure>,
}
impl CatalogUploadStatus {
    pub(crate) const fn phase(&self) -> UploadPhase {
        self.phase
    }
    pub(crate) const fn next_offset(&self) -> u64 {
        self.next_offset
    }
    pub(crate) const fn error(&self) -> Option<&CatalogPreparationFailure> {
        self.error.as_ref()
    }
}
impl CatalogUploadOwner {
    pub(crate) async fn begin(&self, c: &SocketViewClient) -> Result<UploadStarted, UploadError> {
        self.upload.catalog_begin(c).await
    }
    pub(crate) async fn begin_typed(
        &self,
        c: &SocketViewClient,
    ) -> Result<UploadStarted, UploadRoutingError> {
        self.upload.catalog_begin_typed(c).await
    }
    pub(crate) async fn accept_data(
        &self,
        c: &SocketViewClient,
        d: InputData<'_>,
    ) -> Result<InputAck, UploadError> {
        self.upload.catalog_data(c, d).await
    }
    pub(crate) async fn finish(
        &self,
        c: &SocketViewClient,
        u: Id128,
        total: u64,
    ) -> Result<UploadPreparing, UploadError> {
        let catalog = self.catalog.clone();
        let actor = c.context.actor().clone();
        self.upload
            .catalog_finish(c, u, total, move |bytes, runtime| {
                let parsed = parse_catalog_document(bytes).map_err(|e| {
                    CatalogPreparationFailure::Upload(if e == CatalogError::Allocation {
                        CatalogBufferError::Allocation
                    } else {
                        CatalogBufferError::InvalidDocument
                    })
                })?;
                match parsed {
                    ParsedCatalogDocument::Complete(draft) => Ok(draft),
                    ParsedCatalogDocument::Bind {
                        target,
                        source,
                        harness,
                        profile,
                        workspace,
                        launch_path,
                    } => runtime.block_on(async move {
                        let observed = catalog
                            .observe_workspace(&actor, target, source, workspace, &deadline())
                            .await
                            .map_err(CatalogPreparationFailure::Catalog)?;
                        observed.bind(harness, profile, launch_path).map_err(|e| {
                            CatalogPreparationFailure::Upload(if e == CatalogError::Allocation {
                                CatalogBufferError::Allocation
                            } else {
                                CatalogBufferError::InvalidDocument
                            })
                        })
                    }),
                }
            })
            .await
    }
    pub(crate) async fn finish_typed(
        &self,
        c: &SocketViewClient,
        u: Id128,
        total: u64,
    ) -> Result<UploadPreparing, UploadRoutingError> {
        let catalog = self.catalog.clone();
        let actor = c.context.actor().clone();
        self.upload
            .catalog_finish_typed(c, u, total, move |bytes, runtime| {
                let parsed = parse_catalog_document(bytes).map_err(|e| {
                    CatalogPreparationFailure::Upload(if e == CatalogError::Allocation {
                        CatalogBufferError::Allocation
                    } else {
                        CatalogBufferError::InvalidDocument
                    })
                })?;
                match parsed {
                    ParsedCatalogDocument::Complete(draft) => Ok(draft),
                    ParsedCatalogDocument::Bind {
                        target,
                        source,
                        harness,
                        profile,
                        workspace,
                        launch_path,
                    } => runtime.block_on(async move {
                        let observed = catalog
                            .observe_workspace(&actor, target, source, workspace, &deadline())
                            .await
                            .map_err(CatalogPreparationFailure::Catalog)?;
                        observed.bind(harness, profile, launch_path).map_err(|e| {
                            CatalogPreparationFailure::Upload(if e == CatalogError::Allocation {
                                CatalogBufferError::Allocation
                            } else {
                                CatalogBufferError::InvalidDocument
                            })
                        })
                    }),
                }
            })
            .await
    }
    pub(crate) async fn status(
        &self,
        c: &SocketViewClient,
        u: Id128,
    ) -> Result<CatalogUploadStatus, UploadError> {
        let s = self.upload.catalog_status(c, u).await?;
        let phase = match s.phase {
            CatalogBufferPhase::Receiving => UploadPhase::Receiving,
            CatalogBufferPhase::Preparing => UploadPhase::Preparing,
            CatalogBufferPhase::Ready => UploadPhase::Ready,
            CatalogBufferPhase::Failed => UploadPhase::Failed,
            _ => return Err(UploadError::BodyUnavailable),
        };
        Ok(CatalogUploadStatus {
            phase,
            next_offset: s.next_offset,
            error: s.error,
        })
    }
    pub(crate) async fn status_typed(
        &self,
        c: &SocketViewClient,
        u: Id128,
    ) -> Result<CatalogUploadStatus, UploadRoutingError> {
        let s = self.upload.catalog_status_typed(c, u).await?;
        let phase = match s.phase {
            CatalogBufferPhase::Receiving => UploadPhase::Receiving,
            CatalogBufferPhase::Preparing => UploadPhase::Preparing,
            CatalogBufferPhase::Ready => UploadPhase::Ready,
            CatalogBufferPhase::Failed => UploadPhase::Failed,
            _ => return Err(UploadRoutingError::Business(UploadError::BodyUnavailable)),
        };
        Ok(CatalogUploadStatus {
            phase,
            next_offset: s.next_offset,
            error: s.error,
        })
    }
    pub(crate) async fn release(&self, c: &SocketViewClient, u: Id128) -> Result<(), UploadError> {
        self.upload.catalog_release(c, u).await
    }
    pub(crate) async fn release_typed(
        &self,
        c: &SocketViewClient,
        u: Id128,
    ) -> Result<(), UploadRoutingError> {
        self.upload.catalog_release_typed(c, u).await
    }
}
#[derive(Clone, Debug)]
pub enum CatalogKnownResult {
    Registered(Result<CatalogRequestId, CatalogFailure>),
    Applied(Result<CatalogReceipt, CatalogFailure>),
    Queried(Result<CatalogFact, CatalogFailure>),
}
fn known_result(receipt: &CatalogWorkReceipt) -> Option<CatalogKnownResult> {
    match receipt.original_result() {
        Some(CatalogJobResult::Registered(r)) => Some(CatalogKnownResult::Registered(r)),
        Some(CatalogJobResult::Applied(r)) => Some(CatalogKnownResult::Applied(r)),
        Some(CatalogJobResult::Queried(r)) => Some(CatalogKnownResult::Queried(r)),
        _ => None,
    }
}
#[derive(Clone, Debug)]
pub enum CatalogCommandFailure {
    Catalog(CatalogFailure),
    WorkerUnconfirmed {
        request: Option<CatalogRequestId>,
        observation: CatalogTaskObservation,
        original: Option<CatalogKnownResult>,
    },
}
impl CatalogCommandFailure {
    pub(crate) const fn commit(&self) -> crate::domain::native_catalog::CommitKnowledge {
        match self {
            Self::Catalog(e) => e.commit(),
            Self::WorkerUnconfirmed { .. } => {
                crate::domain::native_catalog::CommitKnowledge::Unknown
            }
        }
    }
    pub(crate) const fn cleanup(&self) -> crate::domain::native_catalog::CleanupKnowledge {
        match self {
            Self::Catalog(e) => e.cleanup(),
            Self::WorkerUnconfirmed { .. } => {
                crate::domain::native_catalog::CleanupKnowledge::UnconfirmedOriginal
            }
        }
    }
}
#[derive(Clone)]
pub struct CatalogRegisterReceipt(CatalogWorkReceipt);
#[derive(Clone)]
pub struct CatalogApplyReceipt(CatalogWorkReceipt, CatalogRequestId);
#[derive(Clone)]
pub struct CatalogQueryReceipt(CatalogWorkReceipt, CatalogRequestId);
macro_rules! receipt {
    ($name:ident,$variant:ident,$output:ty,$request:expr) => {
        impl $name {
            pub(crate) fn observe(&self) -> CatalogTaskObservation {
                self.0.observation()
            }
            pub(crate) fn identity(
                &self,
            ) -> crate::application::catalog_preparation::CatalogTaskIdentity {
                self.observe().identity
            }
            pub(crate) fn original_result(&self) -> Option<Result<$output, CatalogFailure>> {
                match self.0.original_result() {
                    Some(CatalogJobResult::$variant(r)) => Some(r),
                    _ => None,
                }
            }
            pub(crate) fn try_result(&self) -> Option<Result<$output, CatalogCommandFailure>> {
                match self.0.try_result() {
                    Some(CatalogJobResult::$variant(r)) => {
                        Some(r.map_err(CatalogCommandFailure::Catalog))
                    }
                    Some(CatalogJobResult::WorkerUnconfirmed(observation)) => {
                        Some(Err(CatalogCommandFailure::WorkerUnconfirmed {
                            request: $request(self),
                            observation,
                            original: known_result(&self.0),
                        }))
                    }
                    _ => None,
                }
            }
            pub(crate) async fn wait(&self) -> Result<$output, CatalogCommandFailure> {
                match self.0.wait().await {
                    CatalogJobResult::$variant(r) => r.map_err(CatalogCommandFailure::Catalog),
                    CatalogJobResult::WorkerUnconfirmed(observation) => {
                        Err(CatalogCommandFailure::WorkerUnconfirmed {
                            request: $request(self),
                            observation,
                            original: known_result(&self.0),
                        })
                    }
                    _ => std::future::pending().await,
                }
            }
        }
    };
}
receipt!(
    CatalogRegisterReceipt,
    Registered,
    CatalogRequestId,
    |_: &CatalogRegisterReceipt| None
);
receipt!(
    CatalogApplyReceipt,
    Applied,
    CatalogReceipt,
    |r: &CatalogApplyReceipt| Some(r.1)
);
receipt!(
    CatalogQueryReceipt,
    Queried,
    CatalogFact,
    |r: &CatalogQueryReceipt| Some(r.1)
);
struct SocketOperations {
    context: Weak<LiveSocketContext>,
    owner: Weak<Core>,
    id: Id128,
    inner: Mutex<Operations>,
}
struct Operations {
    sealed: bool,
    apply: HashMap<CatalogRequestId, CatalogApplyReceipt>,
}
impl SocketLifetimeParticipant for SocketOperations {
    fn seal(&self) {
        let removed = {
            let mut inner = lock(&self.inner);
            inner.sealed = true;
            std::mem::take(&mut inner.apply)
        };
        drop(removed);
        if let Some(owner) = self.owner.upgrade() {
            lock(&owner.registry).sockets.remove(&self.id);
        }
    }
}
struct Registry {
    sealed: bool,
    sockets: HashMap<Id128, Arc<SocketOperations>>,
}
struct Core {
    catalog: Arc<NodeCatalogCore<SqliteNodeConfiguration>>,
    uploads: Arc<NodeInputUploads>,
    preparation: CatalogPreparationOwner,
    authority: Arc<SocketViewAuthority>,
    scope: RegistryOwnerScope,
    registry: Mutex<Registry>,
}
#[derive(Clone)]
pub struct NodeCatalogManagement {
    core: Arc<Core>,
}
impl NodeCatalogManagement {
    pub(super) fn new(
        catalog: Arc<NodeCatalogCore<SqliteNodeConfiguration>>,
        uploads: Arc<NodeInputUploads>,
        preparation: CatalogPreparationOwner,
        authority: Arc<SocketViewAuthority>,
        scope: RegistryOwnerScope,
    ) -> Self {
        Self {
            core: Arc::new(Core {
                catalog,
                uploads,
                preparation,
                authority,
                scope,
                registry: Mutex::new(Registry {
                    sealed: false,
                    sockets: HashMap::new(),
                }),
            }),
        }
    }
    fn check(
        &self,
        c: &SocketViewClient,
        kind: CatalogOperationKind,
    ) -> Result<(), CatalogFailure> {
        if !Arc::ptr_eq(c.context.authority(), &self.core.authority) {
            return Err(failure(kind, CatalogError::InvalidActor));
        }
        self.core
            .scope
            .require_open()
            .map_err(|e| failure(kind, e.into()))?;
        if lock(&self.core.registry).sealed {
            return Err(failure(kind, CatalogError::NodeStopping));
        }
        Ok(())
    }
    pub(crate) fn open_upload(
        &self,
        c: &SocketViewClient,
    ) -> Result<CatalogUploadOwner, UploadError> {
        Ok(CatalogUploadOwner {
            upload: self.core.uploads.open(c)?,
            catalog: self.core.catalog.clone(),
        })
    }
    fn socket(
        &self,
        c: &SocketViewClient,
        kind: CatalogOperationKind,
    ) -> Result<Arc<SocketOperations>, CatalogFailure> {
        self.check(c, kind)?;
        let id = c.context.connection_id();
        let existing = lock(&self.core.registry).sockets.get(&id).cloned();
        if let Some(existing) = existing {
            if existing
                .context
                .upgrade()
                .is_some_and(|original| Arc::ptr_eq(&original, &c.context))
            {
                return Ok(existing);
            }
            return Err(failure(kind, CatalogError::InvalidActor));
        }
        let created = Arc::new(SocketOperations {
            context: Arc::downgrade(&c.context),
            owner: Arc::downgrade(&self.core),
            id,
            inner: Mutex::new(Operations {
                sealed: false,
                apply: HashMap::new(),
            }),
        });
        let participant: Arc<dyn SocketLifetimeParticipant> = created.clone();
        c.context
            .lifetime
            .register_participant(Arc::downgrade(&participant))
            .map_err(|e| view_failure(kind, e))?;
        c.context
            .lifetime
            .with_live(|| {
                let mut registry = lock(&self.core.registry);
                if registry.sealed {
                    return Err(crate::domain::shared_view::ViewError::NodeStopping);
                }
                registry
                    .sockets
                    .try_reserve(1)
                    .map_err(|_| crate::domain::shared_view::ViewError::AllocationFailure)?;
                Ok(registry.sockets.entry(id).or_insert(created).clone())
            })
            .map_err(|e| {
                failure(
                    kind,
                    if e == crate::domain::shared_view::ViewError::AllocationFailure {
                        CatalogError::Allocation
                    } else {
                        CatalogError::NodeStopping
                    },
                )
            })
    }
    pub(crate) async fn register(
        &self,
        c: &SocketViewClient,
        u: Id128,
    ) -> Result<CatalogRegisterReceipt, CatalogFailure> {
        self.check(c, CatalogOperationKind::Register)?;
        let owner = self
            .core
            .uploads
            .open(c)
            .map_err(|e| upload_failure(CatalogOperationKind::Register, e))?;
        let core = self.core.catalog.clone();
        let actor = c.context.actor().clone();
        let receipt = owner
            .catalog_register(c, u, move |draft, runtime| {
                runtime.block_on(async move { core.register(&actor, draft, &deadline()).await })
            })
            .await
            .map_err(|e| upload_failure(CatalogOperationKind::Register, e))?;
        Ok(CatalogRegisterReceipt(receipt))
    }
    pub(crate) async fn apply(
        &self,
        c: &SocketViewClient,
        id: CatalogRequestId,
    ) -> Result<CatalogApplyReceipt, CatalogFailure> {
        let kind = CatalogOperationKind::Apply;
        let socket = self.socket(c, kind)?;
        let (_, guard) = self
            .core
            .authority
            .current_actor_guard(c)
            .await
            .map_err(|e| view_failure(kind, e))?;
        let core = self.core.catalog.clone();
        let actor = c.context.actor().clone();
        let selected = c
            .context
            .lifetime
            .with_live(|| {
                let mut ops = lock(&socket.inner);
                if ops.sealed {
                    return Err(crate::domain::shared_view::ViewError::StreamSealed);
                }
                if let Some(receipt) = ops.apply.get(&id) {
                    return Ok((receipt.clone(), None));
                }
                ops.apply
                    .try_reserve(1)
                    .map_err(|_| crate::domain::shared_view::ViewError::AllocationFailure)?;
                let work =
                    self.core
                        .preparation
                        .job(
                            crate::application::catalog_preparation::CatalogTaskPhase::Apply,
                            move |runtime| {
                                CatalogJobResult::Applied(runtime.block_on(async move {
                                    core.apply(&actor, id, &deadline()).await
                                }))
                            },
                        )
                        .map_err(buffer_view)?;
                let receipt = CatalogApplyReceipt(work.receipt.clone(), id);
                ops.apply.insert(id, receipt.clone());
                Ok((receipt, Some(work)))
            })
            .map_err(|e| {
                failure(
                    kind,
                    if e == crate::domain::shared_view::ViewError::AllocationFailure {
                        CatalogError::Allocation
                    } else {
                        CatalogError::NodeStopping
                    },
                )
            })?;
        drop(guard);
        if let Some(work) = selected.1 {
            work.start();
        }
        Ok(selected.0)
    }
    pub(crate) async fn query(
        &self,
        c: &SocketViewClient,
        id: CatalogRequestId,
    ) -> Result<CatalogQueryReceipt, CatalogFailure> {
        let kind = CatalogOperationKind::Query;
        self.check(c, kind)?;
        let (_, guard) = self
            .core
            .authority
            .current_actor_guard(c)
            .await
            .map_err(|e| view_failure(kind, e))?;
        let core = self.core.catalog.clone();
        let actor = c.context.actor().clone();
        let work =
            c.context
                .lifetime
                .with_live(|| {
                    self.core
                        .preparation
                        .job(
                            crate::application::catalog_preparation::CatalogTaskPhase::Query,
                            move |runtime| {
                                CatalogJobResult::Queried(runtime.block_on(async move {
                                    core.query(&actor, id, &deadline()).await
                                }))
                            },
                        )
                        .map_err(buffer_view)
                })
                .map_err(|e| view_failure(kind, e))?;
        drop(guard);
        Ok(CatalogQueryReceipt(work.start(), id))
    }
    #[cfg(test)]
    pub(crate) fn install_worker_observer_for_test(
        &self,
        observer: crate::application::catalog_preparation::CatalogWorkerObserver,
    ) -> Result<(), UploadError> {
        self.core
            .preparation
            .install_observer_for_test(observer)
            .map_err(|_| UploadError::Conflict)
    }
    pub(super) fn seal(&self) {
        let sockets = {
            let mut registry = lock(&self.core.registry);
            registry.sealed = true;
            std::mem::take(&mut registry.sockets)
        };
        for socket in sockets.into_values() {
            socket.seal();
        }
        self.core.preparation.seal();
    }
}
impl NodeRuntime {
    pub(crate) fn catalog_upload_owner(
        &self,
        c: &SocketViewClient,
    ) -> Result<CatalogUploadOwner, UploadError> {
        let services = self
            .session_services
            .as_ref()
            .ok_or(UploadError::NodeStopping)?;
        Ok(CatalogUploadOwner {
            upload: services.uploads.open(c)?,
            catalog: services.catalog.clone(),
        })
    }
    pub(crate) fn catalog_management(&self) -> Result<NodeCatalogManagement, CatalogFailure> {
        self.session_services
            .as_ref()
            .map(|s| s.catalog_management.clone())
            .ok_or_else(|| failure(CatalogOperationKind::Query, CatalogError::NodeStopping))
    }
}

/// Only closed safe facts, never the private profile, launch arguments, paths or protection plaintext.
pub fn catalog_fact_metadata(f: &CatalogFact) -> serde_json::Value {
    use crate::domain::native_catalog::{
        CatalogAction as A, CatalogResult as R, CatalogState as S,
    };
    let result=f.result().map(|r|match r{R::Harness{harness,source,profile}=>serde_json::json!({"kind":"harness","harnessId":harness.encode(),"sourceId":source.encode(),"profileId":profile.id().encode(),"profileRevision":profile.revision().to_string()}),R::Profile{harness,source,profile}=>serde_json::json!({"kind":"profile","harnessId":harness.encode(),"sourceId":source.encode(),"profileId":profile.id().encode(),"profileRevision":profile.revision().to_string()}),R::Target{target,workspace,harness,source,profile}=>serde_json::json!({"kind":"target","targetId":target.encode(),"workspaceId":workspace.encode(),"harnessId":harness.encode(),"sourceId":source.encode(),"profileId":profile.id().encode(),"profileRevision":profile.revision().to_string()})});
    serde_json::json!({"catalogRequestId":f.request_id().encode(),"actorId":f.actor_id().encode(),"action":match f.action(){A::InstallHarness=>"install_harness",A::PublishProfile=>"publish_profile",A::BindTargetInitial=>"bind_target_initial"},"state":match f.state(){S::Registered=>"registered",S::Committed=>"committed",S::Rejected=>"rejected"},"registeredAtMs":f.registered_at_ms().to_string(),"registerAuthGeneration":f.register_auth_generation().to_string(),"admissionAuthGeneration":f.admission_auth_generation().map(|v|v.to_string()),"resultAtMs":f.result_at_ms().map(|v|v.to_string()),"result":result})
}

const fn commit_name(k: crate::domain::native_catalog::CommitKnowledge) -> &'static str {
    use crate::domain::native_catalog::CommitKnowledge::{
        Confirmed, NoWrite, RolledBack, Unknown, WritePossible,
    };
    match k {
        NoWrite => "no_write",
        WritePossible => "write_possible",
        Confirmed => "confirmed",
        RolledBack => "rolled_back",
        Unknown => "unknown",
    }
}
const fn cleanup_name(k: crate::domain::native_catalog::CleanupKnowledge) -> &'static str {
    use crate::domain::native_catalog::CleanupKnowledge::{
        ConfirmedOriginal, NoDetachedConnection, PendingOriginal, UnconfirmedOriginal,
    };
    match k {
        NoDetachedConnection => "no_detached_connection",
        PendingOriginal => "pending_original",
        ConfirmedOriginal => "confirmed_original",
        UnconfirmedOriginal => "unconfirmed_original",
    }
}
const fn operation_name(k: CatalogOperationKind) -> &'static str {
    match k {
        CatalogOperationKind::Register => "register",
        CatalogOperationKind::Apply => "apply",
        CatalogOperationKind::Query => "query",
        CatalogOperationKind::Resolve => "resolve",
        CatalogOperationKind::ResolveRead => "resolve_read",
        CatalogOperationKind::Reconcile => "reconcile",
    }
}
fn sql_metadata(s: &crate::domain::native_catalog::CatalogSqlObservationSet) -> serde_json::Value {
    use crate::domain::native_catalog::{CatalogSqlRole as R, TransactionKnowledge as T};
    let slots=(0..2).map(|index|s.slot(index).map(|o|serde_json::json!({"role":match o.role(){R::IdentityRead=>"identity_read",R::RegisteredSnapshot=>"registered_snapshot",R::RegisterWrite=>"register_write",R::ApplyWrite=>"apply_write",R::QueryRead=>"query_read",R::ConfigurationRead=>"configuration_read",R::RecoveryRead=>"recovery_read"},"transaction":match o.transaction(){T::NoTransaction=>"no_transaction",T::BegunRead=>"begun_read",T::BegunWrite=>"begun_write",T::CommitPolling=>"commit_polling",T::CommitConfirmed=>"commit_confirmed",T::RollbackConfirmed=>"rollback_confirmed",T::EffectUnknown=>"effect_unknown"},"cleanup":cleanup_name(o.cleanup())}))).collect::<Vec<_>>();
    serde_json::json!({"operation":operation_name(s.operation_kind()),"slots":slots})
}
fn original_failure_metadata(f: &CatalogFailure) -> serde_json::Value {
    use crate::domain::native_catalog::CatalogError::{
        Allocation, Cancelled, CommitUnconfirmed, Conflict, CorruptState, InvalidActor,
        InvalidConfiguration, InvalidSource, InvalidTarget, NodeStopping, ProtectionUnavailable,
        RandomUnavailable, SqlCloseUnconfirmed, StorageUnavailable, UnknownRequest,
    };
    let code = match f.kind() {
        InvalidActor => "invalid_actor",
        InvalidSource => "invalid_source",
        InvalidTarget => "invalid_target",
        UnknownRequest => "unknown_request",
        InvalidConfiguration => "invalid_configuration",
        Conflict => "conflict",
        CorruptState => "corrupt_state",
        RandomUnavailable => "random_unavailable",
        Allocation => "allocation_failed",
        ProtectionUnavailable => "protection_unavailable",
        StorageUnavailable => "storage_unavailable",
        Cancelled => "cancelled",
        NodeStopping => "node_stopping",
        CommitUnconfirmed => "commit_unconfirmed",
        SqlCloseUnconfirmed { .. } => "sql_close_unconfirmed",
    };
    serde_json::json!({"kind":"catalog","error":code,"catalogRequestId":f.request_id().map(super::super::domain::native_catalog::CatalogRequestId::encode),"commit":commit_name(f.commit()),"cleanup":cleanup_name(f.cleanup()),"sqlObservation":sql_metadata(f.sql_observation()),"sqlCloseCommitted":match f.kind(){SqlCloseUnconfirmed{committed}=>Some(committed),_=>None}})
}
pub fn catalog_receipt_metadata(r: &CatalogReceipt) -> serde_json::Value {
    serde_json::json!({"fact":catalog_fact_metadata(r.fact()),"auditWarning":match r.audit_warning(){crate::domain::native_catalog::AuditWarning::None=>"none",crate::domain::native_catalog::AuditWarning::Unavailable=>"unavailable",crate::domain::native_catalog::AuditWarning::Full=>"full"},"sqlObservation":sql_metadata(r.sql_observation())})
}
pub fn catalog_failure_metadata(f: &CatalogCommandFailure) -> serde_json::Value {
    match f {
        CatalogCommandFailure::Catalog(original) => original_failure_metadata(original),
        CatalogCommandFailure::WorkerUnconfirmed {
            request,
            observation,
            original,
        } => {
            use crate::application::catalog_preparation::{
                CatalogTaskExit as E, CatalogTaskJoinState as J, CatalogTaskPhase as P,
            };
            let known = original.as_ref().map(|r| match r {
                CatalogKnownResult::Registered(Ok(id)) => {
                    serde_json::json!({"kind":"registered","catalogRequestId":id.encode()})
                }
                CatalogKnownResult::Applied(Ok(r)) => catalog_receipt_metadata(r),
                CatalogKnownResult::Queried(Ok(f)) => catalog_fact_metadata(f),
                CatalogKnownResult::Registered(Err(e))
                | CatalogKnownResult::Applied(Err(e))
                | CatalogKnownResult::Queried(Err(e)) => original_failure_metadata(e),
            });
            serde_json::json!({"kind":"worker_unconfirmed","catalogRequestId":request.map(super::super::domain::native_catalog::CatalogRequestId::encode),"commit":"unknown","cleanup":"unconfirmed_original","sqlObservation":null,"originalResult":known,"worker":{"phase":match observation.phase{P::Append=>"append",P::Prepare=>"prepare",P::Register=>"register",P::Apply=>"apply",P::Query=>"query",P::Dispose=>"dispose",P::Supervise=>"supervise"},"join":match observation.join{J::NeverStarted=>"never_started",J::Pending=>"pending",J::StartUnconfirmed=>"start_unconfirmed",J::Joined(E::Completed)=>"joined_completed",J::Joined(E::Failed(_))=>"joined_failed"}}})
        }
    }
}
