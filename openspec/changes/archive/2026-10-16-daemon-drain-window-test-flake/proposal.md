# Proposal

## Why

`crates/app/tests/daemon_lifecycle.rs` 的 `the_network_listener_stops_serving_new_connections_before_the_local_drain_finishes`（约 610-660 行）在 CI 上**不稳定**：PR #123 的两轮里一轮失败、一轮通过，失败点是

```text
thread 'the_network_listener_stops_serving_new_connections_before_the_local_drain_finishes' panicked at crates/app/tests/daemon_lifecycle.rs:649
test result: FAILED. 12 passed; 1 failed
```

它挡住了一张**只改前端测试**的 PR。（此前该变更的连跑只在主检出做过 10 次全绿——本机比 CI runner 快。）

该用例的被测行为是对的（「停接入层之后不得再服务新连接」）；不稳的是**它的判据方式**：它等日志出现 `daemon.ingress_stopped` 之后**立即**探一次 HTTP，要求失败。而日志行是先写的、listener 真正停止在后（内核 accept 队列与 accept 循环之间还有一段窗口），于是慢跑器上这一次探针有机会**成功** ⇒ 断言变红。同一文件里已登记过同类问题的另一条（`the_periodic_task_runs_again_after_one_full_cycle` 的 startup 断言）。

## What Changes

- 把该用例的判据改成**确定性**的形态：在「关闭序列已开始且进程仍存活」的前提下，**在有界预算内反复探针直到失败**（或断言在进程退出前从未成功），而不是「探一次就必须失败」。
- 保持被排除的反例：若实现根本不停止接入面（探针在整个预算内一直成功），用例仍必须变红；若进程已退出，则以明确原因失败（不能把「进程没了」当成通过）。
- 顺带核查同文件其它时间窗口断言是否有同类「一次采样」形态，如实登记（不在本变更内顺手改）。

**非目标**：不改关闭序列的实现口径，不改 `shutdown_grace_ms` 的语义，不放宽任何「必须成立」的断言。

## Capabilities

### New Capabilities

（无 —— 本变更只改测试的判据方式，不改任何行为要求。）

## Impact

- `crates/app/tests/daemon_lifecycle.rs`（仅用例判据）。
- CI：`合同门禁 + Rust 检查` job 的不稳定来源之一。
- 与本变更相邻的登记：`openspec/changes/archive/2026-10-16-daemon-shutdown-drain/` 的 3.3（同一份 `daemon_lifecycle.rs` 的另一条时序敏感用例）。
