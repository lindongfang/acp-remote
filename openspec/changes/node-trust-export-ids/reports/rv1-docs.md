<!-- RV1-DOCS：WP5（tasks.md 2.5）文档工作包的独立只读检视报告。字段与 handoff_index 组织见 openspec/schemas/agentic/roles/handoff.md。 -->

# 独立检视报告：RV1-DOCS / WP5 文档同步（task 2.5）

| 字段 | 值 |
| --- | --- |
| `task_id` | 2.5 |
| `role` | reviewer（独立代码/文档检视子 Agent，只读） |
| `phase` | review |
| `stage` | work-package |
| `agent_context` | 新建子 Agent，**不继承**父 Agent 的实现对话，未参与 WP5 的实现与讨论；仅消费调度者给出的检视输入（变更 proposal/design/specs/plan/tasks/verification、`AGENTS.md`、被检视 worktree）与自身只读读取的文件。本 Agent 无法证明宿主未注入其它上下文，只能声明自身可见范围（见「Review Context / 限制」）。 |
| `target_revision` | `d6e01b3b3bbc71d4d6bd2edf34cc4dce3c0043fb`（`agentic/node-trust-export-ids-docs` HEAD，已用 `git rev-parse HEAD` 核实；`git status --porcelain` 为空） |
| `base_revision` | `3cadb12d79456750e40644a2ea53c96380b700e6`（= `refs/heads/main`） |
| `scope` | 分支 `3cadb12..d6e01b3` 的**文档改动**（4 个文件），外加仓库级一致性搜索 |
| `changes` | 本次检视未修改任何代码/测试/规划文件；只在 `reports/` 下新增本报告 |
| `checks` | 见 §3「检查范围与命令」；[PV1] 的 `check:docs` 已独立复现，`check:drift` 记待返（PENDING），`check:agentic` 在本 worktree 不可执行 |
| `issues` | 5 项发现（1 项 MAJOR、3 项 MINOR、1 项 SUGGESTION）+ 2 项规划/资产侧建议，见 §4 |
| `result` | **FAIL**（存在未解决的 MAJOR 发现 RV1-DOCS-F1） |
| `evidence_paths` | 本报告 `openspec/changes/node-trust-export-ids/reports/rv1-docs.md`；被检视实现者证据（非本人证据，仅作对照）`reports/handoff-coder-docs.md`、`reports/wp5-docs.log` |
| `resource_cleanup` | 未创建临时文件、目录联接、数据库、端口或子进程；执行的全部命令为只读（`git`、`grep`、`sed`、`node scripts/check-doc-links.mjs`、`node scripts/check-contract-drift.mjs`，后两者只读文件、无写入）；被检视 worktree `D:\Project\acp-remote-wt\export-ids-docs` 检视后仍为干净（`git status --porcelain` 为空）；未改动主检出受版本控制文件、未切换分支、未提交、未执行 E2E |

## 1. Review Context

- **Review ID**：RV1-DOCS（唯一，首轮）；**Review Type**：branch；**Review Stage**：work-package（WP5 交付后）。
- **Repository / 版本**：代码与文档仓库 `D:\Project\acp-remote`（主检出，`main` = `3cadb12d`）；被检视版本在 worktree `D:\Project\acp-remote-wt\export-ids-docs`，分支 `agentic/node-trust-export-ids-docs`，HEAD = `d6e01b3b`（三个提交 `c3f80580` → `e95c7783` → `d6e01b3b`）。版本稳定：worktree 工作区干净、HEAD 与调度者给定 target 一致、diff 可复现。
- **实际检查范围**：`git -C D:\Project\acp-remote-wt\export-ids-docs diff 3cadb12..d6e01b3`（`--name-only` 只有 4 个文件：`docs/CORE_PORTS_AND_STORAGE.md`、`docs/LOCAL_ADMIN_PROTOCOL.md`、`docs/NODE_LINK_PROTOCOL.md`、`openspec/specs/storage-schema-v2-migration/spec.md`；34 insertions / 17 deletions），逐行读 diff 与改动后的完整上下文，再读所有被引用的权威处（`NODE_LINK_PROTOCOL.md` §8.2/§10/§12.3、`CORE_PORTS_AND_STORAGE.md` §3.5/§5.3/§6/§7.2/§7.3/§9/§11.1/§11.3/§11.6/§11.7/§11.8、`LOCAL_ADMIN_PROTOCOL.md` §5.4/§5.8/§6/§8、`AGENTS.md` §3/§8/§10/§13），并做仓库级反向表述搜索（`docs/**`、`openspec/specs/**`、`openspec/changes/**`、`compatibility/**`、`AGENTS.md`、`README.md`、`docs/adr/**`）。
- **必要资料**：`proposal.md`、`design.md`（D1–D10，唯一设计口径）、三份增量规范（15 个场景）、`plan.md`（Coverage Index R1–R18、Local Checks、Project Verify、Code Review、Main E2E）、`tasks.md` 2.5、`verification.md`（Target、Handoff Index、Check Plan Changes）。以上均按只读方式读取；实现者的自评（`handoff-coder-docs.md`）只作对照，不作为证据。
- **Code Review 计划判据（`plan.md` 的 Code Review 节）**：逐 WP 检视「可见性两处必须同口径」「迁移不得默认放权且列清单与新建库相等」「`settle_pairing` 校验与写入同事务」「CLI 警告不阻断」「文档与 DDL/门禁一致」「无 `unwrap`/detached task」。本轮只覆盖其中**文档面**（DDL 文本、可见性口径、迁移叙述、校验语义、CLI 描述）；代码面属 RV1-impl（tasks 3.2）。

### 限制与不确定性

1. **代码不在本版本内**：`crates/**` 在本分支与 base 相同（WP1–WP4 在 worktree `D:\Project\acp-remote-wt\export-ids`，`git worktree list` 显示它目前仍停在 `3cadb12`，尚无提交）。因此「文档与实现一致」这一条只能以 design/specs 冻结文本为参照做静态判断，**不能**确认代码侧结论。
2. **[PV1] 的 `check:drift` 未通过（PENDING）**：本人独立复跑得 exit 1，唯一差异是 §7 块 1（`OWNED_SCHEMA_V1`）第 20 条语句（`owned_node`），代码侧 `crates/storage-sqlite/src/migrate.rs` 仍是 v3 旧形状（`FILE_FORMAT_VERSION`/`OWNED_SCHEMA_VERSION`/`IMPORTED_SCHEMA_VERSION` = 3/3/3，`owned_node` 无新列）。这与调度者给定情况一致，属预期跨 WP 依赖，不构成本 WP 的缺陷，但按契约记 `evidence_status: PENDING` / `result: BLOCKED`（见 §5）。
3. **`check:agentic` 未执行**：本 worktree 无 `node_modules`（`check:agentic` = `scripts/agentic-gate.mjs` + 项目本地引擎），本人不建立目录联接、不直接调用引擎（引擎默认开启遥测，须走仓库 wrapper）。因此「`openspec validate --all --strict` 是否接受本次对 `openspec/specs/storage-schema-v2-migration/spec.md` 的 Purpose 改动」在**本版本上未验证**，记为待返证据。`plan.md` 的 [PV1] 包含该子门禁，须由在集成树上有 `node_modules` 的执行者补跑。
4. **E2E 不适用**：本 WP 是文档工作包，无用例；`plan.md` 已将变更级 Main E2E 记 `not-applicable`（用户 2026-09-27 批准）。
5. 本 Agent 未参与实现，也不声称能看到宿主注入的全部上下文。

## 2. 被检视改动的范围（changes）

| 文件 | 改动摘要 | 归属 |
| --- | --- | --- |
| `docs/NODE_LINK_PROTOCOL.md` | 文件头新增 2026-09-27 修订记录；§8.2 信任记录文本块 `scopes` 之后加 `exportIds`；把「首阶段不使用 `exportIds` 的两个条件」改成「带 `exportIds` 的三条件 + 只能收窄 + 撤销不级联 + 只在确认时填报（无修改入口）+ 升级副作用 + wire 不加字段/Access 侧孤儿 Import 只文档化」 | task 2.5 第 1 条 |
| `docs/LOCAL_ADMIN_PROTOCOL.md` | 文件头新增「版本：1.4」；§5.4 `NodeRecord` 加 `exportIds string[]`；`node.pair.confirm` 的 `params` 加必填 `exportIds`、`result` 回显，并写三类失败与「无修改入口/撤销不级联/升级副作用」 | task 2.5 第 2 条 |
| `docs/CORE_PORTS_AND_STORAGE.md` | §3.5 `NodeRecord`/`PairingSettlement`；§7 标题改 v4；§7.2 常量 4/4/3、升级判据改 `< 4`、新增 v3 → v4 段、v1→v2 段第 4 条与 v2→v3 段的落盘口径改写、迁移资产与回滚改 v4；§7.3 `owned_node` 末尾加 `export_ids_json`；§9 判据 1/28；§11.6 第 4 条；§11.7 集合列说明句；§11.8 新增「为什么 v3 → v4 只加列」 | task 2.5 第 3 条 |
| `openspec/specs/storage-schema-v2-migration/spec.md` | 仅 Purpose：「v1 到 v2」→「v1 逐版升级到 v4（v1 → v2 → v3 → v4）」；Requirements/Scenario 未动（留归档同步） | task 2.5 第 4 条 |

## 3. 检查范围与命令（自行执行，均为只读）

| # | 命令 / 方法 | 结果 | 建立的事实 |
| --- | --- | --- | --- |
| C1 | `git rev-parse HEAD`、`git status --porcelain`、`git diff --stat/--name-only 3cadb12..d6e01b3` | HEAD = `d6e01b3b`；工作区空；仅 4 个 md 文件 | 版本稳定、范围与调度者声明一致 |
| C2 | 逐行读 `git diff 3cadb12..d6e01b3` 全文 + 改动后上下文 | 已读全部 51 行变更 | 逐条核对基础 |
| C3 | `node scripts/check-doc-links.mjs`（worktree 根，node v24.19.0） | **exit 0**：`380 relative links, 5090 section refs across 324 markdown files` | **独立复现** [PV1] `check:docs` PASS，且与实现者报告数字逐字一致（380/5090） |
| C4 | `node scripts/check-contract-drift.mjs` | **exit 1**：`§7 块 1 (OWNED_SCHEMA_V1) 第 20 条语句不一致` | 与实现者声明一致；见 §1 限制 2 与 §5 |
| C5 | `grep -rn "exportIds\|export_ids\|export-id"` 全仓库（排除 `target/`、`node_modules`）、`grep -n "可见\|推后\|不使用\|两个条件\|按类别"` 定向搜索 | 见 §4 F1/F2/F5 与 §5 待返项 | 反向表述与重复定义定位 |
| C6 | `grep -n "v3\|user_version\|推进到\|文件格式版本"` 于 `CORE_PORTS_AND_STORAGE.md`/`MODULE_ARCHITECTURE.md`/`SECURITY_DESIGN.md`/主规范 | 见 §4 F3 | §7.2 全量改 4/4/3 的遗留扫描 |
| C7 | 逐字比对 `docs/CORE_PORTS_AND_STORAGE.md` §7.3 第 1052 行与 design D3 | 一致 | 见 §5 核对表 V3 |
| C8 | 读 `scripts/check-contract-drift.mjs` 全文（归一化口径：SQL 删 `--` 注释、压空白、转小写、去 `IF NOT EXISTS`；覆盖 §7 SQL 块与 §5 rust 块） | 已读 | 判定「门禁覆盖范围」：§5.3/§6 的**散文**不在门禁内，因此 F1/F2 不会被门禁发现 |
| C9 | 读 `schemas/local-admin/v1/envelope.schema.json`、`fixtures/local-admin/v1/**`、`compatibility/commands/v1/commands.json`（只读） | `params`/`result` 确为通用对象；无 `node.pair.confirm` 逐方法载荷 fixture；无 `exportIds` | 「资产不改」的依据成立（与 verification.md 一致） |
| C10 | 读 `crates/storage-sqlite/src/migrate.rs`（v3 旧版）与 `crates/server/src/local_admin/params.rs`（v3 旧版） | 见 §4 F1 之外的「跨 WP 风险」 | 交叉验证 v3 段守卫与 params 白名单属 WP1/WP3 义务 |

## 4. Findings

| ID | 级别 | 位置（target `d6e01b3b`） | 触发条件 | 预期 / 实际 | 影响 | 建议 |
| --- | --- | --- | --- | --- | --- | --- |
| RV1-DOCS-F1 | **MAJOR** | `docs/CORE_PORTS_AND_STORAGE.md` §6 第 5 条（第 791 行，「授权调用点」） | Access 节点 N 的 `grants = {grant.remote-work}`、`exportIds = [E1]`；本机另有一个未撤销的 E2，其 `scopes` 与 N 的 grants 相交但 `E2.exportId ∉ N.exportIds`；N 提交一条 `required_grant` 可由 E2 满足的命令 | 预期（design D1、`NODE_LINK_PROTOCOL.md` §8.2 第 274 行、同文件 §11.6 第 4 条新增句）：core 的 Owner 侧判定必须要求目标 Export **在该信任行的 `exportIds` 内**，「目录里看不到的 Export 也不能被命令选中」。实际：§6 第 5 条仍把 Owner 侧要求写成「存在一个未撤销的 `ExportRecord`——它的 `scopes` 含该 grant、与本节点信任记录的 grants 有交集（`export.scopes ∩ node.grants ≠ ∅`，与 Node Link 的可见性口径同源）」，**只列两个条件**，并把这个两条件口径明确标注为「与 Node Link 的可见性口径同源」 | 同一文档内自相矛盾：§11.6 第 4 条（本次新增）写「清单只能**收窄** Owner 侧的命令授权与 catalog 可见性」，而规定该授权调用点内容的 §6 第 5 条没有清单条件。`check:contract-drift` 只比对 §7 SQL 块与 §5 rust 块（C8 已核实），**抓不到 §6 散文**。后果：迁移/实现者（WP2）与后续 reviewer 可能按 §6 的两条件实现 core 侧，漏掉 D1 明确要求的「清单外 Export 不能被命令选中」，形成「目录不可见但命令可用」的授权缺口（违反 `AGENTS.md` §3 安全不变量与「有效权限是交集」口径） | 在 §6 第 5 条的 Owner 侧句中补入清单条件（「**并且**目标 Export 同时在该 `(nodeId, kind=access)` 行的 `exportIds` 内，与 §8.2 的三条件同口径」），并去掉/改写「与 Node Link 的可见性口径同源」这一现在不完整的括注。这是**小范围文本修复**，但超出 design D10 与 tasks 2.5 列出的文档清单，请主 Agent 决定是否在本 WP 内补（属扩范围决策） |
| RV1-DOCS-F2 | MINOR | `docs/CORE_PORTS_AND_STORAGE.md` §5.3（第 220 行，「Node Link 资源读面的三条端口 seam」） | 读 §5.3 判定「谁拥有 Export 可见性」 | 预期：与 §8.2 三条件一致，或只指回权威处。实际：括注写「（D14：未撤销且 `export.scopes ∩ 节点 grants ≠ ∅`）」，仍以两条件内容复述「单点可见性策略」 | 同一语义的第二处定义且内容过期；`AGENTS.md` §10 要求「两份文档重叠时保留一个权威定义，另一处只写概要并链接过去」。同处「不在 core 重复实现」的表述在 core 侧新增条件 3 授权判定后也需限定（core 判的是**授权**，不是可见性投影） | 把括注改为「（三条件见 `NODE_LINK_PROTOCOL.md` §8.2；core 只在授权侧按同一节点行复核清单）」或直接删除括注内内容，只留链接 |
| RV1-DOCS-F3 | MINOR | `docs/CORE_PORTS_AND_STORAGE.md` §11.3（第 1451 行） | 读升级/「过新」用例的取值 | 预期：与 §7.2 一致（「版本过新拒绝启动」用例把 `user_version` 顶到 `FILE_FORMAT_VERSION + 1`，当前即 5）。实际：仍写「「过新」用例使用高于新版本的值（`user_version = 3`）」 | 与同文档 §7.2 冲突；本次版本推进到 4 后「高于新版本的值 = 3」已明显不成立。**经 base 对照确认这是既有过期表述**（`git show 3cadb12:docs/CORE_PORTS_AND_STORAGE.md` 同一行逐字相同，在 v3 时代 3 也已不是「高于」值），非本次引入；但本次正是「版本常量全量同步」的改动，遗留它会让读者以为文档只改了一半 | 改为「…用例使用高于新版本的值（`FILE_FORMAT_VERSION + 1`，见 §7.2）」。是否纳入本 WP 由主 Agent 决定（一行文本） |
| RV1-DOCS-F4 | MINOR | `docs/CORE_PORTS_AND_STORAGE.md` §9「验收判据」列表（判据 23/24 附近；第 1323–1355 行） | 查找新增 MUST 的验收判据 | 预期：design D9 要求 `settle_pairing` 覆盖「存在且未撤销 / 已撤销 / 不存在 / 与 grants 不相交 / 空集合」五类校验与失败零写入，合同侧应有对应判据行。实际：§9 只更新了判据 1 与 28（迁移面）；判据 23 只覆盖「一次管理端口调用 = 一个事务」的原子性，判据 24 覆盖配对/撤销/重启，均未断言清单校验的四类规则 | 合同声明了 MUST（§11.6 第 4 条、§5.4、§8.2）而「实现该合同的测试」清单里没有对应判据，[PV3] 的判据来源因此不完整；`AGENTS.md` §10 要求端口/写集语义变化同步 `CORE_PORTS_AND_STORAGE.md` | 在判据 24 之后追加一条（或在判据 23/24 内补句）：`settle_pairing` 对清单的四类校验 + 空集合不查 Export + 任一失败整事务回滚（零信任行、零状态推进、零审计）。同上，属范围决策 |
| RV1-DOCS-F5 | SUGGESTION | `docs/NODE_LINK_PROTOCOL.md` 第 7 行（2026-09-26 修订记录） | 自上而下读文件头修订记录 | 预期：历史记录可保留（本文件体例保留了 2026-09-18/09-26 等旧记录）。实际：该条仍写「§8.2 明确首阶段 Export 可见性只取〔两个条件〕，`exportIds` 维度推后（用户裁决 (b)…）」，与紧邻上方的 2026-09-27 记录完全相反 | 只是历史日志，不构成合同矛盾（正文 §8.2 已统一为三条件）；但两条相邻记录直接对立，读者可能误以为 §8.2 仍有两条件口径 | 可选：在该条末尾加半句「（2026-09-27 的 v1 内合同修订已取代此口径）」。非阻断 |
| RV1-DOCS-F6 | SUGGESTION | `plan.md` 的 Work Packages 表 WP5 行「Write Scope」 | 核对规划工件与已交付改动 | 预期：WP5 的写入范围包含 tasks 2.5 点名的 `openspec/specs/storage-schema-v2-migration/spec.md`。实际：该列只列三个 `docs/**` 文件，而交付中确实改了第 4 个文件（tasks 2.5 与 `verification.md` 均已正确登记） | 规划工件与执行步骤的表层不一致（不影响交付正确性；tasks 2.5 是步骤权威） | 可选：在 plan.md 的 WP5 行补上该文件。规划工件归主 Agent，本轮不修改 |

**附带判断（非发现）**：实现者报告「Issues」第 2 条（`LOCAL_ADMIN_PROTOCOL.md` §5.8 未补 `--export-id`）**判定为可接受**：design D10 只要求 §5.4 + 修订记录，§5.8 是「CLI 子命令 ↔ 方法」的唯一映射，本次 `node pair` 行的方法链（`begin` → `status` → `confirm`）未变；该表对 `--grant`/`--sas` 等既有旗标也未逐条列出，故不列 `--export-id` 与该表既有粒度一致。若主 Agent 想补，属可选文档增强。

## 5. Assessment

### 5.1 逐检查 ID 结论（计划 ↔ 实际）

| 检查 ID / 来源 | 核对方式 | 结论 |
| --- | --- | --- |
| [PV1] `check:docs`（`plan.md` Project Verify；`verification.md` Handoff Index 2.5 行） | 本人独立执行 `node scripts/check-doc-links.mjs`（C3） | **已核对，PASS 且独立复现**（exit 0，380 相对链接 / 5090 章节引用，与实现者报告及 `verification.md` 登记数字一致） |
| [PV1] `check:drift` | 本人独立执行（C4）：exit 1，仅 §7 块 1 第 20 条语句（`owned_node`）不一致；代码侧常量仍 3/3/3 | **待返回证据（PENDING）**：按契约记为 `result: BLOCKED`。文档侧文本本身已按 design D3 静态核对通过（V3），「文档与代码一致」须在 WP1 落地后的集成/候选阶段复跑。**该 PENDING 不影响 §4 的静态判断** |
| [PV1] `check:agentic` | 未执行（worktree 无 `node_modules`；不直接调用引擎以避开遥测与写入风险，见 §1 限制 3） | **待返回证据（PENDING）**：须在集成树补跑，确认 `openspec validate --all --strict` 接受 `openspec/specs/storage-schema-v2-migration/spec.md` 的 Purpose 改动 |
| [PV1] `check:boundaries` 及其余 6 项合同门禁 | 本次 diff 只含 4 个 Markdown 文件（C1）；`check:boundaries` 判据是 `cargo metadata` 的 crate 依赖，无 `Cargo.toml`/`crates/**` 变更即不受影响。其余 6 项实现者已跑（其报告与 `wp5-docs.log`），本人核对「资产未改」的反证（C9） | **已核对**：不受影响 / 反证成立（`schemas/**`、`fixtures/**`、`commands.json` 未被改动，见 5.4） |
| [PV3]/[PV4]/[PV5]（代码面） | 代码不在本分支（C10） | **NOT_APPLICABLE 于本 WP**：由 RV1-impl（tasks 3.2）与 3.1 的候选检查覆盖 |
| E2E | `plan.md` 记 `not-applicable`（用户批准） | NOT_APPLICABLE（文档 WP，不变更） |

### 5.2 design D1–D10 ↔ 文档逐条核对

| 设计决策 | 核对结论 |
| --- | --- |
| D1 三条件 / 两判定点同口径 | **部分**：三条件与「只能收窄、空清单 = 无可见」在 `NODE_LINK_PROTOCOL.md` §8.2（第 274 行）完整成立；「两个判定点」中的 catalog/`resource.attach` 一侧成立，**core 的 Owner 侧判定未在 `CORE_PORTS_AND_STORAGE.md` §6 第 5 条反映**（F1） |
| D2 JSON 列、不建关联表 | 成立：§7.3 加列、§11.7 归入集合列并指回 §8.2；DDL 中未新增表 |
| D3 `ALTER TABLE` + 末尾追加 + 常量 4/4/3 | **逐字一致**（见 5.3 V3/V4）；回滚口径已改 v4 |
| D4 校验在同一事务 + 三类失败 | 成立：§11.6 第 4 条完整（存在且未撤销 → `NotFound(EntityRef::Export)`/`local.not_found`；与本次 grants 无交集 → `InvalidRequest`/`local.invalid_params`；空集合不查 Export；失败零信任/零推进/零审计）；§5.4 与两端 spec 一致。仅缺 §9 判据行（F4） |
| D5 只在本机管理面、wire 不加字段 | 成立：§5.4 的 `params`/`result`/`NodeRecord` 与 §8.2「wire 不加字段」段；且准确保留了「JSON Schema 不表达必填、由运行期强制」的机器可判定边界（C9 证实 `params`/`result` 是通用对象） |
| D6 CLI 行为 | 文档侧只要求 §5.4（清单填报语义）+ 修订记录；CLI 参数细节属 WP4，本轮无文档遗漏（§5.8 不补为可接受，见 §4 附带判断） |
| D7 审计不新增 | 成立：§11.6 第 4 条明写「沿用 `node.paired`，不新增 `node.export_ids_changed`」，未动 `SECURITY_DESIGN.md` §14.2 |
| D8 兼容/已知限制/副作用 | **全部写明**：不兼容依据（`LOCAL_ADMIN_PROTOCOL.md` 1.4 记录：v1 未发布、CLI 与 Daemon 同版本发布、先例 `node.challenge.catalogRevision`）、「只在配对确认时填报、无修改入口（已知限制）」、升级副作用（v3 及更早库升 v4 后既有节点行清单为空、连接与握手仍正常、需重新确认）、Access 侧孤儿 Import 只文档化 |
| D9 测试策略 | 迁移面在 §9 判据 28 ①②与 §7.2 落实；**写集校验的判据缺行**（F4） |
| D10 影响面 | 除 §5.3/§6 第 5 条外全部落实；另有一处**超出**清单但合理的顺带同步（§7 标题、§9 判据 1、迁移资产/回滚的版本链），其必要性成立（否则文档会同时声称「当前版本 = 4」与「升级链止于 v3」） |

### 5.3 关键文本逐字/静态核对

| ID | 判据 | 结论 |
| --- | --- | --- |
| V1 | 15 个场景与文档逐条一致 | **全部有落点**：① 按信任记录过滤 → §8.2 条件②；② 清单收窄可见集 → 条件③+「只能收窄」；③ 空清单看不到任何 Export → 「空清单即看不到任何 Export——包括 `scopes` 与 `grants` 相交的那些」；④ 清单内 Export 被撤销后不可见且条目不被级联清理 → 「`export.revoke` 不级联清理清单条目…条目原样保留」；⑤ 超大批次稳定切分 → §12.3/§2.5 未变且本次声明 wire 不变；⑥ Owner 模式配对 → §5.4；⑦ 确认时填报并回显 → §5.4 `params`/`result`；⑧ 空清单仍建立信任 → §5.4「空清单合法」+ 三类失败不含空清单；⑨ 无效条目拒绝（两类码）→ §5.4 逐条对应；⑩ access 模式 `local.unsupported` → §5.4/§5.8 原文未动仍成立（R10 的文档面）；⑪ 连续两次打开文本不变 → §7.2；⑫ 中途失败整体回滚 → §7.2；⑬ v3→v4 既有行 `'[]'` → §7.2 v3→v4 段；⑭ 升级库/新建库 owned 列清单相等 → §7.2 + §9 判据 28 ①；⑮ v2→v3 保留审计与词表 → §7.2 + §9 判据 28 |
| V2 | §7.2 版本常量是否全量 4/4/3（扫描「当前 v3」「`user_version < 3`」「= 3 跳过」等） | **§7.2 内已全量**：第 855 行「当前 **v4 = 4**」/「`owned_schema_version`（当前 4）、`imported_schema_version`（当前 3）」；第 857 行 `< 4` + 三段链路 + `= 4` 跳过 + 新建库建成 v4；第 862–864 行落盘口径自洽；第 867/869 行资产与回滚改 v4；§7 标题、§9 判据 1/28 同步。**域外遗留 1 处**：§11.3 第 1451 行（F3，base 同文，非本次引入） |
| V3 | §7.3 `owned_node` 新列与 design D3 逐字一致 | **一致**：第 1052 行 `export_ids_json TEXT NOT NULL DEFAULT '[]',`（列名/类型/`NOT NULL`/`DEFAULT '[]'` 与 D3 相同）；位置在列清单**末尾**（`revoke_reason` 之后）、表级 `PRIMARY KEY`/CHECK 之前，即 `ALTER TABLE ADD COLUMN` 的列序位置。行内 `--` 注释不影响门禁（C8：SQL 归一化删 `--` 注释） |
| V4 | §9 判据 1/28 与 §7.2 自洽 | **自洽**：判据 1 升级链改 `v1 → v2 → v3 → v4`；判据 28 补 ①「升级库与新建库 owned 家族列清单（名/序/类型/`NOT NULL` 与默认值）逐项相等、`export_ids_json` 在末尾」②「v3 及更早库升级后既有节点行 `export_ids_json = '[]'`（不得默认放权）、其余列逐列不变」，与 §7.2 第 864 行、spec 场景 ⑬⑭ 表述一致 |
| V5 | §7.2 自身自洽（「v3 库只走 v4 段」等） | 文本自洽，但**对代码有硬要求**：现版 `migrate()` 只在 `file_version < FILE_FORMAT_VERSION` 内对 v2 段加了 `file_version < 2` 守卫，`V3_UPGRADE_OWNED`/`V3_UPGRADE_IMPORTED` **无条件执行**（C10，`crates/storage-sqlite/src/migrate.rs` 第 950 行判据、第 959 行 v2 守卫、第 963–964 行无守卫的 v3 两段）。常量改 4 后，v3 库会重跑一遍审计表 12-step 重建，违反本节新写的「v3 库只走 v4 段」与 §11.8「v4 只加列、不重建」→ 见 §6 跨 WP 交接项 1（属 WP1 义务，不是本 WP 的文档缺陷） |
| V6 | 修订记录体例与「不兼容变更」依据 | **合规**：`NODE_LINK_PROTOCOL.md` 新记录置于日期序首位、`>` 引用 + 行尾双空格、点名变更/日期/未实现未发布，与该文件既有体例一致；`LOCAL_ADMIN_PROTOCOL.md` 新增「版本：1.4」在 1.3 之上，体例与其 1.1–1.3 基本一致（仅日期后分隔符用「，」并多一句状态说明，非阻断）。不兼容依据成立：`§8` 明确把「给已有方法增加必填 `params` 字段」列为不兼容示例；「v1 未发布 + CLI 与 Daemon 同版本发布 + 先例 `node.challenge.catalogRevision`（该文件 2026-09-26 记录确有同一处理）」三段依据齐备，且未「静默」改变语义（已登记） |
| V7 | 封闭词表与合同资产未被误改 | **成立**：`git diff --name-only` 只有 4 个 md；`schemas/**`、`fixtures/**`、`compatibility/commands/v1/commands.json` 未改；§6 的方法集/错误码表未动；C9 证实 envelope schema 不表达逐方法 `required` |

### 5.4 全局一致性搜索（是否存在与本次相反的表述）

| 位置 | 内容 | 判定 |
| --- | --- | --- |
| `openspec/specs/node-link-owner-server/spec.md` 第 81 行 | 「首阶段的可见性规则…未撤销且 `export.scopes ∩ grants ≠ ∅`（§8.2 的 `exportIds` 字段推后，属后续阶段待办）」 | **待归档同步**（非本 WP 缺陷）：`AGENTS.md` §12 规定「归档后能力规范落入 `openspec/specs/…`」，本变更的增量规范 `specs/node-link-owner-server/spec.md` 的 MODIFIED 需求会在归档时替换它。**归档轮次必须逐字确认该行已被替换**，否则主规范会与 `docs/NODE_LINK_PROTOCOL.md` §8.2 相反 |
| `openspec/specs/storage-schema-v2-migration/spec.md` 第 11/16 行 | 仍写「一并推进到 3」「两族表结构版本都为 3」 | 同上待归档同步；**注意本文件已被 WP5 改了 Purpose（写 v4）**，因此在归档前该文件内部暂时不自洽（Purpose v4 ↔ Requirement v3）。这是 tasks 2.5 第 4 条的有意取舍（Purpose 不参与增量同步机制），但若变更最终未归档，主规范会被留在半改动状态 |
| `openspec/specs/local-admin-methods/spec.md` | 未含 `node.pair.confirm` 的 `exportIds` | 待归档同步（同上） |
| `openspec/changes/archive/2026-09-27-node-link-owner/**`（design/proposal/tasks/verification/specs、`reports/**` 多行） | 仍写「`exportIds` 推后/属后续阶段待办」「按 `exportIds` 过滤」等 | **历史归档，不得修改**（`AGENTS.md` §12 与归档语义） |
| `docs/CORE_PORTS_AND_STORAGE.md` §5.3 / §6 第 5 条 | 仍以两条件口径描述可见性/授权 | **F1（MAJOR）/ F2（MINOR）** |
| `docs/SECURITY_DESIGN.md`、`docs/INITIAL_DESIGN.md`、`docs/MODULE_ARCHITECTURE.md`、`docs/adr/**`、`README.md`、`AGENTS.md`、`compatibility/**` | 未出现「信任记录不落 `exportIds`」「可见性只有两个条件」等相反表述；`SECURITY_DESIGN.md` 只有「Access Node 不能扩大 Owner Export grant」这类同向不变量；`MODULE_ARCHITECTURE.md` §4.1 只把 `NodeRecord` 列为冻结类型名、§4.7 只描述表族/职责，不含列级或可见性口径 → 按 `AGENTS.md` §10 无需随动 | **未发现问题**（已核对） |
| 同一语义的重复定义 | 三条件出现在 `NODE_LINK_PROTOCOL.md` §8.2（权威）与 `LOCAL_ADMIN_PROTOCOL.md` §5.4（概要 + 链接），属可接受的「一处权威 + 一处概要并链接」；§5.3/§6 的过期复述构成第三/四处定义（F1/F2） | 见 F1/F2 |

### 5.5 `AGENTS.md` §13 与措辞（不得把未实现的能力写成已存在）

- 本次三处正文用**合同语气**（「信任记录**带** `exportIds`」「MUST/必填」），与该仓库「编码前契约」体例一致；实现状态由文件头承载：`NODE_LINK_PROTOCOL.md` 状态行明写节点侧状态机（含「Export 可见性与授权」）**尚未实现**，两条 2026-09-27 修订记录均写「v1 内合同修订，**未实现未发布**」；`LOCAL_ADMIN_PROTOCOL.md` 1.4 记录同样写「未实现未发布」。**未发现「能力已上线」式误述**。
- 限制条件已明确写出：本切片**无配对后修改入口**（改 = 撤销重配）、升级后既有配对需本机重新确认、Access 侧孤儿 Import 不自动清理、wire 与 fixture 不变。
- **残留风险（非本 WP 缺陷，交主 Agent）**：文档描述的运行期行为（缺少 `exportIds` → `local.invalid_params`）与**当前 main 二进制**相反（`crates/server/src/local_admin/params.rs` 第 741–743 行的 `reject_unknown_fields(params, &["pairingId","grants"])` 会把带 `exportIds` 的请求按未知字段拒绝）。因此**文档分支不得早于 WP1–WP4 单独合入 main**；同一处代码注释（`params.rs` 第 741 行 `（§5.4）：{ pairingId, grants }`）必须由 WP3（tasks 2.3）同批更新，否则会出现「注释引用的 §5.4 已变而注释未变」的隐性漂移（门禁不检查注释）。

## 6. 跨 WP 交接项（供主 Agent 转派，均不在本 WP 写入范围）

1. **WP1（tasks 2.1）必须给 v3 段加 `file_version < 3` 守卫**：§7.2 新文本要求「v3 库只走 v4 段」，而现版 `migrate()` 无条件执行 `V3_UPGRADE_OWNED`/`V3_UPGRADE_IMPORTED`（C10）。缺该守卫时 v3 库会多跑一次审计表 12-step 重建（与 §11.8「v4 只加列」相悖）；§9 判据 28 的现有断言（列清单相等、`'[]'`）**可能仍通过**，因此门禁与用例未必能捕获，建议 WP1 另加一条「v3 库升级不重建审计表」的断言（例如升级前后 `owned_audit` 的 `sqlite_sequence`/行 rowid 不变）。这也正是 `check:drift` 之外本变更唯一的代码侧已知风险。
2. **WP3（tasks 2.3）**：`crates/server/src/local_admin/params.rs` 的 `reject_unknown_fields` 白名单与第 741 行注释需与 §5.4 的新 `params` 形状同批更新（tasks 2.3 已点名 `params.rs`，此处只补充「注释也引用 §5.4」这一处）。
3. **归档轮次**：按 5.4 表逐行确认三份主规范已被增量同步替换（尤其 `openspec/specs/node-link-owner-server/spec.md` 第 81 行的两条件口径）。
4. **[PV1] 的 `check:drift` 与 `check:agentic`**：在集成树（有 `node_modules` + WP1 落地）复跑并登记；本报告的 PENDING 行须由该次执行覆盖。

## 7. handoff_index

```yaml
handoff_index:
  - task_id: "2.5"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "d6e01b3b3bbc71d4d6bd2edf34cc4dce3c0043fb"
    evidence_type: REVIEW
    evidence_id: RV1-DOCS
    report_path: "reports/rv1-docs.md"
    result: FAIL
    evidence_status: NEW
    applicability_basis: "新建只读 reviewer，worktree D:\\Project\\acp-remote-wt\\export-ids-docs、base 3cadb12d、target d6e01b3b 上自行读取完整 diff 与权威文档/规范/门禁脚本后给出；含 1 项 MAJOR（RV1-DOCS-F1）未解决，故 FAIL"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "d6e01b3b3bbc71d4d6bd2edf34cc4dce3c0043fb"
    evidence_type: CHECK
    evidence_id: "PV1 (check:docs)"
    report_path: "reports/rv1-docs.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本人于 target d6e01b3b 的 worktree 独立执行 `node scripts/check-doc-links.mjs`（node v24.19.0）→ exit 0，380 relative links / 5090 section refs，与实现者报告及 verification.md 登记逐字一致"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "d6e01b3b3bbc71d4d6bd2edf34cc4dce3c0043fb"
    evidence_type: CHECK
    evidence_id: "PV1 (check:drift)"
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "本人独立执行 `node scripts/check-contract-drift.mjs` → exit 1，仅 §7 块 1（OWNED_SCHEMA_V1）第 20 条语句（owned_node）不一致；代码侧 migrate.rs 仍为 v3 旧形状（常量 3/3/3），需 WP1（tasks 2.1）落地后在集成/候选阶段复跑（tasks 3.1、6.3）。待补 ID：PV1(check:drift)；门禁：check:contract-drift.mjs。该 PENDING 不影响本报告对文档文本的静态结论"
    source_evidence: NOT_APPLICABLE
```

## 8. 结论

**FAIL**（对应 target revision `d6e01b3b3bbc71d4d6bd2edf34cc4dce3c0043fb`）。

依据：§4 的 **RV1-DOCS-F1（MAJOR）** 未解决——本次同一改动在 `docs/CORE_PORTS_AND_STORAGE.md` §11.6 第 4 条新增了「清单只能收窄 Owner 侧命令授权」的句子，却把规定该判定内容的 §6 第 5 条留在两条件旧口径，形成文档内自相矛盾且与 design D1 不一致的授权口径，而 `check:contract-drift` 不覆盖该散文（C8），存在被实现/复核漏掉的安全相关授权缺口。次级项：F2/F3/F4（MINOR）与 F5/F6（SUGGESTION）。另有 [PV1] 的 `check:drift`、`check:agentic` 记为 **待返回证据（PENDING/BLOCKED）**，不影响上述文档静态结论，但排除 F1 后单凭 PENDING 也足以让本 WP 不能记 PASS。文档改动的其余部分（三条件、只收窄、空清单语义、撤销不级联、无修改入口、升级副作用、wire 不变、常量 4/4/3 与 §7.3 DDL 逐字一致、修订记录与不兼容依据、封闭词表与资产未改、措辞未把未实现能力写成已上线）经逐条核对成立。修复 F1 后请另开新 ID 复核。
