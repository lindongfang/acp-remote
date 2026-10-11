//! R7、R22–R37：`session.resume` 在**真实 Node Link 管线**上的受控路径（R22–R37 的入口证据）。
//!
//! 入口与既有 `node_link_e2e.rs` 同源且**不是** mock 绕过：
//!
//! - 配对 → claim → 本地确认（真实 `LocalAdminRouter`）→ status approved → WSS 握手
//!   → `resource.attach` → `command.submit` 全部走**真实 loopback listener**上的真实报文；
//! - Owner 侧用真实 SQLite、真实 `identity_auth::Authority`、组合根自己的装配点
//!   （`app::daemon::{net_config, register_node_link_paths}`）；
//! - 唯一的替身是 Agent 后端（脚本化端点，不启动进程）与 `SessionStore` 的故障注入装饰器，
//!   两者都在 `support/owner.rs` 的模块注释里声明。
//!
//! **复用声明**：既有 `crates/server/src/node_link/command/tests.rs`（WP6 的内联用例）已在同一受控
//! 路由上覆盖 R27/R28/R29/R31/R33/R34/R36/R37 的**路由级**断言，以及 R25 的「目录被删除」变体。
//! 本文件**不复制**那些断言，只补它们没有的三个维度：
//!
//! 1. **R22** 的恢复列字节不变（既有 `storage-sqlite` 用例在存储层证明，本文件证明它经真实 wire 管线
//!    同样成立，成功与失败两条路径都不改写）；
//! 2. **R23/R25/R26** 的目录复校验维度（别名改指向、首次即失败、持久化取值优先）；
//! 3. **R32** 的**持久化读回**维度：关掉 broker 之后重开同一 `data_dir` 的真实存储，
//!    `uncertain` 仍然能从持久行读回——证明不确定性不只在内存里。
//!
//! （来自评审、裁决转给 TP2）：补 `Actor::Node` 的 R31 断言——观察型 Access 对
//! 「存在但不覆盖 agent 的会话」与「不存在的会话」得到**同一响应**，且 `load_recovery` 计数为 0。

mod support;

use acp_core::model::{Actor, NodeId, RequestId, SessionRecoveryRecord};
use acp_core::ports::{ReplayLimit, SessionStore as _};
use serde_json::{Value, json};
use server::local_admin::Method;
use support::nodelink::{AccessKey, NodeLinkClient, PairingTicket};
use support::owner::{OwnerNode, ResumeBehavior};

/// 本文件扮演的 Access Node（`grant.remote-work` 那一档）。
const ACCESS_WORK: &str = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";
/// 只持 `grant.observe` 的 Access Node（R30/R31）。
const ACCESS_OBSERVE: &str = "2ae1c07c-9242-46e9-a9d2-4ec58c130f50";
/// 不存在的会话 id（R31/R34 的「不可区分」对照）。
const SESSION_ABSENT: &str = "9ae1c07c-9242-46e9-a9d2-4ec58c130f4a";
const EXPORT_VISIBLE: &str = "export.visible";
const WORKSPACE_ALIAS: &str = "project.one";
const AGENT: &str = "codex";
const FULL_GRANTS: [&str; 3] = ["grant.observe", "grant.interact", "grant.remote-work"];

/// 一次配对准备的结果（Owner + Access 身份 + ticket）。
struct Paired {
    owner: OwnerNode,
    access: AccessKey,
    ticket: PairingTicket,
}

impl Paired {
    /// 走到「status approved」为止：注册 workspace/agent/Export → `node.pair.begin` → Access claim →
    /// 本地确认 → status approved。
    ///
    /// `workspace_root` 是本次注册到 `WORKSPACE_ALIAS` 的目录（R23/R25 需要它可被删除或改指向）。
    async fn up_to_approval(
        label: &str,
        access_node: &str,
        grants: &[&str],
        workspace_root: &std::path::Path,
    ) -> Self {
        let owner = OwnerNode::start(label).await;
        owner
            .admin(
                Method::WorkspaceSelect,
                json!({
                    "alias": WORKSPACE_ALIAS,
                    "displayName": "Project One",
                    "rootPath": workspace_root.display().to_string(),
                }),
            )
            .await;
        owner
            .admin(
                Method::AgentConfigure,
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
        owner
            .admin(
                Method::ExportCreate,
                json!({
                    "exportId": EXPORT_VISIBLE,
                    "displayName": "Export visible",
                    "agentIds": [AGENT],
                    "workspaceAliases": [
                        { "alias": WORKSPACE_ALIAS, "displayName": "Project One" },
                    ],
                    "defaultWorkspaceAlias": WORKSPACE_ALIAS,
                    "templates": [
                        {
                            "templateId": "tpl.default",
                            "displayName": "Default",
                            "workspaceAlias": WORKSPACE_ALIAS,
                            "params": [],
                        },
                    ],
                    "defaultTemplateId": "tpl.default",
                    "scopes": FULL_GRANTS,
                    "cachePolicy": "no-content-cache",
                }),
            )
            .await;

        let access = AccessKey::new(access_node);
        let begin = owner
            .admin(
                Method::NodePairBegin,
                json!({
                    "mode": "owner",
                    "pairingUrl": null,
                    "displayName": "Owner Node",
                    "requestedGrants": grants,
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
            .admin(
                Method::NodePairConfirm,
                json!({
                    "pairingId": pairing_id,
                    "grants": grants,
                    "exportIds": [EXPORT_VISIBLE],
                }),
            )
            .await;

        let pairing_request_id = body["pairingRequestId"].as_str().expect("pairingRequestId");
        let server_nonce = body["serverNonce"].as_str().expect("serverNonce");
        let status = support::nodelink::status_request(
            &ticket,
            &access,
            pairing_request_id,
            &support::nodelink::nonce_text(0x42),
        );
        let _ = server_nonce;
        let reply = support::nodelink::post_json(
            owner.addr,
            server::node_link::STATUS_PATH,
            &serde_json::to_vec(&status).expect("status 可序列化"),
        )
        .await;
        assert_eq!(reply.json()["status"], json!("approved"));

        Self {
            owner,
            access,
            ticket,
        }
    }

    /// 拆成独立三件，使调用方能**拥有** `OwnerNode`（`stop` 需要所有权）。
    fn into_parts(self) -> (OwnerNode, AccessKey, PairingTicket) {
        (self.owner, self.access, self.ticket)
    }
}

/// 在真实 loopback listener 上完成 WSS 握手（含 `catalogRevision` 验签）。
async fn connect(owner: &OwnerNode, access: &AccessKey, ticket: &PairingTicket) -> NodeLinkClient {
    let mut client = NodeLinkClient::connect_plain(owner.addr).await;
    support::nodelink::handshake(
        &mut client,
        access,
        ticket,
        &support::nodelink::nonce_text(0x41),
    )
    .await;
    client
}

/// 建一个真实存在、且 `canonicalize` 结果与自身逐字相同的目录（core 的 cwd 复校验要求）。
fn workspace_dir(label: &str) -> std::path::PathBuf {
    let root = std::fs::canonicalize(std::env::temp_dir()).expect("临时目录可规范化");
    let path = root.join(format!("acp-remote-tp2-{label}"));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("建立工作目录");
    std::fs::canonicalize(&path).expect("新建目录可规范化")
}

fn session_create_body(request_id: &str) -> Value {
    json!({
        "requestId": request_id,
        "command": "session.create",
        "sessionRef": null,
        "attachmentId": null,
        "attachmentGeneration": null,
        "expectedVersion": null,
        "payload": {
            "agentId": AGENT,
            "exportId": EXPORT_VISIBLE,
            "workspaceAlias": WORKSPACE_ALIAS,
        },
    })
}

/// `session.resume`：session-scoped，必须带当前 attachment 代际，`payload` 恒为 `{}`。
fn session_resume_body(
    request_id: &str,
    session: &Value,
    attachment: &Value,
    payload: Value,
) -> Value {
    json!({
        "requestId": request_id,
        "command": "session.resume",
        "sessionRef": session.clone(),
        "attachmentId": attachment["attachmentId"],
        "attachmentGeneration": attachment["attachmentGeneration"],
        "expectedVersion": null,
        "payload": payload,
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

/// 建一个会话并拿到 `(remoteSessionRef, attachment)`。
async fn create_session(client: &mut NodeLinkClient) -> (Value, Value) {
    let request = support::nodelink::uuid_text();
    client.step("session.create");
    client
        .send("command.submit", session_create_body(&request))
        .await;
    let accepted = client.expect("command.accepted").await;
    assert_eq!(accepted["body"]["result"], Value::Null);
    let terminal = client.expect("command.terminal").await;
    assert_eq!(terminal["body"]["terminal"]["status"], json!("completed"));
    let session = terminal["body"]["terminal"]["result"]["remoteSessionRef"].clone();
    let attachment = attach(client, &session).await;
    (session, attachment)
}

/// R22 + R26 + R35 + R27/R34 的结果形状：一次成功恢复走完整条真实管线。
///
/// 关键点：
///
/// - `accepted.result` 必须是 `null`（R34：mutation 先 accepted）；终态 `result` 是
///   `SessionResumeResult`（含 `remoteSessionRef` 与 `sessionMeta`），**不是** `{}`（R27）；
/// - 后端收到的 `ResumeSessionRequest` 逐字等于**创建时持久化**的取值——即使别名在恢复前已被改指到
///   别的目录（R26/R23）；
/// - 恢复成功与失败**都不改写**两列（R22）；恢复后该会话仍可 `session.prompt`（R35）。
#[test]
fn a_successful_resume_returns_the_session_reference_and_leaves_the_columns_untouched() {
    support::block_on(async {
        let original = workspace_dir("resume-original");
        let moved = workspace_dir("resume-moved");
        let paired =
            Paired::up_to_approval("resume-ok", ACCESS_WORK, &FULL_GRANTS, &original).await;
        let (owner, access, ticket) = paired.into_parts();
        let mut client = connect(&owner, &access, &ticket).await;

        let (session, attachment) = create_session(&mut client).await;
        let session_id = session["sessionId"].as_str().expect("sessionId").to_owned();
        let session_key = acp_core::model::SessionId::new(&session_id).expect("session id");

        // 创建后持久化的恢复数据（经产品读路径 `load_recovery` 读回，不是内存模型）。
        let before = owner
            .sessions
            .load_recovery(&session_key)
            .await
            .expect("读回恢复数据")
            .expect("创建成功后必须持久化 ACP 会话标识与创建时目录");
        assert_eq!(
            before.workspace_cwd.as_deref(),
            Some(original.to_string_lossy().as_ref()),
            "持久化的 workspace_cwd 必须就是创建时解析出的规范化目录"
        );
        let persisted_agent_session_id = before
            .agent_session_id
            .clone()
            .expect("创建成功后必须持久化 ACP 会话标识");

        // R23/R26：把**同一个 alias** 改指到另一个真实目录。恢复必须仍用持久化取值。
        owner
            .admin(
                Method::WorkspaceSelect,
                json!({
                    "alias": WORKSPACE_ALIAS,
                    "displayName": "Project One",
                    "rootPath": moved.display().to_string(),
                }),
            )
            .await;

        // R34：先回 `accepted(result = null)`，再回 `completed` 终态。
        let request = support::nodelink::uuid_text();
        client.step("session.resume");
        client
            .send(
                "command.submit",
                session_resume_body(&request, &session, &attachment, json!({})),
            )
            .await;
        let accepted = client.expect("command.accepted").await;
        assert_eq!(accepted["body"]["command"], json!("session.resume"));
        assert_eq!(
            accepted["body"]["result"],
            Value::Null,
            "session.resume 的 accepted 必须是 result = null"
        );
        let terminal = client.expect("command.terminal").await;
        assert_eq!(terminal["body"]["requestId"], json!(request));
        assert_eq!(terminal["body"]["terminal"]["status"], json!("completed"));
        let result = &terminal["body"]["terminal"]["result"];
        assert_eq!(
            result["remoteSessionRef"], session,
            "终态结果必须回传同一个会话引用（R35）：{terminal}"
        );
        assert_eq!(
            result["sessionMeta"]["state"],
            json!("idle"),
            "SessionResumeResult 必须带 sessionMeta：{terminal}"
        );
        assert!(
            terminal["body"]["terminal"]["terminalEventId"].is_string(),
            "终态必须带 terminalEventId：{terminal}"
        );

        // R26：后端收到的是**持久化原文**，不是改指后的目录，也不是重新解析出的路径。
        assert_eq!(owner.resume_probe.calls(), 1, "后端只被调用一次");
        let received = owner
            .resume_probe
            .last_request()
            .expect("后端必须收到恢复请求");
        assert_eq!(
            received.agent_session_id, persisted_agent_session_id,
            "恢复用的 ACP 会话标识必须逐字是持久化取值"
        );
        assert_eq!(
            received.workspace_cwd,
            original.to_string_lossy().into_owned(),
            "恢复用的 cwd 必须逐字是持久化取值（不按别名重新解析，R26）"
        );
        assert_ne!(
            received.workspace_cwd,
            moved.to_string_lossy().into_owned(),
            "cwd 不得被换成别名现在指向的目录"
        );

        // R22：成功恢复**不改写**两列。
        let after_success = owner
            .sessions
            .load_recovery(&session_key)
            .await
            .expect("读回恢复数据")
            .expect("恢复后恢复数据仍在");
        assert_same_recovery(&before, &after_success, "成功恢复之后");

        // R22：失败恢复同样**不改写**两列（把后端切成「不支持」）。
        owner
            .resume_probe
            .set_behavior(ResumeBehavior::BackendUnsupported);
        let failing = support::nodelink::uuid_text();
        client.step("session.resume (unsupported)");
        client
            .send(
                "command.submit",
                session_resume_body(&failing, &session, &attachment, json!({})),
            )
            .await;
        client.expect("command.accepted").await;
        let failed = client.expect("command.terminal").await;
        assert_eq!(failed["body"]["terminal"]["status"], json!("failed"));
        assert_eq!(
            failed["body"]["terminal"]["error"]["code"],
            json!("nodelink.command.unsupported"),
            "R37：目标 Agent 未宣告能力时终态错误码是 unsupported：{failed}"
        );
        let after_failure = owner
            .sessions
            .load_recovery(&session_key)
            .await
            .expect("读回恢复数据")
            .expect("失败恢复不得删除两列");
        assert_same_recovery(&before, &after_failure, "失败恢复之后");

        // R35：恢复后的会话仍可继续接受 `session.prompt`。
        let marker = "marker-after-resume-7b31";
        let prompt_request = support::nodelink::uuid_text();
        client.step("session.prompt after resume");
        client
            .send(
                "command.submit",
                json!({
                    "requestId": prompt_request,
                    "command": "session.prompt",
                    "sessionRef": session.clone(),
                    "attachmentId": attachment["attachmentId"],
                    "attachmentGeneration": attachment["attachmentGeneration"],
                    "expectedVersion": null,
                    "payload": { "content": [ { "type": "text", "text": marker } ] },
                }),
            )
            .await;
        client.expect("command.accepted").await;
        assert_eq!(
            owner.release_events(marker),
            2,
            "恢复后的会话必须真的能派发 turn"
        );
        owner.broker.pump(&session_key).await.expect("提交后端事件");
        client.expect("command.terminal").await;

        owner.stop().await;
    });
}

/// 两条 `SessionRecoveryRecord` 必须逐字相同（R22 的字节断言）。
fn assert_same_recovery(
    before: &SessionRecoveryRecord,
    after: &SessionRecoveryRecord,
    label: &str,
) {
    assert_eq!(
        before.agent_session_id, after.agent_session_id,
        "{label}：agent_session_id 不得被改写"
    );
    assert_eq!(
        before.workspace_cwd, after.workspace_cwd,
        "{label}：workspace_cwd 不得被改写（不得用别名、新解析路径或新会话标识覆盖）"
    );
    assert_eq!(before.agent, after.agent, "{label}：agent 不得被改写");
}

/// R28：同一 `(requestId, command, payload)` 重试返回**首次**结果，副作用只发生一次。
#[test]
fn a_repeated_resume_request_id_replays_the_first_result_once() {
    support::block_on(async {
        let workspace = workspace_dir("resume-idempotent");
        let paired =
            Paired::up_to_approval("resume-idem", ACCESS_WORK, &FULL_GRANTS, &workspace).await;
        let (owner, access, ticket) = paired.into_parts();
        let mut client = connect(&owner, &access, &ticket).await;
        let (session, attachment) = create_session(&mut client).await;

        let request = support::nodelink::uuid_text();
        let body = session_resume_body(&request, &session, &attachment, json!({}));
        client.step("first resume");
        client.send("command.submit", body.clone()).await;
        client.expect("command.accepted").await;
        let first = client.expect("command.terminal").await;
        assert_eq!(first["body"]["terminal"]["status"], json!("completed"));

        client.step("retried resume");
        client.send("command.submit", body).await;
        let replay = client.expect("command.terminal").await;
        assert_eq!(
            replay["body"]["terminal"]["result"], first["body"]["terminal"]["result"],
            "同键重试必须逐字回首次结果：{replay}"
        );
        assert_eq!(
            owner.resume_probe.calls(),
            1,
            "重试不得产生第二次后端调用（副作用只发生一次）"
        );

        // R29：同一 requestId 配不同语义 → 幂等冲突，不执行第二次副作用。
        client.step("conflict");
        client
            .send(
                "command.submit",
                json!({
                    "requestId": request,
                    "command": "session.create",
                    "sessionRef": null,
                    "attachmentId": null,
                    "attachmentGeneration": null,
                    "expectedVersion": null,
                    "payload": {
                        "agentId": AGENT,
                        "exportId": EXPORT_VISIBLE,
                        "workspaceAlias": WORKSPACE_ALIAS,
                    },
                }),
            )
            .await;
        let conflict = client.expect("command.rejected").await;
        assert_eq!(
            conflict["body"]["error"]["code"],
            json!("nodelink.command.idempotency_conflict"),
            "同键不同语义必须冲突：{conflict}"
        );
        assert_eq!(
            owner.resume_probe.calls(),
            1,
            "冲突的请求不得触发第二次副作用"
        );

        owner.stop().await;
    });
}

/// R31（补充 `Actor::Node` 的「先于本机读取」证据）。
///
/// 只持 `grant.observe` 的 Access 提交 `session.resume` 时：
///
/// 1. 回 `command.rejected`（`nodelink.export.not_granted`）；
/// 2. `load_recovery` 计数为 0 ⇒ **授权先于本机读取**；
/// 3. 对「存在但**不覆盖**该节点授权的会话」与「不存在的会话」得到**完全相同**的响应——判别力来自
///    目标会话的持久化目录**已被删除**：若实现先读会话行再做目录复校验，错误码会是
///    `nodelink.internal.unavailable` 而不是 `export.not_granted`。
#[test]
fn an_unauthorized_resume_is_rejected_before_any_local_read() {
    support::block_on(async {
        let workspace = workspace_dir("resume-unauthorized");
        // 先用有权限的节点建出会话并落持久化恢复数据。
        let paired =
            Paired::up_to_approval("resume-denied", ACCESS_WORK, &FULL_GRANTS, &workspace).await;
        let (owner, access, ticket) = paired.into_parts();
        let mut worker = connect(&owner, &access, &ticket).await;
        let (session, _attachment) = create_session(&mut worker).await;

        // 让目标会话的持久化目录消失：任何「先读后校验」的实现都会因此回 unavailable。
        std::fs::remove_dir_all(&workspace).expect("删除持久化目录");

        // 再配一个只持 `grant.observe` 的 Access 节点。
        let observer = AccessKey::new(ACCESS_OBSERVE);
        let begin = owner
            .admin(
                Method::NodePairBegin,
                json!({
                    "mode": "owner",
                    "pairingUrl": null,
                    "displayName": "Owner Node",
                    "requestedGrants": ["grant.observe"],
                }),
            )
            .await;
        let pairing_id = begin["pairingId"].as_str().expect("pairingId").to_owned();
        let ticket = support::nodelink::ticket_from_url(
            begin["pairingUrl"].as_str().expect("pairingUrl"),
            &pairing_id,
        );
        let client_nonce = support::nodelink::nonce_text(0x61);
        let claim = support::nodelink::claim_request(&ticket, &observer, &client_nonce);
        let reply = support::nodelink::post_json(
            owner.addr,
            server::node_link::CLAIM_PATH,
            &serde_json::to_vec(&claim).expect("claim 可序列化"),
        )
        .await;
        assert_eq!(reply.status, 201);
        let body = reply.json();
        support::nodelink::verify_owner_proof(&ticket, &body, &observer, &client_nonce);
        owner
            .admin(
                Method::NodePairConfirm,
                json!({
                    "pairingId": pairing_id,
                    "grants": ["grant.observe"],
                    "exportIds": [EXPORT_VISIBLE],
                }),
            )
            .await;
        let status = support::nodelink::status_request(
            &ticket,
            &observer,
            body["pairingRequestId"].as_str().expect("pairingRequestId"),
            &support::nodelink::nonce_text(0x62),
        );
        let reply = support::nodelink::post_json(
            owner.addr,
            server::node_link::STATUS_PATH,
            &serde_json::to_vec(&status).expect("status 可序列化"),
        )
        .await;
        assert_eq!(reply.json()["status"], json!("approved"));

        let mut client = NodeLinkClient::connect_plain(owner.addr).await;
        support::nodelink::handshake(
            &mut client,
            &observer,
            &ticket,
            &support::nodelink::nonce_text(0x61),
        )
        .await;

        let attachment = attach(&mut client, &session).await;

        // 观察点自检（不是假设）：`recovery_reads` 必须真的在计数，否则下面「授权先于本机读取」的
        // 「计数不变」断言是**恒真**的。（该计数器曾因 `Arc` 建了两个实例而恒为 0，TP2 tester-A2 修正。）
        let session_key =
            acp_core::model::SessionId::new(session["sessionId"].as_str().expect("sessionId"))
                .expect("session id");
        let probe_before = owner.recovery_reads();
        assert!(
            owner
                .sessions
                .load_recovery(&session_key)
                .await
                .expect("读回")
                .is_some(),
            "R31 的前提：目标会话确有持久化恢复数据"
        );
        assert!(
            owner.recovery_reads() > probe_before,
            "load_recovery 的计数器必须真的递增，否则「授权先于本机读取」是恒真断言"
        );

        let reads_before = owner.recovery_reads();
        let resume_calls_before = owner.resume_probe.calls();

        client.step("observe resume (existing)");
        client
            .send(
                "command.submit",
                session_resume_body(
                    &support::nodelink::uuid_text(),
                    &session,
                    &attachment,
                    json!({}),
                ),
            )
            .await;
        let denied = client.expect("command.rejected").await;
        assert_eq!(denied["body"]["command"], json!("session.resume"));
        assert_eq!(
            denied["body"]["error"]["code"],
            json!("nodelink.export.not_granted"),
            "越权恢复必须回 not_granted（目录已被删：若是先读后校验，这里会是 internal.unavailable）"
        );
        assert_eq!(
            owner.recovery_reads(),
            reads_before,
            "授权必须先于本机读取（load_recovery 是读会话行的唯一入口）"
        );
        assert_eq!(
            owner.resume_probe.calls(),
            resume_calls_before,
            "越权恢复不得触碰后端"
        );

        // 响应不因会话是否存在而不同：换一个不存在的会话 id，得到**同形**的拒绝。
        let mut absent = session.clone();
        absent["sessionId"] = json!(SESSION_ABSENT);
        client.step("observe resume (absent)");
        client
            .send(
                "command.submit",
                session_resume_body(
                    &support::nodelink::uuid_text(),
                    &absent,
                    &attachment,
                    json!({}),
                ),
            )
            .await;
        let absent_denied = client.expect("command.rejected").await;
        assert_eq!(
            absent_denied["body"]["error"]["code"], denied["body"]["error"]["code"],
            "存在与不存在不得产生可区分的响应"
        );
        // 「不存在」侧的额外防泄露断言：`details` 两条路径**实测同为 `{}`（空对象，非 `null`）**——
        // 两条都走 `command.rs::deny` 的 `CommandFault::NotGranted(None)` 分支（`RawObject::empty()`）。
        // 所以这是**结构性防泄露守卫**（若将来某条路径回 `NotGranted(Some(parameter))`，本断言会失败），
        // 而不是当前的判别点；真正的判别力在上一条 `code` 断言与「目录已被删除」这个前提上。
        // 口径来源：实测。
        assert_eq!(
            absent_denied["body"]["error"]["details"], denied["body"]["error"]["details"],
            "错误详情也不得泄露目标会话的存在性"
        );
        assert_eq!(
            owner.recovery_reads(),
            reads_before,
            "不存在会话的越权恢复同样不得读会话行"
        );

        owner.stop().await;
    });
}

/// R37 口径：能力未宣告的终态必须是 **`failed`** + `nodelink.command.unsupported`
/// （`uncertain` 只属崩溃窗口 R32），且不创建新会话、不报告成功。
///
/// 同时用另一种失败（Agent 明确拒绝恢复）证明**两类失败不是同一个漏斗**：错误码不同。
#[test]
fn an_unsupported_agent_fails_the_resume_terminal_without_creating_a_session() {
    support::block_on(async {
        let workspace = workspace_dir("resume-unsupported");
        let paired =
            Paired::up_to_approval("resume-unsupported", ACCESS_WORK, &FULL_GRANTS, &workspace)
                .await;
        let (owner, access, ticket) = paired.into_parts();
        let mut client = connect(&owner, &access, &ticket).await;
        let (session, attachment) = create_session(&mut client).await;
        let session_id = session["sessionId"].as_str().expect("sessionId").to_owned();
        let session_key = acp_core::model::SessionId::new(&session_id).expect("session id");
        let before = owner
            .sessions
            .load_recovery(&session_key)
            .await
            .expect("读回")
            .expect("恢复数据");

        // ① 后端不支持（目标 Agent 未宣告 `sessionCapabilities.resume`）。
        owner
            .resume_probe
            .set_behavior(ResumeBehavior::BackendUnsupported);
        client.step("resume with unsupported agent");
        client
            .send(
                "command.submit",
                session_resume_body(
                    &support::nodelink::uuid_text(),
                    &session,
                    &attachment,
                    json!({}),
                ),
            )
            .await;
        client.expect("command.accepted").await;
        let terminal = client.expect("command.terminal").await;
        assert_eq!(
            terminal["body"]["terminal"]["status"],
            json!("failed"),
            "能力未宣告是确定类失败，终态必须是 failed（uncertain 只属崩溃窗口 R32）：{terminal}"
        );
        assert_eq!(
            terminal["body"]["terminal"]["error"]["code"],
            json!("nodelink.command.unsupported"),
            "{terminal}"
        );
        assert!(
            terminal["body"]["terminal"]["result"].is_null(),
            "失败终态不得报告成功"
        );

        // ② Agent 明确拒绝恢复：同样是 failed，但是**另一个**错误码（两类失败不共漏斗）。
        owner.resume_probe.set_behavior(ResumeBehavior::Refused);
        client.step("resume refused by agent");
        client
            .send(
                "command.submit",
                session_resume_body(
                    &support::nodelink::uuid_text(),
                    &session,
                    &attachment,
                    json!({}),
                ),
            )
            .await;
        client.expect("command.accepted").await;
        let refused = client.expect("command.terminal").await;
        assert_eq!(refused["body"]["terminal"]["status"], json!("failed"));
        assert_ne!(
            refused["body"]["terminal"]["error"]["code"],
            terminal["body"]["terminal"]["error"]["code"],
            "「不支持」与「Agent 拒绝」必须可区分：{refused}"
        );

        // ③ 没有新建会话：会话清单仍然只有创建出来的那一条。
        let listed = owner
            .sessions
            .list(acp_core::ports::SessionQuery::default())
            .await
            .expect("列出会话");
        assert_eq!(listed.len(), 1, "失败的恢复不得创建新会话：{listed:?}");
        // ④ 两列未被改写。
        let after = owner
            .sessions
            .load_recovery(&session_key)
            .await
            .expect("读回")
            .expect("恢复数据仍在");
        assert_same_recovery(&before, &after, "失败的恢复之后");

        owner.stop().await;
    });
}

/// R32（崩溃窗口，含持久化读回维度）：终态提交失败后命令停在非终态；调用**组合根启动时
/// 调用的同一入口** `recover_unsettled` 之后，`command.status` 回 `uncertain`——并且该 `uncertain`
/// 存在于**持久行**里：关掉 broker 之后重开同一 `data_dir` 的真实存储，仍然读回同一条 `uncertain`
/// 记录（证明不确定性不只在进程内存里）。
///
/// 判别力：终态提交被 `FlakySessionStore` 按 marker 拦下（marker 只出现在终态事件视图里，
/// `resume_session` 的 accepted 提交不含事件，因此不会误伤）；若实现把崩溃窗口错判成 `failed`
/// 或 `completed`，`command.status` 的断言会失败。
#[test]
fn a_crash_window_leaves_uncertain_persisted_across_a_reopened_store() {
    support::block_on(async {
        let workspace = workspace_dir("resume-uncertain");
        let paired =
            Paired::up_to_approval("resume-uncertain", ACCESS_WORK, &FULL_GRANTS, &workspace).await;
        let (owner, access, ticket) = paired.into_parts();
        let mut client = connect(&owner, &access, &ticket).await;
        let (session, attachment) = create_session(&mut client).await;
        let session_id = session["sessionId"].as_str().expect("sessionId").to_owned();
        let session_key = acp_core::model::SessionId::new(&session_id).expect("session id");
        let before = owner
            .sessions
            .load_recovery(&session_key)
            .await
            .expect("读回")
            .expect("恢复数据");
        let resume_calls_before = owner.resume_probe.calls();

        // ① 只让**本次终态提交**失败（marker = 本次 requestId，它只出现在终态事件视图里）。
        let request = support::nodelink::uuid_text();
        owner.fail_commits_with(Some(&request));
        client.step("resume with terminal commit fault");
        client
            .send(
                "command.submit",
                session_resume_body(&request, &session, &attachment, json!({})),
            )
            .await;
        let accepted = client.expect("command.accepted").await;
        assert_eq!(
            accepted["body"]["result"],
            Value::Null,
            "崩溃窗口里命令停在 accepted"
        );
        let drained = client.collect(std::time::Duration::from_millis(600)).await;
        // 本次连接上**确实**出现了一条 `command.terminal`（实现选择先回一条本地终态、持久记录留在
        // accepted）。它的 `status`/`terminalEventId` 必须在末尾按 R32 逐项核对；先把其余证据取全，
        // 使一次失败也能留下完整证据链。
        let immediate = drained
            .iter()
            .find(|message| message["type"] == json!("command.terminal"))
            .cloned();
        assert!(
            owner
                .rejected_commits
                .load(std::sync::atomic::Ordering::SeqCst)
                >= 1,
            "故障必须真的命中终态提交"
        );
        owner.fail_commits_with(None);
        assert_eq!(
            owner.resume_probe.calls(),
            resume_calls_before + 1,
            "崩溃窗口里不得自动重试后端副作用"
        );

        // ② 组合根启动时调用的**同一入口**跑启动恢复：把非终态记录结为 `uncertain`。
        let settled = owner
            .core
            .recover_unsettled(&Actor::LocalCli, ReplayLimit::new(16))
            .await
            .expect("启动恢复");
        assert_eq!(settled, 1, "恰好一条未终结命令被终结");

        // ③ 经 wire 的 `command.status` 重查：`uncertain` 而不是猜测成功/失败。
        client.step("command.status after recovery");
        client
            .send(
                "command.submit",
                command_status_body(&support::nodelink::uuid_text(), &request),
            )
            .await;
        let reread = client.expect("command.terminal").await;
        assert_eq!(reread["body"]["command"], json!("session.resume"));
        assert_eq!(
            reread["body"]["terminal"]["status"],
            json!("uncertain"),
            "崩溃窗口必须进入 uncertain：{reread}"
        );
        assert!(
            !reread["body"]["terminal"]["error"].is_null(),
            "uncertain 必须带结构化错误：{reread}"
        );
        assert!(
            reread["body"]["terminal"]["terminalEventId"].is_string(),
            "恢复必须写持久终态事件：{reread}"
        );

        // ④ 重开同一 `data_dir` 的**真实存储**，从持久行读回同一条 `uncertain` 记录。
        let reopened = owner.reopen_store().await;
        // Node Link 的 actor 两个节点维度都是**对端**（§12.5 的幂等键维度），不是本机 node id。
        let access_node = NodeId::new(ACCESS_WORK).expect("node id");
        let actor = Actor::Node {
            node: access_node.clone(),
            access_node,
        };
        let request_id = RequestId::new(&request).expect("request id");
        let record = reopened
            .find_request(&request_id, &actor)
            .await
            .expect("重开后读持久记录")
            .expect("uncertain 记录必须存在于持久行里（不只在进程内存里）");
        assert_eq!(
            record.status().as_str(),
            "uncertain",
            "重开存储后读回的终态必须仍是 uncertain"
        );
        assert_eq!(
            record.command(),
            "session.resume",
            "持久记录必须是 session.resume 的那条"
        );
        reopened.close().await;

        // ⑤ 崩溃窗口同样不改写两列，且没有第二个端点/会话。
        let after = owner
            .sessions
            .load_recovery(&session_key)
            .await
            .expect("读回")
            .expect("恢复数据仍在");
        assert_same_recovery(&before, &after, "崩溃窗口之后");
        let listed = owner
            .sessions
            .list(acp_core::ports::SessionQuery::default())
            .await
            .expect("列出会话");
        assert_eq!(listed.len(), 1, "崩溃窗口不得产生第二个会话");

        // ⑥ 崩溃窗口里**那一轮**的本地终态帧：`docs/NODE_LINK_PROTOCOL.md` §12.7（修复轮
        //    注记）已把这条收窄写进合同——终态落盘失败时仍会发一帧本地 `command.terminal`，但
        //    `terminalEventId` 为 `null`，它**不**构成可稳定重放的首次结果。本用例因此断言的正是
        //    合同要求的两件事：
        //    ① 该帧**不得带** `terminalEventId`（否则 Access 会把它当成持久终态事实）；
        //    ② 权威终态只能由启动恢复之后的 `command.status` 给出（已在 ③/④ 断言为 `uncertain`）。
        let immediate = immediate.unwrap_or_else(|| {
            panic!("终态提交被拦下时连接上必须出现一条终态（否则 §11.5 的终态推送缺失）")
        });
        assert!(
            immediate["body"]["terminal"]["terminalEventId"].is_null(),
            "落盘失败那一轮的本地终态帧不得带 terminalEventId（§12.7：不构成可重放的首次结果）：{immediate}"
        );

        // ⑦ 崩溃窗口不得把副作用重试一遍，也不得把 `uncertain` 写成 `completed`/`failed`：
        //    权威终态是 `uncertain`（上面 ③ 已断言），因此任何把它说成成功/失败的回归都会被抓住。
        assert_eq!(
            reread["body"]["terminal"]["status"],
            json!("uncertain"),
            "权威终态是 uncertain（崩溃窗口不猜测成功或失败，R32）"
        );

        owner.stop().await;
    });
}

/// R24 + R25：目录复校验的两个失败变体，以及「能力不支持」的归类。
///
/// - 目录被删除 ⇒ `nodelink.internal.unavailable`（服务端不可用类），后端**不被调用**；
/// - 持久化路径仍存在但 `canonicalize` 结果不同 ⇒ 同样是 `internal.unavailable`，且**持久化取值
///   未被改写**（不使用新解析出的路径）；
/// - 能力不支持（受控后端回 `BackendUnsupported`）⇒ `nodelink.command.unsupported`，**不是**
///   `internal.unavailable`。
///
/// **本用例不造「两列为 `NULL` 的会话」**（第 ③ 段只把**受控后端**切到
/// `ResumeBehavior::BackendUnsupported`，会话自身的两列始终有值）。两列 `NULL` 侧的真实证据在别处：
/// `server/src/node_link/command/tests.rs::session_resume_without_persisted_recovery_data_fails_as_unsupported`
/// 与 `crates/storage-sqlite/tests/resume_columns.rs`（`a_half_null_recovery_pair_is_still_no_recovery_data`
/// / `upgraded_sessions_keep_their_bytes_and_report_no_recovery_data`）。规格要求这两者**同一条路径**
/// （`nodelink.command.unsupported`，裁决后的措辞），本用例负责的是「能力不支持」这一侧。
#[test]
fn workspace_revalidation_and_null_recovery_data_take_distinct_paths() {
    support::block_on(async {
        let workspace = workspace_dir("resume-revalidate");
        let paired =
            Paired::up_to_approval("resume-revalidate", ACCESS_WORK, &FULL_GRANTS, &workspace)
                .await;
        let (owner, access, ticket) = paired.into_parts();
        let mut client = connect(&owner, &access, &ticket).await;
        let (session, attachment) = create_session(&mut client).await;
        let session_id = session["sessionId"].as_str().expect("sessionId").to_owned();
        let session_key = acp_core::model::SessionId::new(&session_id).expect("session id");
        let before = owner
            .sessions
            .load_recovery(&session_key)
            .await
            .expect("读回")
            .expect("恢复数据");
        let calls_before = owner.resume_probe.calls();

        // ① 目录被删除 → 服务端不可用类，后端不被调用。
        std::fs::remove_dir_all(&workspace).expect("删除持久化目录");
        client.step("resume with deleted workspace");
        client
            .send(
                "command.submit",
                session_resume_body(
                    &support::nodelink::uuid_text(),
                    &session,
                    &attachment,
                    json!({}),
                ),
            )
            .await;
        client.expect("command.accepted").await;
        let deleted = client.expect("command.terminal").await;
        assert_eq!(deleted["body"]["terminal"]["status"], json!("failed"));
        assert_eq!(
            deleted["body"]["terminal"]["error"]["code"],
            json!("nodelink.internal.unavailable"),
            "目录复校验失败是服务端不可用类（不是 unsupported）：{deleted}"
        );
        assert_eq!(
            owner.resume_probe.calls(),
            calls_before,
            "目录复校验必须在调用后端之前完成（R24）"
        );

        // ② 别名改指到**另一个真实存在**的目录，而持久化目录仍然缺失：不得回退到「最接近的目录」。
        let moved = workspace_dir("resume-revalidate-moved");
        owner
            .admin(
                Method::WorkspaceSelect,
                json!({
                    "alias": WORKSPACE_ALIAS,
                    "displayName": "Project One",
                    "rootPath": moved.display().to_string(),
                }),
            )
            .await;
        client.step("resume with alias pointing elsewhere");
        client
            .send(
                "command.submit",
                session_resume_body(
                    &support::nodelink::uuid_text(),
                    &session,
                    &attachment,
                    json!({}),
                ),
            )
            .await;
        client.expect("command.accepted").await;
        let alias_elsewhere = client.expect("command.terminal").await;
        assert_eq!(
            alias_elsewhere["body"]["terminal"]["status"],
            json!("failed"),
            "别名指向别处不得让恢复成功：{alias_elsewhere}"
        );
        assert_eq!(
            alias_elsewhere["body"]["terminal"]["error"]["code"],
            json!("nodelink.internal.unavailable"),
            "不得回退到按别名重新解析的目录：{alias_elsewhere}"
        );
        assert_eq!(
            owner.resume_probe.calls(),
            calls_before,
            "目录复校验必须在调用后端之前完成"
        );

        // ③ 在**同一路径**重新建立目录后恢复成功 ⇒ 每次恢复都**重新**复校验（「它曾经合法」不是跳过
        //    校验的理由，反过来也一样：不因上一次的结论而跳过）。
        std::fs::create_dir_all(&workspace).expect("同名重建");
        client.step("resume after same-path recreate");
        client
            .send(
                "command.submit",
                session_resume_body(
                    &support::nodelink::uuid_text(),
                    &session,
                    &attachment,
                    json!({}),
                ),
            )
            .await;
        client.expect("command.accepted").await;
        let recreated = client.expect("command.terminal").await;
        assert_eq!(
            recreated["body"]["terminal"]["status"],
            json!("completed"),
            "持久化目录重新有效后恢复必须成功（复校验每次都重跑）：{recreated}"
        );
        assert_eq!(
            owner.resume_probe.calls(),
            calls_before + 1,
            "成功这一次才允许调用后端"
        );

        // ③ 能力不支持与「目录不可用」是两个可区分的路径（裁决后的规格措辞：两列为 `NULL`
        //    与能力不支持同路径，其余校验失败仍是服务端不可用类）。
        //    本段只把**受控后端**切到 `BackendUnsupported`，**没有**造出两列 `NULL` 的会话；
        //    两列 `NULL` 侧由 `server/src/node_link/command/tests.rs::
        //    session_resume_without_persisted_recovery_data_fails_as_unsupported` 与
        //    `crates/storage-sqlite/tests/resume_columns.rs` 承担。
        owner
            .resume_probe
            .set_behavior(ResumeBehavior::BackendUnsupported);
        client.step("resume when backend cannot resume");
        client
            .send(
                "command.submit",
                session_resume_body(
                    &support::nodelink::uuid_text(),
                    &session,
                    &attachment,
                    json!({}),
                ),
            )
            .await;
        client.expect("command.accepted").await;
        let unsupported = client.expect("command.terminal").await;
        assert_eq!(
            unsupported["body"]["terminal"]["error"]["code"],
            json!("nodelink.command.unsupported"),
            "能力不支持是 unsupported：{unsupported}"
        );
        assert_ne!(
            unsupported["body"]["terminal"]["error"]["code"],
            deleted["body"]["terminal"]["error"]["code"],
            "「目录不可用」与「不支持」必须是两个可区分的错误码"
        );

        // ④ 持久化取值在三条失败路径之后都不被改写。
        let after = owner
            .sessions
            .load_recovery(&session_key)
            .await
            .expect("读回")
            .expect("恢复数据仍在");
        assert_same_recovery(&before, &after, "目录复校验失败之后");

        owner.stop().await;
    });
}

/// R36：`payload` 携带**任何**键都在 accepted 之前被拒，且不启动后端、不改会话状态。
#[test]
fn any_resume_payload_field_is_rejected_before_side_effects() {
    support::block_on(async {
        let workspace = workspace_dir("resume-payload");
        let paired =
            Paired::up_to_approval("resume-payload", ACCESS_WORK, &FULL_GRANTS, &workspace).await;
        let (owner, access, ticket) = paired.into_parts();
        let mut client = connect(&owner, &access, &ticket).await;
        let (session, attachment) = create_session(&mut client).await;
        let session_id = session["sessionId"].as_str().expect("sessionId").to_owned();
        let session_key = acp_core::model::SessionId::new(&session_id).expect("session id");
        let before = owner
            .sessions
            .load_recovery(&session_key)
            .await
            .expect("读回")
            .expect("恢复数据");
        let calls_before = owner.resume_probe.calls();

        for (payload, field) in [
            (json!({ "cwd": "/tmp/attacker" }), "cwd"),
            (json!({ "agentId": AGENT }), "agentId"),
            (json!({ "cwd": null }), "cwd"),
            (json!({ "unknown": 1 }), "unknown"),
        ] {
            client.step("resume with forbidden payload field");
            client
                .send(
                    "command.submit",
                    session_resume_body(
                        &support::nodelink::uuid_text(),
                        &session,
                        &attachment,
                        payload.clone(),
                    ),
                )
                .await;
            let rejected = client.expect("command.rejected").await;
            assert_eq!(
                rejected["body"]["error"]["code"],
                json!("nodelink.command.unsupported_field"),
                "携带任何键都必须被拒（{payload}）：{rejected}"
            );
            assert_eq!(
                rejected["body"]["error"]["details"]["field"],
                json!(field),
                "details.field 必须指出被拒字段：{rejected}"
            );
        }
        assert_eq!(
            owner.resume_probe.calls(),
            calls_before,
            "被拒的 payload 不得启动 Agent 进程"
        );
        let after = owner
            .sessions
            .load_recovery(&session_key)
            .await
            .expect("读回")
            .expect("恢复数据仍在");
        assert_same_recovery(&before, &after, "被拒的 payload 之后");

        owner.stop().await;
    });
}

// ===== R25 / R24 的「路径存在、但不再是当初那个路径」两族 =====
//
// 三条用例覆盖同一个判定（`core::broker::revalidate_resume_workspace` 里
// 「`canonicalize` 的结果与持久化取值逐字相同」）的三种达成方式，**平台分工不同**：
//
// - 平台无关变体：本机（Windows）**真实执行并通过**；
// - `#[cfg(unix)]` 两族：只在 Unix 上编译与运行，本机（Windows）**从未执行**，
//   由 Linux CI 的 `checks` job（`ubuntu-latest`）真实执行。**本地未执行不等于通过。**
//
// 三条用例的共同前提是「路径仍然存在」——这正是 R25 与 R24（目录已被删除）的分界：
// 若路径不存在，测到的就是 R24 的判定，`canonicalize` 逐字比对这条分支就永远没被走到。

/// R25 的**平台无关**变体：持久化路径**仍然存在、仍是目录**，但 `canonicalize` 的结果与持久化取值
/// **逐字不同** ⇒ 拒绝恢复，且**不使用**新解析出的路径发送 `session/resume`。
///
/// 「未规范化但存在」的形态用**尾部多余分隔符**实现（`…/dir` vs `…/dir/`）。选它而不是 `..` 或 `.`
/// 是因为实测（`rustc` 单文件探针，Windows）：canonicalize 在 Windows 上返回 `\\?\` verbatim 前缀的
/// 路径，而 verbatim 路径**不做归一化**——`..`、`.`、`//`、`/.` 这些形态在 Windows 上连 `metadata`
/// 都会直接失败（os error 123/3），根本达不到「仍然存在」。尾部分隔符在两个平台上都同时满足
/// 「`metadata` 成功且是目录」与「`canonicalize` 结果不同」，是唯一真正平台无关的形态。
///
/// **判别力**（如果实现回退成「直接用新解析出的路径」）：新解析出的路径恰好就是**那个完全合法、
/// 已注册、仍然存在的**目录（前提断言④），因此回退实现会
/// ① 得到 `completed` 而不是 `failed`、② 让后端 `resume` 计数 +1、③ 让后端收到的 cwd 等于合法目录——
/// 三条断言全部失败。
#[test]
fn a_persisted_cwd_whose_canonical_form_differs_is_refused_before_any_backend_call() {
    use acp_core::model::{AgentId, AgentRef, ResumeSessionRequest};

    support::block_on(async {
        let workspace = workspace_dir("resume-noncanonical");
        let non_canonical = format!("{}{}", workspace.display(), std::path::MAIN_SEPARATOR);
        let paired =
            Paired::up_to_approval("resume-noncanonical", ACCESS_WORK, &FULL_GRANTS, &workspace)
                .await;
        let (owner, access, ticket) = paired.into_parts();
        let mut client = connect(&owner, &access, &ticket).await;
        let (session, attachment) = create_session(&mut client).await;
        let session_id = session["sessionId"].as_str().expect("sessionId").to_owned();
        let session_key = acp_core::model::SessionId::new(&session_id).expect("session id");
        let calls_before = owner.resume_probe.calls();

        // 前置①：产品写路径**只会**持久化规范化结果——这正是必须绕过端口直接写库的原因。
        let created = owner
            .sessions
            .load_recovery(&session_key)
            .await
            .expect("读回")
            .expect("创建成功后必须持久化恢复数据");
        assert_eq!(
            created.workspace_cwd.as_deref(),
            Some(workspace.to_string_lossy().as_ref()),
            "产品写路径持久化的必须是规范化结果"
        );
        owner
            .overwrite_persisted_workspace_cwd(&session_key, &non_canonical)
            .await;

        // 前置②：产品**读**路径现在看到的就是那个未规范化取值（改写确实生效，且没有第二处口径）。
        let tampered = owner
            .sessions
            .load_recovery(&session_key)
            .await
            .expect("读回")
            .expect("恢复数据仍在");
        assert_eq!(
            tampered.workspace_cwd.as_deref(),
            Some(non_canonical.as_str()),
            "读路径必须原样返回库里的字节"
        );
        assert_eq!(
            tampered.agent_session_id, created.agent_session_id,
            "只改写 cwd 一列（另一列是 R22 的对照）"
        );
        assert_eq!(tampered.agent, created.agent, "agent 不得被改写");

        // 前置③：R25 的四个条件同时成立，且**失败点唯一**（下面四条全是断言，不是假设）。
        let candidate = std::path::Path::new(&non_canonical);
        assert!(
            candidate.is_absolute(),
            "复校验的第一条（绝对）必须通过，否则失败点不是本用例要测的那一条"
        );
        let metadata = std::fs::metadata(&non_canonical)
            .expect("R25 的前提：持久化路径仍然存在（不存在就是 R24「目录被删」）");
        assert!(metadata.is_dir(), "复校验的第三条（是目录）必须通过");
        let canonical =
            std::fs::canonicalize(&non_canonical).expect("R25 的前提：新解析结果可得出");
        assert_ne!(
            canonical.to_str(),
            Some(non_canonical.as_str()),
            "R25 的前提：canonicalize 的结果与持久化取值逐字不同"
        );
        assert_eq!(
            canonical, workspace,
            "新解析出的路径恰好是那个合法且已注册的目录（回退实现会因此恢复成功）"
        );

        // 前置④：值对象**接受**该取值（绝对、非空、无 NUL、长度合规）——因此拒绝不可能来自
        // `ResumeSessionRequest::try_new`，只能来自 canonicalize 的逐字比对。
        assert!(
            ResumeSessionRequest::try_new(
                AgentRef::try_new(AgentId::new(AGENT).expect("agent id"), "Codex")
                    .expect("agent ref"),
                tampered
                    .agent_session_id
                    .clone()
                    .expect("创建成功后必须持久化 ACP 会话标识"),
                non_canonical.clone(),
            )
            .is_ok(),
            "值对象必须接受该取值：否则本用例测到的是值对象构造失败而不是 R25"
        );

        // 走真实恢复入口。
        client.step("resume with non-canonical persisted cwd");
        client
            .send(
                "command.submit",
                session_resume_body(
                    &support::nodelink::uuid_text(),
                    &session,
                    &attachment,
                    json!({}),
                ),
            )
            .await;
        client.expect("command.accepted").await;
        let terminal = client.expect("command.terminal").await;
        assert_eq!(
            terminal["body"]["terminal"]["status"],
            json!("failed"),
            "规范化结果与持久化取值不同必须拒绝恢复：{terminal}"
        );
        assert_eq!(
            terminal["body"]["terminal"]["error"]["code"],
            json!("nodelink.internal.unavailable"),
            "服务端不可用类（不是 unsupported）：{terminal}"
        );

        // 「不使用新解析出的路径发送 session/resume」：后端**一次都没被调用**。
        assert_eq!(
            owner.resume_probe.calls(),
            calls_before,
            "复校验必须发生在调用后端之前——后端未被调用即「没有发出任何 session/resume」"
        );
        assert!(
            owner.resume_probe.last_request().is_none(),
            "本用例内后端从未收到过任何恢复请求（因此也不可能收到新解析出的路径）"
        );

        // R22：持久化取值不得被改写成新解析出的路径。
        let after = owner
            .sessions
            .load_recovery(&session_key)
            .await
            .expect("读回")
            .expect("恢复数据仍在");
        assert_same_recovery(&tampered, &after, "规范化结果变化被拒之后");

        // 没有产生新会话。
        let listed = owner
            .sessions
            .list(acp_core::ports::SessionQuery::default())
            .await
            .expect("列出会话");
        assert_eq!(listed.len(), 1, "被拒的恢复不得创建新会话：{listed:?}");

        let _ = std::fs::remove_dir_all(&workspace);
        owner.stop().await;
    });
}

/// R25 的 `#[cfg(unix)]` 变体（**符号链接改指**）：持久化路径**仍然存在、仍是目录**，但它已被换成
/// 指向别处的符号链接 ⇒ `canonicalize` 结果与持久化取值不同 ⇒ 拒绝恢复、不发出 `session/resume`。
///
/// 这是 spec 给 R25 举的原例。**执行平台**：本机是 Windows 开发机，本用例**从未执行**；由 Linux CI 的
/// `checks` job（`ubuntu-latest`）真实执行。本地未执行**不等于**通过。
#[cfg(unix)]
#[test]
fn a_persisted_directory_replaced_by_a_symlink_is_refused_before_any_backend_call() {
    support::block_on(async {
        let base = workspace_dir("resume-symlink");
        let real = base.join("real");
        let elsewhere = base.join("elsewhere");
        std::fs::create_dir_all(&real).expect("建立持久化目录");
        std::fs::create_dir_all(&elsewhere).expect("建立改指目标目录");

        let paired =
            Paired::up_to_approval("resume-symlink", ACCESS_WORK, &FULL_GRANTS, &real).await;
        let (owner, access, ticket) = paired.into_parts();
        let mut client = connect(&owner, &access, &ticket).await;
        let (session, attachment) = create_session(&mut client).await;
        let session_id = session["sessionId"].as_str().expect("sessionId").to_owned();
        let session_key = acp_core::model::SessionId::new(&session_id).expect("session id");
        let calls_before = owner.resume_probe.calls();

        let created = owner
            .sessions
            .load_recovery(&session_key)
            .await
            .expect("读回")
            .expect("恢复数据");
        assert_eq!(
            created.workspace_cwd.as_deref(),
            Some(real.to_string_lossy().as_ref()),
            "持久化取值必须是 real 的规范化结果"
        );

        // 把持久化路径**本身**换成指向别处的符号链接（spec 给 R25 举的原例）。
        std::fs::remove_dir_all(&real).expect("移除原目录");
        std::os::unix::fs::symlink(
            std::fs::canonicalize(&elsewhere).expect("改指目标可规范化"),
            &real,
        )
        .expect("建立符号链接");

        // 前提自证（断言，不是假设）。
        let metadata = std::fs::metadata(&real).expect("R25 的前提：路径仍然存在");
        assert!(metadata.is_dir(), "符号链接仍解析为目录");
        let canonical = std::fs::canonicalize(&real).expect("新解析结果可得出");
        assert_ne!(
            canonical.to_str(),
            created.workspace_cwd.as_deref(),
            "R25 的前提：canonicalize 结果与持久化取值逐字不同"
        );
        assert_eq!(
            canonical,
            std::fs::canonicalize(&elsewhere).expect("改指目标可规范化"),
            "新解析出的路径是别处那个目录（回退实现会拿它去恢复）"
        );

        client.step("resume with symlink retargeted");
        client
            .send(
                "command.submit",
                session_resume_body(
                    &support::nodelink::uuid_text(),
                    &session,
                    &attachment,
                    json!({}),
                ),
            )
            .await;
        client.expect("command.accepted").await;
        let terminal = client.expect("command.terminal").await;
        assert_eq!(
            terminal["body"]["terminal"]["status"],
            json!("failed"),
            "符号链接改指必须拒绝恢复：{terminal}"
        );
        assert_eq!(
            terminal["body"]["terminal"]["error"]["code"],
            json!("nodelink.internal.unavailable"),
            "服务端不可用类：{terminal}"
        );
        assert_eq!(
            owner.resume_probe.calls(),
            calls_before,
            "不得用新解析出的路径发出 session/resume"
        );
        let after = owner
            .sessions
            .load_recovery(&session_key)
            .await
            .expect("读回")
            .expect("恢复数据仍在");
        assert_same_recovery(&created, &after, "符号链接改指被拒之后");

        let _ = std::fs::remove_dir_all(&base);
        owner.stop().await;
    });
}

/// R24 的 `#[cfg(unix)]` 变体（**权限丢失**）：持久化目录变得**不可访问** ⇒ 服务端不可用类错误、
/// 不启动/不派发任何东西。
///
/// 权限作用在持久化目录的**父目录**上而不是叶子目录：`stat`/`lstat` 路径上的每个分量都需要**父目录**的
/// search 权限，只把叶子目录 chmod 成 `000` 并不会让 `metadata`/`canonicalize` 失败（叶子自身不参与
/// 自己的查找）。本用例用前提断言自证这一点（下方 `metadata(...).is_err()`），因此即使平台行为与预期
/// 不同也会**响亮地**失败而不是默默退化成另一个场景。
///
/// **执行平台**：本机是 Windows 开发机，本用例**从未执行**；由 Linux CI 的 `checks` job
/// （`ubuntu-latest`，非 root）真实执行。本地未执行**不等于**通过。
#[cfg(unix)]
#[test]
fn an_inaccessible_persisted_directory_is_refused_before_any_backend_call() {
    use std::os::unix::fs::PermissionsExt as _;

    support::block_on(async {
        let base = workspace_dir("resume-chmod");
        let work = base.join("work");
        std::fs::create_dir_all(&work).expect("建立持久化目录");

        let paired = Paired::up_to_approval("resume-chmod", ACCESS_WORK, &FULL_GRANTS, &work).await;
        let (owner, access, ticket) = paired.into_parts();
        let mut client = connect(&owner, &access, &ticket).await;
        let (session, attachment) = create_session(&mut client).await;
        let session_id = session["sessionId"].as_str().expect("sessionId").to_owned();
        let session_key = acp_core::model::SessionId::new(&session_id).expect("session id");
        let calls_before = owner.resume_probe.calls();

        let created = owner
            .sessions
            .load_recovery(&session_key)
            .await
            .expect("读回")
            .expect("恢复数据");
        assert_eq!(
            created.workspace_cwd.as_deref(),
            Some(work.to_string_lossy().as_ref()),
            "持久化取值必须是 work 的规范化结果"
        );

        // 去掉**父目录**的访问权限（Drop 时恢复，避免残留 `000` 污染临时目录）。
        let restore = ModeRestore::new(&base);
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o000)).expect("chmod 000");

        // 前提自证：该形态必须真的让持久化路径不可访问（以 root 运行时本用例的前提不成立，
        // 此时会在这里失败，而不是给出一条「本该失败却通过了」的假证据）。
        assert!(
            std::fs::metadata(&work).is_err(),
            "chmod 000 必须让持久化路径不可访问；若本进程是 root，POSIX 权限检查对它无效"
        );

        client.step("resume with inaccessible workspace");
        client
            .send(
                "command.submit",
                session_resume_body(
                    &support::nodelink::uuid_text(),
                    &session,
                    &attachment,
                    json!({}),
                ),
            )
            .await;
        client.expect("command.accepted").await;
        let terminal = client.expect("command.terminal").await;
        assert_eq!(
            terminal["body"]["terminal"]["status"],
            json!("failed"),
            "不可访问的持久化目录必须拒绝恢复：{terminal}"
        );
        assert_eq!(
            terminal["body"]["terminal"]["error"]["code"],
            json!("nodelink.internal.unavailable"),
            "服务端不可用类：{terminal}"
        );
        assert_eq!(
            owner.resume_probe.calls(),
            calls_before,
            "不可访问时不得启动/派发任何后端"
        );
        let after = owner
            .sessions
            .load_recovery(&session_key)
            .await
            .expect("读回")
            .expect("恢复数据仍在");
        assert_same_recovery(&created, &after, "权限丢失被拒之后");

        drop(restore);
        let _ = std::fs::remove_dir_all(&base);
        owner.stop().await;
    });
}

/// `Drop` 时把目录模式位恢复成 `0700`（`#[cfg(unix)]` 用例用；权限丢失用例在 panic 时也靠它收尾）。
#[cfg(unix)]
struct ModeRestore {
    path: std::path::PathBuf,
}

#[cfg(unix)]
impl ModeRestore {
    fn new(path: &std::path::Path) -> Self {
        Self {
            path: path.to_path_buf(),
        }
    }
}

#[cfg(unix)]
impl Drop for ModeRestore {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o700));
    }
}
