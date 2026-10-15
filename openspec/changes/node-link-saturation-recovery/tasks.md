# Tasks

## 1. 让饱和状态可恢复（单工作包）

写范围：`crates/server/src/node_link/conn/registry.rs`（饱和判定与低水位常量）、`crates/server/src/node_link/conn/session.rs`（Tick 分支）、`crates/server/src/node_link/catalog.rs` 与 `resource.rs`（受害面）、`crates/server/src/node_link/conn/tests.rs`（用例）。**非目标**：`crates/server/src/sync/**`（它的同构实现已在 WP3 修对，是本次的**对照**而非改动对象）、`crates/core/**`、`crates/node-link-protocol/**`、`schemas/**`、`fixtures/**`。

- [x] 1.1 在 `registry.rs` 的 `consumed()` 里按**低水位迟滞**清除饱和状态：先在同一临界区内算出「是否已降到低水位以下」再清除（对照 `crates/server/src/sync/conn/registry.rs` 的 `consumed` 与 `RECOVERY_MESSAGES`/`RECOVERY_BYTES`），并补上低水位常量与其文档（低水位不得等于高水位）。完成判据：新增用例「曾达高水位 → 消费到低水位以下 → 不再被判为饱和」；把修法回退（不清除）时该断言变红。
- [x] 1.2 修正 `session.rs` 的 Tick 分支结构，使**饱和期间心跳仍发出**（今天 `else if now >= next_ping` 因饱和分支短路而永久不可达）。完成判据：饱和未超宽限时**心跳判定不被短路** —— `tick_action` 到周期时仍返回 `Ping`（用例 `the_tick_keeps_the_heartbeat_while_the_connection_is_saturated`），且 ping 因高水位被拒时按一级处置推迟而不是结束连接（用例 `a_heartbeat_deferred_by_the_high_water_mark_is_not_fatal`）；把「饱和时短路心跳判定」回退回去时前者变红（实测：34 passed / 1 failed）。**口径（如实说明，2026-10-16 检视后收窄）**：队列持续位于高水位**之上**时，`link.ping` 与被撑满的队列共用同一个有界队列，那一状态下不会有 ping 真的发出，因此「饱和期间真的收到 ping」不可观测；可观测的是「心跳判定不被短路 + 被拒时推迟而非断开 + 断开只由看门狗按宽限判定」，delta spec 的措辞已按此改准。
- [x] 1.3 修复受害面：`catalog.rs` 与 `resource.rs` 的 `handle.saturated()` 门必须在恢复后重新开始读取批次。完成判据：用例 `a_recovered_connection_resumes_catalog_batches_and_event_delivery` 断言恢复后 catalog 的两个批次**全部重发**、resource 走完 `snapshot_begin → chunk → snapshot_end`；把任一受害门回退成「永不再读」时该用例变红（实测：32 passed / 3 failed）。**注（判据收窄，2026-10-16 检视后补）**：同一用例里「扇出的 `resource.event` 仍送达」那一步**不是**判别证据 —— 事件扇出路径不读 `saturated()`，它只作伴随观察。
- [x] 1.4 补齐 node_link 侧缺失的两侧覆盖：**瞬时突发后回落**（不得断开）与**持续饱和**（必须断开）各一条；两者都要断言其它连接不受影响。完成判据：两条用例各自在把对应修法回退时变红。判别力实测（2026-10-16，主检出）：把 `TickAction::Disconnect` 分支临时改成「同时 `request_close` 同节点的其他连接」后，本侧用例在新增的跨连接断言处变红（`assert_eventually(active()==1)` 超时）；还原后 `cargo test -p server --lib node_link::conn::tests` = 35 passed / 0 failed。
- [x] 1.5 同步实现文档注释：`registry.rs`/`session.rs` 的模块文档要写明高/低水位与饱和期保活的口径；若发现 `docs/NODE_LINK_PROTOCOL.md` 或 `docs/MODULE_ARCHITECTURE.md` 有与实现不符的表述，如实登记而不是顺手改。

## 2. 规格落地（本变更已完成，供对照）

- [x] 2.1 `specs/node-link-owner-server/spec.md` 修改「心跳、超时与慢连接隔离」：补上「持续过慢」的可恢复判定（低水位迟滞）、饱和期心跳必须继续、高/低水位须有明确常量；三个场景（恢复后不断开、饱和期心跳不停、真正持续过慢仍断开）。

## 3. 相邻项（本变更发现、未在包内处理）

- [x] 3.1 **（另案登记）** `crates/server/src/transport/net/ws.rs` 的 `send_text`/`recv` 是同一 `WebSocket` 上的裸 await、**没有写期限** —— node_link 与 sync **共有**的相邻缺口（不是本变更引入）。完成判据：判定「无写期限」在慢消费者场景下的真实影响（是否会让会话任务卡在写路径上而绕过饱和看门狗），并给出修法与证据。
