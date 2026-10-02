# WP6 实现报告（coder-F2）

## Shared Report

- **task_id**: `2.6`
- **role**: coder
- **phase**: fix（本实例是 `coder-F` 被宿主终止后的**接手实例** `coder-F2`；交付同时含首轮的未提交实现与本轮的修复）
- **agent_context**: 子 Agent `coder-F2`，由主 Agent 在同一 worktree、同一上下文起点派发；未继承 `coder-F` 的实现对话，只拿到「现场事实 + 义务清单」。**因此本报告对上一实例的评估基于代码与 diff 的独立阅读，不是同一记忆的延续。**
- **target_revision**: `76ab01126114294e7064e2618eedad0615b2561d`（短 sha `76ab011`）
- **scope**: `crates/server/`、`crates/app/`、`docs/NODE_LINK_PROTOCOL.md`（仅 §12.7 的 `:662` 收窄句与 §12.7 示例注记）、`docs/SESSION_CONTINUITY_DESIGN.md`、`README.md`（「仓库当前状态」）、`docs/DEVELOPMENT_PLAN.md`。未写任何禁止区文件。
- **changes**: 10 个文件，+972 / −71（含上一实例的未提交改动，全部纳入本次交付提交 `76ab011`）
- **checks**: PV1 阶段 1（`fmt` / `clippy -p server -p app` / `test -p server -p app`）全绿；PV2（`npm run check`）绿；补充取证（workspace `clippy --keep-going`）绿
- **issues**: 自测发现的 4 类缺陷已全部修复（见「对上一实例现场的评估」）；无未闭环的实现缺陷
- **result**: **PASS（本工作包 PV1 阶段 1 + PV2）**；**待独立 reviewer（CR6）复核**——本报告的自测不构成 CR6 结论
- **evidence_paths**: 本报告 `reports/wp6-coder.md`；日志 `reports/wp6-coder-f2-*.log`（未覆盖上一实例的 `reports/wp6-coder-workspace-clippy-keepegoing.log`）
- **resource_cleanup**: 见「资源释放」

---

## 一、对上一实例（coder-F）现场的评估

### 1.1 派发简报的前提与实测的差异

派发简报说 coder-F 死时「正在修 `crates/server/src/node_link/command/tests.rs` 里的一个多余 `}`」，并说红窗口有 8 条（`E0046`×7 + `E0004`×1）。**实测与该前提不符**，记录如下以便主 Agent 校正后续简报：

| 项 | 简报 | 实测 |
| --- | --- | --- |
| HEAD | `5ab7e9d` | `5ab7e9d` ✅ |
| 未提交文件 | 5 | 5 ✅ |
| 未提交规模 | 约 +935 / −56 | +935 / −56（`git diff --stat` 逐文件吻合）✅ |
| 语法错误（多余 `}`） | 存在，正在修 | **不存在**。接手后第一条命令 `cargo check -p server -p app --all-targets --all-features` 即 **exit 0**；紧接着的 `cargo clippy -p server -p app --all-targets --all-features -- -D warnings` 也 **exit 0** |
| 8 条红窗口（`E0046`×7 + `E0004`×1） | 存在 | **已全部消失**。`E0046` 的五条 `load_recovery` 替身与 `core_payload()` 的 `SessionResume` 早退臂在未提交改动里**已经写好了** |

结论：**现场没有损坏，可以直接接手。** coder-F 是在「已经修好编译」之后、开始跑 `cargo test` 的阶段被终止的（它的 `wp6-coder-workspace-clippy-keepegoing.log` 是中途状态，3 个 error 里两个是编译中断造成的截断）。因此**没有**触发「重写」路径，本实例一行实现都没有推倒重来。

### 1.2 逐文件：保留 / 修正

| 文件 | 处置 | 说明 |
| --- | --- | --- |
| `crates/server/src/node_link/command.rs`（+408） | **原样保留，未改一行** | 逐段复核后确认它已经正确实现了 R22/R27–R37、`DR1-F41`（`session_resume_result` 与 `session_create_result` 共用 `remote_session_ref_and_meta`）、`CR3-F1`（`create_session` 提交后失败改结 `uncertain`，判据是「有没有持久 `owned_command` 行」）、`DR1-F39`（五类确定类失败一律 `failed`）、`DR1-F48` Path A（`core_payload` 早退）、`DR1 Round 9`（`port_error_code` 的 `BackendUnsupported` → `nodelink.command.unsupported` 臂）。见 §三 的需求映射 |
| `crates/server/src/node_link/command/tests.rs`（+515） | **保留 8 条用例，修正 6 处缺陷，删除 1 个调试残留** | 缺陷见 §1.3 |
| `crates/server/src/local_admin/test_support.rs`（+23） | **原样保留** | `NotTouched`/`FixedStore` 的 `load_recovery` + `NotTouched` 的 `resume` |
| `crates/server/src/node_link/resource/tests.rs`（+7） | **原样保留** | `SliceStore::load_recovery` |
| `crates/app/tests/support/owner.rs`（+38） | **保留语义，改写 1 处** | `FlakySessionStore::load_recovery`（原样转发，故障注入只作用在 `commit`）、`ScriptedBackends::resume`（回带持久化的 `agent_sessionId`）、`ScriptedEndpoint::agent_session_id`。**改写**：`create` 的实现点漏了新增的 `agent_session_id` 字段——coder-F 写了 `resume` 侧的构造却漏了 `create` 侧（`owner.rs` 的 `ScriptedEndpoint { reference, sink, parked }` 两处构造），这是编译错误，**本实例补齐为 `agent_session_id: None`** |
| `crates/server/src/node_link/conn/tests.rs`（+4） | **本实例新增** | 见 §1.3 缺陷 4 |

### 1.3 coder-F 留下的 6 处缺陷（本实例修复）

这 6 处都是**测试侧**的缺陷，**不涉及任何契约或设计判断**，修掉即可：

1. **`session_submit_body` 的 `scope` 白名单漏 `session.resume`**（`command/tests.rs`）。该辅助函数按「是否会话范围命令」决定 `sessionRef`/`attachmentId`/`attachmentGeneration` 给不给值；`session.resume` 是会话范围命令（`NODE_LINK_PROTOCOL.md` §12.7「`sessionRef`/`attachmentId`/`attachmentGeneration` 必须为指向该 Export 可见会话的非 `null` 值」），但白名单没列它，于是 8 条新用例全部在信封层被判 `nodelink.protocol.schema_invalid`，**根本没走到路由**。这是接手后第一次 `cargo test -p server --lib` 8 条新用例全红的根因。修法：把 `"session.resume"` 加进 `scope` 匹配臂。
2. **两处 `error_code(terminal[0])` 取错路径**。`error_code` 辅助函数读的是 `body.error.code` / `body.code`（`command.rejected` / `link.error` 的形状），而 `command.terminal` 的错误在 `body.terminal.error.code`。coder-F 用它断言终态错误码，`expect("错误码")` 直接 panic。改为显式断言 `body["terminal"]["error"]["code"]`（与既有 `session.create` 用例在 `tests.rs` 中对 `body["terminal"]["error"]["code"]` 的写法一致）。
3. **成功用例断言了不存在的字段**。coder-F 断言 `result["sessionMeta"]["sessionId"]` 与 `result["sessionMeta"]["agent"]["agentId"]`；实际 wire 的 `SessionMeta` 只有 `{ state, version }`（`crates/node-link-protocol/src/common.rs:409`）。改为断言 `sessionMeta.state == "idle"`。
4. **`conn::tests` 的 fixture 归类门禁红**。WP2 新增了 `fixtures/node-link/v1/invalid/session-resume-with-cwd.json`（`manifest.json` 中 `valid: false`），`node_link::conn::tests::fixtures_are_consumed_by_the_handshake_and_error_layers` 要求每个 `valid: false` 的业务族 fixture 显式登记在「body 级反例」清单里（该清单的注释原文：「body 级判定归 WP5/WP6」）。本实例按该登记的归属把它加进 `deferred_body_cases`，并写明它在本层的落点（`command.rejected(nodelink.command.unsupported_field)` + `details.field`）。
5. **调试残留**。文件末尾留了一个 `zz_debug_resume`（无条件 `panic!` 的取证用例，命名与断言风格也不合规）。**已删除**——它不是回归测试，且会无条件失败。
6. **格式**。`let body = \|payload: Value\| {        json!({` 是 coder-F 编辑中断留下的同行残片（`cargo fmt --check` 报错处）。已修正；随后 `cargo fmt --all` 统一了 `app/tests/support/owner.rs` 的 import 折行。

另外，本实例**新增**了两处同文件内的测试基础设施修正（均不改变断言语义）：

- `Workspace` 守卫：恢复用例需要在真实目录上跑 core 的 `canonicalize` 复校验，`canonical_workspace` 原本裸建临时目录不清理。改为返回值对象 + `Drop` 删目录（`remove_dir_all` 的错误有意忽略，因为「目录被删」的用例会先自己删掉）。
- `conn::tests` 的 `deferred_body_cases` 补登（上文缺陷 4）。

### 1.4 结论

**coder-F 的实现是可用的、高质量的**：它把 6 条义务里最难的 4 条（`DR1-F41` 投影同源、`CR3-F1` 的 `uncertain` 判据、`DR1-F39` 的终态唯一、`DR1-F48` Path A 早退）都想清楚了，注释把「为什么不能用 `failed` 撒谎」写在了代码旁边。缺陷全部集中在**新测试的构造细节**上——典型的是「用例跑不到被测代码」。本实例因此只做「评估 + 修缺陷 + 补文档」，没有推翻任何设计。

---

## 二、交付与依赖

- **base（起点）**: `5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3`（U1 集成基线，含 WP1–WP5）
- **target（交付）**: `76ab01126114294e7064e2618eedad0615b2561d`（短 `76ab011`）
- **提交数**: 1（含 coder-F 的全部未提交改动；无分离的「修复轮」提交，因为接手时改动尚未提交，不存在可拆的历史）
- **worktree / 分支**: `D:/Project/acp-remote-wt/session-resume-wp6` / `agentic/session-resume-wp6`（未切分支、未新建 worktree、未 `reset`/`checkout` 任何未提交改动）

### 依赖包含关系（`git merge-base --is-ancestor <sha> 76ab011`，实测）

| 上游交付 | 结果 |
| --- | --- |
| `248d9b9`（WP1 `acp-protocol`） | **ANCESTOR-YES** |
| `32f71f5`（WP2 封闭词表原子点） | **ANCESTOR-YES** |
| `4f7a2355`（WP3 `core` 恢复语义） | **ANCESTOR-YES** |
| `4a3882d`（WP4 `storage-sqlite` v5） | **ANCESTOR-YES** |
| `1376e1b`（WP5 `agent-host` 恢复后端） | **ANCESTOR-YES** |

---

## 三、改动文件 ↔ 需求 / 义务映射

### 3.1 需求行（`plan.md` Coverage Index 实际行号）

WP6 行声明的需求是 **R22、R27–R37**。逐条落点（`crates/server/src/node_link/command.rs`，除注明外）：

| R 行（spec 标题） | 落点 | 状态 |
| --- | --- | --- |
| R22（`storage-schema`「恢复流程不覆写持久化取值」） | `on_session_resume` 全程**不发起任何 `StateChange`**：它向 core 提交的只有 `resume_session` 内部的 `accepted` 幂等行（测试替身 `CommandStore::commit` 的 `commit.state.is_none() && commit.command_terminal.is_none()` 分支即证明这点：只有幂等行落盘），因此 `agent_session_id`/`workspace_cwd` 两列不被覆写（该行的主要落点在 WP4 的 `storage-sqlite`，此处是适配层不越界） | ✅ |
| R27（`node-link-owner-server`「命令授权、幂等与终态」） | `on_session_resume` 步骤 ②③④⑤⑦ `:985–1201` 的完整管线（授权 → attachment → 幂等 → `accepted` → 终态） | ✅ |
| R28（相同 `requestId` 重试不重复派发） | 步骤 ④ `command_status` 回读：已终结直接 `send_terminal`，未终结只重发 `accepted` 并 `watch`，**不再调 `resume_session`**。用例 `a_repeated_session_resume_replays_the_first_result` | ✅ |
| R29（同键不同语义被拒绝） | 步骤 ④ `record.command()/kind()/request_fingerprint()` 三重比对失配 → `nodelink.command.idempotency_conflict`；core 侧的 `PortError::Conflict(IdempotencyConflict)` 同样映射。用例 `the_same_request_id_with_a_different_command_conflicts` | ✅ |
| R30（越权命令被拒绝） | 步骤 ② `authorize_node(..., "grant.remote-work", ...)`（`:1002`）→ `deny(...)` 早退 | ✅ |
| R31（越权恢复被拒绝且先于本机读取） | 步骤 ② 在 `attachment_is_current` 与 `resume_session` **之前**；`session_target`（`:1210`）把「会话不存在」与「会话存在」合并为同形响应。用例 `an_unauthorized_session_resume_is_rejected_before_any_local_read` | ✅ |
| R32（崩溃窗口进入 `uncertain`） | 步骤 ⑦ 只有 `LocalOutcome::Uncertain`（**仅**投影失败，`:1097`）写 `Uncertain`；无持久记录时 `local_terminal` 仍以记录为唯一权威回读。`uncertain` 在本路径**不**用于任何确定类失败 | ✅ |
| R33（命令限流） | 步骤 ④ 末尾 `admit_in_flight(handle)`（与 `session.create` 同一入口），超限直接 `RouteOutcome::Claimed` 不入管线 | ✅ |
| R34（`session.resume` 的 `payload` 与结果契约） | `forbidden_payload_field` 的 `CommandName::SessionResume => true` `:2062`（**严格解码之前**，`on_submit` 的第一步）+ `completed_result` 的 `SessionResumeResult` 臂 `:2200` + `accepted` 必须为 `null` `:1542` | ✅ |
| R35（正常恢复并回传会话引用） | `session_resume_result` `:926`（与 `session_create_result` **同源**，共用 `remote_session_ref_and_meta` `:943`）+ `settle_session_resume` 落终态 `:1133`。用例 `session_resume_returns_accepted_then_the_composite_result` | ✅ |
| R36（携带字段被拒绝且不启动进程） | `forbidden_payload_field` + `reject_field`（`details.field`）。用例 `session_resume_with_any_payload_field_is_rejected_without_side_effects` | ✅ |
| R37（Agent 不支持恢复时终态失败） | `port_error_code`（`:2373`）的 `BackendUnsupported` 臂（`:2381`）→ `nodelink.command.unsupported` + `LocalOutcome::Failed`（`:1123`，**不是** `uncertain`，DR1-F39）。用例 `session_resume_without_persisted_recovery_data_fails_as_unsupported` | ✅ |

WP6 之外的**关联**行（主要落在 WP3/WP4/WP5，本包只做适配层不破坏它们）：R23–R26（`workspace-resolution` 的目录复校验；本包用例 `session_resume_with_a_deleted_workspace_fails_as_unavailable` 从路由侧断言错误码为 `nodelink.internal.unavailable` 而非 `unsupported`）、R5–R12（`local-agent-host`；本包不碰进程模型）、R1–R4（`acp-wire-protocol`）。

### 3.2 六条义务逐条

| 义务 | 状态 | 证据 |
| --- | --- | --- |
| **①路由 `session.resume`（R22、R27–R37）** | ✅ 完成 | `on_submit` 的分发臂 `:265` → `on_session_resume` `:985–1201`：定位 → 授权 → attachment → 幂等 → `accepted(null)` → `resume_session` → 投影 → `settle_session_resume` → 终态回包。测试见 §3.3 |
| **②组合根接线（`crates/app/` 接真实 `SqliteStore` + `AgentHost`）** | ✅ **no-op（经核实）** | 派发简报把这条写成「要接线」，但实测**不需要任何 `crates/app/src/**` 改动**：`app::compose` 装配的是 `SqliteStore`（WP4 已实现 `load_recovery`）与 `AgentHost`（WP5 已实现 `resume`），`core::broker` 的依赖注入是 trait 对象，端口签名由 WP3 冻结、`app` 只是消费者。接线在组合根层面**已由上游闭合**。WP6 在 `crates/app/` 内实际要做的只有**编译涟漪**（`app/tests/support/owner.rs` 的两个替身），已完成。**这是本轮唯一需要主 Agent 留意的「义务与实际不符」项**（见 §六 待澄清问题 1） |
| **③收口编译涟漪（8 条红窗口）** | ✅ **8/8 全清** | 见 §3.4 |
| **④`（CR3-F1）create_session` 提交后失败结 `uncertain`** | ✅ 完成 | `command.rs:760–785`：`create_session` 返回错误时先 `command_status` 回读；**有**持久 `owned_command` 行（= 会话行已在同一次提交里落盘）→ `Uncertain`（`:781`）；**无**行（会话行确实没提交）→ `Failed`。注释写明了为什么这与投影失败分支同判据 |
| **⑤`（DR1-F41）`SessionResumeResult` 投影 + `settle_session_resume`** | ✅ 完成 | `session_resume_result` `:926` 与 `session_create_result` 共用 `remote_session_ref_and_meta` `:943`（**同源**，`remoteSessionRef.exportId` 只有适配层有）；core 恢复可交互后 `settle_session_resume` 落终态 `:1133/:1153/:1164`；**投影失败结 `uncertain`**（`:1097`），未用 `failed` 撒谎 |
| **⑥`（DR1-F51）`docs/NODE_LINK_PROTOCOL.md` 两处** | ✅ 完成 | `:662` 收窄句加 `session.resume`（`session.list`/`session.create`/`session.resume` 三者投影 wire 结果）；§12.7 示例下方加「（非规范示例：新建会话在创建流程内的第二次提交完成后，可见版本为 `2`；上面这帧展示的是结果形状，不是可复现的终态取值。）」。**未触碰 §10（WP2 区域）** |

### 3.3 本包新增 / 修复的单元测试（与被写代码同目录，随实现交付）

全部在 `crates/server/src/node_link/command/tests.rs`（`session.resume` 段，起于 `:2639`）：

| 用例 | 覆盖 |
| --- | --- |
| `session_resume_returns_accepted_then_the_composite_result` | R34/R35：`accepted(result = null)` → `completed` + `SessionResumeResult` 三字段与 `sessionMeta` |
| `session_resume_with_any_payload_field_is_rejected_without_side_effects` | R34/R36：`command.rejected(nodelink.command.unsupported_field)` + `details.field`，不进 accepted/terminal 管线，`commit_calls` 不变 |
| `an_unauthorized_session_resume_is_rejected_before_any_local_read` | R30/R31：只持 `grant.observe` → `nodelink.export.not_granted`，`commit_calls` 不变；**换一个不存在的 sessionId 得到同形拒绝**（存在性不可区分） |
| `session_resume_without_persisted_recovery_data_fails_as_unsupported` | R37 + **DR1-F39**：两列 `NULL`（同「能力未宣告」路径）→ 终态 `failed` + `nodelink.command.unsupported`（**不是** `uncertain`），`result` 为 null |
| `session_resume_with_a_deleted_workspace_fails_as_unavailable` | R24（关联，WP3 为主）：创建时目录被删 → `failed` + `nodelink.internal.unavailable`（**不是** `unsupported`） |
| `a_repeated_session_resume_replays_the_first_result` | R27/R28：同 `requestId` 重试回首次终态，逐字一致，不再 `accepted`、不重复派发 |
| `the_same_request_id_with_a_different_command_conflicts` | R29：同键异义 → `nodelink.command.idempotency_conflict`，无提交 |
| `session_resume_with_a_stale_attachment_is_rejected` | R27：过期 attachment → `attach_generation_stale`，无副作用 |

外加 `forbidden_session_resume_fields_are_recognised_before_the_typed_decode`（纯函数层）——核对 `forbidden_payload_field` 对 6 种 payload 形态的判定在**严格解码之前**成立。

> 派发简报第 7 节明确「不写集成/E2E 用例、驱动脚本、测试数据——那是 TP2 的职责」，因此本轮**没有**新增 `crates/app/tests/` 或 `crates/server/tests/` 的端到端用例（`tasks.md` 2.6 原文有「补齐受控路径的端到端用例」一句）。见 §六 待澄清问题 2。

### 3.4 红窗口收口逐条

派发简报列的 8 条（`E0046`×7 + `E0004`×1）：

| # | 位置 | 收口方式 | 状态 |
| --- | --- | --- | --- |
| 1 | `local_admin/test_support.rs` `NotTouched::load_recovery` | `unreachable!("{NOT_TOUCHED}")`（与该替身其余方法的既有风格一致） | ✅ |
| 2 | `local_admin/test_support.rs` `FixedStore::load_recovery` | 同上 | ✅ |
| 3 | `node_link/command/tests.rs` `CommandStore::load_recovery` | 会话行不存在或任一列为 `NULL` → `Ok(None)`；**不推导、不补齐** | ✅ |
| 4 | `node_link/resource/tests.rs` `SliceStore::load_recovery` | `unreachable!`（该路由用例不读恢复数据） | ✅ |
| 5 | `app/tests/support/owner.rs` `FlakySessionStore::load_recovery` | 原样转发 inner（故障注入只作用在 `commit`） | ✅ |
| 6 | `app/tests/support/owner.rs` `ScriptedBackends::resume` + `ScriptedEndpoint::agent_session_id` | 恢复时回带持久化的 `agentSessionId` | ✅ |
| 7 | `local_admin/test_support.rs` `NotTouched::resume` | `unreachable!` | ✅ |
| 8 | `node_link/command.rs` `core_payload()` 穷尽匹配失配（`E0004`） | `WirePayload::SessionResume(_) => return Err("session.resume is dispatched by its own handler")`，与 `SessionCreate` **同形早退**；该文件仍**无** `_ =>` 通配臂 | ✅ |

`port_error_code` 的 `UnavailableKind::BackendUnsupported → nodelink.command.unsupported` 臂（`:2381`；DR1 Round 9）另计，**未新增任何错误码 / feature**（`nodelink.command.unsupported` 是 registry 既有码，`retryable = false`）。

---

## 四、必做取证：`app` 是否终于被编译？

**答：`app` 已被编译，且 workspace 级 clippy 零诊断。**

| 项 | 实测 |
| --- | --- |
| 接手前的状况 | `Checking app` 在上一实例日志中 **0 行**——`server` 一失败 `app` 就没有机会跑，因此 `app/tests/support/owner.rs` 的 3 个替身错误一直被遮蔽 |
| 本轮命令 | `cargo clippy --locked --workspace --all-targets --all-features --keep-going`（`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp6`） |
| 日志 | `reports/wp6-coder-f2-workspace-clippy-keepegoing.log` |
| `Checking app` | **出现**（第 13 行）：`Checking app v0.0.0 (…\crates\app)` |
| 诊断条数 | **0**（exit 0；`--keep-going` 下无任何 `error`/`warning` 输出） |
| 12 个 crate 全部 `Checking` | ✅（`acpr-transcript` / `core` / `acp-protocol` / `acpr-wire` / `sync-protocol` / `node-link-protocol` / `storage-sqlite` / `agent-host` / `identity-auth` / `server` / `identity-keystore` / `app`） |
| PV1 阶段 1 的 `-p app --all-targets` | `cargo test -p app --all-features` 实跑了 **7 个测试二进制**（`app` lib 57 项、`audit_export` 1、`cli_commands` 11、`daemon_lifecycle` 12、`node_link_e2e` 3、`node_link_listener` 8、`node_pair_export_ids` 2），全部 ok |

### 诊断是否**逐条**对应已登记义务？

**是，无遗漏、无新增。** `app` 侧的三条诊断恰好就是已登记的三条，对应关系如下（接手时的错误基线取自派发简报；本实例在跑 workspace clippy **之前**已先用 `cargo check -p server -p app --all-targets --all-features` 与 `cargo clippy -p server -p app --all-targets --all-features -- -D warnings` 分别确认了 `server` 与 `app` 两侧都干净，因此没有出现「先跑 workspace 时 `server` 仍红而遮蔽 `app`」的情况）：

| `app` 侧诊断（`E0046`，接手时） | 登记归属 | 本轮收口 |
| --- | --- | --- |
| `FlakySessionStore` 缺 `load_recovery` | DR1-F42「五个 `load_recovery` 替身」之一 | ✅ 原样转发 |
| `ScriptedBackends` 缺 `resume` | DR1 Round 9「WP3 新增三个必需 trait 方法」 | ✅ |
| `ScriptedEndpoint` 缺 `agent_session_id` | 同上 | ✅ |

### 有无第二种成因？

**没有。** 证据：workspace `--keep-going` 在 `server` 已干净的前提下 exit 0，且 `app` 的 `--all-targets`（含 `tests/support/owner.rs`）全部通过；除上表三条外，`app` 侧**没有**任何其它诊断（既不是「同文件另一处漏改」，也不是「某个 trait 的默认实现缺失导致连锁」，也不是平台 `cfg` 分支差异）。

---

## 五、Checks（每个 Check ID 一行）

| Check ID | 命令 | 目录 | 退出码 | 日志 |
| --- | --- | --- | --- | --- |
| **PV1 阶段 1 · fmt** | `cargo fmt --all -- --check` | worktree 根 | **0** | `reports/wp6-coder-f2-PV1-fmt.log` |
| **PV1 阶段 1 · clippy** | `cargo clippy --locked -p server -p app --all-targets --all-features -- -D warnings` | worktree 根 | **0** | `reports/wp6-coder-f2-PV1-clippy.log` |
| **PV1 阶段 1 · test** | `cargo test --locked -p server -p app --all-features` | worktree 根 | **0**（server lib **295 passed / 0 failed**；app 7 个二进制合计 **94 passed / 0 failed**） | `reports/wp6-coder-f2-PV1-test.log` |
| **PV2** | `npm run check` | worktree 根 | **0**（十道门禁串行全绿，含 `check:acp` / `check:command-catalog` / `check:drift` / `check:boundaries` / `check:docs` / `check:agentic`） | `reports/wp6-coder-f2-PV2.log` |
| **补充取证 · workspace clippy** | `cargo clippy --locked --workspace --all-targets --all-features --keep-going` | worktree 根 | **0** | `reports/wp6-coder-f2-workspace-clippy-keepegoing.log` |

- **环境**：Windows 11 / `rustc` 与 `rustfmt` 由 `rust-toolchain.toml` 固定（`1.98.1-x86_64-pc-windows-msvc`）/ Node `v24.19.0`（≥ commitlint 21 的下限 22.12）/ `CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp6`（本 worktree 独立）。
- **配置**：`cargo` 全部带 `--locked`；`clippy` 带 `--all-targets --all-features -D warnings`。
- **依赖差异**：无。所有上游交付（`248d9b9`/`32f71f5`/`4f7a2355`/`4a3882d`/`1376e1b`）已在 base `5ab7e9d` 中；本轮无证据复用。
- **未执行项**：
  - `npm run check:rust`（= workspace 全量 `fmt` + `clippy` + `test`）**未整体执行**。按 `plan.md` 的 PV1 阶段制，workspace 全量属**阶段 2**（集成/候选/主分支），WP6 分支阶段的通过条件是 `-p server -p app`。本轮**已单独**执行其中的 workspace `clippy --all-targets --all-features --keep-going`（即派发简报第 5 节的补充取证）与 workspace `fmt`（`cargo fmt --all` 天然覆盖全 workspace，exit 0）；workspace 全量 `test` 未跑，**留给候选阶段的 PV1 阶段 2**。
  - CI-only 判定（`deps` / `advisories` / `secrets`）**无本地等价物**，本轮未执行（AGENTS.md §8）；未执行不等于通过。
  - **CR6（独立检视）尚未派发** —— 本报告不冒充其结论。
- **失败记录保留**：`cargo test -p server -p app --all-features` 的**第一次**运行（接手时的现场）为 **exit 101，server lib 287 passed / 9 failed**。这 9 条里有 8 条是 coder-F 的新用例（`session.resume_*` / `the_same_request_id_with_a_different_command_conflicts` / `zz_debug_resume`），1 条是 `node_link::conn::tests::fixtures_are_consumed_by_the_handshake_and_error_layers`。该轮日志保留在 `/tmp/wp6-test0.log`（未归档进 reports，因为它是「接手时的现场」而非交付证据；§1.3 逐条记录了成因与修法）。**没有**用删测试、弱化断言或忽略失败的方式消除任何一条。

---

## 六、资源释放

| 资源 | 处置 |
| --- | --- |
| `CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp6` | 保留（worktree 私有构建目录，由 provisioner 分配并按 worktree 回收；本实例只构建、未删除） |
| 测试临时目录 | **已释放**。本实例新增的 `Workspace` 守卫在句柄析构时 `remove_dir_all` 自己建的 `acp-remote-resume-*` 目录（「目录已被用例自己删掉」的情形有意忽略错误）。接手前 coder-F 留下的同名残留目录已由该守卫在新一轮运行中清掉 |
| SQLite 库文件 | 本轮未手工创建任何库文件；`-p app` 的测试各自用独立临时库（`plan.md` Runtime Resources：WP6 与他人不共享） |
| loopback 监听器 | 本轮新增的用例全部走 `Fixture` 替身，**不开** TCP 监听器；`-p app` 既有 `node_link_listener` / `daemon_lifecycle` 用例用 `127.0.0.1:0` 内核分配端口并自行释放 |
| 进程 | 无遗留子进程（`cargo` 全部正常退出；`agent-host` 的子进程树清理由 WP5 既有测试保证，本轮未新增 spawn） |
| 分支 / worktree / main | 未切分支、未新建或清理 worktree、未 push、未动 `refs/heads/main` |
| `node_modules` | 只用既有联接（`node_modules -> /d/Project/acp-remote/node_modules`）；**未** `npm install`/`npm ci`/`npm update`，**未** `rm -rf` |

---

## 七、待澄清问题（需主 Agent 裁定，本实例未自行决定）

1. **义务 ②「组合根接线」的措辞与实际不符（唯一需要裁定的项）**。`tasks.md` 2.6 与派发简报都写「`crates/app/` 组合根接线：把恢复路径接到真实的 `SqliteStore` + `AgentHost`」。实测该接线**已由上游闭合**：`app::compose` 装配的 `SqliteStore`（WP4）与 `AgentHost`（WP5）已实现新端口，`core::broker` 经 trait 对象注入，WP6 在 `crates/app/src/**` **不需要任何改动**。本实例因此把该义务**记为经核实的 no-op**，只在 `crates/app/tests/` 收了编译涟漪。**若主 Agent 认为还存在未接的线**，请指出具体入口，我按图索骥补做；否则建议由主 Agent 在合入时把这条义务的措辞改为「核实接线为 no-op」，以免 U1 覆盖核对（tasks 6.5）按字面判为未覆盖。
2. **「补齐受控路径的端到端用例」的归属**。`tasks.md` 2.6 原文含这一句，但派发简报第 7 节与 `roles/coder.md` 均规定 coder **不得**代写 E2E/集成用例（`crates/**/tests/` 的新增用例文件由 TP2 独占，见 `plan.md` Shared File Ownership）。本实例按简报执行，**未写**端到端用例。恢复路径的可执行端到端覆盖（真实 Node Link 路由 + WP5 的 fake ACP Agent 与 `--dump-request-params`）落在 TP2。
3. **coders-F 现场的描述偏差**（§1.1）：派发简报说「死时正在修一个多余 `}`」与「8 条红窗口」，实测两者都不成立。若后续简报由模板生成，建议改为「先跑 `cargo check` 取准确基线」。

---

## 八、修复对应关系（对 §1.3 六处缺陷）

| 缺陷 | 位置 | 修法 | 验证 |
| --- | --- | --- | --- |
| 1 scope 漏 `session.resume` | `command/tests.rs` `session_submit_body` | 白名单加一项 | 8 条新用例从「全红（信封层 `schema_invalid`）」变为全绿 |
| 2 终态错误码取错路径 | 同上，2 处 | 改断言 `body["terminal"]["error"]["code"]` | 2 条用例绿 |
| 3 断言不存在的字段 | 同上，成功用例 | 改断言 `sessionMeta.state` | 1 条用例绿 |
| 4 fixture 未归类 | `node_link/conn/tests.rs` | 加入 `deferred_body_cases` 并注明本层落点 | 该 conn 用例绿 |
| 5 调试残留 | `command/tests.rs` 末尾 | 删除 `zz_debug_resume` | server lib 计数由 296→295，无条件 panic 消失 |
| 6 格式残片 | `command/tests.rs`、app owner.rs | 修正 + `cargo fmt --all` | `cargo fmt --all -- --check` exit 0 |

---

## 九、结果

- **本工作包 PV1 阶段 1 + PV2：PASS。** 8 条红窗口全清，`app` 终于被编译且零诊断。
- **独立 review（CR6）尚未返回 —— 待补。** 本报告是 coder 自测证据，**不得**当作 CR6 的结论、也不代表候选/主分支阶段 PV1 阶段 2 已通过。
- **合入**：本实例不 push、不动 `refs/heads/main`、不归档。

```yaml
handoff_index:
  - task_id: "2.6"
    work_package: WP6
    role: coder
    phase: fix
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "76ab01126114294e7064e2618eedad0615b2561d"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/session-resume/reports/wp6-coder.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "分支阶段形态（plan.md 的 PV1 阶段制）：cargo fmt --all -- --check exit 0；cargo clippy --locked -p server -p app --all-targets --all-features -- -D warnings exit 0；cargo test --locked -p server -p app --all-features exit 0（server lib 295 passed、app 7 个二进制 94 passed）。工具链 1.98.1-x86_64-pc-windows-msvc（rust-toolchain.toml）、Windows 11、独立 CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp6；上游 WP1–WP5 全部为 76ab011 的祖先，无版本差异。日志 reports/wp6-coder-f2-PV1-{fmt,clippy,test}.log。workspace 全量（PV1 阶段 2）按计划留到候选/主分支，本轮另行执行了 workspace clippy --keep-going（exit 0）作为 app 取证。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.6"
    work_package: WP6
    role: coder
    phase: fix
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "76ab01126114294e7064e2618eedad0615b2561d"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/session-resume/reports/wp6-coder.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "npm run check 在 worktree 根执行，Node v24.19.0（≥ commitlint 21 下限 22.12），只用既有 node_modules 联接（未安装/未升级依赖），十道门禁串行 exit 0。日志 reports/wp6-coder-f2-PV2.log。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.6"
    work_package: WP6
    role: coder
    phase: fix
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "76ab01126114294e7064e2618eedad0615b2561d"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/session-resume/reports/wp6-coder.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "交付提交 76ab011（base 5ab7e9d），10 个文件 +972/−71，含 coder-F 的全部未提交改动。依赖包含关系实测：git merge-base --is-ancestor 对 248d9b9 / 32f71f5 / 4f7a2355 / 4a3882d / 1376e1b 全部 ANCESTOR-YES。追加取证：cargo clippy --locked --workspace --all-targets --all-features --keep-going exit 0 且 'Checking app' 出现（0 诊断），日志 reports/wp6-coder-f2-workspace-clippy-keepegoing.log；app 侧诊断逐条对应已登记的三个替身义务（FlakySessionStore::load_recovery、ScriptedBackends::resume、ScriptedEndpoint::agent_session_id），无第二种成因。CR6 独立检视未派发，待补。"
    source_evidence: NOT_APPLICABLE
```
