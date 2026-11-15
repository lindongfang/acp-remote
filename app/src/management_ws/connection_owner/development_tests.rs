#![allow(
    clippy::too_many_lines,
    clippy::significant_drop_tightening,
    reason = "Development scenarios keep the real TLS/authentication/source/read and original cleanup handles visible in causal order"
)]
//! Author development checks: real TLS, management proof and original socket writer.
//! The cfg(test) attachment uses a real module source/read; it is not a production `ViewPort`.
use super::*;
use crate::{claims::verify_auth_host, jws::sign, storage::Storage};
use p256::ecdsa::SigningKey;
use rustls::pki_types::{PrivatePkcs8KeyDer, ServerName};
use std::path::PathBuf;
use tokio::{
    net::{TcpListener, TcpSocket, TcpStream},
    sync::oneshot,
    task::JoinHandle,
};
use tokio_rustls::{TlsAcceptor, TlsConnector};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{ClientRequestBuilder, Message as PeerMessage},
};
type Peer = WebSocketStream<tokio_rustls::client::TlsStream<TcpStream>>;
struct Listener {
    tcp: TcpListener,
    tls: TlsAcceptor,
    small: bool,
}
// cfg(test)-only FFI on this fixture's owned accepted socket. This changes
// actual kernel buffering, never the real Sink poll result or production IO.
#[cfg(windows)]
fn constrain_accepted_send_buffer(tcp: &TcpStream) {
    use std::os::windows::io::AsRawSocket;
    #[link(name = "Ws2_32")]
    unsafe extern "system" {
        fn setsockopt(socket: usize, level: i32, name: i32, value: *const u8, len: i32) -> i32;
        fn getsockopt(socket: usize, level: i32, name: i32, value: *mut u8, len: *mut i32) -> i32;
    }
    let size = 1024i32;
    let mut actual = -1i32;
    let mut len = 4i32;
    // SAFETY: the fixture retains the live socket; both pointers refer to i32
    // values for their complete four-byte Winsock option calls, without aliasing.
    unsafe {
        assert_eq!(
            setsockopt(
                usize::try_from(tcp.as_raw_socket()).unwrap(),
                0xffff,
                0x1001,
                (&raw const size).cast(),
                4
            ),
            0
        );
        assert_eq!(
            getsockopt(
                usize::try_from(tcp.as_raw_socket()).unwrap(),
                0xffff,
                0x1001,
                (&raw mut actual).cast(),
                &raw mut len
            ),
            0
        );
    }
    assert_eq!(
        actual, size,
        "accepted socket must use exact small kernel send buffer"
    );
    eprintln!("real TCP fixture: accepted SO_SNDBUF={actual}");
}
#[cfg(not(windows))]
fn constrain_accepted_send_buffer(_: &TcpStream) {
    panic!("this author slow-peer fixture requires Windows actual socket setup")
}
impl axum::serve::Listener for Listener {
    type Io = tokio_rustls::server::TlsStream<TcpStream>;
    type Addr = SocketAddr;
    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (t, p) = self.tcp.accept().await.expect("TCP accept");
            if self.small {
                constrain_accepted_send_buffer(&t);
            }
            if let Ok(t) = self.tls.accept(t).await {
                return (t, p);
            }
        }
    }
    fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.tcp.local_addr()
    }
}
struct Harness {
    observer: Option<JoinHandle<Option<connection_owner::ConnectionSettlement>>>,
    observations: mpsc::UnboundedReceiver<ConnectionObservation>,
    database: PathBuf,
    storage: Storage,
    service: ManagementService,
    peer: Peer,
    stop: oneshot::Sender<()>,
    server: JoinHandle<()>,
    client_id: Id128,
    address: SocketAddr,
    tls_config: Arc<rustls::ClientConfig>,
}
impl Harness {
    async fn open() -> Self {
        Self::open_with_small_buffers(false).await
    }
    async fn open_with_small_buffers(small: bool) -> Self {
        let base = std::env::var_os("ACP_SOCKET_TEST_DATA_ROOT").map_or_else(
            || std::env::temp_dir().join("acpr-shared-socket-author"),
            PathBuf::from,
        );
        std::fs::create_dir_all(&base).unwrap();
        let area = base.join(Id128::new(crate::random::bytes().unwrap()).to_base64url());
        std::fs::create_dir(&area).unwrap();
        let storage = Storage::open(&area.join("test.db"))
            .await
            .expect("in-memory store");
        let trust = storage.trust_store();
        let identity = Arc::new(NodeIdentity::for_test([93; 32]));
        let key = SigningKey::from_slice(&[94; 32]).unwrap();
        let client_id = Id128::new([95; 16]);
        let public = PublicKeySec1::new(
            key.verifying_key()
                .to_sec1_point(false)
                .as_bytes()
                .try_into()
                .unwrap(),
        )
        .unwrap();
        trust
            .register_client(
                &client_id.to_base64url(),
                public.as_bytes(),
                "socket author test",
            )
            .await
            .unwrap();
        let origin = "https://node.example.test".to_owned();
        let pairing =
            Arc::new(PairingService::new(identity.clone(), origin.clone(), trust.clone()).unwrap());
        let (control, _) = IpcControl::new(identity.node_id().to_owned(), origin.clone());
        let policy = Arc::new(ApplicationPolicy::new(
            identity.node_id().to_owned(),
            trust.clone(),
            &pairing,
            control.clone(),
        ));
        let nodekey = PublicKeySec1::new(identity.public_key().try_into().unwrap()).unwrap();
        let service = ManagementService::new(identity, origin, trust, pairing, control, policy);
        // The service and policy must use the original same trust coordinator.
        let service = service.expect("management service");
        let (accept_tx, accept_rx) = oneshot::channel();
        *service.state.socket_test_accept.lock().unwrap() = Some(accept_tx);
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
        let der = cert.cert.der().clone();
        let private = PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der());
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![der.clone()], private.into())
            .unwrap();
        let mut roots = rustls::RootCertStore::empty();
        roots.add(der).unwrap();
        let client = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let tcp = if small {
            let socket = TcpSocket::new_v4().unwrap();
            socket.set_send_buffer_size(1024).unwrap();
            socket.bind("127.0.0.1:0".parse().unwrap()).unwrap();
            socket.listen(8).unwrap()
        } else {
            TcpListener::bind("127.0.0.1:0").await.unwrap()
        };
        let addr = tcp.local_addr().unwrap();
        let listener = Listener {
            tcp,
            tls: TlsAcceptor::from(Arc::new(config)),
            small,
        };
        let (observer_tx, mut observer_rx) = mpsc::unbounded_channel();
        let router = Router::new()
            .route(CONTROL_PATH, get(observer_upgrade))
            .with_state((service.state.clone(), observer_tx))
            .nest("/production", service.router())
            .layer(Extension(PeerAddress(addr)));
        let (stop, stopped) = oneshot::channel();
        let server = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
                .unwrap();
        });
        let tls_config = Arc::new(client);
        let peer_tcp = if small {
            let socket = TcpSocket::new_v4().unwrap();
            socket.set_recv_buffer_size(8192).unwrap();
            socket.connect(addr).await.unwrap()
        } else {
            TcpStream::connect(addr).await.unwrap()
        };
        let tls = TlsConnector::from(tls_config.clone())
            .connect(ServerName::try_from("localhost").unwrap(), peer_tcp)
            .await
            .unwrap();
        let request = ClientRequestBuilder::new(
            format!("wss://node.example.test{CONTROL_PATH}")
                .parse()
                .unwrap(),
        )
        .with_sub_protocol(CONTROL_PROTOCOL)
        .with_header("Origin", "https://node.example.test");
        let (mut peer, _) = tokio_tungstenite::client_async(request, tls).await.unwrap();
        peer.send(PeerMessage::Text(serde_json::json!({"v":1,"type":"hello","clientId":client_id,"clientPublicKey":public,"clientNonce":Nonce256::new([96;32])}).to_string().into())).await.unwrap();
        let challenge = next_json(&mut peer).await;
        assert_eq!(challenge["type"], "challenge");
        let proof = verify_auth_host(challenge["hostProof"].as_str().unwrap(), &nodekey).unwrap();
        let signed = sign(
            JwsType::AuthClient,
            &serde_json::to_vec(&proof).unwrap(),
            &key,
        )
        .unwrap();
        peer.send(PeerMessage::Text(
            serde_json::json!({"v":1,"type":"proof","proof":signed})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
        assert_eq!(next_json(&mut peer).await["type"], "authenticated");
        let (_attachment, _outbound) = timeout(Duration::from_secs(5), accept_rx)
            .await
            .unwrap()
            .unwrap();
        Self {
            observer: Some(spawn_observation(
                timeout(Duration::from_secs(5), observer_rx.recv())
                    .await
                    .unwrap()
                    .unwrap(),
            )),
            observations: observer_rx,
            database: area.join("test.db"),
            storage,
            service,
            peer,
            stop,
            server,
            client_id,
            address: addr,
            tls_config,
        }
    }
    async fn same_actor_second_socket(
        &mut self,
    ) -> (Peer, Arc<SocketAttachment>, ConnectionObservation) {
        let (tx, rx) = oneshot::channel();
        *self.service.state.socket_test_accept.lock().unwrap() = Some(tx);
        let tls = TlsConnector::from(self.tls_config.clone())
            .connect(
                ServerName::try_from("localhost").unwrap(),
                TcpStream::connect(self.address).await.unwrap(),
            )
            .await
            .unwrap();
        let request = ClientRequestBuilder::new(
            format!("wss://node.example.test{CONTROL_PATH}")
                .parse()
                .unwrap(),
        )
        .with_sub_protocol(CONTROL_PROTOCOL)
        .with_header("Origin", "https://node.example.test");
        let (mut peer, _) = tokio_tungstenite::client_async(request, tls).await.unwrap();
        let key = SigningKey::from_slice(&[94; 32]).unwrap();
        let public = PublicKeySec1::new(
            key.verifying_key()
                .to_sec1_point(false)
                .as_bytes()
                .try_into()
                .unwrap(),
        )
        .unwrap();
        peer.send(PeerMessage::Text(serde_json::json!({"v":1,"type":"hello","clientId":self.client_id,"clientPublicKey":public,"clientNonce":Nonce256::new([101;32])}).to_string().into())).await.unwrap();
        let challenge = next_json(&mut peer).await;
        let proof = verify_auth_host(
            challenge["hostProof"].as_str().unwrap(),
            &self.service.state.node_public_key,
        )
        .unwrap();
        let signed = sign(
            JwsType::AuthClient,
            &serde_json::to_vec(&proof).unwrap(),
            &key,
        )
        .unwrap();
        peer.send(PeerMessage::Text(
            serde_json::json!({"v":1,"type":"proof","proof":signed})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
        assert_eq!(next_json(&mut peer).await["type"], "authenticated");
        let (attachment, _) = timeout(Duration::from_secs(5), rx).await.unwrap().unwrap();
        let observation = timeout(Duration::from_secs(5), self.observations.recv())
            .await
            .unwrap()
            .unwrap();
        (peer, attachment, observation)
    }
    async fn finish(mut self) {
        let peer = timeout(Duration::from_secs(5), self.peer.close(None)).await;
        drop(self.peer);
        self.service.begin_shutdown();
        let connections = timeout(Duration::from_secs(5), self.service.wait_closed()).await;
        let _ = self.stop.send(());
        let server = timeout(Duration::from_secs(5), &mut self.server).await;
        let server_finished = if let Ok(result) = server {
            result.is_ok()
        } else {
            self.server.abort();
            let _ = self.server.await;
            false
        };
        if let Some(mut observer) = self.observer.take() {
            if let Ok(result) = timeout(Duration::from_secs(5), &mut observer).await {
                result.unwrap();
            } else {
                observer.abort();
                let _ = observer.await;
                panic!("observer join timed out");
            }
        }
        self.storage.close().await;
        assert!(peer.is_ok(), "peer close deadline");
        assert!(connections.is_ok(), "original connections close deadline");
        assert!(server_finished, "original server join deadline/error");
    }
}
async fn next_json(peer: &mut Peer) -> Value {
    loop {
        match timeout(Duration::from_secs(5), peer.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
        {
            PeerMessage::Text(s) => return serde_json::from_str(&s).unwrap(),
            PeerMessage::Ping(p) => peer.send(PeerMessage::Pong(p)).await.unwrap(),
            other => panic!("unexpected auth message {other:?}"),
        }
    }
}

type ObserverSender = mpsc::UnboundedSender<ConnectionObservation>;
async fn observer_upgrade(
    State((state, observer)): State<(Arc<ManagementState>, ObserverSender)>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.protocols([CONTROL_PROTOCOL])
        .on_upgrade(move |socket| {
            let lease = state.connections.acquire().unwrap();
            let mut shutdown = state.connections.shutdown_receiver();
            let observation = serve_connection(socket, state, lease, &mut shutdown);
            assert!(observer.send(observation).is_ok());
            async {}
        })
        .into_response()
}
fn spawn_observation(
    mut observation: ConnectionObservation,
) -> JoinHandle<Option<connection_owner::ConnectionSettlement>> {
    tokio::spawn(async move {
        (&mut observation).await;
        observation.settlement()
    })
}
impl Harness {
    async fn raw_socket(
        &self,
        production: bool,
    ) -> Result<Peer, tokio_tungstenite::tungstenite::Error> {
        let tls = TlsConnector::from(self.tls_config.clone())
            .connect(
                ServerName::try_from("localhost").unwrap(),
                TcpStream::connect(self.address).await.unwrap(),
            )
            .await
            .unwrap();
        let prefix = if production { "/production" } else { "" };
        let request = ClientRequestBuilder::new(
            format!("wss://node.example.test{prefix}{CONTROL_PATH}")
                .parse()
                .unwrap(),
        )
        .with_sub_protocol(CONTROL_PROTOCOL)
        .with_header("Origin", "https://node.example.test");
        tokio_tungstenite::client_async(request, tls)
            .await
            .map(|(peer, _)| peer)
    }
}
async fn close_code(peer: &mut Peer) -> (u16, String) {
    match timeout(Duration::from_secs(5), peer.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
    {
        PeerMessage::Close(Some(frame)) => (frame.code.into(), frame.reason.to_string()),
        other => panic!("expected original close, got {other:?}"),
    }
}
async fn wait_for(mut predicate: impl FnMut() -> bool) {
    timeout(Duration::from_secs(5), async {
        while !predicate() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connection_owner_development_cancel_waiter_retains_real_writer() {
    let mut h = Harness::open().await;
    let barrier = h.service.pause_next_write();
    h.peer.send(PeerMessage::Text(serde_json::json!({"v":1,"type":"command","requestId":Id128::new([113;16]),"action":"node.info","payload":{}}).to_string().into())).await.unwrap();
    timeout(Duration::from_secs(5), barrier.wait_until_entered())
        .await
        .unwrap();
    let observer = h.observer.take().unwrap();
    observer.abort();
    assert!(observer.await.unwrap_err().is_cancelled());
    let premature = timeout(Duration::from_millis(150), h.service.wait_closed())
        .await
        .is_ok();
    barrier.release();
    h.finish().await;
    assert!(
        !premature,
        "actual writer was blocked, but registry prematurely reported empty after observer cancellation"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connection_owner_development_unauthenticated_drop_and_other_socket_survives() {
    let mut h = Harness::open().await;
    let mut unauth = h.raw_socket(false).await.unwrap();
    let observation = timeout(Duration::from_secs(5), h.observations.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(h.service.state.connections.snapshot_for_test().total, 2);
    drop(observation); // Not even one poll: IO was already adopted by the owner.
    assert_eq!(
        close_code(&mut unauth).await,
        (1001, "node stopping".into())
    );
    drop(unauth);
    wait_for(|| h.service.state.connections.snapshot_for_test().total == 1).await;
    h.peer.send(PeerMessage::Text(serde_json::json!({"v":1,"type":"command","requestId":Id128::new([114;16]),"action":"node.info","payload":{}}).to_string().into())).await.unwrap();
    assert_eq!(next_json(&mut h.peer).await["ok"], true);
    h.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connection_owner_development_authenticated_drop_keeps_second_socket() {
    let mut h = Harness::open().await;
    let (mut other, _attachment, observation) = h.same_actor_second_socket().await;
    drop(observation);
    assert_eq!(close_code(&mut other).await.0, 1001);
    drop(other);
    wait_for(|| h.service.state.connections.snapshot_for_test().total == 1).await;
    h.peer.send(PeerMessage::Text(serde_json::json!({"v":1,"type":"command","requestId":Id128::new([115;16]),"action":"node.info","payload":{}}).to_string().into())).await.unwrap();
    assert_eq!(next_json(&mut h.peer).await["ok"], true);
    h.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connection_owner_development_closing_after_upgrade_ticket_owns_rejection() {
    let h = Harness::open().await;
    let registered = h
        .service
        .pause_next_boundary(ManagementBoundary::UpgradeRegistered);
    let rejected = h
        .service
        .pause_next_boundary(ManagementBoundary::RejectedClose);
    let mut peer = {
        let connecting = h.raw_socket(true);
        tokio::pin!(connecting);
        tokio::select! {
            () = registered.wait_until_entered() => (),
            result = &mut connecting => panic!("upgrade escaped original registration barrier: {}", result.is_ok()),
        }
        assert_eq!(
            h.service
                .state
                .connections
                .snapshot_for_test()
                .pending_upgrades,
            1
        );
        h.service.begin_shutdown();
        registered.release();
        (&mut connecting).await.unwrap()
    };
    timeout(Duration::from_secs(5), rejected.wait_until_entered())
        .await
        .unwrap();
    let premature = timeout(Duration::from_millis(50), h.service.wait_closed())
        .await
        .is_ok();
    let snapshot = h.service.state.connections.snapshot_for_test();
    rejected.release();
    assert_eq!(close_code(&mut peer).await, (1001, "node stopping".into()));
    drop(peer);
    timeout(Duration::from_secs(5), h.service.wait_closed())
        .await
        .unwrap();
    let refused = h.raw_socket(true).await.unwrap_err();
    assert!(
        matches!(refused, tokio_tungstenite::tungstenite::Error::Http(ref response) if response.status().as_u16() == 503)
    );
    h.finish().await;
    assert!(!premature);
    assert!(snapshot.slots >= 1 && snapshot.protocol_handles >= 1);
    assert_eq!(snapshot.pending_upgrades, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connection_owner_development_limit_rejected_close_is_owned_without_lease() {
    let h = Harness::open().await;
    let leases: Vec<_> = (0..MAX_UNAUTHENTICATED_CONNECTIONS)
        .map(|_| h.service.state.connections.acquire().unwrap())
        .collect();
    let count = h.service.state.connections.snapshot_for_test().total;
    let rejected = h
        .service
        .pause_next_boundary(ManagementBoundary::RejectedClose);
    let mut peer = h.raw_socket(true).await.unwrap();
    timeout(Duration::from_secs(5), rejected.wait_until_entered())
        .await
        .unwrap();
    let snapshot = h.service.state.connections.snapshot_for_test();
    rejected.release();
    assert_eq!(close_code(&mut peer).await, (4429, "limit exceeded".into()));
    drop(peer);
    wait_for(|| h.service.state.connections.snapshot_for_test().slots == 1).await;
    drop(leases);
    h.finish().await;
    assert_eq!(
        snapshot.total, count,
        "rejected close did not acquire a success lease"
    );
    assert_eq!(
        snapshot.slots, 2,
        "rejection is nevertheless a real network work slot"
    );
    assert_eq!(snapshot.pending_upgrades, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connection_owner_development_busy_revoke_survives_observer_stop() {
    use sqlx::Connection;
    let mut h = Harness::open().await;
    let target = Id128::new([116; 16]).to_base64url();
    let key = SigningKey::from_slice(&[117; 32]).unwrap();
    h.service
        .state
        .trust
        .register_client(
            &target,
            key.verifying_key().to_sec1_point(false).as_bytes(),
            "revoke target",
        )
        .await
        .unwrap();
    let mut blocker = sqlx::SqliteConnection::connect_with(
        &sqlx::sqlite::SqliteConnectOptions::new().filename(&h.database),
    )
    .await
    .unwrap();
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut blocker)
        .await
        .unwrap();
    let target_gate = h.service.state.trust.client_gate(&target);
    h.peer.send(PeerMessage::Text(serde_json::json!({"v":1,"type":"command","requestId":Id128::new([118;16]),"action":"device.revoke","payload":{"clientId":target}}).to_string().into())).await.unwrap();
    // The original revoke owns its target gate while blocked in real BEGIN IMMEDIATE.
    wait_for(|| target_gate.try_lock().is_err()).await;
    let observer = h.observer.take().unwrap();
    observer.abort();
    assert!(observer.await.unwrap_err().is_cancelled());
    let premature = timeout(Duration::from_millis(100), h.service.wait_closed())
        .await
        .is_ok();
    let snapshot = h.service.state.connections.snapshot_for_test();
    sqlx::query("ROLLBACK").execute(&mut blocker).await.unwrap();
    blocker.close().await.unwrap();
    timeout(Duration::from_secs(5), h.service.wait_closed())
        .await
        .unwrap();
    let record = h
        .service
        .state
        .trust
        .client_by_id(&target)
        .await
        .unwrap()
        .unwrap();
    h.finish().await;
    assert!(!premature);
    assert_eq!(snapshot.total, 1);
    assert_eq!(snapshot.protocol_handles, 1);
    assert_eq!(
        record.status,
        crate::trust::ClientStatus::Revoked,
        "the original admitted command actually committed after stop"
    );
}

#[tokio::test]
async fn connection_owner_development_unused_upgrade_ticket_releases_only_pending_work() {
    let registry = ConnectionRegistry::new_shared();
    let ticket = registry.prepare_upgrade().unwrap();
    let snapshot = registry.snapshot_for_test();
    assert_eq!(snapshot.total, 0);
    assert_eq!(snapshot.pending_upgrades, 1);
    assert!(
        timeout(Duration::from_millis(25), registry.wait_empty())
            .await
            .is_err()
    );
    drop(ticket);
    timeout(Duration::from_secs(5), registry.wait_empty())
        .await
        .unwrap();
    registry.begin_shutdown();
    timeout(Duration::from_secs(5), registry.wait_empty())
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connection_owner_development_protocol_panic_preserves_typed_receipt_and_writer_join() {
    let mut h = Harness::open().await;
    h.service
        .state
        .test_hooks
        .protocol_failure
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut peer = h.raw_socket(false).await.unwrap();
    let mut observation = timeout(Duration::from_secs(5), h.observations.recv())
        .await
        .unwrap()
        .unwrap();
    timeout(Duration::from_secs(5), &mut observation)
        .await
        .unwrap();
    let result = observation.settlement().unwrap();
    assert_eq!(close_code(&mut peer).await, (1001, "node stopping".into()));
    drop(peer);
    drop(observation);
    wait_for(|| h.service.state.connections.snapshot_for_test().slots == 1).await;
    h.finish().await;
    assert_eq!(result.kind, connection_owner::ConnectionKind::Protocol);
    assert_eq!(
        result.protocol,
        connection_owner::ProtocolSettlement::TaskFailed
    );
    assert_eq!(result.writer, Some(WriterSettlement::Joined(Ok(()))));
}
