//! 受控路径全链路集成测试里的 **Access Node** 与它的 wire 客户端。
//!
//! 这里是测试本地的「脚本化 fake Access 客户端」：它按 `docs/NODE_LINK_PROTOCOL.md` 的 wire 规则构造
//! 消息（字段名来自 `node-link-protocol` 的类型与 schema，不手抄一份），经**真实 loopback listener**
//! 完成配对、握手与业务消息，并且与 Owner 共享同一个 `public_origin` host（Host 边界因此真正生效）。
//!
//! 边界（与 `node-link-client` 缺席的关系）：本切片没有出站客户端实现，因此这份客户端只服务测试；
//! 它不做重连、不做游标持久化，只提供用例需要的「构造/解析/断言」三种能力。

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use acp_core::model::{NodeId, NodeKind, Nonce, PairingId, PeerPublicKey, Timestamp};
use base64::Engine as _;
use identity_auth::{
    FeatureList, NodeLinkChallenge, NodeLinkPairingProof, NodeLinkPairingStatus, NodeLinkProof,
    P1363Signature, PairingRequestId, PairingSecret,
};
use node_link_protocol::common::Base64Url;
use node_link_protocol::pairing::{AccessNodeKind, ClaimRequest, Endpoint, QrPayload};
use p256::ecdsa::SigningKey;
use p256::ecdsa::signature::Signer as _;
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::TcpStream;

/// 测试用的 canonical public origin host（`daemon.public_origin` 的 host；Host 边界按它判定）。
pub const PUBLIC_HOST: &str = "acpr-test.example.invalid";

/// 单次 I/O 的超时：用例失败要快速失败，不挂住测试进程。
pub const IO_TIMEOUT: Duration = Duration::from_secs(10);

/// 等一条消息的超时（给命令终态观察循环（250ms 轮询）与事件扇出留足余量）。
pub const MESSAGE_TIMEOUT: Duration = Duration::from_secs(15);

/// Access Node 的长期密钥（固定私钥：用例不依赖随机源，也不产生需要清理的密钥材料）。
pub struct AccessKey {
    node_id: String,
    signing: SigningKey,
}

impl AccessKey {
    /// 按 node id 派生固定私钥（同一 node id 得到同一密钥，不同 node id 得到不同密钥）。
    pub fn new(node_id: &str) -> Self {
        let mut scalar = [0u8; 32];
        // 标量取 `1..=8`（非零、远小于曲线阶）：测试只需要「不同节点不同密钥」。
        scalar[31] = 1 + (node_id.as_bytes()[0] % 8);
        Self {
            node_id: node_id.to_owned(),
            signing: SigningKey::from_slice(&scalar).expect("固定标量是合法私钥"),
        }
    }

    /// 节点标识的文本形式。
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// 65 字节 SEC1 未压缩公钥（配对与验签用）。
    pub fn public_key(&self) -> PeerPublicKey {
        let point = self.signing.verifying_key().to_encoded_point(false);
        PeerPublicKey::try_from_bytes(point.as_bytes()).expect("65 字节 SEC1 公钥")
    }

    /// wire 上的公钥文本（无填充 base64url）。
    pub fn wire_public_key(&self) -> Base64Url<65> {
        Base64Url::parse(&base64url(self.public_key().as_bytes())).expect("65 字节公钥的 base64url")
    }

    /// 对一段 transcript 的 P1363 签名（无填充 base64url）。
    pub fn sign(&self, transcript: &[u8]) -> String {
        let signature: p256::ecdsa::Signature = self.signing.sign(transcript);
        base64url(signature.to_bytes().as_slice())
    }
}

/// 无填充 base64url 编码。
pub fn base64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// 32 字节 nonce 的规范文本（末字节低 2 位清零，与 `nonce` 的 wire 约束一致）。
pub fn nonce_text(seed: u8) -> String {
    let mut bytes = [seed; 32];
    bytes[31] &= 0b1111_1100;
    base64url(&bytes)
}

/// 16 字节 nonce（`link.ping` 的 `nonce` 是固定 16 字节，与配对握手的 32 字节不同）。
pub fn nonce_text_16(seed: u8) -> String {
    base64url(&[seed; 16])
}

/// 规范 UUID 文本（`Uuid::new_v4` 的输出必然合规）。
pub fn uuid_text() -> String {
    uuid::Uuid::new_v4().hyphenated().to_string()
}

// ---------------------------------------------------------------------------------------------
// HTTP（配对 claim/status）
// ---------------------------------------------------------------------------------------------

/// 一次 HTTP 响应。
#[derive(Debug, Clone)]
pub struct HttpReply {
    /// 状态码。
    pub status: u16,
    /// 响应头（小写名）。
    pub headers: Vec<(String, String)>,
    /// 响应体原文。
    pub body: Vec<u8>,
}

impl HttpReply {
    /// 响应体的 JSON（不是 JSON 即 panic，含原文便于定位）。
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|error| {
            panic!(
                "响应体不是 JSON（{error}）：{}",
                String::from_utf8_lossy(&self.body)
            )
        })
    }

    /// 按名字取响应头（大小写不敏感）。
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// 置 `ACPR_NODELINK_TRACE=1` 时把双向报文打到 stderr（排查全链路失败用）。
fn trace_enabled() -> bool {
    std::env::var_os("ACPR_NODELINK_TRACE").is_some()
}

/// 向监听地址发一条 `POST`（明文；Host 用 `PUBLIC_HOST`，与 `daemon.public_origin` 同源）。
pub async fn post_json(addr: SocketAddr, path: &str, body: &[u8]) -> HttpReply {
    let stream = tokio::time::timeout(IO_TIMEOUT, TcpStream::connect(addr))
        .await
        .expect("连接超时")
        .expect("连接 loopback");
    send_post(Box::new(stream), path, body).await
}

/// 同上，但走 TLS（`tls.mode = "direct"` 的轮次：配对 HTTP 也没有明文面）。
pub async fn post_json_tls(
    addr: SocketAddr,
    path: &str,
    body: &[u8],
    config: Arc<rustls::ClientConfig>,
) -> HttpReply {
    let stream = tokio::time::timeout(IO_TIMEOUT, TcpStream::connect(addr))
        .await
        .expect("连接超时")
        .expect("连接 loopback");
    let connector = tokio_rustls::TlsConnector::from(config);
    let server_name = rustls_pki_types::ServerName::try_from(PUBLIC_HOST)
        .expect("合法 SNI")
        .to_owned();
    let stream = tokio::time::timeout(IO_TIMEOUT, connector.connect(server_name, stream))
        .await
        .expect("TLS 握手超时")
        .expect("TLS 握手");
    send_post(Box::new(stream), path, body).await
}

/// 发一条 HTTP/1.1 请求并读回响应（明文或 TLS 流）。
async fn send_post(mut stream: Box<dyn Stream>, path: &str, body: &[u8]) -> HttpReply {
    let mut request =
        format!("POST {path} HTTP/1.1\r\nHost: {PUBLIC_HOST}\r\nConnection: close\r\n")
            .into_bytes();
    request.extend_from_slice(b"Content-Type: application/json\r\n");
    request.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
    request.extend_from_slice(body);
    tokio::time::timeout(IO_TIMEOUT, stream.write_all(&request))
        .await
        .expect("写请求超时")
        .expect("写请求");

    let mut raw = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    loop {
        if let Some(head_end) = find_subsequence(&raw, b"\r\n\r\n") {
            if raw.len() >= head_end + 4 + content_length(&raw[..head_end]) {
                break;
            }
        }
        let read = tokio::time::timeout(IO_TIMEOUT, stream.read(&mut chunk))
            .await
            .expect("读响应超时")
            .expect("读响应");
        if read == 0 {
            break;
        }
        raw.extend_from_slice(&chunk[..read]);
    }
    parse_reply(&raw)
}

fn parse_reply(raw: &[u8]) -> HttpReply {
    let head_end = find_subsequence(raw, b"\r\n\r\n")
        .unwrap_or_else(|| panic!("响应缺少 header 结束标记：{}", String::from_utf8_lossy(raw)));
    let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or_else(|| panic!("状态行缺少状态码：{status_line}"));
    let headers = lines
        .filter_map(|line| {
            line.split_once(':')
                .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        })
        .collect::<Vec<_>>();
    let length = content_length(&raw[..head_end]);
    HttpReply {
        status,
        headers,
        body: raw
            .get(head_end + 4..head_end + 4 + length)
            .unwrap_or_default()
            .to_vec(),
    }
}

fn content_length(head: &[u8]) -> usize {
    let text = String::from_utf8_lossy(head);
    text.split("\r\n")
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0)
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

// ---------------------------------------------------------------------------------------------
// 配对：claim 与 status
// ---------------------------------------------------------------------------------------------

/// `node.pair.begin` 的二维码解出的公开字段（Access 侧配对所需的全部输入）。
#[derive(Debug, Clone)]
pub struct PairingTicket {
    /// 配对 id。
    pub pairing_id: String,
    /// 二维码里的**一次性** secret（claim/status 证明的 HMAC 密钥）。
    pub secret: PairingSecret,
    /// Owner 在该配对里宣告的 endpoint（claim 必须原样回传）。
    pub endpoint: String,
    /// 过期时间（规范时间戳文本）。
    pub expires_at: String,
    /// Owner 公钥（验 `ownerProof` 与握手里的 `nodeProof`）。
    pub owner_public_key: PeerPublicKey,
    /// Owner 节点 id。
    pub owner_node_id: String,
}

/// 从 `pairingUrl` 的 fragment（`#data=<无填充 base64url(json)>`）解出二维码 payload。
pub fn ticket_from_url(url: &str, pairing_id: &str) -> PairingTicket {
    let data = url
        .split("#data=")
        .nth(1)
        .expect("pairingUrl 必须带 #data= fragment");
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(data)
        .expect("fragment 是无填充 base64url");
    let payload: QrPayload = serde_json::from_slice(&raw).expect("二维码 payload 形状");
    let owner_public_key = PeerPublicKey::try_from_bytes(payload.owner_public_key.as_bytes())
        .expect("ownerPublicKey 是 65 字节 SEC1 公钥");
    PairingTicket {
        pairing_id: pairing_id.to_owned(),
        secret: PairingSecret::try_from_bytes(payload.pairing_secret.as_bytes())
            .expect("pairingSecret 是 32 字节"),
        endpoint: payload.endpoint.as_str().to_owned(),
        expires_at: payload.expires_at.as_str().to_owned(),
        owner_public_key,
        owner_node_id: payload.owner_node_id.as_str().to_owned(),
    }
}

/// 构造一次 claim 请求（proof = `HMAC(pairingSecret, node-link-pairing-proof/v1)`）。
pub fn claim_request(
    ticket: &PairingTicket,
    access: &AccessKey,
    client_nonce: &str,
) -> ClaimRequest {
    let access_node_id = node(access.node_id());
    let client_nonce = Nonce::new(client_nonce).expect("规范 nonce");
    let public_key = access.public_key();
    let proof = NodeLinkPairingProof {
        owner_node_id: node(&ticket.owner_node_id),
        access_node_id: access_node_id.clone(),
        pairing_id: pairing(&ticket.pairing_id),
        pairing_expires_at: Timestamp::new(&ticket.expires_at).expect("规范时间戳"),
        access_public_key: public_key.clone(),
        client_nonce: client_nonce.clone(),
        node_name: "Integration Access".to_owned(),
        node_kind: NodeKind::Access,
    }
    .hmac(&ticket.secret)
    .expect("认领证明可计算");
    ClaimRequest {
        protocol_version: ProtocolVersionV1::new(1).expect("v1"),
        pairing_id: uuid(&ticket.pairing_id),
        owner_node_id: uuid(&ticket.owner_node_id),
        access_node_id: uuid(access.node_id()),
        node_name: node_link_protocol::common::NonEmptyText::parse("Integration Access")
            .expect("节点名合法"),
        node_kind: AccessNodeKind,
        endpoint: Endpoint::parse(&ticket.endpoint).expect("endpoint 合法"),
        access_public_key: access.wire_public_key(),
        client_nonce: Base64Url::<32>::parse(client_nonce.as_str()).expect("32 字节 nonce"),
        proof: Base64Url::parse(&base64url(proof.as_bytes())).expect("32 字节 HMAC"),
    }
}

/// 验证 claim 响应里的 `ownerProof`（Access 侧必须能自证 Owner 身份）。
pub fn verify_owner_proof(
    ticket: &PairingTicket,
    body: &Value,
    access: &AccessKey,
    client_nonce: &str,
) {
    let signature = P1363Signature::try_from_base64url(
        body["ownerProof"].as_str().expect("ownerProof 是字符串"),
    )
    .expect("ownerProof 是 64 字节 P1363");
    let input = identity_auth::NodeLinkPairingOwnerProof {
        owner_node_id: node(&ticket.owner_node_id),
        access_node_id: node(access.node_id()),
        pairing_id: pairing(&ticket.pairing_id),
        owner_public_key: ticket.owner_public_key.clone(),
        access_public_key: access.public_key(),
        client_nonce: Nonce::new(client_nonce).expect("规范 nonce"),
        server_nonce: Nonce::new(body["serverNonce"].as_str().expect("serverNonce"))
            .expect("serverNonce 是规范 nonce"),
        pairing_request_id: PairingRequestId::parse(
            body["pairingRequestId"].as_str().expect("pairingRequestId"),
        )
        .expect("pairingRequestId 是规范 UUID"),
    };
    input
        .verify(&ticket.owner_public_key, &signature)
        .expect("ownerProof 必须由二维码里的 ownerPublicKey 验证通过");
}

/// 构造一次 status 请求（proof = `HMAC(pairingSecret, node-link-pairing-status/v1)`）。
pub fn status_request(
    ticket: &PairingTicket,
    access: &AccessKey,
    pairing_request_id: &str,
    request_nonce: &str,
) -> Value {
    let input = NodeLinkPairingStatus {
        owner_node_id: node(&ticket.owner_node_id),
        access_node_id: node(access.node_id()),
        pairing_id: pairing(&ticket.pairing_id),
        pairing_request_id: PairingRequestId::parse(pairing_request_id)
            .expect("pairingRequestId 是规范 UUID"),
        request_nonce: Nonce::new(request_nonce).expect("规范 nonce"),
    };
    let proof = input.hmac(&ticket.secret).expect("status 证明可计算");
    json!({
        "protocolVersion": 1,
        "ownerNodeId": ticket.owner_node_id,
        "accessNodeId": access.node_id(),
        "pairingId": ticket.pairing_id,
        "pairingRequestId": pairing_request_id,
        "requestNonce": request_nonce,
        "proof": base64url(proof.as_bytes()),
    })
}

fn uuid(text: &str) -> node_link_protocol::common::Uuid {
    node_link_protocol::common::Uuid::parse(text).expect("测试用 UUID 文本必须规范")
}

fn node(text: &str) -> NodeId {
    NodeId::new(text).expect("测试用 NodeId 文本必须规范")
}

fn pairing(text: &str) -> PairingId {
    PairingId::new(text).expect("测试用 PairingId 文本必须规范")
}

use node_link_protocol::common::ProtocolVersionV1;

// ---------------------------------------------------------------------------------------------
// WebSocket 客户端（明文或 TLS）
// ---------------------------------------------------------------------------------------------

/// 一条可以被明文或 TLS 包装的字节流。
pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<S: AsyncRead + AsyncWrite + Unpin + Send> Stream for S {}

/// 一个 WebSocket 帧。
#[derive(Debug)]
enum Frame {
    Text(String),
    Close(u16, String),
    Other,
}

/// Node Link 的 Access 侧连接（WebSocket 之上的 MessageType 层）。
pub struct NodeLinkClient {
    stream: Box<dyn Stream>,
    buffer: Vec<u8>,
    connection_id: Option<String>,
    outbound: u64,
    server_sequence: u64,
    pending: Vec<Value>,
    /// 本方向最近一次发出的 `connectionSequence`（失败信息用）。
    last_sent: Option<String>,
    /// 当前步骤名（失败信息用）。
    step: String,
    /// 连接建立时刻（`ACPR_NODELINK_TRACE` 的时间戳）。
    opened_at: std::time::Instant,
}

impl NodeLinkClient {
    /// 明文连接并完成 WebSocket 升级（proxy 模式）。
    pub async fn connect_plain(addr: SocketAddr) -> Self {
        let stream = TcpStream::connect(addr).await.expect("连接 loopback");
        Self::upgrade(Box::new(stream)).await
    }

    /// TLS 连接（`direct` 模式）并完成 WebSocket 升级。
    pub async fn connect_tls(addr: SocketAddr, config: Arc<rustls::ClientConfig>) -> Self {
        let stream = tokio::time::timeout(IO_TIMEOUT, TcpStream::connect(addr))
            .await
            .expect("连接超时")
            .expect("连接 loopback");
        let connector = tokio_rustls::TlsConnector::from(config);
        let server_name = rustls_pki_types::ServerName::try_from(PUBLIC_HOST)
            .expect("合法 SNI")
            .to_owned();
        let stream = tokio::time::timeout(IO_TIMEOUT, connector.connect(server_name, stream))
            .await
            .expect("TLS 握手超时")
            .expect("TLS 握手");
        Self::upgrade(Box::new(stream)).await
    }

    async fn upgrade(mut stream: Box<dyn Stream>) -> Self {
        let key = base64::engine::general_purpose::STANDARD.encode([0x2au8; 16]);
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {PUBLIC_HOST}\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Protocol: {}\r\n\r\n",
            server::node_link::WS_PATH,
            server::node_link::WS_SUBPROTOCOL,
        );
        tokio::time::timeout(IO_TIMEOUT, stream.write_all(request.as_bytes()))
            .await
            .expect("写升级请求超时")
            .expect("写升级请求");
        let mut buffer = Vec::new();
        let head_end = loop {
            if let Some(head_end) = find_subsequence(&buffer, b"\r\n\r\n") {
                break head_end;
            }
            let mut chunk = [0u8; 1024];
            let read = tokio::time::timeout(IO_TIMEOUT, stream.read(&mut chunk))
                .await
                .expect("读升级响应超时")
                .expect("读升级响应");
            assert!(read > 0, "升级响应未读全就被关闭");
            buffer.extend_from_slice(&chunk[..read]);
        };
        let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
        assert!(
            head.starts_with("HTTP/1.1 101 "),
            "WebSocket 升级必须成功，实际：{head}"
        );
        buffer.drain(..head_end + 4);
        Self {
            stream,
            buffer,
            connection_id: None,
            outbound: 0,
            server_sequence: 0,
            pending: Vec::new(),
            last_sent: None,
            step: String::new(),
            opened_at: std::time::Instant::now(),
        }
    }

    /// 已到达但当前步骤尚未取用的消息（负向断言用：证明「某类消息没有出现」）。
    pub fn pending(&self) -> &[Value] {
        &self.pending
    }

    /// 标记当前步骤（失败信息里带上它，便于定位握手/业务阶段）。
    pub fn step(&mut self, label: &str) {
        self.step = label.to_owned();
    }

    /// 发送一条文本帧。
    async fn send_text(&mut self, text: &str) {
        let payload = text.as_bytes();
        let mut frame = Vec::with_capacity(payload.len() + 14);
        frame.push(0x81);
        if payload.len() < 126 {
            frame.push(0x80 | payload.len() as u8);
        } else {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        }
        let mask = [0x21u8, 0x43, 0x65, 0x87];
        frame.extend_from_slice(&mask);
        frame.extend(
            payload
                .iter()
                .enumerate()
                .map(|(index, byte)| byte ^ mask[index % 4]),
        );
        tokio::time::timeout(IO_TIMEOUT, self.stream.write_all(&frame))
            .await
            .expect("写帧超时")
            .expect("写帧");
    }

    /// 认证前消息（信封不带 `connectionId`/`connectionSequence`）。
    pub async fn send_pre_auth(&mut self, message_type: &str, body: Value) {
        let text = json!({
            "protocolVersion": 1,
            "type": message_type,
            "messageId": uuid_text(),
            "body": body,
        })
        .to_string();
        self.send_text(&text).await;
    }

    /// 认证后消息（信封带 `connectionId` 与「下一个应到」的 `connectionSequence`）。
    ///
    /// §2.2 的序号属于「对端发出的消息」：服务端每条合法序号的消息都让它的期望值前进（哪怕该消息
    /// 随后因 type/body 被拒），因此本方向的序号就是「已发条数 + 1」，与响应是否被受理无关。
    pub async fn send(&mut self, message_type: &str, body: Value) {
        let connection_id = self
            .connection_id
            .clone()
            .expect("认证后消息必须已有 connectionId");
        self.outbound += 1;
        let sequence = self.outbound.to_string();
        self.last_sent = Some(sequence.clone());
        let text = json!({
            "protocolVersion": 1,
            "type": message_type,
            "messageId": uuid_text(),
            "connectionId": connection_id,
            "connectionSequence": sequence,
            "body": body,
        })
        .to_string();
        if trace_enabled() {
            eprintln!("[access] -> {text}");
        }
        self.send_text(&text).await;
    }

    /// 取下一条消息（先看待取队列，再读 wire）。
    ///
    /// 「序号严格加一」与 `connectionId` 的核对发生在**从 wire 解析时**（见 [`Self::next_message`]）：
    /// 从待取队列取回的消息已经被核对过，重复核对会把它当成第二次到达。
    pub async fn take(&mut self) -> Value {
        self.next_message().await
    }

    /// 读一段固定时长（`take` 的全部消息收进一个列表，超时返回已收到的部分）。
    ///
    /// 负向断言用：「此后一段时间内没有出现 X」要么用待取队列断言，要么用它收集整段时间的全部消息。
    pub async fn collect(&mut self, duration: Duration) -> Vec<Value> {
        let deadline = tokio::time::Instant::now() + duration;
        let mut collected = Vec::new();
        loop {
            match tokio::time::timeout_at(deadline, self.take()).await {
                Ok(message) => collected.push(message),
                Err(_) => return collected,
            }
        }
    }

    /// 期待下一条消息的 `type`：不是该类型时把消息放进待取队列并 panic 出可读的上下文。
    ///
    /// 只跳过「与本步骤无关的推送」——调用方要断言的类型必须真的到达（不静默丢弃）。
    pub async fn expect(&mut self, message_type: &str) -> Value {
        let deadline = tokio::time::Instant::now() + MESSAGE_TIMEOUT;
        let mut skipped: Vec<Value> = Vec::new();
        loop {
            let message = self.take().await;
            if message["type"] == json!(message_type) {
                // 跳过的推送按到达顺序放回待取队列（后续步骤仍能取到它们）。
                self.pending.extend(skipped);
                return message;
            }
            // 与当前步骤无关的推送（`resource.event`／别处的终态观察）留在待取队列里，由需要它的
            // 步骤取用；其余类型直接失败，避免把协议错误吃成超时。
            let other = message["type"].as_str().unwrap_or("<无 type>").to_owned();
            let detail = message.to_string();
            let tolerable = other.starts_with("catalog.snapshot") || other.starts_with("resource.");
            assert!(
                tolerable,
                "步骤「{}」期待 `{message_type}`，收到无关类型 `{other}`（已发 {}）：{detail}",
                self.step,
                self.last_sent.as_deref().unwrap_or("<无>")
            );
            // 跳过的消息**不进**待取队列：进了就会被本轮 `take` 再次取回，loop 变成空转。
            skipped.push(message);
            assert!(
                tokio::time::Instant::now() < deadline,
                "步骤「{}」在超时前没有等到 `{message_type}`（已收到 {} 条无关推送）",
                self.step,
                skipped.len()
            );
        }
    }

    /// 取下一条消息（控制帧跳过；连接结束即 panic）。
    async fn next_message(&mut self) -> Value {
        if !self.pending.is_empty() {
            return self.pending.remove(0);
        }
        loop {
            match self.next_frame().await {
                None => panic!(
                    "步骤「{}」连接在对端消息之前结束（已发 {}）",
                    self.step,
                    self.last_sent.as_deref().unwrap_or("<无>")
                ),
                Some(Frame::Text(text)) => {
                    if trace_enabled() {
                        eprintln!(
                            "[access] t={:?} <- {}",
                            self.opened_at.elapsed(),
                            &text[..text.len().min(160)]
                        );
                    }
                    let message: Value = serde_json::from_str(&text)
                        .unwrap_or_else(|error| panic!("服务端消息不是 JSON（{error}）：{text}"));
                    self.check_wire_sequence(&message);
                    return message;
                }
                Some(Frame::Other) => continue,
                Some(Frame::Close(code, reason)) => panic!(
                    "步骤「{}」对端在消息之前关闭了连接（t={:?}）：{code} {reason}",
                    self.step,
                    self.opened_at.elapsed()
                ),
            }
        }
    }

    /// 核对 wire 上到达的认证后消息：`connectionSequence` 严格加一、`connectionId` 与本连接一致。
    ///
    /// 认证前的信封把 `connectionId` 放在 `body` 里（没有顶层字段），因此不会被计入。
    fn check_wire_sequence(&mut self, message: &Value) {
        if message.get("connectionId").is_none() {
            return;
        }
        self.server_sequence += 1;
        assert_eq!(
            message["connectionSequence"],
            json!(self.server_sequence.to_string()),
            "服务端出站序号必须严格加一：{message}"
        );
        assert_eq!(
            message["connectionId"],
            json!(
                self.connection_id
                    .clone()
                    .expect("认证后消息必须已有 connectionId")
            ),
            "服务端消息必须带本连接的 connectionId：{message}"
        );
    }

    /// 读到 close 帧（中途的文本帧收进待取队列），返回 close code。
    pub async fn close_code(&mut self) -> u16 {
        loop {
            match self.next_frame().await {
                None => panic!("对端在没有 close 帧的情况下结束了连接"),
                Some(Frame::Text(text)) => self
                    .pending
                    .push(serde_json::from_str(&text).expect("文本帧是 JSON")),
                Some(Frame::Close(code, _)) => return code,
                Some(Frame::Other) => {}
            }
        }
    }

    async fn next_frame(&mut self) -> Option<Frame> {
        self.fill(2).await?;
        let opcode = self.buffer[0] & 0x0f;
        let mut length = u64::from(self.buffer[1] & 0x7f);
        let mut offset = 2usize;
        if length == 126 {
            self.fill(4).await?;
            length = u64::from(u16::from_be_bytes([self.buffer[2], self.buffer[3]]));
            offset = 4;
        } else if length == 127 {
            self.fill(10).await?;
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&self.buffer[2..10]);
            length = u64::from_be_bytes(bytes);
            offset = 10;
        }
        let length = usize::try_from(length).expect("帧长度可放进 usize");
        self.fill(offset + length).await?;
        let payload = self.buffer[offset..offset + length].to_vec();
        self.buffer.drain(..offset + length);
        Some(match opcode {
            0x1 => Frame::Text(String::from_utf8(payload).expect("文本帧必须是 UTF-8")),
            0x8 => {
                let code = payload
                    .get(..2)
                    .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
                    .unwrap_or(0);
                let reason = payload
                    .get(2..)
                    .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
                    .unwrap_or_default();
                Frame::Close(code, reason)
            }
            _ => Frame::Other,
        })
    }

    /// 确保缓冲区里至少有 `needed` 字节；返回 `None` 表示对端已关闭流。
    async fn fill(&mut self, needed: usize) -> Option<()> {
        while self.buffer.len() < needed {
            let mut chunk = [0u8; 16 * 1024];
            let read = tokio::time::timeout(MESSAGE_TIMEOUT, self.stream.read(&mut chunk))
                .await
                .expect("读帧超时")
                .expect("读帧");
            if read == 0 {
                return None;
            }
            self.buffer.extend_from_slice(&chunk[..read]);
        }
        Some(())
    }
}

// ---------------------------------------------------------------------------------------------
// 握手（含 catalogRevision 验签路径）
// ---------------------------------------------------------------------------------------------

/// Access 侧声明的 feature（本切片实现的五条 mvp feature）。
const DECLARED_FEATURES: [&str; 5] = [
    "node-link.core.v1",
    "node-link.raw-acp.v1",
    "node-link.command-status.v1",
    "node-link.session-create.v1",
    "node-link.export-revoke.v1",
];

/// 执行一次完整握手（hello → challenge → proof → ready），返回 `node.ready`。
///
/// Access 侧在这里做两件必须做的事：
///
/// 1. **验证 Owner 的挑战证明**（`node.challenge.nodeProof`）：它必须由二维码里的 `ownerPublicKey`
///    对 `node-link-challenge/v1`（tag 6 = `catalogRevision`）验证通过——这就是「catalogRevision 验签
///    路径」；取错 revision 必然验签失败；
/// 2. **构造自己的证明**（`node.proof`）：同一条 revision 进 `node-link-proof/v1` 的 tag 6。
pub async fn handshake(
    client: &mut NodeLinkClient,
    access: &AccessKey,
    ticket: &PairingTicket,
    client_nonce: &str,
) -> Value {
    client.step("node.hello");
    client
        .send_pre_auth(
            "node.hello",
            json!({
                "minProtocolVersion": 1,
                "maxProtocolVersion": 1,
                "accessNodeId": access.node_id(),
                "role": "access",
                "clientNonce": client_nonce,
                "supportedFeatures": DECLARED_FEATURES,
                "requiredFeatures": ["node-link.core.v1"],
            }),
        )
        .await;

    client.step("node.challenge");
    let challenge = client.expect("node.challenge").await;
    let body = challenge["body"].clone();
    let connection_id = body["connectionId"]
        .as_str()
        .expect("node.challenge 必须带 connectionId")
        .to_owned();
    client.connection_id = Some(connection_id.clone());
    let selected: Vec<String> = body["selectedFeatures"]
        .as_array()
        .expect("selectedFeatures")
        .iter()
        .map(|feature| feature.as_str().expect("feature id").to_owned())
        .collect();
    let catalog_revision: u64 = body["catalogRevision"]
        .as_str()
        .expect("node.challenge 必须带 catalogRevision")
        .parse()
        .expect("catalogRevision 是十进制串");

    // ① 验 Owner 的挑战证明（同一 revision）。
    let challenge_input = NodeLinkChallenge {
        owner_node_id: node(&ticket.owner_node_id),
        access_node_id: node(access.node_id()),
        catalog_revision,
        client_nonce: Nonce::new(client_nonce).expect("规范 nonce"),
        server_nonce: Nonce::new(body["serverNonce"].as_str().expect("serverNonce"))
            .expect("serverNonce 是规范 nonce"),
        connection_id: identity_auth::ChallengeId::parse(&connection_id)
            .expect("connectionId 是规范 UUID"),
        negotiated_features: FeatureList::new(selected.clone()),
    };
    let signature =
        P1363Signature::try_from_base64url(body["nodeProof"].as_str().expect("nodeProof 是字符串"))
            .expect("nodeProof 是 64 字节 P1363");
    challenge_input
        .verify(&ticket.owner_public_key, &signature)
        .expect("node.challenge 的 nodeProof 必须由 ownerPublicKey 对 catalogRevision 验签通过");

    // ② 用同一条 revision 构造自己的证明。
    let proof_input = NodeLinkProof {
        owner_node_id: node(&ticket.owner_node_id),
        access_node_id: node(access.node_id()),
        catalog_revision,
        client_nonce: Nonce::new(client_nonce).expect("规范 nonce"),
        server_nonce: Nonce::new(body["serverNonce"].as_str().expect("serverNonce"))
            .expect("serverNonce 是规范 nonce"),
        connection_id: identity_auth::ChallengeId::parse(&connection_id)
            .expect("connectionId 是规范 UUID"),
        negotiated_features: FeatureList::new(selected),
    };
    let transcript = proof_input.transcript().expect("proof transcript 可装配");
    client.step("node.proof");
    client
        .send_pre_auth(
            "node.proof",
            json!({
                "connectionId": connection_id,
                "accessNodeId": access.node_id(),
                "nodeProof": access.sign(&transcript),
            }),
        )
        .await;

    client.step("node.ready");
    let ready = client.expect("node.ready").await;
    assert_eq!(
        ready["body"]["ownerNodeId"],
        json!(ticket.owner_node_id),
        "node.ready 必须回本机（Owner）node id"
    );
    assert_eq!(
        ready["body"]["catalogRevision"],
        json!(catalog_revision.to_string()),
        "node.ready 的 catalogRevision 必须与 node.challenge 同源"
    );
    ready
}
