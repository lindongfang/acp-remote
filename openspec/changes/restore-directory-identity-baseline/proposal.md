# Proposal

## Why

**用户原始要求**：

> 作为另一个变更修复。

上一轮落地配对页时，整仓门禁暴露出两组**与配对页无关**的既有失败：

1. `clients/app` 的协议镜像仍停在旧的目录模型（`alias` + `displayName`），而根 `schemas/`、`fixtures/` 与 Rust 端已经改成 `directoryId` + `path`。`npm run check` 因此有 6 条失败：`src/protocol/contract.test.ts` 2 条、`src/sync-client/wire-fixtures.test.ts` 3 条、`src/sync-client/feature-declarations.test.ts` 1 条。
2. Rust 工作区存在既有的 `cargo fmt --check` 差异（例如 `crates/storage-sqlite/tests/workspace_alias.rs`），`npm run verify` 在格式门禁就停下。

归档变更 `2026-10-17-directory-auto-registration` 的 `tasks.md` 把前端 WP4 标成已完成，但代码从未迁移：前端仍按别名渲染目录、`/dir/[alias]` 路由仍在、`session.create` 仍发 `workspaceAlias`、`core.workspace-path.v1` 仍在客户端 feature 广告里。结果是“规范与合同资产已经前进、实现停在旧模型”。本变更按该归档变更的 WP4 把前端补齐，并只做机械格式化修复 Rust 基线；它不引入新的产品行为。

## What Changes

- 前端协议镜像改为当前 wire 形状：
  - `WorkspaceRef` = `{ directoryId }`；
  - `WorkspaceCatalogEntry` = `{ directoryId, path }`（`path` 始终在场，不再受 feature 门控）；
  - 会话摘要的 `workspace` = `{ directoryId } | null`。
- 前端目录模型改为 `directoryId` 唯一标识；展示名由服务端下发的路径尾段派生，同名时用更多路径段消歧；目录标识只作为路由与命令引用，MUST NOT 出现在界面文本里。
- 目录详情路由由 `/dir/[alias]` 改为 `/dir/[directoryId]`，路由参数、链接与测试同步。
- 目录页搜索改为匹配展示名与会话标题，不再提“别名”（目录模型里已经没有别名）。
- 创建会话的目录引用改为 `directoryId` 或 `workspacePath`；`createIfMissing` 与 `directoryId` 互斥。已记录目录的选择器、浏览回退与提交载荷同步。
- 客户端 feature 广告移除 `core.workspace-path.v1`；`core.workspace-browse.v1`、`core.session-create.v1`、`core.local-catalog.v1` 不变。`feature-declarations.test.ts` 增加“该 feature 已撤销”的负向断言。
- e2e 夹具与 R15/R16/路由用例改用 `{ directoryId, path }` 与会话 `{ directoryId }`；期望的展示名改为路径尾段派生值。
- `cargo fmt` 修复既有 Rust 格式差异；同时清理基线恢复后遗留的未用代码/导入，并让 `UseCases::sync_workspace_catalog` 去掉已撤销的 `include_path` 参数（`path` 已始终下发）。不改变任何 Rust 业务语义。
- 修改 `pwa-web-client` 的搜索要求：去掉别名匹配，明确目录标识不参与搜索。
- 同步 `docs/FRONTEND_DESIGN.md` 中仍写“别名/旧路由”的现行口径（如存在）。

## Capabilities

### New Capabilities

（无）

### Modified Capabilities

- `pwa-web-client`：目录页搜索的匹配对象由“展示名、别名、会话标题”改为“展示名（由服务端路径派生）与会话标题”，并明确目录标识不参与搜索。

## Impact

- **前端**：`clients/app/src/protocol/**`、`clients/app/src/sync-client/**`、`clients/app/src/domain/**`、`clients/app/src/features/**`、`clients/app/src/components/**`、`clients/app/app/dir/**`、`clients/app/src/routing.test.ts` 与相关单测。
- **e2e**：`clients/app/e2e/page/**` 的夹具、R15/R16/路由断言。
- **Rust**：仅 `cargo fmt` 的格式变化；无生产逻辑、schema、wire 或存储变更。
- **文档**：`docs/FRONTEND_DESIGN.md`、`README.md`、`prototypes/IMPLEMENTATION-GAPS.md`（如仍有旧目录身份口径）。
- **用户可见**：目录仍以路径尾段命名；目录标识不显示；路由地址里的目录段从别名变成目录标识；搜索不再命中别名。

## 非目标

- 不改变 Sync/Node Link 协议、根 `schemas/`、`fixtures/`、Rust 行为或数据库。
- 不新增目录登记、重命名、删除或手工分组。
- 不改变目录页/详情页/对话页的视觉形态；只把身份与数据来源换到当前合同。
- 不重新设计目录浏览或创建会话的产品行为；只对齐字段名与载荷。
- 不重构与目录身份无关的前端模块。

## 硬约束

- 目录标识 MUST NOT 作为界面文本呈现；路径 MUST 逐字取自服务端下发条目，客户端 MUST NOT 拼接、规范化或反推。
- 展示名与同名消歧 MUST 由服务端下发的路径派生，MUST NOT 使用目录标识代替路径。
- 客户端 MUST NOT 广告已撤销的 `core.workspace-path.v1`；`path` 始终在场。
- 会话创建载荷 MUST 只含 `agentId` 与恰好一个目录引用（`directoryId` 或 `workspacePath`），`createIfMissing` 只与 `workspacePath` 相容。
- `cd clients/app && npm run check`、`npm run verify` 与浏览器 e2e 必须全绿。

## 成功判据

1. `clients/app` 协议镜像与根 schema 逐项一致；6 条既有失败全部转绿。
2. 目录页、详情页、左栏与创建弹层只使用 `directoryId`/`path`；界面不出现目录标识；同名目录用路径段消歧。
3. `/dir/<directoryId>` 可打开；旧 `/dir/<alias>` 不再作为产品路由；路由测试与 e2e 同步。
4. `session.create` 载荷与 `command.schema.json` 一致；`workspace.browse` 回退选择仍可用。
5. 客户端 feature 广告与 `compatibility/features/v1/features.json` 一致，`core.workspace-path.v1` 不再被广告。
6. `npm run verify` 全绿（含 `cargo fmt --check`）；`clients/app npm run check` 全绿；`node e2e/run-e2e.mjs` 全绿。
