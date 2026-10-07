# TP3 交付报告（r1）

- **包**：TP3（testing-3），测试设计 + 实现
- **worktree / 分支**：`.worktrees/tp3` / `feat/tp3`
- **基线**：`f6ea252e09fd65834a67fe7123c786beec339209`
- **交付提交**：`16ef844 test(frontend): TP3 固定向量跨语言对拍与契约/状态机覆盖`
- **用例总数**：359 → **451**（净增 92，8 个新文件）
- **Write Scope 自查**：全部 8 条路径以 `.test.ts` 结尾且位于 `clients/app/` 下；未改动任何实现文件（`src/**` 非测试文件、`app/**`、`scripts/**`、`package.json`、`vitest.config.ts` 一律未动）

---

## 0. 本包实际填补的空洞（为什么这些用例不是重复劳动）

进入本包前，全仓**没有一条用例**做到下面三件事：

| 空洞 | 证据 | 本包的处理 |
|---|---|---|
| 真实 `digestPort` 从未参与过任何快照校验 | `sync-client/testing/harness.ts` 的 `fakeDigest` 是 FNV-1a；`composition.test.ts` 的 `setup()` 注入 `ports: { digest: fakeDigest() }`；`review-wp7-r2 §7.3` 明确登记 | `r11-fixed-vectors.test.ts` 第 3 组 + `r14-rebuild-snapshot.test.ts`：经真实组合根，快照摘要由 **Node `crypto.createHash`（独立实现）**算出并被真实 SHA-256 端口接受；另有一条专门证伪「把 FNV 当期望值」 |
| 真实 `HostIdentityPort` 从未验过一支真实签名的字节 | `composition.test.ts` 的 `cacheBackedHost()` 里 `verifyHostChallenge` 直接 `return true` | `r11-fixed-vectors.test.ts` 第 2 组：用 `host-challenge.json` 里 **Rust 侧签的** `p1363Signature` 驱动真实 `PairedHostIdentity` |
| 「TS 与 Rust 编出同一串字节」从未被验证 | `platform/transcript.test.ts` 按 registry **自己重建**字段顺序再调 `encodeTranscript`，断言的是编码器对自己一致；生产代码的 `TAG → 字段选择与顺序` 无任何用例覆盖 | `r11-fixed-vectors.test.ts` 第 1 组：走**生产**握手路径取到被真实设备密钥签的字节，再与向量逐字节比对，并让向量自己的签名验过它 |

另外，连接迁移表**从未被穷举**（WP6 的用例是逐场景的），命令状态机与派发层的 `requestId` 复用从未在派发层判定，§9.3 的服务端模型在既有 `connection.test.ts` 里只有一种组合。

---

## 1. 需求逐条覆盖对照表

> 「覆盖」= 该 MUST 至少有一条新增用例断言它；行号是本包新增文件中的用例起始行。

### R11 设备身份由不可导出密钥承载

| MUST | 用例（文件:行） | 断言要点 |
|---|---|---|
| 设备证明 transcript 与 Rust **逐字节相同** | `r11-fixed-vectors.test.ts:502`、`r11-fixed-vectors.test.ts:520` | 十六进制逐字节比对 + `transcriptBase64url` + `transcriptSha256Hex` 三者同时相等 |
| 向量自带的 `p1363Signature`（Rust 签的）能验过生产字节 | `r11-fixed-vectors.test.ts:520` | 真 `crypto.subtle.verify` 返回 true；同条内翻一个 bit 后返回 false（证明该断言不是恒真） |
| 密钥不可导出（MUST NOT 以可导出形式落盘） | `r11-fixed-vectors.test.ts:545` | `importKey(..., extractable=false)` 后 `exportKey("pkcs8")` 被真实 `crypto.subtle` 拒绝 |
| 每次连接重新握手（不复用会话级凭据） | `r11-fixed-vectors.test.ts:574`（`同一向量换一个字节即被真实验签拒绝并进入 identity_changed`）+ `r14-rebuild-snapshot.test.ts` 每条用例各自跑一次完整握手 | 每次装配都重跑四步；验签失败即阻断且不发 `client_proof` |
| 来源变化后不再自动信任 | 由 `platform/secure-storage.test.ts`（WP5b）覆盖；本包不重复 | — |

### R12 连接状态是互斥状态机

| MUST | 用例（文件:行） | 断言要点 |
|---|---|---|
| 状态集合覆盖 12 态（8 常规 + 4 阻断） | `r12-connection-transitions.test.ts:184` | 与本文件独立枚举无差集 |
| **全部合法迁移与非法迁移**（12 × 11 = 132 条组合） | `r12-connection-transitions.test.ts:191`（合法来源表逐条相等）、`:198`（每条合法组合真的被接受）、`:210`（每条非法组合抛 `IllegalConnectionTransition` 且 `from`/`event` 字段正确） | 期望表在测试里**手写**，不复用生产的 `ALLOWED_SOURCES` |
| MUST NOT 以布尔组合表达状态 | `r12-connection-transitions.test.ts:249` | 每条合法迁移结果的键集恒为 `["blocking","caughtUp","state"]`，`state` 必须是字符串枚举 |
| 阻断态停止自动重连并说明原因与下一步 | `r12-connection-transitions.test.ts:288`、`:298`、`:317` | 四个阻断态 + 每个 `blocked` 事件产生的四个态逐一断言 `shouldAutoReconnect === false` 且 `reason`/`nextStep` 非空 |
| 追平期不谎称已在线 | `r12-connection-transitions.test.ts:277` | `caughtUp === (state === "online")` 对每条合法迁移成立 |
| 顶替后旧页面停止争抢 | `r12-connection-transitions.test.ts:329` | 四个阻断态都不接受 `connect_started`，且 `blocking_cleared` 只回到 `disconnected` |
| 离线/重连态输入 MUST NOT 被标记为已发送或已排队 | `r12-connection-transitions.test.ts:341`、`:352`、`:375` | 12 态中只有 `online` 可标记；完整路径在 `caught_up` 之前逐态断言为 false |

### R13 命令以稳定标识与终态收敛

| MUST | 用例（文件:行） | 断言要点 |
|---|---|---|
| 命令状态区分 7 态 | `r13-command-idempotency.test.ts:213` | 与本文件独立枚举一致 |
| MUST NOT 仅因断线就断定结果不确定 | `r13-command-idempotency.test.ts:222` | 对 3 个非终态 + 4 个终态逐一断言 `markConnectionLost` **返回同一对象**且 `serverConfirmedUncertain === false` |
| 只有服务端明确确认才呈现不确定 | `r13-command-idempotency.test.ts:242`、`:255` | 本地无任何入口能造出该终态；四条非 uncertain 结果都不打开不确定呈现 |
| MUST NOT 为同一意图生成新的请求标识 | `r13-command-idempotency.test.ts:270`、`:284`（状态机层）、`r13-dispatch-idempotency.test.ts:114`、`:141`（派发层）、`:158`（wire 层） | `requestId` 不变 + `dispatchCount` 递增 + 标识集合大小恒为 1 + 三次出站 wire 的 `requestId` 集合大小为 1 |
| 终态不被迟到结果改写 | `r13-command-idempotency.test.ts:301` | 4 个终态 × 5 个迟到状态 × 2 条入口，全部 `toBe(before)` |
| 未确认接受前不显示已接受 | `r13-command-idempotency.test.ts:335`、`:347` | 6 个非 accepted 状态在线/离线均不呈现；accepted 需在线且 `acceptedAt` 非空 |

### R14 事件按稳定标识去重且不跳过

| MUST | 用例（文件:行） | 断言要点 |
|---|---|---|
| 按 `eventId` 去重，重复不重复呈现 | `r14-replay-recovery.test.ts:328`、`:344`、`:358` | 重投 3 次呈现 1 次 + 2 次 `duplicate_dropped` + 只发 1 条 ACK；换 `messageId` 仍判重复 |
| 从最后确认位置连续恢复，MUST NOT 出现丢失区间 | `r14-replay-recovery.test.ts:229`（§9.3 建模，scope 过滤造成的空洞不算丢失）、`:262`（服务端漏投的可见事件被**补齐呈现一次**且序列仍严格不重不漏） | 呈现序列断言为 `["1","2","3","4","6","8","10","7"]`——不可见的 5/9 从未出现，漏投的 7 恰好出现一次 |
| 重复到达不得使已呈现内容错乱 | `r14-replay-recovery.test.ts:275` | 缺投事件到达后水位仍为 barrier 值（不因补齐而回退或虚进） |
| 缺口恢复有界 | `r14-replay-recovery.test.ts:305` | 连续漏投时 `subscribeCount` 有界且出现 `gap_resubscribe_exhausted` |
| 重建快照时丢弃未完成的暂存快照，MUST NOT 用未完成内容覆盖已完成状态 | `r14-rebuild-snapshot.test.ts:319`、`:373`、`:424` | 组合根 + 真实 `SnapshotStaging` + 真实 `ClientStore`；第一组在 `reset_required` 之后**再把被打断快照的剩余 chunk 补齐并按完整摘要收尾**，仓库仍必须是已完成那份 |

### R17 未上报的上下文不得以零冒充

| MUST | 用例（文件:行） | 断言要点 |
|---|---|---|
| 三态（已知 / 未上报 / 零窗口）可区分 | `r17-r19-store-contract.test.ts:88`、`:100`、`:113`、`:127` | 经 `ClientStore` 生产管线投影；`kind` 三者互不相同 |
| 未上报态不以零冒充 | `r17-r19-store-contract.test.ts:88` | 渲染结果里去掉 `data-context-state` 后**不含任何阿拉伯数字**，且不含 `0%`/`NaN`/`Infinity` |
| 窗口为零时同样视为未上报、不算占比 | `r17-r19-store-contract.test.ts:100` | `kind === "zero_window"`、无 `data-context-percent`、无 `NaN`/`Infinity` |
| MUST NOT 自行推算 | `r17-r19-store-contract.test.ts:169` | 仓库 `usage` 映射里根本没有该键——「从未收到」由缺席表达 |
| 未上报不禁用其它交互 | `r17-r19-store-contract.test.ts:140` | 三态下 `capabilities` 完全相等 |

### R19 Agent 目录与连接状态分两层呈现

| MUST | 用例（文件:行） | 断言要点 |
|---|---|---|
| 覆盖层缺席呈现「状态未知」而非「已断开」 | `r17-r19-store-contract.test.ts:187`、`:203` | 逐行 `connection === "unknown"`；逐行 `connectionLabel === "状态未知"` |
| MUST NOT 因覆盖层缺席而移除 Agent | `r17-r19-store-contract.test.ts:187`、`:342` | 条目数恒为 2；断连时面板仍列出 2 行 |
| MUST NOT 以会话活跃度推断连接状态 | `r17-r19-store-contract.test.ts:219` | 会话 `state: "running"` 之后 overlay 仍为空 |
| 同名 Agent 以标识区分 | `r17-r19-store-contract.test.ts:243` | 两个 `displayName: "codex"` 的条目各自保留独立状态，互不覆盖 |
| 断连时仍列出已配置 Agent | `r17-r19-store-contract.test.ts:342` | `connectionState === "reconnecting"` 时面板仍有 2 行 + 离线快照说明 |

### R20 imported 资源不落盘且离线只显示元数据

| MUST | 用例（文件:行） | 断言要点 |
|---|---|---|
| 七类内容（正文/提示/回复/工具内容/差异/终端输出/附件/ACP 原文）MUST NOT 写入任何持久浏览器存储 | `r20-no-content-cache.test.ts:63`、`:73`、`:84`、`:104` | 走**真实 `openLocalCache()`** 的 IndexedDB 事务管线，并用 `__fakeIndexedDBDump()` **直接翻查存储全文**确认正文原文一个字节都不在 |
| 来源不可达时只显示元数据 | `r20-no-content-cache.test.ts:84` | 落盘的只有允许清单内的元数据；正文只在内存载体，`clear()` 后消失 |
| MUST NOT 把用户输入标记为已发送或已排队 | `r20-no-content-cache.test.ts:126`、`:131` | imported 来源即便 `online` 也不可标记 |
| 摘要缓存按固定容量 + LRU 淘汰 | `r20-no-content-cache.test.ts:159`、`r20-no-content-cache.test.ts:170` | 三条 `updatedAtMs` **完全相同**（写入顺序与 LRU 序在写入阶段不可区分），读一次 `a` 后写第四条：LRU 淘汰 `b`、FIFO 会淘汰 `a` |
| 容量与存活期是固定值，MUST NOT 作为可放宽的配置项 | `r20-no-content-cache.test.ts:148`、`r20-no-content-cache.test.ts:159` | 常量逐值断言 + `openLocalCache.length === 0`（无入参即无配置入口） |

---

## 2. 变异验证表

方法：临时改坏实现 → 跑对应新增测试文件 → 记录红/绿 → **立即还原**。15 项全部被捕获。还原后 `git status --porcelain` 为空。

| # | 变异内容 | 目标文件 | 结果 | 捕获者（用例） |
|---|---|---|---|---|
| 1 | `connection.ts` device proof 的 `deviceId` 字段 tag 由 3 改成 11（与 connectionId 换位） | `src/sync-client/connection.ts` | 🔴 6 红 / 4 绿 | `r11` 第 1、2 组 + 2 条 tamper 用例 + 全部 3 条 digest 用例（真实验签失败 → 握手阻断） |
| 2 | `composition.ts` `digestPort` 输出前 4 字节异或 `0xA5`（等价于换算法） | `src/composition.ts` | 🔴 2 红 | `r11` 的两条「真实 digestPort 参与校验」 |
| 3 | `composition.ts` host challenge 的 `canonicalOrigin` 字段 tag 由 6 改成 7 | `src/composition.ts` | 🔴 6 红 / 4 绿 | 同 #1 |
| 4 | `composition.ts` **完全跳过** ECDSA 验签（有记录即通过） | `src/composition.ts` | 🔴 1 红（`同一向量换一个字节即被真实验签拒绝并进入 identity_changed`） | 正是为它设计的负向用例 |
| 5 | `connection-machine.ts` 让阻断态 `revoked` 也接受 `connect_started` | `src/state/connection-machine.ts` | 🔴 3 红 | `r12` 的「期望表逐条相等」「非法组合逐条抛错」「阻断态不得直接 connect_started」 |
| 6 | `connection-machine.ts` `canMarkInputAsSent` 在 `replaying` 也返回 true | `src/state/connection-machine.ts` | 🔴 2 红 | `r12` 的「只有 online 允许标记」「迁移到 online 之前始终不可标记」 |
| 7 | `command-machine.ts` `retrySameRequest` 为重试生成新的 `crypto.randomUUID()` | `src/state/command-machine.ts` | 🔴 10 红 / 10 绿 | `r13` 的 ID 复用、终态不可改写、记录形状等 |
| 8 | `command-machine.ts` `markConnectionLost` 在 `submitting` 时自行判为 `uncertain` | `src/state/command-machine.ts` | 🔴 1 红 | `r13` 的「断线不得自行断定结果不确定（7 态穷举）」 |
| 9 | `dedupe.ts` 去重键由 `eventId` 换成 `globalSequence` | `src/sync-client/dedupe.ts` | 🔴 全量红，但**均在既有测试** | 既有 `dedupe.test.ts`；本包新增的 `r14-replay-recovery.test.ts` 在该变异下 **7/7 全绿**（订正见下） |
| 10 | `snapshot.ts` `begin()` 不再丢弃被取代的暂存区 | `src/sync-client/snapshot.ts` | 🔴 全量红，但**均在既有测试** | 既有 `snapshot.test.ts`；本包新增的 `r14-rebuild-snapshot.test.ts` 在该变异下 **3/3 全绿**（订正见下） |
| 11 | `domain/conversation.ts` 窗口为零时返回 `known` 且 `percent: 0`（零冒充） | `src/domain/conversation.ts` | 🔴 3 红 | `r17` 的三态与不推算用例 |
| 12 | `domain/agent-catalog.ts` 覆盖层缺席时默认 `disconnected` | `src/domain/agent-catalog.ts` | 🔴 4 红（连带 R17 用例） | `r19` 的覆盖层缺席与不推断用例 |
| 13 | `connection.ts` `#onResetRequired` **不调用** `discardStaging()` | `src/sync-client/connection.ts` | 🔴 1 红 | `r14-rebuild-snapshot` 第一组（该用例在 `reset_required` 之后补齐被打断快照的剩余 chunk 并按完整摘要收尾——没有丢弃暂存区时它会通过校验并**覆盖**已完成状态） |
| 14 | `local-cache.ts` 淘汰排序由 `lastUsedAtMs` 改为 `updatedAtMs`（退化为 FIFO） | `src/platform/local-cache.ts` | 🔴 1 红 | `r20` 的 LRU 用例（三条 `updatedAtMs` 相同，唯一差别来自那次 `get`） |
| 15 | `connection.ts` 去掉缺口恢复的次数上限 | `src/sync-client/connection.ts` | 🔴 1 红 | `r14` 的「缺口恢复有次数上限」 |

> **订正（r2，`reports/review-tp3-r1.md` MINOR-1）**：上表第 9、10 行的捕获者原写作「`r14` 的三条 eventId 去重用例」与「由 #13 一并覆盖」，与实测不符——该两项变异下本包新增文件（`r14-replay-recovery.test.ts` 7/7、`r14-rebuild-snapshot.test.ts` 3/3）**全绿**，实际捕获者是既有 `dedupe.test.ts` 与 `snapshot.test.ts`。第 9 行的红数也因此改为「全量红，但均在既有测试」。**无覆盖缺口**（两项都有既有测试守着），但「同源覆盖更强」的印象是错的。r2 已在 `r14-replay-recovery.test.ts` 补一条「同序号但 `eventId` 不同判 `late`」的用例，把「去重键不是序号」这一半收进本测试包；详见 `reports/deliver-tp3-r2.md` §5 MINOR-1。

**两轮返工记录**（变异没被抓住 → 先改测试再重跑变异）：

- #13 第一版：只在 `reset_required` 后断言「仓库没变」。实测**变异后仍全绿**——因为未完成的暂存区会被下一份 `snapshot_begin` 的 `supersede` 顺手清掉，观察不到差别。改为在 `reset_required` 之后**主动补齐被打断快照的剩余 chunk 并按完整三 chunk 的摘要收尾**，此时「暂存区是否被丢弃」成为可观测事实，重跑变异变红。
- #14 第一版：三条写入用互不相同的 `nowMs`，于是 `updatedAtMs` 序与 `lastUsedAtMs` 序恰好相同，FIFO 与 LRU 无法区分。改为三条 `updatedAtMs` **完全相同**、只用一次 `get` 抬 `a` 的 `lastUsedAtMs`，重跑变异变红。

---

## 3. 发现的实现缺陷

**无。** 本包未发现任何 CRITICAL / MAJOR / MINOR 级实现缺陷，因此没有需要「保持为红」的回归用例。

变异验证过程中曾出现两次「变异未被捕获」，两次均判定为**本包用例的判别力不足**而非实现缺陷，已按上节记录修正测试后重跑通过。

两处值得下游留意但**不构成缺陷**的观察（均为既有实现的显式设计，已在源码注释中登记）：

1. `SyncClient.resumeCursor` 在 `composition.ts:711` 被固定传 `null`，注释明确「首次装配从无游标开始」。跨进程持久化恢复点被登记为独立工作，不是 R14 的违约（单进程内重连的连续恢复由 `#ackedCursor` 承接，本包 `r14-replay-recovery` 已覆盖）。
2. 真实 `crypto.subtle` 的 promise 在 Node 里需要一次宏任务才 resolve，`sync-client/testing/harness.ts` 的 `waitUntil` 只轮转微任务。凡经真实 WebCrypto 的用例必须自带宏任务等待器（本包三处：`r11`、`r14-rebuild-snapshot`）。这是**测试基础设施的既有局限**，不是生产缺陷。

---

## 4. 未验证项

1. **`npm run check` 的第十道门禁 `check:agentic` 未通过**——worktree 根没有 `node_modules`，脚本自述需先在仓库根执行 `npm ci`，而本包指令明确禁止该操作。前九道门禁（schemas / commands / errors / features / assets / acp / docs / boundaries / drift）全部通过。**未以任何方式绕过**。需在具备根 `node_modules` 的环境复跑。
2. **真实浏览器内的私钥结构化克隆保真**未由本包验证——`run-browser-check.mjs` 的 16 条断言仍是原有内容、本包未新增浏览器断言，浏览器侧证据属 TP4。
3. `crates/identity-auth`、`crates/sync-protocol`、`crates/acpr-transcript` 的 Rust 侧未重新编译或跑测试；本包只**消费**其产出的冻结向量，未改动它们。
4. 真实 WebSocket / 真实 WebCrypto 在**浏览器内核**中的行为仍只由既有的 16 条浏览器断言覆盖；本包全部对拍在 Node 22 的 WebCrypto 上完成（与浏览器同源实现，但不是同一进程）。

---

## 5. 门禁退出码

| 门禁 | 命令 | 退出码 |
|---|---|---|
| 类型检查 | `npx tsc --noEmit`（`clients/app`） | **0** |
| 单元测试 | `npx vitest run`（`clients/app`） | **0**（451/451） |
| 浏览器断言 | `node scripts/run-browser-check.mjs` | **0**（16/16） |
| 仓库门禁 | `npm run check`（仓库根） | **1** — 前 9 道绿，第 10 道 `check:agentic` 因缺根 `node_modules` 未运行（见「未验证项」1） |