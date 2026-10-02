# CR-PM 合并后差异检视报告（Round 1）

> 持久化说明：本报告由 reviewer 实例 `reviewer-PM`（run `9a86b747-ae93-463d-8f30-0819ea274b75`，只读工具集、无 shell）产出。主 Agent 在 Round 2 独立复核指出「该报告从未落盘」，本文件于 2026-10-01 依据其**交付全文**忠实补录（见 Round 2 的 CR-PM-F7）。

## Shared Report

- **task_id**: `6.8` · **work_package**: `U1` · **role**: reviewer · **phase**: merge · **round**: 1 · **stage**: main
- **target_revision**: `69f1ac1bc6af81461d199257f512965f646ec55c`
- **result**: **FAIL**（0×CRITICAL / **2×MAJOR** / 4×MINOR）
- **限制**：本实例**无 shell、无 git 工具**，四条 `git` 产物未能亲跑，改以 `.git/refs/*` 直读、日志求和、内容核对作**证据替代**，并逐项标注。
- **隔离**：全新实例，未参与任何工作包实现、整合或合并；**把派发文本里的断言当作待核输入，不当作证据**。

## A. 合并新增差异的独立复核

1. **四条 git 产物**：**无法亲跑**（无 shell）。可独立确认的部分：`.git/refs/heads/main` 直读 = `69f1ac1bc6af81461d199257f512965f646ec55c`，与 Target 逐字一致；`merge-u1-main-diff-materials.md` 记录 shortstat 为 `130 files changed, 23103 insertions(+), 213 deletions(-)`。
2. **「零产品代码差异」与 62/68 划分**：零差异本身无法复现，但做了**内部一致性重算**——逐条数 130 条路径：`crates/**` 38、`docs/**` 8、`schemas/node-link/v1` 2、`fixtures/node-link/v1` 4、`compatibility/**` 2、脚本/配置若干 ⇒ 产品/合同/文档面 **62**；`openspec/changes/session-resume/**` **68**（含 5 份 specs、`dispatch-queue.jsonl`、`.openspec.yaml`、`reports/` 下 55 个 `.md` + 1 个 `.txt`）。**62 + 68 = 130，与 shortstat 一致**。
3. **`69f1ac1` 是否只改报告**：**是**。只动 `reports/merge-u1-integrate-wp4.md` 一行；`cr4-review.md` 实际标题为 `## Review` / `### Review Context` / `### Findings` / `### Assessment` / `### handoff_index`，**无 §9**；改后指向 `wp4-coder.md:100 ### PV1` 与 `cr4-review.md` 的 `Assessment` 小节（行 50），**两处都真实存在**。
4. **68 个规划/证据资产入库风险**：`plan.md` 16 个 `##` 章节齐全；`verification.md` 18 个 `##` 章节连续无缺；`tasks.md` 1.1→9.1 连续；`dispatch-queue.jsonl` 36 行逐行合法 JSON。**`.log` 确未入库**：130 条清单中零个 `.log`，`.gitignore:22` 的 `*.log` 与 `.gitignore:27` 的 `openspec/changes/**/reports/**/*.log` 双重规则生效，磁盘上 80+ 个 `.log` 保留但未跟踪。
5. **夹带检查**：**无**。130 条路径全部落在授权范围，无临时文件、调试产物或其它目录的未跟踪文件。

## B. 抽查断言的逐条佐证状态

| # | 断言 | 状态 | 依据 |
| --- | --- | --- | --- |
| 1 | workspace test **1074 passed / 0 failed / 2 ignored** | **已佐证** | 把 `merge-u1-main-final-workspace-test.log` 的 91 条 `test result: ok.` 逐条求和 = **1074**，其中 2 条带 `1 ignored`；与候选日志逐项相同 |
| 2 | fmt exit 0 / clippy 0 诊断 | **已佐证** | `merge-u1-main-final-pv1.log:4 FMT_EXIT=0`、`:7 CLIPPY_EXIT=0`，全文无 warning/error |
| 3 | 门禁 8/9/10 exit 0 | **已佐证** | `merge-u1-main-final-remaining-gates.log`：`BOUNDARIES_EXIT=0` / `DRIFT_EXIT=0` / `AGENTIC_EXIT=0` |
| 4 | **`npm run check` exit 0、十道门禁全绿** | **被证伪** | `merge-u1-main-final-check.log` 末行 `CHECK_EXIT=1`，第 7 道 `check:docs` 报 **3 个错误**（全指向未跟踪的 `dr1-dependency-review-round19.md` 第 18/22 行）；merger-A6 报告明写「完整 `npm run check` exit 0 **未达成**」、6.7 判 **BLOCKED** |
| 5 | `Totals: 20 passed, 0 failed` 代表全绿 | **误引** | 该串来自 `check:agentic` 内部的 `openspec validate`（20 个 spec/change 条目），只覆盖第 10 道 |
| 6 | 「零产品差异」 | **未证实** | 两条 `--stat` 空输出只见于 merger 产物 |
| 7 | 零冲突、父提交对 `(81e350f, 2ed142d)` | **未证实** | 仅见于 merger 产物 |
| 8 | `.log` 未入库 / 无夹带 | **已佐证** | 见 A.4、A.5 |

## Findings

| ID | Severity | Location | Impact | Recommendation |
| --- | --- | --- | --- | --- |
| **CR-PM-F1** | **MAJOR** | `tasks.md` 6.7（工作区未提交改动）；同一不实陈述亦出现在派发简报中 | 权威记录写入了一条被日志直接证伪的门禁结论，并把未完成的 6.7 结为完成；违反「不得把待补证据写成 PASS」。下游（8.2 C2、`workflow check --stage final`、最终验收）会以此为前提 | 把 6.7 改回未勾选或改写为 BLOCKED 实况（PV1 PASS + PV2 exit 1 卡在 `check:docs`），删除 `Totals: 20 passed` 作为全绿依据的表述；6.7 只能在 merger 重跑 `npm run check` 取得 exit 0 后勾选 |
| **CR-PM-F2** | **MAJOR** | HEAD `69f1ac1`；工作区 delta vs HEAD | 已合入的 `plan.md` 仍含被门禁判错的引用；该修正**只存在于未提交的工作区**。同时 `merge-u1-main.md`、`merge-u1-main-diff-materials.md`、`dr1-dependency-review-round19.md` 三份关键证据**均未跟踪**；`verification.md` 的 `## Merge History` 仍是空表 | 主分支在被检视版本上对合同门禁是**红**的；干净 checkout 该提交会复现失败；合入这一事实只存在于未入库文件中，审计链断在版本控制之外 | 先把 `plan.md` 修正与三份证据报告提交进版本控制；填 `## Merge History` 的 M1 行；再由 merger 在最终 HEAD 上重跑 `npm run check` |
| **CR-PM-F3** | MINOR | `reports/merge-u1-integrate-wp4.md:459` | 同一文件第 459 行仍把某个指向 `cr4-review.md` 的节号写成「与 wp4-coder.md 及 cr4-review.md 的第九节报告的 131/0/2 逐字一致」，而 `cr4-review.md` **无第九节**。（为避免复述触发门禁，此处不逐字引用原句。）该行位于 ```yaml 围栏内，`check-doc-links.mjs` 对围栏内行 `if (inFence) return;`，故门禁看不见 | merger 报告「剩余错误 2→1，本角色负责的那处已闭环」只对门禁成立；同一份报告里仍留着一个不存在的节号引用 | 一并改为指向真实小节 |
| **CR-PM-F4** | MINOR | `reports/premerge-receipt-u1.md` 与 `verification.md` 的 `## Premerge` 块的 `contract_digest` | 两处由 `sha256:fed00eb6…` 改写为 `sha256:2adbf605…`，但 receipt 自身声明是「premerge 门禁 PASS 后**原样固化**」的门禁输出 | receipt 不再等于门禁实际判定的那份契约摘要，削弱可审计性；合并后无法复跑取回原值 | 在 receipt 中保留原始摘要并另起一行记录更新原因，而不是就地覆盖 |
| **CR-PM-F5** | MINOR | `reports/merge-u1-main.md` §2.2「暂存内容逐项核对」 | 该表逐项枚举合计 70 项，而同表结论与总数都是 **68** | 核对表的算术与自身总数不符，削弱「逐项核对通过」的可信度 | 把 `57 份` 改为 `55 份`，使枚举合计等于 68 |
| **CR-PM-F6** | MINOR | `verification.md` 的 `## Merge History` | 表格为空，而 tasks 6.6 要求「记录实际提交」 | 合并事实缺权威落点 | 随 F2 一并补 M1 行 |

## Assessment

`CR-PM-F1` 与 `CR-PM-F2` 均为已证实且未解决的 MAJOR：派发简报里的「候选门禁与主分支回归均已通过 / PV2 exit 0 十道门禁全绿」这一前提**不成立**——合并后主分支 `npm run check` 是 exit 1，6.7 被记录的结论与日志直接矛盾；被检视版本 `69f1ac1` 本身对合同门禁为红，且闭环只存在于未提交的工作区与未跟踪文件中。合并本身的机械面（父提交对、零冲突、零产品差异、`.log` 排除、无夹带、`69f1ac1` 只改一行报告且引用真实）**未见夹带、未见产品面回归**。

### handoff_index

- `task_id: "6.8"` · `work_package: U1` · `role: reviewer` · `phase: merge` · `round: 1` · `stage: main` · `target_revision: "69f1ac1bc6af81461d199257f512965f646ec55c"` · `evidence_type: REVIEW` · `evidence_id: CR-PM` · `result: FAIL` · `evidence_status: NEW` · `report_path`: 本文件
- `applicability_basis`: 只读检视 target_revision 69f1ac1；合并机械面已佐证；PV2 断言被 `merge-u1-main-final-check.log(CHECK_EXIT=1)` 证伪。无 shell/git，四条 git 产物未能亲跑，已逐项标注为证据替代。