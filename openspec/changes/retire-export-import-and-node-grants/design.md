# Design

## Context

见 `proposal.md` 的 Why。相关实现事实：

- `owned_export`（含 `workspace_directories`、`templates`、`scopes` 的 JSON 表示）与 `imported_import`/`imported_import_export` 承载跨节点授权；`owned_node` 另有 `grants_json`、`export_ids_json` 两列。
- `identity-auth` 把 `grant.*` 展开成命令名集合；`core::broker::required_grant` 是它的 Rust 镜像；`compatibility/commands/v1/commands.json` 的 `grants` 是唯一机器来源。
- Node Link 的 catalog 按 Export 过滤；命令面按 grant 判定。
- 依赖前序变更：A 把目录引用改为 `directoryId`；B 把节点名改为配置 `[node] name`。本变更必须在二者归档后落地。

## Goals / Non-Goals

- **Goals**：删除整条跨节点授权链；把"配对即信任"写成显式威胁模型；让 imported 家族按 Owner 节点寻址。
- **Non-Goals**：不删除 Node Link、不改设备配对、不加新的授权层、不改 ACP wire。

## Decisions

### D1 配对即完全信任，并把它写进威胁模型

删除 grant 后，「已认证设备/节点即可执行本节点命令」这条已经适用于设备方向的规则，扩展到节点方向。必须在 `SECURITY_DESIGN.md` 的信任边界与威胁模型里逐条写清：

- 配对动作等价于授权该节点：读全部会话、发命令、**创建会话（启动本机 Agent 进程）**、批准权限请求；
- 仍然保留的边界：`local.*` 永不远程、连接/消息/速率上限、配对须本机用户确认、撤销即时生效、Access 不落正文、审计留痕。

替代方案（已被拒绝）：保留一个粗开关（如 `allowRemoteWork`）。用户明确选择"什么都不限制"；保留半套授权会让合同与实现再次分叉。

### D2 Export/Import 整族删除

- CLI：`export create|list|revoke`、`import add|list|remove` 删除。
- 本地方法：`local.export.manage` 及其下全部方法删除。
- 存储：`owned_export` 删除；`imported_import` 与 `imported_import_export` 删除；`imported_session`/`imported_delivery_index`/`imported_command_ref` 去掉 `export_id` 维度。

### D3 grant 词表删除

- `compatibility/commands/v1/commands.json` 删除 `grants` 块与命令条目上的 grant 归属。
- `core::broker::required_grant` 镜像删除；命令面不再计算 grant。
- `identity-auth` 的授权展开与 `Authority` 的 grant 集合删除；节点认证事实只携带主体标识。
- `owned_node.grants_json` 删除。

### D4 imported 家族按 Owner 节点寻址

`imported_session` 的主键由 `(owner_node_id, export_id, session_id)` 变为 `(owner_node_id, session_id)`；迁移时同一 Owner 下跨 Export 的重复行合并（保留最新 cursor 与元数据）。`imported_import` 删除后，"用哪台 Owner"由 `owned_node` 的 `kind = 'owner'` 记录表达（B 的 `[node] name` 提供展示名）。

迁移需要 12-step 表重建（主键变化）；单事务、失败整体回滚。

### D5 Node Link 的可见面

- catalog：返回该节点可见的全部资源，不再按 Export 过滤。
- 命令：去掉 grant 判定，保留连接级上限、幂等与终态契约。
- `session.create`：payload 只剩 `agentId` + `directoryId`（A 的作用域），`exportId`/`templateParams`/路径字段全部拒绝。
- 撤销：`node.revoke` 仍即时关闭该节点的连接。

### D6 节点名来自配置

`node pair` 的 `--display-name` 删除；对端展示名取自 B 的 `[node] name`（缺失时回落到 `nodeId` 前缀）。改名只影响后续握手展示，不改身份。

## Migration Plan

1. 依赖顺序：A（目录 v9）→ B（Agent/配置 v10）→ C（本变更，存储 v11）。C 的 delta 必须在 A/B 归档后按新主规格补写。
2. 存储 v11：删除 Export/Import 表、`owned_node` 两列；重建 `imported_*` 三张表（去 `export_id`）。合并重复行的规则写在迁移用例里。
3. 升级前备份；回滚只能靠备份（`user_version` 过新会被拒绝打开）。
4. 兼容性：本变更删除 wire 字段，无可部署历史客户端，不保留兼容分支。

## Risks / Trade-offs

- **[安全等级变化]** 已配对节点获得在本机执行程序的能力 → 威胁模型显式登记；配对确认文案需同步提示"配对后对方可使用本机全部 Agent"。
- **[imported 行合并]** 跨 Export 的重复会话合并可能丢失某个 Export 的 cursor → 合并规则取"最新 cursor"，并在迁移用例中断言不产生孤儿投递索引。
- **[合同资产大面积删除]** → 由四道检查（命令目录、feature 词表、schema/fixture、文档）一次性对齐，避免只删一半。
- **[回滚不可行]** → 升级前备份是唯一回滚手段，文档必须写明。

## Open Questions

（无。）
