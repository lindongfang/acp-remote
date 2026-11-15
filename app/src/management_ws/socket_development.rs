#![allow(
    clippy::too_many_lines,
    clippy::significant_drop_tightening,
    reason = "Development scenarios keep the real TLS/authentication/source/read and original cleanup handles visible in causal order"
)]
//! Author development checks: real TLS, management proof and original socket writer.
//! The cfg(test) attachment uses a real module source/read; it is not a production `ViewPort`.
use super::*;
use crate::{
    application::shared_views::{
        self as views, RetainedRaw, SharedViewOwner,
        controller::{HookSelector, ViewHookPoint},
        indexed_journal::{ProjectionEnvelope, ViewProjector},
        ingress::source_ingress,
        read_observation::ReadObservation,
        reader::{PrepareNext, ViewReadService},
        wire::{RecordKind, TransferHeader},
    },
    claims::verify_auth_host,
    domain::{
        EntityId,
        native::{NativeSessionKey, NativeSourceId},
    },
    jws::sign,
    storage::Storage,
};
use futures_util::FutureExt;
use p256::ecdsa::SigningKey;
use rustls::pki_types::{PrivatePkcs8KeyDer, ServerName};
use std::panic::AssertUnwindSafe;
use std::{num::NonZeroUsize, path::PathBuf};
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
    storage: Storage,
    service: ManagementService,
    peer: Peer,
    attachment: Arc<SocketAttachment>,
    outbound: Outbound,
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
        let router = service.router().layer(Extension(PeerAddress(addr)));
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
        let (attachment, outbound) = timeout(Duration::from_secs(5), accept_rx)
            .await
            .unwrap()
            .unwrap();
        Self {
            storage,
            service,
            peer,
            attachment,
            outbound,
            stop,
            server,
            client_id,
            address: addr,
            tls_config,
        }
    }
    async fn same_actor_second_socket(&self) -> (Peer, Arc<SocketAttachment>) {
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
        (peer, attachment)
    }
    async fn credit(&mut self, id: Id128, packet: u64) {
        self.peer
            .send(PeerMessage::Binary(
                TransferHeader::credit(id, packet)
                    .encode(&[])
                    .unwrap()
                    .into(),
            ))
            .await
            .unwrap();
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
async fn binary(peer: &mut Peer) -> Vec<u8> {
    loop {
        match timeout(Duration::from_secs(5), peer.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
        {
            PeerMessage::Binary(b) => return b.to_vec(),
            PeerMessage::Ping(p) => peer.send(PeerMessage::Pong(p)).await.unwrap(),
            other => panic!("unexpected data message {other:?}"),
        }
    }
}
#[derive(Clone)]
struct Raw(Arc<[u8]>);
impl RetainedRaw for Raw {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn bytes(&self) -> &[u8] {
        &self.0
    }
}
struct Projector;
impl ViewProjector<Raw> for Projector {
    fn inspect(&self, _: &Raw) -> ProjectionEnvelope {
        ProjectionEnvelope::unknown(Some(Arc::from("socket-session")))
    }
}
type Reads = ViewReadService<u64, Raw, SocketViewAuthority, SocketViewClient>;
type SourceFixture = (
    SharedViewOwner<u64, Raw>,
    Reads,
    views::reader::OpenedView,
    views::ingress::SourceProducer<u64, Raw>,
);
async fn source(h: &Harness) -> SourceFixture {
    source_raw(
        h,
        Arc::from(&b"{\"unknown\":1,\"unknown\":2,\"big\":900719925474099312345}"[..]),
    )
    .await
}
async fn source_raw(h: &Harness, raw: Arc<[u8]>) -> SourceFixture {
    let scope = 1u64;
    let source = NativeSourceId(EntityId::from_bytes([98; 16]));
    let target = NativeSessionKey::from_config(Some(source.0), "socket-session").unwrap();
    let (mut producer, consumer, probe) = source_ingress(scope);
    let owner = SharedViewOwner::start_source(
        scope,
        source,
        consumer,
        probe,
        Arc::new(Projector),
        NonZeroUsize::new(8).unwrap(),
        tokio::runtime::Handle::current(),
    )
    .unwrap();
    producer.bind_route(target.clone(), None).unwrap();
    producer
        .retain(&scope, 1, 1, Raw(raw), ReadObservation::unavailable())
        .unwrap();
    owner.handle().core.probe.wait_processed(1).await.unwrap();
    let authority = h.service.state.socket_authority.clone();
    authority.register_module_source(&owner.handle()).unwrap();
    let reads = ViewReadService::new(owner.handle(), authority, owner.supervisor.clone());
    let read = timeout(
        Duration::from_secs(5),
        reads.open_current(&h.attachment.client, &target),
    )
    .await
    .expect("original read deadline")
    .expect("original read qualification");
    h.attachment.attach_read(&read).unwrap();
    // Return the original producer: dropping it seals the capture ingress.
    (owner, reads, read, producer)
}
async fn wait_until(mut condition: impl FnMut() -> bool) {
    timeout(Duration::from_secs(5), async {
        while !condition() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_socket_actual_flush_credit_and_snapshot_end() {
    let mut h = Harness::open().await;
    let (owner, reads, read, _producer) = source(&h).await;
    let outcome = AssertUnwindSafe(async {
        let stream = read.socket_stream().unwrap();
        h.credit(stream.id(), 0).await;
        wait_until(|| stream.ready().unwrap_or(false)).await;
        let mut gate = owner
            .test_controller()
            .arm_pause(
                ViewHookPoint::AfterFlushFactBeforePublish,
                HookSelector::Any,
            )
            .unwrap();
        let PrepareNext::Chunk(chunk) = reads
            .next_snapshot(&h.attachment.client, &read)
            .await
            .unwrap()
        else {
            panic!("expected first chunk")
        };
        assert!(
            h.outbound
                .send_shared(chunk.into_socket().unwrap(), &h.attachment.client)
                .is_ok()
        );
        timeout(Duration::from_secs(5), gate.arrived())
            .await
            .unwrap()
            .unwrap();
        let bytes = binary(&mut h.peer).await;
        let (header, payload) = TransferHeader::decode(&bytes).unwrap();
        assert_eq!(header.packet(), 1);
        assert_eq!(header.record_kind(), Some(RecordKind::Status));
        assert!(!payload.is_empty());
        h.credit(stream.id(), 1).await;
        wait_until(|| stream.observation().unwrap().pending_early_credit).await;
        assert_eq!(stream.observation().unwrap().highest_committed, 0);
        assert_eq!(stream.observation().unwrap().highest_flushed, 1);
        gate.release();
        wait_until(|| stream.observation().unwrap().highest_committed == 1).await;
        let expected = [
            RecordKind::OriginalRaw,
            RecordKind::ItemProjection,
            RecordKind::Status,
            RecordKind::SnapshotEnd,
        ];
        for (index, kind) in expected.into_iter().enumerate() {
            match reads
                .next_snapshot(&h.attachment.client, &read)
                .await
                .unwrap()
            {
                PrepareNext::Chunk(c) => {
                    assert_eq!(c.address().kind(), kind);
                    assert_eq!(c.packet_sequence(), index as u64 + 2);
                    let packet = c.packet_sequence();
                    assert!(
                        h.outbound
                            .send_shared(c.into_socket().unwrap(), &h.attachment.client)
                            .is_ok()
                    );
                    let b = binary(&mut h.peer).await;
                    let (received, payload) = TransferHeader::decode(&b).unwrap();
                    assert_eq!(received.packet(), packet);
                    if received.record_kind() == Some(RecordKind::OriginalRaw) {
                        assert!(
                            payload
                                == b"{\"unknown\":1,\"unknown\":2,\"big\":900719925474099312345}",
                            "original raw bytes differ"
                        );
                    }
                    h.credit(stream.id(), packet).await;
                    wait_until(|| stream.observation().unwrap().highest_committed == packet).await;
                }

                _ => panic!("unexpected snapshot state"),
            }
        }
        assert!(matches!(
            reads
                .next_snapshot(&h.attachment.client, &read)
                .await
                .unwrap(),
            PrepareNext::SnapshotFinished
        ));
    })
    .catch_unwind()
    .await;
    finish_case(h, owner, read, outcome).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_socket_seal_before_start_has_no_send_or_cursor() {
    let mut h = Harness::open().await;
    let (owner, reads, read, _producer) = source(&h).await;
    let outcome = AssertUnwindSafe(async {
        let stream = read.socket_stream().unwrap();
        h.credit(stream.id(), 0).await;
        wait_until(|| stream.ready().unwrap_or(false)).await;
        let mut gate = owner
            .test_controller()
            .arm_pause(ViewHookPoint::BeforeSinkReady, HookSelector::Any)
            .unwrap();
        let PrepareNext::Chunk(c) = reads
            .next_snapshot(&h.attachment.client, &read)
            .await
            .unwrap()
        else {
            panic!("chunk")
        };
        assert!(
            h.outbound
                .send_shared(c.into_socket().unwrap(), &h.attachment.client)
                .is_ok()
        );
        timeout(Duration::from_secs(5), gate.arrived())
            .await
            .unwrap()
            .unwrap();
        stream.seal().unwrap();
        gate.release();
        wait_until(|| stream.explicitly_sealed().unwrap()).await;
        assert_eq!(stream.observation().unwrap().highest_started, 0);
        assert_eq!(stream.observation().unwrap().highest_committed, 0);
        wait_until(|| {
            owner
                .supervisor
                .task_state(views::tasks::ViewTaskKind::Reader)
                == views::probe::ViewTaskState::Sealing
        })
        .await;
    })
    .catch_unwind()
    .await;
    finish_case(h, owner, read, outcome).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_socket_real_actor_revoke_after_flush_preserves_fact_without_cursor() {
    let mut h = Harness::open().await;
    let (owner, reads, read, _producer) = source(&h).await;
    let outcome = AssertUnwindSafe(async {
        let stream = read.socket_stream().unwrap();
        h.credit(stream.id(), 0).await;
        wait_until(|| stream.ready().unwrap_or(false)).await;
        let mut gate = owner
            .test_controller()
            .arm_pause(
                ViewHookPoint::AfterFlushFactBeforePublish,
                HookSelector::Any,
            )
            .unwrap();
        let PrepareNext::Chunk(c) = reads
            .next_snapshot(&h.attachment.client, &read)
            .await
            .unwrap()
        else {
            panic!("chunk")
        };
        assert!(
            h.outbound
                .send_shared(c.into_socket().unwrap(), &h.attachment.client)
                .is_ok()
        );
        timeout(Duration::from_secs(5), gate.arrived())
            .await
            .unwrap()
            .unwrap();
        let _ = binary(&mut h.peer).await;
        h.service
            .state
            .trust
            .revoke(
                "local-owner",
                &Id128::new([99; 16]).to_base64url(),
                &h.client_id.to_base64url(),
            )
            .await
            .unwrap();
        gate.release();
        wait_until(|| stream.explicitly_sealed().unwrap()).await;
        let observed = stream.observation().unwrap();
        assert_eq!(observed.highest_flushed, 1);
        assert_eq!(observed.highest_started, 1);
        assert_eq!(observed.highest_committed, 0);
    })
    .catch_unwind()
    .await;
    finish_case(h, owner, read, outcome).await;
}

async fn finish_case(
    h: Harness,
    owner: SharedViewOwner<u64, Raw>,
    read: views::reader::OpenedView,
    outcome: Result<(), Box<dyn std::any::Any + Send>>,
) {
    drop(read);
    let report = owner
        .shutdown(tokio::time::Instant::now() + Duration::from_secs(5))
        .await;
    let closed = AssertUnwindSafe(h.finish()).catch_unwind().await;
    if let Err(first) = outcome {
        std::panic::resume_unwind(first)
    }
    if let Err(close) = closed {
        std::panic::resume_unwind(close)
    }
    assert!(report.worker_joined() && report.readers_joined() && report.cleanup_joined());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_socket_queue_rejection_returns_original_prepared() {
    let mut h = Harness::open().await;
    let (owner, reads, read, _producer) = source(&h).await;
    let outcome = AssertUnwindSafe(async {
        let stream = read.socket_stream().unwrap();
        h.credit(stream.id(), 0).await;
        wait_until(|| stream.ready().unwrap_or(false)).await;
        let PrepareNext::Chunk(c) = reads
            .next_snapshot(&h.attachment.client, &read)
            .await
            .unwrap()
        else {
            panic!("chunk")
        };
        let token = c.into_socket().unwrap();
        let header = token.header();
        let bytes = h
            .outbound
            .bytes
            .clone()
            .try_acquire_many_owned(u32::try_from(MAX_OUTBOUND_BYTES).unwrap())
            .unwrap();
        let Err(SocketEnqueueError::Rejected(returned)) =
            h.outbound.send_shared(token, &h.attachment.client)
        else {
            panic!("original token must return")
        };
        assert_eq!(returned.header(), header);
        assert_eq!(stream.observation().unwrap().highest_started, 0);
        drop(bytes);
        assert!(
            h.outbound
                .send_shared(returned, &h.attachment.client)
                .is_ok()
        );
        let actual = binary(&mut h.peer).await;
        assert_eq!(TransferHeader::decode(&actual).unwrap().0, header);
        h.credit(stream.id(), header.packet()).await;
        wait_until(|| stream.observation().unwrap().highest_committed == header.packet()).await;
    })
    .catch_unwind()
    .await;
    finish_case(h, owner, read, outcome).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_socket_authenticated_detach_seals_original_read() {
    let mut h = Harness::open().await;
    let (owner, _reads, read, _producer) = source(&h).await;
    let outcome = AssertUnwindSafe(async {
        let stream = read.socket_stream().unwrap();
        h.credit(stream.id(), 0).await;
        wait_until(|| stream.ready().unwrap_or(false)).await;
        h.peer
            .send(PeerMessage::Binary(
                TransferHeader::detach(stream.id())
                    .encode(&[])
                    .unwrap()
                    .into(),
            ))
            .await
            .unwrap();
        wait_until(|| stream.explicitly_sealed().unwrap()).await;
        assert_eq!(stream.observation().unwrap().highest_started, 0);
        assert_eq!(stream.observation().unwrap().highest_committed, 0);
    })
    .catch_unwind()
    .await;
    finish_case(h, owner, read, outcome).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_socket_same_actor_other_connection_cannot_attach_or_credit_original_read() {
    let mut h = Harness::open().await;
    let (owner, _reads, read, _producer) = source(&h).await;
    let outcome=AssertUnwindSafe(async{
 let stream=read.socket_stream().unwrap();let(mut other,attachment)=h.same_actor_second_socket().await;
 assert!(Arc::ptr_eq(attachment.client.context.authority(),h.attachment.client.context.authority()));
 assert!(matches!(attachment.attach_read(&read),Err(crate::domain::shared_view::ViewError::InvalidClient)));
 other.send(PeerMessage::Binary(TransferHeader::credit(stream.id(),0).encode(&[]).unwrap().into())).await.unwrap();
 let closed=timeout(Duration::from_secs(5),other.next()).await.unwrap().unwrap().unwrap();assert!(matches!(closed,PeerMessage::Close(Some(frame)) if frame.code==tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Library(CLOSE_FORMAT)));
 assert!(!stream.ready().unwrap());assert_eq!(stream.observation().unwrap().highest_committed,0);h.credit(stream.id(),0).await;wait_until(||stream.ready().unwrap_or(false)).await;let _=other.close(None).await;
 }).catch_unwind().await;
    finish_case(h, owner, read, outcome).await;
}

// Prefill uses original source/read packets and real peer Credit only after
// each actual flush. The peer deliberately does not read those raw packets.
// No fake sink, flush setter, extra writer or control-JSON body is involved.
async fn start_slow_raw(h: &mut Harness, reads: &Reads, read: &views::reader::OpenedView) -> u64 {
    let stream = read.socket_stream().unwrap();
    h.credit(stream.id(), 0).await;
    wait_until(|| stream.ready().unwrap_or(false)).await;
    let PrepareNext::Chunk(first) = reads
        .next_snapshot(&h.attachment.client, read)
        .await
        .unwrap()
    else {
        panic!("status chunk")
    };
    assert_eq!(first.address().kind(), RecordKind::Status);
    assert!(
        h.outbound
            .send_shared(first.into_socket().unwrap(), &h.attachment.client)
            .is_ok()
    );
    let bytes = binary(&mut h.peer).await;
    assert_eq!(TransferHeader::decode(&bytes).unwrap().0.packet(), 1);
    h.credit(stream.id(), 1).await;
    wait_until(|| stream.observation().unwrap().highest_committed == 1).await;
    let pending = timeout(Duration::from_secs(5), async {
        loop {
            let PrepareNext::Chunk(raw) = reads
                .next_snapshot(&h.attachment.client, read)
                .await
                .unwrap()
            else {
                panic!("raw chunk")
            };
            assert_eq!(raw.address().kind(), RecordKind::OriginalRaw);
            let packet = raw.packet_sequence();
            let before = stream.flush_poll_observation().pending;
            assert!(
                h.outbound
                    .send_shared(raw.into_socket().unwrap(), &h.attachment.client)
                    .is_ok()
            );
            loop {
                let polls = stream.flush_poll_observation();
                if polls.success == packet {
                    // Valid original wire control; no Data bytes are consumed.
                    h.credit(stream.id(), packet).await;
                    wait_until(|| stream.observation().unwrap().highest_committed == packet).await;
                    break;
                }
                if polls.pending > before {
                    assert_eq!(polls.success, packet - 1);
                    return packet;
                }
                tokio::task::yield_now().await;
            }
        }
    })
    .await;
    assert!(
        pending.is_ok(),
        "no actual Pending within original 5s fixture budget: polls={:?} transfer={:?}",
        stream.flush_poll_observation(),
        stream.observation().unwrap()
    );
    let packet = pending.unwrap();
    let polls = stream.flush_poll_observation();
    let transfer = stream.observation().unwrap();
    assert_eq!(polls.success, packet - 1, "target still genuinely Pending");
    assert!(polls.entered >= polls.pending + polls.success + polls.error);
    assert_eq!(polls.error, 0);
    assert_eq!(transfer.highest_started, packet);
    assert_eq!(transfer.highest_flushed, packet - 1);
    assert_eq!(transfer.highest_committed, packet - 1);
    assert!(
        h.service
            .state
            .trust
            .client_gate(&h.client_id.to_base64url())
            .try_lock_owned()
            .is_err(),
        "same original gate covers Pending"
    );
    eprintln!(
        "slow-poll evidence: actual pending={} success={} error={} started={} flushed={} committed={}",
        polls.pending,
        polls.success,
        polls.error,
        transfer.highest_started,
        transfer.highest_flushed,
        transfer.highest_committed
    );
    packet
}
async fn read_slow_raw(h: &mut Harness, packet: u64) {
    for expected in 2..=packet {
        let bytes = binary(&mut h.peer).await;
        let (header, payload) = TransferHeader::decode(&bytes).unwrap();
        assert_eq!(header.packet(), expected, "no skipped or reordered frame");
        assert_eq!(header.record_kind(), Some(RecordKind::OriginalRaw));
        assert!(
            !payload.is_empty() && payload.iter().all(|b| *b == b'x'),
            "raw synthetic chunk differs"
        );
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_socket_real_pending_accepts_early_credit_before_actual_flush() {
    let mut h = Harness::open_with_small_buffers(true).await;
    let (owner, reads, read, _producer) =
        source_raw(&h, Arc::from(vec![b'x'; 8 * 1024 * 1024])).await;
    let outcome = AssertUnwindSafe(async {
        let stream = read.socket_stream().unwrap();
        let packet = start_slow_raw(&mut h, &reads, &read).await;
        h.credit(stream.id(), packet).await;
        wait_until(|| stream.observation().unwrap().pending_early_credit).await;
        assert_eq!(stream.flush_poll_observation().success, packet - 1);
        assert_eq!(stream.observation().unwrap().highest_committed, packet - 1);
        read_slow_raw(&mut h, packet).await;
        wait_until(|| stream.observation().unwrap().highest_committed == packet).await;
        assert_eq!(stream.flush_poll_observation().success, packet);
        assert_eq!(stream.flush_poll_observation().error, 0);
    })
    .catch_unwind()
    .await;
    finish_case(h, owner, read, outcome).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_socket_real_pending_revoke_waits_for_original_gate_until_flush() {
    use std::{future::Future, task::Poll};
    let mut h = Harness::open_with_small_buffers(true).await;
    let (owner, reads, read, _producer) =
        source_raw(&h, Arc::from(vec![b'x'; 8 * 1024 * 1024])).await;
    let outcome=AssertUnwindSafe(async {
        let stream=read.socket_stream().unwrap();
        let packet=start_slow_raw(&mut h,&reads,&read).await;
        h.credit(stream.id(),packet).await;
        wait_until(||stream.observation().unwrap().pending_early_credit).await;
        let revision=h.service.state.trust.revision_receiver();
        let before=*revision.borrow();
        let trust=h.service.state.trust.clone();
        let client=h.client_id.to_base64url();
        let request=Id128::new([112;16]).to_base64url();
        let mut revoke=Box::pin(trust.revoke("local-owner",&request,&client));
        let pending=std::future::poll_fn(|cx|Poll::Ready(revoke.as_mut().poll(cx).is_pending())).await;
        assert!(pending,"real revoke must await original writer gate");
        assert_eq!(*revision.borrow(),before);
        assert_eq!(stream.flush_poll_observation().success,packet-1);
        let mut publication=owner.test_controller().arm_pause(ViewHookPoint::AfterFlushFactBeforePublish,HookSelector::Any).unwrap();
        read_slow_raw(&mut h,packet).await;
        timeout(Duration::from_secs(5),publication.arrived()).await.unwrap().unwrap();
        assert_eq!(stream.flush_poll_observation().success,packet);
        timeout(Duration::from_secs(5),revoke).await.unwrap().unwrap();
        assert!(*revision.borrow()>before);
        publication.release();
        wait_until(||stream.explicitly_sealed().unwrap()).await;
        assert_eq!(stream.observation().unwrap().highest_flushed,packet);
        assert_eq!(stream.observation().unwrap().highest_committed,packet-1);
        eprintln!("revoke-order evidence: Pending gate blocks revoke; actual flush Ready precedes revoke revision; old cursor remains previous committed packet");
    }).catch_unwind().await;
    finish_case(h, owner, read, outcome).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_socket_real_pending_detach_seals_without_waiting_for_actor_gate() {
    let mut h = Harness::open_with_small_buffers(true).await;
    let (owner, reads, read, _producer) =
        source_raw(&h, Arc::from(vec![b'x'; 8 * 1024 * 1024])).await;
    let outcome=AssertUnwindSafe(async {
        let stream=read.socket_stream().unwrap();
        let packet=start_slow_raw(&mut h,&reads,&read).await;
        h.peer.send(PeerMessage::Binary(TransferHeader::detach(stream.id()).encode(&[]).unwrap().into())).await.unwrap();
        wait_until(||stream.explicitly_sealed().unwrap()).await;
        assert_eq!(stream.flush_poll_observation().success,packet-1);
        assert!(h.service.state.trust.client_gate(&h.client_id.to_base64url()).try_lock_owned().is_err());
        read_slow_raw(&mut h,packet).await;
        wait_until(||stream.flush_poll_observation().success==packet).await;
        assert_eq!(stream.observation().unwrap().highest_committed,packet-1);
        eprintln!("detach-order evidence: original stream seal completed during real Pending while actor gate remained held");
    }).catch_unwind().await;
    finish_case(h, owner, read, outcome).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_socket_actor_revoke_before_writer_gate_has_no_start_or_flush() {
    let mut h = Harness::open().await;
    let (owner, reads, read, _producer) = source(&h).await;
    let outcome = AssertUnwindSafe(async {
        let stream = read.socket_stream().unwrap();
        h.credit(stream.id(), 0).await;
        wait_until(|| stream.ready().unwrap_or(false)).await;
        let mut before = owner
            .test_controller()
            .arm_pause(
                ViewHookPoint::AfterChunkQueuedBeforeWriterGate,
                HookSelector::Any,
            )
            .unwrap();
        let PrepareNext::Chunk(chunk) = reads
            .next_snapshot(&h.attachment.client, &read)
            .await
            .unwrap()
        else {
            panic!("status")
        };
        assert!(
            h.outbound
                .send_shared(chunk.into_socket().unwrap(), &h.attachment.client)
                .is_ok()
        );
        timeout(Duration::from_secs(5), before.arrived())
            .await
            .unwrap()
            .unwrap();
        h.service
            .state
            .trust
            .revoke(
                "local-owner",
                &Id128::new([113; 16]).to_base64url(),
                &h.client_id.to_base64url(),
            )
            .await
            .unwrap();
        before.release();
        wait_until(|| stream.explicitly_sealed().unwrap()).await;
        let transfer = stream.observation().unwrap();
        assert_eq!(transfer.highest_started, 0);
        assert_eq!(transfer.highest_flushed, 0);
        assert_eq!(transfer.highest_committed, 0);
        let polls = stream.flush_poll_observation();
        assert_eq!((polls.pending, polls.success, polls.error), (0, 0, 0));
    })
    .catch_unwind()
    .await;
    finish_case(h, owner, read, outcome).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn peer_close_flushes_real_reply_before_original_writer_joins() {
    let mut h = Harness::open().await;
    let outcome = AssertUnwindSafe(async {
        h.peer.close(Some(tokio_tungstenite::tungstenite::protocol::CloseFrame {
            code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Normal,
            reason: "".into(),
        })).await.expect("send explicit peer Close1000");
        let reply = timeout(Duration::from_secs(5), h.peer.next()).await
            .expect("close reply deadline").expect("actual reply frame")
            .expect("real TLS close reply, never EOF");
        assert!(matches!(reply, PeerMessage::Close(Some(frame)) if frame.code == tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Normal), "actual peer Close1000 reply");
        timeout(Duration::from_secs(5), h.service.wait_closed()).await
            .expect("original writer and connection join");
        assert_eq!(*h.service.state.test_hooks.writer_settlement.lock().unwrap(),
            Some(WriterSettlement::Joined(Ok(()))), "actual original writer close result and join");
    }).catch_unwind().await;
    let cleanup = AssertUnwindSafe(h.finish()).catch_unwind().await;
    if let Err(first) = outcome {
        std::panic::resume_unwind(first);
    }
    if let Err(cleanup) = cleanup {
        std::panic::resume_unwind(cleanup);
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_socket_closed_stream_accepts_trailing_detach_and_credit_while_rejecting_foreign() {
    let mut h = Harness::open().await;
    let (owner, _reads, read, _producer) = source(&h).await;
    let outcome = AssertUnwindSafe(async {
        let stream = read.socket_stream().unwrap();
        let closed_id = Id128::new(*stream.id().as_bytes());
        assert!(
            h.attachment
                .streams
                .lock()
                .unwrap()
                .remove(&closed_id)
                .is_some(),
            "attached stream must be present before close/rebind"
        );
        h.attachment.view_close_receipts.lock().unwrap().insert(
            closed_id,
            Err(crate::domain::shared_view::ViewError::StreamSealed),
        );

        let detach_bytes = TransferHeader::detach(closed_id).encode(&[]).unwrap();
        assert!(
            h.attachment
                .control(&detach_bytes, &h.outbound)
                .await
                .is_ok(),
            "trailing empty Detach for own closed/rebound stream must be ignored"
        );
        let credit_bytes = TransferHeader::credit(closed_id, 1).encode(&[]).unwrap();
        assert!(
            h.attachment
                .control(&credit_bytes, &h.outbound)
                .await
                .is_ok(),
            "trailing empty Credit for own closed/rebound stream must be ignored"
        );
        assert!(
            TransferHeader::detach(closed_id).encode(b"bad").is_err(),
            "Detach header encoding must reject non-empty payload"
        );
        let mut non_empty_detach = detach_bytes.clone();
        non_empty_detach.extend_from_slice(b"bad");
        assert!(
            h.attachment
                .control(&non_empty_detach, &h.outbound)
                .await
                .is_err(),
            "trailing non-empty Detach bytes must not be ignored"
        );

        let foreign_id = Id128::new([123; 16]);
        let foreign_detach = TransferHeader::detach(foreign_id).encode(&[]).unwrap();
        assert_eq!(
            h.attachment.control(&foreign_detach, &h.outbound).await,
            Err(crate::domain::shared_view::ViewError::InvalidClient)
        );

        // Sending trailing Detach over the real TLS/WebSocket must keep the socket open
        h.peer
            .send(PeerMessage::Binary(detach_bytes.into()))
            .await
            .unwrap();
        let (mut second_peer, _second_attachment) = h.same_actor_second_socket().await;
        second_peer
            .send(PeerMessage::Binary(foreign_detach.into()))
            .await
            .unwrap();
        let close_msg = timeout(Duration::from_secs(5), async {
            loop {
                match second_peer.next().await.unwrap().unwrap() {
                    PeerMessage::Close(frame) => break frame,
                    PeerMessage::Ping(p) => second_peer.send(PeerMessage::Pong(p)).await.unwrap(),
                    other => panic!("expected 4400 close frame on foreign detach, got {other:?}"),
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(
            close_msg.map(|f| u16::from(f.code)),
            Some(4400),
            "foreign stream detach must still close WebSocket with 4400"
        );
        drop(second_peer);
    })
    .catch_unwind()
    .await;
    finish_case(h, owner, read, outcome).await;
}
