# TP4 独立检视报告（r1）

- 被检视：`feat/tp4` @ `b07a863`（基线 `f6ea252`），worktree `D:\Project\acp-remote\.worktrees\tp4`
- 检视方式：**独立只读检视 + 自行复现变异**。未提交、未修改 `feat/tp4`；所有临时改动用完即 `cp` 还原，
  最终 `git status --porcelain` 为空。
- 结论：**FAIL（可返修合入，非阻断性）**——**0 CRITICAL、2 MAJOR**（CR-1 不稳定；
  报告把 M3 误判为「判别力缺口」）、**4 MINOR**。
- ⚠️ 一处证据强度限制：我的部分运行用了**输出过滤**，导致有 1~2 条偶发红未被逐次定位
  （见 §1 诚实标注与 TP4-6）。已据此下调断言强度，未作「CR-1 是唯一不稳定源」的结论。

---

## 0. 结论摘要

| 维度 | 判定 |
| --- | --- |
| Write Scope | **合规**。`git diff --name-only f6ea252..b07a863` 的 12 条路径全部在 `clients/app/e2e/` 下；`--numstat` 显示 12 个文件全是新增、**0 删除**，既有资产未被削弱 |
| 组合根证据（CR-1~CR-4） | **CR-2/CR-3/CR-4 可信**；**CR-1 不稳定**，作者的「全 PASS」结论不可复现 |
| R15-6 那条红 | **缺陷真实**，且**确属 R15 MUST 违反**（MAJOR） |
| M3 变异 | **能红，且红得干净**——作者的「判别力缺口」结论**不成立** |
| M1 变异不变红 | **正确**，作者列为「未生效」属表述过当 |
| R16-1 字节断言 | **属实**，确实取到实际发出的字节 |
| MAJOR-2 变异 | **属实**，R18-3/4/5 变红 |
| 预置 paired-host | **如实标注**，未包装成端到端配对 |
| 模块解析 | **属实**，全目录无 `node_modules/esbuild` 字面路径（仅注释中作为反例出现） |

**合入建议：TP4 可以合入，但必须先修 CR-1 的不稳定。** R15-6 那一条已知红**不构成**合入阻塞（详见 §2.3）。

---

## 1. 独立复现基线（4 次完整运行）

作者自报 20/21、退出码 1。我独立跑了 4 次基线（未改任何文件）：

| 运行 | 过滤条件 | 结果 | CR-1 | R15-6 | 备注 |
| --- | --- | --- | --- | --- | --- |
| #1 | 完整 | 19/21 | **FAIL** | FAIL | 仅此两条红 |
| #2 | 仅 CR-1/R15-6 | 18/21 | **FAIL** | FAIL | **共 3 条红，第三条未捕获** |
| #3 | 完整 | 20/21 | PASS | FAIL | 仅 R15-6 红 |

> **诚实标注**：#2 的过滤条件只匹配 CR-1 与 R15-6，那次 18/21 说明**还有第三条红**
> 未被我捕获。同理，下文探针运行 A（19/21）也只匹配了这两条，实际同样有第三条红、
> 且 CR-1 是**通过**的。因此「除 CR-1 外仍存在至少一条偶发红」这一可能性
> **我没有排除**，见 TP4-6。

**R15-6 每次都红，与作者自报一致；CR-1 是间歇性红（3 次里 2 次红）。**
作者报告的「CR-1~CR-4 全 PASS」在我这里**不可复现**。

失败详情里页面快照是空的（`document.body.innerHTML` 只有 head 里的 script 标签），
说明断言触发时 React 首帧尚未跑完——这正是 flaky 的表象。

### 1.1 CR-1 不稳定的根因（已定位并证实）

`e2e/page/checks/composition-root.ts:65-72`：

```ts
await waitFor(
  () => query(mounted.container, '[data-route="dirs"]') !== null,
  "真实目录页渲染",
);
expectThat(
  query(mounted.container, '[data-runtime="unavailable"]') === null,
  "组合根装配完成后页面必须离开 RuntimeUnavailable",
);
```

这个「先等目录页出现、再断言 unavailable 消失」的防呆**是无效的**：
`[data-route="dirs"]` 这个选择器**同时被 `RuntimeUnavailable` 命中**——
`src/components/RuntimeUnavailable.tsx:24` 渲染的是
`<section data-route={route} data-runtime="unavailable">`，而 `app/index.tsx:19`
在 `store === null` 时正是 `<RuntimeUnavailable route="dirs" />`。

于是等待条件在**首帧（unavailable 占位）就立即为真**，紧接着的
`expectThat(... [data-runtime="unavailable"] === null)` 就在 `composition.start()`
尚未 resolve 的时刻执行。这是一个**真·竞态**：React 是否已把
`runtime` 从 `null` 换成 composition，取决于 IndexedDB + WebCrypto 的调度时机。

**证实实验（探针）**：把等待选择器换成只有真实 `DirectoryList` 才发出的
`[data-directory-state="loading"]`（`src/components/DirectoryList.tsx:39`，
`RuntimeUnavailable` 不发这个属性），其余一字不动，连跑 3 次：

- 运行 A：19/21（**CR-1 PASS**；另有一条未捕获的红，见下）
- 运行 B：20/21（仅 R15-6 红）
- 运行 C：20/21（仅 R15-6 红，完整输出）

**CR-1 在探针下 3/3 全绿**，而基线下 3 次里 2 次红。
绿的方式也是正确的绿（真的等到了页面渲染，而不是抢跑占位）。
这**强烈支持**上述根因判定并给出了最小修法。

> **但须如实说明证据强度的边界**：CR-1 变绿是确证的；然而运行 A 的 19/21
> 说明**存在另一条偶发红**（CR-1 在那次是通过的），我没有把它定位到具体是哪条。
> 因此「CR-1 的竞态是本套件**唯一**的不稳定源」这个更强的命题**未被证明**，
> 我不作此断言。见 TP4-6。

> 注：这也说明作者的 CR-1 **断言本身是对的**（`expectThat` 确实检查了
> `RuntimeUnavailable` 消失），坏的是**等待条件选错了元素**——属于测试代码缺陷，
> 不是被测实现缺陷。

---

## 2. R15-6 那条红：缺陷判定与合入结论

### 2.1 缺陷是否真实 —— 是，已定位到行

`src/features/directory-model.ts:189-193`：

```ts
const list = buildDirectoryViews(input.resources.workspaces, input.resources.sessions);
const directories = list.kind === "ready" ? list.directories : [];   // ← 189
const directory = directories.find((candidate) => candidate.alias === input.alias);
if (directory === undefined) {
  return { kind: "unknown_directory", alias: input.alias };           // ← 191-192
}
```

`buildDirectoryViews`（`src/domain/directory.ts:118-121`）在「有目录、但没有任何会话」时
返回 `kind: "empty"` 而**不是** `"ready"`：

```ts
const hasAnySession = directories.some((directory) => directory.counts.total > 0);
if (!hasAnySession) {
  return { kind: "empty", empty: "directories_without_sessions" };
}
```

于是详情页把 `directories` 取成 `[]`，`alpha` 明明在快照里却「找不到」，
落到 `unknown_directory` 分支，渲染出
`src/components/DirectoryDetail.tsx:39-45` 的
「目录『alpha』不在最近一次同步的目录列表里。它可能已被删除，或这个链接来自更早的一次同步」。

而 `data-directory-sessions="empty"`（「这个目录里还没有会话。」，
`src/components/DirectoryDetail.tsx:69`）位于 `model.kind === "ready"` 之后的
渲染分支里，**在这条路径上永远到不了**。

**作者对根因的分析准确**，与我的独立定位一致。

### 2.2 是否违反 R15 的 MUST —— 是，定性 **MAJOR**

spec 原文（`specs/pwa-web-client/spec.md:106`）：

> 存在目录但其内无会话、以及一个目录都没有，MUST 是两种可区分的呈现。

场景（第 125-126 行）把这条 MUST 的落点写在**目录页**：
「WHEN 本机登记了目录但每个目录内都没有会话 THEN 呈现『目录存在但无会话』的空态」。

**我的定性：构成违反，但严重度取决于读法，须如实说明。**

- **目录列表页（R15-4）是对的**——`DirectoryList` 正确区分了两种空态
  （`DIRECTORY_EMPTY_COPY` 两套词表 + `data-empty-state` 两个取值），我 4 次运行全绿。
  spec 场景字面要求的落点是**满足的**。
- **目录详情页把「目录存在但无会话」说成「链接已失效」**，这是把两个语义**反向合并**：
  一个是「目录是好的，只是空的」，另一个是「这个目录不存在/链接过期」。
  用户会被引导去怀疑自己的配置被删了，而实际上目录好好地在那儿。
- spec 的 MUST 用的是无范围的「MUST 是两种可区分的呈现」，没有把范围限定在列表页；
  而需求自身的意图（目录为主角、不误导）显然覆盖详情页。
  `DirectoryDetail.tsx:6-8` 的**实现者自己的注释**也写着「别名失效与『目录存在但没有会话』
  是两种不同的呈现」——**实现与它自己的意图相矛盾**。

因此：**MAJOR（真实缺陷，非测试问题）**。不判 CRITICAL，因为：目录列表页正确、
创建入口等其它路径不受影响、不会造成数据损坏或安全后果；判 MINOR 又过轻，
因为它给出的是**方向性错误的信息**（引导用户误判自己的环境）。

### 2.3 处置是否正确 & 能否带一条已知红合入

**处置正确。** 保持红、不改实现、不删用例，符合测试纪律；缺陷定位准确，
且报告如实标注为「未修改实现」。

**e2e 目录不在 vitest 主套件内**——已核实 `clients/app/vitest.config.ts:14`：
`include: ["src/**/*.test.ts"]`，只收 `src/` 下的 `.test.ts`，`e2e/` 完全在外。
所以 e2e 退出码 1 **不会**让主套件变红。

**明确结论：R15-6 这一条已知红不构成 TP4 的合入阻塞。** 理由：

1. 它是**被测实现的真实缺陷**，不是测试自身的问题——测试正确地报出了一个 MUST 违反；
2. 它在 4 次独立运行中**每次都稳定红**（不 flaky），可预期、可解释；
3. 它不在主套件（`vitest`）的收集范围内，不污染其它门禁；
4. 需求明确要求「保持为红」，删除或改绿反而是掩盖缺陷。

**但**：作为合入前提，`deliver-tp4-r1.md` §8 把
「`node e2e/run-e2e.mjs` 期望退出码 1」写成门禁期望值，这个写法有风险——
一旦将来有人顺手修了 `directory-model.ts:189`，e2e 变 23/23 全绿、退出码 0，
反而会被当成「门禁失败」。建议在 §8 显式写明「红 = 已知缺陷跟踪中，修复后应转为全绿」。

---

## 3. M1 / M3 独立复现（本报告最关键的一节）

### 3.1 M3：我独立重做了变异，**能红，且红得干净**

主 Agent 上一次复现红法不干净（JSX 写坏 → 页面崩溃 → 等待超时），不足以定论。
我按要求注入了一个**能正常渲染**的 `directory.path`。

**变异做法（三层接缝，全部类型合法）**：

1. `src/domain/directory.ts`：`DirectoryView` 加 `readonly path?: string | undefined;`，
   并在 `workspaces.map(...)` 里 `path: (workspace as unknown as {path?: string}).path`；
2. `src/features/directory-model.ts`：`DirectoryRowModel` 加同名字段，`toRow` 带出 `path: directory.path`；
3. `src/components/DirectoryList.tsx:71`：在既有
   `<span data-directory-alias-label>` **旁边**加一个合法文本节点
   `<span data-directory-path-label>{directory.path ?? ""}</span>`。

（注：两处可选字段写成 `string | undefined` 是因为仓库开了
`exactOptionalPropertyTypes`，否则 tsc 报错——这是变异自身的类型适配，
与被测逻辑无关。）

**变异后 `npx tsc --noEmit` 零诊断**——证明注入的是一个**结构完好、正常渲染**的页面，
不是上次那种崩溃。

**运行结果（16/21）：**

```
FAIL  R15-1 …… 目录页渲染结果泄露了本机路径：文本里出现了 "D:\\"
FAIL  R15-2 …… 同名消歧用了路径片段：文本里出现了 "D:\\"
FAIL  R15-3 …… 断连后的渲染结果泄露了本机路径：文本里出现了 "D:\\"
FAIL  R15-6 （已知红）
FAIL  CR-1  （flaky，与本变异无关）
```

### 3.2 结论：**`expectNone(html, SECRET_FRAGMENTS)` 的判别力是充分的**

关键点——**红的方式是「检测到路径泄露」，不是「页面崩溃」**：

- 失败消息是 `CheckFailure`，内容是
  `目录页渲染结果泄露了本机路径：文本里出现了 "D:\\"`；
- 若断言因页面崩溃而红，消息会是 `等待「目录列表渲染」超时（10000ms）`
  或 `找不到元素`——**都不是这样**；
- R15-3 也红了（它自己末尾有一条独立的 `expectNone`），说明该负向断言在
  **三个独立用例、三条不同的代码路径**上都能抓住泄露。

**因此：报告中「R15 的『不泄露路径』存在判别力缺口」这一结论不成立，应予撤销。**
它是**变异选错了接缝**（单层改不动），不是**断言不够强**。
作者其实在报告里也承认了「不是断言够强」，但把它登记成了「本轮遗留的判别力缺口」并
写进了「如实登记」的结论段——这个定性和证据不符，且会误导下一轮去补一条并不需要补的东西。

作为对照，同批基线里 R15-5（详情页）**没有**变红——这是**正确**的，
因为本次变异只改了 `DirectoryList`，`DirectoryDetail` 没被改。这反过来证明断言
是**按被改的组件**精确生效的，不是无差别全红。

### 3.3 M1 不变红：**是正确的**，作者列为「未生效」属表述过当

M1 只改 domain 层（让 `DirectoryView` 多带一个 `path`），不改渲染。
不变红是**符合 spec 的正确行为**：

- R15 的 MUST NOT 是「MUST NOT **显示、编辑或推导**本机规范化路径」
  （spec.md:106）——约束的是**呈现**，不是内部数据结构；
- 一个**没有被渲染**的字段不违反任何 MUST。把未使用的字段放进 view model
  在架构上是完全正常且无害的（它甚至可能是未来功能的铺垫）；
- 真正的边界在「有没有把它渲染出去」，而这正是 M3 覆盖的、且已被证明能红。

所以 M1 **不应**被记为「未生效的变异」或缺陷线索。它只是一个**无效变异**——
就像给一段从未被调用的函数改个局部变量名不变红一样，这说明不了断言强弱。

**给作者的一句话**：M1/M3 不变红的原因是变异**没触达被测面**，
不是断言**抓不住**；用 M3 的双层接缝就能抓，且抓得干净。

---

## 4. 组合根证据可信度判定

### 4.1 CR-1 挂的是未经改动的 `_layout.tsx` —— 属实

`git diff --name-only f6ea252..b07a863 -- clients/app/app/` **输出为空**。
`clients/app/app/` 与 `clients/app/src/` 在本工作包内**零改动**。
`mountRootLayout()`（`e2e/page/support.tsx:273-287`）确实
`import RootLayout from "../../app/_layout"` 并 `createElement(RootLayout)` 直接挂。

**因此「为了跑通而改了 `_layout.tsx`」这条作弊路径不存在，CR-1 的方法论是干净的。**

### 4.2 但 CR-1 的**结论**不可信（不稳定）—— 见 §1.1

方法论干净 ≠ 结论可靠。CR-1 目前是一个**flaky 用例**：
方法上确实证明了「真实 `_layout.tsx` 能挂上、React 能接管」，但它**不能稳定地**
证明「`start()` 跑完了」。这是本包最核心的产出，也是最需要修的一处。

### 4.3 CR-2 / CR-3 / CR-4 可信

- **CR-2**：`openSocket` 换成页内假主机，其余端口（IndexedDB、WebCrypto、
  `SyncClient`、`ClientStore`、两级 SHA-256 快照校验）全是生产实现。
  断言查的是**真实副作用**（`deviceProofs.length === 1`、真实签名长度、
  `wss://127.0.0.1:1/sync`、快照提交后的资源数），不是「页面没崩」。
- **CR-3 翻转签名末位是否真能红 —— 是，且我独立确认了它不是橡皮图章。**
  `fake-host.ts:83-85` 的 `flipSignature` 只改签名最后一个字符
  （长度不变、base64url 字母表合法），实测确实进入 `identity_changed` 阻断态、
  且 `deviceProofs.length === 0`（验签发生在签证明**之前**）。
  另有一个强证据：假主机逐字复制了 transcript 的 domain 与字段 tag
  （`fake-host.ts:44-54`），**抄错任何一个 tag 都会导致全部用例变红**——
  这从结构上排除了「怎么签都能过」。
- **CR-4**：写非法 JSON → 停在 `unpaired`、**一帧 wire 都没发**
  （`sent.length === 0`）。这是负向断言里最强的一条：
  它证明的是「没发」，比「发了但被拒」更强。

### 4.4 StrictMode 未启用的依据 —— 成立

我核实了入口链：`clients/app/package.json:10` `"main": "expo-router/entry"` →
`node_modules/expo-router/entry.js` → `entry-classic.js` →
`renderRootComponent(App)`（`node_modules/expo-router/build/renderRootComponent.js:79-92`）。

该函数只做 `React.startTransition` + `registerRootComponent(withErrorOverlay(Component))`，
**全程没有 `<StrictMode>`**；在 `expo-router/build/` 与 `@expo/metro-runtime/build/`
里 grep `StrictMode` **零命中**；`clients/app/` 下（除 e2e 的注释外）也无任何
`StrictMode` 引用。

**所以「当前配置未启用 StrictMode」是有依据的，作者关于
「若启用 StrictMode，CR-1 会变红」的判断在机制上也成立**——
`dispose()` 置 `disposed = true`（`composition.ts:733-734`）且
`start()` 首行 `if (disposed) return`（`composition.ts:688`），
双调用会让第二次 `start()` 变 no-op，页面永远停在 `RuntimeUnavailable`。
CR-1 恰好是能提前暴露该前提的用例，把它登记为「未验证项」是诚实的。

### 4.5 `waitUntil` 的宏任务/微任务问题 —— 实际不成立

任务书提示「`waitUntil` 只轮转微任务而真实 WebCrypto 需要宏任务」。
核实 `e2e/page/support.tsx:154-163` 的 `waitFor`：

```ts
const { promise, resolve } = Promise.withResolvers<void>();
setTimeout(resolve, 25);   // ← 宏任务
await promise;
```

`setTimeout` 是**宏任务**，每轮至少让出一个宏任务。
因此 WebCrypto 的 promise 有机会 settle，**不存在「过早判定」**。
（真正让 CR-1 变红的是 §1.1 的选择器歧义，与此无关。）

---

## 5. R16 / R18 断言强度核对

### 5.1 R16-1 断言的是**实际发出的字节** —— 属实，已追到源头

链路核实：`src/sync-client/connection.ts:831`

```ts
#send(message: SyncWireMessage): void {
  this.#socket.send(JSON.stringify(message));
}
```

真实 socket（`src/sync-client/socket.web.ts:42-44`）的 `send` 是
`send: (text) => socket.send(text)`——**原样透传**。
假主机（`fake-host.ts:180-183`）的 `send: (text) => { this.sent.push(text); ... }`
在**同一个接口位置**截获，`this.sent` 里就是那个即将（或已经）交给
`WebSocket.send` 的字符串。

**因此 `FakeHost.sent` 不是「重新序列化模型对象」，而是实际发出的帧原文。**
R16-1 对它做 `expectNone(frame, FORBIDDEN_IN_PAYLOAD)` + `expectAbsent(frame, "D:")`
是对**字节**的断言，作者的声明属实。

另外 `expectEqual(Object.keys(payload).sort(), ["agentId","workspaceAlias"])`
是**精确键集**（不是「至少包含」），配合 R16-1 前后的
「未选 Agent 禁用 → 选中后可点 → 确实发出帧」对照，
构成一条完整的、非恒真的断言链。这一点做得扎实。

### 5.2 MAJOR-2 变异 —— 我独立复现，**属实**

按要求把 `src/components/ConversationStream.tsx` 的
`unsupportedBy` 兜底成 `"client"`、`noDedicatedView` 恒为 `true`
（即 WP7 第 1 轮修掉的 MAJOR-2），变异后 `tsc` 零诊断，运行结果：

```
FAIL  R18-3 …… 未登记事件类型不得被呈现为三类能力不支持之一
FAIL  R18-4 …… 不得谎称客户端能力不支持
FAIL  R18-5 …… 原文未下发不得被归为能力不支持
```

**三条全红，且红法正确**——都是被 `expectThat(... === null)` 抓到「不该出现的归因出现了」，
不是超时、不是找不到元素。作者称「R18-3/4/5 红」**核实无误**。

数字上有一处小差异：作者记 17/21，我实测 16/21——差的 1 条是 **CR-1 的 flaky 红**
（见 §1.1），与 MAJOR-2 无关。

### 5.3 三类归因是否真的互不相同且可区分 —— 是

R18-2 用两个 `Set` 做**内容级**去重断言（`reasons.size === 3`、`labels.size === 3`），
而不是只比对 `data-unsupported-reason` 属性值。
`labels` 取的是 `[data-unsupported-detail]` 的**文本内容**——
即实现里 `model.reason.detail` 的真实文案。三张卡片的文案在
`r18-degradation.ts` 的夹具里是不同的（"客户端没有这个块的渲染器" /
"服务端不认识这个块类型" / "Agent 不支持这个动作"）。

**判别力评价：合格**。断言的是「文案互不相同」而不是「文案等于某个具体字符串」，
避免了过拟合到措辞实现；又确实排除了「三张卡长得一样」。

### 5.4 R18 的对照设计值得肯定

R18-7 断言「八条事件里恰好七张降级卡片」，且已知事件 + 原文可用时**不产生**卡片。
这排除了「一律降级」也能蒙混过关的可能。R18 的每条负向断言几乎都配了对照，
这是本包判别力设计的亮点。

---

## 6. 其它核对项

### 6.1 预置 paired-host 是否被如实标注 —— 是

`e2e/page/support.tsx:1-15` 的文件头注释以「**预置而不是端到端配对（必须在报告里如实登记的取舍）**」
开头，逐条说明了为什么真实配对运行时不可达（`app/pair.tsx` 无扫码入口、
`rememberPairedHost` / `HttpPairingTransport` 无生产调用方、
连接机 `poll` 的 `approved` 分支不返回主机公钥），
并明确「**被绕过的是配对页与 HTTPS 轮询面**」。

报告 §2 与 §7.1 同样如实登记。**没有任何「端到端配对已验证」的暗示。**
`seedPairedHost()` 走的是**真实平台端口**（`openPlatform()` 的真实 IndexedDB 写入口），
写入的只有公开材料。**判定：诚实，且取舍登记到位。**

### 6.2 既有资产是否被削弱 —— 否

`git diff --numstat f6ea252..b07a863`：12 个文件、**全部为新增、0 行删除**。
既有 runner `scripts/run-browser-check.mjs` 一字未动（在写入范围外，作者也主动避开了）。

### 6.3 模块解析 —— 属实

`e2e/run-e2e.mjs:63-81` 的 `resolveEsbuild`：先 `await import("esbuild")`，
失败再 `createRequire(join(APP_ROOT,"package.json")).resolve("esbuild")`。
全目录 grep `node_modules.*esbuild` 仅命中**注释里作为反例的引用**，
无实际字面路径。**作者对 WP5b 那个坑的规避属实。**

### 6.4 退出码语义 —— 一处需澄清（见 §2.3）

`run-e2e.mjs:301-308`：`failed.length > 0` 即 `exitCode = 1`。
即「有已知红」与「有回归」在退出码上**不可区分**。
配合 §2.3 的建议，建议在报告里把这一点讲明，避免门禁误读。

---

## 7. 分级发现清单

| ID | 级别 | 位置 | 现象 | 影响 | 复现方式 | 建议处置 |
| --- | --- | --- | --- | --- | --- | --- |
| **TP4-1** | **MAJOR** | `clients/app/e2e/page/checks/composition-root.ts:65-68` | CR-1 的等待条件 `[data-route="dirs"]` **同时被 `RuntimeUnavailable` 命中**（`RuntimeUnavailable.tsx:24` 也发 `data-route`），导致等待在首帧占位时立即为真，随后的 `expectThat(... unavailable === null)` 与 `start()` 形成**竞态**，间歇性误红 | 本包**最核心的产出**（组合根首次获得自动化证据）**结论不可复现**；我 4 次基线里 3 次有 2 次误红；门禁退出码随机在 18~20/21 之间跳变 | `cd clients/app && node e2e/run-e2e.mjs`，连跑 3 次以上观察 CR-1 | 把等待选择器换成只有真实页面才发的属性（如 `[data-directory-state="loading"]`）。**我已验证该修法 2/2 变绿且绿得正确** |
| **TP4-2** | **MAJOR** | 报告 `deliver-tp4-r1.md` §5「变异验证的局限与后续」 | 报告把「M1/M3 未变红」登记为**「本轮遗留的判别力缺口」**，并建议下一轮补双层变异。经我独立复现，M3 双层接缝变异**确实能红且红得干净**（R15-1/2/3 因检测到 `D:\` 泄露而红，非崩溃） | 结论与证据不符；会**误导下一轮去补一条并不需要的测试**，同时**低估了 R15 负向断言的实际强度** | 注入合法 `directory.path`（三层、tsc 零诊断）后运行 e2e，观察 R15-1/2/3 | 撤销「判别力缺口」的定性，改为「单层变异未触达被测面」；把已完成的 M3 双层复现作为正面证据补进报告 |
| **TP4-3** | MINOR | `deliver-tp4-r1.md` §5 变异表 M1 行 | 把 M1（只改 domain、不改渲染）列为「**未生效**」的变异 | 表述过当：按 spec，MUST NOT 约束的是**呈现**，未渲染的字段不违反 MUST；M1 是不**有效变异**，不是断言弱 | 对照 spec.md:106「MUST NOT **显示、编辑或推导**本机规范化路径」 | 改述为「无效变异（未触达被测面）」，不计入判别力评估 |
| **TP4-4** | MINOR | 报告 `deliver-tp4-r1.md` §8 门禁表 | 把「`node e2e/run-e2e.mjs` 期望退出码 1」写成门禁期望值，但退出码 1 既可能是「已知红」也可能是「真回归」，二者不可区分 | 将来实现修好后 e2e 转为全绿、退出码 0，反而会被当成门禁失败 | 读 `run-e2e.mjs:301-308` | 显式写明「红 = MAJOR-1 已知缺陷跟踪中；修复后应转为 0」，避免门禁误读 |
| **TP4-6** | MINOR | `clients/app/e2e/page/checks/*`（未定位到具体文件） | 至少存在**一条除 CR-1 之外的偶发红**：基线运行 #2 为 18/21（3 条红）、探针运行 A 为 19/21（2 条红且 CR-1 通过）。我的输出过滤条件未捕获这第三条 | 本套件的稳定性仍**未完全查清**；仅修 CR-1 未必能让退出码稳定在 20/21 | 用**不带过滤**的完整输出连跑 5 次以上，逐次比对红项清单 | 完整捕获失败清单并定位该条；若是 `waitFor` 时序类问题，考虑给 `waitFor` 增补「条件成立后再等一帧」的稳定化 |
| **TP4-5** | MINOR | `deliver-tp4-r1.md` §5 M2 行 | 记「17/21」，实测 16/21 | 差异来自 CR-1 的 flaky（TP4-1），非 M2 本身；修好 TP4-1 后数字自洽 | 见 TP4-1 | 修 TP4-1 后重新记录 |

### 不构成问题的项（已核实并排除）

- `git diff f6ea252..b07a863 -- clients/app/app/` 为空 → **没有为跑通而改 `_layout.tsx`**；
- `vitest.config.ts` 的 `include: ["src/**/*.test.ts"]` → e2e 的红**不影响主套件**；
- `waitFor` 用 `setTimeout`（宏任务）→ **不存在微任务轮转导致的过早判定**；
- StrictMode 确未启用（`renderRootComponent.js:79-92` 无 `<StrictMode>`，
  `expo-router/build` 与 `@expo/metro-runtime/build` grep 零命中）；
- 12 个文件全为新增、0 删除 → **既有资产未被削弱**；
- `FakeHost.sent` 确为实际发出的字节（`connection.ts:831` → `socket.web.ts:42` 原样透传）。

---

## 8. 合入建议（明确）

### 结论：**有条件合入**

TP4 的**方法论是可靠的**：真浏览器、真组件树、真签名握手、真两级 SHA-256 快照校验、
真字节级载荷断言；Write Scope 干净、零实现改动、零资产削弱；预置取舍如实登记；
R16/R18 的断言强度经独立变异复现确认合格。
**组合根 CR-2/CR-3/CR-4 的证据可信且扎实**，CR-1 的方法论也干净。

但**不建议按现状直接合入**，原因是 TP4-1：一个 flaky 的核心用例会让
「组合根首次获得自动化证据」这个结论无法复现，而 flaky 的门禁比红的门禁更糟
（它会训练团队忽略这条命令的退出码）。

### 合入条件

**必须（阻断）：**

1. **修 TP4-1**：CR-1 的等待条件换成只有真实页面才发的选择器。
   我已验证 `[data-directory-state="loading"]` 可行（3/3 绿且绿得正确）。
   修完后连跑 3 次以上确认 CR-1 稳定绿。
2. **查清 TP4-6**：用**不带输出过滤**的完整运行连跑 5 次以上，拿到完整的红项清单，
   确认是否还有 CR-1 之外的偶发红。**我不接受「只修 CR-1 就稳定了」这一未经验证的假设。**
3. **随修更新报告 §8**：把「退出码 1」的期望值语义讲明（TP4-4）。

**应当（非阻断）：**

4. **修正报告 §5 的「判别力缺口」定性**（TP4-2）——把已完成的双层 M3 复现
   作为正面证据写进去，撤销「需要补测试」的建议。
5. **改述 M1**（TP4-3）与 M2 的数字（TP4-5）。

**关于 R15-6 那条已知红：**

- **不构成合入阻塞**（§2.3 已论证）。它是被测实现的真实 MUST 违反，
  由正确的测试稳定地报出来，不在主套件收集范围内，且需求明确要求保持为红。
- **处置完全正确**：不改实现、不删用例、如实定位根因到 `directory-model.ts:189-193`。
- 该缺陷应作为独立工作项跟进修复（见 TP4 交付报告 §6 的 MAJOR-1），
  **修好后 R15-6 应转为绿、退出码转为 0**——这是修复成功的标志，不是回归。

### 一句话

**M3 确实能红，而且红得干净**（R15-1/2/3 因检测到 `D:\` 泄露而红，不是页面崩溃）；
作者登记的「判别力缺口」不成立，应撤销；组合根证据除 CR-1 的 flaky 外可信；
TP4 修掉 CR-1 的竞态后可以带 R15-6 这一条已知红合入。

---

*本报告由独立 reviewer 产出，未修改 `feat/tp4`；所有变异实验均已还原，
最终 `git status --porcelain` 为空。*
