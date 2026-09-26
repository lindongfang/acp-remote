# Verification: agent-host-oversize-exit-ordering

权威验证记录（主 Agent 维护；结构按 `openspec/schemas/agentic/templates/verification.md`）。

## Target

- Code Repository: `D:\Project\acp-remote`
- Target Ref: `refs/heads/main`
- 规划时刻背景值（非验收依据）：main = `84a8a05`（2026-09-26）。

## Handoff Index

| Task | Stage | Executor / Reviewer | Base / Target Version | Evidence Type / ID | Report | Result | Evidence Status | Applicability |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2.1 | work-package | worker 53d46466（coder，fresh，deepseek-flash） | base `998cb0f` → `af64e85` | CHECK / NOT_APPLICABLE | `reports/wp1-handoff.md` | PASS | NEW | 新顺序 = 标记→投递→terminate；diff 与报告逐字吻合（单文件 +4/−2）；D2 四条不变量逐行核实 |
| 2.2 | work-package | 同上 | 同上 | CHECK / NOT_APPLICABLE | `reports/wp1-local-checks.log` | PASS | NEW | `cargo test -p agent-host` EXIT=0（oversize_frame 通过且未修改）+ fmt + clippy 全绿 |
| 3.2 | work-package | reviewer 8fa55311（RV1，fresh 只读） | `af64e85` | REVIEW / RV1 | `reports/rv1-wp1.md` | PASS（2 MINOR 报告级） | NEW | 五关注点全过；MINOR 已订正（含 RV2 发现的第二处 56→55 残留） |
| 6.1 | candidate | worker c28d2fe2（integrator，fresh） | main `84a8a05`（两次核实未动） | DELIVERY / NOT_APPLICABLE | `reports/integrator.md`（阶段 A） | PASS | NEW | 基线核实：is-ancestor 真、0/5 可 ff-only |
| 6.2 | candidate | 同上 | 候选 `25acb00`（tree `116ae4b5`） | DELIVERY / NOT_APPLICABLE | `reports/integrator.md`（阶段 A） | PASS | NEW | 候选固定 + 构建 EXIT=0；代码面单文件 +4/−2；blob 与 `af64e85` 一致 |
| 6.4 | candidate | reviewer c632885d（RV2，fresh 只读） | `25acb00` | REVIEW / RV2 | `reports/rv2-candidate.md` | PASS（2 MINOR + 1 SUGGESTION） | NEW | 候选静态检视全过；F1/F3 已订正，F2 按「合并后簿记」流程登记；其 blob 待证项已由主 Agent 机械闭合 |
| 6.6 | main | worker c28d2fe2（integrator，阶段 B 运行 195dc2a5） | main = `25acb00`（ff） | DELIVERY / NOT_APPLICABLE | `reports/integrator.md`（阶段 B） | PASS | NEW | `--ff-only` 84a8a05→25acb00；树哈希一致；未推送 |
| 6.7 | main | 同上 | main = `25acb00` | CHECK / PV1 | `reports/main-verify.log` | PASS | NEW | `npm run verify` EXIT=0；17 道合同门禁 + 82 段 test result 全 ok；oversize_frame ok；acpr-* 0/0 |
| 6.8 | main | 主 Agent | main = `25acb00` | REVIEW / 复用 RV2 | 本行自身 + `reports/integrator.md` 阶段 B | PASS | NEW | fast-forward、零新增差异（树哈希相等、diff 为空）→ 按 tasks 6.8 完成条件引用 RV2，无需新 reviewer |

（任务交接时逐行登记。）

## Checks

### 运行时基线（任务 1.1，2026-09-26，主 Agent）

| 项 | 命令 | 结果 |
| --- | --- | --- |
| 目标引用 | `git rev-parse refs/heads/main` | `84a8a0593b2272252a458bfeb0648bc6f1a2f669` |
| 工作树 | `git status --porcelain` | 仅 `openspec/changes/agent-host-oversize-exit-ordering/`（本变更目录，预期内） |
| 实施分支 | `git switch -c feat/agent-host-oversize-exit-ordering` | 已创建于 `84a8a05` |

### 契约边界（任务 1.2）

- 增量规范：`specs/local-agent-host/spec.md`（1 条 ADDED Requirement「超限结束的失败关闭顺序」+ 2 场景）。
- 写入范围：仅 `crates/agent-host/src/process.rs` 的 `abort_agent` 函数与其注释 + 本变更目录。
- 若实施中发现需要改动其它函数/文件：先回主 Agent 更新计划（plan.md「Contract Changes」）。

### 资源安排（任务 1.3）

- PV2 三连跑窗口内不并行其它 `cargo test`（被测场景即全量并行负载本身）；`target/` 串行共享。
- 本变更不涉及数据库、容器、端口或外部账号。

### 上游依赖（任务 1.4）

- 不适用：单工作包（WP1）、首个交付单元（DU1），无代码交接；依据见 `plan.md` 的 Dependency Handoffs。

### 5.1/5.2 集成就绪（2026-09-26，主 Agent）

- 5.1：独立集成 Agent 已创建（worker `c28d2fe2`，deepseek/deepseek-flash，fresh 上下文，未参与实现/review），交接 roles/integrator.md 要点、计划、证据清单与合入条件；主 Agent 未兼任。
- 5.2：DU1 组成核对 = WP1（independent，无 TP）。证据有效性：PV1/PV2（三连跑）于 `af64e85` 执行（其后仅 openspec 簿记提交），RV1 PASS 于 `af64e85`；候选轮将按 6.3 对最终候选重跑 PV1/PV2，故当前证据有效且会被候选轮刷新。

### Project Verify 记录

| Check | 执行者/版本 | 命令 | 结果 | 证据 |
| --- | --- | --- | --- | --- |
| PV1 | 主 Agent / `af64e85`（执行时树 `e544ac9`，仅 openspec 簿记差异） | `npm run verify` | **PASS**：EXIT=0，0 FAILED | `reports/final-verify.log` |
| PV2 | 主 Agent / `af64e85`（执行时树 `e544ac9`，同上） | `cargo test --locked --workspace --all-features` 连跑 3 次（串行窗口） | **PASS**：3 轮均 EXIT=0，`oversize_frame` 每轮 ok，0 FAILED | `reports/stress-runs.log` |

## Check Plan Changes

（无。执行中如调整检查计划在此登记：原值、新值、原因、影响分析、证据失效/复用历史。）

## Dependency Handoffs

无代码交接（见「上游依赖」）。

## Runtime Resources

（PV2 连跑窗口与实际清理记录在执行时登记。）

## Review Findings

### RV1（branch / work-package，reviewer 8fa55311，fresh 只读）

- 目标版本：`af64e85`；报告：`reports/rv1-wp1.md`；结论：**PASS**（0 CRITICAL/MAJOR）。
- 确认：新顺序「标记→投递→terminate」与注释一致；D2 核实记录逐条独立复核为真；`wait_loop`/测试/签名零改动；spec 增量与实现语义一致（`mark` 先于第一个 `send`，无缝隙）。
- 2 条 MINOR（均为 `wp1-handoff.md` 行号/摘要精度）已由主 Agent 订正：56→55（已改）；`@@` 头系重建，已加注「权威 diff 以 `git diff 998cb0f..af64e85` 为准」并已机械核对（单文件 +4/−2）。
- 1 条 SUGGESTION（并发关闭介入时错误种类的理论窗口）：非新问题（旧顺序下同类二选一已存在）、不可复现、spec 未承诺错误种类——不修复，登记为后续归因参考。
- reviewer 无 shell/git 权的残余已由主 Agent 闭合：`git diff --name-status 998cb0f..af64e85` = 单代码文件 + 报告文件。

## Merge History

- **DU1 本地合入（6.6，2026-09-26，集成 Agent 195dc2a5）**：`git merge --ff-only feat/agent-host-oversize-exit-ordering`，
  main 从 `84a8a05` 快进到 `25acb00`（10 文件 +755/−2，无新提交对象）；树哈希 `116ae4b5…` 与候选逐字节一致。
  未推送（远端操作未授权）。
- **主分支回归（6.7）**：main 上 `npm run verify` EXIT=0（`reports/main-verify.log`，68137 B）。
- **合入差异审查（6.8）**：fast-forward ⇒ 零新增差异 ⇒ 复用 RV2（`reports/rv2-candidate.md`）。
- 过程事件：工作树发现第三个脏文件（`wp1-handoff.md` 的 RV2-F1 修正），集成 Agent 经 `contact_supervisor`
  请示后按「三文件对称备份→checkout→合入→写回」处理（备份 sha256 与工作树逐字节一致，无内容丢失）。

## Test Design and Authoring

不适用：Main E2E 为 not-applicable（`plan.md` 已记录降级批准），本变更不设 TP。

## Candidate E2E

NOT_APPLICABLE：模式、理由、依据与降级批准见 `plan.md` 的 Main E2E 节；替代检查为 PV1/PV2（候选轮已 PASS，见 6.3）。

### 7.1/7.2 替代验证与资源核实（2026-09-26，主 Agent，main = `25acb00`）

- 替代检查证据可读性：`reports/candidate-pv1-verify.log`、`reports/stress-runs.log`、`reports/main-verify.log`
  均可读；候选轮 PV1 PASS、PV2 三连跑全绿（复用依据 = blob 一致）；main 回归 PV1 PASS 且跑前跑后 `acpr-*` 均为 0。
- 资源清理：系统临时目录 `acpr-*` = 0；`git worktree list` = 1；无遗留 cargo/rustc/acp-remote 进程；
  仓库外三份备份已随本节记录提交后删除。

## Main E2E

NOT_APPLICABLE：同上。`[e2e-owned]` 任务 7.3 由扩展的 `e2e check` 判定。

### RV2（merge / candidate，reviewer c632885d，fresh 只读）

- 目标版本：`25acb00`；报告：`reports/rv2-candidate.md`；结论：**PASS**（0 CRITICAL/MAJOR）。
- 确认：新顺序与注释一致、spec 增量语义无缝隙（`mark` 先于第一个 `send`）、`wait_loop`/测试零改动、
  `docs/`/`compatibility/` 按 AGENTS.md §10 确实无需同步。
- 发现处置：RV2-F1（`wp1-handoff.md` 第二处 56→55 残留）已订正；RV2-F2（候选未含集成簿记）按既定
  「合并后簿记」流程处理——premerge 门读文件系统、报告于合入后随簿记提交落到 main，此处记录依据；
  RV2-F3（版本标注）已把 Project Verify 表改为「`af64e85`（执行时树 `e544ac9`）」。
- reviewer 无 shell/git 权的待补项已由主 Agent 闭合：main=`84a8a05` 未移动、is-ancestor 为真、
  `25acb00` 与 `af64e85` 的 `process.rs` blob 均为 `46fbe458…`（逐字节一致）、候选全量 diff = 单代码文件 +
  本变更目录（集成 Agent 阶段 A 已机械核验）。

### 6.3 候选轮验证（主 Agent，候选 `25acb00`，2026-09-26）

| Check | 命令 | 结果 | 证据 |
| --- | --- | --- | --- |
| PV1（候选轮） | `npm run verify` | **PASS**：EXIT=0，0 FAILED | `reports/candidate-pv1-verify.log` |
| PV2（候选轮） | `cargo test --locked --workspace --all-features` 连跑 3 次 | **PASS（REUSED）**：复用 `af64e85` 工作树的三连跑（`reports/stress-runs.log`）；适用依据 = 候选 `process.rs` blob `46fbe458…` 与 `af64e85` 逐字节一致（集成 Agent 核验、RV2 复核）、命令/环境相同、`openspec/` 文档差异不影响 cargo 测试行为 | `reports/stress-runs.log` |

### 6.5 Main E2E not-applicable 核对（主 Agent）

- `plan.md` 的 Main E2E 节：`mode: not-applicable`；`reason`/`basis`/`alternative_checks`（2 项）齐备；
  `downgrade_approval` = 「2026-09-26 用户原话：『同意降级』」可追溯（本会话）。
- 替代检查 [PV1] 候选轮真实执行 PASS、[PV2] 三连跑全绿（复用依据见上）。

### 候选证据块（合入前门禁输入；按自指约束不随候选提交）

```agentic-premerge
version: 1
delivery_unit: DU1
target_ref: refs/heads/main
target_commit: 84a8a0593b2272252a458bfeb0648bc6f1a2f669
candidate_commit: 25acb00d6895bcbe1ba36b658e0dd4706fd0347a
contract_digest: sha256:9b057653b38b79841d7fe3621ea29265a4aedba99c87449cc129f2e83ab4a8ee
verify:
  result: PASS
  candidate_commit: 25acb00d6895bcbe1ba36b658e0dd4706fd0347a
  evidence: {path: reports/candidate-pv1-verify.log, sha256: "sha256:3ba297718d74d43799c977aeca4fb925adb9114d47ef45f44f0dac4700db3a5e"}
review:
  result: PASS
  candidate_commit: 25acb00d6895bcbe1ba36b658e0dd4706fd0347a
  reviewer: RV2（fresh 只读 reviewer 子 Agent，运行 c632885d）
  author: WP1 实现子 Agent（worker 53d46466）+ 主 Agent（簿记）
  evidence: {path: reports/rv2-candidate.md, sha256: "sha256:ce87ffde381ff422166b87cddff2a12190aec192a62f4d3a8699b06a02caaf2b"}
alternative_checks:
  - name: "PV1: npm run verify（全量 Rust 测试 + 十道合同门禁 + spec 校验）"
    result: PASS
    candidate_commit: 25acb00d6895bcbe1ba36b658e0dd4706fd0347a
    evidence: {path: reports/candidate-pv1-verify.log, sha256: "sha256:3ba297718d74d43799c977aeca4fb925adb9114d47ef45f44f0dac4700db3a5e"}
  - name: "PV2: cargo test --locked --workspace --all-features 连跑 3 次全绿（含 oversize_frame 用例，复现 PRO-4 触发负载）"
    result: PASS
    candidate_commit: 25acb00d6895bcbe1ba36b658e0dd4706fd0347a
    evidence: {path: reports/stress-runs.log, sha256: "sha256:447574013e7a9bf149c76ad05c4fc85db1fdbfb8d73d0cab4603b77419ce7cda"}
```

## Failures and Retests

（无。执行中的失败、重试与恢复记录在此。）

## Final Assessment

（8.1 最终验收时按 `templates/verification.md` 写入 `agentic-assessment` 块。）
