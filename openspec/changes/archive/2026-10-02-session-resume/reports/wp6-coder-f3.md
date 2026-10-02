> coder 子 Agent（`coder-F3`，CR6 修复轮实例）的交付报告。本文件只覆盖 CR6 Round 1 的三条
> 非阻断发现（CR6-F1 / CR6-F2 / CR6-F3）的定点修复，不复述 CR6 的检视结论，也不修改权威
> `plan.md` / `tasks.md` / `verification.md` / `design.md` / `specs/**`。

## Shared Report

| 字段 | 值 |
| --- | --- |
| `task_id` | `2.6`（WP6 DELIVERY）、`3.7`（WP6 REVIEW，本行仅记录待复核） |
| `role` | `coder` |
| `phase` | `fix` |
| `agent_context` | 子 Agent `coder-F3`（CR6 修复轮实现实例），由主 Agent 派发；上下文**不继承** `coder` / `coder-F2`，依据是 `reports/cr6-review.md` 全文 + 本轮派发全文 + 只读契约 |
| `work_package` | `WP6` |
| `stage` | `work-package` |
| `base_revision` | `76ab01126114294e7064e2618eedad0615b2561d`（`feat(node-link): 路由 session.resume 并收口恢复路径的编译涟漪`；开工前 `git rev-parse HEAD` 与派发给定值逐字一致、`git status --porcelain` 为空） |
| `target_revision` | `a7bc596499170a6365e061c70668a203fa2534bb`（短 sha `a7bc596`） |
| `scope` | 只写 `crates/server/src/node_link/command.rs`、`crates/server/src/node_link/command/tests.rs`、`docs/NODE_LINK_PROTOCOL.md`（**仅 §12.7** 的 `session.resume` 结果契约段，diff 只有一个 hunk，`@@ -660,6 +660,8 @@`）。未触碰 `crates/app/`、`crates/core`、`crates/agent-host`、任何 `*-protocol` crate、`plan.md`、`tasks.md`、`verification.md`、`design.md`、`specs/**`，也未触碰 §10 |
| `result` | **PASS**（本轮派发的四条 Check 全部满足）。**独立复核（CR6 Round 2）尚未执行，待新的独立 reviewer 判定** |
| `issues` | 无未决实现问题。3 条待澄清问题见文末，均不阻断本轮判定 |

## 本轮目标：CR6 Round 1 的三条非阻断发现

CR6 结论为 PASS（0 CRITICAL / 0 MAJOR），3 条非阻断发现按派发逐条定点修复，**不扩范围**：
不改通用 mutation 臂（`on_dispatched` 等）、不改任何协议/契约/错误码词表、不改代码行为以外的语义。

### CR6-F1（MINOR，本轮主要修复）——幂等比对漏了会话身份

**问题**：`session.resume` 的 `payload` 恒为 `{}`，语义全部落在 `sessionRef`。路由层的幂等回读
（`command.rs` 的 `Ok(Some(record))` 臂）原本只比「命令名 + 种类 + `payload` 指纹」，因此同一
`(ownerNodeId, accessNodeId, requestId)` 改指**另一个同样已授权**的会话时 `same == true`，会直接把
**首次会话**的终态结果回给调用方，而不是 `nodelink.command.idempotency_conflict`。

**修法（按 reviewer 建议，未自创）**：在 resume 的幂等比对里追加会话身份检查
`record.session() == Some(&target.session)`；不一致时走既有的 `reject_code(...,
"nodelink.command.idempotency_conflict")`。**未新增任何错误码**，`wire_error` 词表、
`compatibility/errors/v1/errors.json` 均未改动。

- 位置：`crates/server/src/node_link/command.rs:1020-1024`（注释：为什么必须比会话身份）、`:1036-1039`（`same` 的第四项）。
- 依据：core 的 `resume_session`（`crates/core/src/broker.rs:1519` 附近的 `OwnedCommit.idempotency`）
  在落幂等行时已把目标会话写进 `IdempotencyRecord.session`，`CommandRecord::session()`
  （`crates/core/src/model/session.rs:1192`）能读回同一信息；core 的 `commit_owned` 比对
  （`broker.rs` 的 `is_none_or(|session| existing.session() == Some(session))`）本来就含会话维度，
  只是路由层在调用 core **之前**就短路返回了首次结果，所以那层保护够不到本路径。
- 契约侧无需改文档：`docs/NODE_LINK_PROTOCOL.md:594`（§12.5）已写「同一
  `(ownerNodeId, accessNodeId, requestId)` 的重复提交必须返回首次结果；`command`、`sessionRef`、
  `expectedVersion` 或解码后的 `payload` 语义不同则返回 `nodelink.command.idempotency_conflict`」。
  本轮是**让实现回到已有契约**，不是新增契约。
- **明确不做**（按派发与 reviewer 的边界）：没有把该检查扩到通用 mutation 臂（`on_dispatched`
  等既有 session-scoped mutation）。那条路径存在同形盲区，属本包范围之外，未触碰。

**新增测试**（`crates/server/src/node_link/command/tests.rs`）：
`session_resume_reusing_a_request_id_for_another_session_is_an_idempotency_conflict`

- 先用 `REQUEST` + `sessionRef = SESSION` 走一次成功恢复，断言终态 `completed` 且结果的
  `remoteSessionRef.sessionId == SESSION`；
- 再种下**第二个同样已授权、同样有持久化恢复数据**的会话 `SESSION_2`（同一 Export、同一
  `grant.remote-work`、同样 attach 成功、同样有 `agent_session_id` + 创建时目录），因此「换一个会话」
  本身合法，冲突只可能来自 requestId 复用；
- 用**同一 `REQUEST`** + `sessionRef.sessionId = SESSION_2` 再次提交，断言：恰好一帧
  `command.rejected`、错误码是 `nodelink.command.idempotency_conflict`、且**没有** `command.accepted`
  也**没有** `command.terminal`（即不得回首次会话的终态结果）。
- 为此新增的测试支撑（都在 `#[cfg(test)]` 内）：`SESSION_2` 常量、`session_summary_of`（把既有
  `session_summary()` 参数化，行为不变）、`seed_recoverable_for`（`seed_recoverable` 委托给它）、
  `Fixture::attach_session`（`Fixture::attach` 委托给它，行为不变）。

**回归证明（红窗口可复现）**：把 `same` 的第四项临时删掉后单跑该用例 →
`FAILED`（`assertion left == right failed: 同一 requestId 改指另一个会话必须被拒，而不是重新接受`，
`tests.rs:2819`）；恢复该行后同一用例 `ok`。日志：`wp6-coder-f3-regression-proof.log`。
这条证明该用例真的在测 CR6-F1，而不是恒真。

### CR6-F2（MINOR）——R31 的路由层证据强度不足

**问题**：用例 `an_unauthorized_session_resume_is_rejected_before_any_local_read` 的唯一副作用断言是
`store.commit_calls()` 不变，无法区分「授权先于本机读取」与「授权先于副作用提交」，而用例名与文档
声称的是前者。

**修法：选 reviewer 的第 ① 条**（给 `CommandStore` 加 `load_recovery` 调用计数，在被拒路径断言为 0），
不选第 ② 条（改用例名）。理由：

1. 第 ① 条不引入「不必要的测试支撑代码」——`CommandStore` 本来就实现了 `load_recovery`
   （`tests.rs` 里按 §3.6 口径返回 `Ok(None)`/记录），加一个 `Arc<AtomicUsize>` 计数与既有的
   `commit_calls: Arc<AtomicUsize>` 完全同形，没有新增替身方法、没有新增 `unreachable!` 面，代价是
   1 个字段 + 1 个自增 + 1 个读取方法。
2. 改了检查之后，用例名与文档声称的「不读取会话行」重新变成**如实**的：核心侧
   `Broker::resume_session` 读会话行的唯一入口就是 `deps.store.load_recovery`
   （`broker.rs` 的 `authorize` 之后第一行），因此 `recovery_read_calls == 0` 是「未读会话行」的
   直接证据，而不是间接推断。选第 ② 条则只能把声称降级，R31 在路由层就彻底没有直接证据了。
3. 该计数不改变任何生产路径：只加在 `#[cfg(test)]` 的替身上。

**如实说明**：修复后该用例有**两条**独立断言，含义分别是——
`recovery_read_calls` 不变 = 授权先于本机读取（`load_recovery` 未被调用），
`commit_calls` 不变 = 授权先于副作用提交。用例的 doc 注释与两条断言的失败信息已按此改写，
不再把两者混成一句「授权先于本机读取与副作用」。真正的区分证据在 core 侧
（`resume_session_authorizes_before_reading_the_session_row`，用 `recovery_read_count`）仍然存在，
两条证据现在同向。

### CR6-F3（SUGGESTION）——`local_terminal` 兜底收窄未登记

**问题**：恢复成功但 `settle_session_resume` 因存储不可用返回 `Ok(false)` 时，Owner 本地发一帧
`completed`（`terminalEventId: null`），而持久记录仍是 `accepted`；`command.status` 重查回 `accepted`，
启动恢复按 `CORE_PORTS_AND_STORAGE.md` §6 第 16 条改写为 `uncertain`。与 `session.create` 同源
（`:649` 的 RV2-WP6-F1 已登记），非本包引入，但 resume 的副作用更重。

**修法（纯文档登记，未改任何代码行为）**：在 `docs/NODE_LINK_PROTOCOL.md` §12.7 的
`session.resume` **结果契约**段补一条与 `:649` 同形的注记，并如实写明 resume 的差异（第一次尝试
**可能已经拉起了 Agent 进程**；重试会命中幂等行而不产生第二次副作用）。

顺带（同一段、同一 hunk）：把 CR6-F1 的跨会话 requestId 复用口径也登记在同一段——理由是这条行为
现在在 wire 上可观察（`command.rejected` + `nodelink.command.idempotency_conflict`），而 §12.7 是
`session.resume` 结果契约的归属段；它复述 §12.5 已有的规定，不新增规定。两行都在 §12.7 内，
**§10 未触碰**（`git diff` 该文件只有一个 hunk，`@@ -660,6 +660,8 @@`）。

## 改动清单（3 个文件）

| 文件 | 改动 |
| --- | --- |
| `crates/server/src/node_link/command.rs` | +5 行注释、+1 行条件（`same` 追加 `record.session() == Some(&target.session)`）。生产路径无新增 `unwrap()`/`expect()`/`panic!` |
| `crates/server/src/node_link/command/tests.rs` | `CommandStore` 增 `recovery_reads: Arc<AtomicUsize>` + `recovery_read_calls()`，`load_recovery` 入口自增；R31 用例拆成两条断言并改写 doc 注释；新增 CR6-F1 用例 1 个；新增测试支撑 `SESSION_2` / `session_summary_of` / `seed_recoverable_for` / `Fixture::attach_session`（均为既有 helper 的参数化委托，既有调用点行为不变） |
| `docs/NODE_LINK_PROTOCOL.md` | §12.7 `session.resume` 结果契约 +2 行（CR6-F3 收窄注记、跨会话 requestId 复用口径） |

`git diff --stat`（提交前逐文件核对）：`command.rs 7 +-` / `tests.rs 112 +++--` /
`NODE_LINK_PROTOCOL.md 2 +`，**无其它文件**。

## Checks

工作目录统一为 `D:/Project/acp-remote-wt/session-resume-wp6`（worktree 根），
目标目录为 worktree 自带的 `target/`（`.gitignore` 覆盖，未被 Git 跟踪），工具链版本取自
`rust-toolchain.toml`，配置为各命令的派发原样参数。

| Check ID | 命令 | 目录 | 退出码 | 日志 |
| --- | --- | --- | --- | --- |
| **PV1** | `cargo fmt --all -- --check` | worktree 根 | **0** | `reports/wp6-coder-f3-PV1-fmt.log`（空输出 = 通过） |
| **PV1** | `cargo clippy --locked -p server --all-targets --all-features -- -D warnings` | worktree 根 | **0** | `reports/wp6-coder-f3-PV1-clippy.log` |
| **PV1** | `cargo test --locked -p server --all-features` | worktree 根 | **0**（296 + 14 + 6 + 4 passed；0 failed） | `reports/wp6-coder-f3-PV1-test.log` |
| **PV2** | `npm run check` | worktree 根（用 worktree 内 `node_modules` 联接，未执行任何 `npm install/ci/update`） | **0**（合同门禁全绿，含 19 个 spec 校验） | `reports/wp6-coder-f3-PV2.log` |

辅助记录（非门禁）：

- `reports/wp6-coder-f3-PV1-fmt-first-failed.log`：**首次** `cargo fmt --all -- --check` 退出码 **1**
  （新测试里一行链式调用需要 rustfmt 换行）。按派发的修复流程执行 `cargo fmt --all`（该命令只重排了本轮
  改动的 3 个文件，`git diff --stat` 已核对无越界），随后重跑 `--check` 得 0。保留此记录以免「只报最后
  一次成功」掩盖前序失败。
- `reports/wp6-coder-f3-regression-proof.log`：CR6-F1 新用例的红窗口（临时删掉修复行 → `FAILED`，
  退出码 101；随后恢复源文件并重跑全套 → 全绿）。该临时改动**未进入任何提交**（提交前 `git diff`
  已逐行核对 `command.rs` 只含修复本身）。

依赖差异说明：三条 cargo 命令的依赖状态与 `76ab011` 上 coder-F2 记录的一致（未改 `Cargo.toml` /
`Cargo.lock`），因此上一轮的检查证据未被本轮作废；本轮三条命令是**在 `a7bc596` 的源码上重新执行**的
NEW 证据，不是复用。

## 资源释放

- 未使用任何共享或独占资源：无 Agent 子进程、无监听端口、无临时目录以外的落盘产物。
- 构建产物只在 worktree 自带的 `target/`（Git 忽略），**未**触碰其它 worktree 的 target 目录，也未
  删除 `node_modules` 联接。
- 测试用例在 `std::env::temp_dir()` 下建的目录沿用既有 helper（`canonical_workspace`，标签 `idem`），
  与既有 `ok`/`denied`/`field` 等标签同机制，不新增清理义务。

## 未执行项

- **独立复核（CR6 Round 2）未执行**：本角色不自行承担独立 review。`3.7` 的复核结论待新的独立
  reviewer 实例返回（必须换新 reviewer，见 `AGENTS.md`「并发派发」）。
- **workspace 全量 `cargo clippy` / `cargo test` 未在本轮跑**：派发给的是 `-p server` 子集；
  workspace 全量属 PV1 阶段 2 / 候选阶段门禁，由 merger / 主 Agent 在候选 worktree 执行。**未执行
  不等于通过**。
- **CI 专属判定（`deps` / `advisories` / `secrets`）不在本地范围**（需网络或额外二进制），本轮未跑。
- 未写任何集成 / E2E 用例、驱动脚本或测试数据（归 TP2）。

## 待澄清问题（不阻断本轮判定）

1. **通用 mutation 臂的同形盲区**（`on_dispatched` 等既有 session-scoped mutation）本轮按边界**未修**。
   是否开一个独立发现/工作包处理（涉及 `on_dispatched` 的行为变更与 `§12.5` 的落实范围），请主 Agent
   决定；本报告不代为决定。
2. **CR6-F3 的注记归属**：`session.create` 的 RV2 记在 `session.create` 结果契约段，resume 的记在
   `session.resume` 结果契约段（本轮选择），而不是抽成一条跨 `create`/`resume` 的统一表述。两种写法都
   被 reviewer 接受；若后续 review 倾向统一表述，可在下一轮收拢为一条。
3. **`command.rs:1178-1199` 的 `local_terminal` 兜底本身未改**：CR6-F3 是 SUGGESTION 且派发明确
   「不得改代码行为」，本轮只做登记。若后续要求把该兜底从 wire 上移除（例如改为不回本地终态帧），
   那是协议语义变更，需要独立决策与契约同步。

## handoff_index

- task_id: "2.6"
  work_package: WP6
  role: coder
  phase: fix
  round: NOT_APPLICABLE
  stage: work-package
  target_revision: "a7bc596499170a6365e061c70668a203fa2534bb"
  evidence_type: CHECK
  evidence_id: PV1
  report_path: "openspec/changes/session-resume/reports/wp6-coder-f3.md"
  result: PASS
  evidence_status: NEW
  applicability_basis: "在 a7bc596 的源码上于 worktree D:/Project/acp-remote-wt/session-resume-wp6 重新执行 cargo fmt --all -- --check / cargo clippy --locked -p server --all-targets --all-features -- -D warnings / cargo test --locked -p server --all-features，退出码 0/0/0；依赖与配置未改（与 76ab011 上一轮一致），本轮是重跑而非复用"
  source_evidence: NOT_APPLICABLE
- task_id: "2.6"
  work_package: WP6
  role: coder
  phase: fix
  round: NOT_APPLICABLE
  stage: work-package
  target_revision: "a7bc596499170a6365e061c70668a203fa2534bb"
  evidence_type: CHECK
  evidence_id: PV2
  report_path: "openspec/changes/session-resume/reports/wp6-coder-f3.md"
  result: PASS
  evidence_status: NEW
  applicability_basis: "在同一 worktree 根执行 npm run check（合同门禁，含文档引用、合同漂移、crate 依赖方向、封闭词表、agentic 宿主入口等），退出码 0；本轮改动了 docs/NODE_LINK_PROTOCOL.md §12.7，该门禁对本轮文档改动敏感且已实跑"
  source_evidence: NOT_APPLICABLE
- task_id: "3.7"
  work_package: WP6
  role: coder
  phase: fix
  round: NOT_APPLICABLE
  stage: work-package
  target_revision: "a7bc596499170a6365e061c70668a203fa2534bb"
  evidence_type: DELIVERY
  evidence_id: NOT_APPLICABLE
  report_path: "openspec/changes/session-resume/reports/wp6-coder-f3.md"
  result: PASS
  evidence_status: NEW
  applicability_basis: "CR6-F1/F2/F3 三条发现的修复交付；CR6 Round 2 的独立复核尚未执行，必须由新的独立 reviewer 实例返回，本行不代为判定"
  source_evidence: NOT_APPLICABLE
