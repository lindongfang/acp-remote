<!-- 执行安排：把 specs 的行为要求与 design 的技术方案转成可派发的并行工作包。
     plan 只写“谁在哪个隔离工作区做什么、按什么顺序合入、怎么验”，不重复 specs/design 的内容。 -->

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出三个工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）。工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；工作包不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`，每次合入后跑主检出 Project Verify。
- 验证：隔离工作区内**没有** `target/`、`node_modules` 与浏览器内核，因此仓库级检查与浏览器级检查一律只在主检出执行（`docs/WORKFLOW.md` §4.3）。三个工作包的交付都是**代码 + 静态核对**，运行由主 Agent 在 PV1–PV5 完成。
- 检视：每个工作包由一个**独立** spawn（不与作者同实例）读 patch 与 `agent://<id>` 后给出结构化结论。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 生产入口文档不再依赖内联样式形态（design D1/D2；scenario「真实 CSP 下样式真的生效」的生产形态）：`dist/index.html` 抽出内联 reset 为同源 `entry-reset.css`、在 bundle 之前声明空的同源 `rnw-stylesheet.css` link；转换自身失败关闭 | none | wp-coder | `clients/app/scripts/`, `clients/app/package.json` | `<id>.patch`：新增入口文档转换模块（导出纯函数 + CLI 入口）并接进 `export:web`；**不另写单测**（vitest `include` 只收 `src/**/*.test.ts`，写进 `scripts/` 会被静默跳过，理由见 design D2）；报告写明三条构建期断言与它们的触发条件 | `cd clients/app && npm run check`（typecheck + vitest + `expo export`；三条断言在 `export:web` 里执行） |
| WP2 | e2e 面按生产口径带 CSP，并新增「计算样式真的生效」的检查（design D3；task 2.2） | none | wp-tester | `clients/app/e2e/` | `<id>.patch`：`run-e2e.mjs` 下发与 `STATIC_HEADERS` 同取值的五个响应头、内联错误收集脚本改为外部文件、入口 HTML 声明空表 link；新增检查断言计算样式 + 页面无内联 `<style>`，并注册进检查集 | `cd clients/app && node e2e/run-e2e.mjs`（真实 Chromium；主 Agent 执行） |
| WP3 | 如实登记新实绩与清除陈旧断言（task 3.1） | none | wp-coder | `docs/FRONTEND_DESIGN.md`, `docs/SECURITY_DESIGN.md`, `README.md`, `crates/server/src/transport/net/static_ui.rs` | `<id>.patch`：文档登记入口文档的样式形态与「同源托管下前端可用」的实绩；按实绩改写 `docs/SECURITY_DESIGN.md` §11.1 的 `[实现现状]`（旧句断言「内联 `<style id="expo-reset">` 被拒、冲突交 `frontend-sync-wiring` 解决」，本变更后不再成立；须写明基线未放宽、内联形态已移除、`insertRule` 不是 CSP 检查点，并按该文件惯例在头部补一条本次修订记录）；清掉 `static_ui.rs` 里同一条过期注释 | `npm run check:docs`, `npm run check:drift`, `npm run check:rust`（第三项覆盖 Rust 注释改动） |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 无写范围重叠 | `cd clients/app && npm run check` |
| 2 | WP2 | 无写范围重叠（`clients/app/e2e/` 与 WP1 的 `scripts/`、`package.json` 不相交） | `cd clients/app && node e2e/run-e2e.mjs` |
| 3 | WP3 | 无写范围重叠 | `npm run check:docs && npm run check:drift && npm run check:rust` |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cd clients/app && npm run check` | typecheck + vitest + `expo export --platform web` 全绿；导出后 `dist/index.html` 不含内联 `<style>`，含两份同源样式表 link |
| PV2 | `cd clients/app && node e2e/run-e2e.mjs` | 全部检查通过（含新增的样式检查）；**且**把样式改回内联形态时该检查变红（反向核对由主 Agent 在做完 PV2 后临时改回一次确认，随即还原） |
| PV3 | `npm run check`（仓库根） | 十项合同门禁全绿（含 `check:docs`、`check:drift`、`check:openspec`） |
| PV4 | `npm run verify`（仓库根） | 合同门禁 + `cargo fmt --check` / `clippy -D warnings` / `cargo test --workspace` 全绿（覆盖 WP3 的 Rust 注释改动） |
| PV5 | 主检出重新 `expo export` 后重建 Daemon，按 `## Main E2E` 的 recipe 观察 | 真实 Daemon 同源托管的页面上断言计算样式生效（见 Main E2E） |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 失败关闭是否真的关（抽不到 reset / 缺 link / 仍含内联样式三种情况都不得静默产出）；link 是否排在 bundle `<script>` 之前；空表约束是否被实现与注明；`export:web` 接线是否漏掉其它调用点 | 无 blocker/major 未闭环项 |
| WP2 | reviewer | 下发的五个头是否与 `static_ui::STATIC_HEADERS` **同取值**（不是「大意相同」）；新增检查是否**可证伪**（拦下样式时必须变红，而不是只断言 DOM 里有 `<style>`）；是否覆盖 RNW 生成的样式（不只是 reset）；既有检查集在带 CSP 的面上是否仍可运行（内联脚本、同源 WebSocket） | 无 blocker/major 未闭环项 |
| WP3 | reviewer | 文档断言与代码事实一致；没有把未实现的能力写成已实现；`static_ui.rs` 的注释改动是否只删过期断言、没有顺手放宽 CSP 的描述 | 无 blocker/major 未闭环项 |

## Main E2E

```yaml
mode: required # required | not-applicable
reason: spec 的 WHEN 条件写的是「浏览器从 Daemon 同源托管的来源加载页面（响应带上述 CSP）」，因此判据必须在**真实 Daemon 托管**的面上取，而不是只在假主机面上。
basis: docs/WORKFLOW.md §7 要求按被测面选 prelude（Web/PWA 用 browser prelude）且「E2E 通过的标准是断言了可观察行为」。recipe 沿用上一变更（`archive/2026-10-16-frontend-sync-wiring/plan.md`）的真机验收方式：dev 模式 Daemon + `public_origin` + 真实 Chromium 内核。
alternative_checks: []
```

断言（主 Agent 给出实测输出，截图不作为通过判据）：

1. 从 Daemon 同源托管来源加载页面，响应确实带 `default-src 'self'; script-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'`；
2. `document.querySelectorAll('style').length === 0`（页面不再依赖内联样式形态）；
3. `#root` 的计算 `display === 'flex'`、计算高度等于视口高度（reset 真的生效）；
4. 至少一处**RNW 生成**的样式真的生效（取首个子元素的计算样式：`display === 'flex'` 且 `flex-direction === 'column'`），而不是只验证 reset；
5. 对照：把前端改动 stash 掉、重新 `expo export` **并重新构建 Daemon 二进制**（产物在编译期嵌入，只换 `dist` 不会改变托管内容）后重启，同一批断言必须让第 3/4 条变红，随即还原（证明判据有区分力）。

## Risks

- **[隔离区跑不了浏览器与仓库级检查]** WP1/WP2 的作者在隔离工作区内无法运行 `npm run check`、`node e2e/run-e2e.mjs` → 交付以代码 + 静态核对为准，真实运行由主 Agent 在 PV1–PV5 补齐；子 Agent 不得尝试安装依赖或下载浏览器。
- **[e2e 面加 CSP 会牵动既有检查集]** 既有十条检查全部改在带 CSP 的面上跑 → 若变红，先判定是「CSP 连带的接线问题（内联脚本、同源 WebSocket）」还是「样式回归」，再决定派修复还是回退该工作包。
- **[空表约束易被顺手破坏]** 往 `rnw-stylesheet.css` 里写内容会让 RNW 的 hydration 抛错、页面整页空白 → 已列为 WP1/WP2 的检视项，且由 PV2 的检查兜住。
- **[生产面与 e2e 面是两份入口文档]** 生产入口文档的样式形态由 WP1 的构建期断言覆盖，它的**运行时效果**由 e2e 面同一机制覆盖（两者共用 RNW 的 id 约定）→ 两处都要成立；PV5 是对生产面的端到端复核。
- **[判据有区分力这件事本身要验]** 「断言会变红」不能靠推理 → PV2 与 Main E2E 各有一条反向核对（临时改回内联形态确认变红，随即还原）；Main E2E 的核对必须**重新构建 Daemon 二进制**——前端产物在编译期嵌入，只重跑 `expo export` 不会改变托管内容。
