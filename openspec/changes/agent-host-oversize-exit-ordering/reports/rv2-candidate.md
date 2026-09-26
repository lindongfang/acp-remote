I have completed my read-only review. Below is the full report artifact.

---

# RV2 — 候选（merge 前）独立只读 Review 报告

## Shared Report

- **task_id**: `6.4`（DU1 候选准备阶段的独立只读检视）
- **role**: reviewer
- **phase**: review
- **agent_context**: 独立只读子 Agent（fresh 上下文；未参与实现、未参与 RV1；与实现/集成 Agent 无私有对话继承）；工具面只有文件读取、内容检索与工作树差异查看，**无 shell / 无可执行 git 命令 / 无写工具**（除运行时指定的会话产物输出路径外未写任何文件）。隔离设置与子 Agent ID 由主 Agent 关联登记。
- **target_revision**: `25acb00d6895bcbe1ba36b658e0dd4706fd0347a`（分支 `feat/agent-host-oversize-exit-ordering` tip；短 SHA `25acb00`）
- **base_revision**: `84a8a0593b2272252a458bfeb0648bc6f1a2f669`（`refs/heads/main` tip）
- **launch_head**: `25acb00d6895bcbe1ba36b658e0dd4706fd0347a`（我的工作树差异基线，`watchdog_diff` 自报）
- **scope**（实际检视）:
  - 候选树内容：`openspec/changes/agent-host-oversize-exit-ordering/**`（proposal / design / plan / tasks / verification / `specs/local-agent-host/spec.md` / `reports/{wp1-handoff.md, wp1-local-checks.log, rv1-wp1.md, final-verify.log, stress-runs.log, integrator.md}` / `.openspec.yaml`）逐份通读。
  - 代码接缝：`crates/agent-host/src/process.rs` 的 `abort_agent`（496–520）、`ExitState`（31–60）、`is_running`/`has_exited`（200–210）、`exit.mark` 全部调用点、`abort_agent` 全部调用点、`MAX_MESSAGE_BYTES` 判据；`crates/agent-host/tests/supervision.rs` 的 `oversize_frame` 用例（396–428）。
  - 版本面（替代 git）：`.git/logs/refs/heads/main`、`.git/logs/refs/heads/feat/agent-host-oversize-exit-ordering`、`.git/objects/{46,af,25}` 的松散对象清单、`watchdog_diff`。
  - 需求映射：`docs/MODULE_ARCHITECTURE.md` §4.5 与 `docs/SECURITY_DESIGN.md`、`compatibility/**` 是否因本变更需要同步（核对 `AGENTS.md` §10 的「变更类型 → 权威文档」）。
  - 未检视（越界）：候选轮 [PV1]/[PV2] 的实际执行（主 Agent 正在跑）；E2E 降级核对（6.5）；合入与主分支回归（阶段 B）。
- **changes**: 本轮只读，未做任何修改。
- **checks**: 未执行任何构建或测试（reviewer 无执行权，规则禁止）。本轮使用的证据全部是已有报告的只读核对；需要主 Agent 执行的命令列在 Assessment 的「待补 / 需主 Agent 执行」。
- **issues**: 无 CRITICAL / MAJOR。2 条 MINOR（1 条上轮修复残留、1 条候选簿记未入库）+ 1 条 SUGGESTION，均非阻断。
- **result**: PASS
- **evidence_paths**:
  - `openspec/changes/agent-host-oversize-exit-ordering/reports/rv2-candidate.md`（本报告；仓库内落盘待主 Agent 执行）
  - 被核对对象：`reports/{integrator.md, rv1-wp1.md, wp1-handoff.md, final-verify.log, stress-runs.log, wp1-local-checks.log}`、`crates/agent-host/src/process.rs`、`crates/agent-host/tests/supervision.rs`、`verification.md`、`tasks.md`、`specs/local-agent-host/spec.md`
- **resource_cleanup**: 未启动任何进程、未创建 worktree/容器/端口、未新增依赖、未写仓库文件；`target/` 未被我触碰。

## Review Context

| 项 | 值 |
| --- | --- |
| Review ID | **RV2** |
| Review Type | merge（候选合并前） |
| Review Stage | 候选准备阶段（对应 tasks 6.4） |
| Work Package | DU1（唯一交付单元；含 WP1） |
| Repository | `D:\Project\acp-remote` |
| Base Revision | `84a8a0593b2272252a458bfeb0648bc6f1a2f669` |
| Target Revision | `25acb00d6895bcbe1ba36b658e0dd4706fd0347a` |
| 读取的规则与需求 | `AGENTS.md`（项目约束）、`openspec/schemas/agentic/roles/reviewer.md`、`roles/handoff.md`、`procedures/workflow-check.md`；`proposal.md`、`design.md`（D1–D3）、`plan.md`（五关注点 + Coverage Index R1–R3 + Main E2E not-applicable）、`tasks.md`、`verification.md`、增量规范 `specs/local-agent-host/spec.md`（1 条 ADDED Requirement + 2 Scenario）、主规范 `openspec/specs/local-agent-host/spec.md`（12 条既有 Requirement） |
| 使用的验证证据（只读核对） | `reports/final-verify.log`（PV1，1286 行，末行 `EXIT(npm run verify)=0`）、`reports/stress-runs.log`（PV2 三连跑，3 轮 `EXIT=0` 且每轮 `oversize_frame … ok`）、`reports/rv1-wp1.md`（RV1，target `af64e85`，PASS）、`reports/integrator.md`（阶段 A：基线/候选固定 + `cargo build` EXIT=0 + 代码面单文件）、`reports/wp1-local-checks.log`、`reports/wp1-handoff.md` |
| 待补证据 | 候选轮 [PV1]/[PV2]（主 Agent 并行执行中）——**待核对**；按 reviewer 规则记为待返回，不影响本轮静态判断（见 Assessment） |

### 版本与差异面的独立核对（替代不可执行的 git 命令）

我无法运行 `git rev-parse` / `git diff` / `git hash-object`（工具面无 shell）。改用可读的 git 元数据做等价核对：

1. **两个 SHA 未移动、祖先关系成立**（任务第 1 点）：`.git/logs/refs/heads/main` 末行为
   `ebd14ac7… 84a8a0593b2272252a458bfeb0648bc6f1a2f669 … commit: docs(repo): 归档 test-temp-dir-cleanup` ⇒ **main tip = `84a8a059…`**，与 Base Revision 一致。
   `.git/logs/refs/heads/feat/agent-host-oversize-exit-ordering` 首行 `0000… 84a8a059… branch: Created from HEAD`，后续为
   `84a8a05→998cb0f→af64e85→e544ac9→dee3e22→25acb00` 的逐次 ref 更新，末行 `dee3e223… 25acb00d6895bcbe1ba36b658e0dd4706fd0347a … commit: docs(repo): 登记集成就绪核对 [5.1-5.2]` ⇒ **候选 tip = `25acb00…`**。分支自 `84a8a05` 创建且只前向移动 ⇒ `merge-base --is-ancestor 84a8a05 25acb00` 为真（与 integrator 的 `EXIT=0`、`rev-list --left-right` `0 5` 自洽：5 个提交 = `998cb0f`/`af64e85`/`e544ac9`/`dee3e22`/`25acb00`）。
2. **只有 1 个代码提交**：5 个分支提交里只有 `af64e85 fix(agent-host): 超限结束时先标记退出再投递错误 [PRO-4]` 是代码提交，其余 4 个提交信息均为 `docs(repo): …` 规划/簿记。这是「代码面 = 1 个文件」的旁证，独立于 integrator 的 `-- crates/` 输出。
3. **候选代码 = 已验收实现（blob 一致性）— 部分复核**：`.git/objects/af/64e85cc252515039b0da97dcc3b88dbde277dc`、`.git/objects/25/acb00d6895bcbe1ba36b658e0dd4706fd0347a` 存在 ⇒ 两个提交对象真实；`af64e85` 与 `25acb00` 在**同一分支链**上且其间三个提交均为文档簿记。integrator 声称的候选 `process.rs` blob `46fbe458cb5745b02f084d6391b697eb0c9a248a` 在 `.git/objects/46/fbe458cb5745b02f084d6391b697eb0c9a248a` **真实存在**（松散对象，非杜撰哈希），且 main 侧 `process.rs` blob `4e448cbf…` 无松散对象（早已打包）——与「`46fbe458…` 是本变更新写出的、即修复后内容」相符。**但我无法重算 SHA-1，因此「逐字节相同」仍是旁证而非我独立复算的结论**（见 Assessment 待补项）。
4. **工作树 = 候选内容（代码接缝）**：`watchdog_diff`（path = `crates/agent-host/src/process.rs`）报
   `No working-tree changes against reviewer-launch HEAD 25acb00…` ⇒ 我读到的 `process.rs` **就是提交 `25acb00` 里存的内容**，不是某个未提交的中间态。
5. **代码面之外的候选差异**（任务第 2 点的 `docs/`/`schemas/`/`fixtures/`/`compatibility/`/根配置零改动）我**无法**自行执行 `git diff --name-status`；我改为核对了**「本变更是否按 `AGENTS.md` §10 需要改这些文档」**：`docs/` 全仓检索 `abort_agent|超限|is_running|标记退出|exit.mark|先标记|后标记`，`MODULE_ARCHITECTURE.md` §4.5（`agent-host` 职责/进程树清理/stdio 传输/失败路径）未规定退出标记与错误投递的顺序，`SECURITY_DESIGN.md:378`（「超限即报错并结束该 Agent」）仍被满足，`compatibility/**` 无 `oversize|超限` 相关条目 ⇒ **本变更不需要任何 `docs/`/`compatibility/` 同步**，proposal 的「零改动」主张在语义上成立。逐字 diff 仍需主 Agent 执行（见待补项）。

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| RV2-F1 | MINOR | `openspec/changes/agent-host-oversize-exit-ordering/reports/wp1-handoff.md:96`（对照 `crates/agent-host/src/process.rs:55`） | 该行仍写「该覆写是 **56 行**无条件覆写的既有行为」，而 `*slot = Some(status);` 实际在 `process.rs:55`（我逐行计数：53 `fn mark`、54 `if let Ok(mut slot) = …{`、55 `*slot = Some(status);`、56 `}`、57 `done.store(true, SeqCst)`、58 `notify_waiters()`、59 `}`）。同一报告的 :88 已按 RV1-MINOR-1 改成「55 行」 ⇒ **RV1-MINOR-1 的订正只覆盖了两处中的一处**，而 `verification.md` 的 Review Findings 记「56→55（已改）」。 | 不影响代码与结论（D2-1 断言的实体「无条件覆写 / 单点置位 / 幂等」在 53–59 行逐条可查）；影响的是权威记录对「已订正」的表述精度，会误导后续复核者以为报告已无残留错误引用。 | 把 `wp1-handoff.md:96` 的 `56 行` 改为 `55 行`（或删去行号、改为「`ExitState::mark` 内的无条件覆写」）；同时把 `verification.md` 的 RV1 处置句改为「两处 56→55 已订正」。 | 未修复（本轮只读，交主 Agent；属文档精度，不阻断） |
| RV2-F2 | MINOR | 候选树 `25acb00` 的变更目录簿记未入库：`reports/integrator.md`（未跟踪）、`tasks.md` 第 28–29 行勾选（未提交）；`verification.md` 无 6.1/6.2 记录 | `watchdog_diff`（全仓）相对 launch HEAD `25acb00` 报：`tasks.md` 中 `- [ ] 6.1 / - [ ] 6.2` → `- [x]`（未提交），且未跟踪清单仅 `openspec/changes/agent-host-oversize-exit-ordering/reports/integrator.md`。`.gitignore` 只忽略 `openspec/changes/**/reports/**/*.log` 与根 `/reports/`，**不**忽略 `reports/*.md` ⇒ `integrator.md` 属「应当入库但尚未入库」。`verification.md` 现有节（Target / Handoff Index / Checks / 5.1-5.2 / Project Verify / Review Findings / …）中**没有** 6.1/6.2 的基线、候选提交、树哈希与 `cargo build` 结果登记，而 `tasks.md` 6.1/6.2 的完成条件明确要求「写入 `verification.md`」。 | 候选修订 `25acb00` 的变更目录**尚不完整**：若以该提交直接 `--ff-only` 合入，`main` 将不含集成报告与候选记录，而 premerge/最终验收会引用 `reports/integrator.md` 与候选证据。对代码正确性无影响。 | 在 6.6 合入前补一个 `docs(repo)` 簿记提交，把 `tasks.md` 勾选、`reports/integrator.md`、`verification.md` 的 6.1/6.2（以及后续 6.3/6.4/6.5）记录一并入库，再以该 tip 作为 premerge 的 `candidate_commit`；若坚持用 `25acb00` 作候选，则该提交必须包含上述文件。 | 未修复（本轮只读；需在 6.6/premerge 门之前闭合） |
| RV2-F3 | SUGGESTION | `verification.md` 的 Project Verify 表 vs `reports/final-verify.log:1`、`reports/stress-runs.log:1` | `verification.md` 记「PV1 主 Agent / `af64e85`」「PV2 主 Agent / `af64e85`」，而两份日志首行自报版本为 **`e544ac9`**（`=== PV1 npm run verify（任务 3.1，e544ac9，2026-09-25T23:56:37Z）===`、`=== PV2 三连跑（任务 3.1，e544ac9，2026-09-25T23:58:21Z）===`）。reflog 时间线（`af64e85` = 23:55:06Z、`e544ac9` = 23:55:53Z、PV1 = 23:56:37Z）说明执行时 tip 确为 `e544ac9`。`e544ac9` 是 `af64e85` 的文档簿记后继（提交信息 `docs(repo): 登记 WP1 交接验收与任务勾选 [2.1-2.2]`）。 | 不影响结论：`e544ac9` 相对 `af64e85` 只增加簿记文档，代码相同（integrator 的 blob 比对 + 我上面的旁证），因此「证据适用于 `af64e85` 的代码」成立。影响是审计时「记录版本」与「原始日志版本」不一致，且 RV1 报告本身已区分 `target_revision=af64e85` 与 `launch_head=e544ac9`——`verification.md` 缺这一层区分。 | 在 Project Verify 表写「`af64e85`（执行时树 `e544ac9`，仅簿记差异）」；后续在 `agentic-premerge` 块中把候选证据的 `applicability_basis` 锚定到**代码 blob `46fbe458…`** 而非仅提交 SHA——因为 6.3/6.4/6.5 的簿记提交必然继续把 HEAD 前移，仅靠提交 SHA 会让候选绑定反复失效。 | 不适用（建议类） |

## Assessment

### 本轮检视结论（对应 Target Revision `25acb00d6895bcbe1ba36b658e0dd4706fd0347a`）

**PASS** — 完成约定范围的检视，无未解决 CRITICAL / MAJOR；`plan.md` Code Review 的五条阻断标准**无一条被触发**。

逐关注点（均为我独立读码核对，非采信 RV1 结论）：

1. **新顺序 = 「标记 → 投递 → terminate」且与注释一致** ✓
   `crates/agent-host/src/process.rs:504` 结构化错误日志 → `:508 exit.mark(format!("ACP 消息超限（{actual} > {limit} 字节）"))` → `:509-512` 单锁内 `drain()` 取整表 → `:513-518` 逐个 `sender.send(Err(HostError::Protocol(AcpError::Oversize { limit, actual })))` → `:519 tree.terminate()`（函数体内唯一一条、无条件）。注释 `:505-507` 与代码逐句吻合，并说明反向顺序的后果。
2. **D2 两条不变量的核实记录真实可查** ✓（除 F1 的行号残差）
   `ExitState::mark` = `process.rs:53-59`：`status` 无条件覆写（`:55`，last-wins）、`done.store(true, SeqCst)` 单点置位（`:57`）、`notify_waiters()`（`:58`）；全仓 `exit.mark` 调用点**仅** `:508` 与 `:652`；`wait_loop` 仍为「先 drain → 投递 `AgentExited` → 最后 `mark`」（`is_running()` 的 `done` 读在 `:203`、`has_exited` 读在 `:209`）。`abort_agent` 体内无 `if`/提前 return，不查询 `is_running()` ⇒ 「标记后、terminate 前」不会让任何路径跳过终止。消费方穷举（`grep is_running\(\)|has_exited\(\)`）与交接报告一致：`process.rs:103/249/281`、`host.rs:220/512/628`，其中读到「假」的分支都是**更多**清理或失败关闭（`SessionClosed`），无反向逻辑；`has_exited()` 生产代码无消费方（仅 `supervision.rs:475`）。
3. **`wait_loop` 与其余 `exit.mark` 调用点零改动** ✓ — 见上；`abort_agent` 定义 `:496`、调用点仅 `:470`（`read_loop` 缓冲超限）与 `:535`（`handle_line` 单帧超限），两者共用同一函数体 ⇒ 两条超限入口同时获得新顺序（与 spec Scenario 2「自然退出不受约束」不冲突）。
4. **未弱化断言、未触碰测试** ✓ — `tests/supervision.rs:396-428` 仍断言「错误必须是 `Oversize`」且「错误可见时 `!is_running()`」；其文档注释（`:396`）与设计期引用行号一致；`wp1-local-checks.log` 显示 `supervision.rs` 15/15 通过（含该用例）。
5. **spec 增量与实现语义一致** ✓ — 增量文件形态合规（无 Purpose，仅 `## ADDED Requirements` 1 条 Requirement + 2 Scenario）；`mark`（`:508`）严格先于取表（`:509`）与第一个 `send`（`:514`），drain 为单锁内一次性取整表 ⇒ 被唤醒的调用方观察到错误时 `exit.done` 必为真、`closing` 为假 ⇒ `is_running()` 必为假（确定性来自构造，非调度运气）。主规范 `openspec/specs/local-agent-host/spec.md` 未包含该 Requirement（12 条既有 Requirement 中只有「失败的明确结果」与「进程树清理」相邻），归档时落盘符合 design 与 proposal 的声明。

### 检查计划对应的证据核对（逐 ID）

| Check / Review ID | 记录（`verification.md`） | 原始文件核对结果 | 是否影响本轮判断 |
| --- | --- | --- | --- |
| PV1 | 主 Agent / `af64e85` / `npm run verify` EXIT=0 | `reports/final-verify.log` 可读、1286 行、末行 `EXIT(npm run verify)=0`；`:298` 有 `oversize_frame … ok`；无 `FAILED` 行。版本标注差异见 **RV2-F3**。 | 否（静态判断不依赖它）；待候选轮刷新后由主 Agent 核对 |
| PV2 | 主 Agent / `af64e85` / 三连跑 | `reports/stress-runs.log` 可读：3 轮均 `EXIT(runN)=0`、`FAILED行数: 0`、每轮 `oversize_frame … ok`（与 plan 的 pass criteria 完全一致）。 | 否 |
| RV1 | reviewer / `af64e85` / PASS | `reports/rv1-wp1.md` 可读；其 `target_revision` = `af64e85cc252515039b0da97dcc3b88dbde277dc`、`base_revision` = `998cb0f…`、`launch_head` = `e544ac9…`，与 reflog 时间线一致。RV1-MINOR-2（`@@` 头）已闭合（`wp1-handoff.md` 的 diff 块已加注「权威 diff 以 `git diff 998cb0f..af64e85` 为准」）；RV1-MINOR-1 **部分闭合**（见 **RV2-F1**）。 | 否 |
| 局部检查（2.2） | `wp1-handoff.md` / `wp1-local-checks.log` | 日志可读：`cargo test -p agent-host` EXIT=0（4+0+22+13+15+1+0 全绿）、`cargo fmt --all -- --check` EXIT=0、`cargo clippy -p agent-host --all-targets --all-features -D warnings` EXIT=0。 | 否 |
| 候选轮 PV1/PV2（6.3） | 未登记 | **待返回**（主 Agent 并行执行中）。 | 否——本轮为静态代码检视，公开契约（spec 增量、D1 顺序、断言未弱化）已可独立判定；但**不得**用本 PASS 代替候选轮 Project Verify，premerge 门仍须等其 PASS |

Main E2E：`plan.md` 记为 `mode: not-applicable`（含 `reason`/`basis`/两项 `alternative_checks`/用户降级批准原话，均已填写）；6.5 的逐项核对属主 Agent，我未越界。

### 待补 / 需主 Agent 执行（我无 shell 与写权）

1. `git rev-parse refs/heads/main HEAD`、`git merge-base --is-ancestor 84a8a05… 25acb00…`（期望 `84a8a059…` / `25acb00…` / EXIT=0）——我已用 reflog 与松散对象清单等价核对，逐字命令输出仍建议留档。
2. `git rev-parse 25acb00:crates/agent-host/src/process.rs af64e85:crates/agent-host/src/process.rs`（期望两者均为 `46fbe458cb5745b02f084d6391b697eb0c9a248a`）——**这是本轮唯一未能自证的关键项**；我只证明了该 blob 对象真实存在、`process.rs` 工作树内容等于 `25acb00` 的提交内容、且分支上只有 1 个代码提交。
3. `git diff --name-status 84a8a05..25acb00`（期望 1 行 `M crates/agent-host/src/process.rs` + 其余 `A openspec/changes/agent-host-oversize-exit-ordering/**`）与 `git diff --name-status 84a8a05..25acb00 -- docs/ schemas/ fixtures/ compatibility/ '*.toml' '*.json' '*.mjs' '*.yml'`（期望空）。
4. 候选轮 [PV1]/[PV2]（6.3）结果回填，并在 `agentic-premerge` 块中把 `candidate_commit` 与证据路径/摘要对齐（注意 RV2-F3 的建议：以代码 blob 为适用性锚点）。
5. 本报告落盘：我无写工具，未能写入仓库内 `openspec/changes/agent-host-oversize-exit-ordering/reports/rv2-candidate.md`；本报告由运行时持久化到本会话产物目录的同名相对路径，需要仓库内副本与 `verification.md` 的 Review Findings / tasks 6.4 登记时请主 Agent 落盘。

### 结论

- **分级汇总**：CRITICAL 0；MAJOR 0；MINOR 2（RV2-F1 报告残留行号、RV2-F2 候选簿记未入库）；SUGGESTION 1（RV2-F3 证据版本标注/适用性锚点）。
- 代码面在 `25acb00` 上**忠实且最小**地落实了 design D1 与增量规范两个 Scenario，未触及 `wait_loop`、测试、签名、错误类型、错误码与其它 crate；`docs/`/`compatibility/` 按 `AGENTS.md` §10 确实无需同步。
- **PASS**（仅覆盖本轮范围：候选静态检视）。候选轮 Project Verify 与 premerge 门仍待完成，本 PASS 不得用于宣称变更可归档。

## handoff_index

```yaml
handoff_index:
  - task_id: "6.4"
    role: reviewer
    phase: review
    stage: candidate
    target_revision: "25acb00d6895bcbe1ba36b658e0dd4706fd0347a"
    evidence_type: REVIEW
    evidence_id: RV2
    report_path: openspec/changes/agent-host-oversize-exit-ordering/reports/rv2-candidate.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "在候选修订 25acb00d6895bcbe1ba36b658e0dd4706fd0347a（base 84a8a0593b2272252a458bfeb0648bc6f1a2f669，工作树相对该提交干净、代码接缝已核对）上只读检视：变更目录全部文件 + crates/agent-host/src/process.rs 的 abort_agent(496-520)/ExitState(31-60)/is_running(202-204)/wait_loop/exit.mark 全部调用点 + supervision.rs 的 oversize_frame 用例(396-428) + 主规范 local-agent-host。独立复核 plan.md Code Review 五关注点全通过；版本面用 .git 的 reflog 与松散对象清单等价核对（main=84a8a05…、候选=25acb00…、分支自 84a8a05 创建且仅 1 个代码提交 af64e85）。发现 2 条 MINOR（wp1-handoff.md:96 的 56→55 残留、候选未含 6.1/6.2 簿记与 reports/integrator.md）+ 1 条 SUGGESTION（验证记录与日志的版本标注），无 CRITICAL/MAJOR，故 PASS。未执行：候选轮 PV1/PV2（主 Agent，待返回）、git diff/blob 逐字复核（reviewer 无 shell，已列为主 Agent 待执行项，其中 blob 一致性为代表项）。"
    source_evidence: NOT_APPLICABLE
```

---

未修改任何仓库文件；未暂存任何内容；未执行构建、测试或 git 写操作。