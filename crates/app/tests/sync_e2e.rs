//! Sync 协议级端到端验收（`sync-server-connection` 任务 8.1–8.3）。
//!
//! 驱动方式：脚本化 fake Sync 客户端（`support::sync`）经**真实 loopback listener**（`/sync/v1` 的
//! WSS 升级）走完「配对 claim → 本机确认 → status approved → 四消息握手 → 订阅 → 快照 → ACK」，
//! 并覆盖四条边界。Owner 侧是真实 SQLite + 真实 `identity_auth::Authority` + 真实本机管理通道
//! （`server::local_admin::LocalAdminRouter`）与真实组合根装配（`app::daemon::{net_config,
//! register_node_link_paths, register_sync_paths}` —— 与 `app::daemon::run` 同一批函数）。
//!
//! 证据口径：每个用例的每条断言都指向**可观察事实**（收到的帧、断开码、错误码、连接表计数），
//! 且都能在实现错误时变红；「摘要复算」由客户端对收到的原始帧文本**独立**计算后逐字节比对。

mod support;

use serde_json::{Value, json};
use support::sync::{DeviceKey, HostNode, SyncClient, nonce_text, snapshot_items, uuid_text};

/// 本机宿主名（`daemon.public_origin` 的 authority；`Host` 头与 `Origin` 头都按它判）。
const HOST: &str = "sync-e2e.example.invalid";
/// 本机真实管理的 agent selector（`agent.configure` 与快照 `agents` chunk 都用它）。
const AGENT: &str = "codex";
/// 设备名（`deviceName`；`device.pair.begin` 不携带展示名，由 claim 声明）。
const DEVICE_NAME: &str = "Test Phone";
/// 设备配对**不提交也不分配任何授权集合**（`pairing-model` 删除设备权限层）：`device.pair.begin`
/// 只收 `expiresInMs`、`confirm` 只收 `pairingId`，配对状态与认证事实里都不得再出现这组词汇——
/// 它们是「已删掉的授权集合与短验证码」的清单，出现在响应任何层级都让对应用例变红。
const REMOVED_AUTHORIZATION_KEYS: [&str; 5] = ["sas", "scopes", "grants", "packs", "presets"];

/// 客户端声明的 feature（含三类清单资源门控 `core.local-catalog.v1`：快照因此下发三块）。
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
        "core.future-thing.v1",
    ]
}

/// 配对索赔方（设备）的材料。
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

/// 一次 `POST` 的响应（状态码 + 解析出的 JSON）。
struct HttpReply {
    status: u16,
    body: Value,
}

/// 向真实 listener 的配对端点发一条 `POST`（`Host` 与 `Origin` 都取本机 origin，§3.2/§7.1）。
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

/// 本机登记一次 workspace alias（快照 `workspaces` chunk 的投影来源；真实本地管理通道）。
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

/// 本机登记一个 agent profile（快照 `agents` chunk 的投影来源）。
async fn register_agent(host: &HostNode) {
    host.admin(
        server::local_admin::Method::AgentConfigure,
        json!({
            "agentId": AGENT,
            "displayName": "Codex",
            "command": env!("CARGO_BIN_EXE_acp-remote"),
            "args": [],
            "envAllowlist": [],
            "default": true,
        }),
    )
    .await;
}

/// 走完「device.pair.begin → claim → device.pair.confirm → status approved」。
///
/// 全部是**真实**路径：本地管理通道（`LocalAdminRouter`）、配对 HTTP 端点（`/sync/v1/pairing/*`）与
/// `identity_auth::Authority` 的状态机。
async fn pair_and_approve(host: &HostNode, device: &Device) -> Value {
    // ① 本机开始配对（真实本地通道），拿到二维码 URL（含 pairing secret fragment）。
    //    设备方向的参数面只有 `expiresInMs`：`requestedPacks`/`requestedScopes` 之类的授权词汇
    //    出现即 `local.invalid_params`（配对不授予授权）。
    let begin = host
        .admin(
            server::local_admin::Method::DevicePairBegin,
            json!({ "expiresInMs": null }),
        )
        .await;
    let pairing_id = begin["pairingId"].as_str().expect("pairingId").to_owned();
    let pairing_url = begin["pairingUrl"].as_str().expect("pairingUrl");
    let (ticket, secret) = qr_payload(pairing_url);

    // ② 设备侧 claim（`acp-remote/pairing-proof/v1` 的 HMAC；真实 HTTP 端点）。
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
    let claim_body = replied.body;
    assert_eq!(
        claim_body["status"],
        json!("pending_confirmation"),
        "claim 之后必须仍是待确认：{claim_body}"
    );

    // ③ 本机确认（真实本地通道）：**只**提交 `pairingId`——设备配对不分配任何授权集合，
    //    确认因此没有可提交的 scope/pack；这些键出现即 `local.invalid_params`。
    let confirmed = host
        .admin(
            server::local_admin::Method::DevicePairConfirm,
            json!({ "pairingId": pairing_id }),
        )
        .await;
    assert_eq!(
        confirmed["deviceId"],
        json!(device.id()),
        "确认必须创建该设备的信任记录：{confirmed}"
    );

    // ④ 设备侧 status（同 secret 的 HMAC）→ approved。
    let status = json!({
        "protocolVersion": 1,
        "hostId": host.host_id(),
        "deviceId": device.id(),
        "pairingId": pairing_id,
        "pairingRequestId": claim_body["pairingRequestId"],
        "requestNonce": device.request_nonce,
        "proof": status_proof(
            &ticket,
            &secret,
            device,
            claim_body["pairingRequestId"].as_str().expect("pairingRequestId"),
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
    // 已批准响应只承载协议事实与两侧的非秘密元数据：设备配对既不再派生短验证码（`sas`），也不再
    // 分配任何授权集合，因此这组词汇出现在响应任何层级都让本断言变红。
    for removed in REMOVED_AUTHORIZATION_KEYS {
        assert!(
            !support::sync::contains_key_deep(&replied.body, removed),
            "`{removed}` 不得出现在配对状态响应里：{}",
            replied.body
        );
    }
    replied.body
}

/// 二维码 payload（`#data=<无填充 base64url(json)>`）→ `(payload, secret)`。
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

/// 设备公钥的 wire 形状（65 字节 SEC1 未压缩点，无填充 base64url）。
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

/// status 证明：`acp-remote/pairing-status/v1` 域的 HMAC-SHA256。
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

/// HMAC-SHA256（与 `identity-auth` 同口径；测试侧只做装配，不复制它的校验逻辑）。
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    use sha2::{Digest as _, Sha256};
    // RFC 2104 的直写实现：块长 64 字节；`identity-auth` 用同一构造（`hmac_sha256`）。
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

// ---------------------------------------------------------------------------------------------
// 8.1：fake Sync 客户端对真实 listener 完成一次完整认证
// ---------------------------------------------------------------------------------------------

/// 8.1 的完成判据：客户端能对真实 listener 完成一次完整认证并收到 `auth.authenticated`。
#[tokio::test]
async fn a_device_completes_the_four_message_handshake_on_the_real_listener() {
    let host = HostNode::start("sync-handshake", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;

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
    assert_eq!(
        authenticated["body"]["deviceId"],
        json!(device.id()),
        "authenticated 必须回本设备：{authenticated}"
    );
    // 认证事实（§8.4）：设备标识、事件库 epoch 与水位、心跳间隔与本次连接的 limits。设备方向
    // **不携带任何授权集合**——设备权限层已删除，`scopes` 之类的键出现即本断言集合变红。
    let head = host.head().await;
    assert_eq!(
        authenticated["body"]["serverEpoch"],
        json!(head.server_epoch.as_str()),
        "serverEpoch 必须是本机事件库的 epoch：{authenticated}"
    );
    assert_eq!(
        authenticated["body"]["headGlobalSequence"],
        json!(head.global_sequence.get().to_string()),
        "headGlobalSequence 必须是事件库当前水位：{authenticated}"
    );
    assert_eq!(
        authenticated["body"]["heartbeatIntervalMs"],
        json!(server::sync::conn::limits::DEFAULT_HEARTBEAT_INTERVAL_MS),
        "心跳间隔必须是协议固定值 30 秒：{authenticated}"
    );
    assert_eq!(
        authenticated["body"]["limits"],
        json!({
            "maxMessageBytes": server::sync::conn::limits::DEFAULT_MAX_MESSAGE_BYTES,
            "maxPromptBytes": server::sync::conn::limits::DEFAULT_MAX_PROMPT_BYTES,
            "maxReplayEventsPerBatch":
                server::sync::conn::limits::DEFAULT_MAX_REPLAY_EVENTS_PER_BATCH,
        }),
        "limits 必须是与连接内部判定同源的三个值（多一个键也算漂移）：{authenticated}"
    );
    let mut body_keys: Vec<&str> = authenticated["body"]
        .as_object()
        .expect("authenticated 的 body 是 object")
        .keys()
        .map(String::as_str)
        .collect();
    body_keys.sort_unstable();
    assert_eq!(
        body_keys,
        vec![
            "deviceId",
            "headGlobalSequence",
            "heartbeatIntervalMs",
            "limits",
            "serverEpoch"
        ],
        "认证事实只有这五个键：任何授权集合（`scopes`/`grants`/`packs`/`presets`）出现即变红：{authenticated}"
    );
    // 连接字段：认证后每条消息都带，且 `connectionId` 与本连接一致（客户端已逐条核对过序号）。
    assert!(
        client.connection_id().is_some(),
        "认证后必须记住 connectionId"
    );
    assert_eq!(
        host.active_sync_connections(),
        1,
        "认证成功后必须有一条活动 Sync 连接"
    );

    host.stop().await;
}

/// 快照断言辅助的可见性检查（编译期确认 support 面被真的用上）。
#[test]
fn snapshot_helpers_are_exported() {
    let snapshot_text = support::sync::recompute_digest(&[]);
    assert_eq!(snapshot_text.len(), 43, "空输入的 base64url 摘要长度");
    let _ = snapshot_items;
    let _ = uuid_text;
    let _: Option<&Value> = None;
}

// ---------------------------------------------------------------------------------------------
// 本机会话的来源（不经 wire）
// ---------------------------------------------------------------------------------------------

/// 本机建一条 owned 会话并**落账**（§6 第 20 条的三笔时序：受理 → 执行 → 落账），返回会话标识。
///
/// 受理提交只落幂等记录——会话行与终态在**落账提交**里同批落盘，而本文件的用例都要求这条会话真实
/// 存在（快照的 `sessions` chunk 要列得到它、事件回放要有它的事件），因此创建后必须再结算一次
/// （与生产适配层同形）。终态结果原文只是形状占位：本文件不读它。
async fn local_session(host: &HostNode, fingerprint: &str) -> acp_core::model::SessionId {
    let request_id = acp_core::model::RequestId::new(&uuid_text()).expect("request id 规范");
    let prepared = match host
        .core()
        .create_session(
            &acp_core::model::Actor::LocalCli,
            request_id.clone(),
            ac1_digest(fingerprint),
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
    prepared.session
}

// ---------------------------------------------------------------------------------------------
// 8.2：主链路（配对 → 握手 → 订阅 → 快照 → 摘要复算 → caught_up → ACK）
// ---------------------------------------------------------------------------------------------

/// 8.2：三类资源齐备的快照 + 按收到的原始帧独立复算摘要 + `sync.caught_up` + ACK。
///
/// 顺序（§9.2/§9.4 的 wire 协议）：`sync.subscribe(null)` → `sync.reset_required{initial_sync}` →
/// `sync.snapshot_request` → `snapshot_begin`/`chunk`×3/`snapshot_end` → `sync.caught_up` → `sync.ack`。
/// **MUST 走真实 listener**（`/sync/v1` 升级）而不是直调处理器：这条链路因此覆盖 WP6a 的
/// `with_route(SubscribeRoute)` 接线。
#[tokio::test]
async fn the_main_chain_runs_snapshot_digest_recompute_and_ack() {
    let host = HostNode::start("sync-main", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;

    // 本机侧事实：一个 workspace alias + 一个 agent profile（快照后两块 chunk 的投影来源）、
    // 一个本地会话（`sessions` chunk）与一条**真实**的节点级事件（`agent.connected`，
    // 经 `Broker::commit_node_event` 落库——因此事件库里 head > 0，快照屏障游标可断言）。
    register_workspace(&host, "project.one").await;
    register_agent(&host).await;
    // 水位取样在**本地会话落账之后**：落账提交自身也会追加一条终态事件（推进水位），若把它夹在两次
    // 读数之间，下面的断言就会被它顺带满足，不再证明「节点级事件推进水位」这件事。
    let session_id = local_session(&host, "acp-remote/sync-e2e/session-create").await;
    let head_before = host.head().await;
    host.emit_agent_lifecycle(AGENT, "connected").await;
    let head = host.head().await;
    assert!(
        head.global_sequence.get() > head_before.global_sequence.get(),
        "节点级事件必须推进事件库水位：{head_before:?} → {head:?}"
    );

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
    let authenticated_head = authenticated["body"]["headGlobalSequence"]
        .as_str()
        .expect("headGlobalSequence")
        .to_owned();

    // ① 首次同步：cursor = null → 服务端先下发 `sync.reset_required{initial_sync}`。
    client.step("首次订阅");
    client.subscribe(None).await;
    let reset = client.expect_type("sync.reset_required").await;
    assert_eq!(
        reset["body"]["reason"],
        json!("initial_sync"),
        "首次同步必须先收到 initial_sync 复位：{reset}"
    );

    // ② 快照（协商了 `core.local-catalog.v1` → 三类资源齐备）。
    client.step("快照");
    client.snapshot_request("initial_sync").await;
    let snapshot = client.expect_snapshot().await;
    assert_eq!(
        snapshot.chunk_count, 3,
        "三类资源必须各占一块：{snapshot:?}"
    );
    let resources: Vec<&str> = snapshot
        .chunks
        .iter()
        .map(|chunk| chunk.resource.as_str())
        .collect();
    assert_eq!(
        resources,
        vec!["sessions", "workspaces", "agents"],
        "chunk 顺序固定为 sessions → workspaces → agents：{snapshot:?}"
    );
    for index in 0..snapshot.chunks.len() {
        assert_eq!(
            snapshot.chunks[index].chunk_index, index as u64,
            "chunkIndex 必须从 0 起连续：{snapshot:?}"
        );
    }
    let sessions = snapshot_items(&snapshot, "sessions");
    assert_eq!(
        sessions.len(),
        1,
        "本机有一个本地会话，快照 sessions 必须正好一条：{sessions:?}"
    );
    assert_eq!(
        sessions[0]["sessionId"],
        json!(session_id.as_str()),
        "快照里的会话必须是本机创建的那一个：{sessions:?}"
    );
    assert_eq!(
        snapshot_items(&snapshot, "workspaces").len(),
        1,
        "workspaces chunk 必须带本机登记的别名"
    );
    assert_eq!(
        snapshot_items(&snapshot, "agents").len(),
        1,
        "agents chunk 必须带本机登记的 profile"
    );

    // ③ 摘要：客户端对收到的**原始帧文本**独立复算，与声明值逐字节比对。
    assert!(
        snapshot.digest_matches(),
        "snapshotDigest 必须与客户端按原始帧复算的结果逐字节相同（声明值 {}，复算值 {}）",
        snapshot.declared_digest,
        support::sync::recompute_digest(
            &snapshot
                .chunks
                .iter()
                .map(|chunk| chunk.frame_text.clone())
                .collect::<Vec<_>>()
        )
    );

    // ④ 快照之后是 `sync.caught_up`（屏障已建立），其 cursor 不低于 authenticated 的 head。
    let caught_up = client.expect_type("sync.caught_up").await;
    let caught_up_sequence = caught_up["body"]["cursor"]["globalSequence"]
        .as_str()
        .expect("globalSequence")
        .to_owned();
    assert!(
        caught_up_sequence.as_str() >= authenticated_head.as_str(),
        "caught_up 的游标不得低于认证时拿到的 head（{authenticated_head} → {caught_up_sequence}）"
    );

    // ⑤ ACK：服务端接受它（只写本连接状态）。此后到达的只可能是实时 `event` 帧（屏障已建立），
    //    不得出现 `error`——因此断言集合里没有 error，而不要求「没有任何消息」。
    client.step("ACK");
    client.ack(caught_up["body"]["cursor"].clone()).await;
    let after_ack = client.collect(support::sync::COLLECT_WINDOW).await;
    assert!(
        after_ack
            .iter()
            .all(|message| message["type"] != json!("error")),
        "屏障 ACK 必须被接受（不得回错误）：{after_ack:?}"
    );
    assert!(
        after_ack
            .iter()
            .all(|message| message["type"] == json!("event")
                || message["type"] == json!("control.pong")
                || message["type"] == json!("error")),
        "ACK 之后只应有实时事件（或心跳）；出现别的类型说明消息路由异常：{after_ack:?}"
    );

    host.stop().await;
}

// ---------------------------------------------------------------------------------------------
// 8.3：四条边界
// ---------------------------------------------------------------------------------------------

/// 边界①：撤销设备后活动连接收到 `4410`，后续认证被拒（`auth.device_revoked`）。
///
/// 撤销走**真实本机管理通道**（`LocalAdminRouter` 的 `device.revoke`），因此持久提交 → 连接表关闭
/// 这条生产路径被完整执行；连接表的计数同时作为独立证据（不只依赖对端可见的 close 帧）。
#[tokio::test]
async fn revoking_a_device_closes_the_connection_with_4410_and_blocks_later_auth() {
    let host = HostNode::start("sync-revoke", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;

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
    assert_eq!(
        host.active_sync_connections(),
        1,
        "认证后必须有一条活动连接"
    );

    // 撤销（真实本地通道：`device.revoke`）。
    host.admin(
        server::local_admin::Method::DeviceRevoke,
        json!({ "deviceId": device.id() }),
    )
    .await;

    assert_eq!(
        client.close_code().await,
        server::sync::conn::close::REVOKED,
        "撤销提交后活动连接必须收到 4410"
    );
    assert_eq!(host.active_sync_connections(), 0, "关闭后连接表必须为空");

    // 后续认证被拒：同一设备的新连接在 proof 阶段得到 `auth.device_revoked`。
    let mut second = SyncClient::connect_plain(host.addr(), host.host_name(), &host.origin()).await;
    let refused = second
        .authenticate(
            &host.host_id(),
            &device.key,
            &supported_features(),
            &device.client_nonce,
        )
        .await;
    assert_eq!(
        refused["type"],
        json!("error"),
        "撤销后的认证必须被拒，实际：{refused}"
    );
    assert_eq!(
        refused["body"]["code"],
        json!("auth.device_revoked"),
        "拒绝码必须是「设备已撤销」，实际：{refused}"
    );

    host.stop().await;
}

/// 边界②：新连接顶替旧连接时旧连接收到 `4411`，且设备仍可用。
#[tokio::test]
async fn a_new_connection_replaces_the_previous_one_with_4411() {
    let host = HostNode::start("sync-replace", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;

    let mut first = SyncClient::connect_plain(host.addr(), host.host_name(), &host.origin()).await;
    let authenticated = first
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
    assert_eq!(host.active_sync_connections(), 1);

    // 同一设备认证第二条连接（顶替）。
    let mut second = SyncClient::connect_plain(host.addr(), host.host_name(), &host.origin()).await;
    let replaced = second
        .authenticate(
            &host.host_id(),
            &device.key,
            &supported_features(),
            &device.client_nonce,
        )
        .await;
    assert_eq!(
        replaced["type"],
        json!("auth.authenticated"),
        "新连接必须完成认证：{replaced}"
    );
    assert_eq!(
        first.close_code().await,
        server::sync::conn::close::REPLACED,
        "旧连接必须被新连接顶替（4411）"
    );
    assert_eq!(
        host.active_sync_connections(),
        1,
        "顶替后仍然只有一条活动连接"
    );
    // 设备没有被撤销：新连接仍可继续走业务阶段（订阅 → reset_required）。
    second.step("顶替后订阅");
    second.subscribe(None).await;
    let reset = second.expect_type("sync.reset_required").await;
    assert_eq!(reset["body"]["reason"], json!("initial_sync"), "{reset}");

    host.stop().await;
}

/// 边界③：三种非法 cursor 分别得到三种 `details.reason`（epoch 不匹配 / 超 head / 早于保留窗口）。
///
/// 每条断言各自对应一种 reason，因此任一分支的实现错误都会让对应断言变红。
#[tokio::test]
async fn three_invalid_cursors_get_three_distinguishable_reasons() {
    let host = HostNode::start("sync-cursor", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    // 事件库里必须至少有一条事件，否则 `beyond_head` 无从判定（head = 0 时任何序号都「超 head」）。
    host.emit_agent_lifecycle(AGENT, "connected").await;

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
    let epoch = authenticated["body"]["serverEpoch"]
        .as_str()
        .expect("serverEpoch")
        .to_owned();
    let head = authenticated["body"]["headGlobalSequence"]
        .as_str()
        .expect("headGlobalSequence")
        .to_owned();
    assert_ne!(head, "0", "事件库里必须有事件，否则 beyond_head 无从判定");

    // ① 错误 epoch：序号合法但 epoch 不是当前事件库的。
    client.step("错误 epoch");
    let wrong_epoch = uuid_text();
    client
        .subscribe(Some(json!({
            "serverEpoch": wrong_epoch,
            "globalSequence": head,
        })))
        .await;
    let error = client.expect_error().await;
    assert_eq!(
        error["body"]["code"],
        json!("sync.cursor_invalid"),
        "epoch 不符必须回 sync.cursor_invalid：{error}"
    );
    assert_eq!(
        error["body"]["details"]["reason"],
        json!("epoch_mismatch"),
        "三种 reason 必须可区分（epoch 分支）：{error}"
    );
    // 复位由服务端主动下发（§9.4：`epoch_mismatch` 两种后果都有）。
    let reset = client.expect_type("sync.reset_required").await;
    assert_eq!(reset["body"]["reason"], json!("epoch_mismatch"), "{reset}");

    // ② 超 head：epoch 正确、序号可表示但大于当前 head。
    client.step("超 head");
    let beyond = head.parse::<u64>().expect("head 是十进制串") + 1;
    client
        .subscribe(Some(json!({
            "serverEpoch": epoch,
            "globalSequence": beyond.to_string(),
        })))
        .await;
    let error = client.expect_error().await;
    assert_eq!(
        error["body"]["code"],
        json!("sync.cursor_invalid"),
        "越界 cursor 必须回 sync.cursor_invalid：{error}"
    );
    assert_eq!(
        error["body"]["details"]["reason"],
        json!("beyond_head"),
        "三种 reason 必须可区分（beyond_head 分支）：{error}"
    );

    // ③ 早于保留窗口：判据是「提交的 cursor 早于仍可重放的窗口下界」
    //    （`after + 1 < MIN(global_sequence)`）。保留期清理只删「已过期」的行，最近写入的永远不会
    //    过期，因此产品路径在测试的时间尺度上造不出这个形状——用测试替身删掉最早的事件，把窗口下界
    //    推到 2 以上，再提交 cursor 0。
    for _ in 0..2 {
        host.emit_agent_lifecycle(AGENT, "connected").await;
    }
    let before_drop = host.head().await;
    assert!(
        before_drop.global_sequence.get() >= 3,
        "至少要有三条事件，删掉最早的之后仍留下可投递的事件：{before_drop:?}"
    );
    host.drop_earliest_events(2).await;
    client.step("早于保留窗口");
    client
        .subscribe(Some(json!({
            "serverEpoch": epoch,
            "globalSequence": "0",
        })))
        .await;
    let error = client.expect_error().await;
    assert_eq!(
        error["body"]["code"],
        json!("sync.cursor_invalid"),
        "窗口外 cursor 必须回 sync.cursor_invalid：{error}"
    );
    assert_eq!(
        error["body"]["details"]["reason"],
        json!("cursor_expired"),
        "三种 reason 必须可区分（cursor_expired 分支）：{error}"
    );

    host.stop().await;
}

/// 边界④：以更旧 cursor 重订阅时投递从该 cursor 起，而不是从服务端保存的设备 ACK 起。
#[tokio::test]
async fn resubscribing_with_an_older_cursor_replays_from_that_cursor() {
    let host = HostNode::start("sync-older-cursor", HOST).await;
    let device = Device::new();
    pair_and_approve(&host, &device).await;
    register_agent(&host).await;
    local_session(&host, "acp-remote/sync-e2e/replay-session").await;
    let older_head = host.head().await;

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
    let epoch = authenticated["body"]["serverEpoch"]
        .as_str()
        .expect("serverEpoch")
        .to_owned();

    // 先走完整链路（快照 + caught_up + ACK），把本连接的最高游标推到 head。
    client.step("首次完整同步");
    client.subscribe(None).await;
    let reset = client.expect_type("sync.reset_required").await;
    assert_eq!(reset["body"]["reason"], json!("initial_sync"));
    client.snapshot_request("initial_sync").await;
    let _snapshot = client.expect_snapshot().await;
    let caught_up = client.expect_type("sync.caught_up").await;
    let acked = caught_up["body"]["cursor"].clone();
    client.ack(acked.clone()).await;

    // 再产生一条事件，随后从**更旧**的 cursor 重订阅。
    host.emit_agent_lifecycle(AGENT, "connected").await;
    let head_now = host.head().await;
    assert!(
        head_now.global_sequence.get() > older_head.global_sequence.get(),
        "必须真的有新事件可重放：{older_head:?} → {head_now:?}"
    );
    let older = json!({
        "serverEpoch": epoch,
        "globalSequence": older_head.global_sequence.get().to_string(),
    });

    client.step("以更旧 cursor 重订阅");
    client.subscribe(Some(older.clone())).await;
    let resent = client.expect_type("event").await;
    let resent_sequence = resent["body"]["globalSequence"]
        .as_str()
        .expect("globalSequence")
        .parse::<u64>()
        .expect("十进制串");
    assert!(
        resent_sequence > older_head.global_sequence.get(),
        "重投必须从**更旧的 cursor 之后**开始（不是从已 ACK 的 head 起）：{resent_sequence}"
    );

    // ACK 被接受（只写本连接状态）：此后不得出现 `error`（实时事件是允许的）。
    // ACK 的游标形状是 `{serverEpoch, globalSequence}`，因此从事件帧与连接已知 epoch 组装。
    client
        .ack(json!({
            "serverEpoch": epoch,
            "globalSequence": resent_sequence.to_string(),
        }))
        .await;
    let after = client.collect(support::sync::COLLECT_WINDOW).await;
    assert!(
        after
            .iter()
            .all(|message| message["type"] != json!("error")),
        "重放后的 ACK 必须被接受（不得回错误）：{after:?}"
    );

    host.stop().await;
}
