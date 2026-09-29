# RV1-IMPL 独立代码检视报告（WP1–WP4 = tasks.md 2.1–2.4，交付单元 DU1）

```yaml
task_id: "2.1 / 2.2 / 2.3 / 2.4（合并为主行；逐条见 handoff_index）"
role: reviewer
phase: review
stage: work-package
agent_context: "新建 reviewer 子 Agent（本报告作者）；不继承实现对话、未参与本变更任何实现或实现讨论；只读检视（除本报告文件外未修改任何文件），未执行 cargo/npm，未切换分支、未提交"
target_revision: 5190924d041dcc7ab9bd93b4541b215f25f26bea
scope: "git -C D:/Project/acp-remote-wt/export-ids diff 3cadb12d79456750e40644a2ea53c96380b700e6 5190924d041dcc7ab9bd93b4541b215f25f26bea 的全部 25 文件（+1528/−133），以及其调用方与数据流：visible_exports / export_is_visible / is_nominated、Broker::node_allowed 的 Owner 侧判定、settle_pairing 的存储实现与适配层（params/router/view）、node list 投影、CLI 参数与展示"
changes: "本次检视为只读：未修改 worktree 内代码、测试、规划或任务状态；仅新增本报告"
checks: "[PV1] 已核对（主 Agent 亲跑日志）；[PV2]/[PV3]/[PV4] 由 [PV1] 的 workspace 轮次间接覆盖；[PV5] 与 check:drift / check:agentic 待返回（PENDING）"
issues: "3 MINOR + 3 SUGGESTION，无 CRITICAL/MAJOR（详见 Findings）"
result: PASS
evidence_paths:
  - "D:/Project/acp-remote/openspec/changes/node-trust-export-ids/reports/rv1-impl.md（本报告）"
  - "D:/Project/acp-remote/openspec/changes/node-trust-export-ids/reports/du1-pv1.log（主 Agent 独立执行，非实现者自述）"
resource_cleanup: "未创建任何资源（未跑 cargo/npm、未建临时目录、未起进程、未占端口）；本报告为唯一写入"
```

## 1. Review Context

- **Review ID / Type / Stage**：RV1-IMPL（本轮唯一）/ `branch` / work-package 候选准备（交付提交已产生、尚未集成）。这是该实现的**首轮**检视，`Previous Findings = NOT_APPLICABLE`。
- **被检对象**：worktree `D:\Project\acp-remote-wt\export-ids`，分支 `agentic/node-trust-export-ids`，HEAD = `5190924d041dcc7ab9bd93b4541b215f25f26bea`；工作区干净（`git status --porcelain` 为空）。
- **Base / Target**：`3cadb12d` → `5190924d`。25 个改动文件全部落在 `crates/{app,core,server,storage-sqlite}`。
- **越界文件核对（主 Agent 关切⑤）**：`git diff --name-only 3cadb12d 5190924d -- docs schemas fixtures compatibility openspec` = **0 个文件**；`Cargo.toml` / `Cargo.lock` = **0 个文件** ⇒ 无合同资产越界、无新增依赖。
- **需求基线**：`proposal.md`（四项裁决 + 三项小决策）、`design.md` D1–D10（含 D9 的 2026-09-27 勘误）、三份增量规范（R1–R15）、`plan.md`（WP 表 / Local Checks / [PV1]–[PV5] / 覆盖索引 R1–R18）、`tasks.md` 2.1–2.4、`verification.md`（Check Plan Changes）。
- **检视方法**：自读 base→target **完整** diff（逐文件，不依赖摘要）+ 读被改函数的调用方与数据流（`visible_exports` 的全部复用点、`node_allowed` 全分支、`settle_pairing` 事务、`upsert_node` 与 `owned_node` 的所有写入点、CLI/路由/视图投影），并用测试文件本身核对「断言是否真有判别力」（含 v1/v2/v3 夹具的**实际形状**，从 `.sqlite3` 夹具内嵌 DDL 直接读出）。
- **证据来源**：本报告的代码结论只来自目标版本的源码与测试；主 Agent 的 `reports/du1-pv1.log` 被我按「命令 / 退出码 / 测试汇总」逐项核对（85 个测试二进制、`passed` 求和 = 989、`0 failed` 85 次、`exit=0`），实现者自述日志（`wp*-*.log`）未被用作结论依据。

## 2. R1–R18 覆盖表

判定口径：**覆盖**=该场景/需求的每个 MUST 都有可读断言或直接代码判据；**覆盖（部分）**=核心判据覆盖，但缺一条组合/端到端断言；**缺口**=无断言也无可判据实现。

| # | 需求 / 场景 | 覆盖它的用例或代码位置（target=5190924d） | 结论 |
| --- | --- | --- | --- |
| R1 | 按信任记录过滤（`scopes ∩ grants = ∅` → 不可见） | `crates/server/src/node_link/catalog.rs` `visibility_requires_a_non_empty_scopes_intersection_with_the_node_grants`、`visibility_drops_revoked_exports_and_requires_a_paired_node`；`node_link/resource/tests.rs` `an_export_disjoint_from_the_node_grants_is_not_granted`；`node_link_e2e.rs` 的 `EXPORT_HIDDEN` 断言 | 覆盖 |
| R2 | 清单收窄可见集（`scopes ∩ grants` 相交但不在清单 → 不可见） | 新用例 `catalog.rs::visibility_narrows_to_the_nominated_export_ids_without_widening`（判别力充分：去掉条件③即断言失败）、路由层 `catalog.rs::the_catalog_snapshot_narrows_to_the_nominated_export_ids`、`resource/tests.rs::an_export_outside_the_nominated_list_is_not_granted`；E2E 的 `!ids.contains(EXPORT_NARROWED)` | 覆盖 |
| R3 | 空清单看不到任何 Export（即使相交） | `catalog.rs::visibility_narrows_...`（空清单分支）、`the_catalog_snapshot_narrows_...`（空快照）、`admin_store.rs::node_pairing_approval_stores_the_nominated_export_ids`（空清单落盘 `[]`）、`use_cases.rs::owner_side_node_authorization_requires_the_export_to_be_nominated` 分支③ | 覆盖 |
| R4 | 清单内 Export 被撤销后不可见 + 清单**不**被级联清理 | `catalog.rs::visibility_narrows_...` 末段（已撤销者即使被点名也不可见；并断言 `node_row.export_ids()` 未被改写）；`admin_store.rs` 同名用例（`revoke_export` 后清单列逐字保留） | 覆盖 |
| R5 | 超大批次稳定切分 | 既有 `catalog.rs::the_catalog_snapshot_is_batched_by_the_negotiated_size_in_a_stable_order`、`visibility_is_sorted_by_export_id`（第三条件在排序/切分**之前**过滤，`catalog.rs:257-272`） | 覆盖（本变更未改切分逻辑，无回归） |
| R6 | Owner 模式节点配对 | `local_admin/router.rs::node_pair_confirm`（已随 `exportIds` 更新）、`admin_store.rs::node_pairing_approval_persists_access_trust` | 覆盖 |
| R7 | 确认时填报可见清单（params/result/`node.list` 回显） | 新用例 `router.rs::node_pair_confirm_requires_and_forwards_the_export_ids` ③（乱序+重复 → `["export-a","export-b"]`，并断言落盘同序）、`router.rs` `node.list` 断言 `exportIds`（`router.rs:2891`）、`admin_store.rs::node_pairing_approval_stores_the_nominated_export_ids`、E2E `confirmed["exportIds"]` 断言 | 覆盖（CLI 侧展示为 `print_result` 原样输出，见 R8 行） |
| R8 | 空清单仍然建立信任 | `admin_store.rs`（空清单 → 信任行创建 + 列 `[]`）、`router.rs`（`exportIds: []` → `confirmed["exportIds"] == []`）、`cli/pairing.rs::an_empty_export_id_list_warns_without_blocking`（警告不阻断） | 覆盖 |
| R9 | 无效清单条目拒绝确认（`local.not_found` / `local.invalid_params`，零写入零审计） | `admin_store.rs::node_pairing_approval_rejects_invalid_export_ids_without_writing`（三类：不存在 / 已撤销 / 与本次 grants 不相交；逐项断言不建信任行、不推进配对状态、不写节点行、不写审计、不动既有配对行）；`params::node_pair_confirm` 的缺字段/类型非法（`router.rs:3109` 用例）+ 通用 `map_port_error`（`params.rs:914-940`） | 覆盖（部分）：错误分类与零写入有强断言，但**没有一条经真实 router+真实存储的组合断言**把 `NotFound(Export)` 映到 `local.not_found`（替身 `FakeTrust::settle_pairing` 不校验清单，见 F6） |
| R10 | `mode = "access"` / `rotate-key.begin` → `local.unsupported` | 既有 `router.rs` 用例（`local.unsupported`）+ `crates/app/tests/cli_commands.rs` 的 `node pair --mode access` 进程级断言；本变更未触碰该分支 | 覆盖（不适用为本变更新证据；行为未改） |
| R11 | 连续两次打开 schema 文本不变（4/4/3） | `migration.rs::current_version_database_is_untouched_by_two_consecutive_starts`（`owned_schema_version == 4`、`imported == 3`、每条 DDL 逐字节相同）、`commit.rs::session_update_keeps_state_when_absent_and_bumps_version`（`user_version == FILE_FORMAT_VERSION`） | 覆盖 |
| R12 | 升级中途失败整体回滚 | `migration.rs::a_failed_upgrade_rolls_back_to_v1`（在 v1→v4 连续升级链的 v2 段注入故障 → 整体回滚 → 重开可再升；升级后 `owned_schema_version == 4`） | 覆盖（v4 段本身是**单条** `ALTER TABLE`，不可半途失败；故障注入点在链的更早段，符合场景「或更早版本连续升级」） |
| R13 | v3 → v4 既有节点行写空清单 | `migration.rs::v3_database_upgrades_to_v4_by_appending_the_export_id_column_only` ①（现场造 v3 库：`ALTER TABLE owned_node DROP COLUMN export_ids_json` + 版本降 3；升级后逐列拼接断言 `"[]|…|pair|∅|…"` 其余列逐字不变）；真实 v2 夹具（`fixtures/storage/v2/empty.sqlite3` 内嵌 DDL 证实其 `owned_node` **存在且无清单列**）经 `v2_fixture_upgrades_to_v3_and_widens_only_the_audit_checks` 也走通 ALTER（该用例 `SqliteStore::open` 必须成功） | 覆盖 |
| R14 | 升级库与新建库 owned 列清单逐项相等 | 同 R13 用例 ②：`column_specs`（名称/顺序/类型/NOT NULL/默认值）对 `OWNED_TABLES`（18 张，含 `owned_node`）+ 6 张 `imported_*` 与新建库逐项相等，并断言 `export_ids_json` 是**最后一列**、`NOT NULL` + 默认 `'[]'`；v1 派生库侧由 `v1_fixture_upgrades_to_v3_and_preserves_rows` 的 `column_names` 覆盖（该夹具**无** `owned_node`，走「DDL 建当前形状、跳过 v4 段」分支，即守卫的 false 分支） | 覆盖（v2 夹具→v4 路径无 `column_specs` 断言；我按夹具内嵌 DDL 逐列比对，其列集合/顺序/类型与当前 DDL 常量减去新列后一致，故该路径也会相等——见 Assessment 的残余不确定项） |
| R15 | v2 → v3 升级保留审计并扩展词表（并继续升到 v4） | `migration.rs::v2_fixture_upgrades_to_v3_and_widens_only_the_audit_checks`（升级前断言 v2 CHECK 真的拒新词表；升级后 `owned_schema_version == 4`、`imported == 3`、`user_version == 4`、审计行与 `audit_id`/序列（7）保留、新动作与 `pairing_claimant` 可写、`owned_command.actor_kind` 保持三值） | 覆盖 |
| R16 | 需求「Export 过滤的 catalog 投影」（三条件 + 只收窄 + 批次稳定 + 全字段） | 代码 `catalog.rs:257-272`（`visible_exports` 三段过滤）+ R1–R5 的用例 + `the_catalog_snapshot_carries_every_field_of_the_visible_export_only` | 覆盖 |
| R17 | 需求「节点配对与信任方法」（`exportIds` 必填、三类失败、回显、不放宽） | 代码 `params.rs:748-770`（必填与归一化）、`router.rs:820-880`（透传与回显）、`trust.rs:1294-1320`（事务内校验）；用例见 R6–R9 + 本报告 §4 关切② | 覆盖 |
| R18 | 需求「版本常量与 migration 幂等」（4/4/3、只追加不重建、旧行置空、幂等、列清单相等） | 代码 `migrate.rs:17-22,214-235,611-621,945-993`；用例见 R11–R15；`rebuild_text_differs_from_the_fresh_text` 为「DDL 逐字不变」断言提供**判别力来源** | 覆盖 |

**缺口汇总**：无 CRITICAL/MAJOR 级缺口；两处「覆盖（部分）」是 **R9 的组合断言缺失**（F6，建议）与 **R14 的 v2 路径无断言**（仅人工核对，不阻断）。

## 3. Findings

Severity 依 `roles/reviewer.md`：CRITICAL / MAJOR / MINOR / SUGGESTION。**本轮无 CRITICAL、无 MAJOR。**

| ID | 目标版本位置 | 触发条件 | 预期 / 实际 | 影响 | 建议 | 级别 |
| --- | --- | --- | --- | --- | --- | --- |
| RV1-IMPL-F1 | `plan.md` Main E2E 的 `alternative_checks` 第 1 条 + `design.md` D9 末条（**规划文本**，无代码行） | 进入 [PV5]/7.2 证据核对时 | 预期：`import.add` 该 export 得到 `local.not_found`。实际：本切片 `import.add` **恒**返回 `local.unavailable`（`crates/server/src/local_admin/router.rs:437-443`，注释「无论参数是否合法都不校验」；证据 `router.rs:1914` 用例名 `import_add_is_always_unavailable_and_import_remove_maps_not_found`，且 `docs/LOCAL_ADMIN_PROTOCOL.md` §5.5 明确如此）。`crates/app/tests/node_link_e2e.rs` 全文没有 `import.` 步骤（`grep -n "import\."` 零命中），`[PV5]` 的命令（`cargo test -p server -p app`）也不可能产出该断言 | 7.2 的替代验证文字要求了一条**永远无法成立**的断言；若照抄「已验证」会形成无依据的证据复用 | 改 `plan.md` 的 `alternative_checks`/`design.md` D9 措辞（改为「Access 侧孤儿 Import 行为本次只文档化，不纳入 [PV5] 断言」），或在 7.2 明确该子项 `not-applicable` 并记依据；**规划文本由主 Agent 维护，本 reviewer 不改** | MINOR |
| RV1-IMPL-F2 | `crates/server/src/node_link/resource.rs:325-350`（`on_attach` 两步判定）+ `catalog.rs:283-305` | 对端发 `resource.attach`：`exportId` 可解析但本机 `owned_export` 无该行 | `design.md` D9 勘误写「① 本机不存在/未导出的 id → `nodelink.export.not_found`」。实际：`export_id(...)` 解析成功即进入第 ② 步 `export_is_visible`，而它对「本机无该 Export」返回 `Ok(false)`（`catalog.rs:283-305`，同理未配对节点/已撤销/不相交/不在清单）→ 一律映射 `nodelink.export.not_granted`。真正产生 `nodelink.export.not_found` 的只有：`ownerNodeId` 不是本机（`resource.rs:328-331`）或 id 不可解析（`resource.rs:333-340`） | 无验收场景依赖 `attach` 错误码（specs 未规定，`plan.md` 的 [PV5] 行也未点名），故不阻断；风险仅是勘误文字仍与实现不符，未来若有人按该句写用例会失败 | 把 D9 勘误的 ① 改为「非本机 ownerNodeId / 不可解析的 id → `nodelink.export.not_found`；`exportId` 合法但本机无该 Export 或可见性复核失败 → `nodelink.export.not_granted`」。**主 Agent 的裁决本身（新条件落在可见性复核、wire 词表不变）已核实成立**，见 §4 关切① | MINOR |
| RV1-IMPL-F3 | `crates/core/src/use_cases.rs:995-1026`（`node_link_session_access`）被 `node_link_replay`（`use_cases.rs:928-937`）、`node_link_replay_head`、`node_link_session_view` 复用；`crates/server/src/node_link/resource.rs` 的 `snapshot`/`replay` 只调这几个入口 | 同一 Access 节点**未撤销**时重新配对（`settle_pairing` → `approve_node` 是 upsert，见 `trust.rs:1369-1402`）并收窄清单，而该节点仍有活动连接与 attachment | 预期（D1「可见性保持单一判定点」的延伸读法）：收窄应立即生效。实际：**已建立 attachment 的后续快照/重放不复查清单**——`snapshot`/`replay` 只经 core 的归属前置（对端已配对 + Export 存在 + 会话属于该 Export），不含条件③，也不含条件② | 属**既有口径**（条件②在同一位置同样不复检，本变更未使其变差），且 `design.md` D8 把「修改清单」定为 `node.revoke` + 重新配对（撤销会推 `node.trust.revoked` 并以 4410 关闭连接，`command.rs:1561+`，即文档化路径不受影响）；命令、catalog 与 `resource.attach` 三处**均按当次持久记录实时判定**，故可观测的授权面没有被放宽 | 建议登记为「已知限制」并在 `NODE_LINK_PROTOCOL.md` §8.2/`design.md` D8 写明「清单变更对**既有** attachment 的事件流不重放」；是否需在重新配对时清空该节点的既有 attachment 属产品口径，**需主 Agent 裁决**，本轮不作为阻断项 | MINOR（不确定项：我未找到任何「重新配对即清空既有连接」的机制，若实际上有等价机制，请据此驳回本条） |
| RV1-IMPL-F4 | `crates/server/src/local_admin/test_support.rs:1428-1431`（`FakeTrust::revoke_node` 构造撤销记录时传 `Vec::new()`） vs `crates/storage-sqlite/src/admin/trust.rs:572-590`（真实 `revoke_node` 只 `SET state='revoked', revoked_at=…`，本 diff 亦未让 `upsert_node` 清列） | 任何经 `FakeTrust` 走 `node.revoke` 后再读该行的用例/断言 | 预期：替身与真实存储对同一操作给出同形结果。实际：替身把清单清空，真实存储保留 | 无用例被此差异掩盖（撤销后行不可见）；但这是替身保真度缺口：将来若加「撤销后 `node.list` 仍显示清单」的断言，替身会给错答案，而真实行为又无用例覆盖 | 让 `revoke_node` 传 `record.export_ids().to_vec()`（与 `advance_node_connected_at` 的既有写法一致，`test_support.rs:1716-1727`） | SUGGESTION |
| RV1-IMPL-F5 | `crates/server/src/node_link/catalog.rs:247-254`（`visible_exports` 文档注释） | 阅读该注释 | 预期：措辞与 `design.md` D1「清单只能收窄，不能放宽」一致。实际：写作「清单只能收窄**不掉宽**」 | 纯文案（同一句在 D1/§8.2 有正确表述），不影响判定 | 改为「不能放宽」（可与文档线收口时一并处理，不单开轮次） | SUGGESTION |
| RV1-IMPL-F6 | `crates/storage-sqlite/src/admin/trust.rs:866-881`（清单校验仅 `PairingTarget::Node` 分支）+ `crates/core/src/model/identity.rs:920-926`（`validate()` 不看该字段） | 若有人构造 `PairingTarget::Device` 且 `granted_export_ids` 非空的落定 | 预期：「不适用即无该字段」或显式拒绝。实际：字段被**静默忽略**（`approve_device` 不接收它） | **今日从 wire 不可达**：`device.pair.confirm` 用 `reject_unknown_fields(["pairingId","scopes"])`，`Authority::settle`（`identity-auth/src/pairing.rs:308`）只产出空清单，`settle_pairing` 的调用方只有 `router.rs:603`（设备）与 `router.rs:850`（节点，已附清单），故无生产路径；仅防御性硬化议题 | 可在节点分支之外加一条「设备配对的清单必须为空，否则 `InvalidRequest`」守卫；**是否值得加由主 Agent 决定** | SUGGESTION |
| RV1-IMPL-F7 | `crates/app/tests/node_link_e2e.rs:386-401`（受控路径的 `resource.attach on a narrowed export`） | 有人把条件③从 `export_is_visible` 摘掉后在 [PV5] 重跑 | 我按「真实拒绝原因」逐条推演：该步把 `sessionRef.exportId` 换成 `EXPORT_NARROWED`（`agentIds=[AGENT]`，scopes=GRANTS），若条件③缺失则第 ② 步通过、第 ③ 步 `node_link_session_access` 也通过（Export 存在未撤销、会话 Agent 属于它）→ `resource.attached` 会出现 → `client.expect("link.error")` 失败 | **断言有判别力**（我最初怀疑与「会话不属于该 Export」同码而无法区分，逐行核对后否定了该怀疑；`attach_fault` 对两种原因确实同为 `export.not_granted`，但后者在本用例中不会触发）。故不作为 finding，仅登记核对过程以免复核者重复怀疑 | 无需修改；若后续想加固，可另加一条「清单内 Export 的 attach 成功」作为正对照（现有 ⑤ 步已间接提供） | 无（登记项） |

## 4. 对 5 个「实现者自报风险 + 主 Agent 裁决」的核对

1. **attach 的错误码**：✅ 实现确实如此。新条件落在 `catalog.rs` 的 `is_nominated`（`catalog.rs:283-287`，由 `visible_exports` 第三段过滤调用），`resource.attach` 的第 ② 步「可见性复核」失败即 `ErrorCode::ExportNotGranted`（`resource.rs:343-352`）；`resource.rs` 的 diff **只改注释**（`git diff` 仅两行注释），wire 行为字面不变。本 diff 对 `compatibility/errors/v1/errors.json`、`docs/NODE_LINK_PROTOCOL.md`、`schemas/**`、`fixtures/**` **零改动**（已用 `git diff --name-only` 逐目录确认）。新条件**没有**被并进「不存在」分支（该分支是 `owner_matches` 与 id 解析，见 F2）。⚠️ 唯一瑕疵是 D9 勘误对第 ① 步的**描述**仍不准（F2，MINOR，不影响裁决成立）。
2. **`PairingSettlement` 形状**：✅ 核对通过。`Approved` 新增 `granted_export_ids`，`approved()` 签名保持，`with_granted_export_ids()` 归一化（排序+去重，`identity.rs:878-892`）。生产路径穷举：`settle_pairing` 的**全部**调用点是 `router.rs:603`（设备，`settle()` 产出空清单）与 `router.rs:850`（节点，紧接着 `.with_granted_export_ids(confirm.export_ids.clone())`）；`identity-auth` 的 `Authority::settle` 是唯一产出批准的地方且不含清单（`identity-auth/src/pairing.rs:275-312`），其调用点也只在各 router 配对方法内。因此**不存在「settle 一个节点配对却不附清单」的生产路径**。core 构造校验（去重/字典序/空集合合法，`normalized_export_ids`）与存储侧一致：`node_from_row`（`trust.rs:92-120`）把解码出的 Vec 交给 `NodeRecord::try_new` 做同一份归一化，`upsert_node` 直接取 `record.export_ids()`（`trust.rs:1394-1399`）不另写一套；`admin_store.rs` 用例用乱序+重复输入验证落盘文本为 `["export-one","export-two"]`。空集合固定 `'[]'`（`[]` 用例断言）。
3. **列序与迁移守卫**：✅ 四条路径逐条核对通过。
   - v1/v2 库（`fixtures/storage/v2/from-v1.sqlite3`、`fixtures/storage/v1/empty.sqlite3` 内嵌 DDL 显示**无** `owned_node`）→ `owned_node_pre_existing = false` → **跳过** v4 段，表由 DDL 常量按当前形状（含新列）建成；`v1_fixture_upgrades_to_v3_and_preserves_rows` 的 18 张 owned 表列清单断言覆盖此分支（若误跑 ALTER 会因重复列报错 → 用例必红）。
   - v2 夹具（**有** `owned_node`、无清单列）→ 守卫 true → `ALTER TABLE` 追加 → `v2_fixture_...` 用例必须升级成功。
   - 造出的 v3 库 → 同 v2 路径，且用 `column_specs` 断言「新列在末尾 + NOT NULL + 默认 `'[]'`」与新建库逐项相等。
   - 新建库 → `new_database = true` ⇒ 整个升级块被跳过，列由 DDL 常量给出。
   - `owned_node_pre_existing` 在第 947 行取值，**早于**第 949-950 行的两个 DDL 常量与任何 12-step 重建（`V4_UPGRADE_OWNED` 在 991 行执行）⇒ 不存在「v1/v2 库因 DDL 先建表而误判 pre-existing=false/true」的错序。
   - v4 段只有一条 `ALTER TABLE … ADD COLUMN`（`migrate.rs:619-621`），**无** 12-step 重建；v3 段已加 `if file_version < 3` 守卫（`migrate.rs:987-990`），且 `rebuild_text_differs_from_the_fresh_text` 为该守卫的判别力提供反证（重建文本 ≠ 新建文本）。DDL 里 `export_ids_json` 位于表级 `PRIMARY KEY`/`CHECK` **之前**、`revoke_reason` **之后**（`migrate.rs:226`），与 `ALTER TABLE` 的追加位一致。
4. **CLI 空清单警告的输出通道**：✅ 核对通过。无新增开关（只加 `#[arg(long = "export-id")] export_id: Vec<String>`，`cli.rs:246`，`Vec` 即可重复）；零次 → 空清单且**照常执行**（`pairing.rs:417-421` 打印 `empty_export_ids_notice` 后继续 `settle_claim`，用例 `an_empty_export_id_list_warns_without_blocking` 断言 `spy.methods() == [NodePairConfirm]` 且 `params["exportIds"] == []`）；警告走 `println!`（stdout），与「失败时 stderr 恰一行 JSON」不冲突——失败契约的严格版用例（`cli_commands.rs:22-31` 要求 stdout 恰一行）命中的是 `Supplied::parse`/`node.pair.begin` 失败（这两个都发生在打印认领信息与警告之前），多步仪式的失败路径用的正是较宽松的 `assert_failure_only`（`cli_commands.rs:34-37`）。`confirm_params` 对 `PairTarget::Node` **总是**插入 `exportIds`（含空数组，`pairing.rs:349-363`），设备方向不插入（同目录用例断言设备方向传了值也不进 params）。`print_result` 只打印 `result`，不新增 stdout 形态。
5. **`node list` 展示**：✅ 核对通过。`view::node`（`view.rs:192-199`）无条件输出 `exportIds`（空清单输出 `[]`，不省略），`node.list` 路由把 `NodeRecord` 逐个投影；`router.rs:2891` 断言 `listed["nodes"][0]["exportIds"] == []`；CLI 的 `node list` 走 `call_once` + `print_result`（`cli.rs:386`、`cli.rs:861-863`）原样打印 daemon 的 `result`，不做二次投影。

## 5. 主 Agent 关切项逐条结论

① **`visible_exports` 是唯一判定点**：✅（有一处**口径澄清**）。是唯一实现：catalog 投影（`project`）、`export_is_visible`（attach）、`session.list` 过滤、命令的 Export 归属（`command.rs:562/1038/1049`）全部调用同一个 `visible_exports`/`is_nominated`，代码中不存在第二份清单判定。但「事件/ACK 归属复核也复用它」在事实层面**不完全成立**：`snapshot`/`replay` 只走 core 的 `node_link_session_view`/`node_link_replay`（不含条件②也不含条件③）。这是**既有**结构（条件②同样不在那里），本变更未使一致性变差；见 F3（MINOR，建议文档化）。

② **core 的 Owner 侧判定与目录层同口径**：✅。`Broker::node_allowed`（`broker.rs:704-733`）在原有「未撤销 ∧ scopes∩trust.grants≠∅」之后新增 `trust.export_ids()` 成员判定，**空清单时恒 `continue`**，与 catalog 的空清单=空可见集同口径；强度关系可推导：core 已有 `trust.grants().contains(grant)` 且要求 `export.scopes().contains(grant)` ⇒ 条件②自动成立，再加条件③ ⇒ core 的判定是目录层的**超集约束**（不会出现「目录可见而命令不可用」以外的放宽）。核心断言有用例：`use_cases.rs::owner_side_node_authorization_requires_the_export_to_be_nominated` 的三分支（清单内→通过、清单外→`authorization.scope_denied`、空清单→`authorization.scope_denied`），且我核对了该用例的判别力（两个 Export 的 scopes 都与 grants 相交，差别只在清单与 agent ⇒ 失败只能归因条件③）。

③ **`settle_pairing` 同事务、四类失败零写入零审计**：✅。调用顺序为 `validate_export_ids`（`trust.rs:1294-1320`，逐项 `SELECT scopes_json, revoked_at FROM owned_export WHERE export_id=?`）→ `approve_node`（内部才 `bind_peer_key` + `upsert_node`）→ `UPDATE owned_pairing` → `insert_audit_rows` → `enforce_capacity_gate` → `tx.commit()`（`trust.rs:866-899`）；任何 `Err` 提前返回 ⇒ 事务对象被丢弃 ⇒ 整事务回滚。四类失败：不存在 → `NotFound(EntityRef::Export)`；已撤销 → 同一个 `NotFound`（撤销是终态，不复用旧记录）；与本次 `granted_grants` 不相交 → `InvalidRequest("granted export ids must intersect the granted grants")`；缺字段 → 在 `params::node_pair_confirm` 阶段以 `local.invalid_params` 拒绝（未触及存储）。`admin_store.rs::node_pairing_approval_rejects_invalid_export_ids_without_writing` 对三类存储失败逐项断言（信任行 0、配对仍 `pending_confirmation`、审计 0、`owned_node` 计数 0、既有配对行数不变），`router.rs` 用例断言缺字段时 `node_count == 0`。校验在**同一事务内**，无「先校验后写入」的两次调用窗口。

④ **`export.revoke` 不级联清理清单条目**：✅。`revoke_export` 只改 `owned_export`；`owned_node` 的写入点只有 `upsert_node`（配对确认）与两处 `UPDATE`（撤销节点 / 推进 `last_connected_at`），都不触碰清单列；`admin_store.rs` 用例在 `revoke_export` 后断言清单列逐字保留。

⑤ **无 `unwrap`/`expect`/无说明 panic、无新增依赖、无越界文件**：✅。逐文件统计新增行中的 `unwrap(`/`expect(`/`panic!`/`todo!`/`unimplemented!`：`broker.rs`/`identity.rs`/`params.rs`/`router.rs`/`view.rs`/`resource.rs`/`trust.rs`/`migrate.rs`/`cli.rs` 均为 **0**；`use_cases.rs`(8)/`catalog.rs`(3)/`cli/pairing.rs`(2) 的命中全部位于 `mod tests`（已逐个定位确认）。`Cargo.toml`/`Cargo.lock` 零改动 ⇒ 无新增依赖。`docs/**`、`schemas/**`、`fixtures/**`、`compatibility/**`、`openspec/**` 零改动。

⑥ **新用例真断言 / 无跳过 / 无弱化**：✅（含一处方法论确认）。新增/修改的用例都带具名断言或具名错误分类（不是「不 panic 即通过」）；`migrate.rs` 的 `rebuild_text_differs_from_the_fresh_text` 专门为「DDL 逐字不变」提供判别力反证；`v3_..._appending_the_export_id_column_only` 在造库阶段自证前提（断言 `owned_node` 真的没有清单列）；E2E 的 narrowed attach 断言经推演具备判别力（F7 登记项）。工作区测试中的 2 个 `#[ignore]` 都是**既有**的（crash-recovery 辅助 `commit.rs:1092`、夹具生成器 `migration.rs:970`，均在 base 提交即存在），本变更未新增跳过或弱化。主 Agent 的 `du1-pv1.log` 与 `env1-baseline.log` 的 `ignored` 计数同形。

## 6. E2E 用例审查（受控路径，[PV5]）

- **E2E ID / 需求映射**：`crates/app/tests/node_link_e2e.rs::the_controlled_path_runs_end_to_end_and_revocation_propagates` → 直接对应 R1/R2/R3/R4 的目录面（`EXPORT_VISIBLE` 唯一可见、`EXPORT_HIDDEN` 因不相交被滤、`EXPORT_NARROWED` 因不在清单被滤），并覆盖 R7 的 `confirm` 回显。
- **关键置数据**：三条 Export（`EXPORT_VISIBLE`/`EXPORT_NARROWED` 的 `scopes = GRANTS`、`EXPORT_HIDDEN` 的 `scopes = ["grant.approve"]`）+ workspace alias + agent profile；确认时 `exportIds = [EXPORT_VISIBLE]`。由于 `EXPORT_NARROWED` 的 scopes 与节点 grants 相交，其不出现**只能**由条件③解释 —— 这就是该用例对新语义的判别力所在（我在 §3 F7 里记录了「与 attach 第 ③ 步同码」的怀疑与逐行排除）。
- **真实入口**：真实 loopback listener（`OwnerNode::start`，`127.0.0.1:0`）、真实 SQLite（临时 data dir）、真实签名材料（`AccessKey` / `verify_owner_proof` / HMAC status proof）、真实 `LocalAdminRouter` 走 `node.pair.confirm`、真实 `server::node_link` 路由（无 mock 掉被测入口）；唯一的替身是「脚本化 Access 客户端」，与上一变更 `Main E2E = not-applicable` 的既定降级口径一致（Access 侧 `node-link-client` 尚未落地）。
- **正常/异常路径**：正常——清单内 Export 的 `catalog.subscribe`、`session.create`、`resource.attach`/`subscribe`/`event`/`ack`、`session.prompt` 终态、`export.revoke` 与 `node.revoke` 的传播；异常——清单外 Export 的 `catalog` 缺席（两条负断言）→ `resource.attach` 被拒（`nodelink.export.not_granted`）。
- **可观察断言**：catalog `ids == [EXPORT_VISIBLE]`、`!ids.contains(EXPORT_HIDDEN)`、`!ids.contains(EXPORT_NARROWED)`、`confirmed["exportIds"] == [EXPORT_VISIBLE]`、`refused["body"]["code"] == "nodelink.export.not_granted"`。均为外部可观察帧字段，不是内部状态。
- **是否以 mock 绕过被测入口**：否。
- **结论**：用例设计通过检视（无 CRITICAL/MAJOR）；**执行证据 PENDING**（[PV5] 的 `reports/pv5-windows-nodelink.log` 尚未产生；本轮的 workspace 轮次 [PV1] 已覆盖该用例的编译与执行，但按裁定 [PV5] 需在候选/主分支单独留证）。

## 7. Assessment

- **结论：PASS（对应 Target Revision `5190924d`）**。在只读、未继承实现对话的隔离条件下，我逐文件读完了 base→target 完整 diff，并按 5 个自报风险、6 项主 Agent 关切、R1–R18 覆盖索引逐条核对：**未发现 CRITICAL 或 MAJOR**；3 条 MINOR 与 3 条 SUGGESTION 均为非阻断项（其中 F1、F2 是**规划/设计文本**与实现不符，代码侧裁决成立；F3 是既有口径下建议登记的已知限制，需主 Agent 裁决是否文档化）。
- **不可据本报告宣称的事**：本轮结论**只覆盖** `5190924d` 的代码检视；**不代替** Project Verify，**不代替** [PV5] 与集成阶段 `npm run check`（`check:drift`/`check:agentic`/`check:docs`），也不代表可归档。
- **按检查 ID 的核对状态**：
  - [PV1]：**已核对（NEW，主 Agent 亲跑）**。`reports/du1-pv1.log`（修订 `5190924d`，2026-09-27 08:57:43）：`cargo fmt --all -- --check` exit 0；`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` exit 0；`cargo test --locked --workspace --all-features` — 85 个 `test result` 行、`passed` 求和 **989**、每行均 `0 failed`、末尾 `exit=0`，与我逐行统计一致。合同门禁按裁定在主检出跑，本轮**不在**该日志内。
  - [PV2]：**待返回**（`node scripts/check-crate-boundaries.mjs` 未在本轮证据内；从 diff 看无跨 crate 新依赖、模块内新增函数不改变依赖方向，风险低，但不能据此记 PASS）。
  - [PV3]/[PV4]：**已由 [PV1] 的 workspace 轮次间接执行并全绿**（`storage-sqlite`/`core`/`server`/`app` 的测试二进制都在 85 行汇总内，均为 `0 failed`），但**未按检查 ID 单独出证**；如需按 ID 的独立证据，请让检查执行者补跑 `cargo test --locked -p storage-sqlite -p core --all-features` 与 `-p server -p app --all-features` 并留日志。
  - [PV5]：**PENDING**（`reports/pv5-windows-nodelink.log` 未产生；且其 `alternative_checks` 第 1 条含一条本切片不可能成立的 `import.add → local.not_found` 断言，见 F1，需先修文字或明确 `not-applicable`）。
  - `check:drift`（契约漂移）：**待返回**。我做了人工预览：`docs/CORE_PORTS_AND_STORAGE.md` §7.3 的 `owned_node` DDL（文档 worktree `f47f9da`）与 `migrate.rs:214-235` 归一化（去注释/去 `IF NOT EXISTS`/压空白/转小写）后**逐条相同**，新列位置一致 ⇒ 集成后该门禁就 `owned_node` 一项应可通过；但门禁实际结果取决于两个分支是否都已并入集成树（**本分支不含 docs 改动**）。
  - `check:docs`/`check:agentic`：**待返回**（依赖含变更目录的权威树；与本分支的代码改动无直接关系）。
- **残余不确定项（已在 Findings 标注，不因此改变结论）**：F3 的触发路径（重新配对而非 `node.revoke`）我未找到「清空既有连接」的等价机制，但未穷举 server 连接注册表的全部路径；F2 的错误码分类为「更保守的码」，我确认没有任何 spec 场景依赖它；R14 的 v2 夹具路径由我人工比对夹具内嵌 DDL 而非断言。

### handoff_index

```yaml
handoff_index:
  - task_id: "2.1"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: 5190924d041dcc7ab9bd93b4541b215f25f26bea
    evidence_type: REVIEW
    evidence_id: RV1-IMPL
    report_path: openspec/changes/node-trust-export-ids/reports/rv1-impl.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "只读检视 storage-sqlite 面的实现（v4 迁移、owned_node 清单列读写、settle_pairing 事务内校验）与 5190924d 的完整 diff；未执行 cargo/npm，测试结论引用 [PV1] 的主 Agent 证据"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.2"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: 5190924d041dcc7ab9bd93b4541b215f25f26bea
    evidence_type: REVIEW
    evidence_id: RV1-IMPL
    report_path: openspec/changes/node-trust-export-ids/reports/rv1-impl.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "同上；覆盖 core 的 NodeRecord/PairingSettlement 形状与 Broker::node_allowed 的清单条件，并核对与目录层同口径"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.3"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: 5190924d041dcc7ab9bd93b4541b215f25f26bea
    evidence_type: REVIEW
    evidence_id: RV1-IMPL
    report_path: openspec/changes/node-trust-export-ids/reports/rv1-impl.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "同上；覆盖 visible_exports 第三条件、resource.attach 结论一致、local_admin 参数必填与透传、node.list 投影"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.4"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: 5190924d041dcc7ab9bd93b4541b215f25f26bea
    evidence_type: REVIEW
    evidence_id: RV1-IMPL
    report_path: openspec/changes/node-trust-export-ids/reports/rv1-impl.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "同上；覆盖 CLI --export-id 参数映射、空清单警告不阻断、confirm params 必带 exportIds、node list 原样展示"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.1 / 2.2 / 2.3 / 2.4"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: 5190924d041dcc7ab9bd93b4541b215f25f26bea
    evidence_type: CHECK
    evidence_id: PV1
    report_path: openspec/changes/node-trust-export-ids/reports/rv1-impl.md
    result: PASS
    evidence_status: REUSED
    applicability_basis: "复用主 Agent 在**同一** target_revision 上亲跑的 reports/du1-pv1.log，实施者为主 Agent 而非实现者；本角色只做逐项核对（85 个 test result 行、passed 求和 989、0 failed、exit=0），未重跑"
    source_evidence:
      id: PV1
      report_path: openspec/changes/node-trust-export-ids/reports/du1-pv1.log
      target_revision: 5190924d041dcc7ab9bd93b4541b215f25f26bea
  - task_id: "2.1 / 2.2 / 2.3 / 2.4"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: 5190924d041dcc7ab9bd93b4541b215f25f26bea
    evidence_type: CHECK
    evidence_id: PV2
    report_path: openspec/changes/node-trust-export-ids/reports/rv1-impl.md
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "node scripts/check-crate-boundaries.mjs 本轮未执行（本角色不跑 npm）；待检查执行者在集成树补跑"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.1 / 2.2 / 2.3 / 2.4"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: 5190924d041dcc7ab9bd93b4541b215f25f26bea
    evidence_type: CHECK
    evidence_id: PV5
    report_path: openspec/changes/node-trust-export-ids/reports/rv1-impl.md
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "reports/pv5-windows-nodelink.log 尚未产生；且其 alternative_checks 第 1 条含本切片不可能成立的 import.add → local.not_found 断言（RV1-IMPL-F1），需先修文字或标 not-applicable"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.1 / 2.2 / 2.3 / 2.4"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: 5190924d041dcc7ab9bd93b4541b215f25f26bea
    evidence_type: CHECK
    evidence_id: "check:drift / check:docs / check:agentic"
    report_path: openspec/changes/node-trust-export-ids/reports/rv1-impl.md
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "合同门禁按裁定在含完整变更目录的权威树、且两支（代码 docs）并入后跑；本分支不含 docs 改动。人工预览：§7.3 owned_node DDL 与 migrate.rs 归一化后逐条相同"
    source_evidence: NOT_APPLICABLE
```

### 返回给主 Agent 的摘要

- **结论**：**PASS**（Target `5190924d`；无 CRITICAL/MAJOR）。本轮只覆盖代码检视，不代表 Project Verify 或可归档。
- **Findings**：F1（MINOR，`plan.md`/`design.md` 的 `import.add → local.not_found` 断言在本切片不可能成立，需修文字或标 `not-applicable`）；F2（MINOR，D9 勘误对 `attach` 第 ① 步的描述仍与实现不符，新条件落点与 wire 词表不变已核实）；F3（MINOR，清单收窄对**既有 attachment** 的事件流不重放——既有口径，建议登记为已知限制，是否加机制需你裁决）；F4/F5/F6（SUGGESTION：替身保真、注释措辞、设备方向静默忽略清单位）。
- **R1–R18 缺口**：无阻断缺口；两处「覆盖（部分）」= R9 缺一条经真实 router+存储的 `local.not_found` 组合断言（F6 建议，可在 [PV5] 补一步）、R14 的 v2 夹具路径无 `column_specs` 断言（我已人工比对夹具 DDL，结论相等）。
- **报告路径**：`openspec/changes/node-trust-export-ids/reports/rv1-impl.md`。
- **我未做**：修改任何 worktree 代码/测试/规划/任务状态，跑 cargo/npm，执行 E2E，切分支或提交。
