<!-- 主 Agent 维护协作计划；规则见 schema.yaml，角色细节见 roles/。
     本文件写安排，tasks 写步骤，verification 写执行历史。 -->

## Scope and Contracts

- Specs Revision: 增量规范 `specs/local-agent-host/spec.md`（2026-09-26）：1 条 ADDED Requirement
  「超限结束的失败关闭顺序」+ 2 个 Scenario；主规范其余文本不变。
- Design Revision: `design.md`（2026-09-26）：D1 顺序调整为「标记 → 投递 → terminate」、D2 两条
  实现期核实不变量、D3 测试零改动 + 构造性确定与 3 连跑验收口径。
- Convergence Check: 一致——spec 的场景 1 由 D1 满足、场景 2 由「`wait_loop` 不动」满足；
  验收口径（D3）与 proposal 的 success_criteria 一致。
- Skip Specs: no（本变更含 1 条增量 Requirement）。

## Contract Changes

无契约资产（`schemas/`、`fixtures/`、`compatibility/`、`docs/`）改动。增量规范属于
`local-agent-host` 能力的行为语义补充，不涉及 wire/DDL/端口签名。

## Coverage Index

```agentic-coverage
version: 1
target_ref: refs/heads/main
rows:
  - id: R1
    source:
      path: specs/local-agent-host/spec.md
      heading: "### Requirement: 超限结束的失败关闭顺序"
    tasks: ["2.1", "3.1"]
    checks: [PV1, PV2]
    evidence: [reports/final-verify.log, reports/stress-runs.log]
  - id: R2
    source:
      path: specs/local-agent-host/spec.md
      requirement: "### Requirement: 超限结束的失败关闭顺序"
      heading: "#### Scenario: 错误可见时 Agent 已被标记退出"
    tasks: ["2.1", "2.2", "3.1"]
    checks: [PV2]
    evidence: [reports/stress-runs.log]
  - id: R3
    source:
      path: specs/local-agent-host/spec.md
      requirement: "### Requirement: 超限结束的失败关闭顺序"
      heading: "#### Scenario: 自然退出路径不受本顺序约束"
    tasks: ["2.1", "3.2"]
    checks: [RV1]
    evidence: [reports/rv1-wp1.md]
```

## Work Packages

| ID | Goal / Scenarios | Dependencies | Owner | Reviewer | Branch / Worktree | Write Scope | Inputs / Outputs | Verification |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WP1 | R1/R2/R3：`abort_agent` 顺序调整 + D2 不变量核实 + 局部验证 | 无 | 实现 Agent（deepseek/deepseek-flash） | 独立只读 reviewer（fresh 上下文） | 共享工作树（单写入方，主 Agent 监督） | 仅 `crates/agent-host/src/process.rs` 的 `abort_agent` 函数与其注释 + 本变更 reports/ | 输入：proposal/specs/design；输出：顺序调整、核实记录、局部测试绿 | PV1、PV2、RV1 |

单工作包理由：改动是单函数内的语句顺序（预期个位数行），一个负责人串行完成；
不拆 TP（Main E2E not-applicable，见下）。

## Execution Waves

- Wave 1（唯一）：WP1 串行——实现与不变量核实（2.1）→ 局部验证（2.2）。

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |

不适用：单工作包、无上游交付依赖（依据：本变更只改一个函数，不从其它变更接入代码）。

## Runtime Resources

| Checks / Work Packages | Resource | Isolation / Configuration | Exclusive Scheduling | Owner / Cleanup |
| --- | --- | --- | --- | --- |
| PV2（3 连跑） | 构建与测试进程、`target/` | 默认共享 | 连跑窗口内**不并行**其它 `cargo test`（避免负载来源混杂、也避免干扰「全量并行负载」这一被测场景的真实性） | 主 Agent；结束后无遗留（预期 `acpr-*` 残留为 0，顺带观察，不作判据） |
| 全量测试（PV1） | 系统临时目录 | 上个变更已保证测试自清理 | 串行 | 主 Agent |

依据：本变更不引入数据库、端口、容器或外部服务；PV2 的「全量并行负载」本身就是被测场景，
因此不需要额外负载生成，只需要窗口内不混入无关构建。

## Target Repository and Main Branch

- Code Repository: `D:\Project\acp-remote`
- Target Ref: `refs/heads/main`
- Version Confirmation Owner: 主 Agent（机械核实，6.1 时执行 `git rev-parse refs/heads/main` 并记录）
- Confirmation Method / Evidence: `git rev-parse` + `git status --porcelain`，结果写入 `verification.md`
  （规划时刻的已知值：main = `84a8a05`，仅作背景，不作验收依据）

## Merge Strategy

| Delivery Unit / Mode | WP / TP | Owners: Implementation / Test / Review / Merge | Start / Readiness | Candidate Checks / Critical E2E | Order |
| --- | --- | --- | --- | --- | --- |
| DU1 / independent | WP1（无 TP） | 实现 Agent / — / 独立 reviewer / 独立集成 Agent | design 冻结即可开始 | PV1 + PV2；Main E2E not-applicable（见下） | 1（唯一单元） |

- Integration Branch / Worktree: `feat/agent-host-oversize-exit-ordering`（共享工作树；WP1 唯一写入方）
- Source Revision / Handoff: 以 6.1 核实的 `refs/heads/main` 当前提交为基线；无上游交接。
- Local Merge Conditions / Report Path: 候选轮 [PV1][PV2] 全绿 + RV1 PASS + premerge 门 PASS 后，
  集成 Agent 本地合入 `refs/heads/main`（优先 `--ff-only`）；报告 `reports/integrator.md`。
  满足条件即合入，无须重复确认；回滚须另行授权。

## Verification Strategy

### Local Checks

实现期间：`cargo test -p agent-host`（重点 `oversize_frame` 用例）；`cargo fmt --all -- --check` +
`cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings`。

### Project Verify

| Check ID | Stages / Work Packages | Command / Working Directory | Configuration / Environment | Scope / Pass Criteria | Evidence |
| --- | --- | --- | --- | --- | --- |
| PV1 | 交付前、候选、主分支 | `npm run verify`（仓库根） | 与 CI 的 `checks` job 同源（fmt + 十道合同门禁 + clippy + 全量测试 + spec 校验） | EXIT=0 | `reports/final-verify.log`（候选/交付前）、`reports/main-verify.log`（主分支） |
| PV2 | 交付前、候选 | `cargo test --locked --workspace --all-features` **连跑 3 次**（仓库根，串行窗口） | Windows 本机全量并行负载（与 PRO-4 触发环境同形） | 3 次均 EXIT=0 且 `oversize_frame` 用例每次都绿；任何一次红即 FAIL 并保留日志 | `reports/stress-runs.log` |

### Additional Independent Validation

不启用：RV1 独立 review 已覆盖「顺序正确性、D2 不变量核实质量、断言未弱化」的独立判断需求。

### Code Review

RV1：独立只读 reviewer（fresh 上下文，不参与实现），范围 = WP1 全部 diff + `abort_agent`/`wait_loop`/
`ExitState` 上下文。关注点：① 新顺序为「标记 → 投递 → terminate」且与注释一致；② D2 两条不变量的
核实记录真实可查（引用代码行）；③ `wait_loop` 与其余 `exit.mark` 调用点未被改动；④ 未弱化任何断言、
未触碰其它文件；⑤ spec 增量与实现语义一致。阻断标准：任一条不满足。报告 `reports/rv1-wp1.md`。

### Main E2E

```yaml
mode: not-applicable
reason: "单函数内部语句顺序修正：无产品级端到端新路径，被测行为（超限结束的失败关闭顺序）由既有 oversize_frame 用例直接覆盖，产品入口、协议与端口均不变"
basis: "openspec/config.yaml 的 x-agentic.e2e.command 为空（项目约定：无 E2E 入口时按变更逐次批准降级）；本变更的验收判据（连跑全绿）本身是负载实证的替代检查"
alternative_checks: ["PV1: npm run verify（全量 Rust 测试 + 十道合同门禁 + spec 校验）", "PV2: cargo test --locked --workspace --all-features 连跑 3 次全绿（含 oversize_frame 用例，复现 PRO-4 触发负载）"]
downgrade_approval: "2026-09-26 用户原话：『同意降级』（本会话，对主 Agent 就 plan.md Main E2E 提问的直接回复）"
```

## Failure and Recovery

- 实现/修复回 WP1 原负责人；review 发现按 RV1 轮次重派并由新 reviewer 复核。
- PV2 任一轮红：保留该轮完整日志，按「是新竞态还是其它 flaky」归因；若是本变更相关，回到 WP1；
  若发现**其它**既有 flaky（与本变更无关），如实登记为新的 PRO 项交用户决策，不静默重跑掩盖。
- 回滚条件：本变更无数据/协议面，出问题直接 revert 合入提交（须另行授权）。

## Completion Criteria

- 目标分支 `refs/heads/main` 含合入提交；候选与主分支的 PV1 均绿、PV2 三连跑全绿、RV1 PASS。
- `verification.md` 记录：版本核实、候选构建/验证、review 报告、合入证据、E2E 降级批准与替代检查结果。
- 阻断清零后按 `.agents/skills/agentic-verify/SKILL.md` 执行最终验收（PASS 才可归档）。
