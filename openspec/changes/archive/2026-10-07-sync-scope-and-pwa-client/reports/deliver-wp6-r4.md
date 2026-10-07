# WP6（MU3c）第 4 轮修复报告 — deliver-wp6-r4

- 分支 / 提交：`feat/wp6` @ **`4ada2a2`**（基线 `3875f6f`，仓库基线 `263d3ba`）
- 工作树：`D:\Project\acp-remote\.worktrees\wp6`（提交后 `git status --porcelain` 为空）
- 写入范围：8 个文件，全部在 `clients/app/src/sync-client/`（含 `*.test.ts`）
- 凭据：`reports/PV3-wp6-r4.log`（181 行）
- 日期：2026-10-05
- 纪律：未改动 `design.md` / `plan.md` / `docs/**` / `crates/**` / `schemas/**` / `scripts/**`；
  未新增 npm 依赖；未新增 `.native.ts`；任何层都没有 import `src/platform/**`（`src/layers.test.ts` 9 passed）。

---

## 0. 一句话

前三轮修的分别是「账本怎么重建」「循环怎么退避」「水位怎么推进」，都没有碰那条**错误的假设**：
`dedupe.evaluate` 把 `received !== last + 1` 等同于事件丢失。对 scope 过滤后的 feed，序号不连续是
`docs/SYNC_PROTOCOL.md:658` 明文允许的**常态**，于是每一次合法空洞都会：判 gap → 丢事件 → 重订阅 →
服务端按 §9.3 重放同一条 → 再判 gap。本轮把这条假设换掉：水位只由**服务端屏障**与**确实呈现过的事件**
抬高，「真实丢失」改由「服务端声明过的水位之下又送来一条从未呈现过的事件」来判定。

---

## 1. 正确的口径

### 1.1 水位的两个来源（`dedupe.ts`）

| 来源 | API | 语义依据 |
| --- | --- | --- |
| 服务端屏障 | `advanceTo(cursor)` | §9.3 第 4 步「发送 `sync.caught_up`，之后继续实时事件」；§9.4 的 `snapshot_end.cursor`；§9.5「即使某段 global sequence 全部因权限过滤而不可见，客户端也可以 ACK `sync.caught_up.cursor`」 |
| 已呈现事件 | `commit(event)` | 客户端自己的进展，取**最大值**（`late` 事件低于水位时不得抬高） |

客户端**不数序号**。数序号等于把那条错误假设换个地方再实现一遍。

### 1.2 `evaluate` 的三个判定（`gap` 判定被删除）

```
seen(eventId)          → duplicate      // 见过：丢弃且无任何副作用
seq 形状非法 / 越界     → rejected       // 不可判定：拒绝而不是猜
seq ≤ 已处理界          → duplicate      // 记忆已释放的那一段（见 1.4）
seq ≤ 水位              → late           // 真实丢失证据：补齐呈现 + 有界重订阅
否则                    → new            // 呈现并抬高水位
```

**为什么这样就不丢事件了**：`late` 事件的语义是「服务端已经声明过『这条序号之前都已交付』，
却又送来一条我从未呈现过的事件」。无论成因是服务端违反了自己的 barrier 声明，还是客户端此前误丢，
这条事件都**必须被补齐呈现**——R14 的两条要求在这里冲突，而重复呈现是可见、可去重、可诊断的，
丢弃是静默且不可逆的，所以选「不丢」。同时它**必然**被记为 `gap_detected` 并触发从最后确认位置的
有界重订阅，去看服务端是否还压着别的未交付事件。

### 1.3 「真正的丢失」仍然可检出——以及不可检出的边界

- **可检出**：服务端违反 barrier 声明（先发 `caught_up@N`、后补投 ≤ N 的可见事件）、
  客户端此前误丢、以及屏障之前的乱序投递（`received < 已呈现最大值`）。三者在信息上都等价于
  「水位之下、未呈现过」，一律判 `late`。
- **不可判定**（协议层面本就不可判定）：屏障之下服务端**从未投递**的空洞。服务端已经声明交付，
  没送就是不可见，客户端无权也无能区分「不可见」与「漏投」。这一段由 §9.3 的屏障消解，
  客户端不得据此推断丢失——这正是 §9.2 那句话要防的事。任何声称能在这一段检出丢失的实现，
  都在把合法空洞误报成丢失，也就会把合法事件丢掉。

### 1.4 有界记忆的唯一取舍（`#accountedFloor`）

去重键窗口默认 2000 条。淘汰掉的条目之上「重复」与「丢失」不可区分，因此 `#accountedFloor`
记录「这一段及以下的序号都已处理过、且记忆已释放」（初值 = 续接游标；每次淘汰抬到被淘汰条目本身
的序号——**不是**剩余条目里最旧的那个，后者会把从未交付的 scope 空洞一并划进已处理段）。
落在这段里的未见事件按重复丢弃，落在它之上、水位之下的判 `late` 补齐。

取舍理由：§9.3 的重放严格从游标**之后**开始、§8.5 的切换窗口重复投递紧贴当前水位，正常服务端
不会重投已释放的那一段；而把看不见的旧事件当成「丢失」会反复触发重订阅。

### 1.5 P1-A 为什么能被结构性消除

r3 的 P1-A 是「`advanceTo` 抬高的水位之下，可能压着**已投递但被判 gap 而丢弃**的事件」。
本轮 `gap` 分支与丢弃分支**都不存在了**：`late` 一律补齐呈现、`new` 一律呈现、`duplicate` 才是丢弃。
再加上两条结构性事实：

1. 每条非重复 verdict 都在**同一个同步调用栈**内被 `commit` 并呈现，不存在跨消息的暂存态；
2. `sync.caught_up` / `sync.snapshot_end` 各自是一次独立的入站调用，不会在某条事件的呈现过程中插入。

于是 `advanceTo` 抬高的水位之下**不可能**存在「已投递、尚未呈现」的事件，无需再引入
「已丢弃序号」参数或缓冲补齐逻辑。这条论证写在 `dedupe.ts` 的文件头里（`design.md` 不动）。

---

## 2. 连带必修

| 编号 | 修法 | 位置 |
| --- | --- | --- |
| **P1-A** | 删除 gap 丢弃分支；`late` 补齐呈现；`advanceTo` 的「不存在未呈现中间态」写成不变式 | `dedupe.ts` 文件头、`connection.ts#onEventMessage` |
| **P1-B** | `#gapAttempts` 只在**真的有进展**时清零：呈现了一条 `new` 事件，或 `advanceTo` 返回 `advanced`。`advanceTo` 的返回值从 `boolean` 升级为 `advanced / already_covered / rejected`，正是为了把「收到过 `caught_up`」与「水位真的被抬高」分开 | `connection.ts:620-641`、`dedupe.ts:262-270` |
| **MINOR-1** | 跨 epoch 的 `caught_up`：账本 `rejected(epoch_mismatch)` ⇒ 连接侧**不**采纳为 `#acked`、**不**发 ACK、**不**宣告 online，只上报 `caught_up_epoch_mismatch`。`advanceTo` 先于 `#acked` 赋值，两侧对同一条消息给出同一判断 | `connection.ts#onCaughtUp`、`#moveAcked` |
| **MINOR-2** | 序号统一走 `parseSequence`：复用 `wire.ts` 导出的 `isDecimalString`，并加上界 `SEQUENCE_UPPER_BOUND = 2^53-1`。越界 / 非十进制的屏障 → `rejected`，水位不动（越界屏障一旦被采纳，此后每条真实事件都落到水位之下，事件流静默停摆且无法自愈）。同一道防线也覆盖 `commit()` 与构造期的 `resumeFrom`（形状非法直接抛错，不静默退化成「无游标」） | `dedupe.ts:74-128`、`wire.ts:178-181` |
| **MINOR-3** | `SyncClient` 只保留**一个**时间来源：`nowMs` 缺省时委托 `clock.now()`，组合根不必再提供第二个时钟 | `client.ts:84-92,122` |

「不要动的东西」逐条复核：CRITICAL-1（`acceptChunk` 同步段无 `await`）、CRITICAL-2（`#ledgerFor` 仍是
唯一构造点）、CRITICAL-3（出站序号从 1 严格递增、退避定时不消耗序号）、MAJOR-2（门面
`#ackedCursor` / `ackedCursor` getter）、NEW-2/3/4/5、MINOR-1/2/3/5/6、两条 SUGGESTION、MINOR-4
（不改代码）——均未被触碰，判别力由既有用例继续守着。

---

## 3. 判别力：六条断言逐条证据

### 3.1 判别力基础设施 —— 真正建模 §9.3 的服务端

前三轮的盲区是同一个：**测试替身不建模 §9.3**。本轮新增 `attachReplayServer`
（`connection.test.ts:135`）：在握手**之前**包装 socket 的 `send`，每收到一条
`sync.subscribe` 就——

1. 按订阅游标升序重放本设备**可见**的事件；
2. 发送 `sync.caught_up(屏障游标)`；
3. 之后继续实时事件。

应答进队列、由 `drain()` 交付（真实 WSS 上消息异步到达；同步递归只会把失控表现成爆栈）。
`withheld` 集合用来建模「服务端明知可见却不投递」的漏投。

### 3.2 修复前红 / 修复后绿

判别力差分：把 `dedupe.ts` / `connection.ts` / `client.ts` / `wire.ts` / `index.ts` 五个源文件
`git checkout 3875f6f --`，保留全部新用例。

```
修复前（3875f6f 源 + r4 用例）： Test Files 3 failed | 5 passed (8)
                              Tests  15 failed | 126 passed (141)
修复后（4ada2a2）：              Test Files 17 passed (17)
                              Tests  269 passed (269)
```

15 条红逐条对应：`scope 过滤造成的序号空洞不判丢失`、`每条服务端投递的可见事件都被呈现…`、
`水位不前进时预算不被 caught_up 清零…`、`屏障真正抬高水位时预算才复位…`、
`跨 epoch 的 caught_up 不被采纳…`、`去重键不是序号…`、`续接游标之后的序号跳跃直接接受…`、
`水位之下未呈现过的事件判 late…`、`水位之下的乱序投递同样判 late…`、
`采纳屏障后屏障之上的序号跳跃仍直接接受`、`重复或更旧的屏障是 already_covered`、
`跨 epoch 的屏障被拒绝`、`越界与形状非法的屏障被拒绝`、`形状非法的续接游标在构造期就响亮地失败`、
`未注入 nowMs 时快照时间戳实时读自 clock.now()`。

### 3.3 两条探针的实际读数（临时探针建在 `src/sync-client/zzprobe.test.ts`，已删除）

**探针 1**（复刻新用例 2 的场景：resume@1，2/3/4 不可见，5/6/7 重放，10..15 可见但漏投，屏障固定 15，
逐轮补投 10..15）：

```
修复前： presented=[]  gaps=3  dup=6  subscribes=2  exhausted=0  delays=[250,500,1000]
修复后： presented=[5,6,7,10,11,12,13,14,15] gaps=6 dup=0 subscribes=6 exhausted=1 delays=[250,500,1000,2000,4000]
```

修复前 **9 条服务端实际投递的可见事件全部丢失**（5/6/7 被 gap 分支丢掉，10..15 被推进的水位判成
duplicate），只留下 3 条 `gap_detected` 诊断——即 r3 reviewer 说的「安静的丢失」。

**探针 2**（复核 reviewer 的 P1-B 探针形态：屏障固定不前进，服务端每轮重放同一条 seq 5）：

```
修复前： presented=[]  gaps=301  subscribes=301  exhausted=0  delays=[250,250,250,…（恒 250ms）]
修复后： presented=[5] gaps=0    subscribes=1    exhausted=0  delays=[]
```

修复前复现了 reviewer 的 `subscribe 400 / exhausted 0`、`delays` 恒为 250ms——**退避上限在
合规服务端下完全不可达**。修复后这条路径上根本不存在丢失信号（事件被呈现、重投判重复），
一次订阅就结束。

### 3.4 六条硬性断言的逐条对照

| # | 断言 | 用例与断言点 | 修复前 | 修复后 |
| --- | --- | --- | --- | --- |
| 1 | 假服务端按 §9.3 建模 | `attachReplayServer`（`connection.test.ts:130`）：subscribe → 重放可见事件 → `caught_up` → 实时 | 替身只被动注入，`caught_up` 缺席 | 替身闭环成立 |
| 2 | 存在 scope 过滤造成的不可见事件 | 事件库 2/3/4 不可见，5/6/7 可见 | 5/6/7 全被丢弃（`presented=[]`） | `presented=["5","6","7"]`，`gap_detected` 0 条，`subscribeCount` 仍为 1 |
| 3 | 每条服务端投递的可见事件都被呈现 | 同上用例步骤 (1)(4)(6)(7) | `presented=[]` | `presented=["5","6","7","11","9"]` |
| 4 | 重订阅有界 + 退避递增 + 到顶后真的停 | 用例「水位不前进时预算不被 caught_up 清零」 | `subscribes=301 / exhausted=0 / delays 恒 250`（探针 2） | `delays=[250,500,1000,2000,4000]`（逐轮 `advanceBy(delay-1)` 不触发、`advanceBy(1)` 触发）、`subscribes=1+5`、`exhausted=1`、`pendingTimers=0`，且第 6 条漏投事件**仍被补齐呈现** |
| 5 | 事后重放仍能补齐、不重复呈现 | 同上用例步骤 (5)(7) | 重放 5/6/7 → `dup=6`，永久不可恢复 | 重放 → `duplicate_dropped` 3 条、`presented` 不变；服务端补投的漏投事件 9 **被补齐呈现**且再投判重复 |
| 6 | 不存在「已投递未呈现且不可恢复」 | 探针 1 读数 + 用例 (1)(6) | 9/9 丢失，永久 | `presented` 恰为服务端投递的全集 `[5,6,7,10,11,12,13,14,15]`，无重复、无丢失 |

第 4 条另有一条正例用例「屏障真正抬高水位时预算才复位」：先走完 6 条漏投事件把预算耗尽，
服务端 `setHead("40")` 后重放 → `caught_up@40` 抬高水位 → 预算复位 → 新一段抖动第 1 次就是 250ms
（`advanceBy(249)` 不触发、`advanceBy(1)` 触发）。

### 3.5 退化路径的兜底

- 预算到顶后**只停重订阅，不停呈现**：第 6 条漏投事件在 `exhausted` 之后仍进入 `presented`
  （用例末尾断言）。
- 跨 epoch 屏障：`ackedCursor` 停在 1、`sync.ack` 0 条、`online` 0 条、上报 `caught_up_epoch_mismatch`。
- 越界 / 形状非法屏障：水位不动，随后的 seq 8 仍判 `new`（可自愈）。

---

## 4. 退避参数与依据

| 参数 | 值 | 依据 |
| --- | --- | --- |
| `baseDelayMs` | 250 | 救回「服务端补投只晚了几百毫秒」这类最常见抖动；低于用户感知的亚秒级停顿阈值，一次服务端抖动不会被放大成恢复风暴 |
| 倍率 / `maxDelayMs` | 2 / 4000 | 250/500/1000/2000/4000，连续 5 次累计 7.75s，足以覆盖一次短暂重放抖动，又不会在几秒内把服务端打穿 |
| `maxAttempts` | 5 | 单连接最多补发 5 次；到顶上报 `gap_resubscribe_exhausted` 并**真的停止**，恢复路径回到「重连 + 快照重建」（那条通道有全局退避） |
| 清零条件 | 呈现了一条 `new` 事件，或 `advanceTo` 返回 `advanced` | §9.3 要求每次重放后都发 `caught_up`，按「收到过就清零」会让预算每轮复位（探针 2：301 次 subscribe、exhausted 0） |

触发源从「序号不连续」收紧为「水位之下未呈现过的事件」，因此 scope 空洞**不再**触发重订阅。

---

## 5. 门禁与退出码

| 门禁 | 结果 |
| --- | --- |
| `clients/app` `npx tsc --noEmit` | **exit 0**，无诊断 |
| `clients/app` `npx vitest run` | **exit 0**，17 文件 / **269 passed (269)** |
| `clients/app` `node scripts/run-browser-check.mjs` | **exit 0**，**16/16 passed**（脚本未改动） |
| `clients/app` `npx expo export --platform web` | **exit 0** |
| 根 `npm run check`（十道合同门禁） | **exit 0**（schemas 149 valid/51 invalid、commands 13、features、assets、acp、docs、boundaries、drift、agentic 21 passed / 0 failed） |
| `git diff --name-only 3875f6f..HEAD` | 8 文件，全在 `clients/app/src/sync-client/` |
| `git diff --stat 3875f6f..HEAD -- openspec` | **空** → `design.md` / `plan.md` 零改动 |
| `git status --porcelain`（提交后） | 空 |

`contract_digest` 应保持 `sha256:ecfc245c119d41b1b76a7bfdde9b11f93e751628a894f4b56f1fc2dba90e0e91`：

- 该 digest 由 `openspec/changes/sync-scope-and-pwa-client/` 下的规划文件算出，而**这个目录在
  worktree 里根本不存在**（`ls .worktrees/wp6/openspec/changes/` 只有 `archive`；该变更目录只存在于
  主检出且在主检出里也是未跟踪状态）。
- 本轮未写入主检出，未改动任何规划文件，`git diff --stat 3875f6f..HEAD -- openspec` 为空。
- 因此 digest **定义上未变**。我未能本地复算（`workflow-check.mjs` 不在本仓库内），
  按「已验证」记账的是「输入文件未变」这一事实，复算交由门禁侧完成。

**CI-only 的 `deps` / `advisories` / `secrets` 三个 job 未执行，本报告不声称它们通过。**

---

## 6. 未验证 / 未声称

| 项 | 状态 |
| --- | --- |
| 真实 daemon 在收到重订阅时到底重放什么 | **仍未实测**——但本轮不再依赖它：§9.3 的文本已足以定义语义，判别力用例即按该文本建模（reviewer r3 指出的最大盲区已按文本闭合，接真 daemon 仍是后续项） |
| 服务端**确实**违反 barrier 声明的概率 | 无实测。`late` 路径按「必须可达且必须有界」设计，不依赖该概率 |
| 有界记忆段（`#accountedFloor` 以下）的重复投递 | 未对真实服务端实测；取舍已在 §1.4 写明 |
| `DigestPort` / `ClockPort` / `TranscriptCodec` / `HostIdentityPort` / `RandomPort` 的真实实现 | 不存在（WP7 组合根职责），只用替身验证 |
| 退避的真实时序（后台标签页节流） | 未实测；`FakeClock` 确定性推进是既定取舍 |
| 固定向量字节对拍 | 归 TP3（`tasks.md` 2.6 分工） |
| `deps` / `advisories` / `secrets` | 未执行（CI-only，本地无凭据） |
| 断网复验 `npm run check` | 未做（本地缓存命中） |

---

## 7. 刻意保留的取舍（供复核攻击）

1. **宁可重复呈现也不丢**：`late` 分支上 R14 的两条要求冲突，选「不丢」。若复核认为重复呈现不可接受，
   唯一正确的替代是把缓冲交给服务端——但服务端已经没有未交付的东西了。
2. **`#accountedFloor` 以下按重复丢弃**：记忆已释放的那一段不可判定，选「不重复呈现」。
   代价是一条窗口外的漏投事件会被丢弃；收益是不会把整段旧事件误判成丢失而反复重订阅。
3. **`commit()` 对形状非法的序号抛错**：正常路径上 `evaluate` 已先拒；构造期 `resumeFrom` 非法属于
   编程错误，响亮失败优于静默把恢复点清零。
4. **跨 epoch 的 `caught_up` 不宣告 online**：连接层无法自愈（真正的恢复是 §9.4 的
   `sync.reset_required` 路径），本轮只保证不把外来游标写进恢复点 + 给出可诊断原因。
5. **`late` 不发 ACK**：水位不前进，§9.5 的 ACK 只能前进；该序号已被更高的已确认游标覆盖。