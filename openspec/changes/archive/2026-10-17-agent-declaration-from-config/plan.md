# Plan

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出多个工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）；工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；不产生 commit 或分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`，每次合入后跑主检出 Project Verify。
- 验证：仓库级检查只在主检出执行；隔离区没有 `target/` 与 `node_modules/`。
- 检视：每个工作包由独立 `reviewer` spawn 检视 patch 与报告。
- **前置**：变更 A（`directory-auto-registration`）必须先归档，否则存储版本与主规格基线都不对。

## Work Packages

| ID | Goal | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 配置模型：`[node]`/`[agent]` 解析与校验、profile 来源改为配置、去掉 Provider 端口与解析 | A 已归档 | wp-coder | `crates/app/src/config.rs`, `crates/core/` | patch + 配置解析/校验用例 | 主检出 `cargo test -p core -p app` |
| WP2 | agent-host：profile 来自注入端口、固定必要环境变量、删除绑定解析与保留前缀拒绝保留 | WP1 | wp-coder | `crates/agent-host/` | patch + 启动规格与环境集合用例 | 主检出 `cargo test -p agent-host` |
| WP3 | 存储：v10 删除两张表与相关 store | WP1 | wp-coder | `crates/storage-sqlite/` | patch + 迁移用例 | 主检出 `cargo test -p storage-sqlite` |
| WP4 | 管理面与 CLI：删除两个方法/命令、新增 `agent.list` / `agent list` | WP1, WP3 | wp-coder | `crates/server/`, `crates/app/` | patch + 方法集与 CLI 用例 | 主检出 `cargo test -p server -p app` |
| WP5 | 文档：CONFIG_REFERENCE、LOCAL_ADMIN、MODULE_ARCHITECTURE、CORE_PORTS、SECURITY_DESIGN、README | WP2, WP4 | wp-coder | `docs/`, `README.md` | patch + 文档门禁 | 主检出 `npm run check:docs` |
| WP6 | 与 A 重叠的 delta 补写：`local-admin-methods`、`cli-commands`、`storage-schema-v2-migration` | A 已归档、本变更的 specs 已落地 | wp-coder | `openspec/changes/agent-declaration-from-config/specs/` | patch + `openspec validate --strict` | 主检出 `npm run check:openspec` |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 无重叠 | `cargo test -p core -p app` |
| 2 | WP3 | 与 WP1 无文件重叠；存储版本依赖 WP1 的值对象 | `cargo test -p storage-sqlite` |
| 3 | WP2 | 依赖 WP1 的 profile 形状 | `cargo test -p agent-host` |
| 4 | WP4 | 依赖 WP1 与 WP3 | `cargo test -p server -p app` |
| 5 | WP5 | 文档反映已落地行为 | `npm run check:docs` |
| 6 | WP6 | 只在 A 归档后写，且必须在实现合入前完成 | `npm run check:openspec` |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cargo test -p core -p app -p agent-host -p storage-sqlite`（主检出根） | 配置解析、环境注入、v10 迁移、方法集用例全绿 |
| PV2 | `cargo test -p server`（主检出根） | 删除的方法返回 `local.unsupported`，`agent.list` 返回配置条目 |
| PV3 | `npm run check`（主检出根） | 十项门禁全绿 |
| PV4 | `cd clients/app && npm run check`（主检出 `clients/app`） | 前端不受影响（Agent 目录形状不变） |
| PV5 | 端到端验收（见 Main E2E） | 真实 Daemon：改配置 → 重启 → 浏览器 Agent 列表随之变化 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 非法条目是否启动失败关闭；是否彻底移除 Provider 端口与解析；默认唯一性 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 环境集合是否恰为固定五项+PATH；是否仍静态拒绝保留前缀；是否不再读任何凭据 | 同上 |
| WP3 | reviewer | 迁移是否只删两张表、失败整体回滚、审计行保留 | 同上 |
| WP4 | reviewer | 删除是否干净；`agent list` 是否只读、不启动进程、不复制配置解析规则 | 同上 |
| WP5 | reviewer | 文档是否写成已落地；升级影响（Agent 不自动搬运）是否写明 | 同上 |
| WP6 | reviewer | delta 是否与 A 归档后的主规格逐字匹配、场景无丢失 | 同上 |

## Main E2E

```yaml
mode: required
reason: 用户可见变化（Agent 列表来自配置文件）需要浏览器级证据；纯 CLI 断言不足以证明目录页/选择器随之变化。
basis: docs/WORKFLOW.md §7（Web/PWA 用 browser prelude）。
alternative_checks: []
```

验收清单：

1. 在配置文件中删除一个 Agent → 重启 → 浏览器目录页/创建选择器不再出现它；
2. 新增一个 Agent（含 argv 形态）→ 重启 → 选择器出现它，`agent list` 输出与配置一致；
3. 把 `default` 指向不存在的 id → `daemon start` 明确失败并指出键名；
4. `agent configure` / `provider configure` 返回未知子命令；
5. 子进程环境中没有 `ACPR_*` / `ACP_REMOTE_*` 变量（用 fake Agent 的 `--dump-env` 断言）。
