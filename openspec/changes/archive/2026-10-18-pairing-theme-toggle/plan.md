# Plan

## Execution Model

- 本变更只有一个前端工作包；由主 Agent 在独立分支上直接实现并按 `docs/WORKFLOW.md` 合入。
- 验证在主检出执行；隔离工作区没有 `node_modules`。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 配对页右上角明暗切换，复用共享主题偏好；不恢复顶栏 | none | wp-coder | `clients/app/`, `docs/FRONTEND_DESIGN.md`, `prototypes/README.md`, `openspec/changes/pairing-theme-toggle/` | patch + 组件测试 + e2e + 文档 | `cd clients/app && npm run typecheck && npm run test`；`node e2e/run-e2e.mjs`；`npm run check:docs`；`npm run check:openspec` |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 唯一工作包 | `cd clients/app && npm run typecheck && npm run test`；`npm run check:docs` |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cd clients/app && npm run typecheck && npm run test` | 类型检查与全部单测通过 |
| PV2 | `cd clients/app && node e2e/run-e2e.mjs` | 真实 Chromium：配对页按钮存在、点击切换、刷新保持、原有配对用例不回归 |
| PV3 | `npm run check:docs`；`npm run check:openspec` | 文档与变更产物校验通过 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 共享偏好复用、按钮可达性、不恢复顶栏、窄屏遮挡、测试判别力 | 无 blocker/major 未闭环 |

## Main E2E

```yaml
mode: required
reason: 这是浏览器 UI 行为（按钮可见、点击切换、持久化）。
basis: ""
alternative_checks: []
```

## Risks

- 320px 宽下固定按钮可能压住内容；e2e 需覆盖窄屏并确认输入格可用。
- 现有 e2e 可能断言配对页“没有主题按钮”；需要同步更新旧断言，而不是删除覆盖。
