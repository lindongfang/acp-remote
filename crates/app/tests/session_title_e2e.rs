//! `session.title.set` 的 Node Link Owner 面验收（变更 `custom-session-title` 任务 2.2）。
//!
//! 组合方式与 `session_activity_cross_layer.rs` 相同：**真实** `server::node_link` listener（
//! `support::owner::OwnerNode`，真实 SQLite / 身份 / 命令路由与终态观察者）+ **真实** wire 客户端
//! （`support::nodelink`，含 catalogRevision 验签与序号）。
//!
//! 两条判据：
//!
//! 1. 已协商 `core.session-title.v1`：命令在**一次**应答里终结为 `completed`，`terminalEventId` 非空，
//!    Owner 落盘 trim 后的自定义标题并写一条带有效标题的 `session.updated`；
//! 2. 未协商：`command.rejected(nodelink.protocol.feature_required)` + `details.features`，且零写入
//!    （机制上连 sessionRef/attachment 都还没被解析）。

mod support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};
use server::local_admin::Method;
use support::nodelink::{AccessKey, NodeLinkClient, PairingTicket};
use support::owner::OwnerNode;

/// 本文件扮演的 Access Node。
const ACCESS_NODE: &str = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";
/// 脚本化 agent selector（`support::owner` 的脚本后端按它供数）。
const AGENT: &str = "codex";
/// Access 声明的 feature（除 `core.session-title.v1` 外的同一套）。
const DECLARED_WITHOUT_TITLE: [&str; 6] = [
    "node-link.core.v1",
    "node-link.raw-acp.v1",
    "node-link.command-status.v1",
    "node-link.session-create.v1",
    "node-link.export-revoke.v1",
    "core.session-activity.v1",
];

struct Paired {
    owner: OwnerNode,
    access: AccessKey,
    ticket: PairingTicket,
    directory_id: String,
}

/// 走到「status approved」为止：注册目录 → `node.pair.begin` → Access claim → 本地确认 → status approved。
async fn pair(owner: OwnerNode, workspace_root: &Path) -> Paired {
    let directory_id = support::seed_directory(&owner.data_dir(), workspace_root).await;
    let access = AccessKey::new(ACCESS_NODE);
    let begin = owner
        .admin(
            Method::NodePairBegin,
            json!({ "mode": "owner", "pairingUrl": null }),
        )
        .await;
    let pairing_id = begin["pairingId"].as_str().expect("pairingId").to_owned();
    let ticket = support::nodelink::ticket_from_url(
        begin["pairingUrl"].as_str().expect("pairingUrl"),
        &pairing_id,
    );
    let client_nonce = support::nodelink::nonce_text(0x41);
    let claim = support::nodelink::claim_request(&ticket, &access, &client_nonce);
    let reply = support::nodelink::post_json(
        owner.addr,
        server::node_link::CLAIM_PATH,
        &serde_json::to_vec(&claim).expect("claim 可序列化"),
    )
    .await;
    assert_eq!(reply.status, 201, "合法 claim 必须 201");
    let body = reply.json();
    support::nodelink::verify_owner_proof(&ticket, &body, &access, &client_nonce);

    owner
        .admin(Method::NodePairConfirm, json!({ "pairingId": pairing_id }))
        .await;

    let status = support::nodelink::status_request(
        &ticket,
        &access,
        body["pairingRequestId"].as_str().expect("pairingRequestId"),
        &support::nodelink::nonce_text(0x42),
    );
    let reply = support::nodelink::post_json(
        owner.addr,
        server::node_link::STATUS_PATH,
        &serde_json::to_vec(&status).expect("status 可序列化"),
    )
    .await;
    assert_eq!(reply.json()["status"], json!("approved"));

    Paired {
        owner,
        access,
        ticket,
        directory_id,
    }
}

/// 真实 loopback 上的 WSS 握手；`declared` 决定本次协商的 feature 交集。
async fn connect(
    owner: &OwnerNode,
    access: &AccessKey,
    ticket: &PairingTicket,
    nonce_seed: u8,
    declared: &[&str],
) -> (NodeLinkClient, Vec<String>) {
    let mut client = NodeLinkClient::connect_plain(owner.addr).await;
    let ready = support::nodelink::handshake_with_features(
        &mut client,
        access,
        ticket,
        &support::nodelink::nonce_text(nonce_seed),
        declared,
    )
    .await;
    let selected = ready["selectedFeatures"]
        .as_array()
        .expect("selectedFeatures")
        .iter()
        .map(|feature| feature.as_str().expect("feature id").to_owned())
        .collect();
    (client, selected)
}

fn session_create_body(request_id: &str, directory_id: &str) -> Value {
    json!({
        "requestId": request_id,
        "command": "session.create",
        "sessionRef": null,
        "attachmentId": null,
        "attachmentGeneration": null,
        "expectedVersion": null,
        "payload": { "agentId": AGENT, "directoryId": directory_id },
    })
}

/// `session.title.set` 的 wire body（§12.7 的 closed payload `{ title }`）。
fn title_body(request_id: &str, session: &Value, attachment: &Value, title: &str) -> Value {
    json!({
        "requestId": request_id,
        "command": "session.title.set",
        "sessionRef": session.clone(),
        "attachmentId": attachment["attachmentId"],
        "attachmentGeneration": attachment["attachmentGeneration"],
        "expectedVersion": null,
        "payload": { "title": title },
    })
}

async fn attach(client: &mut NodeLinkClient, session: &Value) -> Value {
    client
        .send(
            "resource.attach",
            json!({ "remoteSessionRef": session.clone() }),
        )
        .await;
    let attached = client.expect("resource.attached").await;
    json!({
        "attachmentId": attached["body"]["attachmentId"],
        "attachmentGeneration": attached["body"]["attachmentGeneration"],
    })
}

async fn create_session(client: &mut NodeLinkClient, directory_id: &str) -> (Value, Value) {
    client.step("session.create");
    client
        .send(
            "command.submit",
            session_create_body(&support::nodelink::uuid_text(), directory_id),
        )
        .await;
    let accepted = client.expect("command.accepted").await;
    assert_eq!(accepted["body"]["result"], Value::Null);
    let terminal = client.expect("command.terminal").await;
    assert_eq!(terminal["body"]["terminal"]["status"], json!("completed"));
    let session = terminal["body"]["terminal"]["result"]["remoteSessionRef"].clone();
    let attachment = attach(client, &session).await;
    (session, attachment)
}

/// 等一条**指定 requestId** 的命令终态（跳过别的推送与事件）。
async fn take_terminal(client: &mut NodeLinkClient, request: &str) -> Value {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let message = match tokio::time::timeout_at(deadline, client.take()).await {
            Ok(message) => message,
            Err(_) => panic!("10 秒内没有等到 {request} 的命令终态"),
        };
        if message["type"] == json!("command.terminal")
            && message["body"]["requestId"] == json!(request)
        {
            return message;
        }
        if message["type"] == json!("command.rejected")
            && message["body"]["requestId"] == json!(request)
        {
            return message;
        }
    }
}

/// 真实存在的目录，且规范化结果与自身逐字相同（core 的 cwd 复校验要求）。
fn workspace_dir(label: &str) -> PathBuf {
    let root = std::fs::canonicalize(std::env::temp_dir()).expect("临时目录可规范化");
    let path = root.join(format!("acp-remote-title-{label}"));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("建立工作目录");
    std::fs::canonicalize(&path).expect("新建目录可规范化")
}

/// 从 `sessionRef.remoteSessionRef` 取 core 的会话标识。
fn session_id(session: &Value) -> String {
    session["sessionId"]
        .as_str()
        .expect("remoteSessionRef.sessionId")
        .to_owned()
}

/// 直接读库：`owned_session` 的 `(title, custom_title)`。
async fn stored_titles(owner: &OwnerNode, session: &str) -> (Option<String>, Option<String>) {
    let mut connection = raw_connection(owner).await;
    let row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT title, custom_title FROM owned_session WHERE session_id = ?1",
    )
    .bind(session)
    .fetch_one(&mut connection)
    .await
    .expect("读 owned_session.title/custom_title");
    drop(connection);
    row
}

/// 直接读库：该会话 `session.updated` 事件的载荷（按会话内序号升序）。
async fn session_updated_payloads(owner: &OwnerNode, session: &str) -> Vec<Value> {
    let mut connection = raw_connection(owner).await;
    let rows = sqlx::query_as::<_, (String,)>(
        "SELECT payload_json FROM owned_event \
         WHERE session_id = ?1 AND event_type = 'session.updated' ORDER BY session_sequence ASC",
    )
    .bind(session)
    .fetch_all(&mut connection)
    .await
    .expect("读 session.updated");
    drop(connection);
    rows.into_iter()
        .map(|(payload,)| serde_json::from_str(&payload).expect("事件载荷是 JSON"))
        .collect()
}

async fn raw_connection(owner: &OwnerNode) -> sqlx::SqliteConnection {
    let config = storage_sqlite::migrate::StorageConfig::new(owner.data_dir());
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(config.database_path())
        .create_if_missing(false)
        .busy_timeout(Duration::from_secs(10));
    sqlx::Connection::connect_with(&options)
        .await
        .expect("打开同一 data_dir 的原始 SQL 连接")
}

/// 已协商 `core.session-title.v1`：一次应答即终态，`terminalEventId` 非空，Owner 落盘 trim 后的标题
/// 并写带有效标题的 `session.updated`。
#[test]
fn a_negotiated_title_set_completes_and_updates_the_summary() {
    support::block_on(async {
        let workspace = workspace_dir("negotiated");
        let paired = pair(OwnerNode::start("title-negotiated").await, &workspace).await;
        let Paired {
            owner,
            access,
            ticket,
            directory_id,
        } = paired;
        let (mut client, selected) = connect(
            &owner,
            &access,
            &ticket,
            0x51,
            &support::nodelink::DECLARED_FEATURES,
        )
        .await;
        assert!(
            selected
                .iter()
                .any(|feature| feature == "core.session-title.v1"),
            "本次握手必须选中 title feature：{selected:?}"
        );
        let (session, attachment) = create_session(&mut client, &directory_id).await;
        let session_id = session_id(&session);

        let request = support::nodelink::uuid_text();
        client.step("session.title.set");
        client
            .send(
                "command.submit",
                title_body(&request, &session, &attachment, "  我的标题  "),
            )
            .await;
        let terminal = take_terminal(&mut client, &request).await;
        assert_eq!(terminal["type"], json!("command.terminal"), "{terminal}");
        let body = &terminal["body"];
        assert_eq!(body["terminal"]["status"], json!("completed"), "{terminal}");
        assert!(
            body["terminal"]["terminalEventId"].is_string(),
            "终态必须带 Owner 的 terminalEventId：{terminal}"
        );
        assert_eq!(body["terminal"]["error"], Value::Null, "{terminal}");

        // 落库：自定义标题是 trim 后的文本；Agent 标题列保持不变。
        let (agent_title, custom_title) = stored_titles(&owner, &session_id).await;
        assert_eq!(
            custom_title.as_deref(),
            Some("我的标题"),
            "{custom_title:?}"
        );
        assert_eq!(agent_title, None, "Agent 标题列不受用户命令影响");

        // 事件：`session.updated` 摘要携带有效标题。
        let payloads = session_updated_payloads(&owner, &session_id).await;
        assert!(
            payloads
                .iter()
                .any(|payload| payload["session"]["title"] == json!("我的标题")),
            "session.updated 必须携带有效标题：{payloads:?}"
        );

        owner.stop().await;
    });
}

/// 未协商 `core.session-title.v1`：`command.rejected(nodelink.protocol.feature_required)` +
/// `details.features`，且零写入（标题列仍为 NULL、没有新增 `session.updated`）。
#[test]
fn an_unnegotiated_title_set_is_rejected_without_a_write() {
    support::block_on(async {
        let workspace = workspace_dir("unnegotiated");
        let paired = pair(OwnerNode::start("title-unnegotiated").await, &workspace).await;
        let Paired {
            owner,
            access,
            ticket,
            directory_id,
        } = paired;
        let (mut client, selected) =
            connect(&owner, &access, &ticket, 0x52, &DECLARED_WITHOUT_TITLE).await;
        assert!(
            !selected
                .iter()
                .any(|feature| feature == "core.session-title.v1"),
            "本连接不得选中 title feature：{selected:?}"
        );
        let (session, attachment) = create_session(&mut client, &directory_id).await;
        let session_id = session_id(&session);
        let before = session_updated_payloads(&owner, &session_id).await.len();

        let request = support::nodelink::uuid_text();
        client.step("未协商 feature 的 session.title.set");
        client
            .send(
                "command.submit",
                title_body(&request, &session, &attachment, "不该落盘"),
            )
            .await;
        let rejected = take_terminal(&mut client, &request).await;
        assert_eq!(rejected["type"], json!("command.rejected"), "{rejected}");
        assert_eq!(
            rejected["body"]["error"]["code"],
            json!("nodelink.protocol.feature_required"),
            "{rejected}"
        );
        assert_eq!(
            rejected["body"]["error"]["details"]["features"],
            json!(["core.session-title.v1"]),
            "{rejected}"
        );

        let (_, custom_title) = stored_titles(&owner, &session_id).await;
        assert_eq!(custom_title, None, "未协商时不得写入自定义标题");
        assert_eq!(
            session_updated_payloads(&owner, &session_id).await.len(),
            before,
            "未协商时不得追加 session.updated"
        );

        owner.stop().await;
    });
}
