# DR1 Round 25 — 订正复核（Review Type: `plan`）

> 持久化说明：本报告由**第 25 个全新独立 reviewer 实例**（只读工具集：read/grep/find/ls/watchdog_diff/contact_supervisor，**无 shell、无写权限**）以全文返回、由主 Agent 原样落盘。本实例未参与 Round 1–24、未参与任何工作包实现/合并/本轮记录订正。派发书中 main 的一切**自述**一律不作为证据；下文每条判定都给出「我亲自读到的文件位置 + 行号」或「main 代跑命令的**原文**」。凡涉及命令输出，我要求的是完整原文而非摘要（DR1-F107 的登记口径）。

## Review Context

| 项 | 实际取值 |
| --- | --- |
| Review ID / Round / Type | `DR1` / **25** / `plan`（`phase: plan`、`stage: plan`，`target_revision` = 当前 `contractDigest`） |
| Target Revision | `sha256:93356f963422dac3a2d95ad700449b914432c83d267e12d422bfdc7a71329ba2` — **实测逐字一致**（A1） |
| `requirementsDigest` | `sha256:5394327093e57d05b49ea4b04db708fcb97160791ea6cca8cec5a833154408fa` — **实测逐字一致，与 Round 14–24 相同**（A1） |
| Repository / 工作区 | `D:/Project/acp-remote`；`HEAD = 6fb558fc71ab78f3542b9e4fbd31dfb49c3f9a5a`（分支 `feat/session-resume`）、`refs/heads/main = 56de6bd9e7c934895f8ae2ea5c489ca9187052d9`（**两者不相等**，A2/B1 实测） |
| Base Revision | NOT_APPLICABLE（规划审查不针对代码 diff） |
| Requirements | `tasks.md:85–91`、`verification.md`（`## Dependency Declaration Review`、`## Check Plan Changes:128–129`、`## Independent Validation:416`、验收块 `443–445` 与说明区 `464–472`）、`reports/ci-pr37-checks-evidence.md`（全文 68 行）、`reports/dr1-dependency-review-round24.md`（全文）、`reports/dr1-dependency-review-round23.md`（F98/F99/F100 行）、`reports/final-gate-final-2026-10-02-c9ab2fc.json`、`reports/merge-u1-candidate.md:258–265`、`reports/tp2-tester.md:518–534`、`reports/validation-session-resume.md:275–282` |
| 项目规则 / 判据来源 | `AGENTS.md` §10/§11；`roles/reviewer.md`；`roles/_shared/role-report.md`；门禁源码 `node_modules/@dongfanglin/openspec-agentic/src/workflow-check.mjs`（`checkDependencyReview` 512–576；验收块判据 1259–1292）、`workflow-contract.mjs:31–46`（`contractDigest` 输入清单）、`e2e-run.mjs:254–308`（`evaluateRecordFreshness`/`uncommittedOutside`） |
| 实测来源 | main 代跑的**只读命令原文**（A1/A2/B1/B3/B6/C1），逐条与本机文件交叉核对；`gh run view 36973882474 --json event,headSha,conclusion` 原文 |
| 隔离方式 | 全新子 Agent（第 25 个独立实例），不继承实现对话；只读工具集，**无 shell、无写权限**；未执行 E2E；未跑 `cargo`/`npm`/门禁（未跑 `check-doc-links.mjs`、未跑 commitlint）；未修改任何文件 |
| 未重判 | Round 1–23 已 PASS 的内容；Round 24 已判为**成立**的 (A) 与 (B) 结论层（除本轮的订正复核） |

---

## (A) 摘要 `6badf05b…` → `93356f96…` 的归因 —— **判定：成立（三条全部与实测一致）**

**A1. 两个摘要实测**（main 代跑原文）

```
$ npx --quiet --no-install openspec-agentic workflow check --change session-resume --stage plan --planning-root D:/Project/acp-remote --json
EXIT=1
"contractDigest": "sha256:93356f963422dac3a2d95ad700449b914432c83d267e12d422bfdc7a71329ba2"
"requirementsDigest": "sha256:5394327093e57d05b49ea4b04db708fcb97160791ea6cca8cec5a833154408fa"
"errors": [ "Dependency Declaration Review 的 DR1 Plan Revision 未绑定当前规划契约摘要（计划或契约变化后须重新审查并更新该行）" ]
```

⇒ 与本轮给定 Target Revision **逐字一致**；**只有 1 条错误**，正是本轮 PASS 之后必须由第 25 行闭合的那条（与第 22–24 轮同款中间态）。`requirementsDigest` 与 Round 14–24 **逐字相同**。

**A2. ① 「本轮唯一参与摘要的改动就是 `tasks.md` 9.1 那一行」—— 成立**

- 判据口径我读过源码：`workflow-contract.mjs:31–46`，`contractDigest` 的输入是 `.openspec.yaml`、`proposal.md`、`design.md`、`plan.md`、`tasks.md`（**仅把 `^(\s*[-*]\s*)\[[ xX]\]` 归一化为 `[ ]`**）＋ `specs/**` 全部 `.md` ＋ `plan.md` 的 `agentic-coverage` 各行 `source.path`；**`verification.md` 与 `reports/**` 根本不入摘要**。故「记录层改动不改摘要」是机制事实。
- 实测 `git status --porcelain -uall`（原文）：

```
 M openspec/changes/session-resume/reports/ci-pr37-checks-evidence.md
 D openspec/changes/session-resume/reports/final-gate-final-2026-10-02.json
 M openspec/changes/session-resume/tasks.md
 M openspec/changes/session-resume/verification.md
?? openspec/changes/session-resume/reports/dr1-dependency-review-round24.md
?? openspec/changes/session-resume/reports/final-gate-final-2026-10-02-c9ab2fc.json
```

⇒ 工作区**没有任何** `proposal.md`/`design.md`/`plan.md`/`specs/**`/`.openspec.yaml` 的改动（含未跟踪）。**索引为空**（无暂存）。

- 实测 `git diff HEAD -- tasks.md`（原文摘 hunk 头）：`@@ -85,5 +85,6 @@`，**1 删 2 加、单一 hunk、全在 9.1 说明段**（旧的一行「未闭合项 ①…④」被拆成「曾列为未闭合项、已由 CI 实测证据关闭（…DR1-F101/F102 订正）」与「仍未闭合项…」两行），**无任何复选框取值变化**（且即便有也会被归一化）。
- 实测 `git log --oneline -5 --name-only`（原文）：`6fb558f` 只含 `reports/ci-pr37-checks-evidence.md` 与 `verification.md`；`39d4b9b` 只含 `reports/wp5-coder-fix-cr5f1.md` 与 `verification.md`；`56de6bd` 含 `tasks.md`（**本轮之前**的既有改动）与 `reports/**`+`verification.md` ⇒ 两次 HEAD 提交都**不涉任何摘要输入**。

**A3. ② 「`requirementsDigest` 未变 ⇒ 行为契约未动」—— 成立**（A1 实测；`workflow-contract.mjs:51–57` 的输入只含 `proposal.md` ＋ `specs/**`）。

**A4. ③ 「`proposal.md`/`design.md`/`plan.md`/`specs/**` 未被触碰」—— 成立**：A2 的工作区清单与提交区间清单双重覆盖。

⇒ **(A) 判定：通过**。摘要变化的**唯一成因**是 `tasks.md` 9.1 说明段的文字替换；行为契约未动。

---

## (B) F101–F106 逐条闭环核实 —— **判定：5 条已闭环、F101 只闭环 2/3 处（F108），另发现 F103① 的第二处口径问题（F109）**

> 方法：逐条**实读当前文件**（`read`/`grep` 全文搜两个用例名），不采信自述。

| ID | 上一轮要求 | 我实测到的当前状态 | 判定 |
| --- | --- | --- | --- |
| **F101** | `tasks.md:88`、`verification.md:127/128`、`reports/ci-pr37-checks-evidence.md:8–13` 三处改名 | ① `tasks.md:88`：**已改**（两条均写 `a_persisted_…`、`an_inaccessible_persisted_directory_is_refused_before_any_backend_call ... ok`）；② `verification.md:128`：**已改**，且 `store::unix_modes::…` 只出现在「DR1-F101 更正留痕」括注里（我逐字读了该整行）；③ `reports/ci-pr37-checks-evidence.md`：**只加了后文订正**——`:58–59` 给出正确两条、`:62` 留痕，但 **`## 一、` 下代码块（`:10–13`，错名字在 `:12`）仍是 `test store::unix_modes::private_directory_and_entry_file_modes_are_restrictive ... ok`** | **部分闭环** → **F108**（main 回复第 7 问自认「漏改，不是有意保留」） |
| **F102** | 去掉「另 7 条」数字、改指向证据文件 | `tasks.md:88` 现为「另有多条 unix/权限相关用例通过（`unix_endpoint_*` 家族、`store::unix_modes::…`、`transport::…unix…` 等，**完整名单以 `reports/ci-pr37-checks-evidence.md` 为准、不在此计数**）」 | **已闭环** ✅ |
| **F103①** | 触发方式改为逐运行为准 | `:3` 现为「`36973886861`（head `39d4b9b`，事件 **`pull_request`**）与 `36973882474`（head `56de6bd`，事件 `push`）」+ F103 订正注 | **事件口径已闭环**，但**第二个 `headSha` 与实测冲突** → **F109** |
| **F103②** | 节标题不得把 2 条非门控用例统称 `#[cfg(unix)]` | `:15` 改为「同批通过、与 unix/权限相关（或名称相关）的用例（节选）」并就地标明 `unix_mode_bits_map_to_the_expected_verdicts`（`storage-sqlite/tests/permissions.rs`）与 `params_keep_integer_fidelity`（`server/src/local_admin/envelope.rs`）按源码**不是** `#[cfg(unix)]` | **已闭环** ✅ |
| **F103③** | 删掉文件末尾残留 `SS` | 我完整读了该文件（68 行），末行为「…非本次 CI 可判定项，仍然开放。」，**无 `SS`** | **已闭环** ✅ |
| **F104** | 三处改「①② 已由 CI 关闭、③ 仍未闭合」 | ①`## Independent Validation:416` =「**① ② 已由 CI 实测证据关闭、③ 仍未闭合（DR1-F104 订正）**」；②验收块 `Unresolved:471` 同口径（①② 关闭、③④ 未闭合）；③`## Check Plan Changes:129` 已由「**未回改 `tasks.md`**」改为「本批**已回改** `tasks.md` 9.1 的说明（①② 改为已关闭，并按 DR1-F101/F102 订正用例引用与计数口径，③④ 保留）」；全文 grep `未回改` **只命中历史轮报告**，`verification.md` 内零残留 | **已闭环** ✅ |
| **F105** | 验收块说明行不再写死 `sha256` | `:464` 现为「`contractDigest` = **以 `--stage plan --json` 的实测值为准，不在此写死**（DR1-F98/F105：本行曾写死 `6ee3e5d5…`、订正后仍写死 `b706a9ca…`…）」；该行保留的**唯一** sha256 是 `requirementsDigest`（本轮实测 `53943270…8fa` **仍然准确**，且该值本身承担「行为契约未动」的举证，属正当保留） | **已闭环** ✅ |
| **F106** | 验收块说明区补「收尾顺序（DR1 Round 24 勘定）」整段 | `:468` 已逐字补入（含 `if (head !== target) fail('当前代码 HEAD 与计划目标引用不一致…')` 的引文与①②③④⑤五行顺序） | **已闭环** ✅（但顺序本身缺一环 → **F110**） |

**第 (5) 条另存核实 —— 与预期**不符**：
- `reports/final-gate-final-2026-10-02.json`：`read` 返回 **ENOENT**；main 的 `git status` 原文含 ` D openspec/changes/session-resume/reports/final-gate-final-2026-10-02.json`；`ls` 只列出 `…-c9ab2fc.json` 与 `…-fail.json`。main 回复确认是**改名**（不是复制），并明确「『当前绑定值对应的通过那次』至今尚未生成」。
- 现存 `-c9ab2fc.json` 内容：`result: PASS`、`targetCommit: c9ab2fc…`、`contractDigest: sha256:b706a9ca…`、`errors: []` ⇒ 与 Round 24 记录的「c9ab2fc 那次通过输出」**内容相符**；但它**未入库**（`??`）。
- 因此 `verification.md:465` 的「**三份产物均保留**：`…-2026-10-02.json`（当前绑定值对应的通过那次）…」与 `tasks.md:86` 的「原始输出见 `reports/final-gate-final-2026-10-02.json`」**当下为假**（且若照现状提交，`56de6bd` 已入库的那份产物会从版本控制中消失） → **F111**（DR1-F99 同类复发）。

⇒ **(B) 判定：结论层与引用层总体成立**；`F101` 的第三处未闭环（F108），`F103①` 剩一处 `headSha` 错配（F109），「另存」实为改名留下产物缺口（F111），收尾顺序缺一环（F110）。四项均为**记录层、非阻断**。

---

## (C) 收尾顺序与 DCR 绑定 —— 三个确认

### C-1 第 24 行的绑定**不是**不实陈述，且不违反上一轮 (C-1) 约束 —— **确认成立**

- 第 24 轮报告的 Target Revision 我逐字读过：`sha256:6badf05bf9b2910a92ef4b6b4aea27d5dcd0be6c3f4fb371124066b6a915ed76`，结论 **PASS**（`reports/dr1-dependency-review-round24.md` 的 Review Context 与 Assessment 段）⇒ 该行 `Plan Revision = 6badf05b…`、`Result = 裸 PASS` 是**对该轮真实审过版本的绑定**。
- 上一轮 (C-1) 的约束是「**不可**把第 24 行绑到**改动后**的新摘要」；实测其 `Plan Revision` 就是第 24 轮审过的旧摘要、`Result` 单元格是整格恰好 `PASS`、`Report Path = reports/dr1-dependency-review-round24.md` 且该文件可读 ⇒ **不违反**。
- 门禁侧旁证：`--stage plan` 实测**只有 1 条**错误，且是「Plan Revision 未绑定当前摘要」⇒ DCR 的其余判据（Reviewer 非 WP Owner、`Result` 整格 `PASS`、报告可读；同时所有 1–24 行都带 Round 且无重复）**全部通过**。即：第 24 轮「(甲) 绑旧摘要 + 随后单独改 `tasks.md` 并重开 Round 25」这条路被正确执行。

### C-2 门禁只取最大轮次行、第 24 行随之退出判据 —— **确认成立**

`workflow-check.mjs` 的 `checkDependencyReview`（512–576）我逐行读过：表按 **Review ID 分组** → 校验 `Round` 为 ≥1 整数且不重复（多条记录必须逐轮带 Round）→ `withRound.reduce((best, entry) => entry.round > best.round ? entry : best)` **取最大 Round 行**作为该 ID 的当前结论 → **只有被选中的那一行**参与四项判据（`Reviewer` 非 WP Owner、`Plan Revision === contractDigest`、`Result` 整格 `PASS`、`Report Path` 可读）。故第 25 行写入后，**第 24 行（`6badf05b…`）退出全部判据**，仅在「多条记录都必须带 Round」与「Round 不得重复」两条上被看到（两者当前均满足）。门禁输出「恰 1 条错误」是该机制在机器层的直接证实。

**追加第 25 行的 5 项前提（缺一即红，沿用 Round 24 C-1 并补第 5 项）**：① `Plan Revision` **逐字**等于写入时刻的实测摘要；② `Result` 整格 `trim().toUpperCase() === 'PASS'`（不能写 `PASS（…）`、加粗、前缀式）；③ `Reviewer` 不得等于 `plan.md` 的任何 WP Owner；④ `Report Path` 必须可解析且**文件在门禁运行时已存在**（先落盘本轮报告再写入该行）；⑤ **验收块的 `contract_digest` 必须同批更新为同一实测值**（`workflow-check.mjs:1272`；见 F110）。

### C-3 我认定的**最终收尾顺序**（含 `HEAD === refs/heads/main` 一环）

1. **零摘要代价的记录修复批**（对应 F108/F109/F110/F111，全部落在 `verification.md` 与 `reports/**`）：改 `ci-pr37-checks-evidence.md:12`；改 `:3` 的 `headSha` 配对；把 `verification.md:468` 第 ③ 步补上「并同批把本块 `contract_digest` 更新为该轮实测值」；把 `-c9ab2fc.json` 复制回主名（或 `git checkout --` 恢复）并 `git add` 副本，同时按「绑定哪一次运行」重写 `:465` 的括注。**此步绝不可触碰 `tasks.md`/`plan.md`/`design.md`/`proposal.md`/`specs/**`/`.openspec.yaml`。**
2. 重新实测 `--stage plan --json`（预期仍 `93356f96…`）；先落盘本轮报告 `reports/dr1-dependency-review-round25.md`，再追加 DCR 第 25 行（`DR1 | 25 | <新独立实例，非任何 WP Owner> | <实测值> | PASS | reports/dr1-dependency-review-round25.md`），并把验收块 `contract_digest` 改为同一值。
3. 提交这一整批记录（含 `reports/**`），走 PR 合入，使 `refs/heads/main` 前进到 M。
4. **在 `git rev-parse HEAD` == `git rev-parse --verify refs/heads/main^{commit}` == M 的工作区**操作：把验收块 `target_commit` 改为 M（**该次编辑留在工作区不提交**——它在变更目录内，`e2e-run.mjs:254–276` 的 `uncommittedOutside` 只统计**变更目录之外**的未提交改动，故 `freshness` 仍为 `fresh`），跑 `workflow check --stage final`；**只有 exit 0** 才把**这一次**的输出写进 `reports/final-gate-final-2026-10-02.json`（其 `targetCommit` 因此也是 M，与块一致）。
5. 仅把该行提交 → M+1；本块因此**落后一个只动记录层的提交**，在 M+1 上原样复跑 `--stage final` **因 `target_commit` 过期而红，这是自指固有结果、不是缺陷**（`:467` 已声明）。`--stage archive` 需要自己的独立一次调用。

**若偏离，会产出哪些不实陈述**（逐条对应门禁判据）：

| 偏离 | 会产出的不实陈述 / 门禁后果 |
| --- | --- |
| 在 PR 合入前的分支上跑 `--stage final` 并声称 exit 0 | `workflow-check.mjs:1270` 必报「当前代码 HEAD 与计划目标引用不一致」（实测 `HEAD=6fb558f ≠ main=56de6bd`）⇒ 绿色声明为假（F106 就是这条） |
| 第 ③ 步只改 `target_commit`、不改 `contract_digest` 就声称绿 | `:1272` 还会报「验收结论的 contract_digest 已失效」（块 `:445` 现为 `b706a9ca…`，实测已 `93356f96…`）⇒ 部分绿色声明为假（F110） |
| 第 25 行绑一个**未在写入时刻实测**的摘要（例如改动前测的值，或在 `tasks.md` 定稿前测的值） | 门禁报「Plan Revision 未绑定当前规划契约摘要」；若仍报 PASS，则属「评审对象与实际绑定版本不同」的声明不实（`roles/reviewer.md` §3 的 MAJOR 类） |
| 把第 25 行绑到 Round 24 报告 | 该报告只对 `6badf05b…` 成立 ⇒ 以未审摘要冒充已审 ⇒ 声明不实 |
| 写入第 25 行后再改 `tasks.md`/`plan.md`/`design.md`/`proposal.md`/`specs/**` | 摘要再变、第 25 行绑定与块内 `contract_digest` 同时失效；此后任何「DCR 当前有效 / final 绿」的说法为假（本轮以后**必须**冻结这些文件） |
| 提交块内 `target_commit` 那一行后声称该 tip 绿 | 块落后一个记录层提交 ⇒ 绿色声明为假（`:467` 已把这一稳态写明） |

---

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| **DR1-F108** | MINOR | `reports/ci-pr37-checks-evidence.md:10–13`（错名字在 `:12`；`:62` 的留痕括注因此也不准确） | DR1-F101 点名的第三处**只加了后文订正**：`## 一、` 下代码块仍为 `test a_persisted_directory_replaced_by_a_symlink_is_refused_before_any_backend_call ... ok` + `test store::unix_modes::private_directory_and_entry_file_modes_are_restrictive ... ok`（我直接读到的原文）；正确两条只出现在 `:58–59`（`## 三·补`）。main 回复第 7 问自认「**那是漏改，不是有意保留**」。对照依据：`reports/merge-u1-candidate.md:262–263`、`reports/tp2-tester.md:520–521`、`crates/app/tests/session_resume_e2e.rs:1512–1514` 均指 `an_inaccessible_…` | 同一份被 `tasks.md:88` 与 `verification.md:128` 点名的唯一原始证据落点**自相矛盾**：只读 `## 一` 的读者会按名索证到另一条用例；「本文件…原把第二条记成…」的留痕与实际正文不符 | 把 `:12` 换成 `an_inaccessible_persisted_directory_is_refused_before_any_backend_call ... ok`（或在块下加一行「本块第二行已由 §三·补 订正」），并把 `:62` 措辞改为「本文件 §一 的代码块原把…」。`reports/**` 不入摘要，零代价 | 待复核（F101 第三处未闭环） |
| **DR1-F109** | MINOR | `reports/ci-pr37-checks-evidence.md:3` | 该行把两次运行配成「`36973886861` @ `39d4b9b`（`pull_request`）／`36973882474` @ `56de6bd`（`push`）」，但 main 本轮实测原文为 `gh run view 36973882474 --json event,headSha,conclusion` → `{"conclusion":"success","event":"push","headSha":"39d4b9b68cd028aac896ae6db019f6a3b008f505"}` ⇒ 该次运行的 head 是 **`39d4b9b`**，不是 `56de6bd`（`56de6bd` 是 `39d4b9b` 的父提交、即当时的 `refs/heads/main`；Round 24 的 C4 亦记 `39d4b9b` 非 main 祖先） | 证据文件把一次 CI 运行挂到一个它并未运行的提交上；因两次运行实际都在同一提交、且 Round 24 C4 已实测 `git diff --stat 39d4b9b..HEAD -- crates/` 为空（产品代码一致），**实质影响有限**，但属引用层不精确，与「如实留痕」口径不符 | 改为「`36973882474` @ `39d4b9b`（`push` 事件）」并注明「两次运行落在同一提交 `39d4b9b`（一次 `push`、一次 `pull_request`）」。`reports/**` 不入摘要，零代价 | 待复核（F103① 的第二处口径） |
| **DR1-F110** | MINOR | `verification.md:468`（收尾顺序第 ③ 步）＋ `:445`（验收块 `contract_digest`） | `workflow-check.mjs:1272` 还有 `if (assessment.contract_digest !== result.contractDigest) fail('验收结论的 contract_digest 已失效')`；实测摘要已从 `b706a9ca…` 变为 `93356f96…`，而块内 `:445` 仍是 `sha256:b706a9ca…`；`:468` 的第 ③ 步只要求「把本块 `target_commit` 改为 M」，未提该字段（Round 24 的 C-1 前提⑤提到过，但**未写进这段「须逐字执行」的顺序**） | 按字面逐字执行该顺序，第 ③ 步仍会红一条；若就此声称绿，即 F93 型「记录声称绿而门禁红」的不实陈述 | 第 ③ 步改为「把本块 `target_commit` 改为 M、**并把 `contract_digest` 更新为该轮 `--stage plan --json` 实测值**（两处编辑都留在工作区不提交）」。`verification.md` 不入摘要，零代价 | 待复核 |
| **DR1-F111** | MINOR（DR1-F99 同类复发） | `reports/final-gate-final-2026-10-02.json`（实测不存在）、`verification.md:465`、`tasks.md:86` | 我 `read` 该路径返回 **ENOENT**；main 的 `git status --porcelain -uall` 原文含 ` D openspec/changes/session-resume/reports/final-gate-final-2026-10-02.json`；`ls` 只列 `…-c9ab2fc.json`（`result: PASS`、`targetCommit: c9ab2fc…`、`contractDigest: b706a9ca…`、`errors: []`）与 `…-fail.json`。main 确认是**改名**：「『当前绑定值对应的通过那次』至今尚未生成（须等收尾第 ③ 步）」。而 `:465` 写「三份产物均保留：`…-2026-10-02.json`（**当前绑定值对应的通过那次**）…」，`tasks.md:86` 写「原始输出见 `reports/final-gate-final-2026-10-02.json`」 | 两处**当下为假**；照现状提交还会把 `56de6bd` 已入库的那份通过产物从版本控制中删掉（`-c9ab2fc.json` 现为未跟踪副本）；「当前绑定值对应」这一标签本身也不成立（当前最大轮次行的摘要是 `6badf05b…`，该产物绑的是 `c9ab2fc…`/`b706a9ca…`） | 提交记录批**之前**：把 `-c9ab2fc.json` 复制回主名（或 `git checkout --` 恢复）并 `git add` 该副本；把 `:465` 括注改为按「该产物绑的是哪一次运行/哪个摘要」标注（主名＝`tasks.md:86` 所指的 `c9ab2fc…` 那次，收尾第 ③ 步会被 M 那次覆写）。全部在 `verification.md`/`reports/**` 内，**不动 `tasks.md`** 的做法正确（改它会再开一轮）。**我对 main 提议的评估**：复制回主名确实让 `tasks.md:86` 的句子重新为真（该句已自限于「验收时目标提交 `c9ab2fc…`」），但我认为必须**同时**改 `:465` 的标签，否则缺口只是从「文件缺失」换成「标签不实」 | 待复核 |

> **未发现 CRITICAL / MAJOR。** Round 24 的 F101/F102/F103/F104/F105/F106：**F102、F103②③、F104、F105、F106 已闭环**；**F101 只闭环 2/3 处**（F108）；**F103① 的事件口径已闭环、`headSha` 配对仍错**（F109）；F107（过程层 SUGGESTION）不适用。

---

## Assessment

**本轮结论：PASS**（**0×CRITICAL、0×MAJOR**；4×MINOR：`DR1-F108`–`F111`，均为记录层、零摘要代价、均可与收尾同批修掉，且都不影响「①② 已由 CI 关闭」「行为契约未动」「第 24 行绑定属实」这三项实质结论）。

- **(A) 通过**：`contractDigest` 实测 `sha256:93356f96…` 与派发书逐字一致；`requirementsDigest` 仍 `5394…8fa`（⇒ 行为契约未动）；`git diff HEAD -- tasks.md` 为单一 hunk `@@ -85,5 +85,6 @@`（1 删 2 加，全在 9.1 说明段）；`git status -uall` 显示除 `tasks.md` 外无任何摘要输入被改（含未跟踪）；两次 HEAD 提交只动 `reports/**`+`verification.md`。三条归因全部成立。
- **(B) 逐条**：F102、F103②、F103③、F104（三处均已改为「①② 已关闭、③ 仍未闭合」，`未回改` 零残留）、F105（`:464` 不再写死 `contractDigest`；保留的 `requirementsDigest` 本轮实测仍准确）、F106（`:468` 整段已补）**均已闭环**；**F101 第三处（`reports/ci-pr37-checks-evidence.md:10–13`）未闭环**（F108），F103① 剩 `headSha` 错配（F109），第 (5) 条「另存」实为改名（F111），收尾顺序缺 `contract_digest` 一环（F110）。四项均非阻断。
- **(C)**：**C-1 确认成立**（第 24 行绑 `6badf05b…`+裸 `PASS` 是如实绑定，不违反上一轮约束；门禁实测量亦一致）；**C-2 确认成立**（按 Review ID 分组取最大 Round，第 24 行随之退出全部判据，只保留在「所有多条记录须带 Round／Round 不得重复」两条上，均满足）；**C-3 已给出最终收尾顺序**（含 `HEAD == refs/heads/main` 一环，并补 F110 的 `contract_digest` 一环）与各偏离会产出的不实陈述清单。

**待补 / 未验证（不影响本轮判断）**：
1. 我**没有 shell**：A1/A2/B1/B3 的全部命令原文、`gh run view` 原文均由 main 代跑回传；我另行用 `read`/`grep` 独立复读到的文件内容与之交叉核对（`tasks.md:88–89`、`verification.md:128–129/416/443–445/464–472`、`ci-pr37-checks-evidence.md` 全文、`-c9ab2fc.json`、`-fail.json` 是否存在）。
2. `36973882474` 的 **jobs 明细**未复核（本轮只核到 `event`/`headSha`/`conclusion`）；`cargo-deny`/`gitleaks` 的 job 结论沿用 Round 24 的 B-2（该轮已核对 `.github/workflows/ci.yml` 的 job↔工具映射）。
3. `reports/ci-pr37-checks-evidence.md` 第 3 节「markdown 链接指向未跟踪文件 = 0」的全量复核我未独立复算（旁证：该次 CI 的 `checks` job = success，含 `check:docs`）。
4. 我未执行任何测试、未跑门禁、未跑 `check-doc-links.mjs`/commitlint、未动任何文件。**静态检视结论不扩展到未执行的门禁**：`tasks.md:86` 与 `verification.md:465` 的「exit 0」断言仍是**过去时点、且其产物当前缺失**（F111），必须按 C-3 顺序重跑后才具现行效力。

**残留风险**：① F108/F109/F110/F111 若不修即提交，会把「证据文件自相矛盾」「产物缺失」写进主分支记录（F99/F101 型复发）；② 第 25 行若绑到未在写入时刻实测的摘要，立即升为 MAJOR 类声明不实；③ 收尾第 ③ 步若照 `:468` 字面执行（只改 `target_commit`），门禁仍会红一条 `contract_digest`（F110）。

**隔离方式与限制**：全新子 Agent（第 25 个独立实例），只读工具集、无 shell、无写权限；未参与任何实现、合并或既往轮次；一切实测由 main 代跑并回传**完整原文**，我逐条与仓库内文件交叉核对；本轮我未调用任何写工具、未提交、未跑 `cargo`/`npm`。

```yaml
handoff_index:
  - task_id: "NOT_APPLICABLE"
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    round: 25
    stage: plan
    target_revision: "sha256:93356f963422dac3a2d95ad700449b914432c83d267e12d422bfdc7a71329ba2"
    evidence_type: REVIEW
    evidence_id: DR1
    report_path: openspec/changes/session-resume/reports/dr1-dependency-review-round25.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "订正复核轮（Round 25）：在同一 contractDigest 上核实 (A) 摘要变化的唯一成因（tasks.md 9.1 单 hunk 1 删 2 加）、requirementsDigest 未变、(B) F101–F106 逐条闭环与第 (5) 条另存、(C) DCR 最大轮次取行机制与最终收尾顺序；0×CRITICAL/0×MAJOR，4×MINOR（F108–F111，记录层）"
    source_evidence: NOT_APPLICABLE
```

---

```acceptance-report
{
  "criteriaSatisfied": [
    {
      "id": "criterion-1",
      "status": "satisfied",
      "evidence": "报告含 (A)(B)(C) 逐条判定（A：实测 contractDigest=sha256:93356f96…、requirementsDigest=5394…8fa 未变、git diff/status/log 原文证明唯一参与摘要的改动是 tasks.md 9.1 单 hunk；B：F102/F103②③/F104/F105/F106 已闭环、F101 第三处未闭环、第 (5) 条另存实为改名；C：第 24 行绑定属实、门禁取最大 Round、最终收尾顺序含 HEAD===refs/heads/main 并列出各偏离的不实后果）、4 条 findings（F108–F111 全为 MINOR）、明确 PASS 判定、隔离方式与限制、残留风险"
    }
  ],
  "changedFiles": [],
  "testsAddedOrUpdated": [],
  "commandsRun": [
    {
      "command": "npx --quiet --no-install openspec-agentic workflow check --change session-resume --stage plan --planning-root D:/Project/acp-remote --json",
      "result": "not-run",
      "summary": "由 main 代跑（本实例无 shell）；实测 EXIT=1、contractDigest=sha256:93356f963422dac3a2d95ad700449b914432c83d267e12d422bfdc7a71329ba2、requirementsDigest=sha256:5394327093e57d05b49ea4b04db708fcb97160791ea6cca8cec5a833154408fa、errors 恰 1 条（DCR DR1 Plan Revision 未绑定当前摘要）"
    },
    {
      "command": "git rev-parse HEAD; git rev-parse --verify refs/heads/main^{commit}; git status --porcelain -uall",
      "result": "not-run",
      "summary": "由 main 代跑；实测 HEAD=6fb558f（≠ main=56de6bd）；工作区：改 ci-pr37-checks-evidence.md / tasks.md / verification.md，删 D final-gate-final-2026-10-02.json，未跟踪 round24 报告与 -c9ab2fc.json；索引为空"
    },
    {
      "command": "git diff HEAD -- openspec/changes/session-resume/tasks.md; git log --oneline -5 --name-only",
      "result": "not-run",
      "summary": "由 main 代跑并回传原文；tasks.md 为单 hunk @@ -85,5 +85,6 @@（1 删 2 加，全在 9.1 说明段，无复选框变化）；6fb558f/39d4b9b 只动 reports/** 与 verification.md"
    },
    {
      "command": "gh run view 36973882474 --json event,headSha,conclusion",
      "result": "not-run",
      "summary": "由 main 代跑；实测 {\"conclusion\":\"success\",\"event\":\"push\",\"headSha\":\"39d4b9b…\"} ⇒ 证据文件把该运行的头提交写成 56de6bd 有误（F109）"
    },
    {
      "command": "ls -l openspec/changes/session-resume/reports/final-gate-*（及 read 该 JSON 路径）",
      "result": "not-run",
      "summary": "由 main 代跑 + 我独立 read 复现：主名 JSON 不存在（ENOENT / git status ' D'），只存在 -c9ab2fc.json（PASS，targetCommit=c9ab2fc…）与 -fail.json"
    }
  ],
  "validationOutput": [
    "plan gate: EXIT=1, contractDigest=sha256:93356f963422dac3a2d95ad700449b914432c83d267e12d422bfdc7a71329ba2（与 Target Revision 逐字一致）, requirementsDigest=sha256:5394327093e57d05b49ea4b04db708fcb97160791ea6cca8cec5a833154408fa, errors=['Dependency Declaration Review 的 DR1 Plan Revision 未绑定当前规划契约摘要']",
    "git: HEAD=6fb558fc71ab78f3542b9e4fbd31dfb49c3f9a5a, refs/heads/main=56de6bd9e7c934895f8ae2ea5c489ca9187052d9, status=' M ci-pr37-checks-evidence.md / D final-gate-final-2026-10-02.json / M tasks.md / M verification.md / ?? round24 报告 / ?? -c9ab2fc.json'",
    "tasks.md diff: 单 hunk @@ -85,5 +85,6 @@，1 删 2 加，全在 9.1 说明段；复选框无变化",
    "gh run 36973882474: event=push, headSha=39d4b9b68cd028aac896ae6db019f6a3b008f505（与证据文件所写 56de6bd 不符）",
    "ci-pr37-checks-evidence.md:10–13 代码块仍含 'test store::unix_modes::private_directory_and_entry_file_modes_are_restrictive ... ok'；正确两条仅在 :58–59（## 三·补）",
    "verification.md:445 块内 contract_digest=sha256:b706a9ca…，与实测 93356f96… 不符（workflow-check.mjs:1272 会判红）"
  ],
  "residualRisks": [
    "F108/F109/F110/F111 若不修即提交，会把「证据文件自相矛盾」「产物缺失」写进主分支记录（DR1-F99/F101 型复发）",
    "第 25 行若绑到未在写入时刻实测的摘要（例如 tasks.md 定稿前测的值），立即构成 MAJOR 类声明不实",
    "收尾第 ③ 步若照 verification.md:468 字面执行（只改 target_commit），第 33 步仍会红一条 contract_digest（F110）",
    "PR 合入前在分支上跑 --stage final 必红（HEAD=6fb558f ≠ main=56de6bd），据此声称绿即不实（F106 已勘定）",
    "本实例无 shell，全部命令输出依赖 main 代跑原文；我未独立复跑门禁/测试，静态结论不扩展到未执行的门禁",
    "36973882474 的 jobs 明细未复核；证据文件第 3 节「0 处链接指向未跟踪文件」未独立复算"
  ],
  "noStagedFiles": true,
  "diffSummary": "只读检视，本实例未改动任何文件、未暂存任何内容；工作区存在 main 的未提交改动（tasks.md 9.1 说明段 1 删 2 加、verification.md、reports/ci-pr37-checks-evidence.md、以及 final-gate-final-2026-10-02.json 被改名导致的删除），我只读未触碰",
  "reviewFindings": [
    "no blockers: 0×CRITICAL / 0×MAJOR ⇒ PASS",
    "minor: reports/ci-pr37-checks-evidence.md:10–13（:12 错名字）- F101 第三处未闭环，代码块仍把第二条写成 store::unix_modes::private_directory_and_entry_file_modes_are_restrictive；正确名只在 :58–59（DR1-F108）",
    "minor: reports/ci-pr37-checks-evidence.md:3 - 运行 36973882474 的 headSha 写成 56de6bd，实测为 39d4b9b（两次运行同一提交、一次 push 一次 pull_request）（DR1-F109）",
    "minor: verification.md:468 收尾顺序第 ③ 步 + :445 块内 contract_digest - 门禁还比对 assessment.contract_digest（workflow-check.mjs:1272），块内 b706a9ca… 已过期于实测 93356f96…，顺序未含该字段更新（DR1-F110）",
    "minor: reports/final-gate-final-2026-10-02.json 不存在（git status ' D'，实为改名）- verification.md:465「三份产物均保留…（当前绑定值对应的通过那次）」与 tasks.md:86「原始输出见 …json」当下为假；须复制回主名并改正 :465 标签，且副本需入库（DR1-F111，F99 同类复发）"
  ],
  "manualNotes": "报告全文已在回复中返回，请 main 原样落盘到 openspec/changes/session-resume/reports/dr1-dependency-review-round25.md（本实例无写权限）。本轮 Target Revision 固定为 sha256:93356f963422dac3a2d95ad700449b914432c83d267e12d422bfdc7a71329ba2，判定 PASS（0×CRITICAL/0×MAJOR，4×MINOR）。收尾按报告 (C-3)：先在 verification.md/reports/** 内零代价修 F108–F111（绝不可动 tasks.md/plan.md/design.md/proposal/specs/.openspec.yaml）→ 先落盘本报告再追加 DCR 第 25 行（Plan Revision 绑实测值、Result 整格裸 PASS）并同批把验收块 contract_digest 改为同一值 → 提交并合入 main 得 M → 在 HEAD==refs/heads/main==M 的工作区把块内 target_commit 改为 M（留在工作区不提交，变更目录内的脏不破坏 freshness）→ 只有 --stage final exit 0 才把该次输出写进 reports/final-gate-final-2026-10-02.json → 仅提交该行得 M+1（块落后一个记录层提交是自指稳态）。另：我对你「把 -c9ab2fc.json 复制回主名以让 tasks.md:86 重新为真、且不动 tasks.md」的提议**认为方向正确**，但必须同时改 verification.md:465 的「当前绑定值对应的通过那次」标签，并把复制出的 -c9ab2fc.json 一并入库。我方未调用任何写工具、未提交、未跑 cargo/npm/门禁。"
}
```