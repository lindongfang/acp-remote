<!-- 本文件是 apply 期间的执行与变更历史记录；策略与安排在 plan.md，步骤与进度在 tasks.md。 -->

## Target

- 变更：`core-turn-view-fields`（core 侧 `turnId`/`version` 收口，实施切片 3）
- 仓库：`D:/Project/acp-remote`（绝对路径）
- 约定主分支：`local` / `refs/heads/main`；本变更分支 `feat/core-turn-view-fields`
- 基线（apply 起点）：`1cd0416`（`refs/heads/main`，apply 开始时 `git rev-parse refs/heads/main` 与 `HEAD` 一致，工作区干净）
- 实现提交：`f582376`（core 注入/断言、storage-sqlite 缺陷修复、文档与用例；见下 Checks）
- 版本确认负责人 / 方式：主 Agent（environment/recon 类事实核实）；`git rev-parse refs/heads/main` + `git status --porcelain`，每次执行固定自身提交
- 文档提交与代码提交关系：本变更的规划文档（`openspec/changes/core-turn-view-fields/**`）与代码同分支推进；`plan.md`/`tasks.md` 的计划文字更新与实现同在 `f582376`，执行记录（本文件）在其后提交

## Checks

实现角色：本变更的实现类任务由**主 Agent 以 coder 角色**执行（无独立宿主 Agent 可用时按 schema 允许的串行方式，如实记录）；交付前检查与用例随实现同批产出，独立 review 见 Review Findings。

| Check ID / Stage / Work Package | Revision / Base | Scope | Executor | Command / Steps | Environment | Result / Exit Code | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| PV1 / WP1 交付前 | `f582376`（基线 `1cd0416`） | 十道合同门禁（schemas/commands/errors/features/assets/acp/docs/boundaries/drift/agentic）；核对 `check:boundaries` 仍为 8 crate、core 依赖闭包未变、`check:drift` 仍与 `migrate.rs`/`ports.rs` 一致 | 主 Agent（coder 角色） | `npm run check`（cwd `D:/Project/acp-remote`） | Windows x64；Node v24.19.0 / npm 12.0.2；rustc 1.98.1（`rust-toolchain.toml`）；无网络需求 | PASS / exit 0（8/8 spec 校验通过，drift 36 DDL + 15 trait/87 sigs） | `openspec/changes/core-turn-view-fields/reports/wp1-contract-docs.log` |
| PV2 / WP2 交付前 | `f582376` | `cargo fmt --check`、`cargo clippy -p core -D warnings`、`node scripts/check-crate-boundaries.mjs`、`cargo test -p core`（注入/冲突/版本/漂移/重放/保真/TurnAccepted 共 12 条新用例 + 既有 78 条） | 主 Agent（coder 角色） | 见 log 头部逐条命令 | 同上；无 `CARGO_TARGET_DIR` 覆盖 | PASS / 全部 exit 0；`cargo test -p core` = **91 passed / 0 failed / 0 ignored** | `reports/wp2-core-injection.log` |
| PV5 / WP3 交付前 | `f582376` | 真实 SQLite（临时文件）上的版本规则：纯事件提交不递增、状态变更恰好 +1、过期 `expected_version` 被拒；含 `Update` 分支回填 `epoch` 的回归 | 主 Agent（coder 角色） | `cargo test --locked -p storage-sqlite --all-features` | 同上；落盘目录为系统临时目录 `acpr-storage-*`，用例自清 | PASS / exit 0（新增 `session_version_rule` 3 passed；既有套件 91 passed / 2 ignored——`crash_child`、`regenerate_v2_fixtures` 为既有 `#[ignore]`） | `reports/wp3-storage-version.log` |
| PV3 / WP4 交付前 | `f582376` | 跨 crate 契约：真实适配器（fake ACP 子进程三场景）产出的 view 不含 `turnId`/`version`、其余 §10.3 最低字段齐备、`EndpointEvent.turn` 为 `None`、core 独占类型不被适配器产出 | 主 Agent（coder 角色） | `cargo test --locked -p core -p agent-host --all-features` | 同上；agent-host 用例会 spawn fake ACP 子进程与临时文件，用例自清 | PASS / exit 0（core 91 passed；agent-host 53 passed，含新增 `view_contract` 1 passed） | `reports/wp4-agent-host-contract.log` |
| PV4 / DU1 候选与主分支 | 待候选（见 Merge History） | 统一入口：`npm run check` + fmt + clippy（workspace all-targets）+ 全工作区测试 | 待独立检查执行者 | `npm run verify` | 同上 | 待记录 | 待 `reports/du1-pv1.log`、`reports/du1-main-verify.log` |

交付前自检复核（tasks 3.2/3.3/3.4 的完成条件，由实现者执行并留证）：

- **3.2（PV2）**：log 含逐条命令与退出码；`cargo test -p core` 用例数由 78 增至 91（新增 9 条 broker 用例 + 3 条 model 用例），无整体跳过、无零用例套件；既有用例的改动**只**是把「适配器视图里自造的 `turnId` 占位」去掉（真实适配器不产该字段，见 Check Plan Change 3 与 Failures F2），没有删除或弱化任何断言。
- **3.3（PV5）**：新增用例经真实 `SqliteStore`（非替身）打开临时库；反向验证思路：把 `session_store.rs` 的 `Update` 分支修复回退（不填 `origin_epoch`）→ `state_change_commits_bump_the_session_version_by_one` 立刻变红（`InvalidRequest("a session-scoped event requires an origin epoch")`）→ 恢复后变绿，证明该用例可证伪。
- **3.4（PV3）**：`view_contract` 用例使用真实 `AgentHost`/`SessionEndpoint` 与 fake ACP 子进程产出的事件（不是重新声明一份映射），并对「本次必须真实检查到的类型」逐项断言（缺任一即失败）；反向验证思路：给任一 view 加上 `turnId`、或从检查清单移除产物类型，用例即红。

## Check Plan Changes

实现期登记 4 条（原/新值、原因、影响与受影响任务；specs 与 plan 的覆盖索引已同步）：

1. **R1：imported 路径由「补齐 `turnId`」改为「保留 Owner 注入的取值」**（原：「imported 路径同样补齐…含顶层 `turnId`，取值等于该事件携带的 turn 归属」；新：「保留 Owner 给出的取值，本端不重写、不补齐、不伪造」）。原因：`deliver_imported` 的入参只有 `payload: Option<EventPayload>`，core 在 imported 路径上**没有**独立的 turn 来源；且无正文索引的 `payload_digest` 由调用方给出并覆盖 Owner 的视图字节（`storage-sqlite/src/session_store.rs` 对 imported 只绑定调用方摘要），重写 view 会让摘要与正文不再一致。所以「补齐」既不可实现也有害。风险覆盖：imported 视图的 `turnId` 由同一份 core 在 Owner 侧注入（Node Link 尚未实现，无历史兼容负担）；新增用例 `imported_deliveries_keep_the_owner_view_bytes` 覆盖「带身份字节逐字转发」与「缺失时不伪造」两个方向。受影响任务：2.4、2.7、plan 覆盖行 R4/R11（scenario 标题已同步）。
2. **R4：版本推导/比对的范围限定 + 漂移检测的时机写明**（原：「系统 SHALL 在提交前按…推导预期会话版本，并在提交后与存储层返回的版本比对」全覆盖；新：限定为「含 §10.3 要求 `version` 的 view 的提交」，并写明「比对发生在存储返回之后，本端不撤销已落盘的行」；R4 的第二个 scenario 由「该批事件不进入可重放状态」改为「不得把该批报告为成功」）。原因：① 对**每次**提交都推导需要额外读一次会话版本（事件批没有 `expected_version`），而实测 `expected_version` 为 `Some` 且无状态变更时存储层**不**校验版本，按它断言会产生误报，故只在该字段真正被写入的提交上推导/比对；② 存储层提交是原子的、没有回滚接口，失败关闭只能保证「不发布、不报成功」。风险覆盖：fake 存储的脚本化漂移用例（`a_version_rule_drift_fails_closed`）与真实存储的规则用例（PV5）共同覆盖；受影响任务：2.5、2.7。
3. **R7：把「适配器不返回 turn 标识」改为「返回占位值时归属仍由 core 决定」**（scenario 标题与 THEN 同步）。原因：`TurnAccepted.turn` 的类型是必填 `TurnId`（`crates/core/src/ports.rs`），「不返回」在当前端口形状下不可表达；新增用例用两种占位值（全零与任意 UUID）覆盖「不产生第二个 turn 行、id 与 view 的 `turnId` 都取 core 权威值」。受影响任务：2.5、2.6。
4. **WP3 实测发现的存储层缺陷与修复**（WP3 写范围由「仅 `tests/**`」扩为「+ `src/session_store.rs` 一处 1 行修复」）：`SessionStore::commit` 在 `StateChange::Update` 分支未像无状态分支那样回填 `origin_epoch`，导致「状态变更 + 会话级事件」同批提交被拒（`InvalidRequest("a session-scoped event requires an origin epoch")`）。这与 §5.2 的 `OwnedCommit.origin_epoch` 文档（「新建会话时由 core 生成并传入；存储层只校验已有 epoch 时必须一致」）矛盾，而且是 broker 的常规路径（终态事件与状态变更同批；`Broker::commit_owned` 只在建会话时传 `epoch`）。修复：`Update` 分支在校验通过后 `origin_epoch.get_or_insert(stored_epoch)`，与无状态分支一致；未改 DDL/`migrate.rs`、未改端口签名。证据见 Failures F1；受影响任务：2.9（文字已同步）。

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| WP3 | WP2 | `f582376`；`reports/wp2-core-injection.log`（PV2 PASS） | `f582376` | WP3 只用 `acp_core::ports` 的 `OwnedCommit`/`CommitOutcome`/`SessionStore` 与 `acp_core::model` 值对象，不引用 broker 内部逻辑；包含关系：`cargo test -p storage-sqlite` 编译并跑通引用 `acp_core` 的集成用例（PV5 PASS） | core 的版本语义或端口签名变化 ⇒ WP3 重跑 |
| WP4 | WP2、WP3 | `f582376`；`reports/wp2-core-injection.log` + `reports/wp3-storage-version.log` | `f582376` | WP4 只用 `agent-host` 的真实 `AgentHost`/`SessionEndpoint` 与 `acp_core::model` 类型（不含 broker），断言适配器与 core 的字段分工；包含关系：`cargo test -p agent-host` 编译并跑通 `tests/view_contract.rs`（PV3 PASS） | core 注入口径或适配器 view 产出变化 ⇒ WP4 重跑 |

无代码交接阻塞：WP3/WP4 都以 WP2 的**公开**类型为契约，不需要跨工作区拷贝实现。

## Runtime Resources

- 共享构建目录 `target/`：本次全部 cargo 命令**串行**执行（同一时间一个执行者），未使用 `CARGO_TARGET_DIR` 隔离，因此无并发污染；执行前后 `git status --porcelain` 与 `cargo fmt --check` 结果一致。
- 测试用临时资源：`crates/storage-sqlite` 用例在系统临时目录建 `acpr-storage-*` 库文件（用例自建自清）；`crates/agent-host` 的 fake ACP 场景会 spawn 子进程并在退出时由 endpoint/supervisor 结束进程树；`cargo test -p agent-host` 退出后未见遗留子进程。
- 无数据库服务、端口、容器、外部账号或网络依赖；`cargo-deny`/`gitleaks` 只在 CI 判定（本地无等价物，本变更不声称其通过）。

## Review Findings

RV1（独立 review，WP1–WP4 交付前）：待执行——由新的隔离上下文（bash 可用的只读 reviewer）执行，报告路径 `reports/rv1-wp1.md`（覆盖 WP1+WP2）与 `reports/rv1-wp3.md`（覆盖 WP3+WP4，含 `origin_epoch` 修复）；复核轮 `reports/rv1-wp2-recheck.md`。

| ID | Revision | Reviewer | Location | Severity / Impact | Resolution | Recheck Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| 待记录 | `f582376` | 待记录（隔离上下文） | 待记录 | 待记录 | 待记录 | 待记录 |

## Merge History

- 集成方式：`integrated`（单一交付单元 DU1，WP1–WP4）。独立性：尚无独立集成 Agent 记录——本变更的候选构造与合入由主 Agent 执行，独立检查与 review 仍由隔离上下文承担（见 Review Findings 与 Checks 的 PV4 段）；此处按 schema 允许的串行方式如实记录，待 5.1/5.2 复核后补齐实际 ID 与报告路径。
- 候选/合入/主分支提交与 PV4 证据：待记录（`reports/du1-integration.md`、`reports/du1-pv1.log`、`reports/du1-main-verify.log`）。

## Test Design and Authoring

不适用：本变更 Main E2E 为 `not-applicable`，不生成 TP 与 E2E 用例。行为用例随实现产出（WP2/WP3/WP4 各自的回归与契约用例），其独立检视在 Review Findings 记录。

## Main E2E

- 项目开关（`npx --quiet --no-install openspec-agentic e2e --json`，2026-09-24 实测）：`enabled = true`、`command = ""`、`maxAttempts = 3`。
- `plan.md` 的 mode：`not-applicable`；`downgrade_approval`：「用户 2026-09-24 本会话批准原话：「同意本变更 Main E2E 记 not-applicable」（来源：本变更启动前的范围确认提问第 1 项）」。
- reason：项目当前没有可端到端运行的产品路径（`server`/`app`/`identity-*`/前端尚未实现），本次改动落在 core 库内部的提交漏斗与 view 组装。
- basis：项目级开关开启但 `command` 为空；`openspec/config.yaml` 的 E2E 段记录 2026-09-23 用户确认「保持命令为空、按切片各自批准降级」，本变更已取得当次批准。
- alternative_checks（在 Checks 表逐项留证，最终阶段在 `reports/alt-final-verification.md` 汇总）：PV4 `npm run verify`；PV2 `cargo test -p core`；PV5 `cargo test -p storage-sqlite`；PV3 `cargo test -p core -p agent-host`。
- E2E 记 **NOT_APPLICABLE**；`[e2e-owned]` 门禁行在替代验证完成后由 `e2e check` 自动勾选（tasks 7.3）。

## Failures and Retests

| Issue ID / Task | Source / Check or E2E ID / Attempt / Version | Owner | Fix / Recovery | Review / Readiness Evidence | Retest Evidence | Current Status / Basis |
| --- | --- | --- | --- | --- | --- | --- |
| F1 / 2.9 | WP3 首轮 `cargo test -p storage-sqlite --test session_version_rule`（`f582376` 之前的工作区）→ `state_change_commits_bump_the_session_version_by_one` FAIL：`InvalidRequest("a session-scoped event requires an origin epoch")` | 主 Agent（coder 角色） | 修 `crates/storage-sqlite/src/session_store.rs` 的 `Update` 分支：校验通过后 `origin_epoch.get_or_insert(stored_epoch)`（与无状态分支一致，符合 §5.2） | 待 RV1-WP3 复核（Check Plan Change 4） | `reports/wp3-storage-version.log`（3 passed，含该用例）；回退修复即变红（见 3.3 自检） | 已解决（待独立复核） |
| F2 / 2.2–2.7 | WP2 首次 `cargo test -p core`：17 条既有用例 FAIL，原因是测试自造的适配器视图带占位 `turnId`（`uuid_text(0)`）与新的冲突校验（R2）冲突 | 主 Agent（coder 角色） | 按「适配器不产 `turnId`」的事实更新既有测试数据（`turn_view` 辅助函数、delta/permission/thought 视图去掉自造字段），并新增注入断言；未删除或弱化任何断言 | 待 RV1-WP1 复核 | `reports/wp2-core-injection.log`（91 passed） | 已解决（预期变化，待独立复核） |
| F3 / 2.7 | WP2 `replayed_commits_do_not_reinject_view_fields` 首轮 FAIL：用例把注入版本写死为 `"1"`，实际是运行期版本 | 主 Agent（coder 角色） | 改为与「当时会话版本」比较（`session.version()`），并让脚本不带终态事件以避免版本在断言期间变化 | 待 RV1-WP1 复核 | `reports/wp2-core-injection.log` | 已解决（用例自身修正，待独立复核） |
| F4 / 3.4 | `crates/agent-host/tests/view_contract.rs` 首轮编译失败：`SessionBackendFactory` 未导入、`Vec<String>::contains(&str)` 类型不符 | 主 Agent（coder 角色） | 导入 trait、改用 `iter().any(...)` 比较 | 待 RV1-WP3 复核 | `reports/wp4-agent-host-contract.log`（1 passed） | 已解决（编译期修正） |

## Final Assessment

- Assessment ID / Time：待最终阶段（apply 结束前）
- Target / Task：`refs/heads/main` 的最终提交（待记录）/ tasks 8.1（`[final-verification]`）
- CLI State：本文件写入时 `openspec status --change core-turn-view-fields` 为 `all_done` 之前的 `ready`（待复述原始输出）
- Audit / Evidence：待记录（Checks、Review Findings、Merge History、Main E2E 的引用）
- Result / Open Issues：未验收（RV1、DU1 候选/合入、替代验证、`e2e check`、`workflow check --stage final` 均待执行）
- Required Follow-up：RV1 复核 → DU1 候选 PV4 与 review → 本地合入与主分支复跑 → 替代验证四项 → `e2e check` → 最终验收
