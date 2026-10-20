//! 设备短二维码（`#q2=`）的**模拟原生消费方 ↔ 真实 Daemon** 端到端验收
//! （`device-pairing-qr-reference` 的 WP5 任务 5.1 / `PV6`）。
//!
//! 驱动方式与 `support::sync` 之外的其他用例不同：这里启动**真实 `acp-remote` 二进制**
//! （真实单实例锁、真实 SQLite、真实 `identity_auth::Authority`、真实 loopback listener），
//! 通过真实本地管理通道（`device.pair.*`）与真实 HTTP（`/sync/v1/pairing/qr-lookup`、
//! `/sync/v1/pairing/claim`、`/sync/v1/pairing/status`）和真实 WSS（`/sync/v1`）走完
//! 「扫码 → 取回 → 验指纹 → claim → 本机确认 → status → 四消息认证」的完整 C 链路。
//!
//! 为什么值得另起一个文件：`pairing_lookup.rs` 用进程内的 `HostNode` 覆盖端点的状态机细节；
//! 本文件覆盖的是**消费者契约**——二维码地址里只有来源、取回凭据与主机公钥指纹，消费者必须
//! 用同一来源取回、按指纹钉住公钥、再用取回的配对秘密认领。两者不互相替代：前者证明服务端
//! 规则，后者证明「按二维码消费的客户端确实只能这样走」。
//!
//! ## 断言口径
//!
//! - 指纹：测试侧独立解码 65 字节载荷，并对 `qr-lookup` 返回的**原始** 65 字节公钥复算
//!   SHA-256；不复用服务端的编码器，也不信任响应里的任何摘要字段。
//! - 「没有发 claim」：不靠「没看到请求」这种弱证据，而是先断 `device.pair.status` 仍是
//!   `pending`，再用正确的资料认领成功——claim 只成功一次，之前若发过就不可能再成功。
//! - 幂等 / 冲突：重复 claim 必须返回首次的 `pairingRequestId`；换了设备身份的 claim 必须
//!   `409 pairing.already_claimed`，且原配对的 `deviceId` 不变。
//! - 敏感材料：取回凭据（`qrToken`）与配对秘密只在本进程内存里用于计算；断言 Daemon 的
//!   JSON 日志与 stdout/stderr 都不含它们的完整文本，失败信息也只给字段名。
//!
//! 边界：本文件模拟的是**原生消费方**的协议行为，不是 Android/iOS App；相机、keystore 与
//! 真机扫码不在本次交付内（见任务 5.3 的文档状态）。

mod support;

use std::net::SocketAddr;

use serde_json::{Value, json};
use support::sync::{DeviceKey, SyncClient, base64url, nonce_text, uuid_text};
use support::{Daemon, params_of};

/// `daemon.public_origin` 的 host 部分（测试 harness 的基础配置，见 `support::Daemon::build`）。
const HOST: &str = "acpr-test.example.invalid";
/// 二维码地址里的 canonical origin；所有 HTTP/WSS 请求都必须显式带上它。
const ORIGIN: &str = "https://acpr-test.example.invalid";
/// 原生消费方声明的设备名（claim 的 `deviceName`）。
const DEVICE_NAME: &str = "Native Scan Phone";
/// 本机确认时展示的设备名不会变化：确认只按 `pairingId`。
const QR_LOOKUP_PATH: &str = server::sync::pairing::QR_LOOKUP_PATH;
const CLAIM_PATH: &str = server::sync::pairing::CLAIM_PATH;
const STATUS_PATH: &str = server::sync::pairing::STATUS_PATH;

/// 四消息握手时声明的最小 feature 集（服务端按 `supportedFeatures` 协商）。
const SUPPORTED_FEATURES: [&str; 2] = ["core.snapshot.v1", "core.session-activity.v1"];

/// 一次 HTTP 响应（状态码 + 小写响应头 + 解析后的 JSON）。
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

    /// `qr-lookup` 的统一失败：401 + `pairing.qr_invalid`，且不含任何配对材料。
    fn assert_qr_invalid(&self) {
        assert_eq!(self.status, 401, "qr-lookup 失败必须是 401");
        assert_eq!(
            self.body["code"],
            json!("pairing.qr_invalid"),
            "qr-lookup 失败只暴露统一错误码"
        );
        assert!(
            self.body.get("pairingSecret").is_none()
                && self.body.get("pairingId").is_none()
                && self.body.get("hostPublicKey").is_none(),
            "失败响应不得携带任何配对材料"
        );
    }

    fn assert_security_headers(&self) {
        assert_eq!(self.header("cache-control"), Some("no-store"));
        assert_eq!(self.header("pragma"), Some("no-cache"));
        assert_eq!(self.header("referrer-policy"), Some("no-referrer"));
        assert_eq!(self.header("x-content-type-options"), Some("nosniff"));
    }
}

/// 启动真实 Daemon 并返回它实际绑定的 loopback 地址。
fn start_daemon(label: &str) -> (Daemon, SocketAddr) {
    let mut daemon =
        Daemon::configure_with_dev_mode(label, "127.0.0.1:0", "allow_plaintext = true", "");
    daemon.start();
    let status = daemon.ok_value(server::local_admin::Method::DaemonStatus, support::params());
    let listen = status["listen"]
        .as_array()
        .unwrap_or_else(|| panic!("daemon.status 必须报告绑定地址：{status}"));
    assert_eq!(listen.len(), 1, "共享 listener 只报告一个地址");
    let addr: SocketAddr = listen[0]
        .as_str()
        .expect("listen 项是 ip:port 文本")
        .parse()
        .expect("listen 项可解析");
    assert!(addr.ip().is_loopback(), "测试只连 loopback：{addr}");
    (daemon, addr)
}

/// 向真实 listener 发一条 JSON `POST`。`origin` 是显式的目标来源（`None` = 不带该头）。
async fn post_json(addr: SocketAddr, origin: Option<&str>, path: &str, body: &Value) -> Reply {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let bytes = serde_json::to_vec(body).expect("请求体可序列化");
    let origin_line = origin
        .map(|origin| format!("Origin: {origin}\r\n"))
        .unwrap_or_default();
    let head = format!(
        "POST {path} HTTP/1.1\r\nHost: {HOST}\r\n{origin_line}\
         Content-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
        bytes.len()
    );
    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .expect("连接真实 listener");
    stream.write_all(head.as_bytes()).await.expect("写请求头");
    stream.write_all(&bytes).await.expect("写请求体");
    let mut raw = Vec::new();
    tokio::time::timeout(support::sync::IO_TIMEOUT, stream.read_to_end(&mut raw))
        .await
        .expect("读响应超时")
        .expect("读响应");
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((text.as_str(), "{}"));
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or_else(|| panic!("响应状态行无法解析：{head:?}"));
    let headers = head
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    Reply {
        status,
        headers,
        body: serde_json::from_str(body).unwrap_or(Value::Null),
    }
}

/// 新设备短二维码地址 → `(qrToken, hostKeyFingerprint, canonicalOrigin)`。
///
/// 测试侧独立解码：只接受 `https://<origin>/pair#q2=<87 字符>`、无 query、65 字节
/// `0x02 || token || fingerprint` 布局；旧 `#data=` 或其他版本直接失败。
fn decode_device_qr(url: &str) -> ([u8; 32], [u8; 32], String) {
    let (base, fragment) = url.split_once('#').expect("设备二维码地址带 fragment");
    assert!(!base.contains('?'), "短地址不得带 query");
    let origin = base.strip_suffix("/pair").expect("短地址路径必须是 /pair");
    assert!(origin.starts_with("https://"), "外层地址必须是 HTTPS");
    let payload = fragment.strip_prefix("q2=").expect("fragment 只接受 q2=");
    assert!(!payload.contains('#'), "fragment 只能有一个键");
    assert_eq!(payload.len(), 87, "q2 payload 是 87 个 ASCII 字符");
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, payload)
        .expect("q2 payload 是无填充 base64url");
    assert_eq!(bytes.len(), 65, "q2 payload 解出 65 字节");
    assert_eq!(bytes[0], 2, "q2 版本必须是 2");
    let mut token = [0_u8; 32];
    let mut fingerprint = [0_u8; 32];
    token.copy_from_slice(&bytes[1..33]);
    fingerprint.copy_from_slice(&bytes[33..]);
    (token, fingerprint, origin.to_owned())
}

/// `device.pair.begin`（真实本地管理通道）：返回配对标识、短地址与过期时间。
fn begin(daemon: &Daemon) -> Value {
    daemon.ok_value(
        server::local_admin::Method::DevicePairBegin,
        params_of(json!({ "expiresInMs": null })),
    )
}

/// `device.pair.status` 的状态词（`pending|claimed|approved|rejected|expired`）。
fn pairing_state(daemon: &Daemon, pairing_id: &str) -> String {
    let view = daemon.ok_value(
        server::local_admin::Method::DevicePairStatus,
        params_of(json!({ "pairingId": pairing_id })),
    );
    view["state"].as_str().expect("state 是文本").to_owned()
}

/// 用真实 `qr-lookup` 取回配对资料。
async fn qr_lookup(addr: SocketAddr, token: &[u8; 32], origin: &str) -> Reply {
    post_json(
        addr,
        Some(origin),
        QR_LOOKUP_PATH,
        &json!({
            "protocolVersion": 1,
            "qrToken": base64url(token),
        }),
    )
    .await
}

/// 取回资料并**独立**核对二维码钉住的公钥指纹。
///
/// 返回响应正文与 32 字节配对秘密；`hostPublicKey` 必须是 65 字节 SEC1 未压缩点，且
/// `SHA-256(公钥)` 与二维码里的摘要逐字节相同——否则消费方必须停止。
struct Retrieved {
    body: Value,
    secret: [u8; 32],
}

fn verify_material(reply: &Reply, fingerprint: &[u8; 32]) -> Retrieved {
    assert_eq!(reply.status, 200, "扫码取回必须成功");
    reply.assert_security_headers();
    assert_eq!(
        reply.body["canonicalOrigin"],
        json!(ORIGIN),
        "取回资料的 canonicalOrigin 必须等于二维码固定的来源"
    );
    let public_key = base64::Engine::decode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        reply.body["hostPublicKey"]
            .as_str()
            .expect("hostPublicKey 是文本"),
    )
    .expect("hostPublicKey 是无填充 base64url");
    assert_eq!(public_key.len(), 65, "主机公钥是 65 字节");
    assert_eq!(public_key[0], 0x04, "主机公钥是 SEC1 未压缩点");
    use sha2::{Digest as _, Sha256};
    let digest: [u8; 32] = Sha256::digest(&public_key).into();
    assert_eq!(
        &digest, fingerprint,
        "二维码固定的是完整 32 字节主机公钥指纹"
    );
    let secret = base64::Engine::decode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        reply.body["pairingSecret"]
            .as_str()
            .expect("pairingSecret 是文本"),
    )
    .expect("pairingSecret 是无填充 base64url");
    Retrieved {
        body: reply.body.clone(),
        secret: secret.as_slice().try_into().expect("配对秘密是 32 字节"),
    }
}

/// 设备公钥的 wire 形状（65 字节 SEC1，无填充 base64url）。
fn public_key_base64url(device: &DeviceKey) -> String {
    base64url(device.public_key().as_bytes())
}

/// HMAC-SHA256（与 `identity-auth` 同口径；测试侧只做装配）。
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
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&outer.finalize());
    bytes
}

/// claim 证明：`acp-remote/pairing-proof/v1` 域的 HMAC-SHA256。
fn pairing_proof(
    material: &Value,
    secret: &[u8; 32],
    device: &DeviceKey,
    client_nonce: &str,
    device_name: &str,
) -> String {
    use identity_auth::{CanonicalOrigin, ClientKind, Nonce, PairingProof, SyncPairingProof};
    let input = SyncPairingProof {
        host_id: acp_core::model::NodeId::new(material["hostId"].as_str().expect("hostId"))
            .expect("host id 规范"),
        device_id: acp_core::model::DeviceId::new(device.device_id()).expect("device id 规范"),
        pairing_id: acp_core::model::PairingId::new(
            material["pairingId"].as_str().expect("pairingId"),
        )
        .expect("pairing id 规范"),
        pairing_expires_at: identity_auth::Timestamp::new(
            material["expiresAt"].as_str().expect("expiresAt"),
        )
        .expect("expiresAt 是规范 RFC 3339 文本"),
        canonical_origin: CanonicalOrigin::parse(ORIGIN).expect("规范 canonical origin"),
        device_public_key: device.public_key(),
        client_nonce: Nonce::new(client_nonce).expect("规范 nonce"),
        device_name: device_name.to_owned(),
        client_kind: ClientKind::Ios,
    };
    let transcript = input.transcript().expect("transcript 可装配");
    let output = hmac_sha256(secret, &transcript);
    let proof = PairingProof::try_from_bytes(&output).expect("32 字节 proof");
    base64url(proof.as_bytes())
}

/// claim 请求体（材料来自 `qr-lookup`，秘密同样来自取回响应）。
fn claim_body(
    material: &Value,
    secret: &[u8; 32],
    device: &DeviceKey,
    client_nonce: &str,
    device_name: &str,
) -> Value {
    json!({
        "protocolVersion": 1,
        "pairingId": material["pairingId"],
        "hostId": material["hostId"],
        "deviceId": device.device_id(),
        "deviceName": device_name,
        "clientKind": "ios",
        "canonicalOrigin": ORIGIN,
        "devicePublicKey": public_key_base64url(device),
        "clientNonce": client_nonce,
        "proof": pairing_proof(material, secret, device, client_nonce, device_name),
    })
}

/// 用真实 HTTP 端点 claim。
async fn claim(addr: SocketAddr, body: &Value) -> Reply {
    post_json(addr, Some(ORIGIN), CLAIM_PATH, body).await
}

/// status 证明：`acp-remote/pairing-status/v1` 域的 HMAC-SHA256。
fn status_proof(
    material: &Value,
    secret: &[u8; 32],
    device: &DeviceKey,
    pairing_request_id: &str,
    request_nonce: &str,
) -> String {
    use identity_auth::{Nonce, PairingProof, SyncPairingStatus};
    let input = SyncPairingStatus {
        host_id: acp_core::model::NodeId::new(material["hostId"].as_str().expect("hostId"))
            .expect("host id 规范"),
        device_id: acp_core::model::DeviceId::new(device.device_id()).expect("device id 规范"),
        pairing_id: acp_core::model::PairingId::new(
            material["pairingId"].as_str().expect("pairingId"),
        )
        .expect("pairing id 规范"),
        pairing_request_id: identity_auth::PairingRequestId::parse(pairing_request_id)
            .expect("pairingRequestId 规范"),
        request_nonce: Nonce::new(request_nonce).expect("规范 nonce"),
    };
    let transcript = input.transcript().expect("transcript 可装配");
    let output = hmac_sha256(secret, &transcript);
    let proof = PairingProof::try_from_bytes(&output).expect("32 字节 proof");
    base64url(proof.as_bytes())
}

/// 提交一次 status 查询（若已批准必须携带主机身份材料）。
async fn poll_status(
    addr: SocketAddr,
    material: &Value,
    secret: &[u8; 32],
    device: &DeviceKey,
    pairing_request_id: &str,
    request_nonce: &str,
) -> Reply {
    post_json(
        addr,
        Some(ORIGIN),
        STATUS_PATH,
        &json!({
            "protocolVersion": 1,
            "pairingId": material["pairingId"],
            "hostId": material["hostId"],
            "deviceId": device.device_id(),
            "pairingRequestId": pairing_request_id,
            "requestNonce": request_nonce,
            "proof": status_proof(material, secret, device, pairing_request_id, request_nonce),
        }),
    )
    .await
}

/// 携带二维码固定 Origin 的四消息握手（`hostId` 来自取回响应）。
async fn authenticate_with_host(
    addr: SocketAddr,
    host_id: &str,
    device: &DeviceKey,
    client_nonce: &str,
) -> Value {
    let mut client = SyncClient::connect_plain(addr, HOST, ORIGIN).await;
    client.step("四消息握手");
    client
        .authenticate(host_id, device, &SUPPORTED_FEATURES, client_nonce)
        .await
}

/// 断言证据文本里没有完整凭据；失败信息只给字段名，不回显值。
fn assert_absent(haystack: &str, needle: &str, label: &str) {
    assert!(!haystack.contains(needle), "{label}");
}

/// `device.list` 里的设备条目（按 deviceId 查找）。
fn listed_devices(daemon: &Daemon) -> Vec<Value> {
    daemon.ok_value(server::local_admin::Method::DeviceList, support::params())["devices"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// 走完一次合法配对（begin → 短二维码 → qr-lookup → 验指纹 → claim → 确认），返回
/// `(设备, 取回资料, claim 响应, 配对标识)`。
///
/// 这是其余用例的共同前置：每一跳都走真实端点，且 claim 用的秘密**只**来自取回响应。
fn pair_once(daemon: &Daemon, addr: SocketAddr, label: &str) -> (DeviceKey, Retrieved, Value) {
    let started = begin(daemon);
    let pairing_id = started["pairingId"].as_str().expect("pairingId").to_owned();
    assert_eq!(
        started["pairingUrl"].as_str().map(str::len),
        Some(ORIGIN.len() + "/pair".len() + "#q2=".len() + 87),
        "{label}：短地址长度 = 来源 + /pair + #q2= + 87 字符载荷"
    );
    let (token, fingerprint, qr_origin) =
        decode_device_qr(started["pairingUrl"].as_str().expect("pairingUrl"));
    assert_eq!(
        qr_origin, ORIGIN,
        "{label}：二维码来源与 daemon.public_origin 一致"
    );
    assert_eq!(
        pairing_state(daemon, &pairing_id),
        "pending",
        "{label}：起点未认领"
    );

    let replied = support::block_on(qr_lookup(addr, &token, &qr_origin));
    let retrieved = verify_material(&replied, &fingerprint);
    assert_eq!(
        retrieved.body["pairingId"],
        json!(pairing_id),
        "{label}：取回的就是本次配对"
    );

    let device = DeviceKey::new(&uuid_text());
    let client_nonce = nonce_text(0x71);
    let claimed = support::block_on(claim(
        addr,
        &claim_body(
            &retrieved.body,
            &retrieved.secret,
            &device,
            &client_nonce,
            DEVICE_NAME,
        ),
    ));
    assert_eq!(claimed.status, 201, "{label}：取回秘密必须能认领");
    assert_eq!(
        pairing_state(daemon, &pairing_id),
        "claimed",
        "{label}：认领后等待本机确认"
    );

    (device, retrieved, claimed.body)
}

// ---------------------------------------------------------------------------------------------
// 正向：完整 C 链路
// ---------------------------------------------------------------------------------------------

/// 模拟原生消费方对真实 Daemon 的完整 C 链路：
/// 短二维码 →（显式 Origin）取回 → 验 65 字节公钥与 SHA-256 指纹 → 用取回 secret claim →
/// 本机管理确认 → status approved → 携带同一 Origin 的 WSS 四消息认证成功。
#[test]
fn the_native_consumer_completes_the_short_qr_pairing_chain_against_a_real_daemon() {
    let (mut daemon, addr) = start_daemon("device-qr-chain");
    let started = begin(&daemon);
    let pairing_id = started["pairingId"].as_str().expect("pairingId").to_owned();
    let pairing_url = started["pairingUrl"]
        .as_str()
        .expect("pairingUrl")
        .to_owned();

    // ① 短二维码只承载来源、取回凭据与公钥指纹；不出现秘密、配对标识或完整公钥。
    let (token, fingerprint, qr_origin) = decode_device_qr(&pairing_url);
    assert_eq!(qr_origin, ORIGIN, "二维码来源必须是 daemon.public_origin");
    assert_eq!(
        pairing_url.len(),
        ORIGIN.len() + "/pair".len() + "#q2=".len() + 87,
        "短地址长度 = 来源 + /pair + #q2= + 87 字符载荷"
    );
    assert!(
        !pairing_url.contains(started["expiresAt"].as_str().expect("expiresAt")),
        "二维码不得包含过期时间"
    );
    assert!(!pairing_url.contains(&pairing_id), "二维码不得包含配对标识");

    // ② 取回：显式携带同一 Origin；返回的公钥 65 字节、指纹与二维码逐字节相同。
    let replied = support::block_on(qr_lookup(addr, &token, &qr_origin));
    let retrieved = verify_material(&replied, &fingerprint);
    assert_eq!(retrieved.body["pairingId"], json!(pairing_id));
    assert_eq!(retrieved.body["expiresAt"], started["expiresAt"]);
    assert_eq!(
        pairing_state(&daemon, &pairing_id),
        "pending",
        "取回不推进状态"
    );

    // ③ 认领：HMAC 的密钥就是取回响应里的 pairingSecret。
    let device = DeviceKey::new(&uuid_text());
    let client_nonce = nonce_text(0x71);
    let claim_body = claim_body(
        &retrieved.body,
        &retrieved.secret,
        &device,
        &client_nonce,
        DEVICE_NAME,
    );
    let claimed = support::block_on(claim(addr, &claim_body));
    assert_eq!(claimed.status, 201, "取回秘密必须能认领");
    let pairing_request_id = claimed.body["pairingRequestId"]
        .as_str()
        .expect("pairingRequestId")
        .to_owned();
    assert_eq!(pairing_state(&daemon, &pairing_id), "claimed");

    // ④ 认领已生效：之后**开始**的取回不再交付任何材料（不因内存仍保留秘密而重开）。
    let after_claim = support::block_on(qr_lookup(addr, &token, &qr_origin));
    after_claim.assert_qr_invalid();
    after_claim.assert_security_headers();

    // ⑤ 本机管理确认 → status approved（证明用过取回的秘密且设备身份就是刚认领的那一个）。
    let confirmed = daemon.ok_value(
        server::local_admin::Method::DevicePairConfirm,
        params_of(json!({ "pairingId": pairing_id })),
    );
    assert_eq!(
        confirmed["deviceId"],
        json!(device.device_id()),
        "确认必须创建该设备的信任记录"
    );
    assert_eq!(pairing_state(&daemon, &pairing_id), "approved");

    let status = support::block_on(poll_status(
        addr,
        &retrieved.body,
        &retrieved.secret,
        &device,
        &pairing_request_id,
        &nonce_text(0x72),
    ));
    assert_eq!(status.status, 200, "status 一律 200");
    assert_eq!(status.body["status"], json!("approved"));
    assert_eq!(
        status.body["device"]["deviceId"],
        json!(device.device_id()),
        "已批准响应回落的设备标识必须是刚确认的那一个"
    );

    // ⑥ WSS：携带同一 Origin 完成四消息认证（设备证明的密钥与 claim 的 devicePublicKey 同源）。
    let authenticated = support::block_on(authenticate_with_host(
        addr,
        retrieved.body["hostId"].as_str().expect("hostId"),
        &device,
        &nonce_text(0x73),
    ));
    assert_eq!(
        authenticated["type"],
        json!("auth.authenticated"),
        "认证必须成功"
    );
    assert_eq!(
        authenticated["body"]["deviceId"],
        json!(device.device_id()),
        "认证响应必须属于刚批准的那台设备"
    );

    // ⑦ 证据不含凭据：先停止 Daemon（关闭日志文件、刷出全部行），再核对日志与进程输出都
    //    不得出现完整 qrToken 或 pairingSecret。
    assert!(daemon.stop().success(), "显式停止必须正常退出");
    let token_text = base64url(&token);
    let secret_text = base64url(&retrieved.secret);
    for (label, text) in [
        ("daemon JSON 日志", daemon.log()),
        ("daemon stdout", daemon.stdout()),
        ("daemon stderr", daemon.stderr()),
    ] {
        assert_absent(&text, &token_text, &format!("{label}不得打印完整 qrToken"));
        assert_absent(
            &text,
            &secret_text,
            &format!("{label}不得打印完整 pairingSecret"),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 负向：丢包重试、竞争、错误指纹/来源/绑定、幂等与冲突
// ---------------------------------------------------------------------------------------------

/// 响应丢失后重试同一凭据：两次取回必须逐字段相同，且第二份资料仍可认领。
#[test]
fn a_lost_qr_lookup_response_is_retried_with_identical_material() {
    let (mut daemon, addr) = start_daemon("device-qr-retry");
    let started = begin(&daemon);
    let pairing_id = started["pairingId"].as_str().expect("pairingId").to_owned();
    let (token, fingerprint, qr_origin) =
        decode_device_qr(started["pairingUrl"].as_str().expect("pairingUrl"));

    let first = support::block_on(qr_lookup(addr, &token, &qr_origin));
    let retried = support::block_on(qr_lookup(addr, &token, &qr_origin));
    let (first, retried) = (
        verify_material(&first, &fingerprint),
        verify_material(&retried, &fingerprint),
    );
    assert!(
        first.body == retried.body,
        "重试必须返回逐字段完全相同的七字段资料（含配对秘密与过期时间）"
    );
    assert_eq!(
        pairing_state(&daemon, &pairing_id),
        "pending",
        "重复取回不推进配对状态"
    );

    // 第二份资料仍然可认领：重试没有消耗秘密，也没有创建第二条配对。
    let device = DeviceKey::new(&uuid_text());
    let client_nonce = nonce_text(0x74);
    let claimed = support::block_on(claim(
        addr,
        &claim_body(
            &retried.body,
            &retried.secret,
            &device,
            &client_nonce,
            DEVICE_NAME,
        ),
    ));
    assert_eq!(claimed.status, 201, "重试之后的资料仍必须可认领");
    assert_eq!(pairing_state(&daemon, &pairing_id), "claimed");
    assert_eq!(
        listed_devices(&daemon).len(),
        0,
        "认领本身不创建设备信任记录"
    );

    assert!(daemon.stop().success());
}

/// 取回/认领的先后：认领生效后开始的取回统一 401；重复提交**相同** claim 幂等返回首次结果。
#[test]
fn claim_closes_retrieval_and_an_identical_repeat_returns_the_first_result() {
    let (mut daemon, addr) = start_daemon("device-qr-idempotent");
    let started = begin(&daemon);
    let pairing_id = started["pairingId"].as_str().expect("pairingId").to_owned();
    let (token, fingerprint, qr_origin) =
        decode_device_qr(started["pairingUrl"].as_str().expect("pairingUrl"));

    let retrieved = verify_material(
        &support::block_on(qr_lookup(addr, &token, &qr_origin)),
        &fingerprint,
    );
    let device = DeviceKey::new(&uuid_text());
    let client_nonce = nonce_text(0x75);
    let body = claim_body(
        &retrieved.body,
        &retrieved.secret,
        &device,
        &client_nonce,
        DEVICE_NAME,
    );
    let first = support::block_on(claim(addr, &body));
    assert_eq!(first.status, 201);
    let first_request = first.body["pairingRequestId"].clone();

    // 完全相同的重复 claim：返回首次结果（同一 pairingRequestId / serverNonce），不产生第二条记录。
    let repeated = support::block_on(claim(addr, &body));
    assert_eq!(repeated.status, 201, "相同内容的重试必须幂等成功");
    assert_eq!(
        repeated.body["pairingRequestId"], first_request,
        "幂等重试必须返回首次的配对请求标识"
    );
    assert_eq!(
        repeated.body["serverNonce"], first.body["serverNonce"],
        "幂等重试必须返回首次的服务端随机数"
    );
    assert_eq!(pairing_state(&daemon, &pairing_id), "claimed");

    // 认领生效之后开始的取回：即便内存仍保留秘密，也必须统一失败。
    let after = support::block_on(qr_lookup(addr, &token, &qr_origin));
    after.assert_qr_invalid();

    // 本机确认后只有一条设备记录（幂等重试没有创建第二条配对请求）。
    daemon.ok_value(
        server::local_admin::Method::DevicePairConfirm,
        params_of(json!({ "pairingId": pairing_id })),
    );
    let devices = listed_devices(&daemon);
    assert_eq!(devices.len(), 1, "一次配对只创建一条设备记录");
    assert_eq!(devices[0]["deviceId"], json!(device.device_id()));
    assert_eq!(devices[0]["state"], json!("active"));

    assert!(daemon.stop().success());
}

/// 不同内容的 claim（另一台设备身份）必须冲突，且不改变首次认领的设备与配对状态。
#[test]
fn a_claim_with_different_content_conflicts_without_mutating_the_pairing() {
    let (mut daemon, addr) = start_daemon("device-qr-conflict");
    let (device, retrieved, claimed) = pair_once(&daemon, addr, "conflict");
    let pairing_id = retrieved.body["pairingId"]
        .as_str()
        .expect("pairingId")
        .to_owned();
    let first_request = claimed["pairingRequestId"].clone();

    // 第二台“设备”：另一把密钥、另一个 deviceId 与 nonce，但用同一配对秘密正确签名。
    // 内容不同 → 冲突；不是证明失败（响应必须是 409，而不是 401）。
    let intruder = DeviceKey::new(&uuid_text());
    let intruder_nonce = nonce_text(0x76);
    let conflicting = support::block_on(claim(
        addr,
        &claim_body(
            &retrieved.body,
            &retrieved.secret,
            &intruder,
            &intruder_nonce,
            "Another Phone",
        ),
    ));
    assert_eq!(conflicting.status, 409, "不同内容的 claim 必须是 409");
    assert_eq!(
        conflicting.body["code"],
        json!("pairing.already_claimed"),
        "冲突必须使用 pairing.already_claimed"
    );

    // 原有认领不受影响：设备标识、状态与首次请求标识都不变。
    assert_eq!(pairing_state(&daemon, &pairing_id), "claimed");
    let view = daemon.ok_value(
        server::local_admin::Method::DevicePairStatus,
        params_of(json!({ "pairingId": pairing_id })),
    );
    assert_eq!(view["deviceId"], json!(device.device_id()));
    assert_eq!(view["state"], json!("claimed"));

    // 原设备仍能按首次结果完成本机确认与 status（冲突没有挤掉它）。
    daemon.ok_value(
        server::local_admin::Method::DevicePairConfirm,
        params_of(json!({ "pairingId": pairing_id })),
    );
    let status = support::block_on(poll_status(
        addr,
        &retrieved.body,
        &retrieved.secret,
        &device,
        first_request.as_str().expect("pairingRequestId"),
        &nonce_text(0x77),
    ));
    assert_eq!(status.body["status"], json!("approved"));
    assert_eq!(status.body["device"]["deviceId"], json!(device.device_id()));

    assert!(daemon.stop().success());
}

/// 错误指纹 / 错误来源 / 错误绑定：消费方在 claim 之前停止；`device.pair.status` 仍是
/// `pending`，并且随后用正确资料认领**成功**（claim 只成功一次，证明之前没有发过 claim）。
#[test]
fn wrong_fingerprint_origin_and_binding_stop_before_claim() {
    let (mut daemon, addr) = start_daemon("device-qr-guards");
    let started = begin(&daemon);
    let pairing_id = started["pairingId"].as_str().expect("pairingId").to_owned();
    let (token, fingerprint, qr_origin) =
        decode_device_qr(started["pairingUrl"].as_str().expect("pairingUrl"));

    // ① 错误指纹：地址里的指纹被篡改一个字节。取回本身成功，但消费方复算后必须停止，
    //    不把任何资料交给 claim。
    let mut tampered = fingerprint;
    tampered[31] ^= 0x01;
    let replied = support::block_on(qr_lookup(addr, &token, &qr_origin));
    assert_eq!(replied.status, 200, "取回不受二维码指纹影响");
    let public_key = base64::Engine::decode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        replied.body["hostPublicKey"]
            .as_str()
            .expect("hostPublicKey"),
    )
    .expect("公钥 base64url");
    use sha2::{Digest as _, Sha256};
    let observed: [u8; 32] = Sha256::digest(&public_key).into();
    assert_ne!(observed, tampered, "被篡改的指纹必须与真实公钥指纹不同");
    assert_eq!(
        pairing_state(&daemon, &pairing_id),
        "pending",
        "指纹不符时不得发送 claim"
    );

    // ② 错误来源：二维码被换成另一个 origin，请求显式带该 Origin → 统一 401，不交付任何材料。
    let other_origin = "https://other-host.example.invalid";
    let foreign_origin_reply = support::block_on(qr_lookup(addr, &token, other_origin));
    foreign_origin_reply.assert_qr_invalid();
    foreign_origin_reply.assert_security_headers();
    assert_eq!(
        pairing_state(&daemon, &pairing_id),
        "pending",
        "来源不符时不得发送 claim"
    );

    // ③ 错误绑定：另一台真实主机的凭据（另一 Daemon 生成）在这台主机上不可取回。
    let (mut foreign_daemon, _foreign_addr) = start_daemon("device-qr-foreign");
    let foreign_started = begin(&foreign_daemon);
    let (foreign_token, _foreign_fingerprint, _) =
        decode_device_qr(foreign_started["pairingUrl"].as_str().expect("pairingUrl"));
    let foreign_binding = support::block_on(qr_lookup(addr, &foreign_token, &qr_origin));
    foreign_binding.assert_qr_invalid();
    assert_eq!(
        pairing_state(&daemon, &pairing_id),
        "pending",
        "另一台主机的凭据不得影响本配对，也不得触发 claim"
    );
    // 对方主机自己的配对仍完好（本用例没有对它发过任何 claim）。
    assert_eq!(
        pairing_state(
            &foreign_daemon,
            foreign_started["pairingId"].as_str().expect("pairingId")
        ),
        "pending",
        "外来凭据在本机被拒不应消耗原主机的配对"
    );

    // ④ 正确资料仍然可用：此前三条失败路径都没有发送过 claim。
    let retrieved = verify_material(
        &support::block_on(qr_lookup(addr, &token, &qr_origin)),
        &fingerprint,
    );
    let device = DeviceKey::new(&uuid_text());
    let claimed = support::block_on(claim(
        addr,
        &claim_body(
            &retrieved.body,
            &retrieved.secret,
            &device,
            &nonce_text(0x78),
            DEVICE_NAME,
        ),
    ));
    assert_eq!(claimed.status, 201, "正确的指纹/来源/绑定必须仍可认领");
    assert_eq!(pairing_state(&daemon, &pairing_id), "claimed");

    assert!(
        listed_devices(&foreign_daemon).is_empty(),
        "外来 Daemon 没有创建任何设备信任"
    );
    assert!(foreign_daemon.stop().success());
    assert!(daemon.stop().success());
}
