# reviewer 报告 · RV1（branch：WP1+WP2 工作包分支验证，任务 3.2）

- task_id: `3.2`
- role: reviewer
- phase: review
- stage: work-package
- agent_context: 新建只读独立子 Agent，未参与实现与实现讨论，不继承 coder/主 Agent 上下文；本轮只读检视，未修改任何代码、测试、规划或任务状态。
- target_revision: `ac74102897f8d7d9b0830b3de3c7e958512b5de2`（分支 `test/agent-host-spawn-failure-coverage` HEAD）
- base_revision: `f53dad55e72111bb7e026a0d13f5180f82c61358`（main）
- scope: WP1（`crates/agent-host/tests/catalog.rs` 新增集成测试）+ WP2（`crates/agent-host/src/error.rs` 追加 `#[cfg(test)] mod tests`）+ 增量 spec S1
- result: **PASS**（无未解决 CRITICAL/MAJOR）

## Review Context

- Review ID: RV1；Review Type: branch；Review Stage: 工作包分支验证（Branch Validation）；Work Package: WP1 + WP2。
- Repository: `D:\Project\acp-remote`（只读；未切换分支、未写文件）。
- 读取的规则与需求：proposal.md（Intent and Constraints）、specs/local-agent-host/spec.md（MODIFIED 需求与 S1）、design.md（决策 1–4）、plan.md（Code Review 关注点、Verification Strategy）、tasks.md（2.1/2.2 完成条件）、AGENTS.md（§3/§7/§9）。
- 实际检查范围：
  - `crates/agent-host/tests/catalog.rs` 全文（重点 `spawn_failure_is_explicit_unavailable_without_side_effects` 及其与既有测试的分工）；
  - `crates/agent-host/src/error.rs` 全文（映射表逐行 vs 测试表逐行比对）；
  - 被测行为路径：`host.rs` `create`/`open`/`ensure_runtime`（:206-258、:448-527）、`launch.rs:64-71`、`process.rs:129`、`core/src/model/error.rs`（`PortError`/`UnavailableKind`/`ConflictKind` 定义）、`acp-protocol/src/error.rs`（`AcpError::Json` 变体存在性）；
  - 验证证据：`reports/LC1.log`、`LC2.log`、`LC3.log`、`WP-verify-agent-host-all.log`（经 coder 报告引用）、`falsifiability-probe.log`、`PV1.log`、`verification.md`。
- 版本核实方法与限制：本环境无 git 读取能力，以「工作区 ≡ reviewer-launch HEAD」间接核实——`watchdog_diff` 显示工作区相对 HEAD 仅有 `openspec/config.yaml` 未提交改动（角色模型配置，非本变更范围，verification.md 已声明全程不提交）与未跟踪的本变更规划/报告文件；`catalog.rs`/`error.rs` 与 HEAD 一致。coder 报告与 verification.md 均记录日志采集时 HEAD = ac74102。两文件的 base 内容无法独立 diff，采用交叉佐证：error.rs 产品部分与 proposal 假设引用（error.rs:126、launch.rs:68）逐字吻合，coder numstat 声称纯追加（172+/0-），未发现任何矛盾。
- 未验证内容：Linux CI 实跑（静态判断，见关注点 5）；`git diff base..target` 的独立逐字节核对（限制如上）。

## Findings

| ID | Severity | Location | Issue | Evidence | Smallest Fix |
| --- | --- | --- | --- | --- | --- |
| RV1-F1 | MINOR | `plan.md` Work Packages 表 WP2 行与 Code Review 关注点 | 两处仍写「`to_port_error()` 16 变体」；实际 `HostError` 有 19 个变体，`tasks.md` 2.2 与 `design.md` 决策 2 已同步为 19，仅 plan.md 漏同步。不影响行为与检视结论（单测覆盖全部 19 个），但 plan 与其他契约文件数字不一致。 | `crates/agent-host/src/error.rs` 19 个变体逐一计数 vs `plan.md` 两处「16」；verification.md「Check Plan Changes」只记录了 tasks/design 的同步 | 主 Agent 把 plan.md 两处「16」同步为「全部变体（当前 19 个，以代码为准）」 |
| RV1-F2 | SUGGESTION | `crates/agent-host/src/error.rs:154` `mod tests` | 变体覆盖靠「新增变体时人工补一行」的注释约束，无编译期强制：未来加第 20 个变体时测试不会变红。现有标签不重复断言只能防表内重复，防不了漏行。 | 测试数组 `[…; 19]` 大小固定但与 `HostError` 变体集无编译期关联 | 可选：增加一个对 `HostError` 全部变体无通配 `match` 的穷尽性护栏（如逐变体构造函数的 match），使新增变体直接编译错误 |

无 CRITICAL / MAJOR 发现。

## 关注点逐项核对（plan.md Code Review + 本轮输入）

1. **真实驱动 spawn 失败路径**：成立。测试直接调 `create()`（catalog.rs 本文件辅助 → `host.create`），不经 `agents()` 可用性预检；`host.rs:456-459` `create → ensure_runtime`，`host.rs:232-233` `Supervisor::start(spec).await?`，`process.rs:129` `command.spawn()` 失败收敛为 `HostError::SpawnFailed`。与预检测试 `availability_is_per_entry_and_credentials_fail_closed` 的分工写入测试文档注释。失败注入符合 design 决策 4（不存在程序名 `acpr-does-not-exist-anywhere`）。
2. **断言精确到 `UnavailableKind::IoError` 且覆盖 S1 三个可观察结果**：成立。① `assert_eq!(kind, UnavailableKind::IoError)` 精确匹配 kind（区别于 `InvalidRequest`/`KeystoreUnavailable`/`Busy`）；② 无副作用：`wait_until_not_running` + `runtime_generation == None` + `open()` 同会话报 `PortError::InvalidRequest(_)`；③ 重试不毒化：第二次 `create()` 仍是干净的 `Unavailable(IoError)`。与实现交叉验证：`ensure_runtime` 在 `Supervisor::start` 失败时不代增 generation、不登记 runtime（`host.rs:232-258`，generation 递增在 start 成功之后、insert 在 initialize 成功之后），`create` 在 `session/new` 成功前不写会话映射（`host.rs:462-490`），三个断言均钉在真实行为接缝上。
3. **`error.rs` 单测覆盖全部 19 变体、只钉类别/kind、标签不重复断言有效**：成立。`HostError` 19 个变体逐一计数并与测试表 19 行一一对应；每行期望值与 `to_port_error()` 实现逐行比对一致（含 `SpawnFailed → Unavailable(IoError)`、`Timeout → Unavailable(Busy)`、`CredentialUnavailable → Unavailable(KeystoreUnavailable)`、`DuplicateSession → Conflict(AlreadyExists)`、`UnknownInteraction → Conflict(AlreadyResolved)`）；`Shape` 归约只保留类别与 `ConflictKind`/`UnavailableKind`，丢弃消息文本（符合 design 决策 3）；`shape()` 对 `PortError` 六个变体无通配穷尽匹配；`BTreeSet` 标签去重断言有效；额外钉住 `From<HostError>` 与 `to_port_error()` 同源。`AcpError::Json { detail: String }` 变体确实存在（`acp-protocol/src/error.rs:20`），测试可编译性成立。
4. **无产品代码夹带**：成立（方法见 Review Context 的限制说明）。`error.rs` 的 `#[cfg(test)]` 模块之外代码与 proposal/design 引用的既有行为逐字吻合；`catalog.rs` 除新增测试与一行 `use` 增补（`UnavailableKind`）外均为既有测试；工作区相对 HEAD 无其他代码改动。
5. **Linux CI 可运行**：成立（静态判断）。失败注入仅用不存在程序名，Windows（CreateProcess 立即失败）与 Linux（fork/exec 错误管道回报 spawn Err）上均稳定快速失败；无 chmod/目录占用/权限类平台敏感手法；无 `cfg(windows)` 假设；`error.rs` 单测为纯函数表驱动。最终以 Linux CI 实跑为准（CI 属主 Agent/PR 门禁范围）。

## Assessment（按检查 ID）

| Check ID | 计划来源 | 核对结果 |
| --- | --- | --- |
| LC1（`cargo test … spawn_failure`） | plan.md Local Checks | 已核对：`reports/LC1.log` 命令与计划逐字一致、exit=0、目标测试 `ok`、21 filtered out（与既有测试数一致） |
| LC2（`cargo test … error::tests`） | plan.md Local Checks | 已核对（经 coder-wp2 报告与 PV1.log:220 交叉）：`to_port_error_is_pinned_per_variant ... ok` |
| LC3（fmt + clippy `-D warnings`） | plan.md Local Checks | 已核对：`reports/LC3.log` 两条命令 exit=0 |
| WP-verify（agent-host 全量） | coder 附加 | 已核对存在性；PV1.log 中 catalog 目标 22 passed（21 既有 + 1 新增）、lib 4 passed（含新单测），0 失败 |
| 假证探针 | coder 附加（非计划 ID） | 已核对：`falsifiability-probe.log` 显示改坏映射后两个新测试均 FAILED 且指名变体、探针已还原（`git diff --stat` 无输出）——两组测试均非恒真 |
| PV1（`npm run verify`，任务 3.1） | plan.md Project Verify | **已由主 Agent 完成并记录 PASS**（verification.md Checks 表，exit 0）。我抽查 `reports/PV1.log`：`npm run check` 全部合同门禁 OK（含 check:agentic 链路中 `✓ change/agent-host-spawn-failure-coverage`，openspec validate 13 passed / 0 failed）、fmt `Finished`、clippy 编译 agent-host 零警告、workspace 全量测试全部 `test result: ok` 含两个新测试。与本轮检视同一版本（含两处新增） |

待返回项：无（PV1 已在检视期间返回并核对）。

## changes（检视确认的差异构成）

- `crates/agent-host/tests/catalog.rs`：新增 `spawn_failure_is_explicit_unavailable_without_side_effects`（约 72 行含文档注释）+ `use` 增补 `UnavailableKind`；无产品代码。
- `crates/agent-host/src/error.rs`：文件末尾追加 `#[cfg(test)] mod tests`（约 172 行纯追加）；`HostError` 定义与 `to_port_error()` 实现未变。
- 增量 spec S1 与实现/测试断言一一对应，S2–S5 原文保留且由 PV1 全量回归确认未破。

## issues

见 Findings（RV1-F1 MINOR、RV1-F2 SUGGESTION，均不阻断）。

## evidence_paths

- 本报告：`openspec/changes/agent-host-spawn-failure-coverage/reports/review-rv1.md`
- 引用证据：`reports/LC1.log`、`reports/LC2.log`、`reports/LC3.log`、`reports/WP-verify-agent-host-all.log`、`reports/falsifiability-probe.log`、`reports/PV1.log`、`reports/coder-wp1.md`、`reports/coder-wp2.md`、`verification.md`

## resource_cleanup

- 本角色只读，未启动进程、未创建/修改/删除任何文件（报告文件由主 Agent 代为落盘）；无资源需清理。

```yaml
handoff_index:
  - task_id: "3.2"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "ac74102897f8d7d9b0830b3de3c7e958512b5de2"
    evidence_type: REVIEW
    evidence_id: RV1
    report_path: openspec/changes/agent-host-spawn-failure-coverage/reports/review-rv1.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "branch 检视固定 base f53dad5 / target ac74102；范围为 WP1+WP2 全部改动与增量 spec S1；五项 Code Review 关注点逐项核对成立；无未解决 CRITICAL/MAJOR；Findings：RV1-F1（MINOR，plan.md 变体计数漂移）、RV1-F2（SUGGESTION，穷尽性护栏）；PV1 已返回并抽查核对（PASS）"
    source_evidence: NOT_APPLICABLE
```
