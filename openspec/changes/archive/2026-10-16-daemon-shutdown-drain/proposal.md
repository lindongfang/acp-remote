# Proposal

## Why

`daemon stop` 的关闭序列把**发出这条 `daemon.stop` 的连接**也算作「在途客户端连接」并纳入排空等待。于是排空时长取决于该客户端进程何时被调度：正常情况下它收到 `accepted` 就立刻关闭连接（毫秒级），负载下被延迟调度就会吃满整个 `graceMs`；而宽限一旦被排空耗尽，后续任务的 deadline 已过期，会级联触发 `daemon.task_stop_timeout`。（注意：关闭序列的**总**耗时另有与排空无关的组成部分 —— 例如后台 maintenance 任务的取消延迟，实测可达约 3020ms 并单独触发 `task_stop_timeout` —— 因此「整条关闭序列的耗时」不能当作排空的判据。）

这在 CI 上的表现是一条时序敏感的 flaky 用例：`crates/app/tests/daemon_lifecycle.rs` 的 `stop_does_not_wait_for_the_full_grace_without_other_clients` 曾在一次 run 里失败、而同一提交的另一个 run 通过。**一处需要澄清的读法**：当时拿到的 `daemon.drain` 的 `drain_ms=2999`（宽限 3000）曾被解读为「排空被吃满」，但该字段当时记的是 `deadline - now`（排空前算出的**剩余等待预算**，按构造恒 ≈ 宽限），**不是耗时** —— 后续已把该日志挪到排空结束之后、`drain_ms` 改记**实测耗时**，并用 `window_ms` 保留预算值。而旧断言判的是「整条关闭序列的耗时 < 2/3 宽限」，关闭序列又含与排空无关的阶段，因此它本身就带着掷硬币的成分。flaky 的代价不只是重跑几分钟 —— 红灯成为常态后，真失败也会被当成「再跑一次」。

## What Changes

- 排空窗口 MUST NOT 把「发出该 `daemon.stop` 的那条连接」计为在途客户端连接：它的语义是**请求关闭**，而不是正在进行业务。
- 其它在途客户端连接仍照旧被等待；宽限耗尽仍未结束者仍被关闭，并留下 `daemon.drain_timeout` 可观察记录（该口径不变）。
- 把「关闭时的连接排空」写进 `daemon-lifecycle` 规格 —— **本轮之前该规格没有任何排空要求**，该契约只存在于代码与注释里，这也是它长期无人发现的原因之一。

## Capabilities

### New Capabilities

（无。本变更只修改既有能力。）

### Modified Capabilities

- `daemon-lifecycle`：新增「关闭时的连接排空」这条要求（含「关闭请求自身的连接不进入排空」与「其它在途连接仍被等待」两个场景）。
