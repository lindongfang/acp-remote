# DU1 — task 6.3 独立 Project Verify 报告

- **task_id**：`6.3`（独立检查执行者；`plan.md` 未为该任务指派命名角色 —— `tester` 仅做 E2E，`validator` 需 plan 显式启用，故本执行者是不继承实现对话的独立执行者）
- **role**：独立检查者（独立 Project Verify 执行者）
- **phase**：交付单元 DU1 候选验证阶段（tasks `6.3`）
- **agent_context**：隔离上下文；未参与 `013f2b9`…`aed9fb5` 任何实现或修复对话；输入仅取自本任务书给出的命令清单、仓库内 `openspec/changes/admin-state-persistence-v2/` 的权威变更目录与 `AGENTS.md` §8
- **target_revision**：`aed9fb5fc7c244883a12e08fe4c9c5b6540f6822`（分支 `feat/admin-state-persistence-v2`）；基线/目标主分支 `refs/heads/main` = `37a398e9dbafa368bdb15853e1c1d9b40f19b28d`。
  复核 `git rev-parse HEAD` = `aed9fb5fc7c244883a12e08fe4c9c5b6540f6822`，与固定候选一致；本执行者未做任何 `checkout`/`reset`/`commit`。
  工作树相对候选的差异：仅 ` M openspec/changes/admin-state-persistence-v2/tasks.md`（**主 Agent 的记录回填**，非本执行者改动；任务书禁改该文件）。验证是在「候选 + 该未提交记录改动」的工作树上执行的；该改动是 `tasks.md` 的勾选与执行记录文字，不触及产品代码、测试、夹具与合同文档，对下述四条命令的结果无影响（`npm run check` 的十道门禁与该文件无关，`check:agentic` 读的是 `verification.md` 非 `tasks.md`）。
- **scope**：只在固定候选上**运行** `npm run verify` 口径的四条命令并留证；不改产品代码/测试/夹具/合同/变更记录；不执行 E2E（本变更为 `not-applicable`）；不 `git add`/`commit`/`push`/`merge`。
- **changes**：除两份证据文件与隔离构建目录外，**未修改、未创建任何文件**（详见 `resource_cleanup`）。本执行者是纯验证者，无代码改动。

## 复核口径

`package.json` 的 `verify` = `npm run check && npm run check:rust`，其中 `check:rust` = `cargo fmt --all -- --check && cargo clippy --locked --workspace --all-targets --all-features -- -D warnings && cargo test --locked --workspace --all-features`。即与 `AGENTS.md` §8 / 任务书的四条命令逐条等价。按任务书要求，四条命令**分四次独立执行**、逐条记录退出码（未用 `&&` 串成一条，以便失败定位）。

每条 `cargo` 命令前设 `CARGO_TARGET_DIR=D:\Project\acp-remote\target\du1-pv1`（本执行者专属，未使用默认 `target/`）；`npm run check` 不涉及 cargo，未设该变量。

## checks（逐命令）

| # | 命令 | 退出码 | 耗时 | 工具版本 | 关键输出 |
|---|---|---|---|---|---|
| 1 | `npm run check` | **0** | 5314 ms | node v24.19.0 / npm 12.0.2 | 十道门禁全 OK：schema fixtures 117 valid / 23 invalid、command catalog 12、error registry 58 / 2 protocols、feature registry 11 / 2、contract assets 17 schemas / 155 fixture files / 12 transcript vectors / 20 negative vectors / 2 SAS、acp matrix 25 methods / 71 rows、doc links 367 links / 2164 section refs / 101 md、crate boundaries 6 crates、`contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；§5 的 15 个 trait / 87 个方法签名与 crates\core\src\ports.rs 一致`；agentic gate `Installation: PASS` + `change/admin-state-persistence-v2` + `Totals: 1 passed, 0 failed` |
| 2 | `cargo fmt --all -- --check` | **0** | 623 ms | rustfmt 1.9.0-stable (48a229ceae) / rustc 1.98.1 | 无输出（全部文件已按 rustfmt 格式化，无 diff） |
| 3 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | **0** | 14263 ms | clippy 0.1.98 (48a229ceae) / cargo 1.98.1 | 全量编译 173 个依赖 + 6 个 crate（`acpr-transcript`/`core`/`acpr-wire`/`sync-protocol`/`node-link-protocol`/`storage-sqlite`），`Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 14.02s`；**0 warning、0 error**（`-D warnings` 生效下仍退出 0） |
| 4 | `cargo test --locked --workspace --all-features` | **0** | 26940 ms（首跑，流分离）／4027 ms（合并流重跑，编译产物已缓存） | cargo 1.98.1 | 38 个测试单位（32 个 test 目标 + 6 个 doc-test 目标）；合计 **285 passed / 0 failed / 2 ignored**；无 `error`、无失败目标、无异常跳过 |
| 4a | 同上（重跑，`2>&1` 合并流） | **0** | 4027 ms | 同上 | 与首跑结论一致；合并流使「目标 ↔ `test result:`」可配对，见日志附录 B |

### 4/4 逐目标结果（日志附录 B 摘要）

| 目标 | running | result |
|---|---|---|
| `unittests src\lib.rs`（acpr_transcript） | 0 | ok. 0 passed; 0 failed; 0 ignored |
| `tests\codec.rs` | 18 | ok. 18 passed |
| `tests\table.rs` | 6 | ok. 6 passed |
| `unittests src\lib.rs`（acpr_wire） | 19 | ok. 19 passed |
| `tests\cj1_fixtures.rs` | 1 | ok. 1 passed |
| **`unittests src\lib.rs`（acp_core）** | **77** | ok. 77 passed; 0 failed |
| `unittests src\lib.rs`（node_link_protocol） | 6 | ok. 6 passed |
| `tests\envelope_fixtures.rs`（node-link-protocol） | 6 | ok. 6 passed |
| `tests\pairing_fixtures.rs`（node-link-protocol） | 7 | ok. 7 passed |
| `tests\schema_drift.rs`（node-link-protocol） | 5 | ok. 5 passed |
| `tests\tables_match_registry.rs`（node-link-protocol） | 3 | ok. 3 passed |
| `tests\transcript_vectors.rs`（node-link-protocol） | 2 | ok. 2 passed |
| `unittests src\lib.rs`（storage_sqlite） | 3 | ok. 3 passed |
| **`tests\admin_store.rs`** | **28** | ok. 28 passed; 0 failed |
| `tests\attachments.rs` | 5 | ok. 5 passed |
| `tests\commit.rs` | 16 | ok. 15 passed; 0 failed; **1 ignored**（`crash_child`） |
| `tests\compaction_recovery.rs` | 3 | ok. 3 passed |
| `tests\contract_v03.rs` | 5 | ok. 5 passed |
| **`tests\enum_coverage.rs`** | **2** | ok. 2 passed; 0 failed |
| `tests\imported.rs` | 9 | ok. 9 passed |
| **`tests\migration.rs`** | **8** | ok. **7 passed; 0 failed; 1 ignored**（`regenerate_v2_fixtures` = 夹具生成器） |
| `tests\permissions.rs` | 4 | ok. 4 passed |
| `tests\retention.rs` | 8 | ok. 8 passed |
| `unittests src\lib.rs`（sync_protocol） | 1 | ok. 1 passed |
| `tests\body_constraints.rs`（sync-protocol） | 9 | ok. 9 passed |
| `tests\envelope_fixtures.rs`（sync-protocol） | 7 | ok. 7 passed |
| `tests\field_constraints.rs`（sync-protocol） | 10 | ok. 10 passed |
| `tests\pairing_fixtures.rs`（sync-protocol） | 6 | ok. 6 passed |
| `tests\schema_drift.rs`（sync-protocol） | 5 | ok. 5 passed |
| `tests\tables_match_registry.rs`（sync-protocol） | 3 | ok. 3 passed |
| `tests\transcript_vectors.rs`（sync-protocol） | 2 | ok. 2 passed |
| `tests\view_projections.rs`（sync-protocol） | 3 | ok. 3 passed |
| `Doc-tests`（6 个 crate） | 0 | ok. 0 passed（见下「零用例说明」） |

与 `verification.md`/`plan.md` 的既有期望一致：`core` 77 passed、`admin_store` 28、`enum_coverage` 2、`migration` 7 passed / 1 ignored、drift 门禁 36 DDL / 15 trait / 87 方法。

### 本单元关键行为用例在本轮确实执行且通过（取自 4/4 输出）

```
test use_cases::tests::pairing_settlement_carries_the_target_family_audit ... ok
test use_cases::tests::create_session_authorizes_before_resolving_the_workspace ... ok
test model::tests::entity_ref_exposes_kind_and_target_id_for_storage_columns ... ok
test model::tests::local_config_values_enforce_their_invariants ... ok
test a_failed_upgrade_rolls_back_to_v1 ... ok            (tests\migration.rs)
test v1_fixture_upgrades_to_v2_and_preserves_rows ... ok  (tests\migration.rs)
```

### 零用例 / ignored 说明（判定为设计状态，非异常）

- **ignored 2 条，均由用例注释声明为手动/子进程用例**：`crash_child`（由 `crash_recovery_leaves_an_consistent_database` 起子进程）、`regenerate_v2_fixtures`（夹具生成器，仅重建 `fixtures/storage/v2` 时手动跑）。两者都不是「被跳过的断言」：migration 目标 8 条中 7 条真跑。
- **零用例目标**（不构成「异常跳过」）：`acpr_transcript` lib 单元测试为 0 —— `crates/acpr-transcript/src` 内 `` `#[cfg(test)]` `` 经 grep 无命中，其覆盖由 `tests/codec.rs`(18) + `tests/table.rs`(6) 承担；6 个 doc-test 目标为 0 —— 全仓 rustdoc 内唯一代码块是 `crates/storage-sqlite/tests/migration.rs` 里标了 `text` 语言的三反引号块（非 Rust，且不在 doctest 目标内），故无 doctest 可收集。两者在 4/4 输出中均有各自的 `Running`/`Doc-tests` 进度行与 `test result: ok.` 行，说明目标确实被执行而非被跳过。

## issues

- **无阻断项、无失败项**。四条命令退出码全 0；未出现零失败目标、未出现 `error:`、未出现 `warning:`（clippy 在 `-D warnings` 下通过）。
- 观察项（不影响结论，供主 Agent 记录/判断）：
  1. 工作树在候选之上带有主 Agent 未提交的 `openspec/changes/admin-state-persistence-v2/tasks.md` 改动（勾选 3.2/3.4/3.6 并追加执行记录）。本报告已声明验证对象 = 「候选 + 该记录改动」，该文件与四条命令的判定无关。若要求对**纯净候选树**再跑一遍，需要主 Agent 先提交/暂存该改动（本执行者无 `stash`/`checkout` 授权）。
  2. `cargo test` 第 4 条命令被跑了两次（首跑 stdout/stderr 分流导致「目标 ↔ result」无法配对，故以 `2>&1` 合并流重跑一次用于留证）。两次均 exit 0，结论一致；日志附录 A 保留首跑完整输出，正文 `[4/4]` 为合并流运行。
  3. 本执行者未跑 `cargo-deny`/`gitleaks`（CI 专属，本地无等价物，依 `plan.md` 不作为本机 PV1 门禁）。
- **未解决项**：无。候选 PASS **不等于**已合入或最终验收 PASS（合并需授权、`6.6`–`8.x` 另行处理）。

## result

**PASS** —— 固定候选 `aed9fb5` 上 `npm run verify` 口径四条命令全部退出 0：`npm run check` 十道门禁全 OK（含 `contract drift OK: 36 DDL / 15 trait / 87 方法`、crate 边界 6/6、agentic gate PASS）；`cargo fmt --check` 零 diff；`cargo clippy … -D warnings` 零告警；`cargo test --locked --workspace --all-features` 38 个测试单位共 **285 passed / 0 failed / 2 ignored（均为声明的设计性 ignored）**。无零失败目标、无异常跳过。

## evidence_paths

- `reports/du1-pv1.log`（71 241 字节 / 1 463 行；含四条命令**完整输出**与逐子项退出码、工具版本、耗时，附录 A = 4/4 首跑完整输出，附录 B = 逐目标汇总）
  - SHA-256：`65895455952f9c5abbe69b2745f59ae3312c6a791639b93d5ac958889ea0c490`
  - `*.log` 受 `.gitignore` 约束，未 `git add`（按任务书要求）
- `reports/du1-checker.md`（本报告）

## resource_cleanup

- 本执行者创建：① `reports/du1-pv1.log`；② `reports/du1-checker.md`；③ 隔离构建目录 `D:\Project\acp-remote\target\du1-pv1\`（任务书明确允许的隔离构建目录；`target/` 被 gitignore，`git status --porcelain` 中不出现）。无其它文件创建。
- 验证后 `git status --porcelain` 复核：仅 ` M openspec/changes/admin-state-persistence-v2/tasks.md`（主 Agent 记录改动，非本执行者所写）+ 既有未跟踪的宿主项（`.omp/`、`.pi/**`、`reports/parked-idle-identity-retention.patch`、`reports/du1-integrator.md` 为并行执行者产物）。`git diff --stat -- fixtures` 为空 ⇒ 测试未改动任何夹具二进制。
- 未创建临时脚本、未写入报告以外的目录、未触碰 `.omp/`、`.pi/**`、`reports/parked-idle-identity-retention.patch`，未与其它执行者共用文件。
- 如需回收磁盘：`rmdir /s /q target\du1-pv1`（保留了日志即可重现结论）。如需重跑：见日志头部的 `CARGO_TARGET_DIR` 与四条命令。
