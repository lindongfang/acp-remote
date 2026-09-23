<!-- 本文件承接 proposal 的动机、范围与影响，说明如何实现以及技术决策的理由。 -->

## Context

`docs/CORE_PORTS_AND_STORAGE.md` 的 §11.5–§11.9 已经把管理状态的目标形状冻结成可实现的端口签名、值对象与 DDL，但按同文档的修订记录，这些形状**刻意没有**写入 §5/§7——因为 `scripts/check-contract-drift.mjs` 把 §5 的 ```rust 块与 `crates/core/src/ports.rs`、§7 的 ```sql 块与 `crates/storage-sqlite/src/migrate.rs` 的 `OWNED_SCHEMA_V1`/`IMPORTED_SCHEMA_V1` 逐条绑定（§7 期望恰好 2 个 sql 块、按语句顺序比较）。

现状事实（已只读核对）：

- `core` 只有 `async-trait` + `thiserror` 两个直接依赖，闭包 allow-list 冻结在 `scripts/check-crate-boundaries.mjs`（§9 判据 13、`AGENTS.md` §12）。
- `core::ports` 已有 `TrustStore`/`ExportStore`/`AuditStore`，但形状是 §11.5 之前的旧版（`upsert_*`/`revoke_*` 携带 `at`、`TrustStore::node(&NodeId)` 无角色维度、`PairingPeer` 只有指纹）；`LocalConfigStore`/`CredentialResolver` 完全不存在。
- `storage-sqlite` 只实现 `SessionStore`/`ReadView`/`RemoteDeliveryStore`/`AttachmentStore`（`session_store.rs`），管理三族只有测试替身（`core/src/broker.rs` 的 `FakeTrust`/`FakeExports`/`TestAudit`）；`migrate.rs` 的 `FILE_FORMAT_VERSION`/`OWNED_SCHEMA_VERSION`/`IMPORTED_SCHEMA_VERSION` 都是 1，`migrate()` 只跑两段 `raw_sql` + 写 meta 版本。
- `UseCases::remove_import` 目前是先 `deliveries.drop_import` 再 `exports.remove_import` 两次调用；`revoke_device`/`revoke_node`/`settle_pairing`/`create_pairing` 都是「先写状态再补审计」。
- §7.5 的容量度量实现是「`SqliteStore::open` 时由 `sqlite_master` + `pragma_table_info` 现读两家族全部 TEXT 列拼出并缓存」，因此新增管理表会被度量自动纳入，无需手写列清单。
- `p256 0.13` / `sha2 0.11` 已在 `[workspace.dependencies]` 预留（注释写明「identity-auth 落地时引用」），`workspace.dependencies` 里 `p256` 带 `ecdsa` feature。
- `core::broker::port_error_public` 对 `PortError::Conflict(_)` 与 `PortError::Unavailable(_)` 各有一条通配臂（都映射 `internal.unavailable`），新增枚举取值不会被编译器提醒。

## Goals / Non-Goals

**Goals:**

- `core::model` / `core::ports` / `core::use_cases` 具备 §11.5–§11.9 的形状：公钥值对象、写集 DTO、目标 `TrustStore`/`ExportStore` 签名、新端口 `LocalConfigStore`/`CredentialResolver`、workspace 解析。
- `storage-sqlite` 提供管理状态的唯一持久化实现：v2 DDL、v1→v2 migration、原子写集、失败关闭、保留/容量纳入。
- §5/§7 成为管理状态合同的一部分，合同漂移门禁能对新增端口签名与 DDL 逐条断言。

**Non-Goals:**

- 不实现 `identity-auth`/`identity-keystore`/`server`/`app`/`agent-host`/`node-link-client`/前端；配对 proof 与 WSS 握手的验证仍属后续 crate。
- 不改 wire schema、错误码词表、feature 词表；`CredentialResolver` 的 keystore 实现属组合根。
- 不做加密离线缓存、catalog 投影表或离线 Import 编辑。

## Decisions

### D1 依赖边界：core 引入 p256 + sha2（用户已批准方案 A）

- `crates/core/Cargo.toml` 增加 `p256.workspace = true`、`sha2.workspace = true`；若 `p256::PublicKey::from_sec1_bytes` 需要 `arithmetic` 类 feature，只在本 crate 的依赖声明里追加 feature，版本仍来自 `[workspace.dependencies]` 的单一 pin。
- 同步更新四处：`scripts/check-crate-boundaries.mjs` 的 `CORE_ALLOWED_CLOSURE`、`docs/CORE_PORTS_AND_STORAGE.md` §9 判据 13、`AGENTS.md` §12、`docs/MODULE_ARCHITECTURE.md` §3.1 的依赖说明（第 3 项按 §10 的映射规则）。
- 闭包预期从 7 个扩到约 25 个（`p256`/`sha2` 及其 `elliptic-curve`/`sec1`/`crypto-bigint`/`subtle`/`zeroize`/`digest`/`block-buffer`/`cfg-if`/`cpufeatures` 等传递依赖）：实现时以 `cargo tree -p core --edges normal` 的实际输出为准逐项登记，不预先臆造名单。CI 的 `deps`/`advisories` 是这批新依赖的唯一自动门禁（本地没有等价物）。
- 备选方案 B（core 只做结构校验、点校验移到身份边界）被否决：它会让「指纹与公钥不一致」成为可落库的非法状态，而 SQLite 的 CHECK 无法校验 SHA-256 关系。

### D2 端口形状：写集承载「状态 + 集合 + 审计」

- 新增 `WriteContext { at, audit: Vec<PendingAudit> }`，所有管理写 DTO 内嵌它；`PendingAudit` 与 `AuditRecord` 字段一致但不带 `at`。
- `TrustStore` 读面：`node(&NodeId, NodeKind)`、`nodes_for(&NodeId)`、`peer_key(&PeerIdentity)`、`pairing_peer(&PairingId)`；写面：`put_device`/`put_node`/`revoke_device`/`revoke_node`/`create_pairing`/`claim_pairing`/`settle_pairing`/`expire_pairings`。
- `ExportStore` 写面：`put_export`/`revoke_export`/`add_import`/`remove_import`；读面不变。
- 新增 `LocalConfigStore`（profile/workspace/provider_ref/seed）与 `CredentialResolver`（`resolve_env(&AgentProfile) -> Vec<(String, SecretValue)>`）。`SecretValue` 不实现 `Debug`/`Serialize`，不进日志。
- `UseCases::remove_import` 收敛为一次 `ImportRemoval`；`RemoteDeliveryStore::drop_import` 保留「连接级清空」语义，两者不再串起来充当完整删除。
- 端口纯度不变：新类型只用 §3 的 `core::model` 类型，不出现 `serde_json::Value`/SQL/HTTP。

### D3 `PeerPublicKey` 的构造与派生

- `PeerPublicKey([u8; 65])`：私有字段 + `try_from_bytes`/`FromStr`；顺序固定为长度 65 → 首字节 `0x04` → `p256::PublicKey::from_sec1_bytes`，任一步失败返回 `InvalidValue`（新增取值，`as_str` 给出稳定消息）。
- `fingerprint()` 是唯一入口：`sha2::Sha256::digest` 后格式化为 64 字符小写 hex，返回既有 `Fingerprint`。
- `PairingPeer` 用 `public_key: PeerPublicKey` 取代 `public_key_fingerprint: Fingerprint`；`LOCAL_ADMIN_PROTOCOL.md` 的 wire 字段语义不变（值是派生结果）。
- 负例固化为常驻测试：33 字节压缩点被拒、长度/前缀非法被拒、非法曲线点被拒、指纹与公钥一致（不能出现只传指纹的构造路径）。

### D4 错误枚举与映射

- `ConflictKind` 增 `AlreadyExists`/`IdentityMismatch`/`DuplicateOwnership`，`UnavailableKind` 增 `KeystoreUnavailable`；`ALL` 数组与 `as_str` 同步，`model/tests.rs` 的长度断言随实现一起改。
- `AuditAction` 增 `ExportCreated`/`ExportRevoked`/`ImportAdded`/`ImportRemoved`/`ProviderConfigured`，`ALL` 同步——这是 `crates/storage-sqlite/tests/enum_coverage.rs` 断言 DDL 字面量与枚举逐条相等的输入。
- `broker::port_error_public`：为新增的 `ConflictKind` 取值补**显式**分支，使其在命令路径上不再静默落入 `internal.unavailable`；本地管理适配器（尚未实现的 `server::local_admin`）负责映射为 `local.conflict`/`local.unavailable`，本变更在 core 侧留下显式分支与注释，避免「新增取值没有编译错误」的陷阱。
- `storage-sqlite/src/error.rs` 的 `StorageError → PortError` 映射补齐新分支：唯一键/条件更新 → `Conflict(AlreadyExists|AlreadyClaimed|Consumed|Expired)`、指纹不一致 → `Conflict(IdentityMismatch)`、归属冲突 → `Conflict(DuplicateOwnership)`、约束失败 → `InvalidRequest`、`SQLITE_FULL` → `Unavailable(StorageFull)`。

### D5 DDL 组织与 migration 结构

- §7 仍保持 **2 个 ```sql 块**：管理表追加进 `OWNED_SCHEMA_V1` 尾部，`imported_import_export` 追加进 `IMPORTED_SCHEMA_V1`；合同与实现同一次改动落地（中途漂移门禁必红属预期）。
- `migrate()` 改为「读版本 → 按需升级 → 写版本」：`user_version` 或两族版本 < 2 时，在同一事务内执行 v2 升级步骤，然后写版本 2；已为 2 时跳过升级，因此第二次打开不会重写 `sqlite_master`（§9.1 的逐字节幂等）。
- v2 升级步骤：① `CREATE TABLE IF NOT EXISTS` 全部新管理表与索引；② `owned_audit` 12-step 重建（新建 → 拷数据含 `audit_id` → 换名 → 重建索引），保证 AUTOINCREMENT 序列不回退；③ `imported_import` 重建：去掉 `export_id` 与 `UNIQUE(owner_node_id, export_id)`，把每个原行的 export 写进新的 `imported_import_export`（`added_at` 取原 `created_at`）；④ 写 `meta.*_schema_version = 2` 与 `PRAGMA user_version = 2`。
- 「没有可信来源的 grants」的 Import：迁移只搬事实（owner/export/endpoint/时间戳），不构造 grants；实现上用「`grants_json` 为空且无法从旧行推出的 Import 保持不可用」表达，具体判定写在 store 的读取路径（`ImportRecord` 的权限集合为空即不可用），不凭空补默认值。
- 容量度量与清理顺序无需改实现（现读 TEXT 列），但要有测试证明管理表分量被计入；「撤销 tombstone 不被容量清理删除」由现有清理谓词（只清 delta/正文/状态/已压缩 delta/附件/到期审计）天然保证，加断言即可。

### D6 workspace 解析（core 内）

- `create_session` 在调用 `SessionBackendFactory::create` 前：查 `LocalConfigStore::workspace(alias)` → 校验绝对路径 + `std::fs::metadata().is_dir()` + `std::fs::canonicalize` → 组装 `ResolvedWorkspace { alias, canonical_path }` 放进 `CreateSessionRequest`。
- 失败分类：Export 未声明 alias → 既有参数类错误；已声明但本机解析失败 → `PortError::Unavailable(UnavailableKind::IoError)`；UNC/网络路径检测到（Windows 前缀或 `\\`）时记一次结构化警告（core 只用 `tracing`? 不可——core 不引入日志依赖，因此用返回路径上的显式「警告集合」或由组合根记录；实现选择：core 不记日志，改为在 `ResolvedWorkspace` 上不加字段、由 `UnavailableKind`/错误分类表达，UNC 警告由后续 `server` 层记录，本变更只保证不改变授权模型）。**决策**：UNC 警告不在 core 实现，落到后续 server 层；本变更不因它增加 core 依赖。
- `ResolvedWorkspace.canonical_path` 只出现在后端调用的入参里，不进事件、错误 `details` 与审计摘要前像。

### D7 文档并入策略（避免两处正文）

- §11.5/§11.6 的形状并入 §5.3 的 ```rust 块；§11.7 的 DDL 追加进 §7.3/§7.4 的 ```sql 块；§11.5 的值对象并入 §3.5/§3.6 的表格；§11.8 的版本表并入 §7.2；§11.9 的规则并入 §3.6 与 §5.1；§9 增加对应判据编号（沿用现有编号续号，不重排）。
- §11 改为「已实现」小节：保留设计理由与实现索引（指向 §5/§7 的段落），删除与 §5/§7 重复的签名与 DDL 正文。
- `IDENTITY_AND_AUTH_CONTRACT.md` §2/§3 改成引用 `CORE_PORTS_AND_STORAGE.md` §5.3/§3.5 的**已实现**形状（去掉「实现变更要新增」的将来时）；`MODULE_ARCHITECTURE.md` §3.1/§4.1/§4.7/§5 同步 core 依赖与 `LocalConfigStore`/`CredentialResolver` 的职责归属；`CONFIG_REFERENCE.md` 的「配置与管理状态的权威」与 seed 语义核对一遍，如有冲突以该文件为准并同改本变更。

## Risks / Trade-offs

- **core 依赖闭包显著扩大** → 唯一自动门禁是 CI 的 `deps`/`advisories`；缓解：只加 workspace 已 pin 的两个 crate，本变更不引入其它版本选择，本地明确记录「未在本地执行 cargo-deny」。
- **DDL 批量变更 + 审计表 12-step 重建有数据损坏风险** → 缓解：升级在单事务内完成、失败整体回滚；用 `fixtures/storage/v2/from-v1.sqlite3` 做行数与 `audit_id` 保留断言；升级中途失败后重开可重试。
- **漂移门禁在改动中期必红**（§7/§5 与实现不同步）→ 缓解：本变更作为单一交付单元闭合，中间态只作局部检查，最终以 `npm run verify` 为准。
- **写集把审计纳入事务后，审计不可写会放大失败面** → 这是 §11.2 第 6 条的有意选择（宁可整事务失败也不留无审计的状态变更）；测试必须覆盖「审计写失败 → 状态不落库」。
- **Import 迁移可能让旧 Import 失效**（无可信 grants）→ 有意取舍：不凭空放权，保持不可用待本地重新授权；在迁移测试中断言「不补默认 grants」。
- **E2E 记 not-applicable**：本变更没有可运行的产品入口，替代验证覆盖「迁移 + 原子性 + 重启 + 撤销 + 失败关闭 + imported 无正文」的库级路径，但不覆盖真实 WSS/配对握手；该缺口已由用户逐变更批准，并记录在 plan.md 的 `downgrade_approval`。
- **`nodes()`/`node()` 的角色维度是破坏性签名变化** → 受影响的只有尚未实现的 `server::local_admin` 与测试替身，本变更一并更新，不保留兼容层。

## Migration Plan

- 升级方向唯一：v1 → v2，在单实例锁之后、开始监听之前，单事务执行；失败整体回滚，重开可重试；版本高于已知版本的库拒绝打开且零写入（不回退、不降级写入）。
- 保留不变量：`server_epoch`、会话 `origin_epoch`/序号、事件 `global_sequence`/`session_sequence`、`requestId`、幂等行、命令终态与全部既有审计；`audit_id` 不重编号。
- 夹具与回归：新增 `fixtures/storage/v2/empty.sqlite3`（v2 空库）、`fixtures/storage/v2/from-v1.sqlite3`（含会话/事件/cursor/幂等/审计的 v1 库）、`fixtures/storage/v2/too-new.sqlite3`（`user_version = 3`）；`fixtures/storage/v1/` 的既有文件保留为历史资产，测试改为覆盖 v2 三件套。
- 回滚条件与方法：v2 库不能被 v1 二进制打开（版本过新拒绝），因此回滚 = 恢复升级前的数据库备份 + 回退二进制；本变更不提供自动降级迁移（§11.3 已裁定）。
