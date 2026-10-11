<!-- 执行安排：把 specs 的行为要求与 design 的 D1–D7 转成可派发的并行工作包。
     plan 只写“谁在哪个隔离工作区做什么、按什么顺序合入、怎么验”，不重复 specs/design 的内容。

     本仓库的执行原语是 Oh My Pi（omp）：
     - 工作包由主 Agent 用 `task` 工具的 batch 形状扇出（共享 `context` + 逐包 `task`/`solutionSpace`）；
     - 每个工作包跑在隔离工作区（`.omp/config.yml` 的 `task.isolation`：`merge: patch`、`apply: false`）；
     - 交付物是 `<id>.patch` + `agent://<id>` + `history://<id>`，不产生 commit、不产生分支；
     - 合入由主 Agent 串行 `git apply --check` → `git apply`，每次合入后跑主检出 Project Verify；
     - 隔离工作区**没有** `clients/app/node_modules`（`docs/WORKFLOW.md` §4.3），因此
       `tsc` / `vitest` / `expo export` / `e2e/run-e2e.mjs` 一律只在主检出跑。 -->

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出多个工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）。工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；工作包不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`，每次合入后跑主检出 Project Verify。
- 验证：隔离工作区内**没有** `clients/app/node_modules`；子 Agent 不得尝试 `npm run check`、`vitest`、
  `tsc`、`expo export`、`e2e/run-e2e.mjs`，可以**只读**借用主检出的 `node_modules`（绝对路径）做渲染/解析探针。
- 检视：每个工作包由一个**独立** spawn（不与作者同实例）读 patch、`agent://<id>` 与 `history://<id>` 后给出结论。
- 派发前主检出工作区应相对干净；本次起点是 `main` 上已归档 `pwa-directory-ui` 的状态。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | design D1/D2/D4 的判定层：`DirectoryPageModel.empty` 带上已登记目录；`DirectoryRowModel` 增加目录的最近活动时间 | none | wp-coder | `clients/app/src/features/directory-model.ts`, `clients/app/src/features/r15-directory-model.test.ts`, `clients/app/src/features/directory-filter.test.ts`, `clients/app/src/features/testing/fixtures.ts` | patch：模型与纯函数 + 单测；报告给出两条关键口径的证据（"全部为空"时 `empty.directories` 非空且带零汇总；最近活动时间取该目录下 `updatedAt` 的最大值，并有一个"不是第一条、也不是当前时间"的反例） | 主检出 PV2 |
| WP2 | design D1/D3/D4/D5/D7 的组件层：卡片标题成为进入详情页的入口、「查看全部」改用新动作值、卡片呈现最近活动时间与目录级创建入口、空态下同时渲染目录列表、`EmptyState`/`CreateSessionSheet` 补 `data-*` 锚点 | code:WP1 | wp-coder | `clients/app/src/components/DirectoryCard.tsx`, `clients/app/src/components/DirectoryList.tsx`, `clients/app/src/components/EmptyState.tsx`, `clients/app/src/components/CreateSessionSheet.tsx`, `clients/app/src/components/r15-directory-list.test.ts` | patch：组件改动 + 组件级断言；报告给出属性对照（`[data-directory-alias]` 仍每项一处；新增/改名的动作值逐条列出）与空态+列表同时呈现的证据 | 主检出 PV2 |
| WP3 | design D5：目录页挂载创建弹层并把打开状态收敛为单一来源；详情页删掉本地 `sheetOpen` | code:WP1, code:WP2 | wp-coder | `clients/app/app/index.tsx`, `clients/app/app/dir/[alias].tsx` | patch：两页的接线；报告说明 `createSheetAlias` 如何成为唯一来源、以及"同一时刻只有一个弹层"如何成立 | 主检出 PV2 |
| WP4 | specs 的浏览器级覆盖：修 `router-stub` 的跨路由渲染；补"点入口进详情页""返回目录页查询词仍在""权限提示条""零会话目录可达"用例；把剩余标签型选择器去标签化；更新被 D3 改名的既有断言 | code:WP3 | wp-tester | `clients/app/e2e/page/router-stub.tsx`, `clients/app/e2e/page/checks/r15-directory.ts`, `clients/app/e2e/page/checks/r15-directory-detail.ts`, `clients/app/e2e/page/checks/r16-create.ts`, `clients/app/e2e/page/checks/r18-degradation.ts`, `clients/app/e2e/page/checks/composition-root.ts`, `clients/app/e2e/page/index.tsx`, `clients/app/e2e/page/fixtures.tsx` | patch：替身修复 + 新用例 + 选择器迁移；报告必须给出**替身缺陷的复现证据**（修复前怎么红）与修复后的机制说明（为什么 key 是必要的），并逐条列出新增用例的判别力 | 主检出 PV3 |
| WP5 | 呈现口径落到权威文档：`docs/FRONTEND_DESIGN.md` §4.1 补四项（空态仍列目录、卡片标题入口、最近活动时间、目录级创建入口，含"无权限时说明由页面承担"）；`prototypes/IMPLEMENTATION-GAPS.md` 的对照表同步 | none | wp-coder | `docs/FRONTEND_DESIGN.md`, `prototypes/IMPLEMENTATION-GAPS.md` | patch：只改这两处叙述；报告给出改动前后段落 | 主检出 PV1 |

**跨包规则：**

- WP2 不得把 `data-directory-alias` 加到新的按钮上（R15-1 数它的次数）；新增的 `data-*` 一律新增，不改既有取值，唯一例外是 D3 声明的 `open-directory` → 标题、`open-directory-all` → 查看全部。
- WP4 独占 `clients/app/e2e/**`；WP2/WP3 不得改 e2e。
- `clients/app/src/features/testing/fixtures.ts` 由 **WP1 独占**（WP2/WP3 需要额外夹具时在自己的测试文件里构造）。
- 组件测试文件是 `*.test.ts`（`vitest.config.ts` 的 `include` 只收 `.test.ts`），用 `createElement` + `renderToStaticMarkup`，不得引入 JSX 形式的 `.test.tsx`。
- 新增的组件 props **一律可选**（缺省等价于不启用），保证既有调用点继续编译。

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 写范围无重叠（feature 层） | PV2 |
| 2 | WP2 | 写范围无重叠（components 层），依赖 WP1 的模型字段 | PV2 |
| 3 | WP3 | 写范围无重叠（两个页面文件），依赖 WP2 的 props | PV2 |
| 4 | WP4 | 写范围无重叠（e2e），但它断言的是 WP2/WP3 产出的 DOM 与交互；必须最后合入 | PV3 |
| 5 | WP5 | 写范围无重叠（两份文档），与代码无耦合 | PV1 |

**关于中间状态：** WP2 改名了动作属性取值（D3），而更新 e2e 断言的 WP4 在其之后合入，因此
**WP2 合入之后、WP4 合入之前 `node e2e/run-e2e.mjs` 会是红的**（R15-10/R15-11 仍按旧取值查找）。
起点是 **WP2** 而不是 WP3：D3 的取值改名落在 WP2 的 `DirectoryCard.tsx`，而把既有断言换成新取值的 WP4 在其后合入；
WP3 与本次红窗口无关。（独立验证已指出这一点并给出复算依据，本行据此订正。）
该窗口内只跑 PV2、不对 e2e 下结论；e2e 的结论只在 WP4 合入后由 PV3 给出。若 WP4 失败导致窗口无法关闭，
回退 WP2/WP3 重新拆分——不允许用"保留旧取值"的方式绕过。

### 实测记录（本次执行，全部在主检出）

| Check | 结果 |
| --- | --- |
| 基线（未改动） | `npm run check`（`clients/app`）= 40 文件 / 518 用例；`node e2e/run-e2e.mjs` = `32/32 passed` |
| 死路复现（变更起点证据） | 只读探针渲染真实组件：①全部目录零会话 → `kind=empty`、目录项 0、`data-empty-state` 1、入口 0、**空态指引存在**；②部分目录零会话 → 2 个目录项但只有 1 个入口（0 会话那项无入口） |
| PV2（每次合入后） | WP1 → 523、WP2 → 528、WP3 → 528、WP5 + 文档订正 → 528；均 typecheck 干净 + `expo export` 成功 |
| PV1 | 仓库根 `npm run check` 十道门禁全绿（`check:docs` 384 链接、`check:openspec` 的 `--strict`） |
| PV3 | `node e2e/run-e2e.mjs` → **`39/39 passed`**（既有 32 条 + 新增 7 条：R15-19..R15-24、R16-4）；`__pageErrors` 为空（runner 里非空即硬失败） |
| 独立验证 | `ValidateFollowup`（非作者）复核：死路确已关闭（自行复跑 39/39 + 代码核对）、4 条 Requirement 逐条有证据、既有契约未削弱、未覆盖项登记与代码一致；1 条 MINOR（红窗口起点应为 WP2）已据此订正 plan/tasks |
| 辅助目视 | browser prelude 打开真实页面（真实组合根 + 假主机）：**有会话**的目录页每张卡片显示 `api alpha/beta`、三项汇总、`最近 2026-10-05 09:12 UTC`、`⊕ 新建`、`查看全部 6 个 ›`（`titleEntries=2 / allEntries=2 / createEntries=2`）；**零会话**时空态「目录已登记，但还没有会话 / 在下面选一个目录进入详情页…」下方真的列出两张卡片（各带标题入口与 `⊕ 新建`），`__pageErrors` 为空 |

**证据面更正（上一轮遗留）**：上一轮 `pwa-directory-ui` 记录的"把 `Stack` 改成 `createElement` 后详情路由停在 `RuntimeUnavailable`"**与修法无关**，那是探针的模块实例分裂造成的假象（临时目录里的替身与 `./router-stub` 是两份模块 → `useLocalSearchParams` 返回 `undefined` → 撞上详情页自己的守卫）。本轮 WP4 复现了真正的根因（hooks 槽位错配 → `areHookInputsEqual` 读 `undefined.length`）并修好。

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `npm run check`（仓库根） | 十道合同门禁全绿（含 `check:docs` 与 `check:openspec` 的 `--strict` 校验） |
| PV2 | `npm run check`（`clients/app`） | typecheck 通过；`vitest run` 全过；`expo export --platform web` 成功产出 `dist` |
| PV3 | `node e2e/run-e2e.mjs`（`clients/app`；需本机 Chromium 内核或 `BROWSER_PATH`） | 全部 PASS，且**至少**覆盖：既有 32 条（选择器迁移后仍全绿）、跨路由进入详情页、返回目录页后查询词仍在、目录页权限提示条、零会话目录可达、卡片级创建入口发出的是两引用载荷、替换后的动作属性取值。这是本变更的 Main E2E 入口 |

仓库根 `npm run check` 与 `check:rust` **不覆盖** `clients/app`（它是独立 npm 工程，CI 的 `frontend` job 才跑它），
因此 PV2 必须单独执行。

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | `empty.directories` 的口径（`no_directories` 为空数组、零会话时带零汇总且不被过滤掉）；最近活动时间是**最大值**且不经过本地时区；既有"目录项只有展示名与别名"的字段约束未被新字段破坏（不得引入路径类槽位） | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | `[data-directory-alias]` 仍每项恰好一处；两个入口的动作取值可区分且与 plan/design 一致；空态与目录列表能同时呈现且 `data-empty-state` 仍只在 `empty` 出现；新增锚点只是属性、未改文案与 props 契约；`DirectoryCard` 仍是纯函数（判定来自 props） | 无 CRITICAL/MAJOR 未闭环项 |
| WP3 | reviewer | `sheetOpen` 是否真的收敛为单一来源（`createSheetAlias===alias`），不存在两个页面各持一份；目录页发出的创建命令与详情页是**同一条**（不含新增载荷字段）；未改动 store 的连接/命令语义 | 无 CRITICAL/MAJOR 未闭环项 |
| WP4 | reviewer | 替身缺陷的**复现证据**是否可读（修复前怎么红）；`key` 的必要性是否有机制说明而不是"加上就好了"；新增用例是否有判别力（实现回退时会红）；去标签化是否保留了断言强度（尤其 R15-1 的计数、R15-2 的双重消歧、R15-4 的空态对比、R18 的负向断言）；有无为了变绿而弱化或删除断言 | 无 CRITICAL/MAJOR 未闭环项 |
| WP5 | reviewer | 文档只改 §4.1 与缺口清单；新增四项与 `specs/` 的措辞一致（尤其"无权限时不呈现卡片入口、说明由页面承担"这一条不得被写成"隐藏入口即可"） | 无 CRITICAL/MAJOR 未闭环项 |

## Main E2E

```yaml
mode: required
```

入口是既有的 `clients/app/e2e/run-e2e.mjs`（真实 Chromium 内核 + CDP；挂真实 `app/_layout.tsx` 组合根与真实路由组件，
快照经真实两级摘要校验）。本次必须断言的可观察行为：

- 从目录页点击目录项（标题）与「查看全部 N 个」两条路径都能进入该目录的详情页；返回目录页后查询词仍在。
- 零会话的两种情形（全部目录无会话、部分目录无会话）都能从目录页进入某个目录的详情页。
- 卡片级创建入口发出的命令载荷仍只有 `{ workspaceAlias, agentId }` 两个引用；无权限时该入口不呈现且页面有说明。
- 目录卡片呈现该目录下最晚的会话时间；无会话时不呈现。
- 既有 32 条用例在选择器迁移与动作取值调整后仍全绿。

只启动成功、退出码为零、截图都不算通过。

## 已知未覆盖项

<!-- 检视与实施过程中发现的、本变更**不**声称解决的项。每一项都必须写清"没覆盖什么、为什么、留给谁"。 -->

| # | 未覆盖 | 原因 | 处置 |
| --- | --- | --- | --- |
| 1 | 「跨屏共享弹层」：真实 expo-router 的多屏栈若保留下层屏幕，目录页与详情页会因同一份 `createSheetAlias` 同时渲染一份 `[data-create-sheet]`；从详情页返回目录页时目录页也不会重跑挂载复位，因此会"跟着"呈现用户没在那里打开的弹层 | 修法需要"当前聚焦路由"信号（如 `useFocusEffect`），而 e2e 的路由替身没有 NavigationContainer、直接用它会抛错，属于替身能力的扩展而非本变更的接线 | 登记为未覆盖；`plan.md` 的 PV3 断言只覆盖"替身单路由下至多一份弹层"，不得被读作该风险的证据。修法留给下一次（与 `router-stub` 的进一步改造一并考虑） |
| 2 | 目录卡片体在 0 会话时是空白（原型有「这个目录还没有会话」提示行） | 不属于本次 4 条 Requirement 的任何一条；可达性已由标题入口解决 | 已写进 `prototypes/IMPLEMENTATION-GAPS.md` 的对照表为"仍缺"；下次补齐 |
| 3 | 既有 R15 句「目录项 MUST 只显示展示名与别名」与"目录项呈现会话汇总/最近活动时间"之间的措辞张力 | 该张力在上一变更引入 counts 时就已经存在，不是本变更新增 | 检视已记录；若要收口需要改主规范措辞，属独立变更 |

## Risks

| 风险 | 缓解 |
| --- | --- |
| `router-stub` 的修复第二次失败（上一轮已失败一次） | 先复现再动刀；WP4 的报告必须给出复现证据与机制说明；若确实修不动，把两条场景登记为未覆盖项并在检视时确认，**不允许**弱化断言来"变绿" |
| WP2 的动作取值改名后 e2e 变红，而 WP4 尚未合入 | 已在 `## Merge Order` 登记该窗口；窗口内只跑 PV2、不引用 e2e 结论 |
| 目录页与详情页同时挂 `CreateSessionSheet` | D5 的单一来源方案 + WP4 的"同一时刻只有一个弹层"断言 |
| 卡片新增两处信息在窄屏挤掉既有内容 | 沿用断点 720 与既有"窄/宽信息集合相同"的断言；新增元素纳入该集合比对 |
| 隔离工作区没有 `node_modules`，WP1–WP3 的 patch 合入前无法自测 | 改动保持机械、小步；主检出每次合入后立即跑 PV2，红了就回退重派 |
| 文档把"无权限"写成"隐藏入口即可"，与 spec 冲突 | WP5 的检视行把这一条列为必查 |
