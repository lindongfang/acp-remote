# Design

## Context

动机与范围见 proposal.md。下面是制定方案需要的现状与约束（都已实测核对）：

- **这两页的行为层已经落地且有证据。** `clients/app/src/features/directory-model.ts` 提供 `DirectoryPageModel` / `DirectoryDetailModel`；`DirectoryList` / `DirectoryDetail` 渲染语义标记；`e2e/run-e2e.mjs` 的 R15-1～R15-7 覆盖"不泄露路径（负向）""同名以别名区分""汇总由摘要得出""两种空态可区分""详情页别名失效 vs 无会话"。
- **视觉层为零。** `clients/app` 的 `src/` 与 `app/` 下 `className`、`StyleSheet`、`style=` 的匹配数为 **0**。`app.json` 只有 `platforms: ["web"]`、`bundler: "metro"`、`output: "single"`；没有 `public/`、没有图标、没有 manifest。
- **渲染器是 react-native-web 0.21.3。** 页面目前返回的是**原生 DOM 标签**（`main`/`nav`/`ul`/`li`/`button`），不是 RN 原语——也就是说这些组件在浏览器里能跑，在原生端不能（RN 的 renderer 只认 RN 组件）。`docs/FRONTEND_DESIGN.md` §8.1 期望原生端复用"大部分页面"，这一条目前并未成立。
- **RNW 支持 `dataSet`。** `node_modules/react-native-web/dist/modules/createDOMProps/index.js:754-768` 把 `dataSet` 的键 `hyphenate` 成 `data-*` 属性；`forwardedProps/index.js:12` 把 `dataSet` 列入转发表。因此**属性名与取值可以逐一保留**，只有标签名会变（`View` → `div`、`Text` → `div[dir=auto]`、`Pressable` → `div`）。
- **交互无法由组件测试覆盖。** `clients/app/vitest.config.ts` 的 `include` 只收 `*.test.ts`，组件测试用 `renderToStaticMarkup`（无事件、无 `useEffect`），所以"输入查询词 → 列表变化"这类行为只能落在**纯函数单测**与**浏览器 e2e** 两处。
- **浏览器 e2e 不在 CI。** `.github/workflows/ci.yml` 的 `frontend` job 只跑 `npm ci` + `npm run check`（typecheck + vitest + `expo export`）；需要真 Chromium 的 `e2e/run-e2e.mjs` 仍须本机手动跑（README「合同检查」与 ci.yml 的注释都记了这条）。
- **样式取值有现成基准。** `prototypes/acp-remote-pwa.html:22-41` 定义了整套 token（浅色 `:root` 与深色 `:root[data-theme="dark"]`），`docs/FRONTEND_DESIGN.md` §9 第 1 条又把该原型定为这两页的交互口径来源。

## Goals / Non-Goals

**Goals:**

- 在不改变任何可观察行为语义的前提下，给 `clients/app` 建立**唯一一套**样式机制，并让目录页与目录详情页（含这两页里可见的共享组件）用它渲染。
- 把搜索、筛选、按月分档、目录内搜索、同步时间标注做成 **feature 层的纯函数**，使它们在 `renderToStaticMarkup` 之外可以被单测直接判定。
- 让既有 7 条 R15 浏览器用例在迁移后**继续可跑且仍然有判别力**（选择器迁移而非删除）。
- 落地方式不引入新依赖，且不把 web-only 的样式孤岛写进一个自述为"Expo/React Native 通用工程"的目录。

**Non-Goals:**

- 不改协议、`schemas/`、`fixtures/`、任何 Rust crate、`server::sync`。
- 不做对话页、配对页、`app/hosts.tsx` 独立页面、`DiagnosticsDrawer`、`DegradedEventCard`、`ContextUsageBar`、`ConversationStream` 的样式迁移。
- 不做顶栏 / logo / hostchip / 开发模式横幅 / 帮助抽屉 / 主题切换开关 / toast。
- 不做 PWA manifest、service worker、安装引导、图标、i18n。
- 不实现原型里的路径行（目录页尾段路径、详情页 `pathline`）——见 D8。
- 不改变任何 store 的同步语义（连接、重放、去重、命令幂等）；本次新增的 store 字段纯粹是 UI 状态。

## Decisions

### D1 样式机制：react-native-web 原语 + `StyleSheet` + token 模块

**选它。** 目录页/详情页及其页内可见组件改为 `View` / `Text` / `Pressable` + `StyleSheet.create`；token 集中在新建的 `src/components/theme.ts`（浅色/深色两套调色板 + 间距/字号/圆角/阴影），调色板取值直接取 `prototypes/acp-remote-pwa.html:22-41` 的 CSS 变量，保证视觉基准与原型一致。

理由：不新增依赖（`react-native-web` 已由 Expo 提供）；`StyleSheet` 的对象形状在原生端同样成立，是 §8.1"原生复用大部分页面"**唯一**不返工的路；不引入第二套样式机制。

**被否的替代：**

| 方案 | 为什么否 |
| --- | --- |
| web-only CSS 文件（Metro 在 web 支持 `import "./x.css"`） | 能几乎照抄原型、最快，但会在自述为通用工程的项目里埋一个 web-only 样式孤岛；原生端要整层重写。与 §8.1 冲突 |
| 引入样式库（nativewind / tamagui / restyle 等） | 新增依赖需过 `AGENTS.md` §8 的必要性/许可/平台审查；且 README 记录过两次"RN 大版本升级必然打断 `expo export`"（#68/#69），这块依赖面是已知脆弱点 |

**已知代价（必须承认）**：这是一次**标记迁移**，不是"加几行样式"。文本必须包在 `Text` 里（web 上字符串直接放在 `View` 里能渲染，原生端不允许；统一包住以免将来逐处排查）；`ul`/`li` 的列表语义需要用 `role` 补回；`app/_layout.tsx` 需要加一层根容器。

### D2 `data-*` 属性原样保留，只迁移选择器

所有既有属性名与取值逐一保留，改用 `dataSet` 传递（`dataSet={{ directoryAlias: alias }}` → `data-directory-alias="alpha"`，机理由 Context 里的 `createDOMProps` 证据支撑）。**渲染出的标签名会变**，所以 e2e 的选择器要同批迁移：`li[data-directory-alias]` → `[data-directory-alias]`、`ul[data-directory-list]` → `[data-directory-list]` 等。

这是把"唯一的机器证据"从"标签+属性"降为"属性"的取舍。为了补偿，迁移**不得**同时删除断言：R15-1 的负向断言、R15-2 的"两种消歧依据不许有第二种"、R15-4 的两种空态对比都必须原样保留。

### D3 判定放 feature 层纯函数，页面只渲染

`src/features/directory-model.ts` 新增（或扩展）纯函数：查询匹配、状态筛选、按月分档、会话行投影。理由是两条既有约定：`app/index.tsx` 与 `app/dir/[alias].tsx` 的注释写明"任何数据判定都在 `src/features/` 里，页面里没有业务分支"；而 `renderToStaticMarkup` 没有事件，交互无法由组件测试证明，判定必须是可被 `vitest` 直接调用的纯函数。

### D4 查询/筛选/月份是 store 的 UI 状态

在 `ClientState` 增加四个字段：目录页查询词、目录页筛选、详情页查询词、详情页月份，并配同名 setter；形状与既有 `createSheetAlias` / `diagnosticsEventId` 完全一致（字段 + setter + `#commit`）。目录页与详情页的查询词**是两个独立字段**（原型里也是两个独立的搜索框），这直接支撑 spec 的"与目录页的查询互不影响"场景。

### D5 匹配与空态口径

- 目录页：查询词命中 `displayName`、`alias`、或**该目录下任一会话的标题**；trim + 大小写不敏感。
- 详情页：只命中该目录下会话的标题。
- 空标题一律呈现为「未命名会话」（复用 `UNTITLED_SESSION_LABEL`）。
- 三层"空"必须互不冒充，且各自有独立标记：`no_directories`、`directories_without_sessions`（既有，仍由 `data-empty-state` 表达）、"搜索无结果"、"目录内搜索无结果"、"目录存在但没有会话"、"别名不在快照里"（后两者已在 `DirectoryDetail` 里有独立呈现）。新增的两种"无结果"**不占用** `data-empty-state`，以免污染既有负向断言。

### D6 分档按会话自身时间

`SessionRowModel.updatedAt` 已有（`src/features/directory-model.ts`），按它取 `YYYY-MM` 分档，档内与档间都倒序。

注意：原型此处**是错的**——`prototypes/acp-remote-pwa.html` 的 `viewDirDetail` 里 `rows.forEach(function(s){ var k=monthKey(NOW); … })` 用的是 `NOW` 而不是 `s.when`，所以原型的"按月分档"实际只会产出一个档（原型的会话时间只是展示字符串，它也没法正确分档）。本实现必须按每个会话自己的时间分档，spec 里也把这条写成了 MUST NOT。

### D7 响应式用 `useWindowDimensions()`

RNW 没有媒体查询，用 `useWindowDimensions()` + 一个断点常量决定单列/宽屏。spec 的"窄屏不隐藏信息"由"同一份数据、只改布局"保证：**窄屏与宽屏渲染的信息集合必须相同**，这是可被浏览器 e2e 断言的部分。

### D8 不实现路径行（有意偏离原型）

原型在详情页显示完整规范化路径、在目录页显示尾段路径用于同名消歧；`docs/FRONTEND_DESIGN.md` §2.2/§4.1、`SECURITY_DESIGN.md` §12.3 与既有 R15 都要求只显示展示名与别名。因此同名目录仍只用别名区分，路径行**不实现**。这条已在 proposal 的"本次不包含"里登记。

### D9 浅色/深色跟随系统，不做开关

调色板备两套，用 `useColorScheme()` 取系统偏好。产品内的主题开关属 `docs/FRONTEND_DESIGN.md` §11 的暂缓决策（"主题是否做成产品内开关…按 PWA 规范应跟随系统"），本次不引入。

### D10 验证策略（这条决定了 plan 的 Verify 列）

| 层 | 手段 | 能证明什么 | 不能证明什么 |
| --- | --- | --- | --- |
| 纯函数 | `vitest`（`*.test.ts`） | 匹配、筛选、分档、会话行投影、空态三分 | 渲染与事件 |
| 组件渲染 | `renderToStaticMarkup` 负向断言 | 渲染结果里不出现路径片段；属性与文案仍在 | 任何交互 |
| 浏览器 e2e | `node e2e/run-e2e.mjs`（真 Chromium + 真组合根 + 真快照管线） | 输入查询词后列表真的变了；分档与筛选真的生效；窄屏下信息不缺 | CI 里不会跑（本机手动） |
| 打包 | `npm run check` 的 `expo export --platform web` | 样式层没有打断打包 | 视觉正确性 |

因此：**每条新增行为的"判定"用纯函数单测，每条新增行为的"接上了"用浏览器 e2e**；`expo export` 只是不回归的下限。

## Risks / Trade-offs

| 风险 | 缓解 |
| --- | --- |
| 标签名变化让既有 7 条 R15 浏览器用例失效，若同批没迁移就会留下"红着的证据" | 选择器迁移与组件改动放在**同一个工作包**，Verify 里必须包含 `node e2e/run-e2e.mjs`；`li[...]` 这类选择器全仓清点后一并改（`grep -rn "li\\[" clients/app/e2e`） |
| 浏览器 e2e 不在 CI，回归只能本机跑；改完没人自动挡 | 在 plan 的 Project Verify 里把 `e2e/run-e2e.mjs` 列为**必跑**项并写明需要 Chromium；合入前由主检出执行 |
| 标记迁移过程中"顺手"改了文案或属性，削弱既有负向断言的判别力 | 检视聚焦：`data-*` 属性名/取值逐个比对、两套空态文案与 `SECRET_FRAGMENTS` 断言未被动过 |
| 只迁移两个页面 → 与仍是无样式原语的对话页/配对页/`hosts` 页视觉不一致 | 明确登记为本次的 Non-Goal；这两页的组件（`ConnectionBadge`/`HostConnectionPanel`/`CreateSessionSheet`）因在这两页可见而一并迁移，其余留给后续变更 |
| `Text` 包裹是逐处机械改动，容易漏（漏了 web 上也看不出来，原生端才炸） | 迁移时统一规则：`View` 内不出现裸字符串；检视时对 patch 做一次"裸字符串"扫描 |
| 深色配色取错（原型有两套变量，容易只抄浅色） | token 模块必须同时含浅/深两套，取自 `prototypes/acp-remote-pwa.html:22-41` |
| 搜索引入新的列表分支，可能让"两种空态"被合并成一句话 | spec 明确三层空态互不冒充；e2e 保留 R15-4 的对比断言，并新增"搜索无结果不占用 `data-empty-state`"的断言 |

## Open Questions

- 内联会话行的展示上限取值（原型 `MAX_ROWS = 5`）与折叠默认态（展开 / 折叠）——可在实现时按原型取 5 与"展开"，spec 只要求"有上限且超出时给查看全部"。
- 时间文案的粒度（"已同步 14:33" 还是 "2 分钟前"）——按原型的两种都实现即可，spec 只要求"时刻或距今时长"。
