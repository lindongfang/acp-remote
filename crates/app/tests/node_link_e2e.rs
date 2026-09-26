//! 受控路径全链路集成测试（切片 5 的验收闭环，任务 2.22 / `[PV5]` 主体）。
//!
//! 驱动方式：脚本化 fake Access 客户端（`support::nodelink`）经**真实 loopback listener** 走完
//! 配对 claim → 本地确认（真实 `LocalAdminRouter`）→ status approved → 握手（含 `catalogRevision`
//! 验签路径）→ `catalog.snapshot`（grant 交集过滤）→ `session.create` → `resource.attach`/`subscribe`
//! → `resource.event`/`ack` → `session.prompt` 的终态推送 → `export.revoke`/`node.revoke` 的撤销传播。
//! Owner 侧用真实 SQLite + 真实 `Authority` + 组合根自己的装配点（`app::daemon`）。
//!
//! 三条必须的端到端断言（来自检视遗留）：
//!
//! - **R61**：broker 提交失败时连接上**不出现**对应 `resource.event`（`FlakySessionStore` 注入）；
//! - **终态推送**：被接受的 mutation 终态经命令路由的终态观察循环推到提交方连接；
//! - **撤销端到端**：`export.revoke` 后受影响连接收到 `export.revoked` 且后续命令被拒；`node.revoke`
//!   后连接收到 `node.trust.revoked` 并以 4410 关闭。
//!
//! 已知取舍（登记，不修）：① 扇出队列满即丢（`EVENT_QUEUE_CAPACITY`，RV1-WP5 残余①）；② 「会话阻塞在
//! socket 写」路径（RV1-WP4 L5）；③ 预持久化失败可能留下无端点的会话行（RV2-WP6 残余⑤）。

mod support;

use serde_json::{Value, json};
use server::local_admin::Method;
use support::nodelink::{AccessKey, NodeLinkClient, PairingTicket};
use support::owner::OwnerNode;

/// 本用例扮演的 Access Node 标识。
const ACCESS_NODE: &str = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";
/// 可见的 Export（`scopes` 与节点 grants 有交集）。
const EXPORT_VISIBLE: &str = "export.visible";
/// 不可见的 Export（`scopes` 与节点 grants 不相交）。
const EXPORT_HIDDEN: &str = "export.hidden";
/// 注册的 workspace alias（`session.create` 只能带别名，不能带路径）。
const WORKSPACE_ALIAS: &str = "project.one";
/// Agent selector。
const AGENT: &str = "codex";
/// 本地确认授予的 grants（也是 Export 的 `scopes` 取值域）。
const GRANTS: [&str; 3] = ["grant.observe", "grant.interact", "grant.remote-work"];

/// 一次完整的配对准备（Owner 侧的 profile/workspace/Export + 配对开始 + claim + 本地确认 + status）。
struct Chain {
    owner: OwnerNode,
    access: AccessKey,
    ticket: PairingTicket,
}

impl Chain {
    /// 走到「status approved」为止（不含 WSS 握手）。
    async fn up_to_approval(label: &str) -> Self {
        let owner = OwnerNode::start(label).await;
        let access = AccessKey::new(ACCESS_NODE);
        // ① 本机注册：workspace alias + agent profile + 两个 Export。
        let workspace_root = owner.data_dir();
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
        for (export_id, scopes) in [
            (EXPORT_VISIBLE, GRANTS.to_vec()),
            (EXPORT_HIDDEN, vec!["grant.approve"]),
        ] {
            owner
                .admin(
                    Method::ExportCreate,
                    json!({
                        "exportId": export_id,
                        "displayName": format!("Export {export_id}"),
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
                        "scopes": scopes,
                        "cachePolicy": "no-content-cache",
                    }),
                )
                .await;
        }

        // ② 配对开始（二维码）。
        let begin = owner
            .admin(
                Method::NodePairBegin,
                json!({
                    "mode": "owner",
                    "pairingUrl": null,
                    "displayName": "Owner Node",
                    "requestedGrants": GRANTS,
                }),
            )
            .await;
        let pairing_id = begin["pairingId"].as_str().expect("pairingId").to_owned();
        let pairing_url = begin["pairingUrl"].as_str().expect("pairingUrl");
        let ticket = support::nodelink::ticket_from_url(pairing_url, &pairing_id);

        // ③ Access 侧 claim（HMAC proof），并验证 Owner 证明。
        let client_nonce = support::nodelink::nonce_text(0x41);
        let claim = support::nodelink::claim_request(&ticket, &access, &client_nonce);
        let reply = support::nodelink::post_json(
            owner.addr,
            server::node_link::CLAIM_PATH,
            &serde_json::to_vec(&claim).expect("claim 可序列化"),
        )
        .await;
        assert_eq!(
            reply.status,
            201,
            "合法 claim 必须返回 201：{}",
            String::from_utf8_lossy(&reply.body)
        );
        let body = reply.json();
        support::nodelink::verify_owner_proof(&ticket, &body, &access, &client_nonce);

        // ④ 本地确认（真实的 local_admin 入口）。
        let confirmed = owner
            .admin(
                Method::NodePairConfirm,
                json!({ "pairingId": pairing_id, "grants": GRANTS }),
            )
            .await;
        assert_eq!(
            confirmed["nodeId"],
            json!(ACCESS_NODE),
            "确认必须创建该 Access Node 的信任记录"
        );

        // ⑤ status approved（同一 secret 的 HMAC 证明）。
        let pairing_request_id = body["pairingRequestId"]
            .as_str()
            .expect("pairingRequestId")
            .to_owned();
        let server_nonce = body["serverNonce"]
            .as_str()
            .expect("serverNonce")
            .to_owned();
        let status = support::nodelink::status_request(
            &ticket,
            &access,
            &pairing_request_id,
            &support::nodelink::nonce_text(0x42),
        );
        let reply = support::nodelink::post_json(
            owner.addr,
            server::node_link::STATUS_PATH,
            &serde_json::to_vec(&status).expect("status 可序列化"),
        )
        .await;
        assert_eq!(reply.status, 200, "status 一律 200");
        let body = reply.json();
        assert_eq!(body["status"], json!("approved"), "{body}");
        assert_eq!(body["node"]["accessNodeId"], json!(ACCESS_NODE), "{body}");

        // 配对材料由 ticket（secret/endpoint/owner 公钥）承载；`pairingRequestId`/`serverNonce` 只用于
        // 出 status 请求，取到即用完，不必在 Chain 上留字段。
        let _ = (&pairing_request_id, &server_nonce);
        Self {
            owner,
            access,
            ticket,
        }
    }

    /// 在真实 loopback listener 上完成 WSS 握手（含 `catalogRevision` 验签）。
    async fn connect(&self) -> NodeLinkClient {
        let mut client = NodeLinkClient::connect_plain(self.owner.addr).await;
        support::nodelink::handshake(
            &mut client,
            &self.access,
            &self.ticket,
            &support::nodelink::nonce_text(0x41),
        )
        .await;
        client
    }
}

/// 主人的 `session.create` 请求（`payload` 只允许四个键，禁带 `cwd`/`mcpServers`）。
fn session_create_body(request_id: &str, export_id: &str, alias: &str) -> Value {
    json!({
        "requestId": request_id,
        "command": "session.create",
        "sessionRef": null,
        "attachmentId": null,
        "attachmentGeneration": null,
        "expectedVersion": null,
        "payload": {
            "agentId": AGENT,
            "exportId": export_id,
            "workspaceAlias": alias,
        },
    })
}

/// `session.prompt`（session-scoped：必须带代际）。
fn session_prompt_body(request_id: &str, session: &Value, attachment: &Value, text: &str) -> Value {
    json!({
        "requestId": request_id,
        "command": "session.prompt",
        "sessionRef": session.clone(),
        "attachmentId": attachment["attachmentId"],
        "attachmentGeneration": attachment["attachmentGeneration"],
        "expectedVersion": null,
        "payload": { "content": [ { "type": "text", "text": text } ] },
    })
}

/// 主链：配对 → 握手 → catalog → session.create → attach/subscribe → event/ack → 终态 → R61 → 撤销。
#[test]
fn the_controlled_path_runs_end_to_end_and_revocation_propagates() {
    support::block_on(async {
        let chain = Chain::up_to_approval("e2e").await;
        let owner = &chain.owner;
        let mut client = chain.connect().await;

        // ① catalog.snapshot：可见性 = 未撤销 ∧ scopes ∩ 节点 grants ≠ ∅。
        client.step("catalog.subscribe");
        client
            .send("catalog.subscribe", json!({ "knownRevision": null }))
            .await;
        let snapshot = client.expect("catalog.snapshot").await;
        let exports = snapshot["body"]["exports"]
            .as_array()
            .expect("exports 数组");
        let ids: Vec<&str> = exports
            .iter()
            .map(|entry| entry["exportId"].as_str().expect("exportId"))
            .collect();
        assert_eq!(
            ids,
            vec![EXPORT_VISIBLE],
            "可见集必须只含 scopes 与节点 grants 有交集的 Export：{snapshot}"
        );
        assert!(
            !ids.contains(&EXPORT_HIDDEN),
            "与节点 grants 不相交的 Export 不得出现在 catalog 里"
        );

        // ② session.create 正常路径：accepted(result = null) → terminal(SessionCreateResult)。
        let create_request = support::nodelink::uuid_text();
        client.step("session.create");
        client
            .send(
                "command.submit",
                session_create_body(&create_request, EXPORT_VISIBLE, WORKSPACE_ALIAS),
            )
            .await;
        let accepted = client.expect("command.accepted").await;
        assert_eq!(accepted["body"]["requestId"], json!(create_request));
        assert_eq!(accepted["body"]["command"], json!("session.create"));
        assert_eq!(
            accepted["body"]["result"],
            Value::Null,
            "`session.create` 的 accepted 必须是 result = null"
        );
        let terminal = client.expect("command.terminal").await;
        assert_eq!(terminal["body"]["requestId"], json!(create_request));
        assert_eq!(terminal["body"]["terminal"]["status"], json!("completed"));
        let result = &terminal["body"]["terminal"]["result"];
        let session = result["remoteSessionRef"].clone();
        assert_eq!(
            session["ownerNodeId"],
            json!(owner.owner_node_id().as_str()),
            "Owner 必须回自己的 node id（传错会让全部 attach/ack 被拒）"
        );
        assert_eq!(session["exportId"], json!(EXPORT_VISIBLE));
        assert_eq!(result["sessionMeta"]["state"], json!("idle"));
        let session_id = session["sessionId"].as_str().expect("sessionId").to_owned();

        // ③ 幂等重试：同 requestId + 同 payload → 直接回首次结果；同 requestId 换 payload →
        // `idempotency_conflict`（不能因为重试而创建第二个会话）。
        client.step("idempotent retry");
        client
            .send(
                "command.submit",
                session_create_body(&create_request, EXPORT_VISIBLE, WORKSPACE_ALIAS),
            )
            .await;
        let replay = client.expect("command.terminal").await;
        assert_eq!(
            replay["body"]["terminal"]["result"]["remoteSessionRef"]["sessionId"],
            json!(session_id),
            "同键重试必须回首次结果（不创建第二个会话）：{replay}"
        );
        // 换一个「同样合法、但语义不同」的 payload：显式给空的 `templateParams`（§12.7 允许空参数集，
        // 但它属于被指纹的 payload 字节），因此必须命中冲突而不是回首次结果。
        client.step("idempotency conflict");
        let mut drifted = session_create_body(&create_request, EXPORT_VISIBLE, WORKSPACE_ALIAS);
        drifted["payload"]["templateParams"] = json!({});
        client.send("command.submit", drifted).await;
        let conflict = client.expect("command.rejected").await;
        assert_eq!(
            conflict["body"]["requestId"],
            json!(create_request),
            "冲突判定必须针对同一 requestId：{conflict}"
        );
        assert_eq!(
            conflict["body"]["error"]["code"],
            json!("nodelink.command.idempotency_conflict"),
            "同 requestId 换语义必须冲突：{conflict}"
        );

        // ④ 拒绝路径：禁带字段 → unsupported_field；未知 workspaceAlias → not_granted。
        client.step("forbidden field");
        let mut forbidden = session_create_body(
            &support::nodelink::uuid_text(),
            EXPORT_VISIBLE,
            WORKSPACE_ALIAS,
        );
        forbidden["payload"]["cwd"] = json!("/tmp/attacker");
        client.send("command.submit", forbidden).await;
        let rejected = client.expect("command.rejected").await;
        assert_eq!(
            rejected["body"]["error"]["code"],
            json!("nodelink.command.unsupported_field")
        );
        assert_eq!(rejected["body"]["error"]["details"]["field"], json!("cwd"));

        client.step("unknown workspace alias");
        client
            .send(
                "command.submit",
                session_create_body(
                    &support::nodelink::uuid_text(),
                    EXPORT_VISIBLE,
                    "not.registered",
                ),
            )
            .await;
        let rejected = client.expect("command.rejected").await;
        assert_eq!(
            rejected["body"]["error"]["code"],
            json!("nodelink.export.not_granted")
        );
        assert_eq!(
            rejected["body"]["error"]["details"]["parameter"],
            json!("workspaceAlias")
        );

        // ⑤ resource.attach → 新 generation；旧代际被拒。
        client.step("resource.attach");
        client
            .send("resource.attach", json!({ "remoteSessionRef": session }))
            .await;
        let attached = client.expect("resource.attached").await;
        let attachment = json!({
            "attachmentId": attached["body"]["attachmentId"],
            "attachmentGeneration": attached["body"]["attachmentGeneration"],
        });

        // ⑥ resource.subscribe(cursor = null) → 快照（只有元数据）。
        client.step("resource.subscribe");
        client
            .send(
                "resource.subscribe",
                json!({
                    "attachmentId": attachment["attachmentId"],
                    "attachmentGeneration": attachment["attachmentGeneration"],
                    "cursor": null,
                }),
            )
            .await;
        let begin = client.expect("resource.snapshot_begin").await;
        let chunk = client.expect("resource.snapshot_chunk").await;
        let end = client.expect("resource.snapshot_end").await;
        assert_eq!(
            begin["body"]["snapshotId"], end["body"]["snapshotId"],
            "快照的 snapshotId 必须首尾一致"
        );
        assert_eq!(begin["body"]["chunkCount"], json!(1));
        assert_eq!(end["body"]["chunkCount"], json!(1));
        assert!(
            end["body"]["snapshotDigest"]
                .as_str()
                .is_some_and(|digest| !digest.is_empty()),
            "快照必须以非空 digest 收尾：{end}"
        );
        assert_eq!(chunk["body"]["resource"], json!("session_meta"));
        assert_eq!(chunk["body"]["chunkIndex"], json!("0"));
        let items = chunk["body"]["items"].as_array().expect("items 数组");
        assert_eq!(items.len(), 1, "会话快照只承载 session_meta：{chunk}");
        assert_eq!(items[0]["sessionRef"], session);
        assert_eq!(items[0]["sessionMeta"]["state"], json!("idle"));
        assert!(
            !chunk.to_string().contains("\"content\""),
            "快照不得携带正文：{chunk}"
        );
        let cursor = end["body"]["cursor"].clone();

        // ⑦ resource.ack（累计水位；无响应，只能靠后续 ping 证明连接仍然健康）。
        client.step("resource.ack");
        client
            .send(
                "resource.ack",
                json!({ "sessionRef": session, "cursor": cursor }),
            )
            .await;
        ping(&mut client, 0x51).await;

        // ⑧ 事件扇出 + 终态推送：accepted 先回，后端事件稍后由用例释放，终态经观察循环推送。
        let ok_marker = "marker-ok-4f2a";
        let prompt_request = support::nodelink::uuid_text();
        client.step("session.prompt");
        client
            .send(
                "command.submit",
                session_prompt_body(&prompt_request, &session, &attachment, ok_marker),
            )
            .await;
        // turn 尚未结束（脚本端点暂存了事件），因此回复是 `command.accepted` 而不是终态。
        let accepted = client.expect("command.accepted").await;
        assert_eq!(accepted["body"]["requestId"], json!(prompt_request));
        assert_eq!(accepted["body"]["command"], json!("session.prompt"));
        let accepted_turn = accepted["body"]["result"]["turnId"]
            .as_str()
            .expect("active turn 的 accepted 必须带 turnId")
            .to_owned();
        // 会话状态机先行的两条事件（submit 落 `turn.queued`、派发落 `turn.started`）。
        let lifecycle = client.collect(std::time::Duration::from_millis(300)).await;
        let lifecycle_types: Vec<&str> = lifecycle
            .iter()
            .filter(|message| message["type"] == json!("resource.event"))
            .map(|message| message["body"]["eventType"].as_str().unwrap_or("<无>"))
            .collect();
        assert_eq!(
            lifecycle_types,
            vec!["turn.queued", "turn.started"],
            "会话状态必须先走 queued → running：{lifecycle:?}"
        );
        assert!(
            !lifecycle
                .iter()
                .any(|message| message.to_string().contains(ok_marker)),
            "释放之前不得有正文事件：{lifecycle:?}"
        );

        // 释放后端事件（模拟 Agent 稍后产出），再驱动合并窗口落盘 + 广播。
        assert_eq!(owner.release_events(ok_marker), 2, "delta + turn 终态");
        owner
            .broker
            .pump(&session_key(&session_id))
            .await
            .expect("提交后端事件");
        let event = client.expect("resource.event").await;
        assert_eq!(event["body"]["eventType"], json!("agent.message.delta"));
        assert_eq!(event["body"]["sessionRef"]["sessionId"], json!(session_id));
        assert_eq!(
            event["body"]["sessionRef"]["ownerNodeId"],
            json!(owner.owner_node_id().as_str())
        );
        assert!(
            event["body"]["payload"]["view"]
                .to_string()
                .contains(ok_marker),
            "事件正文必须是本次 prompt 的内容：{event}"
        );
        assert!(
            event["body"]["payload"].get("acp").is_none(),
            "view-only 事件不得伪造 ACP 原文：{event}"
        );
        // 终态：turn 结束后由命令路由的终态观察循环推给提交方连接（RV1-WP6-F9/F4）。
        let terminal = client.expect("command.terminal").await;
        assert_eq!(terminal["body"]["requestId"], json!(prompt_request));
        assert_eq!(terminal["body"]["terminal"]["status"], json!("completed"));
        assert_eq!(
            terminal["body"]["terminal"]["result"]["turnId"],
            json!(accepted_turn),
            "终态结果必须回本次 turn（与 accepted 的 turnId 一致）：{terminal}"
        );
        assert!(
            terminal["body"]["terminal"]["terminalEventId"].is_string(),
            "终态必须带 terminalEventId：{terminal}"
        );
        client
            .send(
                "resource.ack",
                json!({
                    "sessionRef": session,
                    "cursor": {
                        "originEpoch": event["body"]["originEpoch"],
                        "originSequence": event["body"]["originSequence"],
                    },
                }),
            )
            .await;
        ping(&mut client, 0x52).await;

        // ⑨ R61：broker 提交失败时连接上不出现对应的 `resource.event`（故障注入）。
        //
        // 注入点只让「带 marker 的那一批」失败（＝ agent.message.delta 那批），§6.9 规定失败批次一律不发布。
        // 该批属于**在跑的** turn，因此 §6 第 9 条（RV1-WP7-F3）之后：同一个 turn 的终态批也不得照常
        // 落盘——broker 把该 turn 终结为 `turn.failed`、把命令置为 `uncertain`（正文缺块的 turn 不报完成），
        // 迟到的事件与终态被丢弃（而不是被记到下一个 turn 上）。
        let fault_marker = "marker-fault-9c11";
        let fault_request = support::nodelink::uuid_text();
        client.step("faulted prompt");
        client
            .send(
                "command.submit",
                session_prompt_body(&fault_request, &session, &attachment, fault_marker),
            )
            .await;
        let accepted = client.expect("command.accepted").await;
        assert_eq!(accepted["body"]["requestId"], json!(fault_request));
        owner.fail_commits_with(Some(fault_marker));
        assert_eq!(owner.release_events(fault_marker), 2, "delta + turn 终态");
        owner
            .broker
            .pump(&session_key(&session_id))
            .await
            .expect("写失败按 §6.9 收口为「不发布」，不向调用方冒泡");
        assert!(
            owner
                .rejected_commits
                .load(std::sync::atomic::Ordering::SeqCst)
                >= 1,
            "故障必须真的命中一次 commit"
        );
        owner.fail_commits_with(None);
        // 连接仍然可用（后面的撤销推送还要走它），且失败批次的事件一条也没有出现。
        ping(&mut client, 0x53).await;
        let after_fault = client.collect(std::time::Duration::from_millis(500)).await;
        assert!(
            !after_fault
                .iter()
                .any(|message| message.to_string().contains(fault_marker)),
            "提交失败的事件批次不得出现在连接上：{after_fault:?}"
        );
        assert!(
            !after_fault
                .iter()
                .any(|message| message["body"]["eventType"] == json!("agent.message.delta")),
            "失败的 delta 批次不得有任何一条到达：{after_fault:?}"
        );
        let failed_turn = after_fault
            .iter()
            .find(|message| message["body"]["eventType"] == json!("turn.failed"))
            .cloned()
            .unwrap_or_else(|| panic!("正文缺块的 turn 必须显式失败：{after_fault:?}"));
        assert_eq!(
            failed_turn["body"]["sessionRef"]["sessionId"],
            json!(session_id)
        );
        // 同一个 turn 不得再出现 `turn.completed`（本连接上还有前面正常轮次的事件，因此按 turnId 比对）。
        let abandoned_turn = failed_turn["body"]["payload"]["view"]["turnId"].clone();
        assert!(
            !after_fault.iter().any(|message| {
                message["body"]["payload"]["view"]["turnId"] == abandoned_turn
                    && message["body"]["eventType"] == json!("turn.completed")
            }),
            "正文缺块的 turn 不得以 completed 收尾：{after_fault:?}"
        );
        // 终态可能已经被上面那 500 ms 收走（观察循环 250 ms 一跳），两条路径都接受。
        let terminal = match after_fault
            .iter()
            .find(|message| message["type"] == json!("command.terminal"))
        {
            Some(terminal) => terminal.clone(),
            None => client.expect("command.terminal").await,
        };
        assert_eq!(terminal["body"]["requestId"], json!(fault_request));
        assert_eq!(
            terminal["body"]["terminal"]["status"],
            json!("uncertain"),
            "正文缺块的命令不能报 completed：{terminal}"
        );
        assert_eq!(
            terminal["body"]["terminal"]["error"]["code"],
            json!("nodelink.command.uncertain"),
            "{terminal}"
        );

        // ⑩ export.revoke：受影响连接收到 export.revoked，之后的命令被拒。
        owner
            .admin(Method::ExportRevoke, json!({ "exportId": EXPORT_VISIBLE }))
            .await;
        let revoked = client.expect("export.revoked").await;
        assert_eq!(revoked["body"]["exportId"], json!(EXPORT_VISIBLE));
        client.step("command after export revoke");
        client
            .send(
                "command.submit",
                session_prompt_body(
                    &support::nodelink::uuid_text(),
                    &session,
                    &attachment,
                    "marker-after-revoke",
                ),
            )
            .await;
        let rejected = client.expect("command.rejected").await;
        assert_eq!(
            rejected["body"]["error"]["code"],
            json!("nodelink.export.not_granted"),
            "撤销后同一 Export 上的命令必须被拒：{rejected}"
        );

        // ⑪ node.revoke：推送 node.trust.revoked 并以 4410 关闭连接。
        owner
            .admin(Method::NodeRevoke, json!({ "nodeId": ACCESS_NODE }))
            .await;
        let revoked = client.expect("node.trust.revoked").await;
        assert_eq!(revoked["body"]["revokedNodeId"], json!(ACCESS_NODE));
        assert_eq!(
            client.close_code().await,
            4410,
            "节点撤销必须以 4410 关闭连接"
        );

        chain.owner.stop().await;
    });
}

/// TLS `direct` 轮次：自签证书 + rustls，走完同一批配对/握手步骤（配对 HTTP 也在 TLS 之上）。
#[test]
fn tls_direct_terminates_the_same_handshake() {
    support::block_on(async {
        let certificate = TestCertificate::generate("e2e-tls");
        let config = certificate.client_config();
        let owner =
            OwnerNode::start_tls("e2e-tls", &certificate.cert_path, &certificate.key_path).await;
        let access = AccessKey::new(ACCESS_NODE);

        // ① 配对开始 + claim（TLS POST）。
        let begin = owner
            .admin(
                Method::NodePairBegin,
                json!({
                    "mode": "owner",
                    "pairingUrl": null,
                    "displayName": "Owner Node",
                    "requestedGrants": GRANTS,
                }),
            )
            .await;
        let pairing_id = begin["pairingId"].as_str().expect("pairingId").to_owned();
        let ticket = support::nodelink::ticket_from_url(
            begin["pairingUrl"].as_str().expect("pairingUrl"),
            &pairing_id,
        );
        let client_nonce = support::nodelink::nonce_text(0x61);
        let claim = support::nodelink::claim_request(&ticket, &access, &client_nonce);
        let reply = support::nodelink::post_json_tls(
            owner.addr,
            server::node_link::CLAIM_PATH,
            &serde_json::to_vec(&claim).expect("claim 可序列化"),
            std::sync::Arc::clone(&config),
        )
        .await;
        assert_eq!(
            reply.status,
            201,
            "TLS 上的合法 claim 必须返回 201：{}",
            String::from_utf8_lossy(&reply.body)
        );
        let body = reply.json();
        support::nodelink::verify_owner_proof(&ticket, &body, &access, &client_nonce);

        // ② 本地确认 + status approved。
        owner
            .admin(
                Method::NodePairConfirm,
                json!({ "pairingId": pairing_id, "grants": GRANTS }),
            )
            .await;
        let status = support::nodelink::status_request(
            &ticket,
            &access,
            body["pairingRequestId"].as_str().expect("pairingRequestId"),
            &support::nodelink::nonce_text(0x62),
        );
        let reply = support::nodelink::post_json_tls(
            owner.addr,
            server::node_link::STATUS_PATH,
            &serde_json::to_vec(&status).expect("status 可序列化"),
            std::sync::Arc::clone(&config),
        )
        .await;
        assert_eq!(reply.json()["status"], json!("approved"));

        // ③ WSS（TLS 之上）完成同一握手，并取回一份 catalog 快照。
        let mut client =
            NodeLinkClient::connect_tls(owner.addr, std::sync::Arc::clone(&config)).await;
        support::nodelink::handshake(&mut client, &access, &ticket, &client_nonce).await;
        client
            .send("catalog.subscribe", json!({ "knownRevision": null }))
            .await;
        let snapshot = client.expect("catalog.snapshot").await;
        assert_eq!(
            snapshot["body"]["exports"]
                .as_array()
                .expect("exports")
                .len(),
            0,
            "本用例没有创建 Export，快照因此是空集（握手与 catalog 都跑在 TLS 上）"
        );
        owner.stop().await;
    });
}

/// 自签证书（`rcgen`）与它的 PEM 路径。
struct TestCertificate {
    directory: std::path::PathBuf,
    cert_path: std::path::PathBuf,
    key_path: std::path::PathBuf,
    der: rustls_pki_types::CertificateDer<'static>,
}

impl TestCertificate {
    fn generate(label: &str) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "acpr-wp7-e2e-cert-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("创建临时目录");
        let certified = rcgen::generate_simple_self_signed(vec![
            support::nodelink::PUBLIC_HOST.to_owned(),
            "localhost".to_owned(),
        ])
        .expect("自签证书");
        let cert_path = directory.join("cert.pem");
        let key_path = directory.join("key.pem");
        std::fs::write(&cert_path, certified.cert.pem()).expect("写证书");
        std::fs::write(&key_path, certified.signing_key.serialize_pem()).expect("写私钥");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&cert_path, std::fs::Permissions::from_mode(0o600))
                .expect("chmod 0600");
            std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))
                .expect("chmod 0600");
        }
        Self {
            directory,
            cert_path,
            key_path,
            der: certified.cert.der().clone(),
        }
    }

    /// 只信任本用例现算的那张证书的 rustls 客户端配置（不做危险的「跳过校验」）。
    fn client_config(&self) -> std::sync::Arc<rustls::ClientConfig> {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(self.der.clone()).expect("自签证书可加入信任根");
        let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("默认协议版本可用")
            .with_root_certificates(roots)
            .with_no_client_auth();
        std::sync::Arc::new(config)
    }
}

impl Drop for TestCertificate {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// `link.ping` 往返：证明连接在拒绝/失败之后仍然处于合法状态。
async fn ping(client: &mut NodeLinkClient, seed: u8) {
    let nonce = support::nodelink::nonce_text_16(seed);
    client.step("link.ping");
    client.send("link.ping", json!({ "nonce": nonce })).await;
    let pong = client.expect("link.pong").await;
    assert_eq!(pong["body"]["nonce"], json!(nonce));
}

/// `remoteSessionRef.sessionId` → `SessionId`。
fn session_key(session_id: &str) -> acp_core::model::SessionId {
    acp_core::model::SessionId::new(session_id).expect("sessionId 是规范 uuid 文本")
}
