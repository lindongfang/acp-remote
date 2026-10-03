<!-- 已有能力（storage-schema-v2-migration）的增量：省略 ## Purpose，只写需求变化。 -->

## MODIFIED Requirements

### Requirement: 版本常量与 migration 幂等

系统 SHALL 把文件格式版本与 owned 家族表结构版本推进到 6，imported 家族表结构版本保持 3；v6 相对 v5 的唯一差异是 `owned_session` 末尾新增一列 `workspace_alias`（TEXT，可空，无默认值）——该列只做追加，MUST NOT 触发 12-step 表重建（重建只用于改既有 CHECK）。该列只在会话创建流程中写入：既有会话行 MUST 保持 `NULL`，且 `NULL` MUST 被解释为「该会话没有目录归属」，MUST NOT 被任何读取路径补全、推导或按规范化路径反查别名。v5 相对 v4 的差异（`owned_session` 末尾新增 `agent_session_id` 与 `workspace_cwd` 两列）与既有行为保持不变：这两列只在会话创建与恢复流程中写入，既有会话行 MUST 保持 `NULL`，`NULL` MUST 被解释为「该会话没有可用于恢复的标识或目录」，MUST NOT 被推导或替换为别名解析结果。v4 相对 v3 的差异（`owned_node` 末尾新增 `export_ids_json`）与既有行为保持不变：既有节点行由默认值得到空清单，即**不得默认放权**。migration MUST 在单实例锁之后、开始监听之前执行，单事务、失败整体回滚、可重复打开；v1/v2/v3/v4/v5 库 MUST 经对应段连续升级到 v6，序号只回填一次、中间版本不单独落盘。连续两次打开后 `user_version`、两族版本键与库内每条 DDL 文本 MUST 逐字节相同；升级库与新建库的 owned 家族列清单 MUST 逐项相等。

#### Scenario: 连续两次打开 schema 文本不变

- **WHEN** 用同一个数据目录连续打开并关闭存储两次
- **THEN** 文件格式版本与 owned 家族表结构版本都为 6（imported 家族为 3），且库内每条表与索引的 SQL 文本逐字节相同

#### Scenario: 升级中途失败整体回滚

- **WHEN** 一次 v5 到 v6（或更早版本连续升级）在事务中途失败
- **THEN** 库保持升级前的版本与内容不变，重开后可再次尝试升级，且不产生半张已改结构的表

#### Scenario: v5 到 v6 升级保留既有会话行且新列为空

- **WHEN** 一个含 owned 会话行（v5 形状，没有目录归属列）的库升级到 v6
- **THEN** 每行其余列逐字节不变，`workspace_alias` 为 `NULL`，且这些会话的目录归属被解释为「未分组」而不是被反查补齐

#### Scenario: 新列写入后可读回且不推导

- **WHEN** 一个新建会话在其创建流程中写入 `workspace_alias`，随后重新打开存储读取该行
- **THEN** 该取值按写入时的字节原样读回；另一条没有写入该列的会话行读回仍为 `NULL`，不出现空串、别名猜测或占位路径

#### Scenario: v4 到 v5 升级保留既有会话行且新列为空

- **WHEN** 一个含 owned 会话行（v4 形状，没有恢复所需的两列）的库升级到 v5（并继续升级到 v6）
- **THEN** 每行其余列逐字节不变，`agent_session_id` 与 `workspace_cwd` 为 `NULL`，且这些会话不被任何路径当作可恢复会话

#### Scenario: 升级库与新建库的 owned 列清单相等

- **WHEN** 分别打开升级库与新建库并读取 owned 家族的列清单
- **THEN** 两边的列名、顺序、类型与 `NOT NULL`/默认值逐项相等（`export_ids_json`、恢复两列与 `workspace_alias` 按各自追加顺序位于末尾）

#### Scenario: v3 到 v4 升级给既有节点行写空清单

- **WHEN** 一个含已配对节点行（v3 形状，没有清单列）的库升级到 v4（并继续升级到 v6）
- **THEN** 每行的 `export_ids_json` 为 `'[]'`、其余列逐字节不变，节点仍可完成握手，但看不到任何 Export，直到本机重新确认配对

#### Scenario: v2 到 v3 升级保留审计并扩展词表

- **WHEN** 一个含既有审计行的 v2 库升级到 v3（并继续升级到 v6）
- **THEN** 全部审计行与 `audit_id`/序列原样保留，升级后 `actor_kind = 'pairing_claimant'` 与 `action = 'node.authenticated'`/`'node.auth_failed'` 可写入，旧词表之外的取值仍被拒绝，`owned_command` 的 `actor_kind` CHECK 保持三值不变

## ADDED Requirements

### Requirement: 会话目录归属列的写入与解释

存储层 SHALL 在 owned 会话创建流程中写入 `workspace_alias`：只在解析出别名时写入，且 MUST 写入别名原文，MUST NOT 写入任何由规范化路径派生的取值（含路径末段）。恢复流程 MUST NOT 读取或改写该列。该列为 `NULL` 时 MUST 被解释为「该会话没有目录归属」，MUST NOT 被任何读取路径按 `workspace_cwd` 反查 `owned_workspace` 补齐，也 MUST NOT 用空串或占位别名代替。

#### Scenario: 写入别名原文

- **WHEN** 一个会话创建请求携带已登记且解析成功的 workspace 别名
- **THEN** 该会话行的 `workspace_alias` 与请求中的别名原文逐字节相同

#### Scenario: 恢复不改写目录归属

- **WHEN** 对一条已带 `workspace_alias` 的会话执行恢复
- **THEN** 恢复前后该列的字节相同，不出现用别名解析结果或新登记值覆盖已存取值

#### Scenario: NULL 不被路径反查补齐

- **WHEN** 一条会话行的 `workspace_alias` 为 `NULL`，而其 `workspace_cwd` 与某个已登记 workspace 的规范化路径相同
- **THEN** 读取该会话得到的目录归属仍为「未分组」，不产生任何别名
