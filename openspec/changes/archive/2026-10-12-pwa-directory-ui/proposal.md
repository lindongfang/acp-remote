<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

用户要求「实现目录页和目录详情页」，并补充「只实现 UI」——即不碰协议、不碰服务端、不碰其它页面。对照现状，这两页欠着两笔不同的账：

1. **视觉层为零。** `clients/app` 的 `src/` 与 `app/` 下 `className`、`StyleSheet`、`style=` 的出现次数是 **0**（实测 `grep -rn` 计数为 0）；组件输出的是 `data-*` 语义标记（它们存在是为了给 `clients/app/e2e/` 当选择器）。也就是说这两页现在只有结构与数据，没有界面——"实现了 70%" 只在行为轴上成立，用户什么都看不见。
2. **权威文档已经承诺、但还没有实现的行为。** `docs/FRONTEND_DESIGN.md` §4.1 的"必须实现"列表里有 **目录搜索**与**响应式手机布局**；§9 第 1 条把原型定为这两页的**交互口径来源**，并点名要求"目录为主角的列表、**按月分档的详情**"。这三项在 `clients/app` 里都不存在，`openspec/specs/pwa-web-client/spec.md` 的 11 条 Requirement 里也没有一条覆盖它们。

之所以现在做：这两页的数据与判定已经落地且有浏览器级证据（`e2e/run-e2e.mjs` 的 R15-1～R15-7 共 7 条、21 条全绿），**行为面已经稳定到可以只在其上铺界面**；反过来，继续等下去只会让"文档承诺 vs 实现"的差距越积越多。

**用户原始要求**：实现目录页与目录详情页；只实现 UI；范围取"视觉 + 这两页的 UI 完整度（tier 2）"。

**硬约束**：

- 不改协议 wire、`schemas/`、`fixtures/`，不改任何 crate，不碰 `server::sync`。
- 不新增第三方依赖。
- 现有 `data-*` 选择器与其取值、两套空态文案、"目录项不泄露本机路径"的负向断言，全部必须继续成立——它们是目前唯一的机器证据。

**成功判据**：目录页与目录详情页在浏览器里呈现为可用的界面；搜索、状态筛选、按月分档、目录页内联会话行、同步时间标注都可用；窄屏下单栏可用；`clients/app` 的 `npm run check`（typecheck + vitest + `expo export`）与 `e2e/run-e2e.mjs` 全绿。

## What Changes

**一、建立客户端样式层**

- 新增 `clients/app/src/components/theme.ts`（设计 token：浅/深两套调色板、间距、字号、圆角，取值基准是原型 `:root` 的 CSS 变量）与 `useTheme()`，以及三个新的展示原语（会话行、分段控件、目录卡片骨架）。渲染一律用 `react-native-web` 的 `View`/`Text`/`Pressable` + `StyleSheet`，不额外包一层原语。
- 目录页与目录详情页，以及**在这两页里可见**的共享组件（`DirectoryList`、`DirectoryDetail`、`EmptyState`、`StatusPill`、`ConnectionBadge`、`HostConnectionPanel`、`CreateSessionSheet`、`RuntimeUnavailable`）改用该层渲染。
- 不引样式库、不写 web-only CSS 文件（取舍见 design.md D1）。

**二、目录页（`#/dirs`）**

- 目录卡片：展示名 + 别名 + 会话总数/运行中/待处理 + 最近活动时间；卡片可折叠。
- **搜索框**：按目录展示名/别名与该目录下的会话标题匹配（§4.1 要求）。
- **状态筛选段**：全部 / 运行中 / 待处理。
- **目录内联会话行**（原型：最多 3～5 条）+「查看全部 N 个 ›」。
- **同步时间标注**：在线时显示"已同步 HH:MM"，断连时显示"离线 · N 分钟前同步"（消费已建模但从未渲染的 `lastSyncedAtMs`）。
- 无 `session.create` 权限时的提示条（现在只在详情页按钮旁表达）。
- 搜索无结果时呈现"没有匹配的结果"，与既有两种空态区分。

**三、目录详情页（`#/dir/<alias>`）**

- 页头（展示名、别名、会话总数）+ 会话行列表（标题、Agent、状态）。
- **目录内搜索**与同一套状态筛选；"当前显示 N / M"。
- **按月分档**：按会话自身的时间分组（§9.1 要求），月份分段可切换。
- 底部创建入口旁补"将在你的电脑上启动一个 Agent 进程"副作用提示。

**四、把上述判定放进 feature 层**

- `src/features/directory-model.ts` 增加纯函数：查询匹配、状态筛选、按月分档、会话行投影。
- `src/features/client-store.ts` 增加四个 UI 状态（目录页查询词、目录页筛选、详情页查询词、详情页月份），形状与既有 `createSheetAlias` / `diagnosticsEventId` 一致。
- 页面因此仍然"没有业务分支"，与 `app/index.tsx` 现有约定一致。

**五、响应式**

- 按 `useWindowDimensions()` 断点做窄屏单栏 / 宽屏多栏（§4.1 要求"响应式手机布局"）。

**六、规格与文档**

- `openspec/specs/pwa-web-client/spec.md` 新增 4 条 Requirement（搜索与筛选、按月分档与目录内搜索、同步时间与离线标注、宽度自适应布局）。
- `docs/FRONTEND_DESIGN.md` §4.1 补上筛选段与同步时间标注的呈现口径（§4.1 已要求搜索与响应式，无需新增；筛选段与同步时间标注是原型独有、文档未写的）。

**本次不包含**：

- 协议、`schemas/`、`fixtures/`、任何 Rust crate、`server::sync` 与 `/ui` 静态托管。
- 对话页、配对页、`app/hosts.tsx` 独立页面、`DiagnosticsDrawer`、`DegradedEventCard`、`ContextUsageBar`、`ConversationStream`。
- 顶栏 / logo / hostchip / 开发模式横幅（`FRONTEND_DESIGN` §4.4 的全局壳），以及主题切换。
- PWA `manifest.json`、service worker、安装引导、图标。
- 新依赖、i18n、表单库/状态库选型。
- 原型的 6 位 SAS、配对入口（配对页不在本次范围）。
- **原型里显示本机路径的行**（目录页尾段路径、详情页 `pathline`）：`FRONTEND_DESIGN` §2.2/§4.1 与 `SECURITY_DESIGN` §12.3 明令只显示展示名与别名，且现有 e2e 有负向断言。这是**有意的偏离**，不是缺口。

## Capabilities

### New Capabilities

无。本次不引入新的能力边界：目录页与目录详情页的呈现属于既有 `pwa-web-client` 能力。

### Modified Capabilities

- `pwa-web-client`: 新增"目录页的搜索与状态筛选""目录详情的按月分档与目录内搜索""断连时的同步时间标注""按设备宽度的自适应布局"四条 Requirement，并把"目录页内联会话行与查看全部"作为目录页呈现的一部分明确下来。

## Impact

- **前端代码**：`clients/app/src/components/`（新增 `theme.ts` 与展示原语；改 `DirectoryList.tsx`、`DirectoryDetail.tsx`、`EmptyState.tsx`、`StatusPill.tsx`、`ConnectionBadge.tsx`、`HostConnectionPanel.tsx`、`CreateSessionSheet.tsx`、`RuntimeUnavailable.tsx`、`index.ts`）、`clients/app/src/features/`（`directory-model.ts`、`client-store.ts`）、`clients/app/app/`（`index.tsx`、`dir/[alias].tsx`）。
- **测试**：`clients/app/src/**` 下新增/修改的单测；`clients/app/e2e/page/checks/r15-directory.ts` 的选择器迁移（标签名从 `ul/li/button` 变成 react-native-web 输出的 `div`，`data-*` 属性名与取值不变）与新增用例。
- **依赖**：不新增任何依赖。`clients/app/package.json` 与锁文件不变。
- **合同资产**：`schemas/`、`fixtures/`、`compatibility/` 一行不改；`npm run check` 的十道脚本判定面不变。
- **权威文档**：`docs/FRONTEND_DESIGN.md`（§4.1 的呈现口径）。`docs/MODULE_ARCHITECTURE.md`、`docs/SYNC_PROTOCOL.md`、`SECURITY_DESIGN.md`、`CORE_PORTS_AND_STORAGE.md` 无需改动——本次不改变模块边界、协议语义、安全模型与端口签名。
- **风险**：DOM 标签名变化会让既有 7 条 R15 浏览器用例的 `li[...]` 选择器失效，必须同批迁移；浏览器 e2e **不在 CI**（需要真 Chromium），所以这一步的回归只能在本机跑；其它页面（对话页、配对页、`hosts` 页）本次仍是无样式原语，视觉上会不一致（见 design.md 的 Non-Goals 与 Risks）。
