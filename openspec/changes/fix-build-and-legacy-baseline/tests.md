# Tests

本文由独立 `tester` 依据 [proposal.md](proposal.md)、[design.md](design.md) 与当前仓库真实代码基线编写，定义 `fix-build-and-legacy-baseline` 变更的可重复验收用例。本变更在 `.openspec.yaml` 声明 `skip_specs: true`，不改变外部网络协议或前端交互规格，验收聚焦洁净构建边界、全特性静态门禁、`Storage` 运行时封装与关闭顺序不变量，以及 SQLite `SCHEMA_VERSION = 15` 死表收口与历史升级安全性。详细执行日志在正式验收阶段另存至本地被 Git 忽略的 `testing/runs/fix-build-and-legacy-baseline/` 目录，精简逐例结论回填至同目录施工计划文档。

## 共用执行条件与真实边界

- **版本快照与角色独立性**：正式集成验收必须在与独立 `reviewer` 共享的同一受测本地分支 Git `commit SHA`（`git rev-parse HEAD` 且受跟踪工作区 `git status` 干净）上执行；`tester` 不继承 `coder` 开发会话，不以开发自测代替独立验收。
- **洁净无忽略产物工作区边界**：验证 `SC1` 时不得依赖开发机工作区残留的 `testing/runs/`、`.tmp/`、`target/`、`dist/` 或 `node_modules/` 本地忽略产物；须通过 `git archive` 导出或全新隔离 `git worktree` 构造完全不含 `testing/runs/` 的洁净源码树，真实检验受版本控制源码的自包含编译能力。负向规则测试必须在系统临时目录副本中注入违规配置并核验非零退出码，严禁原地污染受跟踪仓库文件。
- **隔离数据目录与真实 SQLite 边界**：全部 `Storage` 生命周期与 SQLite Schema 迁移验证均使用真实 SQLite 引擎（含 `vendor/sqlx-sqlite` 连接所有者与生命周期跟踪、WAL 模式及 `PRAGMA foreign_keys = ON`），禁止用内存 Mock 替代磁盘 SQLite 文件、WAL/SHM 句柄及连接池物理关闭收敛检查。每条数据库用例必须使用系统临时目录（如 `%TEMP%\acp-remote-fbl-<pid>-<nonce>\`）下的独占 `.sqlite3` 文件与独立节点数据根，严禁读取、修改或删除用户真实运行数据目录、私钥或本机私有配置。
- **工具链与清理要求**：Rust 工具链严格遵循仓库 `rust-toolchain.toml`，Node 工具链遵循 `.nvmrc` 与根目录 `package.json`。并行执行编译或测试时使用独立 `CARGO_TARGET_DIR` 防止构建锁冲突；无论用例通过、失败或中断，结束前均须停止本次拉起的受管子进程树（如 `registration_crash_probe`、`session_capacity_acceptance_peer` 或节点进程），关闭全部数据库连接并清理临时目录及 `.sqlite3`、`.sqlite3-wal`、`.sqlite3-shm` 文件。本变更全部判据均可通过命令行、真实编译器与真实 SQLite 探针自动化验证，不设 `LX` 人工判断用例。

## 需求关联

| 来源文档 | 章节 | 成功判据 | 用例 |
| --- | --- | --- | --- |
| proposal.md | Success Criteria | SC1 | FBL-001、FBL-002 |
| proposal.md | Success Criteria | SC2 | FBL-003、FBL-004 |
| proposal.md | Success Criteria | SC3 | FBL-005、FBL-006 |
| proposal.md | Success Criteria | SC4 | FBL-007、FBL-008、FBL-009、FBL-010 |

## Cases

### FBL-001 洁净无忽略目录工作树下验收对端源码在库与编译验证

- 用例级别：L1
- 操作步骤：1) 在受测 `commit SHA` 上执行 `git ls-files --error-unmatch app/dev-fixtures/session_capacity_acceptance_peer.rs`，并检查 `app/Cargo.toml` 中所有 `[[bin]]`、`[[test]]`、`[lib]` 与 `[patch.crates-io]` 的 `path` 字段；2) 使用 `git archive HEAD` 解压至系统临时目录（如 `%TEMP%\acp-remote-clean-src-<nonce>`，确保目录内物理不存在 `testing/runs/`、`.tmp/`、`target/` 等任何 `.gitignore` 忽略目录），结合已锁定依赖在该洁净副本上执行 `cargo build --manifest-path app/Cargo.toml --bin session_capacity_acceptance_peer --features native-dispatch-dev-fixtures --locked` 与 `cargo check --manifest-path app/Cargo.toml --all-targets --all-features --locked`；3) 验证完成后删除临时洁净源码目录与临时 `CARGO_TARGET_DIR`。
- 预期结果：`app/dev-fixtures/session_capacity_acceptance_peer.rs` 已纳入 Git 版本控制且源码非空，`app/Cargo.toml` 中 `session_capacity_acceptance_peer` 的 `path` 精确指向 `"dev-fixtures/session_capacity_acceptance_peer.rs"` 且不再包含任何 `testing/runs/` 跨界引用；在完全不存在 `testing/runs/` 目录的洁净源码树中，`session_capacity_acceptance_peer` 二进制与 `--all-targets --all-features` 全部目标均以退出码 0 编译通过。

### FBL-002 `verify-suite.mjs` 对 `Cargo.toml` 忽略路径与缺失源码的负向防复发拦截验证

- 用例级别：L2
- 操作步骤：1) 在当前干净仓库根目录执行 `node testing/suites/verify-suite.mjs --change fix-build-and-legacy-baseline` 与 `node testing/suites/verify-suite.mjs` 记录正向基线；2) 在系统临时目录创建受检工作区最小隔离副本（不修改当前 Git 工作区任何文件），依次构造五组负向篡改场景并分别运行副本内的 `verify-suite.mjs`：场景 A 将 `app/Cargo.toml` 某目标 `path` 改为 `"../testing/runs/deliver-session-capacity/peer.rs"`（即使在副本磁盘预建该文件）、场景 B 改为 `"../.tmp/temp_peer.rs"`、场景 C 改为 `"target/generated_peer.rs"`、场景 D 改为 `"../dist/fake_peer.rs"` 或 `"../node_modules/fake_peer.rs"`、场景 E 改为受跟踪前缀下磁盘不存在的 `"dev-fixtures/missing_fixture_peer.rs"`；3) 核对各场景退出码与错误输出后彻底清理临时副本目录。
- 预期结果：正向基线两次 `verify-suite.mjs` 调用均退出码 0 通过；场景 A 至场景 E 五组负向测试中，`verify-suite.mjs` 均以非零退出码失败退出，并在错误输出中精确定位 `app/Cargo.toml` 违规 `path` 条目（明确拦截指向 `testing/runs/`、`.tmp/`、`target/`、`dist/`、`node_modules/` 等被忽略路径以及磁盘缺失源码路径），证明静态防复发守门有效。

### FBL-003 Rust 全特性与单特性组合零告警编译、Clippy 及测试链接验证

- 用例级别：L1
- 操作步骤：1) 在仓库根目录执行 `cargo clippy --manifest-path app/Cargo.toml --all-targets --all-features --locked -- -D warnings`；2) 分别对默认特性集以及 `app/Cargo.toml` 声明的 6 个独立特性（`shared-view-acceptance-fixtures`、`registration-acceptance-fixtures`、`acp-runtime-acceptance-fixtures`、`native-dispatch-dev-fixtures`、`approval-owner-dev-fixtures`、`approval-owner-acceptance-fixtures`）设置 `RUSTFLAGS="-D warnings"` 执行 `cargo check --manifest-path app/Cargo.toml --all-targets --no-default-features --features <feature> --locked`；3) 执行 `cargo test --manifest-path app/Cargo.toml --all-targets --all-features --no-run --locked` 验证全特性测试可执行文件完整编译与链接。
- 预期结果：全特性 `cargo clippy`、6 个单特性独立检查及全特性测试目标链接全部以退出码 0 完成，编译器与 Clippy 输出零 `private_interfaces` 可见性告警、零 `dead_code` 或未使用测试钩子告警、零 `clippy::all` / `clippy::pedantic` / `clippy::nursery` 告警，任意单个或全量特性开启时均无符号缺失或条件编译断裂。

### FBL-004 全量工程检查 `npm run check` 与 `.github/workflows/ci.yml` 全特性门禁核验

- 用例级别：L2
- 操作步骤：1) 在仓库根目录执行 `npm run check`（串联 `check:spec`、`check:web`、`check:rust`）与 `npm run build:rust`，并执行 `git diff --check` 检查工作区格式卫生；2) 读取 `.github/workflows/ci.yml` 与 `package.json`，核对持续集成 `product` 任务（或 `check:rust` 入口）是否已显式纳入 `cargo clippy --manifest-path app/Cargo.toml --all-targets --all-features --locked -- -D warnings` 门禁步骤；3) 核查 `Cargo.lock`、`package-lock.json` 与 `clients/web/package-lock.json` 确认未引入未经审查的新增外部依赖，记录实际执行的测试套件数量。
- 预期结果：`npm run check`、`npm run build:rust` 与 `git diff --check` 全部退出码 0 通过，Web 与 Rust 实跑用例数量大于 0；`.github/workflows/ci.yml` 已持久包含 `--all-features --all-targets --locked -- -D warnings` 的 Clippy 强阻断门禁；锁文件保持一致且无计划外第三方依赖引入。

### FBL-005 `Storage` 内部表示私有化、`Deref` 移除与废弃存储访问器清理验证

- 用例级别：L2
- 操作步骤：1) 检视 `app/src/storage.rs` 及 `app/src/storage/` 下子模块源码，核实 `#[doc(hidden)] pub struct StorageData`、`impl std::ops::Deref for Storage` 以及 `Storage::approvals`、`Storage::histories`、`Storage::sessions` 三个方法是否已彻底删除，队列与准备阶段运行时关闭状态是否已收拢至专用内部协调结构；2) 使用 `cargo doc --manifest-path app/Cargo.toml --no-deps` 检查 `acp_remote::storage` 公开文档符号面，并通过外部集成测试视角验证仅保留 `Storage::open`、`Storage::open_owned`、`load_node_meta`、`write_node_meta`、`list_revision`、`request_registry`、`native_configuration_provider`、`start_queue_service*`、`close` 等受控 API，无法再对 `Storage` 实例直接解引用访问 `pool`、`queue_supervisor`、`queue_closed` 或 Windows `*_shutdown` 槽位。
- 预期结果：`StorageData` 与 `impl Deref for Storage` 从公开及隐藏导出面彻底消失，`Storage::{approvals, histories, sessions}` 在生产与 `#[cfg(test)]` 下均零残留；外部模块无法绕过 `Storage` 方法直接读写 SQLite 连接池或运行时监督字段，持久化职责与运行时协调职责边界清晰分离。

### FBL-006 节点启动、队列运行时单次认领与监督先于连接池关闭收敛回归验证

- 用例级别：L5
- 操作步骤：1) 在隔离临时目录创建独占 SQLite 数据库路径与节点运行目录，运行 `cargo test --manifest-path app/Cargo.toml --lib storage:: --locked`（含 `pool_close_tests`）以及全套注册表与关闭所有权集成验收测试 `cargo test --manifest-path app/Cargo.toml --features registration-acceptance-fixtures --test registration_shutdown_ownership --test registration_owner_acceptance --test registration_reconcile_ownership --test registration_restart --test registration_factory_waiter --locked`；2) 验证同一 `Storage` 作用域内二次调用 `start_queue_service*` 的单次所有权认领互斥保护，以及存在在途请求登记/准备任务时调用 `Storage::close()`、`close_after_users()` 与 `Drop` 的完整关闭时序；3) 启动真实节点最小闭环（初始化 `v15` 库、写入/读取节点身份元数据、建立请求登记服务并执行优雅关闭），结束后清理临时目录与全部受管进程。
- 预期结果：`storage` 单元测试、5 个 `registration_*` 集成验收套件与节点启动/关闭回归全部通过（实跑测试数非零）；同一 `Storage` 重复启动队列运行时被确定性拒绝；关闭时严格遵循“先置关闭栅栏并调用 `RuntimeSupervisor::cancel()` 与 `stop_and_join()` 完成监督任务收敛 -> 等待作用域用户在途操作结束 -> `seal_connections()` 与 `close_connections()` -> `final_sql_fence()` -> 物理关闭 `SqlitePool` 至连接数为 0”的不变量，无死锁、无提前断池导致的 SQL 账本写入失败、无孤儿任务或句柄泄漏。

### FBL-007 全新 SQLite 数据库（`v0 -> v15`）初始化、现役目录父表保留与死表缺席验证

- 用例级别：L3
- 操作步骤：1) 在系统临时目录生成不存在的新数据库路径，调用 `Storage::open` 完成 `v0` 全新初始化，写入 `NodeMeta { schema_version: SCHEMA_VERSION, ... }` 后通过 `load_node_meta` 读回并关闭 `Storage`；2) 使用独立只读 SQLite 连接查询该数据库文件的 `PRAGMA main.user_version`、`SELECT schema_version FROM main.node_meta`、`SELECT type, name, tbl_name FROM main.sqlite_schema`、`temp.sqlite_schema`、`PRAGMA main.foreign_key_check` 以及 `node_catalog_schema`、`node_management_audit`、`node_approval_audit` 相关表的 `pragma_foreign_key_list`；3) 逐一核对现役表集合与 15 张退役死表（`history_content_pins`、`history_integrity`、`session_state`、`approval_audit_links`、`approval_deliveries`、`approval_options`、`approvals`、`approval_run_sources`、`approval_connections`、`conversation_items`、`runs`、`turns`、`session_commands`、`content_objects`、`sessions`）及 3 个退役唯一索引（`tool_reference_key`、`tool_native_identity`、`one_foreground_run`），最后关闭连接并删除临时数据库文件。
- 预期结果：`crate::storage::SCHEMA_VERSION` 等于 `15`，新库 `PRAGMA user_version = 15` 且 `node_meta.schema_version = 15`；`harnesses`、`workspaces` 两张基础目录父表及 `product_sources`、`product_harness_sources`、`product_targets`、`node_catalog_*`、`node_management_audit`、`node_approval_*`、`node_request_*`、`node_native_*`、`node_source_sessions*`、`node_session_capacity*`、`node_session_trees*` 等全部现役表、索引和外键关系 100% 完整存在，`PRAGMA foreign_key_check` 零违例；15 张第一代 `SESSION` 死表与 3 个旧索引在新库中从未创建、`sqlite_schema` 匹配计数严格为 0。

### FBL-008 既有 `v2..=v14` 数据库（含双向外键历史行与 `v2/v3/v4` 迁移链）原子升级至 `v15` 验证

- 用例级别：L4
- 操作步骤：1) 在隔离临时目录按 `v14` 真实历史 DDL 创建 SQLite 数据库（开启 `PRAGMA foreign_keys = ON`），写入合法的 `node_meta(schema_version = 14)`、`clients`、`harnesses`、`workspaces`、`product_sources`、`product_harness_sources`、`product_targets`、`node_management_audit` 等现役业务行，同时向 `sessions`、`content_objects`、`session_commands`、`turns`、`runs`、`conversation_items`、`approval_connections`、`approval_run_sources`、`approvals`、`approval_options`、`approval_deliveries`、`approval_audit_links`、`session_state`、`history_integrity`、`history_content_pins` 全部 15 张旧表写入非空关联行（显式构造 `approvals.selected_choice_id = choice_id` 与 `approval_options.approval_id = approval_id` 双向外键互指记录）；2) 调用 `Storage::open` 执行 `v14 -> v15` 升级，并运行覆盖 `v1`、`v2`、`v3`、`v4` 至最新版本的历史迁移测试套件（含 `schema3_migration`、`schema4_migration`、`independent_schema5` 及 `session_storage` 迁移单测）；3) 使用故障注入检查点在 `v15` 迁移事务提交前触发中断，验证回滚后原 `v14` 库状态并再次重开完成升级，最后清理全部临时文件。
- 预期结果：含 `approvals <-> approval_options` 双向外键历史行的 `v14` 库及 `v1..=v13` 各历史版本库均在 `PRAGMA foreign_keys = ON` 下一次性平滑升级至 `v15`（`PRAGMA user_version = 15`、`node_meta.schema_version = 15`），无外键约束报错；升级后 15 张第一代死表与 3 个旧索引在 `sqlite_schema` 中全部消失，`PRAGMA main.foreign_key_check` 返回空集，而 `harnesses`、`workspaces`、`clients`、`product_targets` 与审计记录等现役表数据 100% 完整无损；提交前注入故障时 `BEGIN IMMEDIATE` 事务原子回滚至升级前一致版本，不产生半清理损坏库，重试打开后可正常升级至 `v15`。

### FBL-009 SQLite 高于 `v15` 未知版本安全拒绝与 `v15` 残留死表/非法篡改预检拦截验证

- 用例级别：L3
- 操作步骤：1) 在隔离临时目录创建 `v15` 数据库后关闭，将其 `PRAGMA user_version` 分别设为 `16` 与 `77` 并写入哨兵表 `sentinel(value='preserve-me')`，调用 `Storage::open` / `preflight` 观察错误类型并检查原库数据是否被改写；2) 分别构造四组 `user_version = 15`（且 `node_meta.schema_version = 15`）的结构异常临时库：组 A 在 `main` 模式残留任一已退役死表（如 `sessions` 或 `approvals`）、组 B 在 `temp` 模式或 `main` 模式残留退役唯一索引（如 `tool_reference_key` 或同名阴影视图）、组 C 在 `harnesses` 或 `workspaces` 上挂载未授权触发器、组 D 删除现役父表 `harnesses` 或 `workspaces`，逐组调用 `Storage::open` 验证预检拦截；3) 测试完成后删除全部临时数据库文件。
- 预期结果：针对 `user_version = 16` 与 `77` 的数据库，`Storage::open` 在只读预检阶段精确返回 `StorageError::UnknownSchema(16)` / `StorageError::UnknownSchema(77)`，原库文件与哨兵表数据零改写（同时确认旧 `v14` 节点面对 `v15` 库时的安全拒绝语义一致）；组 A 至组 D 四类篡改或死表残留的 `v15` 库均被 `validate` 精确识别并返回 `StorageError::UnrecognizedSchema` 拒绝启动，防止带病或半迁移库进入生产读写路径。

### FBL-010 第一代 `legacy`、`history`、`approvals` 废弃模块移除与全库全特性回归验证

- 用例级别：L2
- 操作步骤：1) 检查仓库工作树确认 `app/src/domain/history.rs`、`app/src/domain/approvals.rs`、`app/src/application/history.rs`、`app/src/application/approvals.rs`、`app/src/session_storage/legacy.rs`、`app/src/session_storage/history.rs`、`app/src/session_storage/approvals.rs` 共 7 个废弃文件已从磁盘与 Git 索引中移除；2) 在 `app/src/` 与 `app/tests/` 全范围检索 `InputRepository`、`InputService`、`session_storage::legacy`、`session_storage::history`、`session_storage::approvals` 等第一代聊天正文遗留符号，同时核对 `EntityId`、`ActorContext` 及现役第二代 `approval_owner` / `session_tree` / `source_sessions` / `session_capacity` 模块完整保留；3) 在独立临时测试环境下执行 `cargo test --manifest-path app/Cargo.toml --all-targets --all-features --locked` 全量测试回归并清理临时产物。
- 预期结果：7 个第一代废弃模块文件及其在 `domain/mod.rs`、`application/mod.rs`、`session_storage/mod.rs` 中的模块挂载、`InputRepository` / `InputService` 抽象均彻底清零；现役控制面存储、目录、配对、请求队列、第二代严格审批所有者及会话容量模块无任何误删或行为回归，全特性全目标自动化测试 100% 通过。
