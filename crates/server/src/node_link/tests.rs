//! `server::node_link::pairing` 的行为用例（`[PV3]` 的 WP3 部分）。
//!
//! 驱动方式：真实 loopback listener（只经 `transport::net` 的公开形状注册处理器）+ 本模块自带的极简
//! HTTP/1.1 客户端，因此 path 路由、Host 边界、请求体上限与响应头都在真实连接上被验证。配对侧的事实由
//! `local_admin::test_support::TestWorld` 的端口替身提供（同一 crate 的测试设施，不是第二个实现；生产
//! 路径不跨 adapter 调用），本机用户的动作（`node.pair.begin`/`node.pair.confirm`）经 `local_admin` 的
//! 路由执行——HTTPS claim 路径正是它当初预留的那条。
//!
//! 覆盖映射（`specs/node-link-pairing-http/spec.md` 的 R19–R35）见本文件的用例名与 handoff 的映射表：
//!
//! | 需求 | 用例 |
//! |---|---|
//! | [R17] 配对请求体超限 | `claim_body_over_the_transport_limit_is_rejected_with_413` |
//! | [R19]/[R20] claim 成功路径 | `claim_enters_pending_confirmation_and_returns_a_verifiable_owner_proof` |
//! | [R21] 重复 claim 被拒绝 | `repeated_claim_from_another_access_node_is_rejected_with_409` |
//! | [R22] 失败语义与状态码 | `malformed_claim_body_is_rejected_with_400`、`unknown_pairing_is_rejected_with_404`、`endpoint_host_outside_the_configured_origin_is_rejected_with_403`、`expired_pairing_is_rejected_with_410`、`claiming_without_a_configured_origin_is_rejected_with_403` |
//! | [R23] proof 无效不泄露差异 | `invalid_proof_is_unauthorized_without_leaking_the_difference` |
//! | [R24] 相同内容重试幂等 | `retrying_the_same_claim_returns_the_original_pairing_request` |
//! | [R25] 过期配对 410 | `expired_pairing_is_rejected_with_410` |
//! | [R30]/[R31] 安全响应头与凭据边界 | `every_pairing_response_carries_the_four_security_headers`、`pairing_responses_never_carry_the_pairing_secret` |
//! | [R34] claim 超限 429 | `claim_rate_limit_returns_429_after_ten_attempts_per_ip` |

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use acp_core::model::{NodeId, NodeKind, Nonce, PairingId, PairingState, PeerPublicKey, Timestamp};
use acp_core::ports::TrustStore as _;
use base64::Engine as _;
use identity_auth::{
    NodeLinkPairingOwnerProof, NodeLinkPairingProof, P1363Signature, PairingRequestId,
    PairingSecret,
};
use node_link_protocol::common::{Base64Url, NonEmptyText, ProtocolVersionV1, Uuid};
use node_link_protocol::pairing::{AccessNodeKind, ClaimRequest, Endpoint, QrPayload};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;

use crate::local_admin::envelope::{AdminOutcome, AdminRequest, AdminResponse, decode_request};
use crate::local_admin::handler::LocalAdminHandler as _;
use crate::local_admin::method::Method;
use crate::local_admin::test_support::{TestWorld, test_nonce, test_public_key};
use crate::node_link::pairing::{CLAIM_PATH, PairingHttp, PairingHttpConfig};
use crate::transport::net::{NetConfig, NetError, NetListener, Shutdown, ShutdownHandle};

/// 测试用 Access Node 标识（`NODE_LINK_PROTOCOL.md` §13.2 的 `accessNodeId`）。
const ACCESS_NODE: &str = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";
/// 另一个 Access Node（重复 claim 用例）。
const OTHER_ACCESS_NODE: &str = "9c8f6b1d-7a35-4f0b-9b6a-2f6d5c4e3b1a";
/// 本节点标识（与 `TestWorld` 装配的 `Authority` 同源）。
const OWNER_NODE: &str = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
/// 单次 I/O 的超时（用例失败要快速失败，不挂住测试进程）。
const IO_TIMEOUT: Duration = Duration::from_secs(10);
/// 四个安全响应头（§13.1）。
const SECURITY_HEADERS: [(&str, &str); 4] = [
    ("cache-control", "no-store"),
    ("pragma", "no-cache"),
    ("referrer-policy", "no-referrer"),
    ("x-content-type-options", "nosniff"),
];

// ---------------------------------------------------------------------------------------------
// 测试世界
// ---------------------------------------------------------------------------------------------

/// 一次已登记（`created`）的配对：`node.pair.begin` 的结果 + 二维码里的一次性 secret。
struct Pairing {
    id: String,
    secret: PairingSecret,
    endpoint: String,
    expires_at: String,
}

/// 端口替身世界 + claim 端点 + 真实 loopback listener。
struct Harness {
    world: TestWorld,
    addr: SocketAddr,
    shutdown: ShutdownHandle,
    join: JoinHandle<Result<(), NetError>>,
}

impl Harness {
    /// 起一个已注册 claim 端点的接入层（`127.0.0.1:0`，端口由内核分配）。
    async fn start(world: TestWorld, config: PairingHttpConfig) -> Self {
        let pairing = Arc::new(PairingHttp::new(
            world.core.clone(),
            world.authority.clone(),
            config,
        ));
        let mut listener = NetListener::bind(NetConfig {
            listen: "127.0.0.1:0".to_owned(),
            ..NetConfig::default()
        })
        .await
        .expect("绑定 loopback listener");
        listener
            .register_post(CLAIM_PATH, pairing.claim_handler())
            .expect("注册 claim 端点");
        let addr = listener.local_addrs()[0];
        let (shutdown, signal) = Shutdown::channel();
        let join = tokio::spawn(async move { listener.serve(signal).await });
        Self {
            world,
            addr,
            shutdown,
            join,
        }
    }

    /// 默认配置（`daemon.public_origin` 就是 `TestWorld` 的 canonical origin）。
    async fn with_origin() -> Self {
        Self::start(
            TestWorld::new(),
            PairingHttpConfig {
                public_origin: Some(
                    crate::local_admin::test_support::TEST_PUBLIC_ORIGIN.to_owned(),
                ),
            },
        )
        .await
    }

    /// 结束接入层（用例末尾调用；端口随 listener 释放）。
    async fn stop(self) {
        self.shutdown.trigger();
        let _ = tokio::time::timeout(IO_TIMEOUT, self.join).await;
    }

    /// 本机用户登记一次节点配对（`node.pair.begin`），返回二维码里的公开字段与 secret。
    async fn begin(&self) -> Pairing {
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
        let result = success(&response);
        let url = result["pairingUrl"].as_str().expect("pairingUrl");
        let payload: QrPayload = serde_json::from_slice(&qr_payload(url)).expect("二维码 payload");
        Pairing {
            id: result["pairingId"].as_str().expect("pairingId").to_owned(),
            secret: PairingSecret::try_from_bytes(payload.pairing_secret.as_bytes())
                .expect("32 字节 pairing secret"),
            endpoint: payload.endpoint.as_str().to_owned(),
            expires_at: payload.expires_at.as_str().to_owned(),
        }
    }

    /// 发送一条 claim 请求（`application/json`）。
    async fn claim(&self, body: &[u8]) -> Response {
        self.claim_with(body, Some("application/json")).await
    }

    /// 发送一条 claim 请求，可指定 `Content-Type`（`None` = 不带该头）。
    async fn claim_with(&self, body: &[u8], content_type: Option<&str>) -> Response {
        self.post(CLAIM_PATH, body, content_type).await
    }

    /// 发送一个 POST 请求到已注册的 path。
    async fn post(&self, path: &str, body: &[u8], content_type: Option<&str>) -> Response {
        send(self.addr, path, body, content_type).await
    }

    /// 该配对的持久化记录（状态断言用）。
    fn record(&self, pairing: &Pairing) -> acp_core::model::PairingRecord {
        self.world
            .trust
            .pairing(&pairing.id)
            .expect("配对必须已落库")
    }
}

// ---------------------------------------------------------------------------------------------
// 请求/响应
// ---------------------------------------------------------------------------------------------

/// 解析出的 HTTP 响应。
struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Response {
    /// 按名字取响应头（大小写不敏感）。
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// 响应体的 JSON（失败时给出可读的消息）。
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|error| {
            panic!(
                "响应体不是 JSON（{error}）：{}",
                String::from_utf8_lossy(&self.body)
            )
        })
    }

    /// 响应体的 JSON 文本（断言「不出现 secret」用）。
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// 断言四个安全头齐全（§13.1/§13.4）。
    fn assert_security_headers(&self) {
        for (name, expected) in SECURITY_HEADERS {
            assert_eq!(
                self.header(name),
                Some(expected),
                "配对响应（{}）必须携带 {name}: {expected}",
                self.status
            );
        }
    }
}

/// 发一条 HTTP/1.1 请求并读回一个响应。
///
/// 写失败被容忍（`413` 这类响应会在服务端提前关闭连接时让写入报 `EPIPE`）：此时仍然必须能读到响应，
/// 否则用例会因「没有响应」而明确失败。
async fn send(addr: SocketAddr, path: &str, body: &[u8], content_type: Option<&str>) -> Response {
    let mut stream = tokio::time::timeout(IO_TIMEOUT, TcpStream::connect(addr))
        .await
        .expect("连接超时")
        .expect("连接 loopback");
    let mut request = format!(
        "POST {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n",
        host_of(addr)
    )
    .into_bytes();
    if let Some(content_type) = content_type {
        request.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
    }
    request.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
    request.extend_from_slice(body);
    let _ = tokio::time::timeout(IO_TIMEOUT, stream.write_all(&request)).await;

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
    parse_response(&raw)
}

/// 解析 HTTP/1.1 响应（状态行 + 头 + 按 `Content-Length` 的体）。
fn parse_response(raw: &[u8]) -> Response {
    let head_end = find_subsequence(raw, b"\r\n\r\n")
        .unwrap_or_else(|| panic!("响应缺少 header 结束标记：{}", String::from_utf8_lossy(raw)));
    let head = String::from_utf8_lossy(&raw[..head_end]);
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
        .collect();
    let length = content_length(&raw[..head_end]);
    let body = raw
        .get(head_end + 4..head_end + 4 + length)
        .unwrap_or_default()
        .to_vec();
    Response {
        status,
        headers,
        body,
    }
}

/// `Content-Length`（缺失或非法按 0 处理：本接入层的有体响应都带该头）。
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

/// `Host` 头取值（默认配置只接受 loopback 形态）。
fn host_of(addr: SocketAddr) -> String {
    addr.to_string()
}

// ---------------------------------------------------------------------------------------------
// 配对与请求的构造（测试扮演 Access Node）
// ---------------------------------------------------------------------------------------------

/// `node.pair.begin` 的响应必须成功（失败时给出可读的消息）。
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

/// 二维码 URL 的 fragment（`#data=<无填充 base64url(json)>`）。
fn qr_payload(url: &str) -> Vec<u8> {
    let data = url.split("#data=").nth(1).expect("URL 带 data fragment");
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(data)
        .expect("fragment 是无填充 base64url")
}

fn uuid(text: &str) -> Uuid {
    Uuid::parse(text).expect("测试用 UUID 文本必须规范")
}

fn node(text: &str) -> NodeId {
    NodeId::new(text).expect("测试用 NodeId 文本必须规范")
}

fn pairing_id(text: &str) -> PairingId {
    PairingId::new(text).expect("测试用 PairingId 文本必须规范")
}

/// 32 字节 nonce 的规范无填充 base64url 文本（末字节的低 2 位固定为 0）。
fn nonce_text(seed: u8) -> String {
    let mut bytes = [seed; 32];
    bytes[31] &= 0b1111_1100;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// 测试扮演的 Access Node 公钥（基点 G 的 SEC1 未压缩编码）。
fn access_public_key() -> PeerPublicKey {
    test_public_key()
}

fn wire_public_key(key: &PeerPublicKey) -> Base64Url<65> {
    Base64Url::parse(&base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key.as_bytes()))
        .expect("65 字节 SEC1 公钥的 base64url")
}

fn wire_nonce(text: &str) -> Base64Url<32> {
    Base64Url::parse(text).expect("32 字节 nonce 的 base64url")
}

fn wire_proof(proof: &identity_auth::PairingProof) -> Base64Url<32> {
    Base64Url::parse(&base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(proof.as_bytes()))
        .expect("32 字节 proof 的 base64url")
}

/// 构造一次 claim 请求：proof 用二维码里的 secret 对 `node-link-pairing-proof/v1` 计算。
fn claim_request(
    pairing: &Pairing,
    access_node: &str,
    node_name: &str,
    endpoint: &str,
    client_nonce: &str,
) -> ClaimRequest {
    let client_nonce = Nonce::new(client_nonce).expect("规范 nonce");
    let public_key = access_public_key();
    let proof = NodeLinkPairingProof {
        owner_node_id: node(OWNER_NODE),
        access_node_id: node(access_node),
        pairing_id: pairing_id(&pairing.id),
        pairing_expires_at: Timestamp::new(&pairing.expires_at).expect("规范时间戳"),
        access_public_key: public_key.clone(),
        client_nonce: client_nonce.clone(),
        node_name: node_name.to_owned(),
        node_kind: NodeKind::Access,
    }
    .hmac(&pairing.secret)
    .expect("证明必须可计算");
    ClaimRequest {
        protocol_version: ProtocolVersionV1::new(1).expect("v1"),
        pairing_id: uuid(&pairing.id),
        owner_node_id: uuid(OWNER_NODE),
        access_node_id: uuid(access_node),
        node_name: NonEmptyText::parse(node_name).expect("节点名合法"),
        node_kind: AccessNodeKind,
        endpoint: Endpoint::parse(endpoint).expect("endpoint 合法"),
        access_public_key: wire_public_key(&public_key),
        client_nonce: wire_nonce(client_nonce.as_str()),
        proof: wire_proof(&proof),
    }
}

/// 一次合法 claim 的请求体（Access Node 与 client nonce 可参数化）。
fn claim_body(pairing: &Pairing, access_node: &str, client_nonce: &str) -> Vec<u8> {
    let request = claim_request(
        pairing,
        access_node,
        "Office Access",
        &pairing.endpoint.clone(),
        client_nonce,
    );
    serde_json::to_vec(&request).expect("claim 请求可序列化")
}

/// 断言响应里的 `ownerProof` 可由二维码里的 `ownerPublicKey` 验证（R20）。
async fn assert_owner_proof_verifies(
    world: &TestWorld,
    pairing: &Pairing,
    body: &Value,
    client_nonce: &str,
) {
    let owner_public_key = world
        .authority
        .node_public_key()
        .await
        .expect("本节点公钥可用");
    let signature = P1363Signature::try_from_base64url(
        body["ownerProof"].as_str().expect("ownerProof 是字符串"),
    )
    .expect("ownerProof 是 64 字节 P1363");
    let input = NodeLinkPairingOwnerProof {
        owner_node_id: node(OWNER_NODE),
        access_node_id: node(ACCESS_NODE),
        pairing_id: pairing_id(&pairing.id),
        owner_public_key: owner_public_key.clone(),
        access_public_key: access_public_key(),
        client_nonce: Nonce::new(client_nonce).expect("规范 nonce"),
        server_nonce: Nonce::new(body["serverNonce"].as_str().expect("serverNonce"))
            .expect("serverNonce 是规范 nonce"),
        pairing_request_id: PairingRequestId::parse(
            body["pairingRequestId"].as_str().expect("pairingRequestId"),
        )
        .expect("pairingRequestId 是规范 UUID"),
    };
    input
        .verify(&owner_public_key, &signature)
        .expect("ownerProof 必须由二维码里的 ownerPublicKey 验证通过");
}

/// 错误 body 的稳定部分（`correlationId` 每次请求不同，不参与比较）。
fn error_shape(response: &Response) -> Value {
    let body = response.json();
    json!({
        "code": body["code"],
        "message": body["message"],
        "retryable": body["retryable"],
        "details": body["details"],
    })
}

// ---------------------------------------------------------------------------------------------
// R19/R20：claim 成功路径
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn claim_enters_pending_confirmation_and_returns_a_verifiable_owner_proof() {
    let harness = Harness::with_origin().await;
    let pairing = harness.begin().await;
    let nonce = nonce_text(0x01);

    let response = harness
        .claim(&claim_body(&pairing, ACCESS_NODE, &nonce))
        .await;
    assert_eq!(response.status, 201, "首次 claim 成功返回 201（§13.4）");
    assert_eq!(
        response.header("content-type").map(str::to_owned),
        Some("application/json".to_owned())
    );
    response.assert_security_headers();
    let body = response.json();
    assert_eq!(body["protocolVersion"], json!(1));
    assert_eq!(body["status"], json!("pending_confirmation"));
    assert_eq!(body["expiresAt"], json!(pairing.expires_at));
    assert_owner_proof_verifies(&harness.world, &pairing, &body, &nonce).await;

    // 服务器签发的 `serverNonce`/`pairingRequestId` 就是本机内存里的配对材料。
    let material = harness
        .world
        .authority
        .pairing_request_material(&pairing_id(&pairing.id))
        .expect("claim 之后仍有配对材料");
    assert_eq!(
        body["pairingRequestId"].as_str().expect("pairingRequestId"),
        material.pairing_request_id.as_str()
    );
    assert_eq!(
        body["serverNonce"].as_str().expect("serverNonce"),
        material.server_nonce.as_str()
    );

    // 配对推进到 `pending_confirmation`，对端行已固定。
    let record = harness.record(&pairing);
    assert_eq!(record.state(), PairingState::PendingConfirmation);
    let peer = harness
        .world
        .trust
        .pairing_peer(&pairing_id(&pairing.id))
        .await
        .expect("读对端行")
        .expect("认领后必须有对端行");
    assert_eq!(
        peer.id(),
        &acp_core::model::PeerIdentity::Node(node(ACCESS_NODE))
    );
    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// R21/R22：失败语义与状态码
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn repeated_claim_from_another_access_node_is_rejected_with_409() {
    let harness = Harness::with_origin().await;
    let pairing = harness.begin().await;
    let first_nonce = nonce_text(0x02);
    assert_eq!(
        harness
            .claim(&claim_body(&pairing, ACCESS_NODE, &first_nonce))
            .await
            .status,
        201
    );

    // 另一个 Access Node（不同 accessNodeId 与 clientNonce）对同一 pairingId 再 claim。
    let second = harness
        .claim(&claim_body(&pairing, OTHER_ACCESS_NODE, &nonce_text(0x03)))
        .await;
    assert_eq!(second.status, 409, "已被 claim 的配对返回 409（§13.4）");
    second.assert_security_headers();
    assert_eq!(second.json()["code"], json!("nodelink.auth.proof_invalid"));

    // 已有配对记录与对端行不受影响。
    assert_eq!(
        harness.record(&pairing).state(),
        PairingState::PendingConfirmation
    );
    let peer = harness
        .world
        .trust
        .pairing_peer(&pairing_id(&pairing.id))
        .await
        .expect("读对端行")
        .expect("对端行仍在");
    assert_eq!(
        peer.id(),
        &acp_core::model::PeerIdentity::Node(node(ACCESS_NODE)),
        "首个 claim 固定的对端不被替换"
    );
    assert_eq!(
        harness.world.trust.pairing_count(),
        1,
        "不创建第二条配对记录"
    );
    harness.stop().await;
}

#[tokio::test]
async fn malformed_claim_body_is_rejected_with_400() {
    let harness = Harness::with_origin().await;
    let pairing = harness.begin().await;

    // ① JSON 语法错误 → `nodelink.protocol.invalid_json`。
    let broken = harness.claim(b"{\"pairingId\":").await;
    assert_eq!(broken.status, 400);
    broken.assert_security_headers();
    assert_eq!(
        broken.json()["code"],
        json!("nodelink.protocol.invalid_json")
    );

    // ② 形状/取值域错误（未知字段）→ `nodelink.protocol.schema_invalid`。
    let mut value: Value =
        serde_json::from_slice(&claim_body(&pairing, ACCESS_NODE, &nonce_text(0x04)))
            .expect("请求体是 JSON");
    value["unexpected"] = json!("field");
    let extra = harness
        .claim(&serde_json::to_vec(&value).expect("可序列化"))
        .await;
    assert_eq!(extra.status, 400);
    extra.assert_security_headers();
    assert_eq!(
        extra.json()["code"],
        json!("nodelink.protocol.schema_invalid")
    );

    // ③ 必填字段缺失 → 同样是 400（schema 层）。
    let missing = harness.claim(b"{}").await;
    assert_eq!(missing.status, 400);
    assert_eq!(
        missing.json()["code"],
        json!("nodelink.protocol.schema_invalid")
    );

    // ④ `Content-Type` 不是 application/json → 400（§13.1 只接受 application/json）。
    let wrong_type = harness
        .claim_with(
            &claim_body(&pairing, ACCESS_NODE, &nonce_text(0x05)),
            Some("text/plain"),
        )
        .await;
    assert_eq!(wrong_type.status, 400);
    wrong_type.assert_security_headers();
    assert_eq!(
        wrong_type.json()["code"],
        json!("nodelink.protocol.schema_invalid")
    );

    // 四次失败都不推进配对状态，也不创建对端行。
    assert_eq!(harness.record(&pairing).state(), PairingState::Created);
    assert_eq!(harness.world.trust.pairing_count(), 1);
    assert!(
        harness
            .world
            .trust
            .pairing_peer(&pairing_id(&pairing.id))
            .await
            .expect("读对端行")
            .is_none()
    );
    harness.stop().await;
}

#[tokio::test]
async fn unknown_pairing_is_rejected_with_404() {
    let harness = Harness::with_origin().await;
    let pairing = harness.begin().await;
    // 同一 secret 但 pairingId 不存在：证明必然验不过，先命中 404（§13.4：pairing ID 不存在）。
    let mut request = claim_request(
        &pairing,
        ACCESS_NODE,
        "Office Access",
        &pairing.endpoint.clone(),
        &nonce_text(0x06),
    );
    request.pairing_id = uuid("2bc8b944-2a4f-46a7-8c31-b2c40923f60b");
    let response = harness
        .claim(&serde_json::to_vec(&request).expect("可序列化"))
        .await;
    assert_eq!(response.status, 404);
    response.assert_security_headers();
    harness.stop().await;
}

#[tokio::test]
async fn endpoint_host_outside_the_configured_origin_is_rejected_with_403() {
    let harness = Harness::with_origin().await;
    let pairing = harness.begin().await;
    // endpoint 不是本机宣告的 host：proof 与其余字段都合法，仍然 403（§13.4）。
    let body = serde_json::to_vec(&claim_request(
        &pairing,
        ACCESS_NODE,
        "Office Access",
        "wss://elsewhere.example.ts.net/node-link/v1",
        &nonce_text(0x07),
    ))
    .expect("可序列化");
    let response = harness.claim(&body).await;
    assert_eq!(response.status, 403);
    response.assert_security_headers();
    assert_eq!(
        response.json()["code"],
        json!("nodelink.auth.proof_invalid")
    );
    assert_eq!(harness.record(&pairing).state(), PairingState::Created);
    harness.stop().await;
}

#[tokio::test]
async fn claiming_without_a_configured_origin_is_rejected_with_403() {
    // 未配置 `daemon.public_origin`：本机没有可宣告的 endpoint，claim 一律 403（失败关闭）。
    // 配对本身建得出来（世界里有 origin），但处理器的配置快照里没有——这正是「配置与登记不一致」的接线场景。
    let harness = Harness::start(TestWorld::new(), PairingHttpConfig::default()).await;
    let pairing = harness.begin().await;
    let response = harness
        .claim(&claim_body(&pairing, ACCESS_NODE, &nonce_text(0x08)))
        .await;
    assert_eq!(response.status, 403);
    response.assert_security_headers();
    assert_eq!(
        response.json()["code"],
        json!("nodelink.auth.proof_invalid")
    );
    assert_eq!(
        harness.record(&pairing).state(),
        PairingState::Created,
        "被拒的 claim 不推进配对状态"
    );
    harness.stop().await;
}

#[tokio::test]
async fn expired_pairing_is_rejected_with_410() {
    let harness = Harness::with_origin().await;
    let pairing = harness.begin().await;
    // 时钟推进到 `expiresAt`（时间戳从左到右即时间序）：claim 返回 410 且不推进状态。
    harness.world.clock.set(&pairing.expires_at);
    let response = harness
        .claim(&claim_body(&pairing, ACCESS_NODE, &nonce_text(0x09)))
        .await;
    assert_eq!(response.status, 410);
    response.assert_security_headers();
    assert_eq!(
        response.json()["code"],
        json!("nodelink.auth.proof_invalid")
    );
    assert_eq!(harness.record(&pairing).state(), PairingState::Created);
    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// R23：proof 无效不泄露差异
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn invalid_proof_is_unauthorized_without_leaking_the_difference() {
    let harness = Harness::with_origin().await;
    let pairing = harness.begin().await;

    // 情形 A：HMAC 被篡改（其余字段合法）。
    let mut tampered: Value =
        serde_json::from_slice(&claim_body(&pairing, ACCESS_NODE, &nonce_text(0x0a)))
            .expect("请求体是 JSON");
    let mut proof_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(tampered["proof"].as_str().expect("proof"))
        .expect("proof 是 base64url");
    proof_bytes[0] ^= 0xff;
    tampered["proof"] = json!(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(proof_bytes));
    let tampered = harness
        .claim(&serde_json::to_vec(&tampered).expect("可序列化"))
        .await;

    // 情形 B：proof 是按另一个 clientNonce 算的（字段与证明不匹配）。
    let mut mismatched: Value =
        serde_json::from_slice(&claim_body(&pairing, ACCESS_NODE, &nonce_text(0x0a)))
            .expect("请求体是 JSON");
    mismatched["clientNonce"] = json!(nonce_text(0x0b));
    let mismatched = harness
        .claim(&serde_json::to_vec(&mismatched).expect("可序列化"))
        .await;

    // 情形 C：proof 是另一台 Access Node 的载荷（声明的 `accessNodeId` 与证明里的不一致）。
    let mut other_request = claim_request(
        &pairing,
        ACCESS_NODE,
        "Office Access",
        &pairing.endpoint.clone(),
        &nonce_text(0x0a),
    );
    let foreign = NodeLinkPairingProof {
        owner_node_id: node(OWNER_NODE),
        access_node_id: node(OTHER_ACCESS_NODE),
        pairing_id: pairing_id(&pairing.id),
        pairing_expires_at: Timestamp::new(&pairing.expires_at).expect("规范时间戳"),
        access_public_key: access_public_key(),
        client_nonce: Nonce::new(&nonce_text(0x0a)).expect("规范 nonce"),
        node_name: "Office Access".to_owned(),
        node_kind: NodeKind::Access,
    }
    .hmac(&pairing.secret)
    .expect("证明必须可计算");
    other_request.proof = wire_proof(&foreign);
    let other = harness
        .claim(&serde_json::to_vec(&other_request).expect("可序列化"))
        .await;

    for response in [&tampered, &mismatched, &other] {
        assert_eq!(
            response.status, 401,
            "所有证明类失败统一 401（§13.2/§13.4）"
        );
        response.assert_security_headers();
    }
    assert_eq!(error_shape(&tampered), error_shape(&mismatched));
    assert_eq!(error_shape(&tampered), error_shape(&other));
    assert_eq!(
        tampered.json()["code"],
        json!("nodelink.auth.proof_invalid")
    );
    assert_eq!(tampered.json()["details"], json!({}));
    // `correlationId` 是每次请求的诊断标识，不泄露校验差异。
    for response in [&tampered, &mismatched, &other] {
        let body = response.json();
        assert!(body["correlationId"].is_string() || body["correlationId"].is_null());
    }
    // 配对状态不变：失败不产生任何推进。
    assert_eq!(harness.record(&pairing).state(), PairingState::Created);
    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// R24：相同内容重试幂等
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn retrying_the_same_claim_returns_the_original_pairing_request() {
    let harness = Harness::with_origin().await;
    let pairing = harness.begin().await;
    let nonce = nonce_text(0x0c);
    let body = claim_body(&pairing, ACCESS_NODE, &nonce);

    let first = harness.claim(&body).await;
    assert_eq!(first.status, 201);
    let original_request_id = first.json()["pairingRequestId"].clone();
    assert!(original_request_id.is_string());

    // 响应丢失后的重试：相同 accessNodeId/clientNonce 与相同内容 → 原 pairing request，且不建第二条记录。
    let retry = harness.claim(&body).await;
    assert_eq!(retry.status, 201);
    retry.assert_security_headers();
    assert_eq!(retry.json()["pairingRequestId"], original_request_id);
    assert_eq!(
        retry.json()["serverNonce"],
        first.json()["serverNonce"],
        "serverNonce 也来自本机为该配对登记的同一份材料"
    );
    assert_eq!(
        harness.world.trust.pairing_count(),
        1,
        "不创建第二条配对记录"
    );
    assert_eq!(
        harness.record(&pairing).state(),
        PairingState::PendingConfirmation
    );

    // 不同内容的重试 → 409（§13.4）。
    let conflict = harness
        .claim(&claim_body(&pairing, ACCESS_NODE, &nonce_text(0x0d)))
        .await;
    assert_eq!(conflict.status, 409);
    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// R17：请求体上限（接入层）
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn claim_body_over_the_transport_limit_is_rejected_with_413() {
    let harness = Harness::with_origin().await;
    let pairing = harness.begin().await;
    let body = vec![b'a'; 80 * 1024];
    let response = harness.claim(&body).await;
    assert_eq!(
        response.status, 413,
        "超过 `daemon` 请求体上限的配对请求在接入层被拒（§13.4）"
    );
    assert_eq!(harness.record(&pairing).state(), PairingState::Created);
    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// R34：claim 限流
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn claim_rate_limit_returns_429_after_ten_attempts_per_ip() {
    let harness = Harness::with_origin().await;
    let pairing = harness.begin().await;
    // 前 10 次尝试都进入处理器（这里用语法错误的 body，因此不推进状态）；第 11 次被限流。
    for _ in 0..10 {
        assert_eq!(harness.claim(b"not json").await.status, 400);
    }
    let limited = harness.claim(b"not json").await;
    assert_eq!(
        limited.status, 429,
        "同一 IP 一分钟内的第 11 次 claim 返回 429（§2.5）"
    );
    limited.assert_security_headers();
    let body = limited.json();
    assert_eq!(body["code"], json!("nodelink.resource.rate_limited"));
    assert_eq!(body["retryable"], json!(true));
    let retry_after = body["details"]["retryAfterMs"]
        .as_u64()
        .expect("details.retryAfterMs 是登记字段");
    assert!(retry_after <= 60_000, "退避不超过窗口：{retry_after}");
    // 该次请求没有进入配对状态机。
    assert_eq!(harness.record(&pairing).state(), PairingState::Created);
    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// R30/R31：安全响应头与凭据边界
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn every_pairing_response_carries_the_four_security_headers() {
    let harness = Harness::with_origin().await;
    let pairing = harness.begin().await;
    let nonce = nonce_text(0x0e);

    // 201（成功）与 400/401/403/404/409/410/429（各种失败）都带四个头。
    let responses = vec![
        harness
            .claim(&claim_body(&pairing, ACCESS_NODE, &nonce))
            .await,
        harness.claim(b"not json").await,
        harness
            .claim(&claim_body(&pairing, ACCESS_NODE, &nonce_text(0x0f)))
            .await,
        harness.claim(b"{}").await,
    ];
    for response in &responses {
        response.assert_security_headers();
    }
    harness.stop().await;
}

#[tokio::test]
async fn pairing_responses_never_carry_the_pairing_secret() {
    let harness = Harness::with_origin().await;
    let pairing = harness.begin().await;
    let secret_text =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(pairing.secret.as_bytes());

    let success = harness
        .claim(&claim_body(&pairing, ACCESS_NODE, &nonce_text(0x10)))
        .await;
    assert!(
        !success.text().contains(&secret_text),
        "成功响应不得包含 pairingSecret"
    );
    let failure = harness.claim(b"not json").await;
    assert!(
        !failure.text().contains(&secret_text),
        "错误响应不得包含 pairingSecret"
    );
    harness.stop().await;
}

/// 静默使用的 `test_nonce` 只用于构造合法 nonce 的对照，确保本文件的 `nonce_text` 与它同口径。
#[test]
fn nonce_helper_matches_the_shared_test_nonce() {
    let shared = test_nonce();
    assert_eq!(shared.as_str().len(), 43);
    assert_eq!(nonce_text(0x00), shared.as_str());
}
