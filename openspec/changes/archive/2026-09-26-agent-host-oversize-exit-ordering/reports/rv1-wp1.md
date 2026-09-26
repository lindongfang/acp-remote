## Review

I read the authoritative inputs and the code seam at the target revision before judging. Evidence base (all read-only): `openspec/changes/agent-host-oversize-exit-ordering/{proposal.md, design.md, plan.md, tasks.md, verification.md, specs/local-agent-host/spec.md, reports/wp1-handoff.md, reports/wp1-local-checks.log}`, `crates/agent-host/src/{process.rs, host.rs, platform.rs}`, `crates/agent-host/tests/supervision.rs`, `openspec/schemas/agentic/roles/handoff.md`.

---

# RV1 — WP1 独立只读 Review 报告

## Shared Report

- **task_id**: `3.2`（覆盖 `2.1`/`2.2` 交付物的 WP1 branch validation）
- **role**: reviewer
- **phase**: review
- **agent_context**: 独立只读子 Agent（fresh 上下文，未参与实现；run 产物 `…/outputs/8fa55311-…/reports/rv1-wp1.md`），只读文件工具，无 shell / 无 git 写、无构建或测试执行权；与实现 Agent 无私有对话继承。
- **target_revision**: `af64e85cc252515039b0da97dcc3b88dbde277dc`（完整 SHA，取自 `.git/logs/refs/heads/feat/agent-host-oversize-exit-ordering:3` 的真实 reflog 行：`998cb0f333e13953bc542e8c25efec242618dea9 → af64e85cc252515039b0da97dcc3b88dbde277dc commit: fix(agent-host): 超限结束时先标记退出再投递错误 [PRO-4]`；短 SHA `af64e85` 未做猜测）
- **base_revision**: `998cb0f333e13953bc542e8c25efec242618dea9`
- **launch_head**: `e544ac901aa938f79a42169b8ce19c8dafe5dca5`（`e544ac9` 的 reflog 行确认只登记 2.1/2.2 交接与勾选，与本轮代码检视目标一致）
- **scope**（实际检视）:
  - 变更面：`crates/agent-host/src/process.rs` 的 `abort_agent`（496–520）全文、其两个调用点（`read_loop` 470、`handle_line` 535）、`ExitState`（31–60）与 `mark` 唯一调用点集合（全仓仅 508、652）、`wait_loop`（636–653）、`is_running`/`has_exited`（202–210）及其全部消费方（`process.rs:103/249/281`、`host.rs:220/512/628`）、`Supervisor::shutdown`（358–397）与 `wait_exit`（338–352）。
  - 需求面：`proposal.md`（Why / Intent and Constraints / Capabilities / Impact）、增量规范 1 条 ADDED Requirement + 2 Scenario、`design.md` D1–D3、`plan.md` 的 Code Review 五关注点与 Coverage Index R1–R3、`tasks.md` 2.1/2.2/3.2。
  - 证据面：`reports/wp1-handoff.md` 的 Changes 摘要、D2 核实记录、局部检查表；`reports/wp1-local-checks.log` 原始输出（仅作核对，不作我的结论）。
  - 未检视（越界）：PV1/PV2 的命令执行与日志（主 Agent 并行执行，plan 归 3.1/6.3）；`openspec/` 权威文件的勾选与登记质量（主 Agent 职责）。
- **changes**: 我未做任何修改（本轮只读）。检视目标在 `af64e85` 的实际内容 = 「`abort_agent` 内 `exit.mark` 上移到 drain/send 之前 + 注释同步为三步顺序」，单函数内语句顺序调整，无语句增删、无签名/错误类型变化。
- **checks**: 未执行任何构建或测试（reviewer 无执行权）。静态核对项与结论见下「Findings」；需要 supervisor 执行的验证命令列在「Assessment」。
- **issues**: 未发现 CRITICAL / MAJOR。2 条 MINOR/SUGGESTION 级、非阻断（均为 `reports/wp1-handoff.md` 的行号精度问题 + 1 条 report-only 窗口分析）。
- **result**: PASS
- **evidence_paths**:
  - `openspec/changes/agent-host-oversize-exit-ordering/reports/rv1-wp1.md`（本报告）
  - 被核对对象：`reports/wp1-handoff.md`、`reports/wp1-local-checks.log`、`crates/agent-host/src/process.rs`、`crates/agent-host/src/host.rs`
- **resource_cleanup**: 未启动任何进程、未创建 worktree/容器/端口、未新增依赖、未写文件；`target/` 未被我触碰。

## Review Context

- 检视对象：WP1（DU1 唯一工作包）在 `af64e85` 的实现，判据来自 `plan.md` 的 Code Review 五关注点、`design.md` D1–D3 与增量规范。
- 本轮为 **work-package stage** 的 branch validation，只判定 WP1 交付物本身；**不**代表候选轮、主分支或最终验收结论。
- 用户意图（`proposal.md` Intent and Constraints）：以选项 A（根治顺序）为准，不改测试断言、不引入「有界等待」、不改签名/错误类型/wire 行为。
- 边界说明：reviewer 规则禁止 shell 与 git 访问，因此 `git diff --name-status`、`git diff 998cb0f..af64e85` 的逐字原文**未由我执行**；我用「目标修订的工作树内容 + 规划期（实现前）文档快照的行号锚点」做交叉核对（详见 Findings ③④）。

## Findings

### ① 新顺序 = 「`exit.mark` → drain 投递 → `tree.terminate()`」且与注释一致 — 通过（无发现）

- `crates/agent-host/src/process.rs:508` = `exit.mark(format!("ACP 消息超限（{actual} > {limit} 字节）"));`
- `crates/agent-host/src/process.rs:509-512` = `pending.lock().map(|mut map| map.drain().collect())`（一次性取整表）
- `crates/agent-host/src/process.rs:513-518` = 逐个 `sender.send(Err(HostError::Protocol(AcpError::Oversize { limit, actual })))`
- `crates/agent-host/src/process.rs:519` = `tree.terminate()`（函数体内唯一一条，无条件）
- 行号已被独立验证（非采信报告）：grep 实证 `496 fn abort_agent` / `501 exit: &Arc<ExitState>` / `508 exit.mark` / `519 tree.terminate()`，与我按行计数得出的同一结果一致。
- 注释（505–507）与代码语义逐句吻合：先标记（`is_running()` 立即为假、坏 runtime 不复用）→ 再唤醒等待中的请求 → 最后结束整棵树，并说明反过来会暴露不一致。与 `design.md` D1 的三步文本一致。

### ② D2 两条不变量的核实记录 — 实质通过（2 条行号精度 MINOR/SUGGESTION）

我逐条独立验证（不采信结论，只看代码）：

- **`ExitState::mark` 语义（D2-1）— 实质正确**：`process.rs:53-59`；`status` 为 `Mutex<Option<String>>` 的无条件覆写（`process.rs:55` `*slot = Some(status);`，无 `is_none` 守卫 → last-wins）；`done` 仅 `process.rs:57` 一处写（grep `done.store|done.load|done.swap` 全仓：写仅 57，读在 203 / 209 / 345，均与报告一致），因此置位不可逆；58 行 `notify_waiters()`。**报告结论成立**：提前 `mark` 只是让 `done=true` 提前到达，重复调用仍是幂等置位 + status 覆写，无二次状态翻转。
- **`wait_loop` 空 drain 与 status 覆写（D2-1）— 正确**：`process.rs:637-653`，`child.wait()` → drain（642-645，空表）→ 投递 `AgentExited`（646-650）→ `exit.mark(status)`（652）。超限路径上 `abort_agent` 已 drain 过，`wait_loop` 不会二次投递；其 status 覆写在新旧顺序下**同样发生**（旧顺序下 `mark` 也在 `terminate` 之前，`wait_loop` 一定更晚），确认不是本变更引入的新交互。
- **`terminate` 无条件 + 无「依赖 `is_running()` 为真才清理」的反向逻辑（D2-2）— 正确**：`abort_agent`（496–520）体内无 `if`/提前 return，也不查询 `is_running()`；`host.rs:220` 读到「假」走的是**更多**清理分支（`runtimes.remove` → `close_session` → `clear_sessions` → `supervisor.shutdown().await` → 重启新一代），`host.rs:512` 读到「假」返回 `SessionClosed`（失败关闭），`host.rs:628`/`process.rs:103` 仅诊断读。我做了**全仓**穷举（`**/*.rs` grep `is_running|has_exited`），生产代码仅上述 7 处，报告枚举完整且方向正确；`has_exited()` 在生产代码中**无任何消费方**（仅测试 `supervision.rs:475`），不存在反向逻辑。`Supervisor::shutdown`（358-397）不查询该位，顺序仍为 `fail_pending` → 关 stdin → grace → 必要时 `terminate` → `close` → join，未因提前 `mark` 跳过清理。

**MINOR-1（非阻断，报告精度）**：`reports/wp1-handoff.md` 的 D2-1 写「（last-wins，56 行 `*slot = Some(status)`）」，实际在 **55** 行（54 = `if let Ok(mut slot) = …{`，56 = 该 `if` 的 `}`）；同段 57 行（`done.store`）、203/209/345 行读取、53–59 行函数范围均准确。最小修正：把 56 改为 55。该处不构成「核实记录不真实」——断言的实体（无条件覆写、单点置位、幂等）逐条在 53–59 行可查。

### ③ `wait_loop` 与其余 `exit.mark` 调用点零改动、`tests/**` 零改动 — 通过（附 git 面限制）

- 全仓 `exit.mark` 调用点**仅两处**：`process.rs:508`（超限路径，被改动）与 `process.rs:652`（`wait_loop`，自然退出路径）；`ExitState` 结构、`is_running()`、`has_exited()`、`shutdown`、`wait_exit` 文本均保持变更前形态（`wait_loop` 仍是「先 drain 后 mark」，符合 spec Scenario 2「自然退出路径不受本顺序约束」）。
- `abort_agent` 调用点**仅两处**：`process.rs:470`（`read_loop` 未收完即超限）与 `process.rs:535`（`handle_line` 单帧超限）；二者调用同一函数体，故两条入口同时获得新顺序（核对任务第 3 点成立）。由此 `design.md` Non-Goals「不改 `wait_loop`」与 spec Scenario 2 同时满足。
- 测试未被弱化的实证（非仅采信报告）：`crates/agent-host/tests/supervision.rs:396-427` 仍断言「必须收到明确 `Oversize` 错误」+「`!supervisor.is_running()`」；且该文件 396 行的文档注释文本（「并让 `is_running()` 立即为假」）与 **实现前** 写就的 `design.md` 引用（`supervision.rs:396`）逐字一致、行号未漂移——若测试文件被改动，这两项很难同时保持。
- **限制（非阻断）**：`git diff --name-status 998cb0f..af64e85`（应为单文件 `M`）与逐字 diff 属 supervisor/主 Agent 可执行项，我无权执行。旁证：规划期文档快照的行号锚点 `is_running` 在 `process.rs:202`（实现前后 grep 实证均为 202，说明该点以上零漂移），`wait_loop` 由设计期 635/650 变为现 637/652（净 +2 行，且全部落在 `abort_agent` 内），与报告的「+4/−2、无语句增删、未触碰其它行」自洽；工作树对 launch HEAD `e544ac9` 干净（`watchdog_diff`：`No working-tree changes`）。

### ④ 未弱化断言、未触碰其它文件 — 通过（同 ③ 的 git 面限制）

- 无任何断言被删除或放宽：超限用例仍要求「错误类型必须是 `Oversize`」且「错误可见时 `is_running()` 必须为假」（`supervision.rs:411-425`），正是本变更的回归判据。
- `docs/`、`schemas/`、`fixtures/`、`compatibility/`、其它 crate 在检视范围内未见任何相关改动痕迹（`process.rs` 内 `MAX_MESSAGE_BYTES` 判据与错误类型不变；错误码/wire 语义未变）。
- **MINOR-2（SUGGESTION 级，非阻断，报告精度）**：`reports/wp1-handoff.md` 的 diff 摘要使用 `@@ -502,6 +502,10 @@`，但目标文件的 `tracing::error!` 实际在 **504** 行（grep 锚定 496/501/508/519 反推），而 `is_running`（202）以上零漂移 ⇒ 该 `@@` 头行号与真实 git diff 头不自洽（疑似手工重建）。hunk **内容**与「+4 注释 / −2 行」的净变化仍被行号漂移（202 不动、`wait_loop` +2）独立佐证，故不影响结论。最小修正：以真实 `git diff` 输出替换，或只保留三步顺序表。

### ⑤ spec 增量与实现语义一致（含「无部分请求先收到错误」的缝隙核对）— 通过

- 增量规范原文要求「SHALL 先把该 Agent 标记为退出（`is_running()` 为假、该运行时不得再被复用），再把超限错误投递给等待中的请求；在错误对调用方可见的任何时刻，不得仍把该 Agent 当作「仍在运行」」。
- 无缝隙：`mark`（508）严格先于取表（509）与**第一个** `sender.send`（514），且 drain 是单锁内一次性取整表后再逐个投递 ⇒ 任何被唤醒的调用方在观察错误时 `exit.done` 必为真；`is_running() = !done && !closing`（202-204），此路径 `closing` 为假，故必为假。`SeqCst` 存储 + `oneshot` 的同步关系使该观测确定性成立，非调度运气。
- 「不得再被复用」在实现侧有真实承接且未改动：`begin_request`（249）在 `mark` 后直接返回 `not_running_error()`（322-327），其 `exit.status()` 已是超限描述 ⇒ 报错更准确；`host.rs:220` 会作废运行时而**不**跨代复用（对应既有测试 `catalog.rs:949 open_refuses_a_runtime_whose_process_has_exited`）。
- Scenario 2（自然退出不受约束）与 `abort_agent` 仅被超限分支调用（470/535）一致；`wait_loop` 原有时序保留。
- 增量文件形态合规：`specs/local-agent-host/spec.md` 无 Purpose、仅 `## ADDED Requirements` 1 条 Requirement + 2 Scenario。

### SUGGESTION（report-only，无动作要求）

**窗口分析（已被 design 记录并确认非回归）**：新顺序使 `is_running()==false` 出现在「pending 尚未收敛」之前（508 → 509-518）。理论上若此时并发的 `AgentHost::ensure_runtime`（`host.rs:220`）走到 else 分支并 `supervisor.shutdown().await`，其 `fail_pending` 与 `abort_agent` 的 drain 争同一把锁，可能出现某个等待中的请求收到 `ShutdownPending` 而非 `Oversize`。判断为非新问题、非阻断：(a) 旧顺序下任何并发触发的 `shutdown()`（空闲回收 / `shutdown_all` / `host.rs:220`）与 `abort_agent` 的 drain 本来就会产生同一类 `Oversize`/`ShutdownPending` 二选一（drain 是整表原子取走，先到者赢）；(b) 我无法给出可复现路径（该窗口仅数个指令宽），且 spec 只承诺「标记先于投递」，未承诺并发关闭介入时的错误种类；(c) 被测用例直接驱动 `Supervisor`，不受该路径影响。结论：无需修改，供后续如出现相关 flaky 时归因参考。

## Assessment

- **静态结论**：`af64e85` 的实现**忠实且最小**地落实了 `design.md` D1，满足增量规范的两个 Scenario 与 `plan.md` Code Review 的五个关注点；`plan.md` 的阻断标准（①–⑤ 任一不满足即阻断）**无一条被触发**。工作包写入范围与 spec 一致，未触及 `wait_loop`、测试、签名、错误类型、错误码与其它 crate。
- **分级汇总**：CRITICAL 0；MAJOR 0；MINOR 2（均为 `reports/wp1-handoff.md` 的行号/摘要精度，实质结论已被我逐条独立复核为真）；SUGGESTION 1（窗口分析，report-only）。
- **未决 / 待核对**（不改变本报告 PASS，但影响下游门禁）：
  1. [PV1]/[PV2] 未返回：`npm run verify` 与 `cargo test --locked --workspace --all-features` 连跑 3 次（`oversize_frame` 每轮绿）尚未由我核对，属 plan 3.1/6.3 归主 Agent。本 PASS **不**覆盖验收口径中的「构造性确定 + 多次全绿」，不得据此宣称候选可合入。
  2. 需 supervisor 执行的 git 面确认（reviewer 无权）：`git diff --name-status 998cb0f..af64e85`（期望单文件 `M crates/agent-host/src/process.rs`）与 `git diff 998cb0f..af64e85`（期望 +4/−2、仅 `abort_agent`）。现有旁证（202 行零漂移、`wait_loop` 净 +2、工作树对 `e544ac9` 干净、reflog 提交信息）已高度自洽，但逐字原文仍属未执行项。
  3. 报告写盘：本角色无写工具，未能直接写入仓库内 `openspec/changes/agent-host-oversize-exit-ordering/reports/rv1-wp1.md`；本报告由运行时持久化到会话产物目录的同名相对路径。若工作流需要仓库内副本与 `verification.md` 的 Review Findings 登记，请由主 Agent 落盘/登记。
- **结论**：**PASS**（完成约定检视范围，无未解决 CRITICAL/MAJOR）。

## handoff_index

```yaml
handoff_index:
  - task_id: "3.2"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "af64e85cc252515039b0da97dcc3b88dbde277dc"
    evidence_type: REVIEW
    evidence_id: RV1
    report_path: openspec/changes/agent-host-oversize-exit-ordering/reports/rv1-wp1.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "在目标修订 af64e85cc252515039b0da97dcc3b88dbde277dc（base 998cb0f333e13953bc542e8c25efec242618dea9）的工作树内容上只读检视 abort_agent（process.rs:496-520）/ ExitState(31-60) / wait_loop(637-653) / is_running 全部消费方 / 增量规范与 design D1-D3。覆盖 plan.md Code Review 五关注点与 plan Coverage Index 的 R1/R2/R3；发现 2 条 MINOR（wp1-handoff.md 行号/摘要精度）+ 1 条 report-only SUGGESTION，无 CRITICAL/MAJOR，故 PASS。未执行：PV1/PV2 结果核对（属 3.1/6.3）与 git diff 逐字原文（reviewer 无 shell/git 权），已在本报告 Assessment 中列为待核对项。"
    source_evidence: NOT_APPLICABLE
```

---

未修改任何文件；未暂存任何内容；未执行构建、测试或 git 写操作。