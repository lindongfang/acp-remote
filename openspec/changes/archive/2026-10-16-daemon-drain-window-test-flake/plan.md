<!-- 执行安排：把 proposal/tasks 的要求转成可派发的工作包。执行原语固定为 omp（batch task、patch 模式隔离、主 Agent 串行 git apply）。 -->

# Plan

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
| WP1 | 把排空窗口用例的判据改成有界预算内的确定性形态（任务 1.2 / 2.1 / 2.2）：保留两个反例方向，不改实现 | none | wp-tester | `crates/app/tests/daemon_lifecycle.rs` | patch + 「改前/改后」对照 + 为什么新判据在旧实现上必绿、在「接入面不停」与「进程已退出」两个反例上必红 | 主检出 `cargo test -p app --test daemon_lifecycle` 连跑 ≥10 次全绿；反例探针见 PV5 |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 写范围仅一个测试文件；本变更无其它代码包 | `cargo test -p app --test daemon_lifecycle`（连跑 ≥10 次） |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cargo test -p app --test daemon_lifecycle`（主检出，连跑 ≥10 次） | 每次 12 passed / 0 failed；无一次红 |
| PV2 | `cargo test --locked -p app`（主检出） | 整个 `app` crate 全绿 |
| PV3 | `cargo clippy --locked -p app --all-targets -- -D warnings` 与 `cargo fmt --all -- --check`（主检出） | 无告警、无格式差异 |
| PV4 | `npm run check`（主检出） | 十道合同脚本全绿（本变更零机器合同改动） |
| PV5 | 反例探针（主检出，临时改动后恢复） | ① 让接入面**永不停止**（临时去掉 `trigger_shutdown`）⇒ 该用例必须红；② 让进程在停止前退出 ⇒ 必须以便于区分的原因红（不能当成通过）。两次实验后都还原并复跑 PV1 |
| PV6 | CI 连跑两次 `合同门禁 + Rust 检查` | 两次全绿（本地连跑不能替代 CI 慢跑器的证据，见任务 2.3） |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 新判据是否**确定性**（不依赖「日志一出现」的采样时机）；两个反例方向是否真的还在（接入面不停 ⇒ 红；进程退出 ⇒ 明确失败）；是否误删/放宽了既有断言；是否改到了实现；同文件其它时间窗口断言的核查结论是否属实 | 无 CRITICAL/MAJOR 未闭环项 |

## Main E2E

```yaml
mode: not-applicable
reason: 本变更只改一处测试判据，没有用户可触达的界面或运行入口；被观察的对象是 Daemon 关闭序列与监听面行为，入口就是该集成测试（真实 Daemon 进程 + 真实本地管理通道 + 真实 loopback HTTP 探针）。
basis: docs/WORKFLOW.md §7 要求 E2E 断言可观察行为；本变更的可观察行为已由 `crates/app/tests/daemon_lifecycle.rs` 在真实进程上断言，没有第二层可叠。
alternative_checks:
  - PV1：真实 Daemon 上连跑 ≥10 次，证明判据不再依赖采样时机。
  - PV5：两个反例探针证明判别力仍在（不停 ⇒ 红；进程退出 ⇒ 明确红）。
  - PV6：CI 慢跑器上连跑两次，证明 flake 已消失（本地快机不能代替这条）。
```

## Risks

- **[隔离区跑不了 `cargo test`]** WP1 的作者无法在隔离区验证 ⇒ 交付是「用例代码 + 自证说明」，真实运行由主 Agent 按 PV1/PV5 执行；报告不得声称已在隔离区通过。
- **[反例探针要动实现]** PV5 需要临时改 `crates/app/src/daemon.rs`（去掉 `trigger_shutdown`）⇒ 实验后必须 `git checkout` 还原并复跑 PV1，实验本身不入库。
- **[探针成本必须受控]** 已真实发生并已修（design D5）：`probe_http` 不设 `connect` 超时时，一次失败探针约 2 秒 ⇒ 「有界预算」失效、判据假红（本机 PV1 连跑 10 次全红，而事件的 `ts` 证明接入任务 1 毫秒就停了）⇒ 修复后必须重跑 PV1 与 PV5。
