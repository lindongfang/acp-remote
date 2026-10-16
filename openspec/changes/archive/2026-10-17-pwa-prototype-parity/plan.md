<!-- 执行安排：把 specs 的行为要求与 design 的技术方案转成可派发的并行工作包。
     plan 只写“谁在哪个隔离工作区做什么、按什么顺序合入、怎么验”，不重复 specs/design 的内容。 -->

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出多个工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）。工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；工作包不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`，全部合入后在主检出跑 `## Project Verify`。
- 验证：隔离工作区内**没有** `clients/app/node_modules/`，因此 `npm run check`、`expo export`、`e2e/run-e2e.mjs` 只在主检出执行；工作包内只允许 `node --check`/`tsc --noEmit` 级别的自检。
- 检视：每个工作包由一个**独立** spawn（不与作者同实例）读 patch 与 `agent://<id>` 后给出结构化结论。
- **冻结接口（派发时写进每个工作包的 `context`，避免同一处被两个包各改一半）**：
  - 视图模型层（WP2 交付，WP3/WP4/WP6/WP7 消费）：目录详情模型带 `page`/`pageCount`/`pageSize` 与「纠正后的页码」；会话行时间字段由模型给出**相对文案**（时钟经模型输入注入）；对话页模型带左栏会话列表与右栏差异行。
  - 创建链路（WP5 交付，WP3/WP4 消费）：`DirectoryPicker` 的 props 固定为「初始路径、已登记目录集合、确认/取消回调」；`CreateSessionSheet` 的 `onSubmit` 载荷在既有 `{ agentId, directoryPath }` 上**只增加一个布尔**（是否允许先建目录）。
  - 外壳（WP1 交付，WP8 消费）：`Shell` 组件不接受业务 props，只读 `usePathname()` 与连接状态。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 样式基座与共享外壳：token 补断点/字重/层级/动效、主题三态与 UI 偏好、顶栏（标识/设备胶囊/开发模式徽标/主题开关/帮助）、开发模式横幅、帮助抽屉 + 遮罩 + toast、全局滚动条与 reduced-motion | none | wp-coder | `clients/app/src/components/theme.ts`, `clients/app/src/components/shell/`, `clients/app/app/_layout.tsx`, `clients/app/e2e/page/index.tsx`, `clients/app/e2e/page/checks/ui-stylesheet.ts` | 全部路由共用一份外壳；配对路由隐藏全局顶栏；主题三态可持久化 | 主检出：`cd clients/app && npm run check`；`node e2e/run-e2e.mjs` 的 `ui-stylesheet` 用例全绿 |
| WP2 | 视图模型：详情页分页与页码纠正、相对时间（注入时钟）、对话页左栏会话列表、次级视图扩到四项、右栏差异行消费 | none | wp-coder | `clients/app/src/features/`, `clients/app/src/domain/` | 纯函数与既有 `client-store` 选择器；不新增 store 字段以外的东西 | 主检出：`cd clients/app && npm run check`（vitest 覆盖分页夹紧、非法页码纠正、相对时间三档、左栏列表） |
| WP3 | 目录页：卡片八槽位与缩进树、会话行五字段与状态词、同名路径尾段、查看全部文案、空态与无命中文案 | code:WP2 | wp-coder | `clients/app/src/components/DirectoryList.tsx`, `clients/app/src/components/DirectoryCard.tsx`, `clients/app/src/components/SessionRow.tsx`, `clients/app/src/components/EmptyState.tsx`, `clients/app/app/index.tsx`, `clients/app/e2e/page/checks/r15-directory.ts`, `clients/app/src/components/r15-directory-list.test.ts` | 目录页呈现与原型逐项对齐；别名不再出现；路径取自快照 | 主检出：`cd clients/app && npm run check`；`node e2e/run-e2e.mjs` 的 `r15-directory` 用例全绿 |
| WP4 | 目录详情页：返回 + 标题 + 会话数芯片 + 路径行、平铺倒序 + 每页 10 条 + 页码翻页与底栏 | code:WP2 | wp-coder | `clients/app/src/components/DirectoryDetail.tsx`, `clients/app/app/dir/[alias].tsx`, `clients/app/src/components/r15-directory-detail.test.ts`, `clients/app/e2e/page/checks/r15-directory-detail.ts` | 月份分档、目录内搜索、「当前显示 N / M」移除；分页可用 | 主检出：`cd clients/app && npm run check`；`node e2e/run-e2e.mjs` 的 `r15-directory-detail` 用例全绿 |
| WP5 | 创建弹层与文件夹浏览：目录块两种形态、四类分支提示、Agent 单选卡、副作用提示、选择文件夹弹层（根列表/面包屑/上一级/新建文件夹/已登记标记） | contract:WP2, code:WP3, code:WP4 | wp-coder | `clients/app/src/components/CreateSessionSheet.tsx`, `clients/app/src/components/DirectoryPicker.tsx`, `clients/app/src/features/create-session-model.ts` | 两条创建路径都可用；服务端不支持浏览时明确说明且保留已登记目录路径 | 主检出：`cd clients/app && npm run check`（`r16-create-session.test.ts` 改写后全绿） |
| WP6 | 对话页骨架与输入区：三栏网格、会话头（标题/状态徽标/Agent 胶囊）、连接状态条、上下文胶囊与告警条、模型/模式胶囊与弹层、输入行三态与 Enter 发送、跳转胶囊 | code:WP2 | wp-coder | `clients/app/src/components/ConversationStream.tsx`, `clients/app/src/components/ConversationComposer.tsx`, `clients/app/src/components/ConversationSettingsPanel.tsx`, `clients/app/app/session/[id].tsx`, `clients/app/e2e/page/checks/conversation-interaction.ts` | 宽屏三栏、窄屏次级视图四项；输入区行为与原型一致 | 主检出：`cd clients/app && npm run check`；`node e2e/run-e2e.mjs` 的 `conversation-interaction` 用例全绿 |
| WP7 | 对话页时间线与右栏：用户/Agent 气泡、代码卡、折叠块（默认按状态展开 + 显式覆盖 + 与右栏联动）、权限卡三按钮、降级卡、运行指示、失败条、变更文件列表 + 差异行 | code:WP2, code:WP6 | wp-coder | `clients/app/src/components/ConversationTimeline.tsx`, `clients/app/src/components/FileChangesPanel.tsx`, `clients/app/src/components/Markdown.tsx`, `clients/app/src/components/CodeBlock.tsx`, `clients/app/e2e/page/checks/conversation-timeline.ts`, `clients/app/e2e/page/checks/conversation-content.ts` | 条目形态与右栏差异行按原型呈现；没有结构化差异行时明确说明 | 主检出：`cd clients/app && npm run check`；`node e2e/run-e2e.mjs` 的 `conversation-timeline`/`conversation-content` 用例全绿 |
| WP8 | 裸标记组件与其余路由外壳：`ContextUsageBar`、`DiagnosticsDrawer`、`DegradedEventCard`、`PairingPanel` 迁到样式基座；`hosts`/`pair`/`node-link/pair` 三个路由改用共享外壳 | code:WP1 | wp-coder | `clients/app/src/components/ContextUsageBar.tsx`, `clients/app/src/components/DiagnosticsDrawer.tsx`, `clients/app/src/components/DegradedEventCard.tsx`, `clients/app/src/components/PairingPanel.tsx`, `clients/app/app/hosts.tsx`, `clients/app/app/pair.tsx`, `clients/app/app/node-link/pair.tsx` | 四个组件有样式且 `data-*` 集合不变；三个路由共用外壳 | 主检出：`cd clients/app && npm run check`；既有 `r17`/`r18`/`node-degraded-panel` 断言原样通过 |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 无重叠；它是 WP8 的基线 | 无 |
| 2 | WP2 | 无重叠；它是 WP3–WP7 的视图模型基线 | 无 |
| 3 | WP3 | 与 WP5 在 `app/index.tsx` 上有交集（WP5 接线创建链路） | 合入 WP5 后重跑 `cd clients/app && npm run check` |
| 4 | WP4 | 与 WP5 在 `app/dir/[alias].tsx` 上有交集 | 合入 WP5 后重跑 `cd clients/app && npm run check` |
| 5 | WP5 | 依赖 WP2 的模型、WP3/WP4 的页面位置 | `cd clients/app && npm run check` + `node e2e/run-e2e.mjs` |
| 6 | WP6 | 与 WP7 在对话页组件间有接口依赖 | 合入 WP7 后重跑 `cd clients/app && npm run check` |
| 7 | WP7 | 依赖 WP6 冻结的接口 | `cd clients/app && npm run check` + `node e2e/run-e2e.mjs` |
| 8 | WP8 | 与 WP1 在路由外壳上有顺承关系（WP1 定外壳，WP8 让其余路由用上它） | `cd clients/app && npm run check` |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cd clients/app && npm run check` | typecheck + vitest + `expo export` 全绿；改写的断言全部通过，无被跳过的用例 |
| PV2 | `cd clients/app && node e2e/run-e2e.mjs` | 真实 Chromium + 假主机的全部浏览器用例通过；`ui-stylesheet` 的计算样式断言（无内联 `<style>`、`#react-native-stylesheet` 是同源 `<link>` 且 `cssRules.length > 0`）仍成立 |
| PV3 | `npm run check`（仓库根） | 合同门禁全绿，确认本变更没有意外改动 `schemas/`、`fixtures/`、`compatibility/`、crate 依赖 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 外壳是否真的只有一处判据；主题偏好是否落进 UI 偏好槽位而不是与身份/缓存混槽；CSP 与 `/rnw-stylesheet.css` 边界是否未被破坏 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 时钟是否经输入注入（未引入全局打桩）；分页与非法页码纠正是否在纯函数里；左栏列表是否复用既有快照选择器 | 无 CRITICAL/MAJOR 未闭环项 |
| WP3 | reviewer | 别名是否彻底不再出现；路径是否逐字取自快照（无拼接）；会话行是否仍未引入路径 | 无 CRITICAL/MAJOR 未闭环项 |
| WP4 | reviewer | 月份分档与目录内搜索是否被彻底移除（含 feature 层残留）；分页越界与非法输入是否原地纠正；页面里是否还有业务分支 | 无 CRITICAL/MAJOR 未闭环项 |
| WP5 | reviewer | 载荷是否仍只含规范允许的键；服务端不支持浏览时是否明确说明而不是静默失败；是否有路径被客户端拼接 | 无 CRITICAL/MAJOR 未闭环项 |
| WP6 | reviewer | 窄屏是否仍保留四列内容在 DOM；Enter 发送在不该提交时是否真的不发命令；上下文未上报是否仍不显示数字 | 无 CRITICAL/MAJOR 未闭环项 |
| WP7 | reviewer | 差异行是否只来自结构化来源；没有差异行时是否明确说明而不是渲染空区；折叠默认值是否由状态决定 | 无 CRITICAL/MAJOR 未闭环项 |
| WP8 | reviewer | 四个组件的 `data-*` 集合与行为是否与改前一致；路由外壳替换后配对页是否仍落在可用入口 | 无 CRITICAL/MAJOR 未闭环项 |

## Main E2E

```yaml
mode: required
reason: ""
basis: ""
alternative_checks: []
```

浏览器级验收（`browser` prelude，真实 Chromium）：

- 用 `cd clients/app && node e2e/run-e2e.mjs` 跑完全部既有与改写用例，确认 `data-*` 锚点与信息集合的新口径。
- 另起一次人工核对：把 `prototypes/acp-remote-pwa.html` 与实现页面并排打开，逐页核对结构、分区、控件、列宽、状态色与文案口径，把差异清单写进报告——差异只允许剩 `proposal.md` 里登记的三处有意偏离。
- 断连、无目录、无会话、会话已关闭四种状态各切一次，确认降级呈现与原型一致。

## Risks

- **WP3/WP4 与 WP5 在页面文件上重叠**：按 Merge Order 串行，后合入者重跑 `npm run check`。派发时把「WP5 会碰哪些行」写进 `context`，减少冲突面。
- **WP6/WP7 的组件接口**：两者共享对话页组件的 props 面。缓解：把接口在派发时冻结并写进 `context`；WP7 只改 `ConversationTimeline`/`FileChangesPanel` 内部。
- **e2e 计算样式断言最容易因换外壳而红**：WP1 的写范围直接包含 `ui-stylesheet.ts`，让它在同一个工作包里被更新，而不是留给后面某个包去猜。
- **断言更新可能被写成「删掉变红的断言」**：检视时专门核对「被删除的断言是否都有对应新断言」，并确认新增用例在原型形态被改回去时变红。
