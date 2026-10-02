# DR1 Round 20 — 最终摘要绑定与验收记录结构一致性核实报告（Review Type: plan）

> 持久化说明：本报告由**第 20 个全新独立 reviewer 实例**（只读工具集，无 shell、无写权限）以全文返回、由主 Agent 落盘至 `openspec/changes/session-resume/reports/dr1-dependency-review-round20.md`。本实例未参与前 19 轮、CR1–CR8/CR-C1/CR-PM/validator，也未参与任何工作包实现或合并。
> **`contractDigest` 由主 Agent 代实测**：`workflow check --stage plan --json` → `contractDigest = sha256:d720f7d6479058d77eca829619f68e4f45922c905d9fb2ebade10e995762091c`（与本轮更正后的派发值逐字一致）、`requirementsDigest = sha256:5394327093e57d05b49ea4b04db708fcb97160791ea6cca8cec5a833154408fa`（与 Round 14–19 相同 ⇒ **行为契约一字未动**）、`errors[]` 仅一条（DR1 表未绑定当前摘要，即本轮待登记动作）。
> **Target Revision 更正留痕**：派发书原给 `sha256:70b2421d…`，主 Agent 在 Round 20 运行中更正为 `sha256:d720f7d6…`。本报告**按 `d720f7d6…` 绑定结论**。该更正本身是门禁机制的正常表现（见 §二·A③）。

```yaml
task_id: "NOT_APPLICABLE（规划门禁，Round 20）"
round: 20
stage: plan
target_revision: "sha256:d720f7d6479058d77eca829619f68e4f45922c905d9fb2ebade10e995762091c"
result: PASS
issues: "0×CRITICAL / 0×MAJOR / 5×MINOR（DR1-F84…F88，全部为记录层；均不改产品行为）"
```

## 一、范围与隔离

- 只读边界遵守：未修改任何文件、未提交、未跑 `cargo`/`npm`；`watchdog_diff` 仅用于读取工作区 delta（结果为空——本轮相关改动均已提交，属工具限制，已在 §五如实登记）。
- 实测值（`contractDigest` / `requirementsDigest` / `--stage final` 的 `errors[]` / 相关提交的文件清单 / `refs/heads/main` 完整 SHA）全部由主 Agent 代跑并回传完整原文，本报告未凭任何自述采信。
- 读取的项目规则：`AGENTS.md` §4/§10、`openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md`。
- 门禁机制的判据来源：`node_modules/@dongfanglin/openspec-agentic/src/workflow-check.mjs` 与 `workflow-contract.mjs`（只读）。

## 二、（A）摘要从 `2adbf605…` → `d720f7d6…` 的归因

### A① 自 Round 19 之后的改动是否只涉及 `tasks.md`？——**部分成立，需按文件分别陈述**

主 Agent 代跑的 `git log 1693ab3..HEAD --name-only -- openspec/changes/session-resume/` 逐字结果：

| 提交 | 改动的文件 |
| --- | --- |
| `0f45b8d` docs(repo): 补记最终验收的环境阻塞与恢复执行步骤 | `reports/final-handoff-session-resume.md` |
| `feb0674` docs(repo): 台账结案至 merged 并对齐 final 门禁的行内证据格式 | `dispatch-queue.jsonl`、`tasks.md`、`verification.md` |
| `9715911` docs(repo): 记录 CR-PM 第三轮复核通过并订正证据绑定表述 | `reports/cr-pm-post-merge-review-round3.md`、`tasks.md`、`verification.md` |
| `07edf4f` | `reports/merge-u1-main.md` |
| `def680f` | `reports/merge-u1-main.md`、`verification.md` |
| `42b4d0e` | `verification.md` |

加上主 Agent 代跑的各契约文件最后修改提交：`plan.md` = `20c1623`；`design.md` / `proposal.md` / `specs/**` = `0d2be6d`；`tasks.md` = `feb0674`。

**判定：**
- **`proposal.md`、`design.md`、`plan.md`、`specs/**` 全部未被触碰**（最后修改提交都早于 Round 19）——**成立**，且有两条独立机械证据（git 提交归属 + `requirementsDigest` 未变）。
- **`tasks.md` 是唯一进入摘要的改动源**——**成立**。
- **但派发书的说法「只涉及 `tasks.md`」不完整**：`verification.md` 与 `dispatch-queue.jsonl` 也被改了（`42b4d0e`/`def680f`/`9715911`/`feb0674`）。这两者**都不在 `contractDigest` 覆盖范围内**，因此不影响摘要归因——但「只有 `tasks.md` 变」这句话按字面不成立，应按上表陈述。

### A② `requirementsDigest` 不变是否符合门禁机制？——**成立**

`workflow-contract.mjs:51-56`：`requirementsDigest` 的文件集是 `['proposal.md', ...specFiles(changeRoot)]`，其中 `specFiles`（`:17-28`）递归 `specs/` 下全部 `.md`。**不含 `design.md` / `plan.md` / `tasks.md`**。实测值 `sha256:53943270…` 与 Round 14–19逐字相同 ⇒ **`proposal.md` 与 5 份增量 spec字节级未变**。这正是该摘要的设计用途（注释原文：*行为契约摘要：只含需求来源…不含 design/plan/tasks 等调度与执行安排*）。行为契约未被改动，**符合门禁机制**。

### A③ `contractDigest` 变化是否属于「流程动作而非行为契约变更」？——**成立**

`workflow-contract.mjs:31-39`：
- 文件集 = `.openspec.yaml`、`proposal.md`、`design.md`、`plan.md`、`tasks.md` + `specs/**/*.md` + `agentic-coverage` 块引用的源文件；
- 对 `tasks.md` **只**做一次归一化：`text.replace(/^(\s*[-*]\s*)\[[ xX]\]/gm, '$1[ ]')`。

本次追加的完成说明是**任务行下方的嵌套项目符号**（如 `      - **2026-10-01 闭环（…）**：…`），其行首没有 `[ ]`/`[x]`，**不匹配该正则**，因此整段文本原样进入摘要。9.1 的勾选与撤回同理：复选框本身被归一化，**说明文字的改写不被归一化**。

**判定**：完成说明文本与 9.1 说明的改写必然改变 `contractDigest`，这是门禁机制的既定行为（`procedures/workflow-check.md:44-45` 明写「任务复选框进度不参与」「规范标题/任务描述/计划变化都会使旧契约失效」）。变化来自**记录/流程层**，与 A② 的 `requirementsDigest` 未变相互印证。**属于流程动作，不是行为契约变更。**

### A 小结

摘要两跳（`2adbf605…` → `70b2421d…` → `d720f7d6…`）可完整归因于 `tasks.md` 的完成说明文本与 9.1 的勾选/撤回改写；契约文件一字未动；`requirementsDigest` 全程未变。**无 CRITICAL / MAJOR。**

## 三、（B）最终验收记录的结构一致性（只读核对，不判内容真伪）

### B1 `## Checks` —— **PASS（附一处派发前提订正）**

- 候选期行与 final 行**并存**：`verification.md:151-152` 的 `C1 / final / 全变更`（证据 `reports/PV1.log`）、`C2 / final / 全变更`（证据 `reports/PV2.log`）与 `verification.md:161-162` 的候选期 `C1/C2 / final / 全变更`（证据 `reports/merge-u1-candidate-PV1-stage2-workspace-test.log` / `reports/merge-u1-candidate-PV2.log`，Result 标注「已被上方 final 行取代」）。
- **门禁实际只看 C1/C2 两行，且取首个匹配**：`checkAlternativeChecks`（`workflow-check.mjs:905-924`）的 token 来自 `plan.md` 的 `alternative_checks: [C1, C2]`（`plan.md:447`），用 `table.data.find(...)` 取**首个** `Check ID / Stage / Work Package` 含该 token 的行 →命中 `verification.md:151`（C1）与 `:152`（C2）。
- 这两行：Result 分别为 `PASS / exit 0（1074 passed / 0 failed / 2 ignored）`、`PASS / exit 0（schemas/… 逐道 exit 0）`——**以 `PASS` 开头、无加粗**，满足 `/^\s*PASS\b/i`；Evidence 为**裸路径** `reports/PV1.log` / `reports/PV2.log`，满足 `looksLikePath`，且**两文件在 `reports/` 下实测存在**（另 `plan.md:472` 的 Completion Criteria 也把这两个路径写为正式证据，措辞一致）。
- **派发前提订正（不是仓库缺陷，故不计入 findings）**：派发书称「final **四行**的 Result 格以 `PASS` 开头」，按文件内容这不成立——`提交信息合规 / final` 行（`verification.md:153`）的 Result 是 ` **PASS / exit 0（…）**`（**带加粗**），`Rust 辅助 / final` 行（`:154`）以 `exit 0 / exit 0（0 诊断）` 开头，**都不以 PASS 开头**。但按上述机制，这两行的 Check ID 不是 `alternative_checks` 的 token，**门禁不读取它们的 Result 格**，因此对 `--stage final` 无影响。此处仅订正前提，不报缺陷。

### B2 `## Dispatch Reconciliation` —— **PASS（附一处 MINOR，见 DR1-F86）**

逐字比对 `dispatch-queue.jsonl` 的归约结果（每 WP 的最后一条事件）与表内 8 行：

| 表内 | 台账最后事件 | 一致 |
| --- | --- | --- |
| WP1 / 1 / coder-A / merged | `{"wp":"WP1",…,"executor":"coder-A","attempt":1,"state":"merged"}`（10-01T16:06:14Z） | ✓ |
| WP2 / 1 / coder-B / merged | `coder-B` / 1 / `merged`（16:06:16Z） | ✓ |
| TP1 / 2 / tester-A2 / merged | `tester-A2` / 2 / `merged`（16:07:15Z） | ✓ |
| WP3 / 2 / coder-C / merged | `coder-C` / 2 / `merged`（16:06:18Z） | ✓ |
| WP4 / 1 / reviewer-D / merged | `reviewer-D` / 1 / `merged`（16:06:20Z） | ✓ |
| WP5 / 2 / reviewer-E2 / merged | `reviewer-E2` / 2 / `merged`（16:06:23Z） | ✓ |
| WP6 / 3 / reviewer-F2 / merged | `reviewer-F2` / 3 / `merged`（16:06:25Z） | ✓ |
| TP2 / 3 / reviewer-T3 / merged | `reviewer-T3` / 3 / `merged`（16:06:28Z） | ✓ |

- **8行的 `Attempt`/`Executor`/`State` 与台账归约逐字一致，`State` 全部为 `merged`**；`checkReconciliation`（`workflow-check.mjs:960-976`）的四项判据全部满足。
- **Evidence 全部为含斜杠的仓库相对路径**（`openspec/changes/session-resume/dispatch-queue.jsonl`），满足 `looksLikePath`（`:43` 要求含 `/` 或 `\`），且该文件实测存在。
- **DR1-F86**：表下方注记块（`verification.md` `## Dispatch Reconciliation` 末段）称「**TP1 保持 `fixing`**……**台账事实照实保留、不粉饰**」，与**同节表格的 `TP1 | 2 | tester-A2 | merged`**、以及台账 16:07:15Z 的 `merged` 事件**直接矛盾**。注记自陈其目的是陈述台账事实，却陈述了已失效的旧状态。

### B3 `## Merge History` —— **PASS**

`verification.md:364-367` 共 4 行（M1 / M1-后续 / M1-收尾 / M1-提交信息合规）：
- M1 记录实际合并提交 **`0d2be6d`**（`--no-ff`，父提交对 `81e350ff…` + 候选 `2ed142d…`，零冲突）与修正提交 `69f1ac1`；✓
- 后续修正提交逐条登记：`20c1623b…`、`ac5de2a6…`、`cae3dbd1…`、`ee51795…`；✓
- `checkMergeHistory`（`workflow-check.mjs:1087-1093`）要求的 `Target Ref` / `Merger` / `Candidate Commit` / `Merged Commit` 四列均已填且非空。**本轮未重判这些 SHA 的真实性**（属 Round 18/19 已 PASS 的内容真伪范围）。

### B4 `## Review Findings` —— **位置 PASS；登记完整性 FAIL（MINOR，见 DR1-F85）**

- **位置核对（派发书特别要求项）：PASS**。`## Review Findings` 的标题行在 `verification.md:243`，其后**紧接** `| ID | Work Package | … |` 表头（`:244`）与 `| --- | … |` 分隔行（`:245` 上下文确认），**两条 CR-PM 行在 `:246` 与 `:247`，确实位于表头与分隔行之后**，能被 `readTable` 正确解析为该表的前两行数据行。此前「插在表头之前」的缺陷未复发。
- **登记完整性：不符合派发书所述「CR-PM 三轮判定均已登记」**。`grep '^\| CR-PM'` 在整份 `verification.md` 中**只命中 2 行**：
  - `verification.md:246`（Round 1，run `9a86b747`，Recheck = 「Round 2 已复核闭环」）
  - `verification.md:247`（Round 2，run `6f9fa48c`，Recheck = 「**待 Round 3 复核**」）
  
  **Round 3（run `4ce133b6`，判 PASS）没有任何一行。** 进一步核实：`reports/cr-pm-post-merge-review-round3.md` 确实存在于 `reports/`（由 `9715911` 入库），但 `verification.md` 中**既无 `round3`、也无 `4ce133b6` 的任何引用**——即该报告**未被 `## Handoff Index`、`## Review Findings` 或 `## Merge History` 任何一表引用**。Round 3 的判定目前只出现在 `tasks.md` 6.8 的完成说明里。
- 门禁影响：`checkReviewFindings`（`workflow-check.mjs:439-473`）不读 `Recheck` 列、不要求逐轮登记，且 CR-PM 两行的 `Work Package = U1` 属已退役工作包（台账 `superseded`）→ `:458` 直接 `continue`。**因此不产生门禁失败**，属记录完整性缺陷。

### B5 `## Independent Validation` —— **字段已填 PASS；格式 FAIL（MINOR，见 DR1-F84）**

- 按派发书的字面判据「Result 与 Report Path 已填、报告文件存在」：**PASS**。`verification.md:383`：Task `7.1`、Target Revision `69f1ac1…`（完整 SHA）、Result 非空、Report Path = `reports/validation-session-resume.md`，该文件实测存在。
- **但该 Result 格在 `--stage final` / `--stage archive` 会触发两条新错误**（详见 DR1-F84）。

### B6 `tasks.md` —— **勾选状态 PASS；未结案说明部分不准确（MINOR，见 DR1-F87）**

- **39 个任务为 `[x]`、9.1 为 `[ ]`，逐行清点确认**：§1（1.1–1.5 = 5）+ §2（2.1–2.8 = 8）+ §3（3.1–3.9 = 9）+ §4（4.1–4.3 = 3）+ §5（5.1–5.2 = 2）+ §6（6.1–6.8 = 8）+ §7（7.1 = 1）+ §8（8.1–8.3 = 3）= **39**；§9 仅 9.1 保持 `- [ ]`。✓
- **9.1 的未结案说明**声明 final 门禁尚有 2 项：① DR1 最高轮次行未绑定当前契约摘要；② 缺 `agentic-assessment` 块。主 Agent 代跑的 `--stage final --json` **实测 `errors[]` 恰为这 2 条**（逐字：`"Dependency Declaration Review 的 DR1 Plan Revision 未绑定当前规划契约摘要（计划或契约变化后须重新审查并更新该行）"`、`"需要唯一的 agentic-assessment 代码块"`）——**该「2 项」的陈述对当前时点准确**。✓- **但同一段说明里内嵌的摘要值已过期**：写的是 `sha256:70b2421d…`，而实测当前摘要为 `sha256:d720f7d6…`（见 DR1-F87）。
- **派发前提订正（不是仓库缺陷）**：「39 个任务为 `[x]`（**每个带完成说明**）」按文件内容不成立——实际只有 **6.6、6.7、6.8、7.1、8.1、8.2** 六个任务带 `完成`/`完成（…）` 说明，其余 33 个是纯复选框 + 任务正文（1.5 的「完成条件」是任务本身的验收条件，不是完成说明）。这不影响任何门禁（复选框状态本就被归一化、不入摘要），仅订正前提。

### B7 `reports/final-handoff-session-resume.md` 的「当前状态」陈述 —— **3 处不准确（MINOR，见 DR1-F88）**

该文件是本轮新增记录，其 §2「当前准确状态」表与步骤模板中的以下陈述与实测不符：
1. `本地 HEAD = feb0674f226fa78ae935f72e943a02723a23d4fe` —— 实测 `refs/heads/main = 0f45b8d170d1af4c7621edcbdf025dacfefc28d2`。`feb0674` 是写入该文件时的 HEAD，其后主 Agent 又提交了 `0f45b8d`（该交接说明自身入库）。
2. `当前契约摘要：sha256:70b2421d…`（以及 §3 步骤 2 的 DCR 行模板、步骤 4 的 `agentic-assessment` 模板里**写死**的同一值）—— 实测为 `sha256:d720f7d6…`。按 `checkDependencyReview`（`workflow-check.mjs:566`）与 `:1272`，这两个模板若照抄会直接判失败。
3. §3 步骤 1 第 3 点「`tasks.md` 39 个 `[x]` **均带完成说明**」—— 见 B6 的清点结果。

其余陈述（`origin/main = 81e350f`、工作区 tracked 干净、39/40、`--stage plan` PASS、`--stage premerge` PASS + receipt 路径、`check-doc-links` exit 0、1074 passed、十道门禁逐道 exit 0、final 恰好 2 项）本轮未逐项复测，**不在本轮判定范围**，按 role-report 记为待补，不影响本轮结论。

## 四、Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| **DR1-F84** | MINOR | `verification.md:383`（`## Independent Validation` 7.1 行的 `Result` 格） | 该格内容为 `**PASS**（0×CRITICAL/0×MAJOR；…）`。判据在 `workflow-check.mjs:501`：`else if (result.toUpperCase() !== 'PASS') { if (required) fail('Independent Validation 的 … 不是 PASS'); continue; }` —— `pick` 只做 `.trim()`（`:96`），**不剥 markdown 加粗、不做前缀匹配**，因此 `**PASS**（…）` 精确不等于 `PASS`。`required = (stage === 'final' \|\| 'archive')`（`:480`）。**当前不触发的原因**：`workflow-check.mjs:1261` 的 `block(verificationText,'agentic-assessment')` 因块缺失先 `throw`（`workflow-contract.mjs:11`），第 1264 行的 `checkIndependentValidation` **根本没被执行** | 一旦按交接说明步骤 4 补上 `agentic-assessment` 块，`--stage final` 会**新增两条错误**（`Independent Validation 的 7.1 不是 PASS` + `计划中的独立验证任务 7.1 缺少 PASS 记录`），交接说明的「一次归零」不会发生。属**记录格式**问题，不改变 validator 的任何结论，也不影响产品行为 | 把 7.1 的 `Result` 格改为**以裸 `PASS` 开头**（与 `## Checks` 的 C1/C2 行同一处理），括注移入该行的 `Report Path` 或节下注记。**代价为零**：`verification.md` 不在 `contractDigest` 覆盖范围（`workflow-contract.mjs:32` 的文件集无 `verification.md`），因此不触发新一轮 DR1 | 待主 Agent 修正后由新实例复核 |
| **DR1-F85** | MINOR | `verification.md` `## Review Findings`（表在 `:243` 起）；CR-PM Round 1 行 `:246`、Round 2 行 `:247` | `grep '^\| CR-PM'` 全文只命中 2 行；**Round 3（run `4ce133b6`，PASS）无对应行**，Round 2 行 `Recheck` 仍写「**待 Round 3 复核**」。且 `verification.md` 中**完全没有** `cr-pm-post-merge-review-round3` 或 `4ce133b6` 的引用——`reports/cr-pm-post-merge-review-round3.md` 已由 `9715911` 入库且文件存在，但未被任何一表引用 | 门禁不读取 `Recheck` 列、不要求逐轮登记（`workflow-check.mjs:439-473`），故**不产生门禁失败**；但受门禁校验的权威表会长期显示一个「未复核」的合入后 review 项，与 `tasks.md` 6.8 记录的「Round 3 判 PASS、11 条 findings 全部真正闭环」**互相矛盾**，读者据表判断会得到相反结论 | 在 `## Review Findings` 追加 CR-PM Round 3 行（Round 3 = `PASS`，Location/Resolution/Recheck 指向 `reports/cr-pm-post-merge-review-round3.md`），并把 Round 2 行的 `Recheck` 改为「**Round 3 已复核闭环**」。同样**零摘要代价**（`verification.md` 不入摘要） | 待主 Agent 修正后由新实例复核 |
| **DR1-F86** | MINOR | `verification.md` `## Dispatch Reconciliation` 表下注记块（`> **TP1 保持 fixing**…`） | 注记称「TP1 保持 `fixing`」「**台账事实照实保留、不粉饰**」；但同节表格第 3行为 `TP1 | 2 | tester-A2 | merged`，且 `dispatch-queue.jsonl` 末条 TP1 事件（`2026-10-01T16:07:15.488Z`）为 `{"wp":"TP1","executor":"tester-A2","attempt":2,"state":"merged"}` | 注记自陈的用途就是陈述台账事实，却陈述了已被台账覆盖的旧状态；与同节表格**内部矛盾**。表格行本身正确、门禁判据（表 vs 台账）**完全一致**，故无门禁影响 | 把注记改为「TP1 已在 `2026-10-01T16:07:15Z` 结案为 `merged`（其可执行用例按用户决定并入 TP2）」，或直接删除该句并保留表格为唯一事实 |记录层修复，无需重开 DR1 轮次 |
| **DR1-F87** | MINOR | `tasks.md` 9.1 的未结案说明子行 | 说明写「① `## Dependency Declaration Review` 的 DR1 最高轮次行尚未绑定当前契约摘要 `sha256:70b2421d…`」。实测当前 `contractDigest = sha256:d720f7d6479058d77eca829619f68e4f45922c905d9fb2ebade10e995762091c` | 该说明是 9.1 的**完成条件依据**。若照它去登记 DR1 第 20 行并填 `70b2421d…`，`workflow-check.mjs:566` 的 `revision !== contractDigest` 会直接判失败，且**改 `tasks.md` 又会再次变更摘要**，形成「改一次摘要变一次」的循环。**该值是本轮运行中（撤回 9.1 勾选之后）就已经过期的** | 把 9.1 说明里的摘要值改为 `sha256:d720f7d6…`（或删去具体值、改写为「尚未绑定当前契约摘要（以 `--stage plan --json` 实测值为准）」）。**注意**：这一步会再次改变 `contractDigest`，因此**必须与本轮 DR1 第 20 行的登记、以及 DR1-F88 的模板修正在同一笔编辑里完成，并在之后再开一轮 DR1 复核**——否则第 20 行的绑定会当场失效 | 需与 DR1 第 20 行登记同批修正，并新开一轮复核 |
| **DR1-F88** | MINOR | `reports/final-handoff-session-resume.md` §2 状态表、§3 步骤 1 第 3 点、§3 步骤 2/步骤 4 模板 | ① 「本地 `HEAD` = `feb0674f…`」，实测 `refs/heads/main = 0f45b8d170d1af4c7621edcbdf025dacfefc28d2`（`feb0674` 是写该文件时的 HEAD，其后 `0f45b8d` 才把本文件入库）；② 「当前契约摘要：`sha256:70b2421d…`」，且步骤 2 的 DCR 行模板、步骤 4 的 `agentic-assessment` 模板**把该值写死**；③步骤 1 第 3 点「`tasks.md` 39 个 `[x]` **均带完成说明**」，实测只有 6.6/6.7/6.8/7.1/8.1/8.2六个带说明 | 该文件是留给接手者的执行书。三处陈述中，② 会**直接把执行者带向两次门禁失败**（`:566` 与 `:1272`）；① 会让 `target_commit` 取到已被后续提交取代的值。③ 不影响门禁但会误导对记录完整性的判断 | 把 ② 的三处写死值改为 `sha256:d720f7d6…`，并在文件顶部加一句「本文件的状态取自 `0f45b8d`；执行前请以 `git rev-parse refs/heads/main` 与 `--stage plan --json` 的实测值重新取值」；③ 改为「39 个 `[x]`（其中 6.6/6.7/6.8/7.1/8.1/8.2附完成说明）」。`reports/**` 不在摘要覆盖范围，改动零代价 | 记录层修复，无需重开 DR1 轮次 |

**未发现 CRITICAL / MAJOR。** 五条 finding 全部为记录层，不改变 `proposal.md` / `specs/**` 的任何行为要求，也不影响产品代码；其中 F84/F85/F86/F88 的修复**不触及 `contractDigest` 覆盖的文件**，代价为零；F87 会触及 `tasks.md`，须与 DR1 第 20 行登记同批处理。

## 五、待补证据与限制

| 项 | 状态 | 是否影响本轮判断 | 应在哪个门禁前补齐 |
| --- | --- | --- | --- |
| 自 Round 19 起的**提交级 diff** | 未取得（`watchdog_diff` 只给工作区 delta，本轮为空——相关改动均已提交，属工具限制，非仓库问题） | 否。已由主 Agent 代跑 `git log --name-only` 与各契约文件最后修改提交取得等效证据（§二·A①） | 无需补 |
| `verification.md` 之外的 `origin/main`、`check-doc-links`、`commitlint`、workspace 测试、十道门禁等状态陈述 | 本轮**未复测**（不在 (A)/(B) 范围，且只读边界禁止 `npm`/`cargo`） | 否 | 9.1 验收前 |
| DR1-F84 的**实测复现** | 静态定位（判据 + 文件内容）已完整；因缺 `agentic-assessment` 块时该检查不被执行，当前无法在不写块的前提下实测触发 | 否（机制级证据已足：`:501` 判据 + `:383` 格内容 + `:1261` 先抛错的执行顺序） | 主 Agent 补 `agentic-assessment` **之前**先改7.1 的 Result 格 |
| `## Merge History` 中各 SHA 的真实性、Coverage Index 37/37 等 | Round 18/19 已 PASS，本轮按指令**不重判** | 否 | — |

**残余风险**：① F84/F85/F87 若未在勾选 9.1 之前修完，`--stage final` 仍会红，且 F87 的修正会再变更一次 `contractDigest`、需再开一轮 DR1；② DR1 第 20 行的绑定值必须用 `d720f7d6…`，一旦此后 `tasks.md` 再被编辑（如 F87 的修正），该行立即失效——建议把 F87 的修正与第 20 行登记放在**同一次编辑之后**再开第 21 轮复核；③ 本轮未取得提交级 diff，属工具限制。

## 六、结论

**PASS**（0×CRITICAL / 0×MAJOR；5×MINOR `DR1-F84`…`DR1-F88`，全部为记录层）。

绑定**实测** `contractDigest = sha256:d720f7d6479058d77eca829619f68e4f45922c905d9fb2ebade10e995762091c`，`requirementsDigest = sha256:5394327093e57d05b49ea4b04db708fcb97160791ea6cca8cec5a833154408fa`（与 Round 14–19 相同 ⇒ 行为契约未变）。

**是否阻塞 9.1 的最终验收：否**——但**阻塞「照交接说明原样执行就能一次归零」这个预期**：F84、F85、F87、F88 四项须先处理，其中 F87 会再次变更 `contractDigest`，因此 DR1 第 20 行的绑定应留到 `tasks.md` 定稿之后再开一轮。

本结论仅覆盖 `Target Revision` 所绑定的规划契约摘要与 `verification.md` 的结构自洽性；**不表示最终验收（9.1）已通过，也不表示该变更可归档**。

```yaml
handoff_index:
  - task_id: "NOT_APPLICABLE（规划门禁，Round 20）"
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    round: 20
    stage: plan
    target_revision: "sha256:d720f7d6479058d77eca829619f68e4f45922c905d9fb2ebade10e995762091c"
    evidence_type: REVIEW
    evidence_id: DR1
    report_path: "openspec/changes/session-resume/reports/dr1-dependency-review-round20.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本轮范围刻意收窄为「绑定最终摘要 + 核实最终验收记录的结构一致性」：(A) 摘要自 2adbf605…→70b2421d…→d720f7d6… 的归因（主 Agent 代跑 git log --name-only + 各契约文件最后修改提交；requirementsDigest 由主 Agent 实测未变）；(B) verification.md 五处权威记录的内部自洽与引用可解析性（独立按 workflow-check.mjs/workflow-contract.mjs 判据逐条核对，含文件存在性实测）。contractDigest/requirementsDigest/--stage final errors[] 均由主 Agent 代跑并回传完整原文。"
    source_evidence: NOT_APPLICABLE
```