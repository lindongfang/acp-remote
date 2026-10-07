# Provisioner Handoff — WP6（MU3c）

- **agent_id**: `ProvisionerW1`
- **fork_turns**: `none`
- **worktree**: `D:\Project\acp-remote\.worktrees\wp6`
- **branch**: `feat/wp6`
- **plan Work Package**: `WP6`（Owner `coding-6` / Reviewer `review-6` / Branch / Worktree `wt/wp6`）
- **baseline**: `263d3ba8b48ae5dae8ab87b16bc4c0ea86f62314`（MU3b 合入后的 `refs/heads/main`）

## Enter Condition 核对

`plan.md` 的 Execution Waves 记 `W4 | WP6 | code:WP5a / code:WP5b 已合入（MU3a/MU3b），schemas/sync/v1/command.schema.json@WP1 已冻结 | code-dependency`。

| 前置 | 状态 | 证据 |
| --- | --- | --- |
| `code:WP5a` 已合入 | ✅ `a4440d6`（MU3a，`## Merge History`） | `refs/heads/main` 祖先链 |
| `code:WP5b` 已合入 | ✅ `263d3ba`（MU3b，本次基线） | `git -C .worktrees/wp6 rev-parse HEAD` == `refs/heads/main` |
| `schemas/sync/v1/command.schema.json@WP1` 已冻结 | ✅ 文件存在于仓库根，MU1a 已合入 | `schemas/sync/v1/command.schema.json` |

## 执行记录

| ID | 命令 | 工作目录 | 退出码 | 观察 |
| --- | --- | --- | --- | --- |
| PV-WT6 | `git worktree add .worktrees/wp6 -b feat/wp6 refs/heads/main` | `D:\Project\acp-remote` | 0 | `HEAD is now at 263d3ba` |
| PV-WT6-2 | `git -C .worktrees/wp6 rev-parse HEAD` / `rev-parse refs/heads/main` | `D:\Project\acp-remote` | 0 | 两侧同为 `263d3ba8b48ae5dae8ab87b16bc4c0ea86f62314` |
| PV-WT6-3 | `npm ci` | `.worktrees/wp6/clients/app` | 0 | `added 856 packages`；独立 `node_modules/` 就位（435 顶层目录） |
| PV-WT6-4 | `npx tsc --noEmit` | `.worktrees/wp6/clients/app` | 0 | 无诊断 |
| PV-WT6-5 | `npx vitest run` | `.worktrees/wp6/clients/app` | 0 | `Test Files 7 passed (7)` / `Tests 84 passed (84)` |
| PV-WT6-6 | `node scripts/run-browser-check.mjs` | `.worktrees/wp6/clients/app` | 0 | 真实 Chromium `16/16 passed` |

开工前基线自检（PV-WT6-4/5/6）确认继承的 WP5a/WP5b 代码在独立依赖安装下仍然全绿，WP6 的任何失败都可归因到本包自身。

## 隔离

- 仓库根 `node_modules/` **共享只读**（不改、不装）。
- `clients/app/node_modules/` 为本 worktree **独占**，由 provisioner 一次性安装，执行者不得再改动依赖清单。
- 未创建 `.native.ts`（v1 只交付 Web，该裁决出自 tasks 2.6 的 WP5b 条目，对 WP6 同理适用）。
- 主检出 `D:\Project\acp-remote` 在本单元内**只读**：禁止在其中执行 `git merge` / `checkout` / `reset`。

## 交接对象

| 字段 | 值 |
| --- | --- |
| Work Package | WP6 |
| Attempt | 1 |
| Executor | `coder-w6-r1` |
| Received At | 2026-10-05T05:55:00Z |
| Evidence | `reports/provision-wp6.md` |
| Dispatch 记录 | `dispatch-queue.jsonl`：`{"wp":"WP6","role":"coder","executor":"coder-w6-r1","attempt":1,"state":"coding"}` @ 2026-10-05T05:57:19.813Z |

## Write Scope（执行者硬约束）

`clients/app/src/sync-client/`、`clients/app/src/state/`。

本轮新增的 `clients/app/scripts/run-browser-check.mjs` 与 `src/platform/testing/browser-checks.ts` 属 WP5b 已登记的浏览器验证入口（MU3b 的归属提示已备查）；WP6 若需扩展浏览器用例，应在这些文件内追加，**不得**新建第二个浏览器 runner，也不得改动 `src/platform/**` 的实现。

## 未声称

- `deps` / `advisories` / `secrets` 三个 CI-only job 本地无等价物，未执行、未声称通过。
- 本文件只证明 worktree 与依赖就位、基线自检全绿；不证明 WP6 的任何交付内容。
