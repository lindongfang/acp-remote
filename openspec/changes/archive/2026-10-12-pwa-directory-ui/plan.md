<!-- 执行安排：把 specs 的行为要求与 design 的 D1–D10 转成可派发的并行工作包。
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
- 验证：隔离工作区内**没有** `clients/app/node_modules`，子 Agent 不得尝试 `npm run check`、
  `vitest`、`tsc`、`expo export` 或 `e2e/run-e2e.mjs`；这些一律只在主检出执行。
- 检视：每个工作包由一个**独立** spawn（不与作者同实例）读 patch、`agent://<id>` 与 `history://<id>`
  后给出结构化结论。
- 派发前主检出工作区应相对干净：隔离区的基线是**整个工作目录的副本**（`docs/WORKFLOW.md` §4.3）。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | design D1/D9 的基座：token 模块（浅/深两套调色板取自 `prototypes/acp-remote-pwa.html:22-41`）+ `useTheme()` + 三个新展示原语（会话行、分段控件、目录卡片骨架） | none | wp-coder | `clients/app/src/components/theme.ts`, `clients/app/src/components/SessionRow.tsx`, `clients/app/src/components/SegmentControl.tsx`, `clients/app/src/components/DirectoryCard.tsx`, `clients/app/src/components/index.ts`, `clients/app/src/components/theme.test.ts` | patch：新增 token、hook 与三个原语并登记 barrel；报告列出 token 键清单与两套调色板来源行号 | 主检出 PV2 |
| WP2 | design D1/D2：把 6 个既有共享组件迁到 RNW 原语 + `StyleSheet`，`data-*` 一律改经 `dataSet` 保留 | code:WP1 | wp-coder | `clients/app/src/components/EmptyState.tsx`, `.../StatusPill.tsx`, `.../ConnectionBadge.tsx`, `.../HostConnectionPanel.tsx`, `.../CreateSessionSheet.tsx`, `.../RuntimeUnavailable.tsx`, `.../r19-host-panel.test.ts`, `.../r16-create-session.test.ts` | patch：6 个组件迁移 + 两处测试随标签变化更新；报告给出每个组件迁移前后的属性清单对照 | 主检出 PV2 |
| WP3 | specs「目录页提供搜索与状态筛选」「目录详情按月分档」「目录详情的目录内搜索」「断连时标注上次同步时间」的**判定**部分（design D3/D4/D5/D6） | none | wp-coder | `clients/app/src/features/directory-model.ts`, `.../client-store.ts`, `.../index.ts`, `.../r15-directory-model.test.ts`, `.../testing/fixtures.ts`, 新增 `.../directory-filter.test.ts` | patch：纯函数（匹配、筛选、按月分档、会话行投影）+ 四个 UI 状态字段与 setter + 单测；报告列出每个纯函数的签名与覆盖的场景 | 主检出 PV2 |
| WP4 | specs「目录页提供搜索与状态筛选」「目录页以内联会话行呈现目录内容」「断连时标注上次同步时间」「按设备宽度自适应」「创建入口的副作用与权限说明」在**目录页**的落点（design D5/D7/D8）；并把新增的筛选段与同步时间标注口径写进 `docs/FRONTEND_DESIGN.md` §4.1 | code:WP1, code:WP2, code:WP3 | wp-coder | `clients/app/src/components/DirectoryList.tsx`, `clients/app/app/index.tsx`, `clients/app/src/components/r15-directory-list.test.ts`, `docs/FRONTEND_DESIGN.md`, `clients/app/src/features/directory-model.ts`, `clients/app/src/features/client-store.ts`, `clients/app/src/features/index.ts` | patch：目录页改用原语渲染，接上搜索/筛选/内联行/折叠/同步时间/权限条；报告给出与既有 `data-*` 的逐项对照（证明无一处被改名或删除） | 主检出 PV2 |
| WP5 | specs「目录详情按月分档」「目录详情的目录内搜索」「断连时标注上次同步时间」「按设备宽度自适应」「创建入口的副作用与权限说明」在**目录详情页**的落点（design D5/D6/D7/D8） | code:WP1, code:WP2, code:WP3 | wp-coder | `clients/app/src/components/DirectoryDetail.tsx`, `clients/app/app/dir/[alias].tsx`, 新增 `clients/app/src/components/r15-directory-detail.test.ts`, `clients/app/src/components/r15-directory-list.test.ts`（仅在与 WP4 冲突处） | patch：详情页改用原语渲染，接上目录内搜索/月份分段/「当前显示 N / M」/副作用提示；报告给出三种"空"呈现的文案与标记 | 主检出 PV2 |
| WP6 | 浏览器用例：把 3 处标签型选择器去标签化（`li[data-directory-alias]`、`li[data-session-id]`、`li[data-degraded-entry]`），并为本次新增行为补断言（design D10） | code:WP4, code:WP5 | wp-tester | `clients/app/e2e/page/checks/r15-directory.ts`, 新增 `clients/app/e2e/page/checks/r15-directory-detail.ts`, `clients/app/e2e/page/index.tsx`, `clients/app/e2e/page/fixtures.tsx` | patch：选择器迁移 + 新增"输入查询词后列表真的变了""筛选生效""两个月份档正确""目录内搜索不影响目录页查询词""窄屏信息不缺"用例；报告给出新增用例清单与各自的判别力说明 | 主检出 PV3 |

**跨包规则（避免写范围隐性重叠）：**

- `clients/app/src/features/testing/fixtures.ts` 由 **WP3 独占**。WP3 必须交付覆盖"两个不同月份"的会话夹具；
  WP4/WP5 需要额外数据时在自己的测试文件里构造，**不得**修改该文件。
- `clients/app/src/components/r15-directory-list.test.ts` 同时被 WP4 与 WP5 触及：WP4 先（它拥有目录页断言），
  WP5 后（详情页断言需要拆到新文件 `r15-directory-detail.test.ts`），按 `## Merge Order` 串行。
- `clients/app/e2e/**` 由 **WP6 独占**：WP4/WP5 不得改 e2e，因此 WP4/WP5 的 Verify 只到 PV2。
- 组件测试文件是 `*.test.ts`（`vitest.config.ts` 的 `include` 只收 `.test.ts`），用 `createElement` +
  `renderToStaticMarkup`；新增测试必须遵守这条，不得引入 JSX 形式的 `.test.tsx`。
- **实施中的再切分（2026-10-11 补登记）**：WP4 在实施时发现目录页的内联会话行在 feature 层没有数据源
  ——`DirectoryPageModel` 只带三项汇总，而 design D3 与 WP3 的交付项都写了「会话行投影」由 feature 层承担，
  WP3 只落了详情页那一支。因此把 `buildSessionRowsByAlias`（按目录分组、组内倒序、不经详情页查询/月份）
  与只读选择器 `directoryPageSessions()` 补落到 **WP4** 的写范围（WP3 已合入，无在途重叠）。
  不采用「复用 `directoryDetail(alias).sessions`」的方案：它会被详情页的查询词/月份过滤，
  导致目录页的内联行条数与汇总徽标不一致（徽标说 3 条、实际只列 1 条）。
- 页内输入控件（搜索框）用 WP1 的 `theme.ts` 追加导出的 `TextInput`（`TextInputProps` 继承 `ViewProps`，
  `dataSet` 已可用）；**不得**在组件里另写一份 `@ts-expect-error` 绑定，否则会出现第二套原语机制。

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 写范围无重叠（全为新建文件 + barrel） | PV2 |
| 2 | WP2 | 写范围无重叠（6 个既有组件与它们的两个测试文件） | PV2 |
| 3 | WP3 | 写范围无重叠（feature 层 + 夹具） | PV2 |
| 4 | WP4 | 写范围无重叠（目录页组件与页面） | PV2 |
| 5 | WP5 | 与 WP4 在 `r15-directory-list.test.ts` 上重叠 → 必须晚于 WP4，且需在 WP4 已改过的工作区上重跑 | PV2 |
| 6 | WP6 | 与 WP4/WP5 无文件重叠，但它断言的是 WP4/WP5 产出的 DOM；必须最后合入 | PV3 |

**关于中间状态（必须如实登记）：**

1. **e2e 的红窗口**：WP4 合入之后、WP6 合入之前，`e2e/run-e2e.mjs` 会因为
选择器仍是标签型（`li[...]`）而失败——这是本次"标签名变化"的已知后果，不是回归。
因此该窗口内**不得**对 e2e 下任何结论：WP4/WP5 的 Verify 只到 PV2，e2e 的结论只在 WP6 合入后由 PV3 给出。
若需要消除这个窗口，可在 WP6 里先只做选择器去标签化（`li[…]` → `[…]` 对迁移前后两种 DOM 都成立）、
再补新断言，但两者仍必须在同一个 patch 内交付。

2. **typecheck 的红窗口（实施中发现，2026-10-11 补登记）**：WP3 给 `DirectoryPageModel` 增加了
`no_match` 分支，而消费该联合的 `DirectoryList.tsx` 属 WP4。因此在 **WP3 合入之后、WP4 合入之前**，
`npm run check`（`clients/app`）会以 `TS2339` 失败——这是"给判别联合加分支、消费者后到"的固有顺序，
不是回归。处置：WP4 与 WP5 与 WP2 同批派发（三者都只依赖已合入的 WP1/WP3），使该窗口尽量短；
窗口内只记录、不对 PV2 下"全绿"的结论。若 WP4 失败导致窗口无法关闭，则回退 WP3 重新拆分。

## Integration Fixes

<!-- 合入期由主 Agent 直接在主检出做的最小修复：不属于任何工作包的 patch，
     但必须可追溯（改动内容、原因、谁验证）。检视者应把它们纳入对应用例的评估。 -->

| # | 文件 | 改动 | 原因 | 证据 |
| --- | --- | --- | --- | --- |
| IF1 | `clients/app/src/components/SegmentControl.tsx` | props 与组件改为泛型 `<T extends string = string>` | WP4 的 patch 把收窄过的 `onFilterChange: (filter: DirectoryFilter) => void` 直接接给 `onChange: (value: string) => void`，主检出 `tsc` 报 TS2322（WP4 在隔离区无法跑 tsc，未能自检） | `npm run check`（`clients/app`）= 40 文件 / 511 用例 / tsc 干净 / expo export 成功 |
| IF2 | `clients/app/src/components/theme.ts` | 追加 `TextInput` 与 `TextInputProps` 导出 | WP4/WP5 都需要可输入控件；若不收敛到 theme.ts，两个包各写一份 `@ts-expect-error` 绑定，等于第二套原语机制 | `tsc --noEmit` 干净；WP5 按该导出写 import |
| IF3 | `clients/app/src/components/r15-directory-detail.test.ts` | 把「查询词命中别的目录时不呈现任何会话」的负向断言从 `not.toContain("<查询词>")` 改为按**会话标识**断言 | 原断言与实现自相矛盾：搜索框是受控的，会把查询词回显进 `<input>` 的 value，因此该字符串必然出现在 HTML 里（同文件的对照用例也依赖这处回显）。改后仍能证伪「渲染了别的目录的会话」 | 定向前后均以 `vitest run src/components/r15-directory-detail.test.ts` 观测；整体 `npm run check` 绿 |

## Review Outcomes

<!-- 每个工作包一次独立 spawn 的检视结论与处置。检视输入是 patch 绝对路径 + agent://<id> + history://<id>。 -->

| 工作包 | Reviewer | verdict | Findings | 处置 |
| --- | --- | --- | --- | --- |
| WP1 | ReviewWp1 | changes-requested | 2 条（theme.ts 缺 `TextInput` 导出、`SegmentControlProps` 未泛型化）——两条都是**patch 快照早于主 Agent 的整合修复**造成的，检视自己也写明「以主检出工作区为准，两处均已闭环」 | 由 IF2 / IF1 闭环，无需新 spawn；已在合并后的主检出复跑 `npm run check` |
| WP2 | ReviewWp2 | correct | 0 | 无需处置 |
| WP3 | ReviewWp3 | incorrect | 1 条 P2：月份档选中值与派生档集合不对账（可选档消失后出现空白列表 + 无选中控件 + 误导性「没有符合条件的会话」；跨目录进入时查询词/月份还会残留） | 派新 spawn `FixWp3MonthReconcile` 修复：模型侧对账（不可表示的月份回落 `all` 并在 `ready.month` 暴露生效值）+ 进入目录时复位详情页查询词与月份；合入后 PV2 518 用例全绿 |
| WP4 | ReviewWp4 | correct | 0（另有两处「非缺陷观察」：`index.tsx` 中不可观测的 `loading` 子句、以及 `r15-directory-list.test.ts` 的权限用例覆盖范围） | 无需处置 |
| WP5 | ReviewWp5 | correct | 0，并确认 IF3 的修法正确且未削弱判别力 | 无需处置 |
| WP6 | （见 tasks.md 的分支验证组） | 待跑 | — | — |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `npm run check`（仓库根） | 十道合同门禁全绿。本次不改 `schemas/`/`fixtures/`/`compatibility/`，但改了 `docs/FRONTEND_DESIGN.md` 与 `openspec/changes/pwa-directory-ui/**`，因此 `check:docs`（链接）与 `check:openspec`（`validate --all --strict`）必须绿 |
| PV2 | `npm run check`（`clients/app`） | `tsc --noEmit` 通过；`vitest run` 全过（含 WP1/WP3 新增与 WP2/WP4/WP5 更新的用例）；`expo export --platform web` 成功产出 `dist`（证明样式层没打断打包） |
| PV3 | `node e2e/run-e2e.mjs`（`clients/app`；需要本机 Chromium 内核，或用 `BROWSER_PATH` 指定） | 全部检查 PASS，且**至少**包含：原有 21 条（选择器迁移后仍全绿）+ 本次新增的搜索、筛选、按月分档、目录内搜索隔离、窄屏信息完整、搜索无结果不占用 `data-empty-state` 等用例。这是本变更的 Main E2E 入口 |

`npm run check`（仓库根）与 `npm run check:rust` **不覆盖** `clients/app`：前端是独立 npm 工程
（`.github/workflows/ci.yml` 的 `frontend` job 才跑它）。因此 PV2 必须单独执行，不能以 PV1 代替。

### 实测记录（2026-10-11，全部在主检出）

| Check | 结果 |
| --- | --- |
| 基线（未改动） | `npm run check`（`clients/app`）绿（458 用例）；`node e2e/run-e2e.mjs` → `21/21 passed` |
| PV1 | `npm run check`（仓库根）十道门禁全绿（含 `check:docs` 384 链接、`check:openspec` 的 `--strict` 校验） |
| PV2 | 每次合入后执行；最终 `clients/app` = typecheck 干净 + `40 文件 / 518 用例` 全过 + `expo export --platform web` 产出 `dist` |
| PV3 | `node e2e/run-e2e.mjs` → **`32/32 passed`**（既有 21 条 + 新增 11 条：R15-8..R15-18） |
| 辅助目视 | 用 browser prelude 打开由真实页面 + 真实组合根 + 假主机打成的页面（临时探针，已删除）：目录页与目录详情页各一张截图，两张分别落在浅/深两套调色板上；断言到 `rows=2 / inlines=6 / note=true / synced=true`、`alias=alpha / shown=6 / total=6 / sideEffect=true`、`__pageErrors` 为空。**说明**：`expo export` 出来的静态产物连不上任何主机（`server::sync` 未落地），因此目视走的是与 e2e 同一条“真实页面 + 假主机”路径，而不是直接开 `dist` |
| 未覆盖（如实登记） | ①「点击『查看全部』进入详情页」与「返回目录页后查询词仍在」两个 spec 场景**没有浏览器用例**：`e2e/page/router-stub.tsx` 的 `Stack` 直接调用 `current.component()`，跨路由切换组件类型会让 hooks 槽位错配并抛错（既有缺陷，非本次引入；WP6 实测尝试修 `createElement(current.component)` 后详情路由停在 `RuntimeUnavailable`，故未改）。② 目录页的「无创建权限提示条」只有组件级用例与检视证据，没有浏览器用例（e2e 夹具默认带 `session.create`）。③ `composition-root.ts:134` 与 `r18-degradation.ts:178/354` 三处仍是标签型选择器（本次未在其写范围内），它们因 RNW 把 `role="listitem"` 渲染成 `li` 而仍然命中 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | token 的浅/深两套是否都取自原型且键集合一致；`useTheme()` 是否随系统主题切换；新原语是否只吃视图模型、没有引入业务判定（R10 分层） | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 6 个组件的 `data-*` 属性名与取值是否逐一保留（改经 `dataSet`）；`View` 内是否出现裸字符串（原生端会炸）；`r19-host-panel.test.ts` 的 `<li data-agent-row=` 断言是否同步更新 | 无 CRITICAL/MAJOR 未闭环项 |
| WP3 | reviewer | 匹配/筛选/分档是否为纯函数且无副作用；分档是否按**会话自身**时间（不是当前时间）；空态三层是否互不冒充；四个 UI 状态字段是否只影响渲染、未触碰连接/命令语义 | 无 CRITICAL/MAJOR 未闭环项 |
| WP4 | reviewer | R15 的既有 MUST NOT 是否仍然成立（展示名+别名、同名只以别名消歧、渲染结果不含路径片段）；是否有任何 `data-*` 被改名/删除；搜索/筛选是否零命令派发；折叠是否不影响其它目录项 | 无 CRITICAL/MAJOR 未闭环项 |
| WP5 | reviewer | 三种"空"（无会话 / 别名失效 / 搜索无结果）是否互不冒充；「当前显示 N / M」的分子分母口径；副作用提示文案；与 WP4 的 `r15-directory-list.test.ts` 合并是否丢了断言 | 无 CRITICAL/MAJOR 未闭环项 |
| WP6 | reviewer | 迁移后的选择器是否仍然只命中一处（例如 `[data-directory-alias]` 不得同时命中目录项与其内部元素）；新增用例是否有判别力（能红）；既有 21 条的断言强度是否被削弱 | 无 CRITICAL/MAJOR 未闭环项 |

## Main E2E

```yaml
mode: required
```

入口是既有的 `clients/app/e2e/run-e2e.mjs`（真实 Chromium 内核 + CDP，挂载**真实** `app/_layout.tsx`
组合根与真实路由组件；快照经真实 SHA-256 两级摘要校验）。本次要断言的可观察行为：

- 目录页：输入查询词后列表项集合发生变化；切换「运行中」/「待处理」后只剩相应目录；无结果时呈现
  「没有匹配的结果」且不占用 `data-empty-state`；目录卡片可折叠；同步时间标注在断连时出现。
- 目录详情页：两个月份档各自只含本月会话；「当前显示 N / M」随筛选变化；目录内搜索不影响目录页查询词。
- 响应式：在窄视口下重挂一次，会话行、三项汇总与创建入口仍然存在（信息集合与宽视口一致）。
- 既有 21 条用例全绿（选择器迁移后）。

只启动成功、退出码为零、截图都不算通过；截图仅作辅助证据。

## Risks

| 风险 | 缓解 |
| --- | --- |
| WP4 合入后到 WP6 合入前，主检出的 e2e 是红的 | 已在 `## Merge Order` 登记；该窗口内只跑 PV2、不对 e2e 下结论；WP6 必须紧随 WP4/WP5 合入 |
| 隔离工作区没有 `clients/app/node_modules`，WP1–WP5 的 patch 在合入前无法自测 | 每个 WP 的改动保持机械、小步；主检出在**每次**合入后立即跑 PV2，红了就回退重派；WP6 的用例判别力由 review 预检 |
| 标签迁移同时动了属性或文案，削弱既有负向断言的判别力 | WP4 的 Deliverable 明确要求给出 `data-*` 逐项对照；review 行把它列为必查项；PV3 保留原有 21 条 |
| `li` → `div` 后 `[data-directory-alias]` 可能命中多个元素，导致既有计数断言失真 | WP6 的 review 行列为必查项；WP4 必须保证该属性只出现在每个目录项的一个元素上 |
| 窄屏/宽屏渲染出不同的信息集合（悄悄少显示） | spec 写明 MUST NOT 隐藏；PV3 增加窄视口用例做集合对比 |
| 文档改动落在 `docs/FRONTEND_DESIGN.md` 时与 §9 的映射不一致 | 只改 §4.1 的呈现口径，不动阶段、状态模型与平台边界；PV1 的 `check:docs` 会挡住坏链接 |
