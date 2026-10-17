# Design

## Context

见 `proposal.md` 的 Why。当前与本次改动直接相关的实现事实：

- `owned_workspace(alias PRIMARY KEY, display_name, canonical_path, ...)` 是**人工登记**的目录表；`owned_session.workspace_alias` 保存创建时的别名原文，`workspace_cwd` 保存规范化路径。
- Sync 快照的 `workspaces` 条目是 `{ alias, displayName, path? }`，`path` 受 feature `core.workspace-path.v1` 门控；会话摘要的 `workspace` 是 `{ alias, displayName }`。
- 目录页/详情页/左栏按目录聚合，数据源是这份登记表；`/dir/<alias>` 是前端路由。
- `session.create` 的目录引用是 `{ workspaceAlias | workspacePath, createIfMissing? }`；Node Link 面只接受别名。
- 存储 schema 当前为 owned v8 / imported v3（`crates/storage-sqlite/src/migrate.rs`），而 `storage-schema-v2-migration` 的主规格正文仍停在 v6——本变更顺手把版本叙述推进到本变更落地后的取值。

## Goals / Non-Goals

- **Goals**：目录清单由会话自动派生；目录以稳定 id 被引用；目录被删后历史仍可读；wire 形状一次性对齐并同步全部合同资产。
- **Non-Goals**：不迁移会话正文的存储位置；不做目录监视与自动清理；不做目录改名/手工分组；不改 Agent 声明与跨节点授权。

## Decisions

### D1 目录标识：规范化路径的稳定摘要

`directoryId = lower_hex(sha256(canonical_path)[..16])`（32 个 hex 字符）。

- 选它而不是直接用路径：路径要进 URL 与命令载荷，直接放会引入百分号编码、长度与"看起来像客户端可构造的输入"三类问题；摘要同时天然去重。
- 选它而不是自增 id：自增 id 需要额外映射表，且无法从路径推导；摘要可以随时重算校验。
- 大小写与符号链接：`canonicalize` 在 Windows 上返回磁盘真实大小写并解析 junction/symlink，因此摘要输入是**规范化之后的**字节；实现必须有用例覆盖「同一目录的不同大小写/链接写法得到同一个 id」。
- 目录移动 = 路径变 = id 变。接受这个后果（用户已确认）：历史会话保留在旧 id 下，UI 显示「目录已不存在」。

### D2 自动登记的时机：与成功的会话创建同一次提交

目录记录只在**会话创建成功**的那一次提交里 upsert（会话行 + 目录行 + 终态同一事务）。理由：

- 需求是「保存哪些目录**新建过**会话」；失败或 `uncertain` 不应把目录塞进清单。
- 与既有不变量一致：受理阶段只落幂等记录，不得有其它副作用。
- 目录行 upsert 是幂等的（`INSERT ... ON CONFLICT(directory_id) DO NOTHING`），并发创建同一目录不会冲突。

替代方案（已被拒绝）：在受理时登记——会把失败尝试的目录写进清单；在快照组装时按会话反推——需要每次全表扫描且无法表达「目录已删除」。

### D3 wire 形状

| 位置 | 旧 | 新 |
|---|---|---|
| 快照 `workspaces` 条目 | `{ alias, displayName, path? }` | `{ directoryId, path }` |
| 会话摘要 `workspace` | `{ alias, displayName }` 或 `null` | `{ directoryId }` 或 `null` |
| `session.create` payload | `workspaceAlias` / `workspacePath` | `directoryId` / `workspacePath` |
| Node Link `session.create` payload | `exportId` / `workspaceAlias` | `exportId` / `directoryId`（路径仍禁止） |

`directoryId` 在客户端是**不透明字符串**：客户端 MUST NOT 拼接、推导或反查。`path` 始终随条目下发（用户已确认其他已配对设备也能看到这些目录），因此 feature `core.workspace-path.v1` 撤销；`core.local-catalog.v1` 仍是整组目录/Agent 资源的门控。

### D4 目录被删除后的状态

目录行保留；解析该目录失败时返回服务端不可用类错误（`internal.unavailable`），不删除会话、不回退到任何替代目录。前端在目录项与详情页标「目录已不存在」并禁用新建入口。

替代方案（已被拒绝）：自动隐藏目录——历史会话会再次变成孤儿；级联删除会话——违反 Owner 会话日志的权威与保留口径。

### D5 路由

`/dir/<directoryId>`。`directoryId` 是不透明串，前端不做任何解析，只用于路由与命令回传。

### D6 Node Link 仍不接受路径

远程建会话的目录引用只能是清单内的 `directoryId`；出现任何路径字段仍按既有的 `nodelink.command.unsupported_field` 拒绝。这条保证「远程不携带任意 Owner 路径」的安全不变量不变。

### D7 存储迁移

- 新增 `owned_directory(directory_id PRIMARY KEY, canonical_path UNIQUE, created_at, last_used_at)`。
- 迁移：对每个既有 `owned_workspace` 行 ⇒ 插入目录行（由 `canonical_path` 算 id）；对每条 `owned_session.workspace_alias` 非空的行 ⇒ 按别名对应目录的 id 回填新列；对 `workspace_alias` 为空但 `workspace_cwd` 非空的行 ⇒ **也**登记该目录并回填（历史孤儿会话因此获得分组）。
- `owned_session.workspace_alias` 重命名为 `workspace_directory_id`（取值从别名原文改为 id）；SQLite 支持 `RENAME COLUMN`，不触发 12-step 重建。
- 升级版本推进到本变更落地后的 next 值（当前 v8 → v9）；升级单事务、失败回滚、可重复打开；无降级路径（与既有口径一致，升级前备份）。

### D8 前端数据源

目录页/详情页/左栏不再读 `alias`/`displayName`：展示名取 `path` 的最后一段；同名目录用更多路径段消歧（既有文案逻辑保留）。「本机没有任何目录」空态保留，但语义变为「还没有任何目录被会话使用过」。

## Migration Plan

1. 三个变更按 A → B → C **串行落地**，每个变更自身保持自洽：A 已把 Export 的目录集合改为目录标识（Export 仍存在），C 再整族删除 Export/Import。B（Agent 声明）与 A/C 无共享表，可插在任意位置。
2. 存储升级不可逆：升级前备份数据目录（`acp-remote.sqlite3` + WAL）。升级失败整体回滚，重开可重试。
3. 回滚策略：回退二进制到旧版本会因 `user_version` 过新被拒绝打开（既有口径）；需要恢复时用备份。

## Risks / Trade-offs

- **[wire 破坏性变更]** → 无已部署客户端；`schemas/`、`fixtures/`、`compatibility/` 与协议文档同批改，`npm run check` 一次性转绿。
- **[目录膨胀]** 每在临时目录建一次会话就会多一条目录记录 → 只记录成功创建；后续若成为问题，再加「最近使用时间 + 隐藏」而非删除。
- **[路径对全部已配对设备可见]** → 用户已明确接受（"其他设备打开时也能看到打开过的目录"）；威胁模型里登记这一点。
- **[历史会话归组变化]** 迁移会把原先「未分组」但带 `workspace_cwd` 的会话归入目录 → 属于期望行为，但要在验收里显式断言，避免被当成意外。
- **[feature 撤销]** 旧客户端（不存在）会因条目形状变化而拒收整条消息 → 不做兼容分支，靠 fixture 与门禁对齐。

## Open Questions

（无。D1–D8 均已与用户确认。）
