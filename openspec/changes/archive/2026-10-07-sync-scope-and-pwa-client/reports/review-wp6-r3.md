# WP6 第三轮独立复核（review-wp6-r3）

- 被检对象：`D:\Project\acp-remote\.worktrees\wp6` @ `3875f6f`（分支 `feat/wp6`）
- 基线：`263d3ba`　第 2 轮交付：`545c895`
- 第 1/2 轮报告：`reports/review-wp6-r1.md`、`reports/review-wp6-r2.md`（作背景，结论已独立重核）
- 作者报告：`reports/deliver-wp6-r3.md`
- 日期：2026-10-05
- 纪律：只读检视；15 个临时探针用例建在 `clients/app/src/zzprobe/` 下，全部已删除；
  源文件回退均已还原；最终 `git status --porcelain` 为空、HEAD 仍 `3875f6f`。

---

## 0. 结论

**FAIL — 阻断合入。**

第 2 轮的 5 条（NEW-1~5）中，**NEW-2/3/4/5 四条真关闭**且方式正确；NEW-1 的**根因确实被结构性消除**，
但作者选的修法在 §9.2/§9.5 明确允许的常态下引入了**两个新的行为缺陷**，其中一个是
**静默的永久事件丢失**（P1），另一个让本轮的核心卖点「退避有界」在真实服务端下**失效**（P1）。

作者报告里最关键的两个自报数据需要修正：

| 作者自报 | 我的独立复核 |
| --- | --- |
| 「重订阅次数有界：`3001 → ≤6`」 | **仅在「服务端从不发 `sync.caught_up`」的替身下成立**。§9.3 合规服务端（重放后必发 `caught_up`）下，我实测 **subscribe 400 次 / `exhausted` 0 次**，退避延迟恒为 250ms，`maxAttempts` 完全不可达 |
| 「advanceTo 不吞掉真缺口」 | **账本层成立**（屏障之上 N+3 仍判 gap，我独立验证），但**连接层不成立**：屏障会把**服务端已实际投递、只是被判了 gap 的可见事件**永久推进水位并丢弃 |

判别力差分我独立复现为 **12 failed / 253 passed (265)**，与作者自报一致；
写入范围、分层、`tsc`、265 全绿均干净。

| 门禁 | 我的独立结果 |
| --- | --- |
| `npx vitest run` @ `3875f6f` | 17 文件 / **265 passed (265)** |
| `npx tsc --noEmit`（删除探针后） | 退出码 0，无诊断 |
| `src/layers.test.ts` | 9 passed，分层无绕过（`sync-client/`、`state/` 无 import `src/platform/`） |
| 判别力差分（4 源文件回退 `545c895`） | **12 failed / 169 passed (181)**（sync-client+state 子集）／全量 **32 failed** 含探针；作者自报 12 条红**逐条对应 NEW-1~5** |
| `git diff --name-only 263d3ba..3875f6f` | 26 文件，全部在 `clients/app/src/sync-client/` 与 `clients/app/src/state/` |
| `git status --porcelain`（复核结束） | 空 |

---

## 1. NEW-1~5 逐条判定

| # | 判定 | 一句话 |
| --- | --- | --- |
| NEW-1（P1）无界重订阅循环 | **未关闭（根因已消除，但换成两个新缺陷）** | `advanceTo` 消除了分叉，但引入 P1-1 静默事件丢失；退避上限在 §9.3 合规服务端下不可达 |
| NEW-2（P2）epoch 切换后 ACK 永久被拒 | **真关闭** | `#onAuthenticated` 清空 `#acked`/`#lastAcked` 生效，实测 cursor=null + 3 条 ACK |
| NEW-3（P2）digest 孤儿 promise | **真关闭** | `unhandledRejection` 计数 0（我独立构造 2 个孤儿同样为 0） |
| NEW-4（P2）摘要失败暂存区残留 | **真关闭** | `digest_unavailable` + `hasPending=false` + 诊断反映本次 |
| NEW-5（P3）`begin()` 假警报 | **真关闭** | 首次 `null`、取代时 `superseded_by_new_snapshot` |

### NEW-1　逐个攻击点的独立结论

#### 攻击点 1：`advanceTo` 会不会吞掉真缺口？——**账本层不会，连接层会（这是本轮最严重的问题）**

**账本层： author's claim 成立。** 我独立构造（探针 A）：

- `advanceTo({seq:100})` 后 `evaluate(103)` → `{kind:"gap", expected:"101"}`，`evaluate(101)` → `{kind:"new"}`；
- 逐条 `commit` 101..105 后水位为 105，连续性判定照常；
- 三个守卫（跨 epoch 拒绝、只前进不后退、`null` 时接受）行为与注释一致。

**但连接层有一个账本层看不到的语义缺口：`advanceTo` 推进的是「已处理水位」，
而屏障之下的可见事件可能「已投递但被判了 gap、因而被丢弃」。**

复现（探针 P1/L1/H1，**完全合规的 §9.3 服务端**）：设备 resume@1，事件 2/3/4 因 scope 过滤
不可见（§9.2 明文允许），服务端重放可见的 5/6/7 后发 `caught_up@7`：

```
P1 presented=["8"] gapEvents=3 ackedCursor={"globalSequence":"8"} online=true
```

**5、6、7 是服务端实际投递给本设备的可见事件，三条全部没有呈现，一条 `gap_detected` 之后
客户端把 `ackedCursor` 推到 8 并宣告 `online`。** R14 要求「MUST NOT 出现丢失区间」——
这里正是丢失区间，而且是**静默**的：没有 `gap_detected` 之外的任何诊断告诉消费方「5/6/7 被丢了」。

作者在 `dedupe.ts` 注释里的论证是「屏障之下已被服务端重放消解」。这句话对**账本**成立
（水位对齐了），但对**事件流**不成立：那三条事件在到达时就被 gap 分支 `return` 掉了，
`#seen` 里没有它们的 ID，水位被抬到 7 之后它们**永久无法呈现**。

**不可恢复性我单独验证（探针 P2）**：事后让服务端重放 5/6/7：

```
P2 presented=[] duplicateDropped=3
```

三条全被判 `duplicate` 丢弃。**丢失是永久的。**

**与 r2 的对比（我实测同一探针在 `545c895` 上）**：

```
r2: L1 presented=[] gaps=4 subscribe=5      ← 事件流停摆（吵的坏）
r3: L1 presented=["8"] gaps=3 subscribe=2   ← 事件静默丢失（安静的坏）
```

r3 比 r2 好（不再无界循环），但**没有消除数据丢失，只是把它从「可见的停摆」变成
「不可见的丢失」**。r2 至少有 4~5 次 `gap_detected` + 5 次重订阅可供诊断；
r3 只报 3 次 gap，随后安静地把水位推过它们。对「不丢事件」这个 MUST 而言，两者都不合格。

**修法方向（供作者参考）**：`#onCaughtUp` / `#onSnapshotEnd` 在 `advanceTo` 之前，
必须先处理**已投递但未呈现**的可见事件。最小可行做法是把 gap 分支里被丢弃的事件暂存下来
（按 seq 排序的小缓冲），屏障到达后按序补齐并呈现；补不齐的部分才由屏障消解。
当前 `#onEventMessage` 的 gap 分支（`connection.ts:530`）是无条件 `return`，没有任何缓冲。

#### 攻击点 2：`advanceTo` 的三个守卫够不够？——**够，但有一个非规格路径的鲁棒性缺口**

三个守卫我逐条验证通过（探针 A1/A2/A3）。两个补充观察：

- **同 epoch 但屏障异常大被无条件接受**（探针 A4）：`advanceTo({seq:"999999999999999999999"})`
  返回 `true`，此后所有真实事件被判 `duplicate`，事件流静默停摆且无法自愈。
  按 §9.2 服务端对超 head 的 cursor 返回 `cursor_invalid`，所以合规服务端不会这么发；
  但 `caught_up` 的 cursor **客户端侧没有任何上限校验**。**MINOR**（非规格路径，需恶意/故障服务端）。
- **`globalSequence` 非十进制不被拒**（探针 A5）：`advanceTo({seq:"-5"})` 不抛错，
  水位写成 `"-5"`。`wire.ts` 的 `decodeCaughtUp` 只做 `isCursor` 形状检查，不校验十进制。
  **MINOR**（同属非规格路径；`commit()` 路径有同样暴露面，是既有问题，非本轮引入）。

#### 攻击点 3：退避会不会因 `caught_up` 反复清零而永不生效？——**会。这是 P1-2。**

作者的机制是「缺口补齐（成功 commit 一条 `new`）**或收到 `caught_up`** 时 `#gapAttempts = 0`」
（`connection.ts:541` 与 `connection.ts:590`）。

问题在于 **§9.3 明确规定服务端在每次增量重放后都要发 `sync.caught_up`**：

> `docs/SYNC_PROTOCOL.md:662-669`
> 「1. 服务端从 SQLite 读取 cursor 之后的可见事件。2. 按 globalSequence 升序发送 event。
>  3. 与实时 dispatcher 建立无缝 barrier… 4. **发送 `sync.caught_up`，之后继续实时事件。**」

于是**每一次重订阅都必然伴随一次 `caught_up`** → 预算清零 → 下一轮又是满预算
（250ms）。`maxAttempts = 5` 与 `2^` 指数退避**在这种服务端下永远不可达**。

我构造了两个探针，模拟 §9.3 合规服务端（收到 `sync.subscribe` → 重放可见事件 → 发 `caught_up`）：

**探针 O2（屏障不变，`caught_up@5`）**：

```
O2 delays=[250,500,250,250,250,250,250,250,...（共 313 项，全部 250/500 交替后恒为 250）]
    subscribe=300+  exhausted=0
```

**探针 I1（`caught_up@1`，屏障完全不推进）**：

```
I1 subscribe=400 gaps=798 exhausted=0
```

**探针 O3（`caught_up@head` 推进屏障，对客户端最有利）**：

```
O3 delays=[250,500,250,250,250,250,...]  subscribe=31  exhausted=0
```

**对照组 O1/N3（服务端**不**发 `caught_up`）证明退避本身是对的**：

```
O1/N3 subscribe=6  exhausted=7
```

即：`gap_resubscribe_exhausted` **只在一个不发 `caught_up` 的服务端下才会触发**。
作者自己写的响应式服务端用例（`connection.test.ts`）恰好就是「从不发 `caught_up`」的那种，
所以它测出了 `≤6`，却**完全没有覆盖真实服务端**。

这正是 r2 报告指出的最大盲区（「真实 daemon 在收到重订阅时到底重放什么未实测」）在本轮
**仍未闭合**，而且后果比 r2 更隐蔽：r2 的洪泛是无节流的（303 次），r3 是**有节流但无上限**
（250ms 一次，可无限持续）。`exhausted` 上报路径在这条真实路径上**完全不可达**，
所以「到顶就停」这个安全网等于不存在。

作者报告里「重订阅次数有界：3001 → ≤6」这句话**只在替身模型下为真**，不适用于 §9.3 合规服务端。
这是本轮最需要修正的自报。

**修法方向**：预算清零的条件应是「缺口确实被补齐」或「屏障确实推进了水位（`advanceTo` 返回 `true`）」，
而不是「收到过 `caught_up`」。`#onCaughtUp` 里已经有 `advanceTo` 的返回值可用：

```ts
// 只有屏障真的抬高了水位，才说明恢复通道是通的
if (this.#ledger?.advanceTo(message.body.cursor) === true) this.#gapAttempts = 0;
```

#### 攻击点 4：`ClockPort` 变必填对 WP7 的影响？——**合理，无契约冲突**

核实结果：

- `ClockPort` **不是本轮新增的契约**：`ports.ts:144-152` 在 r1 就已声明
  （`CancelTimer`、`ClockPort` 都已在 `index.ts:32-33` 导出）。本轮只是**接线**
  （`connection.ts:170` 加必填 `clock`，`client.ts:85` 加必填 `clock`）。
- `ports.ts:22` 的端口表本来就写明 `ClockPort` 的职责是「现在时间与退避定时」，
  本轮接线与既定设计一致，**没有引入新的分层义务**。
- `SyncClientOptions` 同时保留 `clock` 与既有的 `nowMs?: () => number`，两者职责不重叠
  （`clock` 用于退避定时，`nowMs` 用于快照时间戳），不算契约冲突，但**留了两套时间来源**，
  组合根要同时提供。**MINOR**（可用性瑕疵，非缺陷）。
- 对 WP7 的实际负担：组合根需实现 `now()` 与 `schedule()`。真实实现尚不存在（作者已如实登记）。
  这是**已知且合理的接线成本**，不构成阻断。

#### 攻击点 5：退避定时器是否消耗出站序号？——**不消耗，符合作者声明**

- 未到期的退避**不发任何消息**（探针 C1：`pendingTimers===1` 且 `sent.length` 不变）。
- 到期后发出的 `sync.subscribe` 走 `#sendSubscribe` → `#nextOutboundSequence()`，
  序号**严格递增**（探针 C1 实测 `subscribeSeqs=1,2`，无重复无跳号）。
- CRITICAL-3 的计数器语义未被破坏。

#### 攻击点 6：响应式服务端用例的判别力 —— **有效但模型不真实**

`connection.test.ts > 响应式服务端：每次收到 subscribe 就重放同一条事件时，重订阅次数必须有界`
确实是本轮唯一能抓住无界循环的用例形态（我确认它在 r2 代码上会红，见差分表），
且包装时机、队列建模、上限设置都写得克制。**判别力合格。**

但它的服务端模型**不发 `caught_up`**，因此它只能证明「无 `caught_up` 时退避有界」，
**不能**证明真实场景有界。作者在报告里把它当作 NEW-1 闭环的判别力证据，
这是**推理过强**：真实服务端会发 `caught_up`（§9.3 第 4 步），那条路径无用例覆盖。

---

### NEW-2　epoch 切换后 ACK 永久被拒 —— **真关闭**

修法核对（`connection.ts:419-424`）：`#onAuthenticated` 里 `ledgerFor` 之后、`#sendSubscribe()` 之前，
当 `#acked.serverEpoch` 与本次认证 epoch 不一致时把 `#acked` 与 `#lastAcked` 一起置 `null`。

**我独立复现（探针 D1，用 `resumeCursor={EPOCH, 2318}` 起手、认证回新 epoch）**：

```
D1 subscribeCursor=null
D1 acks=3  notAdvancing=0  acked={"serverEpoch":"11111111-…","globalSequence":"3"}
```

与 r2 实测的「旧 epoch 游标 + 0 条 ACK + 3 条 `ack_not_advancing`」逐项反转，确认关闭。

**与 `advanceTo` 的对称性核实（作者点名的攻击面）**：我专门构造了「账本拒绝但连接侧采用」的不对称
（探针 E1/E2，服务端发一个**别的 epoch** 的 `caught_up`）：

```
E1 ledger=null  acked={"serverEpoch":"11111111-…","globalSequence":"500"}
E2 acks=1  rejected=[]
```

即：跨 epoch 的 `caught_up` 被 `advanceTo` **正确拒绝**（账本水位不动），但 `#onCaughtUp`
（`connection.ts:588`）**仍然无条件把 `#acked` 写成那个外来 epoch 游标，并照常发出 `sync.ack`**。
这与 NEW-2 建立的「epoch 不可比就清空」原则**方向相反**。

严重度不高——需要服务端发错 epoch 的 `caught_up`，属非规格路径——但它是本轮**新引入**的
不一致（`advanceTo` 与 `#acked` 赋值对同一条消息给出相反判断）。记 **MINOR**。

---

### NEW-3　digest 孤儿 promise → `unhandledRejection` —— **真关闭**

修法核对（`snapshot.ts:194-197`）：`push` 前挂 `void digest.catch(() => {})`，但
`#chunkDigests.push(digest)` 存的是**同一个 promise 对象**。

我独立验证两件事：

1. **传播未被削弱**（探针 K2）：摘要端口每次调用都 reject，`complete()` 仍抛
   `SnapshotValidationError{reason:"digest_unavailable"}`。挂 handler 不改变原 promise 的
   rejected 状态，`Promise.all` 照样抛——作者的推理正确。
2. **孤儿确实不再泄漏**（探针 K3 单 chunk、K4 双 chunk）：先挂着不决 → `discard()` → 再让摘要失败，
   推进两次 `setImmediate` 后 `unhandledRejection` 计数 **0**。K4 构造两个孤儿同样为 0。
3. **正常路径未被破坏**（探针 K3'）：digest 正常时两级哈希仍验证通过（`result=verified`），
   证明挂 catch 没有把 catch 后的值替换回数组。

**另附 CRITICAL-1 的回归核实**：`acceptChunk` 体内仍**无任何 `await`**
（我逐行扫描函数体，只有注释里出现「await」字样），同步段未被破坏。

---

### NEW-4　摘要失败时暂存区残留 + 诊断失真 —— **真关闭**

修法核对（`snapshot.ts:233-243`）：两级哈希包在 `try/catch`，失败时先 `discard("digest_unavailable")`
再抛 `SnapshotValidationError`。

**我独立复现（探针 K1，完全照 r2 探针 P3 的形态：先写一次无关的 `reset_required`）**：

```
K1 err=SVE:digest_unavailable  hasPending=false  lastDiscard=digest_unavailable
```

与 r2 实测的 `Error: digest exploded / hasPending=true / lastDiscard=superseded_by_new_snapshot`
逐项反转。连接层 `catch (SnapshotValidationError)`（`connection.ts:636`）因此能正常处理，
不再有原始错误进入 `void` 调用的无人 promise。

**新 reason 与 `digest_mismatch` 分开是否合理**：合理，且理由在注释里成立。
`digest_mismatch` = 「算出来了但与服务端期望不符」→ 指向服务端/传输（篡改、编码不一致）；
`digest_unavailable` = 「本机根本算不出」→ 指向本机实现或环境（`crypto.subtle` 在非 HTTPS
上下文不可用、权限策略拒绝）。二者的恢复策略完全不同（前者应触发重连换快照，
后者应重试或降级），混成一个 reason 会让诊断失去分流价值。**这个判断我认同。**

---

### NEW-5　`begin()` 假警报 —— **真关闭**

修法核对（`snapshot.ts:139-142`）：仅当 `this.#staged !== null` 才 `discard("superseded_by_new_snapshot")`。

**我独立复现（探针 K6）**：

```
K6 first=null  second=superseded_by_new_snapshot
K7 lastDiscard=null hasPending=false   （成功提交后清空，仍成立）
```

首次 `begin` 不再谎报；确有旧暂存区被取代时仍正确记录，且覆盖上一条无关原因。

---

## 2. 回归核实（第 2 轮已确认成立的修复）

我**独立**核实，未采信作者声明：

| 项 | 核实方式 | 结论 |
| --- | --- | --- |
| CRITICAL-1 同步段（`acceptChunk` 无 `await`） | 逐行扫描函数体 | **仍成立**。本轮只在摘要 promise 上多挂一个 `.catch()`，不引入让出执行权点 |
| CRITICAL-2 `#ledgerFor` 是账本唯一构造点 | `grep "new EventLedger"` 于 `client.ts`/`connection.ts` | **仍成立**，仅 `client.ts:241` 一处 |
| CRITICAL-3 出站序号按连接重置、严格递增 | grep + 探针 C1 | **仍成立**。退避定时器不消耗序号（见攻击点 5） |
| MINOR-1/2/3/5/6 与两条 SUGGESTION | 读代码 + 跑测试 | **仍成立**，本轮未触碰 |
| MINOR-4 不改代码 | 核对 `docs/FRONTEND_DESIGN.md:198` | **仍成立**（该文档比 spec 更严，`docs/` 不在 Write Scope），相关代码与用例本轮未动 |

**`advanceTo` 是否间接改变了 CRITICAL-2 的重连语义？——我专门核实了这一点。**

CRITICAL-2 的核心是「`#ledgerFor` 是账本唯一构造点，跨连接复用账本，`resumeFrom` 取门面
`#ackedCursor`，跨 epoch 置 null」。`advanceTo` 的引入**没有**新增构造点，**没有**改动
`#ledgerFor` 的任何一行，**没有**改动 `carryOverToNextConnection`。它只改了 `#lastSequence`
的**取值来源**（多了服务端屏障这一个入口）。

副作用我核实了两条，都不构成 CRITICAL-2 的回归：

- 跨连接时账本水位现在可能**高于** `resumeCursor`（因为屏障推进过）。
  这是**期望的修复效果**（消除分叉），不是回归；
- 跨 epoch 时 `advanceTo` 拒绝外来屏障（探针 E1），与 `#ledgerFor` 的跨 epoch `resumeFrom=null`
  **方向一致**。唯一的不对称在连接侧的 `#acked` 赋值（已记 MINOR）。

**结论：无回归。**

---

## 3. 判别力差分（我自己复现）

把 4 个源文件（`connection.ts`、`dedupe.ts`、`snapshot.ts`、`client.ts`）
`git checkout 545c895 --`，保留全部新用例，跑 `npx vitest run`：

```
 Test Files  4 failed | 6 passed (10)          ← sync-client + state 子集
      Tests  12 failed | 169 passed (181)
```

**12 条红与作者自报完全一致**，且逐条对应 NEW-1~5（我逐条核对了红点名）：

| 阻断项 | 抓住它的红用例 | 覆盖判断 |
| --- | --- | --- |
| NEW-1 | `响应式服务端…重订阅次数必须有界` / `caught_up 把账本推进到屏障后…` / `连续缺口达到上限后停止重订阅` / `缺口补齐后重试预算清零` / `判 gap 后从最后确认游标重发 subscribe`（5 条） | **覆盖**（但模型不含 `caught_up`，见 §4） |
| NEW-1（账本） | `推进到屏障后，屏障之上的真缺口仍被判缺口` / `只前进不后退` / `跨 epoch 的屏障被拒绝`（3 条） | **覆盖** |
| NEW-2 | `服务端 epoch 变化后首个 sync.subscribe 的 cursor 为 null…` | **覆盖** |
| NEW-3 | `摘要 promise 在暂存区被 discard 后成为孤儿时不产生 unhandledRejection` | **覆盖** |
| NEW-4 | `digest reject 时 complete 抛 SnapshotValidationError…` | **覆盖** |
| NEW-5 | `首次 begin 不谎报「被新快照取代」` | **覆盖** |

还原后 `3875f6f` 重跑 **265 passed / 265**。最终 `git status --porcelain` 为空。

**差分的局限（我判定必须记录）**：12 条红**全部**落在「服务端不发 `caught_up`」或
「`caught_up` 推进水位后不再出现缺口」的模型上。因此
**P1-1（屏障吞掉已投递事件）与 P1-2（`caught_up` 清零预算）两条新缺陷一条都抓不到**——
差分全绿不代表这两条不存在，我用独立探针才构造出来。

---

## 4. 新发现的分级清单

### P1-1　`advanceTo` 把「已投递但被判 gap」的可见事件永久推进水位并丢弃（静默事件丢失）

- **位置**：`dedupe.ts:134-140`（`advanceTo`）、`connection.ts:588`（`#onCaughtUp` 无条件采用）、
  `connection.ts:635`（`#onSnapshotEnd` 同）、`connection.ts:530`（gap 分支无条件 `return`，无缓冲）
- **触发条件**：设备存在任何因 scope 过滤而不可见的序号段（§9.2 明文允许的**常态**），
  且服务端按 §9.3 重放屏障之上的可见事件后发 `caught_up`
- **实测**（探针 P1，合规服务端）：`presented=["8"]`，5/6/7 三条可见事件全部丢弃，
  客户端 `ackedCursor=8` 且宣告 `online`
- **不可恢复**（探针 P2）：事后重放 5/6/7 → `duplicateDropped=3`，`presented=[]`
- **违反**：R14「MUST NOT 出现丢失区间」（`openspec/changes/sync-scope-and-pwa-client/specs/pwa-web-client/spec.md:92`）
- **与 r2 的关系**：r2 是「吵的坏」（停摆 + 无界洪泛，有诊断），r3 是「安静的坏」（静默丢失）。
  **根因未消除，只是换了表现形式**
- **作者论证的漏洞**：`dedupe.ts` 注释称「屏障之下已被服务端重放消解」——
  对账本水位成立，但被 gap 分支 `return` 掉的事件从未进入 `#seen`、从未呈现，
  水位抬过后它们永久无法补齐
- **建议**：`#onCaughtUp` / `#onSnapshotEnd` 在 `advanceTo` 之前先补齐缓冲区里
  「已投递但被判 gap」的可见事件（按 seq 排序），补不齐的部分才由屏障消解；
  或让 `advanceTo` 接受一个「已知被丢弃的最大序号」参数，只推进到该点为止

### P1-2　`caught_up` 无条件清零缺口预算，使 `maxAttempts` 与指数退避在真实服务端下不可达

- **位置**：`connection.ts:590`（`#onCaughtUp` 里 `#gapAttempts = 0`）
- **触发条件**：服务端遵守 §9.3 第 4 步（每次重放后发 `sync.caught_up`）——**这是规范要求的行为**
- **实测**：探针 I1 `subscribe=400 / exhausted=0`；探针 O2 退避延迟恒为 250ms（313 次）；
  探针 O3 `subscribe=31 / exhausted=0`。对照组（不发 `caught_up`）才是 `subscribe=6 / exhausted=7`
- **影响**：`gap_resubscribe_exhausted` 这条安全网在真实路径上**完全不可达**；
  退避退化为固定 250ms 的无上限重订阅洪泛。虽比 r2 的无节流洪泛轻，
  但**仍是持续无界**的 `sync.subscribe`，与 §8.5 要避免的重连风暴同类
- **作者自报的修正**：「重订阅次数有界 3001→≤6」**仅在不发 `caught_up` 的替身下成立**，
  不适用于 §9.3 合规服务端
- **建议**：预算清零条件改为「水位确实被屏障推进」（用 `advanceTo` 的返回值）或
  「缺口确实被补齐」，而不是「收到过 `caught_up`」：
  ```ts
  if (this.#ledger?.advanceTo(message.body.cursor) === true) this.#gapAttempts = 0;
  ```

### MINOR-1　跨 epoch 的 `caught_up` 被账本拒绝却被连接侧采用（NEW-1 修复引入的不对称）

- **位置**：`dedupe.ts:136`（`advanceTo` 拒绝）vs `connection.ts:588`（`#acked` 无条件赋值）+ `:591`（照常发 ACK）
- **实测**（探针 E1/E2）：`ledger=null` 但 `acked={11111111-…, 500}`，且发出 1 条 `sync.ack`，无任何 `rejected`
- **影响**：需服务端发错 epoch 的 `caught_up`（非规格路径），严重度低；
  但它是本轮**新引入**的、方向相反的判断，且与 NEW-2 建立的 epoch 原则冲突
- **建议**：`#onCaughtUp` 里当 `advanceTo` 返回 `false`（含跨 epoch 拒绝）时，
  不要采用该游标作为 `#acked`，或至少上报 `rejected`

### MINOR-2　`advanceTo` 对同 epoch 的异常大屏障无上限校验，可致事件流永久静默

- **位置**：`dedupe.ts:137-139`（只比较 epoch 与单调性，无 head 上界）
- **实测**（探针 A4）：`advanceTo({seq:"999999999999999999999"})` 返回 `true`，
  此后所有真实事件被判 `duplicate`，无法自愈
- **定性**：§9.2 规定服务端对超 head 的 cursor 返回 `cursor_invalid`，故合规服务端不会如此；
  属非规格路径的鲁棒性缺口。`commit()` 路径有同样暴露面（既有问题）

### MINOR-3　`SyncClient` 同时存在 `clock` 与 `nowMs` 两套时间来源

- **位置**：`client.ts:85-86`（`clock` 必填 + `nowMs?` 可选）
- **影响**：组合根需同时提供两者；两者若来自不同时钟会产生时间戳与退避不一致。
  可用性瑕疵，非行为缺陷

### 观察项（非缺陷，记录备案）

- **`caught_up` 与 `#onSnapshotEnd` 的 `advanceTo` 都在推 `#acked` 之后调用**，
  顺序上若 `advanceTo` 抛错（当前不会，`BigInt` 对非法串抛 `SyntaxError`——
  见 MINOR-4 的探针 A5 边界）会让 `#acked` 与账本分叉。建议改为先 `advanceTo` 再赋 `#acked`
- **响应式服务端用例的 3000 次重放上限**：60 轮 × 50 条。修复前实测 303 次，
  上限留了 10 倍余量，合理；但该上限也意味着若将来回归到更慢的失控形态，
  用例会以「未达预期上限」而非 OOM 失败——这是好的（可读的失败数字）

---

## 5. 作者修改的两条既有用例：加严而非放松 —— **核实成立**

### MAJOR-1 `判 gap 后从最后确认游标重发 subscribe`

| | r2（`545c895`） | r3（`3875f6f`） | 判定 |
| --- | --- | --- | --- |
| 等待重订阅 | `waitUntil(subscribe 数量 === 2)` | `expect(pendingTimers).toBe(1)` + `expect(subscribe).toHaveLength(1)` + `advanceBy(250)` + `toHaveLength(2)` | **加严**：从「最终会出现 2 条」变成「退避前确实只有 1 条、确有 1 个定时器在等、推进后恰好 2 条」 |
| 恢复游标断言 | 保留 | 保留 | 不变 |
| 出站序号断言 | 保留 | 保留 | 不变 |

新增的三条断言都是**收紧**（原来 `waitUntil` 会一直等到 2 条出现，无法区分「立即重发」与
「退避后重发」）。**判定：加严。**

### MINOR-2 `每次 discard 都记录结构化原因，成功提交后清空`

r3 拆成两条：

1. `每次 discard 都记录结构化原因，成功提交后清空` —— 保留 r2 的原有断言（`reset_required` → `superseded_by_new_snapshot`）
2. `首次 begin 不谎报「被新快照取代」，取代真实发生时才记` —— **新增**，断言首次 `begin` 后为 `null`

拆分后原用例的断言**一条未删**，另加一条独立用例覆盖新行为。**判定：加严。**

（我另外独立验证了 `成功提交后清空` 仍成立——探针 K7：`lastDiscard=null, hasPending=false`。）

---

## 6. 未验证项的定性

作者登记了 8 项未验证。我逐项判断是否阻塞：

| 项 | 是否阻塞 | 判断 |
| --- | --- | --- |
| **真实 daemon 在收到重订阅时到底重放什么未实测** | **是（阻塞）** | 这正是 r2 报告指出的最大盲区，本轮**仍未闭合**，且已直接导致 P1-2。§9.3 明文要求服务端在重放后发 `sync.caught_up`，而 `#gapAttempts` 的清零正挂在这个消息上。作者的响应式服务端用例刻意不建模它，于是该盲区在测试里表现为「全绿」。**这是本轮最需要补齐的一项** |
| `DigestPort` 真实实现失败行为 | 否 | 只用替身验证了 reject 路径；`crypto.subtle` 在非 HTTPS 的失败形态未实测。真实实现尚不存在（WP7），不阻塞本包 |
| 退避的真实时序行为 | 否 | `FakeClock` 确定性推进是正确取舍；真实 `setTimeout` 的后台标签页节流未实测，但 `ClockPort` 正是为该接缝预留的 |
| 固定向量字节对拍 | 否 | 归 TP3，与 `tasks.md` 2.6 分工一致，r2 已登记 |
| `ClockPort` / `TranscriptCodec` / `HostIdentityPort` / `RandomPort` 真实实现不存在 | 否 | 组合根职责，WP7 提供。`ClockPort` 契约 r1 已声明，本轮只是接线 |
| 服务端页内 `(createdAt, messageId)` 升序 | 否 | 已写成显式契约，无法对拍 |
| `deps` / `advisories` / `secrets` 未执行 | 否 | CI-only，本地无凭据，作者**未**声称通过。诚实 |
| `npm run check` 十道门禁 | 否 | 作者已标注网络项为本地缓存命中，未做断网复验。诚实 |

**结论**：8 项中 7 项不阻塞；**「真实 daemon 的重订阅语义」仍是阻塞性风险**，
且它已经不再只是「未实测」——§9.3 的规范文本足以判定当前实现有缺陷（P1-2），
无需实测即可确认。

---

## 7. 合入建议

**不可合入。**

MU3c 之前必须完成（我建议的顺序）：

1. **修 P1-2（退避预算）** —— 这是小改动、收益最大：
   把 `#onCaughtUp` 里的 `#gapAttempts = 0` 改为「`advanceTo` 返回 `true` 才清零」。
   同时**必须新增一条覆盖 §9.3 语义的用例**：服务端收到 `sync.subscribe` 后
   **重放 + 发 `caught_up`**，断言重订阅次数仍然有界。这条用例在当前实现下会红。
2. **修 P1-1（屏障吞事件）** —— 需要设计缓冲：`#onEventMessage` 的 gap 分支不能无条件丢弃，
   屏障到达时先补齐。补齐后仍需一条用例：服务端重放 `5/6/7` + `caught_up@7`，
   断言 5/6/7 **全部呈现**。这条用例在当前实现下会红（我实测 `presented=["8"]`）。
3. **接真 daemon 或至少按 §9.3 补一条用例** —— 作者登记的最大盲区。
   规范文本已经明确服务端会发 `caught_up`，继续用「不发 `caught_up`」的替身
   会让这一类缺陷永远测不出来。
4. MINOR-1（跨 epoch `caught_up` 的不对称）建议一并处理，成本极低。

修完后建议**第 4 轮复核**，重点攻击：屏障与缓冲的交互顺序、跨快照的缓冲残留、
以及退避预算在「屏障推进」与「屏障不推进」两种服务端行为下的分别表现。

不需要返工的部分：NEW-2/3/4/5 四条已真关闭，CRITICAL-1/2/3 与全部 MINOR/SUGGESTION
无回归，两条既有用例确为加严，判别力差分（12 条红）诚实且有效，写入范围与分层干净。

---

## 8. 纪律声明

- 只读检视：未提交、未改动 `feat/wp6`、未 `git checkout` 任何提交到分支。
- 临时探针：15 个用例建在 `clients/app/src/zzprobe/`（4 个文件），**已全部删除**。
- 源文件回退（判别力差分，2 次）：4 个文件 `git checkout 545c895 --` 后
  均 `git checkout 3875f6f --` 还原。
- 复核结束实测：`git status --porcelain` 输出为空；`git log --oneline -1` 仍为 `3875f6f`。
- `npx tsc --noEmit`（删除探针后）退出码 0；`npx vitest run` 265 passed / 265。
