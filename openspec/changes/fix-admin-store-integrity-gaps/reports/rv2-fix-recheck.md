## 固定字段

| 字段 | 值 |
| --- | --- |
| task_id | RV2（复核；新发现编号前缀 `RV2-F<n>`） |
| role | reviewer（独立代码检视子 Agent；isolated，未参与实现、未参与上一轮检视，未继承实现对话） |
| phase | 工作包交付前（branch validation，候选合入之前） |
| agent_context | 新上下文子 Agent；只读；未修改任何文件、未切换分支、未提交、未勾任务、未写 `verification.md`、未执行任何 shell 命令；不执行 E2E |
| target_revision | base `1ef6640431079f9ca22b409c42071de2d9a79fa2` + 工作区未提交改动层。调度者标识 tracked diff sha256 = `945d1d509b428a8b84e8d2168d980600098a14d57086d8bd2e4c6957b21e6170`；我按内容逐文件比对 `watchdog_diff` 与 `reports/branch-diff.patch`（hunk 位置与计数一致）、与 `reports/branch-diff-stat.txt`（6 文件、+556/−69）一致 |
| scope | 本轮复核 RV1 两条发现的修复与其回归面：`crates/storage-sqlite/tests/imported.rs`（`receipt_without_imported_session_is_rejected`、`seed_import`/`seed_session` 夹具、新用例）、`design.md` D5/Context/D3、`tasks.md` 2.2；并按需核对 `session_store.rs` 守卫顺序、`specs/**` R1–R12 自洽性、`plan.md`/`proposal.md` 措辞一致性。未重复检视 RV1 已判定的四条守卫主体实现与文档落点 |
| changes | 相对 RV1 版本仅 `crates/storage-sqlite/tests/imported.rs` 变化（+7/−2 行，per-file stat 183→192）；其余 5 个文件的 hunk 头与 per-file 计数与 `reports/round1/branch-diff.patch` 逐项相同；`design.md`/`tasks.md` 为未跟踪文件，按全文核对 |
| checks | 只读核对 LC1（`lc1-admin-store.log` 32 passed、`lc1-imported.log` 10 passed）、PV1–PV5 日志；**我未执行任何命令**，不冒充执行；`cargo-deny`/`gitleaks` 本地未执行（CI-only） |
| issues | RV1-F1 **已解决**；RV1-F2 **已解决**（在 RV1 指名的两处）；新增 RV2-F1（SUGGESTION/P2，非阻断）。无 CRITICAL/MAJOR |
| result | **PASS**（对应上述 Target Revision） |
| evidence_paths | `reports/{branch-diff.patch,branch-diff-stat.txt,lc1-admin-store.log,lc1-imported.log,pv1-fmt.log,pv2-clippy.log,pv3-cargo-test.log,pv4-check.log,pv5-verify.log,rv1-fix.md}`、`reports/round1/*`（上一轮归档）、`proposal.md`、`specs/**/spec.md`、`design.md`、`plan.md`、`tasks.md`、`verification.md`、`docs/CORE_PORTS_AND_STORAGE.md`、`crates/storage-sqlite/{src,tests}/**` |
| resource_cleanup | 未创建、删除或运行任何东西；无 `target/`、无临时 SQLite、无日志写入；无残留 |

### Review Context

- Review ID / Type / Stage：**RV2 / recheck / 工作包（WP1、WP2、WP3）交付前**。
- Repository：`D:/Project/acp-remote`（Windows，本地 `refs/heads/main`）。
- 读取的规则与需求（全文/相关章节）：`openspec/schemas/agentic/roles/reviewer.md`（全文先读）、`AGENTS.md` §1/§3/§4/§7/§9/§10/§12、`proposal.md`（含 `agentic-intent`）、`specs/admin-state-persistence/spec.md`、`specs/peer-identity-material/spec.md`、`design.md`（Context、D1–D6、Risks）、`plan.md`（Contract Changes、Coverage Index R1–R12、Verification Strategy、Code Review 关注点）、`tasks.md`（2.x/3.2）、`docs/CORE_PORTS_AND_STORAGE.md` §5.2/§5.3/§7.4/§9 判据 30/§11.2 第 5 条、`verification.md`。
- 实际检查范围：
  1. 本轮 diff 的实际构成（`watchdog_diff` 逐文件；hunk 头与 `branch-diff.patch` 一致），确认修复只触及 `tests/imported.rs`；
  2. `receipt_without_imported_session_is_rejected` 的夹具与断言（`imported.rs:312-338`）、`seed_import`/`seed_session`（`imported.rs:74-100`）及其对同文件 8 条用法的回归面（逐条读用例与断言）；
  3. `commit_receipt`/`upsert_session` 的守卫与分支顺序（`session_store.rs:2445-2489`、`2666-2674`）；
  4. `design.md` D5/Context/D3 与 `tasks.md` 2.2 的订正是否与代码事实一致；`plan.md`/`proposal.md` 同措辞残留；
  5. Coverage Index R1–R12 与现有用例的对应（`admin_store.rs` 三组新/扩用例、`imported.rs` 新用例、正向对照断言）；
  6. LC1/PV1–PV5 日志只读核对，含新旧日志的时间戳与内容差异。
- 限制：无法自算 sha256、无法使用 git（不切换分支/不读提交范围），因此「工作区 = 调度者标识的 target」只能由逐 hunk 内容比对支持；不执行构建/测试；不执行 E2E。

### Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| RV1-F1 | MINOR（P2，非阻断） | `crates/storage-sqlite/tests/imported.rs:312-313,318,328-331`（本轮复核）；相关 `session_store.rs:2475-2489` | **已解决**。① 夹具已先建档：`imported.rs:318 seed_import(&store).await;`（`seed_import` 定义 `:74-96`，经 `add_import` 写 `imported_import` + `imported_import_export`），因此 `ensure_import_owns`（`session_store.rs:2445-2465`）在 `commit_receipt` 中通过；随后 `next_local_sequence` 查询（`:2476-2488`）无行 → `ok_or_else` 返回 `NotFound(EntityRef::Session(session_id))`，被绕过的分支**重新被触达**（顺序：归属前置 → `next_local_sequence` → `NotFound(Session)`）。② 断言已收窄为 `matches!(&error, PortError::NotFound(EntityRef::Session(id)) if id == &session())`（`:328-331`），且保留「`imported_delivery_index` 计数为 0」（`:333-338`）。③ 可证伪性：删掉 `:318` 的 `seed_import` → 守卫先返回 `NotFound(Export)` → 断言失败；把 `:2488` 的错误变体改成任何非 `Session` 值 → 断言失败；返回 `NotFound(Session(其他 id))` 亦失败。④ 文档已同步：用例注释 `:312-313`、`design.md:122-125` 均改为「归属行在、会话行不在 → `NotFound(Session)`，已先 `seed_import` 并收窄断言（RV1-F1）」，与代码一致 | 原 P2 覆盖缺口闭合：§7.4 `[决定]`（「会话行不存在时 `commit_receipt` 返回 `NotFound`」）恢复定向回归 | 无需进一步动作 | **本轮（RV2）复核：已解决** |
| RV1-F2 | SUGGESTION（P2，仅报告） | `design.md:10,86-87`、`tasks.md:14` | **已解决**（在 RV1 指名的两处）。`design.md:10` 改为「…同样不读该列（模块头未涉及该口径）」，`design.md:86-87` 与 `tasks.md:14` 改为「修正 `load_peer_key` 文档注释里「指纹列不参与判定」的口径（**模块头无需改动**，RV1-F2）」。代码事实核对：`crates/storage-sqlite/src/admin/trust.rs` 的模块头（`1-21`）不含该口径语句，且 tracked diff 在 `trust.rs` 的第一个 hunk 起点是 `@@ -40,8 +40,9 @@`（`DEVICE_COLUMNS`），即 base→target 在 `1-39` 区间无改动 → 「模块头无需改动」的表述为真；被修正的注释位于 `trust.rs:259-262`（`load_peer_key` 文档注释，diff hunk `@@ -222,8 +257,9 @@`） | 不再把实际不存在的改动写成改动项 | 无需进一步动作 | **本轮（RV2）复核：已解决**（但见 RV2-F1 的残留位置） |
| RV2-F1 | SUGGESTION（P2，仅报告，非阻断） | `plan.md:15`、`proposal.md:78` | 同一类事实精度残留，落在 RV1-F2 未指名的两个文件：`plan.md:15` 的 Contract Changes 仍写「`crates/storage-sqlite/src/admin/trust.rs` **模块头与** `load_peer_key` 的注释（「指纹列不参与判定」→…）」；`proposal.md:78` 的 Impact 仍写「`crates/storage-sqlite/src/admin/trust.rs`（…）**与其模块头注释**」。证据：`trust.rs` 模块头（`1-21`）在 base 与 target 中均无该口径语句，tracked diff 在 `trust.rs` 无 `1-39` 区间 hunk（首个 hunk 为 `@@ -40,8 +40,9 @@`）；而 `design.md:86-87`/`tasks.md:14` 已声明「模块头无需改动」。三份变更内文档因此互相矛盾 | 无代码、无行为、无门禁影响（`check:contract-drift`/`check-doc-links` 不判此措辞）；影响仅限变更内文档一致性与最终验收对 `proposal.md` Impact 的核对 | 在同一轮内把 `plan.md:15`、`proposal.md:78` 的该措辞改为实际位置（`load_peer_key` 的文档注释）或补注「（模块头无需改动）」，使 proposal/plan/design/tasks 口径一致；属主 Agent 写入范围 | 新发现（RV2） |

**未发现问题（已核对为正确，逐条列证据）**

1. **修复范围最小、无隐藏改动**：本轮相对 RV1 版本只有 `tests/imported.rs` 变化（per-file stat 183→192；总 +7/−2，与统计 +556/−69 vs +549/−67 完全吻合）。增量可逐行拆解为三处：`+ seed_import(&store).await;`（1 行）、用例注释 1 行改 2 行（+2/−1）、断言 1 行改 4 行（+4/−1），合计 +7/−2 —— 无其它改动。其余 5 个文件的 hunk 头（文件序、`@@` 区间与计数）与 `reports/round1/branch-diff.patch` 逐项相同，`branch-diff-stat.txt` 各列数值相同，故 RV1 对那 5 个文件的结论不被本轮修复影响。
2. **修复未引入回归（夹具面）**：`seed_session` 现在先 `seed_import`；`imported.rs` 中 `seed_session` 的 8 个调用点分属 8 个不同 `#[tokio::test]`（`:239`、`:347`、`:427`、`:534`、`:615`、`:725`、`:788`、`:831`，逐一读过），每个用例各自 `temp_dir`（`support/mod.rs:44-46` 用 `std::env::temp_dir()` + 进程 id，目录名各异），因此不存在同一用例内重复 `add_import` 触发 `UNIQUE` 冲突的路径。
3. **`drop_import_keeps_audit_rows` 的夹具改写自洽**：删除的那两段 raw `INSERT`（`imported_import`/`imported_import_export`）确实已由 `add_import` 承担；该用例断言 `imported_audit` 计数为 `1`（`:400-404`）同时证明 `add_import`（`audit: Vec::new()`）不写审计行 —— 与新用例 `imported_audit == 0`（`:520-523`）一致，两处不矛盾。断言 `delivery_index_removed == 1`、`command_refs_removed == 1`、行集为 0 均保留。
4. **新用例仍可证伪且断言对应 spec R4**：`late_callbacks_after_a_full_removal_cannot_rebuild_the_index`（`:417-528`）在 base 上会 panic（`upsert_session` 未被守卫时成功 → `expect_err` 失败），错误定位断言 `NotFound(EntityRef::Export)`（`:487-490`、`:502-505`），并断言 `imported_import`/`imported_import_export`/`imported_session`/`imported_delivery_index`/`imported_command_ref` 全空（`:507-517`）与 `imported_audit == 0`（`:520-523`），覆盖 spec「保持为空且不产生新的 `local_sequence`」与 design D5（a）(b)(c)。
5. **R1–R12 与用例仍一一对应**：R2/R3 → `export_revocation_is_terminal_for_plain_writes`（`admin_store.rs:2466-2547`，含 `assert_conflict(AlreadyExists)` + `table_snapshot` 逐行不变 + 零审计）；R5–R8 → `timestamps_are_advanced_never_erased`（`admin_store.rs:1957+`：`approve_device` 落 NULL 后首次写入 `Some(at(3))` 覆盖 R6；`Some(at(2))` 晚于 `at(3)` 保留 `at(3)` 且 `display_name` 更新覆盖 R7；`device_record_with_seen(.., None)` 覆盖 R8；`owned_node` 同形）；R9–R12 → `corrupted_identity_material_fails_closed_on_read`（`admin_store.rs:775+`，含「一致时四条读取路径照常」的正向对照 = R12）。本轮断言收窄不改变任何 Coverage Index 行的指向（R4 指向新用例，未受影响）。
6. **设计/任务订正文本与代码事实一致**：`design.md:122-125` 的「已改为先 `seed_import` 并把断言收窄为 `NotFound(EntityRef::Session)`」、`design.md:86-87`/`tasks.md:14` 的「模块头无需改动」均可在源码中找到对应事实（见上表两条 Recheck 依据）。`design.md:13` 引用的 `imported.rs:281` 是 **base** 行号（Context 段明示 `HEAD 1ef6640`），非当前行号，不构成漂移。
7. **未因修复产生越界改动**：`watchdog_diff` 的 tracked 清单仍是 6 个文件，未出现 `migrate.rs`、`crates/core/**`、`schemas/`、`fixtures/`、`compatibility/`、`LOCAL_ADMIN_PROTOCOL.md`；未跟踪新增只有 `openspec/changes/fix-admin-store-integrity-gaps/**`（含 `reports/round1/` 归档与 `rv1-fix.md`），无临时库/构建残留。
8. **上一轮证据确实保留、未被覆盖**：`reports/round1/` 含完整的一套日志与 patch；`round1/pv4-check.log:27` 与顶层 `pv4-check.log:27` 的差异（2487/127 files → 2523/128 files）与 5 个 PV 日志时间戳（23:14 系列 → 23:19 系列）共同说明顶层日志是本轮重跑、且重跑前新增了一个 markdown（`reports/rv1-fix.md`），与「先出 RV1 报告、再修复、再重跑」的顺序自洽。
9. **一处曾被怀疑的表述经核对不构成问题**：`design.md:124-125` 说「新用例覆盖『无归属行』（从未导入与完整移除后在端口层不可区分，走同一条 `NotFound(Export)` 路径）」。新用例覆盖的是「完整移除后」，但「从未导入」在端口层走同一段代码（`ensure_import_owns` 的同一条 `SELECT`，无分支差异），R4 的场景本身也是「完整移除后」，因此该措辞可接受，不构成缺陷（仅提示：若后续要更严格，可另加一条「从未 `add_import` 直接 `upsert_session`/`commit_receipt`」的正向负例）。

### Assessment

**Target Revision：base `1ef6640431079f9ca22b409c42071de2d9a79fa2` + 工作区改动层（tracked diff 逐 hunk 与 `branch-diff.patch`/`branch-diff-stat.txt` 一致）→ 本轮检视结论 PASS。**

- **结论**：RV1-F1 **已解决**、RV1-F2 **已解决**（在其指名的 `design.md`/`tasks.md` 位置），修复未引入回归；新增 1 条非阻断项 RV2-F1（SUGGESTION/P2，同类残留措辞，落在 `plan.md:15`/`proposal.md:78`）。无未解决的 CRITICAL/MAJOR（按本仓库 severity 口径即无阻断项）→ **PASS**，可继续 3.2/后续门禁流程，RV2-F1 建议在本轮顺手订正但不阻断合入。
- **实际检查范围**：本轮 diff 的完整构成与 hunk 级比对；`imported.rs` 全部 10 条用例的夹具/断言（重点是 `receipt_without_imported_session_is_rejected`、`late_callbacks_after_a_full_removal_cannot_rebuild_the_index`、`drop_import_keeps_audit_rows`）；`session_store.rs` 的 `ensure_import_owns`/`commit_receipt`/`upsert_session`；`admin_store.rs` 三组用例与 Coverage Index R2/R3/R5–R12；`design.md`（Context/D3/D5/D6）、`tasks.md` 2.2、`plan.md` Contract Changes/Code Review、`proposal.md` Impact；`specs/**` 两处 `## ADDED Requirements` 与 12 个场景；LC1/PV1–PV5 日志与 `reports/round1/` 归档的只读核对。

**按检查 ID 的证据核对（我只读日志，未执行命令）**

| Check ID | 我核对的证据 | 结论 / 是否影响本轮判断 |
| --- | --- | --- |
| LC1 | `lc1-admin-store.log`：`running 32 tests`、`32 passed; 0 failed`，含 `export_revocation_is_terminal_for_plain_writes`、`corrupted_identity_material_fails_closed_on_read`、`timestamps_are_advanced_never_erased`；`lc1-imported.log`：`running 10 tests`、`10 passed; 0 failed`，含 `receipt_without_imported_session_is_rejected` 与 `late_callbacks_after_a_full_removal_cannot_rebuild_the_index` | 已核对，与 `verification.md` 的「42 passed」一致。**注意**：该用例修复前也通过（旧断言过宽但恒真），故两份 LC1 日志无法独立证明 RV1-F1 已修；我的结论基于 target 源码静态核对（见 Findings） |
| PV1 | `pv1-fmt.log`：`cargo fmt --all -- --check`，`# exit: 0` | 已核对，一致 |
| PV2 | `pv2-clippy.log`：`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`，`# exit: 0`，无 warning 行 | 已核对，一致 |
| PV3 | `pv3-cargo-test.log`：38 条 `test result: ok`、0 条 `FAILED`，其中 `admin_store` 32（`:274`）、`imported` 10（`:353`）；文件头时间 `2026-09-23T23:19:26+08:00`（round1 为 `23:14:07`） | 已核对；时间戳支持「本轮重跑」。不影响本轮代码判断 |
| PV4 | `pv4-check.log`：`schema fixtures OK`、`command catalog OK: 12`、`error registry OK: 58`、`feature registry OK: 11`、`contract assets OK`、`ACP matrix OK`、`doc links OK: 367 relative links, 2523 section refs`、`crate boundaries OK`、`contract drift OK: §7 的 36 条 DDL…§5 的 15 个 trait / 87 个方法签名`、`check:agentic` 全 PASS、`# exit: 0` | 已核对，一致；`contract drift OK` 同时印证本轮修复未触碰 §5/§7 fenced 块 |
| PV5 | `pv5-verify.log`：`npm run verify`，`# exit: 0`，无 `FAILED`/`error[` | 已核对，一致 |
| cargo-deny / gitleaks | 未在 `reports/` 下找到对应日志，调度者如实记为「本地未执行」 | 属 CI-only，不属本阶段必要材料；不影响本轮判断，但合入/最终验收前需由 CI 补齐 |

**待补资料（主 Agent 负责，非本 target 的代码缺陷；供 3.2/3.3 与最终验收前补齐）**

1. `verification.md` 的 `Review Findings` 仍为「待补（3.2 RV1）」，尚未关联 `reports/rv1-fix.md` 与完整的 RV1（含本 RV2 复核）结论与 agent/隔离信息。
2. `verification.md` 的 revision 记录仍是修复前版本：`tracked_diff_sha256 = 1341369ae898f0627ad9ac751bebb53ddb0e1ceec4483443d7d1c79cabbd80fc`、`+549/−67`；本轮为 `945d1d5…`、`6 文件、+556/−69`（差异仅限 `tests/imported.rs`）。Checks 表的 LC1/PV1–PV5 行同样未注明「本轮为重跑、日志时间 23:19」。
3. 既有用例语义变更尚未在任何记录中留痕：`receipt_without_imported_session_is_rejected` 的夹具（新增 `seed_import`）与断言（`NotFound(_)` → `NotFound(EntityRef::Session(id))`、并新增用例注释），以及 `seed_session` 夹具变更（同一文件 7 条其它用例的起点由「无 Import 行」变为「先 `import.add`」）。R4 已引用新用例（`plan.md` Coverage Index R4 → tasks 2.5/2.6、`reports/lc1-imported.log`），无需改行；但按 plan「按原 WP ID 重新派发、不新增 WP」的口径，建议在 `verification.md` 记录该夹具/断言语义变更与 tasks 2.6 的从属关系（是否补进 tasks 2.6 措辞由主 Agent 决定）。
4. 顶层 `reports/*.log` 本身不含 revision/sha 头（仅 PV 系列含时间戳），建议在 `verification.md` 的对应行写明「与本 revision 绑定的重跑」，以便后续 candidate/main 阶段判定证据是否可复用（plan 的 Dependency Handoffs 允许在无差异时复用）。

**未验证内容（不影响本轮判断）**

1. 我未执行 `cargo fmt/clippy/test`、`npm run check/verify` 或任何 git 命令；上述结论来自静态阅读与日志核对，**不构成**我本人运行通过的声明。
2. 无法自算 sha256，无法以 git 证明工作区与调度者标识的 target 完全等价（仅逐 hunk 内容与 per-file 计数一致）。
3. 未做运行时/动态验证与 E2E（本变更 `mode = not-applicable`；reviewer 也不执行 E2E）。
4. `server`/`app`/`node-link-client` 未落地：`RemoteDeliveryStore::upsert_session` 目前无生产调用方，`put_export` 仅由未落地的本地管理适配器调用，四条守卫当前主要是库级契约，其真实链路影响无法在本轮验证。

**残留风险（供主 Agent 决策）**

- 重导入后用 `(ownerNodeId, exportId)` 无法区分新旧连接的迟到回调：已在 `proposal.md` 的 `non_goals`、`design.md` Risks、`docs` §5.2/§7.4「已知边界」与 §11.2 第 5 条四处登记为「需导入实例标识或连接代际、v1 未实现」，未伪装闭合（本轮修复未改变该状态）。
- 既有库若已含「指纹与公钥互相矛盾」的行，读取立即失败关闭且 `devices()` 整体报错、不提供自动修复（`design.md` Risks 明示为有意行为）。
- RV2-F1 若不订正，`proposal.md` Impact 与实际 diff 的对应关系会带着一处不实陈述进入最终验收（`SKILL.md` 验收需按用户意图逐条核对 Impact 时可见）。