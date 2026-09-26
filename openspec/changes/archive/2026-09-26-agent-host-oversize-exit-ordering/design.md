<!-- 说明实现方案与决策理由；行为以 specs/local-agent-host/spec.md 的增量为准，协作安排写 plan.md。 -->

## Context

动机见 `proposal.md` 的 Why。影响方案的现状事实（2026-09-26 读码核实）：

- **竞态窗口**：`crates/agent-host/src/process.rs` 的 `abort_agent`（约 495–517 行）当前顺序为
  「drain pending 并投递 `Oversize` 错误 → `exit.mark(...)` → `tree.terminate()`」。投递错误会唤醒等待中的
  测试/调用方任务，而 `mark` 还没执行——`is_running()`（= `!exit.done && !closing`，`process.rs:202`）
  在该窗口内仍为真。满载并行时调用方可在窗口内完成断言，即 PRO-4 的偶发红。
- **意图矛盾**：`abort_agent` 自身注释（「先标记退出（`is_running()` 立即为假），再结束整棵树」）与
  测试文档注释（`supervision.rs:396`「并让 `is_running()` 立即为假」）都要求「先标记」，实现却后标记。
- **相关路径已核实**：
  - `ExitState::mark`（`process.rs:53-59`）：`status` 覆写（last-wins）、`done` 幂等置位、`notify_waiters`。
    调用点仅两处：`abort_agent:516` 与自然退出观察 `wait_loop:650`。
  - `wait_loop`（`process.rs:635-652`）：自然退出时先 drain pending（`AgentExited` 错误）再 `mark`。
    超限路径上 `terminate` 触发 child 退出后，`wait_loop` 的 drain 拿到的是空表（pending 已被
    `abort_agent` drain），不会二次投递；它随后的 `mark` 会把 status 覆写为真实退出状态——
    **该覆写在当前顺序下同样发生**，不是本变更引入的新交互。
- **被否决的备选**：「有界等待」（PRO-4 登记时的原始建议）——只改测试容忍竞态，会把「立即为假」的
  设计保证弱化为「最终为假」，未来真回归时反而测不出。用户已于 2026-09-26 选定根治（选项 A）。

## Goals / Non-Goals

**Goals:**

- 超限结束路径上，退出标记严格先于错误投递（构造性消除竞态，不靠重试或等待）。
- 既有 `oversize_frame` 用例不改断言即恢复确定性，继续充当回归测试。
- 增量规范把该顺序钉为可验收行为（specs 的 `local-agent-host` 增量）。

**Non-Goals:**

- 不改动 `wait_loop`（自然退出路径的「先投递后标记」保持原样——spec 增量场景 2 明确本顺序只约束
  超限结束路径）。
- 不调整 `ExitState`/`is_running()` 的实现或签名。
- 不做「证明 flaky 永不复发」的无限重跑。

## Decisions

### D1：顺序调整为「标记 → 投递 → terminate」

`abort_agent` 改为：

```text
exit.mark(format!("ACP 消息超限（{actual} > {limit} 字节）"));   // 1. 先标记（is_running() 立即为假）
drain pending 并投递 Oversize 错误;                              // 2. 再唤醒调用方
tree.terminate();                                                // 3. 结束进程树（原顺序不变）
```

- 选择理由：函数内唯一竞态源是「投递在前、标记在后」；把 `mark` 提前到 drain 之前后，任何被错误唤醒的
  调用方观察到的 `exit.done` 必为真——**确定性由构造保证**，不依赖调度运气。
- `terminate` 仍在最后：与「先标记、后杀树」的既有注释完全一致；标记与真正退出之间 Agent 短暂
  「已标记但未死」与当前顺序下的窗口同形（当前是「先标记再 terminate」本来就有的窗口），无新状态。
- 同步把函数注释更新为实际顺序（注释本来就描述这个顺序，只需确保文字与新代码一致）。

### D2：实现时必须核实并留证的两条不变量

1. `ExitState::mark` 幂等/覆写语义不因提前调用产生问题：`done` 置位不可逆；`wait_loop` 随后的
   status 覆写与当前行为一致（已读码确认，见 Context）。实现 Agent 需在交接报告中引用这两处代码行。
2. `is_running()` 在「标记后、terminate 前」为假的窗口内，不会有路径因此**跳过** `terminate`
   （已核实 `abort_agent` 内 `tree.terminate()` 无条件执行）；其它 `is_running()` 消费方
   （shutdown、新请求拒绝）看到「假」只会更安全（失败关闭），实现 Agent 需确认不存在
   「依赖 `is_running()` 为真才继续清理」的反向逻辑。

### D3：测试零改动；验收 = 构造性确定 + 多次全绿

- 不改 `supervision.rs` 的任何断言（其文档注释「立即为假」在 D1 后恢复为真）。
- 验收口径：本机 `cargo test --locked --workspace --all-features` **连跑 3 次**全绿（含该用例），
  加 `npm run verify`。3 次是「并行负载下稳定性」的实证下限；竞态类问题不以有限次数证明零概率，
  确定性由 D1 的构造给出。

## Risks / Trade-offs

- [标记先于 terminate：`has_exited()` 在进程真正死亡前短暂为真] → 与当前顺序下「标记后 terminate 前」
  的窗口同形（毫秒级、同函数内同步执行）；消费方按失败关闭处理（拒绝复用），无反向依赖（D2-2 核实）。
- [未来有人以其它理由再调整这三行顺序，重新引入竞态] → 增量规范（specs）把顺序钉为可验收行为 +
  既有用例是回归测试；评审时以规范为准。
- [3 次全绿仍不能数学证明零复发] → 已在 proposal 的 assumptions 声明口径：确定性来自构造，
  重跑只是负载下的实证抽样；接受该局限。

<!-- 无部署/数据迁移内容，省略 Migration Plan；无可安全延后的开放问题，省略 Open Questions。 -->
