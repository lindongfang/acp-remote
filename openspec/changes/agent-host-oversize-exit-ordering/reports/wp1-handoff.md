<!-- WP1 实现阶段交接报告（coder）。行为以 specs/local-agent-host/spec.md 为准，方案以 design.md D1–D3 为准。 -->

# WP1 实现交接报告（coder / implement）

## Shared Report

- **task_id**: `2.1`（顺序修正 + D2 不变量核实）、`2.2`（局部验证）
- **role**: coder
- **phase**: implement
- **agent_context**: 独立实现子 Agent（worker，run id `53d46466-f556-4104-89c7-9d9a43741f41`），
  继承主 Agent 的交接简报（proposal / specs / design / plan / tasks / AGENTS.md 摘要）与仓库读写工具，
  不继承其它角色的私有对话；共享工作树中本次唯一写入方；不担任 review / 集成 / 验收角色。
- **target_revision**: NOT_AVAILABLE——本次不提交代码（`AGENTS.md`/角色契约禁止运行任何 git 写命令），
  工作树状态由主 Agent 固化；本次改动基于分支 `feat/agent-host-oversize-exit-ordering` 的基线
  `998cb0f`（`git rev-parse HEAD` = `998cb0f333e13953bc542e8c25efec242618dea9`，`git status --short`
  仅 ` M crates/agent-host/src/process.rs`）。
- **scope**（实际写入）:
  - `crates/agent-host/src/process.rs`：仅 `abort_agent` 函数体内的语句顺序与其函数内注释；
  - `openspec/changes/agent-host-oversize-exit-ordering/reports/`（本报告与局部检查日志）。
  未触碰：函数签名、错误类型、`ExitState`、`is_running()`/`has_exited()`、`wait_loop`、`host.rs`、
  任何测试（`crates/agent-host/tests/**` 零改动）、`docs/`、`schemas/`、`fixtures/`、`compatibility/`、
  `openspec/` 的权威文件（plan/tasks/verification/proposal/design/specs 全部未改）。
- **changes**: `crates/agent-host/src/process.rs` 的 `abort_agent` 由「drain pending 并投递 Oversize
  错误 → `exit.mark(...)` → `tree.terminate()`」改为「`exit.mark(...)` → drain pending 并投递
  Oversize 错误 → `tree.terminate()`」，并把原来的函数内注释（先标记退出、再结束整棵树）上移并补全为
  实际顺序的三步说明（标记 → 唤醒等待中的请求 → 结束整棵树）。净变动：+4 行注释 / −2 行（`exit.mark`
  行与旧注释行），语句块整体上移，无新增/删除语句，无签名或语义以外的行为变化。
- **checks**: 见下「局部检查」——`cargo test -p agent-host`、`cargo fmt --all -- --check`、
  `cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings` 三项 EXIT=0。
- **issues**: 无阻断问题；无需要回主 Agent 决策的契约歧义。未执行项见「未执行 / 越界说明」。
- **result**: PASS（2.1 与 2.2 的本地可判定条件均满足）
- **evidence_paths**:
  - `openspec/changes/agent-host-oversize-exit-ordering/reports/wp1-handoff.md`（本报告）
  - `openspec/changes/agent-host-oversize-exit-ordering/reports/wp1-local-checks.log`（2.2 三条命令原始输出 + 退出码）
  - 注：本报告同时按运行时指定的权威输出路径写入会话产物目录
    `…/subagent-artifacts/outputs/53d46466-f556-4104-89c7-9d9a43741f41/openspec/changes/agent-host-oversize-exit-ordering/reports/wp1-handoff.md`，
    两份内容一致；仓库内 `reports/wp1-handoff.md` 是供 agentic 工作流读取的副本。
- **resource_cleanup**: 测试进程全部退出（三条命令均已返回）；观测 `$TEMP` 下 `acpr-*` 残留为 0；
  未创建 worktree、容器、端口或外部服务；未新增依赖；`target/` 仅常规共享使用（连跑窗口属 PV2/主 Agent，
  本次只做单次 `cargo test -p agent-host`）。

## 交付单元与工作包覆盖

- DU1 / WP1（唯一工作包），覆盖权威 tasks.md 的 2.1、2.2；无 TP。

## Changes（diff 摘要）

唯一改动文件 `crates/agent-host/src/process.rs`，唯一改动函数 `abort_agent`（函数体行 496–520）：

```diff
@@ -502,6 +502,10 @@（RV1-MINOR-2 订正：本 hunk 头为报告撰写时重建；权威 diff 以 git diff 998cb0f..af64e85 为准，主 Agent 已机械核对单文件 +4/−2） fn abort_agent(
     tracing::error!(reason, limit, actual, "ACP stdout 违反上限，结束该 Agent");
+    // 顺序即契约（`local-agent-host` 增量规范「超限结束的失败关闭顺序」）：先标记退出
+    // （`is_running()` 立即为假，坏 runtime 不会被继续复用），再唤醒等待中的请求，最后结束整棵树。
+    // 反过来则被错误唤醒的调用方会在「错误已可见、Agent 仍显示在运行」的窗口里观察到不一致。
+    exit.mark(format!("ACP 消息超限（{actual} > {limit} 字节）"));
     let drained: Vec<(u64, oneshot::Sender<Result<Value, HostError>>)> = pending
         .lock()
         .map(|mut map| map.drain().collect())
@@ -512,8 +516,6 @@ fn abort_agent(
             actual,
         })));
     }
-    // 先标记退出（`is_running()` 立即为假，坏 runtime 不会被继续复用），再结束整棵树。
-    exit.mark(format!("ACP 消息超限（{actual} > {limit} 字节）"));
     tree.terminate();
 }
```

改动后的实际顺序（行号基于改动后工作树）：

| 步 | 行号 | 语句 |
| --- | --- | --- |
| 0 | 504 | `tracing::error!(reason, limit, actual, "ACP stdout 违反上限，结束该 Agent")` |
| 1 | 508 | `exit.mark(format!("ACP 消息超限（{actual} > {limit} 字节）"))` |
| 2 | 509–518 | drain `pending` 并向每个等待者投递 `HostError::Protocol(AcpError::Oversize { limit, actual })` |
| 3 | 519 | `tree.terminate()`（无条件执行） |

`abort_agent` 的两个调用点都用同一函数体，因此两条超限入口（未收完就超限 `read_loop` 行 470、
单帧超限 `handle_line` 行 535）同时获得新顺序。

## D2 不变量核实记录（design D2 要求）

行号均为改动后工作树的 `crates/agent-host/src/process.rs` 行号（改动只发生在本函数内部，未影响其它行号）。

**D2-1 `ExitState::mark` 幂等/覆写语义不因提前调用产生问题（PASS）**

- `fn mark` 在 53–59 行：`status` 为 `Mutex<Option<String>>` 无条件覆写（last-wins，55 行
  `*slot = Some(status)`）；`done` 用 `store(true, SeqCst)` 置位（57 行），全 crate 只有这一处写 `done`
  （`grep done.store|done.load|done.swap` 结果：写仅 57 行，读在 203/209/345 行），因此 `done` 置位**不可逆**、
  不存在被复位成 `false` 的路径；57 行随后 58 行 `notify_waiters()` 唤醒关闭路径。
  结论：提前调用 `mark` 只是让「`done` 为真」提前到达，重复调用仍是幂等置位 + status 覆写，不产生二次状态翻转。
- `wait_loop`（637–653 行）：`child.wait()` 返回后（638–641 行）先 drain `pending`（642–645 行）并投递
  `AgentExited`（646–650 行），最后 652 行 `exit.mark(status)`。超限路径上 `abort_agent` 已经 drain 过
  `pending`，因此 `wait_loop` 的 drain 拿到空表（无二次投递）；它随后的 `exit.mark(status)` 会把 status
  覆写为真实退出状态，而该覆写是 55 行无条件覆写的既有行为——**在当前（改动前）顺序下同样发生**，
  不是本变更引入的新交互。`done` 在此时已为真，重复 `store(true)` 与 `notify_waiters()` 无副作用。

**D2-2 不存在「依赖 `is_running()` 为真才继续清理」的反向逻辑（PASS）**

- `abort_agent` 内 `tree.terminate()`（519 行）**无条件执行**：函数体内没有任何 `if`/提前 return，
  也不查询 `is_running()`（496–520 行无该调用），因此「标记后、terminate 前 `is_running()` 为假」的窗口
  不会让任何路径跳过终止。
- 全仓 `is_running()` / `runtime_running()` 消费方逐一核实（含定义处共 5 处生产代码）：
  - `process.rs:203` 定义（`!exit.done && !closing`）、`process.rs:208 has_exited`（同一 `done` 位）。
  - `process.rs:249`（`begin_request`）与 `process.rs:281`（`send_notification`）：`if !self.is_running()`
    直接返回 `not_running_error()`——新请求被拒绝，属失败关闭；且 `not_running_error()`（322–327 行）
    会返回 `exit.status()` 中已写入的超限状态，报错更准确，无反向依赖。
  - `host.rs:220`（`ensure_runtime`）：`if is_running() { 复用 } else { 作废会话端点 → 回收 runtime →
    重启新一代 }`。读到「假」走的是**更多**清理（含 `supervisor.shutdown().await`），无「为真才清理」分支。
  - `host.rs:512`（重开既有映射）：`if !is_running() { return Err(SessionClosed) }`——失败关闭。
  - `host.rs:624–629`（`runtime_running`）与 `process.rs:103`（`Debug`）：仅诊断读取，不改变状态。
- 关闭路径不依赖该位：`Supervisor::shutdown`（359–397 行）内部不查询 `is_running()`（已 grep 确认），
  它始终 `fail_pending` → 关 stdin → 等 grace → 必要时 `tree.terminate()` → `tree.close()` → join 子任务。
- 测试侧同向证据（未修改，仅引用）：`crates/agent-host/tests/catalog.rs:949`
  `open_refuses_a_runtime_whose_process_has_exited` 断言「进程已退出的运行时不得被复活」、
  `catalog.rs:133–142` 的 `wait_until_not_running` 只是轮询等待，均把「假」当作失败关闭信号。

**D2 第四条（`abort_agent` 内 `terminate` 无条件执行）**：见上 D2-2 第 1 点，519 行单条语句、无外层条件。

## 局部验证（task 2.2）

命令与退出码（完整原始输出见 `reports/wp1-local-checks.log`）：

| # | 命令 | 退出码 | 关键结果 |
| --- | --- | --- | --- |
| 1 | `cargo test -p agent-host` | `EXIT=0` | 7 个测试目标全绿：4 + 0 + 22 + 13 + 15 + 1 + 0，`0 failed`；`tests/supervision.rs` 15/15 通过，含 `test oversize_frame_ends_the_agent_and_fails_pending_requests ... ok` |
| 2 | `cargo fmt --all -- --check` | `EXIT=0` | 无输出（无格式差异） |
| 3 | `cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings` | `EXIT=0` | `Finished` 无 warning（`-D warnings` 下无输出即通过） |

测试零改动的证据：`git status --short` 仅 ` M crates/agent-host/src/process.rs`，`crates/agent-host/tests/**`
无改动；`oversize_frame` 用例（断言错误可见时 `!is_running()`）在原样未改的状态下通过。

## 未执行 / 越界说明

- **PV1（`npm run verify`）与 PV2（`cargo test --locked --workspace --all-features` 连跑 3 次）属主 Agent**
  （tasks 3.1 / 6.3），本实现 Agent 未执行、也不声称其结论；本报告只对 2.1/2.2 的本地局部检查负责。
- **RV1 独立 review 属独立只读 reviewer**（tasks 3.2），本报告不构成独立复核证据。
- 未运行任何 git 写命令（只读用了 `git status` / `git diff` / `git rev-parse`）；未提交、未暂存、未推送。

## 待澄清问题

无。设计 D1 的改动确认为同一函数内的语句移动 + 注释同步，未发现需要扩大写入范围的必要；函数签名、
错误类型、`wait_loop`、其它 `exit.mark` 调用点与测试均未触碰。

## handoff_index

```yaml
handoff_index:
  - task_id: "2.1"
    role: coder
    phase: implement
    stage: work-package
    target_revision: NOT_AVAILABLE
    evidence_type: CHECK
    evidence_id: NOT_APPLICABLE
    report_path: openspec/changes/agent-host-oversize-exit-ordering/reports/wp1-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "基线 998cb0f（分支 feat/agent-host-oversize-exit-ordering）；改动仅 crates/agent-host/src/process.rs 的 abort_agent；本行覆盖「新顺序 = 标记 → 投递 → terminate 且与注释一致、wait_loop 与其它 exit.mark 调用点零改动、D2 四条不变量按代码行核实、测试零改动」。target_revision 写 NOT_AVAILABLE 的原因：本角色不提交代码，工作树状态由主 Agent 固化，基线 998cb0f。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.2"
    role: coder
    phase: implement
    stage: work-package
    target_revision: NOT_AVAILABLE
    evidence_type: CHECK
    evidence_id: NOT_APPLICABLE
    report_path: openspec/changes/agent-host-oversize-exit-ordering/reports/wp1-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一基线与改动；执行 cargo test -p agent-host（EXIT=0，oversize_frame 通过且未修改）、cargo fmt --all -- --check（EXIT=0）、cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings（EXIT=0）；原始输出存 reports/wp1-local-checks.log。target_revision 写 NOT_AVAILABLE 的原因同上（未提交，工作树由主 Agent 固化）。"
    source_evidence: NOT_APPLICABLE
```

## 复核建议（供 RV1 / 主 Agent）

1. 对 `crates/agent-host/src/process.rs` 的 `git diff` 应为单文件、单函数、+4/−2 行级别（无语句增删）。
2. 复核 D2 行号：`mark` 53–59、`is_running` 202–204、`abort_agent` 496–520（`exit.mark` 508、`terminate` 519）、
   `wait_loop` 637–653（`exit.mark` 652）、`host.rs` 220 / 512 / 624–629。
3. 复核测试零改动（`crates/agent-host/tests/**` 无 diff）与 `oversize_frame` 断言未被弱化。
