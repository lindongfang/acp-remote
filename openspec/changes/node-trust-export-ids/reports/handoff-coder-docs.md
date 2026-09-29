<!-- WP5（tasks.md 2.5）文档与规范工作包的角色报告。字段与 handoff_index 的组织方式见 openspec/schemas/agentic/roles/handoff.md。 -->

# 角色报告：WP5 / task 2.5（同步权威文档 + 一处主规范 Purpose 勘误）

| 字段 | 值 |
| --- | --- |
| `task_id` | 2.5 |
| `role` | coder（实现 Agent） |
| `phase` | implement |
| `stage` | work-package |
| `agent_context` | 新建任务级最小上下文：只读本次变更的 proposal/design/specs/plan/tasks 与本仓库相关权威文档，不继承其它 WP 的实现对话；本次只做文档与规范，未触任何 `crates/**` 代码 |
| `workspace` | worktree `D:\Project\acp-remote-wt\export-ids-docs`；分支 `agentic/node-trust-export-ids-docs` |
| `base_revision` | `3cadb12d79456750e40644a2ea53c96380b700e6`（= `refs/heads/main`，verification.md 的 Target） |
| `target_revision` | `d6e01b3b3bbc71d4d6bd2edf34cc4dce3c0043fb`（三个提交：`c3f8058024f6144bd6aaddfcd0a368b3cdc5e274` → `e95c77836466a834420ff756ab9564fcd404386c` → `d6e01b3b3bbc71d4d6bd2edf34cc4dce3c0043fb`） |
| `scope` | `docs/NODE_LINK_PROTOCOL.md`、`docs/LOCAL_ADMIN_PROTOCOL.md`、`docs/CORE_PORTS_AND_STORAGE.md`、`openspec/specs/storage-schema-v2-migration/spec.md`（仅 Purpose）；报告目录 `reports/` |
| `result` | **BLOCKED**——`blocked_scope` 仅为 `[PV1]` 的 `check:drift` 子项（需 WP1 把同一列写进 `migrate.rs`，只能在集成阶段复跑）。文档改动本身已完成、已提交，`check:docs` 与其余适用门禁全部 PASS（见下） |
| `issues` | 无阻断项；两项范围判断与一项实现侧观察见「Issues」节 |
| `evidence_paths` | `reports/wp5-docs.log`（本 worktree 全部命令、退出码与关键输出）；本文件 `reports/handoff-coder-docs.md` |
| `resource_cleanup` | 临时目录联接 `node_modules` 与两个 `/tmp` 临时文件已删除并核实；worktree 无其它自建资源；工作区干净（`git status --porcelain` 为空） |

## 1. 输入版本（只读读取）

- 契约：`D:\Project\acp-remote\openspec\changes\node-trust-export-ids\` 的 `design.md`（D1–D10，唯一口径）、`proposal.md`、`specs/{node-link-owner-server,local-admin-methods,storage-schema-v2-migration}/spec.md`、`plan.md`（Coverage Index R1–R18）、`tasks.md`（2.5）、`verification.md`（Target / Check Plan Changes）。
- 规则：`AGENTS.md` §10（变更类型 → 权威文档映射）、§13（未实现能力不得写成已存在）。
- 实测基线：`node --version` = v24.19.0；`crates/storage-sqlite/src/migrate.rs` 在本 worktree 仍是**旧版**（`FILE_FORMAT_VERSION`/`OWNED_SCHEMA_VERSION`/`IMPORTED_SCHEMA_VERSION` = 3/3/3，`owned_node` 无新列）——这是 `check:drift` 必然红的原因，也是本报告把它记为 BLOCKED 的直接依据。

## 2. Changes：改动文件 ↔ 需求映射

| 文件 | 需求 | 覆盖索引 | 提交 |
| --- | --- | --- | --- |
| `docs/NODE_LINK_PROTOCOL.md`（文件头修订记录 + §8.2） | tasks.md 2.5 第 1 条；design D1/D5/D8 | R1–R5（可见性三条件、收窄、撤销不级联、wire 不变；R5 的批次切分未变） | `c3f80580` |
| `docs/LOCAL_ADMIN_PROTOCOL.md`（文件头修订记录 + §5.4） | tasks.md 2.5 第 2 条；design D4/D5/D8 | R6–R10（R6/R7/R8/R9 的参数与失败语义；R10 的 `mode=access` 不在本切片，原文未动） | `e95c7783` |
| `docs/CORE_PORTS_AND_STORAGE.md`（§3.5、§7 标题、§7.2、§7.3、§9 判据 1/28、§11.6 第 4 条、§11.7、§11.8） | tasks.md 2.5 第 3 条；design D2/D3/D4/D7/D8/D10 | R11–R15（R11/R12/R14/R15 的迁移面；R13 的旧行置空）；R6–R9 的落盘面 | `d6e01b3b` |
| `openspec/specs/storage-schema-v2-migration/spec.md`（仅 Purpose） | tasks.md 2.5 第 4 条 | R18（需求层文本随归档同步，本次只勘误 Purpose） | `d6e01b3b` |

未改动且已核实依据（design D5 / verification.md 的 Check Plan Changes）：`crates/**`、`schemas/**`、`fixtures/**`、`compatibility/commands/v1/commands.json`。

## 3. 逐处原值 → 新值摘要

1. `NODE_LINK_PROTOCOL.md` §8.2
   - 信任记录 ```text 块：`scopes` 之后新增 `exportIds       # ExportId 集合；本机 node.pair.confirm 填报，只能收窄可见 Export`。
   - 原句「首阶段**不使用**独立的 `exportIds` 维度（v1 的信任记录不落这一列）……将来要落地必须同时定义填报、撤销语义与迁移，并按 §2.3 的兼容流程处理」→ 改为四段：① 信任记录**带** `exportIds`，可见性 = **未撤销 ∧ `scopes ∩ grants ≠ ∅` ∧ `exportId ∈ exportIds`**，**只收窄不放宽**，空清单（含 `scopes` 与 `grants` 相交时）看不到任何 Export；② `export.revoke` **不**级联清理清单条目、重新上架得到新 id 不复活旧 id；③ 只在配对确认时填报，**没有配对后修改入口**（修改 = `node.revoke` + 重新配对，登记为已知限制），填报校验失败两类分别 `local.not_found`/`local.invalid_params`；④ **升级副作用**（v3 及更早的库升到 v4 后既有节点行清单为空 → 需本机重新确认才可见，连接与握手正常，**不得默认放权**）+ **wire 不加字段**（Access 仍按 catalog 可见集推导 `local.not_found`/`local.unavailable`；握手/catalog/resource/command 与 `fixtures/node-link/v1/` 不变，§2.3 无需启动）+ Access 侧孤儿 Import 本次只文档化。
   - 文件头新增一行修订记录（2026-09-27，`node-trust-export-ids` 变更，v1 内合同修订，未实现未发布）。
2. `LOCAL_ADMIN_PROTOCOL.md` §5.4
   - `NodeRecord` 文本块 `grants` 之后新增 `exportIds string[]`（说明去重/字典序、空数组含义、语义指向 §8.2）。
   - `node.pair.confirm`：`params` 由 `{ pairingId, grants: string[] }` → `{ pairingId, grants: string[], exportIds: string[] }`，写明**必填**、缺字段 → `local.invalid_params` 且不创建信任，并注明 envelope schema 只把 params/result 建模为通用对象、「必填由 Daemon 运行期强制」；`result` 由 `{ nodeId, grants, confirmedAt }` → `{ nodeId, grants, exportIds, confirmedAt }`。
   - 新增三条语义：只能收窄（三条件）；每个 id 必须存在且未撤销（`local.not_found`）/ `scopes` 与本次 `grants` 有交集（`local.invalid_params`），**三类失败都不创建信任、不推进配对状态、不写审计**，空清单合法；无配对后修改入口 + 撤销不级联 + 升级副作用指向 §8.2。
   - 文件头按该文件既有体例新增 `版本：1.4（2026-09-27，v1 内合同修订，未实现未发布：…先例 `node.challenge.catalogRevision`）`；§6 方法集/错误码表格未动。
3. `CORE_PORTS_AND_STORAGE.md`
   - §3.5：`NodeRecord` 行补 `exportIds` 与其收窄语义、去重/字典序/空集合合法；`PairingSettlement` 行 `Approved` 由 `{ granted_scopes, granted_grants }` → `{ granted_scopes, granted_grants, granted_export_ids: Vec<ExportId> }`（去重、字典序、空集合合法、校验在 `settle_pairing` 同一事务内）。
   - §7 标题：`` `storage-sqlite` v3 表结构 `` → `` `storage-sqlite` v4 表结构（`imported_*` 家族仍为 v3） ``。
   - §7.2：版本常量 `（当前 **v3 = 3**）`/`（当前都为 3）` → `（当前 **v4 = 4**）`/「`owned_schema_version`（当前 4）、`imported_schema_version`（当前 3，imported 家族本次未变）」；升级判据 `user_version < 3` → `< 4`、链路改为 `v1 → v2 → v3 → v4`、`= 4` 跳过、新建库建成 **v4** 形状；v1 → v2 段第 4 条的版本落盘口径改为「按当前常量统一写入（`'4'`/`'3'` 与 `4`）」；v2 → v3 段去掉「置版本为 `'3'`」改为指向下一条；**新增 v3 → v4 段**（`ALTER TABLE owned_node ADD COLUMN export_ids_json TEXT NOT NULL DEFAULT '[]'`，不走 12-step 重建，既有行得 `'[]'`、不得默认放权，列追加在末尾故升级库与新建库列清单逐项相等，随后把 `owned_schema_version`/`user_version` 置 4、`imported_schema_version` 保持 3）；迁移测试资产与回滚两条的版本链/「v3 库」改为 v4。
   - §7.3：`owned_node` 建表块**最后一列之后、表级 `PRIMARY KEY` 之前**新增
     `export_ids_json TEXT NOT NULL DEFAULT '[]',   -- LOCAL_ADMIN_PROTOCOL.md §5.4 的 exportIds[]（清单只收窄，不默认放权）`
     ——位置即 `ALTER TABLE ADD COLUMN` 的列序位置；语句内容与 design D3 给出的列定义逐字一致（漂移门禁忽略注释/空白后逐条比较）。
   - §9 判据 1/28：升级链写全到 v4；判据 28 末尾补两条断言——① 升级库与新建库的 **owned** 家族列清单（名/序/类型/`NOT NULL` 与默认值）逐项相等且 `export_ids_json` 在末尾，② v3 及更早的库升级后既有节点行 `export_ids_json = '[]'`（不得默认放权）、其余列逐列不变。
   - §11.6 第 4 条：补 `settle_pairing` 的清单校验（同一事务内、`NotFound(EntityRef::Export)` / `InvalidRequest`、空集合不查任何行、失败零写入）+ 清单只收窄不放宽 + **审计动作不新增**（沿用 `node.paired`）。
   - §11.7：集合字段那条说明句补 `owned_node.export_ids_json` 的索引与语义（只收窄、配对后无修改入口、撤销不级联、孤儿 Import 不自动清理）。
   - §11.8：新增「为什么 v3 → v4 只加列、不做 12-step 重建」（唯一理由是改 CHECK；`ALTER TABLE ADD COLUMN` 恰好追加在末尾；既有行置空、不得默认放权、升级副作用指向 §8.2）。
4. `openspec/specs/storage-schema-v2-migration/spec.md`：Purpose 由「v1 到 v2」→「从文件格式 v1 逐版升级到 v4（v1 → v2 → v3 → v4）」，Requirements/Scenario 正文未动（留给归档时由增量规范同步）。

## 4. Checks（[PV1] 拆分 + 反证）

全部命令在 worktree 根 `D:\Project\acp-remote-wt\export-ids-docs` 执行，完整输出与退出码见 `reports/wp5-docs.log`。

| 检查 | 命令 | 退出码 | 结果 | 说明 |
| --- | --- | --- | --- | --- |
| 运行时 | `node --version` | 0 | v24.19.0 | 与 CI/基线一致 |
| **[PV1] check:docs** | `node scripts/check-doc-links.mjs` | 0 | **PASS** | `380 relative links, 5090 section refs across 324 markdown files`（相对基线 5069 section refs 增加了本次新增引用，无悬空引用） |
| [PV1] check:drift | `node scripts/check-contract-drift.mjs` | 1 | **BLOCKED（PENDING）** | 预期失败：本 worktree 的 `crates/storage-sqlite/src/migrate.rs` 仍是旧版（`owned_node` 无新列），§7 块 1（`OWNED_SCHEMA_V1`）第 20 条语句不一致。已用与门禁同一套归一化的探针定位：29 条语句**仅此 1 条**不同，差异恰为新增的 `export_ids_json …`，且位于最后一列之后、表级约束之前（= `ALTER TABLE ADD COLUMN` 的列序位置）。PASS 需等 WP1（tasks.md 2.1）把同一列写进 `migrate.rs`，在集成/候选阶段复跑——本 WP 不得为让它变绿去改代码 |
| 反证（合同资产未动） | `node scripts/check-schema-fixtures.mjs` | 0 | PASS | `118 valid, 25 invalid (ajv Draft 2020-12), 39 event views bound` |
| 反证（封闭词表未动） | `node scripts/check-command-catalog.mjs` | 0 | PASS | `12 commands`（含 §6 方法集/错误码与 `local.*` 能力三处一致） |
| 反证（其余合同门禁） | `node scripts/check-error-registry.mjs`、`check-features.mjs`、`check-contract-assets.mjs`、`check-acp-compatibility.mjs` | 均 0 | PASS | 58 错误码 / 11 feature / 17 schemas+156 fixtures / ACP 矩阵 71 行——本次文档改动未影响任何合同资产 |

环境说明（必须按 `AGENTS.md` §8 如实记录）：本 worktree 无 `node_modules`，`check-schema-fixtures.mjs` 需要 `ajv`。为取得真实证据，临时把 `node_modules` 以目录联接（junction）指向主检出 `D:\Project\acp-remote\node_modules` 后运行，**检查结束已用 `rmdir` 删除**（主检出 `node_modules/ajv` 完好）。未执行 `npm run check`（本 worktree 无完整 npm 环境，且任务明确禁止）；`cargo *` 与 `git push`/分支切换均未执行（超出本 WP 范围与写入权限）。提交用 `git commit --no-verify` 跳过 husky 本地钩子（worktree 无 `node_modules`，钩子未装配），三条提交信息另以 `npx --no-install commitlint`（读 `commitlint.config.mjs`）预校验通过（退出码 0）。

## 5. Issues / 判断与观察

1. **`check:drift` 依赖 WP1**：如上一节，属预期的跨 WP 依赖，不是本 WP 的缺陷；集成阶段必须复跑并按 `verification.md` 的 `[PV1]` 行登记。
2. **`LOCAL_ADMIN_PROTOCOL.md` §5.8 未改**（范围判断）：proposal 的 Impact 提到 §5.4/§5.8，但 design D10（唯一口径）只要求 §5.4 + 修订记录；§5.8 是「CLI 子命令 ↔ 方法」映射表（`node pair` → `begin/status/confirm`），本次 `--export-id` 是 WP4 的参数细节、不改变该映射，故未动。若 reviewer 认为应在 §5.8 补一句 `--export-id` 说明，请主 Agent 决定后再改（避免自行扩大范围）。
3. **§11.7/§11.8 的落点判断**：这两节原本没有「孤儿 Import」「无修改入口」的句子，故按任务给的备选做法补**索引/说明句**，把展开语义留在主题归属处（`NODE_LINK_PROTOCOL.md` §8.2：可见性、撤销、无修改入口、升级副作用、Access 侧孤儿 Import 不自动清理），并在 §11.7 指回 §8.2、在 §11.8 用同款「为什么……」体例补 v4 的迁移理由。
4. **顺带的一致性更新**（同一版本链表述，不新增语义）：§7 标题 `v3` → `v4`、升级判据/迁移测试资产/回滚/§9 判据 1 与 28 中的 `v1 → v2 → v3` → `v1 → v2 → v3 → v4`。若不改，文档会在「当前文件格式版本 = 4」之下同时声称升级链止于 v3，属自相矛盾。
5. **实现侧观察（交主 Agent 转 WP1，不在本 WP 写入范围）**：`crates/storage-sqlite/src/migrate.rs` 现把 `V3_UPGRADE_OWNED`/`V3_UPGRADE_IMPORTED` 放在 `file_version < FILE_FORMAT_VERSION` 分支内**无条件**执行；一旦常量改为 4，v3 库也会走一遍 v2 → v3 的两张审计表 12-step 重建。合同现在写明「v3 库只走 v4 段」，因此 WP1 需要给 v3 段加 `file_version < 3` 守卫。本报告只作提示，未改任何代码。
6. **迁移夹具**：本次没有要求新增 `fixtures/storage/v3/`；文档只把 `fixtures/storage/v2/` 三件套的升级链表述更新为到 v4。若 WP1 选择新增 v3 夹具，请主 Agent 判定是否需要再补一句 §7.2 的夹具清单（同样属范围决策）。

## 6. Resource cleanup

- 删除临时目录联接 `D:\Project\acp-remote-wt\export-ids-docs\node_modules`（→ `D:\Project\acp-remote\node_modules`），删除后 worktree 内无 `node_modules`，主检出 `node_modules/ajv/package.json` 仍存在（已核实）。
- 删除临时探针脚本 `/tmp/drift-probe.mjs` 与临时输出 `/tmp/out.txt`（探针输出全文已留在 `reports/wp5-docs.log`）。
- 未创建数据库、容器、端口、证书或子进程；worktree 工作区干净（`git status --porcelain` 为空，无未提交/未暂存改动）；未改动主检出受版本控制文件。

## 7. handoff_index

```yaml
handoff_index:
  - task_id: "2.5"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "d6e01b3b3bbc71d4d6bd2edf34cc4dce3c0043fb"
    evidence_type: CHECK
    evidence_id: "PV1 (check:docs)"
    report_path: "reports/handoff-coder-docs.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "worktree D:\\Project\\acp-remote-wt\\export-ids-docs、base 3cadb12d、node v24.19.0 上执行 `node scripts/check-doc-links.mjs`，退出码 0（380 相对链接 / 5090 章节引用全部可解析）；本 WP 的全部改动均为 Markdown 文档，本门禁是该改动适用的直接判定，且它不依赖 crates/** 的当前形状"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "d6e01b3b3bbc71d4d6bd2edf34cc4dce3c0043fb"
    evidence_type: CHECK
    evidence_id: "PV1 (check:drift)"
    report_path: "reports/handoff-coder-docs.md"
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "同一修订上执行 `node scripts/check-contract-drift.mjs` → 退出码 1：§7 块 1（OWNED_SCHEMA_V1）第 20 条语句不一致。原因是本 worktree 的 crates/storage-sqlite/src/migrate.rs 仍是旧版（WP1/tasks.md 2.1 尚未把 export_ids_json 写入该常量），属预期跨 WP 依赖；探针已证明 29 条语句中仅此 1 条不同且差异恰为新列。PASS 需在 WP1 落地的集成/候选阶段复跑（任务 3.1 的 [PV1] 与 6.3）"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "d6e01b3b3bbc71d4d6bd2edf34cc4dce3c0043fb"
    evidence_type: CHECK
    evidence_id: "WP5-SUPP (合同资产反证)"
    report_path: "reports/handoff-coder-docs.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一修订上执行 `node scripts/check-schema-fixtures.mjs`、`check-command-catalog.mjs`、`check-error-registry.mjs`、`check-features.mjs`、`check-contract-assets.mjs`、`check-acp-compatibility.mjs`，退出码均为 0；本 WP 未改 schemas/**、fixtures/**、compatibility/**，这六道门禁是该结论的可机械复核反证（ajv 依赖经临时 node_modules 联接满足，检查后已移除）"
    source_evidence: NOT_APPLICABLE
```

---

## 8. fix 轮次（对 RV1-DOCS 的修复，WP5 / task 2.5）

本节是**追加**：上方「实现轮次」的内容与结论保持原样，不改动。本节记录独立 reviewer 报告
`reports/rv1-docs.md`（Review ID `RV1-DOCS`，结论 FAIL）中主 Agent 裁定为「本 WP 内必修」的问题。

| 字段 | 值 |
| --- | --- |
| `task_id` | 2.5 |
| `role` | coder（实现 Agent） |
| `phase` | fix |
| `stage` | work-package |
| `workspace` | worktree `D:\Project\acp-remote-wt\export-ids-docs`；分支 `agentic/node-trust-export-ids-docs` |
| `base_revision` | `3cadb12d79456750e40644a2ea53c96380b700e6`（= `refs/heads/main`） |
| 被修版本（RV1-DOCS 的 `target_revision`） | `d6e01b3b3bbc71d4d6bd2edf34cc4dce3c0043fb` |
| `target_revision`（本轮） | `426f8ae296d838e9843f0e9ca43a24680a2d60a9`（`c7660100` → `426f8ae2`） |
| `scope` | 只改 `docs/CORE_PORTS_AND_STORAGE.md`、`docs/NODE_LINK_PROTOCOL.md` 与 `reports/`；未触 `crates/**`、`schemas/**`、`fixtures/**`、`compatibility/**`、`plan.md`/`tasks.md`/`verification.md` |
| `result` | **BLOCKED**（`blocked_scope` 仅为 `[PV1]` 的 `check:drift` 子项，与上一轮同因：需 WP1 把同一列写进 `crates/storage-sqlite/src/migrate.rs`，只能在集成/候选阶段复跑）；修复本身已完成、已提交，`check:docs` PASS |
| `issues` | F1–F5 全部修复；F6（`plan.md` WP5 Write Scope 漏列一份 spec）归主 Agent 的规划文档，本轮未动；F4 的编号按主 Agent 裁定改动见下 |
| `evidence_paths` | `reports/wp5-docs-fix.log`（本轮全部命令、退出码与关键输出）；本文件 |
| `resource_cleanup` | 临时目录联接 `node_modules`（仅用于 commitlint 自查）已删除并核实；未创建其它临时资源；worktree 工作区干净 |

## 8.1 原问题 ID → 修复位置与改动摘要

| 问题 ID | 级别 | 修复位置（新版本 `426f8ae2`） | 改动摘要 |
| --- | --- | --- | --- |
| RV1-DOCS-F1（MAJOR） | 必修 | `docs/CORE_PORTS_AND_STORAGE.md` §6 第 5 条「授权调用点」（第 791 行） | Owner 侧 Export 判定由两条件改为三条件：要求目标 Export **在该节点信任记录 `exportIds` 清单内**（括注补「清单为空或目标 Export 不在清单内一律不放行」）；同条后半句的「只需前两条（信任 grant + 存在满足条件的 Export）」改为「只需「信任 grant + 存在满足条件的 Export」（此处的「满足条件」含上面新增的清单条件，即 `NODE_LINK_PROTOCOL.md` §8.2 的三条件）」 |
| RV1-DOCS-F2（MINOR） | 必修 | 同文件 §5.3 的单点可见性策略句（第 220 行） | 括注由「（D14：未撤销且 `export.scopes ∩ 节点 grants ≠ ∅`）」改为「（`D14`；2026-09-27 起为三条件：未撤销 ∧ `export.scopes ∩ 节点 grants ≠ ∅` ∧ `exportId ∈ 节点信任记录的 exportIds`，权威定义见 `NODE_LINK_PROTOCOL.md` §8.2 与本文件 §3.5）」，并在句尾补「core 的 Owner 侧授权判定（§6 第 5 条）必须与它同口径」——只留一处权威定义 + 指回链接 |
| RV1-DOCS-F3（MINOR） | 必修 | 同文件 §11.3 末条（第 1452 行） | 「过新」用例取值由 `user_version = 3` 改为 `user_version = 5`，即 `FILE_FORMAT_VERSION + 1`（与 §7.2、§9 判据 1 的临时副本口径一致；`fixtures/storage/v2/too-new.sqlite3` 的 `user_version = 3` 是**夹具**取值，未动） |
| RV1-DOCS-F4（MINOR） | 必修（编号经主 Agent 裁定） | 同文件 §9「验收判据」新增 **判据 32**（第 1359 行，排在现有判据 31 之后） | 主题「节点信任记录的 `exportIds` 清单」（来源 §5.4、§7.2、§7.3、§11.6 第 4 条、`NODE_LINK_PROTOCOL.md` §8.2），体例与 25–31 条一致，四条断言：① `settle_pairing` 在同一事务内校验清单（不存在/已撤销 → `NotFound(EntityRef::Export)`、与本次 `granted_grants` 无交集 → `InvalidRequest`、空集合合法且不查任何 Export 行；任一失败整事务回滚、零信任行、配对状态不推进、零审计）；② 节点行清单读写在去重 + 字典序归一化后往返一致，空集合固定落盘 `'[]'`；③ 可见性三条件同时成立才可见，空清单节点看不到任何 Export（即使 `scopes ∩ grants` 非空）；④ `export.revoke` 不级联清理清单条目。<br>**编号说明**：任务书写的「接在 30 之后，即 31」与现状冲突——31 已被 2026-09-24 `core-turn-view-fields` 变更的「§10.3 的 view 身份与版本」占用，且被文件头版本记录（第 17 行）与 §5.1（第 265 行）交叉引用。实施前已请示，主 Agent 裁定方案 (A)：**新判据追加为 32，既有 1–31 的编号与措辞一字不动** |
| RV1-DOCS-F5（SUGGESTION） | 必修（仅加半句） | `docs/NODE_LINK_PROTOCOL.md` 文件头 2026-09-26 修订记录（第 7 行） | 保留原历史记录一字不改，仅在其后追加半句「（其中 `exportIds` 推后一条已被 2026-09-27 的修订取代，见上一条）」 |
| 额外（自查发现的第三处过期复述） | 任务书第「逐条自查」条要求 | `docs/CORE_PORTS_AND_STORAGE.md` §10「未决项」的 `[已裁定]`（2026-09-26）Owner 侧授权条（第 1398 行） | 全文搜索「两个条件 / 两条件 / 未撤销且 `export.scopes` / exportIds 推后 / 前两条」后发现的第三处两条件复述（原文含「无会话命令只要求前两条」）。**不改写历史记录**，只在其末尾追加一句「本条是 2026-09-26 的裁定记录；2026-09-27 的 `node-trust-export-ids` 变更在该 Export 判定上增加清单条件——目标 Export 还须在该节点信任记录的 `exportIds` 内，当前口径以 §6 第 5 条与 `NODE_LINK_PROTOCOL.md` §8.2 为准」 |
| RV1-DOCS-F6（SUGGESTION） | **未修**（归主 Agent） | `plan.md` 的 WP5 Write Scope 列 | 规划工件由主 Agent 维护，本轮依任务书明确不动 `plan.md` |

其它扫描结果（未改，已核过不是过期复述）：`openspec/specs/node-link-owner-server/spec.md` 第 81 行的两条件可见性属**归档同步**项（RV1-DOCS §5.4 已列出，不在本 WP 写入范围）；`docs/CONFIG_REFERENCE.md` §9 的 `export_ids = ["exp-work"]` 是 Access 侧 Import 记录的管理明细字段（`import add` 的入参），与 Owner 侧信任记录的 `exportIds` 不是同一字段。

## 8.2 提交

| 提交 | 信息 | 文件 |
| --- | --- | --- |
| `c7660100` | `docs(storage): 补齐 Owner 侧授权与可见性的 exportIds 清单条件` | `docs/CORE_PORTS_AND_STORAGE.md`（F1/F2/F3/F4 + §10 追加句） |
| `426f8ae2` | `docs(node-link): 标注 exportIds 推后口径已被 2026-09-27 修订取代` | `docs/NODE_LINK_PROTOCOL.md`（F5） |

两条均用 `git commit --no-verify`（worktree 无 `node_modules`，husky 钩子未装配）；提交信息合规性用主检出
`node_modules/.bin/commitlint --from d6e01b3b --to HEAD`（读同一份 `commitlint.config.mjs`，md5 与主检出一致）
自查通过（exit 0）。新 HEAD = `426f8ae296d838e9843f0e9ca43a24680a2d60a9`，`git status --porcelain` 为空。

## 8.3 检查结果（本轮）

| 检查 | 命令 | 退出码 | 结果 | 说明 |
| --- | --- | --- | --- | --- |
| **[PV1] check:docs** | `node scripts/check-doc-links.mjs` | 0 | **PASS** | `380 relative links, 5101 section refs across 324 markdown files`（上一轮 5090，本轮新增 11 处引用全部可解析） |
| [PV1] check:drift | `node scripts/check-contract-drift.mjs` | 1 | **BLOCKED / PENDING** | 与上一轮**完全相同**的唯一差异：§7 块 1（`OWNED_SCHEMA_V1`）第 20 条语句（`owned_node`）——代码侧 `migrate.rs` 仍是 v3 旧形状。本轮未改 §7 的任何 SQL 块（diff 只含 §5.3/§6/§9/§10/§11.3 散文与 §9 判据 32），PASS 仍待 WP1（tasks 2.1）落地后在集成/候选阶段复跑（tasks 3.1、6.3） |
| §9 判据编号自检 | `awk '/^## 9\. 验收判据/,/^## 10\./' … \| grep -o "^[0-9]*\."` | 0 | 无重号 | `1…30 31 32`，31 仍是原 view 判据，32 为本次新增 |
| 提交信息自查 | `commitlint --from d6e01b3b --to HEAD`（临时 node_modules 联接） | 0 | PASS | 非 [PV1] 证据，仅为提交格式自查；联接已删除 |

未执行：`node scripts/check-schema-fixtures.mjs` 等其余合同门禁（本轮 diff 只有两个 Markdown 文件，上一轮已跑仍成立）、`cargo fmt/clippy/test`、`npm run check`/`verify`（跨 WP 或环境限制）、E2E（非适用）。

## 8.4 handoff_index（fix 轮次追加）

```yaml
handoff_index:
  - task_id: "2.5"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "426f8ae296d838e9843f0e9ca43a24680a2d60a9"
    evidence_type: CHECK
    evidence_id: "PV1 (check:docs)"
    report_path: "reports/handoff-coder-docs.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "对 RV1-DOCS（target d6e01b3b）的修复轮次。在 worktree D:\Project\acp-remote-wt\export-ids-docs、base 3cadb12d、node v24.19.0 上，于新 revision 426f8ae2 执行 `node scripts/check-doc-links.mjs`，退出码 0（380 相对链接 / 5101 章节引用全部可解析）。本轮改动全部是 Markdown 文档，本门禁是该改动适用的直接判定，且不依赖 crates/** 的当前形状；新增的 §8.2/§3.5/§5.4/§7.2/§7.3/§6 第 5 条引用均被它核对"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "426f8ae296d838e9843f0e9ca43a24680a2d60a9"
    evidence_type: CHECK
    evidence_id: "PV1 (check:drift)"
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "对 RV1-DOCS 的修复轮次。同一 revision 426f8ae2 上执行 `node scripts/check-contract-drift.mjs` → 退出码 1：唯一差异仍是 §7 块 1（OWNED_SCHEMA_V1）第 20 条语句（`owned_node`），因 worktree 的 crates/storage-sqlite/src/migrate.rs 仍是 v3 旧形状（WP1/tasks 2.1 未落地），与上一轮逐字相同；本轮未改 §7 的 SQL 块或 §5 的端口签名。执行记录见 `reports/wp5-docs-fix.log`；PASS 需在 WP1 落地后的集成/候选阶段复跑（tasks 3.1、6.3）"
    source_evidence: NOT_APPLICABLE
```

## 9. fix 轮次 2（RV2-DOCS 的新增发现）

### 9.1 输入与授权

独立复核报告 RV2-DOCS（`reports/rv2-docs.md`，target `426f8ae2`）：文档静态结论 **PASS**（RV1-DOCS 的
F1–F5 全部已解决，F6 由主 Agent 闭环），`BLOCKED` 仅因 `[PV1]` 的 `check:drift`（及同组 `check:agentic`）
在本 worktree 必然 PENDING。主 Agent 指派本 Agent 处理其两条**新增**发现：

- **RV2-DOCS-F1（MINOR，必修）**：`docs/CORE_PORTS_AND_STORAGE.md` 文件头缺本次变更的版本记录。
- **RV2-DOCS-F3（SUGGESTION，主 Agent 裁决：修）**：§9 判据 32 缺 core 侧 Owner 授权判定的核心断言。

（RV2-DOCS-F2 是 `verification.md` 第 38 行「新增 §9 判据 31」的笔误，该文件由主 Agent 维护，不在本
Agent 写入范围，故本轮未处理。）

### 9.2 修复位置与改动摘要

| 发现 | 位置 | 改动摘要 |
| --- | --- | --- |
| RV2-DOCS-F1 | `docs/CORE_PORTS_AND_STORAGE.md` 文件头（`> 版本：0.13` 行之后、`## 1.` 之前） | 在版本列表**末尾**追加一行 `> 版本：0.14（2026-09-27，`node-trust-export-ids` 变更：…）`，体例与 0.12/0.13 一致。内容覆盖本变更对该文件的全部改动：§3.5 `NodeRecord`/`PairingSettlement::Approved` 加清单字段、§5.3 与 §6 第 5 条的 Owner 侧授权判定改三条件（未撤销 ∧ `scopes ∩ grants ≠ ∅` ∧ `exportId ∈ exportIds`）、§7 升级到 v4（`owned_node` 末尾追加 `export_ids_json`，`ALTER TABLE ADD COLUMN`，既有行置空、不得默认放权）、§7.2 版本常量改 4/4/3、§9 新增判据 32、§11.6 第 4 条补清单校验与审计不新增、§11.7/§11.8 补集合列说明与「为什么只加列不重建」；并注明 §7 的 SQL 块与 §5 的 rust 块已同批更新、漂移门禁继续逐条成立。**既有版本行（0.3–0.13，含 0.9 在 0.8 之前的既有乱序）逐字未动** |
| RV2-DOCS-F3 | 同文件 §9 判据 32 | 在 ③ 之后插入新的 **④**（原 ④ 顺延为 **⑤**，其「`export.revoke` 不级联清理清单条目」措辞未动），并把 `§6 第 5 条` 加入该判据的来源列表。④ 的内容：core 的 **Owner 侧授权判定**（§6 第 5 条）必须与可见性**同口径**——清单为空、或目标 Export 不在该节点 `exportIds` 清单内时，即使 `export.scopes ∩ grants ≠ ∅`，该 Export 的会话命令也**不可用**（拒绝或失败关闭，不得降级放行），与 catalog 看不到它的结论一致。判据 32 现为 ①…⑤ 连续无重号；§9 的 1–31 条编号与措辞未动，其它小节未动 |

本轮只改 `docs/CORE_PORTS_AND_STORAGE.md`（`git diff --stat` = 1 file changed, 3 insertions(+), 1 deletion(-)）；
`crates/**`、`schemas/**`、`fixtures/**`、`compatibility/**`、其它 `docs/**` 与规划工件均未触碰。修订说明全文见
`reports/wp5-docs-fix.log` 的「第二轮 fix」小节。

### 9.3 提交

| 提交 | 信息 | 文件 |
| --- | --- | --- |
| `7d5abeb8` | `docs(storage): 补文件头 0.14 版本行并为判据 32 补 core 侧授权断言` | `docs/CORE_PORTS_AND_STORAGE.md` |

用 `git commit --no-verify`（worktree 无 `node_modules`，husky 钩子未装配）；提交信息合规性用主检出
`node_modules/.bin/commitlint --from 426f8ae2 --to HEAD`（读同一份 `commitlint.config.mjs`）自查通过（exit 0，
scope `storage` 在词表内），校验后临时目录联接已删除。**新 HEAD = `7d5abeb8c2372ccd77a1d35cc0a4f580bd648054`**，
`git status --porcelain` 为空。

### 9.4 检查结果（本轮）

| 检查 | 命令 | 退出码 | 结果 | 说明 |
| --- | --- | --- | --- | --- |
| **[PV1] check:docs** | `node scripts/check-doc-links.mjs` | 0 | **PASS** | `380 relative links, 5114 section refs across 324 markdown files`（上一轮 5101；本轮新增的 §3.5/§5.3/§6 第 5 条/§7/§7.2/§9/§11.6/§11.7/§11.8 引用全部可解析） |
| [PV1] check:drift | `node scripts/check-contract-drift.mjs` | 1 | **BLOCKED / PENDING** | 与上一轮**逐字相同**的唯一差异：§7 块 1（`OWNED_SCHEMA_V1`）第 20 条语句（`owned_node`）——代码侧 `migrate.rs` 仍是 v3 旧形状（WP1/tasks 2.1 未落地）。本轮未改 §7 的任何 SQL 块、§5 的端口签名或 §7.3 的 DDL，故未引入新漂移；PASS 仍待 WP1 落地后在集成/候选阶段复跑（tasks 3.1、6.3） |
| 自检 ①：§9 编号 | `awk '/^## 9\. 验收判据/,/^## 10\./' … \| grep -o "^[0-9]*\."` | 0 | 无重号缺号 | `1…31 32` 连续；判据 32 含 ① ② ③ ④ ⑤ 共 5 个子项（⑤ = 原 ④，措辞未动） |
| 自检 ②：文件头 | `git diff HEAD~1`（hunk `@@ -19,6 +19,8 @@`） | — | 只新增一行 | 该 hunk **只有插入、无删除**：既有 0.3–0.13 各行逐字未改 |
| 自检 ③：diff 范围 | `git diff --name-only HEAD~1` | — | 只含 1 个文件 | 仅 `docs/CORE_PORTS_AND_STORAGE.md` |
| 提交信息自查 | `commitlint --from 426f8ae2 --to HEAD`（临时 node_modules 联接） | 0 | PASS | 非 [PV1] 证据，仅为提交格式自查；联接已删除 |

未执行：需 `ajv` 的四道合同门禁、`cargo fmt/clippy/test`、`npm run check`/`verify`、E2E（跨 WP 或环境限制）；
`openspec/specs/node-link-owner-server/spec.md:81` 的归档同步与 `verification.md` 第 38 行的笔误均不在本 Agent
写入范围，未处理；未为 `check:drift` 失败做任何「修」（代码在另一分支 `D:\Project\acp-remote-wt\export-ids`）。

### 9.5 handoff_index（fix 轮次 2 追加）

```yaml
handoff_index:
  - task_id: "2.5"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "7d5abeb8c2372ccd77a1d35cc0a4f580bd648054"
    evidence_type: CHECK
    evidence_id: "PV1 (check:docs)"
    report_path: "reports/handoff-coder-docs.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "RV2-DOCS 的修复轮次（第二轮 fix，target 426f8ae2 → 7d5abeb8）。在 worktree D:\Project\acp-remote-wt\export-ids-docs、base 3cadb12d、node v24.19.0 上于新 revision 7d5abeb8 执行 `node scripts/check-doc-links.mjs`，退出码 0（380 相对链接 / 5114 章节引用全部可解析）。本轮改动全部是 Markdown 文档（文件头一行版本记录 + §9 判据 32 的一句断言文本），本门禁是该改动适用的直接判定，且不依赖 crates/** 的当前形状；新增的 §3.5/§5.3/§6 第 5 条/§7/§7.2/§9/§11.6/§11.7/§11.8 引用均由它核对。执行记录见 reports/wp5-docs-fix.log「第二轮 fix」小节"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "7d5abeb8c2372ccd77a1d35cc0a4f580bd648054"
    evidence_type: CHECK
    evidence_id: "PV1 (check:drift)"
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "RV2-DOCS 的修复轮次（第二轮 fix）。同一 revision 7d5abeb8 上执行 `node scripts/check-contract-drift.mjs` → 退出码 1：唯一差异仍是 §7 块 1（OWNED_SCHEMA_V1）第 20 条语句（`owned_node`），因 worktree 的 crates/storage-sqlite/src/migrate.rs 仍是 v3 旧形状（WP1/tasks 2.1 未落地），与上一轮逐字相同；本轮 diff 未触碰 §7 的任何 SQL 块、§5 的端口签名或 §7.3 的 DDL。执行记录见 reports/wp5-docs-fix.log；PASS 需在 WP1 落地后的集成/候选阶段复跑（tasks 3.1、6.3）"
    source_evidence: NOT_APPLICABLE
```

## 10. fix 轮次 3（RV3-DOCS 的新增发现）

### 10.1 输入与授权

独立复核报告 RV3-DOCS（`reports/rv3-docs.md`，target `7d5abeb8`）：文档静态结论 **PASS**（RV2-DOCS-F1/F3
均已解决、fix 轮次 2 未引入回归），`BLOCKED` 仅因 `[PV1]` 的 `check:drift`（及同组 `check:agentic`）在本
worktree 必然 PENDING。主 Agent 指派本 Agent 处理其两条**新增**发现，且为**范围受限**的第三轮（只改两处
文字，不扩大自查）：

- **RV3-DOCS-F1（MINOR，必修）**：`docs/CORE_PORTS_AND_STORAGE.md` 文件头 0.14 版本行自身两处陈述不实
  （① 写「§5.3 的单点可见性策略」而该策略只在本文件 §4；② 写「§5 的 rust 块已同批更新」而 §5 在整条
  变更内零改动、`crates/core/src/ports.rs` 也未改），且枚举漏记 §9 判据 1/28、§10、§11.3 三处。
- **RV3-DOCS-F2（SUGGESTION，主 Agent 裁决：修）**：§9 判据 32 来源列表的裸 `§5.4` 指向本文件「发布与
  基础设施」（`EventSink`/`Clock`/`IdGenerator`），而清单形状的权威是 `LOCAL_ADMIN_PROTOCOL.md` §5.4
  与本文件 §3.5。

### 10.2 修复位置与改动摘要

| 发现 | 位置 | 改动摘要 |
| --- | --- | --- |
| RV3-DOCS-F1 | `docs/CORE_PORTS_AND_STORAGE.md` 文件头第 22 行（`> 版本：0.14（…）`） | **整行替换**（保持 `>` 前缀与一行式体例，未拆行）：① `§5.3 的单点可见性策略` → `§4 的单点可见性策略`；② 三条件写成 `export.scopes ∩ grants`（与 §6 第 5 条、判据 32 ③ 同形）；③ `**§7 的 SQL 块与 §5 的 rust 块已同批更新**` → `**本次未改任何端口签名：§5 的 rust 块与 `crates/core/src/ports.rs` 均未变；§7 的 SQL 块已同批更新**`；④ 枚举补齐 `§9 新增判据 32 并同步判据 1/28 的版本链与 owned 列清单断言，§10 给历史裁定条目补当前口径提示，§11.3 的「过新」用例取值改为 5`。**既有 0.3–0.13 各行逐字未动**（含 0.9 在 0.8 之前的既有乱序） |
| RV3-DOCS-F2 | 同文件 §9 判据 32 首行的来源列表 | 旧 `（§5.4、§7.2、§7.3、§11.6 第 4 条、§6 第 5 条、`NODE_LINK_PROTOCOL.md` §8.2）` → 新 `（本文件 §3.5、§7.2、§7.3、§11.6 第 4 条、§6 第 5 条，`LOCAL_ADMIN_PROTOCOL.md` §5.4，`NODE_LINK_PROTOCOL.md` §8.2）`。判据正文 ①–⑤ 一字未动，改动只在该行的来源括注 |

本轮只改 `docs/CORE_PORTS_AND_STORAGE.md`（`git diff --stat` = 1 file changed, 2 insertions(+), 2 deletions(-)；
`git diff 7d5abeb8..HEAD --numstat` = `2 2`，两个 hunk `@@ -22 +22 @@` / `@@ -1361 +1361 @@`）；
`crates/**`、`schemas/**`、`fixtures/**`、`compatibility/**`、其它 `docs/**` 与规划工件均未触碰。修订说明全文
见 `reports/wp5-docs-fix.log` 的「第三轮 fix」小节。

### 10.3 提交

| 提交 | 信息 | 文件 |
| --- | --- | --- |
| `39be2f77` | `docs(storage): 校正 0.14 版本行节号与未改端口签名口径，并精确标注判据 32 来源` | `docs/CORE_PORTS_AND_STORAGE.md` |

用 `git commit --no-verify`（worktree 无 `node_modules`，husky 钩子未装配）；提交信息合规性用主检出
`node_modules/.bin/commitlint --from 7d5abeb8 --to HEAD`（读同一份 `commitlint.config.mjs`）自查通过
（exit 0，scope `storage` 在词表内），校验后临时目录联接已删除。**新 HEAD =
`39be2f7792b2ccfaa143b386d64fc731eedf2442`**，`git status --porcelain` 为空。

### 10.4 检查结果（本轮）

| 检查 | 命令 | 退出码 | 结果 | 说明 |
| --- | --- | --- | --- | --- |
| **[PV1] check:docs** | `node scripts/check-doc-links.mjs` | 0 | **PASS / NEW** | `380 relative links, 5117 section refs across 324 markdown files`（上一轮 5114；+3 来自文件头行新增的 §3.5/§4/§10/§11.3 等引用与判据 32 来源列表的 `LOCAL_ADMIN_PROTOCOL.md` §5.4 引用，全部可解析） |
| [PV1] check:drift | `node scripts/check-contract-drift.mjs` | 1 | **BLOCKED / PENDING** | 与上一轮**逐字相同**的唯一差异：§7 块 1（`OWNED_SCHEMA_V1`）第 20 条语句（`owned_node`）——代码侧 `migrate.rs` 仍是 v3 旧形状（WP1/tasks 2.1 未落地）。本轮未改 §7 的任何 SQL 块、§5 的端口签名或 §7.3 的 DDL，故未引入新漂移；PASS 仍待 WP1 落地后在集成/候选阶段复跑（tasks 3.1、6.3） |
| 自检 ①：范围 | `git diff --stat` / `--name-only` | 0 | 只含 1 个文件 | 1 file changed, 2 insertions(+), 2 deletions(-)，仅 `docs/CORE_PORTS_AND_STORAGE.md` |
| 自检 ②：两处改动 | `git diff --numstat/-U0 7d5abeb8..HEAD` | 0 | 只有两处行内整行替换 | `2 2`；2 个 hunk = 文件头 0.14 行（1 增 1 删）+ 判据 32 首行来源列表（1 增 1 删） |
| 自检 ③：§9 未受影响 | `git diff -U0 7d5abeb8..HEAD` 的 §9 命中行 + `awk` 提取判据编号 | 0 | 通过 | §9 只命中第 1361 行；判据编号 = `1…31 32`（连续、无重号无缺号）；判据 32 子项 = `① ② ③ ④ ⑤`（措辞未动）；判据 1（1327）/28（1357）正文行未出现在 diff |
| 提交信息自查 | `commitlint --from 7d5abeb8 --to HEAD`（临时 node_modules 联接） | 0 | PASS | 非 [PV1] 证据，仅为提交格式自查；联接已删除 |

未执行：`check:agentic`（无 `node_modules`）、需 `ajv` 的四道资产门禁（输入零改动，以 `git diff --name-only
3cadb12..HEAD -- schemas fixtures compatibility` = 空反证）、`cargo fmt/clippy/test`、`npm run check`/`verify`、
E2E（跨 WP 或环境限制，`plan.md` 记 `not-applicable`）；`openspec/specs/node-link-owner-server/spec.md:81` 的
归档同步与 `verification.md` 的表述均不在本 Agent 写入范围，未处理；未为 `check:drift` 失败做任何「修」
（代码在另一分支 `D:\Project\acp-remote-wt\export-ids`）。

### 10.5 handoff_index（fix 轮次 3 追加）

```yaml
handoff_index:
  - task_id: "2.5"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "39be2f7792b2ccfaa143b386d64fc731eedf2442"
    evidence_type: CHECK
    evidence_id: "PV1 (check:docs)"
    report_path: "reports/handoff-coder-docs.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "RV3-DOCS 的修复轮次（第三轮 fix，target 7d5abeb8 → 39be2f77）。在 worktree D:\Project\acp-remote-wt\export-ids-docs、base 3cadb12d、node v24.19.0 上于新 revision 39be2f77 执行 `node scripts/check-doc-links.mjs`，退出码 0（380 相对链接 / 5117 章节引用全部可解析；上一轮 5114）。本轮改动全部是 Markdown 文档的两行文字（文件头 0.14 版本行整行替换 + §9 判据 32 来源列表精确标注），本门禁是该改动适用的直接判定，且不依赖 crates/** 的当前形状；文件头新引用的 §3.5/§4/§10/§11.3 与判据 32 来源列表改写的 `LOCAL_ADMIN_PROTOCOL.md` §5.4 引用均由它核对。执行记录见 reports/wp5-docs-fix.log「第三轮 fix」小节"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "39be2f7792b2ccfaa143b386d64fc731eedf2442"
    evidence_type: CHECK
    evidence_id: "PV1 (check:drift)"
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "RV3-DOCS 的修复轮次（第三轮 fix）。同一 revision 39be2f77 上执行 `node scripts/check-contract-drift.mjs` → 退出码 1：唯一差异仍是 §7 块 1（OWNED_SCHEMA_V1）第 20 条语句（`owned_node`），因 worktree 的 crates/storage-sqlite/src/migrate.rs 仍是 v3 旧形状（WP1/tasks 2.1 未落地），与上一轮逐字相同；本轮 diff 只含文件头一行版本记录与判据 32 首行来源列表，未触碰 §7 的任何 SQL 块、§5 的端口签名或 §7.3 的 DDL。执行记录见 reports/wp5-docs-fix.log；PASS 需在 WP1 落地后的集成/候选阶段复跑（tasks 3.1、6.3）"
    source_evidence: NOT_APPLICABLE
```

## 11. fix 轮次 4（RV4-DOCS-F1 的新增发现）

### 11.1 输入与授权

独立复核报告 RV4-DOCS（`reports/rv4-docs.md`，target `39be2f77`）：被检 artifact
（`docs/CORE_PORTS_AND_STORAGE.md` @ `39be2f77`）结论 **PASS**，RV3-DOCS-F1/F2 均已解决、fix 轮次 3 未引入回归；
本轮唯一新增发现为 **RV4-DOCS-F1（MINOR，纯排版、非阻断）** —— 文件头 0.14 版本行全角括号 **3 开 / 4 闭**
（`7d5abeb8` 同行为 3/3，故属 fix 轮次 3 新引入）。主 Agent 指派第四轮 fix，**范围严格限定为一个字符**
（不扩大自查、不动其它行）：

- 复核报告 §4.2 给出的首选修法是**删掉「为什么只加列不重建」之后那个 `）`**（即恢复原有的外层括号嵌套，
  `7d5abeb8` 的行结构就是这样），并列出等价备选（保留该 `）`、删句末那个）。
- 任务书原文写「删除行尾那个 `）`」并「使该行以 `漂移门禁继续逐条成立。` 结束」——本 Agent 核对后确认这两条
  在**单字符删除**下不可能同时成立（该行本来就没有句末 `。`：删行尾 552 会以 `…逐条成立` 结束，删 465 则以
  `…逐条成立）` 结束），且与报告首推的位置不同。因此按流程**发起 need_decision** 请主 Agent 裁定，未自行选择。
- **主 Agent 2026-09-27 裁定：选 A** —— 删索引 465 的 `）`、保留行尾索引 552 的 `）`；依据是该文件既有版本行
  一律「一个外层括号、在行尾闭合」（0.4/0.5/0.7/0.10… 皆以 `）` 结束），原「以 `…逐条成立。` 结束」的表述作废；
  **明确排除备选 C**（把行尾 `）` 改成 `。`，会使外层括号提前闭合、与同文件其它版本行不一致）。

### 11.2 修复位置与改动摘要

| 发现 | 位置 | 改动摘要 |
| --- | --- | --- |
| RV4-DOCS-F1 | `docs/CORE_PORTS_AND_STORAGE.md` 文件头第 22 行（`> 版本：0.14（…）`） | **删 1 个字符**：删掉索引 **465** 的 `）`（紧接「为什么只加列不重建」之后），保留行尾索引 **552** 的 `）`。行内文本 `…不重建」）。**本次未改` → `…不重建」。**本次未改` |

括号计数（`node -e` 只读统计第 22 行）：改前 `（` @9/169/240、`）` @229/314/465/552，**开 3 / 闭 4**，行长 554；
改后**开 3 / 闭 3**，行长 553。该行现在以 `…§7 的 SQL 块已同批更新**，漂移门禁继续逐条成立）` 结束，与
0.12/0.13 等既有版本行「一个外层括号、行尾闭合」的体例一致。**零语义改动**：措辞、0.3–0.13 既有版本行
（含 0.9/0.8 的既有乱序）、其它小节均未触碰。

`git diff --numstat 39be2f77..HEAD` = `1 1 docs/CORE_PORTS_AND_STORAGE.md`；`git diff 39be2f77..HEAD |
rg '^[+-][^+-]'` 只有这一行的一增一删。修订说明全文见 `reports/wp5-docs-fix.log` 的「第四轮 fix」小节。

### 11.3 提交

| 提交 | 信息 | 文件 |
| --- | --- | --- |
| `f47f9da` | `docs(storage): 修正 0.14 版本行多余的全角右括号，恢复既有嵌套体例` | `docs/CORE_PORTS_AND_STORAGE.md` |

用 `git commit --no-verify`（worktree 无 `node_modules`，husky 钩子未装配）；提交信息合规性用主检出
`node_modules/.bin/commitlint --from 39be2f77 --to HEAD`（读同一份 `commitlint.config.mjs`）自查通过
（exit 0，scope `storage` 在词表内），校验后临时目录联接已删除。**新 HEAD =
`f47f9dadd8189725c9217c3d6fd0e337fc96b547`**（父提交 `39be2f77`），`git status --porcelain` 为空。

### 11.4 检查结果（本轮）

| 检查 | 命令 / 工作目录 | 退出码 | 结果 |
| --- | --- | --- | --- |
| **[PV1] check:docs（文档 worktree）** | `node scripts/check-doc-links.mjs`，cwd = `D:\Project\acp-remote-wt\export-ids-docs` | **0** | **PASS**：`380 relative links, 5117 section refs across 324 markdown files`（与本轮改动前逐字相同，纯标点改动不影响引用解析） |
| check:docs（主检出，只读复核） | `node scripts/check-doc-links.mjs`，cwd = `D:\Project\acp-remote` | **0** | **PASS**：`381 relative links, 7006 section refs across 350 markdown files`。与 RV4 记录不同——RV4 在此为 exit 1（`verification.md:70` 的 §3.5 归因错误，OOS-1/RV4-DOCS-F2），该行已由主 Agent 修好；本轮未触碰 `verification.md` |
| 自检 ①：括号计数 | `node -e` 打印第 22 行 | 0 | 开 3 / 闭 3（改前 3/4，行长 554 → 553） |
| 自检 ②：范围 | `git diff --numstat 39be2f77..HEAD` | 0 | 只有 `docs/CORE_PORTS_AND_STORAGE.md`，`1 1` |
| 自检 ③：只有一行 | `git diff 39be2f77..HEAD \| rg '^[+-][^+-]'` | 0 | 只有 0.14 版本行的一增一删 |
| 提交信息自查 | `commitlint --from 39be2f77 --to HEAD`（临时 node_modules 联接） | 0 | PASS（非 [PV1] 证据） |

未执行：`check:drift`（跨 WP 已知 PENDING，代码在另一 worktree；本轮未触碰 §5/§7 的 rust/SQL 块）、
`check:agentic` 与需 `ajv` 的四道资产门禁（worktree 无 `node_modules`，且本轮只改一个全角括号）、
`cargo fmt/clippy/test`、`npm run check`/`verify`、E2E（`plan.md` 记 `not-applicable`）。

### 11.5 handoff_index（fix 轮次 4 追加）

```yaml
handoff_index:
  - task_id: "2.5"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "f47f9dadd8189725c9217c3d6fd0e337fc96b547"
    evidence_type: CHECK
    evidence_id: "PV1 (check:docs)"
    report_path: "reports/handoff-coder-docs.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "RV4-DOCS-F1 的修复轮次（第四轮 fix，target 39be2f77 → f47f9da）。在 worktree D:\Project\acp-remote-wt\export-ids-docs、base 3cadb12d、node v24.19.0 上于新 revision f47f9da 执行 `node scripts/check-doc-links.mjs`，退出码 0（380 相对链接 / 5117 章节引用全部可解析，与改动前逐字相同）。本轮改动是 Markdown 文档第 22 行的单个全角右括号删除（无新增/改写引用），本门禁是该改动唯一适用的直接判定；同一命令在含权威 openspec/changes/** 的主检出（D:\Project\acp-remote）复跑亦为退出码 0。执行记录见 reports/wp5-docs-fix.log「第四轮 fix」小节"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "f47f9dadd8189725c9217c3d6fd0e337fc96b547"
    evidence_type: CHECK
    evidence_id: "PV1 (check:drift)"
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "RV4-DOCS-F1 的修复轮次（第四轮 fix，同 revision f47f9da）。本轮未执行也不适用：改动只是文件头版本行的标点（删除 1 个全角右括号），§5 的 rust 块、§7/§7.3 的 SQL 块与 §9 判据均未触碰（git diff 39be2f77..HEAD 只含该行），因此不改变任何一条漂移判据。该门禁在 worktree 的 PENDING 状态（代码在另一 worktree D:\Project\acp-remote-wt\export-ids，migrate.rs 仍为 v3 旧形状）与 RV3/RV4 记录逐字相同，PASS 仍待 WP1 落地后在集成/候选阶段复跑（tasks 3.1、6.3）"
    source_evidence: NOT_APPLICABLE
```

## 12. fix 轮次 5（RV2-IMPL-F1 的文档面）

### 12.1 输入与授权

独立复核报告 RV2-IMPL（`reports/rv2-impl.md`，target 实现侧 `e4c980c0`）的 **RV2-IMPL-F1（MINOR）**：`node.pair.confirm` 不调用
`close_node`（当时全代码库仅 `node.revoke` 一处），而 `storage-sqlite::approve_node` 对已 `paired` 行没有状态守卫、
`claim_pairing` 没有「该对端已配对」守卫 ⇒ **不撤销、直接重新配对**即可改写 `exportIds` 清单，而既有连接与已建立的
attachment 继续收到该 Export 的 `resource.event`（实时扇出只守「已配对 ∧ Export 未撤销 ∧ 会话属于该 Export」）。

主 Agent 已实测确认并裁定**修，而非仅文档化**：代码侧（另一 Agent，run `3c6af73b`，不在本 worktree）在
`node.pair.confirm` 提交信任行之后复用 `node.revoke` 的**同一机制**（`deps.pairing.close_node(&node_id)`）关闭该节点现有
的活动连接，并**明确否决**在 core 读 seam 复制条件 ③（那会造成第三处判定点）。本 WP（task 2.5，run `3d79c3cb`）只做
**文档面**：把「重新配对在落定后关连接、收窄自下一次握手生效」写进 `docs/NODE_LINK_PROTOCOL.md` §8.2，并消除
「改清单 = `node.revoke` + 重新配对」这一与实现不符的表述。

范围严格限定为 `docs/NODE_LINK_PROTOCOL.md`；`crates/**`、`schemas/**`、`fixtures/**`、`compatibility/**`、其它
`docs/**`（含 `LOCAL_ADMIN_PROTOCOL.md`、`CORE_PORTS_AND_STORAGE.md`）与规划工件（`plan.md`/`tasks.md`/`design.md`/
`verification.md`）均未触碰。

### 12.2 修复位置与改动摘要

| 发现 | 位置（新版本 `0d0afc3`） | 改动摘要 |
| --- | --- | --- |
| RV2-IMPL-F1（MINOR，文档面） | `docs/NODE_LINK_PROTOCOL.md` §8.2 第 278 行 | 原句「没有『配对后修改清单』的入口，**修改 = `node.revoke` + 重新配对**，这是本切片的**已知限制**」→「没有『配对后修改清单』的入口，**改清单只能重新配对——要么先 `node.revoke` 再重新配对，要么直接重新配对**（同一 `nodeId` 再次经 `node.pair.begin`/claim/`node.pair.confirm` 落定，含密钥变化后的重新配对），这是本切片的**已知限制**」。原文把 revoke 写成改清单的必需前置，与 `approve_node`/`claim_pairing` 无守卫、可直接重配的事实不符 |
| 同上（新增语义句） | 同文件 §8.2 第 280 行（新增段落「**收窄在落定后于连接边界强制**」） | `node.pair.confirm` 提交信任行后**立即关闭该节点现有的 active connection**（与 `node.revoke` 用同一把「落定后关连接」的机制）⇒ 收窄在**下一次握手后的新连接**上生效；新连接上的 `catalog.snapshot`、`resource.attach` 与 Owner 侧命令授权都按已提交的信任行重算（§12.3、§12.4），Access 侧对 `state = paired` 的记录会自动重连并重取 catalog（§15）；因此**不需要**在 core 的会话读面上再复制一遍可见性判定——那条读面只硬校验「对端是已配对的 `access` 行 ∧ Export 未撤销 ∧ 会话属于该 Export」，三条件仍只是适配器的单点策略（`CORE_PORTS_AND_STORAGE.md` §4） |
| 文件头修订记录 | 同文件第 5 行（**既有** 2026-09-27 那条） | 未新增第二条同日行；在该行末尾追加半句「……消息形状与错误码登记均未动；**并写明重新配对（同一 `nodeId` 再次经 `node.pair.begin`/claim/`node.pair.confirm` 落定，含密钥变化后的重新配对）在落定后立即关闭该节点的活动连接，收窄因此自下一次握手后的新连接生效**。」保持一行式体例，0.3–0.13 的既有版本行与其它修订记录逐字未动 |

矛盾表述扫描（grep `收窄`/`配对后修改`/`无修改入口`/`立即生效`/`活动连接`/`可见性`）：本文件内**只有第 278 行**与
新语义冲突，已按上表修掉；第 274 行「`catalog.snapshot` 与 `resource.attach` 复用同一份判定」、第 518 行
`export.revoked`「立即生效」（条件①的撤销**通知**，另一条机制）、第 625 行「必须同时存在于该 Access 可见的 Export」、
第 926 行 §15「撤销即停」（只讲 `node.revoke`/`node.trust.revoked`）都与新段落一致，未改。

`git diff --numstat f47f9da..HEAD` = `4 2 docs/NODE_LINK_PROTOCOL.md`（`--name-only` 只有这一个文件）；
改动共 3 处行内容（文件头一行 + §8.2 一行整行替换 + 一个新增段落含其空行）。修订说明全文见
`reports/wp5-docs-fix.log` 的「第五轮 fix」小节。

### 12.3 提交

| 提交 | 信息 | 文件 |
| --- | --- | --- |
| `0d0afc3` | `docs(node-link): 写明重新配对在落定后关闭活动连接，收窄自下一次握手生效` | `docs/NODE_LINK_PROTOCOL.md` |

用 `git commit --no-verify`（worktree 无 `node_modules`，husky 钩子未装配）；提交信息合规性用主检出
`node_modules/.bin/commitlint --from f47f9da --to HEAD`（读同一份 `commitlint.config.mjs`）自查通过（exit 0，
scope `node-link` 在词表内），校验后临时目录联接已删除。**新 HEAD =
`0d0afc3b73f367594bdba02942493485ca7a2135`**（父提交 `f47f9da`），`git status --porcelain` 为空。

### 12.4 检查结果（本轮）

| 检查 | 命令 / 工作目录 | 退出码 | 结果 |
| --- | --- | --- | --- |
| **[PV1] check:docs（文档 worktree）** | `node scripts/check-doc-links.mjs`，cwd = `D:\Project\acp-remote-wt\export-ids-docs` | **0** | **PASS**：`380 relative links, 5121 section refs across 324 markdown files`（上一轮 5117；新增的 §12.3/§12.4/§15/`CORE_PORTS_AND_STORAGE.md` §4 引用全部可解析） |
| check:docs（主检出，只读复核） | `node scripts/check-doc-links.mjs`，cwd = `D:\Project\acp-remote` | **0** | **PASS**：`381 relative links, 7052 section refs across 353 markdown files` |
| 自检 ①：范围 | `git diff --numstat f47f9da..HEAD` | 0 | 只有 `docs/NODE_LINK_PROTOCOL.md`，`4 2` |
| 自检 ②：文件头体例 | `sed -n '1,8p'` | 0 | 无第二条 2026-09-27 行；既有行只在其末尾追加半句，0.3–0.13 版本行与其它修订记录未动 |
| 提交信息自查 | `commitlint --from f47f9da --to HEAD`（临时 node_modules 联接） | 0 | PASS（非 [PV1] 证据） |

未执行 / 不适用：`check:drift`（跨 WP 已知 PENDING——代码在另一 worktree `D:\Project\acp-remote-wt\export-ids`，
`migrate.rs` 仍为 v3 旧形状；本轮未触碰 §5 的 rust 块与 §7 的 SQL 块，不改变任何漂移判据）、`check:agentic` 与需
`ajv` 的四道资产门禁（worktree 无 `node_modules`，本轮只改一个 Markdown 文件）、`cargo fmt/clippy/test`、
`npm run check`/`verify`、E2E（跨 WP 或环境限制，`plan.md` 记 `not-applicable`）。

### 12.5 handoff_index（fix 轮次 5 追加）

```yaml
handoff_index:
  - task_id: "2.5"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "0d0afc3b73f367594bdba02942493485ca7a2135"
    evidence_type: CHECK
    evidence_id: "PV1 (check:docs)"
    report_path: "reports/handoff-coder-docs.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "RV2-IMPL-F1 的文档面（第五轮 fix，target f47f9da → 0d0afc3）。在 worktree D:\Project\acp-remote-wt\export-ids-docs、base 3cadb12d、node v24.19.0 上于新 revision 0d0afc3 执行 `node scripts/check-doc-links.mjs`，退出码 0（380 相对链接 / 5121 章节引用全部可解析；上一轮 5117）。本轮改动全部是 Markdown 文档（文件头既有 2026-09-27 行追加半句 + §8.2 一行整行替换 + §8.2 一个新增段落），本门禁是该改动适用的直接判定，且不依赖 crates/** 的当前形状；新增的 §12.3/§12.4/§15 与 `CORE_PORTS_AND_STORAGE.md` §4 引用均由它核对。同一命令在含权威 openspec/changes/** 的主检出（D:\Project\acp-remote）复跑亦为退出码 0（381 相对链接 / 7052 章节引用）。执行记录见 reports/wp5-docs-fix.log「第五轮 fix」小节"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "0d0afc3b73f367594bdba02942493485ca7a2135"
    evidence_type: CHECK
    evidence_id: "PV1 (check:drift)"
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "RV2-IMPL-F1 的文档面（第五轮 fix，同 revision 0d0afc3）。本轮未执行也不适用：改动只在 docs/NODE_LINK_PROTOCOL.md 的 §8.2 与文件头散文，§5 的 rust 块、§7/§7.3 的 SQL 块与 §9 判据均未触碰（git diff f47f9da..HEAD 只含该文件），因此不改变任何一条漂移判据。该门禁在 worktree 的 PENDING 状态（代码在另一 worktree D:\Project\acp-remote-wt\export-ids，migrate.rs 仍为 v3 旧形状、export_ids_json 未写入）与 RV2/RV3/RV4 记录逐字相同，PASS 仍待 WP1 落地后在集成/候选阶段复跑（tasks 3.1、6.3）"
    source_evidence: NOT_APPLICABLE
```

### 12.6 交主 Agent 的开放项（不在本 WP 写入范围）

1. `docs/LOCAL_ADMIN_PROTOCOL.md` 第 385 行（§5.4）仍是「清单只在确认时填报，没有配对后修改入口：**修改 = `node.revoke` + 重新配对**（已知限制）」，同文件第 597 行（§7 撤销与关闭顺序）只列 `device.revoke`/`node.revoke`/`export.revoke`、未含 `node.pair.confirm`。本 WP 被明确要求不动该文件，故未改。**（已在下方「fix 轮次 6」（§13）关闭：该文件已按主 Agent 指派的受限范围同步。）**
2. `design.md` D8「已知限制：修改 = `node.revoke` + 重新配对」与 `plan.md`/`tasks.md`/`verification.md` 中同一措辞（RV2-IMPL-F1 已点名的位置）属规划工件，由主 Agent 维护。
3. `openspec/specs/node-link-owner-server/spec.md` 第 81 行的两条件可见性属归档同步项（既有 OOS，未变）。

## 13. fix 轮次 6（把 §8.2 的新语义同步到 `LOCAL_ADMIN_PROTOCOL.md`）

### 13.1 输入与授权

触发：**§12.6 开放项 1**（本轮由主 Agent 指派处理，该开放项因此关闭）。第五轮（`0d0afc3`）已在
`docs/NODE_LINK_PROTOCOL.md` §8.2 落定新语义——**重新配对**（同一 `nodeId` 再次经 `node.pair.begin`/claim/
`node.pair.confirm` 落定，含密钥变化后的重新配对）在**落定后立即关闭该节点的活动连接**，因此 `exportIds`
的收窄自**下一次握手后的新连接**生效；「改清单 = `node.revoke` + 重新配对」是**过时口径**（revoke 不是
改清单的必需前置）。但 `docs/LOCAL_ADMIN_PROTOCOL.md` 仍是旧口径，两份权威文档因此不一致。

授权范围极小：**只改 `docs/LOCAL_ADMIN_PROTOCOL.md`**；不改代码（`crates/**`）、不改协议资产
（`schemas/**`、`fixtures/**`、`compatibility/**`）、不改其它 `docs/**`、不改规划工件
（`proposal.md`/`design.md`/`plan.md`/`tasks.md`/`verification.md`）。报告目录 `reports/` 追加本文件与
`wp5-docs-fix.log`。

### 13.2 修复位置与改动摘要（新版本 `fd59736`）

| 位置 | 旧句 → 新句摘要 |
| --- | --- |
| 第 4 行（文件头**既有** `版本：1.4（2026-09-27…）`） | 在该行末尾闭合 `）` 之前追加半句「**；并同步「改清单 = 重新配对（落定后关闭活动连接，收窄自下一次握手生效）」**」。未新增第二条 1.4 行或新版本号；`版本：1.3/1.2/1.1` 与 `日期`/`上位文档` 等头行逐字未动 |
| 第 385 行（§5.4 `node.pair.confirm` 清单说明） | 旧：「清单只在确认时填报，没有配对后修改入口：**修改 = `node.revoke` + 重新配对（已知限制）**；`export.revoke` 不级联清理清单条目。…」→ 新：「清单只在确认时填报，没有配对后修改入口（**已知限制**，见 `NODE_LINK_PROTOCOL.md` §8.2）：**改清单只能重新配对——要么先 `node.revoke` 再重新配对，要么直接重新配对；两种做法都在落定后立即关闭该节点的活动连接，收窄因此自下一次握手后的新连接生效**。`export.revoke` 不级联清理清单条目。…」——保留「无配对后修改入口仍属已知限制」并指向 `NODE_LINK_PROTOCOL.md` §8.2，后半句逐字未动 |
| 第 597 行（§7「撤销与关闭顺序」） | 在 `device.revoke`/`node.revoke`/`export.revoke` 枚举之后、Daemon 关闭顺序之前补一句：「节点方向的 `node.pair.confirm` 在信任行提交后同样关闭该节点的活动连接（收窄/重新授权自下一次握手生效，`NODE_LINK_PROTOCOL.md` §8.2）」——措辞与同段既有体例一致 |

矛盾表述核对：全文 `grep 重新配对|配对后修改|立即关闭|活动连接|active connection|无修改入口` 只命中第 4/328/385/401/510/597 行；
第 328 行讲 `device.revoke`（设备）、第 401 行 `node.revoke`（同向）、第 510 行 §5.7 密钥轮换后必须重新配对
（与 §8.2 一致），均不冲突、未改。§7 该段没有「关闭由谁发起/如何实现」的额外约束句，未发现与新语义冲突的措辞。

`git diff --numstat 0d0afc3b..HEAD` = `3 3 docs/LOCAL_ADMIN_PROTOCOL.md`（`--name-only` 只有该文件；
`-U0` 的 hunk 只有 `@@ -4 +4 @@`、`@@ -385 +385 @@`、`@@ -597 +597 @@`）。改动全文见
`reports/wp5-docs-fix.log` 的「第六轮 fix」小节。

### 13.3 提交

| 提交 | 信息 | 文件 |
| --- | --- | --- |
| `fd59736` | `docs(app): 同步「改清单 = 重新配对」与落定后关连接口径到本地通道文档` | `docs/LOCAL_ADMIN_PROTOCOL.md` |

`git commit --no-verify`（worktree 无 `node_modules`，husky 钩子未装配）；提交信息合规性用主检出
`node_modules/.bin/commitlint --from 0d0afc3b73f367594bdba02942493485ca7a2135 --to HEAD`（读同一份
`commitlint.config.mjs`）自查通过（exit 0，scope `app` 在词表内），校验后临时目录联接已删除。
**新 HEAD = `fd597360e9bbf8b775a8508e56349dbcc94c8d83`**（父提交 `0d0afc3b`），
`git status --porcelain` 为空。

### 13.4 检查结果（本轮）

| 检查 | 命令 / 工作目录 | 退出码 | 结果 |
| --- | --- | --- | --- |
| **[PV1] check:docs（文档 worktree）** | `node scripts/check-doc-links.mjs`，cwd = `D:\Project\acp-remote-wt\export-ids-docs` | **0** | **PASS**：`380 relative links, 5123 section refs across 324 markdown files`（上一轮 5121；+2 = 两处新增的 `NODE_LINK_PROTOCOL.md` §8.2 引用，均可解析） |
| check:docs（主检出，只读复核） | `node scripts/check-doc-links.mjs`，cwd = `D:\Project\acp-remote` | **0** | **PASS**：`381 relative links, 7073 section refs across 353 markdown files` |
| 自检 ①：范围 | `git diff --numstat 0d0afc3b..HEAD` | 0 | 只有 `docs/LOCAL_ADMIN_PROTOCOL.md`，`3 3` |
| 自检 ②：§6 词表未动 | `sed -n '568,585p'` + hunk 落点核对 | 0 | §6 本地错误码表（第 568–585 行）与 §5.1 的 `local.*` 能力↔方法表（第 176 行）都不在 3 个 hunk 内 |
| 自检 ③：版本行 | `sed -n '4,9p'` + hunk 落点核对 | 0 | 文件头只有第 4 行一处改动；1.3/1.2/1.1 版本行逐字未动，无第二条 1.4 行 |
| 提交信息自查 | `commitlint --from 0d0afc3b --to HEAD`（临时 node_modules 联接） | 0 | PASS（非 [PV1] 证据） |

未执行 / 不适用：`check:drift`（跨 WP 已知 PENDING——代码在另一 worktree `D:\Project\acp-remote-wt\export-ids`，
`migrate.rs` 仍为 v3 旧形状；本轮未触碰 `CORE_PORTS_AND_STORAGE.md` §5 rust 块与 §7 SQL 块，不改变任何漂移判据）、
`check:agentic` 与需 `ajv` 的四道资产门禁、`cargo fmt/clippy/test`、`npm run check`/`verify`、E2E（跨 WP 或环境限制，
`plan.md` 记 `not-applicable`）。

### 13.5 handoff_index（fix 轮次 6 追加）

```yaml
handoff_index:
  - task_id: "2.5"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "fd597360e9bbf8b775a8508e56349dbcc94c8d83"
    evidence_type: CHECK
    evidence_id: "PV1 (check:docs)"
    report_path: "reports/handoff-coder-docs.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "§12.6 开放项 1 的文档面（第六轮 fix，target 0d0afc3b → fd59736）：把 NODE_LINK_PROTOCOL.md §8.2 已落定的新语义同步到 LOCAL_ADMIN_PROTOCOL.md。在 worktree D:\Project\acp-remote-wt\export-ids-docs、base 3cadb12d、node v24.19.0 上于新 revision fd59736 执行 `node scripts/check-doc-links.mjs`，退出码 0（380 相对链接 / 5123 章节引用全部可解析；上一轮 5121，+2 为本轮新增的两处 `NODE_LINK_PROTOCOL.md` §8.2 引用）。本轮改动全部是 Markdown（文件头既有 1.4 行追加半句 + §5.4 一行整行替换 + §7 一行整行替换），本门禁是该改动适用的直接判定，且不依赖 crates/** 的当前形状。同一命令在含权威 openspec/changes/** 的主检出（D:\Project\acp-remote）复跑亦为退出码 0（381 相对链接 / 7073 章节引用）。执行记录见 reports/wp5-docs-fix.log「第六轮 fix」小节"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "fd597360e9bbf8b775a8508e56349dbcc94c8d83"
    evidence_type: CHECK
    evidence_id: "PV1 (check:drift)"
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "§12.6 开放项 1 的文档面（第六轮 fix，同 revision fd59736）。本轮未执行也不适用：改动只在 docs/LOCAL_ADMIN_PROTOCOL.md 的 §5.4 与 §7 散文加文件头那一行，§5 的 rust 块、§7/§7.3 的 SQL 块与 §9 判据均未触碰（git diff 0d0afc3b..HEAD 只含该文件、只有 3 个 hunk），因此不改变任何一条漂移判据。该门禁在 worktree 的 PENDING 状态（代码在另一 worktree D:\Project\acp-remote-wt\export-ids，migrate.rs 仍为 v3 旧形状、export_ids_json 未写入）与 RV2/RV3/RV4/RV5 记录逐字相同，PASS 仍待 WP1 落地后在集成/候选阶段复跑（tasks 3.1、6.3）"
    source_evidence: NOT_APPLICABLE
```

### 13.6 交主 Agent 的开放项（本轮之后仍不在本 WP 写入范围）

1. `design.md` D8「已知限制：修改 = `node.revoke` + 重新配对」与 `plan.md`/`tasks.md`/`verification.md` 中
   同一措辞属规划工件，由主 Agent 维护（§12.6 开放项 2，本轮未动）。
2. `openspec/specs/node-link-owner-server/spec.md` 第 81 行的两条件可见性属归档同步项（既有 OOS，未变；
   §12.6 开放项 3）。
3. 两份权威文档在「改清单 = 重新配对、落定后关连接」这一语义上现已一致；`docs/LOCAL_ADMIN_PROTOCOL.md`
   与 `docs/NODE_LINK_PROTOCOL.md` 之间未发现本轮改动引入的新分歧。

## 14. fix 轮次 7（把「重新配对的关连接」与「撤销的关连接」在文档里区分开）

### 14.1 输入与授权

触发：上一轮（`0d0afc3`，报告 §12）在 `docs/NODE_LINK_PROTOCOL.md` §8.2 写下的段落把重新配对的关连接描述为
「与 `node.revoke` 用**同一把**『落定后关连接』的机制」。方向正确（都是「提交后关连接」），但不精确：读者会
据此推断关闭码也相同，而代码最初正是复用撤销路径——推送 `node.trust.revoked` 并以 **`4410`** 关闭，reason
还是 `"the node trust was revoked"`。这与 §15 的「撤销即停」（收到 `node.trust.revoked` 或本地 `node.revoke`
→ **停止重连**）冲突：**重新配对不是撤销**，该节点必须继续按 §15 重连、并在新连接上重取 catalog。

主 Agent 已定（另一 Agent 在 worktree `D:\Project\acp-remote-wt\export-ids` 同批改代码，本轮与之对齐）：

- 重新配对：新增专用关闭路径——**不**推送 `node.trust.revoked`，以 **`1000`（正常关闭）** 关闭，close reason
  形如 `"the node trust was re-confirmed; reconnect to fetch the updated catalog"`；
- 撤销（`node.revoke`）**不变**：仍推送 `node.trust.revoked` 并以 **`4410`** 关闭。

授权范围极小：**只改 `docs/NODE_LINK_PROTOCOL.md`**；不改代码（`crates/**`）、不改协议资产
（`schemas/**`、`fixtures/**`、`compatibility/**`）、不改其它 `docs/**`、不改规划工件
（`proposal.md`/`design.md`/`plan.md`/`tasks.md`/`verification.md`）。报告目录 `reports/` 追加本文件与
`wp5-docs-fix.log`。

### 14.2 修复位置与改动摘要（新版本 `ee1ca27`）

| 位置 | 旧句 → 新句摘要 |
| --- | --- |
| 第 5 行（文件头**既有** 2026-09-27 修订行末尾） | 在半句「…收窄因此自下一次握手后的新连接生效」之后追加「**；并明确该关闭与 `node.revoke` 的语义与关闭码不同：重新配对不推送 `node.trust.revoked`、以 `1000`（正常关闭）关闭并给出说明「信任已重新确认、请重连以重取 catalog」的 close reason，因此 §15 的「撤销即停」不适用、该节点仍按 §15 自动重连；只有 `node.revoke` 才推送 `node.trust.revoked` 并以 `4410` 关闭、停止重连。**」。未新增同日第二条修订行；其余修订行与 `版本`/`日期` 行逐字未动 |
| 第 280 行（§8.2「收窄在落定后于连接边界强制」） | 旧括注「（与 `node.revoke` 用同一把「落定后关连接」的机制）」→ 新正文「——与 `node.revoke` 一样都是「提交后关连接」，但**两者的语义与关闭码不同**：重新配对**不**推送 `node.trust.revoked`，而是以 **`1000`（正常关闭）** 关闭并给出说明「信任已重新确认、请重连以重取 catalog」的 close reason（形如 `"the node trust was re-confirmed; reconnect to fetch the updated catalog"`）——这是「授权已变化、请重新握手」，**不是**撤销，因此 §15 的「撤销即停」**不**适用；只有 `node.revoke` 才推送 `node.trust.revoked` 并以 `4410`（§14.2「节点已撤销」）关闭、停止重连」。该段后半（`exportIds` 收窄自下一次握手生效、core 读面不复制条件③）逐字保留 |
| 第 906 行（§14.2 close code 表下，新增一条表注） | 新句（表内 12 行 code 取值与原义**逐字未动**）：「表注：服务端因**授权变化**主动结束连接时用 `1000`（正常关闭）加 close reason，不用 `4410`——例如 `node.pair.confirm` 提交后的重新配对（§8.2）使既有连接作废、要求对端重连以重取 catalog，客户端因此**不**按 §15 的「撤销即停」停止重连，而是按 §15 自动重连；`4410` 只用于节点撤销（收到 `node.trust.revoked` 或本地 `node.revoke`）。」 |
| 第 930 行（§15 `[决定]（2026-09-23）连接建立与重连` 的「撤销即停」条目） | 旧句「**撤销即停**：收到 `node.trust.revoked`（§12.6）或本地 `node.revoke` 提交后→停止重连并关闭连接。」→ 句末追加「该条**只**由撤销触发（这两个触发条件之一），**不**包括服务端因其它原因主动关闭连接——例如 `node.pair.confirm` 提交后的重新配对以 `1000` 关闭该节点的活动连接（§8.2）：那只是连接作废、授权已重算，对端仍按上面的首次/启动连接与重连退避规则自动重连。」原句与 `（§12.6）` 引用逐字未动 |

节号说明（**不改变文档内容，只说明本轮落笔口径**）：任务书写「§13 的『撤销即停』」，该条实际位于本文件
**§15**「顺序、重放和冲突」下的 `[决定]（2026-09-23）连接建立与重连` 块（第 930 行）；§13 是「节点配对 HTTP」。
文档内引用因此按实际章节号写 `§15`（含「按 §15 自动重连」「§15 的『撤销即停』」），未按任务书的 §13 落笔，
以免在权威文档里留下错误节号。

`git diff --numstat fd597360..HEAD` = `5 3 docs/NODE_LINK_PROTOCOL.md`（`--name-only` 只有该文件；
`-U0` 的 hunk 只有 `@@ -5 +5 @@`、`@@ -280 +280 @@`、`@@ -905,0 +906,2 @@`、`@@ -928 +930 @@`）。全文改动见
`reports/wp5-docs-fix.log` 的「第七轮 fix」小节。

### 14.3 提交

| 提交 | 信息 | 文件 |
| --- | --- | --- |
| `ee1ca27` | `docs(node-link): 区分重新配对与撤销的关闭语义（1000 与 4410）` | `docs/NODE_LINK_PROTOCOL.md` |

`git commit --no-verify`（worktree 无 `node_modules`，husky 钩子未装配）；提交信息合规性用主检出
`node_modules/.bin/commitlint --from fd597360e9bbf8b775a8508e56349dbcc94c8d83 --to HEAD`（读同一份
`commitlint.config.mjs`）自查通过（exit 0，scope `node-link` 在词表内），校验后临时目录联接已删除。
**新 HEAD = `ee1ca2774e29c15b9b71ff2005497ea4fe01bd8e`**（父提交 `fd597360`），
`git status --porcelain` 为空。

### 14.4 检查结果（本轮）

| 检查 | 命令 / 工作目录 | 退出码 | 结果 |
| --- | --- | --- | --- |
| **[PV1] check:docs（文档 worktree）** | `node scripts/check-doc-links.mjs`，cwd = `D:\Project\acp-remote-wt\export-ids-docs` | **0** | **PASS**：`380 relative links, 5131 section refs across 324 markdown files`（上一轮 5123；+8 = 新写入的 §15×4、§14.2×2、§8.2、§12.6 正文引用，均可解析） |
| check:docs（主检出，只读复核） | `node scripts/check-doc-links.mjs`，cwd = `D:\Project\acp-remote` | **0** | **PASS**：`381 relative links, 7104 section refs across 353 markdown files`（本轮报告写入前；写入后复跑见下） |
| 自检 ①：范围 | `git diff --numstat fd597360..HEAD` | 0 | 只有 `docs/NODE_LINK_PROTOCOL.md`，`5 3` |
| 自检 ②：§14.2 code 表未动 | `git diff -U0 fd597360..HEAD` + hunk 落点核对 | 0 | 唯一涉及 §14.2 的 hunk 是 `+906,2`（新增一条表注）；表内 12 行 close code（`1000` 至 `4500`）的取值与含义逐字未动 |
| 自检 ③：文件头无同日第二条 | `git diff -U0` 第 5 行 hunk | 0 | 只改既有 2026-09-27 行末尾；`版本：1.0`、`日期` 与其余 4 条修订行逐字未动 |
| 提交信息自查 | `commitlint --from fd597360..HEAD`（临时 node_modules 联接） | 0 | PASS（非 [PV1] 证据） |

未执行 / 不适用：`check:drift`（跨 WP 已知 PENDING——代码在另一 worktree `D:\Project\acp-remote-wt\export-ids`；
本轮只改一个 Markdown 文件，未触碰 `CORE_PORTS_AND_STORAGE.md` §5 rust 块与 §7 SQL 块，不改变任何漂移判据）、
`check:agentic` 与需 `ajv` 的四道资产门禁、`check:errors`/`check-command-catalog`（本轮未改任何 close code
取值；`scripts/check-error-registry.mjs` 只比对 `compatibility/errors/v1/errors.json` 的
`protocols.{sync,node_link}.closeCodes`，不解析本文档 §14.2 表格）、`cargo fmt/clippy/test`、
`npm run check`/`verify`、E2E（跨 WP 或环境限制，`plan.md` 记 `not-applicable`）。

### 14.5 handoff_index（fix 轮次 7 追加）

```yaml
handoff_index:
  - task_id: "2.5"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "ee1ca2774e29c15b9b71ff2005497ea4fe01bd8e"
    evidence_type: CHECK
    evidence_id: "PV1 (check:docs)"
    report_path: "reports/handoff-coder-docs.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "§12/§13 已落定语义的精度修正（第七轮 fix，target fd597360 → ee1ca27）：删除「重新配对的关连接与 node.revoke 是同一把机制」的歧义表述，写明两者语义与关闭码不同（重新配对 = 不推 node.trust.revoked + 1000 + 专用 close reason；node.revoke = node.trust.revoked + 4410），并澄清 §15「撤销即停」只由撤销触发、重新配对后的节点仍按 §15 自动重连。在 worktree D:\Project\acp-remote-wt\export-ids-docs、base 3cadb12d、node v24.19.0 上于新 revision ee1ca27 执行 `node scripts/check-doc-links.mjs`，退出码 0（380 相对链接 / 5131 章节引用全部可解析；上一轮 5123，+8 为本轮正文新增的 §15/§14.2/§8.2/§12.6 引用）。本轮改动全部是 Markdown（文件头既有 2026-09-27 行追加半句 + §8.2 整句替换 + §14.2 新增表注 + §15 条目追加半句），本门禁是该改动适用的直接判定，且不依赖 crates/** 的当前形状。同一命令在含权威 openspec/changes/** 的主检出（D:\Project\acp-remote）复跑亦为退出码 0（381 相对链接 / 7104 章节引用）。执行记录见 reports/wp5-docs-fix.log「第七轮 fix」小节"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "ee1ca2774e29c15b9b71ff2005497ea4fe01bd8e"
    evidence_type: CHECK
    evidence_id: "PV1 (check:drift)"
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "§12/§13 已落定语义的精度修正（第七轮 fix，同 revision ee1ca27）。本轮未执行也不适用：改动只在 docs/NODE_LINK_PROTOCOL.md 的 §8.2/§14.2/§15 散文与文件头那一行，§5 的 rust 块、§7/§7.3 的 SQL 块与 §9 判据均未触碰（git diff fd597360..HEAD 只含该文件、只有 4 个 hunk），因此不改变任何一条漂移判据。该门禁在 worktree 的 PENDING 状态（代码在另一 worktree D:\Project\acp-remote-wt\export-ids，migrate.rs 仍为 v3 旧形状、export_ids_json 未写入）与 RV2/RV3/RV4/RV5/RV6 记录逐字相同，PASS 仍待 WP1 落地后在集成/候选阶段复跑（tasks 3.1、6.3）"
    source_evidence: NOT_APPLICABLE
```

### 14.6 交主 Agent 的开放项（本轮之后仍不在本 WP 写入范围）

1. **代码侧对齐**（另一 Agent 的范围，本轮只做文档面）：worktree `D:\Project\acp-remote-wt\export-ids` 的
   HEAD（`8ae3b61`）上，重新配对仍走撤销路径——`crates/server/src/local_admin/router.rs` 的
   `node.pair.confirm` 调用 `close_node`，而 `crates/app/src/compose.rs` 的 `NodeLinkCloser::close_node` 会
   `node_revoked`（推送 `node.trust.revoked` + `4410`）；`crates/app/tests/node_link_e2e.rs:837` 的断言
   `assert_eq!(code, 4410, "重新配对按 node.revoke 的同一机制关闭连接")` 仍是旧口径。专用 `1000` + 新 reason
   路径落地后，该断言与 `crates/server/src/node_link/conn/registry.rs` 的 `close_node(node, code, reason)`
   调用点需要同步（本轮未改任何 `crates/**`）。
2. 任务书里的节号「§13」与本文件实际节号「§15」不一致（见 §14.2 的节号说明），文档按实际节号落笔；
   若后续报告/评审引用该条，请以 §15 为准。
3. `design.md` D8「已知限制：修改 = `node.revoke` + 重新配对」与 `plan.md`/`tasks.md`/`verification.md`
   中同一措辞属规划工件，由主 Agent 维护（§12.6 开放项 2，本轮未动）。
4. `openspec/specs/node-link-owner-server/spec.md` 第 81 行的两条件可见性属归档同步项（既有 OOS，未变；
   §12.6 开放项 3）。

## 15. fix 轮次 8（RV3-IMPL-F2 复核发现的既有错误交叉引用勘误）

### 15.1 输入与授权

- 发现来源：独立复核 `RV3-IMPL-F2`；主 Agent 已逐条核实「`node.trust.revoked` 登记在 §12.3 而非 §12.6」。
- 授权范围（本轮只做这一件事）：改 `docs/NODE_LINK_PROTOCOL.md` 第 930 行一处节号；不改代码、不改规划工件、
  不改文件头修订记录（不新增同日第二条）。base = `ee1ca2774e29c15b9b71ff2005497ea4fe01bd8e`。

### 15.2 修复位置与改动摘要（新版本 `659e590`）

| 位置 | 旧句 → 新句摘要 |
| --- | --- |
| 第 930 行（§15 `[决定]（2026-09-23）连接建立与重连` 的「撤销即停」条目） | 行内 `收到 \`node.trust.revoked\`（§12.6）` → `收到 \`node.trust.revoked\`（§12.3）`；行内其余文字（含第七轮追加的限定句「该条**只**由撤销触发…对端仍按上面的首次/启动连接与重连退避规则自动重连。」）**逐字未动**。属既有引用勘误，语义未变（§12.3 是 Catalog 消息家族、第 521 行即 `node.trust.revoked` 的字段表；§12.6 只含 ping/pong、`link.error`、`link.backpressure`，不含该消息） |

**文件头**：本轮**未**改动文件头（`git diff -U0 ee1ca27..HEAD` 的 hunk 只有 `@@ -930 +930 @@`），
即未新增修订记录行、也未在既有 2026-09-27 行追加勘误句——理由是这属纯节号勘误、不改任何 wire/合同语义，
而该行描述的是本次变更的合同增量；勘误事实改由本报告与 `reports/wp5-docs-fix.log` 的「第八轮 fix」小节承载。

### 15.3 全文检出（本文件内 `§12.6`/`§12.3` 逐处判断）

| # | 位置 | 原文引用 | 判断 | 依据 |
| --- | --- | --- | --- | --- |
| 1 | 第 930 行（§15「撤销即停」） | `node.trust.revoked`（§12.6） | **改为（§12.3）** | §12.3 第 521 行是该消息的字段表；§12.6 只含 ping/pong、`link.error`、`link.backpressure` |
| 2 | 第 280 行（§8.2） | `（§12.3、§12.4）` | 保留不动 | 指 catalog/attach/授权按信任行重算，分别落在 §12.3、§12.4 |
| 3 | 第 627 行（§11.4） | 可见 Export（§12.3） | 保留不动 | Export 条目与可见性语义在 §12.3 |
| 4 | 第 931 行（§15「每次连接都重取 catalog」） | catalog（§12.3） | 保留不动 | catalog 消息在 §12.3 |
| 5 | 第 597 行 | 章节标题 `### 12.6 控制与错误消息` | 保留不动 | 章节自身标题（无 § 前缀），非引用 |

改动后本文件已无 `§12.6` 形式的引用（`rg -n "§12\.6" docs/NODE_LINK_PROTOCOL.md` 无输出、exit 1）；
不带 § 前缀的 `12.6` 只剩第 597 行的章节标题本身。另：`rg` 全仓库 `.md` 搜 `node.trust.revoked`/`export.revoked`
的节号标注，只有 `docs/LOCAL_ADMIN_PROTOCOL.md:467` 引用 `export.revoked`（本文件 §12.3）——引用正确、未触碰。

### 15.4 提交

| 提交 | 信息 | 文件 |
| --- | --- | --- |
| `659e590` | `docs(node-link): 勘误 §15 对 node.trust.revoked 的交叉引用（§12.6 → §12.3）` | `docs/NODE_LINK_PROTOCOL.md` |

`git commit --no-verify`（worktree 无 `node_modules`，husky 钩子未装配）；提交信息合规性用主检出
`node_modules/.bin/commitlint --from ee1ca2774e29c15b9b71ff2005497ea4fe01bd8e --to HEAD`（读同一份
`commitlint.config.mjs`）自查通过（exit 0，scope `node-link` 在词表内），校验后临时目录联接已删除。
**新 HEAD = `659e5900ecf79daa110278a07a31bef418dc5d0d`**（父提交 `ee1ca27`），`git status --porcelain` 为空。

### 15.5 检查结果（本轮）

| 检查 | 命令 / 工作目录 | 退出码 | 结果 |
| --- | --- | --- | --- |
| **[PV1] check:docs（文档 worktree）** | `node scripts/check-doc-links.mjs`，cwd = `D:\Project\acp-remote-wt\export-ids-docs` | **0** | **PASS**：`380 relative links, 5131 section refs across 324 markdown files`（与第七轮同值：§12.6 → §12.3 不增减引用数） |
| check:docs（主检出，只读复核） | `node scripts/check-doc-links.mjs`，cwd = `D:\Project\acp-remote` | **0** | **PASS**：`381 relative links, 7247 section refs across 354 markdown files`（较第七轮 7104/353 的增长来自各角色新写入的 `reports/**`，非本轮改动） |
| 自检 ①：范围 | `git diff --numstat ee1ca277..HEAD` / `--name-only` | 0 | 只有 `docs/NODE_LINK_PROTOCOL.md`，`1 1` |
| 自检 ②：§12.6 消息清单未动 | `git diff -U0 ee1ca277..HEAD` | 0 | 唯一 hunk 是 `@@ -930 +930 @@`；§12.6 的第 597–603 行（ping/pong、`link.error`、`link.backpressure` 三行）不在任何 hunk 内 |
| 自检 ③：文件头未动 | `git diff -U0 ee1ca277..HEAD` | 0 | 第 5 行（2026-09-27 修订行）与其余修订行、`版本`/`日期` 行均不在 hunk 内；未新增同日第二条 |
| 提交信息自查 | `commitlint --from ee1ca27..HEAD`（临时 node_modules 联接） | 0 | PASS（非 [PV1] 证据） |

**门禁能力的限制（必须随证据一起读）**：`scripts/check-doc-links.mjs` **不校验 `§N` 交叉引用是否真实存在、
是否指向正确章节**（只解析 markdown 链接与「同子句内指名文档」的章节引用归属），因此 `（§12.6）` 这个
错误引用在它眼里同样是「可解析」。本处修改的直接证据是 §15.3 的逐处核对表与
`reports/wp5-docs-fix.log`「第八轮 fix」小节里的 `rg`/`sed` 原文输出；两次门禁只证明改动未打断既有引用归属。

未执行 / 不适用：`check:drift`（同 §14.4，跨 WP 已知 PENDING）、`check:agentic` 与需 `ajv` 的四道资产门禁、
`check:errors`/`check-command-catalog`、`cargo fmt/clippy/test`、`npm run check`/`verify`、E2E。

### 15.6 handoff_index（fix 轮次 8 追加）

```yaml
handoff_index:
  - task_id: "2.5"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "659e5900ecf79daa110278a07a31bef418dc5d0d"
    evidence_type: CHECK
    evidence_id: "PV1 (check:docs)"
    report_path: "reports/handoff-coder-docs.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "RV3-IMPL-F2 复核发现的既有错误交叉引用勘误（第八轮 fix，target ee1ca27 → 659e590）：§15「撤销即停」条目把 `node.trust.revoked` 标为（§12.6），但该消息登记在 §12.3 Catalog 消息（第 521 行字段表；§12.6 只含 ping/pong、`link.error`、`link.backpressure`），base（3cadb12d）即错、非本次变更引入；本轮只把这一处节号改为（§12.3），行内其余文字（含第七轮追加的限定句）与文件头逐字未动。在 worktree D:\Project\acp-remote-wt\export-ids-docs、base 3cadb12d、node v24.19.0 上于新 revision 659e590 执行 `node scripts/check-doc-links.mjs`，退出码 0（380 相对链接 / 5131 章节引用全部可解析，与第七轮同值）。本轮改动全部是 Markdown 单行节号，本门禁是该改动适用的直接判定且不依赖 crates/** 的当前形状；但该门禁不校验 §N 是否真实存在/正确，逐处核对（§15.3 的表 + 日志里的 rg/sed 原文输出）才是本次修改的直接证据。同一命令在含权威 openspec/changes/** 的主检出（D:\Project\acp-remote）复跑亦为退出码 0（381 相对链接 / 7247 章节引用 / 354 个 md，增长来自各角色新写入的 reports/**）。执行记录见 reports/wp5-docs-fix.log「第八轮 fix」小节"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "659e5900ecf79daa110278a07a31bef418dc5d0d"
    evidence_type: CHECK
    evidence_id: "PV1 (check:drift)"
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "同上（第八轮 fix，同 revision 659e590）。本轮未执行也不适用：改动只是 docs/NODE_LINK_PROTOCOL.md 第 930 行一个字面节号（git diff ee1ca27..HEAD 只有该文件、单个 hunk `@@ -930 +930 @@`），未触碰 CORE_PORTS_AND_STORAGE.md §5 的 rust 块、§7/§7.3 的 SQL 块与 §9 判据，因此不改变任何一条漂移判据。该门禁在 worktree 的 PENDING 状态（代码在另一 worktree D:\Project\acp-remote-wt\export-ids，migrate.rs 仍为 v3 旧形状、export_ids_json 未写入）与 RV2/RV3/RV4/RV5/RV6 及第七轮记录逐字相同，PASS 仍待 WP1 落地后在集成/候选阶段复跑（tasks 3.1、6.3）"
    source_evidence: NOT_APPLICABLE
```
