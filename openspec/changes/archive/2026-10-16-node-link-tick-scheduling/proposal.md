# Proposal

## Why

Node Link 会话业务阶段的 tick 是**每轮重建**的 `sleep(500ms)`，而且在 biased select 里排在**最后**（`crates/server/src/node_link/conn/session.rs` 约 816-826）。只要出站队列连续非空，`select!` 的 biased 顺序就会一直命中断言更高优先级的出站分支，`connection.recv()` 与 tick 都**永远不被轮询**：

- 90 秒静默判定（约 884）停摆；
- 慢消费者看门狗（约 893-905）停摆；
- 心跳 `link.ping`（约 906-923）停摆 —— 于是**恰恰在最需要保活的时候，保活不再运行**。

同一份代码在 sync 侧已在本仓 WP3 修复轮改对（`sync/conn/session.rs` 用 `sleep_until(next_tick)` 跨轮保留 deadline，并把 tick 排在出站分支**之前**，约 1003-1007/1020-1028/1080）。node_link 侧仍是旧形态，且 CI 全绿（触发需要出站队列**持续**非空）。

## What Changes

- tick 的 deadline 跨轮保留（`sleep_until`），不再每轮重建，避免「每次被唤醒都重新睡 500ms」造成的累积饥饿。
- 把 tick 分支排在出站分支**之前**，使它不能被持续就绪的出站队列永久饿死。
- 补一条用例：在出站队列持续非空的压力下，tick 驱动的行为（心跳 / 静默判定 / 慢消费者看门狗）仍按预期发生。
- 不新增规格要求：本变更是把既有要求（「按 `heartbeatIntervalMs` 发送 `link.ping`」「连续 90 秒无入站消息或 `link.pong` 时关闭」）真正实现，而不是改变它们 —— 按 `openspec` 的指示在 `.openspec.yaml` 置 `skip_specs: true`。

## Capabilities

### New Capabilities

（无。）

### Modified Capabilities

（无 —— 纯实现修复，不改既有要求的语义。已按门禁要求在 `.openspec.yaml` 声明 `skip_specs: true`。）
