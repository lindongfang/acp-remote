# Proposal

## Why

Node Link 的慢消费者判定**不可恢复**：`ConnectionHandle::consumed` 的清除点不可达（`crates/server/src/node_link/conn/registry.rs` 的 `saturated()` 粘性短路、`consumed()` 不可达、`mark_saturated()` 只置位，且**没有低水位恢复常量** —— sync 侧的同构实现有 `RECOVERY_MESSAGES`/`RECOVERY_BYTES`）。后果有三层：

1. `saturated_since` 一旦置位**永不清除**，于是「**持续**过慢」这个判定事实上退化成「**曾经**过慢」；
2. 同一 Tick 分支里的 `else if now >= next_ping` 因此**永久不可达** —— **心跳停摆**，一条已经恢复的合法连接会被关闭；
3. 受害面是读入门：`node_link/catalog.rs:139` 与 `node_link/resource.rs:591` 的 `handle.saturated()` 让该连接**永久停止**读取新的快照批次与事件批次。

它在 CI 上全绿，因为触发需要**持续**饱和（瞬时突发不会暴露）；而线上表现是「一条连接莫名其妙不再收到任何东西、随后被断开」。

## What Changes

- 让饱和状态**可恢复**：队列被消费到低水位以下时清除饱和状态（低水位迟滞，对照 sync 侧既有做法），并补上明确的高/低水位常量。
- 饱和期间**心跳继续**：饱和不是静默，不得因心跳未发出而走进「连续 90 秒无入站消息」的关闭路径。
- 修复受害面：饱和恢复后，快照批次与事件批次的读取必须重新开始。
- 补上 node_link 侧缺失的覆盖：瞬时突发后回落（不断开）与持续饱和（断开）两侧各一条；`limits.rs` 只做限额协商、无需改动。
- 同步 `node-link-owner-server` 规格：把「持续过慢」的可恢复语义与饱和期保活写成明文（今天的要求只说「持续过慢时断开」，没有说「恢复后必须恢复」）。

## Capabilities

### New Capabilities

（无。本变更只修改既有能力。）

### Modified Capabilities

- `node-link-owner-server`：修改「心跳、超时与慢连接隔离」要求 —— 补上饱和状态的可恢复判定（低水位迟滞）、饱和期心跳必须继续、以及高/低水位阈值须有明确常量。
