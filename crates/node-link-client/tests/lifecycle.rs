//! 连接生命周期：真实握手的 Owner 侧脚本、catalog 投递与撤销后停止重连。
//!
//! 客户端的握手、信封、序号与撤销判定都走真实实现；只有网络承载是内存通道。

mod support;

use std::sync::Arc;
use std::time::Duration;

use acp_core::model::{
    AttachmentId, NodeId, OriginCursor, OriginEpoch, PeerPublicKey, Sequence, SessionId, Timestamp,
};
use async_trait::async_trait;
use identity_auth::P1363Signature;
use node_link_protocol::attachment::{FrameBody, StatusBody, TransferState};
use node_link_protocol::catalog::{
    AgentEntryList, CatalogSessionList, CatalogSnapshot, DirectoryEntryList,
};
use node_link_protocol::common::{Text, Timestamp as WireTimestamp};
use node_link_protocol::envelope::MessageType;

use node_link_client::handshake::OwnerTrust;
use node_link_client::identity::NodeIdentity;
use node_link_client::{ClientConfig, ClientError, NodeLinkClient, TlsTrust, TransportFactory};

use support::{
    MemTransport, OwnerIdentity, OwnerIo, RecordingDeliveries, RecordingSink, accept_handshake,
    decimal, encode_envelope, uuid_text,
};

const ACCESS_NODE: &str = "9f1c3a52-6a4e-4f0e-8d3c-5b2a1e0f7c11";

/// 固定 P-256 身份的 `NodeIdentity`（测试不需要平台 keystore）。
struct FixedIdentity {
    node_id: NodeId,
    signing: p256::ecdsa::SigningKey,
}

impl FixedIdentity {
    fn new(seed: [u8; 32]) -> Self {
        Self {
            node_id: NodeId::new(ACCESS_NODE).expect("node id"),
            signing: p256::ecdsa::SigningKey::from_slice(&seed).expect("signing key"),
        }
    }
}

#[async_trait]
impl NodeIdentity for FixedIdentity {
    fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    async fn public_key(&self) -> Result<PeerPublicKey, ClientError> {
        let encoded = self.signing.verifying_key().to_sec1_point(false);
        PeerPublicKey::try_from_bytes(encoded.as_bytes())
            .map_err(|_| ClientError::Local("test key"))
    }

    async fn sign(&self, transcript: &[u8]) -> Result<P1363Signature, ClientError> {
        use p256::ecdsa::signature::Signer as _;
        let signature: p256::ecdsa::Signature = self.signing.sign(transcript);
        P1363Signature::try_from_bytes(signature.to_bytes().as_slice())
            .map_err(|_| ClientError::Local("test signature"))
    }
}

fn owner_trust(owner: &OwnerIdentity) -> OwnerTrust {
    OwnerTrust {
        node_id: owner.node_id.clone(),
        public_key: owner.public_key.clone(),
    }
}

async fn wait_for<F: Fn() -> bool>(condition: F, what: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

/// 握手 → catalog 投递 → `node.trust.revoked`；撤销后**不再重连**（transport 只被连接一次）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revocation_stops_reconnect_and_never_reopens_the_transport() {
    let owner = OwnerIdentity::new([0x31; 32], "5c6d7e8f-9012-4a3b-8c4d-5e6f708192a3");
    let transport = Arc::new(MemTransport::default());
    let (client_connection, mut io) = OwnerIo::pair();
    transport.push(client_connection);

    let features = [
        "node-link.core.v1",
        "node-link.command-status.v1",
        "node-link.session-create.v1",
    ];
    let owner_task = tokio::spawn(async move {
        let session = accept_handshake(&mut io, &owner, &features).await;
        let mut inbound = session.inbound_next;
        let subscribe = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");
        assert_eq!(subscribe.message_type(), MessageType::CatalogSubscribe);
        let snapshot = CatalogSnapshot {
            revision: decimal(1),
            agents: AgentEntryList::new(Vec::new()).expect("agents"),
            directories: DirectoryEntryList::new(Vec::new()).expect("directories"),
            sessions: CatalogSessionList::new(Vec::new()).expect("sessions"),
        };
        io.send_envelope(
            MessageType::CatalogSnapshot,
            &session.connection_id,
            &snapshot,
        )
        .await;
        let revoked = node_link_protocol::catalog::NodeTrustRevoked {
            revoked_node_id: uuid_text(ACCESS_NODE),
            revoked_at: WireTimestamp::parse("2026-10-20T00:00:00.000Z").expect("timestamp"),
            reason: Text::<512>::parse("revoked in test").expect("reason"),
        };
        io.send_envelope(
            MessageType::NodeTrustRevoked,
            &session.connection_id,
            &revoked,
        )
        .await;
    });

    let identity = FixedIdentity::new([0x41; 32]);
    let sink = Arc::new(RecordingSink::default());
    let deliveries = Arc::new(RecordingDeliveries::default());
    let client = NodeLinkClient::new(
        transport.factory(),
        Arc::new(identity),
        sink.clone(),
        deliveries,
    );
    let link = client
        .open(ClientConfig::new(
            owner_trust(&OwnerIdentity::new(
                [0x31; 32],
                "5c6d7e8f-9012-4a3b-8c4d-5e6f708192a3",
            )),
            "wss://owner.test/node-link/v1",
            TlsTrust::PlaintextDev,
        ))
        .expect("open");

    wait_for(
        || *sink.catalogs.lock().expect("catalogs") == 1,
        "catalog snapshot",
    )
    .await;
    wait_for(|| link.is_revoked(), "revocation").await;
    assert!(!link.is_online(), "a revoked link must be offline");
    owner_task.await.expect("owner task");

    // 撤销后 supervisor 必须停止重连：再等一小段时间，连接次数仍是 1。
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(transport.opens(), 1, "撤销后不得重连");
    assert!(link.shared().uncertain().is_empty());
    link.shutdown().await;
}

/// 迟到的心跳不会破坏撤销语义：`link.error`（`auth.node_revoked`）同样停止重连。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_auth_revoked_error_also_stops_the_link() {
    let owner = OwnerIdentity::new([0x32; 32], "5c6d7e8f-9012-4a3b-8c4d-5e6f708192a4");
    let transport = Arc::new(MemTransport::default());
    let (client_connection, mut io) = OwnerIo::pair();
    transport.push(client_connection);
    let owner_for_task = OwnerIdentity::new([0x32; 32], "5c6d7e8f-9012-4a3b-8c4d-5e6f708192a4");
    let owner_task = tokio::spawn(async move {
        let session = accept_handshake(
            &mut io,
            &owner_for_task,
            &["node-link.core.v1", "node-link.command-status.v1"],
        )
        .await;
        let mut inbound = session.inbound_next;
        let _ = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");
        let error = node_link_protocol::error::Body::new(
            node_link_protocol::error::ErrorCode::AuthNodeRevoked,
            "revoked",
            false,
        )
        .expect("error body");
        let envelope = encode_envelope(
            MessageType::LinkError,
            Some(node_link_protocol::envelope::ConnectionFields {
                connection_id: session.connection_id.clone(),
                connection_sequence: decimal(2),
            }),
            &error,
        );
        io.send_raw(envelope).await;
    });

    let client = NodeLinkClient::new(
        transport.factory(),
        Arc::new(FixedIdentity::new([0x42; 32])),
        Arc::new(RecordingSink::default()),
        Arc::new(RecordingDeliveries::default()),
    );
    let link = client
        .open(ClientConfig::new(
            owner_trust(&owner),
            "wss://owner.test/node-link/v1",
            TlsTrust::PlaintextDev,
        ))
        .expect("open");
    wait_for(|| link.is_revoked(), "revocation via link.error").await;
    owner_task.await.expect("owner task");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        transport.opens(),
        1,
        "link.error(auth.node_revoked) 后不得重连"
    );
    link.shutdown().await;
}

/// 一个永远不返回数据的本地源：模拟阻塞在文件/管道读取上的上传。
struct NeverReady;

impl tokio::io::AsyncRead for NeverReady {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Pending
    }
}

/// 打开一个到固定 Owner 的客户端（supervisor 由本函数启动）。
fn open_link(
    transport: &Arc<MemTransport>,
    owner: &OwnerIdentity,
    seed: [u8; 32],
) -> Arc<node_link_client::OwnerLink> {
    let client = NodeLinkClient::new(
        transport.factory(),
        Arc::new(FixedIdentity::new(seed)),
        Arc::new(RecordingSink::default()),
        Arc::new(RecordingDeliveries::default()),
    );
    client
        .open(ClientConfig::new(
            owner_trust(owner),
            "wss://owner.test/node-link/v1",
            TlsTrust::PlaintextDev,
        ))
        .expect("open")
}

/// 多个 Owner 的 `shutdown` 必须经由真实的关闭通道立刻结束 supervisor，
/// 而不是每个都等满 5 s 兜底（旧的「无接收者 watch sender」会这样串行阻塞）。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shutdown_stops_every_supervisor_without_waiting_for_the_timeout() {
    let first_owner = OwnerIdentity::new([0x44; 32], "5c6d7e8f-9012-4a3b-8c4d-5e6f708192c2");
    let second_owner = OwnerIdentity::new([0x45; 32], "5c6d7e8f-9012-4a3b-8c4d-5e6f708192c3");
    let first_transport = Arc::new(MemTransport::default());
    let second_transport = Arc::new(MemTransport::default());
    let (first_connection, mut first_io) = OwnerIo::pair();
    let (second_connection, mut second_io) = OwnerIo::pair();
    first_transport.push(first_connection);
    second_transport.push(second_connection);
    let (first_stop_tx, first_stop_rx) = tokio::sync::oneshot::channel();
    let (second_stop_tx, second_stop_rx) = tokio::sync::oneshot::channel();
    let first_owner_for_task =
        OwnerIdentity::new([0x44; 32], "5c6d7e8f-9012-4a3b-8c4d-5e6f708192c2");
    let second_owner_for_task =
        OwnerIdentity::new([0x45; 32], "5c6d7e8f-9012-4a3b-8c4d-5e6f708192c3");
    let first_task = tokio::spawn(async move {
        let session =
            accept_handshake(&mut first_io, &first_owner_for_task, &["node-link.core.v1"]).await;
        let mut inbound = session.inbound_next;
        let _ = first_io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await;
        first_stop_rx.await.expect("stop");
    });
    let second_task = tokio::spawn(async move {
        let session = accept_handshake(
            &mut second_io,
            &second_owner_for_task,
            &["node-link.core.v1"],
        )
        .await;
        let mut inbound = session.inbound_next;
        let _ = second_io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await;
        second_stop_rx.await.expect("stop");
    });

    let first_link = open_link(&first_transport, &first_owner, [0x46; 32]);
    let second_link = open_link(&second_transport, &second_owner, [0x47; 32]);
    wait_for(|| first_link.is_online(), "first link online").await;
    wait_for(|| second_link.is_online(), "second link online").await;

    let started = std::time::Instant::now();
    tokio::time::timeout(Duration::from_secs(1), async {
        first_link.shutdown().await;
        second_link.shutdown().await;
    })
    .await
    .expect("each shutdown must signal the supervisor instead of waiting for the 5 s fallback");
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(!first_link.is_online());
    assert!(!second_link.is_online());
    assert_eq!(first_transport.opens(), 1);
    assert_eq!(second_transport.opens(), 1);
    first_stop_tx.send(()).expect("stop first owner");
    second_stop_tx.send(()).expect("stop second owner");
    first_task.await.expect("first owner task");
    second_task.await.expect("second owner task");
}

/// 附件连接的入站序号从握手之后开始（`node.ready` 占 1），并随 `shutdown` 立即中止在途读取。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn attachment_connections_use_the_post_handshake_sequence_and_abort_on_shutdown() {
    let owner = OwnerIdentity::new([0x48; 32], "5c6d7e8f-9012-4a3b-8c4d-5e6f708192c4");
    let transport = Arc::new(MemTransport::default());
    let (business, mut business_io) = OwnerIo::pair();
    let (first_attachment, mut first_io) = OwnerIo::pair();
    let (second_attachment, mut second_io) = OwnerIo::pair();
    let (third_attachment, mut third_io) = OwnerIo::pair();
    transport.push(business);
    transport.push(first_attachment);
    transport.push(second_attachment);
    transport.push(third_attachment);
    let (second_status_tx, second_status_rx) = tokio::sync::oneshot::channel();
    let (upload_started_tx, upload_started_rx) = tokio::sync::oneshot::channel();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let owner_for_task = OwnerIdentity::new([0x48; 32], "5c6d7e8f-9012-4a3b-8c4d-5e6f708192c4");
    let features = ["node-link.core.v1", "node-link.attachment-transfer.v1"];
    let owner_task = tokio::spawn(async move {
        let session = accept_handshake(&mut business_io, &owner_for_task, &features).await;
        let mut inbound = session.inbound_next;
        let _ = business_io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");

        // 第一条附件连接：真实握手（`node.ready` 是 Owner 的序号 1）→ 回复 status 用序号 2。
        let first = accept_handshake(&mut first_io, &owner_for_task, &features).await;
        let mut first_inbound = first.inbound_next;
        let request = first_io
            .recv_frame(&first.connection_id, &mut first_inbound)
            .await
            .expect("attachment.status");
        assert!(matches!(request.body, FrameBody::Status(_)));
        first_io
            .send_frame(
                &first.connection_id,
                FrameBody::Status(StatusBody {
                    transfer_id: uuid_text("018f0000-0000-4000-8000-000000000301"),
                    state: TransferState::Receiving,
                    received_length: decimal(0),
                    next_offset: decimal(0),
                    sealed_sha256: node_link_protocol::common::Nullable::null(),
                }),
            )
            .await;

        // 第二条附件连接：收下 status 请求但不回复，等待本机 shutdown 中止它。
        let second = accept_handshake(&mut second_io, &owner_for_task, &features).await;
        let mut second_inbound = second.inbound_next;
        let request = second_io
            .recv_frame(&second.connection_id, &mut second_inbound)
            .await
            .expect("attachment.status");
        assert!(matches!(request.body, FrameBody::Status(_)));
        second_status_tx.send(()).expect("second status observed");

        // 第三条附件连接：上传只发出了 `attachment.begin`，客户端随后阻塞在本地源读取上；
        // shutdown 必须直接打断这次读取，而不是等下一次发送帧。
        let third = accept_handshake(&mut third_io, &owner_for_task, &features).await;
        let mut third_inbound = third.inbound_next;
        let request = third_io
            .recv_frame(&third.connection_id, &mut third_inbound)
            .await
            .expect("attachment.begin");
        assert!(matches!(request.body, FrameBody::Begin(_)));
        upload_started_tx.send(()).expect("upload started");
        stop_rx.await.expect("stop");
    });

    let link = open_link(&transport, &owner, [0x49; 32]);
    wait_for(|| link.is_online(), "link online").await;
    let transfer = AttachmentId::new("018f0000-0000-4000-8000-000000000301").expect("transfer");
    let first = node_link_client::AttachmentForwarder::connect(&link)
        .await
        .expect("attachment connection");
    let status = first.status(&transfer).await.expect("status reply");
    assert_eq!(
        status.state,
        acp_core::model::AttachmentTransferState::Receiving
    );
    first.close().await;

    let second = Arc::new(
        node_link_client::AttachmentForwarder::connect(&link)
            .await
            .expect("second attachment connection"),
    );
    let pending = tokio::spawn({
        let second = Arc::clone(&second);
        let transfer = transfer.clone();
        async move { second.status(&transfer).await }
    });
    second_status_rx.await.expect("second status request");

    // 上传已发出 `begin`，调用方正阻塞在本地源的 `poll_read`（`NeverReady`）。
    let third = Arc::new(
        node_link_client::AttachmentForwarder::connect(&link)
            .await
            .expect("third attachment connection"),
    );
    let upload = tokio::spawn({
        let third = Arc::clone(&third);
        let session = acp_core::model::SessionId::new("018f0000-0000-4000-8000-000000000302")
            .expect("session");
        async move {
            third
                .upload(&transfer, &session, 1024, "text/plain", "a.txt", NeverReady)
                .await
        }
    });
    upload_started_rx.await.expect("upload began");

    let started = std::time::Instant::now();
    tokio::time::timeout(Duration::from_secs(1), link.shutdown())
        .await
        .expect("shutdown must cancel in-flight attachment transfers");
    assert!(started.elapsed() < Duration::from_secs(1));
    let error = tokio::time::timeout(Duration::from_secs(1), pending)
        .await
        .expect("the in-flight status must resolve")
        .expect("join");
    assert!(
        matches!(error, Err(ClientError::TransferAborted)),
        "got {error:?}"
    );
    let upload = tokio::time::timeout(Duration::from_secs(1), upload)
        .await
        .expect("the in-flight upload must resolve")
        .expect("join");
    assert!(
        matches!(upload, Err(ClientError::TransferAborted)),
        "got {upload:?}"
    );
    stop_tx.send(()).expect("stop owner");
    owner_task.await.expect("owner task");
}

/// 附件握手不再无限等待：Owner 接受升级后不说话时，15 s（暂停时钟下即瞬时时）失败关闭。
#[tokio::test(start_paused = true)]
async fn an_attachment_handshake_that_never_answers_times_out() {
    let owner = OwnerIdentity::new([0x4a; 32], "5c6d7e8f-9012-4a3b-8c4d-5e6f708192c5");
    let transport = Arc::new(MemTransport::default());
    let (business, mut business_io) = OwnerIo::pair();
    let (attachment, mut attachment_io) = OwnerIo::pair();
    transport.push(business);
    transport.push(attachment);
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let owner_for_task = OwnerIdentity::new([0x4a; 32], "5c6d7e8f-9012-4a3b-8c4d-5e6f708192c5");
    let owner_task = tokio::spawn(async move {
        let session = accept_handshake(
            &mut business_io,
            &owner_for_task,
            &["node-link.core.v1", "node-link.attachment-transfer.v1"],
        )
        .await;
        let mut inbound = session.inbound_next;
        let _ = business_io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");
        // 读走 `node.hello` 但永不回复 challenge/proof/ready。
        let hello = attachment_io.recv().await.expect("node.hello");
        assert!(hello.contains("node.hello"));
        stop_rx.await.expect("stop");
    });

    let link = open_link(&transport, &owner, [0x4b; 32]);
    wait_for(|| link.is_online(), "link online").await;
    let error = node_link_client::AttachmentForwarder::connect(&link)
        .await
        .expect_err("an unanswered attachment handshake must time out");
    assert!(matches!(error, ClientError::Timeout), "got {error:?}");
    stop_tx.send(()).expect("stop owner");
    owner_task.await.expect("owner task");
    link.shutdown().await;
}

/// 传输按**每条 Owner 的 `ClientConfig`** 构造：信任方式与 `max_message_bytes` 不再被进程级共享的
/// 传输吞掉（修复前 `AccessManager` 只用 `Roots` 构造一个传输，`PlaintextDev`/上限从未生效）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_owner_link_builds_its_transport_from_its_own_config() {
    #[derive(Default)]
    struct RecordingFactory {
        seen: std::sync::Mutex<Vec<(String, usize)>>,
    }

    impl TransportFactory for RecordingFactory {
        fn build(
            &self,
            trust: &TlsTrust,
            max_message_bytes: usize,
        ) -> Result<Arc<dyn node_link_client::Transport>, ClientError> {
            self.seen
                .lock()
                .expect("seen")
                .push((format!("{trust:?}"), max_message_bytes));
            // 连接必然失败：本用例只断言构造参数，随后立即 shutdown。
            Ok(Arc::new(MemTransport::default()))
        }
    }

    let factory = Arc::new(RecordingFactory::default());
    let client = NodeLinkClient::new(
        factory.clone(),
        Arc::new(FixedIdentity::new([0x4c; 32])),
        Arc::new(RecordingSink::default()),
        Arc::new(RecordingDeliveries::default()),
    );
    let first_owner = OwnerIdentity::new([0x51; 32], "5c6d7e8f-9012-4a3b-8c4d-5e6f708192d1");
    let second_owner = OwnerIdentity::new([0x52; 32], "5c6d7e8f-9012-4a3b-8c4d-5e6f708192d2");

    let mut plaintext = ClientConfig::new(
        owner_trust(&first_owner),
        "ws://owner.test/node-link/v1",
        TlsTrust::PlaintextDev,
    );
    plaintext.max_message_bytes = 4096;
    let first = client.open(plaintext).expect("open plaintext config");

    let mut roots = ClientConfig::new(
        owner_trust(&second_owner),
        "wss://owner.test/node-link/v1",
        TlsTrust::Roots(vec![rustls_pki_types::CertificateDer::from(vec![0u8; 1])]),
    );
    roots.max_message_bytes = 8192;
    let second = client.open(roots).expect("open roots config");

    {
        let seen = factory.seen.lock().expect("seen");
        assert_eq!(seen.len(), 2, "每个 Owner 各构造一次传输");
        assert!(seen[0].0.contains("PlaintextDev"), "got {seen:?}");
        assert_eq!(seen[0].1, 4096, "每个 Owner 的单消息上限必须真正传入");
        assert!(seen[1].0.contains("Roots"), "got {seen:?}");
        assert_eq!(seen[1].1, 8192);
    }

    first.shutdown().await;
    second.shutdown().await;
}

/// 已撤销导出/旧 epoch cursor 的恢复 attach 被请求级拒绝后：连接保持可用，重连时恢复计划**跳过**
/// 该持久化订阅——不会形成「重连 → 再 attach → 再拒绝」的循环。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_permanently_rejected_subscription_is_not_retried_after_reconnect() {
    const IMPORTED_SESSION: &str = "2bc8b944-2a4f-46a7-8c31-b2c40923f60b";
    const OWNER_NODE: &str = "5c6d7e8f-9012-4a3b-8c4d-5e6f708192a5";
    let features = ["node-link.core.v1", "node-link.command-status.v1"];
    let owner = OwnerIdentity::new([0x33; 32], OWNER_NODE);
    let transport = Arc::new(MemTransport::default());
    let (first_connection, mut first_io) = OwnerIo::pair();
    let (second_connection, mut second_io) = OwnerIo::pair();
    transport.push(first_connection);
    transport.push(second_connection);
    let owner_for_task = OwnerIdentity::new([0x33; 32], OWNER_NODE);
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let owner_task = tokio::spawn(async move {
        let snapshot = || CatalogSnapshot {
            revision: decimal(1),
            agents: AgentEntryList::new(Vec::new()).expect("agents"),
            directories: DirectoryEntryList::new(Vec::new()).expect("directories"),
            sessions: CatalogSessionList::new(Vec::new()).expect("sessions"),
        };
        // 第一条连接：恢复计划里的 attach 得到请求级拒绝（不可重试）。
        let session = accept_handshake(&mut first_io, &owner_for_task, &features).await;
        let mut inbound = session.inbound_next;
        let _ = first_io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");
        first_io
            .send_envelope(
                MessageType::CatalogSnapshot,
                &session.connection_id,
                &snapshot(),
            )
            .await;
        let attach = first_io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("startup resource.attach");
        assert_eq!(attach.message_type(), MessageType::ResourceAttach);
        let mut body = node_link_protocol::error::Body::new(
            node_link_protocol::error::ErrorCode::ProtocolSequenceInvalid,
            "the cursor is not usable for this session on this connection",
            false,
        )
        .expect("error body");
        body.correlation_id = acpr_wire::Nullable::value(attach.message_id().clone());
        first_io
            .send_envelope(MessageType::LinkError, &session.connection_id, &body)
            .await;
        // 拒绝本身不得关闭连接；随后由 Owner 断开以触发一次重连。
        tokio::time::sleep(Duration::from_millis(100)).await;
        first_io.close().await;

        // 第二条连接：恢复计划必须跳过被拒绝的会话（除 catalog.subscribe 外没有请求）。
        let session = accept_handshake(&mut second_io, &owner_for_task, &features).await;
        let mut inbound = session.inbound_next;
        let _ = second_io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");
        second_io
            .send_envelope(
                MessageType::CatalogSnapshot,
                &session.connection_id,
                &snapshot(),
            )
            .await;
        let retried = tokio::time::timeout(
            Duration::from_millis(300),
            second_io.recv_envelope(&session.connection_id, &mut inbound),
        )
        .await;
        assert!(
            retried.is_err(),
            "a permanently rejected subscription must not be retried after reconnect"
        );
        stop_rx.await.expect("stop");
    });

    let at = Timestamp::new("2026-10-20T00:00:00.000Z").expect("timestamp");
    let epoch = OriginEpoch::new("018f0000-0000-4000-8000-0000000000ee").expect("epoch");
    let deliveries = Arc::new(RecordingDeliveries::default());
    deliveries
        .sessions
        .lock()
        .expect("sessions")
        .push(acp_core::ports::ImportedSessionRecord {
            session: acp_core::model::RemoteSessionRef::new(
                NodeId::new(OWNER_NODE).expect("owner"),
                SessionId::new(IMPORTED_SESSION).expect("session"),
            ),
            origin_epoch: Some(epoch.clone()),
            title: None,
            agent: None,
            state: None,
            version: None,
            created_at: None,
            last_origin_sequence: None,
            acked: Some(OriginCursor::new(
                epoch,
                Sequence::new(7).expect("sequence"),
            )),
            attachment: None,
            updated_at: at,
        });
    let client = NodeLinkClient::new(
        transport.factory(),
        Arc::new(FixedIdentity::new([0x43; 32])),
        Arc::new(RecordingSink::default()),
        deliveries,
    );
    let mut config = ClientConfig::new(
        owner_trust(&owner),
        "wss://owner.test/node-link/v1",
        TlsTrust::PlaintextDev,
    );
    config.reconnect = node_link_client::ReconnectPolicy {
        initial: Duration::from_millis(20),
        max: Duration::from_millis(20),
    };
    let link = client.open(config).expect("open");
    let wire = node_link_protocol::common::RemoteSessionRef {
        owner_node_id: uuid_text(OWNER_NODE),
        session_id: uuid_text(IMPORTED_SESSION),
    };
    wait_for(
        || link.shared().subscription_rejected(&wire),
        "the attach rejection to mark the subscription unavailable",
    )
    .await;
    assert!(
        link.is_online(),
        "a request-level rejection must keep the connection"
    );
    wait_for(|| transport.opens() >= 2, "the supervisor to reconnect").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        transport.opens(),
        2,
        "a rejected subscription must not cause a reconnect storm"
    );
    stop_tx.send(()).expect("stop owner");
    owner_task.await.expect("owner task");
    link.shutdown().await;
}
