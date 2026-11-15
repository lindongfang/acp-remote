//! Stable ownership of one Node's original acquisition and closure operation.
#![allow(
    clippy::significant_drop_tightening,
    clippy::too_many_lines,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    AppError, ApplicationPolicy, Arc, DataDirectory, Duration, IpcControl, LocalIpcAddress,
    ManagementService, NodeConfig, NodeIdentity, NodeRuntime, PairingService,
    SHUTDOWN_RESPONSE_TIMEOUT, SHUTDOWN_TIMEOUT, SecretProtector, ShutdownResult, Storage,
    TcpListener, TlsAcceptor, identity, ipc, pairing_http, run_network, storage_startup_reason,
    tls, watch,
};
#[cfg(test)]
use super::{NetworkDevelopmentHooks, Router, TcpStream, run_network_inner, timeout};
#[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
use super::{
    NodeShutdownDisposition, NodeShutdownFailure, NodeShutdownObserver, NodeShutdownPhase,
    NodeShutdownStage, NodeStageOutcome,
};
use crate::session_storage::sqlite_lifecycle::{
    StorageLifecycleHost, StorageOpenOwner, StorageOpenRoots,
};
use futures_util::FutureExt;
use std::{
    future::pending,
    panic::AssertUnwindSafe,
    sync::{
        Mutex, MutexGuard,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::{
    runtime::Handle,
    sync::{Mutex as AsyncMutex, oneshot},
    task::JoinHandle,
    time::{Instant, sleep_until},
};

fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Ready {
    Starting,
    Available,
    Failed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Actual {
    Acquiring,
    Running,
    Closing,
    RetainedUnconfirmed,
    DriverUnconfirmed,
    NoDatabaseAcquired,
    Joined,
}
#[derive(Clone)]
struct StopRequest {
    started: Instant,
    timeout: Duration,
}
#[cfg(test)]
pub(super) struct DirectoryView {
    database: std::path::PathBuf,
}
#[cfg(test)]
impl DirectoryView {
    pub(super) fn database_path(&self) -> std::path::PathBuf {
        self.database.clone()
    }
}
#[derive(Default)]
struct Resources {
    directory: Option<Arc<DataDirectory>>,
    storage: Option<Arc<Storage>>,
    #[cfg(windows)]
    session_services: Option<Arc<super::session_services::OwnedNodeSessionServices>>,
    storage_host: Option<Arc<crate::session_storage::sqlite_lifecycle::StorageLifecycleHost>>,
    identity: Option<Arc<NodeIdentity>>,
    pairing: Option<Arc<PairingService>>,
    management: Option<Arc<ManagementService>>,
    policy: Option<Arc<ApplicationPolicy>>,
    control: Option<IpcControl>,
    network_stop: Option<watch::Sender<bool>>,
    ipc_address: Option<LocalIpcAddress>,
    startup_error: Option<AppError>,
    failures: Vec<String>,
    database_started: bool,
}
#[derive(Default)]
struct Tasks {
    network: Option<JoinHandle<Result<(), String>>>,
    ipc: Option<JoinHandle<Result<(), AppError>>>,
}
pub(super) struct NodeCloseOperation {
    config: NodeConfig,
    runtime: Handle,
    protector: Arc<dyn SecretProtector>,
    resources: Mutex<Resources>,
    tasks: Mutex<Option<Arc<AsyncMutex<Tasks>>>>,
    stop: watch::Sender<Option<StopRequest>>,
    ready: watch::Sender<Ready>,
    budget: watch::Sender<ShutdownResult>,
    actual: watch::Sender<Actual>,
    data_handles: AtomicUsize,
    #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
    observer: Arc<NodeShutdownObserver>,
    #[cfg(test)]
    hooks: DevHooks,
}
pub(super) struct Host {
    pub(super) operation: Arc<NodeCloseOperation>,
    task: AsyncMutex<Option<JoinHandle<()>>>,
    retention: Mutex<Option<Arc<Self>>>,
}
/// Keeps the original Node operation and runtime work alive through true closure.
pub struct NodeProcessOwner {
    host: Arc<Host>,
    armed: bool,
}
#[cfg(all(test, windows))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FactoryObservation {
    pub(super) operation: usize,
    pub(super) stage: super::session_services::FactoryStage,
    pub(super) claim: bool,
    pub(super) supervisor: Option<usize>,
    pub(super) services_installed: bool,
    pub(super) ready: bool,
}
impl NodeProcessOwner {
    /// Registers ownership before any acquisition await on the given original runtime.
    pub fn begin(
        runtime: &Handle,
        config: NodeConfig,
        protector: Arc<dyn SecretProtector>,
    ) -> Self {
        Self::begin_inner(
            runtime,
            config,
            protector,
            #[cfg(test)]
            DevHooks::default(),
        )
    }
    #[cfg(test)]
    pub(super) fn begin_control_only_for_test(
        runtime: &Handle,
        config: NodeConfig,
        protector: Arc<dyn SecretProtector>,
    ) -> Self {
        Self::begin_inner(
            runtime,
            config,
            protector,
            DevHooks {
                control_only: true,
                ..DevHooks::default()
            },
        )
    }
    #[cfg(all(test, windows))]
    pub(super) fn begin_for_factory_test(
        runtime: &Handle,
        config: NodeConfig,
        protector: Arc<dyn SecretProtector>,
        factory: super::session_services::FactoryTestHooks,
    ) -> Self {
        Self::begin_inner(
            runtime,
            config,
            protector,
            DevHooks {
                factory,
                ..DevHooks::default()
            },
        )
    }
    #[cfg(all(test, windows))]
    pub(super) fn factory_observation_for_test(&self) -> FactoryObservation {
        let roots = lock(&self.host.operation.resources);
        let (claim, supervisor) = roots.storage.as_ref().map_or((false, None), |s| {
            s.original_runtime_registration_for_test()
        });
        FactoryObservation {
            operation: Arc::as_ptr(&self.host.operation) as usize,
            stage: self.host.operation.hooks.factory.stage(),
            claim,
            supervisor,
            services_installed: roots.session_services.is_some(),
            ready: *self.host.operation.ready.borrow() == Ready::Available,
        }
    }
    fn begin_inner(
        runtime: &Handle,
        config: NodeConfig,
        protector: Arc<dyn SecretProtector>,
        #[cfg(test)] hooks: DevHooks,
    ) -> Self {
        let (ready, _) = watch::channel(Ready::Starting);
        let (budget, _) = watch::channel(ShutdownResult::Pending);
        let (stop, _) = watch::channel(None);
        let (actual, _) = watch::channel(Actual::Acquiring);
        let operation = Arc::new(NodeCloseOperation {
            config,
            runtime: runtime.clone(),
            protector,
            resources: Mutex::new(Resources::default()),
            tasks: Mutex::new(None),
            stop,
            ready,
            budget,
            actual,
            data_handles: AtomicUsize::new(0),
            #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
            observer: Arc::new(NodeShutdownObserver::new()),
            #[cfg(test)]
            hooks,
        });
        let (installed, gate) = oneshot::channel();
        let driver = Arc::clone(&operation);
        let task = runtime.spawn(async move {
            if gate.await.is_ok() {
                driver.drive().await;
            }
        });
        let host = Arc::new(Host {
            operation,
            task: AsyncMutex::new(Some(task)),
            retention: Mutex::new(None),
        });
        *lock(&host.retention) = Some(host.clone());
        // No external acquisition can run until the original task slot is installed.
        let _ = installed.send(());
        Self { host, armed: true }
    }
    /// Observes readiness; cancelling this wait does not cancel acquisition.
    /// # Errors
    /// Returns the original first startup failure or rejection after sealing.
    pub async fn ready(&self) -> Result<NodeRuntime, AppError> {
        self.host.ready().await
    }
    /// Waits on the original top-level task without moving its handle into this waiter.
    /// # Errors
    /// A driver failure is unconfirmed and must not be reported as released.
    pub async fn join_actual(&self) -> Result<(), AppError> {
        self.host.join_actual().await
    }
    pub(super) const fn disarm(&mut self) {
        self.armed = false;
    }
}
impl Drop for NodeProcessOwner {
    fn drop(&mut self) {
        if self.armed {
            self.host.operation.request_stop(None, SHUTDOWN_TIMEOUT);
        }
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.operation.request_stop(None, SHUTDOWN_TIMEOUT);
    }
}
struct ReadyWaitGuard<'a> {
    operation: &'a NodeCloseOperation,
    armed: bool,
}
impl Drop for ReadyWaitGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.operation.request_stop(None, SHUTDOWN_TIMEOUT);
        }
    }
}
impl Host {
    async fn ready(self: &Arc<Self>) -> Result<NodeRuntime, AppError> {
        let mut cancelled = ReadyWaitGuard {
            operation: &self.operation,
            armed: true,
        };
        let mut ready = self.operation.ready.subscribe();
        loop {
            let status = *ready.borrow_and_update();
            match status {
                Ready::Starting => {
                    if ready.changed().await.is_err() {
                        return Err(AppError::Startup("node driver unavailable".into()));
                    }
                }
                Ready::Failed => {
                    cancelled.armed = false;
                    return Err(lock(&self.operation.resources)
                        .startup_error
                        .take()
                        .unwrap_or_else(|| {
                            AppError::Startup("original acquisition failed or was stopped".into())
                        }));
                }
                Ready::Available => {
                    // Ready publication, stop and projection installation use this same fence.
                    let roots = lock(&self.operation.resources);
                    if self.operation.stop.borrow().is_some() {
                        return Err(AppError::Startup("node is stopping".into()));
                    }
                    let ipc_address = roots
                        .ipc_address
                        .clone()
                        .ok_or_else(|| AppError::Startup("IPC readiness unavailable".into()))?;
                    let control = roots
                        .control
                        .clone()
                        .ok_or_else(|| AppError::Startup("control readiness unavailable".into()))?;
                    #[cfg(test)]
                    let data_dir = roots.directory.as_ref().map(|d| {
                        Arc::new(DirectoryView {
                            database: d.database_path(),
                        })
                    });
                    self.operation.data_handles.fetch_add(1, Ordering::AcqRel);
                    cancelled.armed = false;
                    return Ok(NodeRuntime {
                        lifecycle: Arc::clone(self),
                        control,
                        storage: roots.storage.clone(),
                        #[cfg(windows)]
                        session_services: roots.session_services.clone(),
                        identity: roots.identity.clone(),
                        pairing: roots.pairing.clone(),
                        #[cfg(test)]
                        management: roots.management.clone(),
                        application_policy: roots.policy.clone(),
                        #[cfg(test)]
                        data_dir,
                        listen: self.operation.config.listen,
                        ipc_address,
                        #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
                        shutdown_observer: Arc::clone(&self.operation.observer),
                    });
                }
            }
        }
    }
    pub(super) async fn budget(&self) -> Result<(), AppError> {
        let mut rx = self.operation.budget.subscribe();
        loop {
            let current = rx.borrow_and_update().clone();
            match current {
                ShutdownResult::Pending => {
                    if rx.changed().await.is_err() {
                        return Err(AppError::Shutdown(
                            "original budget observation unavailable".into(),
                        ));
                    }
                }
                ShutdownResult::Completed => return Ok(()),
                ShutdownResult::Failed(message) => return Err(AppError::Shutdown(message)),
            }
        }
    }
    pub(super) fn release_data(&self) {
        if self.operation.data_handles.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.operation.request_stop(None, SHUTDOWN_TIMEOUT);
        }
    }
    pub(super) async fn join_actual(&self) -> Result<(), AppError> {
        let mut slot = self.task.lock().await;
        if let Some(task) = slot.as_mut() {
            match task.await {
                Ok(()) => {
                    *slot = None;
                }
                Err(_) => {
                    return Err(AppError::Shutdown(
                        "original Node driver join is unconfirmed".into(),
                    ));
                }
            }
        }
        drop(slot);
        if self.operation.resources.is_poisoned()
            || self.operation.tasks.is_poisoned()
            || self.retention.is_poisoned()
        {
            return Err(AppError::Shutdown(
                "original Node ownership state is unconfirmed".into(),
            ));
        }
        if *self.operation.actual.borrow() == Actual::NoDatabaseAcquired
            || *self.operation.actual.borrow() == Actual::Joined
        {
            {
                let mut roots = lock(&self.operation.resources);
                #[cfg(windows)]
                roots.session_services.take();
                roots.storage.take();
                roots.storage_host.take();
                roots.identity.take();
                roots.pairing.take();
                roots.management.take();
                roots.policy.take();
                roots.directory.take();
            }
            lock(&self.retention).take();
            return Ok(());
        }
        Err(AppError::Shutdown(
            "physical closure has no complete original receipt".into(),
        ))
    }
}
impl NodeCloseOperation {
    #[cfg(test)]
    pub(super) async fn replace_ipc_for_test(&self, replacement: JoinHandle<Result<(), AppError>>) {
        let slots = lock(&self.tasks)
            .clone()
            .expect("original task slots installed");
        let mut tasks = slots.lock().await;
        if let Some(task) = tasks.ipc.take() {
            task.abort();
            let _ = task.await;
        }
        tasks.ipc = Some(replacement);
    }
    fn seal_resources(roots: &Resources) {
        #[cfg(windows)]
        if let Some(services) = &roots.session_services {
            services.seal();
        }
        if let Some(control) = &roots.control {
            control.request_stop();
        }
        if let Some(management) = &roots.management {
            management.begin_shutdown();
        }
        if let Some(storage) = &roots.storage {
            storage.begin_runtime_shutdown();
        } else if let Some(host) = &roots.storage_host {
            host.owner.request_stop();
        }
        if let Some(stop) = &roots.network_stop {
            stop.send_replace(true);
        }
    }
    pub(super) fn request_stop(&self, failure: Option<String>, timeout: Duration) {
        let mut roots = lock(&self.resources);
        if let Some(message) = failure
            && !roots.failures.contains(&message)
        {
            #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
            self.observer.flag(NodeShutdownFailure::InitialFailure);
            roots.failures.push(message);
        }
        // Hold the resource fence through sealing and stop publication. No await occurs here.
        Self::seal_resources(&roots);
        self.stop.send_modify(|request| match request {
            Some(request) => request.timeout = request.timeout.min(timeout),
            None => {
                *request = Some(StopRequest {
                    started: Instant::now(),
                    timeout,
                });
            }
        });
        drop(roots);
    }
    fn fail(&self, message: String) {
        let mut roots = lock(&self.resources);
        if !roots.failures.contains(&message) {
            roots.failures.push(message);
        }
    }
    async fn drive(self: Arc<Self>) {
        let result = AssertUnwindSafe(self.run_inner()).catch_unwind().await;
        if result.is_err() {
            self.actual.send_replace(Actual::DriverUnconfirmed);
            self.fail("original Node driver unwound; resources remain retained".into());
            self.ready.send_replace(Ready::Failed);
            self.request_stop(None, SHUTDOWN_TIMEOUT);
            self.publish_budget("original Node driver unwound; physical closure unconfirmed");
        }
        if result.is_ok()
            && (*self.actual.borrow() == Actual::Joined || !lock(&self.resources).database_started)
        {
            if *self.actual.borrow() != Actual::Joined {
                self.actual.send_replace(Actual::NoDatabaseAcquired);
            }
            return;
        }
        // The outer driver remains alive with this exact ledger after Unknown or unwind.
        // Current SQLx windows provide no event that could safely upgrade physical Unknown.
        pending::<()>().await;
        drop(self);
    }
    async fn run_inner(&self) {
        // Acquisition remains the original future; only budget observation runs alongside it.
        tokio::join!(self.run_resources(), self.observe_stop_budget());
    }
    async fn observe_stop_budget(&self) {
        let mut requests = self.stop.subscribe();
        let mut budget = self.budget.subscribe();
        loop {
            if !matches!(*budget.borrow_and_update(), ShutdownResult::Pending) {
                return;
            }
            let request = requests.borrow_and_update().clone();
            if let Some(request) = request {
                let deadline = request.started
                    + request
                        .timeout
                        .saturating_sub(SHUTDOWN_RESPONSE_TIMEOUT.min(request.timeout));
                tokio::select! {
                    () = sleep_until(deadline) => {
                        self.publish_budget("shutdown budget exceeded while closing storage and original resource users");
                        return;
                    }
                    changed = requests.changed() => { if changed.is_err() { return; } }
                    changed = budget.changed() => { if changed.is_err() { return; } }
                }
            } else {
                tokio::select! {
                    changed = requests.changed() => { if changed.is_err() { return; } }
                    changed = budget.changed() => { if changed.is_err() { return; } }
                }
            }
        }
    }
    async fn run_resources(&self) {
        if let Err(error) = self.acquire().await {
            let message = error.to_string();
            {
                lock(&self.resources).startup_error = Some(error);
            }
            self.fail(message);
            self.ready.send_replace(Ready::Failed);
            self.request_stop(None, SHUTDOWN_TIMEOUT);
        } else {
            #[cfg(all(test, windows))]
            {
                self.hooks
                    .factory
                    .mark(super::session_services::FactoryStage::BeforeReady);
                if let Some(barrier) = &self.hooks.factory.before_ready {
                    barrier.pause().await;
                }
            }
            let roots = lock(&self.resources);
            if self.stop.borrow().is_none() {
                self.actual.send_replace(Actual::Running);
                self.ready.send_replace(Ready::Available);
                #[cfg(all(test, windows))]
                self.hooks
                    .factory
                    .mark(super::session_services::FactoryStage::Ready);
            } else {
                self.ready.send_replace(Ready::Failed);
            }
            drop(roots);
        }
        self.wait_stop().await;
        self.actual.send_replace(Actual::Closing);
        #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
        self.observer.phase(NodeShutdownPhase::Entered);
        let shutdown_deadline = self
            .stop
            .borrow()
            .clone()
            .map_or_else(Instant::now, |request| {
                request.started
                    + request
                        .timeout
                        .saturating_sub(SHUTDOWN_RESPONSE_TIMEOUT.min(request.timeout))
            });
        let cleanup = self.close_original(shutdown_deadline);
        tokio::pin!(cleanup);
        let mut request_rx = self.stop.subscribe();
        let mut completed = false;
        loop {
            let request = request_rx.borrow_and_update().clone();
            let Some(request) = request else {
                self.fail("original stop registration unavailable".into());
                break;
            };
            let deadline = request.started
                + request
                    .timeout
                    .saturating_sub(SHUTDOWN_RESPONSE_TIMEOUT.min(request.timeout));
            tokio::select! {
                () = &mut cleanup => { completed = true; self.publish_close_result(); break; }
                () = sleep_until(deadline) => {
                    #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
                    self.observer.fail(NodeShutdownStage::Storage, NodeStageOutcome::Deadline, NodeShutdownFailure::StorageDeadline);
                    self.publish_budget("shutdown budget exceeded while closing storage and original resource users");
                    break;
                }
                changed = request_rx.changed() => { if changed.is_err() { self.publish_budget("original stop observation unavailable"); break; } }
            }
        }
        let request = self.stop.borrow().clone();
        let response_deadline = request.map_or_else(Instant::now, |r| r.started + r.timeout);
        let responses = async {
            let control = lock(&self.resources).control.clone();
            if let Some(control) = control
                && let Err(error) = control.finish_stop_responses(response_deadline).await
            {
                #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
                self.observer.fail(
                    NodeShutdownStage::StopResponses,
                    NodeStageOutcome::ReturnedError,
                    NodeShutdownFailure::StopResponsesReturnedError,
                );
                self.fail(error);
            }
        };
        let remaining = async {
            if !completed {
                cleanup.await;
            }
        };
        tokio::join!(remaining, responses);
        let host = lock(&self.resources).storage_host.clone();
        let joined = if let Some(host) = host {
            host.observe_actual().await
        } else {
            !lock(&self.resources).database_started
        };
        if joined && !self.resources.is_poisoned() && !self.tasks.is_poisoned() {
            self.actual
                .send_replace(if lock(&self.resources).database_started {
                    Actual::Joined
                } else {
                    Actual::NoDatabaseAcquired
                });
        } else {
            self.actual.send_replace(Actual::RetainedUnconfirmed);
        }
    }
    async fn wait_stop(&self) {
        let mut registered = self.stop.subscribe();
        let control = lock(&self.resources).control.clone();
        if let Some(control) = control {
            let mut stop = control.stop_receiver();
            loop {
                if registered.borrow().is_some() {
                    return;
                }
                if *stop.borrow_and_update() {
                    self.request_stop(control.failure(), SHUTDOWN_TIMEOUT);
                    return;
                }
                tokio::select! {
                    changed = registered.changed() => { if changed.is_err() { self.request_stop(None, SHUTDOWN_TIMEOUT); return; } }
                    changed = stop.changed() => { if changed.is_err() { self.request_stop(Some("owner stop channel closed".into()), SHUTDOWN_TIMEOUT); return; } }
                }
            }
        } else if registered.borrow().is_none() {
            self.request_stop(None, SHUTDOWN_TIMEOUT);
        }
    }
    fn publish_close_result(&self) {
        let roots = lock(&self.resources);
        let joined = roots.storage_host.as_ref().is_some_and(|h| {
            h.owner.state.actual() == crate::session_storage::sqlite_lifecycle::Actual::Joined
        }) || !roots.database_started;
        if joined
            && roots.failures.is_empty()
            && !self.resources.is_poisoned()
            && !self.tasks.is_poisoned()
        {
            let published = self.budget.send_if_modified(|current| {
                if !matches!(current, ShutdownResult::Pending) {
                    return false;
                }
                *current = ShutdownResult::Completed;
                true
            });
            if published {
                if let Some(control) = &roots.control {
                    control.complete(ShutdownResult::Completed);
                }
                #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
                {
                    self.observer.phase(NodeShutdownPhase::ReportPublished);
                    self.observer.select_result(NodeShutdownDisposition::Ok);
                }
            }
        } else {
            drop(roots);
            self.publish_budget("physical SQLite worker closure is unconfirmed");
        }
    }
    fn publish_budget(&self, fallback: &str) {
        let roots = lock(&self.resources);
        let message = if roots.failures.is_empty() {
            fallback.to_owned()
        } else {
            roots.failures.join("; ")
        };
        let result = ShutdownResult::Failed(message);
        let published = self.budget.send_if_modified(|current| {
            if !matches!(current, ShutdownResult::Pending) {
                return false;
            }
            *current = result.clone();
            true
        });
        if !published {
            return;
        }
        if let Some(control) = &roots.control {
            control.complete(result);
        }
        drop(roots);
        #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
        {
            self.observer.phase(NodeShutdownPhase::ReportPublished);
            self.observer.select_result(NodeShutdownDisposition::Error);
        }
    }
    async fn acquire(&self) -> Result<(), AppError> {
        let config = &self.config;
        let directory = Arc::new(DataDirectory::acquire(&config.data_dir)?);
        {
            let mut roots = lock(&self.resources);
            roots.directory = Some(Arc::clone(&directory));
            roots.database_started = true;
        }
        #[cfg(not(test))]
        let prepared = StorageOpenOwner::prepare(
            &self.runtime,
            StorageOpenRoots::node(directory.clone(), self.protector.clone()),
        );
        #[cfg(test)]
        let prepared = StorageOpenOwner::prepare_for_test(
            &self.runtime,
            StorageOpenRoots::node(directory.clone(), self.protector.clone()),
            self.hooks.sqlite.clone().unwrap_or_default(),
        );
        let storage_owner = prepared.map_err(|e| AppError::Startup(storage_startup_reason(&e)))?;
        let storage_host = StorageLifecycleHost::install(storage_owner.clone());
        lock(&self.resources).storage_host = Some(storage_host);
        #[cfg(test)]
        if let Some(barrier) = &self.hooks.before_open {
            barrier.pause().await;
        }

        let ticket = storage_owner
            .start()
            .map_err(|e| AppError::Startup(storage_startup_reason(&e)))?;
        let storage = ticket
            .ready()
            .await
            .map_err(|e| AppError::Startup(storage_startup_reason(&e)))?;
        {
            let mut roots = lock(&self.resources);
            roots.storage = Some(Arc::clone(&storage));
            if self.stop.borrow().is_some() {
                storage.begin_runtime_shutdown();
            }
            drop(roots);
        }
        #[cfg(test)]
        assert!(
            !self.hooks.panic_after_storage,
            "development driver unwind after original Storage registration"
        );
        directory.secure_managed_files()?;
        let identity = Arc::new(
            identity::initialize(directory.path(), &storage, self.protector.as_ref()).await?,
        );
        {
            lock(&self.resources).identity = Some(Arc::clone(&identity));
        }
        let trust = storage.trust_store();
        let pairing = Arc::new(PairingService::new(
            Arc::clone(&identity),
            config.canonical_origin.clone(),
            trust.clone(),
        )?);
        {
            lock(&self.resources).pairing = Some(Arc::clone(&pairing));
        }
        self.install_transports(&directory, identity, pairing, trust)
            .await
    }
    #[cfg(windows)]
    async fn install_session_services(
        &self,
        identity: &NodeIdentity,
        policy: &Arc<ApplicationPolicy>,
        management: &ManagementService,
    ) -> Result<(), AppError> {
        #[cfg(test)]
        let control_only = self.hooks.control_only;
        #[cfg(not(test))]
        let control_only = false;
        if !control_only {
            #[cfg(test)]
            {
                self.hooks
                    .factory
                    .mark(super::session_services::FactoryStage::BeforeServices);
                if let Some(barrier) = &self.hooks.factory.before_services {
                    barrier.pause().await;
                }
            }
            let storage = lock(&self.resources)
                .storage
                .clone()
                .ok_or_else(|| AppError::Startup("product storage unavailable".into()))?;
            let services = Arc::new(
                super::session_services::OwnedNodeSessionServices::start(
                    &storage,
                    identity,
                    policy.clone(),
                    self.protector.clone(),
                    management.socket_authority(),
                    #[cfg(test)]
                    &self.hooks.factory,
                )
                .await?,
            );
            #[cfg(test)]
            if self.hooks.factory.reject_services {
                return Err(AppError::Startup(
                    "injected session service installation failure".into(),
                ));
            }
            {
                let mut roots = lock(&self.resources);
                roots.session_services = Some(services.clone());
                if self.stop.borrow().is_some() {
                    Self::seal_resources(&roots);
                }
            }
            management
                .install_session_services(services.management_services())
                .map_err(|_| {
                    AppError::Startup("product session services installation failed".into())
                })?;
        }
        Ok(())
    }
    fn spawn_https(
        listener: TcpListener,
        tls_config: Arc<rustls::ServerConfig>,
        network_stop_rx: watch::Receiver<bool>,
        router: axum::Router,
        network_control: IpcControl,
    ) -> JoinHandle<Result<(), String>> {
        tokio::spawn(async move {
            let result = run_network(
                listener,
                TlsAcceptor::from(tls_config),
                network_stop_rx,
                router,
            )
            .await;
            if let Err(error) = &result {
                network_control.mark_failed(error.clone());
            }
            result
        })
    }
    #[allow(clippy::too_many_lines)]
    async fn install_transports(
        &self,
        directory: &DataDirectory,
        identity: Arc<NodeIdentity>,
        pairing: Arc<PairingService>,
        trust: crate::trust::TrustStore,
    ) -> Result<(), AppError> {
        let config = &self.config;
        let tls_config = tls::load_server_config(&config.tls_cert, &config.tls_key)?;
        let address = LocalIpcAddress::for_data_directory(directory.path())?;
        let first_pipe = address
            .bind_first()
            .map_err(|e| AppError::Ipc(format!("binding owner control failed: {e}")))?;
        let (control, _) = IpcControl::new(
            identity.node_id().to_owned(),
            config.canonical_origin.clone(),
        );
        let policy = Arc::new(ApplicationPolicy::new(
            identity.node_id().to_owned(),
            trust.clone(),
            &pairing,
            control.clone(),
        ));
        let management = Arc::new(
            ManagementService::new(
                Arc::clone(&identity),
                config.canonical_origin.clone(),
                trust.clone(),
                Arc::clone(&pairing),
                control.clone(),
                Arc::clone(&policy),
            )
            .map_err(|e| {
                AppError::Startup(format!("management transport initialization failed: {e}"))
            })?,
        );
        {
            let mut roots = lock(&self.resources);
            roots.control = Some(control.clone());
            roots.policy = Some(policy.clone());
            roots.management = Some(management.clone());
            if self.stop.borrow().is_some() {
                Self::seal_resources(&roots);
            }
        }
        #[cfg(windows)]
        self.install_session_services(&identity, &policy, &management)
            .await?;
        let listener = TcpListener::bind(config.listen)
            .await
            .map_err(|source| AppError::Io {
                context: "binding the HTTPS listener",
                source,
            })?;
        let (network_stop, network_stop_rx) = watch::channel(false);
        let router = pairing_http::router(
            Arc::clone(&pairing),
            config.canonical_origin.clone(),
            control.clone(),
        )
        .merge(management.router());
        {
            let mut roots = lock(&self.resources);
            roots.control = Some(control.clone());
            roots.policy = Some(policy);
            roots.management = Some(management);
            roots.network_stop = Some(network_stop);
            roots.ipc_address = Some(address.clone());
            let published = self.budget.borrow().clone();
            if !matches!(published, ShutdownResult::Pending) {
                control.complete(published);
            }
            if self.stop.borrow().is_some() {
                Self::seal_resources(&roots);
            }
            drop(roots);
        }
        let network_task = Self::spawn_https(
            listener,
            tls_config,
            network_stop_rx,
            router,
            control.clone(),
        );
        // The only consumer of task slots is this driver. Installation contains no await.
        #[cfg(windows)]
        let ipc_catalog = lock(&self.resources)
            .session_services
            .as_ref()
            .map(|services| services.configuration.clone());
        #[cfg(not(windows))]
        let ipc_catalog = None;
        let ipc_control = control;
        #[cfg(test)]
        let ipc_hooks = self.hooks.ipc.clone();
        let ipc_task = tokio::spawn(async move {
            #[cfg(test)]
            let result = ipc::serve_development_with_catalog(
                first_pipe,
                address,
                ipc_control.clone(),
                trust,
                pairing,
                ipc_catalog,
                ipc_hooks,
            )
            .await;
            #[cfg(not(test))]
            let result = ipc::serve_with_catalog(
                first_pipe,
                address,
                ipc_control.clone(),
                trust,
                pairing,
                ipc_catalog,
            )
            .await;
            if let Err(error) = &result {
                ipc_control.mark_failed(error.to_string());
            }
            result
        });
        *lock(&self.tasks) = Some(Arc::new(AsyncMutex::new(Tasks {
            network: Some(network_task),
            ipc: Some(ipc_task),
        })));
        Ok(())
    }
    #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
    fn enter_stage(&self, stage: NodeShutdownStage, present: bool) {
        if matches!(*self.budget.borrow(), ShutdownResult::Pending) {
            self.observer.enter(stage, present);
        }
    }
    async fn join_network_original(
        &self,
        task: Option<&mut JoinHandle<Result<(), String>>>,
    ) -> bool {
        if let Some(task) = task {
            match task.await {
                Ok(Ok(())) => true,
                Ok(Err(error)) => {
                    #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
                    self.observer.fail(
                        NodeShutdownStage::Network,
                        NodeStageOutcome::ReturnedError,
                        NodeShutdownFailure::NetworkReturnedError,
                    );
                    self.fail(error);
                    true
                }
                Err(_) => {
                    #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
                    self.observer.fail(
                        NodeShutdownStage::Network,
                        NodeStageOutcome::JoinError,
                        NodeShutdownFailure::NetworkJoinError,
                    );
                    self.fail("original network tree join is unconfirmed".into());
                    false
                }
            }
        } else {
            true
        }
    }
    async fn join_ipc_original(&self, task: Option<&mut JoinHandle<Result<(), AppError>>>) -> bool {
        if let Some(task) = task {
            match task.await {
                Ok(Ok(())) => true,
                Ok(Err(error)) => {
                    #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
                    self.observer.fail(
                        NodeShutdownStage::OwnerIpcTask,
                        NodeStageOutcome::ReturnedError,
                        NodeShutdownFailure::OwnerIpcReturnedError,
                    );
                    self.fail(error.to_string());
                    true
                }
                Err(_) => {
                    #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
                    self.observer.fail(
                        NodeShutdownStage::OwnerIpcTask,
                        NodeStageOutcome::JoinError,
                        NodeShutdownFailure::OwnerIpcJoinError,
                    );
                    self.fail("original IPC tree join is unconfirmed".into());
                    false
                }
            }
        } else {
            true
        }
    }
    async fn close_original(&self, shutdown_deadline: Instant) {
        let (storage, pairing, management) = {
            let r = lock(&self.resources);
            (r.storage.clone(), r.pairing.clone(), r.management.clone())
        };
        #[cfg(windows)]
        let session_services = lock(&self.resources).session_services.clone();
        let (users_done, users) = oneshot::channel();
        let transport = async {
            let slots = lock(&self.tasks).clone();
            let Some(slots) = slots else {
                let _ = users_done.send(());
                return;
            };
            let mut tasks = slots.lock().await;
            #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
            self.enter_stage(NodeShutdownStage::Network, tasks.network.is_some());
            let network = self.join_network_original(tasks.network.as_mut());
            // Pairing and management are existing resource users, not new supervisors.
            let services = async {
                #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
                self.enter_stage(NodeShutdownStage::Pairing, pairing.is_some());
                if let Some(pairing) = pairing {
                    pairing.clear().await;
                }
                #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
                {
                    self.observer.finish_if_pending(NodeShutdownStage::Pairing);
                    self.enter_stage(NodeShutdownStage::Management, management.is_some());
                }
                let management = async {
                    if let Some(management) = management {
                        management.wait_closed().await;
                    }
                };
                #[cfg(windows)]
                let sessions = async {
                    if let Some(services) = session_services {
                        services.shutdown(shutdown_deadline).await
                    } else {
                        true
                    }
                };
                #[cfg(not(windows))]
                let sessions = async { true };
                let ((), sessions_ok) = tokio::join!(management, sessions);
                #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
                self.observer
                    .finish_if_pending(NodeShutdownStage::Management);
                sessions_ok
            };
            let (network_ok, sessions_ok) = tokio::join!(network, services);
            #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
            {
                self.observer.finish_if_pending(NodeShutdownStage::Network);
                self.enter_stage(NodeShutdownStage::OwnerIpcTask, tasks.ipc.is_some());
            }
            let ipc_ok = self.join_ipc_original(tasks.ipc.as_mut()).await;
            #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
            self.observer
                .finish_if_pending(NodeShutdownStage::OwnerIpcTask);
            drop(tasks);
            if network_ok && ipc_ok && sessions_ok {
                let _ = users_done.send(());
            } else {
                pending::<()>().await;
            }
        };
        let database = async {
            if let Some(storage) = storage {
                #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
                self.enter_stage(NodeShutdownStage::Storage, true);
                storage
                    .close_after_users(async {
                        if users.await.is_err() {
                            pending::<()>().await;
                        }
                    })
                    .await;
                if storage.registry_close_status().is_complete() {
                    #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
                    self.observer.finish_if_pending(NodeShutdownStage::Storage);
                } else {
                    #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
                    self.observer.fail(
                        NodeShutdownStage::Storage,
                        NodeStageOutcome::RegistryUnconfirmed,
                        NodeShutdownFailure::RegistryUnconfirmed,
                    );
                    self.fail(
                        "request registry SQLite connection closure could not be confirmed".into(),
                    );
                }
            } else {
                if users.await.is_err() {
                    pending::<()>().await;
                }
                let host = lock(&self.resources).storage_host.clone();
                if let Some(host) = host {
                    host.owner.request_stop();
                    if !host.observe_actual().await {
                        pending::<()>().await;
                    }
                }
            }
        };
        tokio::join!(transport, database);
    }
}
#[cfg(test)]
#[derive(Default)]
struct DevHooks {
    #[cfg(windows)]
    factory: super::session_services::FactoryTestHooks,
    control_only: bool,
    before_open: Option<Arc<crate::test_support::AsyncBarrier>>,
    sqlite: Option<crate::session_storage::sqlite_lifecycle::TestControl>,
    panic_after_storage: bool,
    ipc: Option<Arc<ipc::IpcDevelopmentHooks>>,
}
#[cfg(test)]
mod development_tests;

#[cfg(test)]
impl NodeProcessOwner {
    pub(crate) fn begin_for_sqlite_test(
        runtime: &Handle,
        config: NodeConfig,
        protector: Arc<dyn SecretProtector>,
        hooks: crate::session_storage::sqlite_lifecycle::TestControl,
    ) -> Self {
        Self::begin_inner(
            runtime,
            config,
            protector,
            DevHooks {
                sqlite: Some(hooks),
                control_only: true,
                ..DevHooks::default()
            },
        )
    }
    pub(crate) fn sqlite_state_for_test(
        &self,
    ) -> Option<std::sync::Weak<crate::session_storage::sqlite_lifecycle::SqliteOpenState>> {
        lock(&self.host.operation.resources)
            .storage_host
            .as_ref()
            .map(|h| Arc::downgrade(&h.owner.state))
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeSlotKey(usize);
#[cfg(test)]
pub struct NodeTestSnapshot {
    pub(crate) original_operation: NodeSlotKey,
    pub(crate) driver_installed: Option<bool>,
    pub(crate) budget: ShutdownResult,
    pub(crate) actual: Actual,
    pub(crate) directory_retained: bool,
    pub(crate) protector: NodeSlotKey,
    pub(crate) sqlite:
        Option<std::sync::Weak<crate::session_storage::sqlite_lifecycle::SqliteOpenState>>,
}
#[cfg(test)]
pub struct NodeTestProbe(std::sync::Weak<Host>);
#[cfg(test)]
impl NodeTestProbe {
    pub(crate) async fn observe_actual(&self) -> Result<(), AppError> {
        let host = self
            .0
            .upgrade()
            .ok_or_else(|| AppError::Shutdown("original Node host unavailable".into()))?;
        host.join_actual().await
    }
    pub(crate) fn snapshot(&self) -> Result<Option<NodeTestSnapshot>, NodeProbeUnavailable> {
        let Some(host) = self.0.upgrade() else {
            return Ok(None);
        };
        let roots = host
            .operation
            .resources
            .lock()
            .map_err(|_| NodeProbeUnavailable)?;
        let snapshot = NodeTestSnapshot {
            original_operation: NodeSlotKey(Arc::as_ptr(&host.operation) as usize),
            driver_installed: host.task.try_lock().ok().map(|slot| slot.is_some()),
            budget: host.operation.budget.borrow().clone(),
            actual: *host.operation.actual.borrow(),
            directory_retained: roots.directory.is_some(),
            protector: NodeSlotKey(Arc::as_ptr(&host.operation.protector).cast::<()>() as usize),
            sqlite: roots
                .storage_host
                .as_ref()
                .map(|h| Arc::downgrade(&h.owner.state)),
        };
        drop(roots);
        Ok(Some(snapshot))
    }
}
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub struct NodeProbeUnavailable;
#[cfg(test)]
impl NodeProcessOwner {
    pub(crate) fn probe_for_sqlite_test(&self) -> NodeTestProbe {
        NodeTestProbe(Arc::downgrade(&self.host))
    }
}
