//! `server::node_link::conn` 的行为用例（`[PV3]` 的 WP4 部分）。
//!
//! 驱动方式：真实 loopback listener（只经 `transport::net` 的公开形状注册 WSS 处理器）+ 本模块自带的
//! 极简 WebSocket 客户端。因此「Host/subprotocol 准入 → 帧 → 信封 → 握手 → 业务」整条链路都在真实
//! 连接上被验证，接入层与协议层之间的接线（例如 binary 帧、超限、close code）不是手工调用的结果。
//!
//! 配对侧的事实由 `local_admin::test_support` 的端口替身提供（同一 crate 的测试设施，不是第二个实现）：
//! `node.pair.begin` → `TestWorld::claim` → `node.pair.confirm` 经真实的本地通道推进到 `approved`，
//! 与 WP3 的 HTTPS claim 路径落库的事实同形。
//!
//! 覆盖映射（`specs/node-link-owner-server/spec.md`）：
//!
//! | 需求 | 用例 |
//! |---|---|
//! | [R36] 握手准入与版本/feature 协商 | `handshake_completes_and_enters_the_business_phase`、`an_envelope_version_other_than_v1_is_closed_with_4406`、`a_handshake_that_never_gets_a_hello_is_closed_with_4408` |
//! | [R37] 正常握手完成 | `handshake_completes_and_enters_the_business_phase` |
//! | [R38] 认证前发送业务消息 | `a_business_message_before_hello_is_closed_with_4401` |
//! | [R39] 必需 feature 未满足 | `a_missing_required_feature_is_reported_with_details_and_closed` |
//! | [R40] 节点双向认证与凭据状态 | `handshake_completes_and_enters_the_business_phase`（双向证明与审计） |
//! | [R37]/[R40] 的验收项（任务 2.29） | `the_challenge_catalog_revision_is_the_proof_transcript_source` |
//! | [R41] proof 无效被拒绝 | `an_invalid_proof_is_closed_with_4401_and_audited` |
//! | [R42] 已撤销节点连接被拒绝 | `a_revoked_node_is_closed_with_4410` |
//! | [R43] 未知节点不泄露存在性之外的能力 | `an_unknown_node_gets_a_challenge_and_fails_as_node_unknown` |
//! | [R44]/[R45] limits 只下调 | `node_ready_echoes_the_negotiated_limits_and_never_raises_them` |
//! | [R46] 固定常量不可协商 | `node_ready_echoes_the_negotiated_limits_and_never_raises_them`、`protocol_constants_are_not_configurable` |
//! | [R47] 信封与 connectionSequence 校验 | `envelope_and_sequence_violations_are_rejected_without_closing`、`a_binary_frame_is_closed_with_4400` |
//! | [R48] 序号回退被拒绝 | `envelope_and_sequence_violations_are_rejected_without_closing` |
//! | [R49] post_mvp 消息显式拒绝 | `envelope_and_sequence_violations_are_rejected_without_closing`（`catalog.changed`/`link.backpressure`） |
//! | [R50] 未知字段被拒绝 | `envelope_and_sequence_violations_are_rejected_without_closing` |
//! | [R32] 首次认证清除已批准配对 | `handshake_completes_and_enters_the_business_phase` |
//! | [R79]/[R80] 心跳与 90 秒静默 | `heartbeat_round_trip_keeps_the_connection_alive`、`silence_beyond_the_window_is_closed_with_4408` |
//! | [R81] 慢连接不影响其他连接 | `a_saturated_connection_is_disconnected_without_affecting_its_peer` |
//! | [R82]/[R83] 审计与日志边界 | `handshake_completes_and_enters_the_business_phase`、`an_invalid_proof_is_closed_with_4401_and_audited` |
//! | 验收「schema/fixture 漂移」 | `fixtures_are_consumed_by_the_handshake_and_error_layers` |
//!
//! 两处**刻意的测试手法**（都在用例里就地说明）：
//!
//! - `node-link-proof/v1` 的 `catalogRevision`：用例只从 `node.challenge` 的 body 取该字段（Access 该走的路），
//!   不再带外取值——2026-09-26 的 v1 内合同修订把字段补进了握手消息（`NODE_LINK_PROTOCOL.md` §12.2 的修订记录，
//!   design D13）；`the_challenge_catalog_revision_is_the_proof_transcript_source` 进一步锁定
//!   「取错值必然验签失败」，证明该字段真的进了验签输入；
//! - 90 秒静默与 30 秒慢消费者宽限：用 [`NodeLinkConn::with_test_windows`] 调短窗口（常量取值由
//!   `protocol_constants_are_not_configurable` 与静默边界断言锁定，机制由真实连接覆盖）。

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use acp_core::model::{
    AuditAction, AuditOutcome, EntityRef, NodeId, NodeState, PairingId, PairingPeer, PairingState,
    PeerIdentity,
};
use acp_core::ports::{AuditQuery, AuditStore as _, SessionStore};
use acp_core::use_cases::NodeLinkHandshakeView;
use base64::Engine as _;
use identity_auth::{ChallengeId, FeatureList, NodeLinkChallenge, NodeLinkProof, P1363Signature};
use node_link_protocol::common::Uuid;
use node_link_protocol::envelope::MessageType;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;

use crate::local_admin::envelope::{AdminOutcome, AdminRequest, AdminResponse, decode_request};
use crate::local_admin::handler::LocalAdminHandler as _;
use crate::local_admin::method::Method;
use crate::local_admin::test_support::{
    FixedStore, TEST_PUBLIC_ORIGIN, TEST_SERVER_EPOCH, TestWorld, test_nonce, test_public_key,
};
use crate::node_link::conn::handshake::SUPPORTED_FEATURES;
use crate::node_link::conn::limits::{
    DEFAULT_CATALOG_SNAPSHOT_BATCH_SIZE, DEFAULT_HEARTBEAT_INTERVAL_MS,
    DEFAULT_MAX_IN_FLIGHT_COMMANDS, DEFAULT_MAX_MESSAGE_BYTES, DEFAULT_MAX_PENDING_QUEUE_BYTES,
    DEFAULT_MAX_PENDING_QUEUE_MESSAGES, DEFAULT_RESOURCE_SNAPSHOT_BATCH_SIZE, NodeLinkConfig,
};
use crate::node_link::conn::session::HEARTBEAT_SILENCE_TIMEOUT;
use crate::node_link::conn::{
    ConnectionRegistry, HANDSHAKE_TIMEOUT, NodeLinkConn, WS_PATH, WS_SUBPROTOCOL, WsEndpoint,
};
use crate::transport::net::{NetConfig, NetError, NetListener, Shutdown, ShutdownHandle};

// ---------------------------------------------------------------------------------------------
// 常量与固定素材
// ---------------------------------------------------------------------------------------------

/// 本节点标识（与 `TestWorld` 装配的 `Authority` 同源）。
const OWNER_NODE: &str = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
/// 测试扮演的 Access Node（经真实配对通道批准）。
const ACCESS_NODE: &str = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";
/// 从未配对的 Access Node（[R43]）。
const UNKNOWN_NODE: &str = "9c8f6b1d-7a35-4f0b-9b6a-2f6d5c4e3b1a";
/// `FixedStore` 的水位：`node.ready.catalogRevision`/`serverEpoch` 必须与它同源。
const HEAD_SEQUENCE: u64 = 7;
/// 单次 I/O 的超时（用例失败要快速失败，不挂住测试进程）。
const IO_TIMEOUT: Duration = Duration::from_secs(10);
/// [`a_handshake_that_never_gets_a_hello_is_closed_with_4408`] 需要等满 15 秒握手窗口。
const TIMEOUT_WINDOW_IO: Duration = Duration::from_secs(25);
/// `node-link.core.v1` 是唯一必需 feature（`features.json` 的 `required: true`）。
const REQUIRED_FEATURE: &str = "node-link.core.v1";

// ---------------------------------------------------------------------------------------------
// 世界与端点
// ---------------------------------------------------------------------------------------------

/// 端口替身世界 + 真实 loopback listener + 已注册的 WSS 端点。
struct Harness {
    world: TestWorld,
    addr: SocketAddr,
    conn: Arc<NodeLinkConn>,
    shutdown: ShutdownHandle,
    join: JoinHandle<Result<(), NetError>>,
}

impl Harness {
    /// 起一个已注册 `/node-link/v1` 的接入层（`127.0.0.1:0`，端口由内核分配）。
    async fn start(world: TestWorld, config: NodeLinkConfig) -> Self {
        let registry = ConnectionRegistry::new();
        let conn = Arc::new(NodeLinkConn::new(
            world.core.clone(),
            world.authority.clone(),
            config,
            Arc::clone(&registry),
        ));
        let mut listener = NetListener::bind(NetConfig {
            listen: "127.0.0.1:0".to_owned(),
            ..NetConfig::default()
        })
        .await
        .expect("绑定 loopback listener");
        listener
            .register_ws(
                WS_PATH,
                WS_SUBPROTOCOL,
                WsEndpoint::handler(Arc::clone(&conn)),
            )
            .expect("注册 Node Link 端点");
        let addr = listener.local_addrs()[0];
        let (shutdown, signal) = Shutdown::channel();
        let join = tokio::spawn(async move { listener.serve(signal).await });
        Self {
            world,
            addr,
            conn,
            shutdown,
            join,
        }
    }

    /// 默认配置 + 带固定水位的存储替身（`daemon.public_origin` 与 `TestWorld` 的 canonical origin 同源）。
    async fn with_store(store: Arc<dyn SessionStore>) -> Self {
        Self::start(
            TestWorld::with_store(store),
            NodeLinkConfig {
                public_origin: Some(TEST_PUBLIC_ORIGIN.to_owned()),
                ..NodeLinkConfig::default()
            },
        )
        .await
    }

    /// 默认世界与默认配置（水位固定为 [`HEAD_SEQUENCE`]）。
    async fn new() -> Self {
        Self::with_store(Arc::new(FixedStore::new(TEST_SERVER_EPOCH, HEAD_SEQUENCE))).await
    }

    /// 结束接入层（用例末尾调用；端口随 listener 释放）。
    async fn stop(self) {
        self.shutdown.trigger();
        let _ = tokio::time::timeout(IO_TIMEOUT, self.join).await;
    }

    /// 本机用户登记一次节点配对（`node.pair.begin`），返回配对 id。
    async fn begin(&self) -> String {
        let response = self
            .world
            .router()
            .handle(admin_request(
                Method::NodePairBegin,
                json!({
                    "mode": "owner",
                    "pairingUrl": null,
                    "displayName": "Owner Node",
                    "requestedGrants": ["grant.observe"],
                }),
            ))
            .await;
        success(&response)["pairingId"]
            .as_str()
            .expect("pairingId")
            .to_owned()
    }

    /// Access 侧认领（与 HTTPS claim 落库的事实同形：peer 行 + `pending_confirmation`）。
    ///
    /// `host_binding` 取注册值的逐字回显（§11.2 第 1 条：绑定不一致的认领必须被拒）。
    async fn claim(&self, pairing: &str) {
        let binding = self
            .world
            .trust
            .pairing(pairing)
            .expect("登记后的配对必须在库里")
            .host_binding()
            .to_owned();
        let peer = PairingPeer::try_new(
            PeerIdentity::Node(node(ACCESS_NODE)),
            "Office Access",
            test_public_key(),
            &binding,
            test_nonce(),
        )
        .expect("测试用 peer 合法");
        self.world
            .claim(&pairing_id(pairing), peer)
            .await
            .expect("认领必须成功");
    }

    /// 本机用户确认配对（真实本地通道；`approved` + 信任行 + 验签材料）。
    async fn approve(&self, pairing: &str, grants: &[&str]) {
        let response = self
            .world
            .router()
            .handle(admin_request(
                Method::NodePairConfirm,
                json!({"pairingId": pairing, "grants": grants}),
            ))
            .await;
        assert_eq!(
            success(&response)["grants"],
            json!(grants),
            "确认后的授权集合必须逐条回显"
        );
    }

    /// 走完整配对链路并返回配对 id（`approved`，授权 `grant.observe`）。
    async fn approved_pairing(&self) -> String {
        let pairing = self.begin().await;
        self.claim(&pairing).await;
        self.approve(&pairing, &["grant.observe"]).await;
        pairing
    }

    /// 本机用户撤销该节点（`node.revoke`，[R42] 的前置）。
    async fn revoke_node(&self, node_id: &str) {
        let response = self
            .world
            .router()
            .handle(admin_request(
                Method::NodeRevoke,
                json!({"nodeId": node_id}),
            ))
            .await;
        let _ = success(&response);
        assert_eq!(
            self.world.trust.nodes_of(node_id)[0].state(),
            NodeState::Revoked,
            "撤销必须先落库"
        );
    }

    /// 持久化信任的当次快照（用例断言 / 带外取 revision 用）。
    async fn view(&self, node_id: &str) -> NodeLinkHandshakeView {
        self.world
            .core
            .node_link_handshake_view(&node(node_id))
            .await
            .expect("握手视图可读")
    }

    /// 该节点最近一次配对的 id（快照里的 `pairing` 行）。
    async fn pairing_of(&self, node_id: &str) -> String {
        self.view(node_id)
            .await
            .pairing
            .expect("该节点有配对行")
            .id()
            .as_str()
            .to_owned()
    }

    /// 审计行（按动作过滤）：`AuditStore` 的行 + 信任写集提交时随事务写入的行。
    async fn audit_rows(&self, actions: Vec<AuditAction>) -> Vec<acp_core::model::AuditRecord> {
        let mut rows = self
            .world
            .audit
            .query(AuditQuery {
                since: None,
                until: None,
                actions: actions.clone(),
                actor: None,
                target: None,
                limit: None,
            })
            .await
            .expect("审计查询");
        rows.extend(
            self.world
                .trust
                .audits()
                .into_iter()
                .filter(|row| actions.is_empty() || actions.contains(&row.action())),
        );
        rows.sort_by(|left, right| left.at().as_str().cmp(right.at().as_str()));
        rows
    }
}

fn node(text: &str) -> NodeId {
    NodeId::new(text).expect("测试用 NodeId 文本必须规范")
}

fn pairing_id(text: &str) -> PairingId {
    PairingId::new(text).expect("测试用 PairingId 文本必须规范")
}

fn admin_request(method: Method, params: Value) -> AdminRequest {
    let bytes = serde_json::to_vec(&json!({
        "v": 1,
        "id": "2ae1c07c-0000-4000-8000-0000000000ff",
        "method": method.as_str(),
        "params": params,
    }))
    .expect("请求信封可序列化");
    decode_request(&bytes).expect("信封合法")
}

fn success(response: &AdminResponse) -> Value {
    match response.outcome() {
        AdminOutcome::Success { result } => Value::Object(result.clone()),
        AdminOutcome::Failure { error } => panic!(
            "期望成功响应，实际 {}：{}",
            error.code().as_str(),
            error.message()
        ),
    }
}

/// 规范的 32 字节 nonce 文本（末字节低 2 位固定为 0）。
fn nonce_text(seed: u8) -> String {
    let mut bytes = [seed; 32];
    bytes[31] &= 0b1111_1100;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// 16 字节 nonce 文本（`link.ping`/`link.pong`）。
fn ping_nonce(seed: u8) -> String {
    let bytes = [seed; 16];
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// 用固定私钥（标量 1，公钥即 `test_public_key()` 的基点 G）签名一段 transcript。
///
/// 与 `FakeKeystore::sign` 同一素材：Access 侧证明必须能由配对批准写入持久化的公钥验证。
fn sign_with_access_key(transcript: &[u8]) -> String {
    use p256::ecdsa::signature::Signer as _;
    let mut scalar = [0u8; 32];
    scalar[31] = 1;
    let signing = p256::ecdsa::SigningKey::from_slice(&scalar).expect("标量 1 是合法私钥");
    let signature: p256::ecdsa::Signature = signing.sign(transcript);
    P1363Signature::try_from_bytes(signature.to_bytes().as_slice())
        .expect("P-256 签名是 64 字节")
        .to_base64url()
}

// ---------------------------------------------------------------------------------------------
// 极简 WebSocket 客户端
// ---------------------------------------------------------------------------------------------

/// 一个入站帧。
enum Frame {
    Text(String),
    Binary,
    Close(u16, String),
    Ping,
    Pong,
}

/// 极简 WebSocket 客户端（文本/二进制/close 帧 + 客户端掩码；不做扩展与分片）。
struct Client {
    stream: TcpStream,
    buffer: Vec<u8>,
    /// 本方向下一个出站序号（认证后从 `1` 开始）。
    outbound_sequence: u64,
    /// `node.challenge` 下发的连接标识（认证后消息必须携带）。
    connection_id: Option<String>,
    /// 已读到但用例尚未取用的消息。
    pending: Vec<Value>,
    /// 已收到的服务端出站序号（认证后从 `node.ready` 的 1 开始）。
    server_sequence: u64,
    /// 当前用例段落（失败信息用）。
    step: String,
    /// 最近一条本方向消息的 `connectionSequence`（失败信息用）。
    last_sent_sequence: Option<String>,
    /// 读帧的超时（15 秒握手窗口的用例需要更长的预算）。
    read_timeout: Duration,
}

impl Client {
    /// 建立连接并完成 WebSocket 升级（带上 subprotocol）。
    async fn connect(addr: SocketAddr) -> Self {
        Self::connect_with_timeout(addr, IO_TIMEOUT).await
    }

    async fn connect_with_timeout(addr: SocketAddr, read_timeout: Duration) -> Self {
        let mut stream = tokio::time::timeout(IO_TIMEOUT, TcpStream::connect(addr))
            .await
            .expect("连接超时")
            .expect("连接 loopback");
        let mut key_bytes = [0x11u8; 16];
        key_bytes[0] = 0x42;
        let key = base64::engine::general_purpose::STANDARD.encode(key_bytes);
        let request = format!(
            "GET {WS_PATH} HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Protocol: {WS_SUBPROTOCOL}\r\n\r\n"
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
        assert!(
            head.to_ascii_lowercase()
                .contains(&format!("sec-websocket-protocol: {WS_SUBPROTOCOL}")),
            "升级响应必须回显协商出的 subprotocol，实际：{head}"
        );
        buffer.drain(..head_end + 4);
        Self {
            stream,
            buffer,
            outbound_sequence: 0,
            connection_id: None,
            pending: Vec::new(),
            server_sequence: 0,
            step: String::new(),
            last_sent_sequence: None,
            read_timeout,
        }
    }

    /// 发送一条文本帧。
    async fn send_text(&mut self, text: &str) {
        self.send_frame(0x1, text.as_bytes()).await;
    }

    /// 发送一条二进制帧。
    async fn send_binary(&mut self, bytes: &[u8]) {
        self.send_frame(0x2, bytes).await;
    }

    /// 发送一个 close 帧（`1000`）。
    async fn send_close(&mut self) {
        self.send_frame(0x8, &1000u16.to_be_bytes()).await;
    }

    async fn send_frame(&mut self, opcode: u8, payload: &[u8]) {
        let mut frame = Vec::with_capacity(payload.len() + 14);
        frame.push(0x80 | opcode);
        // 客户端帧必须掩码；掩码键取固定值（对端不校验取值）。
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

    /// 读取下一个帧（`None` = 连接已结束）。
    async fn next_frame(&mut self) -> Option<Frame> {
        self.fill(2).await?;
        let opcode = self.buffer[0] & 0x0f;
        let masked = self.buffer[1] & 0x80 != 0;
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
        if masked {
            offset += 4;
        }
        self.fill(offset + length).await?;
        let mask: Option<[u8; 4]> = masked.then(|| {
            let mut key = [0u8; 4];
            key.copy_from_slice(&self.buffer[offset - 4..offset]);
            key
        });
        let mut payload = self.buffer[offset..offset + length].to_vec();
        if let Some(mask) = mask {
            for (index, byte) in payload.iter_mut().enumerate() {
                *byte ^= mask[index % 4];
            }
        }
        self.buffer.drain(..offset + length);
        Some(match opcode {
            0x1 => Frame::Text(String::from_utf8(payload).expect("文本帧必须是 UTF-8")),
            0x2 => Frame::Binary,
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
            0x9 => Frame::Ping,
            0xa => Frame::Pong,
            other => panic!("未预期的 WebSocket opcode：{other}"),
        })
    }

    /// 确保缓冲区里至少有 `needed` 字节；返回 `None` 表示对端已关闭流。
    async fn fill(&mut self, needed: usize) -> Option<()> {
        while self.buffer.len() < needed {
            let mut chunk = [0u8; 16 * 1024];
            let read = tokio::time::timeout(self.read_timeout, self.stream.read(&mut chunk))
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

    /// 取下一条消息（跳过 WS 控制帧）；`None` = 连接结束。
    async fn next_message(&mut self) -> Option<Value> {
        if !self.pending.is_empty() {
            return Some(self.pending.remove(0));
        }
        loop {
            match self.next_frame().await? {
                Frame::Text(text) => {
                    return Some(
                        serde_json::from_str(&text).unwrap_or_else(|error| {
                            panic!("服务端消息不是 JSON（{error}）：{text}")
                        }),
                    );
                }
                Frame::Binary => panic!("本切片的服务端消息必须是文本帧"),
                Frame::Close(code, reason) => panic!("对端在消息之前关闭了连接：{code} {reason}"),
                Frame::Ping | Frame::Pong => continue,
            }
        }
    }

    /// 取下一条消息，并（若是认证后消息）核对服务端出站序号严格加一。
    async fn take_message(&mut self) -> Value {
        let message = self.next_message().await.expect("对端已关闭连接");
        if message.get("connectionId").is_some() {
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
                )
            );
        }
        message
    }

    /// 期待下一条消息的 `type` 是 `message_type`，并返回整条消息。
    async fn expect_type(&mut self, message_type: &str) -> Value {
        let message = self.take_message().await;
        assert_eq!(
            message["type"],
            json!(message_type),
            "步骤「{}」的消息类型不符（本方向计数 {}，上一条发出 {}）：{message}",
            self.step,
            self.outbound_sequence,
            self.last_sent_sequence.as_deref().unwrap_or("<无>")
        );
        message
    }

    /// 期待一条 `link.error` 并返回其 body（消息级拒绝）。
    async fn expect_error(&mut self, expected_code: &str) -> Value {
        let message = self.expect_type("link.error").await;
        assert_eq!(
            message["body"]["code"],
            json!(expected_code),
            "错误码不符：{message}"
        );
        message["body"].clone()
    }

    /// 读到 close 帧（中途的消息收进 `pending`），返回 close code。
    async fn expect_close(&mut self) -> u16 {
        loop {
            match self.next_frame().await {
                None => panic!("对端在没有 close 帧的情况下结束了连接"),
                Some(Frame::Text(text)) => {
                    self.pending
                        .push(serde_json::from_str(&text).expect("文本帧是 JSON"));
                }
                Some(Frame::Close(code, _reason)) => return code,
                Some(Frame::Binary | Frame::Ping | Frame::Pong) => {}
            }
        }
    }

    /// 构造并发送一条认证后消息（`connectionId`/`connectionSequence` 由客户端维护）。
    ///
    /// 序号用「本方向下一个应到的值」，但**不**推进本地计数：只有服务端确实受理后才由调用方
    /// [`Client::advance_after_accepted`] 推进（被拒的消息不推进任何一侧的序号，§2.2）。
    async fn send_post_auth(&mut self, message_type: &str, body: Value) {
        let connection_id = self
            .connection_id
            .clone()
            .expect("认证后消息必须已有 connectionId");
        let sequence = (self.outbound_sequence + 1).to_string();
        self.last_sent_sequence = Some(sequence.clone());
        let text = json!({
            "protocolVersion": 1,
            "type": message_type,
            "messageId": uuid_text(),
            "connectionId": connection_id,
            "connectionSequence": sequence,
            "body": body,
        })
        .to_string();
        self.send_text(&text).await;
    }

    /// 服务端受理了一条本方向的消息后推进序号。
    fn advance_after_accepted(&mut self) {
        self.outbound_sequence += 1;
    }

    /// 标记当前段落（失败信息里带上它，便于定位是哪一步）。
    fn step(&mut self, label: &str) {
        self.step = label.to_owned();
    }

    /// 发送一条认证后消息，但完全由调用方给出信封字段（边界用例）。
    async fn send_raw(&mut self, envelope: Value) {
        self.last_sent_sequence = envelope["connectionSequence"].as_str().map(str::to_owned);
        self.send_text(&envelope.to_string()).await;
    }

    /// 认证后消息的信封骨架（可由用例逐字段改写）。
    fn envelope(&self, message_type: &str, body: Value) -> Value {
        json!({
            "protocolVersion": 1,
            "type": message_type,
            "messageId": uuid_text(),
            "connectionId": self.connection_id.clone().expect("认证后消息"),
            "connectionSequence": (self.outbound_sequence + 1).to_string(),
            "body": body,
        })
    }

    /// 用「下一条合法序号」发送一条 `link.ping`，并期待回填同一 nonce 的 `link.pong`。
    ///
    /// 这是本文件里「连接仍然可用」的统一证据：每次拒绝之后都跑一次，证明拒绝没有让连接进入非法状态。
    async fn ping_round_trip(&mut self, seed: u8) -> Value {
        let nonce = ping_nonce(seed);
        self.send_post_auth("link.ping", json!({"nonce": nonce}))
            .await;
        let pong = self.expect_type("link.pong").await;
        assert_eq!(pong["body"]["nonce"], json!(nonce), "pong 必须原样回填");
        self.advance_after_accepted();
        pong
    }
}

/// 在 buffer 里找子切片。
fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn uuid_text() -> String {
    uuid::Uuid::new_v4().to_string()
}

// ---------------------------------------------------------------------------------------------
// 握手辅助（Access 侧）
// ---------------------------------------------------------------------------------------------

/// 发送 `node.hello`（认证前的第一条消息）。
async fn send_hello(client: &mut Client, access_node: &str, supported: &[&str], required: &[&str]) {
    let text = json!({
        "protocolVersion": 1,
        "type": "node.hello",
        "messageId": uuid_text(),
        "body": {
            "minProtocolVersion": 1,
            "maxProtocolVersion": 1,
            "accessNodeId": access_node,
            "role": "access",
            "clientNonce": nonce_text(0x31),
            "supportedFeatures": supported,
            "requiredFeatures": required,
        },
    })
    .to_string();
    client.send_text(&text).await;
}

/// 全部 mvp feature 加一条 Owner 不实现的 ID（协商必须只取交集）。
fn declared_features() -> Vec<&'static str> {
    let mut features: Vec<&'static str> = SUPPORTED_FEATURES.to_vec();
    features.push("node-link.node-rotation.v1");
    features
}

/// 从 `node.challenge` 的 body 取 `catalogRevision`（decimal string）并转成 transcript 用的 `u64`。
///
/// 这就是 Access 该走的路：2026-09-26 的修订把该字段放进握手消息，两个连接 transcript domain 的 tag 6
/// 都取它（§9.4）。
fn challenge_catalog_revision(challenge: &Value) -> u64 {
    challenge["body"]["catalogRevision"]
        .as_str()
        .unwrap_or_else(|| panic!("node.challenge 必须带 catalogRevision：{challenge}"))
        .parse()
        .expect("catalogRevision 是无前导零的十进制串")
}

/// 发送 `node.proof`：`mutate` 可破坏签名以覆盖 [R41]。
///
/// 两个输入都只从 `node.challenge` 取：`connectionId`/`serverNonce`/`selectedFeatures` 与
/// [`challenge_catalog_revision`]（Access 侧拿不到别的来源）。
async fn send_proof(client: &mut Client, access_node: &str, challenge: &Value, corrupt: bool) {
    send_proof_with_revision(
        client,
        access_node,
        challenge,
        challenge_catalog_revision(challenge),
        corrupt,
    )
    .await;
}

/// 发送 `node.proof`，并显式给出进 transcript 的 `catalogRevision`（供「取错值必然失败」的用例）。
async fn send_proof_with_revision(
    client: &mut Client,
    access_node: &str,
    challenge: &Value,
    catalog_revision: u64,
    corrupt: bool,
) {
    let body = challenge["body"].clone();
    let selected: Vec<String> = body["selectedFeatures"]
        .as_array()
        .expect("selectedFeatures")
        .iter()
        .map(|feature| feature.as_str().expect("feature id").to_owned())
        .collect();
    let input = NodeLinkProof {
        owner_node_id: node(OWNER_NODE),
        access_node_id: node(access_node),
        catalog_revision,
        client_nonce: identity_auth::Nonce::new(&nonce_text(0x31)).expect("规范 nonce"),
        server_nonce: identity_auth::Nonce::new(body["serverNonce"].as_str().expect("serverNonce"))
            .expect("规范 nonce"),
        connection_id: ChallengeId::parse(body["connectionId"].as_str().expect("connectionId"))
            .expect("规范 uuid"),
        negotiated_features: FeatureList::new(selected),
    };
    let transcript = input.transcript().expect("transcript 可装配");
    let mut signature = P1363Signature::try_from_base64url(&sign_with_access_key(&transcript))
        .expect("签名是 64 字节 P1363");
    if corrupt {
        let mut bytes = *signature.as_bytes();
        bytes[0] ^= 0xff;
        signature = P1363Signature::try_from_bytes(&bytes).expect("翻转一个字节仍是 64 字节");
    }
    let text = json!({
        "protocolVersion": 1,
        "type": "node.proof",
        "messageId": uuid_text(),
        "body": {
            "connectionId": body["connectionId"],
            "accessNodeId": access_node,
            "nodeProof": signature.to_base64url(),
        },
    })
    .to_string();
    client.send_text(&text).await;
}

/// 完成一次成功握手（hello → challenge → proof → ready），返回 `node.ready` 消息。
async fn authenticated(client: &mut Client, access_node: &str, corrupt_proof: bool) -> Value {
    send_hello(
        client,
        access_node,
        &declared_features(),
        &[REQUIRED_FEATURE],
    )
    .await;
    let challenge = client.expect_type("node.challenge").await;
    client.connection_id = Some(
        challenge["body"]["connectionId"]
            .as_str()
            .expect("connectionId")
            .to_owned(),
    );
    send_proof(client, access_node, &challenge, corrupt_proof).await;
    client.expect_type("node.ready").await
}

/// 认证前必须省略连接字段（§2.2）：`node.challenge` 的信封检查。
fn assert_no_connection_fields(message: &Value) {
    let object = message.as_object().expect("信封是对象");
    assert!(
        !object.contains_key("connectionId") && !object.contains_key("connectionSequence"),
        "认证前消息必须省略连接字段：{message}"
    );
}

// ---------------------------------------------------------------------------------------------
// R36/R37/R40/R32：正常握手
// ---------------------------------------------------------------------------------------------

/// [R36]/[R37]/[R40]/[R32]：已配对节点完成整条四步握手后进入业务阶段。
///
/// 逐项断言：挑战信封省略连接字段、Owner 挑战证明可验证、`node.ready` 的 `catalogRevision`/`serverEpoch`
/// 与持久化水位同源、limits 与 §2.5 默认值一致、连接已登记、首次认证在同一写集里把已批准配对推进到
/// `consumed` 并留下 `node.authenticated`，连接结束后注册表清空。
#[tokio::test]
async fn handshake_completes_and_enters_the_business_phase() {
    let harness = Harness::new().await;
    let pairing = harness.approved_pairing().await;
    let mut client = Client::connect(harness.addr).await;

    send_hello(
        &mut client,
        ACCESS_NODE,
        &declared_features(),
        &[REQUIRED_FEATURE],
    )
    .await;
    let challenge = client.expect_type("node.challenge").await;
    assert_no_connection_fields(&challenge);

    // R40「双向」的 Owner 侧一半：挑战证明可用本节点公钥在 `node-link-challenge/v1` 域验证。
    // `catalogRevision` 也从 wire 取（它进了两个连接 domain 的 tag 6，见文件头说明）。
    let body = challenge["body"].clone();
    let selected = feature_ids(&body);
    let catalog_revision = challenge_catalog_revision(&challenge);
    assert_eq!(
        catalog_revision, HEAD_SEQUENCE,
        "挑战的修订号必须与本机水位同源"
    );
    let mut expected_selected: Vec<String> = SUPPORTED_FEATURES
        .iter()
        .map(|id| (*id).to_owned())
        .collect();
    expected_selected.sort();
    assert_eq!(
        selected, expected_selected,
        "selectedFeatures 必须是对端声明与 Owner 支持集合的交集（升序）"
    );
    let owner_public_key = harness
        .world
        .authority
        .node_public_key()
        .await
        .expect("本节点公钥可用");
    NodeLinkChallenge {
        owner_node_id: node(OWNER_NODE),
        access_node_id: node(ACCESS_NODE),
        catalog_revision,
        client_nonce: identity_auth::Nonce::new(&nonce_text(0x31)).expect("规范 nonce"),
        server_nonce: identity_auth::Nonce::new(body["serverNonce"].as_str().expect("serverNonce"))
            .expect("规范 nonce"),
        connection_id: ChallengeId::parse(body["connectionId"].as_str().expect("connectionId"))
            .expect("规范 uuid"),
        negotiated_features: FeatureList::new(selected.clone()),
    }
    .verify(
        &owner_public_key,
        &P1363Signature::try_from_base64url(body["nodeProof"].as_str().expect("nodeProof"))
            .expect("挑战证明是 64 字节 P1363"),
    )
    .expect("Owner 挑战证明必须可用本节点公钥验证");

    let connection_id = body["connectionId"]
        .as_str()
        .expect("connectionId")
        .to_owned();
    client.connection_id = Some(connection_id.clone());
    send_proof(&mut client, ACCESS_NODE, &challenge, false).await;

    // R37：`node.ready` 携带 catalogRevision/limits/serverEpoch，且信封序号从 1 开始。
    let ready = client.expect_type("node.ready").await;
    assert_eq!(ready["connectionId"], json!(connection_id));
    assert_eq!(ready["body"]["ownerNodeId"], json!(OWNER_NODE));
    assert_eq!(
        ready["body"]["catalogRevision"],
        json!(HEAD_SEQUENCE.to_string()),
        "catalogRevision 必须与本机水位同源"
    );
    assert_eq!(ready["body"]["serverEpoch"], json!(TEST_SERVER_EPOCH));
    assert_eq!(
        ready["body"]["limits"],
        json!({
            "maxMessageBytes": DEFAULT_MAX_MESSAGE_BYTES,
            "catalogSnapshotBatchSize": DEFAULT_CATALOG_SNAPSHOT_BATCH_SIZE,
            "resourceSnapshotBatchSize": DEFAULT_RESOURCE_SNAPSHOT_BATCH_SIZE,
            "maxInFlightCommands": DEFAULT_MAX_IN_FLIGHT_COMMANDS,
            "maxPendingQueueBytes": DEFAULT_MAX_PENDING_QUEUE_BYTES,
            "maxPendingQueueMessages": DEFAULT_MAX_PENDING_QUEUE_MESSAGES,
            "heartbeatIntervalMs": DEFAULT_HEARTBEAT_INTERVAL_MS,
        }),
        "默认部署下发 §2.5 的默认值"
    );

    // 业务阶段：连接已登记，句柄的事实与握手一致。
    let handles = harness.conn.registry().handles();
    assert_eq!(handles.len(), 1, "认证成功后必须登记连接");
    let handle = &handles[0];
    assert_eq!(handle.connection_id().as_str(), connection_id);
    assert_eq!(handle.node_id(), &node(ACCESS_NODE));
    assert_eq!(handle.client_ip().to_string(), "127.0.0.1");
    assert!(!handle.saturated(), "刚建连的队列不应处于高水位");
    let _ = client.ping_round_trip(0x41).await;

    // R32（衔接侧）：首次认证的写集把已批准配对推进到 consumed，并留下 node.authenticated。
    let consumed = harness.world.trust.pairing(&pairing).expect("配对仍在库里");
    assert_eq!(
        consumed.state(),
        PairingState::Consumed,
        "首条成功认证必须消费已批准配对"
    );
    assert!(
        consumed.terminal_at().is_some(),
        "consumed 必须带终态时间（§11.2）"
    );
    let auth_rows = harness
        .audit_rows(vec![AuditAction::NodeAuthenticated])
        .await;
    assert_eq!(auth_rows.len(), 1, "首条成功认证留一行审计：{auth_rows:?}");
    assert_eq!(auth_rows[0].outcome(), AuditOutcome::Success);
    assert_eq!(auth_rows[0].via_node(), Some(&node(ACCESS_NODE)));
    assert_eq!(
        auth_rows[0].target(),
        &EntityRef::Pairing(pairing_id(&pairing)),
        "首次认证的审计目标是那次配对"
    );
    assert_eq!(
        auth_rows[0].local_principal_ref(),
        None,
        "节点级信任模型下 localPrincipalRef 为空（§8.3）"
    );
    assert_eq!(auth_rows[0].detail_digest(), None, "握手不写细节摘要");
    assert_eq!(
        harness
            .view(ACCESS_NODE)
            .await
            .pairing
            .map(|record| record.state()),
        Some(PairingState::Consumed),
        "重新读快照时不再有待消费的配对"
    );

    // 连接结束后注册表清空（撤销传播与关闭路径依赖这一条）。
    client.send_close().await;
    assert_eventually(
        || harness.conn.registry().active() == 0,
        "会话结束后必须从注册表摘除",
    )
    .await;
    harness.stop().await;
}

/// 挑战/就绪消息 body 的 `selectedFeatures`。
fn feature_ids(body: &Value) -> Vec<String> {
    body["selectedFeatures"]
        .as_array()
        .expect("selectedFeatures")
        .iter()
        .map(|feature| feature.as_str().expect("feature id").to_owned())
        .collect()
}

// ---------------------------------------------------------------------------------------------
// 2.29 的验收用例：`node.challenge.catalogRevision` 是 proof transcript 的 tag 6 来源
// ---------------------------------------------------------------------------------------------

/// [R37]/[R40]（任务 2.29 的验收项，design D13）：Access 只能从 `node.challenge` 拿到 `catalogRevision`，
/// 用它构造的 `node-link-proof/v1` transcript 必须通过验签；改用别的值必然 `proof_invalid`。
///
/// 反例是这条用例的关键：它证明该字段真的进了验签输入（Owner 用签发挑战时记录的同一个值验证），
/// 而不是被忽略的装饰字段；正例则走完整路径（挑战 → proof → `node.ready`）并核对 `node.ready` 与
/// 挑战同源。
#[tokio::test]
async fn the_challenge_catalog_revision_is_the_proof_transcript_source() {
    let harness = Harness::new().await;
    let _ = harness.approved_pairing().await;

    // 反例：拿挑战里的修订号 +1 签名 → proof_invalid + 4401（配对不被消费，留给下面的正例）。
    let mut wrong = Client::connect(harness.addr).await;
    send_hello(
        &mut wrong,
        ACCESS_NODE,
        &declared_features(),
        &[REQUIRED_FEATURE],
    )
    .await;
    let challenge = wrong.expect_type("node.challenge").await;
    let revision = challenge_catalog_revision(&challenge);
    send_proof_with_revision(&mut wrong, ACCESS_NODE, &challenge, revision + 1, false).await;
    wrong.expect_error("nodelink.auth.proof_invalid").await;
    assert_eq!(wrong.expect_close().await, 4401, "取错修订号以 4401 关闭");

    // 正例：同一个修订号（从 wire 读的那个）签出来的 proof 必须被接受。
    let mut client = Client::connect(harness.addr).await;
    let ready = authenticated(&mut client, ACCESS_NODE, false).await;
    assert_eq!(
        ready["body"]["catalogRevision"],
        json!(revision.to_string()),
        "node.ready 与 node.challenge 的修订号必须同源"
    );
    let _ = client.ping_round_trip(0x6a).await;
    harness.stop().await;
}

/// 在预算内轮询一个条件（异步状态收敛的断言，失败时给出固定消息）。
async fn assert_eventually(mut condition: impl FnMut() -> bool, message: &str) {
    let deadline = Instant::now() + IO_TIMEOUT;
    while !condition() {
        assert!(Instant::now() < deadline, "等待超时：{message}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// ---------------------------------------------------------------------------------------------
// R38/R39：认证前的消息准入
// ---------------------------------------------------------------------------------------------

/// [R38]：握手前收到业务消息 → `link.error`（`type_unsupported`）+ 4401 关闭，且不产生任何副作用。
#[tokio::test]
async fn a_business_message_before_hello_is_closed_with_4401() {
    let harness = Harness::new().await;
    let mut client = Client::connect(harness.addr).await;

    // `command.submit` 只允许在认证后出现，因此信封形状按认证后给出（带连接字段）。
    let text = json!({
        "protocolVersion": 1,
        "type": "command.submit",
        "messageId": uuid_text(),
        "connectionId": uuid_text(),
        "connectionSequence": "1",
        "body": {},
    })
    .to_string();
    client.send_text(&text).await;
    let error = client
        .expect_error("nodelink.protocol.type_unsupported")
        .await;
    assert_eq!(error["retryable"], json!(false), "协议类错误不可重试");
    assert_eq!(
        client.expect_close().await,
        4401,
        "认证前的业务消息以 4401 关闭"
    );

    // 没有副作用：没有信任行、没有审计、没有连接登记。
    assert_eq!(harness.world.trust.node_count(), 0);
    assert!(harness.audit_rows(vec![]).await.is_empty());
    assert_eq!(harness.conn.registry().active(), 0);
    harness.stop().await;
}

/// [R39]：对端要求 Owner 不实现的 feature → `feature_required` + `details.features` + 关闭。
#[tokio::test]
async fn a_missing_required_feature_is_reported_with_details_and_closed() {
    let harness = Harness::new().await;
    let _ = harness.approved_pairing().await;
    let mut client = Client::connect(harness.addr).await;

    send_hello(
        &mut client,
        ACCESS_NODE,
        &declared_features(),
        &["node-link.node-rotation.v1", REQUIRED_FEATURE],
    )
    .await;
    let error = client
        .expect_error("nodelink.protocol.feature_required")
        .await;
    assert_eq!(
        error["details"]["features"],
        json!(["node-link.node-rotation.v1"]),
        "details.features 必须给出排序去重后未被选择的必需 feature"
    );
    assert_eq!(
        client.expect_close().await,
        4400,
        "未进入业务阶段即以 4400 关闭"
    );
    // 未进入业务阶段，也不消费配对。
    assert_eq!(
        harness
            .world
            .trust
            .pairing(&harness.pairing_of(ACCESS_NODE).await)
            .map(|record| record.state()),
        Some(PairingState::Approved),
        "协商失败不得消费配对"
    );
    assert_eq!(harness.conn.registry().active(), 0);
    harness.stop().await;
}

/// [R36]：信封 `protocolVersion` 不是 1 → `version_unsupported` + 4406。
#[tokio::test]
async fn an_envelope_version_other_than_v1_is_closed_with_4406() {
    let harness = Harness::new().await;
    let mut client = Client::connect(harness.addr).await;
    let text = json!({
        "protocolVersion": 2,
        "type": "node.hello",
        "messageId": uuid_text(),
        "body": {},
    })
    .to_string();
    client.send_text(&text).await;
    let error = client
        .expect_error("nodelink.protocol.version_unsupported")
        .await;
    assert_eq!(error["retryable"], json!(false));
    assert_eq!(client.expect_close().await, 4406, "版本不兼容以 4406 关闭");
    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// R36：握手超时
// ---------------------------------------------------------------------------------------------

/// [R36]：握手窗口内没有 `node.hello` → 15 秒后以 4408 关闭，且不留认证失败审计（对端身份未知）。
///
/// 这里必须等满真实的 15 秒：窗口是 §2.5 的固定常量，缩短它就无法证明生产路径上的窗口是 15 秒。
#[tokio::test]
async fn a_handshake_that_never_gets_a_hello_is_closed_with_4408() {
    let harness = Harness::new().await;
    let mut client = Client::connect_with_timeout(harness.addr, TIMEOUT_WINDOW_IO).await;
    let started = Instant::now();
    assert_eq!(client.expect_close().await, 4408, "握手超时以 4408 关闭");
    assert!(
        started.elapsed() >= HANDSHAKE_TIMEOUT,
        "关闭必须发生在 15 秒窗口之后，实际 {:?}",
        started.elapsed()
    );
    assert!(
        harness
            .audit_rows(vec![AuditAction::NodeAuthFailed])
            .await
            .is_empty(),
        "还不知道对端身份的时刻不写认证失败审计（无法归因）"
    );
    assert_eq!(harness.conn.registry().active(), 0);
    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// R41/R42/R43：认证结果
// ---------------------------------------------------------------------------------------------

/// [R41]：签名被破坏的 proof → `proof_invalid` + 4401 + `node.auth_failed` 审计，信任记录不变。
#[tokio::test]
async fn an_invalid_proof_is_closed_with_4401_and_audited() {
    let harness = Harness::new().await;
    let pairing = harness.approved_pairing().await;
    let mut client = Client::connect(harness.addr).await;
    send_hello(
        &mut client,
        ACCESS_NODE,
        &declared_features(),
        &[REQUIRED_FEATURE],
    )
    .await;
    let challenge = client.expect_type("node.challenge").await;
    send_proof(&mut client, ACCESS_NODE, &challenge, true).await;
    let error = client.expect_error("nodelink.auth.proof_invalid").await;
    assert_eq!(error["retryable"], json!(false));
    assert_eq!(client.expect_close().await, 4401, "证明无效以 4401 关闭");

    let rows = harness.audit_rows(vec![AuditAction::NodeAuthFailed]).await;
    assert_eq!(rows.len(), 1, "认证失败必须留痕：{rows:?}");
    assert_eq!(rows[0].outcome(), AuditOutcome::Failed);
    assert_eq!(
        rows[0].actor(),
        &acp_core::model::Actor::Node {
            node: node(ACCESS_NODE),
            access_node: node(ACCESS_NODE),
        }
    );
    assert_eq!(rows[0].target(), &EntityRef::Node(node(ACCESS_NODE)));
    assert_eq!(rows[0].via_node(), Some(&node(ACCESS_NODE)));
    assert_eq!(
        rows[0].detail_digest(),
        None,
        "失败原因不进审计行，只进日志"
    );

    // 「信任记录不受影响」：配对仍是 approved（未被消费）、节点仍是 paired、没有连接登记。
    assert_eq!(
        harness
            .world
            .trust
            .pairing(&pairing)
            .map(|record| record.state()),
        Some(PairingState::Approved)
    );
    assert_eq!(
        harness.world.trust.nodes_of(ACCESS_NODE)[0].state(),
        NodeState::Paired
    );
    assert_eq!(harness.conn.registry().active(), 0);
    harness.stop().await;
}

/// [R41] 的第二个分支：nonce 对不上（签名本身有效，但为另一个挑战签发）同样 `proof_invalid`。
#[tokio::test]
async fn a_proof_for_another_challenge_is_rejected_with_the_same_code() {
    let harness = Harness::new().await;
    let _ = harness.approved_pairing().await;
    let mut client = Client::connect(harness.addr).await;
    send_hello(
        &mut client,
        ACCESS_NODE,
        &declared_features(),
        &[REQUIRED_FEATURE],
    )
    .await;
    let challenge = client.expect_type("node.challenge").await;
    // 用另一个 serverNonce 装配 proof（等价于把挑战 A 的证明重放到挑战 B）。
    let mut tampered = challenge.clone();
    tampered["body"]["serverNonce"] = json!(nonce_text(0x77));
    send_proof(&mut client, ACCESS_NODE, &tampered, false).await;
    let error = client.expect_error("nodelink.auth.proof_invalid").await;
    assert_eq!(error["code"], json!("nodelink.auth.proof_invalid"));
    assert_eq!(client.expect_close().await, 4401);
    assert_eq!(
        harness
            .audit_rows(vec![AuditAction::NodeAuthFailed])
            .await
            .len(),
        1
    );
    harness.stop().await;
}

/// [R42]：已撤销节点即使交出签名有效的 proof 也被拒 → `node_revoked` + 4410。
#[tokio::test]
async fn a_revoked_node_is_closed_with_4410() {
    let harness = Harness::new().await;
    let _ = harness.approved_pairing().await;
    harness.revoke_node(ACCESS_NODE).await;
    let mut client = Client::connect(harness.addr).await;
    send_hello(
        &mut client,
        ACCESS_NODE,
        &declared_features(),
        &[REQUIRED_FEATURE],
    )
    .await;
    let challenge = client.expect_type("node.challenge").await;
    // 签名仍然有效：验签材料作为墓碑保留（撤销不删公钥，§11.6 第 5 条）。
    send_proof(&mut client, ACCESS_NODE, &challenge, false).await;
    let error = client.expect_error("nodelink.auth.node_revoked").await;
    assert_eq!(error["retryable"], json!(false));
    assert_eq!(client.expect_close().await, 4410, "已撤销节点以 4410 关闭");
    let rows = harness.audit_rows(vec![AuditAction::NodeAuthFailed]).await;
    assert_eq!(rows.len(), 1, "被撤销节点的连接尝试要留痕");
    assert_eq!(rows[0].outcome(), AuditOutcome::Denied);
    assert_eq!(harness.conn.registry().active(), 0, "不得进入业务阶段");
    harness.stop().await;
}

/// [R43]：从未配对的节点照常拿到挑战（不用错误区分存在性），proof 阶段失败并映射为 `node_unknown`。
#[tokio::test]
async fn an_unknown_node_gets_a_challenge_and_fails_as_node_unknown() {
    let harness = Harness::new().await;
    let mut client = Client::connect(harness.addr).await;
    send_hello(
        &mut client,
        UNKNOWN_NODE,
        &declared_features(),
        &[REQUIRED_FEATURE],
    )
    .await;
    // 未知节点也照常签发挑战（用 `daemon.public_origin` 推出本机 endpoint）。
    let challenge = client.expect_type("node.challenge").await;
    assert_no_connection_fields(&challenge);
    assert!(
        !feature_ids(&challenge["body"]).is_empty(),
        "未知节点不因此降低 feature 协商结果"
    );
    send_proof(&mut client, UNKNOWN_NODE, &challenge, false).await;
    let error = client.expect_error("nodelink.auth.node_unknown").await;
    assert_eq!(error["retryable"], json!(false));
    assert_eq!(client.expect_close().await, 4401);
    let rows = harness.audit_rows(vec![AuditAction::NodeAuthFailed]).await;
    assert_eq!(rows.len(), 1, "未知节点的尝试同样留痕");
    assert_eq!(rows[0].outcome(), AuditOutcome::Denied);
    assert_eq!(
        harness.world.trust.node_count(),
        0,
        "未知节点的失败不得创建信任行"
    );
    let view = harness.view(UNKNOWN_NODE).await;
    assert!(view.node.is_none() && view.public_key.is_none() && view.pairing.is_none());
    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// R44/R45/R46：limits
// ---------------------------------------------------------------------------------------------

/// [R44]/[R45]/[R46]：`node.ready.limits` 只下调、与连接内部判定同一份数字，固定常量不受配置影响。
#[tokio::test]
async fn node_ready_echoes_the_negotiated_limits_and_never_raises_them() {
    let harness = Harness::start(
        TestWorld::with_store(Arc::new(FixedStore::new(TEST_SERVER_EPOCH, HEAD_SEQUENCE))),
        NodeLinkConfig {
            public_origin: Some(TEST_PUBLIC_ORIGIN.to_owned()),
            // 三个下调项 + 两个试图上调的配置。
            max_in_flight_commands: 8,
            heartbeat_interval_ms: 15_000,
            max_pending_queue_messages: 10,
            max_message_bytes: u64::MAX,
            catalog_snapshot_batch_size: 4_096,
            ..NodeLinkConfig::default()
        },
    )
    .await;
    let _ = harness.approved_pairing().await;
    let mut client = Client::connect(harness.addr).await;
    let ready = authenticated(&mut client, ACCESS_NODE, false).await;

    assert_eq!(
        ready["body"]["limits"],
        json!({
            "maxMessageBytes": DEFAULT_MAX_MESSAGE_BYTES,
            "catalogSnapshotBatchSize": DEFAULT_CATALOG_SNAPSHOT_BATCH_SIZE,
            "resourceSnapshotBatchSize": DEFAULT_RESOURCE_SNAPSHOT_BATCH_SIZE,
            "maxInFlightCommands": 8,
            "maxPendingQueueBytes": DEFAULT_MAX_PENDING_QUEUE_BYTES,
            "maxPendingQueueMessages": 10,
            "heartbeatIntervalMs": 15_000,
        }),
        "下调生效、上调回落默认值（R45），未出现的项取默认值"
    );

    // 下发的数字就是连接内部判定用的数字（避免「下发一套、自己按另一套」）。
    let handle = harness.conn.registry().handles()[0].clone();
    assert_eq!(handle.limits().max_in_flight_commands(), 8);
    assert_eq!(handle.limits().max_pending_queue_messages(), 10);
    assert_eq!(
        handle.limits().heartbeat_interval(),
        Duration::from_millis(15_000)
    );
    assert_eq!(
        handle.limits().max_pending_queue_bytes(),
        DEFAULT_MAX_PENDING_QUEUE_BYTES
    );
    harness.stop().await;
}

/// [R46]：握手超时与心跳静默超时是固定常量（15 s / 90 s），不是配置项、也不随协商变化。
#[test]
fn protocol_constants_are_not_configurable() {
    assert_eq!(HANDSHAKE_TIMEOUT, Duration::from_secs(15));
    assert_eq!(HEARTBEAT_SILENCE_TIMEOUT, Duration::from_secs(90));
    // 配置里根本没有这两个键：`NodeLinkConfig` 只有 §2.5 表里标为「可下调」的七项 + `public_origin`。
    let config = NodeLinkConfig {
        max_message_bytes: u64::MAX,
        catalog_snapshot_batch_size: u64::MAX,
        resource_snapshot_batch_size: u64::MAX,
        max_in_flight_commands: u64::MAX,
        max_pending_queue_bytes: u64::MAX,
        max_pending_queue_messages: u64::MAX,
        heartbeat_interval_ms: u64::MAX,
        public_origin: None,
    };
    let limits = crate::node_link::conn::SessionLimits::negotiate(&config);
    assert!(
        limits.max_message_bytes() <= DEFAULT_MAX_MESSAGE_BYTES
            && u64::from(limits.max_in_flight_commands()) <= DEFAULT_MAX_IN_FLIGHT_COMMANDS
            && limits.max_pending_queue_messages() <= DEFAULT_MAX_PENDING_QUEUE_MESSAGES as usize
            && limits.heartbeat_interval() <= Duration::from_millis(DEFAULT_HEARTBEAT_INTERVAL_MS),
        "任何配置都只能下调，不可能把任何限额抬到 §2.5 默认值以上"
    );
}

// ---------------------------------------------------------------------------------------------
// R47/R48/R49/R50：信封、序号与 type
// ---------------------------------------------------------------------------------------------

/// [R47]/[R48]/[R49]/[R50]：已认证连接上的信封/序号/type 违规判定。
///
/// 断言的两件事同等重要：（1）错误码与信封形状符合 §2.2/§14.1；（2）消息级拒绝**不关闭**连接——
/// 每次拒绝后都跑一次 `link.ping`/`link.pong` 往返，且被拒绝的消息不推进入站序号。
#[tokio::test]
async fn envelope_and_sequence_violations_are_rejected_without_closing() {
    let harness = Harness::new().await;
    let _ = harness.approved_pairing().await;
    let mut client = Client::connect(harness.addr).await;
    let _ = authenticated(&mut client, ACCESS_NODE, false).await;

    client.step("① 序号跳号");
    // ① 序号跳号（期望 1，给 2）→ sequence_invalid。
    let mut envelope = client.envelope("link.ping", json!({"nonce": ping_nonce(0x51)}));
    envelope["connectionSequence"] = json!("2");
    client.send_raw(envelope).await;
    client
        .expect_error("nodelink.protocol.sequence_invalid")
        .await;
    client.ping_round_trip(0x52).await;

    client.step("② 序号回退");
    // ② 序号回退（期望 2，给 1）→ sequence_invalid（R48）。
    let mut envelope = client.envelope("link.ping", json!({"nonce": ping_nonce(0x53)}));
    envelope["connectionSequence"] = json!("1");
    client.send_raw(envelope).await;
    client
        .expect_error("nodelink.protocol.sequence_invalid")
        .await;
    client.ping_round_trip(0x54).await;

    client.step("③ connectionId 不匹配");
    // ③ connectionId 不匹配 → sequence_invalid。
    let mut envelope = client.envelope("link.ping", json!({"nonce": ping_nonce(0x55)}));
    envelope["connectionId"] = json!(uuid_text());
    client.send_raw(envelope).await;
    client
        .expect_error("nodelink.protocol.sequence_invalid")
        .await;
    client.ping_round_trip(0x56).await;

    client.step("④ 只给 connectionId");
    // ④ 只给 connectionId、不给 connectionSequence → schema_invalid（信封形状错）。
    let envelope = json!({
        "protocolVersion": 1,
        "type": "link.ping",
        "messageId": uuid_text(),
        "connectionId": client.connection_id.clone().expect("connectionId"),
        "body": {"nonce": ping_nonce(0x57)},
    });
    client.send_raw(envelope).await;
    client
        .expect_error("nodelink.protocol.schema_invalid")
        .await;
    client.ping_round_trip(0x58).await;

    client.step("⑤ 未知信封字段");
    // ⑤ 未知信封字段 → schema_invalid（控制信封是 closed object）。
    let mut envelope = client.envelope("link.ping", json!({"nonce": ping_nonce(0x59)}));
    envelope["surprise"] = json!(1);
    client.send_raw(envelope).await;
    client
        .expect_error("nodelink.protocol.schema_invalid")
        .await;
    client.ping_round_trip(0x5a).await;

    client.step("⑥ 未知 body 字段");
    // ⑥ 未知 body 字段 → schema_invalid（R50）。信封本身合法，因此该序号已被对端占用
    //    （§2.2 的序号属于「对端发出的消息」，body 是否被接受不影响它）。
    client
        .send_post_auth("link.ping", json!({"nonce": ping_nonce(0x5b), "extra": 1}))
        .await;
    client
        .expect_error("nodelink.protocol.schema_invalid")
        .await;
    client.advance_after_accepted();
    client.ping_round_trip(0x5c).await;

    client.step("⑦ 非法 JSON");
    // ⑦ 不是合法 JSON → invalid_json（消息不生效、连接可用；信封无法解析，因此序号未被占用）。
    client.send_text("{not json").await;
    client.expect_error("nodelink.protocol.invalid_json").await;
    client.ping_round_trip(0x5d).await;

    client.step("⑧ 未知 type");
    // ⑧ 未知 type → type_unsupported（不泄露「存在但未实现」与「完全未知」的差异）。
    client.send_post_auth("node.future.thing", json!({})).await;
    client
        .expect_error("nodelink.protocol.type_unsupported")
        .await;
    client.ping_round_trip(0x5e).await;

    client.step("⑨ post_mvp");
    // ⑨ post_mvp 消息显式拒绝（R49）：用 manifest 里登记为「schema 合法」的两份 fixture。
    for (index, fixture) in ["catalog-changed.json", "link-backpressure.json"]
        .iter()
        .enumerate()
    {
        let case: Value = read_fixture(&format!("valid/{fixture}"));
        let mut envelope = case.clone();
        envelope["messageId"] = json!(uuid_text());
        envelope["connectionId"] = json!(client.connection_id.clone().expect("connectionId"));
        envelope["connectionSequence"] = json!((client.outbound_sequence + 1).to_string());
        client.send_raw(envelope).await;
        client
            .expect_error("nodelink.protocol.type_unsupported")
            .await;
        // post_mvp 消息的信封合法，因此它的序号已被对端占用（§2.2），连接不能因一次显式拒绝而错位。
        client.advance_after_accepted();
        client.ping_round_trip(0x60 + index as u8).await;
    }

    client.step("⑩ 未实现的 v1 type");
    // ⑩ 本切片（WP4）还没有路由的 v1 业务 type：显式 `type_unsupported` 而不是静默丢弃。
    let case: Value = read_fixture("valid/resource-ack.json");
    let mut envelope = case.clone();
    envelope["messageId"] = json!(uuid_text());
    envelope["connectionId"] = json!(client.connection_id.clone().expect("connectionId"));
    envelope["connectionSequence"] = json!((client.outbound_sequence + 1).to_string());
    client.send_raw(envelope).await;
    client
        .expect_error("nodelink.protocol.type_unsupported")
        .await;
    client.advance_after_accepted();
    client.ping_round_trip(0x7f).await;

    harness.stop().await;
}

/// [R47]：binary 帧不是本协议的载荷 → `invalid_json` + 4400 关闭。
#[tokio::test]
async fn a_binary_frame_is_closed_with_4400() {
    let harness = Harness::new().await;
    let _ = harness.approved_pairing().await;
    let mut client = Client::connect(harness.addr).await;
    let _ = authenticated(&mut client, ACCESS_NODE, false).await;
    client.send_binary(b"\x01\x02\x03").await;
    let error = client.expect_error("nodelink.protocol.invalid_json").await;
    assert_eq!(error["retryable"], json!(false));
    assert_eq!(client.expect_close().await, 4400, "binary 帧以 4400 关闭");
    harness.stop().await;
}

/// 读一份 `fixtures/node-link/v1/` 下的 fixture（路径相对仓库根，与 `CARGO_MANIFEST_DIR` 无关）。
fn read_fixture(relative: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures/node-link/v1")
        .join(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("fixture 必须存在（{}）：{error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("fixture 必须是 JSON（{}）：{error}", path.display()))
}

// ---------------------------------------------------------------------------------------------
// R79/R80/R81：心跳、静默与慢连接
// ---------------------------------------------------------------------------------------------

/// [R79]：按协商的间隔发送 `link.ping`，回填 nonce 的 `link.pong` 让连接继续存活。
///
/// 间隔用 `node.ready.limits.heartbeatIntervalMs = 1_000`（schema 下限）驱动，无需等待真实 30 秒。
#[tokio::test]
async fn heartbeat_round_trip_keeps_the_connection_alive() {
    let harness = Harness::start(
        TestWorld::with_store(Arc::new(FixedStore::new(TEST_SERVER_EPOCH, HEAD_SEQUENCE))),
        NodeLinkConfig {
            public_origin: Some(TEST_PUBLIC_ORIGIN.to_owned()),
            heartbeat_interval_ms: 1_000,
            ..NodeLinkConfig::default()
        },
    )
    .await;
    let _ = harness.approved_pairing().await;
    let mut client = Client::connect(harness.addr).await;
    let _ = authenticated(&mut client, ACCESS_NODE, false).await;

    // Owner 必须主动发心跳（认证后出站序号从 1 开始的 link.ping）。
    let ping = client.expect_type("link.ping").await;
    let heartbeat_sequence = client.server_sequence;
    assert_eq!(
        heartbeat_sequence, 2,
        "心跳是 `node.ready`（1）之后的首个出站消息"
    );
    let nonce = ping["body"]["nonce"].as_str().expect("nonce").to_owned();
    client
        .send_post_auth("link.pong", json!({"nonce": nonce}))
        .await;
    client.advance_after_accepted();
    // 回填之后连接仍可用（心跳不是「发一次就断」）。
    client.ping_round_trip(0x71).await;
    // 下一个心跳周期到达后连接仍然活着（静默计时未误触发）。
    let second = client.expect_type("link.ping").await;
    assert!(
        client.server_sequence > heartbeat_sequence,
        "心跳必须按协商的间隔持续发送"
    );
    assert_ne!(
        second["body"]["nonce"],
        json!(nonce),
        "每次心跳的 nonce 必须不同"
    );
    harness.stop().await;
}

/// [R80]：静默超过窗口（§2.5 固定 90 秒）→ 4408 关闭，信任记录不受影响。
///
/// 窗口用 [`NodeLinkConn::with_test_windows`] 调成 2 秒：90 秒的真实等待在用例里不可接受，而窗口取值由
/// [`protocol_constants_are_not_configurable`] 锁定，本用例证明的是「静默判定 → 4408」这条机制。
#[tokio::test]
async fn silence_beyond_the_window_is_closed_with_4408() {
    let world = TestWorld::with_store(Arc::new(FixedStore::new(TEST_SERVER_EPOCH, HEAD_SEQUENCE)));
    let registry = ConnectionRegistry::new();
    let conn = Arc::new(
        NodeLinkConn::new(
            world.core.clone(),
            world.authority.clone(),
            NodeLinkConfig {
                public_origin: Some(TEST_PUBLIC_ORIGIN.to_owned()),
                // 心跳间隔取 schema 上限以免 ping 掩盖静默判定（静默窗口仍独立计时）。
                heartbeat_interval_ms: 300_000,
                ..NodeLinkConfig::default()
            },
            Arc::clone(&registry),
        )
        .with_test_windows(Duration::from_secs(2), Duration::from_secs(30)),
    );
    let mut listener = NetListener::bind(NetConfig {
        listen: "127.0.0.1:0".to_owned(),
        ..NetConfig::default()
    })
    .await
    .expect("绑定 loopback listener");
    listener
        .register_ws(
            WS_PATH,
            WS_SUBPROTOCOL,
            WsEndpoint::handler(Arc::clone(&conn)),
        )
        .expect("注册 Node Link 端点");
    let addr = listener.local_addrs()[0];
    let (shutdown, signal) = Shutdown::channel();
    let join = tokio::spawn(async move { listener.serve(signal).await });

    let harness = Harness {
        world,
        addr,
        conn,
        shutdown,
        join,
    };
    let pairing = harness.approved_pairing().await;
    let mut client = Client::connect_with_timeout(addr, IO_TIMEOUT).await;
    let _ = authenticated(&mut client, ACCESS_NODE, false).await;
    let started = Instant::now();
    assert_eq!(
        client.expect_close().await,
        4408,
        "静默超过窗口以 4408 关闭"
    );
    assert!(
        started.elapsed() >= Duration::from_secs(2),
        "关闭必须发生在窗口之后"
    );
    assert_eq!(
        harness
            .world
            .trust
            .pairing(&pairing)
            .map(|record| record.state()),
        Some(PairingState::Consumed),
        "静默关闭只影响连接，不改写已经落库的持久化状态"
    );
    assert_eventually(
        || harness.conn.registry().active() == 0,
        "会话结束后必须从注册表摘除",
    )
    .await;
    harness.stop().await;
}

/// [R81]：慢消费者（待发送队列持续高水位）被断开，同 Owner 上其他连接不受影响。
///
/// 队列上限下调到 1 条 / 1 KiB，客户端的读被暂停；慢消费者宽限用 [`NodeLinkConn::with_test_windows`]
/// 调成 1.5 秒（同 [R80] 的理由：30 秒的真实等待在用例里不可接受）。
#[tokio::test]
async fn a_saturated_connection_is_disconnected_without_affecting_its_peer() {
    let world = TestWorld::with_store(Arc::new(FixedStore::new(TEST_SERVER_EPOCH, HEAD_SEQUENCE)));
    let registry = ConnectionRegistry::new();
    let conn = Arc::new(
        NodeLinkConn::new(
            world.core.clone(),
            world.authority.clone(),
            NodeLinkConfig {
                public_origin: Some(TEST_PUBLIC_ORIGIN.to_owned()),
                max_pending_queue_messages: 1,
                max_pending_queue_bytes: 1_024,
                heartbeat_interval_ms: 300_000,
                ..NodeLinkConfig::default()
            },
            Arc::clone(&registry),
        )
        .with_test_windows(Duration::from_secs(90), Duration::from_millis(1_500)),
    );
    let mut listener = NetListener::bind(NetConfig {
        listen: "127.0.0.1:0".to_owned(),
        ..NetConfig::default()
    })
    .await
    .expect("绑定 loopback listener");
    listener
        .register_ws(
            WS_PATH,
            WS_SUBPROTOCOL,
            WsEndpoint::handler(Arc::clone(&conn)),
        )
        .expect("注册 Node Link 端点");
    let addr = listener.local_addrs()[0];
    let (shutdown, signal) = Shutdown::channel();
    let join = tokio::spawn(async move { listener.serve(signal).await });
    let harness = Harness {
        world,
        addr,
        conn,
        shutdown,
        join,
    };
    let _ = harness.approved_pairing().await;

    // ① 快速连接：正常往来，作为「其他连接」的对照组。
    let mut healthy = Client::connect(addr).await;
    let _ = authenticated(&mut healthy, ACCESS_NODE, false).await;

    // ② 慢连接：完成握手后停止读（socket 缓冲被填满 → 会话写阻塞 → 队列达高水位）。
    let mut slow = Client::connect(addr).await;
    let _ = authenticated(&mut slow, ACCESS_NODE, false).await;

    let slow_id = harness
        .conn
        .registry()
        .handles()
        .iter()
        .map(|handle| handle.connection_id().as_str().to_owned())
        .find(|id| id != healthy.connection_id.as_deref().expect("connectionId"))
        .expect("慢连接必须已登记");
    let slow_handle = harness
        .conn
        .registry()
        .handles()
        .into_iter()
        .find(|handle| handle.connection_id().as_str() == slow_id)
        .expect("句柄仍存在");

    // 往慢连接投递大消息直到高水位（WP5 的「停读快照批次」在同一条判定上）。
    let payload = "x".repeat(512);
    let mut saturated = false;
    for _ in 0..4_096 {
        match slow_handle.send(MessageType::LinkError, &json!({"padding": payload})) {
            Ok(()) => {}
            Err(crate::node_link::conn::SendFault::HighWater) => {
                saturated = true;
                break;
            }
            Err(fault) => panic!("未预期的投递失败：{fault:?}"),
        }
    }
    assert!(saturated, "持续投递必须让队列达到高水位");
    assert!(slow_handle.saturated(), "高水位判定必须对投递方可见");
    assert_eq!(
        slow_handle.pending().messages,
        1,
        "队列里必须真的留下了一条未写出的消息（其余被高水位拦下）"
    );

    // ③ 对照组仍能正常往来（R81 的「不影响其他连接」）。
    healthy.ping_round_trip(0x81).await;

    // ④ 持续过慢 → 断开慢连接（宽限 1.5 秒）。
    let deadline = Instant::now() + IO_TIMEOUT;
    while harness.conn.registry().active() > 1 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        harness.conn.registry().active(),
        1,
        "慢连接必须被断开，快连接必须留着"
    );
    assert_eq!(
        harness.conn.registry().handles()[0]
            .connection_id()
            .as_str(),
        healthy.connection_id.as_deref().expect("connectionId"),
        "留下的必须是快连接"
    );
    healthy.ping_round_trip(0x82).await;
    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// manifest fixture 漂移
// ---------------------------------------------------------------------------------------------

/// 消费 `fixtures/node-link/v1/manifest.json` 的每一份用例：本层（信封/握手/错误/心跳）的正反判定。
///
/// 判定口径按 manifest 的 `valid` 与消息族分组——不是「随便挑几份」：
///
/// | 分组 | `valid = true` | `valid = false` |
/// |---|---|---|
/// | WP4 解码的 body 族（`node.hello`/`node.challenge`/`node.proof`/`node.ready`） | 信封判定通过 + body 能解码成类型化 DTO + 往返保真 | 信封/body 至少一层拒绝 |
/// | 握手与心跳的 `link.*` 族 | 同上（`link.error` 认证前无连接字段、认证后有） | 信封判定拒绝（`schema_invalid`） |
/// | WP5/WP6 的业务族 | 信封判定通过（post_mvp 一族按 [R49] 拒绝） | 信封判定通过、body 级拒绝归 WP5/WP6 |
///
/// 新增 fixture 必须落进这三张表之一，否则本用例失败（漂移门禁的意图：manifest 是唯一来源）。
#[test]
fn fixtures_are_consumed_by_the_handshake_and_error_layers() {
    let manifest: Value = read_fixture("manifest.json");
    let cases = manifest["cases"].as_array().expect("manifest.cases");
    assert!(!cases.is_empty(), "manifest 必须有用例");

    // body 级拒绝由 WP5/WP6 承担的反例（本层只断言「信封本身是合法的」）。
    let deferred_body_cases = [
        "invalid/session-create-with-cwd.json",
        "invalid/resource-event-missing-origin.json",
        "invalid/stale-attachment-generation.json",
        "invalid/command-unknown-field.json",
        "invalid/resource-event-missing-session.json",
        "invalid/resource-ack-missing-session.json",
        "invalid/command-terminal-missing-command.json",
        "invalid/session-create-accepted-with-result.json",
    ];

    let mut handshake_bodies = 0usize;
    let mut link_bodies = 0usize;
    let mut routed = 0usize;
    let mut pairing_cases = 0usize;
    for case in cases {
        let fixture = case["fixture"].as_str().expect("fixture path");
        let valid = case["valid"].as_bool().expect("valid");
        let schema = case["schema"].as_str().expect("schema path");
        if schema.ends_with("pairing.schema.json") {
            // 配对请求/响应不是 `message.schema.json` 的消息（WP3 的端点用例已逐条消费）。
            pairing_cases += 1;
            continue;
        }
        let message: Value = read_fixture(fixture);
        let message_type = message["type"]
            .as_str()
            .unwrap_or_else(|| panic!("{fixture} 不是消息 fixture"));
        let text = message.to_string();
        let has_connection = message.get("connectionId").is_some();
        let phase = if has_connection {
            node_link_protocol::envelope::Phase::PostAuth
        } else {
            node_link_protocol::envelope::Phase::PreAuth
        };
        let connection_id = message
            .get("connectionId")
            .and_then(|value| Uuid::parse(value.as_str()?).ok());
        let sequence = message
            .get("connectionSequence")
            .and_then(|value| value.as_str().and_then(|text| text.parse::<u64>().ok()));
        let (judged, next) = crate::node_link::conn::wire::judge(
            &text,
            phase,
            Some(sequence.unwrap_or(1)),
            connection_id.as_ref(),
        );

        match (fixture_kind(message_type), valid) {
            (FixtureKind::Handshake | FixtureKind::Link, true) => {
                match fixture_kind(message_type) {
                    FixtureKind::Handshake => handshake_bodies += 1,
                    _ => link_bodies += 1,
                }
                let envelope = match judged {
                    crate::node_link::conn::wire::Inbound::Message(envelope) => envelope,
                    other => panic!("{fixture} 必须是本层接受的消息，实际：{}", describe(&other)),
                };
                if has_connection {
                    assert_eq!(next, sequence.map(|value| value + 1), "{fixture}");
                }
                // body 必须能用类型化 DTO 解码，且往返 JSON 与原文等价（未知字段在 closed object 上必失败）。
                let body = envelope.body().get();
                let round_trip = decode_known_body(message_type, body)
                    .unwrap_or_else(|error| panic!("{fixture}：类型化 body 被拒：{error}"));
                assert_eq!(round_trip, message["body"], "{fixture} 往返必须保真");
            }
            (FixtureKind::Handshake, false) => {
                // 握手 body 由本层类型化解码：信封合法、缺字段在 body 级被拒（不是信封级）。
                let envelope = match judged {
                    crate::node_link::conn::wire::Inbound::Message(envelope) => envelope,
                    other => panic!(
                        "{fixture} 的信封必须合法（缺字段是 body 级问题）：{}",
                        describe(&other)
                    ),
                };
                assert!(
                    decode_known_body(message_type, envelope.body().get()).is_err(),
                    "{fixture} 的 body 必须被类型化解码拒绝"
                );
            }
            (FixtureKind::Link, false) => {
                assert!(
                    matches!(
                        judged,
                        crate::node_link::conn::wire::Inbound::Rejected { .. }
                            | crate::node_link::conn::wire::Inbound::Fatal { .. }
                    ),
                    "{fixture} 必须被本层拒绝"
                );
            }
            (FixtureKind::Routed, valid) => {
                routed += 1;
                let post_mvp = matches!(
                    message_type,
                    "catalog.changed"
                        | "resource.detach"
                        | "node.rotate-key.request"
                        | "node.rotate-key.result"
                        | "link.backpressure"
                );
                if post_mvp {
                    assert!(
                        matches!(
                            judged,
                            crate::node_link::conn::wire::Inbound::Rejected { .. }
                        ),
                        "{fixture} 是 post_mvp：必须被显式拒绝（R49）"
                    );
                } else {
                    assert!(
                        matches!(judged, crate::node_link::conn::wire::Inbound::Message(_)),
                        "{fixture} 的信封必须合法（body 级判定归 WP5/WP6）"
                    );
                    if !valid {
                        assert!(
                            deferred_body_cases.contains(&fixture),
                            "{fixture} 未在用例的「body 级反例」清单里登记：新增 fixture 必须显式归类"
                        );
                    }
                }
            }
        }
    }

    // 三类都真的被走到（避免清单为空导致用例静默通过）。
    assert!(
        handshake_bodies >= 3,
        "握手 body 的用例数：{handshake_bodies}"
    );
    assert!(link_bodies >= 2, "link.* 的用例数：{link_bodies}");
    assert!(routed >= 10, "业务族的用例数：{routed}");
    assert_eq!(pairing_cases, 2, "配对 fixture 恰好两份（claim/status）");
}

/// fixture 的消息族。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixtureKind {
    /// WP4 解码的握手 body（`node.hello`/`node.challenge`/`node.proof`/`node.ready`）。
    Handshake,
    /// 握手阶段与心跳的 `link.*`（`link.error`/`link.ping`/`link.pong`）。
    Link,
    /// WP5/WP6 的业务族（作为信封级输入经本层）。
    Routed,
}

fn fixture_kind(message_type: &str) -> FixtureKind {
    match message_type {
        "node.hello" | "node.challenge" | "node.proof" | "node.ready" => FixtureKind::Handshake,
        "link.error" | "link.ping" | "link.pong" => FixtureKind::Link,
        _ => FixtureKind::Routed,
    }
}

/// 把 body 解码成类型化 DTO 并重新编码成 JSON（未知字段在 `deny_unknown_fields` 上必然失败）。
fn decode_typed_body<T>(body: &str) -> Result<Value, serde_json::Error>
where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let decoded: T = serde_json::from_str(body)?;
    serde_json::to_value(&decoded)
}

/// 本层解码的 handshake/`link.*` body 的类型化往返（`Err` = 该 body 在本层的 DTO 上不合法）。
///
/// `match` 穷尽 WP4 的 body 族：新增家族成员时这里会编译失败，不会静默落到 `panic` 分支之外的缺口。
fn decode_known_body(message_type: &str, body: &str) -> Result<Value, serde_json::Error> {
    match message_type {
        "node.hello" => decode_typed_body::<node_link_protocol::handshake::NodeHello>(body),
        "node.challenge" => decode_typed_body::<node_link_protocol::handshake::NodeChallenge>(body),
        "node.proof" => decode_typed_body::<node_link_protocol::handshake::NodeProof>(body),
        "node.ready" => decode_typed_body::<node_link_protocol::handshake::NodeReady>(body),
        "link.ping" => decode_typed_body::<node_link_protocol::error::Ping>(body),
        "link.pong" => decode_typed_body::<node_link_protocol::error::Pong>(body),
        "link.error" => decode_typed_body::<node_link_protocol::error::Body>(body),
        other => panic!("{other} 的类型不在 WP4 的 body 族里"),
    }
}

/// 判定结果的可读描述（用例失败时给出原因）。
fn describe(inbound: &crate::node_link::conn::wire::Inbound) -> String {
    match inbound {
        crate::node_link::conn::wire::Inbound::Message(_) => "接受".to_owned(),
        crate::node_link::conn::wire::Inbound::Rejected { code, .. } => {
            format!("消息级拒绝（{}）", code.as_str())
        }
        crate::node_link::conn::wire::Inbound::Fatal { code, .. } => {
            format!("致命拒绝（{}）", code.as_str())
        }
    }
}
