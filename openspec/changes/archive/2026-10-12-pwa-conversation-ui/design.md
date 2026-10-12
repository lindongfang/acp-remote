# Design

动机与范围见 `proposal.md`；行为要求见 `specs/`。下面是制定方案需要的现状与决定（现状都已核对到具体位置）。

## Context

- **对话页现状**：`clients/app/app/session/[id].tsx` 只挂 `ConversationStream`（只读一栏）+ `ContextUsageBar` + `DiagnosticsDrawer`；`ConversationStream`、`ContextUsageBar`、`DegradedEventCard`、`DiagnosticsDrawer` 四个文件**零样式**（无 `theme.ts`、无 `StyleSheet`），而目录页已迁到 RNW 原语 + token。
- **事件消费现状**：`src/features/client-store.ts` 的 `#withEvent` 只按名字认 5 种事件类型；`src/features/degradation-model.ts` 用 `isKnownEventType`（= 34 个 `$defs` 里有这个名字）当「客户端有专用视图」。实测：8 条带完整原文的结构化事件（delta / 工具调用 / 权限请求 / 终端 / plan / file / config / mode）入 store 后 `messages: 0, degraded: []`。
- **结构化历史的供给现状**：`agentContentBlock` 只有 `text`/`image_ref`/`resource_ref`/`unsupported`；`snapshotItem.turns` 是闭合对象（`turnId`/`state`/三个时间戳/终态错误）。工具调用、终端、plan 只在实时事件流里存在，而订阅从快照 head 开始、ACK 只有全局 cursor。
- **Owner 侧的原料是齐的**：`owned_event` 行里同时有 `payload_json`（视图）、`acp_raw_json`/`acp_raw_unavailable_reason`、`session_sequence`、`created_at`，并且有唯一索引 `owned_event_session(session_id, session_sequence)`；`storage-sqlite::read_session` 已按会话、按 `global_sequence` 升序、跳过 `compacted_into IS NOT NULL` 的行取一页事件——**但它返回的 `HistoryPage.events` 只有信封**（`CommittedEvent`：id/type/序号/时间），没有视图与原文。
- **端口与门禁的形状**：`ReadView::{read_session, event_payload, replay, node_link_slice}`；`node_link_slice` 已经返回**带正文**的 `OwnedEventRecord`，按 origin 序号分页。`docs/CORE_PORTS_AND_STORAGE.md` §5 的 rust 块与 `crates/core/src/ports.rs` 由 `scripts/check-contract-drift.mjs` 逐成员比对；`crates/sync-protocol/tests/schema_drift.rs` 逐条比对 schema 与 Rust 镜像。
- **Node Link 面与 imported**：Access 回源 Owner 依赖的 `node-link-client` 待落地；`sync-snapshot-scope` 已有 Requirement 规定「分页只作用于客户端同步面」。客户端 `SUPPORTED_FEATURES` 里含 `resource.remote-origin.v1`。
- **约束**：ACP 内容不改（`acp.rawJson` 字节保真、扩展字段往返保真）；v1 无破坏性变更；不新增命令名与载荷字段；样式复用既有 token。

## Goals / Non-Goals

**Goals:**

- 让「会话的结构化历史」有一条**已定义、可断言**的读取路径，且历史条目与实时事件**同形**——客户端只需一套解析与一套卡片。
- 把客户端「我会渲染什么」的判断收回客户端自己，并让「没有视图」这条路上的结果**只能是降级卡片**。
- 对话页的交互闭环（输入/取消/授权应答）只用既有命令与既有 wire，不扩协议语义。
- 在不破坏 CSP 与「Agent 内容一律不可信」的前提下引入 Markdown 与语法高亮。

**Non-Goals:**

- 不做 `server::sync` 的投影实现（本变更只把 wire 合同 + core/storage 读路径补齐）。
- 不做 Node Link 面的结构化历史（imported 会话在客户端显式降级）。
- 不做统一时间线的服务端合并（见 D2 的备选甲）；不做客户端事件重放重建会话（见 D6 的备选）。
- 不解析 `acp.rawJson` 还原结构化差异正文。
- 不新增 DDL、不做数据迁移。

## Decisions

### D1 历史条目复用事件视图形状

条目携带：事件标识、事件类型、会话内序号、创建时间、`view`，以及按 `acp.raw-payload.v1` 协商结果出现的 `acp`（原文或带原因的不可用）。

理由：客户端已经有一套 `projectView(eventType, view)` 与一张卡片/降级表；历史若另造形状，等于**两套解析、两套降级、两套测试**，而它们描述的是同一批事件。复用后「实时 vs 历史」只剩数据的来源差异。

备选：服务器先把 `tool.call.started/updated` 聚合成一条「步骤」再下发。否决——那是把判定搬到服务端（且要新增状态），而客户端为实时路径**本来就要做同样的归并**（同一 `toolCallId` 的三条事件），复用归并逻辑比新增一套服务端聚合更省、更保真。

### D2 分页用**会话内序号**，与消息分页完全分离

请求新增可选的 `beforeHistory`（十进制字符串，会话内事件序号），结果新增 `history` 与 `historyHasEarlier`；既有 `before`/`hasEarlier`（消息）一字不改。

理由：① `owned_event_session(session_id, session_sequence)` 是**已有的唯一索引**，按序号分页零 DDL；② 会话内序号在提交时分配，天然单调唯一，**不需要同刻并列的次级排序规则**，也不会出现「同一 `createdAt` 的多条条目」这类边界；③ Node Link 的会话切片已经是用序号（origin 序号）分页，口径一致。

备选：
- **把 `hasEarlier` 挪进每个资源对象**（每个资源自带页标记）：改既有字段位置 = v1 破坏性变更，否决。
- **规定「一次请求最多一种可分页资源」**：省，但 `before` 的语义变成上下文相关，规范里只能靠文字约束而不被 schema 表达，否决。
- **游标用 `(createdAt, eventId)` 复合**（与消息同一口径）：需要为「按时间排序」新建索引（DDL + migration + §7 SQL 块 + 漂移门禁）并保留并列次序规则，收益只是"两个游标形式相同"。它是**可接受的备选**，若评审更看重游标形式统一，可改；本设计按成本更低、边界更少的序号方案定稿。
- **甲：服务端合并成一条统一时间线资源**（消息与事件同一个游标）：客户端只有一个页边界，最贴近原型；但服务端要在两个 id 空间之间做合并与并列排序（现有 fixture 里已经出现过同刻时间样例），且等于把消息资源的权威复制一份。否决。

### D3 历史条目的类型集合：17 进 / 17 不进

不进的是四类：**承载消息正文**的（`user.message.delta`、`agent.message.delta`、`agent.thought.delta`、`agent.message.completed`——正文权威在 `messages`，两份权威会打架）、**摘要/仪表类**（`session.created`/`session.updated`/`session.info.changed`/`session.usage.changed`/`session.commands.changed`/`session.origin.online_changed`——已有专门承载）、**命令终态类**（`command.completed`/`command.failed`/`command.uncertain`——由命令结果承担）、**连接审计类**（`agent.connected`/`agent.disconnected`/`device.revoked`/`turn.delta_compacted`）。

进的是时间线上有意义的结构化事件：`turn.{queued,started,completed,cancelled,failed}`、`tool.call.{started,updated,completed}`、`permission.{requested,resolved}`、`elicitation.{requested,resolved}`、`terminal.output`、`file.changed`、`session.plan.changed`、`session.mode.changed`、`session.config.changed`（共 17）。

两个附带口径：① 这个集合是**时间线语义**的判断，不是「客户端支持什么」——后者归客户端（D5），因此服务器不因某个客户端没有卡片就改集合；② 已按保留策略压缩的行（`compacted_into IS NOT NULL`）不产生条目，所以**思考内容不承诺出现在历史里**（`agent.thought.delta` 没有收尾事件，压缩后即消失）——这条要如实写进文档，不能假装有。

### D4 读路径在 core/storage 侧补齐「带正文的按会话分页」

把会话历史读的返回从「只有信封」改成「信封 + 持久化正文」：即 `HistoryPage` 的事件元素携带视图与原文（复用既有 `OwnedEventRecord`/`EventPayload`/`AcpRaw`，不新增值对象），`HistoryInclude` 增加对应开关，`storage-sqlite` 在读行时一并取 `payload_json` 与 ACP 两列。

理由：原料本来就在同一行里；不这么做，未来的 `server::sync` 只能对每条事件再调一次 `event_payload`（N+1），页大小一上来就退化成往返风暴。改动是**端口形状**（受 `check:drift` 约束，必须同批改 `docs/CORE_PORTS_AND_STORAGE.md` §5 的 rust 块），不是 DDL。

备选：新增一个专用端口方法（如 `session_history(session, after, limit) -> Vec<OwnedEventRecord>`）。更小、更不影响既有调用方，但会让「按会话读历史」有两个入口（`read_session` 与它），且 §5 同样要改。**可接受**；若实现期发现 `read_session` 的既有调用方（`use_cases`/fake 读视图/测试）改动面比预期大，就换这条。

### D5 客户端自持「呈现覆盖」，服务端词表不再充当它

新增一份客户端自己的**覆盖集合**（事件类型 → 有专用视图），与「协议认识哪些类型」彻底分开：协议词表只决定**解析**，覆盖集合决定**呈现**。判定改为「被消费的事件要么在覆盖集合里（走卡片），要么走降级卡片」，**没有第三种结果**。

理由：这正是 `docs/FRONTEND_DESIGN.md` §6「客户端呈现能力由客户端自己维护」的字面要求，也是 R18 现在不成立的原因。附带好处：这份集合本身就是「还缺哪些卡片」的清单——每实现一个卡片就收窄一处。

代价：覆盖集合**必须覆盖历史条目集合里客户端声称支持的每个类型**，否则用户会在时间线上看到一堆降级卡片。这条是本变更的检视点之一（见下）。

### D6 实时尾巴 = 按消息标识的易失叠加层

`messageId` 在 `agent.message.delta`/`user.message.delta`/`agent.thought.delta` 与 `session.read` 的 `messages` 元素里是**同一个标识**，因此叠加层按 `messageId` 累加即可，读回来的同 id 内容直接替换；`turn.delta_compacted` 到达时作废该轮次的累加。叠加层只在内存、随页面存活，「正文只来自 `session.read`」这条不变量不破。

备选：把 `session.read` 也接进事件流做增量合并（双写正文）。否决——那就是两套权威，与 `client-store` 现有的 D8 口径（明细一律现取）冲突。

### D7 授权应答只对本设备亲历的请求开放

`pending_interactions` 的读出项只有 `interactionId`/`kind`/`state`/`schema`/`createdAt`——**没有可选项**；核心层刻意不持久化 `options`（注释明确「`options` 只存在于事件 payload 里，是它的唯一权威来源」）。因此只有收到过 `permission.requested`/`elicitation.requested` 的客户端才有 `optionId` 可提交。历史待处理项呈现为不可操作并说明原因。

顺带登记一个**协议待澄清项**：`snapshotItem.pending_interactions` 的 `schema` 与 `state` 的取值没有任何文档定义（`schema` 是自由对象、`state` 是自由字符串）。它不影响本变更（本变更不依赖它们），但晚到的客户端要能应答已存在的授权，就必须先把它们定义清楚。

### D8 右栏只用结构化统计，原文只做诊断入口

`file.changed` 的视图只有 `changeId`/`kind`/`displayPath`/`summary`/`addedLines`/`deletedLines`/`outsideWorkspace`——**没有差异行**。因此右栏呈现统计与相对化路径，并把 `acp` 原文放进既有诊断抽屉；**不**为了填满右栏去解析原文再渲染。副作用要如实呈现：`addedLines`/`deletedLines` 只覆盖 Agent 在工具调用里声明的改动，判定不出时省略而不是填零。

### D9 依赖选型与不可信输入边界

新增两个依赖：Markdown 渲染与语法高亮。硬约束（来自 `FRONTEND_DESIGN.md` §4.4）：渲染成 RN 视图而不是 WebView/HTML、构建期打包、运行时不取任何 CDN、可关掉原始 HTML、不解析 `acp.rawJson`。

- Markdown：`react-native-marked`（`marked.js` 驱动，RN + web）为首选；备选 `@ronradtke/react-native-markdown-display`（纯 JS 无原生代码）；再退一步是 `marked` + 自建 RNW 映射（可控性最好，但要自己写块级/内联映射）。
- 高亮：`highlight.js` 的 core + 按需注册语言，自己拼等宽视图；备选 `react-native-syntax-highlighter`（省事、但维护弱、包体大）。

**先做可用性探针再进实现**：本工程把版本钉死（React 19.2.8 / RN 0.86.3 / RNW 0.21），老牌 RN markdown 库的 peer 范围常与 React 19 冲突，`npm ci` 或 `expo export` 会先失败。探针不通过就按上面的顺序降级，**不允许**为了装上依赖而放宽 React/RN 版本。

### D10 窄屏形态与草稿边界

宽屏：时间线 + 并列的变更文件视图；窄屏：变更文件降为可切换的次级视图（沿用既有断点与「窄/宽信息集合相同」的断言口径）。草稿只在内存保留，imported 会话的草稿 MUST NOT 落盘（`FRONTEND_DESIGN.md` §7 的 `no-content-cache` 边界）。

### D11 Node Link 面不动

只在 Sync 面的 include 枚举与 schema 里加取值；Node Link 自己的 schema/文档保持原样。理由：`sync-snapshot-scope` 已有 Requirement 要求 Node Link 面行为不因 Sync 分页变化，而 Access 回源 Owner 依赖的 `node-link-client` 还未落地——现在两面一起加只能是纸面合同。imported 会话在客户端显式降级，并按 §「文档维护」把它记为已知缺口。

## Compatibility

- **v1 无破坏性变更**：只新增可选请求字段、新增结果字段、新增 include 取值与新增资源键；既有 `before`/`hasEarlier`/`messages` 语义不变。
- **feature 门控**：新增一个非必需 feature ID；服务器不广告它时客户端不带该 include，服务器对未协商却请求该 include 的客户端返回明确的不支持类错误（不静默返回空结果）。
- **合同资产联动**（漏一处就会被门禁拦下）：`schemas/sync/v1/**` ↔ `crates/sync-protocol/src/{sync,command,views}.rs`（`schema_drift.rs` 双向）；`fixtures/sync/v1/**` + `manifest.json`（正例 + 「明细资源不入快照」型负例）；`compatibility/features/v1/features.json` ↔ 协议 §5.2 表（`check:features`）；`docs/CORE_PORTS_AND_STORAGE.md` §5 ↔ `crates/core/src/ports.rs`（`check:drift`）；`clients/app/src/protocol/**` ↔ 同一份 schema（`contract.test.ts`）。
- **Node Link**：`schemas/node-link/v1/**` 与 `docs/NODE_LINK_PROTOCOL.md` 不改。

## Migration Plan

- **无数据迁移**：不新增表、不新增列、不改约束（按会话分页所需的唯一索引已存在）。若实现期发现确实需要新索引，必须走既有版本化 migration，并同批更新 `docs/CORE_PORTS_AND_STORAGE.md` §7 的 SQL 块与 `check:drift`，不得就地改 DDL。
- **回滚**：本变更是叠加式的。回滚 = 还原代码与合同资产；已落库的事件数据无需回滚（新资源只是既有行的另一种读法）。若协议资产已发布而实现回滚，服务器只需停止广告该 feature，客户端自动退回不带该 include 的旧行为。
- **发布顺序**：Rust 侧（合同 + 读路径）可与前端并行开发，但前端依赖合同形状，故合入顺序为先合同、再读路径与客户端，最后浏览器用例。

## Risks / Trade-offs

| 风险 | 缓解 |
| --- | --- |
| 新增依赖与 React 19 / RN 0.86 的 peer 范围冲突 | 先做可用性探针（`npm ci` + typecheck + `expo export`）；不通过按 D9 的顺序降级；禁止为此放宽框架版本 |
| 覆盖集合与历史条目集合不对齐，时间线上出现成片降级卡片 | 把「覆盖集合 ⊇ 历史条目集合中客户端声称支持的每个类型」写成检视点与浏览器用例的断言 |
| 历史条目带原文导致页体积膨胀 | 复用既有 `limit` 与单条上限；原文按 `acp.raw-payload.v1` 协商与否出现；不可用时按已登记原因标记而不是截断冒充 |
| 端口形状改动（D4）面比预期大（fake 读视图、既有测试、§5 rust 块） | 备选 D4 的第二条路径（新增专用端口方法）保留在设计中；两种都在同一工作包内完成，不跨包 |
| 序号游标与会话压缩/清理的交互被误读成「丢条」 | 保留窗口之前显式返回保留期已过类错误（spec 有场景）；实现期用「压缩后不可读」的断言固定 |
| 实时尾巴与读取结果交替时出现重复或闪烁 | 以 `messageId` 为唯一键替换而不是追加；浏览器用例覆盖「先增量、后读取」这一序列 |
| 与在飞的 `pwa-directory-ui-followup` 抢 `e2e/**` 与页面文件 | 本变更排在其后；`plan.md` 的 Merge Order 声明该前置与写范围边界 |

## Open Questions

- 语法高亮初版注册哪些语言（先按 fenced code 里最常出现的几种，后续按需增删）。
- 窄屏「次级视图」的入口形态（顶部切换 vs 底部 tab）与移动原型对齐到什么程度——沿用既有断点，具体入口在实现期定。
- `snapshotItem.pending_interactions` 的 `schema`/`state` 语义（D7 登记的协议待澄清项）——不影响本变更，留给 Node Link 面或后续变更。
