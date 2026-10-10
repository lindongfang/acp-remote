<!-- 本文件定义变更动机、范围、能力与影响；行为细节放入 specs，技术方案放入 design.md，
 协作安排见 tasks.md。本变更不改变 wire schema 与 DDL，只收口管理存储的状态完整性。 -->

## Why

`storage-sqlite` 的管理 store（`crates/storage-sqlite/src/admin/` 与 `session_store.rs` 的 imported 家族）有一组「写入可越过已提交的终态」的漏洞，四条都是同一类问题——已撤销或已删除的状态可以被后续写入部分恢复或倒退：

1. **Import 完整移除后可被迟到回调部分重建**：`upsert_session` 只做 `ON CONFLICT ... DO UPDATE`，从不检查 `(owner_node_id, export_id)` 是否仍归属某个 `imported_import`。`remove_import` 已删掉会话行与索引，但一个在途回调仍能写下新的 `imported_session` 行，进而让 `commit_receipt` 重新写入交付索引——违反 `docs/CORE_PORTS_AND_STORAGE.md` §11.2 第 5 条「失去 Import 的在途回调必须被拒绝，不能重建已删除的索引」。
2. **Export 撤销可被更新操作清除**：`put_export` 的 `DO UPDATE` 把 `revoked_at = excluded.revoked_at` 直接写入，传入 `revoked_at = None` 的写集会把已撤销记录改回未撤销，使一个已被 `export.revoke` 永久关闭的 Export 重新可用，违反 §11.1「撤销记录保留」与 §11.2 第 4 条。
3. **损坏的身份材料未失败关闭**：`load_peer_key` 只读 `owned_peer_key.public_key`，不核对同行的 `fingerprint` 列；指纹与公钥不一致（外部改写、损坏库）时仍把公钥交给握手，使「指纹与公钥互相矛盾的行」成为可用的验签材料。设备记录读取路径（`device()`/`devices()`）同样只读 `fingerprint` 列、从不校验同行的 `public_key`。
4. **最近活动时间可倒退**：`upsert_device.last_seen_at` / `upsert_node.last_connected_at` 只用 `COALESCE(excluded.x, x)`，只能防止被抹成 NULL；传入比已存值更早的时间戳会直接把「最近一次认证成功 / 连接时间」写回过去，破坏该字段作为最近活动判据的语义。

四条都在同一层（管理状态的持久化边界），且都不需要改 DDL、端口签名、wire schema 或封闭词表：修的是既已冻结的写路径守卫与读取期校验缺失。

## What Changes

- **imported 写路径先验归属**：`RemoteDeliveryStore::upsert_session` 与 `commit_receipt` 在同一写事务内先要求 `(owner_node_id, export_id)` 仍归属某个 Import（`imported_import_export` 有行），无归属返回 `PortError::NotFound(EntityRef::Export(exportId))`，不写入会话行、索引或 `local_sequence`。
- **Export 撤销为终态**：`put_export` 先拒绝携带 `revoked_at` 的记录（`InvalidRequest`，撤销只走 `revoke_export`），再对「库内已撤销 + 传入未撤销」返回 `Conflict(ConflictKind::AlreadyExists)`；`revoked_at` 永不被 `put_export` 清空。不新增 `export.update` 一类对外方法。
- **身份材料读取核对同行指纹**：`load_peer_key`（`owned_peer_key`）、`pairing_peer_from_row`（`owned_pairing_peer`）、设备记录读取路径（`owned_device` 的 `public_key` 与 `fingerprint` 同行比对，`device()`/`devices()` 共用）以及写设备的兜底路径 `load_existing_device_key`，都在读取时核对指纹列与公钥派生值，不一致返回损坏类错误（`PortError::Corrupt`）而不是返回材料。
- **活动时间单调不减**：`upsert_device`/`upsert_node` 的该两列改为显式 `CASE`：旧值为空写入新值、新值为空保留旧值、两者非空取较大者；不返回错误，也不因该列丢弃同一次写入的其他字段。
- **合同同步**：`docs/CORE_PORTS_AND_STORAGE.md` 的 §5.2/§5.3 约束、§7.4 的 `[决定]`、§9 验收判据与 §11.2 第 5 条补上上述规则；**不改** §5/§7 的 fenced 代码块（端口签名与 DDL 文本不变，漂移门禁继续逐条成立）。
- 不包含：新增 Import 实例标识或连接代际字段、新增 `ConflictKind`/错误码取值、改 DDL/迁移、改 wire schema 与 `compatibility/` 词表、任何前端或协议行为。

## Capabilities

### New Capabilities

- 无。四条修复都是既有能力内部的行为收口，不引入新的能力域。

### Modified Capabilities

- `admin-state-persistence`: 新增「撤销与删除后的写入不得复活资源」（Export 撤销终态、Import 移除后迟到回调被拒）与「设备与节点记录的活动时间只前进」（活动时间单调不减）两项要求。
- `peer-identity-material`: 新增「身份材料读取必须核对同行指纹」（`owned_peer_key`/`owned_pairing_peer`/`owned_device` 读取路径失败关闭）要求。

增量规范位于 `specs/admin-state-persistence/spec.md` 与 `specs/peer-identity-material/spec.md`，均为 `## ADDED Requirements`（不重写既有需求块），归档时并入 `openspec/specs/` 下的同名能力。

## Impact

- **Rust 生产代码**：`crates/storage-sqlite/src/session_store.rs`（`upsert_session`、`commit_receipt`）、`crates/storage-sqlite/src/admin/export.rs`（`put_export`）、`crates/storage-sqlite/src/admin/trust.rs`（`load_peer_key`、`pairing_peer_from_row`、`device_from_row`/`DEVICE_COLUMNS`、`load_existing_device_key`、`upsert_device`、`upsert_node`）与其 `load_peer_key` 的文档注释（模块头无需改动，RV1-F2）。core 侧零改动（不新增端口方法、不改值对象、不改错误枚举）。
- **测试**：`crates/storage-sqlite/tests/imported.rs`（归属守卫回归）、`crates/storage-sqlite/tests/admin_store.rs`（Export 撤销终态、身份材料读取失败关闭、活动时间三方用例）。
- **权威文档**：`docs/CORE_PORTS_AND_STORAGE.md`（§5.2/§5.3 约束、§7.4 `[决定]`、§9 验收判据、§11.2 第 5 条）。§5/§7 的 fenced 代码块与 DDL 文本不变。
- **门禁**：`npm run check` 全绿（`check:contract-drift` 因签名与 DDL 未变而保持逐条成立；不涉及 `compatibility/`、`schemas/`、`fixtures/`）；`cargo fmt/clippy/test` 全绿。
- **兼容性**：无 wire/协议影响；既有数据库无需迁移。已存在的、指纹与公钥互相矛盾的行会在读取时失败关闭（有意的行为变化，见 design 的 Risks）。
- **本地管理通道**：无新方法、无新错误码；`put_export` 的新失败分支只影响非法写入（清除撤销标记、或经 `put_export` 自行撤销），`export.create`/`export.revoke` 的正常路径不变。
