# CR-PM 合并后差异检视报告 · Round 2（复核轮）

> 持久化说明：本报告由 reviewer 实例 `reviewer-PM2`（run `6f9fa48c-2745-4f5d-9fd4-2fc59bffc2b4`，只读工具集，**无 shell、无写权限**）以全文返回、由主 Agent 落盘。

## Shared Report

- **task_id**: `6.8` · **work_package**: `U1` · **role**: reviewer · **phase**: merge · **round**: 2 · **stage**: main
- **target_revision**: `0cc795fcd5656ab4b250ada2394694d0b890eb24`
- **result**: **FAIL**（0×CRITICAL / **3×MAJOR**（F7、F8、F9）/ 2×MINOR（F10、F11））
- **隔离**：全新实例，未参与本变更任何实现、整合或合并。
- **限制**：**无 shell**，未执行 `git diff --stat` / `git show --stat` / `git ls-files` / `check-doc-links.mjs` / `npx commitlint` / `git status`；凡依赖这些命令的子项均改用只读替代证据（`.git/refs/*`、`.git/logs/*`、源码与日志内容）并**逐条标注为「证据替代」而非「亲跑」**。

## 上一轮 6 条 findings 的逐条闭环判定

| ID | 判定 | 依据 |
| --- | --- | --- |
| **CR-PM-F1** | **已闭环** | ① `tasks.md:66` 完整保留「撤回结案」记录，三条不实理由与结案条件俱在，未被抹掉；② 重新勾选所引 `merge-u1-main-closure-npm-check.log` 首行 `# revision: 3458542b9edf…` 与 `tasks.md:65` 声明逐字一致、末行 `# exit: 0`，逐道日志十道 `EXIT=0` 齐全；③ 完成说明全文已无 `Totals` 字样，`merge-u1-main.md` 反而显式声明「不以 `Totals: 20 passed, 0 failed` 充当全绿依据」 |
| **CR-PM-F2** | **已闭环** | ① `.git/logs/refs/heads/main` 证明 `plan.md` 修正随 `20c1623` 入库；`finalhead2-npm-check.log`（`# revision: 0cc795f…`）中 `@@@ check:docs EXIT=0`，输出 `doc links OK: 404 relative links, 9379 section refs across 485 markdown files`；并独立核对了 `plan.md:370` 修正点本身符合「取最后一个可解析命中」的归属规则。② 三份报告已在版本控制内（未跟踪清单为空 + `.md` 不被 `.gitignore` 排除）。③ `## Merge History` 四行齐备 |
| **CR-PM-F3** | **已闭环** | `merge-u1-integrate-wp4.md:459` 现指向 `cr4-review.md` 的 `Assessment` 小节，该小节经 grep 确认存在（行 50），不再含不存在的节号 |
| **CR-PM-F4** | **已闭环** | `premerge-receipt-u1.md:7` 同时保留原值 `sha256:fed00eb6…` 与更新值 `sha256:2adbf605…`（后者与 `verification.md:330` 逐字一致），并写明合并后无法重跑取回原值的原因 |
| **CR-PM-F5** | **部分闭环** | `merge-u1-main.md:52` 已改为 55 + 1 = 68，算术自洽；但同节第 167 行仍写「58 份角色报告」与之矛盾 → **CR-PM-F11** |
| **CR-PM-F6** | **已闭环** | `verification.md:357` 的 M1 行记录合并提交 `0d2be6d`（`--no-ff`、父提交对、零冲突）与修正提交 `69f1ac1`，`.git/logs/refs/heads/main` 可核实 |

## 第 3 节：四项新增独立复核结论

### 3.1 合并后新增提交是否只改规划/证据资产与词表、零产品代码 —— **PASS（附限制）**

以 `.git/logs/refs/heads/main` 逐条核对，`81e350f` 之后共 **13** 次 ref 更新：合并提交 `0d2be6d`；`69f1ac1`、`20c1623`、`3458542`、`6af2549`、`774ee9e`、`976b34c`、`ac5de2a`、`661932d`、`2ec1f0f`、`ee51795`、`0cc795f` 共 11 个 `docs(...)`；`cae3dbd` 为**唯一的 `build(repo)`**（词表）。**合并后 12 个提交无一是 `feat`/`fix`/`refactor`**，主题层面无产品代码变更。
门禁级旁证：`0cc795f` 上 `check:boundaries`（12 crate 依赖方向与 §5 矩阵一致）与 `check:drift`（§7 的 36 条 DDL、§5 的 15 trait/96 方法签名逐条一致）均 `EXIT=0`。
⚠️ 文件级断言未能亲跑 `git diff --stat` / `git show --stat`。

### 3.2 提交信息合规 —— **PASS（附限制）**

证据替代：`finalhead2-commitlint.log` 首行 `# revision: 0cc795f…`、`# command: npx commitlint --from "81e350f^" --to HEAD`、`# exit: 0`，**全文无任何 problem 行**（此前 exit 1 / 10 problems）。
词表两处说明确实存在且如实：`commitlint.config.mjs:46` 增列 `session-resume`；`:57-63` 对 `test` 写明「历史兼容项，不是交付面边界…历史提交 `3484541` 把它误用成了 scope…**只改测试请用 `test(<交付面>)`**」；`AGENTS.md:232` 同段落复述来历并加「新增提交不要用它」。

### 3.3 最终 HEAD 的门禁证据 —— **PASS（本轮最强的独立复核项）**

- **revision 绑定**：5 份 `commitlint-scope-fix-finalhead2-*.log` 首行**全部**为 `# revision: 0cc795fcd5656ab4b250ada2394694d0b890eb24`，与 `.git/refs/heads/main` 实际 HEAD **逐字相等**。
- **workspace test**：reviewer **逐行汇总日志中全部 91 条 `test result:`**（自加总，不采信自述）—— `1074 passed`、每条 `0 failed`、恰好 2 条 `1 ignored`，合计 **1074 / 0 / 2**，与 `tasks.md:77` 声明逐字相符；末行 `# exit: 0`。
- **十道门禁逐道**：`finalhead2-npm-check.log` 中十道齐全，各带 `EXIT=0`（`check:docs` = `404 relative links, 9379 section refs across 485 markdown files`）。Rust 另两条：fmt `# exit: 0`、clippy `# exit: 0` 且无任何诊断输出。

### 3.4 是否引入夹带 / 工作区洁净度 —— **FAIL（CR-PM-F8）**

- **`.log` 未入库 ✅**：`.gitignore` 双重排除（`*.log` 与 `openspec/changes/**/reports/**/*.log`，后者附「不要 `git add -f` 强行入库」的策略说明）；未跟踪清单为空，`reports/` 下 150 余份 `.log` 均按此策略留在工作区。
- **工作区不干净 ❌**：相对 Target `0cc795f` 存在**未暂存**改动 `openspec/changes/session-resume/tasks.md`（+6 / −3），内容是 7.1、8.1、8.2 三行由未勾选改为已勾选并各加一条完成说明。**派单所述「工作区干净」与事实不符。**

## Findings

| ID | Severity | Location | Impact | Recommendation |
| --- | --- | --- | --- | --- |
| **CR-PM-F7** | **MAJOR** | `verification.md:358` 与 `reports/merge-u1-main.md:462` | 两处把 `reports/cr-pm-post-merge-review.md` 记为证据/触发来源，但该文件**在工作区、仓库内与配套 worktree 中均不存在**；且 `## Review Findings` 表**没有任何 CR-PM 行**，即 Round 1 的 FAIL 判定与其 6 条 findings 在仓内**无处可查** | ① 把 Round 1 报告落盘并入库（或按实际文件名订正引用）；② 在 `## Review Findings` 补 CR-PM-F1..F6 六行 |
| **CR-PM-F8** | **MAJOR** | 工作区 `tasks.md`（相对 `0cc795f` +6 / −3，未暂存） | ① 本轮检视绑定 `0cc795f`，但读到的 `tasks.md` 含目标版本之外的内容，违反「固定 base/target」；② 与上一轮 **F2（关键记录未入库）同类复发**；③ 若按 HEAD 交付，final 验收会看到 7.1/8.1/8.2 未完成而报告声称已完成 | 把这 3 行完成记录提交（或撤回）使工作区回到 `0cc795f` 状态；后续派单在正文写明「工作区不干净时不得声称干净」 |
| **CR-PM-F9** | **MAJOR** | `verification.md` 的 `## Checks` 表（C1 / C2 两行） | tasks 8.1/8.2 明文要求「记录版本、命令、退出码与结果到 verification 的 `## Checks`」。该表 C1/C2 行仍指向**候选期**日志（target `2ed142d`）；主分支与最终 HEAD 的全部门禁运行在 `## Checks` 中一行也没有 | 在 `## Checks` 增最终 HEAD 的 C1/C2 行。**记录层改动不触及 `plan.md`/`tasks.md`，不使 `contractDigest` 失效** |
| **CR-PM-F10** | MINOR | `tasks.md:65`–`:66` | 第 65 行是「闭环（已提交、新 HEAD 门禁 exit 0）」；紧接的第 66 行是「撤回结案」，其中以**现在时**写「现状：…尚未提交，故 HEAD `69f1ac1` 上的合同门禁仍为红」并给出**已经满足**的结案条件。两条同日期、顺序相反 ⇒ 自上而下读完的读者最后看到的是**已被推翻的现状** | 在第 66 行末追加带日期的结案指向，或把「现状」改为「撤回当时现状」 |
| **CR-PM-F11** | MINOR | `reports/merge-u1-main.md:167` vs `:52` | F5 的算术只修了 `:52`，同节 `:167` 仍写「58 份角色报告」，按该口径合计 ≠ 68 | 把 `:167` 改为与 `:52` 一致的份数 |

> **观察（不计入严重级）**：`verification.md:108`（PV2 预变更基线行）仍以「PASS / exit 0（`Totals: 20 passed, 0 failed`）」记录，该括注正是 CR-PM-F1 批评的取据方式。它属 `81e350f` 基线行、**非本次 diff 引入**，按「diff review 只报由该 diff 引起或使其可达的问题」不立项；建议主 Agent 下次触碰该表时改为逐道退出码。

## Assessment

- **F1–F6 全部闭环**（F5 留一处 MINOR 残留，不升级）。
- **第 3 节**：3.1 PASS（限制：文件级 diff 未亲跑）、3.2 PASS（限制：commitlint 未亲跑）、**3.3 PASS（本轮最扎实：revision 与 HEAD 逐字相等、1074/0/2 由 reviewer 逐行加总、十道门禁逐道 `EXIT=0`）**、3.4 FAIL。
- **阻断项 F7/F8/F9 均为交付记录层缺陷，不是产品代码缺陷**；产品代码面与最终 HEAD 的门禁绿本身未发现任何问题。三项最小修复均为记录层落笔，**不触及 `plan.md`/`tasks.md` 的契约内容**，因此不使 `contractDigest` 失效、不需重开 DR1。

## 结论：**FAIL**（0×CRITICAL / **3×MAJOR** / 2×MINOR），绑定 Target Revision `0cc795fcd5656ab4b250ada2394694d0b890eb24`

### handoff_index

- `task_id: "6.8"` · `work_package: U1` · `role: reviewer` · `phase: merge` · `round: 2` · `stage: main` · `target_revision: "0cc795fcd5656ab4b250ada2394694d0b890eb24"` · `evidence_type: REVIEW` · `evidence_id: CR-PM` · `result: FAIL` · `evidence_status: NEW` · `report_path`: 本文件
- `applicability_basis`: 81e350f..0cc795f 合并后差异复核；F1–F6 逐条闭环（6/6）；提交信息合规、最终 HEAD 门禁证据核实通过；新报 3×MAJOR + 2×MINOR。无 shell/git，多个子项以只读等价证据替代并已逐条标注。