# WP7 交付检视报告 r1（review-7，独立只读）

- **被检视对象**：`D:\Project\acp-remote\.worktrees\wp7` @ `221e76e`（基线 `4ada2a24e23ff3f07d37eb0face70921a66ed987`）
- **检视方式**：只读。未修改 `feat/wp7` 上任何文件；所有变异/探针均已回退还原，最终 `git status --porcelain` 为空。
- **作者报告**：`reports/deliver-wp7-r1.md`

---

## 1. 结论

**FAIL —— 阻断合入。**

| 级别 | 数量 |
| --- | --- |
| CRITICAL | 1 |
| MAJOR | 2 |
| MINOR | 4 |
| SUGGESTION | 3 |

CRITICAL-1 是本轮 P1 修复**自身引入的回归**：门面的 `#isAdoptableBarrier` 在服务端 epoch 变化后会把**合法的新 epoch 快照永久丢弃**，快照仓库停留在重建前的旧状态。这条失效链与被修复的 P1-N1/N2 同型（永久卡死），只是触发条件换成了「服务端事件库重建」这条 `SYNC_PROTOCOL.md` §9.4 明列的正常路径。

MAJOR-1/2 都在 R18 的生产路径上：R18 的全部用例都直调 `buildDegradedEventModel`，从未经过 `ClientStore` 的事件管线，因此两处生产缺陷带着 338/338 全绿通过。

作者自报的三项缺口：(a) 的**技术论断成立**，但「阶段边界」的定性不成立（见 §5.1）；(b) 成立；(c) 成立且无掩盖。

---

## 2. 门禁复现（我自己跑的）

| 检查 | 命令 | 结果 |
| --- | --- | --- |
| 类型检查 | `clients/app: npx tsc --noEmit` | exit 0，无诊断 ✅ |
| 单元/契约 | `clients/app: npx vitest run` | **25 文件 / 338 passed** ✅ 与作者自报一致 |
| 仓库根门禁 | `npm run check` | exit 0，agentic `21 passed, 0 failed` ✅ |

`cargo fmt --check` 未跑（本包零 Rust 改动，与作者一致）。

---

## 3. CRITICAL-1：epoch 变化后合法快照被门面永久丢弃（本轮引入的回归）

### 位置

`clients/app/src/sync-client/client.ts:270-276`（`#isAdoptableBarrier`），由 `client.ts:281-285` 调用。

### 现象

`#isAdoptableBarrier` 以门面的 `#ackedCursor` 为基准判断「同 epoch」。但 `#ackedCursor` 在 epoch 变化时**不会被清空**——清空只发生在连接层的 `#acked`（`connection.ts:442-445`）与门面的 `#ledgerFor`（`client.ts:241-251`）里，两者都不写 `#ackedCursor`。于是：

1. 客户端在 epoch A 下 `#ackedCursor = {A, "10"}`；
2. 服务端事件库重建，重连后 `auth.authenticated` 带 epoch B。连接层正确地把 `#acked`/`#lastAcked` 清成 `null`，门面的 `#ledgerFor` 也用 `resumeFrom = null` 建了新账本；
3. 服务端按 §9.4 补发快照 `snapshot_end`，cursor 为 `{B, "1"}`；
4. **连接层判定通过**（新账本，`advanceTo` 接受），发出 `snapshot_verified`；
5. 门面 `#isAdoptableBarrier({B,"1"})` 看到 `#ackedCursor.serverEpoch (A) !== B` → 返回 `false` → **快照既不替换仓库也不推进恢复点**；
6. 后续增量事件会走 `client.ts:288` 把 `#ackedCursor` 抬到 B，但**已经来不及**——重建快照是那一次性的，仓库此后永久停留在 epoch A 的旧资源上，直到下一次快照请求（而服务端不会主动再发）。

### 影响

违反 R14「服务端要求重建快照时丢弃未完成的暂存快照并重新请求」的结果语义：重建请求发出了、重建快照验证通过了，却从未进入仓库。UI 会持续呈现 epoch A 的目录/会话/Agent 列表，而事件流已经在 epoch B 上继续推进——**列表与事件永久不一致，且无任何错误呈现**。这条路径在 `SYNC_PROTOCOL.md:633`（数据库重建 / 不可兼容恢复 / 事件历史身份改变时生成新 epoch）与 `:706`（`reset_required` 的 `reason: epoch_mismatch`）里是**正常恢复流程**，不是异常。

### 复现（我实测，两次探针，均已删除）

探针 A（`src/sync-client/zz-probe.test.ts`，跑完删除）：
```
PROBE 连接层结论: [{k:"snapshot_verified", c:{serverEpoch:"00384a03-...", globalSequence:"10"}},
                   {k:"snapshot_verified", c:{serverEpoch:"7f11b7b1-...", globalSequence:"1"}}]
PROBE 仓库 snapshotId: 8194de43-...  期望: 5b0d4c1e-...
```
→ 连接层**交付**了新 epoch 快照，门面**丢弃**了它。

探针 B（`zz-probe2.test.ts`，追加新 epoch 增量事件后复查，跑完删除）：
```
PROBE2 仓库 snapshotId: 8194de43-...  期望: 5b0d4c1e-...
PROBE2 仓库里的标题: "重建前的标题"
PROBE2 ackedCursor: {"serverEpoch":"7f11b7b1-...", "globalSequence":"2"}
```
→ 后续事件把 `#ackedCursor` 推到新 epoch，但仓库仍是旧快照。**事件救不回快照**，与被修的 P1 同型。

**回归确认**：把 `client.ts` 单独回退到基线 `4ada2a2`（保留 `connection.ts` 的修复），同一个探针立即 **通过**——仓库拿到新快照、`ackedCursor` 为 `{B, "1"}`。所以这不是既有缺陷，是 `221e76e` 引入的。

### 建议处置

`#isAdoptableBarrier` 的基准必须与账本一致。最小改动：在 `auth.authenticated` 事件上把 `#ackedCursor` 按 epoch 判定清空（与 `connection.ts:442-445` 同一口径），或让 `#isAdoptableBarrier` 改为对照 `this.#ledger?.serverEpoch` 而非 `#ackedCursor.serverEpoch`。同时补一条 epoch 变化 → 新快照入仓的正向用例。

### 为什么作者的两处判定「同结论」不成立

作者在 `client.ts:266-268` 写「两处判定必须同结论，否则既不替换快照仓库也不移动恢复点」。CRITICAL-1 正是**分歧本身**：连接层判「采纳」、门面判「拒绝」。作者的两条拒绝屏障用例（外来 epoch、越界序号）都发生在**账本已拒绝**的路径上，连接层根本没交付，所以从未触发分歧。要暴露分歧，必须让连接层接受、门面拒绝——即 epoch 变化。

---

## 4. MAJOR

### MAJOR-1：`raw_not_synced` 分支在生产管线中不可达（R18 MUST 被违反）

- **位置**：`clients/app/src/features/client-store.ts:175` — `raw: { kind: "absent" }` 硬编码。
- **现象**：`degradedModelFor` 调 `isDegraded(...)` 时把 `raw` 写死成 `{kind:"absent"}`，而 `isDegraded` 的第二个判据正是 `input.raw.kind === "not_synced"`（`degradation-model.ts:154`）。于是：**事件类型已知（`isKnownEventType` 为真）+ 原文因单条上限/保留期未下发**，两个条件同时成立时 `isDegraded` 返回 `false` → 不产生降级卡片。
- **影响**：违反 R18 MUST「对因单条上限或保留期等原因未随事件下发的原文，客户端 MUST 呈现明确的未同步说明」。用户看到的是**静默丢失**——既没有卡片，也没有「原文未同步」的说明，正是 R18 要禁止的行为。`buildDegradedEventModel` 的 `raw_not_synced` 分支（`degradation-model.ts:190-191`）在生产路径上不可达。
- **复现**（实测探针，已删除）：向 store 注入 `file.changed` + `rawUnavailable:{reason:"size_limit"}`：
  ```
  PROBE3 降级条目数: 0
  PROBE3 HTML 含 raw_not_synced: false  含原文: false
  ```
- **为什么测试全绿**：`r18-degradation.test.ts` 的全部用例都直调 `buildDegradedEventModel(...)`，**从不经过 `ClientStore`**。`buildDegradedEventModel` 自己会正确读 `event.body.payload.acp`（`degradation-model.ts:182`），所以它是对的；错的是上游的**筛选**。判别力缺口：R18 的「原文未下发时不渲染占位」只在抽屉渲染层被验证，从未在「原文未下发 → 是否产生降级呈现」这一层被验证。
- **建议**：`degradedModelFor` 传 `rawAvailability(event.body.payload.acp)` 而不是硬编码 `{kind:"absent"}`；补一条经 `store.apply({kind:"event"})` 的端到端用例。

### MAJOR-2：二进制未下发的降级卡片谎称「客户端不支持」（R18 三类可区分被污染）

- **位置**：`clients/app/src/components/ConversationStream.tsx:36` + `clients/app/src/components/DegradedEventCard.tsx:41,43`。
- **现象**：`ConversationStream.tsx:36` 写 `const unsupportedBy = model.reason.kind === "unsupported" ? model.reason.by : "client"`——把**所有非 unsupported 原因**兜底成 `"client"`。`DegradedEventCard.tsx:41` 又对**所有原因**无条件输出 `data-unsupported-by={unsupportedBy}`，`:43` 无条件输出 `当前客户端未提供专用视图`。
- **影响**：一条「事件已收到、图片没下发」的事件，卡片上写着「当前客户端未提供专用视图 / 客户端不支持」。这**正是 R18 禁止的**「合并为一句笼统的不可用」的镜像版本：把「内容未下发」误报成「客户端能力不支持」，三类原因在卡片这一层被污染成两类。R18 MUST NOT 把结构化事件降级成普通文本呈现——这条错误归因让用户按错误原因去排查。
- **复现**（实测探针，已删除）：向 store 注入 `agent.message.completed` + `image_ref/not_fetched`：
  ```html
  <div data-degraded-event="agent.message.completed" data-unsupported-by="client">
    <strong>agent.message.completed</strong>
    <span data-degraded-view="no-dedicated-view">当前客户端未提供专用视图</span>
    <span data-unsupported-reason="client"></span>
  </div>
  <p data-binary-not-delivered="image/png">结构化事件已收到，但它引用的二进制内容没有下发到本设备。</p>
  ```
  同一张卡片上既有「客户端不支持」又有「内容未下发」，自相矛盾。
- **为什么测试全绿**：`r18-degradation.test.ts` 的三类断言全部走 `renderDrawer`（`DiagnosticsDrawer`），而 `DiagnosticsDrawer.tsx:57-61` 按 `reason.kind` 分派、**不经过** `DegradedEventCard`。降级卡片这一路只在 `r18:141-149` 被测了一条 `unsupported` 正例，从未测 `binary_not_delivered` / `raw_not_synced` 走卡片时的归因。
- **建议**：`unsupportedBy` 改为 `reason.kind === "unsupported" ? reason.by : null`，`DegradedEventCard` 在 `unsupportedBy === null` 时不输出该属性与「未提供专用视图」文案；补卡片层的非 unsupported 用例。

---

## 5. 作者自报的三项缺口 —— 独立定性

### 5.1 组合根无归属：**技术论断成立，定性不成立 —— 判为设计缺口（MAJOR 级流程问题）**

**技术论断核实：成立。** 我实测 `layerOfSourcePath`（`src/zz-layers.test.ts` 探针，已删除）：
```
src/composition.ts -> null
src/app-wiring.ts  -> null
src/platform       -> platform
```
`layers.test.ts:170,178-181` 确实对**七层中每一层**（含 `app`）把 `PLATFORM_LAYER` 判为违规。作者说的「组合根在 WP7 的 Write Scope 内无处安放」**正确**。

**但作者据此得出的定性错了。** 作者写「建议：新增一个层级中立文件……两者都需要扩 Write Scope，故本轮未自行创建」。这就把责任推给了 Write Scope，实际问题是另一件事：

- `layerOfSourcePath("src/composition.ts") === null` 意味着 `src/composition.ts` **能** import `src/platform/`，且 `layers.test.ts:170` 的 `from === null` 分支会跳过它。**技术上完全可行，不需要改 `layers.ts` 或 `layers.test.ts`。**
- 唯一的障碍是 `plan.md:866` 的 WP7 Write Scope 字面不含 `clients/app/src/*.ts`（只含 `app/`、`src/features/`、`src/components/`）。这是**计划登记的收窄**，不是分层约束。
- 我核对了全仓：`grep -rn "组合根|openPlatform" plan.md tasks.md` 在本 change 内**零命中**（`openPlatform` 只在 `src/platform/index.ts:67` 有实现，`plan.md`/`tasks.md` 从未提及它）。也就是说：**整个 plan 里没有任何工作包负责组合根**——WP5b 只写 `src/platform/`，WP6 只写 `src/sync-client/`+`src/state/`，WP7 只写三层，TP3/TP4 只写测试。**这是一个计划级遗漏，不是 WP7 的越界，也不是可接受的阶段边界。**

**分量判断：这是设计缺口，不是阶段边界。** 理由：
1. 「阶段边界」意味着后续工作包会补。`tasks.md` 4.10（TP3）与 4.13（TP4）都只写 `*.test.ts` 与 `e2e/`，**都不含组合根**。MU3e/final 阶段没有任何一个工作包会补它。
2. 后果不是「延迟」而是**产品不可用**：`_layout.tsx:25` 写死 `store={null}`，所有页面渲染 `RuntimeUnavailable`。R15/R16/R18/R19 的页面在真实运行时**一条都跑不到**。这直接使 TP4（`tasks.md` 4.14，浏览器验证 R15/R16/R18 的可观察行为）**无法执行**——浏览器里只会看到 RuntimeUnavailable。
3. 作者的处理（渲染 `RuntimeUnavailable` 而非空目录）**选择正确**——伪造空数据会违反 R15 第一种空态的语义。这点值得肯定。但正确的处理不能替代缺失的归属。

**建议处置**：main 裁决新增一个工作包（或扩 WP7 的 Write Scope 至 `clients/app/src/*.ts`），把 `openPlatform()` 调用 + 五个端口实现 + `PairingTransport` HTTPS + `ConnectionIdentity` 取值一次性落地。`SyncClient` 未暴露 `connectionId`（`client.ts:254` 是私有 `#connection?.connectionId`），组合根需要 `SyncClient` 补一个公开只读 getter，否则 `client-store.ts:565` 的第一道闸门恒为「尚未建立已认证连接」，**创建会话永远发不出去**——这条应一并登记为前置。

### 5.2 测试文件归属冲突：成立

`plan.md:930` 明确记「`clients/app/**/*.test.ts`：WP7 只写实现文件，测试文件由 TP3 独占」（Merge Order 已是 WP7 → TP3）。本轮新增 7 个 `.test.ts`：

```
src/components/r15-directory-list.test.ts
src/components/r16-create-session.test.ts
src/components/r17-context-usage.test.ts
src/components/r18-degradation.test.ts
src/components/r19-host-panel.test.ts
src/features/r15-directory-model.test.ts
src/features/conversation-paging.test.ts
src/sync-client/snapshot-barrier.test.ts
```

全部落在 `clients/app/**/*.test.ts` 字面范围内，与 TP3 登记**字面重叠**。作者的选择（在 Write Scope 内的 `src/features/`+`src/components/` 里放，未碰 `src/protocol/`、`src/domain/`、`src/state/`、`src/platform/`）是最小冲突面，且 Merge Order 已为「WP7 → TP3」预置了合并次序。

**判定：可接受的阶段边界，非缺陷。** 登记冲突是既有的、由计划预先裁定的，且合并次序已排好。

**最小处置建议**：把这 8 个文件登记进 `Shared File Ownership` 的 Region Note，把「WP7 只写实现文件」更新为「WP7 写 R15–R19 的负向用例，TP3 在其上追加状态机与契约用例」；`src/features/testing/fixtures.ts` 明确为 WP7→TP3 的移交件（它不 import 任何其它测试文件，可直接移交）。`snapshot-barrier.test.ts` 属已授权例外目录，留在原处。

### 5.3 `react-dom/server` 手写声明：成立，无掩盖

- `vitest.config.ts:12` 的 `include: ["src/**/*.test.ts"]` 确实只收 `.ts`；`package.json` 不在 WP7 Write Scope，确实不能加依赖。两项事实成立。
- **声明与实际形状一致**：我实测 `require('react-dom/server')` 导出为 `{version, renderToString, renderToStaticMarkup, renderToPipeableStream}`；`react-dom-server.d.ts:11` 声明 `renderToStaticMarkup(element: unknown): string`，是实际签名的**收窄**（真实签名是 `ReactNode`），对调用方更宽松而非更严。**没有掩盖**，`tsc` 通过对应真实运行时行为。
- **断言确实执行了 React 渲染**：所有组件断言走 `renderToStaticMarkup(createElement(Comp, props))`，断言对象是真实 HTML 字符串（如 CRITICAL/MAJOR 节的复现输出就是渲染产物），不是纯函数返回值。判别力成立。

**判定：非缺陷。** 代价是组件渲染测试只在 Node 的静态渲染下成立（无事件、无 `useEffect`），浏览器级证据归 TP4——作者在 §7 已如实登记。

---

## 6. R15–R19 逐条核对

### R15 目录页

| MUST / MUST NOT | 证据 | 判别力 |
| --- | --- | --- |
| 目录项只显示展示名与别名 | `r15-directory-model.test.ts:135-137`（模型键恰为 `["alias","counts","displayName"]`）+ `r15-directory-list.test.ts:57-65`（断言 **HTML** 无 `D:\work`/`path=`/`root=`） | **真**。夹具确实挂了 `path`/`root`/`cwd`（`r15-directory-model.test.ts:36-40`，用 `as unknown as WorkspaceRef[]` 绕过类型），路径若被透传必现红。断言查的是渲染后 HTML，非模型键 |
| 不显示/编辑/推导本机路径 | `DirectoryRowModel`（`directory-model.ts:33-37`）只有三个字段，无 id/父目录/末段槽位 | **真（结构性）**。R15 攻击点已封死 |
| 同名目录 MUST NOT 用路径消歧 | `r15-directory-list.test.ts:67-82`：两个 `displayName: "api"`，断言 `match(/data-directory-display-name="api"/g)` 长度 **2** + HTML 无 `\`、无盘符正则 | **真**。是真同名（不是碰巧不重名）；按名去重会变 1，红 |
| 汇总由会话摘要得出，断连可呈现 | `r15-directory-model.test.ts:161-172`：断连时 `counts` 恰为 `{total:2, active:1, pending:1}` | **真**。给 0 或不给都会红 |
| 数据来自快照，MUST NOT 依赖在线查询 | `r15-directory-model.test.ts:186-197`：注入记账网关，走完 `directoryPage()`/`directoryDetail()`/`hostPanel()` 后 `dispatches` 为 `[]`；对照 `:199-224` 证明派发通道通 | **真**。`ports.ts` 的窄接口（`SnapshotGateway` 只读 `current`，无查询方法）使在线查询**无表达方式** |
| 两种空态可区分 | `r15-directory-list.test.ts:127-155`：`no_directories` 断言**不**含另一态的 guidance；`:157-164` `resources: null` → loading 且不含 `data-empty-state` | **真**。共用一句话会红 |

**R15 结论：全部守住，判别力真实。** 路径攻击我额外核过：`grep -rniE "path|root|cwd" features/*-model.ts components/*.tsx` 在实现侧零命中（仅注释里解释为何没有），无 aria-label / title / data-* 泄露路径的路径。

### R16 会话创建

| MUST / MUST NOT | 证据 | 判别力 |
| --- | --- | --- |
| 载荷只含两个引用 | `r16-create-session.test.ts:56-72`：断言 `Object.keys(payload).sort()` **恰为**两键 + `JSON.stringify(command)` 无 `D:\` / `/home/` / `token|password|apiKey|mcp|cwd|exportId|templateParams` | **真**。查的是**序列化后的字节**，不是模型对象；放宽成「至少两键」会红 |
| 越界载荷被拒 | `:74-98`：6 组逐个测（`cwd`/`path`/`exportId`/`mcpServers`/`token`/`templateParams`），每组 `expect(...).toThrow(/越界/)`；`:96-99` 对照合法载荷仍通过 | **真**，且有「非一律拒绝」对照 |
| 候选只来自已配置 Agent | `:113-127`：提交未登记 Agent → `ok:false` 且 `gateway.dispatches` 为 `[]`（真的没发命令） | **真** |
| 无权限时入口不可用**且说明原因** | `:133-146`：`data-create-permission="denied"` + `disabled` + 含「电脑端」；`:148-157` 对照有权限时按钮可用；`:159-168` 未认证理由（「尚未连接」）与无权限可区分 | **真** |
| 创建中不接受重复提交 | `:180-199`：`data-create-submission="creating"` + 正则 `data-action="submit-create"[^>]*disabled` | **真** |
| 终态分别呈现 | `:201-256`：`created`+`sessionId` / `failed`+码+可重试+消息 | **真** |
| 结果不确定不被自动消解 | `:266-327`：再来无关事件（`globalSequence:"11"`）后 `uncertainRequestIds` 仍含该 id；只有 `dismissUncertain` 才移除 | **真** |

**R16 结论：全部守住。**

### R17 上下文

| MUST / MUST NOT | 证据 | 判别力 |
| --- | --- | --- |
| 未上报 MUST NOT 以零冒充 | `r17-context-usage.test.ts:52-62`：断言 HTML **不含** `data-context-used`/`data-context-size`/`0%`/`>\s*0\s*<` | **真**，直接针对数字槽位 |
| 零窗口同样视为未上报、不得算出占比 | `:64-77`：断言无 `NaN`/`Infinity`/`0%`/数字槽位 | **真** |
| 三态可区分 | `:79-98`：已知态出 `25.0%` + 两个数字（对照）；`:100-110` 三种 `data-context-state` 去重为 3 | **真**。三态是 `buildContextUsageView` 的判别联合取值，`ContextUsageBar.tsx` 三分支。去重计数的局限：若三态共用一个 `data-context-state` 但文案不同会漏——但同用例还断言了各自的数字/文案，合并即红 |
| 未上报不阻断其它交互 | `:118-134`、`:136-148` | **弱**。CAPABILITIES 常量被**注入** `buildConversationModel` 后再断言输出等于输入（该函数只做透传）——这是同义反复。真正的产生点 `conversationCapabilities`（`client-store.ts:473-479`）**不看 usage**，结构上满足 MUST，但 `canCancel`/`canViewHistory` 未在真实函数上被差分断言。`:150-169` 的 R20 对照也没把连接推到 online，`canSend=false` 是必然，对照组无效 |
| 「已上报」分支端到端 | — | **缺口**。全仓无测试注入 `session.usage.changed`。我用探针实测 store 管线**能**正确归并（`{"kind":"known","used":"30000","size":"120000","percent":25}`），故非缺陷，但 TP3 应补 |

**R17 结论：三条 MUST NOT 守住；「未上报不阻断其余交互」的用例判别力不足（MINOR-1），但 MUST 本身因 `conversationCapabilities` 不读 usage 而成立。**

### R18 降级

| MUST / MUST NOT | 证据 | 判别力 |
| --- | --- | --- |
| 原文未下发时不渲染占位 | `r18-degradation.test.ts:74-100` | **中**。断言 `not.toContain('data-raw-json="available"')` + `not.toContain("{}")` + `not.toContain("undefined")`。**只禁三种字面量**：空串、`（无）`、`…`、`N/A`、`data-raw-json="placeholder"` 全部滑过。`"{}"` 这条之所以绿，是因为 `DiagnosticsDrawer.tsx:84-86` 无条件渲染 `JSON.stringify(model.view, null, 2)` 而夹具给了非空 view（`fixtures.ts` `eventMessage` 缺省 `view = {}`）——断言锁的是夹具形状。若 `:172` 那条改调 `renderDrawer`，输出会是字面 `{}` 并把 `:95` 打红 |
| 三类不支持可区分 | `r18-degradation.test.ts:118-137` | **中**。三段 HTML 去重为 3 —— 但即使三个文案改成同一句，三段 HTML 仍因 `data-unsupported-by` 属性与逐例 `detail` 不同而互不相同，去重断言照样绿。真正能抓的只有 `:134` 对 `UNSUPPORTED_LABELS` 常量表的自检。若把 `DiagnosticsDrawer.tsx:60` 改成忽略 `UNSUPPORTED_LABELS` 统一渲染一句笼统话，`:132` 与 `:134` **都绿**而 R18 被违反 |
| 未识别事件保留原文可查 | `:145-165` | **真** |
| 二进制未下发的说明 | `:174-190`：断言含「结构化事件已收到」+ 无 `<img` | **真**，但只测抽屉；卡片路径见 MAJOR-2 |
| 未知事件不致会话渲染失败 | `:167-172` | **真** |
| 事件元数据可查 | `DiagnosticsDrawer.tsx:30-40` | **缺口**。`data-metadata` 五个字段全仓零断言 |

**R18 结论：MUST NOT 在抽屉路径守住；MAJOR-1（生产筛选让 `raw_not_synced` 不可达）与 MAJOR-2（卡片错误归因）是真缺陷。两处断言强度不足列为 MINOR-2 / MINOR-3。**

### R19 Agent 目录与连接状态

**这是本包质量最高的一条。**

| MUST / MUST NOT | 证据 | 判别力 |
| --- | --- | --- |
| 列表来自快照，断连仍可呈现 | `r19-host-panel.test.ts:41-57`：连接态用 `reconnecting`（刻意避开「已断开」），断言 2 行 `data-agent-row` | **真** |
| 覆盖层缺席 = 状态未知 ≠ 已断开 | `:59-79`：每行 `connection === "unknown"`，断言无 `data-agent-connection-label="disconnected"`、无 `>已断开<`、2 处 `data-agent-connection="unknown"` | **真** |
| **MUST NOT 以会话活跃程度推断** | `:81-95`：**两条 `running` 会话 + 零覆盖层** → `["unknown","unknown"]` + `overlayPresent === false` | **真，且结构性**。`buildHostPanelModel`（`host-panel-model.ts:98-102`）的构造签名**不接收会话**，「没有可推断的输入」；`buildAgentCatalogView`（`domain/agent-catalog.ts:68-89`）对每个 Agent 只查 overlay Map，缺席即 `"unknown"`。作者声称有这条用例——**确实存在，且确实这么断言** |
| 同名 Agent 以标识区分 | `:112-127`（两个 `codex` 为 2 行、两个 id 都渲染）、`:129-146`（覆盖层按 id 索引，互不覆盖） | **真** |
| 未配对时不编造主机身份 | `:150-176` | **真** |

**R19 结论：全部守住，判别力真实。** 我另行确认覆盖层的唯一来源是 `AgentConnectionOverlayStore.ingest`（`client-store.ts:332-333`）消费 `agent.connected`/`agent.disconnected` 事件，`buildAgentCatalogView` 无其它输入。

---

## 7. 判别力差分（我自己复现，全部已还原）

`src/sync-client/` 两个源文件回退到基线、保留新用例：

| 变异 | 结果 |
| --- | --- |
| 两个文件都回退 `4ada2a2` | **6 failed / 4 passed** ✅ 与作者自报一致 |
| 只回退 `connection.ts`（保留门面的 `#isAdoptableBarrier`） | **4 failed / 6 passed**。红的是连接层的两条拒绝屏障 + 越界序号的门面/P1-N2 两例 |
| 只回退 `client.ts`（保留连接层前移） | **10 passed** |

**第三条是本报告的关键发现之一。** 作者在报告 §3 声称「门面去掉幂等守卫（只保留连接层的重排）→ 用例 B 的恢复点断言变红」。**实测不成立**：门面守卫对 10 条新用例**零判别力**，全部由连接层的前移独立承担。

这有两层含义：
1. 作者的证据陈述与实测不符（报告 §3「修复（幂等冗余）」一段的判别力论证不成立）；
2. 更要紧的是——`#isAdoptableBarrier` 是一条**零测试覆盖、且已被证明会致错**的代码。CRITICAL-1 正是它造成的。既然它对现有 10 条用例毫无贡献，最合理的处置是**删掉它**（连接层的前移已足够关闭 P1-N1，且我的 epoch 探针证明保留它反而有害），而不是给它补正确的 epoch 语义。

若 main 决定保留该守卫，则必须：(a) 修好 epoch 基准；(b) 补一条能单独证伪它的用例（当前无此用例）。

每次变异后我都执行了 `git checkout 221e76e -- <file>` 还原，最终 `git status --porcelain` 为空。

---

## 8. 既有测试是否被削弱

`git diff 4ada2a2..221e76e -- '*.test.ts' | grep -cE "^-[^-]"` → **0**。

**本轮未删除或改写任何既有用例行。** 既有 269 条全绿。`snapshot-barrier.test.ts` 是唯一新增的既有目录测试文件。

`layers.test.ts` 未出现在 diff 中 → 分层机检未被改写、未被绕过。我实测 `vitest run` 25 文件全绿，其中含 `layers.test.ts` 的两条方向判定。

---

## 9. 新增 69 条用例的质量审计

我在读完全部 8 个新用例文件后，分类如下：

**真实判别力（占多数）**：R15 全部 6 条、R16 全部 7 条、R19 全部 6 条、P1-N1/N2 的连接层两条 + 前提实测两条。

**发现的测试质量问题**（按严重度）：

1. **MAJOR-1 / MAJOR-2 的根因是「绕过生产路径」**：R18 的 8 条用例全部直调 `buildDegradedEventModel`，从不经过 `ClientStore`；`r19` 的面板用例有 `storeWith` 版本的对照（`:81-95`），证明作者知道怎么写端到端，只是 R18 没写。这是**覆盖形状的系统性缺口**，不是单条用例的疏忽。

2. **去重计数式断言的判别力边界**（作者报告 §8.4 点名要我核实）：
   - `r18:132`（三段 HTML 去重为 3）：**抓不住**「三类文案合并为一句」。因为三段 HTML 还带不同的 `data-unsupported-by` 属性值和不同的 `detail` 文案。
   - `r18:134`（`UNSUPPORTED_LABELS` 值去重为 3）：抓得住常量表本身，但**抓不住组件忽略该表**。
   - `r17:100-110`（三个 `data-context-state` 去重为 3）：**够用**，因为该属性是三态的判别标记本身，且同组用例断言了各自数字。
   - `r19:120`（两个 `data-agent-display-name="codex"` 长度 2）：**够用**，配合 `:124-126` 的两个 id 与 `<li>` 计数。

3. **同义反复**：`r17:118-134` 把 `CAPABILITIES` 常量注入 `buildConversationModel` 再断言输出等于输入——该函数只做 `{...input.capabilities, canLoadEarlier}` 透传，断言的是测试自己设的值。

4. **无效对照组**：`r17:150-169`（R20 imported 离线对照）没把连接推到 `online`，`canSend=false` 无论实现如何都成立。

5. **mock echo**：`r18:145` 的 `data-unsupported-detail="broker"` —— 该属性（`ConversationStream.tsx:52`）装的是 `reason.by` 而非 `reason.detail`；断言的是回声，测不出 detail 是否正确呈现。

6. **冗余断言**：`conversation-paging.test.ts:105` 的 `Object.keys` 断言紧随 `:96` 的 `toEqual`，无增量信息。

**未发现**：恒真断言、只断言非空/长度的断言（`toHaveLength(2)` 类都绑定了具体值）、复制被测逻辑（唯一接近的是 `r15-directory-model.test.ts` 自己造了一份 `session()` 构造器，但它复刻的是 wire 形状不是被测投影）。

---

## 10. 分级清单

### CRITICAL（1）

**CRITICAL-1** `clients/app/src/sync-client/client.ts:270-276` — epoch 变化后合法快照被门面永久丢弃。
现象：连接层接受并交付新 epoch 快照，门面以旧 epoch 的 `#ackedCursor` 为基准拒绝，仓库永久停留在重建前状态，列表与事件流长期不一致且无错误呈现。
影响：R14 重建快照语义被违反；列表永久陈旧；与被修的 P1-N1/N2 同型（永久卡死）。本轮引入（回退 `client.ts` 即消失）。
复现：见 §3 两个探针。
处置：修 `#isAdoptableBarrier` 的 epoch 基准（或删除该守卫——它对现有 10 条用例零判别力），补 epoch 变化 → 新快照入仓的正向用例。

### MAJOR（2）

**MAJOR-1** `clients/app/src/features/client-store.ts:175` — `raw` 硬编码 `{kind:"absent"}` 使 `raw_not_synced` 分支在生产不可达。
现象：已知事件类型 + 原文未下发 → 不产生降级卡片，静默丢失。
影响：违反 R18「原文未下发时 MUST 呈现未同步说明」。
复现：探针 `降级条目数: 0`。
处置：传 `rawAvailability(event.body.payload.acp)`；补经 `store.apply` 的端到端用例。

**MAJOR-2** `clients/app/src/components/ConversationStream.tsx:36` + `DegradedEventCard.tsx:41,43` — 非 unsupported 原因被兜底标成「客户端不支持」。
现象：二进制未下发事件的卡片写着「当前客户端未提供专用视图 / 客户端不支持」，与同卡片的「内容未下发」自相矛盾。
影响：违反 R18「三类不支持可区分」；错误归因误导排查。
复现：探针 HTML 见 §4。
处置：`unsupportedBy` 改为 `null` 而非 `"client"`，卡片据此不输出该属性与文案；补卡片层非 unsupported 用例。

### MINOR（4）

**MINOR-1** `src/components/r17-context-usage.test.ts:118-148` — 「未上报不阻断其余交互」是同义反复（常量注入后断言输出等于输入）；`conversationCapabilities` 的 `canCancel`/`canViewHistory` 无真实差分断言。不确定在哪：`canCancel`/`canViewHistory` 的实现（`conversation-model.ts:150-152`）看起来正确，但若被改成依赖 usage，当前用例不会红。

**MINOR-2** `src/components/r18-degradation.test.ts:95-97` — `not.toContain("{}")` 只禁三种字面量；`"{}"` 之所以绿是依赖夹具的 `view` 非空（`fixtures.ts` 的 `eventMessage` 缺省 `view = {}`），断言锁的是夹具形状。不确定在哪：空串 / `（无）` / `…` / `N/A` 形式的占位当前不会被拦。

**MINOR-3** `src/components/r18-degradation.test.ts:132` — 三段 HTML 去重为 3 抓不住「三类文案合并为一句」（属性值与 detail 仍不同）。真正的把关只有 `:134` 的常量表自检。

**MINOR-4** `src/features/conversation-paging.test.ts:96-105` — 复合游标断言依赖夹具本身升序（`messages[0]` 恰为最早），而生产走 `domain/conversation.ts:134-139` 的 `messages[0]`；降序页会返回最新一条当游标，无测试覆盖。另 `:105` 的 `Object.keys` 断言冗余。

### SUGGESTION（3）

**SUG-1** `src/sync-client/snapshot-barrier.test.ts` 报告 §3 的判别力陈述与实测不符：门面守卫对 10 条用例零判别力（§7 变异三）。建议删除该守卫或补能证伪它的用例，并修正交付报告的证据段。

**SUG-2** `plan.md:930` 的 Shared File Ownership 需按 §5.2 更新 Region Note，把 8 个新用例文件与 `features/testing/fixtures.ts` 登记为 WP7→TP3 的移交件。

**SUG-3** 补齐两处零覆盖：R18 的事件元数据（`DiagnosticsDrawer.tsx:30-40` 的五个 `data-metadata` 字段）、R17 的 `session.usage.changed` 端到端路径（我用探针确认生产实现正确，只是无用例）。

---

## 11. 对 TP3 / TP4（MU3e）的输入建议

**给 TP3：**

1. **必须补的三条端到端用例**（全部经 `ClientStore`，不直调模型）：
   - `session.usage.changed` → `usage.kind === "known"`（我已探针确认生产实现正确，只是无用例）
   - 已知事件类型 + `acp.rawUnavailable` → 必须产生 `raw_not_synced` 降级条目（**当前会红**，即 MAJOR-1）
   - epoch 变化 → 新 epoch 快照入仓（**当前会红**，即 CRITICAL-1）
2. **形态建议**：R18/R17 的用例应从「直调 `buildDegradedEventModel` + 手搭模型」改为「`storeWith` → `store.apply({kind:"event"})` → 读 `store.conversationPage(...)`」。R19 的 `:81-95` 已经是这个形态，可作为样板。
3. **`conversationCapabilities` 的真实差分**：对同一会话构造「有 usage 事件」与「无 usage 事件」两种输入，断言 `canSend`/`canCancel`/`canViewHistory` **三者全等**。当前的同义反复写法挡不住实现改成依赖 usage。
4. 覆盖 `DiagnosticsDrawer` 的五个 `data-metadata` 字段。

**给 TP4：**

1. **TP4 目前无法执行**。`_layout.tsx:25` 写死 `store={null}`，浏览器里每个路由都是 `RuntimeUnavailable`。`tasks.md` 4.13/4.14 覆盖的 R15/R16/R18 可观察行为（目录页离线渲染、两种空态、提交载荷、无权限态、降级卡片）在真实浏览器里**一条都观察不到**。**这是 5.1 组合根缺口的直接后果**，建议 main 在派发 TP4 之前先裁决组合根归属，否则 TP4 的浏览器环境只能验证 RuntimeUnavailable。
2. **组合根的前置依赖**：`SyncClient` 未公开 `connectionId`（`client.ts:254` 为私有）。组合根需要 `SyncClient` 暴露只读 getter，否则 `client-store.ts:565` 的第一道闸门恒不通过，**R16 的创建会话在真实运行时永远发不出去**，TP4 的「提交载荷只含两个引用」浏览器断言也无法成立。
3. **选择器已就绪**：作者留的 `data-route` / `data-content-origin` / `data-empty-state` / `data-context-state` / `data-create-permission` / `data-create-submission` / `data-degraded-event` / `data-raw-unavailable` / `data-agent-row` / `data-connection-state` 可直接用；但注意 `data-unsupported-by` 在非 unsupported 卡片上当前恒为 `"client"`（MAJOR-2），TP4 写断言前应等该缺陷修掉，否则会把错误行为固化成期望。

**给 main：**

4. **组合根必须单独立包或扩 Write Scope**，不能推到 MU3e 的 TP3/TP4（它们都不含该文件）。同时把 `SyncClient.connectionId` 的公开化一并登记为前置。
5. **CRITICAL-1 的处置建议删除 `#isAdoptableBarrier` 而非修它**：它对 10 条新用例零判别力（§7 变异三），连接层的前移已独立关闭 P1-N1，而保留它已被证明在 epoch 变化时有害。

---

## 12. 纪律声明

- 未修改 `feat/wp7` 上任何源文件。所有变异经 `git checkout 221e76e -- <file>` 还原。
- 四个临时探针（`sync-client/zz-probe.test.ts`、`zz-probe2.test.ts`、`features/zz-probe3.test.ts`、`zz-probe4.test.ts`）与两个分层探针（`src/zz-layers.test.ts`）已全部删除。
- 最终 `git status --porcelain` 输出为空，`HEAD` 仍为 `221e76e`。
