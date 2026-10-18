//! Sync 协议级端到端验收的**设备侧**（fake Sync client）与它的 wire 客户端。
//!
//! 形状来源（每个字段名都取自律法的公开类型，不手抄一份）：
//!
//! - 信封与握手 body：`sync_protocol::auth`（`ClientHello`/`ServerChallenge`/`ClientProof`/
//!   `Authenticated`）、`sync_protocol::envelope`（`Envelope` 的连接字段规则）；
//! - 订阅与快照 body：`sync_protocol::sync`（`Subscribe`/`SnapshotRequest`/`Ack`/`CaughtUp`/
//!   `SnapshotBegin`/`SnapshotChunk`/`SnapshotEnd`）；
//! - 证明的转录：`identity_auth::SyncDeviceProof`（域 `acp-remote/device-proof/v1`）；
//! - 端到端的行为口径：`docs/SYNC_PROTOCOL.md` §4/§8（认证前无连接字段、认证后从 1 严格加一）。
//!
//! 边界（与 `support::nodelink` 同族）：本 crate 没有 `sync-client` 实现，因此这份客户端只服务测试；
//! 它不做重连、不做游标持久化，只提供「握手 / 订阅 / 收快照并复算摘要 / 收事件 / ACK」五种能力。
//! **它必须经真实 loopback listener**（`support::owner::OwnerNode` 绑定的地址）连接，不得直调处理器。
//!
//! 「复算摘要」是本模块的**独立**计算路径：客户端只保存收到的 `snapshot_chunk` **帧文本**，
//! 逐 chunk 求 SHA-256 后按 `chunkIndex` 顺序连接再求一次（§9.4），与声明值逐字节比对。

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use super::TempRoot;
use acp_core::broker::Broker;
use acp_core::model::PortError;
use acp_core::ports::SessionBackendFactory;
use acp_core::use_cases::{UseCaseDeps, UseCases};
use base64::Engine as _;
use identity_auth::{
    Authority, CanonicalOrigin, ChallengeId, FeatureList, Nonce, P1363Signature, SyncDeviceProof,
};
use serde_json::{Value, json};
use server::local_admin::{LocalAdminDeps, LocalAdminRouter};
use sha2::{Digest as _, Sha256};
use storage_sqlite::session_store::SqliteStore;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::TcpStream;

/// 单次 I/O 的超时：用例失败要快速失败，不挂住测试进程。
pub const IO_TIMEOUT: Duration = Duration::from_secs(10);

/// 等一条消息的超时（快照/重放都留给 loopback 真实传输，10 秒已远超需要）。
pub const MESSAGE_TIMEOUT: Duration = Duration::from_secs(10);

/// 负向断言（「一段时间内没有出现」）的收集窗口。
pub const COLLECT_WINDOW: Duration = Duration::from_millis(400);

/// 一条可以被明文或 TLS 包装的字节流。
pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<S: AsyncRead + AsyncWrite + Unpin + Send> Stream for S {}

/// 设备长期密钥（固定标量：用例不依赖随机源，仓库也不提交私钥材料）。
///
/// 末字节由设备 id 派生，因此每个用例的设备各有一把可复现的密钥（与 `support::nodelink::AccessKey`
/// 同款做法）。
pub struct DeviceKey {
    device_id: String,
    signing: p256::ecdsa::SigningKey,
}

impl DeviceKey {
    pub fn new(device_id: &str) -> Self {
        let mut scalar = [0u8; 32];
        scalar[31] = 3;
        let seed = device_id
            .bytes()
            .fold(0u8, |accumulator, byte| accumulator ^ byte);
        scalar[30] = seed;
        Self {
            device_id: device_id.to_owned(),
            signing: p256::ecdsa::SigningKey::from_slice(&scalar).expect("固定标量是合法私钥"),
        }
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// 65 字节 SEC1 未压缩点（配对 claim 与信任行都按它登记）。
    pub fn public_key(&self) -> acp_core::model::PeerPublicKey {
        let point = self.signing.verifying_key().to_sec1_point(false);
        acp_core::model::PeerPublicKey::try_from_bytes(point.as_bytes()).expect("65 字节 SEC1 公钥")
    }

    /// 对 `acp-remote/device-proof/v1` 转录签名，输出无填充 base64url（wire 上的 64 字节）。
    pub fn sign_challenge(&self, input: &SyncDeviceProof) -> String {
        use p256::ecdsa::signature::Signer as _;
        let transcript = input.transcript().expect("transcript 可装配");
        let signature: p256::ecdsa::Signature = self.signing.sign(&transcript);
        base64url(signature.to_bytes().as_slice())
    }
}

/// 无填充 base64url 编码。
pub fn base64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// 规范 UUID 文本（`Uuid::new_v4` 的输出必然合规）。
pub fn uuid_text() -> String {
    uuid::Uuid::new_v4().hyphenated().to_string()
}

/// 柜台（每条消息一个 id，便于把失败定位到具体一条）。
static COUNTER: AtomicU32 = AtomicU32::new(1);

fn next_mark() -> String {
    format!("{:012x}", COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// 一条 12 位十六进制尾组的 uuid 文本（messageId/自造 id 都按 canonical uuid 校验）。
fn uuid_mark() -> String {
    format!("00000000-0000-4000-8000-{}", next_mark())
}

/// 32 字节 nonce 的规范文本（`clientNonce`/`serverNonce` 的 wire 约束）。
pub fn nonce_text(seed: u8) -> String {
    base64url(&[seed; 32])
}

// ---------------------------------------------------------------------------------------------
// 快照摘要的独立复算
// ---------------------------------------------------------------------------------------------

/// **独立**按收到的原始帧文本复算 `snapshotDigest`（§9.4）。
///
/// 与本模块不共享服务端实现的任何代码：输入是收到的 `snapshot_chunk` 帧**完整文本字节**，
/// 逐 chunk 求 SHA-256，按 `chunkIndex` 升序连接后再求一次 SHA-256，无填充 base64url 表示。
pub fn recompute_digest(chunk_frames: &[String]) -> String {
    let mut hasher = Sha256::new();
    for frame in chunk_frames {
        hasher.update(Sha256::digest(frame.as_bytes()));
    }
    base64url(&hasher.finalize())
}

/// 在任意层级查找一个键（对象与数组都递归）。
///
/// 用于「某个键**不得**出现」这类检查：授权集合（`scopes`/`grants`/`packs`/`presets`）与短验证码
/// （`sas`）在下行消息里可能被塞进嵌套块（`statusResponse.device`、`auth.authenticated.body`），
/// 只查顶层会漏掉它们。
pub fn contains_key_deep(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(map) => {
            map.contains_key(key) || map.values().any(|entry| contains_key_deep(entry, key))
        }
        Value::Array(items) => items.iter().any(|item| contains_key_deep(item, key)),
        _ => false,
    }
}

/// 一次收到的快照（三类资源的 chunk 帧文本与声明值，供用例逐步断言）。
#[derive(Debug)]
pub struct ReceivedSnapshot {
    pub snapshot_id: String,
    pub chunk_count: u64,
    /// 按 `chunkIndex` 升序的 chunk（帧文本原样保留，摘要复算只用它们）。
    pub chunks: Vec<SnapshotChunkView>,
    /// `sync.snapshot_begin` 的 barrier cursor。
    pub begin_cursor: Value,
    /// `sync.snapshot_end` 的 cursor（与 begin 相同）。
    pub end_cursor: Value,
    pub declared_digest: String,
}

#[derive(Debug)]
pub struct SnapshotChunkView {
    pub resource: String,
    pub chunk_index: u64,
    /// 收到的**原始帧文本**（含 envelope），摘要复算的输入。
    pub frame_text: String,
    pub items: Vec<Value>,
}

impl ReceivedSnapshot {
    /// 按收到的原始帧文本复算摘要，与声明值逐字节比对。
    pub fn digest_matches(&self) -> bool {
        let frames: Vec<String> = self
            .chunks
            .iter()
            .map(|chunk| chunk.frame_text.clone())
            .collect();
        recompute_digest(&frames) == self.declared_digest
    }

    fn items_of(&self, resource: &str) -> Vec<&Value> {
        self.chunks
            .iter()
            .find(|chunk| chunk.resource == resource)
            .map(|chunk| chunk.items.iter().collect())
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------------------------
// WebSocket 客户端
// ---------------------------------------------------------------------------------------------

/// 一个 WebSocket 帧。
#[derive(Debug)]
enum Frame {
    Text(String),
    Close(u16, String),
    Other,
}

/// 设备侧的 Sync 连接（WebSocket 之上的信封层）。
pub struct SyncClient {
    stream: Box<dyn Stream>,
    buffer: Vec<u8>,
    host_id: String,
    origin: String,
    connection_id: Option<String>,
    /// 本方向已发出的认证后消息数（`connectionSequence` = 它 + 1）。
    outbound: u64,
    /// 对端已到达的认证后消息数（期望序号 = 它 + 1）。
    inbound: u64,
    pending: Vec<Value>,
    step: String,
}

impl SyncClient {
    /// 明文连接并完成 WebSocket 升级（`dev_mode.allow_plaintext` 的轮次）。
    pub async fn connect_plain(addr: SocketAddr, host: &str, origin: &str) -> Self {
        let stream = TcpStream::connect(addr).await.expect("连接 loopback");
        Self::upgrade(Box::new(stream), host, origin).await
    }

    /// TLS 连接并升级（`tls.mode = "direct"` 的轮次）。
    pub async fn connect_tls(
        addr: SocketAddr,
        host: &str,
        origin: &str,
        config: std::sync::Arc<rustls::ClientConfig>,
    ) -> Self {
        let stream = tokio::time::timeout(IO_TIMEOUT, TcpStream::connect(addr))
            .await
            .expect("连接超时")
            .expect("连接 loopback");
        let connector = tokio_rustls::TlsConnector::from(config);
        let server_name = rustls_pki_types::ServerName::try_from(host)
            .expect("合法 SNI")
            .to_owned();
        let stream = tokio::time::timeout(IO_TIMEOUT, connector.connect(server_name, stream))
            .await
            .expect("TLS 握手超时")
            .expect("TLS 握手");
        Self::upgrade(Box::new(stream), host, origin).await
    }

    async fn upgrade(mut stream: Box<dyn Stream>, host: &str, origin: &str) -> Self {
        let key = base64::engine::general_purpose::STANDARD.encode([0x2a_u8; 16]);
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Protocol: {}\r\nOrigin: {origin}\r\n\r\n",
            server::sync::conn::WS_PATH,
            server::sync::conn::WS_SUBPROTOCOL,
        );
        tokio::time::timeout(IO_TIMEOUT, stream.write_all(request.as_bytes()))
            .await
            .expect("写升级请求超时")
            .expect("写升级请求");
        let mut buffer = Vec::new();
        let head_end = loop {
            if let Some(head_end) = sub(&buffer, b"\r\n\r\n") {
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
            host_id: String::new(),
            origin: origin.to_owned(),
            connection_id: None,
            outbound: 0,
            inbound: 0,
            pending: Vec::new(),
            step: String::new(),
        }
    }

    /// 尝试升级：`Err(升级响应头)` 表示被接入层拒绝（非 101）。
    pub async fn try_upgrade(addr: SocketAddr, host: &str, origin: &str) -> Result<Self, String> {
        let stream = TcpStream::connect(addr).await.expect("连接 loopback");
        let mut stream: Box<dyn Stream> = Box::new(stream);
        let key = base64::engine::general_purpose::STANDARD.encode([0x2a_u8; 16]);
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Protocol: {}\r\nOrigin: {origin}\r\n\r\n",
            server::sync::conn::WS_PATH,
            server::sync::conn::WS_SUBPROTOCOL,
        );
        tokio::time::timeout(IO_TIMEOUT, stream.write_all(request.as_bytes()))
            .await
            .expect("写升级请求超时")
            .expect("写升级请求");
        let mut buffer = Vec::new();
        let head = loop {
            if let Some(head_end) = sub(&buffer, b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
                buffer.drain(..head_end + 4);
                break head;
            }
            let mut chunk = [0u8; 1024];
            let read = tokio::time::timeout(IO_TIMEOUT, stream.read(&mut chunk))
                .await
                .expect("读升级响应超时")
                .expect("读升级响应");
            if read == 0 {
                return Err(String::from_utf8_lossy(&buffer).into_owned());
            }
            buffer.extend_from_slice(&chunk[..read]);
        };
        if !head.starts_with("HTTP/1.1 101 ") {
            return Err(head);
        }
        Ok(Self {
            stream,
            buffer,
            host_id: String::new(),
            origin: origin.to_owned(),
            connection_id: None,
            outbound: 0,
            inbound: 0,
            pending: Vec::new(),
            step: String::new(),
        })
    }

    /// 标记当前步骤（失败信息里带上它，便于定位握手/订阅/快照阶段）。
    pub fn step(&mut self, label: &str) {
        self.step = label.to_owned();
    }

    /// 本连接的 canonical origin（握手转录要用同一个值）。
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// 认证后拿到的 `connectionId`（未认证时为 `None`）。
    pub fn connection_id(&self) -> Option<&str> {
        self.connection_id.as_deref()
    }

    /// 四消息握手：`auth.client_hello` → `auth.server_challenge` → `auth.client_proof` →
    /// `auth.authenticated`。返回最后一条消息（失败路径上是 `error`）。
    pub async fn authenticate(
        &mut self,
        host_id: &str,
        device: &DeviceKey,
        supported_features: &[&str],
        client_nonce: &str,
    ) -> Value {
        self.host_id = host_id.to_owned();
        let supported = supported_features
            .iter()
            .map(|feature| (*feature).to_owned())
            .collect::<Vec<_>>();
        self.send_pre_auth(
            "auth.client_hello",
            json!({
                "minProtocolVersion": 1,
                "maxProtocolVersion": 1,
                "hostId": host_id,
                "deviceId": device.device_id(),
                "clientKind": "pwa",
                "clientNonce": client_nonce,
                "supportedFeatures": supported,
                "requiredFeatures": [],
            }),
        )
        .await;
        let challenge = self.expect_type("auth.server_challenge").await;
        let body = &challenge["body"];
        let connection_id = body["connectionId"]
            .as_str()
            .expect("connectionId")
            .to_owned();
        let selected: Vec<String> = body["selectedFeatures"]
            .as_array()
            .expect("selectedFeatures")
            .iter()
            .map(|feature| feature.as_str().expect("feature id").to_owned())
            .collect();
        let server_nonce = body["serverNonce"]
            .as_str()
            .expect("serverNonce")
            .to_owned();
        let input = SyncDeviceProof {
            host_id: acp_core::model::NodeId::new(host_id).expect("host id 规范"),
            device_id: acp_core::model::DeviceId::new(device.device_id()).expect("device id 规范"),
            canonical_origin: CanonicalOrigin::parse(&self.origin).expect("规范 canonical origin"),
            client_nonce: Nonce::new(client_nonce).expect("规范 nonce"),
            server_nonce: Nonce::new(&server_nonce).expect("规范 nonce"),
            connection_id: ChallengeId::parse(&connection_id).expect("connectionId 是 uuid"),
            negotiated_features: FeatureList::new(selected),
        };
        let proof = device.sign_challenge(&input);
        self.send_pre_auth(
            "auth.client_proof",
            json!({
                "connectionId": connection_id,
                "deviceId": device.device_id(),
                "deviceProof": proof,
            }),
        )
        .await;
        let authenticated = self.expect_any().await;
        if authenticated["type"] == json!("auth.authenticated") {
            self.connection_id = Some(connection_id);
        }
        authenticated
    }

    /// 认证前消息（信封不带 `connectionId`/`connectionSequence`，§4.1）。
    pub async fn send_pre_auth(&mut self, message_type: &str, body: Value) {
        let text = json!({
            "protocolVersion": 1,
            "type": message_type,
            "messageId": uuid_mark(),
            "body": body,
        })
        .to_string();
        self.send_text(&text).await;
    }

    /// 认证后消息（信封带 `connectionId` 与下一序号；§4.1：序号描述对端已发出的消息）。
    pub async fn send(&mut self, message_type: &str, body: Value) {
        let connection_id = self
            .connection_id
            .clone()
            .expect("认证后消息必须已有 connectionId");
        self.outbound += 1;
        let sequence = self.outbound.to_string();
        let text = json!({
            "protocolVersion": 1,
            "type": message_type,
            "messageId": uuid_mark(),
            "connectionId": connection_id,
            "connectionSequence": sequence,
            "body": body,
        })
        .to_string();
        self.send_text(&text).await;
    }

    /// `sync.subscribe`（`cursor` 可为 `null` = 首次同步）。
    pub async fn subscribe(&mut self, cursor: Option<Value>) {
        self.send(
            "sync.subscribe",
            json!({ "cursor": cursor, "scope": "machine" }),
        )
        .await;
    }

    /// `sync.snapshot_request`（收到 `sync.reset_required` 之后）。
    pub async fn snapshot_request(&mut self, reason: &str) {
        self.send("sync.snapshot_request", json!({ "reason": reason }))
            .await;
    }

    /// `sync.ack`。
    pub async fn ack(&mut self, cursor: Value) {
        self.send("sync.ack", json!({ "cursor": cursor })).await;
    }

    /// 派发一条 `command` 帧（查询与 mutation 同一个帧形状；`sessionId` 可缺席）。
    pub async fn command(
        &mut self,
        request_id: &str,
        command: &str,
        session_id: Option<&str>,
        payload: Value,
    ) {
        self.command_with_version(request_id, command, session_id, payload, None)
            .await;
    }

    /// 同上，但带上 body 顶层的 `expectedVersion`（`session.mode.set`/`session.config.set` 必填）。
    pub async fn command_with_version(
        &mut self,
        request_id: &str,
        command: &str,
        session_id: Option<&str>,
        payload: Value,
        expected_version: Option<&str>,
    ) {
        let mut body = json!({
            "requestId": request_id,
            "command": command,
            "payload": payload,
        });
        if let Some(session_id) = session_id {
            body["sessionId"] = json!(session_id);
        }
        if let Some(version) = expected_version {
            body["expectedVersion"] = json!(version);
        }
        self.send("command", body).await;
    }

    /// 等下一条**属于该 requestId** 的 `command.result`（跳过事件与控制帧），返回它的 `body`。
    ///
    /// 只按 requestId 过滤、不按到达顺序假定：mutation 的 `accepted` 与终态是两条独立的
    /// `command.result`，调用方按其 `status` 自己判定。
    pub async fn expect_command_result(&mut self, request_id: &str) -> Value {
        let deadline = tokio::time::Instant::now() + MESSAGE_TIMEOUT;
        loop {
            let message = self.take().await;
            if message["type"] == json!("command.result")
                && message["body"]["requestId"] == json!(request_id)
            {
                return message["body"].clone();
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "步骤「{}」在超时前没有等到 `{request_id}` 的 command.result",
                self.step
            );
        }
    }

    /// 期待下一条消息的类型（跳过 `control.ping`；其它类型 panic 出可读上下文）。
    pub async fn expect_type(&mut self, message_type: &str) -> Value {
        let deadline = tokio::time::Instant::now() + MESSAGE_TIMEOUT;
        loop {
            let message = self.take().await;
            if message["type"] == json!(message_type) {
                return message;
            }
            assert_eq!(
                message["type"],
                json!(message_type),
                "步骤「{}」期待 `{}`，实际：{}",
                self.step,
                message_type,
                message
            );
            assert!(
                tokio::time::Instant::now() < deadline,
                "步骤「{}」在超时前没有等到 `{}`",
                self.step,
                message_type
            );
        }
    }

    /// 取下一条消息（跳过控制帧，不做类型断言）。
    pub async fn take(&mut self) -> Value {
        self.expect_any().await
    }

    /// 收集一段时间内的全部消息（负向断言用：证明「某类消息没有出现」）。
    ///
    /// 正常的投递断言用 [`Self::expect_type`]/[`Self::expect_snapshot`]（它们会核对出站序号）；
    /// 本方法只用于「此后不应再出现 X」这类否定式观察。
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

    /// 读 close 帧（中途的文本帧收进待取队列），返回 close code。
    pub async fn close_code(&mut self) -> u16 {
        loop {
            match self.next_frame().await {
                None => panic!("步骤「{}」对端在没有 close 帧的情况下结束了连接", self.step),
                Some(Frame::Text(text)) => self
                    .pending
                    .push(serde_json::from_str(&text).expect("文本帧是 JSON")),
                Some(Frame::Close(code, _)) => return code,
                Some(Frame::Other) => {}
            }
        }
    }

    /// 读一条完整的快照（begin → chunk* → end），保存帧文本并返回视图。
    pub async fn expect_snapshot(&mut self) -> ReceivedSnapshot {
        let begin = self.expect_type("sync.snapshot_begin").await;
        let snapshot_id = begin["body"]["snapshotId"]
            .as_str()
            .expect("snapshotId")
            .to_owned();
        let chunk_count = begin["body"]["chunkCount"]
            .as_u64()
            .expect("chunkCount 是整数");
        let begin_cursor = begin["body"]["cursor"].clone();
        let mut chunks: Vec<SnapshotChunkView> = Vec::new();
        let last_index = chunk_count - 1;
        loop {
            let (message, frame_text) = self.expect_raw("sync.snapshot_chunk").await;
            let body = &message["body"];
            let chunk_index = body["chunkIndex"]
                .as_str()
                .expect("chunkIndex")
                .parse::<u64>()
                .expect("chunkIndex 是十进制串");
            chunks.push(SnapshotChunkView {
                resource: body["resource"].as_str().expect("resource").to_owned(),
                chunk_index,
                frame_text,
                items: body["items"].as_array().expect("items 数组").to_vec(),
            });
            if chunk_index == last_index {
                break;
            }
        }
        let end = self.expect_type("sync.snapshot_end").await;
        assert_eq!(
            end["body"]["snapshotId"],
            json!(snapshot_id),
            "快照的 snapshotId 必须首尾一致：{end}"
        );
        assert_eq!(
            end["body"]["chunkCount"],
            json!(chunk_count),
            "begin 与 end 的 chunkCount 必须一致：{end}"
        );
        ReceivedSnapshot {
            snapshot_id,
            chunk_count,
            chunks,
            begin_cursor,
            end_cursor: end["body"]["cursor"].clone(),
            declared_digest: end["body"]["snapshotDigest"]
                .as_str()
                .expect("snapshotDigest")
                .to_owned(),
        }
    }

    /// 读一条 `error` 消息并返回它（关闭帧随后由 `close_code` 读）。
    pub async fn expect_error(&mut self) -> Value {
        self.expect_type("error").await
    }

    /// 取下一条消息（先看待取队列，再读 wire）。
    async fn expect_any(&mut self) -> Value {
        if !self.pending.is_empty() {
            return self.pending.remove(0);
        }
        loop {
            match self.next_frame().await {
                None => panic!(
                    "步骤「{}」连接在对端消息之前结束（已发 {}）",
                    self.step, self.outbound
                ),
                Some(Frame::Text(text)) => {
                    let message: Value = serde_json::from_str(&text)
                        .unwrap_or_else(|error| panic!("服务端消息不是 JSON（{error}）：{text}"));
                    self.check_wire_sequence(&message);
                    return message;
                }
                Some(Frame::Other) => continue,
                Some(Frame::Close(code, reason)) => panic!(
                    "步骤「{}」对端在消息之前关闭了连接：{code} {reason}",
                    self.step
                ),
            }
        }
    }

    /// 同 [`Self::expect_type`]，但**同时**回传收到的原始帧文本（摘要复算需要它）。
    async fn expect_raw(&mut self, message_type: &str) -> (Value, String) {
        while !self.pending.is_empty() {
            let message = self.pending.remove(0);
            if message["type"] == json!(message_type) {
                let text = serde_json::to_string(&message).expect("可序列化");
                return (message, text);
            }
        }
        loop {
            match self.next_frame().await {
                None => panic!("步骤「{}」连接在对端 {} 之前结束", self.step, message_type),
                Some(Frame::Text(text)) => {
                    let message: Value = serde_json::from_str(&text)
                        .unwrap_or_else(|error| panic!("服务端消息不是 JSON（{error}）：{text}"));
                    if message["type"] == json!(message_type) {
                        self.check_wire_sequence(&message);
                        return (message, text);
                    }
                }
                Some(Frame::Other) => continue,
                Some(Frame::Close(code, reason)) => panic!(
                    "步骤「{}」对端在 {} 之前关闭了连接：{code} {reason}",
                    self.step, message_type
                ),
            }
        }
    }

    /// 核对 wire 上到达的认证后消息：`connectionSequence` 严格加一、`connectionId` 与本连接一致。
    fn check_wire_sequence(&mut self, message: &Value) {
        if message.get("connectionId").is_none() {
            return;
        }
        // 认证后的第一条消息（`auth.authenticated`）自带连接字段：此时本方向还不知道自己的
        // `connectionId`，因此先由它建立（其后每条都必须与它一致）。
        if self.connection_id.is_none() {
            self.connection_id = message["connectionId"]
                .as_str()
                .map(str::to_owned)
                .filter(|id| !id.is_empty());
        }
        self.inbound += 1;
        assert_eq!(
            message["connectionSequence"],
            json!(self.inbound.to_string()),
            "服务端出站序号必须严格加一：{}",
            message
        );
        assert_eq!(
            message["connectionId"],
            json!(
                self.connection_id
                    .clone()
                    .expect("认证后消息必须已有 connectionId")
            ),
            "服务端消息必须带本连接的 connectionId：{}",
            message
        );
    }

    async fn send_text(&mut self, text: &str) {
        let payload = text.as_bytes();
        let mut frame = Vec::with_capacity(payload.len() + 14);
        frame.push(0x81);
        let mask = [0x21u8, 0x43, 0x65, 0x87];
        if payload.len() < 126 {
            frame.push(0x80 | payload.len() as u8);
        } else if payload.len() <= u16::MAX as usize {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        } else {
            frame.push(0x80 | 127);
            frame.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        }
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

/// `haystack` 里 `needle` 第一次出现的下标。
fn sub(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// 快照的资源断言辅助（用例只按名字取条目）。
pub fn snapshot_items<'a>(snapshot: &'a ReceivedSnapshot, resource: &str) -> Vec<&'a Value> {
    snapshot.items_of(resource)
}

/// 对 `P1363Signature` 只做形状转换的轻量包装（测试不需要再验一次签）。
pub fn signature_of(base64url_text: &str) -> P1363Signature {
    P1363Signature::try_from_base64url(base64url_text).expect("64 字节 P1363 签名")
}

// ---------------------------------------------------------------------------------------------
// 本机（Owner）侧：真实 SQLite + 真实 Authority + 真实组合根装配
// ---------------------------------------------------------------------------------------------

/// Sync 端到端验收里的**本机**（Owner Node）。
///
/// 与 `support::owner::OwnerNode` 的关系：那份替身只注册 Node Link 的 path（`NodeLinkCloser` 里的
/// Sync 连接表恒空），因此不能驱动 Sync 连接面。本 harness 用**真实组合根自己的装配点**
/// （`app::daemon::{net_config, register_node_link_paths, register_sync_paths}`，与 `app::daemon::run`
/// 同一批函数）同时注册两个 path 家族，并把撤销通知缝接到**真实**的 Sync 连接表；
/// broker/存储/Authority/本地管理路由全部是真实现。
///
/// 三处刻意的测试替身（与 `owner.rs` 同款口径，只是换成「为 Sync 服务」）：
///
/// ① **Agent 后端与目录替身**：本用例不启动真实 Agent 子进程（`agent.connected`/`agent.disconnected`
///   是唯一会被真实提交的节点级事件，见 [`HostNode::emit_agent_lifecycle`]）；
/// ② **`ScriptedBackends`**：`create`/`open`/`resume` 都不启动进程，`prompt` 只把脚本事件暂存，
///    由用例显式释放（`release_events`）来制造一条**实时**事件；它同时执行 [`ScriptedScript`] 的行为
///    脚本（`create` 失败注入、窗口期事件注入、调用计数与终止记录）；
/// ③ **`OwnerReachability` 替身**（恒 `false`）：`sync_visible_sessions` 的 imported 分支要求调用方
///   给出它，而本用例不产生 imported 会话（import 需要 `import.add`，与本次判据无关），恒不可达
///   因此 imported 清单为空——这是一个**已知且被断言收窄**的替身，不是被测行为。
pub struct HostNode {
    root: TempRoot,
    /// 真实存储（全部端口共用同一个 `SqliteStore` 实例）。
    store: Arc<SqliteStore>,
    /// 真实用例面（`UseCases`）。
    core: Arc<UseCases>,
    /// 真实 broker。
    broker: Arc<Broker>,
    /// 真实身份状态机。
    authority: Arc<Authority>,
    /// 真实本地管理路由（本机确认/撤销都走它）。
    router: Arc<LocalAdminRouter>,
    /// 真实 listener 的实际地址。
    addr: SocketAddr,
    /// Sync 的活动连接表（撤销/顶替的观测口）。
    sync_registry: Arc<server::sync::conn::ConnectionRegistry>,
    /// 脚本端点暂存的后端事件。
    parked: Arc<ParkedEvents>,
    /// 脚本化后端的行为脚本（失败注入、窗口期事件注入与后端调用计数）。
    script: Arc<ScriptedScript>,
    /// 宿主名（`daemon.public_origin` 的 authority）。
    host_name: String,
    shutdown: server::transport::net::ShutdownHandle,
    tasks: Vec<(&'static str, tokio::task::JoinHandle<()>)>,
    _temp: (),
}

/// 脚本化后端暂存的 Agent 事件（`release` 时一次性送进 sink）。
#[derive(Default)]
pub struct ParkedEvents {
    events:
        std::sync::Mutex<std::collections::BTreeMap<String, Vec<acp_core::model::EndpointEvent>>>,
    sink: std::sync::Mutex<Option<acp_core::ports::EventSink>>,
}

impl ParkedEvents {
    /// 脚本端点建立时登记它的 sink：`release` 把暂存事件从这里送进真实扇出。
    ///
    /// 后建的端点会覆盖前一个（同一会话在用例里只有一个活动端点时就足够）。
    fn attach(&self, sink: &acp_core::ports::EventSink) {
        *self.sink.lock().expect("用例内互斥量不会中毒") = Some(sink.clone());
    }

    fn park(&self, marker: &str, events: Vec<acp_core::model::EndpointEvent>) {
        self.events
            .lock()
            .expect("用例内互斥量不会中毒")
            .insert(marker.to_owned(), events);
    }

    /// 释放某个 marker 暂存的事件（模拟 Agent 在 accepted 之后产出）。
    pub fn release(&self, marker: &str) -> usize {
        let events = self
            .events
            .lock()
            .expect("用例内互斥量不会中毒")
            .remove(marker)
            .unwrap_or_default();
        let count = events.len();
        match self.sink.lock().expect("用例内互斥量不会中毒").as_ref() {
            Some(sink) => {
                for event in events {
                    sink.send(event);
                }
            }
            None => panic!("脚本端点还没有拿到 sink"),
        }
        count
    }
}

/// 脚本化后端的**行为脚本**（`session-create-settle-order` 的失败与窗口期观测口）。
///
/// 默认取值是「建立成功、窗口期不产事件、终止是空操作」——其余 Sync 用例的行为因此逐字不变；
/// 只有显式调过 [`ScriptedScript::fail_create`] / [`ScriptedScript::emit_window_events`] 的用例才会
/// 走到失败与窗口期那两条路径。
#[derive(Default)]
pub struct ScriptedScript {
    /// 此后每一次 `create` 的失败类别；`None` = 正常建立。
    ///
    /// 存 [`acp_core::model::UnavailableKind`]（`Copy`）而不是 `PortError`：后者不实现 `Clone`，而每
    /// 一轮 `create` 都必须现造一个错误值（真实后端每次失败也都是新的错误值）。
    failure: std::sync::Mutex<Option<acp_core::model::UnavailableKind>>,
    /// 此后每一次 `create` 在返回之前推进 sink 的事件条数（窗口期事件的注入点）。
    window_events: std::sync::Mutex<usize>,
    /// `create` 的调用次数（「同 `requestId` 重发不派发第二次创建」的观测口）。
    creates: std::sync::atomic::AtomicUsize,
    /// 交给 `create` 的会话标识（按调用顺序）：失败路径下它就是「那一条**不该**有会话行的会话」。
    created: std::sync::Mutex<Vec<String>>,
    /// 收到 `terminate_agent` 的会话标识（窗口期超限失败关闭的观测口）。
    terminated: std::sync::Mutex<Vec<String>>,
}

impl ScriptedScript {
    /// 让此后每一次 `create` 都以该类别失败：`SessionNotEstablished` = 可证明 Agent 侧未产生会话
    /// （终态 `failed`），其余取值 = 说不清（终态 `uncertain`）。
    pub fn fail_create(&self, kind: acp_core::model::UnavailableKind) {
        *self.failure.lock().expect("用例内互斥量不会中毒") = Some(kind);
    }

    /// 让此后每一次 `create` 在**返回之前**向该会话的 sink 推 `count` 条事件。
    ///
    /// 这是「受理已落盘、会话行尚未落账」那一刻到达的后端事件（真实后端在建会话期间就会产出
    /// `session/update`）：core 把它们排队，落账提交之后按到达顺序写库再广播。
    pub fn emit_window_events(&self, count: usize) {
        *self.window_events.lock().expect("用例内互斥量不会中毒") = count;
    }

    fn failure(&self) -> Option<acp_core::model::UnavailableKind> {
        *self.failure.lock().expect("用例内互斥量不会中毒")
    }

    fn window_events(&self) -> usize {
        *self.window_events.lock().expect("用例内互斥量不会中毒")
    }

    /// `create` 已被调用几次（幂等重发的观测口，按后端收到的调用计数）。
    pub fn create_calls(&self) -> usize {
        self.creates.load(Ordering::SeqCst)
    }

    /// 交给 `create` 的会话标识（按调用顺序）：「失败零会话行」因此能断言到**具体会话**，而不是只看总数。
    pub fn created(&self) -> Vec<String> {
        self.created.lock().expect("用例内互斥量不会中毒").clone()
    }

    /// 已收到 `SessionBackendFactory::terminate_agent` 的会话标识（按调用顺序）。
    pub fn terminated(&self) -> Vec<String> {
        self.terminated
            .lock()
            .expect("用例内互斥量不会中毒")
            .clone()
    }
}

/// 窗口期注入的一条后端事件（`session.mode.changed`）。
///
/// view 里只写 `currentModeId`：`version` 是会话版本（§10.3），由 core 在提交时按当时的会话版本补齐，
/// 这里自己编一个可能与落库版本不一致的取值只会让提交失败——那不是本夹具要测的东西。
fn window_event(index: usize) -> acp_core::model::EndpointEvent {
    use acp_core::model::{EndpointEvent, EventKind, EventPayload, EventType, Timestamp, ViewJson};
    EndpointEvent::new(
        EventKind::State,
        EventType::new("session.mode.changed").expect("合法事件类型"),
        EventPayload::new(
            ViewJson::new(&format!(r#"{{"currentModeId":"m{index}"}}"#))
                .expect("view 是 JSON object"),
            None,
        ),
        None,
        None,
        Timestamp::new("2026-10-14T10:00:00.000Z").expect("规范时间戳"),
    )
}

struct ScriptedBackends {
    parked: Arc<ParkedEvents>,
    script: Arc<ScriptedScript>,
}

struct ScriptedCatalog;

#[async_trait::async_trait]
impl acp_core::ports::AgentCatalog for ScriptedCatalog {
    async fn agents(&self) -> Result<Vec<acp_core::model::AgentDescriptor>, PortError> {
        Ok(vec![acp_core::model::AgentDescriptor::new(
            acp_core::model::AgentRef::try_new(
                acp_core::model::AgentId::new(SCRIPTED_AGENT).expect("agent id"),
                "Codex",
            )
            .expect("agent ref"),
            true,
            acp_core::model::ResourceOrigin::Local,
        )])
    }

    async fn agent_capabilities(
        &self,
        _agent: &acp_core::model::AgentRef,
    ) -> Result<acp_core::model::CapabilitySet, PortError> {
        Ok(acp_core::model::CapabilitySet::empty())
    }

    async fn profile(
        &self,
        _id: &acp_core::model::AgentId,
    ) -> Result<Option<acp_core::model::AgentProfile>, PortError> {
        Ok(Some(
            acp_core::model::AgentProfile::try_new(
                acp_core::model::AgentId::new(SCRIPTED_AGENT).expect("agent id"),
                env!("CARGO_BIN_EXE_acp-remote"),
                Vec::new(),
                true,
            )
            .expect("profile"),
        ))
    }
}

/// 脚本化 agent selector（`agent.configure` 与快照 `agents` chunk 都按它断言）。
pub const SCRIPTED_AGENT: &str = "codex";

/// 快照 `sessions` 里断言的本地 agent 引用（与 `SCRIPTED_AGENT` 同值）。
pub const SCRIPTED_AGENT_ID: &str = "codex";

struct ScriptedEndpoint {
    sink: acp_core::ports::EventSink,
    parked: Arc<ParkedEvents>,
    /// Agent 侧会话标识：真实后端（`agent-host`）从 `session/new` 的响应里取，**不编造**；这里的
    /// `create` 分支给一个由会话标识派生的可预测取值（见 [`scripted_agent_session_id`]），`open`
    /// 分支为 `None`（与 `support::owner` 同款），`resume` 带回归属方的持久化取值。
    agent_session_id: Option<acp_core::model::AgentSessionId>,
}

/// 脚本：一条 `agent.message.delta` + 一条 `turn.completed`（真实 Agent 的产出形状）。
fn scripted_events(marker: &str) -> Vec<acp_core::model::EndpointEvent> {
    use acp_core::model::{EndpointEvent, EventKind, EventPayload, EventType, Timestamp, ViewJson};
    let at = Timestamp::new("2026-10-14T10:00:00.000Z").expect("规范时间戳");
    vec![
        EndpointEvent::new(
            EventKind::Delta,
            EventType::new("agent.message.delta").expect("合法事件类型"),
            EventPayload::new(
                ViewJson::new(&format!(
                    r#"{{"state":"running","text":"{marker}","messageId":"2ae1c07c-0000-4000-8000-0000000000aa"}}"#
                ))
                .expect("view 是 JSON object"),
                None,
            ),
            None,
            None,
            at.clone(),
        ),
        EndpointEvent::new(
            EventKind::State,
            EventType::new("turn.completed").expect("合法事件类型"),
            EventPayload::new(
                ViewJson::new(r#"{"state":"completed"}"#).expect("view 是 JSON object"),
                None,
            ),
            None,
            None,
            at,
        ),
    ]
}

/// 脚本化 `create` 建立的 ACP 会话标识（可预测：与 `support::owner` 同款）。
///
/// **它必须非空**：ACP 会话标识是恢复流程（`session.resume`）的输入，而它由 `session.create` 的**落账
/// 提交**（`StateChange::SettleCreate`）与终态同批落盘——返回 `None` 会让「ACP 会话标识已落库」这条
/// 断言（以及依赖它的恢复用例）观察不到任何东西。
fn scripted_agent_session_id(
    session: &acp_core::model::SessionId,
) -> acp_core::model::AgentSessionId {
    acp_core::model::AgentSessionId::new(&format!("acp-{}", &session.as_str()[..8]))
        .expect("会话标识非空、无 NUL、长度合规")
}

#[async_trait::async_trait]
impl SessionBackendFactory for ScriptedBackends {
    /// 替身不持有进程：终止本身是成功的空操作，但**记录**这一次调用——窗口期超限的失败关闭
    /// （终态 `uncertain` + 零会话行 + 「后端被要求终止」）只有在这里才能被协议级用例观察到。
    async fn terminate_agent(&self, session: &acp_core::model::SessionId) -> Result<(), PortError> {
        self.script
            .terminated
            .lock()
            .expect("用例内互斥量不会中毒")
            .push(session.as_str().to_owned());
        Ok(())
    }

    async fn create(
        &self,
        session: &acp_core::model::SessionId,
        _request: acp_core::model::CreateSessionRequest,
        sink: acp_core::ports::EventSink,
    ) -> Result<Box<dyn acp_core::ports::SessionEndpoint>, PortError> {
        self.script.creates.fetch_add(1, Ordering::SeqCst);
        self.script
            .created
            .lock()
            .expect("用例内互斥量不会中毒")
            .push(session.as_str().to_owned());
        // 窗口期事件**先于**失败注入：真实后端在 `session/new` 之后可能已经送出过 `session/update`，
        // 然后建会话才失败——那条路径必须一样零会话行、且一条事件行都不留。
        for index in 0..self.script.window_events() {
            sink.send(window_event(index));
        }
        if let Some(kind) = self.script.failure() {
            return Err(PortError::Unavailable(kind));
        }
        Ok(Box::new(ScriptedEndpoint::attach(
            sink,
            Arc::clone(&self.parked),
            Some(scripted_agent_session_id(session)),
        )))
    }

    async fn open(
        &self,
        _reference: acp_core::model::SessionReference,
        sink: acp_core::ports::EventSink,
    ) -> Result<Box<dyn acp_core::ports::SessionEndpoint>, PortError> {
        Ok(Box::new(ScriptedEndpoint::attach(
            sink,
            Arc::clone(&self.parked),
            None,
        )))
    }

    async fn resume(
        &self,
        _session: &acp_core::model::SessionId,
        request: acp_core::model::ResumeSessionRequest,
        sink: acp_core::ports::EventSink,
    ) -> Result<Box<dyn acp_core::ports::SessionEndpoint>, PortError> {
        Ok(Box::new(ScriptedEndpoint::attach(
            sink,
            Arc::clone(&self.parked),
            Some(request.agent_session_id.clone()),
        )))
    }
}

#[async_trait::async_trait]
impl acp_core::ports::SessionEndpoint for ScriptedEndpoint {
    fn reference(&self) -> acp_core::model::SessionReference {
        unreachable!("Sync 端到端用例不经后端取引用")
    }

    fn agent_session_id(&self) -> Option<&acp_core::model::AgentSessionId> {
        self.agent_session_id.as_ref()
    }

    async fn prompt(
        &self,
        request: acp_core::model::PromptRequest,
        _at: acp_core::model::Timestamp,
    ) -> Result<acp_core::ports::TurnAccepted, PortError> {
        let marker = prompt_marker(&request);
        self.parked.park(&marker, scripted_events(&marker));
        Ok(acp_core::ports::TurnAccepted {
            turn: acp_core::model::TurnId::new(&uuid_text()).expect("turn id 是规范 uuid 文本"),
        })
    }

    async fn cancel(&self, _turn: Option<acp_core::model::TurnId>) -> Result<(), PortError> {
        Ok(())
    }

    async fn set_mode(&self, _mode: &acp_core::model::ModeId) -> Result<(), PortError> {
        unreachable!("Sync 端到端用例不切模式")
    }

    async fn list_config(&self) -> Result<Vec<acp_core::model::ConfigOption>, PortError> {
        Ok(Vec::new())
    }

    async fn modes(&self) -> Result<acp_core::model::ModeState, PortError> {
        Ok(acp_core::model::ModeState::new(None, Vec::new()))
    }

    async fn set_config(
        &self,
        _id: &acp_core::model::ConfigOptionId,
        _value: acp_core::model::ConfigValue,
    ) -> Result<(), PortError> {
        unreachable!("Sync 端到端用例不改配置")
    }

    async fn resolve_interaction(
        &self,
        _interaction: &acp_core::model::InteractionId,
        _resolution: acp_core::model::InteractionResolution,
    ) -> Result<(), PortError> {
        unreachable!("Sync 端到端用例不解析交互")
    }

    async fn read_history(
        &self,
        _query: acp_core::ports::HistoryQuery,
    ) -> Result<acp_core::ports::HistoryPage, PortError> {
        unreachable!("Sync 端到端用例不读历史")
    }

    async fn close(&self) -> Result<(), PortError> {
        Ok(())
    }
}

/// prompt 正文里的 marker（唯一内容块是 `{"type":"text","text":"<marker>"}`）。
fn prompt_marker(request: &acp_core::model::PromptRequest) -> String {
    let block = request.content.first().expect("prompt 至少一个内容块");
    let value: Value = serde_json::from_str(block.as_str()).expect("prompt 内容块是 JSON object");
    value
        .get("text")
        .and_then(Value::as_str)
        .expect("prompt 内容块带 text")
        .to_owned()
}

/// `OwnerReachability` 替身：恒不可达（本用例不产生 imported 会话）。
struct UnreachableOwners;

impl server::sync::subscribe::OwnerReachability for UnreachableOwners {
    fn reachable(&self, _owner: &acp_core::model::NodeId) -> bool {
        false
    }
}

/// `DaemonControl` 替身（本 harness 不经 `daemon.*` 方法）。
struct UnusedDaemonControl;

#[async_trait::async_trait]
impl server::local_admin::DaemonControl for UnusedDaemonControl {
    async fn status(&self) -> server::local_admin::DaemonStatus {
        unreachable!("本 harness 不经 daemon.status")
    }

    async fn stop(&self, _grace_ms: Option<u64>) -> Result<(), server::local_admin::AdminError> {
        unreachable!("本 harness 不经 daemon.stop")
    }
}

/// 与 `storage-sqlite` 的目录创建同口径（Unix 上必须 owner-only，否则权限检查失败关闭）。
#[cfg(unix)]
fn create_owner_only_dir(path: &std::path::Path) {
    use std::os::unix::fs::DirBuilderExt as _;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .expect("创建 owner-only 数据目录");
}

#[cfg(not(unix))]
fn create_owner_only_dir(path: &std::path::Path) {
    std::fs::create_dir_all(path).expect("创建数据目录");
}

impl HostNode {
    /// 起一个真实的本机节点：真实 SQLite + 真实 `Authority` + 真实 listener（两个 path 家族）。
    pub async fn start(label: &str, host: &str) -> Self {
        let host_name = host.to_owned();
        let root = TempRoot::new(label);
        let data_dir = root.join("data");
        create_owner_only_dir(&data_dir);
        let config_text = format!(
            "[daemon]\ndata_dir = \"{}\"\npublic_origin = \"https://{host_name}\"\n\
             listen = \"127.0.0.1:0\"\n\
             [identity]\nkeystore = \"ephemeral\"\n\
             [dev_mode]\nenabled = true\nallow_plaintext = true\n\
             [agent]\ndefault = \"codex\"\ncodex = [\"{}\"]\n",
            data_dir.display().to_string().replace('\\', "/"),
            env!("CARGO_BIN_EXE_acp-remote").replace('\\', "/"),
        );
        let config = app::Config::from_toml(&config_text).expect("测试配置必须合法");
        let clock: Arc<dyn acp_core::ports::Clock> = Arc::new(app::clock::SystemClock::new());
        let ids: Arc<dyn acp_core::ports::IdGenerator> = Arc::new(app::compose::UuidIdGenerator);
        let store = Arc::new(
            SqliteStore::open(config.storage.clone(), &clock.now())
                .await
                .expect("打开真实存储"),
        );
        let sessions: Arc<dyn acp_core::ports::SessionStore> = store.clone();
        let deliveries: Arc<dyn acp_core::ports::RemoteDeliveryStore> = store.clone();
        let trust: Arc<dyn acp_core::ports::TrustStore> = store.clone();
        let audit: Arc<dyn acp_core::ports::AuditStore> = store.clone();
        let local_config: Arc<dyn acp_core::ports::LocalConfigStore> = store.clone();
        let attachments: Arc<dyn acp_core::ports::AttachmentStore> = store.clone();

        let entropy: Arc<dyn identity_auth::EntropySource> =
            Arc::new(identity_keystore::OsEntropy::new());
        let keystore: Arc<dyn identity_auth::IdentityKeystore> = Arc::new(
            identity_keystore::EphemeralKeystore::new(Arc::clone(&entropy)),
        );
        let identity = app::NodeIdentity::ensure(keystore.as_ref())
            .await
            .expect("本机身份就绪");
        let authority = Arc::new(identity_auth::Authority::new(
            identity.node_id().clone(),
            identity.key().clone(),
            Arc::clone(&keystore),
            Arc::clone(&entropy),
            Arc::clone(&clock),
        ));

        let parked = Arc::new(ParkedEvents::default());
        let script = Arc::new(ScriptedScript::default());
        let backends: Arc<dyn acp_core::ports::SessionBackendFactory> =
            Arc::new(ScriptedBackends {
                parked: Arc::clone(&parked),
                script: Arc::clone(&script),
            });
        let catalog: Arc<dyn acp_core::ports::AgentCatalog> = Arc::new(ScriptedCatalog);
        // 实时扇出的接收端先在这里取走，再装进发布分叉（与 `NetIngress::start` 的接线顺序一致）。
        let mut sync_publisher = server::sync::subscribe::fanout::SyncPublisher::channel();
        let sync_events = sync_publisher.take_receiver();
        let (publisher, event_queue) =
            app::compose::forked_publisher_with_sync(Some(sync_publisher));
        let broker = Arc::new(acp_core::broker::Broker::new(
            acp_core::broker::BrokerDeps {
                store: Arc::clone(&sessions),
                deliveries: Arc::clone(&deliveries),
                backends,
                trust: Arc::clone(&trust),
                publisher,
                clock: Arc::clone(&clock),
                ids: Arc::clone(&ids),
                audit: Some(Arc::clone(&audit)),
            },
            acp_core::broker::BrokerConfig::default(),
        ));
        let core = Arc::new(UseCases::new(UseCaseDeps {
            broker: Arc::clone(&broker),
            store: sessions,
            deliveries,
            trust,
            audit: Arc::clone(&audit),
            config: local_config,
            attachments,
            catalog,
            clock: Arc::clone(&clock),
            ids: Arc::clone(&ids),
        }));

        // —— 网络接入面：与组合根同一批装配点（含 Sync 的 path 注册与实时扇出）——
        let mut listener =
            server::transport::net::NetListener::bind(app::daemon::net_config(&config))
                .await
                .expect("绑定 loopback listener");
        let node_link = app::daemon::register_node_link_paths(
            &mut listener,
            &Arc::clone(&core),
            &Arc::clone(&authority),
            &server::node_link::NodeLinkConfig {
                public_origin: config.public_origin.clone(),
                ..server::node_link::NodeLinkConfig::default()
            },
        )
        .expect("注册 Node Link path");
        let sync = app::daemon::register_sync_paths(
            &mut listener,
            &Arc::clone(&core),
            &Arc::clone(&authority),
            &Arc::clone(&broker),
            &app::daemon::sync_config(&config),
            config.public_origin.clone(),
            Arc::new(UnreachableOwners),
            sync_events,
        )
        .expect("注册 Sync path");
        let addr = listener.local_addrs()[0];
        let (shutdown, signal) = server::transport::net::Shutdown::channel();
        let mut tasks: Vec<(&'static str, tokio::task::JoinHandle<()>)> = Vec::new();
        tasks.push((
            "listener",
            tokio::spawn({
                let signal = signal.clone();
                async move {
                    let _ = listener.serve(signal).await;
                }
            }),
        ));
        tasks.push((
            "node_link_fanout",
            tokio::spawn({
                let resource = Arc::clone(node_link.resource());
                let signal = signal.clone();
                async move { resource.dispatch(event_queue, signal).await }
            }),
        ));
        tasks.push((
            "node_link_commands",
            tokio::spawn({
                let command = Arc::clone(node_link.command());
                let signal = signal.clone();
                async move { command.dispatch(signal).await }
            }),
        ));
        // Sync 的实时扇出（真实 `fanout::dispatch`）。
        let sync_registry = Arc::clone(sync.registry());
        // Sync 命令面的终态观察者（与 `app::daemon::NetIngress::start` 同一条接线）：
        // 没有它，mutation 的 `accepted` 之后就永远收不到终态。
        let sync_command = Arc::clone(sync.command());
        tasks.push((
            "sync_commands",
            tokio::spawn({
                let signal = signal.clone();
                async move { sync_command.dispatch(signal).await }
            }),
        ));
        let (sync_route, fanout_registry, sync_events) = sync.into_fanout();
        let sync_reader = sync_route.reader();
        tasks.push((
            "sync_fanout",
            tokio::spawn(async move {
                server::sync::subscribe::fanout::dispatch(
                    fanout_registry,
                    sync_route,
                    sync_reader,
                    sync_events,
                    signal,
                )
                .await
            }),
        ));

        // —— 本地管理面：与本地通道同一个处理器；撤销缝接**真实** Sync 连接表 ——
        let router = Arc::new(LocalAdminRouter::new(LocalAdminDeps {
            daemon: Arc::new(UnusedDaemonControl),
            core: Arc::clone(&core),
            keystore: Arc::clone(&keystore),
            audit,
            clock: Arc::clone(&clock),
            pairing: Arc::new(server::local_admin::PairingSessions::new(
                Arc::clone(&authority),
                config.public_origin.clone(),
                config.node_name.clone(),
                Arc::new(app::compose::NodeLinkCloser::new(
                    Arc::clone(node_link.command()),
                    Arc::clone(&sync_registry),
                )),
            )),
        }));

        Self {
            root,
            store,
            core,
            broker,
            authority,
            router,
            addr,
            sync_registry,
            parked,
            script,
            host_name,
            shutdown,
            tasks,
            _temp: (),
        }
    }

    /// 真实 listener 的地址。
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// 数据目录（注册 workspace alias 时需要一个真实存在的目录）。
    pub fn data_dir(&self) -> std::path::PathBuf {
        self.root.join("data")
    }

    /// 本机 node id（`auth.client_hello.hostId` 与 claim 的 `hostId`）。
    pub fn host_id(&self) -> String {
        self.authority.local_node().as_str().to_owned()
    }

    /// canonical origin（`https://<host>`；与配置的 `daemon.public_origin` 同值）。
    pub fn origin(&self) -> String {
        format!("https://{}", self.host_name)
    }

    /// `daemon.public_origin` 的 authority 部分（`Host` 头与 Origin 头都用它）。
    pub fn host_name(&self) -> &str {
        &self.host_name
    }

    /// 本机公钥（`statusResponse.host.hostPublicKey` 的来源）。
    pub async fn public_key(&self) -> identity_auth::PeerPublicKey {
        self.authority.node_public_key().await.expect("本机公钥")
    }

    /// 当前事件库 head（`{serverEpoch, globalSequence}`）。
    pub async fn head(&self) -> acp_core::model::GlobalCursor {
        self.broker.head().await.expect("事件库水位")
    }

    /// 用例面（`session.list`/`session.create` 等真实入口）。
    pub fn core(&self) -> &Arc<UseCases> {
        &self.core
    }

    /// 真实 broker（提交节点级事件与读重放窗口）。
    pub fn broker(&self) -> &Arc<Broker> {
        &self.broker
    }

    /// 脚本端点暂存的事件。
    pub fn parked(&self) -> &Arc<ParkedEvents> {
        &self.parked
    }

    /// 脚本化后端的行为脚本（失败注入、窗口期事件注入、后端调用计数）。
    pub fn script(&self) -> &Arc<ScriptedScript> {
        &self.script
    }

    /// 真实身份状态机（配对码/secret 的内存态观测口：`has_secret`、`consume_pairing_code`）。
    pub fn authority(&self) -> &Arc<Authority> {
        &self.authority
    }

    /// 真实存储（用例对持久化事实做独立只读复核）。
    pub fn store(&self) -> &Arc<SqliteStore> {
        &self.store
    }

    /// 调用一个本地管理方法（与本地通道同一个处理器；失败即 panic 出可读消息）。
    pub async fn admin(&self, method: server::local_admin::Method, params: Value) -> Value {
        let response = self.call(method, params).await;
        match response.outcome() {
            server::local_admin::AdminOutcome::Success { result } => Value::Object(result.clone()),
            server::local_admin::AdminOutcome::Failure { error } => panic!(
                "`{}` 期望成功，实际 {}：{}",
                method.as_str(),
                error.code().as_str(),
                error.message()
            ),
        }
    }

    /// 调用一个本地管理方法并返回原始响应。
    pub async fn call(
        &self,
        method: server::local_admin::Method,
        params: Value,
    ) -> server::local_admin::AdminResponse {
        let payload = serde_json::to_vec(&serde_json::json!({
            "v": 1,
            "id": uuid_mark(),
            "method": method.as_str(),
            "params": params,
        }))
        .expect("请求信封可序列化");
        let request: server::local_admin::AdminRequest =
            server::local_admin::decode_request(&payload).expect("信封合法");
        use server::local_admin::LocalAdminHandler as _;
        self.router.handle(request).await
    }

    /// 提交一条**真实**的节点级事件（`agent.connected`/`agent.disconnected`，§10.3）。
    ///
    /// 这是本 harness 里唯一不由 Agent 子进程产生的事件来源：`commit_node_event` 会像生产一样
    /// 分配 `global_sequence`、落库并触发提交后发布（因此实时扇出能看到它）。
    pub async fn emit_agent_lifecycle(&self, agent: &str, state: &str) {
        use acp_core::model::{
            EndpointEvent, EventKind, EventPayload, EventType, Timestamp, ViewJson,
        };
        let at = Timestamp::new("2026-10-14T10:00:00.000Z").expect("规范时间戳");
        let event = EndpointEvent::new(
            EventKind::State,
            EventType::new(if state == "connected" {
                "agent.connected"
            } else {
                "agent.disconnected"
            })
            .expect("合法事件类型"),
            EventPayload::new(
                ViewJson::new(&format!(r#"{{"agentId":"{agent}","state":"{state}"}}"#))
                    .expect("view 是 JSON object"),
                None,
            ),
            None,
            None,
            at,
        );
        self.broker
            .commit_node_event(event)
            .await
            .expect("节点级事件提交");
    }

    /// 删除库里最早若干条 `owned_event` 行（**原始 SQL**，只在构造测试前提时使用）。
    ///
    /// `cursor_expired` 的判定是「提交的 cursor 早于仍可重放的窗口下界」，而下界就是
    /// `MIN(global_sequence)`——首个事件一旦写下就永远是 1，任何合法 cursor 都不满足判据。保留期
    /// 清理只删「已过期」的行（`expires_at <= now`，最近写入的永远不会），因此产品路径在本测试的
    /// 时间尺度上造不出这个形状。这与 `support::owner::OwnerNode::overwrite_persisted_workspace_cwd`
    /// 同属**测试替身级介入**：产品写路径永远产生不了这个形状，只用它把「早于保留窗口」这一分支的
    /// 判据变成真。
    pub async fn drop_earliest_events(&self, count: u64) {
        let config = storage_sqlite::migrate::StorageConfig::new(self.data_dir());
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(config.database_path())
            .create_if_missing(false)
            .busy_timeout(Duration::from_secs(10));
        let mut connection: sqlx::SqliteConnection = sqlx::Connection::connect_with(&options)
            .await
            .expect("打开同一 data_dir 的原始 SQL 连接");
        let deleted = sqlx::query(
            "DELETE FROM owned_event WHERE global_sequence IN \
             (SELECT global_sequence FROM owned_event ORDER BY global_sequence ASC LIMIT ?1)",
        )
        .bind(i64::try_from(count).expect("count 可放进 i64"))
        .execute(&mut connection)
        .await
        .expect("删除最早的 owned_event 行")
        .rows_affected();
        assert_eq!(
            deleted, count,
            "必须恰好删除 {count} 行最早事件，否则「早于保留窗口」的前提没有造出来"
        );
        drop(connection);
    }

    /// Sync 的活动连接数（顶替/撤销的观测口：不只依赖对端可见的 close 帧）。
    pub fn active_sync_connections(&self) -> usize {
        self.sync_registry.active()
    }

    /// 关闭接入面并释放存储（用例末尾调用）。
    pub async fn stop(self) {
        let Self {
            root,
            store,
            core,
            broker,
            authority,
            router,
            addr: _,
            sync_registry,
            parked,
            script,
            host_name: _,
            shutdown,
            tasks,
            _temp,
        } = self;
        let _ = (&store, &sync_registry, &parked, &script);
        shutdown.trigger();
        for (name, mut handle) in tasks {
            match tokio::time::timeout(Duration::from_secs(10), &mut handle).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => panic!("接入面任务 `{name}` 异常结束：{error}"),
                Err(_) => {
                    handle.abort();
                    let _ = handle.await;
                    panic!("接入面任务 `{name}` 未在期限内退出");
                }
            }
        }
        drop(router);
        drop(core);
        drop(broker);
        drop(authority);
        drop(store);
        drop(parked);
        drop(script);
        drop(sync_registry);
        drop(root);
    }
}

impl ScriptedEndpoint {
    /// 建端点并把它挂上暂存表的 sink（否则 `ParkedEvents::release` 没有出口）。
    fn attach(
        sink: acp_core::ports::EventSink,
        parked: Arc<ParkedEvents>,
        agent_session_id: Option<acp_core::model::AgentSessionId>,
    ) -> Self {
        parked.attach(&sink);
        Self {
            sink,
            parked,
            agent_session_id,
        }
    }
}
