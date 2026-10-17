# Proposal

## Why

根 `schemas/sync/v1/`、`fixtures/sync/v1/` 与 Rust 端（`sync-protocol`、`server::sync`、`core`）已经使用新的目录身份：快照 `workspaces` 条目是 `{ directoryId, path }`，会话摘要的 `workspace` 是 `{ directoryId } | null`，`session.create` 的目录引用是 `directoryId` 或 `workspacePath`。

归档变更 `2026-10-17-directory-auto-registration` 的 `tasks.md` 把前端 WP4 标成已完成，但 `clients/app` 仍停在旧模型：协议镜像按 `alias` + `displayName` 解码、`/dir/[alias]` 路由仍在、`session.create` 仍发 `workspaceAlias`、`core.workspace-path.v1` 仍在客户端 feature 广告里。`clients/app npm run check` 因此有 6 条失败：`src/protocol/contract.test.ts` 2 条、`src/sync-client/wire-fixtures.test.ts` 3 条、`src/sync-client/feature-declarations.test.ts` 1 条。

本变更按归档变更的 WP4 把前端补齐，不引入新的产品行为，也不改协议、schema、Rust 或存储。

## What Changes

- 前端协议镜像改为当前 wire 形状：`WorkspaceRef = { directoryId }`、`WorkspaceCatalogEntry = { directoryId, path }`；会话摘要 `workspace` 仍为可选/可空，但只含目录标识。
- 目录模型改为 `directoryId` 唯一标识；展示名由服务端下发的路径尾段派生，同名时用更多路径段消歧；目录标识只作为路由与命令引用，MUST NOT 出现在界面文本里。
- 目录详情路由由 `/dir/[alias]` 改为 `/dir/[directoryId]`；路由参数、链接与测试同步。
- 目录页搜索改为匹配展示名与会话标题，不再提“别名”；规范增量同时明确目录标识不参与搜索。
- 创建会话的目录引用改为 `directoryId` 或 `workspacePath`；`createIfMissing` 与 `directoryId` 互斥。
- 客户端 feature 广告移除 `core.workspace-path.v1`；`feature-declarations.test.ts` 增加“该 feature 已撤销”的负向断言。
- e2e 夹具与 R15/R16/路由用例改用 `{ directoryId, path }` 与会话 `{ directoryId }`；期望展示名改为路径尾段派生值。
- 同步 `docs/FRONTEND_DESIGN.md`、`docs/SYNC_PROTOCOL.md`、`docs/CORE_PORTS_AND_STORAGE.md`、`docs/MODULE_ARCHITECTURE.md` 与缺口清单中仍写旧目录身份/旧 feature 的现行口径。

## Capabilities

### New Capabilities

（无）

### Modified Capabilities

- `pwa-web-client`：目录页搜索的匹配对象由“展示名、别名、会话标题”改为“展示名（由服务端路径派生）与会话标题”，并明确目录标识不参与搜索。

## Impact

- **前端**：`clients/app/src/protocol/**`、`clients/app/src/sync-client/**`、`clients/app/src/domain/**`、`clients/app/src/features/**`、`clients/app/src/components/**`、`clients/app/app/dir/**`、`clients/app/src/routing.test.ts` 与相关单测。
- **e2e**：`clients/app/e2e/page/**` 的夹具、R15/R16/路由断言。
- **文档**：`docs/FRONTEND_DESIGN.md`、`docs/SYNC_PROTOCOL.md`、`docs/CORE_PORTS_AND_STORAGE.md`、`docs/MODULE_ARCHITECTURE.md`、`prototypes/IMPLEMENTATION-GAPS.md`、`README.md`（如仍有旧前端口径）。
- **Rust**：无变更。

## 非目标

- 不改变 Sync/Node Link 协议、根 `schemas/`、`fixtures/`、Rust 行为或数据库。
- 不新增目录登记、重命名、删除或手工分组。
- 不改变目录页/详情页/对话页的视觉形态；只把身份与数据来源换到当前合同。
- 不重新设计目录浏览或创建会话的产品行为；只对齐字段名与载荷。

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
6. `npm run verify`、`clients/app npm run check`、`node e2e/run-e2e.mjs` 全部通过。
