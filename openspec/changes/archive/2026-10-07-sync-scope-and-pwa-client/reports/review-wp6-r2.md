# WP6 第二轮独立复核（review-wp6-r2）

- 被检对象：`D:\Project\acp-remote\.worktrees\wp6` @ `545c895`
- 基线：`263d3ba`　第 1 轮交付：`2302c41`
- 第 1 轮报告：`reports/review-wp6-r1.md`（作背景，结论已独立重核）
- 作者报告：`reports/deliver-wp6-r2.md`
- 日期：2026-10-05
- 纪律：只读检视；所有临时探针已删除并还原，最终 `git status --porcelain` 为空、HEAD 未动（仍 `545c895`）。

---

## 0. 结论

**FAIL — 阻断合入。**

第 1 轮的 3 CRITICAL + 2 MAJOR **确实全部关闭**，且关闭方式是结构性的而非补丁式的；判别力差分我独立复现为
**13 failed / 241 passed (254)**，与作者自报完全一致；写入范围、分层、`tsc` 均干净。

**但修复引入了 2 个新的 P1 缺陷，其中 1 个是无界的重订阅风暴循环**，位于本轮新抽出的 `#sendSubscribe()` 上。
作者在 r1 复核建议里被明确要求「并对连续 gap 做退避/上限」，本轮未做；更关键的是新路径与
`sync.caught_up` 语义（§9.5 明文允许 ACK 不可见序号段）组合后会**必然**进入死循环，而不是边缘情况。

| 门禁 | 我的独立结果 |
| --- | --- |
| `npx vitest run` @ `545c895` | 17 文件 / **254 passed** |
| `npx tsc --noEmit` | 退出码 0，无诊断 |
| `src/layers.test.ts` | 9 passed，分层无绕过 |
| 判别力差分（6 源文件回退 `2302c41`） | **13 failed / 241 passed (254)** —— 与作者自报一致 |
| `git diff --name-only 263d3ba..545c895` | 26 文件，全部在 `src/sync-client/` 与 `src/state/` |
| `git status --porcelain`（复核结束） | 空 |

---

## 1. 五条阻断项逐条判定

| # | 判定 | 一句话 |
| --- | --- | --- |
| CRITICAL-1 | **真关闭** | 同步段消除了竞态，`Promise[]` 按 index 占位消除了摘要错链；我未能构造出新的顺序错位 |
| CRITICAL-2 | **真关闭** | `#ledgerFor` 已是唯一构造点，跨连接复用成立 |
| CRITICAL-3 | **真关闭** | 出站序号从 1 严格递增、按连接重置、不抄服务端计数器 |
| MAJOR-1 | **关闭但引入新问题（P1）** | gap 后确实重订阅了，但**无退避、无上限**，且与 `caught_up` 语义组合会进入**无界循环** |
| MAJOR-2 | **真关闭（有 P2 衍生问题）** | `#ackedCursor` 三点推进方向正确，但它与账本 `#lastSequence` 是两份会漂移的拷贝 |

### CRITICAL-1　快照 chunk burst 竞态 —— 真关闭

**修法核对**（`snapshot.ts:176-186`）：`acceptChunk` 的「校验 → 推进序号 → 合并资源」现在确实在**同一个同步段**内，
`this.#digest.sha256(...)` 的结果只被 `push` 进 `#chunkDigests` 而不被 `await`；`complete()`（`snapshot.ts:217`）
用 `Promise.all(this.#chunkDigests)` 按 index 等待。我逐行确认 `acceptChunk` 的完整函数体内**没有任何 `await`**，
因此「读 `receivedChunks`」与「写回 `receivedChunks + 1`」之间不可能被抢占。竞态在结构上消失。

**我额外验证的四个子问题，全部为阴性**：

1. **摘要未 settle 时 `complete()` 被调用** —— 无害。`Promise.all` 会等齐；我实测（P1 探针）在
   `acceptChunk` 的三个 promise 全部 pending 时直接调 `complete()`，结果 `NONE(verified)`，
   digest 校验通过。因为同步段已保证 `#chunkDigests.length === staged.receivedChunks`，
   `complete()` 的前置检查 `receivedChunks !== chunkCount`（`snapshot.ts:206`）必然在 digest 未齐时先失败，
   不会读到半个数组。
2. **`discard()` 与在途摘要的交互** —— 无污染。`discard()`（`snapshot.ts:233-237`）把 `#chunkDigests` 整数组换成新空数组，
   在途 promise 只是不再被等待，不会被错误地 join 进下一次快照。我实测（P2/P11 探针）：
   chunk0（sessions，1 条）被 discard 后开新快照再收 agents chunk，`complete()` 得
   `sessions: 0, agents: 1`，**旧资源零泄漏**，digest 也仍然匹配（用的是新快照自己的 rawText）。
3. **校验失败时已发起的摘要会不会错链** —— 不会错链，但**会产生一个孤儿 rejected promise**（P2，见 §3）。
   `#chunkDigests` 被整体替换，孤儿 promise 的 rejection 无人处理 → `unhandledRejection`。
   我实测（P10 探针）：`unhandledRejection count: 1`。这是本轮新结构的副作用，r1 的 `await` 版本不会有。
4. **序号已推进但摘要后来失败，暂存区是否自洽** —— **不自洽**（P3 探针）：
   `sha256` 抛错时 `complete()` 抛出的是普通 `Error` 而非 `SnapshotValidationError`，
   `hasPending` 仍为 `true`，`lastDiscardReason` 停留在上一次 `begin()` 写的
   `superseded_by_new_snapshot`（误导性诊断）。`connection.ts:536-542` 的 catch 只处理
   `SnapshotValidationError`，其余 `throw error` 向上抛进 `void this.#onSnapshotEnd(...)` 的无人处理 promise。
   严重度低于 CRITICAL-1 本身（摘要失败是罕见路径），记为 **P2**（见 §3）。

**R1 未覆盖到的场景**：`snapshot.test.ts` 与 `client.test.ts` 的新用例都是「先投完 chunk 再投 end」，
即 `complete()` 总在 digest settle 之后。我额外验证了 burst 路径下 `snapshot_end` **紧跟** chunk 0 到达的情形，
`Promise.all` 依然等齐，结论不变。

### CRITICAL-2　重连时去重账本被无条件重建 —— 真关闭

`client.ts` 里已无 `new EventLedger` 的第二处构造点：`#ledgerFor`（`client.ts:224-236`）是唯一构造点，
`resumeFrom` 取门面的 `#ackedCursor`，跨 epoch 时置 `null`。我 grep 确认 `new EventLedger` 在
`client.ts` 只出现这一次。

**我额外验证的三个子问题**：

1. **`serverEpoch` 变化时丢弃旧账本是否仍正确** —— **账本侧正确，但 `#ackedCursor` 侧错误**（P1 缺陷，见 §2 NEW-1）。
   `#ledgerFor` 正确地在跨 epoch 时把 `resumeFrom` 置 `null`，但**连接侧的 `#acked`/`#lastAcked`
   仍被 `options.resumeCursor` 初始化成旧 epoch 的游标**（`connection.ts:210-211`），且
   `#sendSubscribe()`（`connection.ts:396`）发的 `body.cursor` 就是这个旧游标。
   实测（Z2 探针）：epoch 从 `00384a03…` 切到 `11111111…` 后，第二条连接的首个 `sync.subscribe`
   body 仍是 `{"serverEpoch":"00384a03-bc90-4095-b65d-82fb8cc47e13","globalSequence":"5"}`。
   按 `docs/SYNC_PROTOCOL.md:660`，服务端对 epoch 不匹配的 cursor「一律返回 `sync.cursor_invalid`，
   `details.reason` 取 `epoch_mismatch`」。客户端会收到 `sync.cursor_invalid` 走 `#onProtocolError` 的
   `default` 分支，只发一个 `rejected` 事件、**不做任何恢复**，事件流停摆。
2. **跨 epoch 时 `#sendAck` 会被误判为回退** —— 同一根因的另一面。实测（Q2/Z3 探针）：
   epoch 切换后连发 3 条新 epoch 的事件，客户端产出 3 个 `rejected:ack_not_advancing`，
   **`sync.ack` 一条都没发出去**。因为 `compareCursors` 跨 epoch 恒返回 `-1`（`connection.ts:673`），
   `last <= 0` 成立 → `#sendAck` 直接 return。这直接违反 §9.5「客户端在事件已经进入其可恢复本地状态后发送累计 ACK」，
   且 §9.4 明确把「`serverEpoch` 不匹配」列为**必须重建快照**的正常场景，不是异常路径。
3. **有没有把「本应去重的新事件」误判成重复** —— 没有。`evaluate` 的重复判定依赖 `#seen`（eventId）
   与 `received <= last`，重连复用账本后 `#seen` 保留，重投仍判 duplicate。CRITICAL-2 的
   「重投已见事件判重复」场景我实测成立（`duplicate_dropped`，`event` 计数不增）。

### CRITICAL-3　出站 `connectionSequence` 恒为 `"0"` —— 真关闭

`#nextOutboundSequence()`（`connection.ts:632-635`）从 `0` 起 `+= 1` 返回 `String(...)`，首值 `"1"`，
严格加一。五个出站点（`subscribe:394`、`pong:442`、`snapshot_request:510`、`ack:625`、
`sendCommand:608`）全部改用它，`#onAuthenticated` 已不再读 `message.connectionSequence`。

**我额外验证的四个子问题，全部为阴性或可接受**：

1. **`sendCommand` 覆盖调用方占位值是否让调用方的序号语义失效** —— 否。调用方（`client.ts:212`
   的 `buildStatusQuery`）本就把 `"0"` 降级为**占位**，且 `client.ts:212` 与 `connection.ts:604-607`
   的注释都写明序号属于连接层。覆盖后 wire 上是合法递增序号，调用方拿到的 `CommandMessage` 对象
   本身未被 mutate（`{ ...command, connectionSequence: … }` 是新对象），因此重试时用原对象重发仍会被覆盖。
2. **未认证时不消耗序号是否与「严格递增」矛盾** —— 不矛盾。§4.1（`docs/SYNC_PROTOCOL.md:136`）
   明文「认证前消息省略」该字段，未认证消息根本不上 wire，谈不上消耗计数器。
   `sendCommand`（`connection.ts:604-606`）在 `#connectionId === null` 时原样发送、不递增，
   与 §4.1 一致。**一处细节**：`#connectionId` 在 `#onServerChallenge`（`connection.ts:312`）
   就被赋值，早于 `#onAuthenticated`（`:373`）。因此「已收到 challenge、未收到 authenticated」这个窗口内
   `sendCommand` 会**提前消耗序号**。但该窗口内业务上不应派发命令（`authenticatedInfo` 仍为 `null`），
   且 spec §8 规定认证阶段只允许握手消息。记为 **P3 观察**，不构成缺陷。
3. **计数器是否在重连后被正确重置** —— **正确**。`#outboundSequence` 是 `SyncConnection` 的实例字段，
   每次 `connect()` 新建实例（`client.ts:139-162`）→ 计数器归零。实测（P6 探针）：
   第一条连接出站 `["1","2"]`，断线重连后第二条连接出站 `["1"]`。§4.1 的语义是每连接每方向独立，重置是对的。
4. **认证前两条消息确实不带 `connectionSequence`** —— 实测通过（`client.test.ts:407-417` 用例 +
   我自己的检查，两条握手消息的 `connectionSequence` 与 `connectionId` 均为 `undefined`）。

### MAJOR-1　序号缺口无恢复路径 —— **关闭，但引入 P1 缺陷**

**关闭部分成立**：gap 分支（`connection.ts:478-481`）确实调用了 `#sendSubscribe()`，
恢复游标取 `#acked`（断点，不是缺口后的序号、不是 `null`），实测 cursor 为 `{…, globalSequence:"1"}`
且序号为 `"2"`。r1「静默停摆」确实被消除。

**但新引入了两个问题，其中第一个是无界循环。**

#### NEW-1（P1，CRITICAL 级行为缺陷）：gap 恢复与 `sync.caught_up` 组合会进入**无界重订阅循环**

根因是 `#acked` 与账本 `#lastSequence` 的**语义错配**：

- `#onCaughtUp`（`connection.ts:488-493`）把 `#acked` 推到 `caught_up` 的 cursor，
  门面侧 `#onConnectionEvent` 的 `online` 分支（`client.ts:263-265`）也把 `#ackedCursor` 推到同一值。
- 但 **`EventLedger.#lastSequence` 完全不感知 `caught_up`** —— 它只在 `commit()` 里推进（`dedupe.ts:113-116`），
  没有任何 API 让它接受一个外部游标。
- 于是 reconnect 时 `#ledgerFor` 用 `resumeFrom = #ackedCursor` 构造账本（首次连接时 `#ackedCursor` 为 `null`，
  账本 `#lastSequence` 保持 `null`），而 `evaluate` 在 `#lastSequence === null` 时**硬要求首条恰为 1**（`dedupe.ts:88-96`）。
- 服务端随后下发 `globalSequence: 101` → 判 `gap` → `#sendSubscribe()` 用 `#acked=100` 重订阅
  → 服务端按 §9.3 从 cursor 100 重放**同一条** 101 → 又判 `gap` → 再重订阅 → **永不停止**。

这条路径不是边缘情况：`docs/SYNC_PROTOCOL.md:658` 明文「全局 sequence 对单个设备可以有不可见的空洞」，
`:830` 明文「即使某段 global sequence 全部因权限过滤而不可见，客户端也可以 ACK `sync.caught_up.cursor`」。
**只要设备有任何一条事件因 scope 过滤而不可见，客户端就必然进入此循环。**

**我的复现**（R1 探针，一个遵守 §9.3 的响应式服务端：每个 `sync.subscribe` 都重放可见事件 101）：

```
R1 subscribe count: 303
R1 gap count: 302
R1 presented events: 0        ← 事件流彻底停摆
R1 ack count: 1
R1 runaway: true              ← 我在 300 次处主动熔断，否则不停止
```

R3 探针（`caught_up@200` 后 `event 201`）给出同一结论：`online` 之后立刻 `gap_detected`，
subscribe 计数从 1 涨到 2，`event` 计数为 0。

**影响**：(a) 违反 R14「MUST NOT 出现丢失区间」与「从最后确认的位置连续恢复」——循环里**一条事件都不会被呈现**，
比 r1 的「静默停摆」更糟，因为 r1 至少不会打爆服务端；(b) 无退避、无上限，
对服务端是持续的 `sync.subscribe` 洪泛（§4.1 的 `connectionSequence` 也会被单调消耗到很大值）；
(c) §8.5/`:613` 明确要求避免「重连风暴」，本构造是同一类问题。

**r1 复核建议原文就是「并在 gap 分支用 `#acked` 重发 `sync.subscribe`（**并对连续 gap 做退避/上限**）」，
本轮只做了前半句。** 作者在 `connection.ts:475-477` 的注释写「缺口必须**真的**恢复」，
但没有意识到「每次恢复都用同一个游标、服务端重放同一条事件」会构成不动点。

#### NEW-2（P2）：gap 恢复缺少退避/上限，一次异常即打爆服务端

即使排除 `caught_up` 组合，单纯重复 gap 也会线性放大。实测（P4 探注：连续投 50 条各自
`globalSequence:"5"` 的不同 eventId）：

```
P4 subscribes after 50 gapped events: 45 | gaps: 44
```

45 次 `sync.subscribe` 对 44 次 gap，**一对一、无节流**。一条乱序/丢帧就能触发同样后果。

### MAJOR-2　`resumeCursor` 是构造期常量 —— 真关闭（有 P2 衍生问题）

`#ackedCursor`（`client.ts:105`）跨连接存活，`connect()`（`client.ts:151`）与 `#ledgerFor`
（`client.ts:232`）都读它，不再读 `options.resumeCursor`。新增 `ackedCursor` getter（`client.ts:129-131`）
供 WP7 持久化。

**三个推进点的完备性核对**：

| 事件 | 是否推进 | 判定 |
| --- | --- | --- |
| `snapshot_verified` | `client.ts:245` | **正确**。快照 cursor 是 barrier，其后的 `event` 必然是 `cursor+1` 起 |
| `event` | `client.ts:249` | **正确**，取 `this.#ledger?.cursor`（由账本给出，不自己拼游标） |
| `online` | `client.ts:252` | **正确**（`caught_up` 是 §9.5 明确允许的 barrier ACK） |
| `duplicate_dropped` | 不推进 | **正确**（§9.5「ACK 是最高已连续处理的**可见**事件 cursor」） |
| `gap_detected` | 不推进 | **正确** |
| `reset_required` / `blocked` | 不推进 | 可接受（此时不该推进） |

**没有漏推进，也没有过度推进**。`ackedCursor` 与账本 `#lastSequence` 确实是两份拷贝，
但在**事件驱动**的路径上二者由同一次 `commit()` 同步推进（`connection.ts:485-486` 的
`ledger.commit(message)` → `#sendAck(cursor)` → `client.ts:249`），不会漂移。

**唯一的漂移点是 NEW-1 的根因**：`online` 与 `snapshot_verified` 推进了 `#ackedCursor`
却推进不了账本（账本无此 API），两份拷贝从这里开始分叉。这是 P1 的深层原因，
不是独立的第二个缺陷。

---

## 2. 新发现的分级清单

### NEW-1（P1）　gap 恢复与 `sync.caught_up` 组合导致无界重订阅循环，事件流彻底停摆

- **位置**：`connection.ts:478-481`（gap 分支调 `#sendSubscribe()`）、`connection.ts:396`（用 `#acked` 作恢复游标）、
  `connection.ts:488-493`（`#onCaughtUp` 只推 `#acked`）、`dedupe.ts:88-96`（`#lastSequence === null` 时硬要求首条为 1）、
  `dedupe.ts:113-116`（`commit` 是 `#lastSequence` 的唯一推进点）
- **触发条件**：设备存在任何一条因 scope 过滤而不可见的事件（§9.2/§9.5 明文允许），
  且服务端按 §9.3 对重订阅重放同一条可见事件
- **影响**：303 次 subscribe / 302 次 gap / **0 条事件被呈现**（我的复现数据）。
  违反 R14「MUST NOT 出现丢失区间」；对服务端构成持续洪泛
- **不修会怎样**：这是本包声称交付的「事件按 `eventId` 去重且不跳过」（R14）的**核心路径**，
  且触发条件是 spec 明确允许的常态，不是异常
- **建议**：给 `EventLedger` 增加一个接受外部游标的方法（让 `caught_up` / `snapshot_verified` 能把
  `#lastSequence` 推到 barrier），使 gap 判定与 `#acked` 对齐；并在 gap 分支加退避与连续次数上限。

### NEW-2（P2）　epoch 变化后订阅游标未清空 + ACK 永久被拒

- **位置**：`connection.ts:210-211`（`#acked`/`#lastAcked` 用旧 epoch 的 `options.resumeCursor` 初始化）、
  `connection.ts:396`（`body.cursor` 用该值）、`connection.ts:613-617`（`compareCursors` 跨 epoch 返回 `-1` → 判回退）
- **触发条件**：服务端 `serverEpoch` 变化（§9.4 列为必须重建快照的**正常**场景）后重连
- **影响**：(a) 首条 `sync.subscribe` 带旧 epoch 游标 → 服务端按 `:660` 返回 `sync.cursor_invalid`，
  客户端只发 `rejected` 不恢复；(b) 之后每条事件都产出 `rejected:ack_not_advancing`，
  实测 **3 条事件 → 0 条 `sync.ack`**，违反 §9.5 的 ACK MUST
- **与 CRITICAL-2 修复的关系**：`#ledgerFor` 已正确处理跨 epoch（`:232-233` 把 `resumeFrom` 置 null），
  但**连接侧没有跟上**——`#acked`/`#lastAcked` 与 `sync.subscribe` 的 cursor 仍取未清空的旧值。
  修复不完整
- **建议**：`#onAuthenticated` 里在 `ledgerFor` 返回新 epoch 账本时，同步把 `#acked`/`#lastAcked`
  置 `null`（与 `resumeFrom` 的处理对齐）

### NEW-3（P2）　digest promise 变孤儿，校验失败路径产生 `unhandledRejection`

- **位置**：`snapshot.ts:185`（`push` 摘要 promise 但不 `await`）、`snapshot.ts:233-237`
  （`discard()` 整数组换新，旧 promise 无人处理）
- **触发条件**：`DigestPort.sha256` reject，且在 `complete()` 到达前暂存区被 `discard()`
  （乱序 chunk / 断线 / 阻断）
- **影响**：实测 `unhandledRejection count: 1`。浏览器里表现为控制台错误与潜在的全局
  `unhandledrejection` 处理器误触发；Node 下按版本可能直接终止进程
- **这是本轮新结构的副作用**：r1 的 `await this.#digest.sha256(...)` 会把 rejection
  沿 `acceptChunk` 冒泡到 `#onSnapshotChunk` 的 catch，不会变成孤儿
- **建议**：`push` 时挂一个 `.catch(() => {})` 吞掉孤儿 rejection（真正的失败仍由 `complete()` 的
  `Promise.all` 抛出），或在 `discard()` 里对旧数组逐个挂 handler

### NEW-4（P2）　摘要失败不是 `SnapshotValidationError`，暂存区残留且诊断失真

- **位置**：`snapshot.ts:217-219`（`Promise.all` 抛出的是 digest 原始错误）、
  `connection.ts:536-542`（catch 只处理 `SnapshotValidationError`，其余 `throw error`
  进入 `void` 调用的无人 promise）
- **实测**：`P3 error: Error: digest exploded | hasPending: true | lastDiscard: superseded_by_new_snapshot`
  —— `hasPending` 仍为 `true`（暂存区没被丢弃），而 `lastDiscardReason` 停在**上一次 `begin()`**
  写的 `superseded_by_new_snapshot`，与本次失败毫无关系
- **影响**：`MINOR-2` 声称修好了「为什么没提交可观测」，但摘要失败这条路径上该字段给出的是误导值；
  暂存区残留会让下一次 `begin()` 之前的 `complete()` 有机会用到旧 chunk 集合
- **建议**：把 digest 失败包成带 `digest_mismatch` 之外专用 reason 的 `SnapshotValidationError`，
  或在 catch 里显式 `discard()`

### NEW-5（P3）　`begin()` 无条件写 `lastDiscardReason`，首次快照也报「被新快照取代」

- **位置**：`snapshot.ts:133-134`（`begin()` 开头无条件 `this.discard("superseded_by_new_snapshot")`）
- **实测**：`P3 lastDiscard: superseded_by_new_snapshot` —— 即使这是**第一次**快照、根本没有旧暂存区可取代
- **影响**：诊断字段在健康路径上给出假警报。严重度低（不影响行为），但与 NEW-4 叠加后
  这个字段的可信度进一步下降
- **建议**：`begin()` 里仅当 `this.#staged !== null` 时才记 `superseded_by_new_snapshot`

### 观察项（非缺陷，记录备案）

- **未认证窗口消耗序号**：`#connectionId` 在 `#onServerChallenge`（`connection.ts:312`）即被赋值，
  早于 `#onAuthenticated`（`:373`）。若业务在「已收 challenge、未收 authenticated」窗口内 `dispatch`，
  会提前消耗一个出站序号。该窗口内 `authenticatedInfo` 为 `null`、spec §8 也禁止业务消息，
  业务上不应发生，且序号仍严格递增。**不构成缺陷**。
- **计数器重置正确**：`#outboundSequence` 是实例字段，`connect()` 新建实例即归零（实测 `["1","2"]` → `["1"]`）。

---

## 3. 判别力差分（我自己复现）

把 6 个源码文件（`command-machine.ts`、`connection-machine.ts`、`client.ts`、`connection.ts`、
`paging.ts`、`snapshot.ts`）`git checkout 2302c41 --`，保留全部新用例，跑 `npx vitest run`：

```
 Test Files  6 failed | 11 passed (17)
      Tests  13 failed | 241 passed (254)
```

**与作者自报的 `13 failed / 241 passed` 完全一致。** 恢复后 `545c895` 重跑 **254 passed / 254**。

13 条红的分布与 5 条阻断项的对应关系（我逐条核对红点名）：

| 阻断项 | 抓住它的红用例 | 覆盖判断 |
| --- | --- | --- |
| CRITICAL-1 | `snapshot.test.ts > 三个 chunk 不经 await 连着投递仍按 index 收齐并验证通过`；`client.test.ts > 三个 chunk 背靠背投递（不逐个 await）仍能完成并替换仓库` | **覆盖**，且 socket 级那条是真 burst（`for` 循环不 await） |
| CRITICAL-2 | `client.test.ts > 连接 → 断 → 重连 → 重投已见事件判重复，且从最后确认位置续传`；`…> 重连后首个事件序号远大于 1 时不得判成缺口` | **覆盖**，两条分别对应「重复呈现」与「续传停摆」 |
| CRITICAL-3 | `client.test.ts > 认证后的出站消息序号从 1 开始严格递增，且不抄服务端计数器`；`connection.test.ts > 同连接内连续 ACK 与 subscribe 的序号不重复、不回退` | **覆盖**，且第二条断言的是精确数组 `["1","2","3","4"]`，不是恒真 |
| MAJOR-1 | `connection.test.ts > 判 gap 后从最后确认游标重发 subscribe（不静默停摆）`；`…> 重新订阅后从断点续传的事件可正常处理` | **覆盖「发了 subscribe」与「重订阅后能处理」，但不覆盖 NEW-1/NEW-2** |
| MAJOR-2 | 无独立用例；由 CRITICAL-2 第一条顺带断言（`subscribe.body.cursor === {…, "2"}`） | **覆盖**（断言真实） |
| MINOR-1/2/3、discard 原因、页边界 | 各有独立红用例（共 6 条） | 覆盖 |

**结论：5 条阻断项每一条都有至少一条真正能抓住它的用例，13 条红无一是恒真断言。**
新发现的 NEW-1 / NEW-2 属于**这 13 条红也覆盖不到的路径**（`caught_up` 后续事件、epoch 切换），
它们不是判别力造假，而是覆盖面边界——与 r1 的问题同类：作者修好了被指出的路径，
但相邻的组合路径仍无用例。

**无恒真断言、无 `toBeDefined()` 兜底、无复制被测逻辑**：`expectedSnapshotDigest`
（`harness.ts`）是独立复算；`connection.test.ts` 的序号断言是精确数组比较。

---

## 4. 七条 MINOR / SUGGESTION 的处置复核

| # | 作者处置 | 我的复核结论 |
| --- | --- | --- |
| MINOR-1 | **修**（删掉 4 个阻断态来源） | **成立**。`connection-machine.ts:161-179` 注释与表现在一致，`transition` 不再静默清空 `blocking` |
| MINOR-2 | **修**（`lastDiscardReason` + 2 个新 reason） | **部分成立**。新增 `connection_closed`/`connection_blocked` 正确（断线与阻断确实不是 `reset_required`）。但有 **NEW-4**（摘要失败时残留误导值）与 **NEW-5**（健康路径写假 reason）两条衍生缺陷 |
| MINOR-3 | **修**（`accepted` 也拒绝重试） | **成立**。`command-machine.ts:174-181` 语义正确，判别力用例存在 |
| **MINOR-4** | **不改代码**，理由是 `docs/FRONTEND_DESIGN.md:198` 比 spec 更严 | **成立——文档依据核实通过**（详见下） |
| MINOR-5 | **修**（注释分列两项 + 补页边界校验） | **成立**。`paging.ts:148-170` 的边界校验方向正确（`older` 的最新不得晚于 `newer` 的最早），注释也诚实标注了「不是页级完整性的证明」 |
| MINOR-6 | **修**（补前置条件注释 + 两条用例） | **成立**。`paging.ts:44-59` 把「服务端按 `(createdAt, messageId)` 升序」写成显式契约，并说明违反后的两种后果 |
| SUGGESTION-1 | **修**（改 `Date.parse` epoch 毫秒比较） | **成立**。语义等价（固定形状时间戳下）且让假设显式化 |
| SUGGESTION-2 | **修**（注释改为「转发」、断言改直白） | **成立**。`client.ts:11` 的类注释现在准确描述了「转发 + 由 WP7 翻译」 |

### MINOR-4 文档依据的独立核实（本轮唯一「以文档冲突为由不改代码」的处置）

我打开 `docs/FRONTEND_DESIGN.md` 第 198 行原文：

```
196: 最低要求：
197:
198: - 只有进入 `online` 后，客户端才能把命令显示为已被服务器接受。
```

**核实结论：文档依据完全成立，不需要重判。**

- 行号精确（就是 198）；原文是「**只有**进入 `online` **后**，客户端**才能**把命令显示为已被服务器接受」——
  这是对呈现时机的**单向必要条件**约束，比 spec（只禁止「确认前显示已接受」）更严。
- `command-machine.ts:186-189` 的 `mayDisplayAsAccepted` 要求 `connectionOnline`，
  正是这条文档约束的直接实现。**代码与设计文档一致，改代码反而会与 `docs/FRONTEND_DESIGN.md` 冲突。**
- `docs/FRONTEND_DESIGN.md` 不在本包 Write Scope（Write Scope 是 `clients/app/src/sync-client/` 与
  `clients/app/src/state/`），因此「改 docs 以迁就代码」与「改代码以迁就 spec」**两者都越界**，
  作者选择「不改代码 + 补用例钉住现状 + 写明依据」是本包范围内唯一正确的处置。
- 补充核实：作者新增的 `MINOR-4` 用例（把现状钉住）与这条文档要求方向一致，没有把错误行为固化成「期望」。

**这条处置判定为：成立。**

---

## 5. 其他必查项

### 5.1 写入范围 —— 通过

`git diff --name-only 263d3ba..545c895` 共 26 个文件，我用 grep 反查：
`grep -v "^clients/app/src/sync-client/\|^clients/app/src/state/"` 输出为空。
**全部在 Write Scope 内**，无越界。

### 5.2 分层 —— 通过

`src/layers.test.ts` 9 passed。无 `import ... from "src/platform"`（含 `import type`）。
本轮新增的 `SnapshotDiscardReason` 类型导入走的是 `./snapshot` 同层模块，未跨层。

### 5.3 未验证项的定性

| 作者登记 | 我的定性 |
| --- | --- |
| 无真实 Daemon 可连 | **可接受**，但请注意 NEW-1 恰恰是「假服务端不会重放同一条事件」这一替身特性掩盖的——作者把 socket 级用例从「逐个 await」改成了 burst 形态是对的，但**响应式服务端**（收到 subscribe 就重放）这个形态仍无用例 |
| 固定向量字节对拍未做（归 TP3） | **可接受**，与 `tasks.md` 2.6 分工一致 |
| `HostIdentityPort` / `DigestPort` 真实实现不存在 | **可接受**。但 NEW-3/NEW-4 说明 `DigestPort` 的**失败路径**完全无用例，而这正是替身（永不 reject）掩盖的 |
| `ClockPort` / 重连退避不在本包 | **可接受**，归 WP7/TP4 |
| 服务端页内 `(createdAt, messageId)` 升序无法对拍 | **可接受**，已写成显式契约 |
| `deps`/`advisories`/`secrets` 未跑 | **可接受**，CI-only |

**两条「可接受」的边界我必须指出**：作者用「无真实 Daemon」为由登记的两项，
恰好是 NEW-1 与 NEW-3/NEW-4 的成因。替身把「服务端响应 subscribe」建模成被动事件注入，
于是「客户端主动重订阅 → 服务端重放 → 客户端再判 gap」这个闭环在测试里根本不存在。

### 5.4 对 WP7 的可用性

**变好**：
- `SyncClient.ackedCursor` getter（`client.ts:129-131`）是新的公开读出口，WP7 组合根可据此持久化游标。
  `SyncClientOptions.resumeCursor` 现在语义清晰了——它是**首次**注入点，不是每次连接的覆盖值。
- 5 条阻断项全部关闭后，WP7 面对的链路**能真正产出目录快照、能 ACK、不会重复呈现历史事件**。
  r1 状态下 WP7 若先行，会写到一条永不产生活目录快照、永不 ACK 的链上（r1 报告已如此警告）。

**变坏 / 新增缺口**：
- **NEW-1 是 WP7 必须先解决的问题**：事件流在「有被过滤事件」时彻底停摆，
  而被过滤事件在真实多设备场景是常态。WP7 的任何「重连后补齐历史」逻辑都会踩到它。
- **NEW-2 让 epoch 切换路径对 WP7 不可用**：服务端重启后客户端既订阅失败又不 ACK。
- WP7 仍需自建 `TranscriptCodec` / `HostIdentityPort` / `DigestPort` / `RandomPort` / `ClockPort`
  五个端口实现（沿用 r1 结论，本轮未变）。

---

## 6. 复核建议（给 coder）

按此顺序修：

1. **NEW-1（P1）**：给 `EventLedger` 增加接受外部游标的能力（`resumeFrom` / `advanceTo(cursor)`），
   让 `#onCaughtUp`（`connection.ts:490`）与 `#onSnapshotEnd`（`:533`）能把账本推到 barrier，
   使 gap 判定与 `#acked` 对齐；**同时**在 gap 分支加指数退避与连续次数上限（r1 复核建议的后半句，本轮遗漏）。
   补一条**响应式服务端**用例：每次 `sync.subscribe` 都重放同一条可见事件，断言 subscribe 次数有界。
2. **NEW-2（P2）**：`#onAuthenticated` 里当 `ledgerFor` 返回新 epoch 账本时，
   同步把 `#acked`/`#lastAcked` 置 `null`，与 `#ledgerFor` 的跨 epoch 处理对齐。
   补一条 epoch 切换用例，断言首条 `sync.subscribe` 的 cursor 为 `null` 且新 epoch 事件能 ACK。
3. **NEW-3（P2）**：`snapshot.ts:185` 的 `push` 挂 `.catch(() => {})` 吞掉孤儿 rejection。
4. **NEW-4（P2）**：digest 失败时让暂存区被 `discard()` 并给出正确的 `lastDiscardReason`。
5. **NEW-5（P3）**：`begin()` 仅在确有旧暂存区时记 `superseded_by_new_snapshot`。

---

## 7. 纪律声明

- 只读检视，**未提交、未改动 `feat/wp6`**。HEAD 仍为 `545c895`。
- 全部复现用临时探针文件（`zz-probe{,2,3,4,5}.test.ts`）已删除；
  判别力差分用的 6 个源文件回退已用 `git checkout 545c895 --` 还原。
- **最终 `git status --porcelain` 输出为空。**
