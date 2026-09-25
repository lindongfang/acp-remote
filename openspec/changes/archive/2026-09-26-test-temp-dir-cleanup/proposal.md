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

## Intent and Constraints

```agentic-intent
sources:
  - "2026-09-25 会话：用户指示『先删除/tmp 下约 1.4 万个历史遗留 acpr-* 测试目录』；存量删除完成后，用户选择继续 propose『测试临时目录清理』（即根治：让测试不再泄漏临时目录）"
constraints:
  - "只改测试代码（tests/ 与 #[cfg(test)] 模块）；不触碰产品代码路径的行为"
  - "不得引入共享 utils/common crate（AGENTS.md §4）：每个 crate 的测试助手各自落地守卫"
  - "正常工作路径不得 unwrap/expect 新增于产品代码；测试代码沿用各 crate 现有风格"
  - "守卫必须 panic 安全（panic 展开时 Drop 仍执行）；进程被强杀（CI 取消等）无法清理属于平台事实，如实说明"
  - "命名保持每进程唯一（现有 {pid}/{sequence}/线程号 约定），并行测试互不误删"
non_goals:
  - "不新增 CI 门禁或 lint 规则（可作为后续变更候选）"
  - "不修改 openspec/specs/ 的任何产品能力规范（产品行为不变）"
  - "不清理 `/tmp` 存量（已完成）"
  - "不统一各 crate 的测试助手形态，只补齐自清理"
success_criteria:
  - "完整 `cargo test --locked --workspace --all-features` 运行前后，系统临时目录的 `acpr-*` 条目数不增加（本机 Windows 实测）"
  - "上述运行全部通过（不因守卫改动引入新的测试失败）"
  - "`npm run verify` 全绿"
decision_bounds:
  - "守卫的具体实现形态（Drop + Deref 的具体写法、命名）由 Agent 自主决定"
  - "逐个创建点是否「核实后判定无泄漏而保持不动」由 Agent 自主决定，但必须在 tasks 证据里给出结论"
  - "新增 CI 门禁、改动产品代码、改变测试语义（而非资源管理）必须回到用户决策"
assumptions:
  - "历史遗留目录名 `acpr-backups`/`acpr-integration`/`acpr-core-name-probe`/`acpr-job-probe`/`acpr-spike` 在当前代码中无来源（grep 验证：仅 docs/INITIAL_DESIGN.md 的探针叙述与 identity-keystore 代码命中部分前缀），属一次性探针残留，已由 2026-09-25 的手工删除处理"
  - "测试二进制的并发模型是『进程内多线程 + 每 crate 一个测试进程』，现有命名唯一性足以防止并行误删；若盘点中发现共享路径，须在 design 中处理"
```

## Capabilities

<!-- 先检查现有规范，区分新增能力与已有能力的需求变化。
     仅当规范层面的行为不变时（如不改变行为的纯重构、工具或文档变更），
     在该变更的 .openspec.yaml 中设置 skip_specs: true，说明理由并引用既有行为契约。
     此时两类能力清单均无变更，删除占位项；否则至少声明一项能力并生成对应增量规范。
     不得为通过校验而编造需求。 -->

本变更为**纯测试基础设施修复**：产品行为、协议 wire、端口合同、存储 DDL 均不变，
16 个既有能力规范的每一条 Requirement/Scenario 的语义保持原样（测试仍验证同样的行为，
只是临时资源改为自清理）。按 agentic schema 的规则，在该变更的 `.openspec.yaml` 中设置
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
