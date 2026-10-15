<!-- 执行安排：把 specs 的行为要求与 design 的技术方案转成可派发的工作包。
     执行原语固定为 omp：batch task 派发、patch 模式隔离、`<id>.patch` 交付、主 Agent 串行 git apply。 -->

# Plan

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出多个工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）。工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；工作包不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`，每次合入后跑主检出 Project Verify。
- 验证：隔离工作区内**没有** `target/` 与 `node_modules/`，因此仓库级检查只在主检出执行。
- 检视：每个工作包由一个**独立** spawn（不与作者同实例）读 patch 与 `agent://<id>` 后给出结构化结论。
- **现状（apply 阶段必读）**：WP1 的实现与判别力用例**已合入主检出**（排空不再等待发出 `daemon.stop` 的那条连接：`0c7a9a3` / #97 是首次修复，`c2e212e` / #102 把判据改成排空阶段实测耗时 `drain_ms`）；本变更的 proposal/specs/tasks 立案于 `7057e01` 之前的同批文档提交。apply 时：①1.1 按证据勾选；②1.2/1.3 由本计划的主 Agent 侧探针与连跑取证；③WP1–WP4 是本批**真正要做的**（文档措辞、CLI 注释、协议条文、`drain_timeout` 正向断言）；④3.3 的判定与登记写进 design.md。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 文档措辞同步（任务 1.4）：两处「关闭顺序」散文改成「排空**其余**在途连接，发出该 `daemon.stop` 的连接不计入」 | none | wp-coder | `docs/MODULE_ARCHITECTURE.md`, `docs/CORE_PORTS_AND_STORAGE.md` | patch + 两处「改前/改后」对照（带行号） | 主检出 `npm run check`（含 `check:docs` 与 `check:drift`） |
| WP2 | CLI 注释如实化 + 冗余判定（任务 3.1）：`release_runtime` 文档块与 `daemon_stop` 尾部注释改成不依赖「排空会等满宽限」的表述；判定 `release_runtime()` 是否已冗余并**说明而不删** | none | wp-coder | `crates/app/src/cli.rs` | patch + 「改前/改后」对照 + 冗余判定（区分事实与推断） | 主检出 `cargo test -p app` |
| WP3 | 协议条文（任务 3.2）：`LOCAL_ADMIN_PROTOCOL.md` §5.2 补上「`daemon.stop` 响应写回后由服务端结束该连接」，并与「关闭期新请求以 `local.unavailable` 失败」写清区别 | none | wp-coder | `docs/LOCAL_ADMIN_PROTOCOL.md` | patch + 新增句（带行号） | 主检出 `npm run check` |
| WP4 | 规格场景覆盖缺口（任务 3.4）：在 `stop_cleans_up_even_when_the_drain_budget_is_spent` 里补一条**正向**断言「宽限耗尽时日志里出现 `daemon.drain_timeout`」 | none | wp-tester | `crates/app/tests/daemon_lifecycle.rs` | patch + 新断言原文 + 为何只在“预算占满”形态下成立 | 主检出 `cargo test -p app --test daemon_lifecycle` |
| WP5 | 证据与登记（任务 1.2 / 1.3 / 3.3，主 Agent 侧）：回退探针取判别力证据、`daemon_lifecycle` 连跑 ≥10 次、`the_periodic_task_runs_again_after_one_full_cycle` 竞态的判定与修法建议（另案登记） | code:WP1（1.2 的探针回退的是已合入的实现） | 主 Agent（无 spawn） | `openspec/changes/daemon-shutdown-drain/design.md`, `openspec/changes/daemon-shutdown-drain/plan.md`, `openspec/changes/daemon-shutdown-drain/tasks.md` | 变更产物 + 实测记录 | 主检出：回退探针变红→还原全绿；连跑输出；`openspec validate daemon-shutdown-drain --strict` |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 写范围仅两处 `docs/`，与其余包不重叠 | `npm run check` |
| 2 | WP2 | 写范围仅 `crates/app/src/cli.rs` | `cargo test -p app` |
| 3 | WP3 | 写范围仅 `docs/LOCAL_ADMIN_PROTOCOL.md` | `npm run check` |
| 4 | WP4 | 写范围仅 `crates/app/tests/daemon_lifecycle.rs`；断言的对象是已合入的排空与超时分支 | `cargo test -p app --test daemon_lifecycle` |
| 5 | WP5 | 写范围仅变更目录的产物；记录的是前四包与探针的实测结论，必须最后写 | `openspec validate daemon-shutdown-drain --strict` |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cargo test -p app --test daemon_lifecycle`（主检出） | 12 项用例全绿（含 WP4 新增的正向 `drain_timeout` 断言） |
| PV2 | `cargo test --locked -p app`（主检出） | 整个 `app` crate 全绿（daemon、CLI、组合根） |
| PV3 | `cargo clippy --locked -p app --all-targets -- -D warnings` 与 `cargo fmt --all -- --check`（主检出） | 无告警、无格式差异 |
| PV4 | `npm run check`（主检出） | 十道合同脚本全绿（本批改了 `docs/`，`check:docs` 与 `check:drift` 必须仍绿） |
| PV5 | 回退探针 + 连跑 ≥10（主检出，临时改动后恢复） | ① 把「关闭请求者」重新计入排空后，`stop_does_not_wait_for_the_full_grace_without_other_clients` 必须变红；② `daemon_lifecycle` 连跑 ≥10 次全绿，且**没有**放宽任何既有断言 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 改动是否只订正措辞、没有把「宽限仍存在」删掉、没有改动章节结构或破坏 §引用；是否与 delta spec 的措辞一致 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 注释是否改成与实现一致的表述（不得仍暗示「只能等满宽限」）；`release_runtime` 的冗余判定是否区分了事实与推断；是否**没有**删函数/删调用/改逻辑 | 同上 |
| WP3 | reviewer | 新增条文是否把「关闭期新请求失败」与「请求者连接被服务端结束」两件事写清且不互相覆盖；是否与 §5.2 既有条目风格一致 | 同上 |
| WP4 | reviewer | 新断言是否有判别力（超时分支真的命中时才会出现该事件）；是否放宽或删改了既有断言；是否引入脆弱的日志解析 | 同上 |
| WP5 | reviewer | 勾选是否都有可读证据（1.1 实现/用例、1.2 探针双方输出、1.3 连跑统计）；3.3 的判定是否给出证据并明确标为另案；是否把「已实现」写成事实而实际只是计划 | 同上 |

## Main E2E

```yaml
mode: not-applicable
reason: 本变更没有浏览器或桌面 UI 入口——被观察的是 Daemon 关闭序列的时序与日志（`daemon.drain` 的 `drain_ms`、`daemon.drain_timeout`、连接结束原因），唯一可运行的入口是本地管理通道上的 `daemon.stop`。
basis: docs/WORKFLOW.md §7 要求 E2E 断言可观察行为；本变更的真实入口已经由 `crates/app/tests/daemon_lifecycle.rs` 覆盖——它启动**真实 Daemon 进程**、经**真实本地管理通道**发出 `daemon.stop`、读取真实结构化日志与运行记录，没有第二层可叠。
alternative_checks:
  - PV1/PV2：真实进程 + 真实通道上的排空行为（请求者不计入、其它在途连接仍被等待、宽限耗尽仍关闭并留记录）。
  - PV5①：回退探针证明该判定有判别力（把请求者重新计入排空即变红）。
  - PV5②：连跑 ≥10 次证明判定不依赖调度时序；PV4 确认文档改动没有破坏链与漂移门禁。
```

## Risks

- **[实现已在主检出]** 隔离工作区的基线是整个工作目录的副本，已合入的修复会被带进工作包 → 派发时写明「不要重做实现；只做文档/注释/用例补强」。
- **[本变更的用例时序敏感]** 排空判据依赖“宽限”这一时间窗（`MIN_DRAIN_MS = 200`、`grace = 3000`）→ WP4 只允许**补**正向日志断言，不得改动窗口与阈值；PV5② 的连跑在**没有任何其它 cargo 进程并发**时执行，否则测量本身不可信。
- **[文档面门禁]** WP1/WP3 改的是权威文档的散文 → 只订正事实、不新增规范性条款（行为契约在 delta spec 里），合入后必须跑 `npm run check`。
- **[隔离区跑不了 `cargo test`/`npm`]** WP1–WP4 的作者只能交付 patch 与自述未验证点 → 真实运行由主 Agent 按 PV1–PV4 执行；报告不得声称已在隔离区通过。
