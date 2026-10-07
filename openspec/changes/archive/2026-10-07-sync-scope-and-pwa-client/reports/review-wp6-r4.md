# WP6（MU3c）第 4 轮独立复核 — review-wp6-r4

- 被复核对象：`D:\Project\acp-remote\.worktrees\wp6` @ **`4ada2a2`**（本轮基线 `3875f6f`，总基线 `263d3ba`）
- 复核方式：只读检视 + **自建探针**（`Server93` 等，`zzprobe*-review.test.ts`，用完已删）；未复用作者的 `attachReplayServer`
- 纪律：`git status --porcelain` 结束为空；源文件回退已还原；全量套件复跑 **17 文件 / 269 passed**
- 日期：2026-10-05

---

## 0. 结论

**FAIL（可带 2 项 P1 修复合入）**。

本轮**确实**修掉了前三轮的根因，而不是症状：`evaluate` 的 `last + 1` 连续性判定被**结构性删除**，
「服务端已声明过的水位之下又送来一条从未呈现过的事件」（`late`）是本轮新增的、且是**唯一**的丢失判据。
这一点我独立判定为**正确且与 §9.2/§9.3/§9.5 一致**：它不再把协议明文允许的 scope 空洞误报成丢失。

P1-A（`advanceTo` 吞掉已投递事件）与 P1-B（退避上限不可达）**均已真关闭**，我用自建探针分别证伪了修复前
的行为并确认修复后成立。既有测试的改写我逐条对照过：**没有一条实质性变弱**（见 §5 表格）。

但本轮**新引入 2 个 P1**，都落在同一个位置——**快照屏障路径**（`connection.ts` `#onSnapshotEnd` →
`client.ts` `#onConnectionEvent`）：

| # | 问题 | 位置 |
| --- | --- | --- |
| **P1-N1** | 账本拒绝快照屏障（越界 / 跨 epoch）后，连接层正确地不采纳，但**门面 `#ackedCursor` 仍被那条被拒的游标覆盖**——恢复点被污染 | `client.ts:261-265` + `connection.ts:699-702` |
| **P1-N2** | 被污染的越界游标进入下一条连接的构造期 `resumeFrom`，`EventLedger` **抛错**，而抛错点在 `#onAuthenticated` 内（socket 回调栈）→ 客户端再也无法建立连接，**永久卡死** | `dedupe.ts:176-178` ← `client.ts:245-247` ← `connection.ts:426` |

两者构成一条**可复现的完整失效链**：一次畸形（或故障）快照 → 恢复点被污染 → 重连抛错 → 同步能力永久丧失。
这正是本轮报告 §7「刻意保留的取舍」没有覆盖的路径。

MINOR-1/2/3 **真关闭**（但 MINOR-2 的防线只打在 `caught_up` 与账本入口，**漏了门面这一层**，即 P1-N1）。
CRITICAL-1/2/3、MAJOR-2、NEW-2/3/4/5 无回归。`contractDigest` 复算一致。

---

## 1. 新口径是否正确：独立判定

### 1.1 协议一致性：判定为**一致**

我逐条对照 `docs/SYNC_PROTOCOL.md`：

| 协议文本 | 实现 | 判定 |
| --- | --- | --- |
| §9.2「全局 sequence 对单个设备可以有不可见的空洞，**不能据此推断隐藏事件**」（`:658`） | `dedupe.ts:203-227` 的 `evaluate` **没有任何连续性分支**；`received !== last+1` 这一推理链在代码里已不存在 | ✅ 一致。这正是前三轮翻车的根因，本轮被消除 |
| §9.3 第 4 步「发送 `sync.caught_up`，之后继续实时事件」 | `advanceTo` 采纳之（`dedupe.ts:268-273`） | ✅ 一致 |
| §9.5「即使某段 global sequence 全部因权限过滤而不可见，客户端也可以 ACK `sync.caught_up.cursor`」 | 屏障取 `head`（含不可见段），`connection.ts:627-636` | ✅ 一致。**这条是本轮口径成立的关键**：它证明「屏障之下未投递 = 不可见」是协议承认的语义，客户端无权推断丢失 |
| §9.5「ACK 是最高已连续处理的可见事件 cursor」+「ACK 只能前进」 | `late` 不发 ACK、不抬高水位（`connection.ts:556-558`）；`commit` 取 max（`dedupe.ts:234-236`） | ✅ 一致 |
| §9.2 `sync.cursor_invalid` `epoch_mismatch` | `advanceTo` 拒跨 epoch（`dedupe.ts:270`） | ✅ 一致 |

**结论**：新口径不是「自洽但不符合协议」，而是**与协议一致**。作者指出「客户端不靠数序号推进水位」是
正确的——数序号等于把 §9.2 禁止的推理换个地方再实现一遍。

### 1.2 自建探针读数（`Server93`：按 §9.3 文本逐条建模）

我自建的 `Server93` 与作者的 `attachReplayServer` **无任何共享**：它实现 §9.3 的四步
（读游标之后的**可见**事件 → 升序发 `event` → 无缝 barrier → 发 `caught_up` → 继续实时），
barrier 取 `head`（依 §9.5 允许覆盖不可见段），并额外支持 `withheld`（服务端明知可见却拒不投递）。

| 场景 | 探针 | 读数 |
| --- | --- | --- |
| scope 空洞（2/3/4 不可见，5..12 可见，barrier@12） | 探针 A | `deliveredToClient=[5..12]`；`presented=[5..12]`（**完全一致**）；`gaps=0`；`timers=0`；`subscribes=1`；`acked=12`；随后实时 20 正常呈现，`subscribes` 仍为 1 |
| 服务端重放中**真漏投** seq 4（barrier 仍发 9，违反自身声明），事后补投 | 探针 B | 首轮 `presented=[2,3,5,6,7,8,9]`（4 从未投递）；补投后 `presented=[…,4]` **被补齐**，`gaps=1`；再投判 `duplicate`，不二次呈现 |
| 同一 eventId 重复投递 **40** 次（含 20 轮整轮重放） | 探针 B | `presented=[5]`（**仅一次**）；`duplicate_dropped=40`；`gaps=0`；`sync.ack` **仅 1 条**（ACK 未回退） |
| 服务端**乱序**投递（先 9 后 5） | 探针 B | `presented=[9,5]`，5 **未被丢弃**；`gaps=1`（如实报告，不静默） |
| barrier 不前进 + 逐轮补投 6 条漏投事件 | 探针 C | `delays=[250,500,1000,2000,4000]`（**真实递增，逐毫秒测得**）；`subscribes=6`（=1+5）；`pendingTimers=0`；`gap_resubscribe_exhausted` 已上报；`presented == deliveredToClient`（**逐条相等**），且**无重复** |
| barrier 抬高水位（15→40）后的下一段抖动 | 探针 C | `acked=40`；新一段第 1 次退避即 **250ms**（249ms 不触发、250ms 触发）→ 预算**真复位** |
| `late` 补齐后的副作用 | 探针 D | 重订阅游标 = **最后确认位置**；ACK 序列严格单调；出站序号 `[1..n]` 严格递增无回退 |

### 1.3 「同时成立」的防守：两条 R14 要求各自的可证伪场景

这是本轮最关键的一问——纠正连续性假设最自然的失败方式是「不再相信序号连续性 → 真丢也检测不到」。
我构造了能**分别证伪**其中一条的场景：

**证伪「MUST NOT 出现丢失区间」的探针（探针 B，漏投 → 事后补投）**：
修复前（`3875f6f`）这类事件被当作 `duplicate` **静默丢弃、永久不可恢复**——正是 r3 reviewer 判的
「安静的丢失」。修复后被检出为 `late` 并**补齐呈现**。→ **该要求本轮真实成立**。

**证伪「重复投递 MUST NOT 造成重复呈现」的探针（探针 B，40 次重投）**：
`eventId` 去重键未变，40 次重投只呈现 1 次，ACK 只发 1 条。→ **该要求仍成立，无回归**。

**两条同时成立的实测（探针 C 末尾）**：`presented` 序列**逐条等于** `deliveredToClient`，
且 `new Set(presented).size === presented.length`（无重复）。**「不丢」与「不重」在同一序列上同时成立。**

---

## 2. 两条 r3 P1 的判定

### 2.1 P1-A（r3：`advanceTo` 把已投递但被判 gap 的可见事件永久推进水位并丢弃）

**判定：真关闭。**

我核实了作者的两条声称，逐条成立：

1. **「gap 丢弃分支已删除」**：`connection.ts#onEventMessage` 里 `gap` 分支不存在了；
   `late` 一律 `commit` + `event` 补齐（`connection.ts:551-558`）；`advanceTo` 也不再接收
   「已丢弃序号」参数（`dedupe.ts:269` 只有一个 `cursor` 形参）。
2. **「每条非重复 verdict 在同一同步栈内 commit + 呈现」**：`#onEventMessage` 是同步函数，
   `ledger.commit()` → `onEvent({kind:"event"})` 在同一栈内连续执行（`connection.ts:551-558`、`561-564`）。
   屏障 `#onCaughtUp` 与快照 `#onSnapshotEnd` 各自是一次**独立的入站调用**——
   `#onMessage` 的 `switch` 是同步分派，不会在某条事件的呈现过程中插入。

**探针 D 独立验证**：构造「服务端重放漏投 4 → 事后补投 4」，补齐后 `acked=9`（水位未前进）、
重订阅游标 = 9、ACK 单调。**没有出现「已投递但未呈现」的中间态。**

> 残留观察（非缺陷）：`#onSnapshotEnd` 内 `await this.#staging.complete(message)` 之后才 `advanceTo`，
> 期间其他入站消息不会被处理（`acceptChunk` 同步段无 `await`，CRITICAL-1 仍成立），因此不存在交错窗口。

### 2.2 P1-B（r3：`#onCaughtUp` 无条件清零 `#gapAttempts`，退避上限在合规服务端下不可达）

**判定：真关闭。**

`advanceTo` 的返回值确实从 `boolean` 升级为三态（`dedupe.ts:255-259`：`advanced` / `already_covered` /
`rejected`），`#gapAttempts = 0` 只在 `advance.kind === "advanced"` 时执行（`connection.ts:634`），
以及呈现了一条 `new` 事件时执行（`connection.ts:563`）。`late` 分支**刻意不清零**（`connection.ts:559-560`）。

**关键一问：「§9.3 要求每次重放后都发 `caught_up`，在此前提下退避上限是否真可达？」——我的探针 C 回答：可达。**

`Server93` 建模的正是 §9.3 合规服务端（**每轮重订阅都重放 + 都发 `caught_up`**），head 固定在 15，
因此每轮的 `caught_up@15` 都是 `already_covered`（水位不前进）→ 预算不清零 → 实测
`delays=[250,500,1000,2000,4000]`、`subscribes=6`、`exhausted` 已上报、`pendingTimers=0`。
作者报告里「探针 2 修复前 301 次 subscribe / exhausted 0 / delays 恒 250」的形态，我在修复后代码上
**不可复现**（该路径已不存在丢失信号）。

---

## 3. 判别力：作者自报差分的独立复现 + 六条断言的独立判定

### 3.1 `attachReplayServer` 是否真的建模了 §9.3

**判定：是，与前三轮的被动注入替身有本质区别。**

它在握手**之前**包装 `socket.send`（`connection.test.ts:167-183`），对每条 `sync.subscribe`：
① 按订阅游标升序重放**可见**事件（② 用 `lastCursor` 过滤 ≤ 游标的，即模拟「从游标**之后**开始」）；
③ `queue.push(caughtUp(head))`，barrier 取 `head`；④ 应答进队列由 `drain()` 交付（不递归）。
这四条正是 §9.3 的四步。r3 reviewer 点名的「响应式服务端」用例恰好不发送 `caught_up`——本轮已补上。

### 3.2 判别力差分的独立复现

作者自报：**修复前 15 failed / 126 passed（sync-client 子集）**。我独立复现（把 `dedupe.ts` /
`connection.ts` / `client.ts` / `wire.ts` / `index.ts` 五个源文件 `git checkout 3875f6f --`，
保留全部新用例，跑全量 `npx vitest run`）：

```
Test Files  3 failed | 14 passed (17)
     Tests  15 failed | 254 passed (269)
```

15 条红逐条核对与作者列表**完全一致**（5 条 `connection.test.ts` + 9 条 `dedupe.test.ts` +
1 条 `client.test.ts`）。恢复后全量 **17 文件 / 269 passed**，`git status --porcelain` 为空。

口径说明：作者报的 **126 passed (141)** 是 **sync-client 子集**（8 文件）。
我复现的 **254 passed (269)** 是**全量**。两者一致——子集 141 = 15 failed + 126 passed，
全量 269 = 15 failed + 254 passed。**作者口径正确，不是夸大。**

### 3.3 六条断言的独立判定

| # | 断言 | 独立判定 | 依据 |
| --- | --- | --- | --- |
| 1 | 服务端模型符合 §9.3 | ✅ **成立** | 我的 `Server93` 独立建模四步；`attachReplayServer` 亦成立（§3.1） |
| 2 | 存在因 scope 过滤而不可见的事件 | ✅ **成立** | 探针 A：`library` 中 2/3/4 显式 `visible=false`，且服务端**不投递**它们 |
| 3 | 每条服务端投递的可见事件都被呈现 | ✅ **成立** | 探针 A/B/C：`presented` 序列**逐条等于** `deliveredToClient` |
| 4 | 重订阅有界 + 退避真递增 + 到顶后真**停止** | ✅ **成立** | 探针 C：`delays=[250,500,1000,2000,4000]`（逐毫秒推进测得，非推断）、`subscribes=6`、`pendingTimers=0`。**到顶后仍继续呈现**（`presented` 含全部 6 条漏投事件）——安全网只停重订阅，不停呈现 |
| 5 | 事后重放漏投事件**仍能补齐** | ✅ **成立** | 探针 B：漏投的 4 被补齐呈现且再投判 `duplicate` |
| 6 | 不存在「已投递但未呈现且不可恢复」 | ⚠️ **在有界记忆窗口内成立；窗口外有例外** | 见 §6 的 **P2-N1**（`#accountedFloor` 之上仍成立；之下是**已知取舍**，但作者对它的可达性描述过于乐观） |

---

## 4. 新发现的缺陷

### P1-N1：账本拒绝快照屏障后，门面 `#ackedCursor` 仍被被拒游标覆盖（恢复点污染）

**位置**：`clients/app/src/sync-client/client.ts:261-265`（污染点）
配合 `clients/app/src/sync-client/connection.ts:699-702`（拒绝点）

**事实链**（探针 I1/I3 实测，非推断）：

1. 服务端发来 `sync.snapshot_end`，其 `cursor.globalSequence = "999999999999999999999"`（越界）。
2. `#onSnapshotEnd`（`connection.ts:699`）调 `advanceTo` → `rejected(sequence_out_of_range)` →
   上报 `snapshot_barrier_sequence_out_of_range` 并 `return`，**正确地不调 `#moveAcked`**
   （`connection.ts:704` 不执行）。连接层这一侧是对的。
3. **但** `snapshot_verified` 事件是在**拒绝之前**发出的（`connection.ts:693`，
   紧接 `complete()` 之后、`advanceTo` 之前）。
4. 门面 `#onConnectionEvent` 收到 `snapshot_verified` 后**无条件**执行
   `this.#ackedCursor = event.snapshot.cursor`（`client.ts:264`）——
   **完全不看账本是否拒绝过这条游标**。

**实测读数**：
```
I1 rejected: [{"reason":"snapshot_barrier_sequence_out_of_range","detail":"999999999999999999999"}]
I1 facade ackedCursor: {"serverEpoch":"00384a03-…","globalSequence":"999999999999999999999"}
```
跨 epoch 的外来游标（`serverEpoch=99999999-…`, `globalSequence=4242`）同样污染门面：
```
F1 rejected: [{"reason":"snapshot_barrier_epoch_mismatch","detail":"4242"}]
F1 facade ackedCursor: {"serverEpoch":"99999999-9999-4999-8999-999999999999","globalSequence":"4242"}
```

**为什么这是缺陷**：MINOR-1 的修复目标是「账本 rejected ⇒ 连接侧不采纳」。连接侧做到了，
**但门面这一层没做到**，而门面 `#ackedCursor` 恰恰是**跨连接存活的恢复点**
（`client.ts:114-121` 注释明说「它必须活在门面上而不是某个 `SyncConnection` 实例里」，
且 `client.ts:167` 用它作下一条连接的 `resumeCursor`）。因此被账本明确拒绝的游标，
**仍然成了下一条连接的恢复点**——MINOR-1 的意图在跨连接尺度上未达成。

这与 `caught_up` 路径形成不对称：`#onCaughtUp`（`connection.ts:627-634`）把 `advanceTo` 放在
`#moveAcked` **之前**并据此短路；`#onSnapshotEnd` 发事件的顺序反了。

**影响**：见 P1-N2（可导致永久卡死）。

### P1-N2：被污染的越界游标在下一条连接构造期抛错 → 永久无法建立连接

**位置**：`clients/app/src/sync-client/dedupe.ts:176-178` ← `client.ts:245-247` ← `connection.ts:426`

**事实链**（探针 I2/J1 实测）：

1. 门面 `#ackedCursor = {serverEpoch: EPOCH, globalSequence: "999999999999999999999"}`（由 P1-N1 污染）。
2. 下一条 `connect()` → `resumeCursor: this.#ackedCursor`（`client.ts:167`）→
   `#ledgerFor(epoch)` → `new EventLedger({ serverEpoch, resumeFrom })`（`client.ts:247`）。
3. **epoch 相同** ⇒ `resumeFrom` 不被置 null（`client.ts:246` 只在 epoch 不同时置 null）⇒
   构造期 `parseSequence("999999999999999999999")` 越界 ⇒ **`throw`**（`dedupe.ts:176-178`）。
4. 该 `throw` 发生在 `#onAuthenticated` 内，而 `#onAuthenticated` 由 socket 的 `onMessage` 回调
   同步调用（`connection.ts:426-427`）⇒ 异常从 socket 回调栈逸出，**连接永远建立不起来**。

**实测读数**：
```
J1 threw: "resumeFrom 的 globalSequence 不可用：999999999999999999999（sequence_out_of_range）"
I2 reconnect subscribe cursor: {"serverEpoch":"00384a03-…","globalSequence":"999999999999999999999"}
```
注意 I2：重连时 `#onAuthenticated` 抛错前，客户端**已经把污染的游标发出去了**
（`#sendSubscribe` 在 `#onAuthenticated` 尾部，抛错发生在更早的 `ledgerFor`，
但探针显示 subscribe 仍带污染游标发出——说明异常路径下状态机进入不确定态）。
无论如何，**恢复点已永久损坏**：每条新连接都会在同一个点抛错，客户端**再也无法同步**。

**为什么是 P1**：这是**永久性能力丧失**（不是可恢复的失败），触发条件只需要**一次**畸形/故障快照，
且恢复点会被持久化（`#ackedCursor` 是「跨连接持有」的持久游标），因此**跨会话持续**。
`dedupe.ts` 构造期 `throw` 的设计取舍（「响亮失败优于静默把恢复点清零」）本身是合理的，
问题在于**上游把一条账本已经拒绝的游标写进了恢复点**——修 P1-N1 即可同时消除 P1-N2。

**修法方向**：`client.ts:261-265` 只在账本未拒绝该游标时才更新 `#ackedCursor`
（例如让 `snapshot_verified` 携带账本的采纳结论，或让连接层在拒绝时不发 `snapshot_verified`）。

### P2-N1：`#accountedFloor` 之上仍成立，但作者对「不可检出」边界的描述过于乐观

**位置**：`clients/app/src/sync-client/dedupe.ts:292-302`（`#evictIfNeeded`）

作者在 §1.4 与 §7-2 把「记忆已释放段按重复丢弃」描述为「正常服务端不会重投已释放的那一段」，
并把它归为「有界记忆的唯一取舍」。我的探针 E1/E2/E3 与 H1 给出更精确的读数：

```
E1（retention=2）: 投 100(new) → 3(late) → 101(new) ⇒ 淘汰 FIFO 首个（seq 100）⇒ floor=100
E1 verdict for never-delivered seq 50: duplicate      ← 真正从未投递的 50 被静默丢弃
E2（retention=2，纯升序）: 1,2,3 ⇒ floor 抬到 2 ⇒ seq 2/1 判 duplicate、seq 4 判 new  ← 正确
E3（默认 retention=2000）: 2100 条事件后 seq 50 → duplicate  ← 需 2000+ 条才可达
H1（默认 retention=2000）: barrier@10000 + 3000 条实时事件后补投 5000 → presented 不变、gaps=0
   ← **补投被静默丢弃**，无 gap_detected、无重订阅
```

**E1 揭示了一个作者未提及的正确性问题**：`#evictIfNeeded` 抬界用的是「被淘汰条目**本身**的序号」
（`dedupe.ts:298-300`），注释（`dedupe.ts:288-290`）说这是为了「不把从未交付的 scope 空洞一并划进
已处理段」。但**恰恰相反**：一旦被淘汰的是一条**高序号**条目（因 `late` 事件的插入顺序打乱升序），
floor 就会被抬到那个高序号，从而把**夹在中间、真正从未投递**的低序号事件（E1 的 seq 50）一并划进
「已处理段」→ 静默丢弃。

- E2（纯升序、无 `late`）证明**常规路径**抬界正确；
- E1（有 `late` 打乱顺序）证明**该路径**抬界错误。

作者注释里的推理（「用剩余条目里最旧的会把空洞划进去」）在 E2 下成立，但在 E1 下**两种做法都错**——
问题不在抬界用谁，而在「用单条目序号代表一整段已处理」这个前提本身在乱序下不成立。

**为什么是 P2 而不是 P1**：需要 (a) 至少一条 `late` 事件**且** (b) 之后累计超过 `retention`（默认 2000）条
事件，才会实际吞掉一条真实漏投。属长连接 + 服务端持续违反 barrier 的组合条件。
但 H1 证明它**确实可达**，且症状是 r3 reviewer 判过的「安静的丢失」——**静默、无诊断、无重订阅**。

**修法方向**（可选，非阻断）：`#accountedFloor` 只应在**被淘汰条目是当前最小序号**时抬界，
或改为记录「已释放的 eventId 集合」而非单一序号；或在 `late` 事件被 commit 时把 floor 抬到**它自己**的序号
（而不是让后续淘汰把它拉高）。

### MINOR-N1（不阻断，记录）：跨 epoch `caught_up` 被拒后连接无前进通道

探针 G1：`rejected`（跨 epoch caught_up）→ 随后同 epoch 的正常事件与 `caught_up` 仍能前进
（`acked=5`、发出 `online`）。**行为正确**：外来 epoch 的屏障只被拒一次，同 epoch 的通道完好。
作者 §7-4 已声明「连接层无法自愈，真正的恢复是 §9.4 的 `sync.reset_required` 路径」。
**本轮无回归**。

### 判别力基础设施的不足（非缺陷，记录）

`attachReplayServer` 的 `withheld` 只能表达「服务端明知可见却不投递」，
**无法表达「服务端 barrier 撒谎」（barrier 低于实际 head）」。我的 `Server93` 用
「head 固定在 15 而库里存在 16..」间接覆盖了后者。属测试替身的能力边界，不构成实现缺陷。

---

## 5. 既有测试改写的逐条加严 / 放松对照

作者自报「删除的是错误前提，不是断言强度」。我逐条 diff `dedupe.test.ts` 与 `connection.test.ts`，
把被删除的断言与新增的断言**逐条对照**：

### 5.1 `dedupe.test.ts`

| # | r3 断言 | r4 断言 | 判定 |
| --- | --- | --- | --- |
| 1 | 「从续接游标起步：第一条必须是游标的下一条」——断言 seq 12 在游标 10 后判 `gap(expected:11)` | 「续接游标之后的序号跳跃直接接受」——断言 seq 12 判 `new`、水位到 12；并**新增**「空洞那条若后来补投 → 判 `late`」 | ✅ **前提被纠正，强度不减**：删的是与 §9.2 冲突的**错误前提**；新增的 `late` 断言比原用例更强（它验证了空洞事件的后续处置） |
| 2 | 「去重键不是连接序号：序号相同但 eventId 不同仍判**重复**」 | 「去重键不是序号：序号相同但 eventId 不同**不是**重复事件，按丢失证据补齐」——断言 `late` | ✅ **前提被纠正**。同一序号不同 eventId 在 §9.3 下是「服务端复用了事件位」，不是重投。断言仍精确匹配整个 verdict 对象（`{kind,watermark,received}`），强度不减 |
| 3 | 「缺口不推进游标，后续补齐后可继续」——3 段断言 | 「水位之下未呈现过的事件判 `late`；补齐后水位不动，再投判重复」——**5 段**断言（`late` verdict 全字段 / `commit` 后水位仍为 10 / `seenCount` / 再投判 `duplicate`） | ✅ **加严**（3 → 5 段，且新增 `seenCount` 与「再投不二次呈现」） |
| 4 | 「NEW-1：推进到屏障后，屏障之上的真缺口仍被判缺口」——断言 103 判 `gap(expected:101)`、101 判 `new` | 「采纳屏障后，屏障之上的序号跳跃仍**直接接受**」——断言 103 判 `new`、水位到 103 | ✅ **前提被纠正**（`last+1` 连续性在屏障之上同样不成立）。断言仍精确到水位值 |
| 5 | 「只前进不后退：更旧的屏障不得让水位回退」——`advanceTo(100)` 返 `false`，水位留 500 | 「重复或更旧的屏障是 `already_covered`」——`advanceTo(100)`→`already_covered`、`advanceTo(40)`→`already_covered`、水位留 100 | ✅ **等价且更具体**：`false` → 具名 verdict，且**新增了「重复屏障」这一 r3 未覆盖的情形** |
| 6 | 「跨 epoch 的屏障被拒绝」——`toBe(false)` | 「跨 epoch 的屏障被拒绝」——`toEqual({kind:"rejected", reason:"epoch_mismatch"})` + 水位断言 | ✅ **加严**（具名 reason） |
| 7 | （无） | 「越界与形状非法的屏障被拒绝」——新增 MINOR-2 用例 | ✅ **新增** |
| 8 | （无） | 「形状非法的续接游标在构造期就响亮地失败」——新增 | ✅ **新增** |
| 9 | （无） | 「水位之下的乱序投递同样判 `late`」——新增 | ✅ **新增** |
| 10 | 「超出保留窗口时按插入顺序淘汰最旧的键」——淘汰后重投判 `duplicate` | 同名用例，注释改为说明落在「已释放的记忆段」 | ⚠️ **等价**（断言未变）。**注意**：这个用例用的正是 E1/E2 的抬界路径，而它只覆盖 E2（纯升序）形态，**没有覆盖 E1（`late` 打乱顺序）形态**——即 P2-N1 未被任何用例捕获 |
| 11 | 「首次同步从序号 1 开始；**不从 1 开始判缺口**」 | 「首次同步：没有水位时任何序号都直接接受」 | ⚠️ **断言未变**（仍是 seq 1）。标题说得更宽（"任何序号"）但**断言仍只测 seq 1**。属标题与断言不匹配，非放松 |
| 12 | 「大序号按十进制字符串比较而非字典序」 | 原样保留 | ✅ 未变 |
| 13 | 「同一 eventId 重复投递判为重复」/「去重键是 eventId：messageId 不同…」 | 原样保留 | ✅ 未变 |

### 5.2 `connection.test.ts`

| # | r3 断言 | r4 断言 | 判定 |
| --- | --- | --- | --- |
| 1 | 「序号出现缺口时不推进游标并上报 gap」——断言 `gap_detected(expected:2,received:5)`、ACK 0 条、acked 留 1 | 「scope 过滤造成的序号空洞不判丢失」——断言 `gap_detected` **不存在**、`pendingTimers=0`、`presented=["5"]`、ACK **恰好 1 条**、acked=5 | ✅ **前提被纠正，强度不减**：删的是与 §9.2 冲突的断言；新用例断言更多（含 ACK 条数与定时器数） |
| 2 | 「判 gap 后从最后确认游标重发 subscribe」——断言 `pendingTimers=1`、250ms 后 subscribes=2、游标=1、出站序号="2" | **已删除**（无替代于同一层） | ⚠️ **删除**。但**其意图已被保留并加强**：r4 的「水位不前进时预算不被 caught_up 清零」用例逐轮断言重订阅游标 = 最后确认位置（15）、且逐轮断言出站序号不重复。**能力未丢失** |
| 3 | 「重新订阅后从断点续传的事件可正常处理」——断言 1 条 event、acked=2 | **已删除** | ⚠️ **删除**。但**意图被 §5.2-4 的主用例覆盖**（每条服务端投递的可见事件都被呈现）。**能力未丢失** |
| 4 | （无） | 「每条服务端投递的可见事件都被呈现；合法空洞不触发重订阅；漏投可检出且能补齐」——**7 段**（(1) presented 精确序列 / (2) gap 0 条 + timers 0 + subscribeCount=1 / (3) acked 与 online 游标 / (4) 屏障后实时事件 / (5) 事后重放不重复呈现 + `duplicate_dropped` **恰好 3 条** / (6) 漏投补齐 + `gap_detected` **精确对象** / (7) 再投不二次呈现） | ✅ **大幅加严**（0 → 7 段，且用 `toEqual` 精确匹配序列与 verdict 对象，非「断言长度」） |
| 5 | 「响应式服务端：每次收到 subscribe 就重放同一条事件时，重订阅次数必须有界」——断言 `subscribes ≤ 6`、`pendingTimers=0`、**0 条 event** | **已删除** | ⚠️ **删除**。**其前提已被本轮纠正**（服务端重放同一条 seq 5 在 §9.3 下不再产生 gap 信号——探针实测 `presented=["5"]`、`gaps=0`、`subscribes=1`）。「0 条 event 是正确的」这个结论在新口径下**不再成立**，故必须删除。**不是放松，是前提失效** |
| 6 | 「连续缺口达到上限后停止重订阅，并上报可诊断的原因」——断言 `subscribes ≤ baseline+5` + exhausted | 「水位不前进时预算不被 caught_up 清零」——断言 `delays` **逐轮精确**（250/500/1000/2000/4000，且 `advanceBy(delay-1)` 不触发 / `advanceBy(1)` 触发）、`subscribes` 精确、每轮游标=15、exhausted、`pendingTimers=0`、末尾断言 presented 全集 | ✅ **大幅加严**：从「≤ 上界」变成「**逐轮精确退避序列 + 逐轮精确触发时刻**」。作者自报「退避递增序列与 exhausted 上报改为逐轮断言」**属实** |
| 7 | 「缺口补齐后重试预算清零，下一段抖动拿得到完整预算」 | 「屏障真正抬高水位时预算才复位」——先走完 6 条漏投耗尽预算 → `setHead(40)` 重放 → 断言 `advanceBy(249)` 不触发 / `advanceBy(1)` 触发 | ✅ **加严**（从「`pendingTimers=1`」变成「精确的 250ms 边界」） |
| 8 | 「caught_up 把账本推进到屏障后，下一条可见事件不再被判缺口」 | 同名保留，改标题为「恢复通道与水位对齐」，断言从「event 长度 1」改为 `presentedSequences == ["101"]` | ✅ **加严**（从长度断言改为精确序列断言） |
| 9 | （无） | 「跨 epoch 的 caught_up 不被采纳：不 ACK、不宣告 online、也不推进已确认游标」——**4 段**（acked 留 1 / ACK 0 条 / online **不存在** / `caught_up_epoch_mismatch` 已上报）——MINOR-1 用例 | ✅ **新增** |
| 10 | （无） | 「scope 过滤造成的序号空洞不判丢失」（见 §5.2-1） | ✅ **新增** |
| 11 | CRITICAL-3 / R12 阻断态 / R14 跨连接去重 / 纯函数判定 | **原样保留，未改一字** | ✅ 未变 |

### 5.3 改写是否放松？

**结论：否。** 逐条对照 24 条（dedupe 13 + connection 11）：

- **删除 3 条**（§5.2-2、-3、-5）：其中 -5 的前提已被本轮纠正的假设**取代**（服务端重放不再产生 gap 信号，
  「0 条 event 是正确的」在新口径下不成立），-2/-3 的**意图**被 §5.2-4/-6 的更强用例完整覆盖。
  **无一条是「因为实现做不到而删掉断言」。**
- **改写 6 条**：全部为「前提与 §9.2 冲突」或「加严」（3→5 段、≤ 上界→逐轮精确序列、长度→精确序列）。
- **新增 9 条**：覆盖 MINOR-1/2/3 与三条新的 `late` 语义。
- **未变 6 条**：一字未动。

**唯一值得记录的不匹配**：§5.1-11 的标题从「不从 1 开始判缺口」变成「**没有水位时任何序号都直接接受**」，
但断言仍只测 seq 1——**标题比断言宽**，属文档措辞，不是放松。

**作者自报「删除的是错误前提，不是断言强度」——经逐条核对，成立。**

---

## 6. 连带必修项的核实

| 编号 | 作者声称 | 我的独立核实 | 判定 |
| --- | --- | --- | --- |
| **MINOR-1** | 「账本 rejected ⇒ 连接侧不采纳 `#acked`、不发 ACK、不宣告 `online`，上报 `caught_up_epoch_mismatch`」 | `caught_up` 路径**完全成立**：`connection.ts:627-634` 先 `advanceTo`、被拒即 `return`，ACK 0 条、online 0 条、`caught_up_epoch_mismatch` 已上报（探针 + 用例双证）。**但快照路径未覆盖**：门面 `#ackedCursor` 仍被污染 | ⚠️ **`caught_up` 路径真关闭；快照路径遗漏 → P1-N1** |
| **MINOR-2** | 「`parseSequence` 复用 `wire` 的 `isDecimalString` 并加上界 `2^53-1`，同一防线覆盖 `commit()` 与构造期 `resumeFrom`」 | 核实：`wire.ts:178-181` 已导出 `isDecimalString`；`dedupe.ts:120-124` 的 `parseSequence` 复用之并加上界；`commit()`（`dedupe.ts:225-227`）与构造期 `resumeFrom`（`dedupe.ts:176-178`）都走它。`BigInt` 语义自洽：`SEQUENCE_UPPER_BOUND = BigInt(Number.MAX_SAFE_INTEGER)`，`>` 比较对 BigInt 正确；`"-5"` 被 `DECIMAL_PATTERN`（`^(0\|[1-9][0-9]*)$`）拒为 `malformed_sequence`。**但**：越界游标仍能经 `snapshot_verified` 进入门面恢复点（P1-N1），使这道防线在跨连接尺度上被绕过 | ⚠️ **防线本身正确；覆盖面有漏 → P1-N1** |
| **MINOR-3** | 「`nowMs` 缺省委托 `clock.now()`」 | 核实：`client.ts:122` `this.#now = options.nowMs ?? ((): number => options.clock.now())`。新增用例「未注入 nowMs 时快照时间戳实时读自 `clock.now()`」在差分中确实变红（15 条红之一）→ **有判别力** | ✅ **真关闭** |
| **MINOR-4** | 不改代码（`docs/FRONTEND_DESIGN.md:198` 比 spec 更严，`docs/` 不在 Write Scope） | 核实：`git diff --name-only 263d3ba..4ada2a2 -- docs` 为空 → 未触碰 | ✅ **确认** |

### 「不得回归」逐条独立核实（不只读作者自述清单）

| 项 | 独立核实方式与结果 | 判定 |
| --- | --- | --- |
| **CRITICAL-1**（`acceptChunk` 体内无 `await`） | `grep -n "await" snapshot.ts` → 唯一的 `await` 在 `:239`（`complete()` 内），`acceptChunk` 的同步段（`:186` 注释「校验 → 推进序号 → 合并资源，中间没有任何 `await`」）未变；本轮 diff **未触碰** `snapshot.ts` | ✅ 无回归 |
| **CRITICAL-2**（`#ledgerFor` 唯一构造点） | `grep -rn "new EventLedger" src --include=*.ts`（排除 test）→ **唯一命中** `client.ts:247`。本轮 `connection.ts` 仍只调 `this.#options.ledgerFor(...)`（`connection.ts:426`），未新增构造点 | ✅ 无回归 |
| **CRITICAL-3**（出站序号从 1 起、每连接重置、不抄服务端值、退避不消耗序号） | `#outboundSequence = 0`（`connection.ts:233`）+ `#nextOutboundSequence()` 先 `+= 1` 再 `String()`（`:808-809`）→ 从 1 起；退避重订阅经 `#sendSubscribe()` 走同一计数器（探针 D 实测出站序号 `[1..n]` 严格递增、无回退）；`sendCommand` 仍覆盖为客户端方向计数器 | ✅ 无回归 |
| **MAJOR-2**（门面 `#ackedCursor`） | `client.ts:117/123/142-144/167` 结构未变。**但**该字段的**写入路径**新增了一个不受账本约束的来源（P1-N1） | ⚠️ **结构无回归；写入路径新增缺陷** |
| **NEW-2/3/4/5** | 本轮 diff 未触碰相关代码路径（`connection.ts` 的 epoch 切换、快照竞态、账本重建分支结构未变）；全量 269 passed 包含这些用例 | ✅ 无回归 |
| **MINOR-5/6 / 两条 SUGGESTION** | 本轮 diff 未触碰对应代码；全量套件含其用例且通过 | ✅ 无回归 |
| **分层** | `npx vitest run src/layers.test.ts` → **9 passed**；本轮 diff 未新增任何 import，`clients/app/src/sync-client/` 无 `src/platform/**` 导入 | ✅ 无回归、无绕过 |

---

## 7. 其他核实项

### 7.1 写入范围

```
git diff --name-only 263d3ba..4ada2a2
```
→ 26 个文件，全部在 `clients/app/src/sync-client/`（20 个）与 `clients/app/src/state/`（6 个）。
**无越界。** ✅

### 7.2 `design.md` / `plan.md` 零改动

- worktree 内 `openspec/changes/` **只有 `archive`** —— 该变更目录不在 worktree 里，**定义上未被触碰**。✅
- `git diff --name-only 263d3ba..4ada2a2 -- openspec docs schemas crates scripts` → **空**。✅
- 主检出的规划文件未被本轮触碰：`plan.md` mtime `2026-10-05 07:25:18`、`design.md` mtime `2026-10-03 15:15:15`，
  两者都**早于**本轮提交 `4ada2a2`（commit 时间在当日晚间），且本轮 `git status --porcelain` 在 worktree 为空、
  主检出的变更目录是未跟踪状态（`?? openspec/changes/sync-scope-and-pwa-client/`），**本轮未写入主检出**。✅

### 7.3 `contract_digest` 复算（作者未能完成，本轮代为复算）

```
cd D:/Project/acp-remote
npx --quiet --no-install openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --json
```
```json
{ "result": "PASS", "stage": "plan",
  "contractDigest": "sha256:ecfc245c119d41b1b76a7bfdde9b11f93e751628a894f4b56f1fc2dba90e0e91",
  "requirementsDigest": "sha256:c2eb41a848e4e9bd24f4e03550709fe3a44c560574fe45c4bbf040fb34cf2752",
  "errors": [] }
```

**`contractDigest` = `sha256:ecfc245c…e0e91`，与作者登记值逐字一致，且 `errors` 为空、`result` 为 PASS。**
作者的「输入文件未变 ⇒ digest 定义上未变」推断**得到证实**（本轮补上了他缺的那次实跑）。✅

### 7.4 未验证项的定性

| 项 | 本轮判定 |
| --- | --- |
| **真实 daemon 在收到重订阅时到底重放什么仍未实测** | **仍是最主要的残余风险，但本轮的风险轮廓已变**。前三轮的盲区是「替身不建模 §9.3」，本轮 `attachReplayServer` 与我的 `Server93` 都按 §9.3 文本建模，**方向正确**；剩余的不确定性是「真实 daemon 是否也遵守 §9.5 允许 barrier 覆盖不可见段」这一点。若真实 daemon 发的 `caught_up` 取「本次重放到的最大**可见**序号」而非 `head`，水位会**偏低**——那不会造成丢失（只会让更多事件判 `new`），因此**不是阻塞性风险**。 |
| 服务端违反 barrier 声明的概率 | 无实测。但本轮设计**不依赖**该概率：`late` 路径「必须可达且必须有界」已被探针 C 独立验证 |
| `#accountedFloor` 以下段的重复投递 | 未对真实服务端实测。取舍已在 §4 的 **P2-N1** 中给出更精确的可达性刻画 |
| 端口真实实现（Digest/Clock/Transcript/HostIdentity/Random） | 不存在（WP7 组合根职责），本轮沿用替身 |
| 退避的真实时序（后台标签页节流） | 未实测；`FakeClock` 确定性推进是既定取舍 |
| 固定向量字节对拍 / `deps` / `advisories` / `secrets` | 归 TP3 / CI-only，本轮不声称 |

---

## 8. 合入建议

**建议：暂不合入。修掉 P1-N1（并因此消除 P1-N2）后合入；P2-N1 可作为同轮或下一轮的加固项。**

理由：

1. **本轮的核心价值成立且应予保留**。根因（`last+1` 连续性假设）被结构性消除，与 §9.2/§9.3/§9.5 一致；
   P1-A、P1-B、MINOR-3 真关闭；MINOR-1/2 在 `caught_up` 路径上真关闭；既有测试改写**逐条核对无一放松**；
   判别力差分我独立复现为 **15 failed**（作者口径正确）；`contractDigest` 复算一致。
   **不建议回退本轮。**

2. **P1-N1/N2 是本轮新引入的、且是永久性失效**。它们只影响快照路径，但一旦触发就是
   **同步能力永久丧失**（恢复点被持久化，每条新连接都在同一处抛错）。
   触发条件只需一次畸形/故障快照，不需要任何特殊时序。
   这与前三轮的问题同类——**都在「恢复通道」这条路径上**——因此应在合入前修掉，而不是留到后续。

3. **P1-N1 的修法很小**：`client.ts:261-265` 增加「账本是否拒绝该游标」的判断
   （例如把 `snapshot_verified` 的发出移到 `advanceTo` 之后，或让该事件携带采纳结论）。
   改动面限于 `connection.ts` 与 `client.ts` 各一处，不触碰 `dedupe.ts` 的新口径，
   因此**不影响本轮已验证的全部结论**，修后可沿用本报告的判别力用例直接复验。

4. **P2-N1 建议同轮加固**：E1 证明 `#accountedFloor` 的抬界在 `late` 打乱插入顺序时会把
   真正未投递的低序号事件划进「已处理段」。不修也能合入（需 2000+ 事件 + 服务端持续违反 barrier），
   但它产生的是 r3 reviewer 判过的那种「安静的丢失」症状，且修法同样是局部的。

5. **本报告的探针可复用**：`Server93` 的形态（barrier 取 `head`、`withheld` 集合、显式 `flush()`）
   建议直接用于修复后的复验；`attachReplayServer` 可补一个「barrier 低于 head」的能力以覆盖 P2-N1 的 E1 形态
   （当前替身无法表达该形态，故 P2-N1 未被任何既有用例捕获）。

---

## 9. 检查清单（复现命令）

| 项 | 命令 | 结果 |
| --- | --- | --- |
| 基线套件 | `cd .worktrees/wp6/clients/app && npx vitest run` | 17 文件 / **269 passed** |
| 分层 | `npx vitest run src/layers.test.ts` | **9 passed** |
| 判别力差分 | `git checkout 3875f6f -- dedupe/connection/client/wire/index.ts` 后 `npx vitest run` | **15 failed / 254 passed (269)**（子集 15/126 of 141） |
| 写入范围 | `git diff --name-only 263d3ba..4ada2a2` | 26 文件，全在 `sync-client/` + `state/` |
| 规划资产 | `git diff --name-only 263d3ba..4ada2a2 -- openspec docs schemas crates scripts` | **空** |
| contractDigest | `npx openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --json` | PASS，`sha256:ecfc245c…e0e91` |
| 纪律 | `git status --porcelain`（结束态） | **空** |

**自建探针**：`zzprobe-review.test.ts`（A/B/C/D，10 用例）、`zzprobe2-review.test.ts`（E/F/G/H，7 用例）、
`zzprobe3-review.test.ts`（I，3 用例）、`zzprobe4-review.test.ts`（J，2 用例）——共 22 用例，**全部通过**，
**用完已删除**，worktree 结束态干净。
