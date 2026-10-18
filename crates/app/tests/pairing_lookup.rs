//! Sync 配对码 lookup 与扫码资料取回（qr-lookup）端点及过期/重启清理接线的协议级验收
//! （`pairing-code-web-entry` 的 WP3 + `device-pairing-qr-reference` 的 WP3）。
//!
//! 驱动方式与 `sync_e2e.rs` 相同：真实 loopback listener + 真实 `identity_auth::Authority` + 真实
//! `LocalAdminRouter`/SQLite，配对码来自真实的 `device.pair.begin` 响应。每条断言都指向可观察事实
//! （HTTP 状态/响应字段/管理面状态/内存 secret），覆盖成功、单次消耗、统一失败、来源不符、独立限流与
//! 两条清理路径；不只跑 happy path。
//!
//! 敏感值口径：配对码与 pairing secret 只在测试断言里出现，不写日志；失败响应必须不区分「不存在」与
//! 「已消耗」。

mod support;

use acp_core::ports::TrustStore as _;
use serde_json::{Value, json};
use support::sync::{DeviceKey, HostNode, nonce_text, uuid_text};

/// 本机宿主名（`Host` 与 `Origin` 都按它判）。
const HOST: &str = "pairing-lookup.example.invalid";
/// claim 声明的设备名。
const DEVICE_NAME: &str = "Lookup Phone";

/// lookup 端点（与组合根注册的常量同源）。
const LOOKUP_PATH: &str = server::sync::pairing::LOOKUP_PATH;
const QR_LOOKUP_PATH: &str = server::sync::pairing::QR_LOOKUP_PATH;
const CLAIM_PATH: &str = server::sync::pairing::CLAIM_PATH;

/// 配对索赔方（设备）的材料。
struct Device {
    key: DeviceKey,
    client_nonce: String,
}

impl Device {
    fn new() -> Self {
        Self {
            key: DeviceKey::new(&uuid_text()),
            client_nonce: nonce_text(0x71),
        }
    }

    fn id(&self) -> String {
        self.key.device_id().to_owned()
    }
}

/// 一次 `POST` 的响应（状态码 + 响应头小写名 + 解析出的 JSON）。
struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Value,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// 统一失败断言：401 + `pairing.code_invalid` + 不泄露任何配对材料。
    fn assert_code_invalid(&self) {
        assert_eq!(self.status, 401, "必须是统一的 401：{}", self.body);
        assert_eq!(
            self.body["code"],
            json!("pairing.code_invalid"),
            "必须是统一错误码：{}",
            self.body
        );
        assert!(
            self.body.get("pairingSecret").is_none() && self.body.get("pairingId").is_none(),
            "失败响应不得包含任何配对材料：{}",
            self.body
        );
    }

    /// 扫码取回的统一失败断言：401 + `pairing.qr_invalid` + 不泄露任何配对材料。
    fn assert_qr_invalid(&self) {
        assert_eq!(self.status, 401, "必须是统一的 401：{}", self.body);
        assert_eq!(
            self.body["code"],
            json!("pairing.qr_invalid"),
            "必须是统一错误码：{}",
            self.body
        );
        assert!(
            self.body.get("pairingSecret").is_none() && self.body.get("pairingId").is_none(),
            "失败响应不得包含任何配对材料：{}",
            self.body
        );
    }

    fn assert_security_headers(&self) {
        assert_eq!(self.header("cache-control"), Some("no-store"));
        assert_eq!(self.header("pragma"), Some("no-cache"));
        assert_eq!(self.header("referrer-policy"), Some("no-referrer"));
        assert_eq!(self.header("x-content-type-options"), Some("nosniff"));
    }
}

/// 向真实 listener 发一条裸 `POST`（可覆盖 `Origin`/`Host`；body 是原始文本，用于构造 schema 错误）。
async fn post_raw(
    host: &HostNode,
    path: &str,
    origin: Option<&str>,
    host_header: Option<&str>,
    body: &str,
) -> Reply {
    request_raw(
        host,
        "POST",
        path,
        origin,
        host_header,
        Some("application/json"),
        body.as_bytes(),
    )
    .await
}

/// 向真实 listener 发一条任意方法的裸请求（方法/Content-Type/字节体都可覆盖）。
///
/// 只用于**接入层边界**的断言（405/413/非 JSON 400），因此不声明 `Content-Type` 时头里也不带它。
async fn request_raw(
    host: &HostNode,
    method: &str,
    path: &str,
    origin: Option<&str>,
    host_header: Option<&str>,
    content_type: Option<&str>,
    body: &[u8],
) -> Reply {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let origin_line = origin
        .map(|origin| format!("Origin: {origin}\r\n"))
        .unwrap_or_default();
    let content_type_line = content_type
        .map(|content_type| format!("Content-Type: {content_type}\r\n"))
        .unwrap_or_default();
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {name}\r\n{origin_line}{content_type_line}\
         Connection: close\r\nContent-Length: {}\r\n\r\n",
        body.len(),
        name = host_header.unwrap_or(host.host_name()),
    );
    let mut stream = tokio::net::TcpStream::connect(host.addr())
        .await
        .expect("连接 loopback");
    stream.write_all(head.as_bytes()).await.expect("写请求头");
    stream.write_all(body).await.expect("写请求体");
    let mut raw = Vec::new();
    tokio::time::timeout(support::sync::IO_TIMEOUT, stream.read_to_end(&mut raw))
        .await
        .expect("读响应超时")
        .expect("读响应");
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((text.as_str(), ""));
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or_else(|| panic!("响应状态行无法解析：{status_line:?}"));
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    Reply {
        status,
        headers,
        body: serde_json::from_str(body).unwrap_or(Value::Null),
    }
}

/// 正常来源的 lookup 请求。
async fn lookup(host: &HostNode, code: &str) -> Reply {
    let body = json!({ "protocolVersion": 1, "pairingCode": code }).to_string();
    post_raw(host, LOOKUP_PATH, Some(&host.origin()), None, &body).await
}

/// `device.pair.begin`（真实本地管理通道）。
async fn begin(host: &HostNode, expires_in_ms: Option<u64>) -> Value {
    host.admin(
        server::local_admin::Method::DevicePairBegin,
        json!({ "expiresInMs": expires_in_ms }),
    )
    .await
}

/// 新设备短二维码地址 → `(qrToken, hostKeyFingerprint, canonicalOrigin)`。
///
/// 测试侧独立解码（`app` 不依赖 `sync-protocol`）：只接受 `https://<origin>/pair#q2=<87 字符>`、
/// 无 query、65 字节 `0x02 || token || fingerprint` 布局；旧 `#data=` 或其它版本直接失败。
fn decode_device_qr(url: &str) -> ([u8; 32], [u8; 32], &str) {
    let (base, fragment) = url.split_once('#').expect("设备二维码地址带 fragment");
    assert!(!base.contains('?'), "短地址不得带 query：{url}");
    let origin = base.strip_suffix("/pair").expect("短地址路径必须是 /pair");
    assert!(
        origin.starts_with("https://"),
        "外层地址必须是 HTTPS：{url}"
    );
    let payload = fragment
        .strip_prefix("q2=")
        .expect("设备二维码 fragment 只接受 q2=");
    assert!(
        !payload.contains('#') && payload.len() == 87,
        "q2 payload 是 87 个 ASCII 字符：{url}"
    );
    let bytes = base64_decode(payload);
    assert_eq!(bytes.len(), 65, "q2 payload 解出 65 字节");
    assert_eq!(bytes[0], 2, "q2 版本必须是 2");
    let mut token = [0_u8; 32];
    let mut fingerprint = [0_u8; 32];
    token.copy_from_slice(&bytes[1..33]);
    fingerprint.copy_from_slice(&bytes[33..]);
    (token, fingerprint, origin)
}

/// 新设备短地址的 fragment 载荷 → `(qrToken, hostKeyFingerprint)`。
///
/// 地址必须是 `https://<origin>/pair#q2=<87 字符>`；解析在本地独立实现里完成，
/// 用例不复制服务端的编解码器。
fn qr_payload(url: &str) -> ([u8; 32], [u8; 32]) {
    let (token, fingerprint, _origin) = decode_device_qr(url);
    assert_eq!(
        url.len(),
        url.split_once('#').expect("fragment").0.len() + "#q2=".len() + 87
    );
    (token, fingerprint)
}

/// 实际的 `qr-lookup` 请求（同一来源、`application/json`）。
async fn qr_lookup(host: &HostNode, token: &[u8; 32]) -> Reply {
    let body = json!({
        "protocolVersion": 1,
        "qrToken": support::sync::base64url(token),
    })
    .to_string();
    post_raw(host, QR_LOOKUP_PATH, Some(&host.origin()), None, &body).await
}

/// `qr-lookup` 取回 → `(七字段响应, secret)`，并验证二维码里固定的是完整主机公钥指纹。
async fn retrieve(host: &HostNode, started: &Value) -> (Value, [u8; 32]) {
    let (token, fingerprint) = qr_payload(started["pairingUrl"].as_str().expect("pairingUrl"));
    let replied = qr_lookup(host, &token).await;
    assert_eq!(replied.status, 200, "扫码取回必须成功：{}", replied.body);
    replied.assert_security_headers();
    assert_eq!(replied.body["pairingId"], started["pairingId"]);
    assert_eq!(replied.body["canonicalOrigin"], json!(host.origin()));
    assert_eq!(replied.body["expiresAt"], started["expiresAt"]);
    let host_public_key = base64_decode(
        replied.body["hostPublicKey"]
            .as_str()
            .expect("hostPublicKey"),
    );
    use sha2::{Digest as _, Sha256};
    assert_eq!(
        <[u8; 32]>::from(Sha256::digest(&host_public_key)),
        fingerprint,
        "二维码固定的是完整 32 字节公钥指纹"
    );
    let secret = base64_decode(
        replied.body["pairingSecret"]
            .as_str()
            .expect("pairingSecret"),
    );
    (
        replied.body,
        secret
            .as_slice()
            .try_into()
            .expect("pairing secret 是 32 字节"),
    )
}

fn base64_decode(text: &str) -> Vec<u8> {
    base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, text)
        .expect("无填充 base64url")
}

/// 设备公钥的 wire 形状。
fn public_key_base64url(device: &Device) -> String {
    support::sync::base64url(device.key.public_key().as_bytes())
}

/// claim 证明：`acp-remote/pairing-proof/v1` 域的 HMAC-SHA256。
fn pairing_proof(ticket: &Value, secret: &[u8; 32], device: &Device) -> String {
    use identity_auth::{CanonicalOrigin, ClientKind, Nonce, PairingProof, SyncPairingProof};
    let input = SyncPairingProof {
        host_id: acp_core::model::NodeId::new(ticket["hostId"].as_str().expect("hostId"))
            .expect("host id 规范"),
        device_id: acp_core::model::DeviceId::new(&device.id()).expect("device id 规范"),
        pairing_id: acp_core::model::PairingId::new(
            ticket["pairingId"].as_str().expect("pairingId"),
        )
        .expect("pairing id 规范"),
        pairing_expires_at: identity_auth::Timestamp::new(
            ticket["expiresAt"].as_str().expect("expiresAt"),
        )
        .expect("expiresAt 是规范 RFC 3339 文本"),
        canonical_origin: CanonicalOrigin::parse(
            ticket["canonicalOrigin"].as_str().expect("canonicalOrigin"),
        )
        .expect("规范 canonical origin"),
        device_public_key: device.key.public_key(),
        client_nonce: Nonce::new(&device.client_nonce).expect("规范 nonce"),
        device_name: DEVICE_NAME.to_owned(),
        client_kind: ClientKind::Pwa,
    };
    let transcript = input.transcript().expect("transcript 可装配");
    let output = hmac_sha256(secret, &transcript);
    let proof = PairingProof::try_from_bytes(&output).expect("32 字节 proof");
    support::sync::base64url(proof.as_bytes())
}

/// HMAC-SHA256（与 `identity-auth` 同口径；测试侧只做装配）。
///
/// 与 `sync_e2e.rs` 同款的直写实现：`app` 的测试目标只声明了 `sha2`，不额外引入 `hmac` 依赖。
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    use sha2::{Digest as _, Sha256};
    let mut padded = [0u8; 64];
    for (index, byte) in key.iter().take(64).enumerate() {
        padded[index] = *byte;
    }
    let mut inner = Sha256::new();
    for byte in &padded {
        inner.update([byte ^ 0x36]);
    }
    inner.update(message);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    for byte in &padded {
        outer.update([byte ^ 0x5c]);
    }
    outer.update(inner);
    let output = outer.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&output);
    bytes
}

/// claim 请求体（ticket 提供配对绑定；secret 来自二维码或 lookup 响应）。
fn claim_body(ticket: &Value, secret: &[u8; 32], device: &Device) -> Value {
    json!({
        "protocolVersion": 1,
        "pairingId": ticket["pairingId"],
        "hostId": ticket["hostId"],
        "deviceId": device.id(),
        "deviceName": DEVICE_NAME,
        "clientKind": "pwa",
        "canonicalOrigin": ticket["canonicalOrigin"],
        "devicePublicKey": public_key_base64url(device),
        "clientNonce": device.client_nonce,
        "proof": pairing_proof(ticket, secret, device),
    })
}

/// 用真实 HTTP 端点 claim，返回响应（成功时携带 `pairingRequestId`）。
async fn claim(host: &HostNode, ticket: &Value, secret: &[u8; 32], device: &Device) -> Reply {
    let body = claim_body(ticket, secret, device).to_string();
    post_raw(host, CLAIM_PATH, Some(&host.origin()), None, &body).await
}

// ---------------------------------------------------------------------------------------------
// 成功、单次消耗与「不推进配对状态」
// ---------------------------------------------------------------------------------------------

/// 正确配对码返回完整绑定与 secret；同码第二次失败；lookup 本身不推进状态，返回的 secret 仍可 claim。
#[tokio::test]
async fn lookup_returns_the_binding_consumes_the_code_and_leaves_the_pairing_claimable() {
    let host = HostNode::start("lookup-success", HOST).await;
    let device = Device::new();
    let started = begin(&host, None).await;
    let pairing_id = started["pairingId"].as_str().expect("pairingId").to_owned();
    let code = started["pairingCode"]
        .as_str()
        .expect("pairingCode")
        .to_owned();
    let (ticket, qr_secret) = retrieve(&host, &started).await;
    assert_eq!(pairing_id, ticket["pairingId"].as_str().expect("ticket id"));
    // 响应里的码与二维码 secret 派生出的码逐字相同（同一来源、同一生命周期）。
    let pairing_secret = identity_auth::PairingSecret::try_from_bytes(&qr_secret).expect("secret");
    assert_eq!(
        code,
        identity_auth::derive_pairing_code(&pairing_secret)
            .expect("derive code")
            .as_str(),
        "begin 返回的配对码必须由该配对的 secret 派生"
    );
    assert!(
        code.len() == 8
            && code
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()),
        "配对码是 8 位规范大写形态：{code}"
    );

    let replied = lookup(&host, &code).await;
    assert_eq!(replied.status, 200, "正确码必须成功：{}", replied.body);
    replied.assert_security_headers();
    assert_eq!(replied.body["protocolVersion"], json!(1));
    assert_eq!(replied.body["pairingId"], json!(pairing_id));
    assert_eq!(replied.body["hostId"], json!(host.host_id()));
    assert_eq!(replied.body["canonicalOrigin"], json!(host.origin()));
    assert_eq!(replied.body["expiresAt"], started["expiresAt"]);
    assert_eq!(
        replied.body["hostPublicKey"],
        json!(support::sync::base64url(host.public_key().await.as_bytes()))
    );
    assert_eq!(
        replied.body["pairingSecret"], ticket["pairingSecret"],
        "lookup 返回完整 256 bit secret，与二维码载荷逐字相同"
    );

    // lookup 不推进状态：管理面仍是 pending。
    let status = host
        .admin(
            server::local_admin::Method::DevicePairStatus,
            json!({ "pairingId": pairing_id }),
        )
        .await;
    assert_eq!(status["state"], json!("pending"), "{status}");
    assert!(
        status.get("pairingCode").is_none(),
        "status 不得回显配对码：{status}"
    );

    // 同码第二次：统一 401；与一个格式合法的随机码不可区分（错误码与消息相同）。
    let second = lookup(&host, &code).await;
    second.assert_code_invalid();
    let random = lookup(&host, "ABCDEFGH").await;
    random.assert_code_invalid();
    assert_eq!(second.body["code"], random.body["code"]);
    assert_eq!(second.body["message"], random.body["message"]);
    assert_eq!(second.body["retryable"], random.body["retryable"]);

    // 返回的 secret 可直接用于 claim：lookup 没有推进配对状态。
    let claimed = claim(&host, &replied.body, &qr_secret, &device).await;
    assert_eq!(
        claimed.status, 201,
        "lookup 后仍必须可 claim：{}",
        claimed.body
    );
    assert_eq!(claimed.body["status"], json!("pending_confirmation"));
    let claimed_status = host
        .admin(
            server::local_admin::Method::DevicePairStatus,
            json!({ "pairingId": pairing_id }),
        )
        .await;
    assert_eq!(
        claimed_status["state"],
        json!("claimed"),
        "{claimed_status}"
    );
    host.stop().await;
}

// ---------------------------------------------------------------------------------------------
// 统一失败面与来源校验顺序
// ---------------------------------------------------------------------------------------------

/// 码格式/缺字段/未知字段/坏 JSON 都是同一个 401；来源不符不消耗码；不同的 Host 在接入层即被拒。
#[tokio::test]
async fn every_lookup_failure_is_the_same_401_and_a_wrong_origin_does_not_burn_the_code() {
    let host = HostNode::start("lookup-failures", HOST).await;
    let started = begin(&host, None).await;
    let code = started["pairingCode"]
        .as_str()
        .expect("pairingCode")
        .to_owned();

    for body in [
        // 长度错误
        r#"{"protocolVersion":1,"pairingCode":"ABCD"}"#.to_owned(),
        // Crockford 之外的字符（U）
        r#"{"protocolVersion":1,"pairingCode":"ABCD-EFGU"}"#.to_owned(),
        // 缺字段
        r#"{"protocolVersion":1}"#.to_owned(),
        // 未知字段
        format!(r#"{{"protocolVersion":1,"pairingCode":"{code}","extra":true}}"#),
        // 坏 JSON
        "{".to_owned(),
    ] {
        let replied = post_raw(&host, LOOKUP_PATH, Some(&host.origin()), None, &body).await;
        replied.assert_code_invalid();
        replied.assert_security_headers();
    }

    // 来源不符：统一 401，且**不**进入配对码查找（因此码仍可用）。
    let wrong_origin = post_raw(
        &host,
        LOOKUP_PATH,
        Some("https://attacker.example.invalid"),
        None,
        &json!({ "protocolVersion": 1, "pairingCode": code }).to_string(),
    )
    .await;
    wrong_origin.assert_code_invalid();
    let accepted = lookup(&host, &code).await;
    assert_eq!(
        accepted.status, 200,
        "来源不符的请求不得消耗配对码：{}",
        accepted.body
    );

    // Host 由接入层判定：不在允许清单内的 Host 在处理器之前就失败，且仍带安全响应头。用**新配对**的
    // 配对码：上面的 `code` 已被成功 lookup 消耗，拿它断言无法区分「被 Host 边界拒绝」与「码已失效」。
    let fresh = begin(&host, None).await;
    let fresh_code = fresh["pairingCode"]
        .as_str()
        .expect("pairingCode")
        .to_owned();
    let wrong_host = post_raw(
        &host,
        LOOKUP_PATH,
        Some(&host.origin()),
        Some("attacker.example.invalid"),
        &json!({ "protocolVersion": 1, "pairingCode": fresh_code }).to_string(),
    )
    .await;
    assert_ne!(wrong_host.status, 200, "不允许的 Host 不得成功");
    wrong_host.assert_security_headers();
    assert!(
        wrong_host.body.get("pairingSecret").is_none(),
        "Host 不匹配不得返回任何材料：{}",
        wrong_host.body
    );
    // 请求没有到达处理器：新码仍可正常兑换。
    let survived = lookup(&host, &fresh_code).await;
    assert_eq!(
        survived.status, 200,
        "Host 不匹配的请求不得消耗配对码：{}",
        survived.body
    );
    host.stop().await;
}

// ---------------------------------------------------------------------------------------------
// 独立限流
// ---------------------------------------------------------------------------------------------

/// lookup 每来源 10 次/分钟；lookup 的消耗不占 claim 的额度（两者各自独立）。
#[tokio::test]
async fn lookup_and_claim_have_independent_rate_limits() {
    let host = HostNode::start("lookup-ratelimit", HOST).await;
    let device = Device::new();
    let started = begin(&host, None).await;
    let (ticket, secret) = retrieve(&host, &started).await;

    // 十次失败 lookup 用满 lookup 额度（随机码形状合法）。
    for _ in 0..10 {
        let replied = lookup(&host, "ABCDEFGH").await;
        replied.assert_code_invalid();
    }
    let limited = lookup(&host, "ABCDEFGH").await;
    assert_eq!(limited.status, 429, "超限必须 429：{}", limited.body);
    assert_eq!(limited.body["code"], json!("resource.rate_limited"));
    limited.assert_security_headers();

    // claim 使用**独立**额度：同来源紧接着 claim 仍被处理（不是 429）。
    let claimed = claim(&host, &ticket, &secret, &device).await;
    assert_eq!(
        claimed.status, 201,
        "lookup 的额度不得占用 claim：{}",
        claimed.body
    );
    host.stop().await;
}

/// claim 的额度耗尽不阻塞 lookup（反向独立）。
#[tokio::test]
async fn claim_exhaustion_does_not_block_lookup() {
    let host = HostNode::start("claim-ratelimit", HOST).await;
    let device = Device::new();
    let started = begin(&host, None).await;
    let code = started["pairingCode"]
        .as_str()
        .expect("pairingCode")
        .to_owned();
    let (ticket, secret) = retrieve(&host, &started).await;

    // 以不存在的 pairingId 发起十条 claim：它们在记录查找处 404，不会消耗任何真实配对的失败计数。
    for _ in 0..10 {
        let mut body = claim_body(&ticket, &secret, &device);
        body["pairingId"] = json!(uuid_text());
        let replied = post_raw(
            &host,
            CLAIM_PATH,
            Some(&host.origin()),
            None,
            &body.to_string(),
        )
        .await;
        assert_eq!(replied.status, 404, "{}", replied.body);
    }
    let mut body = claim_body(&ticket, &secret, &device);
    body["pairingId"] = json!(uuid_text());
    let limited = post_raw(
        &host,
        CLAIM_PATH,
        Some(&host.origin()),
        None,
        &body.to_string(),
    )
    .await;
    assert_eq!(limited.status, 429, "claim 超限必须 429：{}", limited.body);

    let looked_up = lookup(&host, &code).await;
    assert_eq!(
        looked_up.status, 200,
        "claim 的额度不得占用 lookup：{}",
        looked_up.body
    );
    host.stop().await;
}

// ---------------------------------------------------------------------------------------------
// 已认领的配对
// ---------------------------------------------------------------------------------------------

/// 已认领（等待本机确认）的配对不可再 lookup：统一 401，且不返回 secret。
#[tokio::test]
async fn a_claimed_pairing_is_not_lookupable() {
    let host = HostNode::start("lookup-claimed", HOST).await;
    let device = Device::new();
    let started = begin(&host, None).await;
    let pairing_id = started["pairingId"].as_str().expect("pairingId").to_owned();
    let code = started["pairingCode"]
        .as_str()
        .expect("pairingCode")
        .to_owned();
    let (ticket, secret) = retrieve(&host, &started).await;

    // 用二维码 secret 认领（配对码尚未被消耗）。
    let claimed = claim(&host, &ticket, &secret, &device).await;
    assert_eq!(claimed.status, 201, "{}", claimed.body);
    let status = host
        .admin(
            server::local_admin::Method::DevicePairStatus,
            json!({ "pairingId": pairing_id }),
        )
        .await;
    assert_eq!(status["state"], json!("claimed"), "{status}");

    lookup(&host, &code).await.assert_code_invalid();
    // 配对状态不因一次失败的 lookup 改变。
    let after = host
        .admin(
            server::local_admin::Method::DevicePairStatus,
            json!({ "pairingId": pairing_id }),
        )
        .await;
    assert_eq!(after["state"], json!("claimed"), "{after}");
    host.stop().await;
}

// ---------------------------------------------------------------------------------------------
// 过期与清理接线
// ---------------------------------------------------------------------------------------------

/// 已过期的配对不可 lookup（统一 401）；周期维护随后清除内存 secret 并终结持久记录。
#[tokio::test]
async fn an_expired_pairing_is_not_lookupable_and_maintenance_clears_its_secret() {
    let host = HostNode::start("lookup-expired", HOST).await;
    let started = begin(&host, Some(1000)).await;
    let pairing_id = started["pairingId"].as_str().expect("pairingId").to_owned();
    let code = started["pairingCode"]
        .as_str()
        .expect("pairingCode")
        .to_owned();
    let id = acp_core::model::PairingId::new(&pairing_id).expect("pairing id");
    assert!(host.authority().has_secret(&id), "登记后内存持有 secret");

    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    lookup(&host, &code).await.assert_code_invalid();

    // 维护前：持久状态按时间呈现为 expired，内存 secret 仍在（lookup 的过期判定不依赖维护）。
    let before = host
        .admin(
            server::local_admin::Method::DevicePairStatus,
            json!({ "pairingId": pairing_id }),
        )
        .await;
    assert_eq!(before["state"], json!("expired"), "{before}");
    assert!(host.authority().has_secret(&id));

    let terminated =
        app::daemon::expire_due_pairings(host.core(), host.authority(), &host.authority().now())
            .await
            .expect("periodic pairing sweep");
    assert_eq!(terminated, 1, "到期的未确认配对必须被终结");
    assert!(
        !host.authority().has_secret(&id),
        "过期维护必须清除内存 secret"
    );
    let stored = host
        .store()
        .pairing(&id)
        .await
        .expect("pairing")
        .expect("pairing row");
    assert_eq!(stored.state(), acp_core::model::PairingState::Expired);
    host.stop().await;
}

/// 周期维护只清到期配对：仍有效的配对保留 secret 与配对码。
#[tokio::test]
async fn maintenance_keeps_still_valid_pairings_untouched() {
    let host = HostNode::start("lookup-maintenance", HOST).await;
    let expiring = begin(&host, Some(1000)).await;
    let valid = begin(&host, None).await;
    let expiring_id =
        acp_core::model::PairingId::new(expiring["pairingId"].as_str().expect("pairingId"))
            .expect("pairing id");
    let valid_id = acp_core::model::PairingId::new(valid["pairingId"].as_str().expect("pairingId"))
        .expect("pairing id");
    let expiring_code = expiring["pairingCode"].as_str().expect("pairingCode");
    let valid_code = valid["pairingCode"].as_str().expect("pairingCode");

    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    app::daemon::expire_due_pairings(host.core(), host.authority(), &host.authority().now())
        .await
        .expect("periodic pairing sweep");

    assert!(!host.authority().has_secret(&expiring_id));
    assert!(
        host.authority().has_secret(&valid_id),
        "仍有效的配对不得被清理"
    );
    // 扫码取回与配对码同源判定：仍有效的配对可反复取回，过期配对的凭据统一 401。
    let (valid_token, _) = qr_payload(valid["pairingUrl"].as_str().expect("pairingUrl"));
    assert_eq!(
        qr_lookup(&host, &valid_token).await.status,
        200,
        "仍有效的配对必须可扫码取回"
    );
    let (expired_token, _) = qr_payload(expiring["pairingUrl"].as_str().expect("pairingUrl"));
    qr_lookup(&host, &expired_token).await.assert_qr_invalid();
    let parsed_valid = identity_auth::PairingCode::parse(valid_code).expect("合法码");
    let looked_up = host
        .authority()
        .consume_pairing_code(&parsed_valid)
        .expect("仍有效的配对码必须可查找");
    assert_eq!(looked_up.pairing, valid_id);
    let parsed_expired = identity_auth::PairingCode::parse(expiring_code).expect("合法码");
    assert!(
        host.authority()
            .consume_pairing_code(&parsed_expired)
            .is_none(),
        "过期配对的码必须失效"
    );

    let expired = host
        .admin(
            server::local_admin::Method::DevicePairStatus,
            json!({ "pairingId": expiring["pairingId"] }),
        )
        .await;
    assert_eq!(expired["state"], json!("expired"), "{expired}");
    let pending = host
        .admin(
            server::local_admin::Method::DevicePairStatus,
            json!({ "pairingId": valid["pairingId"] }),
        )
        .await;
    assert_eq!(pending["state"], json!("pending"), "{pending}");
    host.stop().await;
}

/// 启动路径：`reset_memory` 清空全部内存 secret/码，`unrecoverable_after_restart` 终结未确认配对；
/// 已批准配对（与设备信任）不被改写。
#[tokio::test]
async fn restart_recovery_clears_memory_and_terminates_only_unconfirmed_pairings() {
    let host = HostNode::start("lookup-restart", HOST).await;
    let device = Device::new();
    // 已批准的配对：完整走 begin → claim → confirm。
    let approved = begin(&host, None).await;
    let approved_id =
        acp_core::model::PairingId::new(approved["pairingId"].as_str().expect("pairingId"))
            .expect("pairing id");
    let (ticket, secret) = retrieve(&host, &approved).await;
    let claimed = claim(&host, &ticket, &secret, &device).await;
    assert_eq!(claimed.status, 201, "{}", claimed.body);
    host.admin(
        server::local_admin::Method::DevicePairConfirm,
        json!({ "pairingId": approved["pairingId"] }),
    )
    .await;
    let approved_status = host
        .admin(
            server::local_admin::Method::DevicePairStatus,
            json!({ "pairingId": approved["pairingId"] }),
        )
        .await;
    assert_eq!(approved_status["state"], json!("approved"));

    // 未确认的配对：重启后已无法验密。
    let unconfirmed = begin(&host, None).await;
    let unconfirmed_id =
        acp_core::model::PairingId::new(unconfirmed["pairingId"].as_str().expect("pairingId"))
            .expect("pairing id");
    let unconfirmed_code =
        identity_auth::PairingCode::parse(unconfirmed["pairingCode"].as_str().expect("code"))
            .expect("合法码");
    assert!(host.authority().has_secret(&unconfirmed_id));

    let terminated = app::daemon::recover_pairings_after_restart(host.core(), host.authority())
        .await
        .expect("restart recovery");
    assert_eq!(terminated, 1, "只有未确认的配对需要被终结");
    assert!(!host.authority().has_secret(&unconfirmed_id));
    assert!(
        !host.authority().has_secret(&approved_id),
        "重启语义要求清空全部内存 secret"
    );
    assert!(
        host.authority()
            .consume_pairing_code(&unconfirmed_code)
            .is_none()
    );
    // 已批准记录不被改写，设备信任仍然存在。
    assert_eq!(
        host.store()
            .pairing(&approved_id)
            .await
            .expect("pairing")
            .expect("pairing row")
            .state(),
        acp_core::model::PairingState::Approved
    );
    let devices = host
        .admin(server::local_admin::Method::DeviceList, json!({}))
        .await;
    assert_eq!(devices["devices"].as_array().expect("devices").len(), 1);
    assert_eq!(devices["devices"][0]["deviceId"], json!(device.id()));
    assert!(
        !support::sync::contains_key_deep(&devices, "pairingCode"),
        "device.list 不得返回配对码：{devices}"
    );
    let unconfirmed_status = host
        .admin(
            server::local_admin::Method::DevicePairStatus,
            json!({ "pairingId": unconfirmed["pairingId"] }),
        )
        .await;
    assert_eq!(unconfirmed_status["state"], json!("expired"));
    host.stop().await;
}

// ---------------------------------------------------------------------------------------------
// 扫码资料取回：成功、统一失败、独立限流与接入层边界
// ---------------------------------------------------------------------------------------------

/// 扫码取回的成功响应与配对码 lookup **同形**（七个字段），重复取回逐字相同，且不推进状态、
/// 不消耗网页配对码；网页码消耗后扫码凭据仍可取回，直到认领关闭入口。
#[tokio::test]
async fn qr_lookup_returns_the_same_binding_repeatedly_without_advancing_state() {
    let host = HostNode::start("qr-success", HOST).await;
    let device = Device::new();
    let started = begin(&host, None).await;
    let pairing_id = started["pairingId"].as_str().expect("pairingId").to_owned();
    let begin_object = started.as_object().expect("begin 结果是 JSON object");
    assert_eq!(
        begin_object.len(),
        4,
        "device.pair.begin 仍返回四个键：{started}"
    );
    for key in ["pairingId", "pairingUrl", "pairingCode", "expiresAt"] {
        assert!(begin_object.contains_key(key), "缺少 `{key}`：{started}");
    }
    let code = started["pairingCode"]
        .as_str()
        .expect("pairingCode")
        .to_owned();
    let (token, _fingerprint) = qr_payload(started["pairingUrl"].as_str().expect("pairingUrl"));

    let first = qr_lookup(&host, &token).await;
    assert_eq!(first.status, 200, "扫码取回必须成功：{}", first.body);
    first.assert_security_headers();
    let object = first.body.as_object().expect("响应是 JSON object");
    assert_eq!(object.len(), 7, "成功响应恰好七个字段：{}", first.body);
    for key in [
        "protocolVersion",
        "pairingId",
        "hostId",
        "hostPublicKey",
        "canonicalOrigin",
        "pairingSecret",
        "expiresAt",
    ] {
        assert!(object.contains_key(key), "缺少字段 `{key}`：{}", first.body);
    }
    assert_eq!(first.body["protocolVersion"], json!(1));
    assert_eq!(first.body["pairingId"], json!(pairing_id));
    assert_eq!(first.body["hostId"], json!(host.host_id()));
    assert_eq!(first.body["canonicalOrigin"], json!(host.origin()));
    assert_eq!(first.body["expiresAt"], started["expiresAt"]);
    assert_eq!(
        first.body["hostPublicKey"],
        json!(support::sync::base64url(host.public_key().await.as_bytes()))
    );
    let secret: [u8; 32] =
        base64_decode(first.body["pairingSecret"].as_str().expect("pairingSecret"))
            .as_slice()
            .try_into()
            .expect("pairing secret 是 32 字节");

    // 重复取回：逐字相同。
    let second = qr_lookup(&host, &token).await;
    assert_eq!(second.status, 200);
    assert_eq!(second.body, first.body, "重复取回必须返回相同资料");

    // 状态仍是 pending：取回不推进配对状态。
    let status = host
        .admin(
            server::local_admin::Method::DevicePairStatus,
            json!({ "pairingId": pairing_id }),
        )
        .await;
    assert_eq!(status["state"], json!("pending"), "{status}");

    // 网页配对码仍可成功取回，且交付同一个 secret（两入口同一份绑定）。
    let code_lookup = lookup(&host, &code).await;
    assert_eq!(code_lookup.status, 200, "{}", code_lookup.body);
    assert_eq!(
        code_lookup.body["pairingSecret"],
        first.body["pairingSecret"]
    );
    // 配对码被消费后，扫码凭据仍可在未认领期间重复取回。
    let third = qr_lookup(&host, &token).await;
    assert_eq!(third.status, 200, "{}", third.body);
    assert_eq!(third.body, first.body);

    // 取回的 secret 可直接 claim；认领生效后取回入口关闭。
    let claimed = claim(&host, &first.body, &secret, &device).await;
    assert_eq!(claimed.status, 201, "{}", claimed.body);
    qr_lookup(&host, &token).await.assert_qr_invalid();
    host.stop().await;
}

/// 请求形状、凭据不存在、来源缺失/不符都是同一个 `401 pairing.qr_invalid`；Host 由接入层拒绝；
/// 非 JSON 沿用既有 400；失败不消耗凭据。
#[tokio::test]
async fn every_qr_lookup_failure_is_the_same_401() {
    let host = HostNode::start("qr-failures", HOST).await;
    let started = begin(&host, None).await;
    let (token, _fingerprint) = qr_payload(started["pairingUrl"].as_str().expect("pairingUrl"));
    let token_text = support::sync::base64url(&token);
    let unknown_token = support::sync::base64url(&[0_u8; 32]);

    for body in [
        r#"{"protocolVersion":2,"qrToken":"AAAA"}"#.to_owned(),
        r#"{"protocolVersion":1}"#.to_owned(),
        r#"{"protocolVersion":1,"qrToken":"AAAA"}"#.to_owned(),
        format!(r#"{{"protocolVersion":1,"qrToken":"{unknown_token}"}}"#),
        format!(r#"{{"protocolVersion":1,"qrToken":"{token_text}","extra":true}}"#),
    ] {
        let replied = post_raw(&host, QR_LOOKUP_PATH, Some(&host.origin()), None, &body).await;
        replied.assert_qr_invalid();
        replied.assert_security_headers();
    }

    // 坏 JSON 不是凭据失败：§7.4 要求它与非 JSON 一样返回 400 `protocol.schema_invalid`。
    let bad_json = post_raw(&host, QR_LOOKUP_PATH, Some(&host.origin()), None, "{").await;
    assert_eq!(bad_json.status, 400, "{}", bad_json.body);
    assert_eq!(
        bad_json.body["code"],
        json!("protocol.schema_invalid"),
        "{}",
        bad_json.body
    );
    bad_json.assert_security_headers();

    // 来源缺失与来源不符：统一 401，且不进入凭据查找。
    let missing_origin = request_raw(
        &host,
        "POST",
        QR_LOOKUP_PATH,
        None,
        None,
        Some("application/json"),
        format!(r#"{{"protocolVersion":1,"qrToken":"{token_text}"}}"#).as_bytes(),
    )
    .await;
    missing_origin.assert_qr_invalid();
    let wrong_origin = post_raw(
        &host,
        QR_LOOKUP_PATH,
        Some("https://attacker.example.invalid"),
        None,
        &format!(r#"{{"protocolVersion":1,"qrToken":"{token_text}"}}"#),
    )
    .await;
    wrong_origin.assert_qr_invalid();

    // 非 JSON 沿用既有 400（不新增 415）。
    let non_json = request_raw(
        &host,
        "POST",
        QR_LOOKUP_PATH,
        Some(&host.origin()),
        None,
        Some("text/plain"),
        format!(r#"{{"protocolVersion":1,"qrToken":"{token_text}"}}"#).as_bytes(),
    )
    .await;
    assert_eq!(non_json.status, 400, "{}", non_json.body);
    assert_eq!(
        non_json.body["code"],
        json!("protocol.schema_invalid"),
        "{}",
        non_json.body
    );
    non_json.assert_security_headers();

    // 不在允许清单内的 Host：接入层在处理器之前拒绝，仍带安全响应头且不泄露材料。
    let wrong_host = post_raw(
        &host,
        QR_LOOKUP_PATH,
        Some(&host.origin()),
        Some("attacker.example.invalid"),
        &format!(r#"{{"protocolVersion":1,"qrToken":"{token_text}"}}"#),
    )
    .await;
    assert_ne!(wrong_host.status, 200, "不允许的 Host 不得成功");
    wrong_host.assert_security_headers();
    assert!(
        wrong_host.body.get("pairingSecret").is_none(),
        "Host 不匹配不得返回材料：{}",
        wrong_host.body
    );

    // 上述失败都不消耗凭据：正确取回仍然成功。
    assert_eq!(qr_lookup(&host, &token).await.status, 200);
    host.stop().await;
}

/// 扫码取回有**独立**的 10 次/分钟额度：超限 429 + `Retry-After`，且不占网页码 lookup 或 claim 的额度；
/// 反向亦然（本用例覆盖两个方向）。
#[tokio::test]
async fn qr_lookup_rate_limit_is_independent_and_carries_retry_after() {
    let host = HostNode::start("qr-ratelimit", HOST).await;
    let device = Device::new();
    let started = begin(&host, None).await;
    let code = started["pairingCode"]
        .as_str()
        .expect("pairingCode")
        .to_owned();
    let (token, _fingerprint) = qr_payload(started["pairingUrl"].as_str().expect("pairingUrl"));
    let unknown = support::sync::base64url(&[0_u8; 32]);

    // 用满扫码额度（合法形状但不存在凭据：统一 401）。
    for _ in 0..10 {
        let replied = post_raw(
            &host,
            QR_LOOKUP_PATH,
            Some(&host.origin()),
            None,
            &format!(r#"{{"protocolVersion":1,"qrToken":"{unknown}"}}"#),
        )
        .await;
        replied.assert_qr_invalid();
    }
    let limited = qr_lookup(&host, &token).await;
    assert_eq!(limited.status, 429, "超限必须 429：{}", limited.body);
    assert_eq!(limited.body["code"], json!("resource.rate_limited"));
    limited.assert_security_headers();
    let retry_after = limited
        .header("retry-after")
        .expect("429 必须带 Retry-After")
        .parse::<u64>()
        .expect("Retry-After 是整数秒");
    assert!(retry_after >= 1, "Retry-After 必须可执行：{retry_after}");
    assert!(
        limited.body["details"]["retryAfterMs"]
            .as_u64()
            .is_some_and(|ms| ms > 0),
        "details.retryAfterMs 必须给出毫秒精度：{}",
        limited.body
    );

    // 网页码 lookup 与 claim 的额度不受影响。
    let code_lookup = lookup(&host, &code).await;
    assert_eq!(code_lookup.status, 200, "{}", code_lookup.body);
    let secret: [u8; 32] = base64_decode(
        code_lookup.body["pairingSecret"]
            .as_str()
            .expect("pairingSecret"),
    )
    .as_slice()
    .try_into()
    .expect("pairing secret 是 32 字节");
    let claimed = claim(&host, &code_lookup.body, &secret, &device).await;
    assert_eq!(claimed.status, 201, "{}", claimed.body);
    host.stop().await;
}

/// 反向独立：网页码 lookup 或 claim 的额度耗尽都不阻塞扫码取回（各自独立额度）。
#[tokio::test]
async fn lookup_and_claim_exhaustion_do_not_block_qr_lookup() {
    let host = HostNode::start("qr-ratelimit-reverse", HOST).await;
    let device = Device::new();

    // 网页码 lookup 的额度耗尽。
    let started = begin(&host, None).await;
    let (token, _) = qr_payload(started["pairingUrl"].as_str().expect("pairingUrl"));
    for _ in 0..10 {
        lookup(&host, "ABCDEFGH").await.assert_code_invalid();
    }
    assert_eq!(lookup(&host, "ABCDEFGH").await.status, 429);
    assert_eq!(
        qr_lookup(&host, &token).await.status,
        200,
        "lookup 的额度不得占用 qr-lookup"
    );

    // claim 的额度耗尽（不存在的 pairingId 在记录查找处 404；额度用尽后 429）。
    let second = begin(&host, None).await;
    let (second_token, _) = qr_payload(second["pairingUrl"].as_str().expect("pairingUrl"));
    let ticket = qr_lookup(&host, &second_token).await.body;
    let secret: [u8; 32] = base64_decode(ticket["pairingSecret"].as_str().expect("pairingSecret"))
        .as_slice()
        .try_into()
        .expect("pairing secret 是 32 字节");
    for _ in 0..10 {
        let mut body = claim_body(&ticket, &secret, &device);
        body["pairingId"] = json!(uuid_text());
        let replied = post_raw(
            &host,
            CLAIM_PATH,
            Some(&host.origin()),
            None,
            &body.to_string(),
        )
        .await;
        assert_eq!(replied.status, 404, "{}", replied.body);
    }
    let mut body = claim_body(&ticket, &secret, &device);
    body["pairingId"] = json!(uuid_text());
    assert_eq!(
        post_raw(
            &host,
            CLAIM_PATH,
            Some(&host.origin()),
            None,
            &body.to_string()
        )
        .await
        .status,
        429
    );
    assert_eq!(
        qr_lookup(&host, &second_token).await.status,
        200,
        "claim 的额度不得占用 qr-lookup"
    );
    host.stop().await;
}

/// 方法错误、请求体超限、Host 拒绝与 429 等预处理响应同样带四个安全头与 no-store；405/413 不进入处理器。
#[tokio::test]
async fn qr_lookup_preprocessing_responses_carry_security_headers() {
    let host = HostNode::start("qr-preprocessing", HOST).await;
    let started = begin(&host, None).await;
    let (token, _) = qr_payload(started["pairingUrl"].as_str().expect("pairingUrl"));
    let token_text = support::sync::base64url(&token);

    let method_not_allowed = request_raw(
        &host,
        "GET",
        QR_LOOKUP_PATH,
        Some(&host.origin()),
        None,
        None,
        b"",
    )
    .await;
    assert_eq!(
        method_not_allowed.status, 405,
        "{}",
        method_not_allowed.body
    );
    method_not_allowed.assert_security_headers();

    let mut oversized =
        format!(r#"{{"protocolVersion":1,"qrToken":"{token_text}","pad":""#).into_bytes();
    oversized.resize(16 * 1024 + 1, b'x');
    oversized.extend_from_slice(b"\"}");
    let too_large = request_raw(
        &host,
        "POST",
        QR_LOOKUP_PATH,
        Some(&host.origin()),
        None,
        Some("application/json"),
        &oversized,
    )
    .await;
    assert_eq!(too_large.status, 413, "{}", too_large.body);
    too_large.assert_security_headers();

    // 413 在处理器之前返回：凭据没有被消耗。
    assert_eq!(qr_lookup(&host, &token).await.status, 200);

    let wrong_host = request_raw(
        &host,
        "POST",
        QR_LOOKUP_PATH,
        Some(&host.origin()),
        Some("attacker.example.invalid"),
        Some("application/json"),
        format!(r#"{{"protocolVersion":1,"qrToken":"{token_text}"}}"#).as_bytes(),
    )
    .await;
    assert_eq!(wrong_host.status, 400, "{}", wrong_host.body);
    wrong_host.assert_security_headers();
    host.stop().await;
}

/// 认领（含幂等 Repeat）关闭扫码取回入口：认领之后开始的取回一律 401；相同内容的重试返回首次结果，
/// 不同内容的重试冲突；两个入口争用同一 claim 状态机，至多一次认领。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claim_closes_qr_retrieval_and_repeat_claim_is_idempotent() {
    let host = HostNode::start("qr-claim", HOST).await;
    let device = Device::new();
    let started = begin(&host, None).await;
    let (ticket, secret) = retrieve(&host, &started).await;
    let (token, _) = qr_payload(started["pairingUrl"].as_str().expect("pairingUrl"));

    let claimed = claim(&host, &ticket, &secret, &device).await;
    assert_eq!(claimed.status, 201, "{}", claimed.body);
    let first_request_id = claimed.body["pairingRequestId"].clone();
    qr_lookup(&host, &token).await.assert_qr_invalid();

    // 认领前获准的资料仍可用于幂等重试：相同内容返回首次的 pairingRequestId。
    let repeated = claim(&host, &ticket, &secret, &device).await;
    assert_eq!(repeated.status, 201, "{}", repeated.body);
    assert_eq!(repeated.body["pairingRequestId"], first_request_id);
    qr_lookup(&host, &token).await.assert_qr_invalid();

    // 不同内容的重复 claim：冲突，不创建第二条配对请求。
    let other = Device::new();
    let conflict = claim(&host, &ticket, &secret, &other).await;
    assert_eq!(conflict.status, 409, "{}", conflict.body);
    assert_eq!(conflict.body["code"], json!("pairing.already_claimed"));

    // 并发取回与认领交错：所有成功取回都必须早于认领生效且资料一致；认领后不再成功。
    let raced = begin(&host, None).await;
    let (race_ticket, race_secret) = retrieve(&host, &raced).await;
    let (race_token, _) = qr_payload(raced["pairingUrl"].as_str().expect("pairingUrl"));
    let claim_future = claim(&host, &race_ticket, &race_secret, &device);
    let first_lookup = qr_lookup(&host, &race_token);
    let second_lookup = qr_lookup(&host, &race_token);
    let (raced_claim, first_race, second_race) =
        tokio::join!(claim_future, first_lookup, second_lookup);
    assert_eq!(raced_claim.status, 201, "{}", raced_claim.body);
    for replied in [&first_race, &second_race] {
        if replied.status == 200 {
            assert_eq!(replied.body["pairingSecret"], race_ticket["pairingSecret"]);
        } else {
            replied.assert_qr_invalid();
        }
    }
    qr_lookup(&host, &race_token).await.assert_qr_invalid();
    host.stop().await;
}

/// 拒绝、过期与内存失效都不可取回；扫码取回的失败本身**不**累计配对证明失败次数（不使配对失效）。
#[tokio::test]
async fn rejection_expiry_and_invalidation_close_qr_retrieval_without_counting_qr_failures() {
    let host = HostNode::start("qr-states", HOST).await;
    let device = Device::new();

    // 拒绝：认领后由本机拒绝，取回入口保持关闭（reject 只为轮询保留 secret，不重新开放取回）。
    let rejected = begin(&host, None).await;
    let (rejected_ticket, rejected_secret) = retrieve(&host, &rejected).await;
    let (rejected_token, _) = qr_payload(rejected["pairingUrl"].as_str().expect("pairingUrl"));
    let rejected_claim = claim(&host, &rejected_ticket, &rejected_secret, &device).await;
    assert_eq!(rejected_claim.status, 201, "{}", rejected_claim.body);
    host.admin(
        server::local_admin::Method::DevicePairReject,
        json!({ "pairingId": rejected["pairingId"], "reason": null }),
    )
    .await;
    qr_lookup(&host, &rejected_token).await.assert_qr_invalid();

    // 过期（短窗口 + 本地时钟越过 expiresAt）。
    let expiring = begin(&host, Some(1000)).await;
    let (expiring_token, _) = qr_payload(expiring["pairingUrl"].as_str().expect("pairingUrl"));
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    qr_lookup(&host, &expiring_token).await.assert_qr_invalid();

    // 内存失效：同一配对 5 次证明失败后，取回入口关闭。
    let invalidated = begin(&host, None).await;
    let (invalidated_token, _) =
        qr_payload(invalidated["pairingUrl"].as_str().expect("pairingUrl"));
    let invalidated_ticket = qr_lookup(&host, &invalidated_token).await.body;
    let secret: [u8; 32] = base64_decode(
        invalidated_ticket["pairingSecret"]
            .as_str()
            .expect("pairingSecret"),
    )
    .as_slice()
    .try_into()
    .expect("pairing secret 是 32 字节");
    for _ in 0..5 {
        let mut body = claim_body(&invalidated_ticket, &secret, &device);
        body["proof"] = json!(support::sync::base64url(&[0_u8; 32]));
        let replied = post_raw(
            &host,
            CLAIM_PATH,
            Some(&host.origin()),
            None,
            &body.to_string(),
        )
        .await;
        assert_eq!(replied.status, 401, "{}", replied.body);
    }
    qr_lookup(&host, &invalidated_token)
        .await
        .assert_qr_invalid();
    host.stop().await;

    // 新的 listener：扫码额度和失败计数都从零开始。
    let host = HostNode::start("qr-states-failures", HOST).await;

    // 扫码取回的失败不累计证明失败：9 次失败（低于本端点自己的限流）之后仍可正常 claim。
    let fresh = begin(&host, None).await;
    let (_fresh_token, _) = qr_payload(fresh["pairingUrl"].as_str().expect("pairingUrl"));
    for _ in 0..9 {
        qr_lookup(&host, &[0_u8; 32]).await.assert_qr_invalid();
    }
    let (fresh_ticket, fresh_secret) = retrieve(&host, &fresh).await;
    assert_eq!(
        fresh_ticket["pairingId"], fresh["pairingId"],
        "取回失败不得影响配对资格"
    );
    let claimed = claim(&host, &fresh_ticket, &fresh_secret, &device).await;
    assert_eq!(claimed.status, 201, "{}", claimed.body);
    host.stop().await;
}
