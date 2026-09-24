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
