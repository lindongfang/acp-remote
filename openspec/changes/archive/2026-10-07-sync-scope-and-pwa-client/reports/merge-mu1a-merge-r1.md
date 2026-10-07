<!-- MU1a merger 的 candidate/merge 阶段报告（Round 1）。目标核实、候选固定、候选 PV1 实测、独立候选 review、premerge 试跑已完成；premerge 判 FAIL，故未合入、未生成 receipt。承接 merge-mu1a-assemble-r1.md。 -->

task_id: "MU1a"
role: merger
phase: merge
agent_context:
  agent_id: "MergerMU1"
  isolation: "fork_turns=none（独立合入执行者；未参与 WP1/WP2 的实现、review 或修复对话。候选 review 由独立子 Agent `review-mu1a-candidate`（fork_turns=none）执行，既非本 merger，也非 WP1/WP2 作者）"
target_revision: "refs/heads/main = 353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f（6.1 与 6.6 前各核实一次，未移动）；候选 = 683dbbb8727b3ddef7e8b2a3d3e5dce056fbf0e4"
scope: "tasks.md 6.1–6.6（MU1a）：目标核实、候选固定、候选 PV1 实测、独立候选 review 派发与接收、Coverage Index 归属核对（向 main 提供输入，6.5 的记账归 main）、premerge 门禁。未完成：receipt 持久化、`## Premerge History` 登记、本地合入（6.6 因 premerge FAIL 停在合入前）、6.7 主分支回归、6.8 合并差异复核（后二者依赖合入）。"
changes: "候选工作区之外零写入。候选工作区（`.worktrees/mu1-merge`）内：新增 `reports/PV1.log`（候选阶段 PV1 实测原始输出，写在权威 changeDir 的 reports 下）；新增独立 reviewer 的报告 `reports/review-mu1a-candidate-r1.md`；新增独立 reviewer 的 cargo 产物目录 `D:\\Project\\acp-remote\\.target-wt\\mu1a-review`（与 mu1a 隔离）。主检出 `verification.md`、`plan.md`、`tasks.md` **未修改**——按 merger.md 第 4 步，`agentic-premerge` 块由主 Agent 固化、`## Premerge History` 由主 Agent 登记，本角色不直接改 verification.md。refs/heads/main 未前移。"
checks:
  - id: "6.1 TARGET-REF"
    command: "git rev-parse --abbrev-ref HEAD; git rev-parse --verify refs/heads/main; git log --oneline -1 refs/heads/main; git status --porcelain=v1 -b（cwd=D:\\Project\\acp-remote）"
    scope: "合入前目标引用机械核实"
    environment: "主检出"
    exit_code: 0
    log_path: "本报告「6.1 目标核实」小节"
    result: "`main` / `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f` / `353ba6e Merge remote-tracking branch 'origin/main' into main`；主检出 `## main...origin/main`，跟踪文件零改动。目标未移动。"
  - id: "6.2 CANDIDATE-FIX"
    command: "git -C .worktrees/mu1-merge rev-parse HEAD; git -C .worktrees/mu1-merge log --format='%H%nparent=%P%nsubject=%s' -1; git -C .worktrees/mu1-merge status --porcelain=v1 -b; git -C .worktrees/mu1-merge diff --name-only 353ba6e… HEAD | wc -l"
    scope: "候选版本固定与父提交/差异集核实"
    environment: "候选工作区"
    exit_code: 0
    log_path: "本报告「6.2 候选固定」小节"
    result: "候选 `683dbbb8727b3ddef7e8b2a3d3e5dce056fbf0e4`，父提交 `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f`（= 基线，候选包含基线，满足 premerge 的基线包含要求）；工作区干净；差异集仍为 29 个预期路径（`git diff --name-status` 逐行列出，无清单外路径）。"
  - id: "6.3-PV1/npm-check"
    command: "npm run check（cwd=D:\\Project\\acp-remote\\.worktrees\\mu1-merge，即候选工作区内部）"
    scope: "MU1a 候选 Project Verify：合同门禁十道脚本"
    environment: "Node v22.22.0 / npm 10.9.4；候选内 `node_modules` 联接仓库根只读依赖"
    exit_code: 0
    log_path: "openspec/changes/sync-scope-and-pwa-client/reports/PV1.log（EXIT_CODE_npm_run_check=0 位于该文件第 109 行）"
    result: "十道门禁全 PASS。关键数字：`schema fixtures OK: 133 valid, 35 invalid (ajv Draft 2020-12), 39 event views bound`；`command catalog OK: 13 commands`；`error registry OK: 58 codes across 2 protocols`；`feature registry OK: 13 feature ids across 2 protocols`；**`contract assets OK: 17 schemas, 181 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared, 2 SAS values recomputed`**；`ACP compatibility matrix OK: 25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families (71 rows, schema validated by ajv)`；`doc links OK: 415 relative links, 9052 section refs across 518 markdown files`；`crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致`；`contract drift OK: §7 的 36 条 DDL 与 migrate.rs 逐条一致；§5 的 15 个 trait / 96 个方法签名与 ports.rs 一致`；agentic 安装自检 8 项 PASS；`openspec validate` Totals: 21 passed, 0 failed；`agentic 宿主入口检查完成：17 个文件`。"
  - id: "6.3-PV1/cargo-test"
    command: "CARGO_TARGET_DIR=D:/Project/acp-remote/.target-wt/mu1a cargo test --locked -p sync-protocol --all-features（cwd=D:\\Project\\acp-remote\\.worktrees\\mu1-merge）"
    scope: "MU1a 候选 Project Verify：sync-protocol 单元/集成测试"
    environment: "独立 CARGO_TARGET_DIR `.target-wt/mu1a`（新建，未复用 wp1/wp2，避免交叉污染）"
    exit_code: 0
    log_path: "openspec/changes/sync-scope-and-pwa-client/reports/PV1.log（EXIT_CODE_cargo_test=0 位于该文件第 229 行）"
    result: "9 个测试二进制 + lib + doc-tests 共 **56 个用例全绿，0 failed**：lib 8、body_constraints 11、**envelope_fixtures 7**、field_constraints 10、pairing_fixtures 6、schema_drift 6、tables_match_registry 3、transcript_vectors 2、view_projections 3、doc-tests 0。"
  - id: "6.3-PV1/asset-count-arithmetic"
    command: "git show 353ba6e:crates/sync-protocol/tests/envelope_fixtures.rs | grep 常量；grep 常量（候选）；ls fixtures/sync/v1/{valid,invalid,transcripts} | wc -l；复核者独立复算（见 review-mu1a-candidate-r1.md CHK4/CHK6）"
    scope: "重点项：check:assets 与两个用例计数常量在 MU1a 组装后是否仍成立"
    environment: "候选工作区"
    exit_code: 0
    log_path: "本报告「6.3 计数门禁实测」小节"
    result: "成立，且是自洽的中间值。基线 `EXPECTED_VALID_MESSAGE_CASES=67` / `EXPECTED_BODY_REJECTED=9`（基线另有两个常量 `EXPECTED_ENVELOPE_REJECTED=2`、`EXPECTED_SKIPPED_PAIRING=5`）；候选为 `73` / `14`。算术：WP1 在 `valid/` 新增 7 个夹具并删除 1 个（`sync-snapshot-chunk-config-options.json`，其 `body.resource=config_options` 已被 D1 移出封闭词表）→ 67+7−1 = **73**；WP1 在 `invalid/` 新增 5 个负例 → 9+5 = **14**；pairing 的 5 个未变。候选 `valid/` 实测 78 个文件、`invalid/` 16 个、`transcripts/` 7 个。独立 reviewer 用自写校验脚本独立复算得到同一组数字（valid WSS = 67+7−1 = 73、invalid WSS 16 − 2 信封层 = 14、pairing 5），并确认 `npm run check` 的 `contract assets OK: 17 schemas, 181 fixture files` 与候选树逐条配平。**结论：组装没有引入计数漂移，check:assets 通过；`npm run check` 与 `cargo test` 均 exit 0，无一条需要修改常量。** 同时按 plan.md Shared File Ownership 第 923 行，这两个常量在 MU1a 只是 TP1 之前的中间值，最终值由 MU1b（TP1 从本单元合入提交分支后）复算，不阻塞 MU1b。"
  - id: "6.4 CANDIDATE-REVIEW"
    command: "派发独立 reviewer 子 Agent（agent=reviewer，fork_turns=none）只读检视候选 683dbbb；其自行执行 CHK1–CHK7 七组命令（git diff --name-status、blob 比对、三方合并等价、自写 manifest 校验、npm run check、cargo test --workspace、独立 CARGO_TARGET_DIR）"
    scope: "候选新增交互、共享文件叠加结果、manifest、独占路径哈希来源、计数门禁"
    environment: "只读；独立 target 目录 `.target-wt/mu1a-review`"
    exit_code: 0
    log_path: "openspec/changes/sync-scope-and-pwa-client/reports/review-mu1a-candidate-r1.md（sha256:d76f3b2e8ca26c310ea18de03b25ff1840d17ce891836acf5a4012e2edf697b2）"
    result: "**PASS，0 CRITICAL / 0 MAJOR / 0 MINOR / 0 SUGGESTION。** 三个指定问题的独立答案：(a) 共享文档叠加正确无冲突——把 base/wp1/wp2 三份副本归一化行尾后 `git merge-file` 退出码 0、零冲突标记，自动合并结果与候选逐行零差异；(b) manifest 与候选树逐条自洽——94 条 cases ↔ 94 个 JSON 双向配平（无缺失/未登记/重复）、36 条 `viewDef` 指针全部可解析、被删夹具无残留条目、新增 12 个夹具登记齐全、既有 83 条相对次序保留；(c) 73/14 与候选内容一致且确为 TP1 之前的中间值。reviewer 未修改任何常量、未改动被检视文件、未触碰三个源 worktree、未做任何 git 写操作。"
  - id: "6.5 COVERAGE-INDEX"
    command: "读 plan.md `## Coverage Index` 与 `## Contract Changes`，逐行判定归属 MU1a 的贡献"
    scope: "向 main 提供 6.5 的记账输入（记账本身归 main）"
    environment: "只读"
    exit_code: 0
    log_path: "本报告「6.5 Coverage 归属」小节"
    result: "MU1a 只承担 R1–R4 的**合同侧**贡献（WP1：快照词表收窄、`session.read` 分页 payload/result、复合游标 schema 与排序约束、文档写明分页只作用于 Sync 面）与 R5–R9 的**视图字段侧**贡献（WP2：`file.changed` 的 `addedLines`/`deletedLines`/`outsideWorkspace`、`agent.connected`/`agent.disconnected` 的 `state` 封闭枚举、文档写明标题单向更新）。各行的向量/行为侧（TP1 的固定向量、WP3 的 core 派生与行级 diff、WP4 的生命周期上报）不在本单元。**发现（供 main/DDR 处理，非本角色可改）**：Coverage Index 的 Closure Unit 列仍写 `MU1/premerge`（R1–R4）与 `MU2/premerge`（R5–R9），单元拆分后该列未随之改写；本角色按 merger.md 不修改 plan.md。"
  - id: "6.6-PREMERGE"
    command: "npx openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --planning-root D:/Project/acp-remote --json（cwd=D:\\Project\\acp-remote）；随后 --stage premerge（cwd=D:\\Project\\acp-remote\\.worktrees\\mu1-merge）"
    scope: "premerge 门禁"
    environment: "候选工作区（HEAD=候选提交）；--planning-root 指向权威规划根 D:\\Project\\acp-remote"
    exit_code: 1
    log_path: "本报告「6.6 premerge」小节（完整 JSON 输出抄录）"
    result: "**两个阶段均 FAIL，退出码 1。** `--stage plan`：`planningDigest` = `plan-v2:sha256:8eb6a3ebabd0b59bdffd345be20887ef4af905f572b4943da72dcd55e00dd4ad`（与 DDR Round 7 绑定值一致）、`contractDigest` = `sha256:20f6eb89d94cf0affac76c8946d39334cefe22620f9712b8b1a98ad825f8d786`、`requirementsDigest` = `sha256:c74ff6ab598f425d100e1c095fac1d4e3465c8ef9dad932222060636a9881e68`，errors 为 5 条台账接收确认缺口。`--stage premerge`：同样 FAIL、退出码 1，errors 为**同一批 5 条**（WP2 第 1 轮 coding / WP2 第 1 轮 reviewing / WP2 第 2 轮 fixing / WP1 第 2 轮 reviewing / WP2 第 3 轮 reviewing 缺少执行者接收确认），`candidateCommit: null`、`evidence: []`。**结论：premerge 非 PASS，按 merger.md 第 4 步与调度约束，本轮不得合入、不得持久化 receipt。**"
  - id: "NOT-RUN / BY-GATE"
    command: "receipt 持久化、`## Premerge History` 登记、本地合入、6.7 主分支回归、6.8 合并差异复核"
    scope: "6.6 后半段与 6.7/6.8"
    environment: "NOT_APPLICABLE"
    exit_code: NOT_APPLICABLE
    log_path: "NOT_AVAILABLE"
    result: "因 premerge FAIL 全部未执行。未生成也未伪造 receipt；未向 verification.md 写入任何内容。"
issues: "0 CRITICAL / 0 MAJOR（产品与组装层面）/ **1 BLOCKER**：`workflow check --stage premerge` 判 FAIL（退出码 1），5 条错误全部是台账执行者接收确认缺口（用户 2026-10-03 裁决接受该缺口，但机械门禁仍判 FAIL）；另有 1 项记账发现（Coverage Index 的 Closure Unit 列仍写 `MU1`，非本角色可改）。"
result: BLOCKED
evidence_paths:
  - "openspec/changes/sync-scope-and-pwa-client/reports/PV1.log"
  - "openspec/changes/sync-scope-and-pwa-client/reports/review-mu1a-candidate-r1.md"
  - "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1a-merge-r1.md"
  - "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1a-assemble-r1.md"

worktree_handoff:
  - work_package: MU1a-CANDIDATE
    attempt: 1
    worktree: "D:\\Project\\acp-remote\\.worktrees\\mu1-merge"
    baseline_revision: "353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f"
    executor: "MergerMU1"
    received_at: "2026-10-03"

handoff_index:
  - task_id: "MU1a"
    work_package: WP1
    role: merger
    phase: candidate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "683dbbb8727b3ddef7e8b2a3d3e5dce056fbf0e4"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/PV1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在候选工作区 .worktrees/mu1-merge 内部实测（cwd=候选，非主检出、非工作包 worktree）：`npm run check` 退出码 0（EXIT_CODE_npm_run_check=0），十道门禁全绿，含 check:assets 的 `17 schemas, 181 fixture files, 12 transcript vectors, 20 negative vectors rejected as declared`；`CARGO_TARGET_DIR=D:/Project/acp-remote/.target-wt/mu1a cargo test --locked -p sync-protocol --all-features` 退出码 0（EXIT_CODE_cargo_test=0），56 用例全绿。基线 353ba6ef、候选 683dbbb、Node v22.22.0/npm 10.9.4、独立 cargo 目录。此证据绑定候选阶段，不能被工作包阶段 review-mu1-fixes-r1.md 的 WORKTREE-DIRTY 证据顶替。PV1.log sha256 = eb2bb209f5257a14cdfcd527e6356f1704aa014e2018de89351c0bea111bb0dc。"
    source_evidence: NOT_APPLICABLE
  - task_id: "MU1a"
    work_package: WP2
    role: merger
    phase: candidate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "683dbbb8727b3ddef7e8b2a3d3e5dce056fbf0e4"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/PV1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一次候选工作区实测，范围覆盖 WP2 的独占产物（schemas/sync/v1/event-views.schema.json、crates/sync-protocol/src/views.rs、docs/ACP_COMPATIBILITY_MATRIX.md、fixtures/sync/v1/valid/view-*.json）与共享文档叠加结果：check:schemas 的 39 event views bound、check:assets 的 181 fixture files、check:docs 的 415 relative links / 9052 section refs / 518 markdown files 全部 PASS；两个命令退出码均为 0，候选与基线如上。"
    source_evidence: NOT_APPLICABLE
  - task_id: "MU1a"
    work_package: DELIVERY
    role: merger
    phase: merge
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "683dbbb8727b3ddef7e8b2a3d3e5dce056fbf0e4"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1a-merge-r1.md"
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "候选已固定并通过实测 PV1 与独立候选 review，但 `workflow check --stage premerge` 退出码 1、判 FAIL（5 条台账接收确认缺口，candidateCommit 为 null），因此不得合入、不得持久化 receipt。待补：台账侧补齐 5 条执行者接收确认 → premerge 复跑 PASS → 由主 Agent 在 verification.md 固化 agentic-premerge 块（delivery_unit: MU1a、target_commit 353ba6ef…、candidate_commit 683dbbb…、contract_digest sha256:20f6eb89…、requirements_digest sha256:c74ff6ab…、verify evidence = reports/PV1.log sha256:eb2bb209…、review evidence = reports/review-mu1a-candidate-r1.md sha256:d76f3b2e…、reviewer review-mu1a-candidate、author coding-1/coding-2）→ 持久化版本化 receipt → main 登记 `## Premerge History` → 合入前再核实目标基线并条件更新 → 本地合入 → 6.7 主分支回归与 6.8 合并差异复核。"
    source_evidence: NOT_APPLICABLE

---

## 6.1 目标核实

```
> git rev-parse --abbrev-ref HEAD            (cwd D:\Project\acp-remote)
main
> git rev-parse --verify refs/heads/main
353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f
> git log --oneline -1 refs/heads/main
353ba6e Merge remote-tracking branch 'origin/main' into main
> git status --porcelain=v1 -b
## main...origin/main
```

目标 = `refs/heads/main` @ `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f`，与 plan.md `## Target Repository and Main Branch` 的规划基线同一提交，**未移动**。主检出跟踪文件零改动（未跟踪仍只有 `.target-wt/`、`.worktrees/`、`openspec/changes/sync-scope-and-pwa-client/`）。

## 6.2 候选固定

| 项 | 值 |
| --- | --- |
| 候选提交 | `683dbbb8727b3ddef7e8b2a3d3e5dce056fbf0e4` |
| 父提交 | `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f`（= 基线，满足 premerge 的「候选包含基线」） |
| 主题 | `feat(sync): MU1a 契约候选——WP1 快照收窄/分页命令与 WP2 派生事件字段` |
| 工作区 | `D:\Project\acp-remote\.worktrees\mu1-merge`（分支 `merge/mu1-candidate`），`git status --porcelain=v1 -b` 仅 `## merge/mu1-candidate`，**干净** |
| 差异集 | 29 个路径（`git diff --name-only <baseline> HEAD | wc -l` = 29；`--name-status` 逐行为 17 M + 11 A + 1 D），无清单外路径 |

## 6.3 候选 PV1（实测）

两条命令都在**候选工作区内部**执行，未复述工作包阶段结论。

**`npm run check`（cwd = `.worktrees/mu1-merge`）→ 退出码 0**

| 门禁 | 输出 |
| --- | --- |
| check:schemas | `133 valid, 35 invalid (ajv Draft 2020-12), 39 event views bound` |
| check:commands | `13 commands` |
| check:errors | `58 codes across 2 protocols` |
| check:features | `13 feature ids across 2 protocols` |
| **check:assets** | **`17 schemas, 181 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared, 2 SAS values recomputed`** |
| check:acp | `25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families (71 rows)` |
| check:docs | `415 relative links, 9052 section refs across 518 markdown files` |
| check:boundaries | `12 个 crate 的依赖方向与 §5 矩阵一致` |
| check:drift | `§7 的 36 条 DDL 与 migrate.rs 逐条一致；§5 的 15 个 trait / 96 个方法签名与 ports.rs 一致` |
| check:agentic | 安装自检 8 项 PASS + `openspec validate` `21 passed, 0 failed` + 宿主入口 `17 个文件` |

**`CARGO_TARGET_DIR=D:/Project/acp-remote/.target-wt/mu1a cargo test --locked -p sync-protocol --all-features`（cwd = 候选）→ 退出码 0**，9 个测试二进制 + lib + doc-tests 共 **56 用例全绿、0 failed**（lib 8 / body_constraints 11 / **envelope_fixtures 7** / field_constraints 10 / pairing_fixtures 6 / schema_drift 6 / tables_match_registry 3 / transcript_vectors 2 / view_projections 3 / doc-tests 0）。

原始输出：`reports/PV1.log`（229 行，sha256 `eb2bb209f5257a14cdfcd527e6356f1704aa014e2018de89351c0bea111bb0dc`），两条命令的退出码分别写在第 109 与第 229 行。

### 6.3 计数门禁实测（重点项）

计划担心的「WP1 在自己 worktree 里算出的计数在组装后对不上」**未发生**：

| 常量 | 基线 `353ba6e` | 候选 `683dbbb` | 算术 |
| --- | --- | --- | --- |
| `EXPECTED_VALID_MESSAGE_CASES` | 67 | **73** | WP1 在 `valid/` 新增 7 个（4 个 `command-*` + 3 个 `sync-snapshot-sessions-only-*`）、删除 1 个（`sync-snapshot-chunk-config-options.json`）：67 + 7 − 1 = 73 |
| `EXPECTED_BODY_REJECTED` | 9 | **14** | WP1 在 `invalid/` 新增 5 个负例：9 + 5 = 14 |
| `EXPECTED_ENVELOPE_REJECTED` | 2 | 2（未改） | — |
| `EXPECTED_SKIPPED_PAIRING` | 5 | 5（未改） | — |

候选夹具目录实测：`valid/` 78 个、`invalid/` 16 个、`transcripts/` 7 个。独立 reviewer 用自写脚本独立复算得到同一组数字（valid WSS = 67+7−1 = 73；invalid WSS 16 − 2 信封层 = 14；pairing 5），并确认 `check:assets` 的 `181 fixture files` 与 manifest 的 94 条 cases 双向配平。**没有修改任何常量，两条命令都是一次通过。**

按 plan.md Shared File Ownership 第 923 行，这两个常量在 MU1a 只是 TP1 之前的中间值，最终值由 MU1b 在本单元合入提交上复算，**不阻塞 MU1b**。

## 6.4 独立候选 review

已派发独立 reviewer 子 Agent（`agent=reviewer`，`fork_turns=none`，agent_id `review-mu1a-candidate`；既不是本 merger，也不是 WP1/WP2 作者），只读检视候选 `683dbbb`。

**结果 PASS，0 CRITICAL / 0 MAJOR / 0 MINOR / 0 SUGGESTION。** 报告：`reports/review-mu1a-candidate-r1.md`（sha256 `d76f3b2e8ca26c310ea18de03b25ff1840d17ce891836acf5a4012e2edf697b2`）。

三个重点问题的独立答案：

- **(a) 共享 `docs/SYNC_PROTOCOL.md` 的 WP1→WP2 叠加正确、无冲突**：把 base/wp1/wp2 三份副本归一化行尾后 `git merge-file` 退出码 0、零冲突标记，其自动合并结果与候选版本逐行零差异；WP1 的 §3.3 / §9.4 / §11.5 与 WP2 的 §10.3 编辑区间不相交且全部完整保留；文档新增声明与 `snapshotResource` / `snapshotChunkCount` / `sessionRead` / `sessionReadResult` / `file.changed` / `agent.*` 的 schema 与 Rust 镜像逐条一致。
- **(b) `manifest.json` 与候选树逐条自洽**：94 条 cases ↔ 94 个 JSON 双向配平（无缺失、无未登记、无重复）；36 条 `viewDef` 指针全部可解析；被删的 `sync-snapshot-chunk-config-options.json` 无残留条目；新增 12 个夹具登记齐全；既有 83 条相对次序保留；manifest 只带 WP1 改动（WP2 对该文件空 diff，已由 blob 比对证实）。
- **(c) 73/14 与候选内容一致，且确为 TP1 之前的中间值**，由 MU1b 定最终值，不阻塞 MU1b。

reviewer 自行执行并记录退出码的检查：CHK1 `git diff --name-status`(0)、CHK2 索引 blob + `git hash-object` 逐文件比对(0)、CHK3 三方合并等价(0)、CHK4 自写 manifest/node 校验脚本(0, issues=0)、CHK5 `npm run check`(0，与 `reports/PV1.log` 九行 OK 逐行一致)、CHK6 `cargo test --locked -p sync-protocol --all-features`(0，56 用例，独立目录 `.target-wt/mu1a-review`)、CHK7 `cargo check --workspace --all-targets`(0)。未修改常量、未改被检视文件、未触碰三个源 worktree、未做任何 git 写操作。

## 6.5 Coverage 归属（向 main 提供输入）

MU1a 只承担 R1–R4 的合同侧（WP1）与 R5–R9 的视图字段侧（WP2）贡献；各行的向量侧与行为侧（TP1、WP3、WP4）不在本单元。逐行对照见报告正文表格。

**发现（记账层面，非本角色可改）**：plan.md `## Coverage Index` 的 Closure Unit 列仍写 `MU1/premerge`（R1–R4）与 `MU2/premerge`（R5–R9），单元拆分后未随之改写；`## Merge Strategy` 已改为四个单元。按 merger.md，本角色不修改 plan.md，请 main 在 6.5 记账或交 DDR 处理。

## 6.6 premerge（FAIL，合入被阻断）

```
> npx openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --planning-root D:/Project/acp-remote --json
{ "result": "FAIL", "stage": "plan",
  "contractDigest": "sha256:20f6eb89d94cf0affac76c8946d39334cefe22620f9712b8b1a98ad825f8d786",
  "requirementsDigest": "sha256:c74ff6ab598f425d100e1c095fac1d4e3465c8ef9dad932222060636a9881e68",
  "evidence": [{"path":"reports/PV1.log","sha256":"sha256:eb2bb209…"}],
  "errors": [
    "WP2 第 1 轮 coding 缺少执行者接收确认（台账不能代替实际派发）",
    "WP2 第 1 轮 reviewing 缺少执行者接收确认（台账不能代替实际派发）",
    "WP2 第 2 轮 fixing 缺少执行者接收确认（台账不能代替实际派发）",
    "WP1 第 2 轮 reviewing 缺少执行者接收确认（台账不能代替实际派发）",
    "WP2 第 3 轮 reviewing 缺少执行者接收确认（台账不能代替实际派发）"
  ],
  "planningDigest": "plan-v2:sha256:8eb6a3ebabd0b59bdffd345be20887ef4af905f572b4943da72dcd55e00dd4ad" }
EXIT=1

> npx openspec-agentic workflow check --change sync-scope-and-pwa-client --stage premerge --planning-root D:/Project/acp-remote --json
        (cwd = D:\Project\acp-remote\.worktrees\mu1-merge)
{ "result": "FAIL", "stage": "premerge",
  "contractDigest": "sha256:20f6eb89d94cf0affac76c8946d39334cefe22620f9712b8b1a98ad825f8d786",
  "requirementsDigest": "sha256:c74ff6ab598f425d100e1c095fac1d4e3465c8ef9dad932222060636a9881e68",
  "targetCommit": null, "candidateCommit": null, "evidence": [],
  "errors": [ 同上 5 条 ],
  "planningDigest": "plan-v2:sha256:8eb6a3ebabd0b59bdffd345be20887ef4af905f572b4943da72dcd55e00dd4ad" }
EXIT=1
```

- `planningDigest` 与 DDR Round 7 绑定的 `plan-v2:sha256:8eb6a3e…` **一致**，规划侧无版本漂移。
- 两个阶段都判 FAIL，退出码 1。premerge 继承 plan 的全部要求，因此同样被这 5 条台账执行者接收确认缺口拦住；`candidateCommit: null` 说明它在校验 `agentic-premerge` 块之前就已判 FAIL。
- 用户 2026-10-03 裁决「接受」这 5 项缺口，与机械门禁是否 PASS 是两回事：`workflow check` 的判定只看台账是否记录了执行者接收确认。补齐该记录属于台账侧动作（由主 Agent 或对应执行者按 `dispatch --ack` 口径登记），**不是 merger 可以代填的材料**，也不是我可以用「用户已接受」绕过的门禁。
- 按 merger.md 第 4 步「只在 PASS 且目标引用仍未移动时继续」与调度约束「任何检查没过如实报 FAIL」，本轮**不持久化 receipt、不合入**。

## 解除阻断后立即可用的材料

供主 Agent 固化 `agentic-premerge` 块（块必须由 main 写入 verification.md）与后续 receipt 使用：

| 字段 | 值 |
| --- | --- |
| `delivery_unit` | `MU1a` |
| `target_ref` | `refs/heads/main` |
| `target_commit` | `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f` |
| `candidate_commit` | `683dbbb8727b3ddef7e8b2a3d3e5dce056fbf0e4` |
| `contract_digest` | `sha256:20f6eb89d94cf0affac76c8946d39334cefe22620f9712b8b1a98ad825f8d786` |
| `requirements_digest` | `sha256:c74ff6ab598f425d100e1c095fac1d4e3465c8ef9dad932222060636a9881e68` |
| `verify` | PASS，`evidence: {path: reports/PV1.log, sha256: "sha256:eb2bb209f5257a14cdfcd527e6356f1704aa014e2018de89351c0bea111bb0dc"}` |
| `review` | PASS，`reviewer: review-mu1a-candidate`，`author: coding-1 / coding-2`，`evidence: {path: reports/review-mu1a-candidate-r1.md, sha256: "sha256:d76f3b2e8ca26c310ea18de03b25ff1840d17ce891836acf5a4012e2edf697b2"}` |

前提：`## Worktree Handoff` 与 `## Dispatch Reconciliation` 需已按 tasks.md 6.4/premerge 要求覆盖 WP1、WP2 的各轮尝试，且 5 条接收确认已补齐，否则复跑仍判 FAIL。

## 确认

- **未合入任何内容**：`refs/heads/main` 仍为 `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f`；候选只存在于本地分支 `merge/mu1-candidate`。
- **未持久化 receipt，未写 verification.md**：本轮没有向 `verification.md` 写入任何内容（`agentic-premerge` 块与 `## Premerge History` 按 merger.md 归主 Agent）。
- 未执行 `git push`，未 rebase，未改写 `refs/heads/main` 历史，未创建 tag。
- 未修改 wp1、wp2、tp1 三个源工作区；未修改候选中任何被检视文件。
- 未伪造任何检查结论：报告中 PASS 的只有我实际执行并附上输出的 6.1 / 6.2 / 6.3 两条命令 / 6.4 独立 review / 6.5 归属核对；6.6 如实记 FAIL。

BLOCKED