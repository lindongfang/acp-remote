//! WP6 跨层验收：本机会话生命周期在**真实组合根 + 真实 SQLite + 真实 loopback listener** 上的
//! 可观察行为（`openspec/changes/session-activation-lifecycle/specs/local-session-lifecycle/spec.md`）。
//!
//! 驱动方式与 `sync_commands_e2e.rs` 同源：脚本化 fake Sync 客户端经真实 WSS 升级走完配对与四消息
//! 握手，命令与查看控制帧都是真实报文；Owner 侧是真实 `SqliteStore`、真实 `identity_auth::Authority`、
//! 组合根自己的装配点（`app::daemon::{net_config, register_node_link_paths,
//! register_sync_paths_with_lifecycle}`）与真实 `LocalSessionLifecycle` 实例。
//!
//! 与既有用例的分工（不复制它们的断言）：
//!
//! - `sync_commands_e2e.rs` 已覆盖 `session.resume` 的 accepted→completed 形状、ready 复用、同
//!   requestId 幂等、并发合并、容量拒绝与查看帧的版本/租期/限流；
//! - `crates/core/src/lifecycle/tests.rs` 已用假时钟覆盖保护判定与资源代状态机本身；
//! - 本文件只补**跨层组合**的五条：
//!   1. 保留期到期 → 逐会话回收 → 历史正文仍可读 → `session.resume` 恢复**同一身份**（同一 sessionId、
//!      没有新建、没有重放旧 prompt、沿用持久化的 ACP 会话标识）；
//!   2. Daemon 重启（同一 `data_dir` 上的新组合根，固定身份种子）：启动收敛不信旧 ready、不自动重派
//!      `uncertain`、恢复后上下文仍可读；
//!   3. 多设备查看租约：只有最后一个查看者离开后才计时，撤销设备立即清理该连接的查看者；
//!   4. 满额时优先回收安全候选（LRU）后才接纳新激活；没有安全候选时明确 `session.capacity_exceeded`；
//!   5. 回收未确认（`Failed`）保持活跃与容量占位，不伪造空闲。
//!
//! 唯一替身是 Agent 后端（脚本化端点，不启动进程），行为脚本在 `support/sync.rs` 的模块注释里声明。

mod support;

use std::time::Duration;

use acp_core::model::{Actor, PortError, SessionId};
use acp_core::ports::ReleaseOutcome;
use serde_json::{Value, json};
use support::TempRoot;
use support::sync::{DeviceKey, HostNode, SyncClient, base64url, nonce_text, uuid_text};

/// 本机宿主名（`daemon.public_origin` 的 authority；`Host` 头与 `Origin` 头都按它判）。
const HOST: &str = "session-lifecycle.example.invalid";
/// 设备名（`deviceName`）。
const DEVICE_NAME: &str = "Lifecycle Phone";
/// 脚本化 agent selector（`support::sync` 的脚本后端按它供数）。
const AGENT: &str = "codex";
/// 本机生命周期 feature（与 `server::sync::local_lifecycle::LOCAL_LIFECYCLE_FEATURE` 同值；集成测试
/// 各自成 crate，因此各自持有字面量）。
const LIFECYCLE_FEATURE: &str = "core.local-session-lifecycle.v1";

/// 设备配对不提交授权集合（`pairing-model` 删除设备权限层），握手只需协商本文件用到的 feature。
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
        LIFECYCLE_FEATURE,
    ]
}

fn config(idle_timeout_ms: u64, max_active_sessions: u32) -> acp_core::use_cases::LifecycleConfig {
    acp_core::use_cases::LifecycleConfig::new(idle_timeout_ms, max_active_sessions)
        .expect("生命周期配置合法")
}

/// 重启前后固定节点身份的种子：`EphemeralKeystore` 只在进程内，不固定种子重启就会换 node id，已批准
/// 设备的信任行随之失效——那就不是「同一节点的重启」。
fn restart_seed() -> [u8; 32] {
    let mut seed = [0u8; 32];
    seed[31] = 9;
    seed
}

// ---------------------------------------------------------------------------------------------
// 配对 / 握手 / 帧体（与 `sync_commands_e2e.rs` 同一批真实路径；集成测试各自成 crate，各自持有一份）
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
            client_nonce: nonce_text(0x71),
            request_nonce: nonce_text(0x72),
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

/// 走完「device.pair.begin → claim → device.pair.confirm → status approved」。
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
            replied.body["pairingRequestId"]
                .as_str()
                .expect("pairingRequestId"),
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

/// 新设备短二维码地址 → `(qrToken, hostKeyFingerprint, canonicalOrigin)`。
fn decode_device_qr(url: &str) -> ([u8; 32], [u8; 32], &str) {
    let (base, fragment) = url.split_once('#').expect("设备二维码地址带 fragment");
    let origin = base.strip_suffix("/pair").expect("短地址路径必须是 /pair");
    let payload = fragment
        .strip_prefix("q2=")
        .expect("设备二维码 fragment 只接受 q2=");
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, payload)
        .expect("q2 payload 是无填充 base64url");
    let mut token = [0_u8; 32];
    let mut fingerprint = [0_u8; 32];
    token.copy_from_slice(&bytes[1..33]);
    fingerprint.copy_from_slice(&bytes[33..]);
    (token, fingerprint, origin)
}

/// 扫码取回（真实 `qr-lookup` 端点）：新短地址 → `(七字段响应, secret)`。
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
    let proof =
        PairingProof::try_from_bytes(&hmac_sha256(secret, &transcript)).expect("32 字节 proof");
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
    let proof =
        PairingProof::try_from_bytes(&hmac_sha256(secret, &transcript)).expect("32 字节 proof");
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

/// 请求指纹（`Digest`：32 字节摘要的无填充 base64url）。
fn digest(label: &str) -> acp_core::model::Digest {
    use base64::Engine as _;
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(label.as_bytes());
    acp_core::model::Digest::new(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize()),
    )
    .expect("32 字节摘要")
}

// ---------------------------------------------------------------------------------------------
// 会话、命令与读面辅助
// ---------------------------------------------------------------------------------------------

async fn connect_lifecycle(host: &HostNode, device: &Device) -> SyncClient {
    let mut client = SyncClient::connect_plain(host.addr(), host.host_name(), &host.origin()).await;
    client.step("四消息握手（lifecycle）");
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

/// 本机建一条 owned 会话并落账（与 `sync_commands_e2e.rs` 的同一入口同形），返回会话标识。
async fn local_session(host: &HostNode) -> String {
    let request_id = acp_core::model::RequestId::new(&uuid_text()).expect("request id 规范");
    let workspace_path = host.data_dir().join("workspace");
    std::fs::create_dir_all(&workspace_path).expect("测试工作目录");
    let canonical = std::fs::canonicalize(&workspace_path).expect("规范化工作目录");
    let workspace =
        acp_core::model::ResolvedWorkspace::at_path(canonical.to_string_lossy().into_owned())
            .expect("绝对路径工作目录");
    let prepared = match host
        .core()
        .create_session(
            &Actor::LocalCli,
            request_id.clone(),
            digest("acp-remote/session-lifecycle/create"),
            acp_core::model::CreateSessionRequest::new(
                acp_core::model::AgentRef::try_new(
                    acp_core::model::AgentId::new(AGENT).expect("agent id"),
                    "Codex",
                )
                .expect("agent ref"),
                Some(workspace),
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
                &Actor::LocalCli,
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

/// 脚本化后端为 `session.create` 分配的 ACP 会话标识（`support/sync.rs` 的可预测取值）。
fn persisted_agent_session_id(session: &str) -> String {
    format!("acp-{}", &session[..8])
}

/// 派发 `session.prompt` 并等它完成：accepted → 释放脚本事件 → pump → completed。
///
/// 返回终态 body。
async fn prompt_to_completion(
    host: &HostNode,
    client: &mut SyncClient,
    session: &str,
    marker: &str,
) -> Value {
    let request = uuid_text();
    client
        .command(
            &request,
            "session.prompt",
            Some(session),
            json!({ "content": [{ "type": "text", "text": marker }] }),
        )
        .await;
    let accepted = client.expect_command_result(&request).await;
    assert_eq!(accepted["status"], json!("accepted"), "{accepted}");
    assert!(
        accepted["result"]["turnId"]
            .as_str()
            .is_some_and(|turn| !turn.is_empty()),
        "session.prompt 的 accepted 必须带稳定 turnId：{accepted}"
    );
    let released = host.parked().release(marker);
    assert!(
        released >= 2,
        "脚本后端应放出 delta 与 turn.completed 两条事件"
    );
    host.broker()
        .pump(&SessionId::new(session).expect("session id"))
        .await
        .expect("事件落盘");
    let terminal = client.expect_command_result(&request).await;
    assert_eq!(terminal["status"], json!("completed"), "{terminal}");
    terminal
}

/// 派发 `session.resume`：accepted → 等终态，返回终态 body。
async fn resume(client: &mut SyncClient, session: &str) -> Value {
    let request = uuid_text();
    client
        .command(&request, "session.resume", Some(session), json!({}))
        .await;
    let accepted = client.expect_command_result(&request).await;
    assert_eq!(accepted["status"], json!("accepted"), "{accepted}");
    client.expect_command_result(&request).await
}

/// `session.list` 的 `completed` 结果（`sessions` 数组）。
async fn list_sessions(client: &mut SyncClient) -> Vec<Value> {
    let request = uuid_text();
    client
        .command(&request, "session.list", None, json!({}))
        .await;
    let result = client.expect_command_result(&request).await;
    assert_eq!(result["status"], json!("completed"), "{result}");
    result["result"]["sessions"]
        .as_array()
        .expect("sessions 数组")
        .clone()
}

/// 该会话在 `session.list` 里的 `localActivity`（协商后的权威投影）。
async fn local_activity(client: &mut SyncClient, session: &str) -> Value {
    list_sessions(client)
        .await
        .into_iter()
        .find(|entry| entry["sessionId"] == json!(session))
        .unwrap_or_else(|| panic!("会话 {session} 必须仍在清单里"))["localActivity"]
        .clone()
}

/// `session.read` 的完成结果（`resources`：`messages` + `history`）。
async fn read_resources(client: &mut SyncClient, session: &str) -> Value {
    let request = uuid_text();
    client
        .command(
            &request,
            "session.read",
            Some(session),
            json!({ "include": ["messages", "history"] }),
        )
        .await;
    let result = client.expect_command_result(&request).await;
    assert_eq!(result["status"], json!("completed"), "{result}");
    assert_eq!(
        result["result"]["sessionId"],
        json!(session),
        "读回的会话标识必须就是请求的那条：{result}"
    );
    result["result"]["resources"].clone()
}

/// 消息数组里的全部正文文本（`content[].text` 拼接）。
fn message_texts(resources: &Value) -> Vec<String> {
    resources["messages"]
        .as_array()
        .expect("messages 数组")
        .iter()
        .map(|message| {
            message["content"]
                .as_array()
                .map(|blocks| {
                    blocks
                        .iter()
                        .filter_map(|block| block["text"].as_str())
                        .collect::<String>()
                })
                .unwrap_or_default()
        })
        .collect()
}

/// 历史条目的 `(eventId, eventType)` 序列（顺序即提交顺序）。
fn history_ids(resources: &Value) -> Vec<(String, String)> {
    resources["history"]
        .as_array()
        .expect("history 数组")
        .iter()
        .map(|entry| {
            (
                entry["eventId"].as_str().expect("eventId").to_owned(),
                entry["eventType"].as_str().expect("eventType").to_owned(),
            )
        })
        .collect()
}

/// 轮询保留期候选，直到该会话出现在其中（保留期用真实时钟，因此只能等；超时即失败）。
async fn wait_for_retention_candidate(
    host: &HostNode,
    session: &str,
) -> acp_core::ports::ReclaimCandidate {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let candidates = host
            .lifecycle()
            .retention_due(&Actor::LocalCli, host.clock().now())
            .await
            .expect("保留候选");
        if let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.session.as_str() == session)
        {
            return candidate.clone();
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "保留期到期后 {session} 必须成为候选，实际候选：{candidates:?}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// 该会话当前是否在保留期候选里。
async fn is_retention_candidate(host: &HostNode, session: &str) -> bool {
    host.lifecycle()
        .retention_due(&Actor::LocalCli, host.clock().now())
        .await
        .expect("保留候选")
        .iter()
        .any(|candidate| candidate.session.as_str() == session)
}

/// 页面级查看声明（`session.view` 控制帧）。
async fn declare_view(client: &mut SyncClient, session: Option<&str>, version: u64) -> Value {
    client
        .send(
            "session.view",
            json!({ "sessionId": session, "viewVersion": version }),
        )
        .await;
    client.expect_type("session.view.result").await["body"].clone()
}

// ---------------------------------------------------------------------------------------------
// 1. 保留期到期 → 逐会话回收 → 历史可读 → 原身份恢复
// ---------------------------------------------------------------------------------------------

/// 保留期到期后由组合根的回收入口逐会话释放；历史正文与既有事件仍可读；之后 `session.resume`
/// 恢复**同一**会话身份与持久化的 ACP 会话标识，不新建会话，也不重放旧 prompt。
///
/// 变红点：①回收后历史读不到或正文被清空；②恢复后 `sessionId` 变了、后端收到 `create`、或旧 prompt
/// 被重发（`prompt_calls` 增加）；③恢复用了新的 ACP 会话标识（`resumed_agent_ids` 与创建时不同）；
/// ④恢复成功后摘要仍是 idle（资源代没置 ready）。
#[tokio::test]
async fn a_retention_reclaim_keeps_history_and_resume_restores_the_same_identity() {
    let host = HostNode::start_with_lifecycle("life-retention", HOST, config(250, 50)).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let session = local_session(&host).await;
    let mut client = connect_lifecycle(&host, &device).await;

    let marker = "retention-context-marker";
    prompt_to_completion(&host, &mut client, &session, marker).await;
    assert_eq!(
        local_activity(&mut client, &session).await,
        json!("active"),
        "创建并完成一轮后资源仍在，本机展示必须是 active"
    );

    let before = read_resources(&mut client, &session).await;
    assert!(
        message_texts(&before).iter().any(|text| text == marker),
        "回收前必须能读到这一轮的正文：{before}"
    );
    let before_history = history_ids(&before);
    assert!(
        before_history
            .iter()
            .any(|(_, event_type)| event_type == "turn.started"),
        "回收前必须已有该轮的历史事件：{before_history:?}"
    );
    let creates_before = host.script().create_calls();
    let prompts_before = host.script().prompt_calls();
    assert_eq!(creates_before, 1, "前置：恰好一次创建");
    assert_eq!(prompts_before, 1, "前置：恰好一次 prompt 派发");

    // 没有查看者、任务已结束：保留期到期后该会话成为候选，组合根按候选逐会话释放。
    let candidate = wait_for_retention_candidate(&host, &session).await;
    let outcome = host
        .lifecycle()
        .release_reclaim_candidate(&Actor::LocalCli, &candidate, host.clock().now())
        .await
        .expect("逐会话释放");
    assert_eq!(
        outcome,
        ReleaseOutcome::Released,
        "脚本后端可确认释放时必须返回 Released"
    );
    assert_eq!(
        host.script().release_calls(),
        1,
        "必须真的请求过一次逐会话释放"
    );
    assert_eq!(
        local_activity(&mut client, &session).await,
        json!("idle"),
        "已确认释放后本机展示才是 idle"
    );

    // 历史正文不丢失：消息逐字不变，既有事件仍在（回收只追加生命周期事件）。
    let after = read_resources(&mut client, &session).await;
    assert_eq!(
        message_texts(&after),
        message_texts(&before),
        "回收 MUST NOT 改动历史正文：{after}"
    );
    let after_history = history_ids(&after);
    for (event_id, event_type) in &before_history {
        assert!(
            after_history
                .iter()
                .any(|(id, kind)| id == event_id && kind == event_type),
            "回收前的历史事件 {event_id}({event_type}) 必须仍可读：{after_history:?}"
        );
    }

    // 恢复：同一 sessionId、同一 ACP 会话标识、没有新建、没有重放。
    let terminal = resume(&mut client, &session).await;
    assert_eq!(terminal["status"], json!("completed"), "{terminal}");
    assert_eq!(terminal["result"]["sessionId"], json!(session));
    assert_eq!(
        terminal["result"]["session"]["localActivity"],
        json!("active"),
        "恢复成功 ⇒ 资源代 ready ⇒ active：{terminal}"
    );
    assert_eq!(
        host.script().resume_calls(),
        1,
        "回收后恢复必须真的走一次后端恢复"
    );
    assert_eq!(
        host.script().create_calls(),
        creates_before,
        "恢复 MUST NOT 新建替代会话"
    );
    assert_eq!(
        host.script().prompt_calls(),
        prompts_before,
        "恢复 MUST NOT 重放旧 prompt"
    );
    assert_eq!(
        host.script().resumed_agent_ids(),
        vec![persisted_agent_session_id(&session)],
        "恢复必须沿用创建时持久化的 ACP 会话标识"
    );

    // 清单里仍然只有这一条会话（没有替代身份），正文仍只有一份。
    let sessions = list_sessions(&mut client).await;
    assert_eq!(sessions.len(), 1, "不得新建替代会话：{sessions:?}");
    assert_eq!(sessions[0]["sessionId"], json!(session));
    let resumed = read_resources(&mut client, &session).await;
    assert_eq!(
        message_texts(&resumed),
        message_texts(&before),
        "恢复 MUST NOT 把旧输入再写一遍历史"
    );

    host.stop().await;
}

// ---------------------------------------------------------------------------------------------
// 2. Daemon 重启（同一 data_dir 上的新组合根）
// ---------------------------------------------------------------------------------------------

/// 重启后：启动收敛不把旧 `active` 当成资源仍可用；已受理但无法确认的 prompt 由组合根的启动恢复
/// 结为 `uncertain` 且**不自动重派**；恢复后原会话上下文仍可读、仍用原身份。
///
/// 变红点：①重启后无任务的会话仍显示 active（信任了旧 ready）；②启动恢复把 accepted 记成 completed/
/// failed 或重派 prompt；③恢复后历史读不到 marker；④恢复新建了会话或换了 ACP 会话标识。
#[tokio::test]
async fn a_daemon_restart_converges_without_trusting_old_resources() {
    let root = TempRoot::new("life-restart");
    let lifecycle = config(1_800_000, 50);
    let first = HostNode::start_in_root(root.path(), HOST, lifecycle, restart_seed()).await;
    let node_id = first.host_id();
    let device = Device::new();
    pair_and_approve(&first, &device).await;

    // 会话 S：一轮已完成（重启前只有资源、没有任务）。
    let settled_session = local_session(&first).await;
    let mut client = connect_lifecycle(&first, &device).await;
    let marker = "restart-context-marker";
    prompt_to_completion(&first, &mut client, &settled_session, marker).await;
    assert_eq!(
        local_activity(&mut client, &settled_session).await,
        json!("active")
    );

    // 会话 T：prompt 已受理但后端从未产出终态（崩溃时无法确认结果）。
    let unsettled_session = local_session(&first).await;
    let unsettled_request = uuid_text();
    let unsettled_marker = "restart-uncertain-marker";
    client
        .command(
            &unsettled_request,
            "session.prompt",
            Some(&unsettled_session),
            json!({ "content": [{ "type": "text", "text": unsettled_marker }] }),
        )
        .await;
    let accepted = client.expect_command_result(&unsettled_request).await;
    assert_eq!(accepted["status"], json!("accepted"), "{accepted}");
    assert_eq!(
        local_activity(&mut client, &unsettled_session).await,
        json!("active"),
        "未结束任务必须让会话保持 active"
    );

    // 崩溃：停止组合根，不结算那条 prompt。
    drop(client);
    first.stop().await;

    // 同一 data_dir 上重启（固定身份种子 ⇒ 同一节点；同一设备信任仍然有效）。
    let restarted = HostNode::start_in_root(root.path(), HOST, lifecycle, restart_seed()).await;
    assert_eq!(restarted.host_id(), node_id, "重启必须还是同一个节点身份");

    // 启动收敛不信旧 ready：S 重启前是 active，但本进程没有任何会话资源。
    assert_eq!(
        restarted.script().resume_calls(),
        0,
        "启动收敛 MUST NOT 自动恢复 Agent 会话"
    );

    // 组合根 `daemon::run` 的同一启动恢复入口（§6 第 16 条）。
    let settled = restarted
        .core()
        .recover_unsettled(&Actor::LocalCli, acp_core::ports::ReplayLimit::new(16))
        .await
        .expect("启动恢复");
    assert_eq!(settled, 1, "恰好一条未终结命令被终结");
    assert_eq!(
        restarted.script().prompt_calls(),
        0,
        "启动恢复 MUST NOT 重派 prompt"
    );

    let mut after = connect_lifecycle(&restarted, &device).await;
    assert_eq!(
        local_activity(&mut after, &settled_session).await,
        json!("idle"),
        "旧 ready 事实不得被重放成 active"
    );
    assert_eq!(
        local_activity(&mut after, &unsettled_session).await,
        json!("idle"),
        "uncertain 终结后没有任务与资源，必须是 idle"
    );

    // 持久终态读回：崩溃窗口只能结为 uncertain。
    let status_request = uuid_text();
    after
        .command(
            &status_request,
            "command.status",
            None,
            json!({ "targetRequestId": unsettled_request }),
        )
        .await;
    let queried = after.expect_command_result(&status_request).await;
    assert_eq!(queried["status"], json!("completed"), "{queried}");
    assert_eq!(
        queried["result"]["state"],
        json!("uncertain"),
        "重启只能把无法确认的结果结为 uncertain：{queried}"
    );

    // 历史仍可读。
    let history = read_resources(&mut after, &settled_session).await;
    assert!(
        message_texts(&history).iter().any(|text| text == marker),
        "重启后历史正文必须仍可读：{history}"
    );

    // 恢复原会话：同一身份、沿用持久化 ACP 标识、无新会话、无重放。
    let terminal = resume(&mut after, &settled_session).await;
    assert_eq!(terminal["status"], json!("completed"), "{terminal}");
    assert_eq!(
        terminal["result"]["session"]["localActivity"],
        json!("active")
    );
    assert_eq!(terminal["result"]["sessionId"], json!(settled_session));
    assert_eq!(
        restarted.script().resumed_agent_ids(),
        vec![persisted_agent_session_id(&settled_session)],
        "重启后的恢复必须沿用原 ACP 会话标识"
    );
    assert_eq!(
        restarted.script().create_calls(),
        0,
        "恢复不得新建会话（重启不创建任何会话）"
    );
    assert_eq!(
        restarted.script().prompt_calls(),
        0,
        "恢复不得重放旧 prompt"
    );

    // 另一条（uncertain 的）会话也能恢复，且不会把那条旧 prompt 重发一次。
    let other = resume(&mut after, &unsettled_session).await;
    assert_eq!(other["status"], json!("completed"), "{other}");
    assert_eq!(
        restarted.script().prompt_calls(),
        0,
        "恢复 uncertain 会话不得重派旧 prompt"
    );

    restarted.stop().await;
}

// ---------------------------------------------------------------------------------------------
// 3. 多设备查看租约与撤销
// ---------------------------------------------------------------------------------------------

/// 两个设备同时查看同一条会话：只离开一个不开始计时；最后一个离开后才进入保留期；撤销设备立即
/// 关闭它的连接并清理该连接的查看者，而另一设备的查看保护不受影响。
///
/// 变红点：①任一设备仍在查看时该会话成为保留候选；②撤销后连接没有关闭或查看者没有被清理；
/// ③最后一个查看者离开后没有开始保留期。
#[tokio::test]
async fn only_the_last_viewer_starts_the_retention_window() {
    let host = HostNode::start_with_lifecycle("life-leases", HOST, config(250, 50)).await;
    let first_device = Device::new();
    let second_device = Device::new();
    pair_and_approve(&host, &first_device).await;
    pair_and_approve(&host, &second_device).await;
    let session = local_session(&host).await;

    let mut first = connect_lifecycle(&host, &first_device).await;
    let mut second = connect_lifecycle(&host, &second_device).await;

    // 前置：没有查看者时，保留期一到它就成为候选（证明下面的「不是候选」确实来自查看者保护）。
    wait_for_retention_candidate(&host, &session).await;

    let viewing_first = declare_view(&mut first, Some(session.as_str()), 1).await;
    assert_eq!(
        viewing_first["status"],
        json!("accepted"),
        "{viewing_first}"
    );
    let viewing_second = declare_view(&mut second, Some(session.as_str()), 1).await;
    assert_eq!(
        viewing_second["status"],
        json!("accepted"),
        "{viewing_second}"
    );

    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(
        !is_retention_candidate(&host, &session).await,
        "有查看者时不得进入保留候选"
    );

    // 第一个设备离开（同一连接声明 null）：只剩第二个查看者，仍不计时。
    let left = declare_view(&mut first, None, 2).await;
    assert_eq!(left["status"], json!("accepted"), "{left}");
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(
        !is_retention_candidate(&host, &session).await,
        "另一个设备仍在查看时不得开始保留期"
    );

    // 第一个设备重新查看后被撤销：连接关闭，它的查看者随连接一起清理。
    let viewing_again = declare_view(&mut first, Some(session.as_str()), 3).await;
    assert_eq!(
        viewing_again["status"],
        json!("accepted"),
        "{viewing_again}"
    );
    host.admin(
        server::local_admin::Method::DeviceRevoke,
        json!({ "deviceId": first_device.id() }),
    )
    .await;
    assert_eq!(
        first.close_code().await,
        server::sync::conn::close::REVOKED,
        "撤销提交后该设备的连接必须收到 4410"
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while host.active_sync_connections() > 1 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        host.active_sync_connections(),
        1,
        "撤销只关闭被撤销设备的连接"
    );
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(
        !is_retention_candidate(&host, &session).await,
        "撤销一个查看者后，另一个仍在查看的设备必须继续保护该会话"
    );

    // 最后一个查看者断开：之后才开始完整保留期。
    drop(second);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while host.active_sync_connections() > 0 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(host.active_sync_connections(), 0, "连接必须已关闭");
    let candidate = wait_for_retention_candidate(&host, &session).await;
    assert_eq!(candidate.session.as_str(), session.as_str());

    host.stop().await;
}

// ---------------------------------------------------------------------------------------------
// 4. 容量：满额时优先回收安全候选
// ---------------------------------------------------------------------------------------------

/// 容量 2 已满、两个会话都无人查看且无任务（尚未满 30 分钟保留期）：新激活必须先回收**最久未使用**
/// 的安全候选，再把名额交给它。
///
/// 变红点：①拒绝新激活而不是回收候选；②回收了较新使用的那个会话；③回收未完成就把名额交出去
/// （新激活 admitted 但被回收的会话还 active）。
#[tokio::test]
async fn capacity_pressure_reclaims_the_lru_candidate_before_admitting() {
    let host =
        HostNode::start_with_lifecycle("life-capacity-lru", HOST, config(1_800_000, 2)).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;

    // 目标会话 C 先存在（曾 ready，随后被逐会话释放，因此不在运行期目录里占名额）。
    let target = local_session(&host).await;
    let generation = host
        .lifecycle()
        .resource_generation(&SessionId::new(&target).expect("session id"))
        .expect("创建成功后必须有资源代");
    host.lifecycle()
        .release_session(
            &Actor::LocalCli,
            SessionId::new(&target).expect("session id"),
            generation,
            host.clock().now(),
        )
        .await
        .expect("释放目标会话的名额");

    // 占满两个名额：older 先建，newer 稍后（LRU 的判定只看 `last_protected_at`）。
    let older = local_session(&host).await;
    tokio::time::sleep(Duration::from_millis(40)).await;
    let newer = local_session(&host).await;

    let mut client = connect_lifecycle(&host, &device).await;
    assert_eq!(
        local_activity(&mut client, &older).await,
        json!("active"),
        "前置：older 占用名额且 ready"
    );
    assert_eq!(
        local_activity(&mut client, &newer).await,
        json!("active"),
        "前置：newer 占用名额且 ready"
    );

    // 名额已满：恢复 target 必须先回收最久未使用者。
    let releases_before = host.script().release_calls();
    let terminal = resume(&mut client, &target).await;
    assert_eq!(
        terminal["status"],
        json!("completed"),
        "满额时存在安全候选 ⇒ 必须回收后接纳，而不是拒绝：{terminal}"
    );
    assert_eq!(
        terminal["result"]["session"]["localActivity"],
        json!("active")
    );
    assert_eq!(
        local_activity(&mut client, &older).await,
        json!("idle"),
        "最久未使用的安全候选必须先被回收：older"
    );
    assert_eq!(
        local_activity(&mut client, &newer).await,
        json!("active"),
        "较新使用的候选不得被回收：newer"
    );
    assert_eq!(
        host.script().release_calls(),
        releases_before + 1,
        "回收必须先完成（恰一次逐会话释放）才交出名额"
    );

    host.stop().await;
}

/// 满额且**没有**安全候选（唯一的占位会话正被查看）：新激活必须明确拒绝
/// `session.capacity_exceeded`（可重试），不中断被保护的会话，目标会话历史仍可读。
///
/// 变红点：①返回 accepted/completed；②错误码不是 `session.capacity_exceeded` 或不可重试；
/// ③被查看的会话被回收或降为 idle。
#[tokio::test]
async fn a_full_node_with_only_protected_sessions_rejects_the_activation() {
    let host =
        HostNode::start_with_lifecycle("life-capacity-full", HOST, config(1_800_000, 1)).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;

    // 目标会话 C：有历史正文，但当前不占名额（已被释放）。
    let target = local_session(&host).await;
    let mut client = connect_lifecycle(&host, &device).await;
    let marker = "capacity-history-marker";
    prompt_to_completion(&host, &mut client, &target, marker).await;
    let generation = host
        .lifecycle()
        .resource_generation(&SessionId::new(&target).expect("session id"))
        .expect("资源代");
    host.lifecycle()
        .release_session(
            &Actor::LocalCli,
            SessionId::new(&target).expect("session id"),
            generation,
            host.clock().now(),
        )
        .await
        .expect("释放目标会话");

    // 唯一名额交给另一条会话，并由这台设备持续查看（受保护，不是安全候选）。
    let protected = local_session(&host).await;
    let viewing = declare_view(&mut client, Some(protected.as_str()), 1).await;
    assert_eq!(viewing["status"], json!("accepted"), "{viewing}");

    let request = uuid_text();
    client
        .command(&request, "session.resume", Some(&target), json!({}))
        .await;
    let refused = client.expect_command_result(&request).await;
    assert_eq!(
        refused["status"],
        json!("rejected"),
        "没有安全候选时必须在受理前拒绝：{refused}"
    );
    assert_eq!(
        refused["error"]["code"],
        json!("session.capacity_exceeded"),
        "{refused}"
    );
    assert_eq!(
        refused["error"]["retryable"],
        json!(true),
        "容量拒绝必须可重试：{refused}"
    );

    // 被保护的会话不受影响；目标会话历史仍可读。
    assert_eq!(
        local_activity(&mut client, &protected).await,
        json!("active"),
        "被查看的会话 MUST NOT 因容量拒绝被回收或降级"
    );
    let releases_before = host.script().release_calls();
    assert_eq!(
        releases_before, 1,
        "前置：只有目标会话自己在准备阶段被释放过一次"
    );
    let history = read_resources(&mut client, &target).await;
    assert_eq!(
        host.script().release_calls(),
        releases_before,
        "没有安全候选时不得尝试释放任何会话"
    );
    assert!(
        message_texts(&history).iter().any(|text| text == marker),
        "容量拒绝后目标会话历史必须仍可读：{history}"
    );

    host.stop().await;
}

/// 回收未被确认（`Failed`）时保持活跃与容量占位：不得显示 idle、不得把名额交给新激活。
///
/// 变红点：①`Failed` 之后 `localActivity` 变成 idle；②名额被释放（下一次恢复被准入）；
/// ③失败被吞掉（没有 `release_calls`）。
#[tokio::test]
async fn an_unconfirmed_reclaim_keeps_the_slot_and_stays_active() {
    let host =
        HostNode::start_with_lifecycle("life-reclaim-failed", HOST, config(1_800_000, 1)).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;

    let target = local_session(&host).await;
    let generation = host
        .lifecycle()
        .resource_generation(&SessionId::new(&target).expect("session id"))
        .expect("资源代");
    host.lifecycle()
        .release_session(
            &Actor::LocalCli,
            SessionId::new(&target).expect("session id"),
            generation,
            host.clock().now(),
        )
        .await
        .expect("释放目标会话");

    let occupied = local_session(&host).await;
    let mut client = connect_lifecycle(&host, &device).await;

    // 回收不能确认：适配器返回 Failed。
    host.script().set_release_outcome(ReleaseOutcome::Failed);
    let releases_before = host.script().release_calls();
    let generation = host
        .lifecycle()
        .resource_generation(&SessionId::new(&occupied).expect("session id"))
        .expect("资源代");
    let outcome = host
        .lifecycle()
        .release_session(
            &Actor::LocalCli,
            SessionId::new(&occupied).expect("session id"),
            generation,
            host.clock().now(),
        )
        .await
        .expect("释放调用本身可返回");
    assert_eq!(
        outcome,
        ReleaseOutcome::Failed,
        "夹具必须真的走到「找不到可确认的释放」这条路径"
    );
    assert_eq!(
        host.script().release_calls(),
        releases_before + 1,
        "必须真的尝试过释放，失败是后端给出的结果"
    );

    assert_eq!(
        local_activity(&mut client, &occupied).await,
        json!("active"),
        "回收未确认必须保持 active，不得伪造空闲"
    );

    // 名额仍被占：新激活明确容量拒绝。
    let request = uuid_text();
    client
        .command(&request, "session.resume", Some(&target), json!({}))
        .await;
    let refused = client.expect_command_result(&request).await;
    assert_eq!(
        refused["status"],
        json!("rejected"),
        "占位未确认时不得把名额交给新激活：{refused}"
    );
    assert_eq!(
        refused["error"]["code"],
        json!("session.capacity_exceeded"),
        "{refused}"
    );

    // 恢复必须可证明失败：把释放结果改回 Released，名额才真的交还。
    host.script().set_release_outcome(ReleaseOutcome::Released);
    let generation = host
        .lifecycle()
        .resource_generation(&SessionId::new(&occupied).expect("session id"))
        .expect("资源代");
    let released = host
        .lifecycle()
        .release_session(
            &Actor::LocalCli,
            SessionId::new(&occupied).expect("session id"),
            generation,
            host.clock().now(),
        )
        .await
        .expect("释放调用");
    assert_eq!(released, ReleaseOutcome::Released);
    assert_eq!(
        local_activity(&mut client, &occupied).await,
        json!("idle"),
        "只有确认 Released 之后才允许 idle"
    );
    let admitted = resume(&mut client, &target).await;
    assert_eq!(
        admitted["status"],
        json!("completed"),
        "名额确认交还后新激活必须被准入：{admitted}"
    );

    host.stop().await;
}

// ---------------------------------------------------------------------------------------------
// 5. 历史正文与持久事实（回收/重启之外的最后一道防线）
// ---------------------------------------------------------------------------------------------

/// 已被逐会话释放的会话仍能不带任何活体端点读到完整正文与既有历史事件（回收只追加生命周期事件，
/// 不删改正文）。
///
/// 变红点：①释放后 `session.read` 失败；②消息被清空或改写；③既有历史事件消失。
#[tokio::test]
async fn history_stays_readable_after_release_without_a_live_endpoint() {
    let host = HostNode::start_with_lifecycle("life-history", HOST, config(1_800_000, 50)).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let session = local_session(&host).await;
    let mut client = connect_lifecycle(&host, &device).await;

    let marker = "history-marker";
    prompt_to_completion(&host, &mut client, &session, marker).await;
    let before = read_resources(&mut client, &session).await;

    let generation = host
        .lifecycle()
        .resource_generation(&SessionId::new(&session).expect("session id"))
        .expect("资源代");
    assert_eq!(
        host.lifecycle()
            .release_session(
                &Actor::LocalCli,
                SessionId::new(&session).expect("session id"),
                generation,
                host.clock().now(),
            )
            .await
            .expect("逐会话释放"),
        ReleaseOutcome::Released
    );

    let request = uuid_text();
    client
        .command(
            &request,
            "session.read",
            Some(&session),
            json!({ "include": ["messages", "history"] }),
        )
        .await;
    let after = client.expect_command_result(&request).await;
    assert_eq!(after["status"], json!("completed"), "{after}");
    let resources = &after["result"]["resources"];
    assert_eq!(
        message_texts(resources),
        message_texts(&before),
        "没有端点时历史正文必须逐字不变"
    );
    for (event_id, event_type) in history_ids(&before) {
        assert!(
            history_ids(resources)
                .iter()
                .any(|(id, kind)| id == &event_id && kind == &event_type),
            "逐会话释放不得删改既有历史事件 {event_id}({event_type})：{resources}"
        );
    }

    host.stop().await;
}

/// 释放后对同一代重复请求释放不会得到 `Released`（旧代结果不得绑定到新代），也不得因此把会话
/// 从 idle 改回 active。这里顺手证明 `PortError` 的失败路径不会污染本机展示。
#[tokio::test]
async fn a_stale_release_generation_never_reopens_a_released_session() {
    let host =
        HostNode::start_with_lifecycle("life-stale-release", HOST, config(1_800_000, 50)).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    let session = local_session(&host).await;
    let mut client = connect_lifecycle(&host, &device).await;

    let id = SessionId::new(&session).expect("session id");
    let first = host.lifecycle().resource_generation(&id).expect("资源代");
    assert_eq!(
        host.lifecycle()
            .release_session(&Actor::LocalCli, id.clone(), first, host.clock().now())
            .await
            .expect("第一次释放"),
        ReleaseOutcome::Released
    );
    assert_eq!(local_activity(&mut client, &session).await, json!("idle"));

    // 同一代重复释放：不再 `Released`，`localActivity` 仍是 idle。
    let again = host
        .lifecycle()
        .release_session(&Actor::LocalCli, id.clone(), first, host.clock().now())
        .await;
    match again {
        Ok(outcome) => assert_ne!(
            outcome,
            ReleaseOutcome::Released,
            "已释放的同一资源代不得再次确认释放"
        ),
        Err(PortError::Unavailable(_)) => {}
        Err(other) => panic!("重复释放只允许返回未确认或明确的不可用，得到 {other:?}"),
    }
    assert_eq!(
        local_activity(&mut client, &session).await,
        json!("idle"),
        "迟到的释放结果不得把已释放的会话改回 active"
    );
    assert_eq!(
        host.lifecycle()
            .current_activity(&Actor::LocalCli, id.clone())
            .await
            .expect("当前活跃程度"),
        acp_core::model::LocalActivity::Idle
    );

    host.stop().await;
}
