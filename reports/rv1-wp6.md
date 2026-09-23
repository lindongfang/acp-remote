# rv1-wp6.md — WP6（`storage-sqlite` 管理 store）独立对抗性 review

## 0. 检视对象、范围与方法

- 仓库 / 分支：`D:\Project\acp-remote`，`feat/admin-state-persistence-v2`
- 被检视 revision：`git rev-parse HEAD` = `013f2b93a77a31cec3e1a18f589c5e984f013128`（W0 基线提交）+ **未提交工作区**
- `git status --porcelain`（检视时刻）：

```text
 M Cargo.lock
 M crates/storage-sqlite/Cargo.toml
 M crates/storage-sqlite/src/error.rs
 M crates/storage-sqlite/src/lib.rs
 M crates/storage-sqlite/src/session_store.rs
 M openspec/changes/admin-state-persistence-v2/tasks.md
 M openspec/changes/admin-state-persistence-v2/verification.md
?? crates/storage-sqlite/src/admin/            (4 个新文件：mod.rs 194 行、trust.rs 995 行、export.rs 515 行、local_config.rs 422 行)
?? crates/storage-sqlite/tests/admin_store.rs  (2376 行，21 个 #[tokio::test])
```

`?? .omp/`、`?? .pi/prompts/opsx-verify.md`、`?? .pi/skills/openspec-verify-change/` 是宿主安装文件，与本变更无关。（`tasks.md`/`verification.md` 的改动是 W0 记录回填，不在检视范围内。）

- 版本说明：review 过程中作者改了两次工作区——`admin/export.rs` 删除了 `#[allow(dead_code)] fn _types(...)` 死代码与其未使用导入；`tests/admin_store.rs` 的 `full_import_removal_and_connection_drop_both_keep_audit` 扩展出「第二个 Import 不经连接级清空、直接 `remove_import`」的路径。**本报告基于改动后的版本**（两处已重读；`export.rs` 现在以 `same_set` 收尾，无死代码）。
- 检视范围：`crates/storage-sqlite/src/admin/{mod,trust,export,local_config}.rs`、`tests/admin_store.rs`（21 用例）、`session_store.rs` 的未提交 diff（`pools`/`writable`/`window`/`measure_sql`/`blob`/`actor_key`/`enforce_capacity` 可见性与注释）、`error.rs`、`Cargo.toml`、`lib.rs`。
- 对照的权威：`docs/CORE_PORTS_AND_STORAGE.md` §5.3/§7.3/§7.4/§7.5/§8/§9 判据 23–29/§11.1/§11.2/§11.6/§11.7/§11.8、`docs/LOCAL_ADMIN_PROTOCOL.md` §5.4/§6、5 份 spec、`design.md`、`tasks.md`（2.19–2.22）、`verification.md`（WP6 期决定）、`crates/core/src/{ports,use_cases,model/{error,identity,config,export,ids}}.rs`、`crates/storage-sqlite/src/migrate.rs` 的 v2 DDL 常量。
- 只读命令与退出码：
  - `git rev-parse HEAD`、`git status --porcelain` → 0
  - `node scripts/check-contract-drift.mjs` → 0：`contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；§5 的 15 个 trait / 87 个方法签名与 crates\core\src\ports.rs 一致`
  - `node scripts/check-crate-boundaries.mjs` → 0：`crate boundaries OK: 6 个 crate 的依赖方向与 §5 矩阵一致`
  - 一个只读 SQLite 复现（`node -e` + `node:sqlite` 内存库，复刻 `owned_pairing` 的 CHECK 与两条 UPDATE 谓词；未写任何仓库文件）——证实 WP6-1 的两条结论
  - 未运行 `cargo test` / `cargo clippy` / `cargo fmt`（按 assignment：实现者已跑过；本轮价值在读代码找反例）
- 检视范围外：`migrate.rs`/夹具/`tests/{migration,retention,imported,commit,enum_coverage,permissions}.rs`（W0/WP4）、`crates/core/**`（WP1–WP3，已冻结）、`CredentialResolver` 与 `server`/`identity-auth` 接线（尚未实现）。

## 1. 逐条核对结论

### 1.1 写集原子性 —— 成立（只有一处规格明写的「零写入」早返回）

- 16 个管理写路径（`put_device`/`put_node`/`revoke_device`/`revoke_node`/`create_pairing`/`claim_pairing`/`settle_pairing`/`expire_pairings`/`put_export`/`revoke_export`/`add_import`/`remove_import`/`put_profile`/`put_workspace`/`put_provider_ref`/`mark_seeded`）的第一行都是 `self.writable()?`（`grep -c "self.writable()?"` = 4+4+8 = 16），随后 `pools().write.begin_with("BEGIN IMMEDIATE")`。
- **没有事务外的行写入**：全模块 25 处 `.execute(` 全部绑定 `&mut *tx` / `&mut **tx`（逐文件计数 6/5/2/12 与 `.execute(` 总数相等）；没有任何 `execute(&self.pools()…)` 或池上的写；`self.pools().read` 只出现在读路径。
- **没有「先提交再补审计」**：审计在同一个 tx 内、`tx.commit()` 之前写入；`insert_audit_rows`（mod.rs:43-67）失败会让状态一起回滚（用例 `a_failed_audit_write_rolls_back_the_whole_write_set` 以 `BEFORE INSERT ON owned_audit` 触发器证实：workspace 行与审计行都不存在，且移除触发器后同一写集成功）。
- **失败路径真回滚**：`?` 早返回时 `Transaction` 未 `commit()` 即 drop，sqlx 回滚；本 crate 既有路径（`session_store::drop_import` 的 `NotFound`）就是同一写法，属既有约定。
- 唯一的「返回 `Ok` 却零持久化」是 `mark_seeded` 在标记已存在时的早返回（local_config.rs:392-397，提交一个空事务）——规格明写「种子被忽略，且不产生新的写入」，不是缺陷。
- 容量清理不越界：`enforce_capacity` 的 ①→②→③ 只删 `owned_event` 与 `owned_interaction`（session_store.rs:1719/1789/2028 的 DELETE），不碰 `owned_device`/`owned_node`/`owned_peer_key`/`owned_export`/`owned_audit`，因此「不删活动信任、撤销记录与未到期审计」成立。

### 1.2 错误映射 —— 基本成立；一处**可达**的约束失败仍落进 `Backend`（见 WP6-1）

- 8 个约束敏感插入使用 `into_conflict`（error.rs:56-63）：export.rs:351（`put_export` → `AlreadyExists`）、442（管理行 → `AlreadyExists`）、455（关联行 → `DuplicateOwnership`）、local_config.rs:298（workspace → `AlreadyExists`）、368（provider_ref → `AlreadyExists`）、trust.rs:427（`owned_peer_key` → `IdentityMismatch`）、539（`create_pairing` → `AlreadyExists`）、645（claim 的 peer 行 → `AlreadyClaimed`）。
- 其余写（`upsert_device`/`upsert_node`/`insert_audit_rows`、`revoke_*` 与 `expire_pairings` 的 UPDATE、`mark_seeded` 的 `meta` 插入）走通用映射，约束失败 → `PortError::Backend`。可达性判定：core 的构造器已把 DDL 中每个 CHECK/NOT NULL 的相关字段预先校验（`DeviceRecord`/`NodeRecord` 的状态↔`revoked_at`、`owner_endpoint` 与 `kind` 配对；`PairingRecord` 的时间戳↔状态；`ExportRecord` 的默认 alias/template 归属；`ImportRecord` 的 endpoint/非空 export 集合），因此除 WP6-1 的「已批准→拒绝」外都不可达（外部改写库除外）。
- `StorageError::ColumnValue → PortError::Backend` 是本 crate 既有口径（error.rs 映射表），新增读路径沿用；集合字段的 JSON 解析错误走 `Corrupt`（mod.rs:128-133、189-193）✔。
- `ConflictKind`/`UnavailableKind` 取值都被实际使用且语义对齐：`IdentityMismatch`（换钥、角色指纹不一致、撤销复活）、`AlreadyClaimed`、`Expired`/`Consumed`、`DuplicateOwnership`、`AlreadyExists`、`VersionMismatch`（provider 版本不递增）、`Unavailable(StorageFull)`、`Corrupt`（失败关闭）。未发现「通配臂静默吞掉」的实现侧问题：`port_error_public` 的显式分支属 WP2/WP3（`§11.6` 已裁定）。

### 1.3 §11.6 第 1–7 条与本地配置决策 —— 逐条成立（例外见 Findings）

1. `put_device`（trust.rs:325-378）：以**绑定材料的公钥指纹**（336-344）、既有行的指纹与状态（345-350）为判据，材料来源只有「已绑定行」或「既有 `owned_device.public_key`」（353-362），任一处不一致 → `Conflict(IdentityMismatch)`；`revoked` 状态的写入被拒（`InvalidRequest`，撤销只走 `revoke_device`）；`upsert_device`（897-924）的 `DO UPDATE` 不碰 `created_at`，且只可能写入 `pending`/`active`（`revoked_at` 恒 `NULL`），不会清掉撤销标记。`device.scopes_changed` 审计由 core 用例构造写集时给出（use_cases.rs:422-430）。
2. `put_node`（380-434）：写 `owned_node` 行 + `owned_peer_key`（`ON CONFLICT … DO UPDATE`，同 ID 同角色只保留一条材料）；记录指纹必须由同一公钥派生；同 `NodeId` 既有角色逐一比对指纹、任一 `revoked` 即拒（「已撤销不可经普通写入复活」）；双角色共享一条身份材料。
3. `claim_pairing`（548-664）：存在性 → `NotFound`；目标族与对端身份同类；终态 → `Expired`/`Consumed`；非 `created` → `AlreadyClaimed`；`at >= expires_at` → `Expired`；请求集合不得超过登记集合；条件更新 `AND state = 'created'`，0 行时回读给具名冲突；peer 行插入冲突 → `AlreadyClaimed`；状态推进 + peer 行 + `pairing.claimed` 审计同一事务。§11.6 的「本机绑定一致」未实现（见 WP6-4）。
4. `settle_pairing`（668-808）：未认领 → `InvalidRequest`；终态 → 冲突；拒绝只推进终态（不建信任）；批准：过期 → `Expired`、授予集合不得超请求集合、设备配对不对带 grants、既有设备行指纹/撤销检查、写设备行 + 公钥转入 `owned_peer_key`、`approved` 更新、`pairing.approved` 审计、容量门，全部同事务。**缺口：缺少 `pending_confirmation` 守卫（WP6-1）；节点批准失败关闭（WP6-2）。**
5. `revoke_device`/`revoke_node`（436-500）：存在性 → `NotFound`；`COALESCE(revoked_at, at)`/`COALESCE(revoke_reason, …)` 保留首次时间与原因（重复撤销幂等）；按 `NodeId` 的 UPDATE 覆盖两种角色，`owned_peer_key` 保留为 tombstone；不加容量门（安全动作必须可用）。
6. `expire_pairings`（810-855）：只终结 `state IN ('created','claimed','pending_confirmation') AND expires_at <= at` 的行，写 `terminal_at` 与 `COALESCE(claimed_at, at)`，按配对写 `pairing.expired`；已批准的配对与信任不受影响；`context.audit` 只作 actor 出处（缺口 6）。
7. `put_export`/`revoke_export`/`add_import`/`remove_import`（export.rs:316-490）：Export upsert 不覆盖 `created_at`；撤销幂等且保留首次 `revoked_at`；`add_import` 先校验 `exports == record.export_ids()`（集合相等，顺序无关）、重复 import id → `AlreadyExists`、`(owner_node_id, export_id)` 被占用 → `DuplicateOwnership`，管理行 + 全部关联行 + 审计一次提交；`remove_import` 先取关联对（防级联后失联）、删 `imported_session`（级联交付索引/命令引用）再删管理行（级联关联行），`imported_audit` 保留；与 `drop_import` 的删除权威不重叠（drop 只删交付索引/命令引用，且不再写 `removed_at`，session_store.rs:2838-2885）。
8. 本地配置（local_config.rs:237-420）：`put_profile` 先降级旧默认再 upsert（部分唯一索引 `owned_agent_profile_default`），切换是一次调用；绑定必须指向已登记 Provider 的已配置字段；`put_provider_ref` 版本必须严格递增（`VersionMismatch`）；`mark_seeded` 标记存在即零写入返回，首轮导入的 profile 与标记同事务（空列表也写标记），同 ID 冲突 → `AlreadyExists`。

### 1.4 无正文、无秘密 —— 成立

- WP6 唯一的 `imported_*` 写路径是 `add_import`（export.rs:426-457），写入的列只有 `import_id`/`owner_node_id`/`endpoint_ref`/`display_name = NULL`/`'no-content-cache'`/`owner_server_epoch = NULL`/`created_at`/`removed_at = NULL`/`grants_json`；不写 `imported_session`/`imported_delivery_index`/`imported_command_ref`（属 `RemoteDeliveryStore`）。
- 无秘密：`owned_pairing.secret_digest` 只写 `PairingRecord.secret_digest`（用例以 `digest_text(PAIRING_SECRET)` 断言库内是摘要而非明文）；公钥只以 65 字节 BLOB + `PeerPublicKey::fingerprint()` 派生值落库；`owned_provider_ref` 只有字段名 + `keystore_ref` + 版本；`no_secret_material_lands_in_any_column` 用全库全 TEXT 列 `instr` 扫描证实 `PAIRING_SECRET`/`CREDENTIAL_VALUE` 无命中。
- 审计行只写 `action/actor_kind/actor_id/via_node_id/local_principal_ref/target_kind/target_id/outcome/detail_digest`（mod.rs:34-47）；`detail_digest` 的类型是 `Digest`（不可能承载正文）。
- 本机路径只落在 `owned_workspace.canonical_path`（用例 `workspace_records_stay_local_and_keep_created_at` 断言 `columns_containing` 恰好命中该列，Export 侧只有别名）。

### 1.5 规格场景覆盖 —— 见 §3

### 1.6 测试质量

- 断言对象是**可观察结果**：端口返回值（含具名错误分类）+ 库内行（`table_snapshot`/`scalar_i64`/`columns_containing`/`meta_rows`/`column_names`）。没有对内部分支、SQL 文本或私有函数的断言；`provider_reference_version_must_advance` 读 `pragma_table_info` 的列名清单，那是合同冻结的列集合，属合同断言而非实现断言。
- 未发现恒真断言：`assert!(report.delivery_index_removed >= 1)` 依赖前置真实插入的 1 行；具名断言（`assert_conflict`/`assert_invalid_request`/`assert_unavailable`）在分类不符时 panic。
- 无 `#[allow(...)]`、无 `unwrap()`/`expect()`/`panic!`/`todo!` 进入 `src/admin/**`（`grep` 无命中）。
- 两个触发器用例确实走真实事务（触发器在 store 自己的 tx 内触发），但注入点位置不同（见 WP6-3）。
- 偏好级备注：`assert_invalid_request(error, "<精确文案>")` 把断言绑到错误字符串（`InvalidRequest(&'static str)` 是给调用方的具名原因，可接受，但文案一改测试就红）。

### 1.7 实现自述缺口 —— 见 §4

### 1.8 依赖方向与边界 —— 成立

- `Cargo.toml` 新增 `serde.workspace = true` / `serde_json.workspace = true`，只被 `admin/mod.rs`（集合字段编解码）与 `admin/{export,local_config}.rs` 使用；DB record 结构体（`AliasJson`/`TemplateJson`/`ParamJson`/`BindingJson`）是 `pub(crate)`，SQL 与 JSON 形状都不进 core（`acp_core::model/ports` 无 serde 类型）；`Cargo.lock` 只多 2 行、无版本变动。
- `admin` 的依赖只有 `acp_core`、`sqlx`、`serde*` 与 `crate::{error, migrate, session_store}`（`grep "^use "`），没有调用任何其它出站适配器（无 `identity-auth`/`node-link-client`/`agent-host` 引用），未越权。
- `node scripts/check-crate-boundaries.mjs` 退出 0。

## 2. Findings

| ID | 严重度 | 类别 | 位置 | 问题 | 证据 | 建议 |
|---|---|---|---|---|---|---|
| WP6-1 | 高 | 实现缺陷（合同违规：约束失败未具名化 + 状态守卫缺失） | `crates/storage-sqlite/src/admin/trust.rs:717-718`（另一处同形：792-793） | `settle_pairing` 只排除**终态**（`state NOT IN ('rejected','expired','consumed')`），而已 `approved` 的配对不是终态；因此（a）重复批准被接受——`approved_at` 被改写成后一次时间、`owned_device` 行被再次 upsert（`last_seen_at` 被重置为 NULL）、追加第二条 `pairing.approved` 审计；（b）对已批准配对执行**拒绝**落定时，UPDATE 把 `state` 改成 `'rejected'` 却保留非空 `approved_at`，违反 `owned_pairing` 的 `CHECK ((state IN ('approved','consumed')) = (approved_at IS NOT NULL))`，整个写集以 `PortError::Backend` 失败（不是具名冲突），配对停在 `approved`。触发条件：同一 pairing 的第二次 `settle_pairing` 调用（`device.pair.confirm`/`node.pair.confirm` 重试或在批准后点拒绝）；`UseCases::settle_pairing`（core/src/use_cases.rs:580-601）只做 `settlement.validate()`，不做状态前置校验，故端口是该状态的唯一守卫。 | 谓词与 `claim_pairing` 的正向谓词（trust.rs:626 `AND state = 'created'`）不对称；`PairingState::is_terminal()` = `Rejected|Expired|Consumed`（core/src/model/identity.rs:418-420）不含 `Approved`；只读复现（`node:sqlite`，复刻该 CHECK 与两条 UPDATE 谓词）：`reject-after-approve: UPDATE FAILED -> CHECK constraint failed: (state IN ('approved','consumed')) = (approved_at IS NOT NULL)`、`approve-again: UPDATE OK`，行变为 `{"state":"approved","approved_at":"t9"}`（首次时间被覆盖）。 | 两处 UPDATE 谓词改为 `AND state = 'pending_confirmation'`（保留「已认领且未落定」的唯一合法前态）；或在加载记录后对 `PairingState::Approved` 直接返回 `PortError::Conflict(ConflictKind::Consumed)`，使重复落定显式失败而不是改时间或触发 CHECK。 |
| WP6-2 | 高（阻断，修在合同侧） | 合同缺口（冻结写集不足以实现 `§11.6` 第 4 条） | `crates/storage-sqlite/src/admin/trust.rs:784-789` | 节点配对的**批准**分支硬失败（`InvalidRequest`，零写入），于是「批准节点配对」在存储层不可达：配对行停在 `pending_confirmation`、没有 `pairing.approved` 审计、也没有节点信任记录，而 `LOCAL_ADMIN_PROTOCOL.md:369-373` 规定 `node.pair.confirm`「只有 Owner 侧本地确认才创建信任记录并分配初始 `grant.*`；必须在持久状态提交后才返回」。根因是 `PairingSettlementWrite` 只携带 `pairing` + `settlement`（core/src/ports.rs:706-712），`PairingRecord` 不含角色，`owned_node.kind` 是必填列，存储层无法凭空取得 `NodeKind`。触发条件：任何 `PairingTarget::Node` 的 `Approved` 落定。 | trust.rs:784-789 的返回；tasks.md 2.19 要求「设备与节点双角色、配对认领/落定/过期」；`verification.md` 第 16 条把「批准节点配对必须先经 `UseCases::put_node`」记为决定，但该路径不能让配对进入 `approved`，未描述完整流程；`node.pair.confirm` 在 `LOCAL_ADMIN_PROTOCOL.md:167` 的能力表里没有 `post_mvp` 标记。 | 由合同/端口侧闭合（二选一）：① `PairingSettlementWrite`（或 `PairingRecord`）携带 `NodeKind`，存储层在批准时写 `owned_node` 行 + `owned_peer_key`；② 若确定节点批准**不经** `settle_pairing`，则同步改 `LOCAL_ADMIN_PROTOCOL.md` §5.4、§11.6 第 4 条与 `UseCases::settle_pairing` 的文档，并给出「`put_node` + 配对状态推进」的完整流程（含 `pairing.approved` 与审计）。完成后 WP6 复验该分支。 |
| WP6-3 | 低 | 偏好（测试加固） | `crates/storage-sqlite/tests/admin_store.rs:2083-2112`（对照 2010-2046） | 「整事务回滚」缺一条最强证据：批准路径的故障注入（`BEFORE INSERT ON owned_device`）在**事务的第一次写入之前**中止，因此断言「设备/身份材料不存在、配对仍是 `pending_confirmation`」无法区分「整体回滚」与「还没写到那一步」；而另一条注入用例把审计触发器加在 `put_workspace` 上，不是 spec 场景列举的「设备撤销、节点撤销或配对确认」。 | `settle_pairing` 的设备批准分支第一条写语句是 `upsert_device`（trust.rs:758-776，其前只有 `device_identity` 读）；`put_workspace` 的审计注入（tests:2010-2046）确实覆盖了「先写状态、后写审计失败 → 状态一起回滚」。 | 把审计触发器注入到 `settle_pairing` 的批准路径（或在 `owned_peer_key` 上注入），断言设备行、`owned_peer_key` 行、配对状态与审计行都停在调用前；两条用例的注入点各覆盖「状态先写」与「审计后写」的一半，合起来才排除「把某些写放到事务外」这一类 bug。 |
| WP6-4 | 中 | 合同缺口 + 记录不一致 | `crates/storage-sqlite/src/admin/trust.rs:529-530`（声明见 6-12 行） | `owned_pairing.host_binding` 恒写空串，且整条认领/落定路径没有任何本机绑定比对，因此 `§11.2` 第 1 条的「本机绑定一致」与 `§7.3`（`docs/CORE_PORTS_AND_STORAGE.md:973`：`host_binding TEXT NOT NULL -- 设备：canonical origin；节点：owner endpoint`）/`§11.1`（「`owned_pairing` … 包含本节点身份/origin 或 endpoint 绑定」）在实现里没有落点：列恒为 `''`，重启后也无从校验。另外，该偏差**未登记**在 `verification.md` 的 Check Plan Changes（`grep -n "host_binding\|本机绑定"` 在 verification.md/tasks.md/design.md 均无命中），而 trust.rs:6-8 声称「两条**记录在案**的合同缺口 … 见 `verification.md` 的 Check Plan Changes」——缺口 2（节点批准）登记为第 16 条，缺口 1 没有登记。 | trust.rs:9-12、529-530；`docs/CORE_PORTS_AND_STORAGE.md:973`、§11.1 表、§11.2 第 1 条；`CreatePairing`/`PairingClaimWrite` 不携带绑定的字段（core/src/ports.rs:660-690）。 | 合同侧闭合：把本机绑定并入 `PairingWrite`/`PairingClaimWrite`（或 `PairingRecord`）并在认领/落定时比对；若确定绑定校验归调用方且不持久化，则同步修改 §7.3 的列注释与 §11.1 的表述（列承载什么、谁校验），并在 `verification.md` 的 Check Plan Changes 补上这一条决定——否则冻结的 DDL 与冻结的写集互相矛盾且无人记录。 |

## 3. 规格场景覆盖

| 规格 / 场景 | 对应用例 或 范围判定 |
|---|---|
| **admin-state-persistence**：审计写入失败时状态不落库 | `a_failed_audit_write_rolls_back_the_whole_write_set`（机制同一，但注入点是 `put_workspace` 而非场景列举的撤销/确认写集，见 WP6-3） |
| 约束冲突时不留下半条授权 | `a_constraint_failure_during_approval_leaves_no_half_authorization` |
| 并发认领只有一个成功 | `second_claim_of_the_same_pairing_is_rejected` |
| 拒绝或过期不创建信任 | `rejected_settlement_creates_no_trust_and_ends_the_pairing`、`expired_pairing_is_refused_on_claim_and_terminated_by_restart_sweep`。子分支缺口（低）：`settle_pairing(Approved)` 对「已过期但尚未被扫描」的配对返回 `Expired`（trust.rs:734-737）无用例；行为明确且失败关闭 |
| 重启后撤销仍然有效 | `device_revocation_is_idempotent_and_survives_reopen`、`node_revocation_covers_both_roles_and_survives_reopen` |
| 重启终结已过期的未确认配对 | `expired_pairing_is_refused_on_claim_and_terminated_by_restart_sweep`（store 行为）；「重启时调用 `expire_pairings`」的接线在 WP6 之外（`UseCases::expire_pairings` 已存在，daemon 组合根属后续 WP） |
| 同一 Export 归属冲突被拒 | `import_ownership_is_exclusive_and_write_set_divergence_is_rejected` |
| 完整移除后审计仍在 | `full_import_removal_and_connection_drop_both_keep_audit` |
| 未登记动作无法写入 | WP6 范围外：`AuditAction` 是闭合枚举（不可构造未登记取值），DDL CHECK 的逐值断言在 W0 的 `tests/enum_coverage.rs` |
| 失败请求的审计不含内容 | 等价覆盖：`rejected_settlement_…`（写入一条 `PairingRejected`/`Denied` 审计）+ `no_secret_material_lands_in_any_column`（全库全 TEXT 列扫描）；无「失败/拒绝审计行的列级」专门断言 |
| **peer-identity-material**：接受合法未压缩公钥 / 拒绝压缩点与非法长度 | WP6 范围外（core 值对象与 `crates/core/src/model/tests.rs`）；WP6 侧只复用 `PeerPublicKey::try_from_bytes`（trust.rs:190-197） |
| 认领后重启仍能取到验签公钥 | `pairing_approval_persists_identity_material_across_reopen`（逐字节比较 + 重启） |
| 指纹与公钥不一致无法落库 | `second_node_role_with_a_foreign_fingerprint_is_rejected`（节点）、`revoked_or_rekeyed_device_cannot_be_reactivated_or_rebound`（设备） |
| 双角色共享身份材料 | `node_roles_share_one_identity_material` |
| 按节点撤销覆盖两种角色 | `node_revocation_covers_both_roles_and_survives_reopen` |
| 角色指纹不一致被拒 | `second_node_role_with_a_foreign_fingerprint_is_rejected` |
| 已绑定身份换钥被拒 | 设备：`revoked_or_rekeyed_device_cannot_be_reactivated_or_rebound`。缺口（低）：**节点**换一枚自洽的新公钥（记录指纹 == 新公钥指纹、但与已绑定材料不同 → `load_peer_key` 分支 trust.rs:405-409）无用例；该分支同样返回 `IdentityMismatch` |
| 已撤销身份不能经普通写入复活 | `device_revocation_…`、`node_revocation_…` |
| 库内不出现秘密材料 | `no_secret_material_lands_in_any_column` |
| **local-agent-config**：默认 profile 唯一且切换原子 | `default_profile_switch_is_atomic_and_unique` |
| 非法绑定在写入时被拒 | `profile_bindings_must_reference_registered_provider_fields`（Provider 存在性与字段存在性）；白名单/重复绑定/保留名由 core 构造器拒（范围外） |
| 空种子也标记已初始化 | `seed_marks_initialized_once_and_ignores_later_seeds` |
| 重复打开不重导种子 | `seed_marks_initialized_once_and_ignores_later_seeds`（profile/meta 快照逐行相等 + Provider 表为 0） |
| 引用失效即不可用 / 白名单是上限 / 引用失效时失败关闭 / 日志只记录变量名与数量 | WP6 范围外（`CredentialResolver` + keystore，属后续 WP；WP6 只持久化引用与字段名） |
| 引用版本推进 | `provider_reference_version_must_advance` |
| 使用不存在目录时明确失败 | WP6 范围外（使用期目录校验属 §5.1 的解析路径；WP6 只持久化 `owned_workspace` 记录） |
| 远程目录不泄漏本机路径 | `workspace_records_stay_local_and_keep_created_at`（库内只落 `canonical_path`、Export 侧只有别名）；catalog/事件侧范围外 |
| **storage-schema-v2-migration**：连续两次打开 schema 文本不变 / 升级中途失败整体回滚 / 升级后重放与幂等仍一致 / 缺少可信来源的 Import 保持不可用 / 过新库零写入 / 黄金列清单逐项相等 / 投递正文后库内无正文 | WP6 范围外（W0 的 `migrate.rs`、v2 夹具与 `tests/{migration,imported,commit,enum_coverage}.rs`；`RemoteDeliveryStore` 的无正文用例为既有测试） |
| 损坏库的写路径全部被拒 | `fail_closed_store_rejects_every_admin_write_path`：抽样 6/16 条管理写路径（`put_workspace`/`put_profile`/`create_pairing`/`put_export`/`add_import`/`mark_seeded`）+ 只读查询仍可用。其余 10 条由每条写路径首行的 `self.writable()?` 保证（本轮逐条读代码核实，见 §1.1）；补测可覆盖 `put_device`/`put_node`/`revoke_*`/`claim`/`settle`/`expire`/`revoke_export`/`remove_import`/`put_provider_ref` |
| 权限过宽在 Unix 上失败关闭 | WP6 范围外（既有 `tests/permissions.rs`） |
| 超限时拒绝新写入而不是删信任 | `capacity_limit_refuses_new_admin_writes_without_deleting_trust` |
| 度量包含管理表分量 | 同用例：预算由 `measured_storage_bytes` 从**只有管理数据**（device/pairing/audit）的库派生，若 store 的度量恒为 0 则写入不会被拒、用例失败；度量语句本身从 `sqlite_master` 现读全部 TEXT 列（session_store.rs:635-668），管理表自然计入 |
| 清空与移除都保留审计 | `full_import_removal_and_connection_drop_both_keep_audit`（含 `remove_import` 单独带走交付索引/命令引用的新增路径） |
| **workspace-resolution** 全部 7 个场景 | 全部 WP6 范围外：别名 → 规范化路径的解析归 core（§5.1/§3.6 的 `ResolvedWorkspace` + `create_session`），WP6 只承载 `owned_workspace` 的持久化 |

## 4. 实现自述缺口的判断

| # | 自述缺口 | 判断 | 合同依据与理由 |
|---|---|---|---|
| 1 | `owned_pairing.host_binding` 写空串 | **必须修（合同侧）** | §11.1 表（`owned_pairing`「包含本节点身份/origin 或 endpoint 绑定」）、§7.3 列注释（设备 canonical origin / 节点 owner endpoint）、§11.2 第 1 条（认领必须检查「本机绑定一致」）。冻结写集不携带该事实 → 任何实现都无法写入真实绑定，也无法校验；调用方虽可重算本机 origin，但库内没有可审计的绑定。另：该偏差未登记在 `verification.md`（代码注释却声称「记录在案」），见 WP6-4 |
| 2 | 节点配对**批准**返回 `InvalidRequest` | **必须修（合同侧，阻断）** | §11.6 第 4 条（`Approved` 时创建信任行、把 peer 公钥转入 `owned_peer_key`、更新配对为 `approved`、写 `pairing.approved`）与 `LOCAL_ADMIN_PROTOCOL.md` §5.4 的 `node.pair.confirm` 在本实现下不可达；fail-closed 本身正确（绝不允许「配对已批准但没有持久信任」），但 tasks.md 2.19 的「设备与节点双角色、配对认领/落定/过期」因此对节点不成立，且 `verification.md` 第 16 条没有给出可工作的替代流程 |
| 3 | 种子标记落在 `meta.local_config_seeded_at` | 可接受 | `meta` 是 §7.3 的库级键值表（`key TEXT PRIMARY KEY, value TEXT NOT NULL`，无键白名单），新增键不改表结构、不动漂移门禁断言；§11.8 第 4 条只要求「种子与标记同事务提交」——`mark_seeded` 满足（空列表也写标记、已初始化时零写入返回）。唯一代价是 `meta` 成为「版本键 + 业务键」混合表，读取方需按前缀/常量区分 |
| 4 | `expire_pairings` 对从未被认领的配对回填 `claimed_at` | 可接受 | 冻结 DDL 的 `CHECK ((state = 'created') = (claimed_at IS NULL))` 使「离开 `created`」必须同时给 `claimed_at`；§11.6 第 6 条要求终结未确认且已过期的行。语义已在代码注释与 `verification.md` 第 13 条记录为「离开 `created` 的时刻」。副作用是 `claimed_at` 不再等价于「被认领」——合同未把该列定义为「被认领时刻」，但读取方（后续 WP）不应据此判断认领状态 |
| 5 | 容量门只加在「会新增行」的管理写路径 | 可接受 | §7.5/§11.2 第 4/5 条要求撤销先提交再阻断访问；`enforce_capacity` 的 ①→②→③ 只删 `owned_event`/`owned_interaction`，不碰信任/撤销/审计（§9 判据 29），故「超限拒绝新写入而不是删信任」成立（用例证实新写入被拒、撤销仍可用）。代价是撤销/移除在长期满容量下仍会写审计行——这正是合同要求的方向 |
| 6 | `ExpiryWrite.context.audit` 只作出处、不整体追加 | 可接受（建议加固） | §11.6 第 6 条把 `pairing.expired` 的写入定在扫描路径（按配对一行），core 的唯一调用方传空（use_cases.rs:605-615），因此当前没有静默丢弃真实数据；但 DTO 形状（`WriteContext.audit`）对将来的接入方暗示「审计会随写集落库」。建议对「非空且动作不是 `pairing.expired`」的 `context.audit` 返回 `InvalidRequest`，或把「本写集忽略 `context.audit`」写进 §11.6 |

## 5. 结论

- **有 1 条阻断项（修在合同/端口侧）**：WP6-2 —— 节点配对的批准路径在存储层不可达（`trust.rs:784-789` 返回 `InvalidRequest`、零写入），使 `node.pair.confirm`（`LOCAL_ADMIN_PROTOCOL.md` §5.4：必须创建信任记录且提交后才返回）与 §11.6 第 4 条对节点不可达。修复面是冻结的 `PairingSettlementWrite`（需携带节点角色）或 `settle_pairing` 的职责定义，不在 WP6 的四个新文件内；WP6 自身在该点的失败关闭是正确的（不留半条授权），修复后需复验该分支。
- **WP6 工作区本身无未解决阻断项**：写集原子性（16/16 条写路径单事务、25/25 条 `execute` 绑定事务、审计与状态同事务、失败整体回滚）、错误映射（8 处具名冲突映射 + 失败关闭）、无正文与无秘密、依赖方向与边界（`serde` 只在适配器内、record 不进 core）均与合同一致；`node scripts/check-contract-drift.mjs` 与 `node scripts/check-crate-boundaries.mjs` 均退出 0。
- **建议本轮一并闭合**：WP6-1（`settle_pairing` 补 `state = 'pending_confirmation'` 守卫；这是唯一**可达**的「约束失败落进 `PortError::Backend`」路径，并会改写 `approved_at`）、WP6-4（`host_binding`：合同侧补字段或改表述，并在 `verification.md` 登记该决定——目前代码注释声称「记录在案」但与 `verification.md` 不符）；WP6-3 为低优先级的测试加固。
- 检视基线：`013f2b9`（W0 基线）+ 上述工作区改动（时间点见 §0）；本次未运行 `cargo` 测试或 lint，未修改任何仓库文件（唯一写入是本报告）。
