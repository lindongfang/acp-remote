# TP3 独立只读检视报告 r1

- **被检视对象**：`.worktrees/tp3` @ `16ef844fefd1d25ad9119b22f166df1aa2f02ca1`（基线 `f6ea252e09fd65834a67fe7123c786beec339209`）
- **作者报告**：`reports/deliver-tp3-r1.md`
- **检视方式**：只读。未提交、未改动 `feat/tp3`。共施加 **17 项临时源码变异**，逐项跑测试后立即 `git checkout --` 还原；另建 1 个临时探针文件（`src/zzprobe.test.ts`）已删除。最终 `git status --porcelain` **为空**，`HEAD` 仍为 `16ef844`，`git stash list` 为空，`fixtures/`/`schemas/`/`compatibility/` 的 diff 为 0 行。

---

## 1. 结论

**PASS —— 不阻断合入。** 但带 **2 条 MAJOR**（判别力缺口，作者自报时未察觉）与 **5 条 MINOR / SUGGESTION**。

| 级别 | 数量 |
| --- | --- |
| CRITICAL | 0 |
| MAJOR | 2 |
| MINOR | 3 |
| SUGGESTION | 2 |

**为什么不是 FAIL**：本包的核心交付——跨语言固定向量对拍——**经我独立验证是真的**，不是自证循环、不是替身冒充实测。这是本包最难的一件事，而它做对了。R13/R14/R17/R19/R20 的新增覆盖也都有真实判别力（我用变异逐条证实）。两条 MAJOR 都是**局部断言过弱**，不影响任何一条 spec MUST 被证伪，也不掩盖实现缺陷——它们是「声称覆盖得比实际更多」，方向上偏保守（宁可少说），不是虚报。

**为什么必须记 MAJOR**：交付报告 §1 的 R12 行写着「每条合法迁移都断言了目标状态」，§2 变异表把 15 项都标为「🔴 捕获」并给出用例名。这两处陈述与代码不符，且我在**新文件单独跑**时确认了逃逸。这是可信度问题，不是覆盖缺失。

---

## 2. Write Scope 核实：无越界

`git diff --name-only f6ea252 16ef844` 的 8 条路径全部以 `.test.ts` 结尾且位于 `clients/app/` 下；`git diff --numstat` 8 行全为 `N 0`（**零删除行**），即**没有改动任何既有文件的一行**。

| 检查 | 结果 |
| --- | --- |
| 是否只新增 `clients/app/**/*.test.ts` | ✅ 8/8 |
| 是否删除/改写既有测试 | ✅ 零删除行，既有 28 个测试文件一行未动 |
| 是否改动实现文件（`composition.ts`、`connection.ts`、`local-cache.ts` 等） | ✅ 零 |
| 是否改动 `app/**`、`scripts/**`、`package.json`、`vitest.config.ts` | ✅ 零 |
| `fixtures/`、`schemas/`、`compatibility/` 是否被改 | ✅ `git diff … -- fixtures/ schemas/ compatibility/ \| wc -l` = **0** |
| 是否在 `.test.ts` 里 monkey-patch 生产模块导出 | ✅ 无。唯一的包装 `recordingIdentity`（`r11-fixed-vectors.test.ts:249`）**原样转发**给真实端口，且是本文件自己造的对象，不是对生产模块的改写 |

**结论：Write Scope 干净。**

---

## 3. 跨语言固定向量对拍：**可信**（本包最核心交付，独立判定为真）

这是我重点核查的一项，逐个子问题回答：

### 3.1 有没有复用 `composition.test.ts` 里那个 `verifyHostChallenge → return true` 的替身？

**没有。** 三条独立证据：

1. **代码层**：`r11-fixed-vectors.test.ts:313` 注入的 `ports` 只有 `{ clock, random }`。`composition.ts:598-600` 的 `inputs.ports?.host ?? new PairedHostIdentity(...)` 因此取 `undefined` → 走**生产 `PairedHostIdentity`**。相比之下 `composition.test.ts:208` 注入的是 `{ clock, digest: fakeDigest(), host: cacheBackedHost(cache) }`——三者全被替身化。r11 一个都没注入。
2. **字节层**：`host-challenge.json` 的 `p1363Signature` 是 Rust 侧签的固定字节。若 `verifyHostChallenge` 是 `return true`，这条签名**根本不会被读**，用例会平凡通过。
3. **变异层（决定性）**：我把 `composition.ts:277-279` 的 `crypto.subtle.verify(...)` 整段替换为 `return Promise.resolve(true)`（即把生产替身化的极端情形），跑 r11 → **1 failed**（`同一向量换一个字节即被真实验签拒绝并进入 identity_changed`）。真实验签确实在跑。

同样的逻辑适用于 `digestPort`：把 `composition.ts:152-154` 的 SHA-256 输出首字节 `^= 0xA5` → r11 **2 failed**。真实摘要端口确实参与校验。

### 3.2 判据「向量签名能验过生产字节」是否等价于「逐字节相同」？

**等价，且我独立验证了两端。**

- **TS 端签的字节 vs Rust 端签的字节**：Rust `SyncDeviceProof::verify`（`crates/identity-auth/src/transcript.rs:611-618`）签/验的是 `self.transcript()?`，即 `acpr-transcript` 按 domain `acp-remote/device-proof/v1` 与 `tags [2,3,6,9,10,11,12]` 编出的完整 transcript。TS 生产 `connection.ts:412-421` 用**同一组 tag**（`TAG` 定义在 `connection.ts:60-69`：1/2/3/6/9/10/11/12）编 device proof。两侧 tag、顺序、domain 一致。
- **签名格式对齐**：fixture 的 `p1363Signature` 是 **64 字节 raw `r||s`**（实测），TS `crypto.subtle.verify` 接受 P1363 raw。我用 **Node `crypto.verify(..., 'ieee-p1363')`** 完全独立地复核了两个向量：

```
device-proof   len 303  sha256==expected: true  verify: true   tampered verify: false
host-challenge len 298  sha256==expected: true  verify: true   tampered verify: false
```

即：**Rust 签的那串字节 = 向量的 `transcriptBase64url`；而 TS 用例断言生产编出的字节逐字节等于该 base64url**（`r11-fixed-vectors.test.ts:516-518`，同时比 hex、base64url、SHA-256 三者）。ECDSA-SHA256 验签通过 ⟹ 被验字节与签名时字节相同（除非哈希碰撞）。**判据等价性成立。**

### 3.3 期望值是否来自独立实现（非自证循环）？

**是。** 三处期望值都独立于被测对象：

| 期望值 | 来源 | 独立性证据 |
| --- | --- | --- |
| transcript 字节 | `fixtures/sync/v1/transcripts/*.json`（Rust 产出） | TS 侧 `encodeAsRegistryDomain`（`r11:196`）虽是测试内的「服务端模型」，但它调 `encodeTranscript` 时传的 `tag` 来自 **registry JSON**（`r11:180-193`），与生产 `TAG` 常量是两个独立来源；且判据是 fixture 自带的 Rust 签名，不是测试自算的值 |
| 快照 digest | **Node `crypto.createHash('sha256')`**（`r11:653-658`、`r14-rebuild-snapshot.test.ts:270-274`） | 与被测的 `crypto.subtle.digest` 是两套 API；变异已证其判别力 |
| FNV 反证期望 | 测试内**重新实现** FNV-1a（`r11:466-480`） | 用来证明「若真实端口被换成 `fakeDigest` 则必然入不了仓」（`r11:765`） |

**没有从被测实现导出期望值的情况。** 唯一接近边界的是 `r11:644` 的 `expect(sortedFeatures(features)).toEqual([...features])`——它断言的是**向量自带的 features 已有序**，用的是生产导出的 `sortedFeatures`，但断言内容是输入数据的性质而非生产行为（见 MINOR-2）。

### 3.4 每个向量都被覆盖了吗？

**是。** `fixtures/sync/v1/transcripts/` 下与 R11 相关的两个向量都被完整驱动：

- `device-proof.json`：`r11:513`（生产字节逐字节相同）、`:524`（Rust 签名验过生产字节 + 同条内翻 bit 后验签失败）、`:539`（向量私钥导入后签出 P1363 可被向量公钥验证）、`:562`（真实端口不可导出密钥）。
- `host-challenge.json`：`r11:592`（真实 `PairedHostIdentity` 接受向量签名）、`:609`（换字节即拒并进 `identity_changed`）、`:630`（向量自洽：magic/version/domain 长度/字段数/features 有序）。

其余 5 个 pairing 系列向量属 R11 之外的配对面，TP3 范围只划给这两个，**不算漏**。

### 3.5 fixture 未被改动

✅ 见 §2 表。独立复核：`git diff f6ea252 16ef844 -- fixtures/ schemas/ compatibility/` 输出 0 行。

### 3.6 「反向验签在某条路径上退化成恒真」？

我逐条查了 r11 里所有「看起来很强」的断言，**没有恒真**：

| 断言 | 恒真风险 | 我的核实 |
| --- | --- | --- |
| `:520` `verify(production) === true` | 若 `verify` 参数搞反（如把签名当消息）可能恒真 | 同条 `:526-528` 翻一个 bit 后 `verify === false` → **非恒真** |
| `:518` `sha256Hex(production) === expected` | 若 `sha256Hex` 实现错 | 独立用 Node `crypto` 复算 fixture 的 SHA-256 与 `expected` 逐位相同（§3.2） |
| `:563-566` `privateKey.extractable === false` + `exportKey` rejects | 若 `openDeviceIdentityStore` 返回替身 | `exportKey` 由真实 `crypto.subtle` 抛出 `InvalidAccessError`（浏览器侧实测亦然） |
| `:632-643` 向量自洽（magic `ACPR`、version `1`、domain 长度、字段数） | 纯自洽断言，但断言的是 registry 视图 | 属**辅助**断言，不承担判别力；主判据是 `:516-518` 的 fixture 比对 |

**结论：跨语言对拍可信。** 这是本包最值得肯定的部分——它精确地堵住了 `review-wp6-r4 §7.3` 登记的 `fakeDigest` 与真实 SHA-256 无共享向量对拍的空洞。

---

## 4. 变异验证：我独立复现的结果

作者自报 15 项变异「全部被捕获」，并主动交代 #13、#14 第一版未被捕获。我挑了 **8 项** 亲自复现（覆盖 R11/R12/R13/R14/R17/R19/R20 各域），外加 9 项自主探索变异。**17 项全部已还原。**

### 4.1 作者重点交代的两项（可信度的关键）——都真的变红了

| # | 变异 | 我的实测 | 与自报 |
| --- | --- | --- | --- |
| **13** | `connection.ts` `#onResetRequired` **不调用** `discardStaging()` | `r14-rebuild-snapshot.test.ts` **1 failed / 2 passed**。红的是 `:379` `expect(storedTitles(...)).toEqual([COMPLETED_TITLE])`，实收 `["未完成的暂存快照"]` | ✅ 一致（作者自报 1 红） |
| **14** | `local-cache.ts` 淘汰排序 `lastUsedAtMs` → `updatedAtMs`（退化为 FIFO） | `r20-no-content-cache.test.ts` **1 failed / 12 passed**。红的是 `:196` `expect(await cache.get("a", NOW)).not.toBeNull()`，实收 `null` | ✅ 一致（作者自报 1 红） |

**这两项的重做是真实有效的，不是改断言改绿。** 我特意检查了 #13 的重做形态：`r14-rebuild-snapshot.test.ts:370-379` 确实在 `reset_required` 之后**主动补齐被打断快照的剩余 chunk 并按完整三 chunk 的摘要收尾**——正是作者描述的「把暂存区是否被丢弃变成可观测事实」。#14 的 `:185-188` 三条 `putLocalSummary` 的 `nowMs` 确实**完全相同**（`writtenAt`），只有一次 `get("a")` 抬 `lastUsedAtMs`。

### 4.2 其余我复现的作者变异

| # | 变异 | 我的实测（跑对应新增文件） | 与自报 |
| --- | --- | --- | --- |
| 1 | `connection.ts` device proof 的 `deviceId` tag 由 3 改成 11 | r11 **6 failed / 4 passed**，含「生产字节逐字节相同」「Rust 签名验过」「host-challenge 接受」「三条 digest 用例」 | ✅ 一致 |
| 2 | `composition.ts` `digestPort` 输出首字节 `^= 0xA5` | r11 **2 failed / 8 passed** | ✅ 一致 |
| 3 | `composition.ts` host challenge 的 `canonicalOrigin` tag 6→7 | r11 **6 failed / 4 passed** | ✅ 一致 |
| 4 | `composition.ts` **完全跳过** ECDSA 验签（`return Promise.resolve(true)`） | r11 **1 failed / 9 passed**，红的正是那条负向 tamper 用例 | ✅ 一致 |
| 5 | `connection-machine.ts` 让 `revoked` 也接受 `connect_started` | r12 **3 failed / 15 passed**（期望表逐条相等 / 非法组合逐条抛错 / 阻断态不得直接直接 connect_started） | ✅ 一致 |
| 6 | `canMarkInputAsSent` 在 `replaying` 也返回 true | r12 **2 failed / 16 passed** | ✅ 一致 |
| 11 | `conversation.ts` 窗口为零时返回 `known` + `percent: 0` | r17-r19 **3 failed / 10 passed** | ✅ 一致 |
| 15 | `connection.ts` 去掉缺口恢复次数上限 | r14-replay-recovery **1 failed / 6 passed**，红的正是「缺口恢复有次数上限」 | ✅ 一致 |

**8/8 与作者自报逐位一致。作者的变异表可信。**

### 4.3 我自主探索的 9 项变异（作者未列）

| 变异 | 实测 | 说明 |
| --- | --- | --- |
| 9 | `dedupe.ts` 去重键 `eventId` → `globalSequence` | 全量 **2 failed**，均在既有 `dedupe.test.ts`；**新增的 r14 文件 7/7 全绿**（见 MINOR-1） |
| 9b | `dedupe.ts` 去重键 `eventId` → `messageId` | r14-replay-recovery **1 failed**（「换 messageId 但 eventId 相同仍判重复」）——作者自报 #9 的等价物确实被新增用例抓住 |
| 10 | `snapshot.ts` `begin()` 不再 `discard("superseded_by_new_snapshot")` | 全量 **1 failed**，在**既有** `snapshot.test.ts`；**新增 r14-rebuild-snapshot 3/3 全绿**（作者在表里标注「由 #13 一并覆盖」，实际由既有测试覆盖） |
| 12 | `agent-catalog.ts` 覆盖层缺席时默认 `disconnected` | 全量 **14 failed**，含 r17-r19 的 4 条与既有 r19/domain 的多条 | ✅ |
| 7 | `command-machine.ts` `retrySameRequest` 生成新 `crypto.randomUUID()` | r13-command **10 failed / 10 passed** | ✅ 一致 |
| 8 | `markConnectionLost` 在 `submitting` 时自行判 `uncertain` | r13-command **1 failed / 19 passed** | ✅ 一致 |
| R13 | `client.ts` `dispatch` 用 `random.uuid()` 换掉 `command.body.requestId` | r13-dispatch **6 failed / 2 passed**（含 wire 层那条） | ✅ 判别力强 |
| R17/R19 | `client-store.ts` 关闭 `usage` 映射写入（`if (usage !== null …)` → `if (false …)`） | r17-r19 **3 failed / 10 passed** —— 证明「经 `ClientStore` 生产管线」不是文档声称 | ✅ |
| R20 | `local-cache.ts` 关掉 `assertPersistableForOrigin` 的 imported 分支 | 全量 **7 failed**，含 r20 新增文件 3 条 | ✅ |

---

## 5. 逐条需求覆盖核对

### 5.1 R12（132 条手写期望表）——**期望表独立，但「断言目标状态」这一说法不成立**

| 攻击点 | 我的判定 |
| --- | --- |
| 期望表是否独立手写（非从 `ALLOWED_SOURCES` 导出）？ | ✅ **独立**。`EXPECTED_SOURCES`（`r12:74-107`）是本文件内的字面量，文件里**没有 import `ALLOWED_SOURCES`**（只 import 了公开的 `allowedSourceStates()`）。作者自报成立。 |
| 期望表内容是否正确（与生产逐条相等）？ | ✅ 用例 `:189` 逐条比对相等；变异 5 证明任何增删都会红。 |
| **合法迁移是否逐条断言了目标状态？** | ❌ **否**。见 MAJOR-1。 |
| 非法迁移是否逐条断言 `from`/`event` 字段？ | ✅ **是**。`:202-224`：不只断言「抛错」，还断言 `error instanceof IllegalConnectionTransition`、`error.from === state`、`error.event === kind`，并把两类失败分别收集到 `accepted`/`wrongFields` 数组。 |
| `caughtUp === (state === 'online')` 是否成立且用例能红？ | ✅ **成立且能红**。`:277` 对每条合法迁移断言该恒等式；把 `caught_up` 分支改成 `caughtUp: false` 会红。 |
| 「12 态中只有 `online` 可标记已发送」是否成立且能红？ | ✅ **成立且能红**。`:341` 收集所有 `canMarkInputAsSent === true` 的态并断言 `=== ["online"]`；变异 6 直接证伪。 |
| 阻断态停止重连 / 原因与下一步？ | ✅ `:288`、`:298`、`:317` 三层断言（构造态、由 `blocked` 事件产生的态、非阻断态反向清单），非空判定用 `.length > 0`。 |
| 顶替后旧页面停止争抢？ | ✅ `:329` 四个阻断态均不接受 `connect_started`，且 `blocking_cleared` 只回 `disconnected`（断言 `canMarkInputAsSent === false`，防「解除后直接在线」）。 |

### 5.2 R13（`requestId` 复用）——**三层都真覆盖，wire 层确实取了三次出站**

- **状态机层** ✅ `r13-command-idempotency.test.ts:270`（`retrySameRequest` 后 `requestId` 不变、`dispatchCount` 递增）、`:284`（连续 5 次重试，`ids.size === 1`、`dispatchCount === 6`）。
- **派发层** ✅ `r13-dispatch-idempotency.test.ts:114`（三次派发 `dispatchCount === 3`、登记只有一条）、`:141`（换 ID 得第二条独立登记，且从未派发的 ID 查不到 `null`）。
- **wire 层** ✅ **确实取了三次出站，非平凡**。`:158-176`：先 `authenticate()` 建立连接，再连派三次，然后 `const sent = wire.map(m => m.parsed).filter(m => m["type"] === "command")` 并**先断言 `expect(sent).toHaveLength(3)`**，再断言 `new Set(ids)` 只有一个成员。若只发了一次，`toHaveLength(3)` 会先红。我用 `client.ts` 的 `dispatch` 变异实测该用例变红。

**其余 MUST**：`:222` 对 3 个非终态逐一 `expect(after).toBe(before)`（同一对象引用）+ `serverConfirmedUncertain === false`；`:242`/`:255` 穷举本地入口（断线/提交/重试都到不了 `uncertain`）；`:301` 4 终态 × 5 迟到状态 × 2 入口全部 `toBe(before)`；`:335`/`:347` 呈现口径。**判别力经变异 7/8 证实。**

### 5.3 R14（§9.3 建模）——**真正建模了 §9.3，不是「发了就等」**

`attachReplayServer`（`r14-replay-recovery.test.ts:114-170`）确实是响应式的：

1. 包装 `socket.send` **在握手之前装上**（`:150` 注释明确，`:151` `bind` 后覆写）——认证后的首条 subscribe 同样触发重放；
2. 收到 `sync.subscribe` → 记 `lastCursor`、`subscribes += 1`、从 cursor 之后取**可见**事件按 `globalSequence` 升序入队；
3. 重放后必发 `sync.caught_up` 屏障（§9.3 第 4 步）；
4. `drain()` 逐条 `deliver`（非同步递归，注释解释了原因）。

**两个场景放在同一条模型上互为对照**：`:231` 用 `withheld: {}`（服务端不扣任何事件）→ 断言 `presentedSequences === ["1","2","3","4","6","7","8","10"]`、`gap_detected` 为 0、`subscribeCount === 1`；`:262` 用扣下 7 的服务端 → 断言 7 被补齐呈现一次、序列 `["1","2","3","4","6","8","10","7"]`、水位不回退、触发一次重订阅。

**这一条回应了 `review-wp6-r3` 的批评**（既有替身不建模 §9.3 是三轮翻车的根源）：既有的 `attachReplayServer` 在 `connection.test.ts` 里，本文件**另写**了一个。✅

**事件去重**：`:328` 重投 3 次 → 呈现 1 次 + 2 次 `duplicate_dropped` + 只发 1 条 ACK；`:344` 换 `messageId` 仍判重复（变异 9b 证实这条能红）。

**重建快照**：`r14-rebuild-snapshot.test.ts` 走**真实组合根 + 真实 `SnapshotStaging` + 真实 `ClientStore`**（`:104-142`），三条用例覆盖 `reset_required` 丢弃、被新 begin 取代、摘要不符不提交。变异 13 证实。

**一处不精确**（MINOR-1）：作者变异 #9 声称被「r14 的三条 eventId 去重用例」捕获，实测新增文件对 `eventId → globalSequence` 变异 **7/7 全绿**，实际捕获者是既有 `dedupe.test.ts`。作者对 `eventId → messageId` 的等价变异（#9b）确实被新增用例抓住，所以「去重键不是 messageId」已覆盖；**只有「去重键不是序号」这一半**仍只有既有测试守着——而既有测试守着，所以无覆盖缺口，只是**归因写错**。

### 5.4 R17（未上报不得以零冒充）——**断言方式可靠**

`:88` 的做法是：先 `renderToStaticMarkup` 出 HTML，断言不含 `0%`/`data-context-used`/`data-context-size`/`NaN`/`Infinity`，再用 `html.replace(/data-context-state="unreported"/u, "")` 剔除属性后断言 `/[0-9]/.test(...) === false`。

**我实测这个剔除是否会误判**：临时探针渲染出

```
HTML>>>    <div data-context-state="unreported" data-context-label="未上报"><span>上下文未上报</span></div><<<
STRIPPED>>> <div  data-context-label="未上报"><span>上下文未上报</span></div><<<
HAS_DIGIT>>> false <<<
```

剔除精确命中该属性（React 服务端渲染的引号形式一致），剩余部分确实无数字。**不会误判，也不会漏判**——若实现多渲染任何数字槽位（`<progress>`、`data-context-percent`、百分比文案）都会带上数字而使断言变红。组件 `ContextUsageBar.tsx:30-36` 的 `unreported` 分支确实没有数字槽位。✅

**其余**：`:100` 零窗口 → `kind === "zero_window"`、无 `data-context-percent`；`:113` 三态 `kind` 互不相同（`new Set(kinds).size === 3`）；`:140` 三态下 `capabilities` 完全相等；`:169` 仓库 `usage` 映射**无该键**（「从未收到」由缺席表达）——我变异 `client-store.ts:325` 的写入后 **3 条红**，证明这条真的经生产管线。✅

### 5.5 R19（Agent 目录两层）——**经 `ClientStore` 生产管线**

`:187` 覆盖层缺席 → `["unknown","unknown"]` + 长度 2 + `overlay` 长度 0；`:203` `connectionLabel === "状态未知"` 逐行 + `connection === "unknown"` 逐行；`:219` 会话 `state: "running"` 之后 overlay 仍空（**不以会话活跃度推断**）；`:243` 同名 Agent 以 `agentId` 区分（两条 `displayName: "codex"` 各自保留独立状态）；`:342` `connectionState === "reconnecting"` 时面板仍有 2 行 + 离线快照说明。

变异 12（默认 `disconnected`）→ **14 red**。✅ 这一条正面回应了 `review-wp7-r1` 的「直调模型漏掉两处生产缺陷」批评。

### 5.6 R20（imported 不落盘）——**走真实 IndexedDB 管线，翻查断言有效**

**`.test.ts` 里翻存储全文的断言是否真的有效？** 我核实了三件事：

1. `__fakeIndexedDBDump()`（`fake-indexeddb.ts:130-137`）**遍历所有 store 的所有 record** 并返回完整数组，**不是**只返回当前读的 key。
2. `persistedText()`（`r20:68`）对它 `JSON.stringify` ——**键与值都在**。
3. **断言检查了全部七类**：`:77-81` 对 7 个 `ContentKind` 逐一 `routeContent(REMOTE, kind) === {destination:"memory_only"}`；落盘侧则用 `assertPersistableForOrigin` 的**字段允许清单**（`local-cache.ts:199-206`，7 个字段）在运行期拦截**任何**多余字段——这是**类型之外的运行期守门**，比逐类断言更强。

**「元数据确实落盘了」这条对照存在**（`:106` `expect(persistedText()).toContain(METADATA.contentDigestSha256)`），因此「正文不在」不是「什么都没写」的平凡通过。✅

变异（关掉 `assertPersistableForOrigin` 的 imported 分支）→ **7 red**，含 r20 的 3 条。✅

**LRU 判别力**：`:179-205` 三条 `putLocalSummary` 的 `nowMs` **完全相同**（`writtenAt = NOW - 1000`），只有一次 `get("a", NOW)` 抬 `lastUsedAtMs`；随后断言 `a` 存活、`b` 被淘汰、`c`/`d` 存活。变异 14 精确命中 `:196`。✅

**容量固定不可配置**：`:148` 四个常量逐值断言 + `:170` `openLocalCache.length === 0`（无入参即无配置入口）。✅

---

## 6. 新发现的分级清单

### MAJOR-1：R12 的「合法迁移断言了目标状态」不成立——`disconnect_requested` 与 `identity_lost` 的目标态零断言

- **位置**：`clients/app/src/state/r12-connection-transitions.test.ts:183-200`（用例本体）、`:17-18`（文件头声称）
- **现象**：文件头 `:17-18` 明写「因此每条合法组合都同时断言**目标状态**与**阻断载荷**」；交付报告 §1 R12 行亦称「每条合法组合真的被接受」。但 `:183` 的用例体只 `try { transition(...) } catch { rejected.push(...) }` ——**只断言「不抛错」，完全不检查返回对象的 `state`**。文件里唯一断言目标态的地方是 `:230`（`ALL_STATES` 包含）与 `:277`（`caughtUp === (state==="online")`）——后者对**任何**非 `online` 目标态都成立，因此对「目标态选错」零判别力。
- **影响**：我实测把 `connection-machine.ts:273` 的 `disconnect_requested` 目标从 `disconnected` 改成 `unpaired`，**全量 451 条全绿**；把 `:264` 的 `identity_lost` 目标从 `unpaired` 改成 `disconnected`，**新增 r12 文件 18/18 全绿**（全量仅被既有 `connection-machine.test.ts` 抓到 1 条）。即：**用户主动断开后 UI 会显示「未配对」、浏览器清数据后会显示「已断开」这类真实缺陷，当前无任何新增用例能抓住**。R12 的 spec MUST 是「以互斥状态机表达连接状态」，目标态选错仍是该状态机的违约。
- **复现**：
  ```bash
  # connection-machine.ts:272-273，把 disconnect_requested 的目标态改成 unpaired
  cd clients/app && npx vitest run          # → 36 files / 451 passed（无一条红）
  npx vitest run src/state/r12-connection-transitions.test.ts   # → 18 passed
  ```
- **建议处置**：在 `:183` 的用例里把 `transition(machineIn(state), event)` 的返回值收集起来，对每条合法组合断言一个**手写的目标态表**（与 `EXPECTED_SOURCES` 并列的第二张表，例如 `EXPECTED_TARGETS[kind]`）；或至少对 11 个事件各加一条「典型来源 → 目标态」的点对点断言。文件头的措辞应与实际断言对齐。

### MAJOR-2：R20 的「imported 来源即便 online 也不可标记」断言的是**无生产调用方**的函数

- **位置**：`clients/app/src/platform/r20-no-content-cache.test.ts:146-148`
- **现象**：该用例断言 `mayMarkInputAsSent({ online: true, origin: REMOTE })` 为 `false`。但 `grep -rn "mayMarkInputAsSent" clients/app/src clients/app/app | grep -v test` 的结果只有 **定义处 `imported-content.ts:76` 与 re-export `index.ts:86`**——**全仓无任何生产代码调用它**。真正决定 UI 能否标记已发送的是 `conversationCapabilities`（`features/conversation-model.ts:141-155`），它经 `client-store.ts:474` 被 `ClientStore` 调用，用的是 `input.origin === "local"` 这个**不同的判据**。
- **影响**：我实测把 `conversation-model.ts:147` 的 `sendable` 改为 `(input.origin === "local" || input.origin.kind === "remote")`（即生产路径上 imported 会话**允许**标记已发送，直接违反 R20 的 MUST），**全量 451 条全绿**。既有 `r17-context-usage.test.ts:168` 那条只覆盖 `online: false` 的场景（`connectionOnline` 已为 false，`sendable` 因连接未在线而为 false，**没走到 origin 判据**），所以也抓不住。也就是说：**R20「MUST NOT 把用户输入标记为已发送或已排队」在生产路径上目前无任何用例守住**。
- **复现**：
  ```bash
  # features/conversation-model.ts:147
  #   const sendable = input.connectionOnline && open && input.origin === "local";
  #   → ... && (input.origin === "local" || input.origin.kind === "remote");
  cd clients/app && npx vitest run          # → 36 files / 451 passed（无一条红）
  ```
- **建议处置**：新增一条经 `ClientStore.conversationPage()` 的用例——构造 `origin: { kind: "remote", online: true }` 的会话**且连接处于 `online`**，断言 `capabilities.canSend === false`。（`r17-r19-store-contract.test.ts` 的 `storeWithSnapshot()` 已把连接推到 `online`，只需换一份带 remote-origin 会话的快照。）另建议处置 `mayMarkInputAsSent` 这个孤儿导出：要么接线，要么在 `tasks.md` 登记为待接线，否则它会持续误导读者以为「输入标记」已被守住。

### MINOR-1：变异 #9 / #10 的捕获者归因写错

- **位置**：`reports/deliver-tp3-r1.md` §2 表格第 9、10 行
- **现象**：#9（去重键 `eventId → globalSequence`）标注捕获者是「r14 的三条 eventId 去重用例」，实测**新增 `r14-replay-recovery.test.ts` 7/7 全绿**，实际捕获者是既有 `dedupe.test.ts` 的两条。#10（`snapshot.begin` 不丢弃）标注「由 #13 一并覆盖」，实测**新增 `r14-rebuild-snapshot.test.ts` 3/3 全绿**，实际由既有 `snapshot.test.ts` 捕获。
- **影响**：无覆盖缺口（两项都有既有测试守着），但**变异表的可信度受影响**：读者会以为「同源覆盖」更强。另需注意新增的 `r14-replay-recovery.test.ts` 缺了「去重键不是序号」这一半——若将来既有 `dedupe.test.ts` 被重构，这半会失守。
- **建议处置**：把 #9/#10 的捕获者改为既有文件名；在 `r14-replay-recovery.test.ts` 补一条「同序号不同 `eventId` 判 `late` 而非 `duplicate`」的用例（成本约 10 行）。

### MINOR-2：`sortedFeatures` 的断言是输入数据自洽性，不覆盖生产排序

- **位置**：`clients/app/src/r11-fixed-vectors.test.ts:643-644`
- **现象**：`expect(sortedFeatures(features)).toEqual([...features])` 断言的是**向量自带的 features 已经有序**——用的是生产导出的 `sortedFeatures`，但断言内容是输入数据的性质。
- **影响**：低。spec §5.2 要求的「features 按 UTF-8 升序」在**生产侧**由 `encodeNulJoinedUtf8` 的硬校验保证（`transcript.ts:143-150` 顺序错误即抛 `field_order`），不是软约定，因此**没有静默违约风险**。我实测把 `connection.ts` 与 `composition.ts` 里的 `sortedFeatures(...)` 全部去掉，**451 条全绿**——说明这条断言确实不覆盖生产排序；但由于编码器有硬校验，去掉排序只会让未排序输入**抛错**而非产出错字节。
- **建议处置**：可不动。若要收紧，构造一条未排序的 `selectedFeatures` 喂进握手，断言因 `field_order` 阻断即可。

### MINOR-3：`npm run check` 在 worktree 内 exit 1 的定性——我确认是环境限制，且主 Agent 的核实成立

- **位置**：worktree 根 `npm run check`
- **现象**：我实测 exit=1，前 9 道门禁全绿（schemas / commands / errors / features / assets / acp / docs / boundaries / drift，含 `check:assets` 的「12 transcript vectors re-encoded from input」），第 10 道 `check:agentic` 输出：`agentic 门禁：缺少 node_modules/@dongfanglin/openspec-agentic/bin/openspec-agentic.mjs，请先在仓库根运行 npm ci。` worktree 根**确无** `node_modules`（`ls -d node_modules` → 无；主检出 `D:/Project/acp-remote/node_modules` 存在）。
- **影响**：不构成覆盖缺口。TP3 只新增 `clients/app/**/*.test.ts`，十道根门禁均不检视该路径。我在主检出跑 `npm run check` → agentic **22 passed, 0 failed**。
- **建议处置**：无。登记为环境限制即可。

### SUGGESTION-1：三条未验证项的定性——均**不阻塞** TP3 合入

作者登记的三项未验证，逐条我的判定：

| 项 | 我的判定 |
| --- | --- |
| **真实浏览器内的私钥结构化克隆保真** | **不阻塞**。这是浏览器内核语义，Node 侧的 `fake-indexeddb` 明确声明「不声称实现对结构化克隆的保真」（`fake-indexeddb.ts:11-14`），作者如实登记了边界。既有 16 条浏览器断言（含 `R11 真实 IndexedDB 往返后 extractable 仍为 false`）在**真实 Chromium** 里跑过，我实测 `node scripts/run-browser-check.mjs` → **16/16 passed, exit 0**。TP3 未新增浏览器断言，但这属于 TP4 范围。**归属正确，无低估。** |
| **`harness.ts` 的 `waitUntil` 只轮转微任务** | **不阻塞，且作者高估了它对本包的影响**。我核实 `harness.ts:500-507` 确实只 `await Promise.resolve()`。但作者的处置是**本包三处自带宏任务等待器**（`r11:78-90`、`r14-rebuild-snapshot:45-56`），因此**本包的用例不受此局限影响**——这是测试基础设施的既有局限，不是覆盖缺口。 |
| **`composition.ts:712` 固定传 `resumeCursor: null`** | **不阻塞**。我核实 `:711-712` 确有注释「恢复点跨**连接**由门面的 `#ackedCursor` 承接；跨进程需要持久化，本组合根不持有第二份恢复点存储」。R14 的 spec 场景是「服务端要求重建快照时丢弃未完成的暂存快照」——**跨进程恢复点不在 R14 的 MUST 文本内**（spec 原文只说「从最后确认的位置连续恢复」，而 `r14-replay-recovery.test.ts:229` 覆盖的正是这条）。作者的定性准确。 |

**唯一需要补充的未验证项**（作者未登记）：上述 MAJOR-2 揭示的「生产路径的输入标记判据无用例」与 MAJOR-1 的「目标态无断言」——这两处本应被登记为已知缺口。

### SUGGESTION-2：`attachReplayServer` 的 `withheld` 参数从未被使用

- **位置**：`clients/app/src/sync-client/r14-replay-recovery.test.ts:114-133`
- **现象**：`options.withheld` 与 `withheldUsed` 逻辑齐备（`:123`、`:130`、`:133`），但两处调用都传 `{}`（`:232`、`:249`）——**从未走过 withheld 分支**。真正测「服务端漏投」的是 `:265-283` 里另写的一份 `socket.send` 覆写。
- **影响**：纯冗余，无覆盖影响（漏投场景由 `:262` 那条独立模型覆盖，且其判别力经变异 15 证实）。但读者会以为 `:262` 复用同一模型，实际是两份实现。
- **建议处置**：删除 `withheld`/`withheldUsed` 参数，或把 `:262` 那条改用 `attachReplayServer(context, { withheld: new Set(["7"]) })` 统一。

---

## 7. 其它核实项

| 项 | 结论 |
| --- | --- |
| **既有测试是否被削弱** | ✅ **零削弱**。`git diff --numstat` 8 行全为 `N 0`，无任何删除或改写行。作者的 359 → 451 净增 92 条（实测 `grep -c '^  it('`：10+3+13+18+20+8+7+13 = **92** ✅）与自报一致。 |
| **用例总数** | ✅ 独立跑出 **36 files / 451 passed**，与自报一致。 |
| **恒真断言 / 只断言非空 / 复制被测逻辑 / 断言替身回声** | 未发现。R12 期望表、R13 三层登记、R14 §9.3 模型、R17 渲染断言、R19 生产管线、R20 存储翻查——逐条经变异验证有真实判别力。唯一「像恒真」的两处（`r11:520` 的 verify、`r11:88` 的无数字）经我独立复核**均非恒真**。 |
| **`tsc --noEmit`** | ✅ exit 0，无诊断。 |
| **浏览器断言** | ✅ `node scripts/run-browser-check.mjs` → 16/16 passed, exit 0。与自报一致（本包未新增浏览器断言）。 |
| **worktree 最终状态** | ✅ `git status --porcelain` 为空；`HEAD = 16ef844fefd1d25ad9119b22f166df1aa2f02ca1`；无 stash；fixture/schema/registry diff 0 行。 |

---

## 8. 最终判定

**PASS，不阻断合入。** 两条 MAJOR 建议在下个测试包补齐（成本都很低：MAJOR-1 加一张目标态表，MAJOR-2 加一条 online + remote 的 store 用例）。本包的核心价值——跨语言固定向量对拍真的用生产实现、判据真的等价于逐字节相同、期望值真的来自独立实现、fixture 真的没被动过——**四项全部独立核实成立**，且作者主动交代的两处返工（#13、#14）经验证确实变红，其变异表 8/8 与我的实测逐位一致。这是一份可信度较高的测试交付。
