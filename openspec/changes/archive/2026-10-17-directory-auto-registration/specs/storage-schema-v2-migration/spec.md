# Storage Schema Migration 增量

## MODIFIED Requirements

### Requirement: 版本常量、migration 执行时序与连续升级

系统 SHALL 把文件格式版本与 owned 家族表结构版本推进到 9，imported 家族表结构版本保持 3。

migration MUST 在单实例锁之后、开始监听之前执行，单事务、失败整体回滚、可重复打开；v1/v2/v3/v4/v5/v6/v7/v8 库 MUST 经对应段连续升级到 v9，序号只回填一次、中间版本不单独落盘。

连续两次打开后 `user_version`、两族版本键与库内每条 DDL 文本 MUST 逐字节相同；升级库与新建库的 owned 家族列清单 MUST 逐项相等。

#### Scenario: 连续两次打开 schema 文本不变

- **WHEN** 用同一个数据目录连续打开并关闭存储两次
- **THEN** 文件格式版本与 owned 家族表结构版本都为 9（imported 家族为 3），且库内每条表与索引的 SQL 文本逐字节相同

#### Scenario: 升级中途失败整体回滚

- **WHEN** 一次 v8 到 v9（或更早版本连续升级）在事务中途失败
- **THEN** 库保持升级前的版本与内容不变，重开后可再次尝试升级，且不产生半张已改结构的表

#### Scenario: 升级库与新建库的 owned 列清单相等

- **WHEN** 分别打开升级库与新建库并读取 owned 家族的列清单
- **THEN** 两边的列名、顺序、类型与 `NOT NULL`/默认值逐项相等（`export_ids_json`、恢复两列与目录归属列按各自追加顺序位于末尾）

#### Scenario: v2 到 v3 升级保留审计并扩展词表

- **WHEN** 一个含既有审计行的 v2 库升级到 v3（并继续升级到 v9）
- **THEN** 全部审计行与 `audit_id`/序列原样保留，升级后 `actor_kind = 'pairing_claimant'` 与 `action = 'node.authenticated'`/`'node.auth_failed'` 可写入，旧词表之外的取值仍被拒绝，`owned_command` 的 `actor_kind` CHECK 保持三值不变

## REMOVED Requirements

### Requirement: v6 的 workspace_alias 列追加与列语义

**Reason**: 目录别名体系随人工登记一并删除；`owned_session.workspace_alias`（别名原文）在 v9 被重命名为 `workspace_directory_id` 并改存目录标识，本要求描述的列已不存在。

**Migration**: v8→v9 迁移在重命名前先按目录表回填：`workspace_alias` 非空的会话映射到对应目录的 `directory_id`；`workspace_alias` 为空但 `workspace_cwd` 非空的会话也登记该目录并回填。回填后执行列重命名；升级在单事务内完成，失败整体回滚。

### Requirement: 会话目录归属列的写入与解释

**Reason**: 列语义由「别名原文」改为「目录标识」，原场景（写入别名原文）不再成立。

**Migration**: 由新要求「会话目录归属列的写入与解释（目录标识）」承接；「恢复不改写该列」「NULL 不被路径反查补齐」两条不变量不变。

## ADDED Requirements

### Requirement: 会话目录归属列的写入与解释（目录标识）

存储层 SHALL 在 owned 会话创建流程中写入 `workspace_directory_id`：只在本次创建解析出目录标识时写入，且 MUST 写入该标识原文，MUST NOT 写入路径或由路径派生的其它文本。恢复流程 MUST NOT 读取或改写该列。该列为 `NULL` 时 MUST 被解释为「该会话没有目录归属」，MUST NOT 被任何读取路径按 `workspace_cwd` 反查目录表补齐，也 MUST NOT 用空串或占位值代替。

#### Scenario: 写入目录标识原文

- **WHEN** 一个会话创建请求携带已记录的目录标识并解析成功
- **THEN** 该会话行的 `workspace_directory_id` 与请求中的标识逐字节相同

#### Scenario: 恢复不改写目录归属

- **WHEN** 对一条已带 `workspace_directory_id` 的会话执行恢复
- **THEN** 恢复前后该列的字节相同，不出现用目录表或新创建结果覆盖已存取值

#### Scenario: NULL 不被路径反查补齐

- **WHEN** 一条会话行的 `workspace_directory_id` 为 `NULL`，而其 `workspace_cwd` 与某个已记录目录的规范化路径相同
- **THEN** 读取该会话得到的目录归属仍为「未分组」，不产生任何目录归属

### Requirement: v9 的目录表与目录归属列

v9 相对 v8 的差异是：新增 `owned_directory(directory_id PRIMARY KEY, canonical_path UNIQUE NOT NULL, created_at NOT NULL, last_used_at NOT NULL)`，并用 `ALTER TABLE ... RENAME COLUMN` 把 `owned_session.workspace_alias` 改名为 `workspace_directory_id`（不触发 12-step 表重建）。`owned_workspace` 表在 v9 删除。

迁移 MUST 从既有 `owned_workspace` 行与既有会话行的 `workspace_cwd` 生成目录行，使升级后不存在「会话引用了目录表里没有的 id」的行；目录标识由规范化路径派生，MUST 与运行期派生规则一致。

#### Scenario: v8 升级保留历史会话并归组

- **WHEN** 一个含已登记目录与既有会话行的 v8 库升级到 v9
- **THEN** 每条会话行的其余列逐字节不变，`workspace_directory_id` 等于由该会话原目录路径派生的标识

#### Scenario: 原先未分组的会话被归组

- **WHEN** 一条 v8 会话行的 `workspace_alias` 为 `NULL`，但 `workspace_cwd` 非空
- **THEN** 升级后该目录被记录，且该会话的 `workspace_directory_id` 非空

#### Scenario: 标识由规范化路径派生

- **WHEN** 两个既有目录的 `canonical_path` 只有大小写差异（或指向同一目录的链接写法）
- **THEN** 升级后它们合并为同一条目录行，且没有任何会话引用一个不存在的标识
