## Execution Model

本次由主 Agent 在共享主检出实现；可用环境没有 omp 隔离派发工具，不伪造 patch 交付或合入记录。工作区基线仅含用户未跟踪的 `.diagnostics/`，不改动其内容。独立只读检视使用单独 spawn，验证在主检出执行。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 三段布局、滚动、折叠、输入栏与缺失数据说明 | none | root | clients/app/src/components/, clients/app/src/features/conversation-settings-model.ts, clients/app/src/features/conversation-settings-model.test.ts, clients/app/e2e/page/checks/conversation-scroll-paging.ts, clients/app/e2e/page/checks/conversation-interaction.ts, clients/app/e2e/page/checks/conversation-reliability.ts, docs/FRONTEND_DESIGN.md | 工作区 diff | PV1–PV3 |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 单工作包，无并发写入 | PV1–PV3 |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | npm.cmd --prefix clients/app run check / 仓库根 | 类型、测试与 Web 构建通过 |
| PV2 | npm.cmd run check / 仓库根 | 合同与文档门禁通过 |
| PV3 | node clients/app/e2e/run-e2e.mjs / 仓库根，E2E_CHECK_PREFIX=SCROLL | 浏览器位置与布局断言通过 |

## Code Review

| Work Package | Reviewer | Scope | Result |
| --- | --- | --- | --- |
| WP1 | 独立 spawn review_conversation | 真实 git diff | 两项发现已修复，独立复核未发现新的确凿滚动错误；验证结果见 validation.md |

## Main E2E

- required: 真实浏览器验证历史分页、持续上读、底部增长跟随、顶部折叠和工具栏区域位置。
