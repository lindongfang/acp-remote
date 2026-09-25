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

### Project Verify 记录

（[PV1]/[PV2] 执行时登记：命令、版本、退出码、日志路径。）

## Check Plan Changes

（无。执行中如调整检查计划在此登记：原值、新值、原因、影响分析、证据失效/复用历史。）

## Dependency Handoffs

无代码交接（见「上游依赖」）。

## Runtime Resources

（PV2 连跑窗口与实际清理记录在执行时登记。）

## Review Findings

（RV1 报告登记处。）

## Merge History

（DU1 合入时登记：基线、候选、树哈希、命令、证据。）

## Test Design and Authoring

不适用：Main E2E 为 not-applicable（`plan.md` 已记录降级批准），本变更不设 TP。

## Candidate E2E

NOT_APPLICABLE：模式、理由、依据与降级批准见 `plan.md` 的 Main E2E 节；替代检查为 PV1/PV2。

## Main E2E

NOT_APPLICABLE：同上。`[e2e-owned]` 任务 7.3 由扩展的 `e2e check` 判定。

## Failures and Retests

（无。执行中的失败、重试与恢复记录在此。）

## Final Assessment

（8.1 最终验收时按 `templates/verification.md` 写入 `agentic-assessment` 块。）
