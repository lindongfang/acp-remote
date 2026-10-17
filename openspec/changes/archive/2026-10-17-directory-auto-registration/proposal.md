# Proposal

## Why

**用户原始要求**：

> 1. acp-remote 需要保存在哪些目录新建过会话，只保存目录信息。目录不需要和设备绑定，其他设备打开时，也能看到打开过的目录。
> 2. 用派生 id。
> 3. 目录删了之后：历史可读。

**现状**：目录清单靠 `workspace select` 手工登记（`alias` + `displayName` + `rootPath` 三个必填参数）。但 `core.workspace-browse.v1` 落地后，用户已经可以在**任意目录**建会话（`session.create` 的 `workspacePath` 分支，实测通过）。于是出现两个互不相干的事实：

- 目录页、目录详情页与对话页左栏**都按目录聚合**，而目录的数据源只有「已登记」那一张表；
- 在未登记目录里建的会话**在界面上找不回来**。实测：建完会话后目录页仍是「本机没有任何目录」，整页里没有任何入口指向刚创建的会话；会话页左栏显示「这个会话没有目录归属」。

也就是说：登记不是为了「允许建会话」，而是为了「让建过的会话有一个能被找到的分组」。既然如此，目录清单应当由**会话本身派生**，而不是由用户维护。

## What Changes

- **删除** `workspace select` 子命令与 `local.workspace.select` 方法；目录不再需要人工登记。
- 目录在**首次被会话使用时自动登记**（与创建会话的提交同一事务），同一目录后续复用同一条记录。
- 目录标识改为**由规范化路径派生的 id**（Windows 上先做大小写归一），不再有 `alias` / `displayName`；展示名取路径尾段。
- Sync 快照的 `workspaces` 条目由 `{ alias, displayName, path? }` 改为 `{ directoryId, path }`；会话摘要的目录引用由 `{ alias, displayName }` 改为 `{ directoryId }`。
- feature `core.workspace-path.v1` **撤销**（路径就是目录条目的内容，不再单独门控）。
- `session.create` 的目录引用由 `workspaceAlias` 改为 `directoryId`；Node Link 面同样以「清单内 directoryId」代替别名，仍然禁止携带任意 Owner 路径。
- 目录在磁盘上被删除后：目录记录**保留**并标记「目录已不存在」，历史会话照常可读，新建被明确拒绝（不静默换目录、不自动删除会话）。
- 前端目录路由由 `/dir/<alias>` 改为 `/dir/<directoryId>`。
- **BREAKING**：`workspaces` 条目、会话摘要 `workspace` 字段、`session.create` 载荷三处 wire 形状变更；`core.workspace-path.v1` 从 feature 词表删除。本仓库无可部署的历史客户端，因此不保留兼容分支，由合同门禁与 fixture 一次性对齐。

## Capabilities

### New Capabilities

（无）

### Modified Capabilities

- `sync-workspace-catalog`：目录条目形状与来源（自动登记、派生 id）、目录引用形状、路径可见面与 feature 门控
- `workspace-resolution`：解析输入由别名改为目录 id、失败分类、创建时归属列的持久化与投影来源
- `sync-session-create`：载荷的目录引用、目录校验与失败分类
- `local-admin-methods`：删除 `workspace.select`
- `cli-commands`：删除 `workspace` 子命令
- `pwa-web-client`：目录页数据源与空态、目录项入口、路由与左栏归属
- `storage-schema-v2-migration`：目录表形状与版本推进、会话归属列
- `node-link-owner-server`：`session.create` 的目录引用改为清单内 `directoryId`

## Impact

- **协议资产**：`schemas/sync/v1/**`（`workspaceCatalogEntry` / `workspaceRef` / `session.create` 载荷）、`schemas/node-link/v1/**`、`compatibility/features/v1/features.json`、`compatibility/commands/v1/commands.json`、`fixtures/**/v1/**`。
- **代码**：`core`（目录值对象、创建会话用例、端口）、`storage-sqlite`（目录表与迁移、归属列）、`server::sync`（快照组装、命令校验）、`server::local_admin`（方法集）、`server::node_link`（远程建会话）、`app`（CLI 子命令）、`clients/app`（目录页/详情页/左栏/路由）。
- **文档**：`docs/SYNC_PROTOCOL.md`、`docs/NODE_LINK_PROTOCOL.md`、`docs/CORE_PORTS_AND_STORAGE.md`、`docs/FRONTEND_DESIGN.md`、`docs/LOCAL_ADMIN_PROTOCOL.md`、`docs/CONFIG_REFERENCE.md`、`README.md`。
- **用户可见**：目录不再需要 CLI 登记；同一目录只会出现一次；目录被删后历史仍可读。

## 非目标

- 不把会话正文或会话库搬进项目目录（会话仍由 Owner 的 SQLite 权威保存）。
- 不做目录监视：目录删除只在下次使用/读取时被发现。
- 不做「目录删了就删会话」的级联删除。
- 不做目录改名、别名或手工分组。
- 不改变 Agent 声明的来源（属变更 `agent-declaration-from-config`）。
- 不改变跨节点授权模型（属变更 `retire-export-import-and-node-grants`）。

## 硬约束

- 会话日志的唯一权威仍是 Owner 的 SQLite；目录只是会话上的一个字段。
- 路径只出现在两处对端可见输出：快照的 `workspaces` 条目与 `workspace.browse` 的逐层结果。
- 客户端提交的路径 MUST NOT 被回显；目录 id MUST NOT 由客户端构造。
- `npm run check` 与 `clients/app` 的 `npm run check` 必须全绿。

## 成功判据

1. 全新数据目录、不执行任何登记命令：在任意目录建会话后，该目录自动出现在目录页与对话页左栏；
2. 同一目录（含大小写差异、符号链接差异）多次使用只产生一条目录记录；
3. 删除磁盘目录后：历史会话仍可打开，目录项显示「目录已不存在」，在该目录新建会话被明确拒绝且不产生会话行；
4. 远程（Node Link）建会话只能引用清单内的 `directoryId`，携带任意路径或未登记 id 一律被拒；
5. 合同门禁全绿（含 schema/fixture 覆盖、feature 词表、命令目录一致性）。
