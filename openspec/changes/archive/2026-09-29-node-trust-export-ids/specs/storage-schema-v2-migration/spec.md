## MODIFIED Requirements

### Requirement: 版本常量与 migration 幂等

系统 SHALL 把文件格式版本与 owned 家族表结构版本推进到 4，imported 家族表结构版本保持 3；v4 相对 v3 的唯一差异是 `owned_node` 末尾新增 `export_ids_json`（TEXT，`NOT NULL DEFAULT '[]'`，有类型 JSON 数组文本，空集合固定 `'[]'`）——该列只做追加，MUST NOT 触发 12-step 表重建（重建只用于改既有 CHECK）。既有节点行 MUST 由默认值得到空清单，即**不得默认放权**：升级后这些配对在 Owner 本机重新确认之前看不到任何 Export（连接与握手仍正常）。migration MUST 在单实例锁之后、开始监听之前执行，单事务、失败整体回滚、可重复打开；v1/v2 库 MUST 经对应段连续升级到 v4，序号只回填一次、中间版本不单独落盘。连续两次打开后 `user_version`、两族版本键与库内每条 DDL 文本 MUST 逐字节相同；升级库与新建库的 owned 家族列清单 MUST 逐项相等。

#### Scenario: 连续两次打开 schema 文本不变

- **WHEN** 用同一个数据目录连续打开并关闭存储两次
- **THEN** 文件格式版本与 owned 家族表结构版本都为 4（imported 家族为 3），且库内每条表与索引的 SQL 文本逐字节相同

#### Scenario: 升级中途失败整体回滚

- **WHEN** 一次 v3 到 v4（或更早版本连续升级）在事务中途失败
- **THEN** 库保持升级前的版本与内容不变，重开后可再次尝试升级，且不产生半张已改结构的表

#### Scenario: v3 到 v4 升级给既有节点行写空清单

- **WHEN** 一个含已配对节点行（v3 形状，没有清单列）的库升级到 v4
- **THEN** 每行的 `export_ids_json` 为 `'[]'`、其余列逐字节不变，节点仍可完成握手，但看不到任何 Export，直到本机重新确认配对

#### Scenario: 升级库与新建库的 owned 列清单相等

- **WHEN** 分别打开升级库与新建库并读取 owned 家族的列清单
- **THEN** 两边的列名、顺序、类型与 `NOT NULL`/默认值逐项相等（`export_ids_json` 在末尾）

#### Scenario: v2 到 v3 升级保留审计并扩展词表

- **WHEN** 一个含既有审计行的 v2 库升级到 v3（并继续升级到 v4）
- **THEN** 全部审计行与 `audit_id`/序列原样保留，升级后 `actor_kind = 'pairing_claimant'` 与 `action = 'node.authenticated'`/`'node.auth_failed'` 可写入，旧词表之外的取值仍被拒绝，`owned_command` 的 `actor_kind` CHECK 保持三值不变
