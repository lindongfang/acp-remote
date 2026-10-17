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
| WP1 | 配对页前端实现：地址唯一入口、固定呈现模型、`expiresAtMs` 与到期定时器、无载荷/类型不匹配/过期分类、删除输入框时代的字段与分支；居中状态页、转圈与倒计时、固定失败页、approved 后 1.6 秒 `replace` 到 `/`；同步单测与组合根测试 | none | wp-coder | `clients/app/src/features/pairing-model.ts`, `clients/app/src/features/pairing-model.test.ts`, `clients/app/src/features/client-store.ts`, `clients/app/src/composition.ts`, `clients/app/src/composition.test.ts`, `clients/app/src/components/PairingPanel.tsx`, `clients/app/app/pair.tsx`, `clients/app/app/node-link/pair.tsx` | patch + 配对模型/仓库/组合根/页面/面板与单测 | PV1, PV2 |
| WP2 | 浏览器 e2e：地址片段驱动 PW-1/PW-3/PW-4/NL-1/NL-2/NL-3；无载荷页、倒计时归零、自动进目录、无页头/返回；更新 e2e 支撑注释与必要的场景参数 | code:WP1 | wp-coder | `clients/app/e2e/page/checks/pairing-first-run.ts`, `clients/app/e2e/page/checks/pairing-kinds.ts`, `clients/app/e2e/page/checks/ui-shell.ts`, `clients/app/e2e/page/support.tsx`, `clients/app/e2e/page/fixtures.tsx` | patch + e2e 断言与场景 | PV2, PV3 |
| WP3 | 权威文档与缺口清单：`FRONTEND_DESIGN` 的配对页现状/约束/验收、`IMPLEMENTATION-GAPS` §2.4/§5、README 中与配对页现状冲突的句子 | none | wp-coder | `docs/FRONTEND_DESIGN.md`, `prototypes/IMPLEMENTATION-GAPS.md`, `prototypes/README.md`, `README.md` | patch + 文档更新 | PV1 |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 前端实现与测试必须一起合入：模型改动会让旧面板/旧测试无法编译，不能把中间状态留在主检出 | PV1, PV2 |
| 2 | WP3 | 与代码包无写范围重叠；落地文档口径后再写 e2e 说明 | PV1 |
| 3 | WP2 | 依赖 WP1；页面锚点与行为稳定后才写浏览器断言 | PV2, PV3 |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `npm run verify`（仓库根） | 合同门禁 + Rust 检查全绿；含 `check:openspec`、schema/fixture、边界与 clippy/test |
| PV2 | `cd clients/app && npm run check` | 前端 typecheck、e2e typecheck、vitest 全绿、`expo export --platform web` 成功 |
| PV3 | `cd clients/app && node e2e/run-e2e.mjs` | 真实 Chromium 下配对页新增/改写用例全绿；含无载荷、倒计时归零、自动进目录、无页头/返回 |
| PV4 | `cargo test -p server expired_pairing_is_rejected_with_410_and_writes_nothing` 与 `cargo test -p server status_reports_the_five_business_states_with_200`（仓库根） | 两条既有用例通过，作为「越过 `expiresAt` 后 claim 与 status 都失败」的后端证据；后端无生产代码变更 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 配对秘密边界、状态迁移、定时器取消、错误分类、无载荷/过期/类型不匹配、自动跳转清理、主题调色板、固定文案与数据锚点 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 断言是否真的走地址片段、是否覆盖倒计时/无载荷/自动跳转、是否保留原有安全与密码学负向用例 | 无 CRITICAL/MAJOR 未闭环项 |
| WP3 | reviewer | 文档与 specs/实现的一致性、是否把计划写成已实现、是否残留「粘贴入口」旧口径 | 无 CRITICAL/MAJOR 未闭环项 |

## Main E2E

```yaml
mode: required
reason: ""
basis: "配对页的入口、倒计时、自动跳转与失败呈现都是浏览器级可观察行为；假主机 e2e 已在真实 Chromium 中驱动真实页面与真实组合根。"
alternative_checks: []
```

## Risks

- **WP1 体量较大**：逻辑、页面与单测必须一起合入，否则主检出会在两个工作包之间无法编译；这是由模型接口与面板签名强耦合造成的，不是排期选择。
- **e2e 时间断言**：倒计时与 1.6 秒跳转依赖真实时钟，断言要留足余量，不能写死毫秒级相等。
- **没有后端 patch**：后端证据来自既有用例与 PV4；若既有用例名或行为在合入时变化，先修用例名/证据再继续，不新增生产逻辑。
