# 收敛洁净构建边界、Storage 运行时职责与第一代废弃会话表基线

## Why

当前仓库存在 `app/Cargo.toml` 跨界引用被 `.gitignore` 忽略的 `testing/runs/` 测试源码、CI 未覆盖 `--all-features` 导致特性门控目标累积编译/Clippy 告警、`Storage` 通过隐藏 `Deref` 反向持有应用层运行时监督槽位，以及生产 SQLite 仍强制创建并校验已废弃的第一代聊天正文死表（`SESSION`）与数千行 `#[cfg(test)]` 遗留代码。在启动下一阶段会话控制权接管与混合展示快照库开发前，需先完成工程与存储基线收敛。

## Success Criteria

### 用户原始要求

> 你是一名系统架构师，从架构层面分析一下，当前项目的架构设计是否合理，是有有需要调整的
> 需要新建变更，然后实现吗？
> 按照你的建议来推进。
> A1: 一并清理第一代 SQLite 聊天正文死表（SESSION 表及 legacy/history/approvals 测试遗留代码，通过 schema15 迁移或新库基线收口）
> A2: 先将当前工作区已完成的远期规划修订（CT-001/TC-001 相关文档与 x-deferred 变更）单独提交为一个规划 commit，再新建本次活跃变更

### 硬约束与非目标

- 本变更属于不改变外部可观察产品规格与网络协议契约的内部工程与存储基线收敛，在 `.openspec.yaml` 声明 `skip_specs: true`，严格遵守 [总体设计](../../../../docs/architecture/overview.md)、[AGENTS.md](../../../../AGENTS.md) 与 `openspec/specs/` 既有长期规格。
- 仅移除第一代废弃聊天正文/会话/旧审批死表及其专属 `#[cfg(test)]` 遗留模块（`legacy`、`history`、`approvals`），**严禁误删仍在生产目录与外键链路中使用的 `harnesses`、`workspaces` 及 `product_sources`、`product_harness_sources`、`product_targets`**。
- 不在本变更中重写 `vendor/sqlx-sqlite` 补丁驱动、不拆分 `clients/web/src/main.ts`（留归后续独立前端预重构变更），也不触碰任何 `x-deferred: true` 远期变更的业务实现。
- 不删除用户真实运行数据目录、私钥或本机私有配置文件。

### 本 change 的成功判据

- SC1：消除 `app/Cargo.toml` 对 `.gitignore` 忽略目录 `testing/runs/` 的源码依赖，将验收对端迁入受版本控制的 `app/dev-fixtures/`，并在 `testing/suites/verify-suite.mjs` 增加静态防复发校验，确保全新洁净克隆下所有构建目标源码完整在库。
- SC2：修复 Rust 全特性（`--all-features --all-targets`）下的全部编译可见性告警、死代码告警与 Clippy 阻断项，并将 `--all-features` 静态检查纳入本地检查脚本或 `.github/workflows/ci.yml` 持续集成门禁。
- SC3：移除 `Storage` 对外暴露内部字段的 `#[doc(hidden)] pub struct StorageData` 与 `Deref` 实现，将队列与准备阶段的运行时监督/关闭槽位封装为独立协调组件，保持既有节点启动、请求登记与物理关闭收敛语义零回归。
- SC4：通过 SQLite `SCHEMA_VERSION = 15` 迁移与新库初始化收口，保留 `harnesses` 与 `workspaces` 两张基础目录父表，在新库中不再创建、在既有 `v2..=v14` 库升级时安全清理其余第一代 `SESSION` 聊天正文/会话/旧审批死表及索引，并同步移除 `domain`/`application`/`session_storage` 下对应的 `legacy`、`history`、`approvals` 废弃测试模块。

## What Changes

- 将 `session_capacity_acceptance_peer` 的源码从被 Git 忽略的 `testing/runs/deliver-session-capacity/tester/2026-11-16-request-runtime/capacity_peer.rs` 迁入受跟踪的 `app/dev-fixtures/session_capacity_acceptance_peer.rs`，更新 `app/Cargo.toml`，并在 `testing/suites/verify-suite.mjs` 新增对 `Cargo.toml` 路径不得指向忽略目录的机器检查。
- 清理 `approval-owner-acceptance-fixtures`、`native-dispatch-dev-fixtures` 等特性组合下的 `private_interfaces`、`dead_code` 与 Clippy 告警，在 `.github/workflows/ci.yml` 增加 `cargo clippy --manifest-path app/Cargo.toml --all-targets --all-features --locked -- -D warnings` 门禁。
- 重构 `app/src/storage.rs` 及其子模块：取消 `impl Deref for Storage`，提取应用运行时监督与关闭协调结构（`RuntimeSupervisorSlot` / 运行时关闭协调器），使 `Storage` 聚焦 SQLite 生命周期、连接所有者与核心元数据访问，同时移除 `Storage::approvals`、`Storage::histories`、`Storage::sessions` 废弃测试入口。
- 推进核心控制面 SQLite `SCHEMA_VERSION` 由 `14` 升至 `15`：将 `harnesses` 与 `workspaces` 独立为目录基础父表定义；新库（`v0 -> v15`）不再创建其余 15 张第一代 `SESSION` 死表与 3 个旧索引；既有库（`v2..=v14 -> v15`）在 `BEGIN IMMEDIATE` 迁移事务内按外键拓扑顺序清理这批死表与旧索引，并在 `v15` 校验中确认其彻底缺席；同步删除 `domain::{history, approvals}`、`application::{history, approvals}`、`InputRepository`/`InputService` 及 `session_storage::{legacy, history, approvals}` 废弃代码。

## Capabilities

### New Capabilities
- 无

### Modified Capabilities
- 无

## Impact

- 构建与工程校验：`app/Cargo.toml`、`app/dev-fixtures/`、`testing/suites/verify-suite.mjs`、`.github/workflows/ci.yml`。
- 存储与运行时装配：`app/src/storage.rs`、`app/src/storage/`、`app/src/session_storage/schema.rs`、`app/src/session_storage/mod.rs`、`app/src/domain/mod.rs`、`app/src/application/mod.rs` 及被移除的第一代 `#[cfg(test)]` 遗留模块。
- 前置与后续衔接：在已归档 [deliver-session-capacity](../2026-11-19-deliver-session-capacity/proposal.md) 之上收敛工程底座，为后续前端工作台预重构及 [deliver-session-control-takeover](../../deliver-session-control-takeover/proposal.md)、[deliver-session-transcript-cache](../2026-11-20-deliver-session-transcript-cache/proposal.md) 提供干净基线。

## Exploration

- **清理范围确认（用户确认 A1）**：确认在本次工程收敛变更中一并清理第一代 `SESSION` 聊天正文死表及 `legacy`/`history`/`approvals` 测试遗留代码，采用 `SCHEMA_VERSION = 15` 迁移兼容历史测试迁移链（如 `v2/v3/v4` 升级用例）并收口新库基线；经核查 `node_catalog_schema.rs` 与 `node_management_audit` 外键约束，`harnesses`、`workspaces` 及 `product_sources`/`product_harness_sources`/`product_targets` 仍为现役父表，予以完整保留。
- **工作区基线处理确认（用户确认 A2）**：已按用户确认先将分支 `v2` 上已完成的 CT-001/TC-001 远期规划修订独立提交为规划 commit（`35d048d`），确保本变更在干净受跟踪基线上建立。
- **术语与 ADR 复核**：复用 [领域术语表](../../../GLOSSARY.md) 既有定义，不引入新业务概念，无需新增独立 ADR。
