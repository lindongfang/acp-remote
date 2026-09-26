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
//! | [R26]/[R27] 五状态一律 200 | `status_reports_the_five_business_states_with_200` |
//! | [R28] status proof 无效 401 | `status_with_an_invalid_proof_is_unauthorized` |
//! | [R29] 重试复用 nonce 返回原响应 | `status_retry_with_the_same_nonce_returns_the_original_response` |
//! | [R30]/[R31] 安全响应头与凭据边界 | `every_pairing_response_carries_the_four_security_headers`、`pairing_responses_never_carry_the_pairing_secret`、`status_responses_carry_the_security_headers_and_no_secret`、`access_layer_rejections_on_the_pairing_paths_carry_the_security_headers`（413/Host-400，RV1-WP3C 裁决项①） |
//! | [R32] secret 按期清除 | `status_reports_the_five_business_states_with_200`（expired/consumed 两条路径断言 secret 已清除） |
//! | [R33]/[R35] status 限流 | `status_rate_limit_returns_429_after_sixty_queries_per_pairing`、`pairing::tests::pairing_id_window_*` |
//! | [R34] claim 超限 429 | `claim_rate_limit_returns_429_after_ten_attempts_per_ip` |

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use acp_core::model::{NodeId, NodeKind, Nonce, PairingId, PairingState, PeerPublicKey, Timestamp};
use acp_core::ports::TrustStore as _;
use base64::Engine as _;
use identity_auth::{
    NodeLinkPairingOwnerProof, NodeLinkPairingProof, NodeLinkPairingStatus, P1363Signature,
    PairingRequestId, PairingSecret,
};
use node_link_protocol::common::{Base64Url, NonEmptyText, ProtocolVersionV1, Uuid};
use node_link_protocol::pairing::{
    AccessNodeKind, ClaimRequest, Endpoint, QrPayload, StatusRequest,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;

use crate::local_admin::envelope::{AdminOutcome, AdminRequest, AdminResponse, decode_request};
use crate::local_admin::handler::LocalAdminHandler as _;
use crate::local_admin::method::Method;
use crate::local_admin::test_support::{TestWorld, test_nonce, test_public_key};
use crate::node_link::pairing::{CLAIM_PATH, PairingHttp, PairingHttpConfig, STATUS_PATH};
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
        // 与 WP7 的接线同口径：配对注册必须声明四个安全头（否则接入层预拒绝的 413/Host 400 不带）。
        let default_headers = pairing.default_response_headers();
        listener
            .register_post(CLAIM_PATH, pairing.claim_handler(), default_headers)
            .expect("注册 claim 端点");
        listener
            .register_post(STATUS_PATH, pairing.status_handler(), default_headers)
            .expect("注册 status 端点");
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

    /// 发送一条 status 请求（`application/json`）。
    async fn status(&self, body: &[u8]) -> Response {
        self.post(STATUS_PATH, body, Some("application/json")).await
    }

    /// 发送一个 POST 请求到已注册的 path。
    async fn post(&self, path: &str, body: &[u8], content_type: Option<&str>) -> Response {
        send(self.addr, path, body, content_type).await
    }

    /// 发送一个 POST 请求到已注册的 path，并用指定的 `Host`（Host 边界用例）。
    async fn post_with_host(
        &self,
        path: &str,
        host: &str,
        body: &[u8],
        content_type: Option<&str>,
    ) -> Response {
        send_with_host(self.addr, path, host, body, content_type).await
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
    send_with_host(addr, path, &host_of(addr), body, content_type).await
}

/// 同上，但可指定 `Host`（Host 边界用例）。
async fn send_with_host(
    addr: SocketAddr,
    path: &str,
    host: &str,
    body: &[u8],
    content_type: Option<&str>,
) -> Response {
    let mut stream = tokio::time::timeout(IO_TIMEOUT, TcpStream::connect(addr))
        .await
        .expect("连接超时")
        .expect("连接 loopback");
    let mut request =
        format!("POST {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n").into_bytes();
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

    // 本用例断言成功（201）与两类 400（JSON 语法错误、缺字段）以及 409（同一 Access Node 换 nonce 的重复
    // claim）；401/403/404/410/413/429 与接入层 400 的安全头由各自的专测覆盖。
    let claimed = harness
        .claim(&claim_body(&pairing, ACCESS_NODE, &nonce))
        .await;
    assert_eq!(claimed.status, 201, "首次 claim 成功（§13.2）");
    let malformed = harness.claim(b"not json").await;
    assert_eq!(malformed.status, 400, "JSON 语法错误是 400（§13.4）");
    let repeated = harness
        .claim(&claim_body(&pairing, ACCESS_NODE, &nonce_text(0x0f)))
        .await;
    assert_eq!(
        repeated.status, 409,
        "同节点换 nonce 的重复 claim 是 409（§13.4）"
    );
    let missing = harness.claim(b"{}").await;
    assert_eq!(missing.status, 400, "缺字段的 schema 错误是 400（§13.4）");
    for response in [&claimed, &malformed, &repeated, &missing] {
        response.assert_security_headers();
    }
    harness.stop().await;
}

/// [R30]/[R31]（RV1-WP3C 的安全头覆盖）：§13.1 的「所有配对 HTTP 响应」也包括接入层在调用处理器前
/// 产生的拒绝——413（请求体超限）与 400（Host 不匹配）——它们靠注册时声明的每路径默认响应头满足。
#[tokio::test]
async fn access_layer_rejections_on_the_pairing_paths_carry_the_security_headers() {
    let harness = Harness::with_origin().await;
    // ① 请求体超限：413 由接入层产生（处理器不被调用）。
    let oversized = harness.claim(&vec![b'a'; 80 * 1024]).await;
    assert_eq!(oversized.status, 413, "超过请求体上限的 claim 在接入层被拒");
    oversized.assert_security_headers();
    // ② Host 不在允许集合内：400 发生在路由之前（本接入层在未注册 path 上也先给 400）。
    for path in [CLAIM_PATH, STATUS_PATH] {
        let rejected = harness
            .post_with_host(path, "evil.example.com", b"{}", Some("application/json"))
            .await;
        assert_eq!(rejected.status, 400, "{path} 上的错误 Host 必须 400");
        rejected.assert_security_headers();
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

// ---------------------------------------------------------------------------------------------
// R26–R29、R32–R35：status 端点
// ---------------------------------------------------------------------------------------------

/// 一个自洽的 `pairingRequestId` 占位值（HMAC 覆盖声明值本身，因此 proof 仍自洽；真实的
/// `pairingRequestId` 由 claim 响应的用例断言过来源）。
const PLACEHOLDER_REQUEST_ID: &str = "b2f3fbfa-c2c6-4f49-9e0c-643d152de18d";

/// 构造一次 status 查询：proof 用给定的 secret 对 `node-link-pairing-status/v1` 计算。
fn status_request_with_secret(
    pairing: &Pairing,
    secret: &PairingSecret,
    pairing_request_id: &str,
    request_nonce: &str,
) -> StatusRequest {
    let nonce = Nonce::new(request_nonce).expect("规范 nonce");
    let proof = NodeLinkPairingStatus {
        owner_node_id: node(OWNER_NODE),
        access_node_id: node(ACCESS_NODE),
        pairing_id: pairing_id(&pairing.id),
        pairing_request_id: PairingRequestId::parse(pairing_request_id).expect("规范 request id"),
        request_nonce: nonce.clone(),
    }
    .hmac(secret)
    .expect("证明必须可计算");
    StatusRequest {
        protocol_version: ProtocolVersionV1::new(1).expect("v1"),
        owner_node_id: uuid(OWNER_NODE),
        access_node_id: uuid(ACCESS_NODE),
        pairing_id: uuid(&pairing.id),
        pairing_request_id: uuid(pairing_request_id),
        request_nonce: wire_nonce(nonce.as_str()),
        proof: wire_proof(&proof),
    }
}

/// 用二维码里的 secret 构造一次 status 查询。
fn status_request(
    pairing: &Pairing,
    pairing_request_id: &str,
    request_nonce: &str,
) -> StatusRequest {
    status_request_with_secret(pairing, &pairing.secret, pairing_request_id, request_nonce)
}

/// status 请求体。
fn status_body(pairing: &Pairing, pairing_request_id: &str, request_nonce: &str) -> Vec<u8> {
    serde_json::to_vec(&status_request(pairing, pairing_request_id, request_nonce))
        .expect("status 请求可序列化")
}

/// 第 `index` 个不同的 32 字节 nonce（避免重试缓存命中）。
fn nonce_for_index(index: usize) -> String {
    let mut bytes = [0u8; 32];
    bytes[..8].copy_from_slice(&u64::try_from(index).expect("小整数").to_be_bytes());
    bytes[31] &= 0b1111_1100;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

impl Harness {
    /// 以有效 proof 查询一次状态。
    async fn query_status(
        &self,
        pairing: &Pairing,
        request_id: &str,
        request_nonce: &str,
    ) -> Response {
        self.status(&status_body(pairing, request_id, request_nonce))
            .await
    }
}

/// claim 一次并回带本次的 `pairingRequestId`（status 请求必须带同一个值）。
async fn claimed(harness: &Harness, pairing: &Pairing) -> String {
    let response = harness
        .claim(&claim_body(pairing, ACCESS_NODE, &nonce_text(0x20)))
        .await;
    assert_eq!(response.status, 201, "claim 必须成功");
    response.json()["pairingRequestId"]
        .as_str()
        .expect("pairingRequestId")
        .to_owned()
}

/// 用真实路径把配对推进到 `approved`（本机用户在本地通道上确认）。
async fn approve(harness: &Harness, pairing: &Pairing) {
    let response = harness
        .world
        .router()
        .handle(admin_request(
            Method::NodePairConfirm,
            json!({"pairingId": pairing.id, "grants": ["grant.observe"]}),
        ))
        .await;
    let result = success(&response);
    assert_eq!(result["grants"], json!(["grant.observe"]));
}

/// 用真实路径把配对推进到 `rejected`。
async fn reject(harness: &Harness, pairing: &Pairing) {
    let response = harness
        .world
        .router()
        .handle(admin_request(
            Method::NodePairReject,
            json!({"pairingId": pairing.id, "reason": null}),
        ))
        .await;
    let _ = success(&response);
}

/// R26/R27/R32：五种业务状态一律 200，业务状态只由 body 的 `status` 表达。
#[tokio::test]
async fn status_reports_the_five_business_states_with_200() {
    let harness = Harness::with_origin().await;

    // ① pending_confirmation（claim 之后、本机确认之前）。
    let pending = harness.begin().await;
    let request_id = claimed(&harness, &pending).await;
    let response = harness
        .query_status(&pending, &request_id, &nonce_text(0x21))
        .await;
    assert_eq!(response.status, 200);
    response.assert_security_headers();
    let body = response.json();
    assert_eq!(body["status"], json!("pending_confirmation"));
    assert_eq!(body["expiresAt"], json!(pending.expires_at));
    assert_eq!(body["pairingRequestId"], json!(request_id));
    assert_eq!(
        body.as_object().expect("对象").len(),
        4,
        "未批准的状态只带四个字段（不签发凭据、不带 node/owner）"
    );

    // ② approved：非秘密元数据 + 已授予的 grant + Owner identity。
    approve(&harness, &pending).await;
    let approved = harness
        .query_status(&pending, &request_id, &nonce_text(0x22))
        .await;
    assert_eq!(approved.status, 200);
    let body = approved.json();
    assert_eq!(body["status"], json!("approved"));
    assert_eq!(body["node"]["accessNodeId"], json!(ACCESS_NODE));
    assert_eq!(body["node"]["name"], json!("Office Access"));
    assert_eq!(
        body["node"]["scopes"],
        json!(["grant.observe"]),
        "已授予集合来自本机确认后的节点行，而不是登记/请求值"
    );
    assert_eq!(body["owner"]["ownerNodeId"], json!(OWNER_NODE));
    assert!(
        body["owner"]["ownerPublicKey"].is_string(),
        "owner 块只给身份公钥，不签发任何凭据"
    );
    assert!(
        body.get("token").is_none() && body.get("credential").is_none(),
        "approved 不签发 bearer 凭据"
    );
    assert_eq!(
        body.as_object().expect("对象").len(),
        6,
        "approved 恰好带四个公共字段 + node + owner"
    );

    // ③ rejected。
    let rejected = harness.begin().await;
    let rejected_id = claimed(&harness, &rejected).await;
    reject(&harness, &rejected).await;
    let response = harness
        .query_status(&rejected, &rejected_id, &nonce_text(0x23))
        .await;
    assert_eq!(response.status, 200);
    assert_eq!(response.json()["status"], json!("rejected"));

    // ④ expired：过期扫描清除内存 secret（§4.3），此后 status 按 §13.4 的「过期语义」回 200 + expired。
    let expired = harness.begin().await;
    let expired_id = claimed(&harness, &expired).await;
    harness.world.clock.set(&expired.expires_at);
    let record = harness.record(&expired);
    let due = harness.world.authority.due_pairings(
        std::slice::from_ref(&record),
        &Timestamp::new(&expired.expires_at).expect("时间戳"),
    );
    assert_eq!(
        due,
        vec![pairing_id(&expired.id)],
        "未终结的配对需要提交终态写集"
    );
    assert!(
        !harness.world.authority.has_secret(&pairing_id(&expired.id)),
        "到期后本机不再持有 pairingSecret"
    );
    let response = harness
        .query_status(&expired, &expired_id, &nonce_text(0x24))
        .await;
    assert_eq!(response.status, 200, "expired 也是 200（§13.4）");
    assert_eq!(response.json()["status"], json!("expired"));

    // ⑤ consumed：首次 WSS 认证成功后的收尾（`complete_auth` → `consume_pairing`）。
    let consumed = harness.begin().await;
    let consumed_id = claimed(&harness, &consumed).await;
    approve(&harness, &consumed).await;
    let completion = harness.world.authority.complete_auth(
        identity_auth::IdentityFact::Node {
            node: node(ACCESS_NODE),
            kind: NodeKind::Access,
            grants: acp_core::model::GrantSet::try_from_iter(["grant.observe"]).expect("grants"),
        },
        Some(&pairing_id(&consumed.id)),
        &Timestamp::new("2026-09-18T09:12:03.412Z").expect("时间戳"),
    );
    assert_eq!(
        completion.consume_pairing,
        Some(pairing_id(&consumed.id)),
        "首次认证成功后需要把该配对推进为 consumed"
    );
    harness
        .world
        .core
        .consume_pairing(
            &acp_core::model::Actor::Node {
                node: node(ACCESS_NODE),
                access_node: node(OWNER_NODE),
            },
            &pairing_id(&consumed.id),
        )
        .await
        .expect("消费写集");
    assert_eq!(harness.record(&consumed).state(), PairingState::Consumed);
    assert!(
        !harness
            .world
            .authority
            .has_secret(&pairing_id(&consumed.id)),
        "首次认证成功后 secret 提前清除（§4.3）"
    );
    let response = harness
        .query_status(&consumed, &consumed_id, &nonce_text(0x25))
        .await;
    assert_eq!(response.status, 200, "consumed 也是 200（§13.4）");
    assert_eq!(response.json()["status"], json!("consumed"));

    harness.stop().await;
}

/// R28：proof 无效（含未登记的 pairingId）→ 401，且不返回任何业务状态。
#[tokio::test]
async fn status_with_an_invalid_proof_is_unauthorized() {
    let harness = Harness::with_origin().await;
    let pairing = harness.begin().await;
    let _ = claimed(&harness, &pairing).await;

    // ① 篡改 proof。
    let mut tampered: Value = serde_json::from_slice(&status_body(
        &pairing,
        PLACEHOLDER_REQUEST_ID,
        &nonce_text(0x26),
    ))
    .expect("status 请求体是 JSON");
    let mut proof_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(tampered["proof"].as_str().expect("proof"))
        .expect("base64url");
    proof_bytes[0] ^= 0xff;
    tampered["proof"] = json!(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(proof_bytes));
    let tampered = harness
        .status(&serde_json::to_vec(&tampered).expect("可序列化"))
        .await;

    // ② 用**另一把** secret 对同一个配对的 transcript 计算 proof。
    let foreign_pairing = harness.begin().await;
    let foreign = status_request_with_secret(
        &pairing,
        &foreign_pairing.secret,
        PLACEHOLDER_REQUEST_ID,
        &nonce_text(0x26),
    );
    let foreign = harness
        .status(&serde_json::to_vec(&foreign).expect("可序列化"))
        .await;

    for response in [&tampered, &foreign] {
        assert_eq!(response.status, 401, "proof 无效 → 401（§13.3）");
        response.assert_security_headers();
        let body = response.json();
        assert_eq!(body["code"], json!("nodelink.auth.proof_invalid"));
        assert!(body.get("status").is_none(), "401 不返回任何业务状态信息");
    }
    assert_eq!(error_shape(&tampered), error_shape(&foreign));

    // 未登记的 pairingId 同样是 401（status 端点不区分存在性）。
    let unknown = Pairing {
        id: "2bc8b944-2a4f-46a7-8c31-b2c40923f60b".to_owned(),
        secret: PairingSecret::try_from_bytes(&[0x22; 32]).expect("32 字节 secret"),
        endpoint: pairing.endpoint.clone(),
        expires_at: pairing.expires_at.clone(),
    };
    let unknown = harness
        .query_status(&unknown, PLACEHOLDER_REQUEST_ID, &nonce_text(0x27))
        .await;
    assert_eq!(unknown.status, 401);
    assert!(unknown.json().get("status").is_none());
    harness.stop().await;
}

/// R29：同一 `requestNonce` 的重复请求返回原响应；新 nonce 的轮询看到最新状态。
#[tokio::test]
async fn status_retry_with_the_same_nonce_returns_the_original_response() {
    let harness = Harness::with_origin().await;
    let pairing = harness.begin().await;
    let request_id = claimed(&harness, &pairing).await;
    let nonce = nonce_text(0x28);

    let first = harness.query_status(&pairing, &request_id, &nonce).await;
    assert_eq!(first.status, 200);
    assert_eq!(first.json()["status"], json!("pending_confirmation"));

    // 状态推进到 approved 之后，用同一个 `requestNonce` 重试：仍返回原响应（§13.3）。
    approve(&harness, &pairing).await;
    let retry = harness.query_status(&pairing, &request_id, &nonce).await;
    assert_eq!(retry.status, 200);
    assert_eq!(retry.json(), first.json(), "网络重试必须返回原响应");

    // 新 nonce 的轮询不受重试缓存影响。
    let fresh = harness
        .query_status(&pairing, &request_id, &nonce_text(0x29))
        .await;
    assert_eq!(fresh.status, 200);
    assert_eq!(fresh.json()["status"], json!("approved"));
    harness.stop().await;
}

/// R33/R35：status 每 `pairingId` 60 次/分钟，第 61 次 429。
#[tokio::test]
async fn status_rate_limit_returns_429_after_sixty_queries_per_pairing() {
    let harness = Harness::with_origin().await;
    let pairing = harness.begin().await;
    let request_id = claimed(&harness, &pairing).await;
    for index in 0..60 {
        let response = harness
            .query_status(&pairing, &request_id, &nonce_for_index(index))
            .await;
        assert_eq!(response.status, 200, "第 {index} 次查询必须在限额内");
    }
    let limited = harness
        .query_status(&pairing, &request_id, &nonce_for_index(60))
        .await;
    assert_eq!(limited.status, 429, "第 61 次查询超限（§2.5）");
    limited.assert_security_headers();
    let body = limited.json();
    assert_eq!(body["code"], json!("nodelink.resource.rate_limited"));
    assert!(body["details"]["retryAfterMs"].as_u64().is_some());
    harness.stop().await;
}

/// R30/R31：status 端点的响应同样带四个安全头，且不出现 `pairingSecret`。
#[tokio::test]
async fn status_responses_carry_the_security_headers_and_no_secret() {
    let harness = Harness::with_origin().await;
    let pairing = harness.begin().await;
    let request_id = claimed(&harness, &pairing).await;
    let secret_text =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(pairing.secret.as_bytes());

    let mut bad_proof: Value =
        serde_json::from_slice(&status_body(&pairing, &request_id, &nonce_text(0x2a)))
            .expect("status 请求体是 JSON");
    bad_proof["proof"] = json!(nonce_text(0x2b));
    let responses = vec![
        harness
            .query_status(&pairing, &request_id, &nonce_text(0x2c))
            .await,
        harness
            .status(&serde_json::to_vec(&bad_proof).expect("可序列化"))
            .await,
        harness.status(b"not json").await,
    ];
    for response in &responses {
        response.assert_security_headers();
        assert!(
            !response.text().contains(&secret_text),
            "status 响应不得包含 pairingSecret"
        );
    }
    assert_eq!(responses[0].status, 200);
    assert_eq!(responses[1].status, 401);
    assert_eq!(responses[2].status, 400);
    harness.stop().await;
}
