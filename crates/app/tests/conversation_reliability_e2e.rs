//! 对话可靠性（`conversation-reliability` 任务 5.1）的跨层套件：真实组合根形状 + 真实 SQLite +
//! 真实 loopback listener，只有 Agent 边界是脚本化后端。
//!
//! 覆盖 Main E2E 里**可以自动化**的部分（E1–E7），断言全部指向可观察事实：
//!
//! - E1：首条无回显 + 两条后续输入在停/重启 Daemon 后仍完整可读，第二个已认证设备读到同一批正文；
//! - E2：历史“继续”后的两次独立“继续”各自有身份；同一 requestId 重试只保存一次、只派发一次；
//! - E3：受理事务失败零派发零广播；保存后派发失败正文仍在；重启把 accepted 收敛为 uncertain 且不重发；
//! - E5：文字与附件同事务保存、上传/封存失败不派发、派发失败后已保存输入仍在、异内容回显保留；
//! - E6：准备 `allocate → restore` 同 ID、同一资源代与同一临时端点（浏览器刷新见 `clients/app/e2e/`）；
//! - E7：两个已认证连接并发 restore 同一份准备，不产生第二个准备 ID 或第二个临时端点。
//!
//! 浏览器/真实 Chromium 与真实 Codex 的验收不在本文件：页面路径见 `clients/app/e2e/`（PV4），
//! 真实 Agent 见任务 5.2。本文件不冒充其中任何一项。

mod support;

use std::time::Duration;

use serde_json::{Value, json};
use support::sync::{DeviceKey, HostNode, SyncClient, base64url, nonce_text, uuid_text};

/// 本机宿主名（`daemon.public_origin` 的 authority；`Host` 头与 `Origin` 头都按它判）。
const HOST: &str = "conversation-reliability.example.invalid";
/// 设备名（`deviceName`）。
const DEVICE_NAME: &str = "Reliability Phone";
/// 脚本化 Agent selector。
const AGENT: &str = "codex";
/// 重启用例的固定节点身份种子（`EphemeralKeystore` 只活在进程内，固定后才是「同一节点的重启」）。
const SEED: [u8; 32] = [0x7b; 32];

/// 本套件协商的 feature 集合：正文读取、命令终态、准备、附件与生命周期缺一不可。
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
        "core.session-activity.v1",
        "core.session-title.v1",
        "core.typed-media.v1",
        server::sync::local_lifecycle::LOCAL_LIFECYCLE_FEATURE,
        server::sync::attachments::ATTACHMENT_FEATURE,
        server::sync::preparation::PREPARATION_FEATURE,
    ]
}

// ---------------------------------------------------------------------------------------------
// 配对与握手（真实 HTTP 端点 + 真实四消息握手；与 `session_preparation` 同一套真实路径）
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

/// `device.pair.begin → claim → device.pair.confirm → status approved`。
async fn pair_and_approve(host: &HostNode, device: &Device) -> Value {
    let begin = host
        .admin(
            server::local_admin::Method::DevicePairBegin,
            json!({ "expiresInMs": null }),
        )
        .await;
    let pairing_id = begin["pairingId"].as_str().expect("pairingId").to_owned();
    let pairing_url = begin["pairingUrl"].as_str().expect("pairingUrl");
    let (ticket, secret) = retrieve(host, pairing_url).await;

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
    replied.body
}

/// 业务连接：真实握手 + 本套件需要的 feature。
async fn connect(host: &HostNode, device: &Device) -> SyncClient {
    let mut client = SyncClient::connect_plain(host.addr(), host.host_name(), &host.origin()).await;
    client.step("四消息握手（业务连接）");
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
        "设备必须完成认证：{authenticated}"
    );
    client
}

/// 独立附件连接（`/sync/v1/attachments`）：同一条设备身份、同一套四消息握手。
async fn connect_attachments(host: &HostNode, device: &Device) -> SyncClient {
    let mut client = SyncClient::connect_plain_path(
        host.addr(),
        server::sync::conn::ATTACHMENTS_WS_PATH,
        host.host_name(),
        &host.origin(),
    )
    .await;
    client.step("四消息握手（附件连接）");
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
        "附件连接必须完成独立认证：{authenticated}"
    );
    client
}

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
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, payload)
        .expect("q2 payload 是无填充 base64url");
    assert_eq!(bytes.len(), 65, "q2 payload 解出 65 字节");
    assert_eq!(bytes[0], 2, "q2 版本必须是 2");
    let mut token = [0_u8; 32];
    let mut fingerprint = [0_u8; 32];
    token.copy_from_slice(&bytes[1..33]);
    fingerprint.copy_from_slice(&bytes[33..]);
    (token, fingerprint, origin)
}

async fn retrieve(host: &HostNode, pairing_url: &str) -> (Value, [u8; 32]) {
    let (token, fingerprint, origin) = decode_device_qr(pairing_url);
    assert_eq!(origin, host.origin());
    use sha2::{Digest as _, Sha256};
    let expected: [u8; 32] = Sha256::digest(host.public_key().await.as_bytes()).into();
    assert_eq!(fingerprint, expected, "二维码固定的是完整主机公钥指纹");
    let replied = post_json(
        host,
        server::sync::pairing::QR_LOOKUP_PATH,
        &json!({
            "protocolVersion": 1,
            "qrToken": base64url(&token),
        }),
    )
    .await;
    assert_eq!(replied.status, 200, "扫码取回必须成功：{}", replied.body);
    let secret = base64::Engine::decode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        replied.body["pairingSecret"]
            .as_str()
            .expect("pairingSecret"),
    )
    .expect("secret 是无填充 base64url");
    let secret: [u8; 32] = secret
        .as_slice()
        .try_into()
        .expect("pairing secret 是 32 字节");
    (replied.body, secret)
}

fn public_key_base64url(device: &Device) -> String {
    base64url(device.key.public_key().as_bytes())
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
    base64url(proof.as_bytes())
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
    base64url(proof.as_bytes())
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
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&outer.finalize());
    bytes
}

// ---------------------------------------------------------------------------------------------
// 准备 / 首条 / 读取 / 派发的驱动件
// ---------------------------------------------------------------------------------------------

/// `allocate` 的 `ready` 结果（会话 ID、凭据与运行期代）。
#[derive(Clone)]
struct Prepared {
    session: String,
    receipt: Value,
    runtime_epoch: String,
    generation: String,
}

/// 一个真实存在、可规范化的测试工作目录。
fn workspace_path(host: &HostNode) -> String {
    let path = host.data_dir().join("reliability-workspace");
    std::fs::create_dir_all(&path).expect("测试工作目录");
    std::fs::canonicalize(&path)
        .expect("规范化工作目录")
        .to_string_lossy()
        .into_owned()
}

/// `session.preparation.allocate`（真实控制帧）：要求 `ready`。
async fn allocate(client: &mut SyncClient, workspace: &str) -> Prepared {
    let operation = uuid_text();
    client.step("session.preparation.allocate");
    client
        .send(
            "session.preparation",
            json!({
                "operationId": operation,
                "action": "allocate",
                "agentId": AGENT,
                "workspace": { "workspacePath": workspace },
                "tabNonce": nonce_text(0x41),
            }),
        )
        .await;
    let body = client.expect_type("session.preparation.result").await["body"].clone();
    assert_eq!(body["operationId"], json!(operation), "{body}");
    assert_eq!(body["action"], json!("allocate"), "{body}");
    assert_eq!(body["status"], json!("ready"), "分配必须 ready：{body}");
    Prepared {
        session: body["sessionId"].as_str().expect("sessionId").to_owned(),
        receipt: body["receipt"].clone(),
        runtime_epoch: body["runtimeEpoch"]
            .as_str()
            .expect("runtimeEpoch")
            .to_owned(),
        generation: body["generation"].as_str().expect("generation").to_owned(),
    }
}

/// `session.preparation.restore`：返回完整结果 body（调用方按 status 断言）。
async fn restore(client: &mut SyncClient, prepared: &Prepared) -> Value {
    let operation = uuid_text();
    client.step("session.preparation.restore");
    client
        .send(
            "session.preparation",
            json!({
                "operationId": operation,
                "action": "restore",
                "sessionId": prepared.session,
                "receipt": prepared.receipt,
                "knownRuntimeEpoch": prepared.runtime_epoch,
                "knownGeneration": prepared.generation,
            }),
        )
        .await;
    let body = client.expect_type("session.preparation.result").await["body"].clone();
    assert_eq!(body["operationId"], json!(operation), "{body}");
    assert_eq!(body["action"], json!("restore"), "{body}");
    body
}

/// `session.start` 的 `kind: initial` 载荷。
fn initial_payload(prepared: &Prepared, content: &[Value], submission: Option<&str>) -> Value {
    let mut payload = json!({
        "kind": "initial",
        "receipt": prepared.receipt,
        "runtimeEpoch": prepared.runtime_epoch,
        "generation": prepared.generation,
        "content": content,
    });
    if let Some(submission) = submission {
        payload["submissionId"] = json!(submission);
    }
    payload
}

/// 派发一条首条（`kind: initial`）。
async fn start_initial(
    client: &mut SyncClient,
    request: &str,
    prepared: &Prepared,
    content: &[Value],
    submission: Option<&str>,
) {
    client.step("session.start（initial）");
    client
        .command(
            request,
            "session.start",
            Some(&prepared.session),
            initial_payload(prepared, content, submission),
        )
        .await;
}

/// 释放脚本端点暂存的事件并落盘（真实路径上这一步由 `agent-host` 驱动）。
async fn settle_dispatch(host: &HostNode, session: &str, marker: &str) -> usize {
    let released = host.parked().release(marker);
    host.broker()
        .pump(&acp_core::model::SessionId::new(session).expect("session id"))
        .await
        .expect("事件落盘");
    released
}

/// `session.read` 的消息页（正文读面）。
async fn read_messages(client: &mut SyncClient, session: &str) -> Vec<Value> {
    let request = uuid_text();
    client.step("session.read（messages）");
    client
        .command(
            &request,
            "session.read",
            Some(session),
            json!({ "include": ["messages"] }),
        )
        .await;
    let result = client.expect_command_result(&request).await;
    assert_eq!(result["status"], json!("completed"), "{result}");
    result["result"]["resources"]["messages"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// 一条消息里的文本块拼接（消息正文的消费面形状）。
fn message_text(message: &Value) -> String {
    message["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|block| block["type"] == json!("text"))
        .filter_map(|block| block["text"].as_str())
        .collect()
}

/// 用户消息（`role = "user"`）的 `(messageId, 正文)`。
fn user_messages(messages: &[Value]) -> Vec<(String, String)> {
    messages
        .iter()
        .filter(|message| message["role"] == json!("user"))
        .map(|message| {
            (
                message["messageId"].as_str().expect("messageId").to_owned(),
                message_text(message),
            )
        })
        .collect()
}

/// `command.status`（需要协商 `core.command-status.v1`）。
async fn command_status(client: &mut SyncClient, target: &str) -> Value {
    let request = uuid_text();
    client.step("command.status");
    client
        .command(
            &request,
            "command.status",
            None,
            json!({ "targetRequestId": target }),
        )
        .await;
    let result = client.expect_command_result(&request).await;
    assert_eq!(result["status"], json!("completed"), "{result}");
    result["result"].clone()
}

/// 派发一条普通输入并等到终态；返回首次响应与终态响应。
async fn prompt_until_terminal(
    host: &HostNode,
    client: &mut SyncClient,
    session: &str,
    request: &str,
    text: &str,
) -> (Value, Value) {
    client.step("session.prompt");
    client
        .command(
            request,
            "session.prompt",
            Some(session),
            json!({ "content": [{ "type": "text", "text": text }] }),
        )
        .await;
    let accepted = client.expect_command_result(request).await;
    assert_eq!(accepted["status"], json!("accepted"), "{accepted}");
    let released = settle_dispatch(host, session, text).await;
    assert_eq!(released, 2, "一次派发应释放 agent delta 与 turn.completed");
    let terminal = client.expect_command_result(request).await;
    (accepted, terminal)
}

/// 用准备流程建立一条正式会话（首条无回显），返回会话 ID。
async fn prepared_session(host: &HostNode, client: &mut SyncClient, first_text: &str) -> String {
    let prepared = allocate(client, &workspace_path(host)).await;
    let request = uuid_text();
    start_initial(
        client,
        &request,
        &prepared,
        &[json!({ "type": "text", "text": first_text })],
        None,
    )
    .await;
    assert_eq!(
        client.expect_command_result(&request).await["status"],
        json!("accepted")
    );
    let released = settle_dispatch(host, &prepared.session, first_text).await;
    assert_eq!(released, 2, "首条派发应释放 agent delta 与 turn.completed");
    assert_eq!(
        client.expect_command_result(&request).await["status"],
        json!("completed")
    );
    prepared.session
}

/// 直接读库连接（同一 data_dir 的原始 SQL；只用于独立复核持久事实）。
async fn raw_connection(host: &HostNode) -> sqlx::SqliteConnection {
    let config = storage_sqlite::migrate::StorageConfig::new(host.data_dir());
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(config.database_path())
        .create_if_missing(false)
        .busy_timeout(Duration::from_secs(10));
    sqlx::Connection::connect_with(&options)
        .await
        .expect("打开同一 data_dir 的原始 SQL 连接")
}

/// `owned_message` 里该会话 `channel = 'user'` 的行数。
async fn stored_user_messages(host: &HostNode, session: &str) -> i64 {
    let mut connection = raw_connection(host).await;
    let count: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM owned_message WHERE session_id = ?1 AND channel = 'user'",
    )
    .bind(session)
    .fetch_one(&mut connection)
    .await
    .expect("统计持久用户消息");
    drop(connection);
    count
}

/// `owned_event` 里 `user.message.delta` 的 `payload_json`（按入库顺序）。
async fn stored_user_echo_payloads(host: &HostNode, session: &str) -> Vec<String> {
    let mut connection = raw_connection(host).await;
    let rows = sqlx::query_as::<_, (String,)>(
        "SELECT payload_json FROM owned_event \
         WHERE session_id = ?1 AND event_type = 'user.message.delta' ORDER BY global_sequence ASC",
    )
    .bind(session)
    .fetch_all(&mut connection)
    .await
    .expect("读 owned_event");
    drop(connection);
    rows.into_iter().map(|(payload,)| payload).collect()
}

fn sha256_digest(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    base64url(&Sha256::digest(bytes))
}

fn device_subject(device: &Device) -> Value {
    json!({ "kind": "device", "id": device.id() })
}

// ---------------------------------------------------------------------------------------------
// E1：首条无回显 + 两条后续输入，重启与第二个设备都读到完整正文
// ---------------------------------------------------------------------------------------------

/// 变红点：①后续输入没有持久正文（只靠 Agent 回显）⇒ 重启/第二个设备读不到；②重启清掉正文或换
/// 消息身份；③新设备读到的是另一批消息或空页。
#[tokio::test]
async fn e1_inputs_survive_restart_and_a_second_device_reads_them() {
    let root = support::TempRoot::new("reliability-e1");
    let lifecycle =
        acp_core::use_cases::LifecycleConfig::new(1_800_000, 50).expect("默认生命周期配置合法");
    let host = HostNode::start_in_root(root.path(), HOST, lifecycle, SEED).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect(&host, &device).await;

    // 首条：准备流程 + 无 Agent 回显（脚本默认不回显）。
    let session = prepared_session(&host, &mut client, "E1 first message").await;
    // 两条后续输入各自独立受理，也都没有回显。
    let first_request = uuid_text();
    let (_, first_terminal) = prompt_until_terminal(
        &host,
        &mut client,
        &session,
        &first_request,
        "E1 follow-up one",
    )
    .await;
    assert_eq!(
        first_terminal["status"],
        json!("completed"),
        "{first_terminal}"
    );
    let second_request = uuid_text();
    let (_, second_terminal) = prompt_until_terminal(
        &host,
        &mut client,
        &session,
        &second_request,
        "E1 follow-up two",
    )
    .await;
    assert_eq!(
        second_terminal["status"],
        json!("completed"),
        "{second_terminal}"
    );
    assert_eq!(host.script().prompt_calls(), 3, "首条 + 两条后续各派发一次");

    let before_restart = user_messages(&read_messages(&mut client, &session).await);
    assert_eq!(
        before_restart
            .iter()
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>(),
        vec!["E1 first message", "E1 follow-up one", "E1 follow-up two"],
        "三条用户输入必须按顺序完整可读：{before_restart:?}"
    );
    assert_eq!(
        stored_user_messages(&host, &session).await,
        3,
        "持久用户消息恰好三条"
    );

    // —— 停/重启 Daemon（同一 data_dir、同一节点身份）——
    let dispatches_before_restart = host.script().prompt_calls();
    assert_eq!(dispatches_before_restart, 3, "首条 + 两条后续各派发一次");
    host.stop().await;
    let host = HostNode::start_in_root(root.path(), HOST, lifecycle, SEED).await;
    let (recovered, converged) = host.startup_sequence().await;
    assert_eq!(recovered, 0, "全部命令都已终结，启动恢复不应结算任何命令");
    assert_eq!(
        converged, 1,
        "收敛会检查已终态的首条记录并原样返回终态，不写库、不重发"
    );
    assert_eq!(
        host.script().prompt_calls(),
        0,
        "重启后新的后端实例不得收到任何自动重发"
    );
    let mut client = connect(&host, &device).await;
    let after_restart = user_messages(&read_messages(&mut client, &session).await);
    assert_eq!(
        after_restart, before_restart,
        "重启前后正文与身份必须逐字一致：{after_restart:?}"
    );

    // —— 第二个设备（第二客户端）读同一批正文 ——
    let second_device = Device::new();
    pair_and_approve(&host, &second_device).await;
    let mut second_client = connect(&host, &second_device).await;
    let from_second = user_messages(&read_messages(&mut second_client, &session).await);
    assert_eq!(
        from_second, before_restart,
        "第二个设备必须读到同一批正文与身份：{from_second:?}"
    );

    host.stop().await;
}

// ---------------------------------------------------------------------------------------------
// E2：两次“继续”相互独立；同一 requestId 重试只一次
// ---------------------------------------------------------------------------------------------

/// 变红点：①按相同文字抵消或合并 ⇒ 只有一条用户消息；②重试再派发一次 ⇒ `prompt_calls` 增长或出现
/// 第二条持久消息；③两条“继续”共用 turnId ⇒ 客户端无法按身份接替。
#[tokio::test]
async fn e2_two_continues_are_independent_and_a_retried_request_dispatches_once() {
    let host = HostNode::start("reliability-e2", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect(&host, &device).await;
    let session = prepared_session(&host, &mut client, "E2 seed message").await;

    let text = "继续";
    let first_request = uuid_text();
    client.step("session.prompt（第一次继续）");
    client
        .command(
            &first_request,
            "session.prompt",
            Some(&session),
            json!({ "content": [{ "type": "text", "text": text }] }),
        )
        .await;
    let first_accepted = client.expect_command_result(&first_request).await;
    assert_eq!(
        first_accepted["status"],
        json!("accepted"),
        "{first_accepted}"
    );
    let first_turn = first_accepted["result"]["turnId"]
        .as_str()
        .expect("accepted 必须带 turnId")
        .to_owned();
    settle_dispatch(&host, &session, text).await;
    assert_eq!(
        client.expect_command_result(&first_request).await["status"],
        json!("completed")
    );

    let second_request = uuid_text();
    client.step("session.prompt（第二次继续）");
    client
        .command(
            &second_request,
            "session.prompt",
            Some(&session),
            json!({ "content": [{ "type": "text", "text": text }] }),
        )
        .await;
    let second_accepted = client.expect_command_result(&second_request).await;
    assert_eq!(
        second_accepted["status"],
        json!("accepted"),
        "{second_accepted}"
    );
    let second_turn = second_accepted["result"]["turnId"]
        .as_str()
        .expect("accepted 必须带 turnId")
        .to_owned();
    assert_ne!(
        first_turn, second_turn,
        "两次有意发送必须各自得到独立 turnId"
    );
    settle_dispatch(&host, &session, text).await;
    assert_eq!(
        client.expect_command_result(&second_request).await["status"],
        json!("completed")
    );

    let users = user_messages(&read_messages(&mut client, &session).await);
    assert_eq!(
        users.iter().filter(|(_, body)| body == text).count(),
        2,
        "两次“继续”必须是两条独立用户消息：{users:?}"
    );
    let continues: Vec<&(String, String)> = users.iter().filter(|(_, body)| body == text).collect();
    assert_ne!(
        continues[0].0, continues[1].0,
        "相同文字必须保留不同消息身份：{users:?}"
    );
    assert_eq!(
        stored_user_messages(&host, &session).await,
        3,
        "种子消息 + 两条继续 = 三条持久用户输入"
    );

    // 同一 requestId 重试：只回放，不第二次派发、不写第二条消息。
    let dispatches_before = host.script().prompt_calls();
    client.step("重试第二次继续（同一 requestId）");
    client
        .command(
            &second_request,
            "session.prompt",
            Some(&session),
            json!({ "content": [{ "type": "text", "text": text }] }),
        )
        .await;
    let replay = client.expect_command_result(&second_request).await;
    assert!(
        replay["status"] == json!("completed") || replay["status"] == json!("accepted"),
        "重试必须回执原命令：{replay}"
    );
    assert_eq!(
        host.script().prompt_calls(),
        dispatches_before,
        "重试不得再次派发"
    );
    assert_eq!(host.parked().release(text), 0, "重试不得再产生一份后端事件");
    let after_retry = user_messages(&read_messages(&mut client, &session).await);
    assert_eq!(after_retry, users, "重试不得增删正文：{after_retry:?}");

    host.stop().await;
}

// ---------------------------------------------------------------------------------------------
// E3：保存失败零副作用；保存后派发失败正文仍在
// ---------------------------------------------------------------------------------------------

/// 变红点：①事务失败仍派发/仍广播 ⇒ `prompt_calls` 非零或事件库水位前进；②失败留下半条消息；
/// ③派发失败把已保存输入清掉；④保存失败后同一 requestId 不能重试。
#[tokio::test]
async fn e3_save_failure_is_side_effect_free_and_saved_input_survives_dispatch_failure() {
    let host = HostNode::start("reliability-e3", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect(&host, &device).await;
    let session = prepared_session(&host, &mut client, "E3 seeded message").await;

    // ① 保存事务失败：注入存储故障（只布防下一次普通输入受理）。
    let watermark = host.head().await;
    host.session_store_faults().fail_next_prompt_commit();
    let failed_request = uuid_text();
    client.step("session.prompt（保存事务失败）");
    client
        .command(
            &failed_request,
            "session.prompt",
            Some(&session),
            json!({ "content": [{ "type": "text", "text": "E3 not saved" }] }),
        )
        .await;
    let rejected = client.expect_command_result(&failed_request).await;
    assert_eq!(rejected["status"], json!("rejected"), "{rejected}");
    assert_eq!(
        rejected["error"]["code"],
        json!("internal.unavailable"),
        "保存失败必须是明确的不可用错误：{rejected}"
    );
    assert_eq!(
        host.session_store_faults().rejected(),
        1,
        "故障注入必须真的命中受理提交"
    );
    assert_eq!(
        host.script().prompt_calls(),
        1,
        "保存失败不得派发（只有首条那一次）"
    );
    let after_failure = host.head().await;
    assert_eq!(
        after_failure.global_sequence, watermark.global_sequence,
        "保存失败不得写入或广播任何事件"
    );
    let users = user_messages(&read_messages(&mut client, &session).await);
    assert_eq!(users.len(), 1, "失败不得留下半条消息：{users:?}");
    assert_eq!(users[0].1, "E3 seeded message");

    // 同一 requestId 可修正重试：没有幂等记录，重试正常受理并只保存一次。
    client.step("重试同一 requestId");
    client
        .command(
            &failed_request,
            "session.prompt",
            Some(&session),
            json!({ "content": [{ "type": "text", "text": "E3 retried input" }] }),
        )
        .await;
    let retried = client.expect_command_result(&failed_request).await;
    assert_eq!(retried["status"], json!("accepted"), "{retried}");
    settle_dispatch(&host, &session, "E3 retried input").await;
    assert_eq!(
        client.expect_command_result(&failed_request).await["status"],
        json!("completed")
    );
    let users = user_messages(&read_messages(&mut client, &session).await);
    assert_eq!(
        users
            .iter()
            .filter(|(_, text)| text == "E3 retried input")
            .count(),
        1,
        "重试只保存一条：{users:?}"
    );

    // ② 保存后派发失败：正文已落盘，失败与输入分开呈现。
    host.script()
        .fail_prepared_prompt(acp_core::model::UnavailableKind::IoError);
    let dispatch_failed_request = uuid_text();
    client.step("session.prompt（派发失败）");
    client
        .command(
            &dispatch_failed_request,
            "session.prompt",
            Some(&session),
            json!({ "content": [{ "type": "text", "text": "E3 saved then failed" }] }),
        )
        .await;
    let first = client.expect_command_result(&dispatch_failed_request).await;
    let terminal = if first["status"] == json!("accepted") {
        client.expect_command_result(&dispatch_failed_request).await
    } else {
        first
    };
    assert_eq!(
        terminal["status"],
        json!("failed"),
        "派发失败必须终结为 failed：{terminal}"
    );
    assert!(
        terminal["result"]["turnId"].as_str().is_some(),
        "快速失败只回终态时仍必须带 turnId：{terminal}"
    );
    let users = user_messages(&read_messages(&mut client, &session).await);
    assert!(
        users.iter().any(|(_, text)| text == "E3 saved then failed"),
        "派发失败后已保存输入必须仍在：{users:?}"
    );
    assert_eq!(
        stored_user_messages(&host, &session).await,
        3,
        "种子 + 重试 + 派发失败 = 三条持久输入（失败不删正文）"
    );

    host.stop().await;
}

/// E3（重启边界）：一条已受理但未终态的普通输入，重启后收敛为 `uncertain`，且绝不自动重发。
///
/// 变红点：①重启把 accepted 当 completed；②启动恢复重新派发（`prompt_calls` 非零）；③恢复后正文
/// 读不到。
#[tokio::test]
async fn e3_restart_marks_accepted_input_uncertain_without_redispatch() {
    let root = support::TempRoot::new("reliability-e3-restart");
    let lifecycle =
        acp_core::use_cases::LifecycleConfig::new(1_800_000, 50).expect("默认生命周期配置合法");
    let host = HostNode::start_in_root(root.path(), HOST, lifecycle, SEED).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect(&host, &device).await;
    let session = prepared_session(&host, &mut client, "E3 restart seed").await;

    let request = uuid_text();
    let text = "E3 uncertain after restart";
    client.step("session.prompt（受理后进程退出）");
    client
        .command(
            &request,
            "session.prompt",
            Some(&session),
            json!({ "content": [{ "type": "text", "text": text }] }),
        )
        .await;
    let accepted = client.expect_command_result(&request).await;
    assert_eq!(accepted["status"], json!("accepted"), "{accepted}");
    // 不释放脚本事件：真实崩溃窗口里 Agent 可能已收到、也可能没收到。
    host.stop().await;

    let host = HostNode::start_in_root(root.path(), HOST, lifecycle, SEED).await;
    let (recovered, _converged) = host.startup_sequence().await;
    assert_eq!(recovered, 1, "那条仍为 accepted 的命令必须被收敛一次");
    let mut client = connect(&host, &device).await;

    let status = command_status(&mut client, &request).await;
    assert_eq!(
        status["state"],
        json!("uncertain"),
        "结果不确定必须保持 uncertain：{status}"
    );
    assert_eq!(
        status["error"]["code"],
        json!("command.uncertain"),
        "{status}"
    );
    assert_eq!(
        host.script().prompt_calls(),
        0,
        "重启不得自动重发已保存输入"
    );
    let users = user_messages(&read_messages(&mut client, &session).await);
    assert!(
        users.iter().any(|(_, body)| body == text),
        "重启后正文必须仍可读：{users:?}"
    );

    host.stop().await;
}

// ---------------------------------------------------------------------------------------------
// E5：附件与文字同事务；上传/封存失败不派发；异内容回显保留
// ---------------------------------------------------------------------------------------------

/// 变红点：①附件与正文分两次写 ⇒ 派发前读不到 `attachment_ref`；②清单未封存也被受理；③派发失败后
/// 已保存的文字+附件消失；④异内容回显被当成重复吞掉。
#[tokio::test]
async fn e5_text_and_attachment_are_saved_together_and_kept_when_dispatch_fails() {
    let host = HostNode::start("reliability-e5-attachment", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect(&host, &device).await;
    let session = prepared_session(&host, &mut client, "E5 attachment seed").await;

    let bytes = b"reliability attachment payload";
    let transfer = uuid_text();
    let submission = uuid_text();

    // ① 封存失败（错误摘要）：不得产生可引用的附件，后续引用必须被拒且不派发。
    let mut attachment_client = connect_attachments(&host, &device).await;
    attachment_client.step("attachment.begin（正式会话）");
    attachment_client
        .send(
            "attachment.begin",
            json!({
                "transferId": transfer,
                "sessionId": session,
                "subject": device_subject(&device),
                "totalLength": bytes.len().to_string(),
                "chunkSize": 32768,
                "mediaType": "text/plain",
                "fileName": "notes.txt",
            }),
        )
        .await;
    attachment_client.step("attachment.chunk");
    attachment_client
        .send(
            "attachment.chunk",
            json!({
                "transferId": transfer,
                "offset": "0",
                "data": base64url(bytes),
            }),
        )
        .await;
    let ack = attachment_client.expect_type("attachment.chunk_ack").await["body"].clone();
    assert_eq!(ack["nextOffset"], json!(bytes.len().to_string()), "{ack}");
    attachment_client.step("attachment.seal（错误摘要）");
    attachment_client
        .send(
            "attachment.seal",
            json!({
                "transferId": transfer,
                "length": bytes.len().to_string(),
                "sha256": sha256_digest(b"wrong bytes"),
            }),
        )
        .await;
    let failed_seal = attachment_client.expect_type("attachment.error").await["body"].clone();
    assert_eq!(
        failed_seal["code"],
        json!("attachment.digest_mismatch"),
        "{failed_seal}"
    );

    let dispatches_before = host.script().prompt_calls();
    let rejected_request = uuid_text();
    client.step("session.prompt（引用未封存清单）");
    client
        .command(
            &rejected_request,
            "session.prompt",
            Some(&session),
            json!({
                "content": [
                    { "type": "text", "text": "E5 should not dispatch" },
                    {
                        "type": "attachment_ref",
                        "attachmentId": uuid_text(),
                        "mediaType": "text/plain",
                        "byteLength": bytes.len().to_string(),
                        "sha256": sha256_digest(bytes),
                    }
                ],
                "submissionId": submission,
            }),
        )
        .await;
    let rejected = client.expect_command_result(&rejected_request).await;
    assert_eq!(rejected["status"], json!("rejected"), "{rejected}");
    assert_eq!(
        host.script().prompt_calls(),
        dispatches_before,
        "未封存清单不得派发"
    );

    // ② 正常封存 + 清单封存，正文与附件在**同一**受理事务里落盘。
    let sealed = {
        // 同一传输在失败摘要之后不能继续写入；重新开始一次干净上传。
        let transfer = uuid_text();
        attachment_client.step("attachment.begin（重试上传）");
        attachment_client
            .send(
                "attachment.begin",
                json!({
                    "transferId": transfer,
                    "sessionId": session,
                    "subject": device_subject(&device),
                    "totalLength": bytes.len().to_string(),
                    "chunkSize": 32768,
                    "mediaType": "text/plain",
                    "fileName": "notes.txt",
                }),
            )
            .await;
        attachment_client.step("attachment.chunk（重试上传）");
        attachment_client
            .send(
                "attachment.chunk",
                json!({
                    "transferId": transfer,
                    "offset": "0",
                    "data": base64url(bytes),
                }),
            )
            .await;
        attachment_client.expect_type("attachment.chunk_ack").await;
        attachment_client.step("attachment.seal");
        attachment_client
            .send(
                "attachment.seal",
                json!({
                    "transferId": transfer,
                    "length": bytes.len().to_string(),
                    "sha256": sha256_digest(bytes),
                }),
            )
            .await;
        let sealed = attachment_client.expect_type("attachment.sealed").await["body"].clone();
        assert_eq!(sealed["sha256"], json!(sha256_digest(bytes)), "{sealed}");

        attachment_client.step("attachment.manifest_begin");
        attachment_client
            .send(
                "attachment.manifest_begin",
                json!({
                    "submissionId": submission,
                    "sessionId": session,
                    "subject": device_subject(&device),
                    "attachmentCount": "1",
                }),
            )
            .await;
        attachment_client.step("attachment.manifest_page（final）");
        attachment_client
            .send(
                "attachment.manifest_page",
                json!({
                    "submissionId": submission,
                    "transferIds": [transfer],
                    "final": true,
                }),
            )
            .await;
        let digest =
            server::attachment_transfer::manifest_digest_of([transfer.as_str()].into_iter());
        attachment_client.step("attachment.manifest_sealed");
        attachment_client
            .send(
                "attachment.manifest_sealed",
                json!({
                    "submissionId": submission,
                    "attachmentCount": "1",
                    "manifestDigest": digest.as_str(),
                    "sealedAt": host.clock().now().as_str(),
                }),
            )
            .await;
        let body = attachment_client
            .expect_type("attachment.manifest_sealed")
            .await["body"]
            .clone();
        assert_eq!(body["attachmentCount"], json!("1"), "{body}");
        sealed
    };
    let attachment_id = sealed["attachmentId"]
        .as_str()
        .expect("attachmentId")
        .to_owned();

    // 派发失败（可证明未发出）：受理已提交，正文与附件都在。
    host.script()
        .fail_prepared_prompt(acp_core::model::UnavailableKind::SessionNotEstablished);
    let mixed_request = uuid_text();
    client.step("session.prompt（文字 + 附件，派发失败）");
    client
        .command(
            &mixed_request,
            "session.prompt",
            Some(&session),
            json!({
                "content": [
                    { "type": "text", "text": "E5 mixed input" },
                    {
                        "type": "attachment_ref",
                        "attachmentId": attachment_id,
                        "mediaType": "text/plain",
                        "byteLength": bytes.len().to_string(),
                        "sha256": sha256_digest(bytes),
                    }
                ],
                "submissionId": submission,
            }),
        )
        .await;
    let first = client.expect_command_result(&mixed_request).await;
    let terminal = if first["status"] == json!("accepted") {
        client.expect_command_result(&mixed_request).await
    } else {
        first
    };
    assert_eq!(terminal["status"], json!("failed"), "{terminal}");
    assert!(
        terminal["result"]["turnId"].as_str().is_some(),
        "快速失败只回终态时仍必须带 turnId：{terminal}"
    );

    // 已保存的输入（文字 + 附件引用）在派发失败后仍完整可读。
    let messages = read_messages(&mut client, &session).await;
    let mixed = messages
        .iter()
        .find(|message| message_text(message) == "E5 mixed input")
        .unwrap_or_else(|| panic!("派发失败后已保存输入必须仍可读：{messages:?}"));
    let blocks = mixed["content"].as_array().expect("content 数组");
    let attachment_blocks: Vec<&Value> = blocks
        .iter()
        .filter(|block| block["type"] == json!("file_ref"))
        .collect();
    assert_eq!(
        attachment_blocks.len(),
        1,
        "同一条已保存消息必须带着附件引用：{mixed}"
    );
    assert_eq!(
        attachment_blocks[0]["attachmentId"],
        json!(attachment_id),
        "附件引用必须指向本次封存的附件：{mixed}"
    );
    assert_eq!(
        stored_user_messages(&host, &session).await,
        2,
        "种子 + 混合输入 = 两条持久用户消息"
    );

    host.stop().await;
}

/// E5：Agent 回显了**不同内容**时保留为独立事件，不覆盖已保存正文，也不被当成重复吞掉。
///
/// 变红点：回显只按文本/时间近似关联 ⇒ 异内容被吞（事件库里没有那条 `user.message.delta`）或覆盖
/// 已保存正文；把重复计数提前加一 ⇒ `duplicate_echoes` 非零。
#[tokio::test]
async fn e5_differing_agent_echo_is_kept_as_its_own_event() {
    let host = HostNode::start("reliability-e5-echo", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect(&host, &device).await;
    let session = prepared_session(&host, &mut client, "E5 echo seed").await;

    let echo_message = uuid_text();
    host.script()
        .echo_plain_prompt_with_text(&echo_message, "Agent 改写后的内容");
    let request = uuid_text();
    client.step("session.prompt（异内容回显）");
    client
        .command(
            &request,
            "session.prompt",
            Some(&session),
            json!({ "content": [{ "type": "text", "text": "E5 original input" }] }),
        )
        .await;
    let accepted = client.expect_command_result(&request).await;
    assert_eq!(accepted["status"], json!("accepted"), "{accepted}");
    let released = settle_dispatch(&host, &session, "E5 original input").await;
    assert_eq!(
        released, 3,
        "异内容回显必须真的产出第三条事件（agent delta + 回显 + turn.completed）"
    );
    let terminal = client.expect_command_result(&request).await;
    assert_eq!(terminal["status"], json!("completed"), "{terminal}");
    assert_eq!(
        host.script().duplicate_echoes(),
        0,
        "异内容回显不得被判定为重复"
    );

    // 权威正文仍是用户原文；异内容回显只作为独立事件存在。
    let users = user_messages(&read_messages(&mut client, &session).await);
    assert_eq!(
        users
            .iter()
            .filter(|(_, text)| text == "E5 original input")
            .count(),
        1,
        "已保存正文不得被回显改写：{users:?}"
    );
    assert!(
        users.iter().all(|(_, text)| text != "Agent 改写后的内容"),
        "异内容回显不得冒充已保存的用户消息：{users:?}"
    );
    let echoes = stored_user_echo_payloads(&host, &session).await;
    assert!(
        echoes.iter().any(
            |payload| payload.contains("Agent 改写后的内容") && payload.contains(&echo_message)
        ),
        "异内容回显得作为独立事件保留（含它自己的 messageId）：{echoes:?}"
    );

    host.stop().await;
}

// ---------------------------------------------------------------------------------------------
// E6：准备 allocate → restore 同 ID、同一资源代、同一临时端点
// ---------------------------------------------------------------------------------------------

/// 变红点：restore 走「重新分配」⇒ 会话 ID/资源代变化；同代 restore 重建端点 ⇒ 建立计数增长；
/// 两条连接各自占一个准备 ⇒ 目录条目 > 1。
#[tokio::test]
async fn e6_restore_reuses_the_same_preparation_id_and_endpoint() {
    let host = HostNode::start("reliability-e6", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect(&host, &device).await;
    let prepared = allocate(&mut client, &workspace_path(&host)).await;
    assert_eq!(
        host.preparation_endpoints().establish_calls(),
        1,
        "allocate 建立一次临时端点"
    );
    assert_eq!(
        host.preparations().directory().occupied(),
        1,
        "同一标签页只占一个准备"
    );

    // 刷新 / 重新挂载后的 restore：同一 ID，不分配第二个。
    let restored = restore(&mut client, &prepared).await;
    assert_eq!(restored["status"], json!("ready"), "{restored}");
    assert_eq!(
        restored["sessionId"],
        json!(prepared.session),
        "restore 必须回到同一准备 ID：{restored}"
    );
    assert_eq!(
        restored["generation"],
        json!(prepared.generation),
        "同一代 restore 不得换代：{restored}"
    );
    assert_eq!(
        host.preparation_endpoints().establish_calls(),
        1,
        "同代 restore 不得重建临时端点"
    );
    assert_eq!(
        host.preparations().directory().occupied(),
        1,
        "restore 不得产生第二个准备条目"
    );

    // 幂等：再 restore 一次仍是同 ID/同代，准备数量不变。
    let again = restore(&mut client, &prepared).await;
    assert_eq!(again["status"], json!("ready"), "{again}");
    assert_eq!(again["sessionId"], json!(prepared.session), "{again}");
    assert_eq!(
        host.preparations().directory().occupied(),
        1,
        "重复 restore 不得产生第二个准备"
    );

    host.stop().await;
}

// ---------------------------------------------------------------------------------------------
// E7：两个连接并发 restore 不产生第二个准备
// ---------------------------------------------------------------------------------------------

/// 变红点：并发恢复各建一个端点/各分配一个 ID ⇒ 建立计数或目录条目增长；两者返回不同 ID。
#[tokio::test]
async fn e7_concurrent_restores_do_not_create_a_second_preparation() {
    let host = HostNode::start("reliability-e7", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect(&host, &device).await;
    let prepared = allocate(&mut client, &workspace_path(&host)).await;

    // 同一设备只保留一条连接（新连接会替换旧的），因此用同一条连接并发发出两次 restore：
    // 两次请求在读取任何结果前都已入站，服务端仍必须合并到同一个准备。
    let first_operation = uuid_text();
    let second_operation = uuid_text();
    let restore_body = |operation: &str, prepared: &Prepared| {
        json!({
            "operationId": operation,
            "action": "restore",
            "sessionId": prepared.session,
            "receipt": prepared.receipt,
            "knownRuntimeEpoch": prepared.runtime_epoch,
            "knownGeneration": prepared.generation,
        })
    };
    client.step("session.preparation.restore（两条并发请求）");
    client
        .send(
            "session.preparation",
            restore_body(&first_operation, &prepared),
        )
        .await;
    client
        .send(
            "session.preparation",
            restore_body(&second_operation, &prepared),
        )
        .await;

    let first = client.expect_type("session.preparation.result").await["body"].clone();
    let second_result = client.expect_type("session.preparation.result").await["body"].clone();
    assert_eq!(first["operationId"], json!(first_operation), "{first}");
    assert_eq!(
        second_result["operationId"],
        json!(second_operation),
        "{second_result}"
    );
    assert_eq!(first["status"], json!("ready"), "{first}");
    assert_eq!(second_result["status"], json!("ready"), "{second_result}");
    assert_eq!(
        first["sessionId"],
        json!(prepared.session),
        "第一次恢复必须回到原准备：{first}"
    );
    assert_eq!(
        second_result["sessionId"],
        json!(prepared.session),
        "并发恢复必须回到同一准备，不得分配第二个 ID：{second_result}"
    );
    assert_eq!(
        first["generation"], second_result["generation"],
        "并发恢复必须使用同一资源代：{first} / {second_result}"
    );
    assert_eq!(
        host.preparation_endpoints().establish_calls(),
        1,
        "并发恢复不得建立第二个临时端点"
    );
    assert_eq!(
        host.preparations().directory().occupied(),
        1,
        "并发恢复不得产生第二个准备条目"
    );

    host.stop().await;
}
