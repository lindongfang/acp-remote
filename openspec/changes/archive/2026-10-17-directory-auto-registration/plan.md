# Plan

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出多个工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）；工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；工作包不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`，每次合入后跑主检出 Project Verify。
- 验证：隔离工作区内**没有** `target/` 与 `node_modules/`，仓库级检查只在主检出执行。
- 检视：每个工作包由**独立** spawn（`reviewer`）读 patch 与 `agent://<id>` 后给出结构化结论。

## Work Packages

| ID | Goal | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | core 值对象/端口/用例与 storage 迁移：目录 id、自动 upsert、归属列改名、v8→v9 | none | wp-coder | `crates/core/`, `crates/storage-sqlite/` | patch + 目录 id 派生与迁移用例 | 主检出 `cargo test -p storage-sqlite -p core`（crate 名以 `cargo metadata` 为准） |
| WP2 | 协议与入站面：快照条目、会话摘要引用、`session.create` 载荷、Node Link 校验、删 `workspace.select` 方法、合同资产对齐 | WP1 | wp-coder | `crates/sync-protocol/`, `crates/node-link-protocol/`, `crates/server/`, `schemas/`, `compatibility/`, `fixtures/` | patch + 协议投影与命令校验用例 | 主检出 `cargo test -p server` + `npm run check` |
| WP3 | CLI：删除 `workspace` 命令组及其参数校验；其余命令不受影响 | WP2 | wp-coder | `crates/app/` | patch + CLI 参数解析用例 | 主检出 `cargo test -p app` |
| WP4 | 前端：`/dir/<directoryId>`、目录页/详情页/左栏数据源、目录已删除态、空态文案 | WP2 | wp-coder | `clients/app/` | patch + 视图模型与浏览器检查用例 | 主检出 `cd clients/app && npm run check` |
| WP5 | 文档同步：SYNC/NODE_LINK/CORE_PORTS/FRONTEND/LOCAL_ADMIN/CONFIG/README | WP2, WP4 | wp-coder | `docs/`, `README.md` | patch + 文档门禁 | 主检出 `npm run check:docs` |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 无重叠 | `cargo test -p storage-sqlite -p core` |
| 2 | WP2 | 依赖 WP1 的值对象与列；与 WP1 无文件重叠 | `cargo test -p server` + `npm run check` |
| 3 | WP3 | 依赖 WP2 删掉的管理方法 | `cargo test -p app` |
| 4 | WP4 | 依赖 WP2 的 wire 形状 | `cd clients/app && npm run check` |
| 5 | WP5 | 文档必须反映已落地的行为 | `npm run check:docs` |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cargo test -p storage-sqlite -p core`（主检出根） | 目录 id 派生、upsert 幂等、v8→v9 迁移与历史会话归组用例全绿 |
| PV2 | `cargo test -p server -p app`（主检出根） | 快照投影、命令校验、Node Link 拒绝路径、CLI 不再接受 `workspace` 子命令 |
| PV3 | `npm run check`（主检出根） | 十项合同门禁全绿（schema/fixture、feature 词表、命令目录一致性、文档引用） |
| PV4 | `cd clients/app && npm run check`（主检出 `clients/app`） | typecheck + vitest + `expo export` 全绿 |
| PV5 | 端到端验收（见 Main E2E） | 真实 Daemon + 真实浏览器：任意目录建会话 → 目录自动出现 → 删目录后历史可读 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | id 派生是否真的做了规范化（大小写/链接）；upsert 是否与会话创建同事务；迁移是否保留既有行且失败整体回滚 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 条目/引用/载荷三处形状是否与 delta specs 逐条一致；Node Link 是否仍拒绝任何路径；feature 撤销是否在所有登记处同步 | 同上 |
| WP3 | reviewer | 删除是否干净（无残留子命令/参数/错误分支）；错误输出契约不变 | 同上 |
| WP4 | reviewer | 是否只按 `path` 与 `directoryId` 渲染、不再出现别名；目录已删除态是否可用且不隐藏历史；路由变化是否覆盖 | 同上 |
| WP5 | reviewer | 文档是否写成已落地；是否有与本变更无关的改写 | 同上 |

## Main E2E

```yaml
mode: required
reason: 本变更改变用户可见的目录行为（目录自动出现、目录删除后的历史可读），必须用 browser prelude 对真实 Daemon 断言可观察行为；假主机用例只作回归。
basis: docs/WORKFLOW.md §7 要求按被测面选 prelude（Web/PWA 用 browser prelude）。
alternative_checks: []
```

验收清单（主 Agent 必须给出实测输出）：

1. 全新数据目录、不执行任何登记命令：经「⊕ 新建 → 选择文件夹」在任意目录建会话成功；
2. 会话创建后目录页出现该目录，`directoryId` 与快照条目一致；同一目录再建一条会话不新增目录项；
3. 整页重载后目录与左栏仍按该目录归组（不再出现「没有目录归属」）；
4. 删除该磁盘目录后重载：目录项显示「目录已不存在」，历史会话仍可打开，新建入口禁用或明确报错；
5. 远程通道（Node Link）用未登记 `directoryId` 建会话被拒，携带任意路径被拒。
