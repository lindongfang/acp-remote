## Execution Model

仓库标准为 omp batch task、task.isolation patch 模式且 apply:false、patch + agent:// 报告及独立 reviewer，主检出串行 git apply。当前宿主未提供 omp task；可用协作工具只用于独立隔离副本的 UI patch 与只读检视，根 Agent 完成 store 与主检出验证。此差异明确记录，不声称已执行 omp。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 刷新状态、触发与测试文档 | none | root | clients/app/src/features/, clients/app/src/composition.ts, clients/app/app/session/[id].tsx, docs/, openspec/ | diff 与测试证据 | PV1/PV2 |
| WP2 | 按钮反馈和展开目录上报 | contract:WP1 | wp-coder | clients/app/src/components/SessionFilesPanel.tsx | 隔离副本 patch 与报告 | PV1/PV3 |

## Merge Order

先 WP1，后 WP2。写范围不重叠，WP2 合入后重新验证类型与集成测试。

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | npm.cmd run typecheck / clients/app | 类型检查通过 |
| PV2 | npm.cmd run test / clients/app | 全部前端测试通过 |
| PV3 | npm.cmd run export:web / clients/app | 生成静态资源成功 |
| PV4 | npm.cmd run check / 仓库根 | 合同检查通过 |

## Code Review

| Work Package | Reviewer | Input | Pass Criteria |
| --- | --- | --- | --- |
| WP1 | 独立只读 spawn | diff、访谈与测试证据 | 无阻断问题 |
| WP2 | 独立只读 spawn | patch、报告与浏览器证据 | 无阻断问题 |

## Main E2E

required：使用现有浏览器检查入口，验证按钮反馈、请求触发和无正文覆盖。

## Risks

主检出已有用户未提交改动，必须保留；UI 从当前副本生成最小 patch，合入先 git apply --check。
