# Provisioner Handoff — WP7（MU3d）

- **agent_id**: `ProvisionerW1`
- **fork_turns**: `none`
- **worktree**: `D:\Project\acp-remote\.worktrees\wp7`
- **branch**: `feat/wp7`
- **plan Work Package**: `WP7`（Owner `coding-7` / Reviewer `review-7` / Branch / Worktree `wt/wp7`）
- **baseline**: `4ada2a24e23ff3f07d37eb0face70921a66ed987`（MU3c 合入后的 `refs/heads/main`）

## Enter Condition 核对

`plan.md` 的 Execution Waves 记 `W5 | WP7 | code:WP5a/code:WP5b/code:WP6 已合入（MU3a/MU3b/MU3c），schemas/sync/v1/event-views.schema.json@WP2 已冻结 | code-dependency`。

| 前置 | 状态 | 证据 |
| --- | --- | --- |
| `code:WP5a` 已合入 | ✅ `a4440d6`（MU3a） | `git merge-base --is-ancestor a4440d6 refs/heads/main` 成立 |
| `code:WP5b` 已合入 | ✅ `263d3ba`（MU3b） | 同上 |
| `code:WP6` 已合入 | ✅ `4ada2a2`（MU3c，本次基线） | `git -C .worktrees/wp7 rev-parse HEAD` == `refs/heads/main` |
| `schemas/sync/v1/event-views.schema.json@WP2` 已冻结 | ✅ 文件可读 | `ls schemas/sync/v1/event-views.schema.json` |

## 执行记录

| ID | 命令 | 工作目录 | 退出码 | 观察 |
| --- | --- | --- | --- | --- |
| PV-WT7 | `git worktree add .worktrees/wp7 -b feat/wp7 refs/heads/main` | `D:\Project\acp-remote` | 0 | `HEAD is now at 4ada2a2` |
| PV-WT7-2 | `git -C .worktrees/wp7 rev-parse HEAD` / `rev-parse refs/heads/main` | `D:\Project\acp-remote` | 0 | 两侧同为 `4ada2a24e23ff3f07d37eb0face70921a66ed987` |
| PV-WT7-3 | `npm ci` | `.worktrees/wp7/clients/app` | 0 | `added 856 packages`；独立 `node_modules/` 就位（435 顶层目录） |
| PV-WT7-4 | `npx tsc --noEmit` | `.worktrees/wp7/clients/app` | 0 | 无诊断 |
| PV-WT7-5 | `npx vitest run` | `.worktrees/wp7/clients/app` | 0 | `Test Files 17 passed (17)` / `Tests 269 passed (269)` |
| PV-WT7-6 | `node scripts/run-browser-check.mjs` | `.worktrees/wp7/clients/app` | 0 | 真实 Chromium `16/16 passed` |

开工前基线自检（PV-WT7-4/5/6）确认 WP5a/WP5b/WP6 的继承代码在独立依赖安装下仍然全绿，WP7 的任何失败都可归因到本包自身。

## 一次环境事故与修复（据实登记）

provisioner 执行期间发生了一次**由 provisioner 自己造成**的环境事故，如实记录：

1. 首次执行 `cd .worktrees/wp7/clients/app && npm ci` 时，`cd` 因路径尚未就绪而失败（`cd: i/o error`），`npm ci` 遂在**仓库根**执行；
2. 根 `npm ci` 在清理阶段被一个**残留的 `tsc.exe` 进程**（PID 228308，来自更早的客户端类型检查）阻塞，Windows 返回 `EPERM: unlink`，`npm ci` 中途失败；
3. 结果：仓库根 `node_modules/` 处于**不完整状态**——`node_modules/.bin/` 整体缺失，`@dongfanglin/openspec-agentic/bin/` 亦缺失，导致 `openspec-agentic dispatch` 无法解析。

**修复**：终止残留进程 PID 228308 → 在仓库根重跑 `npm ci` → `added 154 packages`，`.bin/`（33 项）与扩展 `bin/` 目录均恢复。

**修复后核验**：`npm run check` **22/22 全绿**（十道合同门禁含 `check:agentic`）；`git status --porcelain` 仅余三个预期未跟踪项；`refs/heads/main` 仍为 `4ada2a2`；13 个 worktree 全部完好；`.worktrees/wp7/clients/app/node_modules` 独立安装未受影响。

**影响评估**：事故仅波及**本地依赖目录**，未触及任何被版本控制的内容、未改动规划文件、未影响已合入的 MU3a/MU3b/MU3c。`node_modules/` 不在版本控制内，修复后与修复前等价。

**遗留风险**：`npm ci` / `rm -rf node_modules` 在本机可能被残留的 `tsc.exe` 阻塞。后续任何执行者在跑依赖安装前应先确认无遗留 `node_modules` 进程。

## 隔离

- 仓库根 `node_modules/` **共享只读**（不改、不装）。上条事故的教训：任何 `npm ci` 都必须确认 `cwd` 确实是目标目录。
- `clients/app/node_modules/` 为本 worktree **独占**，由 provisioner 一次性安装，执行者不得再改动依赖清单。
- 未创建 `.native.ts`（v1 只交付 Web，出自 tasks 2.6 的裁定）。
- 主检出 `D:\Project\acp-remote` 在本单元内**只读**：禁止在其中执行 `git merge` / `checkout` / `reset`。

## Write Scope 与一条已授权的例外

**标准 Write Scope**（`plan.md` 的 WP7 行）：`clients/app/app/`、`clients/app/src/features/`、`clients/app/src/components/`。

**已授权例外**：执行者**还必须**修复 MU3c 随主分支带入的两项 P1 缺陷，它们位于 `clients/app/src/sync-client/`（WP6 的 Write Scope）：

| ID | 内容 | 位置 |
| --- | --- | --- |
| P1-N1 | 快照屏障被账本拒绝后，门面 `#ackedCursor` 仍被无条件覆盖 | `client.ts:261-265` 早于 `connection.ts:699` 的 `advanceTo` 生效 |
| P1-N2 | 该越界游标进入下次连接的构造期 `resumeFrom`，`EventLedger` 抛错 → 同步永久卡死 | `dedupe.ts:176-178` ← `client.ts:245-247` ← `connection.ts:426` |

**例外依据**：用户在 MU3c 合入时裁决「按 reviewer 建议带 2 项 P1 合入」，并在 `verification.md` 的 `## Merge History` MU3c 行与 `## Review Findings` RF-12 的 Resolution 列把归属写为「WP7 接线轮修复」。该例外是**用户裁决的直接产物**，不是执行者的自行扩范围。

**为什么不改 plan.md 登记这条例外**：`contractDigest` 覆盖 `plan.md`，改动会使 DDR Round 13 绑定的摘要失效、须补 Round 14 重开一轮计划审查；代价大于收益。本仓库对同类情形有一致先例（MU3b 的 `clients/app/scripts/run-browser-check.mjs` 同样在 Write Scope 之外，以「登记备查」处置）。因此本例外登记在 `verification.md`，**不改 plan.md**。

**无并发写者**：WP6 已于 MU3c 合入主分支，`feat/wp6` 不再推进；本单元对 `src/sync-client/` 的写入是**顺序扩展**，不存在重叠写冲突。

## 交接对象

| 字段 | 值 |
| --- | --- |
| Work Package | WP7 |
| Attempt | 1 |
| Executor | `coder-w7-r1` |
| Received At | 2026-10-05T13:37:00Z |
| Evidence | `reports/provision-wp7.md` |
| Dispatch 记录 | `dispatch-queue.jsonl`：`{"wp":"WP7","role":"coder","executor":"coder-w7-r1","attempt":1,"state":"coding"}` |

## 未声称

- `deps` / `advisories` / `secrets` 三个 CI-only job 本地无等价物，未执行、未声称通过。
- 本文件只证明 worktree 与依赖就位、基线自检全绿，以及一次环境事故已修复并核验；不证明 WP7 的任何交付内容。
