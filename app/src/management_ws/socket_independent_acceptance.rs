//! Independent P1 component acceptance. Synthetic source bytes are not ACP Received evidence.
//! Every noninitial wire Credit follows a Data frame decoded by this real TLS peer.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::manual_let_else,
    clippy::many_single_char_names,
    clippy::significant_drop_tightening,
    clippy::too_many_lines,
    clippy::unreadable_literal,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::*;
use crate::{
    application::shared_views::{
        RetainedRaw, SharedViewOwner,
        controller::{HookSelector, ViewHookPoint},
        indexed_journal::{ProjectionEnvelope, ViewProjector},
        ingress::{SourceProducer, source_ingress},
        read_observation::ReadObservation,
        reader::{OpenedView, PrepareNext, ViewReadService},
        wire::{RecordKind, TransferHeader, WireError},
    },
    claims::verify_auth_host,
    domain::{
        EntityId,
        native::{NativeSessionKey, NativeSourceId},
        shared_view::ViewError,
    },
    jws::sign,
    storage::Storage,
};
use futures_util::FutureExt;
use p256::ecdsa::SigningKey;
use rustls::pki_types::{PrivatePkcs8KeyDer, ServerName};
use std::{
    future::Future,
    num::NonZeroUsize,
    panic::AssertUnwindSafe,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
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
type Reads = ViewReadService<u64, Raw, SocketViewAuthority, SocketViewClient>;
#[path = "socket_independent_acceptance/capacity_restore_development.rs"]
mod capacity_restore_development;
const OBSERVE: Duration = Duration::from_secs(5);
static AREA: AtomicUsize = AtomicUsize::new(1);
fn pass(vector: &str) {
    println!("{{\"independentVector\":\"{vector}\",\"result\":\"pass\"}}");
}
async fn bound<F: Future>(f: F) -> F::Output {
    timeout(OBSERVE, f)
        .await
        .expect("one observation budget; no retry")
}
async fn until(mut f: impl FnMut() -> bool) {
    bound(async {
        while !f() {
            tokio::task::yield_now().await;
        }
    })
    .await;
}
fn socket_option(t: &TcpStream, name: i32, value: &[u8]) {
    use std::os::windows::io::AsRawSocket;
    #[link(name = "Ws2_32")]
    unsafe extern "system" {
        fn setsockopt(s: usize, l: i32, n: i32, v: *const u8, z: i32) -> i32;
    }
    // SAFETY: live owned TCP socket; immutable option bytes live through this call.
    let result = unsafe {
        setsockopt(
            t.as_raw_socket() as usize,
            0xffff,
            name,
            value.as_ptr(),
            value.len() as i32,
        )
    };
    assert_eq!(result, 0, "owned TCP option failed");
}
struct Listener {
    tcp: TcpListener,
    tls: TlsAcceptor,
    small: bool,
}
impl axum::serve::Listener for Listener {
    type Io = tokio_rustls::server::TlsStream<TcpStream>;
    type Addr = SocketAddr;
    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (tcp, address) = self.tcp.accept().await.expect("owned listener accept");
            if self.small {
                socket_option(&tcp, 0x1001, &1024i32.to_ne_bytes());
            }
            if let Ok(tls) = self.tls.accept(tcp).await {
                return (tls, address);
            }
        }
    }
    fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.tcp.local_addr()
    }
}
struct Session {
    peer: Option<Peer>,
    attachment: Arc<SocketAttachment>,
    outbound: Outbound,
    client_id: Id128,
}
impl Session {
    const fn peer(&mut self) -> &mut Peer {
        self.peer.as_mut().expect("original peer still owned")
    }
    async fn control(&mut self, bytes: Vec<u8>) {
        self.peer()
            .send(PeerMessage::Binary(bytes.into()))
            .await
            .expect("actual peer control write");
    }
    async fn initial(&mut self, stream: &SocketViewStream) {
        self.control(control_wire(2, stream.id(), 0)).await;
        until(|| stream.ready().unwrap_or(false)).await;
    }
    async fn received_credit(&mut self, expected: Id128, expected_packet: u64) -> Packet {
        let bytes = binary(self.peer()).await;
        let packet_received = packet(&bytes);
        assert!(
            packet_received.stream == expected && packet_received.sequence == expected_packet,
            "peer received wrong stream/packet"
        );
        self.control(control_wire(
            2,
            packet_received.stream,
            packet_received.sequence,
        ))
        .await;
        packet_received
    }
    fn abrupt(&mut self, reset: bool) {
        if let Some(peer) = self.peer.take() {
            if reset {
                socket_option(peer.get_ref().get_ref().0, 0x0080, &[1, 0, 0, 0]);
            }
            drop(peer);
        }
    }
}
struct Harness {
    storage: Storage,
    service: ManagementService,
    address: SocketAddr,
    tls: Arc<rustls::ClientConfig>,
    stop: Option<oneshot::Sender<()>>,
    server: JoinHandle<()>,
    sessions: Vec<Session>,
    root: PathBuf,
}
impl Harness {
    async fn new(small: bool) -> Self {
        let base = std::env::var_os("P1_DATA_ROOT")
            .filter(|v| !v.is_empty())
            .map_or_else(
                || {
                    let dir =
                        std::env::temp_dir().join(format!("acpr-p1-data-{}", std::process::id()));
                    std::fs::create_dir_all(&dir).expect("create p1 temp root");
                    dir
                },
                PathBuf::from,
            );
        assert!(base.is_absolute());
        let base = base.canonicalize().expect("precreated owned root");
        let root = base.join(format!(
            "{}-{}",
            std::process::id(),
            AREA.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).expect("fresh owned area");
        let storage = Storage::open(&root.join("socket.sqlite3"))
            .await
            .expect("original Storage open");
        let trust = storage.trust_store();
        let identity = Arc::new(NodeIdentity::for_test([41; 32]));
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
        let service =
            ManagementService::new(identity, origin, trust, pairing, control, policy).unwrap();
        assert!(
            !service
                .state
                .test_hooks
                .fresh_command_window
                .load(Ordering::SeqCst)
        );
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let der = cert.cert.der().clone();
        let key = PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der());
        let server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![der.clone()], key.into())
            .unwrap();
        let mut roots = rustls::RootCertStore::empty();
        roots.add(der).unwrap();
        let tls = Arc::new(
            rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        );
        let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = tcp.local_addr().unwrap();
        let listener = Listener {
            tcp,
            tls: TlsAcceptor::from(Arc::new(server_config)),
            small,
        };
        let router = service.router().layer(Extension(PeerAddress(address)));
        let (stop, stopped) = oneshot::channel();
        let server = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
                .expect("original server result");
        });
        let mut h = Self {
            storage,
            service,
            address,
            tls,
            stop: Some(stop),
            server,
            sessions: Vec::new(),
            root,
        };
        let first = h.authenticate(0, small).await;
        h.sessions.push(first);
        h
    }
    async fn raw_peer(&self, small: bool) -> Peer {
        let socket = TcpSocket::new_v4().unwrap();
        if small {
            socket.set_recv_buffer_size(4096).unwrap();
        }
        let tcp = socket.connect(self.address).await.unwrap();
        let tls = TlsConnector::from(self.tls.clone())
            .connect(ServerName::try_from("localhost").unwrap(), tcp)
            .await
            .unwrap();
        let request = ClientRequestBuilder::new(
            format!("wss://node.example.test{CONTROL_PATH}")
                .parse()
                .unwrap(),
        )
        .with_sub_protocol(CONTROL_PROTOCOL)
        .with_header("Origin", "https://node.example.test");
        tokio_tungstenite::client_async(request, tls)
            .await
            .unwrap()
            .0
    }
    async fn authenticate(&self, identity: u8, small: bool) -> Session {
        let key = SigningKey::from_slice(&[71 + identity; 32]).unwrap();
        let client_id = Id128::new([31 + identity; 16]);
        let public = PublicKeySec1::new(
            key.verifying_key()
                .to_sec1_point(false)
                .as_bytes()
                .try_into()
                .unwrap(),
        )
        .unwrap();
        if identity > 0 {
            self.service
                .state
                .trust
                .register_client(
                    &client_id.to_base64url(),
                    public.as_bytes(),
                    "independent peer",
                )
                .await
                .unwrap();
        } else if self.sessions.is_empty() {
            self.service
                .state
                .trust
                .register_client(
                    &client_id.to_base64url(),
                    public.as_bytes(),
                    "independent primary",
                )
                .await
                .unwrap();
        }
        let (tx, rx) = oneshot::channel();
        *self.service.state.socket_test_accept.lock().unwrap() = Some(tx);
        let mut peer = self.raw_peer(small).await;
        let mut nonce = [22 + identity; 32];
        nonce[30..].copy_from_slice(&(self.sessions.len() as u16).to_be_bytes());
        peer.send(PeerMessage::Text(serde_json::json!({"v":1,"type":"hello","clientId":client_id,"clientPublicKey":public,"clientNonce":Nonce256::new(nonce)}).to_string().into())).await.unwrap();
        let challenge = text(&mut peer).await;
        assert_eq!(challenge["type"], "challenge");
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
        assert_eq!(text(&mut peer).await["type"], "authenticated");
        let (attachment, outbound) = bound(rx).await.unwrap();
        Session {
            peer: Some(peer),
            attachment,
            outbound,
            client_id,
        }
    }
    async fn second(&mut self, actor: u8) -> usize {
        let session = self.authenticate(actor, false).await;
        self.sessions.push(session);
        self.sessions.len() - 1
    }
    async fn revoke(&self, index: usize) {
        self.service
            .state
            .trust
            .revoke(
                "local-owner",
                &Id128::new([62; 16]).to_base64url(),
                &self.sessions[index].client_id.to_base64url(),
            )
            .await
            .unwrap();
    }
    async fn finish(mut self) {
        for session in &mut self.sessions {
            session.abrupt(false);
        }
        self.service.begin_shutdown();
        let connections = timeout(Duration::from_secs(20), self.service.wait_closed()).await;
        let _ = self.stop.take().unwrap().send(());
        let server = timeout(Duration::from_secs(5), &mut self.server).await;
        self.storage.close().await;
        println!(
            "P1_RESOURCE retained_owned_directory={} sqlx_worker_join=not_observed",
            self.root.display()
        );
        assert!(
            connections.is_ok(),
            "original connection owners did not close"
        );
        assert!(
            matches!(server, Ok(Ok(()))),
            "original server JoinHandle not confirmed"
        );
    }
}
async fn frame(peer: &mut Peer) -> PeerMessage {
    loop {
        match bound(peer.next())
            .await
            .expect("peer frame")
            .expect("peer wire result")
        {
            PeerMessage::Ping(p) => peer.send(PeerMessage::Pong(p)).await.unwrap(),
            other => return other,
        }
    }
}
async fn text(peer: &mut Peer) -> Value {
    let PeerMessage::Text(t) = frame(peer).await else {
        panic!("expected control text")
    };
    serde_json::from_str(&t).unwrap()
}
async fn binary(peer: &mut Peer) -> Vec<u8> {
    let PeerMessage::Binary(b) = frame(peer).await else {
        panic!("expected actual Data frame")
    };
    b.to_vec()
}
async fn closed(peer: &mut Peer, expected: u16) {
    loop {
        match frame(peer).await {
            PeerMessage::Close(Some(c)) => {
                assert_eq!(u16::from(c.code), expected);
                return;
            }
            PeerMessage::Text(_) => {}
            _ => panic!("expected close code"),
        }
    }
}
fn control_wire(kind: u8, stream: Id128, ack: u64) -> Vec<u8> {
    let mut b = vec![0u8; 72];
    b[..4].copy_from_slice(b"ACPV");
    b[4] = 1;
    b[5] = kind;
    b[8..24].copy_from_slice(stream.as_bytes());
    b[24..32].copy_from_slice(&ack.to_be_bytes());
    b
}
struct Packet {
    stream: Id128,
    sequence: u64,
    kind: u8,
    offset: u64,
    total: u64,
    payload: Vec<u8>,
}
fn packet(bytes: &[u8]) -> Packet {
    assert!(
        bytes.len() >= 72 && &bytes[..4] == b"ACPV" && bytes[4] == 1 && bytes[5] == 1,
        "actual Data header"
    );
    assert!(
        bytes[6] == 0
            && bytes[7] <= 1
            && bytes[57..64].iter().chain(&bytes[68..72]).all(|b| *b == 0)
    );
    let number = |offset| u64::from_be_bytes(bytes[offset..offset + 8].try_into().unwrap());
    let n = u32::from_be_bytes(bytes[64..68].try_into().unwrap()) as usize;
    assert!(n <= 16384 && bytes.len() == 72 + n);
    let offset = number(40);
    let total = number(48);
    let end = offset.checked_add(n as u64).unwrap();
    assert!(end <= total && (bytes[7] == 1) == (end == total));
    assert!(number(24) > 0 && number(32) > 0 && (1..=4).contains(&bytes[56]));
    Packet {
        stream: Id128::new(bytes[8..24].try_into().unwrap()),
        sequence: number(24),
        kind: bytes[56],
        offset,
        total,
        payload: bytes[72..].to_vec(),
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
        ProjectionEnvelope::unknown(Some(Arc::from(
            "independent-native-canary-900719925474099312345",
        )))
    }
}
struct Source {
    fragmented_utf8: AtomicUsize,
    owner: SharedViewOwner<u64, Raw>,
    reads: Reads,
    opened: Vec<OpenedView>,
    producer: Option<SourceProducer<u64, Raw>>,
    target: NativeSessionKey,
    raw: Arc<[u8]>,
}
impl Source {
    async fn new(h: &Harness, raw: Arc<[u8]>) -> Self {
        let scope = 724u64;
        let source = NativeSourceId(EntityId::from_bytes([107; 16]));
        let target = NativeSessionKey::from_config(
            Some(source.0),
            "independent-native-canary-900719925474099312345",
        )
        .unwrap();
        let (mut producer, consumer, probe) = source_ingress(scope);
        let owner = SharedViewOwner::start_source(
            scope,
            source,
            consumer,
            probe,
            Arc::new(Projector),
            NonZeroUsize::new(16).unwrap(),
            tokio::runtime::Handle::current(),
        )
        .unwrap();
        producer.bind_route(target.clone(), None).unwrap();
        producer
            .retain(
                &scope,
                1,
                1,
                Raw(raw.clone()),
                ReadObservation::unavailable(),
            )
            .unwrap();
        owner.handle().core.probe.wait_processed(1).await.unwrap();
        let authority = h.service.state.socket_authority.clone();
        authority.register_module_source(&owner.handle()).unwrap();
        let reads = ViewReadService::new(owner.handle(), authority, owner.supervisor.clone());
        let mut s = Self {
            fragmented_utf8: AtomicUsize::new(0),
            owner,
            reads,
            opened: Vec::new(),
            producer: Some(producer),
            target,
            raw,
        };
        s.open(h, 0).await;
        s
    }
    async fn open(&mut self, h: &Harness, peer: usize) -> usize {
        let read = bound(
            self.reads
                .open_current(&h.sessions[peer].attachment.client, &self.target),
        )
        .await
        .unwrap();
        h.sessions[peer].attachment.attach_read(&read).unwrap();
        self.opened.push(read);
        self.opened.len() - 1
    }
    fn stream(&self, index: usize) -> SocketViewStream {
        self.opened[index].socket_stream().unwrap()
    }
    async fn prepare(&self, h: &Harness, read: usize, peer: usize) -> PreparedSocketChunk {
        let PrepareNext::Chunk(chunk) = bound(
            self.reads
                .next_snapshot(&h.sessions[peer].attachment.client, &self.opened[read]),
        )
        .await
        .unwrap() else {
            panic!("original read did not prepare chunk")
        };
        chunk.into_socket().unwrap()
    }
    async fn send(&self, h: &Harness, read: usize, peer: usize) {
        let c = self.prepare(h, read, peer).await;
        assert!(
            h.sessions[peer]
                .outbound
                .send_shared(c, &h.sessions[peer].attachment.client)
                .is_ok(),
            "original queue rejected"
        );
    }
    async fn finish(mut self) {
        for read in self.opened.drain(..) {
            let _ = self.reads.release(read);
        }
        drop(self.producer.take());
        let report = self
            .owner
            .shutdown(tokio::time::Instant::now() + OBSERVE)
            .await;
        assert!(
            report.worker_joined() && report.readers_joined() && report.cleanup_joined(),
            "original source/reader/cleanup JoinHandle receipts required"
        );
    }
}
fn raw_bytes(length: usize) -> Arc<[u8]> {
    let mut b = b"{\"unknown\":1,\"unknown\":2,\"n\":900719925474099312345,\"text\":\"".to_vec();
    while b.len() < length {
        b.extend_from_slice("x🌍\\u0041".as_bytes());
    }
    b.extend_from_slice(b"\"}");
    b.into()
}
async fn acknowledged(s: &Source, h: &mut Harness, read: usize, peer: usize) -> Packet {
    let stream = s.stream(read);
    let packet_expected = stream.observation().unwrap().highest_started + 1;
    s.send(h, read, peer).await;
    let p = h.sessions[peer]
        .received_credit(stream.id(), packet_expected)
        .await;
    until(|| stream.observation().unwrap().highest_committed == p.sequence).await;
    p
}
async fn drain_snapshot(s: &Source, h: &mut Harness, read: usize, peer: usize) -> (usize, Vec<u8>) {
    let stream = s.stream(read);
    h.sessions[peer].initial(&stream).await;
    let mut raw = Vec::new();
    let mut count = 0;
    loop {
        let next = bound(
            s.reads
                .next_snapshot(&h.sessions[peer].attachment.client, &s.opened[read]),
        )
        .await
        .unwrap();
        match next {
            PrepareNext::Chunk(c) => {
                let n = c.packet_sequence();
                assert!(
                    h.sessions[peer]
                        .outbound
                        .send_shared(
                            c.into_socket().unwrap(),
                            &h.sessions[peer].attachment.client
                        )
                        .is_ok()
                );
                let p = h.sessions[peer].received_credit(stream.id(), n).await;
                until(|| stream.observation().unwrap().highest_committed == n).await;
                if p.kind == 1 {
                    if std::str::from_utf8(&p.payload).is_err() {
                        s.fragmented_utf8.fetch_add(1, Ordering::Relaxed);
                    }
                    assert_eq!(p.offset, raw.len() as u64);
                    assert_eq!(p.total, s.raw.len() as u64);
                    raw.extend(p.payload);
                }
                count += 1;
            }
            PrepareNext::SnapshotFinished => break,
            _ => panic!("unexpected original snapshot progression"),
        }
    }
    (count, raw)
}
// Exactly 48 independently permitted streams fill the real original writer. No Data is credited from an observer.
async fn congest(s: &mut Source, h: &mut Harness) -> (Vec<SocketViewStream>, usize, Instant) {
    while s.opened.len() < 48 {
        s.open(h, 0).await;
    }
    for i in 0..48 {
        let stream = s.stream(i);
        h.sessions[0].initial(&stream).await;
        let metadata = acknowledged(s, h, i, 0).await;
        assert_eq!(metadata.kind, 3);
    }
    let streams = (0..48).map(|i| s.stream(i)).collect::<Vec<_>>();
    let mut prepared = Vec::new();
    for i in 0..48 {
        prepared.push(s.prepare(h, i, 0).await);
    }
    let began = Instant::now();
    for original in prepared {
        assert!(
            h.sessions[0]
                .outbound
                .send_shared(original, &h.sessions[0].attachment.client)
                .is_ok()
        );
    }
    until(|| {
        streams
            .iter()
            .any(|stream| stream.flush_poll_observation().pending > 0)
    })
    .await;
    let target = streams
        .iter()
        .position(|stream| stream.flush_poll_observation().pending > 0)
        .unwrap();
    let observation = streams[target].observation().unwrap();
    assert!(
        observation.highest_started == 2
            && observation.highest_flushed == 1
            && observation.highest_committed == 1
    );
    assert_eq!(streams[target].flush_poll_observation().success, 1);
    println!(
        "P1_PENDING stream_index={target} packet=2 real_pending_polls={} queued_streams=48",
        streams[target].flush_poll_observation().pending
    );
    (streams, target, began)
}
async fn drain_congested(h: &mut Harness, streams: &[SocketViewStream]) {
    for stream in streams {
        let p = h.sessions[0].received_credit(stream.id(), 2).await;
        assert_eq!(p.kind, 1);
    }
}
async fn finish(h: Harness, s: Source, outcome: Result<(), Box<dyn std::any::Any + Send>>) {
    let transport = AssertUnwindSafe(h.finish()).catch_unwind().await;
    let owners = AssertUnwindSafe(s.finish()).catch_unwind().await;
    if let Err(e) = outcome {
        std::panic::resume_unwind(e)
    }
    if let Err(e) = transport {
        std::panic::resume_unwind(e)
    }
    if let Err(e) = owners {
        std::panic::resume_unwind(e)
    }
}

async fn info(peer: &mut Session, ordinal: u16) {
    let mut id = [155; 16];
    id[14..].copy_from_slice(&ordinal.to_be_bytes());
    let id = Id128::new(id);
    peer.peer().send(PeerMessage::Text(serde_json::json!({"v":1,"type":"command","requestId":id,"action":"node.info","payload":{}}).to_string().into())).await.unwrap();
    let result = text(peer.peer()).await;
    assert!(
        result["type"] == "result"
            && result["requestId"] == serde_json::to_value(id).unwrap()
            && result["ok"] == true,
        "original node.info failed"
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sv061_real_authentication_component_source() {
    let mut h = Harness::new(false).await;
    let s = Source::new(&h, raw_bytes(100)).await;
    let outcome = AssertUnwindSafe(async {
        let (count, raw) = drain_snapshot(&s, &mut h, 0, 0).await;
        assert!(count >= 5 && raw.as_slice() == s.raw.as_ref());
        pass("sv061-original-policy-auth-source-read");
        let b = h.second(1).await;
        assert!(!Arc::ptr_eq(
            &h.sessions[0].attachment,
            &h.sessions[b].attachment
        ));
        assert!(!s.stream(0).belongs_to(&h.sessions[b].attachment.client));
        pass("sv061-other-authenticated-binding-refused");
    })
    .catch_unwind()
    .await;
    finish(h, s, outcome).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sv062_revoke_before_actual_chunk_copy() {
    let mut h = Harness::new(false).await;
    let mut s = Source::new(&h, raw_bytes(100)).await;
    let outcome = AssertUnwindSafe(async {
        let b = h.second(1).await;
        let stream = s.stream(0);
        h.sessions[0].initial(&stream).await;
        let mut gate = s
            .owner
            .test_controller()
            .arm_pause(
                ViewHookPoint::BeforeChunkCopy,
                HookSelector::Stream(s.opened[0].stream_id()),
            )
            .unwrap();
        let original = s
            .reads
            .next_snapshot(&h.sessions[0].attachment.client, &s.opened[0]);
        let revoke = async {
            bound(gate.arrived()).await.unwrap();
            h.revoke(0).await;
            gate.release();
        };
        let (result, ()) = tokio::join!(original, revoke);
        assert!(result.is_err(), "revoked actual read must not copy");
        assert_eq!(stream.observation().unwrap().highest_started, 0);
        pass("sv062-committed-revoke-before-copy");
        let r = s.open(&h, b).await;
        let (_, raw) = drain_snapshot(&s, &mut h, r, b).await;
        assert_eq!(raw.as_slice(), s.raw.as_ref());
        pass("sv062-trusted-b-original-content-readable");
    })
    .catch_unwind()
    .await;
    finish(h, s, outcome).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sv063_queued_bytes_do_not_authorize_send() {
    let mut h = Harness::new(false).await;
    let s = Source::new(&h, raw_bytes(100)).await;
    let outcome = AssertUnwindSafe(async {
        let stream = s.stream(0);
        h.sessions[0].initial(&stream).await;
        let mut gate = s
            .owner
            .test_controller()
            .arm_pause(
                ViewHookPoint::AfterChunkQueuedBeforeWriterGate,
                HookSelector::Stream(s.opened[0].stream_id()),
            )
            .unwrap();
        s.send(&h, 0, 0).await;
        bound(gate.arrived()).await.unwrap();
        h.revoke(0).await;
        gate.release();
        until(|| stream.explicitly_sealed().unwrap()).await;
        let o = stream.observation().unwrap();
        assert!(o.highest_started == 0 && o.highest_flushed == 0 && o.highest_committed == 0);
        pass("sv063-revoke-wins-before-writer-gate");
    })
    .catch_unwind()
    .await;
    finish(h, s, outcome).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sv064_actual_pending_holds_actor_gate_until_flush() {
    let mut h = Harness::new(true).await;
    let mut s = Source::new(&h, raw_bytes(32768)).await;
    let outcome = AssertUnwindSafe(async {
        let (streams, target, _) = congest(&mut s, &mut h).await;
        let actor = h
            .service
            .state
            .trust
            .client_gate(&h.sessions[0].client_id.to_base64url());
        assert!(actor.try_lock().is_err());
        let trust = h.service.state.trust.clone();
        let client = h.sessions[0].client_id.to_base64url();
        let operation = Id128::new([163; 16]).to_base64url();
        let mut revoke =
            Box::pin(async move { trust.revoke("local-owner", &operation, &client).await });
        let polled =
            std::future::poll_fn(|cx| std::task::Poll::Ready(revoke.as_mut().poll(cx))).await;
        assert!(polled.is_pending());
        assert!(streams[target].flush_poll_observation().pending > 0);
        pass("sv064-real-flush-pending-revoke-waits");
        for stream in streams.iter().take(target + 1) {
            let p = h.sessions[0].received_credit(stream.id(), 2).await;
            assert_eq!(p.kind, 1);
        }
        bound(revoke).await.unwrap();
        until(|| streams[target].explicitly_sealed().unwrap()).await;
        let target_observation = streams[target].observation().unwrap();
        assert!(
            target_observation.highest_started == 2
                && target_observation.highest_flushed == 2
                && target_observation.highest_committed <= 2
        );
        assert!(
            streams.iter().skip(target + 1).all(|stream| stream
                .observation()
                .unwrap()
                .highest_started
                == 1)
        );
        assert!(
            s.reads
                .next_snapshot(&h.sessions[0].attachment.client, &s.opened[target])
                .await
                .is_err()
        );
        pass("sv064-one-started-boundary-no-next-chunk");
    })
    .catch_unwind()
    .await;
    finish(h, s, outcome).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sv065_source_seal_before_and_after_actual_start() {
    for after_flush in [false, true] {
        let mut h = Harness::new(false).await;
        let s = Source::new(&h, raw_bytes(100)).await;
        let outcome = AssertUnwindSafe(async {
            let stream = s.stream(0);
            h.sessions[0].initial(&stream).await;
            let point = if after_flush {
                ViewHookPoint::AfterFlushFactBeforePublish
            } else {
                ViewHookPoint::BeforeSinkReady
            };
            let mut gate = s
                .owner
                .test_controller()
                .arm_pause(point, HookSelector::Stream(s.opened[0].stream_id()))
                .unwrap();
            s.send(&h, 0, 0).await;
            bound(gate.arrived()).await.unwrap();
            if after_flush {
                let bytes = binary(h.sessions[0].peer()).await;
                assert_eq!(packet(&bytes).stream, stream.id());
            }
            s.owner.handle().core.seal().unwrap();
            gate.release();
            until(|| stream.explicitly_sealed().unwrap()).await;
            let o = stream.observation().unwrap();
            assert_eq!(o.highest_committed, 0);
            if after_flush {
                assert!(o.highest_started == 1 && o.highest_flushed == 1);
                pass("sv065-source-seal-retains-actual-flush-no-cursor");
            } else {
                assert!(o.highest_started == 0 && o.highest_flushed == 0);
                pass("sv065-source-seal-before-start-no-send");
            }
        })
        .catch_unwind()
        .await;
        finish(h, s, outcome).await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sv066_control_auth_strict_shape_depth_and_command_limit() {
    let mut h = Harness::new(false).await;
    let mut s = Source::new(&h, raw_bytes(100)).await;
    let outcome=AssertUnwindSafe(async {
        let mut raw=h.raw_peer(false).await;raw.send(PeerMessage::Binary(control_wire(2,Id128::new([5;16]),0).into())).await.unwrap();closed(&mut raw,4400).await;drop(raw);pass("sv066-unauthenticated-binary-rejected");
        for (mode,label,expected) in [(0,"sv066-text-over-32k",4429),(1,"sv066-depth-nine",4400),(2,"sv066-unknown-dto-field",4400),(3,"sv066-detach-shape-reason",4400)] {
            let index=h.second(0).await;
            let detach_stream = if mode == 3 {
                let positive_read = s.open(&h, index).await;
                let positive_stream = s.stream(positive_read);
                assert!(!positive_stream.explicitly_sealed().unwrap());
                h.sessions[index].control(control_wire(3, positive_stream.id(), 0)).await;
                until(|| positive_stream.explicitly_sealed().unwrap()).await;
                info(&mut h.sessions[index], 66).await;
                let negative_read = s.open(&h, index).await;
                let negative_stream = s.stream(negative_read);
                assert!(!negative_stream.explicitly_sealed().unwrap());
                assert_ne!(negative_stream.id(), positive_stream.id());
                Some(negative_stream.id())
            } else {
                None
            };
            let payload=match mode {
                0=>PeerMessage::Text("x".repeat(32769).into()),
                1=>{let mut nested=Value::Null;for _ in 0..9{nested=serde_json::json!({"a":nested});}PeerMessage::Text(serde_json::json!({"v":1,"type":"command","requestId":Id128::new([3;16]),"action":"node.info","payload":nested}).to_string().into())},
                2=>PeerMessage::Text(serde_json::json!({"v":1,"type":"command","requestId":Id128::new([4;16]),"action":"node.info","payload":{},"extra":true}).to_string().into()),
                _=>{let valid=control_wire(3,detach_stream.unwrap(),0);let mut b=valid.clone();b[32]=1;assert_eq!(b.len(),72);let differences:Vec<_>=b.iter().zip(&valid).enumerate().filter_map(|(i,(a,z))|(a!=z).then_some(i)).collect();assert_eq!(differences,vec![32]);PeerMessage::Binary(b.into())}
            };
            h.sessions[index].peer().send(payload).await.unwrap();closed(h.sessions[index].peer(),expected).await;h.sessions[index].abrupt(false);pass(label);
        }
        let start=Instant::now();for i in 1..=120 {info(&mut h.sessions[0],i).await;}assert!(start.elapsed()<Duration::from_secs(60));
        h.sessions[0].peer().send(PeerMessage::Text(serde_json::json!({"v":1,"type":"command","requestId":Id128::new([8;16]),"action":"node.info","payload":{}}).to_string().into())).await.unwrap();closed(h.sessions[0].peer(),4429).await;pass("sv066-real-command-120-121");
    }).catch_unwind().await;
    finish(h, s, outcome).await;
}
#[test]
fn sv067_independent_72_byte_wire_oracle_and_mutations() {
    let stream = Id128::new([181; 16]);
    let payload = [0x80, 0xf0, 0x9f, 0x8c, 0x8d];
    let offset = u64::MAX - 5;
    let header = TransferHeader::data(
        stream,
        9007199254740993,
        18446744073709551614,
        offset,
        u64::MAX,
        RecordKind::OriginalRaw,
        5,
    )
    .unwrap();
    let mut oracle = vec![0; 77];
    oracle[..4].copy_from_slice(b"ACPV");
    oracle[4] = 1;
    oracle[5] = 1;
    oracle[7] = 1;
    oracle[8..24].copy_from_slice(stream.as_bytes());
    for (at, value) in [
        (24, 9007199254740993u64),
        (32, 18446744073709551614),
        (40, offset),
        (48, u64::MAX),
    ] {
        oracle[at..at + 8].copy_from_slice(&value.to_be_bytes());
    }
    oracle[56] = 1;
    oracle[64..68].copy_from_slice(&5u32.to_be_bytes());
    oracle[72..].copy_from_slice(&payload);
    assert_eq!(header.encode(&payload).unwrap(), oracle);
    let (decoded, body) = TransferHeader::decode(&oracle).unwrap();
    assert!(decoded == header && body == payload);
    pass("sv067-exact-header-u64-and-opaque-bytes");
    for at in [
        0, 1, 2, 3, 4, 5, 6, 7, 56, 57, 58, 59, 60, 61, 62, 63, 68, 69, 70, 71,
    ] {
        let mut bad = oracle.clone();
        bad[at] = 255;
        assert!(
            TransferHeader::decode(&bad).is_err(),
            "closed header mutation accepted"
        );
    }
    for n in 0..72 {
        assert!(TransferHeader::decode(&oracle[..n]).is_err());
    }
    for n in [72, 73, 74, 75, 76] {
        assert!(TransferHeader::decode(&oracle[..n]).is_err());
    }
    let mut extra = oracle.clone();
    extra.push(0);
    assert!(TransferHeader::decode(&extra).is_err());
    for range in [24..32, 32..40] {
        let mut bad = oracle.clone();
        bad[range].fill(0);
        assert!(TransferHeader::decode(&bad).is_err());
    }
    assert!(TransferHeader::data(stream, 1, 1, u64::MAX, 0, RecordKind::OriginalRaw, 1).is_err());
    assert!(TransferHeader::data(stream, 1, 1, 0, 16385, RecordKind::OriginalRaw, 16385).is_err());
    for kind in [
        RecordKind::OriginalRaw,
        RecordKind::ItemProjection,
        RecordKind::Status,
    ] {
        assert!(TransferHeader::data(stream, 1, 1, 0, 0, kind, 0).is_err());
    }
    let empty = TransferHeader::data(stream, 1, 1, 0, 0, RecordKind::SnapshotEnd, 0)
        .unwrap()
        .encode(&[])
        .unwrap();
    assert!(empty.len() == 72 && packet(&empty).kind == 4);
    for (kind, ack) in [(2, 0), (2, u64::MAX), (3, 0)] {
        let expected = control_wire(kind, stream, ack);
        let h = if kind == 2 {
            TransferHeader::credit(stream, ack)
        } else {
            TransferHeader::detach(stream)
        };
        assert_eq!(h.encode(&[]).unwrap(), expected);
        assert_eq!(TransferHeader::decode(&expected).unwrap().0, h);
    }
    pass("sv067-closed-header-length-record-offset-and-control-matrix");
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sv068_streaming_raw_bytes_cross_utf8_and_unknown_number() {
    let mut h = Harness::new(false).await;
    let mut crossing =
        b"{\"unknown\":1,\"unknown\":2,\"n\":900719925474099312345,\"text\":\"".to_vec();
    crossing.resize(16383, b'x');
    crossing.extend_from_slice("🌍\\u0041".as_bytes());
    crossing.resize(70000, b'x');
    crossing.extend_from_slice(b"\"}");
    let s = Source::new(&h, crossing.into()).await;
    let outcome = AssertUnwindSafe(async {
        let (_, bytes) = drain_snapshot(&s, &mut h, 0, 0).await;
        assert_eq!(bytes.as_slice(), s.raw.as_ref());
        assert!(std::str::from_utf8(&bytes).is_ok());
        assert!(
            s.fragmented_utf8.load(Ordering::Relaxed) >= 2,
            "actual payload boundaries must split the four-byte UTF8 character"
        );
        pass("sv068-real-peer-reassembly-original-utf8-duplicates-big-number");
        let data = TransferHeader::data(
            Id128::new([52; 16]),
            u64::MAX,
            u64::MAX,
            9007199254740993,
            9007199254740994,
            RecordKind::OriginalRaw,
            1,
        )
        .unwrap();
        let b = data.encode(&[0x80]).unwrap();
        let (d, p) = TransferHeader::decode(&b).unwrap();
        assert!(d.total() == 9007199254740994 && p == [0x80]);
        pass("sv068-unallocatable-length-format-only");
    })
    .catch_unwind()
    .await;
    finish(h, s, outcome).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sv069_single_credit_duplicates_future_and_bindings() {
    let mut h = Harness::new(false).await;
    let s = Source::new(&h, raw_bytes(100)).await;
    let outcome = AssertUnwindSafe(async {
        let stream = s.stream(0);
        h.sessions[0].initial(&stream).await;
        h.sessions[0].control(control_wire(2, stream.id(), 0)).await;
        s.send(&h, 0, 0).await;
        let bytes = binary(h.sessions[0].peer()).await;
        assert_eq!(packet(&bytes).sequence, 1);
        let premature = stream
            .accept_control(
                &h.sessions[0].attachment.client,
                &control_wire(2, stream.id(), 2),
            )
            .await;
        assert!(premature.is_err());
        assert_eq!(stream.observation().unwrap().highest_committed, 0);
        assert!(matches!(
            s.reads
                .next_snapshot(&h.sessions[0].attachment.client, &s.opened[0])
                .await,
            Ok(PrepareNext::AwaitCredit)
        ));
        pass("sv069-future-ack-does-not-grant-cursor");
        h.sessions[0].control(control_wire(2, stream.id(), 1)).await;
        until(|| stream.observation().unwrap().highest_committed == 1).await;
        h.sessions[0].control(control_wire(2, stream.id(), 1)).await;
        let p = acknowledged(&s, &mut h, 0, 0).await;
        assert_eq!(p.sequence, 2);
        pass("sv069-initial-and-duplicate-credit-nonaccumulating");
        let b = h.second(1).await;
        assert!(h.sessions[b].attachment.attach_read(&s.opened[0]).is_err());
        assert!(
            stream
                .accept_control(
                    &h.sessions[b].attachment.client,
                    &control_wire(2, stream.id(), 2)
                )
                .await
                .is_err()
        );
        let other = control_wire(2, Id128::new([176; 16]), 2);
        assert!(
            stream
                .accept_control(&h.sessions[0].attachment.client, &other)
                .await
                .is_err()
        );
        pass("sv069-actual-other-actor-socket-stream-rejected");
    })
    .catch_unwind()
    .await;
    finish(h, s, outcome).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sv070_real_peer_data_then_credit_before_publication() {
    let mut h = Harness::new(false).await;
    let s = Source::new(&h, raw_bytes(100)).await;
    let outcome = AssertUnwindSafe(async {
        let stream = s.stream(0);
        h.sessions[0].initial(&stream).await;
        let mut before_start = s
            .owner
            .test_controller()
            .arm_pause(
                ViewHookPoint::BeforeSinkReady,
                HookSelector::Stream(s.opened[0].stream_id()),
            )
            .unwrap();
        let mut gate = s
            .owner
            .test_controller()
            .arm_pause(
                ViewHookPoint::AfterFlushFactBeforePublish,
                HookSelector::Stream(s.opened[0].stream_id()),
            )
            .unwrap();
        s.send(&h, 0, 0).await;
        bound(before_start.arrived()).await.unwrap();
        assert!(
            stream
                .accept_control(
                    &h.sessions[0].attachment.client,
                    &control_wire(2, stream.id(), 1)
                )
                .await
                .is_err()
        );
        assert_eq!(stream.observation().unwrap().highest_started, 0);
        pass("sv070-before-start-future-credit-refused");
        before_start.release();
        bound(gate.arrived()).await.unwrap();
        let data = h.sessions[0].received_credit(stream.id(), 1).await;
        assert_eq!(data.kind, 3);
        until(|| stream.observation().unwrap().pending_early_credit).await;
        let before = stream.observation().unwrap();
        assert!(before.highest_flushed == 1 && before.highest_committed == 0);
        assert!(
            stream
                .accept_control(
                    &h.sessions[0].attachment.client,
                    &control_wire(2, stream.id(), 2)
                )
                .await
                .is_err()
        );
        pass("sv070-peer-received-target-data-early-credit-held-only");
        gate.release();
        until(|| stream.observation().unwrap().highest_committed == 1).await;
        assert_eq!(acknowledged(&s, &mut h, 0, 0).await.sequence, 2);
        pass("sv070-actual-flush-publication-linearizes-once");
    })
    .catch_unwind()
    .await;
    finish(h, s, outcome).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sv071_more_than_120_data_credit_keep_command_window() {
    let mut h = Harness::new(false).await;
    let s = Source::new(&h, raw_bytes(132 * 16384)).await;
    let outcome=AssertUnwindSafe(async {let began=Instant::now();let (frames,raw)=drain_snapshot(&s,&mut h,0,0).await;assert!(frames>120&&raw.as_slice()==s.raw.as_ref());
        assert!(!h.service.state.test_hooks.fresh_command_window.load(Ordering::SeqCst));pass("sv071-over120-real-peer-data-credit");
        for i in 1..=120 {info(&mut h.sessions[0],i).await;}assert!(began.elapsed()<Duration::from_secs(60),"must remain same original 60s window");
        h.sessions[0].peer().send(PeerMessage::Text(serde_json::json!({"v":1,"type":"command","requestId":Id128::new([9;16]),"action":"node.info","payload":{}}).to_string().into())).await.unwrap();closed(h.sessions[0].peer(),4429).await;pass("sv071-management120-121-after-credit-same-socket-window");
    }).catch_unwind().await;
    finish(h, s, outcome).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sv072_real_backpressure_socket_isolation_and_queue_bounds() {
    let mut h = Harness::new(true).await;
    let mut s = Source::new(&h, raw_bytes(32768)).await;
    let outcome = AssertUnwindSafe(async {
        let b = h.second(1).await;
        let (streams, _, _) = congest(&mut s, &mut h).await;
        info(&mut h.sessions[b], 1).await;
        assert!(h.sessions[0].outbound.bytes.available_permits() <= MAX_OUTBOUND_BYTES);
        pass("sv072-real-flush-pending-other-actor-socket-progress");
        drain_congested(&mut h, &streams).await;
        until(|| {
            streams
                .iter()
                .all(|stream| stream.observation().unwrap().highest_committed == 2)
        })
        .await;
        let read = s.open(&h, b).await;
        let (_, raw) = drain_snapshot(&s, &mut h, read, b).await;
        assert_eq!(raw.as_slice(), s.raw.as_ref());
        pass("sv072-resume-preserves-original-node-source-content");
    })
    .catch_unwind()
    .await;
    finish(h, s, outcome).await;
    for bytes in [false, true] {
        let mut h = Harness::new(false).await;
        let mut s = Source::new(&h, raw_bytes(100)).await;
        let outcome = AssertUnwindSafe(async {
            let stream = s.stream(0);
            h.sessions[0].initial(&stream).await;
            let mut gate = s
                .owner
                .test_controller()
                .arm_pause(
                    ViewHookPoint::BeforeSinkReady,
                    HookSelector::Stream(s.opened[0].stream_id()),
                )
                .unwrap();
            let extra = s.open(&h, 0).await;
            let spare_stream = s.stream(extra);
            h.sessions[0].initial(&spare_stream).await;
            let first = s.prepare(&h, 0, 0).await;
            let spare = s.prepare(&h, extra, 0).await;
            let original_header = spare.header();
            let original_length = spare.byte_len();
            assert!(
                h.sessions[0]
                    .outbound
                    .send_shared(first, &h.sessions[0].attachment.client)
                    .is_ok()
            );
            bound(gate.arrived()).await.unwrap();
            let payload = if bytes {
                Value::String("q".repeat(8192))
            } else {
                Value::Null
            };
            let mut accepted = 0;
            loop {
                if h.sessions[0].outbound.send_json(&payload).is_err() {
                    break;
                }
                accepted += 1;
                assert!(accepted <= 256);
            }
            if bytes {
                assert!(accepted < 256);
            } else {
                assert_eq!(accepted, 256);
            }
            let held = if bytes {
                let available = h.sessions[0].outbound.bytes.available_permits();
                Some(
                    h.sessions[0]
                        .outbound
                        .bytes
                        .clone()
                        .try_acquire_many_owned(available as u32)
                        .unwrap(),
                )
            } else {
                None
            };
            let rejected = match h.sessions[0]
                .outbound
                .send_shared(spare, &h.sessions[0].attachment.client)
            {
                Err(SocketEnqueueError::Rejected(original)) => original,
                _ => panic!("queue rejection must return original Prepared"),
            };
            assert!(rejected.header() == original_header && rejected.byte_len() == original_length);
            assert_eq!(spare_stream.observation().unwrap().highest_started, 0);
            drop(rejected);
            drop(held);
            if bytes {
                pass("sv072-original-1mib-byte-permit-rejection");
            } else {
                pass("sv072-original-256-frame-queue-rejection");
            }
            assert_eq!(stream.observation().unwrap().highest_started, 0);
            gate.release();
        })
        .catch_unwind()
        .await;
        finish(h, s, outcome).await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sv073_detach_read_close_and_new_connection_current_open() {
    let mut h = Harness::new(false).await;
    let mut s = Source::new(&h, raw_bytes(100)).await;
    let outcome = AssertUnwindSafe(async {
        let stream = s.stream(0);
        h.sessions[0].initial(&stream).await;
        let p = acknowledged(&s, &mut h, 0, 0).await;
        assert_eq!(p.sequence, 1);
        h.sessions[0].control(control_wire(3, stream.id(), 0)).await;
        until(|| stream.explicitly_sealed().unwrap()).await;
        let old_result = s
            .reads
            .next_snapshot(&h.sessions[0].attachment.client, &s.opened[0])
            .await;
        assert!(matches!(old_result, Err(_) | Ok(PrepareNext::Closed)));
        pass("sv073-wire-detach-only-old-stream");
        let b = h.second(0).await;
        assert!(h.sessions[b].attachment.attach_read(&s.opened[0]).is_err());
        let index = s.open(&h, b).await;
        assert_ne!(s.stream(index).id(), stream.id());
        let (_, raw) = drain_snapshot(&s, &mut h, index, b).await;
        assert_eq!(raw.as_slice(), s.raw.as_ref());
        pass("sv073-new-real-socket-fresh-stream-same-retained-content");
        h.sessions[b]
            .peer()
            .close(Some(tokio_tungstenite::tungstenite::protocol::CloseFrame {
                code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Normal,
                reason: "".into(),
            }))
            .await
            .unwrap();
        closed(h.sessions[b].peer(), 1000).await;
        h.sessions[b].abrupt(false);
        until(|| s.stream(index).explicitly_sealed().unwrap()).await;
        pass("sv073-real-close-handshake-seals-reader");
    })
    .catch_unwind()
    .await;
    finish(h, s, outcome).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sv074_real_flush_error_and_original_fifteen_second_deadline() {
    for deadline in [false, true] {
        let mut h = Harness::new(true).await;
        let mut s = Source::new(&h, raw_bytes(32768)).await;
        let outcome = AssertUnwindSafe(async {
            let (streams, target, began) = congest(&mut s, &mut h).await;
            if deadline {
                timeout(Duration::from_secs(18), async {
                    while !streams[target].explicitly_sealed().unwrap() {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("original 15s writer deadline must terminate");
                assert!(
                    began.elapsed() >= Duration::from_secs(14)
                        && began.elapsed() < Duration::from_secs(18)
                );
                assert_eq!(streams[target].flush_poll_observation().error, 0);
                pass("sv074-original15s-deadline-no-refresh-no-peer-close");
            } else {
                h.sessions[0].abrupt(true);
                until(|| streams[target].flush_poll_observation().error > 0).await;
                until(|| streams[target].explicitly_sealed().unwrap()).await;
                pass("sv074-real-poll-flush-error-after-owned-peer-reset");
            }
            let o = streams[target].observation().unwrap();
            assert!(o.highest_flushed == 1 && o.highest_committed == 1);
            assert!(streams[target].explicitly_sealed().unwrap());
            let b = h.second(1).await;
            let r = s.open(&h, b).await;
            let (_, raw) = drain_snapshot(&s, &mut h, r, b).await;
            assert_eq!(raw.as_slice(), s.raw.as_ref());
            if deadline {
                pass("sv074-deadline-no-cursor-new-open-retains-content");
            } else {
                pass("sv074-flush-error-no-cursor-new-open-retains-content");
            }
        })
        .catch_unwind()
        .await;
        finish(h, s, outcome).await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sv075_public_formatting_does_not_expose_native_canaries() {
    let mut h = Harness::new(false).await;
    let s = Source::new(&h, raw_bytes(200)).await;
    let outcome = AssertUnwindSafe(async {
        let descriptor = format!("{:?}", s.opened[0].descriptor());
        let strings = [
            descriptor,
            format!("{:?}", ViewError::InvalidClient),
            ViewError::InvalidClient.to_string(),
            format!("{:?}", WireError::Header),
            WireError::Header.to_string(),
        ];
        for formatted in strings {
            assert!(
                !formatted.contains("independent-native-canary")
                    && !formatted.contains("900719925474099312345")
                    && !formatted.contains("unknown"),
                "public formatting exposed native canary"
            );
        }
        pass("sv075-public-descriptor-and-error-formatting-static-categories");
        let (_, raw) = drain_snapshot(&s, &mut h, 0, 0).await;
        assert_eq!(raw.as_slice(), s.raw.as_ref());
        pass("sv075-current-body-path-still-opaque-original-bytes");
    })
    .catch_unwind()
    .await;
    finish(h, s, outcome).await;
}
