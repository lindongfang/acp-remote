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
- **现状（apply 阶段必读）**：WP1 与 WP3 已随 `fcb564e`（2026-10-14，PR #98）合入主检出；WP2 的 1.2 / 1.3 同批合入。本计划保留完整的工作包边界与验证策略，apply 时只补齐 tasks.md 里仍未勾选的 1.5，**不得重做 1.1–1.4**。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 入站序号推进口径（delta spec「每次消息级拒绝都推进入站期望序号」的实现侧；任务 1.1 / 1.4）：形状类与连接字段禁用类拒绝照常推进，保留「读不出序号则不推进」一支，模块文档写明口径与例外 | none | wp-coder | `crates/server/src/node_link/conn/wire.rs` | patch + 「每个拒绝分支的记账返回值、四项推进条件的落点、模块文档与规格例外条款的对应」 | 主检出 `cargo test -p server --lib node_link::conn::tests` |
| WP2 | 用例面（任务 1.2 / 1.3 / 1.5）：改掉用例⑤的错误期望、补形状错误回归用例、补 ≥128 深度的「已知残留」用例 | code:WP1（断言的对象是 WP1 的记账口径） | wp-tester | `crates/server/src/node_link/conn/tests.rs` | patch + 每个段落的判别力说明（回退 WP1 的哪一支会让哪一段变红） | 主检出 `cargo test -p server --lib node_link::conn::tests` |
| WP3 | delta spec（任务 2.1）：`node-link-owner-server` 新增「每次消息级拒绝都推进入站期望序号」要求与两个场景 | none | wp-coder | `openspec/changes/node-link-malformed-sequence/specs/node-link-owner-server/spec.md` | patch + 「需求文本 ↔ `wire.rs` 文档注释 ↔ 用例」的三方对应 | 主检出 `openspec validate node-link-malformed-sequence --strict` 与 `npm run check:openspec` |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP3 | 写范围仅变更目录内的 delta spec，与代码包不重叠；行为契约先于实现 | `openspec validate node-link-malformed-sequence --strict` |
| 2 | WP1 | 写范围仅 `wire.rs`，与其余包不重叠 | `cargo test -p server --lib node_link::conn::tests` |
| 3 | WP2 | 写范围仅 `tests.rs`（与 WP1 不重叠），但**断言的对象是 WP1 的口径**，必须在其后合入 | 重跑 `cargo test -p server --lib node_link::conn::tests`（WP1 已改判定行为） |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cargo test -p server --lib node_link::conn::tests`（主检出） | conn 的全部用例通过，含形状错误回归用例与 ≥128 已知残留用例 |
| PV2 | `cargo test --locked -p server`（主检出） | 整个 `server` crate 全绿（含接入层、local_admin、node_link 其余用例） |
| PV3 | `cargo clippy --locked -p server --all-targets -- -D warnings` 与 `cargo fmt --all -- --check`（主检出） | 无告警、无格式差异 |
| PV4 | `npm run check`（主检出） | 十道合同脚本全绿（本变更预期零机器合同改动；报红即说明实现偏离了已冻结合同） |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 推进条件是否收窄到「读得出本连接的下一个序号」而不是按拒绝码枚举；认证前与 `Mismatch` / `Required` / 非 JSON 是否仍不推进；合法帧是否为零额外解析；模块文档是否与规格例外条款一致且未声称已消除残留 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 用例⑤是否真的改成正确口径且保留「不关连接」断言；回归用例是否覆盖未登记信封字段与缺 `messageId`；≥128 用例是否写明「已知限制」并引用规格例外条款、是否钉的是解析边界而不是策略；是否存在只证明「没抛错」的断言 | 同上 |
| WP3 | reviewer | 需求是否只写可观察行为（不写函数名/库选型）；两个场景是否与实现和用例一一对应；例外条款是否把已知残留写全且没有替实现开脱 | 同上 |

## Main E2E

```yaml
mode: not-applicable
reason: 本变更没有用户可触达的入口——它是 WSS 连接上的入站记账行为，可观察面只有「对端发一条合法消息之后得到 pong 还是 sequence_invalid」；浏览器/桌面客户端本期不承载 Node Link 连接。
basis: docs/WORKFLOW.md §7 要求 E2E 断言可观察行为；本变更唯一可运行的真实入口是 `crates/server/src/node_link/conn/tests.rs` 的 loopback WSS 连接（真实 listener + 真实握手 + 真实帧），没有第二层可叠。
alternative_checks:
  - PV1：真实 loopback WSS 连接上先制造形状错误、再发一条序号正常递增的合法消息，断言得到 `link.pong` 而不是 `sequence_invalid`（覆盖 delta spec 的两个场景）。
  - 回退实验：把 WP1 的记账口径回退成 `None`，PV1 的相关用例必须变红（证明用例有判别力，不是恒真断言）；随后恢复并复跑。
  - PV4：合同门禁确认本变更没有触碰 wire schema / fixture / 错误码词表。
```

## Risks

- **[1.1–1.4 已合入]** 隔离工作区的基线是整个工作目录的副本，已合入的实现会被带进工作包；若任务描述被误读成「重做」，会出现重复实现 → 派发 WP2 时必须在 task 文本里写明「判定口径已在主检出，只补与改用例」。
- **[≥128 边界数字来自 `serde_json` 的递归上限实现细节]** 127/128 是设计与注释的依据；**实测优先** —— 若 PV1 显示边界不是 127/128，以实测为准改用例与注释，并如实登记，不得为了迁就实现去改规格的例外条款措辞。
- **[隔离区跑不了 `cargo test`]** WP2 作者无法在隔离区运行用例 → 交付是「用例代码 + 判别力说明 + 自报的未验证点」，真实运行由主 Agent 按 PV1 执行；报告不得声称已在隔离区通过。
- **[`tests.rs` 是同一文件的累积用例]** 新增用例不得改动既有段落的行为期望（用例⑤除外，那正是本变更要改的那一处）→ 检视项已列入 WP2。
