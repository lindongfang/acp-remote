<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

目录页与目录详情页已经有界面（`6a11291`，归档于 `2026-10-12-pwa-directory-ui`；欠项由在飞的 `pwa-directory-ui-followup` 收尾），但**对话页仍是只读的一栏**：`clients/app/app/session/[id].tsx` 只挂了一个只读正文流、三态上下文条与降级卡片，且这套组件没有任何样式。用户进得去、看得见消息，但**发不了消息、停不了 turn、应答不了权限请求**——而 `docs/FRONTEND_DESIGN.md` §4.1 把这些列为「必须实现」，「结构化工具调用、权限请求、终端、文件修改和 diff 的基础展示」与 §9 验收第 5 条同样如此。

核对中另有两个**不是"还没做 UI"而是"边界没定/已失真"**的问题，本变更一并处理：

1. **结构化事件被静默丢弃。** `src/features/degradation-model.ts` 把「事件类型在 `event-views.schema.json` 的 34 个 `$defs` 里」当作「客户端有专用视图」，而 `src/features/client-store.ts` 的 `#withEvent` 只按名字消费 5 种事件类型（`session.created`/`session.updated`/`session.usage.changed`/`agent.connected`/`agent.disconnected`）。于是**其余 29 种已知事件既不渲染、也不生成降级卡片**——实测把 8 条带完整原文的结构化事件（消息 delta、工具调用、权限请求、终端输出、plan、文件改动、config、mode）喂进 store，结果是 `messages: 0, degraded: []`。这与 §6「客户端呈现能力由客户端自己维护」以及 R18「没有专用视图的结构化事件 MUST NOT 被静默隐藏」直接冲突。
2. **结构化历史没有读取路径。** `agentContentBlock` 只有 `text`/`image_ref`/`resource_ref`/`unsupported` 四种；`snapshotItem.turns` 是闭合对象，只有 `turnId`/`state`/时间戳。也就是说工具调用、终端、plan 这些事件**只在实时事件流里存在**，而新设备入网从快照 head 订阅、ACK 只有全局 cursor——**重开页面看不到连接之前发生过的结构化条目**。同时 `session.origin.online_changed` 也被丢弃，R20 的「来源离线」标注因此拿不到触发源。

**用户要求**：只做对话页。前提是**复用 ACP 协议、不改变 ACP 内容**；结构化历史按「扩 Sync wire」方案做（新增一类资源，不碰 ACP）；右栏只给统计、路径与诊断入口，不去解析 ACP 原文还原 diff 行；Markdown 要引渲染库与语法高亮库；单独一个变更，排在 `pwa-directory-ui-followup` 之后。

**硬约束**：

- **ACP 内容不变**：`acp.rawJson` 字节保真（不得解析后重新序列化）、不改 ACP 方法与内容块语义、未知字段与扩展 payload 继续往返保真。新增能力只落在 Sync 边界。
- 不新增命令名、不新增既有命令的载荷字段：对话页只用既有 `session.prompt`/`session.cancel`/`permission.resolve`/`session.config.set`/`session.mode.set`/`session.config.list`。
- v1 无破坏性变更：不改既有字段的位置与含义（`before`/`hasEarlier` 仍是消息的分页语义），新增一律为可选字段或新增资源。
- Node Link 面行为不变（`sync-snapshot-scope` 既有 Requirement「分页只作用于客户端同步面」）；imported 会话的结构化历史**显式降级**，不静默、也不在本变更里实现。
- 样式继续用既有 `src/components/theme.ts` 的 RNW 原语与 token，不加第二套样式体系；判定仍全部落在 `src/features/` 的纯函数里，页面没有业务分支。
- 不触碰在飞的 `pwa-directory-ui-followup` 的写范围（`e2e/**`、`app/index.tsx`、`router-stub`）。

**成功判据**：对话页可发消息、可取消当前 turn、可应答权限请求，且这三者的呈现一律以服务端终态为准（不做「已发送」的本地断言）；工具调用/终端/计划/权限卡片在**实时与历史两条来源上走同一套渲染**，重开页面能看到本机自有会话的历史结构化条目；29 种事件不再静默丢弃（客户端自有的覆盖表 + 降级卡片）；imported 会话显示明确的结构化历史降级说明；Markdown 与语法高亮可用且不违反 CSP（不加载远端资源、不渲染 raw HTML、不解析 `acp.rawJson`）；`npm run check`（仓库根）、`cargo test`、`clients/app` 的 `npm run check` 与 `node e2e/run-e2e.mjs` 全绿。

## What Changes

**一、Sync 面新增一类会话明细资源：结构化历史条目（wire 半）**

- `session.read` 的 `include` 增加一个取值（历史条目），结果新增可选的历史条目数组与**独立的**「是否还有更早」布尔值；既有 `messages` 的 `before`/`hasEarlier` 语义**一字不改**。
- 历史条目复用**事件视图**形状：定位字段 + `eventType` + `view` + 可缺席的 ACP 原文。因此历史与实时走同一套解析、同一张卡片，客户端不需要为两条来源各写一套。
- 条目按**会话内序号**（会话级单调、提交时分配）分页，索引已存在；同一会话以同一游标重复读取结果相同。
- 协议文档显式列出**哪些事件类型进入历史条目**（工具调用、权限、elicitation、终端输出、文件改动、计划、turn 生命周期、模式/配置变化），消息正文的权威仍然只在 `messages` 资源——避免同一段文本两份权威。
- 新增一个 feature ID（非必需）：服务器不广告它时客户端不带该 include。
- 在 core 与 storage 侧补齐**「按会话、带正文、可分页」**的读路径——今天 `HistoryPage.events` 只带事件信封（无 `view`、无原文），逐条取正文会退化成 N+1。

**二、客户端呈现能力自持（覆盖表）**

- 新增客户端自有的「呈现覆盖」判定：只有**本客户端确实有视图**的事件类型才算被覆盖；其余一律走 R18 降级卡片（含原文与元数据入口、三类不支持原因可区分）。
- 修掉 `session.origin.online_changed` 被丢弃：来源离线状态按该事件更新，R20 的标注随之可达。

**三、对话页交互闭环与时间线（前端半）**

- 时间线渲染：消息与结构化条目按时间交错，共用一套卡片；覆盖表里已实现的卡片（思考、工具调用、终端输出、计划、权限、文件改动）与降级卡片并列。
- 实时尾巴：消息 delta 按 `messageId` 累加为易失叠加层，`session.read` 返回同 id 的完整正文后叠加层退场（「正文只来自 `session.read`」这条不变量不破）；`turn.delta_compacted` 到达时作废该 turn 的累加内容。
- 输入与命令：输入框 + 发送/取消，`Enter` 发送 / `Shift+Enter` 换行，仅文本与「附件暂不支持」说明；开放/进行中/已关闭三态；提交与取消都只在收到终态后改变呈现。
- 授权应答：权限卡（允许一次/始终允许/拒绝）与 elicitation 应答；**只对本设备连接期间亲历的请求提供应答**，读回来的历史待处理项显示为不可操作并说明原因（其可选项不在下发内容里）。
- 模型与模式：来自 `config_options`（`category=model`）与模式状态，含「下一轮生效」与不支持态；选择走既有命令。
- 右栏「变更文件」：文件列表 + 行数统计 + 相对化展示路径 + 「查看 ACP 原文（诊断）」入口；**不**还原 diff 行。
- 窄屏：桌面三栏；窄屏把右栏降为可切换的次级视图，沿用既有断点与「窄/宽信息集合相同」的断言口径。草稿只在内存保留。
- 依赖：新增 Markdown 渲染库与语法高亮库（运行时零远端资源，禁 raw HTML），并让其实用样式走既有 token。

**本次不包含**：

- `server::sync` 的投影实现（另开变更；本变更只把 wire 合同与 core/storage 读路径补齐）。
- Node Link 面的结构化历史（记为已知缺口：imported 会话在客户端显式降级）。
- diff 正文行（不解析 `acp.rawJson` 还原）。
- 配对页、`app/hosts.tsx`、顶栏/hostchip/开发模式横幅/主题开关、PWA manifest/service worker/图标、i18n。
- 目录页与目录详情页（`pwa-directory-ui-followup` 的范围）。
- 主题的自定义开关（跟随系统）。

## Capabilities

### New Capabilities

- `sync-session-history`: Sync 面按会话读取**结构化历史条目**的契约——条目形状（复用事件视图 + 可缺席的 ACP 原文）、进入条目的类型集合、会话内序号游标与独立「是否还有更早」、feature 门控、保留期内的显式不可读，以及它与既有消息分页互不影响的口径。

### Modified Capabilities

- `sync-snapshot-scope`: 「快照只承载清单类资源」的明细资源集合加入新增的历史条目资源（快照仍 MUST NOT 承载任何明细），并明确新增资源同样不改变 Node Link 面的行为。
- `pwa-web-client`: 对话页的行为口径——时间线与结构化卡片（实时与历史同一渲染）、实时尾巴与 `session.read` 的权威关系、输入与取消、授权应答、模型与模式、客户端自持的呈现覆盖与降级、来源离线标注、窄屏形态。

## Impact

- **协议资产**：`schemas/sync/v1/sync.schema.json`（新条目形状）、`command.schema.json`（`session.read` 的 include 与结果）、`fixtures/sync/v1/**` + `manifest.json`（新增正例/负例）、`compatibility/features/v1/features.json`（新增 feature ID）、`docs/SYNC_PROTOCOL.md`（§5.2 feature 表、§9.4 明细资源集合、§11.5 `session.read` 行）。
- **Rust**：`crates/sync-protocol/src/{sync,command,views}.rs`（schema 的 Rust 镜像，`tests/schema_drift.rs` 逐条断言）；`crates/core/src/ports.rs` + `docs/CORE_PORTS_AND_STORAGE.md` §5（端口签名，`check:drift` 逐条断言）；`crates/storage-sqlite/src/session_store.rs`（带正文的分页读）；`crates/core/src/{broker,use_cases}.rs` 的读视图与用例层。
- **前端**：`clients/app/src/protocol/**`（新资源 DTO，与 schema 一一对应）、`src/sync-client/**`（消费新结果）、`src/features/**`（覆盖表、时间线、输入/授权/配置模型）、`src/components/**`（卡片、输入区、右栏、Markdown 渲染）、`app/session/[id].tsx`、`e2e/page/checks/**`、`package.json`（两个新依赖）。
- **权威文档**：`docs/FRONTEND_DESIGN.md`（§4.1 的对话页口径、§6 的呈现覆盖自持）、`docs/CORE_PORTS_AND_STORAGE.md`（§5 端口、§7 若不新增索引则不动）、`docs/SYNC_PROTOCOL.md`；`prototypes/IMPLEMENTATION-GAPS.md` 的对照表同步。
- **依赖**：新增两个 npm 依赖（Markdown 渲染、语法高亮），先做可用性探针再进实现。
- **风险**：新增依赖的 peer 范围可能与本工程钉死的 React 19 / RN 0.86 不兼容；新资源的排序与游标口径必须一次定死，否则未来 `server::sync` 实现会与合同漂移。
