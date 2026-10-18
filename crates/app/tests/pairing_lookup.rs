//! Sync 配对码 lookup 端点与过期/重启清理接线的协议级验收（`pairing-code-web-entry` 的 WP3）。
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
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let origin_line = origin
        .map(|origin| format!("Origin: {origin}\r\n"))
        .unwrap_or_default();
    let head = format!(
        "POST {path} HTTP/1.1\r\nHost: {name}\r\n{origin_line}\
         Content-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
        body.len(),
        name = host_header.unwrap_or(host.host_name()),
    );
    let mut stream = tokio::net::TcpStream::connect(host.addr())
        .await
        .expect("连接 loopback");
    stream.write_all(head.as_bytes()).await.expect("写请求头");
    stream.write_all(body.as_bytes()).await.expect("写请求体");
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

/// 二维码 payload（`#data=<无填充 base64url(json)>`）→ `(payload, secret)`。
fn qr_payload(url: &str) -> (Value, [u8; 32]) {
    let fragment = url.split('#').nth(1).expect("二维码 URL 带 fragment");
    let (key, value) = fragment.split_once('=').expect("fragment 是 data=…");
    assert_eq!(key, "data", "二维码 fragment 的键必须是 data");
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, value)
        .expect("payload 是无填充 base64url");
    let payload: Value = serde_json::from_slice(&bytes).expect("payload 是 JSON object");
    let secret = base64::Engine::decode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        payload["pairingSecret"].as_str().expect("pairingSecret"),
    )
    .expect("secret 是无填充 base64url");
    (
        payload,
        secret
            .as_slice()
            .try_into()
            .expect("pairing secret 是 32 字节"),
    )
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
    let (ticket, qr_secret) = qr_payload(started["pairingUrl"].as_str().expect("pairingUrl"));
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
    let (ticket, secret) = qr_payload(started["pairingUrl"].as_str().expect("pairingUrl"));

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
    let (ticket, secret) = qr_payload(started["pairingUrl"].as_str().expect("pairingUrl"));

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
    let (ticket, secret) = qr_payload(started["pairingUrl"].as_str().expect("pairingUrl"));

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
    let (_valid_secret, _) = qr_payload(valid["pairingUrl"].as_str().expect("pairingUrl"));
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
    let (ticket, secret) = qr_payload(approved["pairingUrl"].as_str().expect("pairingUrl"));
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
