# WP6 第二轮修复报告（deliver-wp6-r2）

- 检视对象：`review-wp6-r1.md`（FAIL，阻断合入）
- worktree：`D:\Project\acp-remote\.worktrees\wp6`，分支 `feat/wp6`
- 基线：`263d3ba`　修复前：`2302c41`　**修复后：`545c895`**
- 日期：2026-10-05

---

## 0. 结论

3 条 CRITICAL、2 条 MAJOR 全部修复；7 条 MINOR/SUGGESTION 逐条处置（5 修、1 接受并钉住、1 修注释）。

**判别力证据的核心**：把 6 个源码文件回退到 `2302c41`（保留新用例）后跑 `vitest`，得到
**`Tests 13 failed | 241 passed (254)`**；恢复修复后 **`254 passed (254)`**。13 条红覆盖全部
5 条阻断项与全部被采纳的 MINOR，无一条是恒真断言。

| 门禁 | 退出码 | 结果 |
| --- | --- | --- |
| `npx tsc --noEmit`（`clients/app`） | 0 | 无诊断 |
| `npx vitest run` | 0 | 17 文件 / 254 用例全绿 |
| `node scripts/run-browser-check.mjs` | 0 | 16/16（脚本未改动） |
| `npx expo export --platform web` | 0 | 成功 |
| `npm run check`（仓库根） | 0 | 十道门禁全绿 |
| `git diff --name-only 263d3ba..HEAD` | — | 26 文件，全部在 Write Scope 内 |

---

## 1. 三条 CRITICAL

### CRITICAL-1　多 chunk 快照在 burst 下必然失败

**根因**：`acceptChunk` 在「读 `receivedChunks`」与「写回 `receivedChunks + 1`」之间 `await` 摘要。
服务端背靠背下发 chunk 0/1/2 时，chunk 1 在 chunk 0 的 `await` 处被抢占，读到未推进的
`receivedChunks === 0` → `chunk_out_of_order` → `discard()` 抹掉整份暂存区。

**修复**（`snapshot.ts`）：
1. **校验 → 推进序号 → 合并资源收进一个同步段**，中间没有任何 `await`；摘要只**发起**不等待。
   竞态因此在结构上不可能出现，而不是靠调用顺序约定。
2. `#chunkDigests` 从 `Uint8Array[]` 改为 `Promise<Uint8Array>[]`，**按 `chunkIndex` 占位**；
   `complete()` 用 `Promise.all` 按 index 等待。若仍按「算完再追加」，完成顺序与到达顺序不一致时
   摘要链会错位、两级哈希失配——占位是这个修复不可省略的另一半。

`sync.snapshot_chunk` 的 `void this.#onSnapshotChunk(...)` 保持不变：fire-and-forget 本身没问题，
问题在于被调用方让出了执行权。

**判别力**
- `snapshot.test.ts > CRITICAL-1：burst 投递下的 chunk 摄入 > 三个 chunk 不经 await 连着投递仍按 index 收齐并验证通过`
  修复前：`SnapshotValidationError chunk_out_of_order`（`acceptChunk` 在 `snapshot.ts:148` 抛出）；
  修复后：绿。
- `snapshot.test.ts > …> 摘要按 chunkIndex 连接，与完成顺序无关`　修复前红（占位缺失）、修复后绿。
- `client.test.ts > CRITICAL-1：socket 级 burst 下的多 chunk 快照 > 三个 chunk 背靠背投递（不逐个 await）仍能完成并替换仓库`
  **socket 级、不逐个 await**：`for (const chunk of chunks) socket.deliver(chunk.rawText);` 之后立刻投
  `snapshot_end`，digest 由 `expectedSnapshotDigest` 独立复算。修复前 `waitUntil` 超时（仓库仍为 `null`），
  且会带出 `chunk_out_of_order` 的 `rejected`；修复后三段清单齐全、无 `rejected`。

### CRITICAL-2　重连时去重账本被无条件重建

**根因**：`SyncClient.#onConnectionEvent` 收到 `authenticated` 时 `new EventLedger({ serverEpoch })`，
丢掉 `ledgerFor` 跨连接带来的 `seenEventIds`，并把 `resumeFrom` 置空。

**修复**（`client.ts`）：删掉该重建分支，`#ledgerFor` 成为账本的**唯一**构造点（注释写明这条约束，
免得后续再加）。`#ledgerFor` 的 `resumeFrom` 改为取门面的 `#ackedCursor`，且跨 `serverEpoch` 时置
`null`（跨 epoch 游标不可比，服务端事件库已重建）。

**判别力**
- `client.test.ts > CRITICAL-2：重连后账本与去重不得重置 > 连接 → 断 → 重连 → 重投已见事件判重复，且从最后确认位置续传`
  走完 **连接 → `serverClose(1006)` → `connect()` → 重新握手 → 重投 seq 1** 全流程。修复前重投产出的是
  `event`（二次呈现），断言在 `client.test.ts:350` 变红；修复后产出 `duplicate_dropped`，且重连时的
  `sync.subscribe` body 是 `{cursor:{serverEpoch,globalSequence:"2"}}`。
- `…> 重连后首个事件序号远大于 1 时不得判成缺口（续传起点不是 1）`
  从持久游标 2317 起，2318 → 断 → 重连 → 服务端续发 2319。修复前账本被重置后 `resumeFrom=null`，
  `evaluate` 要求首条恰为 1 → 判 `gap` → 事件流停摆，`waitUntil` 超时；修复后 2319 正常处理。

### CRITICAL-3　出站 `connectionSequence` 恒为 `"0"` 且从不递增

**根因**：`#connectionSequence = 0` 从未 `++`，却被盖在 pong / ack / snapshot_request 上；
`subscribe` 直接抄服务端 `auth.authenticated` 的序号；`buildStatusQuery` 写死 `"0"`。

**修复**（`connection.ts`）：`#outboundSequence` 计数器 + `#nextOutboundSequence()`（从 1 严格加一），
五个出站点全部改用它；`subscribe` 抽成 `#sendSubscribe()` 并使用自己的计数器。`sendCommand` 在已认证时
**覆盖**调用方给的 `connectionSequence`（未认证时不消耗计数器，§4.1 要求认证前省略该字段），
`buildStatusQuery` 里的 `"0"` 随之降级为占位并注明。

**判别力**
- `client.test.ts > CRITICAL-3：客户端方向出站序号 > 认证后的出站消息序号从 1 开始严格递增，且不抄服务端计数器`
  服务端的 `serverConnectionSequence` 被刻意设为 `"42"`。修复前输出
  `["sync.subscribe:42", …]`，断言在 `client.test.ts:409` 变红；修复后首项为 `sync.subscribe:1`，
  且整段序号数组等于 `[1,2,3,…]` 的自增序列。
- `client.test.ts > …> 认证前的两条消息不带 connectionSequence（§4.1）`　钉住 §4.1 的前半条。
- `connection.test.ts > CRITICAL-3：出站序号从 1 严格递增 > 同连接内连续 ACK 与 subscribe 的序号不重复、不回退`
  修复前实际为 `["1","0","0",…]`（含抄来的服务端序号），断言在 `connection.test.ts:297` 变红；
  修复后为 `["1","2","3","4"]`。

---

## 2. 两条 MAJOR

### MAJOR-1　`gap_detected` 之后从不重新订阅

**修复**（`connection.ts`）：抽出 `#sendSubscribe()`，认证后首次订阅与缺口恢复走**同一条**路径——
两者都从 `#acked` 重新订阅，区别只在游标是否为空。缺口分支在上报 `gap_detected` 之后真的重发
`sync.subscribe`，事件流不再静默停摆。

**判别力**
- `connection.test.ts > MAJOR-1：序号缺口必须真的恢复 > 判 gap 后从最后确认游标重发 subscribe（不静默停摆）`
  修复前 `waitUntil` 超时（`connection.test.ts:249`）；修复后第二条 `sync.subscribe` 的 cursor 为
  `{serverEpoch, globalSequence:"1"}`（断点，不是缺口后的 5，也不是 null），序号为 `"2"`。
- `…> 重新订阅后从断点续传的事件可正常处理`　先投 9 制造缺口，再投 2；修复前 `event` 计数为 0。

### MAJOR-2　`resumeCursor` 是构造期常量

**修复**（`client.ts`）：新增门面字段 `#ackedCursor`（跨连接存活，含 `ackedCursor` getter 供 WP7 组合根
读出并持久化），在 `snapshot_verified` / `event` / `online` 三条事件上推进；`connect()` 传给连接的
`resumeCursor` 与账本的 `resumeFrom` 都用它，不再读构造期常量。

**判别力**：上面 CRITICAL-2 的第一条用例同时断言了它——重连的 `sync.subscribe` body 从修复前的
`{"cursor":null,"scope":"machine"}` 变为 `{serverEpoch, globalSequence:"2"}`，断言在 `client.test.ts:350`。

---

## 3. 七条 MINOR / SUGGESTION 的逐条处置

| # | 发现 | 处置 | 理由 |
| --- | --- | --- | --- |
| 1 | `connection-machine.ts` 的 `connect_started` 允许从四个阻断态直达，与自身注释矛盾，且 `transition` 会把 `blocking` 清空 | **修**：从 `ALLOWED_SOURCES.connect_started` 删掉 `revoked`/`incompatible`/`identity_changed`/`replaced`，恢复必须经 `blocking_cleared → disconnected → connect_started`；注释改为陈述表本身的事实 | 表与注释只能有一个是对的。删掉之后 `transition` 对非法迁移抛错，`blocking` 不再被静默丢弃，R12 的「原因与下一步」得以保留 |
| 2 | `discard()` 完全忽略 `reason`（`void reason`），七种原因到不了消费方 | **修**：`SnapshotStaging` 记录 `#lastDiscardReason` 并暴露 getter；新增 `connection_closed` / `connection_blocked` 两个原因，因为断线与阻断**不是** `reset_required`，硬编码成它本身就是诊断信息失真；`discardStaging(reason)` 接受参数 | 「为什么这次快照没提交」必须可观测；给两个新原因是为了让调用点不再谎报 |
| 3 | `retrySameRequest` 允许从 `accepted` 退回 `submitting` | **修**：`accepted` 与终态同样拒绝重试，错误信息指向 `command.status` | 服务端已确认的结论不应被本地回退；`accepted` 之后该做的是查询而不是重发 |
| 4 | `mayDisplayAsAccepted` 额外要求在线 | **接受为已知非阻断项，未改**；补一条用例把现状钉住并在注释里写明依据 | **`docs/FRONTEND_DESIGN.md` §5 明文要求**「只有进入 `online` 后，客户端才能把命令显示为已被服务器接受」。评审的依据是 spec 只禁止「确认前显示已接受」，但设计文档给的是更严的约束，而设计文档不在本包 Write Scope 内。改代码去迎合 spec 会与 docs 冲突，属越界。已加 `MINOR-4` 用例 + 引用行号，供后续裁决 |
| 5 | `assertPagesDoNotOverlap` 注释承诺「不重不漏」但只查重 | **修**：注释改为分列两项可验证的内容，**并补上页边界校验**（较早页不得含比最新页最早一条更新的消息） | 既然写「不漏」就该真做一点；wire 上没有页边界声明，所以注释同时写明这不是页级完整性的证明 |
| 6 | `earliestCursor` 的平票裁决依赖服务端页内次序，无文档无用例 | **修**：`earliestCursor` 的注释新增「前置条件：服务端按 `(createdAt, messageId)` 升序返回同一页」一节，并说明违反时的两种后果（重复抛错 / 漏则静默）；补两条用例（页内同刻取首次出现者、epoch 毫秒比较） | spec 禁止按 `messageId` 排序，因此这个前置条件无法在客户端消除，只能写成契约并用用例固定当前语义。`server::sync` 未落地，无法对拍，故以文档 + 用例代替 |
| 7 | `earliestCursor` 用字符串 `<` 比较时间戳，字典序等于时序是隐式假设 | **修**：改用 `Date.parse` 的 epoch 毫秒比较；补一条跨毫秒精度的用例 | 让假设显式化；将来放宽时间戳精度时不会静默出错 |
| 8 | `client.ts` 类注释称「把 `SyncClientEvent` 翻译成连接状态机事件」，实际只是原样转发；`connection-machine.test.ts` 阻断断言用 `?? 0` 兜底 | **修**：类注释改为「**转发**……由组合根（WP7）翻译」，并补一行说明门面跨连接持有已确认游标；阻断断言改为先 `expect(blocked.blocking).not.toBeNull()` 再断言字段 | 文档与实现的偏差会误导 WP7 去门面找翻译逻辑 |

---

## 4. 写入范围与分层

- `git diff --name-only 263d3ba..545c895` 共 26 个文件，**全部**落在
  `clients/app/src/sync-client/` 与 `clients/app/src/state/`。
- 本轮**新增**的改动只涉及 13 个文件（其中 6 个源码、6 个测试、1 个 harness）。
- `src/layers.test.ts` 9 用例全绿：分层方向合法，无 `import ... from "src/platform"`（含 `import type`）。
- 未新增 npm 依赖，未新增 `.native.ts`，未改动 `scripts/run-browser-check.mjs`。

---

## 5. 未验证项（如实声明）

| 项 | 状态 | 说明 |
| --- | --- | --- |
| `deps` / `advisories` / `secrets` | **未执行、未声称通过** | CI-only job，本机无 CI 环境 |
| 无真实 Daemon 可连 | **未验证** | 多帧与跨连接时序仍由替身驱动，但已从「逐个 await」改为 burst 形态（不逐个 await），这正是 r1 逃逸的成因 |
| 固定向量字节对拍 | **未做** | 归 TP3（与 `tasks.md` 2.6 的分工一致） |
| `HostIdentityPort` / `DigestPort` 真实实现 | **不存在** | 判定逻辑由假端口覆盖；`RandomPort`/`DigestPort` 的真实实现是 WP7 组合根的前置工作 |
| 重连退避与 `ClockPort` 消费方 | **不在本包** | 归 WP7/TP4；R12 的「阻断态停止自动重连」由 `shouldAutoReconnect` 满足 |
| 服务端页内 `(createdAt, messageId)` 升序 | **无法对拍** | `server::sync` 未落地；已写成显式前置条件 + 用例固定语义 |
| 迁移覆盖 12×10 逐一覆盖 | **未做** | 沿用 r1 判断：封闭表的枚举式断言边际收益低于维护成本，且 r1 已确认它能测 |

---

## 6. 给 WP7 的交接

- `SyncClient.ackedCursor` 是新的公开读出口，组合根据此把游标持久化（`SyncClientOptions.resumeCursor`
  是**首次**注入点，不是每次连接的覆盖值）。
- WP7 仍需自建 `TranscriptCodec` / `HostIdentityPort` / `DigestPort` / `RandomPort` / `ClockPort`
  五个端口的实现（`PlatformPorts` 里没有它们），`HostIdentityPort` 缺失会让握手直接进阻断态。
- `SyncClientEvent` 到 `ConnectionEvent` 的翻译在组合根，**不在** `SyncClient`（类注释已更正）。