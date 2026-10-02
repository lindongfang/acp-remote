# CR8 独立用例检视报告 · Round 2（test-case / TP2 / 复核轮）

> 持久化说明：本报告由 reviewer 实例 `reviewer-T3`（只读工具集：无写权限、无 shell、无 git）以全文返回、由主 Agent 原样落盘。run `73ae307c-9c64-4779-a5c9-7fa38a2a72cb`。

## Shared Report

- **task_id**: `2.8` · **work_package**: TP2 · **role**: reviewer · **phase**: `test-case`（Round 2 沿用原 Review Type）
- **agent_context**: 实例 `reviewer-T3`，**全新隔离实例**。只读到：角色契约、Round 1 报告、`tp2-tester.md` §10、`tp2-test-design.md`、3 份 a3 日志，以及目标提交 `3484541` 的**文件内容**。**未参与**本轮修正（修正者为 `tester-A3`），也**不是**用例作者（作者为 `tester-A` / `tester-A2` / `tester-A3`）。**未运行**任何 `cargo` / `npm` / `git`。
- **target_revision**: `3484541`（base `f44301e`）
- **scope**: 逐条闭环 Round 1 的 8 条 finding（F1–F6、S1、S2），并独立验证修正未引入新问题。不重判 Round 1 已 PASS 的 A/B/E/F 结论。
- **changes**: 无（只读） · **checks**: 未执行任何命令
- **issues**: 0 CRITICAL / 0 MAJOR / 2 MINOR / 1 SUGGESTION · **result**: **PASS**

## Review Context

| 项 | 实际取值 |
| --- | --- |
| Review ID / Round | **CR8 / 2**（Round 1 的 PASS 绑定 `f44301e`，**不沿用**） |
| Repository | `D:/Project/acp-remote-wt/session-resume-tp1`（分支 `agentic/session-resume-tp1`） |
| Base / Target | `f44301e` → `3484541`（reviewer-launch HEAD = `3484541977a1`） |
| 读取的规则与需求 | `AGENTS.md`（§9/§10）、`roles/reviewer.md`、`roles/_shared/role-report.md`、`cr8-review.md`（Round 1） |
| 读取的目标内容 | `crates/storage-sqlite/tests/resume_columns.rs`、`crates/app/tests/session_resume_e2e.rs`、`crates/app/tests/support/owner.rs`、`crates/server/src/node_link/command.rs`、`crates/server/src/node_link/command/tests.rs`、`crates/acp-wire/src/lib.rs` |
| 读取的报告/日志 | `tp2-test-design.md`、`tp2-tester.md` §10、`tp2-tester-a3-fmt.log`、`tp2-tester-a3-test.log`、`tp2-tester-a3-s1-probe.log`、`tp2-tester-a2-metadata-locked.log` |

### 限制（如实声明，与 Round 1 同类）

1. **diff 工具不覆盖已提交范围。** `watchdog_diff` 返回 `No working-tree changes against reviewer-launch HEAD 3484541977a1. Committed changes are not included.`；亦**无 shell / 无 git**。因此本轮**无法产出 `git diff f44301e..3484541`**，也**无法**独立复核 `4146611` 的存在性、tree 相同性或 dangling 状态。
   - 凡属「零改动 / 未变」的结论一律标注 **[内容级已核 / diff 级受限]**。
   - 需要 diff 级判定的项已列入「待补证据」，请 merger 或主 Agent 在合入前用 `git diff --name-only f44301e..3484541` 与 `git show 3484541 --stat` 机械复核。
2. 不重复 Project Verify；日志只作**辅助**证据。
3. 本轮**未**复核 `crates/acp-protocol/tests/session_resume.rs`、`crates/agent-host/tests/resume.rs`、`crates/node-link-protocol/tests/session_resume_command.rs` 的内容（修正者声明未触碰这三个文件）——这一「未触碰」声明**只能采信为记录，无法 diff 级核实**。

## 1. 八条 finding 逐条闭环判定

### CR8-F1（MINOR）注释与代码矛盾 —— ✅ **已闭环**（内容级；「零断言改动」为 diff 级受限）

| 核对点 | 位置 | 目标版本内容 | 判定 |
| --- | --- | --- | --- |
| rustdoc ① | `resume_columns.rs:144–148` | 「以**不带引号**的 `CREATE TABLE owned_session (` 开头（即追加路径）…**注意**：`IF NOT EXISTS` **不能**用作判别式…**两条路径都不含**，按它断言会恒真」 | ✅ 与断言一致 |
| 内联注释 ① | `:179–186` | 「追加路径 ⇒ 无引号；重建路径 ⇒ 有引号」「`IF NOT EXISTS` 不能用作判别式」 | ✅ |
| 断言 ① | `:188` / `:192` | `ddl_after.starts_with("CREATE TABLE owned_session (")` ／ `!ddl_after.starts_with("CREATE TABLE \"owned_session\" (")` | ✅ 判别式**仍是双向的、非恒真** |
| rustdoc ④ | `:153–155` | 「`owned_audit`，其文本以**带双引号**的 `CREATE TABLE "owned_audit" (` 开头（RENAME 的签名）」 | ✅ |
| 断言 ④ | `:264` / `:268` | `rebuilt.starts_with("CREATE TABLE \"owned_audit\" (")` ／ `!rebuilt.starts_with("CREATE TABLE owned_audit (")` | ✅ 反证仍成立 |
| 残留矛盾表述 | 全文件 grep `IF NOT EXISTS` | 仅剩 `:146`、`:147`、`:185`、`:186`，**全部是「不能作判别式」的否定式说明**；无任何正向「DDL 含 `IF NOT EXISTS`」表述 | ✅ 无残留 |

- **注释现在是否与代码断言一致？** 是。注释的判别式（表名是否带引号）与两条 `assert!` 的字面量**逐字对应**。
- **断言是否一个字符都没被动过？** **[内容级已核 / diff 级受限]**。可给的内容级旁证：断言表达式与 Round 1 报告 §E-2 的**逐字引用**完全一致，且**双向断言成对存在**（若被改成恒真式，负向那条会与正向那条自相矛盾）。**无法给出 diff 级证明**。
- **CR8 警告的失败模式（「按注释修断言 → 判别式改回恒真」）未发生**：注释被改成跟随断言，而不是断言被改成跟随旧注释。

### CR8-F2（MINOR）提交号错绑 —— ✅ **已闭环（订正与痕迹）**；⚠️ 「amend 前 SHA」的说法**无法独立验证**

- 订正已落到 `tp2-test-design.md:9–13`（头部订正记录）、`:140`（§4 前言补记）、§6 标题。
- **订正痕迹保留**：全仓 `4146611` 的剩余出现只有三类——Round 1 报告自身（应保留）、设计报告的**订正记录段**、`:140` 括注。三处都是「说明它已被订正」，**没有任何一处仍把交付绑定到 `4146611`**。✅
- **独立可 corroborate 的部分**：权威值 `f44301e` 有第二条独立证据——`tp2-tester-a2-metadata-locked.log:2` 记 `# revision: f44301e` / `# exit: 0`。✅
- **无法验证的部分**：`4146611` 是否真实存在、是否 dangling、是否与 `f44301e` tree 相同——**无 git，不能实测**。已列入待补证据，**不影响本轮 PASS**（方向保守：把可疑 SHA 收敛到有日志支撑的 SHA）。

### CR8-F3（MINOR）证据指针 —— ✅ **已闭环，且逐条独立核实了被引用的行号**

| 行 | 报告现在写的 | 独立核实 |
| --- | --- | --- |
| R30 | `tests.rs::an_unauthorized_command_is_rejected_audited_and_has_no_side_effect`（`:1509`） | ✅ grep 命中 |
| R30 | `::an_unauthorized_session_resume_is_rejected_before_any_local_read`（`:2888`） | ✅ grep 命中 |
| R33 | `::the_command_rate_limit_replies_rate_limited_and_then_closes`（`:2303`，`[R71]` 120/分钟） | ✅ grep 命中，其上 `:2302` 确为该 doc |
| R33 | `command.rs:248` 的 `if !self.admit_rate(handle)` 在 `match submit.command`（`:251`）**之前**、对全部命令生效 | ✅ grep 命中；另核 `:253` 处 `match` 分支含 `CommandName::SessionResume => self.on_session_resume(...)` |

Round 1 指出的错误指针 `:992` 已从报告中移除。✅

### CR8-F4（MINOR）计数不自洽 —— ✅ **已闭环**

- `:74`：「新增 **33** 条本机可执行用例，另有 **2** 条 `#[cfg(unix)]` 变体零本机证据…**不得计入 PASS**」。
- `:117–121`：「**33** 条**本机可执行**：acp-protocol 7、storage-sqlite 6、agent-host 7、node-link-protocol 5、app **8**；**另有 2** 条 `#[cfg(unix)]`…**不在此计数内**…状态为 **PENDING**」。
- 算术自洽：7+6+7+5+8 = **33** ✅。旧的「32 / app 7」在 §2 已无残留；§5 仍写 `app/session_resume_e2e 7`，但该节标题明确是第 1 轮的历史记录，**不构成内部矛盾**。
- 与机器证据一致：`tp2-tester-a3-test.log:153` `running 8 tests`、`:357` `running 6 tests` ✅。

### CR8-F5（MINOR）覆盖混档 + 注释把「能力不支持」说成「NULL」 —— ✅ **已闭环**

- **报告侧（R25 拆三档）**：`tp2-test-design.md:96` 已改为 ① 平台无关变体（本机实跑 PASS）／② 2 条 `#[cfg(unix)]`（**不得记 PASS**）／③ 上游同族变体（已覆盖并本机 PASS），结果列写「①③ PASS；② PENDING」。✅
- **代码侧（第 ③ 段注释）**：文件级 rustdoc `:991–996` 与段内注释 `:1115–1120` 均已改为「**本用例不造两列为 `NULL` 的会话**」并把 NULL 侧证据指向 `server/.../tests.rs::session_resume_without_persisted_recovery_data_fails_as_unsupported` 与 `resume_columns.rs`。
- **被指向的用例真实存在**：`crates/server/src/node_link/command/tests.rs:2936` ✅（不是悬空指针）。
- **该用例的断言未被改动** **[内容级已核 / diff 级受限]**：第 ③ 段执行序列与断言（`set_behavior(BackendUnsupported)` → `submit` → `accepted` → `terminal` → `code == nodelink.command.unsupported` → `assert_ne!`）与 Round 1 记录一致；④ 段的 `assert_same_recovery` 仍在。

### CR8-F6 —— ⚠️ **仍为 PENDING（确认保持）**，见第 3 节

### CR8-S1 —— ✅ **已按实测分支处置**；三点裁定见第 2 节

### CR8-S2 —— ✅ **已闭环**

`crates/app/tests/support/owner.rs:12` 已改为「**三处刻意的测试替身/测试介入**」，`:15–17` 登记 ③ `overwrite_persisted_workspace_cwd`（TP2 第 2 轮新增）经 `sqlx` **原始 SQL** 直接改写 `owned_session.workspace_cwd`、**绕过全部端口与本地管理方法**，并写明它是三者中**介入最深**的一处及理由。✅

## 2. CR8-S1 三点裁定

**① 实测值 `{}` 是否可信？—— 可信（高置信），并给出与探针互相独立的第二条证据。**

静态路径完整走了一遍，结论与探针输出一致：

1. `command.rs:995–1015` `on_session_resume`：① `session_target`（`:1218–1248`）**只解析 `sessionRef`**——只校验 `owner_node_id` 指向本节点 + 复合引用格式合法，**不查会话是否存在**（注释 `:1216–1217` 明写「不区分『不存在』与『不属于本机』」）→ ② `authorize_node(..., "grant.remote-work", ...)` 失败即 `deny`。
2. `deny`（`:1354–1366`）：`CommandFault::NotGranted(None) => RawObject::empty()`；`NotGranted(Some(parameter))` 才走 `parameter_details`。
3. `acpr-wire/src/lib.rs:493–495`：`RawObject::empty()` 的字面量就是 `"{}"`。
4. `acpr-wire/src/lib.rs:861–868`：`PublicError.details: RawObject`，**必填键、无 `skip_serializing_if`**；`command.rs:1600–1601` `send_rejected` 直接 `error.details = details` 后序列化。

⇒ 存在/不存在两条路径**必然**得到 `details == {}`，与 `tp2-tester-a3-s1-probe.log`（`denied.details={} absent.details={} …is_object=true`）一致。**Round 1 的前提「两侧同为 `null`」字面确实不成立。**

**② 「不加固、只补注释」是否正确？—— 正确。**

派发的分支规则以实测值为准；实测为 `{}` 后，「先 `is_object()` 再比相等」属于**加固**而非**修正**：它不会把恒真断言变成有判别力的断言（两侧仍来自同一条硬编码分支），只在 `details` 键**整体缺失**时多抓一种回归。在一条本机已 PASS、且本轮定位为「记录层修正」的用例里不引入新断言，是最小风险的选择；把实测值、来源与角色定位写进注释（`:701–705`）。**接受。**

**③ 「回归护栏、不是当前判别器」的定位是否诚实？—— 诚实，且做了比 CR8 更严的区分。**

注释 `:703–704` 明确写「这是**结构性防泄露守卫**…**而不是当前的判别点**；真正的判别力在上一条 `code` 断言与「目录已被删除」这个前提上」。R31 的可证伪判别力实际由三条承担（`:644–648` 计数器自检；`:664–679` 目标会话目录**已被删除** ⇒ 若把 `authorize` 挪到 `load_recovery` 之后会得到 `internal.unavailable` 而非 `not_granted`；`:668–672` 与 `:710–715` 两次计数不变）。**唯一可补的一句（不构成 finding）**：`code` 相等断言**自身**在当前实现下也近乎恒真（`session_target` 合并了「不存在/不属于本机」两种情形），严格说判别力来自「目录已被删除」这一前提 + 读计数；注释已并列提到两者，读者不至于被误导。

## 3. CR8-F6 仍是 PENDING —— ✅ 确认

| 确认项 | 证据 | 判定 |
| --- | --- | --- |
| 两条用例仍在 | `session_resume_e2e.rs:1406` `#[cfg(unix)] #[test] a_persisted_directory_replaced_by_a_symlink_…`、`:1512` `#[cfg(unix)] #[test] an_inaccessible_persisted_directory_…` | ✅ 未删除 |
| 未被 `#[ignore]` 包裹 | 全文件 grep `#\[ignore\]` / `#\[should_panic\]` → **No matches found** | ✅ |
| 未改成 `#[cfg(not(unix))]` | 全文件 `cfg` 命中仅 5 处，全部是 `#[cfg(unix)]` | ✅ |
| 未被掏空 | 符号链接用例体断言完整：前提自证 3 条 + `status == failed` + `code == nodelink.internal.unavailable` + `resume_probe.calls()` 不变 + `assert_same_recovery` | ✅ 仍是 Round 1 审过的那份 |
| 未出现「本机已执行」措辞 | `:1235–1236`、`:1404–1405`、`:1510–1511` 三处仍写「本机（Windows）**从未执行**…**本地未执行不等于通过**」 | ✅ |
| 机器证据仍成立 | `tp2-tester-a3-test.log:153` `running 8 tests`，8 个用例名**不含**两条 unix 变体；全部 `ok` | ✅ |
| 报告侧如实标注 | `tp2-test-design.md §7`「CR8-F6 保持 PENDING（重申）」；`tp2-tester.md §10.3`；handoff 行 `evidence_status: PENDING` | ✅ |

**33 条本机可执行用例的断言是否因本轮文字修正而改变？** **[内容级已核 / diff 级受限]** —— `resume_columns.rs` 断言 ①/②/③/④ 完整、6 条用例名与计数与第 2 轮一致；`session_resume_e2e.rs` 8 条用例名与第 2 轮一致，R31（`:640–716`）与 R24/R25（`:997–1158`）的断言结构、口径、阈值与 Round 1 记录一致（`assert_eq!` 而非放宽的 `matches!`；`code` 精确值；计数相等；`assert_same_recovery` 仍在）。**无法给 diff 级证明。**

## 4. Findings

| ID | Severity | Location | Impact | Recommendation |
| --- | --- | --- | --- | --- |
| **CR8-F7** | MINOR | `tp2-tester-a3-fmt.log:1`、`tp2-tester-a3-test.log:1`（`# revision: f44301e…`）vs `tp2-tester.md §10.8` 把 A3-C1/A3-C2 记为 `target_revision: "3484541"` / `evidence_status: NEW` | 两份日志头部记录的是 **base `f44301e`**，而交接行把同一份执行证据绑定到 **`3484541`**。最合理解释是「测试在提交前的脏工作区上跑，HEAD 仍是 `f44301e`」，但日志**无法自证**被测内容就是 `3484541` 的内容。**不改变本轮 PASS**：本轮仅注释/记录，断言已由内容核对确认与 Round 1 审过的那份一致，行为风险为零 | 在日志头补一行（如 `# worktree: f44301e + 3 个 tests 文件的注释修正（未提交）`），或在 §10.6/§10.8 注明「执行发生在提交前的工作区，内容与 `3484541` 的这三个文件一致」 |
| **CR8-F8** | MINOR | `tp2-tester.md §10.8` 的 `CR8-F6-UNIX-VARIANTS` 行：`result: NOT_EXECUTED` | `roles/_shared/role-report.md` 规定 `result` 只取 `PASS`/`FAIL`/`BLOCKED`/`NOT_APPLICABLE`，且「`PENDING` 的 `result` 必须为 `BLOCKED`」。下游按契约消费可能解析失败或被误读。**方向是保守的**（没有把未执行写成 PASS），不构成不诚实 | 改为 `result: BLOCKED`（`evidence_status: PENDING` 保持不变） |
| **CR8-F9** | SUGGESTION | `session_resume_e2e.rs:706–709` | CR8-S1 的原始加固建议在实测值下已不是「修正」而是「加固」：两侧都来自 `NotGranted(None) → RawObject::empty()`，加 `is_object()` 不会产生判别力，只多抓「`details` 键整体缺失」一种回归 | 若主 Agent 认为值得：在 `:706` 的 `assert_eq!` 之前加 `assert!(denied["body"]["error"]["details"].is_object(), "publicError.details 是必填对象（common.schema.json）")`，可同时对两侧各断言一次。**不作本轮 PASS 的必要条件** |

**未发现 CRITICAL / MAJOR。** Round 1 的 8 条 finding 全部闭环（F6 为「按原判保持 PENDING」，符合 Round 1 的处置建议）。

## Assessment

- **本轮结论：PASS**（0×CRITICAL / 0×MAJOR）
- **A/B**：不重判。修正**未触碰**任何用例的 ID、入口或需求归属；37 行映射结构与 Round 1 审过的一致，本轮只改了 R25/R30/R33 三行的**证据指针与分档**。R31 与 R24/R25 的断言逐条仍在、口径未放宽；`resume_columns` 的判别式仍是双向（非恒真）断言对。
- **C**：本轮声明零改动；`Cargo.toml` / `Cargo.lock` 未被触碰（**diff 级受限**）。
- **D/E**：`#[cfg(unix)]` 两族的标注如实、结构未动、断言未掏空；`recovery_reads` 单 `Arc` 的自查结论未被触及。
- **F**：未引入 `#[ignore]`、未删断言、未弱化阈值；未新增/删除用例（8 + 6 的用例名与计数与第 2 轮逐项一致）。

### 待补证据

| 待补项 | 影响本轮判断 | 应在哪个门禁前补 |
| --- | --- | --- |
| `git diff f44301e..3484541 --stat`（证明只有 3 个 `crates/**/tests/**` 文件、零断言改动、零清单改动） | 否（内容级已核） | merger 候选 review / 合入前 |
| `git cat-file -t 4146611`、`git rev-parse 4146611^{tree} f44301e^{tree}` | 否 | 记录订正闭环 |
| 两条 `#[cfg(unix)]` 在 `ubuntu-latest` 的编译与执行（CR8-F6） | 否 | **Linux CI `checks` job**；失败则开新问题编号回 TP2，不改判本轮结论 |
| PV1 阶段 2、PV2（`npm run check`）、`cargo-deny`、`gitleaks` | 否 | 候选 / 主分支门禁（交 merger） |
| validator 覆盖充分性判定 | 否 | `final` 阶段 |

### handoff_index

- `task_id: "2.8"` · `work_package: TP2` · `role: reviewer` · `phase: test-case` · `round: 2` · `stage: work-package` · `target_revision: "3484541"` · `evidence_type: REVIEW` · `evidence_id: CR8` · `report_path: "openspec/changes/session-resume/reports/cr8-review-round2.md"` · `result: PASS` · `evidence_status: NEW`
- `applicability_basis`: 静态检视 `3484541` 的三个 tests 文件内容与 report 侧；逐条闭环 Round 1 的 F1–F6/S1/S2；独立核实 F3 引用的 4 个用例行号（`:1509`/`:2303`/`:2888`/`:2936`）与 `command.rs:248`/`:251`；对 S1 的 `details` 取值做静态路径独立复核并与 a3 探针日志互证；确认两条 `#[cfg(unix)]` 用例仍为 PENDING 且未被 ignore/删除/改写。未执行任何 cargo/npm/git；diff 工具不覆盖已提交范围。Round 1 的 PASS 绑定 `f44301e`，本轮不沿用。
