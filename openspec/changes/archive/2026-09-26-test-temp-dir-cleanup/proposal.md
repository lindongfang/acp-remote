<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

测试在系统临时目录留下不自清理的 `acpr-*` 目录：2026-09-25 实测本机 `C:\Users\zhang\AppData\Local\Temp`
下积累 14,557 个（占该目录总条目 15,261 的 95%）。主犯是 `storage-sqlite` 测试助手的
`temp_dir(name)`（裸 `PathBuf` 返回、无清理，约 100 个调用点），另有 `identity-keystore` 单元测试、
`app::cli::input` 测试、若干 `agent-host` 测试的散落创建点。每次 `cargo test --workspace` 都会再长一批，
长期累积拖慢临时目录枚举并污染排查现场；切片 4 的 CI 排障期间「哪些目录是本次产生的」已无法回答。

## What Changes

- 让**每个测试自建临时目录/文件**都经「Drop 守卫」自清理（panic 展开路径同样生效），逐 crate 盘点并修复：
 - `crates/storage-sqlite/tests/support/mod.rs` 的 `temp_dir()` 改为返回带 `Drop` 的守卫（约 100 个调用点，
 用 `Deref<Target = Path>` 尽量零改动）；
 - `crates/identity-keystore/src/store.rs` 单元测试（`acpr-keystore-modes-*`/`acpr-keystore-atomic-*`）；
 - `crates/app/src/cli/input.rs` 测试（`acpr-wp4b-input-*`）；
 - `crates/agent-host` 测试的 `acpr-agent-host-*.txt` 文件（catalog/supervision，逐个核实）；
 - `crates/core/src/use_cases.rs` 测试（`acpr-ws-*`，逐个核实是否真实创建）；
 - `crates/server` 测试（`audit.rs`/`params.rs` 的散落创建点，逐个核实）。
- 已有自清理的助手（`app` 的 `TempRoot`/`TempDir`、`identity-keystore` 测试 `TempRoot`、`server` 的
 `TestWorld::temporary_directory`、`app::lock` 测试 `TempDir`）保持不动，只核对无遗漏。
- 验收按「跑一次完整 `cargo test --locked --workspace --all-features`，系统临时目录的 `acpr-*`
 条目数不增加」判定。

不包含：产品行为变更（无）；CI 新增门禁（不在本次引入）；历史存量清理（已于 2026-09-25 手工删除，
无代码路径再产生）；一次性探针目录（`acpr-job-probe`/`acpr-spike` 等，`grep` 确认当前代码无来源）。

## Capabilities

<!-- 先检查现有规范，区分新增能力与已有能力的需求变化。
 仅当规范层面的行为不变时（如不改变行为的纯重构、工具或文档变更），
 在该变更的 .openspec.yaml 中设置 skip_specs: true，说明理由并引用既有行为契约。
 此时两类能力清单均无变更，删除占位项；否则至少声明一项能力并生成对应增量规范。
 不得为通过校验而编造需求。 -->

本变更为**纯测试基础设施修复**：产品行为、协议 wire、端口合同、存储 DDL 均不变，
16 个既有能力规范的每一条 Requirement/Scenario 的语义保持原样（测试仍验证同样的行为，
只是临时资源改为自清理）。按 变更流程 schema 的规则，在该变更的 `.openspec.yaml` 中设置
`skip_specs: true`，不生成增量规范文件。

### New Capabilities

无（产品行为不变，见上方 skip_specs 说明）。

### Modified Capabilities

无（同上；既有规范的验证语义不因此改变）。

## Impact

- **代码**：仅 `crates/*/tests/**` 与 `crates/*/src/**` 的 `#[cfg(test)]` 模块；预期修改
 `storage-sqlite/tests/support/mod.rs`（主）、`identity-keystore/src/store.rs`（测试模块）、
 `app/src/cli/input.rs`（测试模块）、`agent-host/tests/{catalog,supervision}.rs`、
 `core/src/use_cases.rs`（测试模块）、`server/src/local_admin/{audit,params}.rs`（测试模块）中的若干点。
- **API/ABI**：无（全部在测试边界内）。
- **依赖**：不新增任何依赖（守卫用 std 即可）。
- **文档/合同资产**：无；`npm run check` 的判定面不变。
- **CI**：判定面不变（不新增/不修改 job）；改动后 CI 的 `checks` job 应照常绿。
