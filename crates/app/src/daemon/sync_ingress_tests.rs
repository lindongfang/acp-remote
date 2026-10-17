//! Sync 接入面的组合根接线用例（`sync-server-connection` 任务 6.1 与 6.4）。
//!
//! 这些用例**经真实组合根**跑：`NetIngress::start` —— 与 `crate::daemon::run` 是同一个装配点，因此
//! 「Sync 的 path 是否注册」「用的是哪个 subprotocol」「配对端点的安全响应头是否交给接入层」「设备撤销
//! 关的是哪张连接表」都在这里被钉住；只调 `register_sync_paths` 会让「组合根忘了接线」这一类回归逃掉。
//!
//! 对端一律是手写的极简客户端（HTTP 请求 + WebSocket 帧，与 `server::sync::conn` 的用例同款做法）：
//! 本 crate 没有 `sync-protocol` 依赖，也不该为了测试引入一个——wire 形状按
//! `schemas/sync/v1/` 与 `docs/SYNC_PROTOCOL.md` §4/§8 手写，签名/转录仍只经 `identity-auth` 的公开面。
//!
//! 设备信任行不走过配对 HTTP（HMAC）面：本用例的断言对象是**连接与撤销**，不是配对；设备经
//! `identity_auth::Authority` 的公开状态机 + `core::use_cases` 落库（与 `device.pair.begin/confirm`
//! 同一条写入路径，只是不经过本地通道信封）。

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use acp_core::model::{
    Actor, DeviceId, GrantSet, NodeId, Nonce, PairingId, PeerIdentity, PeerPublicKey, ScopeSet,
};
use acp_core::ports::{AuditQuery, TrustRecordRef};
use acp_core::use_cases::UseCases;
use base64::Engine as _;
use server::sync::subscribe::fanout::SyncPublisher;

use crate::compose::forked_publisher_with_sync;
use identity_auth::{
    Authority, CanonicalOrigin, ChallengeId, ClaimFields, ClaimKindFields, ClaimOutcome,
    ClientKind, FeatureList, P1363Signature, PairingDecision, PairingProof, PairingSpec,
    RequestedCapabilities, SyncDeviceProof, SyncPairingProof, timestamp_from_unix_seconds,
};
use p256::ecdsa::signature::Signer as _;
use serde_json::{Value, json};
use server::local_admin::{
    AdminOutcome, AdminRequest, DaemonControl, DaemonStatus, LocalAdminDeps, LocalAdminHandler,
    LocalAdminRouter, PairingSessions, decode_request,
};
use server::node_link::{WS_PATH as NODE_LINK_WS_PATH, WS_SUBPROTOCOL as NODE_LINK_WS_SUBPROTOCOL};
use server::sync::conn::{
    SUPPORTED_FEATURES, WS_PATH as SYNC_WS_PATH, WS_SUBPROTOCOL as SYNC_WS_SUBPROTOCOL,
};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;

use crate::compose::Composition;
use crate::config::Config;
use crate::daemon::NetIngress;

/// listener 的 host 名（`daemon.public_origin` 的 authority，同时也是设备配对的绑定与请求的 `Host`）。
///
/// `pub(super)`：同级的 `ui_hosting_tests`（静态面用例）复用同一个 host 与同一套 HTTP 助手，因此
/// 「请求怎么发」只有一处实现。
pub(super) const HOST: &str = "sync-ingress.example";

/// 一次读写的预算（真实 loopback 上远小于它；超时即失败而不是挂住）。
pub(super) const IO_TIMEOUT: Duration = Duration::from_secs(10);

/// 设备密钥的标量（**只用于测试**：仓库不提交任何私钥材料，签名原语仍取自 `p256`）。
const DEVICE_SCALAR: [u8; 32] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7,
];

/// 客户端的 nonce 素材（32 字节；wire 上按无填充 base64url 传）。
const CLIENT_NONCE: [u8; 32] = [0x3c; 32];

/// 入站帧（只覆盖本用例会遇到的 opcode）。
#[derive(Debug)]
enum Frame {
    Text(String),
    Close(u16),
    Control,
}

/// 消息标识的计数器（每条信封都要有；不同的 id 便于把失败定位到具体一条）。
static MESSAGE_IDS: AtomicU32 = AtomicU32::new(1);

/// 一条 12 位十六进制尾组的 uuid 文本（`DeviceId`/`PairingId`/`messageId` 都按 canonical uuid 校验）。
fn uuid_text() -> String {
    format!(
        "00000000-0000-4000-8000-{:012}",
        MESSAGE_IDS.fetch_add(1, Ordering::SeqCst)
    )
}

/// 无填充 base64url（协议里的 nonce/签名字段）。
fn base64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时钟不早于 Unix 纪元")
        .as_secs()
}

/// 临时数据目录（用例结束即删；字段声明顺序保证它最后析构）。
struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "acpr-sync-ingress-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 与 `storage-sqlite` 的目录创建同口径（Unix 上必须 owner-only，否则 `strict_permissions` 失败关闭）。
#[cfg(unix)]
fn create_owner_only_dir(path: &Path) {
    use std::os::unix::fs::DirBuilderExt as _;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .expect("创建 owner-only 数据目录");
}

#[cfg(not(unix))]
fn create_owner_only_dir(path: &Path) {
    std::fs::create_dir_all(path).expect("创建数据目录");
}

/// 真实组合根 + 真实 loopback listener。
///
/// 字段的**声明顺序即析构顺序**：持有存储的 `composition` 先释放，临时目录最后删除。
///
/// `pub(super)`：同级的 `ui_hosting_tests` 经它复用**同一个装配点**（`NetIngress::start`）——静态面与
/// 协议面因此在生产路径与用例里不会分叉。
pub(super) struct Harness {
    pub(super) addr: SocketAddr,
    authority: Arc<Authority>,
    core: Arc<UseCases>,
    router: Arc<LocalAdminRouter>,
    ingress: Option<NetIngress>,
    /// 组合根本身（与真实 Daemon 一样在整个用例期间存活：它持有存储、Agent host 与事件缝）。
    _composition: Composition,
    _temp: TempDir,
}

impl Harness {
    pub(super) async fn start(label: &str) -> Self {
        let temp = TempDir::new(label);
        let data_dir = temp.0.join("data");
        create_owner_only_dir(&data_dir);
        let config = Config::from_toml(&format!(
            "[daemon]\ndata_dir = {dir:?}\npublic_origin = \"https://{HOST}\"\n\
             listen = \"127.0.0.1:0\"\nshutdown_grace_ms = 1000\n\
             [identity]\nkeystore = \"ephemeral\"\n\
             [dev_mode]\nenabled = true\nallow_plaintext = true\n",
            dir = data_dir.display()
        ))
        .expect("测试配置必须合法");
        let mut sync_publisher = SyncPublisher::channel();
        let sync_events = sync_publisher.take_receiver();
        let (publisher, event_queue) = forked_publisher_with_sync(Some(sync_publisher));
        let composition = Composition::assemble(config, publisher)
            .await
            .expect("组合根装配");
        let ingress = NetIngress::start(&composition, event_queue, sync_events)
            .await
            .expect("网络接入面必须启动");
        let addr = ingress
            .listen()
            .first()
            .expect("listener 至少有一个地址")
            .parse::<SocketAddr>()
            .expect("listener 地址可解析");
        // 与 `crate::daemon::run` 同一个撤销缝接线点（`NetIngress::connection_closer`）：因此「设备撤销
        // 关的是哪张连接表」在生产路径与用例里不会分叉。
        let closer = Arc::new(ingress.connection_closer());
        let router = Arc::new(LocalAdminRouter::new(LocalAdminDeps {
            daemon: Arc::new(UnusedDaemonControl),
            core: Arc::clone(composition.use_cases()),
            keystore: Arc::clone(composition.keystore()),
            audit: composition.audit_store(),
            clock: Arc::clone(composition.clock()),
            pairing: Arc::new(PairingSessions::new(
                Arc::clone(composition.authority()),
                composition.config().public_origin.clone(),
                closer,
            )),
        }));
        Self {
            addr,
            authority: Arc::clone(composition.authority()),
            core: Arc::clone(composition.use_cases()),
            router,
            ingress: Some(ingress),
            _composition: composition,
            _temp: temp,
        }
    }

    fn origin(&self) -> String {
        format!("https://{HOST}")
    }

    /// 组合根持有者：`AppDaemonControl::new` 需要它（静态面用例断言 `daemon.status` 的字段来源）。
    pub(super) fn composition(&self) -> &Composition {
        &self._composition
    }

    /// 接入面句柄（`NetIngress::ui_build_version`/`listen` 的读取点）。
    pub(super) fn ingress(&self) -> &NetIngress {
        self.ingress.as_ref().expect("接入面仍在")
    }

    /// Sync 的活动连接数（撤销断言用它，避免只看对端可见的 close 帧）。
    fn active_sync_connections(&self) -> usize {
        self.ingress
            .as_ref()
            .expect("接入面仍在")
            .sync_registry()
            .active()
    }

    pub(super) async fn stop(mut self) {
        if let Some(ingress) = self.ingress.take() {
            ingress.trigger_shutdown();
            ingress
                .wait_stopped(std::time::Instant::now() + Duration::from_secs(10))
                .await;
        }
    }
}

/// 仅供本文件的最小 `DaemonControl`（这些用例不经 `daemon.*`）。
struct UnusedDaemonControl;

#[async_trait::async_trait]
impl DaemonControl for UnusedDaemonControl {
    async fn status(&self) -> DaemonStatus {
        unreachable!("本文件的用例不经 daemon.status")
    }

    async fn stop(&self, _grace_ms: Option<u64>) -> Result<(), server::local_admin::AdminError> {
        unreachable!("本文件的用例不经 daemon.stop")
    }
}

// ---------------------------------------------------------------------------------------------
// HTTP 客户端（配对 POST 面）
// ---------------------------------------------------------------------------------------------

/// 一条已读全的 HTTP 响应（状态码、头、body 文本）。
pub(super) struct RawResponse {
    pub(super) status: u16,
    pub(super) headers: Vec<(String, String)>,
    pub(super) body: String,
}

impl RawResponse {
    pub(super) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// §7.1 的四个安全响应头（缺失即失败）。
    fn assert_security_headers(&self) {
        for (name, value) in [
            ("cache-control", "no-store"),
            ("pragma", "no-cache"),
            ("referrer-policy", "no-referrer"),
            ("x-content-type-options", "nosniff"),
        ] {
            assert_eq!(
                self.header(name),
                Some(value),
                "配对 HTTP 的每个响应都必须带 `{name}`（§7.1）"
            );
        }
    }
}

/// 发一条 POST 并读全响应（`content_type` 为 `None` 时不带 `Content-Type`）。
async fn post(
    addr: SocketAddr,
    path: &str,
    content_type: Option<&str>,
    body: &[u8],
) -> RawResponse {
    let content_type_line = content_type
        .map(|value| format!("Content-Type: {value}\r\n"))
        .unwrap_or_default();
    let head = format!(
        "POST {path} HTTP/1.1\r\nHost: {HOST}\r\nConnection: close\r\n{content_type_line}\
         Content-Length: {}\r\n\r\n",
        body.len()
    );
    let mut stream = tokio::time::timeout(IO_TIMEOUT, TcpStream::connect(addr))
        .await
        .expect("连接 loopback 超时")
        .expect("连接 loopback");
    let mut request = head.into_bytes();
    request.extend_from_slice(body);
    tokio::time::timeout(IO_TIMEOUT, stream.write_all(&request))
        .await
        .expect("写请求超时")
        .expect("写请求");
    let raw = tokio::time::timeout(IO_TIMEOUT, async {
        let mut raw = Vec::new();
        let _ = stream.read_to_end(&mut raw).await;
        raw
    })
    .await
    .expect("读响应超时");
    parse_response(&raw)
}

pub(super) fn parse_response(raw: &[u8]) -> RawResponse {
    let text = String::from_utf8_lossy(raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((text.as_str(), ""));
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or_else(|| panic!("响应状态行无法解析：{status_line:?}"));
    let headers = lines
        .filter_map(|line| {
            line.split_once(':')
                .map(|(key, value)| (key.to_owned(), value.trim().to_owned()))
        })
        .collect();
    RawResponse {
        status,
        headers,
        body: body.to_owned(),
    }
}

// ---------------------------------------------------------------------------------------------
// WebSocket 客户端（Sync 连接面）
// ---------------------------------------------------------------------------------------------

/// 极简 WebSocket 客户端：升级 + 文本帧 + close 帧（不做扩展与分片）。
pub(super) struct WsClient {
    stream: TcpStream,
    buffer: Vec<u8>,
}

impl WsClient {
    /// 尝试升级：`Err(升级响应头)` 表示被接入层拒绝（非 101）。
    pub(super) async fn try_upgrade(
        addr: SocketAddr,
        path: &str,
        subprotocol: &str,
        origin: Option<&str>,
    ) -> Result<Self, String> {
        let mut stream = tokio::time::timeout(IO_TIMEOUT, TcpStream::connect(addr))
            .await
            .expect("连接 loopback 超时")
            .expect("连接 loopback");
        let key = base64::engine::general_purpose::STANDARD.encode([0x2a_u8; 16]);
        let origin_line = origin
            .map(|origin| format!("Origin: {origin}\r\n"))
            .unwrap_or_default();
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {HOST}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Protocol: {subprotocol}\r\n{origin_line}\r\n"
        );
        tokio::time::timeout(IO_TIMEOUT, stream.write_all(request.as_bytes()))
            .await
            .expect("写升级请求超时")
            .expect("写升级请求");
        let mut buffer = Vec::new();
        let head = read_head(&mut stream, &mut buffer).await?;
        if !head.starts_with("HTTP/1.1 101 ") {
            return Err(head);
        }
        assert!(
            head.to_ascii_lowercase()
                .contains(&format!("sec-websocket-protocol: {subprotocol}")),
            "升级响应必须回显约定的 subprotocol，实际：{head}"
        );
        Ok(Self { stream, buffer })
    }

    /// 完成一次成功升级（升级失败即失败）。
    async fn upgrade(addr: SocketAddr, path: &str, subprotocol: &str, origin: &str) -> Self {
        Self::try_upgrade(addr, path, subprotocol, Some(origin))
            .await
            .unwrap_or_else(|head| panic!("WebSocket 升级必须成功，实际响应：{head}"))
    }

    async fn send_text(&mut self, text: &str) {
        let payload = text.as_bytes();
        let mut frame = Vec::with_capacity(payload.len() + 14);
        frame.push(0x81);
        let mask = [0x21_u8, 0x43, 0x65, 0x87];
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

    /// 下一条帧（`None` = 对端关闭了流且没有 close 帧）。
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
            0x2 => Frame::Control,
            0x8 => Frame::Close(
                payload
                    .get(..2)
                    .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
                    .unwrap_or(0),
            ),
            _ => Frame::Control,
        })
    }

    async fn fill(&mut self, needed: usize) -> Option<()> {
        while self.buffer.len() < needed {
            let mut chunk = [0u8; 16 * 1024];
            let read = tokio::time::timeout(IO_TIMEOUT, self.stream.read(&mut chunk))
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

    /// 下一条 JSON 消息（跳过控制帧；遇到 close 帧即失败）。
    async fn next_message(&mut self) -> Value {
        loop {
            match self.next_frame().await {
                Some(Frame::Text(text)) => {
                    return serde_json::from_str(&text).expect("服务端消息必须是 JSON");
                }
                Some(Frame::Close(code)) => panic!("对端在消息之前关闭了连接：{code}"),
                Some(Frame::Control) => continue,
                None => panic!("对端在没有消息的情况下结束了连接"),
            }
        }
    }

    /// 期待下一条消息的类型并返回整条消息。
    async fn expect_type(&mut self, message_type: &str) -> Value {
        let message = self.next_message().await;
        assert_eq!(
            message["type"],
            json!(message_type),
            "消息类型不符：{message}"
        );
        message
    }

    /// 读到 close 帧（中途的文本消息丢弃），返回 close code。
    async fn expect_close(&mut self) -> u16 {
        loop {
            match self.next_frame().await {
                Some(Frame::Text(_)) | Some(Frame::Control) => continue,
                Some(Frame::Close(code)) => return code,
                None => panic!("对端在没有 close 帧的情况下结束了连接"),
            }
        }
    }
}

/// 读到 HTTP 响应头结束（`\r\n\r\n`）；流在读到响应头之前就结束 → `Err`。
async fn read_head(stream: &mut TcpStream, buffer: &mut Vec<u8>) -> Result<String, String> {
    loop {
        if let Some(head_end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
            buffer.drain(..head_end + 4);
            return Ok(head);
        }
        let mut chunk = [0u8; 1024];
        let read = tokio::time::timeout(IO_TIMEOUT, stream.read(&mut chunk))
            .await
            .expect("读升级响应超时")
            .expect("读升级响应");
        if read == 0 {
            return Err(String::from_utf8_lossy(buffer).into_owned());
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

/// 一条信封（认证前的消息省略连接字段）。
fn envelope(message_type: &str, body: Value) -> String {
    json!({
        "protocolVersion": 1,
        "type": message_type,
        "messageId": uuid_text(),
        "body": body,
    })
    .to_string()
}

/// 握手的前两步：`auth.client_hello`（声明 `declared` 条 supportedFeatures）→ `auth.server_challenge`。
///
/// `declared` 由调用方给：协商是**交集**（`selected = 客户端 ∩ 服务端`），因此少报一条就是「这一条协商
/// 不到」——这条路径在真实握手上可达。
async fn hello_and_challenge(
    client: &mut WsClient,
    host: &NodeId,
    device: &DeviceId,
    declared: Vec<&str>,
) -> Value {
    let client_nonce = base64url(&CLIENT_NONCE);
    client
        .send_text(&envelope(
            "auth.client_hello",
            json!({
                "minProtocolVersion": 1,
                "maxProtocolVersion": 1,
                "hostId": host.as_str(),
                "deviceId": device.as_str(),
                "clientKind": "pwa",
                "clientNonce": client_nonce,
                "supportedFeatures": declared,
                "requiredFeatures": [],
            }),
        ))
        .await;
    client.expect_type("auth.server_challenge").await
}

/// `auth.server_challenge` 的 `connectionId`（认证后的消息必须回填它 + `connectionSequence`）。
fn challenge_connection_id(challenge: &Value) -> String {
    challenge["body"]["connectionId"]
        .as_str()
        .expect("connectionId")
        .to_owned()
}

/// `auth.server_challenge` 的 `selectedFeatures`（本次协商结果）。
fn selected_feature_ids(challenge: &Value) -> Vec<String> {
    challenge["body"]["selectedFeatures"]
        .as_array()
        .expect("selectedFeatures")
        .iter()
        .map(|feature| feature.as_str().expect("feature id 是文本").to_owned())
        .collect()
}

/// 用 `auth.server_challenge` 的 body 提交 `auth.client_proof`，交回最后一条消息
/// （`auth.authenticated` 或 `error`）。
async fn prove(
    client: &mut WsClient,
    host: &NodeId,
    device: &DeviceId,
    origin: &str,
    signing: &p256::ecdsa::SigningKey,
    challenge: &Value,
) -> Value {
    let body = challenge["body"].clone();
    let connection_id = challenge_connection_id(challenge);
    let input = SyncDeviceProof {
        host_id: host.clone(),
        device_id: device.clone(),
        canonical_origin: CanonicalOrigin::parse(origin).expect("canonical origin"),
        client_nonce: Nonce::new(&base64url(&CLIENT_NONCE)).expect("nonce"),
        server_nonce: Nonce::new(body["serverNonce"].as_str().expect("serverNonce"))
            .expect("nonce"),
        connection_id: ChallengeId::parse(&connection_id).expect("connectionId 是 uuid"),
        negotiated_features: FeatureList::new(selected_feature_ids(challenge)),
    };
    let transcript = input.transcript().expect("transcript 可装配");
    let signature: p256::ecdsa::Signature = signing.sign(&transcript);
    let proof = P1363Signature::try_from_bytes(signature.to_bytes().as_slice())
        .expect("P-256 签名是 64 字节");
    client
        .send_text(&envelope(
            "auth.client_proof",
            json!({
                "connectionId": connection_id,
                "deviceId": device.as_str(),
                "deviceProof": proof.to_base64url(),
            }),
        ))
        .await;
    client.next_message().await
}

/// 设备侧四消息握手：`auth.client_hello` → `auth.server_challenge` → `auth.client_proof` →
/// `auth.authenticated`。返回最后一条消息（`auth.authenticated` 或 `error`）。
async fn authenticate(
    client: &mut WsClient,
    host: &NodeId,
    device: &DeviceId,
    origin: &str,
    signing: &p256::ecdsa::SigningKey,
) -> Value {
    let challenge = hello_and_challenge(client, host, device, SUPPORTED_FEATURES.to_vec()).await;
    prove(client, host, device, origin, signing, &challenge).await
}

/// 本机用户登记、认领并批准一次设备配对，落库出 `active` 的设备信任行。
///
/// 这是 `device.pair.begin → claim → confirm` 的写入路径（同一批用例面与状态机入口），只是不经过本地
/// 通道信封与配对 HTTP 的 HMAC 面——本用例要验的是**连接与撤销**，不是配对。
async fn approve_device(
    core: &UseCases,
    authority: &Authority,
    device: &DeviceId,
    public: &PeerPublicKey,
    origin: &CanonicalOrigin,
) -> PairingId {
    let pairing_id = PairingId::new(&uuid_text()).expect("pairing id 是 canonical uuid");
    let expires_at = timestamp_from_unix_seconds(unix_now() + 60).expect("60 秒后是合法时间戳");
    let draft = authority
        .begin_pairing(
            &pairing_id,
            &PairingSpec::Device {
                canonical_origin: origin.clone(),
            },
            // 设备配对不申请任何授权词汇（`pairing-model` 删除设备权限层）：两侧集合都必须为空，
            // 否则 `RequestedCapabilities::matches_target(Device)` 以 `CapabilityKindMismatch` 拒绝。
            &RequestedCapabilities::empty(),
            Some("Test Phone"),
            &authority.now(),
            &expires_at,
        )
        .expect("登记配对");
    core.create_pairing(&Actor::LocalCli, draft.record.clone())
        .await
        .expect("配对落库");
    let client_nonce = Nonce::new(&base64url(&CLIENT_NONCE)).expect("nonce");
    let mut fields = ClaimFields {
        pairing: pairing_id.clone(),
        peer: PeerIdentity::Device(device.clone()),
        kind: ClaimKindFields::Device {
            client_kind: ClientKind::Pwa,
        },
        display_name: "Test Phone".to_owned(),
        host_binding: origin.as_str().to_owned(),
        public_key: public.clone(),
        client_nonce: client_nonce.clone(),
        requested: RequestedCapabilities::empty(),
        proof: PairingProof::try_from_bytes(&[0u8; 32]).expect("32 字节占位 proof"),
    };
    fields.proof = SyncPairingProof {
        host_id: authority.local_node().clone(),
        device_id: device.clone(),
        pairing_id: pairing_id.clone(),
        pairing_expires_at: expires_at.clone(),
        canonical_origin: origin.clone(),
        device_public_key: public.clone(),
        client_nonce,
        device_name: "Test Phone".to_owned(),
        client_kind: ClientKind::Pwa,
    }
    .hmac(&draft.secret)
    .expect("claim 证明");
    let ClaimOutcome::Claimed(claimed) = authority
        .verify_claim(&draft.record, None, &fields)
        .expect("认领判定")
    else {
        panic!("首次认领必须成功");
    };
    core.claim_pairing(
        &Actor::PairingClaimant {
            pairing: pairing_id.clone(),
        },
        claimed.to_claim().expect("认领写集"),
    )
    .await
    .expect("认领落库");
    let record = core
        .pairing(
            &Actor::PairingClaimant {
                pairing: pairing_id.clone(),
            },
            &pairing_id,
        )
        .await
        .expect("读配对")
        .expect("配对必须已落库");
    let settlement = authority
        .settle(
            &record,
            &PairingDecision::Approve {
                granted_scopes: ScopeSet::empty(),
                granted_grants: GrantSet::empty(),
            },
            &authority.now(),
        )
        .expect("批准配对");
    let reference = core
        .settle_pairing(&Actor::LocalCli, &pairing_id, settlement)
        .await
        .expect("落定落库");
    assert_eq!(
        reference,
        TrustRecordRef::Device(device.clone()),
        "设备配对落定必须创建设备信任行"
    );
    pairing_id
}

/// 经**真实本地管理处理器**撤销设备（与 `device.revoke` 同一条路径：持久提交 → 关闭活动连接）。
async fn revoke_device(router: &LocalAdminRouter, device: &DeviceId) {
    let payload = serde_json::to_vec(&json!({
        "v": 1,
        "id": uuid_text(),
        "method": "device.revoke",
        "params": { "deviceId": device.as_str() },
    }))
    .expect("编码");
    let request: AdminRequest = decode_request(&payload).expect("信封合法");
    match router.handle(request).await.outcome() {
        AdminOutcome::Success { .. } => {}
        other => panic!("`device.revoke` 必须成功，实际：{other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// 6.1：path 注册、subprotocol 判定、配对端点的安全头与请求体上限
// ---------------------------------------------------------------------------------------------

/// `/sync/v1` 带 Sync 的 subprotocol 升级成功，带 Node Link 的 subprotocol 被拒。
#[tokio::test]
async fn the_sync_endpoint_upgrades_only_with_the_sync_subprotocol() {
    let harness = Harness::start("subprotocol").await;

    let client = WsClient::try_upgrade(harness.addr, SYNC_WS_PATH, SYNC_WS_SUBPROTOCOL, None).await;
    assert!(
        client.is_ok(),
        "`/sync/v1` 必须按 Sync 的 subprotocol 升级成功，实际：{:?}",
        client.err()
    );
    drop(client);

    let mismatched =
        WsClient::try_upgrade(harness.addr, SYNC_WS_PATH, NODE_LINK_WS_SUBPROTOCOL, None).await;
    let head = match mismatched {
        Err(head) => head,
        Ok(_) => panic!("带 Node Link subprotocol 的升级必须被拒"),
    };
    assert!(
        head.starts_with("HTTP/1.1 400 "),
        "升级被拒必须是 4xx（接入层在升级前判定 subprotocol），实际：{head}"
    );

    // 反向：Node Link 的端点在 Sync 的 subprotocol 下同样不升级（两个端点各有自己的约定值）。
    let reversed =
        WsClient::try_upgrade(harness.addr, NODE_LINK_WS_PATH, SYNC_WS_SUBPROTOCOL, None).await;
    let head = match reversed {
        Err(head) => head,
        Ok(_) => panic!("`/node-link/v1` 不接受 Sync 的 subprotocol"),
    };
    assert!(head.starts_with("HTTP/1.1 400 "), "实际：{head}");

    harness.stop().await;
}

/// 两条配对 POST 进入 Sync 的逻辑（不是 404），且每个响应都带 §7.1 的四个安全头。
///
/// 探针：`/sync/v1/pairing/claim` 与 `/sync/v1/pairing/status` 在**未注册**时都会得到 404
/// （`transport::net` 的 fallback），因此「400 + `protocol.invalid_json`」就是「进了 Sync 处理器」
/// 的判据。
#[tokio::test]
async fn both_pairing_paths_reach_the_sync_handlers_with_the_security_headers() {
    let harness = Harness::start("pairing-paths").await;

    let claim = post(
        harness.addr,
        server::sync::pairing::CLAIM_PATH,
        Some("application/json"),
        b"{not json",
    )
    .await;
    assert_eq!(claim.status, 400, "claim 的 JSON 语法错误必须是 400");
    assert!(
        claim.body.contains("protocol.invalid_json"),
        "claim 必须由 Sync 处理器判定（而不是 404/405），实际 body：{}",
        claim.body
    );
    claim.assert_security_headers();

    let status = post(
        harness.addr,
        server::sync::pairing::STATUS_PATH,
        Some("application/json"),
        b"{not json",
    )
    .await;
    assert_eq!(status.status, 400, "status 的 JSON 语法错误必须是 400");
    assert!(
        status.body.contains("protocol.invalid_json"),
        "status 必须由 Sync 处理器判定（而不是 404/405），实际 body：{}",
        status.body
    );
    status.assert_security_headers();

    // 非 `application/json` 的请求也被 Sync 处理器拒绝（同样是「已进入 Sync 逻辑」的证据）。
    let wrong_content_type = post(
        harness.addr,
        server::sync::pairing::CLAIM_PATH,
        Some("text/plain"),
        b"{}",
    )
    .await;
    assert_eq!(wrong_content_type.status, 400);
    assert!(wrong_content_type.body.contains("protocol.schema_invalid"));
    wrong_content_type.assert_security_headers();

    harness.stop().await;
}

/// 配对 HTTP 的请求体上限是 §14 冻结的 16 KiB：20 KiB 的请求必须得到 413（且带四个安全头）。
///
/// 探针：把接入层的默认上限改回 64 KiB，这条用例就会变红——它锁的正是「16–64 KiB 区间曾被放行」这个
/// 偏差。
#[tokio::test]
async fn the_pairing_body_limit_is_sixteen_kibibytes() {
    let harness = Harness::start("body-limit").await;
    let body = vec![b'{'; 20 * 1024];

    let response = post(
        harness.addr,
        server::sync::pairing::CLAIM_PATH,
        Some("application/json"),
        &body,
    )
    .await;
    assert_eq!(
        response.status, 413,
        "超过 16 KiB 的配对请求体必须被接入层以 413 拒绝（§14）"
    );
    // 413 是接入层在调用处理器**之前**产生的响应：四个安全头必须由 `register_post` 的每路径默认头补齐。
    response.assert_security_headers();

    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// 6.4：撤销钩子接真实连接表
// ---------------------------------------------------------------------------------------------

/// `device.revoke` 提交后：该设备的活动 Sync 连接被关闭（4410），后续认证被拒（`auth.device_revoked`）。
#[tokio::test]
async fn revoking_a_device_closes_its_connection_and_blocks_later_authentication() {
    let harness = Harness::start("revocation").await;
    let origin = CanonicalOrigin::parse(&harness.origin()).expect("canonical origin");
    let device = DeviceId::new(&uuid_text()).expect("device id 是 canonical uuid");
    let signing = p256::ecdsa::SigningKey::from_slice(&DEVICE_SCALAR).expect("测试标量合法");
    let public =
        PeerPublicKey::try_from_bytes(signing.verifying_key().to_sec1_point(false).as_bytes())
            .expect("设备公钥是 65 字节 SEC1 未压缩点");
    let node_id = harness.authority.local_node().clone();
    approve_device(&harness.core, &harness.authority, &device, &public, &origin).await;

    // ① 活动连接：真实握手后连接登记进 Sync 的连接表。
    let mut client = WsClient::upgrade(
        harness.addr,
        SYNC_WS_PATH,
        SYNC_WS_SUBPROTOCOL,
        &harness.origin(),
    )
    .await;
    let authenticated =
        authenticate(&mut client, &node_id, &device, &harness.origin(), &signing).await;
    assert_eq!(
        authenticated["type"],
        json!("auth.authenticated"),
        "设备必须完成认证，实际：{authenticated}"
    );
    assert_eq!(
        harness.active_sync_connections(),
        1,
        "认证成功后必须有一条活动连接"
    );

    // ② 撤销（真实本地管理路径：持久提交 → `ConnectionCloser::close_device`）。
    revoke_device(&harness.router, &device).await;

    // ③ 活动连接被关闭：对端看到 4410（§12.3 的「设备已撤销」），连接表随之清空。
    assert_eq!(
        client.expect_close().await,
        server::sync::conn::close::REVOKED,
        "撤销必须关闭该设备的活动连接"
    );
    assert_eq!(harness.active_sync_connections(), 0, "关闭后连接表必须为空");

    // ④ 后续认证被拒：同一设备的新连接在 proof 阶段得到 `auth.device_revoked`（凭据状态已不再是 active）。
    let mut second = WsClient::upgrade(
        harness.addr,
        SYNC_WS_PATH,
        SYNC_WS_SUBPROTOCOL,
        &harness.origin(),
    )
    .await;
    let refused = authenticate(&mut second, &node_id, &device, &harness.origin(), &signing).await;
    assert_eq!(
        refused["type"],
        json!("error"),
        "撤销后的认证必须被拒，实际：{refused}"
    );
    assert_eq!(
        refused["body"]["code"],
        json!("auth.device_revoked"),
        "拒绝码必须是「设备已撤销」，实际：{refused}"
    );
    assert_eq!(
        harness.active_sync_connections(),
        0,
        "被拒的连接不得进入连接表"
    );

    harness.stop().await;
}

/// 走完一次真实握手并**交回协商出的 feature 集合**（`selected = 客户端 ∩ 服务端`）。
///
/// `omitted = Some(id)` 时客户端少报那一条 —— 真实握手路径上可达（协商是交集，不是服务端单方面决定）。
/// 只走前两步（`auth.client_hello` → `auth.server_challenge`）就够：协商结果在 challenge 上。
async fn challenge_features(
    client: &mut WsClient,
    host: &NodeId,
    device: &DeviceId,
    omitted: Option<&str>,
) -> Vec<String> {
    let declared: Vec<&str> = SUPPORTED_FEATURES
        .iter()
        .copied()
        .filter(|feature| omitted != Some(feature))
        .collect();
    let challenge = hello_and_challenge(client, host, device, declared).await;
    selected_feature_ids(&challenge)
}

/// 组合根接线的门控：协商集合由**客户端声明 ∩ 服务端支持**决定，而它决定后续 `event` 帧的形状。
///
/// 这条用例闭合 6.1 交付时自报的缺口——「组合根装的是哪条路由 / feature 有没有真的传到各面」此前
/// 只有代码审阅把关，没有可执行断言。同一设备发两条连接：一条报全部 feature，一条少报
/// `core.event-origin.v1`；两条的 `selectedFeatures` 必须不同，且**协商到的那条**才有门控输入。
#[tokio::test]
async fn the_negotiated_features_differ_per_connection_and_gate_the_event_shape() {
    let harness = Harness::start("feature-gate").await;
    let origin = CanonicalOrigin::parse(&harness.origin()).expect("canonical origin");
    let device = DeviceId::new(&uuid_text()).expect("device id 是 canonical uuid");
    let signing = p256::ecdsa::SigningKey::from_slice(&DEVICE_SCALAR).expect("测试标量合法");
    let public =
        PeerPublicKey::try_from_bytes(signing.verifying_key().to_sec1_point(false).as_bytes())
            .expect("设备公钥是 65 字节 SEC1 未压缩点");
    let node_id = harness.authority.local_node().clone();
    approve_device(&harness.core, &harness.authority, &device, &public, &origin).await;

    let feature = server::sync::subscribe::event::EVENT_ORIGIN_FEATURE;

    // 协商集合在 server_challenge 上（`selected = 客户端 ∩ 服务端`）：`auth.authenticated` 不带它。
    // 与既有 `authenticate` 内层取法一致，这里直接从 challenge 读。
    let mut full = WsClient::upgrade(
        harness.addr,
        SYNC_WS_PATH,
        SYNC_WS_SUBPROTOCOL,
        &harness.origin(),
    )
    .await;
    let negotiated_full = challenge_features(&mut full, &node_id, &device, None).await;
    assert!(
        negotiated_full.contains(&feature.to_string()),
        "报全部 feature 的连接必须协商到 core.event-origin.v1，实际 {negotiated_full:?}"
    );

    // ② 少报一条的连接：协商集合必须【不含】它。
    let mut partial = WsClient::upgrade(
        harness.addr,
        SYNC_WS_PATH,
        SYNC_WS_SUBPROTOCOL,
        &harness.origin(),
    )
    .await;
    let negotiated_partial =
        challenge_features(&mut partial, &node_id, &device, Some(feature)).await;
    assert!(
        !negotiated_partial.contains(&feature.to_string()),
        "客户端少报时必须协商不到 core.event-origin.v1，实际 {negotiated_partial:?}"
    );

    // ③ 两条连接的协商集合确实不同 —— 这是「门控按连接判」的前提。
    assert_ne!(
        negotiated_full, negotiated_partial,
        "同一条链路上的两条连接必须能协商出不同集合"
    );

    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// `workspace.browse`：真实 daemon + 真实 SQLite 上的端到端往返与负向断言（任务 3.3–3.5）
// ---------------------------------------------------------------------------------------------

/// 一条认证后、能发 `command` 帧的连接（认证后的消息必须带 `connectionId`/`connectionSequence`，
/// §4.1，因此这里把连接标识与序号一起持住）。
struct CommandClient {
    client: WsClient,
    connection_id: String,
    /// 本方向的期望序号（四条 `auth` 消息在认证前，因此第一条 `command` 从 1 起）。
    sequence: u64,
    /// 本次协商出的 feature 集合（门控的输入）。
    selected: Vec<String>,
}

impl CommandClient {
    /// 发一条 `command` 帧并交回服务端的**下一条**消息（`command.result` 或 `error`）。
    async fn command(&mut self, request_id: &str, command: &str, payload: Value) -> Value {
        self.sequence += 1;
        self.client
            .send_text(
                &json!({
                    "protocolVersion": 1,
                    "type": "command",
                    "messageId": uuid_text(),
                    "connectionId": self.connection_id,
                    "connectionSequence": self.sequence.to_string(),
                    "body": {
                        "requestId": request_id,
                        "command": command,
                        "payload": payload,
                    },
                })
                .to_string(),
            )
            .await;
        self.client.next_message().await
    }
}

/// 配对一台设备、升级 `/sync/v1` 并走完握手，交回可发命令的连接。
///
/// 与其余用例同一条写入路径（`identity_auth` 的状态机 + `core::use_cases` 落库）与同一个装配点
/// （`NetIngress::start`）。
async fn command_client(
    harness: &Harness,
    origin: &CanonicalOrigin,
    device: &DeviceId,
) -> CommandClient {
    let signing = p256::ecdsa::SigningKey::from_slice(&DEVICE_SCALAR).expect("测试标量合法");
    let public =
        PeerPublicKey::try_from_bytes(signing.verifying_key().to_sec1_point(false).as_bytes())
            .expect("设备公钥是 65 字节 SEC1 未压缩点");
    let node_id = harness.authority.local_node().clone();
    approve_device(&harness.core, &harness.authority, device, &public, origin).await;
    let mut client = WsClient::upgrade(
        harness.addr,
        SYNC_WS_PATH,
        SYNC_WS_SUBPROTOCOL,
        &harness.origin(),
    )
    .await;
    let challenge =
        hello_and_challenge(&mut client, &node_id, device, SUPPORTED_FEATURES.to_vec()).await;
    let selected = selected_feature_ids(&challenge);
    let authenticated = prove(
        &mut client,
        &node_id,
        device,
        &harness.origin(),
        &signing,
        &challenge,
    )
    .await;
    assert_eq!(
        authenticated["type"],
        json!("auth.authenticated"),
        "设备必须完成认证，实际：{authenticated}"
    );
    CommandClient {
        connection_id: challenge_connection_id(&challenge),
        client,
        sequence: 0,
        selected,
    }
}

/// 本机目录表与审计表的行数（浏览「无副作用」的判据）。
async fn table_rows(core: &UseCases) -> (usize, usize) {
    let directories = core
        .directories(&Actor::LocalCli)
        .await
        .expect("目录表可读");
    let audit = core
        .audit(&Actor::LocalCli, AuditQuery::default())
        .await
        .expect("审计表可读");
    (directories.len(), audit.len())
}

/// 测试夹具：直接写一条目录记录（A 之后目录只能由成功的会话创建写入）。
async fn seed_directory_row(harness: &Harness, id: &str, path: &str) {
    let database = harness._temp.0.join("data").join("acp-remote.sqlite3");
    let url = format!("sqlite://{}", database.display());
    let pool = sqlx::SqlitePool::connect(&url)
        .await
        .expect("打开测试数据库");
    sqlx::query(
        "INSERT INTO owned_directory (directory_id, canonical_path, created_at, last_used_at) \
         VALUES (?1, ?2, ?3, ?3) ON CONFLICT(directory_id) DO NOTHING",
    )
    .bind(id)
    .bind(path)
    .bind("2026-10-17T09:00:00.000Z")
    .execute(&pool)
    .await
    .expect("写入目录记录");
    pool.close().await;
}

/// 一次真实浏览：逐层返回**直接**子目录（不含文件、不含更深层），并回显请求的父路径。
///
/// 同时钉住「浏览不产生登记与审计副作用」：调用前后 workspace 登记表与审计表的行数都不变（登记表里
/// 已有一条记录，因此这不是「0 == 0」的弱断言），且已登记目录在同一次浏览里被标成 `registered`。
#[tokio::test]
async fn a_workspace_browse_round_trip_has_no_registration_or_audit_side_effect() {
    let harness = Harness::start("browse").await;
    let origin = CanonicalOrigin::parse(&harness.origin()).expect("canonical origin");
    let device = DeviceId::new(&uuid_text()).expect("device id 是 canonical uuid");
    let mut client = command_client(&harness, &origin, &device).await;
    assert!(
        client
            .selected
            .contains(&"core.workspace-browse.v1".to_owned()),
        "客户端报全部 feature，必须协商到 core.workspace-browse.v1：{:?}",
        client.selected
    );

    // 一棵真实目录树：两个子目录 + 一个普通文件（文件条目与更深层都不得出现在结果里）。
    let tree = TempDir::new("browse-tree");
    std::fs::create_dir_all(tree.0.join("alpha")).expect("建子目录");
    std::fs::create_dir_all(tree.0.join("beta").join("deeper")).expect("建更深的层");
    std::fs::write(tree.0.join("note.txt"), b"content").expect("写文件");

    // 先把 `beta` 记录成一个目录：结果里它必须带 `registered: true`，而目录表本身不得被改动。
    let beta_path = tree.0.join("beta");
    let canonical = std::fs::canonicalize(&beta_path).expect("可规范化");
    let beta_text = canonical.to_str().expect("路径是 UTF-8");
    let beta_id = acp_core::model::derive_directory_id(beta_text);
    seed_directory_row(&harness, beta_id.as_str(), beta_text).await;

    let before = table_rows(&harness.core).await;
    assert_eq!(
        before.0, 1,
        "登记表里必须已有一条记录（否则行数断言没有意义）"
    );

    let requested = tree.0.to_string_lossy().into_owned();
    let message = client
        .command(
            "0a6f3c85-91d2-4e77-b5c8-3f4a2d9e6b10",
            "workspace.browse",
            json!({ "path": requested }),
        )
        .await;
    assert_eq!(message["type"], json!("command.result"), "{message}");
    let body = &message["body"];
    assert_eq!(body["status"], json!("completed"), "{message}");
    assert_eq!(
        body["result"]["path"],
        json!(requested),
        "结果逐字回显请求的父路径"
    );
    assert_eq!(body["result"]["truncated"], json!(false));
    let entries = body["result"]["entries"]
        .as_array()
        .expect("entries 是数组");
    let names: Vec<&str> = entries
        .iter()
        .map(|entry| entry["name"].as_str().expect("name"))
        .collect();
    assert_eq!(
        names,
        ["alpha", "beta"],
        "只含直接子目录（不含文件，也不含 beta 下的 deeper），且按名称升序"
    );
    for (entry, name) in entries.iter().zip(["alpha", "beta"]) {
        assert_eq!(
            entry["path"]
                .as_str()
                .and_then(|path| Path::new(path).canonicalize().ok()),
            tree.0.join(name).canonicalize().ok(),
            "条目的绝对路径必须指回该子目录：{entry}"
        );
    }
    assert_eq!(
        entries[0]["registered"],
        json!(false),
        "未登记的目录不是已登记：{entries:?}"
    );
    assert_eq!(
        entries[1]["registered"],
        json!(true),
        "已登记的目录必须被标成 registered：{entries:?}"
    );
    assert!(
        entries
            .iter()
            .all(|entry| entry["name"] != json!("note.txt")),
        "文件条目不得到达对端：{entries:?}"
    );

    let after = table_rows(&harness.core).await;
    assert_eq!(after.0, before.0, "浏览不得改变 workspace 登记表");
    assert_eq!(after.1, before.1, "浏览不得写入审计记录");

    harness.stop().await;
}

/// 失败路径不回显客户端提供的路径：形状非法与不可达各自的错误 `details` 只含登记字段（这里为空），
/// 整条帧里都不出现那段路径文本（浏览与建目录两条都查）。
#[tokio::test]
async fn a_failed_browse_or_create_never_echoes_the_path() {
    let harness = Harness::start("browse-negative").await;
    let origin = CanonicalOrigin::parse(&harness.origin()).expect("canonical origin");
    let device = DeviceId::new(&uuid_text()).expect("device id 是 canonical uuid");
    let mut client = command_client(&harness, &origin, &device).await;
    /// 只出现在客户端输入里的哨兵：它一出现就说明服务端把路径回显了。
    const SENTINEL: &str = "acpr-leak-sentinel-wp3";

    // ① 形状非法（相对路径 + 父目录引用）：参数类错误，`details` 为空，帧里没有哨兵。
    let relative = format!("../{SENTINEL}");
    let message = client
        .command(
            "1c8e5b73-2a49-4d60-9f31-6b0d7e2c8a54",
            "workspace.browse",
            json!({ "path": relative }),
        )
        .await;
    assert_eq!(
        message["body"]["error"]["code"],
        json!("protocol.schema_invalid"),
        "{message}"
    );
    assert_eq!(
        message["body"]["error"]["details"],
        json!({}),
        "错误 details 不得携带任何路径：{message}"
    );
    assert!(
        !message.to_string().contains(SENTINEL),
        "形状非法的错误不得回显路径：{message}"
    );

    // ② 形状合法但不存在：服务端不可用类，同样不回显。
    let missing = std::env::temp_dir()
        .join(SENTINEL)
        .to_string_lossy()
        .into_owned();
    let message = client
        .command(
            "2a5f0b7d-3c61-4b8c-9d72-8e1f2a3b4c5d",
            "workspace.browse",
            json!({ "path": missing }),
        )
        .await;
    assert_eq!(
        message["body"]["error"]["code"],
        json!("internal.unavailable"),
        "{message}"
    );
    assert!(
        !message.to_string().contains(SENTINEL),
        "不可达的错误不得回显路径：{message}"
    );

    // ③ 建目录失败（`createIfMissing` 为真但路径形状非法）：只带字段名与规则，不带路径。
    let message = client
        .command(
            "3b6a1c8d-4d72-4c9d-8e83-9f2a3b4c5d6e",
            "session.create",
            json!({
                "agentId": "codex",
                "workspacePath": relative,
                "createIfMissing": true,
            }),
        )
        .await;
    assert_eq!(
        message["body"]["error"]["code"],
        json!("protocol.schema_invalid"),
        "{message}"
    );
    assert!(
        !message.to_string().contains(SENTINEL),
        "建目录失败的错误不得回显路径：{message}"
    );

    harness.stop().await;
}
