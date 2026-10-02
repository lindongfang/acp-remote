<!-- 已有能力（storage-schema-v2-migration）的增量：省略 ## Purpose，只写需求变化。 -->

## MODIFIED Requirements

### Requirement: 版本常量与 migration 幂等

系统 SHALL 把文件格式版本与 owned 家族表结构版本推进到 5，imported 家族表结构版本保持 3；v5 相对 v4 的唯一差异是 `owned_session` 末尾新增两列：`agent_session_id`（TEXT，可空，无默认值）与 `workspace_cwd`（TEXT，可空，无默认值）——两列只做追加，MUST NOT 触发 12-step 表重建（重建只用于改既有 CHECK）。这两列只在会话创建与恢复流程中写入：既有会话行 MUST 保持 `NULL`，且 `NULL` MUST 被解释为「该会话没有可用于恢复的标识或目录」，MUST NOT 被任何读取路径补全、推导或替换为别名解析结果。v4 相对 v3 的差异（`owned_node` 末尾新增 `export_ids_json`）与既有行为保持不变：既有节点行由默认值得到空清单，即**不得默认放权**。migration MUST 在单实例锁之后、开始监听之前执行，单事务、失败整体回滚、可重复打开；v1/v2/v3 库 MUST 经对应段连续升级到 v5，序号只回填一次、中间版本不单独落盘。连续两次打开后 `user_version`、两族版本键与库内每条 DDL 文本 MUST 逐字节相同；升级库与新建库的 owned 家族列清单 MUST 逐项相等。

#### Scenario: 连续两次打开 schema 文本不变

- **WHEN** 用同一个数据目录连续打开并关闭存储两次
- **THEN** 文件格式版本与 owned 家族表结构版本都为 5（imported 家族为 3），且库内每条表与索引的 SQL 文本逐字节相同

#### Scenario: 升级中途失败整体回滚

- **WHEN** 一次 v4 到 v5（或更早版本连续升级）在事务中途失败
- **THEN** 库保持升级前的版本与内容不变，重开后可再次尝试升级，且不产生半张已改结构的表

#### Scenario: v4 到 v5 升级保留既有会话行且新列为空

- **WHEN** 一个含 owned 会话行（v4 形状，没有恢复所需的两列）的库升级到 v5
- **THEN** 每行其余列逐字节不变，`agent_session_id` 与 `workspace_cwd` 为 `NULL`，且这些会话不被任何路径当作可恢复会话

#### Scenario: 新列写入后可读回且不推导

- **WHEN** 一个新建会话在其创建流程中写入 `agent_session_id` 与 `workspace_cwd`，随后重新打开存储读取该行
- **THEN** 两个取值按写入时的字节原样读回；另一条没有写入这两列的会话行读回仍为 `NULL`，不出现空串、别名或占位路径

#### Scenario: 升级库与新建库的 owned 列清单相等

- **WHEN** 分别打开升级库与新建库并读取 owned 家族的列清单
- **THEN** 两边的列名、顺序、类型与 `NOT NULL`/默认值逐项相等（`export_ids_json` 与本次两列按各自追加顺序位于末尾）

#### Scenario: v3 到 v4 升级给既有节点行写空清单

- **WHEN** 一个含已配对节点行（v3 形状，没有清单列）的库升级到 v4（并继续升级到 v5）
- **THEN** 每行的 `export_ids_json` 为 `'[]'`、其余列逐字节不变，节点仍可完成握手，但看不到任何 Export，直到本机重新确认配对

#### Scenario: v2 到 v3 升级保留审计并扩展词表

- **WHEN** 一个含既有审计行的 v2 库升级到 v3（并继续升级到 v5）
- **THEN** 全部审计行与 `audit_id`/序列原样保留，升级后 `actor_kind = 'pairing_claimant'` 与 `action = 'node.authenticated'`/`'node.auth_failed'` 可写入，旧词表之外的取值仍被拒绝，`owned_command` 的 `actor_kind` CHECK 保持三值不变

## ADDED Requirements

### Requirement: 恢复所需列的读写边界

存储层 SHALL 在 owned 会话的创建与恢复流程中读写 `agent_session_id` 与 `workspace_cwd`：`agent_session_id` 只在会话创建取得 ACP 会话标识之后写入，`workspace_cwd` 只在创建时解析出规范化目录时写入；两者一旦写入 MUST NOT 被恢复流程改写（恢复只读它们）。

#### Scenario: 恢复流程不覆写持久化取值

- **WHEN** 对一条已带有 `agent_session_id` 与 `workspace_cwd` 的会话执行恢复
- **THEN** 恢复成功或失败后，这两列的字节都与恢复前相同，不出现用别名、新解析路径或新会话标识覆盖已存取值
