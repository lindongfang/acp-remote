# Plan

## Execution Model

前置 access-owner-data-stateless 的全部包、独立检视与 Main E2E 必须完成后才实现本变更。按 omp batch task 派发满足依赖的包；task.isolation=patch、apply:false，独立工作区交付 `<id>.patch`、`agent://<id>`、`history://<id>`。独立 reviewer spawn 读绝对 patch 路径和报告，主 Agent 串行 git apply --check、git apply，检查仅在主检出执行。本轮仅规划，不合入代码。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 标题命令、feature、有效摘要合同和权威文档 | code:access-owner-data-stateless/WP4 | wp-coder | compatibility/, schemas/, fixtures/, crates/sync-protocol/, crates/node-link-protocol/, docs/ | patch + 设置/拒绝清空/旧字段兼容 fixture | PV1 |
| WP2 | Owner 两类标题、原子命令和远程接线 | contract:WP1, code:access-owner-data-stateless/WP3 | wp-coder | crates/core/, crates/storage-sqlite/, crates/server/, crates/node-link-client/, crates/app/, scripts/ | patch + 迁移/优先级/重试/运行中改名/回源测试 | PV1、PV2 |
| WP3 | 原地双击编辑、标题统一显示与离页语义 | code:WP2, code:access-owner-data-stateless/WP5 | wp-coder | clients/app/ | patch + 编辑状态、失焦、输入法、迟到结果及浏览器证据 | PV3、Main E2E |

WP1 维护 design D5 中全部权威文档，WP2/WP3 更新各自代码注释与行为测试；需要订正合同时先回 WP1 修订 patch，不能静默修改其写范围。

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 先冻结命令与摘要形状，范围独立 | PV1；中间实现缺口必须记录 |
| 2 | WP2 | 消费新合同和无持久中转前置实现 | PV1、PV2 |
| 3 | WP3 | 服务端接通后接 UI，不能恢复旧 imported 路径 | PV1、PV2、PV3、Main E2E |

与前置变更相同目录按上述变更顺序串行修改。离线页变更只在本变更完成后开始，不因实现 /offline 而覆盖标题编辑行为。

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | npm run check / 仓库根 | 共享 schema/fixture、登记词表、文档、边界与漂移/OpenSpec 全绿 |
| PV2 | npm run check:rust / 仓库根 | fmt/clippy/workspace tests 全绿；含 Agent 不覆盖、同文确立自定义、Owner 同请求去重、迁移保留与 Access 无落盘 |
| PV3 | npm run check / clients/app | TS/E2E 类型、单测和 Web 导出通过；Enter/blur 一次提交、Esc 不保存及 composition 规则有用例 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer，独立 spawn | 旧 title 兼容、非 ACP capability、无恢复命令、无过时 Export 模型 | 无未闭环 CRITICAL/MAJOR |
| WP2 | reviewer，独立 spawn | Owner 唯一持久化、原子终态、Agent 原文、同值语义与来源查询 | 无未闭环 CRITICAL/MAJOR |
| WP3 | reviewer，独立 spawn | 仅双击入口、失焦/取消竞态、离页放弃、错误可见与多端一致 | 无未闭环 CRITICAL/MAJOR |

## Main E2E

```yaml
mode: required
```

Owner + Access + 可控 Agent + 两个客户端：双击顶部、Enter/外点保存、Esc 取消、输入法 Enter；列表/搜索/头部一致；Agent 改名与置空不覆盖自定义；同文保存也固定标题；两客户端并发后写生效；刷新/Owner 重启保留；保存结果丢失只查原请求；切换会话不写错资源、失败不保留跨页草稿；Access 重启无标题/命令/收据落盘，重新读取有效值。没有 /offline 路由不视为本变更失败，但不可用时不允许保存或显示缓存远程数据。

## Risks

前置未完成时本计划不可派发，不通过临时 SQLite imported 索引接通功能。人工浏览器观察必须包含断言与可读证据，不以截图或测试计数代替实际结果。
