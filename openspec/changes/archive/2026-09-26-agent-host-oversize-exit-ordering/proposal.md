<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

`agent-host` 的 `oversize_frame` 用例在 Windows 全量并行负载下偶发红（登记为 PRO-4，切片 4 的 CI 实踩过一次候选轮红）。读码确认根因不是测试太严，而是**实现没有做到自己注释承诺的顺序**：`crates/agent-host/src/process.rs` 的 `abort_agent` 先把超限错误投递给等待中的请求、之后才 `exit.mark` 标记退出，两者之间存在线程调度窗口，使「错误已可见但 `is_running()` 仍为真」成为可能。该函数自己的注释（「先标记退出（`is_running()` 立即为假）」）与测试的文档注释（supervision.rs:396「并让 `is_running()` 立即为假」）都要求相反顺序。偶发红会持续消耗「是已知 flaky 还是真 bug」的人工判断成本，并损害整个测试套件的报警信用。

## What Changes

- **`crates/agent-host/src/process.rs` 的 `abort_agent`**：把 `exit.mark(...)` 移到「向未完成请求投递超限错误」**之前**（顺序变为：标记退出 → 投递错误 → 结束进程树），使代码符合其注释承诺的顺序，消除「错误可见时 Agent 仍显示在运行」的窗口。函数内顺序调整，不改任何函数签名、错误类型或终止语义。
- **`local-agent-host` 能力规范**新增一条 Requirement（增量）：把「超限结束路径上退出状态先于错误投递被标记」钉为可验收行为，防止未来重构把竞态加回来。
- 既有 `oversize_frame` 用例**不改**：它的断言（错误可见时 `!is_running()`）在顺序修正后恢复确定性，继续充当该保证的回归测试。

不包含：其它失败路径的顺序审查（如自然退出、取消路径——本次读码未见同类注释/实现矛盾，见 design 的核实记录）；「有界等待」式测试容忍方案（已被根治方案取代，登记为已否决的备选）；除 `abort_agent` 外的任何产品行为变更。

## Intent and Constraints

```agentic-intent
sources:
  - "2026-09-25 切片 4 收尾时登记的 PRO-4（agent-host oversize_frame 用例在 Windows 全量并行负载下偶发红；当时建议『有界等待』）"
  - "2026-09-26 本会话：用户选择以 PRO-4 立变更；主 Agent 读码发现根因是 abort_agent 的顺序与其注释/测试文档矛盾，向用户提出 A（调正顺序，根治）/ B（有界等待，容忍）/ C（两者都做）三选项，用户原话选『A』"
constraints:
  - "以选项 A 为准：只调整 abort_agent 内部顺序（exit.mark 先于错误投递），不改测试断言、不引入『有界等待』"
  - "不改函数签名、错误类型、ACP wire 行为与终止语义；改动必须能被既有 oversize_frame 用例原样验证"
  - "exit.mark 的幂等性与「先标记后 terminate」对退出观察路径（exit watcher）的影响必须在实现时核实并留证（不得引入二次状态翻转）"
  - "验收证据需包含该用例在全量并行负载下多次全绿（量级见 plan 的替代检查），单次绿不算数"
non_goals:
  - "不审查或重构 abort_agent 以外的失败/退出路径"
  - "不实现『有界等待』备选方案"
  - "不改动 acp-protocol 的 limits 定义或其它 crate"
  - "不追求『证明 flaky 永不复发』（竞态类问题无法用有限次数证明不存在；以『构造上确定 + 多次全绿』为验收口径）"
success_criteria:
  - "abort_agent 的顺序为「标记退出 → 投递错误 → terminate」，与其注释一致"
  - "既有 oversize_frame 用例不改断言即恢复确定性（错误可见时 is_running() 必为假）"
  - "全量 `cargo test --locked --workspace --all-features` 在本机连跑 3 次全绿（含该用例），`npm run verify` 全绿"
decision_bounds:
  - "exit.mark 落点的精确位置（循环前）、ExitState 幂等性核实方式、是否补充实现注释由实现 Agent 自主决定"
  - "连跑次数如需超过 3 次由主 Agent 按现场稳定性判断"
  - "改动 abort_agent 以外代码、改测试断言、改公开 API 必须回到用户决策"
assumptions:
  - "自然退出、取消、shutdown 等其它调用 exit.mark 的路径不依赖「错误投递先于标记」的旧顺序（实现时以代码核实为准；若发现依赖，暂停并回主 Agent）"
  - "该用例的偶发率足够低，3 次全绿 + 构造性确定（先标记后投递）构成充分验收依据；不以有限重跑『证明』零概率"
```

## Capabilities

<!-- 先检查现有规范，区分新增能力与已有能力的需求变化。 -->

### New Capabilities

无新增能力。

### Modified Capabilities

- `local-agent-host`: 新增一条 Requirement（增量 ADDED）——「超限结束的失败关闭顺序」：把「结束 Agent 时退出状态先于错误投递被标记（`is_running()` 在错误可见时已为假、运行时不可复用）」钉为可验收行为。现有「失败路径的明确结果与 turn 不设超时」等 Requirement 的文本不变。

## Impact

- **代码**：`crates/agent-host/src/process.rs`（仅 `abort_agent` 函数内的语句顺序，预期个位数行变动 + 注释同步）。
- **规范**：`openspec/specs/local-agent-host/spec.md` 新增一条 Requirement（本变更的增量规范先行，归档时落盘）。
- **测试**：无新增/修改（既有用例即回归测试）；验收侧增加「全量连跑 3 次」的执行证据。
- **API/ABI**：无签名变化；可观察语义变化仅为「`is_running()`/`has_exited()` 在超限错误可见时已为假」——这正是既有注释与测试早已承诺的行为，属于「使实现符合既有契约表述」。
- **依赖/文档/合同资产**：无新增依赖；`docs/`、`schemas/`、`fixtures/`、`compatibility/` 零改动；`npm run check` 判定面不变。
- **CI**：判定面不变；预期消除该类偶发红。
