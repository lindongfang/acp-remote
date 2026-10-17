# Proposal

## Why

**用户原始要求**：

> `node pair --grant` 去掉；Export/Import 相关指令全部去掉；agent、workspace、scope 都不限制。

**现状**：跨节点能力由三层限制叠加而成——Owner 的 Export（哪些 Agent、哪些目录、哪些 grant）、Access 的 Import（要哪些 Export、愿意给哪些 grant）、以及命令级的 grant 判定（`grant.observe`/`interact`/`configure-session`/`approve`/`remote-work`）。设备方向早已没有授权维度，但节点方向仍保留这一整套。

**为什么现在做**：前两个变更把「目录」与「Agent」的限制都拿掉了（任意目录、配置声明 Agent）。Export 剩下的字段只有 `export_id`/`display_name`（用户明确要去掉）、`agent_ids`（不再限制）、`workspaceDirectories`（不再限制）、`scopes`（不再限制）——**没有任何信息可装**。Import 剩下的 `owner_endpoint + owner_node_id` 就是 `node pair` 记录本身。继续保留只会产生两套同义记录、一条永远为空的授权链，以及一批需要维护的合同资产。

## What Changes

**BREAKING**：以下变更删除 wire 字段与整族命令。

- **删除** `export create|list|revoke`、`import add|list|remove` 六个子命令与 `local.export.manage` 方法族。
- **删除** `grant.*` 词表：`compatibility/commands/v1/commands.json` 的 `grants`、`broker::required_grant` 镜像、`identity-auth` 的授权展开、`owned_node.grants_json`。
- `node pair` 删除 `--grant`、`--export-id`、`--display-name`（节点名改由变更 B 的配置 `[node] name` 提供）。
- 节点信任记录删除 `grants_json` 与 `export_ids_json`；`imported_*` 家族删除 `export_id` 维度，改为按 Owner 节点寻址。
- Node Link：catalog 不再按 Export 过滤（返回该节点可见的全部资源）；命令不再做 grant 判定（**已认证即允许**，`local.*` 仍永不远程授予）。
- **威胁模型变更（必须写进 `docs/SECURITY_DESIGN.md`）**：配对 = 完全信任。已配对节点可以读全部会话、发命令、**在本机创建会话（启动配置里的 Agent 进程）**、批准工具权限请求。保留的非授权边界：`local.*` 永不远程、连接/消息/速率上限、配对须本机确认、撤销即时生效、Access 不落远程正文、审计留痕。

## Capabilities

### New Capabilities

（无）

### Modified Capabilities

- `scope-expansion`：删除导出授权展开，只保留「本地管理能力永不远程授予」
- `identity-pairing`：节点配对不再携带授权集合
- `admin-state-persistence`：删除 Export/Import 写集
- `node-link-owner-server`：catalog 不按 Export 过滤、命令不做 grant 判定（**A/B 归档后补写**）
- `local-admin-methods`：删除 Export/Import 方法族（**A/B 归档后补写**）
- `cli-commands`：删除命令组与参数（**A/B 归档后补写**）
- `storage-schema-v2-migration`：删表、删列、imported 家族去 export 维度（**A/B 归档后补写**）
- `sync-snapshot-scope` / `pwa-web-client`：imported 资源不再有 Export 维度（**归档后按需补写**）

## Impact

- **协议资产**：`schemas/sync/v1/**`、`schemas/node-link/v1/**`、`compatibility/commands/v1/commands.json`（`grants` 与命令集）、`compatibility/features/v1/features.json`（如 `resource.remote-origin.v1` 的字段变化）、`fixtures/**/v1/**`。
- **代码**：`core`（去掉 grant 参数与 Export/Import 用例）、`identity-auth`（授权展开）、`storage-sqlite`（删表/删列/imported 迁移）、`server::local_admin`（方法集）、`server::node_link`（catalog 与命令判定）、`app`（CLI）。
- **文档**：`docs/SECURITY_DESIGN.md`（威胁模型与授权章节）、`docs/NODE_LINK_PROTOCOL.md`、`docs/LOCAL_ADMIN_PROTOCOL.md`、`docs/SYNC_PROTOCOL.md`（imported origin 字段）、`docs/CORE_PORTS_AND_STORAGE.md`、`docs/CONFIG_REFERENCE.md` §9、`README.md`。
- **用户可见**：配对后即可用对方的全部 Agent；没有 Export/Import 概念。

## 非目标

- 不删除 Node Link 本身（跨节点仍可用，只是不再有限制）。
- 不改变设备配对与设备信任。
- 不移除 `local.*` 的本地能力边界。
- 不引入新的授权层（例如按 Agent 或按目录的开关）。

## 硬约束

- `local.*` 能力仍 MUST NOT 远程授予；本地管理面不因远程请求而改变。
- 连接级上限、配对确认仪式、撤销即时生效、Access 不落远程正文保持不变。
- 已配对节点的能力变化必须写进威胁模型，不得只改代码。
- `npm run check` 全绿。

## 成功判据

1. `compatibility/`、两个协议 schema、SYNC/SECURITY 表格与 `broker` 镜像中不再出现 `grant.`/`export`/`import`；
2. `node pair` 无授权参数即可完成配对；配对后 Access 能读会话并建会话；
3. 旧库升级后 Export/Import 表与两列被删除，设备/节点信任与既有会话不受影响；
4. 威胁模型写实「配对即完全信任」，`npm run check` 全绿。
