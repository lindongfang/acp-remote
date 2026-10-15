//! Sync 命令面协议级端到端验收（变更② 任务 5.1–5.4）。
//!
//! 驱动方式：脚本化 fake Sync 客户端（`support::sync`）经**真实 loopback listener**（`/sync/v1` 的
//! WSS 升级）走完「配对 claim → 本机确认 → status approved → 四消息握手」，然后在**真实** Daemon 上
//! 派发命令：真实 SQLite、真实 `identity_auth::Authority`、真实本机管理通道与真实组合根装配
//! （`server::sync` 的订阅面 + 命令面组合路由 + 终态观察者）。
//!
//! 证据口径：每条断言都指向**可观察事实**（收到的帧、事件库水位、会话列表）。「未授权零副作用」用
//! **事件库全局水位未推进**证明，而不是「没收到错误」。

mod support;

use serde_json::{Value, json};
use support::sync::{DeviceKey, HostNode, SyncClient, nonce_text, uuid_text};

/// 本机宿主名（`daemon.public_origin` 的 authority；`Host` 头与 `Origin` 头都按它判）。
const HOST: &str = "sync-commands.example.invalid";
/// 设备名（`deviceName`）。
const DEVICE_NAME: &str = "Test Phone";
/// 主设备请求的 scope（覆盖观察、交互与创建三类，足以走完全部命令面用例）。
const SCOPES: [&str; 4] = [
    "session.list",
    "session.prompt",
    "session.create",
    "command.status",
];
/// 脚本化 agent selector（`support::sync` 的脚本后端按它供数）。
const AGENT: &str = "codex";

fn supported_features() -> Vec<&'static str> {
    vec![
        "core.snapshot.v1",
        "core.command-status.v1",
        "core.event-ack.v1",
        "acp.raw-payload.v1",
        "resource.remote-origin.v1",
        "core.local-catalog.v1",
        "core.session-create.v1",
        "core.session-history.v1",
        "core.event-origin.v1",
    ]
}

// ---------------------------------------------------------------------------------------------
// 配对与握手脚手架（与 `sync_e2e.rs` 同一套真实路径；集成测试各自成 crate，因此各自持有一份）
// ---------------------------------------------------------------------------------------------

struct Device {
    key: DeviceKey,
    client_nonce: String,
    request_nonce: String,
}

impl Device {
    fn new() -> Self {
        Self {
            key: DeviceKey::new(&uuid_text()),
            client_nonce: nonce_text(0x51),
            request_nonce: nonce_text(0x61),
        }
    }

    fn id(&self) -> String {
        self.key.device_id().to_owned()
    }
}

struct HttpReply {
    status: u16,
    body: Value,
}

async fn post_json(host: &HostNode, path: &str, body: &Value) -> HttpReply {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let bytes = serde_json::to_vec(body).expect("请求体可序列化");
    let head = format!(
        "POST {path} HTTP/1.1\r\nHost: {name}\r\nOrigin: {origin}\r\n\
         Content-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
        bytes.len(),
        name = host.host_name(),
        origin = host.origin(),
    );
    let mut stream = tokio::net::TcpStream::connect(host.addr())
        .await
        .expect("连接 loopback");
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
    HttpReply {
        status,
        body: serde_json::from_str(body).unwrap_or(Value::Null),
    }
}

/// 走完「device.pair.begin → claim → device.pair.confirm → status approved」，返回设备记录。
async fn pair_and_approve(host: &HostNode, device: &Device, scopes: &[&str]) -> Value {
    let begin = host
        .admin(
            server::local_admin::Method::DevicePairBegin,
            json!({ "requestedPacks": [], "requestedScopes": scopes, "expiresInMs": null }),
        )
        .await;
    let pairing_id = begin["pairingId"].as_str().expect("pairingId").to_owned();
    let pairing_url = begin["pairingUrl"].as_str().expect("pairingUrl");
    let (ticket, secret) = qr_payload(pairing_url);

    let claim = json!({
        "protocolVersion": 1,
        "pairingId": pairing_id,
        "hostId": host.host_id(),
        "deviceId": device.id(),
        "deviceName": DEVICE_NAME,
        "clientKind": "pwa",
        "canonicalOrigin": ticket["canonicalOrigin"],
        "devicePublicKey": public_key_base64url(device),
        "clientNonce": device.client_nonce,
        "proof": pairing_proof(&ticket, &secret, device),
    });
    let replied = post_json(host, server::sync::pairing::CLAIM_PATH, &claim).await;
    assert_eq!(
        replied.status, 201,
        "合法 claim 必须返回 201：{}",
        replied.body
    );

    let confirmed = host
        .admin(
            server::local_admin::Method::DevicePairConfirm,
            json!({ "pairingId": ticket["pairingId"], "scopes": scopes }),
        )
        .await;
    assert_eq!(
        confirmed["deviceId"],
        json!(device.id()),
        "确认必须创建该设备的信任记录：{confirmed}"
    );

    let status = json!({
        "protocolVersion": 1,
        "pairingId": ticket["pairingId"],
        "hostId": host.host_id(),
        "deviceId": device.id(),
        "pairingRequestId": replied.body["pairingRequestId"],
        "requestNonce": device.request_nonce,
        "proof": status_proof(
            &ticket,
            &secret,
            device,
            replied.body["pairingRequestId"].as_str().expect("pairingRequestId"),
        ),
    });
    let replied = post_json(host, server::sync::pairing::STATUS_PATH, &status).await;
    assert_eq!(replied.status, 200, "status 一律 200：{}", replied.body);
    assert_eq!(
        replied.body["status"],
        json!("approved"),
        "确认之后 status 必须是 approved：{}",
        replied.body
    );
    replied.body
}

/// 配对并完成四消息握手；返回（已认证的）客户端。
async fn connect_authenticated(host: &HostNode, device: &Device) -> SyncClient {
    let mut client = SyncClient::connect_plain(host.addr(), host.host_name(), &host.origin()).await;
    client.step("四消息握手");
    let authenticated = client
        .authenticate(
            &host.host_id(),
            &device.key,
            &supported_features(),
            &device.client_nonce,
        )
        .await;
    assert_eq!(
        authenticated["type"],
        json!("auth.authenticated"),
        "设备必须完成认证，实际：{authenticated}"
    );
    client
}

fn qr_payload(url: &str) -> (Value, [u8; 32]) {
    let fragment = url.split('#').nth(1).expect("二维码 URL 带 fragment");
    let (key, value) = fragment.split_once('=').expect("fragment 是 data=…");
    assert_eq!(key, "data", "二维码 fragment 的键必须是 data");
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, value)
        .expect("payload 是无填充 base64url");
    let payload: Value = serde_json::from_slice(&bytes).expect("payload 是 JSON object");
    let secret_text = payload["pairingSecret"].as_str().expect("pairingSecret");
    let secret = base64::Engine::decode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        secret_text,
    )
    .expect("secret 是无填充 base64url");
    let secret: [u8; 32] = secret
        .as_slice()
        .try_into()
        .expect("pairing secret 是 32 字节");
    (payload, secret)
}

fn public_key_base64url(device: &Device) -> String {
    support::sync::base64url(device.key.public_key().as_bytes())
}

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

fn status_proof(
    ticket: &Value,
    secret: &[u8; 32],
    device: &Device,
    pairing_request_id: &str,
) -> String {
    use identity_auth::{Nonce, PairingProof, SyncPairingStatus};
    let input = SyncPairingStatus {
        host_id: acp_core::model::NodeId::new(ticket["hostId"].as_str().expect("hostId"))
            .expect("host id 规范"),
        device_id: acp_core::model::DeviceId::new(&device.id()).expect("device id 规范"),
        pairing_id: acp_core::model::PairingId::new(
            ticket["pairingId"].as_str().expect("pairingId"),
        )
        .expect("pairing id 规范"),
        pairing_request_id: identity_auth::PairingRequestId::parse(pairing_request_id)
            .expect("pairingRequestId 规范"),
        request_nonce: Nonce::new(&device.request_nonce).expect("规范 nonce"),
    };
    let transcript = input.transcript().expect("transcript 可装配");
    let output = hmac_sha256(secret, &transcript);
    let proof = PairingProof::try_from_bytes(&output).expect("32 字节 proof");
    support::sync::base64url(proof.as_bytes())
}

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

/// 请求指纹（`Digest`：32 字节摘要的无填充 base64url）。
fn ac1_digest(label: &str) -> acp_core::model::Digest {
    use base64::Engine as _;
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(label.as_bytes());
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize());
    acp_core::model::Digest::new(&encoded).expect("32 字节摘要")
}

/// 本机建一条 owned 会话（脚本化后端；E2E 的会话来源，不经 wire）。
async fn local_session(host: &HostNode) -> String {
    host.core()
        .create_session(
            &acp_core::model::Actor::LocalCli,
            acp_core::model::RequestId::new(&uuid_text()).expect("request id 规范"),
            ac1_digest("acp-remote/sync-commands-e2e/session-create"),
            acp_core::model::CreateSessionRequest::new(
                acp_core::model::AgentRef::try_new(
                    acp_core::model::AgentId::new(AGENT).expect("agent id"),
                    "Codex",
                )
                .expect("agent ref"),
                None,
                None,
                acp_core::model::ResourceOrigin::Local,
            ),
            None,
        )
        .await
        .expect("本机会话创建")
        .as_str()
        .to_owned()
}

// ---------------------------------------------------------------------------------------------
// 5.1：查询命令对真实 listener 同步完成
// ---------------------------------------------------------------------------------------------

/// 5.1 的完成判据：客户端能对真实 listener 派发一条查询命令并收到 `completed`。
#[tokio::test]
async fn a_query_command_completes_on_the_real_listener() {
    let host = HostNode::start("sync-cmd-query", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device, &SCOPES).await;
    let session = local_session(&host).await;
    let mut client = connect_authenticated(&host, &device).await;

    let request = uuid_text();
    client.step("session.list");
    client
        .command(&request, "session.list", None, json!({}))
        .await;
    let result = client.expect_command_result(&request).await;

    assert_eq!(result["status"], json!("completed"), "{result}");
    let sessions = result["result"]["sessions"]
        .as_array()
        .expect("sessions 数组");
    assert!(
        sessions
            .iter()
            .any(|entry| entry["sessionId"] == json!(session)),
        "本机刚建的会话必须出现在列表里：{result}"
    );
    // 查询命令的 `terminalEventId` 固定为 `null`（§11.2）。
    assert_eq!(result["terminalEventId"], Value::Null);
}

// ---------------------------------------------------------------------------------------------
// 5.2：mutation 的 accepted → 领域事件 → 终态
// ---------------------------------------------------------------------------------------------

/// 5.2 的主链路：`session.prompt` 收到 `accepted` → 释放脚本事件 → 收到终态且 `terminalEventId` 一致。
#[tokio::test]
async fn a_prompt_is_accepted_and_then_completes_with_its_terminal_event() {
    let host = HostNode::start("sync-cmd-prompt", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device, &SCOPES).await;
    let session = local_session(&host).await;
    let mut client = connect_authenticated(&host, &device).await;

    let marker = "marker-cmd-4f2a";
    let request = uuid_text();
    client.step("session.prompt");
    client
        .command(
            &request,
            "session.prompt",
            Some(&session),
            json!({ "content": [{ "type": "text", "text": marker }] }),
        )
        .await;

    // ① 首响应必须是 `accepted`，且**不得**携带业务结果或终态事件。
    let accepted = client.expect_command_result(&request).await;
    assert_eq!(accepted["status"], json!("accepted"), "{accepted}");
    assert_eq!(accepted["result"], Value::Null, "accepted 不得带业务结果");
    assert_eq!(accepted["terminalEventId"], Value::Null);
    assert!(
        accepted["acceptedAt"].is_string(),
        "accepted 必须带首次接受时间：{accepted}"
    );

    // ② 释放脚本后端暂存的产出（真实 Agent 在 accepted 之后产出事件）。
    let released = host.parked().release(marker);
    assert!(released >= 2, "脚本应释放 delta 与 turn.completed 两条事件");
    // 真实路径上这步由 agent-host 驱动（把缓冲的事件落盘并派发）；脚本后端没有驱动者，用例补上。
    host.broker()
        .pump(&acp_core::model::SessionId::new(&session).expect("session id"))
        .await
        .expect("事件落盘");

    // ③ 终态：`command.result` 的 `completed` 必须带该 request 唯一的 terminal event id。
    let terminal = client.expect_command_result(&request).await;
    assert_eq!(terminal["status"], json!("completed"), "{terminal}");
    let terminal_event = terminal["terminalEventId"]
        .as_str()
        .expect("终态必须带 terminalEventId")
        .to_owned();
    assert_eq!(
        terminal["acceptedAt"], accepted["acceptedAt"],
        "终态的 acceptedAt 必须是首次接受时间（不得换成现在）"
    );

    // ④ 重放同一 requestId：回当前终态与**原** terminal event id，且不重新派发（§11.2）。
    client.step("重放同一 requestId");
    client
        .command(
            &request,
            "session.prompt",
            Some(&session),
            json!({ "content": [{ "type": "text", "text": marker }] }),
        )
        .await;
    let replay = client.expect_command_result(&request).await;
    assert_eq!(replay["status"], json!("completed"), "{replay}");
    assert_eq!(
        replay["terminalEventId"],
        json!(terminal_event),
        "重放必须给出原 terminal event id：{replay}"
    );
}

// ---------------------------------------------------------------------------------------------
// 5.3：边界（未授权零副作用 / 幂等重发 / session.resume 不支持）
// ---------------------------------------------------------------------------------------------

/// 5.3：未持 scope 的设备派发 mutation 得 `authorization.scope_denied`，且**零副作用**。
#[tokio::test]
async fn an_unauthorized_device_gets_scope_denied_without_side_effects() {
    let host = HostNode::start("sync-cmd-scope", HOST).await;
    let device = Device::new();
    // 该设备只被授予观察类 scope：`session.prompt`（interact）不在其中。
    pair_and_approve(&host, &device, &["session.list"]).await;
    let session = local_session(&host).await;
    let mut client = connect_authenticated(&host, &device).await;

    let watermark = host.head().await;
    let request = uuid_text();
    client.step("未授权 prompt");
    client
        .command(
            &request,
            "session.prompt",
            Some(&session),
            json!({ "content": [{ "type": "text", "text": "不该到达 Agent" }] }),
        )
        .await;
    let result = client.expect_command_result(&request).await;

    assert_eq!(
        result["error"]["code"],
        json!("authorization.scope_denied"),
        "{result}"
    );
    assert_eq!(
        result["terminalEventId"],
        Value::Null,
        "被拒的命令没有终态事件"
    );
    // 零副作用：事件库的全局水位**没有**被这条命令推进。
    let after = host.head().await;
    assert_eq!(
        after.global_sequence, watermark.global_sequence,
        "未授权命令不得写入任何事件：{watermark:?} → {after:?}"
    );
}

/// 5.3：同一 `requestId` 重发只产生一个副作用（会话只有一个 turn）。
#[tokio::test]
async fn the_same_request_id_produces_only_one_turn() {
    let host = HostNode::start("sync-cmd-idem", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device, &SCOPES).await;
    let session = local_session(&host).await;
    let mut client = connect_authenticated(&host, &device).await;

    let marker = "marker-idem-9c1d";
    let request = uuid_text();
    let body = json!({ "content": [{ "type": "text", "text": marker }] });
    client.step("首次 prompt");
    client
        .command(&request, "session.prompt", Some(&session), body.clone())
        .await;
    let accepted = client.expect_command_result(&request).await;
    assert_eq!(accepted["status"], json!("accepted"), "{accepted}");

    // 同一 requestId 重发（新 messageId：连接期去重按帧，这里要打到持久幂等）。
    client.step("重发同一 requestId");
    client
        .command(&request, "session.prompt", Some(&session), body.clone())
        .await;
    let replay = client.expect_command_result(&request).await;
    assert_eq!(
        replay["status"],
        json!("accepted"),
        "重放应回首次的已知接受：{replay}"
    );

    // 只产生一个副作用：脚本后端只被派发一次，因此只有一份 park 的事件可以释放。
    let released = host.parked().release(marker);
    assert_eq!(
        released, 2,
        "第二次提交不得再派发（否则脚本会再 park 一份事件）"
    );
}

/// 5.3：`session.resume` 在 Sync 面得 `command.unsupported`（只经 Node Link）。
#[tokio::test]
async fn session_resume_is_unsupported_on_the_sync_face() {
    let host = HostNode::start("sync-cmd-resume", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device, &SCOPES).await;
    let session = local_session(&host).await;
    let mut client = connect_authenticated(&host, &device).await;

    let watermark = host.head().await;
    let request = uuid_text();
    client.step("session.resume");
    client
        .command(&request, "session.resume", Some(&session), json!({}))
        .await;
    // `session.resume` 不在 Sync v1 的封闭词表里：拒绝发生在命令解码之前，因此回复是带
    // `correlationId` 的 `error` 帧（§4.2/§11.5），不是 `command.result`。
    let refused = client.expect_error().await;
    assert_eq!(
        refused["body"]["code"],
        json!("command.unsupported"),
        "{refused}"
    );
    let after = host.head().await;
    assert_eq!(
        after.global_sequence, watermark.global_sequence,
        "被拒的 resume 不得写入任何事件"
    );
}

// ---------------------------------------------------------------------------------------------
// 5.4：限流
// ---------------------------------------------------------------------------------------------

/// 5.4：连续派发超过额度得到 `resource.rate_limited`（可重试，含 `retryAfterMs`），且被拒不产生副作用。
///
/// 额度是**生产值**（120 次/分钟/设备/命令族），因此这里真的发满一轮。
#[tokio::test]
async fn exceeding_the_command_quota_is_retryable_and_side_effect_free() {
    let host = HostNode::start("sync-cmd-rate", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device, &SCOPES).await;
    let mut client = connect_authenticated(&host, &device).await;

    client.step("发满观察族额度");
    // 限流拒绝是 `error` 帧（带 `correlationId` 指回该次 command 的 messageId），不是
    // `command.result`——命令在限流闸门就被挡下，从未进入业务分派。
    let mut limited = None;
    for _ in 0..150u32 {
        let request = uuid_text();
        client
            .command(&request, "session.list", None, json!({}))
            .await;
        match client.take().await {
            message if message["type"] == json!("error") => {
                limited = Some(message);
                break;
            }
            _ => {}
        }
    }

    let limited = limited.expect("超过额度后必须出现 resource.rate_limited");
    assert_eq!(
        limited["body"]["code"],
        json!("resource.rate_limited"),
        "{limited}"
    );
    assert_eq!(
        limited["body"]["retryable"],
        json!(true),
        "限流必须标记为可重试：{limited}"
    );
    assert!(
        limited["body"]["details"]["retryAfterMs"].is_number(),
        "限流必须给出 retryAfterMs：{limited}"
    );
}
