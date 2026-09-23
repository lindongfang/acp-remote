<!-- 承接 proposal 的动机、范围与影响，说明如何实现以及技术决策的理由。
     本变更不改 DDL、端口签名与 wire schema，因此设计聚焦四条写路径守卫与读取期校验的落点。 -->

## Context

只读核对的事实（2026-09-23，`HEAD` `1ef6640`，工作区干净，`cargo metadata` 可运行）：

- `crates/storage-sqlite/src/session_store.rs:2629` `upsert_session`：`INSERT ... ON CONFLICT(owner_node_id, export_id, session_id) DO UPDATE`，事务内只查 `last_origin_epoch`，**不查** `imported_import_export`；`crates/storage-sqlite/src/admin/export.rs:465` `remove_import` 在同一事务删 `imported_session`（级联 `imported_delivery_index`/`imported_command_ref`）与 `imported_import`（级联关联行）。因此删除后到达的回调能重新插入会话行，其后的 `commit_receipt`（`session_store.rs:2438`）只要求会话行存在即可继续分配 `local_sequence`。
- `crates/storage-sqlite/src/admin/export.rs:316` `put_export` 的 `DO UPDATE` 含 `revoked_at = excluded.revoked_at`，而 `revoke_export`（`:359`）用 `COALESCE(revoked_at, ?2)` 保留首次撤销时间——两处语义相反。`UseCases::put_export`（`crates/core/src/use_cases.rs:650`）注释覆盖 `export.create`/`export.update`，但 `LOCAL_ADMIN_PROTOCOL.md` §5.5 与 `compatibility/commands/v1/commands.json` 只登记 `export.create`/`export.list`/`export.revoke`。
- `crates/storage-sqlite/src/admin/trust.rs:227` `load_peer_key` 只 `SELECT public_key`，不读同行的 `fingerprint`，其文档注释明写「指纹列不参与判定——权威是公钥字节本身」；`pairing_peer_from_row`（`:156`）同样不读该列（模块头未涉及该口径）。`DEVICE_COLUMNS`（`:43-44`）不含 `public_key`，`device_from_row`（`:63`）因此无法校验设备行的指纹；`load_existing_device_key`（`:1090`）只在 `put_device` 缺绑定材料时兜底。
- `crates/storage-sqlite/src/admin/trust.rs:1003` `upsert_device` / `:1034` `upsert_node` 用 `COALESCE(excluded.x, x)`；`docs/CORE_PORTS_AND_STORAGE.md` §3.2 冻结 `Timestamp` 为固定宽度 `%Y-%m-%dT%H:%M:%S%.3fZ`，「TEXT 排序即时间序」。SQLite 标量 `max(a, b)` 在任一参数为 NULL 时返回 NULL（聚合 `max` 才会忽略 NULL），因此「`MAX(COALESCE(新值, 旧值), 旧值)`」在旧值为空时会写入 NULL。
- DDL 事实：`owned_device.public_key`/`owned_peer_key.public_key`/`owned_pairing_peer.public_key` 都是 65 字节 BLOB，`fingerprint` 为 64 字符小写 hex，三者都只有各自列上的 CHECK（长度/字符集），**没有任何跨列 CHECK** 把 `fingerprint` 与 `public_key` 绑定；因此「指纹与公钥互相矛盾」只能由读取期判定。
- 既有测试基线：`crates/storage-sqlite/tests/admin_store.rs:1859` `timestamps_are_advanced_never_erased`（只覆盖 Some→Some 与 Some→None），`:2039` `full_import_removal_and_connection_drop_both_keep_audit`（移除后只断言行集为空，不触发回调），`:731` `an_empty_stored_binding_is_reported_as_corrupt`（读取期 `Corrupt` 的既有先例）；`crates/storage-sqlite/tests/imported.rs:281` `receipt_without_imported_session_is_rejected`。

约束：不改 DDL/迁移、端口签名、值对象与 wire（`AGENTS.md` §4/§6/§10）；`scripts/check-contract-drift.mjs` 只读 §5.1–§5.4 的 ```rust 块与 §7.3/§7.4 的 ```sql 块，因此文档改动必须落在这些 fenced 块之外。

## Goals / Non-Goals

**Goals:**

- 四条缺陷都在既有写事务内闭合，错误映射复用既有词汇，不新增 `ConflictKind`/`UnavailableKind`/错误码。
- 失败关闭先于写入：非法状态或损坏材料不产生任何部分写入，也不返回可用于信任判定的材料。
- 活动时间在任一侧为空时仍按「只前进」语义落库（尤其旧值为空时的首次写入）。
- 四条修复都有可证伪的回归用例，且都能被现有定向与全量检查覆盖；文档同步后 `npm run check` 全绿。

**Non-Goals:**

- 不引入 Import 实例标识、import generation 或连接代际字段（需新字段 + DDL + 迁移 + 新用例面，属用户决策项），因此同一 `(ownerNodeId, exportId)` 被重新导入后旧连接的迟到回调仍会通过归属检查。
- 不新增 `export.update` 方法或改本地管理方法集/错误映射，不改 `LOCAL_ADMIN_PROTOCOL.md`。
- 不改 `SessionStore` 的 owned 家族、不改审计取值、保留策略、容量门与 `prune`。
- 不为既有损坏数据提供自动修复或迁移；只在读取时失败关闭。

## Decisions

### D1 imported 写路径的归属守卫（WP2）

- 在 `upsert_session` 与 `commit_receipt` 的 `BEGIN IMMEDIATE` 事务内、执行任何写入之前，先执行
  `SELECT 1 FROM imported_import_export WHERE owner_node_id = ?1 AND export_id = ?2`；查不到即以
  `PortError::NotFound(EntityRef::Export(session.export_id.clone()))` 返回，事务回滚、零写入。
- `commit_receipt` 也加这一步，而不是只依赖「会话行存在的复合外键」：`upsert_session` 被守住后外键已足够，
  但同一不变量的两个入口各写一次读才是可独立测试的守卫；两处都以 `(ownerNodeId, exportId)` 为判据，
  与 `imported_import_export` 的 `UNIQUE (owner_node_id, export_id)` 语义一致。
- 错误定位取 `EntityRef::Export(exportId)`（而非 `Import`）：调用方只持有 `RemoteSessionRef`
  （`ownerNodeId`/`exportId`/`sessionId`），关联行缺失时无法回推出 `importId`（`imported_import` 行已删、
  关联行已级联删除；靠审计行反查会把审计当权威），而 `Export` 变体恰好能表达「这个对端 Export 在本机
  不再归属任何 Import」。返回 `NotFound` 与 §11.2 第 5 条「在途回调必须被拒绝」一致。
- 否决「在 `upsert_session` 里 `JOIN imported_import` 后 `INSERT ... SELECT`」：错误分类会退化成
  `rows_affected == 0`，无法区分「归属缺失」与「后续约束冲突」，且读路径仍要单独写一遍。
- 否决「只在 `commit_receipt` 加守卫」：`upsert_session` 本身就重建了被删除的会话行（用户报告的第一句），
  只守收据会留下「会话行被悄悄重建」的部分复活。
- 残留缺口（用户复核指出，本变更不闭合）：同一 Owner/Export 重新导入后，`(ownerNodeId, exportId)`
  再次拥有归属行，旧连接的迟到回调与新连接无法区分。闭合方向是「导入实例标识」或
  「连接代际 / `imported_session.attachment_generation` 校验」，后者依赖尚未实现的 `node-link-client`
  的 attachment 语义；本变更只把守卫写成 `(ownerNodeId, exportId)` 级别并在文档与风险中显式记录该边界。

### D2 Export 撤销终态（WP1）

- `put_export` 的检查顺序（严格按用户复核意见）：
  1. `record.revoked_at().is_some()` → `PortError::InvalidRequest("use export.revoke to revoke an export")`
     （与 `put_device` 拒绝 `revoked` 记录的既有措辞风格一致：撤销只走专用写集），整事务零写入；
  2. 已存行 `revoked_at IS NOT NULL` 且传入记录未撤销 → `PortError::Conflict(ConflictKind::AlreadyExists)`；
  3. 其余情况照常 upsert，SQL 把该列写成
     `revoked_at = CASE WHEN owned_export.revoked_at IS NOT NULL THEN owned_export.revoked_at ELSE excluded.revoked_at END`
     ——已撤销行的撤销时间永不被覆盖（重复 revoke 幂等语义与 `revoke_export` 的 `COALESCE` 对齐）。
- 第 1 步先于第 2 步，保证「经 `put_export` 试图自行撤销」在任何库状态下都是参数类拒绝，而不是被
  「已存在」冲突掩盖；第 2 步覆盖本次报告的漏洞（传入 `revoked_at = None` 清除撤销标记）。
- `ConflictKind::AlreadyExists` 是既有取值（`docs/CORE_PORTS_AND_STORAGE.md` §2 已登记，本地管理适配器映射
  为 `local.conflict`），复用它可以避免封闭词表、`ALL`/`as_str`、`port_error_public` 显式分支与文档表格
  的连带改动；语义上「该 `exportId` 已存在且不可被本次写入改写」与同名的既有用法一致。
- 不新增对外方法：本次只修 `put_export` 端口自身的漏洞（用户复核意见），不为未来可能的 `export.update`
  预留行为。
- 否决「静默保留 `revoked_at` 并返回成功」：调用方写出未撤销记录却读到已撤销行，会让配置/CLI 侧形成
  静默分歧，违反 `AGENTS.md` §3「不能静默」的原则；失败关闭让调用方明确看到该 Export 已终止。

### D3 身份材料读取核对同行指纹（WP1）

- 三个读取点分别在同一 `SELECT` 里取回 `public_key` 与 `fingerprint` 两列，用
  `PeerPublicKey::fingerprint()`（`§3.5` 的唯一指纹入口，适配器不自行现算）比对：
  - `load_peer_key`（`owned_peer_key`）：不一致 → `StorageError::Corrupt("owned_peer_key fingerprint does not match its public key")`（映射为 `PortError::Corrupt`）；
  - `pairing_peer_from_row`（`owned_pairing_peer`）：`PAIRING_PEER_COLUMNS` 已包含 `fingerprint`，只补比对；
  - `device_from_row`（`owned_device`）：把 `public_key` 加入 `DEVICE_COLUMNS` 并在映射函数内比对；`device()`/`devices()`/`load_device` 共用它，因此单读与列表读都失败关闭（用户复核要求）。
- `load_existing_device_key`（写设备时的兜底来源）同样改为同 `SELECT` 取两列并比对，使写路径的损坏分类也是
  `Corrupt` 而不是「指纹与写集不一致 → `Conflict(IdentityMismatch)`」的误导性分类。
- 选择 `Corrupt` 而非 `Conflict`：这里不是「请求与已存状态冲突」（请求可能完全正确），而是持久化材料自身
  自相矛盾；`§8` 的失败关闭条款与 `pairing_from_row`/`pairing_peer` 的既有 `Corrupt` 先例都指向它。
- 同时修正 `trust.rs` `load_peer_key` 文档注释里「指纹列不参与判定」的口径（模块头无需改动，RV1-F2）：新规则是「指纹列不作权威
  来源、由公钥派生，但读取时必须核对，不一致即失败关闭」。
- 否决「只在 `load_peer_key` 补比对」：用户复核已指出 `load_existing_device_key` 覆盖不到普通读取，
  设备记录读取路径必须一并纳入，否则「损坏的设备记录一经读取就失败关闭」不成立。
- 否决「在 DDL 上加跨列 CHECK」：SQLite 不能在既有表上加 CHECK 而不重建表（§7.2 的 12-step 重建成本），
  且 `fingerprint` 必须由公钥派生（SQL 无法计算 SHA-256），只能在应用层判定。

### D4 活动时间单调不减（WP1）

- `upsert_device`/`upsert_node` 的 `DO UPDATE` 把该列改为显式 `CASE`（两个函数各自一份，形状相同）：
  ```sql
  last_seen_at = CASE
      WHEN excluded.last_seen_at IS NULL THEN last_seen_at
      WHEN last_seen_at IS NULL THEN excluded.last_seen_at
      WHEN excluded.last_seen_at > last_seen_at THEN excluded.last_seen_at
      ELSE last_seen_at
    END
  ```
  `last_connected_at` 同形。三条分支分别对应「新值为空保留旧值」「旧值为空写入新值」「两者非空取较大者」，
  结果要么是 NULL（两者为空）要么是两者之一，不会因为 NULL 参与比较而丢值。
- 比较用 TEXT 字典序：`Timestamp` 是固定宽度 `%Y-%m-%dT%H:%M:%S%.3fZ`（§3.2 明确「TEXT 排序即时间序，
  因此该宽度是硬约束」），因此无需解析即正确。
- 否决「`MAX(COALESCE(excluded.x, x), x)`」：SQLite 标量 `max(a, NULL)` 返回 NULL，旧值为空时会把首次写入
  丢成 NULL（用户已在内存 SQLite 上复现）；`coalesce(max(...), ...)` 之类的补丁只会增加分支而降低可读性。
- 否决「改回 `COALESCE(excluded.x, x)` 再加应用层比较」：需要额外一次读并把比较逻辑移出 SQL，多一次往返
  且与设备/节点两条路径重复。
- 行为选择「成功但保留较晚值」而非返回错误：该列是审计性的最近活动判据，迟到或旧快照写入不应让整个
  写集（含审计与其它字段）失败；`AGENTS.md` §3 的「不能静默」针对能力与状态语义，这里语义被显式定义为
  「最近 = 最大值」。

### D5 回归用例设计（WP1/WP2）

- `tests/imported.rs`（WP2）：新增「Import 完整移除后 `upsert_session` 与 `commit_receipt` 都被拒」——
  先 `add_import` + `upsert_session` + `commit_receipt` 建出基线，再 `remove_import`，随后
  (a) 同一 `RemoteSessionRef` 的 `upsert_session` 断言 `NotFound(Export)`；(b) `commit_receipt` 断言
  `NotFound(Export)`；(c) 用 raw pool 断言三张 imported 表为空且 `imported_audit` 计数不变。
  与既有 `receipt_without_imported_session_is_rejected`（「归属行在、会话行不在」→ `NotFound(Session)`）区分开：
  该用例已改为先 `seed_import` 并把断言收窄为 `NotFound(EntityRef::Session)`（否则新守卫会先返回
  `NotFound(Export)`、失去对「会话行不存在」分支的定向覆盖，RV1-F1）；新用例覆盖「无归属行」（从未导入
  与完整移除后在端口层不可区分，走同一条 `NotFound(Export)` 路径）。
- `tests/admin_store.rs`（WP1）：
  - Export 撤销终态：`put_export` → `revoke_export` → 再 `put_export` 未撤销记录断言 `Conflict(AlreadyExists)`
    且 raw pool 读回首次 `revoked_at` 不变；再补一例「`put_export` 携带已撤销记录 → `InvalidRequest` 且零写入」（含 `owned_export` 行数/内容不变的断言）。
  - 身份材料：用 raw write pool 把 `owned_peer_key.fingerprint` 改成另一枚合法指纹 → `peer_key` 返回
    `Corrupt`；同类改写 `owned_device.fingerprint` → `device()` 与 `devices()` 都返回 `Corrupt`；
    改写 `owned_pairing_peer.fingerprint` → `pairing_peer()` 返回 `Corrupt`；并保留一条「指纹一致时读取照常」的正向断言，避免整组用例变成恒真。
  - 活动时间：扩展现有 `timestamps_are_advanced_never_erased`（或新增同级用例）覆盖三方：
    空→Some 必须落库（正是被 NULL 吞掉的分支）、Some(晚)→Some(早) 保留晚值、Some→None 保留 Some；
    设备与节点各一遍，并断言「更早写入不影响同写集的其他字段」（例如 `display_name`/`grants` 仍更新）。
- 用例只断言可观察结果（端口返回值 + raw pool 读回的行），不复制实现的判定逻辑；「零写入」一律用行数/内容比对而不是只看错误类型。

### D6 文档同步点（WP3）

- `docs/CORE_PORTS_AND_STORAGE.md` 的改动只在 fenced 代码块**之外**：
  - §5.2 约束列表：`upsert_session`/`commit_receipt` 必须先验 `(ownerNodeId, exportId)` 归属并说明错误类型；
  - §5.3 约束列表：`put_export` 的撤销终态规则、身份材料读取核对指纹、活动时间单调；
  - §7.4 的 `[决定]`：补一条「失去 Import 的写入必须被拒绝；同一 (ownerNodeId, exportId) 重新导入后无法区分新旧连接（未实现，需导入实例或连接代际标识）」；
  - §9 新增判据 30：四条修复的验收断言（不修改既有判据编号与措辞）；
  - §11.2 第 5 条：把「失去 Import 的在途回调必须被拒绝」细化为端口级规则并附残留缺口注记。
- `check-contract-drift` 只读 §5.1–§5.4 的 ```rust 块与 §7.3/§7.4 的 ```sql 块；本变更不动这些块，也确认不新增/删除 fenced 块，避免把 §5/§7 的绑定关系打断。
- 否决「把新规则写进 §5/§7 的代码块」：块内容与 `ports.rs`/`migrate.rs` 归一化后必须相等，加入实现层规则会让门禁报漂移。

## Risks / Trade-offs

- [同一 Owner/Export 重新导入后旧回调仍可通过归属检查] → 本变更不引入字段；已记录为 proposal 的 non_goals 与文档 §7.4/§11.2 的显式注记。闭合需要「导入实例标识」或连接代际（依赖未实现的 `node-link-client`），属需用户决策项。
- [既有库中若已存在指纹与公钥不一致的行，读取会立即失败关闭，`devices()` 列表会整体报错] → 这是有意的失败关闭：宁可拒绝读取也不返回互相矛盾的材料；诊断信息带具体列与原因（`Corrupt` 的静态字符串），运维可按 §7.3 的 65 字节/64 hex CHECK 与配对重新确认修复。不提供自动修复或静默跳过。
- [Export 撤销终态使「`put_export` 携带已撤销记录」由成功变为 `InvalidRequest`] → 影响面仅限直接构造 `ExportWrite` 的内部调用；`UseCases::put_export` 的调用方（`export.create`）不可能产生 `revoked_at = Some` 的记录，`config` 种子路径按 `CONFIG_REFERENCE.md` 必须拒绝 `exports` 键。
- [活动时间「成功但保留较晚值」可能让调用方以为自己的时间戳已生效] → 文档与用例把语义固定为「最近 = 最大值」；该列不参与授权或幂等判定，误判风险限于展示层。
- [`device_from_row` 增加 `public_key` 读取会改变 `DEVICE_COLUMNS`] → 该常量只是本模块的读取列表，不是 §7 的 DDL 文本，`check-contract-drift` 不比对它；用例会覆盖单读/列表读两条路径。
- [断言 `Corrupt` 的具体字符串会脆] → 断言只比对错误变体（`PortError::Corrupt`）与「不返回材料」，不比对完整消息文本。

## Migration Plan

无 schema 变更：不新增/修改表、列、索引、CHECK 与版本常量，因此不需要 migration、不需要夹具升级，也不需要改动 `fixtures/`。对既有数据的唯一影响是上面 Risks 第二条（读取期失败关闭），它是有意的行为变化且不落库任何修复写入。回滚方式为恢复到 `put_export` 与 `trust.rs`/`session_store.rs` 的变更前提交；由于不涉及持久状态改写，回滚不需要数据修复。

## Open Questions

无。用户复核已确认四条修复方向与错误映射；唯一遗留项（重导入后区分新旧连接）已按用户意见记录为非目标与需决策项，不影响本变更的实现与验收。
