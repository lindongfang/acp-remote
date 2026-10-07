# TP4 测试设计报告（前端浏览器级验证）

- 变更：`sync-scope-and-pwa-client`（前端 Web/PWA 客户端）
- 工作包：TP4，基线 `f6ea252`，分支 `feat/tp4`
- 写入范围：仅 `clients/app/e2e/`
- 被测实现（只读）：`clients/app/src/{components,features,domain,sync-client,platform}`、
  `src/composition.ts`、`app/`
- 运行入口：`cd clients/app && node e2e/run-e2e.mjs`（真实 Chromium，退出码 0/1）

---

## 1. 交付物与运行方式

| 文件 | 作用 |
| --- | --- |
| `clients/app/e2e/run-e2e.mjs` | Node 侧入口：解析 esbuild → 打包页面 → 起静态服务 → 拉起 Chromium → CDP 驱动 → 逐条打印 |
| `clients/app/e2e/page/index.tsx` | 页面入口，把检查集合挂到 `window.__tp4` |
| `clients/app/e2e/page/support.tsx` | 断言原语、DOM 读取、场景装配、**身份预置** |
| `clients/app/e2e/page/fake-host.ts` | 页内假主机：真实签名握手 + 真实快照下发 + 命令应答 |
| `clients/app/e2e/page/router-stub.tsx` | `expo-router` 替身（仅 `Stack`/`useRouter`/`useLocalSearchParams`） |
| `clients/app/e2e/page/react-dom-client.d.ts` | `react-dom/client` 的最小声明（无 `@types/react-dom`，不新增依赖） |
| `clients/app/e2e/page/fixtures.tsx` | 违规夹具（带 `path` 的目录）、事件构造器、路由条目 |
| `clients/app/e2e/page/checks/*.ts` | 四组检查：组合根 / R15 / R16 / R18 |
| `clients/app/e2e/tsconfig.json` | e2e 的独立类型检查（`expo-router` 指向替身，`lib` 提到 ES2024） |

### 1.1 与既有 runner（`scripts/run-browser-check.mjs`，WP5b）的关系与分工

两者**并存且不重叠**：

- WP5b 的 runner 跑 `src/platform/testing/browser-checks.ts`：平台端口的浏览器事实
  （IndexedDB 往返、`crypto.subtle` 拒绝导出不可导出私钥）。它的页面里**没有 React、没有路由、
  没有 `useEffect`**，因此答不了「组合根接线通不通」。
- TP4 的入口跑的是**真实页面**：真实 `app/_layout.tsx`（或与它同构的挂载件）→ 真实 `app/` 路由
  组件 → 真实 `ClientStore` → 真实 `SyncClient`。它新增的是「点按钮会发生什么」「DOM 里出现了什么」。
- 本工作包**没有改动** WP5b 的 runner，也不在其写入范围内；`node scripts/run-browser-check.mjs`
  仍为 16/16。

### 1.2 对 WP5b runner 那个「硬编码 esbuild 路径」坑的规避

`scripts/run-browser-check.mjs:56` 用
`join(APP_ROOT, "node_modules", "esbuild", "lib", "main.js")` 加载 esbuild——那是**未声明的传递依赖**
（esbuild 只经 vite/vitest 间接安装），安装布局一变就 `ERR_MODULE_NOT_FOUND`。

`e2e/run-e2e.mjs:69-81` 一律走**模块解析**：先 `await import("esbuild")`（裸说明符，按模块图解析），
失败再 `createRequire(clients/app/package.json).resolve("esbuild")`。全仓库检索，本目录不含
`node_modules/esbuild` 字面路径。

---

## 2. 预置而非端到端配对（明确登记的取舍）

`app/pair.tsx` 没有二维码扫描入口，`src/composition.ts` 的 `rememberPairedHost` /
`HttpPairingTransport` 没有生产调用方，连接机 `poll` 的 `approved` 分支也不返回主机公钥——
**真实配对在运行时不可达**（`review-wp7-r2` 登记）。

因此 `support.tsx:seedPairedHost()` 直接用**真实平台端口**（`openPlatform()` 的真实 IndexedDB
写入口）写入两样东西：

1. 设备身份：`deviceIdentity.ensureIdentity({deviceId, canonicalOrigin})`——
   真实 `crypto.subtle.generateKey({extractable:false})` + 真实 IndexedDB 落盘；
2. `paired-host/<origin>` 摘要：`localCache.putLocalSummary({key, summary, nowMs})`，
   形状与 `composition.ts:678-685` 的 `rememberPairedHost` 一致，内容是
   `{hostId, publicKeyBase64Url}`（只含公开材料）。

**预置的是「已配对」这个起点**；其后的握手、`hostProof` 验签、`auth.client_proof` 签名、
快照两级 SHA-256 校验、命令派发全部走生产代码。**被绕过的是配对页与 HTTPS 轮询面**，
它们登记在第 6 节「未验证项」。

---

## 3. R15 / R16 / R18 逐条覆盖对照

用例总数 **21**（组合根 4 + R15 7 + R16 3 + R18 7）。

### 3.1 R15 目录页以目录为主角且可离线渲染

| 需求条目 | 用例 | 位置（`文件:行号`） |
| --- | --- | --- |
| 目录项只显示展示名与别名；MUST NOT 显示/编辑/推导本机规范化路径 | R15-1（快照 workspaces 注入协议外 `path`/`root`，断言渲染后 DOM 无任何片段） | `e2e/page/checks/r15-directory.ts:93-108` |
| 同名目录之间 MUST NOT 通过路径片段消歧 | R15-2（两条同名目录，别名是唯一区分依据，DOM 无第二依据） | `e2e/page/checks/r15-directory.ts:111-124` |
| 目录内会话数/运行中/待处理由会话摘要得出，断连时仍可呈现 | R15-3（先断连前断言 `live` + 三项计数，再断连断言 `last_sync` 且列表仍在） | `e2e/page/checks/r15-directory.ts:127-160` |
| 「存在目录但其内无会话」与「一个目录都没有」MUST 可区分（目录页） | R15-4（两个场景比对 `data-empty-state` + 标题 + 指引） | `e2e/page/checks/r15-directory.ts:163-190` |
| 同上（目录详情页层面） | R15-6 / R15-7 | `e2e/page/checks/r15-directory.ts:227-259` |
| 目录详情页不泄露路径 | R15-5 | `e2e/page/checks/r15-directory.ts:199-225` |
| 离线仍可渲染 | R15-3（断连后 DOM 仍有两条目录与文本） | 同上 |

被测实现对应位置：`src/components/DirectoryList.tsx:35-79`（只渲染 displayName/alias/三项计数，
组件没有任何接收 path 的 props）、`src/features/directory-model.ts:108-154`、
`src/domain/directory.ts:84-123`、`src/components/DirectoryDetail.tsx:50-88`。

### 3.2 R16 创建载荷只含两个引用

| 需求条目 | 用例 | 位置 |
| --- | --- | --- |
| 载荷只含两个引用；MUST NOT 携带 Owner 路径或 Provider/MCP 凭据；**断言实际发出的字节** | R16-1（点真实的提交按钮，抓 `FakeHost.sent` 里的帧**原文**，断言键集恰好 `{agentId, workspaceAlias}`，且原文不含 `path/cwd/mcp/token/secret/...` 与 `D:`） | `e2e/page/checks/r16-create.ts:109-158` |
| 无权限态可区分 | R16-2（缺 `session.create` scope 时入口 `disabled` + `[data-create-entry-unavailable]` 说明 + 不得发出命令） | `e2e/page/checks/r16-create.ts:159-184` |
| 未认证的拒绝理由与无权限不同 | R16-3（两个模型都由**真实** `store.createEntry()` 在浏览器里算出，交给真实 `CreateSessionSheet` 渲染，断言两条 `data-create-denied-reason` 文案不同） | `e2e/page/checks/r16-create.ts:185-244` |
| 对照：权限具备时按钮可点、命令确实发出 | R16-1 的前后两段断言（未选 Agent 禁用 → 选中后可点） | `e2e/page/checks/r16-create.ts:117-122` |

被测实现对应位置：`src/features/create-session-model.ts:57-112`（`assertCreateSessionRefs` +
就地构造的 payload）、`src/features/client-store.ts:577-604`（四道闸门）、
`src/components/CreateSessionSheet.tsx:87-135`、`src/components/DirectoryDetail.tsx:62-67`。

### 3.3 R18 降级卡片与诊断抽屉

| 需求条目 | 用例 | 位置 |
| --- | --- | --- |
| 原文因单条上限/保留期未下发：MUST 明确说明未同步及原因 | R18-1（卡片给出 `data-raw-not-synced="size_limit"` 与文案） | `e2e/page/checks/r18-degradation.ts:196-232` |
| MUST NOT 渲染占位使其看起来完整 | R18-1（抽屉内 `pre[data-raw-json]` **必须不存在**，且槽位里不得出现 `"`） | 同上 |
| 原文未下发的**元数据**（字节长度、摘要）仍要给出 | R18-1（抽屉文本含 byteLength 与 sha256） | 同上 |
| 三类不支持可区分（client / broker / agent） | R18-2（三张卡片的 `data-unsupported-reason` 与说明文案互不相同） | `e2e/page/checks/r18-degradation.ts:234-263` |
| 未登记事件类型 / 二进制未下发 / 客户端能力不支持三者可区分 | R18-3（未登记：呈现「没有专用视图」，且**不**冒充三类之一）、R18-4（二进制未下发：呈现内容缺失） | `e2e/page/checks/r18-degradation.ts:264-303` |
| 「内容未下发」不得呈现为「客户端不支持」（WP7 第 1 轮 MAJOR-2） | R18-4、R18-5（断言**不出现** `data-unsupported-by` 与「当前客户端未提供专用视图」） | `e2e/page/checks/r18-degradation.ts:284-303`、`:305-317` |
| 诊断抽屉对原文可用的事件给出 `rawJson` | R18-6（抽屉 `pre[data-raw-json="available"]` 与服务端原文逐字一致 + 事件 ID/类型元数据） | `e2e/page/checks/r18-degradation.ts:319-337` |
| 对照：已知事件类型 + 原文可用 → 不产生降级卡片 | R18-7（八条事件里恰好七张卡片） | `e2e/page/checks/r18-degradation.ts:339-355` |

被测实现对应位置：`src/features/degradation-model.ts:147-226`、`src/components/ConversationStream.tsx:32-82`、
`src/components/DegradedEventCard.tsx:50-67`、`src/components/DiagnosticsDrawer.tsx:67-93`、
`src/features/client-store.ts:180-183`。

---

## 4. 组合根接线在真实浏览器中的实测结论

这是本工作包的核心交付：**既有组件测试全部走 `renderToStaticMarkup`，没有事件、没有 `useEffect`**，
因此 `app/_layout.tsx` 里 `await composition.start()` 在任何测试中都没有执行过（`review-wp7-r1` §5.3）。

### 4.1 CR-1：真实的 `app/_layout.tsx`（PASS）

`e2e/page/checks/composition-root.ts:57-91`。直接挂**未经改动的** `app/_layout.tsx` 默认导出，
组合根是**完全无参**的 `createComposition()`：`openPlatform()` 取真实 IndexedDB 与真实
WebCrypto，`openWebSocket` 取真实 `WebSocket`。预置的 `paired-host/<origin>` 指向
`https://127.0.0.1:1`（一个**不存在**的端口），因此连接必然失败。

实测结论（三条断言全绿）：

1. **页面确实离开了 `RuntimeUnavailable`**——`[data-route="dirs"]` 出现且
   `[data-runtime="unavailable"]` 消失。`useEffect → composition.start() → setRuntime()` 这条链
   在真实浏览器里跑通了；`start()` 会真的去读 IndexedDB 里的设备身份与配对记录。
2. 主机面板出现 `[data-host-identity="paired"]`，`data-host-address` 等于预置的
   `https://127.0.0.1:1`——证明组合根读到了预置记录，不是「未配对」。
3. `[data-connection-state="unpaired"]` 不存在——状态机离开了 `unpaired`。

**关于 `dispose()` 不可复活的已知前提（`review-wp7-r2` 的 MINOR）**：本条用例给出的正面证据是
「页面离开 `RuntimeUnavailable`」。当前配置未启用 StrictMode，effect 只调用一次，因此不会触发
「第二次 `start()` 变 no-op」。**未验证项**：若将来启用 `<StrictMode>`，本目录的 CR-1 会**变红**——
这不是缺陷，而是它恰好能提前暴露该前提；届时的修法属于实现变更，不在本工作包范围内。

### 4.2 CR-2：握手 → 快照 → 状态机 → 页面（PASS）

`e2e/page/checks/composition-root.ts:92-126`。`openSocket` 换成页内假主机，其余端口全部生产实现。
实测：`connection.state === "online"`；客户端用 IndexedDB 里那把不可导出的私钥签出**一条**设备证明；
连接 URL 为 `wss://127.0.0.1:1/sync`（由 `syncUrlFor` 从规范化来源推导）；快照经**真实两级 SHA-256**
校验后进入 `ClientStore`（2 个目录、2 条会话、3 个 scope）；目录页渲染出两条目录行。

### 4.3 CR-3：负向对照——验签不是橡皮图章（PASS）

`e2e/page/checks/composition-root.ts:128-153`。假主机故意签一条**错的** `hostProof`
（翻转签名末位，长度与字母表均合法）。实测：连接进入 `identity_changed` 阻断态、页面呈现
`[data-blocking-detail="host_identity_changed"]`、**且客户端一条设备证明都没签**
（验签发生在签名之前）。这同时证明假主机不是「怎么签都能过」。

### 4.4 CR-4：预置记录被改坏 → 按「从未配对」处理（PASS）

`e2e/page/checks/composition-root.ts:154-182`。把 `paired-host/<origin>` 写成非法 JSON。
实测：状态机停在 `unpaired`、**一帧 wire 都没发**、目录页呈现「尚未收到第一份快照」而不是
「本机没有任何目录」，主机面板明确写「尚未配对」。

---

## 5. 变异验证

方法：临时改坏实现 → 跑全套 → 确认相关用例变红 → `git checkout --` 还原 → 确认
`git status --porcelain` 只剩 `clients/app/e2e/`。

| # | 变异 | 预期变红 | 实际 |
| --- | --- | --- | --- |
| M1 | `src/domain/directory.ts` 让 `DirectoryView` 带上协议外的 `path`，`DirectoryList.tsx` 把它渲染进 DOM | R15-1、R15-5 | **未变红**——见「变异验证的局限」 |
| M2 | `ConversationStream.tsx` 把非 `unsupported` 的原因兜底成 `"client"`，且 `noDedicatedView` 恒为 `true`（复现 WP7 第 1 轮 MAJOR-2） | R18-3、R18-4、R18-5 | **变红**（全套 17/21）——有效 |

### 变异验证的局限与后续（**r2 已更正：不是判别力缺口，是变异没触达被测面**）

> **更正（r2，依据 `review-tp4-r1` 的 TP4-2 与 r2 的独立复现）**：下面这段把「M1/M3 未变红」
> 登记成「本轮遗留的判别力缺口」**与证据不符，已撤销**。正确结论是：**单层变异未触达被测面**
> （M1 只改 domain、M3 只改组件，二者各自都到不了渲染面）。审查方用**三层接缝**
> （`directory.ts` 的 `DirectoryView.path` → `directory-model.ts` 的 `toRow` →
> `DirectoryList.tsx` 里一个合法文本节点）复现后 **R15-1/2/3 全部变红，且红法干净**
> （失败信息为「文本里出现了 `"D:\"`」，即检测到泄露，不是崩溃或超时），r2 已独立复现同一结果
> （见 `deliver-tp4-r2.md` §5）。**R15 的负向断言判别力充分，无需补测试**；M1 应记为
> **无效变异**（按 spec，MUST NOT 约束的是**呈现**，未被渲染的字段不违反任何 MUST）。

M1 与 M3 都没有变红，这**不是**「断言够强」，而是**变异选错了接缝**。

R15「不泄露本机路径」这条 MUST NOT 的真正守门人是**类型边界**，不是组件代码：
`DirectoryRowModel`（`src/features/directory-model.ts:108-118`）与 `DirectoryList` 的 props 里
**根本没有 path 可渲染**；`buildDirectoryViews`（`src/domain/directory.ts:108-117`）也不把
`WorkspaceRef` 之外的字段带进 `DirectoryView`。于是：

- 只改组件（M3）→ 没有字段可渲染，DOM 里不会出现路径；
- 只改 domain 层（M1）→ 字段停在 `DirectoryView`，到不了 `DirectoryRowModel`，组件仍渲染不到。

**要构造有效变异，必须同时改 `directory-model.ts` 的 `toRow`（把 path 带进行模型）与
`DirectoryList.tsx`（把它渲染出来）**——跨这两层的接缝才有可观测的泄露面。

~~如实登记：R15 的「不泄露路径」在真实浏览器里目前是**通过**的，但尚未被证明**能红**。
这是本轮遗留的判别力缺口，下一轮应补上这个双层变异。~~
**（r2 更正，已撤销）**：上面这句与证据不符。r2 独立复现了上面描述的跨层变异
（`directory.ts` → `directory-model.ts` 的 `toRow` → `DirectoryList.tsx` 一个合法文本节点，
`tsc` 零诊断、页面正常渲染），**R15-1/2/3 全部变红且红法干净**（失败信息为
「文本里出现了 `"D:\"`」，即检测到泄露，不是崩溃或超时）。因此 R15 的负向断言
**判别力充分、已被证明能红**，**不需要**补测试；M1 应记为**无效变异**（未触达呈现面）。
证据见 `deliver-tp4-r2.md` §5 与 `PV3-tp4-r2.log` §5。
与之对照，M2 证明 R18 那组负向断言**确实能红**（MAJOR-2 回归被抓住）。

---

## 6. 发现的实现缺陷

### MAJOR-1：目录存在但无会话时，目录详情页把它说成「链接已失效」

- 复现：`e2e/page/checks/r15-directory.ts:227-244`（**当前为红**，全套 20/21）
- 现象：快照里 `workspaces` 有 `alpha`、`sessions` 为空时，目录详情页渲染的是
  `[data-directory-detail-state="unknown_directory"]`——「目录『alpha』不在最近一次同步的目录列表里。
  它可能已被删除，或这个链接来自更早的一次同步」。
- 根因：`src/features/directory-model.ts:188-193`
  ```ts
  const list = buildDirectoryViews(...);
  const directories = list.kind === "ready" ? list.directories : [];
  const directory = directories.find((candidate) => candidate.alias === input.alias);
  if (directory === undefined) return { kind: "unknown_directory", alias: input.alias };
  ```
  `buildDirectoryViews`（`src/domain/directory.ts:118-121`）在「有目录、但一个会话都没有」时返回
  `kind: "empty"` 而不是 `ready`。于是详情页**看不到那个确实存在的目录**，把
  「目录存在但无会话」与「别名失效」合并成同一种呈现——正是 R15 要求区分的那一对。
  目录**列表页**（R15-4）是对的，只有详情页这一层漏了。
- 影响：用户会以为自己的目录被删了或链接过期，实际只是还没在里面建过会话。
- 处置：**未修改实现**（本工作包只写测试）。按要求保留能红的用例。

### 未发现问题的部分

R16、R18 的全部断言在真实浏览器里通过；组合根接线（CR-1..CR-4）全绿。

---

## 7. 未验证项（如实登记，不用替身制造「已验证」）

1. **真实配对流程**（扫码 / `pairing` 轮询 / `rememberPairedHost` 的生产调用方）——运行时不可达，
   本目录预置了「已配对」这个起点（第 2 节）。
2. **真实 `wss://` 传输**——CR-1 走的是真实 `WebSocket` 但目标是**不存在的端口**（只验到「连不上也不崩」）；
   CR-2..CR-4 把 `SocketPort` 换成页内假主机。TLS、代理、真实服务端协议编排在本目录之外。
3. **真实 expo-router 的 URL 同步与历史栈**——`app/` 的路由组件用的是本目录的替身
   （`router-stub.tsx`），只提供 `Stack`/`useRouter`/`useLocalSearchParams`。被测面是组合根、
   feature 判定与展示组件，导航栈本身不在 TP4 范围。R15-5 因此直接挂详情路由，
   不经过「点目录行跳转」。
4. **React StrictMode 下的 `dispose()` 不可复活**（`review-wp7-r2` 的 MINOR）——当前未启用 StrictMode；
   本目录能证明的是「页面确实离开 `RuntimeUnavailable`」（CR-1）。
5. **`session.read` 分页续取**（R17）——假主机只回空页；TP4 的交付范围是 R15/R16/R18。
6. **跨连接恢复游标的持久化**——`createComposition` 里 `resumeCursor: null`，本目录不覆盖。

---

## 8. 与门禁的关系

| 命令 | 期望 | 说明 |
| --- | --- | --- |
| `node e2e/run-e2e.mjs` | 退出码 1（**20/21**） | 唯一的红是 MAJOR-1 的能红用例，按要求保留 |
| `npx tsc --project e2e/tsconfig.json --noEmit` | 0 诊断 | e2e 独立类型检查 |
| `npx tsc --noEmit`（app 主 tsconfig） | 0 诊断 | e2e 不在主 tsconfig 的 include 内，因此单独门禁 |
| `npx vitest run` | 全绿 | 未改实现 |
| `node scripts/run-browser-check.mjs` | 16/16 | 未改 WP5b runner |
| `npm run check`（仓库根） | 前 9 道全绿；`check:agentic` **因环境前置缺失失败**（exit=1） | 见下方说明 |

**关于 `node e2e/run-e2e.mjs` 的退出码语义（r2 更正，依据 `review-tp4-r1` 的 TP4-4）**：
上表把「退出码 1」写成期望值有风险。`e2e/run-e2e.mjs:301-308` 的逻辑是 `failed.length > 0`
即退出 1，**「存在已知红」与「出现真回归」在退出码上不可区分**。正确口径是
「红集合 = 期望红（当前为 `{R15-6}`）」，**R15-6 修好后 e2e 变全绿、退出码 0 是修复成功的标志，
不是门禁失败**。详见 `deliver-tp4-r2.md` §6.2。

**关于 `npm run check` 的第 10 道门禁**：`check:agentic` 报
`缺少 node_modules/@dongfanglin/openspec-agentic/bin/openspec-agentic.mjs，请先在仓库根运行 npm ci`。
该包在**主检出**的 `node_modules` 里存在，但 **worktree 不共享 `node_modules`**，因此该门禁在
worktree 里必然失败；而本工作包被明确禁止在仓库根跑 `npm ci` / `npm install`。
这不是本次改动引入的回归（本次只新增 `clients/app/e2e/` 下的文件，实现与配置零改动），
是**环境前置缺失**。在有根 `node_modules` 的检出上重跑该门禁即可。