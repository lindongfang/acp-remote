# Tasks

## 1. Dependency and Resource Setup

- [x] 1.1 核实起点：`git rev-parse refs/heads/main` 与归档后的 `openspec/changes/archive/2026-10-12-pwa-directory-ui/` 就位；判据是 `clients/app` 的 `npm run check` 与 `node e2e/run-e2e.mjs` 在未改动的基线上全绿（基线应为 40 文件 / 518 用例 / 32 条浏览器用例）
- [x] 1.2 记录派发前的主检出状态：判据是把 `git status --porcelain` 原文记进交接；在途改动要么先提交、要么登记为基线的一部分（隔离区是整个工作目录的副本）
- [x] 1.3 复现死路作为本变更的起点证据：判据是在主检出用一个只读探针（或直接读 `src/domain/directory.ts` 的 `hasAnySession` 分支 + `DirectoryList.tsx` 的 `onOpenAll`）证明"零会话时目录列表不渲染、因此没有任何入口"，并把证据写进交接

## 2. Implementation

- [x] 2.1 [wp:WP1] `DirectoryPageModel.empty` 带上 `directories`（`no_directories` 为 `[]`；零会话时为各目录的零汇总行），且 `ready`/`empty` 的判定与 `data-empty-state` 语义不变；判据是 `r15-directory-model.test.ts` 新增用例断言"全部目录零会话时 `empty.directories` 非空且每项 `counts` 全零"，以及 `no_directories` 时为空数组
- [x] 2.2 [wp:WP1] `DirectoryRowModel` 增加最近活动时间（该目录下会话 `updatedAt` 的最大值，无会话为 `null`），实现不经过 `Date`/本地时区；判据是新增用例断言"取最大值"与一个反例（把第一条或当前时间当结果即红），且既有 R15 字段约束未被新字段破坏（仍无路径类槽位）
- [x] 2.3 [wp:WP2] `DirectoryCard` 的标题成为进入详情页的入口（新动作取值）且「查看全部 N 个」改用新动作取值；判据是 `r15-directory-list.test.ts` 通过且断言两个入口各自带可区分的动作属性、`[data-directory-alias]` 仍每项恰好一处
- [x] 2.4 [wp:WP2] `DirectoryCard` 呈现最近活动时间（无会话时不呈现）与目录级创建入口（回调缺省时不呈现）；判据是组件用例覆盖"有时间/无时间""有回调/无回调"四种组合
- [x] 2.5 [wp:WP2] `DirectoryList` 在 `empty` 分支下同时呈现空态与目录列表（有目录时），并接线 `onCreateSession`；判据是用例断言"`data-empty-state` 存在且目录项存在"以及"`no_directories` 时目录项不存在"
- [x] 2.6 [wp:WP2] `EmptyState` 的标题/指引与 `CreateSessionSheet` 的标题补显式 `data-*` 锚点（只加属性，不改文案与 props 契约）；判据是组件用例改为按锚点取值，且两段空态文案与既有 props 契约逐字未变
- [x] 2.7 [wp:WP3] 目录页挂载 `CreateSessionSheet` 并把打开状态收敛为 `store.createSheetAlias === alias`；详情页删掉本地 `sheetOpen` 改用同一来源；判据是两页都能打开/关闭弹层、同一时刻只有一个弹层，且 `git grep sheetOpen` 不再出现第二个独立来源
- [x] 2.8 [wp:WP3] 目录页的创建入口与详情页使用同一条命令与同一份权限判定（`createEntry(alias)` + `submitCreateSession`），无权限时卡片入口不呈现且页面顶部有说明；判据是组件/模型用例断言无权限时不渲染该入口、有权限时载荷仍只有两个引用（`assertCreateSessionRefs` 或等价断言）
- [x] 2.9 [wp:WP4] 修 `router-stub.tsx` 的跨路由渲染（按 path 作 key 渲染路由组件）；判据是报告给出**修复前的复现证据**（跨路由切换时抛错/整棵树卸载）与修复后机制说明，并在 PV3 里跑通"点入口进入详情页"
- [x] 2.10 [wp:WP4] 新增浏览器用例：跨路由进入详情页、返回目录页后查询词仍在、目录页无权限提示条、零会话（全部为零与部分为零两种）可进入详情页、卡片级创建载荷为两引用；判据是这些用例逐条接到 `e2e/page/index.tsx` 后 `node e2e/run-e2e.mjs` 全绿，且报告逐条写明断言的可观察量与判别力
- [x] 2.11 [wp:WP4] 把剩余标签型选择器去标签化（`li[…]`×3、`pre[…]`）并按 WP2 的新锚点替换 `h2`/`p` 查询，同时更新被 D3 改名的既有断言（R15-10/R15-11 等）；判据是迁移后既有 32 条仍全绿，且 R15-1 的计数、R15-2 的双重消歧、R15-4 的空态对比、R18 的负向断言强度未被削弱
- [x] 2.12 [wp:WP5] `docs/FRONTEND_DESIGN.md` §4.1 补四项呈现口径（空态仍列目录、卡片标题入口、最近活动时间、目录级创建入口与"无权限说明由页面承担"）；判据是改动只落在 §4.1，且措辞与 `specs/pwa-web-client/spec.md` 的四条 Requirement 一致
- [x] 2.13 [wp:WP5] `prototypes/IMPLEMENTATION-GAPS.md` 的对照表同步这两项与死路；判据是 `npm run check`（仓库根）的 `check:docs` 与 `check:openspec` 全绿

## 3. Branch Validation

- [x] 3.1 [wp:WP1] 新建独立 reviewer 检视 WP1 固定 patch；重点：`empty.directories` 口径、最近活动时间的最大值与无时区依赖、新字段未引入路径类槽位；判据是结构化结论 pass
- [x] 3.2 [wp:WP2] 新建独立 reviewer 检视 WP2；重点：`[data-directory-alias]` 仍每项一处、两个入口动作取值可区分、空态与列表可同时呈现、新锚点只加属性、组件仍是纯函数；判据是结构化结论 pass
- [x] 3.3 [wp:WP3] 新建独立 reviewer 检视 WP3；重点：`createSheetAlias` 是唯一打开来源、两页同一条命令、未改 store 语义；判据是结构化结论 pass
- [x] 3.4 [wp:WP4] 新建独立 reviewer 检视 WP4；重点：替身缺陷的复现证据与 key 的机制说明、新用例判别力、去标签化未削弱断言、有无为变绿而删弱断言；判据是结构化结论 pass
- [x] 3.5 [wp:WP5] 新建独立 reviewer 检视 WP5；重点：只改指定两处、与 spec 措辞一致（"无权限"不得写成"隐藏入口即可"）；判据是结构化结论 pass

## 4. Merge and Gate

- [x] 4.1 按 `plan.md` 的 Merge Order 串行合入 WP1→WP2→WP3→WP4→WP5；判据是每个 patch 先 `git apply --check` 通过再 `git apply`，且每次合入后立即跑 PV2（WP4 前不对 e2e 下结论；WP4 后跑 PV3）
- [x] 4.2 跑 PV3（`clients/app` 下 `node e2e/run-e2e.mjs`）；判据是零失败、零页面错误，且覆盖 plan 的 Main E2E 列出的五类行为
- [x] 4.3 跑 PV1（仓库根 `npm run check`）；判据是十道合同门禁全绿

## 5. Independent Validation

- [x] 5.1 新建非作者 validator 检视固定版本，逐条核对本次新增的 4 条 Requirement 是否都有实现与可读证据；判据是每条 Requirement 至少一个 Scenario 能指向具体用例或浏览器断言，并给出结构化结论
- [x] 5.2 复算死路是否真的关闭：判据是用只读探针或浏览器用例分别验证"全部目录零会话"与"部分目录零会话"两种情形下都能进入某个目录的详情页，并给出证据
- [x] 5.3 确认中间红窗口（**WP2 合入起、WP4 合入止**，见 plan.md 的订正）未被任何结论引用；判据是交接记录里该窗口只写了 PV2。独立验证（`ValidateFollowup`）已按 plan 声明的写范围复算并确认：没有产物把中间状态当结论引用，唯一的 e2e 证据是 WP4 合入后的 39/39

## 6. Final E2E

- [x] 6.1 主检出跑 Main E2E 全量（`node e2e/run-e2e.mjs`）；判据是既有 32 条 + 本次新增全部通过，且 `__pageErrors` 为空
- [x] 6.2 用 browser prelude 对目录页做一次辅助目视（截图仅作辅助证据）；判据是截图存在且能看到"目录卡片含最近活动时间与两个入口"，以及零会话时列表与空态同时呈现——**通过判据仍是 6.1 的断言**

## 7. Final Verification

- [x] 7.1 逐条对照 `proposal.md` 的成功判据与 `## What Changes` 四节，确认每一项都有产物或证据；判据是产出汇总里每节都能指向具体文件、用例名或命令输出

## Workflow follow-up

- 远端落地按 `docs/WORKFLOW.md` §6.2：开分支、推远端、`gh pr create --fill`、等六个必需检查（含 `前端检查（clients/app）`）全绿后 `gh pr merge --squash`；PR 标题须符合 `commitlint.config.mjs`（scope 用 `frontend` 或 `docs`）。**不要**跑 `omp commit`。
- 合入后按 `/opsx-archive` 归档本变更，把 4 条 Requirement 合并进 `openspec/specs/pwa-web-client/spec.md`。
- 本次未覆盖、留给后续变更的（仅记录）：对话页/配对页/`app/hosts.tsx` 的样式与交互、`DiagnosticsDrawer` 的迁移、顶栏与开发模式横幅、PWA manifest/service worker、把 `e2e/run-e2e.mjs` 接进 CI、`server::sync`（前端连真机仍缺它）。
