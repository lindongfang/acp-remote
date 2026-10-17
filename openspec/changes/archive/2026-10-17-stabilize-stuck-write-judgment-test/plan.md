<!-- 执行安排：把 specs 的行为要求与 design 的技术方案转成可派发的并行工作包。
     plan 只写“谁在哪个隔离工作区做什么、按什么顺序合入、怎么验”，不重复 specs/design 的内容。 -->

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）。工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；工作包不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`，每次合入后跑主检出 Project Verify。
- 验证：隔离工作区内**没有** `target/` 与 `node_modules/`，因此仓库级检查只在主检出执行。
- 检视：每个工作包由一个**独立** spawn（不与作者同实例）读 patch 与 `agent://<id>` 后给出结构化结论。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 两条同构用例（`a_stuck_write_still_lets_the_session_judgments_run`）的余量：注入写窗口 30s → 300s、断言预算 `SILENCE + 11_400ms` → `SILENCE + 59_400ms`，并同步改写记录数字与理由的注释（合法路径构成、本机 ≈3.5s、CI 慢跑器 7.54s、负载翻倍时 16.3s/29.7s、新余量 ≈2× 最差实测且与写窗口保持 5× 间距）。**不改断言所证明的命题**，也不动同文件 `GRACE + 8s` 的两处预算 | none | wp-coder | `crates/server/src/sync/conn/tests.rs`、`crates/server/src/node_link/conn/tests.rs` | patch：两个文件的 diff；报告须给出改前/改后的三个数字与注释全文对照，并说明为何判别力不变 | `cargo test -p server --lib a_stuck_write_still_lets_the_session_judgments_run`（主检出）与 PV2 |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 唯一工作包；写范围只有两个测试文件，与其它变更无重叠 | PV1、PV2、PV3 |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cargo test -p server --lib a_stuck_write_still_lets_the_session_judgments_run`（主检出根） | 两个同构用例各跑一次、通过；随后再跑一次 `cargo test -p server --lib` 确认同文件其它计时用例未受影响 |
| PV2 | `cargo fmt --all -- --check` 与 `cargo clippy --workspace --all-targets -- -D warnings`（主检出根） | 全绿（改动含注释，格式门禁必须逐字通过） |
| PV3 | `npm run check`（主检出根） | 十项合同门禁全绿（本变更不碰合同资产，用来证明没有误改） |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 是否**只**放大余量：命题（「写卡住时仍会被静默判定收尾」）必须原样成立，判别关系「预算 ≪ 写窗口」不得反向；三个数字与注释是否自洽；两个孪生文件是否对称（改了一处漏另一处即判不通过）；有没有顺手改断言条件或压力源 | 无 CRITICAL/MAJOR 未闭环项 |

## Main E2E

```yaml
mode: not-applicable
reason: 本变更只放大既有测试的墙钟余量，不改任何产品行为，也没有可驱动的用户界面或桌面进程。
basis: docs/WORKFLOW.md §7 按被测面选 prelude：被测对象是 `server` crate 的进程内连接用例（真实 TCP + 进程内装配），既非 Web/PWA 面也非桌面/原生面。
alternative_checks:
  - PV1 的两条用例本身即该行为的可观察断言（连接在有界预算内被收尾）；
  - 对照实验（一次性、不留代码）：把预算临时回退到 12s 后在同等负载形态下复跑，用例变红即证明改动只是放大余量、没有削弱命题；
  - PV3 证明合同资产未被误改。
```

## Risks

- **诊断延迟**：失败时最长等 60s 才报错（正常路径仍在 ≈3.5s 结束）。取舍见 design。
- **负载再翻倍**：60s 仍可能不够；但与写窗口保持 5× 间距，届时应按新实测再抬，不会退化成与写窗口打架。
- **本地无法声称已复现 CI 负载**：本机没有两条完整工作流并行的等价负载。缓解：判据改为「结构性自洽 + 已观测数字的 2× 余量」，并把 CI 复跑结果作为交付证据；不要在报告里声称本地已复现 CI 的负载倍数。
- **同族用例照旧**：另外两处 `GRACE + 8s` 未改（无实测抖动）。若它们随后出现同类抖动，应另开变更而不是顺手调大。
