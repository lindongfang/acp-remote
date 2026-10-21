# Plan

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
| WP1 | connection-health：认证后 1 秒 ping、仅匹配 pong、3 秒离线、可见性暂停/恢复、既有退避重连 | none | wp-coder | `clients/app/src/sync-client/`, `clients/app/src/composition.ts` | patch + 探活/可见性/超时单测 | PV2 |
| WP2 | connection-problem-overlay：认证门、`/pair` 回目录、顶栏在线/离线、五类弹窗与动作、错误详情 | code:WP1 | wp-coder | `clients/app/src/features/`, `clients/app/src/components/shell/`, `clients/app/app/`, `clients/app/src/components/index.ts` | patch + 状态映射/弹窗/认证门测试 | PV2 |
| WP3 | 删除对话/文件/目录/详情重复状态提示与同步时间，保留页面可访问时的功能性原因；写回前端与同步协议文档 | none | wp-coder | `clients/app/src/components/ConversationStream.tsx`, `clients/app/src/components/SessionFilesPanel.tsx`, `clients/app/src/components/SessionRow.tsx`, `clients/app/src/components/DirectoryDetail.tsx`, `clients/app/src/components/DirectoryList.tsx`, `clients/app/src/components/conversation-page.test.ts`, `clients/app/src/components/session-files-panel.test.ts`, `clients/app/src/components/r15-directory-detail.test.ts`, `clients/app/src/components/r15-directory-list.test.ts`, `clients/app/src/features/directory-model.ts`, `clients/app/src/features/client-store.ts`, `clients/app/src/features/conversation-submission.test.ts`, `clients/app/e2e/page/checks/r15-directory.ts`, `clients/app/e2e/page/checks/r15-directory-detail.ts`, `docs/FRONTEND_DESIGN.md`, `docs/SYNC_PROTOCOL.md` | patch + 移除后回归测试、文档更新 | PV1、PV2 |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 无重叠；先提供探活事件与可见性接线 | PV2 |
| 2 | WP2 | 消费 WP1 的离线事件；与 WP3 在 `components/` 下文件不同但同目录，串行更稳 | PV2 |
| 3 | WP3 | 文档与移除项在行为冻结后写回；与 WP1/WP2 无重叠文件 | PV1、PV2 |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `npm run check` / 仓库根 | 文档、协议、边界与 OpenSpec 门禁全绿；无 wire/schema/crate 依赖误改 |
| PV2 | `npm run check` / `clients/app` | 类型检查、E2E 类型、单测与 Web 导出全绿；覆盖探活、五类弹窗、认证门与移除项 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer，独立 spawn | 只认匹配 pong、可见性、超时关闭、清理与重连不重发 mutation | 无未闭环 CRITICAL/MAJOR |
| WP2 | reviewer，独立 spawn | 认证门、在线/离线映射、五类动作、错误详情不泄密、恢复入口不弹窗 | 无未闭环 CRITICAL/MAJOR |
| WP3 | reviewer，独立 spawn | 只删状态提示不删功能性原因、同步时间范围、测试不再锁定旧内容条 | 无未闭环 CRITICAL/MAJOR |

## Main E2E

```yaml
mode: required
```

真实浏览器 + 可控假主机：

- 认证前断言业务页与顶栏不出现；认证完成后进入目录。
- 已配对设备打开 `/pair` 自动回 `/`。
- 模拟 3 秒无 pong：断言顶栏转离线、弹窗出现、自动重连进展更新；恢复 pong 后弹窗消失。
- 分别触发 `revoked`（4410）、`incompatible`（4406）、`identity_changed`（4409）、`replaced`（4411），断言五类弹窗的顶栏档位、文案、错误码/关闭码与按钮动作。
- 断言对话页无内容来源条、会话行无来源离线标签、文件栏无「当前不在线」提示；输入不可提交时功能原因仍在。
- `/pair`、`/node-link/pair`、`/hosts` 不显示连接问题弹窗。
- 宽窄屏均可操作，技术详情可展开且不含敏感材料。

## Risks

- 1 秒定时器在真实浏览器后台会被节流；E2E 必须显式模拟 `visibilitychange`，不能仅靠等待。
- 探活测试依赖可控时钟；不得用真实 `setTimeout` 等待 3 秒，否则测试会抖动。
- 删除内容来源条会波及既有快照/降级测试；WP3 必须区分“离线状态提示”和“同步时间/功能性原因”。
- 认证门可能让既有测试在未认证时访问业务路由失败；WP2 需提供明确的测试装配入口，而不是在各路由散落认证判断。
- 顶栏在线 + 阻断弹窗的观感容易引起误解；E2E 需断言弹窗文案明确写“网络在线但连接不可用”。
