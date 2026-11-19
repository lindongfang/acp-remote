pub use crate::session_storage::sqlite_lifecycle::{
    StorageLifecycleHost, StorageOpenOwner, StorageOpenRoots, StorageOpenTicket,
};
use std::{path::Path, sync::Arc, time::Duration};

use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};

use crate::error::StorageError;
mod native_runtime;
mod runtime_coordinator;

pub const SCHEMA_VERSION: i64 = 15;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeMeta {
    pub schema_version: i64,
    pub node_id: String,
    pub public_key: Vec<u8>,
    pub key_file_ref: String,
}

#[cfg(windows)]
mod catalog_upload;
#[cfg(windows)]
mod document_transfer;
#[cfg(windows)]
mod prompt_preparation;
pub struct Storage {
    inner: Arc<StorageInner>,
    lifecycle: std::sync::Weak<crate::session_storage::sqlite_lifecycle::SqliteOpenState>,
    host: Option<Arc<crate::session_storage::sqlite_lifecycle::StorageLifecycleHost>>,
}

struct StorageInner {
    pool: SqlitePool,
    #[cfg(test)]
    product_store: crate::session_storage::product_commands::ProductStore,
    request_registry: crate::session_storage::request_registry::SqliteRequestRegistry,
    coordinator: runtime_coordinator::StorageRuntimeCoordinator,
    #[cfg(test)]
    queue_randoms: Arc<crate::application::request_registry::random_hooks::Controller>,
    #[cfg(test)]
    close_barrier: std::sync::Mutex<Option<Arc<crate::test_support::AsyncBarrier>>>,
}

impl Storage {
    fn projection(
        &self,
        host: Arc<crate::session_storage::sqlite_lifecycle::StorageLifecycleHost>,
    ) -> Self {
        Self {
            inner: self.inner.clone(),
            lifecycle: self.lifecycle.clone(),
            host: Some(host),
        }
    }

    #[cfg(windows)]
    pub fn native_configuration_provider<
        C: crate::session_storage::native_configuration::TrustedNativeConfiguration,
    >(
        &self,
        configuration: Arc<C>,
    ) -> crate::session_storage::native_configuration::SqliteNativeConfigurationProvider<C> {
        crate::session_storage::native_configuration::SqliteNativeConfigurationProvider::new(
            self.request_registry(),
            configuration,
        )
    }
    #[must_use]
    pub fn request_registry(
        &self,
    ) -> crate::session_storage::request_registry::SqliteRequestRegistry {
        self.inner.request_registry.clone()
    }
    pub(crate) fn native_release_operations(
        &self,
    ) -> crate::session_storage::native_release::SqliteNativeReleaseOperations {
        crate::session_storage::native_release::SqliteNativeReleaseOperations::new(
            self.request_registry(),
        )
    }
    #[cfg(test)]
    pub(crate) fn queue_random_faults_for_test(
        &self,
    ) -> Arc<crate::application::request_registry::random_hooks::Controller> {
        self.inner.queue_randoms.clone()
    }
    fn management_audit_port(&self) -> Arc<dyn crate::application::request_registry::AuditPort> {
        Arc::new(self.inner.request_registry.management_audit())
    }

    /// Starts with this Storage owner's actual SQL audit port.
    /// # Errors
    /// Preserves the original runtime claim and startup errors.
    pub async fn start_queue_service<A, V, C>(
        &self,
        authority: A,
        validator: V,
        clock: C,
        environment: Arc<dyn crate::application::queue_owner::QueueEnvironmentPort>,
    ) -> Result<
        (
            crate::application::queue_owner::QueueService<
                A,
                V,
                crate::session_storage::request_registry::SqliteRequestRegistry,
                C,
            >,
            crate::application::queue_owner::NativeQueueDriver<V::Body>,
        ),
        crate::domain::request_registry::RegistryError,
    >
    where
        A: crate::application::request_registry::RequestAuthority + 'static,
        V: crate::application::request_registry::ValidatedBodyPort + 'static,
        C: crate::application::request_registry::NodeClock + 'static,
    {
        self.start_queue_service_with_repository(authority, validator, clock, environment, Ok)
            .await
    }

    /// Wraps only the necessary repository; audit stays bound to this Storage.
    /// # Errors
    /// Preserves original wrapper, identity and startup errors.
    pub async fn start_queue_service_with_repository<A, V, C, R, F>(
        &self,
        authority: A,
        validator: V,
        clock: C,
        environment: Arc<dyn crate::application::queue_owner::QueueEnvironmentPort>,
        wrap: F,
    ) -> Result<
        (
            crate::application::queue_owner::QueueService<A, V, R, C>,
            crate::application::queue_owner::NativeQueueDriver<V::Body>,
        ),
        crate::domain::request_registry::RegistryError,
    >
    where
        A: crate::application::request_registry::RequestAuthority + 'static,
        V: crate::application::request_registry::ValidatedBodyPort + 'static,
        C: crate::application::request_registry::NodeClock + 'static,
        R: crate::application::request_registry::RequestRegistryRepository + 'static,
        F: FnOnce(
                crate::session_storage::request_registry::SqliteRequestRegistry,
            ) -> Result<R, crate::domain::request_registry::RegistryError>
            + Send,
    {
        self.start_queue_service_composed(
            authority,
            validator,
            clock,
            environment,
            self.management_audit_port(),
            wrap,
        )
        .await
    }

    /// Keeps the original injected audit and repository wrapper in explicit fixture builds.
    /// # Errors
    /// Preserves the original wrapper, owner-scope and startup failures.
    #[cfg(any(test, feature = "registration-acceptance-fixtures"))]
    pub async fn start_queue_service_with_repository_and_audit_for_fixture<A, V, C, R, F>(
        &self,
        authority: A,
        validator: V,
        clock: C,
        environment: Arc<dyn crate::application::queue_owner::QueueEnvironmentPort>,
        audit: Arc<dyn crate::application::request_registry::AuditPort>,
        wrap: F,
    ) -> Result<
        (
            crate::application::queue_owner::QueueService<A, V, R, C>,
            crate::application::queue_owner::NativeQueueDriver<V::Body>,
        ),
        crate::domain::request_registry::RegistryError,
    >
    where
        A: crate::application::request_registry::RequestAuthority + 'static,
        V: crate::application::request_registry::ValidatedBodyPort + 'static,
        C: crate::application::request_registry::NodeClock + 'static,
        R: crate::application::request_registry::RequestRegistryRepository + 'static,
        F: FnOnce(
                crate::session_storage::request_registry::SqliteRequestRegistry,
            ) -> Result<R, crate::domain::request_registry::RegistryError>
            + Send,
    {
        self.start_queue_service_composed(authority, validator, clock, environment, audit, wrap)
            .await
    }

    /// Starts this storage owner's single queue runtime. Returned handles may still be Starting/Unconfirmed.
    /// # Errors
    /// Rejects a second live runtime or a failure before startup ownership exists.
    #[cfg(any(test, feature = "registration-acceptance-fixtures"))]
    pub async fn start_queue_service_with_audit_for_fixture<A, V, C>(
        &self,
        authority: A,
        validator: V,
        clock: C,
        environment: Arc<dyn crate::application::queue_owner::QueueEnvironmentPort>,
        audit: Arc<dyn crate::application::request_registry::AuditPort>,
    ) -> Result<
        (
            crate::application::queue_owner::QueueService<
                A,
                V,
                crate::session_storage::request_registry::SqliteRequestRegistry,
                C,
            >,
            crate::application::queue_owner::NativeQueueDriver<V::Body>,
        ),
        crate::domain::request_registry::RegistryError,
    >
    where
        A: crate::application::request_registry::RequestAuthority + 'static,
        V: crate::application::request_registry::ValidatedBodyPort + 'static,
        C: crate::application::request_registry::NodeClock + 'static,
    {
        self.start_queue_service_composed(authority, validator, clock, environment, audit, Ok)
            .await
    }
    /// Trusted repository wrapping preserves this exact startup claim and owner registry.
    /// # Errors
    /// Rejects a second runtime or a wrapper/random failure before startup is owned.
    async fn start_queue_service_composed<A, V, C, R, F>(
        &self,
        authority: A,
        validator: V,
        clock: C,
        environment: Arc<dyn crate::application::queue_owner::QueueEnvironmentPort>,
        audit: Arc<dyn crate::application::request_registry::AuditPort>,
        wrap: F,
    ) -> Result<
        (
            crate::application::queue_owner::QueueService<A, V, R, C>,
            crate::application::queue_owner::NativeQueueDriver<V::Body>,
        ),
        crate::domain::request_registry::RegistryError,
    >
    where
        A: crate::application::request_registry::RequestAuthority + 'static,
        V: crate::application::request_registry::ValidatedBodyPort + 'static,
        C: crate::application::request_registry::NodeClock + 'static,
        R: crate::application::request_registry::RequestRegistryRepository + 'static,
        F: FnOnce(
                crate::session_storage::request_registry::SqliteRequestRegistry,
            ) -> Result<R, crate::domain::request_registry::RegistryError>
            + Send,
    {
        use crate::{
            application::{queue_owner::QueueService, request_registry::random_id},
            domain::{native::RuntimeEpoch, request_registry::RegistryError},
        };
        authority.verify_owner_scope(&self.inner.request_registry.owner_scope())?;
        let map_claim_err = |failure| match failure {
            runtime_coordinator::ClaimFailure::Unavailable => RegistryError::StorageUnavailable,
            runtime_coordinator::ClaimFailure::Stopping => RegistryError::NodeStopping,
            runtime_coordinator::ClaimFailure::AlreadyStarted => {
                RegistryError::RuntimeAlreadyStarted
            }
        };
        let (service, driver, startup) =
            self.inner.coordinator.claim_runtime(map_claim_err, |ctx| {
                #[cfg(test)]
                let generated = self
                    .inner
                    .queue_randoms
                    .take(crate::application::request_registry::random_hooks::Point::Runtime)?
                    .unwrap_or_else(random_id);
                #[cfg(not(test))]
                let generated = random_id();
                let runtime = RuntimeEpoch(generated?);
                let repository = wrap(self.request_registry())?;
                let (service, driver) = QueueService::new(
                    authority,
                    validator,
                    repository,
                    self.inner.request_registry.owner_scope(),
                    clock,
                    runtime,
                    environment,
                    audit,
                    ctx.claim(),
                    #[cfg(test)]
                    self.inner.queue_randoms.clone(),
                );
                // Retain the supervisor before the first await. Dropping the factory waiter cannot
                // remove Storage's cancellation/join path or start a second runtime.
                #[cfg(windows)]
                let supervisor: Arc<
                    dyn crate::application::queue_owner::RuntimeSupervisor,
                > = Arc::new(prompt_preparation::QueuePreparationSupervisor {
                    service: service.clone(),
                    preparation: ctx.preparation(),
                    document: ctx.document(),
                    catalog: ctx.catalog(),
                });
                #[cfg(not(windows))]
                let supervisor: Arc<
                    dyn crate::application::queue_owner::RuntimeSupervisor,
                > = Arc::new(service.clone());
                ctx.install_supervisor(supervisor).map_err(map_claim_err)?;
                let startup = service.launch_startup()?;
                Ok((service, driver, startup))
            })?;
        let _ = startup.await;
        Ok((service, driver))
    }
    #[cfg(test)]
    #[must_use]
    pub fn product_commands(&self) -> crate::session_storage::product_commands::ProductStore {
        self.inner.product_store.clone()
    }
    /// Opens production storage while retaining its original node directory ownership guard.
    /// # Errors
    /// Rejects an unstable snapshot, unsupported structure or failed initialization.
    pub fn open_owned(
        directory: Arc<crate::data_dir::DataDirectory>,
    ) -> impl std::future::Future<Output = Result<Self, StorageError>> + Send {
        let prepared = (|| {
            use crate::session_storage::sqlite_lifecycle::{
                StorageLifecycleHost, StorageOpenOwner, StorageOpenRoots,
            };
            let owner = StorageOpenOwner::prepare(
                &tokio::runtime::Handle::try_current()
                    .map_err(|_| StorageError::Lifecycle("original storage runtime unavailable"))?,
                StorageOpenRoots::directory(directory),
            )?;
            let host = StorageLifecycleHost::install(owner.clone());
            let ticket = owner.start()?;
            Ok::<_, StorageError>((owner, host, ticket))
        })();
        async move {
            let (owner, host, ticket) = prepared?;
            match ticket.ready().await {
                Ok(storage) => Ok(storage.projection(host)),
                Err(e) => {
                    owner.request_stop();
                    if !host.observe_actual().await {
                        std::future::pending::<()>().await;
                    }
                    drop(ticket);
                    drop(host);
                    drop(owner);
                    Err(StorageError::from(e))
                }
            }
        }
    }

    /// Opens the SQLite database and initializes the supported schema.
    /// The caller must own this startup path exclusively (or an isolated test copy); live nodes
    /// use `open_owned`, retaining the directory guard through preflight and transaction validation.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot be opened, contains an unknown schema, or the
    /// schema transaction cannot be completed. An initialization failure closes the connection
    /// pool before returning.
    pub fn open(
        path: &Path,
    ) -> impl std::future::Future<Output = Result<Self, StorageError>> + Send + use<> {
        Self::open_with_pool_options(path, SqlitePoolOptions::new().max_connections(1))
    }

    fn open_with_pool_options(
        path: &Path,
        pool_options: SqlitePoolOptions,
    ) -> impl std::future::Future<Output = Result<Self, StorageError>> + Send + use<> {
        Self::open_with_pool_options_inner(
            path,
            pool_options,
            #[cfg(test)]
            None,
            #[cfg(test)]
            None,
        )
    }

    fn open_with_pool_options_inner(
        path: &Path,
        pool_options: SqlitePoolOptions,
        #[cfg(test)] fault: Option<crate::session_storage::schema::MigrationFault>,
        #[cfg(test)] namespace: Option<
            crate::session_storage::schema9_namespace_test_hook::NamespaceScenario,
        >,
    ) -> impl std::future::Future<Output = Result<Self, StorageError>> + Send + use<> {
        use crate::session_storage::sqlite_lifecycle::{
            StorageLifecycleHost, StorageOpenOwner, StorageOpenRoots,
        };
        let prepared = (|| {
            let owner = StorageOpenOwner::prepare(
                &tokio::runtime::Handle::try_current()
                    .map_err(|_| StorageError::Lifecycle("original storage runtime unavailable"))?,
                StorageOpenRoots::path(path),
            )?;
            let host = StorageLifecycleHost::install(owner.clone());
            let ticket = owner.start_with(
                pool_options,
                false,
                #[cfg(test)]
                fault,
                #[cfg(test)]
                namespace,
            )?;
            Ok::<_, StorageError>((owner, host, ticket))
        })();
        async move {
            let (owner, host, ticket) = prepared?;
            match ticket.ready().await {
                Ok(storage) => Ok(storage.projection(host)),
                Err(error) => {
                    owner.request_stop();
                    if !host.observe_actual().await {
                        std::future::pending::<()>().await;
                    }
                    drop(ticket);
                    drop(host);
                    drop(owner);
                    Err(StorageError::from(error))
                }
            }
        }
    }
    pub(crate) async fn open_in_scope(
        path: &Path,
        pool_options: SqlitePoolOptions,
        scope: &Arc<crate::session_storage::sqlite_lifecycle::SqliteOpenState>,
        #[cfg(test)] fault: Option<crate::session_storage::schema::MigrationFault>,
        #[cfg(test)] namespace: Option<
            crate::session_storage::schema9_namespace_test_hook::NamespaceScenario,
        >,
    ) -> Result<Self, StorageError> {
        crate::session_storage::schema::preflight_in_scope(path, scope).await?;
        #[cfg(test)]scope.hooks.visit(crate::session_storage::sqlite_lifecycle::TestPoint::PreflightJoinedBeforePoolConnect).await;
        let options = SqliteConnectOptions::new()
            .filename(path)
            .worker_registry(scope.registry().clone())
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Full)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        let pool = pool_options.connect_with(options).await?;
        scope.store_pool(pool.clone());
        #[cfg(test)]
        scope
            .hooks
            .visit(
                crate::session_storage::sqlite_lifecycle::TestPoint::PoolConnectedBeforeInitialize,
            )
            .await;
        let storage = Self {
            lifecycle: Arc::downgrade(scope),
            host: None,
            inner: Arc::new(StorageInner {
                #[cfg(test)]
                product_store: crate::session_storage::product_commands::ProductStore::new(
                    pool.clone(),
                ),
                request_registry:
                    crate::session_storage::request_registry::SqliteRequestRegistry::new_scoped(
                        pool.clone(),
                        Arc::downgrade(scope),
                    ),
                coordinator: runtime_coordinator::StorageRuntimeCoordinator::new(),
                #[cfg(test)]
                queue_randoms: Arc::default(),
                pool,
                #[cfg(test)]
                close_barrier: std::sync::Mutex::new(None),
            }),
        };
        #[cfg(test)]
        let initialization = crate::session_storage::schema::initialize_catalog_for_test(
            &storage.inner.pool,
            fault,
            namespace,
        )
        .await;
        #[cfg(not(test))]
        let initialization = crate::session_storage::schema::initialize(&storage.inner.pool).await;
        initialization?;
        Ok(storage)
    }

    /// Loads the persisted node identity metadata, if present.
    ///
    /// # Errors
    ///
    /// Returns an error when the metadata query fails.
    pub async fn load_node_meta(&self) -> Result<Option<NodeMeta>, StorageError> {
        let row = sqlx::query(
            "SELECT schema_version, node_id, public_key, key_file_ref \
             FROM node_meta WHERE singleton = 1",
        )
        .fetch_optional(&self.inner.pool)
        .await?;
        Ok(row.map(|row| NodeMeta {
            schema_version: row.get("schema_version"),
            node_id: row.get("node_id"),
            public_key: row.get("public_key"),
            key_file_ref: row.get("key_file_ref"),
        }))
    }

    /// Atomically writes the node identity metadata singleton.
    ///
    /// # Errors
    ///
    /// Returns an error when the metadata transaction cannot be committed.
    pub async fn write_node_meta(&self, meta: &NodeMeta) -> Result<(), StorageError> {
        if meta.schema_version != SCHEMA_VERSION {
            return Err(StorageError::UnrecognizedSchema);
        }
        let mut transaction = self.inner.pool.begin().await?;
        sqlx::query(
            "INSERT INTO node_meta(singleton, schema_version, node_id, public_key, key_file_ref) \
             VALUES (1, ?, ?, ?, ?) \
             ON CONFLICT(singleton) DO UPDATE SET \
                schema_version = excluded.schema_version, \
                node_id = excluded.node_id, \
                public_key = excluded.public_key, \
                key_file_ref = excluded.key_file_ref",
        )
        .bind(meta.schema_version)
        .bind(&meta.node_id)
        .bind(&meta.public_key)
        .bind(&meta.key_file_ref)
        .execute(&mut *transaction)
        .await?;
        #[cfg(test)]
        crate::test_support::crash_at("identity-before-meta-commit");
        transaction.commit().await?;
        #[cfg(test)]
        crate::test_support::crash_at("identity-after-meta-commit");
        Ok(())
    }

    pub(crate) async fn has_identity_history(&self) -> Result<bool, StorageError> {
        let present: i64 = sqlx::query_scalar(
            "SELECT CASE WHEN
                EXISTS(SELECT 1 FROM clients)
                OR EXISTS(SELECT 1 FROM management_commands)
                OR EXISTS(SELECT 1 FROM security_audit)
             THEN 1 ELSE 0 END",
        )
        .fetch_one(&self.inner.pool)
        .await?;
        Ok(present != 0)
    }

    /// Reads the current device-list revision.
    ///
    /// # Errors
    ///
    /// Returns an error when the revision row cannot be read.
    pub async fn list_revision(&self) -> Result<i64, StorageError> {
        sqlx::query_scalar("SELECT revision FROM node_state WHERE singleton = 1")
            .fetch_one(&self.inner.pool)
            .await
            .map_err(StorageError::from)
    }

    #[cfg(test)]
    pub(crate) fn original_runtime_registration_for_test(&self) -> (bool, Option<usize>) {
        self.inner.coordinator.runtime_registration_for_test()
    }
    /// Synchronously seals installation on the original runtime lifecycle fence.
    pub(crate) fn begin_runtime_shutdown(&self) {
        self.inner.coordinator.begin_shutdown();
        // Keep registry leases available through native source metadata closure. The
        // combined supervisor performs physical shutdown and required SQL bookkeeping
        // before joining; close_in_scope seals the registry only after that join.
    }
    pub async fn close(&self) {
        self.close_after_users(std::future::ready(())).await;
    }
    pub(crate) async fn close_after_users(&self, users: impl std::future::Future<Output = ()>) {
        self.begin_runtime_shutdown();
        if let Some(scope) = self.lifecycle.upgrade() {
            scope.require_users();
            let users = async {
                users.await;
                scope.users_finished();
            };
            tokio::join!(users, scope.stop_and_observe());
        }
    }
    pub(crate) async fn close_in_scope(&self) {
        self.inner.coordinator.close_supervisor_and_join().await;
        #[cfg(test)]
        {
            let barrier = self
                .inner
                .close_barrier
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            if let Some(barrier) = barrier {
                barrier.pause().await;
            }
        }
        // Native/Queue/body closure advances concurrently with original transport users.
        if let Some(scope) = self.lifecycle.upgrade() {
            scope.wait_users().await;
            self.inner.request_registry.seal_connections();
        }
        let registry = self.inner.request_registry.close_connections();
        let pool_close = async {
            loop {
                self.inner.pool.close().await;
                if self.inner.pool.size() == 0 {
                    break;
                }
                // Time only limits rechecks; normal closure still requires the real pool count.
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        registry.await;
        if let Some(scope) = self.lifecycle.upgrade() {
            scope.final_sql_fence().await;
        }
        pool_close.await;
    }
    pub(crate) fn required_close_status_in_scope(
        &self,
    ) -> crate::session_storage::ConnectionCloseStatus {
        self.inner.request_registry.close_status()
    }
    pub(crate) fn registry_close_status(&self) -> crate::session_storage::ConnectionCloseStatus {
        use crate::session_storage::{ConnectionCloseStatus as Status, sqlite_lifecycle::Actual};
        match self.lifecycle.upgrade().map(|s| s.actual()) {
            Some(Actual::Joined) => self.inner.request_registry.close_status(),
            Some(Actual::Unconfirmed) | None => Status::Unconfirmed,
            _ => Status::Pending,
        }
    }

    #[cfg(test)]
    pub(crate) fn pause_next_close(&self) -> Arc<crate::test_support::AsyncBarrier> {
        let barrier = Arc::new(crate::test_support::AsyncBarrier::default());
        let mut slot = self
            .inner
            .close_barrier
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(slot.is_none(), "only one close barrier can be installed");
        *slot = Some(Arc::clone(&barrier));
        barrier
    }

    #[must_use]
    pub(crate) fn trust_store(&self) -> crate::trust::TrustStore {
        crate::trust::TrustStore::new(self.request_registry())
    }

    #[cfg(test)]
    pub(crate) fn pool_for_test(&self) -> &SqlitePool {
        &self.inner.pool
    }
}

impl Drop for Storage {
    fn drop(&mut self) {
        // Seal the storage-scoped registry before releasing the last Storage owner. A retained
        // external registry clone must hand off its idle connection into the tracked close owner.
        self.inner.request_registry.seal_connections();
        if self.host.is_some()
            && let Some(scope) = self.lifecycle.upgrade()
        {
            crate::session_storage::sqlite_lifecycle::StorageOpenOwner { state: scope }
                .request_stop();
        }
    }
}

#[cfg(test)]
mod pool_close_tests;

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn test_database() -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "acp-remote-storage-{}-{nonce}.sqlite3",
            std::process::id()
        ))
    }

    #[tokio::test]
    async fn initializes_and_persists_node_metadata() {
        let path = test_database();
        let storage = Storage::open(&path).await.expect("storage");
        assert_eq!(storage.load_node_meta().await.expect("meta"), None);
        let meta = NodeMeta {
            schema_version: SCHEMA_VERSION,
            node_id: "a".repeat(64),
            public_key: vec![4; 65],
            key_file_ref: "identity.key".to_owned(),
        };
        storage.write_node_meta(&meta).await.expect("write meta");
        assert_eq!(
            storage.load_node_meta().await.expect("read meta"),
            Some(meta)
        );
        storage.close().await;
        std::fs::remove_file(path).expect("cleanup");
    }

    #[tokio::test]
    async fn rejects_an_unknown_schema_without_modifying_data() {
        let path = test_database();
        let storage = Storage::open(&path).await.expect("storage");
        sqlx::query("PRAGMA user_version = 77")
            .execute(storage.pool_for_test())
            .await
            .expect("set schema");
        sqlx::query("CREATE TABLE sentinel(value TEXT NOT NULL)")
            .execute(storage.pool_for_test())
            .await
            .expect("create sentinel");
        sqlx::query("INSERT INTO sentinel(value) VALUES ('preserve-me')")
            .execute(storage.pool_for_test())
            .await
            .expect("insert sentinel");
        storage.close().await;

        let error = Storage::open(&path).await.err().expect("unknown schema");
        assert!(matches!(error, StorageError::UnknownSchema(77)));

        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(false);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("inspect preserved database");
        let value: String = sqlx::query_scalar("SELECT value FROM sentinel")
            .fetch_one(&pool)
            .await
            .expect("read sentinel");
        assert_eq!(value, "preserve-me");
        pool.close().await;
        std::fs::remove_file(path).expect("cleanup");
    }

    #[tokio::test]
    async fn close_cancels_and_joins_runtime_supervisor_before_sealing_and_closing_pool() {
        let path = test_database();
        let storage = Arc::new(Storage::open(&path).await.expect("storage"));
        let supervisor = storage
            .inner
            .coordinator
            .claim_runtime(
                |err| err,
                |ctx| {
                    let supervisor =
                        runtime_coordinator::tests::RecordingSupervisor::new(ctx.claim());
                    ctx.install_supervisor(Arc::clone(&supervisor)
                        as Arc<dyn crate::application::queue_owner::RuntimeSupervisor>)?;
                    Ok(supervisor)
                },
            )
            .expect("claim runtime supervisor");

        let close_task = {
            let storage = Arc::clone(&storage);
            tokio::spawn(async move {
                storage.close().await;
            })
        };

        tokio::time::timeout(Duration::from_secs(2), supervisor.join_entered.notified())
            .await
            .expect("supervisor cancel triggered stop_and_join");

        assert_eq!(supervisor.events(), vec!["cancel", "stop_and_join_start"]);
        assert!(
            !storage.pool_for_test().is_closed(),
            "pool must remain open until supervisor stop_and_join finishes"
        );
        assert!(
            !storage.required_close_status_in_scope().is_complete(),
            "registry connections must not complete close before supervisor joins"
        );

        supervisor.release_join.notify_one();
        tokio::time::timeout(Duration::from_secs(2), close_task)
            .await
            .expect("storage close completes")
            .expect("close task did not panic");

        assert_eq!(
            supervisor.events(),
            vec!["cancel", "stop_and_join_start", "stop_and_join_finish"]
        );
        assert_eq!(
            storage.original_runtime_registration_for_test(),
            (false, None)
        );
        assert!(storage.pool_for_test().is_closed());
        assert_eq!(storage.pool_for_test().size(), 0);
        assert!(storage.required_close_status_in_scope().is_complete());
        drop(storage);
        std::fs::remove_file(path).expect("cleanup");
    }
}
