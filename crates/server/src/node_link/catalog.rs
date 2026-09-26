//! Node Link 的 catalog 投影与 `catalog.subscribe` 处理（`design.md` D4/D14、`NODE_LINK_PROTOCOL.md` §12.3）。
//!
//! 三条边界：
//!
//! - **实时投影，不做缓存副本**：每次 `catalog.subscribe` 都按当次持久化记录重新投影（core 的
//!   `node_link_catalog_view`），因此撤销不需要等任何推送就立即生效（D7 的推送只负责「立即通知」）；
//! - **可见性只有一个判定点**：[`visible_exports`] 是「哪些 Export 对该 Access 可见」的**唯一**实现
//!   （D14 的用户裁决：未撤销且 `export.scopes ∩ 该节点信任记录 grants ≠ ∅`；`exportIds` 独立维度
//!   属后续阶段待办）。撤销也在这里生效——已撤销的 Export 不进 catalog；
//! - **零参数 template 是发布前置**：首切片发布的 template 必须是 `params = []`（§12.3）。带参数的
//!   Export 无法在本切片内忠实发布，因此整个投影**失败关闭**（回 `internal.unavailable` 并记日志），
//!   既不静默丢弃该 Export，也不发布一个违反 MUST 的条目。
//!
//! 不读时钟、不查 SQLite、不调用平级 adapter：全部持久事实经 `core::use_cases` 取得。

use std::sync::Arc;

use acp_core::model::{ExportRecord, NodeRecord};
use acp_core::use_cases::NodeLinkCatalogView;
use node_link_protocol::catalog::{CatalogSnapshot, CatalogSubscribe, ExportEntryList};
use node_link_protocol::common::{
    ExportAgent, ExportEntry, ExportId, GrantList, GrantName, NoContentCache, NonEmptyText,
    OpaqueRef, WorkspaceAlias, WorkspaceAliasEntry, WorkspaceTemplate,
};
use node_link_protocol::envelope::{Envelope, MessageType};
use node_link_protocol::error::ErrorCode;
use serde::de::DeserializeOwned;
use std::str::FromStr as _;
use tracing::{debug, warn};

use crate::node_link::conn::{ConnectionHandle, MessageRoute, RouteOutcome, SendFault};
use crate::node_link::link_error;

/// `catalog.subscribe` 的处理件（无连接级状态：投影每次现算）。
pub struct CatalogRoute {
    core: Arc<acp_core::use_cases::UseCases>,
}

impl CatalogRoute {
    /// 装配。
    pub fn new(core: Arc<acp_core::use_cases::UseCases>) -> Self {
        Self { core }
    }
}

#[async_trait::async_trait]
impl MessageRoute for CatalogRoute {
    async fn route(&self, session: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
        if message.message_type() != MessageType::CatalogSubscribe {
            return RouteOutcome::Unclaimed;
        }
        let Some(request) = decode::<CatalogSubscribe>(message) else {
            return self
                .reject(session, ErrorCode::ProtocolSchemaInvalid, message)
                .await;
        };
        // `knownRevision` 非空时同样回完整快照（`catalog.changed` 属 post_mvp），因此本层只记录它，
        // 不据此裁剪投影。
        debug!(
            event = "node_link.catalog_subscribe",
            access_node_id = session.node_id().as_str(),
            known_revision = request.known_revision.as_ref().map(|value| value.as_str()),
            "the access node subscribed to the catalog"
        );
        let view = match self.core.node_link_catalog_view(session.node_id()).await {
            Ok(view) => view,
            Err(error) => {
                warn!(
                    event = "node_link.catalog_view_failed",
                    access_node_id = session.node_id().as_str(),
                    error = ?error,
                    "the catalog projection source could not be read"
                );
                return self
                    .fail(session, ErrorCode::InternalUnavailable, message)
                    .await;
            }
        };
        let entries = match project(&view) {
            Ok(entries) => entries,
            Err(fault) => {
                warn!(
                    event = "node_link.catalog_projection_failed",
                    access_node_id = session.node_id().as_str(),
                    fault = fault.as_str(),
                    "the catalog projection cannot be published as-is"
                );
                return self
                    .fail(session, ErrorCode::InternalUnavailable, message)
                    .await;
            }
        };
        self.send_batches(session, &view, entries).await;
        RouteOutcome::Claimed
    }
}

impl CatalogRoute {
    /// 按协商批次大小分批发送（`catalogSnapshotBatchSize`；schema 的 `maxItems` 也是 500，限额只会更小）。
    ///
    /// 慢连接（高水位）时**停止**继续发送：调用方不能被一个慢连接拖住，剩余批次留给重连后的
    /// 重新订阅（§2.5：先停读快照批次，持续过慢由会话的慢消费者看门狗断开）。
    async fn send_batches(
        &self,
        session: &ConnectionHandle,
        view: &NodeLinkCatalogView,
        entries: Vec<ExportEntry>,
    ) {
        let revision =
            match node_link_protocol::common::DecimalString::parse(&view.revision.to_string()) {
                Ok(revision) => revision,
                Err(_) => {
                    warn!(
                        event = "node_link.catalog_revision_invalid",
                        access_node_id = session.node_id().as_str(),
                        "the catalog revision is not a decimal string"
                    );
                    return;
                }
            };
        let batch_size = session.limits().catalog_snapshot_batch_size() as usize;
        let batch_size = batch_size.max(1);
        if entries.is_empty() {
            // 没有可见 Export 时仍回一条空快照：对端需要知道「当前视图为空」，而不是等一个永不到来的批次。
            if let Err(fault) = session.send(
                MessageType::CatalogSnapshot,
                &CatalogSnapshot {
                    revision,
                    exports: ExportEntryList::new(Vec::new()).expect("空批次合法"),
                },
            ) {
                self.report_send_fault(session, fault);
            }
            return;
        }
        let total = entries.len();
        for (index, batch) in entries.chunks(batch_size).enumerate() {
            if index > 0 && session.saturated() {
                debug!(
                    event = "node_link.catalog_batch_paused",
                    access_node_id = session.node_id().as_str(),
                    pending_messages = session.pending().messages,
                    "the connection is above the queue high water mark; the remaining batches are deferred"
                );
                return;
            }
            let body = CatalogSnapshot {
                revision: revision.clone(),
                exports: match ExportEntryList::new(batch.to_vec()) {
                    Ok(list) => list,
                    Err(_) => {
                        warn!(
                            event = "node_link.catalog_batch_invalid",
                            access_node_id = session.node_id().as_str(),
                            "a catalog batch exceeds the schema item limit"
                        );
                        return;
                    }
                },
            };
            if let Err(fault) = session.send(MessageType::CatalogSnapshot, &body) {
                self.report_send_fault(session, fault);
                return;
            }
        }
        debug!(
            event = "node_link.catalog_snapshot_sent",
            access_node_id = session.node_id().as_str(),
            revision = view.revision,
            exports = total,
            "the catalog snapshot was queued"
        );
    }

    /// 投递失败的处置：高水位由会话的慢消费者看门狗负责，连接已结束则无需再做任何事。
    fn report_send_fault(&self, session: &ConnectionHandle, fault: SendFault) {
        match fault {
            SendFault::HighWater | SendFault::Closed => debug!(
                event = "node_link.catalog_not_delivered",
                access_node_id = session.node_id().as_str(),
                fault = ?fault,
                "the catalog snapshot could not be queued"
            ),
            SendFault::Encoding => warn!(
                event = "node_link.catalog_encoding_failed",
                access_node_id = session.node_id().as_str(),
                "the catalog snapshot could not be encoded"
            ),
        }
    }

    /// 消息级拒绝（连接保持可用）。
    async fn reject(
        &self,
        session: &ConnectionHandle,
        code: ErrorCode,
        message: &Envelope,
    ) -> RouteOutcome {
        self.send_error(
            session,
            code,
            "the catalog request does not match the v1 schema",
            message,
        );
        RouteOutcome::Claimed
    }

    /// 内部故障：显式回 `link.error`（不静默），连接保持可用。
    async fn fail(
        &self,
        session: &ConnectionHandle,
        code: ErrorCode,
        message: &Envelope,
    ) -> RouteOutcome {
        self.send_error(
            session,
            code,
            "the owner node cannot serve the catalog",
            message,
        );
        RouteOutcome::Claimed
    }

    fn send_error(
        &self,
        session: &ConnectionHandle,
        code: ErrorCode,
        message: &'static str,
        envelope: &Envelope,
    ) {
        let Some(body) = link_error(code, message, Some(envelope.message_id())) else {
            warn!(
                event = "node_link.error_body_failed",
                code = code.as_str(),
                "the link.error body cannot be assembled"
            );
            return;
        };
        debug!(
            event = "node_link.catalog_rejected",
            code = code.as_str(),
            "the catalog request was rejected without closing the connection"
        );
        let _ = session.send(MessageType::LinkError, &body);
    }
}

/// 可见性策略（`design.md` D14 的用户裁决，唯一的过滤点）：未撤销且
/// `export.scopes ∩ 该节点信任记录 grants ≠ ∅`。
///
/// 结果按 `exportId` 升序（批次内顺序稳定，同一份投影在多次订阅里逐条一致）；节点信任行缺失或未配对时
/// 可见集为空（连接层已保证认证，这里只是失败关闭）。
pub(crate) fn visible_exports<'a>(
    node: Option<&NodeRecord>,
    exports: &'a [ExportRecord],
) -> Vec<&'a ExportRecord> {
    let Some(node) = node.filter(|row| row.state() == acp_core::model::NodeState::Paired) else {
        return Vec::new();
    };
    let mut visible: Vec<&ExportRecord> = exports
        .iter()
        .filter(|export| !export.is_revoked())
        .filter(|export| intersects(export, node))
        .collect();
    visible.sort_by(|left, right| left.export_id().as_str().cmp(right.export_id().as_str()));
    visible
}

/// `export.scopes ∩ node.grants` 是否非空。
fn intersects(export: &ExportRecord, node: &NodeRecord) -> bool {
    export
        .scopes()
        .iter()
        .any(|scope| node.grants().contains(scope))
}

/// 单个 Export 是否对该 Access 节点可见（[`visible_exports`] 的单项形式，供 `resource.attach` 复用）。
///
/// 可见性只有一个判定点：这里的实现直接复用 [`visible_exports`]，因此 catalog 与 resource 不可能
/// 对同一个 Export 给出不同结论。未知/未配对的节点、已撤销的 Export、与 grants 不相交的 Export
/// 都返回 `Ok(false)`；`Err` 只表示读视图失败（内部故障，调用方按 `internal.unavailable` 处置）。
pub(crate) async fn export_is_visible(
    core: &acp_core::use_cases::UseCases,
    node: &acp_core::model::NodeId,
    export: &acp_core::model::ExportId,
) -> Result<bool, acp_core::model::PortError> {
    let view = core.node_link_catalog_view(node).await?;
    Ok(visible_exports(view.node.as_ref(), &view.exports)
        .iter()
        .any(|record| record.export_id() == export))
}

/// 投影失败的原因（只用于结构化日志；对端一律看到 `internal.unavailable`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectionFault {
    /// 某条导出记录的字段超出了 wire 的取值域（本机数据与协议不一致）。
    Field,
    /// 某个 template 带参数：首切片只能发布 `params = []`（§12.3）。
    ParametricTemplate,
}

impl ProjectionFault {
    fn as_str(self) -> &'static str {
        match self {
            Self::Field => "field_out_of_range",
            Self::ParametricTemplate => "parametric_template",
        }
    }
}

/// 把 core 的 Export 记录投影成 wire 条目（全字段，含首切片约束校验）。
///
/// 两处**派生的不透明引用**（`NODE_LINK_PROTOCOL.md` §12.3 只要求形状，内容由 Owner 定义：
/// `LOCAL_ADMIN_PROTOCOL.md` §5.5 明说它们「不出现在本地方法里」而由 Owner 在发布时填充）：
/// `agents[].capabilitiesRef` 取 `agentId`、`capabilityCeilingRef` 取 `exportId`。两者都是稳定、
/// 对端只能原样回传的不透明串；本切片没有别的语义来源，因此不编造（见 WP5 handoff 的 G3 结论）。
fn project(view: &NodeLinkCatalogView) -> Result<Vec<ExportEntry>, ProjectionFault> {
    let mut entries = Vec::new();
    for export in visible_exports(view.node.as_ref(), &view.exports) {
        entries.push(project_entry(view, export)?);
    }
    Ok(entries)
}

fn project_entry(
    view: &NodeLinkCatalogView,
    export: &ExportRecord,
) -> Result<ExportEntry, ProjectionFault> {
    let agents = export
        .agent_ids()
        .iter()
        .map(|agent| {
            // 名字来自本机 Agent 目录；目录里查不到（Agent 已被移除）时退回 agentId——它是
            // 稳定值，不编造展示名。
            let name = view
                .agents
                .iter()
                .find(|descriptor| descriptor.agent.agent_id() == agent)
                .map(|descriptor| descriptor.agent.name().to_owned())
                .unwrap_or_else(|| agent.as_str().to_owned());
            Ok(ExportAgent {
                agent_id: node_link_protocol::common::AgentId::parse(agent.as_str())
                    .map_err(|_| ProjectionFault::Field)?,
                name: NonEmptyText::<128>::parse(&name).map_err(|_| ProjectionFault::Field)?,
                capabilities_ref: OpaqueRef::parse(agent.as_str())
                    .map_err(|_| ProjectionFault::Field)?,
            })
        })
        .collect::<Result<Vec<_>, ProjectionFault>>()?;
    let workspace_aliases = export
        .workspace_aliases()
        .iter()
        .map(|entry| {
            Ok(WorkspaceAliasEntry {
                alias: WorkspaceAlias::parse(entry.alias().as_str())
                    .map_err(|_| ProjectionFault::Field)?,
                display_name: NonEmptyText::<128>::parse(entry.display_name())
                    .map_err(|_| ProjectionFault::Field)?,
            })
        })
        .collect::<Result<Vec<_>, ProjectionFault>>()?;
    let mut templates = Vec::with_capacity(export.templates().len());
    for template in export.templates() {
        if !template.params().is_empty() {
            return Err(ProjectionFault::ParametricTemplate);
        }
        templates.push(WorkspaceTemplate {
            template_id: node_link_protocol::common::TemplateId::parse(
                template.template_id().as_str(),
            )
            .map_err(|_| ProjectionFault::Field)?,
            display_name: NonEmptyText::<128>::parse(template.display_name())
                .map_err(|_| ProjectionFault::Field)?,
            workspace_alias: WorkspaceAlias::parse(template.workspace_alias().as_str())
                .map_err(|_| ProjectionFault::Field)?,
            params: Vec::new(),
        });
    }
    let scopes = export
        .scopes()
        .iter()
        .map(GrantName::from_str)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ProjectionFault::Field)?;
    Ok(ExportEntry {
        export_id: ExportId::parse(export.export_id().as_str())
            .map_err(|_| ProjectionFault::Field)?,
        display_name: NonEmptyText::<128>::parse(export.display_name())
            .map_err(|_| ProjectionFault::Field)?,
        agents,
        workspace_aliases,
        default_workspace_alias: WorkspaceAlias::parse(export.default_workspace_alias().as_str())
            .map_err(|_| ProjectionFault::Field)?,
        templates,
        scopes: GrantList::new(scopes).map_err(|_| ProjectionFault::Field)?,
        capability_ceiling_ref: OpaqueRef::parse(export.export_id().as_str())
            .map_err(|_| ProjectionFault::Field)?,
        cache_policy: NoContentCache,
        revoked: export.is_revoked(),
    })
}

/// 解码消息 body（`Envelope` 的 body 是原始字节；失败按 schema 错误处理）。
fn decode<T: DeserializeOwned>(message: &Envelope) -> Option<T> {
    serde_json::from_str(message.body().get()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_admin::test_support::test_public_key;
    use acp_core::model::NodeState;

    fn timestamp(text: &str) -> acp_core::model::Timestamp {
        acp_core::model::Timestamp::new(text).expect("固定时间戳")
    }

    fn node(grants: &[&str], state: NodeState) -> NodeRecord {
        // `revoked_at` 与 `Revoked` 必须成对成立（模型不变量）。
        let revoked_at =
            (state == NodeState::Revoked).then(|| timestamp("2026-06-01T00:00:00.000Z"));
        NodeRecord::try_new(
            acp_core::model::NodeId::new("2ae1c07c-9242-46e9-a9d2-4ec58c130f49").expect("node id"),
            "Office Access",
            acp_core::model::NodeKind::Access,
            test_public_key().fingerprint(),
            acp_core::model::GrantSet::try_from_iter(grants.iter().copied()).expect("grants"),
            state,
            None,
            timestamp("2026-01-01T00:00:00.000Z"),
            None,
            revoked_at,
        )
        .expect("节点记录")
    }

    fn export(id: &str, scopes: &[&str], revoked: bool) -> ExportRecord {
        let alias = acp_core::model::WorkspaceAlias::new("project").expect("alias");
        ExportRecord::try_new(
            acp_core::model::ExportId::new(id).expect("export id"),
            "Project Export",
            vec![acp_core::model::AgentId::new("codex").expect("agent id")],
            vec![
                acp_core::model::WorkspaceAliasEntry::try_new(alias.clone(), "Project")
                    .expect("alias entry"),
            ],
            alias.clone(),
            vec![
                acp_core::model::ExportTemplate::try_new(
                    acp_core::model::TemplateId::new("template-1").expect("template id"),
                    "Template",
                    alias,
                    Vec::new(),
                )
                .expect("template"),
            ],
            acp_core::model::TemplateId::new("template-1").expect("template id"),
            acp_core::model::GrantSet::try_from_iter(scopes.iter().copied()).expect("scopes"),
            acp_core::model::CachePolicy::NoContentCache,
            timestamp("2026-01-01T00:00:00.000Z"),
            revoked.then(|| timestamp("2026-02-01T00:00:00.000Z")),
        )
        .expect("Export 记录")
    }

    /// R51/R52 的可见性规则：**E2 与节点 grants 不相交即不可见**（2026-09-26 裁决的 (b)）。
    #[test]
    fn visibility_requires_a_non_empty_scopes_intersection_with_the_node_grants() {
        let paired = node(&["grant.observe"], NodeState::Paired);
        let visible = export(
            "11111111-1111-4111-8111-111111111111",
            &["grant.observe", "grant.interact"],
            false,
        );
        // E2：scopes 与该节点 grants 不相交（同一条 Export 对另一个节点可见与否只由 grants 决定）。
        let disjoint = export(
            "22222222-2222-4222-8222-222222222222",
            &["grant.remote-work"],
            false,
        );
        let exports = vec![visible.clone(), disjoint.clone()];
        let kept = visible_exports(Some(&paired), &exports);
        assert_eq!(kept.len(), 1, "只有相交的那一条可见");
        assert_eq!(kept[0].export_id(), visible.export_id());

        // 空 scopes、空 grants 都是不相交：空集与任何集合的交集为空。
        assert!(visible_exports(Some(&node(&[], NodeState::Paired)), &exports).is_empty());
        assert!(
            visible_exports(
                Some(&node(&["grant.observe"], NodeState::Paired)),
                &[export("33333333-3333-4333-8333-333333333333", &[], false)]
            )
            .is_empty()
        );
    }

    /// R51/R52 的两条边界：已撤销的 Export 不进 catalog；节点行不存在或未配对时可见集为空。
    #[test]
    fn visibility_drops_revoked_exports_and_requires_a_paired_node() {
        let paired = node(&["grant.observe"], NodeState::Paired);
        let live = export(
            "11111111-1111-4111-8111-111111111111",
            &["grant.observe"],
            false,
        );
        let revoked = export(
            "00000000-0000-4000-8000-000000000000",
            &["grant.observe"],
            true,
        );
        let exports = vec![revoked, live.clone()];
        let kept = visible_exports(Some(&paired), &exports);
        assert_eq!(kept.len(), 1, "已撤销的条目必须被过滤");
        assert_eq!(kept[0].export_id(), live.export_id());

        assert!(
            visible_exports(None, &exports).is_empty(),
            "未配对节点看不到任何 Export"
        );
        assert!(
            visible_exports(
                Some(&node(&["grant.observe"], NodeState::Revoked)),
                &exports
            )
            .is_empty()
        );
    }

    /// 批次内顺序稳定：按 `exportId` 升序，与记录顺序无关。
    #[test]
    fn visibility_is_sorted_by_export_id() {
        let paired = node(&["grant.observe"], NodeState::Paired);
        let exports = vec![
            export(
                "33333333-3333-4333-8333-333333333333",
                &["grant.observe"],
                false,
            ),
            export(
                "11111111-1111-4111-8111-111111111111",
                &["grant.observe"],
                false,
            ),
        ];
        let kept: Vec<&str> = visible_exports(Some(&paired), &exports)
            .iter()
            .map(|record| record.export_id().as_str())
            .collect();
        assert_eq!(
            kept,
            [
                "11111111-1111-4111-8111-111111111111",
                "33333333-3333-4333-8333-333333333333"
            ]
        );
    }
}
