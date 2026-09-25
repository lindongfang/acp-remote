<!-- 主 Agent 维护协作计划；规则见 schema.yaml，角色细节见 roles/。
     本文件写安排，tasks 写步骤，verification 写执行历史。 -->

## Scope and Contracts

- Specs Revision: 无增量规范（skip_specs）；共同依据为 `proposal.md` 的 Intent and Constraints 与
  `design.md` 的 D1–D5（2026-09-26 版本，含 14,557 个残留目录的实测盘点）。
- Design Revision: `design.md`（2026-09-26）：守卫形态 `Drop + Deref<Target = Path>`、调用点审计准则、
  尽力而为 Drop + 计数兜底验收、命名唯一性保持现状。
- Convergence Check: 一致——无规范变更，design 的每个决策都可追溯到 proposal 的约束与成功判据。
- Skip Specs: yes。理由：纯测试基础设施修复，产品行为、协议 wire、端口合同、存储 DDL 均不变；
  既有行为契约 = `openspec/specs/` 的 16 个能力规范（语义保持原样，测试仍验证同样的行为）与
  `AGENTS.md` §9（测试要求）。

## Contract Changes

无（本变更不触碰任何契约资产；`schemas/`、`fixtures/`、`compatibility/`、`docs/` 零改动）。
若实施中发现必须改动产品代码或文档，先回写本节并重新评估证据有效性。

## Coverage Index

<!-- skip_specs：行引用既有契约与 proposal 的验收判据。 -->

```agentic-coverage
version: 1
target_ref: refs/heads/main
rows:
  - id: R1
    source:
      path: proposal.md
      heading: "## Intent and Constraints"
    tasks: ["2.1", "2.2", "2.3", "3.1"]
    checks: [PV2]
    evidence: [reports/temp-count-final.log]
  - id: R2
    source:
      path: ../../../AGENTS.md
      heading: "## 9. 测试要求"
    tasks: ["2.2", "2.3", "3.1"]
    checks: [PV1]
    evidence: [reports/final-verify.log]
  - id: R3
    source:
      path: design.md
      heading: "## Decisions"
    tasks: ["2.1", "2.2", "2.3", "3.2"]
    checks: [RV1]
    evidence: [reports/rv1-wp1.md]
```

## Work Packages

| ID | Goal / Scenarios | Dependencies | Owner | Reviewer | Branch / Worktree | Write Scope | Inputs / Outputs | Verification |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WP1 | 全部测试临时产物自清理（R1/R2/R3）：盘点 + 各 crate 守卫落地 + 局部验证 | 无 | 实现 Agent（deepseek/deepseek-flash） | 独立只读 reviewer（fresh 上下文） | 共享工作树（单写入方，主 Agent 监督） | 仅 `crates/*/tests/**` 与 `crates/*/src/**` 的 `#[cfg(test)]` 模块 | 输入：proposal/design；输出：盘点表 `reports/inventory.md`、守卫实现、局部测试绿 | PV1、PV2、RV1 |

单工作包的理由：各 crate 的测试文件物理不相交，但改动小且审计准则统一（design D2），一个负责人
串行推进比一个协调多个 Agent 更可靠；不拆 TP（见 Main E2E 的 not-applicable）。

## Execution Waves

- Wave 1（唯一）：WP1 串行执行——先盘点（2.1）再修复（2.2/2.3）再局部验证（2.4）。
  无契约/代码/资源依赖需要跨波次等待。

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |

不适用：单工作包、无上游交付依赖（依据：本变更只改测试代码，不从其它变更接入代码）。

## Runtime Resources

| Checks / Work Packages | Resource | Isolation / Configuration | Exclusive Scheduling | Owner / Cleanup |
| --- | --- | --- | --- | --- |
| PV2（计数验收） | 系统临时目录（Windows：`C:\Users\zhang\AppData\Local\Temp`；Linux CI：`/tmp`） | 以 `acpr-*` 名称前缀过滤，只统计该前缀 | 计数前后窗口内**不得并行**运行其它 `cargo test`（会互相污染计数）；本地串行执行 | 主 Agent；PV2 结束后删除本次产生的残留（预期为零） |
| 全量测试（PV1） | `target/` 共享构建目录 | 默认共享；串行执行不加 `CARGO_TARGET_DIR` 分片 | 与其它任务串行 | 主 Agent |

依据：本变更不引入数据库、端口、容器或外部服务；唯一共享资源是测试进程共同写入的系统临时目录，
其并发风险只影响「计数验收」这一观察动作，因此用「串行 + 前缀过滤」隔离。

## Target Repository and Main Branch

- Code Repository: `D:\Project\acp-remote`
- Target Ref: `refs/heads/main`
- Version Confirmation Owner: 主 Agent（机械核实，6.1 时执行 `git rev-parse refs/heads/main` 并记录）
- Confirmation Method / Evidence: `git rev-parse` + `git status --porcelain`，结果写入 `verification.md`
  （规划时刻的已知值：main = `ad29199aeb0d8fe51776ec207997ed1218106937`，仅作背景，不作验收依据）

## Merge Strategy

| Delivery Unit / Mode | WP / TP | Owners: Implementation / Test / Review / Merge | Start / Readiness | Candidate Checks / Critical E2E | Order |
| --- | --- | --- | --- | --- | --- |
| DU1 / independent | WP1（无 TP） | 实现 Agent / — / 独立 reviewer / 独立集成 Agent | design 冻结 + 资源约定（本计划）即可开始 | PV1 + PV2；Main E2E not-applicable（见下） | 1（唯一单元） |

- Integration Branch / Worktree: `feat/test-temp-dir-cleanup`（本仓库共享工作树；WP1 为唯一写入方，
  冻结期内主 Agent 不写代码文件）
- Source Revision / Handoff: 以 6.1 核实的 `refs/heads/main` 当前提交为基线；无上游交接。
- Local Merge Conditions / Report Path: 候选轮 [PV1][PV2] 全绿 + RV1 独立审查 PASS + premerge 门
  PASS 后，集成 Agent 本地合入 `refs/heads/main`（优先 `--ff-only`）；报告 `reports/integrator.md`。
  满足条件即合入，无须重复确认；回滚须另行授权。

## Verification Strategy

### Local Checks

实现期间的快速反馈：`cargo test -p <受影响 crate>`（逐 crate 局部验证）；`cargo fmt --all -- --check` +
`cargo clippy --locked -p <crate> --all-targets -- -D warnings`。

### Project Verify

| Check ID | Stages / Work Packages | Command / Working Directory | Configuration / Environment | Scope / Pass Criteria | Evidence |
| --- | --- | --- | --- | --- | --- |
| PV1 | 候选、主分支 | `npm run verify`（仓库根） | 与 CI 的 `checks` job 同源（fmt + 十道合同门禁 + clippy + `cargo test --locked --workspace --all-features`） | EXIT=0 | `reports/final-verify.log`（候选）、`reports/main-verify.log`（主分支） |
| PV2 | 交付前、候选 | 计数前后对比：`ls -d $TEMP/acpr-*` 计数 → `cargo test --locked --workspace --all-features` → 再计数（仓库根；Windows 本机执行） | 串行窗口（不并行其它 cargo test）；前缀过滤 | **差值为 0**，且全量测试 EXIT=0；若有残余必须逐个归因到具体用例并修复 | `reports/temp-count-final.log` |

### Additional Independent Validation

不启用：RV1 独立 review 已覆盖「盘点是否漏点、守卫是否正确、断言是否被弱化」的独立判断需求。

### Code Review

RV1：独立只读 reviewer（fresh 上下文，不参与实现），范围 = WP1 全部 diff。关注点：① 盘点核对表与
`grep -rn 'temp_dir()' crates/ --include=*.rs` 的实际输出一致、无漏点；② 每个新守卫满足 design D1–D4
（Drop 不 panic、Deref 语义、命名唯一、无 inline 临时值误用）；③ 未改动任何产品代码与契约资产；
④ 未为通过检查而删测试或弱化断言。阻断标准：上述任一条不满足。报告 `reports/rv1-wp1.md`。

### Main E2E

```yaml
mode: not-applicable
reason: "纯测试基础设施修复：diff 面只含 tests/ 与 #[cfg(test)] 模块，产品行为、协议 wire、端口合同均不变，没有可供端到端运行的新产品路径（Daemon/CLI 行为一行未改）"
basis: "openspec/config.yaml 的 x-agentic.e2e.command 为空（项目约定：无 E2E 入口时按变更逐次批准降级）；本变更的成功判据（临时目录计数差为 0）本身就是可机器验证的替代检查"
alternative_checks: ["PV1: npm run verify（全量 Rust 测试 + 十道合同门禁）", "PV2: cargo test --locked --workspace --all-features 前后系统临时目录 acpr-* 条目数差为 0"]
downgrade_approval: "2026-09-26 用户原话：『同意降级』（本会话，对主 Agent 就 plan.md Main E2E 提问的直接回复）"
```

## Failure and Recovery

- 实现/修复回 WP1 原负责人；review 发现按 RV1 轮次重派并复核。
- PV2 计数差非零：按残留目录名前缀归因到 crate/用例（`acpr-storage-*` → storage-sqlite 等），修复后
  重跑 PV2；不允许用「尽力而为」豁免系统性泄漏（design D3）。
- 资源异常（测试中断留下目录）：手工删除后重新计数即可，不计失败。
- 回滚条件：本变更无数据/协议面，出问题直接 revert 合入提交即可（须另行授权）。

## Completion Criteria

- 目标分支 `refs/heads/main` 含本单元合入提交；候选与主分支的 PV1 均绿、PV2 差值为 0、RV1 PASS。
- `verification.md` 记录：版本核实、候选构建/验证、review 报告、交接与合入证据、E2E 降级批准与替代检查结果。
- 阻断清零后按 `.agents/skills/agentic-verify/SKILL.md` 执行最终验收（PASS 才可归档）。
