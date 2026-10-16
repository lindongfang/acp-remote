# Tasks

## 1. 样式基座与共享外壳（WP1）

- [x] 1.1 在 `clients/app/src/components/theme.ts` 补断点、字重、层级与动效四类尺度 token，并把三处硬编码的 `720` 改为读该 token；判据：`cd clients/app && npm run check` 全绿，且 `src/components/theme.test.ts` 里关于 token 键集合的断言按新契约更新后仍能拦住「第二处定义」
- [x] 1.2 新增主题偏好面（`system | light | dark`，缺省跟随系统）并接进 `useTheme()`，偏好落在客户端 UI 偏好槽位；判据：`cd clients/app && npm run check` 全绿，且新增用例覆盖「显式取值跨刷新保留」与「跟随系统随系统变化」
- [x] 1.3 新增共享外壳组件（顶栏：标识、设备胶囊三档色 + 异常短词 + 折角、开发模式徽标、主题开关、帮助入口；开发模式横幅；帮助抽屉 + 遮罩 + toast），在 `clients/app/app/_layout.tsx` 装配为「横幅 → 顶栏 → 路由内容 → 抽屉/遮罩/toast」；判据：`cd clients/app && npm run check` 全绿，且新增用例覆盖「配对路由不呈现全局顶栏」「根路由呈现顶栏」「明文连接呈现横幅与徽标」
- [x] 1.4 让连接状态只由设备胶囊表达（移除各路由自行拼出的第二个状态徽标），并保留从胶囊进入主机与连接面板的入口；判据：`cd clients/app && npm run check` 全绿，且用例覆盖在线态不出现状态短词、阻断态出现对应短词
- [x] 1.5 全局滚动条样式与 `prefers-reduced-motion` 处理；判据：`node clients/app/e2e/run-e2e.mjs` 的 `ui-stylesheet` 用例（无内联 `<style>`、`#react-native-stylesheet` 是同源 `<link>` 且 `cssRules.length > 0`）全绿
- [x] 1.6 更新 `docs/FRONTEND_DESIGN.md` §4.1 与 §9：共享外壳、主题三态、相对时间口径、目录详情分页、对话页三栏、路径与结构化差异行的前置条件（内容以本变更的规格增量与 `design.md` 为准，不依赖其它工作包的代码）；判据：文档与 `openspec/changes/pwa-prototype-parity/specs/pwa-web-client/spec.md` 逐条口径一致（人工核对并写进报告）

## 2. 视图模型（WP2）

- [x] 2.1 在 `clients/app/src/features/directory-model.ts` 增加详情页分页：每页条数、总页数、当前页切片、页码夹紧与非法输入的就地纠正；页码状进 store（与目录页查询词同构）；判据：`cd clients/app && npm run check` 全绿，且新增用例覆盖「越界夹紧」「非数字纠正」「一处非法输入不改变列表内容」
- [x] 2.2 把会话行时间改为相对口径，并且**时钟经输入注入**（不 mock 全局 `Date`）；判据：`cd clients/app && npm run check` 全绿，且用例覆盖三档（刚刚 / N 分钟前 / 日期）与「同一输入渲染同一文本」
- [x] 2.3 在对话页模型里增加左栏所需数据（同目录会话列表、目录上下文、已呈现数与总数、继续加载）；只读快照资源，不派发新命令；判据：`cd clients/app && npm run check` 全绿，且用例覆盖「只列同目录会话」「无目录归属时说明而不是隐藏」
- [x] 2.4 把次级视图取值扩到四项并更新 `client-store` 的读写面；判据：`cd clients/app && npm run check` 全绿，且用例覆盖四个取值的互斥与缺省
- [x] 2.5 在对话页模型里接入右栏差异行（有差异行 / 有不可用标记 / 两者都没有三种形态），不解析 ACP 原文；判据：`cd clients/app && npm run check` 全绿，且用例覆盖三形态互不冒充

## 3. 目录页（WP3）

- [x] 3.1 把 `clients/app/src/components/DirectoryCard.tsx` 的卡片头改为原型的槽位（折叠箭头、目录图标、展示名、同名时的路径尾段、计数胶囊、运行中/待处理徽标、弹性空白、`⊕ 新建`、`查看全部 ›`）并把卡体改为缩进树；同名消歧取快照里的路径，展示名唯一时不呈现路径；判据：`cd clients/app && npm run check` 全绿，`r15-directory-list.test.ts` 改写后全绿
- [x] 3.2 让别名彻底不出现在界面上（`data-directory-alias-label` 相关呈现移除，别名只留作路由与命令引用）；判据：改写后的用例断言渲染结果里不含别名文本，且 `data-directory-alias` 仍只在卡片根节点出现一次
- [x] 3.3 把 `clients/app/src/components/SessionRow.tsx` 的会话行改为原型的五字段（状态圆点、标题、状态词、Agent、相对时间），空标题呈现为未命名会话；判据：`cd clients/app && npm run check` 全绿，且用例覆盖「等待授权/等待输入各出现对应状态词」与「会话行仍不出现路径」
- [x] 3.4 更新目录页工具条与搜索/筛选/空态/无命中的呈现（含 `EmptyState`），并把 `clients/app/app/index.tsx` 的自建导航换成共享外壳；判据：`cd clients/app && npm run check` 全绿，`node e2e/run-e2e.mjs` 的 `r15-directory` 用例全绿
- [x] 3.5 同步 `clients/app/e2e/page/checks/r15-directory.ts` 的锚点与断言，使它在「卡片退回旧的八槽位形态」时变红；判据：`node clients/app/e2e/run-e2e.mjs` 全绿，并在报告里给出「故意改回旧形态后该用例变红」的一次实测记录

## 4. 目录详情页（WP4）

- [x] 4.1 把 `clients/app/src/components/DirectoryDetail.tsx` 的页头改为「返回 + 标题 + 会话数芯片 + 路径两行」，并移除月份切换、目录内搜索、「当前显示 N / M」与三项汇总徽标；判据：`cd clients/app && npm run check` 全绿，`r15-directory-detail.test.ts` 改写后全绿且不再断言 `data-segment="月份"`
- [x] 4.2 正文改为按时间倒序平铺 + 每页 10 条 + 底栏（左 `⊕ 新建` + 副作用提示，右上一页/页码输入/下一页，只有一页时不呈现翻页）；非法页码在原输入位置纠正且不重画列表；判据：`cd clients/app && npm run check` 全绿，且用例覆盖「翻页后回到页首」「只有一页不呈现翻页」
- [x] 4.3 把 `clients/app/app/dir/[alias].tsx` 的自建导航换成共享外壳并接上分页状态；判据：`cd clients/app && npm run check` 全绿，`node e2e/run-e2e.mjs` 的 `r15-directory-detail` 用例全绿
- [x] 4.4 清理 feature 层里因移除月份分档与目录内搜索而失效的判定与测试（`directory-filter.test.ts` 中的月份分档断言等），确认没有留下无人使用的导出；判据：`cd clients/app && npm run check` 全绿，且 `rg` 检索确认月份档相关导出只在不存在的引用里出现

## 5. 创建弹层与文件夹浏览（WP5）

- [x] 5.1 在 `clients/app/src/components/CreateSessionSheet.tsx` 实现目录块的两种形态（已知目录 → 目录/路径两行；顶部入口 → 选择文件夹控件 + 已选路径 + 说明）与四类分支提示；判据：`cd clients/app && npm run check` 全绿，`r16-create-session.test.ts` 改写后全绿
- [x] 5.2 新增 `clients/app/src/components/DirectoryPicker.tsx`：根列表、面包屑（首段为「此电脑」）、上一级、逐层列表（已登记目录标注）、新建文件夹、「选择此文件夹」（根层禁用）；数据经新命令取得，服务端未提供该能力时明确说明并保留已登记目录路径；判据：`cd clients/app && npm run check` 全绿，且用例覆盖「不支持时说明而不是静默失败」
- [x] 5.3 在 `clients/app/src/features/create-session-model.ts` 与提交链路上接上「不存在则先创建」这一个布尔（载荷只增加该键），并保持既有三类终态呈现；判据：`cd clients/app && npm run check` 全绿，且用例断言载荷键集合恰好是规范允许的三类
- [x] 5.4 把 `DirectoryPicker` 接进 `clients/app/app/index.tsx` 与 `clients/app/app/dir/[alias].tsx`；判据：`cd clients/app && npm run check` 全绿，且 `node e2e/run-e2e.mjs` 的 `r16-create` 用例全绿

## 6. 对话页骨架与输入区（WP6）

- [x] 6.1 把 `clients/app/src/components/ConversationStream.tsx` 的宽屏布局改为三栏网格（300 / 1fr / 360），窄屏沿用次级视图并扩到四项，四列内容**始终在 DOM**；判据：`cd clients/app && npm run check` 全绿，且 `conversation-responsive.test.ts` 的「两形态 `data-*` 集合相同」断言扩到四列后仍全绿
- [x] 6.2 实现左栏（目录上下文 + 目录内搜索 + 会话列表 + 已呈现数 + 继续加载 + 吸底新建会话），搜索只影响左栏；判据：`cd clients/app && npm run check` 全绿，且用例覆盖「搜索不改中栏」
- [x] 6.3 实现会话头（标题、状态徽标、Agent 名、Agent 胶囊 popover，缺覆盖层呈现状态未知）与连接状态条；判据：`cd clients/app && npm run check` 全绿，且用例断言状态未知不被写成已断开
- [x] 6.4 实现输入区组合（上下文胶囊 + 弹层、≥85% 告警条、模型/模式胶囊与弹层、输入行三态与发送/停止、提示行）与 Enter 发送 / Shift+Enter 换行；判据：`cd clients/app && npm run check` 全绿，且用例覆盖「不可提交时回车不发命令」
- [x] 6.5 实现「跳到你最近的输入」胶囊与待处理跳转条；判据：`cd clients/app && npm run check` 全绿，且用例覆盖「没有用户输入时不呈现」
- [x] 6.6 把 `clients/app/app/session/[id].tsx` 的裸 `<main><nav>` 换成共享外壳，并接上既有但未接的复制与链接回调；判据：`cd clients/app && npm run check` 全绿，`node e2e/run-e2e.mjs` 的 `conversation-interaction` 用例全绿

## 7. 对话页时间线与右栏（WP7）

- [x] 7.1 把 `clients/app/src/components/ConversationTimeline.tsx` 的条目形态改为原型：用户消息靠右气泡、Agent 气泡、代码卡（语言条 + 复制）、运行指示、失败条；判据：`cd clients/app && npm run check` 全绿，`conversation-timeline-view.test.ts` 改写后全绿
- [x] 7.2 实现结构化条目的展开收起：默认值由条目状态决定、显式操作只影响本条、带文件标识的条目选中时联动右栏；判据：`cd clients/app && npm run check` 全绿，且新增用例覆盖三条规格场景
- [x] 7.3 权限卡改为三按钮形态，并保留「本设备亲历 / 仅历史读到不可操作 / 已被其它设备处理」三种可区分呈现；判据：`cd clients/app && npm run check` 全绿，`interaction-answer-boundaries.test.ts` 原样通过
- [x] 7.4 在 `clients/app/src/components/FileChangesPanel.tsx` 增加差异行的逐行呈现（新增/删除/上下文各一档、按不可信文本处理）与三种形态（有差异行 / 有不可用标记 / 都没有），不解析 ACP 原文；判据：`cd clients/app && npm run check` 全绿，`file-changes-panel.test.ts` 改写后全绿且仍断言差异区不出现 `rawJson`
- [x] 7.5 同步 `clients/app/e2e/page/checks/conversation-timeline.ts` 与 `conversation-content.ts` 的锚点，使它们在「右栏退回只有统计的形态」时变红；判据：`node clients/app/e2e/run-e2e.mjs` 全绿，并在报告里给出「故意改回旧形态后该用例变红」的一次实测记录

## 8. 其余组件与路由外壳（WP8）

- [x] 8.1 把 `clients/app/src/components/ContextUsageBar.tsx` 迁到样式基座并改成胶囊 + 展开明细（进度条、占比、已用/上限；未上报时不显示数字）；判据：`cd clients/app && npm run check` 全绿，`r17-context-usage.test.ts` 原样通过
- [x] 8.2 把 `clients/app/src/components/DiagnosticsDrawer.tsx` 迁到样式基座并改为右侧抽屉（保留 `data-diagnostics-drawer="open"` 与 Esc 关闭）；判据：`cd clients/app && npm run check` 全绿，既有诊断用例原样通过
- [x] 8.3 把 `clients/app/src/components/DegradedEventCard.tsx` 与 `clients/app/src/components/PairingPanel.tsx` 迁到样式基座，字段与 `data-*` 集合不变；判据：`cd clients/app && npm run check` 全绿，`r18-degradation*.test.ts` 与 `node-degraded-panel.test.ts` 原样通过
- [x] 8.4 让 `clients/app/app/hosts.tsx`、`clients/app/app/pair.tsx`、`clients/app/app/node-link/pair.tsx` 改用共享外壳，配对路由按其自身形态呈现头部；判据：`cd clients/app && npm run check` 全绿，`node e2e/run-e2e.mjs` 的配对用例（`pairing-first-run`、`pairing-kinds`）全绿

## Workflow follow-up

- 本变更的验收依赖 `sync-path-browse-and-diff-delivery` 合入（路径、浏览、差异行三处呈现）；若该变更尚未合入，先完成其余工作包与验收，并把三处呈现的验收明确记为待办。
- 全部工作包合入并跑过 Project Verify 后，按 `docs/WORKFLOW.md` §6.2 走远端落地（分支、PR、六个必需检查）。
- 两个变更完成后，把 `prototypes/README.md` 与 `prototypes/IMPLEMENTATION-GAPS.md` 的现状记录刷新到与实现一致（它们不是权威文档，但今天已明显过期）。
