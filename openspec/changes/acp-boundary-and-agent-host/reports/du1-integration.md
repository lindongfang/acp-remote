# DU1 集成与本地合并证据（acp-boundary-and-agent-host）

## 合并方式

用户裁定「4. 先本地合并」→ 在本地把 `feat/acp-boundary-and-agent-host` 以 **fast-forward** 合入 `main`。
不推送、不开 PR、不动远端；`origin/main` 仍停在 `094009b`。

| 项 | 值 |
| --- | --- |
| 合并前 `main` | `094009b`（`docs(repo): 修正归档记录的 target_commit 并固定证据日志的忽略策略 (#13)`） |
| 合并方式 | `git merge --ff-only feat/acp-boundary-and-agent-host`（`main` 未分叉，无合并提交） |
| 合并后 `main` | `6f1515a`（= 分支 HEAD） |
| 合入提交数 | 7（`4b0145e`、`f2ca1f5`、`64a7582`、`e6b4dfa`、`df734e3`、`9d3bb63`、`6f1515a`） |
| 功能分支 | 保留（`feat/acp-boundary-and-agent-host`，与 `main` 同点） |

## 合并后复检

- `npm run check`（10 道合同门禁）→ **exit 0**（`main` 工作区，2026-09-24）。
- 合入前同一工作区的强化证据：`reports/du1-main-verify.log`
  （`npm run check` exit 0、全工作区 **361** 条 `test ... ok`、`cargo clippy --workspace -D warnings` exit 0）。
- 本变更两个新 crate 的边界判定：`crate boundaries OK: 8`（`crates/acp-protocol`、`crates/agent-host` 已登记进 `MODULE_ARCHITECTURE.md` §5 矩阵与 `Cargo.toml` members）。

## 未随合并完成的事项（有意保留）

- **未推送、未开 PR**：`main` 的 ruleset（PR + `checks`/`commits`/`deps`/`advisories`/`secrets`）未在本轮触发；`cargo-deny` 与 `gitleaks` 仍**只在 CI 运行**，本轮没有本地等价执行，不能声称通过。
- **未执行最终验收**：`tasks.md` 7/8 组与 `workflow check --stage final` 未跑，`verification.md` 的 `agentic-assessment` 保持 `BLOCKED`。
- **`turnId`/`version` 缺口**（Check Plan Change 3）：用户已同意走 core 侧收口，但实现留待下一变更（本变更不引入 core 端口/契约改动）。

## 就绪复核（任务 5.2）

- 模式：`integrated`（本变更只有一个交付单元 DU1，无独立单元顺序问题）；WP 组成 = WP1（文档/依赖矩阵冻结）+ WP2（`acp-protocol`）+ WP3（进程监督/平台）+ WP4（会话/映射）+ WP5（配置/凭据/目录/回收），另含 RV1/RV2/RV3/RV4 四轮独立检视后的修复批次。
- 固定版本：基线 `094009b`（= 合入前 `main`，也是 `origin/main`）；候选 `e4a4492`（= 合入后 `main`，因用户裁定「先本地合并」而候选与主分支为同一提交）。
- 3.x 证据对当前候选的有效性：各 WP 的交付前 PV 已在候选版本重跑（`wp2-acp-protocol-tests.log`、`wp3-agent-host-supervision.log`、`wp4-agent-host-session.log`、`wp5-agent-host-config.log`、`pv5-windows-tree.log`、`du1-main-verify.log` 的 378 条 workspace 测试与 10 道门禁）；独立检视证据为 RV1（WP1/WP2/WP3/WP4 全 FAIL → 逐条修复）、RV2（代码 PASS / 文档 FAIL → 修复）、RV3（WP1 6 MINOR + WP5 2 MAJOR → 修复）、RV4（两轮均「无未解决阻断项」，另给新批次 → 修复）、RV5（复核中）。
- 变更过的检查口径（Check Plan Changes 1–4）与受影响任务已在 `verification.md` 逐条登记；其中第 3 条的收口（core 侧注入 `turnId`/`version`）已获用户裁定但不在本变更实现。
- 结论：**就绪**（不阻塞 6.x 的候选与集成检查）；唯一未闭环的是 3.2/3.10 的复核轮结论（RV5）与 6.x 的检查/检视结论。

## 独立集成 Agent 交接记录（任务 5.1）

| 项 | 值 |
| --- | --- |
| 角色模板 | `openspec/schemas/agentic/roles/integrator.md` **全文**随任务明文交接给子 Agent（含 Required Inputs、Role and Workspace、Execution、Report 固定字段） |
| 实际 Agent | `worker` 子 Agent（独立上下文，未参与任何实现与检视）；run id 见 6.2 交接返回 |
| 上下文方式 | fresh（本单元内唯一一次创建；不复用实现者/reviewer） |
| 交付单元 / 模式 | DU1 / `integrated` |
| 固定版本 | 起点与已验收上游 `094009b`；候选 `e4a4492` |
| 独立工作区 | 仓库外的链接 worktree（`D:/Project/acpr-du1-worktree`），由集成 Agent 创建、固定候选提交、用后 `git worktree remove --force` 清理 |
| 目标分支 | 本地 `main`（**本地目标**；明确不推送、不开 PR、不动 `origin`） |
| 授权边界 | **无合并/回滚/推送/发布授权**（合并已按用户裁定由主 Agent 在本地完成）；不得改主工作区被跟踪文件与 plan/tasks/verification |
| 报告路径 | 交接返回（不进仓库）；结论由主 Agent 归档到本文件与 `verification.md` |

## E2E 判定复核（任务 6.5）

- `plan.md` 的 Main E2E mode = `not-applicable`，含 `reason`、`basis`、四项 `alternative_checks` 与可追溯到用户的 `downgrade_approval`（用户原话「1. 同意 2. 同意」，本会话 2026-09-24）。
- 核对：本变更没有任何任务依赖「必须运行的 E2E」；7.3（`[e2e-owned]`）只要求确认不适用判据已按计划固化，不执行测试；7.1/7.2 承载替代验证四项（`cargo test -p acp-protocol -p agent-host`、`npm run check`、`npm run verify`、Windows 上的 `tree_` 断言）。
- 结论：`not-applicable` 路径成立且依据完整，不阻塞合入；替代验证结果见 `alt-final-verification.md`（任务 7.1/7.2）。
