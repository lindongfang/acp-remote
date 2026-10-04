//! 受控路径全链路集成测试（切片 5 的验收闭环，任务 2.22 / `[PV5]` 主体）。
//!
//! 驱动方式：脚本化 fake Access 客户端（`support::nodelink`）经**真实 loopback listener** 走完
//! 配对 claim → 本地确认（真实 `LocalAdminRouter`）→ status approved → 握手（含 `catalogRevision`
//! 验签路径）→ `catalog.snapshot`（未撤销 ∧ grants 相交 ∧ 在确认点名的 `exportIds` 清单内）→
//! `session.create` → `resource.attach`/`subscribe`
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
/// 可见的 Export（`scopes` 与节点 grants 有交集，且是本次确认点名的唯一一条）。
const EXPORT_VISIBLE: &str = "export.visible";
/// 不可见的 Export（`scopes` 与节点 grants 不相交）。
const EXPORT_HIDDEN: &str = "export.hidden";
/// 可见性被清单收窄的 Export：`scopes` 与节点 grants **相交**，但本次确认没有点名它
/// （`design.md` D1 的条件③）。
const EXPORT_NARROWED: &str = "export.narrowed";
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
            // 第三条的 scopes 与节点 grants 相交：它能被排除只能来自确认时点名的清单。
            (EXPORT_NARROWED, GRANTS.to_vec()),
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

        // ②③ 配对开始（二维码）+ Access 侧 claim（HMAC proof），并验证 Owner 证明。
        let (ticket, body) = begin_and_claim(&owner, &access, 0x41).await;
        let pairing_id = ticket.pairing_id.clone();

        // ④ 本地确认（真实的 local_admin 入口）：`exportIds` 是必需参数，本例只点名 `EXPORT_VISIBLE`，
        // 因此 `EXPORT_NARROWED`（scopes 相交）也必须对该节点不可见。
        let confirmed = owner
            .admin(
                Method::NodePairConfirm,
                json!({
                    "pairingId": pairing_id,
                    "grants": GRANTS,
                    "exportIds": [EXPORT_VISIBLE],
                }),
            )
            .await;
        assert_eq!(
            confirmed["nodeId"],
            json!(ACCESS_NODE),
            "确认必须创建该 Access Node 的信任记录"
        );
        assert_eq!(
            confirmed["exportIds"],
            json!([EXPORT_VISIBLE]),
            "confirm 的 result 必须回显本次点名的清单"
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

    /// 同一 Access 身份对同一 Owner **再**走一次配对（不 `node.revoke`），并用 `export_ids` 在本机确认。
    ///
    /// 返回本次配对的 ticket（重新握手要用它取 Owner 身份材料）与 `confirm` 的 result。
    /// `nonce_seed` 必须与首次配对不同，避免两个配对共用同一个 client nonce。
    async fn repair_and_confirm(
        &self,
        export_ids: Value,
        nonce_seed: u8,
    ) -> (PairingTicket, Value) {
        let (ticket, _claim) = begin_and_claim(&self.owner, &self.access, nonce_seed).await;
        let confirmed = self
            .owner
            .admin(
                Method::NodePairConfirm,
                json!({
                    "pairingId": ticket.pairing_id.clone(),
                    "grants": GRANTS,
                    "exportIds": export_ids,
                }),
            )
            .await;
        (ticket, confirmed)
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

/// 配对准备（②③）：`node.pair.begin --mode owner` → Access 侧 claim（HMAC proof）→ 验 Owner 证明。
///
/// 返回本次配对的 ticket 与 claim 响应体（`pairingRequestId`/`serverNonce` 供 status 用）。
async fn begin_and_claim(
    owner: &OwnerNode,
    access: &AccessKey,
    nonce_seed: u8,
) -> (PairingTicket, Value) {
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

    let client_nonce = support::nodelink::nonce_text(nonce_seed);
    let claim = support::nodelink::claim_request(&ticket, access, &client_nonce);
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
    support::nodelink::verify_owner_proof(&ticket, &body, access, &client_nonce);
    (ticket, body)
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

        // ① catalog.snapshot：可见性 = 未撤销 ∧ scopes ∩ 节点 grants ≠ ∅ ∧ 在确认点名的清单内。
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
            "可见集必须只含同时满足三个条件的 Export：{snapshot}"
        );
        assert!(
            !ids.contains(&EXPORT_HIDDEN),
            "与节点 grants 不相交的 Export 不得出现在 catalog 里"
        );
        assert!(
            !ids.contains(&EXPORT_NARROWED),
            "清单外的 Export 不得出现在 catalog 里（即使 scopes ∩ grants 非空）"
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

        // ④b 清单外的 Export 不能经 `resource.attach` 进入（与 catalog 同一个判定点）：即使给出的是一个
        // 合法会话的 `sessionRef`，把 `exportId` 换成清单外的那个也会被可见性复核挡住。
        client.step("resource.attach on a narrowed export");
        let mut narrowed = session.clone();
        narrowed["exportId"] = json!(EXPORT_NARROWED);
        client
            .send("resource.attach", json!({ "remoteSessionRef": narrowed }))
            .await;
        let refused = client.expect("link.error").await;
        assert_eq!(
            refused["body"]["code"],
            json!("nodelink.export.not_granted"),
            "清单外的 Export 与「与 grants 不相交」同一个判定点、同一个错误码：{refused}"
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

/// 同一节点在**不撤销**的情况下重新配对并把清单收窄时，早已建立的 attachment 必须随连接作废。
///
/// 这条路径（`node.pair.begin → claim → node.pair.confirm`）不经过 `node.revoke`，而 `approve_node` 对
/// 「同一 `nodeId` 已有 `paired` 行」没有状态守卫，因此清单可以被静默改写。可见性只在适配器
/// （`node_link::catalog`）与 core 的 Owner 侧命令授权上判定：若确认不落到**连接边界**，这条连接会继续
/// 按旧清单收到该 Export 的 `resource.event`（本用例的负向对照就是删掉 `node_reauth` 的调用）。
///
/// 关闭语义也是用例的一部分：收窄不是撤销，必须以 **1000（正常关闭）**关闭、且全程不给对端
/// `node.trust.revoked`——合规客户端把 4410 与那条消息都读成「停止重连」（`NODE_LINK_PROTOCOL.md`
/// §15、§14.2），复用撤销路径会把「重连取新 catalog」变成「已被撤销、不再重连」。
#[test]
fn a_narrowing_repair_closes_the_live_attachment() {
    support::block_on(async {
        let chain = Chain::up_to_approval("e2e-repair").await;
        let owner = &chain.owner;
        let mut client = chain.connect().await;

        // ① 重新配对之前：本次点名清单里的 Export 可见（与主链同一个三条件判定点）。
        client.step("catalog.subscribe");
        client
            .send("catalog.subscribe", json!({ "knownRevision": null }))
            .await;
        let snapshot = client.expect("catalog.snapshot").await;
        let visible: Vec<&str> = snapshot["body"]["exports"]
            .as_array()
            .expect("exports 数组")
            .iter()
            .map(|entry| entry["exportId"].as_str().expect("exportId"))
            .collect();
        assert_eq!(visible, vec![EXPORT_VISIBLE], "收窄之前：{snapshot}");

        // ② 在该 Export 上建立一条**活的** attachment：会话 → attach → subscribe → 收到一条正文事件。
        client.step("session.create");
        client
            .send(
                "command.submit",
                session_create_body(
                    &support::nodelink::uuid_text(),
                    EXPORT_VISIBLE,
                    WORKSPACE_ALIAS,
                ),
            )
            .await;
        let _ = client.expect("command.accepted").await;
        let terminal = client.expect("command.terminal").await;
        assert_eq!(terminal["body"]["terminal"]["status"], json!("completed"));
        let session = terminal["body"]["terminal"]["result"]["remoteSessionRef"].clone();
        let session_id = session["sessionId"].as_str().expect("sessionId").to_owned();

        client.step("resource.attach");
        client
            .send("resource.attach", json!({ "remoteSessionRef": session }))
            .await;
        let attached = client.expect("resource.attached").await;
        let attachment = json!({
            "attachmentId": attached["body"]["attachmentId"],
            "attachmentGeneration": attached["body"]["attachmentGeneration"],
        });

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
        let _ = client.expect("resource.snapshot_begin").await;
        let _ = client.expect("resource.snapshot_chunk").await;
        let _ = client.expect("resource.snapshot_end").await;

        let marker = "marker-before-repair-71bd";
        client.step("session.prompt");
        client
            .send(
                "command.submit",
                session_prompt_body(
                    &support::nodelink::uuid_text(),
                    &session,
                    &attachment,
                    marker,
                ),
            )
            .await;
        let _ = client.expect("command.accepted").await;
        assert_eq!(owner.release_events(marker), 2, "delta + turn 终态");
        owner
            .broker
            .pump(&session_key(&session_id))
            .await
            .expect("提交后端事件");
        // 会话状态机先行的 `turn.queued`/`turn.started` 也走 `resource.event`，因此要等到带本次 marker 的
        // 正文事件才算「这条 attachment 真的在收事件」。
        loop {
            let candidate = client.expect("resource.event").await;
            if candidate["body"]["payload"]["view"]
                .to_string()
                .contains(marker)
            {
                break;
            }
        }

        // ③ 用更窄的清单（空 = 无可见）重新配对同一节点并确认：全程不 revoke。
        let (ticket, confirmed) = chain.repair_and_confirm(json!([]), 0x71).await;
        assert_eq!(
            confirmed["nodeId"],
            json!(ACCESS_NODE),
            "重新配对必须落在同一节点身份上（没有 revoke、也没有新 nodeId）：{confirmed}"
        );
        assert_eq!(
            confirmed["exportIds"],
            json!([]),
            "本次确认的清单必须如实回显（空清单 = 看不到任何 Export）：{confirmed}"
        );

        // ④ 授权变化必须落到连接边界：既有连接被关闭，新连接上的判定全部按已提交的信任行重算。
        // 但该节点并未撤销，因此这里**既没有** `node.trust.revoked` 推送，**也不用** 4410：关闭是以
        // 1000（正常关闭）发出的「重连取新 catalog」信号。若确认不作废连接，这条连接会继续收到该 Export
        // 的 `resource.event`——超时即「连接仍然活着」就是那个缺口。
        client.step("repair close");
        let code = tokio::time::timeout(std::time::Duration::from_secs(10), client.close_code())
            .await
            .expect(
                "重新配对确认后既有连接必须被关闭（超时 = 连接仍然活着，收窄后的清单没有生效）",
            );
        assert_eq!(
            code, 1000,
            "重新配对不是撤销：必须以正常关闭（1000）发出，不能复用 4410「节点已撤销」"
        );
        // 全程不得出现撤销通知：`close_code` 把 close 帧之前的文本帧收进待取队列，因此「队列里没有
        // `node.trust.revoked`」就是「对端在关闭前没收到撤销推送」的可观察等价断言（撤销路径会先 send
        // 再 close，会话排空已入队消息后才发 close 帧，所以这条消息只能出现在这里）。
        let revoked_notifications: Vec<&Value> = client
            .pending()
            .iter()
            .filter(|message| message["type"] == json!("node.trust.revoked"))
            .collect();
        assert!(
            revoked_notifications.is_empty(),
            "重新配对不是撤销：对端不得收到 node.trust.revoked（收到就会停止重连）：{revoked_notifications:?}"
        );

        // ⑤ 同一身份重新握手：按已提交的空清单重算，catalog 为空（旧 attachment 无法在空清单下复活）。
        let mut fresh = NodeLinkClient::connect_plain(owner.addr).await;
        support::nodelink::handshake(
            &mut fresh,
            &chain.access,
            &ticket,
            &support::nodelink::nonce_text(0x73),
        )
        .await;
        fresh.step("catalog.subscribe after repair");
        fresh
            .send("catalog.subscribe", json!({ "knownRevision": null }))
            .await;
        let snapshot = fresh.expect("catalog.snapshot").await;
        assert_eq!(
            snapshot["body"]["exports"],
            json!([]),
            "空清单必须对新连接立即生效：{snapshot}"
        );
        drop(fresh);

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
                json!({ "pairingId": pairing_id, "grants": GRANTS, "exportIds": [] }),
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

// ---------------------------------------------------------------------------------------------
// AC1（tasks 4.1）：节点级事件的**组合根接线**在真实组合根 + 真实 SQLite + 真实 fake ACP 子进程上
// 落库，且会话标识为空、不被会话级投递路径误收。
//
// 为什么在这里（`node_link_e2e.rs`）：tasks 4.1 把 AC1 的证据面钉在本文件；本节补的正是本文件其余用例
// **覆盖不到**的一段——其余用例用脚本化后端（`support::owner::OwnerNode`），不启动真实 Agent 进程，
// 因此**永远不会产生** `agent.connected`；而节点级事件的唯一生产来源是真实 profile 进程的生命周期。
//
// 本节的装配点是 `app::compose::Composition` **本身**（不是测试自建的替身）：若 `compose.rs` 没有把
// `NodeEvents` 接到 `Broker::commit_node_event`（规划缺口「节点级事件的组合根接线无人认领」），
// `NodeEvents` 保持 unbound，`agent.connected` 会在 core 之前被丢弃，本节的落库断言必然失败。
//
// 本节只覆盖**节点级**事件（`agent.connected`/`agent.disconnected`）。AC1 的另外两项断言
// （`file.changed`（会话级）经 owned 路径落库并按 origin 序回放、会话标题在 `session_info_update`
// 后更新）由紧接本节的两条用例覆盖——它们用同一个组合根与同一个真实 fake ACP 子进程，只是把场景换成
// 会发类型化 Diff 与会话信息更新的 `chunked-updates`。本文件其余用例（脚本化后端）不产生这两类事件。
// ---------------------------------------------------------------------------------------------

/// fake ACP Agent 可执行文件：优先取 `ACPR_FAKE_ACP_AGENT`，否则按「测试可执行文件同 target 目录」
/// 推断（`<profile>/deps/<test>` → `<profile>/acpr-fake-acp-agent[.exe]`）。
///
/// AC1 的环境列明写「fake ACP Agent」。它由 `agent-host` 提供，`cargo test -p app` **不**会构建它，
/// 因此这里在缺失时**明确失败**（不静默跳过——跳过的测试不算通过）：先跑一次 PV1/PV2，或
/// `cargo build -p agent-host --bin acpr-fake-acp-agent`。
fn fake_acp_agent_binary() -> std::path::PathBuf {
    // 显式覆盖（CI/矩阵需要时可指定；取值必须是可执行的 fake ACP Agent）。
    if let Ok(explicit) = std::env::var("ACPR_FAKE_ACP_AGENT") {
        let path = std::path::PathBuf::from(explicit);
        assert!(
            path.is_file(),
            "ACPR_FAKE_ACP_AGENT 指向的文件不存在：{}",
            path.display()
        );
        return path;
    }
    let test_exe = std::env::current_exe().expect("测试可执行文件路径");
    let profile_dir = test_exe
        .parent()
        .and_then(std::path::Path::parent)
        .expect("测试可执行文件位于 <profile>/deps/");
    let name = if cfg!(windows) {
        "acpr-fake-acp-agent.exe"
    } else {
        "acpr-fake-acp-agent"
    };
    let path = profile_dir.join(name);
    assert!(
        path.is_file(),
        "AC1 需要 fake ACP Agent（与测试同 target 目录）：{} 不存在。\
         请先 `cargo build -p agent-host --bin acpr-fake-acp-agent`（或先跑 PV1/PV2 构建全工作区），\
         或用 ACPR_FAKE_ACP_AGENT 指定它的路径。",
        path.display()
    );
    path
}

/// 直接读**真实 SQLite** 的 `owned_event`（`(event_id, session_id 或 '<null>')`）。
///
/// AC1「可按事件库读回」最直接的证据：读的是落盘行，不经过任何 `Broker` 的内存状态。
async fn owned_event_rows(data_dir: &std::path::Path, event_type: &str) -> Vec<(String, String)> {
    let pool = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(data_dir.join("acp-remote.sqlite3"))
            .read_only(true)
            .busy_timeout(std::time::Duration::from_secs(10)),
    )
    .await
    .expect("打开真实 SQLite（只读）");
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT event_id, COALESCE(session_id, '<null>') FROM owned_event \
         WHERE event_type = ?1 ORDER BY global_sequence",
    )
    .bind(event_type)
    .fetch_all(&pool)
    .await
    .expect("查询 owned_event");
    pool.close().await;
    rows
}

/// 组合根的开发模式配置：显式进程内 keystore + 一条指向 fake ACP Agent 的 profile 种子
/// （`--scenario normal`）。
fn ac1_config(data_dir: &std::path::Path, agent_command: &str) -> app::Config {
    ac1_config_with_scenario(data_dir, agent_command, "normal")
}

/// 同上，但 profile 的 `--scenario` 由调用方给出：AC1 的 `file.changed` / 标题断言需要
/// `chunked-updates`（该场景在**同一次 prompt** 里发出带类型化 `diff` 的 `tool_call` 与带 `title` 的
/// `session_info_update`，是真实 Agent 路径上唯一能同时产生这两类事件的场景）。
fn ac1_config_with_scenario(
    data_dir: &std::path::Path,
    agent_command: &str,
    scenario: &str,
) -> app::Config {
    let command = agent_command.replace('\\', "/");
    let text = format!(
        "[daemon]\ndata_dir = {dir:?}\n[identity]\nkeystore = \"ephemeral\"\n\
         [dev_mode]\nenabled = true\n\
         [storage]\npersist_deltas = true\n\
         [[agents.profiles]]\nagent_id = \"codex\"\ndisplay_name = \"Codex\"\n\
         command = \"{command}\"\nargs = [\"--scenario\", \"{scenario}\"]\nenv_allowlist = []\ndefault = true\n",
        dir = data_dir.display(),
    );
    app::Config::from_toml(&text).expect("AC1 配置合法")
}

/// 32 字节 base64url（无填充）的 payload 摘要（`session.create` 的幂等指纹位）。
fn ac1_digest(label: &str) -> acp_core::model::Digest {
    use base64::Engine as _;
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(label.as_bytes());
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize());
    acp_core::model::Digest::new(&encoded).expect("32 字节摘要")
}

/// R30–R35（`specs/core-derived-events/spec.md` §「Agent 连接状态是节点级生命周期」）+ AC1：
/// profile 进程**建立**（真实 ACP 子进程、经组合根自己的装配）时产生一次 `agent.connected`，它经
/// `compose.rs` 的接线 → `Broker::commit_node_event` **落库**，`owned_event.session_id` 为空。
///
/// 同时断言**不被会话级投递路径误收**：组合根实际安装的 `forked_publisher` → `NodeLinkPublisher`
/// （`server::node_link::resource`）是「按会话归属投递」的入站路径，它必须丢弃会话标识为空的事件；
/// 因此扇出队列里**不出现**任何节点级事件（其余会话级事件仍会出现，见下方的正向对照）。
#[test]
fn ac1_the_composition_root_persists_node_level_events_without_session_identity() {
    use acp_core::model::{
        Actor, AgentId, AgentRef, CreateSessionRequest, Digest, ResolvedWorkspace, ResourceOrigin,
        WorkspaceAlias,
    };

    support::block_on(async {
        let root = support::TempRoot::new("ac1-node");
        let data_dir = root.join("data");
        support::create_owner_only_dir(&data_dir);
        let fake = fake_acp_agent_binary();

        // 组合根**实际安装**的发布分叉：扇出接收端留在这里，用来断言节点级事件不进入会话级投递路径。
        let (publisher, mut fanout) = app::compose::forked_publisher();
        let composition = app::compose::Composition::assemble(
            ac1_config(&data_dir, &fake.display().to_string()),
            publisher,
        )
        .await
        .expect("组合根装配");

        // 前置：接线已落地（这正是规划缺口的核心——接线缺失时这里是 false）。
        assert!(
            composition.host().node_events_bound(),
            "组合根必须已接线 NodeEvents（否则节点级事件在 core 之前被丢弃）"
        );

        // 种子导入 profile（fake ACP Agent），随后创建会话：`SessionBackendFactory::create` 会**惰性**
        // 拉起真实子进程并完成 initialize，由此产生本进程实例唯一的一次 `agent.connected`。
        assert_eq!(
            composition.seed_if_needed().await.expect("种子导入"),
            app::compose::SeedOutcome::Imported { count: 1 }
        );

        let agent =
            AgentRef::try_new(AgentId::new("codex").expect("agentId"), "Codex").expect("agent ref");
        let workspace = ResolvedWorkspace::try_new(
            WorkspaceAlias::new("project.one").expect("alias"),
            data_dir.display().to_string(),
        )
        .expect("workspace");
        let request =
            CreateSessionRequest::new(agent, Some(workspace), None, ResourceOrigin::Local);
        let request_id = composition.ids().request_id();
        let fingerprint: Digest = ac1_digest("ac1-session-create");
        let session = composition
            .use_cases()
            .create_session(&Actor::LocalCli, request_id, fingerprint, request, None)
            .await
            .expect("session.create（会拉起真实 fake ACP 子进程）");
        assert_eq!(
            session.as_str().len(),
            36,
            "会话 id 必须是规范 uuid 文本：{session}"
        );

        // 落库是组合根 runtime 上的异步提交任务，轮询等待（上限 15 s）。
        let pool = sqlx::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(data_dir.join("acp-remote.sqlite3"))
                .read_only(true)
                .busy_timeout(std::time::Duration::from_secs(10)),
        )
        .await
        .expect("打开真实 SQLite（只读）");

        // §7.3 的四组 CHECK 成对成立：节点级事件的 `session_id` 为空时，`session_sequence`/
        // `origin_epoch`/`origin_sequence` 必须**同时**为空（否则落盘行违反表级 CHECK）。一并对拍。
        type NodeRow = (
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        );
        let mut rows: Vec<NodeRow> = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while std::time::Instant::now() < deadline {
            rows = sqlx::query_as::<_, NodeRow>(
                "SELECT event_id, session_id, session_sequence, origin_epoch, origin_sequence \
                 FROM owned_event WHERE event_type = 'agent.connected' ORDER BY global_sequence",
            )
            .fetch_all(&pool)
            .await
            .expect("查询 owned_event");
            if !rows.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        assert_eq!(
            rows.len(),
            1,
            "R30：进程建立必须**恰好一次**落库 agent.connected（接线缺失时会漏掉）；实际 {} 行",
            rows.len()
        );
        let (event_id, session_id, session_sequence, origin_epoch, origin_sequence) = &rows[0];
        assert!(
            session_id.is_none(),
            "R30/R8：节点级事件的 session_id 必须为空（事件 {event_id}）；实际 {session_id:?}"
        );
        assert!(
            session_sequence.is_none(),
            "会话标识为空时 session_sequence 必须同时为空（§7.3 成对 CHECK）；实际 {session_sequence:?}"
        );
        assert!(
            origin_epoch.is_none() && origin_sequence.is_none(),
            "节点级事件不得有会话级 origin 游标（§7.3）；实际 {origin_epoch:?}/{origin_sequence:?}"
        );

        // 「不被会话级投递路径误收」：扇出队列（组合根实际安装的 `NodeLinkPublisher`）不得出现节点级事件。
        // 给提交/发布一点余量后把队列看空，收集可见的事件类型。
        let mut seen: Vec<String> = Vec::new();
        let drain_deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        while std::time::Instant::now() < drain_deadline {
            match fanout.try_recv() {
                Ok(event) => seen.push(event.event_type.as_str().to_owned()),
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
            }
        }
        // 丢弃点在 `crates/server/src/node_link/resource.rs` 的 `NodeLinkPublisher::publish`：
        // `if event.session.is_none() { return; }`——节点级事件的会话标识为空，因此**不得**进入扇出。
        // 本场景（`normal`）不产生会话级事件，队列应恰为空；若非空也不得含任何 `agent.*`。
        assert!(
            !seen.iter().any(|kind| kind.starts_with("agent.")),
            "会话级投递路径不得收到节点级事件（agent.*）；实际收到 {seen:?}"
        );
        assert!(
            seen.is_empty(),
            "会话级投递路径只承载会话级事件，本场景应为空；实际 {seen:?}"
        );

        pool.close().await;

        // 关闭序列（与 `crates/app/src/daemon.rs::close` 同序）：先停 Agent（此处产生本进程实例唯一的一次
        // `agent.disconnected`，其提交任务由组合根的 sink spawn），**再** `Composition::close`——后者会先
        // 排空这些提交任务，然后才关存储。若不先 `shutdown_all`，退出路由任务会持有 `AgentRuntime`（进而
        // 持有 `NodeEvents` → `Broker` → 存储），`close` 会以上报 `StoreStillShared` 的方式**如实失败**
        // 而不是静默跳过检查点。
        composition.host().shutdown_all().await;
        composition.close().await.expect("组合根可关闭");

        // R32：断开事件同样落库且会话标识为空（`shutdown_all` 结束进程时产生）。
        let disconnected = owned_event_rows(&data_dir, "agent.disconnected").await;
        assert_eq!(
            disconnected.len(),
            1,
            "R32：进程退出必须**恰好一次**落库 agent.disconnected；实际 {disconnected:?}"
        );
        assert_eq!(
            disconnected[0].1, "<null>",
            "R32：断开事件的 session_id 必须为空"
        );
    });
}

// ---------------------------------------------------------------------------------------------
// AC1（tasks 4.1）断言 A：会话级 `file.changed` 经 **owned 路径**（`session_id` 非空）落库，并按
// **origin 序**回放（R5–R7 / `design.md` D4/D5）。
//
// 与上一节的分工：上一节证明**节点级**事件落库且会话标识为空；本节证明**会话级**事件落库时带会话标识，
// 且按 origin 序回放（不是 UUID 序，也不是「派生后追加在批尾」）。驱动面同上：真实组合根
// `app::compose::Composition` + 真实 SQLite + 真实 fake ACP 子进程。
//
// `chunked-updates` 的一次 prompt 发出：两条 `agent_message_chunk`、一条 `agent_thought_chunk`、一条带
// **类型化 Diff**（`path: "src/main.rs"`、`oldText: "let a = 1;"`、`newText: "let a = 2;"`）的
// `tool_call`、一条 `tool_call_update`（completed，带普通内容块）、plan/commands/mode/config/
// session_info/usage/未来判别子各一条，最后 `stopReason: end_turn`。核心的派生点在组装
// `tool.call.started` 的**同一次提交**内（紧随其前），因此 `file.changed` 落在该簇中间，而不是序列末尾。
// ---------------------------------------------------------------------------------------------

/// 只读连接（每个 helper 自开自闭）。测试进程持有的组合根仍在写这个库，WAL 允许并发读。
async fn ac1_ro_pool(data_dir: &std::path::Path) -> sqlx::SqlitePool {
    sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(data_dir.join("acp-remote.sqlite3"))
            .read_only(true)
            .busy_timeout(std::time::Duration::from_secs(10)),
    )
    .await
    .expect("打开真实 SQLite（只读）")
}

/// 某会话的全部落盘事件类型（按 **origin 序**）。节点级事件（`session_id` 为空）不在其中。
async fn ac1_session_event_types(
    data_dir: &std::path::Path,
    session: &acp_core::model::SessionId,
) -> Vec<String> {
    let pool = ac1_ro_pool(data_dir).await;
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT event_type FROM owned_event WHERE session_id = ?1 ORDER BY origin_sequence ASC",
    )
    .bind(session.as_str())
    .fetch_all(&pool)
    .await
    .expect("查询 owned_event");
    pool.close().await;
    rows
}

/// 组合根 + 真实 fake ACP 子进程驱动**一次** owned 会话的一轮 prompt，返回会话 id。
///
/// `submit_command` 只把 turn 排进队列并派发；后端事件异步写进 sink 的缓冲，必须由持有 runtime 的组合根
/// 周期调 `Broker::pump` 才落盘（与 `crates/app/src/daemon.rs` 的合并窗口是同一件事）。因此这里轮询
/// `pump` 直到该轮的 `turn.completed` 落库——它的落库保证同一轮更早的事件（含 `file.changed` 与
/// `session.info.changed`）已经在更早的批次中提交。
async fn ac1_prompt_once(
    composition: &app::compose::Composition,
    data_dir: &std::path::Path,
    label: &str,
) -> acp_core::model::SessionId {
    use acp_core::model::{
        Actor, AgentId, AgentRef, ClientCommand, CommandKind, CommandPayload, CreateSessionRequest,
        PromptContentBlock, ResolvedWorkspace, ResourceOrigin, ViewJson, WorkspaceAlias,
    };

    let agent =
        AgentRef::try_new(AgentId::new("codex").expect("agentId"), "Codex").expect("agent ref");
    let workspace = ResolvedWorkspace::try_new(
        WorkspaceAlias::new("project.one").expect("alias"),
        data_dir.display().to_string(),
    )
    .expect("workspace");
    let request = CreateSessionRequest::new(agent, Some(workspace), None, ResourceOrigin::Local);
    let session = composition
        .use_cases()
        .create_session(
            &Actor::LocalCli,
            composition.ids().request_id(),
            ac1_digest(&format!("ac1-session-{label}")),
            request,
            None,
        )
        .await
        .expect("session.create（会惰性拉起真实 fake ACP 子进程）");

    let prompt = ClientCommand {
        actor: Actor::LocalCli,
        request: composition.ids().request_id(),
        command: "session.prompt".to_owned(),
        kind: CommandKind::Mutation,
        session: Some(session.clone()),
        expected_version: None,
        request_fingerprint: ac1_digest(&format!("ac1-prompt-{label}")),
        payload: CommandPayload::Prompt {
            content: vec![PromptContentBlock::new(
                ViewJson::new(r#"{"type":"text","text":"请编辑 src/main.rs"}"#).expect("view"),
            )],
        },
    };
    let receipt = composition
        .use_cases()
        .submit_command(&Actor::LocalCli, prompt)
        .await
        .expect("session.prompt 提交");
    assert!(
        matches!(
            receipt,
            acp_core::model::CommandReceipt::Accepted { turn: Some(_), .. }
        ),
        "prompt 必须被接受且带 turn：{receipt:?}"
    );

    // 驱动合并窗口直到本轮的终态事件落库（上限 20 s）。
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        composition
            .broker()
            .pump(&session)
            .await
            .expect("驱动合并窗口");
        let types = ac1_session_event_types(data_dir, &session).await;
        if types.iter().any(|kind| kind == "turn.completed") {
            return session;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "超时：turn.completed 未落库（{label}）；实际事件 {types:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// 组合根的开发模式配置（含 `storage.persist_deltas = true`）+ 指向 fake ACP Agent 的指定场景
/// profile 种子（两条新断言共用）。
///
/// `persist_deltas = true`：默认 `false` 会让 turn 终态后压缩增量（把 delta 行标记 `compacted_into`，
/// 由一条 `turn.delta_compacted` 汇总替代），而压缩行不进 origin 切片（`compacted_into IS NULL`）——
/// 那会让本节的「origin 序恰为 `1..=N`」断言被压缩断层破坏。开 `persist_deltas` 后每轮的 origin 序列
/// 连续，这正是「按 origin 序回放」要观察的对象。
async fn ac1_composition_with_scenario(
    data_dir: &std::path::Path,
    scenario: &str,
) -> (
    app::compose::Composition,
    tokio::sync::mpsc::Receiver<acp_core::model::CommittedEvent>,
) {
    let fake = fake_acp_agent_binary();
    let (publisher, fanout) = app::compose::forked_publisher();
    let composition = app::compose::Composition::assemble(
        ac1_config_with_scenario(data_dir, &fake.display().to_string(), scenario),
        publisher,
    )
    .await
    .expect("组合根装配");
    assert_eq!(
        composition.seed_if_needed().await.expect("种子导入"),
        app::compose::SeedOutcome::Imported { count: 1 }
    );
    // 扇出接收端交回调用方持有：消费者缺席时队列满会丢弃（`EVENT_QUEUE_CAPACITY`），本节只断言落库，
    // 但保有接收端才是组合根在生产里的形态（`daemon` 把接收端交给 `ResourceRoute`）。
    (composition, fanout)
}

/// R5–R7：会话级 `file.changed` 经 owned 路径落库（`session_id` 非空），且**按 origin 序回放**。
///
/// 两个会话各驱动一次 prompt，因此库里恰有**两条** `file.changed`；会话 A 上另断言完整的 origin 序与
/// 游标续读（否则「顺序」无从验证）。可证伪点：把 `Broker::commit_chunk` 的派生块或存储的
/// `origin_sequence` 分配撤掉，本节的前置（`file.changed` 出现 + 序列恰为 `1..=N`）必然失败。
#[test]
fn ac1_the_composition_root_persists_file_changes_on_the_owned_path_in_origin_order() {
    use acp_core::model::OriginCursor;
    use acp_core::ports::ReplayLimit;

    support::block_on(async {
        let root = support::TempRoot::new("ac1-file");
        let data_dir = root.join("data");
        support::create_owner_only_dir(&data_dir);

        let (composition, _fanout) =
            ac1_composition_with_scenario(&data_dir, "chunked-updates").await;
        let first = ac1_prompt_once(&composition, &data_dir, "file-a").await;
        let second = ac1_prompt_once(&composition, &data_dir, "file-b").await;

        // ① 「落库」：库中恰有两条 `file.changed`，分属两个会话且 `session_id` **非空**——这正是它与会话
        //    标识为空、不走会话级投递路径的节点级事件的本质区别。
        let pool = ac1_ro_pool(&data_dir).await;
        type FileChangedRow = (Option<String>, Option<i64>, Option<i64>, String);
        let rows: Vec<FileChangedRow> = sqlx::query_as(
            "SELECT session_id, session_sequence, origin_sequence, payload_json FROM owned_event \
             WHERE event_type = 'file.changed' ORDER BY global_sequence",
        )
        .fetch_all(&pool)
        .await
        .expect("查询 file.changed");
        assert_eq!(
            rows.len(),
            2,
            "两个会话各派生恰好一条 file.changed；实际 {} 行",
            rows.len()
        );
        let mut owners: Vec<&str> = rows
            .iter()
            .map(|(session, _, _, _)| session.as_deref().expect("owned 路径：session_id 非空"))
            .collect();
        owners.sort_unstable();
        let mut expected = vec![first.as_str(), second.as_str()];
        expected.sort_unstable();
        assert_eq!(
            owners, expected,
            "两条 file.changed 必须分别属于两个会话（owned 路径）"
        );

        // 会话 A 的那条：会话标识与两套会话级游标都在（node 级事件三者皆空）。
        let a_row = rows
            .iter()
            .find(|(session, _, _, _)| session.as_deref() == Some(first.as_str()))
            .expect("会话 A 的 file.changed");
        assert!(
            a_row.1.is_some() && a_row.2.is_some(),
            "会话级 file.changed 必须带 session_sequence 与 origin_sequence：{a_row:?}"
        );
        // 视图字段（R5/R6/R7）：类型化 Diff 派生、行级 Myers 结果、工作区内相对路径。
        let view: Value = serde_json::from_str(&a_row.3).expect("file.changed view 是 JSON");
        assert_eq!(view["kind"], json!("modified"), "{view}");
        assert_eq!(view["displayPath"], json!("src/main.rs"), "{view}");
        assert_eq!(view["addedLines"], json!("1"), "{view}");
        assert_eq!(view["deletedLines"], json!("1"), "{view}");
        assert!(
            view.get("outsideWorkspace").is_none(),
            "工作区内的路径不得带 outsideWorkspace：{view}"
        );

        // ② 「按 origin 序回放」：经真实读视图的 origin 切片（`ORDER BY origin_sequence`）取回事件。
        let read = composition.broker().read_view().await.expect("读视图");
        let slice = read
            .node_link_slice(&first, None, ReplayLimit::new(200))
            .await
            .expect("会话 A 的 origin 切片");
        let ordered: Vec<(String, u64)> = slice
            .events
            .iter()
            .map(|record| {
                (
                    record.event.event_type.as_str().to_owned(),
                    record
                        .event
                        .origin_sequence
                        .expect("会话事件必有 origin 序列")
                        .get(),
                )
            })
            .collect();
        let seqs: Vec<u64> = ordered.iter().map(|(_, seq)| *seq).collect();
        assert!(seqs.len() >= 4, "混合序列不足以验证顺序：{ordered:?}");
        assert_eq!(
            seqs,
            (1..=seqs.len() as u64).collect::<Vec<_>>(),
            "回放必须按 origin 序：序列恰为 1..=N 且严格递增；实际 {seqs:?}"
        );

        let position = ordered
            .iter()
            .position(|(kind, _)| kind == "file.changed")
            .expect("会话 A 的 origin 序列必须含 file.changed");
        let change_seq = ordered[position].1;
        assert!(
            position > 0 && position + 1 < ordered.len(),
            "file.changed 必须落在序列内部（不是派生后追加在批尾）：{ordered:?}"
        );
        assert!(
            [position - 1, position + 1]
                .iter()
                .any(|index| ordered[*index].0.starts_with("tool.call.")),
            "file.changed 必须紧邻产生它的工具调用事件（即时派生）：{ordered:?}"
        );

        // ③ 游标续读按 origin 定位到 file.changed 本身（`origin_sequence > after`）。
        let before = read
            .node_link_slice(
                &first,
                Some(OriginCursor {
                    origin_epoch: slice.head.origin_epoch.clone(),
                    origin_sequence: acp_core::model::Sequence::new(change_seq - 1)
                        .expect("sequence"),
                }),
                ReplayLimit::new(200),
            )
            .await
            .expect("增量回放（file.changed 之前一条）");
        assert_eq!(
            before
                .events
                .first()
                .map(|record| record.event.event_type.as_str()),
            Some("file.changed"),
            "after = file.changed 的前一条时，回放的第一条必须就是它（按 origin 序定位）"
        );
        let after = read
            .node_link_slice(
                &first,
                Some(OriginCursor {
                    origin_epoch: slice.head.origin_epoch.clone(),
                    origin_sequence: acp_core::model::Sequence::new(change_seq).expect("sequence"),
                }),
                ReplayLimit::new(200),
            )
            .await
            .expect("增量回放（file.changed 之后）");
        assert!(
            after
                .events
                .iter()
                .all(|record| record.event.event_type.as_str() != "file.changed"),
            "file.changed 之后的重放不得再出现它"
        );
        assert_eq!(
            after.events.len(),
            ordered.len() - change_seq as usize,
            "游标之后的条数必须是 N - seq"
        );
        pool.close().await;

        composition.host().shutdown_all().await;
        composition.close().await.expect("组合根可关闭");
    });
}

// ---------------------------------------------------------------------------------------------
// AC1（tasks 4.1）断言 B：会话标题在 `session_info_update` 后更新（R9 / `design.md` D7）。
//
// 三件事：① 通知把标题**单向**写入会话（创建时为空 → 通知后为通知取值，会话摘要随之反映）；
// ② `session.info.changed` 事件落库且带会话标识；③ 会话的**权威**更新时间取 Daemon 持久化时间，
// 不是 Agent 自报的 `updatedAt`。
//
// 未覆盖的三态之一：fake ACP Agent 只在 `chunked-updates` 里发 `session_info_update`，且恒带非空
// `title`，因此「通知显式置空」与「只带 updatedAt」两条路径无法经真实子进程驱动；它们由 WP3 的
// `crates/storage-sqlite/tests/title_write.rs` 与 `crates/core/src/broker.rs` 的 R9 行为测试覆盖
// （本任务边界不允许改 `crates/agent-host/**` 去加场景）。
// ---------------------------------------------------------------------------------------------

/// R9：会话标题在 `session_info_update` 后更新，`updatedAt` 取 Daemon 持久化时间。
#[test]
fn ac1_the_composition_root_updates_the_session_title_from_the_agent_notification() {
    use acp_core::model::Actor;
    use acp_core::ports::SessionQuery;

    support::block_on(async {
        let root = support::TempRoot::new("ac1-title");
        let data_dir = root.join("data");
        support::create_owner_only_dir(&data_dir);

        let (composition, _fanout) =
            ac1_composition_with_scenario(&data_dir, "chunked-updates").await;

        // 驱动一次 prompt（fake 会在其中发出带 `title: "会话标题"` 的 `session_info_update`）。
        let session = ac1_prompt_once(&composition, &data_dir, "title").await;

        let pool = ac1_ro_pool(&data_dir).await;
        let (title, version, created_at, updated_at): (Option<String>, i64, String, String) =
            sqlx::query_as(
                "SELECT title, version, created_at, updated_at FROM owned_session WHERE session_id = ?1",
            )
            .bind(session.as_str())
            .fetch_one(&pool)
            .await
            .expect("查询 owned_session");

        // ① 标题已被通知写入（创建时为空——由 `create_session` 恒写 `title: None` 决定）。
        assert_eq!(
            title.as_deref(),
            Some("会话标题"),
            "R9：标题必须来自 Agent 的 session_info_update"
        );
        assert!(
            version > 1,
            "标题写入走通用状态变更路径，会话版本必须递增：version={version}"
        );

        // ② 投影出 `session.info.changed` 事件，且它带会话标识（会话级事件）。
        let events: Vec<(Option<String>, String)> = sqlx::query_as(
            "SELECT session_id, payload_json FROM owned_event \
             WHERE session_id = ?1 AND event_type = 'session.info.changed' \
             ORDER BY origin_sequence",
        )
        .bind(session.as_str())
        .fetch_all(&pool)
        .await
        .expect("查询 session.info.changed");
        assert_eq!(
            events.len(),
            1,
            "R9：一次通知投影恰好一条 session.info.changed；实际 {} 行",
            events.len()
        );
        assert!(
            events[0].0.as_deref() == Some(session.as_str()),
            "session.info.changed 是会话级事件，必须带 session_id"
        );
        let event_view: Value =
            serde_json::from_str(&events[0].1).expect("session.info.changed view 是 JSON");
        assert_eq!(event_view["title"], json!("会话标题"), "{event_view}");
        // 事件视图**转发** Agent 自报的 `updatedAt`（合同字段），与会话的权威更新时间是两个来源。
        assert_eq!(
            event_view["updatedAt"],
            json!("2026-09-24T10:00:00.000Z"),
            "{event_view}"
        );

        // ③ 会话的权威更新时间取 Daemon 持久化时间，**不是** Agent 自报值；且单调不早于创建时间。
        assert_ne!(
            updated_at, "2026-09-24T10:00:00.000Z",
            "R9/D7：会话更新时间不得采用 Agent 自报的 updatedAt"
        );
        assert!(
            updated_at.as_str() >= created_at.as_str(),
            "更新时间必须不早于创建时间：{created_at} → {updated_at}"
        );
        pool.close().await;

        // ④ 会话摘要随之反映该标题（目录页的读面）。
        let summaries = composition
            .use_cases()
            .list_sessions(
                &Actor::LocalCli,
                SessionQuery {
                    only: Some(vec![session.clone()]),
                    states: Vec::new(),
                    limit: None,
                },
            )
            .await
            .expect("session.list");
        assert_eq!(summaries.len(), 1, "只应取回本次会话");
        assert_eq!(
            summaries[0].title(),
            Some("会话标题"),
            "会话摘要必须反映被更新的标题"
        );

        composition.host().shutdown_all().await;
        composition.close().await.expect("组合根可关闭");
    });
}
