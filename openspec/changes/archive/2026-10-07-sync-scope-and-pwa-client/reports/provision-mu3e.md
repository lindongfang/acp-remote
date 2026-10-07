# Provisioner Handoff — TP3 / TP4（MU3e）

- **agent_id**: `ProvisionerW1`
- **fork_turns**: `none`
- **worktrees**: `D:\Project\acp-remote\.worktrees\tp3`（分支 `feat/tp3`）、`D:\Project\acp-remote\.worktrees\tp4`（分支 `feat/tp4`）
- **plan Work Packages**: `TP3`（Owner `testing-3` / Reviewer `review-5` / `wt/tp3`）、`TP4`（Owner `testing-4` / Reviewer `review-7` / `wt/tp4`）
- **baseline**: `f6ea252e09fd65834a67fe7123c786beec339209`（MU3d 合入后的 `refs/heads/main`）

## Enter Condition 核对

`plan.md` 的 Execution Waves 把 TP3 与 TP4 同归 W6：

| 工作包 | Enter Condition | 状态 | 证据 |
| --- | --- | --- | --- |
| TP3 | `code:WP5a`/`WP5b`/`WP6`/`WP7` 已合入（MU3a–MU3d）；`contract:WP1` 已冻结（`fixtures/sync/v1/transcripts/**@WP1`、`schemas/sync/v1/**@WP1`） | ✅ `a4440d6` / `263d3ba` / `4ada2a2` / `f6ea252` 均在 main 祖先链；`schemas/sync/v1/event-views.schema.json`、`fixtures/sync/v1/transcripts/{device-proof,host-challenge}.json` 均存在可读 | `git merge-base --is-ancestor` 逐条成立 |
| TP4 | `code:WP7` 已合入（MU3d） | ✅ `f6ea252` | 同上 |

## 执行记录

| ID | 命令 | 工作目录 | 退出码 | 观察 |
| --- | --- | --- | --- | --- |
| PV-WTT3 | `git worktree add .worktrees/tp3 -b feat/tp3 refs/heads/main` | 仓库根 | 0 | `HEAD is now at f6ea252` |
| PV-WTT4 | `git worktree add .worktrees/tp4 -b feat/tp4 refs/heads/main` | 仓库根 | 0 | `HEAD is now at f6ea252` |
| PV-WTT3-2 | `npm ci` | `.worktrees/tp3/clients/app` | 0 | `added 856 packages` |
| PV-WTT4-2 | `npm ci` | `.worktrees/tp4/clients/app` | 0 | `added 856 packages` |
| PV-WTT3-3 | `npx tsc --noEmit` / `npx vitest run` / `node scripts/run-browser-check.mjs` | `.worktrees/tp3/clients/app` | 0 / 0 / 0 | 无诊断；28 文件 / 359 passed；真实 Chromium 16/16 |
| PV-WTT4-3 | `npx tsc --noEmit` / `npx vitest run` / `node scripts/run-browser-check.mjs` | `.worktrees/tp4/clients/app` | 0 / 0 / 0 | 同上 |

两个 worktree 的开工前基线自检**均全绿**，因此任一包后续的失败都可归因到自身改动。

> **注**：本次 `npm ci` 之前，两个 `clients/app` 目录已随 `git worktree add` 就位，`cd` 均成功。上一次准备阶段曾因 `cd` 失败导致 `npm ci` 误在仓库根执行并破坏根 `node_modules/`（已修复并核验，见 `reports/provision-wp7.md` 的事故登记）。本次执行前后已各自确认工作目录。

## 并发与隔离

- 两个工作包同属 W6、同为 `tester` 角色，`openspec/agentic.yaml` 的 `dispatch.pool.testing = 2`，容量足够，**允许真并发**。
- 两个 worktree 各持**独立**的 `clients/app/node_modules/`；仓库根 `node_modules/` 共享只读。
- 两者的 Write Scope 不重叠：TP3 只写 `clients/app/**/*.test.ts`，TP4 只写 `clients/app/e2e/`（该目录当前**不存在**，需由 TP4 创建）。
- **共享读**：`clients/app/src/**`、`fixtures/sync/v1/**`、`schemas/sync/v1/**`、`docs/**` 两者皆只读。

## Write Scope

| 工作包 | Write Scope |
| --- | --- |
| TP3 | `clients/app/**/*.test.ts` |
| TP4 | `clients/app/e2e/` |

两者**均不得**修改任何实现文件。`plan.md` 的 Shared File Ownership 把 `clients/app/**/*.test.ts` 登记为 TP3 独占、Merge Order 为 `WP7 → TP3`：WP7 在其 Write Scope 内已写入的 8 个测试文件（`r15-*`/`r16-*`/`r17-*`/`r18-*`/`r19-*`/`conversation-paging`/`snapshot-barrier`/`epoch-rebuild-barrier`）构成字面重叠，`review-wp7-r1` §5.2 判为「可接受的阶段边界」并建议 TP3 在其上追加。**TP3 的职责是在这些文件之上补齐契约与状态机覆盖，而不是重写它们。**

## 继承的已登记事项（两个工作包开工时须知）

1. **配对在真实运行时不可达**（`review-wp7-r2` 登记）：无二维码扫描入口，`pairing`/`rememberPairedHost` 无生产调用方，`poll` 的 approved 分支不返回 host 公钥。**TP4 的浏览器验证不得依赖真实配对流程**，应预置 `paired-host/<origin>` 记录后再驱动客户端。
2. **组件测试只在 Node 静态渲染下成立**（`review-wp7-r1` §5.3 核实）：`vitest.config.ts` 的 `include` 只收 `src/**/*.test.ts`，且不允许新增依赖，故组件断言走 `renderToStaticMarkup`——**无事件、无 `useEffect`**。这意味着 `composition.ts` 里 `useEffect` 中的 `await start()` 在既有测试中**从未被执行**，其接线正确性目前无自动化证据，正是 TP4 的用武之地。
3. **三条 MINOR 登记备查**（`review-wp7-r2`）：配对无 UI 入口、`authenticatedInfo` getter 缺 `connected` 门控、`dispose()` 使模块作用域单例不可复活（StrictMode 下的地雷）。
4. **`fakeDigest` 的 FNV-1a 与真实 SHA-256 无共享向量对拍**（`review-wp7-r2` §7.3）：全仓无一条用例让真实 `digestPort` 参与快照校验，归属 TP3 的固定向量对拍。

## 未声称

- `deps` / `advisories` / `secrets` 三个 CI-only job 本地无等价物，未执行、未声称通过。
- 本文件只证明 worktree 与依赖就位、基线自检全绿；不证明 TP3/TP4 的任何交付内容。
