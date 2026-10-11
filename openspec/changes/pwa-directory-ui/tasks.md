# Tasks

## 1. Dependency and Resource Setup

- [x] 1.1 核实目标与冻结面：`git rev-parse refs/heads/main` 与 `openspec/changes/pwa-directory-ui/` 就位；判据是 `git status --porcelain -- schemas fixtures compatibility` 输出为空（本变更不改任何合同资产），且 `plan.md` 的写范围里没有这三个目录
- [x] 1.2 核实主检出工作区相对干净再派发：判据是派发前把 `git status --porcelain` 的原文记进交接，在途改动要么先提交、要么在交接里明确登记为本次基线的一部分（隔离区是整个工作目录的副本，见 `docs/WORKFLOW.md` §4.3）
- [x] 1.3 确认主检出具备跑 PV2/PV3 的条件：判据是未改动的基线上 `npm run check`（`clients/app`）= typecheck + vitest（458 条）+ `expo export` 全过，且 `node e2e/run-e2e.mjs` 输出 `21/21 passed`（找不到 Chromium 时记录 `BROWSER_PATH` 的值）

## 2. Implementation

- [x] 2.1 [wp:WP1] 建 `src/components/theme.ts`：浅/深两套调色板 + 间距/字号/圆角，取值逐项取自 `prototypes/acp-remote-pwa.html:22-41`，配 `useTheme()`（随系统主题）；判据是新增 `theme.test.ts` 断言两套键集合完全相同、每键非空，且 `npm run check`（`clients/app`）全绿
- [x] 2.2 [wp:WP1] 新增三个展示原语（会话行、分段控件、目录卡片骨架）并登记 `src/components/index.ts`；判据是它们只 import `domain`/`features` 的视图模型（不 import `sync-client`/`protocol`/平台层），`layers.test.ts` 通过
- [x] 2.3 [wp:WP2] 把 `EmptyState`/`StatusPill`/`ConnectionBadge`/`HostConnectionPanel`/`CreateSessionSheet`/`RuntimeUnavailable` 迁到 RNW 原语 + `StyleSheet`，`data-*` 一律改经 `dataSet`；判据是 `r19-host-panel.test.ts`（含 `<li data-agent-row=` 那条断言）与 `r16-create-session.test.ts` 更新后通过
- [x] 2.4 [wp:WP2] 消除 `View` 内的裸字符串并保持属性完整：判据是对这 6 个组件做一次裸字符串扫描无命中，且交付报告里每个既有 `data-*` 属性名与取值都有迁移前/后对照表
- [x] 2.5 [wp:WP3] 在 `src/features/directory-model.ts` 实现四个纯函数（查询匹配、状态筛选、按月分档、会话行投影）；判据是新增 `directory-filter.test.ts` 覆盖：展示名/别名/会话标题三种命中、大小写与空白、三种筛选取值、跨月分档且按**会话自身**时间、空标题呈现为「未命名会话」
- [x] 2.6 [wp:WP3] 在 `ClientState` 增加目录页查询词、目录页筛选、详情页查询词、详情页月份四个字段与 setter（形状同 `createSheetAlias`）；判据是 setter 只改这四个字段，`r17-r19-store-contract.test.ts`、`r12-connection-transitions.test.ts`、`r13-command-idempotency.test.ts` 仍全绿
- [x] 2.7 [wp:WP3] 在 `src/features/testing/fixtures.ts` 补跨月会话夹具（该文件由本包独占）；判据是 2.5 的用例能构造出两个不同的月份档，且该文件未被 WP4/WP5 修改
- [x] 2.8 [wp:WP4] 目录页（`DirectoryList.tsx` + `app/index.tsx`）改用原语渲染并接上搜索、筛选、内联会话行、查看全部、折叠、同步时间标注、无权限提示条；判据是 `r15-directory-list.test.ts` 通过，且报告给出 `data-directory-alias`/`data-empty-state`/`data-content-origin` 等属性的逐项未变对照
- [x] 2.9 [wp:WP4] 为「搜索无结果」与「超过上限时查看全部」补渲染断言；判据是这两条断言在实现缺失时会失败（报告给出该判断的依据），且「搜索无结果」不占用 `data-empty-state`
- [x] 2.10 [wp:WP4] 更新 `docs/FRONTEND_DESIGN.md` §4.1：补状态筛选段与同步时间标注的呈现口径（搜索与响应式 §4.1 已要求，按月分档 §9 已要求，不重复）；判据是 `npm run check`（仓库根）的 `check:docs` 与 `check:openspec` 全绿
- [x] 2.11 [wp:WP5] 目录详情页（`DirectoryDetail.tsx` + `app/dir/[alias].tsx`）改用原语渲染并接上目录内搜索、月份分段、「当前显示 N / M」、副作用提示；判据是新增 `r15-directory-detail.test.ts` 覆盖「无会话」「别名失效」「搜索无结果」三种呈现互不冒充且 N/M 口径正确
- [x] 2.12 [wp:WP5] 用 `useWindowDimensions()` 做窄屏单列/宽屏布局，且两种宽度渲染的信息集合相同；判据是渲染测试在两种宽度下产出相同的 `data-*` 集合（差异只在样式），且创建入口与会话行在窄屏下仍存在
- [x] 2.13 [wp:WP6] 把 3 处标签型选择器（`li[data-directory-alias]`、`li[data-session-id]`、`li[data-degraded-entry]`）去标签化；判据是该 patch **单独**应用在原基线上时 `node e2e/run-e2e.mjs` 仍 `21/21 passed`（证明去标签化对迁移前后两种 DOM 都成立）
- [x] 2.14 [wp:WP6] 补新增行为用例：搜索改变列表集合、筛选生效、两个月份档正确、目录内搜索不影响目录页查询词、窄视口下信息集合不变、搜索无结果不占用 `data-empty-state`；判据是这些用例逐条接到 `e2e/page/index.tsx` 后 `node e2e/run-e2e.mjs` 全绿，且报告逐条写明每条断言的可观察量

## 3. Branch Validation

- [x] 3.1 [wp:WP1] 作者销毁后新建独立 reviewer 检视 WP1 的固定 patch（输入是 patch 绝对路径 + `agent://<id>` + `history://<id>`）；判据是结构化结论 pass，无 CRITICAL/MAJOR 未闭环项
- [x] 3.2 [wp:WP2] 新建独立 reviewer 检视 WP2：`data-*` 是否逐一保留、`View` 内是否还有裸字符串、`<li data-agent-row=` 断言是否同步更新；判据是结构化结论 pass
- [x] 3.3 [wp:WP3] 新建独立 reviewer 检视 WP3：纯函数无副作用、分档按会话自身时间、空态三层互不冒充、新状态字段未触碰连接与命令语义；判据是结构化结论 pass
- [x] 3.4 [wp:WP4] 新建独立 reviewer 检视 WP4：R15 的既有 MUST NOT 仍成立、无 `data-*` 被改名或删除、搜索与筛选零命令派发、折叠不影响其它目录项、文档改动只落在 §4.1 的呈现口径；判据是结构化结论 pass
- [x] 3.5 [wp:WP5] 新建独立 reviewer 检视 WP5：三种「空」互不冒充、N/M 口径、副作用提示文案、与 WP4 共用的 `r15-directory-list.test.ts` 未丢断言；判据是结构化结论 pass
- [x] 3.6 [wp:WP6] 新建独立 reviewer 检视 WP6：迁移后的 `[data-directory-alias]` 是否仍只命中一处、新增用例是否有判别力、既有 21 条的断言强度未被削弱；判据是结构化结论 pass

## 4. Merge and Gate

- [x] 4.1 按 `plan.md` 的 Merge Order 串行合入 WP1→WP2→WP3→WP4→WP5→WP6（WP5 与 WP4 在 `r15-directory-list.test.ts` 上重叠，必须后合入）；判据是每个 patch 先 `git apply --check` 通过再 `git apply`，且每次合入后立即跑 PV2 全绿
- [x] 4.2 跑 PV3（`clients/app` 下 `node e2e/run-e2e.mjs`）；判据是全部 PASS，且含既有 21 条与本次新增用例；此步在 WP6 合入后执行
- [x] 4.3 跑 PV1（仓库根 `npm run check`）；判据是十道合同门禁全绿

## 5. Independent Validation

- [x] 5.1 新建非作者 validator 检视固定版本，逐条核对 `specs/pwa-web-client/spec.md` 的 7 条新增 Requirement 是否都有实现与可读证据；判据是每条 Requirement 至少一个 Scenario 能指向具体用例或 e2e 断言，并给出结构化结论
- [x] 5.2 复算中间态：确认 WP4 合入后到 WP6 合入前的 e2e 红态只出现在该窗口、且未被任何结论引用；判据是交接记录里该窗口的 PV 结果只写了 PV2

## 6. Final E2E

- [x] 6.1 主检出跑 Main E2E 全量（`node e2e/run-e2e.mjs`，必要时用 `BROWSER_PATH`）；判据是零失败、零页面错误（`__pageErrors` 为空），且覆盖目录页搜索/筛选/无结果、详情页分档与 N/M、窄视口信息集合、目录内搜索隔离
- [x] 6.2 用 browser prelude 打开导出的页面做一次辅助目视（截图仅作辅助证据）；判据是截图存在且两页的版式与 `prototypes/acp-remote-pwa.html` 的关键结构一致——**这只作辅助证据，通过判据仍是 6.1 的断言**

## 7. Final Verification

- [x] 7.1 逐条对照 `proposal.md` 的成功判据与 `## What Changes` 的六节，确认每一项都有产物或证据；判据是产出汇总里每节都能指向具体文件、用例名或命令输出

## Workflow follow-up

- 远端落地按 `docs/WORKFLOW.md` §6.2：为这一批 patch 开分支、推远端、`gh pr create --fill`、等六个必需检查（含 `前端检查（clients/app）`）全绿后 `gh pr merge --squash`；PR 标题须符合 `commitlint.config.mjs`。
- 合入后 `openspec archive pwa-directory-ui`，把 7 条 Requirement 合并进 `openspec/specs/pwa-web-client/spec.md`。
- 本次未覆盖、需要后续变更处理的（仅作记录，不在本变更范围）：对话页/配对页/`app/hosts.tsx` 的样式迁移、顶栏与开发模式横幅、PWA manifest/service worker/图标、主题开关（`FRONTEND_DESIGN.md` §11 的暂缓决策）、把 `e2e/run-e2e.mjs` 接进 CI（需先定 Chromium 供给方式）。
