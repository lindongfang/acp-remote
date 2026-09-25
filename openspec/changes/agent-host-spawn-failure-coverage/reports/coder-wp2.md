# coder 报告 · WP2（R1「可区分原因」：`HostError → PortError` 映射逐变体钉死）

- task_id: `2.2`
- role: coder
- phase: implement
- stage: work-package
- agent_context: 单 Agent 串行实现（同一工作区 `test/agent-host-spawn-failure-coverage`，同一 coder 上下文；WP1 已交付后执行，写入文件与 WP1 不重叠）
- target_revision: `ac74102897f8d7d9b0830b3de3c7e958512b5de2`（WP2 提交，父提交 = WP1 提交 `ea8762f1a71befbff9867fba8e139df9e7832e9d`）
- base_revision: `f53dad55e72111bb7e026a0d13f5180f82c61358`（本变更固定起点）
- scope: 仅 `crates/agent-host/src/error.rs`，**只追加**文件末尾的 `#[cfg(test)] mod tests`（`git show --numstat` = `172 0`，0 删除）
- dependencies: 无代码依赖；契约 = `design.md` 决策 2/3、`error.rs` 现有映射表（只读引用）
- result: `PASS`
- issues: **一处契约文档计数漂移（需主 Agent 同步，不阻断本 WP）**：`tasks.md` 2.2 与 `design.md` 决策 2 写「`HostError` 全部 **16** 个变体」，实际 `crates/agent-host/src/error.rs` 的 `HostError` 有 **19** 个变体（`UnknownProfile`、`NotRunning`、`SpawnFailed`、`Timeout`、`AgentExited`、`Protocol`、`AgentRejected`、`CapabilityNotDeclared`、`DuplicateSession`、`UnknownSession`、`SessionClosed`、`UnknownInteraction`、`InvalidEnvName`、`EnvNotAllowed`、`CredentialUnavailable`、`IdUnavailable`、`InvalidResolution`、`InvalidPrompt`、`ShutdownPending`）。本 WP 按「全部变体」的意图覆盖**全部 19 个**（严格包含那 16 个），并在测试注释里写明该差异；未改任何产品代码，故不涉及行为分歧，不由本角色修改计划/tasks/design。

## changes（需求 → 文件 → 断言映射）

| 需求 | 文件 | 内容 |
| --- | --- | --- |
| R1「可区分原因」（`to_port_error` 映射是 adapter 侧收敛点） | `crates/agent-host/src/error.rs:154`（新增 `#[cfg(test)] mod tests`） | 表驱动 `to_port_error_is_pinned_per_variant`：19 行 `(变体标签, HostError 实例, 期望 Shape)`，逐行断言 `to_port_error()` 的 `PortError` 类别；`ConflictKind`/`UnavailableKind` 精确匹配 |
| 含 `SpawnFailed → Unavailable(IoError)` | 同上（表中 `SpawnFailed` 行） | `SpawnFailed → Shape::Unavailable(UnavailableKind::IoError)`，与 S1 场景锚定的一致 |
| 只钉类别/kind，不钉消息文本（design 决策 3） | 同上 | 局部 `enum Shape { NotFound / Conflict(ConflictKind) / InvalidRequest / Unavailable(UnavailableKind) / Corrupt / Backend }` + `fn shape(&PortError)`：把 `PortError` 归约成类别 + kind，丢弃 `&'static str` 诊断文案；断言全部基于 `Shape`，改文案不会破坏测试 |
| 防止表行重复（把「逐变体覆盖」悄悄退化成少测一行） | 同上 | 断言 `cases` 的变体标签集合大小 == 行数（`BTreeSet` 去重后相等） |
| `From<HostError> for PortError` 不得与 `to_port_error()` 各写一份 | 同上 | 每行额外断言 `PortError::from(error)` 的形状与 `to_port_error()` 相同（`From` 实现即调用 `to_port_error()`，此断言钉住该同源关系） |
| 不改动映射实现（本变更禁令） | 同上 | 提交 `ac74102` 对 `error.rs` 是 172 行纯追加、0 删除；`HostError` 定义与 `to_port_error` 实现逐字未变 |

## checks

| Check ID | 命令（cwd = `D:\Project\acp-remote`） | 退出码 | 日志 | 结论 |
| --- | --- | --- | --- | --- |
| LC2 | `cargo test --locked -p agent-host --all-features error::tests` | 0 | `reports/LC2.log` | PASS：`test error::tests::to_port_error_is_pinned_per_variant ... ok`（1 passed / 0 failed） |
| LC3 | `cargo fmt --all -- --check` + `cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings` | 0 / 0 | `reports/LC3.log` | PASS：fmt 无差异（`cargo fmt --all` 已按 rustfmt 结果整理表行缩进）；clippy 零警告 |
| WP 级验证（agent-host 全量，与 WP1 共用） | `cargo test --locked -p agent-host --all-features` | 0 | `reports/WP-verify-agent-host-all.log` | PASS：4（lib 单测，含本 WP 新增）+ 22 + 13 + 15 + 1 全通过、0 失败 |
| 假证探针（本角色附加证据，非计划内检查 ID） | 见 `reports/falsifiability-probe.log` | 探针态 101 / 还原后 0 | `reports/falsifiability-probe.log` | 临时把 `SpawnFailed` 改成 `InvalidRequest("spawn failed")` 后：`变体 SpawnFailed 的端口错误映射与固定承诺不符` FAILED；还原（`cp` 备份）后 `git diff --stat crates/agent-host/src/error.rs` 无输出、`git status` 干净，探针未提交 |

环境/工具链（`reports/environment.log`）：rustc/cargo `1.98.1`（`rust-toolchain.toml`）、node `v24.19.0`、Windows 本机。

```yaml
handoff_index:
  - task_id: "2.2"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "ac74102897f8d7d9b0830b3de3c7e958512b5de2"
    evidence_type: CHECK
    evidence_id: LC2
    report_path: openspec/changes/agent-host-spawn-failure-coverage/reports/coder-wp2.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "针对 ac74102（WP2 提交）；日志采集时 HEAD == 工作区 == ac74102；命令与 plan.md 的 LC2 逐字一致；工具链 1.98.1、Windows 本机；日志 reports/LC2.log"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.2"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "ac74102897f8d7d9b0830b3de3c7e958512b5de2"
    evidence_type: CHECK
    evidence_id: LC3
    report_path: openspec/changes/agent-host-spawn-failure-coverage/reports/coder-wp2.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "fmt/clippy 在 WP1、WP2 两个提交完成后、工作区与 HEAD == ac74102 一致时执行；clippy 使用 --all-targets --all-features 并覆盖 src 内新增单测模块；日志 reports/LC3.log"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.2"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "ac74102897f8d7d9b0830b3de3c7e958512b5de2"
    evidence_type: VALIDATION
    evidence_id: NOT_APPLICABLE
    report_path: openspec/changes/agent-host-spawn-failure-coverage/reports/coder-wp2.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "假证探针：改坏 SpawnFailed 映射后本单测 FAILED 并指名变体，还原后工作区干净；日志 reports/falsifiability-probe.log"
    source_evidence: NOT_APPLICABLE
```

## resource_cleanup

- 无独占/共享运行资源；未启动子进程。
- 探针备份 `/tmp/error.rs.bak` 已删除；工作区核实干净（`git status --porcelain` 仅 ` M openspec/config.yaml`（用户既有改动，未 stage/未还原）与未跟踪的 `openspec/changes/agent-host-spawn-failure-coverage/`）。
- `git diff --cached` 为空：两个 WP 提交后没有残留暂存内容。

## 未执行项 / 待补

- 独立 review（3.2 / 5.4）与 PV1（任务 3.1，`npm run verify`）由主 Agent 调度，**本报告不代表其结论**。
- 未修改 `tasks.md`/`design.md`/`plan.md`：其中的「16 个变体」计数漂移需主 Agent 裁定是否同步为 19。
- 未 push、未合入、未创建 PR（无该授权）。
