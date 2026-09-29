<!-- RV3-DOCS：WP5（tasks.md 2.5）文档工作包**fix 轮次 2 后的范围受限复核（recheck）**报告。字段与 handoff_index 组织见 openspec/schemas/agentic/roles/handoff.md。 -->

# 独立检视报告（范围受限复核）：RV3-DOCS / WP5 文档同步（task 2.5，fix 轮次 2）

| 字段 | 值 |
| --- | --- |
| `task_id` | 2.5 |
| `role` | reviewer（独立检视子 Agent，只读；本轮为 recheck） |
| `phase` | review |
| `stage` | work-package |
| `agent_context` | 新建子 Agent，**不继承**父 Agent 的实现/fix/RV1/RV2 对话；只消费调度者给出的复核输入（RV2-DOCS 报告、fixer 的 fix 轮次 2 说明与日志、变更的 design/specs/plan/tasks/verification、`AGENTS.md`、被检视 worktree）与自身只读读取的文件与命令输出。本 Agent 未参与本轮 fix，自评文本只作对照、不作为证据。 |
| `evidence_type` | REVIEW |
| `evidence_id` | RV3-DOCS |
| `target_revision` | `7d5abeb8c2372ccd77a1d35cc0a4f580bd648054`（worktree HEAD，已用 `git rev-parse HEAD` 核实；`git status --porcelain` 为空） |
| `base_revision` | `3cadb12d79456750e40644a2ea53c96380b700e6`（= `refs/heads/main`） |
| `recheck_of` | RV2-DOCS（`reports/rv2-docs.md`，被检版本 `426f8ae2`，结论 BLOCKED／文档静态 PASS，新增 RV2-DOCS-F1（MINOR）、F3（SUGGESTION）） |
| `scope` | **受限**：只复核 fix 轮次 2 的 diff `426f8ae2..7d5abeb8`（1 个文件、3 插入 1 删除）及其受影响上下文（文件头版本行、§9 判据 32），外加 `3cadb12..7d5abeb` 的范围回归核对与可在本环境运行的只读门禁；**不重新展开**全量检视 |
| `changes` | 本次检视未修改任何代码/测试/规划/被检视文档；只在 `reports/` 下新增本报告 |
| `checks` | 见 §3：`check-doc-links` = exit 0（380 链接 / 5114 章节引用，独立复现）、`check-command-catalog` = exit 0、`check-contract-drift` = exit 1（跨 WP 已知 PENDING）、`check:agentic` 未执行（无 `node_modules`），后两项记为 PENDING |
| `issues` | RV2-DOCS-F1 **已解决**（新版本行已补齐；但新版本行自身的两处节号/内容陈述不准，见新发现 RV3-DOCS-F1）、RV2-DOCS-F3 **已解决**；新增 2 项（1 MINOR + 1 SUGGESTION），无 CRITICAL/MAJOR |
| `result` | **BLOCKED**——文档静态复核结论为 **PASS**（两条原问题均已解决、修复未引入回归），记 BLOCKED 的唯一原因是 tasks.md 2.5 的完成条件含 `[PV1]` 的 `check:drift`（及同组 `check:agentic`），二者在本 worktree 必然 PENDING（见 §7.3） |
| `evidence_paths` | 本报告 `openspec/changes/node-trust-export-ids/reports/rv3-docs.md`；被复核对象 `reports/rv2-docs.md`、`reports/rv1-docs.md`；实现者证据（非本人证据，仅作对照）`reports/handoff-coder-docs.md` §9、`reports/wp5-docs-fix.log`「第二轮 fix」 |
| `resource_cleanup` | 未创建目录联接、数据库、端口或长驻子进程；全部命令为只读（`git`、`sed`/`grep`/`diff`、`node -e` 只读片段、`node scripts/*.mjs`）；用到的两个临时文件（`/tmp/old32.txt`、`/tmp/new32.txt`，用于判据 32 逐字比对）已删除并核实不存在；被检视 worktree 复核后仍干净（`git status --porcelain` 为空）；未提交、未切分支、未改主检出受版本控制文件（主检出仍只有未跟踪的 `?? openspec/changes/node-trust-export-ids/`）；未执行 E2E |

## 1. Review Context

- **Review ID**：RV3-DOCS（唯一，本轮）；**Review Type**：recheck；**Review Stage**：work-package（WP5 fix 轮次 2 后）；**范围**：RV2-DOCS 的两条新发现 + 该修复的回归面。
- **版本稳定性**：worktree `D:\Project\acp-remote-wt\export-ids-docs` HEAD = `7d5abeb8`（= 调度者给定 target），工作区干净；fix 轮次 2 为单提交 `7d5abeb8`（`docs(storage): 补文件头 0.14 版本行并为判据 32 补 core 侧授权断言`），落在 `426f8ae2` 之上，diff 可复现（2 个 hunk）。
- **复核方法**：先读 RV2-DOCS 报告与 fixer 的 fix 轮次 2 说明，再**重读修复版本与受影响上下文**（文件头 1–23 行、§4/§5/§5.3/§5.4、§6 第 5 条、§7.2/§7.3、§9 全部 32 条及其交叉引用、§11.6 第 4 条、§11.7/§11.8），然后按原问题 ID 逐条判定「已解决/未解决/无法确认」，最后专查本轮修复是否引入回归（范围、合同资产、其它小节、版本行体例与准确性、判据编号与交叉引用）。
- **必要资料（只读读取）**：`design.md` D1/D9、`specs/storage-schema-v2-migration/spec.md` 的 R11–R15 口径（经 §7.2/§9 判据 28 复核）、`specs/node-link-owner-server` 相关增量、`plan.md`、`tasks.md` 2.5、`verification.md`（Target / Handoff Index / Check Plan Changes / Review Findings）、`AGENTS.md` §10/§13、`docs/NODE_LINK_PROTOCOL.md` §8.2（target 版）。
- **本轮只读的跨 worktree 对照（非本分支证据，仅用于判断文件头陈述是否属实）**：`D:\Project\acp-remote-wt\export-ids`（分支 `agentic/node-trust-export-ids`，**未提交工作区**）的 `crates/storage-sqlite/src/migrate.rs` 常量（`FILE_FORMAT_VERSION = 4`、`OWNED_SCHEMA_VERSION = 4`、`IMPORTED_SCHEMA_VERSION = 3`）与 `owned_node` DDL（`export_ids_json TEXT NOT NULL DEFAULT '[]'` 在列清单末尾），以及 `crates/core/src/ports.rs` **未被修改**（`git status --porcelain` 未列出该文件）。该对照只用于核实文件头文字，**不构成** `check:drift` 证据。

### 限制与不确定性

1. **代码不在本版本内**：`crates/**` 在 `3cadb12..7d5abeb` 内零改动（WP1–WP4 在另一 worktree），因此「文档与实现逐字一致」只能由 `check:drift` 承载，本 worktree 必然 PENDING（§3 C9、§7.3）。§1 的跨 worktree 对照是我自行读取的另一分支**未提交**内容，仅作旁证。
2. **`check:agentic` 无法执行**：本 worktree 无 `node_modules`（`scripts/agentic-gate.mjs` + 项目本地引擎不可运行）；本人不建立目录联接、不直接调用引擎（引擎默认开启遥测）。记为待返证据。
3. **需 `ajv` 的四道资产门禁未执行**（`check-error-registry`/`check-features`/`check-contract-assets`/`check-acp-compatibility`，RV2 已如实记录以 `ERR_MODULE_NOT_FOUND` 退出）；替代反证 = 这些门禁的输入（`schemas/**`、`fixtures/**`、`compatibility/**`）在 `3cadb12..7d5abeb` 内**零改动**（C1），不需要 `ajv` 的 `check-command-catalog` 已在 target 上独立跑绿（C8）。
4. **E2E 不适用**：文档工作包，`plan.md` 已记 `not-applicable`（用户 2026-09-27 批准），本轮不改变该判断。
5. 本 Agent 未参与实现/fix，也不声称能看到宿主注入的全部上下文。

## 2. 本轮被复核的改动（changes under review）

复核对象 = fix 轮次 2 的 diff（`426f8ae2..7d5abeb8`，`--name-only` = **1 个文件**，3 插入 / 1 删除，**2 个 hunk**）：

| # | hunk | 位置（target `7d5abeb8`） | 改动 | 归属 |
| --- | --- | --- | --- | --- |
| 1 | `@@ -21,0 +22,2 @@`（**纯插入**：新行 + 空行） | `docs/CORE_PORTS_AND_STORAGE.md` 文件头第 22 行 | 在版本列表**末尾**（0.13 之后、`## 1.` 之前）新增 `> 版本：0.14（2026-09-27，node-trust-export-ids 变更：…）` 一行 | RV2-DOCS-F1 |
| 2 | `@@ -1359 +1361 @@`（同一行行内改写） | 同文件 §9 判据 **32** 第 1361 行 | 来源列表插入「§6 第 5 条、」；子项插入 ④「core 的 **Owner 侧授权判定**（§6 第 5 条）必须与可见性**同口径**…该 Export 的会话命令也**不可用**（拒绝或失败关闭，不得降级放行）——与 catalog 看不到它的结论一致」；原 ④ 顺移为 ⑤ | RV2-DOCS-F3 |

## 3. 检查范围与命令（自行执行，均为只读）

| # | 命令 / 方法 | 结果 | 建立的事实 |
| --- | --- | --- | --- |
| C1 | `git rev-parse HEAD`；`git status --porcelain`；`git diff --stat/--name-only/--diff-filter=AM 3cadb12..7d5abeb`；`git diff --name-only 3cadb12..7d5abeb -- schemas fixtures compatibility crates` | HEAD = `7d5abeb8`；工作区空；整条变更范围 = 4 个文件（`docs/CORE_PORTS_AND_STORAGE.md`、`docs/LOCAL_ADMIN_PROTOCOL.md`、`docs/NODE_LINK_PROTOCOL.md`、`openspec/specs/storage-schema-v2-migration/spec.md`）；`schemas/fixtures/compatibility/crates` 命中 **0 个文件** | 版本稳定；范围与调度者声明一致；资产/代码在整条范围内零改动 |
| C2 | `git diff 426f8ae..7d5abeb`（全文）+ `-U0 \| grep '^@@'` | 只有 2 个 hunk（见 §2），`--stat` = 1 file changed, 3 insertions(+), 1 deletion(-) | 本轮改动逐行读完，位置固定 |
| C3 | 文件头逐字比对：`diff <(git show 426f8ae:… \| sed -n '1,21p') <(sed -n '1,21p' …)` | **无差异**（HEADER LINES 1-21 IDENTICAL）；`grep -n '^> 版本'` 显示 0.3、0.4、0.5、**0.9、0.8**（既有乱序保留）、0.10、0.11、0.12、0.13、**0.14**（新增，第 22 行，位于版本列表末尾） | F1 复核口径：既有版本行逐字未动、乱序未整理、新行位置正确 |
| C4 | 逐项核对 0.14 行所述内容 ↔ 文档实际改动：`grep -n` §3.5（153/160）、§4/§5 边界（192/226）、可见性段（222）、§6 第 5 条（793）、§7 标题（843）、§7.2（857/866）、§7.3 DDL（1054）、§9 判据 32（1361）、§11.6 第 4 条（1503）、§11.7（1539）、§11.8（1550） | §3.5/§6 第 5 条/§7→v4/§7.2 常量 4/4/3/§9 判据 32/§11.6 第 4 条/§11.7/§11.8 **逐项相符**；**两处不符**：① 该行写「§5.3 的单点可见性策略」，而「单点可见性策略」在本文件只出现在 §4（第 222 行，`## 4.` 标题在第 192 行、`## 5.` 在第 226 行之间）与 §6 第 5 条（第 793 行），**§5.3（367–761 行）内不含该表述**；② 该行写「**§5 的 rust 块已同批更新**」，而 §5 在本变更（`3cadb12..7d5abeb`）内**零改动**（§5 范围内无任何 hunk；仅按插入的文件头行整体下移 2 行） | 新发现 RV3-DOCS-F1 的依据 |
| C5 | 判据 32 逐字比对：`node -e` 提取 426f8ae 与 target 的判据 32 行，做「去掉 ④ 与来源列表新增项后」的逐字比较；并在 `426f8ae` 上读旧 ④ 原文 | 除「来源列表新增 `§6 第 5 条、`」与「插入 ④」外**逐字相同**；原 ④（`export.revoke` 不级联清理）逐字保留并顺移为 ⑤ | F3 复核：①②③⑤ 措辞未动、只增 ④ |
| C6 | §9 编号与交叉引用：`grep -n '^2[89]\. \*\*\|^3[012]\. \*\*'`；`grep -n '判据 31\|判据 32'`（全文件） | §9 为 `1…31 32` **连续、无重号无缺号**（1327 起至 1361；28=1357、29=1358、30=1359、31=1360、32=1361）；判据 32 含 ①②③④⑤ 共 5 个子项；「判据 31」只出现在文件头第 17 行（0.11 的历史记录）与 §5.1 第 267 行（指向未改动的第 1360 行 view 判据），**仍正确**；「判据 32」全文只有 1 处（即判据自身），无其它陈旧引用 | F3 复核：编号/交叉引用无回归 |
| C7 | 本轮 diff 是否动其它小节：`git diff 426f8ae..7d5abeb -U0` 的 hunk 落点 | 只有文件头插入与判据 32 行改写；§3.5/§5.1/§5.3/§5.4/§6/§7/§10/§11.3–§11.8 **均未被触碰** | 回归检查：无越界改写 |
| C8 | `node scripts/check-doc-links.mjs` | **exit 0**：`380 relative links, 5114 section refs across 324 markdown files`（与 fixer 报告、`verification.md` 登记**逐字一致**；上一轮 5101，+13） | [PV1] check:docs 在 target 上 PASS，**独立复现** |
| C9 | `node scripts/check-contract-drift.mjs` | **exit 1**：唯一差异 = `§7 块 1 (OWNED_SCHEMA_V1) 第 20 条语句不一致`（`owned_node`）——与 RV2 在 `426f8ae2` 的记录**逐字相同**；已核实脚本会剥掉 `--` 注释（`stripSqlComments`），故 §7.3 DDL 行尾注释的措辞差异**不会**造成额外漂移 | 跨 WP 已知 PENDING（代码在另一 worktree），本轮未引入新漂移 |
| C10 | `node scripts/check-command-catalog.mjs` | **exit 0**：`command catalog OK: 12 commands` | 封闭词表在 target 上仍绿（任务书第 3 点的回归项） |
| C11 | 版本行体例比对：`sed -n '18,24p' … \| cat -A` 与 0.12/0.13 行格式对照 | 新行与既有行同为 `> 版本：<号>（<日期>，<变更名> 变更：<逐节说明>）` 单行体例，嵌套括号用法与 0.13 一致；新行前后各有一个空行，与 0.13 的既有排布一致 | 体例一致（不构成发现） |
| C12 | 跨 worktree 只读对照（旁证）：`export-ids` worktree 的 `migrate.rs` 常量与 DDL、`crates/core/src/ports.rs` 是否被修改 | 常量 = `4/4/3`、`owned_node` 末尾有 `export_ids_json TEXT NOT NULL DEFAULT '[]'`（与 §7.2/§7.3 文本一致）；`ports.rs` **未被修改**（该 worktree `git status` 未列出它） | 支撑「§7.2 常量 4/4/3」属实，且说明 §5 的 rust 块**本就不需要改**（端口签名无变化）——但文件头写成「§5 的 rust 块已同批更新」，仍属陈述不实（F1 ②） |

## 4. Findings

### 4.1 原问题逐条复核（RV2-DOCS-Fn）

| 原 ID | 级别 | 复核位置（target `7d5abeb8`） | 复核结论 | 依据 |
| --- | --- | --- | --- | --- |
| RV2-DOCS-F1 | MINOR | `docs/CORE_PORTS_AND_STORAGE.md` 文件头（第 22 行新增的 0.14 版本行） | **已解决**（原问题的实质 = 文件头缺少本次变更记录：已补齐，位置在版本列表末尾、体例一致、既有行逐字未动） | C3：文件头 1–21 行与 `426f8ae` **逐字相同**（含 0.9/0.8 的既有乱序），新增行只在末尾插入；C11：体例与 0.12/0.13 一致；C4：行内 8 项内容中 §3.5、§6 第 5 条、§7→v4、§7.2 常量 4/4/3、§9 判据 32、§11.6 第 4 条、§11.7、§11.8 **逐项与文档实际改动相符**。**但该行自身有两处陈述与文档实际不符**（§5.3 节号、§5 rust 块「已更新」），作为本轮**新发现 RV3-DOCS-F1** 单列——不改变「原问题已解决」的判定，因为原问题要求的是在本文件留一条变更记录，该要求已满足 |
| RV2-DOCS-F3 | SUGGESTION | 同文件 §9 判据 32（第 1361 行，新增子项 ④ + 来源列表补 §6 第 5 条） | **已解决** | ① 断言已落地：④ 明确「core 的 **Owner 侧授权判定**（§6 第 5 条）必须与可见性**同口径**：清单为空、或目标 Export 不在该节点 `exportIds` 清单内时，即使 `export.scopes ∩ grants ≠ ∅`，该 Export 的会话命令也**不可用**（拒绝或失败关闭，不得降级放行）」——正是 RV2-DOCS-F3 建议的补强文本，且来源列表已加 `§6 第 5 条`；② 口径一致：与 §6 第 5 条（第 793 行「清单为空或目标 Export 不在清单内一律不放行」）、`NODE_LINK_PROTOCOL.md` §8.2（target 版：三条件、「空清单即看不到任何 Export」、`catalog.snapshot`/`resource.attach`/Owner 侧命令授权复用同一份判定）、`design.md` D1（条件 3 在 core 的 Owner 侧命令授权用同一条节点行判定，「目录里看不到的 Export 也不能被命令选中」）与 D9（「`Broker` Owner 侧授权在『清单为空』『清单不含目标 Export』时必须拒绝…**核心断言**：清单为空时，即使 `scopes ∩ grants` 非空也不可见/不可用」）**逐条相符**；③ 子项 ①–⑤ 连续、无重号无缺号（C6），①②③⑤ 措辞未动，只有 ④ 是新增（C5）；④ §9 的 1–31 条在**本轮**（`426f8ae..7d5abeb`）编号与措辞未动（本轮 §9 只有第 1361 行被改写；1 与 28 的 v4 措辞改动发生在本变更更早的轮次，非本轮）；⑤ §5.1 第 267 行的「§9 判据 31」仍指向未改动的第 1360 行 view 判据，**仍然正确**（C6） |

### 4.2 本轮新发现（RV3-DOCS-Fn）

| ID | 级别 | 位置（target `7d5abeb8`） | 触发条件 | 预期 / 实际 | 影响 | 建议 |
| --- | --- | --- | --- | --- | --- | --- |
| RV3-DOCS-F1 | MINOR | `docs/CORE_PORTS_AND_STORAGE.md` 文件头第 22 行（新增的 `版本：0.14` 行） | 按文件头了解「本次改了哪几节、漂移门禁是否仍成立」 | 预期：该行的逐节陈述与文档实际改动一致。实际两处不符：**①「§5.3 的单点可见性策略…改为三条件」**——「单点可见性策略」在本文件只出现在 **§4**（第 222 行；`## 4.` 在第 192 行、`## 5.` 在第 226 行之间）与 §6 第 5 条（第 793 行），**§5.3（367–761 行）不含该表述、本次也未改动**；**②「§5 的 rust 块已同批更新」**——§5 在整条变更 `3cadb12..7d5abeb` 内**零改动**（§7 的 SQL 块确实同批更新了 `owned_node` DDL，但 §5 的 ```rust 块没有任何增删，仅随文件头插入整体下移 2 行）；跨 worktree 旁证显示 `crates/core/src/ports.rs` 也未被修改，即 §5 **本就不需要**改。另外该行的逐节枚举**漏记**了本次同样改到的 §9 判据 1 与判据 28 的 v4 措辞、§11.3 的「过新」取值 `5`、以及 §10 的取代标注（0.13 及更早的版本行也会点名此类正文改动） | 有限：不影响任何合同语义（三条件口径本身四处一致）；但版本行是该权威合同文件自述「当前版本与最近修订」的位置，① 会把读者指向 §5.3（那里没有该策略）并让人误以为 **§5 的冻结形状**（`check:drift` 的绑定面）本次被改动过，② 使「本次改了哪几节」的清单不完整，归档/追溯时容易漏看 §9 判据 1/28 与 §11.3。**不阻断** | 只改一行文字，例如把该行这两段与枚举替换为：「…§3.5 `NodeRecord`/`PairingSettlement::Approved` 加清单字段，**§4 的单点可见性策略与 §6 第 5 条的 Owner 侧授权判定改为三条件（未撤销 ∧ `scopes ∩ grants ≠ ∅` ∧ `exportId ∈ exportIds`）**，§7 升级到 v4（…），§7.2 版本常量改 4/4/3（§9 判据 1/28 的 v4 措辞、§11.3 的「过新」取值同步），§9 新增判据 32，§10 的历史裁定条加取代标注，§11.6 第 4 条补清单校验与审计不新增，§11.7/§11.8 补集合列说明与「为什么只加列不重建」。**§7 的 SQL 块已同批更新、§5 的 ```rust 块未变（无端口签名变化）**，漂移门禁继续逐条成立」（具体措辞由主 Agent/后续 fixer 定；是否本轮修、还是并入归档前的文档收尾，由主 Agent 裁定） |
| RV3-DOCS-F2 | SUGGESTION | 同文件 §9 判据 32 第 1361 行的来源列表（`（§5.4、§7.2、§7.3、§11.6 第 4 条、§6 第 5 条、\`NODE_LINK_PROTOCOL.md\` §8.2）`） | 按来源列表定位 `exportIds` 清单的形状定义 | 预期：本文件内**裸 §号**一律指本文件自己的章节（§9 全部 32 条的括注都是这个约定：`§7.2`、`§7.3`、`§11.6 第 4 条`…；跨文档引用都带文档名，如本条末尾的 `NODE_LINK_PROTOCOL.md` §8.2）。实际：裸 `§5.4` 在本文件是「发布与基础设施」（`EventSink`/`Clock`/`IdGenerator`，第 762–785 行），与 `exportIds` 清单无关；清单的 wire 形状在 **`LOCAL_ADMIN_PROTOCOL.md` §5.4**（`node.pair.confirm` 的 `exportIds` 必填），本文件内则在 **§3.5**（`NodeRecord` 行）。**非本轮 fix 引入**（`426f8ae2` 的该行已含裸 `§5.4`，RV1/RV2 未指出），但它在本轮被改写的同一行上（来源列表刚加入 `§6 第 5 条`） | 极有限：`check-doc-links` 不判定裸章节引用归属（C8 已 exit 0），不影响合同语义；只是引用精度问题，读者按 `§5.4` 去本文件会找到 `IdGenerator` 而非清单形状 | 顺手把来源列表改为「`§3.5`、`§7.2`、`§7.3`、`§11.6 第 4 条`、`§6 第 5 条`、`LOCAL_ADMIN_PROTOCOL.md` §5.4、`NODE_LINK_PROTOCOL.md` §8.2」（或至少给 `§5.4` 补文档名）。**不阻断**；是否本轮修由主 Agent 裁定 |

## 5. 回归检查（任务书第 3 点）

| 检查项 | 方法 | 结论 |
| --- | --- | --- |
| 本轮 diff 是否只动 `docs/CORE_PORTS_AND_STORAGE.md` | `git diff --name-only/--numstat 426f8ae..7d5abeb`（C1/C2） | **成立**：1 file changed, 3 insertions(+), 1 deletion(-)；2 个 hunk 只落在文件头与判据 32 行 |
| 是否误改封闭词表/合同资产 | `git diff --name-only 3cadb12..7d5abeb -- schemas fixtures compatibility crates`（C1）= 0 个文件；`check-command-catalog`（C10） | **未误改**：`schemas/**`、`fixtures/**`、`compatibility/**`、`crates/**` 在整条变更范围内**零改动**，封闭词表门禁在 target 上 exit 0（12 commands） |
| 是否误改其它小节 | 本轮 hunk 落点（C7） | **未误改**：§3.5、§5.1/§5.3/§5.4、§6、§7（含 SQL 块与 §7.3 DDL）、§10、§11.3–§11.8 均未被触碰；§9 除判据 32 外无改动 |
| 版本行体例与既有行完整性 | C3/C11 | **通过**：既有 0.3–0.13 逐字未动（含既有 0.9→0.8 乱序），新行位置在版本列表末尾、体例一致；新行内容有两处不实（RV3-DOCS-F1，不影响既有行） |
| 判据编号与交叉引用 | C5/C6 | **通过**：§9 为 1…32 连续无重号；判据 32 子项 ①–⑤ 连续；§5.1「§9 判据 31」与文件头 0.11 行的「判据 31」仍指向正确的旧判据；全文无第二处「判据 32」引用 |
| 门禁复跑（本环境可运行者） | C8 `check-doc-links` = 0；C10 `check-command-catalog` = 0；C9 `check-contract-drift` = 1（跨 WP，与上一轮逐字相同） | **未引入新失败**：`check:docs` 由 5101 → 5114 引用，全部可解析；漂移差异只有既有的 `owned_node` 一条（代码在另一 worktree） |
| 交付卫生 | `git status --porcelain`、提交信息、hunk 结构 | **通过**：worktree 干净；提交 `docs(storage): …` 类型/scope 合规；轮次 diff 无无关行 |

## 6. 附带判断（非发现）

1. **判据 32 ④ 的覆盖面**：④ 断言的是「该 Export 的**会话命令**不可用」；无会话命令（`session.list`/`command.status`/`session.create`）在 §6 第 5 条里按「存在满足条件的 Export」判定（该条的「满足条件」已含清单条件），且本轮已把 §6 第 5 条加入判据 32 的来源列表，因此两类命令的覆盖都不落空——**不构成发现**（若追求显式，可在 ④ 后加半句「无会话命令同理：清单为空时无任何 Export 满足条件」）。
2. **④ 里「拒绝或失败关闭」的措辞**：`design.md` D9 写「必须拒绝」，④ 写成「拒绝或失败关闭，不得降级放行」——两者都是失败关闭方向，没有放宽语义，**不构成发现**。
3. **`D14` 标签**：§4 第 222 行仍以 `D14` 标识来源并用「2026-09-27 起为三条件」限定，RV2 已判为可接受，本轮复读未见新问题，**不构成发现**。
4. **§5.3 归属的来源链**：把第 222 行当作「§5.3」的写法在 RV1-DOCS、RV2-DOCS 报告与 `verification.md`（Review Findings 的 RV1-DOCS-F2 行）中**都存在**（均为 `docs/CORE_PORTS_AND_STORAGE.md` §5.3 的表述）。fixer 的 0.14 行沿用了这一归属，因此 RV3-DOCS-F1 ① 属于**沿用既有误标**而非新造；主 Agent 若决定修，建议一并校正 `verification.md` 中的对应表述（该文件由主 Agent 维护，本 Agent 未改动）。

## 7. Assessment

### 7.1 逐检查 ID 结论（计划 ↔ 实际，target `7d5abeb8`）

| 检查 ID / 来源 | 核对方式 | 结论 |
| --- | --- | --- |
| [PV1] `check:docs`（`plan.md` Project Verify；`verification.md` fix 轮次 2 行） | 本人独立执行 `node scripts/check-doc-links.mjs`（C8） | **PASS / NEW（独立复现）**：exit 0，380 相对链接 / 5114 章节引用 across 324 md，与 fixer 报告与 `verification.md` 登记逐字一致（上一轮 5101） |
| [PV1] `check:drift` | 本人独立执行（C9）：exit 1，唯一差异仍是 §7 块 1（`OWNED_SCHEMA_V1`）第 20 条语句（`owned_node`） | **BLOCKED / PENDING**：与 RV2 在 `426f8ae2` 的记录逐字相同，原因是代码在另一个 worktree（`crates/storage-sqlite/src/migrate.rs` 仍是 v3 形状）；本轮未触碰 §7 SQL 块/§5 端口签名/§7.3 DDL，故未引入新漂移。PASS 须在集成/候选阶段复跑（tasks 3.1、6.3）。**不影响**本报告的静态结论 |
| [PV1] `check:agentic` | 未执行（worktree 无 `node_modules`，见 §1 限制 2） | **BLOCKED / PENDING**：须在集成树补跑（含 `openspec validate --all --strict` 对本次主规范 Purpose 改动的接受性） |
| [PV1] 其余合同门禁 | `check-command-catalog` 独立跑绿（C10）；需 `ajv` 的 4 道如实记为未执行（§1 限制 3），以「资产零改动」反证（C1） | **已核对/反证成立** |
| [PV3]/[PV4]/[PV5]（代码面） | 代码不在本分支（§1 限制 1） | **NOT_APPLICABLE 于本 WP**（由 tasks 3.2 与 3.1/6.3 覆盖） |
| E2E | `plan.md` 记 `not-applicable`（用户批准） | NOT_APPLICABLE（文档 WP） |

### 7.2 design/spec 口径复核（只记与本轮两条原问题相关的部分）

| 决策 / 要求 | 复核结论 |
| --- | --- |
| `design.md` D1（三条件、判定点仍两处且必须同口径） | 成立：§6 第 5 条（core Owner 侧）+ §8.2（可见性权威）+ §4 单点策略 + §3.5 + §11.6 第 4 条 + §9 判据 32 ①③④ 一致；判据 32 ④ 现在显式断言 core 侧的「不可用」 |
| `design.md` D9（core 面测试策略：清单为空/不含目标 Export 必须拒绝） | 成立：判据 32 ④ 与该「核心断言」逐条对应，且来源列表已点名 §6 第 5 条 |
| `design.md` D2/D4 + `specs/storage-schema-v2-migration` R11–R15（v4 只加列、既有行置空、不得默认放权、列清单与新建库相等） | 未被本轮触碰；§7.2/§7.3/§9 判据 28 的文本与 `docs/NODE_LINK_PROTOCOL.md` §8.2 的「升级副作用」段一致（本报告不重开该面） |
| `AGENTS.md` §10（文档维护：一处权威定义、其余只写概要并链接）与 §13（通俗、结论先行） | 本轮两处改动都未引入第二套口径；新增文本为纯断言与来源补充 |

### 7.3 结论

- **文档静态复核结论：PASS**。RV2-DOCS-F1（文件头缺 0.14 版本行）与 RV2-DOCS-F3（判据 32 缺 core 侧同口径断言）**均已解决**；本轮修复未引入回归（范围、资产/词表、其它小节、版本行既有行、判据编号与交叉引用、可运行门禁均已核对，见 §5）。
- **新发现 2 项，均不阻断**：RV3-DOCS-F1（MINOR，0.14 版本行内两处节号/内容陈述不实 + 枚举漏记 §9 判据 1/28、§11.3、§10）与 RV3-DOCS-F2（SUGGESTION，判据 32 来源列表的裸 `§5.4` 指向本文件「发布与基础设施」而非清单形状）。两者都是**一行文字**级修正，不涉及合同语义。
- **本报告 `result`：BLOCKED**。唯一依据是 `handoff.md`「PENDING 的 `result` 必须为 BLOCKED」与 tasks.md 2.5 的完成条件含 `[PV1]` 的 `check:docs`/`check:drift`：`check:drift`（及同组 `check:agentic`）在本 worktree 必然 PENDING，须在 WP1/WP3 并入后的集成/候选阶段复跑（tasks 3.1、6.3；`verification.md` 已为多个 revision 登记 BLOCKED/PENDING 行）。与 RV1/RV2 的处置一致。
- **给主 Agent 的呈报口径**：若沿用 `verification.md` 既有的「跨 WP 延后」处置，把 `check:drift`/`check:agentic` 留给 3.1/6.3，则 2.5 的文档面可按 **PASS** 闭环（RV3-DOCS 行记 PASS/NEW，另保留两条 PENDING 检查行），RV3-DOCS-F1/F2 可并入归档前的文档收尾或明确接受现状；若严格按 tasks.md 2.5 的完成条件，2.5 在补齐这两项检查前记 **BLOCKED**。**两条路径都不需要新的文档修复轮次**（新发现各一行文字，且均非阻断）。

## 8. handoff_index

```yaml
handoff_index:
  - task_id: "2.5"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "7d5abeb8c2372ccd77a1d35cc0a4f580bd648054"
    evidence_type: REVIEW
    evidence_id: RV3-DOCS
    report_path: "reports/rv3-docs.md"
    result: BLOCKED
    evidence_status: NEW
    applicability_basis: "对 RV2-DOCS（target 426f8ae2，BLOCKED／文档静态 PASS）的范围受限 recheck。新建只读 reviewer，未继承实现/fix/RV1/RV2 检视对话；在 worktree D:\\Project\\acp-remote-wt\\export-ids-docs、base 3cadb12d、target 7d5abeb8 上重读 fix 轮次 2 的 diff（426f8ae2..7d5abeb8，1 文件 3/1 行、2 hunk）与受影响上下文（文件头 1–23 行、§4/§5.3/§5.4、§6 第 5 条、§7.2/§7.3、§9 全部 32 条、§11.6–§11.8）、design D1/D9、NODE_LINK §8.2（target）与仓库回归面后给出。RV2-DOCS-F1/F3 均判定**已解决**；新增 2 项非阻断发现（RV3-DOCS-F1 MINOR、F2 SUGGESTION），无 CRITICAL/MAJOR（文档静态结论 PASS）；记 BLOCKED 的唯一原因是 tasks 2.5 的完成条件含 [PV1] 的 check:drift（及同组 check:agentic），二者在本 worktree 必然 PENDING（见本报告 §7.3 的两条呈报口径）"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "7d5abeb8c2372ccd77a1d35cc0a4f580bd648054"
    evidence_type: CHECK
    evidence_id: "PV1 (check:docs)"
    report_path: "reports/rv3-docs.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本人于 target 7d5abeb8 的 worktree 独立执行 `node scripts/check-doc-links.mjs` → exit 0：`380 relative links, 5114 section refs across 324 markdown files`（node v24.19.0，worktree 无 node_modules 也不需要）；与 fixer 报告（handoff-coder-docs.md §9）及 verification.md 的 fix 轮次 2 行逐字一致，上一轮 5101。本轮新增/改写的引用（§3.5/§4 相关/§6 第 5 条/§7/§7.2/§9/§11.6/§11.7/§11.8）全部可解析。执行记录见本报告 §3 C8"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "7d5abeb8c2372ccd77a1d35cc0a4f580bd648054"
    evidence_type: CHECK
    evidence_id: "PV1 (check:drift)"
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "本人于 target 7d5abeb8 独立执行 `node scripts/check-contract-drift.mjs` → exit 1，唯一差异仍为 §7 块 1（OWNED_SCHEMA_V1）第 20 条语句（owned_node）：本 worktree 的 crates/storage-sqlite/src/migrate.rs 仍是 v3 旧形状（WP1/tasks 2.1 在另一 worktree 未合并）。与 RV2-DOCS 在 426f8ae2 的记录逐字相同；本轮 diff 未触碰 §7 的任何 SQL 块、§5 的 rust 块或 §7.3 DDL，故未引入新漂移（另已核实脚本剥 `--` 注释，DDL 行尾注释措辞差异不产生额外差异）。待补 ID：PV1(check:drift)；门禁：check-contract-drift.mjs；须在 WP1/WP3 并入后的集成/候选阶段复跑（tasks 3.1、6.3）。执行记录见本报告 §3 C9"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "7d5abeb8c2372ccd77a1d35cc0a4f580bd648054"
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
    target_revision: "7d5abeb8c2372ccd77a1d35cc0a4f580bd648054"
    evidence_type: CHECK
    evidence_id: "WP5-SUPP (check:command-catalog)"
    report_path: "reports/rv3-docs.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本人于 target 7d5abeb8 独立执行 `node scripts/check-command-catalog.mjs` → exit 0（`command catalog OK: 12 commands`），任务书第 3 点的回归项：证明本轮 fix 未误改封闭词表。另作为「合同资产未改动」的反证：`schemas/**`、`fixtures/**`、`compatibility/**`、`crates/**` 在 3cadb12..7d5abeb 内命中 0 个文件（本报告 §3 C1/C10）"
    source_evidence: NOT_APPLICABLE
```

## 9. 结论

**文档静态复核：PASS**（RV2-DOCS-F1、RV2-DOCS-F3 均已解决；fix 轮次 2 未引入回归；新发现 RV3-DOCS-F1（MINOR）与 RV3-DOCS-F2（SUGGESTION），无 CRITICAL/MAJOR，两者均为一行的文字级修正）。

**本报告 result：BLOCKED**（唯一原因 = [PV1] 的 `check:drift` 与 `check:agentic` 在本 worktree 必然 PENDING，须在集成/候选阶段复跑；见 §7.3 的两条呈报口径）。
