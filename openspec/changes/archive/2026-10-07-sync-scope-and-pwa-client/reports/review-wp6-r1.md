# WP6 独立检视报告（review-6 / tasks 3.14）

- 被检对象：`D:\Project\acp-remote\.worktrees\wp6` @ `2302c4155faa54ecbfb1666013d78a5270bb61a1`
- 基线：`263d3ba8b48ae5dae8ab87b16bc4c0ea86f62314`
- 作者报告：`reports/deliver-wp6-r1.md`
- 日期：2026-10-05
- 纪律：只读检视；所有临时探针与变异已还原，最终 `git status --porcelain` 为空、HEAD 未动。

---

## 0. 结论

**FAIL — 阻断合入。**

CRITICAL 3 项、MAJOR 2 项、MINOR 6 项、SUGGESTION 2 项。

三条 CRITICAL 都发生在**同一条真实运行路径**上（服务端 burst 下发快照 chunk → 客户端断线重连 → 客户端 ACK/出站信封），
而这三条路径恰好是本包声称已交付的核心（任务 2.7 的「快照暂存与原子替换」「ACK + 增量重放」「事件按 eventId 去重」），
并且**现有 234 个用例一条都没覆盖**。作者的「判别力证据」在本包自己的 fake socket 抽象下成立，
但 fake socket 把服务端的多帧下发折叠成了同步串行，恰好掩盖了这三条缺陷。

作者报告中列出的 13 项变异我复现了 5 项（A/B/L/M/G），**全部真实变红**——判别力本身没有造假。
问题在于被测面选得偏窄：变异测试证明的是「已被写到的用例确实在测」，不能证明「该测的都测了」。

---

## 1. 必查重点逐条核对

| # | 必查项 | 结论 | 证据 |
| --- | --- | --- | --- |
| 1 | 互斥状态机无非法组合 | **通过（有一处契约自相矛盾，见 M-1）** | `state/connection-machine.ts:23-36`（单值 12 态封闭联合）、`:132-143`（机器状态只有 `{state,blocking,caughtUp}` 三键）、`:166-205`（`ALLOWED_SOURCES` 迁移表，表外抛 `IllegalConnectionTransition`）、`:243-266`（阻断映射与 reason/nextStep）、`:277-279`（`shouldAutoReconnect` 在阻断态为假）、`:286-288`（`canMarkInputAsSent` 仅 `online`） |
| 2 | `requestId` 复用 | **通过** | `state/command-machine.ts:174-181`（`retrySameRequest` 只增 `dispatchCount`，ID 不变；`:83-85` `markSubmitting` 保留 ID）、`sync-client/client.ts:172-186`（`dispatch` 以 `requestId` 为键）、`:189-207`（`buildStatusQuery` 自身新 ID、`targetRequestId` 指回原 ID）。全仓无任何「同一意图另生成 ID」的路径 |
| 3 | 快照暂存原子替换 | **逻辑结构通过，但投递路径有 CRITICAL 竞态（C-1）** | 结构面：`snapshot.ts:99-101`（暂存器不持有已完成状态）、`client.ts:38-59`（`SnapshotRepository.replace` 单次赋值）、`connection.ts:485-496`（`reset_required` 只丢暂存、不碰仓库）。digest 只覆盖**已到达** chunk 的原始字节（`snapshot.ts:156`、`:186-189`），且只 merge 三类清单资源（`snapshot.ts:206-213`，`SnapshotResource` 是 3 值封闭词表，`protocol/sync.ts:68`）——D1 满足。**但 `chunkCount>1` 时在真实 socket 上必然失败**，见 C-1 |
| 4 | 明细游标不重不漏 | **通过（一处依赖服务端页内次序，见 S-1）** | `paging.ts:29-33`（复合游标含 `createdAt` + `messageId`）、`:71-80`（任一分量缺失即抛 `IncompleteReadCursorError`，不补全）、`:53-69`（`earliestCursor`）、`:117-137`（`toLoadedPage` 在 `hasEarlier` 缺失/与空页矛盾时抛错）、`:141-153`（相邻页重叠即抛错） |
| 5 | 未确认接受前不显示已接受 / `uncertain` 不由断线断定 | **通过** | `state/command-machine.ts:88-95`（`markConnectionLost` 是恒等函数且**不接受任何参数**）、`:104-141`（`uncertain` 只能由 `applyCommandResult` 进入）、`:144-171`（`applyStatusRecord` 同理）、`:186-189`（`mayDisplayAsAccepted` 要求在线且 `acceptedAt` 非空）、`:194-196`（`shouldPresentUncertain` 要求 `serverConfirmedUncertain`）。`uncertain` 的每一个进入点都要求服务端 payload，没有本地推断路径 |

---

## 2. 独立核实项

### 2.1 写入范围 —— 通过

`git diff --name-only 263d3ba..2302c41` 共 26 个文件，**全部**落在 `clients/app/src/sync-client/` 与 `clients/app/src/state/`。
`src/platform/**`、`src/protocol/**`、`src/domain/**`、`package.json`、`tsconfig` 均未触碰。
新增的 `testing/harness.ts` 在 `src/sync-client/` 内，未越界。

### 2.2 分层 —— 通过

`src/layers.test.ts` 9 个用例全绿（基线 run 中实测）。`src/state`(4) → `src/protocol`(7)、`src/sync-client`(5) → `src/domain`(6)/`src/protocol`(7) 方向合法；全包无 `import ... from "src/platform"`，含 `import type`。`src/sync-client/index.ts` 确为该层唯一公开面（`index.ts:10-12` 自述，其余模块未被 re-export）。

### 2.3 对 WP7 的可用性 —— 交接缺口为真，且比作者描述更严重

- 缺口属实：`clients/app/src/platform/index.ts:52-58` 的 `PlatformPorts` 只有 `deviceIdentity` / `localCache` / `importedContent` / `lifecycle`，
  **没有** `TranscriptCodec` / `HostIdentityPort` / `DigestPort` / `RandomPort` / `ClockPort` 的任何成员，
  `src/platform/` 全目录也搜不到 `sha256` / `getRandomValues` / `randomUUID` 的实现（只有测试里 `node:crypto` 的对拍）。
  WP7 必须自建这五个端口的实现，否则 `SyncClient` 构造不出来。**不会把 WP7 卡死**（端口是纯接口，组合根可自行实现），
  但工作量必须显式排进 WP7，且 `HostIdentityPort` 缺失会让握手在 `connection.ts:317-322` 直接进阻断态。
- **`ClockPort` 无消费方不只是冗余**：`plan.md` 把退避策略放在状态层，而状态层在本包里没有任何消费 `ClockPort` 的代码，
  意味着 R12「阻断态停止自动重连」之外的**非阻断重连退避在本包完全没有实现**，作者报告 §5.7 已如实声明，定性为**可接受的未实现**（WP7/TP4 归属），但 TP3 的迁移覆盖要把它算进去。
- `connection.ts:97-150` 的类注释声称「把 `SyncClientEvent` 翻译成连接状态机事件」，但 `client.ts:228-247` 只是原样转发，
  翻译留给 WP7。文档与实现的偏差属**表述问题**，记 S-2。

### 2.4 测试是否真在测 —— 基本通过，两处弱断言

- 扫了全部 8 个新测试文件的 `toBeDefined()` / `toBeTruthy()` / `toBeGreaterThan(0)`：
  - `client.test.ts:240`、`:243`、`connection.test.ts:96` 是 `last()` / `proof` 的存在性兜底，**弱但不是恒真**（同名断言在别处有内容断言）。
  - `connection-machine.test.ts:180-181` 用 `?? 0` 兜底后比长度——只要 `blocking` 为 `null` 就会得到 `0 > 0` 为假而变红，**能测**，但换成 `expect(blocked.blocking).not.toBeNull()` 更直白。记 S-2。
- 未发现把被测逻辑复制进测试的实例；`expectedSnapshotDigest`（`harness.ts:164-171`）是**独立复算**而非复用被测实现，判别力成立。
- 未发现恒真断言、未发现的 `it.skip` / `#[ignore]` 式跳过。

### 2.5 未验证项的定性

| 作者声明 | 我的定性 |
| --- | --- |
| 无真实 Daemon 可连 | **可接受**。但请注意：这正是 C-1/C-2/C-3 逃逸的直接原因——所有多帧 / 跨连接时序都被 fake socket 的同步 `deliver()` 折叠掉了 |
| 固定向量字节对拍未做（归 TP3） | **可接受**，与 `tasks.md` 2.6 的分工一致 |
| `HostIdentityPort` 无真实实现 | **可接受的未验证**。判定逻辑本身（`:317-322` 不匹配即阻断且不发 `client_proof`）有假端口覆盖 |
| `DigestPort` 用确定性替身 | **可接受的未验证**。digest 的不变量（两级哈希、按 index 连接、覆盖已到达 chunk 的**原始**字节）由独立复算判定，替身不影响判别力 |
| `ClockPort` 无消费方 | **可接受但需移入 WP7/TP4 排期**（见 2.3） |
| 重连退避未实现 | **可接受**，属状态层/WP7 |
| 迁移覆盖是枚举式断言而非 12×10 逐一覆盖 | **可接受**。`connection-machine.test.ts:270-300` 已断言不可达边（如 `unpaired --connect_started-->` 抛错）与每个事件来源非空；`ALLOWED_SOURCES` 是封闭表，全 12×10 逐一断言的边际收益低于它引入的维护成本 |
| `deps`/`advisories`/`secrets` 未跑 | **可接受**，CI-only |

---

## 3. 分级发现

### CRITICAL-1　多 chunk 快照在真实 socket 上必然失败（chunk 竞态）

- **位置**：`clients/app/src/sync-client/connection.ts:402`；`clients/app/src/sync-client/snapshot.ts:146` / `:156` / `:159`
- **现象**：`#onMessage` 用 `void this.#onSnapshotChunk(...)` 即发即忘。`SnapshotStaging.acceptChunk` 在 `:146` 读 `staged.receivedChunks` 做连续性判定后，在 `:156` **await** `digest.sha256`，直到 `:159` 才把 `receivedChunks + 1` 写回。服务端在同一条 WSS 上连续下发 chunk 0/1/2 时，chunk 1 的处理在 chunk 0 的 `await` 处被抢占，读到的仍是 `receivedChunks === 0`，`chunkIndex !== "0"` → `chunk_out_of_order` → 整个暂存区被 `discard()`。
- **影响**：只要 `chunkCount >= 2`（即协商了 `core.local-catalog.v1` 的**正常情况**，D1 明说全量为 3），初始同步就永远拿不到目录快照 → 目录页、Agent 列表、离线渲染全部不可用。命中 spec「快照只承载清单类资源」场景「本地会话的明细不进入快照」的失败态，以及 R12「已断开…本地保有上一次成功替换的快照」无从谈起。
- **复现**（我实际执行过）：临时探针用 `FakeSocket.deliver()` 连续投递 3 个 chunk（这正是 WSS 的真实投递形态），随后投递 digest 完全匹配的 `snapshot_end`：
  ```
  P3 kinds: ["authenticating","authenticated","replaying","rejected","rejected","rejected"]
  P3 rejects: [{"reason":"chunk_out_of_order","detail":"chunkIndex 不连续"},
               {"reason":"snapshot_id_mismatch","detail":"收到 chunk 但没有进行中的快照"},
               {"reason":"snapshot_id_mismatch","detail":"没有进行中的快照"}]
  P3 verified: 0
  ```
- **为什么 234 个用例没抓到**：`snapshot.test.ts:46-54` 每个 chunk 之间都 `await` 了 `acceptChunk`，把竞态消掉了；`client.test.ts` 的 socket 级路径**只投递过 chunkIndex "0" 一个 chunk**（`:191`/`:208`/`:224`/`:254`，即使 `chunkCount: 3` 也只发一块）。多 chunk 的 socket 路径零覆盖。
- **建议处置**：在 `SnapshotStaging` 内把「校验 + 计数推进 + 资源合并」做成同步段，digest 计算改为串行队列（在 `complete()` 时统一按 index 连接计算），或让 `#onMessage` 对 chunk 走一条串行 promise 链。补一条 socket 级多 chunk 用例。

### CRITICAL-2　重连后去重账本被重置 → 重复事件被二次呈现

- **位置**：`clients/app/src/sync-client/client.ts:234-235`
- **现象**：`SyncConnection.#onAuthenticated` 先调 `ledgerFor(serverEpoch)` 拿到跨连接复用的账本，紧接着发 `authenticated` 事件；`SyncClient.#onConnectionEvent` 收到后**无条件** `this.#ledger = new EventLedger({ serverEpoch })`，既丢掉复用账本（`seenEventIds` 全没），又把 `resumeFrom` 置空（`#lastSequence` 回到 `null`）。下一次 `connect()` 的 `ledgerFor` 因 epoch 相同而返回这个残废账本。
- **影响**：违反 R14「重复投递同一事件 MUST NOT 造成重复呈现或重复执行」与场景「重复事件不重复呈现」。更糟的是 `resumeFrom` 为空使 `EventLedger.evaluate`（`dedupe.ts:88-97`）要求首条 `globalSequence` 恰好为 1，而重连后服务端从最后 ACK 游标续发（如 2319）→ 判 `gap` → `connection.ts:459-462` 直接 `return`，事件被静默丢弃且**没有任何重新订阅**，此后每条事件都判 gap，事件流永久停摆。两条 MUST 同时被破坏。
- **复现**（我实际执行过）：连接 1 认证后收 seq 1/2（2 条 `event`），`serverClose(1006)`，`client.connect()` 重新认证，再重投 seq 1：
  ```
  P1 reconnect duplicate kinds: ["authenticating","authenticated","replaying","event"]
  ```
  最后一位是 `event` 而非 `duplicate_dropped` —— 同一条事件被二次呈现。改投 seq 2319 则得到 `gap_detected` 且无任何 `sync.subscribe` 补发。
- **为什么没抓到**：`client.test.ts:72` 的 `resumeCursor` 恒为 `null`，且全文件无任何重连用例；`dedupe.test.ts` 只在单连接内测账本。
- **建议处置**：删掉 `client.ts:234-235`（`ledgerFor` 已是唯一构造点），并把「重连后重投已见事件必须判 duplicate」写成 `client.test.ts` 的用例。

### CRITICAL-3　出站 `connectionSequence` 恒为 `"0"` 且从不递增；`subscribe` 借用服务端计数器

- **位置**：`clients/app/src/sync-client/connection.ts:178`（`#connectionSequence = 0`，全文件无 `++`）、`:375`（`subscribe` 用 `message.connectionSequence`）、`:426`、`:491`、`:596`；`clients/app/src/sync-client/client.ts:197`（`buildStatusQuery` 硬编码 `"0"`）
- **现象**：`docs/SYNC_PROTOCOL.md:136` 规定 `connectionSequence`「每个方向独立，从 `1` 开始严格加一」，`:141` 规定「sequence 重复、回退、跳号或 connection ID 不匹配均为 `protocol.sequence_invalid`」，`:141` 还规定「两个方向分别维护 sequence，不能相互共用计数器」。本实现三条全违反：(a) 起点是 0 不是 1；(b) 计数器从不递增，每条 ACK 都是 `"0"`；(c) `subscribe` 直接把服务端 `auth.authenticated` 的序号抄了过来。
- **影响**：真实服务端会在第一条 ACK 就返回 `protocol.sequence_invalid` 并断连，ACK 循环根本不成立，R14 的「从最后确认的位置连续恢复」无从谈起；即便服务端宽容，`sync.ack` 重复序号也会被拒。
- **复现**（我实际执行过）：一次连接内投递 2 条事件后打印全部出站信封：
  ```
  P2 outbound seqs: ["auth.client_hello:undefined","auth.client_proof:undefined",
                     "sync.subscribe:1","sync.ack:0","sync.ack:0"]
  ```
  两条 `sync.ack` 同为 `"0"`（重复），`sync.subscribe` 的 `1` 来自服务端信封而非客户端自己的计数器。
- **为什么没抓到**：`connection.test.ts` 断言 ACK 时只看 `body.cursor` 与条数，没有一个用例断言 `connectionSequence`；`wire.ts` 只解码入站，不校验出站。
- **建议处置**：新增客户端方向计数器（从 1 开始，每条认证后出站消息严格 +1），`subscribe` 用自己的计数器首值 1，`buildStatusQuery` 同理；补一条「同一连接内连续 ACK 的 sequence 严格递增且从 1 开始」的用例。

### MAJOR-1　`gap_detected` 之后从不重新订阅，事件流永久停摆

- **位置**：`clients/app/src/sync-client/connection.ts:459-462`；事件声明 `connection.ts:108`
- **现象**：事件注释写「序号出现缺口：客户端从最后确认位置重新订阅，不推进游标」，但代码只 `onEvent({kind:"gap_detected"})` 然后 `return`；全仓 grep `gap_detected` 只有定义、发出、以及 `connection.test.ts:234` 的断言，**没有任何重新发 `sync.subscribe` 的代码**（`#onResetRequired` 会重发 `sync.snapshot_request`，但那是另一条路径）。
- **影响**：一次乱序/丢帧就永久停摆，且是静默的。违反 R14「客户端 SHALL 从最后确认的位置连续恢复，…MUST NOT 出现丢失区间」。与 CRITICAL-2 叠加后成为必然结果。
- **建议处置**：在 gap 分支用 `#acked` 重发 `sync.subscribe`（并对连续 gap 做退避/上限），或在 `SyncClient` 里处理 `gap_detected` 事件。

### MAJOR-2　`resumeCursor` 是冻结的常量，重连后永远从 `cursor: null` 重新订阅

- **位置**：`clients/app/src/sync-client/client.ts:80`（`readonly resumeCursor`）、`:143`、`:216`
- **现象**：`SyncClientOptions.resumeCursor` 是构造期常量，`#ledgerFor` 与 `connect()` 都直接读它；已推进的 `#acked` 只存在于单个 `SyncConnection` 实例内，连接一销毁就丢失。类注释（`client.ts:85-87`）自述「一个实例覆盖整个应用生命周期」。
- **影响**：违反 R14「从最后确认的位置连续恢复」。每次重连都让服务端从事件库开头重放，配合 CRITICAL-2 的账本重置，客户端会把整段历史重复呈现一遍。
- **复现**（我实际执行过，同 P1 探针）：
  ```
  P1 reconnect subscribe payload: [{"cursor":null,"scope":"machine"}]
  ```
  第一条连接已处理到 seq 2，重连后的 `sync.subscribe` 仍是 `cursor: null`。
- **建议处置**：由门面持有已 ACK 的游标（`SyncConnection` 通过回调上报 `#acked` 变化），`connect()` 用它而不是 `options.resumeCursor`。

### MINOR-1　迁移表允许 `connect_started` 从 4 个阻断态直达，与其自身文档注释矛盾

`connection-machine.ts:172-182` 的 `ALLOWED_SOURCES.connect_started` 含 `revoked`/`incompatible`/`identity_changed`/`replaced`，
但 `:165` 的注释断言「`replaced` 的恢复必须由用户显式动作（`blocking_cleared` → `disconnected` → `connect_started`）触发」。
两者只能靠「调用方记得先查 `shouldAutoReconnect`」区分，表本身表达不出「用户动作 vs 自动重连」。
一旦调用方直接发 `connect_started`，`transition`（`:239-241`）会把 `blocking` 置 `null`，R12 的「说明原因与可行的下一步」被静默丢弃。
实际 MUST 由 `shouldAutoReconnect`（`:277-279`）兜住，且 `BLOCKING_DETAILS` 的 nextStep 本身就写着「在此页手动重新连接」，
所以不是可证明的 bug，是契约自相矛盾。建议表里删掉阻断态来源，或把事件拆成 `connect_started` / `user_connect_requested`。

### MINOR-2　`discardStaging()` 把原因硬编码成 `reset_required`

`connection.ts:250-252` 与 `:551`（`#block`）都调 `this.#staging.discard("reset_required")`。
`SnapshotDiscardReason`（`snapshot.ts:43-59`）里已经区分了 7 种原因，而 `discard` 的实现（`snapshot.ts:198-201`）是 `void reason`——**reason 参数根本没被记录**。
即「为什么没提交」的结构化原因目前是完全不可观测的，诊断信息丢失。建议 `SnapshotStaging` 记录最后一次 discard 原因供诊断/断言。

### MINOR-3　`retrySameRequest` 允许从 `accepted` 回到 `submitting`

`command-machine.ts:174-181` 只拦终态，`accepted` 不是终态，因此已确认接受过的命令可被重试成 `submitting`，
`mayDisplayAsAccepted`（`:186-189`）随之变 false——已经服务端确认的状态被本地回退成「已发出，等待服务器确认」
（`PENDING_CONFIRMATION_LABEL`）。建议对 `accepted` 也拒绝重试，或至少保留 `acceptedAt` 呈现。

### MINOR-4　`mayDisplayAsAccepted` 把「在线」当作呈现已接受的前提

`command-machine.ts:188` 要求 `connectionOnline`。断线后一条**已被服务端确认接受**的命令不再显示为已接受，
会从「已接受」闪回未确认态。spec 只禁止「确认前显示已接受」，没有要求断线后撤回确认，属过严但会误导用户。建议只依据 `acceptedAt`。

### MINOR-5　`assertPagesDoNotOverlap` 的注释承诺「不重不漏」，实现只查「不重」

`paging.ts:141-153` 的函数名与实现一致（只查 `messageId` 重复），但 `:138-140` 的注释写「验证『不重不漏』」。
「漏」需要比较两页的 `createdAt` 边界才能判定，当前没有。属文档过度承诺。建议改注释或在 `LoadedPage` 层补边界断言。

### MINOR-6　`earliestCursor` 的平局裁决依赖服务端页内次序

`paging.ts:53-69` 只按 `createdAt` 比较，`<` 严格小于意味着同一 `createdAt` 取**页内首次出现**的那条。
若服务端页内不是按 `(createdAt, messageId)` 升序，取到的就不是该时刻 `messageId` 最小的那条；
下一次 `before` 就会跳过或重复同刻消息（重复会被 `assertPagesDoNotOverlap` 抛错，漏则静默）。
spec「不按标识排序」确实禁止用 `messageId` 排序，所以不能简单改成双键比较——但当前实现把正确性完全押在服务端页内次序上，
且没有任何用例覆盖「页内同刻乱序」。不确定点：服务端行为我无法在本仓核对（`server::sync` 未落地）。建议至少补一条注释说明该前置条件。

### SUGGESTION-1　`earliestCursor` 里 `candidate.createdAt` 的比较用字符串 `<`

时间戳是固定 `YYYY-MM-DDTHH:mm:ss.sssZ`（`protocol/sync.ts` 的 `TIMESTAMP_PATTERN`），字典序等价于时序，成立。
但若将来放宽时间戳精度，这里会静默出错。用一次显式的 epoch-millis 转换更稳。

### SUGGESTION-2　`connection-machine.test.ts:180-181` 与 `client.ts:229` 的文档/断言可更直白

前者用 `blocked.blocking?.reason.length ?? 0` 表达「阻断态必须带原因和下一步」，改成 `expect(blocked.blocking).not.toBeNull()` 后再断言字段更清晰；
后者 `client.ts:5-8` 的类注释称「把 `SyncClientEvent` 翻译成连接状态机事件」，实现是原样转发（翻译在 WP7），建议把注释改成「转发，由消费方翻译」。

---

## 4. 我实际执行的复现命令与输出摘要

所有临时改动均已用 `git checkout -- <file>` 还原，**最终 `git status --porcelain` 为空，HEAD 仍为 `2302c41`**。

### 4.1 基线门禁

```
$ npx vitest run --config vitest.config.ts
 Test Files  17 passed (17)
      Tests  234 passed (234)
```
（`src/layers.test.ts` 9 passed；分层禁令无违规。）

### 4.2 探针复现（临时文件 `zz-probe.test.ts` / `zz-probe2.test.ts`，已删除）

```
P1 reconnect duplicate kinds: ["authenticating","authenticated","replaying","event"]
P1 reconnect subscribe payload: [{"cursor":null,"scope":"machine"}]
P2 outbound seqs: ["auth.client_hello:undefined","auth.client_proof:undefined",
                   "sync.subscribe:1","sync.ack:0","sync.ack:0"]
P3 kinds: ["authenticating","authenticated","replaying","rejected","rejected","rejected"]
P3 rejects: [{"reason":"chunk_out_of_order","detail":"chunkIndex 不连续"},
             {"reason":"snapshot_id_mismatch","detail":"收到 chunk 但没有进行中的快照"},
             {"reason":"snapshot_id_mismatch","detail":"没有进行中的快照"}]
P3 verified: 0
```

### 4.3 变异测试复现（作者声称的 A/B/L/M/G，逐项独立执行后还原）

| 变异 | 实际改动 | 我的复现结果 | 与作者自报 |
| --- | --- | --- | --- |
| A | `dedupe.ts` 去重键 `eventId`→`messageId`（`evaluate` + `commit` 两处） | `Tests 3 failed | 25 passed (28)` | 一致（作者记 1，实为 3） |
| B | `dedupe.ts` 去重键 `eventId`→`globalSequence` | `Tests 1 failed | 27 passed (28)` | 一致 |
| L | `agent-overlay.ts` 从 `session.updated` 的 `session.agent.agentId` + `state==="running"` 推断已连接 | `Tests 2 failed | 9 passed (11)`，红点为「会话与 turn 事件不改变覆盖层」「会话处于 running 也不得推断 Agent 已连接」 | 一致 |
| M | `connection.ts#onResetRequired` 去掉 `this.discardStaging()` | `Tests 1 failed | 39 passed (40)`，红点为「reset 之后到达的 snapshot_end 不得完成替换（与未完成暂存的 chunk 完全匹配也无效）」 | 一致 |
| G | `command-machine.ts#retrySameRequest` 生成新 `requestId` | `Tests 12 failed | 18 passed (30)` | 一致（作者记 5，实为 12） |

补充说明：作者报告里 L 项我第一次按字面实现（新增 `session.agent` 事件类型分支）时**没有变红**——因为 `projectView` 对该 eventType 返回 `kind !== "known"`，分支根本走不到。
换成「从 `session.updated` 的既有视图推断」后如期变红。这说明作者的 L 变异描述略不精确，但结论正确。

**判别力结论：作者关于已写用例的判别力声明属实。** 问题不在这 13 项，而在覆盖面——多 chunk socket 路径、跨连接重连路径、出站信封序号三条主路径无用例。

---

## 5. 对后续单元的影响

- **WP7（TUI/页面层）**：可开工，但 `SyncClient` 的接线目前**不可能工作**——CRITICAL-1/2/3 都在 WP7 之下。WP7 若先行会写到一条永不产生活目录快照、永不 ACK、重复呈现历史事件的链上。建议先修 CRITICAL 再派 WP7。
- **WP7 的五个注入端口**：`TranscriptCodec` / `HostIdentityPort` / `DigestPort` / `RandomPort` / `ClockPort` 确实不在 `PlatformPorts`（`platform/index.ts:52-58`）里，缺口为真，不构成死锁，但需在 WP7 任务里显式列出「实现这五个端口」的前置工作；`RandomPort`/`DigestPort` 可直接用 `crypto.getRandomValues` / `crypto.subtle.digest`。
- **TP3（对拍与状态迁移覆盖）**：固定向量对拍按分工归 TP3，OK。但 TP3 的迁移覆盖若要挑刺，`connection-machine.test.ts` 的枚举式断言已经足够，**不需要**改成 12×10 逐一覆盖。
- **MU3c 出口条件**：当前状态不满足 tasks 2.7 的「快照暂存与原子替换」「ACK + 增量重放」「事件按 eventId 去重」三条。建议 WP6 出 r2。

---

## 6. 复核建议（给 coder）

按此顺序修，每修一项补一条**不靠 fake socket 同步折叠**的用例：

1. CRITICAL-1：`SnapshotStaging` 的 chunk 处理去竞态（同步段 + 串行 digest），补 socket 级多 chunk 用例。
2. CRITICAL-2：删掉 `client.ts:234-235`，补「重连后重投已见 `eventId` 判 duplicate」与「重连后首个事件 `globalSequence > 1` 不被判 gap」两条用例。
3. CRITICAL-3：加客户端方向 `connectionSequence` 计数器，补「同连接 ACK 序号从 1 严格递增」。
4. MAJOR-1：gap 后重发 `sync.subscribe`。
5. MAJOR-2：门面持有已 ACK 游标并用于重连订阅。
6. MINOR-1/2/3/4：按上文逐条处置或明确记录为接受。

