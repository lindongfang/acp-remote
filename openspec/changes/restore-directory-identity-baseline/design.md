# Design

## Context

根 `schemas/sync/v1/common.schema.json` 与 `command.schema.json` 已经描述新的目录身份：快照 `workspaces` 条目是 `{ directoryId, path }`，会话摘要的 `workspace` 是 `{ directoryId } | null`，`session.create` 的目录引用是 `directoryId` 或 `workspacePath`。Rust 端（`sync-protocol`、`server::sync`、`core`）同样使用 `DirectoryId`。归档变更 `2026-10-17-directory-auto-registration` 的规范也已写入这套模型，但其前端 WP4 只被勾选、没有被实现：`clients/app` 仍按 `alias` + `displayName` 解码、分组、渲染与提交。

当前前端失败集中在三处：协议镜像的键集合与 schema 不一致；解码边界拒绝新样例；客户端仍广告已撤销的 `core.workspace-path.v1`。另外 Rust 工作区有既有的 `cargo fmt --check` 差异。两者都与配对页无关，需要独立收口。

## Goals / Non-Goals

**Goals:**

- 让前端协议镜像、领域模型、页面、路由、命令载荷与当前 wire/schema 完全一致。
- 让 `clients/app npm run check`、`npm run verify`、浏览器 e2e 全绿。
- 保持目录页/详情页/左栏/创建弹层的现有交互与视觉，只替换身份来源。
- 用 `cargo fmt` 机械修复 Rust 格式，不改变行为。

**Non-Goals:**

- 不改变根 schema、fixtures、Rust 生产逻辑、数据库或协议语义。
- 不新增目录登记/重命名/删除/分组。
- 不重设计目录浏览、创建会话或搜索的产品行为。
- 不处理其它历史文档中与目录身份无关的陈旧描述（除非它直接描述前端现行口径）。

## Decisions

### D1：`directoryId` 是唯一标识，展示名由路径派生

`WorkspaceRef` 只保留 `directoryId`；`WorkspaceCatalogEntry` 保留 `directoryId` 与 `path`。目录的展示名由 `path` 的最后一段派生；两个目录尾段相同时，用更多路径段（从末尾往前）消歧。目录标识只出现在路由参数与命令载荷里，MUST NOT 出现在任何界面文本中。

理由：这是归档变更与当前规范已经确定的模型；服务端不再下发 `displayName`，前端不能继续读一个不存在的键。展示名必须从同一条逐字路径派生，不能拼接或反查。

替代方案：保留 `alias`/`displayName` 作为前端内部别名，再映射到 `directoryId`。否决：会制造第二份身份与展示名来源，且 `session.create`/路由仍要传别名；规范已明确删除别名。

### D2：`path` 始终在场，删除 `core.workspace-path.v1`

快照 `workspaces` 条目的 `path` 是必填键；客户端不再按 feature 决定“读不读路径”。`SUPPORTED_FEATURES` 删除 `core.workspace-path.v1`，`feature-declarations.test.ts` 增加负向断言：机器登记表里没有它，客户端也不广告它。

理由：当前 schema 的 `workspaceCatalogEntry.required` 是 `["directoryId", "path"]`，Rust 端始终下发；继续广告一个已撤销的 feature 会让握手协商一个不存在的 ID。

### D3：路由与会话归属改用 `directoryId`

- 路由文件 `app/dir/[alias].tsx` 改为 `app/dir/[directoryId].tsx`，参数名与所有链接同步。
- `SessionSummaryView.directoryId` 取代 `directoryAlias`；目录分组按 `session.workspace?.directoryId`。
- 目录页/详情页/左栏的模型字段由 `alias` 改为 `directoryId`；展示名由 D1 的派生函数提供。
- 目录标识不参与任何显示文本；测试断言界面里没有它。

### D4：搜索只匹配展示名与会话标题

`matchesQuery` 去掉 `alias` 分支，只接受目录展示名（路径尾段 / 消歧后的展示名）与该目录下会话标题。大小写与 trim 规则不变。规范增量同时删掉“别名”一词。

理由：别名已不存在；继续搜索它是死代码，且会让“目录标识不参与搜索”的验收无法成立。

### D5：创建载荷与浏览回退

`session.create` 的目录引用改为 `{ directoryId }` 或 `{ workspacePath, createIfMissing? }`；`createIfMissing` 与 `directoryId` 互斥。已记录目录选择器与浏览回退使用 `directoryId` + `path`；创建弹层的目录块只显示路径（展示名由路径尾段派生）。`workspace.browse` 的形状不变（`name`/`path`/`registered`/`truncated`）。

### D6：e2e 夹具与断言

`WORKSPACES_WITH_PATHS` 改为 `{ directoryId, path }`；会话夹具的 `workspace` 改为 `{ directoryId }`；路由夹具改为 `/dir/<directoryId>`。期望的展示名由路径尾段派生（例如 `D:\secret\vault\repo-x` → `repo-x`），同名路径用更多段消歧。R15/R16 的断言按新展示名与新路由更新，但保留原有的信息集合与可达性断言。

### D7：Rust 格式只做机械修复

在隔离工作区运行 `cargo fmt`，只提交格式差异；不修改逻辑、不重排模块、不改测试语义。完成后 `cargo fmt --check` 必须通过。

## Risks / Trade-offs

- **界面标签变化**：展示名从服务端 `displayName` 变为路径尾段，既有测试与用户看到的目录名会变化。这是规范要求的行为；e2e 与单测同步更新，不保留旧标签兼容层。
- **一次性大迁移**：协议、领域、页面、路由与测试强耦合，拆成多个可独立编译的工作包会留下中间红树。因此前端迁移作为一个工作包合入；e2e 与文档/格式独立。
- **路径消歧算法**：尾段相同才追加更多段；必须按整份快照计算，不能随搜索/筛选变化。纯函数与单测覆盖。
- **`cargo fmt` 触碰多个 crate**：只格式化、不改语义；reviewer 需确认 diff 中无逻辑变化。

## Migration Plan

无数据迁移、无部署或 wire 变更。前端一次性切到当前合同；旧客户端不在本仓库分发范围内，因此不保留 `alias` 兼容分支。回滚只需回退前端与格式提交；Rust 行为未变。
