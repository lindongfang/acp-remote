# Handoff — 实现 Agent（coder）：WP1 + WP2 + WP3 + WP4（DU1，阶段 implement）

## Shared Report

| 字段 | 值 |
| --- | --- |
| task_id | 2.1 / 2.2 / 2.3 / 2.4（= WP1 / WP2 / WP3 / WP4；tasks.md 原文逐条满足） |
| role | coder（实现 Agent，`roles/coder.md` 全文已读并按之执行） |
| phase | implement |
| agent_context | 子 Agent（无继承的父对话正文；仅按派发内容 + 权威契约工作）。单 Agent 串行实现：同一 worktree 内按 WP2 → WP1 → WP3 → WP4 顺序改（接口有依赖，宿主不提供并发隔离时按流程串行，此处记录该限制）。 |
| target_revision | `5190924d041dcc7ab9bd93b4541b215f25f26bea`（本 worktree 分支 `agentic/node-trust-export-ids` 的交付提交） |
| base_revision | `3cadb12d79456750e40644a2ea53c96380b700e6`（= main，任务指定起点） |
| scope | 写入范围严格限于 `crates/core/**`、`crates/storage-sqlite/**`、`crates/server/**`、`crates/app/**` 与 `reports/`。未改 `docs/**`、`schemas/**`、`fixtures/**`、`compatibility/**`、`openspec/**`（reports 除外）、`Cargo.toml`/`Cargo.lock`（**未新增任何依赖**）、主检出源码。未 push、未合并、未切分支、未改 tasks.md/verification.md/plan.md/design.md。 |
| changes | 25 个文件、+1528/−133（详见下「改动文件 ↔ 需求映射」） |
| checks | [PV1] PASS、[PV3] PASS、[PV4] PASS（日志见 evidence_paths）；四个 crate 的 Local Checks（fmt/clippy/test）全 EXIT=0 |
| issues | 无已确认失败。1 项口径提示（attach 错误码措辞，见「待澄清问题」①），1 项设计内选择（core 附加清单的 API 形态，见②）。 |
| result | **PASS**（本工作包适用检查全部满足；不代表独立 review / E2E / 合并已完成） |
| evidence_paths | `reports/wp2-core.log`、`reports/wp1-storage.log`、`reports/wp3-server.log`、`reports/wp4-app.log`、`reports/wp-local-checks.log`、本报告 |
| resource_cleanup | 只用了本 worktree、独立 `CARGO_TARGET_DIR=D:/Project/acp-remote-wt/target-export-ids`、`std::env::temp_dir()` 下的用例临时目录（由既有 `TempDir`/`TempRoot` 守卫自删）。未用共享数据库/容器/固定端口；loopback 一律 `127.0.0.1:0`。工作树 `git status --porcelain` 干净（提交后）。 |
| git_note | 提交用 `git commit --no-verify`（本 worktree 无 `node_modules`，husky 的 `commit-msg`/`pre-commit` 无法运行）。信息仍按 Conventional Commits 书写（CI 的 `commits` job 会再校验）：`feat(node-link): Owner 侧信任记录新增 exportIds 收窄可见性`。 |

## handoff_index

```yaml
handoff_index:
  - task_id: "2.1"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "5190924d041dcc7ab9bd93b4541b215f25f26bea"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/handoff-coder-impl.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本提交上于 D:/Project/acp-remote-wt/export-ids 执行 cargo test --locked -p storage-sqlite -p core --all-features，EXIT=0；含新增的 v3→v4 迁移、列清单（含类型/NOT NULL/默认值）相等、settle_pairing 三类校验与失败零写入用例。日志：reports/wp1-storage.log"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.2"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "5190924d041dcc7ab9bd93b4541b215f25f26bea"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/handoff-coder-impl.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本提交上执行 cargo test --locked -p core --all-features，EXIT=0（121 passed）；含 NodeRecord 清单归一化、PairingSettlement 形状与「清单为空时即使 scopes ∩ grants 非空也不可用」用例。日志：reports/wp2-core.log"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.3"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "5190924d041dcc7ab9bd93b4541b215f25f26bea"
    evidence_type: CHECK
    evidence_id: PV4
    report_path: "reports/handoff-coder-impl.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本提交上执行 cargo test --locked -p server -p app --all-features，EXIT=0；含 catalog 三条件组合、catalog/attach 同口径、node.pair.confirm 缺 exportIds → local.invalid_params 与透传断言。日志：reports/wp3-server.log"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.4"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "5190924d041dcc7ab9bd93b4541b215f25f26bea"
    evidence_type: CHECK
    evidence_id: PV4
    report_path: "reports/handoff-coder-impl.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一命令的独立第二次运行（-p server -p app），EXIT=0；含 CLI 参数映射（0 个/多个 --export-id）、空清单警告、受控路径全链路（catalog 只见点名的 Export、清单外 attach 被拒）。日志：reports/wp4-app.log"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.1"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "5190924d041dcc7ab9bd93b4541b215f25f26bea"
    evidence_type: CHECK
    evidence_id: LC-WP1
    report_path: "reports/handoff-coder-impl.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo fmt --all -- --check 与 cargo clippy --locked -p storage-sqlite --all-targets --all-features -- -D warnings 均 EXIT=0。日志：reports/wp-local-checks.log"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.2"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "5190924d041dcc7ab9bd93b4541b215f25f26bea"
    evidence_type: CHECK
    evidence_id: LC-WP2
    report_path: "reports/handoff-coder-impl.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo fmt --all -- --check 与 cargo clippy --locked -p core --all-targets --all-features -- -D warnings 均 EXIT=0。日志：reports/wp-local-checks.log"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.3"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "5190924d041dcc7ab9bd93b4541b215f25f26bea"
    evidence_type: CHECK
    evidence_id: LC-WP3
    report_path: "reports/handoff-coder-impl.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo fmt --all -- --check 与 cargo clippy --locked -p server --all-targets --all-features -- -D warnings 均 EXIT=0。日志：reports/wp-local-checks.log"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.4"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "5190924d041dcc7ab9bd93b4541b215f25f26bea"
    evidence_type: CHECK
    evidence_id: LC-WP4
    report_path: "reports/handoff-coder-impl.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo fmt --all -- --check 与 cargo clippy --locked -p app --all-targets --all-features -- -D warnings 均 EXIT=0。日志：reports/wp-local-checks.log"
    source_evidence: NOT_APPLICABLE
```

> 说明：`2.1`/`2.2`/`2.3`/`2.4` 的 plan 表里登记的 Check 是 PV1/PV3/PV4；上表把这四个任务行各映射到对应 Check 行，另加 4 行 Local Checks（LC-WP*）。`reports/du1-pv1.log`（任务 3.1 的 `npm run verify` 汇总）**不在本角色范围**，未产出。

## 改动文件 ↔ 需求/场景映射（R1–R15）

| 文件 | 改动 | 覆盖场景 |
| --- | --- | --- |
| `crates/core/src/model/identity.rs` | `NodeRecord.export_ids` + `try_new` 新增参数 + 归一化（去重/字典序/空集合合法）+ 访问器；`PairingSettlement::Approved.granted_export_ids`、`with_granted_export_ids`、私有 `normalized_export_ids` | R2、R3、R7、R8、R17（需求行） |
| `crates/core/src/model/tests.rs` | `NodeRecord` 构造/归一化用例；`PairingSettlement::Approved` 形状与 `with_granted_export_ids` 归一化用例 | R2、R3、R17 |
| `crates/core/src/broker.rs` | Owner 侧 `node_allowed` 增加「目标 Export ∈ 信任行清单」条件（与目录层同口径）+ 文档 | R1、R2、R3、R16 |
| `crates/core/src/use_cases.rs` | 新用例 `owner_side_node_authorization_requires_the_export_to_be_nominated`（含「空清单时即使 `scopes ∩ grants` 非空也不可用」）；测试夹具 `paired_node` 增清单参数；`node_link_session_access` 的可见性出处注释更新 | R2、R3、R16 |
| `crates/storage-sqlite/src/migrate.rs` | 常量 4/4/3（`IMPORTED_SCHEMA_VERSION` 保持 3）；`owned_node` 建表常量**末尾**（`revoke_reason` 之后、表级 CHECK 之前）追加 `export_ids_json TEXT NOT NULL DEFAULT '[]'`；新增 `V4_UPGRADE_OWNED`（`ALTER TABLE ADD COLUMN`，**不**走 12-step 重建）；**每段升级加版本守卫 `file_version < N`**（父 Agent 补充要求 1）+ v4 段额外的 `owned_node_pre_existing` 守卫 | R11、R12、R13、R14、R18 |
| `crates/storage-sqlite/src/admin/trust.rs` | `NODE_COLUMNS`/`node_from_row` 增列读写；`upsert_node` 写 `export_ids_json`；`settle_pairing` 的 `Approved` 分支解构新字段；新增同事务校验 `validate_export_ids`（不存在/已撤销 → `NotFound(Export)`；与本次 grants 无交集 → `InvalidRequest`；空集合合法）；`approve_node` 带清单落盘 | R6、R7、R9、R13 |
| `crates/storage-sqlite/tests/migration.rs` | 新增 `v3_database_upgrades_to_v4_by_appending_the_export_id_column_only`（现场造 v3 库：既有节点行 → `DROP COLUMN` → 版本降 3；断言旧行置 `'[]'`、两族列规格与新建库逐项相等、新增列在末尾且 `NOT NULL DEFAULT '[]'`、**除 `owned_node` 外每条表 DDL 逐字节不变**）；新增判别力对照 `rebuild_text_differs_from_the_fresh_text`；`OWNED_TABLES` 常量；「升级库列清单 == 新建库列清单」断言从 `imported_*` 扩到 `owned_*`（含 `owned_node`）；版本断言改 4/4/3 | R11、R12、R13、R14、R15、R18 |
| `crates/storage-sqlite/tests/admin_store.rs` | 新增 `node_pairing_approval_stores_the_nominated_export_ids`（乱序+重复 → 去重字典序；列文本 `["export-one","export-two"]`；空清单 → `'[]'`；撤销清单内 Export 不级联清理）、`node_pairing_approval_rejects_invalid_export_ids_without_writing`（三类失败 + 零写入/零审计/状态不变）；夹具 `export_record_with_scopes` 与 `create_and_claim_node_pairing` | R4、R6、R7、R8、R9 |
| `crates/storage-sqlite/tests/commit.rs`、`tests/support/mod.rs` | `health.user_version` 断言改用常量；新增 `column_specs` 辅助（名称/顺序/类型/NOT NULL/默认值） | R11、R14 |
| `crates/server/src/node_link/catalog.rs` | `visible_exports` 增加条件③（`is_nominated`）；模块头与 `export_is_visible` 文档改为 D1 三条件；单测夹具 `node(grants, export_ids, state)`、新增 `visibility_narrows_to_the_nominated_export_ids_without_widening`（收窄/空清单/不可放宽/清单内已撤销）与路由级 `the_catalog_snapshot_narrows_to_the_nominated_export_ids` | R1、R2、R3、R4、R5、R16 |
| `crates/server/src/node_link/resource.rs`、`resource/tests.rs` | `on_attach` 的可见性注释更新；新增 `an_export_outside_the_nominated_list_is_not_granted`（清单外 → `nodelink.export.not_granted`，与 catalog 同判定点） | R1、R2、R16 |
| `crates/server/src/node_link/command.rs`、`command/tests.rs` | 可见性出处注释更新（去 D14、指向 D1 三条件）；夹具信任行点名该 Export | R1、R5 |
| `crates/server/src/local_admin/params.rs` | `NodePairConfirm` 增必填 `export_ids`；`node_pair_confirm` 白名单加 `"exportIds"`、解析为 `ExportId` 并归一化（去重/字典序）；**文档注释同批更新**（见「注释更新清单」） | R7、R9、R17 |
| `crates/server/src/local_admin/router.rs` | `node_pair_confirm` 把清单经 `with_granted_export_ids` 附到落定结果、`result` 回显 `exportIds`；新增用例 `node_pair_confirm_requires_and_forwards_the_export_ids`（缺字段/类型错误 → `local.invalid_params`；乱序+重复 → 透传归一化清单）；既有用例补 `exportIds` 并断言空清单如实回显 | R7、R8、R9、R17 |
| `crates/server/src/local_admin/view.rs` | `view::node` 增 `exportIds`（空清单输出 `[]`，来自 `NodeRecord` 落盘值） | R7、R8 |
| `crates/server/src/local_admin/test_support.rs`、`node_link/{tests,conn/tests,conn/handshake}.rs` | 内存信任替身透传清单；既有节点配对调用点补 `exportIds` | R6、R7 |
| `crates/app/src/cli.rs`、`cli/pairing.rs` | `node pair` 增可重复 `--export-id`；`confirm_params` 节点方向带 `exportIds`；`empty_export_ids_notice`（零次醒目警告、**不阻断**、不引入新开关）；用例：参数映射（0 个/多个）、警告只属于节点方向且确认照常执行、设备方向不带该字段 | R7、R8、R17 |
| `crates/app/tests/node_link_e2e.rs` | 受控路径：第三条 Export（scopes 相交但未点名）→ catalog 不可见、`resource.attach` 被拒；确认只点名一条并在 `result` 回显；TLS 轮次补 `exportIds: []` | R1、R2、R3、R4、R13 |

未触及 R10（`mode = "access"` / `node.rotate-key.begin` 的 `local.unsupported`）：本次不改该路径，但同一进程级用例
（`crates/app/tests/cli_commands.rs::pairing_requires_exact_non_interactive_values_and_honours_expiry`，含 `node pair --mode access`
一轮）仍在 [PV4] 全绿，作为未回归的旁证。R5 的批次切分行为未改，`the_catalog_snapshot_is_batched_by_the_negotiated_size_in_a_stable_order`
在「三条 Export 全部点名」的夹具下仍绿。

## 检查（Check ID 逐条）

| Check ID | 命令（cwd = `D:/Project/acp-remote-wt/export-ids`） | 结果 | 日志 |
| --- | --- | --- | --- |
| [PV1] | `cargo test --locked -p core --all-features` | EXIT=0，121 passed / 0 failed | `reports/wp2-core.log` |
| [PV3] | `cargo test --locked -p storage-sqlite -p core --all-features` | EXIT=0，两 crate 全部 test binary 0 failed（storage 含 42 passed 的 `admin_store`、10 passed 的 `migration`） | `reports/wp1-storage.log` |
| [PV4]（WP3） | `cargo test --locked -p server -p app --all-features` | EXIT=0 | `reports/wp3-server.log` |
| [PV4]（WP4） | 同一条命令的独立第二次运行 | EXIT=0（含 `node_link_e2e` 受控路径 2 passed） | `reports/wp4-app.log` |
| LC-WP1..4 | `cargo fmt --all -- --check`；`cargo clippy --locked -p <crate> --all-targets --all-features -- -D warnings`；`cargo test --locked -p <crate> --all-features`（crate = core / storage-sqlite / server / app） | 12 个 EXIT=0，无失败、无零用例掩盖 | `reports/wp-local-checks.log` |
| 附加（非计划内 Check） | `cargo test --locked --workspace --all-features` | 0 failed（用于确认 `identity-auth` 等未改 crate 未被接口变更破坏） | 未单独留日志；[PV1]/[PV3]/[PV4] 三份日志覆盖其中四个受影响 crate，可复跑 |

环境：`cargo 1.98.1`、`rustc 1.98.1`（由 `rust-toolchain.toml` 固定）、Windows 本机、`CARGO_TARGET_DIR=D:/Project/acp-remote-wt/target-export-ids`
（独立构建目录，避免与主检出抢锁）。所有命令未使用任何跳过手段；没有删除或弱化既有断言（被改的断言都是版本号从 3 → 4 这类
「随合同推进」的更新，以及在既有用例里补 `exportIds` 参数）。

## 注释更新清单（父 Agent 补充要求 2）

1. `crates/server/src/local_admin/params.rs:732-747`：`NodePairConfirm` 的字段文档（新增 `export_ids` 说明：必填、可空数组、已去重字典序、
   校验位置）+ 函数文档从 `{ pairingId, grants }` 改为 `{ pairingId, grants, exportIds }`，并写明「必填由守护进程运行期强制，
   `envelope.schema.json` 只把 `params` 建模为通用对象」。`reject_unknown_fields` 的白名单同批改为 `["pairingId","grants","exportIds"]`
   （辅助函数自身的文档是通用描述，未改）。
2. `crates/server/src/local_admin/router.rs:814-819`：`node_pair_confirm` 的方法文档（必填、失败映射、校验落点 D4）。
3. `crates/server/src/local_admin/router.rs:843-845`：`with_granted_export_ids` 调用处的行内注释（为何清单在此附加）。
4. `crates/server/src/local_admin/test_support.rs:1761-1770`：内存信任替身 `approve_node` 的文档（新增 `granted_export_ids` 语义）。
5. 同 crate 内复述可见性口径的注释一并更新（避免与新合同矛盾）：`node_link/catalog.rs` 模块头、`node_link/command.rs` 模块头与
   `visible_sessions` 文档、`node_link/resource.rs` 的 `on_attach` 注释、`node_link/command/tests.rs` 的 [R66] 说明（原「D14」引用全部
   改为「D1 三条件 / `catalog::visible_exports` 是唯一判定点」）。
6. `crates/core/src/broker.rs`：Owner 侧判定文档（三条件、清单只收窄）。
7. `crates/app/src/cli.rs` / `cli/pairing.rs`：`--export-id` 的帮助文本、`confirm_params` 与 `empty_export_ids_notice` 文档。

## 父 Agent 提问的明确回答：`strings(params, "exportIds")` 缺字段时是否 `local.invalid_params`？

**是**。依据（`crates/server/src/local_admin/params.rs` 现有实现，未改）：

```
fn value(params, key) -> missing 时 Err(invalid_params("missing parameter `{key}`"))   // :59-64
fn array(params, key) -> value(params, key)? 后要求 Value::Array，否则 invalid_params  // :86-93
fn strings(params, key) -> array(params, key)? 后逐项要求字符串                          // :102-111
fn invalid_params(msg) = AdminError::new(LocalErrorCode::InvalidParams, msg)            // :40-42
LocalErrorCode::InvalidParams => "local.invalid_params"                                 // error.rs:54
```

即 `strings(params, "exportIds")?` 在字段缺失时经 `value` 直接返回 `local.invalid_params`（消息含 `missing parameter \`exportIds\``）。
**用例固定**：`crates/server/src/local_admin/router.rs::node_pair_confirm_requires_and_forwards_the_export_ids` 的第 ①/② 组
（缺字段、`null`、字符串、含非字符串项、空串 → 全部 `LocalErrorCode::InvalidParams`，且 `node_count() == 0`）。

## 迁移 v4 段的实现位置与用例名（父 Agent 补充要求 1）

- 守卫实现位置：`crates/storage-sqlite/src/migrate.rs` 的 `migrate()` 升级块（约 :968-995）：
  `if file_version < 2 { V2_OWNED; V2_IMPORTED }`、`if file_version < 3 { V3_OWNED; V3_IMPORTED }`、
  `if file_version < 4 && owned_node_pre_existing { V4_UPGRADE_OWNED }`。
  **同时修掉父 Agent 指出的隐患**：v3 段原来无条件执行（常量 = 3 时恰好正确），常量推进到 4 后会让 v3 库重跑 12-step 重建；
  现已加 `file_version < 3` 守卫（与 `< 2` 同体例）。
  额外的 `owned_node_pre_existing` 守卫（`migrate()` 在跑 DDL 常量**之前**用 `table_exists` 取值，与既有 `new_database` 同款）是实测必需：
  v1 库没有 `owned_node`，它由 `OWNED_SCHEMA_V1` 按**当前形状**（已含 `export_ids_json`）建出，此时再无条件 `ALTER TABLE ADD COLUMN`
  会以 `duplicate column name: export_ids_json` 失败（首次实现即撞上，四个既有迁移用例同时红）。
- 用例名：
  - `crates/storage-sqlite/tests/migration.rs::v3_database_upgrades_to_v4_by_appending_the_export_id_column_only` —— v3 → v4 只加列：
    既有节点行 → `'[]'` 且其余列逐字不变、两族列规格与新建库逐项相等（新增列在末尾、`NOT NULL`、默认 `'[]'`）、
    **除 `owned_node` 外每条表的 DDL 文本逐字节不变**、`user_version = 4`、版本键 4/3。
  - `crates/storage-sqlite/tests/migration.rs::rebuild_text_differs_from_the_fresh_text` —— 上条「DDL 逐字不变」判据的**判别力对照**：
    证明「同名审计表的 12-step 重建文本 ≠ 新建库文本」，因此该断言不是空断言。
  - `crates/storage-sqlite/tests/migration.rs::v1_fixture_upgrades_to_v3_and_preserves_rows`（既有，已扩到 owned 列清单）与
    `current_version_database_is_untouched_by_two_consecutive_starts`、`second_open_of_an_upgraded_database_rewrites_nothing`、
    `a_failed_upgrade_rolls_back_to_v1`、`v2_fixture_upgrades_to_v3_and_widens_only_the_audit_checks`（均随 v4 常量更新并通过）。

## 未执行项与原因

| 项 | 原因 |
| --- | --- |
| `npm run check` / `npm run verify`（[PV1] 的合同门禁部分：`check:docs`、`check:drift`、`check:agentic` 等） | 不在本角色范围（任务明确：文档由 WP5 改、漂移门禁在集成阶段跑）。**但需注意**：`check:contract-drift` 会逐条比对 `docs/CORE_PORTS_AND_STORAGE.md` §5/§7 与本 crate 实现，WP5 必须把 §7.3 的 `owned_node` DDL 写成与 `migrate.rs` 逐字一致（见「待澄清问题」③）。 |
| [PV5] 候选/主分支的受控路径轮次 | 由主 Agent 在 6.5/7.1 执行；本角色只在 [PV4] 内跑同一 `node_link_e2e` 用例（已通过）。 |
| 独立 review（RV1）与 E2E | 本角色不自任独立 review；未产出、也不引用他人的 review 结论。 |
| `docs/**`、`schemas/**`、`fixtures/**`、`compatibility/**` 同步 | 明确禁止改；`fixtures/storage/v2/`（v2/v1 历史夹具）因此保持原样，v3 形状的升级输入在测试里现场构造。 |

## 待澄清问题（交主 Agent 决策，本角色未自行决定）

1. **attach 的错误码措辞**：`plan.md` 的 [PV5] 行与 `design.md` D9 写「另一 Export 的 attach 被拒**为 `export.not_found`**」，
   但实现的既有语义是**所有不可见**（未知/未撤销/与 grants 不相交/不在清单内）统一回 `nodelink.export.not_granted`
   （先例：`crates/server/src/node_link/resource/tests.rs` 的 [R52]/[R53] 用例；`export.not_found` 专用于
   「会话不存在 / ownerNodeId 非本机」）。我按**保持既有语义**实现，e2e 里断言 `nodelink.export.not_granted`。
   若计划确实要改成 `export.not_found`，那是可见性错误码的行为变更（超出本变更已批准范围），需要主 Agent 先裁定并同步规格。
2. **`PairingSettlement` 的 API 形态**：`crates/identity-auth` 不在本任务写入范围，且其 `PairingDecision::Approve` 不携带 `exportIds`，
   因此我**没有**改 `PairingSettlement::approved(scopes, grants)` 的签名，而是新增
   `with_granted_export_ids(self, Vec<ExportId>) -> Self`（归一化；非批准结果原样返回），由 `local_admin::router` 在节点确认路径上附加。
   副作用：任何忘记附加的节点落定路径会得到空清单（**失败关闭**：该节点看不到任何 Export）。若主 Agent 认为应改为
   「`approved` 增参数 + 同批改 `identity-auth`」，需要扩大写入范围与依赖交接表。
3. **§7.3 DDL 的列位置**（供 WP5）：`export_ids_json` 位于 `owned_node` 列清单的**最后一项**，即 `revoke_reason` 之后、表级
   `PRIMARY KEY`/`CHECK` 之前（`ALTER TABLE ADD COLUMN` 只能追加在末尾，两侧必须同序才能通过「升级库 == 新建库」与漂移门禁）。
   文本：`  export_ids_json TEXT NOT NULL DEFAULT '[]',  -- LOCAL_ADMIN_PROTOCOL.md §5.4 的 exportIds[]`。
4. **CLI 警告的通道**：警告走 **stdout**（与配对提示同一通道），位置在「展示认领信息」与「确认」之间。原因是
   `crates/app/tests/support/mod.rs` 的 `failure_code` 断言 **stderr 恰好一行 JSON**（既有契约），把人类可读文本写到 stderr 会破坏它；
   放在落定前（而非 `run` 开头）是为了让 `node pair --mode access` 这类 `begin` 即失败的路径的 stdout 仍满足
   `assert_failure_contract` 的「失败时 stdout 只有一行」。若主 Agent 希望改成 stderr 或提前打印，需要同时调整该测试契约。
5. **`node list` 的 CLI 展示证据链**：CLI 对 `node list` 只做「逐字打印 Daemon 的 `result`」（`cli::print_result`），没有按字段渲染的代码；
   因此「展示 exportIds」由 **Daemon 侧 `view::node` + `router.rs` 的列表断言**证明，CLI 侧只证明参数/结果通路。若要求 CLI 侧
   独立的展示断言，需要新增按字段渲染的代码路径（当前不存在，属新范围）。

## 复验与失败记录

- 无未闭环失败：实施过程中出现过两类**已修复**的失败，均为自查发现并留痕（此处如实登记）：
  1. `duplicate column name: export_ids_json`（4 个既有迁移用例同时红）→ 根因：v1 库的 `owned_node` 由 DDL 常量按当前形状新建；
     修复：`owned_node_pre_existing` 守卫（见上）。
  2. `a_failed_upgrade_rolls_back_to_v1` 等用例的中间失败已随上面修复消失；另有一次
     `an_interactive_rejection...` 之外的 `app` 进程级用例因「stderr 恰好一行」失败 → 修复：警告改走 stdout 并移到落定前。
- 所有修复后均重跑了受影响 crate 的 fmt/clippy/test 与 [PV1]/[PV3]/[PV4]，日志即最终一轮。

## 资源释放与清理

- 构建目录：`D:/Project/acp-remote-wt/target-export-ids`（本 worktree 专用，保留给后续 reviewer 复跑，避免重复全量编译）。
- 临时目录：全部由测试自带的 `TempDir`/`TempRoot` 守卫在析构时删除；本次未手建额外临时目录（脚本写在系统 temp 下，可随时删除）。
- 端口：无固定端口占用，受控路径用 `127.0.0.1:0`。
- git：提交后 `git status --porcelain` 为空；未 push、未合并、未切分支、未触碰主检出。

---

# fix 轮次（RV1-IMPL-F4 / F5 / F6 + R9 覆盖缺口）

## 背景与范围

- 检视依据：`reports/rv1-impl.md`（RV1-IMPL，结论 PASS、无 CRITICAL、无 MAJOR）。本轮只处理主 Agent 裁定要修的四项：
  RV1-IMPL-F4（SUGGESTION）、RV1-IMPL-F5（SUGGESTION）、RV1-IMPL-F6（SUGGESTION，裁定「修成显式拒绝」）、
  R9 的「无效清单条目拒绝确认」**组合断言缺口**（RV1-IMPL §2 判为「覆盖（部分）」）。
- fix 起点：`5190924d041dcc7ab9bd93b4541b215f25f26bea`；fix 后 tip：**`e4c980c029a97966c3229ac067db9a061817b04d`**，由三个提交组成：

| 提交 | 信息 | 内容 |
| --- | --- | --- |
| `d5a33eb` | `fix(storage): 设备配对携带清单时显式拒绝` | RV1-IMPL-F6（存储层守卫 + 新用例） |
| `1304f16` | `fix(server): 节点撤销保留清单并修正收窄注释` | RV1-IMPL-F4（替身保真）、RV1-IMPL-F5（注释错别字） |
| `e4c980c` | `test(app): 补齐节点配对清单的组合断言` | R9 组合断言（新测试目标） |

- 写入范围：只改 `crates/**` 与 `reports/**`（本文件与 `reports/wp-fix-impl.log`）。未改 `docs/**`、`schemas/**`、
  `fixtures/**`、`compatibility/**`、`plan.md`/`tasks.md`/`verification.md`/`design.md`；未新增依赖
  （`git diff 5190924d e4c980c --name-only` 只含 5 个 `crates/**` 路径，`Cargo.toml`/`Cargo.lock` 零改动）。
- 运行路径没有新增 `unwrap`/`expect`/panic：新增的 `expect` 只在 `#[cfg(test)]` 目标里（测试断言与夹具），与既有体例一致。

## 四项落点

| 项 | 落点（文件:函数 / 用例名） | 提交 |
| --- | --- | --- |
| F4 | `crates/server/src/local_admin/test_support.rs` → `FakeTrust::revoke_node`：撤销记录由 `Vec::new()` 改为 `record.export_ids().to_vec()`（与 `advance_node_connected_at` 的既有写法同款），并加注释点明「撤销只改 `state`/`revoked_at`，**不**清理清单」；对称地在**真存储** `crates/storage-sqlite/src/admin/trust.rs::revoke_node` 的文档注释里也写明同一句（真实代码本来就只 UPDATE 三列，本项只是把口径写下来） | `1304f16`（真存储注释在 `d5a33eb`） |
| F5 | `crates/server/src/node_link/catalog.rs` → `visible_exports` 的模块文档注释：「清单只能收窄**不掉宽**」→「清单只能收窄**不放宽**」。**落点与任务描述的 `crates/core/src/model/identity.rs` 不同**：改为按 RV1-IMPL-F5 给出的位置（`catalog.rs:247-254`）；core 的 `identity.rs:308`、`broker.rs:658` 本来就是「不能放宽」「只收窄不放宽」。全仓库检索「掉」的其余用法（`grep -rn 掉 crates/`）均为「去掉 / 丢掉 / 删掉 / 摘掉」等正常词，**未发现同类错别字** | `1304f16` |
| F6 | `crates/storage-sqlite/src/admin/trust.rs` → `TrustStore::settle_pairing` 的 `PairingTarget::Device` 分支：在既有「`a device pairing must not carry grants`」之后加**同一处、同口径**的对称守卫——`granted_export_ids` 非空 → `PortError::InvalidRequest("a device pairing must not carry export ids")`。守卫位于事务内、任何写入之前，失败零写入零审计。用例：`crates/storage-sqlite/tests/admin_store.rs::a_device_pairing_must_not_carry_export_ids`（断言具名错误原因、不建信任行、不写身份材料、不推进配对状态（仍 `pending_confirmation`）、不写审计；并已跑负向对照：临时移除守卫 → 该用例 FAILED，还原后 ok） | `d5a33eb` |
| R9 | 新测试目标 `crates/app/tests/node_pair_export_ids.rs`（真实 `LocalAdminRouter` + 真实 `SqliteStore` + 真实 loopback claim）：<br>① `node_pair_confirm_rejects_an_unknown_export_id_without_writing` —— 清单里混入本机不存在的 `exportId` → 错误码 `local.not_found` 且消息具体指向那个 id；`node.list` 为空（不建信任）；`audit.export` 读出的 `node.paired` 记录数为 `0`，而同一份配对随后成功确认后为 `1`（说明该 `0` 是判别性断言而非筛掉了全部记录）；同一份配对仍能成功确认（配对状态未被推进）；<br>② `node_pair_confirm_rejects_an_export_id_disjoint_from_the_grants` —— `exportId` 存在且未撤销、只是 `scopes ∩ grants = ∅` → `local.invalid_params` 且消息含 `must intersect the granted grants`；`node.list` 为空、审计 `0`、失败后仍能成功确认（这补的是「与本次 grants 不相交」那条此前同样缺的组合断言） | `e4c980c` |

### R9 为什么落在 `crates/app/tests/**` 而不是 `crates/server`

任务原文优先要求放在 `crates/server` 的 local_admin 测试里（能走真存储就走真的，否则改用能覆盖真存储的目标）。
核对结论是**`crates/server` 不能走真 `SqliteStore`**：

- `server` 的 local_admin 用例接的端口由 `crates/server/src/local_admin/test_support.rs` 的 `FakeTrust` 提供，替身**不校验清单**
  （这正是 R9 缺口存在的原因：`FakeTrust::settle_pairing` 的节点分支只拒绝非空 `granted_scopes`）；
- `server` **不允许依赖 `storage-sqlite`**：`docs/MODULE_ARCHITECTURE.md` §5 依赖矩阵里 `server` 行、`storage-sqlite` 列为空；
  `scripts/check-crate-boundaries.mjs` 用 `cargo metadata` 把 `[dependencies]`/`[dev-dependencies]`/`[build-dependencies]` **一起**判，
  给 `server` 加 `storage-sqlite` dev-dependency 会让 `check:boundaries` 直接失败（且违反依赖方向：inbound adapter 不依赖 outbound adapter）。
- 组合根这一层同时有真 router 与真存储：`crates/app/tests/support/owner.rs::OwnerNode` 用同一个 `Arc<SqliteStore>` 装配所有端口，
  并通过 `LocalAdminRouter` 的 `node.pair.confirm`；`crates/app/tests/audit_export.rs` 的文件头已有「为什么放在 app」的同款先例。

因此新用例放在 `crates/app/tests/node_pair_export_ids.rs`（不改 `server` 的依赖、不改任何生产代码）。

## 检查与证据（本轮）

| 检查 | 命令 | 结果 |
| --- | --- | --- |
| Local（fmt） | `cargo fmt --all -- --check` | exit 0 |
| Local（clippy ×3） | `cargo clippy --locked -p server / -p storage-sqlite / -p app --all-targets --all-features -- -D warnings` | 各 exit 0 |
| 新用例（存储层） | `cargo test --locked -p storage-sqlite --test admin_store a_device_pairing_must_not_carry_export_ids` | `1 passed; 0 failed`（exit 0） |
| 新用例（组合层） | `cargo test --locked -p app --test node_pair_export_ids` | `2 passed; 0 failed`（exit 0） |
| 负向对照 | 临时移除 F6 守卫后重跑上面的存储层命令 | 该用例 FAILED（panic 在 `expect_err`），还原后 ok ⇒ 用例有判别力 |
| **[PV1]** | `cargo test --locked --workspace --all-features` | 提交前工作区 exit 0；`tip=e4c980c` 复跑 exit 0；两次均为 **86 个 `test result` 行、passed=992 / failed=0 / ignored=2**（fix 起点 `5190924d` 为 989，+3 = 本轮新增用例）。`ignored=2` 是 base 提交既有的两处 `#[ignore]` |

日志：`reports/wp-fix-impl.log`（含每条命令、退出码、逐目标 `test result` 汇总）。

补充事实（供复核者避免误判）：

- 日志「口径说明」第 2 条登记了一次**组装脚本自身**的失败（不是用例、也不是仓库脚手架）：组装脚本把日志路径写进了环境变量 `TMP`，
  而 `TMP`/`TEMP` 正是原生测试进程读取的临时目录，于是测试把日志文件当成临时根目录、在其下建 `acpr-*` 子目录，
  父路径是文件必然 `Os { code: 183, kind: AlreadyExists }`。改用别的变量名后所有命令逐次通过。
- F4 的替身改动不影响任何既有断言：`FakeTrust` 的既有 `node.revoke` 用例用的都是空清单，`export_ids().to_vec()` 与 `Vec::new()` 等值。
- 本轮没有新增/弱化跳过：`#[ignore]` 数量与 base 一致（2），也没有「不 panic 即通过」的用例（错误路径都断言了错误码/code 与消息关键词）。

## 未做项与理由

| 项 | 理由 |
| --- | --- |
| F6 在 `core` 侧的对称限制 | 核对后判定 core 侧**没有**可对齐的对称校验：`granted_grants` 的「设备配对不得携带」也只在存储层的同一分支里；`core::model::PairingSettlement::validate()` 不接收目标族（`PairingTarget`），`(target, settlement)` 的交叉校验在纯值对象里无处安放。`UseCases::settle_pairing` 虽然读得到配对行，但在那里再加一条会把同一条规则拆成两处（正是本次要对齐的反面）。因此按「同一处对称」的口径只落在 `storage-sqlite` 的 `settle_pairing`。 |
| 替身 `FakeTrust::settle_pairing` 的设备方向守卫 | 替身在**节点**方向本来就不校验清单（R9 缺口即由此而来），只补设备方向会造成「一半像、一半不像」的误导；本轮按指令只在真存储加规则，替身与真存储的这条差异保留为已知差异（[PV1] 全绿，且新用例已改用真存储）。 |
| RV1-IMPL-F1 / F2 / F3（三条 MINOR） | F1/F2 是 `plan.md`/`design.md` 的措辞（主 Agent 已自行修正，且 docs 属本次禁改范围）；F3 是既有口径下的已知限制，是否加机制由主 Agent 裁决。本轮不改。 |
| `npm run check`（合同门禁）/ [PV5] / 独立 review | 不在本角色范围（[PV1] 的合同门禁部分按裁定在集成树跑；[PV5] 由主 Agent 在候选/主分支执行）。 |

## handoff_index（fix 轮次）

```yaml
handoff_index:
  - task_id: "2.1"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "e4c980c029a97966c3229ac067db9a061817b04d"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/wp-fix-impl.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "fix 后 tip 上执行 cargo test --locked --workspace --all-features，exit 0（86 个 test result 行、passed=992、failed=0、ignored=2）。RV1-IMPL-F6 的落点在 storage-sqlite/src/admin/trust.rs 的 settle_pairing 设备分支（设备配对带非空 exportIds → InvalidRequest，失败零写入零审计），用例 admin_store.rs::a_device_pairing_must_not_carry_export_ids；R9 的组合断言也在同一轮里覆盖到存储侧校验（app/tests/node_pair_export_ids.rs 的两个用例都经真 SqliteStore）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.3"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "e4c980c029a97966c3229ac067db9a061817b04d"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/wp-fix-impl.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一轮 [PV1] 与 -p server / -p app 的 Local Checks 均 exit 0。RV1-IMPL-F4 的落点在 server/src/local_admin/test_support.rs 的 FakeTrust::revoke_node（撤销保留清单，与真存储同形）；RV1-IMPL-F5 的落点在 server/src/node_link/catalog.rs 的 visible_exports 注释；R9 缺的组合断言由 app/tests/node_pair_export_ids.rs 的两条用例补上（local.not_found / local.invalid_params，且不建信任、不写审计、不推进配对状态）。"
    source_evidence: NOT_APPLICABLE
```

## 返回给主 Agent 的摘要（fix 轮次）

- **新提交**：`e4c980c029a97966c3229ac067db9a061817b04d`（其中 `d5a33eb` = F6，`1304f16` = F4+F5，`e4c980c` = R9）。
- **四项落点**：F4 `server/src/local_admin/test_support.rs::FakeTrust::revoke_node`；F5 `server/src/node_link/catalog.rs:254`；
  F6 `storage-sqlite/src/admin/trust.rs::TrustStore::settle_pairing`（Device 分支）；
  R9 `crates/app/tests/node_pair_export_ids.rs::node_pair_confirm_rejects_an_unknown_export_id_without_writing` 与
  `::node_pair_confirm_rejects_an_export_id_disjoint_from_the_grants`。
- **[PV1]**：`cargo test --locked --workspace --all-features` exit 0；86 个 `test result` 行、passed=992 / failed=0 / ignored=2。
- **未做**：core 侧对称限制（core 无对称校验可对齐，且值对象层拿不到目标族）、替身仅在设备方向对齐不可行（见上）、
  F1/F2/F3（超出本次写入范围或属主 Agent 裁决）、`npm run check`/[PV5]/独立 review（非本角色范围）。


---

# fix 轮次 2（RV2-IMPL-F1：重新配对收窄清单后未作废既有连接）

## 背景与范围

- 检视依据：`reports/rv2-impl.md`（RV2-IMPL-F1）；主 Agent 已在 `e4c980c0` 上实测确认可达。
- 缺陷形状：`node.pair.confirm` 在节点分支落定后**不关闭**该节点的既有活动连接；存储的 `approve_node`
  对「同一 `nodeId` 已有 `paired` 行」没有状态守卫，因此同一节点可在**不 revoke** 的情况下重新配对并把
  `exportIds` 清单收窄（甚至收到空）。实时扇出（`server::node_link::resource::fan_out` →
  `core.node_link_event_payload`）不判定 `exportIds`，于是早已建立的 attachment 继续收到该 Export 的
  `resource.event`——与「空清单 = 无可见、不得默认放权」（方案 A）和 `node.revoke` 的既有语义都不一致。
- fix 起点：`e4c980c029a97966c3229ac067db9a061817b04d`；fix 后 tip：**`8ae3b6135e549d7f1dac7750fb6f46deb6dc2989`**，两个提交：

| 提交 | 信息 | 内容 |
| --- | --- | --- |
| `5d768d8` | `fix(server): 节点重新配对提交后关闭既有连接` | `node_pair_confirm` 在信任行提交成功后调用与 `node.revoke` **同一个** `self.deps.pairing.close_node(&node_id)`；同文件两处既有 closer 断言一并修正 |
| `8ae3b61` | `test(app): 覆盖重新配对收窄清单后的连接作废` | 组合根新用例 `a_narrowing_repair_closes_the_live_attachment`（真实 router + 真实 SQLite + 真实 loopback listener） |

- 写入范围：只改 `crates/server/src/local_admin/router.rs`、`crates/app/tests/node_link_e2e.rs` 与
  `reports/**`（本文件与 `reports/wp-fix-close.log`）。未改 `docs/**`（同批另一个 Agent 负责）、`schemas/**`、
  `fixtures/**`、`compatibility/**`、`plan.md`/`tasks.md`/`verification.md`/`design.md`；未新增依赖
  （`git diff e4c980c..HEAD --name-only` 只有这 2 个 `crates/**` 路径，`Cargo.toml`/`Cargo.lock` 零改动）。
- 按任务裁定**没有**改 core 的 `node_link_session_access`，也**没有**在 fan-out 里加清单判定：可见性的唯一
  判定点仍是适配器（`node_link::catalog`）与 core 的 Owner 侧命令授权；本次只把授权变化落到**连接边界**。

## 落点与语义

- `crates/server/src/local_admin/router.rs::LocalAdminRouter::node_pair_confirm`：在 `settle_pairing` 成功、
  已取到 `TrustRecordRef::Node(node_id)`、`approved_at` 读回之后、`Ok(..)` 返回之前新增
  `self.deps.pairing.close_node(&node_id).await;`（提交 → 关闭 → 返回，与 `node.revoke` 同一顺序）。
- 为什么无条件调用（而不是「仅当已配对」）：首次配对在生产里恒是 no-op——配对行落定前不存在已配对的
  信任行，握手无法通过，因此没有可关闭的活动连接（这一点由 `node_owner_pairing_flow_pairs_a_node_with_initial_grants`
  的可观察点与「回读值为 `false`（该行未撤销）」共同断言）；而无条件调用还顺带覆盖「已撤销后又重新配对」
  的同一条路径，不需要再引入一个需要维护的「曾配对过」判据。
- 该节点并未撤销，所以 `CommandRoute::node_revoked` 读不到 `revokedAt`，只关连接（4410）、不推
  `node.trust.revoked`——这是既有分支，未改动。

## 检查与证据（本轮）

| 检查 | 命令 | 结果 |
| --- | --- | --- |
| Local（fmt） | `cargo fmt --all -- --check` | exit 0 |
| Local（clippy） | `cargo clippy --locked -p server -p app --all-targets --all-features -- -D warnings` | exit 0 |
| 新用例 | `cargo test --locked -p app --test node_link_e2e --all-features a_narrowing_repair` | `1 passed; 0 failed`（exit 0） |
| 既有断言修正后复跑 | `cargo test --locked -p server --all-features --lib -- node_owner_pairing_flow node_revoke_retry` | `2 passed; 0 failed`（exit 0） |
| 负向对照 | 把那一行 `close_node` 换成注释后重跑上面两条命令 | 新用例 FAILED（`Elapsed(())`，10 s 内连接未被关闭）、两条 router 用例 FAILED（`left: []`）；`git checkout --` 还原后两条命令均回到 ok ⇒ 用例与断言都有判别力 |
| **[PV1]** | `cargo test --locked --workspace --all-features` | `tip=8ae3b61` 上 exit 0；**86 个 `test result` 行、passed=993 / failed=0 / ignored=2**（fix 起点 992，+1 = 本轮新增的那条 app 用例） |

日志：`reports/wp-fix-close.log`（每条命令 + 退出码 + 逐目标 `test result` 汇总 + 负向对照的完整前后输出）。

### 既有 `node.pair.confirm` 相关断言的修正（任务要求一并检查）

两处断言的前提是「确认不调 closer」，已在 `5d768d8` 里一次性修正，语义没有被削弱：

- `node_revoke_retry_and_unknown_id_report_not_found`：先断言「确认本身也关一次、且当时的行未撤销」
  （`closed_nodes() == [NODE_ID]`、`nodes_revoked_when_notified() == [false]`）；撤销后改为
  `vec![NODE_ID; 2]` 与 `[false, true]`（只有撤销那次发生在撤销提交之后）；「重试不再关闭」「未知 id 不关闭」
  原样保留，改成按次数断言（2）。
- `node_owner_pairing_flow_pairs_a_node_with_initial_grants`：确认后先断言一次（`[NODE_ID]` / `[false]`），
  撤销后改为 `vec![NODE_ID; 2]`。
- 其余 `NodePairConfirm` 用例（缺字段/null/非字符串/空串/超集/未认领/已拒绝、以及 `unimplemented_methods_*`
  的参数错误枚举）都只走**失败**路径或断言与 closer 无关的字段，因此没有变化；`closed_nodes().is_empty()`
  那条仍成立（该用例里没有任何成功的节点确认）。

## 未做项与理由

| 项 | 理由 |
| --- | --- |
| core 读 seam 复制条件③ / fan-out 加清单判定 | 任务明确禁止：会造出第二个可见性判定点，且 `node.exportIds` 本来就没有运行期修改入口；本修复把变化落在连接边界（强制重新握手，由新连接按已提交的信任行重算） |
| `device.pair.confirm` 的对称关闭 | 不在本任务范围（RV2-IMPL-F1 只指节点清单；`server::sync` 未落地，设备连接面属切片 7） |
| 为重新配对单独定义 close code / 推送 | 任务要求复用 `node.revoke` 的同一机制；新增 close code 会改 wire 语义（`NODE_LINK_PROTOCOL.md` 的封闭词表），超出「修一个可达缺口」的范围 |
| `npm run check` / `npm run verify` | 本 worktree 无 `node_modules`（`npm ci` 需网络）；本轮不触及合同资产（`git diff --name-only e4c980c..HEAD -- docs schemas fixtures compatibility openspec Cargo.toml Cargo.lock` = 0） |
| [PV5] / 独立 review / 合并 | 非本角色范围（由主 Agent 在集成树执行） |

## handoff_index（fix 轮次 2）

```yaml
handoff_index:
  - task_id: "2.3"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "8ae3b6135e549d7f1dac7750fb6f46deb6dc2989"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/wp-fix-close.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "RV2-IMPL-F1 的修复轮次：server/src/local_admin/router.rs 的 node_pair_confirm 在信任行提交成功后按 node.revoke 的同一机制 close_node，同轮修正该文件两处既有 closer 断言；app/tests/node_link_e2e.rs 新增 a_narrowing_repair_closes_the_live_attachment（配对 + attach + 收到正文事件 → 空清单重新配对同一节点 → 既有连接以 4410 关闭、新连接按空清单重算），并做「临时移除该调用 → 新用例 FAILED(Elapsed) 与两条 router 用例 FAILED → 还原后 ok」的负向对照。tip 上执行 cargo test --locked --workspace --all-features，exit 0（86 个 test result 行、passed=993、failed=0、ignored=2；起点 e4c980c 为 992）；同一 tip 的 cargo fmt --all -- --check 与 cargo clippy --locked -p server -p app --all-targets --all-features -- -D warnings 均 exit 0。"
    source_evidence: NOT_APPLICABLE
```

## 返回给主 Agent 的摘要（fix 轮次 2）

- **新提交**：`8ae3b6135e549d7f1dac7750fb6f46deb6dc2989`（`5d768d8` = 实现修复，`8ae3b61` = 组合层回归用例）。
- **改动位置**：`crates/server/src/local_admin/router.rs`（`node_pair_confirm` 调 `close_node`，两处既有断言修正）；
  `crates/app/tests/node_link_e2e.rs`（新用例 + `begin_and_claim`/`Chain::repair_and_confirm` 抽取）。
- **用例名**：`a_narrowing_repair_closes_the_live_attachment`；负向对照：删掉该行 → 该用例
  `FAILED ... Elapsed(())`、两条 router 用例 `FAILED [left: []]`，还原后全 ok。
- **[PV1]**：`cargo test --locked --workspace --all-features` exit 0；86 个 `test result` 行、passed=993 / failed=0 / ignored=2。
- **未做**：core 读 seam / fan-out 判定（任务禁止）、device 侧对称关闭、新 close code、`npm run check`（无
  `node_modules` 且本轮不触合同资产）、[PV5]/review/合并（非本角色范围）。

---

# Handoff — 实现 Agent（coder）：fix 轮次 3（重新配对不得复用撤销关闭语义）

## 背景（主 Agent 已实测确认的缺陷）

上一轮（`8ae3b613`）我自己列出的残余风险成立：`node.pair.confirm` 复用了 `deps.pairing.close_node`，
而该端口在组合根里转调 `CommandRoute::node_revoked` —— **撤销语义**。由于重新配对时该行并未撤销，
`node_revoked` 读不到 `revokedAt`，落到「no persisted revokedAt … closed without the notification」分支：
不发消息，但关闭码仍是 `4410`（「节点已撤销」）、reason 仍是 `"the node trust was revoked"`。

后果有两层，都不是风格问题：

1. **协议语义自相矛盾**：合规客户端把 `4410` 与 `node.trust.revoked` 都读成「停止重连」
   （`NODE_LINK_PROTOCOL.md` §15「撤销即停」、§14.2 的 4410）。于是「该节点应重连并按新清单重取
   catalog」被读成「该节点已被撤销、不再重连」，与 §8.2 的「收窄自下一次握手后的新连接生效」直接冲突。
2. **话术事实错误**：`reason` 字符串写「信任已被撤销」，而信任并未撤销；日志与运维可见。

## 落点与语义（方案由主 Agent 给定，未自行另设计）

| 层 | 位置 | 内容 |
| --- | --- | --- |
| 端口 | `crates/server/src/local_admin/pairing.rs` | 新增 `ConnectionCloser::close_node_after_reauth(&NodeId)`（`close_node` 保留给撤销；`NoConnections` 补空实现，体例与既有两条一致）；`PairingSessions` 增同名转发方法 |
| 路由 | `crates/server/src/local_admin/router.rs` | `node_pair_confirm` 在信任行提交成功、`approved_at` 读回之后改调 `self.deps.pairing.close_node_after_reauth(&node_id).await`（仍是「提交 → 关闭 → 返回」的同序，只是不复用撤销语义） |
| Node Link | `crates/server/src/node_link/command.rs` | 新增 `CommandRoute::node_reauth(&NodeId) -> usize`：与 `node_revoked` 同址同序（`handles_for_node` → `request_close` → `forget_connection`），但**不推** `node.trust.revoked`、以 `close::NORMAL`（`1000`，常量已存在，未新增）与 reason `"the node trust was re-confirmed; reconnect to fetch the updated catalog"` 关闭；返回被关闭的连接数，日志事件 `node_link.node_reauth_invalidated_connections` |
| 组合根 | `crates/app/src/compose.rs` | `NodeLinkCloser` 实现 `close_node_after_reauth` → 转调 `command.node_reauth(node)`，按既有体例记 `tracing::info!`（事件 `daemon.reauth_connection_sweep`，语义写明「重新配对：已关闭既有连接，等待对端重连取新 catalog」） |

配套的注释/文档串（`§15` 而非上一版草稿里的 `§13`；reason 与 `ee1ca277` 冻结串逐字一致）：

- 为什么客户端读法关键：`NODE_LINK_PROTOCOL.md` §15 的「撤销即停」只由「收到 `node.trust.revoked`」或
  「本地 `node.revoke` 提交」触发，**不**包括服务端因其它原因主动关闭；§14.2 的表注明确「服务端因授权变化
  主动结束连接时用 `1000` 加 close reason，不用 `4410`」。`1000` 属同一冻结 close code 集合，wire 未变。
- 未改 `docs/**`（文档由另一 Agent 同批补），未改 core 读 seam，未新增依赖，未顺手重构。

## 用例改动

- `crates/app/tests/node_link_e2e.rs::a_narrowing_repair_closes_the_live_attachment`：
  关闭码断言 `4410 → 1000`；**新增**「全程未收到 `node.trust.revoked`」的断言——`close_code()` 会把 close
  帧之前的文本帧收进待取队列，撤销路径是「先 send 再 close、会话排空已入队消息后才发 close 帧」，
  因此「队列里没有该类型消息」就是「对端在关闭前没收到撤销推送」的可观察等价断言；重连后
  `catalog.snapshot` 为空这条原样保留。用例文档注释同步改写（负向对照改为「删掉 `node_reauth` 的调用」）。
- `crates/server/src/local_admin/test_support.rs::RecordingCloser`：新增
  `nodes_after_reauth`/`closed_nodes_after_reauth()`，与撤销的 `nodes`/`closed_nodes()` **分表**记录
  （两类关闭因此可区分，「撤销不得复用重新配对的关闭路径」变成可断言的事实）。
- `crates/server/src/local_admin/router.rs` 的两处 closer 断言改为断言**新端口方法**：
  `node_revoke_retry_and_unknown_id_report_not_found` 与
  `node_owner_pairing_flow_pairs_a_node_with_initial_grants` 在确认后断
  `closed_nodes_after_reauth() == [NODE_ID]` **且** `closed_nodes().is_empty()`；撤销路径的计数断言回到
  「撤销关闭 1 次」「重试不再关闭」「未知 id 不关闭」，`nodes_revoked_when_notified()` 回到 `[true]`
  （确认那次不再进撤销表，因此不再有 `[false, true]` 这种混合序列）。
- 新增 `crates/server/src/node_link/command/tests.rs::node_reauth_closes_with_1000_and_no_revocation_notification`：
  与 `node_revocation_notifies_then_closes_with_4410` 成对——断言返回 1、`close_request() == Some((1000, <冻结
  reason>))`、不推 `node.trust.revoked`、观察表随连接消失。

## 检查与证据（本轮，全部在新 tip 上执行）

| 检查 | 命令 | 结果 |
| --- | --- | --- |
| Local（fmt） | `cargo fmt --all -- --check` | exit 0 |
| Local（clippy） | `cargo clippy --locked -p server -p app --all-targets --all-features -- -D warnings` | exit 0 |
| 单元用例 | `cargo test --locked -p server --all-features --lib node_re` | `4 passed; 0 failed`（含新增的 `node_reauth_*`） |
| e2e 用例 | `cargo test --locked -p app --all-features --test node_link_e2e` | `3 passed; 0 failed` |
| 负向对照 1 | 注释掉 `node_pair_confirm` 里的 `close_node_after_reauth` 调用 → 重跑该 e2e 用例 | **FAILED**：`Elapsed(())`（10 s 内连接未被关闭）⇒ 用例对「确认必须落到连接边界」有判别力 |
| 负向对照 2 | 把该调用临时换回 `close_node`（撤销路径）→ 重跑同一用例 | **FAILED**：`left: 4410` / `right: 1000`（断言文案：重新配对不是撤销）⇒ 用例对「1000 vs 4410」有判别力；也复现了原缺陷 |
| 负向对照 3 | 临时让 `node_reauth` 在关闭前先推一条 `node.trust.revoked`（仍以 1000 关闭）→ 重跑同一用例 | **FAILED**：失败点在新增的「未收到 `node.trust.revoked`」断言（打印出那条消息的完整载荷）而非 close code 断言 ⇒ 该断言不是空断言，`close_code()` 之前的文本帧确实进了待取队列 |
| 还原 | `git checkout --` 还原 router.rs / command.rs 后复跑新用例与该单元用例 | 全绿（server lib `4 passed`、e2e `3 passed`）；`git status --short` 干净 |
| **[PV1]** | `cargo test --locked --workspace --all-features` | exit 0；**86 个 `test result` 行、passed=994 / failed=0 / ignored=2**（起点 `8ae3b613` 为 993，+1 = 本轮新增的 `node_reauth_*` 单元用例；app 侧只改断言、未增删用例） |

日志：`reports/wp-fix-reauth.log`（命令 + 退出码 + 逐目标 `test result` 行 + 三次负向对照的原始输出），
`reports/wp-fix-reauth-full.log`（[PV1] 完整输出副本）。

## 未做项与理由

| 项 | 理由 |
| --- | --- |
| `docs/**` | 任务明确要求不改（文档另派 Agent 同批补 §8.2/§14.2 表述）；本轮只对齐已被文档冻结的串（`§15`、1000、reason 逐字一致） |
| core 读 seam / fan-out | 任务禁止：会造出第二个可见性判定点 |
| 新增 close code 常量 | `close::NORMAL = 1000` 已存在（§14.2 同一冻结集合），无需新增 |
| 参数化 `node_revoked` | 任务明确要求「不要把撤销路径参数化到难懂」，因此新增同址的独立入口 |
| `npm run check` / `npm run verify` | 本 worktree 无 `node_modules`（`npm ci` 需网络）；本轮未触任何合同资产（改动只在 `crates/**` 与 reports） |
| [PV5] / 独立 review / 合并 / 归档 | 非本角色范围 |

## handoff_index（fix 轮次 3）

```yaml
handoff_index:
  - task_id: "2.3"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "043b359e60d546316013471ecb7ac31f914f7e30"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/wp-fix-reauth.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "修复「重新配对不得复用撤销关闭语义」：node.pair.confirm 原经 ConnectionCloser::close_node 走 CommandRoute::node_revoked，以 4410 与 'the node trust was revoked' 关闭，被合规客户端读成「撤销即停」（NODE_LINK_PROTOCOL.md §15），与 §8.2「收窄自下一次握手后的新连接生效」冲突。改为新增专路径 close_node_after_reauth → CommandRoute::node_reauth（不推 node.trust.revoked、以 1000 与冻结 reason 'the node trust was re-confirmed; reconnect to fetch the updated catalog' 关闭），并在 a_narrowing_repair_closes_the_live_attachment 断言 1000 + 未收到 node.trust.revoked（重连后 catalog 为空保留），RecordingCloser 把两类关闭分表记录。tip 上执行 cargo test --locked --workspace --all-features，exit 0（86 个 test result 行、passed=994、failed=0、ignored=2；起点 8ae3b613 为 993）；同一 tip 的 cargo fmt --all -- --check 与 cargo clippy --locked -p server -p app --all-targets --all-features -- -D warnings 均 exit 0。负向对照三次：注释掉新调用 → 该用例 FAILED(Elapsed)；换回 close_node → FAILED(4410 != 1000)；临时先推一条 node.trust.revoked → FAILED 且失败点正是「未收到 node.trust.revoked」断言（该断言因此不是空断言）；三次还原后均全绿。"
    source_evidence: NOT_APPLICABLE
```

## 返回给主 Agent 的摘要（fix 轮次 3）

- **新提交**：`043b359e60d546316013471ecb7ac31f914f7e30`（`fix(server): 重新配对不得复用撤销关闭语义`）。
- **新端口方法与调用链**：`LocalAdminRouter::node_pair_confirm` → `PairingSessions::close_node_after_reauth`
  → `ConnectionCloser::close_node_after_reauth` →（组合根 `NodeLinkCloser`）`CommandRoute::node_reauth`：
  不推 `node.trust.revoked`、`close::NORMAL`(1000) + reason
  `"the node trust was re-confirmed; reconnect to fetch the updated catalog"`，返回关闭的连接数并记
  `daemon.reauth_connection_sweep` / `node_link.node_reauth_invalidated_connections`。`close_node` /
  `node_revoked`（撤销：推送 + 4410）完全未动。
- **用例改动**：e2e 断言 4410 → 1000，并新增「未收到 `node.trust.revoked`」断言（重连后 catalog 为空保留）；
  `RecordingCloser` 分表记录两类关闭，router 两处确认断言改断新方法、撤销计数回到 1 次；新增 node_link
  单元用例 `node_reauth_closes_with_1000_and_no_revocation_notification`。
- **负向对照**：注释掉新调用 → `FAILED ... Elapsed(())`；换回 `close_node` → `FAILED left: 4410 / right: 1000`；
  临时在 `node_reauth` 里先推一条 `node.trust.revoked` → `FAILED` 且失败点正是新增的「未收到
  `node.trust.revoked`」断言。三次都 `git checkout --` 还原后全绿，工作区干净。
- **[PV1]**：`cargo test --locked --workspace --all-features` exit 0；86 个 `test result` 行、passed=994 /
  failed=0 / ignored=2（起点 993）。fmt / clippy 均 exit 0。
- **未做**：`docs/**`（另派）、core 读 seam、新 close code（1000 已存在）、`node_revoked` 参数化、`npm run check`
  （无 `node_modules` 且未触合同资产）、[PV5]/review/合并。

# Handoff — 实现 Agent（coder）：fix 轮次 4（注释级：RV3-IMPL-F1 / RV3-IMPL-F2 代码侧）

## 范围与纪律

本轮**只改注释与文档串**：不改任何逻辑、测试断言或 `docs/**`。改动落在两个文件的 `///` 注释内，
`git diff` 无一行可执行代码变化（`2 files changed, 8 insertions(+), 3 deletions(-)`，增删行全在注释里）。

新提交：`cc6faefd27d7d58a01945971c0cc48bcafd08501`（`chore(server): 修正节点配对关闭语义与 revoked 消息章节的注释`，
`git commit --no-verify`；worktree 无 `node_modules`，本地跑不了 commitlint，但类型 `chore` 与 scope `server`
都在 `commitlint.config.mjs` 的词表内）。

## RV3-IMPL-F1：`node_pair_confirm` 文档注释自相矛盾

文件：`crates/server/src/local_admin/router.rs`（`node_pair_confirm` 文档注释，原文 816–817 行）

| | 内容 |
| --- | --- |
| 原句 | 「信任行提交成功后关闭该节点的现有活动连接（**与 `node.revoke` 同一机制**），强制重新握手后按新清单重算——同一节点重新配对并收窄清单时，既有 attachment 不得继续收到该 Export 的事件。」 |
| 新句 | 「信任行提交成功后关闭该节点的现有活动连接，强制重新握手后按新清单重算——同一节点重新配对并收窄清单时，既有 attachment 不得继续收到该 Export 的事件。」另起一段：「`node.revoke` 与节点方向的 `node.pair.confirm` 都是**提交后关连接**，但两条路径的**语义与关闭码不同**：撤销推 `node.trust.revoked` 并以 `4410` 关闭并停止重连；重新配对**不**推消息、以 `1000`（正常关闭）+ close reason 关闭，要求对端重连以重取 catalog（`NODE_LINK_PROTOCOL.md` §8.2/§14.2/§15）。」 |

为什么改：原句「同一机制」与同函数体内 882–884 行的新注释、`crates/server/src/local_admin/pairing.rs`
的 `ConnectionCloser` trait 文档、`crates/app/src/compose.rs` 的 `NodeLinkCloser` 文档直接冲突；而「机制」
一词在实现语境里会被读成「复用撤销路径」，正是 RV2-IMPL-F1 修掉的缺陷。新表述保留原句里仍然正确的事实
（确认成功后确实关闭既有连接、强制重新握手），把两条路径的差异补齐。

## RV3-IMPL-F2（代码侧）：`§12.6` → `§12.3` 逐处清单

判定依据（`docs/NODE_LINK_PROTOCOL.md`，本 worktree revision）：§12.1 家族总览（469 行）把
`export.revoked`、`node.trust.revoked` 列在 **Catalog 族**；§12.3 Catalog 消息（501 行起）逐条登记这两条
（`node.trust.revoked` 在 509 行）；§12.6 控制与错误消息（585 行）只登记
`link.ping`/`link.pong`/`link.error`/`link.backpressure`。

**改动（1 处）**

| 文件:行 | 消息 | 原 | 新 |
| --- | --- | --- | --- |
| `crates/server/src/local_admin/pairing.rs:64`（`ConnectionCloser::export_revoked` 文档） | `export.revoked` | `NODE_LINK_PROTOCOL.md` **§12.6** 的 `export.revoked` | `NODE_LINK_PROTOCOL.md` **§12.3** 的 `export.revoked` |

**保留不动（5 处，均真实指向控制/错误消息或 `link.error` body）**

| 文件:行 | 指向 | 保留理由 |
| --- | --- | --- |
| `crates/node-link-protocol/src/error.rs:2` | `link.error`/`link.ping`/`link.pong`/`link.backpressure` 模块文档 | §12.6 正确 |
| `crates/node-link-protocol/src/error.rs:278` | `link.backpressure` body | §12.6 正确 |
| `crates/node-link-protocol/src/pairing.rs:22` | `§13.4 与 §12.6`（配对 HTTP error 与 `link.error` body 同形状） | 指向 `link.error`，§12.6 正确 |
| `crates/server/src/node_link/conn/session.rs:980` | `link.pong` 必须原样回填 nonce | §12.6 正确 |
| `crates/server/src/node_link/conn/session.rs:1014` | 同上（nonce 原样回填） | §12.6 正确 |
| `crates/server/src/node_link/conn/session.rs:1334` | `link.ping` 的 body | §12.6 正确 |

判定口径：`grep -rn "12\.6" --include=*.rs crates` 的全部命中即上表 6 处；其中只有
`crates/server/src/local_admin/pairing.rs:64` 指向 `node.trust.revoked`/`export.revoked` 家族，**`server` crate
内没有第二处同类错引**（任务提到的「本 crate 内其它同类处」经全量核对不存在）。

**顺带发现的同类错引（不在本轮范围）**：`docs/NODE_LINK_PROTOCOL.md` §15「撤销即停」一行（本 worktree 916 行）
仍写「收到 `node.trust.revoked`（**§12.6**）」——同一类错引在文档侧尚未修；按任务「不改 `docs/**`」未动，
请主 Agent 分派给文档 Agent。

## 检查与证据（本轮）

| 检查 | 命令 | 结果 |
| --- | --- | --- |
| Local（fmt） | `cargo fmt --all -- --check` | exit 0 |
| Local（clippy） | `cargo clippy --locked -p server --all-targets --all-features -- -D warnings` | exit 0 |
| **[PV1]** | `cargo test --locked --workspace --all-features` | exit 0；**86 个 `test result` 行全部 ok；passed=994 / failed=0 / ignored=2**（与 fix 轮次 3 逐项一致，注释改动未影响任何用例；2 个 ignored 是既有的手动夹具/辅助用例：`storage` 的 `crash_child` 与 `regenerate_v1_fixture`） |
| 已知 flaky | `daemon_lifecycle.rs::the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown` | **本轮通过**（`wp-fix-notes.log:351` 为 `... ok`），未触发，因此没有第二次重跑；未做任何跳过或掩盖 |

日志：`reports/wp-fix-notes.log`（[PV1] 完整输出：命令、目标清单、逐目标 `test result` 行）。

## 未做项与理由

| 项 | 理由 |
| --- | --- |
| `docs/**` | 任务明确禁止；文档侧同类错引（§15 的 `（§12.6）`）已在上文登记 |
| 任何逻辑、测试断言 | 本轮范围只有注释，`git diff` 全部落在 `///` 行内 |
| `npm run check` / `npm run verify` | worktree 无 `node_modules`（`npm ci` 需网络）；本轮未触合同资产（`compatibility/**`、`schemas/**`、`fixtures/**`、`docs/**` 未改），也没有可执行代码变化 |
| [PV5] / 独立 review / 合并 / 归档 | 非本角色范围 |

## handoff_index（fix 轮次 4）

```yaml
handoff_index:
  - task_id: "2.3"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "cc6faefd27d7d58a01945971c0cc48bcafd08501"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/wp-fix-notes.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "注释级修复独立复核 RV3 的两条 MINOR：RV3-IMPL-F1（crates/server/src/local_admin/router.rs 的 node_pair_confirm 文档注释原称与 node.revoke「同一机制」，与同函数内的新注释、pairing.rs 的 ConnectionCloser 文档和 app/src/compose.rs 的 NodeLinkCloser 文档矛盾；改为「同为提交后关连接、但语义与关闭码不同：撤销推 node.trust.revoked + 4410 并停止重连；重新配对不推消息、以 1000 + close reason 关闭并要求对端重连重取 catalog」，引 NODE_LINK_PROTOCOL.md §8.2/§14.2/§15）；RV3-IMPL-F2 代码侧（server crate 内唯一把 export.revoked 标为 §12.6 的 crates/server/src/local_admin/pairing.rs:64 改为 §12.3；其余 5 处 §12.6 均真实指向 link.ping/pong/error/backpressure，逐处核对后保留）。不改变任何行为：git diff 仅注释行（2 files、8 insertions、3 deletions）。同一 tip 上 [PV1] cargo test --locked --workspace --all-features exit 0（86 个 test result 行全部 ok、passed=994、failed=0、ignored=2），cargo fmt --all -- --check 与 cargo clippy --locked -p server --all-targets --all-features -- -D warnings 均 exit 0；已知 flaky 的 daemon_lifecycle::the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown 本轮通过，未跳过或掩盖。"
    source_evidence: NOT_APPLICABLE
```

## 返回给主 Agent 的摘要（fix 轮次 4）

- **新提交**：`cc6faefd27d7d58a01945971c0cc48bcafd08501`（`chore(server): 修正节点配对关闭语义与 revoked 消息章节的注释`）。
- **F1**：`router.rs` 的 `node_pair_confirm` 文档注释删去「与 `node.revoke` 同一机制」，改为写明「都是提交后关连接、
  但语义与关闭码不同」（撤销：推 `node.trust.revoked` + 4410 + 停止重连；重新配对：不推消息 + 1000 + close reason +
  要求重连重取 catalog，引 `NODE_LINK_PROTOCOL.md` §8.2/§14.2/§15）。
- **F2（代码侧）**：只改 `crates/server/src/local_admin/pairing.rs:64`（`export.revoked` 的 `§12.6` → `§12.3`）；
  `server` crate 内再无同类错引；另 5 处 `§12.6` 属控制族消息，逐处保留并在上表列出。
- **顺带**：`docs/NODE_LINK_PROTOCOL.md` §15「撤销即停」仍写 `（§12.6）`，属文档侧同类错引，按要求未动，请分派给文档 Agent。
- **[PV1]**：exit 0，86 个 `test result` 行、passed=994 / failed=0 / ignored=2（与上一轮同数）；已知 flaky 本轮通过，
  未触发第二次重跑。
- **未做**：`docs/**`、任何逻辑/断言、`npm run check`（无 `node_modules` 且未触合同资产）、[PV5]/review/合并/归档。
