# rv1-wp4.md — WP4（`storage-sqlite` v2 DDL、v1→v2 迁移与 v2 夹具）独立对抗性 review

## 0. 检视对象、范围与方法

- 仓库 / 分支：`D:\Project\acp-remote`，`feat/admin-state-persistence-v2`。
- 被检视固定版本：`git rev-parse HEAD` = `5404610d35b22c9d427b88b0cea2c61d1e7a5e15`（assignment 指定的 `5404610`）。
- 基线（变更起点）：`28f8cb9`；提交链 `28f8cb9 → 013f2b9（W0 基线）→ 1baea5b → f43f7a7 → 5404610`。
- 工作区状态：`git status --porcelain` 只有宿主安装的未跟踪项（`?? .omp/`、`?? .pi/prompts/opsx-verify.md`、`?? .pi/skills/openspec-verify-change/`、`?? reports/parked-idle-identity-retention.patch`），没有被跟踪文件的未提交改动；本轮唯一写入是本报告。
- 检视范围（WP4 切片）：
  - `crates/storage-sqlite/src/migrate.rs`（版本常量、`OWNED_SCHEMA_V1`/`IMPORTED_SCHEMA_V1`、`V2_UPGRADE_OWNED`/`V2_UPGRADE_IMPORTED`、`migrate()` 与四个新自由函数）
  - `crates/storage-sqlite/src/session_store.rs` 的迁移连带修改（`list_sessions` 的 imports 过滤、`drop_import`、`import_matches`）
  - `fixtures/storage/v2/{empty,from-v1,too-new}.sqlite3` 与 `fixtures/storage/v1/**`（历史资产是否被动过）
  - `crates/storage-sqlite/tests/{migration,enum_coverage,retention,imported,commit}.rs`、`crates/storage-sqlite/tests/support/mod.rs`
- 范围外（按 assignment）：`crates/storage-sqlite/src/admin/**`（已由 `reports/rv1-wp6.md` 与 `reports/rv1-wp6b.md` 检视）、`crates/core/**`、`docs/**`（只作为判据来源，未检视其文字本身）。
- 判据来源：`CORE_PORTS_AND_STORAGE.md` §3/§5/§6/§7.1/§7.2/§7.3/§7.4/§7.5/§9/§11.1–§11.9、`openspec/changes/admin-state-persistence-v2/specs/storage-schema-v2-migration/spec.md`、`tasks.md` 的 2.14–2.18 与其完成条件、`verification.md` 的 Check Plan Changes 与 Failures and Retests。
- 只读命令与退出码（**未运行任何 cargo 命令**）：

```text
git rev-parse HEAD / git log --oneline / git status --porcelain        -> 0
git diff 28f8cb9..5404610 -- crates/storage-sqlite/{src,tests} fixtures/storage -> 0
node scripts/check-contract-drift.mjs                                  -> 0
    contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；
    §5 的 15 个 trait / 87 个方法签名与 crates\core\src\ports.rs 一致
node -e <node:sqlite 复现：12-step 重建 + sqlite_sequence 回填>          -> 0
node -e <node:sqlite 复现：Import 拆分迁移且 foreign_keys=ON>            -> 0
node -e <node:sqlite 探测：ALTER TABLE ... RENAME 对 sqlite_master 文本的影响> -> 0
sed + sort + diff（规范化后的 DDL 逐行比对：4 组）                       -> 0（均逐行相同）
read fixtures/storage/v2/{empty,from-v1,too-new}.sqlite3 + 定向 SQL 查询 -> 0
```

- 独立复算边界：`node` = 24.19.0，其内建 SQLite 与 Rust 侧 `libsqlite3-sys` 捆绑的版本可能不同，因此上面的 SQL 语义复现只用于验证**语句形状与序列算术**，不代替 `cargo test`；下面所有「夹具内容」「DDL 文本一致性」结论都来自我直接读取固定版本的夹具与源码，不是引用实现者日志。
- 实现者证据（**未由本轮独立复算**，只核对了文件存在与 sha256 与 `verification.md` 记录一致）：`openspec/changes/admin-state-persistence-v2/reports/wp4-migration-tests.log`（`ff8405540caefe49c44b5fc63a7d50beafa4c8d6a686af1074153b169b88d949`，其中 `migration` 目标 = 6 passed + 1 ignored 生成器）、`reports/w0-verify-rust.log`、`reports/w0-npm-check.log`、`reports/wp5-contract-drift-before-after.md`。

## 1. 逐条核对（按 assignment 的 6 条）

### 1.1 DDL 组织与逐条一致 —— 成立（含一处计划计数偏差，见 §3 I-1）

- 漂移门禁我复算退出 0，报「**§7 的 36 条 DDL**」。逐条清点：`OWNED_SCHEMA_V1`（`crates/storage-sqlite/src/migrate.rs:47-331`）= 18 表 + 11 索引 = 29 条，`IMPORTED_SCHEMA_V1`（`migrate.rs:337-432`）= 6 表 + 1 索引 = 7 条，合计 36。§7 仍恰好 2 个 ```sql 块（升级步骤写在 `CORE_PORTS_AND_STORAGE.md` §7.2 的普通列表里，未新增围栏）——脚本未报「期望 2 个 sql 块」即已覆盖该点。
- `action` CHECK 与 core 枚举逐值一致（我机械比对，非抽样）：
  - 新建库 `owned_audit`（`migrate.rs:161-167`）的取值集合 == `AuditAction::ALL`（`crates/core/src/model/identity.rs:862-882`，20 个取值），集合双向相等；
  - 重建脚本 `owned_audit_v2`（`migrate.rs:444-460`）与 `imported_audit_v2`（`migrate.rs:484-501`）的**整段表定义**在规范化（去注释、去 `IF NOT EXISTS`、去 `_v2`、压空白）后与新建库定义逐行相同（对：`159-178 ↔ 444-460`、`402-419 ↔ 484-501`、`338-349 ↔ 516-526`），因此四处 `action` CHECK 与 `AuditAction::ALL` 同值；
  - 新建库 `imported_audit`（`migrate.rs:402-419`）的取值集合 ⊇ 20 个 core 取值且无多余 token。
- `public_key` 为 BLOB：`migrate.rs:200`（`owned_device`）、`:238`（`owned_peer_key`）、`:275`（`owned_pairing_peer`，段见 `270-280`），均为 `BLOB NOT NULL` + `CHECK (length(public_key) = 65)`，没有 hex/base64 文本路径。
- `*_json` 空集合写 `'[]'`：`migrate.rs:202`（`scopes_json`）、`:303`（`provider_env_json`）、`:347`（`grants_json`，含「无可信来源时写 `'[]'`」），并有表级 CHECK 兜住配对的两个空集合侧（`migrate.rs:264-266`：设备配对的 `requested_grants_json` 必须 `= '[]'`、节点配对的 `requested_scopes_json` 必须 `= '[]'`）。
- 索引与目标形状一致：`CREATE INDEX IF NOT EXISTS owned_node_role ON owned_node(kind, state)`（`migrate.rs:232`）、部分唯一索引 `CREATE UNIQUE INDEX IF NOT EXISTS owned_agent_profile_default ON owned_agent_profile(is_default) WHERE is_default = 1`（`migrate.rs:309`）。
- 与「实现前冻结的 `CORE_PORTS_AND_STORAGE.md` §11.7 目标形状」的一致性是逐行相同的：把基线 `28f8cb9` 的 §11.7 ```sql 块（9 张管理表 + 2 个索引）与 `migrate.rs:197-330` 规范化后 diff（122 行 vs 119 行，差 3 行全是围栏与一行散文），imported 侧的 `imported_import_export` 同样（基线块 ↔ `migrate.rs:424-431`，差 1 行是闭围栏）。因此 11 条管理语句逐字落地、顺序未变。
- 其余 v2 枚举列与 core 枚举逐值一致（**人工核对**，无机器判据，见 §3 I-2）：`owned_device.state` ↔ `DeviceState`（`crates/core/src/model/identity.rs:175-181`）、`owned_device/owned_node.revoke_reason` ↔ `RevokeReason`（`crates/core/src/ports.rs:574-578`）、`owned_node.kind` ↔ `NodeKind`（`identity.rs:267-270`）、`owned_node.state` ↔ `NodeState`（`identity.rs:275-279`）、`owned_peer_key/owned_pairing_peer.peer_kind` 与 `owned_pairing.target_kind` ↔ `PairingTarget`（`identity.rs:395-398`）、`owned_pairing.state` ↔ `PairingState`（`identity.rs:403-411`）、`owned_provider_ref.kind` ↔ `ProviderRefKind`（`crates/core/src/model/config.rs:265-268`）。

### 1.2 12-step 表重建 —— 成立（序列回填的用例判别力缺口见 §3 F1）

- 手法与顺序：`V2_UPGRADE_OWNED`（`migrate.rs:443-471`）= 建 `owned_audit_v2` → 按列拷贝**含 `audit_id`** 的全部行（`:462-465`）→ `DROP TABLE owned_audit`（`:467`）→ `RENAME`（`:468`）→ 重建两个索引（`:469-470`）；`V2_UPGRADE_IMPORTED` 对 `imported_audit` 同法（`:484-511`）。
- 索引必须重建这一点成立：我用 node:sqlite 复现确认 `DROP TABLE` 会连带删除该表上的索引（探测中 `owned_audit_action` 随 DROP 消失），而重建语句显式补回 `owned_audit_at`/`owned_audit_action`（`:469-470`）与 `imported_audit_at`（`:511`）。
- 序列回填链：`migrate()` 在升级块里先取 `audit_sequences`，再跑两段升级脚本，再 `restore_audit_sequences`，最后 `mark_v2_schema_versions`（`migrate.rs:884-889`）；`audit_sequences`（`:960-970`）读 `sqlite_sequence` 的 `(表名, seq)`，`restore_audit_sequences`（`:976-997`）先 `UPDATE ... WHERE seq < ?2` 再在缺行时 `INSERT ... SELECT ?1, ?2 WHERE NOT EXISTS (...)`。
- 我在 node:sqlite 上复现了这条链的语义：`DROP TABLE` 删除序列行、`ALTER TABLE ... RENAME` 会把 `sqlite_sequence.name` 改成新名（复现：重建后新表序列 = 拷贝进来的 `max(audit_id)`）、`UPDATE`/`INSERT ... WHERE NOT EXISTS` 的形状合法且「只增不减」。在「序列领先于 `max(audit_id)`」的库上（行 `{1,2}`、`seq = 5`）：重建后 `seq = 2`，回填后 `5`，下一条插入得 `audit_id = 6`；**不回填**则下一条得 `3`——即回到已被用过的 id 区间（该缺陷正是回填要防的）。
- 重建后的 `sqlite_master` 文本：`ALTER TABLE ... RENAME` 会把存储的 DDL 重写为 `CREATE TABLE "owned_audit" (...)`（node:sqlite 复现），因此**升级库的 DDL 文本与新建库的不再逐字相同**（表名被重新引用，列与 CHECK 不变）。这不违反任何判据——`CORE_PORTS_AND_STORAGE.md` §9 的逐字节判据针对**同一个库连续两次打开**，由 `second_open_of_an_upgraded_database_rewrites_nothing` 覆盖；但「升级库 DDL 文本 == 新建库 DDL 文本」没有用例断言，我只能用规范化比对人工确认（见 §3 I-2）。
- `from-v1.sqlite3` 的 `audit_id` 空洞（`{1,2,5}`）确实被覆盖（用例与夹具一致），但**序列不回的判别力不成立**：夹具的 `max(audit_id) = 5 = sqlite_sequence.seq`，见 F1。

### 1.3 Import 拆分迁移 —— 成立

- `imported_import` 的新形状：`migrate.rs:338-349`（去 `export_id`、去 `UNIQUE (owner_node_id, export_id)`、加 `grants_json`），列序与 §7.4 冻结清单一致（`tests/imported.rs:99-115` 逐项相等）。
- 关联表：`migrate.rs:424-431`（`PRIMARY KEY (import_id, export_id)` + `UNIQUE (owner_node_id, export_id)` + 只存值的 `owner_node_id`，无跨族外键）。
- 拆分脚本顺序与理由（父表被 FK 引用、`DROP` 会按 `ON DELETE CASCADE` 删子行，故先把 `(owner_node_id, export_id, created_at)` 落进**无外键**的过渡表）：`migrate.rs:475-481` 的注释 + `:513-538` 的语句；`added_at` 取原 `created_at`（`:513-514`、`:536-537`）；`grants_json` 一律 `'[]'`（`:528-531`）。
- 我用 node:sqlite 在 `foreign_keys = ON` 下复现整段：迁移可执行、结果恰好一行关联行（`added_at` = 原 `created_at`）、`PRAGMA foreign_key_check` 为空、FK 确实生效（插入悬空 `import_id` 被拒）。
- 不补 grants 的行为断言存在：`tests/migration.rs:377-395` 断言升级后 `imported_import` 列名不含 `export_id`、`grants_json == "[]"`、关联行 = `import-fixture|node|export-fixture|<created_at>`。
- 夹具侧核对（我直接读库）：`fixtures/storage/v2/from-v1.sqlite3` 的 `imported_import` 是 v1 形状（含 `export_id`、`UNIQUE (owner_node_id, export_id)`）且只有一行；升级目标库（`too-new.sqlite3`/`empty.sqlite3`）才有 `imported_import_export`。

### 1.4 幂等与回滚 —— 成立；「中途失败整体回滚」无用例覆盖（见 §3 F2）

- `migrate()` 顺序（`migrate.rs:848-931`）：单 `BEGIN IMMEDIATE`（`:849`）→ 读 `PRAGMA user_version`（`:850`）→ 过新**先于任何 DDL** 早返回 `FileFormatTooNew`（`:851-857`）→ `new_database = !table_exists(&mut *tx, "owned_session")`（`:858`，`table_exists` 实现在 `:943-953`）→ 执行两个 DDL 常量（`:860-861`）→ 读两族版本并拒绝过新（`:863-879`）→ 升级块（`:881-889`，条件 `!new_database && file_version < FILE_FORMAT_VERSION`）→ 五个 `write_meta_if_absent`（`:891-907`）→ 按需 `PRAGMA user_version`（`:909-912`）→ 读回 `meta` → `commit()`（`:929`）。失败路径都靠 `?` 早返回、不 commit（sqlx 的 `Transaction` drop 回滚）。
- 「已是 2 时跳过升级」：升级块条件不成立即整段不执行；`write_meta_if_absent` 不覆盖既有行（`:1045-1056`）；`PRAGMA user_version` 仅在 `file_version != FILE_FORMAT_VERSION` 时执行（`:909`）。
- 「第二次打开逐字节不变」**确实被断言了**（不是抽样）：`tests/migration.rs:415-458` 对 `from-v1` 复制品开两次，比较 `schema_sql`（`sqlite_master` 全部行的 type/name/sql）、`meta_rows`（全部键值）、`PRAGMA user_version`，以及 `TABLES`（24 张表，`tests/migration.rs:15-40`）逐表的行快照。口径与 `plan.md` 的规定（三个面）一致；未做文件级字节比较，但数据库页布局变化不属于合同所称的「库内容」。
- 「`too-new` 零写入」：夹具 `user_version = 3`（我直接读库确认），用例 `tests/migration.rs:180-217` 断言具名 `FileFormatTooNew { found: 3 }`、`user_version` 仍为 3、`sqlite_master`/`meta`/24 张表的行快照全不变。
- 「升级中途失败整体回滚」：**没有用例**（`migration` 目标只有 6 个用例，`rg TRIGGER` 只命中 `tests/admin_store.rs` 的管理写集注入）。见 F2。

### 1.5 夹具可复现 —— 成立（序列场景判别力见 F1）

- 生成方式可复现：`tests/migration.rs:541-700` 的 `regenerate_v2_fixtures`（`#[ignore]`）在文档注释里写明命令与三件夹具的派生规则——`empty.sqlite3` 走真实 `SqliteStore::open` + `close()`（内部 `wal_checkpoint(TRUNCATE)`）、`too-new.sqlite3` 是它 + `PRAGMA user_version = 3`、`from-v1.sqlite3` 以 `fixtures/storage/v1/empty.sqlite3` 为基底插入 v1 形状的行并保留 `user_version = 1`（`:551-554`、`:687`）。
- v1 资产保留且未被覆写：`git diff --stat 28f8cb9..5404610 -- fixtures/storage/v1` 为空；`fixtures/storage/` 下仍有 `v1/{empty,too-new}.sqlite3`。`v1/too-new.sqlite3` 已无消费方（`grep fixtures/storage` 只命中 `v1/empty.sqlite3` 与 v2 路径），作为历史资产保留，`CORE_PORTS_AND_STORAGE.md` §7.2 已写明。
- 全部用例已切到 v2 三件套：`tests/support/mod.rs:66-73` 的 `copy_fixture` → `fixtures/storage/v2`，`repo_root()`（`:29-38`）锚在 `fixtures/storage` 上（生成器在 v2 目录尚不存在时也能定位仓库根）。实现者日志也显示 6 个用例全部执行（未独立复算）。
- 夹具自洽性（我直接用 SQLite 读固定版本的三件夹具）：
  - `empty.sqlite3`：24 张表（含 `imported_import_export` 与全部 `owned_*` 管理表），`meta` 5 行；
  - `from-v1.sqlite3`：14 张表（**缺**全部 v2 管理表与 `imported_import_export`），`meta.owned_schema_version/imported_schema_version = 1/1`，`user_version = 1`，`sqlite_sequence` = `owned_event 3 / owned_command 1 / owned_audit 5 / imported_audit 1`，`owned_audit` 行 = `{1,2,5}`，`imported_import` 为 v1 形状（含 `export_id`）；
  - `too-new.sqlite3`：24 张表、`user_version = 3`（由 `read` 的 PRAGMA 查询确认）。
  以上与用例的基线断言逐项吻合；夹具「真的长着 v1 的样子」这一点成立。

### 1.6 `enum_coverage` / `retention` / `imported` / `commit` 的 v2 适配

- `enum_coverage.rs`：`cases` 共 16 项（`tests/enum_coverage.rs:88-186`），其中 `owned_audit.action` 与 `imported_audit.action` 都是 `tokens(AuditAction::ALL, AuditAction::as_str)`，且比较是「排序后集合相等」（双向）——**是逐条相等，不是抽样**。但该用例读的是**新建库**的 DDL，且 v2 新增的管理表枚举列一个都没进 `cases`（与其文件头「§7.3/§7.4 的每个 `IN (...)` 枚举列」的声明不符）；升级库一侧只被 `contains("provider.configured")` + 行为插入断言（`tests/migration.rs:341-361`）。我人工核对当前两侧一致，故此项目前不是缺陷，见 §3 I-2。
- `retention.rs`：本次**未改动**（`git diff 28f8cb9..5404610 -- tests/retention.rs` 为空）；容量度量在 `tests/support/mod.rs:183-226` 由 schema 现读「两族全部表」的 TEXT 列（措辞已随 v2 更新），与 `session_store::measure_total` 同口径，新管理表自动计入。
- `imported.rs`：只做结构适配（列清单 `5 → 6`、`drop_import` 用例补一条关联行、无正文表清单加 `imported_import_export`）；`drop_import_keeps_audit_rows` 的断言强度未削弱——仍断言 `delivery_index_removed == 1`、`command_refs_removed == 1`、两张表清空、`imported_audit` 仍有 1 行、未知 import → `NotFound`（`tests/imported.rs:349-383`）。
- `commit.rs`：只改 `assert_eq!(health.user_version, 2)`（`tests/commit.rs:1288`），其余不动；`owned_audit` 黄金列清单用例（`tests/commit.rs:1298-1324`）未改，列集合确实没变（重建只换 CHECK）。
- `session_store.rs` 的三处连带：`list_sessions` 的过滤子查询改查 `imported_import_export`（`session_store.rs:2787-2791`）、`import_matches` 同改（`:2958-2962`）、`drop_import` 重写为「管理行存在性 → 取全部 `(owner_node_id, export_id)` 关联对 → 逐对删交付索引/命令引用」，且不再写 `removed_at`（`:2834-2885`）。语义与 `CORE_PORTS_AND_STORAGE.md` §11.6 的「两处删除权威不重叠」一致，`verification.md` 的 Check Plan Changes 第 10 条已登记该偏差。

## 2. 规格场景覆盖（`openspec/changes/admin-state-persistence-v2/specs/storage-schema-v2-migration/spec.md`）

| Requirement / Scenario | 对应用例 或 范围判定 |
| --- | --- |
| 版本常量与 migration 幂等 / 连续两次打开 schema 文本不变 | `tests/migration.rs::v2_fixture_is_untouched_by_two_consecutive_starts`（既有 v2 夹具：两轮启动比较 `schema_sql`/`meta`/`user_version`）与 `second_open_of_an_upgraded_database_rewrites_nothing`（升级后的库：再开一次比较三面 + 24 表行快照）。两族版本都被断言为 2（`fresh_directory_is_created_at_version_two`、升级用例） |
| 版本常量与 migration 幂等 / 升级中途失败整体回滚 | **无用例覆盖**（见 §3 F2）：`migration` 目标 6 个用例中没有任何失败注入；「拆成多个事务」或「把某一步移出事务」都不会让现有断言变红 |
| v1 数据保留与 Import 归属迁移 / 升级后重放与幂等仍一致 | `tests/migration.rs::v1_fixture_upgrades_to_v2_and_preserves_rows`：断言 `global_sequence`/`session_sequence`/`origin_sequence` = 1/2/3、`origin_epoch` 唯一、`event_id` 逐行不变、幂等行 `requestId|completed|terminal_event_id` 不变、`audit_id` = `{1,2,5}`、`imported_session` 的 cursor/ACK、`imported_delivery_index.local_sequence`、`imported_command_ref.request_id` 逐行保留。**「重放结果与升级前一致」这一半是在库内行层面覆盖的**（未在升级库上走 `local_replay()`，该读路径本次未改） |
| v1 数据保留与 Import 归属迁移 / 缺少可信来源的 Import 保持不可用 | `v1_fixture_upgrades_to_v2_and_preserves_rows`：`grants_json == "[]"`、`imported_import` 无 `export_id`、关联行 `(import-fixture, node, export-fixture, added_at=原 created_at)`；「该 Import 不可用」的读写判定属管理 store（`reports/rv1-wp6.md` 范围） |
| 过新版本拒绝打开 / 过新库零写入 | `tests/migration.rs::too_new_database_is_rejected_without_writing_rows`：`user_version = 3` 夹具 → 具名 `FileFormatTooNew { found: 3 }`，`user_version`/`sqlite_master`/`meta`/24 表行快照全不变（`migrate()` 在 `crates/storage-sqlite/src/migrate.rs:851-857` 早于任何 DDL 返回） |
| 损坏与权限不符时的失败关闭 / 损坏库的写路径全部被拒 | `tests/migration.rs::corrupt_database_fails_closed`（两种可接受结局：具名错误 或 只读失败关闭 + 写路径 `PortError::Corrupt` + 只读查询仍可用）；管理写路径逐条全拒的详尽用例在 `tests/admin_store.rs`（WP6 范围） |
| 损坏与权限不符时的失败关闭 / 权限过宽在 Unix 上失败关闭 | 本切片未改动：`tests/permissions.rs`（4 个用例在实现者日志中 ok，**未独立复算**）；Windows 返回 `Unverifiable` 且不失败关闭，与 `CORE_PORTS_AND_STORAGE.md` §7.1 一致 |
| 管理表纳入保留与容量 / 超限时拒绝新写入而不是删信任 | 范围判定：容量门的调用点在管理 store（WP6 范围，`tests/admin_store.rs::capacity_limit_refuses_new_admin_writes_without_deleting_trust`）；本切片只保证度量口径把两族全部表纳入 |
| 管理表纳入保留与容量 / 度量包含管理表分量 | `tests/support/mod.rs:measured_storage_bytes`（`183-226`）改为从 schema 现读**两族全部表**的 TEXT 列 + 附件字节，与 `session_store::measure_total` 同算法；`tests/retention.rs::capacity_measure_matches_the_store` 断言两端相等（该文件本次未改，实现者日志 ok，**未独立复算**） |
| imported 家族保持无正文 / 黄金列清单逐项相等 | `tests/imported.rs::imported_tables_match_the_frozen_column_lists`（`96-200`）在**新建库**上逐表 `PRAGMA table_info` 与冻结清单逐项（含顺序）相等，6 张表含新增的 `imported_import_export`；**升级库上没有同名断言**（见 §3 I-2） |
| imported 家族保持无正文 / 投递正文后库内无正文 | `tests/imported.rs::imported_tables_never_hold_owned_content`（无正文表清单加入 `imported_import_export`） |
| imported 家族保持无正文 / 清空与移除都保留审计 | `tests/imported.rs::drop_import_keeps_audit_rows`（连接级清空：断言两张表清空、`imported_audit` 仍有 1 行、未知 import → `NotFound`）；`remove_import` 侧在 WP6 范围 |

## 3. 未解决发现

| ID | 级别 | 位置 | 问题与影响 | 证据 | 假想回退与失败点 | 建议 |
| --- | --- | --- | --- | --- | --- | --- |
| F1 | 建议 | `crates/storage-sqlite/tests/migration.rs:225-239`（生成器 `:687`） | 「`audit_id` 与其 AUTOINCREMENT 序列不回退」这条合同要求在夹具上**不成立判别力**：`from-v1.sqlite3` 留下 `audit_id = {1,2,5}` 而 `sqlite_sequence.seq = 5`（`max == seq`），12-step 重建按列拷贝 `audit_id` 时本身就把新表序列置为 5，于是回填步骤成了空操作；断言消息「the fixture keeps an audit sequence that is ahead of max(audit_id)」与夹具实际不符。影响：真回退（尾部行被 365 天 TTL 清理后 `seq > max`）时序列会回退并**复用已用过的 `audit_id`**，而现有用例抓不到 | 夹具直读：`owned_audit` 行 `{1,2,5}`、`sqlite_sequence.owned_audit = 5`；node:sqlite 复现：同一数据下重建**不回填**也得到 `seq = 5`、下一条仍得 `audit_id = 6`（即删掉 `restore_audit_sequences` 全部断言照样绿）；对照复现：行 `{1,2}` 且 `seq = 5` 时，不回填重建后 `seq = 2`、下一条得 3（复用） | 回退 = 删掉 `migrate.rs:887` 的 `restore_audit_sequences(&mut tx, &sequences).await?`（连同 `:960-997` 两个函数）：F1 指出的两条断言与 `MAX(audit_id) == 6` 全部通过，失败点是「没有任何用例能发现序列回退」 | 让夹具真正产生「`seq` 领先于 `max(audit_id)`」：生成器把审计行插到 `audit_id = 1..=7` 再 `DELETE ... WHERE audit_id IN (3,4,6,7)`（保留 `{1,2,5}` 的空洞形状、序列变 7），用例相应改为断言升级后 `seq == 7` 且新插入得 `audit_id == 8`；`imported_audit` 同样处理（插 1..=3 后删 2、3，行数仍为 1）。改后重新运行生成器并提交夹具 |
| F2 | 建议 | `crates/storage-sqlite/tests/migration.rs:415-458`（缺失用例应在该处；实现见 `crates/storage-sqlite/src/migrate.rs:881-889`） | 规格场景「升级中途失败整体回滚」与 `tasks.md` 的 2.17 完成条件「中途失败回滚用例通过」都**没有对应用例**：`migration` 目标只有 6 个用例，全仓库的失败注入（`CREATE TRIGGER ... RAISE`）都在 `tests/admin_store.rs` 的管理写集用例里。影响：v1→v2 升级是本变更里唯一会删表/重建表/搬数据的路径，而「单事务」这一保证没有任何判据；一旦某一步被移出事务或事务边界被改小，半升级的库（管理表已建、审计 CHECK 未换、`imported_import` 已删而关联行未搬）不会被发现 | 用例清单见实现者日志（6 passed + 1 ignored）；`grep -rn TRIGGER crates/storage-sqlite/tests/*.rs` 只命中 `tests/admin_store.rs:2681/2714/2755/2757/2762/2764`；`migrate()` 的事务边界在 `crates/storage-sqlite/src/migrate.rs:849`（`begin_with("BEGIN IMMEDIATE")`） | 回退 = 把升级步骤改成事务外执行（例如直接 `sqlx::raw_sql(...).execute(pool)`、或在 `V2_UPGRADE_OWNED` 与 `V2_UPGRADE_IMPORTED` 之间 commit）：`v1_fixture_upgrades_to_v2_and_preserves_rows`、`second_open_of_an_upgraded_database_rewrites_nothing`、`v2_fixture_is_untouched_by_two_consecutive_starts` 仍全部通过，失败点是「没有任何用例能发现半升级状态」 | 加一条注入用例：在 `from-v1` 复制品上先建 `BEFORE INSERT ON owned_audit_v2` 的 `RAISE(ABORT,'inject')` 触发器（或预建一张同名冲突的 `owned_audit_v2`），`SqliteStore::open` 必须返回错误；随后断言库仍是 v1 形状（`user_version = 1`、`meta.*_schema_version = '1'`、`owned_device`/`imported_import_export` 不存在、`imported_import` 仍含 `export_id` 且行数不变、`owned_audit` 的 DDL 文本未变），移除注入后重开可成功升级（覆盖场景的第三个分句） |
| I-1 | 信息 | 交办本轮的 W0 执行版计划（local://admin-state-persistence-v2-w0-plan.md 的 Verification 段，硬判据「改后必须是 37 条（owned 30、imported 7）」）对照 verification.md:33（记录 36 条） | 交办文件的硬判据计数与实现不符：实现是 OWNED_SCHEMA_V1 29 条 + IMPORTED_SCHEMA_V1 7 条 = **36**（与漂移门禁输出一致），而交办文件把「改前」也写成 25 条（本机清点基线 28f8cb9 的 ^CREATE 语句为 **24** 条：owned 18 + imported 6），两处都多算一条。我按 CORE_PORTS_AND_STORAGE.md §11.7 的目标形状逐条清点，11 条管理语句（9 表 + 2 索引）全部在位、无缺语句，故这是交办/计划侧计数偏差而非交付缺陷；该偏差未登记在 verification.md 的 Check Plan Changes | node scripts/check-contract-drift.mjs 输出 36；规范化 diff（基线 §11.7 块 ↔ migrate.rs:197-330）逐行相同；git show 28f8cb9:...migrate.rs 的 ^CREATE 计数 = 24；verification.md:33 记 36 | 不适用（文档计数，无代码回退） | 在 verification.md 的 Check Plan Changes 登记该计数修正（36 条 = owned 29 + imported 7），以免后续复核按 37 条判定失败 |
| I-2 | 信息 | `crates/storage-sqlite/src/migrate.rs:197-330`（v2 管理表 DDL）与 `:444-460`、`:484-501`（重建脚本） | 本次新增的 v2 枚举列**没有逐值判据**：`tests/enum_coverage.rs` 的 `cases`（`:88-186`）仍是 v1 期的 16 项，未覆盖 `owned_device.state`/`revoke_reason`、`owned_node.kind`/`state`/`revoke_reason`、`owned_peer_key.peer_kind`、`owned_pairing.target_kind`/`state`、`owned_pairing_peer.peer_kind`、`owned_provider_ref.kind`（core 侧都有对应枚举），与其文件头「§7.3/§7.4 的每个 `IN (...)` 枚举列」的声明不符；升级库一侧只被 `contains("provider.configured")` + 行为插入断言（`tests/migration.rs:341-361`）。我已人工核对当前两侧**完全一致**，故不是现存缺陷，但同类漂移（该文件头自己举过 `ElicitationAction::Decline` 的反例）今后不会被判据拦住 | 逐步核对：`crates/core/src/model/identity.rs:175-181`（`DeviceState`）、`:267-270`（`NodeKind`）、`:275-279`（`NodeState`）、`:395-398`（`PairingTarget`）、`:403-411`（`PairingState`）、`crates/core/src/model/config.rs:265-268`（`ProviderRefKind`）、`crates/core/src/ports.rs:574-578`（`RevokeReason`）与 `migrate.rs` 的对应 CHECK 取值相同；重建定义与新建定义规范化后逐行相同（4 组 diff 均空） | 回退 = 让重建脚本的 CHECK 少一个取值（例如删掉 `'consumed'`）：除我做的规范化文本比对外的现有断言都不会失败（`contains("provider.configured")` 仍成立），失败点是「升级库与新建库的枚举取值集合可以悄悄分叉」 | 把 v2 管理表的枚举列加入 `tests/enum_coverage.rs` 的 `cases`（用 core 的 `ALL`/`as_str` 生成期望值），并加一条「升级库的 `owned_audit`/`imported_audit` DDL 的 `action` 取值集合 == 新建库」的断言（在 `tests/migration.rs` 里对升级后的库复用 `enum_coverage` 的解析逻辑） |
| I-3 | 信息 | `crates/storage-sqlite/tests/migration.rs:277-400` | 规格两个场景的 WHEN 条件是「升级到 v2 后」，但对应断言只覆盖了库内行：「重放结果与升级前一致」没有在升级库上走 `local_replay()`，「黄金列清单逐项相等」只在新建库上跑（`tests/imported.rs:96-200`）。当前由「读取路径本次未改」+「重建 DDL 与新建 DDL 逐字相同（人工比对）」支撑，行为等价成立但无用例锁定 | 两个场景的 WHEN 子句见 `specs/storage-schema-v2-migration/spec.md`；`tests/migration.rs` 的升级用例只做 SQL 断言，未调用 `read_view()`/`local_replay()` | 回退 = 在重建脚本里换掉某个列名（例如 `grants_json` 写成 `grants`）：升级库会缺列，而 `tests/imported.rs` 的黄金列清单只查新建库、升级用例只查「不含 `export_id`」，失败点是「升级库的列集合与新建库可以不一致而无人发现」 | 在升级用例里对升级后的库跑一次 `local_replay()` 的等价断言，并对每张 `imported_*` 表（含 `grants_json`）做一次列集合比较（可直接复用 `tests/support/mod.rs:column_names`） |

## 4. 结论

**PASS（无阻断项）。** 阻断项清单：空。

- **代码侧（`crates/storage-sqlite/src/migrate.rs`、`session_store.rs` 的迁移连带、三件 v2 夹具）与合同一致**：§7 的 36 条 DDL 与实现逐条相同（漂移门禁我复算退出 0）、`CORE_PORTS_AND_STORAGE.md` §11.7 的 11 条目标管理语句逐字落地；四处 `action` CHECK 与 `AuditAction::ALL` 逐值相等；`public_key` 为 BLOB、`*_json` 空集合写 `'[]'`、`owned_node_role` 与部分唯一索引 `owned_agent_profile_default` 就位；12-step 重建（含 `sqlite_sequence` 只增不减回填与索引重建）与 `imported_import` 拆分迁移（`added_at` 取原 `created_at`、grants 一律 `'[]'`、无跨族外键）在 node:sqlite 与 `foreign_keys = ON` 下复现通过；`migrate()` 的「读版本 → 过新早返回（早于任何 DDL）→ 建表 → 按需升级 → 写版本」顺序与单事务边界正确；`too-new` 零写入与「升级后第二次打开逐项不变」都有确有判别力的断言。
- **两条建议项（均为测试/夹具侧，修在 WP4 内，不影响上面的代码结论）**：
  - **F1**：让 `from-v1` 夹具真正产生「`sqlite_sequence.seq` 领先于 `max(audit_id)`」。当前夹具（`{1,2,5}` + `seq = 5`）使「AUTOINCREMENT 不回退」的回填步骤成为空操作——删掉 `restore_audit_sequences` 全部断言仍绿（已用 node:sqlite 复现），而该要求是 `CORE_PORTS_AND_STORAGE.md` §9 的判据 28 与 `specs/storage-schema-v2-migration/spec.md` 的明文要求。
  - **F2**：补一条「升级中途失败整体回滚」的注入用例。该场景是规格场景之一，也是 `tasks.md` 的 2.17 完成条件（「中途失败回滚用例通过」）——2.17 在 `tasks.md` 已勾选，但仓库内没有对应用例（`migration` 目标只有 6 个用例，触发器注入只在 `tests/admin_store.rs`）。请复核时按「任务完成条件未满足」处理。
- **三条信息项**：I-1（交办文件把 DDL 条数硬判据写成 37 条、改前 25 条；实现与 verification.md 记录的 36 条、基线 24 条才是实数，属计数偏差，未登记）、I-2（v2 新增管理表枚举列与升级库 DDL 没有逐值判据；我已人工确认当前两侧一致）、I-3（「升级到 v2 后」的两个场景只在库内行/新建库层面断言，未在升级库上重放或比对列集合）。
- **本轮未独立复算的项**（不得据此认定已通过，需主 Agent 在对应门禁前补齐）：`cargo fmt --check`、`cargo clippy -p storage-sqlite --all-targets --all-features -D warnings`、`cargo test --locked -p storage-sqlite --all-features`（`openspec/changes/admin-state-persistence-v2/reports/wp4-migration-tests.log`，sha256 与记录一致）、`cargo test --workspace`（`reports/w0-verify-rust.log`）、`npm run check`（`reports/w0-npm-check.log`）、夹具生成器 `regenerate_v2_fixtures` 未运行（本轮直接检视已提交的三件夹具内容）、`tests/permissions.rs` 的 Unix 分支在 Windows 上未实际执行。
- 本报告只对固定版本 `5404610d35b22c9d427b88b0cea2c61d1e7a5e15` 有效；本切片若在这些文件上继续改动，需要新一轮独立 review（新 Review ID）。
- 本轮唯一写入的文件是本报告；未修改任何代码、测试、夹具、规划文件或他人报告。

## 5. 附录：检视期间的工作区漂移（记录用）

本轮开工时工作区只有宿主的未跟踪项；检视过程中工作区又出现**他人未提交的改动**：`README.md`、`docs/CONFIG_REFERENCE.md`、`docs/CORE_PORTS_AND_STORAGE.md`、`docs/DEVELOPMENT_PLAN.md`、`docs/MODULE_ARCHITECTURE.md`、`openspec/changes/admin-state-persistence-v2/verification.md`。这些都不在本切片范围（`docs/**` 只作判据来源），因此本报告引用的文档文本是**检视时刻**的内容；WP4 的被检视文件（`crates/storage-sqlite/**`、`fixtures/storage/**`）在整个检视过程中没有任何工作区改动，仍等于固定版本 `5404610`。
