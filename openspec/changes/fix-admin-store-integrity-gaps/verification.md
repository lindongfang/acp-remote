# fix-admin-store-integrity-gaps 执行记录

## Target

- 变更名：`fix-admin-store-integrity-gaps`（schema `agentic`）
- 仓库：`D:\Project\acp-remote`（本地 `refs/heads/main`）
- 规划基线（1.1 核实，2026-09-23）：`HEAD` = `main` = `1ef6640431079f9ca22b409c42071de2d9a79fa2`，工作区唯一改动为未跟踪的本变更目录（`git status --porcelain` 仅 `?? openspec/changes/fix-admin-store-integrity-gaps/`）
- 环境：Node `v24.19.0`（≥ 22.12）；`rustc 1.98.1 (48a229cea 2026-09-01)`，与 `rust-toolchain.toml` 的 `channel = "1.98.1"` 一致
- 版本确认负责人：主 Agent；核实方式：`git rev-parse`、`git status --porcelain`、`rustc --version`、`node --version`（可选机械核实交给 environment/recon）
- 目标可核实且未变化；文档提交与代码提交在同一变更工作区内，最终合入提交在 Merge History 记录
- 编码起点与边界（1.2）：起点提交 = `1ef6640`；写入范围 = WP1 `crates/storage-sqlite/src/admin/trust.rs`、`src/admin/export.rs`、`tests/admin_store.rs`；WP2 `src/session_store.rs`、`tests/imported.rs`；WP3 `docs/CORE_PORTS_AND_STORAGE.md`（其余写入面 = 本变更目录）。硬边界：不新增 `ConflictKind`/`UnavailableKind`/错误码，不改 DDL、端口签名、值对象与 wire，不改 `schemas/`/`fixtures/`/`compatibility/`，不改 `LOCAL_ADMIN_PROTOCOL.md`
- 实现期限制（1.3）：本变更为串行单执行者，不涉及共享运行资源；不使用 `fixtures/`
- 版本标识（工作区候选，两轮）：**V1** = `tracked_diff_sha256 = 1341369ae898f0627ad9ac751bebb53ddb0e1ceec4483443d7d1c79cabbd80fc`（6 文件 +549/−67，证据归档在 `reports/round1/`）；**V2**（RV1-F1/F2 修复后 = 当前候选）= `tracked_diff_sha256 = 945d1d509b428a8b84e8d2168d980600098a14d57086d8bd2e4c6957b21e6170`（6 文件 +556/−69，patch sha256 `da7cbcdba0ae4c7cda0e9bc0c36e6645e85bf389ae02c20f84326f06337576bd`）。V1→V2 的差异仅限 `crates/storage-sqlite/tests/imported.rs`（+7/−2），其余 5 个文件逐 hunk 相同

## Checks

| Check ID / Stage / Work Package | Revision / Base | Scope | Executor | Command / Steps | Environment | Result / Exit Code | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| RECON1 / setup / 全变更 | `1ef6640` | 仓库、目标引用、工具链、定向测试可运行性 | 主 Agent（1.1，inline；不新增共享资源） | `git rev-parse HEAD`、`git rev-parse refs/heads/main`、`git status --porcelain`、`node --version`、`rustc --version`、`cargo test --locked -p storage-sqlite --test admin_store --test imported` | Node v24.19.0；rustc 1.98.1；Windows x64 | PASS（exit 0；admin_store 30 passed、imported 9 passed，基线全绿） | 本文件上方 Target 段；基线输出见 `reports/baseline-targeted-tests.log` |
| WP1、WP2 handoff / branch / WP1、WP2 | base `1ef6640` + V2 工作区改动（`tracked_diff_sha256 = 945d1d50…`） | 四条守卫的实现与回归用例 | 主 Agent（coder 角色，inline；完整读取 `openspec/schemas/agentic/roles/coder.md`） | 写入范围：`crates/storage-sqlite/src/admin/trust.rs`、`src/admin/export.rs`、`src/session_store.rs`、`tests/admin_store.rs`、`tests/imported.rs`（6 个文件、+556/−69，不含 `schemas/`/`fixtures/`/`compatibility/`） | 同上 | PASS（局部：`cargo build -p storage-sqlite` 通过；定向测试全绿） | `git diff` 摘要见 `reports/branch-diff-stat.txt`；本文件上方 Target 的 1.2 边界段 |
| LC1 / workpackages、candidate / WP1、WP2 | V2（重跑，顶层日志 23:19 系列；V1 首轮日志已归档 `reports/round1/`） | 12 个规格场景的定向断言 | 主 Agent | `cargo test --locked -p storage-sqlite --test admin_store`（32 passed）与 `--test imported`（10 passed） | 同上 | PASS（exit 0；两文件 42 passed、0 failed；含新增 `export_revocation_is_terminal_for_plain_writes`、`corrupted_identity_material_fails_closed_on_read`、`late_callbacks_after_a_full_removal_cannot_rebuild_the_index`、扩充后的 `timestamps_are_advanced_never_erased`，以及按 RV1-F1 收窄断言的 `receipt_without_imported_session_is_rejected`） | `reports/lc1-admin-store.log`、`reports/lc1-imported.log`（V1 同轮日志：`reports/round1/`） |
| PV1 / workpackages、candidate / WP1–WP3 | V2（重跑） | 格式门禁 | 主 Agent | `cargo fmt --all -- --check`（仓库根） | 固定工具链 1.98.1 | PASS（exit 0；V1 首跑失败于 `tests/admin_store.rs` 的 import 分组换行，已 `cargo fmt --all` 修正后复跑） | `reports/pv1-fmt.log`（V1：`reports/round1/pv1-fmt.log`） |
| PV2 / workpackages、candidate / WP1–WP3 | V2（重跑） | lint 门禁 | 主 Agent | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | 同上 | PASS（exit 0，无警告） | `reports/pv2-clippy.log` |
| PV3 / workpackages、candidate / WP1–WP3 | V2（重跑） | 全量测试 | 主 Agent | `cargo test --locked --workspace --all-features` | 同上 | PASS（exit 0；38 个 `test result: ok`，0 个 `FAILED`） | `reports/pv3-cargo-test.log` |
| PV4 / workpackages、candidate / WP3 | V2（重跑） | 合同门禁（含 `check:contract-drift`、`check:doc-links`、`check:agentic`） | 主 Agent | `npm run check` | Node v24.19.0；`npm ci` 已完成 | PASS（exit 0；`contract drift OK: §7 的 36 条 DDL 与 migrate.rs 逐条一致；§5 的 15 个 trait / 87 个方法签名与 ports.rs 一致`；`doc links OK`；`openspec validate` 6 passed） | `reports/pv4-check.log` |
| PV5 / workpackages、candidate / DU1 | V2（重跑） | 统一入口 | 主 Agent | `npm run verify` | 同上 + 固定工具链 | PASS（exit 0；`check` 全部子门禁 + `check:rust`（fmt/clippy/test）全绿；无 `FAILED`/`error[`） | `reports/pv5-verify.log` |

`cargo-deny` 与 `gitleaks` 只在 CI 运行，本地没有等价物：本轮记为「未在本地执行」，不得声称通过。

## Check Plan Changes

检查清单/命令与范围未变（LC1 + PV1–PV5），但**测试夹具与断言在 V1→V2 之间有一处语义变更**（因 RV1-F1，按原 WP2 重新派发、未新增 WP）：

- 原值：`tests/imported.rs::receipt_without_imported_session_is_rejected` 的夹具未建档（无 `add_import`），断言为 `matches!(error, PortError::NotFound(_))`。
- 新值：夹具先 `seed_import(&store)`（归属行在、会话行不在），断言收窄为 `NotFound(EntityRef::Session(session_id))`（并保留「`imported_delivery_index` 计数为 0」）。同一文件的 `seed_session` 夹具也改为先 `seed_import`（该文件 7 条既有用例的起点由「无 Import 行」变为「先 `import.add`」），`drop_import_keeps_audit_rows` 的两段 raw `imported_import`/`imported_import_export` INSERT 随之删除（改由 `add_import` 承担）。
- 理由：新守卫 `ensure_import_owns` 会在 `commit_receipt` 中先返回 `NotFound(Export)`，旧夹具因此不再触达「会话行不存在」分支，使 §7.4 的该条 `[决定]` 失去定向回归（RV1-F1 的 MINOR 发现）。
- 风险覆盖：两条分支现在各有定向用例（「归属行在、会话行不在」→ `NotFound(Session)`；「无归属行」（从未导入与完整移除后）→ `NotFound(Export)`），共由 [LC1]/[PV3] 覆盖。
- 受影响任务与复核：tasks 2.6（原 WP2）；修复后由新独立 reviewer 复核 → `reports/rv2-fix-recheck.md`（Review ID RV2，PASS）。
- 另有两处**变更内文档事实精度**订正（RV1-F2 / RV2-F1，均无代码影响）：`design.md` Context 与 D3、`tasks.md` 2.2、`plan.md` Contract Changes、`proposal.md` Impact 中「`trust.rs` 模块头注释」的措辞改为实际位置（`load_peer_key` 的文档注释）并注明模块头无需改动。
- 证据失效与复用：V1 的 LC1/PV 证据在 V2 上**已失效并重跑**（日志见顶层 `reports/`，V1 归档 `reports/round1/`）；RV1 对 5 个未变文件的结论按 RV2 的逐 hunk 比对继续适用，RV1-F1/F2 相关结论已由 RV2 复核取代。

## Dependency Handoffs

不适用：本变更为单仓库、单 crate 的守卫补丁，没有跨变更上游代码交付物；WP1/WP2/WP3 各自只依赖本变更已冻结的 specs 与 design（见 plan.md 的 Dependency Handoffs 表），写入文件互不重叠。

## Runtime Resources

- 本变更不涉及数据库服务、容器、端口、账号或外部服务等共享运行资源。
- `target/` 为唯一共享构建目录；本轮实现为串行单执行者，未与其他执行者并发写 `target/`。
- 测试用 SQLite 临时目录由 `support::temp_dir`（含进程 id）隔离，不读写 `fixtures/`；无资源污染。
- 环境侦察与检查均无独立运行资源需要释放。

## Review Findings

| ID | Revision | Reviewer | Location | Severity / Impact | Resolution | Recheck Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| RV1 / RV1-F1 | V1（`1341369a…`，+549/−67） | 独立 reviewer 子 Agent（builtin `reviewer`，`context=fresh`，未继承实现对话；Agent 会话 `6a839420-f6e1-4069-9ccd-c6abd9903221`）；报告 `reports/rv1-fix.md` | `tests/imported.rs`（`receipt_without_imported_session_is_rejected`）、`design.md` D5 | MINOR（P2，非阻断）：夹具未建档使「会话行不存在 → `NotFound(Session)`」分支失去定向覆盖，断言过宽，注释与 design D5 与事实不符 | 已在 V2 修复（夹具先 `seed_import`、断言收窄为 `NotFound(EntityRef::Session(session_id))`、同步 design/注释措辞） | RV2 复核「已解决」：`reports/rv2-fix-recheck.md`（RV1-F1 行，含可证伪性分析：删 `seed_import` 或改错误变体都会失败） |
| RV1 / RV1-F2 | V1 | 同上 | `design.md` Context、`tasks.md` 2.2 | SUGGESTION（P2）：把「模块头注释」写成一处实际不存在的改动项 | 已在 V2 订正为 `load_peer_key` 的文档注释并注明「模块头无需改动」 | RV2 复核「已解决」，并在 `plan.md`/`proposal.md` 发现同类残留（RV2-F1） |
| RV2 / RV2-F1 | V1→V2 | 独立 reviewer 子 Agent（builtin `reviewer`，`context=fresh`，未继承实现或上一轮检视；Agent 会话 `109d71d8-0434-444c-90be-5b2b939b6761`）；报告 `reports/rv2-fix-recheck.md` | `plan.md` Contract Changes、`proposal.md` Impact | SUGGESTION（P2，非阻断）：同类措辞残留使 proposal/plan 与 design/tasks 口径不一致（无代码/行为/门禁影响） | 已由主 Agent 在同一轮订正（`plan.md`/`proposal.md` 改为实际位置并注明模块头无需改动）；属变更内文档订正，产品代码修订（V2）未变，不触发新的产品代码复核 | 本文件 Check Plan Changes 记录；V2 产品修订的 RV2 PASS 继续适用（tracked diff 未变：`945d1d50…`） |

RV2 的总体结论：**PASS**（对应 V2；无未解决 CRITICAL/MAJOR）。RV1 的总体结论：**PASS**（对应 V1；两条非阻断项）。两轮共同确认：四条守卫均在写事务内、失败关闭先于写入；错误变体与判定顺序符合 design D1–D4 与用户复核意见；无越界改动（DDL/端口签名/值对象/wire/`schemas/`/`fixtures/`/`compatibility/`/`LOCAL_ADMIN_PROTOCOL.md` 均未触碰）；文档同步落在 §5/§7 的 fenced 块之外。

reviewer 待补且已由主 Agent 补齐的项（RV2 的「待补资料」）：verification 的 Review Findings 关联（本节）、Revision 更新（Target 段）、既有用例语义变更留痕（Check Plan Changes）。RV2 指出的「顶层日志不含 sha 头」的口径已由 Target 段的版本标识与本节的重跑说明补齐。

## Merge History

待补（4.1、5.1–5.8）。规划授权：用户 2026-09-23 本会话选 (b)——授权本地合入 `refs/heads/main`，不含推送、创建 PR、发布与归档（见 plan.md 的 Merge Strategy）。

## Test Design and Authoring

不适用：Main E2E mode 为 `not-applicable`，按 plan.md 不生成测试工作包（TP）；各 WP 自带行为测试，由 LC1/PV3 留证。

## Candidate E2E

不适用（mode = `not-applicable`）。

## Main E2E

- 项目开关（`npx --quiet --no-install openspec-agentic e2e --json`，2026-09-23 读取）：`enabled: true`、`command: ""`、`maxAttempts: 3`
- plan.md 的 mode：`not-applicable`，带 `downgrade_approval`（用户本会话原话「A同意，B a」/「A同意，B  b」）
- reason：本变更只修 `crates/storage-sqlite` 的写路径守卫与读取期校验；server/app/daemon/CLI/前端尚未实现，无可端到端运行的产品入口
- basis：`openspec/config.yaml` 的 context 已记录「当前阶段没有可端到端运行的产品路径，Main E2E 按变更记 not-applicable，需用户逐变更批准」；`x-agentic.e2e.command` 为空
- alternative_checks：LC1（定向规格断言）+ PV3（全量回归）+ PV4/PV5（合同漂移与文档引用门禁）；逐项证据见 `## Checks`
- 结论：E2E 记 `NOT_APPLICABLE`；替代验证按 `## Checks` 留证（待执行）

## Failures and Retests

无（截至实现开始）。后续失败、重试与复测按来源问题 ID 追加，不覆盖历史。

## Final Assessment

待补（7.1 最终验收）。
