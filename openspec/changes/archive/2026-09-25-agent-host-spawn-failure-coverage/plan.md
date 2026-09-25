# Plan: agent-host-spawn-failure-coverage

<!-- 主 Agent 维护协作计划；规则见 schema.yaml，角色细节见 roles/。
     本文件写安排，tasks 写步骤，verification 写执行历史。 -->

## Scope and Contracts

- Specs Revision: `specs/local-agent-host/spec.md` 增量——MODIFIED「失败路径的明确结果与 turn 不设超时」，新增场景「进程无法启动时返回明确不可用」，其余 4 个既有场景原文保留（2026-09-25 规划版本）。
- Design Revision: `design.md`——集成测试落 `tests/catalog.rs`、`to_port_error` 单测落 `error.rs` 内 `#[cfg(test)]`、只钉错误类别/kind 不钉消息文本、失败注入只用不存在程序名（2026-09-25 规划版本）。
- Convergence Check: 一致。spec 场景的三个可观察结果（明确 Unavailable 且可区分原因 / 无进程副作用与残留映射 / 重试不毒化）与 design 的关键断言一一对应；`to_port_error` 单测是 spec 需求「可区分原因」在 adapter 映射层的钉死，不引入额外行为。
- Skip Specs: no。

## Contract Changes

无。规划阶段未发生契约澄清；apply 中若发现实际行为与 spec 场景不符（design Risks 第 1 条），按 proposal 的 decision_bounds 上报用户后再更新本节与受影响 artifacts。

## Coverage Index

```agentic-coverage
version: 1
target_ref: refs/heads/main
rows:
  - id: R1
    source:
      path: specs/local-agent-host/spec.md
      heading: "### Requirement: 失败路径的明确结果与 turn 不设超时"
    tasks: ["2.1", "2.2", "3.1"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: S1
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 进程无法启动时返回明确不可用"
      requirement: "### Requirement: 失败路径的明确结果与 turn 不设超时"
    tasks: ["2.1"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: S2
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 启动超时失败并回收"
      requirement: "### Requirement: 失败路径的明确结果与 turn 不设超时"
    tasks: ["3.1"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: S3
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: turn 不因超时被杀"
      requirement: "### Requirement: 失败路径的明确结果与 turn 不设超时"
    tasks: ["3.1"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: S4
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 异常退出成为明确事件"
      requirement: "### Requirement: 失败路径的明确结果与 turn 不设超时"
    tasks: ["3.1"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: S5
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 未知响应标识不被误配"
      requirement: "### Requirement: 失败路径的明确结果与 turn 不设超时"
    tasks: ["3.1"]
    checks: [PV1]
    evidence: [reports/PV1.log]
```

说明：S2–S5 是 MODIFIED 需求中原样保留的既有场景，由 `tests/supervision.rs`、`tests/session.rs` 的常驻测试覆盖，本变更不新增对应任务，只由 3.1 的全量回归确认不被破坏；S1 是本变更的唯一新增场景。R1 的「可区分原因」由 2.2（`to_port_error` 逐变体单测）钉死。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Owner | Reviewer | Branch / Worktree | Write Scope | Inputs / Outputs | Verification |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WP1 | S1：`create()` 对不存在程序的 profile 返回 `Unavailable(IoError)`、无副作用、重试不毒化 | 无（契约已冻结） | coder（deepseek/deepseek-flash） | reviewer（kimi-coding/k3） | `test/agent-host-spawn-failure-coverage`（本仓库工作区） | `crates/agent-host/tests/catalog.rs` | 输入：specs S1、design 决策 1/3/4；输出：新增集成测试 `spawn_failure_is_explicit_unavailable_without_side_effects` | PV1 |
| WP2 | R1「可区分原因」：`to_port_error()` 全变体（当前 19 个，以代码为准）逐条钉死映射 | 无（契约已冻结） | coder（deepseek/deepseek-flash） | reviewer（kimi-coding/k3） | `test/agent-host-spawn-failure-coverage`（本仓库工作区） | `crates/agent-host/src/error.rs`（仅追加 `#[cfg(test)]` 模块） | 输入：design 决策 2/3、`error.rs` 现有映射表；输出：`mod tests` 映射单测 | PV1 |

说明：Main E2E mode 为 `not-applicable`（见下），不生成独立测试工作包（TP）；本变更的交付物本身就是测试，行为测试即 WP1/WP2 本体，替代验证由主 Agent 任务（3.1）统一执行并留证。

## Execution Waves

| Wave | Work Packages | 依赖满足条件 | 并发上限 | 说明 |
| --- | --- | --- | --- | --- |
| W1 并行实现 | WP1 ∥ WP2 | specs/design 已冻结；两者写入文件互不重叠（`tests/catalog.rs` ／ `src/error.rs`） | 2 | 单变更单分支，并行仅为任务派发便利；共享 `target/` 时按 Runtime Resources 隔离 |
| W2 集成验证 | 3.1（PV1 全量） | WP1、WP2 均已验收 | 1 | 主 Agent 在同一候选提交上执行 |

## Dependency Handoffs

无代码依赖：WP1 与 WP2 不互相依赖，也不依赖任何未落地 crate。两者共同依赖的契约是已冻结的 specs/design 与 `crates/agent-host/src/error.rs` 现有映射表（只读引用，不修改）。

## Runtime Resources

| Checks / Work Packages | Resource | Isolation / Configuration | Exclusive Scheduling | Owner / Cleanup |
| --- | --- | --- | --- | --- |
| WP1/WP2/3.1 | `target/` 构建目录 | 并行分片时用各自 `CARGO_TARGET_DIR`；串行执行时共享 | 本变更规模小，默认串行，无需独占 | 主 Agent；测试临时文件由各测试自身 `temp_dir` 清理 |

不涉及数据库、端口、容器或外部服务；fake ACP child（`acpr-fake-acp-agent`）由 cargo 测试构建提供，属既有设施。

## Target Repository and Main Branch

- Code Repository: `D:\Project\acp-remote`
- Target Ref: `refs/heads/main`（本地主分支）
- Version Confirmation Owner: 主 Agent（可委托 environment/recon 机械核实）
- Confirmation Method / Evidence: apply 开始时与候选合入前各执行一次 `git -C D:\Project\acp-remote rev-parse refs/heads/main`，结果记入 verification.md；不默认 HEAD 即目标

## Merge Strategy

单一交付单元，integrated 模式：WP1 与 WP2 同属「测试补强」一个语义单元，候选验证、review 与合入一次完成，不拆两次交付。

| Delivery Unit / Mode | WP / TP | Owners: Implementation / Test / Review / Merge | Start / Readiness | Candidate Checks / Critical E2E | Order |
| --- | --- | --- | --- | --- | --- |
| DU1 / integrated | WP1 + WP2 | coder / 不适用（交付物即测试）/ reviewer / 主 Agent | specs/design 冻结；`refs/heads/main` 已核实 | PV1（含新增测试）；无 E2E（not-applicable） | 1 |

- Integration Branch / Worktree: `test/agent-host-spawn-failure-coverage`（本仓库工作区，单执行者，无并发写入）
- Source Revision / Handoff: apply 开始时核实的 `refs/heads/main` 提交，记入 verification.md
- Local Merge Conditions / Report Path: 候选门禁 = PV1 绿 + reviewer 独立复核 PASS + premerge 阶段 `workflow check` PASS；满足后按 AGENTS.md §8 走 PR（Conventional Commits 标题、必需检查全绿、squash 合并），schema 的本地合入不替代 PR；集成报告写入 verification.md

## Verification Strategy

### Local Checks

- `cargo test --locked -p agent-host --all-features spawn_failure`：WP1 快速自检。
- `cargo test --locked -p agent-host --all-features error::tests`：WP2 快速自检。
- `cargo fmt --all -- --check`、`cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings`。

### Project Verify

| Check ID | Stages / Work Packages | Command / Working Directory | Configuration / Environment | Scope / Pass Criteria | Evidence |
| --- | --- | --- | --- | --- | --- |
| PV1 | 候选、主分支 / WP1、WP2、3.1 | `npm run verify`（仓库根 `D:\Project\acp-remote`） | Node ≥ 22.12；Rust 工具链以 `rust-toolchain.toml` 为准 | 统一入口：`npm run check`（合同门禁全绿，含 `openspec validate --strict` 经 `check:agentic` 链路）+ `npm run check:rust`（fmt / clippy / workspace 全量测试，含新增两组测试）；零失败 | `reports/PV1.log`（候选与主分支各一份） |

### Additional Independent Validation

不启用：本变更是确定性测试补强，PV1 与 reviewer 复核已覆盖风险，无需探索性验证。

### Code Review

- Reviewer：kimi-coding/k3（非用例作者），按 `roles/reviewer.md` 独立复核。
- 范围：WP1/WP2 全部改动 + 增量 spec。
- 关注点：新增测试是否真实驱动 spawn 失败路径（而非只测 availability 预检）；断言是否精确到 `UnavailableKind::IoError`；`to_port_error` 单测是否覆盖全部变体（当前 19 个）且未钉死消息文本；是否夹带产品代码改动（本变更不允许）。
- 阻断标准：上述任一项不成立即 FAIL，退回对应 WP 修复。

### Main E2E

```yaml
mode: not-applicable
reason: "本变更只向 crates/agent-host 增补测试与一个规范场景，不改动任何产品代码；server、app、daemon、CLI 与前端尚未实现，仓库中不存在可端到端运行的产品入口（无监听器、无 CLI 子命令、无前端），因此无法在真实入口执行端到端场景。"
basis: "openspec/config.yaml 的 context 已记录「当前阶段没有可端到端运行的产品路径，Main E2E 按变更记 not-applicable，需用户逐变更批准」，且 x-agentic.e2e.command 为空；本变更范围为库 crate 的测试与规范场景。"
alternative_checks:
  - cargo test --locked -p agent-host --all-features（含新增 spawn 失败集成测试与 to_port_error 映射单测；覆盖 S1 与 R1 可区分原因）
  - npm run verify（统一合同门禁与 fmt/clippy/workspace 全量测试；确认 S2–S5 既有场景回归不破且规范增量通过校验）
downgrade_approval: "2026-09-25，本会话，用户原话：「同意 agent-host-spawn-failure-coverage 的 Main E2E 记 not-applicable，替代验证为 agent-host 的 cargo 测试 + npm run verify。」"
```

#### E2E Ownership and Cases

不适用（mode = not-applicable），本节按模板要求删除；替代验证已列为主 Agent 任务（3.1）并在 Coverage Index 中引用。

#### E2E Execution Contract

不适用（mode = not-applicable）。替代验证的入口、命令与证据见 Verification Strategy / Project Verify（PV1）。

#### E2E Execution Waves

不适用（mode = not-applicable）。唯一 [e2e-owned] 门禁行由 `openspec-agentic e2e check --change agent-host-spawn-failure-coverage` 按 not-applicable 判据核对：downgrade_approval 已回填且替代验证任务（3.1）先完成。

## Failure and Recovery

- 修复负责人：触发的缺陷按原 WP ID 重新派发给 coder；review 发现的问题同。
- 若 WP1 暴露实际行为与 S1 不符（真实缺陷）：立即停止编码，按 proposal decision_bounds 上报用户决策「先修实现还是先交付测试」；获批后更新 proposal/specs/design 与本计划，旧证据失效重跑。
- 资源异常释放：测试临时文件由测试自身清理；worktree 无共享状态，失败可直接删除分支重来。
- 回滚：合入前任何问题以丢弃候选分支解决；合入后回滚须另行授权。

## Completion Criteria

- 最终主分支版本包含 WP1/WP2 全部改动，`git rev-parse refs/heads/main` 核实记录于 verification.md。
- PV1 在候选与主分支各跑一次并全绿，日志落 `reports/PV1.log`；reviewer 独立复核 PASS 留证。
- downgrade_approval 已回填用户原话；[e2e-owned] 门禁行由 `e2e check` 判 PASS 自动勾选。
- 阻断清零后按 `.agents/skills/agentic-verify/SKILL.md` 执行最终验收（[final-verification]），仅 PASS 可报告具备归档条件。
