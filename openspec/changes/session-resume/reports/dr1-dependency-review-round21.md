# DR1 Round 21 — 最终摘要绑定、F84–F88 处置复核与 --stage final 源码级门禁推演（Review Type: plan）

## Shared Report

```yaml
task_id: "1.x / 9.1（规划门禁 · Dependency Declaration Review 线程）"
role: reviewer
phase: plan
agent_context: >
  reviewer 子 Agent（DR1 线程第 21 个独立实例），fresh context，未参与前 20 轮、
  未参与任何工作包实现、未参与 F84–F88 的修复。无 shell、无写权限；只读工具
  （CompatRead/CompatGrep/CompatFind/CompatWatchdogDiff8849ea + CompatContactSupervisorb5fbd3）。
target_revision: "sha256:6ee3e5d5e36075f4927fa822bef1c927757a3719f0f346e186857eb18da4dd29"
scope: >
  (A) 摘要自 d720f7d6… 变为 6ee3e5d5… 的归因与行为契约基线；
  (B) DR1-F84–F88 五条 MINOR 的实际处置；
  (C) 主 Agent 计划中的后续动作（勾选 9.1 + 完成说明、追加 DCR 第 21 行、写
      agentic-assessment、提交、跑 --stage final）对 workflow check --stage final
      的源码级门禁推演。
changes: NONE（本轮只读；未修改任何文件、未提交）
checks:
  - "workflow check --stage plan --json（由主 Agent 代跑，原文回传）：result=FAIL（1 项，DCR DR1 未绑定当前摘要）；contractDigest=6ee3e5d5…dd29（与 Target Revision 逐字一致）；requirementsDigest=53943270…8fa"
  - "git rev-parse refs/heads/main / HEAD / status --porcelain（由主 Agent 代跑，原文回传）：HEAD == refs/heads/main == 0f45b8d170d1af4c7621edcbdf025dacfefc28d2；工作区 3 个已改未提交文件"
issues: 4（0×CRITICAL / 0×MAJOR / 4×MINOR）
result: PASS
evidence_paths:
  - "openspec/changes/session-resume/reports/dr1-dependency-review-round21.md（本报告，待落盘）"
  - "node_modules/@dongfanglin/openspec-agentic/src/workflow-contract.mjs"
  - "node_modules/@dongfanglin/openspec-agentic/src/workflow-check.mjs"
  - "node_modules/@dongfanglin/openspec-agentic/src/e2e-run.mjs"
  - "openspec/changes/session-resume/{tasks.md,verification.md,dispatch-queue.jsonl}"
  - "openspec/changes/session-resume/reports/{premerge-receipt-u1.md,final-handoff-session-resume.md}"
resource_cleanup: NOT_APPLICABLE（无资源分配）
```

---

## Review Context

| 项 | 实际取值 |
| --- | --- |
| Review ID / Round | `DR1` / **21**（线程内第 21 个独立实例） |
| Review Type | `plan` |
| Review Stage | 计划门禁 / 依赖声明收尾（`--stage final` 前的最后一次绑定） |
| Base Revision | `NOT_APPLICABLE`（按 `roles/reviewer.md` 规划审查约定） |
| Target Revision | `sha256:6ee3e5d5e36075f4927fa822bef1c927757a3719f0f346e186857eb18da4dd29` |
| 实测复核 | **一致**。主 Agent 代跑 `--stage plan --json` 返回 `contractDigest` 逐字等于上值；`requirementsDigest` = `sha256:5394327093e57d05b49ea4b04db708fcb97160791ea6cca8cec5a833154408fa`，与 Round 14–20 相同。**无需改绑，无差异声明。** |
| Repository | `D:/Project/acp-remote`（`refs/heads/main` == `HEAD` == `0f45b8d170d1af4c7621edcbdf025dacfefc28d2`，主 Agent 实测） |
| 读取的规则 | `AGENTS.md`（§8/§10/§11/§12）、`openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md`、`openspec/schemas/agentic/procedures/workflow-check.md` |
| 读取的契约 | `openspec/changes/session-resume/{proposal.md,design.md,plan.md,tasks.md,verification.md,dispatch-queue.jsonl,reports/premerge-receipt-u1.md,reports/final-handoff-session-resume.md}` |
| 门禁源码 | `@dongfanglin/openspec-agentic@` 已安装版本的 `src/workflow-contract.mjs`、`src/workflow-check.mjs`、`src/e2e-run.mjs`（`package.json` pin 版本，`node_modules` 实际落盘） |
| 实际检查范围 | 工作区未提交 delta（3 文件）+ `verification.md` 五处权威表 + `tasks.md` 40 项复选框结构 + 台账归约 + 收据字段 + 上述门禁源码逐条判据 |
| 使用的验证证据 | 主 Agent 代跑的两组只读命令 stdout 原文（已逐字回传并在上文引用）；**未使用任何 `verification.md` 自述作为结论依据** |
| 限制 | ① 本实例无 shell，全部摘要/祖先关系/门禁退出码均由主 Agent 代跑，我未亲跑；② `watchdog_diff` 只覆盖工作区 delta，**不覆盖已提交区间**——「自 Round 20 以来 `proposal.md`/`design.md`/`plan.md`/`specs/**` 未被触碰」这一条由「实测 requirementsDigest 未变」**间接**证明（该摘要的文件集恰为 `proposal.md` + `specs/**` + Coverage Index 引用源），但我**没有**逐提交核对历史；③ 未执行 E2E、未跑 `cargo`/`npm`。 |
| 未重判 | Round 1–20 已 PASS 的全部内容；CR1–CR8 / CR-C1 / CR-PM / validator 的既有结论 |

---

## (A) 摘要为何从 `d720f7d6…` 变为 `6ee3e5d5…`

**A-1「只涉及 `tasks.md`」— 结论：口径需精确化，但归因成立。**

工作区 delta 实际是 **3 个文件**（`CompatWatchdogDiff8849ea`，全仓库范围，无其它改动、无未跟踪文件）：

| 文件 | 改动 | 是否参与 `contractDigest` |
| --- | --- | --- |
| `openspec/changes/session-resume/tasks.md` | 1 行（9.1 未结案说明） | **是** |
| `openspec/changes/session-resume/verification.md` | 7 行（F84/F85/F86 三处） | 否 |
| `openspec/changes/session-resume/reports/final-handoff-session-resume.md` | 12 行（F88） | 否 |

判据来自 `src/workflow-contract.mjs:31-45`：

```js
export async function contractDigest(changeRoot) {
  const files = ['.openspec.yaml', 'proposal.md', 'design.md', 'plan.md', 'tasks.md'];
  files.push(...(await specFiles(changeRoot)).map(...));
  ...
  if (file === 'tasks.md') text = text.replace(/^(\s*[-*]\s*)\[[ xX]\]/gm, '$1[ ]');
```

即摘要的文件集 = `.openspec.yaml` + `proposal.md` + `design.md` + `plan.md` + `tasks.md` + `specs/**` + Coverage Index 的 `source.path` 引用源。**`verification.md` 与 `reports/**` 不在集合内**，因此 F84/F85/F86/F88 四处修复对摘要的贡献为 0。摘要变化**可 100% 归因于 `tasks.md` 那 1 行**。

> 附一条对第 (C) 项至关重要的同源事实：`tasks.md` 的**复选框标记被归一化为 `[ ]`**。因此**勾选 9.1 本身不改变摘要**，而**新增/改写 9.1 的完成说明文本会改变摘要**。本轮实测（`6ee3e5d5…`）正是 F87 那次「文本改写」的结果，与该判据完全吻合。

**A-2 `requirementsDigest` 未变 — 成立（实测）。**

主 Agent 代跑返回 `sha256:5394327093e57d05b49ea4b04db708fcb97160791ea6cca8cec5a833154408fa`，与 Round 14–20 逐字相同。该摘要的文件集（`workflow-contract.mjs:51-72`）= `proposal.md` + `specs/**` + Coverage Index 引用源，**不含 `design.md`/`plan.md`/`tasks.md`/`verification.md`**。⇒ **行为契约未变**，Round 21 的对照基线成立。

**A-3 其余契约文件未被触碰 — 成立（间接证明 + 直接证据）。**

- 直接：全仓库工作区 delta 仅上述 3 文件，`proposal.md`/`design.md`/`plan.md`/`specs/**` 零改动。
- 间接：`requirementsDigest` 未变 ⇒ `proposal.md` + `specs/**` + 引用源逐字节未变（摘要碰撞可忽略）。
- 未能直接证明的一格：`design.md` 与 `plan.md` 的**已提交区间**历史。`watchdog_diff` 不覆盖提交范围，我不宣称逐提交核对过；但它们**不在** `requirementsDigest` 内，且本轮工作区未触碰，故对本轮 Target Revision 的绑定无影响。

---

## (B) Round 20 五条 MINOR 的实际处置（逐条实读当前文件）

| ID | 判据出处 | 实读结果 | 判定 |
| --- | --- | --- | --- |
| **DR1-F84** | `checkIndependentValidation`，`workflow-check.mjs:501`：`else if (result.toUpperCase() !== 'PASS')`（**整格精确比较，不剥 markdown 加粗**） | `verification.md:384` 的 7.1 行 `Result` 格现为 `PASS（0×CRITICAL/0×MAJOR；…）`，**以裸 `PASS` 开头**；原文 `**PASS**（…）` 的加粗已去除 | **已正确处置** |
| **DR1-F85** | `checkReviewFindings` → `readTable(verification,'Review Findings')`；`tableRows`（`workflow-check.mjs:65-78`）取标题下第一张表、`rows[0]` 为表头、分隔行被 `isSeparator` 过滤 | ① CR-PM Round 3 行已补入（run `4ce133b6`、`PASS`、指向 `reports/cr-pm-post-merge-review-round3.md`）；② Round 2 行 `Recheck` 已由「待 Round 3 复核」改为「**Round 3 已复核闭环**」；③ **位置正确**——该行位于 `verification.md:243` 表头行与 `:244` 分隔行**之后**、Round 2 行之后，未复现「插在表头之前」的缺陷 | **已正确处置** |
| **DR1-F86** | 台账事实 | `verification.md:237` 注记已由「TP1 保持 `fixing`」改为「**TP1 已结案为 `merged`**（台账 2026-10-01T16:07:15Z，attempt 2，认领执行者 `tester-A2`）」；与 `dispatch-queue.jsonl:46` 的终态记录 `{wp:"TP1", attempt:2, executor:"tester-A2", state:"merged", at:"2026-10-01T16:07:15.488Z"}` 逐字一致 | **已正确处置** |
| **DR1-F87** | `workflow-contract.mjs:38` | `tasks.md:86` 的 9.1 未结案说明已改为「尚未绑定当前契约摘要（**以 `--stage plan --json` 的实测值为准**，勿写死具体值）」，不再出现 `sha256:70b2421d…` 之类字面值 | **已正确处置** |
| **DR1-F88** | 记录准确性 | ① 顶部已加重新取值警示（要求用 `git rev-parse refs/heads/main` 与 `--stage plan --json` 取值，「不要沿用本文件里的任何字面值」）；② `HEAD` 行改为「以 `git rev-parse refs/heads/main` 实测为准」；③ 摘要字面值改为占位符（正文「当前契约摘要」、步骤 1 的 Target Revision、步骤 2 的示例行、步骤 4 的 `contract_digest`） | **部分处置**——见 DR1-F90 / DR1-F91 / DR1-F92 |

**小结：F84 / F85 / F86 / F87 四条完全闭环且经源码判据复核为「门禁安全」；F88 的三类字面值已订正，但同一文件仍有三处陈述在本轮实测下为假（详见 findings）。**

> F85 的门禁安全性补充核验：新行 `Work Package = U1`，而 `U1` **不是** `plan.md` 的 Work Package。`checkReviewFindings`（`workflow-check.mjs:453-456`）对不在计划中的 WP 只在 `retiredWorkPackages(changeRoot)` 命中时放行；该集合来自 `dispatch-queue.jsonl` 中终态为 `superseded` 的记录，而 `dispatch-queue.jsonl:36` 正是 `{"wp":"U1", …, "state":"superseded"}`。⇒ 新行**不会**触发「引用了不存在或未退役的工作包」。F85 的修复在门禁上是安全的。

---

## (C) `--stage final` 的源码级门禁推演（逐项 + 建议顺序）

### C-1 摘要口径（决定「什么改动会让摘要再变一次」）

`workflow-contract.mjs:31-45`：文件集固定为 `.openspec.yaml`/`proposal.md`/`design.md`/`plan.md`/`tasks.md`/`specs/**`/Coverage Index 引用源；`tasks.md` 的 `[ ]`/`[x]`/`[X]` 一律归一化为 `[ ]`；`verification.md` 与 `reports/**` **不参与**。

⇒ **勾选 9.1 = 摘要不变**；**写完成说明 = 摘要变**；**追加 DCR 行 / 写 assessment 块 = 摘要不变**。

### C-2 主 Agent 五步动作逐项推演

| # | 动作 | 会触发的门禁判据 | 结论 |
| --- | --- | --- | --- |
| ① | 勾选 9.1 + 写完成说明 | `checkDependencyReview` 的 `revision !== contractDigest`；`inspectWorkflow` 的 `assessment.contract_digest !== result.contractDigest` | **摘要必变**（文本部分）。这是已知两条的根因，但真正的坑是**取值时机**，见 E1/E2 |
| ② | 追加 DCR 第 21 行 | 同上 | 摘要不变（`verification.md` 不参与）。取值必须晚于 ① |
| ③ | 写 `agentic-assessment` 块 | `block()` 要求**恰好一个**围栏块（`workflow-contract.mjs:9-14`，`matches.length !== 1` 直接抛错）；`assessment.result !== 'PASS'`；`nonempty(assessment_id)`；`assessment.evidence` 逐条重算 sha256 | 摘要不变。三个新约束见 E5/E6 |
| ④ | 提交 | `head !== target`；`assessment.target_commit !== target`；`evaluateRecordFreshness` | **本轮最危险的一步**，见 E3 |
| ⑤ | 跑 `--stage final` | 汇总 | 见下方「建议顺序」 |

### C-3 除已知两条外，逐项列出可能被触发的新错误

> 引用格式：`functionName`（已用 `CompatGrep8849ea` 核对函数起始行号）+ 判据原文。

**E1 — DCR 绑定值早于 9.1 定稿（MINOR 级操作陷阱，机制与已知 ① 相同但触发点不同）**
`checkDependencyReview`（起始 `workflow-check.mjs:512`）按 Review ID 分组、**取最大 Round** 的那一行，判据是 `if (!nonempty(revision) || revision !== contractDigest) fail(...)`。当前 `contractDigest` 为 `6ee3e5d5…`；一旦 ① 写入完成说明，摘要即变为新值。若第 21 行沿用 `6ee3e5d5…`，门禁会**原样重报**同一条错误。
**规避**：第 21 行的 Plan Revision 必须在 ① **完成之后**重新实测。

**E2 — `assessment.contract_digest` 同样必须晚于 ①（与 E1 同源，一次取值两处用）**
`inspectWorkflow` 的 assessment 判据：`if (assessment.contract_digest !== result.contractDigest) fail('验收结论的 contract_digest 已失效')`。同一实测值同时供 DCR 行与 assessment 块使用，两者不得分别取值。

**E3 — `assessment.target_commit` 的自指不动点（本轮最有价值的一条，已知清单里没有）**
`inspectWorkflow` 的目标判据是**两条严格相等**：

```js
const target = git(projectRoot, ['rev-parse', '--verify', `${coverage.target_ref}^{commit}`]);
const head   = git(projectRoot, ['rev-parse', 'HEAD']);
if (head !== target) fail('当前代码 HEAD 与计划目标引用不一致；请在目标版本 worktree 检查');
if (assessment.target_commit !== target) fail('验收结论的 target_commit 已失效');
```

`assessment` 块本身在 `verification.md` 内，而 `verification.md` 一旦被提交就会**推动 `refs/heads/main` 前移**。于是：提交前写的 `target_commit` 永远等于提交前的 SHA；提交后 `target_commit` 立刻失效。**把 ③ 和 ④ 按 final-handoff 步骤 4→5 的顺序做（先写块、再提交），`--stage final` 必定报 `验收结论的 target_commit 已失效`。**

可行解（有仓库先例）：**先把要入库的内容提交掉（得到 `T`），再写 `assessment.target_commit: T` 并把这一次编辑留在工作区不提交**。`evaluateRecordFreshness` 调用的 `uncommittedOutside`（`e2e-run.mjs:254-276`）对**变更目录内**的未提交改动显式豁免：

```js
const inside = path.relative(changeAbs, abs);
if (inside === '' || (!inside.startsWith('..') && !path.isAbsolute(inside))) continue;
```

且此时 `head === record.commit === T`，直接判 `fresh`。仓库先例：`openspec/changes/archive/2026-09-23-storage-ddl-constraint-coverage/verification.md:126` 明确记载了「本记录保留为变更目录内的未提交内容」这一约定。

> 注意收敛性：**`target_commit` 的最后一次修改永远不能再被提交**，否则它立刻又过期。要么接受「块落后一个提交」并显式记录理由（先例做法），要么接受「块留在工作区」。

**E4 — 9.1 完成说明里若出现复选框行，会打爆任务解析器**
`inspectWorkflow` 的任务解析：

```js
const taskRows      = [...tasks.matchAll(/^\s*[-*]\s*\[([ xX])\]\s+(\d+\.\d+)\s+(.+)$/gm)];
const allCheckboxes = [...tasks.matchAll(/^\s*[-*]\s*\[[ xX]\].*$/gm)];
if (taskRows.length !== allCheckboxes.length || !taskRows.length) fail('每项任务须使用唯一的 X.Y 编号和非空描述');
```

`\s*` 允许缩进，所以**缩进不能规避**。若完成说明写成 `- [x] …` 或 `- [ ] …`，`allCheckboxes` 会变成 41 而 `taskRows` 仍是 40 ⇒ 直接报「每项任务须使用唯一的 X.Y 编号和非空描述」，并且 9.1 的 `result` 说明还会被当成第二个 `[final-verification]` 候选。
**规避**：完成说明一律用普通 `-` / `*` 项目符号且**不带方括号**（现有 8.1/8.2 的 `- 完成（证据绑定提交 …` 写法即安全范式）。

**E5 — `assessment.evidence` 必须覆盖 Coverage Index 的全部证据路径，且 sha256 要在块写定后才确定**
`inspectWorkflow`：

```js
for (const entry of result.evidence) {
  const saved = assessment.evidence.find((item) => item.path === entry.path);
  if (saved?.sha256 !== entry.sha256) fail(`证据缺失或摘要已改变：${entry.path}`);
}
for (const entry of assessment.evidence) {
  if (!nonempty(entry.path) || entry.sha256 !== digest(await fs.readFile(path.resolve(changeRoot, entry.path)))) fail('验收证据摘要不匹配：${entry.path}');
}
```

`result.evidence` 来自 `plan.md` 的 37 行 Coverage Index 的 `evidence` 字段。我逐行读取（`plan.md:136-316`）后去重，**只有 3 个路径**：

| 路径 | 主 Agent 实测 sha256 |
| --- | --- |
| `reports/merge-u1-candidate-PV1-stage2-workspace-test.log` | `sha256:a93f928e74d1e517b3185d22492d6f8b67ea65f0fd0dbe5931ee73ff99761f0a` |
| `reports/merge-u1-candidate-PV2.log` | `sha256:ea718e0d81b9d09dca30081cdfcb5fd9732e2a05f6bd6faf900d70dad0648f4c` |
| `reports/PV1.log` | `sha256:bd79f79ed04194e5fdd760860889b0731ce53b9cb94d328970dbc58ba82263b3` |

⇒ 三条必须**全部**出现在 `assessment.evidence`；任何**额外**列出的报告也会被逐条重算 sha256。**推论：第 21 轮报告必须在写 assessment 块之前定稿**（若列入 evidence），否则块一落笔就与磁盘字节不符。

**E6 — `agentic-assessment` 围栏块必须恰好一个**
`workflow-contract.mjs:9-14`：`if (matches.length !== 1) throw new Error('需要唯一的 ${name} 代码块')`。`final-handoff` 步骤 4 里带着一份完整模板，若连模板一起粘进 `verification.md`，`matches.length` 变 2 ⇒ 抛错 ⇒ `inspectWorkflow` 的 `catch` 把它记为 error。当前 `verification.md:411-413` 的 `## Final Assessment` 只有一句注释、无围栏块，基线干净。

**E7 — `--stage final` 的执行位置：final-handoff 步骤 3 指的 worktree 会失败**
`head !== target` 要求**在 `projectRoot` 里** `HEAD` 与 `refs/heads/main` 相等。`D:/Project/acp-remote-wt/session-resume-du1` 是按合并单元建立的**集成分支** worktree，其 `HEAD` 是该分支 tip，与已被后续提交推过的 `main` 不相等。主 Agent 实测：**`D:/Project/acp-remote` 自身 `HEAD == refs/heads/main == 0f45b8d170d1af4c7621edcbdf025dacfefc28d2`**。
⇒ 应当在 `D:/Project/acp-remote` 执行，并带 `--planning-root D:/Project/acp-remote`。

**E8 — DCR 的 `Result` 是「整格精确等于 PASS」，比 final-handoff 描述的更严**
`checkDependencyReview` 的判据是 `if (String(result).trim().toUpperCase() !== 'PASS') fail(...)`——**精确比较**，`PASS（0×CRITICAL/0×MAJOR；…）` 与 `**PASS**` 都会失败。final-handoff 步骤 2 括注「门禁按 `/^\s*PASS\b/` 匹配，加粗符号会导致判定失败」对 `## Independent Validation`（F84 那条）成立，但对 DCR 表的描述不准确——那里连括注都不允许。第 21 行必须写**裸 `PASS`**。

**E9 — `U1` 行不会被误判（正向确认，非错误）**
`checkReviewFindings` 对不在计划 Work Packages 中的 WP 只放行 `retired` 命中者；`retiredWorkPackages`（起始 `workflow-check.mjs:433`）取台账终态 `superseded` 的 WP，`dispatch-queue.jsonl:36` 命中。F85 新增的 CR-PM Round 3 行（`Work Package = U1`）因此**安全**。

**E10 — 历史 premerge 收据**不会**被摘要变动打断（正向确认，非错误）**
`checkPremergeHistory`（起始 `workflow-check.mjs:702`）在 final 阶段逐行读 receipt，判据是：

```js
if (nonempty(parsed.requirements_digest)) {
  if (nonempty(requirementsDigestValue) && parsed.requirements_digest !== requirementsDigestValue) fail('…行为契约已失效…');
} else if (nonempty(contractDigest) && parsed.contract_digest !== contractDigest) { fail('…缺少 requirements_digest，且全文契约摘要已变化…'); }
```

`reports/premerge-receipt-u1.md:16` 已写有 `requirements_digest: sha256:5394327…8fa`，且该值与实测一致 ⇒ 走的是**第一个分支**，`contract_digest`（收据里是 `2adbf605…`，早已过期）**不再参与判定**。
⇒ **这是「可以反复改 `tasks.md`」这一结论的最强支撑**：只要不动 `proposal.md`/`specs/**`，历史候选收据、Merge History、Worktree Handoff、Dispatch Reconciliation、Review Findings、独立验证、并发探针都不会被摘要变动重新打破——这些判据的输入分别是 `verification.md` 的表、`dispatch-queue.jsonl` 与 `plan.md`，**没有一个读 `tasks.md`**。

**E11 — `--stage final` 并不要求 9.1 已勾选（正向确认）**
`if (stage !== 'plan') { for (const row of taskRows) if (row[1] === ' ' && !(stage === 'final' && row[3].includes('[final-verification]'))) fail(...) }`——final 阶段**豁免** `[final-verification]` 行。所以 9.1 保持 `[ ]` 也能让 `--stage final` 归零；`--stage archive` 才要求全勾。

**E12 — E2E 不构成新错误（正向确认）**
`checkWorkflow` 对 stage ≠ plan/premerge 会调 `checkE2E`；本变更 `mode: not-applicable`，E2E 侧只核 mode/三字段/降级批准，不要求 `stage=final` 的执行记录（与 `workflow-check.md`「not-applicable 时最终门只核对 mode、三字段与降级批准」一致），且 `tasks.md` 的 8.3 `[e2e-owned]` 行已勾选。

### C-4 建议执行顺序（**除已知两条外**，按「摘要与 SHA 的取值时机」重排）

```
① 定稿 tasks.md 9.1 的「完成说明」文本，暂不勾选
   - 必须用不带方括号的普通项目符号（E4）
   - 这一步之后 contractDigest 才定格
② 落盘并冻结 reports/dr1-dependency-review-round21.md
   - 若打算列入 assessment.evidence，必须在这一步之后不再改它（E5）
③ 实测摘要：workflow check --stage plan --json  → 记为 D
   （此时 errors 应只剩「DCR 未绑定」1 条，可作为 D 正确的自检）
④ 在 verification.md 的 ## Dependency Declaration Review 追加第 21 行
   - Round = 21；Plan Revision = D；Result = 裸 PASS（E8）
   - Report Path 用变更目录相对路径 reports/dr1-dependency-review-round21.md（E7 说明）
   - 不要往 ## Review Findings 加 DR1 行：规划级发现在该表会因「工作包不存在」被判非法
⑤ 提交 tasks.md + verification.md(DCR 行) + 第 21 轮报告（含 F84–F88 的处置）  → 得到 T
⑥ 写唯一一个 agentic-assessment 块到 ## Final Assessment（E6）
   - target_commit: T            ← E3，这是本顺序的关键
   - contract_digest: D          ← 与 DCR 行同值（E1/E2）
   - result: PASS；assessment_id 非空
   - evidence: 三个 PV 日志（E5 表中的实测 sha256）+ 任何你额外列出的报告
   - 这一次编辑留在工作区不提交（E3 的豁免依据）
⑦ 在 D:/Project/acp-remote 执行 workflow check --stage final --planning-root D:/Project/acp-remote --json（E7）
⑧ exit 0 之后：勾选 9.1 为 [x] 并补写「final 门禁 exit 0」作为完成证据，另起一次纯记录提交 → T2
   - 勾选不改变摘要（C-1），所以 D 与 assessment.contract_digest 继续有效
   - 但 T2 会让 assessment.target_commit 落后一个提交：在 ## Final Assessment 里显式记录
     「target_commit 绑定 T（勾选 9.1 之前的最后一个提交），其后仅有记录层提交」——
     与 archive/2026-09-23-storage-ddl-constraint-coverage 的既有做法一致
   - 若一定要让 T2 也全绿：把 target_commit 改成 T2 再跑一次，随后这次修改不得再提交（E3 收敛性）
⑨ --stage archive 是独立的一次调用，须在 ⑧ 之后另跑
```

**一句话回答调度的问题：除已知的两条外，还会产生其它错误——最关键的一条是 `assessment.target_commit` 的自指不动点（若按 final-handoff 步骤 4→5 的「先写块再提交」顺序做，必定失败），其次是「完成说明里写了 `- [x]` 子项会打爆任务解析器」和「在 du1 worktree 上跑 final 会因 HEAD≠main 失败」。**

---

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| **DR1-F89** | MINOR | `openspec/changes/session-resume/reports/`（缺失文件）；`verification.md:74-95` 的 DCR 表 | `CompatFind8849ea`（`**/*round20*`，全仓库）与 `CompatLs8849ea`（reports 目录 190 项）均**无** `dr1-dependency-review-round20.md`；`CompatGrep8849ea` 全 `openspec/` 搜 `round20|Round 20|第 20 轮|d720f7d6` 只命中 `reports/final-handoff-session-resume.md` 的**派发指引**（第 17/37/44/48/56/59/64 行）。DCR 表最大 Round = **19**（`2adbf605…`），无 Round 20 行。 | Round 20 报出的 F84–F88 **没有可读的原始报告**，「Round 20 结论 → 本轮处置」这条链在仓库内不可复现。**不阻断 `--stage final`**：该门禁只按最大 Round 取行（`checkDependencyReview`），第 21 行入库后即为当前结论，且没有任何 verification 文本引用一个不存在的路径。定级参照仓库既有分寸：被引用而缺失 = MAJOR（CR-PM-F7），未被引用而缺失 = MINOR（DR1-F19）——本例属后者 | 二选一：①（推荐，成本最低）在本报告与 `## Planning Findings` 的 Recheck 列写明「F84–F88 由 Round 21 直接对当前文件复核闭环，Round 20 自身报告未落盘」；② 若 Round 20 的 run 日志仍在，按 DR1-F19 的既有做法**原样提取**落盘为 `reports/dr1-dependency-review-round20.md`，**之后**才可在 DCR 表补 Round 20 行——顺序不能反，否则门禁会报「报告不可读取」。**切勿**事后由主 Agent 代写一份 Round 20 报告（那正是 CR-PM-F1 纠正过的失败模式） | 不适用（新发现） |
| **DR1-F90** | MINOR | `reports/final-handoff-session-resume.md:53`（步骤 1 · 复核范围第 3 项） | 该行仍逐字写着「`tasks.md` 39 个 `[x]` 均带完成说明；9.1 未勾选且原因如实」。实读 `tasks.md`（`CompatGrep8849ea` 全文复选框 + `CompatRead8849ea` 1-50 行）后确认**至少 18 项 `[x]` 没有任何完成说明子项**，因为它们在文件中相邻出现：1.1(:7)/1.2(:8)/1.3(:9)/1.4(:10)、2.7(:28)、3.1(:34)/3.2(:35)/…/3.9(:42)、4.1(:46)/4.2(:47)/4.3(:48) | 交接件把一条**可被机械证伪**的陈述交给接手者/reviewer 当作复核前提；照此复核会得出与事实相反的记录结论。非阻断（`reports/**` 不参与任何门禁、也不进 `contractDigest`） | 改为事实表述，例如「`tasks.md` 39 项 `[x]`、9.1 未勾选；带完成说明的是 2.1–2.6、2.8、8.1、8.2 等，其余任务的完成证据见 `verification.md` 的 `## Checks` 与 `## Handoff Index`」，或直接删掉该子句只保留「9.1 未勾选且原因如实」 | 不适用（F88 的残留部分） |
| **DR1-F91** | MINOR | `reports/final-handoff-session-resume.md:25`（「当前准确状态」表 ·「工作区 tracked」行） | 该行仍写「**干净**（0 项改动）」。主 Agent 实测 `git status --porcelain` 返回 3 行 ` M openspec/changes/session-resume/{reports/final-handoff-session-resume.md,tasks.md,verification.md}` | 与 F90 同类：表头自称「当前准确状态」，而其中的工作区状态在读取时即为假。新加的顶部警示只点名了「目标提交」与「契约摘要」两项需重取，未覆盖工作区状态，接手者仍可能照抄 | 把该行一并纳入顶部警示的重取清单（`git status --porcelain`），或直接改为「以 `git status --porcelain` 实测为准」；两者取一即可 | 不适用（F88 的残留部分） |
| **DR1-F92** | MINOR | `reports/final-handoff-session-resume.md:41-43`（步骤 3 的 `cd` 目标）与 `:64`（步骤 2 的 `Report Path` 写法） | ① 步骤 3 让接手者 `cd D:/Project/acp-remote-wt/session-resume-du1` 跑 `--stage final`；但 `inspectWorkflow` 的 `if (head !== target) fail('当前代码 HEAD 与计划目标引用不一致…')` 要求 `projectRoot` 的 `HEAD` 等于 `refs/heads/main`，而 du1 是集成分支 worktree。主 Agent 实测：`D:/Project/acp-remote` 自身 `HEAD == refs/heads/main == 0f45b8d170d1af4c7621edcbdf025dacfefc28d2`。② 步骤 2 的示例行用**仓库相对**路径 `openspec/changes/session-resume/reports/dr1-dependency-review-round20.md`；`checkDependencyReview` 的可读性判据是 `existsAt([changeRoot, projectRoot], report)`，两个 base 分别是变更目录与项目根——变更目录相对拼接必然失败，只有当 `projectRoot` 恰为仓库根（或该文件已提交进所用 worktree）时才命中 | 按交接件原样执行会拿到一条与真实原因无关的 `当前代码 HEAD 与计划目标引用不一致`；若换到仓库根执行，又可能因 `projectRoot` 基准变化而让仓库相对的 `Report Path` 不可解析 | 把步骤 3 的 `cd` 目标改为 `D:/Project/acp-remote`（并保留 `--planning-root D:/Project/acp-remote`）；把 DCR 新行的 `Report Path` 改成**变更目录相对**的 `reports/dr1-dependency-review-round21.md`——该写法在两个 base 下都能解析。历史行沿用仓库相对写法不影响门禁（既有轮次已在两种基准下验证过），不必回改 | 不适用（F88 的残留部分） |

---

## Assessment

**本轮结论：PASS**（0×CRITICAL / 0×MAJOR；4×MINOR 全部为记录层，不阻断 `--stage final`，也不要求重开 DR1 轮次）。

| 检查 ID | 已核对证据 | 差异 | 是否影响本轮判断 |
| --- | --- | --- | --- |
| DR1-21-A（摘要归因） | `workflow-contract.mjs:31-45` 的文件集与复选框归一化；全仓库工作区 delta 仅 3 文件；主 Agent 实测 `contractDigest` = Target Revision | 调度表述「该改动只涉及 `tasks.md`」需精确化为「3 个文件被改，其中只有 `tasks.md` 参与摘要」——归因结论不变 | 否 |
| DR1-21-A2/A3（行为契约基线） | 主 Agent 实测 `requirementsDigest` = `53943270…8fa`，与 Round 14–20 逐字相同 | 无 | 否 |
| DR1-21-B（F84–F88） | 逐条实读 `verification.md:237/244-246/384`、`tasks.md:86`、`reports/final-handoff-*.md`；并按 `checkIndependentValidation:501`、`checkReviewFindings`、`checkReviewFindings` 的 `retired` 集合、`dispatch-queue.jsonl:36/46` 复核门禁安全性 | F84–F87 闭环；F88 部分闭环（F90/F91/F92） | 否（均非阻断） |
| DR1-21-C（final 门禁推演） | 逐条读 `inspectWorkflow` / `checkDependencyReview` / `checkPremergeHistory` / `checkReviewFindings` / `checkIndependentValidation` / `checkReconciliation` / `checkMergeHistory` / `checkAlternativeChecks` / `checkConcurrencyProbe` / `evaluateRecordFreshness`；并用主 Agent 实测的摘要、目标 SHA、工作区状态与 3 个证据 sha256 做交叉验证 | 见 E1–E12 | 否（本项是交付前的操作建议，不改变 PASS 判定） |

**待补/未执行证据（不影响本轮判断，但须在对应门禁前由主 Agent 补齐）**：
1. `--stage final` 的实际退出码与 `errors` 数组 —— **必须在 E3 的顺序落地后由主 Agent 实跑**；本轮无 shell，未执行。
2. `design.md` / `plan.md` 的**已提交区间**是否在 Round 20 之后被触碰 —— `watchdog_diff` 不覆盖提交范围，本轮只由「工作区未触碰」+「requirementsDigest 未变」间接支撑。若需要硬证据，请主 Agent 代跑 `git log --oneline --name-only <Round 20 时的 SHA>..HEAD -- openspec/changes/session-resume/{design.md,plan.md}`。
3. `--stage archive` —— 本轮不判；E11 已说明它对 9.1 的额外要求。

**复用声明**：本轮**未**复用 Round 1–20 的任何 PASS 作为结论依据；F84–F88 的闭环判定全部来自对当前文件与门禁源码的直接复核。Round 20 自身报告缺失已记为 DR1-F89。

**结论适用范围**：仅对 Target Revision `sha256:6ee3e5d5e36075f4927fa822bef1c927757a3719f0f346e186857eb18da4dd29`（主 Agent 实测一致）与 `D:/Project/acp-remote` 的当前工作区状态成立。`tasks.md` 再有任何文本改动即失效。本报告不构成对整个变更可归档的判定。

---

## 隔离方式与限制

- **隔离**：本实例为 DR1 线程第 21 个全新独立 reviewer 子 Agent，fresh context；未参与前 20 轮、未参与任何工作包实现、未参与 F84–F88 的修复，未继承任何实现对话。我只能陈述收到的输入与自身工具边界，不声称能自行证明宿主未注入其它上下文（实际 Agent ID 与隔离设置由调度者关联记录）。
- **只读边界**：未修改任何文件、未提交、未跑 `cargo` / `npm`；仅使用只读文件/搜索工具与一次向主 Agent 索取实测值的 `contact_supervisor`。
- **本报告尚未落盘**：本实例无写权限。请主 Agent 将上文**原样**写入 `openspec/changes/session-resume/reports/dr1-dependency-review-round21.md`。
- **未覆盖**：`watchdog_diff` 不检查已提交区间；本轮所有「未被触碰」的断言限于工作区 delta 与摘要侧证据；E2E 未执行；`--stage final` / `--stage archive` 未实跑。

```yaml
handoff_index:
  - task_id: "1.x"
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    round: 21
    stage: plan
    target_revision: "sha256:6ee3e5d5e36075f4927fa822bef1c927757a3719f0f346e186857eb18da4dd29"
    evidence_type: REVIEW
    evidence_id: "DR1"
    report_path: "openspec/changes/session-resume/reports/dr1-dependency-review-round21.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: >
      Review Type=plan，绑定当前实测 contractDigest（主 Agent 代跑 --stage plan --json 逐字一致），
      requirementsDigest=sha256:5394327…8fa 与 Round 14–20 相同。
      Scope 覆盖 F84–F88 处置复核与 --stage final 的源码级门禁推演。
    source_evidence: NOT_APPLICABLE
  - task_id: "9.1"
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    round: 21
    stage: plan
    target_revision: "sha256:6ee3e5d5e36075f4927fa822bef1c927757a3719f0f346e186857eb18da4dd29"
    evidence_type: REVIEW
    evidence_id: "DR1"
    report_path: "openspec/changes/session-resume/reports/dr1-dependency-review-round21.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: >
      C-4 的执行顺序是 9.1 收尾的前置条件：本报告只给出顺序与判据，未执行任何门禁，
      未勾选任务，未代写 assessment 块。
    source_evidence: NOT_APPLICABLE
```

---

## Review（结论摘要）

- **Correct**：`workflow-contract.mjs:31-45` 的摘要口径已实读确认——`tasks.md` 复选框被归一化（勾选 9.1 不改摘要）、`verification.md` 与 `reports/**` 不参与摘要；这同时解释了 A 的归因与 C 的取值时机。历史 premerge 收据已写有 `requirements_digest`（`reports/premerge-receipt-u1.md:16`）且与实测一致，因此**反复改 `tasks.md` 不会打断已合入单元的历史凭据**——这是收尾可以放心的关键事实。`U1` 在台账中为 `superseded`（`dispatch-queue.jsonl:36`），F85 新增的 `Work Package = U1` 行不会触发门禁误判。
- **Fixed**：本轮未修复任何项（只读边界）。
- **Finding**：4×MINOR — DR1-F89（Round 20 报告从未落盘、无 DCR 行）、DR1-F90（交接件「39 个 `[x]` 均带完成说明」可被证伪，至少 18 项无说明）、DR1-F91（交接件「工作区干净（0 项改动）」实测为 3 项已改未提交）、DR1-F92（交接件步骤 3 的 `cd` 指向集成分支 worktree 会触发 `HEAD≠main`；DCR `Report Path` 宜用变更目录相对写法）。均非阻断。
- **Merge verdict**：**OK**（0×CRITICAL / 0×MAJOR）。四项 MINOR 建议在同一次触碰 `reports/final-handoff-session-resume.md` 时一并订正，成本极低，不值得为此再压一轮。

---

**实测 contractDigest**：`sha256:6ee3e5d5e36075f4927fa822bef1c927757a3719f0f346e186857eb18da4dd29`（与给定 Target Revision 逐字一致）；`requirementsDigest`：`sha256:5394327093e57d05b49ea4b04db708fcb97160791ea6cca8cec5a833154408fa`（未变）。

**(A)** 摘要变化可 100% 归因于 `tasks.md` 那 1 行——工作区虽改了 3 个文件，但 `verification.md` 与 `reports/**` 按 `workflow-contract.mjs:32` 不在摘要文件集内；`requirementsDigest` 实测未变，证明 `proposal.md`/`specs/**`/引用源逐字节未动。

**(B)** F84/F85/F86/F87 **已完全且正确闭环**（并逐条按门禁源码复核为安全）；F88 的三类字面值已订正，但同文件尚有三处陈述为假 → 记 F90/F91/F92。

**(C)** **还有其它潜在错误**。最关键的一条是 `assessment.target_commit` 的**自指不动点**（`inspectWorkflow` 中 `assessment.target_commit !== target`，`target` 取自 `refs/heads/main`）——按 final-handoff「先写块再提交」的顺序做必定失败；解法是先提交拿到 `T`、再写 `target_commit: T` 并把该次编辑留在变更目录内不提交（`e2e-run.mjs:271-272` 对变更目录内改动豁免，仓库有先例）。其次三条：完成说明里出现 `- [ ]`/`- [x]` 子项会打爆 `taskRows.length !== allCheckboxes.length`；在 `wt/session-resume-du1` 上跑 final 会因 `HEAD != refs/heads/main` 失败；DCR 的 `Result` 是**整格精确等于 `PASS`**（比交接件描述更严）。**建议顺序**：定稿 9.1 说明文本（不勾选）→ 冻结报告 → 实测摘要 D → 追加 DCR 第 21 行（裸 `PASS`、变更目录相对 Report Path）→ 提交得 T → 写唯一 assessment 块（`target_commit: T`、`contract_digest: D`、三个实测证据 sha256）并**不提交** → 在 `D:/Project/acp-remote` 跑 `--stage final` → exit 0 后再勾选 9.1 并作纯记录提交。

**findings**：DR1-F89 / F90 / F91 / F92，共 **4×MINOR，0×CRITICAL，0×MAJOR**。

**报告路径**：`openspec/changes/session-resume/reports/dr1-dependency-review-round21.md`（本实例无写权限，请主 Agent 原样落盘）。

```acceptance-report
{
  "criteriaSatisfied": [
    {
      "id": "criterion-1",
      "status": "satisfied",
      "evidence": "Returned PASS with the measured contractDigest (6ee3e5d5…dd29, identical to the given Target Revision), per-item verdicts for (A) and (B), a source-derived (C) gate analysis naming E1–E12 with a concrete 9-step ordering, 4×MINOR findings, and explicit residual risks. Report returned in full because this instance has no write tool."
    }
  ],
  "changedFiles": [],
  "testsAddedOrUpdated": [],
  "commandsRun": [
    {
      "command": "npx --quiet --no-install openspec-agentic workflow check --change session-resume --stage plan --planning-root D:/Project/acp-remote --json",
      "result": "failed",
      "summary": "Run by the main Agent on this reviewer's behalf (no shell in the reviewer). result=FAIL with exactly 1 error (DR1 Plan Revision not bound to current digest); contractDigest=sha256:6ee3e5d5e36075f4927fa822bef1c927757a3719f0f346e186857eb18da4dd29 (matches Target Revision); requirementsDigest=sha256:5394327093e57d05b49ea4b04db708fcb97160791ea6cca8cec5a833154408fa (unchanged since Round 14)."
    },
    {
      "command": "git rev-parse refs/heads/main / git rev-parse HEAD / git status --porcelain",
      "result": "passed",
      "summary": "Run by the main Agent on this reviewer's behalf. HEAD == refs/heads/main == 0f45b8d170d1af4c7621edcbdf025dacfefc28d2; working tree has 3 modified-but-uncommitted files (final-handoff report, tasks.md, verification.md) = the F84–F88 dispositions."
    }
  ],
  "validationOutput": [
    "contractDigest = sha256:6ee3e5d5e36075f4927fa822bef1c927757a3719f0f346e186857eb18da4dd29 (identical to the given Target Revision; no rebinding needed)",
    "requirementsDigest = sha256:5394327093e57d05b49ea4b04db708fcb97160791ea6cca8cec5a833154408fa (identical to Rounds 14–20; behaviour contract unchanged)",
    "--stage plan errors = ['Dependency Declaration Review 的 DR1 Plan Revision 未绑定当前规划契约摘要（计划或契约变化后须重新审查并更新该行）'] (1 error)",
    "coverage-index evidence set (deduped from plan.md:136-316) = 3 paths with measured sha256: reports/merge-u1-candidate-PV1-stage2-workspace-test.log=a93f928e…, reports/merge-u1-candidate-PV2.log=ea718e0d…, reports/PV1.log=bd79f79e…"
  ],
  "residualRisks": [
    "watchdog_diff does not inspect committed ranges; the claim that design.md/plan.md were untouched since Round 20 is supported only indirectly (working-tree delta clean + requirementsDigest unchanged), not by a per-commit check. Main Agent can confirm with: git log --oneline --name-only <round20-sha>..HEAD -- openspec/changes/session-resume/design.md openspec/changes/session-resume/plan.md",
    "The recommended --stage final ordering (freeze tasks.md note -> measure digest -> append DCR row -> commit -> write assessment with target_commit = that commit, left uncommitted -> run gate) has NOT been executed or verified; it is derived from source criteria only.",
    "assessment.target_commit is a self-referential fixed point: any commit that changes refs/heads/main invalidates it, and the last such edit can never itself be committed. Convergence requires either accepting a one-commit-stale target_commit (repo precedent in openspec/changes/archive/2026-09-23-storage-ddl-constraint-coverage/verification.md) or leaving the final assessment uncommitted.",
    "DR1-F89: reports/dr1-dependency-review-round20.md does not exist and no Round 20 row exists in the Dependency Declaration Review table, so Round 20's F84–F88 findings have no persisted original report. Non-blocking (max-Round selection means Round 21 governs), but if the main Agent ever adds a Round 20 DCR row it must land the report first or the gate fails with '报告不可读取'.",
    "E2E, cargo and npm were not executed by this instance; --stage final and --stage archive remain unrun."
  ],
  "noStagedFiles": true,
  "diffSummary": "No changes by this reviewer. Inspected working-tree delta vs launch HEAD: 3 files (tasks.md 1 line, verification.md 7 lines, reports/final-handoff-session-resume.md 12 lines) — the uncommitted dispositions of DR1-F84 through DR1-F88.",
  "reviewFindings": [
    "no blockers",
    "minor: reports/dr1-dependency-review-round20.md — Round 20's report was never landed and no Round 20 row exists in the Dependency Declaration Review table (max Round = 19); F84–F88 have no persisted original report",
    "minor: reports/final-handoff-session-resume.md:53 — 'tasks.md 39 个 [x] 均带完成说明' is false; at least 18 checked tasks (1.1–1.4, 2.7, 3.1–3.9, 4.1–4.3) have no completion sub-bullet",
    "minor: reports/final-handoff-session-resume.md:25 — '工作区 tracked 干净（0 项改动）' is false; git status --porcelain shows 3 modified files",
    "minor: reports/final-handoff-session-resume.md:41-43 and :64 — step 3 points --stage final at the integration worktree wt/session-resume-du1, whose HEAD != refs/heads/main (would fail '当前代码 HEAD 与计划目标引用不一致'); DCR Report Path should be change-dir relative"
  ],
  "manualNotes": "Verdict PASS (0 CRITICAL / 0 MAJOR). (A) The digest change is fully attributable to tasks.md: workflow-contract.mjs:32 fixes the digest file set to .openspec.yaml/proposal.md/design.md/plan.md/tasks.md/specs/**/coverage sources, so the verification.md and reports/** edits contribute nothing. (B) F84/F85/F86/F87 fully and correctly closed; F85's new row uses Work Package 'U1', which is gate-safe only because dispatch-queue.jsonl:36 marks U1 superseded (retiredWorkPackages). F88 only partially closed (F90/F91/F92). (C) Beyond the two known errors there are more; the most important is the assessment.target_commit self-reference — see the 9-step ordering in section C-4. Also note: checking the 9.1 box does NOT change contractDigest (checkbox markers are normalised at workflow-contract.mjs:38), but writing the completion note text does. The report must be persisted verbatim at openspec/changes/session-resume/reports/dr1-dependency-review-round21.md."
}
```