# DR1 Round 24 — 未闭合项关闭证据的事实核查与自指收尾勘定（Review Type: plan）

> 持久化说明：本报告由**第 24 个全新独立 reviewer 实例**（只读工具集，无 shell、无写权限）以全文返回、由主 Agent 原样落盘。


# DR1 Round 24 — 独立复核（Review Type: `plan`）

> 持久化说明：本报告由**第 24 个全新独立只读 reviewer 实例**以全文返回（无 shell、无写工具），由主 Agent 原样落盘到 `openspec/changes/session-resume/reports/dr1-dependency-review-round24.md`。本实例未参与前 23 轮、CR1–CR8/CR-C1/CR-PM/validator，也未参与任何工作包实现、合并或本轮记录订正。
> 派发书中 main 的一切**自述**一律不作为证据；下文每条判定都给出「我亲自读到的文件位置」或「main 代跑命令的原文」。main 本轮对 B4 的自述不实已被我记为 DR1-F107。

## Review Context

| 项 | 实际取值 |
| --- | --- |
| Review ID / Round / Type | `DR1` / **24** / `plan`（`phase: plan`、`stage: plan`，`target_revision` = 当前 `contractDigest`） |
| Target Revision | `sha256:6badf05bf9b2910a92ef4b6b4aea27d5dcd0be6c3f4fb371124066b6a915ed76` — **实测逐字一致**（A1） |
| `requirementsDigest` | `sha256:5394327093e57d05b49ea4b04db708fcb97160791ea6cca8cec5a833154408fa` — **实测逐字一致，与 Round 14–23 完全相同**（A1） |
| Repository | `D:/Project/acp-remote`；`HEAD = 6fb558fc71ab78f3542b9e4fbd31dfb49c3f9a5a`（分支 `feat/session-resume`）、`refs/heads/main = 56de6bd9e7c934895f8ae2ea5c489ca9187052d9`（B1，**两者不相等**） |
| Base Revision | NOT_APPLICABLE（规划审查不针对代码 diff） |
| Requirements | `tasks.md` §9（`85–90` 及**本轮未提交改动**）、`verification.md`（`## Dependency Declaration Review`、`## Check Plan Changes`、`## Independent Validation`、`## Final Assessment`/验收块）、`reports/ci-pr37-checks-evidence.md`（全文）、`reports/final-gate-final-2026-10-02.json`、`reports/merge-u1-candidate.md:258-265`、`reports/tp2-tester.md:518-534`、`reports/validation-session-resume.md:275-282`、上轮报告 `reports/dr1-dependency-review-round23.md` |
| 项目规则 / 判据来源 | `AGENTS.md` §10/§11、`roles/reviewer.md`、`roles/_shared/role-report.md`；门禁源码 `workflow-check.mjs`（`checkDependencyReview` 512–576、验收块判据 1254–1300）、`workflow-contract.mjs`（`contractDigest` / `requirementsDigest`）、`.github/workflows/ci.yml`（job↔工具映射）；本机源码 `crates/**`（`#[cfg(unix)]` 门控清单） |
| 实测来源 | main 代跑的**只读命令原文**（A1/B1–B6/C1–C4，含对其 B4 漏跑的更正原文）；我另行直接读到的文件见上「Requirements」 |
| 隔离方式 | 全新子 Agent（第 24 个独立实例），不继承实现对话；只读工具集（read/grep/find/ls/watchdog_diff/contact_supervisor），**无 shell、无写权限**；未执行 E2E；未跑 `cargo`/`npm` |
| 未重判 | Round 1–23 已 PASS 的内容、CR1–CR8/CR-C1/CR-PM/validator 结论（除本轮的归因与事实核查） |

---

## (A) 摘要 `b706a9ca…` → `6badf05b…` 的归因 —— **判定：成立（三条全部与实测一致）**

**A1. 两个摘要实测**（main 代跑，原文）

```
$ npx --quiet --no-install openspec-agentic workflow check --change session-resume --stage plan --planning-root D:/Project/acp-remote --json
EXIT=1
"contractDigest": "sha256:6badf05bf9b2910a92ef4b6b4aea27d5dcd0be6c3f4fb371124066b6a915ed76"
"requirementsDigest": "sha256:5394327093e57d05b49ea4b04db708fcb97160791ea6cca8cec5a833154408fa"
"errors": [ "Dependency Declaration Review 的 DR1 Plan Revision 未绑定当前规划契约摘要（计划或契约变化后须重新审查并更新该行）" ]
```

⇒ `contractDigest` 与派发书给定值**逐字一致**；**只有 1 条错误**，且正是本轮 PASS 之后必须由第 24 行闭合的那条（与第 23 轮 A1 同款中间态）。

**A2. ①「自 Round 23 以来只有 `tasks.md` 的文字变化」——成立**

- 判据口径我读过源码：`workflow-contract.mjs::contractDigest` 的输入是 `.openspec.yaml`、`proposal.md`、`design.md`、`plan.md`、`tasks.md`（**仅把 `^(\s*[-*]\s*)\[[ xX]\]` 归一化为 `[ ]`**）＋ `specs/**` 全部 `.md` ＋ `plan.md` 的 `agentic-coverage` 各行 `source.path`；**`verification.md` 与 `reports/**` 根本不入摘要**。故「记录层改动不改变摘要」是机制事实，不是自述。
- B1 实测工作区状态只有一行：` M openspec/changes/session-resume/tasks.md`（未提交，**索引为空**）。
- B3 实测 `HEAD` 提交 `6fb558f`（"用 CI 实测证据关闭未闭合项并落档"）只含 `reports/ci-pr37-checks-evidence.md` 与 `verification.md` 两个文件 ⇒ **不涉任何摘要输入**。
- B6 实测 `git diff --name-only c9ab2fc..HEAD` = 9 个文件，**全在变更目录内**：6 个 `reports/**`、`tasks.md`、`verification.md`（+`wp5-coder-fix-cr5f1.md`）——**没有** `proposal.md` / `design.md` / `plan.md` / `specs/**` / `.openspec.yaml`。
- 本轮唯一参与摘要的改动（main 补跑的原始 diff，我逐行读过）：

```
@@ -85,5 +85,6 @@
 - [x] 9.1 [final-verification] …（上下文行，未改）
       - 完成（2026-10-02，验收时目标提交 `c9ab2fc…`）：…（上下文行，未改）
       - **本行两度出错，如实留痕**：…（上下文行，未改）
-      - **未闭合项（如实列出…）**：① …PENDING…；② …；③ …；④ …
+      - **曾列为未闭合项、已由 CI 实测证据关闭（2026-10-02，PR #37）**：① …；② …
+      - **仍未闭合项（不计入本次 PASS 的判定范围）**：③ …。另：④…
       - **运行时验证按用户 2026-10-01 裁定不做**：…（上下文行，未改）
```

即 **1 删 2 加、单一 hunk、全在 9.1 说明段内**；`9.1` 的复选框取值自 `56de6bd` 起就是 `[x]`，**本轮没有任何复选框变化**（即便有也会被归一化）。③ 保留为仍未闭合、④ 移入同句「另：」——与派发书所述改写完全一致。

**A3. ②「`requirementsDigest` 未变 ⇒ 行为契约未动」——成立**，且该值与 Round 14–23 逐字相同（`verification.md:463` 亦如此记载）。`requirementsDigest` 的输入只有 `proposal.md` ＋ `specs/**`（＋ Coverage 引用的既有契约），故**需求/行为契约一字未动**。

**A4. ③「`proposal.md`/`design.md`/`plan.md`/`specs/**` 未被触碰」——成立**：B6（提交区间）＋ B1（工作区唯一改动）双重覆盖，`design.md`/`plan.md` 虽参与 `contractDigest` 但两处都显示未改；`specs/**` 另由 A3 独立证明未改。

⇒ **(A) 判定：通过**。摘要变化的**唯一成因**就是 `tasks.md` 9.1 说明段的文字替换；行为契约未动。

---

## (B) 改写后的**事实核查** —— **判定：核心结论成立，但四处证据口径不实/不精确（F101/F102/F103）**

### B-1 两条 `#[cfg(unix)]` 用例「首次真正执行并全部通过」—— **成立（结论对，引用名错 → F101）**

- 被登记为 PENDING 的两条用例，证据链一致指向 `crates/app/tests/session_resume_e2e.rs`：`:1406` `#[cfg(unix)]` / `:1408` `a_persisted_directory_replaced_by_a_symlink_is_refused_before_any_backend_call`（**符号链接改指**）与 `:1512` `#[cfg(unix)]` / `:1514` `an_inaccessible_persisted_directory_is_refused_before_any_backend_call`（**`chmod 000` 打在父目录**）——见 `reports/merge-u1-candidate.md:262-265`、`reports/tp2-tester.md:520-531`、`reports/validation-session-resume.md:277-278`。我在源码里逐字确认了两条的函数名与 `#[cfg(unix)]` 位置（`app/tests/session_resume_e2e.rs`）。
- CI 日志原文（C2，`gh run view 36973886861 --log`）：

```
test a_persisted_directory_replaced_by_a_symlink_is_refused_before_any_backend_call ... ok
test an_inaccessible_persisted_directory_is_refused_before_any_backend_call ... ok
```

⇒ **两条都真的跑了、都被判 `ok`**（不是 skip、不是 `ignored`——我的过滤同时包含 `FAILED|ignored`，命中集合里没有任何一条）。**「① 已关闭」这个结论成立。**
- 但新文本把**第二条**证据条件名成了 `store::unix_modes::private_directory_and_entry_file_modes_are_restrictive`，而那是 `crates/identity-keystore/src/store.rs:537` 的 `#[cfg(all(test, unix))] mod unix_modes`、`:563` 的另一条用例（**也是** unix 门控、**也**在 CI 通过，但**不是** PENDING 清单里的那条）⇒ 见 **F101**。
- 「首次」这一超词我无法肯定性反证（没有任何证据显示此前有 CI 跑过它们），故**不判为不实**；但它属不可证否的表述，建议此后不再使用「首次」。

### B-2 `cargo-deny`（bans/licenses/sources/advisories）与 `gitleaks` 在 CI 上均 pass —— **成立**

C1 原文（`gh run view 36973886861 --json jobs`）：`合同门禁 + Rust 检查 = success`、`提交信息规范 = success`、`依赖许可证与来源 = success`、`依赖安全公告 = success`、`密钥扫描 = success`（五个 job，与证据文件第 3 行所述数量一致）。我再读 `.github/workflows/ci.yml` 核对了 job 名 ↔ 工具的映射，**不是同名错认**：`deps`（`依赖许可证与来源`）= `cargo-deny … command: check bans licenses sources`（`:102-105`）；`advisories`（`依赖安全公告`）= `command: check advisories`（`:122-125`）；`secrets`（`密钥扫描`）= `gitleaks-action`（`:138-146`）；`checks`（`合同门禁 + Rust 检查`）= `npm run check` + `npm run check:rust`（`:88-92`，后者即 `fmt/clippy/test`，Linux runner 上 `#[cfg(unix)]` 用例正是在这里执行）。⇒ 断言属实。

### B-3 「另 7 条 unix 用例同批通过」—— **不成立 / 口径不明 → F102**

同一日志的完整名单（C3②）显示：该次 Linux CI 通过的**唯一**用例共 **1059** 条（C3①），其中与 unix/权限相关且**确为 `#[cfg(unix)]` 门控**的至少 **14** 条：`a_persisted_directory_replaced_by_a_symlink_…`、`an_inaccessible_persisted_directory_…`、`created_files_are_owner_only`（`storage-sqlite/tests/permissions.rs:121`）、`relaxed_data_directory_fails_closed_in_strict_mode`（同文件 `:72`）、`store::unix_modes::private_directory_and_entry_file_modes_are_restrictive`、`transport::local::platform::unix::tests::peer_user_id_matches_current_user_for_a_local_pair`、`transport::net::permissions::tests::unix_inspection_reports_relaxed_for_world_readable_file`、`transport::net::tests::direct_mode_relaxed_permissions_fail_closed`、以及 `unix_endpoint_*` 家族 **7** 条（`crates/server/tests/local_endpoint_unix.rs` 6 条 + `local_endpoint_naming.rs:60`）。故：若「另 7 条」读作「除上述两条外还有 7 条 unix 用例」⇒ **与事实不符**；若专指 `unix_endpoint_*` 家族（恰好 7 个名字）⇒ 数字碰巧为真，但**文本没有点明该范围**，且该家族里 `unix_endpoint_uses_xdg_runtime_dir_then_data_dir_run` 甚至**不是** `#[cfg(unix)]` 门控（见 F103）；若读作所引证据文件的那份「同批」清单，则清单有 **9** 行 ⇒ **与「7」自相矛盾**。三种读法里没有一种与「文本 + 所引证据」同时自洽。

### B-4 证据落点 `reports/ci-pr37-checks-evidence.md` —— **可读 ✓，但「内容与新文本一致」✗**

我全文读过该文件（55 行）。它**可读**、且第 3 节的 `check:docs` 报红处置与 `39d4b9b` 的实际改动（`reports/wp5-coder-fix-cr5f1.md` + `verification.md`）吻合。但有三处口径不实：第二条用例名错（F101）、「同批…`#[cfg(unix)]` 用例（节选）」清单含 2 条非 unix 门控用例、「均为 push 触发」被实测反证（F103），另有文件末尾残留 `SS`。
**适用性旁证**：C4 实测 `git diff --stat 39d4b9b..HEAD -- crates/` **为空** ⇒ 那次 CI 覆盖的**产品代码与当前完全一致**；但 `39d4b9b` 并非 `refs/heads/main` 的祖先（`merge-base --is-ancestor` 退出码 1，main 停在 `56de6bd`），且该运行是 `pull_request` 事件（跑的是 PR 合并态），引用时须按此口径描述（F103）。

⇒ **(B) 判定：事实核查通过「结论层」，不通过「引用层」**——「①② 已关闭」「cargo-deny/gitleaks 均 pass」两个结论都有原始日志支撑；支撑文本里的用例名、数字、节标题与触发方式共 4 处需要订正。

---

## (C) 两个确认

### C-1 DCR 第 24 行的绑定 —— **确认成立（附 4 个必须同时满足的前提）**

我逐行读了 `workflow-check.mjs:512-576`：

- 表按 **Review ID 分组**、校验 `Round` 为正整数且不重复，然后 `withRound.reduce(max)` **取最大 Round 行**作为该 ID 的当前结论；**只有被选中的那一行**参与 `Plan Revision === contractDigest`、`Result` 整格 `PASS`、`Reviewer` 非 WP Owner、`Report Path` 可读四项判据。⇒ 第 24 行会成为最大轮次行并被取用；**第 23 行（`b706a9ca…`）即使摘要过期也不进入任何判据**，只在「同一 ID 多条记录时都必须带 Round」与「Round 不得重复」两条上被看到（现存 1–23 行都带 Round、24 不重复）⇒ **对门禁无影响，确认成立。**
- 前提（缺一即红）：① `Plan Revision` **逐字**等于当时实测摘要；② `Result` 整格 `trim().toUpperCase() === 'PASS'`（**不能**写 `PASS（…）`、加粗、`裸 PASS 开头`）；③ `Reviewer` 不得等于任何 WP Owner（`plan.md` 的 `Owner` 列，集合为 `coder-A…F`、`tester-A`）；④ `Report Path` 必须能按 `changeRoot`/`projectRoot` 两种基准之一解析且**文件在门禁运行时已存在**（第 20–23 行用的 `reports/…` 变更目录相对写法可命中）；⑤ 验收块的 `contract_digest` 必须同批更新为同一值，否则 `--stage final` 报「验收结论的 contract_digest 已失效」。
- **⚠️ 一个必须点名的时间序约束**：本轮 PASS 的对象是 **`sha256:6badf05b…`**。若 main 先改 `tasks.md`（修 F101/F102）再写第 24 行，则第 24 行的 `Plan Revision` 会变成**从未被本轮审过**的新摘要，属「声明不实」（`roles/reviewer.md` §3 的 MAJOR 类）。⇒ 两条合法路径只能选一条：**(甲)** 第 24 行绑 `6badf05b…`（本轮 PASS 真实有效），随后单独修 `tasks.md` 并重开 **Round 25** 绑新摘要；**(乙)** 先修 `tasks.md`，则**不得**写第 24 行引用本报告，须直接开 Round 25。

### C-2 验收块 `target_commit` 的自指与收尾顺序 —— **措辞方向对，但现计划缺一环（F106）；另需两处措辞**

- 自指机制我确认过源码：`--stage final` 要求 `assessment.target_commit === git rev-parse refs/heads/main^{commit}`、**`HEAD === refs/heads/main`**、`assessment.contract_digest === 实测`、`assessment.result === 'PASS'`；且提交不可能包含自身 ⇒「块已提交且该 tip 上仍绿」不存在。`verification.md:466`（DR1-F96）已如实写明这一点，因此**不产出不实陈述**。
- **但现计划确实缺一环**：实测 `HEAD = 6fb558f`（分支 `feat/session-resume`）≠ `refs/heads/main = 56de6bd` ⇒ 门禁会先以 `if (head !== target) fail('当前代码 HEAD 与计划目标引用不一致；请在目标版本 worktree 检查')` 失败，**与 `target_commit` 写得对不对无关**。按 main 原文顺序（「先提交记录 → 再把 `target_commit` 指向新 tip → 跑 `--stage final` 取绿」）在 PR 合入前执行，**必然红**。见 **F106**；确切句子见下「给 main 的可照抄措辞」第 (4) 条。
- 另需两处措辞（都在零代价文件里）：① `reports/final-gate-final-2026-10-02.json` 与块内 `target_commit` 必须**同批同 tip**（否则 F94 型「结论与产物不一致」）；② `verification.md:463` 写死的摘要、`## Independent Validation` 与 `Unresolved` 的「①② PENDING」、`## Check Plan Changes` 第 128 行「**未回改 `tasks.md`**」——三处都已与新结论冲突（F104/F105）。
- **我判定：按 F106 修正后的顺序逐字执行，不产出不实陈述；「改 `tasks.md` 会再次变更摘要、使本轮绑定失效」这一风险描述属实**（A1 + A2 已实测证明机制）。

---

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| **DR1-F101** | MINOR | `tasks.md:88`（新增 bullet）、`verification.md:127`、`reports/ci-pr37-checks-evidence.md:8-13` | 三处把 PENDING 清单里的**第二条**（`chmod 000`）条件名成 `store::unix_modes::private_directory_and_entry_file_modes_are_restrictive`；既有登记（`reports/merge-u1-candidate.md:263`、`tp2-tester.md:521`、`validation-session-resume.md:278`）与源码（`crates/app/tests/session_resume_e2e.rs:1512-1514`）都指明是 `an_inaccessible_persisted_directory_is_refused_before_any_backend_call`。被误引的那条是 `crates/identity-keystore/src/store.rs:537,563` 的 `#[cfg(all(test, unix))]` 用例（CI 亦 pass，但不是本项要关闭的对象）。**结论成立、引用错**（C2 两条 `... ok` 原文都在） | 关闭断言把 `chmod 000` 那一族的证据指到另一条用例上；后续审计按名索证会落空，与「如实留痕」口径不符。`verification.md`/`reports/**` 不入摘要，`tasks.md` 入摘要（见 C-1） | 按「给 main 的可照抄措辞」(1)(2) 订正三处 | 待复核 |
| **DR1-F102** | MINOR | `tasks.md:88`（同句） | 「另 7 条 unix 用例同批通过」：C3① 该次 CI 通过 **1059** 条；C3② 其中 `#[cfg(unix)]` 门控的至少 **14** 条（含 `unix_endpoint_*` 家族 7 条）；所引证据文件同一标题下的清单是 **9** 行 ⇒ 三种读法无一自洽 | 记录里出现一个无法从所引证据复算的数字；读者按证据文件数会得到 9，与 7 冲突 | 去掉数字，改为「另有多条（名单见证据文件）」；若要保留数字，须点明「`unix_endpoint_*` 家族 7 条」（该数字经 C3② 逐条核对为真） | 待复核 |
| **DR1-F103** | MINOR | `reports/ci-pr37-checks-evidence.md:3`、`:15`、`:55` | ① `:3` 称两次运行为「均为 `push` 触发」，实测 `gh run view 36973886861 --json event` = **`pull_request`**（headSha `39d4b9b`，第二条 `36973882474` 未复核）；② `:15` 标题写「同批一同执行并通过的 `#[cfg(unix)]` 用例（节选）」，但清单 9 行中 `unix_mode_bits_map_to_the_expected_verdicts`（`crates/storage-sqlite/tests/permissions.rs:54-56`，**无 cfg**）与 `params_keep_integer_fidelity`（`crates/server/src/local_admin/envelope.rs:583-584`，**无 cfg**）并非 `#[cfg(unix)]`；③ `:55` 文件末尾残留 `SS`（2 字节） | 证据文件是**被 `tasks.md` 与 `verification.md` 点名的唯一原始证据落点**，其标题口径与触发方式直接决定读者对「哪些用例属于这次 CI 首次执行」的判断；③ 属像素级噪声 | 标题改为「同批通过、与 unix/权限相关（或名称相关的）用例（节选）」并就地标注那两条非门控；触发方式改为逐运行为准；删掉末尾 `SS`。`reports/**` 不入摘要，零代价 | 待复核 |
| **DR1-F104** | MINOR | `verification.md`（`## Independent Validation` 下「**仍未闭合（不计入 PASS）**」句；`## Final Assessment` 的 `Unresolved` ①②；`## Check Plan Changes` 第 128 行） | 三处仍写「① 2 条 `#[cfg(unix)]` 用例 **PENDING**」「② `cargo-deny`/`gitleaks` 只由 CI 判定」，与 `tasks.md:88` 的新结论直接冲突；第 128 行更写「**未回改 `tasks.md`**…此项处置待用户决定」，而实测 `git diff HEAD` 的唯一改动**就是** `tasks.md` 9.1（本轮 DR1 正是因此重开） | 同一变更的两份记录对同一事实给出相反结论；第 128 行已成不实陈述 | 三处按「给 main 的可照抄措辞」(3) 改为「已由 PR #37 的 CI 实测证据关闭（见 `reports/ci-pr37-checks-evidence.md`）」，并保留原句作为历史状态。`verification.md` 不入摘要，零代价 | 待复核 |
| **DR1-F105** | MINOR | `verification.md:463` | DR1-F98 复发：该行仍把 `contractDigest` 写死为 `sha256:b706a9ca…`，而本轮实测为 `sha256:6badf05b…`（若再改 `tasks.md` 还会变） | 同一份记录对同一事实给出过期值；上一轮的整改（「改为以实测值为准，避免再写死」）事实上没做到 | 删掉写死的值，只留「以 `--stage plan --json` 实测值为准（本轮实测 `sha256:6badf05b…`，绑定 DR1 Round 24）」，或在本批末尾按当时实测值更新并注明绑定轮次。零代价 | 待复核 |
| **DR1-F106** | MINOR | `verification.md:464`、`:466`（验收块说明区）与 main 的收尾计划 | `--stage final` 除 `target_commit` 外还要求 `HEAD === refs/heads/main`（`workflow-check.mjs` 验收块判据区）；实测 `HEAD = 6fb558f ≠ refs/heads/main = 56de6bd` ⇒ 现计划在 PR 合入前跑 final 必然红，可能诱使「先把 `target_commit` 改掉就说绿了」这类不实记录 | 收尾顺序缺一环；块内现有自指说明（F96）虽方向正确，但没写明「绿态只在 `HEAD == refs/heads/main` 的工作区上取得」 | 采纳「给 main 的可照抄措辞」(4) 的五行顺序与整句 | 待复核 |
| **DR1-F107** | SUGGESTION（过程层，非目标版本缺陷） | main 本轮对 B4 的回复 | 先写「`git diff HEAD -- tasks.md` **输出为空**」，随后自认「只跑了 `--stat`、**没跑**该命令」，即「空输出」是未执行而非实测为空；随后补发了完整 diff 原文 | 若我据此认定「无改动」，(A)① 会得出相反结论——正是「自述不可采信」这一契约要防的情形。**本轮结论未依赖该自述**（我另用 B1/B3/B6 交叉证明，并在收到更正原文后逐行核对了单 hunk 内容） | 取证阶段凡未执行就写「未执行」，不要写「输出为空」；这是 main 已自认的疏漏，仅登记留痕 | 不适用 |

> 未发现 CRITICAL / MAJOR。Round 23 的 F98（写死摘要）/F99（绿态产物缺失）/F100（findings 未登记）本轮处置状态：**F99 实质已闭合**（`reports/final-gate-final-2026-10-02.json` 实测存在且为 `result: PASS`、`errors: []`、`targetCommit: c9ab2fc…`）；**F98 未闭环且复发**（F105）；**F100 未复核**（不在本轮范围）。

---

## 给 main 的可照抄措辞（一次给足）

**(1) `tasks.md:88` 整行替换**（原行是删除行，新行如下；行首缩进与其余 bullet 一致，**整段仍为一行**）：

```
      - **曾列为未闭合项、已由 CI 实测证据关闭（2026-10-02，PR #37；DR1-F101/F102 订正）**：① 2 条 `#[cfg(unix)]` 用例（`crates/app/tests/session_resume_e2e.rs:1408` 符号链接改指、`:1514` `chmod 000` 打在父目录）原为 **PENDING**——本机 Windows 从未编译执行、交叉编译缺 `x86_64-linux-gnu-gcc`，证据为零；Linux `checks` job **真正编译并执行，两条均通过**（`a_persisted_directory_replaced_by_a_symlink_is_refused_before_any_backend_call ... ok`、`an_inaccessible_persisted_directory_is_refused_before_any_backend_call ... ok`）。同一 job 另有多条 unix/权限相关用例通过（`unix_endpoint_*` 家族、`store::unix_modes::private_directory_and_entry_file_modes_are_restrictive`、`transport::…unix…` 等，**完整名单以 `reports/ci-pr37-checks-evidence.md` 为准、不在此计数**）。② `cargo-deny`（bans/licenses/sources/advisories）与 `gitleaks` 本机无本地等价物、只能由 CI 判定，实测均 **pass**。原始证据见 `reports/ci-pr37-checks-evidence.md`。
```

（若你想保留数字，把括号内改为「`unix_endpoint_*` 家族 7 条 + 其它 unix/权限用例」，该 7 已由 C3② 逐条核对为真；但**不要**再写「另 7 条」这种无法定位范围的总数。）

**(2) `verification.md:127`（`## Check Plan Changes` 第一条）**：把 `store::unix_modes::private_directory_and_entry_file_modes_are_restrictive ... ok` 改为 `an_inaccessible_persisted_directory_is_refused_before_any_backend_call ... ok`，并在句末加「（DR1-F101：第二条原误记为 `crates/identity-keystore` 的 `store::unix_modes::…`，那是一条**另一条** unix 门控用例；本轮已按既有登记 `crates/app/tests/session_resume_e2e.rs:1514` 更正）」。

**(3) `verification.md:128` 整条替换**：

```
  - **注意（如实登记）**：`tasks.md` 9.1 的「未闭合项」①/② 两条写于 CI 运行**之前**；本批**已回改** `tasks.md` 9.1 的说明（①② 改为「已由 CI 实测证据关闭」，③④ 保留为仍未闭合），该项改动使 `contractDigest` 变为 `sha256:6badf05b…`，因此使 DR1 第 23 行的绑定失效——按 DR1 Round 23 报告 (C) 的既定处理，改其文字即须重开一轮（本轮即 DR1 Round 24）；结论以本节与 `reports/ci-pr37-checks-evidence.md` 的实测证据为准。
```

同时把 `## Independent Validation` 下与 `Unresolved` 里的 ①② 一句改为：`**已由 CI 实测证据关闭（2026-10-02，PR #37）**：见 `reports/ci-pr37-checks-evidence.md` 与 DR1 Round 24 报告（③④ 仍未闭合，保留在下）`。

**(4) 追加到 `## Final Assessment` 说明区（替代/补充 `verification.md:466` 的自指段）**：

```
- **收尾顺序（DR1 Round 24 勘定，须逐字执行；补 DR1-F106）**：`--stage final` 除要求 `assessment.target_commit === refs/heads/main^{commit}` 外，还要求 **`HEAD === refs/heads/main`**（`workflow-check.mjs` 的验收块判据区 `if (head !== target) fail('当前代码 HEAD 与计划目标引用不一致；请在目标版本 worktree 检查')`），因此绿态只能在**一个 HEAD 就等于目标引用的工作区**上、且 `target_commit` 已指向该 tip 时取得。顺序：① 落盘本批全部记录改动（`tasks.md` 9.1 说明、本文件、`reports/**`）并提交；② 让 `refs/heads/main` 前进到这批记录（PR 合入后**在 `HEAD == refs/heads/main` 的工作区**操作），记该 tip 为 M；③ 把本块 `target_commit` 改为 M（**该次编辑留在工作区不提交**），跑 `workflow check --stage final`，把**这一次**的输出写进 `reports/final-gate-final-2026-10-02.json`（该产物的 `targetCommit` 因此也是 M，与本块一致）；④ 仅把该行提交，得到 M+1——本块因此落后一个只动记录层的提交，**在 M+1 上原样复跑 `--stage final` 会因 `target_commit` 失效而红，这是本块自指的固有结果、不是缺陷**；⑤ 本结论只在 M（含第 ③ 步那次未提交的该行）上成立，不得据此认为更晚的 tip 可直接复用。
```

**(5) 覆写 `reports/final-gate-final-2026-10-02.json` 前**：该文件现绑定 `c9ab2fc`；一旦第 ③ 步覆写，`verification.md:464` / `tasks.md:86` 里「通过那次见 `…json`」的指向会变成 M。要么两处措辞同批对齐，要么先把旧产物另存（例如 `…-c9ab2fc.json`）再覆写——以保持 F94 的「结论与产物一致」口径。

---

## Assessment

**本轮结论：PASS**（**0×CRITICAL、0×MAJOR**；6×MINOR：`DR1-F101`–`F106`；1×SUGGESTION（过程层）：`DR1-F107`）。

- **(A) 通过**：`contractDigest` 实测 `sha256:6badf05b…` 与派发书逐字一致；`requirementsDigest` 仍为 `sha256:5394…8fa`（⇒ 行为契约未动）；`git diff HEAD` 证明本轮唯一参与摘要的改动是 `tasks.md` 9.1 说明段的 `[-1,+2]` 单 hunk 替换，`HEAD` 提交只动 `reports/**`+`verification.md`（不入摘要），`c9ab2fc..HEAD` 不含 `proposal.md`/`design.md`/`plan.md`/`specs/**`/`.openspec.yaml`。三条归因全部成立。
- **(B) 结论层通过、引用层不通过**：「两条 `#[cfg(unix)]` 用例在 Linux CI 上真正执行并全部通过」（含 `chmod 000` 那条）与「cargo-deny 四类判定 + gitleaks 均 pass」都有原始日志/job 结论支撑（且我在 `ci.yml` 里核对了 job↔工具映射）；但新文本与所引证据文件共 4 处口径不实/不精确：**第二条用例名错配（F101，结论对、引用错）**、「另 7 条 unix 用例」数字不成立（F102）、证据文件节标题把 2 条非门控用例列为 `#[cfg(unix)]` 且「均为 push 触发」被实测反证（F103）。⇒ F101/F102 落在**入摘要**的 `tasks.md` 上，改动会再次移动 `contractDigest`，故必须按 C-1 的 (甲)/(乙) 二选一处理。
- **(C)**：**C-1 确认成立**——门禁按 Review ID 分组取**最大 Round**，第 24 行会成为被选中行，第 23 行过期不进任何判据；附 5 项前提与「第 24 行只能绑 `6badf05b…`，否则属声明不实」的时间序约束。**C-2 方向正确但缺一环**——`--stage final` 还要求 `HEAD === refs/heads/main`，实测不满足（`6fb558f` vs `56de6bd`），按现计划先跑必红；照 (4) 的五行顺序执行则不产出不实陈述（F106）。

**待补 / 未验证（不影响本轮判断）**：① 第二条 CI 运行 `36973882474` 的 `event` 未复核（F103 已按「至少一次为 pull_request」表述）；② `reports/ci-pr37-checks-evidence.md` 第 3 节「markdown 链接指向未跟踪文件 = 0」的全量复核结论我没有独立复算（旁证：`39d4b9b` 上 `checks` job = success，含 `check:docs`）；③ 我未执行任何测试、未跑门禁、未动任何文件。

**关于 `tasks.md` 改动的两条合法路径（请 main 择一并落进记录）**：
- **(甲) 最省一轮**：第 24 行绑 `sha256:6badf05b…` + 同批只改 `verification.md`/`reports/**`（F103/F104/F105）+ 按 (4) 补收尾顺序；`tasks.md` 的 F101/F102 随后单独修，并重开 **Round 25** 绑新摘要后再进入 final/archive。**注意**：这会让 `tasks.md:88` 的错误引用在修好前一直存在于主分支记录中，且 `verification.md:464`/`tasks.md:86` 的「`--stage plan` = 0 errors」在两次改动之间会再次为假，须同批只改 `verification.md` 说明。
- **(乙) 一次到位**：先按 (1) 改 `tasks.md`，**不写**第 24 行引用本报告（本报告只对 `6badf05b…` 成立），直接开 **Round 25** 绑新摘要。
两条都不产生不实陈述；**不可**把第 24 行绑到改动后的新摘要再引用本报告。

**隔离方式与限制**：全新子 Agent、只读工具集、无 shell/无写权限、未参与任何实现或既往轮次；一切实测均由 main 代跑并回传原文（含其对 B4 的更正原文），我逐条与源码/文件交叉核对；`tasks.md` 的改动是 main 的未提交工作区改动，我未触碰任何文件。

---

```acceptance-report
{
  "criteriaSatisfied": [
    {
      "id": "criterion-1",
      "status": "satisfied",
      "evidence": "报告含 (A)(B)(C) 逐条判定（A：contractDigest=sha256:6badf05b…、requirementsDigest=5394…未变、tasks.md 9.1 单 hunk [-1,+2] 归因成立；B：两条 unix 用例与 cargo-deny/gitleaks 结论有原始 CI 原文支撑、但 4 处引用口径不实；C：DCR 最大轮次取行机制确认、final 额外要求 HEAD===refs/heads/main 而实测不满足）、7 条 findings（F101–F106 MINOR + F107 SUGGESTION，无 CRITICAL/MAJOR）、明确 PASS 判定、隔离方式与限制、残留风险"
    }
  ],
  "changedFiles": [],
  "testsAddedOrUpdated": [],
  "commandsRun": [
    {
      "command": "npx --quiet --no-install openspec-agentic workflow check --change session-resume --stage plan --planning-root D:/Project/acp-remote --json",
      "result": "not-run",
      "summary": "由 main 代跑（本实例无 shell）；实测 EXIT=1、contractDigest=sha256:6badf05b…、requirementsDigest=sha256:5394…8fa、errors 仅 DCR 未绑定 1 条"
    },
    {
      "command": "git diff HEAD -- openspec/changes/session-resume/tasks.md / git diff --name-only c9ab2fc..HEAD / git log -4 --name-only / git status --porcelain -uall",
      "result": "not-run",
      "summary": "由 main 代跑；实测工作区唯一改动为 tasks.md 的 9.1 说明段替换（-1/+2，单 hunk），HEAD=6fb558f 只动 reports/**+verification.md，refs/heads/main=56de6bd"
    },
    {
      "command": "gh run view 36973886861 --json event,jobs / gh run view 36973886861 --log | grep 'test … ok'",
      "result": "not-run",
      "summary": "由 main 代跑；五 job 全 success（含 cargo-deny bans/licenses/sources、advisories、gitleaks），event=pull_request；两条 PENDING unix 用例均 '... ok'，唯一通过用例 1059 条"
    }
  ],
  "validationOutput": [
    "plan gate: EXIT=1, contractDigest=sha256:6badf05bf9b2910a92ef4b6b4aea27d5dcd0be6c3f4fb371124066b6a915ed76, requirementsDigest=sha256:5394327093e57d05b49ea4b04db708fcb97160791ea6cca8cec5a833154408fa, errors=['Dependency Declaration Review 的 DR1 Plan Revision 未绑定当前规划契约摘要']",
    "git: HEAD=6fb558fc71ab78f3542b9e4fbd31dfb49c3f9a5a, refs/heads/main=56de6bd9e7c934895f8ae2ea5c489ca9187052d9, status=' M openspec/changes/session-resume/tasks.md'",
    "tasks.md diff: 单 hunk @@ -85,5 +85,6 @@，1 删 2 加，全部位于 9.1 说明段；复选框无变化",
    "CI run 36973886861: conclusion=success, event=pull_request, headSha=39d4b9b；jobs 全 success；'test a_persisted_directory_replaced_by_a_symlink_is_refused_before_any_backend_call ... ok'、'test an_inaccessible_persisted_directory_is_refused_before_any_backend_call ... ok'；唯一通过用例 1059 条；git diff --stat 39d4b9b..HEAD -- crates/ 为空"
  ],
  "residualRisks": [
    "F101/F102 位于入摘要的 tasks.md：修则摘要再次变化、第 24 行绑定失效、须重开一轮；不修则错误引用留在记录中（两条路径都已写明）",
    "第 24 行若被绑到 tasks.md 修改后的新摘要并引用本报告（只对 sha256:6badf05b… 成立），属声明不实的 MAJOR 类风险",
    "现计划未包含 'HEAD === refs/heads/main' 前置条件（实测 HEAD≠main），按原文顺序先跑 --stage final 必红（F106）",
    "覆写 reports/final-gate-final-2026-10-02.json 为 M 的输出时，verification.md:464 与 tasks.md:86 对「通过那次」的指向会漂移（F94 型不一致）",
    "第二条 CI 运行 36973882474 的触发方式未复核；CI 证据文件第 3 节的「0 处链接指向未跟踪文件」未独立复算"
  ],
  "noStagedFiles": true,
  "diffSummary": "只读检视，本实例未改动任何文件、未暂存任何内容；工作区存在 main 的未提交改动 openspec/changes/session-resume/tasks.md（9.1 说明段 -1/+2），我只读未触碰",
  "reviewFindings": [
    "no blockers: 0×CRITICAL / 0×MAJOR",
    "minor: tasks.md:88, verification.md:127, reports/ci-pr37-checks-evidence.md:8-13 - PENDING 第二条（chmod 000）被误记为 store::unix_modes::…, 实为 crates/app/tests/session_resume_e2e.rs:1514 an_inaccessible_persisted_directory_is_refused_before_any_backend_call（DR1-F101）",
    "minor: tasks.md:88 - 「另 7 条 unix 用例同批通过」与证据不符（实测该次 CI 通过 1059 条，其中 cfg(unix) 门控至少 14 条；所引证据清单为 9 行）（DR1-F102）",
    "minor: reports/ci-pr37-checks-evidence.md:3,15,55 - 「均为 push 触发」被实测 event=pull_request 反证；节标题把 2 条非 cfg(unix) 用例列为 cfg(unix)；文件末尾残留 SS（DR1-F103）",
    "minor: verification.md（Independent Validation 未闭合句、Final Assessment 的 Unresolved ①②、Check Plan Changes:128「未回改 tasks.md」）- 与 tasks.md:88 新结论冲突（DR1-F104）",
    "minor: verification.md:463 - DR1-F98 复发，仍写死 contractDigest=sha256:b706a9ca…（现实测 sha256:6badf05b…）（DR1-F105）",
    "minor: verification.md:464/466 与收尾计划 - --stage final 还要求 HEAD===refs/heads/main，实测 6fb558f≠56de6bd，现计划缺前置条件（DR1-F106）",
    "suggestion(process): main 本轮对 B4 的自述不实（先说「输出为空」实为未执行命令），已登记留痕（DR1-F107）"
  ],
  "manualNotes": "报告全文已在本回复中返回，请 main 原样落盘到 openspec/changes/session-resume/reports/dr1-dependency-review-round24.md（本实例无写权限）。本轮 Target Revision 固定为 sha256:6badf05bf9b2910a92ef4b6b4aea27d5dcd0be6c3f4fb371124066b6a915ed76，判定 PASS，但第 24 行的 Plan Revision 只能绑这个值；若先改 tasks.md 修 F101/F102，则不得写第 24 行引用本报告，须直接开 Round 25。本轮我未调用任何写工具、未提交、未跑 cargo/npm（也未跑 scripts/check-doc-links.mjs 或 commitlint——无需）。"
}
```