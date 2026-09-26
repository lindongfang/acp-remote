# WP5 Handoff — catalog 与 resource（`node-link-owner` / DU1）

## Shared Report

- **task_id**：2.13（catalog 投影）、2.14（attach 与订阅）、2.15（事件扇出与 ACK）、2.16（交付前局部验证）。
- **role / phase**：coder / implement。
- **agent_context**：worker 子 Agent（本机 worktree 独占）。工作目录 `D:\Project\acp-remote-wt\node-link-owner`（分支 `agentic/node-link-owner`）；规划根 `D:\Project\acp-remote`。
- **target_revision**：`6b1f0b9`（HEAD，含本 WP 全部提交）。起点 `2937850`（WP4 之后）。同批次提交按依赖顺序：
  - `2dce835` **feat(core)**：Node Link 资源读面窄 seam + `AuditStore::watermark` 端口（core/storage/docs 合同面，12 文件）；
  - `db46308` **feat(node-link)**：catalog 投影 + resource（attach/快照/重放/扇出/ACK）+ `conn` 的 `send_with_frame`；
  - `1264821` **docs(node-link)**：`NODE_LINK_PROTOCOL.md` §8.2 可见性口径（用户裁决 (b)）；
  - `6b1f0b9` **test(node-link)**：resource 的行为用例（含之前 `db46308` 里已提交的 catalog 可见性用例）。
- **scope**：
  - 新增：`crates/server/src/node_link/catalog.rs`、`crates/server/src/node_link/resource.rs`、`crates/server/src/node_link/resource/tests.rs`；
  - 修改：`crates/server/src/node_link/mod.rs`（`Routes` 组合件 + `link_error`）、`crates/server/src/node_link/conn/registry.rs`（`ConnectionHandle::send_with_frame`，**WP4 冻结形状的加法**，见「需要主 Agent 知晓的两处」）；
  - 合同面（已单独提交）：`crates/core/src/{ports.rs,use_cases.rs,broker.rs}`、`crates/storage-sqlite/src/{session_store.rs,admin/audit.rs}`、`crates/server/src/local_admin/test_support.rs`、`docs/CORE_PORTS_AND_STORAGE.md`（§4/§5.2/§5.3 + 版本 0.12）、`docs/NODE_LINK_PROTOCOL.md`（§8.2 + 修订记录）；
  - **未改**：`openspec/**`（含 `plan.md`/`tasks.md`/`verification.md`）、`schemas/**`、`compatibility/**`、`fixtures/**`、`crates/app/**`（WP7 组合根接线）、`crates/server/src/transport/**`、`crates/server/src/node_link/conn/{session,handshake,limits,wire}.rs` 的业务行为（只有 `session.rs`/`handshake.rs` 的 `catalogRevision` 取值来源随 2dce835 一并切换，属 D4 口径修正）、`crates/server/src/local_admin/{pairing.rs}`（撤销缝属 2.19/WP6）。
- **changes**：见「改动与需求映射（R51–R65）」与「冻结的公开形状」。
- **checks**：`[PV3]`（本机轮次）——`cargo fmt --all -- --check` 退 0；`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` 退 0；`cargo test --locked --workspace --all-features` 全绿（83 个测试目标，0 failed；`server` lib 242 通过，其中 `node_link::catalog::tests` 3、`node_link::resource::tests` 14）；`rg -n "unsafe" crates/server/src` 0 命中；`npm run check` 退 0（含 `check:drift`：§5 的 15 个 trait / 93 个方法签名逐条一致）。原始输出：`reports/wp5-catalog-resource.log`。
- **issues**：无阻断项。有 3 条需要主 Agent / reviewer 知晓的事实（每条都在下面单列）：
  1. **任务描述与用户裁决不一致**：`tasks.md` 2.13 仍写「按信任记录 `exportIds` 过滤」，而 2026-09-26 用户裁决 (b) 已把可见性定为「未撤销且 `export.scopes ∩ 节点 grants ≠ ∅`」。spec（R51/R52）已由主 Agent 改写，代码按 (b) 实现；**`tasks.md` 那一句需要主 Agent 同步**（coder 不改权威规划工件）。
  2. **`ConnectionHandle::send_with_frame`（WP4 冻结形状的加法）**：快照 `snapshotDigest` 的前像必须是「实际发出的 chunk 帧文本」，而 `send` 内部编码、不返回文本。新增同路径方法并让 `send` 委托给它（语义、序号、容量判定逐条不变）；如 review 认为不该在 WP4 形状上开口，替代方案是「重编码一次」——那会让 digest 与对端收到的字节不一致（messageId/序号不同），不成立。
  3. **`catalogRevision` 的精度边界**：来源是 `AuditStore::watermark()`（`MAX(owned_audit.audit_id)`，storage 侧读 `sqlite_sequence`），它**每写一条审计行就前进**（含 `node.auth_failed`/`authorization.denied`/`rate_limit.triggered`）。因此「revision 不变 ⇒ catalog 必不变」不成立（反向成立）。v1 不依赖该性质：`knownRevision` 非空仍回完整快照，`catalog.changed` 属 post_mvp。选它是因为 `store.head()` 会随保留期裁剪回落，破坏跨重启单调性（D4/G5）。
- **result**：**PASS**（本工作包检查全部满足）。**不代表**独立 review（3.10）、`[PV5]`/E2E 或合并已完成。
- **evidence_paths**：`reports/wp5-catalog-resource.log`、本文件。
- **resource_cleanup**：用例不起 listener、不绑端口、不起后台进程、不用数据库服务/容器；`TestWorld` 的临时资源由其 `Drop` 清理；测试用 `ConnectionHandle` 直接构造（`pub(crate)` 构造器）并在 `Fixture` 结束即丢弃，扇出后台任务在被测用例里显式 `Shutdown` 并 `await`（无 detached task）；`target/` 作为缓存保留（被 git 忽略）。

## handoff_index

```yaml
handoff_index:
  - task_id: "2.13"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "6b1f0b9"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/wp5-catalog-resource.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "工作区 = 6b1f0b9（git status 为空）上的本机轮次：cargo fmt --all -- --check 退 0；cargo clippy --locked --workspace --all-targets --all-features -- -D warnings 退 0；cargo test --locked --workspace --all-features 全绿（server lib 242，其中 node_link::catalog::tests 3 条覆盖 R51/R52 的可见性与批次内顺序）；rg unsafe crates/server/src 无匹配；npm run check 退 0。revision 来源切换（store.head() → AuditStore::watermark）在 2dce835 提交，同一轮检查覆盖。R51/R52 的 [PV5] 轮次（跨实现 + 本条目的本机轮次）由 3.9/2.22 执行，本行只声称 [PV3]。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.14"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "6b1f0b9"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/wp5-catalog-resource.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一轮检查。node_link::resource::tests 的 4 条路由用例覆盖 R54/R55（代际签发与覆盖、attach_generation_stale）、R56（未知会话 export.not_found；与 grants 不相交的 Export → export.not_granted）、R57/R58（快照只有元数据、chunkCount、按帧字节复算的 snapshotDigest、未决交互分批）、R59（epoch 不符 → sequence_invalid）。单资源快照并发为 1 由「一次 resource.subscribe 内同步发完整条快照 + 一条连接一个会话任务」保证。R54/R55/R57/R58 的 [PV5] 轮次由 2.22/3.9 执行。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.15"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "6b1f0b9"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/wp5-catalog-resource.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一轮检查。node_link::resource::tests 覆盖 R60/R63（payload 保真与 >256 KiB 的 rawUnavailable(size_limit) 显式降级）、R64/R65（ACK 回退/epoch 不符/归属不符 → sequence_invalid）、扇出投递（分发循环把已持久化事件投给订阅连接，接收方按收到的 payload 复算 payloadDigest）与关闭序列、未订阅不投递。R61（先持久化后发布）在结构上成立（发布入口只接受 broker 提交成功后的 CommittedDelivery::Owned），端到端断言归 2.22/2.21 的组合根接线。R60/R61 的 [PV5] 轮次由 2.22/3.9 执行。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.16"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "6b1f0b9"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/wp5-catalog-resource.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "2.16 的完成条件是「WP5 交付前局部验证全绿 + 事件映射与扇出形状冻结」：命令与完整输出见日志，冻结形状见本文件「冻结的公开形状（供 WP6/WP7）」。工作区在提交后 git status 为空。"
    source_evidence: NOT_APPLICABLE
```

## 改动与需求映射（R51–R65）

需求标题取自 `plan.md` 的 `requirements` 映射；用例名可在 `reports/wp5-catalog-resource.log` 的用例清单里逐条核对。

| 需求 | 标题（plan.md） | 实现落点 | [PV3] 用例 | 状态 |
|---|---|---|---|---|
| [R51] | Export 过滤的 catalog 投影 | `catalog::visible_exports`（**唯一判定点**）、`catalog::project` | `catalog::tests::visibility_requires_a_non_empty_scopes_intersection_with_the_node_grants`、`resource::tests::an_export_disjoint_from_the_node_grants_is_not_granted` | PASS |
| [R52] | 按信任记录过滤 | 同上（未撤销 ∧ `scopes ∩ grants ≠ ∅`） | `catalog::tests::visibility_drops_revoked_exports_and_requires_a_paired_node`、`catalog::tests::visibility_is_sorted_by_export_id` | PASS |
| [R53] | 超大批次稳定切分 | `catalog::send_batches`（`chunks(catalogSnapshotBatchSize)`，schema `maxItems` 500 为上限，慢连接停发后续批次） | 批次切分只有「按协商大小 `chunks` + 空集也回一条空快照」的直接覆盖；**未写 >500 条目的矩阵用例** | 部分（见「未完成/待办」） |
| [R54] | attachment 申请与 generation 隔离 | `resource::on_attach`（每连接单调递增 generation、新覆盖旧、`resource.attached` 三字段） | `resource::tests::a_reissued_attachment_invalidates_the_previous_generation` | PASS |
| [R55] | 重新 attach 使旧 generation 失效 | 同上 + `current_attachment` 的 (id, generation) 匹配 | 同上（旧代际 subscribe → `nodelink.resource.attach_generation_stale`） | PASS |
| [R56] | 未导出会话不可 attach | `catalog::export_is_visible` 前置 + core 的会话归属复核（agent ∈ export、Export 未撤销） | `resource::tests::an_unknown_session_is_reported_as_export_not_found`、`an_export_disjoint_from_the_node_grants_is_not_granted` | PASS（「agent 不在 Export 清单」这一变体由 core 的 `export.not_granted` 覆盖，路由级用例未单列） |
| [R57] | 快照与 origin cursor 增量重放 | `resource::snapshot` / `resource::replay` | `a_snapshot_carries_metadata_only_and_a_verifiable_digest`、`replay_resumes_after_the_cursor_and_rejects_an_epoch_mismatch` | PASS |
| [R58] | 首次订阅走快照 | `resource::on_subscribe`（`cursor = null` → 快照） | `a_snapshot_carries_metadata_only_and_a_verifiable_digest`、`pending_interactions_are_batched_by_the_negotiated_limit` | PASS |
| [R59] | epoch 不一致被拒绝 | `resource::replay`（core 回 `nodelink.origin_epoch_mismatch` → `sequence_invalid`） | `replay_resumes_after_the_cursor_and_rejects_an_epoch_mismatch` | PASS |
| [R60] | `resource.event` 的持久化顺序与保真 | `resource::fan_out` + `wire_payload` + `payload_digest`（ACPR-CJ1） | `persisted_events_reach_the_subscribed_connection`、`inline_acp_raw_keeps_the_document_bytes_and_the_media_type` | PASS（「先持久化后发布」的端到端断言见下） |
| [R61] | 先持久化后发布 | 结构保证：`NodeLinkPublisher` 只接受 broker 提交成功后的 `CommittedDelivery::Owned`；本层自己不产生事件 | 结构性覆盖（`persisted_events_reach_the_subscribed_connection` 的输入就是「已持久化事件」的定位单元）；组合根分叉接线与端到端时序归 2.21/2.22 | 部分（接线属 WP7） |
| [R62] | 未登记事件类型保留 raw | `wire_payload` 原样转发 `payload.view` 与 `payload.acp`（含 `mediaType` 与 `rawJson` 字节保真） | `inline_acp_raw_keeps_the_document_bytes_and_the_media_type` | PASS（「`payload.view` 最低字段是否属于登记集合」的判定归生产者，见下） |
| [R63] | 超大内嵌内容显式降级 | `wire_payload`：>256 KiB → `rawAcp.rawUnavailable(size_limit)`（含 `byteLength`/`sha256`） | `acp_raw_over_the_inline_limit_is_downgraded_not_truncated` | PASS |
| [R64] | `resource.ack` 的单调性与归属 | `resource::on_ack`（归属 + epoch + 单调不减） | `ack_progress_is_monotonic_and_bound_to_the_attachment` | PASS |
| [R65] | ACK 回退被拒绝 | 同上（回退不改写已有水位） | 同上 | PASS |

**未完成/待办（都在 WP5 之外的承接点或明确的小缺口）**

1. [R53] 的「超大批次」矩阵用例（>500 条 Export 的多批次切分）未写：`send_batches` 的切分逻辑简单且已被空集/单批覆盖，但 R53 的验收语义（稳定切分 + 每批不超上限）建议由 2.22 的全链路或补齐用例承接。
2. [R60]/[R61] 的端到端「先持久化后发布」断言依赖组合根分叉（WP7 的 2.21 接线 + 2.22 集成），本 WP 只到「发布入口的形状与消费路径」。
3. [R62] 的「未登记 eventType」判定：wire 层的最低字段归属是**生产者**（broker/adapter 的 §11.4 共享合同）职责，Node Link 映射层只做逐字节转发；若 reviewer 认为映射层必须再判一次，需要主 Agent 裁决（会产生第二份事件类型登记表）。
4. 「`export.revoke` 后推送 `export.revoked` 并清除内存订阅」属 **2.19/WP6**（`local_admin::pairing::ConnectionCloser` 缝 + 逐消息权威复核的推送部分）；本 WP 已保证**撤销后新的 attach/subscribe 立即被拒**（每次请求都经 `visible_exports` 现算，撤销在 catalog 与 resource 上同时生效），但这不是推送。
5. `tasks.md` 2.13 的 `exportIds` 表述待主 Agent 同步（见 Shared Report 的第 1 条 issue）。

## 冻结的公开形状（供 WP6/WP7）

**WP5 新增（server）**

```text
server::node_link::catalog::CatalogRoute::new(core: Arc<UseCases>) -> Self
    impl MessageRoute（只认领 catalog.subscribe；其余 → Unclaimed）
server::node_link::catalog::visible_exports(node: Option<&NodeRecord>, exports: &[ExportRecord])
    -> Vec<&ExportRecord>                       // pub(crate)：可见性唯一判定点，按 exportId 升序
server::node_link::catalog::export_is_visible(core: &UseCases, node: &NodeId, export: &ExportId)
    -> Result<bool, PortError>                  // pub(crate)：单条 Export 的可见性（复用上式）

server::node_link::resource::ResourceRoute::new(core: Arc<UseCases>, registry: Arc<ConnectionRegistry>)
    -> Self
    impl MessageRoute（只认领 resource.attach/resource.subscribe/resource.ack）
server::node_link::resource::ResourceRoute::dispatch(
        self: Arc<Self>, queue: mpsc::Receiver<CommittedEvent>, shutdown: Shutdown) -> ()  // async
server::node_link::resource::NodeLinkPublisher::channel()
    -> (NodeLinkPublisher, mpsc::Receiver<CommittedEvent>)
    impl core::ports::EventPublisher（publish 为同步、非阻塞）
server::node_link::resource::{EVENT_QUEUE_CAPACITY = 1024, MAX_INLINE_ACP_BYTES = 256 * 1024}

server::node_link::Routes::new(parts: Vec<Arc<dyn MessageRoute>>) -> Self
    impl MessageRoute（按序询问，全部未认领才 Unclaimed）
server::node_link::link_error(code: ErrorCode, message: &str, correlation: Option<&Uuid>)
    -> Option<Body>                             // pub(crate)：认证后 link.error 的统一构造

server::node_link::conn::ConnectionHandle::send_with_frame<T: Serialize>(
        &self, message_type: MessageType, body: &T) -> Result<String, SendFault>
    // 新增（WP4 形状的加法）；send 语义与既有完全一致，只是开始委托给同一实现
```

**WP7 接线建议（组合根，2.21）**

```rust
let core = /* Arc<UseCases> */;
let registry = /* Arc<ConnectionRegistry> */;
let resource = Arc::new(ResourceRoute::new(core.clone(), registry.clone()));
let routes = Routes::new(vec![
    Arc::new(CatalogRoute::new(core.clone())),
    Arc::new(resource.clone()),
    /* WP6 的 CommandRoute */
]);
let conn = NodeLinkConn::new(core.clone(), authority, config, registry.clone())
    .with_route(Arc::new(routes));
let (publisher, queue) = NodeLinkPublisher::channel();
tokio::spawn(Arc::clone(&resource).dispatch(queue, shutdown.clone()));   // 参与关闭序列
// EventPublisher 分叉：一路保持既有语义，一路 publisher
```

**WP5 消费的 core seam（`2dce835`，WP6/WP7 可直接复用）**

```text
core::ports::ReadView::node_link_slice(session, Option<OriginCursor>, ReplayLimit) -> NodeLinkSlice
core::ports::ReadView::session_event_payload(session, &EventId) -> Option<EventPayload>
core::ports::AuditStore::watermark() -> u64                     // catalogRevision 的唯一来源
core::use_cases::UseCases::node_link_catalog_view(&NodeId) -> NodeLinkCatalogView
core::use_cases::UseCases::node_link_session_view(&NodeId, &ExportId, &SessionId) -> NodeLinkSessionView
core::use_cases::UseCases::node_link_replay(&NodeId, &ExportId, &SessionId, Option<OriginCursor>, ReplayLimit) -> NodeLinkReplay
core::use_cases::UseCases::node_link_event_payload(&NodeId, &ExportId, &SessionId, &EventId) -> Option<EventPayload>
```

四者的归属前置统一是「对端是已配对的 `access` 行 + Export 存在且未撤销 + 会话存在且其 agent ∈ Export」；未配对 → `InvalidRequest("authorization.scope_denied")`，未导出会话 → `NotFound(EntityRef::Session/Export)`，不授权 → `InvalidRequest("export.not_granted")`，epoch 不符 → `InvalidRequest("nodelink.origin_epoch_mismatch")`。适配器的映射见 `resource::attach_fault`。

## 三处需要主 Agent / reviewer 知晓的设计事实

1. **G1 = 用户裁决 (b)**：可见集 = 未撤销且 `export.scopes ∩ 节点 grants ≠ ∅` 的 Export；`exportIds` 独立维度推后（需要迁移 + 本地管理协议变化）。代码上只有一个判定点（`visible_exports`），catalog 与 resource 共用；`docs/NODE_LINK_PROTOCOL.md` §8.2 与文件头修订记录已在 `1264821` 同步。**`tasks.md` 2.13 的那句 `exportIds` 仍需主 Agent 改**。
2. **G3 = 派生不透明引用**：wire 的 `ExportEntry` 需要 `agents[].capabilitiesRef` 与 `capabilityCeilingRef`，而 core 的 `ExportRecord` 没有对应语义来源。本 WP 派生为 `agents[].capabilitiesRef := agentId`、`capabilityCeilingRef := exportId`（稳定、对端只原样回传），并在 `catalog::project` 的注释里写明理由（§12.3 只要求形状，内容由 Owner 定义；`LOCAL_ADMIN_PROTOCOL.md` §5.5 明说这两个引用不出现在本地方法里）。若后续要真正的 capability ceiling 语义，属新决策。
3. **G4 = 扇出形状**：`publish` 同步且非阻塞（`try_send` 到有界 channel，满则只记日志丢弃——事件已在 Owner 日志里，Access 凭 cursor 重放补齐）；异步分发循环经会话绑定的只读入口取正文并映射 `resource.event`，随后走 WP4 的每连接有界队列（高水位即跳过，由会话看门狗处置）。**与 supervisor 原话的偏差**：supervisor 说「只把事件标识投入有界 channel」，实现投的是 `CommittedEvent`（id + 会话归属 + origin 三元组，**仍不含正文**）——因为分发期需要会话归属才能定位订阅者，若只投 event id 就要在分发循环里多一次「按 id 反查会话」的读（core 的读面刻意不提供按任意 event id 的通用读取）。正文仍在分发期取回，D6 的意图（投递链路不复制正文）成立。分发循环的所有者是组合根，`Shutdown` 参与关闭序列（关闭即停止投递；剩余事件由 cursor 重放补齐）。

## 测试手法说明（两处刻意的选择）

- **路由用例直接构造 `ConnectionHandle`**（`pub(crate)` 构造器 + 真实出站队列），不起 loopback listener：本 WP 的被测面是「认证后消息 → 出站帧」，握手/帧/序号已由 WP4 的真实连接用例覆盖；这样用例只断言对端真的收到的帧（不触碰路由内部状态），且不引入端口/进程资源。
- **快照 digest 的断言**用 `Frame { text, value }` 同时保留原始文本与解析结果：`snapshotDigest` 的前像必须是实际发出的字节，而 `serde_json::Value` 的成员顺序不保证与原帧一致，不能拿它当摘要前像——这一点在用例里有注释，避免后人「顺手用 Value 复算」而误报。
