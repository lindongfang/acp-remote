<!-- 执行安排：把 requirements 与 design 转成可派发的工作包。
     执行原语固定为 omp：batch task 派发、patch 模式隔离、`<id>.patch` 交付、主 Agent 串行 git apply。 -->

# Plan

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出多个工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）。工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；工作包不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`，每次合入后跑主检出 Project Verify。
- 验证：隔离工作区内**没有** `target/` 与 `node_modules/`，因此仓库级检查只在主检出执行。
- 检视：每个工作包由一个**独立** spawn（不与作者同实例）读 patch 与 `agent://<id>` 后给出结构化结论。
- **现状（apply 阶段必读）**：WP1–WP3 的实现、用例与 `.openspec.yaml` 的 `skip_specs: true` **已在主检出** —— tick 调度（`sleep_until(next_tick)` + `biased` 里 tick 先于 outbound）与两条「出站队列持续非空」用例随 `0a37b23`（#100，「会话 tick 不再被持续就绪的出站队列饿死」）落地；相邻变更⑥（`node-link-saturation-recovery`）的饱和判据与看门狗实现随 `aa162e3`（#103）落地。本计划保留完整的工作包边界与验证策略；apply 时只做「回退探针取判别力证据 + 按证据勾选 + 补 design/plan」，**不得重做实现**。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | tick 调度口径（任务 1.1 / 1.3）：`sleep_until(next_tick)` 跨轮保留、tick 分支排在出站之前、阈值来源与模块文档写明「为何缺一不可」 | none | wp-coder | `crates/server/src/node_link/conn/session.rs` | patch + 「deadline 的存放位置与下一拍起算点、`biased` 分支顺序、文档段落与实现的逐句对应」 | 主检出 `cargo test -p server --lib node_link::conn::tests` |
| WP2 | 用例面（任务 1.2）：「出站队列持续非空」时 tick 驱动的行为仍发生 —— 心跳照发 + 入站静默仍按窗口关闭 | code:WP1 | wp-tester | `crates/server/src/node_link/conn/tests.rs` | patch + 每条用例「回退哪一处调度会变红」的判别力说明 | 同上 |
| WP3 | 规格决策的落地（任务 2.1）：`skip_specs: true` 与「不改规格」的理由 | none | wp-coder | `openspec/changes/node-link-tick-scheduling/.openspec.yaml` | patch + 「为何本变更不新增/修改规格要求（实现既有要求）」 | 主检出 `openspec validate node-link-tick-scheduling --strict` 与 `npm run check:openspec` |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP3 | 写范围仅 `.openspec.yaml`，与代码包不重叠；规格决策先落地，`apply` 门禁才有正确的前提 | `npm run check:openspec` |
| 2 | WP1 | 写范围仅 `session.rs` | `cargo test -p server --lib node_link::conn::tests` |
| 3 | WP2 | 写范围仅 `tests.rs`（与 WP1 不重叠），但断言的对象是 WP1 的调度行为 | 重跑 `cargo test -p server --lib node_link::conn::tests` |

注：变更⑥（`node-link-saturation-recovery`）也会写 `crates/server/src/node_link/conn/tests.rs`。两条变更的**用例互不重叠**，但同一个文件先后被两个 patch 改过，因此后合入者必须重跑聚焦用例（这条已在两侧的 Merge Order 里声明）。

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cargo test -p server --lib node_link::conn::tests`（主检出） | conn 用例全绿，含「出站持续非空」的两种 tick 行为 |
| PV2 | `cargo test --locked -p server`（主检出） | 整个 `server` crate 全绿 |
| PV3 | `cargo clippy --locked -p server --all-targets -- -D warnings` 与 `cargo fmt --all -- --check`（主检出） | 无告警、无格式差异 |
| PV4 | `npm run check`（主检出） | 十道合同脚本全绿（本变更预期零机器合同改动） |
| PV5 | 回退探针（主检出，临时改动后恢复） | 把调度回退成「tick 排在出站之后 + 每轮重建 `sleep(SESSION_TICK)`」时，WP2 的用例（至少「心跳仍发出」那条）必须变红 —— 这是判别力证据 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | deadline 是否真的跨轮保留（不得轮内重建）；tick 是否真的排在出站之前（`biased` 声明顺序）；下一拍起算点是否取 tick 发生时刻；模块文档是否把「缺一不可」写清；是否有其它分支挤占 tick 的可能 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 用例是否真的构造出「出站队列持续非空」（压力源有下界断言）；断言的是 tick 行为而非副作用；是否会因内核吸走字节而抖动；是否只证明「没抛错」 | 同上 |
| WP3 | reviewer | `skip_specs: true` 的理由是否成立（本变更确实不改任何要求的语义）；是否借 `skip_specs` 绕过本该写入规格的行为变化 | 同上 |

## Main E2E

```yaml
mode: not-applicable
reason: 本变更没有用户可触达的入口——它是会话循环内部的轮询调度，可观察面只有「繁忙连接上心跳/静默/看门狗是否仍按期发生」；浏览器/桌面客户端本期不承载 Node Link 连接。
basis: docs/WORKFLOW.md §7 要求 E2E 断言可观察行为；本变更唯一可运行的真实入口是 `crates/server/src/node_link/conn/tests.rs` 的 loopback WSS 连接（真实 listener + 真实握手 + 真实有界队列），没有第二层可叠。
alternative_checks:
  - PV1：在出站队列持续非空的真实连接上，断言心跳仍按协商间隔发出、入站静默仍按窗口推进到 4408 关闭。
  - PV5：把调度回退（tick 排回最后或每轮重建）时用例必须变红 —— 证明断言有判别力。
  - PV4：合同门禁确认本变更没有触碰 wire schema、fixture 与错误码词表。
```

## Risks

- **[实现已在主检出]** 隔离工作区的基线是整个工作目录的副本，已合入的修复会被带进工作包 → 派发时必须写明「只按证据勾选与补产物，不重做实现」。
- **[与变更⑥ 同文件]** 两条变更都改 `conn/tests.rs` → 合入顺序无冲突（用例不同），但后合入者要重跑聚焦用例；若 `git apply --check` 失败，用后合入者自己的 patch 重建，不手工改代码。
- **[用例时序敏感]** 「持续非空 / 持续饱和」这类用例依赖压力源与内核缓冲行为 → 复用既有 32 KiB 填充口径，不引入依赖时序的新断言；回退探针只在主检出跑一次，不入 CI。
- **[隔离区跑不了 `cargo test`]** WP1–WP2 的作者无法在隔离区运行用例 → 交付是代码/用例 + 判别力说明，真实运行由主 Agent 按 PV1/PV5 执行；报告不得声称已在隔离区通过。
