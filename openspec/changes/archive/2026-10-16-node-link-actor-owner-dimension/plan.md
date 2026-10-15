# Plan

## Execution Model

沿用仓库既有原语（`docs/WORKFLOW.md`）：父 Agent 用 `task` 批量派发；工作包在 `isolated` 工作区执行，patch 模式（`apply: false`），产物是 `<id>.patch` + `agent://<id>` + `history://<id>`；父 Agent 在主检出**串行** `git apply` 合入；`cargo test` / `npm run check` / `npm run verify` 只在主检出跑（隔离区没有 `target/`，也没有 `node_modules/`）。

## Work Packages

| # | Agent | Write Scope | Dependencies | 交付与判据 |
|---|---|---|---|---|
| WP1 | `wp-coder` | `crates/server/src/node_link/**` | none | 按 design D1 改 `node_actor` 与方法化签名；八个调用点随之更新（签名变更使漏改处无法编译）；`poll_pending` 的 `node` 变量改名。既有按 `a/a` 文本断言的用例逐条重跑。不 bump `FILE_FORMAT_VERSION`（D3）。 |
| WP1-review | `reviewer` | read-only（输入：WP1 的 patch + `agent://WP1` + `history://WP1`） | WP1 | 与 WP1 作者是不同 spawn。重点：八处是否一致、是否误改 wire、D2 的决定是否被实现绕过（不得偷偷写迁移）。 |

两个包的写范围不同（一个写代码、一个只读），因此 Merge Order 无需排序。

## Merge Order

1. WP1（合入后主检出跑 Project Verify）。

## Project Verify

合入 WP1 后**只在主检出**执行：

- `cargo test -p server --offline`（含 `node_link` 全部用例）
- `cargo test -p app --offline --lib`（组合根用例触达 `ForkedPublisher` 与装配）
- `npm run check`（`check:drift` 断言 §5/§7 —— 本变更不改端口签名与 DDL，必须仍全绿）
- `npm run check:rust`（fmt + clippy `-D warnings` + 全测试）

## Code Review

| 包 | Reviewer | 输入 | 判据 |
|---|---|---|---|
| WP1 | `reviewer`（独立 spawn） | WP1 的 patch + 其 `agent://`/`history://` | 见 WP1-review 行的重点；另需至少一个对抗实验并报告红/绿（例如把 `node_actor` 改回两半同值，断言必须变红） |

## Main E2E

**not-applicable**。

- **reason**：本变更改的是持久化 actor 键的**填法**，没有可运行的用户可见入口；Node Link 至今没有真实远程对端数据，`node-link-client` 也未落地。
- **basis**：可观察面只在数据库行与读面投影（`sync_replay` 的 `origin.nodeId`），两者都由单元/集成用例直接断言。
- **alternative_checks**：WP1 的真实用例（同一 requestId 下两条不同对端命令互不覆盖 + `origin.nodeId` 指向对端）、`cargo test -p server` 与 `-p app --lib` 全绿、`npm run check` 全绿；合入后由父 Agent 按 PV5 在主检出复跑。
