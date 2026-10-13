<!-- 执行安排：把 specs 的行为要求与 design 的技术方案转成可派发的并行工作包。
     执行原语固定为 omp：batch task 派发、patch 模式隔离、`<id>.patch` 交付、主 Agent 串行 git apply。 -->

# Plan

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出多个工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）。工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history/<id>`（后者即 `history://<id>`）；工作包不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`，每次合入后跑主检出 Project Verify。
- 验证：隔离工作区内**没有** `node_modules/` 与 `dist/`，因此前端检查只在主检出执行（`cd clients/app && npm ci && npm run check`）。
- 检视：每个工作包由一个**独立** spawn（不与作者同实例）读 patch 与 `agent://<id>` 后给出结构化结论。
- **前置**：`sync-server-connection`、`sync-server-commands`、`sync-ui-hosting` 必须先合入（否则真实端到端验收无对象）。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 连接契约对齐（覆盖 `pwa-web-client`「连接端点与子协议与 wire 权威一致」与 MODIFIED 的「子协议被拒时进入可区分的阻断态」、design D5）：端点改 `/sync/v1`、socket 端口容纳子协议并由组合根传入、子协议值与机器登记一致、必需 feature 未选中的处理 | none | wp-coder | `clients/app/src/sync-client/`, `clients/app/src/composition.ts`, `clients/app/src/protocol/contract.test.ts`（若需追加登记断言）, `clients/app/e2e/page/fake-host.ts`（假 socket 记录子协议） | patch + 「出站 URL 的断言方式、子协议值的唯一来源、假 socket 如何让『未声明』可被证伪、必需 feature 判定的落点」 | 主检出 `cd clients/app && npm run check` |
| WP2 | 配对链路闭合（覆盖「配对完成时写入主机身份材料」「配对页提供二维码输入入口」、design D2/D3）：`poll` 已批准结果携带主机材料、配对页输入入口与校验、清除片段秘密、缺材料时可区分失败 | code:WP1（组合根与端口的装配点同为 `composition.ts`） | wp-coder | `clients/app/src/composition.ts`, `clients/app/src/features/pairing-model.ts`, `clients/app/src/features/client-store.ts`（配对相关状态）, `clients/app/app/pair.tsx`, `clients/app/src/components/PairingPanel.tsx` | patch + 「已批准分支的字段来源与落盘时机、缺材料的失败语义、输入校验与片段清除的位置、秘密不落日志的证据」 | 主检出 `cd clients/app && npm run check` |
| WP3 | 路由与片段争用消解（覆盖「路由与配对片段的争用取舍」、design D4）：核实实际路由模式、对齐源码注释与行为、保证二维码内容可读且清除后仍在配对页、其余页面可达 | code:WP2（配对页需先有输入入口才能验片段行为） | wp-coder | `clients/app/app/`（路由与布局）, `clients/app/app.json`（若需路由配置）, `clients/app/src/composition.ts`（若涉及片段读取） | patch + 「实际路由模式的核实证据（读产物或 e2e 观察）、取舍理由与协议依据、其余页面可达性的断言」 | 主检出 `cd clients/app && npm run check` |
| WP4 | 前端自检与登记更新（覆盖「与已就绪 Daemon 的端到端可用」的用例侧 + design D6）：更新 e2e 的「真实配对不可达」登记、新增从「未配对」出发的真实配对用例、新增四处缺口的可证伪断言 | code:WP3 | wp-tester | `clients/app/e2e/**`, `clients/app/src/**/*.test.ts`（仅测试文件） | patch + 断言清单 + 隔离区能做的核对结果 | 主检出 `cd clients/app && npm run check`；端到端用例见 Main E2E |
| WP5 | 文档同步：`docs/FRONTEND_DESIGN.md`（§4.1 实绩、§4.5 边界、§5 状态机、§9 验收标准）、`docs/SECURITY_DESIGN.md`（若配对秘密清理口径需写实）、`README.md`（前端现状） | code:WP4（文档要反映已验证的事实） | wp-coder | `docs/`, `README.md` | patch + 逐份文档的改动点与依据 | 主检出 `npm run check:docs` |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 无重叠（连接层与协议层） | `cd clients/app && npm run check` |
| 2 | WP2 | 与 WP1 在 `clients/app/src/composition.ts` 重叠（两者都改组合根） | 重跑 `cd clients/app && npm run check`（WP1 已改该文件） |
| 3 | WP3 | 与 WP2 在 `clients/app/src/composition.ts` 重叠，且依赖其配对页改动 | 重跑 `cd clients/app && npm run check` |
| 4 | WP4 | 依赖 WP1–WP3 的最终形状（断言针对改完的行为） | `cd clients/app && npm run check`；端到端用例 |
| 5 | WP5 | 无重叠（`docs/` 与 `README.md`） | `npm run check:docs` |

说明：WP1/WP2/WP3 三者都在 `composition.ts` 上重叠（组合根是连接、配对与片段读取的共同装配点），已串行。这是**同一个文件的三个关注点**，无法拆分并行；若在实现中发现可以把其中一块移出组合根（例如片段读取下沉到平台端口），可回到本文件重划工作包，但不得让两个包并行改同一文件。

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cd clients/app && npm ci && npm run check`（主检出） | typecheck + vitest + `expo export` 全绿 |
| PV2 | `npm run check`（仓库根，主检出） | 十道合同脚本全绿（本变更预期零机器合同改动） |
| PV3 | `cd clients/app && node e2e/run-e2e.mjs`（主检出） | 既有检查集（含新增与更新的用例）全部通过；输出含四处缺口的断言证据 |
| PV4 | 端到端验收（见 Main E2E） | 真实浏览器 + 真实 Daemon（同源托管）完成一次真实配对并建立已认证连接、取得快照 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 子协议值是否真取自机器登记（而不是抄一份）；假 socket 是否让「未声明」可被证伪；必需 feature 判定的阻断路径是否可区分于普通断网；端点改动是否与 wire 权威一致 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 主机材料是否真的落盘且在后续握手被使用；缺材料是否真的不进已配对；配对秘密是否可能进日志/localStorage/地址历史；输入校验是否可能被绕过（不得尽力解析） | 同上 |
| WP3 | reviewer | 路由模式的核实是否有证据而不是猜；片段内容是否可能被当作导航指令；清除片段后是否仍在配对页；其余页面可达性是否有断言 | 同上 |
| WP4 | reviewer | 断言是否可证伪（每处缺口都能在实现错误时变红）；是否仍存在「预置已配对」掩盖配对路径的用例；新增用例是否夹带实现代码；对假主机的用例是否被误当作端到端验收 | 同上 |
| WP5 | reviewer | 文档是否写成「已落地」而非「计划」；是否误改与本变更无关的历史内容；对内联样式与 CSP 的冲突是否如实登记 | 同上 |

## Main E2E

```yaml
mode: required
reason: 本变更的全部价值就是「浏览器里真的能用」——它是 PWA 端到端可用的收口，必须用 browser prelude 对**真实 Daemon**（经同源托管）断言可观察行为。对假主机的既有 e2e 保留为回归，但不作为本变更的验收依据。
basis: docs/WORKFLOW.md §7 要求按被测面选 prelude（Web/PWA 用 browser prelude，默认开），且「E2E 通过的标准是断言了可观察行为；只启动成功、退出码为零、截图都不能算通过」。
alternative_checks: []
```

验收清单（WP4 与主 Agent 必须给出实测输出）：

1. 浏览器从 Daemon 托管的来源加载页面（`sync-ui-hosting` 的根目录入口）。
2. 进入配对页，输入一次真实配对二维码内容，走完认领 → 等待本机确认 → 已批准。
3. 已配对主机记录含主机标识与公钥（断言可从共享状态或诊断入口观察）。
4. 客户端建立连接：出站 URL 的路径为权威端点，升级请求携带约定子协议（这两条必须能从实际连接上观察到，而不是断言源码字符串）。
5. 连接进入在线，且首次同步完成后目录页呈现本机已登记目录与其会话。
6. 断开与重连：断开后按已确认游标恢复，不重复呈现已呈现过的内容。
7. **内联样式与 CSP 冲突**：若样式异常，必须把现象与处理方式写进报告；不得在 Rust 侧放宽 CSP 掩盖。

## Risks

- **[WP1/WP2/WP3 同文件串行]** 三者都改 `composition.ts` → 已串行并登记重跑；`git apply --check` 失败时用后合入者的 patch 重建，不手工改代码。
- **[前端工程独立门禁]** `clients/app` 不在根 `npm run check` 链上，漏跑会让前端检查旁路 → PV1 与 PV3 显式在 `clients/app` 下执行；CI 的 `前端检查（clients/app）` job 是必需检查，PD 前必须绿。
- **[真实端到端的环境依赖]** 需要同源 https 部署与真实 Daemon 在验收机器上可跑 → 若不可得，按 `docs/WORKFLOW.md` 如实登记为未验证并**阻断本变更**（它是收口变更，主验收就是这条）；MUST NOT 以假主机用例代替。
- **[路由模式核实]** D4 的取舍依赖「Expo Router 实际用正常路径」这一事实 → 核实放在 WP3 的第一步；若实际是片段路由，需改路由配置（代价已知）。
- **[既有 e2e 登记的更新]** 保留「预置已配对」会让配对路径永远不被覆盖 → WP4 必须新增从「未配对」出发的用例（检视项已列入）。
- **[隔离区限制]** WP4 作者在隔离区无 `node_modules` 与浏览器 → 交付是用例代码 + 隔离区能做的静态核对；真实运行由主 Agent 按 PV3/PV4 执行；报告须写明。
