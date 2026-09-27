//! R9 的组合断言：`node.pair.confirm` 的无效 `exportIds` 在**真实 router + 真实 SQLite** 上的错误码
//! 与零写入（不创建信任记录、不推进配对状态、不写审计）。
//!
//! 为什么放在 `app`：清单校验发生在存储层的落定事务里（`storage-sqlite` 的 `settle_pairing`），而
//! `server` 的 local_admin 用例只接得上 fake `TrustStore`——`server` 不允许依赖 `storage-sqlite`
//! （`docs/MODULE_ARCHITECTURE.md` §5 的依赖矩阵，`scripts/check-crate-boundaries.mjs` 断言）。组合根
//! 这一层同时有真 router 与真存储（`support::owner::OwnerNode`），因此「错误分类正确 + 失败零写入」
//! 的组合断言只能在这里做；`storage-sqlite` 的 `admin_store.rs` 覆盖同一事务的存储侧细节。
//!
//! 两个用例对应 `local-admin-methods`（`node-trust-export-ids`）里 `node.pair.confirm` 的两条失败分支：
//!
//! - 本机不存在的 `exportId` → `local.not_found`；
//! - `exportId` 的 `scopes` 与本次 `grants` 不相交 → `local.invalid_params`。
//!
//! 三条失败判据的可观察面：「不创建信任」读 `node.list`、「不写审计」读 `audit.export` 落下的真实文件
//! （`owned_audit` 的记录）、「不推进配对状态」用「同一份配对随后仍能成功确认」观察。

mod support;

use serde_json::{Value, json};
use server::local_admin::{AdminOutcome, AdminResponse, Method};
use support::nodelink::{self, AccessKey};
use support::owner::OwnerNode;

/// 本用例扮演的 Access Node 标识。
const ACCESS_NODE: &str = "9c4ff2a0-2b1e-4d3a-8f4d-6b6a1d0e5c31";
/// 本次确认授予的 grants（也是可见 Export 的 `scopes` 取值）。
const GRANTS: [&str; 3] = ["grant.observe", "grant.interact", "grant.remote-work"];
/// 可见的 Export：`scopes` 与 `GRANTS` 相交。
const EXPORT_VISIBLE: &str = "export.visible";
/// `scopes` 与 `GRANTS` 不相交的 Export（确认它必须被参数校验拒绝）。
const EXPORT_DISJOINT: &str = "export.disjoint";
/// 注册的 workspace alias（`session.create` 只带别名，Export 也必须挂在一个别名上）。
const WORKSPACE_ALIAS: &str = "project.one";
/// Agent selector。
const AGENT: &str = "codex";

/// 一个已认领、尚未本地确认的节点配对（Owner 用真实存储 + 真实 router，经真实 loopback listener
/// 完成 Pairing claim）。
struct Claimed {
    owner: OwnerNode,
    pairing_id: String,
}

impl Claimed {
    /// ① 本机注册（workspace + agent profile + 两条 Export）→ ② `node.pair.begin` → ③ Access 侧 claim。
    async fn up_to_claim(label: &str) -> Self {
        let owner = OwnerNode::start(label).await;
        let access = AccessKey::new(ACCESS_NODE);
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
            (EXPORT_DISJOINT, vec!["grant.approve"]),
        ] {
            owner
                .admin(
                    Method::ExportCreate,
                    json!({
                        "exportId": export_id,
                        "displayName": format!("Export {export_id}"),
                        "agentIds": [AGENT],
                        "workspaceAliases": [{ "alias": WORKSPACE_ALIAS, "displayName": "Project One" }],
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
        let ticket = nodelink::ticket_from_url(pairing_url, &pairing_id);

        let client_nonce = nodelink::nonce_text(0x41);
        let claim = nodelink::claim_request(&ticket, &access, &client_nonce);
        let reply = nodelink::post_json(
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
        nodelink::verify_owner_proof(&ticket, &body, &access, &client_nonce);

        Self { owner, pairing_id }
    }

    /// `node.pair.confirm` 的原始响应（失败路径也用它）。
    async fn confirm(&self, export_ids: Value) -> AdminResponse {
        self.owner
            .call(Method::NodePairConfirm, self.confirm_params(export_ids))
            .await
    }

    /// 同上，但要求成功（失败即带方法与错误码 panic）。
    async fn confirm_ok(&self, export_ids: Value) -> Value {
        self.owner
            .admin(Method::NodePairConfirm, self.confirm_params(export_ids))
            .await
    }

    fn confirm_params(&self, export_ids: Value) -> Value {
        json!({
            "pairingId": self.pairing_id,
            "grants": GRANTS,
            "exportIds": export_ids,
        })
    }

    /// `node.list` 的 `nodes` 数组（信任记录的可观察面）。
    async fn listed_nodes(&self) -> Value {
        self.owner.admin(Method::NodeList, json!({})).await["nodes"].clone()
    }

    /// `audit.export` 里某个审计类别的真实记录数（读的是 SQLite 写出的文件，不是内存计数）。
    async fn audit_count(&self, label: &str, action: &str) -> u64 {
        let path = self.owner.data_dir().join(format!("audit-{label}.jsonl"));
        let result = self
            .owner
            .admin(
                Method::AuditExport,
                json!({
                    "outputPath": path.display().to_string(),
                    "format": "jsonl",
                    "since": null,
                    "until": null,
                    "categories": [action],
                }),
            )
            .await;
        result["recordCount"].as_u64().expect("recordCount")
    }
}

/// 失败响应的 `(local.* 错误码, 消息)`；成功即 panic。
///
/// 消息也要看：同一个错误码可能来自多个判定点，只有消息能证明失败恰好出自目标规则（错误消息里不出现
/// 对端给的自由文本，只出现方法名、实体类别与适配器自己的原因）。
fn failure_of(response: &AdminResponse) -> (&'static str, String) {
    match response.outcome() {
        AdminOutcome::Failure { error } => (error.code().as_str(), error.message().to_owned()),
        AdminOutcome::Success { .. } => panic!("期望失败，实际成功"),
    }
}

/// 本机不存在的 `exportId`：`local.not_found`，且三类零写入都成立。
#[test]
fn node_pair_confirm_rejects_an_unknown_export_id_without_writing() {
    support::block_on(async {
        let claimed = Claimed::up_to_claim("node-pair-export-unknown").await;

        // ① 错误分类：存储层的 `NotFound(Export)` 原样映到 `local.not_found`（清单里合法与非法 id 混在一起
        //    时也必须整体失败，不能只保留合法项）。
        let failure = claimed
            .confirm(json!([EXPORT_VISIBLE, "export.never-created"]))
            .await;
        let (code, message) = failure_of(&failure);
        assert_eq!(code, "local.not_found");
        assert!(
            message.contains("export.never-created"),
            "失败必须具体指向那个不存在的 id（而不是别的判定点）：{message}"
        );

        // ② 零写入：不建信任记录、不写审计。
        assert_eq!(
            claimed.listed_nodes().await,
            json!([]),
            "失败路径不得创建信任记录"
        );
        assert_eq!(
            claimed.audit_count("unknown", "node.paired").await,
            0,
            "失败路径不得写审计"
        );

        // ③ 配对状态未被推进：同一份配对随后仍能成功确认，且清单落在信任记录上。
        let confirmed = claimed.confirm_ok(json!([EXPORT_VISIBLE])).await;
        assert_eq!(confirmed["nodeId"], json!(ACCESS_NODE));
        assert_eq!(confirmed["exportIds"], json!([EXPORT_VISIBLE]));
        let listed = claimed.listed_nodes().await;
        assert_eq!(listed.as_array().expect("数组").len(), 1);
        assert_eq!(listed[0]["exportIds"], json!([EXPORT_VISIBLE]));

        // ④ 成功路径确实写了一条 `node.paired`：证明 ② 的 0 是判别性断言，而不是过滤器把记录全滤掉了。
        assert_eq!(
            claimed
                .audit_count("unknown-after-approval", "node.paired")
                .await,
            1
        );

        claimed.owner.stop().await;
    });
}

/// 与本次 `grants` 不相交的 `exportId`：`local.invalid_params`，且不建信任、不推进配对状态。
#[test]
fn node_pair_confirm_rejects_an_export_id_disjoint_from_the_grants() {
    support::block_on(async {
        let claimed = Claimed::up_to_claim("node-pair-export-disjoint").await;

        // `EXPORT_DISJOINT` 的三个条件里只有「`scopes ∩ grants ≠ ∅`」不成立（存在、未撤销）。
        let failure = claimed.confirm(json!([EXPORT_DISJOINT])).await;
        let (code, message) = failure_of(&failure);
        assert_eq!(code, "local.invalid_params");
        assert!(
            message.contains("must intersect the granted grants"),
            "失败必须出自「清单与本次 grants 不相交」这条规则：{message}"
        );

        assert_eq!(
            claimed.listed_nodes().await,
            json!([]),
            "失败路径不得创建信任记录"
        );
        assert_eq!(
            claimed.audit_count("disjoint", "node.paired").await,
            0,
            "失败路径不得写审计"
        );

        // 配对没有被推进到终态：换成清单内的 Export 仍能确认。
        let confirmed = claimed.confirm_ok(json!([EXPORT_VISIBLE])).await;
        assert_eq!(confirmed["exportIds"], json!([EXPORT_VISIBLE]));

        claimed.owner.stop().await;
    });
}
