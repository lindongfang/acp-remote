# Plan

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出可并行的工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）。工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；工作包不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`；每次合入后跑主检出 Project Verify。
- 验证：隔离工作区内**没有** `target/` 与 `node_modules/`，因此仓库级检查只在主检出执行。
- 检视：每个工作包由一个**独立** spawn（不与作者同实例）读 patch 与 `agent://<id>` 后给出结构化结论。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 前端整体迁移到当前目录身份：协议镜像、解码边界、领域/页面模型、路由 `/dir/[directoryId]`、创建载荷、feature 广告、全部单测 | none | wp-coder | `clients/app/src/protocol/`, `clients/app/src/sync-client/`, `clients/app/src/domain/`, `clients/app/src/features/`, `clients/app/src/components/`, `clients/app/app/dir/`, `clients/app/src/routing.test.ts`, `clients/app/src/composition.test.ts`, `clients/app/src/layers.test.ts` | patch + 迁移后的前端与单测 | PV2 |
| WP2 | e2e 夹具与 R15/R16/路由断言改为 `{ directoryId, path }`、`{ directoryId }` 与 `/dir/<directoryId>`；保留原有信息集合与可达性断言 | code:WP1 | wp-coder | `clients/app/e2e/` | patch + e2e 夹具与断言 | PV2, PV3 |
| WP3 | 文档现行口径 + Rust 格式与基线死代码清理：`FRONTEND_DESIGN`/`SYNC_PROTOCOL` 的目录身份，README/缺口清单；`cargo fmt` + clippy 死代码清理 + `sync_workspace_catalog` 去掉 `include_path` | none | wp-coder | `docs/FRONTEND_DESIGN.md`, `docs/SYNC_PROTOCOL.md`, `README.md`, `prototypes/IMPLEMENTATION-GAPS.md`, `crates/**`（仅格式与死代码清理） | patch + 文档与 Rust 基线修复 | PV1 |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 前端迁移必须整体合入：协议镜像与领域/页面/路由强耦合，拆开会在中间状态无法编译 | PV2 |
| 2 | WP3 | 与前端写范围无重叠；落地文档与 Rust 格式 | PV1, PV2 |
| 3 | WP2 | 依赖 WP1 的协议形状与路由；页面稳定后再改 e2e | PV2, PV3 |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `npm run verify`（仓库根） | 合同门禁 + Rust 检查全绿；含 `cargo fmt --check`、clippy、Rust tests、`check:openspec` |
| PV2 | `cd clients/app && npm run check` | typecheck、`typecheck:e2e`、vitest 全绿、`expo export --platform web` 成功 |
| PV3 | `cd clients/app && node e2e/run-e2e.mjs` | 真实 Chromium 的 e2e 全绿（75/75 或更新后的等价集合） |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 协议镜像与 schema 逐项一致；目录标识不外显；展示名/消歧只由服务端路径派生；路由与载荷；死代码清理；单测判别力 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 夹具是否真实反映新 wire；断言是否保留原信息集合与可达性；是否仍能抓住“退回别名/目录标识显示”的回归 | 无 CRITICAL/MAJOR 未闭环项 |
| WP3 | reviewer | 文档与实现一致；`cargo fmt` 只含格式差异、无逻辑改动；没有改出写范围 | 无 CRITICAL/MAJOR 未闭环项 |

## Main E2E

```yaml
mode: required
reason: ""
basis: "目录页/详情页/左栏/创建弹层的身份与路由都是浏览器级可观察行为；e2e 已在真实 Chromium 中驱动真实页面与假主机。"
alternative_checks: []
```

## Risks

- **WP1 体量大**：协议、领域、页面、路由与测试必须一起合入；拆成多个可并行包会产生无法编译的中间树。
- **展示名变化**：路径尾段与同名消歧会改变测试期望与用户看到的名称；e2e 必须按路径夹具逐项更新，不能只改一个样例。
- **Rust 格式 diff 面**：`cargo fmt` 可能触碰多个 crate；reviewer 必须确认没有语义变化。
- **旧文档**：`CORE_PORTS_AND_STORAGE.md` / `MODULE_ARCHITECTURE.md` 仍有历史别名描述；本变更只修前端现行口径，历史设计文档另案处理。
