# TP2 测试报告（tester-A / Task 2.8 / Work Package TP2）

## Shared Report

- **task_id**: `2.8`
- **role**: tester
- **phase**: design-author（TP2 同时交付修订设计与可执行用例；plan.md 的 TP2 行 Verification = PV1 阶段 1 口径）
- **agent_context**: 子 Agent `tester-A`（TP2 实例），`fork_turns="none"` 隔离，不继承任何实现对话；
  规划根 = `D:/Project/acp-remote/openspec/changes/session-resume`（未跟踪目录，从主仓库**绝对路径**读写记录）；
  代码 worktree = `D:/Project/acp-remote-wt/session-resume-tp1`，分支 `agentic/session-resume-tp1`
- **target_revision**: `a02e2fd`（TP2 交付提交；base = `95051f9`，U1 集成基线）
- **scope**: ① 按 CR7 及并入的 CR3/CR4/CR5/CR6 意见**修订** TP1 的测试设计；② 在含全部实现的基线上
  编写**可编译、可运行**的用例，覆盖 plan.md `## Coverage Index` 的 R1–R37；③ 执行 PV1 阶段 1 口径的
  基础检查（编译、用例发现、辅助函数单测）。**不写产品代码**（`crates/**/src/**` 零改动）。
- **changes**: 5 个**新增**测试文件 + 1 个**共享测试支撑文件**的追加（TP2 是该目录的 Merge Owner，
  登记见下）；无产品代码改动，无 `schemas/`/`fixtures/`/`compatibility/`/文档/`plan.md`/`tasks.md`/
  `verification.md`/`design.md`/`specs/**` 改动。
- **checks**: PV1 阶段 1（六个 crate 子集）全绿，8 条命令全部退出码 0，逐条见 §3。
- **issues**: 1 项**待主 Agent 裁决的覆盖缺口**（R25 的「规范化结果变化」变体，§4-A1）、1 项**刻意不写**
  的 `#[cfg(unix)]` 变体（§4-A2）、1 项**非阻断观察**（§4-N1）。**无 FAIL 级问题，无产品缺陷。**
- **result**: `PASS`（修订设计已产出并逐条消解 CR7 findings；32 条新用例全部发现、编译、运行并通过；
  37/37 需求行有可读证据；基础检查全绿。覆盖缺口已如实标注，未以缩减覆盖换取绿灯。）
- **evidence_paths**: 本文件；`reports/tp2-test-design.md`（修订设计 + R1–R37 映射 + 实跑结果）；
  `reports/tp2-PV1-*.log`（8 条）
- **resource_cleanup**: 独立 `CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-tp1`（本包专用，
  保留供复跑）；测试用临时目录全部由既有的 `TempDir`/`TempRoot`/`TempFile` 守卫在 `Drop` 清理；
  loopback 监听器由用例自身 `bind("127.0.0.1:0")` 并在 `OwnerNode::stop` 释放；未申请 provisioner 共享资源；
  未执行 `npm install`/`npm ci`，未触碰 `node_modules`。

---

## 1. 两段交付

### 1.1 第一段：修正后的测试设计

产出 `reports/tp2-test-design.md`（**不覆盖** `reports/tp1-test-design.md`，后者保留为历史证据）。
逐条消解：

| finding | 处置 | 落点 |
| --- | --- | --- |
| **CR7-F1（MAJOR）** | 采用**修正后**的规格措辞：断言「线级无 `session/resume` + 心跳停止增长 + `runtime_running()==false` + 无会话绑定 + 错误分类为 `BackendUnsupported`」；**不再**出现「不 spawn」的字面断言 | `agent-host/tests/resume.rs::an_undeclared_capability_sends_no_resume_request_and_reclaims_the_child` |
| **CR7-F2（MAJOR）** | 采用新归类：`NULL` 与能力不支持**同路径** `nodelink.command.unsupported`；同时断言三类错误码互不相同；依据编号由误写的 R19 更正为 R21/R13 | `app/tests/session_resume_e2e.rs::workspace_revalidation_and_null_recovery_data_take_distinct_paths`、`::an_unauthorized_resume_is_rejected_before_any_local_read` |
| **CR7-F3（MINOR）** | 引用改为 `design.md` 的风险注记与正确的 R 编号；不再引用不存在的节 | `tp2-test-design.md` §0 |
| **CR7-F4（MINOR）** | **不采用**可能空转的 v5 段故障注入；R15 登记为**复用**既有确定性用例 `migration.rs::a_failed_upgrade_rolls_back_to_v1`，不复制第二份 | `storage-sqlite/tests/resume_columns.rs` 文件头「复用声明」 |
| **CR7-F5（MINOR）** | fake child 义务已由主 Agent 补进 WP5/tasks 2.5 并交付（`1376e1b`）；TP2 直接复用 `session/resume` 分支、三个场景、`--dump-requests` 与 `--dump-request-params` | `agent-host/tests/resume.rs` |
| **CR7-F6（SUGGESTION）** | **强化**：wire `command.status` 重查 + **重开同一 `data_dir` 的真实存储**从持久行读回（新增 `OwnerNode::reopen_store`） | `app/tests/session_resume_e2e.rs::a_crash_window_leaves_uncertain_persisted_across_a_reopened_store` |
| **CR7-F7（SUGGESTION）** | 不可观察的备选口径**删除**，固定为「每进程独立心跳文件」+ dump 行计数 | `agent-host/tests/resume.rs::resuming_twice_…` |
| **CR7-F8（MINOR）** | 监听器已由主 Agent 补登进 `## Runtime Resources`；不再是待澄清项 | — |
| **CR5-F2（并入，强制）** | SR-R12-2 前提改为「进程复用、单一心跳文件」；断言改为「恰好一个可派发端点 + 只有一个进程」。**不改产品代码** | 同 CR7-F7 |
| **CR4-F3（并入，强制）** | 判别式**按实测标定**（见 §4-N2），并自带反证 | `storage-sqlite/tests/resume_columns.rs::v5_appends_the_recovery_columns_instead_of_rebuilding_owned_session` |
| **CR3-F3（并入，强制）** | `Actor::Node` 的 R31：`load_recovery` 计数 0 + 存在/不存在同响应同 `details` | `app/tests/session_resume_e2e.rs::an_unauthorized_resume_is_rejected_before_any_local_read` |
| **CR5-F5（并入，强制）** | SR-R8-1/SR-R26-2 经**真实子进程 + `--dump-request-params` 读行**，断言键集合恰为两键且 `params` 非 `null` | `agent-host/tests/resume.rs::the_wire_level_resume_params_carry_the_persisted_values` |
| **CR6 Round 2 口径** | 能力未宣告终态必须 `failed` + `command.unsupported`；`uncertain` 只属 R32；并断言「Agent 拒绝恢复」是**另一个**错误码 | `app/tests/session_resume_e2e.rs::an_unsupported_agent_fails_the_resume_terminal_without_creating_a_session` |

### 1.2 第二段：可执行用例

新增 32 条用例，分布在 5 个新文件（见 §2）。全部为真实入口，无 mock 绕过被测路径。

---

## 2. 新增/修改的测试文件清单

| 文件 | 状态 | 用例数 | 说明 |
| --- | --- | --- | --- |
| `crates/acp-protocol/tests/session_resume.rs` | **新增** | 7 | R1–R4。内联字节串（不新增 `fixtures/acp/v1/` 内容，不动 `manifest.json` 计数） |
| `crates/storage-sqlite/tests/resume_columns.rs` | **新增** | 6 | R13/R14/R16/R17/R18/R21 + **CR4-F3 判别式**。程序化回退构造 v4 形状（不新增 `fixtures/storage/v4/`） |
| `crates/agent-host/tests/resume.rs` | **新增** | 7 | R5/R6/R8/R9/R10/R11/R12/R26（**线级**，真实 stdio 子进程） |
| `crates/node-link-protocol/tests/session_resume_command.rs` | **新增** | 5 | R27/R34 的 wire 层；只读复用 WP2 的 fixture 与 `commands.json` |
| `crates/app/tests/session_resume_e2e.rs` | **新增** | 7 | R22–R37 的真实 Node Link 管线行为 |
| `crates/app/tests/support/owner.rs` | **修改**（追加观察点） | — | TP2 是该目录 Merge Owner；追加内容逐项列于 §2.1 |

### 2.1 `crates/app/tests/support/owner.rs` 的**追加**内容（逐项，便于 reviewer 只审增量）

1. `ResumeProbe`（新公开类型）：`calls()`（后端 `resume` 调用计数）、`last_request()`（最近一次
   `ResumeSessionRequest`，用于断言「发出去的是持久化原文」）、`set_behavior(ResumeBehavior)`；
   `ResumeBehavior` 四个取值：`Ok` / `BackendUnsupported` / `Unavailable` / `Refused`。
2. `FlakySessionStore` 新增 `recovery_reads: Arc<AtomicU32>`，在 `load_recovery` 里计数；
   `OwnerNode::recovery_reads()` 暴露它（R31「授权先于本机读取」的计数证据）。
3. `OwnerNode::reopen_store()`：重开**同一 `data_dir`** 的真实 `SqliteStore`（CR7-F6 的持久化读回维度）。
4. `ScriptedBackends::create` 现在返回 `Some(AgentSessionId)`（此前是 `None`）——这是 R5 的前提：没有它，
   `session.create` 落不下持久化恢复数据，受控路径根本无法测 `session.resume`。
5. `OwnerNode::into_parts` 配套：把 `Paired` 拆成独立三件，使调用方能**拥有** `OwnerNode`（`stop` 需所有权）。

**未**修改既有替身的任何既有断言；`node_link_e2e.rs` 等既有测试目标的行为不变（`-p app` 全绿即证）。

---

## 3. Checks（PV1 阶段 1；逐条一行）

固定目标：`a02e2fd`（工作区干净）。`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-tp1`。
工具链由 `rust-toolchain.toml` 固定（`rustc 1.9x`，见 `tp2-PV1-clippy.log` 头）。

| Check ID | 命令 | 退出码 | 日志 |
| --- | --- | --- | --- |
| PV1-1 | `cargo fmt --all -- --check` | **0** | `reports/tp2-PV1-fmt.log` |
| PV1-2 | `cargo clippy --locked -p acp-protocol -p storage-sqlite -p agent-host -p server -p app -p node-link-protocol --all-targets --all-features -- -D warnings` | **0** | `reports/tp2-PV1-clippy.log` |
| PV1-3a | `cargo test --locked -p acp-protocol --all-features` | **0** | `reports/tp2-PV1-test-acp-protocol.log` |
| PV1-3b | `cargo test --locked -p storage-sqlite --all-features` | **0** | `reports/tp2-PV1-test-storage-sqlite.log` |
| PV1-3c | `cargo test --locked -p agent-host --all-features` | **0** | `reports/tp2-PV1-test-agent-host.log` |
| PV1-3d | `cargo test --locked -p node-link-protocol --all-features` | **0** | `reports/tp2-PV1-test-node-link-protocol.log` |
| PV1-3e | `cargo test --locked -p server --all-features` | **0** | `reports/tp2-PV1-test-server.log` |
| PV1-3f | `cargo test --locked -p app --all-features` | **0** | `reports/tp2-PV1-test-app.log` |

新增用例逐目标计数（`test result: ok` 行）：

| 目标 | 新增用例 | 结果 |
| --- | --- | --- |
| `acp-protocol/session_resume` | 7 | 7 passed; 0 failed; 0 ignored |
| `storage-sqlite/resume_columns` | 6 | 6 passed; 0 failed; 0 ignored |
| `agent-host/resume` | 7 | 7 passed; 0 failed; 0 ignored |
| `node-link-protocol/session_resume_command` | 5 | 5 passed; 0 failed; 0 ignored |
| `app/session_resume_e2e` | 7 | 7 passed; 0 failed; 0 ignored |

**未执行项（按 TP2 范围与 PV1 阶段 1 口径，不在本包判定内）**：PV1 阶段 2（`cargo test --locked
--workspace --all-features`，属候选/主分支门禁）、PV2（`npm run check`，TP2 未标 [PV2]）、
`cargo-deny` / `gitleaks`（仅 CI）。**理由**：plan.md 的 TP2 行 Verification 为「PV1（workspace，此时应全绿）」，
但阶段 2 按同一 Check ID 的阶段定义属集成基线/候选/主分支；本包跑的是阶段 1 的 crate 子集
（六 crate 覆盖了本变更全部改动面）。若主 Agent 要求 TP2 直接产出阶段 2 证据，可在本 worktree
追加一条 `cargo test --locked --workspace --all-features`（`target` 已热，预计数分钟）。

---

## 4. Coverage Index 逐行覆盖对照（R1–R37）

完整表见 `reports/tp2-test-design.md` §2。逐行结论：

- **有 TP2 新增用例**（31 行）：R1、R2、R3、R4、R5、R6、R8、R9、R10、R11、R12、R13、R14、R16、R17、
  R18、R21、R22、R23、R24、R26、R27、R28、R29、R31、R32、R34、R35、R36、R37
- **部分覆盖**（1 行）：**R25** —— 覆盖了同族的「目录被删」与「别名改指到别处」两个变体；
  **未**覆盖「持久化路径仍存在但 `canonicalize` 结果不同」的平台无关变体，原因见 §4-A1。
- **登记为上游复用、本包不新增**（6 行）：R7（`agent-host/tests/session.rs::failed_session_new_yields_no_endpoint_and_no_identifier`）、
  R15（`storage-sqlite/tests/migration.rs::a_failed_upgrade_rolls_back_to_v1`）、
  R19（`migration.rs::v3_database_upgrades_to_v5_by_appending_the_export_id_and_recovery_columns_only`）、
  R20（`migration.rs::v2_fixture_upgrades_to_v3_and_widens_only_the_audit_checks`）、
  R30 / R33（`server/src/node_link/command/tests.rs` 的越权与限流用例）。这些用例在 `-p <crate>`
  的全量运行中**均已 PASS**（见 §3 日志）。

### 4-A1（待裁决）R25 的「规范化结果变化」变体未写

`crates/app/Cargo.toml` 的 `[dev-dependencies]` 不含 `sqlx`，而 `Cargo.toml` **不在** TP2 的允许写入范围，
因此 app 层无法用原始 SQL 造出「仍存在但未规范化」的持久化 `workspace_cwd`。可选处置（**请主 Agent 裁决**）：

- (a) 在 `app` 的 `[dev-dependencies]` 加 `sqlx`（属产品清单改动，超出 TP2 写范围）；
- (b) 由 WP6 在 `server` 内补该变体（`server` 已有原始 SQL 设施）；
- (c) 记为已知覆盖缺口，由 validator 在 §7.1 标注。

TP2 **未**采取任何弱化手段（没有删断言、没有 `#[ignore]`、没有把 R25 标成已覆盖）。

### 4-A2（刻意不写）`#[cfg(unix)]` 变体

TP1 的 SR-R24-2（`chmod 000`）与 SR-R25-2（symlink 改指）**未编写**。当前执行环境是 Windows 开发机，
`#[cfg(unix)]` 分支在此**不会被执行**——写出来只能得到「编译通过但从未运行」的伪证据。按角色契约
「人工尚未实际执行不能报告通过」，宁可不写并如实标注。Linux CI runner 才会执行该路径。

### 4-N1（非阻断观察）崩溃窗口那一帧本地终态

编写过程中曾把「终态落盘失败时本次连接上的终态必须是 `uncertain`」写成断言，实测**不成立**：
Owner 会发一帧 `status: "completed"`、`terminalEventId: null` 的**本地**终态。核对
`docs/NODE_LINK_PROTOCOL.md:664`（WP6 的 CR6-F3 修复轮注记）确认：**这是已被合同收窄并明确写下的
行为**，不是缺陷——该帧不构成可稳定重放的首次结果，权威终态由启动恢复后的 `command.status` 给出。
用例因此改为断言合同真正要求的两件事：① 该帧 `terminalEventId` 必为 `null`；
② 权威终态是 `uncertain`（wire 重查 + 重开存储读回持久行，两处都断言）。
**残留风险（供 Access 侧实现者注意）**：同一 `requestId` 在崩溃前会先看到 `completed`、崩溃后却变成
`uncertain`，协议用「不得把这一轮的成功帧当成终态事实」约束客户端。这是合同选择的代价，不是本次可修的项。

### 4-N2（方法说明）CR4-F3 判别式的实测标定

CR4 给的最小修复建议是 `assert!(after.contains("IF NOT EXISTS"))`。**实测该建议不成立**：
SQLite 在 `ALTER TABLE ADD COLUMN` 时会把 `IF NOT EXISTS` 从存储文本里去掉，而 12-step 重建的脚本
本来就不含它——两条路径都不含，断言会**恒真**。实际采用的判别式是**表名是否带双引号**
（重建段的 `ALTER TABLE …_vN RENAME TO …` 会在存储文本里留下 `CREATE TABLE "owned_x"`），
并额外断言既有列定义文本逐字节保留、追加列紧跟其后、`cid` 连续、无 `_vN` 残留表，最后用 v1 库里
**确实重建过**的 `owned_audit` 作**反证**（它必须带引号）——因此判别式不恒真。标定用的临时探针
在取得数据后已删除，不在交付中。

---

## 5. 待澄清问题

1. **§4-A1 的处置选择**（需要主 Agent 裁决，因涉及 TP2 写范围之外的文件）。
2. **§4-A2 的处置选择**：是否要求 TP2 在本包补 `#[cfg(unix)]` 变体（会得到一条在 Windows 上从不
   执行的用例），还是留给 Linux CI / validator。
3. `crates/app/tests/support/owner.rs` 的追加（§2.1 共 5 项，尤其第 4 项 `ScriptedBackends::create`
   由 `None` 改为 `Some(AgentSessionId)`）请 reviewer 确认**不影响既有测试目标的语义**——
   我的证据是 `-p app` 全量 100 passed / 0 failed，但「语义未变」这一判断属 reviewer 职责。

---

## 6. 边界声明

- **零产品代码改动**：`git diff 95051f9..a02e2fd --name-only` 只含 `crates/*/tests/**`（6 个文件）。
- 未 push；未触碰 `refs/heads/main`（仍在 `81e350f`）；未归档；未修改 `plan.md`/`tasks.md`/
  `verification.md`/`design.md`/`specs/**`；未执行 `npm install`/`npm ci`/`npm update`。
- 未自行判断覆盖是否「齐」——该判定属主 Agent 的 Coverage Index 与 validator 的职责。
- 本报告**待独立 reviewer（CR8）复核**。

---

## 7. handoff_index

```yaml
handoff_index:
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "a02e2fd"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: reports/tp2-tester.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "TP2 在含 WP1–WP6 全部实现的基线 95051f9 上修订 TP1 设计（消解 CR7 两条 MAJOR 与 CR3-F3/CR4-F3/CR5-F2/CR5-F5/CR6 口径），并新增 5 个测试文件共 32 条用例；R1–R37 共 37 行全部有可读证据（31 行新增用例、1 行部分覆盖已标注、6 行登记上游复用）；6 个涉及 crate 的 PV1 阶段 1 基础检查（fmt/clippy/6×test）全部退出码 0，0 failed / 0 ignored"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "a02e2fd"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: reports/tp2-PV1-fmt.log
    result: PASS
    evidence_status: NEW
    applicability_basis: "PV1 阶段 1 第 1 项 `cargo fmt --all -- --check`，固定目标 a02e2fd，退出码 0"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "a02e2fd"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: reports/tp2-PV1-clippy.log
    result: PASS
    evidence_status: NEW
    applicability_basis: "PV1 阶段 1 第 2 项 clippy（本变更涉及的 6 个 crate，--all-targets --all-features -D warnings），固定目标 a02e2fd，退出码 0"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "a02e2fd"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: reports/tp2-PV1-test-acp-protocol.log
    result: PASS
    evidence_status: NEW
    applicability_basis: "PV1 阶段 1 第 3 项 `cargo test --locked -p acp-protocol --all-features`；含新增目标 session_resume 的 7 条用例，退出码 0"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "a02e2fd"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: reports/tp2-PV1-test-storage-sqlite.log
    result: PASS
    evidence_status: NEW
    applicability_basis: "PV1 阶段 1 第 4 项 `cargo test --locked -p storage-sqlite --all-features`；含新增目标 resume_columns 的 6 条用例（含 CR4-F3 判别式），退出码 0"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "a02e2fd"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: reports/tp2-PV1-test-agent-host.log
    result: PASS
    evidence_status: NEW
    applicability_basis: "PV1 阶段 1 第 5 项 `cargo test --locked -p agent-host --all-features`；含新增目标 resume 的 7 条用例（真实子进程 + --dump-request-params），退出码 0"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "a02e2fd"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: reports/tp2-PV1-test-node-link-protocol.log
    result: PASS
    evidence_status: NEW
    applicability_basis: "PV1 阶段 1 第 6 项 `cargo test --locked -p node-link-protocol --all-features`；含新增目标 session_resume_command 的 5 条用例，退出码 0"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "a02e2fd"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: reports/tp2-PV1-test-server.log
    result: PASS
    evidence_status: NEW
    applicability_basis: "PV1 阶段 1 第 7 项 `cargo test --locked -p server --all-features`（296 个内联用例，含 WP6 的 session.resume 路由级断言），退出码 0"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "a02e2fd"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: reports/tp2-PV1-test-app.log
    result: PASS
    evidence_status: NEW
    applicability_basis: "PV1 阶段 1 第 8 项 `cargo test --locked -p app --all-features`；含新增目标 session_resume_e2e 的 7 条用例（真实 Node Link 管线），退出码 0；既有 node_link_e2e 等目标同时全绿"
    source_evidence: NOT_APPLICABLE
```

**待审事项**：`reports/tp2-test-design.md`（修订设计与 R1–R37 映射）与本报告需由非作者的新独立
reviewer（plan.md 的 reviewer-T2 / CR8）复核，重点为：① §2.1 的共享支撑追加是否影响既有测试语义；
② §4-A1/A2/N1/N2 四项标注的处置是否被接受；③ 32 条用例的判别力（尤其「如果实现回退，它会不会失败」）。

---

# 第 2 轮（tester-A2，修复轮 / Task 2.8 / Work Package TP2）

## 8. Shared Report（第 2 轮）

- **task_id**: `2.8`
- **role**: tester
- **phase**: design-author 的修复轮（实例 `tester-A2`，新实例、非作者自审）
- **agent_context**: 子 Agent `tester-A2`，从持久材料恢复（第 1 轮报告 + 设计报告 + 实际代码），
  不继承实现对话；规划根 `D:/Project/acp-remote/openspec/changes/session-resume`（绝对路径读写）；
  代码 worktree `D:/Project/acp-remote-wt/session-resume-tp1`，分支 `agentic/session-resume-tp1`
- **target_revision**: `f44301e`（base = `a02e2fd`，第 1 轮交付）
- **scope**: 定点闭合第 1 轮 §4-A1（R25 平台无关变体）与 §4-A2（`#[cfg(unix)]` 两族）；顺带修正一个
  自查发现的**恒真观察点**。**产品代码零改动**（`crates/**/src/**` 零 diff）。
- **issues**: 0 FAIL；1 项**自查发现的测试支撑缺陷**已修正并如实登记（§8.5）；2 项**本机未执行**
  （`#[cfg(unix)]` 两族）已明确标注，不冒充通过（§8.8）；1 项**超出本轮初始授权的写入**已暂停上报、
  经主 Agent 追加授权后完成（§8.2）。
- **result**: `PASS`（限定：仅指派发的两项已在本机可执行的范围内闭合；`#[cfg(unix)]` 两族**尚未有任何
  本机证据**，由 Linux CI 首次编译与执行）
- **evidence_paths**: 本文件 §8–§9；`reports/tp2-test-design.md`（§2 R25 行、§4 A1/A2 补记、§6 实跑）；
  `reports/tp2-tester-a2-*.log`（8 条）

---

## 8.1 开工前核实

| 项 | 派发书要求 | 实际 | 处置 |
| --- | --- | --- | --- |
| worktree | `D:/Project/acp-remote-wt/session-resume-tp1` | 一致 | — |
| 分支 | `agentic/session-resume-tp1` | 一致 | — |
| HEAD | `a02e2fd` | `a02e2fd` | 一致 |
| 工作区 | 干净 | `git status --porcelain` 空 | 一致 |

**未**执行 `reset`、**未**切分支、**未**跑 `npm install/ci/update`、**未**碰 `node_modules`。

---

## 8.2 授权变更：两处清单各一行（并排 diff）

> 派发书只授权 `crates/app/Cargo.toml` 一行。我按该行改完后**实测**发现 `--locked` 全面失败，
> 暂停并上报（`contact_supervisor`，`reason: need_decision`），主 Agent 追加授权 `Cargo.lock` 恰好一行。
> 两处 diff **并排**如下（来自 `git show f44301e`）：

```diff
--- a/Cargo.lock                          (b6017ea..3fd727f)
+++ b/Cargo.lock
@@ -132,6 +132,7 @@ dependencies = [
  "serde_json",
  "server",
  "sha2 0.11.0",
+ "sqlx",
  "storage-sqlite",
  "thiserror 2.0.20",
  "tokio",
```
```diff
--- a/crates/app/Cargo.toml               (d0690ca..20a6ed9)
+++ b/crates/app/Cargo.toml
@@ -77,6 +77,7 @@ rcgen.workspace = true
 rustls.workspace = true
 tokio-rustls.workspace = true
 rustls-pki-types.workspace = true
+sqlx = { workspace = true }

 [lints]
 workspace = true
```

**为什么 `Cargo.lock` 必须跟着改（实测取证，不是推断）**：只改 `Cargo.toml` 后，

```
$ cargo metadata --format-version 1 --locked        # exit 101
$ cargo check  --locked -p app --all-targets
error: cannot update the lock file …\Cargo.lock because --locked was passed to prevent this
```

原因：`Cargo.lock` 的 `[[package]] name = "app"` 条目把**普通 + dev + build 依赖合并登记**（现表里
`rcgen`/`rustls`/`tokio-rustls`/`node-link-protocol`/`p256`/`base64` 这些 dev 依赖都在同一张表里，
`Cargo.lock:114-142`），新增一条 dev 依赖边必然要求该表多一行。

**供应链面未变（CR8 请核这一点）**：

- **本次未新增任何 crate 条目**。`Cargo.lock` 里 `sqlx` 的包条目（`0.8.6`，含 checksum）与它的**全部
  传递依赖原样未变**（diff 只有 `app` 的 `dependencies` 数组多一行，没有任何 `[[package]]` 新增/删除/改版本）。
- 版本与 feature 口径只在根 `Cargo.toml` 的 `[workspace.dependencies]`（`sqlx = { version = "0.8",
  default-features = false, features = ["runtime-tokio","sqlite","migrate"] }`），本行用
  `workspace = true` 继承，**未**在 `app` 里另开 feature 或升级版本。
- `deny.toml` 无需改：`cargo-deny`（许可证/来源/advisory）的判定集合是「被构建的 crate 集合」，
  而该集合未变。`gitleaks` 与本改动无关。
- 依赖方向门禁不受影响：`scripts/check-crate-boundaries.mjs:105` 只对**带 `dependency.path` 的
  工作区内 crate** 比 `MODULE_ARCHITECTURE.md` §5 矩阵；`sqlx` 是 registry 依赖（无 `path`），不参与。
  `CORE_FORBIDDEN`（同文件 `:88`）只作用于 `core`，`app` 加 dev 依赖不触发。
- 合同漂移门禁（`docs/CORE_PORTS_AND_STORAGE.md` §5/§7 ↔ `core::ports` / `storage-sqlite/migrate.rs`）
  不读 `Cargo.lock`，不受影响。

**支撑辅助函数**（测试支撑，非产品代码）：`crates/app/tests/support/owner.rs::overwrite_persisted_workspace_cwd`
——用 `sqlx` 原始连接对同一 `data_dir` 的 `owned_session` 执行
`UPDATE owned_session SET workspace_cwd = ?1 WHERE session_id = ?2`，并断言 `rows_affected() == 1`。

---

## 8.3 R25 的平台无关变体（本机真实执行并通过）

**用例**：`crates/app/tests/session_resume_e2e.rs::a_persisted_cwd_whose_canonical_form_differs_is_refused_before_any_backend_call`

**为何需要原始 SQL**：产品写路径（`workspace.select` → `canonicalize` → 持久化）**永远**写入规范化结果，
因此「持久化取值仍然存在、但 `canonicalize` 的结果与之逐字不同」这个形状**无法**经任何端口或本地管理
方法产生。换层（`server` / `storage-sqlite`）也解决不了：`revalidate_resume_workspace` 只在
`core::broker::resume_session`（`crates/core/src/broker.rs:1505`）这条 app 层真实管线里被触发。

**「未规范化但存在」的形态为什么是「尾部多余分隔符」——实测，不是选偏好**：

TP1 原设计（`reports/tp1-test-design.md:373`）写的是「追加 `/./` 或双重分隔符」。我用 `rustc` 单文件探针
在 Windows 上实测了五种形态（前缀均已是 canonicalize 出来的 `\\?\C:\...`）：

| 形态 | `metadata` | `canonicalize` 结果与取值逐字相同？ |
| --- | --- | --- |
| `…\hop\..\target` | **失败**（os error 123 语法不正确） | 取不到 |
| `…\hop\.\target` | **失败**（os error 3 找不到路径） | 取不到 |
| `…\\target`（双分隔符） | **失败**（os error 123） | 取不到 |
| `…\target\.` | **失败**（os error 123） | 取不到 |
| `…\target\`（**尾部分隔符**） | **成功**，`is_dir = true` | **否**（canonicalize 去掉尾分隔符） |

根因：Windows 上 `std::fs::canonicalize` 返回 `\\?\` **verbatim** 路径，verbatim 路径**不做归一化**，
所以 `..`/`.`/`//` 那些形态在 Windows 上根本**不存在**，达不到 R25 的「路径仍然存在」前提（达不到就是
在测 R24「目录被删」）。尾部分隔符在两个平台上都同时满足「`metadata` 成功且是目录」与
「`canonicalize` 结果逐字不同」——是唯一真正**平台无关**的形态。（Linux 侧：`realpath` 按 POSIX 去掉
尾分隔符，行为一致；这一条**未在本机执行**，见 §8.8。）

**用例的四条前提断言（全部是断言，不是假设）**——目的是让**失败点唯一**：

1. 产品写路径持久化的是规范化结果（改写前）。
2. 改写后，**产品读路径** `load_recovery` 原样返回那个未规范化字符串（改写确实生效、无第二处口径），
   且只改了 `cwd` 一列。
3. `is_absolute` ✓、`metadata` 成功 ✓、`is_dir` ✓、`canonicalize` 可得出且**逐字不同** ——
   ⇒ 复校验（`crates/core/src/broker.rs:3628-3645`）的前三条守卫全部通过，**唯一**可能失败的就是
   「`canonicalize` 的结果与持久化取值逐字相同」这一条。
4. `ResumeSessionRequest::try_new(…, non_canonical)` **成功** ——
   ⇒ 拒绝不可能来自值对象构造（`broker.rs:3613-3621`），只能来自逐字比对。
5. 新解析出的路径**恰好就是那个完全合法、已注册、仍然存在的目录**（前提③的第四条）——
   这是判别力的关键，见下。

**「不使用新解析出的路径发送 `session/resume`」的判据选择与理由**：

派发书给了两个选项（fake ACP child 的 `--dump-request-params`，或 `params.cwd` 不是新解析值），
并允许「按实现实际行为选择可观察的判据」。我选的是**受控后端的调用计数**：

- **理由**：R25 的复校验发生在 `core::broker::resume_session` 内部，**严格早于**
  `SessionBackendFactory::resume`（`broker.rs:1489-1505` 的顺序注释：③ 请求构造与 cwd 复校验 →
  ④ `resume`，后者是「唯一可能 spawn 的副作用」）。app 层受控路径的 Agent 后端是**脚本化替身**
  （`support/owner.rs::ScriptedBackends`，不启动进程），所以在这一层**不存在**真实子进程，
  `--dump-request-params` 根本没有产生的机会；在 `agent-host` 层加真实子进程 dump 也测不到本分支——
  那里根本到不了（`agent-host` 只在 core 校验**之后**才被调用）。
- 因此本层的等价且更强的判据是：**后端 `resume` 一次都没被调用**（`resume_probe.calls()` 不变）
  **且** `resume_probe.last_request()` 为 `None`。后端没被调用 ⇒ 没有任何 `session/resume` 被发出，
  也就**不可能**用新解析出的路径发出。这比「收到的 cwd 不是新值」更强：它是「根本没发」而不是
  「发了但值不对」。

**判别力推演：「如果实现回退成『直接用新解析出的路径』，本用例会怎么失败」**

前提⑤是关键：新解析出的路径就是那个合法且已注册的 `workspace` 目录本身。设回退实现做了以下任一件事：

| 回退方式 | 实际会发生什么 | 本用例哪条断言先炸 |
| --- | --- | --- |
| 跳过 `canonicalize` 逐字比对（或整体跳过复校验），拿新解析的路径去恢复 | 后端被调用一次（`calls()` +1）、`last_request()` 变成 `Some(cwd = workspace)`、终态 `completed` | ① `status == "failed"` 失败；② `calls() == calls_before` 失败；③ `last_request().is_none()` 失败（**三条独立失败**） |
| 复校验失败后**回退到按 workspace 别名重新解析** | 别名仍指向 `workspace`（本用例没改别名）⇒ 同上一行 | 同上三条 |
| 复校验失败后**回退到「最接近的目录」** | 同上（唯一存在的候选就是 `workspace`） | 同上三条 |
| 复校验正确拒绝，但**顺手把持久化取值改写成新解析出的路径** | 终态与调用计数都对 | ④ `assert_same_recovery(&tampered, &after, …)` 失败（`workspace_cwd` 变成规范化结果） |
| 复校验正确拒绝，但**新建了一个会话**兜底 | 会话清单变成 2 条 | ⑤ `listed.len() == 1` 失败 |
| 错误码归类错（归到 `command.unsupported`） | 终态仍是 `failed` | ⑥ `error.code == "nodelink.internal.unavailable"` 失败 |

另外：用例把 `nodelink.internal.unavailable` 与既有 R24/R37 路径的错误码放在同一断言体系里，
**互不相同**这一点由 `workspace_revalidation_and_null_recovery_data_take_distinct_paths` 承担。

**执行结果**：本机（Windows）**真实执行并通过**，`app/session_resume_e2e` 8 passed / 0 failed / 0 ignored。

---

## 8.4 `#[cfg(unix)]` 两族（本机不执行，由 Linux CI 执行）

两条用例都写在 `crates/app/tests/session_resume_e2e.rs`，标 `#[cfg(unix)]`：

| 用例 | 场景 | 前置（自证式断言） | 断言 |
| --- | --- | --- | --- |
| `a_persisted_directory_replaced_by_a_symlink_is_refused_before_any_backend_call` | R25 的 **spec 原例**：持久化目录被换成指向别处的符号链接 | `metadata(路径)` 成功且 `is_dir` ✓；`canonicalize` 结果**逐字不同** ✓；且新结果**等于** `elsewhere` 的规范化路径 | `failed` + `nodelink.internal.unavailable`；后端 `calls()` 不变；持久化取值未被改写 |
| `an_inaccessible_persisted_directory_is_refused_before_any_backend_call` | R24 的**权限丢失**变体 | `metadata(work).is_err()` ✓（前提自证，见下） | `failed` + `nodelink.internal.unavailable`；后端 `calls()` 不变；持久化取值未被改写 |

**权限为什么作用在「父目录」而不是叶子目录**：`stat`/`lstat` 路径上的**每个分量**都需要其**父目录**的
search 权限；叶子目录自身的模式位不参与它自己的查找。只把叶子目录 `chmod 000`，`metadata` 与
`canonicalize` 很可能**仍然成功** ⇒ 那就变成一个「本该失败却通过」的假证据。本用例因此：

- 把 `000` 设在持久化目录的**父目录**上（这才是真正让路径不可访问的那一位）；并
- 用前提断言自证：`assert!(std::fs::metadata(&work).is_err(), "chmod 000 必须让持久化路径不可访问；
  若本进程是 root，POSIX 权限检查对它无效")`。以 root 运行时这里会**响亮地**失败，而不是给出一条
  看似通过的假证据（这正是 CR4-F3 那类「恒真断言」的同类风险，所以不写成假设）。
- 用 `ModeRestore` 守卫（`Drop` 里恢复 `0700`），保证即使中途 panic 也不会在临时目录里留下 `000`。

**状态标注（必读）**：

- **本机（Windows 开发机）从未执行这两条用例**。直接证据：`cargo test -p app --test
  session_resume_e2e -- --list` 的输出**恰好 8 条**，不含这两条（`reports/tp2-tester-a2-test-list-app.log`）。
- **本机也没有它们的编译证据**：`cargo check --locked -p app --all-targets --target
  x86_64-unknown-linux-gnu` 失败在依赖的 C 工具链上（`cc-rs`: `failed to find tool
  "x86_64-linux-gnu-gcc"`），不是本轮代码的问题。
- 因此这两条的**编译与首次执行都在 Linux CI 的 `checks` job**（`.github/workflows/ci.yml` 的
  `runs-on: ubuntu-latest`，非 root）。**本地未执行不等于通过**；若 CI 上它们失败，按流程开新问题编号。
- 可以确认的是：它们在 Windows 上**不产生任何编译影响**——`cargo clippy --locked -p app --all-targets
  --all-features -- -D warnings` 退出码 0（`#[cfg(unix)]` 的 item 在本平台整个被剔除，不会出现
  未用项/未解析符号的告警），且所有 `std::os::unix::fs::{symlink, PermissionsExt}` 引用都在
  `#[cfg(unix)]` 边界内（Windows 上能编译通过即是证据）。

---

## 8.5 自查发现并修正的测试支撑缺陷（`recovery_reads` 恒为 0）

**这是我在写本轮用例时自查发现的，属第 1 轮交付的缺陷，如实登记**：

- **位置**：`crates/app/tests/support/owner.rs::OwnerNode::start_with`（第 1 轮新增的
  `FlakySessionStore.recovery_reads` 与 `OwnerNode.recovery_reads` 字段）。
- **现象**：两处**各自** `Arc::new(AtomicU32::new(0))`——装饰器 `FlakySessionStore` 递增的是**它自己
  那个** `Arc`，而 `OwnerNode::recovery_reads()` 读的是**观察字段那个** `Arc`。两个 `Arc` 从不共享，
  因此计数**恒为 0**。
- **后果**：第 1 轮 R31（`an_unauthorized_resume_is_rejected_before_any_local_read`，CR3-F3 强制项）的
  断言 `owner.recovery_reads() == reads_before` 因此**恒真**——它证明不了「授权先于本机读取」，
  只能证明「0 == 0」。这是与 CR4-F3 同类的恒真断言问题，我上一轮没有发现。
- **修正**（`f44301e`）：两处共用**同一个** `Arc`（`let recovery_reads = Arc::new(...)`，
  装饰器 `Arc::clone(&recovery_reads)`，字段直接用 `recovery_reads`）。
- **防复发**：在 R31 用例里加了**自检断言**——先用同一个端口做一次已知的 `load_recovery`，断言计数
  **确实递增**，再进入「授权先于本机读取」的计数断言。现在这条断言不再可能恒真。
- **验证**：修正后 R31 用例**仍然 PASS**，且自检断言通过 ⇒ 「未授权恢复期间 `load_recovery` 计数不变」
  现在是**真实证据**（不是恒真）。`-p app` 其余目标（`node_link_e2e` 等）行为不变。
- **对第 1 轮结论的影响**：R31 的**行为结论**（「授权先于本机读取」，错误码 `export.not_granted`、
  存在/不存在同响应）不受影响——那部分由错误码与响应同形断言承担，本来就不依赖计数器。受影响的只有
  「计数为 0」这一个维度，而它现在是真证据。第 1 轮 §4 的 A1/A2 结论与本节修正无关。

---

## 8.6 给 CR8 的两条定位信息（不改代码，只给结论）

### (1) `ScriptedBackends::create` 由 `None` 改为 `Some(AgentSessionId)`（主 Agent 已决定交 CR8 评审）

- **位置**：`crates/app/tests/support/owner.rs`，`impl SessionBackendFactory for ScriptedBackends::create`
  （返回值 `Ok(Box::new(ScriptedEndpoint { …, agent_session_id: Some(scripted_agent_session_id(session)), … }))`），
  以及同文件的 `fn scripted_agent_session_id(session: &SessionId) -> AgentSessionId`。
- **为什么必须这样**：R5 要求「创建会话时暴露 ACP 会话标识」，而 `Broker::resume_session` 的输入
  （`SessionRecoveryRecord.agent_session_id`）只能来自**持久化**的 `owned_session.agent_session_id`。
  若 `create` 不给出标识，core 就没有可持久化的值 ⇒ `load_recovery` 返回 `Ok(None)` ⇒
  `session.resume` 走「没有恢复数据 = 不支持」那条路径（`broker.rs:1499-1502`）。**没有它，
  §2 里所有受控路径的 `session.resume` 用例都无从谈起。**
- **影响面**：只有 `ScriptedBackends`（app 受控路径专用的后端替身）。`open` 仍返回 `agent_session_id: None`
  （`open` 不建立 ACP 会话，符合语义）。`-p app` 全量 102 → 本轮 102（57+0+1+11+12+3+8+2+8+0）
  逐目标计数与第 1 轮**逐项一致**（只有 `session_resume_e2e` 从 7 变 8），既有目标
  `node_link_e2e` / `node_link_listener` 等全绿 ⇒ 替身语义的「是否影响既有测试」由 reviewer 判定，
  我的证据是上述逐目标计数对比。

### (2) CR4-F3 判别式的纠正（可独立审计）

- **假设（CR4 给的最小修复建议）**：`assert!(after.contains("IF NOT EXISTS"))` 用来区分
  「`ALTER TABLE ADD COLUMN` 追加」与「12-step 重建」。
- **反证（实测）**：SQLite 在 `ALTER TABLE … ADD COLUMN` 时会把 `IF NOT EXISTS` 从 `sqlite_master` 的
  **存储文本**里去掉；而 12-step 重建脚本本来就不含它。⇒ **两条路径都不含** ⇒ 该断言**恒真**，
  判别力为零。（新建库才含 `IF NOT EXISTS`，但那不是本用例要比的两条路径。）
- **实测（我改用的判别式）**：**表名是否带双引号**。

  | 路径 | `sqlite_master` 存储文本（实测） | 含 `IF NOT EXISTS`？ | 表名带引号？ |
  | --- | --- | --- | --- |
  | ALTER 追加（v4→v5 的 `owned_session`） | `CREATE TABLE owned_session (… closed_at TEXT, agent_session_id TEXT, workspace_cwd TEXT) STRICT` | 否 | **否** |
  | 12-step 重建（v1→v5 的 `owned_audit`） | `CREATE TABLE "owned_audit" (… ) STRICT` | 否 | **是**（`ALTER TABLE …_vN RENAME TO …` 的签名） |
  | 新建库 | `CREATE TABLE IF NOT EXISTS owned_session (… )` | 是 | 否 |

- **因此判别式不恒真**（有反证行），并额外断言：既有列定义文本**逐字节保留**、追加列紧跟其后、`cid` 连续、
  无 `_vN` 残留表。落点：`crates/storage-sqlite/tests/resume_columns.rs::v5_appends_the_recovery_columns_instead_of_rebuilding_owned_session`。
- **本轮未改动该文件**（第 1 轮定稿，本轮只是把它写进报告供审计）。标定用的临时探针在第 1 轮即已删除。

---

## 8.7 Checks（第 2 轮，逐条一行）

固定目标 `f44301e`（工作区干净）；`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-tp1`；
工具链由 `rust-toolchain.toml` 固定（`1.98.1-x86_64-pc-windows-msvc`，见 clippy 日志头）。

| Check ID | 命令 | 退出码 | 日志 |
| --- | --- | --- | --- |
| A2-C1 | `cargo fmt --all -- --check` | **0** | `reports/tp2-tester-a2-fmt.log` |
| A2-C2 | `cargo clippy --locked -p app --all-targets --all-features -- -D warnings` | **0** | `reports/tp2-tester-a2-clippy.log` |
| A2-C3 | `cargo test --locked -p app --all-features` | **0** | `reports/tp2-tester-a2-test-app.log` |
| A2-C4 | `cargo test --locked -p app --all-features --test session_resume_e2e` | **0** | `reports/tp2-tester-a2-test-app-session_resume_e2e.log` |
| A2-C5 | `cargo test --locked -p app --all-features --test session_resume_e2e -- --list` | **0** | `reports/tp2-tester-a2-test-list-app.log` |
| A2-C6 | `cargo test --locked -p storage-sqlite --all-features` | **0** | `reports/tp2-tester-a2-test-storage-sqlite.log` |
| A2-C7 | `cargo test --locked -p agent-host --all-features` | **0** | `reports/tp2-tester-a2-test-agent-host.log` |
| A2-C8 | `cargo metadata --format-version 1 --locked`（清单与锁文件一致性的门禁口径） | **0** | `reports/tp2-tester-a2-metadata-locked.log` |

`app/session_resume_e2e`：**8 passed; 0 failed; 0 ignored**（第 1 轮 7 条 + 本轮 1 条）。
A2-C5 的 `--list` 输出**恰好 8 条**，不含两条 `#[cfg(unix)]` 用例（见 §8.4）。
逐目标计数与第 1 轮逐项一致：`src/lib.rs` 57、`src/main.rs` 0、`audit_export` 1、`cli_commands` 11、
`daemon_lifecycle` 12、`node_link_e2e` 3、`node_link_listener` 8、`node_pair_export_ids` 2、
`session_resume_e2e` **8**（原 7）。

**提交信息规范的一条发现（供主 Agent 处理，不属本包写范围）**：本分支前两个提交
（`f8133f2`、`a02e2fd`）用的 scope 是 `session-resume`，而 `commitlint.config.mjs` 的 `SCOPES` 词表里
**没有** `session-resume`。实测 `echo "test(session-resume): x" | npx --no-install commitlint` 报
`scope must be one of [...]`。本轮提交改用合法 scope `test(app)`（实测通过）。若这两个旧提交会随本分支
进入推送范围，CI 的 `commits` job 会因此失败——请主 Agent 决定（改写历史或 squash 时的标题口径）。

---

## 8.8 未执行项（如实列出，不冒充通过）

1. **`#[cfg(unix)]` 两族的编译与执行**（符号链接改指、权限丢失）：本机 Windows，从未执行；
   本机也**无编译证据**（交叉 `x86_64-unknown-linux-gnu` 缺 `x86_64-linux-gnu-gcc`）。
   由 Linux CI `checks` job 首次编译并执行。**本地未执行不等于通过。**
2. **`cargo check --target x86_64-unknown-linux-gnu -p app --all-targets`**：因缺 C 工具链失败
   （环境限制，非代码问题）。
3. **PV1 阶段 2**（`cargo test --locked --workspace --all-features`）、**PV2**（`npm run check`）：
   同第 1 轮，不在本包判定内。
4. **`cargo-deny` / `gitleaks`**：仅 CI，无本地等价物。§8.2 已给出「判定集合未变」的论证，但**未实测**。
5. **R25 平台无关变体在 Linux 上的行为**：本轮只在 Windows 实跑。尾部分隔符形态在 Linux 上
   依赖 `realpath` 去掉尾分隔符（POSIX 规定），**本机未验证**；Linux CI 会同时重跑这条。

---

## 8.9 边界声明

- **零产品代码改动**：`git diff a02e2fd..f44301e --name-only` =
  `Cargo.lock`、`crates/app/Cargo.toml`、`crates/app/tests/session_resume_e2e.rs`、
  `crates/app/tests/support/owner.rs`。`crates/**/src/**` **零 diff**。
- 清单改动**只有**经授权的两行（§8.2）；未改 `[dependencies]`、未升级版本、未加 feature、未动
  `crates/app/Cargo.toml` 的其它任何部分。
- 未 push；未触碰 `refs/heads/main`；未归档；未修改 `plan.md` / `tasks.md` / `verification.md` /
  `design.md` / `specs/**` / `schemas/**` / `fixtures/**` / `compatibility/**`；
  未执行 `npm install` / `npm ci` / `npm update`；未对 `node_modules` 做任何操作。
- **没有为了让用例通过而弱化断言或 `#[ignore]`**：三条新用例的每条前提都是断言；`#[cfg(unix)]` 的
  权限用例在 root 下会**失败**（而不是跳过）。
- **新测试没有暴露产品缺陷**：R25 平台无关变体在产品**未改动**的前提下首次即通过。
  §8.5 暴露的是**我自己上一轮的测试支撑缺陷**，已在测试范围内修正并登记。

---

## 9. handoff_index（第 2 轮）

```yaml
handoff_index:
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: 2
    stage: work-package
    target_revision: "f44301e"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: reports/tp2-tester.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "第 2 轮定点闭合第 1 轮 §4-A1（R25 平台无关变体，本机 Windows 真实执行并通过：新增 a_persisted_cwd_whose_canonical_form_differs_is_refused_before_any_backend_call）与 §4-A2（#[cfg(unix)] 两族：符号链接改指 + 权限丢失，本机不执行、由 Linux CI 执行、已如实标注）。app/session_resume_e2e 8 passed / 0 failed / 0 ignored；8 条 Check 退出码全 0。附带修正第 1 轮自审未发现的恒真观察点（support/owner.rs 的 recovery_reads 建了两个 Arc，计数恒 0，使 R31 计数断言恒真），并补「计数器确实在动」的自检断言，修正后 R31 仍 PASS。清单改动仅经授权的两行（Cargo.toml 一行 + Cargo.lock 一行），未新增任何 crate 条目，传递依赖原样不变"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: 2
    stage: work-package
    target_revision: "f44301e"
    evidence_type: CHECK
    evidence_id: A2-C1
    report_path: reports/tp2-tester-a2-fmt.log
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo fmt --all -- --check，固定目标 f44301e，退出码 0"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: 2
    stage: work-package
    target_revision: "f44301e"
    evidence_type: CHECK
    evidence_id: A2-C2
    report_path: reports/tp2-tester-a2-clippy.log
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo clippy --locked -p app --all-targets --all-features -- -D warnings，退出码 0；同时证明两条 #[cfg(unix)] 用例在 Windows 平台上被整体剔除、不产生任何编译影响"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: 2
    stage: work-package
    target_revision: "f44301e"
    evidence_type: CHECK
    evidence_id: A2-C3
    report_path: reports/tp2-tester-a2-test-app.log
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo test --locked -p app --all-features，退出码 0；session_resume_e2e 8 passed / 0 failed / 0 ignored，其余目标逐项与第 1 轮一致"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: 2
    stage: work-package
    target_revision: "f44301e"
    evidence_type: CHECK
    evidence_id: A2-C4
    report_path: reports/tp2-tester-a2-test-app-session_resume_e2e.log
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo test --locked -p app --all-features --test session_resume_e2e，退出码 0；R25 平台无关变体在本机真实执行并通过"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: 2
    stage: work-package
    target_revision: "f44301e"
    evidence_type: CHECK
    evidence_id: A2-C5
    report_path: reports/tp2-tester-a2-test-list-app.log
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo test --locked -p app --all-features --test session_resume_e2e -- --list，退出码 0；输出恰好 8 条，不含两条 #[cfg(unix)] 用例——这是「本机不执行它们」的直接证据，不是通过证据"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: 2
    stage: work-package
    target_revision: "f44301e"
    evidence_type: CHECK
    evidence_id: A2-C6
    report_path: reports/tp2-tester-a2-test-storage-sqlite.log
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo test --locked -p storage-sqlite --all-features，退出码 0（受影响的相邻 crate 无回归；含 CR4-F3 判别式用例）"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: 2
    stage: work-package
    target_revision: "f44301e"
    evidence_type: CHECK
    evidence_id: A2-C7
    report_path: reports/tp2-tester-a2-test-agent-host.log
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo test --locked -p agent-host --all-features，退出码 0（含 7 条真实子进程 resume 用例，无回归）"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: 2
    stage: work-package
    target_revision: "f44301e"
    evidence_type: CHECK
    evidence_id: A2-C8
    report_path: reports/tp2-tester-a2-metadata-locked.log
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo metadata --format-version 1 --locked，退出码 0；证明 crates/app/Cargo.toml 与 Cargo.lock 在 --locked 下一致（scripts/check-crate-boundaries.mjs 内部也用 cargo metadata --locked）"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: 2
    stage: work-package
    target_revision: "f44301e"
    evidence_type: EXECUTION
    evidence_id: A2-UNIX-VARIANTS
    report_path: reports/tp2-tester.md#84
    result: NOT_EXECUTED
    evidence_status: PENDING_PLATFORM
    applicability_basis: "两条 #[cfg(unix)] 用例（符号链接改指、权限丢失）在本机（Windows）从未执行，本机也无编译证据（交叉 x86_64-unknown-linux-gnu 缺 x86_64-linux-gnu-gcc）；由 Linux CI checks job（ubuntu-latest，非 root）首次编译并执行。本地未执行不等于通过"
    source_evidence: NOT_APPLICABLE
```

**待审事项（CR8）**：① §8.2 两处清单 diff 与「未新增 crate 条目」的论证；② §8.3 R25 用例的判别力
推演表与判据选择理由；③ §8.4 两条 `#[cfg(unix)]` 用例的前提自证与平台状态标注；④ §8.5 `recovery_reads`
恒真缺陷的修正与 R31 重跑结果；⑤ §8.6 两条定位信息（`ScriptedBackends::create`、CR4-F3 纠正）；
---

# 第 3 轮（tester-A3，CR8 记录层修正轮 / Task 2.8 / Work Package TP2）

## 10. Shared Report（第 3 轮）

- **task_id**: `2.8`
- **work_package**: TP2
- **role**: tester（修复轮实例 `tester-A3`）
- **phase**: `design-author`
- **agent_context**: 实例 `tester-A3`（CR8 Round 1 之后的修正轮）。只读 CR8 报告、前两轮报告与
  `f44301e` 的内容；**未参与** CR8 检视本身，也未与 reviewer 共享线程。
- **target_revision**: `3484541`（base `f44301e`）
- **scope**: CR8 的 6 条 MINOR + 2 条 SUGGESTION 的**记录层修正**。本轮**全是文字/注释修正**：
  不改任何行为、不新增/删除任何用例、不弱化任何断言、不动清单文件。
- **changes**: `crates/app/tests/session_resume_e2e.rs`、`crates/app/tests/support/owner.rs`、
  `crates/storage-sqlite/tests/resume_columns.rs`（**仅注释与 rustdoc**）+
  `reports/tp2-test-design.md`、`reports/tp2-tester.md`（本文件）、3 份日志。
- **checks**: 见 §10.6（本轮只跑 `cargo fmt --check` 与 `-p storage-sqlite -p app` 全量 test，
  判据是**不破坏既有构建与测试**）。
- **issues**: 0 新开。CR8-F6 的 PENDING 状态**原样保留**（见 §10.3）。
- **result**: **PASS**（本轮范围内）

## 10.1 开工前核实

| 项 | 期望 | 实际 | 判定 |
| --- | --- | --- | --- |
| worktree | `D:/Project/acp-remote-wt/session-resume-tp1` | 一致 | ✅ |
| 分支 | `agentic/session-resume-tp1` | 一致 | ✅ |
| HEAD | `f44301e` | `f44301e` | ✅ |
| 工作区 | 干净 | `git status --porcelain` 空 | ✅ |

## 10.2 六条逐条处置与依据

### CR8-F1（MINOR）—— 注释与代码直接矛盾 ✅ 已修

- **位置**：`crates/storage-sqlite/tests/resume_columns.rs` 中
  `v5_appends_the_recovery_columns_instead_of_rebuilding_owned_session` 的 rustdoc ①/④。
- **依据核对**：CR8 的判断**成立**。原 rustdoc ① 写「DDL **含** `IF NOT EXISTS`」、④ 写
  「`owned_audit` 的文本**不含** `IF NOT EXISTS`」；而代码里两条路径的实测都是**不含** `IF NOT EXISTS`
  （`ALTER TABLE ADD COLUMN` 会把它从存储文本里去掉），判别式实为
  **表名是否带双引号**（12-step 重建段的 `RENAME` 签名）。
- **处置**：① 改为「存储文本以**不带引号**的 `CREATE TABLE owned_session (` 开头（即追加路径）」，
  并显式写明 `IF NOT EXISTS` **不能**作判别式（两条路径都不含、按它断言会恒真）；
  ④ 改为「`owned_audit` 必须以**带引号**的 `CREATE TABLE "owned_audit" (` 开头」。
- **未动断言**：确认 `git diff` 中该用例的 `assert!` / `assert_eq!` **一个字符都没改**。

### CR8-F2（MINOR）—— 报告引用了错误的提交 ✅ 已修（含对应关系说明）

- CR8 无法用 git 验证 `4146611` 是否存在。本轮**核实**：`git cat-file -t 4146611` → `commit`，
  完整 SHA `4146611a2e7002b89534227293d2ad547f5e11de`，**dangling**（不在任何 ref 上，
  `git log --all` 无匹配），提交信息与时间与 `f44301e` 相同，且
  `git rev-parse 4146611^{tree}` == `git rev-parse f44301e^{tree}` == `97bbf785cbafb1c2df22878be9b92d263bcfd7ab`，
  `git diff 4146611 f44301e` **为空**。
- **结论**：`4146611` 是**同一提交的 amend 前旧 SHA**（父提交被 amend 而重写），不是另一个交付。
  权威值是 `f44301e`。
- **处置**：`tp2-test-design.md` 头部、§4-A1/A2 的「已闭合」标记、§6 标题三处订正为 `f44301e`，
  并在头部加了一段**订正记录**（含上述 tree 相同、dangling、diff 为空三条实测依据）。
  订正痕迹按 CR8 要求**保留不删**。

### CR8-F3（MINOR）—— 两条覆盖行的证据指针不可定位 ✅ 已修（CR8 判断成立）

- **R30**：补上两条已核实存在的用例名 ——
  `server/src/node_link/command/tests.rs::an_unauthorized_command_is_rejected_audited_and_has_no_side_effect`（**`:1509`**）
  与 `::an_unauthorized_session_resume_is_rejected_before_any_local_read`（**`:2888`**）。
  本轮已用 `grep -n` 复核两处行号。
- **R33**：CR8 说原指针 `:992` 不成立——**成立**。实测 `:992` 落在
  `session_submit_body` 的 session-scope `matches!` 命令清单里（与限流无关）。改为真正的证据
  `::the_command_rate_limit_replies_rate_limited_and_then_closes`（**`:2303`**，其上方注释即
  `[R71]：单连接 120/分钟`），并补上**按构造成立**的理由：`crates/server/src/node_link/command.rs:248`
  的 `if !self.admit_rate(handle)` 位于 `match submit.command`（`:251`）**之前**、对**全部**命令生效，
  因此 `session.resume` 被同一条连接级限流覆盖。本轮已复核这两处行号。

### CR8-F4（MINOR）—— 报告内部计数不自洽 ✅ 已修

- **核对**：第 2 轮后实际为 `acp-protocol` 7 + `storage-sqlite` 6 + `agent-host` 7 +
  `node-link-protocol` 5 + `app` **8** = **33 条本机可执行**（原写 32 / app 7）。
- **处置**：§2 首段与末尾「统计」行都改为 33（app 8），并显式注明**另有 2 条 `#[cfg(unix)]`
  变体不在此计数内**，状态 PENDING、关闭条件是 Linux CI 首次编译+执行。

### CR8-F5（MINOR）—— R25 覆盖混档 + 一条注释把「能力不支持」说成「NULL」 ✅ 已修

- **报告侧**：§2 的 R25 行「结果」列由单档「已覆盖」改为**三档**：
  ① 1 条平台无关变体**本机实跑 PASS，可计入**；② 2 条 `#[cfg(unix)]` 变体**设计上覆盖、判定待
  Linux CI，不得记 PASS**；③ 上游同族变体（删除/别名改指/同名重建）已覆盖并本机 PASS。结果列写
  「①③ PASS；② PENDING」。
- **代码侧**：`crates/app/tests/session_resume_e2e.rs` 的文件级 doc 与第 ③ 段注释。**核实 CR8 成立**：
  第 ③ 段只执行 `owner.resume_probe.set_behavior(ResumeBehavior::BackendUnsupported)`，
  **没有任何语句把会话的两列置 `NULL`**（会话自身的两列始终有值，④ 段还断言恢复数据仍在）。
  改为「能力不支持（本用例） vs 两列 `NULL`（由
  `server/src/node_link/command/tests.rs::session_resume_without_persisted_recovery_data_fails_as_unsupported`
  与 `resume_columns.rs` 承担）」。本轮已复核 `resume_columns.rs` 的
  `a_half_null_recovery_pair_is_still_no_recovery_data` 与
  `upgraded_sessions_keep_their_bytes_and_report_no_recovery_data` 两条确实承担 NULL 侧。
- **未动行为**：该用例的断言与执行序列**一字未改**。

### CR8-F6（MINOR）—— 两条 `#[cfg(unix)]` 零证据 ⚠️ **本轮明确不做，保持 PENDING**

详见 §10.3。

## 10.3 CR8-F6：保持 PENDING 的明确重申

两条 `#[cfg(unix)]` 用例（`a_persisted_directory_replaced_by_a_symlink_is_refused_before_any_backend_call`、
`an_inaccessible_persisted_directory_is_refused_before_any_backend_call`）在本轮结束时**仍为 PENDING**：

- **零编译证据、零执行证据**：本机是 Windows 开发机，`#[cfg(unix)]` 在本机被剔除；
  交叉 `cargo check --target x86_64-unknown-linux-gnu` 因缺 `x86_64-linux-gnu-gcc`（`cc-rs`/`ring`）失败。
- 本轮**未**改动它们的断言、cfg 结构或任何行为；**未**用 `#[ignore]` 或 `#[cfg(not(unix))]`
  包裹或删除它们来「让不确定性消失」；**未**在任何报告或注释里把措辞改写成「已通过」「已验证」。
- **关闭条件（不变）**：Linux CI 的 `checks` job 在 `ubuntu-latest`（非 root）首次**编译并执行**这两条。
  若失败（最可能是编译期），**开新问题编号回 TP2**，不改判本轮结论。

## 10.4 CR8-S1：判断与实测值 ✅ 已处置（按实测值分支，未加会失败的断言）

- **待判断的问题**：`session_resume_e2e.rs` 中
  `assert_eq!(absent_denied["body"]["error"]["details"], denied["body"]["error"]["details"])`
  在两侧同为 `null` 时会平凡成立——**`details` 实际是什么值？**
- **静态追溯**：R31 用例里 Access Node 只拿到 `grant.observe`（不是 `grant.remote-work`），
  因此 `command.rs::on_session_resume` 第 ② 步的 `authorize_node(…, "grant.remote-work", …)` 失败；
  两条路径（会话存在 / 不存在）都先过 `session_target`，而它**只解析 `sessionRef`、不做本机读取**，
  两者的 `CommandFault` 都是 `NotGranted(None)` ⇒ `deny()` 走 `RawObject::empty()`
  ⇒ 按 `acpr-wire` 的实现是 `{}`（空**对象**），不是 `null`。
- **实测（机器证据，不是推断）**：临时在该用例里插入 `eprintln!` 探针（只打印，不改断言），
  单跑该用例后**立即回滚**。输出：
  `S1-PROBE denied.details={} absent.details={} denied.is_object=true absent.is_object=true`
  日志：`reports/tp2-tester-a3-s1-probe.log`。
- **结论**：`details` **实测为 `{}`（空对象，非 `null`）**，`is_object()` 两侧均为 `true`。
  ⇒ 按派发书的分支规则走「**实测非 `null`**」一侧：**不加固断言**，只在断言上方加注释写明
  实测值与它作为**结构性防泄露守卫**的性质。
- **如实补充（比 CR8 的措辞更严格的一层）**：CR8 的前提是「两侧同为 `null` 则平凡成立」——
  这个前提**字面不成立**（实测是 `{}`）。但**判别力**上它仍是恒真的：两条路径都走 `deny()` 的
  `NotGranted(None)` 硬编码分支，与会话是否存在无关。因此该断言的角色是**回归守卫**
  （若将来某条路径改回 `NotGranted(Some(parameter))`，`details` 会变成 `{"parameter": …}` 而失败），
  而**不是当前的判别点**——R31 的判别力由前一条 `code` 相等断言、目标会话目录**已被删除**这个前提，
  以及 `load_recovery` 读计数不变承担。已把这一层写进代码注释。

## 10.5 CR8-S2：模块注释未登记第三种测试介入 ✅ 已修

- **核对**：CR8 成立。`crates/app/tests/support/owner.rs` 模块注释声明「**两处**刻意的测试替身」，
  未登记第 2 轮新增的 `overwrite_persisted_workspace_cwd`。
- **处置**：改为「**三处刻意的测试替身/测试介入**」，补第三条并写明它经 `sqlx` **原始 SQL**
  直接改写 `owned_session.workspace_cwd`、**绕过全部端口与本地管理方法**，且注明它是三者中
  介入最深的一处（产品写路径永远写 `canonicalize` 的结果，该形状无法经任何产品入口产生）。

## 10.6 Checks（第 3 轮，逐条一行）

| Check ID | 命令 | 退出码 | 日志 | 结果 |
| --- | --- | --- | --- | --- |
| A3-C1 | `cargo fmt --all -- --check` | 0 | `reports/tp2-tester-a3-fmt.log` | PASS |
| A3-C2 | `cargo test --locked -p storage-sqlite -p app --all-features` | 0 | `reports/tp2-tester-a3-test.log` | PASS |
| A3-C3 | `cargo test … --test session_resume_e2e an_unauthorized_resume_… -- --nocapture`（CR8-S1 临时探针，**跑完即回滚**，非交付内容） | 0 | `reports/tp2-tester-a3-s1-probe.log` | PASS |

- **A3-C2 与上一轮逐项一致**：`app/session_resume_e2e` **8 passed**、
  `storage-sqlite/resume_columns` **6 passed**；其余目标（`audit_export` 1、`cli_commands` 11、
  `daemon_lifecycle` 12、`node_link_e2e` 3、`node_link_listener` 8、`node_pair_export_ids` 2、
  `admin_audit` 15、`admin_store` 43、`attachments` 5、`commit` 18+1 ignored、
  `compaction_recovery` 3、`contract_v03` 5、`enum_coverage` 2、`imported` 10、`migration` 12+1 ignored、
  `permissions` 4、`retention` 8、`session_version_rule` 3、lib 57）**计数全部不变**，
  **0 failed / 0 新增 ignored**。与 `reports/tp2-tester-a2-test-app.log` +
  `tp2-tester-a2-test-storage-sqlite.log` 的逐行计数比对通过。

## 10.7 边界声明（第 3 轮）

- **产品代码零改动**：`git diff --name-only f44301e..3484541` 仅 3 个 `crates/**/tests/**` 文件；
  `crates/**/src/**` 零改动。
- **清单零改动**：`crates/app/Cargo.toml` 与 `Cargo.lock` 本轮**零改动**（本轮**未再动**清单）。
- `schemas/`、`fixtures/`、`compatibility/`、`docs/**` 零改动；未新增/删除用例；未弱化任何断言。
- 未 push、未动 `refs/heads/main`、未归档、未改契约资产。
- 本轮是**记录层修正轮**：不宣称 TP2 的需求覆盖或验收结论有任何变化；CR8 的 PASS 判定不变，
  CR8-F6 的 PENDING 状态不变。
- CR8 未提出的问题由本轮发现：无。

## 10.8 handoff_index（第 3 轮）

```yaml
handoff_index:
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: 3
    stage: work-package
    target_revision: "3484541"
    evidence_type: CHECK
    evidence_id: A3-C1
    report_path: reports/tp2-tester-a3-fmt.log
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo fmt --all -- --check，退出码 0；本轮只改注释与 rustdoc，注释同样受 fmt 管辖，故仍需过门禁"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: 3
    stage: work-package
    target_revision: "3484541"
    evidence_type: CHECK
    evidence_id: A3-C2
    report_path: reports/tp2-tester-a3-test.log
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo test --locked -p storage-sqlite -p app --all-features，退出码 0；session_resume_e2e 8 passed / resume_columns 6 passed / 0 failed，其余目标计数与上一轮（f44301e）逐项一致；本轮为纯注释修正，可观察行为不变"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: 3
    stage: work-package
    target_revision: "3484541"
    evidence_type: CHECK
    evidence_id: A3-C3
    report_path: reports/tp2-tester-a3-s1-probe.log
    result: PASS
    evidence_status: NEW
    applicability_basis: "CR8-S1 的实测：临时插入 eprintln! 探针（只打印、不改断言）单跑 R31 用例，测得两侧 error.details 均为 {}（空对象，非 null）、is_object 均为 true；跑完立即回滚，探针不在交付内容中（git diff f44301e..3484541 中无该行）"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: 3
    stage: work-package
    target_revision: "3484541"
    evidence_type: CHECK
    evidence_id: CR8-F6-UNIX-VARIANTS
    report_path: reports/tp2-tester.md
    result: NOT_EXECUTED
    evidence_status: PENDING
    applicability_basis: "两条 #[cfg(unix)] 用例（符号链接改指、权限丢失）在本轮结束时仍无本机编译证据与执行证据（本机 Windows 被 cfg 剔除；交叉 x86_64-unknown-linux-gnu 缺 x86_64-linux-gnu-gcc）。本轮未改动其断言/cfg/行为，未用 #[ignore] 或 #[cfg(not(unix))] 包裹或删除，未改写任何措辞。关闭条件：Linux CI checks job（ubuntu-latest，非 root）首次编译并执行；失败则开新问题编号回 TP2，不改判本轮结论"
    source_evidence: NOT_APPLICABLE
```