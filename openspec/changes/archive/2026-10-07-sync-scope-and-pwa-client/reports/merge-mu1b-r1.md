```agentic-handoff
version: 1
task_id: "6.13"
role: merger
phase: merge
agent_context:
  agent_id: "MergerMu1b"
  isolation: "fork_turns=none（新建独立 merger，未参与 TP1 的设计、编写、修复、检视或任何候选组装对话；仅接收调度方传入的角色契约、单元编成、plan.md/tasks.md/verification.md 与目标/候选提交 SHA）。本轮只做「合入前机械核实 → 快进前跑 premerge 门禁留 PASS 凭据 → `--ff-only` 快进 → 主分支回归 → 合并相对候选差异复检」，未重跑候选 PV1、未重做候选、未改任何候选内容。"
target_revision: "33040d78324ff51be49219fcb3aac054d0100cc9"
scope: "MU1b（= TP1，Order 2）合入执行。(1) 合入前机械核实目标仓库/引用/当前提交/工作区状态与「候选未改产品实现」；(2) **快进之前**在候选工作区跑 `workflow check --stage premerge` 并留 PASS 原始 JSON 作为本单元凭据；(3) `git -C D:\\Project\\acp-remote merge --ff-only feat/tp1-contract-vectors` 把 `refs/heads/main` 从 `ad9ad3c` 快进到 `33040d7`；(4) 主分支回归 [PV1] `npm run verify` 并落盘 `reports/PV1-main-mu1b.log`；(5) 复跑合入后门禁并如实记录其固有语义错误；(6) 核对合并相对候选的新增差异。有意不做：推送、发布、归档、回滚；不改 `verification.md`；不创建/删除任何分支或 worktree。"
changes: "`refs/heads/main` 由 `ad9ad3c6b532ce0cc9366c8cc34a217f9f466933` 快进到 `33040d78324ff51be49219fcb3aac054d0100cc9`（`git merge --ff-only`，Fast-forward，exit 0，无合并提交）。主检出跟踪文件**零改动**；本轮新增两份文件（均在未跟踪的变更目录内）：本报告与 `reports/PV1-main-mu1b.log`。`verification.md` 未被我触碰（其于本轮内的多次修改由 main 完成，我观测到的 mtime 依次为 11:05:12 / 11:15:57 / 11:16:57 / 11:18:57，**均早于我本轮的任何写操作**；我全程只读）。未创建/删除分支或 worktree。"
checks:
  - id: "TARGET-REF-VERIFY-6.13-A"
    work_package: TP1
    command: "git -C D:\\Project\\acp-remote rev-parse refs/heads/main; git rev-parse feat/tp1-contract-vectors; git merge-base refs/heads/main feat/tp1-contract-vectors; git merge-base --is-ancestor refs/heads/main feat/tp1-contract-vectors; git status --porcelain; git diff --stat ad9ad3c 33040d7 -- crates/sync-protocol/src/"
    scope: "合入前机械核实（目标引用、候选 tip、祖先关系、工作区状态、候选未改产品实现）"
    environment: "主检出 D:/Project/acp-remote；Windows；git"
    exit_code: 0
    log_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1b-r1.md"
    result: "**PASS**。main = `ad9ad3c…`；候选 = `33040d7…`；merge-base = `ad9ad3c…`（候选正从当前 main 分出）；`--is-ancestor` exit 0（可快进）；`status --porcelain` 仅三个预期未跟踪项；`git diff ad9ad3c 33040d7 -- crates/sync-protocol/src/` **空输出**（TP1 未越界改实现）。逐字原始输出见「1. 合入前机械核实」。"
  - id: "PREMERGE-GATE-6.13"
    work_package: TP1
    command: "cd .worktrees/tp1 && npx --quiet --no-install openspec-agentic workflow check --change \"sync-scope-and-pwa-client\" --stage premerge --planning-root D:/Project/acp-remote --json"
    scope: "**快进前**的 premerge 门禁（复跑并留 PASS 凭据）"
    environment: "候选工作区 `.worktrees/tp1`（HEAD=33040d7）；权威规划根 = 主检出；项目本地引擎 @dongfanglin/openspec-agentic 0.4.0"
    exit_code: 0
    log_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1b-r1.md"
    result: "**PASS（exit 0）**，`errors: []`、`candidateCommit: 33040d78…`、`targetCommit: ad9ad3c6…`、`contractDigest: sha256:e4cc671c…`、`requirementsDigest: sha256:c2eb41a8…`，两条 evidence 摘要与 receipt 逐字相符。完整 JSON 见「3. 快进前 premerge 门禁（PASS 凭据）」。**该 PASS 只在快进前成立**，见「门禁调用方式与顺序」小节。"
  - id: "FF-ONLY-MERGE-6.13"
    work_package: TP1
    command: "git -C D:\\Project\\acp-remote merge --ff-only feat/tp1-contract-vectors"
    scope: "唯一允许的写操作：快进 refs/heads/main"
    environment: "主检出 D:/Project/acp-remote；Windows"
    exit_code: 0
    log_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1b-r1.md"
    result: "**PASS**。输出 `Updating ad9ad3c..33040d7` + `Fast-forward`，exit 0，39 files changed / +2235 / −39。合入后 HEAD = `refs/heads/main` = `33040d78324ff51be49219fcb3aac054d0100cc9`。未使用任何非快进 merge。"
  - id: "MAIN-REGRESSION-6.14"
    work_package: TP1
    command: "CARGO_TARGET_DIR=D:/Project/acp-remote/.target-wt/mu1b-main npm run verify（cwd=主检出，HEAD=33040d7）"
    scope: "合入后的主分支回归 [PV1] = check 十道 + check:rust 三条"
    environment: "Windows；Node v22.22.0 / npm 10.9.4；独占 CARGO_TARGET_DIR=.target-wt/mu1b-main"
    exit_code: 0
    log_path: "openspec/changes/sync-scope-and-pwa-client/reports/PV1-main-mu1b.log"
    result: "**PASS exit 0**（VERIFY_EXIT=0）。check 十道全绿 + check:rust 三条全绿；`cargo test` 94 个 test result 全 `ok`、0 failed。TP1 的两个新 suite 在主分支上实跑通过（`contract_vectors_r1_r9` 17/17、`snapshot_scope_vectors` 8/8），`schema_drift` 6/6、`envelope_fixtures` 7/7、`view_projections` 3/3。未命中 `crates/app/tests/daemon_lifecycle.rs` 已知 flake（12/12 ok）。"
  - id: "POST-MERGE-GATE-6.13"
    work_package: TP1
    command: "cd .worktrees/tp1 && npx --quiet --no-install openspec-agentic workflow check --change \"sync-scope-and-pwa-client\" --stage premerge --planning-root D:/Project/acp-remote --json（快进之后）"
    scope: "合入后复跑同一门禁（如实记录其固有语义错误）"
    environment: "同上"
    exit_code: 1
    log_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1b-r1.md"
    result: "**FAIL（exit 1），3 条 errors，属该阶段的固有语义**：`候选提交必须有待合入的变更`（快进后 target == candidate 恒等）、`候选报告的目标基线已移动`（receipt 记的是合入前基线）、`Premerge History 的 MU1b-r1 目标提交（ad9ad3c…）与当前基线不一致`（历史行同因）。**不代表候选缺陷**：`evidence[]` 两条摘要仍校验通过、`candidateCommit`/`targetCommit` 仍为真值。完整 JSON 见「4. 合入后门禁复跑」。"
  - id: "POST-MERGE-DELTA-6.15"
    work_package: TP1
    command: "git -C D:\\Project\\acp-remote diff --stat feat/tp1-contract-vectors HEAD; git rev-parse HEAD^{tree} feat/tp1-contract-vectors^{tree}"
    scope: "合并相对候选的新增差异（预期为空）"
    environment: "主检出"
    exit_code: 0
    log_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1b-r1.md"
    result: "**PASS——新增差异为零**。`git diff --stat feat/tp1-contract-vectors HEAD` 空输出；两侧 tree SHA 同为 `444997410bf584473fab478f43db3ea897eeb469`。纯快进、无合并提交，主分支结果与候选树逐字节相同。据此 6.15 可复用原候选 review ID `review-tp1-r1`。"
  - id: "DISPATCH-ACK"
    work_package: NOT_APPLICABLE
    command: "npx --quiet --no-install openspec-agentic dispatch --change \"sync-scope-and-pwa-client\" --wp TP1 --executor merger-mu1b-1 --ack"
    scope: "第一步「确认接收」"
    environment: "主检出；引擎 0.4.0"
    exit_code: 1
    log_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1b-r1.md"
    result: "**BLOCKED（结构上不可满足，按派发指示不改台账）**。输出：`openspec-agentic: 接收确认必须绑定当前活跃工作包及其实际执行者`。根因：`acknowledgeDispatch` 要求执行者等于该 WP 的**当前活跃轮次**执行者，而 TP1 的当前状态是 `ready-to-merge`、executor = `review-tp1-r1`；merger 接管不产生新的台账轮次，故 `merger-mu1b-1` 无法 ack（与 MU1a 轮次同型）。**未重开 WP、未伪造 ack、未改动 `dispatch-queue.jsonl`**。"
handoff_index:
  - task_id: "6.13"
    work_package: TP1
    role: merger
    phase: merge
    round: NOT_APPLICABLE
    stage: main
    target_revision: "33040d78324ff51be49219fcb3aac054d0100cc9"
    evidence_type: CHECK
    evidence_id: TARGET-REF-VERIFY-6.13-A
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1b-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "合入前机械核实逐项留原始输出：main=ad9ad3c、候选=33040d7、merge-base=ad9ad3c、--is-ancestor exit 0、status 仅三项目录级未跟踪项、`git diff ad9ad3c 33040d7 -- crates/sync-protocol/src/` 为空。"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.13"
    work_package: TP1
    role: merger
    phase: merge
    round: NOT_APPLICABLE
    stage: main
    target_revision: "33040d78324ff51be49219fcb3aac054d0100cc9"
    evidence_type: CHECK
    evidence_id: PREMERGE-GATE-6.13
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1b-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "**快进前**在候选工作区（HEAD=33040d7、权威规划根=主检出）跑 `workflow check --stage premerge --json`：result=PASS、errors=[]、targetCommit=ad9ad3c6…、candidateCommit=33040d78…、contractDigest=sha256:e4cc671c…、requirementsDigest=sha256:c2eb41a8…、exit 0。快进后同一门禁必然 FAIL（target==candidate），故凭据以快进前的 PASS 为准，见「门禁调用方式与顺序」。"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.13"
    work_package: TP1
    role: merger
    phase: merge
    round: NOT_APPLICABLE
    stage: main
    target_revision: "33040d78324ff51be49219fcb3aac054d0100cc9"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1b-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "`refs/heads/main` 以 `git merge --ff-only feat/tp1-contract-vectors` 由 ad9ad3c 快进到 33040d7，exit 0（`Updating ad9ad3c..33040d7` / `Fast-forward`）。合入后 HEAD = refs/heads/main = 33040d7。主检出跟踪文件零改动。"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.14"
    work_package: TP1
    role: merger
    phase: merge
    round: NOT_APPLICABLE
    stage: main
    target_revision: "33040d78324ff51be49219fcb3aac054d0100cc9"
    evidence_type: CHECK
    evidence_id: MAIN-REGRESSION-6.14
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1b-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "主分支回归 `npm run verify` 于 HEAD=33040d7 实测 exit 0（VERIFY_EXIT=0）；check 十道 + check:rust 三条全绿，94 个 test result 全 ok / 0 failed。原始日志 reports/PV1-main-mu1b.log（2249 行，sha256 f1321230a13cad5717b487df443113c0ac42135340384d9e1995407fadfb5c95）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.15"
    work_package: TP1
    role: merger
    phase: merge
    round: NOT_APPLICABLE
    stage: main
    target_revision: "33040d78324ff51be49219fcb3aac054d0100cc9"
    evidence_type: CHECK
    evidence_id: POST-MERGE-DELTA-6.15
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1b-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "合并相对候选零差异（diff 空、两侧 tree SHA 同为 44499741…），纯 --ff-only 无合并提交。main 可据此在 6.15 复用原候选 review ID review-tp1-r1（Round 1，PASS，reports/review-tp1-r1.md），无需新开检视线程。"
    source_evidence: NOT_APPLICABLE
issues: "0 CRITICAL。0 项未解决阻断。1 项过程缺陷（DISPATCH-ACK：merger 无法对 ready-to-merge 的 TP1 做台账接收确认，结构上不可满足，未改台账）。1 项归属瑕疵（main 越界代改 reports/deliver-tp1-r1.md 的 agentic-handoff 块，见「越界代改的事实登记」，由 main 自陈、我如实登记）。"
result: PASS
evidence_paths:
  - "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1b-r1.md"
  - "openspec/changes/sync-scope-and-pwa-client/reports/PV1-main-mu1b.log"
  - "openspec/changes/sync-scope-and-pwa-client/reports/receipt-mu1b.md"
  - "openspec/changes/sync-scope-and-pwa-client/reports/PV1-tp1.log"
  - "openspec/changes/sync-scope-and-pwa-client/reports/review-tp1-r1.md"
resource_cleanup: "新增并独占使用 CARGO_TARGET_DIR=D:/Project/acp-remote/.target-wt/mu1b-main（仅本次主分支回归；保留供 main 复核，可随时删除）。核实用沙箱规划根 .target-wt/mu1b-planroot 已在本轮内创建并**完全删除**（未写入权威 `verification.md`，未改动权威变更目录的任何字节）。未创建/删除任何分支或 worktree；`feat/tp1-contract-vectors` 与 `.worktrees/tp1` 原样保留（合入后该分支 tip 与 main 相同）。未 push、未打标签、未归档、未回滚。主检出跟踪文件零改动（`git status --porcelain` 仍仅三个预期未跟踪项）。无长驻进程；后台回归作业 bg_219 已结束（exit 0）。"
```

# MU1b 合入执行报告（Round 1）

> **结论速览**：合入前机械核实**全通过**；**快进前** premerge 门禁 **PASS（errors: []）**并留全 JSON 凭据；`refs/heads/main` 以 **`--ff-only`** 由 `ad9ad3c` 快进到 `33040d7`（`Updating ad9ad3c..33040d7` / `Fast-forward`，exit 0）；主分支回归 [PV1] **全绿**（`VERIFY_EXIT=0`，`check` 十道 + `check:rust` 三条，94 个 test result 全 ok / 0 failed）；合并相对候选**新增差异为零**（两侧 tree SHA 同为 `44499741…`）。**未修改 `verification.md`**，未删除任何分支或 worktree，未 push。

---

## 1. 合入前机械核实

执行者 `MergerMu1b`，时间 2026-10-04T03:0x:xxZ，cwd 主检出。**逐字原始输出**：

```
--- rev-parse main ---
ad9ad3c6b532ce0cc9366c8cc34a217f9f466933
--- rev-parse candidate ---
33040d78324ff51be49219fcb3aac054d0100cc9
--- merge-base ---
ad9ad3c6b532ce0cc9366c8cc34a217f9f466933
--- is-ancestor ---
IS_ANCESTOR_OK=0
--- status ---
?? .target-wt/
?? .worktrees/
?? openspec/changes/sync-scope-and-pwa-client/
--- STATUS_END ---
--- diff src ---
--- DIFF_SRC_END ---
```

| 核实项 | 命令 | 结果 |
| --- | --- | --- |
| 目标引用 | `git rev-parse refs/heads/main` | `ad9ad3c6b532ce0cc9366c8cc34a217f9f466933`（与派发给定一致） |
| 候选 tip | `git rev-parse feat/tp1-contract-vectors` | `33040d78324ff51be49219fcb3aac054d0100cc9`（与 receipt `candidate_commit` 一致） |
| 共同祖先 | `git merge-base refs/heads/main feat/tp1-contract-vectors` | `ad9ad3c…` —— **候选正是从当前 main 分出** |
| 可快进性 | `git merge-base --is-ancestor refs/heads/main feat/tp1-contract-vectors` | exit 0（**成功**） |
| 工作区状态 | `git status --porcelain` | 仅三个预期未跟踪项（`.target-wt/`、`.worktrees/`、`openspec/changes/sync-scope-and-pwa-client/`）——**无任何已跟踪文件的修改或暂存项**，未触发 BLOCKED |
| 候选未改产品实现 | `git diff --stat ad9ad3c 33040d7 -- crates/sync-protocol/src/` | **空输出**——TP1 未越界改实现，未触发 BLOCKED |

补充核实（供审计）：增量恰为 `39 files changed, 2235 insertions(+), 39 deletions(-)`，`git rev-list --count ad9ad3c..33040d7` = **1**（单提交、无合并提交）；`ad9ad3c^{tree}` = `6b206e67…`、`33040d7^{tree}` = `44499741…`。receipt 两条 evidence 的 SHA-256 我独立重算了 `sha256sum`，与 receipt 逐字相符：`reports/PV1-tp1.log` = `468ecae4…`、`reports/review-tp1-r1.md` = `35d4eb2d…`。

## 2. 合入瞬间的二次核实与快进

合入瞬间（同一分钟内）再次核实目标引用仍为 `ad9ad3c…`、`HEAD` 仍为 `ad9ad3c…`、`status --porcelain` 仍仅三项目录级未跟踪项、`--is-ancestor` 仍成立，随即执行：

```
$ git -C D:\Project\acp-remote merge --ff-only feat/tp1-contract-vectors
Updating ad9ad3c..33040d7
Fast-forward
 .../sync-protocol/tests/contract_vectors_r1_r9.rs  | 823 +++++++++++++++++++++
 crates/sync-protocol/tests/envelope_fixtures.rs     |  31 +-
 crates/sync-protocol/tests/schema_drift.rs          | 100 ++-
 .../sync-protocol/tests/snapshot_scope_vectors.rs   | 510 +++++++++++++++++
 crates/sync-protocol/tests/support/mod.rs           |   5 +
 crates/sync-protocol/tests/view_projections.rs      |   3 +-
 [ … 33 个 fixture/manifest 路径，略 … ]
 39 files changed, 2235 insertions(+), 39 deletions(-)
 create mode 100644 crates/sync-protocol/tests/contract_vectors_r1_r9.rs
 create mode 100644 crates/sync-protocol/tests/snapshot_scope_vectors.rs
 [ … 37 个 create mode，略 … ]
```

`MERGE_EXIT=0`。合入后核实：

```
HEAD       = 33040d78324ff51be49219fcb3aac054d0100cc9
refs/heads/main = 33040d78324ff51be49219fcb3aac054d0100cc9
feat/tp1-contract-vectors = 33040d78324ff51be49219fcb3aac054d0100cc9
status     = ?? .target-wt/  ?? .worktrees/  ?? openspec/changes/sync-scope-and-pwa-client/
ahead origin/main = 9
```

**主分支前移未产生合并提交**（纯快进，`ad9ad3c..33040d7` 单步），主检出跟踪文件零改动。

## 3. 快进前 premerge 门禁（PASS 凭据）

**顺序说明（关键）**：本阶段门禁的语义前提是「`refs/heads/main` 仍停在合入前基线」。因此依派发指示，**先跑门禁、再快进**；下表为**快进前**的完整 JSON 原始输出（`exit 0`）。

```
$ cd D:/Project/acp-remote/.worktrees/tp1
$ npx --quiet --no-install openspec-agentic workflow check --change "sync-scope-and-pwa-client" \
    --stage premerge --planning-root D:/Project/acp-remote --json
{
  "result": "PASS",
  "stage": "premerge",
  "contractDigest": "sha256:e4cc671c11eeac9f64cf259271ca7821a896a9a25fcbbbda4d4d16fb7e896aa5",
  "requirementsDigest": "sha256:c2eb41a848e4e9bd24f4e03550709fe3a44c560574fe45c4bbf040fb34cf2752",
  "targetCommit": "ad9ad3c6b532ce0cc9366c8cc34a217f9f466933",
  "evidence": [
    {
      "path": "reports/PV1-tp1.log",
      "sha256": "sha256:468ecae4349b1603450a22caec6aba1c3caded7d809a048dec1713b1bdd80c54"
    },
    {
      "path": "reports/review-tp1-r1.md",
      "sha256": "sha256:35d4eb2dc61bb57bf8fa976d7294c96b69f03863592db53b530541dcb8b6da90"
    }
  ],
  "errors": [],
  "boundary": "仅检查结构、引用和版本；不证明用户批准、角色独立性或测试真实性。",
  "planningDigest": "plan-v2:sha256:4e2fb347b974d0e8b6d347afec7cbd50591524f79ae7b33cf0d98ced853c88e1",
  "candidateCommit": "33040d78324ff51be49219fcb3aac054d0100cc9"
}
EXIT=0
```

**门禁实质核对项逐条确认（我按 `inspectPremerge` 的实际分支逐项对照，非目测）**：

| 核对项 | 判据 | 结果 |
| --- | --- | --- |
| 唯一 `agentic-premerge` 块 | 权威 `verification.md` 内恰一块，`version: 1`、`target_ref` 与计划的 `agentic-coverage.target_ref` 一致 | 通过（`refs/heads/main`） |
| target / candidate | `targetCommit = ad9ad3c…`、`candidateCommit = 33040d7…`、`candidate ≠ target`、`receipt.target_commit == target`、`receipt.candidate_commit == candidate` | 全部通过 |
| 候选包含基线 | `git merge-base --is-ancestor ad9ad3c 33040d7` | 通过 |
| 契约摘要 | `receipt.contract_digest == planned.contractDigest`（`sha256:e4cc671c…`） | 通过 |
| 行为契约摘要 | `receipt.requirements_digest == planned.requirementsDigest`（`sha256:c2eb41a8…`） | 通过 |
| verify 证据 | `result: PASS`、`candidate_commit == candidate`、`reports/PV1-tp1.log` 摘要相符 | 通过（`evidence[]` 回显该摘要） |
| review 证据 | `result: PASS`、`candidate_commit == candidate`、`reports/review-tp1-r1.md` 摘要相符、**`reviewer: ReviewerTp1` ≠ `author: testing-1-r1`** | 通过 |
| Delivery Unit | `receipt.delivery_unit = MU1b` 非空 | 通过 |
| 逐包台账就绪 | TP1 台账 `ready-to-merge`、有独立 review 记录、Handoff Index 有绑定该 WP 且属本轮认领执行者 `testing-1-r1` 的 DELIVERY PASS 行 | 通过 |
| 交付证据绑定 | `test_delivery.TP1`（automated，产物可读）+ 三个 basic_check ID（PV1/BT1/BT2）三方对齐（`handoff_index` CHECK 行 × 块内 `checks:` × `basic_checks`） | 通过 |
| 目标未移动 | 门禁结束前复读 `refs/heads/main` 仍为 `ad9ad3c…` | 通过 |
| E2E | 单元级候选阶段**不核对**任何候选 E2E 记录（阶段语义），`e2e-check` 不参与 | NOT_APPLICABLE |

### 门禁调用方式与顺序（含 4 次未 PASS 的如实记录）

本单元的门禁能 PASS 需要同时满足两个**外部前提**，二者都不由本 merger 控制：

1. cwd 必须是**候选工作区**（`.worktrees/tp1`，HEAD=33040d7）并显式传 `--planning-root D:/Project/acp-remote`。在主检出内跑会因 `target = refs/heads/main = HEAD` 恒报 `候选提交必须有待合入的变更`（`workflow-check.mjs:1489`），**在该位置不可能 PASS**。
2. 权威 `verification.md` 内必须已存在唯一 `agentic-premerge` 块（`workflow-check.mjs:1487`）。引擎**只读权威 `verification.md`**，不读 `reports/receipt-mu1b.md`——MU1a 合入后其内联块被移除，故 MU1b 须重新置入。

我在收到上述两前提前实测的 4 次非 PASS，按时间如实登记（均为**环境/登记面**成因，非候选内容缺陷）：

| # | 场景 | 输出 | 成因 |
| --- | --- | --- | --- |
| 1 | cwd=主检出 | `BLOCKED`（exit 2）`需要唯一的 agentic-premerge 代码块` | 权威 `verification.md` 尚无该块 |
| 2 | cwd=候选工作区（main 已置入块） | `FAIL`：`TP1 测试编写交付不可核对：原始报告作者与本轮认领执行者不一致`、`Premerge History 的 MU1b-r1 结果必须是 PASS` | ① `reports/deliver-tp1-r1.md` 的 `agentic-handoff` 块 `agent_context.agent_id` 与台账认领执行者 `testing-1-r1` 不一致；② 历史行 `Result` 仍为 PENDING |
| 3 | 同上（main 修 `agent_id` 与行后） | `FAIL`：`测试基础检查 BT-TP1 缺少同版本 PASS 索引、命令或零退出码` | 块内 `basic_checks`（当时取值 `BT-TP1`）无对应 `handoff_index` CHECK 行、块内无 `checks:` 顶层键、`log_path` 不可读 |
| 4 | 同上（main 逐项修块后） | `FAIL`：`测试基础检查 BT1 缺少同版本 PASS 索引、命令或零退出码` | 同一缺陷的残留（`BT1`/`BT2` 尚未补齐三处对齐） |
| 5 | 同上（三次对齐落地后） | **`PASS`，`errors: []`** | —— |

因该门禁报错措辞只给结论不给字段级定位，我在第 3/4 次之间直接以引擎自有函数 `validateTestDelivery`（`handoff.mjs`）在 Node 内复现并定位到三条精确判据（`handoff_index` 的 CHECK 行 / 块内 `checks:` 的 `command + exit_code: 0` / `log_path` 可读），据此把判据逐条发给 main 后即转 PASS。**我全程未修改 `verification.md` 与 `deliver-tp1-r1.md`**（写区仅本报告与回归日志）；相关修复由 main 在其自己的写区内完成。

## 4. 合入后门禁复跑

快进完成后复跑同一门禁（**同一调用方式**：cwd=候选工作区、`--planning-root` 指向主检出），完整 JSON 原始输出：

```json
{
  "result": "FAIL",
  "stage": "premerge",
  "contractDigest": "sha256:e4cc671c11eeac9f64cf259271ca7821a896a9a25fcbbbda4d4d16fb7e896aa5",
  "requirementsDigest": "sha256:c2eb41a848e4e9bd24f4e03550709fe3a44c560574fe45c4bbf040fb34cf2752",
  "targetCommit": "33040d78324ff51be49219fcb3aac054d0100cc9",
  "evidence": [
    { "path": "reports/PV1-tp1.log", "sha256": "sha256:468ecae4349b1603450a22caec6aba1c3caded7d809a048dec1713b1bdd80c54" },
    { "path": "reports/review-tp1-r1.md", "sha256": "sha256:35d4eb2dc61bb57bf8fa976d7294c96b69f03863592db53b530541dcb8b6da90" }
  ],
  "errors": [
    "候选提交必须有待合入的变更",
    "候选报告的目标基线已移动",
    "Premerge History 的 MU1b-r1 目标提交（ad9ad3c6b532ce0cc9366c8cc34a217f9f466933）与当前基线不一致"
  ],
  "boundary": "仅检查结构、引用和版本；不证明用户批准、角色独立性或测试真实性。",
  "planningDigest": "plan-v2:sha256:4e2fb347b974d0e8b6d347afec7cbd50591524f79ae7b33cf0d98ced853c88e1",
  "candidateCommit": "33040d78324ff51be49219fcb3aac054d0100cc9"
}
EXIT=1
```

**这三条是该阶段的固有语义，不代表候选缺陷**：

- `候选提交必须有待合入的变更`、`候选报告的目标基线已移动`：门禁比较 `target = refs/heads/main` 与 `candidate = HEAD`；快进完成后二者**必然相等**，且 receipt 的 `target_commit` 忠实记录**合入前**基线。这是「合入前 CI 预检」的固有性质（`procedures/workflow-check.md`：「`premerge` 在候选提交工作区执行，要求 `HEAD` 为候选提交、本地 `target_ref` 仍指向规划基线」），合入后前提不再成立。
- `Premerge History 的 MU1b-r1 目标提交（ad9ad3c…）与当前基线不一致`：同因的联动——历史行的 `Target Commit` 记为合入前基线，而当前基线已前移。该行由 main 在登记时按 MU1a 的既有口径处理（MU1a 行同样记 `ad9ad3c`，因其合入后基线恰为 `ad9ad3c`）。

**关键反证**：本次 FAIL 的 `evidence[]` 仍**全部通过摘要校验**（两条 SHA-256 与 receipt 逐字相符），`targetCommit`/`candidateCommit` 均为真值——说明 FAIL 仅来自上述三个结构性恒假/联动条件，实质证据链完好。派发提示预期 2 条，实测 3 条，多出的一条即上述历史行联动，机理同源。

## 5. 主分支回归 [PV1]（tasks 6.14）

```
$ cd D:/Project/acp-remote
$ CARGO_TARGET_DIR=D:/Project/acp-remote/.target-wt/mu1b-main npm run verify \
    > openspec/changes/sync-scope-and-pwa-client/reports/PV1-main-mu1b.log 2>&1
VERIFY_EXIT=0
```

- **证据路径**：`openspec/changes/sync-scope-and-pwa-client/reports/PV1-main-mu1b.log`（tasks 6.14 指定；该文件本轮之前不存在）
- **规模/摘要**：2249 行，`sha256:f1321230a13cad5717b487df443113c0ac42135340384d9e1995407fadfb5c95`
- **结果**：**PASS，exit 0**。`npm run verify` = `npm run check && npm run check:rust`。

**`check` 十道全绿**（逐道原始输出均 exit 0，日志 1–108 行）：

| # | 门 | 关键输出 |
| --- | --- | --- |
| 1 | `check:schemas` | `schema fixtures OK: 149 valid, 51 invalid (ajv Draft 2020-12), 41 event views bound` |
| 2 | `check:commands` | 通过 |
| 3 | `check:errors` | 通过 |
| 4 | `check:features` | 通过 |
| 5 | `check:assets` | `contract assets OK: 17 schemas, 213 fixture files, 12 transcript vectors re-encoded, 20 negative vectors rejected as declared, 2 SAS values recomputed` |
| 6 | `check:acp` | `ACP compatibility matrix OK: 25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families (71 rows)` |
| 7 | `check:docs` | 通过 |
| 8 | `check:boundaries` | 通过 |
| 9 | `check:drift` | 通过 |
| 10 | `check:agentic` | `doctor` 8 项全 `PASS`（toolchain / openspec / config / schema / schema validation / verification skill / AGENTS.md / manifest）；`openspec validate --all --strict` 通过 |

**`check:rust` 三条全绿**（`cargo fmt --all -- --check` → `cargo clippy --locked --workspace --all-targets --all-features -D warnings` → `cargo test --locked --workspace --all-features`）：

- 全量 `cargo test` 共 **94 个 `test result:` 行，全部 `ok`，0 个 `FAILED`**（含 doc-tests）。
- **TP1 交付的两个新 suite 在主分支上实跑通过**：`contract_vectors_r1_r9` **17 passed / 0 failed**、`snapshot_scope_vectors` **8 passed / 0 failed**；共享写点相关套件 `schema_drift` **6/6**、`envelope_fixtures` **7/7**、`view_projections` **3/3**。
- **未命中已知 flake**：`crates/app/tests/daemon_lifecycle.rs` 本次 **12 passed / 0 failed**（60.64s），无需重跑取证。

## 6. 合并相对候选的新增差异（tasks 6.15）

```
$ git -C D:\Project\acp-remote diff --stat feat/tp1-contract-vectors HEAD
(空输出)
$ git -C D:\Project\acp-remote rev-parse HEAD^{tree} feat/tp1-contract-vectors^{tree}
444997410bf584473fab478f43db3ea897eeb469
444997410bf584473fab478f43db3ea897eeb469
```

**结论：新增差异为零**。本轮为纯 `--ff-only` 快进、未产生合并提交，合入结果与候选树**逐字节相同**（两侧 tree SHA 同为 `44499741…`），既无新增路径也无修改路径。

**供 main 登记 6.15 复用**：因合并相对候选零差异，6.15 的合并后审查可**复用原候选 review ID `review-tp1-r1`（Round 1，PASS，`reports/review-tp1-r1.md`，sha256 `35d4eb2d…`）**，无需新开检视线程；如需独立复核仍由 main 另派 reviewer。

## 7. 资源释放与约束遵守

| 项 | 状态 |
| --- | --- |
| 分支 / worktree | **未创建、未删除任何分支或 worktree**。`git branch -v` 仍含 `feat/tp1-contract-vectors`（tip = `33040d7`）、`merge/mu1a-candidate-r1`、`merge/mu1a-candidate`、`feat/wp1-*`、`feat/wp2-*`；`git worktree list` 仍为 5 项，`.worktrees/tp1` 原样保留 |
| 禁止操作 | 未使用非 ff 的 `git merge`、未 `reset --hard`、未 `checkout` 切分支、未 `rebase`、未 `clean`；未 push / 未打标签 / 未归档 / 未回滚 |
| 主检出写入 | 仅新增两份报告文件（均在未跟踪的 `openspec/changes/sync-scope-and-pwa-client/` 内）；跟踪文件零改动 |
| `verification.md` | **未被我修改**；本轮内其两次修改（11:16:57、11:18:57）由 main 完成，均早于我任何写操作；我全程只读 |
| 临时资源 | 核实用沙箱规划根 `.target-wt/mu1b-planroot` 已**完全删除**（构建方式：复制 `openspec/{config.yaml,agentic.yaml,.agentic-install.json,schemas,specs,changes/<name>}` + `package.json` + junction `node_modules`；**从未调用**，最终凭证由真实权威规划根产出）；`CARGO_TARGET_DIR=.target-wt/mu1b-main` 保留供 main 复核 |
| 长驻进程 | 无。后台回归作业 `bg_219` 已结束（exit 0） |
| 本地无等价物的检查 | `deps` / `advisories` / `secrets` 三个 CI-only job **未执行、未声称通过** |

## 8. 越界代改的事实登记（如实抄录 main 自陈）

main 在 steering 中主动声明：为让门禁具备可核对前提，**由其代改**了测试作者的产物 `reports/deliver-tp1-r1.md` 的 `agentic-handoff` 块（`agent_id` 对齐台账执行者 `testing-1-r1`、新增块内顶层 `checks:` 键与 `BT1`/`BT2` 两条 CHECK 行、`basic_checks` 三方对齐、`test_delivery.artifacts` 改为变更目录内可读的 `reports/PV1-tp1.log`），并说明根因是其派发 TP1 时未把 `validateTestDelivery` 对 `test_delivery`（`kind` / `artifacts` / `basic_checks`）与块内 `checks:` 行的完整要求写进指令。main 要求如实登记、不必掩饰。

**本 merger 的独立观测可佐证该自陈**：我在第 3/4 次门禁失败时直接读了该文件，观测到 `## checks` 小节（YAML）位于 `agentic-handoff` 块**之外**、块内 `KEYS = [version, agent_context, handoff_index, test_delivery]`（无 `checks`）、`handoff_index` 仅一条 `PV1` CHECK 行、`basic_checks` 取值经历 `BT-TP1` → `[BT1,BT2,PV1]` 的变化；随后观测到块内新增 `checks:` 键（PV1/BT1/BT2 三项齐备）与两条 CHECK 行，门禁即 PASS。**该归属瑕疵不改变候选内容评价**（TP1 的交付面 `crates/sync-protocol/tests/` 与 `fixtures/sync/v1/` 未被触碰，见第 1 步 `git diff ad9ad3c 33040d7 -- crates/sync-protocol/src/` 为空；`deliver-tp1-r1.md` 属报告登记面），但按角色契约「不得把别人的工作区改动当作自己的交付」「发现上游交付与登记不符时停止并报告」，在此显式登记交 main 处置。

## 9. 接收确认（第一步）与台账

```
$ npx --quiet --no-install openspec-agentic dispatch --change "sync-scope-and-pwa-client" \
    --wp TP1 --executor merger-mu1b-1 --ack
openspec-agentic: 接收确认必须绑定当前活跃工作包及其实际执行者
EXIT=1
```

**结构上不可满足**（与 MU1a 轮次同型）：`acknowledgeDispatch` 要求 `--executor` 等于该 WP 的**当前活跃轮次**执行者；TP1 现处 `ready-to-merge`、executor = `review-tp1-r1`，merger 接管不新增台账轮次。按派发指示，**已如实说明该回退，未为此改动台账状态**（未重开 WP、未伪造 ack、未改 `dispatch-queue.jsonl`）。台账实际状态（我读得）：`WP1 merged`、`WP2 merged`、`MU1a superseded`、`TP1 ready-to-merge`。

## 10. 未声称 / 未执行

- `deps` / `advisories` / `secrets` 三个 CI-only job：本地无等价物，**未执行亦未声称通过**（与 receipt 的 `notes` 一致）。
- 未重跑候选 PV1（候选阶段证据 `reports/PV1-tp1.log` 已由 receipt 绑定并复核摘要）；本轮只做合入后主分支回归。
- 未执行 6.15 的独立审查本身（按 tasks 归独立 reviewer）；本轮只提供「零差异」这一可复用的登记依据。
- 未推送、未发布、未归档、未回滚。

## 11. 交接给 main

1. **合入实况**：`refs/heads/main` `ad9ad3c → 33040d7`（`--ff-only`，`Updating ad9ad3c..33040d7` / `Fast-forward`，exit 0，无合并提交，ahead `origin/main` 9）。请据此登记 `## Merge History` 与 `## Premerge History`（MU1b-r1 行的 `Result` 由 PENDING 改为 PASS；`Target Commit` 按 MU1a 既有口径填 `ad9ad3c`）。
2. **本单元凭据**：`## Premerge History` 应引用 `reports/receipt-mu1b.md`（receipt 已于事前持久化，其两条 evidence 摘要本轮独立重算相符）；premerge PASS 的原始 JSON 见本报告第 3 节。
3. **6.15**：合并相对候选零差异（tree 均 `44499741…`），可直接复用 `review-tp1-r1`，依据见第 6 节。
4. **待处置**：第 8 节越界代改的归属登记；第 9 节 `DISPATCH-ACK` 的过程缺陷。
5. **不要改 receipt 去迎合合入后门禁**：合入后的 3 条 errors 是阶段固有语义，receipt 的 `target_commit = ad9ad3c…` 是**合入前基线的忠实记录**，应保持不动。
