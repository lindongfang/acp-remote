//! WP5 跨层验收（`session-activity-three-states` 任务 5.2）：三态合同在**重启、停止确认、握手协商与
//! 升级重放**四条跨层路径上的可观察行为。
//!
//! 四条判据（`openspec/changes/session-activity-three-states/specs/`）：
//!
//! 1. **崩溃恢复不重复副作用**：已受理但无法确认结果的命令在重启后终结为 `uncertain`，**不**重新
//!    派发；持久行与线上读回都是同一条终态（真实 SQLite + 真实 loopback listener）。
//! 2. **旧执行未确认停止不提升队列**：被放弃的 turn 继续占住执行槽位，已接受的后续 turn 保持
//!    `queued`；只有端点观测到旧 turn 的终态（停止确认）后才按 FIFO 派发下一轮。
//! 3. **新旧握手明确隔离**：未声明 `core.session-activity.v1` 的 Access 被真实 Owner 以
//!    `nodelink.protocol.feature_required`（4400）拒绝；声明它的握手把该 feature 选中。
//! 4. **升级重放与当前摘要分离**：v12 → v13 升级后，旧的 `session.updated` 载荷仍可经重放读回且
//!    逐字节/摘要不变，当前摘要只取三态，不被旧七态覆盖。
//!
//! 第 1、2 条用 `support::owner::OwnerNode`（真实存储 + 组合根同一批装配点 + 脚本化后端，故障注入只在
//! `SessionStore::commit`）；第 3 条用 `support::nodelink` 的 wire 客户端；第 4 条直接在真实
//! `SqliteStore` 上构造 v12 形状并读回复放结果——存储层已有 v13 七态映射用例，这里只补「旧事件原文与
//! 当前三态摘要并存」的应用层读回断言，不复制 DDL 判据。

mod support;

use std::path::Path;
use std::time::Duration;

use acp_core::model::{
    Actor, EventId, EventKind, EventOrigin, EventPayload, EventType, NodeId, OriginEpoch,
    PendingEvent, RequestId, SessionId, StoredPolicy, Timestamp, ViewJson,
};
use acp_core::ports::{
    NewSession, OwnedCommit, ReplayLimit, SessionQuery, SessionStore as _, StateChange,
};
use serde_json::{Value, json};
use server::local_admin::Method;
use sqlx::Connection as _;
use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::nodelink::{AccessKey, NodeLinkClient, PairingTicket};
use support::owner::OwnerNode;

/// 本文件扮演的 Access Node（与其余受控路径用例同一标识）。
const ACCESS_NODE: &str = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";
const AGENT: &str = "codex";

/// 重启前后固定节点身份的种子：`EphemeralKeystore` 只在进程内，不固定种子重启就会换 node id。
fn restart_seed() -> [u8; 32] {
    let mut seed = [0u8; 32];
    seed[31] = 7;
    seed
}

// ---------------------------------------------------------------------------------------------
// 配对 / 连接 / 命令体（与 `session_resume_e2e.rs` 同一批 wire 形状，不另立口径）
// ---------------------------------------------------------------------------------------------

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
            json!({
                "mode": "owner",
                "pairingUrl": null,
            }),
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

impl Paired {
    fn into_parts(self) -> (OwnerNode, AccessKey, PairingTicket, String) {
        (self.owner, self.access, self.ticket, self.directory_id)
    }
}

/// 真实 loopback 上的 WSS 握手（含 `catalogRevision` 验签）。
async fn connect(
    owner: &OwnerNode,
    access: &AccessKey,
    ticket: &PairingTicket,
    nonce_seed: u8,
) -> NodeLinkClient {
    let mut client = NodeLinkClient::connect_plain(owner.addr).await;
    support::nodelink::handshake(
        &mut client,
        access,
        ticket,
        &support::nodelink::nonce_text(nonce_seed),
    )
    .await;
    client
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

fn prompt_body(request_id: &str, session: &Value, attachment: &Value, marker: &str) -> Value {
    json!({
        "requestId": request_id,
        "command": "session.prompt",
        "sessionRef": session.clone(),
        "attachmentId": attachment["attachmentId"],
        "attachmentGeneration": attachment["attachmentGeneration"],
        "expectedVersion": null,
        "payload": { "content": [ { "type": "text", "text": marker } ] },
    })
}

fn command_status_body(request_id: &str, target: &str) -> Value {
    json!({
        "requestId": request_id,
        "command": "command.status",
        "sessionRef": null,
        "attachmentId": null,
        "attachmentGeneration": null,
        "expectedVersion": null,
        "payload": { "targetRequestId": target },
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

/// 真实存在的目录，且 `canonicalize` 结果与自身逐字相同（core 的 cwd 复校验要求）。
fn workspace_dir(label: &str) -> std::path::PathBuf {
    let root = std::fs::canonicalize(std::env::temp_dir()).expect("临时目录可规范化");
    let path = root.join(format!("acp-remote-wp5b-{label}"));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("建立工作目录");
    std::fs::canonicalize(&path).expect("新建目录可规范化")
}

/// 等一条**指定 requestId** 的命令终态（其余推送跳过）。
///
/// 终态观察循环按 250ms 轮询，直接 `expect("command.terminal")` 会把别的命令或 `resource.event`
/// 当成步骤失败；这里按 requestId 过滤，超时给出可读信息。
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
    }
}

async fn session_state(owner: &OwnerNode, session: &SessionId) -> String {
    let mut listed = owner
        .sessions
        .list(SessionQuery {
            only: Some(vec![session.clone()]),
            states: Vec::new(),
            limit: Some(1),
        })
        .await
        .expect("列出会话");
    assert_eq!(listed.len(), 1, "会话摘要必须存在");
    listed.pop().expect("会话摘要").state().as_str().to_owned()
}

/// 驱动合并窗口（与组合根的周期 pump 同一入口）。返回 `rounds` 次派发尝试。
async fn pump_rounds(owner: &OwnerNode, session: &SessionId, rounds: usize) {
    for _ in 0..rounds {
        owner.broker.pump(session).await.expect("驱动合并窗口");
    }
}

// ---------------------------------------------------------------------------------------------
// 1. 重启：uncertain 不重派
// ---------------------------------------------------------------------------------------------

/// 一次已受理的 prompt 在「Agent 未产出终态」时丢失连接并重启：启动恢复把它结为 `uncertain`，
/// 第二次启动的 Owner **一次都没有**再调用后端，持久行与线上 `command.status` 读回同一条终态。
///
/// 判别力：若恢复路径重新派发（再调一次 `prompt`），`restarted.prompt_log()` 非空 ⇒ 红；若把结果猜成
/// `completed`/`failed`，`command.status` 的 `status` 与重开存储读回的 `record.status()` 同时红；若重启
/// 后摘要不是三态（例如仍停在 `running`），摘要断言红。
#[test]
fn a_restart_settles_an_accepted_prompt_as_uncertain_without_redispatch() {
    support::block_on(async {
        let workspace = workspace_dir("restart-uncertain");
        // 临时根的所有权留在用例：两轮 Owner 共用同一个 `data_dir`（重启而不是新建）。
        let root = support::TempRoot::new("activity-restart-uncertain");
        let first = OwnerNode::start_in_root(root.path(), restart_seed()).await;
        let owner_node_id = first.owner_node_id().clone();
        let paired = pair(first, &workspace).await;
        let (owner, access, ticket, directory_id) = paired.into_parts();
        let mut client = connect(&owner, &access, &ticket, 0x41).await;
        let (session, attachment) = create_session(&mut client, &directory_id).await;
        let session_id = session["sessionId"].as_str().expect("sessionId").to_owned();
        let session_key = SessionId::new(&session_id).expect("session id");

        // 受理一条 prompt：脚本化端点拿到 `prompt` 但暂不产出任何事件——这就是「结果无法确认」的点。
        let request = support::nodelink::uuid_text();
        let marker = "restart-uncertain-marker";
        client.step("accepted prompt before restart");
        client
            .send(
                "command.submit",
                prompt_body(&request, &session, &attachment, marker),
            )
            .await;
        let accepted = client.expect("command.accepted").await;
        assert!(
            accepted["body"]["result"]["turnId"].is_string(),
            "mutation 必须先回 accepted 并带 turnId"
        );
        assert_eq!(
            owner.prompt_log(),
            vec![marker.to_owned()],
            "第一轮必须真的派发过一次（否则本用例的前置没有成立）"
        );
        assert_eq!(
            session_state(&owner, &session_key).await,
            "running",
            "已受理且未终结的任务让会话处于运行中"
        );

        // 崩溃：停止 Owner，但**不**跑启动恢复（模拟进程消失时的状态）。
        drop(client);
        owner.stop().await;

        // 同一 data_dir 上重启：固定身份种子让 node id / 公钥不变。
        let restarted = OwnerNode::start_in_root(root.path(), restart_seed()).await;
        assert_eq!(
            restarted.owner_node_id(),
            &owner_node_id,
            "重启必须还是同一个节点（身份由固定种子恢复）"
        );
        // 组合根启动时调用的同一入口。
        let settled = restarted
            .core
            .recover_unsettled(&Actor::LocalCli, ReplayLimit::new(16))
            .await
            .expect("启动恢复");
        assert_eq!(settled, 1, "恰好一条未终结命令被终结");
        assert!(
            restarted.prompt_log().is_empty(),
            "恢复不得重新派发副作用：{:?}",
            restarted.prompt_log()
        );
        assert_eq!(
            restarted.resume_probe.calls(),
            0,
            "恢复不得自动恢复 Agent 会话（那也是副作用）"
        );
        assert_eq!(
            session_state(&restarted, &session_key).await,
            "idle",
            "旧执行确认停止且没有后续任务时会话必须重算为空闲"
        );

        // 持久行：另开一个真实 store 读回（不只在进程内存里）。
        let reopened = restarted.reopen_store().await;
        let node = NodeId::new(ACCESS_NODE).expect("node id");
        let actor = Actor::Node {
            node: node.clone(),
            access_node: node,
        };
        let record = reopened
            .find_request(&RequestId::new(&request).expect("request id"), &actor)
            .await
            .expect("读持久记录")
            .expect("恢复后的终态必须在持久行里");
        assert_eq!(
            record.status().as_str(),
            "uncertain",
            "重启恢复只能结为 uncertain，不得猜成功或失败"
        );
        assert_eq!(
            record.command(),
            "session.prompt",
            "持久记录必须是那条 prompt"
        );
        reopened.close().await;

        // 线上读回：同一批配对信任在重启后仍有效，`command.status` 给出同一条 uncertain 终态。
        let mut after = connect(&restarted, &access, &ticket, 0x43).await;
        after.step("command.status after restart");
        after
            .send(
                "command.submit",
                command_status_body(&support::nodelink::uuid_text(), &request),
            )
            .await;
        let reread = after.expect("command.terminal").await;
        assert_eq!(
            reread["body"]["command"],
            json!("session.prompt"),
            "command.status 的 `command` 是目标命令名：{reread}"
        );
        assert_eq!(
            reread["body"]["terminal"]["status"],
            json!("uncertain"),
            "重启后线上读回的终态必须仍是 uncertain：{reread}"
        );
        assert!(
            !reread["body"]["terminal"]["error"].is_null(),
            "uncertain 必须带结构化原因：{reread}"
        );
        assert!(
            reread["body"]["terminal"]["terminalEventId"].is_string(),
            "恢复必须写持久终态事件：{reread}"
        );

        // 只有那一轮 turn，没有因重派产生的第二轮。
        let view = restarted.sessions.read_view().await.expect("读视图");
        let page = view
            .read_session(acp_core::ports::HistoryQuery {
                session: session_key.clone(),
                include: acp_core::ports::HistoryInclude {
                    messages: false,
                    turns: true,
                    pending_interactions: false,
                    config_options: false,
                    capabilities: false,
                    history: true,
                },
                before_history: None,
                before_messages: None,
                messages_limit: ReplayLimit::new(0),
                after: None,
                limit: ReplayLimit::new(32),
            })
            .await
            .expect("读会话");
        let started = page
            .history
            .iter()
            .filter(|record| record.event.event_type.as_str() == "turn.started")
            .count();
        assert_eq!(started, 1, "重启恢复不得产生第二个已派发 turn");
        assert_eq!(page.turns.len(), 1, "会话只能有那一轮 turn");
        drop(view);

        drop(after);
        restarted.stop().await;
    });
}

// ---------------------------------------------------------------------------------------------
// 2. 停止确认前不提升队列
// ---------------------------------------------------------------------------------------------

/// 落盘失败交出的 turn 在端点观测到终态前继续占住执行槽位：已接受的两条后续 turn 保持 `queued`，
/// 多次驱动合并窗口也不得派发；旧 turn 的终态到达（停止确认）后，FIFO 的下一轮先跑，再下一轮才跑。
///
/// 判别力：若 `dispatch_one` 忽略占位，`prompt_log` 会在释放终态前就出现第二条 ⇒ 红；若占位没有随
/// 终态释放，确认后的 `prompt_log` 不会增长 ⇒ 红；若 FIFO 顺序被颠倒，日志顺序与 B/C 的先后不符 ⇒ 红。
#[test]
fn an_unconfirmed_stop_keeps_the_queue_blocked_until_the_old_turn_is_observed() {
    support::block_on(async {
        let workspace = workspace_dir("queue-hold");
        let owner = OwnerNode::start("activity-queue-hold").await;
        let paired = pair(owner, &workspace).await;
        let (owner, access, ticket, directory_id) = paired.into_parts();
        let mut client = connect(&owner, &access, &ticket, 0x41).await;
        let (session, attachment) = create_session(&mut client, &directory_id).await;
        let session_id = session["sessionId"].as_str().expect("sessionId").to_owned();
        let session_key = SessionId::new(&session_id).expect("session id");

        // 第一轮：受理后让它的**非终态批**落盘失败 ⇒ turn 被放弃，但端点还没观测到终态。
        let request_a = support::nodelink::uuid_text();
        let marker_a = "queue-hold-marker-a";
        client.step("first prompt");
        client
            .send(
                "command.submit",
                prompt_body(&request_a, &session, &attachment, marker_a),
            )
            .await;
        client.expect("command.accepted").await;
        assert_eq!(owner.prompt_log(), vec![marker_a.to_owned()]);

        owner.fail_commits_with(Some(marker_a));
        assert_eq!(
            owner.release_event_prefix(marker_a, 1),
            1,
            "只交付非终态批（delta）"
        );
        pump_rounds(&owner, &session_key, 2).await;
        assert!(
            owner
                .rejected_commits
                .load(std::sync::atomic::Ordering::SeqCst)
                >= 1,
            "故障必须真的命中那次提交"
        );
        owner.fail_commits_with(None);
        let terminal_a = take_terminal(&mut client, &request_a).await;
        assert_eq!(
            terminal_a["body"]["terminal"]["status"],
            json!("uncertain"),
            "落盘失败的那一轮只能以 uncertain 收尾：{terminal_a}"
        );
        assert_eq!(
            session_state(&owner, &session_key).await,
            "running",
            "旧执行尚未确认停止，摘要不得声称空闲"
        );

        // 两条后续 turn 被持久接受并排队。
        let request_b = support::nodelink::uuid_text();
        let marker_b = "queue-hold-marker-b";
        let request_c = support::nodelink::uuid_text();
        let marker_c = "queue-hold-marker-c";
        client.step("queued second prompt");
        client
            .send(
                "command.submit",
                prompt_body(&request_b, &session, &attachment, marker_b),
            )
            .await;
        client.expect("command.accepted").await;
        client.step("queued third prompt");
        client
            .send(
                "command.submit",
                prompt_body(&request_c, &session, &attachment, marker_c),
            )
            .await;
        client.expect("command.accepted").await;

        // 占位期间反复驱动合并窗口：派发尝试会推进占位轮次，但远未到兜底轮次。
        pump_rounds(&owner, &session_key, 10).await;
        assert_eq!(
            owner.prompt_log(),
            vec![marker_a.to_owned()],
            "停止未确认前不得派发任何排队 turn：{:?}",
            owner.prompt_log()
        );
        let queued = owner
            .sessions
            .queued_turns(&session_key)
            .await
            .expect("读持久队列");
        assert_eq!(queued.len(), 2, "两条后续 turn 必须仍留在持久队列里");
        assert_eq!(
            session_state(&owner, &session_key).await,
            "running",
            "排队 turn 计入未结束任务：会话保持运行中"
        );

        // 停止确认：端点观测到旧 turn 的终态，占位释放。
        assert_eq!(
            owner.release_event_prefix(marker_a, 1),
            1,
            "交付旧 turn 的终态事件（停止确认）"
        );
        pump_rounds(&owner, &session_key, 2).await;
        assert_eq!(
            owner.prompt_log(),
            vec![marker_a.to_owned(), marker_b.to_owned()],
            "确认停止后必须按 FIFO 提升下一轮：{:?}",
            owner.prompt_log()
        );
        let remaining = owner
            .sessions
            .queued_turns(&session_key)
            .await
            .expect("读持久队列");
        assert_eq!(remaining.len(), 1, "第二轮运行时只应剩最后一条排队 turn");

        // B 完成 → C 才被提升。
        assert_eq!(
            owner.release_events(marker_b),
            2,
            "派发 B 之后才有它的脚本事件"
        );
        pump_rounds(&owner, &session_key, 2).await;
        let terminal_b = take_terminal(&mut client, &request_b).await;
        assert_eq!(terminal_b["body"]["terminal"]["status"], json!("completed"));
        pump_rounds(&owner, &session_key, 2).await;
        assert_eq!(
            owner.prompt_log(),
            vec![
                marker_a.to_owned(),
                marker_b.to_owned(),
                marker_c.to_owned()
            ],
            "B 结束后才轮到 C：{:?}",
            owner.prompt_log()
        );

        assert_eq!(owner.release_events(marker_c), 2);
        pump_rounds(&owner, &session_key, 2).await;
        let terminal_c = take_terminal(&mut client, &request_c).await;
        assert_eq!(terminal_c["body"]["terminal"]["status"], json!("completed"));
        assert_eq!(
            session_state(&owner, &session_key).await,
            "idle",
            "全部任务结束后会话为空闲"
        );

        drop(client);
        owner.stop().await;
    });
}

// ---------------------------------------------------------------------------------------------
// 3. 新旧握手拒绝
// ---------------------------------------------------------------------------------------------

/// 未声明三态合同的 Access 在真实 Owner 上以 `nodelink.protocol.feature_required` + 4400 被拒绝，
/// `details.features` 指名缺的就是 `core.session-activity.v1`；声明后同一 Owner 的握手把它选中。
///
/// 判别力：若必需 feature 未登记（或拒绝后仍继续挑战），第一条连接会收到 `node.challenge` 而不是
/// `link.error` ⇒ 红；若 Owner 忘记把该 feature 放进协商交集，第二次握手的 `selectedFeatures` 不含它 ⇒ 红。
#[test]
fn a_peer_without_the_three_state_contract_is_rejected_and_one_with_it_negotiates() {
    support::block_on(async {
        let workspace = workspace_dir("handshake");
        let owner = OwnerNode::start("activity-handshake").await;
        let paired = pair(owner, &workspace).await;
        let (owner, access, ticket, _directory_id) = paired.into_parts();

        // 旧客户端：除三态合同外与当前客户端声明同一批 feature。
        let mut old = NodeLinkClient::connect_plain(owner.addr).await;
        old.step("node.hello without core.session-activity.v1");
        old.send_pre_auth(
            "node.hello",
            json!({
                "minProtocolVersion": 1,
                "maxProtocolVersion": 1,
                "accessNodeId": access.node_id(),
                "role": "access",
                "clientNonce": support::nodelink::nonce_text(0x51),
                "supportedFeatures": [
                    "node-link.core.v1",
                    "node-link.raw-acp.v1",
                    "node-link.command-status.v1",
                    "node-link.session-create.v1",
                    "node-link.export-revoke.v1"
                ],
                "requiredFeatures": ["node-link.core.v1"]
            }),
        )
        .await;
        let error = old.expect("link.error").await;
        assert_eq!(
            error["body"]["code"],
            json!("nodelink.protocol.feature_required"),
            "缺必需 feature 必须明确拒绝：{error}"
        );
        assert_eq!(
            error["body"]["details"]["features"],
            json!(["core.session-activity.v1"]),
            "details 必须指名缺少的三态合同：{error}"
        );
        assert_eq!(old.close_code().await, 4400, "协议协商失败按 4400 关闭");
        drop(old);

        // 新客户端：同一声明加三态合同，握手成功并选中它。
        let mut client = NodeLinkClient::connect_plain(owner.addr).await;
        let ready = support::nodelink::handshake(
            &mut client,
            &access,
            &ticket,
            &support::nodelink::nonce_text(0x52),
        )
        .await;
        let selected: Vec<&str> = ready["selectedFeatures"]
            .as_array()
            .expect("selectedFeatures")
            .iter()
            .map(|feature| feature.as_str().expect("feature id"))
            .collect();
        assert!(
            selected.contains(&"core.session-activity.v1"),
            "声明三态合同的握手必须选中它：{selected:?}"
        );
        client
            .send("catalog.subscribe", json!({ "knownRevision": null }))
            .await;
        client.expect("catalog.snapshot").await;
        drop(client);

        owner.stop().await;
    });
}

// ---------------------------------------------------------------------------------------------
// 4. v12 升级 + 旧 session.updated 重放
// ---------------------------------------------------------------------------------------------

/// 在真实 `SqliteStore` 上构造 v12 形状（加回 `closed_at` 并把 `user_version`/meta 降到 12），写入一条
/// 携带旧七态 `waiting_permission` 的 `session.updated`，再用真实迁移升级到 v13：
///
/// - 旧载荷经 `replay` + `event_payload` 逐字节读回，`payload_digest`、全局/会话/origin 序号一字不差；
/// - 当前摘要只有三态（`idle`，而不是旧载荷里的 `waiting_permission`），旧事件不覆盖权威摘要；
/// - `closed_at` 被 v13 删除（升级确实发生了），事件行数不变。
///
/// 判别力：若 v13 重写了事件行（重新序列化或改摘要）⇒ 字节/摘要断言红；若旧 `session.updated` 被当作
/// 实时摘要来源 ⇒ 状态断言红；若迁移没有执行 ⇒ `closed_at` 仍在、版本不是 13 ⇒ 红。
#[test]
fn a_v12_database_keeps_its_old_session_updated_payload_readable_after_upgrade() {
    support::block_on(async {
        let root = support::TempRoot::new("activity-upgrade-replay");
        let data_dir = root.join("data");
        support::create_owner_only_dir(&data_dir);
        let at = Timestamp::new("2026-10-20T00:00:00.000Z").expect("timestamp");
        let epoch = OriginEpoch::new("6f2a1f8e-7b1c-4a8f-9b0d-1c2d3e4f5a6b").expect("epoch");

        let store = SqliteStore::open(StorageConfig::new(&data_dir), &at)
            .await
            .expect("打开当前形状的库");
        let session = store
            .commit(OwnedCommit {
                session: None,
                at: at.clone(),
                expected_version: None,
                state: Some(StateChange::Create(NewSession {
                    title: Some("旧摘要会话".to_owned()),
                    agent: acp_core::model::AgentRef::try_new(
                        acp_core::model::AgentId::new(AGENT).expect("agent id"),
                        "Codex",
                    )
                    .expect("agent ref"),
                })),
                turns: Vec::new(),
                events: Vec::new(),
                interactions: Vec::new(),
                compacted: Vec::new(),
                idempotency: None,
                command_terminal: None,
                origin_epoch: Some(epoch.clone()),
            })
            .await
            .expect("创建会话")
            .session_id
            .expect("session id");

        // 旧七态载荷（升级前的 `session.updated` 形状：主状态仍是 `waiting_permission`）。
        let old_view = format!(
            r#"{{"session":{{"sessionId":"{}","title":"旧摘要会话","state":"waiting_permission","closedAt":null,"version":"1"}}}}"#,
            session.as_str()
        );
        let committed = store
            .commit(OwnedCommit {
                session: Some(session.clone()),
                at: at.clone(),
                expected_version: None,
                state: None,
                turns: Vec::new(),
                events: vec![PendingEvent::new(
                    EventKind::State,
                    EventType::new("session.updated").expect("event type"),
                    StoredPolicy::Durable,
                    EventPayload::new(ViewJson::new(&old_view).expect("view"), None),
                    EventOrigin::Daemon,
                    None,
                    None,
                )],
                interactions: Vec::new(),
                compacted: Vec::new(),
                idempotency: None,
                command_terminal: None,
                origin_epoch: Some(epoch),
            })
            .await
            .expect("追加旧 session.updated");
        let event_id = committed.appended[0].id.clone();
        let sequence_before = (
            committed.appended[0].global_sequence.get(),
            committed.appended[0]
                .session_sequence
                .expect("session sequence")
                .get(),
            committed.appended[0]
                .origin_sequence
                .expect("origin sequence")
                .get(),
        );

        // 升级前的读回：原文逐字相同。
        let view = store.read_view().await.expect("读视图");
        let payload = view
            .event_payload(&event_id)
            .await
            .expect("读正文")
            .expect("旧事件正文必须在升级前可读");
        assert_eq!(
            payload.view.as_str(),
            old_view.as_str(),
            "升级前读回的 view 必须就是写入的字节"
        );
        drop(view);
        store.close().await;

        let digest_before = raw_event(&data_dir, &event_id).await;

        // 构造 v12 形状：v13 段的触发条件是「存在 closed_at」。
        let mut raw = raw_connection(&data_dir).await;
        sqlx::raw_sql("ALTER TABLE owned_session ADD COLUMN closed_at TEXT;")
            .execute(&mut raw)
            .await
            .expect("补回 v13 之前的 closed_at");
        sqlx::query("UPDATE meta SET value = '12' WHERE key = 'owned_schema_version'")
            .execute(&mut raw)
            .await
            .expect("把 owned schema 版本降到 12");
        sqlx::query("PRAGMA user_version = 12")
            .execute(&mut raw)
            .await
            .expect("把文件格式版本降到 12");
        let _ = raw.close().await;

        // 真实升级（组合根打开存储时的同一入口）。
        let upgraded = SqliteStore::open(StorageConfig::new(&data_dir), &at)
            .await
            .expect("v12 库必须升级到当前版本");
        assert_eq!(
            upgraded.metadata().owned_schema_version,
            16,
            "升级必须把 owned schema 推进到当前版本 v16"
        );

        // 当前摘要：只有三态，且不被旧事件里的 `waiting_permission` 覆盖。
        let mut listed = upgraded
            .list(SessionQuery {
                only: Some(vec![session.clone()]),
                states: Vec::new(),
                limit: Some(1),
            })
            .await
            .expect("列出会话");
        let summary = listed.pop().expect("会话摘要");
        assert_eq!(
            summary.state().as_str(),
            "idle",
            "旧 session.updated 不得把当前摘要改回七态"
        );
        assert!(
            matches!(
                summary.state(),
                acp_core::model::SessionState::Idle
                    | acp_core::model::SessionState::Running
                    | acp_core::model::SessionState::Pending
            ),
            "当前摘要必须落在三态取值域里"
        );

        // 重放：同一事件定位与序号，正文逐字节不变。
        let view = upgraded.read_view().await.expect("升级后读视图");
        let batch = view.replay(None, ReplayLimit::new(64)).await.expect("重放");
        let replayed = batch
            .events
            .iter()
            .find(|event| event.id == event_id)
            .cloned()
            .expect("旧 session.updated 必须出现在重放里");
        assert_eq!(
            (
                replayed.global_sequence.get(),
                replayed.session_sequence.expect("session sequence").get(),
                replayed.origin_sequence.expect("origin sequence").get(),
            ),
            sequence_before,
            "升级不得重编号事件"
        );
        assert_eq!(
            replayed.event_type.as_str(),
            "session.updated",
            "重放的事件类型不得改变"
        );
        let payload = view
            .event_payload(&event_id)
            .await
            .expect("升级后读正文")
            .expect("旧事件正文必须在升级后可读");
        assert_eq!(
            payload.view.as_str(),
            old_view.as_str(),
            "升级不得重写旧 session.updated 的原文"
        );
        drop(view);

        let digest_after = raw_event(&data_dir, &event_id).await;
        assert_eq!(
            digest_after, digest_before,
            "升级不得改事件的 payload_json / 摘要 / 序号"
        );

        // 升级确实执行了：closed_at 被 v13 删除。
        let mut raw = raw_connection(&data_dir).await;
        let columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('owned_session')")
                .fetch_all(&mut raw)
                .await
                .expect("读列清单");
        assert!(
            !columns.iter().any(|name| name == "closed_at"),
            "v13 必须删除 closed_at（否则升级没有真的发生）"
        );
        let user_version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&mut raw)
            .await
            .expect("读 user_version");
        assert_eq!(user_version, 16, "升级后的文件格式版本必须是 16");
        let _ = raw.close().await;

        let _ = upgraded.close().await;
    });
}

/// 原始库连接（升级前后对照用；不经过任何端口）。
async fn raw_connection(data_dir: &Path) -> sqlx::SqliteConnection {
    let config = StorageConfig::new(data_dir);
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(config.database_path())
        .create_if_missing(false)
        .busy_timeout(Duration::from_secs(10));
    sqlx::Connection::connect_with(&options)
        .await
        .expect("打开同一 data_dir 的原始 SQL 连接")
}

/// `(payload_json, payload_digest, global_sequence, session_sequence, origin_sequence)` 的逐字快照。
async fn raw_event(data_dir: &Path, event: &EventId) -> (String, String, i64, i64, i64) {
    let mut connection = raw_connection(data_dir).await;
    let row: (String, String, i64, i64, i64) = sqlx::query_as(
        "SELECT payload_json, payload_digest, global_sequence, session_sequence, origin_sequence \
         FROM owned_event WHERE event_id = ?1",
    )
    .bind(event.as_str())
    .fetch_one(&mut connection)
    .await
    .expect("读事件行");
    connection.close().await.expect("关闭原始连接");
    row
}
