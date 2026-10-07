<!-- MU2（Order 3，成员 WP3 + WP4 + TP2）合入执行报告（Round 1）。
     结论：BLOCKED —— 未快进、未合入、未跑主分支回归。两个独立成因：
       (1) premerge 门禁 BLOCKED（权威 verification.md 缺唯一 agentic-premerge 块，exit 2）；
       (2) MU2 成员 TP2 尚未派发/交付，交付单元不完整（plan.md:971 成员 = WP3, WP4, TP2）。
     另记 1 项越界观测供 main 裁决：候选改了 docs/CORE_PORTS_AND_STORAGE.md（17+/1−）。 -->

```agentic-handoff
version: 1
task_id: "6.16"
role: merger
phase: merge
agent_context:
  agent_id: "MergerMu2"
  isolation: "fork_turns=none（新建独立 merger，未参与 WP3/WP4 的实现、修复、候选组装或检视对话；仅接收调度方传入的角色契约、单元编成与目标/候选提交 SHA。本轮只做机械核实与门禁复跑，未重跑候选 PV1/PV2、未重做候选、未改任何候选内容）。"
target_revision: "3b6fe4136e71e5a8a9b36527440348d9be280ae0"
scope: "MU2 合入执行（Round 1）。(1) 合入前机械核实目标引用/候选 tip/祖先关系/工作区状态与冻结合同边界；(2) **快进之前**在候选工作区跑 `workflow check --stage premerge`；(3) 仅在门禁 PASS 且单元成员齐备时以 `git merge --ff-only` 快进 `refs/heads/main`；(4) 合入后主分支回归 [PV1] 并落盘 `reports/PV1-main-mu2.log`；(5) 复检合并相对候选的新增差异。**实际执行到第 2 步即判定 BLOCKED：门禁未 PASS 且 TP2 未交付，故未执行第 3–5 步。** 有意不做：推送、发布、归档、回滚；不改 verification.md；不切换分支/reset/rebase/clean；不创建或删除任何分支或 worktree。"
changes: "仅新增本报告一份文件（在未跟踪的变更目录内）。**主检出、候选工作区 `.worktrees/integ` 与其余 7 个工作区全程零写入**：`refs/heads/main` 未移动（仍为 33040d78…），未执行任何 merge/checkout/reset/rebase/clean，未创建或删除分支或 worktree，未生成 receipt，未改 verification.md 与任何规划文件。**未创建 `reports/PV1-main-mu2.log`**——该文件按 tasks 6.22 应装「合入后 `npm run verify` 的完整输出」，本轮未合入故未运行该回归，不以任何非真实输出填充占位。"
checks:
  - id: "TARGET-REF-VERIFY-6.16"
    work_package: NOT_APPLICABLE
    command: "git -C D:\\Project\\acp-remote rev-parse refs/heads/main; git rev-parse integ/mu2-wiring; git merge-base refs/heads/main integ/mu2-wiring; git merge-base --is-ancestor refs/heads/main integ/mu2-wiring; git status --porcelain"
    scope: "合入前机械核实（目标引用、候选 tip、祖先关系、工作区状态）"
    environment: "主检出 D:/Project/acp-remote；Windows；git"
    exit_code: 0
    log_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu2-r1.md"
    result: "**逐项通过**。refs/heads/main = `33040d78324ff51be49219fcb3aac054d0100cc9`（与派发事实一致）；integ/mu2-wiring = `3b6fe4136e71e5a8a9b36527440348d9be280ae0`（= 目标候选）；merge-base = `33040d78324ff51be49219fcb3aac054d0100cc9`（候选正从当前 main 分出）；`--is-ancestor` **exit 0**（可快进）；`status --porcelain` 仅三个预期未跟踪项、无已跟踪文件修改或暂存项。原始输出见「1. 合入前机械核实」。"
  - id: "BOUNDARY-VERIFY-6.16"
    work_package: NOT_APPLICABLE
    command: "git -C D:\\Project\\acp-remote diff --name-only 33040d78… 3b6fe41… -- crates/sync-protocol/ schemas/ docs/ vendor/; git diff 33040d78… 3b6fe41… -- crates/core/Cargo.toml crates/agent-host/Cargo.toml"
    scope: "冻结合同与边界核实（应全部为空）"
    environment: "主检出；Windows"
    exit_code: 0
    log_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu2-r1.md"
    result: "**一项非空、需 main 裁决**：`crates/sync-protocol/`、`schemas/`、`vendor/` 与两个 Cargo.toml 的 diff 均为**空**（契约与依赖面未被触碰）；但 `docs/CORE_PORTS_AND_STORAGE.md` **非空**（17 insertions / 1 deletion，由 WP3 提交 `0f70c4c` 与 `6bf7a7c` 引入）。按调度口令「非空即越界」的字面判据，本项触发；但按其内容与 plan.md:1058「权威文档更新…与代码同提交」的登记，它是对**新增** `NodeEventSink` 与 WP3 生产者的同提交文档，属 `check:drift` 对齐面而非冻结合同本体。**本 merger 不自行裁决，如实上报**（详见「6. 边界观测」）。无论裁决如何，本轮合入均被 premerge/TP2 两项独立成因挡住。"
  - id: "PREMERGE-GATE-6.21"
    work_package: NOT_APPLICABLE
    command: "cd D:\\Project\\acp-remote\\.worktrees\\integ && npx --quiet --no-install openspec-agentic workflow check --change \"sync-scope-and-pwa-client\" --stage premerge --planning-root D:/Project/acp-remote --json"
    scope: "**快进前**的 premerge 门禁（继承 plan 阶段全部要求）"
    environment: "候选工作区 `.worktrees/integ`（HEAD=3b6fe41，干净）；权威规划根 = 主检出；项目本地引擎 0.4.0"
    exit_code: 2
    log_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu2-r1.md"
    result: "**BLOCKED（exit 2），errors = [`需要唯一的 agentic-premerge 代码块`]，`targetCommit`/`candidateCommit` 均为 null**。根因：权威 `verification.md` 内当前 **0 个** `agentic-premerge` 块；该块须由 main 在候选证据固化后置入（引擎只读权威 verification.md，不读 `reports/` 下的 receipt 文件——见 MU1b 报告对 `workflow-check.mjs:1487` 的定位）。**非候选内容缺陷**，属证据/登记面缺口。完整 JSON 见「2. 快进前 premerge 门禁」。"
  - id: "NOT-RUN-BY-GATE"
    work_package: NOT_APPLICABLE
    command: "git -C D:\\Project\\acp-remote merge --ff-only integ/mu2-wiring"
    scope: "唯一允许的写操作（快进 refs/heads/main）"
    environment: "NOT_APPLICABLE"
    exit_code: NOT_APPLICABLE
    log_path: "NOT_AVAILABLE"
    result: "**未执行**。前置条件（门禁 PASS 且 TP2 就绪）不成立，按调度口令「非 PASS 则不合入」「缺 TP2 就还没到合入时机」停在此步。`refs/heads/main` 未移动。"
  - id: "NOT-RUN-BLOCKED"
    work_package: NOT_APPLICABLE
    command: "CARGO_TARGET_DIR=D:/Project/acp-remote/.target-wt/mu2-main npm run verify; cargo test --locked -p app --test node_link_e2e"
    scope: "合入后主分支回归 [PV1]（tasks 6.22）"
    environment: "NOT_APPLICABLE"
    exit_code: NOT_APPLICABLE
    log_path: "NOT_AVAILABLE"
    result: "**未执行**。依赖合入，未合入故不运行。因此未创建 `reports/PV1-main-mu2.log`（不以任何非真实输出占位）。"
  - id: "POST-MERGE-DELTA-6.23-pre"
    work_package: NOT_APPLICABLE
    command: "git rev-parse HEAD^{tree} 3b6fe41…^{tree}"
    scope: "合并相对候选的新增差异（预期为零）"
    environment: "主检出"
    exit_code: 0
    log_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu2-r1.md"
    result: "**NOT_APPLICABLE（无合入）**。预置事实供后续：合入前 main tree = `444997410bf584473fab478f43db3ea897eeb469`、候选 tree = `abfa4788e7a64939caa0507b7e93cd85c4bf7d62`（不同，符合「候选含待合入变更」）。本轮无合入，无差异可复检。"
  - id: "DISPATCH-ACK"
    work_package: WP3
    command: "npx --quiet --no-install openspec-agentic dispatch --change \"sync-scope-and-pwa-client\" --wp WP3 --executor merger-mu2-1 --ack"
    scope: "第一步「确认接收」"
    environment: "主检出；引擎 0.4.0"
    exit_code: 1
    log_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu2-r1.md"
    result: "**BLOCKED（结构上不可满足，按派发指示不改台账）**。输出：`openspec-agentic: 接收确认必须绑定当前活跃工作包及其实际执行者`。根因同 MU1a/MU1b 轮：`acknowledgeDispatch` 要求执行者等于该 WP 的**当前活跃轮次**执行者，而 WP3 当前状态为 `ready-to-merge`、executor = `review-w3-r2`；merger 接管不产生新的台账轮次，故 `merger-mu2-1` 无法 ack。**未重开 WP、未伪造 ack、未改动 `dispatch-queue.jsonl`/`dispatch-runtime.jsonl`**。"
issues: "0 CRITICAL / 0 MAJOR / 0 MINOR / 2 BLOCKER + 1 待裁决观测。BLOCKER-1：premerge 门禁 BLOCKED（缺唯一 `agentic-premerge` 块，exit 2）。BLOCKER-2：MU2 成员 TP2 未派发/未交付，交付单元不完整（plan.md:971 成员 = WP3, WP4, TP2；TP2 在 queue/runtime/reports/verification 四方均无交付痕迹）。观测：候选改了 `docs/CORE_PORTS_AND_STORAGE.md`（17+/1−），与「冻结合同与边界应全部为空」的字面判据冲突，但内容系新增 `NodeEventSink` 的同提交文档，需 main 裁决 docs 是否属冻结边界。两项 BLOCKER 各自独立足以阻止合入。"
result: BLOCKED
evidence_paths:
  - "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu2-r1.md"

resource_cleanup: "本轮零资源创建、零资源占用：未新建 worktree、未写任何 CARGO_TARGET_DIR（未跑构建/测试）、未新建分支、未生成 receipt。复用了既有候选 worktree `.worktrees/integ`（只读跑门禁，未写入——其 `git status --porcelain` 为空）。未 push、未打标签、未归档、未回滚。主检出跟踪文件零改动（`git status --porcelain` 仍仅 `.target-wt/`、`.worktrees/`、`openspec/changes/`）。无长驻进程。"

handoff_index:
  - task_id: "6.16"
    work_package: NOT_APPLICABLE
    role: merger
    phase: merge
    round: NOT_APPLICABLE
    stage: main
    target_revision: "3b6fe4136e71e5a8a9b36527440348d9be280ae0"
    evidence_type: CHECK
    evidence_id: TARGET-REF-VERIFY-6.16
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu2-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "合入前机械核实逐项留原始输出：main=33040d78…、候选 tip=3b6fe41…、merge-base=33040d7…、`--is-ancestor` exit 0（可快进）、`status --porcelain` 仅三项目录级未跟踪项、无已跟踪修改。目标明确，未触发 BLOCKED（BLOCKED 来自门禁与单元完整性，非目标不明）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.16"
    work_package: NOT_APPLICABLE
    role: merger
    phase: merge
    round: NOT_APPLICABLE
    stage: main
    target_revision: "3b6fe4136e71e5a8a9b36527440348d9be280ae0"
    evidence_type: CHECK
    evidence_id: BOUNDARY-VERIFY-6.16
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu2-r1.md"
    result: "OBSERVATION（待 main 裁决）"
    evidence_status: NEW
    applicability_basis: "`crates/sync-protocol/`、`schemas/`、`vendor/`、两个 Cargo.toml 的 diff 均为空（契约与依赖面未动）；`docs/CORE_PORTS_AND_STORAGE.md` 非空（17+/1−，WP3 的 0f70c4c/6bf7a7c 引入），与字面「应全空」判据冲突，但内容为新增 `NodeEventSink` 与生产者的同提交文档（plan.md:1058 登记「权威文档更新…与代码同提交」）。本 merger 不自行裁决。"
    source_evidence: "reports/review-w3-r2.md（WP3 检视 PASS，覆盖该 docs 改动）"
  - task_id: "6.21"
    work_package: NOT_APPLICABLE
    role: merger
    phase: merge
    round: NOT_APPLICABLE
    stage: main
    target_revision: "3b6fe4136e71e5a8a9b36527440348d9be280ae0"
    evidence_type: CHECK
    evidence_id: PREMERGE-GATE-6.21
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu2-r1.md"
    result: BLOCKED
    evidence_status: NEW
    applicability_basis: "**快进前**在候选工作区 `.worktrees/integ`（HEAD=3b6fe41、权威规划根=主检出）跑 `workflow check --stage premerge --json`：result=BLOCKED、exit 2、`errors=[需要唯一的 agentic-premerge 代码块]`、targetCommit/candidateCommit 均 null。权威 `verification.md` 内 0 个 `agentic-premerge` 块。未 PASS，故未合入。完整 JSON 见本报告 §2。"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.17/6.18/6.19（上游）"
    work_package: NOT_APPLICABLE
    role: merger
    phase: merge
    round: NOT_APPLICABLE
    stage: main
    target_revision: "3b6fe4136e71e5a8a9b36527440348d9be280ae0"
    evidence_type: CHECK
    evidence_id: TP2-READINESS-6.16
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu2-r1.md"
    result: BLOCKED
    evidence_status: NEW
    applicability_basis: "MU2 成员（plan.md:971）= WP3 + WP4 + **TP2**。TP2 未派发/未交付：`dispatch-queue.jsonl` 0 条、`dispatch-runtime.jsonl` 0 条、`reports/` 0 份、`verification.md` Test Design 行 = `NOT_APPLICABLE（未开工）`。单元不完整 → 尚非合入时机。"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.22 / 6.23（下游）"
    work_package: NOT_APPLICABLE
    role: merger
    phase: merge
    round: NOT_APPLICABLE
    stage: main
    target_revision: "3b6fe4136e71e5a8a9b36527440348d9be280ae0"
    evidence_type: CHECK
    evidence_id: NOT-RUN-BY-GATE
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu2-r1.md"
    result: NOT_APPLICABLE
    evidence_status: PENDING
    applicability_basis: "依赖合入：`git merge --ff-only`、主分支回归 [PV1]/[PV2]（`reports/PV1-main-mu2.log`/`PV2-main-mu2.log`）、合并差异复检均因门禁 BLOCKED 与 TP2 缺失而未执行。`refs/heads/main` 仍为 33040d78…。"
    source_evidence: NOT_APPLICABLE
```

---

## 1. 合入前机械核实（6.16）

在**主检出** `D:/Project/acp-remote` 执行，逐字输出如下。

```
$ git rev-parse refs/heads/main
33040d78324ff51be49219fcb3aac054d0100cc9

$ git rev-parse integ/mu2-wiring
3b6fe4136e71e5a8a9b36527440348d9be280ae0

$ git rev-parse --verify integ/mu2-wiring
3b6fe4136e71e5a8a9b36527440348d9be280ae0

$ git merge-base refs/heads/main integ/mu2-wiring
33040d78324ff51be49219fcb3aac054d0100cc9

$ git merge-base --is-ancestor refs/heads/main integ/mu2-wiring ; echo "exit=$?"
exit=0

$ git rev-parse HEAD ; git rev-parse --abbrev-ref HEAD
33040d78324ff51be49219fcb3aac054d0100cc9
main

$ git status --porcelain
?? .target-wt/
?? .worktrees/
?? openspec/changes/sync-scope-and-pwa-client/
```

判定：
- **目标引用** = `refs/heads/main` = `33040d78324ff51be49219fcb3aac054d0100cc9`（与派发事实一致）。
- **候选 tip** = `integ/mu2-wiring` = `3b6fe4136e71e5a8a9b36527440348d9be280ae0`（= 目标候选）。
- **祖先关系**：`merge-base` = 当前 main；`--is-ancestor` **exit 0** → **可快进**。
- **工作区状态**：仅三个预期未跟踪项（`.target-wt/`、`.worktrees/`、`openspec/changes/`），**无已跟踪文件修改或暂存项**，符合预期。

### 候选相对 main 的改动面

```
$ git diff --name-status 33040d78… 3b6fe41…
M  crates/agent-host/src/bin/acpr-fake-acp-agent.rs
M  crates/agent-host/src/host.rs
M  crates/agent-host/src/lib.rs
M  crates/agent-host/src/mapper.rs
A  crates/agent-host/src/node.rs
M  crates/agent-host/src/process.rs
M  crates/agent-host/tests/session.rs
M  crates/app/src/compose.rs
M  crates/app/tests/node_link_e2e.rs
M  crates/core/src/broker.rs
A  crates/core/src/derive.rs
M  crates/core/src/lib.rs
M  crates/core/src/model/json.rs
M  crates/core/src/model/mod.rs
M  crates/core/src/ports.rs
M  crates/storage-sqlite/src/session_store.rs
M  crates/storage-sqlite/tests/{commit,contract_v03,enum_coverage,migration,retention,session_version_rule,workspace_alias}.rs
A  crates/storage-sqlite/tests/title_write.rs
M  docs/CORE_PORTS_AND_STORAGE.md
```

提交链（`git log --oneline 33040d7..3b6fe41`）：

```
3b6fe41 test(app): 补齐 AC1 的 file.changed 落库与 origin 序回放、会话标题更新两项断言
0cb5e62 fix(app): 组合根接线节点级事件（WP4 NodeEvents → WP3 Broker::commit_node_event）
559f21c merge(integ): 集成 WP4 到 WP3 之上以承载组合根接线（MU2 组装预演）
6bf7a7c fix(sync): WP3 修复轮次——标题置空走通用路径、去重键取原始路径、空 agentId 拒绝
0f70c4c feat(sync): core 派生 file.changed 与节点级 Agent 事件，会话标题单向更新（R5–R9）
```

### 冻结合同与边界核实（见 checks 的 BOUNDARY-VERIFY-6.16）

```
$ git diff --name-only 33040d78… 3b6fe41… -- crates/sync-protocol/ schemas/ docs/ vendor/
docs/CORE_PORTS_AND_STORAGE.md        ← 非空

$ git diff 33040d78… 3b6fe41… -- crates/core/Cargo.toml crates/agent-host/Cargo.toml
（空）

$ git diff --numstat 33040d78… 3b6fe41… -- docs/ crates/sync-protocol/ schemas/ vendor/ crates/core/Cargo.toml crates/agent-host/Cargo.toml
17	1	docs/CORE_PORTS_AND_STORAGE.md
```

- `crates/sync-protocol/`、`schemas/`、`vendor/` **空**；两个 `Cargo.toml` **空** → 契约面与依赖面未被触碰。
- `docs/CORE_PORTS_AND_STORAGE.md` **非空**（17+/1−），由 WP3 的 `0f70c4c` 与 `6bf7a7c` 引入，内容为新增 `NodeEventSink` 类型、节点级事件通道说明，以及 `StateChange::Update` 两层可选 `title` 的生产者说明（与 WP3 代码同提交）。**按字面判据触发越界**，但按 plan.md:1058「权威文档更新…与代码同提交」的登记，属 `check:drift` 的对齐面。**本 merger 不自行裁决**，上报 main。

## 2. 快进前 premerge 门禁（6.21）

在**候选工作区** `.worktrees/integ`（HEAD=3b6fe41，`git status --porcelain` 为空）执行，并显式传 `--planning-root D:/Project/acp-remote`（权威规划根）：

```
$ cd D:\Project\acp-remote\.worktrees\integ
$ npx --quiet --no-install openspec-agentic workflow check --change "sync-scope-and-pwa-client" --stage premerge --planning-root D:/Project/acp-remote --json

{
  "result": "BLOCKED",
  "stage": "premerge",
  "contractDigest": "sha256:e4cc671c11eeac9f64cf259271ca7821a896a9a25fcbbbda4d4d16fb7e896aa5",
  "requirementsDigest": "sha256:c2eb41a848e4e9bd24f4e03550709fe3a44c560574fe45c4bbf040fb34cf2752",
  "targetCommit": null,
  "evidence": [],
  "errors": [
    "需要唯一的 agentic-premerge 代码块"
  ],
  "boundary": "仅检查结构、引用和版本；不证明用户批准、角色独立性或测试真实性。",
  "planningDigest": "plan-v2:sha256:4e2fb347b974d0e8b6d347afec7cbd50591524f79ae7b33cf0d98ced853c88e1",
  "candidateCommit": null
}
（exit 2）
```

**根因（机械核实）**：

```
$ grep -c '```agentic-premerge' openspec/changes/sync-scope-and-pwa-client/verification.md
0
```

权威 `verification.md` 内 **0 个** `agentic-premerge` 块（仅 `reports/receipt-mu1a.md`、`reports/receipt-mu1b.md` 两份历史 receipt 含该围栏，而引擎**只读权威 verification.md、不读 `reports/`**——见 MU1b 报告对 `workflow-check.mjs:1487` 的定位）。`targetCommit`/`candidateCommit` 均为 null，说明门禁在解析到候选提交前即因缺块判 BLOCKED。**属证据/登记面缺口，非候选内容缺陷**：`contractDigest`/`requirementsDigest`/`planningDigest` 三项摘要均正常产出且与历史一致。

> 调用方式复核：在**候选工作区**内跑并显式传 `--planning-root` 正确（MU1b 已证实主检出内跑会因 target==HEAD 恒报且不可能 PASS）。本轮未在中错误位置跑；同时实测 `.worktrees/tp1`（HEAD 停在 33040d7，非候选）亦同报缺块，作为旁证。

## 3. 快进主分支（未执行）

`git -C D:\Project\acp-remote merge --ff-only integ/mu2-wiring` **未执行**。理由：调度口令明确「非 PASS 则不合入」「若 MU2 单元因 TP2 缺失而报未就绪…如实报告并停止，不要合入」。两个独立前置均不成立（§2 门禁、§4 成员完整性）。`refs/heads/main` 未移动，仍为 `33040d78324ff51be49219fcb3aac054d0100cc9`。

## 4. 主分支回归（未执行）

`npm run verify` 与 `cargo test --locked -p app --test node_link_e2e` **未执行**（依赖合入）。因此 **未创建 `reports/PV1-main-mu2.log`**：该文件按 tasks 6.22 应装合入后 PV1 的完整真实输出，本轮无合入，不以任何非真实内容占位。**未占用任何 `CARGO_TARGET_DIR`**。

### 单元成员完整性（BLOCKER-2）

plan.md:971 登记 MU2 成员 = **WP3, WP4, TP2**：

```
| MU2 / independent | WP3, WP4, TP2 | coding-3, coding-4 / testing-2 / review-3 / merger | … |
```

TP2 四方均无交付痕迹：

```
$ grep -c TP2 …/dispatch-queue.jsonl    → 0
$ grep -c TP2 …/dispatch-runtime.jsonl  → 0
$ ls …/reports/ | grep -i tp2           → （无）
$ grep '^| TP2 ' …/verification.md      → | TP2 | NOT_APPLICABLE（未开工） | testing-2 | R5–R9 的行为判定 | …
```

MU2 尚缺 TP2，**单元不完整**，即便 premerge 缺块被补齐也不到合入时机。（`verification.md:141/147` 另记 WP3→TP2 的跨条目顺序依赖：`crates/storage-sqlite/tests/` 的 Merge Order 实为 WP3 → TP2，须在 TP2 组装时复核——本轮候选已按 WP3 先行携带该目录连带编辑，属候选既定内容。）

## 5. 合并相对候选的新增差异（未执行）

无合入，无差异可复检。预置事实：合入前 main tree = `444997410bf584473fab478f43db3ea897eeb469`，候选 tree = `abfa4788e7a64939caa0507b7e93cd85c4bf7d62`。后续若以 `--ff-only` 快进，预期新增差异为零（纯快进、无合并提交），可按 MU1b 的口径复检。

## 6. 结论与交 main 处置

**BLOCKED —— 未快进、未合入、未跑主分支回归。** 两项独立 BLOCKER：

1. **premerge 门禁 BLOCKED**（§2）：权威 `verification.md` 缺唯一 `agentic-premerge` 块（exit 2）。**处置方 = main**：待 MU2 三包（WP3/WP4/**TP2**）台账就绪、Handoff Index 的 DELIVERY 绑定齐备后，由 main 在 `verification.md` 置入携带 `delivery_unit: MU2` 的唯一 `agentic-premerge` 块（含 `target_commit=33040d7…`、`candidate_commit=3b6fe41…`、`contract_digest=sha256:e4cc671c…`、与该单元 TP2 对齐的 `basic_checks`），届时本 merger 复跑门禁。
2. **MU2 成员不完整**（§4）：TP2 未派发/未交付；`testing-2` 尚无 queue/runtime/report/verification 任何记录。**当前绝非 MU2 合入时机。**

另 **1 项待裁决观测**（§1 边界核实）：候选改了 `docs/CORE_PORTS_AND_STORAGE.md`（17+/1−）。冻结合同本体（`schemas/`、`crates/sync-protocol/`）与依赖面（两个 `Cargo.toml`）**未被触碰**；docs 改动为新增 `NodeEventSink` 与 WP3 生产者的同提交文档。请 main 裁决 docs 是否列入「冻结边界」，并决定是否需以独立检视覆盖该文档改动。

**未做**：未 push、未发布、未归档、未回滚；未切换分支/reset/rebase/clean；未创建或删除任何分支或 worktree；未改 `verification.md` 或任何规划文件；未改写候选任何内容；未重跑候选 PV1/PV2（不在本轮范围）。

**第一步 dispatch**（见 checks 的 DISPATCH-ACK）：`dispatch --wp WP3 --executor merger-mu2-1 --ack` 输出 `接收确认必须绑定当前活跃工作包及其实际执行者`、exit 1——WP3 现为 `ready-to-merge`、活跃执行者为 `review-w3-r2`，merger 接管不产生新台账轮次故无法 ack（与 MU1a/MU1b 同型）。**按派发指示未改动台账状态**。
