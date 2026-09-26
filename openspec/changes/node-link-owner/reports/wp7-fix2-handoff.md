# WP7 fix2 Handoff — RV2-WP7 的 F1/F2/F3（`node-link-owner` / WP7 工作包交付前修复）

## Shared Report

- **task_id**: `RV2-WP7-F1`（被放弃 turn 的迟到终态被记到下一个 turn 上，**P1**，主 Agent 裁决按 reviewer 建议 (a) 修）、`RV2-WP7-F2`（测试替身注释与修复后语义不符，P2）、`RV2-WP7-F3`（§6 第 19 条「无归属降级」与第 9 条「归属并丢弃」对同一事件类给不同后果，P2）——对应 review 报告 `reports/rv2-wp7.md` 的 Findings。
- **role**: coder；**phase**: fix；**stage**: work-package；**evidence_id**: `RV2-WP7`
- **agent_context**: 任务级 coder 子 Agent（WP7 修复轮次 fix2），**不继承** WP7 实现或 review 会话，只按主 Agent 的 fix 清单与 `rv2-wp7.md` 原文作业。工作目录 `D:\Project\acp-remote-wt\node-link-owner`（worktree，分支 `agentic/node-link-owner`）；权威规划根 `D:\Project\acp-remote`（handoff 与日志写在该处 `openspec/changes/node-link-owner/reports/`，该目录不入版本控制）。工具集含 shell 与 Git：本轮 diff、门禁与红向证据均由本 Agent 亲自执行。
- **base / target_revision**: base = `690b91722f6ef1306660fde55ce1afd0d4607f4b`（RV2-WP7 的 Target Revision；开工时 HEAD 与之一致、`git status --porcelain` 为空）；**target = `5c869160e8dab0361b248124f631beba239183c6`**（本轮两个提交后的 HEAD；`git status --porcelain` 为空、无 staged 文件、分支未 push）。
- **scope**（严格按派单允许写入面）：
  - 批次 1（F1/F3）：`crates/core/src/broker.rs`、`docs/CORE_PORTS_AND_STORAGE.md`（§6 第 9/19 条）。
  - 批次 2（F2）：`crates/app/tests/support/owner.rs`（只改注释）。
  - **未改**：`crates/app/src/**`、`crates/server/**`（无需连带，零编译改动）、`crates/app/tests/node_link_e2e.rs`（R61 断言在 fix1 已按新语义改钉，本轮未再动）、`Cargo.toml`/`Cargo.lock`（零新依赖）、`schemas/**`、`compatibility/**`、`fixtures/**`、`identity-*`、`agent-host`、`storage-sqlite`、`openspec/**`（proposal/specs/design/plan/tasks/verification）。
- **changes**（2 个提交，3 个文件，+319/−5）：

  1. **F1（P1，按建议 (a) 修）**：`commit_chunk` 的 `Err(PortError::Unavailable(_))` → `abandon_failed_turn` 原本在终结该 turn 时调用 `slot.finish_turn()`（清空 `running`），于是同一次 `pump_locked` 紧接着的 `dispatch_one` 就把下一个排队 turn 提升为「在跑的 turn」。而 `agent-host` 的 mapper 给**所有** `EndpointEvent` 都写 `turn: None`（`crates/agent-host/src/mapper.rs::view_event`），归属只能按「有在跑 turn 时归给它」推断（§10.3）——因此被放弃 turn 的迟到 `turn.completed` 会被记到下一个 turn 上：客户端看到**下一个命令 `completed`**，而它的正文被按上一个 turn 的收尾折叠（实测见红向证据·症状复现）。
     - `TurnQueue` 新增 `held: Option<HeldTurn>`；`abandon_failed_turn` 不再 `finish_turn()`，改为 `slot.hold_abandoned_turn(turn)`——**继续占住 `running` 槽位**，占住期间它的迟到事件（含终态）一律按它归属并丢弃（已有的 `abandoned.contains` 分支）。
     - `dispatch_one` 首部新增 `slot.hold_blocks_dispatch()`：占位未释放就 `Ok(false)`，**不**提升下一个排队 turn（该函数是 `waiting.pop_front` 的唯一调用点）。
     - **释放路径①（正常）**：该 turn 的终态事件到达时（`commit_chunk` 的丢弃分支里判 `is_turn_terminal`）调用 `slot.release_held_turn(turn)`——事件本身仍被丢弃，但视为「端点已观测到该 turn 结束」，会话槽重新可用（同一个 `pump_locked` 循环随后派发下一个排队 turn）。`release_held_turn` 只接受占位者本人的终态（更早被放弃 turn 的迟到终态不得提前放行）。
     - **释放路径②（兜底）**：core 不读时钟、不设定时器（`broker.rs` 模块头的既有约束），因此兜底按**驱动轮次**而不是墙钟计时：常量 `ABANDONED_TURN_HOLD_ROUNDS = 1200`（每轮 = 一次派发尝试；组合根按 `storage.flush_interval_ms`（默认 250 ms）周期驱动活动会话 ≈ 5 分钟），轮次用尽仍未观测到终态就释放占位（理由：端点确实不再收敛时优先让会话继续可用）。
     - **会话状态投影**：放弃提交原来无条件把会话写成 `SessionState::Failed`，而组合根的合并窗口只枚举 `queued`/`running`/`waiting_*` 的会话（`crates/app/src/daemon.rs::MERGE_WINDOW_STATES`）——只要仍有排队 turn，写成 `failed` 会让该会话**再也不被驱动**，兜底轮次永远推不动（兜底等于不存在）。因此改为：仍有排队 turn → `Queued`，否则沿用 `Failed`。
     - §6 第 9 条新增三条子项登记：占位语义与理由、两条释放路径（含常量）、会话状态投影；并写明占住期间**被放弃 turn 的 permission/elicitation 请求一并丢弃**（不落库、不写交互行、不广播；收敛该 turn 是端点的职责）。
  2. **F2（P2）**：`app/tests/support/owner.rs::fail_commits_with` 的注释原写「其余提交（turn 记账、终态）照常」，与 §6 第 9 条的现状不符。改为：失败只发生在带该 marker 的那一批提交上；该批属于**在跑的** turn，因此这个 turn 随后被放弃——它的终态批与后续事件都不再提交（命令以 `uncertain` 收尾），其余 turn 的事件照常提交。行为零改动。
  3. **F3（P2）**：`docs/CORE_PORTS_AND_STORAGE.md` §6 第 19 条的「无归属的降级」补**例外**：被放弃的 turn（第 9 条）的迟到事件按第 9 条归属到它并被丢弃，**不走**本条降级；并点明第 9 条对同一事件类给出的是**有意更强**的处置（丢弃 > 无归属落库），两条的适用边界是「有没有可归属的被放弃 turn」。第 9 条对应的子项同步点明这句对比。另补一条用例固定该边界（见下）。

- **checks**（原始输出见 `reports/wp7-integration.log` 的「wp7-fix2 轮次」分节）：
  - `cargo fmt --all -- --check` exit=0；`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` exit=0。
  - `cargo test --locked --workspace --all-features` exit=0（全部 test binary ok；core lib **118** passed，base 为 115，本轮 **+3**；其余 binary 计数与 base 逐条相同，含 2 处 `1 ignored` 不变）。
  - `npm run check` exit=0（10 道门禁；含 `check:docs`、`check:boundaries`、`check:drift`、`check:agentic`）。
  - 提交前钩子（`.husky/pre-commit`）两次提交均通过（fmt → `npm run check` → workspace clippy），commitlint 通过（本地未装 gitleaks，CI 的 `secrets` job 仍判定）。
  - **红向证据（F1，改前实现 690b917 上先写用例）**：两条新用例在修复前都失败——① `a_late_terminal_of_an_abandoned_turn_does_not_complete_the_next_turn` 在 `assert_eq!(prompt_count(), 1, "不得提前派发 T2")` 处 `left: 2 / right: 1`（放弃后立刻派发了 T2）；② `an_unobserved_abandoned_turn_hold_is_released_after_the_bounded_rounds` 在「轮次未用尽前不得派发 T2」处同样 `left: 2 / right: 1`。修复后两条均绿。
  - **红向证据（F1·症状复现）**：同一份 pre-fix 实现下，临时去掉 ① 断言直接暴露归属错误——`assert_eq!(command(request(2)).status(), Accepted)` 得到 `left: Completed / right: Accepted`，即 T1 的迟到 `turn.completed` 把 **T2 的命令报成了 completed**（P1 的原症状）。
  - **F3 用例的性质**：`a_late_event_of_an_abandoned_turn_is_dropped_instead_of_degraded_without_a_turn` 在修复前后都绿——它固定的是「可归属时丢弃 vs 无归属落库」的边界（含会话级事件的阳性对照），**不是**本轮 P1 的判别器；这一点由上面两条红向证据承担。
  - **旁证（非正式轮次）**：`cargo test --locked -p app --all-features --test node_link_e2e -- --test-threads=1` exit=0（2 passed，5.32s）——本轮的会话槽占位改动没有破坏受控路径全链路（R61 的故障注入场景）。该命令只是 coder 侧 smoke，**不**构成 `[PV5]` 的正式证据。
  - **未执行（如实记录）**：`cargo-deny`、`gitleaks` 只在 CI 运行、本地无等价物；本角色未执行 `[PV4]`/正式 `[PV5]`/候选门禁/E2E。
- **issues**（需要主 Agent / reviewer 知晓的四条，均**未擅自扩大范围**）：
  1. **兜底释放后的残余窗口（登记在案，修复本身无法消除）**：占位被兜底（路径②）释放后，会话槽重新可用，此时**无 turn 标识**的迟到事件按既有规则归给「在跑的 turn」（§10.3/§6 第 19 条）——即 T1 的尾巴若在兜底之后才到达、且它不带 turn 标识，仍可能被记到 T2 上。这是「提前派发」与「会话永久卡住」之间的取舍：不引入适配器侧的权威 turn 标识就无法同时消除两者。已写进 §6 第 9 条子项（「兜底是最后手段」），并希望 reviewer 确认该取舍可接受。
  2. **兜底按驱动轮次而非墙钟计时**：core 既有的「不读时钟、不设定时器」约束 + `Timestamp` 无算术 + core 依赖闭包冻结（§9 判据 13：不允许引入时间库）三者叠加，使 core 内无法计算「距占位建立已过 N 秒」。因此常量语义是「最多消耗 1200 个驱动轮次」，其换算（≈5 分钟）依赖组合根的驱动间隔；若 review 认为需要真正的墙钟上界，最小改动路径是给 `Clock` 端口加一个「按 Timestamp 计算间隔」的方法（端口变更 → 需同步 §5 与 drift 门禁，超出本派单范围）。
  3. **「会话关闭」这条兜底在本切片不可达**：core 目前没有会话关闭/端点关闭的入口（`SessionUpdate.closed_at` 无人写入、`SessionEndpoint::close` 无 broker 调用点），因此 reviewer 列出的三条兜底里落地的是「迟到终态观测」+「有界轮次」两条；会话关闭路径要等相应用例面落地后再接。
  4. **`[PV5]` 与候选门禁**：本轮未改 `node_link_e2e.rs` 的断言，但改了 core 的会话槽/状态语义（占位 + `Queued` 投影），因此 `rv2-wp7.md` 里 PENDING 的 `[PV5]` 仍须在候选 revision 上以约定形式重跑（本 Agent 的 smoke 只有 2 passed，不代替正式轮次）。
- **result**: **PASS**（本轮 fix 范围内全部适用条件满足、门禁全绿；**不代表** RV2-WP7 的独立复核、`[PV5]` 重跑、候选门禁、E2E 或合并已完成）
- **evidence_paths**:
  - 本报告：`openspec/changes/node-link-owner/reports/wp7-fix2-handoff.md`
  - 被处理的 review：`openspec/changes/node-link-owner/reports/rv2-wp7.md`
  - 本轮原始输出（红向证据 ×2、定向用例、全量门禁、smoke）：`openspec/changes/node-link-owner/reports/wp7-integration.log` 的「wp7-fix2 轮次」分节
  - 上一轮证据（被本轮修正的注释与断言所在）：`reports/wp7-fix1-handoff.md`、`reports/wp7-integration.log` 的「wp7-fix1 轮次」
- **resource_cleanup**: 未新增端口、数据库、容器、证书或外部账号；未安装依赖、未改 `Cargo.toml`/`Cargo.lock`/任何 schema 与 compatibility 资产。红向证据用的临时文件只落在系统临时目录（`/tmp/broker_head.rs`、`/tmp/broker_prefix.rs`、`/tmp/broker_prefix_attrib.rs`、`/tmp/broker_fixed*.rs`、`/tmp/keep_fixed.rs`，非仓库内），用完即删；两个提交后 `git status --porcelain` 为空，无未跟踪文件、无 staged 文件。

## 提交（供 reviewer 逐条核对）

| SHA | 标题 | 文件 |
|---|---|---|
| `008dcef5fedab4e2d82efce71f24053919f4cb50` | `fix(core): 被放弃的 turn 继续占住会话槽直到终态被端点观测到` | `crates/core/src/broker.rs`、`docs/CORE_PORTS_AND_STORAGE.md`（2 files, +316/−4） |
| `5c869160e8dab0361b248124f631beba239183c6` | `test(app): 修正故障注入替身的注释口径` | `crates/app/tests/support/owner.rs`（1 file, +3/−1） |

（两个提交都经 `.husky/pre-commit` 与 commitlint；提交后 `git status --porcelain` 为空。分支 `agentic/node-link-owner` 未 push。）

## 各条落点（reviewer 复检索引）

| ID | 修复落点 | 用例 / 证据 |
|---|---|---|
| RV2-WP7-F1 | `broker.rs`：新 `HeldTurn` + `TurnQueue.held`；`Slot::hold_abandoned_turn` / `release_held_turn` / `hold_blocks_dispatch` / `has_waiting_turn`；`abandon_failed_turn`（`finish_turn()` → `hold_abandoned_turn()`、会话状态按 `has_waiting_turn()` 投影）；`commit_chunk` 丢弃分支（终态 → 释放占位）；`dispatch_one` 首部（占位期不派发）；常量 `ABANDONED_TURN_HOLD_ROUNDS`；`docs/CORE_PORTS_AND_STORAGE.md` §6 第 9 条三条子项 | `broker::tests::a_late_terminal_of_an_abandoned_turn_does_not_complete_the_next_turn`（① 不得提前派发 T2、会话状态 `Queued`；② 无归属迟到终态按 T1 归属并丢弃；③ T2 命令仍 `Accepted`、无 `turn.completed`/`command.completed`、事件序列钉死、T2 自己的增量落盘且终态为 `Running`）、`broker::tests::an_unobserved_abandoned_turn_hold_is_released_after_the_bounded_rounds`（轮次用尽前不派发、用尽后必须派发）；红向：两条用例在 690b917 上失败，另附症状复现（T2 被报 `Completed`） |
| RV2-WP7-F2 | `crates/app/tests/support/owner.rs::fail_commits_with` 的文档注释（第 275–278 行区域） | 注释口径与 §6 第 9 条一致（失败批所属 turn 随后被放弃，其终态批与后续事件不再提交）；行为零改动，`node_link_e2e` 两个用例全绿 |
| RV2-WP7-F3 | `docs/CORE_PORTS_AND_STORAGE.md` §6 第 19 条「无归属的降级」补例外与交叉引用；§6 第 9 条子项点明「有意更强的处置」 | `broker::tests::a_late_event_of_an_abandoned_turn_is_dropped_instead_of_degraded_without_a_turn`（放弃场景的迟到增量被丢弃；阳性对照：同一时刻的会话级事件照常落库且 `turn` 归属为 NULL）；与既有用例 `a_late_delta_after_turn_end_is_persisted_without_attribution`（无被放弃 turn 时的降级）互为对照 |

## 决策与边界（供 reviewer 判 applicability）

1. **F1 按 reviewer 建议 (a)，未做更大改动**：不引入适配器侧权威 turn 标识（跨 crate 选择）、不改 `EndpointEvent` 形状、不改 `SessionEndpoint` 端口签名、不给 core 加日志依赖；只让被放弃的 turn 占住会话槽。
2. **终态「被观测到」的判据是「该 turn 的终态事件到达」**，不是「命令终态落盘」：事件本身仍被丢弃（§6 第 9 条不让它落库），释放动作与丢弃动作在同一步完成。
3. **兜底的量纲是驱动轮次**（见 issues 第 2 条）：常量值 1200 是本 Agent 按默认 `flush_interval_ms = 250` 折算出的 ≈5 分钟，**未**接入 `CONFIG_REFERENCE.md`（该文件不在本派单允许写入面）；若主 Agent 希望它成为可配置键，需要配置文档 + `BrokerConfig` 字段的同步改动。
4. **会话状态投影改为 `Queued`（仍有排队 turn 时）**：这是让兜底可达的必要条件，不是新决策——§6.4 的排队语义本就要求「会话有排队 turn」是可被驱动的状态；本 Agent 未改 `MERGE_WINDOW_STATES`（`crates/app/src/**` 不在允许写入面）。
5. **未改 imported 路径**：`deliver_imported`/`commit_receipt` 无 turn 队列与占位语义，未受影响。
6. **未改 `node_link_e2e.rs`**：fix1 已按新语义改钉 R61，本轮无需要连带；其 smoke 通过。

## handoff_index

```yaml
handoff_index:
  - task_id: "RV2-WP7-F1"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "5c869160e8dab0361b248124f631beba239183c6"
    evidence_type: CHECK
    evidence_id: RV2-WP7
    report_path: "reports/wp7-fix2-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "被放弃 turn 保留 running 占位（HeldTurn），dispatch_one 在占位期不派发下一个排队 turn；其终态到达（丢弃该事件）即释放，兜底为 1200 个驱动轮次的占位（ABANDONED_TURN_HOLD_ROUNDS）；仍有排队 turn 时放弃提交把会话状态投影为 Queued 以保证合并窗口仍驱动该会话；新增 2 条用例在 690b917 上失败（不得提前派发 T2；兜底轮次），修复后绿；另附症状复现（pre-fix 下 T2 的命令被报成 Completed）"
    source_evidence: { id: "RV2-WP7", report_path: "reports/rv2-wp7.md", target_revision: "690b91722f6ef1306660fde55ce1afd0d4607f4b" }
  - task_id: "RV2-WP7-F2"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "5c869160e8dab0361b248124f631beba239183c6"
    evidence_type: CHECK
    evidence_id: RV2-WP7
    report_path: "reports/wp7-fix2-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "app/tests/support/owner.rs 的 fail_commits_with 注释改为与实际语义一致（失败只命中带 marker 的那一批；该批所属在跑 turn 随后被放弃，其终态批与后续事件不再提交，命令以 uncertain 收尾）；零行为改动，工作区全量门禁 exit=0"
    source_evidence: { id: "RV2-WP7", report_path: "reports/rv2-wp7.md", target_revision: "690b91722f6ef1306660fde55ce1afd0d4607f4b" }
  - task_id: "RV2-WP7-F3"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "5c869160e8dab0361b248124f631beba239183c6"
    evidence_type: CHECK
    evidence_id: RV2-WP7
    report_path: "reports/wp7-fix2-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "§6 第 19 条「无归属的降级」补例外与交叉引用（被放弃 turn 的迟到事件按第 9 条归属并丢弃，不走降级；第 9 条是有意更强的处置）；第 9 条子项同步点明；新增用例 a_late_event_of_an_abandoned_turn_is_dropped_instead_of_degraded_without_a_turn 固定该边界（阳性对照：会话级事件照常落库且 turn 归属为 NULL）"
    source_evidence: { id: "RV2-WP7", report_path: "reports/rv2-wp7.md", target_revision: "690b91722f6ef1306660fde55ce1afd0d4607f4b" }
  - task_id: "RV2-WP7"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "5c869160e8dab0361b248124f631beba239183c6"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: "reports/wp7-integration.log"
    result: PENDING
    evidence_status: STALE
    applicability_basis: "本轮改了 core 的会话槽/会话状态语义（占位 + Queued 投影），rv2-wp7.md 记录的 PENDING [PV5] 仍须以约定形式在候选 revision 上重跑：cargo test --locked -p app --all-features --test node_link_e2e -- --test-threads=1；coder 侧 smoke（2 passed）不代替正式轮次"
    source_evidence: { id: "PV5", report_path: "reports/wp7-integration.log", target_revision: "a3109c8a80ec806cd7899b72709ec07e579cec28" }
```
