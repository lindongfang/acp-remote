#![allow(
    clippy::struct_field_names,
    clippy::too_many_lines,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use std::{
    ffi::OsStr,
    io,
    os::windows::ffi::OsStrExt,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::{ClientOptions, NamedPipeClient, NamedPipeServer},
    sync::{Mutex as AsyncMutex, Semaphore, watch},
    task::JoinSet,
    time::{Instant as TokioInstant, sleep_until, timeout, timeout_at},
};

use crate::{
    application::request_registry::RegistryDeadline,
    claims::Id128,
    domain::{
        EntityId,
        native_catalog::{
            CatalogError, CatalogFailure, CatalogInventory, ProductTargetRecord, WorkspaceRecord,
        },
    },
    error::AppError,
    jws::decode_fixed_base64url,
    pairing::{
        PAIRING_LIFETIME_SECONDS, PairingError, PairingEvent, PairingService, format_pairing_code,
    },
    platform::{create_private_pipe, current_user_sid, verify_named_pipe_owner},
    random,
    session_storage::SqliteNodeConfiguration,
    trust::{ClientPage, RevokeResult, TrustError, TrustStore},
};

pub const MAX_IPC_MESSAGE: usize = 32 * 1024;
const MAX_IPC_CONNECTIONS: usize = 32;
const IPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const IPC_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(16);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalIpcAddress {
    pub pipe_name: String,
    pub owner_sid: String,
}

impl LocalIpcAddress {
    /// Derives the stable local IPC address for a data directory and its current owner.
    ///
    /// # Errors
    ///
    /// Returns an error when the current owner SID cannot be read or the data directory cannot be
    /// canonicalized.
    pub fn for_data_directory(data_dir: &std::path::Path) -> Result<Self, AppError> {
        let owner_sid = current_user_sid()?;
        let canonical = std::fs::canonicalize(data_dir).map_err(|source| AppError::Io {
            context: "canonicalizing the data directory for IPC",
            source,
        })?;
        let digest = {
            use sha2::{Digest, Sha256};
            let mut hash = Sha256::new();
            hash.update(b"acp-remote/local-ipc/v1\0");
            hash.update(owner_sid.as_bytes());
            hash.update([0]);
            for unit in canonical.as_os_str().encode_wide() {
                hash.update(unit.to_le_bytes());
            }
            hex::encode(hash.finalize())
        };
        Ok(Self {
            pipe_name: format!(r"\\.\pipe\acp-remote.{digest}"),
            owner_sid,
        })
    }

    /// Creates the first instance of the local named pipe.
    ///
    /// # Errors
    ///
    /// Returns an error when the pipe already exists or cannot be created with the owner-only
    /// security descriptor.
    pub fn bind_first(&self) -> io::Result<NamedPipeServer> {
        create_private_pipe(OsStr::new(&self.pipe_name), &self.owner_sid, true)
    }

    /// Creates an additional instance of the local named pipe.
    ///
    /// # Errors
    ///
    /// Returns an error when the pipe server cannot be created with the owner-only security
    /// descriptor.
    pub fn bind_next(&self) -> io::Result<NamedPipeServer> {
        create_private_pipe(OsStr::new(&self.pipe_name), &self.owner_sid, false)
    }

    /// Verifies that the connected named-pipe client is the configured Windows owner.
    ///
    /// # Errors
    ///
    /// Returns an error when the client token cannot be inspected, does not match the owner SID,
    /// or impersonation cannot be safely reverted.
    pub fn verify_peer(&self, server: &NamedPipeServer) -> Result<(), AppError> {
        verify_named_pipe_owner(server, &self.owner_sid).map_err(AppError::Platform)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodePhase {
    Ready,
    Stopping,
    Failed,
    Stopped,
}

impl NodePhase {
    const fn as_u8(self) -> u8 {
        match self {
            Self::Ready => 0,
            Self::Stopping => 1,
            Self::Failed => 2,
            Self::Stopped => 3,
        }
    }

    const fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Ready,
            1 => Self::Stopping,
            2 => Self::Failed,
            _ => Self::Stopped,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Stopping => "stopping",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShutdownResult {
    Pending,
    Completed,
    Failed(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeStatus {
    pub state: String,
    pub node_id: String,
    pub origin: String,
    pub version: String,
    pub pid: u32,
}

#[derive(Clone)]
pub struct IpcControl {
    node_id: String,
    origin: String,
    phase: Arc<AtomicU8>,
    failure: Arc<Mutex<Option<String>>>,
    stop_requested: watch::Sender<bool>,
    shutdown: watch::Sender<ShutdownResult>,
    stop_responses: Arc<AsyncMutex<JoinSet<io::Result<()>>>>,
    #[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
    first_revoke_error: Arc<std::sync::OnceLock<RevokeErrorObservation>>,
}

impl IpcControl {
    #[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
    pub(crate) fn first_revoke_error_for_development(&self) -> Option<RevokeErrorObservation> {
        self.first_revoke_error.get().copied()
    }

    #[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
    fn observe_revoke_error(&self, error: &TrustError) {
        let observed = match error {
            TrustError::NotFound => RevokeErrorObservation::NotFound,
            TrustError::InvalidRequest => RevokeErrorObservation::InvalidRequest,
            TrustError::RequestConflict => RevokeErrorObservation::RequestConflict,
            TrustError::PublicKeyAlreadyRegistered => {
                RevokeErrorObservation::PublicKeyAlreadyRegistered
            }
            TrustError::ClientIdCollision => RevokeErrorObservation::ClientIdCollision,
            TrustError::StaleCursor => RevokeErrorObservation::StaleCursor,
            TrustError::StorageUnavailable => RevokeErrorObservation::StorageUnavailable,
            TrustError::CapacityExhausted => RevokeErrorObservation::CapacityExhausted,
            TrustError::Storage(_) => RevokeErrorObservation::Storage,
            TrustError::Database(error) => match error {
                sqlx::Error::PoolClosed => RevokeErrorObservation::PoolClosed,
                sqlx::Error::PoolTimedOut => RevokeErrorObservation::PoolTimedOut,
                sqlx::Error::Io(_) => RevokeErrorObservation::DatabaseIo,
                sqlx::Error::Database(database) => match database.code().as_deref() {
                    Some("5") => RevokeErrorObservation::SqliteBusy,
                    Some("517") => RevokeErrorObservation::SqliteBusySnapshot,
                    Some("6") => RevokeErrorObservation::SqliteLocked,
                    _ => RevokeErrorObservation::DatabaseOtherCode,
                },
                _ => RevokeErrorObservation::DatabaseOther,
            },
        };
        let _ = self.first_revoke_error.set(observed);
    }

    #[must_use]
    pub fn new(node_id: String, origin: String) -> (Self, watch::Receiver<bool>) {
        let (stop_requested, stop_rx) = watch::channel(false);
        let (shutdown, _) = watch::channel(ShutdownResult::Pending);
        (
            Self {
                node_id,
                origin,
                phase: Arc::new(AtomicU8::new(NodePhase::Ready.as_u8())),
                failure: Arc::new(Mutex::new(None)),
                stop_requested,
                shutdown,
                stop_responses: Arc::new(AsyncMutex::new(JoinSet::new())),
                #[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
                first_revoke_error: Arc::new(std::sync::OnceLock::new()),
            },
            stop_rx,
        )
    }

    #[must_use]
    pub fn status(&self) -> NodeStatus {
        NodeStatus {
            state: NodePhase::from_u8(self.phase.load(Ordering::Acquire))
                .as_str()
                .to_owned(),
            node_id: self.node_id.clone(),
            origin: self.origin.clone(),
            version: crate::version().to_owned(),
            pid: std::process::id(),
        }
    }

    pub fn request_stop(&self) {
        let _ = self.phase.compare_exchange(
            NodePhase::Ready.as_u8(),
            NodePhase::Stopping.as_u8(),
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        self.stop_requested.send_replace(true);
    }

    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.phase.load(Ordering::Acquire) == NodePhase::Ready.as_u8()
    }

    pub fn mark_failed(&self, message: impl Into<String>) {
        *self.failure.lock().unwrap_or_else(PoisonError::into_inner) = Some(message.into());
        self.phase
            .store(NodePhase::Failed.as_u8(), Ordering::Release);
        self.stop_requested.send_replace(true);
    }

    #[must_use]
    pub fn failure(&self) -> Option<String> {
        self.failure
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn complete(&self, result: ShutdownResult) {
        let phase = match &result {
            ShutdownResult::Completed => NodePhase::Stopped,
            ShutdownResult::Failed(_) => NodePhase::Failed,
            ShutdownResult::Pending => NodePhase::Stopping,
        };
        self.shutdown.send_if_modified(|published| {
            if !matches!(published, ShutdownResult::Pending) {
                return false;
            }
            self.phase.store(phase.as_u8(), Ordering::Release);
            if matches!(&result, ShutdownResult::Pending) {
                return false;
            }
            *published = result;
            true
        });
    }

    #[must_use]
    pub fn stop_receiver(&self) -> watch::Receiver<bool> {
        self.stop_requested.subscribe()
    }

    #[must_use]
    pub fn shutdown_receiver(&self) -> watch::Receiver<ShutdownResult> {
        self.shutdown.subscribe()
    }

    async fn spawn_stop_response(&self, mut pipe: NamedPipeServer, request_id: String) {
        let control = self.clone();
        self.stop_responses.lock().await.spawn(async move {
            let mut shutdown = control.shutdown_receiver();
            let response = loop {
                if *shutdown.borrow() == ShutdownResult::Completed {
                    break IpcResponse::status(request_id, control.status());
                }
                if let ShutdownResult::Failed(message) = &*shutdown.borrow() {
                    break IpcResponse::error(request_id, message.clone());
                }
                match timeout(IPC_SHUTDOWN_TIMEOUT, shutdown.changed()).await {
                    Ok(Ok(())) => {}
                    Ok(Err(_)) => {
                        break IpcResponse::error(request_id, "node_stopping_without_result");
                    }
                    Err(_) => {
                        break IpcResponse::error(request_id, "shutdown_timeout");
                    }
                }
            };
            write_response(&mut pipe, response).await
        });
    }

    pub(crate) async fn finish_stop_responses(&self, deadline: TokioInstant) -> Result<(), String> {
        let mut responses = self.stop_responses.lock().await;
        let mut failures = Vec::new();
        while !responses.is_empty() {
            match timeout_at(deadline, responses.join_next()).await {
                Ok(Some(Ok(Ok(())))) => {}
                Ok(Some(Ok(Err(error)))) => {
                    failures.push(format!(
                        "delivering the owner stop response failed: {error}"
                    ));
                }
                Ok(Some(Err(error))) => {
                    failures.push(format!("owner stop response task failed: {error}"));
                }
                Ok(None) => break,
                Err(_) => {
                    responses.abort_all();
                    while responses.join_next().await.is_some() {}
                    failures.push(
                        "shutdown exceeded the response deadline while delivering the stop result"
                            .to_owned(),
                    );
                    break;
                }
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }
}

/// Runs the owner-only local control channel until shutdown completes.
///
/// # Errors
///
/// Returns an error when the next pipe instance cannot be created or a connection task terminates
/// unexpectedly.
pub async fn serve(
    first_instance: NamedPipeServer,
    address: LocalIpcAddress,
    control: IpcControl,
    trust: TrustStore,
    pairing: Arc<PairingService>,
) -> Result<(), AppError> {
    serve_with_catalog(first_instance, address, control, trust, pairing, None).await
}

/// Runs the owner-only local control channel with access to node catalog configuration until shutdown completes.
///
/// # Errors
///
/// Returns an error when the next pipe instance cannot be created or a connection task terminates
/// unexpectedly.
pub async fn serve_with_catalog(
    first_instance: NamedPipeServer,
    address: LocalIpcAddress,
    control: IpcControl,
    trust: TrustStore,
    pairing: Arc<PairingService>,
    catalog: Option<Arc<SqliteNodeConfiguration>>,
) -> Result<(), AppError> {
    serve_inner(
        first_instance,
        address,
        control,
        trust,
        pairing,
        catalog,
        #[cfg(test)]
        None,
    )
    .await
}
#[cfg(test)]
pub(crate) struct IpcDevelopmentHooks {
    pub(crate) child: Arc<crate::test_support::AsyncBarrier>,
    pub(crate) accept_error_after_first: bool,
}
#[cfg(test)]
pub(crate) async fn serve_development(
    first_instance: NamedPipeServer,
    address: LocalIpcAddress,
    control: IpcControl,
    trust: TrustStore,
    pairing: Arc<PairingService>,
    hooks: Option<Arc<IpcDevelopmentHooks>>,
) -> Result<(), AppError> {
    serve_inner(
        first_instance,
        address,
        control,
        trust,
        pairing,
        None,
        hooks,
    )
    .await
}
#[cfg(test)]
pub(crate) async fn serve_development_with_catalog(
    first_instance: NamedPipeServer,
    address: LocalIpcAddress,
    control: IpcControl,
    trust: TrustStore,
    pairing: Arc<PairingService>,
    catalog: Option<Arc<SqliteNodeConfiguration>>,
    hooks: Option<Arc<IpcDevelopmentHooks>>,
) -> Result<(), AppError> {
    serve_inner(
        first_instance,
        address,
        control,
        trust,
        pairing,
        catalog,
        hooks,
    )
    .await
}
async fn serve_inner(
    first_instance: NamedPipeServer,
    address: LocalIpcAddress,
    control: IpcControl,
    trust: TrustStore,
    pairing: Arc<PairingService>,
    catalog: Option<Arc<SqliteNodeConfiguration>>,
    #[cfg(test)] hooks: Option<Arc<IpcDevelopmentHooks>>,
) -> Result<(), AppError> {
    let semaphore = Arc::new(Semaphore::new(MAX_IPC_CONNECTIONS));
    let mut shutdown = control.shutdown_receiver();
    let mut stop = control.stop_receiver();
    let mut server = Some(first_instance);
    let mut connections = JoinSet::new();

    let mut failure = None;
    'accepting: loop {
        if *stop.borrow() || !matches!(*shutdown.borrow(), ShutdownResult::Pending) {
            break;
        }
        let Some(current) = server.take() else {
            failure = Some(AppError::Ipc(
                "owner control channel lost its listening instance".to_owned(),
            ));
            break;
        };
        tokio::select! {
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    break;
                }
                server = Some(current);
            }
            changed = shutdown.changed() => {
                if changed.is_err() || !matches!(*shutdown.borrow(), ShutdownResult::Pending) {
                    break;
                }
                server = Some(current);
            }
            connected = current.connect() => {
                if let Err(error) = connected {
                    failure = Some(AppError::Ipc(format!("accepting an owner control connection failed: {error}")));
                    break;
                }
                let Ok(permit) = semaphore.clone().try_acquire_owned() else {
                    // Release the over-budget server before reserving the next listener.
                    drop(current);
                    server = match bind_next_available(&address, &control).await {
                        Ok(server) => server, Err(error) => { failure = Some(error); break 'accepting; }
                    };
                    if server.is_none() { break; }
                    continue;
                };
                let Some(next) = (match bind_next_available(&address, &control).await {
                    Ok(server) => server, Err(error) => { failure = Some(error); break 'accepting; }
                }) else { break; };
                server = Some(next);
                let address = address.clone();
                let control = control.clone();
                let trust = trust.clone();
                let pairing = pairing.clone();
                let catalog = catalog.clone();
                #[cfg(test)]
                let child_hooks = hooks.clone();
                connections.spawn(async move {
                    let _permit = permit;
                    #[cfg(test)]
                    if let Some(hooks) = child_hooks { hooks.child.pause().await; }
                    handle_connection(
                        current,
                        &address,
                        &control,
                        &trust,
                        &pairing,
                        catalog.as_deref(),
                    )
                    .await;
                });
                #[cfg(test)]
                if hooks.as_ref().is_some_and(|hooks| hooks.accept_error_after_first) {
                    failure = Some(AppError::Ipc("development injected IPC accept failure".into()));
                    break;
                }
            }
            joined = connections.join_next(), if !connections.is_empty() => {
                if let Some(Err(error)) = joined {
                    failure = Some(AppError::Ipc(format!("owner control connection task failed: {error}")));
                    break;
                }
                server = Some(current);
            }
        }
    }

    drop(server);
    while !connections.is_empty() {
        match timeout(Duration::from_secs(2), connections.join_next()).await {
            Ok(Some(Ok(())) | None) => {}
            Ok(Some(Err(error))) => {
                if failure.is_none() {
                    failure = Some(AppError::Ipc(format!(
                        "owner control connection task failed: {error}"
                    )));
                }
                connections.abort_all();
                while connections.join_next().await.is_some() {}
            }
            Err(_) => {
                if failure.is_none() {
                    failure = Some(AppError::Ipc(
                        "owner control handlers did not stop in time".into(),
                    ));
                }
                connections.abort_all();
                while connections.join_next().await.is_some() {}
            }
        }
    }
    failure.map_or(Ok(()), Err)
}

async fn bind_next_available(
    address: &LocalIpcAddress,
    control: &IpcControl,
) -> Result<Option<NamedPipeServer>, AppError> {
    let mut stop = control.stop_receiver();
    loop {
        if *stop.borrow() {
            return Ok(None);
        }
        match address.bind_next() {
            Ok(server) => return Ok(Some(server)),
            // Closed server instances may remain allocated until clients close their
            // handles. Treat this as backpressure, not a fatal listener failure.
            Err(error) if error.raw_os_error() == Some(231) => {}
            Err(error) => {
                return Err(AppError::Ipc(format!(
                    "creating the next owner control instance failed: {error}"
                )));
            }
        }
        tokio::select! {
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() { return Ok(None); }
            }
            () = tokio::time::sleep(Duration::from_millis(10)) => {}
        }
    }
}

async fn handle_connection(
    mut pipe: NamedPipeServer,
    address: &LocalIpcAddress,
    control: &IpcControl,
    trust: &TrustStore,
    pairing: &Arc<PairingService>,
    catalog: Option<&SqliteNodeConfiguration>,
) {
    if address.verify_peer(&pipe).is_err() {
        return;
    }
    let Ok(Ok(frame)) = timeout(IPC_REQUEST_TIMEOUT, read_frame(&mut pipe)).await else {
        return;
    };
    let Ok(request) = serde_json::from_slice::<IpcRequest>(&frame) else {
        let _ = write_response(
            &mut pipe,
            IpcResponse::error(String::new(), "invalid_request"),
        )
        .await;
        return;
    };
    let result = if request.command == "device.pair" {
        handle_pair_session(&mut pipe, control, pairing, request).await
    } else if request.command == "stop" && request.v == 1 && is_request_id(&request.request_id) {
        control.request_stop();
        control.spawn_stop_response(pipe, request.request_id).await;
        return;
    } else {
        let response = process_request(request, control, trust, catalog).await;
        write_response(&mut pipe, response).await
    };
    let _ = result;
}

async fn process_request(
    request: IpcRequest,
    control: &IpcControl,
    trust: &TrustStore,
    catalog: Option<&SqliteNodeConfiguration>,
) -> IpcResponse {
    if request.v != 1 || !is_request_id(&request.request_id) {
        return IpcResponse::error(request.request_id, "invalid_request");
    }
    match request.command.as_str() {
        "status" => IpcResponse::status(request.request_id, control.status()),
        "device.list" => {
            if !control.is_ready() {
                return IpcResponse::error(request.request_id, "node_stopping");
            }
            let payload: ListRequest = match serde_json::from_value(
                request.payload.unwrap_or_else(|| serde_json::json!({})),
            ) {
                Ok(payload) => payload,
                Err(_) => return IpcResponse::error(request.request_id, "invalid_request"),
            };
            match trust
                .list_page("local-owner", payload.cursor.as_deref())
                .await
            {
                Ok(page) => IpcResponse::result(
                    request.request_id,
                    serde_json::to_value(page)
                        .map_err(|_| AppError::Ipc("cannot encode the client page".to_owned())),
                ),
                Err(error) => IpcResponse::error(request.request_id, trust_error_code(&error)),
            }
        }
        "device.revoke" => {
            if !control.is_ready() {
                return IpcResponse::error(request.request_id, "node_stopping");
            }
            let payload: RevokeRequest = match serde_json::from_value(
                request.payload.unwrap_or_else(|| serde_json::json!({})),
            ) {
                Ok(payload) => payload,
                Err(_) => return IpcResponse::error(request.request_id, "invalid_request"),
            };
            match trust
                .revoke("local-owner", &request.request_id, &payload.client_id)
                .await
            {
                Ok(result) => IpcResponse::result(
                    request.request_id,
                    serde_json::to_value(result)
                        .map_err(|_| AppError::Ipc("cannot encode the revoke result".to_owned())),
                ),
                Err(error) => {
                    #[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
                    control.observe_revoke_error(&error);
                    IpcResponse::error(request.request_id, trust_error_code(&error))
                }
            }
        }
        "workspace.register" => {
            if !control.is_ready() {
                return IpcResponse::error(request.request_id, "node_stopping");
            }
            let Some(catalog) = catalog else {
                return IpcResponse::error(request.request_id, "product_not_ready");
            };
            let payload: WorkspaceRegisterRequest = match serde_json::from_value(
                request.payload.unwrap_or_else(|| serde_json::json!({})),
            ) {
                Ok(payload) => payload,
                Err(_) => return IpcResponse::error(request.request_id, "invalid_request"),
            };
            let deadline = RegistryDeadline::new(TokioInstant::now() + IPC_REQUEST_TIMEOUT);
            match catalog
                .register_workspace_owner(&payload.path, &deadline)
                .await
            {
                Ok(record) => IpcResponse::result(
                    request.request_id,
                    serde_json::to_value(workspace_ipc_record(&record)).map_err(|_| {
                        AppError::Ipc("cannot encode the workspace record".to_owned())
                    }),
                ),
                Err(error) => IpcResponse::error(request.request_id, catalog_failure_code(&error)),
            }
        }
        "target.register" => {
            if !control.is_ready() {
                return IpcResponse::error(request.request_id, "node_stopping");
            }
            let Some(catalog) = catalog else {
                return IpcResponse::error(request.request_id, "product_not_ready");
            };
            let payload: TargetRegisterRequest = match serde_json::from_value(
                request.payload.unwrap_or_else(|| serde_json::json!({})),
            ) {
                Ok(payload) => payload,
                Err(_) => return IpcResponse::error(request.request_id, "invalid_request"),
            };
            let Ok(source) = EntityId::decode(&payload.source_id) else {
                return IpcResponse::error(request.request_id, "invalid_source");
            };
            let Ok(workspace) = EntityId::decode(&payload.workspace_id) else {
                return IpcResponse::error(request.request_id, "invalid_configuration");
            };
            let deadline = RegistryDeadline::new(TokioInstant::now() + IPC_REQUEST_TIMEOUT);
            match catalog
                .register_target_owner(source, &payload.native_session_id, workspace, &deadline)
                .await
            {
                Ok(record) => IpcResponse::result(
                    request.request_id,
                    serde_json::to_value(product_target_ipc_record(&record))
                        .map_err(|_| AppError::Ipc("cannot encode the target record".to_owned())),
                ),
                Err(error) => IpcResponse::error(request.request_id, catalog_failure_code(&error)),
            }
        }
        "catalog.inventory" => {
            if !control.is_ready() {
                return IpcResponse::error(request.request_id, "node_stopping");
            }
            if !is_empty_pair_request(request.payload.as_ref()) {
                return IpcResponse::error(request.request_id, "invalid_request");
            }
            let Some(catalog) = catalog else {
                return IpcResponse::error(request.request_id, "product_not_ready");
            };
            let deadline = RegistryDeadline::new(TokioInstant::now() + IPC_REQUEST_TIMEOUT);
            match catalog.catalog_inventory_owner(&deadline).await {
                Ok(inventory) => IpcResponse::result(
                    request.request_id,
                    serde_json::to_value(catalog_inventory_ipc(&inventory)).map_err(|_| {
                        AppError::Ipc("cannot encode the catalog inventory".to_owned())
                    }),
                ),
                Err(error) => IpcResponse::error(request.request_id, catalog_failure_code(&error)),
            }
        }
        _ => IpcResponse::error(request.request_id, "invalid_request"),
    }
}

// Keep the ordered pairing transitions together so cancellation and commit boundaries stay clear.
#[allow(clippy::too_many_lines)]
async fn handle_pair_session(
    pipe: &mut NamedPipeServer,
    control: &IpcControl,
    pairing: &PairingService,
    request: IpcRequest,
) -> io::Result<()> {
    let request_id = request.request_id.clone();
    if request.v != 1
        || !is_request_id(&request.request_id)
        || !is_empty_pair_request(request.payload.as_ref())
    {
        return write_response(pipe, IpcResponse::error(request_id, "invalid_request")).await;
    }
    if !control.is_ready() {
        return write_response(pipe, IpcResponse::error(request_id, "node_stopping")).await;
    }
    let (ticket, mut events) = match pairing.create_pairing().await {
        Ok(pairing) => pairing,
        Err(error) => {
            return write_response(
                pipe,
                IpcResponse::error(request_id, pairing_error_code(&error)),
            )
            .await;
        }
    };
    let status = control.status();
    let started = PairingStart {
        pairing_id: ticket.pairing_id.to_base64url(),
        node_id: status.node_id,
        origin: status.origin,
        code: format_pairing_code(&ticket.code),
        expires_at: ticket.expires_at,
    };
    let started = serde_json::to_value(started)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "pairing start encoding failed"))?;
    write_response(pipe, IpcResponse::result(request_id.clone(), Ok(started))).await?;

    let _initial = events.borrow_and_update().clone();
    let deadline = TokioInstant::from_std(ticket.deadline);
    let mut stop = control.stop_receiver();
    let pending = loop {
        let current = events.borrow_and_update().clone();
        if current.event != "started" {
            break current;
        }
        tokio::select! {
            () = sleep_until(deadline) => {
                let _ = pairing.expire_if_due(ticket.pairing_id).await;
                let event = events.borrow_and_update().clone();
                let _ = write_pairing_event(pipe, request_id, event).await;
                return Ok(());
            }
            changed = events.changed() => {
                if changed.is_err() {
                    return Ok(());
                }
            }
            incoming = read_frame(pipe) => {
                let _ = incoming;
                let _ = pairing.cancel(ticket.pairing_id).await;
                return Ok(());
            }
            changed = stop.changed() => {
                if changed.is_err() {
                    return Ok(());
                }
                let _ = pairing.cancel(ticket.pairing_id).await;
                let event = events.borrow_and_update().clone();
                let _ = write_pairing_event(pipe, request_id, event).await;
                return Ok(());
            }
        }
    };
    if pending.event != "pending_confirmation" {
        write_pairing_event(pipe, request_id, pending).await?;
        return Ok(());
    }
    write_pairing_event(pipe, request_id.clone(), pending).await?;

    if *stop.borrow() {
        let _ = pairing.cancel(ticket.pairing_id).await;
        let event = events.borrow_and_update().clone();
        let _ = write_pairing_event(pipe, request_id, event).await;
        return Ok(());
    }
    let decision_frame = tokio::select! {
        () = sleep_until(deadline) => {
            let _ = pairing.expire_if_due(ticket.pairing_id).await;
            let event = events.borrow_and_update().clone();
            let _ = write_pairing_event(pipe, request_id, event).await;
            return Ok(());
        }
        changed = stop.changed() => {
            if changed.is_ok() {
                let _ = pairing.cancel(ticket.pairing_id).await;
                let event = events.borrow_and_update().clone();
                let _ = write_pairing_event(pipe, request_id, event).await;
            }
            return Ok(());
        }
        result = read_frame(pipe) => {
            let Ok(frame) = result else {
                let _ = pairing.cancel(ticket.pairing_id).await;
                return Ok(());
            };
            frame
        }
    };
    let decision: PairDecision = match serde_json::from_slice::<PairDecision>(&decision_frame) {
        Ok(decision)
            if decision.v == 1
                && decision.request_id == request_id
                && decision.payload.is_none() =>
        {
            decision
        }
        _ => {
            let _ = pairing.cancel(ticket.pairing_id).await;
            return write_response(pipe, IpcResponse::error(request_id, "invalid_request")).await;
        }
    };

    match decision.decision {
        PairDecisionValue::Approve => match pairing.approve(ticket.pairing_id).await {
            Ok(_) => {
                let event = events.borrow_and_update().clone();
                write_pairing_event(pipe, request_id, event).await
            }
            Err(error) => {
                if !matches!(error, PairingError::AlreadyApproved) {
                    let _ = pairing.cancel(ticket.pairing_id).await;
                }
                write_response(
                    pipe,
                    IpcResponse::error(request_id, pairing_error_code(&error)),
                )
                .await
            }
        },
        PairDecisionValue::Reject => match pairing.reject(ticket.pairing_id).await {
            Ok(()) => {
                let event = events.borrow_and_update().clone();
                write_pairing_event(pipe, request_id, event).await
            }
            Err(error) => {
                write_response(
                    pipe,
                    IpcResponse::error(request_id, pairing_error_code(&error)),
                )
                .await
            }
        },
    }
}

fn is_empty_pair_request(payload: Option<&serde_json::Value>) -> bool {
    match payload {
        None | Some(serde_json::Value::Null) => true,
        Some(serde_json::Value::Object(fields)) => fields.is_empty(),
        Some(_) => false,
    }
}

async fn write_pairing_event(
    pipe: &mut NamedPipeServer,
    request_id: String,
    event: PairingEvent,
) -> io::Result<()> {
    let event = serde_json::to_value(event)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "pairing event encoding failed"))?;
    write_response(pipe, IpcResponse::result(request_id, Ok(event))).await
}

async fn write_response(pipe: &mut NamedPipeServer, response: IpcResponse) -> io::Result<()> {
    let encoded = serde_json::to_vec(&response)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "response encoding failed"))?;
    if encoded.len() > MAX_IPC_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "IPC response exceeds the message budget",
        ));
    }
    pipe.write_all(&encoded).await?;
    pipe.write_all(b"\n").await?;
    pipe.flush().await
}

async fn read_frame(reader: &mut (impl AsyncRead + Unpin)) -> io::Result<Vec<u8>> {
    let mut frame = Vec::with_capacity(256);
    let mut chunk = [0_u8; 1024];
    loop {
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "IPC request is incomplete",
            ));
        }
        if let Some(end) = chunk[..read].iter().position(|byte| *byte == b'\n') {
            if frame.len() + end > MAX_IPC_MESSAGE {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "IPC request exceeds the message budget",
                ));
            }
            frame.extend_from_slice(&chunk[..end]);
            return Ok(frame);
        }
        if frame.len() + read > MAX_IPC_MESSAGE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "IPC request exceeds the message budget",
            ));
        }
        frame.extend_from_slice(&chunk[..read]);
    }
}

/// Reads the running node status through the owner-only local channel.
///
/// # Errors
///
/// Returns an error when the node is unavailable or rejects the status request.
pub async fn status(address: &LocalIpcAddress) -> Result<NodeStatus, AppError> {
    let response = request(address, "status", None).await?;
    response.into_status()
}

/// Requests a graceful stop and waits for the node to report that resources were released.
///
/// # Errors
///
/// Returns an error when the node is unavailable, shutdown times out, or shutdown reports failure.
pub async fn stop(address: &LocalIpcAddress) -> Result<NodeStatus, AppError> {
    let response = request(address, "stop", None).await?;
    response.into_status()
}

/// Reads one bounded page of registered clients through the owner-only local channel.
///
/// # Errors
///
/// Returns an error when the node is unavailable, rejects the request, or returns a stale cursor.
pub async fn device_list(
    address: &LocalIpcAddress,
    cursor: Option<&str>,
) -> Result<ClientPage, AppError> {
    let payload = serde_json::json!({ "cursor": cursor });
    let response = request(address, "device.list", Some(payload)).await?;
    response.into_result("client page")
}

/// Revokes one registered client through the owner-only local channel.
///
/// # Errors
///
/// Returns an error when the node is unavailable, the client does not exist, the request
/// identifier conflicts, or the revocation transaction cannot commit.
pub async fn device_revoke(
    address: &LocalIpcAddress,
    client_id: &str,
) -> Result<RevokeResult, AppError> {
    let payload = serde_json::json!({ "clientId": client_id });
    let response = request(address, "device.revoke", Some(payload)).await?;
    response.into_result("revoke result")
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WorkspaceIpcRecord {
    pub workspace_id: String,
    pub canonical_path: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SourceIpcSummary {
    pub source_id: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HarnessIpcSummary {
    pub harness_id: String,
    pub name: String,
    pub source_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProfileIpcSummary {
    pub profile_id: String,
    pub revision: String,
    pub harness_id: String,
    pub source_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TargetBindingIpcSummary {
    pub profile_id: String,
    pub profile_revision: String,
    pub workspace_id: String,
    pub launch_path: String,
    pub observed_canonical_path: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProductTargetIpcRecord {
    pub target_id: String,
    pub source_id: String,
    pub native_session_id: String,
    pub workspace_id: Option<String>,
    pub binding: Option<TargetBindingIpcSummary>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CatalogInventoryIpc {
    pub workspaces: Vec<WorkspaceIpcRecord>,
    pub sources: Vec<SourceIpcSummary>,
    pub harnesses: Vec<HarnessIpcSummary>,
    pub profiles: Vec<ProfileIpcSummary>,
    pub targets: Vec<ProductTargetIpcRecord>,
}

/// Registers an existing absolute directory on disk as a workspace through the owner-only local channel.
///
/// # Errors
///
/// Returns an error when the node is unavailable, the directory is invalid, or the workspace cannot be committed.
pub async fn workspace_register(
    address: &LocalIpcAddress,
    path: &str,
) -> Result<WorkspaceIpcRecord, AppError> {
    let payload = serde_json::json!({ "path": path });
    let response = request(address, "workspace.register", Some(payload)).await?;
    response.into_result("workspace record")
}

/// Registers a product target for an existing source and workspace through the owner-only local channel.
///
/// # Errors
///
/// Returns an error when the node is unavailable, source/workspace is invalid, or the target conflicts.
pub async fn target_register(
    address: &LocalIpcAddress,
    source_id: &str,
    native_session_id: &str,
    workspace_id: &str,
) -> Result<ProductTargetIpcRecord, AppError> {
    let payload = serde_json::json!({
        "sourceId": source_id,
        "nativeSessionId": native_session_id,
        "workspaceId": workspace_id,
    });
    let response = request(address, "target.register", Some(payload)).await?;
    response.into_result("target record")
}

/// Reads safe catalog metadata (workspaces, sources, harnesses, profiles, targets) through the owner-only local channel.
///
/// # Errors
///
/// Returns an error when the node is unavailable or catalog metadata cannot be read.
pub async fn catalog_inventory(address: &LocalIpcAddress) -> Result<CatalogInventoryIpc, AppError> {
    let response = request(address, "catalog.inventory", None).await?;
    response.into_result("catalog inventory")
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PairingStart {
    pub pairing_id: String,
    pub node_id: String,
    pub origin: String,
    pub code: String,
    pub expires_at: u64,
}

pub struct PairingSession {
    client: NamedPipeClient,
    request_id: String,
    deadline: Instant,
}

impl PairingSession {
    /// Waits for the next event in the bounded owner pairing session.
    ///
    /// # Errors
    ///
    /// Returns an error when the node closes the session, reports a pairing error, or the
    /// 300-second session deadline elapses.
    pub async fn next_event(&mut self) -> Result<PairingEvent, AppError> {
        let frame = timeout_at(
            TokioInstant::from_std(self.deadline),
            read_frame(&mut self.client),
        )
        .await
        .map_err(|_| AppError::Ipc("pairing session timed out".to_owned()))?
        .map_err(|error| AppError::Ipc(format!("pairing session failed: {error}")))?;
        let response: IpcResponse = serde_json::from_slice(&frame)
            .map_err(|_| AppError::Ipc("pairing response is malformed".to_owned()))?;
        if response.v != 1 || response.request_id != self.request_id {
            return Err(AppError::Ipc(
                "pairing response does not match the request".to_owned(),
            ));
        }
        response.into_result("pairing event")
    }

    /// Submits an explicit approval decision without waiting for the persisted result.
    ///
    /// # Errors
    ///
    /// Returns an error when the decision cannot be written to the owner session.
    pub async fn approve(&mut self) -> Result<(), AppError> {
        self.send_decision(PairDecisionValue::Approve).await
    }

    /// Submits a rejection decision without creating trust.
    ///
    /// # Errors
    ///
    /// Returns an error when the decision cannot be written to the owner session.
    pub async fn reject(&mut self) -> Result<(), AppError> {
        self.send_decision(PairDecisionValue::Reject).await
    }

    async fn send_decision(&mut self, decision: PairDecisionValue) -> Result<(), AppError> {
        let request = PairDecision {
            v: 1,
            request_id: self.request_id.clone(),
            decision,
            payload: None,
        };
        let encoded = serde_json::to_vec(&request)
            .map_err(|_| AppError::Ipc("cannot encode the pairing decision".to_owned()))?;
        if encoded.len() > MAX_IPC_MESSAGE {
            return Err(AppError::Ipc(
                "pairing decision exceeds the message budget".to_owned(),
            ));
        }
        timeout_at(TokioInstant::from_std(self.deadline), async {
            self.client.write_all(&encoded).await?;
            self.client.write_all(b"\n").await?;
            self.client.flush().await
        })
        .await
        .map_err(|_| AppError::Ipc("pairing session timed out".to_owned()))?
        .map_err(|error| AppError::Ipc(format!("sending the pairing decision failed: {error}")))
    }
}

/// Opens an owner-only pairing session and returns its short-lived browser code.
///
/// # Errors
///
/// Returns an error when the node is unavailable, rejects pairing, or returns a malformed start
/// response.
pub async fn begin_pairing(
    address: &LocalIpcAddress,
) -> Result<(PairingSession, PairingStart), AppError> {
    let mut client = ClientOptions::new()
        .open(address.pipe_name.as_str())
        .map_err(|error| {
            AppError::Ipc(format!(
                "node is not running or its owner control channel is unavailable: {error}"
            ))
        })?;
    let request_id = Id128::new(random::bytes::<16>().map_err(AppError::Platform)?).to_base64url();
    let request = IpcRequest {
        v: 1,
        request_id: request_id.clone(),
        command: "device.pair".to_owned(),
        payload: None,
    };
    let encoded = serde_json::to_vec(&request)
        .map_err(|_| AppError::Ipc("cannot encode the pairing request".to_owned()))?;
    let exchange = async {
        client.write_all(&encoded).await?;
        client.write_all(b"\n").await?;
        client.flush().await?;
        read_frame(&mut client).await
    };
    let frame = timeout(IPC_REQUEST_TIMEOUT, exchange)
        .await
        .map_err(|_| AppError::Ipc("pairing request timed out".to_owned()))?
        .map_err(|error| AppError::Ipc(format!("pairing request failed: {error}")))?;
    let response: IpcResponse = serde_json::from_slice(&frame)
        .map_err(|_| AppError::Ipc("pairing response is malformed".to_owned()))?;
    if response.v != 1 || response.request_id != request_id {
        return Err(AppError::Ipc(
            "pairing response does not match the request".to_owned(),
        ));
    }
    let start = response.into_result("pairing start")?;
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(PAIRING_LIFETIME_SECONDS))
        .ok_or_else(|| AppError::Ipc("pairing clock is unavailable".to_owned()))?;
    Ok((
        PairingSession {
            client,
            request_id,
            deadline,
        },
        start,
    ))
}

async fn request(
    address: &LocalIpcAddress,
    command: &str,
    payload: Option<serde_json::Value>,
) -> Result<IpcResponse, AppError> {
    let mut client = ClientOptions::new()
        .open(address.pipe_name.as_str())
        .map_err(|error| {
            AppError::Ipc(format!(
                "node is not running or its owner control channel is unavailable: {error}"
            ))
        })?;

    let request_id = Id128::new(random::bytes::<16>().map_err(AppError::Platform)?).to_base64url();
    let request = IpcRequest {
        v: 1,
        request_id: request_id.clone(),
        command: command.to_owned(),
        payload,
    };
    let encoded = serde_json::to_vec(&request)
        .map_err(|_| AppError::Ipc("cannot encode the local request".to_owned()))?;
    if encoded.len() > MAX_IPC_MESSAGE {
        return Err(AppError::Ipc(
            "local request exceeds the message budget".to_owned(),
        ));
    }
    let exchange = async {
        client.write_all(&encoded).await?;
        client.write_all(b"\n").await?;
        client.flush().await?;
        read_frame(&mut client).await
    };
    let frame = timeout(IPC_REQUEST_TIMEOUT, exchange)
        .await
        .map_err(|_| AppError::Ipc("local request timed out".to_owned()))?
        .map_err(|error| AppError::Ipc(format!("local request failed: {error}")))?;
    let response: IpcResponse = serde_json::from_slice(&frame)
        .map_err(|_| AppError::Ipc("local response is malformed".to_owned()))?;
    if response.v != 1 || response.request_id != request_id {
        return Err(AppError::Ipc(
            "local response does not match the request".to_owned(),
        ));
    }
    Ok(response)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct IpcRequest {
    v: u8,
    request_id: String,
    command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    payload: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct IpcResponse {
    v: u8,
    request_id: String,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<NodeStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl IpcResponse {
    const fn status(request_id: String, status: NodeStatus) -> Self {
        Self {
            v: 1,
            request_id,
            ok: true,
            status: Some(status),
            result: None,
            error: None,
        }
    }

    fn result(request_id: String, result: Result<serde_json::Value, AppError>) -> Self {
        match result {
            Ok(result) => Self {
                v: 1,
                request_id,
                ok: true,
                status: None,
                result: Some(result),
                error: None,
            },
            Err(error) => Self::error(request_id, error.to_string()),
        }
    }

    fn error(request_id: String, error: impl Into<String>) -> Self {
        Self {
            v: 1,
            request_id,
            ok: false,
            status: None,
            result: None,
            error: Some(error.into()),
        }
    }

    fn into_status(self) -> Result<NodeStatus, AppError> {
        if self.ok {
            self.status
                .ok_or_else(|| AppError::Ipc("local response omitted node status".to_owned()))
        } else {
            Err(AppError::Ipc(
                self.error
                    .unwrap_or_else(|| "local request failed".to_owned()),
            ))
        }
    }

    fn into_result<T: serde::de::DeserializeOwned>(self, label: &str) -> Result<T, AppError> {
        if !self.ok {
            return Err(AppError::Ipc(
                self.error
                    .unwrap_or_else(|| "local request failed".to_owned()),
            ));
        }
        let result = self
            .result
            .ok_or_else(|| AppError::Ipc(format!("local response omitted {label}")))?;
        serde_json::from_value(result)
            .map_err(|_| AppError::Ipc(format!("local response contains an invalid {label}")))
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ListRequest {
    cursor: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RevokeRequest {
    client_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct WorkspaceRegisterRequest {
    path: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TargetRegisterRequest {
    source_id: String,
    native_session_id: String,
    workspace_id: String,
}

fn workspace_ipc_record(record: &WorkspaceRecord) -> WorkspaceIpcRecord {
    WorkspaceIpcRecord {
        workspace_id: record.workspace_id.encode(),
        canonical_path: record.canonical_path.clone(),
    }
}

fn product_target_ipc_record(record: &ProductTargetRecord) -> ProductTargetIpcRecord {
    ProductTargetIpcRecord {
        target_id: record.target_id.encode(),
        source_id: record.source_id.encode(),
        native_session_id: record.native_session_id.clone(),
        workspace_id: record.workspace_id.map(super::domain::EntityId::encode),
        binding: record.binding.as_ref().map(|b| TargetBindingIpcSummary {
            profile_id: b.profile_id.encode(),
            profile_revision: b.profile_revision.to_string(),
            workspace_id: b.workspace_id.encode(),
            launch_path: b.launch_path.clone(),
            observed_canonical_path: b.observed_canonical_path.clone(),
        }),
    }
}

fn catalog_inventory_ipc(inventory: &CatalogInventory) -> CatalogInventoryIpc {
    CatalogInventoryIpc {
        workspaces: inventory
            .workspaces
            .iter()
            .map(workspace_ipc_record)
            .collect(),
        sources: inventory
            .sources
            .iter()
            .map(|s| SourceIpcSummary {
                source_id: s.source_id.encode(),
                name: s.name.clone(),
            })
            .collect(),
        harnesses: inventory
            .harnesses
            .iter()
            .map(|h| HarnessIpcSummary {
                harness_id: h.harness_id.encode(),
                name: h.name.clone(),
                source_id: h.source_id.encode(),
            })
            .collect(),
        profiles: inventory
            .profiles
            .iter()
            .map(|p| ProfileIpcSummary {
                profile_id: p.profile_id.encode(),
                revision: p.revision.to_string(),
                harness_id: p.harness_id.encode(),
                source_id: p.source_id.encode(),
            })
            .collect(),
        targets: inventory
            .targets
            .iter()
            .map(product_target_ipc_record)
            .collect(),
    }
}

const fn catalog_failure_code(error: &CatalogFailure) -> &'static str {
    match error.kind() {
        CatalogError::InvalidActor => "revoked",
        CatalogError::InvalidSource => "invalid_source",
        CatalogError::InvalidTarget => "invalid_target",
        CatalogError::UnknownRequest => "unknown_request",
        CatalogError::InvalidConfiguration => "invalid_configuration",
        CatalogError::Conflict => "request_conflict",
        CatalogError::CorruptState | CatalogError::Cancelled => "read_failed",
        CatalogError::RandomUnavailable => "random_unavailable",
        CatalogError::Allocation => "allocation_failed",
        CatalogError::ProtectionUnavailable => "protection_unavailable",
        CatalogError::StorageUnavailable => "necessary_storage_unavailable",
        CatalogError::NodeStopping => "node_stopping",
        CatalogError::CommitUnconfirmed | CatalogError::SqlCloseUnconfirmed { .. } => {
            "commit_unconfirmed"
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PairDecision {
    v: u8,
    request_id: String,
    decision: PairDecisionValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    payload: Option<serde_json::Value>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum PairDecisionValue {
    Approve,
    Reject,
}

#[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RevokeErrorObservation {
    NotFound,
    InvalidRequest,
    RequestConflict,
    PublicKeyAlreadyRegistered,
    ClientIdCollision,
    StaleCursor,
    StorageUnavailable,
    CapacityExhausted,
    Storage,
    PoolClosed,
    PoolTimedOut,
    DatabaseIo,
    SqliteBusy,
    SqliteBusySnapshot,
    SqliteLocked,
    DatabaseOtherCode,
    DatabaseOther,
}

const fn trust_error_code(error: &TrustError) -> &'static str {
    match error {
        TrustError::NotFound => "not_found",
        TrustError::InvalidRequest => "invalid_request",
        TrustError::RequestConflict | TrustError::PublicKeyAlreadyRegistered => "request_conflict",
        TrustError::StaleCursor => "stale_cursor",
        TrustError::CapacityExhausted
        | TrustError::ClientIdCollision
        | TrustError::StorageUnavailable
        | TrustError::Database(_)
        | TrustError::Storage(_) => "storage_unavailable",
    }
}

const fn pairing_error_code(error: &PairingError) -> &'static str {
    match error {
        PairingError::PairingInvalid | PairingError::Jws(_) | PairingError::Challenge(_) => {
            "pairing_invalid"
        }
        PairingError::RequestConflict => "request_conflict",
        PairingError::CapacityExhausted => "capacity_exhausted",
        PairingError::RandomUnavailable | PairingError::ClockUnavailable => "node_unavailable",
        PairingError::StorageUnavailable
        | PairingError::Trust(
            TrustError::StorageUnavailable
            | TrustError::Database(_)
            | TrustError::Storage(_)
            | TrustError::CapacityExhausted
            | TrustError::ClientIdCollision,
        ) => "storage_unavailable",
        PairingError::AlreadyApproved => "already_approved",
        PairingError::Trust(error) => trust_error_code(error),
    }
}

fn is_request_id(value: &str) -> bool {
    decode_fixed_base64url(value, 16).is_ok()
}

#[cfg(test)]
mod tests {
    use std::{
        io::Cursor,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    fn test_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "acp-remote-ipc-{label}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("test directory");
        path
    }

    #[test]
    fn terminal_report_preserves_first_failure() {
        let (control, _) = IpcControl::new("terminal-node".into(), "https://localhost".into());
        control.complete(ShutdownResult::Failed("close deadline".into()));
        control.complete(ShutdownResult::Completed);
        assert_eq!(
            *control.shutdown_receiver().borrow(),
            ShutdownResult::Failed("close deadline".into())
        );
        assert_eq!(control.status().state, "failed");
    }

    #[test]
    fn terminal_report_preserves_first_failure_text() {
        let (control, _) = IpcControl::new("terminal-node".into(), "https://localhost".into());
        control.complete(ShutdownResult::Failed("original failure".into()));
        control.complete(ShutdownResult::Failed("later failure".into()));
        control.complete(ShutdownResult::Pending);
        assert_eq!(
            *control.shutdown_receiver().borrow(),
            ShutdownResult::Failed("original failure".into())
        );
        assert_eq!(control.status().state, "failed");
    }

    #[test]
    fn terminal_report_preserves_completed() {
        let (control, _) = IpcControl::new("terminal-node".into(), "https://localhost".into());
        control.complete(ShutdownResult::Completed);
        control.complete(ShutdownResult::Pending);
        control.complete(ShutdownResult::Failed("late report".into()));
        assert_eq!(
            *control.shutdown_receiver().borrow(),
            ShutdownResult::Completed
        );
        assert_eq!(control.status().state, "stopped");
    }

    #[test]
    fn terminal_report_pending_progress_can_complete() {
        let (control, _) = IpcControl::new("terminal-node".into(), "https://localhost".into());
        control.complete(ShutdownResult::Pending);
        assert_eq!(
            *control.shutdown_receiver().borrow(),
            ShutdownResult::Pending
        );
        assert_eq!(control.status().state, "stopping");
        control.complete(ShutdownResult::Completed);
        assert_eq!(
            *control.shutdown_receiver().borrow(),
            ShutdownResult::Completed
        );
        assert_eq!(control.status().state, "stopped");
    }

    #[test]
    fn terminal_report_notifies_existing_and_late_subscribers_once() {
        let (control, _) = IpcControl::new("terminal-node".into(), "https://localhost".into());
        let mut existing = control.shutdown_receiver();
        let first = ShutdownResult::Failed("original close timeout".into());
        control.complete(first.clone());
        assert!(existing.has_changed().expect("live report sender"));
        assert_eq!(*existing.borrow_and_update(), first);
        control.complete(ShutdownResult::Completed);
        control.complete(ShutdownResult::Pending);
        assert!(!existing.has_changed().expect("live report sender"));
        assert_eq!(*control.shutdown_receiver().borrow(), first);
        assert_eq!(control.status().state, "failed");
    }

    #[test]
    fn terminal_report_concurrent_publishers_share_one_terminal() {
        let (control, _) = IpcControl::new("terminal-node".into(), "https://localhost".into());
        let ready = Arc::new(std::sync::Barrier::new(3));
        let completed_control = control.clone();
        let completed_ready = Arc::clone(&ready);
        let completed = std::thread::spawn(move || {
            completed_ready.wait();
            completed_control.complete(ShutdownResult::Completed);
        });
        let failed_control = control.clone();
        let failed_ready = Arc::clone(&ready);
        let failed = std::thread::spawn(move || {
            failed_ready.wait();
            failed_control.complete(ShutdownResult::Failed("concurrent close timeout".into()));
        });
        ready.wait();
        completed.join().expect("completed publisher joined");
        failed.join().expect("failed publisher joined");
        let published = control.shutdown_receiver().borrow().clone();
        let expected_phase = match &published {
            ShutdownResult::Completed => "stopped",
            ShutdownResult::Failed(message) => {
                assert_eq!(message, "concurrent close timeout");
                "failed"
            }
            ShutdownResult::Pending => panic!("both terminal publishers already joined"),
        };
        assert_eq!(control.status().state, expected_phase);
        control.complete(ShutdownResult::Pending);
        control.complete(ShutdownResult::Completed);
        control.complete(ShutdownResult::Failed("replacement".into()));
        assert_eq!(*control.shutdown_receiver().borrow(), published);
        assert_eq!(control.status().state, expected_phase);
    }

    #[test]
    fn address_is_stable_for_the_same_data_directory() {
        let first = test_directory("first");
        let second = test_directory("second");
        let first_address = LocalIpcAddress::for_data_directory(&first).expect("first address");
        let repeated_address =
            LocalIpcAddress::for_data_directory(&first).expect("repeated address");
        let second_address = LocalIpcAddress::for_data_directory(&second).expect("second address");

        assert_eq!(first_address, repeated_address);
        assert_ne!(first_address.pipe_name, second_address.pipe_name);

        std::fs::remove_dir(first).expect("first cleanup");
        std::fs::remove_dir(second).expect("second cleanup");
    }

    #[tokio::test]
    async fn stop_response_write_failure_is_reported() {
        let directory = test_directory("stop-write-failure");
        let address = LocalIpcAddress::for_data_directory(&directory).expect("address");
        let server = address.bind_first().expect("server");
        let client = ClientOptions::new()
            .open(&address.pipe_name)
            .expect("client");
        server.connect().await.expect("connected pipe");
        drop(client);
        let (control, _) = IpcControl::new("test-node".to_owned(), "https://localhost".to_owned());
        control
            .spawn_stop_response(server, "test-request".to_owned())
            .await;
        control.complete(ShutdownResult::Completed);
        let error = control
            .finish_stop_responses(TokioInstant::now() + Duration::from_secs(2))
            .await
            .expect_err("writing to a disconnected pipe must fail");
        assert!(
            error.contains("delivering the owner stop response failed"),
            "{error}"
        );
        std::fs::remove_dir(directory).expect("cleanup");
    }

    #[tokio::test]
    async fn stop_response_failure_does_not_cancel_other_clients() {
        let broken_directory = test_directory("stop-broken");
        let healthy_directory = test_directory("stop-healthy");
        let broken_address =
            LocalIpcAddress::for_data_directory(&broken_directory).expect("address");
        let healthy_address =
            LocalIpcAddress::for_data_directory(&healthy_directory).expect("address");
        let broken_server = broken_address.bind_first().expect("broken server");
        let healthy_server = healthy_address.bind_first().expect("healthy server");
        let broken_client = ClientOptions::new()
            .open(&broken_address.pipe_name)
            .expect("client");
        let mut healthy_client = ClientOptions::new()
            .open(&healthy_address.pipe_name)
            .expect("client");
        broken_server.connect().await.expect("broken connection");
        healthy_server.connect().await.expect("healthy connection");
        drop(broken_client);
        let (control, _) = IpcControl::new("test-node".to_owned(), "https://localhost".to_owned());
        control
            .spawn_stop_response(broken_server, "broken".to_owned())
            .await;
        control
            .spawn_stop_response(healthy_server, "healthy".to_owned())
            .await;
        control.complete(ShutdownResult::Completed);
        let result = control
            .finish_stop_responses(TokioInstant::now() + Duration::from_secs(2))
            .await;
        assert!(result.is_err(), "broken response must remain visible");
        let frame = timeout(Duration::from_secs(1), read_frame(&mut healthy_client))
            .await
            .expect("healthy response deadline")
            .expect("healthy response");
        let response: IpcResponse = serde_json::from_slice(&frame).expect("response JSON");
        assert!(response.ok);
        assert_eq!(response.request_id, "healthy");
        assert_eq!(response.status.expect("cleanup status").state, "stopped");
        assert_eq!(
            *control.shutdown_receiver().borrow(),
            ShutdownResult::Completed
        );
        std::fs::remove_dir(broken_directory).expect("broken cleanup");
        std::fs::remove_dir(healthy_directory).expect("healthy cleanup");
    }

    #[tokio::test]
    async fn exhausted_os_pipe_instances_wait_for_release_and_remain_cancellable() {
        let directory = test_directory("pipe-instance-backpressure");
        let address = LocalIpcAddress::for_data_directory(&directory).expect("address");
        let mut instances = vec![address.bind_first().expect("first instance")];
        for _ in 1..254 {
            instances.push(address.bind_next().expect("OS instance"));
        }
        assert_eq!(
            address
                .bind_next()
                .expect_err("finite OS ceiling")
                .raw_os_error(),
            Some(231)
        );
        let (control, _) = IpcControl::new("test-node".into(), "https://localhost".into());
        let owned_address = address.clone();
        let owned_control = control.clone();
        let waiting =
            tokio::spawn(async move { bind_next_available(&owned_address, &owned_control).await });
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(
            !waiting.is_finished(),
            "capacity exhaustion must not fail the node"
        );
        drop(instances.pop());
        let recovered = timeout(Duration::from_secs(2), waiting)
            .await
            .expect("capacity recovers")
            .expect("task")
            .expect("no fatal error")
            .expect("new listener");
        let owned_address = address.clone();
        let owned_control = control.clone();
        let waiting =
            tokio::spawn(async move { bind_next_available(&owned_address, &owned_control).await });
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(!waiting.is_finished(), "native ceiling filled again");
        control.request_stop();
        assert!(
            timeout(Duration::from_secs(2), waiting)
                .await
                .expect("stop cancels wait")
                .expect("task")
                .expect("no fatal error")
                .is_none()
        );
        drop(recovered);
        drop(instances);
        std::fs::remove_dir(directory).expect("cleanup owned pipe directory");
    }

    #[tokio::test]
    async fn frame_reader_enforces_the_message_budget() {
        let mut within_budget = Cursor::new(vec![b'a'; MAX_IPC_MESSAGE - 1]);
        within_budget.get_mut().push(b'\n');
        let frame = read_frame(&mut within_budget).await.expect("bounded frame");
        assert_eq!(frame.len(), MAX_IPC_MESSAGE - 1);

        let mut over_budget = Cursor::new(vec![b'a'; MAX_IPC_MESSAGE + 1]);
        over_budget.get_mut().push(b'\n');
        assert!(read_frame(&mut over_budget).await.is_err());
    }
}
