<!-- RV2-DOCS：WP5（tasks.md 2.5）文档工作包 fix 轮次后的**独立只读复核**（recheck）报告。字段与 handoff_index 组织见 openspec/schemas/agentic/roles/handoff.md。 -->

# 独立检视报告（复核）：RV2-DOCS / WP5 文档同步（task 2.5，fix 轮次）

| 字段 | 值 |
| --- | --- |
| `task_id` | 2.5 |
| `role` | reviewer（独立检视子 Agent，只读；本轮为 recheck） |
| `phase` | review |
| `stage` | work-package |
| `agent_context` | 新建子 Agent，**不继承**父 Agent 的需求/实现/上一轮 review 对话，未参与 WP5 的实现、fix 与 RV1-DOCS 的检视讨论；只消费调度者给出的复核输入（RV1-DOCS 报告、fixer 的 fix 轮次说明与日志、变更的 design/proposal/specs/plan/tasks/verification、`AGENTS.md`、被检视 worktree）与自身只读读取的文件与命令输出。本 Agent 无法证明宿主未注入其它上下文，只声明自身可见范围（见「Review Context / 限制」）。 |
| `evidence_type` | REVIEW |
| `evidence_id` | RV2-DOCS |
| `target_revision` | `426f8ae296d838e9843f0e9ca43a24680a2d60a9`（`agentic/node-trust-export-ids-docs` HEAD，已用 `git rev-parse HEAD` 核实；`git status --porcelain` 为空） |
| `base_revision` | `3cadb12d79456750e40644a2ea53c96380b700e6`（= `refs/heads/main`） |
| `recheck_of` | RV1-DOCS（`reports/rv1-docs.md`，被检版本 `d6e01b3b`，结论 FAIL：1 MAJOR + 3 MINOR + 2 SUGGESTION） |
| `scope` | 复核 diff `d6e01b3b..426f8ae2`（2 个文档文件，6 插入 / 5 删除）及其**受影响的完整上下文**，外加 `3cadb12..426f8ae2` 的整条文档 diff、仓库级一致性搜索与适用的只读门禁 |
| `changes` | 本次检视未修改任何代码/测试/规划文件；只在 `reports/` 下新增本报告 |
| `checks` | 见 §3；`check:docs` 与 `check:command-catalog` 独立复跑 PASS，`check:drift`/`check:agentic` 记待返（PENDING，跨 WP/环境） |
| `issues` | 原 5 项发现全部**已解决**（F6 由主 Agent 闭环）；新增 3 项（2 MINOR + 1 SUGGESTION），无 CRITICAL/MAJOR，见 §4 |
| `result` | **BLOCKED**——文档静态复核结论为 **PASS**（无未解决 CRITICAL/MAJOR、修复未引入回归），记 BLOCKED 的唯一原因是 tasks.md 2.5 的完成条件含 `[PV1]` 的 `check:drift`（`check:agentic` 同组），二者在本 worktree 必然 PENDING（详见 §8） |
| `evidence_paths` | 本报告 `openspec/changes/node-trust-export-ids/reports/rv2-docs.md`；被复核实现者证据（非本人证据，仅作对照）`reports/handoff-coder-docs.md` §8、`reports/wp5-docs-fix.log`；上一轮本人证据 `reports/rv1-docs.md` |
| `resource_cleanup` | 未创建目录联接、数据库、端口或长驻子进程；执行的全部命令为只读（`git`、`awk`/`sed`/`grep`/`diff`、`node scripts/check-doc-links.mjs`、`check-contract-drift.mjs`、`check-command-catalog.mjs`、`check-error-registry.mjs`、`check-features.mjs`、`check-contract-assets.mjs`、`check-acp-compatibility.mjs`，后四个因缺 `ajv` 未执行成功、只读退出）；两个临时文件 `/tmp/base7.txt`、`/tmp/new7.txt`（用于比对 NODE_LINK 第 7 行原文）已删除；被检视 worktree `D:\Project\acp-remote-wt\export-ids-docs` 复核后仍为干净（`git status --porcelain` 为空）；未提交、未切换分支、未改主检出受版本控制文件（主检出 `git status --porcelain` 仍只有未跟踪的 `?? openspec/changes/node-trust-export-ids/`）；未执行 E2E |

## 1. Review Context

- **Review ID**：RV2-DOCS（唯一，本轮）；**Review Type**：recheck；**Review Stage**：work-package（WP5 fix 轮次后）。
- **版本稳定性**：worktree HEAD = `426f8ae296d838e9843f0e9ca43a24680a2d60a9`（= 调度者给定 target），工作区干净；fix 轮次两个提交 `c7660100`（`docs(storage): 补齐 Owner 侧授权与可见性的 exportIds 清单条件`）→ `426f8ae2`（`docs(node-link): 标注 exportIds 推后口径已被 2026-09-27 修订取代`），均落在 `d6e01b3b` 之上，diff 可复现。
- **复核方法**：先读 RV1-DOCS 报告与 fixer 的 fix 轮次说明，再**重读修复版本及其受影响上下文**（`git diff d6e01b3..426f8ae` 全文 6/5 行 + §5.3/§6/§9/§10/§11.3 与 NODE_LINK 文件头、§8.2 的完整上下文），按原问题 ID 逐条判定，并另查 fix 自身是否引入回归（范围、封闭词表/合同资产、其他小节、口径一致性、过期复述）。
- **必要资料（只读读取）**：`design.md`（D1–D10，唯一口径）、`proposal.md`、三份增量规范、`plan.md`（Coverage Index R1–R18 抽样核对）、`tasks.md`（2.5 与 3.2 的判据）、`verification.md`（Target / Handoff Index / Check Plan Changes / Review Findings）。实现者自评只作对照，不作为证据；主 Agent 的复跑声明（`check-doc-links` = 380/5101、`git status` 干净）已由本人**独立复现**（§3 C3/C10），未据其声明推定。
- **`plan.md` 的 Code Review 判据（文档面）**：可见性两处必须同口径 → 本轮 F1/F2 的复核主线；迁移不得默认放权且列清单与新建库相等、`settle_pairing` 校验与写入同事务、CLI 警告不阻断、文档与 DDL/门禁一致 → 均属上一轮已核对项，本轮确认**未被 fix 触碰**（§6）。

### 限制与不确定性

1. **代码不在本版本内**：`crates/**` 在 `3cadb12..426f8ae2` 内零改动（WP1–WP4 在 worktree `D:\Project\acp-remote-wt\export-ids`）。因此「文档与实现一致」只能以 design/specs 冻结文本静态判断，「文档与 DLL（DDL）文本逐字相等」这一条由 `check:drift` 承载，本 worktree 必然 PENDING（§8）。
2. **`check:agentic` 无法执行**：本 worktree 无 `node_modules`，`check:agentic`（`scripts/agentic-gate.mjs` + 项目本地引擎）不可运行；本人不建立目录联接、不直接调用引擎（引擎默认开启遥测）。记为待返证据。
3. **需 `ajv` 的 4 道合同门禁无法在本 worktree 执行**：`check-error-registry`/`check-features`/`check-contract-assets`/`check-acp-compatibility` 均以 `ERR_MODULE_NOT_FOUND: Cannot find package 'ajv'` 退出（如实记录，未声称通过）。替代反证：这四道门禁的输入（`schemas/**`、`fixtures/**`、`compatibility/**`）在 `3cadb12..426f8ae2` 内**逐字节未变**（C1），其结论只由 base 内容决定；不需要 `ajv` 的 `check-command-catalog` 已在 target 上独立跑绿（C6）。
4. **E2E 不适用**：文档工作包，`plan.md` 已将变更级 Main E2E 记 `not-applicable`（用户 2026-09-27 批准）。
5. 本 Agent 未参与实现/fix，也不声称能看到宿主注入的全部上下文。

## 2. 本轮被复核的改动（changes under review）

复核对象 = fix 轮次 diff（`d6e01b3b..426f8ae2`，`--name-only` = 2 个文件，6 插入 / 5 删除）；括号内为归属的原问题 ID。

| 文件 | fix 改动摘要 | 归属 |
| --- | --- | --- |
| `docs/CORE_PORTS_AND_STORAGE.md` | ① §5.3 单点可见性策略括注由两条件改为三条件 + 指向权威处 + 句尾补「core 的 Owner 侧授权判定（§6 第 5 条）必须与它同口径」；② §6 第 5 条 Owner 侧 Export 判定补入「**在该节点信任记录 `exportIds` 清单内**的」+ 括注补「清单为空或目标 Export 不在清单内一律不放行」，同条后半句「只需前两条」改为「只需「信任 grant + 存在满足条件的 Export」（此处的「满足条件」含上面新增的清单条件，即 §8.2 的三条件）」；③ §9 插入判据 **32**；④ §10 的 2026-09-26 `[已裁定]` Owner 侧授权条末尾追加一句过期口径提示；⑤ §11.3「过新」用例取值 `3` → `5`（= `FILE_FORMAT_VERSION + 1`） | F2／F1／F4／fixer 自查／F3 |
| `docs/NODE_LINK_PROTOCOL.md` | 文件头第 7 行（2026-09-26 修订记录）保留原文逐字，仅在句尾追加「（其中 `exportIds` 推后一条已被 2026-09-27 的修订取代，见上一条）」 | F5 |

## 3. 检查范围与命令（自行执行，均为只读）

| # | 命令 / 方法 | 结果 | 建立的事实 |
| --- | --- | --- | --- |
| C1 | `git rev-parse HEAD`、`git status --porcelain`、`git diff --name-only/--stat 3cadb12..426f8ae`、`git diff d6e01b3..426f8ae` | HEAD = `426f8ae2`；工作区空；分支内共 4 个 md；fix diff 只有 2 个 md（6/5 行），仅有 5 个 hunk | 版本稳定；fix 范围与调度者声明一致；**fix 未触任何资产/代码** |
| C2 | 逐行读 fix diff 全文 + 改动后上下文（§5.3、§6 第 5 条、§9 判据 30/31/32、§10 末段、§11.3 末段、NODE_LINK 文件头与 §8.2） | 已读 | 逐条复核的基础 |
| C3 | `node scripts/check-doc-links.mjs`（worktree 根，node v24.19.0） | **exit 0**：`380 relative links, 5101 section refs across 324 markdown files` | **独立复现** fixer 与主 Agent 的 `check:docs` PASS（数字逐字一致），新增/改动引用（§8.2、§3.5、§6 第 5 条、§5.4、§7.2、§7.3）全部可解析 |
| C4 | `node scripts/check-contract-drift.mjs` | **exit 1**：唯一差异 = `§7 块 1 (OWNED_SCHEMA_V1) 第 20 条语句不一致`（`owned_node`） | 与上一轮逐字相同；fix **没有**改 §7 的 SQL 块（C1 的 hunk 列表可证），故未引入新的漂移；见 §8 |
| C5 | `awk '/^## 9\. 验收判据/,/^## 10\./' …` 提取 §9 条目号；`grep -n '^[0-9]\+\. \*\*'` 全文件行号；`git diff d6e01b3..426f8ae` 的 §9 hunk | `1 2 … 31 32` 连续、无重号无缺号；判据 30/31 在 diff 中为**上下文行**（措辞未动），新增仅 1 行 | F4 的编号与「不改既有编号」核对（见 §4） |
| C6 | `node scripts/check-command-catalog.mjs` | **exit 0**：`command catalog OK: 12 commands` | 封闭词表（含 §6 方法集/错误码与 `local.*` 能力三处一致）在 target 上仍绿 |
| C7 | `node scripts/check-error-registry.mjs`、`check-features.mjs`、`check-contract-assets.mjs`、`check-acp-compatibility.mjs` | 均 **exit 1**：`ERR_MODULE_NOT_FOUND: Cannot find package 'ajv'`（本 worktree 无 `node_modules`） | 如实记录为**未执行**（见 §1 限制 3）；替代反证 = 这些门禁的输入资产在 `3cadb12..426f8ae` 内零改动（C1） |
| C8 | `git log --format='%h' -- docs/CORE_PORTS_AND_STORAGE.md`（11 个提交）逐个 `git show … \| grep -cE '^[+-]> (版本\|修订)'` | 9 个提交有文件头版本/修订行改动；**唯二没有**的是本次两个提交 `d6e01b3`、`c766010`（fix diff 首个 hunk 在 148 行，文件头未动） | 新发现 RV2-DOCS-F1 的机制性依据（该文件既往体例：正文改动都在文件头留记录） |
| C9 | `git log -p … \| grep -E '^-.*(\[已裁定\]\|\[决定\]\|\[open\])'` 逐提交统计；`git log -S` 探针；读 `docs/IDENTITY_AND_AUTH_CONTRACT.md` 的 `[已废弃]` 块 | 历史上只有本次 `c766010` 删改过 `- \`[已裁定]\`` 行；既有 §10 就地改写仅见于 `[open]` → `[已裁定]` 的状态变化（`0978f09`）；同仓库有显式标注取代的先例（IDENTITY 文档用 `[已废弃]` 块**替换并删除**旧草案） | §5「fix 新增写法是否合体例」的判断依据 |
| C10 | `git status --porcelain`（fix 轮次两提交后）、`git show --format='%s%n%b'`、`git diff --check 3cadb12..426f8ae`、`sed -n '7p' … \| cat -A` | worktree 干净；两条提交信息为 `docs(storage): …` / `docs(node-link): …`（scope 均在 `commitlint.config.mjs` 的 `SCOPES` 内）；`--check` 仅报三条 `> 修订记录` 行的行尾两空格（= 该文件硬换行既有体例，**非**违规） | 交付卫生与「未误改」核对 |
| C11 | 比对 NODE_LINK 第 7 行：`git show d6e01b3:docs/NODE_LINK_PROTOCOL.md` 该行 vs target 该行（`sed` 剥离追加句后 `diff`） | 剥离追加句后逐字相同；追加句位于原句「）。」之后、行尾两空格之前 | F5「保留原文 + 仅补取代说明」逐字核实 |
| C12 | 仓库级反向表述搜索：`grep -rn "两个条件\|两条件\|只要求前两条\|推后\|可见性只取\|首阶段的可见性规则\|后续阶段待办\|不使用独立的"`（`docs/**`、`openspec/specs/**`） | 仅剩 3 处：`CORE_PORTS §10:1398`（**已标注**取代）、`NODE_LINK:7`（**已标注**取代）、`openspec/specs/node-link-owner-server/spec.md:81`（**未标注**，主规范 = 归档同步项，不属本 WP 写入范围） | §6 回归与过期复述核对 |

## 4. Findings

### 4.1 原问题逐条复核（沿用 RV1-DOCS-Fn）

| 原 ID | 级别 | 复核位置（target `426f8ae2`） | 复核结论 | 依据（重读修复版本后的判断） |
| --- | --- | --- | --- | --- |
| RV1-DOCS-F1 | MAJOR | `docs/CORE_PORTS_AND_STORAGE.md` §6 第 5 条（第 791 行） | **已解决** | Owner 侧判定已改为「…**并且**存在一个**在该节点信任记录 `exportIds` 清单内**的、未撤销的 `ExportRecord`——…（`export.scopes ∩ node.grants ≠ ∅`，与 Node Link 的可见性口径同源；**清单为空或目标 Export 不在清单内一律不放行**）…」。**后半句自洽**：原「因此只需前两条（信任 grant + 存在满足条件的 Export）」已改为「因此只需「信任 grant + 存在满足条件的 Export」（此处的「满足条件」含上面新增的清单条件，即 `NODE_LINK_PROTOCOL.md` §8.2 的三条件）」——「前两条」这一会与 §8.2 冲突的计数方式被删除，「满足条件」被就地定义为三条（Export 侧条件），故无「清单外 Export 仍可被命令选中」的口径缺口。三条件现已四处同口径：`NODE_LINK_PROTOCOL.md` §8.2（第 274 行）、本文件 §3.5（第 151 行）、§5.3（第 220 行）、§5.4 的引用（`LOCAL_ADMIN_PROTOCOL.md` 第 384 行），并与 §11.6 第 4 条「清单只能**收窄** Owner 侧的命令授权与 catalog 可见性」（第 1501 行）不再矛盾。 |
| RV1-DOCS-F2 | MINOR | 同文件 §5.3（第 220 行，单点可见性策略） | **已解决** | 括注已改为「（`D14`；2026-09-27 起为**三条件**：未撤销 ∧ `export.scopes ∩ 节点 grants ≠ ∅` ∧ `exportId ∈ 节点信任记录的 exportIds`，**权威定义见 `NODE_LINK_PROTOCOL.md` §8.2 与本文件 §3.5**）」——满足 `AGENTS.md` §10「保留一个权威定义，另一处只写概要并链接过去」；句尾新增「core 的 Owner 侧授权判定（§6 第 5 条）必须与它同口径」补上了 RV1 指出的限定（core 判的是**授权**，不是可见性投影），与 design D1「判定点仍然只有两处、且必须同口径」一致。 |
| RV1-DOCS-F3 | MINOR | 同文件 §11.3 末条（第 1449 行） | **已解决** | 已改为「「过新」用例使用高于新版本的值（`user_version = 5`，即 `FILE_FORMAT_VERSION + 1`）」。一致链核实：§7.2 第 855 行「当前 **v4 = 4**」、第 869 行「「版本过新拒绝启动」用例改在临时副本上把 `user_version` 顶到 `FILE_FORMAT_VERSION + 1`」、§9 判据 1（第 1325 行）同写法；`fixtures/storage/v2/too-new.sqlite3`（`user_version = 3`）是**历史夹具**、已被 §7.2 说明为「v3 之后不再「过新」」，未被误改。 |
| RV1-DOCS-F4 | MINOR | 同文件 §9（第 1359 行新增判据 **32**） | **已解决（编号按主 Agent 裁定 32，核对通过）** | 判据 32 主题「节点信任记录的 `exportIds` 清单」，来源 §5.4、§7.2、§7.3、§11.6 第 4 条、`NODE_LINK_PROTOCOL.md` §8.2，四条断言覆盖：① `settle_pairing` **同一事务内**校验（不存在/已撤销 → `NotFound(EntityRef::Export)`；`scopes` 与本次 `granted_grants` 无交集 → `InvalidRequest`；空集合合法且不查任何 Export 行；任一失败整事务回滚）② 读写在去重 + 字典序归一化后往返一致、空集合固定落 `'[]'` ③ 三条件同时成立、空清单不可见 ④ `export.revoke` 不级联清理条目——与 design D4/D9 与 §11.6 第 4 条逐条对应。**编号核对（任务书第 4 点的四项）**：㈠ §9 现为 `1…32` 连续、无重号无缺号（C5）；㈡ 既有 1–31 的编号与措辞未被改动——`git diff` 该处 hunk 为 `@@ -1356,6 +1356,7 @@`，以 `29./30./31.` 三条为**上下文行**、仅插入 `+32.` 一行，且全文再扫未见任何其它 §9 行被改（C5/C1）；㈢ 判据 31 仍是原「§10.3 的 view 身份与版本」（第 1358 行），故 §5.1 第 265 行的「§9 判据 31」与文件头第 17 行「§9 新增判据 31」两个交叉引用**仍指向正确的旧判据**（C5）；㈣ 全文无第二处「判据 32」引用，无编号冲突。主 Agent 裁定 32（而非 RV1 原文的 31）与仓库「不改既有判据编号」的约定一致。 |
| RV1-DOCS-F5 | SUGGESTION | `docs/NODE_LINK_PROTOCOL.md` 文件头第 7 行（2026-09-26 修订记录） | **已解决** | 原文**逐字保留**（C11：剥离追加句后与 `git show d6e01b3:` 该行完全相同），仅在句尾追加半句「（其中 `exportIds` 推后一条已被 2026-09-27 的修订取代，见上一条）」，行尾仍保留该文件修订记录惯用的两空格硬换行；「见上一条」确指紧邻上方的 2026-09-27 记录（第 5 行）。相邻两条历史记录不再被误读为「§8.2 仍为两条件」。 |
| RV1-DOCS-F6 | SUGGESTION | `plan.md` WP5 行 Write Scope（主检出） | **已解决（主 Agent 闭环，规划工件）** | `plan.md` 第 178 行的 WP5 Write Scope 已含 `openspec/specs/storage-schema-v2-migration/spec.md`（「仅 Purpose 勘误，呈 tasks.md 2.5」），与交付一致；`verification.md` 的 Check Plan Changes 亦登记了该范围调整。本轮不重复检视规划文本细节。 |

### 4.2 本轮新发现（RV2-DOCS-Fn）

| ID | 级别 | 位置（target `426f8ae2`） | 触发条件 | 预期 / 实际 | 影响 | 建议 |
| --- | --- | --- | --- | --- | --- | --- |
| RV2-DOCS-F1 | MINOR | `docs/CORE_PORTS_AND_STORAGE.md` 文件头（第 11–20 行的 `> 版本/修订` 记录块；最后一条是第 20 行的 `版本：0.13`，2026-09-26） | 按该文件既有体例查阅「当前合同版本与最近修订」 | 预期：本次变更（§3.5/§5.3/§6 第 5 条/§7.2/§7.3/§9/§10/§11.3/§11.6/§11.7/§11.8，含 v4 迁移与 `owned_node` 新列）在文件头留一条记录（`> 版本：0.14（2026-09-27，node-trust-export-ids：…）` 或等价的 `> 修订（…）`行）。实际：文件头**完全未动**（C8：11 个触及该文件的提交中，9 个改了文件头版本/修订行，唯二没有的正是本次的 `d6e01b3` 与 `c766010`）。**非本轮 fix 引入**，自 `d6e01b3` 起缺失；RV1-DOCS 未发现 | 该文件头是读者/归档者判断「当前版本与最近修订」以及「本次改了哪几节」的位置。缺失后：① 本次「v4 迁移 + 三条件收窄 + 判据 32」只能靠正文逐节发现；② 与本变更对 `NODE_LINK_PROTOCOL.md`（补 2026-09-27 修订记录）、`LOCAL_ADMIN_PROTOCOL.md`（补 `版本：1.4`）的处置不一致；③ §7.3 的 DDL/§7.2 的常量已先行而代码未落地（`check:drift` 仍红）这一「合同先行」的状态没有任何文件头记录，读者无法从文件自身判断；④ fix 轮次选择在 §10 历史记录里内联提示取代关系（§5），而本文件记录修订的既有位置其实是文件头，取舍略有错位 | 在文件头追加一条版本/修订行（如 `> 版本：0.14（2026-09-27，node-trust-export-ids：§3.5/§5.3/§6 第 5 条/§7.2/§7.3/§9/§10/§11.3/§11.6/§11.7/§11.8；可见性三条件与清单收窄、v3→v4 只加列、判据 32。§7 的 SQL 块与 §7.3 DDL 先行，漂移门禁随 WP1 落地复跑）`）。一行文本；`tasks.md` 2.5 只点名了另两个文件的文件头记录，故这是否在本轮补、还是留待归档轮次，由主 Agent 裁定。**不阻断**（文档内部一致性与可追溯性，非合同语义缺陷） |
| RV2-DOCS-F2 | MINOR | `openspec/changes/node-trust-export-ids/verification.md` 第 38 行（Check Plan Changes「review 驱动的范围调整」第 4 条） | 归档/最终验收按 Check Plan Changes 定位 F4 的落点 | 预期：与同文件 Review Findings 行（第 57 行「追加为**判据 32**」）及 §9 实际（第 1359 行的第 32 条）一致。实际：第 38 行写「…、新增 **§9 判据 31**，并在 `NODE_LINK_PROTOCOL.md` 的历史修订行补半句取代说明」 | 证据登记内部自相矛盾：按第 38 行查找「判据 31」会落到第 1358 行的 view 判据（本次并未改动它），可能被误判为「F4 的判据已存在」或指错复核对象。不影响文档合同内容与交付方向 | 把第 38 行的「新增 §9 判据 31」改为「新增 §9 判据 32」（该文件由主 Agent 维护，本人不修改）。**不阻断** |
| RV2-DOCS-F3 | SUGGESTION | `docs/CORE_PORTS_AND_STORAGE.md` §9 判据 32 的 ③ 与来源列表（第 1359 行） | 需要一条判据覆盖 F1 修复所在的 **core 侧**授权判定 | 预期：design D9 明确要求 `core` 的 `Broker` Owner 侧授权在「清单为空」「清单不含目标 Export」时必须拒绝，并称其为「**核心断言**：清单为空时，即使 `scopes ∩ grants` 非空也不可见/**不可用**」。实际：判据 32 的 ③ 只断言**可见性**（适配器 catalog 面，来源含 `NODE_LINK_PROTOCOL.md` §8.2），来源列表不含 §6 第 5 条，「不可用」这一半没有被这条判据显式断言 | 有限：§9 历来就不单独断言 core 授权（整个判据列表没有 §6 第 5 条的判据行，属既有口径），且 tasks 3.2 的代码 review 判据已写「可见性两处必须同口径（清单为空**既看不到也不可用**）」，因此实际覆盖不落空；此处只是「[PV3] 的判据来源不如 tasks 3.2 明确」的可选补强 | 可选：在 ③ 后补半句或把 §6 第 5 条加入来源——「core 的 Owner 侧授权必须同口径：清单为空或目标 Export 不在清单内时，即使其 `scopes ∩ grants` 非空也必须拒绝（不可用）」。**不阻断** |

## 5. fix 轮次新增改动（§10 历史记录内追加「过期口径提示」）的体例判断

任务书第 6 点要求判断这种写法是否符合本文件既有体例、是否与当前口径一致、是否应改为其他写法。结论：**写法可接受、无新的自相矛盾，不需要改为重写历史；但建议与缺失的文件头版本行（RV2-DOCS-F1）配套**。逐项依据：

1. **事实性**：追加句只陈述「本条是 2026-09-26 的裁定记录；2026-09-27 的 `node-trust-export-ids` 变更在该 Export 判定上增加清单条件——目标 Export 还须在该节点信任记录的 `exportIds` 内，当前口径以 §6 第 5 条与 `NODE_LINK_PROTOCOL.md` §8.2 为准」。与 §6 第 5 条（本次已补清单条件）、§8.2、§11.6 第 4 条完全一致；它**不复述条件**、不新增语义，只做历史标注与指向，因此没有制造第三套口径。该条正文（含「无会话命令只要求前两条」）被保留，但已被同一 bullet 内的标注限定为历史记录 —— 这是消除「同一文档内两套口径」（RV1-F1 的核心风险）的最小非破坏做法。
2. **是否属于本文件既有体例**：本文件对「修订了什么」的常规记录位置是**文件头**（`> 版本：0.x`/`> 修订（…）` 行，0.3–0.13 共 11 条），而 §10 的 `[已裁定]` 条目此前**从未**被追加过取代说明（C9：历史上只有本次 `c766010` 删改过 `- \`[已裁定]\`` 行；§10 的就地改写此前只发生在 `[open]` → `[已裁定]` 的状态变化，如 `0978f09`）。因此「在历史条目里内联标注取代」在本文件是**新写法**，但它与仓库内既有的显式取代标注先例同源：`docs/IDENTITY_AND_AUTH_CONTRACT.md` 第 81 行用 `[已废弃]` 块标注旧草案「已被下面的「已实现形状」取代并**删除**」。本处选择「保留原文 + 标注取代」而非删除，更契合 §10 作为**裁定记录**（时间线）的性质，故我判定为可接受，**不建议**改为重写条目或删段。
3. **与当前口径的自洽性**：三条件口径现已在 §8.2 / §3.5 / §5.3 / §6 第 5 条 / §11.6 第 4 条 / §5.4（`LOCAL_ADMIN_PROTOCOL.md`）/ §9 判据 32 一致，唯一仍写两条件的两处（本 §10 条与 `NODE_LINK_PROTOCOL.md` 第 7 行历史修订记录）**都已带取代标注**（C12）。未发现新的自相矛盾。
4. **可改进之处**：本文件记录修订的既有位置是文件头，而本次文件头是空的（RV2-DOCS-F1）。补上文件头行后，本条内联句可原样保留（它只标注历史、不加新语义）；若主 Agent 认为历史记录应当完全不被触碰，替代写法是在文件头行里点名「§10 的 2026-09-26 授权裁定已被本变更收窄」，但这会削弱「读者在 §10 就地看到该条已过期」的即时性——两者可并存，成本各一行。

## 6. 回归检查（任务书第 7 点）

| 检查项 | 方法 | 结论 |
| --- | --- | --- |
| fix 是否只动两个文档文件 | `git diff --name-only d6e01b3..426f8ae`（C1） | **成立**：仅 `docs/CORE_PORTS_AND_STORAGE.md`、`docs/NODE_LINK_PROTOCOL.md`（6 插入 / 5 删除）；分支总范围 `3cadb12..426f8ae` 仍只有 4 个 md（三份 docs + 一份主规范 Purpose） |
| 是否误改封闭词表/合同资产 | `git diff --name-only 3cadb12..426f8ae` + `check-command-catalog`（C1/C6） | **未误改**：`schemas/**`、`fixtures/**`、`compatibility/**`、`crates/**` 在整条分支范围内零改动；封闭词表门禁在 target 上 exit 0（12 commands）。需 `ajv` 的 4 道资产门禁本 worktree 无法执行（C7，如实记为未执行），其输入资产零改动故不受影响 |
| 是否误改其它小节 | fix diff 逐 hunk 核对（C1/C2） | **未误改**：5 个 hunk 恰好落在 §5.3、§6 第 5 条、§9（插入判据 32）、§10（追加句）、§11.3 与 NODE_LINK 文件头第 7 行；§7 的 SQL 块、§5 的 ```rust 块（第 36–53 行围栏，第 220 行在其外）、§6 方法集/错误码表、§5.4 与 §11.6 主体均未被触碰 |
| 交付卫生 | `git status --porcelain`、两条提交信息、`git diff --check`（C10） | **通过**：worktree 干净；两条提交为 `docs(storage): …`/`docs(node-link): …`，类型与 scope 均在 `commitlint.config.mjs` 词表内；`--check` 只报 `> 修订记录` 行的行尾两空格——与这些行既有硬换行体例一致，非违规 |
| 过期复述再搜（除已标注的历史记录外） | C12 仓库级 grep（`docs/**`、`openspec/specs/**`） | **只剩 3 处**：`CORE_PORTS §10:1398`（已标注取代 ✓）、`NODE_LINK:7`（已标注取代 ✓）、`openspec/specs/node-link-owner-server/spec.md:81`（**未标注**——主规范，属**归档同步**项，RV1-DOCS §5.4 已登记，不在 WP5 写入范围；归档轮次必须逐字确认该行被增量规范替换）。`docs/**` 内已无「可见性只取 / 首阶段的可见性规则 / 后续阶段待办 / 不使用独立的 `exportIds`」等表述；`docs/CONFIG_REFERENCE.md` 的 `export_ids`（`import add` 的 Access 侧 Import 明细）与 Owner 侧信任记录的 `exportIds` 不是同一字段，非过期复述 |
| 结论 | — | 修复**未引入回归**，也未留下未标注的过期复述（除主规范那一处已登记的待归档项） |

## 7. 附带判断（非发现）

1. **§6 第 5 条括注里的「与 Node Link 的可见性口径同源」**：RV1 建议「去掉/改写这一现在不完整的括注」，fix 保留了它，但把「清单为空或目标 Export 不在清单内一律不放行」并入同一括注、且清单条件已写入主句，因此该括注不再构成不完整的复述（「同源」现在只用于说明 ∩ 条件这一条与 §8.2 的同源关系，且 §8.2 的三条件在主句与后半句均被点名）。若追求极致清晰，可写「（即 §8.2 三条件中与 grants 有关的那条；清单为空或目标 Export 不在清单内一律不放行）」，纯属可选精化，**不构成发现**。
2. **§5.3 同句内「不在 core 重复实现」与新增的「core 必须同口径」并读**：前者针对**可见性投影**（design D1 的单点 `visible_exports`），后者针对 **core 的授权判定**（D1 的第二个判定点），二者在 D1 中本就是「两个判定点、必须同口径」；文本未自相矛盾，只是两句紧邻易生歧义。若主 Agent 认为值得，可在该句内补「（读面由适配器先过滤；core 的命令授权判定同口径）」，**不构成发现**。
3. **`D14` 标签的保留**：§5.3 括注仍以 `D14` 标识来源（D14 出自已归档变更 `2026-09-27-node-link-owner` 的 design.md，其原口径为两条件）。fix 已用「2026-09-27 起为三条件」限定，且给了权威文档指向，因此对读者的误导风险很低；若将来有读者想追溯 `D14`，需注意那是**已被本变更收窄**的历史裁决。**不构成发现**（RV1 亦未把它列为独立问题）。

## 8. Assessment

### 8.1 逐检查 ID 结论（计划 ↔ 实际，target `426f8ae2`）

| 检查 ID / 来源 | 核对方式 | 结论 |
| --- | --- | --- |
| [PV1] `check:docs`（`plan.md` Project Verify；`verification.md` fix 轮次行） | 本人独立执行 `node scripts/check-doc-links.mjs`（C3） | **已核对，PASS 且独立复现**（exit 0，380 相对链接 / 5101 章节引用，与 fixer 报告与 `verification.md` 登记逐字一致；上一轮为 5090，新增 11 处引用全部可解析） |
| [PV1] `check:drift` | 本人独立执行（C4）：exit 1，唯一差异仍是 §7 块 1 第 20 条语句（`owned_node`） | **待返回证据（PENDING）**：与上一轮逐字相同，因代码侧 `crates/storage-sqlite/src/migrate.rs` 仍是 v3 形状（WP1 未落地）。**fix 未改 §7 的 SQL 块**（C1 的 hunk 列表），故未引入新漂移；PASS 须在集成/候选阶段复跑（tasks 3.1、6.3）。本 PENDING **不影响** §4 的静态结论 |
| [PV1] `check:agentic` | 未执行（worktree 无 `node_modules`，见 §1 限制 2） | **待返回证据（PENDING）**：须在集成树补跑（含 `openspec validate --all --strict` 对本次主规范 Purpose 改动的接受性） |
| [PV1] 其余合同门禁 | `check-command-catalog` 独立跑绿（C6）；需 `ajv` 的 4 道如实记为未执行（C7），并以「资产零改动」反证（C1） | **已核对/反证成立**：封闭词表未受影响；资产门禁的输入逐字节未变 |
| [PV3]/[PV4]/[PV5]（代码面） | 代码不在本分支（§1 限制 1） | **NOT_APPLICABLE 于本 WP**：由 tasks 3.2（代码 reviewer）与 3.1/6.3 的候选检查覆盖 |
| E2E | `plan.md` 记 `not-applicable`（用户批准） | NOT_APPLICABLE（文档 WP，不变更） |

### 8.2 design D1–D10 复核（仅记与本轮修改相关的部分）

| 设计决策 | 复核结论 |
| --- | --- |
| **D1 三条件 / 两个判定点同口径** | **现已成立**（本轮修复的关键）：§8.2（权威）+ §6 第 5 条（core 授权侧）+ §5.3（单点策略，指向权威）+ §5.4/§11.6 第 4 条/§9 判据 32 全套一致；「清单为空或不在清单内一律不放行」在 §6 第 5 条显式写出。仅剩 RV2-DOCS-F3 的可选补强（判据 32 未显式断言 core 侧的「不可用」） |
| D4 校验同一事务 + 三类失败 | 成立（§11.6 第 4 条 + §9 判据 32 ①，本轮未改动这两处的语义） |
| D9 测试策略 | 迁移面（判据 28 ① ②）与写集面（判据 32）均已落判据；core 面见 RV2-DOCS-F3（可选） |
| D2/D3/D5/D6/D7/D8/D10 | 与 RV1-DOCS §5.2 的结论一致，本轮 fix **未触碰**相关小节（§3.5/§7.2/§7.3/§11.6/§11.7/§11.8/§5.4/文件头修订记录均不在 fix diff 内），无需重新判定其正确性 |

### 8.3 结论

- **文档静态复核结论：PASS**。RV1-DOCS-F1（MAJOR）与 F2/F3/F4（MINOR）、F5（SUGGESTION）**全部已解决**，F6 由主 Agent 闭环；fix 未引入回归（范围、资产/词表、其它小节、口径一致性、过期复述均已核对）；新发现只有 2 项 MINOR（均属文档内部一致性/证据登记，非合同语义）与 1 项 SUGGESTION，**无未解决的 CRITICAL/MAJOR**。
- **本报告 `result`：BLOCKED**。唯一依据是 `handoff.md` 的「PENDING 的 `result` 必须为 BLOCKED」「INVALID/PENDING 不能完成受影响任务」与 `tasks.md` 2.5 的完成条件（`[PV1]` 的 `check:docs`/`check:drift` 通过）：`check:drift`（及同组 `check:agentic`）在本 worktree 必然 PENDING，须在 WP1/WP3 代码并入后的集成/候选阶段复跑（tasks 3.1、6.3；`verification.md` 已为两个 revision 登记 BLOCKED/PENDING 行）。这与 RV1-DOCS 的处置一致（它也判定「排除 F1 后单凭 PENDING 也足以让本 WP 不能记 PASS」）。
- **给主 Agent 的呈报口径**：若按 `verification.md` 既有的「跨 WP 延后」处置接受把 `check:drift`/`check:agentic` 留给 3.1/6.3，则 2.5 的文档面可按 **PASS** 完成（RV2-DOCS 行可记 PASS/NEW，另保留两条 PENDING 检查行）；若严格按 `tasks.md` 2.5 的完成条件执行，2.5 在补齐这两项前记 **BLOCKED**。两条路径都不需要新的文档修复轮次——新发现的两项 MINOR（文件头版本行、`verification.md` 的「判据 31」笔误）各一行文本，可由主 Agent 在归档/合入前顺带处理，或明确接受现状。

## 9. handoff_index

```yaml
handoff_index:
  - task_id: "2.5"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "426f8ae296d838e9843f0e9ca43a24680a2d60a9"
    evidence_type: REVIEW
    evidence_id: RV2-DOCS
    report_path: "reports/rv2-docs.md"
    result: BLOCKED
    evidence_status: NEW
    applicability_basis: "对 RV1-DOCS（target d6e01b3b，FAIL）的 recheck。新建只读 reviewer，未继承实现/fix/RV1 检视对话；在 worktree D:\\Project\\acp-remote-wt\\export-ids-docs、base 3cadb12d、target 426f8ae2 上重读 fix diff（d6e01b3b..426f8ae2，2 文件 6/5 行）与受影响上下文、§9 编号与交叉引用、design D1–D10、tasks/plan/verification 后给出。RV1-DOCS-F1…F5 全部判定已解决、F6 由主 Agent 闭环，无新 CRITICAL/MAJOR（文档静态结论 PASS）；记 BLOCKED 的唯一原因是 tasks 2.5 完成条件含 [PV1] 的 check:drift（及同组 check:agentic），二者在本 worktree 必然 PENDING（见本报告 §8.3 的两条呈报口径）"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "426f8ae296d838e9843f0e9ca43a24680a2d60a9"
    evidence_type: CHECK
    evidence_id: "PV1 (check:docs)"
    report_path: "reports/rv2-docs.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本人于 target 426f8ae2 的 worktree 独立执行 `node scripts/check-doc-links.mjs`（node v24.19.0）→ exit 0，380 relative links / 5101 section refs across 324 markdown files；与 fixer 报告及 verification.md 登记逐字一致（上一轮 5090，新增 11 处引用全部可解析）"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "426f8ae296d838e9843f0e9ca43a24680a2d60a9"
    evidence_type: CHECK
    evidence_id: "PV1 (check:drift)"
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "本人独立执行 `node scripts/check-contract-drift.mjs` → exit 1，唯一差异仍是 §7 块 1（OWNED_SCHEMA_V1）第 20 条语句（owned_node），因本 worktree 的 crates/storage-sqlite/src/migrate.rs 仍是 v3 旧形状（WP1/tasks 2.1 未落地）；fix 的 5 个 hunk 未含 §7 SQL 块，故未引入新漂移。待补 ID：PV1(check:drift)；门禁：check-contract-drift.mjs；须在 WP1/WP3 并入后的集成/候选阶段复跑（tasks 3.1、6.3）。执行记录见本报告 §3 C4"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "426f8ae296d838e9843f0e9ca43a24680a2d60a9"
    evidence_type: CHECK
    evidence_id: "PV1 (check:agentic)"
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "未执行：本 worktree 无 node_modules（check:agentic = scripts/agentic-gate.mjs + 项目本地引擎），本人不建立目录联接、不直接调用引擎（引擎默认开启遥测）。待补 ID：PV1(check:agentic)；须在集成树（有 node_modules）补跑，并确认 `openspec validate --all --strict` 接受 openspec/specs/storage-schema-v2-migration/spec.md 的 Purpose 改动"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "426f8ae296d838e9843f0e9ca43a24680a2d60a9"
    evidence_type: CHECK
    evidence_id: "WP5-SUPP (check:command-catalog)"
    report_path: "reports/rv2-docs.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本人于 target 426f8ae2 独立执行 `node scripts/check-command-catalog.mjs` → exit 0（12 commands），证明 fix 未误改封闭词表（§6 方法集/错误码与 local.* 能力三处一致）。作为「合同资产未改动」的反证之一；另 4 道需 ajv 的资产门禁在本 worktree 以 ERR_MODULE_NOT_FOUND 退出（如实记为未执行），其输入 schemas/**、fixtures/**、compatibility/** 在 3cadb12..426f8ae2 内逐字节未变"
    source_evidence: NOT_APPLICABLE
```

## 10. 结论

**文档静态复核：PASS**（RV1-DOCS-F1…F5 全部已解决、F6 主 Agent 闭环；fix 未引入回归；新发现 RV2-DOCS-F1/F2（MINOR）与 RV2-DOCS-F3（SUGGESTION），无 CRITICAL/MAJOR）。

**本报告 result：BLOCKED**（原因仅 [PV1] 的 `check:drift` 与 `check:agentic` 在本 worktree 必然 PENDING，须在集成/候选阶段复跑；见 §8.3 的两条呈报口径）。修复轮次不需要新的文档修复；两项 MINOR 均为一行文本且分属文件头与 `verification.md`，可由主 Agent 决定是否在归档/合入前顺带处理。
