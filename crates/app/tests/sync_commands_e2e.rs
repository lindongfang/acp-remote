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

/// 本机建一条 owned 会话并**落账**（§6 第 20 条的三笔时序：受理 → 执行 → 落账；E2E 的会话来源，
/// 不经 wire），返回会话标识文本。
///
/// 受理提交只落幂等记录——会话行与终态在**落账提交**里同批落盘，而本文件的用例都要求这条会话真实
/// 存在（`session.list`、快照与事件回放都要列得到它），因此创建后必须再结算一次（与生产适配层同形）。
/// 终态结果原文只是形状占位：本文件不读它。
async fn local_session(host: &HostNode) -> String {
    let request_id = acp_core::model::RequestId::new(&uuid_text()).expect("request id 规范");
    let prepared = match host
        .core()
        .create_session(
            &acp_core::model::Actor::LocalCli,
            request_id.clone(),
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
    {
        acp_core::ports::SessionCreateAdmission::Prepared(prepared) => prepared,
        other => panic!("首次受理必须返回 Prepared，得到 {other:?}"),
    };
    let result = acp_core::model::CommandResult::from_json_text(r#"{"sessionId":"projected"}"#)
        .expect("result object");
    assert!(
        host.core()
            .settle_session_create(
                &acp_core::model::Actor::LocalCli,
                &request_id,
                acp_core::model::CommandStatus::Completed,
                Some(result),
                None,
            )
            .await
            .expect("落账"),
        "落账必须真的写入"
    );
    prepared.session.as_str().to_owned()
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

/// 直接记录一个本机目录（`session.create` 的目录来源）。
async fn register_workspace(host: &HostNode) -> String {
    support::seed_directory(&host.data_dir(), &host.data_dir()).await
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

/// 直接读库：该会话的 `(agent_session_id, workspace_directory_id)`。外层 `None` = 没有这一行。
///
/// 目录归属的**权威来源只有** `owned_session.workspace_directory_id`（`storage-sqlite` 的
/// `workspace_from_row` 也按它投影摘要），因此「来源是路径时它是 `NULL`」在这里是落库事实，不经过
/// 任何投影推断。
async fn stored_session_columns(
    host: &HostNode,
    session_id: &str,
) -> Option<(Option<String>, Option<String>)> {
    let mut connection = raw_connection(host).await;
    let row: Option<(Option<String>, Option<String>)> =
        sqlx::query_as::<_, (Option<String>, Option<String>)>(
            "SELECT agent_session_id, workspace_directory_id FROM owned_session WHERE session_id = ?1",
        )
        .bind(session_id)
        .fetch_optional(&mut connection)
        .await
        .expect("读 owned_session 的两列");
    drop(connection);
    row
}

/// 等到该会话的「恢复列 + 目录归属」那一笔提交落盘再返回：三者由**同一条** `UPDATE` 写入
/// （`agent_session_id`/`workspace_cwd`/`workspace_directory_id`），因此 `agent_session_id` 非 `NULL` 就是
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

/// 8.3：`workspacePath` 成功路径——会话建成，且**来源是路径**时落库的 `workspace_directory_id` 是 `NULL`。
///
/// 变红点：实现若把客户端目录也当别名来源（例如拿路径末段或规范化结果造一个别名）或写成空串占位，
/// 那条 `UPDATE` 会把非 `NULL` 写进 `workspace_directory_id`，直接读库的断言与摘要里的未分组断言同时变红。
#[tokio::test]
async fn session_create_with_a_client_supplied_path_is_created_without_a_workspace_directory_id() {
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

    // ② 读路径：协商了 `core.local-catalog.v1`，摘要里 `workspace` 键必须在、且带派生标识。
    let summary = terminal["result"]["session"]
        .as_object()
        .expect("completed 必须携带会话摘要");
    assert!(
        summary.contains_key("workspace"),
        "协商了目录 feature 时 `workspace` 键必须存在：{terminal}"
    );
    let canonical = std::fs::canonicalize(host.data_dir()).expect("可规范化");
    let expected_id = acp_core::model::derive_directory_id(
        canonical.to_str().expect("UTF-8"),
    );
    assert_eq!(
        summary["workspace"]["directoryId"],
        json!(expected_id.as_str()),
        "客户端提供的目录在成功后被记录并成为归属：{terminal}"
    );

    // ③ 落库事实：目录标识与恢复列一起落盘。
    let (agent_session_id, workspace_id) = await_session_columns(&host, &session).await;
    assert!(
        agent_session_id.is_some(),
        "前提：目录归属那一笔提交已落盘（否则本用例断言不到任何东西）"
    );
    assert_eq!(
        workspace_id,
        Some(expected_id.as_str().to_owned()),
        "来源是路径的会话必须以派生标识落库（不得用空串或占位值）"
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
    let directory_id = register_workspace(&host).await;
    let mut client = connect_authenticated(&host, &device).await;

    let request = uuid_text();
    client.step("session.create（本机已登记的别名）");
    client
        .command(
            &request,
            "session.create",
            None,
            json!({ "agentId": AGENT, "directoryId": directory_id }),
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
        terminal["result"]["session"]["workspace"]["directoryId"],
        json!(directory_id),
        "目录来源必须投影出目录标识（不是未分组）：{terminal}"
    );

    let (agent_session_id, workspace_id) = await_session_columns(&host, &session).await;
    assert!(
        agent_session_id.is_some(),
        "前提：目录归属那一笔提交已落盘（否则本用例断言不到任何东西）"
    );
    assert_eq!(
        workspace_id,
        Some(directory_id),
        "目录标识必须按创建时的解析结果落库（不得只写 cwd、也不得省略）"
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
        json!({ "agentId": AGENT, "directoryId": "ffffffffffffffffffffffffffffffff" }),
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
    let directory_id = register_workspace(&host).await;
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
                "directoryId": directory_id,
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

// ---------------------------------------------------------------------------------------------
// 变更③（`session-create-settle-order`）：`session.create` 的三笔时序、失败分类与窗口期事件
//
// 被测层与替身边界：本组用例跑在**真实**的 Sync 面上（真实 loopback listener、真实 SQLite、真实
// `Authority`、真实 broker 与真实命令面路由）；唯一被替换的是 **Agent 子进程**那一层
// （`support::sync` 的 `ScriptedBackends`）——它在本次判据的边界之外（本组要钉的是「后端报告了哪一类
// 失败」之下**协议面**的结论：可证明 ⇒ `failed`、说不清 ⇒ `uncertain`、失败零会话行、窗口期事件按序
// 落库再广播）。后端自己的分类（spawn 失败 ⇒ `SessionNotEstablished`）属 `crates/agent-host`，由
// `-p agent-host` 与 `-p server` 的用例覆盖。后端的行为脚本（失败注入 / 窗口期事件注入 / 调用计数 /
// 终止记录）见 `support::sync::ScriptedScript`。
// ---------------------------------------------------------------------------------------------

/// `session.create` 的 payload：目录走「客户端提供的目录」这一支（本机数据目录是合法目录），
/// 因此不需要先登记 workspace alias——与本组用例的判据无关。
fn create_payload(host: &HostNode) -> Value {
    json!({
        "agentId": AGENT,
        "workspacePath": host.data_dir().display().to_string(),
    })
}

/// 认证并建立**实时屏障**（`sync.subscribe(null)` → `sync.reset_required` → 快照 → `sync.caught_up`
/// → `sync.ack`）：只有越过屏障的连接才会收到实时 `event` 帧。
async fn connect_with_live_events(host: &HostNode, device: &Device) -> SyncClient {
    let mut client = connect_authenticated(host, device).await;
    client.step("订阅并建立实时屏障");
    client.subscribe(None).await;
    let reset = client.expect_type("sync.reset_required").await;
    assert_eq!(reset["body"]["reason"], json!("initial_sync"), "{reset}");
    client.snapshot_request("initial_sync").await;
    let _snapshot = client.expect_snapshot().await;
    let caught_up = client.expect_type("sync.caught_up").await;
    client.ack(caught_up["body"]["cursor"].clone()).await;
    client
}

/// 一次命令交换收到的帧：该 `requestId` 的 `command.result`（按到达顺序）与途中的 `event` 帧。
struct Exchange {
    results: Vec<Value>,
    events: Vec<Value>,
}

impl Exchange {
    /// 首次响应（`accepted` 或 `rejected`）。
    fn first(&self) -> &Value {
        self.results.first().expect("必须收到首次响应")
    }

    /// 终态响应（`command.result` 的最后一条；调用方已等到终态）。
    fn terminal(&self) -> &Value {
        self.results.last().expect("必须收到终态响应")
    }

    /// 该请求的终态事件帧（`causationRequestId` 指回它，且事件类型是三个 `command.*` 之一）。
    fn terminal_event(&self, request: &str) -> &Value {
        self.events
            .iter()
            .find(|event| {
                event["causationRequestId"] == json!(request)
                    && matches!(
                        event["eventType"].as_str(),
                        Some("command.completed" | "command.failed" | "command.uncertain")
                    )
            })
            .unwrap_or_else(|| panic!("广播里必须有 `{request}` 的终态事件：{:?}", self.events))
    }
}

/// 读到该 `requestId` 的**终态** `command.result` 为止，把途中的 `event` 帧按到达顺序收下来
/// （`SyncClient::expect_command_result` 会把它们丢掉，而本组用例要逐条看它们）。
async fn exchange_until_terminal(client: &mut SyncClient, request_id: &str) -> Exchange {
    let deadline = tokio::time::Instant::now() + support::sync::MESSAGE_TIMEOUT;
    let mut exchange = Exchange {
        results: Vec::new(),
        events: Vec::new(),
    };
    loop {
        let message = client.take().await;
        if message["type"] == json!("command.result")
            && message["body"]["requestId"] == json!(request_id)
        {
            let body = message["body"].clone();
            let terminal = !matches!(body["status"].as_str(), Some("accepted") | Some("rejected"));
            exchange.results.push(body);
            if terminal {
                return exchange;
            }
        } else if message["type"] == json!("event") {
            exchange.events.push(message["body"].clone());
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "超时：`{request_id}` 的终态没有在期限内到达"
        );
    }
}

/// 继续收帧直到 `ready` 为真。
///
/// 为什么需要：实时扇出与终态观察者是两条独立的任务，因此「终态 `command.result` 先于该会话的实时
/// `event` 帧到达」是**允许**的（终态那条由观察者轮询持久记录后推送）。断言事件序列之前必须先确认
/// 期待的事件都到齐了，否则一批正常的实现会被判成丢事件。
async fn drain_until(
    client: &mut SyncClient,
    exchange: &mut Exchange,
    ready: impl Fn(&Exchange) -> bool,
) {
    let deadline = tokio::time::Instant::now() + support::sync::MESSAGE_TIMEOUT;
    while !ready(exchange) {
        let message = client.take().await;
        if message["type"] == json!("event") {
            exchange.events.push(message["body"].clone());
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "超时：期待的事件没有在期限内到齐（已收到 {:?}）",
            exchange.events
        );
    }
}

/// 直接读库：`owned_command` 里该 `requestId` 的
/// `(status, session_id, terminal_event_id, error_code)`；`None` = 没有这一行。
async fn stored_command(
    host: &HostNode,
    request_id: &str,
) -> Option<(String, Option<String>, Option<String>, Option<String>)> {
    let mut connection = raw_connection(host).await;
    let row = sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>)>(
        "SELECT status, session_id, terminal_event_id, error_code FROM owned_command \
         WHERE request_id = ?1",
    )
    .bind(request_id)
    .fetch_optional(&mut connection)
    .await
    .expect("读 owned_command");
    drop(connection);
    row
}

/// 直接读库：该会话有没有一条会话行。
async fn session_row_exists(host: &HostNode, session_id: &str) -> bool {
    let mut connection = raw_connection(host).await;
    let row: Option<i64> =
        sqlx::query_scalar::<_, i64>("SELECT 1 FROM owned_session WHERE session_id = ?1")
            .bind(session_id)
            .fetch_optional(&mut connection)
            .await
            .expect("读 owned_session");
    drop(connection);
    row.is_some()
}

/// 直接读库：某会话的 `owned_event` 行，按**会话内序号**升序的
/// `(eventId, eventType, sessionSequence)`（`session_sequence` 是 INTEGER 列，因此按整数取）。
async fn stored_session_events(host: &HostNode, session_id: &str) -> Vec<(String, String, i64)> {
    let mut connection = raw_connection(host).await;
    let rows = sqlx::query_as::<_, (String, String, i64)>(
        "SELECT event_id, event_type, session_sequence FROM owned_event \
         WHERE session_id = ?1 ORDER BY session_sequence ASC",
    )
    .bind(session_id)
    .fetch_all(&mut connection)
    .await
    .expect("读 owned_event");
    drop(connection);
    rows
}

/// 直接读库：**会话级**事件行（`session_id` 非空）的条数。
async fn session_scoped_event_count(host: &HostNode) -> i64 {
    let mut connection = raw_connection(host).await;
    let count: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM owned_event WHERE session_id IS NOT NULL",
    )
    .fetch_one(&mut connection)
    .await
    .expect("统计会话级事件");
    drop(connection);
    count
}

/// 4.1 ①②：后端**可证明**未建立会话时，客户端先收到 `accepted`、再收到终态 `failed`；那条失败在库里
/// **零会话行**。
///
/// 变红点：①受理不先回 `accepted`（例如把失败直接冒泡成 `rejected`，或只在终态回一条结果）⇒ 首次
/// 响应的 status 断言红；②终态不是 `failed`（例如按旧形状把可证明的失败报成 `uncertain`，或反过来
/// 把 `uncertain` 当 `failed`）⇒ status 与广播事件类型两条同时红；③失败留下会话行或把窗口期排队的
/// 事件冲进库里 ⇒ `owned_session`/`owned_event` 的条数断言红；④错误码不是既有的
/// `internal.unavailable` ⇒ 错误码断言红。
#[tokio::test]
async fn a_failed_create_is_observed_as_accepted_then_failed_with_zero_session_rows() {
    let host = HostNode::start("sync-create-failed", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    // 后端在失败**之前**已经送出过三条件（真实 Agent 建会话期间就会产出 `session/update`）：失败路径
    // 必须把窗口期队列一并作废，一条事件行都不留。
    host.script().emit_window_events(3);
    // `SessionNotEstablished` = 可证明 Agent 侧未产生会话（spawn 失败 / 明确拒绝 / 请求未发出）。
    host.script()
        .fail_create(acp_core::model::UnavailableKind::SessionNotEstablished);
    let mut client = connect_with_live_events(&host, &device).await;

    let rows_before = session_row_count(&host).await;
    assert_eq!(rows_before, 0, "前提：本机还没有任何会话行");

    let request = uuid_text();
    client.step("session.create（可证明未建立会话）");
    client
        .command(&request, "session.create", None, create_payload(&host))
        .await;
    let mut exchange = exchange_until_terminal(&mut client, &request).await;

    // ① 首次响应是 `accepted`：`result` 与 `terminalEventId` 均为 `null`，`acceptedAt` 非空。
    let accepted = exchange.first();
    assert_eq!(accepted["status"], json!("accepted"), "{accepted}");
    assert_eq!(accepted["result"], Value::Null, "accepted 不得带业务结果");
    assert_eq!(
        accepted["terminalEventId"],
        Value::Null,
        "accepted 没有终态事件"
    );
    assert!(
        accepted["acceptedAt"].is_string(),
        "accepted 必须带首次接受时间：{accepted}"
    );

    // ② 随后是终态 `failed`：错误码是既有的 `internal.unavailable`，成因落在 `message` 上
    //    （design D4：不新增错误码）。
    let terminal = exchange.terminal();
    assert_eq!(terminal["status"], json!("failed"), "{terminal}");
    assert_eq!(
        terminal["error"]["code"],
        json!("internal.unavailable"),
        "{terminal}"
    );
    assert!(
        terminal["error"]["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty()),
        "成因必须落在 message 里：{terminal}"
    );
    assert_eq!(
        terminal["result"],
        Value::Null,
        "失败不得携带业务结果：{terminal}"
    );
    let terminal_event = terminal["terminalEventId"]
        .as_str()
        .expect("failed 是终态，必须带 terminalEventId")
        .to_owned();

    // ③ 终态是**持久化并广播**的事件，不是凭空一条回包：实时扇出里能收到该 requestId 的终态事件，
    //    事件类型是 `command.failed`，eventId 与回包里的 terminalEventId 相同。
    drain_until(&mut client, &mut exchange, |exchange| {
        exchange
            .events
            .iter()
            .any(|event| event["causationRequestId"] == json!(request))
    })
    .await;
    let event_frame = exchange.terminal_event(&request);
    assert_eq!(
        event_frame["eventType"],
        json!("command.failed"),
        "{event_frame}"
    );
    assert_eq!(
        event_frame["eventId"],
        json!(terminal_event),
        "{event_frame}"
    );

    // ④ 失败零会话行：**按会话标识**断言到具体那一行（标识取自后端实际收到的 `create` 参数），而不是
    //    只看总数——同时也没有它的任何事件行（三条件随失败作废）。
    let created = host.script().created();
    assert_eq!(created.len(), 1, "后端恰好被要求建立一条会话：{created:?}");
    assert!(
        !session_row_exists(&host, &created[0]).await,
        "失败的那条会话不得有会话行：{}",
        created[0]
    );
    assert!(
        stored_session_events(&host, &created[0]).await.is_empty(),
        "失败的那条会话不得有事件行：{}",
        created[0]
    );
    assert_eq!(
        session_row_count(&host).await,
        rows_before,
        "后端失败 MUST 零会话行"
    );
    assert_eq!(
        session_scoped_event_count(&host).await,
        0,
        "失败路径不得留下任何会话级事件（窗口期队列随失败作废）"
    );
    // ⑤ 持久记录是这一组断言的另一面：终态写在**受理那一次的幂等行**上，且不指向会话行。
    let record = stored_command(&host, &request)
        .await
        .expect("受理已经落盘，必须有该 requestId 的持久记录");
    assert_eq!(record.0, "failed", "持久终态必须是 failed：{record:?}");
    assert_eq!(record.1, None, "failed 的记录不得指向会话行：{record:?}");
    assert_eq!(
        record.2.as_deref(),
        Some(terminal_event.as_str()),
        "回包的 terminalEventId 必须是落库的那条事件：{record:?}"
    );
    assert_eq!(
        record.3.as_deref(),
        Some("internal.unavailable"),
        "{record:?}"
    );
    // ⑥ 失败来自后端调用之内（不是被前置拒绝）：`create` 恰好被派发一次。
    assert_eq!(
        host.script().create_calls(),
        1,
        "失败必须发生在后端调用之内"
    );

    host.stop().await;
}

/// 4.1 ①②的对照组：**说不清**的后端失败必须终结为 `uncertain`，不得报 `failed`，同样零会话行。
///
/// 与上一条成对，两条一起把「可证明 / 说不清」这个判据在协议面钉成**双向**可证伪：实现若一律报
/// `failed`，本条红；若一律报 `uncertain`，上一条红。
///
/// 变红点：①终态是 `failed`（把「说不清」谎报成「什么也没发生」）⇒ status 与广播事件类型同时红；
/// ②失败留下会话行或事件行 ⇒ `owned_session`/`owned_event` 的条数断言红。
#[tokio::test]
async fn an_unprovable_create_failure_ends_as_uncertain_with_zero_session_rows() {
    let host = HostNode::start("sync-create-uncertain", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    // `IoError` 一类**不能证明** Agent 侧没有产生会话（请求可能已发出）⇒ 终态是 `uncertain`。
    host.script()
        .fail_create(acp_core::model::UnavailableKind::IoError);
    let mut client = connect_with_live_events(&host, &device).await;

    let request = uuid_text();
    client.step("session.create（说不清的后端失败）");
    client
        .command(&request, "session.create", None, create_payload(&host))
        .await;
    let mut exchange = exchange_until_terminal(&mut client, &request).await;

    assert_eq!(exchange.first()["status"], json!("accepted"));
    let terminal = exchange.terminal();
    assert_eq!(terminal["status"], json!("uncertain"), "{terminal}");
    assert_eq!(
        terminal["error"]["code"],
        json!("command.uncertain"),
        "说不清的成因取 core 的「无法确认」码：{terminal}"
    );
    assert_eq!(
        terminal["result"],
        Value::Null,
        "不确定的命令不得有结果：{terminal}"
    );
    let terminal_event = terminal["terminalEventId"]
        .as_str()
        .expect("uncertain 是终态，必须带 terminalEventId")
        .to_owned();

    drain_until(&mut client, &mut exchange, |exchange| {
        exchange
            .events
            .iter()
            .any(|event| event["causationRequestId"] == json!(request))
    })
    .await;
    let event_frame = exchange.terminal_event(&request);
    assert_eq!(
        event_frame["eventType"],
        json!("command.uncertain"),
        "终态事件类型必须与判据一致：{event_frame}"
    );
    assert_eq!(
        event_frame["eventId"],
        json!(terminal_event),
        "{event_frame}"
    );

    let created = host.script().created();
    assert_eq!(created.len(), 1, "后端恰好被要求建立一条会话：{created:?}");
    assert!(
        !session_row_exists(&host, &created[0]).await,
        "说不清的失败也不得留下会话行：{}",
        created[0]
    );
    assert!(
        stored_session_events(&host, &created[0]).await.is_empty(),
        "说不清的失败也不得留下事件行：{}",
        created[0]
    );
    assert_eq!(session_row_count(&host).await, 0, "说不清也必须零会话行");
    assert_eq!(
        session_scoped_event_count(&host).await,
        0,
        "失败路径不得留下任何会话级事件"
    );
    let record = stored_command(&host, &request)
        .await
        .expect("受理已经落盘，必须有该 requestId 的持久记录");
    assert_eq!(record.0, "uncertain", "{record:?}");
    assert_eq!(record.1, None, "{record:?}");
    assert_eq!(record.3.as_deref(), Some("command.uncertain"), "{record:?}");
    assert_eq!(host.script().create_calls(), 1);

    host.stop().await;
}

/// 4.1 ③：同一 `requestId` 重发必须拿到**首次的终态**、不产生第二个会话、不派发第二次创建。
///
/// 用**成功**路径而不是失败路径做这一条：失败路径两侧都是零会话行，「不产生第二个会话」在那里恒真
/// （观察不到差别）；成功路径下会话行数从 1 变 2、后端调用数从 1 变 2 都能被看见。
///
/// 变红点：①重发被当成新命令（再派发一次创建）⇒ 后端 `create` 调用数变 2、会话行变 2 条；②重发回
/// 一条新的 `accepted`（而不是首次终态）⇒ 重发交换里 `command.result` 的条数/首条 status 断言红；
/// ③重发覆盖了首次记录（新的 terminalEventId 或新的会话标识）⇒ 与首次逐字段比对的两条断言红。
#[tokio::test]
async fn resending_the_same_request_id_returns_the_first_terminal_without_a_second_dispatch() {
    let host = HostNode::start("sync-create-replay", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect_authenticated(&host, &device).await;

    let request = uuid_text();
    let payload = create_payload(&host);
    client.step("首次 session.create");
    client
        .command(&request, "session.create", None, payload.clone())
        .await;
    let first_round = exchange_until_terminal(&mut client, &request).await;
    // 首次：恰好两条 `command.result`（`accepted` → 终态）。
    assert_eq!(
        first_round.results.len(),
        2,
        "首次提交必须先 `accepted` 再终态：{:?}",
        first_round.results
    );
    assert_eq!(
        first_round.first()["status"],
        json!("accepted"),
        "{:?}",
        first_round.results
    );
    let terminal = first_round.terminal();
    assert_eq!(terminal["status"], json!("completed"), "{terminal}");
    let session = terminal["result"]["sessionId"]
        .as_str()
        .expect("completed 必须给出新会话标识")
        .to_owned();
    let terminal_event = terminal["terminalEventId"]
        .as_str()
        .expect("completed 是终态，必须带 terminalEventId")
        .to_owned();
    let accepted_at = terminal["acceptedAt"].clone();
    assert_eq!(host.script().create_calls(), 1, "首次创建恰好派发一次");
    assert_eq!(
        host.script().created(),
        vec![session.clone()],
        "终态给出的会话标识必须是 core 交给后端的那一个"
    );
    assert_eq!(session_row_count(&host).await, 1, "首次创建恰好一条会话行");

    // 重发：同 `requestId` + 同 payload（幂等键是 `(deviceId, requestId)`，指纹必须一致）。
    client.step("同一 requestId 重发");
    client
        .command(&request, "session.create", None, payload)
        .await;
    let replay_round = exchange_until_terminal(&mut client, &request).await;
    // 重发的**首次**响应就是首次终态（不得再回一条 `accepted`）。
    assert_eq!(
        replay_round.results.len(),
        1,
        "重发必须直接回首次终态（不得再受理一次）：{:?}",
        replay_round.results
    );
    let replayed = replay_round.first();
    assert_eq!(replayed["status"], json!("completed"), "{replayed}");
    assert_eq!(
        replayed["terminalEventId"],
        json!(terminal_event),
        "重发必须回**首次**的 terminalEventId：{replayed}"
    );
    assert_eq!(
        replayed["acceptedAt"], accepted_at,
        "重发的 acceptedAt 必须是首次接受时间：{replayed}"
    );
    assert_eq!(
        replayed["result"]["sessionId"],
        json!(session),
        "重发必须回首次建立的会话：{replayed}"
    );
    // 零新增副作用：后端只被派发一次、本机只有那一条会话行。
    assert_eq!(
        host.script().create_calls(),
        1,
        "重发 MUST NOT 派发第二次创建"
    );
    assert_eq!(
        session_row_count(&host).await,
        1,
        "重发 MUST NOT 产生第二个会话"
    );

    host.stop().await;
}

/// 4.1 ④：崩溃窗口（受理已落盘、落账未发生）经启动恢复终结为 `uncertain`，且本机没有该会话的会话行。
///
/// 窗口直接经用例面构造（`create_session` 返回 `Prepared` 之后**不结算**），因为崩溃窗口的持久形状
/// 就是「有一条 `accepted`、`session_id` 为空的幂等记录，没有会话行」——协议面造不出它（命令面在
/// `create_session` 返回之前不会回 `accepted`）。
///
/// 变红点：①恢复走「有会话分支」或把该记录当已终结 ⇒ `command.status` 的 `state` 不是 `uncertain`；
/// ②恢复自动重试（再派发一次创建）⇒ 后端 `create` 调用数变 2；③恢复顺手补建会话行 ⇒ `owned_session`
/// 的断言红；④终态事件没有落库/没有指向该请求 ⇒ `owned_command` 的四列断言与事件行断言红。
#[tokio::test]
async fn the_crash_window_recovers_as_uncertain_without_a_session_row() {
    let host = HostNode::start("sync-create-crash", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect_authenticated(&host, &device).await;

    // 受理（actor 与 Sync 面同源：`Actor::Device`，因此之后能凭 `command.status` 读到它）之后**不结算**。
    let request = uuid_text();
    let actor = acp_core::model::Actor::Device {
        device: acp_core::model::DeviceId::new(&device.id()).expect("device id 规范"),
    };
    let prepared = match host
        .core()
        .create_session(
            &actor,
            acp_core::model::RequestId::new(&request).expect("request id 规范"),
            ac1_digest("acpr-remote/sync-commands-e2e/crash-window"),
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
        .expect("本机会话受理")
    {
        acp_core::ports::SessionCreateAdmission::Prepared(prepared) => prepared,
        other => panic!("首次受理必须返回 Prepared，得到 {other:?}"),
    };

    // 窗口期的持久形状：只有幂等记录（会话标识为空），没有会话行。
    let window = stored_command(&host, &request)
        .await
        .expect("受理必须落幂等记录");
    assert_eq!(window.0, "accepted", "受理后命令停在 accepted：{window:?}");
    assert_eq!(window.1, None, "受理不得写会话行：{window:?}");
    assert!(
        !session_row_exists(&host, prepared.session.as_str()).await,
        "受理阶段不得有该会话的会话行：{prepared:?}"
    );
    assert_eq!(session_row_count(&host).await, 0, "受理不得建会话行");
    assert_eq!(host.script().create_calls(), 1, "受理派发了一次创建");

    // 启动恢复（组合根在取得单实例锁、开始监听之前调用的那一步）。
    let recovered = host
        .core()
        .recover_unsettled(
            &acp_core::model::Actor::LocalCli,
            acp_core::ports::ReplayLimit::default(),
        )
        .await
        .expect("启动恢复");
    assert_eq!(recovered, 1, "恢复必须终结那条 accepted 记录");

    // 协议级观察：`command.status`（§11.4）读到该命令的终态。`uncertain` 是终态之一，且必须指向持久
    // 终态事件（`owned_command` 的 CHECK 也这么要求）。
    let status_request = uuid_text();
    client.step("command.status（恢复之后重查）");
    client
        .command(
            &status_request,
            "command.status",
            None,
            json!({ "targetRequestId": request }),
        )
        .await;
    let queried = client.expect_command_result(&status_request).await;
    assert_eq!(queried["status"], json!("completed"), "{queried}");
    let status = &queried["result"];
    assert_eq!(
        status["state"],
        json!("uncertain"),
        "崩溃窗口的终态必须是 uncertain：{status}"
    );
    assert_eq!(
        status["error"]["code"],
        json!("command.uncertain"),
        "成因是 core 的「无法确认」码：{status}"
    );
    assert_eq!(
        status["result"],
        Value::Null,
        "没有会话行时不得有结果：{status}"
    );
    let terminal_event = status["terminalEventId"]
        .as_str()
        .expect("uncertain 必须带 terminalEventId")
        .to_owned();

    // 本机没有该会话的会话行，也没有它的任何事件行。
    assert!(
        !session_row_exists(&host, prepared.session.as_str()).await,
        "崩溃窗口不得留下会话行：{prepared:?}"
    );
    assert_eq!(session_row_count(&host).await, 0, "崩溃窗口不得留下会话行");
    assert!(
        stored_session_events(&host, prepared.session.as_str())
            .await
            .is_empty(),
        "崩溃窗口不得留下该会话的事件行"
    );
    // 持久记录与协议回包同源，且**没有自动重试**（恢复不重新派发创建）。
    let persisted = stored_command(&host, &request).await.expect("持久记录");
    assert_eq!(persisted.0, "uncertain", "{persisted:?}");
    assert_eq!(persisted.1, None, "{persisted:?}");
    assert_eq!(
        persisted.2.as_deref(),
        Some(terminal_event.as_str()),
        "{persisted:?}"
    );
    assert_eq!(
        persisted.3.as_deref(),
        Some("command.uncertain"),
        "{persisted:?}"
    );
    assert_eq!(
        host.script().create_calls(),
        1,
        "恢复 MUST NOT 自动重试创建（不得再派发一次）"
    );

    host.stop().await;
}

/// 4.2 ⑤：会话行落账之前到达的后端事件不丢、不重，且在落账后按**到达顺序**落库并广播。
///
/// 变红点：①窗口期事件被丢（例如超限之前就丢弃、或失败关闭误触发）⇒ 会话级事件条数与序号连续两条
/// 断言红；②落账后没有 flush（事件留在内存里）⇒ 条数断言与落库断言同时红；③重排（例如按 view 或
/// eventId 排序、或并发 flush 交错）⇒ `currentModeId` 的顺序断言与「落库 eventId 序列 = 广播序列」
/// 断言红；④重复写库（同一事件两条行）⇒ `owned_event` 的 `session_sequence` 唯一索引会直接报错，
/// 条数断言也红。
#[tokio::test]
async fn window_events_are_stored_and_broadcast_in_arrival_order() {
    let host = HostNode::start("sync-create-window", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    // 后端在 `create` 返回**之前**送出四条事件：那一刻会话行还没有落账（窗口期）。
    let windowed = ["m0", "m1", "m2", "m3"];
    host.script().emit_window_events(windowed.len());
    let mut client = connect_with_live_events(&host, &device).await;

    let request = uuid_text();
    client.step("session.create（后端在窗口期产出四条事件）");
    client
        .command(&request, "session.create", None, create_payload(&host))
        .await;
    let mut exchange = exchange_until_terminal(&mut client, &request).await;
    assert_eq!(exchange.first()["status"], json!("accepted"));
    let terminal = exchange.terminal();
    assert_eq!(terminal["status"], json!("completed"), "{terminal}");
    let session = terminal["result"]["sessionId"]
        .as_str()
        .expect("completed 必须给出新会话标识")
        .to_owned();
    let terminal_event = terminal["terminalEventId"]
        .as_str()
        .expect("completed 必须带 terminalEventId")
        .to_owned();
    // 前提：后端确实被要求建立**这一条**会话（事件序列的断言因此指向真实的会话槽位）。
    assert_eq!(
        host.script().created(),
        vec![session.clone()],
        "终态给出的会话标识必须是 core 交给后端的那一个"
    );

    // 收齐该会话的全部实时事件（终态那条 + 窗口期四条）。
    drain_until(&mut client, &mut exchange, |exchange| {
        exchange
            .events
            .iter()
            .filter(|event| event["sessionId"] == json!(session))
            .count()
            > windowed.len()
    })
    .await;
    let broadcast: Vec<&Value> = exchange
        .events
        .iter()
        .filter(|event| event["sessionId"] == json!(session))
        .collect();
    assert_eq!(
        broadcast.len(),
        windowed.len() + 1,
        "该会话必须恰有「终态 + 窗口期四条」这么多事件：{broadcast:?}"
    );

    // ① 广播：会话内序号连续，且终态事件占 1（会话行与终态在**同一次**落账提交里，窗口期事件在它
    //    之后按到达顺序写库）。
    let sequences: Vec<&str> = broadcast
        .iter()
        .map(|event| {
            event["sessionSequence"]
                .as_str()
                .expect("会话级事件必须带会话内序号")
        })
        .collect();
    assert_eq!(
        sequences,
        vec!["1", "2", "3", "4", "5"],
        "广播的会话内序号必须连续且按到达顺序：{broadcast:?}"
    );
    assert_eq!(
        broadcast[0]["eventId"],
        json!(terminal_event),
        "会话内序号 1 必须是回包指向的那条终态事件：{broadcast:?}"
    );
    assert_eq!(
        broadcast[0]["eventType"],
        json!("command.completed"),
        "{broadcast:?}"
    );
    // 正文顺序 = 到达顺序（重排会在这里变红；不重不丢由条数与序号连续性一并钉住）。
    let modes: Vec<&str> = broadcast
        .iter()
        .skip(1)
        .map(|event| {
            event["payload"]["view"]["currentModeId"]
                .as_str()
                .expect("mode 事件的 view 必须带 currentModeId")
        })
        .collect();
    assert_eq!(
        modes, windowed,
        "窗口期事件必须按到达顺序广播：{broadcast:?}"
    );

    // ② 落库：与广播**逐条相同**（同一条事件、同一个 eventId、同一个会话内序号、同一个顺序）。
    let stored = stored_session_events(&host, &session).await;
    let stored_ids: Vec<String> = stored
        .iter()
        .map(|(event_id, _, _)| event_id.clone())
        .collect();
    let broadcast_ids: Vec<String> = broadcast
        .iter()
        .map(|event| event["eventId"].as_str().expect("eventId").to_owned())
        .collect();
    assert_eq!(
        stored_ids, broadcast_ids,
        "落库 eventId 序列必须与广播序列逐条相同（落库 {stored:?}）"
    );
    let stored_sequences: Vec<i64> = stored.iter().map(|(_, _, sequence)| *sequence).collect();
    assert_eq!(
        stored_sequences,
        vec![1, 2, 3, 4, 5],
        "落库的会话内序号必须连续且一条不重：{stored:?}"
    );
    let stored_types: Vec<String> = stored
        .iter()
        .map(|(_, event_type, _)| event_type.clone())
        .collect();
    assert_eq!(
        stored_types,
        vec![
            "command.completed".to_owned(),
            "session.mode.changed".to_owned(),
            "session.mode.changed".to_owned(),
            "session.mode.changed".to_owned(),
            "session.mode.changed".to_owned(),
        ],
        "落库形状必须是「终态事件 + 窗口期四条」：{stored:?}"
    );

    host.stop().await;
}

/// 4.2 ⑥：窗口期事件超过 `WINDOW_EVENT_QUEUE_LIMIT` 时走**失败关闭**——命令终结为 `uncertain`、零会话
/// 行、且后端被要求**终止**（`SessionBackendFactory::terminate_agent`）。
///
/// 变红点：①超限仍然 `completed`（把半个会话报成建成）⇒ 终态 status 断言红；②超限静默丢事件、只把
/// 命令报成 `uncertain` 但不结束该 Agent ⇒ 脚本后端记录到的 `terminate_agent` 调用为空、条数断言红；
/// ③超限路径顺手建了会话行或写下了已入队的事件 ⇒ `owned_session`/`owned_event` 的条数断言红（被终止
/// 的那条会话标识从后端记录里取得，因此这是对**具体会话**的断言，不是只看总数）。
#[tokio::test]
async fn a_window_event_overflow_closes_the_backend_and_ends_the_create_as_uncertain() {
    let host = HostNode::start("sync-create-overflow", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    // 第 `LIMIT + 1` 条越界：不再入队，由结算路径失败关闭。
    host.script()
        .emit_window_events(acp_core::broker::WINDOW_EVENT_QUEUE_LIMIT + 1);
    let mut client = connect_authenticated(&host, &device).await;

    let request = uuid_text();
    client.step("session.create（窗口期事件超限）");
    client
        .command(&request, "session.create", None, create_payload(&host))
        .await;
    let accepted = client.expect_command_result(&request).await;
    assert_eq!(accepted["status"], json!("accepted"), "{accepted}");
    assert_eq!(accepted["result"], Value::Null, "{accepted}");
    let terminal = client.expect_command_result(&request).await;

    // ① 超限意味着无法确认会话是否已建立：终态是 `uncertain`，不是 `completed`。
    assert_eq!(terminal["status"], json!("uncertain"), "{terminal}");
    assert_eq!(
        terminal["error"]["code"],
        json!("command.uncertain"),
        "{terminal}"
    );
    assert_eq!(
        terminal["result"],
        Value::Null,
        "没有会话行时不得有结果：{terminal}"
    );
    assert!(
        terminal["terminalEventId"].is_string(),
        "uncertain 必须指向持久终态事件：{terminal}"
    );

    // ② 后端被要求**终止**该 Agent 运行时（core 不持进程句柄，只关端点不足以让事件源停下）。
    let terminated = host.script().terminated();
    assert_eq!(
        terminated.len(),
        1,
        "超限必须恰好要求终止一次：{terminated:?}"
    );
    let terminated_session = terminated[0].clone();
    assert_eq!(
        host.script().created(),
        vec![terminated_session.clone()],
        "被终止的必须是后端实际被要求建立的那条会话：{terminated:?}"
    );

    // ③ 零会话行：被终止的那条会话没有会话行，也没有任何事件行。
    assert_eq!(
        session_row_count(&host).await,
        0,
        "失败关闭 MUST 零会话行：{terminated:?}"
    );
    assert!(
        !session_row_exists(&host, &terminated_session).await,
        "被终止的会话不得有会话行：{terminated_session}"
    );
    assert!(
        stored_session_events(&host, &terminated_session)
            .await
            .is_empty(),
        "被终止的会话不得有事件行：{terminated_session}"
    );
    assert_eq!(
        session_scoped_event_count(&host).await,
        0,
        "失败关闭不得写任何会话级事件"
    );

    // ④ 持久记录：终态落在该命令的幂等行上，且不指向会话行。
    let record = stored_command(&host, &request).await.expect("持久记录");
    assert_eq!(record.0, "uncertain", "{record:?}");
    assert_eq!(record.1, None, "{record:?}");
    assert_eq!(record.3.as_deref(), Some("command.uncertain"), "{record:?}");
    // ⑤ 超限发生在**执行之后**的结算路径上：创建本身只被派发一次。
    assert_eq!(host.script().create_calls(), 1);

    host.stop().await;
}
