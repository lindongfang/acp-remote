# WP5 fix1 Handoff — RV1-WP5 的 F1/F3/F4/F5/F6（`node-link-owner` / WP5 catalog 与 resource）

## Shared Report

- **task_id**: `RV1-WP5-F1` / `RV1-WP5-F3` / `RV1-WP5-F4` / `RV1-WP5-F5` / `RV1-WP5-F6`（对应 review 报告 `reports/rv1-wp5.md` 的 Findings；F2 是 report-only，主 Agent 已裁决归 WP7 的 2.21/2.22，本轮**不改**）
- **role**: coder；**phase**: fix；**stage**: work-package；**evidence_id**: `RV1-WP5`
- **agent_context**: 任务级 coder 子 Agent（fix 轮次），**不继承** WP5 实现或 review 会话，只按主 Agent 下发的 fix 清单与 `rv1-wp5.md` 的 Findings 原文作业。工作目录 `D:\Project\acp-remote-wt\node-link-owner`（worktree，分支 `agentic/node-link-owner`）；权威规划根 `D:\Project\acp-remote`（报告与日志写在该处 `openspec/changes/node-link-owner/reports/`，该目录不入版本控制）。工具集含 shell 与 Git：本轮 diff、门禁与红向证据均由本 Agent 亲自执行。
- **base / target_revision**: base = `6b1f0b9cb100638eb9e15ca414d5013a86aebaf1`（RV1-WP5 的 Target Revision；开工时 HEAD 与之一致、`git status --porcelain` 为空）；**target = `27ad3f800c7821846b8991ae7d61b67f59e8ba24`**（本轮三个提交后的 HEAD，`git status --porcelain` 为空、无 staged 文件）。
- **scope**（严格按派单允许写入面）：
  - `crates/server/src/node_link/resource.rs`（F3/F4/F5 的生产改动）；
  - `crates/server/src/node_link/resource/tests.rs`（F3/F4 用例 + fixture 的 `remoteSessionRef` 改取本机 node id）；
  - `crates/server/src/node_link/catalog.rs`（F1 的三条路由级用例，全部在 `#[cfg(test)] mod tests` 内；生产代码未改）；
  - `docs/CORE_PORTS_AND_STORAGE.md`（仅 §4 的计数句，F6）。
  - **未做**：未改 `docs/NODE_LINK_PROTOCOL.md`、`docs/MODULE_ARCHITECTURE.md`、`schemas/**`、`compatibility/**`、`fixtures/**`、协议 crate、`openspec/**`（proposal/specs/design/plan/tasks/verification）、其他工作包文件；无清单外编辑（本轮没有编译连带）。
- **changes**（3 个提交，4 个文件，+464/−30）：
  1. **F1（P2）catalog 补 3 条路由级用例**：用与 `resource/tests.rs` 同款的 Fixture（`TestWorld` 端口替身 + 真实 `ConnectionHandle` 出站队列 + 真实 `CatalogRoute`）覆盖 ① E1/E2 中 E2 与该节点 grants 不相交 → 一帧快照只含 E1（逐字段断言 `exportId`/`displayName`/`agents[].{agentId,name,capabilitiesRef}`/`workspaceAliases[]`/`defaultWorkspaceAlias`/`templates[].{…,params:[]}`/`scopes`/`capabilityCeilingRef`/`cachePolicy`/`revoked` + `revision` 取 `AuditStore::watermark`）；② 协商批次=2、可见 3 条 → 恰好两帧且批次内/批次间按 `exportId` 升序稳定（种子顺序刻意错开，并重复订阅比对逐帧一致）；③ 空可见集 → 一帧空快照。生产代码零改动（切分与过滤逻辑本已正确，本轮只是把 R51/R53 的行为变成可判定证据）。
  2. **F3（P2）`states` 表回收**：`ResourceRoute::subscribers` 在遍历状态表时把「注册表里已无该 `connectionId`」的 key 登记到本地 `ended`，遍历结束后统一 `states.remove`（不在遍历中改动表），并改写方法文档（原先写「留到下次 `forget_if_closed` 或再次注册」——正常结束的连接永远走不到那条路）。新增用例 `a_finished_connection_state_entry_is_reclaimed_by_the_next_fan_out`。
  3. **F4（P2）`ownerNodeId` 校验**：`ResourceRoute` 增 `node_id: NodeId` 字段（`new` 第三个参数；组合根应传 `Authority::local_node`，与握手 `node.ready.ownerNodeId` 同源），新增 `owner_matches(remote)`；`on_attach` 在签发 attachment **之前**比对（不符 → `nodelink.export.not_found`，与既有「export/session id 非法」「会话不在这台 Owner」同码），`on_ack` 在记账**之前**比对（不符 → `nodelink.protocol.sequence_invalid`，保持 §12.4 第 559 行「`sessionRef` 不属于本连接」的既有语义）。回显因此只可能是本机 id：既有用例 `persisted_events_reach_the_subscribed_connection` 里「`sessionRef.ownerNodeId` == 对端 id」的断言改为「== 本机 id」。新增正反用例：`an_attachment_is_refused_for_a_foreign_owner_node`、`an_ack_from_a_foreign_owner_node_is_rejected_without_advancing_the_watermark`（后者用「同一个 cursor 用本机 `sessionRef` 仍被接受」证明被拒的 ACK 没有推进水位）。
  4. **F5（P2）四处静默跳过补结构化 `warn!`**：① 扇出取正文失败/不可映射（`node_link.fanout_payload_invalid`：`access_node_id` + `event_id`）；② 快照未决交互的逐条失败（`node_link.snapshot_pending_interaction_unmappable`：`access_node_id` + `session_id` + `interaction_id` + `origin_event_id`，由新的 `map_pending_interaction` 承担映射体、`pending_interaction` 成为「日志 + 返回」的薄包装）；③ 快照层面的汇总省略（`node_link.snapshot_items_omitted`：`omitted`/`total`，只在该次快照真有条目被省略时发一条，避免与 ② 重复）；④ 重放路径两处（`node_link.replay_event_payload_invalid` / `node_link.replay_event_invalid`：`access_node_id` + `event_id`）。字段与既有 `node_link.fanout_event_invalid` 同一口径：只有标识与计数，**不含任何正文或秘密**。
  5. **F6（P2）文档计数**：`docs/CORE_PORTS_AND_STORAGE.md` §4 的「Node Link 资源读面的三条窄 seam（四个名字）」改为「**三条端口 seam（`ReadView::node_link_slice`/`ReadView::session_event_payload`/`AuditStore::watermark`）与它们的四个用例入口（`node_link_catalog_view`/`node_link_session_view`/`node_link_replay`/`node_link_event_payload`）**」，并把随后一句的主语「三条入口都不放宽……」改为「四个用例入口都不放宽……」。文档头部版本段（§18 行）的「三条窄 seam」本就指的是三个端口方法，未改。
- **checks**（原始输出见 `wp5-catalog-resource.log` 的「wp5-fix1 轮次」）：
  - `cargo fmt --all -- --check` exit=0。
  - `cargo clippy --locked -p server --all-targets --all-features -- -D warnings` exit=0。
  - `cargo test --locked -p server --all-features` exit=0（server lib **248** 通过 / 0 failed，其余 6 个 test binary 全 ok）。target revision 的 server lib 是 **242**，本轮 **+6**（catalog +3、resource +3），**既有用例一条未删**；唯一改写断言的是上面 F4 提到的 `persisted_events_reach_the_subscribed_connection`（`.ownerNodeId` 的期望值从对端 id 改成本机 id）。
  - `npm run check` exit=0（`check:docs` 378 links / 4114 section refs；`check:boundaries` 12 crate；`check:drift` §7 36 条 DDL + §5 15 trait/93 方法；`check:agentic` 全 PASS）。
  - 提交前钩子（`.husky/pre-commit`）：三次提交各三步全通过（fmt → `npm run check` → workspace clippy），commitlint 通过（钩子提示本地未装 gitleaks，CI 的 `secrets` job 仍判定）。
  - **红向证据**（一次 patch 同时改坏四处刚修好的行为，只跑 `-- node_link::catalog node_link::resource`）：去掉 catalog 的可见性过滤（`project()` 改遍历 `view.exports`）→ ① `left: 2 / right: 1`（不相交的 E2 进了快照）、③ `exports` 从 `[]` 变成一条；忽略协商批次大小（`batch_size = 500`）→ ② `left: 1 / right: 2`（只发了一帧）；删掉 `states.remove` → F3 用例 `已结束连接的条目必须被回收` 断言失败；`owner_matches` 恒真 → F4 两条用例分别报「收到 `resource.attached` 而不是 `link.error`」与「回帧数 0 → 1（本该是 sequence_invalid 却被接受）」。**6 条新用例全部变红、且无其它用例变红**；`git checkout --` 恢复后复跑 `cargo test -p server --all-features` 248 通过 / 0 failed。
  - **未执行（如实记录）**：`cargo-deny` 与 `gitleaks` 只在 CI 运行、本地无等价物；本轮未跑 `npm run verify` 的 workspace 全量 `cargo test`（只跑 `-p server`，因为本轮改动面只到 `server` 与一份文档；WP5 上一轮的 workspace 全量结果见同目录日志的上一节）。
- **issues**: 5 项全部落地；无阻断。**一处需主 Agent 知悉的裁决**：F4 在 `resource.ack` 上的错误码与派单字面（「不等即 `nodelink.export.not_found`」）存在冲突——`docs/NODE_LINK_PROTOCOL.md` §12.4 第 559 行把「`sessionRef` 不属于本连接」钉死为 `sequence_invalid`。本 Agent 开工前以 `contact_supervisor(need_decision)` 上报，主 Agent 裁定 **(A)**：attach 用 `export.not_found`（尚无 attachment，「资源不在这台 Owner」）、ack 用 `sequence_invalid`（落回 §12.4 既有规则）；两条路径都「不签发 / 不记账」。实现与用例按 (A) 落地，`docs/NODE_LINK_PROTOCOL.md` 无需改动。
- **result**: **PASS**（本轮 fix 范围内全部适用条件满足、门禁全绿；**不代表** RV2-WP5 独立复核、`[PV5]`、E2E 或合并已完成）
- **evidence_paths**:
  - 本报告：`openspec/changes/node-link-owner/reports/wp5-fix1-handoff.md`
  - 被处理的 review：`openspec/changes/node-link-owner/reports/rv1-wp5.md`
  - 本轮原始输出（含红向证据）：`openspec/changes/node-link-owner/reports/wp5-catalog-resource.log` 的「wp5-fix1 轮次」
- **resource_cleanup**: 未新增端口、数据库、容器、证书或外部账号；未安装依赖、未改 `Cargo.toml`/`Cargo.lock`/任何 schema 与 compatibility 资产。临时文件只写在系统临时目录（`C:\Users\zhang\AppData\Local\Temp\wp5fix1\`，非仓库内）；提交后 `git status --porcelain` 为空，无未跟踪文件、无 staged 文件。

## 提交（供 reviewer 逐条核对）

| SHA | 标题 | 文件 |
|---|---|---|
| `82e7c52c2dd3da88c12c11fe5f375438d6a4071b` | `fix(server): 收紧 Node Link resource 路由的 owner 校验与静默跳过` | `crates/server/src/node_link/resource.rs`（1 file, +101/−13） |
| `fc0cbc03a8764f2a6d7a34fa671453b50d33db87` | `test(server): 补 Node Link catalog/resource 的路由级用例` | `crates/server/src/node_link/catalog.rs`、`crates/server/src/node_link/resource/tests.rs`（2 files, +361/−15） |
| `27ad3f800c7821846b8991ae7d61b67f59e8ba24` | `docs(core): 修正 CORE_PORTS §4 的 Node Link 读面 seam 计数` | `docs/CORE_PORTS_AND_STORAGE.md`（1 file, +2/−2） |

（三个提交均经 `.husky/pre-commit` 与 commitlint；提交后 `git status --porcelain` 为空。分支 `agentic/node-link-owner` 未 push。）

## 各条落点（reviewer 复检索引）

| ID | 修复落点 | 用例 / 证据 |
|---|---|---|
| RV1-WP5-F1 | `catalog.rs`：生产代码零改动；新增测试模块内的路由层 Fixture（`Fixture::new(catalog_snapshot_batch_size)` / `subscribe`）与三个用例 | `catalog.rs::the_catalog_snapshot_carries_every_field_of_the_visible_export_only`、`::the_catalog_snapshot_is_batched_by_the_negotiated_size_in_a_stable_order`、`::an_empty_visible_set_yields_exactly_one_empty_snapshot`；红向证据三条（见上） |
| RV1-WP5-F3 | `resource.rs::ResourceRoute::subscribers`（`ended` 登记 + 遍历后 `states.remove`）+ 方法文档 | `resource/tests.rs::a_finished_connection_state_entry_is_reclaimed_by_the_next_fan_out`（红向：`已结束连接的条目必须被回收`） |
| RV1-WP5-F4 | `resource.rs`：`ResourceRoute::new` 增 `node_id`、`owner_matches`、`on_attach` ① 步、`on_ack` ① 步 | `resource/tests.rs::an_attachment_is_refused_for_a_foreign_owner_node`（正向路径由既有 `attach`/`a_reissued_attachment_invalidates_the_previous_generation` 等覆盖）、`::an_ack_from_a_foreign_owner_node_is_rejected_without_advancing_the_watermark`；红向证据两条 |
| RV1-WP5-F5 | `resource.rs::fan_out`（payload 跳过）、`snapshot`（汇总省略）、`pending_interaction` + 新 `map_pending_interaction`、`send_events`（两处） | 无行为断言（结构化日志不进单测快照）；`clippy --all-targets -D warnings` 与既有扇出/快照/重放用例保证路径未回归。**登记**：v1 没有日志断言工具，故 F5 的红向证据不可自动化，本轮只做代码级核对 |
| RV1-WP5-F6 | `docs/CORE_PORTS_AND_STORAGE.md` §4 两处句子 | `npm run check` 的 `check:docs`（378 relative links / 4114 section refs）与 `check:drift`（§5 端口签名逐条一致）退 0 |

## 决策与边界（供 reviewer 判 applicability）

1. **`remoteSessionRef.ownerNodeId` 判定基准 = 本机 node id**：`ResourceRoute::new` 的第三个参数由组合根注入（WP7 应传 `Authority::local_node()`，与 `node.ready.ownerNodeId` 同源）。WP6 的 `command.submit` 也会带 `sessionRef`，按同一口径需要 `owner_matches`——本层未替 WP6 预置公共辅助（保持最小改动），WP6 落地时沿用 `self.node_id` 即可。
2. **`resource.ack` 的错误码为 `sequence_invalid`（主 Agent 裁定 (A)）**：这是 §12.4 第 559 行既有语义的直接落点，不是新增协议行为；`resource.attach` 用 `export.not_found` 与既有「export/session id 非法 → 同码」一致。
3. **F5 的日志口径**：四处都是「只带标识 + 计数」，字段名与既有 `node_link.fanout_event_invalid` / `node_link.fanout_payload_failed` 同族（`access_node_id`/`event_id`/`session_id`/`interaction_id`/`origin_event_id`），无正文、无 payload、无秘密。快照路径刻意拆成「逐条原因（callee）」+「本次快照汇总（caller，>`0` 才发）」两条，避免同一事件重复两条日志。
4. **F3 的回收是惰性的**（只在扇出时发生）：没有事件要投递的连接不产生任何成本，因此不需要给 `conn` 增加连接结束回调（那会改动 WP4 的连接生命周期接口，超出本层边界）。`forget_if_closed` 保留，作为「投递失败且句柄已摘除」的第二条清理路径。

## handoff_index

```yaml
handoff_index:
  - task_id: "RV1-WP5-F1"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "27ad3f800c7821846b8991ae7d61b67f59e8ba24"
    evidence_type: CHECK
    evidence_id: RV1-WP5
    report_path: "reports/wp5-fix1-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "catalog 补 3 条路由级用例（Fixture：TestWorld 端口替身 + 真实 ConnectionHandle 出站队列 + 真实 CatalogRoute）：E2 与节点 grants 不相交只见 E1 且逐字段齐全、协商批次=2 的 3 条可见 Export 切成两帧且顺序稳定（含重复订阅比对）、空可见集一帧空快照；红向证据：去掉可见性过滤与忽略协商批次大小后 3 条用例全红（2 vs 1 条、[] vs 一条、1 帧 vs 2 帧），恢复后 248 通过"
    source_evidence: NOT_APPLICABLE
  - task_id: "RV1-WP5-F3"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "27ad3f800c7821846b8991ae7d61b67f59e8ba24"
    evidence_type: CHECK
    evidence_id: RV1-WP5
    report_path: "reports/wp5-fix1-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "subscribers() 以注册表为活跃判据并顺带回收不在 handles() 里的 key（遍历中登记 ended、遍历后 remove）；用例 a_finished_connection_state_entry_is_reclaimed_by_the_next_fan_out 红向证据：删掉 remove 后『已结束连接的条目必须被回收』失败；活跃连接的状态仍保留（同用例前段断言 len==1 且事件仍投递）"
    source_evidence: NOT_APPLICABLE
  - task_id: "RV1-WP5-F4"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "27ad3f800c7821846b8991ae7d61b67f59e8ba24"
    evidence_type: CHECK
    evidence_id: RV1-WP5
    report_path: "reports/wp5-fix1-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "新增 node_id 字段 + owner_matches；attach 不符 → export.not_found 且不签发（用例断言无 attachment 条目），ack 不符 → sequence_invalid 且不记账（主 Agent 裁定 (A)，落回 NODE_LINK_PROTOCOL §12.4 第 559 行既有语义）；红向证据：owner_matches 恒真时两条用例分别收到 resource.attached 与多出 1 帧 link.error；sessionRef 回显的 owner 分量因此恒为本机 id"
    source_evidence: NOT_APPLICABLE
  - task_id: "RV1-WP5-F5"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "27ad3f800c7821846b8991ae7d61b67f59e8ba24"
    evidence_type: CHECK
    evidence_id: RV1-WP5
    report_path: "reports/wp5-fix1-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "四处静默跳过全部改为结构化 warn（fanout_payload_invalid / snapshot_pending_interaction_unmappable + snapshot_items_omitted 汇总 / replay_event_payload_invalid / replay_event_invalid），字段与 fanout_event_invalid 同口径且不含正文；映射体拆成 map_pending_interaction 以在唯一出口记日志；F5 无自动化红向证据（v1 无日志断言工具，已在报告中登记）"
    source_evidence: NOT_APPLICABLE
  - task_id: "RV1-WP5-F6"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "27ad3f800c7821846b8991ae7d61b67f59e8ba24"
    evidence_type: CHECK
    evidence_id: RV1-WP5
    report_path: "reports/wp5-fix1-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "CORE_PORTS §4 改为『三条端口 seam（ReadView::node_link_slice/ReadView::session_event_payload/AuditStore::watermark）与它们的四个用例入口（node_link_catalog_view/node_link_session_view/node_link_replay/node_link_event_payload）』，末句主语改『四个用例入口』；npm run check 的 check:docs 与 check:drift 退 0"
    source_evidence: NOT_APPLICABLE
```

## 残留风险与下游注意

1. **待独立复核**：本报告是 coder 自检（`[PV3]` 范围），不代替 RV2-WP5 的复检、`[PV5]`（2.22/3.9）或候选门禁。建议 reviewer 在 `27ad3f8` 上按原 ID 逐条核对，重点看 F4 在 `on_ack` 上的错误码是否与 §12.4 第 559 行一致（本轮按主 Agent 裁定 (A)），以及 F3 的回收是否会影响「连接仍在注册表但句柄即将摘除」的窗口（该窗口内 `handles()` 仍含句柄，因此不会被回收）。
2. **WP7 接线注意**：`ResourceRoute::new` 现在是三参数，第三个参数必须是**本机** node id（`Authority::local_node()`）；传成对端 id 会让所有 `resource.attach`/`resource.ack` 被拒。
3. **WP6 复用形状**：`command.submit` 的 `sessionRef` 也应做同一 owner 分量校验；本层把 `owner_matches` 留在 resource 路由内（未上提为公共辅助），WP6 落地时按同一口径实现即可，若要共享需先经主 Agent 裁决模块边界。
4. **F5 无自动化证据**：四处 warn 只能靠代码核对与 `clippy -D warnings` 保证；若后续希望回归保护，需要先引入日志捕获设施（本轮未引入，属超范围）。
5. **`resource.ack` 无限流**（RV1-WP5 的残余风险 2）与**扇出队列满即丢**（残余风险 1）本轮**未处理**，按 reviewer 的裁决分别留在合同登记与 WP7 集成测试。
6. **未执行（如实记录）**：`cargo-deny` 与 `gitleaks` 只在 CI 生效，本地无等价物；本轮未跑 workspace 全量 `cargo test`（`-p server` 已覆盖改动面）。
