# TP2 测试设计报告（修订版；继承 TP1，消解 CR7 及并入的评审意见）

<!-- 历史证据保留：TP1 的原始设计在 `reports/tp1-test-design.md`，本文件**不覆盖**它。 -->
<!-- 本文件是 TP2 实际编写用例所依据的**修正后设计**，并附实跑结果（见 §5）。 -->

- 变更：`session-resume`（U1，W5 最后一个工作包）
- 基线：`95051f9`（WP1–WP6 集成基线）
- 交付提交：`a02e2fd`（第 1 轮）；`f44301e`（第 2 轮，tester-A2 修复轮：闭合 R25 的两个覆盖缺口）；`3484541`（第 3 轮，tester-A3：**纯注释/记录层订正**，零行为变更）
  - **CR8-F2 订正记录**：本文件原先把第 2 轮交付写作 `4146611`，那是**同一提交的 amend 前旧 SHA**。
    实测：`4146611a2e7002b89534227293d2ad547f5e11de` 与 `f44301e` 的 **tree 完全相同**
    （均为 `97bbf785cbafb1c2df22878be9b92d263bcfd7ab`）、提交信息与时间相同，只是父提交被 amend，
    因此 `4146611` 不在本分支的提交序列（`f8133f2 → a02e2fd → f44301e`）里。**权威值是 `f44301e`**，
    下方所有 `4146611` 均已订正。此订正痕迹按 CR8 要求保留。
- 修订依据：`reports/cr7-review.md`（FAIL，2×MAJOR）、`reports/cr3-review.md`（CR3-F3）、
  `reports/cr4-review.md`（CR4-F3）、`reports/cr5-review.md` + `cr5-review-round2.md`
  （CR5-F2/F5）、`reports/cr6-review-round2.md`（`uncertain` 口径）
- 用户决定（2026-10-01）：TP1 不再单独派修复轮，内容并入 TP2

---

## 0. 相对 TP1 的实质修订（先看这里）

| # | 来源 | TP1 的写法 | 修正后的写法 | 落点 |
| --- | --- | --- | --- | --- |
| **CR7-F1（MAJOR）** | spec R10 已改为「MUST NOT **发送 `session/resume`** + 在返回前**终止并回收**本次为恢复拉起的子进程」 | 沿用「不启动进程」的可验收形式表述，口径来源写成 `plan.md ## Risks / Trade-offs` | **直接采用修正后的规格措辞**：断言 = ①线级无 `session/resume` ②**单一心跳文件停止增长 + `runtime_running() == false`** ③无会话绑定 ④错误分类为 `Unavailable(BackendUnsupported)`。**不再**出现「不 spawn」的字面断言（`initialize` 必先 spawn，物理不可满足） | `crates/agent-host/tests/resume.rs::an_undeclared_capability_sends_no_resume_request_and_reclaims_the_child` |
| **CR7-F2（MAJOR）** | spec `workspace-resolution` 已把「持久化取值为 `NULL`」从「服务端不可用类」改到「与能力不支持**同一条路径**（`nodelink.command.unsupported`）」 | 三处断言沿用旧归类，且把依据写成 R19（**编号错误**） | 采纳新归类并**同时**断言三类错误码互不相同：`export.not_granted`（越权）/ `internal.unavailable`（目录复校验失败）/ `command.unsupported`（不支持 + 两列 `NULL` 同路径）。依据编号更正为 **R21/R13** | `session_resume_e2e.rs::workspace_revalidation_and_null_recovery_data_take_distinct_paths`、`an_unauthorized_resume_is_rejected_before_any_local_read` |
| **CR7-F3（MINOR）** | 口径来源引用 `plan.md ## Risks / Trade-offs`（该节在 `design.md:162`）；NULL 依据写成 R19 | — | 引用改为 `design.md` 的风险注记与**正确**的 R21/R13；本报告不引用不存在的节 | 本文件 §1 各条 |
| **CR7-F4（MINOR）** | SR-R15-1 的 v5 段故障注入（注入同名不同类型列）**可能空转**（按列存在性守卫实现时注入不导致失败） | — | **不采用**该 best-effort 注入。R15 改由既有确定性用例 `migration.rs::a_failed_upgrade_rolls_back_to_v1`（v1 库 + 冲突对象）承担，TP2 **登记为复用**，不复制第二份 | `resume_columns.rs` 文件头「复用声明」 |
| **CR7-F5（MINOR）** | fake child 的场景/选项义务未写进计划（**已由主 Agent 在 CR7 处置中补进 WP5 行与 tasks 2.5**） | — | 不再是待澄清项：WP5 已交付 `session/resume` 分支、`resume-ok`/`resume-error`/`session-new-error`、`--dump-requests` 与**并列**的 `--dump-request-params`（提交 `1376e1b`，CR5 Round 2 PASS）。TP2 直接复用 | `crates/agent-host/tests/resume.rs` |
| **CR7-F6（SUGGESTION）** | SR-R32-1 只在**同一进程内**调用 `recover_unsettled` | — | **强化为两个维度**：①经 wire 的 `command.status` 重查得 `uncertain`；②**重开同一 `data_dir` 的真实存储**，用 `find_request` 从**持久行**读回同一条 `uncertain` 记录（新增 `OwnerNode::reopen_store`） | `session_resume_e2e.rs::a_crash_window_leaves_uncertain_persisted_across_a_reopened_store` |
| **CR7-F7（SUGGESTION）** | SR-R12-1 的备选口径「同一心跳文件 + 断言只有一个写入者存活」**不可观察** | — | 备选口径**删除**。固定为「每进程独立心跳文件」，且计数判据取 `--dump-request-params` 的**行数** | `resume.rs::resuming_twice_reuses_one_process_and_leaves_one_dispatchable_endpoint` |
| **CR7-F8（MINOR）** | loopback 监听器未登记进 `## Runtime Resources`（**已由主 Agent 补登**） | — | 不再是待澄清项 | — |
| **CR5-F2（并入，强制）** | SR-R12-2 前提写「新进程 P2 + 第二条心跳文件 h2」 | — | 前提改为「**进程复用、单一心跳文件**」；断言改为「**恰好一个可派发端点**（不预设是哪一个）+ **只有一个进程**」。**不改产品代码** | 同上 |
| **CR4-F3（并入，强制）** | 迁移测试缺「ALTER 追加 vs 12-step 重建」的判别式；CR4 给的 `contains("IF NOT EXISTS")` 建议**经实测不成立** | — | 见 §2.1：**按实测标定**改用「表名是否带双引号」作判别式，并自带反证 | `resume_columns.rs::v5_appends_the_recovery_columns_instead_of_rebuilding_owned_session` |
| **CR3-F3（并入，强制）** | R31 只在 core 的 `Actor::Device` 分支有证据，Node 分支「先于本机读取」无直接断言 | — | 在 **app 层真实 Node Link 管线**（= `Actor::Node`）补断言：`load_recovery` 计数为 0；对「存在但**不覆盖**授权的会话」与「不存在的会话」得到**完全相同**的响应与 `details` | `session_resume_e2e.rs::an_unauthorized_resume_is_rejected_before_any_local_read` |
| **CR5-F5（并入，强制）** | SR-R8-1/SR-R26-2 必须经**真实子进程 + `--dump-request-params` 读行**才算数 | — | 两个用例都经 `CARGO_BIN_EXE_acpr-fake-acp-agent` 真实 stdio 子进程，并**逐行读 `{"method","params"}`**，断言键集合恰为两键且 `params` 非 `null` | `resume.rs::the_wire_level_resume_params_carry_the_persisted_values` |
| **CR6 Round 2 口径** | 「能力未宣告路径终态 `failed` 或 `uncertain`」（过宽） | — | 收紧为「终态必须是 **`failed`** + `nodelink.command.unsupported`；`uncertain` **只**属崩溃窗口 R32」，并单独断言「Agent 拒绝恢复」是**另一个**错误码（两类不共漏斗） | `session_resume_e2e.rs::an_unsupported_agent_fails_the_resume_terminal_without_creating_a_session` |

### 2.1 CR4-F3 的判别式：为什么不是 `IF NOT EXISTS`

CR4 建议 `assert!(after.contains("IF NOT EXISTS"))`。**实测该建议不成立**（`resume_columns.rs` 的
注释里记录了标定过程，探针在编写后即删除）：

| 路径 | `sqlite_master` 存储文本（实测） | 含 `IF NOT EXISTS`？ | 表名带引号？ |
| --- | --- | --- | --- |
| **ALTER 追加**（v4→v5 的 `owned_session`） | `CREATE TABLE owned_session (\n … closed_at TEXT, agent_session_id TEXT, workspace_cwd TEXT) STRICT` | **否**（SQLite 在 `ALTER TABLE ADD COLUMN` 时把它从存储文本里去掉） | **否** |
| **12-step 重建**（v1→v5 的 `owned_audit`） | `CREATE TABLE "owned_audit" (\n … ) STRICT` | 否 | **是**（`ALTER TABLE …_vN RENAME TO …` 的签名） |
| 新建库 | `CREATE TABLE IF NOT EXISTS owned_session (… )` | 是 | 否 |

因此**两条路径都不含 `IF NOT EXISTS`**，按它断言会恒真。修正后的判别式是**表名是否带双引号**
（重建段的 `RENAME` 签名），并额外断言「既有列定义文本逐字节保留 + 追加列紧跟其后 + `cid` 连续 +
无 `_vN` 残留表」，最后用 v1 库的 `owned_audit` 作**反证**（它必须带引号），确保判别式不恒真。

---

## 1. 输入与真实入口

| 层 | 入口（真实，非 mock 绕过） | 用例文件 |
| --- | --- | --- |
| ACP wire | `RawDocument::parse_bytes` → `Envelope::classify` → `session_resume_request` / `decode_response` | `crates/acp-protocol/tests/session_resume.rs` |
| 存储 | 真实 SQLite 文件 + `storage_sqlite::migrate` 升级路径；断言读 `sqlite_master` / `quote()` / `typeof()` / `PRAGMA table_info` / 端口 `load_recovery` | `crates/storage-sqlite/tests/resume_columns.rs` |
| 本地后端 | `AgentHost` + **真实 stdio 子进程** `acpr-fake-acp-agent`；证据来自 `--dump-request-params` 文件行与 `--heartbeat-file` 增长 | `crates/agent-host/tests/resume.rs` |
| Node Link wire | `Envelope::decode` + 类型化 `CommandSubmit`；只读复用 WP2 的 fixture 与 `commands.json` | `crates/node-link-protocol/tests/session_resume_command.rs` |
| Owner 侧受控路径 | **真实 loopback listener**（`app::daemon::{net_config, register_node_link_paths}`）+ 真实 SQLite + 真实 `identity_auth::Authority` + 真实 WSS 握手；仅 Agent 后端与 `SessionStore` 故障注入是受控替身 | `crates/app/tests/session_resume_e2e.rs` |

断言可观察性总则：协议错误码/结果类型 > 持久化字节 > 派发计数 > 进程存在性（心跳）> 请求痕迹（dump 行）。
不使用日志文本或私有字段作为判据。

---

## 2. 需求映射（R1–R37 → 用例 ID）与实跑结果

TP1 的 58 个 `SR-Rn-m` ID 中，**已由上游实现者在自有写范围内内联覆盖**的部分**登记为复用**（不复制
第二份实现，否则会掩盖原用例失效）。TP2 新增 **33** 条本机可执行用例，另有 **2** 条 `#[cfg(unix)]`
变体零本机证据、待 Linux CI 首次编译+执行（**不得计入 PASS**，见 §4 与 §6）。

| R | 需求/场景 | TP2 用例（`#[test]` 名） | 层 | 结果 |
| --- | --- | --- | --- | --- |
| R1 | `session/resume` 类型化解码与往返保真 | `session_resume_wire_round_trip_keeps_unknown_fields_byte_exact`、`session_resume_typed_encoding_and_decoding_are_inverse`、`session_resume_is_registered_as_a_conditional_client_to_agent_request` | acp | PASS |
| R2 | 正常解码并回写未知字段 | 同上（请求侧）+ `session_resume_response_round_trip_keeps_unknown_fields`（响应侧） | acp | PASS |
| R3 | 缺少 required 字段被拒绝 | `missing_required_fields_are_rejected_without_defaults`、`a_resume_request_missing_cwd_is_rejected_at_the_decode_boundary` | acp | PASS |
| R4 | `session/load` 仍为显式不支持 | `session_load_stays_unsupported_and_resume_is_implemented` | acp | PASS |
| R5 | 会话创建时暴露 ACP 会话标识 | `creating_two_sessions_exposes_two_distinct_agent_session_ids`（agent-host 侧）；app 侧经 `load_recovery` 读到持久化标识 | agent-host + app | PASS |
| R6 | 创建成功暴露标识 | 同上（两次创建标识互不相同） | agent-host | PASS |
| R7 | 未取得标识时不编造取值 | **复用** `agent-host/tests/session.rs::failed_session_new_yields_no_endpoint_and_no_identifier`（WP5，方法级）；TP2 不复制 | agent-host | 复用（已 PASS） |
| R8 | 进程不在时的会话恢复 | `the_wire_level_resume_params_carry_the_persisted_values`（**线级**，CR5-F5）、`a_resumed_endpoint_dispatches_exactly_one_prompt_and_completes_the_turn` | agent-host | PASS |
| R9 | 能力已宣告时恢复到可交互 | `a_resumed_endpoint_dispatches_exactly_one_prompt_and_completes_the_turn` | agent-host | PASS |
| R10 | 能力未宣告时显式不支持且不发送恢复请求 | `an_undeclared_capability_sends_no_resume_request_and_reclaims_the_child`、`a_null_resume_capability_is_the_same_as_an_omitted_one` | agent-host | PASS |
| R11 | Agent 拒绝恢复时明确失败 | `an_agent_refusal_sends_one_resume_request_and_no_new_session` | agent-host | PASS |
| R12 | 反复恢复不产生第二个端点 | `resuming_twice_reuses_one_process_and_leaves_one_dispatchable_endpoint`（**修正后的前提**，CR5-F2） | agent-host | PASS |
| R13 | 版本常量与 migration 幂等 | `a_too_new_database_is_refused_without_touching_a_single_byte`、`two_consecutive_opens_leave_the_schema_byte_identical`；R15 复用 `migration.rs::a_failed_upgrade_rolls_back_to_v1` | storage | PASS |
| R14 | 连续两次打开 schema 文本不变 | `two_consecutive_opens_leave_the_schema_byte_identical`（+ 复用既有两条） | storage | PASS |
| R15 | 升级中途失败整体回滚 | **复用** `migration.rs::a_failed_upgrade_rolls_back_to_v1`（CR7-F4：拒绝空转的第二份注入） | storage | 复用（已 PASS） |
| R16 | v4→v5 保留既有行且新列为空 | `upgraded_sessions_keep_their_bytes_and_report_no_recovery_data` | storage | PASS |
| R17 | 新列写入后可读回且不推导 | `recovery_columns_round_trip_byte_exactly_and_are_never_derived` | storage | PASS |
| R18 | 升级库与新建库的 owned 列清单相等 | `v5_appends_the_recovery_columns_instead_of_rebuilding_owned_session` 的列清单段 + **复用** `migration.rs::v4_database_upgrades_to_v5_…` 的全表 `column_specs` 比对 | storage | PASS |
| R19 | v3→v4 给既有节点行写空清单 | **复用** `migration.rs::v3_database_upgrades_to_v5_by_appending_the_export_id_and_recovery_columns_only` | storage | 复用（已 PASS） |
| R20 | v2→v3 保留审计并扩展词表 | **复用** `migration.rs::v2_fixture_upgrades_to_v3_and_widens_only_the_audit_checks` | storage | 复用（已 PASS） |
| R21 | 恢复所需列的读写边界 | `a_half_null_recovery_pair_is_still_no_recovery_data`（`NULL` 读回语义）+ `recovery_columns_round_trip_byte_exactly_and_are_never_derived`；R22 见 app 层 | storage | PASS |
| R22 | 恢复流程不覆写持久化取值 | `a_successful_resume_returns_the_session_reference_and_leaves_the_columns_untouched`（成功 + 失败两条路径）、`workspace_revalidation_and_null_recovery_data_take_distinct_paths`、`any_resume_payload_field_is_rejected_before_side_effects`、`a_crash_window_…`（崩溃窗口） | app | PASS |
| R23 | 恢复时的目录复校验 | `a_successful_resume_…`（别名改指向仍用持久化值）、`workspace_revalidation_…`（删除 / 别名指向别处 / 同名重建） | app | PASS |
| R24 | 目录已被删除时返回不可用且不启动 Agent | `workspace_revalidation_and_null_recovery_data_take_distinct_paths` 第 ①② 段（后端调用计数为 0） | app | PASS |
| R25 | 规范化结果变化时拒绝恢复 | **三档覆盖（CR8-F5 订正，原先把三档并写成「已覆盖」）**：① **平台无关变体** `a_persisted_cwd_whose_canonical_form_differs_is_refused_before_any_backend_call`（持久化路径仍存在、仍是目录，但 `canonicalize` 结果与之逐字不同）——**本机（Windows）真实执行并 PASS，可计入 PASS**；② **2 条 `#[cfg(unix)]` 变体** `a_persisted_directory_replaced_by_a_symlink_is_refused_before_any_backend_call`（符号链接改指）与 `an_inaccessible_persisted_directory_is_refused_before_any_backend_call`（权限丢失）——**设计上覆盖、判定待 Linux CI 的 `checks` job（`ubuntu-latest`）首次编译+执行，不得记 PASS**（本机 Windows 被 `cfg` 剔除，交叉编译缺 `x86_64-linux-gnu-gcc`，零编译证据零执行证据）；③ **上游同族变体**（目录被删 / 别名改指 / 同名重建）已由 `workspace_revalidation_and_null_recovery_data_take_distinct_paths` 覆盖并本机 PASS | app | ①③ PASS；② PENDING |
| R26 | 目录仍然有效时使用持久化取值 | `the_wire_level_resume_params_carry_the_persisted_values`（**线级**）、`a_successful_resume_…`（对照目录不同） | agent-host + app | PASS |
| R27 | 命令授权、幂等与终态 | `the_command_catalog_row_matches_the_wire_contract`、`session_scoped_and_version_rules_are_enforced_at_the_decode_boundary`、`a_completed_resume_terminal_carries_the_named_result_payload`、app 层的终态形状断言；R30/R33 **复用** WP6 的 `server/src/node_link/command/tests.rs` 内联用例 | nlp + app | PASS |
| R28 | 相同 requestId 重试不重复派发 | `a_repeated_resume_request_id_replays_the_first_result_once`（后端计数 == 1） | app | PASS |
| R29 | 同键不同语义被拒绝 | 同上（第二段） | app | PASS |
| R30 | 越权命令被拒绝 | **复用** `server/src/node_link/command/tests.rs::an_unauthorized_command_is_rejected_audited_and_has_no_side_effect`（`:1509`，通用越权命令）与 `::an_unauthorized_session_resume_is_rejected_before_any_local_read`（`:2888`，`session.resume` 专属）——两条均已独立核实存在 | server | 复用（已 PASS） |
| R31 | 越权恢复被拒绝且先于本机读取 | `an_unauthorized_resume_is_rejected_before_any_local_read`（**CR3-F3 强制项**：`load_recovery` 计数 0 + 存在/不存在同响应） | app | PASS |
| R32 | 崩溃窗口进入 uncertain | `a_crash_window_leaves_uncertain_persisted_across_a_reopened_store`（**CR7-F6 强化**：重开存储读回持久行） | app | PASS |
| R33 | 命令限流 | **复用** `server/src/node_link/command/tests.rs::the_command_rate_limit_replies_rate_limited_and_then_closes`（`:2303`，`[R71]` 120/分钟）。**CR8-F3 订正**：原指针 `:992` 不成立——该行落在 `session_submit_body` 的 session-scope `matches!` 命令清单里，与限流无关。真正的证据是构造性的：`crates/server/src/node_link/command.rs:248` 的 `if !self.admit_rate(handle)` 在 `match submit.command` **之前**、对**全部**命令（含 `session.resume`）生效，因此 `session.resume` 被同一连接级限流覆盖 | server | 复用（已 PASS） |
| R34 | `session.resume` 的 payload 与结果契约 | `any_resume_payload_field_is_rejected_at_the_decode_boundary`、`a_completed_resume_terminal_carries_the_named_result_payload`、`session_scoped_and_version_rules_…`、`any_resume_payload_field_is_rejected_before_side_effects`（端到端 4 个字段） | nlp + app | PASS |
| R35 | 正常恢复并回传会话引用 | `a_successful_resume_returns_the_session_reference_and_leaves_the_columns_untouched`（终态 `SessionResumeResult` + 后续 `session.prompt` 派发成功） | app | PASS |
| R36 | 携带字段被拒绝且不启动进程 | `any_resume_payload_field_is_rejected_before_side_effects`（端到端）+ `any_resume_payload_field_is_rejected_at_the_decode_boundary`（wire） | app + nlp | PASS |
| R37 | Agent 不支持恢复时终态失败 | `an_unsupported_agent_fails_the_resume_terminal_without_creating_a_session`（**CR6 Round 2 口径**：`failed` + `command.unsupported`，且与「Agent 拒绝」码不同） | app | PASS |

**统计**：37/37 行有 ≥1 个可读证据（新增用例或明确登记的上游复用）。新增 **33** 条**本机可执行**用例：
`acp-protocol` 7、`storage-sqlite` 6、`agent-host` 7、`node-link-protocol` 5、`app` 8；
**另有 2 条 `#[cfg(unix)]` 变体**（均在 `app/session_resume_e2e`：符号链接改指、权限丢失）**不在此计数内**——
它们在本机（Windows）既无编译证据也无执行证据，状态为 **PENDING**，关闭条件是 Linux CI 的 `checks` job
在 `ubuntu-latest` 首次编译并执行（若失败，开新问题编号回 TP2，不改判本轮结论）。

---

## 3. 关键路径覆盖

| # | 关键路径 | 用例 | 判别力说明 |
| --- | --- | --- | --- |
| ① | 能力未宣告 ⇒ 不发送 + 回收子进程 | `an_undeclared_capability_sends_no_resume_request_and_reclaims_the_child`、`a_null_resume_capability_is_the_same_as_an_omitted_one` | 线级 dump 无 `session/resume`/`session/new`；心跳**停止增长** + `runtime_running()==false`；`null` 与省略同判 |
| ② | 越权恢复先于本机读取 | `an_unauthorized_resume_is_rejected_before_any_local_read` | 目标会话目录**已删除**：先读后校验的实现必然回 `internal.unavailable`；断言得 `not_granted` + `load_recovery` 计数 0 |
| ③ | 崩溃窗口进入 uncertain | `a_crash_window_leaves_uncertain_persisted_across_a_reopened_store` | 终态提交按 marker 被拦（`rejected_commits ≥ 1`）→ 经 wire `command.status` 得 `uncertain` → **重开存储**从持久行读回 `uncertain`；同时断言那一帧的 `terminalEventId` 为 `null`（§12.7：不构成可重放的首次结果） |
| ④ | `NULL` 列不得被当作可恢复 | `upgraded_sessions_keep_their_bytes_and_report_no_recovery_data`、`a_half_null_recovery_pair_is_still_no_recovery_data`、`an_unsupported_agent_fails_…` | `typeof(col)='null'` 排除空串；缺任一列即 `Ok(None)`（不返回半条记录）；node-link 侧归入 `command.unsupported` |
| ⑤ | v4→v5 追加而非重建 | `v5_appends_the_recovery_columns_instead_of_rebuilding_owned_session` | 表名引号判别式 + 既有列文本逐字节 + `cid` 连续 + 无 `_vN` 残留 + **反证**（`owned_audit` 必带引号） |
| ⑥ | 重复恢复不产生第二个端点 | `resuming_twice_…`（agent-host）、`a_repeated_resume_request_id_replays_the_first_result_once`（app） | 恰好一个端点可派发 + `session/prompt` 线级仅 1 行 + `initialize` 仅 1 行（无第二进程）+ 后端 `resume` 计数 == 1 |

---

## 4. 覆盖缺口与如实声明

> **第 2 轮（tester-A2，提交 `f44301e`；原写作 `4146611`，是 amend 前的旧 SHA，见头部订正记录）已闭合 A1 与 A2。**
> 下方 A1/A2 的**原始描述逐字保留**作为审计痕迹；每条后面跟一行「已闭合」补记，说明实际怎么做的、证据在哪、哪些仍未在本机执行。

- **A1（R25 的「规范化结果变化」变体）**：`crates/app/Cargo.toml` 的 `[dev-dependencies]` 不含 `sqlx`，
  而 `Cargo.toml` 不在 TP2 的允许写入范围，因此 app 层无法用原始 SQL 造「仍存在但未规范化」的持久化
  取值。既有 `server/src/node_link/command/tests.rs::session_resume_with_a_deleted_workspace_fails_as_unavailable`
  （WP6）在路由级覆盖同族的「目录不存在」变体；「`canonicalize` 结果不同」的**平台无关**变体
  （非规范化绝对路径）**未写用例**。→ 需主 Agent 决定：或在 `app` 的 dev-dependencies 加 `sqlx`
  （属产品清单改动，超出 TP2 范围），或由 WP6 在 `server` 内补，或记为已知缺口。
  - **已闭合（`f44301e`）**：主 Agent 授权后在 `crates/app/Cargo.toml` 的 `[dev-dependencies]` 加了
    **恰好一行** `sqlx = { workspace = true }`，并追加授权 `Cargo.lock` 的**恰好一行** `"sqlx"`
    （实测：不加这一行时 `cargo metadata --locked` 与 `cargo check --locked` 立即失败，因为
    `Cargo.lock` 的 `app` 条目合并登记普通/dev/build 依赖）。支撑辅助函数是
    `support/owner.rs::overwrite_persisted_workspace_cwd`。
    「未规范化但存在」的形态选的是**尾部多余分隔符**而不是 `..`/`.`——TP1 原设计的 `/./` 方案经实测
    **在 Windows 上不成立**（canonicalize 返回带 `\\?\` 前缀的 verbatim 路径，verbatim 不做归一化，
    `..`/`.`/`//`/`/.` 连 `metadata` 都失败），尾部分隔符在两个平台上都同时满足「`metadata` 成功且是
    目录」与「`canonicalize` 结果逐字不同」，是唯一真正平台无关的形态。
- **A2（`#[cfg(unix)]` 的符号链接变体）**：TP1 的 SR-R25-2（symlink 改指）与 SR-R24-2（`chmod 000`）
  **未编写**。理由：当前开发机为 Windows，`#[cfg(unix)]` 分支在本机**不会被执行**，写出来只能得到
  「编译通过但从未运行」的证据；按角色契约「人工尚未实际执行不能报告通过」，宁可不写、如实标注，
  也不制造一条永远不执行的伪覆盖。二者由 §4-A1 的同一条路径 + WP6 的既有变体间接覆盖。
  - **已闭合（`f44301e`）**：主 Agent 推翻上述顾虑（`AGENTS.md` §9：Linux runner 会额外执行
    `#[cfg(unix)]` 的权限路径；「本机未执行」的正确处理是**如实标注状态**而不是不写）。两条用例已写入
    `crates/app/tests/session_resume_e2e.rs`，标 `#[cfg(unix)]`：
    ① `a_persisted_directory_replaced_by_a_symlink_is_refused_before_any_backend_call`（符号链接改指）；
    ② `an_inaccessible_persisted_directory_is_refused_before_any_backend_call`（权限丢失，附带
    `ModeRestore` 守卫在 `Drop` 里恢复模式位）。
    **状态：本机（Windows）从未执行**（`--list` 输出里只有 8 条，不含这两条）；**由 Linux CI 的
    `checks` job（`ubuntu-latest`，非 root）首次编译并执行**。本机无法交叉编译验证它们的编译
    （`cargo check --target x86_64-unknown-linux-gnu` 缺 `x86_64-linux-gnu-gcc`，`ring`/`cc-rs` 失败），
    因此这两条的**编译与运行都还没有任何本机证据**。
- **A3（上游复用清单）**：R7、R15、R19、R20、R30、R33 六行**没有** TP2 新增用例，证据来自上游
  在自己写范围内已交付且已 PASS 的用例（逐条列名见 §2）。这是有意的去重，不是遗漏。
- **A4（`specs/acp-wire-protocol` 标题的错字「圆形保真」）**：`plan.md` 的 Coverage Index 与 spec 原文
  一致，TP2 按原文引用，故映射无误；错字属 spec 文本问题，不在 TP2 权限内。

---

## 5. 实跑结果（`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-tp1`，HEAD `a02e2fd`）

| 命令 | 退出码 | 日志 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 0 | `reports/tp2-PV1-fmt.log` |
| `cargo clippy --locked -p acp-protocol -p storage-sqlite -p agent-host -p server -p app -p node-link-protocol --all-targets --all-features -- -D warnings` | 0 | `reports/tp2-PV1-clippy.log` |
| `cargo test --locked -p acp-protocol --all-features` | 0 | `reports/tp2-PV1-test-acp-protocol.log` |
| `cargo test --locked -p storage-sqlite --all-features` | 0 | `reports/tp2-PV1-test-storage-sqlite.log` |
| `cargo test --locked -p agent-host --all-features` | 0 | `reports/tp2-PV1-test-agent-host.log` |
| `cargo test --locked -p node-link-protocol --all-features` | 0 | `reports/tp2-PV1-test-node-link-protocol.log` |
| `cargo test --locked -p server --all-features` | 0 | `reports/tp2-PV1-test-server.log` |
| `cargo test --locked -p app --all-features` | 0 | `reports/tp2-PV1-test-app.log` |

新增用例逐条计数：`acp-protocol/session_resume` 7、`storage-sqlite/resume_columns` 6、
`agent-host/resume` 7、`node-link-protocol/session_resume_command` 5、`app/session_resume_e2e` 7。
**0 failed / 0 ignored**（本包新增用例无一被 `#[ignore]` 或跳过）。

---

## 6. 第 2 轮（tester-A2，HEAD `f44301e`）的实跑结果

同一 `CARGO_TARGET_DIR`；本机 = Windows 开发机。

| 命令 | 退出码 | 日志 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 0 | `reports/tp2-tester-a2-fmt.log` |
| `cargo clippy --locked -p app --all-targets --all-features -- -D warnings` | 0 | `reports/tp2-tester-a2-clippy.log` |
| `cargo test --locked -p app --all-features` | 0 | `reports/tp2-tester-a2-test-app.log` |
| `cargo test --locked -p app --all-features --test session_resume_e2e` | 0 | `reports/tp2-tester-a2-test-app-session_resume_e2e.log` |
| `cargo test --locked -p app --all-features --test session_resume_e2e -- --list` | 0 | `reports/tp2-tester-a2-test-list-app.log` |
| `cargo test --locked -p storage-sqlite --all-features` | 0 | `reports/tp2-tester-a2-test-storage-sqlite.log` |
| `cargo test --locked -p agent-host --all-features` | 0 | `reports/tp2-tester-a2-test-agent-host.log` |
| `cargo metadata --format-version 1 --locked` | 0 | `reports/tp2-tester-a2-metadata-locked.log` |

`app/session_resume_e2e`：**8 passed; 0 failed; 0 ignored**（第 1 轮 7 条 + 本轮 1 条平台无关变体）。
`--list` 输出恰好 8 条，**不含**两条 `#[cfg(unix)]` 用例——这是「本机不执行它们」的直接证据，
不是「它们通过了」。

**未在本机执行 / 未在本机验证的项**（不冒充通过）：

- `#[cfg(unix)]` 两族（符号链接改指、权限丢失）的**编译与运行**：本机无 Unix 平台；
  交叉 `cargo check --target x86_64-unknown-linux-gnu` 因缺 `x86_64-linux-gnu-gcc`（`cc-rs`/`ring`）失败，
  因此连**类型检查**也没有本机证据。由 Linux CI 的 `checks` job 首次编译并执行。
- `PV1 阶段 2`（`cargo test --locked --workspace --all-features`）、`PV2`（`npm run check`）、
  `cargo-deny` / `gitleaks`（仅 CI）：仍不在本包判定内（同第 1 轮）。

---

## 7. 第 3 轮（tester-A3，HEAD `3484541`）的记录层订正

本轮是 **CR8 独立检视后的纯文字/注释修正轮**：**零行为变更、零新增/删除用例、零断言改动**。
代码侧只改 `crates/**/tests/**` 里的**注释与 rustdoc**（3 个文件）：

| 文件 | 改动 | 对应 CR8 |
| --- | --- | --- |
| `crates/storage-sqlite/tests/resume_columns.rs` | `v5_appends_…` 的 rustdoc ①/④ 改为与代码一致的**表名引号判别式**；反证改为 `owned_audit` **必须带引号**；删掉 `IF NOT EXISTS` 的正向表述 | CR8-F1 |
| `crates/app/tests/session_resume_e2e.rs` | 文件级 doc 与第 ③ 段注释改为「能力不支持（本用例） vs 两列 `NULL`（由 `server/.../tests.rs::session_resume_without_persisted_recovery_data_fails_as_unsupported` 与 `resume_columns.rs` 承担）」；`details` 断言上方补实测值注释 | CR8-F5、S1 |
| `crates/app/tests/support/owner.rs` | 模块注释由「两处」改为「**三处**」，补 `overwrite_persisted_workspace_cwd`（经 `sqlx` 原始 SQL **绕过全部端口与本地管理方法**） | CR8-S2 |

报告侧的订正（F2/F3/F4/F5）见头部、§2 与 §6。完整处置与依据见 `reports/tp2-tester.md §10`。

### 实跑结果（同一 `CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-tp1`，本机 = Windows 开发机）

| 命令 | 退出码 | 日志 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 0 | `reports/tp2-tester-a3-fmt.log` |
| `cargo test --locked -p storage-sqlite -p app --all-features` | 0 | `reports/tp2-tester-a3-test.log` |
| `cargo test … --test session_resume_e2e an_unauthorized_resume_… -- --nocapture`（CR8-S1 **临时探针**，跑完即回滚） | 0 | `reports/tp2-tester-a3-s1-probe.log` |

**与第 2 轮逐项一致**：`app/session_resume_e2e` **8 passed**、`storage-sqlite/resume_columns` **6 passed**，
其余目标（`audit_export` 1、`cli_commands` 11、`daemon_lifecycle` 12、`node_link_e2e` 3、
`node_link_listener` 8、`node_pair_export_ids` 2、`admin_audit` 15、`admin_store` 43、`attachments` 5、
`commit` 18+1 ignored、`compaction_recovery` 3、`contract_v03` 5、`enum_coverage` 2、`imported` 10、
`migration` 12+1 ignored、`permissions` 4、`retention` 8、`session_version_rule` 3、lib 57）计数全部不变，
**0 failed**。注释改动不改变任何可观察行为。

### **CR8-F6 保持 PENDING（重申）**

2 条 `#[cfg(unix)]` 变体在本轮**仍为 PENDING**：本机 Windows 被 `cfg` 剔除，交叉
`cargo check --target x86_64-unknown-linux-gnu` 因缺 `x86_64-linux-gnu-gcc` 失败，因此它们
**零编译证据、零执行证据**。本轮**未**改动它们的断言、cfg 结构或任何行为，**未**用
`#[ignore]`/`#[cfg(not(unix))]` 包裹或删除，**未**将任何措辞改写成「已通过」。**关闭条件不变**：
Linux CI 的 `checks` job（`ubuntu-latest`，非 root）首次编译并执行；若失败则开新问题编号回 TP2，
不改判本轮结论。

### 清单零改动（本轮确认）

`git diff --name-only f44301e..3484541` 只有上面 3 个 `crates/**/tests/**` 文件；
`crates/app/Cargo.toml` 与 `Cargo.lock` **零改动**，`crates/**/src/**` **零改动**，
`schemas/`、`fixtures/`、`compatibility/` **零改动**。