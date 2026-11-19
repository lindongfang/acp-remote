# Design

## Context

动机与成功判据见 [proposal.md](proposal.md)。当前主线已完成 `establish-node-access`、`deliver-codex-session-loop` 与 `deliver-session-capacity` 归档（`SCHEMA_VERSION = 14`），但在工程构建、模块职责与数据库表结构三处遗留了跨阶段技术债：
1. `app/Cargo.toml` 的 `[[bin]] session_capacity_acceptance_peer` 将 `path` 指向被 `.gitignore` 忽略的 `testing/runs/deliver-session-capacity/tester/2026-11-16-request-runtime/capacity_peer.rs`，同时 `app/src/node.rs`（`r3_source_wss_development` 模块）通过 4 处 `include!` 引用同目录下的 `capacity_{node,extended,additional,restart}_acceptance.rs`，破坏洁净克隆下的 `--all-features` 编译闭环。
2. 默认 `npm run check:rust` 未开启 6 个验收/开发夹具特性（`shared-view-acceptance-fixtures`、`registration-acceptance-fixtures`、`acp-runtime-acceptance-fixtures`、`native-dispatch-dev-fixtures`、`approval-owner-dev-fixtures`、`approval-owner-acceptance-fixtures`），导致全特性编译下存在 16+ 处可见性与未使用项告警及 60+ 处特性测试 Clippy 诊断。
3. `app/src/storage.rs` 暴露 `#[doc(hidden)] pub struct StorageData` 并实现 `Deref<Target = StorageData>`，内部混装了 SQLite 连接池与队列/提示词准备/文档传输/目录上传的运行时监督槽位。
4. `app/src/session_storage/schema.rs` 的 `SESSION` 常量除现役父表 `harnesses`、`workspaces` 外，仍包含第一代聊天正文与旧会话/审批表（共 15 张死表与 3 个唯一索引），配套保留了仅在 `#[cfg(test)]` 编译的 `legacy`、`history`、`approvals` 模块及其子目录测试。

## Goals / Non-Goals

**Goals:**
- 确保仓库在无任何本地 `testing/runs/` 残留的洁净 Git 检出状态下，`cargo clippy --manifest-path app/Cargo.toml --all-targets --all-features --locked -- -D warnings` 与 `npm run check` 100% 零告警通过，并由 `verify-suite.mjs` 与 CI 持久守护。
- 消除 `Storage` 的公开 `Deref` 泄漏，将应用运行时监督状态内聚封装，保持既有节点与注册表生命周期关闭顺序不变。
- 通过 `SCHEMA_VERSION = 15` 原子移除 15 张第一代 `SESSION` 废弃死表、3 个旧索引及对应的 `#[cfg(test)]` 遗留 Rust 代码，同时保持 `v0` 新库与 `v1..=v14` 历史升级路径平滑可用。

**Non-Goals:**
- 不修改任何 HTTP 配对接口、管理 WebSocket 协议帧格式、ACP 运行时协议行为或前端界面逻辑。
- 不移除现役 `product_sources`、`product_harness_sources`、`product_targets` 表及既有 `product_schema` 迁移测试链（避免破坏 `node_management_audit`、`node_approval_audit` 与 `node_catalog_schema` 外键）。
- 不在本变更中替换 `vendor/sqlx-sqlite` 驱动补丁。

## Decisions

### 1. 验收对端与被忽略 `include!` 源码入库及 `verify-suite.mjs` 忽略路径守护

- 将 `capacity_peer.rs` 迁入受 Git 跟踪的 `app/dev-fixtures/session_capacity_acceptance_peer.rs`（与既有 `app/dev-fixtures/native_dispatch_dev_child.rs`、`app/dev-fixtures/approval_owner_dev_child.rs` 同级），同步将 `app/Cargo.toml` 中 `session_capacity_acceptance_peer` 的 `path` 更新为 `"dev-fixtures/session_capacity_acceptance_peer.rs"`。
- 将 `app/src/node.rs` 中引自 `testing/runs/deliver-session-capacity/tester/2026-11-16-request-runtime/` 的 4 份验收源码（`capacity_node_acceptance.rs`、`capacity_extended_acceptance.rs`、`capacity_additional_acceptance.rs`、`capacity_restart_acceptance.rs`）迁入受版本控制的 `app/src/node/session_services/`（与同级已入库的 `runtime_child_*_acceptance.rs`、`capacity_l6_*.rs` 一致），并将 `app/src/node.rs` 对应 4 处 `include!` 更新为 `include!("node/session_services/capacity_*_acceptance.rs");`。
- 在 `testing/suites/verify-suite.mjs` 中新增 Cargo 与 Rust 编译期路径卫生检查：扫描 `app/Cargo.toml` 中所有 `path = "..."` 条目以及 `app/**/*.rs` 中编译期源码引入（`include!` 与 `#[path = "..."]`），解析为仓库相对路径后对照 `IGNORED_LINK_PREFIX_RE`（涵盖 `testing/runs`、`.tmp`、`target`、`node_modules`、`dist` 等）并校验目标路径在磁盘存在且不在忽略前缀内，从工具链源头杜绝跨入忽略目录。
- 修复 `--all-features --all-targets` 下全部编译与 Clippy 告警，覆盖 `app/src/application/native_dispatch/`（含 `approval_records.rs`、`approval_acceptance_mount.rs` 等）、`app/src/session_storage/approval_owner.rs` 与 `app/src/session_storage/approval_owner/`（含 `test_control.rs`、`test_migration.rs` 等）、`app/src/session_storage/registry_connection_owner.rs`、`app/src/session_storage/schema.rs`、`app/src/node/approval_owner/development_tests.rs`、`app/src/node.rs` 以及 `app/src/node/session_services/`（含 `capacity_l6_*.rs` 与新入库的 `capacity_*_acceptance.rs`）中的可见性不匹配（`private_interfaces`）、未使用测试钩子与 Clippy pedantic/nursery 诊断；在 `.github/workflows/ci.yml` 的 `product` 任务中增加 `--all-features` Clippy 检查步骤。

### 2. `Storage` 取消 `Deref` 泄漏与运行时监督协调内聚化

- 删除 `#[doc(hidden)] pub struct StorageData` 及 `impl std::ops::Deref for Storage`，将 `Storage` 内部状态改为模块私有结构体 `StorageInner`。
- 将原散落在 `StorageData` 顶层的运行时协调字段（`queue_runtime_claim`、`queue_supervisor`、`queue_close`、`queue_lifecycle`、`queue_closed` 以及 Windows 下的 `preparation_shutdown`、`document_shutdown`、`catalog_shutdown`）收拢进专用的内部协调器 `StorageRuntimeCoordinator`（位于 `app/src/storage/runtime_coordinator.rs`），明确区分：
  - **持久化层职责**：`pool`、`request_registry`、`lifecycle`、`load_node_meta`、`write_node_meta`、`trust_store`、连接封存与物理关闭观察；
  - **运行时监督桥接职责**：确保在同一个 `Storage` 作用域内单次领取队列运行时所有权，并在 `close_in_scope` 封存数据库连接前先完成 `RuntimeSupervisor::cancel()` 与 `stop_and_join()`（完全保留 `registration_*` 验收套件对关闭顺序与物理连接收敛的不变量）。
- **分步编译兼容约束**：在 `WP2` 独立实施与自测阶段，`#[cfg(test)]` 下的 `Storage::approvals()`、`Storage::histories()`、`Storage::sessions()` 暂改为内部读取 `self.inner.pool` 以保证 `--lib` 单测目标自包含编译通过；进入 `WP3` 删除第一代遗留模块时，再将这三个废弃访问器一并原子删除，避免跨工作包中间态断编。

### 3. SQLite `SCHEMA_VERSION = 15` 死表收口与遗留模块移除

- **表边界精确划分**：
  - 从原 `SESSION` 切片中保留 `CATALOG_TABLES`（2 张现役父表）：`harnesses`、`workspaces`。
  - 待退役的第一代死表集合 `LEGACY_SESSION_TABLES`（共 15 张表，按子表到父表拓扑排序）：
    1. `history_content_pins`
    2. `history_integrity`
    3. `session_state`
    4. `approval_audit_links`
    5. `approval_deliveries`
    6. `approval_options`
    7. `approvals`
    8. `approval_run_sources`
    9. `approval_connections`
    10. `conversation_items`
    11. `runs`
    12. `turns`
    13. `session_commands`
    14. `content_objects`
    15. `sessions`
  - 待退役的旧唯一索引 `LEGACY_SESSION_INDEXES`（3 个）：`tool_reference_key`、`tool_native_identity`、`one_foreground_run`。
- **新库与迁移双路径行为（`SCHEMA_VERSION = 15`）**：
  - **全新数据库（`current == 0`）**：在 `BEGIN IMMEDIATE` 初始化事务中创建 `CORE`、`CATALOG_TABLES`（`harnesses`、`workspaces`）及 `v3..=v14` 现役表，直接将 `PRAGMA main.user_version = 15`（若 `node_meta` 已存在则同步更新 `schema_version = 15`），**全程不再创建 `LEGACY_SESSION_TABLES` 与 `LEGACY_SESSION_INDEXES`**。
  - **既有数据库升级（`current in 1..=14`）**：
    - `validate(connection, current)` 对 `current in 2..=14` 仍按对应历史版本校验原结构（兼容 `schema3_migration.rs`、`schema4_migration.rs`、`schema5_registration_acceptance.rs` 等从历史快照起跳的升级测试）；若 `current == 1`，仅补建 `CATALOG_TABLES` 而不再补建死表；
    - 当 `current < 15` 进入 v15 迁移步骤时：若 `current >= 2`，先执行 `UPDATE main.approvals SET selected_choice_id = NULL` 与 `DELETE FROM main.approval_options`（解除 `approvals <-> approval_options` 双向外键环在启用 `PRAGMA foreign_keys = ON` 下的删除阻断），再显式 `DROP INDEX IF EXISTS` 移除 3 个旧索引，并按 `LEGACY_SESSION_TABLES` 拓扑顺序执行 `DROP TABLE IF EXISTS main.<name>`（兼容 `current == 1` 未创建死表路径与 `current >= 2` 已存在死表路径）；随后执行 `PRAGMA main.foreign_key_check` 确认零悬空外键，更新 `UPDATE main.node_meta SET schema_version = 15` 与 `PRAGMA main.user_version = 15`。
  - **`v15` 严格校验**：当 `current == 15` 时，`validate` 校验 `CATALOG_TABLES` 定义完整存在，且 `main.sqlite_schema` 与 `temp.sqlite_schema` 中不存在任何 `LEGACY_SESSION_TABLES` 或 `LEGACY_SESSION_INDEXES` 同名表/视图/索引/触发器；`validate_required_triggers` 在 `current >= 15` 时仅检查现役表。
- **同步移除废弃 Rust 模块、重挂载单测与对齐现役子模块/迁移回归断言**：
  - 删除 `app/src/domain/history.rs`、`app/src/domain/approvals.rs`；精简 `app/src/domain/mod.rs` 中仅服务于第一代 `InputRepository` 的废弃类型（保留平台通用的 `EntityId`、`ActorContext` 与 `SessionError::CorruptState`）。
  - 删除 `app/src/application/history.rs`、`app/src/application/history/`（含 `README.md` 与 `tests.rs`）、`app/src/application/approvals.rs`、`app/src/application/approvals-development.md` 及 `app/src/application/mod.rs` 中 `#[cfg(test)]` 的 `InputRepository` / `InputService`。
  - 删除 `app/src/session_storage/legacy.rs`、`app/src/session_storage/history.rs`、`app/src/session_storage/history/`、`app/src/session_storage/approvals.rs`、`app/src/session_storage/approvals/` 以及 `app/src/storage.rs` 中的 `Storage::{approvals, histories, sessions}`；**同时在 `app/src/session_storage/mod.rs` 显式补挂 `#[cfg(test)] mod tests;`**（替代原先隐藏在 `legacy.rs` 末尾的 `#[path = "tests.rs"] mod tests;` 唯一挂载点，防止 `session_storage/tests.rs` 静默脱离编译），并将 `app/src/session_storage/tests.rs` 更新为覆盖 `v0 -> v15` 新库初始化、`v1..=v14 -> v15` 含双向外键行升级清理、迁移故障回滚及 `v15` 残留死表拦截的单测。
  - 同步对齐 `app/src/session_storage/` 内部现役子模块单测对 `v15` 终态的断言（覆盖 `approval_owner_schema/tests.rs`、`product_schema/tests.rs` 及关联 `product_schema/delivery_tests.rs`、`product_commands/tests.rs`、`request_registry/tests.rs`、`native_dispatch_schema/tests.rs`、`session_capacity/development_tests.rs`、`session_capacity/automatic_release_development_tests.rs`、`automatic_capacity_release_schema/development_tests.rs`、`session_tree_schema/development_tests.rs`）：将历史夹具构造所需的旧 `SESSION` DDL 与升级至 `v15` 后的现役保留集（`CATALOG_TABLES` + `CORE` + `product_*` + `node_*`，`head == 15` 且 15 张退役表与 3 个旧索引缺席）明确解耦，不再在新库或 `v15` 升级终态查询已退役的 `content_objects`/`turns`/`conversation_items`/`session_commands`/`approvals` 表。
  - 同步对齐 `app/tests/registration_acceptance/c/{schema3_migration.rs, schema4_migration.rs, management_independent.rs}` 与 `app/tests/registration_acceptance/m/schema5_registration_acceptance.rs` 中升级终态快照断言：验证升级至 `v15` 后 `CORE`、`harnesses`、`workspaces` 与 `product_*` 现役表定义及数据 100% 零丢失，同时 15 张退役 `SESSION` 表与 3 个旧索引在升级成功后确定性缺席（回滚路径仍断言升级前含 `LEGACY_SESSION` 的原库完整未变），并清理 `app/tests/acceptance/` 下未被引用的重复旧迁移孤儿文件（`schema3_migration.rs`、`schema4_migration.rs`、`management_independent.rs`）。

## Risks / Trade-offs

- [`approvals` 与 `approval_options` 存在双向外键引用，启用 `PRAGMA foreign_keys = ON` 时直接 `DROP TABLE` 若表内有开发期残留行会触发外键约束失败] → 在 `v15` 迁移事务删除两表前，先执行 `UPDATE main.approvals SET selected_choice_id = NULL` 与 `DELETE FROM main.approval_options`，并在单测中构造含真实关联行的 `v14` 库验证升级通过。
- [`session_storage/tests.rs` 原先仅经由 `legacy.rs` 间接挂载，删除 `legacy.rs` 易导致单测静默成为孤儿文件] → 在 `app/src/session_storage/mod.rs` 显式补挂 `#[cfg(test)] mod tests;`，并在 `PV4` 中通过实跑用例名过滤确认新 `v15` 单测实际执行数量大于 0。
- [`schema3/4/5` 迁移验收及 `session_storage/` 下 9 份子模块单测原先假定升级后 `v2` 起步的 `LEGACY_SESSION` 表仍原样保留或 `head == 14`] → 保留 `validate(connection, current)` 对 `2..=14` 输入库的前置校验与故障回滚全量快照断言，同时将成功升级至 `v15` 后的终态断言统一对齐为“现役核心/目录/产品/节点表零丢失 + 15 张退役死表与 3 个旧索引确定性缺席 + `head == 15`”。
- [移除 `Storage` 的 `Deref` 可能影响 `app/src/storage/` 子模块及 `WP2 -> WP3` 中间编译] → `WP2` 先通过模块内受控方法与 `StorageRuntimeCoordinator` 消除 `StorageData`/`Deref`，保留 `#[cfg(test)]` 旧访问器内部转发以确保 `WP2` 独立编译通过，再在 `WP3` 与第一代遗留模块原子同删。

## Migration Plan

1. 节点启动执行 `Storage::open_owned` / `Storage::open` 时，只读 `preflight_in_scope` 先按当前 `user_version`（`0..=15`）完成完整性校验；高于 `15` 的未知版本继续按 `StorageError::UnknownSchema` 安全拒绝且不改写数据。
2. 对 `user_version` 为 `1..=14` 的既有节点数据库，在单连接 `BEGIN IMMEDIATE` 事务中自动执行 `v15` 死表清理迁移并原子推进 `node_meta.schema_version` 与 `PRAGMA user_version` 至 `15`；中途任何故障均完整回滚事务，不产生半迁移状态。
3. 升级至 `v15` 后不支持降级回旧二进制（旧二进制识别 `user_version = 15 > 14` 会安全报 `UnknownSchema(15)` 拒绝启动而不破坏库文件）。
