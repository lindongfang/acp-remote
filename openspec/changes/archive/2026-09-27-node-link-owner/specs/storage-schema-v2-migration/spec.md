# storage-schema-v2-migration Delta（node-link-owner）

## MODIFIED Requirements

### Requirement: 版本常量与 migration 幂等

系统 SHALL 把文件格式版本、owned 家族表结构版本与 imported 家族表结构版本一并推进到 3；v3 相对 v2 的唯一差异是 `owned_audit`/`imported_audit` 两张审计表的 CHECK 扩展（`actor_kind` 增 `'pairing_claimant'`、`action` 增 `'node.authenticated'`/`'node.auth_failed'`，因 SQLite 不能修改既有 CHECK 而按 12-step 表重建）。migration MUST 在单实例锁之后、开始监听之前执行，单事务、失败整体回滚、可重复打开；v1 库 MUST 经 v2 段连续升级到 v3，序号只回填一次、中间版本不单独落盘。连续两次打开后 `user_version`、两族版本键与库内每条 DDL 文本 MUST 逐字节相同。

#### Scenario: 连续两次打开 schema 文本不变

- **WHEN** 用同一个数据目录连续打开并关闭存储两次
- **THEN** 文件格式版本与两族表结构版本都为 3，且库内每条表与索引的 SQL 文本逐字节相同

#### Scenario: 升级中途失败整体回滚

- **WHEN** 一次 v2 到 v3（或 v1 连续升级）在事务中途失败
- **THEN** 库保持升级前的版本与内容不变，重开后可再次尝试升级，且不产生部分重建的审计表

#### Scenario: v2 到 v3 升级保留审计并扩展词表

- **WHEN** 一个含既有审计行的 v2 库升级到 v3
- **THEN** 全部审计行与 `audit_id`/序列原样保留，升级后 `actor_kind = 'pairing_claimant'` 与 `action = 'node.authenticated'`/`'node.auth_failed'` 可写入，旧词表之外的取值仍被拒绝，`owned_command` 的 `actor_kind` CHECK 保持三值不变
