<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

`agent-host` 的 `oversize_frame` 用例在 Windows 全量并行负载下偶发红（登记为 PRO-4，切片 4 的 CI 实踩过一次候选轮红）。读码确认根因不是测试太严，而是**实现没有做到自己注释承诺的顺序**：`crates/agent-host/src/process.rs` 的 `abort_agent` 先把超限错误投递给等待中的请求、之后才 `exit.mark` 标记退出，两者之间存在线程调度窗口，使「错误已可见但 `is_running()` 仍为真」成为可能。该函数自己的注释（「先标记退出（`is_running()` 立即为假）」）与测试的文档注释（supervision.rs:396「并让 `is_running()` 立即为假」）都要求相反顺序。偶发红会持续消耗「是已知 flaky 还是真 bug」的人工判断成本，并损害整个测试套件的报警信用。

## What Changes

- **`crates/agent-host/src/process.rs` 的 `abort_agent`**：把 `exit.mark(...)` 移到「向未完成请求投递超限错误」**之前**（顺序变为：标记退出 → 投递错误 → 结束进程树），使代码符合其注释承诺的顺序，消除「错误可见时 Agent 仍显示在运行」的窗口。函数内顺序调整，不改任何函数签名、错误类型或终止语义。
- **`local-agent-host` 能力规范**新增一条 Requirement（增量）：把「超限结束路径上退出状态先于错误投递被标记」钉为可验收行为，防止未来重构把竞态加回来。
- 既有 `oversize_frame` 用例**不改**：它的断言（错误可见时 `!is_running()`）在顺序修正后恢复确定性，继续充当该保证的回归测试。

不包含：其它失败路径的顺序审查（如自然退出、取消路径——本次读码未见同类注释/实现矛盾，见 design 的核实记录）；「有界等待」式测试容忍方案（已被根治方案取代，登记为已否决的备选）；除 `abort_agent` 外的任何产品行为变更。

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
