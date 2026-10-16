<!-- 执行安排：把 specs 的行为要求与 design 的技术方案转成可派发的并行工作包。
     plan 只写“谁在哪个隔离工作区做什么、按什么顺序合入、怎么验”，不重复 specs/design 的内容。 -->

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
| WP1 | 让「剥 verbatim 前缀」这份规则可被 `agent-host` 复用（`local-agent-host` 新增要求「出站 ACP 路径参数的形式」的规则归属） | none | wp-coder | `crates/core/src/` | patch：`strip_verbatim` 的可见性与文档注释；报告说明可见性边界与为何仍只有一处实现 | `cargo test -p core` |
| WP2 | 出站 `session/new` 与 `session/resume` 的 `cwd` 不带 verbatim 前缀（`local-agent-host` 的「出站 ACP 路径参数的形式」与「进程不在时的会话恢复入口与执行」） | code:WP1 | wp-coder | `crates/agent-host/src/, docs/MODULE_ARCHITECTURE.md` | patch：两个参数构造函数应用转换；`MODULE_ARCHITECTURE.md` §4.5 登记该约束 | `cargo test -p agent-host` |
| WP3 | 用假 Agent 断言出站参数形式（含 UNC 还原与非 Windows 直通） | code:WP2 | wp-tester | `crates/agent-host/tests/` | patch：断言 `session/new` 与 `session/resume` **实际收到**的 `cwd` 形态（普通剥离、UNC 还原、无前缀直通） | `cargo test -p agent-host` |
| WP4 | 在真实 Daemon + 真实 `agent-host` + 假 Agent 的通道上同时断言「会话行里的目录列仍是持久化取值」与「Agent 收到的是剥过的 `cwd`」（`workspace-resolution` 的「持久化取值与解析结果不被改写」的场景） | code:WP2 | wp-tester | `crates/app/tests/` | patch：app 层用例读真实 SQLite 的会话行 + 读假 Agent 的 `--dump-request-params` 记录，两侧在同一用例内对拍 | `cargo test -p app` |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 无重叠 | `cargo test -p core` |
| 2 | WP2 | 依赖 WP1 的可见性 | `cargo test -p agent-host` |
| 3 | WP3 | 与 WP2 同属 `crates/agent-host/`（`src/` 与 `tests/`），需在 WP2 之后合入 | `cargo test -p agent-host` |
| 4 | WP4 | 依赖 WP2 的实现；写范围 `crates/app/tests/` 与 WP3 的 `crates/agent-host/tests/` **不重叠**，因此两者只受「WP2 已合入」这一条代码依赖约束，可并行派发 | `cargo test -p app` |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cargo test -p core -p agent-host -p app`（主检出根） | 全部通过；无新增 warning。`-p agent-host` 与 `-p app` 必须写在**同一条**命令里：WP4 的用例要用的 fake ACP Agent 二进制由 `agent-host` 提供，单独跑 `-p app` 时它可能还没被构建（该用例会在缺失时明确失败，不静默跳过） |
| PV2 | `npm run check`（主检出根） | 十项合同门禁全绿（本次改动不涉合同资产，作为回归） |
| PV3 | `cargo clippy --workspace --all-targets -- -D warnings`（主检出根） | 无 warning |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 可见性是否过宽（是否把整条路径规范化口径都暴露出去）、是否真的只有一处实现 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 是否只改出站参数而未污染持久化取值与解析结果；UNC 形态是否与既有实现一致 | 无 CRITICAL/MAJOR 未闭环项 |
| WP3 | reviewer | 断言是否可证伪（假 Agent 记录实际收到的字节，而不是断言内部函数） | 无 CRITICAL/MAJOR 未闭环项 |
| WP4 | reviewer | 断言是否可证伪（读真实 SQLite 落盘行与假 Agent **实际收到**的字节，而不是内部函数）；「Windows 上持久化取值确实带 verbatim 前缀」这个前提是否真的成立（`canonicalize` 的结果），否则用例在非 Windows 上会退化成同义反复 | 无 CRITICAL/MAJOR 未闭环项 |

## Main E2E

```yaml
mode: required
reason: 本变更的验收判据是「经 Daemon 用真实 Agent 真的建成一个会话」——这条链路从跑通过，单元测试无法覆盖它的最后一公里（进程启动、能力协商、会话建立）。
basis: docs/WORKFLOW.md §7 要求按被测面选 prelude；本判据落在「浏览器 → Daemon → 真实 Agent 进程」这条链上，用 browser prelude 驱动真实页面并断言可观察行为；只启动成功、退出码为零、截图都不算通过。
alternative_checks:
  - 假 Agent 断言出站 `cwd` 字节（WP3 的单元/集成断言）作为前置证据，但它不替代真实 Agent 的端到端判据
```

## Risks

- **真实 Agent 仍可能撞上第二个坑**：主判据若仍失败，必须先定位（能力协商 / `session/update` 处理 / `configOptions` 形状）再判定本变更是否完成，不得把"剥了前缀"当作完成。
- **测试与实现的边界**：WP2 与 WP3 同属 `agent-host`，串行合入；WP3 的断言必须基于实际收到的字节，否则无法证伪。
