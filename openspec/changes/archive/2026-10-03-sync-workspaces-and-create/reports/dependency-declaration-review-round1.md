# Dependency Declaration Review — DPR-plan-1 (Round 1)

- Review ID: `DPR-plan-1`
- Round: 1
- Phase / Stage: `phase: plan` / `stage: plan`
- Reviewer: independent reviewer subagent (read-only; not an owner of any work package in this plan)
- Plan Revision (contractDigest at review time): `sha256:4657eabba8b8edad309934559bec95578953b933845c0dc04f147a1f0ab6b45b`
- Verdict: **FAIL**

## 结论

依赖**方向**的声明（WP4 的 `code:WP3`、WP1–WP3 的 `none`、`R1` 构建目录可隔离）全部属实；但 **Write Scope / Shared File Ownership 的完备性声明不成立**：WP2/WP3 已冻结的 Rust 形状变更必然破坏 `crates/server` 的编译，而四个工作包的写范围都不含 `crates/server`，因此 PV1 与 ALT2 无法按计划变绿。另有 1 项 MAJOR 与 3 项 MINOR。

## 逐项核对（摘要）

| 声明 | 核对对象 | 证据 | 结论 |
| --- | --- | --- | --- |
| WP4 `code:WP3` | storage-sqlite 能否脱离 core 改动编译 | `crates/storage-sqlite/Cargo.toml` 的 `core = { path = "../core" }`；`src/session_store.rs` 导入 `acp_core::model::SessionSummary` 与 `acp_core::ports::*` | 属实 |
| WP1–WP3 `none` | 三者是否真不需要彼此代码 | WP1（词表/文档）、WP2（sync wire）、WP3（core 投影）互不依赖对方 crate | 属实 |
| `resource:R1` 可隔离 | `CARGO_TARGET_DIR` 是否每 WP 独立 | 环境变量，可按 WP 分配独立目录 | 属实 |
| Write Scope 完备性 | 被冻结接口变更的编译波及是否都有属主 | 见下方 BLOCKER | **不成立** |

## 发现的问题

1. **[BLOCKER]** `crates/server` 的必然编译破坏不在任何 WP 的 Write Scope 内。
   - (a) WP2 给 wire 类型 `sync_protocol::common::SessionSummary` 增加公开字段 `workspace`（定义见 `crates/sync-protocol/src/common.rs`），而 `crates/server/src/node_link/command.rs` 以**全字段字面量**构造该类型（无 `..` 尾）⇒ E0063，`crates/server` 编译失败；design 的 Non-Goal「不扩展 Node Link 的投影」恰恰意味着该处必须写 `workspace: None`。
   - (b) WP3 给 core `SessionSummary` 增加字段（定义见 `crates/core/src/model/session.rs`，唯一构造器 `try_new` 按投影字段位置传参），`crates/server/src/node_link/command/tests.rs`（多处）与 `crates/server/src/node_link/resource/tests.rs` 以位置参数调用 ⇒ 全部编译失败。
   - 后果：PV1 / ALT2（`--workspace` 级 clippy/test）必红，且无处可派修复。
   - 最小修复：把上述 `crates/server` 文件加入 WP2/WP3 的写范围，并在 Shared File Ownership 登记该目录的合并安排。
2. **[MAJOR]** WP4 的交接包含性检查「`cargo test -p storage-sqlite` 能在该基线编译」在其起点（WP3 已合入、WP4 未开工）不可满足：`crates/storage-sqlite/src/session_store.rs` 的 `SessionSummary::try_new(...)` 位置调用与 `tests/` 中多处 `SessionUpdate` 全字段字面量都会因 WP3 的接口变更而失败。
   - 最小修复：该检查改为核对 WP3 交付物（`cargo test -p core` + `check-contract-drift` 的 §5 部分），storage-sqlite 的编译证据归 WP4 自身验收。
3. **[MINOR]** design 指定的 `docs/SYNC_PROTOCOL.md` §9.6 注记（imported 会话 `workspace` 恒为 `null`）不在任何 WP 的可写区域内。
   - 最小修复：把 §9.6 加入 WP2 在该文件的区域清单。
4. **[MINOR]** Local Checks 引用不存在的 `scripts/check-sync-assets.mjs`（仓库实际存在的是 `check-schema-fixtures.mjs` 与 `check-contract-assets.mjs`）。
   - 最小修复：替换为实际存在的脚本名。
5. **[MINOR]** `docs/FRONTEND_DESIGN.md` 第 9.1 节 不存在（§9 是扁平编号列表），而 proposal/design/plan 三处引用该章节号。
   - 最小修复：统一改为「§9 第 1 条」。

## 阻断性依赖环

无。
