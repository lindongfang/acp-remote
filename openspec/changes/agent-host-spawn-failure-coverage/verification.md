# Verification: agent-host-spawn-failure-coverage

## Target

- 变更：agent-host-spawn-failure-coverage
- 代码仓库：D:\Project\acp-remote
- 目标主分支：refs/heads/main
- 核实方式：`git -C D:\Project\acp-remote rev-parse refs/heads/main`
- apply 启动核实（2026-09-25，任务 5.1 前置/1.1 记录来源）：`f53dad55e72111bb7e026a0d13f5180f82c61358`；当前工作分支 main，工作区另有用户未提交改动 `openspec/config.yaml`（角色模型配置，非本变更范围，全程不提交、不覆盖）
- 实现分支：`test/agent-host-spawn-failure-coverage`（基于上述 main 提交创建）

## Handoff Index

| Task ID | Role / Phase / Stage | Target Revision | Evidence Type / ID | Report Path | Result / Evidence Status | Applicability / Source Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| 1.1 | main / setup / work-package | f53dad55e72111bb7e026a0d13f5180f82c61358 | RESOURCE / NOT_APPLICABLE | verification.md（本文件 Target 节） | PASS / NEW | 可信来源直接引用：仓库根、目标引用、rust-toolchain.toml、npm run verify（AGENTS.md §8），无独立 recon 需求 |
| 1.2 | main / setup / work-package | f53dad55e72111bb7e026a0d13f5180f82c61358 | DELIVERY / NOT_APPLICABLE | verification.md（本节） | PASS / NEW | 契约冻结：proposal/specs/design/plan 于 2026-09-25 规划完成且 plan 阶段 workflow check PASS（contractDigest sha256:b8b94adb…）；文件所有权：WP1 独占 crates/agent-host/tests/catalog.rs，WP2 独占 crates/agent-host/src/error.rs（仅 #[cfg(test)] 模块）；编码起点 f53dad5 |
| 1.3 | main / setup / work-package | f53dad55e72111bb7e026a0d13f5180f82c61358 | RESOURCE / NOT_APPLICABLE | verification.md（本节） | PASS / NEW | 无共享运行资源：fake ACP child 由 cargo 构建提供，测试临时文件自清；target/ 为共享构建目录，本变更串行执行不并行分片 |
| 1.4 | main / setup / work-package | f53dad55e72111bb7e026a0d13f5180f82c61358 | DELIVERY / NOT_APPLICABLE | verification.md（本节） | PASS / NEW | 无代码依赖：WP1/WP2 互不依赖，共同只读引用 error.rs 现有映射表 |
| 2.1 | coder / implement / work-package | ea8762f1a71befbff9867fba8e139df9e7832e9d | CHECK / LC1 | reports/coder-wp1.md（日志 reports/LC1.log） | PASS / NEW | 命令与 plan.md LC1 逐字一致；日志采集时 HEAD=ac74102，tests/catalog.rs 自 ea8762f 逐字节未变（报告已注明） |
| 2.1 | coder / implement / work-package | ea8762f1a71befbff9867fba8e139df9e7832e9d | CHECK / LC3 | reports/coder-wp1.md（日志 reports/LC3.log） | PASS / NEW | fmt/clippy 零警告，exit 0 |
| 2.1 | coder / implement / work-package | ea8762f1a71befbff9867fba8e139df9e7832e9d | CHECK / WP-verify | reports/coder-wp1.md（日志 reports/WP-verify-agent-host-all.log） | PASS / NEW | agent-host 全量测试 exit 0，0 失败；另附假证探针（reports/falsifiability-probe.log）：SpawnFailed 改为 InvalidRequest 时两个新测试均 FAILED，探针已还原未提交 |
| 2.2 | coder / implement / work-package | ac74102897f8d7d9b0830b3de3c7e958512b5de2 | CHECK / LC2 | reports/coder-wp2.md（日志 reports/LC2.log） | PASS / NEW | 命令与 plan.md LC2 逐字一致，error::tests 1 passed，exit 0 |
| 2.2 | coder / implement / work-package | ac74102897f8d7d9b0830b3de3c7e958512b5de2 | CHECK / LC3 | reports/coder-wp2.md（日志 reports/LC3.log） | PASS / NEW | 同 2.1 行（同一 HEAD 采集） |
| 2.2 | coder / implement / work-package | ac74102897f8d7d9b0830b3de3c7e958512b5de2 | CHECK / WP-verify | reports/coder-wp2.md（日志 reports/WP-verify-agent-host-all.log） | PASS / NEW | 同 2.1 行（同一 HEAD 采集） |
| 3.1 | main / verify / work-package | ac74102897f8d7d9b0830b3de3c7e958512b5de2 | CHECK / PV1 | reports/PV1.log | PASS / NEW | npm run verify exit 0；合同门禁全绿 + clippy 零警告 + workspace 全量测试 0 失败，含两个新测试 |
| 3.2 | reviewer / review / work-package | ac74102897f8d7d9b0830b3de3c7e958512b5de2 | REVIEW / RV1 | reports/review-rv1.md | PASS / NEW | 独立隔离子 Agent（无写工具，报告由主 Agent 按其返回全文落盘）；无 CRITICAL/MAJOR；RV1-F1（MINOR）已由主 Agent 同步修复，RV1-F2（SUGGESTION）不采纳入本变更 |

## Checks

| Check ID / Stage / Work Package | Revision / Base | Scope | Executor | Command / Steps | Environment | Result / Exit Code | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| PV1 / branch / WP1+WP2 | ac74102897f8d7d9b0830b3de3c7e958512b5de2（base f53dad5） | npm run check 全部合同门禁 + fmt/clippy/workspace 全量测试（含 spawn_failure_is_explicit_unavailable_without_side_effects 与 error::tests::to_port_error_is_pinned_per_variant） | 主 Agent（kimi-coding/k3） | `npm run verify`（D:\Project\acp-remote，分支 test/agent-host-spawn-failure-coverage） | Windows，Node v24.19.0，rust 1.98.1（rust-toolchain.toml） | PASS / exit 0 | reports/PV1.log |

## Check Plan Changes

- 2026-09-25：变体计数漂移修正。coder 报告 `HostError` 实际有 19 个变体（tasks.md 2.2 与 design.md 决策 2 原写「16 个」）。已将两处同步为「全部变体（当前 19 个，以代码为准）」。影响：无——实现按「全部变体」覆盖了 19 个（严格包含原 16 个），specs 场景与成功判据（「覆盖全部变体」）不含数字，不受影响。任务/需求映射不变。

## Dependency Handoffs

无代码依赖（任务 1.4 记录）。

## Runtime Resources

无共享运行资源：fake ACP child（acpr-fake-acp-agent）由 cargo 测试构建提供；各测试临时文件由测试自身 temp_dir 清理；`target/` 共享构建目录在本变更中串行使用，无需独占队列。

## Review Findings

| ID | Revision | Reviewer | Location | Severity / Impact | Resolution | Recheck Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| RV1-F1 | ac74102 | reviewer（kimi-coding/k3，独立隔离子 Agent） | plan.md WP2 行与 Code Review 关注点 | MINOR / 不阻断：plan.md 两处漏同步变体计数（「16」→ 应为全部变体当前 19 个） | 主 Agent 已于 2026-09-25 同步 plan.md 两处为「全部变体（当前 19 个，以代码为准）」；纯文档同步，不动代码/测试 | 主 Agent 修复，属调度者文书职责，无需独立复核（非代码/测试变更） |
| RV1-F2 | ac74102 | 同上 | crates/agent-host/src/error.rs:154 | SUGGESTION / 不阻断：变体覆盖缺编译期穷尽性护栏 | 不采纳入本变更：现有固定数组 + 标签不重复断言已钉死当前映射，穷尽性护栏属可选增强，超出本变更范围（AGENTS.md §8 不顺手扩范围）；留作后续可选改进 | 不适用 |

## Merge History

（待 DU1 候选/合入阶段填写）

## Test Design and Authoring

不适用（Main E2E mode = not-applicable，无 TP）。

## Candidate E2E

不适用（mode = not-applicable）。

## Main E2E

项目开关：`npx --quiet --no-install openspec-agentic e2e --json` 读得 `enabled=true`、`command=""`、`maxAttempts=3`（2026-09-25 apply 启动时读取）。
mode = not-applicable；reason / basis / alternative_checks 见 plan.md 的 Main E2E 块；downgrade_approval：2026-09-25 本会话用户原话「同意 agent-host-spawn-failure-coverage 的 Main E2E 记 not-applicable，替代验证为 agent-host 的 cargo 测试 + npm run verify」。
E2E 结论：NOT_APPLICABLE。替代检查在 Checks 节逐项留证（PV1 候选 + 主分支 + 最终回归，含 `cargo test --locked -p agent-host --all-features`）。

## Failures and Retests

无（截至目前）。

## Final Assessment

（待最终验收填写）
