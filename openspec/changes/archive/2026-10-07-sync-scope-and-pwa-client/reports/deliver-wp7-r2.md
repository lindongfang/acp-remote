# WP7 交付报告 r2（coding-7，修复 review-wp7-r1 的 CRITICAL-1 + MAJOR-1/2，并落地组合根）

- **分支 / 提交**：`feat/wp7` @ `f6ea252`（基线 `221e76e`，本包基线 `4ada2a2`）
- **被检视对象**：`reports/review-wp7-r1.md`（FAIL：1 CRITICAL + 2 MAJOR）
- **门禁**：`npm run check` 十道全绿（exit 0）；`clients/app` 的 `tsc --noEmit` / `vitest run`（28 文件 359 条）/ `run-browser-check.mjs`（16/16）/ `expo export --platform web` 全部 exit 0。原始输出见 `reports/PV3-wp7-r2.log`。

---

## 1. 结论

| 编号 | 处置 | 判别力（修复前红 → 修复后绿） |
| --- | --- | --- |
| CRITICAL-1 | 已修 | 2 failed / 3 passed → 5 passed |
| MAJOR-1 | 已修 | 4 failed / 9 passed → 6 passed（本文件 6 条） |
| MAJOR-2 | 已修 | 同上（与 MAJOR-1 同批变异） |
| 组合根（§5.1 设计缺口） | 已落地 | 3 处变异各自变红 |

既有 338 条用例**未删除、未改写、未加跳过**：新增 21 条后为 359 条，27 个既有测试文件全部仍绿（`git diff 4ada2a2..HEAD -- '*.test.ts'` 中 r1 交付的用例行净改动为 0，本轮只新增两个测试文件）。

---

## 2. CRITICAL-1：epoch 变化后合法快照被门面永久丢弃

### 2.1 失效链与修法

reviewer 的失效链复述无误。修法取 reviewer 给出的**第一个方向**（在换 epoch 时按 epoch 判定清空恢复点），落点在 `client.ts` 的 `#ledgerFor`：

```ts
const acked = this.#ackedCursor;
const resumeFrom = acked !== null && acked.serverEpoch === serverEpoch ? acked : null;
const ledger = new EventLedger({ serverEpoch, resumeFrom });
this.#ledger = ledger;
// 换 epoch 时恢复点必须**一起**丢掉（与 `connection.ts` 的 `#onAuthenticated` 同一口径）。
if (acked !== null && acked.serverEpoch !== serverEpoch) this.#ackedCursor = null;
```

**为什么是这个方向而不是第二个**（对照 `this.#ledger?.serverEpoch`）：两个方向都能让正向用例转绿，但改基准只把「基准是谁」换了个对象，仍然留下同一个隐患——`ackedCursor` 对外（`get ackedCursor()`）会持续暴露一条**永远比较不出前进**的旧 epoch 游标，而组合根正是把它持久化成下次连接 `resumeFrom` 的那一份。清空则让「恢复点与账本同 epoch」成为结构性不变量，`#isAdoptableBarrier` 的基准因此永远有效。

这与 `connection.ts:442-445` 是同一口径：连接层清 `#acked`/`#lastAcked`，门面对 `#ackedCursor` 做同一件事。三份游标（连接层 `#acked`、门面 `#ackedCursor`、账本 `resumeFrom`）此后同生共死。

### 2.2 用例：`src/sync-client/epoch-rebuild-barrier.test.ts`（新增 5 条）

走真实生产路径：`SyncClient.connect()` → 真 `SyncConnection` → 假 socket 驱动握手与快照（**不** mock 门面）。

| 用例 | 作用 |
| --- | --- |
| 服务端事件库重建：新 epoch 的快照替换仓库，恢复点前进 | 正向主用例，断言仓库 `snapshotId`、会话标题「重建后的标题」与 `ackedCursor` 三项 |
| 后续增量事件到来也救不回被丢弃的快照 | 先把恢复点经 `sync.caught_up` 抬到新 epoch 的 `"7"`，再断言仓库仍是重建后那份 —— 证明主用例不是靠事件推进侥幸通过的 |
| 外来 epoch 的屏障仍不被采纳 | 负向：`snapshot_barrier_epoch_mismatch` 后仓库与恢复点不变 |
| **同 epoch 且序号更旧的快照：连接层交付了它，门面必须拒** | 守卫的直接判别力，见下 |
| epoch 不变时新快照照常替换仓库 | 负向：修法没有把同 epoch 的正常同步也拒掉 |

第四条是本轮特意写的。`EventLedger.advanceTo` 对更旧的屏障返回 `already_covered` 而**不是** `rejected`，因此连接层**会**把它交给门面——这才是门面上那道冗余守卫唯一的用武之地。若把用例写成「连接层已拒」，它就退化成对连接层的断言，守卫本身零判别力，正是 `review-wp7-r1` §7 变异三指出的问题。

### 2.3 判别力数据（实测）

变异：**只删掉 `#ledgerFor` 里那一行清空**（其余全部保留，含新的守卫与用例）。

| 测试文件 | 变异后 |
| --- | --- |
| `epoch-rebuild-barrier.test.ts` | **2 failed** / 3 passed |
| `snapshot-barrier.test.ts`（r1 的 10 条） | 10 passed |
| `composition.test.ts` | 10 passed |
| 合计 | **2 failed / 23 passed** |

变红的两条正是重建后的正向用例；**MU3c 的 P1-N1/N2 十条一条没红**——账本拒绝的屏障仍不被采纳，与 CRITICAL-2 的要求一致。还原后 `src/sync-client/` 156 条全绿。

反向确认（防止修成「一律采纳」或「一律拒绝」）：

| 若把修法换成 | 结果 |
| --- | --- |
| 「一律采纳」（守卫恒真） | 「同 epoch 更旧的快照」与「外来 epoch」两条负向用例变红 |
| 「一律拒绝」（守卫恒假） | 两条正向用例变红（含「epoch 不变时新快照照常替换」） |

---

## 3. MAJOR-1：`raw_not_synced` 分支在生产管线不可达

### 3.1 修法

`client-store.ts` 不再自己拼 `raw`。新增 `degradation-model.ts` 的 `isDegradedEvent(event)`，由它按 `event.body.payload.acp` 得出原文可用性（`rawAvailability` 本来就是该模块的私有函数，判定与 `buildDegradedEventModel` 因此**同源**，不可能再分叉）：

```ts
function degradedModelFor(event: EventMessage): DegradedEventModel | null {
  if (!isDegradedEvent(event)) return null;
  return buildDegradedEventModel({ event });
}
```

### 3.2 用例：`src/components/r18-degradation-pipeline.test.ts`（新增 6 条，全部经 `ClientStore`）

一律走 `store.apply({kind:"event"})` → `store.conversationPage(...)` → `renderToStaticMarkup(ConversationStream)`（内部渲染 `DegradedEventCard`）。这正是 r1 用例缺的那一层。

### 3.3 判别力数据（实测）

变异：`degradedModelFor` 还原为 `isDegraded({..., raw: {kind:"absent"}})`，且 `ConversationStream` 的 `unsupportedBy` 还原为兜底 `"client"`、`noDedicatedView` 还原为 `true`。

| 测试文件 | 变异后 |
| --- | --- |
| `r18-degradation-pipeline.test.ts`（新增 6 条） | **4 failed** / 2 passed |
| `r18-degradation.test.ts`（r1 的 7 条） | 7 passed |
| 合计 | **4 failed / 9 passed** |

变红的四条：raw_not_synced 未产生、binary_not_delivered 谎称客户端不支持、原文未下发谎称客户端不支持、未登记事件被归到三类之一。**r1 的 7 条模型层用例一条没红**——这正是 review 指出的判别力缺口：模型本身是对的，错的是上游筛选与卡片呈现，而 r1 的断言全在模型层。

还原后 6 条全绿。

---

## 4. MAJOR-2：降级卡片对非 unsupported 原因谎称「客户端不支持」

### 4.1 修法

`ConversationStream.tsx` 的兜底删除：

```ts
const unsupportedBy = model.reason.kind === "unsupported" ? model.reason.by : null;
// 「客户端没有专用视图」只对前两类降级成立
const noDedicatedView =
  model.reason.kind === "unsupported" || model.reason.kind === "no_dedicated_view";
```

`DegradedEventCard` 的 `unsupportedBy` 改为 `UnsupportedBy | null`，并在 `null` 时不输出 `data-unsupported-by`、`data-unsupported-reason` 与「当前客户端未提供专用视图」。为此拆出 `noDedicatedView: boolean` 这第二个信号：两者不可互相推导——「未登记事件」要呈现「未提供专用视图」但**不**属于三类能力不支持之一，「能力不支持」两者都成立。

### 4.2 用例

同 §3.2 的生产路径文件，含 binary_not_delivered / raw_not_synced 的卡片层断言，另配两条对照：`unsupported_by_broker` 仍带 `data-unsupported-by="broker"` 与说明文案（证明修复没把归因一起删掉）、未登记事件保留「未提供专用视图」但无 `data-unsupported-by`（证明没把它冒充成三类之一）。

### 4.3 判别力数据

见 §3.3（同一次变异同时回退两条 MAJOR 修复，4 条变红）。

---

## 5. 组合根：`src/composition.ts`（review-wp7-r1 §5.1 的计划级遗漏）

### 5.1 为什么落在这里

reviewer 的技术论断已核实并成立：`layerOfSourcePath("src/composition.ts")` 返回 `null`，因此该文件在 `layers.test.ts:170` 的 `from === null` 分支被跳过，**可以** import `src/platform/`。`layers.ts` 与 `layers.test.ts` 一行未改，9 条分层断言仍全绿。

### 5.2 接线方式

```
app/_layout.tsx
  └─ createComposition()                       ← 模块作用域，装配一次，多路由共享
       ├─ openPlatform()                       deviceIdentity / localCache / importedContent / lifecycle
       ├─ 五个缺失端口的实现                    见下表
       ├─ HttpPairingTransport(fetch)          §7.1 / §7.2 / §7.3
       ├─ SyncClient(openWebSocket, …)
       └─ ClientStore(sync ← SyncClient 只读转发, identity ← connectionId)
```

`_layout.tsx` 在 `useEffect` 里 `await start()`（读 IndexedDB 的设备身份与配对记录）后才把 store 交给 Provider；`start()` 之前 `store` 为 `null`，页面呈现 `RuntimeUnavailable`——这是**真实的**「运行时未就绪」，不再是永久不可用。

### 5.3 五个端口各自的实现

| 端口 | 实现 | 为什么不放进 `src/platform/` |
| --- | --- | --- |
| `TranscriptCodec` | 直接转出 `src/platform/transcript.ts` 的 `encodeTranscript` / `encodeU16be` / `encodeUuid16` / `encodeUtf8` / `encodeNulJoinedUtf8` | 编码规则只有一份实现，包装只会引入第二处 |
| `DigestPort` | `crypto.subtle.digest("SHA-256", …)` | §9.4 的两级哈希要真 SHA-256 |
| `RandomPort` | `crypto.getRandomValues` / `crypto.randomUUID` | §6.1 要求 CSPRNG |
| `ClockPort` | `Date.now()` / `setTimeout` | 退避定时与快照时间戳只保留一个时间来源 |
| `HostIdentityPort` | 已配对主机记录落 `LocalCachePort` 的本节点条目 + `crypto.subtle` 的 ECDSA 验签（65 字节 SEC1 加 P-256 SPKI 前缀后 `importKey("spki")`） | §8.2 要求用**已配对的** Host key 验 `hostProof`；平台层无此端口，而 `src/platform/**` 不在本轮写入范围 |

`HostIdentityPort` 的**验签没有降级分支**：记录缺失、key 无法导入、签名解不开、验签抛错一律返回 `false`，由连接层进入 `identity_changed`。

### 5.4 `PairingTransport` 的 HTTPS 实现

- `readQrPayload`：JSON 解析 + 逐字段校验（协议版本、来源形状、过期时间）；形状不符**抛错**，不尽力解析。
- `claim`：`POST {canonicalOrigin}/sync/v1/pairing/claim`，body 含 `pairingId` / `hostId` / `deviceId` / `deviceName` / `clientKind` / `canonicalOrigin` / `devicePublicKey` / `clientNonce` / `proof`，其中 `proof = HMAC-SHA256(pairingSecret, pairing-proof transcript)`。凭据只走 body，不进 URL（§7.1）。
- `poll`：`POST {canonicalOrigin}/sync/v1/pairing/status`，`proof` 用 pairing-status domain，**每次轮询换新 `requestNonce`**；业务状态只由 body 的 `status` 表达（`expired`/`consumed` 同样是 HTTP 200），不用 `ok` 判定。
- 缺 `pairingRequestId`（未声明过）时明确失败，不发缺字段的请求。
- 错误码只取封闭词表内的码，不在词表内则按 HTTP 状态（§7.4）推导，不从服务端正文取新码。

### 5.5 前置：`SyncClient.connectionId`

选了**补公开只读 getter**（在已授权的 `src/sync-client/` 内）而不是其它绕法，理由是它同时暴露了另一个真问题。`#connection?.connectionId` 在 socket 被服务端关闭后**仍然有值**（连接对象要等重连才被替换），直接透传会让已断开的连接继续拿到标识，于是 `session.create` 能被派发到一条死连接上而服务端根本收不到；`connected` 同理会让组合根以为还在线并跳过重连。因此：

- `SyncConnection` 新增 `get closed()`；
- `SyncClient.connected` 改为「持有连接对象**且**该连接未关闭」；
- `SyncClient.connectionId` 在 `!connected` 时返回 `null`（§8.4：认证状态只属于当前连接，关闭后立即失效）。

顺带修正一处既有缺陷：`#onClose` 原本只在客户端主动 `close()` 时置 `#closed`，服务端关闭这条路径不置，于是连接对象在服务端关闭后仍被判活。

### 5.6 node 级接线测试：`src/composition.test.ts`（新增 10 条）

注入端口替身（内存 `LocalCachePort`、替身 `DeviceIdentityPort`、替身 `LifecyclePort`、`FakeSocketFactory`、假时钟、假 fetch），被测的仍是组合根真实的装配代码。

| 用例 | 断言 |
| --- | --- |
| 未配对时不连接 | 0 条 socket、状态机停在 `unpaired`、`hostPanel().host` 为 `null`（**不编造**主机身份） |
| 事件流到达 store | 快照经 `onEvent` 进 `store.state.resources`，目录页 `kind === "ready"` 且渲染出目录 |
| connectionId 只在认证后可用 | 认证后 `submitCreateSession` 返回 `ok: true` 且 `session.create` 已发出（scopes 显式含 `session.create`，使第一道闸门成为唯一变量）；断连后回到 `ok: false` |
| 连接进行中不安排第二条连接 | 推进 10× 退避窗口后仍只有 1 条 socket |
| pagehide 断开连接 | socket 被关闭 |
| 可重试的断开后按退避重连；阻断态下不再重连 | 假时钟推进前不重连、推进后重连、阻断后推进再久也不重连 |
| 二维码形状校验 | 非 JSON / 协议不符 / 缺字段 / 已过期各抛错 |
| claim 路径与 body | URL、方法、四个关键字段、凭据不进 URL |
| claim 失败映射 | HTTP 410 → `pairing.expired` |
| 状态轮询分派 | pending / approved / 终态 / 未知值四态各自分派，且请求带 `pairingRequestId` 与 `requestNonce` |

**判别力变异实测**：

| 变异 | 结果 |
| --- | --- |
| `ConnectionIdentity` 编造一个固定标识 | **1 failed**（connectionId 用例） |
| `onEvent` 不接 `store.apply` | **3 failed** |
| 不订阅生命周期 | **1 failed** |
| 去掉「当前是否已经在连」的状态判定 | **1 failed**（连接进行中不安排第二条连接） |
| 服务端关闭不再置 `#closed` | **1 failed**（connectionId 用例） |

第一次跑变异时发现两条变异**没变红**，据此改了两处：

1. `connectionId` 用例原本只断言「被拒」，而 scopes 不含 `session.create`，拒绝来自**第二道**闸门——换成「认证后必须成功 + 断连后必须被拒」，把第一道闸门隔离出来。
2. 重连闸门原本同时含 `shouldAutoReconnect` 与状态名白名单，去掉任一条都不红。实测发现状态名白名单才是有效闸门（`shouldAutoReconnect` 只区分阻断/未配对，答不出「此刻是否已经在连」），因此**删掉了冗余的 `shouldAutoReconnect` 调用**并补了「连接进行中不安排第二条连接」这条用例。

---

## 6. 未验证项（如实登记）

1. **浏览器级证据全部缺**：组合根的接线正确性只在 node 级（注入替身）验证。真实 IndexedDB、真实 WebCrypto 密钥生成与验签、真实 WebSocket、`expo export` 产物的实际运行均**未**在浏览器中核对。TP4 的 16 条浏览器断言属 `run-browser-check.mjs`（`src/platform/**`，本轮未改，仍 16/16），它们**不覆盖**组合根。
2. **`HostIdentityPort` 的真实验签未被任何用例执行**：node 级测试注入了替身（验签需要一对真实密钥与真实签名）。ECDSA 验签路径（SEC1 → SPKI → `importKey` → `verify`）目前只有类型检查覆盖。
3. **配对 HTTPS 面未经真实网络核对**：路径、方法、字段、HMAC transcript 的编码均经替身 fetch 断言，但没有真实 Daemon 可对拍；`pairing.qr` 的 SAS 校验与 `hostProof`（配对面，§7.2）**未实现**——`PairingTransport` 的端口签名里没有承载它们的位置，属接口形状问题，需独立变更。
4. **恢复点跨进程不持久**：`resumeCursor` 恒为 `null`，进程内由门面 `#ackedCursor` 承接。R14 的「从最后确认的位置连续恢复」在**重连**维度成立，**刷新页面**维度尚未覆盖（需要第二个持久化存储面，`LocalCachePort` 的 TTL/LRU 使它不适合承载恢复点）。
5. **已配对主机记录受本地缓存 TTL 约束**：`LocalCachePort` 的本节点条目有 30 天 TTL 与 LRU 配额，过期或被淘汰后 `loadPairedHost` 返回 `null`，连接按 §8.2 进入 `identity_changed` 要求重新配对。不是静默降级（验签绝不被跳过），但「已配对」的时长被一条缓存策略限定。彻底解法是给平台层加专用持久端口，属独立变更。
6. **组件渲染断言仍在 Node 静态渲染下成立**（无事件、无 `useEffect`），承自 r1；交互行为归 TP4。
7. **`deps` / `advisories` / `secrets` 三个 CI-only job 未执行**，本报告不声称它们通过。
8. **`cargo fmt --check` 未跑**：本包零 Rust 改动（承自 r1 的同一判断）。
9. **`expo export` 产物中有一处 `node:crypto`**：来自 `expo-constants` 的 `uuidv4` 兜底分支（`'undefined'==typeof window` 判定），非本包代码；本包代码零 node 内建引用。

---

## 7. 写入范围核对

`git diff --name-only 4ada2a2..HEAD` 共 44 个文件，全部落在授权范围内（脚本化核对：不在
`clients/app/{app,src/features,src/components,src/sync-client,src/composition*}` 之下的路径为空）：

| 类别 | 路径 |
| --- | --- |
| 标准范围 `clients/app/app/` | `_layout.tsx`、`dir/[alias].tsx`、`hosts.tsx`、`index.tsx`、`pair.tsx`、`session/[id].tsx` |
| 标准范围 `clients/app/src/components/` | 11 个组件 + `index.ts` + `testing/react-dom-server.d.ts` + 5 个 r1 用例文件 + 新增 `r18-degradation-pipeline.test.ts` |
| 标准范围 `clients/app/src/features/` | `client-store.ts`、`conversation-model.ts`、`create-session-model.ts`、`degradation-model.ts`、`directory-model.ts`、`host-panel-model.ts`、`index.ts`、`pairing-model.ts`、`ports.ts`、`runtime.tsx`、`testing/fixtures.ts`、r1 用例文件 |
| **例外 (a)** `clients/app/src/sync-client/` | `client.ts`、`connection.ts`、`snapshot-barrier.test.ts`（r1）+ 新增 `epoch-rebuild-barrier.test.ts` |
| **例外 (b)** `clients/app/src/composition.ts` | 新增，含 `composition.test.ts` |

未触碰：`src/protocol/**`、`src/domain/**`、`src/state/**`、`src/platform/**`、`scripts/**`、`package.json`、`vitest.config.ts`、`openspec/**`、`plan.md`、`design.md`、`docs/**`、`crates/**`、`schemas/**`、`fixtures/**`。无新增 npm 依赖，未新增 `.native.ts`，`run-browser-check.mjs` 未改动。

## 8. 未削弱既有测试

- 本轮**未删除、未改写**任何既有用例行；新增 21 条（5 + 6 + 10），`359 - 338 = 21`。
- r1 的 8 个测试文件全部仍绿，含 reviewer 确认真实有效的 R15/R16/R19 MUST NOT 负向用例与 MU3c 的 P1-N1/N2。
- `layers.test.ts` 未出现在本轮 diff 中，分层机检未被改写、未被绕过（9 条全绿）。
- 对照组齐备：每条修复都配「非一律采纳/非一律拒绝/非一律降级」的反向用例，避免平凡实现蒙混。