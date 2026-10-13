# Tasks

## 1. 让饱和状态可恢复（单工作包）

写范围：`crates/server/src/node_link/conn/registry.rs`（饱和判定与低水位常量）、`crates/server/src/node_link/conn/session.rs`（Tick 分支）、`crates/server/src/node_link/catalog.rs` 与 `resource.rs`（受害面）、`crates/server/src/node_link/conn/tests.rs`（用例）。**非目标**：`crates/server/src/sync/**`（它的同构实现已在 WP3 修对，是本次的**对照**而非改动对象）、`crates/core/**`、`crates/node-link-protocol/**`、`schemas/**`、`fixtures/**`。

- [ ] 1.1 在 `registry.rs` 的 `consumed()` 里按**低水位迟滞**清除饱和状态：先在同一临界区内算出「是否已降到低水位以下」再清除（对照 `crates/server/src/sync/conn/registry.rs` 的 `consumed` 与 `RECOVERY_MESSAGES`/`RECOVERY_BYTES`），并补上低水位常量与其文档（低水位不得等于高水位）。完成判据：新增用例「曾达高水位 → 消费到低水位以下 → 不再被判为饱和」；把修法回退（不清除）时该断言变红。
- [ ] 1.2 修正 `session.rs` 的 Tick 分支结构，使**饱和期间心跳仍发出**（今天 `else if now >= next_ping` 因饱和分支短路而永久不可达）。完成判据：新增用例「持续饱和期间仍按 `heartbeatIntervalMs` 发出 `link.ping`」。
- [ ] 1.3 修复受害面：`catalog.rs` 与 `resource.rs` 的 `handle.saturated()` 门必须在恢复后重新开始读取批次。完成判据：新增用例「饱和 → 恢复后，该连接继续收到新的快照批次与事件批次」。
- [ ] 1.4 补齐 node_link 侧缺失的两侧覆盖：**瞬时突发后回落**（不得断开）与**持续饱和**（必须断开）各一条；两者都要断言其它连接不受影响。完成判据：两条用例各自在把对应修法回退时变红。
- [ ] 1.5 同步实现文档注释：`registry.rs`/`session.rs` 的模块文档要写明高/低水位与饱和期保活的口径；若发现 `docs/NODE_LINK_PROTOCOL.md` 或 `docs/MODULE_ARCHITECTURE.md` 有与实现不符的表述，如实登记而不是顺手改。

## 2. 规格落地（本变更已完成，供对照）

- [x] 2.1 `specs/node-link-owner-server/spec.md` 修改「心跳、超时与慢连接隔离」：补上「持续过慢」的可恢复判定（低水位迟滞）、饱和期心跳必须继续、高/低水位须有明确常量；三个场景（恢复后不断开、饱和期心跳不停、真正持续过慢仍断开）。

## 3. 相邻项（本变更发现、未在包内处理）

- [ ] 3.1 **（另案登记）** `crates/server/src/transport/net/ws.rs` 的 `send_text`/`recv` 是同一 `WebSocket` 上的裸 await、**没有写期限** —— node_link 与 sync **共有**的相邻缺口（不是本变更引入）。完成判据：判定「无写期限」在慢消费者场景下的真实影响（是否会让会话任务卡在写路径上而绕过饱和看门狗），并给出修法与证据。
