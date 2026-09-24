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
| PV1 / WP1 交付前（`reports/wp1-contract-docs.log` 的 doc-links 计数 2956 refs/153 md 是**修复批提交前**的快照：当时两份 review 报告尚未落盘；最终树（`6856a6e`）复跑见本行证据日志：367 links / 3058 refs / 156 md，exit 0） | `f582376`（基线 `1cd0416`） | 十道合同门禁（schemas/commands/errors/features/assets/acp/docs/boundaries/drift/agentic）；核对 `check:boundaries` 仍为 8 crate、core 依赖闭包未变、`check:drift` 仍与 `migrate.rs`/`ports.rs` 一致 | 主 Agent（coder 角色） | `npm run check`（cwd `D:/Project/acp-remote`）；RV1 修复批后复跑 | Windows x64；Node v24.19.0 / npm 12.0.2；rustc 1.98.1（`rust-toolchain.toml`）；无网络需求 | PASS / exit 0（8/8 spec 校验通过，drift 36 DDL + 15 trait/87 sigs） | `openspec/changes/core-turn-view-fields/reports/wp1-contract-docs.log` |
| PV2 / WP2 交付前 | `f582376` + RV1 修复批 | `cargo fmt --check`、`cargo clippy -p core -D warnings`、`node scripts/check-crate-boundaries.mjs`、`cargo test -p core`（注入/冲突/版本/漂移/重放/保真/`TurnAccepted`/无归属降级/非字符串冲突 共 16 条新用例 + 既有 78 条） | 主 Agent（coder 角色） | 见 log 头部逐条命令 | 同上；无 `CARGO_TARGET_DIR` 覆盖 | PASS / 全部 exit 0；`cargo test -p core` = **94 passed / 0 failed / 0 ignored** | `reports/wp2-core-injection.log` |
| PV5 / WP3 交付前 | `f582376` + RV1 修复批 | 真实 SQLite（临时文件）上的版本规则：纯事件提交不递增、状态变更恰好 +1、过期 `expected_version` 被拒；含 `Update` 分支回填 `epoch` 的回归 | 主 Agent（coder 角色） | `cargo test --locked -p storage-sqlite --all-features` | 同上；落盘目录为系统临时目录 `acpr-storage-*`，用例自清 | PASS / exit 0（13 个测试目标全部 `ok`；新增 `session_version_rule` 3 passed；含既有 2 条 `#[ignore]`——`crash_child`、`regenerate_v2_fixtures`） | `reports/wp3-storage-version.log` |
| PV3 / WP4 交付前 | `f582376` + RV1 修复批 | 跨 crate 契约：真实适配器（fake ACP 子进程四场景，含 `crash-on-prompt` 的 `turn.failed`）产出的 view 不含 `turnId`/`version`、其余 §10.3 最低字段齐备、`EndpointEvent.turn` 为 `None`；「已检查类型集合 = §10.3 两张表 − 显式豁免」的集合相等断言；core 独占类型不被适配器产出 | 主 Agent（coder 角色） | `cargo test --locked -p core -p agent-host --all-features` | 同上；agent-host 用例会 spawn fake ACP 子进程，用例与 `shutdown_all` 自清 | PASS / exit 0（core 94 passed；agent-host 53 passed，含新增 `view_contract` 1 passed） | `reports/wp4-agent-host-contract.log` |
| PV4 / DU1 候选与主分支 | 待候选（见 Merge History） | 统一入口：`npm run check` + fmt + clippy（workspace all-targets）+ 全工作区测试 | 待独立检查执行者 | `npm run verify` | 同上 | 待记录 | 待 `reports/du1-pv1.log`、`reports/du1-main-verify.log` |

交付前自检复核（tasks 3.2/3.3/3.4 的完成条件，由实现者执行并留证）：

- **3.2（PV2）**：log 含逐条命令与退出码；`cargo test -p core` 用例数由 78 增至 91（新增 **13** 条：10 条 broker + 3 条 model；RV1 修复批再 +3 broker → 94），无整体跳过、无零用例套件；既有用例的改动**只**是把「适配器视图里自造的 `turnId` 占位」去掉（真实适配器不产该字段，见 Check Plan Change 3 与 Failures F2），没有删除或弱化任何断言。
- **3.3（PV5）**：新增用例经真实 `SqliteStore`（非替身）打开临时库；反向验证思路：把 `session_store.rs` 的 `Update` 分支修复回退（不填 `origin_epoch`）→ `state_change_commits_bump_the_session_version_by_one` 立刻变红（`InvalidRequest("a session-scoped event requires an origin epoch")`）→ 恢复后变绿，证明该用例可证伪。
- **3.4（PV3）**：`view_contract` 用例使用真实 `AgentHost`/`SessionEndpoint` 与 fake ACP 子进程产出的事件（不是重新声明一份映射），并对「本次必须真实检查到的类型」逐项断言（缺任一即失败）；反向验证思路：给任一 view 加上 `turnId`、或从检查清单移除产物类型，用例即红。

## Check Plan Changes

实现期登记 4 条（原/新值、原因、影响与受影响任务；specs 与 plan 的覆盖索引已同步）：

1. **R1：imported 路径由「补齐 `turnId`」改为「保留 Owner 注入的取值」**（原：「imported 路径同样补齐…含顶层 `turnId`，取值等于该事件携带的 turn 归属」；新：「保留 Owner 给出的取值，本端不重写、不补齐、不伪造」）。原因：`deliver_imported` 的入参只有 `payload: Option<EventPayload>`，core 在 imported 路径上**没有**独立的 turn 来源；且无正文索引的 `payload_digest` 由调用方给出并覆盖 Owner 的视图字节（`storage-sqlite/src/session_store.rs` 对 imported 只绑定调用方摘要），重写 view 会让摘要与正文不再一致。所以「补齐」既不可实现也有害。风险覆盖：imported 视图的 `turnId` 由同一份 core 在 Owner 侧注入（Node Link 尚未实现，无历史兼容负担）；新增用例 `imported_deliveries_keep_the_owner_view_bytes` 覆盖「带身份字节逐字转发」与「缺失时不伪造」两个方向。受影响任务：2.4、2.7、plan 覆盖行 R4/R11（scenario 标题已同步）。
2. **R4：版本推导/比对的范围限定 + 漂移检测的时机写明**（原：「系统 SHALL 在提交前按…推导预期会话版本，并在提交后与存储层返回的版本比对」全覆盖；新：限定为「含 §10.3 要求 `version` 的 view 的提交」，并写明「比对发生在存储返回之后，本端不撤销已落盘的行」；R4 的第二个 scenario 由「该批事件不进入可重放状态」改为「不得把该批报告为成功」）。原因：① 对**每次**提交都推导需要额外读一次会话版本（事件批没有 `expected_version`），而实测 `expected_version` 为 `Some` 且无状态变更时存储层**不**校验版本，按它断言会产生误报，故只在该字段真正被写入的提交上推导/比对；② 存储层提交是原子的、没有回滚接口，失败关闭只能保证「不发布、不报成功」。风险覆盖：fake 存储的脚本化漂移用例（`a_version_rule_drift_fails_closed`）与真实存储的规则用例（PV5）共同覆盖；受影响任务：2.5、2.7。
3. **R7：把「适配器不返回 turn 标识」改为「返回占位值时归属仍由 core 决定」**（scenario 标题与 THEN 同步）。原因：`TurnAccepted.turn` 的类型是必填 `TurnId`（`crates/core/src/ports.rs`），「不返回」在当前端口形状下不可表达；新增用例用两种占位值（全零与任意 UUID）覆盖「不产生第二个 turn 行、id 与 view 的 `turnId` 都取 core 权威值」。受影响任务：2.5、2.6。
4. **WP3 实测发现的存储层缺陷与修复**（WP3 写范围由「仅 `tests/**`」扩为「+ `src/session_store.rs` 一处 1 行修复」）：`SessionStore::commit` 在 `StateChange::Update` 分支未像无状态分支那样回填 `origin_epoch`，导致「状态变更 + 会话级事件」同批提交被拒（`InvalidRequest("a session-scoped event requires an origin epoch")`）。这与 §5.2 的 `OwnedCommit.origin_epoch` 文档（「新建会话时由 core 生成并传入；存储层只校验已有 epoch 时必须一致」）矛盾，而且是 broker 的常规路径（终态事件与状态变更同批；`Broker::commit_owned` 只在建会话时传 `epoch`）。修复：`Update` 分支在校验通过后 `origin_epoch.get_or_insert(stored_epoch)`，与无状态分支一致；未改 DDL/`migrate.rs`、未改端口签名。证据见 Failures F1；受影响任务：2.9（文字已同步）。
5. **PV3 的通过判据按「适配器半 + core 半」的实测口径写明**（RV1-WP3/WP4 复核提出）：原判据写「适配器产出的 view **经 core 提交后**满足 §10.3」，但本次没有任何用例在 `agent-host` 侧驱动 core 提交（`agent-host` 无 fake store，把真实 `SqliteStore` 作为 dev 依赖会破坏 §5 的适配器隔离）。实际证据是两半：`view_contract` 断言适配器视图不含 `turnId`/`version` 且其余最低字段齐备，`crates/core` 的用例断言注入后 view 满足 §10.3 且 ACP 三要素不变。`plan.md` 的 PV3 段与 Main E2E 的 `alternative_checks` 已改写为这一口径；风险无变化（两部分都已执行）。受影响任务：2.10、PV3；**登记的非阻断项**：将来补一条「真实 `Broker` + 真实 `SqliteStore`」的组合用例（需先解决 fake store 的归属问题，建议放在 Sync/Daemon 切片）。

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

RV1（独立 review，WP1–WP4 交付前）：已执行。按 plan 的 WP 粒度拆成两个**隔离上下文**（`subagent` workflow `a268e610…` 启动失败为执行层错误、随后以阻塞式 workflow 重派成功；两子 Agent 均为 `oracle`、`context=fresh`，模型继承主会话）——上下文 A 覆盖 WP1+WP2（报告 `reports/rv1-wp1.md`，原文由主 Agent 逐字落盘）、上下文 B 覆盖 WP3+WP4（报告 `reports/rv1-wp3.md`）。两者结论均为**无未解决阻断项**（无 BLOCKER/MAJOR），共 **7 条 MINOR + 8 条 SUGGESTION**（A：5+4；B：2+4；计数由 RV1-REC 逐份校正）；两者都亲手做了变异自检（A 4 条、B 6 条）与全仓同类实例 `rg` 扫描。

| ID | Revision | Reviewer | Location | Severity / Impact | Resolution | Recheck Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| RV1-WP1-F1 | `f582376` | RV1-A（`oracle`，隔离上下文） | `crates/core/src/broker.rs` 注入条件 + `tasks.md`「实测：无」 | MINOR：turn 终结后晚到的 §10.3 类型事件无归属 → 不注入（降级），但记录写成「无该类路径」 | 已修：`tasks.md` 2.3 改写为如实登记该降级；新增用例 `a_late_delta_after_turn_end_is_persisted_without_attribution`；`docs/CORE_PORTS_AND_STORAGE.md` §6 第 19 条新增「无归属的降级」条目 | RV1-REC（`reports/rv1-wp2-recheck.md`）：确认文字与用例一致 |
| RV1-WP1-F2 | `f582376` | RV1-A | `crates/core/src/broker.rs:865`/`2288`（`submit_mode_set` → `apply_state`） | MINOR：真实 mode/config 流程里 `session.mode.changed` 与状态变更是两次提交 → 注入的是**变更前**版本（spec 字面满足，但 Sync 切片的乐观并发陷阱） | 不在本变更内改行为（会改提交批形状）；已在 `docs/CORE_PORTS_AND_STORAGE.md` §6 第 19 条与 `verification.md` 登记，留给 Sync 切片裁定（建议：合并为同一提交，或明确事件承载变更前版本） | 已登记（非阻断）；RV1-REC 复核登记位置 |
| RV1-WP1-F3 | `f582376` | RV1-A | `crates/core/src/broker.rs:2960-2971`（`view_command_completed`） | MINOR（**变更外既有缺陷**）：`command.completed.result.turnId` 实际承载会话版本（`apply_state` 传入 `Some(version)`） | 本变更不修（未触碰该函数；越界）；已登记为独立事项，建议 Sync/CLI 切片把 `result` 改为 `{"version":…}` 或 `null` | 已登记（非阻断） |
| RV1-WP1-F4 | `f582376` | RV1-A | `docs/CORE_PORTS_AND_STORAGE.md` 修订版本号 | MINOR：新增行用了已存在的版本号 0.7 | 已修：改为 `版本：0.11` | RV1-REC 复核 |
| RV1-WP1-F5 | `f582376` | RV1-A | `plan.md` PV2 判据 | MINOR：仍写「imported 补齐（R4/R11）」，与 Check Plan Change 1 相反 | 已修：改为「imported 保留 Owner 给出的取值（R4/R11；不注入、不重写、不补齐）」 | RV1-REC 复核 |
| RV1-WP3-B1 | `f582376` | RV1-B（`oracle`，隔离上下文） | `plan.md` PV3 判据 | MINOR：判据要求「经 core 提交后满足 §10.3」，但无该组合用例，且未登记为 Check Plan Change | 已修：改写 PV3 判据与 Main E2E 的 `alternative_checks`；补第 5 条 Check Plan Change；组合用例登记为后续项 | RV1-REC 复核 |
| RV1-WP3-B2 | `f582376` | RV1-B | `crates/agent-host/tests/view_contract.rs` 守卫清单 | MINOR：`user.message.delta`/`turn.cancelled`/`turn.failed` 声明覆盖但从不执行，产物消失可静默通过（已用变异证实） | 已修：加 `crash-on-prompt` 场景覆盖 `turn.failed`；守卫改为「已检查集合 == §10.3 两张表 − `NOT_EXERCISED`」的集合相等断言；`NOT_EXERCISED` 显式列出两个 fake 不产出的类型并写明原因 | RV1-REC 复核（含反向变异） |
| RV1-WP1-S1..S4、RV1-WP3-S1..S4 | `f582376` | RV1-A / RV1-B | 见两份报告 | SUGGESTION：NonText 集成用例、`expected_version` 快捷分支用例、§10.3 表多处手抄无门禁、失败批不重投未文档化、WP3 用例未覆盖「broker 组装」面、测试死代码、PV5 计数措辞、`permission.description` 为空 | 已处理：S1/S2 已补用例（`a_non_string_turn_id_fails_closed`、`an_event_only_commit_with_expected_version_keeps_the_current_version`）；S3/S5 登记为非阻断项；S4 已写入 §6 第 19 条；死代码与措辞已修；`permission.description` 属既有 fake 数据，不改（§10.3 只要求 string） | RV1-REC 复核 |

**reviewer 待补检查**：RV1-A 未执行 `npm run check`（PV1）、`-p storage-sqlite`（PV5）、`-p agent-host`（PV3）与全工作区测试（PV4），已在报告中声明；这些由主 Agent 在 PR1–PV4 留证，不影响本轮的代码正确性判断。RV1-B 未重跑 `-p core`（属 WP2 范围）。均已在报告中如实标注，主 Agent 按 Checks 表核对。

**复核轮 RV1-REC（tasks 3.9）**：新的隔离上下文（`oracle`、`context=fresh`）复核修复批 `f582376..27c9155`，执行 7 条变异（M1–M6，含「无归属事件强制注入 turnId」「fake 不再产 `tool.call.updated`」两个关键反向探针，全部真跑变红再还原）并独立复跑了 core 94 / `view_contract` 1 / `session_version_rule` 3 / storage clippy / `check-doc-links`。结论：**RV1 的实质发现全部闭合、生产行为零改动**，但提出 7 条「记录/登记不准」的 MINOR + 4 条 SUGGESTION（报告 `reports/rv1-wp2-recheck.md`）。

| ID | Revision | Reviewer | Location | Severity / Impact | Resolution | Recheck Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| RV1-REC-M1 | `27c9155` | RV1-REC（`oracle`，隔离上下文） | `docs/CORE_PORTS_AND_STORAGE.md` §6 第 19 条 | MINOR：修复批新引入的悬空引用 `§6.9`（本文档 §6 是编号列表，无子节；门禁按设计不拦无归属引用） | 已修为「§6 第 9 条」 | 本批后复跑 `npm run check` exit 0 |
| RV1-REC-M2 | `27c9155` | RV1-REC | `verification.md` F2 行 | MINOR：声称 F2 已写入 CORE 文档 §6 第 19 条，实际权威文档里没有该语义 | 已修：在 `docs/CORE_PORTS_AND_STORAGE.md` §10 未决项新增 `[open]` mode/config `version` 语义条目（含来源与未裁定前的约束） | 同上 |
| RV1-REC-M3 | `27c9155` | RV1-REC | `verification.md` F3 行 | MINOR：F3 只写「已登记为独立事项」而无落点 | 已修：`§10` 未决项新增 `[open]` `command.completed.result.turnId` 条目（含建议改法与待同步资产）；本文件 Final Assessment 的 Follow-up 同步引用 | 同上 |
| RV1-REC-M4 | `27c9155` | RV1-REC | `verification.md` Review Findings 汇总句 | MINOR：计数与两份报告不符（写 5+9，实为 7+8；且 WP3 无 S5） | 已修：改为 7 条 MINOR + 8 条 SUGGESTION，ID 段修为 `RV1-WP1-S1..S4`、`RV1-WP3-S1..S4` | 同上 |
| RV1-REC-M5 | `27c9155` | RV1-REC | `tasks.md` 3.5–3.9 | MINOR：任务完成条件里的报告路径（`rv1-wp2.md`/`rv1-wp4.md`/追加到 `rv1-wp*.md`）与实际产物不符 | 已修：3.5–3.8 改为实际文件名并写明「两个 WP 合并给同一上下文」，3.9 的产物写为 `reports/rv1-wp2-recheck.md` | 同上 |
| RV1-REC-M6 | `27c9155` | RV1-REC | `verification.md` 「验后影响判断」 | MINOR：改动路径枚举漏了 `crates/core/src/broker.rs`（虽全在 `#[cfg(test)] mod tests`） | 已修：枚举改为「`crates/*/tests/**` + `broker.rs` 的测试模块 + `docs`/`openspec`」并标明生产行为零改动 | 同上 |
| RV1-REC-M7 | `27c9155` | RV1-REC | `reports/rv1-wp1.md`/`rv1-wp3.md` | MINOR：5 处行号落到邻接行（结论均可按函数名复现） | 保留报告原文，已在 `reports/rv1-wp2-recheck.md` 的发现里附行号更正；本文件的「行号可复现性」段引用该更正 | 同上 |
| RV1-REC-S1..S4 | `27c9155` | RV1-REC | 见复核报告 | SUGGESTION：`view_contract` 注释仍写「三个场景」（已加第四场景）、豁免理由逐条写、`0.11` 放到版本块末尾、PV1 日志的 doc-links 计数是修复批提交前快照 | 已处理：注释改为「四个场景」+ `NOT_EXERCISED` 每项各一条原因注释（RV-DU1 复核发现首版只写了「见下」而无落点，已补）；`0.11` 移到版本块末行；PV1 行加注「最终树复跑见本行日志（367 links / 3017 refs / 155 md，exit 0）」；本批后已复跑 PV1/PV2/PV3 | 本批后复跑 `reports/wp1-contract-docs.log`、`wp2-core-injection.log`、`wp4-agent-host-contract.log` |

**验后影响判断**：修复批改的是 `crates/*/tests/**`、`crates/core/src/broker.rs` 的 `#[cfg(test)] mod tests`（+108 行全在测试模块内）、`docs/**` 与 `openspec/**`；**生产行为零改动**，因此 PV1/PV2/PV3/PV5 已按修复批复跑（上表 Revision 列注明）且结论未变。RV1-REC 用 7 条变异逐条证伪后确认：`view_command_completed`/`apply_state` 逐行未动、工作区 `git diff` 为空。

**行号可复现性**：两份 reviewer 报告保留原文逐字落盘；RV1-REC 抽查约 40 处 `file:line`，其中 5 处行号落到邻接行（`rv1-wp1.md` 的 `broker.rs:2569`→实为 `:2566`、`:2887-2889`/`:2935-2943`/`:2976-2984`→实为 `:2881`/`:2890`/`:2919`，`rv1-wp3.md` 的死代码行 `:270`→实为 `:271`）；结论均可按函数名复现，更正清单见 `reports/rv1-wp2-recheck.md`。

| RV-DU1-1 | `6856a6e` | RV-DU1（`oracle`，隔离上下文） | `verification.md` PV1 行 | MINOR：doc-links 计数写 `3017 refs / 155 md`，与三份日志及实测不符 | 已修：改为最终树（`6856a6e`）实测 `367 links / 3058 refs / 156 md` | 复跑 `node scripts/check-doc-links.mjs` exit 0；三份日志一致 |
| RV-DU1-2 | `6856a6e` | RV-DU1 | `verification.md` 3.2 自检行 | MINOR：用例增量「9 broker + 3 model」与 78→91（+13）不平 | 已修：改为「13 条（10 broker + 3 model）」，并注明修复批 +3 → 94 | 与 `wp2-core-injection.log` 的 94 passed 一致 |
| RV-DU1-3 | `6856a6e` | RV-DU1 | `verification.md` F2 行 | MINOR：Retest Evidence 写 91 passed，与同文件 PV2 行（94）矛盾 | 已修：注明「首轮 91，日志已被修复批复跑覆盖为 94（见 PV2 行）」 | 同上 |
| RV-DU1-4 | `6856a6e` | RV-DU1 | `crates/agent-host/tests/view_contract.rs` + `verification.md` RV1-REC-S1 行 | MINOR：`NOT_EXERCISED` 的「逐条原因见下」无落点（R1-REC-S1 的闭合声明未落实） | 已修：每项各补一条原因注释；`verification.md` 的措辞同步 | 注释为文档/测试面，改动后 `cargo clippy --workspace -D warnings` 与 `npm run check` exit 0（见本批提交的 pre-commit 输出） |
| RV-DU1-S1 | `6856a6e` | RV-DU1 | `verification.md` 行号段 | SUGGESTION：「更正表」实为行内清单 | 已修：改为「行号更正清单」 | 同上 |
| RV-DU1-S2 | `6856a6e` | RV-DU1 | `docs/CORE_PORTS_AND_STORAGE.md` §6 第 19 条 | SUGGESTION：「§6 第 9 条」与本句口径不完全一致 | 已修：改为「本条 ① 的失败语义（与 §6 第 9 条同口径）」 | `npm run check` exit 0 |

**RV-DU1 结论**：`reports/rv1-du1.md` —— 0 BLOCKER / 0 MAJOR，无未解决阻断项；区间完整性、快进一致性、证据自洽、修复批零生产代码行均已核实。

**RV1-REC 的 7 条 MINOR（M1–M7）闭合依据**：均为记录/引用类修正，依据 `npm run check` exit 0（含 `check:doc-links`、`check:contract-drift`、`check:agentic`）、`openspec validate core-turn-view-fields --strict` = valid，以及 RV-DU1 的逐条复核（已在 `reports/rv1-du1.md` 第 3 节确认 (a)–(g) 全部闭合）；未再单独开第四轮独立复核（非阻断、零生产代码改动），该判断本身已由 RV-DU1 独立验证。


## Merge History

- 集成方式：`integrated`（单一交付单元 DU1，WP1–WP4）。独立性：候选构建/PV4 与主分支 PV4 各由一个隔离上下文执行（非实现者），候选与合并差异另由一个隔离 reviewer 检视（RV-DU1）。
- 基线：`1cd0416` = `refs/heads/main` = `origin/main`（合入前，未移动）。
- 候选：`6856a6e`（`feat/core-turn-view-fields` tip；区间 `1cd0416..6856a6e` = 4 个提交 / 0 merge / 21 files / +2876 −35 / 无 `Cargo.lock` 变化）。
- 候选 PV4：`npm run verify` exit 0（另一个隔离上下文；`reports/du1-pv1.log`，pin `6856a6e`；workspace 398 passed / 0 failed / 2 ignored；`cargo build --locked --workspace --all-features` exit 0）。
- 合入：`git merge --ff-only feat/core-turn-view-fields`（fast-forward，无合并提交；`HEAD == refs/heads/main == 6856a6e48e331a62501c020eeba4dfe0d7ec4e84`）。仅本地引用变更，**未** push（`origin/main` 仍为 `1cd0416`）、未打 tag、未发布。回退路径：`git reset --hard 1cd0416`。
- 主分支 PV4：`npm run verify` exit 0（第三个隔离上下文；`reports/du1-main-verify.log`，pin `6856a6e`，前后置 `git status` 为空；计数与候选日志逐目标一致，剔除耗时后 diff 为空）。
- 合并差异检视：`git diff --stat 6856a6e HEAD` 与 `git diff --stat 6856a6e feat/core-turn-view-fields` 均为空，`git merge-base --is-ancestor 6856a6e HEAD` = 0；RV-DU1 独立复核确认快进未引入候选之外的差异，且修复批为**零生产代码行**改动。
- 详述：`reports/du1-integration.md`。
- 合入后的记录类修正（RV-DU1 的 4 MINOR + 2 SUGGESTION）与最终修订、替代验证证据见下方 Final Assessment。 Agent 执行，独立检查与 review 仍由隔离上下文承担（见 Review Findings 与 Checks 的 PV4 段）；此处按 schema 允许的串行方式如实记录，待 5.1/5.2 复核后补齐实际 ID 与报告路径。
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

## Final E2E（替代验证）

- 判定：`not-applicable`（理由、依据与用户批准原话见 `plan.md` 的 `## Main E2E`；`openspec-agentic e2e --json` = `enabled=true, command=""`）。
- 替代验证（tasks 7.1/7.2）在**最终主分支修订 `c58c0a0`** 上执行 5 条命令，全部 exit 0：`cargo test --locked -p core --all-features`（94 passed）、`cargo test --locked -p storage-sqlite --all-features`（12 个测试目标全 ok，含 `session_version_rule` 3 passed；2 ignored 为既有）、`cargo test --locked -p core -p agent-host --all-features`（agent-host 合计 53 passed，含 `view_contract` 1）、`npm run verify`（十道门禁 + fmt + clippy + 全工作区 398 passed / 0 failed / 2 ignored）、`npm run check`（十道门禁单跑）。
- 证据：`reports/alt-final-verification.log`（逐条命令原文、退出码、计数）与汇总 `reports/alt-final-verification.md`（四项 `alternative_checks` 覆盖核对、资源清理、资产哈希不变）。
- 资源核对：无临时 SQLite 残留、无遗留 `acpr-fake-acp-agent`/cargo/rustc 进程、未新增 `CARGO_TARGET_DIR`；`git status --porcelain` 前后均为空 ⇒ `fixtures/**`、`compatibility/acp/v1/matrix.json`、`schemas/**`、`Cargo.lock` 与 HEAD 逐字节一致。

## Failures and Retests

| Issue ID / Task | Source / Check or E2E ID / Attempt / Version | Owner | Fix / Recovery | Review / Readiness Evidence | Retest Evidence | Current Status / Basis |
| --- | --- | --- | --- | --- | --- | --- |
| F1 / 2.9 | WP3 首轮 `cargo test -p storage-sqlite --test session_version_rule`（`f582376` 之前的工作区）→ `state_change_commits_bump_the_session_version_by_one` FAIL：`InvalidRequest("a session-scoped event requires an origin epoch")` | 主 Agent（coder 角色） | 修 `crates/storage-sqlite/src/session_store.rs` 的 `Update` 分支：校验通过后 `origin_epoch.get_or_insert(stored_epoch)`（与无状态分支一致，符合 §5.2） | 待 RV1-WP3 复核（Check Plan Change 4） | `reports/wp3-storage-version.log`（3 passed，含该用例）；回退修复即变红（见 3.3 自检） | 已解决（待独立复核） |
| F2 / 2.2–2.7 | WP2 首次 `cargo test -p core`：17 条既有用例 FAIL，原因是测试自造的适配器视图带占位 `turnId`（`uuid_text(0)`）与新的冲突校验（R2）冲突 | 主 Agent（coder 角色） | 按「适配器不产 `turnId`」的事实更新既有测试数据（`turn_view` 辅助函数、delta/permission/thought 视图去掉自造字段），并新增注入断言；未删除或弱化任何断言 | 待 RV1-WP1 复核 | `reports/wp2-core-injection.log`（首轮 91 passed；该日志已被 RV1 修复批复跑覆盖为 94 passed，见 PV2 行） | 已解决（预期变化，RV1-WP1 已独立复核） |
| F3 / 2.7 | WP2 `replayed_commits_do_not_reinject_view_fields` 首轮 FAIL：用例把注入版本写死为 `"1"`，实际是运行期版本 | 主 Agent（coder 角色） | 改为与「当时会话版本」比较（`session.version()`），并让脚本不带终态事件以避免版本在断言期间变化 | 待 RV1-WP1 复核 | `reports/wp2-core-injection.log` | 已解决（用例自身修正，待独立复核） |
| F4 / 3.4 | `crates/agent-host/tests/view_contract.rs` 首轮编译失败：`SessionBackendFactory` 未导入、`Vec<String>::contains(&str)` 类型不符 | 主 Agent（coder 角色） | 导入 trait、改用 `iter().any(...)` 比较 | 待 RV1-WP3 复核 | `reports/wp4-agent-host-contract.log`（1 passed） | 已解决（编译期修正） |

## Final Assessment

- Assessment ID / Time：`A1-core-turn-view-fields-2026-09-24`（执行者：主 Agent，按 `.agents/skills/agentic-verify/SKILL.md` + `openspec/schemas/agentic/procedures/acceptance.md`）
- Target / Task：本地 `refs/heads/main`（`HEAD == refs/heads/main`，工作区无被跟踪文件改动，实测提交见下方 `agentic-assessment` 块的 `target_commit`）/ 最终验收任务 = tasks 8.1（唯一的 `[final-verification]` 标记行）
- CLI State：`openspec status --change core-turn-view-fields --json` → `schemaName: agentic`、`isComplete: true`（CLI 完成状态只记录并引用，不改写其含义）；`npx --quiet --no-install openspec-agentic e2e check --change core-turn-view-fields` → `PASS core-turn-view-fields: not-applicable，批准字段非空（用户来源须独立核实）；已按 [e2e-owned] 勾选最终 E2E 任务` + `E2E check: PASS`（该行状态由扩展写回，非主 Agent 手勾）。
- Audit / Evidence（逐审计组结论）：
  - **Contracts and Coverage：PASS**。`proposal.md` 的 Intent and Constraints（含两条用户原话与 `decision_bounds`）、非目标与成功判据与实际交付一致：改动只落在 `crates/core`（注入/失败关闭/version 推导）、`crates/storage-sqlite`（1 行 epoch 回填 + 用例）、`crates/agent-host`（契约用例 + 注释）、`docs/**` 与变更记录；无新依赖、无 DDL/端口/wire/schema 变化（区间内 `Cargo.lock` 无变化）。`plan.md` 的 Coverage Index 22 行（7 Requirement + 15 Scenario）逐项有任务、Check ID 与原始证据；范围/语义澄清已按 **Check Plan Changes 1–5** 记录并同步 spec、plan、tasks 与复核结论。
  - **Delivery and Versions：PASS**。交付单元 DU1（`integrated`，WP1–WP4）；基线 `1cd0416`（= 合入前 `refs/heads/main` = `origin/main`）→ 候选 `6856a6e` → `git merge --ff-only` 合入本地 `main` → 主分支复核。候选与主分支的 PV4 由两个不同的隔离上下文执行（`reports/du1-pv1.log`、`reports/du1-main-verify.log`，均 pin `6856a6e`，计数一致）；合并差异由第三个隔离 reviewer 检视（`reports/rv1-du1.md`）。无竞态：三段执行时间与 `refs/heads/main` 取值逐段核对一致（`reports/du1-integration.md` §3/§4）。
  - **Project Checks and Resources：PASS**。PV1/PV2/PV3/PV4/PV5 均以原始命令、目录、退出码 0 与日志留证（`reports/wp1-contract-docs.log`、`wp2-core-injection.log`、`wp3-storage-version.log`、`wp4-agent-host-contract.log`、`du1-pv1.log`、`du1-main-verify.log`、`alt-final-verification.log`）；统一入口 `npm run verify` 覆盖十道合同门禁 + fmt + clippy(`-D warnings`) + 全工作区 398 passed / 0 failed / **2 ignored 为既有 `#[ignore]`**；未约定跳过的强范围（无 `--workspace` 之外的削弱，未新增 ignore）。资源：无临时 SQLite 残留、无遗留 fake ACP/cargo/rustc 进程、未新增 `CARGO_TARGET_DIR`，`fixtures/**`、`compatibility/acp/v1/matrix.json`、`schemas/**`、`Cargo.lock` 执行前后与 HEAD 一致。`cargo-deny`/`gitleaks`/`commits` 仅 CI 可判，本文件与交付说明均**未**声称本地通过。
  - **Independent Reviews：PASS**。RV1（两个隔离 `oracle` 上下文：`reports/rv1-wp1.md` 检视 WP1+WP2 于 `f582376`、`reports/rv1-wp3.md` 检视 WP3+WP4）、RV1-REC（`reports/rv1-wp2-recheck.md`，复核 `f582376..27c9155`，含 7 条变异自检）、RV-DU1（`reports/rv1-du1.md`，检视 `1cd0416..6856a6e`）：**0 BLOCKER / 0 MAJOR**，故无待闭环的 CRITICAL/MAJOR；共 11 条 MINOR + 14 条 SUGGESTION 已逐条处置（`## Review Findings` 表）并留下依据；每组均记录 base/target、上下文隔离、报告路径与结论。
  - **E2E Design and Execution：NOT_APPLICABLE（合规）**。`plan.md` 记 `not-applicable`，含 `reason`/`basis`/4 条非空 `alternative_checks` 与可追溯到用户原话的 `downgrade_approval`（「同意本变更 Main E2E 记 not-applicable」）；`openspec-agentic e2e --json` 显示 `enabled=true, command=""`（无产品级 E2E 命令），`e2e check` 判 PASS。替代验证按验收要求**另列**为 tasks 7.1/7.2（主 Agent 任务，未并入 `[e2e-owned]` 行），在最终主分支修订 `c58c0a0` 上执行 5 条命令全部 exit 0（`reports/alt-final-verification.log` / `.md`）；`[e2e-owned]` 的 7.3 由扩展在 `e2e check` 判 PASS 后自动勾选。无 mode 变更或不因失败而降级的历史。
  - **Issue Closure and Evidence Validity：PASS**。`## Failures and Retests` 的 F1–F4 与两级 review 的每条发现均关联原问题 ID、责任人、修复版本与复测证据；反向探针（变异自检）由独立上下文执行并记录了变红/还原过程。证据适用性：`du1-pv1.log`/`du1-main-verify.log` pin `6856a6e`，替代验证 pin `c58c0a0`；其后 `b14e1b2`（替代验证记录）、`57553a1`（`e2e check` 勾选 7.3）与本轮验收提交仅动 `openspec/**` 记录与任务框，属无行为面变化的记录类差异，按既有「验后影响判断」口径复用并在此明示，不重复复制测试。
- Result / Open Issues：**PASS**（本轮目标提交见下方块）。阻断项：无（0 CRITICAL / 0 MAJOR、无失败/受阻 PV、无未完成任务——仅验收任务 8.1 在勾选前保持待办）。非阻断未解决项（已在权威文档登记，留待后续切片）：`docs/CORE_PORTS_AND_STORAGE.md` §10 的两条 `[open]`（mode/config `version` 语义、`command.completed.result.turnId` 承载版本）、`reports/rv1-wp1.md`/`rv1-wp3.md`/`rv1-wp2-recheck.md`/`rv1-du1.md` 的 SUGGESTION 级记录（`RV1-WP1-F6..F9`、`RV1-WP2-F3/F6`、`RV1-REC-S2/S4`、`RV-DU1-S1/S2` 已在本轮处置或登记）。
- Required Follow-up：无待复验项；归档按 `.agents/skills/agentic-verify/SKILL.md` 与归档流程另行执行（本验收不授权推送、回滚、发布或归档）。PASS 仅对本轮目标提交及上述有效证据成立；若目标提交或证据再变化，须重新验收。

```agentic-assessment
assessment_id: "A1-core-turn-view-fields-2026-09-24"
target_commit: "PENDING"
contract_digest: "sha256:95f6cc643e0f9928afbef37be794c5046c4d8480da16feba31f05d960f9179a8"
result: PASS
evidence:
  - path: reports/wp1-contract-docs.log
    sha256: "sha256:350351535a28d8f3125232e174d01eedb8682312311c9ea3f84ac4e0acdd7ac0"
  - path: reports/wp2-core-injection.log
    sha256: "sha256:5631fde87d854b60cd030b548f6639cff8fd4d0aad19216592dbdf9d68ff670d"
  - path: reports/wp3-storage-version.log
    sha256: "sha256:594f8b3501628bcd9f29f2e5e793763a21857f6a9caa76b2a605e077ca4aff1c"
  - path: reports/wp4-agent-host-contract.log
    sha256: "sha256:e65d48e036fb54f992b40029fcde96864ef2aee3e1aad7544af77da6c750736a"
  - path: reports/du1-pv1.log
    sha256: "sha256:34c890c28efcd3aa645c528590158de681793e7255d8cc91ca2ae3d932632827"
  - path: reports/du1-main-verify.log
    sha256: "sha256:445453c16d68ff8bd54751ec65ab446389e7e497ddd44ce98781c1afaceb7d9f"
  - path: reports/du1-integration.md
    sha256: "sha256:f616cd19f3f730a391321c86be2baed02cac957a60265cccdf5fc547908f7878"
  - path: reports/alt-final-verification.log
    sha256: "sha256:f14ad13efde0434db000998e6bdbf3617bef907a21e6ff55706545a9f9e9c3d7"
  - path: reports/alt-final-verification.md
    sha256: "sha256:7c161db159fc758a19e95e0cf9c0fa53d09acd7db232e71fc9dc9d48e94a3641"
  - path: reports/rv1-wp1.md
    sha256: "sha256:cdaffa40e464d4d52ee835d90441701f0c8a203cc9f9cc378838ae5a93f7f9f1"
  - path: reports/rv1-wp3.md
    sha256: "sha256:3bc027b555fd76ce0cc5e2032716c718f684f1ca77d007d455cdc913fc491746"
  - path: reports/rv1-wp2-recheck.md
    sha256: "sha256:7347f3a5beb522151457526340608590b2fe36046b93c21aaa66e776f6fe2993"
  - path: reports/rv1-du1.md
    sha256: "sha256:12dc8da1f436f0ef65911ddb624379886a20086ced186f6013ead3e1c61d2bbb"
```
