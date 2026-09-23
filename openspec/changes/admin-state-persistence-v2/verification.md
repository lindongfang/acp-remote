# admin-state-persistence-v2 — 执行记录

## Target

- 变更：`admin-state-persistence-v2`（schema `agentic`）
- 仓库：`D:\Project\acp-remote`
- 主分支引用：`refs/heads/main` = `37a398e9dbafa368bdb15853e1c1d9b40f19b28d`（`git rev-parse`）
- 实现分支：`feat/admin-state-persistence-v2`，起点 `28f8cb97c7297d83b9df7b15b16d0b059e2dec78`（= `refs/heads/docs/pre-implementation-contracts` HEAD，main 的后代）
- **W0 基线提交**：本记录所在的那个提交（分支 `feat/admin-state-persistence-v2`；其父提交是 `28f8cb97c7297d83b9df7b15b16d0b059e2dec78`）。W0 的完成条件（合同漂移门禁绿 + 全部本地门禁绿）就在该状态上成立，W1 的三条轨道以此为固定基线。
- 本轮证据对应的版本：**W0 基线提交**（不再是未提交工作区）；该提交包含 core 的 WP1–WP3、`storage-sqlite` 的 v2 DDL/migration、合同并入与全部 W0 证据日志。
- 版本确认负责人：主 Agent（`git rev-parse`/`git status`/`git worktree list`）

## Checks

| Check ID / Stage / Work Package | Revision / Base | Scope | Executor | Command / Steps | Environment | Result / Exit Code | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| PV4 / WP1 交付前 | `28f8cb9` + 工作区 | `core` 的 fmt/clippy/测试（值对象、端口、用例） | 主 Agent（coder 角色） | `cargo fmt --all -- --check`；`cargo clippy --locked -p core --all-targets --all-features -- -D warnings`；`cargo test --locked -p core --all-features` | rustc 1.98.1（`rust-toolchain.toml`），Node 24.19.0 | **PASS**：fmt 退出 0；clippy 无输出（无 lint）；`75 passed; 0 failed` | `reports/wp1-core-tests.log` |
| PV4 / WP2+WP3 交付前 | `28f8cb9` + 工作区 | 端口目标签名、写集 DTO、`UseCases` 写集路径与 workspace 解析 | 主 Agent（coder 角色） | 同上命令（同一固定版本） | 同上 | **PASS**：`75 passed; 0 failed`（含新增 5 个用例） | `reports/wp3-core-tests.log` |
| PV3 / WP5 | `28f8cb9` + 工作区 | 依赖方向矩阵 + `core` 普通依赖闭包 allow-list | 主 Agent | `node scripts/check-crate-boundaries.mjs` | 本地 `cargo`（`cargo metadata`/`cargo tree`），不联网 | **PASS**：`crate boundaries OK`（6 个 crate 已登记） | `reports/wp5-boundaries.log` |
| PV1（部分）/ 全工作区 | `28f8cb9` + 工作区 | `cargo test --locked --workspace --all-features` | 主 Agent | 同左 | 同上 | **PASS**：37 个测试目标全部 `ok`（含 `storage-sqlite` 的既有 migration/retention/enum_coverage 等） | 本轮终端输出（未落盘；PV1 完整证据待 6.3/6.7） |
| PV2 / 合同漂移 | W0 基线提交（父 `28f8cb9`） | §5.3/§7.3/§7.4 与 `ports.rs`/`migrate.rs` 逐条一致 | 主 Agent | `node scripts/check-contract-drift.mjs` | 只读脚本 | **PASS**：`contract drift OK: §7 的 36 条 DDL 与 crates/storage-sqlite/src/migrate.rs 逐条一致；§5 的 15 个 trait / 87 个方法签名与 crates/core/src/ports.rs 一致`（改前红→改后绿两段都留档） | `reports/wp5-contract-drift-before-after.md` |
| PV3 / WP5（W0 复跑） | W0 基线提交 | 依赖方向矩阵 + `core` 普通依赖闭包 allow-list | 主 Agent | `node scripts/check-crate-boundaries.mjs` | 本地 `cargo`（`cargo metadata`/`cargo tree`），不联网 | **PASS**：`crate boundaries OK`（W0 复跑段追加在文件末尾） | `reports/wp5-boundaries.log` |
| PV4 / WP4 交付前（含 2.15–2.18） | W0 基线提交 | `storage-sqlite` 的 fmt/clippy/测试（v2 DDL、12-step 重建、Import 拆分迁移、v2 夹具与升级保留断言） | 主 Agent | `cargo fmt --all -- --check`；`cargo clippy --locked -p storage-sqlite --all-targets --all-features -- -D warnings`；`cargo test --locked -p storage-sqlite --all-features` | rustc 1.98.1（`rust-toolchain.toml`），Node 24.19.0 | **PASS**：fmt 退出 0；clippy 退出 0（无 lint）；11 个测试目标全部 `ok`（`migration` 6 passed / 1 ignored 为夹具生成器、`imported` 9、`retention` 8、`enum_coverage` 2、`permissions` 4、`commit` 15、`contract_v03` 5、`compaction_recovery` 3、`attachments` 5、lib 单测 3） | `reports/wp4-migration-tests.log` |
| PV1（部分）/ 工作区 Rust（W0） | W0 基线提交 | `cargo fmt` / `cargo clippy --workspace` / `cargo test --locked --workspace --all-features` | 主 Agent | 同左 | 同上 | **PASS**：fmt 与 clippy 退出 0；workspace 全部测试目标 `ok`、0 failed（37 个目标；`core` 75 用例不变） | `reports/w0-verify-rust.log` |
| PV1 / `npm run check`（W0） | W0 基线提交 | 十道合同门禁全绿（含 `check:docs` 与 `check:drift`） | 主 Agent | `npm run check` | Node 24.19.0、仓库内 `node_modules`，不联网 | **PASS**：退出码 0（完整输出留档） | `reports/w0-npm-check.log` |

## Check Plan Changes

本轮实现相对原计划的三处偏差，均已同步到代码注释与 `tasks.md`，需要在任务 2.1 写入合同时一并反映：

1. **`EntityRef` 新增 `Provider(String)` 变体**（`crates/core/src/model/ids.rs`）：§11.6 要求 `provider.configure` 的写集必须携带 `provider.configured` 审计，而 `PendingAudit.target` 是 `EntityRef`，原枚举没有任何可表达 Provider 引用的变体。新增变体后 `kind()` = `"provider"`、`target_id()` 为 Provider 引用 id。影响：§3.1 的 `EntityRef` 行、§7.3/§7.4 审计表的 `target_kind` 取值（存储侧实现时）需一并说明；不影响 wire（`EntityRef` 不出现在协议载荷里）。
2. **`UseCases::create_session` 增加 `workspace_alias: Option<WorkspaceAlias>` 参数**：§3.6 的目标形状把 `CreateSessionRequest.workspace` 改成 `Option<ResolvedWorkspace>`，因此 alias 必须在进入该结构之前解析；解析归 core（§11.9），Export 侧「alias 是否声明」的校验由 `server::node_link` 负责。
3. **`ImportWrite.exports` 与 `record.export_ids()` 必须相等**：§11.6 的 DTO 同时含有 `record.export_ids`（既有模型）与 `exports`，两者分歧会让管理行与关联行不一致；端口文档明确要求存储层返回 `InvalidRequest`。
4. **`owned_audit`/`imported_audit` 的 CHECK 只在 `migrate.rs` 常量的**新建路径**中扩宽（`+5` 个取值）**：这是为了让 `crates/storage-sqlite/tests/enum_coverage.rs` 恢复绿。**v1 既有库的 12-step 表重建（任务 2.15）与非空 DB 的取值可用性仍未实现**——本轮结束时「新建库可写新审计动作、升级库不可写」的不一致状态**不可发布**，必须在任务 2.15/2.17 完成后复核。
5. `UseCases::audit_action` 暂时保留 `#[allow(dead_code)]`：管理写集不再使用它（审计随 `WriteContext` 提交），但保留给尚无关联状态变更的审计入口。
6. **执行模型变更（2026-09-23，本轮暂停时记录）**：原计划的 W1「WP5+WP1 并行」在单会话下被合并为一段串行实现，2.1/2.2（合同并入）被推到实现之后。后续改用多执行者后重新定为 **W0 串行冻结（2.14 → 2.1 → 2.2 + 基线提交）→ W1 三轨道并行（WP4 剩余 ∥ WP6 ∥ WP1–WP3 的独立 review）→ W2/W3/W4**，见 `plan.md` 的 Execution Waves 与多 Agent 分派规则。该重排不改变任何任务编号与要求。
7. **本轮的 2.14 半成品归属**：`migrate.rs` 两条审计 CHECK 的扩宽（为恢复 `enum_coverage`）并入 W0 的 2.14，必须与 12-step 重建语句一起定稿；在此之前该状态**不可发布**（F1）。

### W0 执行期的偏差与决定（2026-09-23，本轮落地）

8. **WP4 的 2.14–2.18 在 W0 内一次闭合（执行波次调整）**：把三个版本常量推进到 2 必然让既有 `tests/migration.rs`（`empty`/`too-new` 夹具假定）与 `tests/commit.rs` 的 `user_version == 1` 变红，删列还会让 `session_store.rs` 的四处 `imported_import` 查询与 `tests/imported.rs` 的黄金列清单失效。因此 W0 实际执行了 tasks.md 的 **2.14 + 2.15 + 2.16 + 2.17 + 2.18** 与这些连带修改；`plan.md` 的 W1「WP4 剩余（2.15–2.18）」轨道因此为空，W1 只剩 WP6（2.19–2.22）与独立 review（3.2/3.4）。任务编号与要求不变。
9. **`imported_import` 的 v2 形状新增 `grants_json TEXT NOT NULL`**：依据 §11.1 目标表的「grant 集合」与 §11.6 的 `ImportWrite { record: ImportRecord, .. }`——Import 的 grants 必须落在管理行上，而 v1 没有这一列。迁移对旧行一律写 `'[]'`（无可信来源，不默认放权），该 Import 因此保持不可用。
10. **`RemoteDeliveryStore::drop_import` 去掉写 `removed_at` 的语句**：合同只把「连接级清空交付索引与命令引用」授予它（§5.2、§11.6 的「两处删除权威不得重叠」），移除标记属 `ImportRemoval` 写集；且本 crate 不读系统时间，旧实现把 `owner_node_id` 绑进 `removed_at` 是一个随重写消失的缺陷。查询改走 `imported_import_export`（一个 Import 可关联多个 Export）。
11. **§11 标题改名连带的引用同步**：`§11.` 由「（待实现）」改为「（形状已并入 §3/§5/§7）」，因此同步更新了 `README.md`、`docs/DEVELOPMENT_PLAN.md` 的锚点与文案，以及 `AGENTS.md` §1/§10 的措辞；`docs/CORE_PORTS_AND_STORAGE.md` 新增 §3.7（本地配置与凭据值对象）以承接从 §11.6 删除的 DTO 形状。
12. **夹具生成方式**：`fixtures/storage/v2/{empty,from-v1,too-new}.sqlite3` 由 `crates/storage-sqlite/tests/migration.rs` 的 `#[ignore]` 生成器 `regenerate_v2_fixtures` 产出（`cargo test -p storage-sqlite --test migration -- --ignored regenerate_v2_fixtures`）；`from-v1.sqlite3` 以 `fixtures/storage/v1/empty.sqlite3` 为基底插入 v1 形状的行，并刻意留下 `owned_audit.audit_id = 1,2,5`（`sqlite_sequence.seq = 5`）以覆盖「序列领先于 `max(audit_id)`」。

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| WP2/WP3 | WP1 | `28f8cb9` + 工作区；`reports/wp1-core-tests.log` | 变更分支起点 | `cargo test -p core` 通过（75 用例） | WP1 值对象或端口形状变化 → WP2/WP3 复验 |
| WP4 | WP5（2.1 的 DDL 定稿） | W0 基线提交的 `migrate.rs` DDL 常量与 §7.3/§7.4 逐条一致（`reports/wp5-contract-drift-before-after.md`） | 变更分支起点 | `node scripts/check-contract-drift.mjs` 退出 0 且报 36 条语句 | §7 DDL 变化 → WP4/WP6 与夹具复验 |
| WP6 | WP2、WP4 | W0 基线提交；`ports.rs` 的写集签名 + `migrate.rs` 的 v2 常量与升级语句；`reports/wp4-migration-tests.log` | W0 基线提交 | `cargo check -p storage-sqlite` 通过（既有实现与测试全绿） | 端口签名或 DDL 变化 → WP6 复验 |
| WP5 | WP1 | core 新增 `p256`/`sha2`（直接依赖固定为 `async-trait`/`thiserror`/`p256`/`sha2`） | 变更分支起点 | `cargo tree -p core --edges normal` 与 allow-list 逐项相等（`reports/wp5-boundaries.log` 的 W0 复跑段） | 依赖版本变化 → allow-list 与 PV3 复验 |

## Runtime Resources

- 共享设施：Rust 构建目录 `target/`（唯一），W0 串行使用，无并发分片，故未设 `CARGO_TARGET_DIR`；W1 的三条并行轨道必须各自设置 `CARGO_TARGET_DIR`（见 `plan.md` 的多 Agent 分派规则）。
- SQLite：W0 的 storage 测试各自使用独立临时目录；夹具 `fixtures/storage/v2/{empty,from-v1,too-new}.sqlite3` 已生成并只读使用（生成器为 `#[ignore]` 用例，不覆盖 `fixtures/storage/v1/`）。
- 无数据库服务、容器、端口、账号或外部网络资源。

## Review Findings

独立 review（任务 3.2 / 3.4）**未执行**：本轮没有可用的独立 reviewer 隔离上下文（单 Agent 会话无法新建不继承实现推理的子 Agent）。按 apply instruction「无独立 reviewer 不得以自审替代」，相关任务保持待办并记为 BLOCKED，直至出现独立检视者。

| ID | Revision | Reviewer | Location | Severity / Impact | Resolution | Recheck Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| RV1-BLOCKED | `28f8cb9` + 工作区 | 无（缺隔离上下文） | WP1–WP3 全部改动 | 阻断：交付前独立 review 未做 | 待独立 reviewer 完成后回填 | 无 |

## Merge History

W0 在门禁全绿的状态上落了**基线提交**（即本记录所在的提交，分支 `feat/admin-state-persistence-v2`；父提交 `28f8cb9`），供 W1 的三条轨道作为固定基线。**未合入 main**、未推送、未开 PR（任务 5.x/6.x 未开始；合并与推送仍受当前会话授权限制）。

## Test Design and Authoring

`mode = not-applicable`，无 TP 分组。各工作包自带行为测试：

- `core`（WP1–WP3）：`crates/core/src/model/tests.rs`（`PeerPublicKey` 正负例、本地配置值对象不变量）与 `crates/core/src/use_cases.rs` 的 `#[cfg(test)] mod tests`（workspace 解析四类输入、未登记别名、缺失目录、`remove_import` 单写集、撤销设备审计随写集）。
- `storage-sqlite`（WP4）：`tests/migration.rs` 的 v2 夹具幂等、过新拒绝零写入、`v1_fixture_upgrades_to_v2_and_preserves_rows`（序号/origin cursor/`requestId`/`audit_id` 与序列保留、12-step 重建的 DDL 文本与新取值可用、Import 关联迁移与不补 grants）与 `second_open_of_an_upgraded_database_rewrites_nothing`；`tests/imported.rs` 的 6 表黄金列清单与无正文行为；`tests/enum_coverage.rs`、`tests/commit.rs`、`tests/retention.rs`、`tests/permissions.rs` 的既有判据在 v2 上复跑。

## Candidate E2E

无（`mode = not-applicable`）。

## Main E2E

- 项目开关：`npx --quiet --no-install openspec-agentic e2e --json` → `enabled: true`、`command: ""`、`maxAttempts: 3`。
- `plan.md` 的 mode：`not-applicable`，附用户降级批准原话（2026-09-23，本会话）。
- E2E 结论：**NOT_APPLICABLE**（未执行任何 E2E）。
- 替代检查：`7.1` 的替代验证（cargo 测试 + `npm run verify` + 漂移门禁 + `from-v1` 夹具重放）**尚未作为最终验证执行**——W0 已在基线上让漂移门禁与 `npm run verify` 的两半都可通过（`npm run check` 与 `cargo fmt/clippy/test` 全绿），`fixtures/storage/v2/from-v1.sqlite3` 也已生成并在 `tests/migration.rs` 中重放；按计划，替代验证仍须在 **W4 的最终主分支版本**上重跑并写入 `reports/alt-final-verification.md`。
- 另注：W0 的 `storage-sqlite` 夹具重放（v1 → v2 升级的行数/序号/`audit_id` 断言）已随 `cargo test -p storage-sqlite --all-features` 执行，证据见 `reports/wp4-migration-tests.log`。

## Failures and Retests

| Issue ID / Task | Source / Check or E2E ID / Attempt / Version | Owner | Fix / Recovery | Review / Readiness Evidence | Retest Evidence | Current Status / Basis |
| --- | --- | --- | --- | --- | --- | --- |
| F1 / 2.15 | `cargo test --workspace`：`storage-sqlite --test enum_coverage` FAILED（审计 DDL 字面量 vs `AuditAction::ALL`，新增 5 个取值） | 主 Agent | 在 `migrate.rs` 的两条审计 CHECK 中补 5 个新取值，并为**既有库**补 12-step 表重建（`V2_UPGRADE_OWNED`/`V2_UPGRADE_IMPORTED`），重建后按旧 `sqlite_sequence` 回填序列 | 独立 review 未做（3.6 属 W2，缺隔离上下文时记 BLOCKED） | 重跑 `cargo test --locked -p storage-sqlite --all-features` → 11 个目标全 `ok`；`v1_fixture_upgrades_to_v2_and_preserves_rows` 断言升级库的 `owned_audit` DDL 含 `provider.configured`、`audit_id` 保留 `[1,2,5]`、序列仍为 5、新写入得 `audit_id = 6` | **已解决**（新建库与升级库两侧一致；证据 `reports/wp4-migration-tests.log`） |
| F2 / 2.1·2.2·2.14 | `node scripts/check-contract-drift.mjs`：§5.3 `TrustStore`/`ExportStore` 方法集、§7.3/§7.4 审计 CHECK 不一致 | 主 Agent | §5.3 逐字转录 `ports.rs` 的 §5.3 区域；§7.3 追加 9 张管理表 + 2 个索引、§7.4 改 `imported_import` 并追加 `imported_import_export`、两张审计表 CHECK 扩宽；§7.2/§3/§9/§11 与关联文档同步 | 独立 review 未做（3.8 属 W2，缺隔离上下文时记 BLOCKED） | 重跑 `node scripts/check-contract-drift.mjs` → 退出 0（`§7 的 36 条 DDL …；§5 的 15 个 trait / 87 个方法签名 …`）；`npm run check` 退出 0 | **已解决**（证据 `reports/wp5-contract-drift-before-after.md`、`reports/w0-npm-check.log`） |
| F3 / 2.1 附带 | `node scripts/check-doc-links.mjs`：`README.md`/`docs/DEVELOPMENT_PLAN.md` 的 `#11-管理状态持久化合同待实现` 锚点失效；`design.md` 里指向 `CORE_PORTS_AND_STORAGE.md` 的 §5.3/§3.5 被误归因到身份合同；本文件的 Required Follow-up 行里的 §12 被误归因到配置参考 | 主 Agent | 按合同 §11 新标题更新两处锚点与文案；把 design.md 的引用改成指名 `CORE_PORTS_AND_STORAGE.md`；重写本文件的 Required Follow-up 行使其不含歧义 §-引用 | 无（门禁自证） | 重跑 `node scripts/check-doc-links.mjs` → 退出 0；`npm run check` 退出 0 | **已解决**（证据 `reports/w0-npm-check.log`） |

## Final Assessment

```agentic-assessment
assessment_id: "admin-state-persistence-v2-w0-baseline"
target_commit: "28f8cb97c7297d83b9df7b15b16d0b059e2dec78"
contract_digest: "sha256:1191945887f7001486e328786ad7b0395999317def7c1f0e51054c225951ec61"
result: BLOCKED
evidence:
  - path: reports/wp1-core-tests.log
    sha256: "sha256:f294a5a7aa12becdd4a1ff7c32f47a8bd03cf0c4b7b5609989f515ef0438e7b9"
  - path: reports/wp3-core-tests.log
    sha256: "sha256:f294a5a7aa12becdd4a1ff7c32f47a8bd03cf0c4b7b5609989f515ef0438e7b9"
  - path: reports/wp4-migration-tests.log
    sha256: "sha256:ff8405540caefe49c44b5fc63a7d50beafa4c8d6a686af1074153b169b88d949"
  - path: reports/wp5-contract-drift-before-after.md
    sha256: "sha256:386a808109d07ff84894da9c366a631b9a38bc9d6a538c08ff3c12e9978b8d76"
  - path: reports/wp5-boundaries.log
    sha256: "sha256:71f677d60f7f23043139a2cb433b0b30e0670a37eef02e6b6968fb8a6f0a12b1"
  - path: reports/w0-verify-rust.log
    sha256: "sha256:2c51ea13ed78c41323425d62aa65c90df00a00df3153474e1f54f5a58c0bd402"
  - path: reports/w0-npm-check.log
    sha256: "sha256:90410d8c4e39b80e398c1581c598d9e71b9faeceb762eb621a509bce27c953ea"
```

`target_commit` 是**本基线提交的父提交**（= 证据产生的变更分支起点 `28f8cb9`）；基线提交本身即本记录所在的提交，W1 以它为固定基线（见 `Target` 的「W0 基线提交」行）。`contract_digest` 由 `npx --quiet --no-install openspec-agentic workflow check --change admin-state-persistence-v2 --stage plan --json` 在 W0 回填 `tasks.md`/本文件之后重跑得到（该次检查 `result: PASS`）；任何契约内容再变化都会使该值失效，必须重跑。

- Assessment ID / Time: `admin-state-persistence-v2-w0-baseline`，2026-09-23（W0 收尾时）
- Target / Task: 见 `Target`；本轮完成 `2.14`–`2.18`、`2.1`、`2.2`、`3.5`、`3.7`；最终验收任务 `8.1` 未开始
- CLI State: `openspec status` = 5/5 artifacts complete；`npx --quiet --no-install openspec-agentic workflow check --change admin-state-persistence-v2 --stage plan --json` = PASS（`contractDigest` 见上方评估块）。CLI 状态不表示实现完成
- Audit / Evidence: W0 完成了 `storage-sqlite` 的 v2 DDL、两张审计表的 12-step 重建（含 `sqlite_sequence` 回填）、`imported_import` 拆分迁移与 v2 三件夹具（`cargo test -p storage-sqlite --all-features` 全绿），合同并入（`check:drift`/`check:boundaries`/`check:docs` 全绿）与合同/关联文档同步；`npm run check`、`cargo fmt`、`cargo clippy --workspace`、`cargo test --workspace` 全部退出 0。仍未做：WP6 的三个管理 store、独立 review（`3.2`/`3.4`/`3.6`/`3.8`/`3.10`）、候选与合入、替代验证与最终验收
- Result / Open Issues: **BLOCKED** —— 未完成任务：`1.4`（WP6 的交接核对，属 W1）、`2.19`–`2.22`、`3.2`/`3.4`/`3.6`/`3.8`/`3.10`、`5.x`、`6.x`、`7.x`、`8.x`；F1/F2/F3 已闭环，无未解决的 FAIL
- Required Follow-up: W1 以本基线提交为固定基线并行推进——WP6（`2.19`–`2.22`：实现三个管理 store，写 `crates/storage-sqlite/src/lib.rs`、`error.rs`、`session_store.rs` 与新增 store 模块）与 WP1–WP3 的独立 review（`3.2`/`3.4`，需不继承实现对话的 reviewer）；随后 W2（`3.6`/`3.9`/`3.10`）→ W3（`5.x`/`6.x`）→ W4（`7.x`/`8.x`）
