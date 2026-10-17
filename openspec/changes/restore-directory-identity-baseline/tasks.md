# Tasks

## 1. WP1 前端迁移到当前目录身份

- [ ] 1.1 `clients/app/src/protocol/common.ts`：`WorkspaceRef` 改为 `{ directoryId: string }`；`WorkspaceCatalogEntry` 改为 `{ directoryId: string; path: string }`（`path` 必填、不再是 feature 可选键）；`SessionSummary.workspace` 仍为可选/可空，但指向新 `WorkspaceRef`。完成判据：协议镜像的键集合与 `schemas/sync/v1/common.schema.json` 逐项一致，`contract.test.ts` 相关断言通过。
- [ ] 1.2 `clients/app/src/protocol/command.ts`：`session.create` 的目录引用改为 `directoryId` 或 `workspacePath`，`createIfMissing` 只与 `workspacePath` 相容；删除 `workspaceAlias`。完成判据：`contract.test.ts` 的 payload 键集/oneOf 断言通过。
- [ ] 1.3 `clients/app/src/sync-client/wire.ts`：`workspaces` 元素必须含 `directoryId` 与 `path`，拒绝 `alias`/`displayName` 或缺 `path` 的条目；`path` 必须是非空字符串。完成判据：`wire.test.ts` 的正向样例受理、旧形状拒收、缺 path 拒收三条通过。
- [ ] 1.4 `clients/app/src/sync-client/snapshot.ts` 与相关注释：删除 `core.workspace-path.v1` 门控叙述，`path` 始终在场。完成判据：`wire-fixtures.test.ts` 的三个 `*workspaces*.json` 样例全部受理。
- [ ] 1.5 `clients/app/src/sync-client/connection.ts`：`SUPPORTED_FEATURES` 删除 `core.workspace-path.v1`；`feature-declarations.test.ts` 从正向列表删除它，并新增负向断言：机器登记表没有该 ID、客户端不广告它。完成判据：feature 测试通过。
- [ ] 1.6 `clients/app/src/domain/directory.ts`：`DirectoryView` 用 `directoryId` 作为 `id`，删除 `alias`；展示名由 `path` 尾段派生；尾段相同的目录用更多路径段消歧；目录列表按 `session.workspace?.directoryId` 汇总。完成判据：`domain.test.ts` 覆盖尾段展示、同名消歧、未归属会话、断连汇总。
- [ ] 1.7 `clients/app/src/domain/session-summary.ts`：`directoryAlias` 改为 `directoryId`。完成判据：全仓库不再有 `directoryAlias` 引用。
- [ ] 1.8 `clients/app/src/features/directory-model.ts`：目录页/详情/左栏模型字段改为 `directoryId`；搜索只匹配展示名与会话标题，不匹配目录标识；详情定位按 `directoryId`。完成判据：`directory-filter.test.ts`、`directory-detail-paging.test.ts` 与相关组件用例更新并通过。
- [ ] 1.9 `clients/app/src/features/create-session-model.ts` 与 `client-store.ts`：已记录目录选择使用 `directoryId`；`session.create` 载荷用 `directoryId` 或 `workspacePath`；`createIfMissing` 只与路径相容；`registeredDirectories()`、`createEntry()`、目录浏览回退全部改用新字段。完成判据：创建会话相关单测通过，载荷键集与 schema 一致。
- [ ] 1.10 组件与路由：所有目录卡片、详情页、对话页左栏、创建弹层、目录选择器改用 `directoryId`；`app/dir/[alias].tsx` 改为 `app/dir/[directoryId].tsx`，参数名与链接同步；`routing.test.ts` 的样例改为目录标识。完成判据：`routing.test.ts` 通过；`grep` 不到 `[alias]`/`dirRoute` 旧参数。
- [ ] 1.11 清理死代码与旧词汇：删除 `alias`/`displayName` 在目录模型、组件与测试中的残留；不再有 `core.workspace-path.v1`、`workspaceAlias` 的前端引用。完成判据：`grep` 零命中（历史注释/文档除外），`npm run typecheck` 通过。
- [ ] 1.12 全量更新前端单测与夹具：`domain.test.ts`、`directory-filter.test.ts`、`directory-detail-paging.test.ts`、`r16-create-session.test.ts`、`r19-host-panel.test.ts`、`composition.test.ts` 等。完成判据：`cd clients/app && npm run test` 全绿（既有 6 条失败转绿）。

## 2. WP2 浏览器 e2e

- [ ] 2.1 `clients/app/e2e/page/fixtures.tsx`：`WORKSPACES_WITH_PATHS` 改为 `{ directoryId, path }`；会话夹具的 `workspace` 改为 `{ directoryId }`；目录路由夹具改为 `/dir/<directoryId>`。完成判据：e2e 类型检查通过。
- [ ] 2.2 R15 目录页/详情页断言：期望展示名改为路径尾段（同名路径用更多段消歧）；目录标识不出现在文本里；详情路由用目录标识；搜索不再命中标识。完成判据：R15-* 全绿。
- [ ] 2.3 R16 创建断言：载荷键为 `agentId` + `directoryId`（或 `workspacePath` + `createIfMissing`）；已记录目录/浏览回退按新字段；目录块只显示路径。完成判据：R16-* 全绿。
- [ ] 2.4 其它引用目录别名/旧路由的 e2e（router-stub、support、frame-acceptance、composition-root）同步更新。完成判据：`npm run typecheck:e2e` 与 `node e2e/run-e2e.mjs` 全绿（75/75 或等价集合）。

## 3. WP3 文档与 Rust 格式

- [ ] 3.1 `docs/FRONTEND_DESIGN.md` / `docs/SYNC_PROTOCOL.md`：目录搜索改为“展示名 + 会话标题”，删除别名匹配；路由写作 `/dir/<directoryId>`；`WorkspaceRef`/`WorkspaceCatalogEntry` 改为 `{ directoryId }` / `{ directoryId, path }`；如仍写 `displayName`/`alias` 的现行口径一并改为路径派生。完成判据：`npm run check:docs` 通过，现行段落无旧词汇。
- [ ] 3.2 `README.md` / `prototypes/IMPLEMENTATION-GAPS.md`：如仍有目录别名/旧路由的前端现状描述，改为 `directoryId` + 路径派生展示名。完成判据：`grep` 不到前端现行口径的 `workspaceAlias`/`displayName`（历史设计文档除外）。
- [ ] 3.3 在隔离工作区运行 `cargo fmt`，只提交格式差异。完成判据：`cargo fmt --check` 通过；diff 中无逻辑/注释语义变化。
- [ ] 3.4 清理基线恢复后遗留的未用代码/导入，并使 `UseCases::sync_workspace_catalog` 去掉已撤销的 `include_path` 参数（更新 server 调用点与 core 测试）。完成判据：`cargo clippy --workspace --all-targets --all-features -- -D warnings` 通过，`sync_workspace_catalog` 无 `include_path` 残留。
- [ ] 3.5 核对 `openspec/changes/restore-directory-identity-baseline/` 的 artifacts 与最终实现一致。完成判据：`npx openspec validate restore-directory-identity-baseline --strict` 通过。

## 4. 合入与验证

- [ ] 4.1 按 Merge Order 合入 WP1 并跑 PV2；`git apply --check` 通过后才 apply。完成判据：`cd clients/app && npm run check` 全绿（含原 6 条失败转绿）。
- [ ] 4.2 合入 WP3 并跑 PV1；完成判据：`npm run verify` 全绿（含 `cargo fmt --check`）。
- [ ] 4.3 合入 WP2 并跑 PV2/PV3。完成判据：`node e2e/run-e2e.mjs` 全绿。
- [ ] 4.4 更新本 tasks 的完成状态与证据；按 `docs/WORKFLOW.md` §6 提交与 PR。完成判据：三条门禁全绿，PR 检查全绿。
