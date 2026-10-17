# Plan

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出多个工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）；工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；不产生 commit 或分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`，每次合入后跑主检出 Project Verify。
- 验证：仓库级检查只在主检出执行。
- 检视：每个工作包由独立 `reviewer` spawn 检视。
- **前置**：变更 A 与 B 必须先归档（存储 v9/v10 与主规格基线）。

## Work Packages

| ID | Goal | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP0 | 按 A/B 归档后的主规格补写本变更的重叠 delta | A、B 已归档 | wp-coder | `openspec/changes/retire-export-import-and-node-grants/specs/` | patch + `openspec validate --strict` | 主检出 `npm run check:openspec` |
| WP1 | core 与 identity：删除 Export/Import 用例、grant 参数与授权展开 | WP0 | wp-coder | `crates/core/`, `crates/identity-auth/` | patch + 用例 | 主检出 `cargo test -p core -p identity-auth` |
| WP2 | 协议资产与 DTO：删除 grant/export/import 字段与命令 | WP0 | wp-coder | `crates/sync-protocol/`, `crates/node-link-protocol/`, `schemas/`, `compatibility/`, `fixtures/` | patch + 合同用例 | 主检出 `cargo test -p sync-protocol -p node-link-protocol` + `npm run check` |
| WP3 | 存储 v11：删表、删列、imported 家族去 export 维度与合并迁移 | WP1 | wp-coder | `crates/storage-sqlite/` | patch + 迁移用例 | 主检出 `cargo test -p storage-sqlite` |
| WP4 | 入站面：删除 Export/Import 方法、catalog 不过滤、命令不做 grant 判定 | WP1, WP2, WP3 | wp-coder | `crates/server/` | patch + 协议级用例 | 主检出 `cargo test -p server` |
| WP5 | CLI：删除六个子命令与三个参数 | WP4 | wp-coder | `crates/app/` | patch + CLI 用例 | 主检出 `cargo test -p app` |
| WP6 | 文档：威胁模型、Node Link、本地管理、Sync、core/storage、配置参考、README | WP4, WP5 | wp-coder | `docs/`, `README.md` | patch + 文档门禁 | 主检出 `npm run check:docs` |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP0 | 只改 openspec；必须先与 A/B 的主规格对齐 | `npm run check:openspec` |
| 2 | WP1 | 无重叠 | `cargo test -p core -p identity-auth` |
| 3 | WP2 | 依赖 WP1 删掉的值对象；与 WP1 无文件重叠 | `npm run check` |
| 4 | WP3 | 依赖 WP1 的模型 | `cargo test -p storage-sqlite` |
| 5 | WP4 | 依赖 WP1–WP3 | `cargo test -p server` |
| 6 | WP5 | 依赖 WP4 删掉的方法 | `cargo test -p app` |
| 7 | WP6 | 文档反映已落地行为 | `npm run check:docs` |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cargo test -p core -p identity-auth -p storage-sqlite`（主检出根） | grant 展开删除、Export/Import 用例删除、v11 迁移与合并规则用例全绿 |
| PV2 | `cargo test -p server -p app`（主检出根） | catalog 不过滤、命令无 grant 判定、六个子命令不再存在 |
| PV3 | `npm run check`（主检出根） | 十项门禁全绿（命令目录/feature/schema/fixture/文档） |
| PV4 | `cd clients/app && npm run check`（主检出 `clients/app`） | 前端不因 imported 字段变化而回归 |
| PV5 | 协议级端到端：`cargo test -p app --test node_link_e2e` | 配对后无 grant 即可读写；撤销即时生效 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP0 | reviewer | delta 是否逐字对上新主规格、场景无丢失 | 无 CRITICAL/MAJOR 未闭环项 |
| WP1 | reviewer | 是否彻底删除授权展开与 Export/Import 用例；`local.*` 边界是否仍在 | 同上 |
| WP2 | reviewer | 合同资产是否四处一致（commands/schema/docs/fixture）；是否残留 `grant.` | 同上 |
| WP3 | reviewer | imported 合并规则是否正确、是否产生孤儿投递索引；12-step 重建是否失败回滚 | 同上 |
| WP4 | reviewer | catalog 是否真的不过滤；命令是否仍保留上限/幂等/终态；撤销是否仍即时 | 同上 |
| WP5 | reviewer | 删除是否干净；帮助输出无残留 | 同上 |
| WP6 | reviewer | 威胁模型是否写实"配对即完全信任"；是否改写无关内容 | 同上 |

## Main E2E

```yaml
mode: not-applicable
reason: 本变更的可观察面在跨节点协议与本地管理面，浏览器端不经过 Export/Import；真实双节点部署不在本次验收环境内。
basis: docs/WORKFLOW.md §7 允许在缺少可运行用户入口时用 alternative_checks；本仓库已有协议级端到端用例（真实 listener + 脚本化 Access 客户端）。
alternative_checks:
  - cargo test -p app --test node_link_e2e
  - cargo test -p server
  - npm run check
```
