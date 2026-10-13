# Proposal

## Why

`daemon stop` 的关闭序列把**发出这条 `daemon.stop` 的连接**也算作「在途客户端连接」并纳入排空等待。于是排空时长取决于该客户端进程何时被调度：正常情况下它收到 `accepted` 就立刻关闭连接（毫秒级），负载下被延迟调度就会吃满整个 `graceMs`，并级联触发 `daemon.task_stop_timeout`（宽限被排空耗尽后，后续任务的 deadline 已过期）。

这在 CI 上的表现是一条时序敏感的 flaky 用例：`crates/app/tests/daemon_lifecycle.rs` 的 `stop_does_not_wait_for_the_full_grace_without_other_clients` 实测 `drain_ms=2999`（宽限 3000），同一提交的另一个 run 通过。flaky 的代价不只是重跑几分钟 —— 红灯成为常态后，真失败也会被当成「再跑一次」。

## What Changes

- 排空窗口 MUST NOT 把「发出该 `daemon.stop` 的那条连接」计为在途客户端连接：它的语义是**请求关闭**，而不是正在进行业务。
- 其它在途客户端连接仍照旧被等待；宽限耗尽仍未结束者仍被关闭，并留下 `daemon.drain_timeout` 可观察记录（该口径不变）。
- 把「关闭时的连接排空」写进 `daemon-lifecycle` 规格 —— **本轮之前该规格没有任何排空要求**，该契约只存在于代码与注释里，这也是它长期无人发现的原因之一。

## Capabilities

### New Capabilities

（无。本变更只修改既有能力。）

### Modified Capabilities

- `daemon-lifecycle`：新增「关闭时的连接排空」这条要求（含「关闭请求自身的连接不进入排空」与「其它在途连接仍被等待」两个场景）。
