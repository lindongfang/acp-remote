# Storage Schema V2 Migration 增量

## ADDED Requirements

### Requirement: v11 删除 Export/Import 与节点授权列

`owned_schema_version` MUST 从 v10 推进到 v11，并在同一事务内删除 `owned_export`、`imported_import`、`imported_import_export`，删除 `owned_node.grants_json`/`export_ids_json`，并重建 `imported_session`、`imported_delivery_index`、`imported_command_ref`（去掉 `export_id`）。迁移 MUST 按 `(owner_node_id, session_id)` 合并重复行并保留最新 cursor，`local_sequence` MUST NOT 回退；失败整体回滚。

#### Scenario: v10 升级到 v11

- **WHEN** 含 Export/Import 行与重复 imported 会话的 v10 库打开
- **THEN** 相关表/列被删除，重复行合并且 cursor 取最新，版本变为 v11

#### Scenario: 失败回滚

- **WHEN** v11 迁移中途失败
- **THEN** schema 与数据保持 v10
