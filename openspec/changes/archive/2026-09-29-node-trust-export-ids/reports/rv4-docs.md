<!-- RV4-DOCS：WP5（tasks.md 2.5）文档工作包 **fix 轮次 3 后的范围受限收口复核（recheck）**报告。字段与 handoff_index 组织见 openspec/schemas/agentic/roles/handoff.md。 -->

# 独立检视报告（范围受限收口复核）：RV4-DOCS / WP5 文档同步（task 2.5，fix 轮次 3）

| 字段 | 值 |
| --- | --- |
| `task_id` | 2.5 |
| `role` | reviewer（独立检视子 Agent，只读；本轮为 recheck） |
| `phase` | review |
| `stage` | work-package |
| `agent_context` | 新建子 Agent，**不继承**父 Agent 的实现/fix/RV1/RV2/RV3 对话；只消费调度者给出的复核输入（RV3-DOCS 报告、fixer 的 fix 轮次 3 说明与日志、`verification.md` 的 Review Findings 表与 `design.md`/`tasks.md` 2.5/`plan.md`）与自身只读读取的 worktree、文件与命令输出。本 Agent 未参与任何实现或 fix，实现者自评文本只作对照、不作证据。 |
| `evidence_type` | REVIEW |
| `evidence_id` | RV4-DOCS |
| `target_revision` | `39be2f7792b2ccfaa143b386d64fc731eedf2442`（worktree HEAD，已用 `git rev-parse HEAD` 核实；`git status --porcelain` 为空） |
| `base_revision` | `3cadb12d79456750e40644a2ea53c96380b700e6`（= `refs/heads/main`） |
| `recheck_of` | RV3-DOCS（`reports/rv3-docs.md`，被检版本 `7d5abeb8`，文档静态 **PASS**／报告 `result` 记 BLOCKED，新增 RV3-DOCS-F1（MINOR）、RV3-DOCS-F2（SUGGESTION）） |
| `scope` | **受限**：只复核 fix 轮次 3 的 diff `7d5abeb8..39be2f77`（1 个文件、2 hunk、2 增 2 删）与 RV3-DOCS-F1/F2 两条原问题的受影响上下文（文件头 1–23 行、§4/§5/§5.3、§7/§7.2/§7.3、§9 全部 32 条），外加 `3cadb12..39be2f77` 的回归核对与可在本环境运行的只读门禁；**不重新展开**全量检视 |
| `changes` | 本次检视未修改被检视文档、代码或规划工件；只在 `reports/` 下新增本报告 |
| `checks` | 见 §3：`check-doc-links`（文档 worktree，target `39be2f77`）= exit 0（380 链接 / 5117 章节引用，独立复现）、`check-command-catalog` = exit 0、`check-contract-drift` = exit 1（跨 WP 已知 PENDING，与上一轮逐字相同）、`check:agentic` 与需 `ajv` 的四道未执行（worktree 无 `node_modules`）；**另在「主检出」复跑 `check-doc-links` = exit 1**，命中 `verification.md:70`（范围外发现 OOS-1，见 §4.3） |
| `issues` | RV3-DOCS-F1 **已解决**（§4 节号、§5/`ports.rs` 未改的陈述、枚举三处漏记均已修正）；RV3-DOCS-F2 **已解决**（来源列表已精确到文档与小节，正文一字未动）；新增 1 项 RV4-DOCS-F1（MINOR，全角括号不配对，纯排版、非阻断），被检 artifact 的级别上限为 MINOR。**范围外**：OOS-1 / RV4-DOCS-F2（MAJOR，`verification.md`:70 让主检出的 `check:docs` 变红，主 Agent 拥有该文件，见 §4.3） |
| `result` | **PASS**（仅针对被检 artifact = `docs/CORE_PORTS_AND_STORAGE.md` @ `39be2f77` 的文档线收口）——按调度者的收口约定：两条原问题均已解决、未引入功能性回归，唯一新发现为一行排版级小项且已如实登记（主 Agent 决定是否收口）。`[PV1]` 的 `check:drift`/`check:agentic` 仍按 `verification.md` 记为 PENDING（跨 WP／无 `node_modules`），见 §7.3；范围外发现 OOS-1 须在集成阶段前处置，见 §4.3 |
| `evidence_paths` | 本报告 `openspec/changes/node-trust-export-ids/reports/rv4-docs.md`；被复核对象 `reports/rv3-docs.md`（另有 `rv2-docs.md`/`rv1-docs.md`）；实现者证据（非本人证据，仅作对照）`reports/handoff-coder-docs.md` §10、`reports/wp5-docs-fix.log`「第三轮 fix」 |
| `resource_cleanup` | 未创建目录联接、数据库、端口、SQLite 文件或长驻子进程；全部命令为只读（`git`、`sed`/`grep`/`diff`、`node -e` 只读片段、`node scripts/*.mjs`）；检视期间唯一临时文件（`/tmp/d1.diff`，用于读完整 diff）已 `rm` 并核实不存在；两个 worktree 的 `git status --porcelain` 复核后仍为空（主检出只有既有的未跟踪 `?? openspec/changes/node-trust-export-ids/` 与本次新增的未跟踪报告）；未提交、未切分支、未改 `verification.md`/`tasks.md`/`plan.md`；未执行 E2E |

## 1. Review Context

- **Review ID**：RV4-DOCS（唯一，本轮）；**Review Type**：recheck；**Review Stage**：work-package（WP5 fix 轮次 3 后，文档线收口）；**范围**：RV3-DOCS 的两条新发现 + 该修复的回归面。
- **版本稳定性**：worktree `D:\Project\acp-remote-wt\export-ids-docs` HEAD = `39be2f77`（= 调度者给定 target），工作区干净；fix 轮次 3 为单提交 `39be2f77`（`docs(storage): 校正 0.14 版本行节号与未改端口签名口径，并精确标注判据 32 来源`），落在 `7d5abeb8` 之上，diff 可复现（2 个 hunk，各 1 增 1 删）。
- **复核方法**：先读 RV3-DOCS 报告与 fixer 的 fix 轮次 3 说明，再**重读修复版本与受影响上下文**（文件头 1–23 行、§4 全节、§5/§5.3 区间边界、§6 第 5 条、§7 标题/§7.2/§7.3、§9 全部 32 条及其编号），然后按原问题 ID 逐条判定「已解决/未解决/无法确认」，最后专查本轮修复是否引入回归（范围、合同资产、其它小节、版本行既有行与体例、判据编号与交叉引用、可运行门禁）。
- **必要资料（只读读取）**：`design.md` D1/D3/D9/D10、`tasks.md` 2.5、`plan.md`（E2E `not-applicable`）、`verification.md`（Target／Review Findings 的 RV3-DOCS-F1/F2 行）、`AGENTS.md` §10/§13、被检视文档 `docs/CORE_PORTS_AND_STORAGE.md`（target 版）、`docs/LOCAL_ADMIN_PROTOCOL.md` §5.4 与 `docs/NODE_LINK_PROTOCOL.md` §8.2（target 版，用于判断判据 32 来源列表是否指向真实且正确的位置）。

### 限制与不确定性

1. **代码不在本版本内**：`crates/**` 在 `3cadb12..39be2f77` 内零改动（WP1–WP4 在另一 worktree），因此「文档与实现逐字一致」只能由 `check:drift` 承载，本 worktree 必然 PENDING（§3 C14、§7.3）。§3 C16 的跨 worktree 只读对照只是旁证，**不构成**门禁证据。
2. **`check:agentic` 无法执行**：本 worktree 无 `node_modules`；本人不建立目录联接、不直接调用 OpenSpec 引擎（引擎默认开启遥测）。记为待返证据。
3. **需 `ajv` 的四道资产门禁未执行**（RV2/RV3 已如实记录以 `ERR_MODULE_NOT_FOUND` 退出）；替代反证 = 这些门禁的输入（`schemas/**`、`fixtures/**`、`compatibility/**`）在 `3cadb12..39be2f77` 内**零改动**（C11）。
4. **行尾差异**：worktree 检出内容为 CRLF（`core.autocrlf=true`），仓库三个相关 revision 的 blob 内 **CR 字节均为 0**（C15）。本报告所有逐字比对都在 blob 内容或同一侧内容上进行，不受该检出行为影响。
5. **E2E 不适用**：文档工作包，`plan.md` 记 `not-applicable`（用户 2026-09-27 批准），本轮不改变该判断。
6. 本 Agent 未参与实现/fix，也不声称能看到宿主注入的全部上下文。

## 2. 本轮被复核的改动（changes under review）

复核对象 = fix 轮次 3 的 diff（`7d5abeb8..39be2f77`，`--name-only` = **1 个文件**，`--stat` = 2 insertions / 2 deletions，**2 个 hunk**）：

| # | hunk | 位置（target `39be2f77`） | 旧值 → 新值 | 归属 |
| --- | --- | --- | --- | --- |
| 1 | `@@ -22 +22 @@`（行内整行替换） | `docs/CORE_PORTS_AND_STORAGE.md` 文件头第 22 行（`> 版本：0.14（…）`） | `§5.3 的单点可见性策略`→`§4 的单点可见性策略`；`scopes ∩ grants`→`export.scopes ∩ grants`；`**§7 的 SQL 块与 §5 的 rust 块已同批更新**`→`**本次未改任何端口签名：§5 的 rust 块与 `crates/core/src/ports.rs` 均未变；§7 的 SQL 块已同批更新**`；枚举补 `§9 新增判据 32 并同步判据 1/28 的版本链与 owned 列清单断言，§10 给历史裁定条目补当前口径提示，§11.3 的「过新」用例取值改为 5` | RV3-DOCS-F1 |
| 2 | `@@ -1361 +1361 @@`（行内整行替换） | 同文件 §9 判据 **32** 第 1361 行的来源括注 | `（§5.4、§7.2、§7.3、§11.6 第 4 条、§6 第 5 条、\`NODE_LINK_PROTOCOL.md\` §8.2）` → `（本文件 §3.5、§7.2、§7.3、§11.6 第 4 条、§6 第 5 条，\`LOCAL_ADMIN_PROTOCOL.md\` §5.4，\`NODE_LINK_PROTOCOL.md\` §8.2）` | RV3-DOCS-F2 |

## 3. 检查范围与命令（自行执行，均为只读）

| # | 命令 / 方法 | 结果 | 建立的事实 |
| --- | --- | --- | --- |
| C1 | `git rev-parse HEAD`；`git status --porcelain`（两个 worktree） | HEAD = `39be2f77…`；两边输出均为空（注：git 不显示空目录，worktree 的 `openspec/changes/node-trust-export-ids/` 下只有一个空 `reports/`，因此不被列出；见 C18） | 版本稳定，与调度者给定的 target/base 一致；无已修改/已暂存的被追踪文件 |
| C2 | `git diff --stat/--name-only/--numstat 7d5abeb8..39be2f77`；`git diff -U0 … \| grep '^@@'` | 1 file changed, 2 insertions(+), 2 deletions(-)；2 个 hunk = `@@ -22 +22 @@`、`@@ -1361 +1361 @@` | 本轮只动文件头 0.14 行与判据 32 首行，无其它行、无行尾改写 |
| C3 | **（c）小节映射**：`git diff -U0 3cadb12..39be2f77 -- docs/CORE_PORTS_AND_STORAGE.md \| grep '^@@'`，并把每个落点归到 `grep -n '^## \|^### '` 出来的小节区间 | -U0 = **19 个 hunk**（= 文件头 1 + 正文 18；默认上下文时合并为 14 个），归为 13 个正文落点：§3.5(2)、§4(1)、§6 第 5 条(1)、**§7 标题(1)**、§7.2(4)、§7.3(1)、§9 判据 1(1)、§9 判据 28+32(2)、§10(1)、§11.3(1)、§11.6 第 4 条(1)、§11.7(1)、§11.8(1) | 0.14 行的枚举与正文实际改动**逐项相符、无遗漏、无多写**（逐项明细见 §4.1；调度者清单未单列 §7 标题行，见 §6.1） |
| C4 | **（a）节号**：`grep -n '单点可见性策略' docs/CORE_PORTS_AND_STORAGE.md`；`grep -n '^## 4\. \|^## 5\. \|^### 5.3'` | 命中 3 处：第 22 行（版本行自身）、**222**（`## 4.` 在 192、`## 5.` 在 226 之间）、793（§6 第 5 条）；`### 5.3` 在 367，**§5.3 区间 367–761 内 0 处** | 「§4 的单点可见性策略」正确，原「§5.3」的误标已消除 |
| C5 | **（b）§5/§7/代码**：§5 区间（226–786）在 `3cadb12..39be2f77` 内是否有 hunk；`git diff --stat … -- crates/core/src/ports.rs`；§7.3 `owned_node` DDL hunk | §5 区间 **0 个 hunk**（与 §5.3 端口签名面完全未动）；`ports.rs` **0 行改动**；target 1051–1054 的 hunk 在 §7.3 的 SQL 块里新增 `export_ids_json TEXT NOT NULL DEFAULT '[]'` | 「§5 的 rust 块与 `crates/core/src/ports.rs` 均未变；§7 的 SQL 块已同批更新」**属实** |
| C6 | **（c）逐句核对**：把 0.14 行拆成 10 项枚举 + 2 句结论，逐项回到正文核对（行号见 C3；内容见 §4.1 依据列） | 10 项枚举全部落在真实改动的正文上（含 §7.2 常量写成 4/4/3、判据 1 的 `v1 → v2 → v3 → v4` 版本链、判据 28 新增的 owned 列清单断言①②、§11.3 取值 `5`、§11.6 第 4 条的「审计**不新增**」）；无一项指向本次未改的小节 | 无多写、无漏写 |
| C7 | **（d）文件头完整性**：`diff <(git show 3cadb12:… \| sed -n '1,21p') <(sed -n '1,21p' …)`；同样比对 `7d5abeb8` | 两次均**无差异**（1–21 行逐字相同） | 0.3–0.13 各行（含 `0.9` 排在 `0.8` 之前的既有乱序）逐字未动；0.14 行仍只在版本列表末尾 |
| C8 | **体例**：逐字读第 22 行；统计全角括号与 `**` 配对（blob 内容） | 仍为单行 `> 版本：<号>（<日期>，<变更名> 变更：…）`，前后各一空行，与 0.13 排布一致；`**` 数量 2（配对）；**全角括号 3 开 / 4 闭（`7d5abeb8` 为 3/3）** | 体例一致；但**新引入一处不配对括号** → 新发现 RV4-DOCS-F1 |
| C9 | **（F2）判据 32**：`node -e` 提取两侧第 1361 行，比较来源括注与「`：①` 起」的正文 | 来源括注 = 调度者要求的精确列表（逐字相同）；正文长度 660 字符且**逐字相同**；子项 ①–⑤ 计数 5 / 5 | 正文一字未动，改动只在来源括注 |
| C10 | **§9 编号**：`awk` 提取 §9 区间的顶层判据行并核号；本轮 §9 命中行数 | `1…32` 连续、无重号无缺号（共 32 条）；本轮 diff 在 §9 只命中第 1361 行 | 判据 1–31 及它们的措辞、交叉引用未受影响（判据 1/28 的 v4 措辞改动发生在更早轮次，非本轮） |
| C11 | **回归-范围**：`git diff --name-only 3cadb12..39be2f77`；`… -- schemas fixtures compatibility crates \| wc -l` | 4 个文件（`docs/CORE_PORTS_AND_STORAGE.md`、`docs/LOCAL_ADMIN_PROTOCOL.md`、`docs/NODE_LINK_PROTOCOL.md`、`openspec/specs/storage-schema-v2-migration/spec.md`）；资产/代码条件命中 **0 个文件** | 本轮 fix 只动 1 个文档文件；整条变更内 `schemas/**`、`fixtures/**`、`compatibility/**`、`crates/**` 零改动 |
| C12 | `node scripts/check-doc-links.mjs`（node v24.19.0） | **exit 0**：`380 relative links, 5117 section refs across 324 markdown files`（另注 2524 条归属未判定属设计） | [PV1] `check:docs` 在 target 上 PASS，**独立复现**，与调度者复跑逐字一致；新增/改写的引用（§3.5/§4/§10/§11.3、`LOCAL_ADMIN_PROTOCOL.md` §5.4）均可解析 |
| C13 | `node scripts/check-command-catalog.mjs` | **exit 0**：`command catalog OK: 12 commands` | 封闭词表未受影响（任务书第 3 点的回归项） |
| C14 | `node scripts/check-contract-drift.mjs` | **exit 1**：唯一差异 = `§7 块 1 (OWNED_SCHEMA_V1) 第 20 条语句不一致`（`owned_node`） | 跨 WP 已知 PENDING（代码在另一 worktree）；与 RV3 在 `7d5abeb8` 的记录**同一处、无新增**，本轮未触碰 §5/§7 的 SQL 块 |
| C15 | 行尾卫生：三个 revision 的 `git cat-file blob` 内 CR 字节数；工作区文件 CR 字节数；`git config core.autocrlf` | blob（`3cadb12`/`7d5abeb8`/`39be2f77`）CR = 0/0/0；工作区 = CRLF；`core.autocrlf=true` | 本轮提交未做任何行尾改写；工作区 CRLF 是仓库既有的检出行为，与 blob 无关 |
| C16 | 旁证（只读、**非门禁证据**）：`D:\Project\acp-remote-wt\export-ids` 的 `crates/storage-sqlite/src/migrate.rs` 常量 | `FILE_FORMAT_VERSION = 4`、`OWNED_SCHEMA_VERSION = 4`、`IMPORTED_SCHEMA_VERSION = 3` | 与 §7.2 及版本行的「4/4/3」一致（`check:drift` 的判定仍待集成阶段复跑） |
| C17 | 交付卫生：两个 worktree 的 `git status --porcelain`、`git diff --cached --name-only` | 均空；无暂存文件 | 只读检视，未产生待提交内容 |
| C18 | 范围外复核：在主检出（含未跟踪的 `openspec/changes/**`）执行 `node scripts/check-doc-links.mjs`；并读 `scripts/check-doc-links.mjs` 的 `attributeDocument`/`listMarkdown` 逻辑、`node -e` 复现该行的引用归因；对比两个 worktree 里该变更目录的实际内容 | **exit 1，且只有 1 个 problem**：`openspec/changes/node-trust-export-ids/verification.md:70: §3.5 在 docs/LOCAL_ADMIN_PROTOCOL.md 中不存在`。该脚本收集全部问题后才退出，因此本条同时证明**本报告自身未引入任何新的引用错误**（我的报告在这一次扫描范围内，problems 仍为 1）。目录对比：worktree 的该变更目录下只有空 `reports/`（0 个 md），主检出 13 个 md（含 `verification.md`）；扫描总数 worktree 324 / 主检出 350 | 范围外发现 OOS-1（§4.3）：文档 worktree 的 exit 0 抓不到它——该 worktree 的 `openspec/changes/node-trust-export-ids/` 下**只有空 `reports/` 子目录**（0 个 md；`git status` 因空目录而不可见，`git ls-files` = 0），`verification.md` 与 `reports/*.md` 只存在于主检出的未跟踪副本。同一次遍历：worktree 324 个 md、主检出 350 个 md（其中 13 个来自该变更目录） |

## 4. Findings

### 4.1 原问题逐条复核（RV3-DOCS-Fn）

| 原 ID | 级别 | 复核位置（target `39be2f77`） | 复核结论 | 依据 |
| --- | --- | --- | --- | --- |
| RV3-DOCS-F1 | MINOR | `docs/CORE_PORTS_AND_STORAGE.md` 文件头第 22 行的 `> 版本：0.14` 行 | **已解决** | ① **节号**：该行已写「§4 的单点可见性策略」；「单点可见性策略」在本文件只有第 22 行、**222（§4）**、793（§6 第 5 条）三处，§5.3（367–761）内 0 处（C4）→ 原「§5.3」误标消除。② **§5/`ports.rs` 口径**：改为「**本次未改任何端口签名：§5 的 rust 块与 `crates/core/src/ports.rs` 均未变；§7 的 SQL 块已同批更新**」；实测 §5 区间（226–786）在 `3cadb12..39be2f77` 内 **0 个 hunk**、`ports.rs` 0 行改动、§7.3 的 SQL 块确实新增了 `export_ids_json`（C5）→ 陈述与事实相符（原「§5 的 rust 块已同批更新」的不实陈述消除）。③ **枚举完整性**：0.14 行按 -U0 归为 13 个正文落点（§3.5、§4、§6 第 5 条、§7 标题、§7.2、§7.3、§9 判据 1、§9 判据 28、§9 判据 32、§10、§11.3、§11.6 第 4 条、§11.7、§11.8 —— 其中 §7 标题由「§7 升级到 v4」覆盖），0.14 行逐项点名，**无遗漏、无多写**（C3/C6）；原先漏记的 §9 判据 1/28 的版本链与 owned 列清单断言、§10 的历史裁定提示、§11.3 的取值 `5` 均已补入，且三处补入项与正文一致（判据 1 = `v1 → v2 → v3 → v4`；判据 28 新增「升级库与新建库的 owned 家族列清单逐项相等、`export_ids_json` 在末尾」与「既有行 `'[]'`」；§11.3 = `user_version = 5`，即 `FILE_FORMAT_VERSION + 1`）。④ **文件头**：1–21 行与 `3cadb12`、`7d5abeb8` **逐字相同**（含 0.9/0.8 的既有乱序），新行仍在版本列表末尾、单行体例与 0.12/0.13 一致（C7/C8）。**唯一遗留** = 该行新引入一处全角括号不配对（纯排版，另列为 RV4-DOCS-F1）——不影响本项「原问题已解决」的判定，因为 F1 的三项实质要求（节号、未改端口签名口径、枚举完整）已全部满足 |
| RV3-DOCS-F2 | SUGGESTION | 同文件 §9 判据 32 第 1361 行的来源括注 | **已解决** | ① 来源列表已改为调度者要求的精确形式：`（本文件 §3.5、§7.2、§7.3、§11.6 第 4 条、§6 第 5 条，`LOCAL_ADMIN_PROTOCOL.md` §5.4，`NODE_LINK_PROTOCOL.md` §8.2）`——原裸 `§5.4`（本文件「发布与基础设施」）已明确归到 `LOCAL_ADMIN_PROTOCOL.md` §5.4（C9，逐字一致）；② 逐条核对指向真实且正确的位置：本文件 §3.5 的 `NodeRecord` 行含 `exportIds`；§7.2 含版本常量与 v4 段；§7.3 含 `export_ids_json`；§11.6 第 4 条含「同一事务内校验 + 审计不新增」；§6 第 5 条含 Owner 侧三条件；`LOCAL_ADMIN_PROTOCOL.md` §5.4 的 `node.pair.confirm` 含必填 `exportIds`；`NODE_LINK_PROTOCOL.md` §8.2 含三条件与收窄语义（C4/C12 佐证引用可解析）；③ 判据正文「`：①` 起」660 字符与 `7d5abeb8` **逐字相同**，子项 ①–⑤ 仍为 5 个（C9）；④ §9 编号 `1…32` 连续、无重号无缺号，本轮 §9 只命中第 1361 行（C10），`check-doc-links` exit 0（C12）→ 无回归 |

### 4.2 本轮新发现（RV4-DOCS-Fn）

| ID | 级别 | 位置（target `39be2f77`） | 触发条件 | 预期 / 实际 | 影响 | 建议 |
| --- | --- | --- | --- | --- | --- | --- |
| RV4-DOCS-F1 | MINOR（纯排版，**非阻断**） | `docs/CORE_PORTS_AND_STORAGE.md` 文件头第 22 行（0.14 版本行） | 阅读文件头的版本记录（人读或工具做括号配对检查） | 预期：与 0.3–0.13 各版本行一致，全角括号成对（0.13 行为 5 开 / 5 闭）。实际：该行**3 开 / 4 闭**——多出的一个 `）` 出现在 `…补集合列说明与「为什么只加列不重建」**）**。**本次未改任何端口签名：…**` 处（即右括号在「本次未改任何端口签名…」这句之前就把外层 `版本：0.14（…` 的括号关掉了），而句末「…漂移门禁继续逐条成立**）**」又有一个无对应的右括号。`7d5abeb8` 的同行为 3/3（平衡），因此这是**本轮 fix 新引入**的、非既有的排版缺陷 | 极有限：不改变任何语义，`check-doc-links` 等门禁不判定括号配对（C12 已 exit 0）；只是在权威合同文件的版本行里留下一个可见的孤立右括号，读者可能对「`本次未改…` 这句在不在括号内」产生一瞬歧义 | 一个字符的最小修法：删掉「为什么只加列不重建」之后那个 `）`（即恢复原有的外层括号结构——`…不重建」。**本次未改任何端口签名：…**，漂移门禁继续逐条成立）`），`7d5abeb8` 的行结构就是这样；另一种等价修法是保留该 `）`、删掉句末那个，把「本次未改…」整句移到括号外作后记。**两种都是一行内一字符改动，不涉及任何语义；是否再开一次 fix 由主 Agent 裁定** |

### 4.3 范围外发现（不属本轮被检 artifact，需主 Agent 处置）

| ID | 级别 | 位置 | 触发条件 | 预期 / 实际 | 影响 | 建议 |
| --- | --- | --- | --- | --- | --- | --- |
| OOS-1（登记为 RV4-DOCS-F2） | **MAJOR（阻断集成门禁）** | `openspec/changes/node-trust-export-ids/verification.md` 第 70 行（Review Findings 表；**主 Agent 维护，不在本轮被检 artifact 内**） | 在**主检出**（含未跟踪的 `openspec/changes/**`）执行 `node scripts/check-doc-links.mjs`（C18） | 预期 exit 0。实际 **exit 1**：`error: openspec/changes/node-trust-export-ids/verification.md:70: §3.5 在 docs/LOCAL_ADMIN_PROTOCOL.md 中不存在（该引用指向它，但该文档没有这个小节）`。归因链（已用 C18 的 `node -e` 复现）：该行在同一个子句里先点名 `LOCAL_ADMIN_PROTOCOL.md` 的 §5.4、再写「本文件 §3.5」（其中「本文件」实指 `docs/CORE_PORTS_AND_STORAGE.md`），因此 `§3.5` 前 48 字符内 `attributeDocument` 只匹配到 `LOCAL_ADMIN_PROTOCOL.md`（脚本没有「本文件」的概念），而 `docs/LOCAL_ADMIN_PROTOCOL.md` 的小节只有 `1/1.1/2/2.1/2.2/3/3.1/4/5/5.1–5.8/6/7/8`（无 3.5）；引用的人意是 CORE 的 §3.5（该文档确有 `### 3.5`）。`verification.md` 自身 0 个编号标题，所以里面每个 `§` 引用都必须靠同行文档名归因 | 高（就集成阶段门禁而言）：`check:docs` 是 `npm run check`/`npm run verify` 的必需门禁，只要该行存在且在主检出运行，**集成与最终验收阶段必红**。为什么前几轮的 exit 0 抓不到：文档 worktree 的 `openspec/changes/node-trust-export-ids/` 只有空 `reports/`（`verification.md` 不在其中），门禁在那里必然看不到该行。与被检改动**无关**：本轮 fix 未触碰该文件，该行文字是主 Agent 在处置 RV3-DOCS-F2 时写的（同一意图表述在 `reports/rv3-docs.md` 里用「，」断开，未触发该误判） | 二选一，均在一行文字内：**(A) 补文档名**——`与本文件 §3.5` → ``与 `CORE_PORTS_AND_STORAGE.md` §3.5``（已模拟：`attributeDocument` 会改认 CORE，且 CORE 有 3.5 → 判定跳过）；**(B) 断句**——`§5.4 与本文件 §3.5` → `§5.4；本文件 §3.5`（该引用转为「无法归因」、只统计不判定）。修完请**在主检出**复跑 `node scripts/check-doc-links.mjs` 确认 exit 0（在文档 worktree 跑等于白跑：那里没有 `verification.md`）。**本人未改 `verification.md`**（不在本角色写入范围） |

## 5. 回归检查（任务书第 3 点）

| 检查项 | 方法 | 结论 |
| --- | --- | --- |
| 本轮 diff 是否只动 `docs/CORE_PORTS_AND_STORAGE.md` | C1/C2 | **成立**：1 file changed, 2 insertions(+), 2 deletions(-)；2 个 hunk 只落在第 22 行与第 1361 行 |
| 是否误改封闭词表/合同资产 | `git diff --name-only 3cadb12..39be2f77 -- schemas fixtures compatibility crates` = 0 个文件（C11）；`check-command-catalog` exit 0（C13） | **未误改**：`schemas/**`、`fixtures/**`、`compatibility/**`、`crates/**` 在整条变更范围内零改动 |
| 是否误改其它小节 | C3（-U0 hunk 逐条归小节） | **未越界**：本轮只命中文件头行与判据 32 首行；§9 其余 31 条、§4/§5/§6/§7/§10/§11 本轮均未触碰（§3.5/§4/§6 第 5 条/§7/§9 判据 1/28/§10/§11.3/§11.6–§11.8 的改动属更早轮次） |
| 版本行既有行与体例 | C7/C8 | **通过（既有行）**：1–21 行与 base、上一版逐字相同（含既有乱序）；新行仍单行、位置正确。**体例有一处新缺陷**：全角括号不配对（RV4-DOCS-F1，不影响既有行） |
| 判据编号与交叉引用 | C9/C10/C12 | **通过**：§9 为 1…32 连续；判据 32 子项 ①–⑤ 未动；来源列表新增的跨文档引用可解析；全文无第二处「判据 32」引用需求变化 |
| 门禁复跑（本环境可运行者） | C12 `check-doc-links` = exit 0（380 / 5117）；C13 `check-command-catalog` = exit 0；C14 `check-contract-drift` = exit 1（同一处、跨 WP） | **未引入新失败**：`check:docs` 由 5114 → 5117（+3，来自新增/改写的引用），全部可解析；漂移差异与上一轮同处、无新增 |
| 交付卫生 | C17 | **通过**：两个 worktree 干净、无暂存文件；本轮提交信息 `docs(storage): …` 类型/scope 合规（fixer 已用 commitlint 自查，本人未复跑） |

## 6. 附带判断（非发现）

1. **调度者清单与 §7 标题行**：任务书列出的实际改动小节未单列「§7 标题」（`## 7. \`storage-sqlite\` v4 表结构（…）`）这一 hunk，但 0.14 行以「§7 升级到 v4」覆盖了它，故**不构成发现的遗漏**；任务书「18 处 hunk」与我实测的「-U0 19 个（含文件头 1 个）」一致（默认上下文的 14 个是同一批改动的合并视图）。
2. **「漂移门禁继续逐条成立」**：在本 worktree 单版本上 `check:drift` exit 1（C14），原因是 v4 代码在另一 worktree；该句与 0.10/0.13 等既有版本行同一体例，描述的是**整条变更**的结论（合同 + 实现同批），`verification.md` 也已把该检查登记为跨 WP PENDING。**不构成本轮发现**，但 PASS 必须由 tasks 3.1/6.3 在集成时补跑。
3. **「§7 的 SQL 块已同批更新」的范围**：本次实际只改了 `OWNED_SCHEMA_V1` 那一块（§7.3 的 `owned_node`），§7.4 的 `imported_*` 家族 DDL 未动；该句是「本次更新过 §7 的 SQL 块」的陈述而非「§7 全部 SQL 块都改了」，且 §7 标题已注明「`imported_*` 家族仍为 v3」，读者不会被误导——**不构成发现**。
4. **fixer 日志的表述**：`reports/wp5-docs-fix.log`「第三轮 fix」写「§9 的判据 1–31、33…（本文档共 32 条）编号与措辞均未受影响」，措辞含糊（本文档共 32 条，不存在 33）。这是**实现者报告文本**、不是被检视交付物，且 `verification.md`/判据编号本身无误——**不构成本轮发现**，仅记录备查。
5. **判据 32 ④ 的覆盖面**：仍只显式断言「该 Export 的**会话命令**不可用」，无会话命令由 §6 第 5 条的「存在满足条件的 Export」（已含清单条件）覆盖；RV3 已判「不构成发现」，本轮复读同一结论（且④的来源列表仍含 §6 第 5 条）。

## 7. Assessment

### 7.1 逐检查 ID 结论（计划 ↔ 实际，target `39be2f77`）

| 检查 ID / 来源 | 核对方式 | 结论 |
| --- | --- | --- |
| [PV1] `check:docs`（`plan.md` Project Verify；`verification.md` fix 轮次 3 行） | 本人独立执行 `node scripts/check-doc-links.mjs`（C12） | **PASS / NEW**：exit 0，380 相对链接 / 5117 章节引用，与调度者复跑逐字一致 |
| [PV1] `check:drift` | 本人独立执行（C14）：exit 1，唯一差异仍是 §7 块 1（`OWNED_SCHEMA_V1`）第 20 条语句（`owned_node`） | **BLOCKED / PENDING**：代码在另一 worktree（`migrate.rs` 本地仍为 v3 形状）；与上一轮**同一处、无新增**，本轮未触碰 §5/§7 的 SQL 块与端口签名。PASS 须在 WP1/WP3 并入后复跑（tasks 3.1、6.3） |
| [PV1] `check:agentic` | 未执行（worktree 无 `node_modules`，§1 限制 2） | **BLOCKED / PENDING**：须在集成树补跑（含 `openspec validate --all --strict` 对本次主规范 Purpose 改动的接受性） |
| [PV1] 其余合同门禁 | `check-command-catalog` 独立跑绿（C13）；需 `ajv` 的四道如实记为未执行（§1 限制 3），以「资产零改动」反证（C11） | **已核对/反证成立** |
| [PV3]/[PV4]/[PV5]（代码面） | 代码不在本分支（§1 限制 1） | **NOT_APPLICABLE 于本 WP**（由 tasks 3.2 与 3.1/6.3 覆盖） |
| RV4-DOCS（本条报告） | 定点复核 RV3-DOCS-F1/F2 + 回归（§4/§5） | **PASS / NEW**（唯一新发现为排版级 MINOR，非阻断；见 §7.3） |
| E2E | `plan.md` 记 `not-applicable`（用户批准） | NOT_APPLICABLE（文档 WP） |

### 7.2 design/spec 口径复核（只记与本轮两条原问题相关的部分）

| 决策 / 要求 | 复核结论 |
| --- | --- |
| `design.md` D1（三条件；判定点只有 `server::node_link::catalog::visible_exports` 与 core 的 Owner 侧命令授权且必须同口径） | 成立：0.14 行把「§4 的单点可见性策略」与「§6 第 5 条的 Owner 侧授权判定」并列写成同一组三条件（`未撤销 ∧ export.scopes ∩ grants ≠ ∅ ∧ exportId ∈ exportIds`），与 D1 的三条件及两处判定点一致；判据 32 ③④ 仍与之一一对应 |
| `design.md` D2/D3（`owned_node` 末尾追加 JSON 列；`ALTER TABLE` + 末尾追加，升级库与新建库列清单逐项相等） | 未被本轮触碰；0.14 行的「§7 升级到 v4（`owned_node` 末尾追加 `export_ids_json`，`ALTER TABLE ADD COLUMN`，既有行置空、不得默认放权）」与 D2/D3 及 §7.2/§7.3/§9 判据 28 的文本一致 |
| `design.md` D9（core 侧「清单为空/不含目标 Export 必须拒绝」的核心断言） | 未被本轮触碰；判据 32 ④ 与来源列表（含 §6 第 5 条）仍完整覆盖，本轮只精确化了来源括注 |
| `design.md` D10 的文档影响面清单（NODE_LINK §8.2 + 修订记录、CORE §3.5/§7.2/§7.3/§9 判据 28/§11.6/§11.7/§11.8、LOCAL_ADMIN §5.4 + 修订记录） | 成立：0.14 行的枚举是它的超集（多出 §4、§6 第 5 条、§9 判据 1/32、§10、§11.3，均为实际改动）；`docs/NODE_LINK_PROTOCOL.md` 与 `docs/LOCAL_ADMIN_PROTOCOL.md` 各自带本变更的修订记录（C11 的 4 文件清单），因此 0.14 行**不需要**代述其它文档的改动 |
| `tasks.md` 2.5 的完成条件 | 文档面（文档与代码/DDL 文本一致 + `check:docs`）在本轮范围内成立；`check:drift` 仍跨 WP PENDING（§7.3） |
| `AGENTS.md` §10（一处权威定义、其余写概要并链接）与 §13（结论先行、通俗表达） | 本轮两处改动未引入第二套口径：0.14 行只是自述与指向，判据 32 的来源括注是精确引用 |

### 7.3 结论

- **文档静态收口结论：PASS**。RV3-DOCS-F1（0.14 版本行的节号误标、§5 rust 块不实陈述、枚举漏记三处）与 RV3-DOCS-F2（判据 32 来源列表的裸 `§5.4` 指错小节）**均已解决**；fix 轮次 3 未引入功能性回归（范围、资产/词表、其它小节、版本行既有行、判据编号与交叉引用、可运行门禁均已核对，见 §5）。
- **本轮新发现 1 项，非阻断**：RV4-DOCS-F1（MINOR，纯排版：0.14 版本行全角括号 3 开 / 4 闭，本轮新引入；一字符即可修）。按调度者的收口约定，如实登记并按 **PASS** 收口，**是否再开一次 fix 由主 Agent 裁定**。
- **两条 PENDING 不改变本轮静态判断**：`[PV1]` 的 `check:drift`（代码在另一 worktree，本地必然失败于同一处 `owned_node` 语句）与 `check:agentic`（worktree 无 `node_modules`）继续按 `verification.md` 记为 BLOCKED/PENDING，须由 tasks 3.1/6.3 在集成/候选阶段复跑；本轮 diff 未触碰漂移门禁的绑定面（§5 的 rust 块与 `crates/core/src/ports.rs` 零改动，§7 的 SQL 块与 §7.3 DDL 本轮未动），因此不引入新漂移。
- **给主 Agent 的呈报口径**：2.5 的文档面可在本轮按 **PASS** 闭环（`verification.md` 的 RV4-DOCS 行可记 PASS/NEW，并保留两条 PENDING 检查行）；若决定修 RV4-DOCS-F1，只需在 `docs/CORE_PORTS_AND_STORAGE.md` 第 22 行删掉一个 `）`（并在修后复跑 `check-doc-links.mjs`），不需要新的完整检视轮次。
- **范围外发现 1 项（OOS-1 / RV4-DOCS-F2，MAJOR）**：`verification.md`:70 会让**主检出**的 `check-doc-links` exit 1（C18），与本次被检改动无关、但在集成阶段会阻断 `npm run check`；修法是一行文字（§4.3）。前几轮与本轮 fixer 的 `check:docs` exit 0 都是在文档 worktree 上跑的，而该 worktree 的 `openspec/changes/node-trust-export-ids/` 只有空 `reports/`（没有 `verification.md`），所以那些 PASS 从未覆盖本行。**本报告 `result` = PASS 只针对被检 artifact 的文档线**，OOS-1 需主 Agent 处置。

## 8. handoff_index

```yaml
handoff_index:
  - task_id: "2.5"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "39be2f7792b2ccfaa143b386d64fc731eedf2442"
    evidence_type: REVIEW
    evidence_id: RV4-DOCS
    report_path: "reports/rv4-docs.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "对 RV3-DOCS（target 7d5abeb8，文档静态 PASS／报告 result BLOCKED）的范围受限收口 recheck。新建只读 reviewer，未继承实现/fix/RV1/RV2/RV3 对话；在 worktree D:\\Project\\acp-remote-wt\\export-ids-docs、base 3cadb12d、target 39be2f77 上重读 fix 轮次 3 的 diff（7d5abeb8..39be2f77，1 文件 2 增 2 删、2 hunk @22/@1361）与受影响上下文（文件头 1–23 行、§4 全节、§5/§5.3 区间、§6 第 5 条、§7 标题/§7.2/§7.3、§9 全部 32 条）、design D1/D3/D9/D10、tasks 2.5、verification.md 的 F1/F2 行与仓库回归面后给出。RV3-DOCS-F1/F2 均判定**已解决**；新增 1 项非阻断排版项（RV4-DOCS-F1 MINOR），无 CRITICAL/MAJOR，无功能性回归。两条 [PV1] 检查（check:drift/check:agentic）仍按 verification.md 记为 PENDING（跨 WP／无 node_modules），按调度者收口约定不改变本轮静态判断——见本报告 §7.3"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "39be2f7792b2ccfaa143b386d64fc731eedf2442"
    evidence_type: CHECK
    evidence_id: "PV1 (check:docs)"
    report_path: "reports/rv4-docs.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本人于 target 39be2f77 的 worktree 独立执行 `node scripts/check-doc-links.mjs` → exit 0：`380 relative links, 5117 section refs across 324 markdown files`（node v24.19.0；worktree 无 node_modules 也不需要）。与调度者主 Agent 复跑逐字一致；本轮新增/改写的引用（§3.5/§4/§10/§11.3 与 `LOCAL_ADMIN_PROTOCOL.md` §5.4）全部可解析。执行记录见本报告 §3 C12"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "39be2f7792b2ccfaa143b386d64fc731eedf2442"
    evidence_type: CHECK
    evidence_id: "WP5-SUPP (check:command-catalog)"
    report_path: "reports/rv4-docs.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本人于 target 39be2f77 独立执行 `node scripts/check-command-catalog.mjs` → exit 0（`command catalog OK: 12 commands`），任务书第 3 点的回归项；同时作为「合同资产未改动」的反证：`schemas/**`、`fixtures/**`、`compatibility/**`、`crates/**` 在 3cadb12..39be2f77 内命中 0 个文件（本报告 §3 C11/C13）"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "39be2f7792b2ccfaa143b386d64fc731eedf2442"
    evidence_type: CHECK
    evidence_id: "PV1 (check:drift)"
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "本人于 target 39be2f77 独立执行 `node scripts/check-contract-drift.mjs` → exit 1，唯一差异仍为 §7 块 1（OWNED_SCHEMA_V1）第 20 条语句（`owned_node`）：本 worktree 的 crates/storage-sqlite/src/migrate.rs 仍是 v3 旧形状（WP1/tasks 2.1 在另一 worktree 未并入）。与 RV3-DOCS 在 7d5abeb8 的记录为同一处、无新增；本轮 diff 只动文件头一行与判据 32 首行，未触碰 §5 的 rust 块、§7 的 SQL 块与 §7.3 DDL，故未引入新漂移。待补 ID：PV1(check:drift)；门禁：check-contract-drift.mjs；须在 WP1/WP3 并入后的集成/候选阶段复跑（tasks 3.1、6.3）。执行记录见本报告 §3 C14"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "39be2f7792b2ccfaa143b386d64fc731eedf2442"
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
    target_revision: "39be2f7792b2ccfaa143b386d64fc731eedf2442"
    evidence_type: CHECK
    evidence_id: "OOS-1 (check:docs, 主检出)"
    report_path: "reports/rv4-docs.md"
    result: FAIL
    evidence_status: NEW
    applicability_basis: "范围外复核（不是被检 artifact 的失败）：在主检出 D:\\Project\\acp-remote（含未跟踪的 openspec/changes/node-trust-export-ids/）执行 `node scripts/check-doc-links.mjs` → exit 1，唯一问题 = `openspec/changes/node-trust-export-ids/verification.md:70: §3.5 在 docs/LOCAL_ADMIN_PROTOCOL.md 中不存在`（该行子句同时指名 LOCAL_ADMIN 与该 § 引用，脚本无「本文件」概念）。该文件由主 Agent 维护，不在本轮 scope 内；集成阶段的 `npm run check` 会在主检出上遇到它，须先修（两种一字符方案见 reports/rv4-docs.md §4.3）再复跑。文档 worktree（target 39be2f77）上的同门禁为 PASS/NEW（380/5117），但该 worktree 的 openspec/changes/node-trust-export-ids/ 下只有空 reports/（无 verification.md，0 个 md），故两结果不矛盾。执行记录见本报告 §3 C18"
    source_evidence: NOT_APPLICABLE
```

## 9. 结论

**文档静态收口复核：PASS**（RV3-DOCS-F1、RV3-DOCS-F2 均已解决；fix 轮次 3 未引入功能性回归；新发现 RV4-DOCS-F1（MINOR，纯排版：0.14 行全角括号不配对，本轮新引入）已如实登记，非阻断，是否收口由主 Agent 裁定）。

**本报告 `result`：PASS**（按调度者的收口约定，仅针对被检 artifact = `docs/CORE_PORTS_AND_STORAGE.md` @ `39be2f77`）。`[PV1]` 的 `check:drift`/`check:agentic` 仍为 PENDING，须在 WP1/WP3 并入后的集成/候选阶段复跑（tasks 3.1、6.3），不改变本轮静态结论。

**请主 Agent 处置（范围外，阻断集成门禁）**：`verification.md`:70 会让**主检出**的 `check:docs` exit 1（OOS-1 / RV4-DOCS-F2，§4.3），修完在主检出复跑 `node scripts/check-doc-links.mjs` 即可。
