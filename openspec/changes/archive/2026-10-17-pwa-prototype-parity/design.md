# Design

## Context

见 `proposal.md` — Why。影响设计的现状事实：

- **没有共享外壳**：`app/index.tsx`、`app/dir/[alias].tsx` 各自拼一条导航；`app/hosts.tsx`、`app/pair.tsx`、`app/session/[id].tsx` 的外层是裸 `<main><nav>`。`app/_layout.tsx` 只做组合根装配 + `<Stack>`。
- **样式基座是现成的**：`src/components/theme.ts` 提供浅/深两套 22 个颜色 token、间距/圆角/字号/字族尺度与 `useTheme()`；22 个组件已按 `buildStyles(colors)` + 模块级 `STYLES = {light, dark}` 的写法落地。
- **四个组件仍是裸标记**：`ContextUsageBar`、`DiagnosticsDrawer`、`DegradedEventCard`、`PairingPanel`。
- **断点硬编码三处**：`DirectoryList`/`DirectoryDetail`/`ConversationStream` 各写一次 `720`。
- **时间口径是刻意的**：`SessionRow.formatUpdatedAt` 与 `DirectoryCard.formatLatestActivity` 的注释都写明「不用本地时区、不画 N 分钟前」，理由是 `renderToStaticMarkup` 与浏览器断言都不该依赖运行时刻。改成相对口径必须解决同一问题，而不是推翻它。
- **对话页是两栏**：`ConversationStream` 的 `data-conversation-column="timeline" | "files"`，窄屏用 `secondaryView`（`both | timeline | files`）切换，且两列内容**始终在 DOM**（隐藏列 `display:none`），这是「窄屏不丢信息」的机器证据。
- **原型没有任何宽度断点**（全文件唯一 `@media` 是 `prefers-reduced-motion`），桌面布局是固定的 300/1fr/360 与 1180px 容器。

## Goals / Non-Goals

**Goals:**

- 三页与外壳在**结构、分区、控件、列宽、状态色、文案口径**上逐项对齐原型，差异只剩 `proposal.md` 登记的三处有意偏离。
- 保留现有全部机器证据形态：`data-*` 锚点继续存在（它们是对齐后的断言锚点），窄屏不丢信息的断言继续成立。
- 保持客户端分层与 CSP 基线不变：页面不碰平台能力，样式仍由同源外部样式表交付。

**Non-Goals:**

- 不移植原型自带的「原型控制台」与帮助抽屉里的原型导航内容。
- 不实现移动端原型独有的 `#/sessions` 独立路由与底部 tabbar（窄屏改用次级视图，见 D5）。
- 不改连接状态机、命令状态机、同步客户端、平台适配层的任何判定。
- 不为原型里那些**无人置位的开关**（`A.offline`、`A.left`/`A.right`、左栏那个没有绑定事件的搜索框）照搬：它们要么本来就该可用，要么不是产品行为。

## Decisions

**D1 共享外壳放在根布局，不放在各路由。**

`app/_layout.tsx` 渲染「开发模式横幅 → 顶栏 → 路由内容 → 抽屉/遮罩/toast」，路由用 `usePathname()` 判定配对页是否隐藏顶栏。理由：一份外壳只有一处判据；三个路由各拼一份正是造成今天三套导航不一致的原因。备选（每个路由各自引用一个 `<Shell>` 组件）被否——那样「哪些路由有顶栏」仍然散落在各路由文件里。

**D2 主题从「只读系统」改为三态 + UI 偏好持久化。**

`useTheme()` 的返回契约不变（仍是 `Theme`），新增一个主题偏好的读取与写入面，取值 `system | light | dark`。偏好落在既有的客户端 UI 偏好槽位（`FRONTEND_DESIGN.md` §7 允许持久化「客户端 UI 偏好」），与设备身份、会话正文、缓存槽位分开。缺省与旧用户（没有该键）= 跟随系统。

**D3 相对时间靠注入时钟，而不是靠真实时间。**

新口径（1 分钟内「刚刚」/ 1 小时内「N 分钟前」/ 否则年月日）需要「现在」。做法：把 `now` 作为**视图模型的输入**，由页面从 store 的时钟端口读一次注入；组件仍是纯函数，测试与 e2e 注入固定的 `now`，因此同一份输入仍渲染出同一段文本——原来那条「不依赖运行时刻」的性质被保留，只是换了实现方式。备选（客户端算但测试里 mock `Date`）被否——全局打桩会污染其它断言，且 `renderToStaticMarkup` 路径不好控。

**D4 详情页分页的判定在 feature 层。**

每页条数、总页数、当前页切片、页码夹紧都由 `src/features/directory-model.ts` 的纯函数算出；页码与目录页的查询词同构，住在 store 里（跨路由存活），组件只渲染并上抛。非法页码的就地纠正也在 feature 层返回「纠正后的值」，组件据它回填输入框——这样「不重画」是一条可断言的行为，而不是实现细节。

**D5 对话页三栏，窄屏沿用次级视图机制并扩到三项。**

宽屏：`300px | minmax(0,1fr) | 360px`。窄屏：`secondaryView` 的取值扩为 `all | timeline | sessions | files`，默认 `timeline`。两形态信息集合相同的现有断言语义**扩展**为「四列内容都在 DOM，未选中的列只是 `display:none`」。备选（照搬移动端原型的底部 tabbar + 独立会话列表路由）被否——那要新增一条路由与一套导航状态，而窄屏用已有的切换控件就能达到同一信息集合。

**D6 差异行只在有结构化来源时渲染。**

右栏在 `file.changed` 视图带差异行时逐行呈现（新增/删除/上下文各一档底色，行内容按不可信文本处理）；带不可用标记时呈现明确说明；两者都没有时保持今天的「只有统计」形态。**不解析 `rawJson` 还原差异行**——这条边界不变。

**D7 四个裸标记组件迁到样式基座，行为不变。**

`ContextUsageBar` 变成原型那种「进度条 + 占比 + 已用/上限」的胶囊（点击展开明细）；`DiagnosticsDrawer` 从内联 `<aside>` 变成右侧抽屉（保留 `data-diagnostics-drawer="open"` 与 Esc 关闭）；`DegradedEventCard` 与 `PairingPanel` 只补样式与结构，不改字段与行为。

**D8 样式机制不变。**

继续用 `react-native-web` 的 `StyleSheet.create` + `theme.ts` 的 token；不引入 CSS 文件、样式库或 `className`。原因是既定的 CSP 基线下样式由同源外部样式表交付，`/rnw-stylesheet.css` 必须保持为空（`ui-csp-inline-styles` 的硬边界），换机制会把这条边界重新打开。

**D9 token 补断点、字重、层级与动效，`theme.test.ts` 的契约随之更新。**

原型用到的、今天 token 里没有的：断点（现在三个组件各写一次 `720`）、字重档、层级（抽屉/遮罩/toast）、动效时长。补齐后 `theme.test.ts` 里「调色板恰好 22 键」的断言改为断言**键集合的唯一定义位置**仍然成立（新增的是尺度而不是颜色键），避免测试变成维护负担。

**D10 折叠块的默认展开由条目状态决定，显式操作覆盖默认值。**

展开状态住在页面级视图状态里（与既有 `openChangeDiagnostics` 同构），不写进 store，也不改变任何数据判定。

## Risks / Trade-offs

| 风险 | 缓解 |
|---|---|
| 信息结构变化打断一批既有断言 | 逐页改断言而不是删断言；新增/改写的浏览器级用例必须能在「把原型形态改回去」时变红（`proposal.md` 的成功判据已把它列为验收项） |
| 相对时间重新引入不确定性 | 时钟作为视图模型输入注入（D3），组件保持纯函数 |
| 三栏在窄屏丢信息 | 隐藏列仍留在 DOM（D5），并保留「两形态 `data-*` 集合相同」的断言，把它扩到四列 |
| 外壳放进根布局会影响配对路由或首帧 | `usePathname()` 判定 + 专门的用例（配对页无全局顶栏、根路径有）；`_layout.tsx` 的装配顺序不变 |
| 依赖 `sync-path-browse-and-diff-delivery` 的三处呈现（路径、浏览、差异行）在该变更合入前无法验收 | 工作包可以并行开工（合同已冻结在规格里），但验收明确标注为「需该变更先合入」；不在本变更里造替代数据源 |
| 四个裸标记组件改动面小但触及既有降级断言 | 只改呈现不改字段与 `data-*` 集合；`node-degraded-panel.test.ts`、`r18-degradation*.test.ts` 的断言原样通过是硬要求 |

## Migration Plan

- **无数据迁移**：主题偏好是新键，缺失即「跟随系统」；无 schema、无索引、无缓存格式变化。
- **发布顺序**：本变更在 `sync-path-browse-and-diff-delivery` 之后合入；若前端先合入，路径/浏览/差异行三处呈现按「数据缺席」的既有降级路径呈现（不显示路径、浏览入口说明不可用、差异区说明未下发），不崩、不伪装。
- **回滚**：回滚前端产物即可；服务端新特征对旧前端无影响（未协商即不下发）。

## Open Questions

- 顶栏在窄屏上是否要进一步收成「标识 + 胶囊 + 主题开关」三件（原型只画了桌面与移动两版，没有画出中间宽度）——按实际渲染结果决定，不影响规格与任务拆分。
- 帮助抽屉里的内容以什么为准（原型抽屉是原型导航）——实现时按 `FRONTEND_DESIGN.md` 与主机面板已有信息拼，属呈现细节。
