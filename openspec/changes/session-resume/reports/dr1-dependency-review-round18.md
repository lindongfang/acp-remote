# DR1 Round 18 — 依赖声明审查报告（Review Type: plan）

> 持久化说明：本报告由**第 18 个全新独立 reviewer 实例**（只读工具集，无 shell、无写权限）以全文返回、由主 Agent 落盘。
> **`contractDigest` 由主 Agent 代实测**：`workflow check --stage plan --json` → `result=FAIL`、`contractDigest=sha256:fed00eb68cb7ef3cfab595a49374a6bb434807d770151c9e8b2d19bea7bb96ba`（**与派发给定值逐字一致**）、`requirementsDigest=sha256:5394327093e57d05b49ea4b04db708fcb97160791ea6cca8cec5a833154408fa`（**与 Round 14–17 相同 ⇒ 行为契约一字未动**）、`errors[]` 仅一条（DR1 表未绑定当前摘要，即本轮待登记动作）。

```yaml
task_id: "NOT_APPLICABLE（规划门禁，Round 18）"
work_package: NOT_APPLICABLE
role: reviewer
phase: plan
round: 18
stage: plan
target_revision: "sha256:fed00eb68cb7ef3cfab595a49374a6bb434807d770151c9e8b2d19bea7bb96ba"
result: PASS
issues: "0×CRITICAL / 0×MAJOR / 4×MINOR（F78–F81）/ 1×SUGGESTION（F82）"
```

## 一、`plan.md` 的 8 处 `Branch / Worktree` 列改写 —— **判定：理由成立、改写正确、门禁可过、信息可恢复、无新不一致**

**① 理由成立（门禁源码自证）** —— `workflow-check.mjs:667`：

```js
else if (isCurrent && nonempty(planned) && !/[<]/.test(planned)
         && worktree.trim().replace(/\\/g,'/') !== planned.trim().replace(/\\/g,'/'))
  fail(`… worktree（…）与计划 Branch / Worktree（…）不一致`);
```

比较只对**当前轮**生效，唯一豁免是 `planned` 含 `<`。原值 `wt/wp1` 既不含 `<`，又永远不可能等于 `verification.md` 里的绝对路径 ⇒ **对每个 WP 的当前轮都必然 FAIL**。改写后 8 行统一为 `` `<worktree：provisioner 分配并登记，见 verification.md 的 ## Worktree Handoff>` ``，命中豁免分支；占位值内不含 `|`，8 行仍各 11 格，未破坏 `readTable.pick` 的列下标。

**② 信息未丢失到不可恢复**：映射落点从 `plan.md` 迁到 `verification.md` 的 `## Worktree Handoff`（按 (WP, Attempt) 逐轮记录，信息量严格不少于原来的「一包一名」）。真正从计划里消失的是**分支名**（旧符号隐含 `agentic/session-resume-wp1`，新占位符不含），分支名现仅见于 provisioner 报告。这是可接受取舍（豁免靠 `<`，分支名是稳定值、绝对路径才是运行时产物），记为 F82。

**③ 无新不一致**：`plan.md` 全文检索 `wt/` 仅剩 `:416` 的 `## Merge Strategy` 的 `Merge Worktree` 行（`wt/u1` + 真实绝对路径并列），该行不在 `## Work Packages` 表内、不参与 `:667` 比对；`## Shared File Ownership` 与 `## Execution Waves` 内零 `wt/*` 引用，无需同步。

## 二、`## Worktree Handoff` 重写 + provisioner 身份登记 —— **结构与判据全部达标，唯一实质缺陷是 3 行基线语义记错**

**① 15 行 vs 台账逐行对账（全对）**：`dispatchAttempts`（`dispatch.mjs:182-196`，取每 (wp, attempt) 首条 `coding`/`fixing`）给出认领事实。15 个 (WP, Attempt) 与表内 15 行一一对应，无重复、无缺轮、无多余（U1 attempt 1 状态 `superseded` 不在覆盖范围，新表已无 U1 行 —— 正确，U1 不是 Work Package）。

| (WP, Att) | 台账认领执行者 | 表内 Executor | 台账首次执行 | 表内 Received At | Received ≤ 认领 |
| --- | --- | --- | --- | --- | --- |
| WP1/1 | coder-A | coder-A ✓ | 02:38:55.044Z | 02:38:00Z | ✓ |
| WP2/1 | coder-B | coder-B ✓ | 02:38:56.819Z | 02:38:00Z | ✓ |
| TP1/1 | tester-A | tester-A ✓ | 02:38:58.611Z | 02:38:00Z | ✓ |
| TP1/2 | tester-A2 | tester-A2 ✓ | 05:39:22.263Z | 05:39:00Z | ✓ |
| WP3/1 | coder-C | coder-C ✓ | 12:13:42.257Z | 12:13:00Z | ✓ |
| WP3/2 | coder-C | coder-C ✓ | 12:44:31.220Z | 12:44:00Z | ✓ |
| WP4/1 | coder-D | coder-D ✓ | 16:08:41.887Z | 16:08:00Z | ✓ |
| WP5/1 | coder-E | coder-E ✓ | 16:08:43.742Z | 16:08:00Z | ✓ |
| WP5/2 | coder-E2 | coder-E2 ✓ | 06:34:58.255Z | 06:34:00Z | ✓ |
| WP6/1 | coder-F | coder-F ✓ | 07:23:26.796Z | 07:23:00Z | ✓ |
| WP6/2 | coder-F2 | coder-F2 ✓ | 07:54:19.019Z | 07:54:00Z | ✓ |
| WP6/3 | coder-F3 | coder-F3 ✓ | 08:17:32.545Z | 08:17:00Z | ✓ |
| TP2/1 | tester-A | tester-A ✓ | 09:03:14.970Z | 09:03:00Z | ✓ |
| TP2/2 | tester-A2 | tester-A2 ✓ | 09:33:15.069Z | 09:33:00Z | ✓ |
| TP2/3 | tester-A3 | tester-A3 ✓ | 10:09:53.875Z | 10:09:00Z | ✓ |

**② `Received At`**：门禁 `:686-693` 要求 `Received At <= Date.parse(claim.startedAt)`。**15/15 满足**（最紧的是 TP2/1，余 15 秒；WP5/2 余 58 秒），全部为可解析 ISO 8601。

**③ `Baseline Revision`**：15 行只用 5 个 SHA，全部 40 位、且在 merger/provisioner 报告中作为实测提交出现（`81e350f…`、`b0a387b…`、`8a08db8…`、`5ab7e9d…`、`95051f9…`）。门禁 `:670-671` 的 `git rev-parse --verify` 本实例无 shell 未能实测，但静态证据链完整、无编造形态。**但 3 行的取值语义不成立 → F79。**

**④ provisioner 身份 ↔ 报告路径自洽**：`Provisioner` 列为裸 token `provisioner-P0`…`P4`；`## Handoff Index` 新增 5 行登记各自报告（`P0→provisioner-worktrees.md`、`P1→provisioner-repoint-wp3.md`、`P2→provisioner-repoint-wp4-wp5.md`、`P3→provisioner-repoint-wp6-and-tp2.md`、`P4→provisioner-repoint-tp2.md`），与新表 `Evidence` 列逐行对应，5 份报告均可读。这同时**闭合了 Round 17 的第 2 条残余风险**（旧格式 `Provisioner` 单元格含空格整句，`provisionerReports.has(…)` 可能恒不成立）。门禁 `:674-678` 的三条核对逐行满足。

**⑤ 重写未丢失原有信息**：TP1/TP2「并入」决定 ✅（`## Dispatch Reconciliation` 说明段、`plan.md:329/340/414`、`## Planning Findings` 的 DR1-F71）；U1 交付单元登记 ✅（5 条 merger DELIVERY 行 + `## Premerge` 的 `delivery_unit: U1`）；wp6/tp1 重定向历史 ✅（P1–P4 四行 + `## Dependency Handoffs` 的重定向机制段）；旧散文行内容 ✅（已改为表后散文）。唯一例外见 F79。

## 三、Round 17 遗留项的确认

| 项 | 判定 | 依据 |
| --- | --- | --- |
| **F72**（7 条 DELIVERY 行错位） | ✅ 已解决 | 7 行现全在 `## Handoff Index` 表体内；逐条比照 `readiness()`（`:733-748`）的五个条件 **7/7 全中** |
| **F73**（3 个执行者未登记） | ⚠️ 2/3 已解决 | ✅ `reviewer-D`（CR4 行）、`reviewer-T3`（CR8 行）已补；❌ `tester-A2` 因 `**` 加粗切不出裸 token → **F80，仍阻塞 premerge** |
| **F75**（拼接残留断句） | ⚠️ 主体已解决 | ① `:135` 截断半句已删（全文 `DR1-F29` 只剩一处完整条目）；② `:172` 孤儿半句已补回行首、主语与时点限定；③ 但同段 `:179`「目前无下游接入」仍与 `:161`「集成基线已建成」矛盾 → **F81** |
| **F76**（CR3-F1 行引用已废弃标签） | ✅ 已解决 | 现为「本行原先引用的…标签**已于 2026-10-01 一并废弃**，现两处均为 `额外义务（CR3-F1 · uncertain 终态）`」 |
| **`verification.md` 结构完整性** | ✅ 结构完好，有 1 处残留损坏 | 397 行、19 个二级章节齐全无缺失无重复、无尾部截断；表列数自洽（Worktree Handoff 8 列×15 行、Handoff Index 10 列、Dispatch Reconciliation 5 列×8 行、Review Findings 8 列、Planning Findings 8 列）；但 `## Runtime Resources` 段尾残留 5 行无表头旧表行 → **F78** |

## 四、Findings

| ID | Severity | Location | Impact | Recommendation |
| --- | --- | --- | --- | --- |
| **DR1-F78** | MINOR | `verification.md` 的 `## Runtime Resources` 段尾、`## Worktree Handoff` 标题之前（`:187-191`） | 5 行无表头旧表行（WP5/2、WP6/2、WP6/3、TP2/2、TP2/3），列语义与新表不同（`Evidence` 填执行者报告而非 provisioner 报告、`Received At` 填该轮**结束/评审**时刻）。其中 WP5/2（06:54:19Z > 认领 06:34:58Z）与 WP6/3（08:21:00Z > 认领 08:17:32Z）**若进交接表会直接触发门禁失败**；`TP2/2` 行记 `95051f9` 与新表同键行的 `81e350f` **自相矛盾** | 门禁惰性（`tableRows` 遇下一标题即 break，无检查读 `Runtime Resources`）⇒ 不阻塞；但会让审计读到两份矛盾且部分违反判据的交接记录 | 删除这 5 行（信息已在 `## Worktree Handoff` 与 `## Dependency Handoffs` 以正确形态存在），或降级为表外散文 |
| **DR1-F79** | MINOR | `verification.md:198`（WP6/1）、`:208`（TP2/1）、`:209`（TP2/2）的 `Baseline Revision` | 三行均记重定向**之前**的 `81e350f…`。但 `provisioner-repoint-wp6-and-tp2.md` 的重定向（`07:09:53Z`）早于 WP6 第 1 轮认领（`07:23:26Z`）；`provisioner-repoint-tp2.md` 的 reflog 重定向（`08:55:27Z`）早于 TP2 第 1 轮（`09:03:14Z`）与第 2 轮（`09:33:15Z`）⇒ 这三轮实际都在**重定向后**的基线上开工 | 门禁只校验基线存在与 Received At，故不阻塞；但记录**低报**了依赖起点，若被 final/validator 当作「依赖起点证据」读取会误读成 WP6/TP2 在缺少上游交付提交的基线上开工。方向偏保守（把合规记成可疑），不会伪造通过 | 三行改为 `5ab7e9d…`（WP6/1）与 `95051f9…`（TP2/1、TP2/2），`Evidence` 相应改指 `provisioner-repoint-wp6-and-tp2.md` / `provisioner-repoint-tp2.md`。**注意**：改 `Evidence` 会触发 `:677-678`（Evidence 必须等于该 Provisioner 登记的报告路径），故须先在 Handoff Index 为 P3/P4 增加 attempt 维度登记 |
| **DR1-F80** | MINOR | `verification.md:43`（`## Handoff Index` TP1 行的 `Executor / Agent`） | 单元格文本含 `**修复轮认领者 tester-A2**`，而 `caseIds`（`:31`）按 `[,，、\s]+` 切分且**不剥 markdown 强调**，切出的是 `tester-A2**（台账`，**没有裸 `tester-A2`**；全文再无第二处。`checkReconciliation`（`:964`）以 `knownExecutors` 白名单核对 Dispatch Reconciliation 的 Executor | **premerge 会 FAIL**：`inspectPremerge`（`:1362-1370`）以 `stage:'premerge'` 调用 `checkReconciliation` ⇒ 「工作包 TP1 的执行者 tester-A2 未在 Handoff Index 中登记」。`--stage plan` 不调用该检查（`:414` 的守卫）故本轮看不到 | 让 `tester-A2` 成为独立 token：去掉包裹它的 `**`，例 `tester-A, run …（worktree wt/tp1）；修复轮认领者 tester-A2（台账 attempt 2，--reopen 打回 CR7 的 2×MAJOR）`。**零摘要成本** |
| **DR1-F81** | MINOR | `verification.md:179` vs `:161`（均在 `## Dependency Handoffs`） | `:161` 写「集成基线已建成（b0a387b…）」，`:179` 仍写「目前无下游接入（W1 尚未合入集成基线）」；实际 WP1–WP6+TP2 全部已并入候选 `2ed142d` | 不被门禁读取；但会让后续 reviewer/validator 读到自相矛盾的执行历史 | 把 `:179` 改为带时点的历史句 |
| **DR1-F82** | SUGGESTION | `plan.md:323-330` 与 `:416` | 新占位符不含**分支名**，该映射只存在于 provisioner 报告；`plan.md:416` 仍保留 `wt/u1` 旧符号风格 | 无门禁影响；仅可读性/风格不一致 | 可选：在占位符里补分支名；`plan.md:416` 一并统一。**改 plan.md 会变更 contractDigest**，建议与下次必须触碰 plan.md 的回写（F62/F63/F64 挂起项）同批 |

## 五、待补证据（不改变本轮判断）

| 待补项 | 状态 | 门禁 |
| --- | --- | --- |
| `git rev-parse --verify <5 个基线 SHA>^{commit}` | 未实测（无 shell），5 个 SHA 均有报告实测输出作静态佐证 | `--stage premerge` 自带核对 |
| `--stage premerge` 实跑（F80 是否真触发） | 未实测；`caseIds` 与 `:964` 的判定链已逐行核实为必然触发 | 6.6 合入前 |
| `contractDigest` 绑定 DR1 第 18 行 | 待主 Agent 写入（本轮唯一门禁 error） | 写入后 `--stage plan` 应转 PASS |

**残余风险（非本轮引入）**：① F74（Coverage Index 31 行 + plan 两处 Evidence 指向不存在的 `reports/PV1.log`/`PV2.log`）仍**只阻塞 final/archive**；② F62/F63/F64 仍挂起（下次触碰 `plan.md` 时同批修）；③ `Handoff Index` 的 DELIVERY 行 `phase` 写作 `deliver`，而 `role-report.md` 的 coder 合法 phase 只有 `implement`/`fix` —— 无门禁校验，仅登记。

## 六、结论

**PASS**（0×CRITICAL / 0×MAJOR；4×MINOR `DR1-F78`–`DR1-F81` + 1×SUGGESTION `DR1-F82`），绑定**实测** `contractDigest = sha256:fed00eb68cb7ef3cfab595a49374a6bb434807d770151c9e8b2d19bea7bb96ba`，`requirementsDigest` 未变。

**是否阻塞 premerge 门禁：是**，但只剩一条：**DR1-F80** —— 去掉 `verification.md:43` 中 `tester-A2` 的加粗即可，零摘要成本。本轮 (A)(B) 两项回写**本身不阻塞**。F78/F79/F81 为记录层，不阻塞，建议在 6.6 前一次性清掉。

### handoff_index

- `task_id: "NOT_APPLICABLE（规划门禁）"` · `role: reviewer` · `phase: plan` · `round: 18` · `stage: plan` · `target_revision: "sha256:fed00eb68cb7ef3cfab595a49374a6bb434807d770151c9e8b2d19bea7bb96ba"` · `evidence_type: REVIEW` · `evidence_id: DR1` · `report_path: "openspec/changes/session-resume/reports/dr1-dependency-review-round18.md"` · `result: PASS` · `evidence_status: NEW`
- `applicability_basis`: 第 18 个全新独立实例，只读检视 plan.md 与 verification.md 的两项回写；contractDigest/requirementsDigest 由主 Agent 代跑 `workflow check --stage plan --json` 实测并回传原文；门禁判据经 workflow-check.mjs 与 dispatch.mjs 源码逐行自证；台账事实取自 dispatch-queue.jsonl 全文；未参与任何工作包实现。