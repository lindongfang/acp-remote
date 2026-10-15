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
- **现状（apply 阶段必读）**：WP1–WP5 的实现与用例**已合入主检出** —— 实现落在 `aa162e3`（#103，「饱和判定改为可恢复的迟滞，饱和期心跳不再停」），用例的填充口径修正落在 `4426264`（#111）；本变更的 proposal/specs/tasks 是在 `fcb564e`（#98，一次修复提交「形状类拒绝不再让连接退化为只出不进的哑管道」）里**顺带立案**的，同批立案了变更⑥⑦⑧（本变更即⑥；⑦=`node-link-tick-scheduling`、⑧=`node-link-malformed-sequence`） —— 它不是一次纯立案提交，按标题检索时注意这点。本计划保留完整的工作包边界与验证策略；apply 时只做「回退探针取判别力证据 + 按证据勾选 + 补 design/plan/登记」，**不得重做实现**（唯一的例外见下）。
- **本批补的一处实现侧缺口**：任务 1.4 要求「持续饱和（必须断开）」那条用例也断言其它连接不受影响，而该用例此前只有一条连接、没有跨连接断言（2026-10-16 检视 finding）。本批按 WP4 的写范围补上「对照组连接 + 断开后仍可用 + 已登记连接数回到 1」，其余实现不动。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 饱和判定改为低水位迟滞（任务 1.1 / 1.5）：`consumed()` 在同一临界区内先算结论再清除；补低水位常量与其文档；模块文档写明清除点与「可恢复」的口径 | none | wp-coder | `crates/server/src/node_link/conn/registry.rs` | patch + 「进入/退出判据的落点、低水位现算的理由、清除点为何必须在同一临界区」 | 主检出 `cargo test -p server --lib node_link::conn::tests` |
| WP2 | 饱和期心跳照发（任务 1.2 / 1.5）：Tick 分支改成心跳与看门狗各自独立判定；模块文档写明饱和不是静默 | code:WP1（判据来自 WP1 的两个结论） | wp-coder | `crates/server/src/node_link/conn/session.rs` | patch + 「`tick_action` 的三路判定、ping 被高水位拒绝时的口径、tick 调度口径」 | 同上 |
| WP3 | 受害面改活判定（任务 1.3）：`catalog.rs` 的批间门与 `resource.rs` 的 chunk 门每批现算 `saturated()`，恢复后重新读批次 | code:WP1 | wp-coder | `crates/server/src/node_link/catalog.rs`, `crates/server/src/node_link/resource.rs` | patch + 「两个门的判定位置、半途中止后由订阅补齐的语义」 | 同上 |
| WP4 | 用例面（任务 1.1 / 1.2 / 1.3 / 1.4）：低水位清除、饱和期心跳、饱和恢复后重读批次、瞬时突发不断开、持续饱和必断开（后两条各断言其他连接不受影响） | code:WP1, WP2, WP3 | wp-tester | `crates/server/src/node_link/conn/tests.rs` | patch + 每个段落「回退哪一处修法会变红」的判别力说明 | 同上 |
| WP5 | delta spec（任务 2.1）：`node-link-owner-server` 的「心跳、超时与慢连接隔离」补上可恢复判定、饱和期心跳、高/低水位常量与三个场景 | none | wp-coder | `openspec/changes/node-link-saturation-recovery/specs/node-link-owner-server/spec.md` | patch + 「需求文本 ↔ 实现 ↔ 用例」的三方对应 | 主检出 `openspec validate node-link-saturation-recovery --strict` 与 `npm run check:openspec` |
| WP6 | 相邻项 3.1 的判定与登记（任务 3.1）：写路径无期限的真实影响 + 修法建议 + 实测证据，登记进 design.md（**不改代码**） | none | scout | `openspec/changes/node-link-saturation-recovery/design.md` | 登记段落 + 「代码依据（文件:行）+ 实测观测 + 量级口径」 | 主检出的实测观测可复现（临时探针，验证后删除） |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP5 | 写范围仅变更目录内的 delta spec，与代码包不重叠；行为契约先于实现 | `openspec validate node-link-saturation-recovery --strict` |
| 2 | WP1 | 写范围仅 `registry.rs` | `cargo test -p server --lib node_link::conn::tests` |
| 3 | WP2 | 写范围仅 `session.rs`（与 WP1 不重叠），但消费 WP1 的两个结论 | 重跑 `cargo test -p server --lib node_link::conn::tests` |
| 4 | WP3 | 写范围仅 `catalog.rs`/`resource.rs`，与其余包不重叠 | 同上 |
| 5 | WP4 | 写范围仅 `tests.rs`，但断言的对象是 WP1–WP3 的行为，必须最后合入 | 同上 |
| 6 | WP6 | 写范围仅 `design.md`（规划产物，不与代码包重叠）；登记的是实测结论，必须最后写 | 不适用（无代码改动） |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cargo test -p server --lib node_link::conn::tests`（主检出） | conn 用例全绿，含低水位清除、饱和期心跳、恢复后重读批次、瞬时突发与持续饱和两侧 |
| PV2 | `cargo test --locked -p server`（主检出） | 整个 `server` crate 全绿 |
| PV3 | `cargo clippy --locked -p server --all-targets -- -D warnings` 与 `cargo fmt --all -- --check`（主检出） | 无告警、无格式差异 |
| PV4 | `npm run check`（主检出） | 十道合同脚本全绿（本变更预期零机器合同改动） |
| PV5 | 回退探针（主检出，临时改动后恢复） | 把 WP1 的清除判据、WP2 的心跳独立判定、WP1+WP2 的看门狗判据分别回退时，对应用例必须变红 —— 这是这些用例**判别力**的证据，不是「跑绿就算过」 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 清除是否在同一临界区内完成（出锁后再判就是原缺陷）；低水位是否由生效限额现算、是否抬到 ≥1；「刚被拒过」是否仍算饱和；模块文档是否与实现一致 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 心跳与看门狗是否**各自独立**判定（`else if` 形状是否真的被消除）；ping 被高水位拒绝时是否不误判为「心跳停摆」；tick 的 deadline 是否跨轮保留且排在出站之前 | 同上 |
| WP3 | reviewer | 两个门是否每批现算（不得锁存）；半途中止是否仍由订阅补齐；日志/计数是否不因恢复而失真 | 同上 |
| WP4 | reviewer | 每条用例是否真能区分「修法与回退」；瞬时突发与持续饱和是否共享同一个可调参数导致断言互相抵消；是否夹带实现代码；是否只证明「没抛错」 | 同上 |
| WP5 | reviewer | 需求是否只写可观察行为；三个场景是否与实现、用例一一对应；高/低水位常量是否写进要求而没有写死具体数值 | 同上 |
| WP6 | reviewer | 判定是否有代码依据（文件:行）；实测数字是否与探针形态一致；是否把「另案」写成了「已修」；修法建议是否越界改动了本变更范围 | 同上 |

## Main E2E

```yaml
mode: not-applicable
reason: 本变更没有用户可触达的入口——它是 WSS 连接上的队列水位判定与读入门，可观察面只有「这条连接是否还在收批次、是否被断开」；浏览器/桌面客户端本期不承载 Node Link 连接。
basis: docs/WORKFLOW.md §7 要求 E2E 断言可观察行为；本变更唯一可运行的真实入口是 `crates/server/src/node_link/conn/tests.rs` 的 loopback WSS 连接（真实 listener + 真实握手 + 真实帧 + 真实有界队列），没有第二层可叠。
alternative_checks:
  - PV1：在真实连接上制造持续饱和（对端不消费）与瞬时突发（对端随后排空），断言前者在宽限后被断开、后者不被断开、饱和期心跳照发、恢复后重新收到全部批次。
  - PV5：把修法回退（不清除/心跳短路/看门狗失效）时对应用例必须变红 —— 证明断言有判别力。
  - PV4：合同门禁确认本变更没有触碰 wire schema、fixture 与错误码词表。
```

## Risks

- **[实现已在主检出]** 隔离工作区的基线是整个工作目录的副本，已合入的修复会被带进工作包 → 派发时必须写明「只按证据勾选与补产物，不重做实现」。
- **[饱和用例的时序敏感]** 本模块的饱和用例曾因内核吸走字节而抖动（`#111` 用 32 KiB 填充修过）→ 任何新增/改动这些用例的包必须复用既有填充口径，不得引入依赖时序的新断言；回退探针只在主检出跑一次，不写进 CI。
- **[隔离区跑不了 `cargo test`]** WP1–WP4 的作者无法在隔离区运行用例 → 交付是代码/用例 + 判别力说明，真实运行由主 Agent 按 PV1/PV5 执行；报告不得声称已在隔离区通过。
- **[WP6 的实测依赖临时探针]** 探针不得入库（会引入依赖内核缓冲大小的脆弱断言）→ 证据写在 design.md 的可复现描述里，探针本身在验证后删除。
