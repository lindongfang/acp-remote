# PWA 实现缺口清单

> **这不是权威文档。** 它是「对照设计原型，当前还差什么」的现状记录，供后续写 openspec 变更提案时切分任务用。
> 产品行为、协议语义、安全模型仍以 `docs/` 下的权威文档为准；若本文件与权威文档冲突，以权威文档为准。
>
> 本轮更新：2026-10-12，依据当前分支实测（桌面原型 `prototypes/acp-remote-pwa.html`，
> 已实现代码在 `clients/app/`），并把对话页（§2.1）按在飞变更 `pwa-conversation-ui` 的写范围重新登记。
> 该变更只覆盖对话页（不含配对页、`hosts` 页壳与主题开关），其模型层、呈现层与浏览器用例与它同批进入主检出，
> 因此凡由它交付的条目一律标为「本变更范围内、尚未落地」——该标记指「代码尚未进入主检出」，
> 不是「设计上不做」；文档其余段落仍以 2026-10-11 的实测结果为准（本轮只补对话页的归属标注）。
> 上一版写于前端工程存在之前（「无 `clients/` 目录」），
> 那部分已整体过时——**前端工程与逻辑层已落地，且 C1/C2/C3 合同也已经补齐**。

---

## 0. 结论速览

| 层 | 状态 | 证据 |
|---|---|---|
| 合同（目录字段 / Sync `session.create` / Agent 目录 / 快照范围） | **已落地** | `openspec/specs/sync-workspace-catalog`、`sync-session-create`、`sync-snapshot-scope`；`compatibility/commands/v1/commands.json` 的 `session.create` 已含 `transport: ["sync"]`；`schemas/sync/v1/common.schema.json` 的 `sessionSummary` 已有 `workspace` |
| 前端工程与逻辑层（配对状态机、连接 12 态、快照/重放、事件去重、命令幂等、目录模型、创建入口、对话读取与翻页、降级卡片与诊断、主机面板、imported 不落盘） | **已落地** | `clients/app/src/**`，含单测与浏览器 E2E（`clients/app/e2e/`） |
| 前端**视觉与交互层** | **部分落地** | 目录页与目录详情页（`clients/app/app/index.tsx`、`clients/app/app/dir/[alias].tsx`）及其页内组件已用 `clients/app/src/components/theme.ts` 的浅/深两套 token + `StyleSheet.create` 出样式，`DirectoryList` / `DirectoryDetail` 各有一个 720px 宽屏断点；**尚未迁移**：对话页、配对页、`hosts` 页壳，以及 `ConversationStream` / `ContextUsageBar` / `DegradedEventCard` / `DiagnosticsDrawer` / `PairingPanel` 仍是未挂样式的语义标记。对话页仍只读。其中对话页与 `ConversationStream` / `ContextUsageBar` / `DegradedEventCard` / `DiagnosticsDrawer` 属在飞变更 `pwa-conversation-ui` 的呈现层范围（该变更尚未进入主检出，见 §2.1）；`PairingPanel`、配对页与 `hosts` 页壳不属该变更 |
| `server::sync` | **仍未落地** | `crates/server/src/transport/net/route.rs` 注释：未注册 path（含 `/sync/v1*`）一律 404 |
| `/ui` 静态资源托管 | **仍未落地** | 无任何注册的静态路由 |

一句话：**能算、能测的层都有了；用户能看见、能点的层除目录页与目录详情页（已出样式与宽屏分档）外基本还没有，且它连不上真机。**

---

## 1. 已实现（不要重做）

以下都已存在且有测试，不要在实现 UI 时重写其语义：

- 七层目录与依赖方向约束（`clients/app/src/layers.ts` + `layers.test.ts`）、组合根（`src/composition.ts`）。
- 设备身份：WebCrypto **不可导出** P-256 `CryptoKey` + IndexedDB（`src/platform/secure-storage.web.ts`）。
- 连接状态机 12 态、命令 6 态、`requestId` 复用与「仅服务端确认才呈现 uncertain」（`src/state/`）。
- Sync 客户端：握手、`subscribe`、ACK、增量重放、快照暂存与原子替换、`eventId` 去重（`src/sync-client/`）。
- 协议 DTO：严格消费 `schemas/sync/v1/**` 与 `fixtures/sync/v1/**`（`src/protocol/`）。
- 目录页与目录详情**数据模型**、两种空态、离线来源标注（`src/features/directory-model.ts`）。
- 会话创建入口：精确两键载荷、无权限说明、创建中/终态/uncertain（`src/features/create-session-model.ts`）。
- 对话页**只读**模型：消息文本、三态上下文条、翻页游标、降级事件、能力标记
  （`src/features/conversation-model.ts`、`src/components/ConversationStream.tsx`）。变更 `pwa-conversation-ui` 在它之外新增时间线、输入/授权/配置视图模型与呈现覆盖，不改动这里已有的消息、翻页与降级语义。
- 降级卡片与诊断抽屉（原文不可用时不渲染占位、三类不支持可区分）。
- 主机与连接面板模型（Agent 列表来自快照、覆盖层缺席 = 状态未知）。
- 本地缓存边界：imported 不落盘、本节点摘要缓存 8 MiB / 30 天 / LRU（`src/platform/local-cache.web.ts`）。

前端目前**只 dispatch 两条命令**：`session.create`、`session.read`。其余命令名只存在于协议类型与测试里。

---

## 2. 对照原型的 UI 缺口

### 2.1 对话页（最大缺口）

原型的对话页是「左栏会话 / 中间流 + 输入区 / 右栏变更文件」三栏，实现只有一个只读的单栏列表。下表第三列是在飞变更 `pwa-conversation-ui` 的写范围——它只覆盖对话页（不含配对页、`hosts` 页壳与主题开关），且代码尚未进入主检出，属于它的条目一律记为「本变更范围内、尚未落地」，不属于它的行标注「不属本变更」，两者都不写作「已落地」；该变更的第三列不代表 `server::sync` 已可用（见 §3.1），它只是把 wire 合同、core/storage 读路径与前端呈现补齐。

| 原型有 | 实现现状 | 本次变更（`pwa-conversation-ui`） |
|---|---|---|
| 中间流把消息与结构化条目排在同一条时间线上 | 只有只读单栏消息列表；结构化条目只在实时事件流里存在，重开页面看不到 | **本变更范围内、尚未落地**：历史条目随 Sync 面 `session.read` 的 `history` include 读取，实时与历史走同一套条目呈现；来源为其它节点（imported）的会话对该部分**显式降级**（`NODE_LINK_PROTOCOL.md` §12.7） |
| 输入框 + 发送/停止按钮、`Enter` 发送 / `Shift+Enter` 换行、仅文本与附件不支持说明 | **完全没有**；`ConversationStream` 只输出 `data-can-send` 等标记，没有输入控件 | **本变更范围内、尚未落地**：文本输入区 + 发送/取消与三态（可输入 / 进行中 / 已关闭）、附件不支持说明；提交与取消的呈现以服务端终态为准 |
| 发送与取消命令（`session.prompt` / `session.cancel`） | 前端无 dispatch | **本变更范围内、尚未落地**：只用既有命令，不新增命令名与载荷字段 |
| 权限请求卡（允许一次 / 始终允许 / 拒绝）、已被电脑处理的折叠块、`interaction.already_resolved` | 无（无 `interaction.*` dispatch） | **本变更范围内、尚未落地**（部分）：应答只对本设备亲历的请求开放（可选项来自该请求自身的下发内容）；仅从历史读到的待处理项呈现为不可操作并说明原因；已被其它设备处理的呈现为已处理 |
| 结构化专用卡：思考、工具调用（含参数/输出/退出码）、终端输出、plan、usage 折叠块 | 无；非文本消息只标一个「含结构化内容」 | **本变更范围内、尚未落地**（部分）：思考、工具调用、终端输出、plan 与降级卡进入同一条时间线；**usage 折叠块不属本变更**（仍只有 `ContextUsageBar` 的三态条） |
| 变更文件右栏 + diff 视图 + 与消息块双向联动 | 无 | **本变更范围内、尚未落地**（部分）：右栏只给结构化行数统计、相对化展示路径与「查看 ACP 原文」诊断入口；**差异正文与「与消息块双向联动」不属本变更**（设计上只给统计与路径，不解析 ACP 原文还原 diff 行） |
| 模型 / 模式选择器（`session.config.list\|set`、`session.mode.list\|set` 及对应 `*changed` 投影） | 无 | **本变更范围内、尚未落地**：呈现当前值、可选项、切换中与「下一轮生效」标注，以及未提供时的不支持态 |
| Markdown 渲染、语法高亮、代码复制、链接/图片安全降级 | 无；消息按纯文本 `<p>` 渲染 | **本变更范围内、尚未落地**：按「Agent 产生的富文本一律是不可信输入」渲染（禁原始 HTML、不加载远端资源、代码块可复制） |
| 三栏在窄屏收为「时间线 + 可切换的次级视图」 | 对话页无断点；只有目录页与目录详情页各有一档 720px 宽度分档 | **本变更范围内、尚未落地**：宽屏时间线与并列的变更文件视图，窄屏把变更文件降为可切换的次级视图，两形态信息集合相同 |
| 上下文 chip 弹层 + ≥85% 警告条 | 只有 `ContextUsageBar` 的三态条 | **不属本变更** |
| Agent chip popover（本会话涉及的 Agent） | 无 | **不属本变更** |
| 左栏：同目录会话列表 + 目录内搜索 + 分页 + 新建入口 | 无 | **不属本变更** |
| 「跳到你最近的输入」胶囊、待处理授权跳转条、连接状态条（正在追平 / 重连第 N 次） | 无 | **不属本变更** |

`pwa-conversation-ui` **明确不做、也不假装已有**的项（后继提案不要误以为它们随本变更落地）：

- 差异正文：设计上只给结构化统计与相对化路径，不解析 `acp.rawJson` 还原 diff 行。
- Node Link 面的结构化历史：本版尚未实现（`NODE_LINK_PROTOCOL.md` §12.7 的已知缺口）；imported 会话在客户端侧对结构化历史显式降级。
- `server::sync` 投影：本次只把 wire 合同与 core/storage 读路径补齐；它本身仍是 §3.1 记录的「整个不存在」。
- 目录页与目录详情页：属 `pwa-directory-ui` / `pwa-directory-ui-followup`，已经落地（§2.2）。

### 2.2 目录页与目录详情

原型（`prototypes/acp-remote-pwa.html` 的 `viewDirs` / `viewDirDetail`）与实现（`clients/app/src/components/DirectoryList.tsx`、`DirectoryCard.tsx`、`DirectoryDetail.tsx`）的逐项对照，状态按变更 `pwa-directory-ui` 与 `pwa-directory-ui-followup` 落地后的代码核对。

上一版这里列的六条缺口——搜索框、状态筛选分段、目录内联会话行 +「查看全部」+ 折叠、详情页按月分档、离线提示条与同步时间、目录页权限提示条——**已全部补齐，不再是缺口**；下表把它们与本次补上的两项（最近活动时间、卡片级创建入口）以及死路的修复一并记下，供后继提案核对。

| 原型有 | 实现现状 |
|---|---|
| 搜索框（按目录名 / 会话标题）、状态筛选分段（全部 / 运行中 / 待处理） | **已实现**：受控输入写进 store；两种空态下不呈现工具条，「搜索无结果」另作第三种呈现，不冒充空态 |
| 每个目录内最多 3～5 条会话 + 「查看全部」+ 卡片折叠 | **已实现**：内联上限 5 条；折叠只翻本项（`data-directory-collapsed`），不影响其它目录项 |
| 详情页按月分档 + 月份分段 + 「当前显示 N / M」 | **已实现**：分档与两个数字都在 `directory-model` 的纯函数里；「目录里从来没有会话」与「被查询、月份档挡住」是两种互不冒充的呈现 |
| 离线提示条与同步时间（「已同步 14:33」/「离线 · 2 分钟前同步」） | **已实现**：来源标注（`data-content-origin`）+ UTC 固定切片的同步时刻（`data-synced-at`）；从未成功同步过时不呈现任何时间 |
| 目录页顶部的「这台设备没有创建会话的权限」提示条 | **已实现**：`data-create-permission-note`，文案取自创建入口模型的 `permission.note`，与详情页同源 |
| 卡片头部的「最近 <活动时间>」（`dwhen`） | **已实现**：`data-directory-latest-at` 带原始时间，文案为「最近 YYYY-MM-DD HH:MM UTC」；取该目录下会话 `updatedAt` 的最大值，无会话时不呈现 |
| 卡片上的「⊕ 新建」（原型用 `A.canCreate` 门控） | **已实现**：`data-action="create-session"`，与目录详情页共用同一条 `session.create` 与同一份权限判定，载荷仍只有 `{ workspaceAlias, agentId }` 两个引用；无权限时卡片不呈现该入口，说明由页面顶部承担 |
| 卡片头部可进入详情页 | **实现优于原型**：原型只有「查看全部」且只在有会话时给出，0 会话的目录因此进不去；实现把目录名做成 `data-action="open-directory"` 的入口，与会话数无关，有会话时再保留「查看全部 N 个 ›」（`data-action="open-directory-all"`），两个入口的机器标识可区分 |
| （原型同样进不去的死路）0 会话时既进不去目录详情页、也建不了第一个会话 | **已修复**：空态（已登记目录但无会话）下同时呈现空态标记与其下的目录列表（`data-directory-list="directories"`），空态指引说的「在下面选一个目录」第一次指向真实存在的列表；本机一个目录都没有（`no_directories`）时不呈现任何目录项 |
| 卡片体内 0 会话时的「这个目录还没有会话」提示行 | **仍缺**（本段唯一残留项）：卡片体在无内联会话行时是空白；目录页只给出目录项本身，可达性不受影响 |

### 2.3 顶栏 / 主机与连接

- 顶部 logo 与 hostchip（点击打开主机面板）。
- 开发模式横幅与「开发模式」徽标（`FRONTEND_DESIGN §4.4` 要求明文连接必须显著提示）。
- 帮助抽屉（ⓘ）。
- 主题切换（跟随系统 / 白天 / 黑夜）。
- 主机面板内容：设备名与类型、设备 ID、当前 scope 标签列表、主机指纹（含「指纹 = SHA-256(主机公钥)」说明）、本版本边界说明、「立即重连」/「重新配对」/「清除数据」二次确认。

### 2.4 配对页

- 二维码扫描/粘贴入口与落地处理。
- 6 位 SAS 人工核对、倒计时环、每 3 秒轮询提示。
- 步骤条（已认领 → 电脑已批准 → 建立安全连接）。
- 权限清单（本次申请的 scopes）、「接下来会发生什么」、防转发安全提示。
- 原型的 18 个状态（S0 / S2a–d / S3a–e / S4 / S5 / S6 / S7 / S8a / S8b / S9 / S10）；
  实现只覆盖 `idle / awaiting_qr / claiming / pending_confirmation / approved / rejected / expired / consumed / failed`。
- 完成总结卡与「进入会话列表」。

### 2.5 新建会话弹层

- 「将在你的电脑上启动一个 X 进程，工作目录 Y」副作用提示。
- 结构化失败原因（凭据缺失 / Agent 启动超时 + 脱敏摘要）。
- 已具备：Agent 选择、默认标记、无权限说明、创建中 / 终态 / 结果不确定。

### 2.6 全局

- **样式基座已落地，覆盖面只到目录页与主机面板**：`clients/app` 的 `src/` 与 `app/` 下仍无 `.css` 文件、无 Tailwind、无 `className`（原型 `prototypes/acp-remote-pwa.html` 自带的 CSS 不进实现）；样式走 `react-native-web` 的 `StyleSheet`，基座是 `clients/app/src/components/theme.ts` 的浅/深两套颜色 token 与间距/圆角/字号/字族尺度，由 11 个组件（`ConnectionBadge`、`CreateSessionSheet`、`DirectoryCard`、`DirectoryDetail`、`DirectoryList`、`EmptyState`、`HostConnectionPanel`、`RuntimeUnavailable`、`SegmentControl`、`SessionRow`、`StatusPill`）与两个路由（`app/index.tsx`、`app/dir/[alias].tsx`）以 `StyleSheet.create` 消费；`ConversationStream`、`ContextUsageBar`、`DegradedEventCard`、`DiagnosticsDrawer`、`PairingPanel` 与对话页、配对页、`hosts` 页壳仍是不带 `style` 的语义标记（其中对话页与 `ConversationStream` / `ContextUsageBar` / `DegradedEventCard` / `DiagnosticsDrawer` 属在飞变更 `pwa-conversation-ui` 的呈现层范围，落地后本条需按实测更新，见 §2.1）。
- 无 PWA `manifest.json`、service worker、安装引导、图标。
- 响应式只在目录这一块做了一档宽度分档：`DirectoryList.tsx` 的 `WIDE_BREAKPOINT = 720`（可用宽度 ≥720px 时目录卡片并排，否则单列）与 `DirectoryDetail.tsx` 的 `WIDE_LAYOUT_MIN_WIDTH = 720`（≥720px 时头部与工具条并排；两档渲染的信息集合相同）；对话页、配对页、`hosts` 页无断点（移动端原型已画出，实现未做；对话页的宽窄两形态属在飞变更 `pwa-conversation-ui`，见 §2.1）。
- 无 i18n（原型为 zh-CN）。

---

## 3. 服务端缺口（PWA 连不上真机的原因）

### 3.1 `server::sync`（整个不存在）

`crates/server/src/` 只有 `local_admin` / `node_link` / `transport`。因此以下全部缺失：

- `/sync/v1` WebSocket 路由与 subprotocol `acp-remote.sync.v1.json`。
- 认证四消息（`client_hello` / `server_challenge` / `client_proof` / `authenticated`）与 `auth.*` 错误码。
- `subscribe` + 增量重放 + `caught_up` / `reset_required`。
- 快照组装（`sessions` / `workspaces` / `agents` 三类资源 + 分块 + `snapshotDigest`）；
  可参照 `crates/server/src/node_link/resource.rs`。
- 12 条 Sync 命令处理器 + 幂等记录 + `command.result` 终态（`docs/SYNC_PROTOCOL.md` §11.5）。
- 34 类事件视图投影（`payload.view`，`schemas/sync/v1/event-views.schema.json` 的 34 个 `$defs`）
  + `acp.rawJson` 装配 + `rawUnavailable`。
- `limits` 下发、限流与 4429、撤销传播、授权拒绝。
- `/sync/v1/pairing/claim` 与 `/sync/v1/pairing/status` 的 HTTP 处理器
  （`identity-auth` 状态机已有，缺 server 侧接线）。

### 3.2 `/ui` 静态资源托管

无静态路由，也没有「前端构建版本 ↔ Daemon」的可检查关联（`FRONTEND_DESIGN §4.3` 要求）。

### 3.3 本次不需要

- `server::acp_facade`：属 Zed 闭环，PWA 走 Sync。
- `node-link-client`：PWA 直连单个 Owner Node。

---

## 4. 与原型不一致、但已按权威文档决策（不是缺口）

| 项 | 原型的做法 | 实现的做法 | 依据 |
|---|---|---|---|
| 目录路径 | 详情页显示完整路径、目录页显示尾段 | 只显示展示名与别名，路由参数里也没有路径 | `FRONTEND_DESIGN §2.2/§4.1`、`SECURITY_DESIGN §12.3`、`pwa-web-client` 的 R15。原型的完整路径是**待决策项**，实现是对的 |
| 会话创建入口 | 原型 README 旧文案写「本版不提供创建会话入口」 | 已实现受限创建入口（v1 内） | `FRONTEND_DESIGN §2.2` 已改为受限入口，旧文案过时 |
| 远程会话 | 原型不涉及 | v1 不协商 `resource.remote-origin.v1` | PWA 直连 Owner，属已记录的产品边界 |
| 队列策略 | 原型用单按钮屏蔽了「运行中排队发送」 | 同样未暴露 | 两边一致，非缺口 |

---

## 5. 待决策 / 尚未设计

- 主题是否做成产品内开关（原型顶栏有按钮，但按 PWA 规范应跟随系统）。
- 移动端布局与响应式断点（移动原型已画，`FRONTEND_DESIGN §9` 列为验收项）。
- PWA 安装引导与 service worker 更新提示（`FRONTEND_DESIGN §4.4` 要求可安装）。
- 手机后台挂起与恢复体验（`INITIAL_DESIGN §16` 未验证项 5）。

---

## 6. 建议的落地顺序

```text
① 视觉与响应式层：把无样式语义标记换成真实 UI（先目录页、对话页）  ← 用户能看见的前提
② 对话页交互闭环：prompt / cancel / permission + 乐观状态与终态呈现
③ 对话页富内容：Markdown 渲染、结构化折叠块、变更文件与 diff
④ 模型 / 模式选择器 + Agent chip + 上下文弹层
⑤ server::sync + /ui                      ← 真正连上 Daemon 的前提
⑥ 配对页交互（扫码 / SAS / 倒计时 / 状态）与主机面板补全
⑦ PWA 安装、主题、移动端布局
```

说明：①②③ 可以先用 `clients/app/e2e/` 的 fake host 做视觉与交互验收，不必等 ⑤；
但**端到端可用**必须等 ⑤。对话页部分（① 的对话页、②、③ 的 Markdown / 结构化卡 / 变更文件统计）
已由在飞变更 `pwa-conversation-ui` 认领，范围以 §2.1 的第三列为准（**不含**差异正文、usage 折叠块
与「与消息块双向联动」）；④ 与 ⑥⑦ 仍未被认领。`server::sync` 与 `/ui`（⑤）不属本变更，
本变更只补齐 wire 合同与 core/storage 读路径。
