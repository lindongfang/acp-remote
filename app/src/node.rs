use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{Extension, Router};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder,
    service::TowerToHyperService,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{Semaphore, watch},
    task::{JoinHandle, JoinSet},
    time::{Instant as TokioInstant, sleep, timeout, timeout_at},
};
use tokio_rustls::TlsAcceptor;

use crate::{
    access_policy::ApplicationPolicy,
    config::NodeConfig,
    data_dir::DataDirectory,
    error::{AppError, StorageError},
    identity::{self, NodeIdentity},
    ipc::{self, IpcControl, LocalIpcAddress, ShutdownResult},
    management_ws::ManagementService,
    pairing::PairingService,
    pairing_http::{self, PeerAddress},
    platform::SecretProtector,
    storage::Storage,
    tls,
};

const MAX_CONNECTIONS: usize = 128;
const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECTION_DRAIN_TIMEOUT: Duration = Duration::from_secs(10);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(15);
const SHUTDOWN_RESPONSE_TIMEOUT: Duration = Duration::from_secs(1);

pub struct NodeRuntime {
    control: IpcControl,
    network_stop: watch::Sender<bool>,
    network_task: Option<JoinHandle<Result<(), String>>>,
    ipc_task: Option<JoinHandle<Result<(), AppError>>>,
    storage: Option<Storage>,
    identity: Option<Arc<NodeIdentity>>,
    pairing: Option<Arc<PairingService>>,
    management: Option<ManagementService>,
    application_policy: Option<Arc<ApplicationPolicy>>,
    data_dir: Option<DataDirectory>,
    listen: SocketAddr,
    ipc_address: LocalIpcAddress,
}

impl NodeRuntime {
    /// Acquires all node resources and starts the HTTPS and owner control listeners.
    ///
    /// # Errors
    ///
    /// Returns an error before reporting readiness when configuration, directory ownership,
    /// storage, identity, TLS material, the listening socket, or local IPC cannot be initialized.
    #[allow(clippy::too_many_lines)]
    pub async fn start(
        config: NodeConfig,
        protector: &dyn SecretProtector,
    ) -> Result<Self, AppError> {
        let data_dir = DataDirectory::acquire(&config.data_dir)?;
        let storage = match Storage::open(&data_dir.database_path()).await {
            Ok(storage) => storage,
            Err(error) => return Err(AppError::Startup(storage_startup_reason(&error))),
        };
        if let Err(error) = data_dir.secure_managed_files() {
            storage.close().await;
            return Err(error);
        }
        let identity = match identity::initialize(data_dir.path(), &storage, protector).await {
            Ok(identity) => Arc::new(identity),
            Err(error) => {
                storage.close().await;
                return Err(error.into());
            }
        };
        let trust = storage.trust_store();
        let pairing = match PairingService::new(
            identity.clone(),
            config.canonical_origin.clone(),
            trust.clone(),
        ) {
            Ok(pairing) => Arc::new(pairing),
            Err(error) => {
                storage.close().await;
                return Err(error.into());
            }
        };
        let tls_config = match tls::load_server_config(&config.tls_cert, &config.tls_key) {
            Ok(config) => config,
            Err(error) => {
                storage.close().await;
                return Err(error);
            }
        };
        let listener = match TcpListener::bind(config.listen).await {
            Ok(listener) => listener,
            Err(source) => {
                storage.close().await;
                return Err(AppError::Io {
                    context: "binding the HTTPS listener",
                    source,
                });
            }
        };
        let ipc_address = match LocalIpcAddress::for_data_directory(data_dir.path()) {
            Ok(address) => address,
            Err(error) => {
                storage.close().await;
                return Err(error);
            }
        };
        let first_pipe = match ipc_address.bind_first() {
            Ok(pipe) => pipe,
            Err(error) => {
                storage.close().await;
                return Err(AppError::Ipc(format!(
                    "binding the owner control channel failed: {error}"
                )));
            }
        };

        let (control, _) = IpcControl::new(
            identity.node_id().to_owned(),
            config.canonical_origin.clone(),
        );
        let application_policy = Arc::new(ApplicationPolicy::new(
            identity.node_id().to_owned(),
            trust.clone(),
            &pairing,
            control.clone(),
        ));
        let management = match ManagementService::new(
            Arc::clone(&identity),
            config.canonical_origin.clone(),
            trust.clone(),
            Arc::clone(&pairing),
            control.clone(),
            Arc::clone(&application_policy),
        ) {
            Ok(management) => management,
            Err(error) => {
                storage.close().await;
                return Err(AppError::Startup(format!(
                    "management transport initialization failed: {error}"
                )));
            }
        };
        let (network_stop, network_stop_rx) = watch::channel(false);
        let router = pairing_http::router(
            Arc::clone(&pairing),
            config.canonical_origin.clone(),
            control.clone(),
        )
        .merge(management.router());
        let network_control = control.clone();
        let network_task = tokio::spawn(async move {
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
        });
        let ipc_control = control.clone();
        let ipc_address_for_task = ipc_address.clone();
        let ipc_pairing = Arc::clone(&pairing);
        let ipc_task = tokio::spawn(async move {
            let result = ipc::serve(
                first_pipe,
                ipc_address_for_task,
                ipc_control.clone(),
                trust,
                ipc_pairing,
            )
            .await;
            if let Err(error) = &result {
                ipc_control.mark_failed(error.to_string());
            }
            result
        });

        Ok(Self {
            control,
            network_stop,
            network_task: Some(network_task),
            ipc_task: Some(ipc_task),
            storage: Some(storage),
            identity: Some(identity),
            pairing: Some(pairing),
            management: Some(management),
            application_policy: Some(application_policy),
            data_dir: Some(data_dir),
            listen: config.listen,
            ipc_address,
        })
    }

    #[must_use]
    pub const fn listen_addr(&self) -> SocketAddr {
        self.listen
    }

    /// In-process application boundary; this adds no network or CLI endpoint.
    #[must_use]
    pub fn application_policy(&self) -> Option<Arc<ApplicationPolicy>> {
        self.application_policy.clone()
    }

    #[must_use]
    pub const fn ipc_address(&self) -> &LocalIpcAddress {
        &self.ipc_address
    }

    #[must_use]
    pub fn node_id(&self) -> &str {
        self.identity
            .as_ref()
            .map_or("", |identity| identity.node_id())
    }

    /// Runs until the owner requests a stop or the process receives Ctrl-C.
    ///
    /// # Errors
    ///
    /// Returns an error when signal handling, network shutdown, IPC shutdown, or resource cleanup
    /// fails.
    pub async fn run(self) -> Result<(), AppError> {
        let mut stop = self.control.stop_receiver();
        if *stop.borrow_and_update() {
            return self.shutdown(None).await;
        }
        let select_failure = tokio::select! {
            signal = tokio::signal::ctrl_c() => signal
                .err()
                .map(|error| format!("waiting for Ctrl-C failed: {error}")),
            changed = stop.changed() => changed
                .err()
                .map(|error| format!("waiting for the owner stop request failed: {error}")),
        };
        self.shutdown(select_failure).await
    }

    /// Waits for an owner stop request without installing a Ctrl-C handler.
    ///
    /// # Errors
    ///
    /// Returns an error when shutdown or resource cleanup fails.
    pub async fn wait_for_stop(self) -> Result<(), AppError> {
        let mut stop = self.control.stop_receiver();
        if *stop.borrow_and_update() {
            return self.shutdown(None).await;
        }
        let changed = stop.changed().await;
        let select_failure = changed
            .err()
            .map(|error| format!("waiting for the owner stop request failed: {error}"));
        self.shutdown(select_failure).await
    }

    async fn shutdown(self, initial_failure: Option<String>) -> Result<(), AppError> {
        self.shutdown_with_deadline(initial_failure, SHUTDOWN_TIMEOUT)
            .await
    }

    #[allow(clippy::too_many_lines)]
    async fn shutdown_with_deadline(
        mut self,
        initial_failure: Option<String>,
        shutdown_timeout: Duration,
    ) -> Result<(), AppError> {
        let started = TokioInstant::now();
        let response_timeout = SHUTDOWN_RESPONSE_TIMEOUT.min(shutdown_timeout);
        let deadline = started + shutdown_timeout.saturating_sub(response_timeout);
        let response_deadline = started + shutdown_timeout;
        let mut failures = Vec::new();
        if let Some(failure) = initial_failure {
            failures.push(failure);
        }
        if let Some(failure) = self.control.failure() {
            failures.push(failure);
        }
        self.control.request_stop();
        if let Some(management) = &self.management {
            management.begin_shutdown();
        }
        let _ = self.network_stop.send(true);
        if let Some(pairing) = &self.pairing
            && timeout_at(deadline, pairing.clear()).await.is_err()
        {
            failures.push(format!(
                "temporary pairing state did not clear within {} seconds",
                shutdown_timeout.as_secs()
            ));
        }

        if let Some(mut task) = self.network_task.take() {
            match timeout_at(deadline, &mut task).await {
                Ok(Ok(Ok(()))) => {}
                Ok(Ok(Err(error))) => failures.push(error),
                Ok(Err(error)) => failures.push(format!("network task failed: {error}")),
                Err(_) => {
                    task.abort();
                    let _ = task.await;
                    failures.push(format!(
                        "shutdown exceeded {} seconds while stopping the network listener and connections",
                        shutdown_timeout.as_secs()
                    ));
                }
            }
        }
        if let Some(management) = &self.management {
            match timeout_at(deadline, management.wait_closed()).await {
                Ok(()) => {}
                Err(_) => failures.push(format!(
                    "shutdown exceeded {} seconds while closing management connections",
                    shutdown_timeout.as_secs()
                )),
            }
        }
        drop(self.management.take());
        drop(self.application_policy.take());
        drop(self.pairing.take());
        if let Some(storage) = self.storage.take()
            && timeout_at(deadline, storage.close()).await.is_err()
        {
            failures.push(format!(
                "shutdown exceeded {} seconds while closing storage",
                shutdown_timeout.as_secs()
            ));
        }
        drop(self.identity.take());
        if let Some(data_dir) = self.data_dir.take() {
            if let Err(error) = data_dir.unlock() {
                failures.push(error.to_string());
            }
            drop(data_dir);
        }

        if let Some(failure) = self.control.failure()
            && !failures.iter().any(|existing| existing == &failure)
        {
            failures.push(failure);
        }
        if let Some(mut task) = self.ipc_task.take() {
            match timeout_at(deadline, &mut task).await {
                Ok(Ok(Ok(()))) => {}
                Ok(Ok(Err(error))) => failures.push(error.to_string()),
                Ok(Err(error)) => failures.push(format!("owner control task failed: {error}")),
                Err(_) => {
                    task.abort();
                    let _ = task.await;
                    failures.push(format!(
                        "shutdown exceeded {} seconds while stopping the owner control channel",
                        shutdown_timeout.as_secs()
                    ));
                }
            }
        }
        let report = if failures.is_empty() {
            ShutdownResult::Completed
        } else {
            ShutdownResult::Failed(failures.join("; "))
        };
        self.control.complete(report.clone());
        if let Err(error) = self.control.finish_stop_responses(response_deadline).await {
            failures.push(error);
        }

        if failures.is_empty() {
            Ok(())
        } else {
            Err(AppError::Shutdown(failures.join("; ")))
        }
    }
}

fn storage_startup_reason(error: &StorageError) -> String {
    match error {
        StorageError::UnknownSchema(version) => {
            format!("storage schema version {version} is unsupported")
        }
        StorageError::UnrecognizedSchema => "storage contains an unrecognized schema".to_owned(),
        StorageError::Database(_) => "storage could not be opened or initialized".to_owned(),
        StorageError::MissingNodeMeta => "storage identity metadata is missing".to_owned(),
        StorageError::MultipleNodeIdentities => {
            "storage contains multiple node identities".to_owned()
        }
    }
}

impl Drop for NodeRuntime {
    fn drop(&mut self) {
        self.control.request_stop();
        if let Some(management) = &self.management {
            management.begin_shutdown();
        }
        if let Some(task) = self.network_task.take() {
            task.abort();
        }
        if let Some(task) = self.ipc_task.take() {
            task.abort();
        }
    }
}

async fn run_network(
    listener: TcpListener,
    tls: TlsAcceptor,
    mut stop: watch::Receiver<bool>,
    router: Router,
) -> Result<(), String> {
    let permits = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    let mut connections = JoinSet::new();

    loop {
        tokio::select! {
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    break;
                }
            }
            accepted = listener.accept() => {
                let (stream, peer) = accepted.map_err(|error| {
                    format!("accepting an HTTPS connection failed: {error}")
                })?;
                let Ok(permit) = permits.clone().try_acquire_owned() else {
                    drop(stream);
                    continue;
                };
                let tls = tls.clone();
                let connection_stop = stop.clone();
                let router = router.clone();
                connections.spawn(async move {
                    let _permit = permit;
                    serve_connection(stream, peer, tls, connection_stop, router).await;
                });
            }
            joined = connections.join_next(), if !connections.is_empty() => {
                if let Some(Err(error)) = joined {
                    return Err(format!("HTTPS connection task failed: {error}"));
                }
            }
        }
    }

    drop(listener);
    let deadline = sleep(CONNECTION_DRAIN_TIMEOUT);
    tokio::pin!(deadline);
    while !connections.is_empty() {
        tokio::select! {
            () = &mut deadline => {
                connections.abort_all();
                while connections.join_next().await.is_some() {}
                return Err(format!(
                    "HTTPS connections did not stop within {} seconds",
                    CONNECTION_DRAIN_TIMEOUT.as_secs()
                ));
            }
            joined = connections.join_next() => {
                if let Some(Err(error)) = joined {
                    connections.abort_all();
                    while connections.join_next().await.is_some() {}
                    return Err(format!("HTTPS connection task failed: {error}"));
                }
            }
        }
    }
    Ok(())
}

async fn serve_connection(
    stream: TcpStream,
    peer: SocketAddr,
    tls: TlsAcceptor,
    mut stop: watch::Receiver<bool>,
    router: Router,
) {
    if *stop.borrow() {
        return;
    }
    let Ok(Ok(tls_stream)) = timeout(TLS_HANDSHAKE_TIMEOUT, tls.accept(stream)).await else {
        return;
    };
    let service = TowerToHyperService::new(router.layer(Extension(PeerAddress(peer))));
    let builder = Builder::new(TokioExecutor::new());
    let connection = builder.serve_connection_with_upgrades(TokioIo::new(tls_stream), service);
    tokio::pin!(connection);
    tokio::select! {
        _ = &mut connection => {}
        changed = stop.changed() => {
            if changed.is_err() || *stop.borrow() {
                connection.as_mut().graceful_shutdown();
                let _ = timeout(CONNECTION_DRAIN_TIMEOUT, &mut connection).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        net::{IpAddr, Ipv4Addr},
        path::{Path, PathBuf},
        sync::{
            Mutex,
            atomic::{AtomicU64, Ordering},
        },
        time::{SystemTime, UNIX_EPOCH},
    };

    use futures_util::{SinkExt, StreamExt};
    use p256::ecdsa::SigningKey;
    use tokio_tungstenite::{
        WebSocketStream, client_async,
        tungstenite::{
            ClientRequestBuilder, Error as WebSocketError, Message,
            handshake::client::Response as WebSocketResponse,
        },
    };

    use super::*;
    use crate::{
        claims::{AuthProof, Id128, Nonce256, PairClaim, PublicKeySec1, verify_auth_host},
        error::PlatformError,
        ipc,
        jws::{JwsType, sign},
        management_ws::{CONTROL_PATH, CONTROL_PROTOCOL},
        platform::SecretProtector,
    };
    use rustls::pki_types::{CertificateDer, ServerName};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_rustls::TlsConnector;

    type ManagementSocket = WebSocketStream<tokio_rustls::client::TlsStream<TcpStream>>;

    static TEST_AREA_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    #[derive(Default)]
    struct FakeProtector {
        values: Mutex<Vec<Vec<u8>>>,
    }

    impl SecretProtector for FakeProtector {
        fn protect(&self, plaintext: &[u8], context: &[u8]) -> Result<Vec<u8>, PlatformError> {
            let mut value = b"FAKE".to_vec();
            value.extend_from_slice(context);
            value.extend_from_slice(plaintext);
            self.values
                .lock()
                .expect("fake protector lock")
                .push(value.clone());
            Ok(value)
        }

        fn unprotect(&self, ciphertext: &[u8], context: &[u8]) -> Result<Vec<u8>, PlatformError> {
            let expected = [b"FAKE".as_slice(), context].concat();
            if !ciphertext.starts_with(&expected) {
                return Err(PlatformError::Protection("bad fake context".to_owned()));
            }
            Ok(ciphertext[expected.len()..].to_vec())
        }
    }

    struct TestArea {
        directory: PathBuf,
        certificate: PathBuf,
        private_key: PathBuf,
    }

    impl TestArea {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let sequence = TEST_AREA_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let directory = std::env::temp_dir().join(format!(
                "acp-remote-node-{}-{nonce}-{sequence}",
                std::process::id()
            ));
            std::fs::create_dir(&directory).expect("test directory");
            let (certificate, private_key) = write_certificate(&directory);
            Self {
                directory,
                certificate,
                private_key,
            }
        }

        fn data_dir(&self) -> PathBuf {
            self.directory.join("data")
        }

        fn config(&self, listen: SocketAddr) -> NodeConfig {
            NodeConfig {
                data_dir: self.data_dir(),
                listen,
                canonical_origin: "https://node.example.test".to_owned(),
                tls_cert: self.certificate.clone(),
                tls_key: self.private_key.clone(),
            }
        }
    }

    impl Drop for TestArea {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    fn write_certificate(directory: &Path) -> (PathBuf, PathBuf) {
        let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])
            .expect("test certificate");
        let certificate = directory.join("server.pem");
        let private_key = directory.join("server-key.pem");
        std::fs::write(&certificate, certified.cert.pem()).expect("certificate");
        std::fs::write(&private_key, certified.signing_key.serialize_pem()).expect("key");
        (certificate, private_key)
    }

    fn free_address() -> SocketAddr {
        let listener =
            std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("free test port");
        listener.local_addr().expect("bound address")
    }

    fn client_public_key(key: &SigningKey) -> PublicKeySec1 {
        PublicKeySec1::new(
            key.verifying_key()
                .to_sec1_point(false)
                .as_bytes()
                .try_into()
                .expect("65-byte SEC1 key"),
        )
        .expect("valid SEC1 key")
    }

    fn tls_client_config(area: &TestArea) -> Arc<rustls::ClientConfig> {
        let certificate = pem::parse(std::fs::read(&area.certificate).expect("certificate bytes"))
            .expect("certificate PEM");
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(CertificateDer::from(certificate.into_contents()))
            .expect("trusted test certificate");
        let provider = rustls::crypto::ring::default_provider();
        let mut client_config = rustls::ClientConfig::builder_with_provider(Arc::new(provider))
            .with_safe_default_protocol_versions()
            .expect("TLS versions")
            .with_root_certificates(roots)
            .with_no_client_auth();
        client_config.alpn_protocols = vec![b"http/1.1".to_vec()];
        Arc::new(client_config)
    }

    async fn tls_connect(
        area: &TestArea,
        listen: SocketAddr,
    ) -> tokio_rustls::client::TlsStream<TcpStream> {
        let stream = TcpStream::connect(listen)
            .await
            .expect("TLS TCP connection");
        let connector = TlsConnector::from(tls_client_config(area));
        let server_name = ServerName::try_from("localhost").expect("server name");
        connector
            .connect(server_name, stream)
            .await
            .expect("TLS handshake")
    }

    async fn https_exchange(area: &TestArea, listen: SocketAddr, request: &[u8]) -> Vec<u8> {
        let mut stream = tls_connect(area, listen).await;
        stream.write_all(request).await.expect("HTTP request");
        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .await
            .expect("HTTP response");
        response
    }

    async fn https_response_prefix(area: &TestArea, listen: SocketAddr, request: &[u8]) -> Vec<u8> {
        let mut stream = tls_connect(area, listen).await;
        stream.write_all(request).await.expect("HTTP request");
        let mut response = vec![0_u8; 4096];
        let length = timeout(Duration::from_secs(2), stream.read(&mut response))
            .await
            .expect("HTTP response timeout")
            .expect("HTTP response");
        response.truncate(length);
        response
    }

    async fn websocket_exchange(
        area: &TestArea,
        listen: SocketAddr,
        uri_host: &str,
        headers: &[(&str, &str)],
    ) -> Result<(ManagementSocket, WebSocketResponse), WebSocketError> {
        let uri = format!("wss://{uri_host}{CONTROL_PATH}")
            .parse()
            .expect("WebSocket URI");
        let mut request = ClientRequestBuilder::new(uri);
        for (name, value) in headers {
            request = request.with_header(*name, *value);
        }
        client_async(request, tls_connect(area, listen).await).await
    }

    async fn valid_management_socket(area: &TestArea, listen: SocketAddr) -> ManagementSocket {
        let (socket, response) = websocket_exchange(
            area,
            listen,
            "node.example.test",
            &[
                ("Origin", "https://node.example.test"),
                ("Sec-WebSocket-Protocol", CONTROL_PROTOCOL),
            ],
        )
        .await
        .expect("management WebSocket handshake");
        assert_eq!(response.status(), 101);
        socket
    }

    async fn send_json(socket: &mut ManagementSocket, value: &serde_json::Value) {
        socket
            .send(Message::text(
                serde_json::to_string(value).expect("frame JSON"),
            ))
            .await
            .expect("WebSocket send");
    }

    async fn next_json(socket: &mut ManagementSocket, wait: Duration) -> serde_json::Value {
        timeout(wait, async {
            loop {
                match socket
                    .next()
                    .await
                    .expect("WebSocket frame")
                    .expect("WebSocket")
                {
                    Message::Text(text) => {
                        return serde_json::from_str(text.as_str()).expect("protocol JSON");
                    }
                    Message::Ping(_) | Message::Pong(_) => {}
                    Message::Close(frame) => {
                        panic!("unexpected WebSocket close: {frame:?}");
                    }
                    Message::Binary(_) | Message::Frame(_) => {
                        panic!("unexpected WebSocket frame");
                    }
                }
            }
        })
        .await
        .expect("timed out waiting for protocol frame")
    }

    async fn next_close(socket: &mut ManagementSocket, wait: Duration) -> (u16, String) {
        timeout(wait, async {
            loop {
                match socket
                    .next()
                    .await
                    .expect("WebSocket frame")
                    .expect("WebSocket")
                {
                    Message::Close(Some(frame)) => {
                        return (u16::from(frame.code), frame.reason.as_str().to_owned());
                    }
                    Message::Ping(_) | Message::Pong(_) => {}
                    Message::Text(text) => panic!("unexpected protocol frame: {text}"),
                    Message::Binary(_) | Message::Frame(_) | Message::Close(None) => {
                        panic!("unexpected WebSocket frame");
                    }
                }
            }
        })
        .await
        .expect("timed out waiting for WebSocket close")
    }

    fn node_public_key(runtime: &NodeRuntime) -> PublicKeySec1 {
        PublicKeySec1::new(
            runtime
                .identity
                .as_ref()
                .expect("node identity")
                .public_key()
                .try_into()
                .expect("65-byte SEC1 key"),
        )
        .expect("valid node public key")
    }

    async fn policy_actor(
        policy: &ApplicationPolicy,
        key: &SigningKey,
        client_id: &str,
        _connection_id: Id128,
    ) -> crate::access_policy::AuthenticatedActor {
        let client_id = Id128::parse_base64url(client_id).expect("client id");
        let key_bytes = client_public_key(key);
        let client_nonce = Nonce256::new([44; 32]);
        let lease = policy
            .issue_authentication_challenge(Some(client_id), key_bytes, client_nonce)
            .expect("challenge");
        let connection_id = lease.connection_id();
        let challenge = lease.challenge();
        let proof = AuthProof {
            v: 1,
            node_id: challenge.node_id,
            node_public_key: challenge.node_public_key,
            origin: challenge.origin.clone(),
            connection_id,
            challenge_id: challenge.challenge_id,
            nonce: challenge.nonce,
            iat: challenge.iat,
            exp: challenge.exp,
            client_id: Some(client_id),
            client_public_key: key_bytes,
            client_nonce,
        };
        let compact = sign(
            JwsType::AuthClient,
            &serde_json::to_vec(&proof).expect("payload"),
            key,
        )
        .expect("proof");
        policy
            .authenticate(connection_id, &compact)
            .await
            .expect("verified actor")
    }

    #[tokio::test]
    async fn runtime_approval_gate_is_independent_of_real_management_authentication() {
        use crate::access_policy::{
            AccessPolicyError, ApprovalPresentation, PolicyClock, RuntimeCapabilityRequest,
        };
        struct FixedClock;
        impl PolicyClock for FixedClock {
            fn unix_seconds(&self) -> Result<u64, AccessPolicyError> {
                Ok(100)
            }
        }
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let runtime = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("node");
        let key = SigningKey::from_slice(&[35; 32]).expect("test key");
        let client_id = pair_browser(&runtime, &key, "Approval boundary test").await;
        let original_policy = runtime.application_policy().expect("policy is assembled");
        let policy = original_policy.with_clock(Arc::new(FixedClock));
        let actor = policy_actor(&policy, &key, &client_id, Id128::new([80; 16])).await;
        policy
            .require_current_actor(&actor)
            .await
            .expect("management qualification");
        let mut dispatched = 0;
        let mut spy = |_: &RuntimeCapabilityRequest| {
            dispatched += 1;
        };
        spy(&RuntimeCapabilityRequest {
            name: "mock.runtime.operation".to_owned(),
            approval: None,
        });
        assert_eq!(
            dispatched, 1,
            "positive control for the side-effect counter"
        );
        dispatched = 0;
        for expires_at in [None, Some(99), Some(100), Some(101)] {
            let request = RuntimeCapabilityRequest {
                name: "mock.runtime.operation".to_owned(),
                approval: expires_at.map(|expires_at| ApprovalPresentation { expires_at }),
            };
            let error = policy
                .request_runtime_capability(&actor, &request, |_| {
                    dispatched += 1;
                })
                .await
                .expect_err("management trust is not approval");
            assert!(match expires_at {
                None => matches!(error, AccessPolicyError::ApprovalRequired),
                Some(99 | 100) => matches!(error, AccessPolicyError::ApprovalExpired),
                _ => matches!(error, AccessPolicyError::ApprovalUnverified),
            });
        }
        assert_eq!(dispatched, 0);
        let reauthenticated = policy_actor(&policy, &key, &client_id, Id128::new([81; 16])).await;
        let expired = RuntimeCapabilityRequest {
            name: "mock.runtime.operation".to_owned(),
            approval: Some(ApprovalPresentation { expires_at: 99 }),
        };
        assert!(matches!(
            policy
                .request_runtime_capability(&reauthenticated, &expired, |_| {
                    dispatched += 1;
                })
                .await,
            Err(AccessPolicyError::ApprovalExpired)
        ));
        ipc::device_revoke(runtime.ipc_address(), &client_id)
            .await
            .expect("owner revocation");
        assert!(matches!(
            policy
                .request_runtime_capability(&actor, &expired, |_| {
                    dispatched += 1;
                })
                .await,
            Err(AccessPolicyError::InvalidActor)
        ));
        assert_eq!(dispatched, 0);
        runtime.shutdown(None).await.expect("shutdown");
        assert!(matches!(
            policy.issue_authentication_challenge(
                None,
                client_public_key(&key),
                Nonce256::new([45; 32])
            ),
            Err(AccessPolicyError::NodeUnavailable)
        ));
        assert!(matches!(
            policy.require_current_actor(&actor).await,
            Err(AccessPolicyError::NodeUnavailable)
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn dropping_node_closes_policy_before_aborted_tasks_can_run() {
        use crate::access_policy::AccessPolicyError;
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let runtime = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("node");
        let policy = runtime.application_policy().expect("policy");
        let key = SigningKey::from_slice(&[36; 32]).expect("fixture key");
        let id = pair_browser(&runtime, &key, "Drop boundary test").await;
        let actor = policy_actor(&policy, &key, &id, Id128::new([83; 16])).await;
        let lease = policy
            .issue_authentication_challenge(None, client_public_key(&key), Nonce256::new([46; 32]))
            .expect("owned challenge");
        drop(runtime);
        assert!(matches!(
            policy.issue_authentication_challenge(
                None,
                client_public_key(&key),
                Nonce256::new([47; 32])
            ),
            Err(AccessPolicyError::NodeUnavailable)
        ));
        assert!(matches!(
            policy.authenticate(lease.connection_id(), "invalid").await,
            Err(AccessPolicyError::NodeUnavailable)
        ));
        assert!(matches!(
            policy.require_current_actor(&actor).await,
            Err(AccessPolicyError::NodeUnavailable)
        ));
        drop(lease);
        drop(policy);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    #[tokio::test]
    async fn owner_stop_reports_storage_failure_at_the_real_fifteen_second_deadline() {
        let area = TestArea::new();
        let runtime = NodeRuntime::start(
            area.config(free_address()),
            &crate::platform::DpapiProtector,
        )
        .await
        .expect("real Windows node");
        let barrier = runtime
            .storage
            .as_ref()
            .expect("storage")
            .pause_next_close();
        let mut stop_signal = runtime.control.stop_receiver();
        let mut report = runtime.control.shutdown_receiver();
        let address = runtime.ipc_address().clone();
        let owner_stop = tokio::spawn(async move { ipc::stop(&address).await });
        timeout(Duration::from_secs(2), stop_signal.changed())
            .await
            .expect("owner stop reaches node")
            .expect("stop signal");
        let started = tokio::time::Instant::now();
        let shutdown = tokio::spawn(runtime.shutdown(None));
        timeout(Duration::from_secs(2), barrier.wait_until_entered())
            .await
            .expect("real storage close reached");
        assert_eq!(*report.borrow(), ShutdownResult::Pending);
        let stop_error = timeout(Duration::from_secs(20), owner_stop)
            .await
            .expect("bounded owner response")
            .expect("owner task")
            .expect_err("failure must not report stopped");
        let shutdown_error = shutdown
            .await
            .expect("shutdown task")
            .expect_err("blocked cleanup is reported");
        // The production 15-second budget reserves its final second for IPC delivery.
        let cleanup_budget = SHUTDOWN_TIMEOUT.saturating_sub(SHUTDOWN_RESPONSE_TIMEOUT);
        assert!(started.elapsed() >= cleanup_budget);
        assert!(started.elapsed() < SHUTDOWN_TIMEOUT + Duration::from_secs(2));
        eprintln!(
            "configured_shutdown_ms={} cleanup_budget_ms={} observed_ms={}",
            SHUTDOWN_TIMEOUT.as_millis(),
            cleanup_budget.as_millis(),
            started.elapsed().as_millis()
        );
        assert!(
            stop_error.to_string().contains("closing storage"),
            "{stop_error}"
        );
        assert!(
            shutdown_error.to_string().contains("15 seconds"),
            "{shutdown_error}"
        );
        report.borrow_and_update();
        assert!(
            matches!(&*report.borrow(), ShutdownResult::Failed(message) if message.contains("closing storage"))
        );
        barrier.release();
        assert!(
            area.data_dir().exists(),
            "failure must not delete stored identity"
        );
    }

    async fn pair_browser(runtime: &NodeRuntime, key: &SigningKey, display_name: &str) -> String {
        let (mut session, start) = ipc::begin_pairing(runtime.ipc_address())
            .await
            .expect("pairing session");
        let pairing = runtime.pairing.as_ref().expect("pairing service");
        let pairing_id = Id128::parse_base64url(&start.pairing_id).expect("pairing id");
        let request_id = Id128::new([90; 16]);
        let lookup = pairing
            .lookup(&start.code, request_id)
            .await
            .expect("pair lookup");
        let claim = PairClaim {
            v: 1,
            node_id: lookup.challenge.node_id,
            origin: lookup.challenge.origin,
            request_id,
            challenge_id: lookup.challenge.challenge_id,
            nonce: lookup.challenge.nonce,
            iat: lookup.challenge.iat,
            exp: lookup.challenge.exp,
            pairing_id,
            client_public_key: client_public_key(key),
            display_name: display_name.to_owned(),
        };
        let proof = sign(
            JwsType::PairClaim,
            &serde_json::to_vec(&claim).expect("claim payload"),
            key,
        )
        .expect("claim proof");
        pairing.claim(&proof).await.expect("claim");
        let pending = session.next_event().await.expect("pending event");
        assert_eq!(pending.event, "pending_confirmation");
        session.approve().await.expect("approval decision");
        let approved = session.next_event().await.expect("approval result");
        assert_eq!(approved.event, "approved");
        approved.client_id.expect("approved client id")
    }

    async fn prove_client(
        socket: &mut ManagementSocket,
        key: &SigningKey,
        client_id: &str,
        node_key: PublicKeySec1,
    ) -> AuthProof {
        prove_client_with_id(socket, key, Some(client_id), node_key)
            .await
            .0
    }

    async fn prove_client_with_id(
        socket: &mut ManagementSocket,
        key: &SigningKey,
        client_id: Option<&str>,
        node_key: PublicKeySec1,
    ) -> (AuthProof, String) {
        send_json(
            socket,
            &serde_json::json!({
                "v": 1,
                "type": "hello",
                "clientId": client_id,
                "clientPublicKey": client_public_key(key),
                "clientNonce": Nonce256::new([0xA5; 32]),
            }),
        )
        .await;
        let challenge = next_json(socket, Duration::from_secs(2)).await;
        assert_eq!(challenge["v"], 1);
        assert_eq!(challenge["type"], "challenge");
        let host_proof = challenge["hostProof"].as_str().expect("host proof");
        let proof = verify_auth_host(host_proof, &node_key).expect("verified host proof");
        assert_eq!(proof.origin, "https://node.example.test");
        assert_eq!(
            proof.client_id.map(Id128::to_base64url).as_deref(),
            client_id
        );
        assert_eq!(proof.client_public_key, client_public_key(key));
        let connection_id = proof.connection_id.to_base64url();
        assert_eq!(
            challenge["connectionId"].as_str(),
            Some(connection_id.as_str())
        );
        let compact = sign(
            JwsType::AuthClient,
            &serde_json::to_vec(&proof).expect("auth payload"),
            key,
        )
        .expect("auth proof");
        send_json(
            socket,
            &serde_json::json!({"v": 1, "type": "proof", "proof": compact}),
        )
        .await;
        (proof, compact)
    }

    async fn authenticate_socket(
        mut socket: ManagementSocket,
        key: &SigningKey,
        client_id: &str,
        node_key: PublicKeySec1,
    ) -> ManagementSocket {
        let host_proof = prove_client(&mut socket, key, client_id, node_key).await;
        let authenticated = next_json(&mut socket, Duration::from_secs(2)).await;
        assert_eq!(authenticated["v"], 1);
        assert_eq!(authenticated["type"], "authenticated");
        assert_eq!(authenticated["clientId"], client_id);
        let connection_id = host_proof.connection_id.to_base64url();
        assert_eq!(
            authenticated["connectionId"].as_str(),
            Some(connection_id.as_str())
        );
        socket
    }

    #[tokio::test]
    async fn starts_reports_ready_and_stops_through_owner_ipc() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let runtime = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("node starts");
        let ipc_address = runtime.ipc_address().clone();
        let node_id = runtime.node_id().to_owned();

        let ready = ipc::status(&ipc_address).await.expect("ready status");
        assert_eq!(ready.state, "ready");
        assert_eq!(ready.node_id, node_id);
        assert_eq!(ready.origin, "https://node.example.test");

        let waiter = tokio::spawn(runtime.wait_for_stop());
        let stopped = ipc::stop(&ipc_address).await.expect("owner stop");
        assert_eq!(stopped.state, "stopped");
        waiter
            .await
            .expect("stop waiter task")
            .expect("clean shutdown");
    }

    #[tokio::test]
    async fn owner_ipc_keeps_thirty_two_active_connections_and_rejects_only_the_next() {
        use tokio::net::windows::named_pipe::ClientOptions;
        let area = TestArea::new();
        let runtime = NodeRuntime::start(area.config(free_address()), &FakeProtector::default())
            .await
            .expect("start");
        let mut clients = Vec::new();
        for _ in 0..33 {
            let client = timeout(Duration::from_secs(2), async {
                loop {
                    match ClientOptions::new().open(&runtime.ipc_address().pipe_name) {
                        Ok(client) => break client,
                        Err(error) if error.raw_os_error() == Some(231) => {
                            tokio::time::sleep(Duration::from_millis(5)).await;
                        }
                        Err(error) => panic!("connect owner pipe: {error}"),
                    }
                }
            })
            .await
            .expect("bounded connect");
            clients.push(client);
            if clients.len() <= 32 {
                clients
                    .last_mut()
                    .expect("new client")
                    .write_all(b"{")
                    .await
                    .expect("partial request");
            }
        }
        let mut rejected = clients.pop().expect("33rd connection");
        let mut byte = [0_u8; 1];
        let result = timeout(Duration::from_secs(2), rejected.read(&mut byte))
            .await
            .expect("over-budget close");
        assert!(
            matches!(result, Ok(0) | Err(_)),
            "no response to rejected connection"
        );
        drop(rejected);
        assert!(
            runtime.control.is_ready(),
            "node failed: {:?}",
            runtime.control.failure()
        );
        for (index, mut client) in clients.into_iter().enumerate() {
            let tail = format!(
                "\"v\":1,\"requestId\":\"{}\",\"command\":\"status\",\"payload\":{{}}}}\n",
                Id128::new([0x72; 16]).to_base64url()
            );
            client
                .write_all(tail.as_bytes())
                .await
                .unwrap_or_else(|error| {
                    panic!(
                        "complete existing request {index}: {error}; node failure {:?}",
                        runtime.control.failure()
                    )
                });
            let mut response = Vec::new();
            timeout(Duration::from_secs(2), client.read_to_end(&mut response))
                .await
                .expect("existing deadline")
                .expect("existing response");
            let response: serde_json::Value =
                serde_json::from_slice(&response).expect("JSON response");
            assert_eq!(
                response["ok"], true,
                "existing connection survives quota rejection"
            );
        }
        assert_eq!(
            ipc::status(runtime.ipc_address())
                .await
                .expect("new connection after release")
                .state,
            "ready"
        );
        runtime.shutdown(None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn owner_ipc_lists_and_revokes_clients_without_a_web_identity() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let listener = free_address();
        let runtime = NodeRuntime::start(area.config(listener), &protector)
            .await
            .expect("node starts");
        let ipc_address = runtime.ipc_address().clone();
        let client_id = crate::claims::Id128::new([10; 16]).to_base64url();
        sqlx::query(
            "INSERT INTO clients(
                client_id, public_key, display_name, status, auth_generation, created_at
             ) VALUES (?, ?, 'Owner test client', 'trusted', 1, 1700000000)",
        )
        .bind(&client_id)
        .bind(vec![4_u8; 65])
        .execute(runtime.storage.as_ref().expect("storage").pool_for_test())
        .await
        .expect("test client");

        let page = ipc::device_list(&ipc_address, None)
            .await
            .expect("owner list");
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].client_id, client_id);
        assert_eq!(page.items[0].display_name, "Owner test client");

        let revoked = ipc::device_revoke(&ipc_address, &client_id)
            .await
            .expect("owner revoke");
        assert_eq!(revoked.client_id, client_id);
        assert_eq!(revoked.status, crate::trust::ClientStatus::Revoked);
        let page = ipc::device_list(&ipc_address, None)
            .await
            .expect("owner list after revoke");
        assert_eq!(page.items[0].status, crate::trust::ClientStatus::Revoked);

        let waiter = tokio::spawn(runtime.wait_for_stop());
        ipc::stop(&ipc_address).await.expect("owner stop");
        waiter
            .await
            .expect("stop waiter task")
            .expect("clean shutdown");

        let restarted = NodeRuntime::start(area.config(listener), &protector)
            .await
            .expect("restart");
        let page = ipc::device_list(restarted.ipc_address(), None)
            .await
            .expect("list after restart");
        assert_eq!(page.items[0].status, crate::trust::ClientStatus::Revoked);
        restarted.shutdown(None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn owner_pairing_session_persists_an_approved_browser_client() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let listen = free_address();
        let runtime = NodeRuntime::start(area.config(listen), &protector)
            .await
            .expect("node starts");
        let ipc_address = runtime.ipc_address().clone();
        let (mut session, start) = ipc::begin_pairing(&ipc_address)
            .await
            .expect("pairing session");
        assert_eq!(start.node_id, runtime.node_id());
        assert_eq!(start.origin, "https://node.example.test");
        assert_eq!(start.code.len(), 9);

        let pairing = runtime.pairing.as_ref().expect("pairing service");
        let pairing_id = Id128::parse_base64url(&start.pairing_id).expect("pairing id");
        let request_id = Id128::new([70; 16]);
        let lookup = pairing
            .lookup(&start.code, request_id)
            .await
            .expect("pair lookup");
        let client_key = SigningKey::from_slice(&[13_u8; 32]).expect("client key");
        let claim = PairClaim {
            v: 1,
            node_id: lookup.challenge.node_id,
            origin: lookup.challenge.origin,
            request_id,
            challenge_id: lookup.challenge.challenge_id,
            nonce: lookup.challenge.nonce,
            iat: lookup.challenge.iat,
            exp: lookup.challenge.exp,
            pairing_id,
            client_public_key: client_public_key(&client_key),
            display_name: "Browser Alice".to_owned(),
        };
        let proof = sign(
            JwsType::PairClaim,
            &serde_json::to_vec(&claim).expect("claim payload"),
            &client_key,
        )
        .expect("claim proof");
        pairing.claim(&proof).await.expect("claim");

        let pending = session.next_event().await.expect("pending event");
        assert_eq!(pending.event, "pending_confirmation");
        assert_eq!(pending.display_name.as_deref(), Some("Browser Alice"));
        assert!(pending.fingerprint.as_deref().is_some_and(|value| {
            value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        }));

        session.approve().await.expect("approval decision");
        let approved = session.next_event().await.expect("approval result");
        assert_eq!(approved.event, "approved");
        let client_id = approved.client_id.expect("approved client id");
        let page = ipc::device_list(&ipc_address, None)
            .await
            .expect("device list");
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].client_id, client_id);
        assert_eq!(page.items[0].status, crate::trust::ClientStatus::Trusted);

        drop(session);
        let waiter = tokio::spawn(runtime.wait_for_stop());
        ipc::stop(&ipc_address).await.expect("owner stop");
        waiter
            .await
            .expect("stop waiter task")
            .expect("clean shutdown");

        let restarted = NodeRuntime::start(area.config(listen), &protector)
            .await
            .expect("restart");
        let page = ipc::device_list(restarted.ipc_address(), None)
            .await
            .expect("list after restart");
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].client_id, client_id);
        assert_eq!(page.items[0].status, crate::trust::ClientStatus::Trusted);
        restarted.shutdown(None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn closing_a_pairing_session_before_claim_cancels_it_immediately() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let runtime = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("node starts");
        let (session, start) = ipc::begin_pairing(runtime.ipc_address())
            .await
            .expect("pairing session");
        drop(session);
        let pairing = runtime.pairing.as_ref().expect("pairing service");
        let request_id = Id128::new([71; 16]);
        timeout(Duration::from_secs(2), async {
            loop {
                match pairing.lookup(&start.code, request_id).await {
                    Err(crate::pairing::PairingError::PairingInvalid) => break,
                    Ok(_) => sleep(Duration::from_millis(10)).await,
                    Err(error) => panic!("unexpected pairing lookup error: {error}"),
                }
            }
        })
        .await
        .expect("disconnected session is cancelled");
        runtime.shutdown(None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn serves_the_embedded_web_ui_and_fails_closed_on_unknown_paths() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let runtime = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("node starts");
        let listen = runtime.listen_addr();

        let index = https_exchange(
            &area,
            listen,
            b"GET / HTTP/1.1\r\nHost: node.example.test\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(
            index.starts_with(b"HTTP/1.1 200")
                && index.windows(9).any(|window| window == b"text/html"),
            "unexpected response: {}",
            String::from_utf8_lossy(&index)
        );
        assert!(
            index
                .windows(32)
                .any(|window| window == b"content-security-policy: default"),
            "unexpected response: {}",
            String::from_utf8_lossy(&index)
        );

        let response = https_exchange(
            &area,
            listen,
            b"GET /missing HTTP/1.1\r\nHost: node.example.test\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(
            response.starts_with(b"HTTP/1.1 404"),
            "unexpected response: {}",
            String::from_utf8_lossy(&response)
        );

        runtime.shutdown(None).await.expect("clean shutdown");
    }

    #[tokio::test]
    async fn serves_the_owner_started_pairing_lookup_over_tls() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let runtime = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("node starts");
        let (session, start) = ipc::begin_pairing(runtime.ipc_address())
            .await
            .expect("pairing session");
        let body = serde_json::to_vec(&serde_json::json!({
            "v": 1,
            "code": start.code,
            "requestId": Id128::new([80; 16]),
        }))
        .expect("lookup body");
        let request = format!(
            "POST /api/access/v1/pair/lookup HTTP/1.1\r\n\
             Host: node.example.test\r\n\
             Origin: https://node.example.test\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\r\n",
            body.len()
        );
        let mut request = request.into_bytes();
        request.extend_from_slice(&body);
        let response = https_exchange(&area, runtime.listen_addr(), &request).await;
        assert!(
            response.starts_with(b"HTTP/1.1 200"),
            "unexpected response: {}",
            String::from_utf8_lossy(&response)
        );
        let body_start = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|index| index + 4)
            .expect("response header terminator");
        let lookup: serde_json::Value =
            serde_json::from_slice(&response[body_start..]).expect("lookup response");
        assert_eq!(lookup["pairingId"], start.pairing_id);
        assert_eq!(lookup["origin"], "https://node.example.test");

        drop(session);
        runtime.shutdown(None).await.expect("clean shutdown");
    }

    #[tokio::test]
    async fn pairing_lookup_rate_limit_uses_the_tcp_ip_not_ephemeral_ports() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let runtime = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("node starts");
        let request = b"POST /api/access/v1/pair/lookup HTTP/1.1\r\n\
                        Host: node.example.test\r\n\
                        Origin: https://node.example.test\r\n\
                        Content-Type: application/json\r\n\
                        Content-Length: 2\r\n\
                        Connection: close\r\n\r\n{}";

        for index in 0..5 {
            let response = https_response_prefix(&area, runtime.listen_addr(), request).await;
            assert!(
                response.starts_with(b"HTTP/1.1 400"),
                "request {index} should be counted but still invalid: {}",
                String::from_utf8_lossy(&response)
            );
        }
        let response = https_response_prefix(&area, runtime.listen_addr(), request).await;
        let response_text = String::from_utf8_lossy(&response);
        assert!(
            response.starts_with(b"HTTP/1.1 429"),
            "a new source port must not reset the lookup budget: {response_text}"
        );
        assert!(
            response_text
                .to_ascii_lowercase()
                .contains("retry-after: 60")
        );
        assert!(response_text.contains(r#""code":"rate_limited","retryAfterSeconds":60"#));

        runtime.shutdown(None).await.expect("clean shutdown");
    }

    #[tokio::test]
    async fn management_handshake_rate_limit_uses_the_tcp_ip_not_ephemeral_ports() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let runtime = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("node starts");
        let listen = runtime.listen_addr();
        let headers = [
            ("Origin", "https://node.example.test"),
            ("Sec-WebSocket-Protocol", CONTROL_PROTOCOL),
        ];

        for index in 0..30 {
            let (mut socket, response) =
                websocket_exchange(&area, listen, "node.example.test", &headers)
                    .await
                    .unwrap_or_else(|error| panic!("handshake {index} failed: {error}"));
            assert_eq!(response.status(), 101);
            socket.close(None).await.expect("close handshake socket");
        }
        match websocket_exchange(&area, listen, "node.example.test", &headers).await {
            Err(WebSocketError::Http(response)) => {
                assert_eq!(response.status(), 429);
                assert_eq!(
                    response
                        .headers()
                        .get("retry-after")
                        .and_then(|value| value.to_str().ok()),
                    Some("60")
                );
            }
            Err(error) => panic!("unexpected handshake error: {error}"),
            Ok((mut socket, _)) => {
                let _ = socket.close(None).await;
                panic!("a new source port unexpectedly reset the handshake budget");
            }
        }

        runtime.shutdown(None).await.expect("clean shutdown");
    }

    #[tokio::test]
    async fn management_websocket_rejects_invalid_origin_host_and_protocols() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let runtime = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("node starts");
        let listen = runtime.listen_addr();
        let canonical = "https://node.example.test";
        let protocol = CONTROL_PROTOCOL;
        let cases: [(&str, Vec<(&str, &str)>); 7] = [
            (
                "other.example.test",
                vec![("Origin", canonical), ("Sec-WebSocket-Protocol", protocol)],
            ),
            (
                "node.example.test",
                vec![("Sec-WebSocket-Protocol", protocol)],
            ),
            (
                "node.example.test",
                vec![
                    ("Origin", "https://other.example.test"),
                    ("Sec-WebSocket-Protocol", protocol),
                ],
            ),
            (
                "node.example.test",
                vec![
                    ("Origin", canonical),
                    ("Origin", canonical),
                    ("Sec-WebSocket-Protocol", protocol),
                ],
            ),
            (
                "node.example.test",
                vec![
                    ("Origin", canonical),
                    ("Sec-WebSocket-Protocol", "acpr.access.v2"),
                ],
            ),
            (
                "node.example.test",
                vec![
                    ("Origin", canonical),
                    ("Sec-WebSocket-Protocol", protocol),
                    ("Sec-WebSocket-Protocol", protocol),
                ],
            ),
            (
                "node.example.test",
                vec![
                    ("Origin", canonical),
                    ("Sec-WebSocket-Protocol", "acpr.access.v1, acpr.access.v2"),
                ],
            ),
        ];

        for (index, (host, headers)) in cases.iter().enumerate() {
            match websocket_exchange(&area, listen, host, headers).await {
                Err(WebSocketError::Http(response)) => {
                    assert_eq!(response.status(), 400, "rejected case {index}");
                }
                Err(error) => panic!("case {index} failed before HTTP rejection: {error}"),
                Ok(_) => panic!("case {index} unexpectedly upgraded"),
            }
        }

        let duplicate_host = format!(
            "GET {CONTROL_PATH} HTTP/1.1\r\n\
             Host: node.example.test\r\n\
             Host: node.example.test\r\n\
             Origin: {canonical}\r\n\
             Connection: Upgrade\r\n\
             Upgrade: websocket\r\n\
             Sec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
             Sec-WebSocket-Protocol: {protocol}\r\n\r\n"
        );
        let response = https_response_prefix(&area, listen, duplicate_host.as_bytes()).await;
        assert!(
            response.starts_with(b"HTTP/1.1 400"),
            "duplicate Host was not rejected: {}",
            String::from_utf8_lossy(&response)
        );
        runtime.shutdown(None).await.expect("clean shutdown");
    }

    #[tokio::test]
    async fn management_websocket_rejects_unauthenticated_frames_and_times_out() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let runtime = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("node starts");
        let listen = runtime.listen_addr();

        let mut socket = valid_management_socket(&area, listen).await;
        send_json(
            &mut socket,
            &serde_json::json!({
                "v": 1,
                "type": "command",
                "requestId": Id128::new([95; 16]),
                "action": "node.info",
                "payload": {}
            }),
        )
        .await;
        let (code, reason) = next_close(&mut socket, Duration::from_secs(2)).await;
        assert_eq!(code, 4400);
        assert_eq!(reason, "invalid frame");

        let mut idle = valid_management_socket(&area, listen).await;
        let (code, reason) = next_close(&mut idle, Duration::from_secs(7)).await;
        assert_eq!(code, 4408);
        assert_eq!(reason, "authentication timeout");

        runtime.shutdown(None).await.expect("clean shutdown");
    }

    #[tokio::test]
    async fn management_websocket_closes_oversized_transport_frames_with_4429() {
        let area = TestArea::new();
        let runtime = NodeRuntime::start(area.config(free_address()), &FakeProtector::default())
            .await
            .expect("node starts");
        for length in [32 * 1024 + 1, 32 * 1024 + 2, 64 * 1024] {
            let mut socket = valid_management_socket(&area, runtime.listen_addr()).await;
            socket
                .send(Message::Text("x".repeat(length).into()))
                .await
                .expect("oversized frame");
            let (code, reason) = next_close(&mut socket, Duration::from_secs(2)).await;
            assert_eq!(code, 4429, "frame length {length}");
            assert_eq!(reason, "limit exceeded");
        }
        runtime.shutdown(None).await.expect("clean shutdown");
    }

    #[tokio::test]
    async fn management_websocket_enforces_the_per_client_connection_limit() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let runtime = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("node starts");
        let listen = runtime.listen_addr();
        let node_key = node_public_key(&runtime);
        let client_key = SigningKey::from_slice(&[17_u8; 32]).expect("client key");
        let client_id = pair_browser(&runtime, &client_key, "Bounded browser").await;

        let mut sockets = Vec::new();
        for _ in 0..8 {
            sockets.push(
                authenticate_socket(
                    valid_management_socket(&area, listen).await,
                    &client_key,
                    &client_id,
                    node_key,
                )
                .await,
            );
        }

        let mut ninth = valid_management_socket(&area, listen).await;
        prove_client(&mut ninth, &client_key, &client_id, node_key).await;
        let (code, reason) = next_close(&mut ninth, Duration::from_secs(3)).await;
        assert_eq!(code, 4429);
        assert_eq!(reason, "limit exceeded");

        for socket in &mut sockets {
            let _ = socket.close(None).await;
        }
        runtime.shutdown(None).await.expect("clean shutdown");
    }

    #[tokio::test]
    async fn revocation_during_authentication_resolution_closes_the_candidate_connection() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let runtime = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("node starts");
        let listen = runtime.listen_addr();
        let node_key = node_public_key(&runtime);
        let client_key = SigningKey::from_slice(&[20_u8; 32]).expect("client key");
        let client_id = pair_browser(&runtime, &client_key, "Racing browser").await;
        let resolve_barrier = runtime
            .management
            .as_ref()
            .expect("management service")
            .pause_next_after_client_resolve();
        let mut socket = valid_management_socket(&area, listen).await;

        prove_client(&mut socket, &client_key, &client_id, node_key).await;
        timeout(Duration::from_secs(2), resolve_barrier.wait_until_entered())
            .await
            .expect("authentication reached the post-resolution boundary");
        let revoked = ipc::device_revoke(runtime.ipc_address(), &client_id)
            .await
            .expect("owner revoke");
        assert_eq!(revoked.client_id, client_id);
        resolve_barrier.release();

        let (code, reason) = next_close(&mut socket, Duration::from_secs(3)).await;
        assert_eq!(code, 4410);
        assert_eq!(reason, "revoked");
        runtime.shutdown(None).await.expect("clean shutdown");
    }

    #[tokio::test]
    async fn management_websocket_discards_auth_challenge_after_invalid_frame() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let runtime = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("node starts");
        let client_key = SigningKey::from_slice(&[33_u8; 32]).expect("client key");
        let mut socket = valid_management_socket(&area, runtime.listen_addr()).await;
        send_json(
            &mut socket,
            &serde_json::json!({
                "v": 1,
                "type": "hello",
                "clientId": null,
                "clientPublicKey": client_public_key(&client_key),
                "clientNonce": Nonce256::new([0x5A; 32]),
            }),
        )
        .await;
        let challenge = next_json(&mut socket, Duration::from_secs(2)).await;
        assert_eq!(challenge["type"], "challenge");
        assert_eq!(
            runtime
                .pairing
                .as_ref()
                .expect("pairing service")
                .active_auth_challenge_count(),
            1
        );

        socket
            .send(Message::text("{}"))
            .await
            .expect("invalid frame");
        let (code, reason) = next_close(&mut socket, Duration::from_secs(3)).await;
        assert_eq!(code, 4400);
        assert_eq!(reason, "invalid frame");
        assert_eq!(
            runtime
                .pairing
                .as_ref()
                .expect("pairing service")
                .active_auth_challenge_count(),
            0
        );
        runtime.shutdown(None).await.expect("clean shutdown");
    }

    #[tokio::test]
    async fn management_websocket_cursor_becomes_stale_after_a_committed_revision_change() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let runtime = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("node starts");
        let listen = runtime.listen_addr();
        let node_key = node_public_key(&runtime);
        let primary_key = SigningKey::from_slice(&[18_u8; 32]).expect("primary client key");
        let primary_id = pair_browser(&runtime, &primary_key, "Paging browser").await;
        let trust = runtime
            .storage
            .as_ref()
            .expect("node storage")
            .trust_store();

        let mut inserted_ids = Vec::new();
        for index in 0..33_u8 {
            let key = SigningKey::from_slice(&[40 + index; 32]).expect("managed client key");
            let client_id = Id128::new([0x80 + index; 16]).to_base64url();
            trust
                .register_client(
                    &client_id,
                    client_public_key(&key).as_bytes(),
                    &format!("Managed device {index}"),
                )
                .await
                .expect("register managed client");
            inserted_ids.push(client_id);
        }

        let mut socket = authenticate_socket(
            valid_management_socket(&area, listen).await,
            &primary_key,
            &primary_id,
            node_key,
        )
        .await;
        send_json(
            &mut socket,
            &serde_json::json!({
                "v": 1,
                "type": "command",
                "requestId": Id128::new([110; 16]),
                "action": "device.list",
                "payload": {"cursor": null}
            }),
        )
        .await;
        let first_page = next_json(&mut socket, Duration::from_secs(2)).await;
        assert_eq!(first_page["ok"], true);
        assert_eq!(
            first_page["result"]["items"].as_array().map(Vec::len),
            Some(32)
        );
        assert!(
            first_page["result"]["items"]
                .as_array()
                .expect("items")
                .iter()
                .all(|item| item.get("publicKey").is_none())
        );
        let cursor = first_page["result"]["nextCursor"]
            .as_str()
            .expect("continuation cursor")
            .to_owned();

        send_json(
            &mut socket,
            &serde_json::json!({
                "v": 1,
                "type": "command",
                "requestId": Id128::new([111; 16]),
                "action": "device.revoke",
                "payload": {"clientId": inserted_ids[0]}
            }),
        )
        .await;
        let revoked = next_json(&mut socket, Duration::from_secs(2)).await;
        assert_eq!(revoked["ok"], true);
        assert_eq!(revoked["result"]["status"], "revoked");
        assert!(
            revoked["result"]["revision"].is_null(),
            "revoke result is the closed client/status shape"
        );

        send_json(
            &mut socket,
            &serde_json::json!({
                "v": 1,
                "type": "command",
                "requestId": Id128::new([112; 16]),
                "action": "device.list",
                "payload": {"cursor": cursor}
            }),
        )
        .await;
        let stale = next_json(&mut socket, Duration::from_secs(2)).await;
        assert_eq!(stale["ok"], false);
        assert_eq!(stale["result"], serde_json::Value::Null);
        assert_eq!(stale["error"]["code"], "stale_cursor");

        socket.close(None).await.expect("close management socket");
        runtime.shutdown(None).await.expect("clean shutdown");
    }

    #[tokio::test]
    async fn management_websocket_receives_going_away_when_the_node_stops() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let runtime = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("node starts");
        let listen = runtime.listen_addr();
        let node_key = node_public_key(&runtime);
        let client_key = SigningKey::from_slice(&[19_u8; 32]).expect("client key");
        let client_id = pair_browser(&runtime, &client_key, "Stopping browser").await;
        let mut socket = authenticate_socket(
            valid_management_socket(&area, listen).await,
            &client_key,
            &client_id,
            node_key,
        )
        .await;

        let ipc_address = runtime.ipc_address().clone();
        let node_task = tokio::spawn(runtime.wait_for_stop());
        let stop_task = tokio::spawn(async move { ipc::stop(&ipc_address).await });

        let (code, reason) = next_close(&mut socket, Duration::from_secs(5)).await;
        assert_eq!(code, 1001);
        assert_eq!(reason, "node stopping");
        stop_task
            .await
            .expect("stop task")
            .expect("owner stop succeeds");
        node_task
            .await
            .expect("node task")
            .expect("node shutdown succeeds");
    }

    #[tokio::test]
    async fn revoked_actor_cannot_be_admitted_or_enqueue_a_completed_result() {
        use crate::management_ws::ManagementBoundary;
        for (at_enqueue, cached) in [(false, false), (true, false), (true, true)] {
            let area = TestArea::new();
            let runtime =
                NodeRuntime::start(area.config(free_address()), &FakeProtector::default())
                    .await
                    .expect("start");
            let key = SigningKey::from_slice(&[0x51; 32]).expect("key");
            let client_id = pair_browser(&runtime, &key, "Admission fixture").await;
            let mut socket = authenticate_socket(
                valid_management_socket(&area, runtime.listen_addr()).await,
                &key,
                &client_id,
                node_public_key(&runtime),
            )
            .await;
            let command = if cached {
                let other = SigningKey::from_slice(&[0x57; 32]).expect("other key");
                let other_id = pair_browser(&runtime, &other, "Cached revoke target").await;
                let command = serde_json::json!({
                    "v":1,"type":"command","requestId":Id128::new([0x58;16]),
                    "action":"device.revoke","payload":{"clientId":other_id}
                });
                send_json(&mut socket, &command).await;
                assert_eq!(
                    next_json(&mut socket, Duration::from_secs(2)).await["ok"],
                    true
                );
                command
            } else {
                serde_json::json!({
                    "v":1,"type":"command","requestId":Id128::new([0x52;16]),
                    "action":"node.info","payload":{}
                })
            };
            let management = runtime.management.as_ref().expect("management");
            let before = management.boundary_counts();
            let barrier = management.pause_next_boundary(if at_enqueue {
                ManagementBoundary::Enqueue
            } else {
                ManagementBoundary::Admission
            });
            send_json(&mut socket, &command).await;
            timeout(Duration::from_secs(2), barrier.wait_until_entered())
                .await
                .expect("reaches boundary");
            ipc::device_revoke(runtime.ipc_address(), &client_id)
                .await
                .expect("commit revoke first");
            barrier.release();
            assert_eq!(
                next_close(&mut socket, Duration::from_secs(3)).await.0,
                4410
            );
            let after = management.boundary_counts();
            assert_eq!(after.0, before.0 + usize::from(at_enqueue));
            assert_eq!(after.1, before.1, "no result enqueued after revoke commit");
            runtime.shutdown(None).await.expect("cleanup");
        }
    }

    #[tokio::test]
    async fn initial_subscription_does_not_consume_a_newer_revision_unseen() {
        use crate::management_ws::ManagementBoundary;
        let area = TestArea::new();
        let runtime = NodeRuntime::start(area.config(free_address()), &FakeProtector::default())
            .await
            .expect("start");
        let key = SigningKey::from_slice(&[0x53; 32]).expect("key");
        let client_id = pair_browser(&runtime, &key, "Subscriber").await;
        let mut socket = authenticate_socket(
            valid_management_socket(&area, runtime.listen_addr()).await,
            &key,
            &client_id,
            node_public_key(&runtime),
        )
        .await;
        let barrier = runtime
            .management
            .as_ref()
            .expect("management")
            .pause_next_boundary(ManagementBoundary::SubscriptionRevision);
        send_json(
            &mut socket,
            &serde_json::json!({
                "v":1,"type":"subscribe","requestId":Id128::new([0x54;16]),"resource":"devices"
            }),
        )
        .await;
        timeout(Duration::from_secs(2), barrier.wait_until_entered())
            .await
            .expect("SQL revision read");
        let other = SigningKey::from_slice(&[0x55; 32]).expect("other key");
        pair_browser(&runtime, &other, "Concurrent new trust").await;
        barrier.release();
        assert_eq!(
            next_json(&mut socket, Duration::from_secs(2)).await["ok"],
            true
        );
        let old = next_json(&mut socket, Duration::from_secs(2)).await;
        let latest = next_json(&mut socket, Duration::from_secs(2)).await;
        assert_eq!(old["type"], "devices.changed");
        assert_eq!(old["revision"], 1);
        assert_eq!(latest["type"], "devices.changed");
        assert_eq!(latest["revision"], 2);
        assert_eq!(latest.as_object().expect("notification").len(), 3);
        send_json(
            &mut socket,
            &serde_json::json!({
                "v":1,"type":"command","requestId":Id128::new([0x56;16]),
                "action":"device.list","payload":{"cursor":null}
            }),
        )
        .await;
        let page = next_json(&mut socket, Duration::from_secs(2)).await;
        assert_eq!(page["result"]["revision"], 2);
        assert_eq!(page["result"]["items"].as_array().expect("items").len(), 2);
        runtime.shutdown(None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn paused_real_writers_close_at_the_byte_or_frame_queue_budget() {
        for byte_first in [true, false] {
            let area = TestArea::new();
            let runtime =
                NodeRuntime::start(area.config(free_address()), &FakeProtector::default())
                    .await
                    .expect("start");
            let key = SigningKey::from_slice(&[0x74; 32]).expect("key");
            let client_id = pair_browser(&runtime, &key, "Slow reader fixture").await;
            if byte_first {
                for value in 0x81_u8..0xA0 {
                    let fixture_key = SigningKey::from_slice(&[value; 32]).expect("fixture key");
                    sqlx::query("INSERT INTO clients(client_id,public_key,display_name,status,auth_generation,created_at) VALUES(?,?,?,'trusted',1,1700000000)")
                        .bind(Id128::new([value;16]).to_base64url())
                        .bind(fixture_key.verifying_key().to_sec1_point(false).as_bytes())
                        .bind("🦀".repeat(80))
                        .execute(runtime.storage.as_ref().expect("storage").pool_for_test())
                        .await.expect("bounded large-page fixture");
                }
            }
            let mut socket = authenticate_socket(
                valid_management_socket(&area, runtime.listen_addr()).await,
                &key,
                &client_id,
                node_public_key(&runtime),
            )
            .await;
            let management = runtime.management.as_ref().expect("management");
            // Only the frame-first test adapts the command window, which is
            // independently tested. No production budget or environment switch changes.
            management.slow_writer_fixture(!byte_first);
            let writer = management.pause_next_write();
            send_json(
                &mut socket,
                &serde_json::json!({"v":1,"type":"command",
                "requestId":Id128::new([0x75;16]),"action":"node.info","payload":{}}),
            )
            .await;
            timeout(Duration::from_secs(2), writer.wait_until_entered())
                .await
                .expect("real writer paused");
            let requests = if byte_first { 119_u16 } else { 257_u16 };
            for index in 1..=requests {
                send_json(&mut socket, &serde_json::json!({"v":1,"type":"command",
                    "requestId":Id128::new(u128::from(index).to_be_bytes()),
                    "action":if byte_first {"device.list"} else {"node.info"},
                    "payload":if byte_first {serde_json::json!({"cursor":null})} else {serde_json::json!({})}
                })).await;
            }
            timeout(Duration::from_secs(1), async {
                while management.queue_failure_snapshot().3 == 0 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("queue limit, not external read timeout");
            let (bytes, frames, rejected, code) = management.queue_failure_snapshot();
            assert_eq!(code, 4429);
            if byte_first {
                assert!(frames < 256 && frames > 0);
                assert!(bytes <= 1_048_576 && bytes + rejected > 1_048_576);
            } else {
                assert_eq!(frames, 256);
                assert!(bytes < 1_048_576);
            }
            writer.release();
            let close = timeout(Duration::from_secs(3), async {
                loop {
                    match socket.next().await.expect("frame").expect("transport") {
                        Message::Close(Some(frame)) => break frame,
                        Message::Text(_) | Message::Ping(_) | Message::Pong(_) => {}
                        _ => panic!("unexpected frame"),
                    }
                }
            })
            .await
            .expect("actual socket close");
            assert_eq!(u16::from(close.code), 4429);
            runtime.shutdown(None).await.expect("cleanup");
        }
    }

    #[tokio::test]
    async fn writer_holding_the_actor_gate_finishes_before_a_competing_revoke_commits() {
        use crate::management_ws::ManagementBoundary;
        let area = TestArea::new();
        let runtime = NodeRuntime::start(area.config(free_address()), &FakeProtector::default())
            .await
            .expect("start");
        let key = SigningKey::from_slice(&[0x76; 32]).expect("key");
        let client_id = pair_browser(&runtime, &key, "Writer-first fixture").await;
        let mut socket = authenticate_socket(
            valid_management_socket(&area, runtime.listen_addr()).await,
            &key,
            &client_id,
            node_public_key(&runtime),
        )
        .await;
        let management = runtime.management.as_ref().expect("management");
        let before = management.protected_write_count();
        let barrier = management.pause_next_boundary(ManagementBoundary::WriteLocked);
        send_json(
            &mut socket,
            &serde_json::json!({
                "v":1,"type":"command","requestId":Id128::new([0x77;16]),
                "action":"node.info","payload":{}
            }),
        )
        .await;
        timeout(Duration::from_secs(2), barrier.wait_until_entered())
            .await
            .expect("writer owns actor gate");
        let mut revoke = Box::pin(ipc::device_revoke(runtime.ipc_address(), &client_id));
        assert!(
            timeout(Duration::from_millis(50), &mut revoke)
                .await
                .is_err(),
            "revoke cannot commit across the active sender"
        );
        let status: String = sqlx::query_scalar("SELECT status FROM clients WHERE client_id = ?")
            .bind(&client_id)
            .fetch_one(runtime.storage.as_ref().expect("storage").pool_for_test())
            .await
            .expect("state while gate held");
        assert_eq!(status, "trusted");
        assert_eq!(management.protected_write_count(), before);
        barrier.release();
        timeout(Duration::from_secs(2), &mut revoke)
            .await
            .expect("revoke after send")
            .expect("commit");
        assert_eq!(
            management.protected_write_count(),
            before + 1,
            "sink.send completed under gate before COMMIT"
        );
        let response = next_json(&mut socket, Duration::from_secs(2)).await;
        assert_eq!(response["ok"], true);
        assert_eq!(response["requestId"], Id128::new([0x77; 16]).to_base64url());
        assert_eq!(
            next_close(&mut socket, Duration::from_secs(2)).await.0,
            4410
        );
        drop(revoke);
        runtime.shutdown(None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn revoke_commit_discards_an_already_queued_management_result() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let runtime = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("node starts");
        let listen = runtime.listen_addr();
        let node_key = node_public_key(&runtime);
        let client_key = SigningKey::from_slice(&[20_u8; 32]).expect("client key");
        let client_id = pair_browser(&runtime, &client_key, "Revoked browser").await;
        let mut socket = authenticate_socket(
            valid_management_socket(&area, listen).await,
            &client_key,
            &client_id,
            node_key,
        )
        .await;

        let write_barrier = runtime
            .management
            .as_ref()
            .expect("management service")
            .pause_next_write();
        send_json(
            &mut socket,
            &serde_json::json!({
                "v": 1,
                "type": "command",
                "requestId": Id128::new([113; 16]),
                "action": "node.info",
                "payload": {}
            }),
        )
        .await;
        timeout(Duration::from_secs(2), write_barrier.wait_until_entered())
            .await
            .expect("queued result reaches the write boundary");

        let revoked = ipc::device_revoke(runtime.ipc_address(), &client_id)
            .await
            .expect("owner revoke");
        assert_eq!(revoked.client_id, client_id);
        write_barrier.release();

        let (code, reason) = next_close(&mut socket, Duration::from_secs(3)).await;
        assert_eq!(code, 4410);
        assert_eq!(reason, "revoked");
        runtime.shutdown(None).await.expect("clean shutdown");
    }

    #[tokio::test]
    async fn run_observes_a_stop_requested_before_subscription() {
        let area = TestArea::new();
        let runtime = NodeRuntime::start(area.config(free_address()), &FakeProtector::default())
            .await
            .expect("node starts");
        runtime.control.request_stop();
        timeout(Duration::from_secs(3), runtime.run())
            .await
            .expect("preexisting stop must not be lost")
            .expect("clean shutdown");
    }

    #[tokio::test]
    async fn wait_for_stop_observes_a_failure_before_subscription() {
        let area = TestArea::new();
        let runtime = NodeRuntime::start(area.config(free_address()), &FakeProtector::default())
            .await
            .expect("node starts");
        runtime
            .control
            .mark_failed("injected failure before subscription");
        let error = timeout(Duration::from_secs(3), runtime.wait_for_stop())
            .await
            .expect("preexisting failure must not be lost")
            .expect_err("failure must remain visible");
        assert!(
            error
                .to_string()
                .contains("injected failure before subscription"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn shutdown_deadline_bounds_storage_close_and_reports_failure() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let runtime = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("node starts");
        let close_barrier = runtime
            .storage
            .as_ref()
            .expect("node storage")
            .pause_next_close();
        let mut stop_signal = runtime.control.stop_receiver();
        let ipc_address = runtime.ipc_address().clone();
        let stop_task = tokio::spawn(async move { ipc::stop(&ipc_address).await });
        timeout(Duration::from_secs(2), stop_signal.changed())
            .await
            .expect("owner stop request reaches the node")
            .expect("owner stop signal remains available");
        assert!(*stop_signal.borrow());
        let shutdown =
            tokio::spawn(runtime.shutdown_with_deadline(None, Duration::from_millis(250)));
        let error = stop_task
            .await
            .expect("owner stop task")
            .expect_err("timed out cleanup must not report success");
        assert!(error.to_string().contains("closing storage"), "{error}");
        let shutdown_error = shutdown
            .await
            .expect("shutdown task")
            .expect_err("shutdown reports the cleanup failure");
        assert!(
            shutdown_error.to_string().contains("closing storage"),
            "{shutdown_error}"
        );
        close_barrier.release();
    }

    #[tokio::test]
    async fn shutdown_does_not_publish_a_report_before_the_ipc_task_finishes() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let mut runtime = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("node starts");
        let finish_barrier = Arc::new(crate::test_support::AsyncBarrier::default());
        let task_barrier = Arc::clone(&finish_barrier);
        if let Some(task) = runtime.ipc_task.take() {
            task.abort();
            let _ = task.await;
        }
        runtime.ipc_task = Some(tokio::spawn(async move {
            task_barrier.pause().await;
            Err(AppError::Ipc(
                "injected owner control finalization failure".to_owned(),
            ))
        }));
        let mut report = runtime.control.shutdown_receiver();
        let shutdown = tokio::spawn(runtime.shutdown_with_deadline(None, Duration::from_secs(5)));

        timeout(Duration::from_secs(2), finish_barrier.wait_until_entered())
            .await
            .expect("shutdown reached the owner-control finalization boundary");
        assert_eq!(*report.borrow(), ShutdownResult::Pending);

        finish_barrier.release();
        let error = shutdown
            .await
            .expect("shutdown task")
            .expect_err("owner-control finalization failure is reported");
        assert!(
            error
                .to_string()
                .contains("injected owner control finalization failure"),
            "{error}"
        );
        timeout(Duration::from_secs(1), report.changed())
            .await
            .expect("shutdown report")
            .expect("report sender");
        assert!(
            matches!(
                &*report.borrow(),
                ShutdownResult::Failed(message)
                    if message.contains("injected owner control finalization failure")
            ),
            "{:?}",
            *report.borrow()
        );
    }

    #[allow(clippy::too_many_lines)]
    #[tokio::test]
    async fn management_websocket_authenticates_queries_subscribes_and_revokes_devices() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let runtime = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("node starts");
        let listen = runtime.listen_addr();
        let node_key = node_public_key(&runtime);
        let primary_key = SigningKey::from_slice(&[14_u8; 32]).expect("primary client key");
        let target_key = SigningKey::from_slice(&[15_u8; 32]).expect("target client key");
        let primary_id = pair_browser(&runtime, &primary_key, "Browser A").await;
        let target_id = pair_browser(&runtime, &target_key, "Browser B").await;

        let mut primary_socket = authenticate_socket(
            valid_management_socket(&area, listen).await,
            &primary_key,
            &primary_id,
            node_key,
        )
        .await;
        let mut target_socket = authenticate_socket(
            valid_management_socket(&area, listen).await,
            &target_key,
            &target_id,
            node_key,
        )
        .await;

        send_json(
            &mut primary_socket,
            &serde_json::json!({
                "v": 1,
                "type": "command",
                "requestId": Id128::new([100; 16]),
                "action": "node.info",
                "payload": {}
            }),
        )
        .await;
        let info = next_json(&mut primary_socket, Duration::from_secs(2)).await;
        assert_eq!(info["v"], 1);
        assert_eq!(info["type"], "result");
        assert_eq!(info["ok"], true);
        assert_eq!(info["error"], serde_json::Value::Null);
        assert_eq!(info["result"]["nodeId"], runtime.node_id());
        assert_eq!(info["result"]["origin"], "https://node.example.test");
        assert_eq!(info["result"]["status"], "ready");
        assert_eq!(
            info["result"]["publicKey"],
            serde_json::to_value(node_key).expect("node key JSON")
        );

        send_json(
            &mut primary_socket,
            &serde_json::json!({
                "v": 1,
                "type": "command",
                "requestId": Id128::new([101; 16]),
                "action": "device.list",
                "payload": {"cursor": null}
            }),
        )
        .await;
        let page = next_json(&mut primary_socket, Duration::from_secs(2)).await;
        assert_eq!(page["ok"], true);
        assert_eq!(page["result"]["items"].as_array().map(Vec::len), Some(2));
        assert_eq!(page["result"]["nextCursor"], serde_json::Value::Null);
        assert!(
            page["result"]["items"]
                .as_array()
                .expect("items")
                .iter()
                .any(|item| item["clientId"] == primary_id)
        );
        assert!(
            page["result"]["items"]
                .as_array()
                .expect("items")
                .iter()
                .any(|item| item["clientId"] == target_id)
        );

        send_json(
            &mut primary_socket,
            &serde_json::json!({
                "v": 1,
                "type": "subscribe",
                "requestId": Id128::new([102; 16]),
                "resource": "devices"
            }),
        )
        .await;
        let subscription = next_json(&mut primary_socket, Duration::from_secs(2)).await;
        assert_eq!(subscription["ok"], true);
        assert_eq!(subscription["result"]["resource"].as_str(), Some("devices"));
        let initial_revision = page["result"]["revision"].as_u64().expect("revision");
        let changed = next_json(&mut primary_socket, Duration::from_secs(2)).await;
        assert_eq!(changed["type"], "devices.changed");
        assert_eq!(changed["revision"].as_u64(), Some(initial_revision));

        send_json(
            &mut primary_socket,
            &serde_json::json!({
                "v": 1,
                "type": "command",
                "requestId": Id128::new([103; 16]),
                "action": "device.revoke",
                "payload": {"clientId": target_id}
            }),
        )
        .await;
        let revoked = next_json(&mut primary_socket, Duration::from_secs(2)).await;
        assert_eq!(revoked["ok"], true);
        assert_eq!(revoked["result"]["clientId"], target_id);
        assert_eq!(revoked["result"]["status"], "revoked");
        let changed = next_json(&mut primary_socket, Duration::from_secs(2)).await;
        assert_eq!(changed["type"], "devices.changed");
        assert!(changed["revision"].as_u64().expect("revision") > initial_revision);

        send_json(
            &mut primary_socket,
            &serde_json::json!({
                "v": 1,
                "type": "command",
                "requestId": Id128::new([104; 16]),
                "action": "node.info",
                "payload": {}
            }),
        )
        .await;
        let still_connected = next_json(&mut primary_socket, Duration::from_secs(2)).await;
        assert_eq!(still_connected["ok"], true);

        let (code, reason) = next_close(&mut target_socket, Duration::from_secs(5)).await;
        assert_eq!(code, 4410);
        assert_eq!(reason, "revoked");

        let mut revoked_socket = valid_management_socket(&area, listen).await;
        prove_client(&mut revoked_socket, &target_key, &target_id, node_key).await;
        let (code, reason) = next_close(&mut revoked_socket, Duration::from_secs(2)).await;
        assert_eq!(code, 4401);
        assert_eq!(reason, "authentication failed");

        runtime.shutdown(None).await.expect("clean shutdown");
    }

    #[tokio::test]
    async fn management_websocket_recovers_identity_and_rejects_replayed_proofs() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let runtime = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("node starts");
        let listen = runtime.listen_addr();
        let node_key = node_public_key(&runtime);
        let client_key = SigningKey::from_slice(&[16_u8; 32]).expect("client key");
        let client_id = pair_browser(&runtime, &client_key, "Recovered browser").await;

        let mut first = valid_management_socket(&area, listen).await;
        let (first_proof, first_compact) =
            prove_client_with_id(&mut first, &client_key, Some(&client_id), node_key).await;
        let authenticated = next_json(&mut first, Duration::from_secs(2)).await;
        assert_eq!(authenticated["type"], "authenticated");
        assert_eq!(authenticated["clientId"], client_id);
        let connection_id = first_proof.connection_id.to_base64url();
        assert_eq!(
            authenticated["connectionId"].as_str(),
            Some(connection_id.as_str())
        );

        send_json(
            &mut first,
            &serde_json::json!({"v": 1, "type": "proof", "proof": first_compact}),
        )
        .await;
        let (code, reason) = next_close(&mut first, Duration::from_secs(2)).await;
        assert_eq!(code, 4400);
        assert_eq!(reason, "invalid frame");

        let mut recovered = valid_management_socket(&area, listen).await;
        prove_client_with_id(&mut recovered, &client_key, None, node_key).await;
        let recovered_message = next_json(&mut recovered, Duration::from_secs(2)).await;
        assert_eq!(recovered_message["type"], "authenticated");
        assert_eq!(recovered_message["clientId"], client_id);

        let mut replay = valid_management_socket(&area, listen).await;
        send_json(
            &mut replay,
            &serde_json::json!({
                "v": 1,
                "type": "hello",
                "clientId": client_id,
                "clientPublicKey": client_public_key(&client_key),
                "clientNonce": Nonce256::new([0xA6; 32]),
            }),
        )
        .await;
        let challenge = next_json(&mut replay, Duration::from_secs(2)).await;
        assert_eq!(challenge["type"], "challenge");
        send_json(
            &mut replay,
            &serde_json::json!({"v": 1, "type": "proof", "proof": first_compact}),
        )
        .await;
        let (code, reason) = next_close(&mut replay, Duration::from_secs(2)).await;
        assert_eq!(code, 4401);
        assert_eq!(reason, "authentication failed");

        recovered.close(None).await.expect("close recovered socket");
        runtime.shutdown(None).await.expect("clean shutdown");
    }

    #[tokio::test]
    async fn identity_survives_restart_and_resources_are_reusable() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let listen = free_address();

        let first = NodeRuntime::start(area.config(listen), &protector)
            .await
            .expect("first start");
        let first_id = first.node_id().to_owned();
        first.shutdown(None).await.expect("first shutdown");

        let second = NodeRuntime::start(area.config(listen), &protector)
            .await
            .expect("restart on released port and lock");
        assert_eq!(second.node_id(), first_id);
        second.shutdown(None).await.expect("second shutdown");
    }

    #[tokio::test]
    async fn rejects_a_second_instance_for_the_same_data_directory() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let first = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("first node");
        let error = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .err()
            .expect("second node must fail");
        assert!(matches!(error, AppError::DataDirectoryBusy));
        first.shutdown(None).await.expect("first shutdown");
    }

    #[tokio::test]
    async fn invalid_tls_and_occupied_port_fail_without_exhausting_resources() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        std::fs::write(&area.certificate, b"not a certificate").expect("invalid certificate");
        let error = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .err()
            .expect("invalid TLS must fail");
        assert!(matches!(error, AppError::Tls(_)));

        let (certificate, private_key) = write_certificate(&area.directory);
        let mut valid = area.config(free_address());
        valid.tls_cert = certificate;
        valid.tls_key = private_key;
        let blocker = TcpListener::bind(valid.listen).await.expect("occupy port");
        let error = NodeRuntime::start(valid.clone(), &protector)
            .await
            .err()
            .expect("occupied port must fail");
        assert!(matches!(
            error,
            AppError::Io {
                context: "binding the HTTPS listener",
                ..
            }
        ));

        drop(blocker);
        let runtime = NodeRuntime::start(valid, &protector)
            .await
            .expect("recovered start");
        runtime.shutdown(None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn corrupt_existing_identity_fails_without_replacement() {
        let area = TestArea::new();
        let protector = FakeProtector::default();
        let runtime = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .expect("first start");
        runtime.shutdown(None).await.expect("shutdown");

        let key_path = area.data_dir().join("identity.key");
        let corrupt = b"corrupt identity material";
        std::fs::write(&key_path, corrupt).expect("corrupt identity");
        let error = NodeRuntime::start(area.config(free_address()), &protector)
            .await
            .err()
            .expect("corrupt identity must fail");
        assert!(matches!(error, AppError::Identity(_)), "{error:?}");
        assert_eq!(
            std::fs::read(&key_path).expect("identity after failure"),
            corrupt
        );
    }

    #[tokio::test]
    async fn unusable_data_directory_fails_before_startup() {
        let area = TestArea::new();
        let file = area.directory.join("not-a-directory");
        std::fs::write(&file, b"occupied").expect("data path file");
        let mut config = area.config(free_address());
        config.data_dir = file;
        let error = NodeRuntime::start(config, &FakeProtector::default())
            .await
            .err()
            .expect("file as data directory must fail");
        assert!(matches!(error, AppError::Io { .. }));
    }

    #[test]
    fn test_addresses_use_loopback() {
        let address = free_address();
        assert_eq!(address.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    }
}
