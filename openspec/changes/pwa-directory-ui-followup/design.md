# Design

## Context

动机与范围见 proposal.md。下面是制定方案需要的现状（都已核对到具体代码位置）：

- **死路的两个成因**：
  1. `clients/app/src/domain/directory.ts` 的 `buildDirectoryViews` 用 `hasAnySession` 判断：只要没有任何目录含会话，就返回 `{ kind: "empty", empty: "directories_without_sessions" }`，**不携带任何目录项**；`DirectoryList` 在该分支只渲染 `EmptyState`（`DirectoryPageModel.empty` 的字段里本来就没有目录）。
  2. `DirectoryList` 里进入详情页的唯一入口是卡片的 `onOpenAll`，而它是 `directory.counts.total === 0 ? null : …`；`DirectoryCard` 的标题是纯 `Text`（不可点），卡片根节点也没有 `onPress`。
  两者叠加：**零会话的机器上没有通往任何详情页的路径**，而空态文案恰好写着"在下面选一个目录进入详情页"。
- **既有用例把这条行为锁住了**：`e2e/page/checks/r15-directory.ts` 的 R15-11 断言"0 条会话的目录项内不呈现「查看全部」入口"（只做对照，没有要求存在别的入口）；R15-4 只比对两种空态的标记与文案，不检查空态里是否有可达入口。
- **原型的对应做法**：`prototypes/acp-remote-pwa.html` 的 `viewDirs` 在卡片头部给出 `dwhen`（「最近 X」）与 `data-new`（「⊕ 新建」）；它的卡片标题同样不可点，`新建` 受 `A.canCreate` 门控——**所以原型在无创建权限时也进不去**，这是我们要故意优于原型的一点。
- **RNW 的标签映射**：`react-native-web` 0.21.3 按 `role` 选语义标签（`list→ul`、`listitem→li`、`button→button`、`heading(+aria-level)→h1..h6`、`paragraph→p`）。因此 `EmptyState` 的标题/指引与 `CreateSessionSheet` 的标题目前只能靠 `h2`/`p` 标签定位；`DiagnosticsDrawer` 仍是原生 `<pre>`（未参与上一轮的迁移）。
- **测试替身缺陷**：`e2e/page/router-stub.tsx` 的 `Stack` 直接 `return current.component()`——那是在 `Stack` 自己的 fiber 上调用组件函数，路由组件的 hooks 会与 `useSyncExternalStore` 共用同一批槽位；切换到一个 hooks 数量/顺序不同的路由组件时抛错并整棵树卸载。上一轮实测：改成 `createElement(current.component)` 后详情路由停在 `RuntimeUnavailable`，**原因未查明**。
- **sheet 状态**：`ClientState.createSheetAlias` 只被写入（`openCreateSheet`/`closeCreateSheet`）从未被读取；详情页另有一个本地 `sheetOpen`。目录页要加创建入口，不能变成"两个页面各持一个开关"。

## Goals / Non-Goals

**Goals:**

- 关掉死路：零会话（无论"全部为零"还是"部分为零"）的目录都能进入详情页，且详情页的创建入口随之可达。
- 与原型对齐两项呈现（最近活动时间、卡片级创建入口），并**在无权限这一点上优于原型**（仍可从标题进入详情页）。
- 用真浏览器用例覆盖"点入口进入详情页"与"返回目录页后查询词仍在"，消除对标签映射的依赖。
- 全程不改变既有 `data-*` 的语义，除本文件 D3 显式声明的两处命名调整。

**Non-Goals:**

- 不引入第二种创建路径：卡片级入口与详情页入口共用同一条 `session.create`、同一份 `createSessionEntryModel`、同一个 `CreateSessionSheet`。
- 不改 `client-store` 的连接/命令语义；不新增 store 的持久化字段（本次只新增只读派生与"派生自既有字段"的读取）。
- 不做目录排序/分组策略（保持工作区登记顺序）、不做卡片内的搜索高亮。
- 不做主题、顶栏、PWA 壳层。

## Decisions

### D1 死路用两条互补的修复，缺一不可

1. **空态也列出目录**：`DirectoryPageModel` 的 `empty` 分支带上 `directories`（`no_directories` 时为 `[]`），`DirectoryList` 在两种空态下都保留空态标记与文案，并在有目录时**同时**渲染目录列表。
2. **卡片标题成为进入详情页的入口**：`DirectoryCard` 的标题改成一个可点元素（`role="button"` + 新属性），与「查看全部」并存。

为什么两条都要：只做 (1)，"部分为空"时（别的目录有会话，某目录 0 会话）该卡片仍进不去；只做 (2)，"全部为空"时根本没有卡片可点。而只做原型的「新建」入口同样不够——**无创建权限时它就是不可用的，用户依然进不去**。

### D2 `empty` 带上目录，但不并入 `ready`

保留三种 kind，`empty` 增加 `directories` 字段。理由：`data-empty-state` 只在 `empty` 出现，是 `specs/pwa-web-client` 里「两种空态可区分」那条 Requirement 的机器判据（R15-4 就查它）；把"有目录但无会话"并入 `ready` 会让该标记消失，等于静默改掉一条既有契约。合并的点只到"模型携带目录项"为止。

### D3 两个入口的动作属性命名

- 卡片**标题**：`data-action="open-directory"`。
- 「查看全部 N 个」：由 `data-action="open-directory"` 改为 **`data-action="open-directory-all"`**。

理由：两个入口指向同一页面但语义不同（"进去看看"vs"看全部会话"），同名会让选择器出现歧义，也会让"有会话时保留查看全部"这条新 Requirement 无法被独立断言。代价是 `r15-directory.ts` 的 R15-10/R15-11 要跟着改（既有断言，不是新增门禁）。`[data-directory-alias]` 仍然只在卡片根节点出现一次——**不得**把它挂到新按钮上（R15-1 数它的个数）。

### D4 最近活动时间

数据来源：该目录下会话 `updatedAt` 的**最大值**。用字符串比较即可——`Timestamp` 是固定形状的 UTC 串（`src/protocol/common.ts` 的 `$defs.timestamp`），字典序即时间序；**不**引入 `Date` 的本地时区换算。呈现格式与目录页的同步时刻保持一致：`YYYY-MM-DD HH:MM UTC`（UTC 固定切片，任何机器渲染同一段文本），另给 `dataSet={{ directoryLatestAt: "<原值>" }}` 供精确断言。该目录没有会话时不呈现（也不编造）。

### D5 卡片级创建入口复用既有三条链路，并把"打开哪一目录的弹层"收敛成一份状态

- 权限：沿用 `createSessionPermission(scopes)` 与 `store.createEntry(alias)`（判定只与 scopes 有关，与别名无关）。
- 命令：同一份 `CreateSessionEntryModel` → `store.submitCreateSession({ workspaceAlias, agentId })`；**不新增命令、不新增载荷字段**。
- 弹层：复用 `CreateSessionSheet`。
- **状态收敛**：`sheetOpen` 改为由 `store.createSheetAlias` 派生（`store.createSheetAlias === alias`），目录页与详情页共用这一份；详情页删掉本地 `sheetOpen`（`createSheetAlias` 从此不再是一份只写不读的状态）。理由是防止真 expo-router 在多屏栈里让两个页面的开关同时成立。选中 Agent 仍是页面局部状态（不跨页存活）。
- 无创建权限时：卡片**不呈现**该按钮（保持卡片干净），权限说明由页面顶部的提示条承担——这条已写进 spec（"入口 MUST 不可用（不呈现，或呈现为不可用），且页面 MUST 呈现说明"）。

### D6 修 `router-stub` 的跨路由渲染

**根因（已实测复现）**：`Stack` 里的 `return current.component()` 是在 `Stack` 自己的 fiber 上**调用**路由组件函数，路由组件的 hooks 与 `Stack` 的 `useSyncExternalStore` 抢同一批槽位；切到 hooks 序列不同的组件（目录页 ↔ 详情页）时 React 拿上一轮 effect 的 deps 去比对（`areHookInputsEqual` 读 `prevDeps.length`，那是 `undefined`）→ 抛 `TypeError` → 整棵树卸载。

**修法**：`Stack` 改为 `createElement(current.component, { key: current.path })`——路由组件成为子元素、hooks 归它自己的 fiber。`key` 取路径的实测价值在**同类型路由之间**切换：没有它时上一屏的局部状态（例如弹层里选中的 Agent）会被带进下一屏；对"目录页 ↔ 详情页"这种类型切换，两种写法都会重挂，因此它**不是**那两条跨路由用例的前提。

**顺带修掉的两处同源缺陷**（同属测试替身，实施中发现）：①`mountRoute` 没把初始路径压栈，`navigate` 只压目标路径，于是第一次 `goBack()` 弹完即无路可退——详情页的「返回目录」是个没有反应的按钮；②`registerRoutes` 累积而不是整表替换，模块级注册表会让上一条用例注册过的路径在本条里仍可导航（"本条忘了给 `extraRoutes`"不红，而是静悄悄点到本该不可达的页面）。

**更正上一轮的结论**：上一轮报告的"改成 `createElement` 后详情路由停在 `RuntimeUnavailable`"**与本修法无关**——那是探针的模块实例分裂（临时目录里的替身与 `./router-stub` 是两份模块，`useLocalSearchParams` 因此拿到 `undefined`，撞上详情页自己的 `typeof alias !== "string"` 守卫）。真实 `run-e2e.mjs` 的插件把 `expo-router` 指向同一份文件，不存在该问题。

实现者必须先**复现**当前崩溃、再验证修复，并给出机制说明。**不允许**为了让用例变绿而弱化断言或改成"直接挂详情路由"来回避跨路由。

### D7 去标签化的做法与边界

- `li[…]`、`pre[…]` 直接去掉标签前缀（属性已唯一）。
- 没有属性可查的 `h2`/`p`：在**组件**上补显式锚点——`EmptyState` 的标题与指引各加一个 `data-*`（如 `data-empty-state-title` / `data-empty-state-guidance`），`CreateSessionSheet` 的标题加一个（如 `data-create-sheet-title`）。只加属性，**不改文案与 props 契约**。
- 边界：不为了去标签化去改 `DiagnosticsDrawer` 的渲染结构（它本次不在范围内，`pre` 只需去掉前缀即可）。

## Risks / Trade-offs

| 风险 | 缓解 |
| --- | --- |
| `router-stub` 无法安全修复（上一轮已失败一次） | 先复现、再定位；若确实修不动，本变更**不**声称覆盖那两条场景，必须在 `plan.md` 的未实现/未覆盖项里显式登记，并由检视确认——**不允许**用弱化断言或跳过场景来代替 |
| 卡片标题可点会诱导把 `data-directory-alias` 挂到按钮上，破坏 R15-1 的"每项恰好一处" | 写进 WP 的判据与检视重点；R15-1 的计数断言在本次必须保持有效 |
| 目录页也挂 `CreateSessionSheet` 后，两个页面可能同时持有打开状态 | D5 的派生方案（单一来源 `createSheetAlias`）保证**状态只有一份**；但"同一时刻至多一个弹层"这条**不能**由浏览器用例证明——e2e 的路由替身每次只渲染一个路由，第二份弹层在那里结构上不可能出现。真实 expo-router 的多屏栈若保留下层屏幕，下层页面会因同一份 `createSheetAlias` 命中同一别名而**同样**渲染一份弹层（反过来，从详情页返回目录页也不会触发挂载复位）。该风险已登记为本变更的未覆盖项（见 `plan.md` 的「已知未覆盖项」），修法需要"当前聚焦路由"信号（如 `useFocusEffect`），超出本变更的写范围 |
| 卡片信息变多（最近活动 + 两个入口）在窄屏挤掉会话行/汇总 | 沿用既有断点 720 与既有"窄/宽信息集合相同"的断言；新增元素一律参与该集合比对 |
| 空态下同时呈现空态标记与目录列表，可能被读成"自相矛盾" | 文案保持不变、列表在其下方；新增一条断言「空态存在且目录项存在」并检查 `data-empty-state` 仍只在 `empty` 出现 |
| 改 `data-action` 取值会让既有断言静默失效（若只改组件不改用例） | R15-10/R15-11 与 `open-directory` 相关的查询在同一工作包内一并更新；Project Verify 的浏览器用例是硬判据 |

## Open Questions

- `no_directories` 空态下是否也呈现一个"去登记目录"的指引按钮（电脑端才有这个动作，网页端做不了）——保持不呈现。
- 卡片是否按最近活动排序——保持工作区登记顺序（排序属于产品决策，不在本次）。
