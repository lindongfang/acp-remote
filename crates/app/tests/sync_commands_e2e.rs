//! Sync 命令面协议级端到端验收（变更② 任务 5.1–5.4；`pairing-model` 的 8.2/8.3）。
//!
//! 驱动方式：脚本化 fake Sync 客户端（`support::sync`）经**真实 loopback listener**（`/sync/v1` 的
//! WSS 升级）走完「配对 claim → 本机确认 → status approved → 四消息握手」，然后在**真实** Daemon 上
//! 派发命令：真实 SQLite、真实 `identity_auth::Authority`、真实本机管理通道与真实组合根装配
//! （`server::sync` 的订阅面 + 命令面组合路由 + 终态观察者）。
//!
//! 证据口径：每条断言都指向**可观察事实**（收到的帧、错误码、事件库水位、会话列表、直接读库的落库列）。
//! 「被拒的命令零副作用」用**不写会话行**与**事件库水位未推进**证明，而不是「没收到错误」。

mod support;

use serde_json::{Value, json};
use support::sync::{DeviceKey, HostNode, SyncClient, nonce_text, uuid_text};

/// 本机宿主名（`daemon.public_origin` 的 authority；`Host` 头与 `Origin` 头都按它判）。
const HOST: &str = "sync-commands.example.invalid";
/// 设备名（`deviceName`）。
const DEVICE_NAME: &str = "Test Phone";
/// 已删掉的授权集合与短验证码词汇（`pairing-model` 删除设备权限层）：设备配对不提交也不分配任何
/// 授权集合，这组键出现在配对状态或 `auth.authenticated` 的任何层级都让对应用例变红。
const REMOVED_AUTHORIZATION_KEYS: [&str; 5] = ["sas", "scopes", "grants", "packs", "presets"];
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

/// 走完「device.pair.begin → claim → device.pair.confirm → status approved」，返回已批准的状态响应。
///
/// 设备配对不授予任何授权（`pairing-model` 删除设备权限层），因此本脚手架**没有** scope/grant 参数：
/// `device.pair.begin` 只收 `expiresInMs`、`confirm` 只收 `pairingId`。
async fn pair_and_approve(host: &HostNode, device: &Device) -> Value {
    let begin = host
        .admin(
            server::local_admin::Method::DevicePairBegin,
            json!({ "expiresInMs": null }),
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
            json!({ "pairingId": pairing_id.as_str() }),
        )
        .await;
    assert_eq!(
        confirmed["deviceId"],
        json!(device.id()),
        "确认必须创建该设备的信任记录：{confirmed}"
    );

    let status = json!({
        "protocolVersion": 1,
        "pairingId": pairing_id.as_str(),
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
    // 已批准响应只承载协议事实与两侧的非秘密元数据（`pairingRequestId`/`status`/`expiresAt`/
    // `device`/`host`）：设备配对不再派生短验证码、也不再分配任何授权集合。
    for removed in REMOVED_AUTHORIZATION_KEYS {
        assert!(
            !support::sync::contains_key_deep(&replied.body, removed),
            "`{removed}` 不得出现在配对状态响应里：{}",
            replied.body
        );
    }
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
    pair_and_approve(&host, &device).await;
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
    pair_and_approve(&host, &device).await;
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
// 5.3：边界（无授权集合照常受理 / 幂等重发 / session.resume 不支持）
// ---------------------------------------------------------------------------------------------

/// 5.3（`pairing-model` 改写）：设备方向不再有命令级授权判定——**没有任何授权集合**的已认证设备
/// 派发 mutation 照常被受理，并产生它应有的副作用（Agent 后端真的被派发一次）。
///
/// 变红点：实现若仍按授权（例如拿空 scope 集合）判拒，第一条回复会是 `rejected`
/// （`authorization.scope_denied`）而不是 `accepted`；实现若受理却不下发到后端，`release` 会返回 0。
#[tokio::test]
async fn an_authenticated_device_without_any_authorization_set_is_still_accepted() {
    let host = HostNode::start("sync-cmd-no-scope", HOST).await;
    let device = Device::new();
    // 配对不给设备分配任何授权集合（`pairing-model` 删除设备权限层）：参数面里根本没有可提交
    // scope/grant 的位置，因此这台设备**只能**是「零授权集合」的。
    pair_and_approve(&host, &device).await;
    let session = local_session(&host).await;

    let mut client = SyncClient::connect_plain(host.addr(), host.host_name(), &host.origin()).await;
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
        "{authenticated}"
    );
    // 认证事实里没有任何授权集合：设备方向没有可用来判拒的维度。
    for absent in REMOVED_AUTHORIZATION_KEYS {
        assert!(
            !support::sync::contains_key_deep(&authenticated["body"], absent),
            "`{absent}` 不得出现在 auth.authenticated 里：{authenticated}"
        );
    }

    let marker = "marker-no-scope-7b31";
    let request = uuid_text();
    client.step("无授权集合的设备派发 prompt");
    client
        .command(
            &request,
            "session.prompt",
            Some(&session),
            json!({ "content": [{ "type": "text", "text": marker }] }),
        )
        .await;

    // ① 受理：首次回复是 `accepted`，而不是 `authorization.scope_denied` 的 `rejected`。
    let accepted = client.expect_command_result(&request).await;
    assert_eq!(
        accepted["status"],
        json!("accepted"),
        "已认证设备必须被受理：{accepted}"
    );
    assert_eq!(accepted["result"], Value::Null, "accepted 不得带业务结果");
    assert_eq!(accepted["terminalEventId"], Value::Null);

    // ② 副作用真的发生了：脚本后端被派发一次，因此恰好有一份事件可以释放（被拒的命令不会有）。
    let released = host.parked().release(marker);
    assert_eq!(
        released, 2,
        "命令必须真的到达 Agent 后端（否则等于零副作用）：{released}"
    );
    host.broker()
        .pump(&acp_core::model::SessionId::new(&session).expect("session id"))
        .await
        .expect("事件落盘");

    // ③ 终态照常到达，且带该 request 的唯一 terminal event id。
    let terminal = client.expect_command_result(&request).await;
    assert_eq!(terminal["status"], json!("completed"), "{terminal}");
    assert!(
        terminal["terminalEventId"].is_string(),
        "终态必须带 terminalEventId：{terminal}"
    );

    host.stop().await;
}

/// 5.3：同一 `requestId` 重发只产生一个副作用（会话只有一个 turn）。
#[tokio::test]
async fn the_same_request_id_produces_only_one_turn() {
    let host = HostNode::start("sync-cmd-idem", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
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
    pair_and_approve(&host, &device).await;
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
    pair_and_approve(&host, &device).await;
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

// ---------------------------------------------------------------------------------------------
// 8.3：`session.create` 的目录来源分支（Sync 面允许客户端提供目录，`pairing-model`）
// ---------------------------------------------------------------------------------------------

/// 本机登记一个 workspace alias（`session.create` 的别名来源；真实本地管理通道）。
async fn register_workspace(host: &HostNode, alias: &str) {
    host.admin(
        server::local_admin::Method::WorkspaceSelect,
        json!({
            "alias": alias,
            "displayName": "Project One",
            "rootPath": host.data_dir().display().to_string(),
        }),
    )
    .await;
}

/// 打开同一 data dir 的原始 SQL 连接（只读复核用；与 `HostNode::drop_earliest_events` 同一手法）。
async fn raw_connection(host: &HostNode) -> sqlx::SqliteConnection {
    let config = storage_sqlite::migrate::StorageConfig::new(host.data_dir());
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(config.database_path())
        .create_if_missing(false)
        .busy_timeout(std::time::Duration::from_secs(10));
    sqlx::Connection::connect_with(&options)
        .await
        .expect("打开同一 data_dir 的原始 SQL 连接")
}

/// 直接读库：该会话的 `(agent_session_id, workspace_alias)`。外层 `None` = 没有这一行。
///
/// 目录归属的**权威来源只有** `owned_session.workspace_alias`（`storage-sqlite` 的
/// `workspace_from_row` 也按它投影摘要），因此「来源是路径时它是 `NULL`」在这里是落库事实，不经过
/// 任何投影推断。
async fn stored_session_columns(
    host: &HostNode,
    session_id: &str,
) -> Option<(Option<String>, Option<String>)> {
    let mut connection = raw_connection(host).await;
    let row: Option<(Option<String>, Option<String>)> =
        sqlx::query_as::<_, (Option<String>, Option<String>)>(
            "SELECT agent_session_id, workspace_alias FROM owned_session WHERE session_id = ?1",
        )
        .bind(session_id)
        .fetch_optional(&mut connection)
        .await
        .expect("读 owned_session 的两列");
    drop(connection);
    row
}

/// 等到该会话的「恢复列 + 目录归属」那一笔提交落盘再返回：三者由**同一条** `UPDATE` 写入
/// （`agent_session_id`/`workspace_cwd`/`workspace_alias`），因此 `agent_session_id` 非 `NULL` 就是
/// 「目录归属已经写定」的判据——读在它之前会让断言不到东西（也可能放过写错值的实现）。
async fn await_session_columns(
    host: &HostNode,
    session_id: &str,
) -> (Option<String>, Option<String>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        if let Some(columns) = stored_session_columns(host, session_id).await {
            if columns.0.is_some() {
                return columns;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "超时：会话 {session_id} 的「恢复列 + 目录归属」那一笔提交未落盘（该行不存在或 \
             `agent_session_id` 仍为 `NULL`）"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// 直接读库：`owned_session` 的行数（「被拒的 `session.create` 不创建会话」的证据）。
async fn session_row_count(host: &HostNode) -> i64 {
    let mut connection = raw_connection(host).await;
    let count: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM owned_session")
        .fetch_one(&mut connection)
        .await
        .expect("统计 owned_session");
    drop(connection);
    count
}

/// 派发一条必然被拒的 `session.create`（`sessionId` 禁止：会话尚不存在），返回被拒的终态。
///
/// 三类断言一起做：`status = rejected`、给定的错误码、**不写会话行**（零副作用）。
async fn rejected_session_create(
    host: &HostNode,
    client: &mut SyncClient,
    payload: Value,
    code: &str,
    why: &str,
) -> Value {
    let before = session_row_count(host).await;
    let request = uuid_text();
    client.step(why);
    client
        .command(&request, "session.create", None, payload)
        .await;
    let rejected = client.expect_command_result(&request).await;
    assert_eq!(rejected["status"], json!("rejected"), "{why}：{rejected}");
    assert_eq!(rejected["error"]["code"], json!(code), "{why}：{rejected}");
    assert_eq!(
        rejected["terminalEventId"],
        Value::Null,
        "{why}：被拒的命令没有终态事件：{rejected}"
    );
    assert_eq!(
        session_row_count(host).await,
        before,
        "{why}：被拒的 session.create 不得创建会话行"
    );
    rejected
}

/// 8.3：`workspacePath` 成功路径——会话建成，且**来源是路径**时落库的 `workspace_alias` 是 `NULL`。
///
/// 变红点：实现若把客户端目录也当别名来源（例如拿路径末段或规范化结果造一个别名）或写成空串占位，
/// 那条 `UPDATE` 会把非 `NULL` 写进 `workspace_alias`，直接读库的断言与摘要里的未分组断言同时变红。
#[tokio::test]
async fn session_create_with_a_client_supplied_path_is_created_without_a_workspace_alias() {
    let host = HostNode::start("sync-create-path", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect_authenticated(&host, &device).await;

    let directory = host.data_dir().display().to_string();
    let request = uuid_text();
    client.step("session.create（客户端提供的目录）");
    client
        .command(
            &request,
            "session.create",
            None,
            json!({ "agentId": AGENT, "workspacePath": directory }),
        )
        .await;

    // ① 先 `accepted`（创建与幂等行已落盘），再 `completed` 带新会话的摘要。
    let accepted = client.expect_command_result(&request).await;
    assert_eq!(accepted["status"], json!("accepted"), "{accepted}");
    assert_eq!(accepted["result"], Value::Null, "accepted 不得带业务结果");
    let terminal = client.expect_command_result(&request).await;
    assert_eq!(terminal["status"], json!("completed"), "{terminal}");
    let session = terminal["result"]["sessionId"]
        .as_str()
        .expect("completed 必须给出新会话标识")
        .to_owned();

    // ② 读路径：协商了 `core.local-catalog.v1`，摘要里 `workspace` 键必须在、值为 `null`（未分组）。
    let summary = terminal["result"]["session"]
        .as_object()
        .expect("completed 必须携带会话摘要");
    assert!(
        summary.contains_key("workspace"),
        "协商了目录 feature 时 `workspace` 键必须存在（未分组是 `null`，不是缺席）：{terminal}"
    );
    assert_eq!(
        summary["workspace"],
        Value::Null,
        "客户端提供的目录不产生别名，摘要必须是未分组：{terminal}"
    );

    // ③ 落库事实：`workspace_alias` 是 SQL `NULL`（`agent_session_id` 非空 = 那笔提交确实跑了）。
    let (agent_session_id, workspace_alias) = await_session_columns(&host, &session).await;
    assert!(
        agent_session_id.is_some(),
        "前提：目录归属那一笔提交已落盘（否则本用例断言不到任何东西）"
    );
    assert_eq!(
        workspace_alias, None,
        "来源是路径的会话必须以 SQL `NULL` 落库（不得用空串、路径末段或占位别名）"
    );

    host.stop().await;
}

/// 8.3：别名来源的对照——`workspaceAlias` 成功时该会话带着**别名原文**落库。
///
/// 这条挡的是「实现永远写 `NULL`」这种错误：那种实现能让「路径 → `NULL`」恒真，但会让本用例变红；
/// 反过来，本用例能过而「路径 → `NULL`」不过，说明目录来源真的被区分了。
#[tokio::test]
async fn session_create_with_a_registered_alias_records_that_alias() {
    let host = HostNode::start("sync-create-alias", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    register_workspace(&host, "project.one").await;
    let mut client = connect_authenticated(&host, &device).await;

    let request = uuid_text();
    client.step("session.create（本机已登记的别名）");
    client
        .command(
            &request,
            "session.create",
            None,
            json!({ "agentId": AGENT, "workspaceAlias": "project.one" }),
        )
        .await;
    let accepted = client.expect_command_result(&request).await;
    assert_eq!(accepted["status"], json!("accepted"), "{accepted}");
    let terminal = client.expect_command_result(&request).await;
    assert_eq!(terminal["status"], json!("completed"), "{terminal}");
    let session = terminal["result"]["sessionId"]
        .as_str()
        .expect("completed 必须给出新会话标识")
        .to_owned();
    assert_eq!(
        terminal["result"]["session"]["workspace"]["alias"],
        json!("project.one"),
        "别名来源必须投影出别名（不是未分组）：{terminal}"
    );

    let (agent_session_id, workspace_alias) = await_session_columns(&host, &session).await;
    assert!(
        agent_session_id.is_some(),
        "前提：目录归属那一笔提交已落盘（否则本用例断言不到任何东西）"
    );
    assert_eq!(
        workspace_alias,
        Some("project.one".to_owned()),
        "别名来源必须以别名原文落库（不得只写 cwd、也不得省略）"
    );

    host.stop().await;
}

/// 8.3：别名未登记 → **参数类**错误（`protocol.schema_invalid`），且不创建会话。
#[tokio::test]
async fn session_create_with_an_unregistered_alias_is_a_parameter_error() {
    let host = HostNode::start("sync-create-bad-alias", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect_authenticated(&host, &device).await;

    rejected_session_create(
        &host,
        &mut client,
        json!({ "agentId": AGENT, "workspaceAlias": "never.registered" }),
        "protocol.schema_invalid",
        "未登记别名",
    )
    .await;

    host.stop().await;
}

/// 8.3：形状非法的目录是**参数类**错误，且判定发生在文件系统访问**之前**。
///
/// 两条输入刻意选成「先查文件系统就会得到别的东西」的形状，因此错误的**分类**本身把判定顺序钉住：
/// - 相对路径且该目录不存在——先查文件系统会回 `internal.unavailable`（不存在），错误码会变红；
/// - 含 `..` 且该路径**存在**（指向数据目录的父目录）——先查文件系统会一路走通并真的创建会话，
///   于是错误码与「不创建会话行」两条同时变红。
#[tokio::test]
async fn session_create_with_a_malformed_workspace_path_is_a_parameter_error() {
    let host = HostNode::start("sync-create-shape", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect_authenticated(&host, &device).await;

    let parent_reference = format!("{}/..", host.data_dir().display());
    for (why, path) in [
        ("相对路径", "not-an-absolute-directory".to_owned()),
        ("含 `..`", parent_reference),
    ] {
        let rejected = rejected_session_create(
            &host,
            &mut client,
            json!({ "agentId": AGENT, "workspacePath": path.as_str() }),
            "protocol.schema_invalid",
            why,
        )
        .await;
        // 客户端自己提交的目录不得被回显为对端可见字段（`workspace-resolution` 的「路径不泄漏」）：
        // 明文与「作为 JSON 字符串出现」两种渲染都不得命中（Windows 路径含反斜杠，会被转义）。
        let rendered = rejected.to_string();
        let as_json_string = serde_json::to_string(&path).expect("路径可序列化");
        for form in [path.as_str(), as_json_string.as_str()] {
            assert!(
                !rendered.contains(form),
                "{why}：错误响应不得回显客户端提交的目录（{form}）：{rejected}"
            );
        }
    }

    host.stop().await;
}

/// 8.3：形式合法（绝对、无 `..`）但本机解析不了 → **服务端不可用类**错误（`internal.unavailable`）。
///
/// 与上一条构成分类的两侧：形状合法时必须真的去解析，因此「不存在的绝对路径」不能被报成形状错误；
/// 「存在但不是目录」也不能被当成目录接受。
#[tokio::test]
async fn session_create_with_an_unresolvable_workspace_path_is_unavailable() {
    let host = HostNode::start("sync-create-missing", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect_authenticated(&host, &device).await;

    for (why, path) in [
        (
            "目录不存在",
            host.data_dir()
                .join("not-created-subdirectory")
                .display()
                .to_string(),
        ),
        // 真实存在的**文件**（本机 `acp-remote` 可执行文件）：形式合法，但不是目录。
        ("不是目录", env!("CARGO_BIN_EXE_acp-remote").to_owned()),
    ] {
        rejected_session_create(
            &host,
            &mut client,
            json!({ "agentId": AGENT, "workspacePath": path }),
            "internal.unavailable",
            why,
        )
        .await;
    }

    host.stop().await;
}

/// 8.3：目录引用**二选一**——同时给出别名与客户端目录时必须在**解码**阶段被拒，不创建会话。
///
/// 给的别名是本机已登记的（否则这条会因「别名未登记」而通过，什么也没验证）：实现若漏掉 `oneOf`，
/// 它会照常创建会话，于是错误帧断言与「不创建会话行」两条同时变红。
#[tokio::test]
async fn session_create_rejects_both_directory_sources_at_once() {
    let host = HostNode::start("sync-create-both", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    register_workspace(&host, "project.one").await;
    let mut client = connect_authenticated(&host, &device).await;

    let before = session_row_count(&host).await;
    let request = uuid_text();
    client.step("同时给出别名与客户端目录");
    client
        .command(
            &request,
            "session.create",
            None,
            json!({
                "agentId": AGENT,
                "workspaceAlias": "project.one",
                "workspacePath": host.data_dir().display().to_string(),
            }),
        )
        .await;
    // payload 是 closed object 且目录来源是 schema 的 `oneOf`：形状问题在**解码**阶段被拒，回复是
    // 带 `correlationId` 的 `error` 帧（§4.2），不是 `command.result`。
    let refused = client.expect_error().await;
    assert_eq!(
        refused["body"]["code"],
        json!("protocol.schema_invalid"),
        "{refused}"
    );
    assert_eq!(
        session_row_count(&host).await,
        before,
        "二选一被拒时不得创建会话：{refused}"
    );

    host.stop().await;
}
