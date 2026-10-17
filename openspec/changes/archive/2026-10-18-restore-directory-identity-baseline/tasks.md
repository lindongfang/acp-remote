# Tasks

## 1. WP1 前端迁移到当前目录身份

- [x] 1.1 `clients/app/src/protocol/common.ts`：`WorkspaceRef` 改为 `{ directoryId: string }`；`WorkspaceCatalogEntry` 改为 `{ directoryId: string; path: string }`；会话摘要 `workspace` 指向新 `WorkspaceRef`。完成判据：协议镜像键集合与根 schema 逐项一致。
- [x] 1.2 `clients/app/src/protocol/command.ts`：`session.create` 的目录引用改为 `directoryId` 或 `workspacePath`；`createIfMissing` 只与 `workspacePath` 相容；删除 `workspaceAlias`。完成判据：`contract.test.ts` payload 断言通过。
- [x] 1.3 `clients/app/src/sync-client/wire.ts`：`workspaces` 元素必须含 `directoryId` 与非空 `path`，旧 `alias`/`displayName` 形状按 `protocol.schema_invalid`/`missing_field` 拒收。完成判据：`wire.test.ts` 正向/负向断言通过。
- [x] 1.4 `clients/app/src/sync-client/snapshot.ts`：`path` 始终在场；删除 `core.workspace-path.v1` 门控叙述。完成判据：三个 `*workspaces*.json` 样例全部受理。
- [x] 1.5 `clients/app/src/sync-client/connection.ts`：`SUPPORTED_FEATURES` 删除 `core.workspace-path.v1`；`feature-declarations.test.ts` 增加负向断言。完成判据：feature 测试通过。
- [x] 1.6 `clients/app/src/domain/directory.ts`：`DirectoryView` 以 `directoryId` 为 `id`，删除 `alias`；展示名由 `path` 尾段派生；同名用更多路径段消歧。完成判据：`domain.test.ts` 覆盖尾段展示、同名消歧、未归属会话、断连汇总。
- [x] 1.7 `clients/app/src/domain/session-summary.ts`：`directoryAlias` 改为 `directoryId`。完成判据：无 `directoryAlias` 残留。
- [x] 1.8 `clients/app/src/features/directory-model.ts`：目录页/详情/左栏字段改为 `directoryId`；搜索只匹配展示名与会话标题；详情按 `directoryId` 定位。完成判据：目录相关单测通过。
- [x] 1.9 `create-session-model.ts` 与 `client-store.ts`：选择/提交/浏览回退全部改用 `directoryId` 或 `workspacePath`。完成判据：创建会话单测与载荷键集断言通过。
- [x] 1.10 组件与路由：所有目录卡片、详情、左栏、创建弹层、选择器改用 `directoryId`；`app/dir/[alias].tsx` 改为 `app/dir/[directoryId].tsx`。完成判据：`routing.test.ts` 通过。
- [x] 1.11 清理死代码与旧词汇：目录模型/组件/测试中不再有 `alias`/`displayName`/`workspaceAlias`/`core.workspace-path.v1` 的前端引用（历史注释除外）。完成判据：`grep` 零命中，`npm run typecheck` 通过。
- [x] 1.12 更新全部前端单测与夹具。完成判据：`cd clients/app && npm run test` 全绿（原 6 条失败转绿）。

## 2. WP2 浏览器 e2e

- [x] 2.1 `fixtures.tsx`：目录夹具改为 `{ directoryId, path }`，会话夹具改为 `{ directoryId }`，路由夹具改为 `/dir/<directoryId>`。完成判据：e2e typecheck 通过。
- [x] 2.2 R15 目录页/详情断言：展示名 = 路径尾段；同名尾段用更多段消歧；目录标识不出现在文本；搜索不命中标识。完成判据：R15-* 全绿。
- [x] 2.3 R16 创建断言：载荷为 `agentId` + `directoryId`（或 `workspacePath` + `createIfMissing`）；目录块只呈现路径。完成判据：R16-* 全绿。
- [x] 2.4 其它引用目录/旧路由的 e2e 同步更新。完成判据：`node e2e/run-e2e.mjs` 全绿。

## 3. WP3 文档

- [x] 3.1 `docs/FRONTEND_DESIGN.md`：目录身份、展示名派生、搜索去别名、`/dir/<directoryId>` 更新；新增修订记录。完成判据：现行段落无旧目录身份口径。
- [x] 3.2 `docs/SYNC_PROTOCOL.md` / `docs/CORE_PORTS_AND_STORAGE.md` / `docs/MODULE_ARCHITECTURE.md`：`WorkspaceRef`/`WorkspaceCatalogEntry` 形状、`sync_workspace_catalog(actor)` 签名、路径门控撤销与目录标识引用更新。完成判据：`npm run check:docs` 通过。
- [x] 3.3 `README.md` / `prototypes/IMPLEMENTATION-GAPS.md`：目录现行口径与缺口清单更新。完成判据：`grep` 不再命中旧的目录前端口径（历史设计文档除外）。
- [x] 3.4 核对本变更 artifacts 与实现一致。完成判据：`npx openspec validate restore-directory-identity-baseline --strict` 通过。

## 4. 验证

- [x] 4.1 `cd clients/app && npm run check`：通过（typecheck、`typecheck:e2e`、vitest、`expo export`）。完成判据：原 6 条失败转绿。
- [x] 4.2 `node e2e/run-e2e.mjs`：**74/74 passed**（本分支的配对页仍是旧版；配对页变更在后续 PR 中替换为 75 条）。完成判据：全绿。
- [x] 4.3 `npm run verify`：通过（合同门禁 + Rust fmt/clippy/test 全绿）。完成判据：全绿。
- [ ] 4.4 推送分支与创建 PR；PR 检查全绿。完成判据：六个必需检查通过。
