# coder 报告 · WP4 增量任务 2.22（`storage-sqlite` 实现 `AuditStore`）

- task_id: `2.22`（WP4 增量，前置于 2.14；`tasks.md` 第 2.22 条）
- role: coder
- phase: implement
- stage: work-package
- agent_context: 独立子 Agent（worker / coder 角色）；主 worktree 的分支 `feat/daemon-cli-and-local-admin`；只继承任务单 2.22 的文本与只读上下文（合同 `docs/CORE_PORTS_AND_STORAGE.md` §5/§7/§11.6、既有 `crates/storage-sqlite/src/admin/{export,trust,local_config}.rs` 与 `tests/`），不继承规划阶段对话；同一时刻只有本写入者编辑本任务的文件
- base_revision: **`9376e96`**（开工前 `git log --oneline -3` 的首行，与任务单一致）
- target_revision: **`fe093b3`**（`test(storage): 补审计查询的混合条件与 limit 组合用例`，本任务交付的**第二个**提交）；实现主体是它的前一个提交 **`bb96a10`**（`bb96a10c5784a4a49e6dd78adb4732176b7435e4`，`feat(storage): 实现 AuditStore（append/query over owned_audit）`）。两个提交合起来构成本任务的全部交付内容；报告以 `docs(storage)` 提交入库，报告本身不属交付物
- scope: `crates/storage-sqlite/src/admin/audit.rs`（新增）、`crates/storage-sqlite/src/admin/mod.rs`（+1 行模块声明）、`crates/storage-sqlite/tests/admin_audit.rs`（新增测试文件）、`openspec/changes/daemon-cli-and-local-admin/reports/`（本报告 + 两个 `.log`）
- result: `PASS`（任务单列出的三项 cargo 检查 + `node scripts/check-crate-boundaries.mjs` + `npm run check` 全部执行且全绿，含一次预期失败的 RED 探针；每次提交后 `git status --porcelain` 均为空；**不代表**独立 review、集成或合并已完成）

## 问题（已核实，未改合同）

`core::UseCases` 的 `UseCaseDeps.audit: Arc<dyn AuditStore>` 是必需字段（`crates/core/src/use_cases.rs:56/71`），`audit.export`（任务 2.12）经 `AuditStore::query` 读审计（`crates/server/src/local_admin/router.rs:1091-1103`），但工作区内**没有生产 `AuditStore` 实现**：`crates/storage-sqlite/src/admin/` 只有 export/local_config/trust 三个 impl。`owned_audit`（`migrate.rs:159`，列与 `action`/`actor_kind`/`outcome` CHECK）与 `imported_audit`（`migrate.rs:402`）表已存在，写集路径经 `insert_audit_rows`（`admin/mod.rs:45`）落审计。

本次在 `crates/storage-sqlite` 补 `impl AuditStore for SqliteStore`，**不改 core 端口、不改 DDL、不新增列**。

## 实现决策（关键点）

| # | 决策 | 依据与实现位置 |
| --- | --- | --- |
| 1 | **表选择**：`append` 只写 `owned_audit`；`query` 只读 `owned_audit` | `admin/mod.rs` 模块文档的既有裁定：「管理写集的审计行写进 `owned_audit`；`imported_audit` 保存的是**从 Owner 收到的**审计元数据（带 `owner_node_id`/`export_id`/`session_id`/`request_id`）」。`AuditRecord`（§3.5）没有承载来源节点/Export 的字段，把 §7.4 的行投影成本类型会静默丢掉归属列，因此 `query` 不跨表。实现见 `audit.rs:150-157`、`audit.rs:179` |
| 2 | **编码复用**：`append` 走 `admin::insert_audit_rows`（与状态写集同一条 `INSERT`、同一套 `actor_key`/`EntityRef::kind`/`target_id` 取值编码） | 只把 `AuditRecord` 逐字段搬成 `PendingAudit`（`audit.rs:39`，`at` 是 §11.6 写集模型里唯一被抽出的列），不另造序列化、不新增列。DDL 未动 → `check:drift` 判定不变（`§7 的 36 条 DDL 逐条一致`） |
| 3 | **`limit` 语义**：`None` = **不加 `LIMIT`**（不限行数）；`Some(0)` = 上限 0，返回空 | 合同 §5.3 的 `AuditQuery` 原文：「返回行数上限；`None` = 不限」。既有实现与合同都**没有**隐式上限（`audit.export` 正是以 `limit: None` 导出全部），因此本次不引入任何默认上限；`LIMIT` 以 `i64` 单独绑定（`audit.rs:246-256`） |
| 4 | **时间区间**：`since`/`until` **含端点**（`at >= since AND at <= until`），比较用列上的 TEXT 字典序 | §3.2：`Timestamp` 是定宽 UTC 毫秒文本，「字典序即时间序」；`owned_audit_at` 索引也是给这种比较用的 |
| 5 | **空结果与 `since > until`**：一律 `Ok(vec![])`，不是错误 | 没有匹配行是「查不到」而不是调用方参数错误；端口只允许 `PortError` 表达失败，合同未给该情形任何错误码。测试 `reversed_time_window_returns_no_rows` 固定这条语义 |
| 6 | **排序**：`ORDER BY at ASC, audit_id ASC` | 合同要求按 `at` 升序；同一时刻追加 `audit_id` 作为插入序的稳定 tiebreak（§13.2.1「跨表查询稳定排序」的同款意图），避免同刻多行顺序随查询计划变化 |
| 7 | **容量门**：`append` 与其他**会新增行**的写路径一致，走 `enforce_capacity_gate` | §7.5 ⑥「仍超限则拒绝新写入（`PortError::Unavailable(StorageFull)`），不静默丢弃、不继续广播」，审计排在清理顺序最后（⑤）；`admin/mod.rs` 的 `enforce_capacity_gate` 文档明确它「只加在会新增行的写路径上」。因此超限时本行随事务回滚（不留下半行），由调用方记录。已由 `append_refuses_a_new_row_when_over_capacity` 覆盖 |
| 8 | **保留交互**：追加的行与写集写的行同表同规则，受 §7.5 ⑤ 的 `storage.audit_retention_days` 约束 | 无新增机制（清理在 `session_store::prune` 的 ⑤ 里，本任务未改）；`appended_rows_are_swept_by_audit_retention` 证明追加行确实被扫掉 |
| 9 | **失败映射**：损坏列按本 crate 惯例走 `decode`/`decode_opt` → `StorageError::ColumnValue`，未知 `target_kind` → `ColumnValue{column:"owned_audit.target_kind", expected:"known entity kind"}`，`AuditRecord::try_new` 的校验失败 → `InvalidRequest`（与 `admin/export.rs` 的读路径同款）；无 `panic!`/`unwrap`/`expect`（`audit.rs` 零命中） | `error.rs` 的 `into_port_error` 表；`target_kind` 无表级 CHECK，未知类别不许猜近似实体（失败关闭） |
| 10 | **actor 投影**：`(actor_kind, actor_id)` 还原 `Actor`；Node 按 `"{node}/{access_node}"` 拆回；设备 scopes 不在审计列里，读回为 `ScopeSet::empty()` | §7.3 的审计列只有这两列；审计只记录归因、授权由 core 判定。该有损投影由 `device_actor_keeps_its_id_but_not_scopes` 显式固定（与 `session_store::actor_from_columns` 对 `owned_command` 的处理同款） |
| 11 | **不读系统时间** | 容量门与保留窗口都用记录自身的 `at`（调用方从 `Clock` 取得），与模块头「不读系统时间」一致 |

## 测试（13 个，全绿；`tests/admin_audit.rs`）

用既有测试基座（`mod support;` 的 `temp_dir`/`raw_pool`/`measured_storage_bytes`）与既有风格（真实 SQLite 文件 + `#[tokio::test]`）。

| 用例（行号取 `bb96a10`；最后一个用例由 `fe093b3` 追加） | 覆盖 |
| --- | --- |
| `round_trip_preserves_every_column`（143） | `append` → `query` 往返：整条记录相等 **且** `at`/`action`/`actor`/`via_node`/`local_principal_ref`/`target`/`outcome`/`detail_digest` 逐项断言 |
| `every_target_kind_round_trips`（182） | `EntityRef` 全部 11 种形状（含 `Command{session: None}`）的 `(target_kind, target_id)` 编解码 |
| `device_actor_keeps_its_id_but_not_scopes`（222） | 设备 actor 的 id 保真、scopes 不落审计列（有损投影已冻结） |
| `time_window_includes_both_endpoints`（266） | `since`/`until` 含端点；只给 `since`、只给 `until` 两种半开区间 |
| `reversed_time_window_returns_no_rows`（318） | `since > until` → 空 `Vec`（非错误） |
| `unmatched_filters_return_empty_results`（343） | 动作/目标/actor 无匹配 → 空 `Vec`；actor 与 target 是合取 |
| `actions_filter_accepts_multiple_values`（385） | `actions` 多值过滤；空 `actions` = 不按动作过滤 |
| `actor_and_target_filters_match_the_stored_columns`（423） | `actor`/`target` 过滤按存储的两列等值匹配（含 Node 复合键的 access node 参与匹配） |
| `rows_come_back_in_ascending_time_order`（489） | 倒序插入仍按 `at` 升序返回 |
| `limit_caps_rows_and_none_means_unbounded`（524） | `limit = 2` 取升序前 2 行；`limit = 0` 为空；`limit = None` 不限 |
| `default_query_returns_every_row`（575） | `AuditQuery::default()` 返回全部（跨动作/actor/时间） |
| `append_refuses_a_new_row_when_over_capacity`（614） | 容量：上限=现值时 `append` → `Unavailable(StorageFull)` 且库内仍只有原 1 行 |
| `appended_rows_are_swept_by_audit_retention`（651） | 保留：`prune` 扫掉过期审计行（`removed_audit == 1`），未到期行仍可查 |
| `combined_filters_and_limit_compose_as_a_conjunction`（486，`fe093b3`） | 全部过滤条件（`since`+`until`+`actions`+`actor`+`target`）与 `limit` 同时在场：动态 SQL 的 `?N` 编号与绑定顺序一一对应；五条无关行各偏离一个条件均被排除；带非空 `WHERE` 绑定时 `LIMIT` 占位符仍正确（见 CT9 的 RED 探针） |

未删除或弱化任何既有断言；`admin_audit` 之外的既有 97 项用例在改动后全部通过（块 (8c) 的 13 条 `test result: ok`）。

## 检查记录（命令 / 退出码 / 日志）

| 检查 ID | 命令（目录 `D:/Project/acp-remote`） | 退出码 | 日志 |
| --- | --- | --- | --- |
| CT1 | `cargo fmt --all -- --check` | 0 | `reports/wp4-storage-audit.log` 块 (1) 与复跑块 (1f) |
| CT2 | `cargo clippy --locked -p storage-sqlite --all-targets --all-features -- -D warnings` | 0 | 同上，块 (2)/(2f) |
| CT3 | `cargo test --locked -p storage-sqlite --all-features` | 0 | 同上，块 (3)/(3b)/(3f)/(8a)/(8c)（新增 14 项 + 既有 97 项全过） |
| CT4 | `node scripts/check-crate-boundaries.mjs` | 0 | 同上，块 (4)：`11 个 crate 的依赖方向与 §5 矩阵一致` |
| CT5 | `npm run check`（10 道门禁，含 `check:drift`） | 0 | `reports/du1-pv1.log` 第三、四轮，均含显式 `EXIT(npm run check)=0`：`contract drift OK: §7 的 36 条 DDL … §5 的 15 个 trait / 87 个方法签名` |
| CT6 | 自检：`rg -n "unsafe" crates/storage-sqlite/src` | 2 命中（**均为 `migrate.rs:780-781` 注释里对该 lint 的说明文字**，本次未改动），新增/改动文件 **0 命中**（块 \((5)/(6b)\) 以退出码 1 证明） | 同上，块 (5)/(6b) |
| CT7 | 自检：`rg -n "unwrap\(|expect\(|panic!" crates/storage-sqlite/src` | 仅 `migrate.rs:1187`（既有 `#[cfg(test)] mod tests`），新增/改动文件 0 命中 | 同上，块 (6)/(6b) |
| CT8 | `git status --porcelain`（每次提交后） | 空 | 同上，块 (7f)；`fe093b3` 提交后同为实测空 |
| CT9 | RED 探针：`LIMIT ?{binds.len() + 1}` 临时改成 `LIMIT ?1` 后跑 `cargo test --test admin_audit combined_filters` | **101（预期失败）**：`Backend(SqlFailure { code: Some("20"), message: "datatype mismatch" })`；还原后同一用例与全套复跑全绿，`git diff --stat -- audit.rs` 为空 | 同上，块 (8b)/(8c) |

关键计数（最终版本 `fe093b3`）：`cargo test -p storage-sqlite --all-features` = **111 项通过**（新增 `admin_audit` **14** 项 + 既有 **97** 项），0 failed，2 ignored（既有 `#[ignore]` 夹具生成器）。任务单的三项 cargo 检查均针对 `-p storage-sqlite` 执行。

> 证据口径说明：本报告的 `EXIT(...)` 只在未加管道的命令行上就是命令自身的退出码；凡把输出送进 `grep`/`tail` 的行，其紧随的 `EXIT(...)` 取的是管道尾命令的退出码，判定依据是输出里的 `test result: ok … 0 failed` / 无 `error`（该口径已写进 `wp4-storage-audit.log` 末尾的「证据口径说明」块，块 (8b) 的 RED 退出码已按测试二进制的失败码 101 更正）。

> 说明（对任务单自检口径的一处偏差）：任务单写「`rg -n "unsafe" crates/storage-sqlite/src` 零命中」，实测**全树**有 2 条命中，但都是 `migrate.rs` 的文档注释在描述 `unsafe_code = "forbid"` 这条 lint 本身，不是代码。本任务新增/改动的两个源文件 0 命中（附块 (6b) 的退出码 1 证据）。**未**为凑「零命中」去改无关文件的注释。

## 未执行项

- 未执行 workspace 级 `cargo test`/`cargo clippy`：任务单只要求 `-p storage-sqlite`。作为替代证据，提交时的 `.husky/pre-commit` 跑了 workspace 级 `cargo fmt --check` + `npm run check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`，输出 `pre-commit: 通过（3 步…）`。
- 未执行 `cargo-deny`（`deps`/`advisories`）与 `gitleaks`（`secrets`）：只在 CI 运行，本机无对应二进制/网络；本任务未新增依赖、未引入凭据。
- 未执行 `audit.export` 的端到端：`crates/app` 组合根尚未落地（`server` 侧仍在增量）。本任务交付的是端口实现与 crate 级行为证据；端到端属任务 2.14 的集成轮。
- 未勾选 `tasks.md` 复选框、未改 `plan.md`/`tasks.md`/`verification.md`/`specs/**`/`docs/**`：不属本 Agent 的写入范围。

## 待澄清 / 残余风险

1. **`query` 只读 `owned_audit`（不跨 `imported_audit`）**：按任务单标题「over owned_audit」与 `admin/mod.rs` 的既有裁定实现。`docs/CORE_PORTS_AND_STORAGE.md:1331` 提到「跨表查询稳定排序，过滤时间与类别」——若主 Agent 认为 `audit.export` 最终**必须**把 Owner 收到的审计元数据也导出，那需要在端口层面决定「来源归属列怎么进 `AuditRecord`」（新增端口/新类型），属合同变更，本 Agent 无此授权，故未做。当前 `imported_audit` 在工作区内也没有任何写入路径（仅建表与迁移）。
2. **`append` 受容量门约束**（决策 7）：超限时独立审计也写不进去（`Unavailable(StorageFull)`）。这与 §7.5 ⑥「宁可拒绝新写入也不静默丢证据」一致，但意味着极端满载时 `authorization.denied` 一类的拒绝审计只能落到结构化日志。若主 Agent 希望审计在容量压力下**无条件**可写（即豁免容量门），需要新的合同口径（现合同把审计排在清理顺序最后，并未给豁免）。
3. **设备 actor 的 scopes 读回为空**（决策 10）：§7.3 没有 scopes 列，属投影而非缺陷；已由用例固定。若将来审计需要「当时的 scopes」，那要扩列 → 走合同与漂移门禁流程，不在本任务范围。
4. **`limit = Some(0)` 返回空**：合同只说「返回行数上限」，`0` 按上限解释（与 `AttachmentStore::sweep_orphans` 的 `limit = 0 不删` 同款「0 是边界而不是不限」口径）。若期望 `0 = 不限`，需合同明确。
5. **新增了一个测试文件**：`tasks.md` 2.22 的「写范围」只列了 `src/admin/audit.rs` 与 `src/admin/mod.rs`，但任务单要求「测试（必测，非跳过）」并指定用「既有 crate 的测试基座」。本 crate 的行为测试基座（`temp_dir`/`raw_pool`/`measured_storage_bytes`）在 `tests/support/`，只能由集成测试使用，因此新增 `tests/admin_audit.rs`（仅测试文件，未改动任何既有文件）。若需要严格按「写范围」两文件收敛，请裁定迁移方式。
6. **同刻多行的 tiebreak 未被独立断言**：`ORDER BY at ASC, audit_id ASC` 的 `audit_id` 部分只由实现保证，现有用例的时间戳互不相同。要覆盖它需两次 `append` 传同一 `at`（可做到）；合同只要求「按 `at` 升序」，同刻顺序对调用方不可观察，因此本次未加该用例。
7. **两个提交而非一个**（任务单写的是一个 `feat(storage)` 提交）：`feat(storage)` 主体 `bb96a10` 已落地且全绿后，在落报告前的复核中补上了一个**混合条件 + `limit` 同时在场**的用例（它约束动态 SQL 的 `?N` 编号与绑定顺序，CT9 的 RED 探针证明它非永真），以 `test(storage)` 的 `fe093b3` 追加而非改写已存在的历史。若需要单提交收口，需在主 Agent 授权的本地重写范围内处理——本 Agent 不自行 rebase/force-push。

## 提交与交付对应

| 提交 | 类型 | 内容 |
| --- | --- | --- |
| `bb96a10` | `feat(storage)` | `crates/storage-sqlite/src/admin/audit.rs`（+265）、`admin/mod.rs`（+1）、`tests/admin_audit.rs`（+679）；3 files changed, 945 insertions(+) |
| `855c8d7` | `docs(storage)` | 本报告初版（不属交付物） |
| `fe093b3` | `test(storage)` | `tests/admin_audit.rs` 追加 `combined_filters_and_limit_compose_as_a_conjunction`（+81/-1） |
| `docs(storage)`（本报告的本次更新） | `docs` | 测试表、计数、检查表（CT9）、提交表与未执行项复述（不属交付物） |

提交用显式路径 `git add`（未 `git add -A`/`.`），未使用 `--no-verify`，未强推，未改写任何既有提交；提交后 `git status --porcelain` 为空（两个 `.log` 被 `.gitignore:27` 忽略，按仓库策略**不**强行入库）。

## 复核（交付提交后）

```text
$ git log --oneline -5
fe093b3 test(storage): 补审计查询的混合条件与 limit 组合用例
f648ecd docs(server): 收敛 RV1-WP3 的 F4/F5 措辞并登记 3.6 结论
855c8d7 docs(storage): 登记 WP4 任务 2.22 的 coder 交接报告
bb96a10 feat(storage): 实现 AuditStore（append/query over owned_audit）
9376e96 docs(server): 登记 2.23 交付
$ git show --stat --oneline bb96a10
 crates/storage-sqlite/src/admin/audit.rs       | 265 ++++++++++++
 crates/storage-sqlite/src/admin/mod.rs         |   1 +
 crates/storage-sqlite/tests/admin_audit.rs     | 679 +++++++++++++++++++++++++++++
 3 files changed, 945 insertions(+)
$ git show --stat --oneline fe093b3
 crates/storage-sqlite/tests/admin_audit.rs | 82 +++++++++++++++++++++++++++++-
 1 file changed, 81 insertions(+), 1 deletion(-)
$ git status --porcelain -- crates/storage-sqlite    # 空（本任务的文件已全部提交）
```

## handoff_index（逐检查 ID 一行）

| 检查 ID | 一行结论 |
| --- | --- |
| CT1 | `cargo fmt --all -- --check` → 退出码 0；`reports/wp4-storage-audit.log` 块 (1)/(1f) |
| CT2 | `cargo clippy -p storage-sqlite --all-targets --all-features -D warnings` → 退出码 0；同日志块 (2)/(2f) |
| CT3 | `cargo test -p storage-sqlite --all-features` → 退出码 0，111 项通过 0 失败（admin_audit 14）；同日志块 (3)/(3b)/(3f)/(8a)/(8c) |
| CT4 | `node scripts/check-crate-boundaries.mjs` → 退出码 0（11 个 crate 与 §5 矩阵一致）；同日志块 (4) |
| CT5 | `npm run check` → 退出码 0（含 `check:drift`：§7 36 条 DDL、§5 15 个 trait/87 个方法签名）；`reports/du1-pv1.log` 第三、四轮均 `EXIT(npm run check)=0` |
| CT6 | `rg -n "unsafe" crates/storage-sqlite/src` → 全树 2 命中（均为 migrate.rs 注释），改动文件 0 命中；同日志块 (5)/(6b) |
| CT7 | `rg -n "unwrap\(|expect\(|panic!" crates/storage-sqlite/src` → 仅 migrate.rs 既有测试 1 命中，改动文件 0 命中；同日志块 (6)/(6b) |
| CT8 | `git status --porcelain` → 空；同日志块 (7f) |
| CT9 | RED 探针（LIMIT 占位符改常量）→ 预期失败 101（`datatype mismatch`），还原后全绿；同日志块 (8b)/(8c) |

## evidence_paths

- `openspec/changes/daemon-cli-and-local-admin/reports/wp4-storage-audit.log`（CT1–CT4、CT6–CT9：块 (1)–(8c)，含每个命令的显式 `EXIT(...)` 行与末尾的「证据口径说明」）
- `openspec/changes/daemon-cli-and-local-admin/reports/du1-pv1.log`（CT5：第三、四轮 `npm run check`，均含显式 `EXIT(npm run check)=0`）
- `crates/storage-sqlite/tests/admin_audit.rs`（14 个用例：合同行为的可执行证据）
