# WP7 fix1 Handoff — RV1-WP7 的 F1/F2/F3/F4（`node-link-owner` / WP7 app 接线与全链路集成）

## Shared Report

- **task_id**: `RV1-WP7-F1`（unwired 收敛用例 vacuous，MINOR）、`RV1-WP7-F2`（网络停 accept 的触发时机与日志时机，MINOR）、`RV1-WP7-F3`（非终态批次写失败被静默丢弃，MINOR，主 Agent 裁决选 (a) 修）、`RV1-WP7-F4`（测试 harness 复刻私有常量，SUGGESTION）——对应 review 报告 `reports/rv1-wp7.md` 的 Findings。
- **role**: coder；**phase**: fix；**stage**: work-package；**evidence_id**: `RV1-WP7`
- **agent_context**: 任务级 coder 子 Agent（WP7 修复轮次 fix1），**不继承** WP7 实现或 review 会话，只按主 Agent 的 fix 清单与 `rv1-wp7.md` 原文作业。工作目录 `D:\Project\acp-remote-wt\node-link-owner`（worktree，分支 `agentic/node-link-owner`）；权威规划根 `D:\Project\acp-remote`（handoff 与日志写在该处 `openspec/changes/node-link-owner/reports/`，该目录不入版本控制）。工具集含 shell 与 Git：本轮 diff、门禁与红向证据均由本 Agent 亲自执行。
- **base / target_revision**: base = `a3109c8a80ec806cd7899b72709ec07e579cec28`（RV1-WP7 的 Target Revision；开工时 HEAD 与之一致、`git status --porcelain` 为空）；**target = `690b91722f6ef1306660fde55ce1afd0d4607f4b`**（本轮两个提交后的 HEAD；`git status --porcelain` 为空、无 staged 文件）。
- **scope**（严格按派单允许写入面）：
  - 批次 1（F3）：`crates/core/src/broker.rs`、`docs/CORE_PORTS_AND_STORAGE.md`（§6 第 9 条同步）、`crates/app/tests/node_link_e2e.rs`（原有 R61 断言钉的就是该残余，必须同步改钉新语义）；
  - 批次 2（F1/F2/F4）：`crates/app/src/daemon.rs`、`crates/app/tests/daemon_lifecycle.rs`、`crates/app/tests/node_link_listener.rs`、`crates/app/tests/support/mod.rs`、`crates/app/tests/support/owner.rs`。
  - **未改**：`crates/server/**`（F2 无需连带，`server::transport::net` 的关闭信号本来就有同步 `trigger()`）、`Cargo.toml`/`Cargo.lock`（零新依赖）、`schemas/**`、`compatibility/**`、`fixtures/**`、其它协议 crate、`openspec/**`（proposal/specs/design/plan/tasks/verification）。
- **changes**（2 个提交，8 个文件，+461/−26）：

  1. **F3（MINOR，裁决 (a) 修）**：`core::broker::commit_chunk` 对**非终态**批次的 `PortError::Unavailable` 原本只做「不发布」就 `Ok(())` 返回，于是同一个 turn 的终态批随后照常落成 `turn.completed` + `command.completed`——命令报完成而正文缺了一块（与 §6 第 9 条字面冲突）。
     - 失败批次里**有**在跑 turn 的事件（`running_in_chunk`）时，新增 `Broker::abandon_failed_turn`：一次提交把该 turn 终结为 `turn.failed`、把命令置为 `uncertain`，并把该 turn **已落盘** delta 的 `agent.message.completed`（§6 第 14 条）与它们同批提交（缺的段落不伪造）。
     - `Slot` 新增 `abandoned: Mutex<Vec<TurnId>>`；`commit_chunk` 的事件循环先按归属丢弃被放弃 turn 的任何事件（含终态）。归属规则保持既有形状（`event.turn` → 在跑 turn 兜底），只在「无在跑 turn 且事件类型属于 §10.3 需要 `turnId` 的集合」时把兜底换成**最后被放弃的 turn**——因为 `agent-host` 的 mapper 给**所有**事件都写 `turn: None`，不兜底的话迟到的 `turn.completed` 会因为无归属而被提交。
     - 可观测信号 = 这次提交里的两条持久事件（core 不含日志依赖，`AGENTS.md` §7）；**不**向调用方冒泡（与「带终态的批次失败 → `uncertain`」的既有口径一致，也避免把「已经终结的 turn」报成另一个命令的失败）。失败批次没有在跑 turn 可归属时（会话级事件）仍然只有「不发布」这一个效果。
     - `docs/CORE_PORTS_AND_STORAGE.md` §6 第 9 条补三条子项把上述口径写进合同（实现本就该如此，是文档/实现收敛，不是新决策）。
  2. **F2（MINOR）**：`NetIngress::stop(async fn)` 拆成同步 `trigger_shutdown()` + 异步 `wait_stopped(deadline)`；`close()` 现在**先同步触发**（listener 立即停 accept），再记 `daemon.ingress_stopped`，再排空本地在途连接，最后 `wait_stopped`（超时 abort）。修复前 `net_ingress.stop(deadline)` 只是构造 future、触发要等到本地排空之后的 `.await`，因此整个本地排空窗口里网络 listener 仍在 accept，而日志已声明「已停止接受新连接」。
  3. **F1（MINOR）**：`node_link_listener.rs` 的 unwired 收敛用例改在 `logging.level = "debug"` 下运行，并加**阳性对照**（同一构造器、同一级别，`daemon.instance_lock = "ipc"` 断言 `daemon.config_unwired` 的明细真的出现）。测试 harness `support/mod.rs` 的 `ConfigParts` 增加 `logging_level` 覆盖（原来把 `[logging] level = "info"` 写死，且 `extra` 里再写 `[logging]` 会被 TOML 判成重复键），新增构造器 `configure_with_dev_mode_and_log_level`。
  4. **F4（SUGGESTION）**：`app/tests/support/owner.rs` 不再复刻私有常量 `NODE_KEY_LABEL`/`KeyHandle::new(format!("{}/primary", …))`，改为 `identity.key().clone()`（`app::NodeIdentity::key()` 是条目标签的唯一来源），并去掉随之无用的 `KeyHandle`/`KeyPurpose` import。

- **checks**（原始输出见 `reports/wp7-integration.log` 的「wp7-fix1 轮次」分节）：
  - `cargo fmt --all -- --check` exit=0；`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` exit=0。
  - `cargo test --locked --workspace --all-features` exit=0（全部 test binary ok；core lib **115** passed，base 为 113，本轮 **+2**；app 侧 `node_link_listener` 8、`daemon_lifecycle` **11**（base 10，+1）、`node_link_e2e` 2）。
  - `npm run check` exit=0（含 `check:docs`、`check:boundaries`、`check:drift`、`check:agentic`；合同漂移门禁不受影响——它只读 §5 端口与 §7 DDL）。
  - 提交前钩子（`.husky/pre-commit`）两步提交均通过（fmt → `npm run check` → workspace clippy），commitlint 通过（本地未装 gitleaks，CI 的 `secrets` job 仍判定）。
  - **红向证据（F3，改前实现 a3109c8 上先写用例后实现）**：两条新用例在修复前都失败，且失败信息正是被登记的缺陷——①「正文缺块的 turn 不得报完成」`left: Completed / right: Uncertain`（命令真的被报成 `completed`）；②事件序列 `left: [turn.queued, turn.started, agent.message.delta, turn.completed, agent.message.completed, command.completed, turn.delta_compacted]`（迟到终态照常落盘且命令完成）。修复后两条均绿。
  - **红向证据（F2，临时把 `crates/app/src/daemon.rs` 回退到 a3109c8）**：新用例 `the_network_listener_stops_serving_new_connections_before_the_local_drain_finishes` 在 0.13s 内失败并指出「停接入层之后不得再服务新连接」——证明判据能区分两种实现（不是靠超时侥幸通过）。恢复后复跑绿；F2 用例连跑 3 次 2.16s/2.16s/2.17s、F1 用例连跑 2 次 0.25s/0.25s，无抖动。
  - **F1 非空转的旁证**：用真实二进制按 debug 级手工跑了一次同形状配置（无 `daemon.instance_lock`）——`daemon.config_unwired` 事件数为 **0**（七个键确实都已接线、清单也确实为空），说明收敛断言判的是「这些键不再上报」而不是「清单读不到」。
  - **未执行（如实记录）**：`cargo-deny`、`gitleaks` 只在 CI 运行、本地无等价物；本角色未执行 `[PV4]`/`[PV5]`/候选门禁/E2E（不由本角色执行）。
- **issues**: 四条 Findings 全部落地，无阻断。需要主 Agent / reviewer 知晓的三条**未擅自扩大的**事项：
  1. **`[PV5]` 含被改动的断言**：`node_link_e2e.rs` 的 R61 原来把 F3 的缺陷当成「登记在案的已知残余（不修）」钉住（断言 `turn.completed` 到达且 `command.terminal.status = completed`），本轮按裁决 (a) 改钉新语义（faulted turn 不再出现 `turn.completed`，终态为 `uncertain` + `nodelink.command.uncertain`）。因此 **`[PV5]` 必须重跑**，其 `REUSED` 状态不再适用。
  2. **F2 判据的形态**：派单写的是「`daemon.stop` 返回 accepted 后再连监听地址被**拒绝**」，但 TCP 层拒绝不是有效判据——accept 循环停下后监听套接字仍被 `axum::serve` 的在途排空持有，内核 backlog 会照常完成握手（实测：修复前 connect 一路成功，直到进程退出才失败）。因此用例改判**应用层不再服务新连接**（同一时刻发一条最小 HTTP 请求，等不到任何响应），并用「进程仍在运行」+ 早于 `daemon.stopped` 排除「因为进程退出而拒绝」。若 review 认为必须钉 TCP 层，需要先在 `server::transport::net` 增加「accept 已停」的可观测点（超出本派单范围，未做）。
  3. **F3 的两条已知边界**（都写进了 §6 第 9 条的子项）：①被放弃 turn 已落盘的 delta **不压缩**（§6 第 15 条是可选动作，错误路径上不再多发一次提交）；②`agent-host` 给所有事件写 `turn: None`，因此**归属仍然来自「当时在跑/兜底的那个 turn」**——若被放弃 turn 的尾巴在下一个 turn 已经开始之后才到达，它会被记到下一个 turn 上（这是既有的归属口径缺陷，不由本轮引入；彻底修需要适配器给出权威 turn 标识，属跨 crate 选择）。
- **result**: **PASS**（本轮 fix 范围内全部适用条件满足、门禁全绿；**不代表** RV2-WP7 独立复核、`[PV5]` 重跑、候选门禁、E2E 或合并已完成）
- **evidence_paths**:
  - 本报告：`openspec/changes/node-link-owner/reports/wp7-fix1-handoff.md`
  - 被处理的 review：`openspec/changes/node-link-owner/reports/rv1-wp7.md`
  - 本轮原始输出（含 F3/F2 红向证据、定向用例、全量门禁）：`openspec/changes/node-link-owner/reports/wp7-integration.log` 的「wp7-fix1 轮次」
  - 上一轮证据（被本轮改动的 R61 断言所在）：`reports/wp7-handoff.md`、`wp7-app-wiring.log`、`wp7-workspace.log`
- **resource_cleanup**: 未新增端口、数据库、容器、证书或外部账号；未安装依赖、未改 `Cargo.toml`/`Cargo.lock`/任何 schema 与 compatibility 资产。临时文件只写在系统临时目录（`/tmp/wp7fix1/**`、`/tmp/daemon.rs.fixed`，非仓库内）；两个提交后 `git status --porcelain` 为空，无未跟踪文件、无 staged 文件。测试用的临时配置/日志（`/tmp/wp7f1/**`）非仓库内，不影响工作区。

## 提交（供 reviewer 逐条核对）

| SHA | 标题 | 文件 |
|---|---|---|
| `ee5f63199a31d99fb04b5df45ce675fe00af151f` | `fix(core): 非终态批次落盘失败时终结 turn，不再留下「报完成但正文缺失」` | `crates/core/src/broker.rs`、`crates/app/tests/node_link_e2e.rs`、`docs/CORE_PORTS_AND_STORAGE.md`（3 files, +288/−13） |
| `690b91722f6ef1306660fde55ce1afd0d4607f4b` | `fix(app): 关闭时先同步停网络 accept，并修掉两处测试口径` | `crates/app/src/daemon.rs`、`crates/app/tests/daemon_lifecycle.rs`、`crates/app/tests/node_link_listener.rs`、`crates/app/tests/support/mod.rs`、`crates/app/tests/support/owner.rs`（5 files, +173/−13） |

（两个提交都经 `.husky/pre-commit` 与 commitlint；提交后 `git status --porcelain` 为空。分支 `agentic/node-link-owner` 未 push。）

## 各条落点（reviewer 复检索引）

| ID | 修复落点 | 用例 / 证据 |
|---|---|---|
| RV1-WP7-F3 | `broker.rs::commit_chunk` 的 `Err(PortError::Unavailable(_))` 分支（`running_in_chunk` → `abandon_failed_turn`）、新 `Broker::abandon_failed_turn`、`Slot::abandoned` 与循环首部的归属丢弃；`docs/CORE_PORTS_AND_STORAGE.md` §6 第 9 条三条子项；`node_link_e2e.rs` ⑨ R61 断言 | `broker.rs` 新用例 `broker::tests::a_failed_non_terminal_batch_terminates_the_turn_instead_of_silently_dropping_it`（命令 `uncertain`、turn `Failed`、无 `turn.completed`/`command.completed`）、`broker::tests::an_abandoned_turn_still_finalises_the_deltas_it_persisted`（已落盘 delta 收尾成 `completed` 且不含丢失段落、迟到终态被丢弃）；红向：两条用例在 a3109c8 上失败（输出见日志分节） |
| RV1-WP7-F2 | `app/src/daemon.rs::NetIngress::trigger_shutdown` + `wait_stopped`、`close()` 步骤 1（同步触发 → 日志 → 本地排空 → `wait_stopped`） | `daemon_lifecycle.rs::the_network_listener_stops_serving_new_connections_before_the_local_drain_finishes`（+ `probe_http` 辅助）；红向：回退 `daemon.rs` 到 a3109c8 后 0.13s 失败 |
| RV1-WP7-F1 | `node_link_listener.rs::the_wired_listener_keys_are_no_longer_reported_as_unwired`（debug 级 + 阳性对照）、`support/mod.rs` 的 `logging_level` 与 `configure_with_dev_mode_and_log_level` | 同用例内的阳性对照断言（`daemon.instance_lock(ipc)` 的明细必须出现）+ 手工旁证（同形状配置在 debug 级下 `daemon.config_unwired` 计数为 0） |
| RV1-WP7-F4 | `app/tests/support/owner.rs` 的 `Authority::new` 调用（`identity.key().clone()`）与 import | `node_link_e2e` 两个用例（受控路径全链路 + TLS direct）全绿：`OwnerNode` 仍以同一 keystore 条目签名握手 |

## 决策与边界（供 reviewer 判 applicability）

1. **F3 选的是 (a) 修，按主 Agent 裁决**：不做「拒绝再消费该会话」这类更大的会话级失败模型，也没给 core 引入日志依赖；只让失败批次所属的在跑 turn 立即终结，并把「后续事件与终态不再提交」限定在该 turn 上。
2. **F3 的终态用 `uncertain` 而不是 `failed`（命令维度）**：命令已经派发给 Agent、副作用无法确认，这与 §6 第 16 条（启动恢复）和既有「终态批失败」分支同一口径；turn 维度仍是 `failed`。若 review 认为命令应记 `failed`，那是一处口径裁决，本 Agent 未擅自改。
3. **F3 不动 imported 路径**：`deliver_imported`/`commit_receipt` 的失败语义未变（无正文、无 turn 队列）。
4. **F1 的收敛断言仍保留原七键清单**（未改成「断言清单为空」）：清单为空是期望态，但按主 Agent 的 fix 清单，可观察性由阳性对照承担；两者合起来判的是同一件事。
5. **F2 只改 `app` 的接线顺序**，未改 `server::transport::net` 的任何行为（关闭信号本来就能同步触发；只是组合根此前没这么用）。

## handoff_index

```yaml
handoff_index:
  - task_id: "RV1-WP7-F1"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "690b91722f6ef1306660fde55ce1afd0d4607f4b"
    evidence_type: CHECK
    evidence_id: RV1-WP7
    report_path: "reports/wp7-fix1-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "unwired 收敛用例改在 logging.level=debug 下运行，并加阳性对照（同构造器/同级别 + daemon.instance_lock=ipc 断言 daemon.config_unwired 明细出现）；harness 新增 [logging] level 覆盖；手工旁证：同形状配置在 debug 级下 config_unwired 计数为 0；定向用例连跑 2 次 0.25s 均绿"
    source_evidence: NOT_APPLICABLE
  - task_id: "RV1-WP7-F2"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "690b91722f6ef1306660fde55ce1afd0d4607f4b"
    evidence_type: CHECK
    evidence_id: RV1-WP7
    report_path: "reports/wp7-fix1-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "NetIngress 拆成同步 trigger_shutdown + 异步 wait_stopped；close() 先同步触发（listener 停 accept）再记 daemon.ingress_stopped、再排空本地连接、最后 wait_stopped；新用例在本地排空窗口内断言监听地址不再服务新连接且进程仍存活；红向：回退 daemon.rs 到 a3109c8 后 0.13s 失败（判据非超时侥幸）"
    source_evidence: NOT_APPLICABLE
  - task_id: "RV1-WP7-F3"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "690b91722f6ef1306660fde55ce1afd0d4607f4b"
    evidence_type: CHECK
    evidence_id: RV1-WP7
    report_path: "reports/wp7-fix1-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "非终态批次 Unavailable 时 abandon_failed_turn 终结在跑 turn（turn.failed + command.uncertain，并同批提交已落盘 delta 的 agent.message.completed）；被放弃 turn 的后续事件（含终态）不再提交；core 无日志依赖，信号为两条持久事件；新增两条用例，二者在 a3109c8 上均失败（Completed vs Uncertain；迟到终态照常落盘），修复后绿；§6 第 9 条与 node_link_e2e 的 R61 断言同步"
    source_evidence: NOT_APPLICABLE
  - task_id: "RV1-WP7-F4"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "690b91722f6ef1306660fde55ce1afd0d4607f4b"
    evidence_type: CHECK
    evidence_id: RV1-WP7
    report_path: "reports/wp7-fix1-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "受控路径 harness 改用 NodeIdentity::key()（去掉复刻的 NODE_KEY_LABEL 与 KeyHandle::new），import 同步收敛；node_link_e2e 两个用例（含 TLS direct 轮次）全绿，证明同一 keystore 条目仍被 Authority 使用"
    source_evidence: NOT_APPLICABLE
  - task_id: "RV1-WP7"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "690b91722f6ef1306660fde55ce1afd0d4607f4b"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: "reports/wp7-integration.log"
    result: PENDING
    evidence_status: STALE
    applicability_basis: "本轮的 F3 修复改了 node_link_e2e.rs 的 R61 断言（原断言钉的是被裁决修掉的残余），因此上一轮 [PV5]（REUSED）不再适用，必须以新断言重跑：cargo test --locked -p app --all-features --test node_link_e2e -- --test-threads=1"
    source_evidence: { id: "PV5", report_path: "reports/wp7-integration.log", target_revision: "a3109c8a80ec806cd7899b72709ec07e579cec28" }
```
