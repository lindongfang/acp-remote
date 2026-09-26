# WP7 fix3 Handoff — RV3-WP7 的三条 P2（`node-link-owner` / WP7 工作包交付前修复）

## Shared Report

- **task_id**: `RV3-WP7-F1`（§6 第 9 条兜底子项未点明显式代价，P2）、`RV3-WP7-F2`（占位期「该会话仍有 active turn」的客户端可见后果未登记、无用例固定，P2）、`RV3-WP7-F3`（常量 `1200` 写进合同却无钉死断言、`pump` 注释未提占位期只落盘不派发，P2）——对应 review 报告 `reports/rv3-wp7.md` 的 Findings。
- **role**: coder；**phase**: fix；**stage**: work-package；**evidence_id**: `RV3-WP7`
- **agent_context**: 任务级 coder 子 Agent（WP7 修复轮次 fix3），**不继承** WP7 实现或 review 会话，只按主 Agent 的 fix 清单与 `rv3-wp7.md` 原文作业。工作目录 `D:\Project\acp-remote-wt\node-link-owner`（worktree，分支 `agentic/node-link-owner`）；权威规划根 `D:\Project\acp-remote`（handoff 与日志写在该处的 `openspec/changes/node-link-owner/reports/`，该目录不入版本控制）。工具集含 shell 与 Git：本轮 diff、门禁与红向证据均由本 Agent 亲自执行。
- **base / target_revision**: base = `5c869160e8dab0361b248124f631beba239183c6`（RV3-WP7 的 Target Revision；开工时 HEAD 与之一致、`git status --porcelain` 为空）；**target = `f9dbece12fd9104bc297cf97655f965fbc1dd4e9`**（本轮 1 个提交后的 HEAD；`git status --porcelain` 为空、无 staged 文件、分支未 push）。
- **scope**（严格按派单允许写入面）：`crates/core/src/broker.rs`、`docs/CORE_PORTS_AND_STORAGE.md`（§6 第 9 条区域）。**未改**：`crates/app/**`、`crates/server/**`、其余 crate、`Cargo.toml`/`Cargo.lock`（零新依赖）、`schemas/**`、`compatibility/**`、`fixtures/**`、`openspec/**`（proposal/specs/design/plan/tasks/verification）。
- **changes**（1 个提交，2 个文件，+58/−2）：

  1. **RV3-WP7-F1（文档）**：§6 第 9 条兜底子项（「释放之后无归属事件回到『归给在跑的 turn』…因此兜底是最后手段」）后面补登记**显式代价**：占位一旦释放，该被放弃 turn 若还有迟到事件（含它终态之后的尾巴），会按在跑的 turn 归属并**照常提交**——兜底把窗口收窄到有界轮次，不是消除它；这是「提前派发（每个被放弃的 turn 都会发作）」与「会话永久卡住」之间的取舍，彻底消除需要适配器为 `EndpointEvent` 给出权威 turn 标识（本切片不要求）。行为零改动。
  2. **RV3-WP7-F2（文档 + 用例）**：§6 第 9 条占位子项（「被放弃的 turn 继续占住会话槽」）末尾补登记占位期的**客户端可见后果**：占位期该会话按**仍有 active turn** 处理（`TurnQueue.running` 未清空），`session.mode.set`/`session.config.set` 返回 `state.version_conflict`（第 8 条），`session.prompt` 按 `sessions.queue_policy` 受理排队或返回 `session.busy`（第 4 条）。**新增 core 用例**把该行为钉死（见「各条落点」）。
  3. **RV3-WP7-F3（用例 + 注释）**：既有兜底用例 `an_unobserved_abandoned_turn_hold_is_released_after_the_bounded_rounds` 里补一行 `assert_eq!(ABANDONED_TURN_HOLD_ROUNDS, 1200, "§6 第 9 条登记值")`（合同值不再只写在 docs 与 `broker.rs` 两处、无人比对）；`Broker::pump` 的文档注释补「被放弃 turn 的【占位】未释放时只落盘、不派发下一个排队 turn（§6 第 9 条，见 `HeldTurn`）」。行为零改动。
  4. **红向证据**：把 `abandon_failed_turn` 里的 `slot.hold_abandoned_turn(turn)` 临时换回 `slot.finish_turn()`（= RV2-WP7-F1 之前的语义），新用例立刻失败（模式切换被受理 → 用例 panic）；随后用备份原样还原（`git diff` 只剩本轮两处目标改动）。原始输出见日志的「wp7-fix3 轮次」分节。

- **checks**（原始输出见 `reports/wp7-integration.log` 的「wp7-fix3 轮次」分节）：
  - `cargo fmt --all -- --check` exit=0。
  - `cargo clippy --locked -p core --all-targets --all-features -- -D warnings` exit=0（派单指定范围；提交钩子另外跑了 workspace 全量 clippy，也 exit=0）。
  - `cargo test --locked -p core --all-features` exit=0（core lib **119** passed，base 为 118，本轮 **+1** = 新增用例数；0 ignored 不变）。
  - `npm run check` exit=0（10 道门禁；`check:docs` 378 links/4135 refs、`check:drift` §5 93 个方法签名、`check:agentic` 16 items 全绿）。
  - 提交钩子（`.husky/pre-commit`）通过：fmt → `npm run check` → workspace clippy；commitlint 通过（本地未装 gitleaks，CI 的 `secrets` job 仍判定）。
  - **未执行（如实记录）**：`cargo-deny`、`gitleaks` 只在 CI 运行、本地无等价物；本轮**未**重跑 workspace 全量 `cargo test` 与受控路径 e2e（改动只涉及 core 的注释/用例与 docs，接口与装配零变化；如 reviewer 要求候选 revision 上重跑，由主 Agent 安排）。
- **issues**（需要主 Agent / reviewer 知晓）：
  1. **日志并发写入残留（已处理，需 reviewer 知晓）**：本轮 append `reports/wp7-integration.log` 期间，**另一个写入者**同时在写同一文件（其输出路径为规划根 `D:\Project\acp-remote` 的 main 分支状态：core 只有 94 个用例、contract 只有 87 个方法签名、`doc links 328 files`，与本 worktree 的 revision 明显不同），两段文本在文件里被交叉写入、部分行首尾相接。本 Agent 已把交叉区域重建为：fix3 分节（完整、干净）+ 一段带标注的「并发残留」原文（原样保留，注明**不属于**本轮 fix3 证据）。fix3 分节本身的内容与命令退出码未受影响；重建前的原始文件备份在 `/tmp/wp7-integration.log.mangled.bak`（临时目录，非仓库内）。
  2. **F2 的 `RejectBusy` 分支在本切片不可达**：生产装配取 `Queue` 默认（`rv3-wp7.md` 已指出），因此文档登记的是「按 `sessions.queue_policy` 受理排队或 `session.busy` 拒绝」两种后果；本轮用例固定的是 **`Queue` 路径下的 `mode.set` → `state.version_conflict`**（默认装配），`reject_busy` 下的 `session.busy` 已有独立既有用例 `reject_busy_policy_rejects_second_prompt` 覆盖「有 active turn 时直接拒绝」，未为本轮新造装配。
  3. **F3 的常量断言是等价式**：`ABANDONED_TURN_HOLD_ROUNDS` 本来就是 1200，该断言不会红；它的价值是拦住「contract 写 1200、实现改成别的值」的漂移（review 要求的是「钉死」而不是「复现缺陷」）。本轮唯一的行为判别器是 F2 的新用例。
  4. **`[PV5]`/候选门禁仍 PENDING**：本轮改动不触及 app/server 装配，但 `rv3-wp7.md` 里 PENDING 的 `[PV5]` 仍须在候选 revision 上以约定形式重跑（该结论由主 Agent 裁定，本 Agent 未执行正式 PV5）。
- **result**: **PASS**（本轮 fix 范围内全部适用条件满足、指定门禁全绿；**不代表** RV3-WP7 的独立复核、`[PV5]` 重跑、候选门禁、E2E 或合并已完成）
- **evidence_paths**:
  - 本报告：`openspec/changes/node-link-owner/reports/wp7-fix3-handoff.md`
  - 被处理的 review：`openspec/changes/node-link-owner/reports/rv3-wp7.md`
  - 本轮原始输出（红向证据、core 全量、fmt/clippy/npm check）：`openspec/changes/node-link-owner/reports/wp7-integration.log` 的「wp7-fix3 轮次」分节
  - 上一轮证据（被本轮补登记的语义所在）：`reports/wp7-fix2-handoff.md`
- **resource_cleanup**: 未新增端口、数据库、容器、证书或外部账号；未安装依赖、未改 `Cargo.toml`/`Cargo.lock`/任何 schema 与 compatibility 资产。红向证据用的临时文件只落在系统临时目录（`/tmp/broker.rs.bak`、`/tmp/broker.rs.bak2`、`/tmp/wp7fix3-*.log`、`/tmp/wp7-integration.log.mangled.bak`，非仓库内）。提交后 `git status --porcelain` 为空，无未跟踪文件、无 staged 文件。

## 提交（供 reviewer 逐条核对）

| SHA | 标题 | 文件 |
|---|---|---|
| `f9dbece12fd9104bc297cf97655f965fbc1dd4e9` | `fix(core): 登记 §6 第 9 条占位期语义与兜底代价并钉死常量` | `crates/core/src/broker.rs`、`docs/CORE_PORTS_AND_STORAGE.md`（2 files, +58/−2） |

（经 `.husky/pre-commit` 与 commitlint；提交后 `git status --porcelain` 为空。分支 `agentic/node-link-owner` 未 push。）

## 各条落点（reviewer 复检索引）

| ID | 修复落点 | 用例 / 证据 |
|---|---|---|
| RV3-WP7-F1 | `docs/CORE_PORTS_AND_STORAGE.md` §6 第 9 条「释放之后无归属事件回到…」子项的末尾新增「**它的代价要显式登记**：…兜底把窗口收窄到有界轮次，不是消除它；彻底消除需要适配器为 `EndpointEvent` 给出权威 turn 标识」 | 纯文档；与 `HeldTurn` 的实现注释一致（`crates/core/src/broker.rs:417-419`） |
| RV3-WP7-F2 | `docs/CORE_PORTS_AND_STORAGE.md` §6 第 9 条「被放弃的 turn 继续占住会话槽」子项末尾新增占位期客户端可见后果；`crates/core/src/broker.rs` 新增用例 `mode_change_rejected_while_an_abandoned_turn_holds_the_session_slot`（doc 注释第 6331–6332 行） | 新用例：放弃提交后占位仍在 → `session.mode.set`（`expected_version` 取当前版本，避免用版本不匹配冒充该拒绝）必须返回 `state.version_conflict`，且 `current_mode()` 仍为 `None`；红向证据：把 `hold_abandoned_turn` 换回 `finish_turn()` 后该用例失败（模式切换被受理） |
| RV3-WP7-F3 | 既有用例 `an_unobserved_abandoned_turn_hold_is_released_after_the_bounded_rounds` 内新增 `assert_eq!(ABANDONED_TURN_HOLD_ROUNDS, 1200, "§6 第 9 条登记值")`（`crates/core/src/broker.rs:6314`，常量本身在 `broker.rs:255`，值未动）；`Broker::pump`（`broker.rs:1612-1614`）的文档注释补占位期语义 | `cargo test --locked -p core --all-features`（119 passed，含该断言）；`pump` 注释与 §6 第 9 条、`HeldTurn`/`hold_blocks_dispatch` 的既有注释口径一致 |

## 决策与边界（供 reviewer 判 applicability）

1. **三条都是「登记/钉死」性质，未改行为**：F1/F2 只改 §6 第 9 条的合同文字，F3 的注释与断言都不影响运行路径；唯一的新增代码是 F2 的用例（+1 个 `#[test]`）。因此本轮不引入新的端口、装配或跨 crate 决策。
2. **F2 的用例只钉 `Queue` 路径**：默认 `queue_policy = Queue` 下 `session.prompt` 在占位期仍是「受理并排队」（不新增断言，因为该行为由既有 `queue_policy_bounds_max_queued_turns`/占位用例间接覆盖）；`reject_busy` 分支的 `session.busy` 由既有用例覆盖。文档登记的是两种策略的后果，未暗示生产装配会走 `reject_busy`。
3. **未扩大写入面**：`broker.rs` 的改动只有一处文档注释、一行断言与一个新用例；`ABANDONED_TURN_HOLD_ROUNDS` 的值、语义与位置未动（无端口/配置变更，不需要动 `CONFIG_REFERENCE.md`）。
4. **日志并发残留的处理方式**：保留原文 + 明确标注，而不是删除他人写入内容；重建后的文件行数 3836，fix3 分节自「# wp7-fix3 轮次」起到标注段落结束，之后是并发残留原文。

## handoff_index

```yaml
handoff_index:
  - task_id: "RV3-WP7-F1"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "f9dbece12fd9104bc297cf97655f965fbc1dd4e9"
    evidence_type: CHECK
    evidence_id: RV3-WP7
    report_path: "reports/wp7-fix3-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "§6 第 9 条兜底子项补登记显式代价：释放占位后该被放弃 turn 的迟到事件（含终态尾巴）回到「归属在跑的 turn」并照常提交，兜底只是把窗口收窄到有界轮次；这是「提前派发」与「会话永久卡住」之间的取舍，彻底消除需适配器给出权威 turn 标识。行为零改动；docs 与 HeldTurn 注释口径一致"
    source_evidence: { id: "RV3-WP7", report_path: "reports/rv3-wp7.md", target_revision: "5c869160e8dab0361b248124f631beba239183c6" }
  - task_id: "RV3-WP7-F2"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "f9dbece12fd9104bc297cf97655f965fbc1dd4e9"
    evidence_type: CHECK
    evidence_id: RV3-WP7
    report_path: "reports/wp7-fix3-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "§6 第 9 条占位子项补登记占位期客户端可见后果（mode/config.set → state.version_conflict；prompt 按 queue_policy 受理或 session.busy），并新增用例 broker::tests::mode_change_rejected_while_an_abandoned_turn_holds_the_session_slot 固定 mode.set 被拒（expected_version 取当前版本以排除版本不匹配路径）；红向证据：把 hold_abandoned_turn 换回 finish_turn 后该用例失败；core lib 118→119 passed"
    source_evidence: { id: "RV3-WP7", report_path: "reports/rv3-wp7.md", target_revision: "5c869160e8dab0361b248124f631beba239183c6" }
  - task_id: "RV3-WP7-F3"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "f9dbece12fd9104bc297cf97655f965fbc1dd4e9"
    evidence_type: CHECK
    evidence_id: RV3-WP7
    report_path: "reports/wp7-fix3-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "既有兜底用例内新增 assert_eq!(ABANDONED_TURN_HOLD_ROUNDS, 1200, \"§6 第 9 条登记值\") 钉死合同值；Broker::pump 文档注释补「占位未释放时只落盘、不派发下一个排队 turn（§6 第 9 条）」。断言为等价式（值本就是 1200），作用是拦合同漂移而非复现缺陷；fmt/clippy/test/npm run check 全绿"
    source_evidence: { id: "RV3-WP7", report_path: "reports/rv3-wp7.md", target_revision: "5c869160e8dab0361b248124f631beba239183c6" }
  - task_id: "RV3-WP7"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "f9dbece12fd9104bc297cf97655f965fbc1dd4e9"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/wp7-integration.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "target 上 fmt exit=0、clippy -p core exit=0（提交钩子另跑 workspace clippy 亦 exit=0）、cargo test -p core exit=0（119 passed）、npm run check exit=0（10 道门禁）；原始输出见日志「wp7-fix3 轮次」分节"
    source_evidence: NOT_APPLICABLE
```
