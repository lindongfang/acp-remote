# WP6 第三轮交付（deliver-wp6-r3）

- 提交：`3875f6f`（`feat/wp6`，基线 `263d3ba`，被修对象 `545c895`）
- 复核依据：`reports/review-wp6-r2.md` §2 的 NEW-1/2/3/4/5
- 证据：`reports/PV3-wp6-r3.log`
- 日期：2026-10-05

---

## 0. 结论

NEW-1（P1）与 NEW-2/3/4/5（P2/P3）五条全部**结构性关闭**，不是打补丁：

- NEW-1 的根因（两份会漂移的游标拷贝）被消除——`EventLedger` 现在有接受外部屏障游标的
  能力，`caught_up` 与已验证快照都会推进账本，gap 判定与 `#acked` 从此同源。
- 叠加退避与连续次数上限后，即使 gap 真的补不齐（落在权限过滤造成的不可见空洞上），
  重订阅次数也**有界**：`3001 → ≤6`。
- 判别力差分：**12 failed / 253 passed (265)** → 修复后 **265 / 265**。

写入范围仍只落在 `clients/app/src/sync-client/`（9 个文件，含测试与 `testing/harness.ts`），
`git diff --name-only 263d3ba` 反查越界为空。

---

## 1. NEW-1（P1）无界重订阅循环

### 根因与修法

reviewer 指出的机制是：`#acked` 被 `caught_up` 抬到屏障，而 `EventLedger.#lastSequence`
只在 `commit()` 里推进，于是两者分叉；重连时账本按旧值起步，`evaluate` 在
`#lastSequence === null` 时硬要求首条恰为 1，服务端下发 101 → 判 gap → 用 `#acked=100`
重订阅 → 服务端按 §9.3 重放同一条 101 → 判 gap → 永不停止。

按 reviewer 的建议分两步：

**第一步（消除分叉）**：`EventLedger.advanceTo(cursor)`（`dedupe.ts`）接受服务端给的
**屏障游标**。`#onCaughtUp`（`connection.ts`）与 `#onSnapshotEnd` 在推 `#acked` 的同一步
调用它，两份游标从此由同一次调用推进。

「游标对齐不得吞掉真缺口」这条纪律落在方法语义上，注释与三条用例都写明了：

- 屏障**之下**的空洞已被服务端重放消解，屏障**之上**的缺口仍由 `evaluate` 的 `last + 1`
  照常判定——`dedupe.test.ts > 推进到屏障后，屏障之上的真缺口仍被判缺口` 直接断言
  `advanceTo(100)` 之后收到 103 仍是 `{kind:"gap", expected:"101"}`；
- 只前进不后退：更旧的屏障返回 `false`，水位不动；
- 跨 epoch 的屏障被拒绝：epoch 已变更意味着事件库重建，序号不可比（§9.4），
  强行采用会用一段不相干的序号覆盖真实水位。

**第二步（让补不齐的 gap 也有界）**：`#scheduleGapResubscribe()` 取代 gap 分支里裸的
`#sendSubscribe()`。

- 退避参数集中在 `GAP_RESUBSCRIBE` 常量，取值与依据写在常量注释里：
  - `baseDelayMs = 250`：既能救回「偶发一次乱序」这种最常见的真缺口，又低于亚秒级
    停顿的用户感知阈值，不会把正常抖动放大成恢复风暴；
  - 倍率 2、`maxDelayMs = 4000`：延迟序列 250 / 500 / 1000 / 2000 / 4000，
    连续 5 次累计 7.75s——足以覆盖服务端一次短暂重放抖动，又不会在几秒内把服务端打穿；
  - `maxAttempts = 5`：单条连接最多补发 5 次。到顶后发
    `rejected{reason:"gap_resubscribe_exhausted"}` 并停止，恢复路径回到
    「重连 + 快照重建」——那是有全局退避的通道，与 §8.5 要避免的重连风暴同类问题
    在这里被隔离在一条连接内。
- 预算在**缺口补齐**（成功处理一条 `new` 事件）或收到 `caught_up` 时清零：
  一次已补齐的缺口说明恢复通道是通的，后续零星抖动应当拿到完整预算。
- 退避走新增的 `ClockPort`（`ports.ts` 早已声明「退避定时」的接缝，本轮接线）。
  用端口而不是 `setTimeout`，是为了让「订阅次数是否有界」这类断言确定性地推进，
  不依赖竞态窗口。定时器在 `close()` / `#block()` / `#onClose()` 三条终止路径上撤销。

### 判别力证据

**新增响应式服务端用例**（`connection.test.ts > 响应式服务端：每次收到 subscribe 就重放
同一条事件时，重订阅次数必须有界`）：这是本轮唯一能抓住 NEW-1 闭环的用例形态。

替身此前的建模是「被动事件注入」——测试自己决定何时投事件，因此
「客户端主动重订阅 → 服务端重放 → 客户端再判 gap」这个闭环在测试里根本不存在。
新用例把 socket 的 `send` 包一层：每收到一条 `sync.subscribe` 就排一次重放，
测试再把排队的重放交给客户端。重放走队列而非同步递归（真实 WSS 上消息异步到达，
同步递归只会把「无界」表现成爆栈，测不出次数）；服务端侧每轮最多重放 50 条、共 60 轮，
给修复前的失控留出余量又不让测试自身 OOM。

- **修复前**：`expected 3001 to be less than or equal to 6`（与 reviewer 实测的
  303 subscribe / 302 gap / 0 条事件呈现同量级）。
- **修复后**：subscribe 次数 ≤ 6，且到顶后 `clock.pendingTimers === 0`——客户端彻底安静。

另有一条用例直接钉住根因（`caught_up 把账本推进到屏障后，下一条可见事件不再被判缺口`）：
`caught_up@100` 之后投 seq 101，修复前是 `gap_detected` + 0 条 event，
修复后是 0 条 gap + 1 条 event + `ackedCursor.globalSequence === "101"`。
这条触发条件不是边缘情况——§9.2/§9.5 明文允许设备有因 scope 过滤而不可见的序号段。

### 一处诚实的补充说明

修复前 reviewer 的复现里「0 条事件被呈现」是**缺陷**；在本轮的响应式用例里
seq 5 相对确认水位 seq 1 仍是真缺口，因此修复后 0 条 event 呈现是**正确**的
（R14「MUST NOT 出现丢失区间」不允许跳过它）。用例对这个 0 写了注释说明，
以免下一轮复核把它误读成停摆。

---

## 2. NEW-2（P2）epoch 变化后订阅游标未清空 + ACK 永久被拒

`#onAuthenticated` 里，`ledgerFor(serverEpoch)` 返回之后、**发 subscribe 之前**，
检查 `#acked.serverEpoch` 与本次认证的 epoch 是否一致；不一致则把 `#acked` 与
`#lastAcked` 一起置 `null`。这与 `#ledgerFor` 已有的跨 epoch 处理（`resumeFrom = null`）
对齐——第 1 轮 CRITICAL-2 只修了账本侧，连接侧没跟上，本轮补齐。

**判别力**（`client.test.ts > 服务端 epoch 变化后首个 sync.subscribe 的 cursor 为 null，
且新 epoch 的事件能被 ACK`）：

- (a) 修复前首条 subscribe 的 body 是
  `{"serverEpoch":"00384a03…","globalSequence":"2318"}`（旧 epoch 游标），
  修复后为 `null`；
- (b) 修复前新 epoch 投 3 条事件 → **0 条 `sync.ack`** + 3 条 `ack_not_advancing`
  （`compareCursors` 跨 epoch 恒返回 -1），修复后 **3 条 `sync.ack`**、0 条
  `ack_not_advancing`、`ackedCursor` 为新 epoch 的 seq 3。

---

## 3. NEW-3（P2）digest promise 变孤儿

`acceptChunk` 在 `push` 之前对摘要 promise **当场**挂一个吞掉 rejection 的处理器
（`void digest.catch(() => {})`），但存进 `#chunkDigests` 的仍是**同一个** promise，
因此 `complete()` 的 `Promise.all` 照样会抛——挂处理器不改变传播，只消除孤儿。

**判别力**（`snapshot.test.ts > 摘要 promise 在暂存区被 discard 后成为孤儿时不产生
unhandledRejection`）：摘要端口替身先挂着不决，测试在 `acceptChunk` 之后先 `discard("connection_closed")`
再让摘要失败，然后监听 `process.on("unhandledRejection")` 并推进一次事件循环阶段
（`setImmediate`，不是「等够久」的真实计时）。修复前 `unhandled.length` 为 1，修复后为 0。

---

## 4. NEW-4（P2）摘要失败不是 SnapshotValidationError，暂存区残留且诊断失真

`SnapshotDiscardReason` 新增 `digest_unavailable`（与 `digest_mismatch` 分开：
「字节对但被篡改」指向服务端/传输，「本地算不出」指向本机实现或环境，退避策略不同）。
`complete()` 的两级哈希包在 `try/catch` 里，失败时先 `discard("digest_unavailable")`
再抛 `SnapshotValidationError`。连接层 `#onSnapshotEnd` 现有的
`catch (SnapshotValidationError)` 分支因此能正常处理，不再有原始错误进
`void` 调用的无人 promise。

**判别力**（`snapshot.test.ts > digest reject 时 complete 抛 SnapshotValidationError、
暂存区被丢弃且诊断反映本次失败`）：测试先写一次与本次失败无关的
`superseded_by_new_snapshot`，再让 digest reject。

- 修复前：抛 `Error: digest exploded`（非 `SnapshotValidationError`），
  `hasPending === true`，`lastDiscardReason === "superseded_by_new_snapshot"`（误导值，
  与 reviewer 的 P3 实测逐字一致）；
- 修复后：`SnapshotValidationError{reason:"digest_unavailable"}`、`hasPending === false`、
  `lastDiscardReason === "digest_unavailable"`。

---

## 5. NEW-5（P3）`begin()` 无条件写 lastDiscardReason

`begin()` 仅在 `this.#staged !== null` 时才记 `superseded_by_new_snapshot`。

**判别力**（`snapshot.test.ts > 首次 begin 不谎报「被新快照取代」，取代真实发生时才记`）：
首次 `begin` 后 `lastDiscardReason` 必须是 `null`（修复前是假警报
`superseded_by_new_snapshot`）；确有旧暂存区在被取代时才记该 reason。

---

## 6. 门禁

| 门禁 | 结果 | 退出码 |
| --- | --- | --- |
| `npx tsc --noEmit` | 无诊断 | 0 |
| `npx vitest run` | 17 文件 / **265 passed (265)** | 0 |
| `node scripts/run-browser-check.mjs` | **16/16 passed**（脚本未改动） | 0 |
| `npx expo export --platform web` | Exported: dist | 0 |
| `npm run check`（仓库根，十道门禁） | 全绿；`openspec validate` 21 passed / 0 failed | 0 |
| `git diff --name-only 263d3ba` 反查越界 | 空（9 文件全在 `src/sync-client/`） | — |
| 分层 | `src/layers.test.ts` 9 passed；无 `import`/`import type` 触达 `src/platform/` | — |
| 判别力差分（4 源文件回退 `545c895`） | **12 failed / 253 passed (265)** | — |

改动文件（9 个，全在 Write Scope 内）：

```
clients/app/src/sync-client/client.ts
clients/app/src/sync-client/client.test.ts
clients/app/src/sync-client/connection.ts
clients/app/src/sync-client/connection.test.ts
clients/app/src/sync-client/dedupe.ts
clients/app/src/sync-client/dedupe.test.ts
clients/app/src/sync-client/snapshot.ts
clients/app/src/sync-client/snapshot.test.ts
clients/app/src/sync-client/testing/harness.ts   （新增 FakeClock）
```

---

## 7. 未验证项（如实登记）

| 项 | 状态 | 说明 |
| --- | --- | --- |
| **无真实 Daemon 可连** | 未验证 | 本轮新增的响应式服务端用例是**对 §9.3 语义的手写建模**，不是真实服务端。真实 daemon 在「收到重订阅时到底重放什么」上是否与该模型一致，未实测。这仍是 r2 报告指出的最大盲区，下一轮若具备条件应优先接真 daemon。 |
| **`DigestPort` 真实实现失败行为** | 部分验证 | 只用替身验证了 reject 路径；真实 `crypto.subtle.digest` 在受限上下文（非 HTTPS、权限策略）下的失败形态未实测。 |
| **退避的真实时序行为** | 未验证 | 退避经 `ClockPort`，用例全部用 `FakeClock` 确定性推进；真实 `setTimeout` 的节流/合并（后台标签页）未实测。 |
| **固定向量字节对拍** | 未做 | 归 TP3，与 `tasks.md` 2.6 分工一致，r2 已登记。 |
| **`HostIdentityPort` / `ClockPort` 真实实现** | 不存在 | `ClockPort` 本轮已接线到 `SyncConnection`，真实实现仍待组合根（WP7）提供；缺它构造 `SyncClient` 会因类型而失败，无降级路径。 |
| **`TranscriptCodec` / `HostIdentityPort` / `RandomPort` 真实实现** | 不存在 | 沿用 r1/r2 结论，本轮未变。 |
| **服务端页内 `(createdAt, messageId)` 升序** | 无法对拍 | 已写成显式契约。 |
| **`deps` / `advisories` / `secrets`** | **未执行** | CI-only job，本地无凭据；**不声称通过**。 |
| **`npm run check` 十道门禁的完整清单** | 已执行 | exit=0；但其中依赖网络的项若存在（如 `check:acp` 的上游契约拉取）在本机为本地缓存命中，未做断网复验。 |

---

## 8. 对第 2 轮已确认项的处置

以下修复 reviewer 已独立确认成立，本轮**未改动**（除 NEW-2 需要与 CRITICAL-2 的
`#ledgerFor` 对齐而**新增**连接侧处理外）：

- CRITICAL-1 同步段 + `Promise[]` 按 index 占位：`acceptChunk` 体内仍无 `await`，
  只在摘要 promise 上多挂了一个处理器，不引入新的让出执行权点。
- CRITICAL-2 `#ledgerFor` 唯一构造点：未改。
- CRITICAL-3 `#outboundSequence` 计数器：未改（新增的退避定时器不发消息，不消耗序号）。
- MINOR-1/2/3/5/6 与两条 SUGGESTION：未改。
- MINOR-4 不改代码：处置与依据原样保留（`docs/FRONTEND_DESIGN.md:198` 比 spec 更严，
  `docs/` 不在 Write Scope）。本轮**未触碰**相关代码与用例。

## 9. 纪律声明

- 未弱化任何断言、未删用例、未加跳过标记。两条既有用例因行为**按设计改变**而更新断言，
  改动方向都是「更严」：`MAJOR-1 > 判 gap 后从最后确认游标重发 subscribe` 增加了
  「退避前确实只有 1 条 subscribe」的断言；`MINOR-2 > 每次 discard 都记录结构化原因`
  拆成两条，把「首次 begin 不记」从原用例里独立出来。
- 复核用的 4 个源文件回退均已 `git stash pop` 还原，HEAD 未在复核期间移动。
- worktree 干净：`git status --porcelain` 为空。