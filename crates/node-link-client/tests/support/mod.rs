//! `node-link-client` 集成测试的脚本化 Owner 与传输。
//!
//! 这些测试不 mock 业务函数：客户端走**真实的**握手、信封、附件帧编解码与序号校验；只有网络字节
//! 承载是内存通道（[`MemTransport`]）。Owner 侧的挑战用真实 P-256 密钥签名，因此客户端的 Owner
//! proof 校验路径也被执行，而不是被跳过。
//!
//! 依赖纪律：本模块只用公开 API（`node-link-protocol`、`identity-auth`、本 crate），不触碰私有字段。

#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use acp_core::model::{
    LocalCursor, NodeId, OriginCursor, PeerPublicKey, RemoteSessionRef, RequestId, SessionId,
    Timestamp,
};
use acp_core::ports::{
    AckOutcome, DeliveryIndexEntry, DeliveryReceipt, ImportedSessionQuery, ImportedSessionRecord,
    PruneReport, ReceiptOutcome, RemoteCommandRef, RemoteDeliveryStore, ReplayLimit,
    RetentionPolicy,
};
use async_trait::async_trait;
use identity_auth::{ChallengeId, FeatureList, NodeLinkChallenge, P1363Signature};
use node_link_protocol::attachment::{Frame, FrameBody};
use node_link_protocol::common::{Base64Url, DecimalString, Timestamp as WireTimestamp, Uuid};
use node_link_protocol::envelope::{ConnectionFields, Envelope, MessageType};
use node_link_protocol::handshake::{NodeChallenge, NodeHello, NodeReady};
use p256::ecdsa::signature::Signer as _;
use tokio::sync::mpsc;

use node_link_client::TlsTrust;
use node_link_client::attachment::AttachmentForwarder;
use node_link_client::handshake::{Established, HandshakeConfig, OwnerTrust};
use node_link_client::identity::NodeIdentity;
use node_link_client::transport::{
    Connection, Incoming, MessageSink, MessageSource, Transport, TransportFactory,
};

use acp_core::model::Nonce;

/// 附件测试使用的固定 Owner 节点标识（真实握手的信任锚）。
pub const TEST_OWNER_NODE: &str = "5c6d7e8f-9012-4a3b-8c4d-5e6f708192c1";

/// Access 侧的固定 P-256 身份（附件测试走真实握手，因此需要签名者）。
pub struct AccessIdentity {
    node_id: NodeId,
    signing: p256::ecdsa::SigningKey,
}

impl AccessIdentity {
    /// 用固定标量构造。
    pub fn new(seed: [u8; 32], node_id: &str) -> Self {
        Self {
            node_id: NodeId::new(node_id).expect("测试 node id 合法"),
            signing: p256::ecdsa::SigningKey::from_slice(&seed).expect("测试私钥合法"),
        }
    }
}

#[async_trait]
impl NodeIdentity for AccessIdentity {
    fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    async fn public_key(&self) -> Result<PeerPublicKey, node_link_client::ClientError> {
        let encoded = self.signing.verifying_key().to_sec1_point(false);
        PeerPublicKey::try_from_bytes(encoded.as_bytes())
            .map_err(|_| node_link_client::ClientError::Local("test public key"))
    }

    async fn sign(
        &self,
        transcript: &[u8],
    ) -> Result<P1363Signature, node_link_client::ClientError> {
        use p256::ecdsa::signature::Signer as _;
        let signature: p256::ecdsa::Signature = self.signing.sign(transcript);
        P1363Signature::try_from_bytes(signature.to_bytes().as_slice())
            .map_err(|_| node_link_client::ClientError::Local("test signature"))
    }
}

/// 脚本化 Owner 的节点身份（固定私钥，测试可复跑）。
pub struct OwnerIdentity {
    /// Owner 节点标识。
    pub node_id: NodeId,
    /// 签名密钥（挑战签名）。
    signing: p256::ecdsa::SigningKey,
    /// SEC1 公钥（客户端信任锚）。
    pub public_key: PeerPublicKey,
}

impl OwnerIdentity {
    /// 用固定标量构造。
    pub fn new(seed: [u8; 32], node_id: &str) -> Self {
        let signing = p256::ecdsa::SigningKey::from_slice(&seed).expect("测试私钥合法");
        let encoded = signing.verifying_key().to_sec1_point(false);
        let public_key = PeerPublicKey::try_from_bytes(encoded.as_bytes()).expect("SEC1 公钥合法");
        Self {
            node_id: NodeId::new(node_id).expect("测试 node id 合法"),
            signing,
            public_key,
        }
    }

    /// 对已编码 transcript 签名（P-1363）。
    pub fn sign(&self, transcript: &[u8]) -> P1363Signature {
        let signature: p256::ecdsa::Signature = self.signing.sign(transcript);
        P1363Signature::try_from_bytes(signature.to_bytes().as_slice()).expect("签名合法")
    }
}

/// 客户端 `Connection` 的实现：sink/source 各自是一条内存通道。
struct MemConnection {
    sink: mpsc::Sender<String>,
    source: mpsc::Receiver<Incoming>,
}

impl Connection for MemConnection {
    fn split(self: Box<Self>) -> (Box<dyn MessageSink>, Box<dyn MessageSource>) {
        (
            Box::new(MemSink { tx: self.sink }),
            Box::new(MemSource { rx: self.source }),
        )
    }
}

struct MemSink {
    tx: mpsc::Sender<String>,
}

#[async_trait]
impl MessageSink for MemSink {
    async fn send(&mut self, text: String) -> Result<(), node_link_client::ClientError> {
        self.tx
            .send(text)
            .await
            .map_err(|_| node_link_client::ClientError::Closed)
    }

    async fn close(&mut self) -> Result<(), node_link_client::ClientError> {
        Ok(())
    }
}

struct MemSource {
    rx: mpsc::Receiver<Incoming>,
}

#[async_trait]
impl MessageSource for MemSource {
    async fn recv(&mut self) -> Result<Incoming, node_link_client::ClientError> {
        self.rx
            .recv()
            .await
            .ok_or(node_link_client::ClientError::Closed)
    }
}

/// 脚本化传输：`connect` 按顺序交出预置连接；用尽后返回连接失败（触发 supervisor 退避）。
#[derive(Default)]
pub struct MemTransport {
    pending: Mutex<VecDeque<Box<dyn Connection>>>,
    opens: AtomicUsize,
}

impl MemTransport {
    /// 供 [`node_link_client::NodeLinkClient`] 使用的工厂：每个 Owner 都拿到同一个脚本化通道
    /// （信任方式与单消息上限由真实 WSS 传输消费，脚本化通道不涉及）。
    pub fn factory(self: &Arc<Self>) -> Arc<dyn TransportFactory> {
        Arc::new(MemTransportFactory(Arc::clone(self)))
    }

    /// 追加一条连接。
    pub fn push(&self, connection: Box<dyn Connection>) {
        self.pending
            .lock()
            .expect("pending 锁")
            .push_back(connection);
    }

    /// 已发生的连接次数。
    pub fn opens(&self) -> usize {
        self.opens.load(Ordering::SeqCst)
    }
}

/// [`MemTransport`] 的工厂包装（脚本化连接不消费 TLS 信任）。
pub struct MemTransportFactory(Arc<MemTransport>);

impl TransportFactory for MemTransportFactory {
    fn build(
        &self,
        _trust: &TlsTrust,
        _max_message_bytes: usize,
    ) -> Result<Arc<dyn Transport>, node_link_client::ClientError> {
        Ok(self.0.clone())
    }
}

#[async_trait]
impl Transport for MemTransport {
    async fn connect(
        &self,
        _target: &node_link_client::ConnectionTarget,
    ) -> Result<Box<dyn Connection>, node_link_client::ClientError> {
        self.opens.fetch_add(1, Ordering::SeqCst);
        self.pending
            .lock()
            .expect("pending 锁")
            .pop_front()
            .ok_or_else(|| {
                node_link_client::ClientError::Transport("no scripted connection".to_owned())
            })
    }
}

/// Owner 侧的一条内存连接。
pub struct OwnerIo {
    sink: mpsc::Sender<Incoming>,
    source: mpsc::Receiver<String>,
    /// Owner 方向的出站序号（认证完成后从 1 开始，`node.ready` 占 1）。
    next_sequence: u64,
}

impl OwnerIo {
    /// 创建一对客户端连接与 Owner 端。
    pub fn pair() -> (Box<dyn Connection>, Self) {
        let (client_tx, owner_rx) = mpsc::channel(256);
        let (owner_tx, client_rx) = mpsc::channel(256);
        (
            Box::new(MemConnection {
                sink: client_tx,
                source: client_rx,
            }),
            Self {
                sink: owner_tx,
                source: owner_rx,
                next_sequence: 1,
            },
        )
    }

    /// 读取一条客户端消息；通道关闭返回 `None`。
    pub async fn recv(&mut self) -> Option<String> {
        self.source.recv().await
    }

    /// 发送一条原始文本。
    pub async fn send_raw(&mut self, text: String) {
        let _ = self.sink.send(Incoming::Text(text)).await;
    }

    /// 关闭（客户端读到 `Incoming::Closed`）。
    pub async fn close(&mut self) {
        let _ = self.sink.send(Incoming::Closed(None)).await;
    }

    /// 读取并解码一条业务信封，同时校验 `connectionId` 与入站序号。
    pub async fn recv_envelope(
        &mut self,
        connection_id: &Uuid,
        inbound: &mut u64,
    ) -> Option<Envelope> {
        let text = self.recv().await?;
        let envelope = Envelope::decode(&text).expect("信封合法");
        let connection = envelope.connection().expect("认证后信封带连接字段");
        assert_eq!(
            connection.connection_id.as_str(),
            connection_id.as_str(),
            "connectionId 必须一致"
        );
        assert_eq!(
            connection.connection_sequence.as_str(),
            inbound.to_string(),
            "入站序号必须严格加一"
        );
        *inbound += 1;
        Some(envelope)
    }

    /// 发送一条认证后业务信封（递增本方向序号）。
    pub async fn send_envelope<T: serde::Serialize>(
        &mut self,
        message_type: MessageType,
        connection_id: &Uuid,
        body: &T,
    ) {
        let text = encode_envelope(
            message_type,
            Some(ConnectionFields {
                connection_id: connection_id.clone(),
                connection_sequence: decimal(self.next_sequence),
            }),
            body,
        );
        self.next_sequence += 1;
        self.send_raw(text).await;
    }

    /// 读取一条附件帧，同时校验连接与入站序号。
    pub async fn recv_frame(&mut self, connection_id: &Uuid, inbound: &mut u64) -> Option<Frame> {
        let text = self.recv().await?;
        let frame = Frame::decode(&text).expect("附件帧合法");
        assert_eq!(frame.connection_id.as_str(), connection_id.as_str());
        assert_eq!(frame.connection_sequence.as_str(), inbound.to_string());
        *inbound += 1;
        Some(frame)
    }

    /// 发送一条附件帧（递增本方向序号）。
    pub async fn send_frame(&mut self, connection_id: &Uuid, body: FrameBody) -> u64 {
        let sequence = self.next_sequence;
        let frame = Frame::new(
            uuid_text("018f0000-0000-4000-8000-000000000001"),
            connection_id.clone(),
            decimal(sequence),
            body,
        );
        self.next_sequence += 1;
        self.send_raw(frame.encode().expect("附件帧可编码")).await;
        sequence
    }

    /// 直接写入附件帧（用于构造序号错误等非法输入）。
    pub async fn send_frame_raw(&mut self, connection_id: &Uuid, sequence: u64, body: FrameBody) {
        let frame = Frame::new(
            uuid_text("018f0000-0000-4000-8000-000000000002"),
            connection_id.clone(),
            decimal(sequence),
            body,
        );
        self.send_raw(frame.encode().expect("附件帧可编码")).await;
    }
}

/// 握手结果（Owner 侧）。
pub struct OwnerSession {
    /// 连接标识。
    pub connection_id: Uuid,
    /// 下一个入站序号（客户端方向）。
    pub inbound_next: u64,
}

/// 完成一次 Owner 侧握手：读 `node.hello` → 签挑战 → 读 `node.proof` → 发 `node.ready`。
pub async fn accept_handshake(
    io: &mut OwnerIo,
    owner: &OwnerIdentity,
    features: &[&str],
) -> OwnerSession {
    let hello_text = io.recv().await.expect("node.hello");
    let hello = Envelope::decode(&hello_text).expect("hello 信封");
    assert_eq!(hello.message_type(), MessageType::NodeHello);
    let hello_body: NodeHello = serde_json::from_str(hello.body().get()).expect("hello body");

    let mut selected: Vec<String> = features
        .iter()
        .map(|feature| (*feature).to_owned())
        .filter(|feature| {
            hello_body
                .supported_features
                .as_slice()
                .iter()
                .any(|declared| declared.as_str() == feature)
        })
        .collect();
    selected.sort();

    let connection_id = uuid_text("018f0000-0000-4000-8000-000000000010");
    let catalog_revision = 0u64;
    let client_nonce_text = base64url(hello_body.client_nonce.as_bytes());
    let server_nonce_text = base64url(&[7u8; 32]);
    let challenge = NodeLinkChallenge {
        owner_node_id: owner.node_id.clone(),
        access_node_id: NodeId::new(hello_body.access_node_id.as_str()).expect("access 节点 id"),
        catalog_revision,
        client_nonce: Nonce::new(&client_nonce_text).expect("client nonce"),
        server_nonce: Nonce::new(&server_nonce_text).expect("server nonce"),
        connection_id: ChallengeId::parse(connection_id.as_str()).expect("challenge id"),
        negotiated_features: FeatureList::new(selected.iter().cloned()),
    };
    let signature = owner.sign(&challenge.transcript().expect("challenge transcript"));
    let body = NodeChallenge {
        selected_protocol_version: node_link_protocol::common::ProtocolVersionV1::new(1)
            .expect("version"),
        connection_id: connection_id.clone(),
        owner_node_id: uuid_text(owner.node_id.as_str()),
        server_nonce: Base64Url::<32>::parse(&server_nonce_text).expect("server nonce"),
        selected_features: {
            let ids = selected
                .iter()
                .map(|feature| {
                    node_link_protocol::common::FeatureId::parse(feature).expect("feature")
                })
                .collect::<Vec<_>>();
            node_link_protocol::common::FeatureList::new(ids).expect("feature list")
        },
        catalog_revision: decimal(catalog_revision),
        node_proof: Base64Url::<64>::parse(&signature.to_base64url()).expect("proof"),
    };
    io.send_raw(encode_envelope(MessageType::NodeChallenge, None, &body))
        .await;

    let proof_text = match tokio::time::timeout(std::time::Duration::from_secs(1), io.recv()).await
    {
        Ok(Some(text)) => text,
        Ok(None) => panic!("the client closed before node.proof"),
        Err(_) => panic!("the client sent no node.proof (challenge rejected?)"),
    };
    let proof = Envelope::decode(&proof_text).expect("proof 信封");
    assert_eq!(proof.message_type(), MessageType::NodeProof);

    io.next_sequence = 1;
    let ready = NodeReady {
        owner_node_id: uuid_text(owner.node_id.as_str()),
        catalog_revision: decimal(catalog_revision),
        limits: limits(),
        server_epoch: uuid_text("018f0000-0000-4000-8000-000000000020"),
    };
    io.send_envelope(MessageType::NodeReady, &connection_id, &ready)
        .await;
    OwnerSession {
        connection_id,
        inbound_next: 1,
    }
}

/// 协议默认限额（合法的最小构造）。
pub fn limits() -> node_link_protocol::common::NodeLinkLimits {
    use node_link_protocol::common::{BoundedU64, UIntAtLeast};
    node_link_protocol::common::NodeLinkLimits {
        max_message_bytes: UIntAtLeast::new(1_048_576).expect("limit"),
        catalog_snapshot_batch_size: UIntAtLeast::new(500).expect("limit"),
        resource_snapshot_batch_size: UIntAtLeast::new(500).expect("limit"),
        max_in_flight_commands: UIntAtLeast::new(32).expect("limit"),
        max_pending_queue_bytes: UIntAtLeast::new(8_388_608).expect("limit"),
        max_pending_queue_messages: UIntAtLeast::new(2_000).expect("limit"),
        heartbeat_interval_ms: BoundedU64::new(30_000).expect("limit"),
    }
}

/// 构造客户端的 `Established`（仅用于不需要握手的形状断言）。
pub fn established(connection_id: &str, features: &[&str]) -> Established {
    Established {
        connection_id: uuid_text(connection_id),
        next_sequence: 1,
        inbound_next: 2,
        selected_features: features
            .iter()
            .map(|feature| (*feature).to_owned())
            .collect(),
        limits: limits(),
        server_epoch: uuid_text("018f0000-0000-4000-8000-000000000020"),
        catalog_revision: decimal(0),
    }
}

/// 建立一对客户端/Owner 端并装配转发句柄。
///
/// **走真实握手**（Owner 侧用真实 P-256 挑战签名）：这样 `Established.inbound_next` 与 Owner 的
/// 出站序号都来自握手事实（`node.ready` 占 1，第一条附件回复是 2），而不是测试里另起的计数器。
pub async fn forwarder_pair(subject: &str) -> (AttachmentForwarder, OwnerIo, Uuid) {
    let (client, mut owner) = OwnerIo::pair();
    let (mut sink, mut source) = client.split();
    let owner_identity = OwnerIdentity::new([0x71; 32], TEST_OWNER_NODE);
    let trust = OwnerTrust {
        node_id: owner_identity.node_id.clone(),
        public_key: owner_identity.public_key.clone(),
    };
    let config = HandshakeConfig {
        identity: std::sync::Arc::new(AccessIdentity::new([0x61; 32], subject)),
        owner: trust,
        supported_features: vec![
            node_link_client::CORE_FEATURE.to_owned(),
            node_link_client::ATTACHMENT_FEATURE.to_owned(),
        ],
        required_features: vec![node_link_client::CORE_FEATURE.to_owned()],
    };
    let client_task = tokio::spawn(async move {
        let established =
            node_link_client::handshake::handshake(&mut *sink, &mut *source, &config).await?;
        Ok::<_, node_link_client::ClientError>((sink, source, established))
    });
    let session = accept_handshake(
        &mut owner,
        &owner_identity,
        &[
            node_link_client::CORE_FEATURE,
            node_link_client::ATTACHMENT_FEATURE,
        ],
    )
    .await;
    let (sink, source, established) = client_task
        .await
        .expect("handshake task")
        .expect("the client handshake must succeed");
    assert_eq!(
        established.connection_id.as_str(),
        session.connection_id.as_str()
    );
    assert_eq!(established.inbound_next, 2, "node.ready 已消费序号 1");
    assert_eq!(owner.next_sequence, 2, "Owner 的第一条回复是序号 2");
    let forwarder = AttachmentForwarder::from_parts(
        sink,
        source,
        established,
        NodeId::new(subject).expect("subject"),
    );
    (forwarder, owner, session.connection_id)
}

/// 文本编码辅助。
pub fn encode_envelope<T: serde::Serialize>(
    message_type: MessageType,
    connection: Option<ConnectionFields>,
    body: &T,
) -> String {
    let raw = serde_json::value::RawValue::from_string(
        serde_json::to_string(body).expect("body 可序列化"),
    )
    .expect("raw value");
    Envelope::new(
        message_type,
        uuid_text("018f0000-0000-4000-8000-0000000000aa"),
        connection,
        raw,
    )
    .expect("信封合法")
    .encode()
    .expect("信封可编码")
}

/// UUID 文本 → wire uuid。
pub fn uuid_text(text: &str) -> Uuid {
    Uuid::parse(text).expect("uuid 合法")
}

/// 十进制字符串。
pub fn decimal(value: u64) -> DecimalString {
    DecimalString::parse(&value.to_string()).expect("十进制字符串")
}

/// 无填充 base64url。
pub fn base64url(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// wire 时间戳。
pub fn wire_timestamp(text: &str) -> WireTimestamp {
    WireTimestamp::parse(text).expect("时间戳合法")
}

/// 记录调用的 `ImportedSink`（事件收据测试）。
#[derive(Default)]
pub struct RecordingSink {
    pub events: Mutex<Vec<(NodeId, String)>>,
    pub catalogs: Mutex<usize>,
    pub metas: Mutex<usize>,
    pub cursor: Mutex<Option<node_link_protocol::common::OriginCursor>>,
}

#[async_trait]
impl node_link_client::link::ImportedSink for RecordingSink {
    async fn event(
        &self,
        owner: &NodeId,
        event: &node_link_protocol::resource::Event,
    ) -> Result<Option<node_link_protocol::common::OriginCursor>, acp_core::model::PortError> {
        self.events
            .lock()
            .expect("events")
            .push((owner.clone(), event.origin_event_id.as_str().to_owned()));
        Ok(self.cursor.lock().expect("cursor").clone())
    }

    async fn catalog(
        &self,
        _owner: &NodeId,
        _snapshot: &node_link_protocol::catalog::CatalogSnapshot,
    ) -> Result<(), acp_core::model::PortError> {
        *self.catalogs.lock().expect("catalogs") += 1;
        Ok(())
    }

    async fn session_meta(
        &self,
        _owner: &NodeId,
        _session: &node_link_protocol::common::RemoteSessionRef,
        _meta: &node_link_protocol::common::SessionMeta,
    ) -> Result<(), acp_core::model::PortError> {
        *self.metas.lock().expect("metas") += 1;
        Ok(())
    }
}

/// 最小 `RemoteDeliveryStore`：只记录 ACK/收据调用，其余返回空（`sessions` 可按需预置）。
#[derive(Default)]
pub struct RecordingDeliveries {
    pub acks: Mutex<Vec<OriginCursor>>,
    pub receipts: Mutex<Vec<DeliveryReceipt>>,
    /// imported 索引行（supervisor 的重连恢复计划从这里读）。
    pub sessions: Mutex<Vec<ImportedSessionRecord>>,
}

#[async_trait]
impl RemoteDeliveryStore for RecordingDeliveries {
    async fn commit_receipt(
        &self,
        receipt: DeliveryReceipt,
    ) -> Result<ReceiptOutcome, acp_core::model::PortError> {
        self.receipts.lock().expect("receipts").push(receipt);
        Ok(ReceiptOutcome {
            local_sequence: LocalCursor::from(1),
            duplicate: false,
        })
    }

    async fn local_replay(
        &self,
        _after: Option<LocalCursor>,
        _limit: ReplayLimit,
    ) -> Result<Vec<DeliveryIndexEntry>, acp_core::model::PortError> {
        Ok(Vec::new())
    }

    async fn ack(
        &self,
        _session: &RemoteSessionRef,
        cursor: OriginCursor,
        _at: Timestamp,
    ) -> Result<AckOutcome, acp_core::model::PortError> {
        self.acks.lock().expect("acks").push(cursor);
        Ok(AckOutcome {
            applied: true,
            cursor: None,
        })
    }

    async fn load_ack(
        &self,
        _session: &RemoteSessionRef,
    ) -> Result<Option<OriginCursor>, acp_core::model::PortError> {
        Ok(None)
    }

    async fn upsert_session(
        &self,
        _record: ImportedSessionRecord,
        _at: Timestamp,
    ) -> Result<(), acp_core::model::PortError> {
        Ok(())
    }

    async fn list_sessions(
        &self,
        query: ImportedSessionQuery,
    ) -> Result<Vec<ImportedSessionRecord>, acp_core::model::PortError> {
        let rows = self.sessions.lock().expect("sessions").clone();
        Ok(match query.only {
            Some(only) => rows
                .into_iter()
                .filter(|row| only.contains(&row.session))
                .collect(),
            None => rows,
        })
    }

    async fn find_imported_session(
        &self,
        _session: &SessionId,
    ) -> Result<Option<ImportedSessionRecord>, acp_core::model::PortError> {
        Ok(None)
    }

    async fn find_remote_request(
        &self,
        _session: &RemoteSessionRef,
        _request: &RequestId,
    ) -> Result<Option<RemoteCommandRef>, acp_core::model::PortError> {
        Ok(None)
    }

    async fn prune(
        &self,
        _policy: RetentionPolicy,
        _at: Timestamp,
    ) -> Result<PruneReport, acp_core::model::PortError> {
        Ok(PruneReport::default())
    }
}
