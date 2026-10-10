<!-- 本文件定义变更动机、范围、能力与影响；行为细节放入 specs，技术方案放入 design.md，
 协作安排见 tasks.md。本变更 skip_specs，行为契约引用既有合同。 -->

## Why

`crates/storage-sqlite/tests/enum_coverage.rs` 用 core 枚举的 `ALL` + `as_str()` 逐值断言 §7.3/§7.4 的枚举列，但四个 DDL 约束列没有进入这张表：

- `owned_device.revoke_reason`、`owned_node.revoke_reason` 的 `IN (...)` 取值集完全未被断言；行为面也只测过 `RevokeReason::UserRequested` 与 `Compromised`，`KeyChanged` 从未经过 `revoke_token()` 落库。
- `owned_export.cache_policy`、`imported_import.cache_policy` 用的是等值 CHECK（`cache_policy = 'no-content-cache'`），现有解析器只认 `IN (...)`，因此完全未被断言。

风险在于：`revoke_token()` 的穷举 `match` 只能保证**新增**枚举变体触发编译检查，不能证明**已有**变体的 token 拼写正确；`cache_policy` 的单值 CHECK 与 `core::model::CachePolicy` 之间也没有任何自动关联。本次补上 DDL 断言与行为回归，使这两类漂移在测试期暴露。

## What Changes

- 扩展 `crates/storage-sqlite/tests/enum_coverage.rs`：为 `owned_device.revoke_reason` 与 `owned_node.revoke_reason` 增加 DDL 允许值断言。
- 为 `owned_export.cache_policy` 与 `imported_import.cache_policy` 的等值 CHECK 增加断言，并与 `core::model::CachePolicy` 当前取值比对（以范围最小的方式扩展现有取值解析，或提供等效断言）。
- 新增 `RevokeReason::KeyChanged` 撤销行为回归测试：通过存储端口对设备与节点执行撤销，确认事务成功并从库内读回 `key_changed`；该测试真正经过 `revoke_token()` 映射，而不是手写一份与 DDL 相同的字符串列表。
- 新增非法 `revoke_reason` 被数据库真实 CHECK 约束拒绝的测试。
- 不包含：修改 DDL、修改 core 枚举或端口签名、修改合同文档、重构既有 26 条断言与辅助函数（除支持等值约束所需的最小扩展）。

## Capabilities

本变更不改变任何规范层面的可观察行为：production 代码零改动，仅补齐既有行为契约的测试断言。因此在该变更的 `.openspec.yaml` 中设置 `skip_specs: true`，既有行为契约直接引用：

- `docs/CORE_PORTS_AND_STORAGE.md` §7.3（`owned_device`/`owned_node` 的 `revoke_reason` CHECK）与 §7.4（`imported_import.cache_policy` CHECK）、§7.2（`owned_export.cache_policy`）。
- `openspec/specs/storage-schema-v2-migration/spec.md`（DDL 文本与版本契约）与 `openspec/specs/admin-state-persistence/spec.md`（撤销与重启恢复语义）。
- `crates/storage-sqlite/tests/enum_coverage.rs` 的既有判据：§7.3/§7.4 的每个枚举列必须与 core 枚举逐值一致。

### New Capabilities

- 无（`skip_specs: true`）。

### Modified Capabilities

- 无（`skip_specs: true`）。

## Impact

- **Rust（预期唯一改动面）**：`crates/storage-sqlite/tests/enum_coverage.rs`、`crates/storage-sqlite/tests/admin_store.rs`；`crates/storage-sqlite/tests/support/mod.rs` 如需最小辅助（预计不需要）。
- **合同与文档**：无。DDL、端口签名、协议 wire 与安全语义均不变。
- **门禁**：`npm run check` 不受影响；`check:contract-drift` 比对的仍是不变的 §7 DDL 与 §5 端口。
- **生产代码**：预期零改动；若实现需要给 storage-sqlite 增加公开面以获取 token 期望值，须在 design.md 决策中说明并保持范围最小。
