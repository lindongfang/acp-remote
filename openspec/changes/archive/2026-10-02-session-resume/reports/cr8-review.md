# CR8 独立用例检视报告（test-case / TP2 / Round 1）

> 持久化说明：本报告由 reviewer 实例 `reviewer-T2`（只读工具集，无写权限）以全文返回、由主 Agent 原样落盘。run `7f5b87c4-5fdb-49ca-8cc3-c7a7b2650e11`。

## Shared Report

- **task_id**: `2.8`
- **work_package**: TP2
- **role**: reviewer
- **phase**: `test-case`
- **agent_context**: 实例 `reviewer-T2`（CR8 Round 1）。**全新线程，未参与 TP2 的任何实现或实现讨论**；调度者以「无继承实现对话」的方式派发。我只读到：角色契约、`plan.md`、5 份 specs、`AGENTS.md`、TP2 的两个报告与全部日志、以及目标提交 `f44301e` 的**内容**。我**不是**用例作者（作者为 `tester-A` 第 1 轮、`tester-A2` 第 2 轮），满足「用例 reviewer 不得是用例作者」。
- **target_revision**: `f44301e`（base `95051f9`）
- **scope**: TP2 的测试用例与需求映射（R1–R37）、断言判别力、两处清单改动、`#[cfg(unix)]` 两族的处置、实现者自报的两处自查。不判需求是否已实现完毕（交 merger / 主 Agent），不判集成候选门禁（交 merger）。
- **changes**: 无（只读）
- **checks**: 我**未执行任何 cargo / npm 命令**。全部结论基于源码、报告与既有日志的交叉比对。
- **issues**: 0 CRITICAL / 0 MAJOR / 6 MINOR / 2 SUGGESTION
- **result**: **PASS**

## Review Context

| 项 | 实际取值 |
| --- | --- |
| Review ID / Round | **CR8 / 1** |
| Review Type / Stage | `test-case` / 测试用例编写完成、待独立检视 |
| Work Package | TP2 |
| Repository | `D:/Project/acp-remote-wt/session-resume-tp1`（reviewer-launch HEAD = `f44301ecfe65`，工作区无未提交改动） |
| Base / Target | `95051f9` → `f44301e` |
| 读取的规则与需求 | `AGENTS.md`（§3/§9/§10/§12）、`roles/reviewer.md`、`roles/_shared/role-report.md`、`plan.md`（Coverage Index 37 行、Work Packages、SFO、PV1/PV2）、`specs/{acp-wire-protocol,local-agent-host,storage-schema-v2-migration,workspace-resolution,node-link-owner-server}/spec.md` |
| 使用的证据 | `reports/tp2-test-design.md`、`reports/tp2-tester.md`（含第 2 轮 §8–§9）、`reports/tp2-PV1-*.log`（8 条）、`reports/tp2-tester-a2-*.log`（8 条） |

### 实际检查范围

**逐字读完**：5 个新增测试文件（`crates/acp-protocol/tests/session_resume.rs`、`crates/storage-sqlite/tests/resume_columns.rs`、`crates/agent-host/tests/resume.rs`、`crates/node-link-protocol/tests/session_resume_command.rs`、`crates/app/tests/session_resume_e2e.rs`）；`crates/app/tests/support/owner.rs` 的构造段、`FlakySessionStore`、`ResumeProbe`、`ScriptedBackends`、`reopen_store`、`overwrite_persisted_workspace_cwd`；`crates/core/src/broker.rs:1474–1504` 与 `:3608–3643`；`crates/server/src/node_link/command.rs:238–284`、`:1622–1655`；`crates/app/Cargo.toml`、根 `Cargo.toml`、`Cargo.lock` 的 `app` 与 `sqlx` 条目；`scripts/check-crate-boundaries.mjs:76–88`、`:236–246`；6 个被复用的上游用例体。

**逐行核对**：Coverage Index 全部 37 行 → `tp2-test-design.md` §2 覆盖表。

### 限制（如实声明）

1. **我的 diff 工具不覆盖已提交范围。** `watchdog_diff` 只报告「相对 reviewer-launch `HEAD` 的工作区 delta」，对本轮返回 `No working-tree changes against reviewer-launch HEAD f44301ecfe65. Committed changes are not included.`。因此**我无法产出 `git diff 95051f9..f44301e` / `a02e2fd..f44301e`**，也**未运行** `git diff --name-only`。
   - 后果：任务清单 F 的「`crates/**/src/**` 零改动」「`compatibility/`、`schemas/`、`fixtures/` 零改动」与 C-1 的「diff 确实只有这两行」，**我只能在目标版本的「内容」层面核对，不能在「diff 集合」层面证明**。下文凡属此类一律标注 `[内容级已核 / diff 级受限]`。
   - 需要 diff 级判定的两项，请由 merger 或主 Agent 在合并前用 `git diff --name-only 95051f9..f44301e` 机械复核。
2. 我不重复 Project Verify；「全绿」我只作为**辅助**证据读取，不作为正确性推定。
3. `4146611` 这个 SHA 我**无法验证是否存在**（无 git）——见 CR8-F2。

---

## A. 需求映射逐行完整性（37 行）

**结论：37/37 行都有可读、可定位的证据；未发现「说了有、实际找不到」的行；未发现漏项未声明。**

| R | 证据（用例 ID / 文件） | 核实到的内容 | 判定 |
| --- | --- | --- | --- |
| R1 | `session_resume_wire_round_trip_keeps_unknown_fields_byte_exact`、`…typed_encoding_and_decoding_are_inverse`、`…is_registered_as_a_conditional_client_to_agent_request`（acp-protocol/tests/session_resume.rs:41/76/167） | 三个 `#[test]` 存在；第 3 个断言 `delivery == ConditionalMvp` | ✅ |
| R2 | 同上 + `session_resume_response_round_trip_keeps_unknown_fields`（:101） | 请求/响应两侧均逐字节比对 | ✅ |
| R3 | `missing_required_fields_are_rejected_without_defaults`（:120）、`a_resume_request_missing_cwd_is_rejected_at_the_decode_boundary`（:154） | 都存在；`null`/类型不符分到 `InvalidField` | ✅ |
| R4 | `session_load_stays_unsupported_and_resume_is_implemented`（:161） | 断言 `session/load` 仍 `NotImplemented` 且其余三方法未顺带提升 | ✅ |
| R5 | `creating_two_sessions_exposes_two_distinct_agent_session_ids`（agent-host/tests/resume.rs:399）＋ app 侧 `load_recovery` 读回 | 存在；两次创建标识互不相同 | ✅ |
| R6 | 同上 | 同一用例覆盖 | ✅ |
| R7 | **复用** `agent-host/tests/session.rs:906 failed_session_new_yields_no_endpoint_and_no_identifier` | **已核实存在** | ✅ 复用成立 |
| R8 | `the_wire_level_resume_params_carry_the_persisted_values`（:186）、`a_resumed_endpoint_dispatches_exactly_one_prompt_and_completes_the_turn`（:236） | 均经 `CARGO_BIN_EXE_acpr-fake-acp-agent` 真实 stdio 子进程 | ✅ |
| R9 | 同上第 2 条 | 恢复后 turn 真的完成 | ✅ |
| R10 | `an_undeclared_capability_sends_no_resume_request_and_reclaims_the_child`（:280）、`a_null_resume_capability_is_the_same_as_an_omitted_one`（:346，三形态循环） | 存在；用修正后的规格措辞，**无**「不 spawn」字面断言 | ✅ |
| R11 | `an_agent_refusal_sends_one_resume_request_and_no_new_session`（:387） | 恰好一条 `session/resume`、零条 `session/new` | ✅ |
| R12 | `resuming_twice_reuses_one_process_and_leaves_one_dispatchable_endpoint`（:434） | 前提已按 CR5-F2 改为「进程复用、单一心跳文件」 | ✅ |
| R13 | `a_too_new_database_is_refused_without_touching_a_single_byte`（:452）、`two_consecutive_opens_leave_the_schema_byte_identical`（:497） | 前者比对 `user_version`/行/DDL/`meta` 四份快照 | ✅ |
| R14 | 同上第 2 条 | 三次打开 DDL 逐字节相同 | ✅ |
| R15 | **复用** `storage-sqlite/tests/migration.rs:1381 a_failed_upgrade_rolls_back_to_v1` | **已核实存在**；断言 `user_version` 仍 1 | ✅ 复用成立 |
| R16 | `upgraded_sessions_keep_their_bytes_and_report_no_recovery_data`（:302） | 其余列 `quote()` 拼接逐字节 + `typeof(...)= 'null'` 排除空串 | ✅ |
| R17 | `recovery_columns_round_trip_byte_exactly_and_are_never_derived`（:397） | 新 store 实例读回 + 库内 `quote()` 复核原文 `/work/demo/./nested/` | ✅ |
| R18 | `v5_appends_the_recovery_columns_instead_of_rebuilding_owned_session` 的列清单段＋ **复用** `migration.rs:1023 v4_database_upgrades_to_v5_by_appending_the_recovery_columns_only` | **已核实**该复用用例对全部 owned 表做全表比对 | ✅ |
| R19 | **复用** `migration.rs:814 v3_database_upgrades_to_v5_by_appending_the_export_id_and_recovery_columns_only` | **已核实存在** | ✅ |
| R20 | **复用** `migration.rs:659 v2_fixture_upgrades_to_v3_and_widens_only_the_audit_checks` | **已核实存在** | ✅ |
| R21 | `a_half_null_recovery_pair_is_still_no_recovery_data`（:358）、`recovery_columns_round_trip…`（:397） | 缺任一列即 `Ok(None)`，不返回半条 | ✅ |
| R22 | `a_successful_resume_…`、`workspace_revalidation_…`、`any_resume_payload_field_is_rejected_before_side_effects`、`a_crash_window_…` | 四条都存在，均含 `assert_same_recovery` 前后逐字段比对 | ✅ |
| R23 | `a_successful_resume_…`、`workspace_revalidation_…`①②③ | 存在 | ✅ |
| R24 | `workspace_revalidation_…`①②（`resume_probe.calls()` 不变）＋ `#[cfg(unix)] an_inaccessible_persisted_directory…` | 存在（后者未执行） | ✅ |
| R25 | `a_persisted_cwd_whose_canonical_form_differs_is_refused_before_any_backend_call`（:1250，**本机实跑**）＋ 两条 `#[cfg(unix)]` ＋ 同族变体 | 平台无关变体存在且**本机 PASS**；两条 unix 变体存在但**未执行** | ⚠️ 见 D 与 CR8-F5 |
| R26 | `the_wire_level_resume_params_carry_the_persisted_values`（线级）+ `a_successful_resume_…` | 存在 | ✅ |
| R27 | `the_command_catalog_row_matches_the_wire_contract`、`session_scoped_and_version_rules_are_enforced_at_the_decode_boundary`、`a_completed_resume_terminal_carries_the_named_result_payload` | 均存在 | ✅ |
| R28 | `a_repeated_resume_request_id_replays_the_first_result_once` | 逐字回首次 result + `calls()==1` | ✅ |
| R29 | 同用例第二段 | 同 `requestId` + 不同 `command`/`payload` → `idempotency_conflict` + `calls()` 仍 1 | ✅ |
| R30 | **复用** `server/src/node_link/command/tests.rs` 的越权用例 | **已核实存在两条**：`an_unauthorized_command_is_rejected_audited_and_has_no_side_effect`(:1509) 与 `an_unauthorized_session_resume_is_rejected_before_any_local_read`(:2888)。但**报告未点名**（CR8-F3） | ✅ 实质成立，登记不足 |
| R31 | `an_unauthorized_resume_is_rejected_before_any_local_read` | 计数器自检 + 计数不变 + 存在/不存在同 `code` 同 `details` | ✅ |
| R32 | `a_crash_window_leaves_uncertain_persisted_across_a_reopened_store` | 注入命中断言 + wire `command.status` + **重开存储**读回持久行 | ✅ |
| R33 | **复用** `server/.../tests.rs` 的限流用例 | **已核实**：`the_command_rate_limit_replies_rate_limited_and_then_closes`(:2303)；`command.rs:248` 的 `admit_rate` 在 `match submit.command` **之前**、对全部命令生效 ⇒ 按构造成立。**报告的指针有误**（CR8-F3） | ✅ 实质成立 |
| R34 | `any_resume_payload_field_is_rejected_at_the_decode_boundary`、`a_completed_resume_terminal_carries_the_named_result_payload`、`session_scoped_and_version_rules_…`、端到端 4 字段循环 | 均存在 | ✅ |
| R35 | `a_successful_resume_returns_the_session_reference_and_leaves_the_columns_untouched` | 终态含 `remoteSessionRef` + `sessionMeta.state == "idle"`，随后 `session.prompt` 真派发 | ✅ |
| R36 | `any_resume_payload_field_is_rejected_before_side_effects` + wire 侧解码边界用例 | 均存在 | ✅ |
| R37 | `an_unsupported_agent_fails_the_resume_terminal_without_creating_a_session` | `failed` + `nodelink.command.unsupported` + 与「Agent 拒绝」码 `assert_ne!` | ✅ |

**6 行「上游复用」的独立核实结果**：R7 / R15 / R19 / R20 / R30 / R33 **全部成立**——被复用的用例**都真实存在**、断言**与各自 R 行的可观察保证等价**、且都在 `-p <crate>` 全量运行的 PASS 日志里。**未发现「登记了复用、实际那条用例不存在或不跑」的情况。**

## B. 断言判别力抽查

### B1. R25 平台无关变体 — **判别力强**

产品实现在 `crates/core/src/broker.rs:3628–3641` 的 `revalidate_resume_workspace` 恰好是四条守卫：`is_absolute` → `metadata` → `is_dir` → `canonical.to_str() == workspace_cwd`。用例的前置断言**逐条对位**这四条，因此**失败点唯一**。

| 回退 | 会先炸的断言 |
| --- | --- |
| 跳过 `canonicalize` 逐字比对 / 整体跳过复校验 | ①`status=="failed"` ②`calls()==calls_before` ③`last_request().is_none()` |
| 复校验失败后回退到「按别名重新解析」 | 同上（前置③已证明新解析值**恰好就是那个合法已注册目录**） |
| 正确拒绝但顺手把持久化取值改写成新解析值 | ④`assert_same_recovery(&tampered, &after)` |
| 正确拒绝但兜底新建会话 | ⑤`listed.len() == 1` |
| 错误码归到 `command.unsupported` | ⑥`code == "nodelink.internal.unavailable"` |

**注入不是空转**：`overwrite_persisted_workspace_cwd` 断言 `rows_affected() == 1`，且前置②用**产品读路径** `load_recovery` 断言读回的正是那个未规范化字符串。**通过。**

### B2. R31 读计数断言 — **现在真的有判别力**

- **共用同一实例已核实**：`owner.rs:135` 建**一个** `recovery_reads`，`:140` 装饰器 `Arc::clone`，`:267` `OwnerNode` 字段直接用同一个 Arc。第 1 轮「两个 Arc」的缺陷已闭环。
- **断言不再恒真**：用例先自己调一次 `load_recovery` 并断言 `recovery_reads() > probe_before`，把「计数器活着」变成前提而不是假设。
- **回退推演**：若把 `broker.rs:1496` 的 `authorize` 挪到 `:1500` 的 `load_recovery` 之后 ⇒ 计数 +1 ⇒ 断言失败；且目标会话目录**已被删除**，错误码会从 `nodelink.export.not_granted` 变成 `nodelink.internal.unavailable` ⇒ 前一条断言也失败。**两条独立失败。**
- **计数口径有效**：`load_recovery` 是 resume 路径**唯一**的会话行读取口。

### B3. CR6 口径 — **能区分 `uncertain`**

`assert_eq!(status, "failed")` 而非 `matches!(failed | uncertain)`，因此**回归到 `uncertain` 必然失败**；`assert_eq!(code, "nodelink.command.unsupported")` + 与「Agent 拒绝」码 `assert_ne!` ⇒ 两类失败不共漏斗；会话清单仍 1 条、两列未改写提供副作用证据。**通过。**

### B4. R29 跨语义 requestId 复用 — **有效**

若实现不查冲突，第二次提交会真的执行 `session.create` 并回 `accepted`/`terminal`；而 `NodeLinkClient::expect` 只跳过 `resource.event`，其它类型直接 panic ⇒ **响亮失败**。跨**会话**复用的等价证据由 WP6 的 `session_resume_reusing_a_request_id_for_another_session_is_an_idempotency_conflict`(:2773) 承担，报告未点名。

### B5. R32 崩溃窗口 — **注入不空转，断言分层正确**

注入命中性有独立断言 `rejected_commits >= 1`；权威终态是 wire `command.status` 得 `uncertain` + **重开同一 `data_dir` 的真实 store** 从持久行读回；`recover_unsettled(&Actor::LocalCli, …)` 确实就是组合根启动时调用的入口，不是测试专用旁路。崩溃窗口那一帧**刻意不断言其 `status`**，与 `docs/NODE_LINK_PROTOCOL.md §12.7` 的收窄条款一致——**诚实的能力边界标注**。

### B6. 其余抽查（要点）

- **R17**：写入 `/work/demo/./nested/` 后跨 store 实例读回**逐字**相同 + 库内 `quote()` 复核原文 ⇒ 若存储层偷偷归一化，两处都失败。
- **R21**：两行分别「只有一列」⇒ 若实现改成「有一列就返回半条」，`assert_eq!(None)` 失败。
- **R10**：线级 `dump_count("session/resume")==0` + `session/new==0` + `initialize==1`（证明门控位置）+ `assert_process_reclaimed` + `!runtime_running()` + 事件流空。
- **R12**：断言口径被**诚实下调**为「`initialize` 恰好 1 行」+「恰好一个端点可派发」，注释明确记录实测边界。

### B7. 全面排查

全 `crates/**` 内**无新增** `#[ignore]`（两处既有的属 crash-child/夹具生成器）；TP2 的 5 个新文件零 `#[ignore]`、零 `#[should_panic]`、零 `todo!`；五个目标全部 `0 ignored`；无「仅截图式」断言；新用例名与上游既有测试无重复。

**B 项结论：未发现新的恒真断言。**

## C. 两处清单改动的核对

| 核对项 | 结论 | 证据 |
| --- | --- | --- |
| C-1 diff 只有两行 | **内容级确认；diff 级受限** | `crates/app/Cargo.toml`：`sqlx = { workspace = true }` 是 `[dev-dependencies]` 的**最后一行**，全文件**仅此一处** `sqlx`；**未改 `[dependencies]`**、**未加 `[features]`**、**未写版本号**。`Cargo.lock:135`：`app` 条目 `dependencies` 数组中**恰好一行** `"sqlx",`，位置在 `"sha2 0.11.0"` 与 `"storage-sqlite"` 之间（字母序正确）。 |
| C-2 `sqlx` 是既有依赖、供应链面不变 | **成立** | 根 `Cargo.toml:52` 已有 `sqlx`；`crates/storage-sqlite/Cargo.toml:22` 已在用；`Cargo.lock` `sqlx 0.8.6` 包条目带 registry source。`app` 侧用 `workspace = true` 继承，**不新增 feature、不升级版本** ⇒ 被构建的 crate 集合不变 ⇒ `cargo-deny`/`gitleaks` 判定集合不受影响。**未实测**（仅 CI 可跑）。 |
| C-3 依赖方向门禁仍成立 | **成立** | `scripts/check-crate-boundaries.mjs:82–83` 只把 `typeof dependency.path === "string"` 的依赖建成边；`sqlx` 是 registry 依赖、**无 `path`** ⇒ 不进 `workspaceEdges`、不参与 §5 矩阵比对。`CORE_FORBIDDEN` 只作用于 `core`，且 `core` 闭包用 `cargo tree -p core --edges **normal**`，`app` 的 dev 依赖不进 `core` 的 normal 闭包。 |
| C-4 `--locked` 成立 | **成立** | `tp2-tester-a2-metadata-locked.log`：`# revision: f44301e` / `# exit: 0`。实现者「暂停上报再获授权」的处理**符合流程**，不是越界。 |

## D. `#[cfg(unix)]` 两族的处置判定

### D-1 标注是否如实 —— **是**

三处交叉印证：① 用例自身 rustdoc 逐条写明「本机是 Windows 开发机，本用例**从未执行**；由 Linux CI 的 `checks` job 真实执行。**本地未执行不等于通过**」；② `--list` 日志恰好 8 条、**不含**这两条（这是「未执行」的直接机器证据，不是「通过」的证据）；③ `tp2-tester.md §8.8` 把「编译与执行」「交叉编译失败原因」「Linux 行为未验证」全部列入「未执行项（不冒充通过）」。**没有任何一处把未跑的东西写成通过。**

### D-2 「`#[cfg(unix)]` 在 Windows 被剔除 + clippy exit 0 ⇒ 不会破坏 Windows 构建」—— **成立**

两个 `#[test] fn`、两个 `struct/impl`（`ModeRestore` 及其 `Drop`）**全部**带 `#[cfg(unix)]`；`std::os::unix::fs::{symlink, PermissionsExt}` 的三处引用**全部在 `#[cfg(unix)]` 边界内**。`clippy --all-targets --all-features -D warnings` exit 0 ⇒ Windows 侧无未用项/未解析符号/平台 API 误用告警。**这条证据只支撑「不破坏 Windows 构建」，不支撑「Unix 上能编译」。**

### D-3 `chmod 000` 打在父目录 —— **处理正确**

POSIX 语义：路径查找中每个分量需要其**父目录**的 search 权限；叶子目录自身的模式位不参与它自己的查找。只 chmod 叶子目录，`metadata`/`canonicalize` 很可能仍成功 ⇒ 变成「本该失败却通过」的假证据。**前提自证**：`assert!(std::fs::metadata(&work).is_err(), "chmod 000 必须让持久化路径不可访问；若本进程是 root，POSIX 权限检查对它无效")`——以 root 运行时**响亮失败**而不是产出假证据。**清理**：`ModeRestore` 的 `Drop` 恢复 `0700`，且 `drop(restore)` 在 `remove_dir_all` 与 `owner.stop()` **之前**，panic 路径也不会留下 `000`。符号链接族的前置同样自证，**失败点唯一**落在逐字比对。

### D-4 是否应视为「已覆盖」—— **分三档，不合并**

| 档 | 内容 | 判定 |
| --- | --- | --- |
| ① 已覆盖（可计入 PASS） | `a_persisted_cwd_whose_canonical_form_differs_…`（平台无关，本机实跑 `ok`）＋ `workspace_revalidation_…` 的删除/别名改指/同名重建三变体 | ✅ 可计入 |
| ② **设计上覆盖、判定待 CI** | 符号链接改指族、`chmod 000` 族 | ⚠️ **不得记 PASS**。既无编译证据也无执行证据。在 CI `checks` job（`ubuntu-latest`，非 root）首次编译+执行之前，**状态是 PENDING**。若 CI 上失败（最可能是编译期），应开新问题编号并回 TP2 |
| ③ 未覆盖 | 第 1 轮的 A1/A2 | 已由第 2 轮移入 ② |

## E. 实现者自报的两处自查 —— 独立核实

### E-1 `recovery_reads` 双 `Arc` 修正 —— **核实成立**（见 B2）

共用同一 `Arc`（`owner.rs:135/140/267`），并新增「计数器确实在动」的自检断言。实现者主动登记自己上一轮的缺陷——**诚实**，不因「测试全绿」而掩盖。

### E-2 CR4-F3 的纠正 —— **核实成立，论证与反证都成立**

- **main 建议的判别式不成立**：`ALTER TABLE … ADD COLUMN` 会把 `IF NOT EXISTS` 从 `sqlite_master` 存储文本里去掉，而 12-step 重建脚本本来就不含它 ⇒ 两条路径都不含 ⇒ 断言恒真。**接受该实测结论。**
- **反证结构成立**：用 v1 库升级后的 `owned_audit` 断言 `rebuilt.starts_with("CREATE TABLE \"owned_audit\" (")` **且** `!rebuilt.starts_with("CREATE TABLE owned_audit (")`。若判别式方向写反或恒真，这条会立刻失败。**判别式不恒真，可核实。**
- **唯一问题**：该用例上方的 **rustdoc 注释仍是第 1 轮的旧版本**，写着「① DDL **含** `IF NOT EXISTS`」「④ 反证：…其文本**不含** `IF NOT EXISTS`」，与代码断言**正好相反**。见 CR8-F1。

## F. 边界

| 核对项 | 结论 |
| --- | --- |
| `crates/**/src/**` 零改动 | **[diff 级受限]** 内容级旁证：五个新文件全在 `crates/*/tests/`；`owner.rs` 属 SFO 已登记的 `crates/app/tests/` 行。**请 merger/主 Agent 补 `--name-only` 机械确认** |
| `compatibility/`、`schemas/`、`fixtures/` 零改动 | **[diff 级受限]** 内容级旁证：nlp 用例只**只读**合同资产，断言值与冻结契约**逐字相符**（若被改动，PV2 会红） |
| 新增错误码 / feature | **无。** 只**断言**既有词表取值 |
| `#[ignore]`/删断言/弱化阈值换绿 | **无。** R25 的形态从 `/./` 改为「尾部分隔符」是**实测标定的等价替换**，不是弱化 |
| `ScriptedBackends::create` 由 `None` 改 `Some` | 核实为**测试替身范围内的必要变更**，不触碰产品代码；`-p app` 全量逐目标计数与第 1 轮**逐项一致**（仅 `session_resume_e2e` 7→8），既有目标全绿 ⇒ **不构成回归** |

## Findings

| ID | Severity | Location | Impact | Recommendation |
| --- | --- | --- | --- | --- |
| **CR8-F1** | MINOR | `crates/storage-sqlite/tests/resume_columns.rs:130–147`（`v5_appends_…` 的 rustdoc） | **代码本身正确、判别式成立**，但注释与代码**直接矛盾**。后续维护者若「按注释修断言」，会把一条经过实测标定的判别式改回恒真断言 | 把该 rustdoc 的 ①/④ 改成与代码一致（表名引号判别式 + `owned_audit` 反证必须**带引号**），删掉 `IF NOT EXISTS` 的正向表述 |
| **CR8-F2** | MINOR | `reports/tp2-test-design.md` 头部、§5、§6 | 报告把第 2 轮交付提交写作 **`4146611`**，而 `tp2-tester.md §8`、`tp2-tester-a2-metadata-locked.log` 与派发书都是 **`f44301e`**；分支提交序列 `f8133f2 → a02e2fd → f44301e` **不含** `4146611` | 证据与版本的绑定链断一环 | 把三处 `4146611` 订正为 `f44301e`（或注明 `4146611` 为 amend 前 SHA 及其对应关系） |
| **CR8-F3** | MINOR | `reports/tp2-test-design.md §2` 的 R30 / R33 两行 | R30 未给用例名（实际两条：`:1509`、`:2888`）；R33 的 `:992` 指针**不成立**（该行落在 `session_submit_body` 的 session-scope `matches!` 里，与限流无关；真正证据是 `the_command_rate_limit_replies_rate_limited_and_then_closes`:2303）。**实质覆盖成立**（`command.rs:248` 的 `admit_rate` 是连接级、对全部命令生效） | R30 补用例名；R33 改为正确指针 + 说明 `admit_rate` 的连接级语义 |
| **CR8-F4** | MINOR | `reports/tp2-test-design.md §2` 首段与末尾统计行 | 仍写「新增 **32** 条」与「app **7**」，而 §6 与日志显示 app 已增至 **8**（合计 33 条本机可执行 + 2 条 `#[cfg(unix)]`）；报告内部计数不自洽 | 更新为 33（app 8），并注明另有 2 条 `#[cfg(unix)]` 待 CI |
| **CR8-F5** | MINOR | `reports/tp2-test-design.md §2` 的 R25 行「结果」列；`session_resume_e2e.rs:1013–1018` | ① R25 写「**已覆盖**」，把「1 条本机实跑 PASS」与「2 条零证据的 `#[cfg(unix)]`」并成一档；② 该用例 doc/③ 段注释说它覆盖「两列为 `NULL`」，但第 ③ 段实际只把**受控后端**切到 `BackendUnsupported`，**并没有造出两列 `NULL` 的会话**（NULL 侧真实证据在 `server/.../tests.rs:2936` 与 `resume_columns.rs`） | R25 拆成「1 条本机 PASS + 2 条待 CI」；③ 段注释改为「能力不支持（本用例） vs 两列 `NULL`（由 `server/.../tests.rs::session_resume_without_persisted_recovery_data_fails_as_unsupported` 承担）」 |
| **CR8-F6** | MINOR | `session_resume_e2e.rs:1391–1560`（两条 `#[cfg(unix)]`） | 两条**既无本机编译证据也无执行证据**；交叉 `cargo check --target x86_64-unknown-linux-gnu` 因缺 `x86_64-linux-gnu-gcc` 失败。实现者已如实标注，**不构成不诚实**。残余风险：若在 Unix 上有编译错误，Linux CI 会失败并卡住候选门禁。已在源码层逐项核对 cfg 边界、符号引用顺序、`ModeRestore` 覆盖与 `Drop` 时序，**未发现编译隐患**，但这不等于已验证 | 在候选/CI 门禁前确认两条在 `ubuntu-latest` 上编译并执行；失败则开新问题编号回 TP2（不改判本轮 PASS） |
| CR8-S1 | SUGGESTION | `session_resume_e2e.rs:663–667` | `assert_eq!(details 相等)`——若两侧同为 `null` 则**平凡成立**（R31 判别力实际由前一条 `code` 相等 + 目录已删除承担） | 可选加固：先断言 `details.is_object()` 再比相等 |
| CR8-S2 | SUGGESTION | `crates/app/tests/support/owner.rs:1–17` | 模块注释声明「**两处**刻意的测试替身」，但第 2 轮新增的 `overwrite_persisted_workspace_cwd`（经 `sqlx` 原始 SQL 直接改写 `owned_session`，**绕过全部端口与本地管理方法**）是一种影响更大的新介入，未登记 | 在模块注释补第三条 |

**未发现 CRITICAL / MAJOR。**

---

## Assessment

- **本轮结论：PASS**（0×CRITICAL / 0×MAJOR）
- **A**：Coverage Index 37 行**全部有可读、可定位、真实存在的证据**；6 行上游复用逐条独立核实成立；无证据不实、无漏项未声明。
- **B**：逐条推演后未发现新的恒真断言。
- **C**：两处清单改动内容级全部核对通过；依赖为既有依赖、供应链面不变、方向门禁不受影响、`--locked` 成立。
- **D**：`#[cfg(unix)]` 标注**如实**；`chmod 000` 处理**正确**；**判定为「设计上覆盖、判定待 Linux CI」，不得记 PASS**。
- **E**：两处自查**核实成立**。
- **F**：未发现 `#[ignore]`、删断言或弱化阈值换绿的痕迹；`ScriptedBackends::create` 的替身变更不构成回归。

### 待补证据

| 待补项 | 影响本轮判断 | 应在哪个门禁前补 |
| --- | --- | --- |
| `git diff --name-only 95051f9..f44301e`（证明 src/compatibility/schemas/fixtures 零改动） | 否 | merger 候选 review / 合入前 |
| `git diff a02e2fd..f44301e --stat`（清单恰两行） | 否 | 同上 |
| `4146611` 是否为真实提交 | 否 | 记录订正（CR8-F2） |
| 两条 `#[cfg(unix)]` 在 `ubuntu-latest` 的编译与执行 | 否 | **Linux CI `checks` job**；失败则开新问题编号回 TP2 |
| PV1 阶段 2、PV2、`cargo-deny`、`gitleaks` | 否 | 候选 / 主分支门禁（交 merger） |
| validator 覆盖充分性判定（7.1） | 否 | `final` 阶段 |

### handoff_index

- `task_id: "2.8"` · `work_package: TP2` · `role: reviewer` · `phase: test-case` · `round: 1` · `stage: work-package` · `target_revision: "f44301e"` · `evidence_type: REVIEW` · `evidence_id: CR8` · `result: PASS` · `evidence_status: NEW` · `report_path: "openspec/changes/session-resume/reports/cr8-review.md"`
- `applicability_basis`: 静态检视 f44301e 的 5 个新增测试文件与 crates/app/tests/support/owner.rs；逐行核对 Coverage Index 37 行；核实 6 行上游复用的存在性与等价性；推演判别力；核对两处清单改动；判定 `#[cfg(unix)]` 两族为「设计上覆盖、判定待 Linux CI」。未执行任何 cargo/npm；diff 工具不覆盖已提交范围，已在限制 1 明示。
