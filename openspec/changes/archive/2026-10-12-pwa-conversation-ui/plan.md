<!-- 执行安排：把 specs 的行为要求与 design 的 D1–D11 转成可派发的并行工作包。
     plan 只写“谁在哪个隔离工作区做什么、按什么顺序合入、怎么验”，不重复 specs/design 的内容。

     本仓库的执行原语是 Oh My Pi（omp）：
     - 工作包由主 Agent 用 `task` 工具的 batch 形状扇出（共享 `context` + 逐包 `task`/`solutionSpace`）；
     - 每个工作包跑在隔离工作区（`.omp/config.yml` 的 `task.isolation`：`merge: patch`、`apply: false`）；
     - 交付物是 `<id>.patch` + `agent://<id>` + `history://<id>`，不产生 commit、不产生分支；
     - 合入由主 Agent 串行 `git apply --check` → `git apply`，每次合入后跑主检出 Project Verify；
     - 隔离工作区**没有** `target/` 与 `clients/app/node_modules`（`docs/WORKFLOW.md` §4.3），因此
       `cargo` / `tsc` / `vitest` / `expo export` / `e2e/run-e2e.mjs` 一律只在主检出跑。 -->

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出多个工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）。工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；工作包不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`，每次合入后跑主检出 Project Verify。
- 验证：隔离工作区内**没有** `target/` 与 `clients/app/node_modules`；子 Agent 不得尝试
  `npm run check`、`cargo test`、`vitest`、`tsc`、`expo export`、`e2e/run-e2e.mjs`，可以**只读**借用主检出的
  `node_modules` 与 `target`（绝对路径）做解析/编译探针。
- 检视：每个工作包由一个**独立** spawn（不与作者同实例）读 patch、`agent://<id>` 与 `history://<id>` 后给出结论。
- **前置**：起点是 `pwa-directory-ui-followup` 已合入的状态——它占着 `clients/app/e2e/**`、
  `clients/app/app/index.tsx` 与 `e2e/page/router-stub.tsx`。本变更不得与它并行改这些文件。

## 共享词表（由 design 定死，任何工作包都不得改名）

| 位置 | 取值 |
| --- | --- |
| `session.read` 的新 include 取值 | `history` |
| 结果里的条目数组 / 页标记 | `history` / `historyHasEarlier` |
| 请求里的游标字段 | `beforeHistory`（会话内事件序号，十进制字符串） |
| feature ID | `core.session-history.v1`（`delivery: mvp`、`required: false`） |

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | specs `sync-session-history` 的全部 Requirement（条目形状、类型集合、序号游标与页标记、与消息分页互不影响、保留窗口、只在 Sync 面生效）：schema、Rust 镜像、正反 fixture、feature 登记、协议文档表格 | none | wp-coder | `schemas/sync/v1/**`, `fixtures/sync/v1/**`, `crates/sync-protocol/**`, `compatibility/features/v1/features.json`, `docs/SYNC_PROTOCOL.md` | patch：schema + `views.rs`/`sync.rs`/`command.rs` 镜像 + fixture + manifest + feature 登记 + 文档表格；报告逐条给出「schema 与新 defs 的对应」「负例 fixture 为何必须被拒」 | 主检出 PV1 |
| WP2 | design D4：core/storage 的「按会话、带正文、可分页」读路径与端口文档 §5（含 `HistoryInclude` 新开关） | none（形状由 design 定死） | wp-coder | `crates/core/**`, `crates/storage-sqlite/**`, `docs/CORE_PORTS_AND_STORAGE.md` | patch：端口 + 读实现 + fake 读视图 + 用例层 + 单测；报告给出「一次读只查一遍库、不逐条取正文」的证据与 §5 rust 块的逐成员对照 | 主检出 PV2 |
| WP3 | specs `pwa-web-client` 的「呈现覆盖由客户端自持」「时间线同一渲染」「实时尾巴与权威正文」「输入与提交以终态为准」「取消以终态为准」「授权与提问应答的可用边界」「来源离线标注由事件驱动」的**状态与模型层** | code:WP1 | wp-coder | `clients/app/src/protocol/**`, `clients/app/src/sync-client/**`, `clients/app/src/domain/**`, `clients/app/src/features/**` | patch：新资源 DTO（与 schema 一一对应）、覆盖表、时间线模型、输入/授权/设置的视图模型、`session.origin.online_changed` 归并、实时尾巴叠加层；报告给出「已知但未覆盖的事件类型 → 降级卡片」的实测证据与覆盖表条目清单 | 主检出 PV3 |
| WP4 | design D9：两个依赖的可用性探针 + 富文本渲染组件（禁原始 HTML、不加载远端资源、样式走既有 token） | none | wp-coder | `clients/app/package.json`, `clients/app/package-lock.json`, `clients/app/src/components/Markdown.tsx`, `clients/app/src/components/CodeBlock.tsx` | patch：依赖 + 两个新组件 + 组件测试；报告必须给出探针证据（装了哪个版本、`npm ci` 与 `expo export` 是否通过、peer 警告原文），探针不通过则按 design D9 降级并给出对照 | 主检出 PV3 |
| WP5 | specs `pwa-web-client` 的呈现层：时间线与各类卡片、输入区、授权卡、模型/模式、变更文件右栏、宽窄两形态、对话页路由接线 | code:WP3, code:WP4 | wp-coder | `clients/app/src/components/**`（`Markdown.tsx`/`CodeBlock.tsx` 除外）, `clients/app/app/session/[id].tsx` | patch：组件与路由 + 组件级断言；报告给出「窄/宽信息集合相同」的属性对照、每张卡片的 `data-*` 锚点清单、以及右栏**不含**差异正文的证据 | 主检出 PV3 |
| WP6 | specs 的浏览器级覆盖：重开页面仍见历史条目、输入/提交/取消、授权应答边界、模型与模式、右栏统计、来源离线的显式降级、富文本不可信渲染、以及既有用例回归 | code:WP5 | wp-tester | `clients/app/e2e/**` | patch：新用例文件 + 既有用例适配；报告逐条列出新增用例的判别力（实现回退时会红的机制），并给出每条断言对应的 `data-*` 锚点 | 主检出 PV4 |
| WP7 | 呈现与协议口径落到权威文档：`docs/FRONTEND_DESIGN.md`（§4.1 对话页口径、§6 呈现覆盖自持）、`prototypes/IMPLEMENTATION-GAPS.md` 对照表、并在 `docs/NODE_LINK_PROTOCOL.md` 登记「Node Link 面无结构化历史」这一已知缺口 | none | wp-coder | `docs/FRONTEND_DESIGN.md`, `docs/NODE_LINK_PROTOCOL.md`, `prototypes/IMPLEMENTATION-GAPS.md` | patch：只改这三处叙述；报告给出改动前后段落，并说明缺口登记的措辞为何不承诺未实现能力 | 主检出 PV1 |

**跨包规则：**

- 共享词表（上一节）是唯一命名来源；任何包**不得**自行改取值或另造同义名。
- WP1 独占 `schemas/**`、`fixtures/**`、`compatibility/**`；WP2 独占 `crates/**` 与 `docs/CORE_PORTS_AND_STORAGE.md`；WP3 独占 `clients/app/src/{protocol,sync-client,domain,features}/**`；WP4 独占 `clients/app/package.json`、`package-lock.json` 与两个新组件文件；WP5 独占其余 `clients/app/src/components/**` 与 `clients/app/app/**`；WP6 独占 `clients/app/e2e/**`；WP7 独占三份文档。
- 组件测试文件是 `*.test.ts`（`vitest.config.ts` 的 `include` 只收 `.test.ts`），用 `createElement` + `renderToStaticMarkup`，不得引入 JSX 形式的 `.test.tsx`。
- 新增的组件 props **一律可选**（缺省等价于不启用），保证既有调用点继续编译。
- ACP 内容边界：任何包都不得解析 `acp.rawJson` 后重新序列化，也不得据它渲染结构化差异（design D8）。
- 隔离工作区不得跑仓库级构建；只读借用主检出的 `node_modules`/`target` 做探针。

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 合同资产是其它包的对齐基准（`schema_drift` 双向门禁） | PV1 |
| 2 | WP2 | 独立目录（`crates/`）与合同形状耦合，但文件无重叠 | PV1 + PV2 |
| 3 | WP3 | 客户端 DTO/模型依赖 WP1 的 schema | PV3 |
| 4 | WP4 | 依赖是 WP5 的编译前置；`package.json` 独占 | PV3 |
| 5 | WP5 | 呈现层依赖 WP3 的模型与 WP4 的组件 | PV3 |
| 6 | WP6 | 断言的是 WP5 产出的 DOM 与交互，必须最后合入 | PV4 |
| 7 | WP7 | 文档无代码耦合 | PV1 |

**关于中间状态：** WP3 合入后、WP6 合入前，浏览器用例仍只覆盖既有场景，**不**对新增行为下结论；
该窗口内只跑 PV3。WP4 若探针失败并按 design D9 降级，必须在合入前把降级决定写回 `design.md` 的 D9
（同一变更内更新，不新开变更）。

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `npm run check`（仓库根） | 十道合同门禁全绿（含 `check:schemas` 的新 fixture、`check:features` 的新 feature、`check:drift` 的 §5 端口、`check:docs` 与 `check:openspec --strict`） |
| PV2 | `npm run check:rust`（仓库根） | `cargo fmt --check`、`clippy -D warnings` 与 `cargo test --workspace --all-features` 全绿，含 `sync-protocol` 的 `schema_drift`/`view_projections` 与 core/storage 的新读路径用例 |
| PV3 | `npm run check`（`clients/app`） | typecheck 通过；`vitest run` 全过；`expo export --platform web` 成功产出 `dist`（依赖探针与 CSP 不得引入远端资源） |
| PV4 | `node e2e/run-e2e.mjs`（`clients/app`） | 全部 PASS，且至少覆盖 Main E2E 一节列出的可观察行为；既有用例（含目录页与降级用例）在适配后仍全绿 |

仓库根 `npm run check` 与 `check:rust` **不覆盖** `clients/app`（它是独立 npm 工程），因此 PV3/PV4 必须单独执行。

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | schema 与 `views.rs`/`sync.rs`/`command.rs` 双向一致；新条目形状是否**真的复用**事件视图而不是另造一套；负例 fixture 的 `expectedKeyword` 是否指向真正拦下它的约束；feature 登记与协议 §5.2 表逐条相等；文档是否按设计只写 Sync 面 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 一次读只查一遍库（无逐条取正文的 N+1）；`docs/CORE_PORTS_AND_STORAGE.md` §5 与 `ports.rs` 逐成员相等；既有调用方（用例层、fake 读视图、既有测试）是否全部随端口形状更新而没有留下放宽的类型；未新增 DDL（若新增必须给出 migration 与 §7 同步） | 无 CRITICAL/MAJOR 未闭环项 |
| WP3 | reviewer | 覆盖判定是否**不再**依赖协议词表；「已知但无视图」是否确实产出降级卡片（要有实测）；时间线模型是否对历史与实时走同一渲染入口；实时尾巴是否按 `messageId` 替换而非追加、压缩事件是否作废；`session.origin.online_changed` 是否只改可达性标注而不越权改动摘要 | 无 CRITICAL/MAJOR 未闭环项 |
| WP4 | reviewer | 探针证据是否可读（版本、peer 警告原文、`npm ci`/`expo export` 结果）；选中的库是否满足「RN 视图 / 打包内置 / 可关原始 HTML / 不取远端资源」四条；样式是否走既有 token 而不是第二套配色 | 无 CRITICAL/MAJOR 未闭环项 |
| WP5 | reviewer | 每张卡片与每个交互是否真的来自 props（页面无业务分支）；右栏是否**不含**差异正文且不解析原文；授权按钮是否只在亲历请求上出现；窄/宽信息集合是否相同；`data-*` 锚点是否唯一且不与既有取值冲突 | 无 CRITICAL/MAJOR 未闭环项 |
| WP6 | reviewer | 新增用例是否有判别力（实现回退会红）；是否覆盖 Main E2E 的每一条；有无为了变绿而弱化或删除既有断言；富文本用例是否真的断言「不执行、不加载远端」而不是只看渲染出文本 | 无 CRITICAL/MAJOR 未闭环项 |
| WP7 | reviewer | 三份文档只改声明处；缺口登记的措辞是否不承诺未实现能力；`FRONTEND_DESIGN` §6 的「呈现能力由客户端自持」是否与 specs 措辞一致 | 无 CRITICAL/MAJOR 未闭环项 |

## Main E2E

```yaml
mode: required
```

入口是既有的 `clients/app/e2e/run-e2e.mjs`（真实 Chromium 内核 + CDP；挂真实 `app/_layout.tsx` 组合根与真实路由组件）。
本次必须断言的可观察行为：

- 时间线在同一视图里同时呈现消息与结构化条目（工具调用、终端输出、计划）；同一条目在实时与历史两条来源下只出现一次。
- 离开并重新进入一个本机自有会话后，保留期内的结构化历史条目仍然呈现。
- 输入区只接受文本并显式说明附件不受支持；提交与取消的呈现以服务端终态为准（未确认前不是「已发送」/「已空闲」）。
- 权限请求只对本设备亲历的请求呈现可选项；仅从历史读到的待处理项不可操作且有说明；已被其它设备处理的呈现为已处理。
- 模型与模式的切换中状态、下一轮生效标注与「未提供」态。
- 右栏呈现行数统计与相对化路径、给出诊断入口，且**不**呈现未下发的差异正文。
- 来源为其它节点的会话在结构化历史上显式降级，且其输入不被标记为已发送。
- 抽样若干「已知但客户端没有专用视图」的事件类型，确认它们产出降级卡片而不是静默消失。
- 富文本：Markdown 里的脚本/原始 HTML 不生效、远端图片与链接不被加载。

只启动成功、退出码为零、截图都不算通过。

## Risks

| 风险 | 缓解 |
| --- | --- |
| 新增依赖与本工程钉死的 React 19 / RN 0.86 peer 冲突 | WP4 先做探针并在报告里给出原文证据；不通过按 design D9 降级；禁止放宽框架版本 |
| 端口形状改动（WP2）比预期扩散到多个调用方 | 与 design D4 的备选路径同包内取舍，不跨包；PV2 是硬判据 |
| 覆盖集合与历史条目集合不对齐，时间线上成片降级卡片 | WP3 报告必须列出覆盖表条目；WP5/WP6 用「条目有卡片」的断言固定 |
| 实时尾巴与读取结果交替导致重复或闪烁 | 以 `messageId` 为键替换而不是追加；WP6 覆盖「先增量、后读取」序列 |
| 隔离工作区没有 `target/` 与 `node_modules`，WP1–WP5 合入前无法自测 | 改动保持机械、小步；主检出每次合入后立即跑对应 PV，红了就回退重派 |
| 与 `pwa-directory-ui-followup` 抢文件 | 本变更排在其后；其合入后 `e2e/**` 与 `app/index.tsx` 归本变更 |
