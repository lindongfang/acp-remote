# Tasks

## 1. 会话循环的 tick 不再被出站队列饿死（单工作包）

写范围：`crates/server/src/node_link/conn/session.rs`（业务阶段的 select 与 tick 调度）、`crates/server/src/node_link/conn/tests.rs`（用例）。**非目标**：`crates/server/src/sync/**`（它的同构实现已修对，是本次对照）、`crates/server/src/node_link/conn/registry.rs` 的饱和判定与低水位（属变更⑥）、`crates/core/**`、`crates/node-link-protocol/**`、`schemas/**`、`fixtures/**`。

- [x] 1.1 tick 的 deadline **跨轮保留**（`sleep_until(next_tick)` 一类），不再每轮重建 `sleep(500ms)`；并把 tick 分支调整到出站分支**之前**，使其不可能被持续就绪的出站队列饿死。完成判据：在出站队列持续非空的压力下，tick 仍按周期发生（由 1.2 的用例断言）。判别力实测（2026-10-16，主检出）：把调度回退成「`outbound` 排在 tick 之前 + 每轮重建 `sleep(SESSION_TICK)`」后，1.2 的两条用例与 `a_permanently_saturated_queue_still_reaches_the_slow_consumer_watchdog` 一起变红（32 passed / 3 failed）；还原后 35 passed / 0 failed。
- [x] 1.2 新增用例：**出站队列持续非空**时，tick 驱动的行为仍按预期发生 —— 至少覆盖「心跳仍按 `heartbeatIntervalMs` 发出」与「入站静默判定仍会推进到超时关闭」两者之一（能覆盖两者更好）。完成判据：把 1.1 的修法回退（tick 排回最后或每轮重建）时该用例变红，并贴出双方输出。**落地形态**：两条都覆盖到了 —— 心跳侧 `a_busy_outbound_queue_does_not_starve_the_heartbeat`，静默侧 `inbound_silence_still_closes_a_connection_with_a_busy_outbound_queue`。实测（2026-10-16）回退后两条都变红（见 1.1 的记录），还原后全模块 35 passed / 0 failed；两者都断言的是 tick 驱动的行为本身（心跳帧 / 4408 关闭），不是「没抛错」。
- [x] 1.3 同步实现文档注释：`session.rs` 的模块文档要写明 tick 的调度口径与「为何它必须排在出站之前」。若发现 `docs/NODE_LINK_PROTOCOL.md` 或 `docs/MODULE_ARCHITECTURE.md` 有与实现不符的表述，**如实登记**而不是顺手改。

## 2. 规格

- [x] 2.1 不改规格：本变更实现的是既有要求（`node-link-owner-server` 的「心跳、超时与慢连接隔离」），按门禁指示在 `.openspec.yaml` 置 `skip_specs: true`。
