# Storage Schema V2 Migration 增量

## ADDED Requirements

### Requirement: v10 删除 Agent 与 Provider 管理表

`owned_schema_version` MUST 从 v9 推进到 v10，并在同一事务内删除 `owned_agent_profile` 与 `owned_provider_ref`；既有审计行 MUST 保留。迁移 MUST 在删除前记录 profile 行数（供启动日志 `daemon.agents_discarded`），失败时整体回滚且版本不前进。

#### Scenario: v9 升级到 v10

- **WHEN** 一个含 `owned_agent_profile` 行的 v9 库打开
- **THEN** 两张表被删除、审计行不变、版本变为 v10，且记录被删除的 profile 条数

#### Scenario: 升级失败回滚

- **WHEN** v10 迁移在中途失败
- **THEN** 两张表与版本保持 v9，不留下半升级状态

#### Scenario: 连续两次打开

- **WHEN** 已升级到 v10 的库再次打开
- **THEN** 不再执行迁移，schema 与行逐字节不变
