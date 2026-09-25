# coder 报告 · WP1（S1：spawn 失败返回明确不可用且无副作用）

- task_id: `2.1`
- role: coder
- phase: implement
- stage: work-package
- agent_context: 单 Agent 串行实现（本仓库工作区 `test/agent-host-spawn-failure-coverage`，无并发写入者；WP1 先于 WP2 执行，由同一 coder 上下文连续完成）
- target_revision: `ea8762f1a71befbff9867fba8e139df9e7832e9d`（WP1 提交，父提交 = 固定起点 `f53dad55e72111bb7e026a0d13f5180f82c61358`）
- base_revision: `f53dad55e72111bb7e026a0d13f5180f82c61358`
- scope: 仅 `crates/agent-host/tests/catalog.rs`（新增一个集成测试 + 一行 `use` 追加）
- dependencies: 无代码依赖；契约 = 冻结的 `specs/local-agent-host/spec.md` S1 与 `design.md` 决策 1/3/4；测试支撑来自 `crates/agent-host/tests/support/mod.rs`（只读）
- result: `PASS`
- issues: 无阻断项。发现一处**文档计数漂移**（非本 WP 范围、不影响结论）：`tasks.md` 2.1/`design.md` 决策 1 描述的是 `create()` 的 spawn 失败路径，与实际实现一致；相关数字漂移在 WP2 报告中记录（见 `coder-wp2.md` 的 issues）。实际行为**与 S1 一致**，未触发 proposal `decision_bounds` 的缺陷上报路径。

## changes（需求 → 文件 → 断言映射）

| 需求/场景 | 文件 | 内容 |
| --- | --- | --- |
| S1「进程无法启动时返回明确不可用」 | `crates/agent-host/tests/catalog.rs:287`（新增 `spawn_failure_is_explicit_unavailable_without_side_effects`） | ① `create()` 必须返回 `Err(PortError::Unavailable(_))` 且 `kind == UnavailableKind::IoError`（把「不可用（IO）」与 `InvalidRequest`／`KeystoreUnavailable`／`Busy` 区分开）；② 无副作用：`wait_until_not_running` 成立、`runtime_generation` 为 `None`（目录里没有半启动 runtime）、对同一会话 `open()` 报 `PortError::InvalidRequest(_)`（没有残留会话映射）；③ 重试不毒化：紧接着第二次 `create()` 仍是干净的 `Unavailable(IoError)`，不报 `Conflict` 之类二次错误 |
| S1（构造方式，design 决策 4） | 同上 | 失败注入只用「程序名不存在」：`support::profile_with("agent-missing", "acpr-does-not-exist-anywhere", &["--scenario","normal"])`（与 `catalog.rs:228` 的 `agent-missing` 先例同形），不使用 chmod/目录类平台敏感手法 |
| S1（绕过目录预检，design 决策 1） | 同上 | 直接调用本文件 `create()` 辅助（`host.create(...)`），不经 `agents()` 预检；与 `availability_is_per_entry_and_credentials_fail_closed` 的分工写进测试文档注释 |
| 断言粒度（design 决策 3） | 同上 | 只匹配 `PortError` 类别与 `UnavailableKind`，不断言诊断消息文本 |

diff：`72 insertions(+), 1 deletion(-)`；唯一删除来自 `use acp_core::model::{...}` 增补 `UnavailableKind` 的同一行改写。**未夹带任何产品代码改动**（`src/` 未出现在本提交）。

## checks

| Check ID | 命令（cwd = `D:\Project\acp-remote`） | 退出码 | 日志 | 结论 |
| --- | --- | --- | --- | --- |
| LC1 | `cargo test --locked -p agent-host --all-features spawn_failure` | 0 | `reports/LC1.log` | PASS：`test spawn_failure_is_explicit_unavailable_without_side_effects ... ok`；其余 4 个测试目标 0 匹配、0 失败（日志采集于 HEAD = `ac74102`，但 `tests/catalog.rs` 自 `ea8762f` 起未被后续提交/格式化改动，故逐字节适用于 WP1 提交） |
| LC3 | `cargo fmt --all -- --check` + `cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings` | 0 / 0 | `reports/LC3.log` | PASS：fmt 无差异；clippy 零警告（`-D warnings` 下 `Finished`） |
| WP 级验证（agent-host 全量，与 WP2 共用） | `cargo test --locked -p agent-host --all-features` | 0 | `reports/WP-verify-agent-host-all.log` | PASS：4 + 22 + 13 + 15 + 1 全通过、0 失败（含既有 S2–S5 回归） |
| 假证探针（本角色附加证据，非计划内检查 ID） | 见 `reports/falsifiability-probe.log` | 探针态 101 / 还原后 0 | `reports/falsifiability-probe.log` | 把 `SpawnFailed` 映射临时改成 `InvalidRequest` 后本测试 FAILED（`spawn 失败必须是不可用类错误，实际是 InvalidRequest("spawn failed")`），证明断言非恒真；探针改动已还原、未提交 |

环境/工具链（`reports/environment.log`）：rustc/cargo `1.98.1`（`rust-toolchain.toml` 固定）、node `v24.19.0`、Windows 本机。

```yaml
handoff_index:
  - task_id: "2.1"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "ea8762f1a71befbff9867fba8e139df9e7832e9d"
    evidence_type: CHECK
    evidence_id: LC1
    report_path: openspec/changes/agent-host-spawn-failure-coverage/reports/coder-wp1.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "针对 ea8762f（WP1 提交）；日志采集时 HEAD = ac74102，而 ac74102 只改 src/error.rs，tests/catalog.rs 与 ea8762f 内容逐字节相同（候选/主分支复用该行时只需核对 catalog.rs 未变）；命令与 plan.md 的 LC1 逐字一致，工具链 1.98.1、Windows 本机、无产品代码改动"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.1"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "ea8762f1a71befbff9867fba8e139df9e7832e9d"
    evidence_type: CHECK
    evidence_id: LC3
    report_path: openspec/changes/agent-host-spawn-failure-coverage/reports/coder-wp1.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "fmt/clippy 在 WP1、WP2 两个提交完成后、工作区与 HEAD = ac74102 一致时执行；clippy 覆盖 -p agent-host --all-targets --all-features（含新增集成测试）；日志 reports/LC3.log"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.1"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "ea8762f1a71befbff9867fba8e139df9e7832e9d"
    evidence_type: VALIDATION
    evidence_id: NOT_APPLICABLE
    report_path: openspec/changes/agent-host-spawn-failure-coverage/reports/coder-wp1.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "假证探针：临时改坏 SpawnFailed 映射后新增测试 FAILED、还原后工作区干净（git status 仅剩用户既有的 openspec/config.yaml 改动与未跟踪 change 目录）；日志 reports/falsifiability-probe.log"
    source_evidence: NOT_APPLICABLE
```

## resource_cleanup

- 无独占/共享运行资源；未启动任何额外进程（测试用 profile 指向不存在的程序，spawn 必失败，无子进程残留）。
- 测试自身未创建临时文件（本用例不写 dump/heartbeat 文件），无需清理。
- 探针用的临时备份 `/tmp/error.rs.bak` 已删除；工作区核实干净（`git status --porcelain` 仅 ` M openspec/config.yaml` 与未跟踪的 change 目录，二者均为用户/主 Agent 资产，未触碰）。

## 未执行项 / 待补

- 独立 review（3.2 / 5.4）与 PV1（任务 3.1，`npm run verify`）由主 Agent 调度，**本报告不代表其结论**。
- 未 push、未合入、未创建 PR（无该授权）。
