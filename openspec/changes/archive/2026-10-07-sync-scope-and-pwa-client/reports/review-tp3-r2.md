# TP3 独立只读复核报告 r2

- **被检视对象**：`.worktrees/tp3` @ `6e8307096629dcfd3fa63ac20559e25b0a3e7a7a`（基线 `f6ea252`；r1 = `16ef844`）
- **作者报告**：`reports/deliver-tp3-r2.md`
- **对照报告**：`reports/review-tp3-r1.md`
- **本轮 diff**：`git -C .worktrees/tp3 diff 16ef844..6e83070` = **5 files / +226 −27**（`r20-input-mark-production-path.test.ts` A 92/0、`r20-no-content-cache.test.ts` 6/0、`r11-fixed-vectors.test.ts` 24/1、`r12-connection-transitions.test.ts` 76/5、`r14-replay-recovery.test.ts` 28/21）。累计 `f6ea252..6e83070` = **9 files**，9/9 以 `.test.ts` 结尾（任务书写的「9 files / +3542」是累计口径，非本轮口径）
- **检视方式**：只读。未提交、未改动 `feat/tp3`。共施加 **10 项临时源码变异**（MUT-A…MUT-J，全部我自选，非作者 M1–M7），逐项跑测试后立即 `git checkout --` 还原。最终 `git status --porcelain` **为空**、`HEAD` 仍为 `6e83070`、`git stash list` 为空、`fixtures/ schemas/ compatibility/ crates/` 的 diff **0 行**。

---

## 1. 结论

**PASS —— 不阻断 MU3e 合入。**

| 级别 | 数量 |
| --- | --- |
| CRITICAL | 0 |
| MAJOR | 0 |
| MINOR | 0（有 4 条非阻断观察，见 §6，均不构成覆盖缺口） |

r1 的两条 MAJOR 我都用**自选的**变异独立复现，确认已从「改坏实现全仓全绿」变成「改坏实现必红且失败信息指向被测面」。5 条 MINOR/SUGGESTION 的处置逐条成立（§4）。基线复跑：`npx vitest run` = **37 files / 455 passed**（与自报一致，r1 的 451 → 455，+2 新文件 +1 r11 +1 r14），`npx tsc --noEmit` exit 0。

---

## 2. MAJOR-1（R12 目标态零断言）——**判定：真修复**

### 2.1 期望表独立性（我独立核查，非采信报告）

| 核查点 | 实测 |
| --- | --- |
| 是否 import `ALLOWED_SOURCES` | ❌ 否。`r12-connection-transitions.test.ts:27-44` 的 import 只有 `BLOCKING_STATES / CONNECTION_STATES / IllegalConnectionTransition / allowedSourceStates / canMarkInputAsSent / initialConnectionState / isBlockingState / shouldAutoReconnect / transition` 与类型。`ALLOWED_SOURCES` 只出现在 `:11` 的**注释**里 |
| `allowedSourceStates()` 的用途 | 只在 `:235` 与手写 `EXPECTED_SOURCES` **比对**，不参与生成期望值 |
| 目标态表是否字面量 | ✅ `EXPECTED_TARGET`（`:176-195`）、`EXPECTED_CONNECT_TARGET`（`:198`）、`EXPECTED_CLOSED_TARGET`（`:201`）、`TARGET_BY_CAUSE`（`:204-209`）全部是文件内字面量；`expectedTargetOf()`（`:212-223`）只做「按事件参数展开」，参数取自**本文件自造的** `everyEvent()` |
| 是否可能「改坏实现连期望表一起绿」 | ❌ 不可能：表不来自实现（见上），且我用 4 项实现侧变异逐项证伪（下表） |
| 穷举完整性 | 11 个事件种类**全部**在表内：8 个字面量 + `connect_started`/`connection_closed`/`blocked` 三张分支表。`ALL_EVENT_KINDS` 与生产 `ConnectionEvent["kind"]` 今日 11/11 一致 |

### 2.2 我自己的变异（**未使用作者的 M1/M2**）

每一项都是「只改实现的目标态，不改任何测试代码」，跑 `npx vitest run src/state/r12-connection-transitions.test.ts`：

| # | 变异（实现侧） | 结果 | 新断言的失败信息（原文） |
| --- | --- | --- | --- |
| MUT-A | `connection-machine.ts` `pair_succeeded` 目标 `disconnected` → `online` | **3 failed / 15 passed** | `pairing --pair_succeeded--> 期望 disconnected 实际 online`（`:270`） |
| MUT-B | `connection-machine.ts` `connection_closed` 两个分支对调（`retryable` → `disconnected`） | **1 failed / 17 passed**（**仅新断言红**） | 8 条，逐条 `… --connection_closed--> 期望 reconnecting 实际 disconnected` / `期望 disconnected 实际 reconnecting`（`:270`） |
| MUT-C | `connection-machine.ts` `connect_started` 两分支对调（`fromReconnect` → `connecting`） | **2 failed / 16 passed** | 12 条 `… --connect_started--> 期望 connecting 实际 reconnecting`（`:270`） |
| MUT-D | `connection-machine.ts` `BLOCKING_STATE_BY_CAUSE` 把 `device_revoked` ↔ `host_identity_changed` 互换 | **2 failed / 16 passed** | 20 条 `… --blocked--> 期望 revoked 实际 identity_changed`（`:270`） |

**红法干净**：四条变异的失败信息一律是「期望 X 实际 Y」的逐条差异列表，**没有**任何一条是崩溃、超时或与目标态无关的断言连带失败。MUT-B 尤其干净——**全文件只有新断言变红**，证明它对该目标态分支有独占判别力。

MUT-A/MUT-C 另有 `caughtUp === (state === "online")`（`:352`）与 `canMarkInputAsSent` 序列（`:443`）两条既有断言一并变红，但那是同一次实现改动的**独立后果**（改的是同一个 `transition` 分支），不是新断言的连带失败。

### 2.3 附加结构性证据（编译期穷举）

我在生产 `ConnectionEvent` 联合类型里插入一个新变体 `{ kind: "zz_probe" }`（MUT-J），`npx tsc --noEmit` → **exit 2**，报错直接指向两张期望表的定义行：

```
src/state/r12-connection-transitions.test.ts(89,7): error TS2741: Property 'zz_probe' is missing …  ← EXPECTED_SOURCES
src/state/r12-connection-transitions.test.ts(176,7): error TS2741: Property 'zz_probe' is missing … ← EXPECTED_TARGET
```

即：**新增事件种类不可能「悄悄绕开」目标态表**（表是 `Record<ConnectionEvent["kind"], …>`，缺键即编译失败）。这比 r1 时只有来源表强。

### 2.4 判定

**r1 MAJOR-1 已真修复。** 目标态断言不仅存在，而且在 4 个**不同**分支（无参事件、闭包分支、连接发起分支、阻断因果分支）上都有判别力；期望值不 import 实现常量、全为字面量；改坏实现确实变红且红法干净。还原后 r12 文件 18/18 全绿。

---

## 3. MAJOR-2（R20 断言孤儿函数）——**判定：真修复，且确实走生产管线**

### 3.1 生产链路核实（我自己读代码，非采信报告）

```
app 组件  ConversationStream.tsx:138/141   ← 只读 model.capabilities.canSend（全仓唯一消费方）
   ↑
ClientStore.conversationPage()  client-store.ts:451（声明）
   → conversationCapabilities({ connectionOnline: this.#state.connection.state === "online", sessionState, origin })  client-store.ts:474
   → sendable = connectionOnline && open && origin === "local"   conversation-model.ts:147
```

- `mayMarkInputAsSent` 全仓（含 `app/`、`crates/`、脚本）**只有定义 `imported-content.ts:76` 与 re-export `index.ts:86`**，零生产调用方——r1 的定性成立，作者在 `r20-no-content-cache.test.ts:155-159` 就地写明的注释与事实一致。
- 新用例的驱动路径是**真生产管线**：`ClientStore.apply()`（`client-store.ts:276-313`，内部经 `applyConnectionEvent → transition` 真状态机）推连接，`store.conversationPage()`（`client-store.ts:471`）取模型。**没有复刻** `conversationCapabilities` 的判据，也没有 monkey-patch 任何生产导出。
- 用例先断言 `store.state.connection.state === "online"`（`:71`）作为前提守卫，杜绝「因没连上而平凡为 false」；并有 `local` 来源对照 `canSend === true`（`:79`），杜绝「组件永远不出可发送」的平凡通过。我核对了 `conversationPage` 对不存在的会话返回 `null`——`remotePage?.capabilities.canSend` 会是 `undefined`，`toBe(false)` 同样会红，因此不存在静默通过路径。

### 3.2 我的变异（比作者 M3 更狠）

我不用作者那处「允许 remote」，而是把 `conversation-model.ts:147` 的判据**整条来源项删掉**（即新用例注释里的「反例 1」）：

```
- const sendable = input.connectionOnline && open && input.origin === "local";
+ const sendable = input.connectionOnline && open;
```

| 范围 | 结果 |
| --- | --- |
| `src/features/r20-input-mark-production-path.test.ts` | **2 failed / 0 passed** |
| 全仓 `npx vitest run` | **2 failed / 453 passed（455）**；Test Files **1 failed | 36 passed** |

失败信息：`expected true to be false`，位置 `r20-input-mark-production-path.test.ts:75` 与 `:90` —— 即断言真的命中了 `conversationCapabilities` 的 `sendable` 判据。**全仓唯一变红的文件就是新文件**，与 r1 实测的「改松后 451 条全绿」形成直接对照。

### 3.3 判定

**r1 MAJOR-2 已真修复**：生产路径上有判别力，且判别力来自真管线而非复刻逻辑。

---

## 4. 5 条次要项处置复核

| r1 条目 | 作者处置 | 我的复核 | 成立？ |
| --- | --- | --- | --- |
| MINOR-1 变异 #9/#10 归因写错 | 「修」：在**原报告** `deliver-tp3-r1.md:127` 就地加订正块，并补一条「同序号但 eventId 不同判 `late`」用例 | 订正块确实写在 r1 报告的对应表格下方（我读到的是 `deliver-tp3-r1.md:127`），且措辞与实测一致（原捕获者是既有 `dedupe.test.ts` / `snapshot.test.ts`）。新用例在 `r14-replay-recovery.test.ts:369-386`。我用变异 **MUT-H**（`dedupe.ts` 去重键 `eventId` → `globalSequence`，改 `#seen.has/set` 两处）复现：r14 文件 **1 failed / 7 passed**，红的正是新用例（`等待条件未成立：检出丢失区间`）。「去重键不是序号」这一半确实收进了本测试包 | ✅ |
| MINOR-2 `sortedFeatures` 断言只测输入自洽 | 「修」：在 `r11-fixed-vectors.test.ts` 补「乱序 features 喂进握手仍通过真实验签」 | 新用例在 `:609-631`。我用 **MUT-F**（去 `composition.ts:239` 的 `sortedFeatures`）→ r11 **1 failed / 10 passed**；**MUT-G**（去 `connection.ts:420` 的 `sortedFeatures`）→ r11 **1 failed / 10 passed**。两处生产编码点各被覆盖。原有那条 `expect(sortedFeatures(features))…`（`:667`）保留，注释改为如实说明其局限 | ✅（红的形态有偏差，见 §6-1） |
| MINOR-3 worktree 内 `npm run check` exit 1 | 「接受」（环境限制） | 我独立确认：worktree 根**无** `node_modules`；`node scripts/agentic-gate.mjs` 输出「缺少 `node_modules/@dongfanglin/openspec-agentic/bin/openspec-agentic.mjs`，请先在仓库根运行 `npm ci`」；`grep -rl "clients/app" scripts/` **0 命中**，即十道根门禁均不检视 `clients/app/**/*.test.ts`。属环境限制，非覆盖缺口 | ✅ |
| SUGGESTION-1 三项未验证项定性 | 「接受」（不阻塞） | 逐条复核成立：① `composition.ts:711-712` 确有「恢复点跨连接由门面的 `#ackedCursor` 承接；跨进程需要持久化，本组合根不持有第二份恢复点存储」的注释，`resumeCursor: null` 有据；② 本包自带宏任务等待器（`r11-fixed-vectors.test.ts:70-78` 的 `waitFor` 用 `setTimeout(resolve, 0)`），不受 `harness.ts:500-507` 只轮转微任务的局限影响；③ 浏览器侧断言属 TP4 范围，本包未新增、未声称 | ✅ |
| SUGGESTION-2 `attachReplayServer` 的 `withheld` 未被使用 | 「修」：漏投场景改用 `withheld`，删掉重复实现 | 现三处调用：`:232`/`:249` 传 `{}`（`scope 空洞` 等真实「不扣留」场景），`:266` 传 `{ withheld: new Set(["7"]) }`。r1 点名的重复实现（局部 `queue`/`subscribes`/`withheldPending` + `socket.send` 覆写）已删除，`subscribeCount()` 取代局部计数器。我用 **MUT-I**（`dedupe.ts` 把 `late` 判成 `duplicate`）复现：r14 **3 failed / 5 passed**，其中包含改造后的漏投用例——判别力未因改造而削弱。`:305` 仍有一处 `socket.send` 覆写，但它属于**另一个**场景（「缺口恢复有次数上限」：服务端在**每轮** subscribe 都只投 1 + barrier 20，需要「持续漏投」而非「仅首轮扣留」），与 `withheld` 的语义不同，不构成 r1 所说的重复 | ✅ |

---

## 5. 其它核实

| 项 | 结果 |
| --- | --- |
| Write Scope（零实现改动） | ✅ `git diff --name-only f6ea252..6e83070` 的 9 条路径 **9/9 为 `clients/app/**/*.test.ts`**；非 `.test.ts` 路径 **0**；`fixtures/ schemas/ compatibility/ crates/` diff **0 行** |
| 是否削弱既有断言 | ✅ 无。`r20-no-content-cache.test.ts` 只 +6 行注释；`r11` 的 1 行删除是注释改写；`r14` 的 21 行删除全部在测试内（移除重复的 `socket.send` 覆写与局部计数器）；`r12` 的 5 行删除是文件头注释改写。`git diff … | grep -n "\.only(\|\.skip("` → **无命中** |
| 用例数 | ✅ 独立跑出 **37 files / 455 passed**；净增 4（新文件 2 + r11 1 + r14 1） |
| 类型检查 | ✅ `npx tsc --noEmit`（`clients/app`）exit 0 |
| worktree 末端状态 | ✅ `git status --porcelain` 为空、`HEAD = 6e83070`、无 stash |

---

## 6. 非阻断观察（**不构成 finding**，不影响合入）

1. **r11 新用例的负向红法形态与报告措辞有偏差。** MUT-F/MUT-G 下该用例的**自身**失败是 `Test timed out in 5000ms`（因为生产在编码前不再排序 → `encodeNulJoinedUtf8` 抛 `field_order` → `verifyHostChallenge` 的 Promise 拒绝无人接管 → 不发 `client_proof` → `waitFor` 等不到），`TranscriptError … field_order` 只出现在 vitest 的 **Unhandled Rejection** 段。`deliver-tp3-r2.md` §6 写作「1 红（`field_order`）」略去了「红的是超时、`field_order` 是 unhandled rejection」这一层。**判别力不受影响**（变异必红、无假绿），且该超时是作者在用例注释里明写的预期失败路径，属**有意设计**；仅影响失败时的可读性（多 5s + 需要看 Unhandled 段）。我实测该 unhandled rejection 未波及同文件其余 10 条用例的通过/失败判定。
2. **`ALL_EVENT_KINDS` 本身没有编译期约束。** `ConnectionEvent` 新增种类时，两张**表**会被 tsc 拦住（§2.3 已证），但若维护者只补表、忘了把新种类加进 `ALL_EVENT_KINDS`（`:136-148`），断言循环仍不会覆盖它。今日 11/11 完整，**当前无缺口**。若要闭合，把循环改为遍历 `Object.keys(allowedSourceStates())` 的并集即可。
3. **`mayMarkInputAsSent` 仍是孤儿导出。** r1 建议「要么接线，要么登记为待接线」；TP3 的 Write Scope 只含 `clients/app/**/*.test.ts`，接线属实现改动、登记属 `tasks.md`，均越界，故作者只能以注释处置。该导出由 WP5b 引入，非本包问题。
4. **`r20-input-mark-production-path.test.ts` 未覆盖「已排队」这一半语义。** spec 的措辞是「MUST NOT 被标记为已发送**或已排队**」，而 `ConversationCapabilitiesModel` 只有 `canSend`、没有排队能力位，因此「已排队」在生产模型里无处可断——属模型层语义缺口（且不在 TP3 范围内），非本包可修。

---

## 7. 合入结论

**不阻断 MU3e 合入。** r1 的 2 条 MAJOR 均经我自选变异独立判定为**真修复**；5 条 MINOR/SUGGESTION 处置逐条成立（3 修 2 接受，其中 MINOR-2 的处置强度高于 r1 要求）；Write Scope 零实现改动、无断言削弱、无跳过标记；`tsc` 0、全量 455 全绿。§6 的 4 条观察均为非阻断，可在后续测试包顺手处理。
