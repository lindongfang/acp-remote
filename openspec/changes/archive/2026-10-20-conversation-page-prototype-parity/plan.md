<!-- 执行安排：把 specs 的行为要求与 design 的技术方案转成可派发的并行工作包。 -->

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出多个工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）。工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；工作包不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`，每次合入后跑主检出 Project Verify。
- 验证：隔离工作区内**没有** `target/` 与 `node_modules/`，因此仓库级检查只在主检出执行。
- 检视：每个工作包由一个**独立** spawn（不与作者同实例）读 patch 与 `agent://<id>` 后给出结构化结论。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | `sync-event-projection` 的工具详情字段：`toolDetail`、部分上限、`truncatedParts`、未知 content 不透传（4 个 scenario） | none | wp-coder | `crates/agent-host/`, `crates/sync-protocol/`, `schemas/sync/v1/`, `fixtures/sync/v1/`, `docs/SYNC_PROTOCOL.md` | patch：Rust view 字段、schema、fixtures、协议文档；报告说明上限与截断实现位置 | `cargo test -p agent-host -p sync-protocol`；`npm run check` |
| WP2 | 客户端消费工具详情：协议类型、时间线卡片归并、原文/事件标识（3 个 scenario） | contract:WP1 | wp-coder | `clients/app/src/protocol/`, `clients/app/src/domain/timeline.ts`, `clients/app/src/domain/domain.test.ts`, `clients/app/src/features/conversation-timeline.ts`, `clients/app/src/features/conversation-timeline.test.ts` | patch：`toolDetail` 类型、跨事件部分合并、卡片 eventId；单测覆盖部分合并与截断并集 | `cd clients/app && npm run typecheck && npx vitest run src/domain src/features/conversation-timeline.test.ts` |
| WP3 | 对话页外框与左栏：填满视口的三栏独立滚动、输入区入中栏、单行会话头、紧凑会话行、返回目录与 pager（左栏 6 个 scenario、宽屏滚动 scenario） | code:WP2 | wp-coder | `clients/app/app/session/`, `clients/app/src/components/ConversationStream.tsx`, `clients/app/src/components/SessionRow.tsx`, `clients/app/src/components/StatusPill.tsx`, `clients/app/src/components/conversation-page.test.ts`, `clients/app/src/components/conversation-responsive.test.ts`, `clients/app/src/components/shell/shell-model.ts`, `clients/app/src/components/shell/shell-view.test.ts`, `clients/app/src/components/shell/ShellView.tsx` | patch：外壳 `contentFill`、三栏内滚动、左栏紧凑行与入口、会话头单行与状态字形；组件测试断言 DOM 结构与滚动属性 | `cd clients/app && npm run typecheck && npx vitest run src/components/conversation-page.test.ts src/components/conversation-responsive.test.ts src/components/shell/shell-view.test.ts` |
| WP4 | 中栏内容与输入区：工具详情折叠块、原文入口、进行中只读、代码行号、右栏前缀与紧凑清单、设置弹层说明（工具 4 个 scenario、输入 scenario、变更文件 scenario、富文本 scenario） | code:WP2 | wp-coder | `clients/app/src/components/ConversationTimeline.tsx`, `clients/app/src/components/ConversationComposer.tsx`, `clients/app/src/components/ConversationSettingsPanel.tsx`, `clients/app/src/components/FileChangesPanel.tsx`, `clients/app/src/components/CodeBlock.tsx`, `clients/app/src/components/Markdown.tsx`, `clients/app/src/components/conversation-timeline-view.test.ts`, `clients/app/src/components/conversation-composer.test.ts`, `clients/app/src/components/conversation-settings-view.test.ts`, `clients/app/src/components/file-changes-panel.test.ts`, `clients/app/src/components/codeblock.test.ts`, `clients/app/src/features/conversation-input-model.ts`, `clients/app/src/features/conversation-submission.test.ts` | patch：工具详情/截断/原文按钮、只读三态、行号、差异前缀、紧凑右栏与设置说明；组件测试逐项断言 | `cd clients/app && npm run typecheck && npx vitest run src/components/conversation-timeline-view.test.ts src/components/conversation-composer.test.ts src/components/conversation-settings-view.test.ts src/components/file-changes-panel.test.ts src/components/codeblock.test.ts src/features/conversation-submission.test.ts` |
| WP5 | 浏览器 E2E：宽屏三栏与滚动、左栏入口、输入区只读、工具详情与截断、差异前缀（Main E2E 用例） | code:WP3, code:WP4 | wp-coder | `clients/app/e2e/` | patch：新增/更新对话页检查集与 `page/index.tsx` 注册；报告列出每条可观察断言 | `cd clients/app && npm run typecheck:e2e && node e2e/run-e2e.mjs` |
| WP6 | 文档写回：对话页口径、原型差异与已落地状态 | code:WP3, code:WP4 | wp-coder | `docs/FRONTEND_DESIGN.md`, `prototypes/IMPLEMENTATION-GAPS.md` | patch：更新对话页三栏/滚动、内容来源提示、左栏行、输入区、工具详情与右栏口径；不写未落地能力 | `npm run check:docs` |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 合同投影必须先于客户端与 UI | `cargo test -p agent-host -p sync-protocol`；`npm run check` |
| 2 | WP2 | 客户端类型依赖 WP1 的字段 | `cd clients/app && npm run typecheck`；`npx vitest run src/domain src/features/conversation-timeline.test.ts` |
| 3 | WP3 | 外框与左栏先落，便于 WP4 的组件挂在最终骨架上 | `cd clients/app && npm run typecheck && npm run test` |
| 4 | WP4 | 组件与 WP3 无写范围重叠；合入后跑完整客户端检查 | `cd clients/app && npm run check` |
| 5 | WP5 | E2E 断言最终 DOM 与滚动行为 | `cd clients/app && node e2e/run-e2e.mjs` |
| 6 | WP6 | 文档描述最终行为 | `npm run check:docs` |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `npm run check`（仓库根） | schema/fixture/合同资产/文档链接/边界/openspec 全绿 |
| PV2 | `cargo test -p agent-host -p sync-protocol`（仓库根） | 工具详情投影、上限、截断与原文保真测试全绿 |
| PV3 | `cd clients/app && npm run check` | typecheck、e2e typecheck、vitest 全绿且 `expo export` 成功 |
| PV4 | `cd clients/app && node e2e/run-e2e.mjs` | 全部浏览器检查通过，含新增对话页用例 |
| PV5 | `npm run verify`（仓库根） | 最终门禁：合同检查 + Rust 格式/ clippy / 全 workspace 测试 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 字段上限与截断、未知 content、原文保真、schema/fixture 一致性 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 跨事件部分合并、截断并集、类型边界、未解析 rawJson | 无 CRITICAL/MAJOR 未闭环项 |
| WP3 | reviewer | 外壳 `contentFill` 只影响对话路由、独立滚动、左栏信息集合与入口 | 无 CRITICAL/MAJOR 未闭环项 |
| WP4 | reviewer | 进行中只读、工具详情与原文边界、差异前缀、无占位内容 | 无 CRITICAL/MAJOR 未闭环项 |
| WP5 | reviewer | E2E 断言可判别性、真实页面路径、无静态快照取巧 | 无 CRITICAL/MAJOR 未闭环项 |
| WP6 | reviewer | 文档与最终实现一致、无未落地能力 | 无 CRITICAL/MAJOR 未闭环项 |

## Main E2E

```yaml
mode: required
reason: 对话页的布局、滚动与输入区状态是视觉/交互行为，组件测试无法证明真实视口与滚动容器
basis: prototypes/acp-remote-pwa.html 的 viewChat；spec 的对话页宽窄形态与输入区三态
alternative_checks: []
```

用例至少覆盖：

- 宽屏下三栏同时可见，输入区位于中栏且不横跨左右栏；滚动时间线时左右栏不移动。
- 左栏会话行只含状态圆点、标题、状态词与时间；「‹ 目录」与「全部 ›」分别到达目录列表与详情。
- 进行中：输入框只读，发送不呈现，停止呈现；按回车不发命令。
- 工具调用折叠块展示输入/输出，`truncatedParts` 时显示截断说明，原文按钮打开诊断。
- 差异行带 `+` / `-` / 空格前缀；代码卡显示行号。
- 窄屏次级视图切换不丢信息。

## Risks

- WP1 与 WP2 的字段名/上限若不一致，合同检查会红；以 `docs/SYNC_PROTOCOL.md` 与 schema 为唯一口径。
- WP3 的 `contentFill` 若泄漏到其它路由，目录页滚动会回归；外壳模型测试与 R15 E2E 必须同时通过。
- WP4 的组件改动若改变既有 `data-*` 集合，既有 E2E 会红；保留原有机器证据并只增不减。
- WP5 依赖最终 DOM，必须最后合入；不要用截图或仅启动成功作为通过判据。
