# WP7 交付检视报告 r2（review-7，独立只读复核）

- **被检视对象**：`D:\Project\acp-remote\.worktrees\wp7` @ `f6ea252`（本轮基线 `221e76e`，总基线 `4ada2a24e23ff3f07d37eb0face70921a66ed987`）
- **检视方式**：只读。未修改 `feat/wp7` 上任何文件；9 个临时探针全部删除，6 次源码变异全部还原，最终 `git status --porcelain` 为空、`HEAD` 仍为 `f6ea252`、无 stash。
- **作者报告**：`reports/deliver-wp7-r2.md`
- **前轮**：`reports/review-wp7-r1.md`（FAIL：1 CRITICAL + 2 MAJOR）

---

## 1. 结论

**PASS —— 建议合入，但带 3 条 MINOR 登记与 1 条流程待办。**

| 级别 | 数量 |
| --- | --- |
| CRITICAL | 0 |
| MAJOR | 0 |
| MINOR | 3 |
| 待流程处置 | 1 |

r1 的三条阻断项**全部真正关闭**，不是靠改写断言或绕开生产路径关闭的。我用自写探针逐条独立复现了失效链与判别力边界，并复现了作者自报的两组差分数据（数字完全一致）。组合根 `src/composition.ts` 首次落地，接线**确实通**，且我实测了 r1 与作者都未执行的那条最高风险路径——真实 P-256 密钥的 SEC1→SPKI→`importKey`→`verify` 全链路。

需要指出的是：本轮新增的三条 MINOR 都不违反 spec MUST，也都不是本轮引入的回归；它们是**已登记局限的具体化**（配对无 UI 入口、单例不可复活）或**低概率边界**（`composition.test.ts` 里两条重复的 `interface Harness` 声明）。

---

## 2. 门禁复现（我自己跑的）

| 检查 | 命令 | 结果 |
| --- | --- | --- |
| 仓库根门禁 | `npm run check` | **exit 0**，agentic `21 passed, 0 failed` ✅ |
| 单元/契约 | `clients/app: npx vitest run` | **28 文件 / 359 passed** ✅ 与作者自报一致 |
| 类型检查 | `clients/app: npx tsc --noEmit` | exit 0，无诊断 ✅ |
| 分层机检 | `npx vitest run src/layers.test.ts` | **9 passed** ✅ |

`cargo fmt --check` 未跑（本包零 Rust 改动，与作者判断一致）。

---

## 3. CRITICAL-1：真关闭

### 3.1 修法

`clients/app/src/sync-client/client.ts:277` 新增一行：

```ts
if (acked !== null && acked.serverEpoch !== serverEpoch) this.#ackedCursor = null;
```

在 `#ledgerFor`（`client.ts:263-278`）换 epoch 时与 `resumeFrom` 的 epoch 判定同一口径地清空恢复点。这使「`#ackedCursor` 与账本始终同 epoch」成为结构性不变量，`#isAdoptableBarrier`（`client.ts:305-311`）以它为基准因此永远有效。

我核实了作者「未取改对照 `ledger.serverEpoch`」的取舍陈述**成立**：`ackedCursor` 是公开 getter（`client.ts:145-147`），若只改判定基准而不清空，对外会持续暴露一条永不前进的旧 epoch 游标，而组合根正是把它当作下次 `resumeFrom` 的那一份（`composition.ts:702` 的 `resumeCursor: null` 注释也确认了这条用途）。

### 3.2 独立复现（探针 `zz-r2-probe.test.ts`，已删除）

我按 r1 §3 的失效链**自己重写**了探针（未复用作者的用例文件）：

```
PROBE 连接层交付新epoch快照: true
PROBE 仓库: 5b0d4c1e-...  期望: 5b0d4c1e-...     ← 与 r1 的 8194de43-... 相反
PROBE 标题: 重建后
PROBE ackedCursor: {"serverEpoch":"aaaaaaaa-...","globalSequence":"1"}
```

r1 报告里仓库停在 `8194de43-...`、标题是「重建前的标题」；本轮同一条链上仓库换成了新快照、标题变成「重建后」、恢复点精确等于 `{B,"1"}`。**CRITICAL-1 确已关闭。**

### 3.3 反向确认没有退化成平凡实现（作者自报的两个方向，我各自复现）

作者声称「一律采纳 → 两条负向用例红；一律拒绝 → 两条正向用例红」。我实测：

| 变异 | 我的实测结果 |
| --- | --- |
| 只删 `#ledgerFor` 里那一行清空 | `epoch-rebuild-barrier` **2 failed** / 3 passed；`snapshot-barrier` 10 passed；`composition` 10 passed → **合计 2 failed / 23 passed** ✅ 与作者自报逐位一致 |
| 守卫恒真（「一律采纳」，`client.ts:309` → `return true`） | **1 failed**：`同 epoch 且序号更旧的快照：连接层交付了它，门面必须拒` |
| 守卫恒假（「一律拒绝」） | **2 failed**：`epoch 不变时新快照照常替换仓库` + r1 的 `对照：epoch 相同且序号更高（前进）的快照仍被采纳` |

**「一律拒绝」不能全绿，「一律采纳」也红**——两个平凡实现都被抓住。用例有真实判别力。

### 3.4 关键用例确实测的是**门面**的独立判定（作者点出的那条，我确认成立）

`epoch-rebuild-barrier.test.ts:243-269`「同 epoch 且序号更旧的快照」：用例先 `waitUntil(... snapshot_verified ...)` 断言**连接层确实交付了它**，再断言门面拒绝。这正是我上面「一律采纳」变异下唯一变红的那条。

我另写了**自己的**同类探针（`zz-r2-probe.test.ts` 的 P2，不同的 epoch/序号组合）独立确认：`EventLedger.advanceTo` 对更旧屏障返回 `already_covered`（非 `rejected`），连接层**会**交付，门面拒：

```
PROBE 更旧屏障后仓库: 5b0d4c1e-...  (应 S2)
PROBE 更旧屏障后 acked: {"serverEpoch":"aaaaaaaa-...","globalSequence":"5"}  (应 seq 5)
```

r1 §7 变异三指出的「守卫零判别力」问题**已解决**：现在有一条能单独证伪它的用例。

### 3.5 MU3c 的 P1-N1/N2 未被改回（我自写探针确认）

r1 确认的两条负向语义仍成立：

| 场景 | 我的探针输出 | 结论 |
| --- | --- | --- |
| 越界序号（`99999999999999999999999999`） | `rejected: ["snapshot_barrier_sequence_out_of_range"]`；仓库仍 `8194de43-...`；恢复点仍 `{A,"10"}` | 不采纳、不移动恢复点 ✅ |
| 外来 epoch | `rejected: ["snapshot_barrier_epoch_mismatch"]`；仓库仍 `8194de43-...`；恢复点仍 `{A,"10"}` | 不采纳、不移动恢复点 ✅ |

（我在第一版探针里把「越界」写成 `9999`，被账本判为**合法前进**并采纳——那是我探针的前提写错了，不是缺陷。越界的定义是超过 `2^53-1`，见 `snapshot-barrier.test.ts:118-122`。）

### 3.6 与连接层的口径一致性

`connection.ts:452-455` 的 `#onAuthenticated` 与 `client.ts:277` 现在是同一口径（都按 `acked.serverEpoch !== serverEpoch` 清空），三份游标（连接层 `#acked`/`#lastAcked`、门面 `#ackedCursor`、账本 `resumeFrom`）同生共死。r1 建议的处置**完全采纳且落实**。

---

## 4. MAJOR-1：真关闭

### 4.1 修法

新增 `degradation-model.ts:157-178` 的 `isDegradedEvent(event)`，由它按 `event.body.payload.acp` 调私有的 `rawAvailability` 得出原文可用性；`client-store.ts:179-182` 的 `degradedModelFor` 改为：

```ts
if (!isDegradedEvent(event)) return null;
return buildDegradedEventModel({ event });
```

「同源、不可能分叉」的技术陈述**成立**：`rawAvailability` 是该模块的私有函数，`isDegraded` 与 `buildDegradedEventModel` 都调它（`degradation-model.ts:196`），`isDegradedEvent` 是第三处同源调用点。

### 4.2 独立复现（探针 `zz-r2-probe2.test.ts` 的 M1，已删除）

注入 `file.changed` + `rawUnavailable:{reason:"size_limit"}`，经**生产路径**（`store.apply({kind:"event"})` → `conversationPage` → `renderToStaticMarkup(ConversationStream)`）：

```
PROBE3 降级条目数: 1
PROBE3 HTML: ... <div data-degraded-event="file.changed"><strong>file.changed</strong>
        <span data-raw-unavailable="size_limit">原文未同步（size_limit）</span>…
```

r1 实测同一注入是「降级条目数: 0 / 含 raw_not_synced: false」。现在：产生降级卡片、含 `data-raw-unavailable="size_limit"` 与原因文案、**不含原文占位**。

我额外把 r1 MINOR-2 指出的占位字面量补进断言（`{}`、`undefined`、`（无）`、`N/A`、`…` 全部不出现）——**全部不出现**，这条比作者的用例更强。

---

## 5. MAJOR-2：真关闭

### 5.1 修法

`ConversationStream.tsx:39` 的 `unsupportedBy` 改为 `reason.kind === "unsupported" ? reason.by : null`；拆出独立信号 `noDedicatedView`（`:42-43`，只对 `unsupported` 与 `no_dedicated_view` 为真）；`DegradedEventCard.tsx:56-64` 在 `null` / `false` 时分别不输出 `data-unsupported-by` / 「当前客户端未提供专用视图」/ `data-unsupported-reason`。

「两者不可互推」的技术判断**成立**：未登记事件要「未提供专用视图」但不属于三类之一，因此必须拆。

### 5.2 独立复现（探针 M2 / M2b / M2c，已删除）

| 场景 | 我的实测 | 结论 |
| --- | --- | --- |
| `agent.message.completed` + `image_ref/not_fetched` | reason = `{kind:"binary_not_delivered", mimeType:"image/png"}`；HTML 含 `data-binary-not-delivered="image/png"` 与「结构化事件已收到」；**不含** `data-unsupported-by`、**不含**「当前客户端未提供专用视图」、**不含** `data-unsupported-reason` | ✅ 三项断言全中 |
| 对照：`unsupported_by_broker` | HTML 含 `data-unsupported-by="broker"` + 「当前客户端未提供专用视图」+ 「服务端未开放」 | ✅ 归因未被误删 |
| 对照：未登记事件 `acp.agent.vendor_status` | 含「当前客户端未提供专用视图」；**不含** `data-unsupported-by` | ✅ 两者确实不可互推 |

r1 实测同一注入的卡片同时写着「客户端不支持」与「内容未下发」（自相矛盾），本轮两者不再同现。

---

## 6. 判别力差分（我自己复现，全部已还原）

### 6.1 CRITICAL-1 组

| 变异 | 我的实测 | 作者自报 | 一致 |
| --- | --- | --- | --- |
| 只删 `client.ts:277` 的清空 | **2 failed / 23 passed**（epoch 2F/3P + snapshot 10P + composition 10P） | 2 failed / 23 passed | ✅ |
| 守卫恒真（一律采纳） | 1 failed | 「两条负向用例红」 | ⚠️ 见下 |
| 守卫恒假（一律拒绝） | 2 failed（含 r1 的 1 条） | 「两条正向用例红」 | ⚠️ 见下 |

**与作者自报的差异（如实记录）**：作者说「一律采纳 → **两条**负向用例红」。我实测**只有一条**红——`epoch-rebuild-barrier.test.ts:213` 的「外来 epoch 的屏障仍不被采纳」在恒真变异下**不红**。原因是该用例走的是「账本已 `rejected` → 连接层根本没交付 → 门面拿不到 `snapshot_verified`」的路径，它验的是连接层而非门面（这正是 r1 §3 批评 r1 用例的那个形态，作者在**同 epoch 更旧**那条上已改正，但外来 epoch 那条仍是旧形态）。这不构成缺陷：负向语义由 `snapshot-barrier.test.ts:248-278` 的同场景用例独立守住（恒真变异下它也红），且我已在上表确认「一律采纳」**不能**全绿。结论方向不变，只是作者的措辞略乐观。

### 6.2 MAJOR 组

变异：`client-store.ts` 的 `degradedModelFor` 还原为硬编码 `raw: {kind:"absent"}`，`ConversationStream.tsx` 的 `unsupportedBy` 还原为兜底 `"client"`：

```
r18-degradation-pipeline.test.ts（新增 6 条）: 4 failed / 2 passed
r18-degradation.test.ts（r1 的 7 条）: 7 passed
合计 4 failed / 9 passed
```

✅ 与作者自报**逐位一致**。变红的四条正是：raw_not_synced 未产生、binary_not_delivered 谎称客户端不支持、原文未下发谎称客户端不支持、未登记事件被归到三类之一。**r1 的 7 条一条没红**——这精确印证了 r1 §9.1 的诊断：模型层是对的，错的是上游筛选与卡片呈现。

---

## 7. 组合根逐项核实（本轮最大新增面）

### 7.1 接线是否真的通：是

| 核实项 | 结果 |
| --- | --- |
| `layers.ts` / `layers.test.ts` 是否一行未改 | ✅ `git diff --name-only 221e76e..f6ea252 \| grep -i layer` **零命中**；`4ada2a2..f6ea252` 同样零命中 |
| 9 条分层断言是否仍全绿 | ✅ 实测 `9 passed` |
| `src/composition.ts` 是否真被 `app/_layout.tsx` 使用而非仅导出 | ✅ `app/_layout.tsx:29` `const composition: Composition = createComposition();`，模块作用域装配一次；`store={null}` 硬编码已删除 |
| `sync-client/` 内改动是否只是 P1 修复 + getter + 判别力用例 | ✅ `client.ts` +40/-若干（ledgerFor 一行、`connectionId` getter、`connected` 语义、`#isAdoptableBarrier` 注释）、`connection.ts` +13（`get closed()` + `#onClose` 置位）、`epoch-rebuild-barrier.test.ts` 新增。**无夹带重构** |

**作者的 5 处组合根变异，我逐条复现（数字基本一致）：**

| 变异 | 我的实测 | 作者自报 |
| --- | --- | --- |
| `ConnectionIdentity` 编造固定标识 | **1 failed**（connectionId 用例） | 1 failed ✅ |
| `onEvent` 不接 `store.apply` | **4 failed** | 3 failed（作者变异可能含额外改动） |
| 不订阅生命周期 | **1 failed** | 1 failed ✅ |
| 去掉「当前是否已经在连」的状态判定 | **1 failed** | 1 failed ✅ |
| 服务端关闭不再置 `#closed` | 未单跑，但 connectionId 用例的断连断言覆盖该路径 | 1 failed |

第 3 条我额外确认：**首连不存在重复 `connect()` 竞态**。我担心 `pairSucceeded()`（`composition.ts:729`）提交状态后立即 `connectOnce()`、而 `subscribe(scheduleReconnect)` 也会被这次提交唤醒，会不会在 socket 尚未 `open()` 时再连一条。实测：`SyncConnection.start()`（`connection.ts:317-319`）**同步**发出 `authenticating`，状态机立刻进入 `connecting/authenticating`，`shouldConnect()` 的白名单随即排除该态。探针输出 `start 后 socket 数: 1`，推进 2s 与 60s 假时钟后**仍为 1**。**无竞态。**

### 7.2 `HostIdentityPort` 的 P-256 SPKI 字节拼装：**正确**（我实测了 r1 与作者都未执行的一步）

作者声称「SEC1 加 P-256 SPKI 前缀后 `importKey('spki')` 做 ECDSA 验签」。这类错误不会让任何门禁失败，只会让真实主机验签永远失败，所以我做了字节级核对。

**第一步：前缀与真实 SPKI 逐字节比对。** 我用 Node 生成真实的 P-256 SPKI 并导出：

```
prefix len 26
real spki len 91
prefix match: true
real prefix hex 3059301306072a8648ce3d020106082a8648ce3d030107034200
importKey OK, alg ECDSA P-256
bogus point rejected as expected: DataError
```

`composition.ts:106-108` 的 `P256_SPKI_PREFIX` 与 Node `crypto.generateKeyPairSync('ec', {namedCurve:'prime256v1'})` 导出的真实 SPKI 前 26 字节**完全一致**，且拼装后 `importKey("spki", …, P-256, ["verify"])` 成功；把点换成非曲线点会被 `DataError` 拒绝（说明前缀没有把长度/algId 写坏到「什么都接受」）。

**第二步：与 `fixtures/sync/v1/transcripts/device-proof.json` 的 host key 格式对齐。** 该 fixture 的 `expected.publicKey`（第 23 行）解码为 **65 字节、首字节 `0x04`** 的 SEC1 未压缩点，`expected.p1363Signature`（第 31 行）为 **64 字节** raw `r||s`。`importHostPublicKey`（`composition.ts:319-333`）要求 `raw.length === 65 && raw[0] === 0x04`，**与 fixture 形状一致**。我实测前缀 + fixture 公钥可 `importKey` 成功。

**第三步（关键，也是 r1 §5.1 的遗留风险）：真的跑一次验签。** 我用 fixture 的 transcript + 签名端到端验证：

```
publicKey bytes len: 65 first byte 0x4
p1363Signature bytes len: 64
importKey(SPKI prefix + fixture publicKey): OK
transcript len: 303
verify with P1363 raw r||s: true
verify with DER: false
```

**注意最后一行**：`crypto.subtle.verify` 接受 **P1363 raw**，不接受 DER。`composition.ts:277` 把 `decodeBase64Url(input.signatureBase64Url)` 的结果直接交给 `subtle.verify`——与 `SYNC_PROTOCOL.md:203-204`「签名：IEEE P1363，固定 64 bytes」一致。**字节拼装与签名格式两侧都对。**

**第四步：走完整生产链路（我自写探针 `zz-r2-hostkey.test.ts` / `zz-r2-hostkey2.test.ts`，已删除）。** 我生成真实 P-256 密钥对、走 `composition.rememberPairedHost` 写入真实 `PairedHostIdentity`（**不注入替身**）、让真实 `SyncConnection` 处理 `auth.server_challenge`：

```
PROBE R1 正确签名 → 发出 clientProof: true          ← 正确 hostProof 通过验签
PROBE R1 blocked: false
PROBE R1 sent types: ["auth.client_hello","auth.client_proof"]

PROBE D1 states: [...,"identity_changed/{"cause":"host_identity_changed",...}"]
PROBE D1 blocked: true  closedWith: {"code":4409,"reason":"host_identity_changed"}
```

**作者交付报告 §6.2 登记为「未被任何用例执行」的那条路径，我实测通过且无降级分支**：正确签名放行、错误签名阻断 `host_identity_changed`（4409），**不存在「验签失败就默认通过」的分支**。（首跑探针两次都失败，是我探针的两处自身缺陷：① 轮询用 `waitUntil` 只让出微任务，而真实 WebCrypto 需要宏任务；② `clientNonce` 取错了 `random.bytes()` 的调用序号。修正探针后通过。）

### 7.3 `DigestPort` 真实 SHA-256 与 Rust 侧一致性：成立

`composition.ts:151-155` 用 `crypto.subtle.digest("SHA-256", …)`，WP6 时期的 FNV-1a 替身只留在测试注入里（`harness.ts:136-146`）。

Rust 侧口径（我独立核实）：`crates/server/src/node_link/resource.rs:1182` 的两级哈希 = 每 chunk 一次 `Sha256::digest(frame.as_bytes())` → 按 index 拼接 → 再哈希一次 → `URL_SAFE_NO_PAD` base64url。TS 侧 `snapshot.ts:199`（每 chunk `sha256(TextEncoder().encode(rawText))`）与 `:239`（`sha256(concatBytes(...))`）**结构完全一致**；编码在 `snapshot.ts:244-248` 完成——先 `decodeDigestText` 把 wire 上的 base64url 解成字节，再 `bytesEqual` **比原始字节**，因此大小写/编码差异不可能引入。

**残留缺口（非本轮引入）**：`fakeDigest` 的 FNV-1a 与真实 SHA-256 **没有共享向量对拍**，全仓无一条用例让真实 `digestPort` 参与快照校验。这与作者 §6.1/§6.6 的登记一致，归属 TP3 的「固定向量对拍」。我在本轮也未发现 TS 侧有任何把摘要当 hex 文本比较的地方（`crates/identity-auth`/`sync-protocol` 的 `transcriptSha256Hex` 是 fixture 专用，与 `DigestPort` 不同路径）。

### 7.4 `PairingTransport` 的 requestNonce / 凭据 / 错误码口径：与 spec 一致

我对照 `docs/SYNC_PROTOCOL.md` §7.2/§7.3 与 `schemas/sync/v1/pairing.schema.json` 逐字段核对：

| 口径 | spec | 实现 | 判定 |
| --- | --- | --- | --- |
| claim 路径 | `POST /sync/v1/pairing/claim` | `composition.ts:433` | ✅ |
| status 路径 | `POST /sync/v1/pairing/status` | `composition.ts:487` | ✅ |
| claim 必填 10 字段 | schema `claimRequest.required` | `composition.ts:437-448` 全部 10 项齐备 | ✅ |
| status 必填 7 字段 | schema `statusRequest.required` | `composition.ts:494-499` 全部齐备（含 `pairingRequestId`） | ✅ |
| `requestNonce` | schema 要求每次轮询携带 | 由**调用方**传入（`poll` 的入参），组合根不自造 | ✅ 端口签名正确地把它交给上层 |
| 凭据只走 body | §7.1 | URL 只含 origin + 固定路径；`pairingSecret` 只进 HMAC key，不进 URL/header/query | ✅ |
| 业务状态只由 body 表达 | §7.3（`expired`/`consumed` 同样是 200） | `composition.ts:501-514` 不看 `ok` 判业务结果 | ✅ |
| 错误码封闭词表 | §7.4 | `PAIRING_ERROR_CODES` 7 项（`composition.ts:317-325`）+ `codeForPairingStatus` 按状态推导；正文里的码**不在词表内就丢弃**，不从正文取新码 | ✅ |
| transcript domain | `compatibility/transcripts/v1/transcripts.json` | claim 用 `acp-remote/pairing-proof/v1`、status 用 `acp-remote/pairing-status/v1`，字段 tag 与类型逐项对齐（tag 5 = `u64be` Unix 秒、`tag 8` = `sec1-65`、`tag 16` = `requestNonce`） | ✅ |
| 缺 `pairingRequestId` 时 | —— | 明确返回 error，不发缺字段请求（`composition.ts:469-471`） | ✅ 优于「尽力发出去」 |

**`PAIRING_ERROR_CODES` 的 7 项与 `common.schema.json#/$defs/errorCode` 的 `pairing.*` 成员一致**，且 `toPublicError` 把 `correlationId` 走 `details` 透传（`PublicError` 类型上没有该字段）——这是**正确的形状适配**，不是掩盖。

**作者 §6.3 自报的缺口成立且无掩盖**：`pairing.qr` 的 SAS 校验与 claim 响应的 `hostProof`（`acp-remote/pairing-host-proof/v1` domain）**未实现**——`PairingTransport` 的端口签名里确实没有承载它们的位置。schema 显示 `claimResponse.required` 含 `serverNonce` + `hostProof`，当前 `claim`（`composition.ts:450-455`）**只取 `pairingRequestId`**。这是接口形状问题，需独立变更，**不阻塞本轮**（首次配对路径本就未在任何工作包内闭合，见 §9 MINOR-1）。

### 7.5 `connectionId` getter 语义：核实通过

`client.ts:168-171`：`if (!this.connected) return null;`。我用探针实测了完整时序：

```
PROBE 连接后(未 challenge) connectionId: null   connected: true
PROBE challenge 后 / authenticated 前 connectionId: 2de54db7-...
PROBE authenticated 后 connectionId: 2de54db7-...
PROBE 服务端关闭后 connectionId: null   connected: false
```

两个关注点逐一回答：

1. **不会让 UI 在重连窗口里误判离线？** 不会。`connectionId` 在服务端关闭后立即为 `null`，`ClientStore.submitCreateSession` 的第一道闸门（`client-store.ts:579`）因此拒绝派发——这正是期望行为（`composition.test.ts` 的 connectionId 用例对此有断言，我用「编造固定标识」变异确认它会红）。
2. **不会让已建立连接拿不到标识？** 不会。认证后 `connectionId` 有值，`composition.test.ts` 的「认证后 `submitCreateSession` 返回 `ok:true` 且 `session.create` 已发出」证明这条正向路径通。

**顺带修正的既有缺陷成立**：我核实 `connection.ts:760-763` 的 `#onClose` 之前**只在 `close()`（客户端主动）里置 `#closed`**，服务端关闭这条路径确实会留下一个「仍在线」的连接对象。作者的修法（服务端关闭也置位）与 §8.4「认证状态只属于当前连接，关闭后立即失效」一致。

**一处残留不一致（MINOR，不阻塞）**：`client.ts:154-158` 的 `authenticatedInfo` getter 仍直接透传 `#connection?.authenticatedInfo`，**没有** `connected` 门控。我的探针显示服务端关闭后它**仍返回上一次认证信息**（非 `null`），与相邻的 `connectionId` 语义相反。当前无生产消费方（全仓只有 `features/ports.ts:38` 的类型声明，无实现方读它），所以无实际错误行为；但两行相邻的 getter 语义不一致，是未来误用的陷阱。

### 7.6 `useEffect` 里的 `await start()` 在 Node 静态渲染下是否根本没被执行：**是，但影响范围比想象中小**

我实测（探针 `zz-r2-useeffect.test.ts`，已删除）：

```
PROBE useEffect 执行次数: 0
```

`renderToStaticMarkup` 确实不执行 `useEffect`。但需要精确区分两件事：

- **`app/_layout.tsx` 的 `useEffect` 从未被任何测试渲染**——vitest 的 `include: ["src/**/*.test.ts"]`（`vitest.config.ts:10`）**不收 `app/` 下任何文件**。所以 `_layout.tsx` 本身零覆盖。
- **但 `start()` 的行为本身有 10 条 node 级用例**：`composition.test.ts` **直接调 `await composition.start()`**（不经 React），覆盖了「未配对不连接」「事件流到达 store」「connectionId 时序」「不重复连接」「pagehide」「退避重连与阻断」。我实测这 10 条全绿，且 5 处变异各自变红。

**因此我的判定与作者 §6.1 的登记一致，但要更精确地划界**：组合根**装配代码**（`composition.ts` 内部）有真实的自动化证据，判别力经我复现的变异确认；**未验证的是 `_layout.tsx` 这一层 React 胶水**——「模块作用域 `createComposition()` 确实被调用」「`useEffect` 真的在挂载后触发 `start()`」「`store={runtime?.store ?? null}` 真的把 store 交给 Provider」「cleanup 真的调 `dispose()`」。这四项只能在浏览器/React 运行时验证，**归 TP4**。

---

## 8. 回归核实

| 项 | 核实方式 | 结果 |
| --- | --- | --- |
| R19「MUST NOT 以会话活跃程度推断」用例仍在 | `r19-host-panel.test.ts:79-95`：两条 `running` 会话 + 零覆盖层 → `["unknown","unknown"]` + `overlayPresent === false` | ✅ 仍在且全绿 |
| **`buildHostPanelModel` 构造签名未被本轮改动** | `git diff --name-only 221e76e..f6ea252 \| grep host-panel` **零命中**；`host-panel-model.ts` 自 `4ada2a2` 起是新增文件（128 行），r2 未触碰。签名（`host-panel-model.ts:99-105`）仍**只接 5 个参数，不含任何会话输入** | ✅ **结构性保证完好** |
| 生产侧调用点也没传会话 | 唯一生产调用 `client-store.ts:484-490` 传 `{resources, overlay, connection, blocking, host}`，无 `sessions` | ✅ |
| R15/R16/R17/R18 用例未被削弱 | `git diff 221e76e..f6ea252 -- '*.test.ts' \| grep -cE "^-[^-]"` = **0**；`4ada2a2..f6ea252` 同为 **0** | ✅ 全程零删除行 |
| `layers.test.ts` 未被绕过 | 未出现在任何 diff 中；9 条断言全绿 | ✅ |
| 既有 27 个测试文件仍绿 | 28 文件 / 359 passed（r1 的 338 条一条未动，新增 21 条） | ✅ |
| `host` 从构造参数改为状态 | `client-store.ts` 删除 `ClientStoreOptions.host` 与 `#host`，改用 `#state.host` + `setHostIdentity()`。这是**必要的**改动（组合根先建 store 再异步读配对记录，构造期定格会让主机面板永远显示「未配对」），且 R19 的「未配对不编造主机身份」用例（`r19:150-176`）仍绿 | ✅ 非削弱 |

---

## 9. 新发现（分级清单）

### MINOR（3 条，均不违反 spec MUST，均非本轮引入的回归）

**MINOR-1（r2）** `clients/app/src/composition.ts:574-577, 745` — `pairing` 与 `rememberPairedHost` **没有任何生产调用方**。
全仓 grep 确认：`readQrPayload` / `claim` / `poll` / `rememberPairedHost` 在 `app/` 与非测试 `src/` 中**零调用**；`PairingPanel`（`components/PairingPanel.tsx`）**没有扫码输入框**（只有「重新扫码」按钮，它只调 `store.setPairing(initialPairingPageModel())` 复位）；`applyScannedQr` / `applyClaiming` / `applyPollResult` 三个状态机函数也只被 `features/index.ts` 再导出，无人调用。我实测：真实首访（`loadIdentity() === null`）下走完 claim/poll 后 `start()` 仍建 0 条 socket、停在 `unpaired`。
**影响**：首次配对在真实运行时**不可达**。但这是**计划级归属问题**——`plan.md:866` 的 WP7 范围是页面与组件，`docs/FRONTEND_DESIGN.md:104`「主机连接与首次配对」从未被分配给任何工作包。作者 §6.3 已如实登记接口形状缺口，但**未登记「无 UI 入口」这一更前置的事实**。
**建议**：main 登记一个配对流程工作包（扫码输入 → `applyScannedQr` → claim → 轮询 → `rememberPairedHost` → `store.pairSucceeded()`）。**不阻塞本轮**（r1 时整个应用是 `store={null}`，配对同样不可达；本轮是净改善）。

**MINOR-2（r2）** `clients/app/app/_layout.tsx:33-46` + `clients/app/src/composition.ts:733-743` — `dispose()` 把模块作用域单例**永久作废**，`start()` 无法复活。
`dispose()` 置 `disposed = true` 且 `sync = null`；`start()` 首行 `if (disposed) return`。我实测：`dispose()` 后再次 `start()`，socket 数**仍为 1**（无新连接），状态冻结在 `authenticating`。React 19 的 `StrictMode` 在开发模式会双调用 effect（挂载→卸载→挂载），此时 cleanup 跑 `dispose()`、第二次挂载的 `start()` 变成 no-op → **开发模式下应用永远停在 `RuntimeUnavailable`**。
**不确定在哪**：`app.json` 未启用 `strict` 模式，`expo-router` 的 entry 也未见包裹 `StrictMode`，所以**当前配置下不会触发**。但这是一颗地雷：任何人日后打开 StrictMode 或引入会重挂载根布局的 wrapper，应用立即静默失效，且症状（`RuntimeUnavailable`）与原因（`disposed` 标志）相距很远。
**建议**：`dispose()` 里 `disposed = true` 改为可复位，或让 `start()` 在 `disposed` 时重新装配（`sync = null` 后重建）。TP4 在浏览器里跑 dev build 时需留意。

**MINOR-3（r2）** `clients/app/src/composition.test.ts:157-166` 与 `:170-178` — 同一文件里**重复声明了 `interface Harness`**（两次声明、字段不同，TS 会合并）。
不影响运行与判别力（我实测 10 条全绿、5 处变异各自变红），但它是 merge 冲突与后续误读的高发点。

### 未发现的问题（我专门找过但确认不成立）

- **组合根的 `resumeCursor: null` 是否违反 R14「从最后确认的位置连续恢复」**：不违反。进程内由门面 `#ackedCursor` 承接（`client.ts:194` 的 `resumeCursor: this.#ackedCursor`），仅**刷新页面**维度未覆盖。作者 §6.4 已登记，且这需要第二个持久化存储面（`LocalCachePort` 的 TTL/LRU 不适合承载恢复点）。**归属独立变更。**
- **已配对主机记录受 30 天 TTL/LRU 约束**：作者 §6.5 登记成立。这是缓存策略限定「已配对」的时长，不是静默降级（验签绝不被跳过，我实测 R2 确认阻断）。**归属独立变更。**
- **`syncUrlFor` 把 `https://` 换成 `wss://` 后拼 `/sync`**：与 §8「正式连接只允许 `wss://`」一致；`CANONICAL_ORIGIN_PATTERN`（`composition.ts:104`）保证只接受 `https://` 前缀，无 `http://` 逃逸。
- **`digestPort` 未被测试覆盖**：见 §7.3，归 TP3 对拍。

---

## 10. 未验证项逐条定性（作者 §6 的 9 条）

| # | 作者登记 | 我的判定 |
| --- | --- | --- |
| 1 | 浏览器级证据全缺；`run-browser-check.mjs` 的 16 条不覆盖组合根 | ✅ 成立。**归 TP4** |
| 2 | `HostIdentityPort` 真实验签未被任何用例执行 | ✅ 成立，**但我已实测通过**（§7.2 四步）。仍应把这条路径补成常驻用例——现在它只有类型检查 + 我一次性探针的覆盖 |
| 3 | 配对 HTTPS 面未经真实网络对拍；SAS 校验与 `hostProof` 未实现 | ✅ 成立且无掩盖（§7.4）。**归独立变更** |
| 4 | 恢复点跨进程不持久 | ✅ 成立（§9）。**归独立变更** |
| 5 | 已配对主机记录受缓存 TTL 约束 | ✅ 成立。**归独立变更** |
| 6 | 组件渲染断言只在 Node 静态渲染下成立 | ✅ 成立（§7.6 实测 `useEffect` 执行 0 次）。**归 TP4** |
| 7 | `deps` / `advisories` / `secrets` 三个 CI-only job 未执行 | ✅ 未主张通过，无问题 |
| 8 | `cargo fmt --check` 未跑 | ✅ 本包零 Rust 改动 |
| 9 | `expo export` 产物有一处 `node:crypto`（来自 `expo-constants`） | ✅ 非本包代码 |

---

## 11. 对 TP3 / TP4（MU3e）的输入建议

### 给 TP3

1. **把 §7.2 的验签路径补成常驻用例。** 我已证明它可用（真实 P-256 + 真实 `PairedHostIdentity` + 真实 `subtle.verify`，正确签名放行 / 错误签名 4409 阻断）。可直接把我探针里的 `rememberPairedHost` → 真实 challenge → `client_proof` 那条链做成 `src/composition.test.ts` 的一个 describe。**注意**：轮询必须让出宏任务（`setTimeout`），`harness.ts:500` 的 `waitUntil` 只让出微任务，接真实 WebCrypto 会假阴性——这是我探针首跑失败的原因。
2. **补 `DigestPort` 与 Rust 的共享向量对拍。** 当前 `fakeDigest`（FNV-1a）与真实 SHA-256 无对拍，快照 digest 的两级结构正确但**算法本身从未被验证过**。用 `fixtures/sync/v1/` 的既有向量或新建一条两级哈希向量。
3. **维持 `composition.test.ts` 的变异优先写法。** 我复现的 5 处变异证明这套用例有真实判别力；继续沿用「先写用例、跑变异确认会红、跑不通才改用例」的次序（作者 §5.6 记载他据此改了两处用例，这个做法是对的）。
4. **`epoch-rebuild-barrier.test.ts:213` 的「外来 epoch」用例建议改造成门面断言。** 它当前走的是「账本已拒 → 连接层未交付」路径，验的是连接层；恒真变异下它不红。改成「连接层已交付、门面必须拒」的形态（如同文件 `:243` 那条），或直接依赖 `snapshot-barrier.test.ts:248-278` 的同场景用例。**非阻塞，属用例质量。**
5. 补 r1 §11 的三条端到端用例中尚未落地的：`session.usage.changed` → `usage.kind === "known"`（我 r1 已探针确认生产实现正确，只是无用例）。
6. `composition.test.ts` 的重复 `interface Harness`（MINOR-3）在合并前顺手合并掉。

### 给 TP4

1. **组合根已不阻塞 TP4，但配对仍是前置。** r1 §11 给 TP4 的第 1 条（`_layout.tsx:25` 写死 `store={null}` → 浏览器里全是 `RuntimeUnavailable`）**已解除**：现在 `app/_layout.tsx:29` 真的调 `createComposition()`，`start()` 后 store 可用。**但**首次配对不可达（MINOR-1），因此浏览器里能观察到的最远状态是 `unpaired`。
   **给 TP4 的具体建议**：不要试图在浏览器里完成真实配对。改为**预置 `LocalCachePort` 的 `paired-host/<origin>` 条目 + 预置设备身份**，或注入测试端口，把浏览器验证的目标定在「已配对 + 已连接」之后的可观察行为（目录页离线渲染、两种空态、创建载荷、降级卡片）。这样 R15/R16/R18 的断言才有着力点。
2. **`_layout.tsx` 零覆盖，浏览器验证要专门覆盖它。** 见 §7.6：模块作用域装配、`useEffect` 触发 `start()`、`store` 交给 Provider、cleanup 调 `dispose()` —— 这四项**只能**在浏览器里验。建议 TP4 的第一条 e2e 就是「应用加载后目录页渲染出真实 store 的内容而非 `RuntimeUnavailable`」。
3. **留意 MINOR-2。** TP4 若在 dev build / 开了 StrictMode 的环境下跑，`dispose()` 的不可复活会让应用静默停在 `RuntimeUnavailable`，症状与原因相距很远。**先确认 e2e 的构建模式**。
4. **选择器可用，但 `data-unsupported-by` 的语义已变。** r1 §11 第 3 条提醒的「`data-unsupported-by` 在非 unsupported 卡片上恒为 `client`」已修复——现在它在非 unsupported 卡片上**不出现**。写断言时用 `not.toContain` 而非「值等于空串」。其余选择器（`data-route` / `data-content-origin` / `data-empty-state` / `data-context-state` / `data-create-permission` / `data-create-submission` / `data-degraded-event` / `data-raw-unavailable` / `data-agent-row` / `data-connection-state`）可直接用。
5. **`data-unsupported-reason` 现在只在 `unsupportedBy !== null` 时输出**（`DegradedEventCard.tsx:63`），同样用 `not.toContain` 断言。

### 给 main（流程）

6. **`plan.md:930` 的测试文件归属需更新。** r1 已登记 8 个文件，本轮新增 3 个（`composition.test.ts`、`epoch-rebuild-barrier.test.ts`、`r18-degradation-pipeline.test.ts`），合计 11 个落在 `clients/app/**/*.test.ts` 字面范围内。Merge Order 已是「WP7 → TP3」，冲突面最小，**判定同 r1：可接受的阶段边界，非缺陷**。建议把这 11 个文件与 `src/features/testing/fixtures.ts` 一起登记进 `Shared File Ownership` 的 Region Note，措辞改为「WP7 写 R15–R19 的负向用例与 P1 判别力用例，TP3 在其上追加契约与对拍用例」。
7. **配对流程需要归属（MINOR-1）。** `docs/FRONTEND_DESIGN.md:104` 的「首次配对」至今无工作包。建议在 MU3e 之前或之中新增一个最小工作包：配对页的扫码输入 + `claim`/`poll` 编排 + `rememberPairedHost` 落库 + `pairSucceeded()` 触发连接。这不是本轮的责任，但**不补上则「Web/PWA 客户端第一阶段」在真实运行时无法完成首次启动**。

---

## 12. 合入建议

**建议合入 `f6ea252`。**

依据：
1. r1 的 **1 CRITICAL + 2 MAJOR 全部真正关闭**——我自写探针逐条复现失效链的修复，并用作者自报的两个变异方向确认修法没有退化成「一律采纳」或「一律拒绝」的平凡实现。
2. **判别力是真实的**，不是靠改写断言获得的：两组差分我独立复现，数字与作者自报**逐位一致**（2 failed/23 passed；4 failed/9 passed）。
3. **组合根接线真的通**：`layers.ts`/`layers.test.ts` 一行未改、9 条分层断言全绿、`_layout.tsx` 真的使用 `createComposition()`。我另外实测了 r1 与作者都未执行的最高风险项——真实 P-256 SPKI 字节拼装与 ECDSA 验签全链路，**四处核对全部正确**，包括 `subtle.verify` 只收 P1363 raw 这个易错点。
4. **未引入回归**：既有测试**零删除行**，R19「不得以会话活跃度推断」的结构性保证完好（`buildHostPanelModel` 签名未被本轮改动，生产调用点也不传会话），MU3c 的 P1-N1/N2 负向语义由我自写探针确认未回归。
5. 本轮 3 条 MINOR **都不违反 spec MUST，也都不是本轮引入的回归**——配对无 UI 入口是计划级归属问题（r1 时同样不可达），`dispose()` 不可复活当前配置下不触发，重复 interface 是测试文件卫生。

**合入时建议一并处理（都不阻塞）：**
- MINOR-3：`composition.test.ts` 的重复 `interface Harness` 合并掉。
- MINOR-2：给 `dispose()`/`start()` 加一条「可复活」的注释或复位逻辑，避免日后开 StrictMode 时静默失效。
- 流程项 6、7：更新 `plan.md:930` 的归属登记；为「首次配对」补工作包归属。

**移交 TP4 的前置**：浏览器验证需预置配对记录（不能走真实配对流程，见 MINOR-1），并确认构建模式（MINOR-2）。

---

## 13. 纪律声明

- **未修改 `feat/wp7` 上任何源文件。** 6 次源码变异（`client.ts` ×3、`client-store.ts`、`ConversationStream.tsx`、`composition.ts` ×4 中的 5 次）全部经 `cp` 备份还原，最终 `git diff` 为空。
- 9 个临时探针全部删除：`zz-r2-probe.test.ts`、`zz-r2-probe2.test.ts`、`zz-r2-probe3.test.ts`、`zz-r2-useeffect.test.ts`、`zz-r2-connid.test.ts`、`zz-r2-hostkey.test.ts`、`zz-r2-hostkey2.test.ts`、`zz-r2-pairing.test.ts`、`zz-r2-dispose.test.ts`。
- 最终 `git status --porcelain` **输出为空**，`HEAD` 仍为 `f6ea252e09fd65834a67fe7123c786beec339209`，`git stash list` 为空。
