<!-- 技术方案：解释「怎么修」。动机见 proposal.md（Why / What Changes），行为契约见 specs/ 的 delta。 -->

# Design

## Context

- 现状（修复前）：`ConnectionHandle::saturated()`（`crates/server/src/node_link/conn/registry.rs`）是**粘性**判定 —— `mark_saturated()` 只置位、`consumed()` 的清除点在实现上不可达（清除判据若出锁后再调 `saturated()`，读到的正是「标志仍置位」），且**没有低水位常量**。于是 `saturated_since` 一旦置位永不清除。
- 三层后果（proposal 的 Why 已列）：①「**持续**过慢」退化成「**曾经**过慢」；② 同一 Tick 分支里的 `else if now >= next_ping` 永久不可达 ⇒ 饱和期心跳停摆，一条已恢复的连接撞上 90 秒静默而被关闭；③ 受害面是读入门 —— `node_link/catalog.rs` 与 `resource.rs` 的 `handle.saturated()` 让该连接永久停读新的快照批次与事件批次。
- 约束（都是既有合同）：`docs/NODE_LINK_PROTOCOL.md` §2.5 定义了每连接待发送队列的高水位语义，且该上限**每连接可下调**（`node.ready.limits.maxPendingQueueBytes` / `maxPendingQueueMessages`，只可下调不可上调）；sync 侧的同构实现（`crates/server/src/sync/conn/registry.rs` 的 `RECOVERY_MESSAGES`/`RECOVERY_BYTES`）是口径对照，本变更不改它。
- 与本变更相关的一条**实测事实**（不是本轮引入的缺口）：传输层写路径没有期限，见文末「相邻项 3.1」。

## Goals / Non-Goals

**Goals:**

- 让「持续过慢」的判定真的描述**持续**：能被恢复清除，也能让持续过慢攒满宽限。
- 饱和期心跳照发：饱和不是静默。
- 受害面（`catalog.rs` / `resource.rs` 的读入门）在恢复后重新读批次。
- 补齐两侧覆盖：瞬时突发后回落（不得断开）与持续饱和（必须断开）各一条，且都断言其他连接不受影响。

**Non-Goals:**

- 不改 `crates/server/src/sync/**`（同构实现已在别处修对，本次只作对照），不改协议上限、schema、fixture。
- 不修「写路径无期限」（相邻项 3.1）：它需要改会话与传输层的收尾口径，属另一个变更。
- 不改 `docs/NODE_LINK_PROTOCOL.md`：发现的条文缺口如实登记，见文末。

## Decisions

**D1 —— 用迟滞（两道阈值）而不是单阈值。** 进入用 `≥` 高水位（占用达上限，或刚被容量判定拒过一次），退出用 `<` 低水位。
理由：单阈值两端都会出错 —— 只按「仍 ≥ 高水位」清除会让在高水位附近抖动的消费者永远攒不满宽限（**永不**断开，判定再次退化）；只累计不清除会把长期健康、偶尔突发的连接判死。两道阈值是这两端的折中。

**D2 —— 低水位由生效限额现算，不用编译期常量。** `low_water_messages` = 生效**条数**上限 ÷ `RECOVERY_WATERMARK_DIVISOR`（= 2）后抬到**至少 1**（wire 的条数下限就是 1 条，抬到 1 让「容量只有 1 条」这个边界仍可恢复）；`low_water_bytes` = 生效**字节**上限 ÷ 2，**不额外抬**（wire 的字节下限是 1 KiB，半值 ≥ 512 B，`max(1)` 没有意义）。
理由：Node Link 的队列上限是**每连接协商**的（只可下调），编译期常量会与实际生效值脱节。抬到 1 是为了让「容量只有 1 条」这个 wire 下限仍可恢复（进入判据是 `≥ 高水位`、退出判据是 `< 低水位`，仍能把 0 条与 1 条分开）。
与 sync 侧的差别只在来源（那边上限是常量）：取一半的口径一致。

**D3 —— 清除必须在同一临界区内「先算出结论、再清除」。** `consumed()` 在持锁期间同时递减占用、判定是否低于**两个**低水位，只在这个布尔为真时才清 `saturated_since`。
理由：清除判据读的正是待清的那个标志与同一份占用账本；把它拆成「出锁后调 `saturated()`」就是原缺陷的形态（清除点不可达）。

**D4 —— 饱和段计时只在两端动作。** 起点：首次被容量判定拒绝的时刻（`mark_saturated`）；终点：占用回到低水位之下的时刻（`consumed`）。
理由：抖动 + 真持续过慢必须能被断开（计时连续累计），瞬时突发必须不被判死（真的排到低水位之下即清除）。`saturated()` 对「刚被拒过」也算饱和：那条消息根本没入队，但调用方应当先停下来。

**D5 —— Tick 分支里心跳与看门狗各自独立判定**（`tick_action(saturated_for, slow_consumer, ping_due)`），不写成 `if saturated { … } else if due { ping }`。
理由：后者让饱和分支一旦命中就把心跳变成永久不可达 —— 那正是「饱和期心跳停摆」的成因。饱和只影响看门狗的判据（当前饱和段是否超过宽限），不影响心跳是否该发。
附带口径：ping 因队列顶满被拒（`SendFault::HighWater`）不算「心跳停摆」，这一轮不发出、下个 tick 重试，生死由看门狗按宽限判定（瞬时突发因此有机会自行排空）。

**心跳在饱和期的可观测口径（如实说明）。** 队列**持续位于高水位之上**时，`link.ping` 与被撑满的队列共用同一个有界队列，`ConnectionHandle::send` 会以 `SendFault::HighWater` 拒绝它，经 `heartbeat_outcome` 映射为一级处置「推迟」（这一轮不发出、下一拍重试）——因此**该状态下不会有 ping 真的发出**，「饱和期间真的收到 ping」不可观测。被保证、也被用例锁定的是三件事：**心跳判定不被短路**（`tick_action` 到周期时仍给出 `Ping`）、**被拒按推迟处置而不是结束连接**、**断开只由看门狗按宽限判定**。delta spec 的措辞按此写准（见 `specs/node-link-owner-server/spec.md` 的「饱和期间心跳判定不被短路」）。

**D6 —— 读入门用活判定，不做锁存。** `catalog.rs` 的批间门与 `resource.rs` 的 chunk 门每批现算 `session.saturated()`。
理由：锁存正是「恢复后永久停读批次」的成因；活判定的代价只是一次原子读。半途中止的批次由 Access 重新 `catalog.subscribe`/`resource.subscribe` 补齐（§2.5 的既有语义），因此不需要额外的续传状态。

## Risks / Trade-offs

- [低水位取一半是否合适] → 与 sync 侧同口径（那边是 `RECOVERY_*` 常量），且退出判据在**同一临界区**内判定，不存在「两处判据不一致」的窗口。
- [判据看的是「队列占用」，不是「对端是否真的在读」] → 这是本判据的已知边界：对端停止读取时队列会顶到高水位并触发饱和，但会话能否**执行**那次断开取决于它有没有被轮询到（相邻项 3.1：写路径没有期限，写一旦卡住 tick 就停摆）。本变更只保证判据本身正确、可恢复，**不**解决调度被写阻塞的问题。
- [宽限粒度] → 看门狗按会话 tick 轮询（`SESSION_TICK` 500 ms），最坏在宽限 + 一个 tick 之后断开；这是既有口径，不在本轮调整。
- [恢复后停读的残留] → 恢复后由 Access 重新订阅补齐；`resource.rs` 的半途快照由 `snapshot_unavailable`/重订阅语义兜住，不在本轮改。

## Migration Plan

不涉及数据迁移、schema 变更或部署步骤：改的是进程内的**连接级**判定（水位置位/清除）与两处读入门。回滚 = 逆向该提交；回滚后回到「曾经过慢即断开」与「恢复后永久停读批次」。

## 已知文档缺口（如实登记，本变更不顺手改）

`docs/NODE_LINK_PROTOCOL.md` §2.5 只写「待发送队列达到高水位后先停止读取新的 snapshot 批次；持续过慢时发送 `link.backpressure`（`post_mvp`）并断开」，**没有**「饱和状态可恢复（低水位迟滞）」与「饱和期心跳照发」两条条文 —— 它们已写进 `node-link-owner-server` 的 delta spec，但协议文档本身仍停留在「达到高水位就停读 / 持续过慢就断开」的表述。`docs/MODULE_ARCHITECTURE.md` 未描述该口径（无冲突）。属任务 1.5 的「如实登记」项。

## 相邻项 3.1 的判定与证据（另案，本变更不修）

**判定：是 —— 写路径没有期限，一旦缓冲填满，写 future 会无限期 Pending；会话把它内联在业务循环里，因此同一 `select!` 的 tick 全部得不到轮询。**

- 代码依据：`crates/server/src/transport/net/ws.rs` 的 `send_text` 就是 `SinkExt::send` 的裸 await（没有任何 `timeout`），`recv` 同样没有读期限；`crates/server/src/node_link/conn/session.rs` 的 `Event::Outbound` 分支**内联** `write_now(&outbound.text).await`，而心跳、90 秒静默判定与慢消费者看门狗都在同一个 `select!` 的 tick 分支里 —— 写一旦挂住，它们不会被执行。
- 实测证据（本轮在真实 loopback listener 上跑的**两次临时探针**，验证后已删除；形态：注册一个写大帧的 `WsHandler`，客户端完成升级与一条触发帧后一个字节都不读，写入用 3 秒 `timeout` 包裹）：
  - **会话的真实写形态（32 KiB 小帧连续写，明文 listener）**：只写了 **7 条（≈224 KiB）就停住**（3 秒内未完成）—— 与仓库既有记述一致（`crates/server/src/node_link/conn/tests.rs` 的用例文档记「填满 socket 只需约 10～30 次 32 KiB 写」「本机 loopback 实测 163,800 B ≈ 546 条」）。
  - **单条超大帧（64 MiB）一次性写（同一明文 listener）**：返回 `Ok`、耗时 14.37 ms —— 这是**写形态的伪影**（内核一次性吸收了那记巨写），既**不代表对端收到**，也**不能**拿它推断会话流量下的容量。本稿早先据此写过「明文能吸收几十 MiB」，那是错的：判据形态换成 32 KiB 小帧后，容量立刻回落到数百 KB。
  - **TLS（`TlsMode::Direct`）**：写 1 MiB 与写 64 MiB **都在 3 秒内没有完成** —— 默认部署形态是 WSS（`NetConfig::default()` 的 `TlsMode::Proxy` 表示 TLS 在上游终止，两种形态下本缺口都存在）。
- 影响（按会话的真实写形态计）：socket 缓冲在**数百 KB**量级就被填满，而每连接出站队列上限是 **8 MiB** —— 因此对端停止读取后，写会在队列还远未排空时就停在 `send_text` 上：会话的 `select!` 停摆，心跳、90 秒静默判定与慢消费者看门狗都得不到轮询，连接不会被断开（这也正是仓库用例文档记下「「段 ≥ 宽限」与「连接还在」为什么能同时成立」的原因）。同一缺口对 `sync` 与 `node_link` 一样成立（它们共用 `transport::net` 的写路径）。
- 影响范围：`node_link` 与 `sync` **共有**同一写路径（都走 `transport::net`），不是本变更引入。
- 修法建议（供另案，需单独变更与验证）：给写加期限（例如 `timeout_at(now + SLOW_CONSUMER_GRACE, send_text)`，超时按慢消费者收尾），或把「对端是否真的在读」纳入饱和判据（按 socket 写完成的字节数记账，而不是队列占用）。两种都会改变现有断言，不能塞进本次修复。
