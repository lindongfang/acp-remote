<!-- 技术方案：解释「怎么修」。动机见 proposal.md（Why / What Changes）；本变更不改规格（`.openspec.yaml` 置 skip_specs: true）。 -->

# Design

## Context

- 现状（修复前）：`crates/server/src/node_link/conn/session.rs` 业务阶段的循环每轮**重建** `sleep(SESSION_TICK)`，且在 `biased` 的 `select!` 里把 tick 分支排在**最后**。只要每连接出站队列连续非空，`biased` 就会一直先命中断言优先级更高的出站分支 —— tick 驱动的三件事全部停摆：90 秒静默判定、慢消费者看门狗、心跳 `link.ping`。于是「恰恰在最需要保活的时候，保活不再运行」。
- 同构实现已在 sync 侧改对（`crates/server/src/sync/conn/session.rs` 用 `sleep_until(next_tick)` 跨轮保留 deadline 并把 tick 排在出站之前），node_link 侧当时是旧形态；本变更把两侧对齐。
- 约束：`docs/NODE_LINK_PROTOCOL.md` §2.5 的窗口（`heartbeatIntervalMs` 默认 30 秒、静默 90 秒）是**既有**要求，本变更只是让它们真正被实现；会话 tick 的粒度是 `SESSION_TICK`（500 ms）。
- 与变更⑥（`node-link-saturation-recovery`）的边界：那边改的是**饱和判据**（高/低水位迟滞与心跳/看门狗是否各自判定），这边只改**调度**（deadline 与分支顺序）。两者互相独立：判据再对，轮询不到也没用；轮询得到，判据错也照样误判。

## Goals / Non-Goals

**Goals:**

- 在出站队列**持续非空**的压力下，tick 仍按固定节拍被轮询：心跳按 `heartbeatIntervalMs` 发出、90 秒静默判定仍会推进到超时关闭、慢消费者看门狗仍会按宽限断开。
- 把调度口径（deadline 跨轮保留 + 必须排在出站之前，以及**为什么缺一不可**）写进模块文档，避免后来者顺手改回去。

**Non-Goals:**

- 不改 `crates/server/src/sync/**`（同构实现已修对，本次只作对照）。
- 不改饱和判据与低水位（属变更⑥），不改 `node-link-protocol`、schema、fixture、规格。
- 不修「写路径无期限」这一相邻缺口（变更⑥ 的任务 3.1 已登记，属另案）。

## Decisions

**D1 —— deadline 跨轮保留，且下一拍从本次 tick 的**时刻**起算。**
循环外建 `next_tick`，轮内用 `sleep_until(next_tick)`；tick 命中后 `next_tick = TokioInstant::now() + SESSION_TICK`。
理由：`now` 取的是 tick 真正发生的时刻，所以某一拍被别的就绪分支挤晚了一点，节拍也不会累积漂移；重建式 `sleep(SESSION_TICK)` 则会让「每次被唤醒都重新睡满一个周期」。

**D2 —— tick 分支排在出站分支之前（`biased` 固定顺序）。**
理由：D1 与 D2 **缺一不可**。只把 tick 提前而每轮重建 deadline，持续的出站流量会让它每轮被唤醒都重新睡满一个周期，永远凑不满；只保留 deadline 而把它排在出站之后，只要出站队列每轮都就绪，它连被轮询的机会都没有。

**D3 —— tick 的动作判定仍归纯函数** `tick_action(saturated_for, slow_consumer, ping_due)`（饱和段是否超宽限 → 断开；否则是否到点 → 发心跳；否则空闲），本变更不改它的判据，只保证它被按时调用。
理由：把判据与调度分开，才能分别用用例锁定（判据的锁定在变更⑥，调度的锁定在本变更的 1.2）。

**D4 —— tick 内的处置顺序：先判静默超时（关连接），再判动作。** 静默是连接级终态，动作是连接级处置；先终态后处置可以避免对一条马上要关掉的连接再发东西。
（与 `session.rs` 现有 Tick 分支的实现顺序一致，本变更不改。）

## Risks / Trade-offs

- [tick 粒度] 500 ms 只是轮询节拍，判定窗口仍由 §2.5 给出（30 秒 / 90 秒 / 宽限 30 秒）；最坏判定延迟 = 一个 tick。这是既有口径，不在本轮调整。
- [tick 优先的代价] 每一轮 tick 就绪时都会先做一次原子读 + 一次纯函数判定，出站投递因此最多被推迟这一点的量级；换的是「判据必然被轮询」。
- [用例的时序敏感] 「出站队列持续非空」的用例依赖压力源真的持续投递（`pressure.sent()` 有下界断言）；本模块此前有过因内核吸走字节而抖动的用例（`#111` 已按 32 KiB 填充修过），新增/改动必须复用同一填充口径。
- [与变更⑥ 的交叠] 两条变更都在 `conn/tests.rs` 里加用例：本变更的两条锁定「持续非空时 tick 仍发生」，变更⑥ 的锁定「饱和判据与恢复」。合入顺序不影响正确性（同一文件不同用例），但后合入者要重跑聚焦用例。

## Migration Plan

不涉及数据迁移、schema 变更或部署步骤：改的是会话循环的调度（deadline 的存放位置与 `select!` 的分支顺序），不改任何协议或持久化口径。回滚 = 逆向该提交；回滚后回到「繁忙连接上判定停摆」。

## 已知文档缺口

**无。** `docs/NODE_LINK_PROTOCOL.md` §2.5/§15 只规定窗口与处置语义（30 秒心跳、90 秒静默、达到高水位先停读、持续过慢断开），**没有**对「实现应当如何调度自己的轮询节拍」的表述，因此不存在与实现不符的条文；`docs/MODULE_ARCHITECTURE.md` 未描述 tick 的调度口径（按 `tick`/`心跳`/`静默` 检索无相关段落）。按任务 1.3 的要求，这里如实登记「未发现不符」，不动任何权威文档。
