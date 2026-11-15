//! Ownership of the original scoped SQLite open, SQL close and thread joins.
use crate::{error::StorageError, storage::Storage};
use futures_util::{FutureExt, StreamExt, stream::FuturesUnordered};
use sqlx::{
    SqlitePool,
    sqlite::{
        SqlitePoolOptions, SqliteWorkerRegistration, SqliteWorkerRegistry, WorkerExitPath,
        WorkerId, WorkerJoinReport, WorkerSpawn,
    },
};
use std::{
    future::pending,
    panic::AssertUnwindSafe,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::{
    runtime::Handle,
    sync::{Mutex as AsyncMutex, Notify, oneshot},
    task::JoinHandle,
};
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub struct StorageOpenRoots {
    pub(crate) path: PathBuf,
    retained: Vec<Arc<dyn Send + Sync>>,
}
impl std::fmt::Debug for StorageOpenRoots {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StorageOpenRoots")
            .field("retained_owner_count", &self.retained.len())
            .finish_non_exhaustive()
    }
}
impl StorageOpenRoots {
    pub fn retained<T: Send + Sync + 'static>(path: &Path, original_owner: Arc<T>) -> Self {
        Self {
            path: path.to_owned(),
            retained: vec![original_owner],
        }
    }
    pub(crate) fn path(path: &Path) -> Self {
        Self {
            path: path.to_owned(),
            retained: Vec::new(),
        }
    }
    #[must_use]
    pub fn directory(directory: Arc<crate::data_dir::DataDirectory>) -> Self {
        Self {
            path: directory.database_path(),
            retained: vec![directory],
        }
    }
    pub(crate) fn node(
        directory: Arc<crate::data_dir::DataDirectory>,
        protector: Arc<dyn crate::platform::SecretProtector>,
    ) -> Self {
        let path = directory.database_path();
        // The tuple is a sized root; the original protector and directory remain owned.
        Self {
            path,
            retained: vec![Arc::new((directory, protector))],
        }
    }
}
#[derive(Clone, Copy)]
enum OpenMode {
    Storage,
    Preflight,
    WorkerScope,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Actual {
    Opening,
    Running,
    Closing,
    Joined,
    Unconfirmed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppJoinState {
    Installed,
    Queued,
    Started,
    Joined,
    Unconfirmed,
}
pub struct SqliteWorkerJoinSlot {
    pub(crate) worker: WorkerId,
    task:
        AsyncMutex<Option<JoinHandle<Result<WorkerJoinReport, sqlx::sqlite::WorkerRegistryError>>>>,
    state: Mutex<AppJoinState>,
    result: Mutex<Option<Result<WorkerJoinReport, sqlx::sqlite::WorkerRegistryError>>>,
    returned: Notify,
    has_returned: AtomicBool,
    task_join_error: AtomicBool,
    changed: Arc<Notify>,
}
impl SqliteWorkerJoinSlot {
    async fn observe(&self, owner: &SqliteOpenState) {
        loop {
            let n = self.returned.notified();
            tokio::pin!(n);
            n.as_mut().enable();
            if self.has_returned.load(Ordering::Acquire) {
                break;
            }
            n.await;
        }
        #[cfg(test)]
        owner
            .hooks
            .visit(TestPoint::AppJoinResultBeforeHandleObservation)
            .await;
        let mut slot = self.task.lock().await;
        if let Some(task) = slot.as_mut() {
            let result = task.await;
            self.task_join_error
                .store(result.is_err(), Ordering::Release);
            #[cfg(test)]
            let state_unavailable = owner.hooks.0.unavailable.swap(false, Ordering::AcqRel);
            #[cfg(not(test))]
            let state_unavailable = false;
            let confirmed = !state_unavailable
                && owner.evidence_available()
                && matches!(result, Ok(Ok(WorkerJoinReport::Ok)));
            *lock(&self.state) = if confirmed {
                AppJoinState::Joined
            } else {
                AppJoinState::Unconfirmed
            };
            *lock(&self.result) =
                Some(result.unwrap_or(Err(sqlx::sqlite::WorkerRegistryError::StateUnavailable)));
            slot.take();
        }
        drop(slot);
        self.changed.notify_waiters();
    }
}
pub struct SqliteOpenState {
    pub(crate) registry: SqliteWorkerRegistry,
    runtime: Handle,
    roots: Mutex<Option<StorageOpenRoots>>,
    admission: Mutex<()>,
    parent_installed: AtomicBool,
    started: AtomicBool,
    gate_open: AtomicBool,
    stop: AtomicBool,
    sql_finished: AtomicBool,
    users_done: AtomicBool,
    retention: Mutex<Option<Arc<Self>>>,
    final_fence: AtomicBool,
    driver: AsyncMutex<Option<JoinHandle<()>>>,
    storage: Mutex<Option<Arc<Storage>>>,
    pool: Mutex<Option<SqlitePool>>,
    error: Mutex<Option<Arc<StorageError>>>,
    ready: AtomicBool,
    actual: Mutex<Actual>,
    joins: Mutex<Vec<Arc<SqliteWorkerJoinSlot>>>,
    changed: Arc<Notify>,
    #[cfg(test)]
    pub(crate) hooks: TestControl,
}
#[derive(Clone)]
pub struct StorageOpenOwner {
    pub(crate) state: Arc<SqliteOpenState>,
}
pub struct StorageOpenTicket {
    owner: StorageOpenOwner,
    delivered: AtomicBool,
    waiters: std::sync::atomic::AtomicUsize,
}
pub struct StorageLifecycleHost {
    pub(crate) owner: StorageOpenOwner,
    _runtime: Handle,
}
impl StorageLifecycleHost {
    #[must_use]
    pub fn install(owner: StorageOpenOwner) -> Arc<Self> {
        owner.state.parent_installed.store(true, Ordering::Release);
        Arc::new(Self {
            _runtime: owner.state.runtime.clone(),
            owner,
        })
    }
    pub async fn observe_actual(&self) -> bool {
        self.owner.observe_actual().await
    }
}
impl Drop for StorageLifecycleHost {
    fn drop(&mut self) {
        self.owner.request_stop();
    }
}
impl StorageOpenOwner {
    /// Options for raw fixture work in this already installed original scope.
    /// # Errors
    /// Rejects missing parent/driver, stop, final seal, or unavailable original roots.
    pub fn connection_options(&self) -> Result<sqlx::sqlite::SqliteConnectOptions, StorageError> {
        if !self.state.parent_installed.load(Ordering::Acquire)
            || !self.state.started.load(Ordering::Acquire)
            || self
                .state
                .driver
                .try_lock()
                .map_or(true, |slot| slot.is_none())
            || self.state.stop.load(Ordering::Acquire)
            || self.state.final_fence.load(Ordering::Acquire)
        {
            return Err(StorageError::Lifecycle(
                "original SQLite scope is not available for raw work",
            ));
        }
        let roots = self
            .state
            .roots
            .lock()
            .map_err(|_| StorageError::Lifecycle("original SQLite roots unavailable"))?;
        let path = roots
            .as_ref()
            .ok_or(StorageError::Lifecycle("original SQLite roots unavailable"))?
            .path
            .clone();
        drop(roots);
        Ok(sqlx::sqlite::SqliteConnectOptions::new()
            .filename(path)
            .worker_registry(self.state.registry.clone()))
    }
    /// Starts normal storage with the caller's original pool options.
    /// # Errors
    /// Rejects an uninstalled, stopped or already started original owner.
    pub fn start_with_pool_options(
        &self,
        options: SqlitePoolOptions,
    ) -> Result<StorageOpenTicket, StorageError> {
        self.start_with(
            options,
            false,
            #[cfg(test)]
            None,
            #[cfg(test)]
            None,
        )
    }
    /// Prepares original ownership without starting any runtime work.
    /// # Errors
    /// This preparation currently always succeeds; errors use the same lifecycle API.
    pub fn prepare(runtime: &Handle, roots: StorageOpenRoots) -> Result<Self, StorageError> {
        Ok(Self {
            state: Arc::new(SqliteOpenState {
                registry: SqliteWorkerRegistry::new(),
                runtime: runtime.clone(),
                roots: Mutex::new(Some(roots)),
                admission: Mutex::new(()),
                parent_installed: AtomicBool::new(false),
                started: AtomicBool::new(false),
                gate_open: AtomicBool::new(false),
                stop: AtomicBool::new(false),
                sql_finished: AtomicBool::new(false),
                users_done: AtomicBool::new(true),
                retention: Mutex::new(None),
                final_fence: AtomicBool::new(false),
                driver: AsyncMutex::new(None),
                storage: Mutex::new(None),
                pool: Mutex::new(None),
                error: Mutex::new(None),
                ready: AtomicBool::new(false),
                actual: Mutex::new(Actual::Opening),
                joins: Mutex::new(Vec::new()),
                changed: Arc::new(Notify::new()),
                #[cfg(test)]
                hooks: TestControl::default(),
            }),
        })
    }
    /// Starts the original storage driver after its parent has been installed.
    /// # Errors
    /// Rejects missing parent, prior start, stop, or an unavailable original driver slot.
    pub fn start(&self) -> Result<StorageOpenTicket, StorageError> {
        self.start_with(
            SqlitePoolOptions::new().max_connections(1),
            false,
            #[cfg(test)]
            None,
            #[cfg(test)]
            None,
        )
    }
    /// Starts read-only validation in the original owned scope.
    /// # Errors
    /// Rejects missing parent, prior start, stop, or an unavailable original driver slot.
    pub fn start_preflight(&self) -> Result<StorageOpenTicket, StorageError> {
        self.start_with(
            SqlitePoolOptions::new().max_connections(1),
            true,
            #[cfg(test)]
            None,
            #[cfg(test)]
            None,
        )
    }
    pub(crate) fn start_with(
        &self,
        pool_options: SqlitePoolOptions,
        preflight_only: bool,
        #[cfg(test)] fault: Option<super::schema::MigrationFault>,
        #[cfg(test)] namespace: Option<super::schema9_namespace_test_hook::NamespaceScenario>,
    ) -> Result<StorageOpenTicket, StorageError> {
        self.start_mode(
            pool_options,
            if preflight_only {
                OpenMode::Preflight
            } else {
                OpenMode::Storage
            },
            #[cfg(test)]
            fault,
            #[cfg(test)]
            namespace,
        )
    }
    /// Starts an owned raw-worker scope without opening SQLite.
    /// # Errors
    /// Rejects missing parent, prior start, stop, or an unavailable original driver slot.
    pub fn start_worker_scope(&self) -> Result<StorageOpenTicket, StorageError> {
        self.start_mode(
            SqlitePoolOptions::new(),
            OpenMode::WorkerScope,
            #[cfg(test)]
            None,
            #[cfg(test)]
            None,
        )
    }
    fn start_mode(
        &self,
        pool_options: SqlitePoolOptions,
        mode: OpenMode,
        #[cfg(test)] fault: Option<super::schema::MigrationFault>,
        #[cfg(test)] namespace: Option<super::schema9_namespace_test_hook::NamespaceScenario>,
    ) -> Result<StorageOpenTicket, StorageError> {
        #[cfg(test)]
        self.state
            .hooks
            .visit_blocking(TestPoint::StartBeforeAdmission);
        let admission = self
            .state
            .admission
            .lock()
            .map_err(|_| StorageError::Lifecycle("original storage admission unavailable"))?;
        if self.state.stop.load(Ordering::Acquire)
            || !self.state.parent_installed.load(Ordering::Acquire)
            || self.state.started.swap(true, Ordering::AcqRel)
        {
            return Err(StorageError::Lifecycle(
                "storage parent missing or already started",
            ));
        }
        *lock(&self.state.retention) = Some(self.state.clone());
        drop(admission);
        #[cfg(test)]
        self.state
            .hooks
            .visit_blocking(TestPoint::StartAdmittedBeforeDriverInstall);
        let mut driver_slot = self
            .state
            .driver
            .try_lock()
            .map_err(|_| StorageError::Lifecycle("original storage driver slot unavailable"))?;
        let (open, gate) = oneshot::channel();
        let state = self.state.clone();
        let task = self.state.runtime.spawn(async move {
            if gate.await.is_err() {
                *lock(&state.actual) = Actual::Unconfirmed;
                state.changed.notify_waiters();
                pending::<()>().await;
            }
            #[cfg(test)]
            state
                .hooks
                .visit(TestPoint::ParentInstalledBeforeDriverGate)
                .await;
            state.gate_open.store(true, Ordering::Release);
            let run = async {
                let result = AssertUnwindSafe(state.clone().drive(
                    pool_options,
                    mode,
                    #[cfg(test)]
                    fault,
                    #[cfg(test)]
                    namespace,
                ))
                .catch_unwind()
                .await;
                if result.is_err() {
                    *lock(&state.actual) = Actual::Unconfirmed;
                    *lock(&state.error) = Some(Arc::new(StorageError::Lifecycle(
                        "original storage driver unwound",
                    )));
                    state.ready.store(true, Ordering::Release);
                    state.changed.notify_waiters();
                    pending::<()>().await;
                }
            };
            let bridge = state.join_bridge();
            tokio::join!(run, bridge);
        });
        // No task can touch SQLite before its original handle is installed.
        *driver_slot = Some(task);
        drop(driver_slot);
        let _ = open.send(());
        Ok(StorageOpenTicket {
            owner: self.clone(),
            delivered: AtomicBool::new(false),
            waiters: std::sync::atomic::AtomicUsize::new(0),
        })
    }
    pub fn request_stop(&self) {
        let Ok(admission) = self.state.admission.lock() else {
            self.state.stop.store(true, Ordering::Release);
            self.state.unconfirmed();
            return;
        };
        self.state.stop.store(true, Ordering::Release);
        let unstarted = !self.state.started.load(Ordering::Acquire);
        drop(admission);
        self.state.changed.notify_waiters();
        if unstarted {
            let _ = self.state.registry.seal();
            if self.state.evidence_available() && self.state.registry.finalize().is_ok() {
                *lock(&self.state.actual) = Actual::Joined;
                self.state.release_roots_after_join();
            }
        }
    }
    pub async fn observe_actual(&self) -> bool {
        loop {
            let n = self.state.changed.notified();
            tokio::pin!(n);
            n.as_mut().enable();
            let actual = self.state.actual();
            match actual {
                Actual::Joined => break,
                Actual::Unconfirmed => return false,
                _ => n.await,
            }
        }
        let mut task = self.state.driver.lock().await;
        if let Some(handle) = task.as_mut() {
            if handle.await.is_err() {
                *lock(&self.state.actual) = Actual::Unconfirmed;
                return false;
            }
            task.take();
        }
        drop(task);
        self.state.release_roots_after_join()
    }
}
struct ReadyGuard<'a>(&'a StorageOpenTicket);
impl Drop for ReadyGuard<'_> {
    fn drop(&mut self) {
        if self.0.waiters.fetch_sub(1, Ordering::AcqRel) == 1
            && !self.0.delivered.load(Ordering::Acquire)
        {
            self.0.owner.request_stop();
        }
    }
}
impl Drop for StorageOpenTicket {
    fn drop(&mut self) {
        if !self.delivered.load(Ordering::Acquire) {
            self.owner.request_stop();
        }
    }
}
impl StorageOpenTicket {
    /// Observes the original storage result without taking its driver.
    /// # Errors
    /// Returns the original open error, or a lifecycle error when no result is available.
    pub async fn ready(&self) -> Result<Arc<Storage>, Arc<StorageError>> {
        self.waiters.fetch_add(1, Ordering::AcqRel);
        let _guard = ReadyGuard(self);
        loop {
            let n = self.owner.state.changed.notified();
            tokio::pin!(n);
            n.as_mut().enable();
            if self.owner.state.ready.load(Ordering::Acquire) {
                self.delivered.store(true, Ordering::Release);
                let error = lock(&self.owner.state.error).clone();
                if let Some(e) = error {
                    return Err(e);
                }
                let storage = lock(&self.owner.state.storage).clone();
                if let Some(s) = storage {
                    return Ok(s);
                }
                return Err(Arc::new(StorageError::Lifecycle(
                    "storage ready unavailable",
                )));
            }
            n.await;
        }
    }
    /// Observes completion of the original preflight or raw-scope initialization.
    /// # Errors
    /// Returns the original preflight or lifecycle error.
    pub async fn preflight_ready(&self) -> Result<(), Arc<StorageError>> {
        self.waiters.fetch_add(1, Ordering::AcqRel);
        let _guard = ReadyGuard(self);
        loop {
            let n = self.owner.state.changed.notified();
            tokio::pin!(n);
            n.as_mut().enable();
            if self.owner.state.ready.load(Ordering::Acquire) {
                self.delivered.store(true, Ordering::Release);
                let error = lock(&self.owner.state.error).clone();
                if let Some(e) = error {
                    return Err(e);
                }
                return Ok(());
            }
            n.await;
        }
    }
}
impl SqliteOpenState {
    pub(crate) fn store_pool(&self, pool: SqlitePool) {
        *lock(&self.pool) = Some(pool);
    }
    pub(crate) const fn registry(&self) -> &SqliteWorkerRegistry {
        &self.registry
    }
    pub(crate) fn worker_status(&self, registration: &SqliteWorkerRegistration) -> Option<bool> {
        if !self.evidence_available() {
            self.unconfirmed();
            return Some(false);
        }
        lock(&self.joins)
            .iter()
            .find(|s| s.worker == registration.id())
            .and_then(|s| {
                let state = *lock(&s.state);
                match state {
                    AppJoinState::Joined => Some(true),
                    AppJoinState::Unconfirmed => Some(false),
                    _ => None,
                }
            })
    }
    pub(crate) async fn wait_worker(&self, registration: &SqliteWorkerRegistration) -> bool {
        let id = registration.id();
        loop {
            let n = self.changed.notified();
            tokio::pin!(n);
            n.as_mut().enable();
            if !self.evidence_available() {
                self.unconfirmed();
                return false;
            }
            let result = lock(&self.joins)
                .iter()
                .find(|s| s.worker == id)
                .map(|s| *lock(&s.state));
            match result {
                Some(AppJoinState::Joined) => return true,
                Some(AppJoinState::Unconfirmed) => return false,
                _ => n.await,
            }
        }
    }
    async fn install_worker_join(self: &Arc<Self>, id: WorkerId) -> Arc<SqliteWorkerJoinSlot> {
        let slot = Arc::new(SqliteWorkerJoinSlot {
            worker: id.clone(),
            task: AsyncMutex::new(None),
            state: Mutex::new(AppJoinState::Installed),
            result: Mutex::new(None),
            returned: Notify::new(),
            has_returned: AtomicBool::new(false),
            task_join_error: AtomicBool::new(false),
            changed: self.changed.clone(),
        });
        lock(&self.joins).push(slot.clone());
        let (sender, gate) = std::sync::mpsc::sync_channel(1);
        let original = self.registry.clone();
        let blocking = slot.clone();
        #[cfg(test)]
        let control = self.hooks.clone();
        let task = self.runtime.spawn_blocking(move || {
            struct Returned(Arc<SqliteWorkerJoinSlot>);
            impl Drop for Returned {
                fn drop(&mut self) {
                    self.0.has_returned.store(true, Ordering::Release);
                    self.0.returned.notify_waiters();
                }
            }
            let _returned = Returned(blocking.clone());
            if gate.recv().is_err() {
                blocking.has_returned.store(true, Ordering::Release);
                blocking.returned.notify_waiters();
                return Err(sqlx::sqlite::WorkerRegistryError::StateUnavailable);
            }
            *lock(&blocking.state) = AppJoinState::Started;
            #[cfg(test)]
            control.visit_blocking(TestPoint::BlockingBeforeVendorClaim);
            #[cfg(test)]
            assert!(
                !control.0.panic_before_claim.swap(false, Ordering::AcqRel),
                "private app blocking fault before vendor claim"
            );
            let result = original.join_worker_blocking(id);
            blocking.has_returned.store(true, Ordering::Release);
            blocking.returned.notify_waiters();
            result
        });
        *slot.task.lock().await = Some(task);
        *lock(&slot.state) = AppJoinState::Queued;
        #[cfg(test)]
        self.hooks
            .visit(TestPoint::AppJoinHandleInstalledBeforeGate)
            .await;
        let _ = sender.send(());
        slot
    }
    async fn finish_joins(&self, snapshot: &sqlx::sqlite::RegistrySnapshot) -> bool {
        let all_app = lock(&self.joins)
            .iter()
            .all(|s| *lock(&s.state) == AppJoinState::Joined);
        let missing_graceful_report = snapshot
            .workers
            .iter()
            .any(|worker| worker.close.graceful_started && !worker.close.graceful_returned_ok);
        if missing_graceful_report {
            self.unconfirmed();
            pending::<()>().await;
        }
        let required_close = lock(&self.storage)
            .as_ref()
            .map_or(super::ConnectionCloseStatus::Complete, |storage| {
                storage.required_close_status_in_scope()
            });
        if required_close == super::ConnectionCloseStatus::Unconfirmed {
            self.unconfirmed();
            pending::<()>().await;
        }
        if snapshot.all_threads_terminated()
            && required_close == super::ConnectionCloseStatus::Complete
            && all_app
            && self.evidence_available()
            && self.registry.finalize().is_ok()
        {
            *lock(&self.actual) = Actual::Joined;
            self.changed.notify_waiters();
            return true;
        }
        if snapshot.unknown
            || lock(&self.joins)
                .iter()
                .any(|s| *lock(&s.state) == AppJoinState::Unconfirmed)
        {
            *lock(&self.actual) = Actual::Unconfirmed;
            self.changed.notify_waiters();
            pending::<()>().await;
        }
        false
    }
    async fn join_bridge(self: &Arc<Self>) {
        let mut observations = FuturesUnordered::new();
        loop {
            let n = self.changed.notified();
            tokio::pin!(n);
            n.as_mut().enable();
            if !self.evidence_available() {
                self.unconfirmed();
                pending::<()>().await;
            }
            let Ok(subscription) = self.registry.subscribe() else {
                *lock(&self.actual) = Actual::Unconfirmed;
                self.changed.notify_waiters();
                pending::<()>().await;
                return;
            };
            let Ok(snapshot) = self.registry.snapshot() else {
                *lock(&self.actual) = Actual::Unconfirmed;
                self.changed.notify_waiters();
                pending::<()>().await;
                return;
            };
            for worker in &snapshot.workers {
                if worker.spawn != WorkerSpawn::Installed
                    || lock(&self.joins).iter().any(|s| s.worker == worker.id)
                {
                    continue;
                }
                if !worker.shutdown.requested
                    && !worker.close.graceful_started
                    && !worker.close.hard_drop_requested
                    && worker.exit_path == WorkerExitPath::Unknown
                    && !worker.thread_is_finished
                {
                    continue;
                }
                let slot = self.install_worker_join(worker.id.clone()).await;
                let owner = self.clone();
                observations.push(async move {
                    slot.observe(&owner).await;
                });
            }
            if self.sql_finished.load(Ordering::Acquire)
                && observations.is_empty()
                && self.finish_joins(&snapshot).await
            {
                return;
            }
            if self
                .registry
                .snapshot()
                .is_ok_and(|s| s.generation != subscription.generation)
            {
                continue;
            }
            tokio::select! { _=subscription.changed()=>{},()=n=>{},_=observations.next(),if !observations.is_empty()=>{} }
        }
    }
    async fn drive(
        self: Arc<Self>,
        pool_options: SqlitePoolOptions,
        mode: OpenMode,
        #[cfg(test)] fault: Option<super::schema::MigrationFault>,
        #[cfg(test)] namespace: Option<super::schema9_namespace_test_hook::NamespaceScenario>,
    ) {
        let path = lock(&self.roots).as_ref().map(|r| r.path.clone());
        let Some(path) = path else {
            *lock(&self.error) = Some(Arc::new(StorageError::Lifecycle(
                "storage roots unavailable",
            )));
            self.finish_sql();
            return;
        };
        let result = if self.stop.load(Ordering::Acquire) {
            Err(StorageError::Lifecycle("storage stopped before open"))
        } else if matches!(mode, OpenMode::WorkerScope) {
            Ok(None)
        } else if matches!(mode, OpenMode::Preflight) {
            super::schema::preflight_in_scope(&path, &self)
                .await
                .map(|()| None)
        } else {
            Storage::open_in_scope(
                &path,
                pool_options,
                &self,
                #[cfg(test)]
                fault,
                #[cfg(test)]
                namespace,
            )
            .await
            .map(|s| Some(Arc::new(s)))
        };
        match result {
            Ok(storage) => {
                *lock(&self.storage) = storage;
                *lock(&self.actual) = Actual::Running;
                #[cfg(test)]
                self.hooks
                    .visit(TestPoint::InitializedBeforeReadyDelivery)
                    .await;
            }
            Err(e) => {
                *lock(&self.error) = Some(Arc::new(e));
                self.stop.store(true, Ordering::Release);
            }
        }
        self.ready.store(true, Ordering::Release);
        self.changed.notify_waiters();
        if !matches!(mode, OpenMode::Preflight) {
            loop {
                let n = self.changed.notified();
                tokio::pin!(n);
                n.as_mut().enable();
                if self.stop.load(Ordering::Acquire) {
                    break;
                }
                n.await;
            }
        }
        *lock(&self.actual) = Actual::Closing;
        let storage = lock(&self.storage).clone();
        if let Some(storage) = storage {
            storage.close_in_scope().await;
        } else {
            self.final_sql_fence().await;
            let pool = lock(&self.pool).clone();
            if let Some(pool) = pool {
                pool.close().await;
            }
        }
        self.finish_sql();
    }
    pub(crate) fn require_users(&self) {
        self.users_done.store(false, Ordering::Release);
    }
    pub(crate) fn users_finished(&self) {
        self.users_done.store(true, Ordering::Release);
        self.changed.notify_waiters();
    }
    pub(crate) async fn wait_users(&self) {
        loop {
            let n = self.changed.notified();
            tokio::pin!(n);
            n.as_mut().enable();
            if self.users_done.load(Ordering::Acquire) {
                return;
            }
            n.await;
        }
    }
    pub(crate) fn evidence_available(&self) -> bool {
        !self.admission.is_poisoned()
            && !self.roots.is_poisoned()
            && !self.storage.is_poisoned()
            && !self.pool.is_poisoned()
            && !self.error.is_poisoned()
            && !self.actual.is_poisoned()
            && !self.joins.is_poisoned()
            && !self.retention.is_poisoned()
            && lock(&self.joins)
                .iter()
                .all(|s| !s.state.is_poisoned() && !s.result.is_poisoned())
    }
    fn unconfirmed(&self) {
        if let Ok(mut actual) = self.actual.lock() {
            *actual = Actual::Unconfirmed;
        }
        self.changed.notify_waiters();
    }
    fn release_roots_after_join(&self) -> bool {
        if !self.evidence_available() {
            self.unconfirmed();
            return false;
        }
        let (Ok(mut roots), Ok(mut pool), Ok(mut storage), Ok(mut retention)) = (
            self.roots.lock(),
            self.pool.lock(),
            self.storage.lock(),
            self.retention.lock(),
        ) else {
            self.unconfirmed();
            return false;
        };
        roots.take();
        pool.take();
        storage.take();
        retention.take();
        true
    }
    pub(crate) fn actual(&self) -> Actual {
        if !self.evidence_available() {
            self.unconfirmed();
            return Actual::Unconfirmed;
        }
        *lock(&self.actual)
    }

    pub(crate) fn final_sql_fence(&self) -> impl std::future::Future<Output = ()> + Send {
        #[cfg(test)]
        let before_fence = self.hooks.visit(TestPoint::BeforeFinalSqlFence);
        #[cfg(not(test))]
        let before_fence = std::future::ready(());
        before_fence.map(|()| {
            self.final_fence.store(true, Ordering::Release);
            let _ = self.registry.seal();
        })
    }
    fn finish_sql(&self) {
        self.sql_finished.store(true, Ordering::Release);
        self.changed.notify_waiters();
    }
    pub(crate) async fn stop_and_observe(self: &Arc<Self>) -> bool {
        StorageOpenOwner {
            state: self.clone(),
        }
        .request_stop();
        StorageOpenOwner {
            state: self.clone(),
        }
        .observe_actual()
        .await
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TestPoint {
    StartBeforeAdmission,
    StartAdmittedBeforeDriverInstall,
    ParentInstalledBeforeDriverGate,
    PreflightConnectedBeforeValidation,
    PreflightJoinedBeforePoolConnect,
    PoolConnectedBeforeInitialize,
    InitializedBeforeReadyDelivery,
    AppJoinHandleInstalledBeforeGate,
    BlockingBeforeVendorClaim,
    AppJoinResultBeforeHandleObservation,
    BeforeFinalSqlFence,
}
#[cfg(test)]
#[derive(Default)]
struct TestControls {
    pauses: Mutex<std::collections::HashMap<TestPoint, Arc<TestPause>>>,
    unavailable: AtomicBool,
    panic_before_claim: AtomicBool,
}
#[cfg(test)]
#[derive(Default, Clone)]
pub struct TestControl(Arc<TestControls>);
#[cfg(test)]
pub struct TestPause {
    arrived: AtomicBool,
    released: Mutex<bool>,
    cond: std::sync::Condvar,
    changed: Notify,
}
#[cfg(test)]
impl TestPause {
    pub(crate) async fn arrived(&self) {
        loop {
            let n = self.changed.notified();
            tokio::pin!(n);
            n.as_mut().enable();
            if self.arrived.load(Ordering::Acquire) {
                return;
            }
            n.await;
        }
    }
    pub(crate) fn release(&self) {
        *lock(&self.released) = true;
        self.cond.notify_all();
        self.changed.notify_waiters();
    }
}
#[cfg(test)]
impl TestControl {
    pub(crate) fn state_unavailable_once(&self) {
        self.0.unavailable.store(true, Ordering::Release);
    }
    pub(crate) fn panic_next_blocking_before_claim(&self) {
        self.0.panic_before_claim.store(true, Ordering::Release);
    }
    pub(crate) fn pause_next(&self, p: TestPoint) -> Arc<TestPause> {
        let b = Arc::new(TestPause {
            arrived: AtomicBool::new(false),
            released: Mutex::new(false),
            cond: std::sync::Condvar::new(),
            changed: Notify::new(),
        });
        lock(&self.0.pauses).insert(p, b.clone());
        b
    }
    pub(crate) async fn visit(&self, p: TestPoint) {
        let b = lock(&self.0.pauses).remove(&p);
        if let Some(b) = b {
            b.arrived.store(true, Ordering::Release);
            b.changed.notify_waiters();
            loop {
                let n = b.changed.notified();
                tokio::pin!(n);
                n.as_mut().enable();
                if *lock(&b.released) {
                    break;
                }
                n.await;
            }
        }
    }
    fn visit_blocking(&self, p: TestPoint) {
        let b = lock(&self.0.pauses).remove(&p);
        if let Some(b) = b {
            b.arrived.store(true, Ordering::Release);
            b.changed.notify_waiters();
            let mut released = lock(&b.released);
            while !*released {
                released = b
                    .cond
                    .wait(released)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            drop(released);
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpaqueSlotKey(usize);
#[cfg(test)]
pub struct JoinTestSnapshot {
    pub(crate) worker: WorkerId,
    pub(crate) operation: OpaqueSlotKey,
    pub(crate) state: AppJoinState,
    pub(crate) handle_installed: Option<bool>,
    pub(crate) task_join_error: bool,
}
#[cfg(test)]
pub struct DriverTestSnapshot {
    pub(crate) parent_installed: bool,
    pub(crate) handle_installed: Option<bool>,
    pub(crate) gate_open: bool,
    pub(crate) ready: bool,
}
#[cfg(test)]
pub struct SqlTestSnapshot {
    pub(crate) stop: bool,
    pub(crate) final_fence: bool,
    pub(crate) users_done: bool,
}
#[cfg(test)]
pub struct TestSnapshot {
    pub(crate) open_operation: OpaqueSlotKey,
    pub(crate) driver: DriverTestSnapshot,
    pub(crate) scope: Result<sqlx::sqlite::RegistrySnapshot, sqlx::sqlite::WorkerRegistryError>,
    pub(crate) pool: Option<OpaqueSlotKey>,
    pub(crate) sql: SqlTestSnapshot,
    pub(crate) roots_retained: bool,
    pub(crate) retained_root_count: usize,
    pub(crate) actual: Actual,
    pub(crate) joins: Vec<JoinTestSnapshot>,
}
#[cfg(test)]
impl SqliteOpenState {
    pub(crate) fn test_snapshot(&self) -> TestSnapshot {
        let (roots_retained, retained_root_count) = {
            let roots = lock(&self.roots);
            (
                roots.is_some(),
                roots.as_ref().map_or(0, |r| r.retained.len()),
            )
        };
        TestSnapshot {
            open_operation: OpaqueSlotKey(std::ptr::from_ref(self) as usize),
            driver: DriverTestSnapshot {
                parent_installed: self.parent_installed.load(Ordering::Acquire),
                handle_installed: self.driver.try_lock().ok().map(|s| s.is_some()),
                gate_open: self.gate_open.load(Ordering::Acquire),
                ready: self.ready.load(Ordering::Acquire),
            },
            scope: if self.evidence_available() {
                self.registry.snapshot()
            } else {
                Err(sqlx::sqlite::WorkerRegistryError::StateUnavailable)
            },
            pool: lock(&self.pool)
                .as_ref()
                .map(|p| OpaqueSlotKey(std::ptr::from_ref(p) as usize)),
            sql: SqlTestSnapshot {
                stop: self.stop.load(Ordering::Acquire),
                final_fence: self.final_fence.load(Ordering::Acquire),
                users_done: self.users_done.load(Ordering::Acquire),
            },
            roots_retained,
            retained_root_count,
            actual: self.actual(),
            joins: lock(&self.joins)
                .iter()
                .map(|s| JoinTestSnapshot {
                    worker: s.worker.clone(),
                    operation: OpaqueSlotKey(Arc::as_ptr(s) as usize),
                    state: *lock(&s.state),
                    handle_installed: s.task.try_lock().ok().map(|h| h.is_some()),
                    task_join_error: s.task_join_error.load(Ordering::Acquire),
                })
                .collect(),
        }
    }
}
#[cfg(test)]
impl StorageOpenOwner {
    pub(crate) fn prepare_for_test(
        runtime: &Handle,
        roots: StorageOpenRoots,
        hooks: TestControl,
    ) -> Result<Self, StorageError> {
        let mut owner = Self::prepare(runtime, roots)?;
        let state = Arc::get_mut(&mut owner.state).ok_or(StorageError::Lifecycle(
            "fresh storage state shared before prepare",
        ))?;
        state.hooks = hooks;
        Ok(owner)
    }
}

#[cfg(test)]
mod development_tests;
