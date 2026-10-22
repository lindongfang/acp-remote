//! 准备式新建会话的**跨层**可重跑套件（`web-session-preparation` 任务 8.1；PV8）。
//!
//! 驱动方式：脚本化 fake Sync 客户端经**真实 loopback listener** 连接真实组合根形状——真实 SQLite
//! （migration、首条事务、暂存目录）、真实 `identity_auth::Authority`（准备凭据是真实验签的）、真实
//! 运行期准备目录与容量账、真实命令/订阅/附件路由。刻意的替身只有两处，且都在 Agent 边界：
//!
//! - 临时 Agent 端点（`agent-host` 的 `TemporaryEndpoint`）→ [`support::sync::ScriptedPreparationEndpoints`]：
//!   同形地复用/换代/转正，事件 sink 取真实 `Broker::sink`；真实 Codex 见 8.2；
//! - 首次提交端口的**故障注入装饰器**（只布防下一次调用，平时逐个委托给真实 `SqliteStore`）。
//!
//! 每条用例按场景编号命名，断言消费者可见的事实：wire 结果的状态与字段、持久行、派发计数、正式读取
//! 的正文。`E1/E5/E6/E8/E9`、相同文本第二条与附件 TTL 在这里都可重跑；真实浏览器/Codex 属于 8.2。
//!
//! 与 `sync_commands_e2e.rs` 同款：配对与握手的脚手架各自成 crate，因此本文件持有一份等价的真实路径
//! 实现（`device.pair.begin → qr lookup → claim → confirm → status`）。

mod support;

use std::time::Duration;

use serde_json::{Value, json};
use support::sync::{DeviceKey, HostNode, SyncClient, base64url, nonce_text, uuid_text};

/// 本机宿主名（`daemon.public_origin` 的 authority；`Host` 头与 `Origin` 头都按它判）。
const HOST: &str = "session-preparation.example.invalid";
/// 设备名（`deviceName`）。
const DEVICE_NAME: &str = "Preparation Phone";
/// 脚本化 Agent selector（`[agent] codex` 与快照 `agents` 都按它断言）。
const AGENT: &str = "codex";
/// 重启用例的固定节点身份种子（`EphemeralKeystore` 每实例只在进程内，固定后才是「同一节点的重启」）。
const SEED: [u8; 32] = [0x5a; 32];

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
        server::sync::local_lifecycle::LOCAL_LIFECYCLE_FEATURE,
        server::sync::attachments::ATTACHMENT_FEATURE,
        server::sync::preparation::PREPARATION_FEATURE,
    ]
}

// ---------------------------------------------------------------------------------------------
// 配对与握手（真实 HTTP 端点 + 真实四消息握手；与 `sync_commands_e2e` 同一套真实路径）
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
            client_nonce: nonce_text(0x61),
            request_nonce: nonce_text(0x71),
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

/// 独立附件连接（`/sync/v1/attachments`）：同一条设备身份、同一套四消息握手，业务阶段只有
/// `attachment.*` 帧。
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
// 准备/首条/读取的驱动件
// ---------------------------------------------------------------------------------------------

/// `allocate` 的 `ready` 结果（会话 ID、凭据与运行期代）。
#[derive(Clone)]
struct Prepared {
    session: String,
    receipt: Value,
    runtime_epoch: String,
    generation: String,
}

/// 一个真实存在、可规范化的测试工作目录（路径来源的准备不需要登记目录）。
fn workspace_path(host: &HostNode) -> String {
    let path = host.data_dir().join("preparation-workspace");
    std::fs::create_dir_all(&path).expect("测试工作目录");
    std::fs::canonicalize(&path)
        .expect("规范化工作目录")
        .to_string_lossy()
        .into_owned()
}

/// `session.preparation.allocate`（真实控制帧）：要求 `ready` 且返回真实端点报告的可用模式。
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
    let modes = body["modes"].as_array().expect("ready 必须给出可用模式");
    assert!(
        modes.iter().any(|mode| mode["modeId"] == json!("plan")),
        "可用模式必须来自真实端点而不是伪造默认值：{body}"
    );
    assert!(
        body["configOptions"].is_array(),
        "ready 必须给出配置项（这里真实端点为 0 项）：{body}"
    );
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

/// 派发一条 `session.preparation.release`（不读取结果；竞争用例用缓冲读口）。
async fn send_release(client: &mut SyncClient, prepared: &Prepared, operation: &str) {
    client.step("session.preparation.release");
    client
        .send(
            "session.preparation",
            json!({
                "operationId": operation,
                "action": "release",
                "sessionId": prepared.session,
                "receipt": prepared.receipt,
                "runtimeEpoch": prepared.runtime_epoch,
                "generation": prepared.generation,
            }),
        )
        .await;
}

async fn release_preparation(client: &mut SyncClient, prepared: &Prepared) -> Value {
    let operation = uuid_text();
    send_release(client, prepared, &operation).await;
    let body = client.expect_type("session.preparation.result").await["body"].clone();
    assert_eq!(body["operationId"], json!(operation), "{body}");
    assert_eq!(body["action"], json!("release"), "{body}");
    body
}

/// 结果缓冲读口：并发请求（`session.start` + `session.preparation.release`）的结果顺序不确定，
/// 断言必须按请求身份取结果，不能假设到达顺序（也不能把先到的另一条结果丢掉）。
#[derive(Default)]
struct Inbox {
    pending: Vec<Value>,
}

impl Inbox {
    async fn matching(
        &mut self,
        client: &mut SyncClient,
        accept: impl Fn(&Value) -> bool,
    ) -> Value {
        if let Some(index) = self.pending.iter().position(|message| accept(message)) {
            return self.pending.remove(index);
        }
        loop {
            let message = client.take().await;
            if accept(&message) {
                return message;
            }
            self.pending.push(message);
        }
    }

    async fn command_result(&mut self, client: &mut SyncClient, request: &str) -> Value {
        let message = self
            .matching(client, |message| {
                message["type"] == json!("command.result")
                    && message["body"]["requestId"] == json!(request)
            })
            .await;
        message["body"].clone()
    }

    async fn preparation_result(&mut self, client: &mut SyncClient) -> Value {
        let message = self
            .matching(client, |message| {
                message["type"] == json!("session.preparation.result")
            })
            .await;
        message["body"].clone()
    }
}

/// `session.start` 的 `kind: initial` 载荷（正文 + 可选附件清单）。
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

/// `command.status`（需要协商 `core.command-status.v1`；首条投影还需要准备 feature）。
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

/// `session.read` 的消息页（正文读面；首条不需要 Agent 回显）。
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

// ---------------------------------------------------------------------------------------------
// 独立读库（与 `sync_commands_e2e` 同一手法：同一 data_dir 的原始 SQL 连接）
// ---------------------------------------------------------------------------------------------

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

/// 一次会话在准备/首条路径上**可能**产生的 owned 行数。
#[derive(Debug, Default, PartialEq, Eq)]
struct OwnedRows {
    sessions: i64,
    first_submissions: i64,
    commands: i64,
    turns: i64,
    user_messages: i64,
    attachment_links: i64,
    media_refs: i64,
}

async fn owned_rows(host: &HostNode, session: &str) -> OwnedRows {
    let mut connection = raw_connection(host).await;
    async fn one(connection: &mut sqlx::SqliteConnection, sql: &str, session: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(sql)
            .bind(session)
            .fetch_one(&mut *connection)
            .await
            .unwrap_or_else(|error| panic!("统计 `{sql}` 失败：{error}"))
    }
    let rows = OwnedRows {
        sessions: one(
            &mut connection,
            "SELECT COUNT(*) FROM owned_session WHERE session_id = ?1",
            session,
        )
        .await,
        first_submissions: one(
            &mut connection,
            "SELECT COUNT(*) FROM owned_first_submission WHERE session_id = ?1",
            session,
        )
        .await,
        commands: one(
            &mut connection,
            "SELECT COUNT(*) FROM owned_command WHERE session_id = ?1",
            session,
        )
        .await,
        turns: one(
            &mut connection,
            "SELECT COUNT(*) FROM owned_turn WHERE session_id = ?1",
            session,
        )
        .await,
        user_messages: one(
            &mut connection,
            "SELECT COUNT(*) FROM owned_message WHERE session_id = ?1 AND channel = 'user'",
            session,
        )
        .await,
        attachment_links: one(
            &mut connection,
            "SELECT COUNT(*) FROM owned_attachment_link WHERE session_id = ?1",
            session,
        )
        .await,
        media_refs: one(
            &mut connection,
            "SELECT COUNT(*) FROM owned_media_ref WHERE message_id IN \
             (SELECT message_id FROM owned_message WHERE session_id = ?1)",
            session,
        )
        .await,
    };
    drop(connection);
    rows
}

/// `owned_first_submission` 的关键列：`(first_submission_id, user_message_id, first_request_id,
/// current_request_id, stage, terminal_outcome)`。
async fn first_submission_row(host: &HostNode, session: &str) -> Value {
    let mut connection = raw_connection(host).await;
    let row: Option<(String, String, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT first_submission_id, user_message_id, first_request_id, current_request_id, \
             stage, terminal_outcome FROM owned_first_submission WHERE session_id = ?1",
    )
    .bind(session)
    .fetch_optional(&mut connection)
    .await
    .expect("读 owned_first_submission");
    drop(connection);
    let (first, message, first_request, current_request, stage, terminal) =
        row.unwrap_or_else(|| panic!("会话 {session} 没有首条关联行"));
    json!({
        "firstSubmissionId": first,
        "userMessageId": message,
        "firstRequestId": first_request,
        "currentRequestId": current_request,
        "stage": stage,
        "terminalOutcome": terminal,
    })
}

/// 整库的目录行数（`allocate` 的路径来源不登记目录）。
async fn directory_rows(host: &HostNode) -> i64 {
    let mut connection = raw_connection(host).await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM owned_directory")
        .fetch_one(&mut connection)
        .await
        .expect("统计 owned_directory");
    drop(connection);
    count
}

// ---------------------------------------------------------------------------------------------
// 附件准备目标（D6）：上传、封存、清单与跨连接重绑
// ---------------------------------------------------------------------------------------------

fn sha256_digest(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    base64url(&Sha256::digest(bytes))
}

fn attachment_target(prepared: &Prepared) -> Value {
    json!({
        "kind": "preparation",
        "sessionId": prepared.session,
        "receipt": prepared.receipt,
        "runtimeEpoch": prepared.runtime_epoch,
        "generation": prepared.generation,
    })
}

fn device_subject(device: &Device) -> Value {
    json!({ "kind": "device", "id": device.id() })
}

async fn begin_transfer(
    client: &mut SyncClient,
    _prepared: &Prepared,
    device: &Device,
    transfer: &str,
    total_length: usize,
    target: &Value,
) {
    client.step("attachment.begin（准备目标）");
    client
        .send(
            "attachment.begin",
            json!({
                "transferId": transfer,
                "target": target,
                "subject": device_subject(device),
                "totalLength": total_length.to_string(),
                "chunkSize": 32768,
                "mediaType": "text/plain",
                "fileName": "notes.txt",
            }),
        )
        .await;
}

async fn send_chunk(client: &mut SyncClient, transfer: &str, offset: usize, bytes: &[u8]) -> Value {
    client.step("attachment.chunk");
    client
        .send(
            "attachment.chunk",
            json!({
                "transferId": transfer,
                "offset": offset.to_string(),
                "data": base64url(bytes),
            }),
        )
        .await;
    client.expect_type("attachment.chunk_ack").await["body"].clone()
}

async fn seal_transfer(client: &mut SyncClient, transfer: &str, bytes: &[u8]) -> Value {
    client.step("attachment.seal");
    client
        .send(
            "attachment.seal",
            json!({
                "transferId": transfer,
                "length": bytes.len().to_string(),
                "sha256": sha256_digest(bytes),
            }),
        )
        .await;
    client.expect_type("attachment.sealed").await["body"].clone()
}

/// 请求一次传输状态。`attachment.status` 的 body 是闭合形状（D6.1「body 不变」）：客户端发来的
/// `state`/`receivedLength`/`nextOffset`/`sealedSha256` 同样必填，只有 `transferId` 是不够的——
/// 服务端只把它当请求，从不用它自报的进度改写事实。
async fn request_status(client: &mut SyncClient, label: &str, transfer: &str) -> Value {
    client.step(label);
    client
        .send(
            "attachment.status",
            json!({
                "transferId": transfer,
                "state": "receiving",
                "receivedLength": "0",
                "nextOffset": "0",
                "sealedSha256": null,
            }),
        )
        .await;
    client.expect_type("attachment.status").await["body"].clone()
}

/// 一份完整的准备上传：begin → chunk → seal。返回 sealed body（`attachmentId` 即封存事实）。
async fn upload_preparation(
    client: &mut SyncClient,
    prepared: &Prepared,
    device: &Device,
    transfer: &str,
    bytes: &[u8],
) -> Value {
    begin_transfer(
        client,
        prepared,
        device,
        transfer,
        bytes.len(),
        &attachment_target(prepared),
    )
    .await;
    let ack = send_chunk(client, transfer, 0, bytes).await;
    assert_eq!(ack["transferId"], json!(transfer), "{ack}");
    assert_eq!(ack["nextOffset"], json!(bytes.len().to_string()), "{ack}");
    assert_eq!(ack["duplicate"], json!(false), "{ack}");
    seal_transfer(client, transfer, bytes).await
}

/// 封存一份准备清单（`manifest_begin → manifest_page(final) → manifest_sealed`）。
async fn seal_manifest(
    client: &mut SyncClient,
    host: &HostNode,
    prepared: &Prepared,
    device: &Device,
    submission: &str,
    transfers: &[String],
) -> Value {
    client.step("attachment.manifest_begin");
    client
        .send(
            "attachment.manifest_begin",
            json!({
                "submissionId": submission,
                "target": attachment_target(prepared),
                "subject": device_subject(device),
                "attachmentCount": transfers.len().to_string(),
            }),
        )
        .await;
    client.step("attachment.manifest_page（final）");
    client
        .send(
            "attachment.manifest_page",
            json!({
                "submissionId": submission,
                "transferIds": transfers,
                "final": true,
            }),
        )
        .await;
    let digest =
        server::attachment_transfer::manifest_digest_of(transfers.iter().map(String::as_str));
    client.step("attachment.manifest_sealed");
    client
        .send(
            "attachment.manifest_sealed",
            json!({
                "submissionId": submission,
                "attachmentCount": transfers.len().to_string(),
                "manifestDigest": digest.as_str(),
                "sealedAt": host.clock().now().as_str(),
            }),
        )
        .await;
    let body = client.expect_type("attachment.manifest_sealed").await["body"].clone();
    assert_eq!(body["submissionId"], json!(submission), "{body}");
    assert_eq!(
        body["attachmentCount"],
        json!(transfers.len().to_string()),
        "{body}"
    );
    body
}

/// 期待一条附件错误帧并返回它的 body。
async fn expect_attachment_error(client: &mut SyncClient, code: &str) -> Value {
    let body = client.expect_type("attachment.error").await["body"].clone();
    assert_eq!(body["code"], json!(code), "附件错误码必须是 {code}：{body}");
    body
}

// ---------------------------------------------------------------------------------------------
// E1：准备不登记，释放交还容量
// ---------------------------------------------------------------------------------------------

/// E1：真实控制帧分配 ID（目录/其他设备/SQLite 都没有 owned 准备行），容量真的被预占，释放后回补。
///
/// 变红点：分配若写 `owned_session`/`owned_directory`/`owned_command` 任意一行 ⇒ 计数非零；容量若只
/// 存在内存索引而不走同一本容量账 ⇒ 第二次分配不会被拒；释放若不交还名额 ⇒ 第三次分配仍被拒。
#[tokio::test]
async fn e1_preparation_is_not_persisted_and_release_reclaims_capacity() {
    let lifecycle = acp_core::use_cases::LifecycleConfig::new(1_800_000, 1).expect("容量 1 合法");
    let host = HostNode::start_with_lifecycle("prep-e1", HOST, lifecycle).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect(&host, &device).await;
    let workspace = workspace_path(&host);

    let first = allocate(&mut client, &workspace).await;
    assert_eq!(
        owned_rows(&host, &first.session).await,
        OwnedRows::default(),
        "准备期不得出现任何 owned 行（会话/首条/命令/turn/消息/附件）"
    );
    assert_eq!(
        directory_rows(&host).await,
        0,
        "路径来源的分配不得登记目录行"
    );

    // 容量只有 1：第二个分配必须被明确拒绝，且**不**产生新 ID 的 side effect。
    let operation = uuid_text();
    client.step("session.preparation.allocate（容量已满）");
    client
        .send(
            "session.preparation",
            json!({
                "operationId": operation,
                "action": "allocate",
                "agentId": AGENT,
                "workspace": { "workspacePath": workspace },
                "tabNonce": nonce_text(0x42),
            }),
        )
        .await;
    let error = client.expect_type("error").await["body"].clone();
    assert_eq!(
        error["code"],
        json!("session.capacity_exceeded"),
        "第二个准备必须命中同一本容量账：{error}"
    );

    // 释放：`released` 且名额回补（再分配成功）。
    let released = release_preparation(&mut client, &first).await;
    assert_eq!(released["status"], json!("released"), "{released}");
    assert_eq!(released["sessionId"], json!(first.session), "{released}");
    let third = allocate(&mut client, &workspace).await;
    assert_ne!(third.session, first.session, "分配必须给出新的 Owner ID");
    assert_eq!(
        owned_rows(&host, &third.session).await,
        OwnedRows::default(),
        "第二个准备同样不得登记任何 owned 行"
    );
    let released = release_preparation(&mut client, &third).await;
    assert_eq!(released["status"], json!("released"), "{released}");

    host.stop().await;
}

// ---------------------------------------------------------------------------------------------
// E5：首条先存再发、无回显可读、回显不重复、相同文本第二条是新身份
// ---------------------------------------------------------------------------------------------

/// E5：受理事务落下会话/完整首条/命令/turn/首条关联；正文在**派发事件之前**就能读；Agent 回显即使
/// 到达也不会造出第二条用户消息。
///
/// 变红点：首条正文只由 Agent 回显承载 ⇒ 释放事件前的 `session.read` 读不到正文；回显没有按
/// `associate_user_echo` 判定 ⇒ 用户消息数变成 2；受理先派发后落盘 ⇒ 读库断言在 `accepted` 时失败。
#[tokio::test]
async fn e5_first_submission_saves_before_dispatch_and_is_readable_without_echo() {
    let host = HostNode::start("prep-e5-save", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect(&host, &device).await;
    let prepared = allocate(&mut client, &workspace_path(&host)).await;
    let message = "E5 first message without agent echo";
    let request = uuid_text();
    start_initial(
        &mut client,
        &request,
        &prepared,
        &[json!({ "type": "text", "text": message })],
        None,
    )
    .await;

    let accepted = client.expect_command_result(&request).await;
    assert_eq!(accepted["status"], json!("accepted"), "{accepted}");
    assert_eq!(accepted["result"], Value::Null, "accepted 不得带业务结果");

    // 事务已提交：全部六类行同批出现（不是「先建会话再补附件/正文」）。
    let rows = owned_rows(&host, &prepared.session).await;
    assert_eq!(rows.sessions, 1, "正式会话行必须已落盘");
    assert_eq!(rows.first_submissions, 1, "首条关联行必须已落盘");
    assert_eq!(rows.commands, 1, "受理命令行必须已落盘");
    assert_eq!(rows.turns, 1, "首条 turn 必须已落盘");
    assert_eq!(rows.user_messages, 1, "完整首条用户消息必须已落盘");
    let row = first_submission_row(&host, &prepared.session).await;
    assert_eq!(row["firstRequestId"], json!(request), "{row}");
    assert_eq!(row["currentRequestId"], json!(request), "{row}");
    assert_ne!(
        row["stage"],
        json!("terminal"),
        "事件未释放前不得是终态：{row}"
    );

    // 无 Agent 回显也能读到完整正文。
    let messages = read_messages(&mut client, &prepared.session).await;
    let users = user_messages(&messages);
    assert_eq!(users.len(), 1, "首条用户消息必须恰好一条：{messages:?}");
    assert_eq!(users[0].1, message, "正文必须逐字可读：{messages:?}");
    assert_eq!(
        json!(users[0].0),
        row["userMessageId"],
        "正文身份必须是持久 user_message_id"
    );

    // 释放脚本端点的事件（真实路径上由 agent-host 产生）并收口终态。
    let released = settle_dispatch(&host, &prepared.session, message).await;
    assert!(released >= 2, "脚本应释放 agent delta 与 turn.completed");
    let terminal = client.expect_command_result(&request).await;
    assert_eq!(terminal["status"], json!("completed"), "{terminal}");
    assert_eq!(
        terminal["result"]["stage"],
        json!("terminal"),
        "completed 的结果必须是六字段首条投影：{terminal}"
    );
    assert_eq!(
        terminal["result"]["firstRequestId"],
        json!(request),
        "首条请求身份不得被改写：{terminal}"
    );
    assert_eq!(
        terminal["result"]["currentRequestId"],
        json!(request),
        "首个尝试就是当前尝试：{terminal}"
    );
    assert_eq!(
        terminal["result"]["canContinue"],
        json!(false),
        "成功终态不可继续：{terminal}"
    );

    host.stop().await;
}

/// E5：Agent 回显与已保存首条可证明重复时，不新增第二条用户消息。
///
/// 变红点：回显被无条件落成 `user.message.delta` ⇒ `session.read` 出现两条同文用户消息；关联规则被
/// 改成按「相同文本」猜测 ⇒ 第二次有意发送相同文字的用例（下一条）会被吞并。
#[tokio::test]
async fn e5_agent_echo_does_not_add_a_second_user_message() {
    let host = HostNode::start("prep-e5-echo", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect(&host, &device).await;
    let prepared = allocate(&mut client, &workspace_path(&host)).await;
    let message = "E5 echoed message";
    // Agent 用自己的消息标识回显同一条正文：关联只看派发上下文与内容摘要。
    host.script().echo_first_attempt(&uuid_text());
    let request = uuid_text();
    start_initial(
        &mut client,
        &request,
        &prepared,
        &[json!({ "type": "text", "text": message })],
        None,
    )
    .await;
    let accepted = client.expect_command_result(&request).await;
    assert_eq!(accepted["status"], json!("accepted"), "{accepted}");
    assert_eq!(
        host.script().duplicate_echoes(),
        1,
        "回显必须真的经过关联规则并判定为重复"
    );

    let released = settle_dispatch(&host, &prepared.session, message).await;
    assert_eq!(
        released, 2,
        "重复回显不得产出事件（应只有 agent delta 与 turn.completed 两条）"
    );
    let terminal = client.expect_command_result(&request).await;
    assert_eq!(terminal["status"], json!("completed"), "{terminal}");

    let messages = read_messages(&mut client, &prepared.session).await;
    let users = user_messages(&messages);
    assert_eq!(
        users.len(),
        1,
        "重复回显不得新增第二条用户消息：{messages:?}"
    );
    assert_eq!(users[0].1, message);
    assert_eq!(
        owned_rows(&host, &prepared.session).await.user_messages,
        1,
        "持久用户消息仍然只有一份"
    );

    host.stop().await;
}

/// E5/相同文本第二条：第二次有意发送相同文字绑定**新的**消息身份，不被回显规则吞并。
///
/// 变红点：按相同文本归并 ⇒ 第二次发送后用户消息仍只有一条；把 Agent 回显的 messageId 当作首条身份
/// ⇒ 第一条消息的 `messageId` 被改写；第二次发送追加第二条首条关联 ⇒ 首条行数变化。
#[tokio::test]
async fn e5_same_text_second_message_gets_a_new_identity() {
    let host = HostNode::start("prep-e5-second", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect(&host, &device).await;
    let prepared = allocate(&mut client, &workspace_path(&host)).await;
    let message = "same text twice";
    let first_request = uuid_text();
    start_initial(
        &mut client,
        &first_request,
        &prepared,
        &[json!({ "type": "text", "text": message })],
        None,
    )
    .await;
    assert_eq!(
        client.expect_command_result(&first_request).await["status"],
        json!("accepted")
    );
    settle_dispatch(&host, &prepared.session, message).await;
    assert_eq!(
        client.expect_command_result(&first_request).await["status"],
        json!("completed")
    );
    let first_row = first_submission_row(&host, &prepared.session).await;

    // 第二次有意发送同一段文字：普通 prompt（没有首条派发上下文），Agent 给新消息身份。
    let second_message = uuid_text();
    host.script().echo_plain_prompt(&second_message);
    let second_request = uuid_text();
    client.step("session.prompt（相同文本）");
    client
        .command(
            &second_request,
            "session.prompt",
            Some(&prepared.session),
            json!({ "content": [{ "type": "text", "text": message }] }),
        )
        .await;
    let accepted = client.expect_command_result(&second_request).await;
    assert_eq!(accepted["status"], json!("accepted"), "{accepted}");
    let released = settle_dispatch(&host, &prepared.session, message).await;
    assert_eq!(
        released, 3,
        "普通 prompt 的 Agent 回显必须是一条独立事件（agent delta + 回显 + turn.completed）"
    );
    let terminal = client.expect_command_result(&second_request).await;
    assert_eq!(terminal["status"], json!("completed"), "{terminal}");

    let messages = read_messages(&mut client, &prepared.session).await;
    let users = user_messages(&messages);
    assert_eq!(users.len(), 2, "两条用户消息必须同时存在：{messages:?}");
    let first_identity = first_row["userMessageId"].as_str().expect("userMessageId");
    let first = users
        .iter()
        .find(|(identity, _)| identity == first_identity)
        .unwrap_or_else(|| panic!("第一条消息身份必须仍在：{users:?}"));
    let second = users
        .iter()
        .find(|(identity, _)| identity == &second_message)
        .unwrap_or_else(|| panic!("第二条消息必须是新身份：{users:?}"));
    assert_ne!(first.0, second.0, "相同文字必须有不同的消息身份：{users:?}");
    assert_eq!(first.1, message, "{users:?}");
    assert_eq!(second.1, message, "{users:?}");
    let after = first_submission_row(&host, &prepared.session).await;
    assert_eq!(
        after, first_row,
        "第二条消息不得改动首条关联（first/user/current request 与阶段都保持）"
    );
    assert_eq!(
        owned_rows(&host, &prepared.session).await.user_messages,
        1,
        "第二条消息不新增首条边界的持久用户消息"
    );

    host.stop().await;
}

// ---------------------------------------------------------------------------------------------
// E6：保存/派发故障围栏
// ---------------------------------------------------------------------------------------------

/// E6（保存前）：受理事务失败 ⇒ 零行、零派发；restore 取得新端点后同 ID 可重试并正常完成。
///
/// 变红点：事务失败留下会话/首条/命令任一行 ⇒ 计数非零；失败后仍然派发 ⇒ `prompt_calls` 非零；
/// 失败后端点未交还 ⇒ 释放计数为 0 或 restore 失败；把同一 requestId 的重试当成已受理/已拒绝回放 ⇒
/// 重试不是 `accepted`、或出现两条受理行。
#[tokio::test]
async fn e6_save_failure_leaves_no_rows_and_no_dispatch() {
    let host = HostNode::start("prep-e6-save", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect(&host, &device).await;
    let prepared = allocate(&mut client, &workspace_path(&host)).await;
    let message = "E6 save failure";
    host.first_submission_faults().fail_next_commit();
    let request = uuid_text();
    start_initial(
        &mut client,
        &request,
        &prepared,
        &[json!({ "type": "text", "text": message })],
        None,
    )
    .await;
    let rejected = client.expect_command_result(&request).await;
    assert_eq!(rejected["status"], json!("rejected"), "{rejected}");
    assert_eq!(
        rejected["error"]["code"],
        json!("internal.unavailable"),
        "{rejected}"
    );

    assert_eq!(
        owned_rows(&host, &prepared.session).await,
        OwnedRows::default(),
        "保存前失败不得留下任何 owned 行（事务必须整体回滚）"
    );
    assert_eq!(host.first_submission_faults().commit_calls(), 1);
    assert_eq!(host.script().prompt_calls(), 0, "保存失败绝不得派发首条");
    assert_eq!(
        host.preparation_endpoints().release_calls(),
        1,
        "失败路径必须释放刚转正的临时端点"
    );

    // 同页同 ID 重试：保存失败已整体回滚（没有受理行），因此同一 requestId 必须重新走完整受理，
    // 而不是回放那次 rejected。restore 重新建立端点后不需要换号。
    let restored = restore(&mut client, &prepared).await;
    assert_eq!(restored["status"], json!("ready"), "{restored}");
    let prepared = Prepared {
        session: prepared.session.clone(),
        receipt: restored["receipt"].clone(),
        runtime_epoch: restored["runtimeEpoch"]
            .as_str()
            .expect("runtimeEpoch")
            .to_owned(),
        generation: restored["generation"]
            .as_str()
            .expect("generation")
            .to_owned(),
    };
    start_initial(
        &mut client,
        &request,
        &prepared,
        &[json!({ "type": "text", "text": message })],
        None,
    )
    .await;
    let retried = client.expect_command_result(&request).await;
    assert_eq!(retried["status"], json!("accepted"), "{retried}");
    settle_dispatch(&host, &prepared.session, message).await;
    let completed = client.expect_command_result(&request).await;
    assert_eq!(completed["status"], json!("completed"), "{completed}");
    let rows = owned_rows(&host, &prepared.session).await;
    assert_eq!(rows.sessions, 1);
    assert_eq!(rows.first_submissions, 1);
    assert_eq!(rows.user_messages, 1);
    assert_eq!(
        rows.commands, 1,
        "同一 requestId 的重试只允许一条受理行（失败那次已整体回滚）"
    );
    assert_eq!(host.script().prompt_calls(), 1, "重试恰好派发一次");

    host.stop().await;
}

/// E6（保存后、派发前）+ 重启：正式行与完整正文保留，真实启动序列收敛为「可证明未派发」，不自动派发；
/// 显式继续使用新的 requestId、不追加第二条首条消息；原 requestId 的结果照旧可回放。
///
/// 变红点：启动恢复抢先把首条尝试写成 `uncertain` ⇒ 收敛 `Conflict(Consumed)` 中止启动（`startup_sequence`
/// 直接 panic），或命令状态不是 `failed`/`internal.unavailable`；启动收敛把这条记录当普通 queued prompt
/// 重放 ⇒ 重启后 `prompt_calls` 非零；收敛直接把首条改成成功/已派发 ⇒ 阶段不是
/// `saved_not_dispatched`；继续复用了原 requestId 或新增首条 ⇒ `firstSubmissionId`/`userMessageId` 改变或
/// 用户消息数变成 2；继续的派发围栏清掉原尝试的终态 ⇒ `command.status(firstRequestId)` 不再是 failed 或
/// 终态时间/事件 id 变了。
#[tokio::test]
async fn e6_saved_but_not_dispatched_survives_restart_and_continue_is_explicit() {
    let root = support::TempRoot::new("prep-e6-restart");
    let lifecycle =
        acp_core::use_cases::LifecycleConfig::new(1_800_000, 50).expect("默认生命周期配置合法");
    let host = HostNode::start_in_root(root.path(), HOST, lifecycle, SEED).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect(&host, &device).await;
    let prepared = allocate(&mut client, &workspace_path(&host)).await;
    let message = "E6 crash after save";
    // 派发围栏写入失败：受理已提交、派发未发生（真实崩溃窗口的形状）。
    host.first_submission_faults().fail_next_begin_dispatch();
    let first_request = uuid_text();
    start_initial(
        &mut client,
        &first_request,
        &prepared,
        &[json!({ "type": "text", "text": message })],
        None,
    )
    .await;
    let rejected = client.expect_command_result(&first_request).await;
    assert_eq!(rejected["status"], json!("rejected"), "{rejected}");
    let row = first_submission_row(&host, &prepared.session).await;
    assert_eq!(row["stage"], json!("saved_not_dispatched"), "{row}");
    assert_eq!(row["terminalOutcome"], Value::Null, "{row}");
    assert_eq!(row["firstRequestId"], json!(first_request), "{row}");
    assert_eq!(host.script().prompt_calls(), 0, "围栏失败不得派发");
    let session_id = prepared.session.clone();

    // —— 重启（同一 data_dir、同一节点身份）——
    host.stop().await;
    let host = HostNode::start_in_root(root.path(), HOST, lifecycle, SEED).await;
    let mut client = connect(&host, &device).await;

    // 真实启动序列（`app::daemon` 的顺序）：先恢复未决命令，再收敛首条提交。两步都必须成功——
    // 恢复若把首条尝试抢先写成 `uncertain`，收敛会在同一命令上 Conflict(Consumed) 并中止启动。
    let (recovered, converged) = host.startup_sequence().await;
    assert_eq!(
        recovered, 0,
        "首条尝试归启动收敛独占，启动恢复不得先把它结算掉"
    );
    assert_eq!(converged, 1, "这条待继续的首条必须被收敛一次");
    assert_eq!(
        host.script().prompt_calls(),
        0,
        "重启/收敛都不得自动派发首条"
    );
    assert_eq!(host.script().create_calls(), 0);
    assert_eq!(host.script().resume_calls(), 0);
    let row = first_submission_row(&host, &session_id).await;
    assert_eq!(row["stage"], json!("saved_not_dispatched"), "{row}");
    assert_eq!(row["terminalOutcome"], json!("failed"), "{row}");

    // 投影：可继续、且仍是同一个首条身份。
    let status = command_status(&mut client, &first_request).await;
    assert_eq!(status["state"], json!("failed"), "{status}");
    assert_eq!(
        status["error"]["code"],
        json!("internal.unavailable"),
        "可证明未派发必须是 failed（internal.unavailable），不是 uncertain：{status}"
    );
    assert!(
        status["terminalEventId"].is_string(),
        "failed 终态必须有终态事件：{status}"
    );
    let first_terminal_at = status["terminalAt"].clone();
    let first_terminal_event = status["terminalEventId"].clone();
    let projection = status["preparation"].clone();
    assert_eq!(
        projection["stage"],
        json!("saved_not_dispatched"),
        "{projection}"
    );
    assert_eq!(projection["canContinue"], json!(true), "{projection}");
    assert_eq!(projection["everDispatched"], json!(false), "{projection}");
    assert_eq!(
        projection["firstSubmissionId"], row["firstSubmissionId"],
        "{projection}"
    );

    // 重启后正文仍可读（收敛不清除正文）。
    let messages = read_messages(&mut client, &session_id).await;
    assert_eq!(
        user_messages(&messages),
        vec![(
            row["userMessageId"].as_str().expect("id").to_owned(),
            message.to_owned()
        )],
        "首条正文必须完整保留：{messages:?}"
    );

    // 显式继续：新 requestId + retryOfRequestId。
    let continue_request = uuid_text();
    client.step("session.start（continue）");
    client
        .command(
            &continue_request,
            "session.start",
            Some(&session_id),
            json!({
                "kind": "continue",
                "firstSubmissionId": projection["firstSubmissionId"],
                "retryOfRequestId": first_request,
            }),
        )
        .await;
    let accepted = client.expect_command_result(&continue_request).await;
    assert_eq!(accepted["status"], json!("accepted"), "{accepted}");
    let released = settle_dispatch(&host, &session_id, message).await;
    assert_eq!(
        released, 2,
        "继续只派发一次首条（agent delta + turn.completed）"
    );
    let terminal = client.expect_command_result(&continue_request).await;
    assert_eq!(terminal["status"], json!("completed"), "{terminal}");
    assert_eq!(
        terminal["result"]["currentRequestId"],
        json!(continue_request),
        "{terminal}"
    );
    assert_eq!(
        terminal["result"]["firstRequestId"],
        json!(first_request),
        "首次请求身份不得被继续改写：{terminal}"
    );
    assert_eq!(
        terminal["result"]["firstSubmissionId"], row["firstSubmissionId"],
        "继续不得新建首条身份：{terminal}"
    );
    assert_eq!(
        terminal["result"]["canContinue"],
        json!(false),
        "成功终态不可再继续：{terminal}"
    );

    // 首次尝试的终态仍按原 requestId 回放：继续的派发围栏在 `owned_first_submission` 上清空的是
    // 「当前尝试」的列，`owned_command` 里那条 failed 终态（时间/事件 id/错误）不得被继续覆盖或丢失。
    let replayed = command_status(&mut client, &first_request).await;
    assert_eq!(replayed["state"], json!("failed"), "{replayed}");
    assert_eq!(
        replayed["error"]["code"],
        json!("internal.unavailable"),
        "{replayed}"
    );
    assert_eq!(replayed["terminalAt"], first_terminal_at, "{replayed}");
    assert_eq!(
        replayed["terminalEventId"], first_terminal_event,
        "{replayed}"
    );
    assert_eq!(
        replayed["preparation"]["currentRequestId"],
        json!(continue_request),
        "投影报当前尝试，但原请求的状态/终态仍是它自己的：{replayed}"
    );
    assert_eq!(
        replayed["preparation"]["stage"],
        json!("terminal"),
        "{replayed}"
    );
    assert_eq!(
        replayed["preparation"]["everDispatched"],
        json!(true),
        "{replayed}"
    );

    let after = first_submission_row(&host, &session_id).await;
    assert_eq!(after["userMessageId"], row["userMessageId"], "{after}");
    assert_eq!(
        after["firstSubmissionId"], row["firstSubmissionId"],
        "{after}"
    );
    assert_eq!(after["firstRequestId"], json!(first_request), "{after}");
    assert_eq!(
        after["currentRequestId"],
        json!(continue_request),
        "{after}"
    );
    assert_eq!(
        owned_rows(&host, &session_id).await.user_messages,
        1,
        "继续不得追加第二条首条消息"
    );
    let messages = read_messages(&mut client, &session_id).await;
    assert_eq!(
        user_messages(&messages).len(),
        1,
        "继续后用户消息仍只有一条：{messages:?}"
    );

    host.stop().await;
}

/// E6（派发围栏已提交）：说不清是否已发出 ⇒ 终态 `uncertain`；任何继续都被拒绝为 `command.uncertain`，
/// 重启也不会再派发。
///
/// 变红点：把 `uncertain` 当 `failed` 允许继发 ⇒ 第二次派发；把首条终态折叠成 `completed` ⇒ 投影
/// `stage = terminal` 但 `everDispatched` 为假；重启收敛自动重发 ⇒ `prompt_calls` 非零。
#[tokio::test]
async fn e6_possible_dispatch_is_uncertain_and_continue_is_refused() {
    let root = support::TempRoot::new("prep-e6-uncertain");
    let lifecycle =
        acp_core::use_cases::LifecycleConfig::new(1_800_000, 50).expect("默认生命周期配置合法");
    let host = HostNode::start_in_root(root.path(), HOST, lifecycle, SEED).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect(&host, &device).await;
    let prepared = allocate(&mut client, &workspace_path(&host)).await;
    let message = "E6 possible dispatch";
    host.script()
        .fail_prepared_prompt(acp_core::model::UnavailableKind::IoError);
    let first_request = uuid_text();
    start_initial(
        &mut client,
        &first_request,
        &prepared,
        &[json!({ "type": "text", "text": message })],
        None,
    )
    .await;
    let result = client.expect_command_result(&first_request).await;
    assert_eq!(result["status"], json!("uncertain"), "{result}");
    assert_eq!(host.script().prompt_calls(), 1);
    let session_id = prepared.session.clone();
    let row = first_submission_row(&host, &session_id).await;
    assert_eq!(row["stage"], json!("terminal"), "{row}");
    assert_eq!(row["terminalOutcome"], json!("uncertain"), "{row}");
    assert_eq!(row["currentRequestId"], json!(first_request), "{row}");

    // 继续被拒：不是 failed 的可证明未派发。
    let continue_request = uuid_text();
    let first_submission = row["firstSubmissionId"].as_str().expect("id").to_owned();
    client.step("session.start（continue，已可能派发）");
    client
        .command(
            &continue_request,
            "session.start",
            Some(&session_id),
            json!({
                "kind": "continue",
                "firstSubmissionId": first_submission,
                "retryOfRequestId": first_request,
            }),
        )
        .await;
    let rejected = client.expect_command_result(&continue_request).await;
    assert_eq!(rejected["status"], json!("rejected"), "{rejected}");
    assert_eq!(
        rejected["error"]["code"],
        json!("command.uncertain"),
        "{rejected}"
    );
    assert_eq!(host.script().prompt_calls(), 1, "被拒的继续不得再次派发");

    // 重启：不自动派发，真实启动序列（恢复 → 收敛）保持 uncertain（不是 failed、也不是 completed）。
    host.stop().await;
    let host = HostNode::start_in_root(root.path(), HOST, lifecycle, SEED).await;
    let mut client = connect(&host, &device).await;
    let (recovered, converged) = host.startup_sequence().await;
    assert_eq!(recovered, 0, "首条尝试归启动收敛结算，恢复不得抢先");
    assert_eq!(converged, 1);
    assert_eq!(host.script().prompt_calls(), 0, "重启不得自动重发");
    assert_eq!(host.script().resume_calls(), 0);
    let row = first_submission_row(&host, &session_id).await;
    assert_eq!(row["stage"], json!("terminal"), "{row}");
    assert_eq!(row["terminalOutcome"], json!("uncertain"), "{row}");
    let status = command_status(&mut client, &first_request).await;
    assert_eq!(status["state"], json!("uncertain"), "{status}");
    let projection = status["preparation"].clone();
    assert_eq!(projection["everDispatched"], json!(true), "{projection}");
    assert_eq!(projection["canContinue"], json!(false), "{projection}");
    let messages = read_messages(&mut client, &session_id).await;
    assert_eq!(
        user_messages(&messages).len(),
        1,
        "可能已派发的正文仍然只有一份：{messages:?}"
    );

    host.stop().await;
}

// ---------------------------------------------------------------------------------------------
// E8：准备附件跨连接、映射与 TTL
// ---------------------------------------------------------------------------------------------

/// E8：准备目标上传可跨连接重绑（同偏移、旧连接迟到帧被拒），封存/清单在 `session.start` 前不产生
/// 正式行；转正只认领、不重新上传。
///
/// 变红点：重绑重置偏移 ⇒ `attachment.status.receivedLength` 归零；旧连接迟到帧被接受 ⇒ 没有
/// `attachment.binding_mismatch`；封存/清单提前写 `owned_session`/`owned_attachment` ⇒ 前置计数非零；
/// 转正复制/重算（这里以附件引用与字节读取断言）失败。
#[tokio::test]
async fn e8_preparation_attachments_rebind_and_reject_the_old_connection() {
    let host = HostNode::start("prep-e8-rebind", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect(&host, &device).await;
    let prepared = allocate(&mut client, &workspace_path(&host)).await;
    let bytes = b"preparation attachment payload";
    let transfer = uuid_text();
    let target = attachment_target(&prepared);

    let mut first_conn = connect_attachments(&host, &device).await;
    begin_transfer(
        &mut first_conn,
        &prepared,
        &device,
        &transfer,
        bytes.len(),
        &target,
    )
    .await;
    let half = bytes.len() / 2;
    let ack = send_chunk(&mut first_conn, &transfer, 0, &bytes[..half]).await;
    assert_eq!(ack["nextOffset"], json!(half.to_string()), "{ack}");

    // 断线重绑：新附件连接重新认证，重发带完整 target 的 begin；偏移保留。
    let mut second_conn = connect_attachments(&host, &device).await;
    begin_transfer(
        &mut second_conn,
        &prepared,
        &device,
        &transfer,
        bytes.len(),
        &target,
    )
    .await;
    let status = request_status(&mut second_conn, "attachment.status（重绑后）", &transfer).await;
    assert_eq!(status["state"], json!("receiving"), "{status}");
    assert_eq!(
        status["receivedLength"],
        json!(half.to_string()),
        "重绑不得重置偏移：{status}"
    );

    // 旧连接迟到帧：不得接管传输。
    first_conn.step("attachment.chunk（旧连接迟到）");
    first_conn
        .send(
            "attachment.chunk",
            json!({
                "transferId": transfer,
                "offset": half.to_string(),
                "data": base64url(&bytes[half..]),
            }),
        )
        .await;
    let error = expect_attachment_error(&mut first_conn, "attachment.binding_mismatch").await;
    assert_eq!(error["transferId"], json!(transfer), "{error}");
    // 断线：连接释放它占用的活动预算，但不丢暂存元数据与已接收字节。
    drop(first_conn);
    let mut third_conn = connect_attachments(&host, &device).await;
    begin_transfer(
        &mut third_conn,
        &prepared,
        &device,
        &transfer,
        bytes.len(),
        &target,
    )
    .await;
    let status = request_status(
        &mut third_conn,
        "attachment.status（断线重连后）",
        &transfer,
    )
    .await;
    assert_eq!(
        status["receivedLength"],
        json!(half.to_string()),
        "断线重连同样不得重置偏移：{status}"
    );

    // 新连接补齐剩余块并封存。
    let ack = send_chunk(&mut third_conn, &transfer, half, &bytes[half..]).await;
    assert_eq!(ack["nextOffset"], json!(bytes.len().to_string()), "{ack}");
    let sealed = seal_transfer(&mut third_conn, &transfer, bytes).await;
    assert_eq!(sealed["length"], json!(bytes.len().to_string()), "{sealed}");
    assert_eq!(
        sealed["sha256"],
        json!(sha256_digest(bytes)),
        "封存摘要必须来自流式摘要：{sealed}"
    );
    let attachment_id = sealed["attachmentId"]
        .as_str()
        .expect("attachmentId")
        .to_owned();

    // 清单封存（准备分支）。
    let submission = uuid_text();
    let manifest = seal_manifest(
        &mut third_conn,
        &host,
        &prepared,
        &device,
        &submission,
        &[transfer.clone()],
    )
    .await;
    assert_eq!(manifest["attachmentCount"], json!("1"), "{manifest}");

    // 转正之前：没有正式会话/附件行，字节只在准备暂存里。
    assert_eq!(
        owned_rows(&host, &prepared.session).await,
        OwnedRows::default(),
        "准备附件不得写正式行（含空会话绕过外键）"
    );

    // 首条认领清单：正文 + attachment_ref 同事务转正。
    let message = "E8 attachment first message";
    let content = vec![
        json!({ "type": "text", "text": message }),
        json!({
            "type": "attachment_ref",
            "attachmentId": attachment_id,
            "mediaType": "text/plain",
            "byteLength": bytes.len().to_string(),
            "sha256": sha256_digest(bytes),
        }),
    ];
    let request = uuid_text();
    start_initial(
        &mut client,
        &request,
        &prepared,
        &content,
        Some(&submission),
    )
    .await;
    assert_eq!(
        client.expect_command_result(&request).await["status"],
        json!("accepted")
    );
    let rows = owned_rows(&host, &prepared.session).await;
    assert_eq!(rows.sessions, 1);
    assert_eq!(rows.first_submissions, 1);
    assert_eq!(rows.turns, 1);
    assert_eq!(rows.user_messages, 1);
    assert_eq!(rows.attachment_links, 1, "附件正式引用必须与首条同批出现");
    assert_eq!(rows.media_refs, 1, "消息媒体引用必须与首条同批出现");
    settle_dispatch(&host, &prepared.session, message).await;
    assert_eq!(
        client.expect_command_result(&request).await["status"],
        json!("completed")
    );

    // 正式读取：认领后的字节就是同一份内容（没有第二份文件/第二次上传）。
    use acp_core::ports::AttachmentStore as _;
    let stored = host
        .store()
        .get(&acp_core::model::AttachmentId::new(&attachment_id).expect("attachment id"))
        .await
        .expect("读取正式附件");
    assert_eq!(
        stored.as_deref(),
        Some(bytes.as_slice()),
        "转正后的正式附件必须与上传字节逐字一致"
    );

    host.stop().await;
}

/// E8：准备目标在提交后返回 `already_committed`、运行期/资源代失效返回 `stale`；存储整体重建后
/// 凭据失效返回不可重试的 `preparation.store_rebuilt`。
///
/// 变红点：提交检查排在运行期检查之后 ⇒ 已提交的准备得到 `stale` 而不是 `already_committed`；
/// `stale` 的 details 不给出原因；重建存储后仍以旧 storeEpoch 接受凭据（或把它误判成普通绑定失败）。
#[tokio::test]
async fn e8_attachment_targets_map_committed_stale_and_store_rebuilt() {
    let root = support::TempRoot::new("prep-e8-maps");
    let lifecycle =
        acp_core::use_cases::LifecycleConfig::new(1_800_000, 50).expect("默认生命周期配置合法");
    let host = HostNode::start_in_root(root.path(), HOST, lifecycle, SEED).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect(&host, &device).await;
    let prepared = allocate(&mut client, &workspace_path(&host)).await;
    let mut attachments = connect_attachments(&host, &device).await;

    // 过代（runtime epoch 不匹配）：真实凭证 + 当前运行期的错误提示字段。
    let stale_target = {
        let mut target = attachment_target(&prepared);
        target["runtimeEpoch"] = json!(uuid_text());
        target
    };
    begin_transfer(
        &mut attachments,
        &prepared,
        &device,
        &uuid_text(),
        4,
        &stale_target,
    )
    .await;
    let error = expect_attachment_error(&mut attachments, "preparation.stale").await;
    assert_eq!(
        error["details"]["reason"],
        json!("runtime_epoch_mismatch"),
        "{error}"
    );
    assert_eq!(error["retryable"], json!(true), "{error}");

    // 资源代不匹配。
    let generation_target = {
        let mut target = attachment_target(&prepared);
        target["generation"] = json!("9");
        target
    };
    begin_transfer(
        &mut attachments,
        &prepared,
        &device,
        &uuid_text(),
        4,
        &generation_target,
    )
    .await;
    let error = expect_attachment_error(&mut attachments, "preparation.stale").await;
    assert_eq!(
        error["details"]["reason"],
        json!("generation_mismatch"),
        "{error}"
    );

    // 正常首条提交。
    let bytes = b"committed map";
    let transfer = uuid_text();
    upload_preparation(&mut attachments, &prepared, &device, &transfer, bytes).await;
    let submission = uuid_text();
    seal_manifest(
        &mut attachments,
        &host,
        &prepared,
        &device,
        &submission,
        &[transfer.clone()],
    )
    .await;
    let message = "E8 committed mapping";
    let request = uuid_text();
    start_initial(
        &mut client,
        &request,
        &prepared,
        &[json!({ "type": "text", "text": message })],
        Some(&submission),
    )
    .await;
    assert_eq!(
        client.expect_command_result(&request).await["status"],
        json!("accepted")
    );
    settle_dispatch(&host, &prepared.session, message).await;
    assert_eq!(
        client.expect_command_result(&request).await["status"],
        json!("completed")
    );

    // 已提交：新的准备上传必须得到 `already_committed`（committed 优先于旧代/租约）。
    begin_transfer(
        &mut attachments,
        &prepared,
        &device,
        &uuid_text(),
        4,
        &attachment_target(&prepared),
    )
    .await;
    let error = expect_attachment_error(&mut attachments, "preparation.already_committed").await;
    assert_eq!(
        error["details"]["sessionId"],
        json!(prepared.session),
        "{error}"
    );
    assert_eq!(error["retryable"], json!(false), "{error}");

    // —— 存储整体重建：同一节点身份、同一设备、新的 storeEpoch ——
    let old_receipt = prepared.receipt.clone();
    let old_session = prepared.session.clone();
    let data_dir = host.data_dir();
    let root_path = root.path().to_path_buf();
    host.stop().await;
    remove_store_files(&data_dir);
    let rebuilt = HostNode::start_in_root(&root_path, HOST, lifecycle, SEED).await;
    // 新库没有该设备的信任行：同一设备重新配对（身份相同，旧 receipt 的签名仍然可验）。
    pair_and_approve(&rebuilt, &device).await;
    let mut attachments = connect_attachments(&rebuilt, &device).await;
    let rebuilt_target = json!({
        "kind": "preparation",
        "sessionId": old_session,
        "receipt": old_receipt,
        "runtimeEpoch": uuid_text(),
        "generation": "1",
    });
    reuse_begin(&mut attachments, &device, &uuid_text(), 4, &rebuilt_target).await;
    let error = expect_attachment_error(&mut attachments, "preparation.store_rebuilt").await;
    assert_eq!(error["retryable"], json!(false), "{error}");
    assert!(error["message"].is_string(), "{error}");

    rebuilt.stop().await;
}

/// 删除测试库文件，并**确认真的删掉了**：Windows 上还有句柄时 `remove_file` 会报 sharing violation，
/// 静默忽略会退化成「新进程连上旧库」——`storeEpoch` 没变，用例会得到误导性的 `already_committed`。
/// 句柄释放是异步的，所以这里是有界重试；期限之外失败要带出真实的 OS 错误。
fn remove_store_files(data_dir: &std::path::Path) {
    let paths: Vec<std::path::PathBuf> = ["", "-wal", "-shm"]
        .iter()
        .map(|suffix| {
            std::path::PathBuf::from(format!(
                "{}{suffix}",
                data_dir.join("acp-remote.sqlite3").display()
            ))
        })
        .collect();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let mut failure = None;
        for path in &paths {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => failure = Some((path.clone(), error)),
            }
        }
        let Some((path, error)) = failure else {
            return;
        };
        if std::time::Instant::now() >= deadline {
            panic!("无法删除重建前的存储文件 {path:?}：{error}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// 与 [`begin_transfer`] 相同，但不借用 `Prepared`（重建存储后的 receipt 只能从旧结果取）。
async fn reuse_begin(
    client: &mut SyncClient,
    device: &Device,
    transfer: &str,
    total_length: usize,
    target: &Value,
) {
    client.step("attachment.begin（重建存储后的旧 receipt）");
    client
        .send(
            "attachment.begin",
            json!({
                "transferId": transfer,
                "target": target,
                "subject": device_subject(device),
                "totalLength": total_length.to_string(),
                "chunkSize": 32768,
                "mediaType": "text/plain",
                "fileName": "notes.txt",
            }),
        )
        .await;
}

/// E8/附件 TTL：已由首条引用（保存但未派发）的字节不被暂存清理删除；空闲未认领的上传按窗口过期。
///
/// 变红点：清理把已认领内容当孤儿删除 ⇒ 正式读取拿不到字节；空闲上传永不过期 ⇒ `status` 仍在、
/// 暂存预算永不回补；清理误删活动传输 ⇒ 前置的上传状态消失。
#[tokio::test]
async fn e8_attachment_ttl_keeps_claimed_bytes_and_expires_idle_uploads() {
    let host = HostNode::start("prep-e8-ttl", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect(&host, &device).await;
    // 端点建立时就决定「报告不了 Agent 会话标识」：必须先布防，再进行分配。
    host.script().prepared_without_agent_session();
    let prepared = allocate(&mut client, &workspace_path(&host)).await;
    let mut attachments = connect_attachments(&host, &device).await;

    // 已认领的附件：首条保存成功，但故意不派发（没有 Agent 会话标识 ⇒ 可证明未派发）。
    let bytes = b"claimed but not dispatched";
    let transfer = uuid_text();
    let sealed = upload_preparation(&mut attachments, &prepared, &device, &transfer, bytes).await;
    let attachment_id = sealed["attachmentId"]
        .as_str()
        .expect("attachmentId")
        .to_owned();
    let submission = uuid_text();
    seal_manifest(
        &mut attachments,
        &host,
        &prepared,
        &device,
        &submission,
        &[transfer.clone()],
    )
    .await;
    // 同一节点上的一个空闲、未被任何清单认领的上传（提交之前就存在）。
    let idle_transfer = uuid_text();
    upload_preparation(
        &mut attachments,
        &prepared,
        &device,
        &idle_transfer,
        b"idle",
    )
    .await;
    let message = "E8 TTL claimed";
    let request = uuid_text();
    start_initial(
        &mut client,
        &request,
        &prepared,
        &[json!({ "type": "text", "text": message })],
        Some(&submission),
    )
    .await;
    let result = client.expect_command_result(&request).await;
    assert_eq!(result["status"], json!("failed"), "{result}");
    let row = first_submission_row(&host, &prepared.session).await;
    assert_eq!(row["stage"], json!("saved_not_dispatched"), "{row}");
    assert_eq!(host.script().prompt_calls(), 0);

    // 以「未来」为窗口执行真实的暂存清理：空闲上传过期，已认领内容保留。
    let future = host
        .clock()
        .now()
        .checked_add_millis(48 * 60 * 60 * 1000)
        .expect("48 小时后的时间戳");
    let staging = host
        .preparations()
        .staging()
        .expect("本进程已装配准备暂存端口")
        .clone();
    staging
        .release_orphans(future)
        .await
        .expect("暂存清理必须成功");
    assert!(
        staging
            .status(acp_core::model::AttachmentId::new(&idle_transfer).expect("transfer id"))
            .await
            .expect("读取空闲传输状态")
            .is_none(),
        "空闲未认领的上传必须按 TTL 过期"
    );

    // 已认领的字节仍在正式内容区：正式读取逐字一致。
    use acp_core::ports::AttachmentStore as _;
    let stored = host
        .store()
        .get(&acp_core::model::AttachmentId::new(&attachment_id).expect("attachment id"))
        .await
        .expect("读取正式附件");
    assert_eq!(
        stored.as_deref(),
        Some(bytes.as_slice()),
        "已提交（保存但未派发）的首条引用必须保留字节"
    );
    let rows = owned_rows(&host, &prepared.session).await;
    assert_eq!(rows.attachment_links, 1, "{rows:?}");
    assert_eq!(rows.media_refs, 1, "{rows:?}");

    host.stop().await;
}

// ---------------------------------------------------------------------------------------------
// E9：响应丢失后的 committed 收口与 release/start 竞争
// ---------------------------------------------------------------------------------------------

/// E9：首次提交响应丢失后 restore 返回 `committed`（首个/当前请求关联可核实），同页读取正式会话；
/// 首条受理优先于旧 lease/资源代，release 竞争不回收已提交资源。
///
/// 变红点：restore 对有效 receipt 返回 `ready` 并重建准备 ⇒ 已提交事实不可见；committed 缺
/// first/current request；release 在提交之后仍回收临时端点/容量 ⇒ 正式会话不可用。
#[tokio::test]
async fn e9_lost_start_response_restore_reports_committed_and_release_keeps_it() {
    let host = HostNode::start("prep-e9-committed", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let mut client = connect(&host, &device).await;
    let prepared = allocate(&mut client, &workspace_path(&host)).await;
    let message = "E9 first message";
    let request = uuid_text();
    start_initial(
        &mut client,
        &request,
        &prepared,
        &[json!({ "type": "text", "text": message })],
        None,
    )
    .await;
    // 「响应丢失」：受理与终态都在网络上到达，但客户端没有据此采取动作；随后用凭据在新页面恢复。
    assert_eq!(
        client.expect_command_result(&request).await["status"],
        json!("accepted")
    );
    settle_dispatch(&host, &prepared.session, message).await;
    assert_eq!(
        client.expect_command_result(&request).await["status"],
        json!("completed")
    );

    // 新页面（新连接）用同一 receipt restore：committed 优先于运行期/租约。
    let mut page = connect(&host, &device).await;
    let restored = restore(&mut page, &prepared).await;
    assert_eq!(restored["status"], json!("committed"), "{restored}");
    assert_eq!(restored["firstRequestId"], json!(request), "{restored}");
    assert_eq!(restored["currentRequestId"], json!(request), "{restored}");
    let row = first_submission_row(&host, &prepared.session).await;
    assert_eq!(
        restored["firstSubmissionId"], row["firstSubmissionId"],
        "{restored}"
    );
    assert_eq!(
        restored["firstMessageId"], row["userMessageId"],
        "{restored}"
    );
    assert!(
        restored.get("receipt").is_none() && restored.get("runtimeEpoch").is_none(),
        "committed 不得携带临时代/配置/receipt：{restored}"
    );

    // 同一页读正式会话（无需重建准备、无需重传）。
    let messages = read_messages(&mut page, &prepared.session).await;
    assert_eq!(
        user_messages(&messages),
        vec![(
            row["userMessageId"].as_str().expect("id").to_owned(),
            message.to_owned()
        )],
        "{messages:?}"
    );

    // 第二份准备：start 帧发出后立刻发 release（同一连接、服务端按序处理），提交必须赢过释放。
    let prepared = allocate(&mut page, &workspace_path(&host)).await;
    let message = "E9 race release";
    let request = uuid_text();
    let mut inbox = Inbox::default();
    start_initial(
        &mut page,
        &request,
        &prepared,
        &[json!({ "type": "text", "text": message })],
        None,
    )
    .await;
    let release_operation = uuid_text();
    send_release(&mut page, &prepared, &release_operation).await;
    // 两条请求的结果各自取到（顺序不定）：断言按请求身份取，不假设到达顺序。
    assert_eq!(
        inbox.command_result(&mut page, &request).await["status"],
        json!("accepted"),
        "首条必须已受理（保存先于派发）"
    );
    settle_dispatch(&host, &prepared.session, message).await;
    assert_eq!(
        inbox.command_result(&mut page, &request).await["status"],
        json!("completed")
    );
    let released = inbox.preparation_result(&mut page).await;
    assert_eq!(
        released["operationId"],
        json!(release_operation),
        "{released}"
    );
    assert_eq!(
        released["status"],
        json!("committed"),
        "已提交的首条必须优先于释放：{released}"
    );
    let second_release = uuid_text();
    send_release(&mut page, &prepared, &second_release).await;
    let released = inbox.preparation_result(&mut page).await;
    assert_eq!(released["operationId"], json!(second_release), "{released}");
    assert_eq!(
        released["status"],
        json!("committed"),
        "restore/release 的 committed 分支必须一致：{released}"
    );

    // 资源没有被回收：正式会话仍能继续使用（endpoint 由持久 Agent 身份重建）。
    let second = "E9 after release";
    let prompt_request = uuid_text();
    page.step("session.prompt（release 竞争之后）");
    page.command(
        &prompt_request,
        "session.prompt",
        Some(&prepared.session),
        json!({ "content": [{ "type": "text", "text": second }] }),
    )
    .await;
    assert_eq!(
        inbox.command_result(&mut page, &prompt_request).await["status"],
        json!("accepted")
    );
    let released_events = settle_dispatch(&host, &prepared.session, second).await;
    assert_eq!(released_events, 2, "正式会话的 prompt 必须真的到达后端");
    assert_eq!(
        inbox.command_result(&mut page, &prompt_request).await["status"],
        json!("completed")
    );

    host.stop().await;
}
