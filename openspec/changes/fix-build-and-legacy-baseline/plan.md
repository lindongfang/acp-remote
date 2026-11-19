# Plan

## Execution Model

- **执行机制**：主执行者统筹活跃变更 `fix-build-and-legacy-baseline`（`.openspec.yaml` 声明 `skip_specs: true`），`coder` 在编码前门禁通过后按工作包实现并完成单元/回归自测闭环；全部开发工作包合入并锁定单一本地分支 Git `commit SHA` 后，由独立 `tester` 执行验收用例、独立 `reviewer` 执行交付检视。
- **规划基线**：本地分支 `v2`，基线提交 `35d048d`（已先行独立提交 CT-001/TC-001 远期规划文档与 `x-deferred` 变更，受跟踪工作区干净）。
- **隔离边界**：`tester` 与 `reviewer` 均通过独立子代理会话执行，不继承 `coder` 开发对话历史；若并行运行多组 Cargo 编译或测试，通过独立 `CARGO_TARGET_DIR` 与系统临时目录隔离构建锁和 `.sqlite3` 测试文件。
- **双门禁约束**：① **编码前门禁**——`node testing/suites/verify-suite.mjs --change fix-build-and-legacy-baseline` 与 `npm run check:spec` 通过后，由独立 `reviewer` 一次性合审 [proposal.md](proposal.md)、[design.md](design.md)、[tests.md](tests.md)、本文件与 [tasks.md](tasks.md)，在下方 `## Code Review` 将编码前门禁标记为 `passed` 后方可进入产品代码修改；② **集成交付门禁**——`WP1`~`WP3` 实施任务完成自测勾选并提交为干净的受测 `commit SHA`（集成门禁任务 `4.1` 在该提交中保持未勾选）后，并行运行独立 `tester` 与独立 `reviewer` 门禁，通过后以纯文档收尾提交回填 `Verification Results` 与门禁状态。

## Parallel Assessment

- **可并行部分**：`WP1` 中 `testing/suites/verify-suite.mjs` 与 `.github/workflows/ci.yml` 的工程脚本修改，同 `WP2`/`WP3` 的 `app/src/` Rust 源码无写范围冲突，可在设计冻结后先行准备。
- **需串行部分**：
  1. `WP1` 与 `WP3` 在 `app/src/session_storage/schema.rs` 存在写范围交集（`WP1` 修复第 1151 行 `initialize_approval_for_test` 的全特性 Clippy 诊断，`WP3` 推进 `SCHEMA_VERSION = 15` 迁移与校验），故 Rust 侧严格按 `WP1 -> WP2 -> WP3` 串行推进。
  2. `WP2`（移除 `StorageData` + `impl Deref for Storage` 并提取 `StorageRuntimeCoordinator`，暂保留 `#[cfg(test)]` 旧访问器转发以维持 `--lib` 中间态可编译）与 `WP3`（将 `app/src/storage.rs` 中 `SCHEMA_VERSION` 升至 `15` 并原子删除 `Storage::{approvals,histories,sessions}` 及第一代遗留模块）共享 `app/src/storage.rs` 写范围且共享同一 Cargo 构建目标，必须串行合入。
  3. `WP4` 集成交付门禁严格依赖 `WP1`、`WP2`、`WP3` 全部合入并生成干净受测 `commit SHA`。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 达成 `SC1`、`SC2`（对应 `FBL-001`~`FBL-004`）：将 `session_capacity_acceptance_peer` 迁入 `app/dev-fixtures/` 并将 `node.rs` 引用的 4 份 `capacity_*_acceptance.rs` 迁入 `app/src/node/session_services/`，在 `verify-suite.mjs` 增加 `Cargo.toml` 与 Rust 编译期路径忽略前缀/缺失源码拦截，清零 `--all-features --all-targets` 编译与 Clippy 告警并在 CI 固化门禁 | none | coder | `app/Cargo.toml`、`app/dev-fixtures/session_capacity_acceptance_peer.rs`、`app/src/node.rs`、`testing/suites/verify-suite.mjs`、`.github/workflows/ci.yml`、`app/src/application/native_dispatch/`、`app/src/session_storage/approval_owner.rs`、`app/src/session_storage/approval_owner/`、`app/src/session_storage/registry_connection_owner.rs`、`app/src/session_storage/schema.rs`、`app/src/node/approval_owner/`、`app/src/node/session_services/` | 入库验收对端与节点验收源码、静态防复发检查规则、全特性零告警修复与 CI 配置 | `node testing/suites/verify-suite.mjs`、`cargo clippy --manifest-path app/Cargo.toml --all-targets --all-features --locked -- -D warnings` |
| WP2 | 达成 `SC3` 核心运行时解耦（支撑 `FBL-005`、`FBL-006`）：移除 `StorageData` 与 `impl Deref for Storage`，提取私有 `StorageRuntimeCoordinator` 统一协调队列与准备阶段运行时关闭顺序（`#[cfg(test)]` 下的 `Storage::{approvals,histories,sessions}` 暂保留内部转发以确保 `WP2` `--lib` 自包含编译，留交 `WP3` 原子同删） | WP1 | coder | `app/src/storage.rs`、`app/src/storage/`、`docs/implementation/session-capacity.md` | 私有化内部表示的 `Storage` 与 `runtime_coordinator.rs` 及生命周期单测 | `cargo test --manifest-path app/Cargo.toml --lib storage:: --locked` 及 `registration_*` 关闭所有权套件 |
| WP3 | 达成 `SC4` 并闭环 `SC3` 旧访问器清理（对应 `FBL-005`、`FBL-007`~`FBL-010`）：推进 SQLite `SCHEMA_VERSION = 15`，保留 `harnesses`/`workspaces` 与现役 `product_*` 表，在新库不建且在 `v1..=v14` 升级时安全清理 15 张第一代 `SESSION` 死表与 3 个旧索引；原子移除 `Storage::{approvals,histories,sessions}` 与 `legacy`/`history`/`approvals` 废弃模块，在 `session_storage/mod.rs` 补挂 `#[cfg(test)] mod tests;`，并同步对齐 9 份 `session_storage` 子模块单测与 `registration_acceptance` 历史迁移回归断言 | WP2 | coder | `app/src/storage.rs`、`app/src/session_storage/schema.rs`、`app/src/session_storage/mod.rs`、`app/src/session_storage/tests.rs`、`app/src/session_storage/legacy.rs`、`app/src/session_storage/history.rs`、`app/src/session_storage/history/`、`app/src/session_storage/approvals.rs`、`app/src/session_storage/approvals/`、`app/src/session_storage/approval_owner_schema/`、`app/src/session_storage/product_schema/`、`app/src/session_storage/product_commands/`、`app/src/session_storage/request_registry/`、`app/src/session_storage/native_dispatch_schema/`、`app/src/session_storage/session_capacity/`、`app/src/session_storage/automatic_capacity_release_schema/`、`app/src/session_storage/session_tree_schema/`、`app/src/domain/mod.rs`、`app/src/domain/history.rs`、`app/src/domain/approvals.rs`、`app/src/application/mod.rs`、`app/src/application/history.rs`、`app/src/application/history/`、`app/src/application/approvals.rs`、`app/src/application/approvals-development.md`、`app/tests/registration_acceptance/`、`app/tests/acceptance/`、`docs/HARNESS_CONTROL_PLANE_REDESIGN.md` | `SCHEMA_VERSION = 15` 迁移/校验实现、死代码与旧访问器原子清理、`session_storage` 子模块及历史迁移 `v15` 断言对齐 | `cargo clippy --manifest-path app/Cargo.toml --all-targets --all-features --locked -- -D warnings`、`cargo test --manifest-path app/Cargo.toml --all-targets --all-features --no-run --locked`、干净特性回归子集（`--features registration-acceptance-fixtures`、`--features acp-runtime-acceptance-fixtures`、`--lib --all-features storage::`、`--lib --all-features session_storage::`、`capacity_rb001_`）与 `npm run check` |
| WP4 | 完成集成交付门禁（覆盖 `FBL-001`~`FBL-010` 全量验收与独立代码检视）：基于单一本地受测 `commit SHA` 并行执行独立 `tester` 验收与独立 `reviewer` 检视，闭环全部阻断项并回填结果摘要 | WP1, WP2, WP3 | tester, reviewer | `openspec/changes/fix-build-and-legacy-baseline/plan.md`、`openspec/changes/fix-build-and-legacy-baseline/tasks.md`（原始运行证据另存本地被忽略目录 `testing/runs/fix-build-and-legacy-baseline/`） | 独立验收结论、独立代码检视结论与 `plan.md` 结果归档 | `node testing/suites/verify-suite.mjs --change fix-build-and-legacy-baseline`、`npm run check:spec` |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 先恢复受版本控制源码树的自包含编译、`verify-suite.mjs` 忽略路径静态守卫与全特性零告警基线（含 `schema.rs` 特性测试函数 Clippy 修复，供 `WP3` 在此基础上演进） | `node testing/suites/verify-suite.mjs` 与 `cargo clippy --manifest-path app/Cargo.toml --all-targets --all-features --locked -- -D warnings` |
| 2 | WP2 | 先解耦 `Storage` 的 `StorageData`/`Deref` 并提取 `StorageRuntimeCoordinator`，暂保留 `#[cfg(test)]` 旧访问器使 `--lib` 单测与 5 套 `registration_*` 关闭所有权套件独立通过 | `cargo test --manifest-path app/Cargo.toml --lib storage:: --locked` 与 5 套 `registration_*` 验收测试 |
| 3 | WP3 | 在 `WP2` 之上原子完成 `SCHEMA_VERSION = 15` 迁移、`Storage::{approvals,histories,sessions}` 与第一代遗留模块同删、`session_storage/mod.rs` 单测重挂载、子模块与历史迁移断言对齐及全特性终验 | `cargo clippy --manifest-path app/Cargo.toml --all-targets --all-features --locked -- -D warnings`、`npm run check`、`npm run build:rust` |
| 4 | WP4 | 针对 `WP1`~`WP3` 提交后的单一干净 `commit SHA` 执行独立 `tester` 与 `reviewer` 门禁，最后纯文档提交勾选门禁任务 | `node testing/suites/verify-suite.mjs --change fix-build-and-legacy-baseline`、`npm run check:spec`、`git diff --check` |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | 根目录 `D:\Project\acp-remote`：`git archive HEAD` 解压至无 `testing/runs/` 临时目录后运行 `cargo build --manifest-path app/Cargo.toml --bin session_capacity_acceptance_peer --features native-dispatch-dev-fixtures --locked` 与临时副本负向篡改 `node testing/suites/verify-suite.mjs` | 对应 `FBL-001`、`FBL-002`：洁净源码树退出码 0 编译通过；指向被忽略目录或缺失文件的负向篡改 100% 非零退出拦截 |
| PV2 | 根目录 `D:\Project\acp-remote`：`cargo clippy --manifest-path app/Cargo.toml --all-targets --all-features --locked -- -D warnings` 及 6 个独立 feature 单开 `RUSTFLAGS="-D warnings" cargo check` | 对应 `FBL-003`：全特性及各独立特性下编译与 Clippy 零告警通过，全特性测试可执行文件成功链接 |
| PV3 | 根目录 `D:\Project\acp-remote`：`cargo doc --manifest-path app/Cargo.toml --no-deps`、`cargo test --manifest-path app/Cargo.toml --lib storage:: --locked` 与 `cargo test --manifest-path app/Cargo.toml --features registration-acceptance-fixtures --test registration_shutdown_ownership --test registration_owner_acceptance --test registration_reconcile_ownership --test registration_restart --test registration_factory_waiter --locked` | 对应 `FBL-005`、`FBL-006`：集成终态下 `StorageData`/`Deref` 与 `Storage::{approvals,histories,sessions}` 零残留，队列单次认领与“先停运行时监督再封存物理连接池”不变量零回归 |
| PV4 | 根目录 `D:\Project\acp-remote`：`cargo test --manifest-path app/Cargo.toml --lib session_storage::tests:: --locked`、`cargo test --manifest-path app/Cargo.toml --lib session_storage:: --locked`、`cargo test --manifest-path app/Cargo.toml --lib schema3_migration --locked`、`cargo test --manifest-path app/Cargo.toml --lib schema4_migration --locked`、`cargo test --manifest-path app/Cargo.toml --all-targets --all-features --no-run --locked`，以及干净全特性套件回归（`--features registration-acceptance-fixtures --locked`、`--features acp-runtime-acceptance-fixtures --locked`、`--lib --all-features storage:: --locked`、`--lib --all-features session_storage:: --locked` 与 `SOURCE_SESSION_CHILD` 单线程 `capacity_rb001_`） | 对应 `FBL-007`~`FBL-010`：`session_storage::tests::` 重挂载后实跑数大于 0，`v0 -> v15` 新库零死表、`v1..=v14 -> v15`（含双向外键行）平滑升级且现役数据无损、`>15` 与残留死表预检拒绝、9 份子模块单测与历史迁移测试全绿、7 个第一代遗留模块文件及符号彻底清零；范围外基线既有的旧开发夹具说明留交 WP4 同步确认 |
| PV5 | 根目录 `D:\Project\acp-remote`：`npm run check`、`npm run build:rust`、`git diff --check` | 对应 `FBL-004` 及整体交付门禁：规范、Web、Rust 全套门禁与构建退出码 0，`.github/workflows/ci.yml` 包含 `--all-features` Clippy 步骤，无计划外依赖变更 |

## Code Review

| Gate / Scope | Reviewer Agent | Review Focus | Pass Condition | Status | Reviewed Snapshot / Notes |
| --- | --- | --- | --- | --- | --- |
| 编码前门禁（`proposal`/`design`/`tests`/`plan`/`tasks`） | 独立 `reviewer` | 核查 `skip_specs: true` 适用边界、`SC1`~`SC4` 100% 映射、现役外键父表（`harnesses`/`workspaces`/`product_*`）保护、`approvals <-> approval_options` 双向外键迁移顺序与测试覆盖充分性 | `verify-suite.mjs --change fix-build-and-legacy-baseline` 与 `npm run check:spec` 通过且独立 `reviewer` 无阻断问题 | passed | 基于分支 `v2` 基线 `35d048d` 完成增量复审（`FBL-R001`~`FBL-R003` 已全部关闭；`verify-suite.mjs --change fix-build-and-legacy-baseline` 与隔离洁净快照 `openspec validate` 全部通过；详见 `testing/runs/fix-build-and-legacy-baseline/reviewer/pre-code-gate-review.md`） |
| 集成交付门禁（受测集成 `commit SHA` 全量 diff 与验收证据） | 独立 `reviewer` | 核查受测 `commit SHA` 真实代码 diff、`verify-suite.mjs` 与 CI 门禁有效性、`Storage` 关闭时序安全性、`SCHEMA_VERSION = 15` 事务原子性、敏感信息零残留与临时测试资源清理 | `FBL-001`~`FBL-010` 独立验收通过且全部审查阻断项在新 commit 复核闭环 | pending | 待 `WP1`~`WP3` 实施完成并锁定干净本地受测 `commit SHA` 后启动 |

## Main E2E

```yaml
mode: not-applicable
reason: pure-refactor
basis: 本变更属于内部构建卫生修复、Storage 运行时协调封装与第一代已废弃 SQLite 死表及测试遗留代码收口的纯重构与基线清理，不改变任何 HTTP 配对接口、管理 WebSocket 协议契约、ACP 宿主交互行为或 Web 前端界面。
alternative_checks:
  - node testing/suites/verify-suite.mjs --change fix-build-and-legacy-baseline
  - cargo clippy --manifest-path app/Cargo.toml --all-targets --all-features --locked -- -D warnings
  - cargo test --manifest-path app/Cargo.toml --all-targets --all-features --no-run --locked
  - cargo test --manifest-path app/Cargo.toml --features registration-acceptance-fixtures --locked
  - cargo test --manifest-path app/Cargo.toml --features acp-runtime-acceptance-fixtures --locked
  - cargo test --manifest-path app/Cargo.toml --lib --all-features storage:: --locked
  - cargo test --manifest-path app/Cargo.toml --lib --all-features session_storage:: --locked
  - npm run check
```

## Risks

- **`approvals` 与 `approval_options` 双向外键环阻断删除风险**：SQLite 开启 `PRAGMA foreign_keys = ON` 时，若旧开发库两表内存在互指记录，直接 `DROP TABLE` 会触发外键约束错误；在 `v15` 迁移事务内先将 `approvals.selected_choice_id` 置 `NULL` 并清空 `approval_options`，且由 `FBL-008` 构造真实互指数据验证。
- **`session_storage/tests.rs` 删除 `legacy.rs` 后静默失联风险**：`session_storage/tests.rs` 当前仅在 `legacy.rs` 内通过 `#[path = "tests.rs"] mod tests;` 挂载；`WP3` 删除 `legacy.rs` 时同步在 `session_storage/mod.rs` 补挂 `#[cfg(test)] mod tests;`，并在 `PV4` 显式执行 `--lib session_storage::tests::` 校验实跑用例数非零。
- **现役目录/产品父表误伤风险**：`node_catalog_schema`、`node_management_audit`、`node_approval_audit` 现役外键依赖 `harnesses`、`workspaces`、`product_sources`、`product_harness_sources`、`product_targets`；通过将 `harnesses`、`workspaces` 独立为 `CATALOG_TABLES` 并在迁移结束前强制运行 `PRAGMA main.foreign_key_check` 双重兜底。
- **`Storage` 关闭顺序回归风险**：`registration_*` 验收测试依赖 `Storage::close` 在封存连接池前先取消并等待队列 `RuntimeSupervisor` 退出；通过 `StorageRuntimeCoordinator` 原样保留 `close_in_scope` 时序并运行全部 5 套 `registration_*` 验收测试验证。

## Handoff

- **编码阶段交接输入（面向 `coder`）**：编码前门禁状态更新为 `passed` 后，按 `WP1 -> WP2 -> WP3` 顺序在本地分支 `v2`（或派生工作分支）实施；仅读取 `openspec/config.yaml`、`AGENTS.md`、`docs/code-standards.md` 与 `openspec/changes/fix-build-and-legacy-baseline/` 下规划产物，严禁篡改 `tests.md` 验收标准；完成 `1.x`~`3.x` 单测与文档同步并勾选对应任务后提交受测集成 `commit SHA`，保持 `4.1` 未勾选并停手交接。
- **门禁阶段交接输入（面向独立 `tester` / `reviewer`）**：仅交接干净受测本地分支 `commit SHA`、`openspec/role/{tester,reviewer}.md`、`openspec/changes/fix-build-and-legacy-baseline/` 文档及 `PV1`~`PV5` 检查入口，不传递任何 `coder` 开发上下文；原始运行日志写入本地被忽略的 `testing/runs/fix-build-and-legacy-baseline/`。

## Findings

| ID | Source | Severity | Commit SHA | Finding | Fix Commit | Status | Retest / Re-review |
| --- | --- | --- | --- | --- | --- | --- | --- |
| FBL-R001 | reviewer | high | 35d048d | `WP2` 计划先于 `WP3` 删除 `Storage::{approvals, histories, sessions}` 并运行 `cargo test --lib storage::`，但 `session_storage/{tests.rs,history/tests.rs,approvals/tests.rs}` 仍在 `--lib` 单测目标中调用上述方法直至 `WP3` 才删除，将导致 `WP2` 中间态单测直接编译失败 | working-tree (design/plan/tasks) | closed | 独立 `reviewer` 增量复审确认已调整为 `WP2` 保留 `#[cfg(test)]` 内部转发、`WP3` 与第一代遗留模块原子同删（`pre-code-gate-review.md` Round 2） |
| FBL-R002 | reviewer | high | 35d048d | ① `app/src/session_storage/tests.rs` 当前仅由 `legacy.rs` 挂载，`WP3` 删除 `legacy.rs` 后若未在 `session_storage/mod.rs` 补挂 `#[cfg(test)] mod tests;` 会导致新 `v15` 单测静默失联；② `design.md` 与 `WP3` `Write Scope` 遗漏 `app/src/session_storage/` 下 9 份依赖 `schema::SESSION`、查询第一代死表或断言旧 DDL 全量保留/`head == 14` 的现役子模块单测 | working-tree (design/plan/tasks) | closed | 独立 `reviewer` 增量复审确认已补齐 `session_storage/mod.rs` 重挂载、`PV4` 实跑计数校验及全部 9 个子模块目录写范围（`pre-code-gate-review.md` Round 2） |
| FBL-R003 | reviewer | medium | 35d048d | 基线 `35d048d` 下实测 `--all-features --all-targets` Clippy 另有 9 个文件（`app/src/node/approval_owner/development_tests.rs`、`app/src/node/session_services/capacity_l6_*.rs` 共 6 个文件、`app/src/session_storage/registry_connection_owner.rs`、`app/src/session_storage/schema.rs`）位于 `WP1` `Write Scope` 之外，且 `schema.rs` 与 `WP3` 存在写范围交集 | working-tree (design/plan/tasks) | closed | 独立 `reviewer` 增量复审确认已将上述全部路径纳入 `WP1` `Write Scope` 与 `design.md` Decision 1，并明确 `schema.rs` 串行交集（`pre-code-gate-review.md` Round 2） |

## Verification Results

- 受测版本：未执行（当前处于编码前规划阶段，基线 commit `35d048d`）
- 执行者与环境：待编码实现并锁定受测集成 `commit SHA` 后由独立 `tester` 与独立 `reviewer` 在 Windows 环境执行
- 验证命令及退出码：当前仅执行规划结构校验（`node testing/suites/verify-suite.mjs --change fix-build-and-legacy-baseline` 与 `npm run check:spec`）

| Case ID | Executor | Commit SHA | Status | Evidence / Limitations |
| --- | --- | --- | --- | --- |
| FBL-001 | pending | n/a | not-run | 编码前规划阶段，待锁定受测集成 commit SHA 后执行 |
| FBL-002 | pending | n/a | not-run | 编码前规划阶段，待锁定受测集成 commit SHA 后执行 |
| FBL-003 | pending | n/a | not-run | 编码前规划阶段，待锁定受测集成 commit SHA 后执行 |
| FBL-004 | pending | n/a | not-run | 编码前规划阶段，待锁定受测集成 commit SHA 后执行 |
| FBL-005 | pending | n/a | not-run | 编码前规划阶段，待锁定受测集成 commit SHA 后执行 |
| FBL-006 | pending | n/a | not-run | 编码前规划阶段，待锁定受测集成 commit SHA 后执行 |
| FBL-007 | pending | n/a | not-run | 编码前规划阶段，待锁定受测集成 commit SHA 后执行 |
| FBL-008 | pending | n/a | not-run | 编码前规划阶段，待锁定受测集成 commit SHA 后执行 |
| FBL-009 | pending | n/a | not-run | 编码前规划阶段，待锁定受测集成 commit SHA 后执行 |
| FBL-010 | pending | n/a | not-run | 编码前规划阶段，待锁定受测集成 commit SHA 后执行 |

- 未执行项与已知限制：`FBL-001`~`FBL-010` 全部处于编码前待执行状态（`not-run`），将在 `WP1`~`WP3` 合入后由独立 `tester` 完整执行并将原始日志归档于 `testing/runs/fix-build-and-legacy-baseline/`。
- 问题关闭依据：none
