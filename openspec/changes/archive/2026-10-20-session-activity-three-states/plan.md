# Plan

## Execution Model

- 使用 omp `task` batch 派发同一依赖阶段内可独立工作的包，共享 context，逐包指定目标与范围。依赖未满足不提前派发。
- 写包均 `isolated: true`；`task.isolation` 为 patch 模式且 `apply: false`。
- 每包交付 `<id>.patch`、`agent://<id>`、`history://<id>`，不创建提交或分支。
- 主 Agent 串行 `git apply --check` → `git apply`，仓库级验证仅在主检出执行，隔离区不运行依赖 target/node_modules 的检查。
- 每包由不同 spawn 的内置 reviewer 读绝对路径 patch 及报告、过程给结构化 verdict。修复及复核均新 spawn，最多三轮，只合入最终通过版本。
- 本文件为待执行计划，创建规划不代表已经派发、检视或实现。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 冻结三态合同、feature、历史版本边界及 delta 基线 | contract:conversation-area-complete 最终合同与规范已落地 | wp-coder | `schemas/`, `fixtures/`, `compatibility/features/v1/features.json`, `crates/sync-protocol/`, `crates/node-link-protocol/`, `scripts/`, `docs/SYNC_PROTOCOL.md`, `docs/NODE_LINK_PROTOCOL.md`, `openspec/changes/session-activity-three-states/` | patch + 新旧协商/枚举/旧历史向量与基线核对报告 | PV1、PV2 |
| WP2 | 三态归约、交互归属与仲裁阶段、失败恢复、持久迁移 | contract:WP1 | wp-coder | `crates/core/`, `crates/storage-sqlite/`, `docs/CORE_PORTS_AND_STORAGE.md`, `docs/INITIAL_DESIGN.md`, `docs/MODULE_ARCHITECTURE.md` | patch + 状态矩阵/崩溃窗口/升级回滚测试 | PV1、PV3 |
| WP3 | 端点结束确认、适配器映射、服务端协商与调度接线 | code:WP2, contract:WP1 | wp-coder | `crates/agent-host/`, `crates/server/`, `crates/app/`, `crates/node-link-client/`, `docs/MODULE_ARCHITECTURE.md`, `docs/ACP_COMPATIBILITY_MATRIX.md`, `compatibility/acp/v1/matrix.json` | patch + 排队继续推进、停止确认和请求处理集成证据；node-link-client 仅在前置变更已落地时修改 | PV1、PV3 |
| WP4 | PWA 三态、原因与结果分离、保留断线状态 | contract:WP1, code:WP2（摘要形状） | wp-coder | `clients/app/src/`, `docs/FRONTEND_DESIGN.md`, `prototypes/` | patch + 目录/会话头/失效卡片/断线用例；不重绘无关 UI | PV1、PV4 |
| WP5 | 跨层回归与真实浏览器验收 | code:WP3, code:WP4 | wp-tester | `clients/app/e2e/`, `crates/app/tests/`, `crates/server/tests/`, `README.md`, `openspec/changes/session-activity-three-states/` | patch + E2E 断言及证据；不得借测试包修改业务实现 | PV1、PV3、PV4、PV5 |

WP3/WP4 在 WP2 后可同批派发。全部写范围以表中精确文件或前缀为限；超出则先修订本计划。

## Merge Order

前置：先落地 `conversation-area-complete`，重定本变更 PWA delta（输入 requirement 的标题和附件全文）以及迁移版本，不回退其新行为。当前主检出的其他未提交改动不属于本变更，实施前单独处理基线。

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 冻结共享合同与重定 delta | PV1、PV2 |
| 2 | WP2 | 消费合同并实现权威语义 | PV1、PV3 |
| 3 | WP3 | 依赖 WP2；与 WP2 重叠 `docs/MODULE_ARCHITECTURE.md`，须从 WP2 后基线派发并保留其描述 | PV1、PV3 |
| 4 | WP4 | 与 WP3 无写范围重叠，串行应用 | PV1、PV4 |
| 5 | WP5 | `crates/app/tests/`、`crates/server/tests/` 与 WP3 的前缀重叠；变更目录与 WP1 重叠，从 WP1–WP4 后基线派发并保留既有内容 | PV1、PV3、PV4、PV5 |

## Project Verify

所有检查在主检出执行；中间合同/实现尚未接齐导致的跨包失败明确记录，不能声称单包可部署。最终合入必须全绿。

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | 根目录 `npm run check` | 全合同门禁，包括迁移合同漂移和 OpenSpec；不修改其他变更以掩盖失败 |
| PV2 | 根目录 `cargo test --locked -p sync-protocol -p node-link-protocol` | 三态词表、必需 feature、新旧握手与历史向量 |
| PV3 | 根目录 `npm run verify` | Rust 格式、clippy、workspace 测试及合同全绿；覆盖 core/store/host/server/app 联动 |
| PV4 | `clients/app` 下 `npm run check` | 类型、领域状态与组件回归全绿，覆盖多个交互和断线 |
| PV5 | 根目录 `node clients/app/e2e/run-e2e.mjs`，并按 Main E2E 使用 browser/computer prelude 补齐真实 Daemon 场景 | 现有脚本只覆盖浏览器与可控同步主机；真实 Daemon + 可控 ACP Agent 另行启动并验收，可复现记录带命令、步骤和断言 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer（独立 spawn） | 词表单源、双向兼容拒绝、历史数据不改写、前置 delta 不回退 | 无未闭环 blocker/major |
| WP2 | reviewer（独立 spawn） | 原子归约、多个交互、应答仲裁与确认、迁移外键与序列、uncertain 防重复 | 无未闭环 blocker/major |
| WP3 | reviewer（独立 spawn） | 调用方与调度闭环、确认停止后再推进、不虚报撤回能力、Access 不写 Owner 状态 | 无未闭环 blocker/major |
| WP4 | reviewer（独立 spawn） | 导航无副作用、失败仍可继续、主状态与原因/连接分离、不覆盖附件和输入策略 | 无未闭环 blocker/major |
| WP5 | reviewer（独立 spawn） | 验收有真实断言、覆盖风险且不只验启动、证据与实际 patch 一致 | 无未闭环 blocker/major |

## Main E2E

```yaml
mode: required
```

Web/PWA 通过 browser prelude；Daemon 重启和进程停止确认通过 computer prelude，按 `docs/WORKFLOW.md` §7 的桌面审批覆盖执行。可控 ACP Agent 依次驱动：新建 idle；受理排队 running；两个请求 pending；处理一个仍 pending；最后一个确认处理 running；任务结束 idle；失败后下一轮继续；取消确认后遗留卡失效；页面切换不中止任务；断线保留状态并在重连后收敛。重启场景断言 uncertain 不重发副作用。旧版本协商拒绝与旧库升级使用集成套件补充；无法运行的层明确报告未完成，不以截图或零退出码代替断言。

## Risks

- 共享在途合同与输入 requirement 正在变化：WP1 前置基线核对，不自行改其他变更。
- 中间包可能暂时无法编译：连续完成依赖链再判最终门禁，不提交或发布半成品。
- Windows 真实进程测试需要停止测试进程：只操作本次测试拥有的进程与临时库，不接触用户工作会话。
