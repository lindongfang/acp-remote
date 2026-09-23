## Why

管理状态（身份与信任、配对、Export/Import、审计、本地配置与 Agent profile）目前只有 §11 的**目标形状**：端口签名与 DDL 都写在 `docs/CORE_PORTS_AND_STORAGE.md` §11.5–§11.9，既不进 §5/§7，也没有任何实现——合同漂移门禁因此检查不到它们，`UseCases` 的设备/节点/Export/Import 路径还是「逐方法写入 + 事后补审计」的非原子写法，本地配置连写入端口都没有。本变更把该目标形状落成 `core::ports` + `storage-sqlite` 的实际实现，并让合同漂移门禁对新的端口签名与 DDL 逐条断言。

## What Changes

- **BREAKING**：`core::ports::TrustStore` / `ExportStore` 的写入口由 `upsert_*`/`revoke_*`/`claim_pairing`/`settle_pairing`/`expire_pairings`（携带 `at`）替换为写集 DTO（`DeviceWrite`/`NodeWrite`/`DeviceRevocation`/`NodeRevocation`/`PairingWrite`/`PairingClaimWrite`/`PairingSettlementWrite`/`ExpiryWrite`/`ExportWrite`/`ExportRevocation`/`ImportWrite`/`ImportRemoval`），每个写集携带 `WriteContext { at, audit }`，一次端口调用 = 一个事务 = 状态 + 引用 + 审计。
- **BREAKING**：`core::model::PairingPeer` 的 `public_key_fingerprint: Fingerprint` 改为 `public_key: PeerPublicKey`（指纹由 `PeerPublicKey::fingerprint()` 派生）；新增 `core::model::PeerPublicKey`（65 字节 SEC1 未压缩 P-256，构造即校验点有效性）。
- **BREAKING**：`TrustStore::node` 增加角色维度（`node(&NodeId, NodeKind)`），新增 `nodes_for(&NodeId)` 与 `peer_key(&PeerIdentity)`；`NodeRecord` 的同一对端可同时存在 `access`/`owner` 两行。
- **BREAKING**：`CreateSessionRequest` 的 `workspace_alias` 改为 `Option<ResolvedWorkspace>`（alias→路径解析归 core，见下）。
- **BREAKING**：`ConflictKind` 新增 `AlreadyExists`/`IdentityMismatch`/`DuplicateOwnership`，`UnavailableKind` 新增 `KeystoreUnavailable`；`AuditAction` 新增 `ExportCreated`/`ExportRevoked`/`ImportAdded`/`ImportRemoved`/`ProviderConfigured`。
- 新增 `core::ports::LocalConfigStore`（`AgentProfile`/`WorkspaceRecord`/`ProviderRef`/`SeedState` 的读写）与 `core::ports::CredentialResolver`（profile 的 `env` 绑定 ∩ `env_allowlist` → 子进程环境变量，keystore 不可用时失败关闭）；新增 `core::model` 值对象 `AgentProfile`/`ProviderEnvBinding`/`WorkspaceRecord`/`ProviderRef`/`ProviderRefKind`/`SeedState`/`SecretValue`/`ResolvedWorkspace`。
- 新增 `owned_device`/`owned_node`/`owned_peer_key`/`owned_pairing`/`owned_pairing_peer`/`owned_export`/`owned_agent_profile`/`owned_workspace`/`owned_provider_ref` 与 `imported_import_export` 表；`owned_audit`/`imported_audit` 走 12-step 表重建补 CHECK 取值；v1 的 `imported_import.export_id` 与 `UNIQUE(owner_node_id, export_id)` 迁到关联表。
- 文件格式与两族 schema 版本推进到 2：`FILE_FORMAT_VERSION`/`OWNED_SCHEMA_VERSION`/`IMPORTED_SCHEMA_VERSION`，新增 `fixtures/storage/v2/{empty,from-v1}.sqlite3`，`too-new` 用例改用高于 v2 的版本。
- `UseCases` 的非原子管理路径被替换：`remove_import` 撤回「先 `drop_import` 再 `remove_import`」，改为一次 `ImportRemoval`；`revoke_*`/`settle_pairing`/`put_*` 的审计由写集携带而不是事后补写；新增 profile/workspace/provider/seed 与 workspace 解析入口。
- `UseCases::create_session` 在调用 `SessionBackendFactory::create` 前完成 alias→规范化绝对路径的解析与校验，后端只收 `ResolvedWorkspace`。
- 文档与门禁同步：§11.5–§11.9 的形状并入 §5.3/§7.2/§7.3/§7.4 并在 §9 判据补充、§11 标记为已实现；`IDENTITY_AND_AUTH_CONTRACT.md` §2/§3、`MODULE_ARCHITECTURE.md` §3.1/§4.1/§4.7/§5、`AGENTS.md` §12、`scripts/check-crate-boundaries.mjs` 的 `CORE_ALLOWED_CLOSURE` 同步 core 新增的 `p256`/`sha2` 依赖。
- 本次不包含：`server::*`、`app`、`identity-auth`、`identity-keystore`、`agent-host`、`node-link-client` 与前端代码；配对 proof/HMAC/transcript 验证与 WSS 握手；Sync/Node Link wire schema 与错误码词表；catalog 投影表。

## Intent and Constraints

```agentic-intent
sources:
  - "用户请求（2026-09-23 本会话）：「将核心与存储合同 §11 的目标形状落入 core 端口和 storage-sqlite：身份与信任记录、配对、Export/Import、审计、本地配置和 Agent profile。把新增端口及 DDL 分别并入该合同 §5/§7，使合同漂移门禁能检查实际实现。验收：旧数据库升级、事务原子性、重启恢复、撤销及损坏记录失败关闭均有测试；imported 表和端口仍不能写入远程会话正文。」"
  - "用户决策（2026-09-23 本会话）：PeerPublicKey 校验位置选方案 A——core 引入 p256/sha2，构造即校验并把指纹计算留在 core::model，同步更新 §9 判据 13 / check-crate-boundaries.mjs 的 allow-list / AGENTS.md §12。"
  - "用户决策（2026-09-23 本会话）：「本变更（admin-state-persistence-v2）同意 Main E2E 记 not-applicable，替代验证为 core/storage-sqlite 的 cargo 测试 + npm run check + 合同漂移门禁 + v1→v2 迁移夹具。」"
  - "权威合同：docs/CORE_PORTS_AND_STORAGE.md §11.1–§11.9（§11.8 的收口清单列出本变更必须同时完成的六件事）；docs/IDENTITY_AND_AUTH_CONTRACT.md §2/§3/§4；docs/MODULE_ARCHITECTURE.md §4.7/§5；docs/LOCAL_ADMIN_PROTOCOL.md §5.3–§5.5；docs/CONFIG_REFERENCE.md 的「配置与管理状态的权威」；docs/SECURITY_DESIGN.md §13.1/§14.2/§15。"
constraints:
  - "遵守 AGENTS.md §3 的不变量：端口纯度与单一写入口、失败关闭、审计不含内容、凭据不入库、imported 家族不得写入远程会话正文。"
  - "core 只新增已批准的 p256/sha2（Q1 方案 A）；仍不依赖 runtime/DB/HTTP/子进程/wire protocol，allow-list 四处定义必须同一改动内同步。"
  - "storage-sqlite 是管理状态的唯一持久化实现；一次端口调用一个事务一个完整写集，不新增独立数据库或通用配置服务。"
  - "表结构、端口签名与 §5/§7 逐条一致：合同漂移门禁（scripts/check-contract-drift.mjs）必须能对本次新增内容断言，§7 仍是 2 个 ```sql 块。"
  - "本地入口 npm run verify（npm run check + cargo fmt/clippy/test）；cargo-deny / gitleaks 只在 CI 运行，本地不得声称通过。"
  - "管理 DTO 与集合字段在 adapter 内按领域构造器校验后编码为有类型 JSON 数组；身份、状态、时间、唯一键与外键用显式列，不用 JSON blob 承担仲裁。"
non_goals:
  - "不实现 server::*、app、identity-auth、identity-keystore、agent-host、node-link-client 与前端代码。"
  - "不实现配对 proof/HMAC/transcript 验证、WSS 握手、nonce/SAS 生命周期与 keystore 平台后端。"
  - "不新增 catalog 投影表、不做离线 Import 编辑、不做应用层数据库加密（SECURITY_DESIGN §13.1 既定决策）。"
  - "不改变 Sync/Node Link wire schema、错误码、feature 词表与 fixture（npm run check 仍需照常通过）。"
success_criteria:
  - "v1 数据库迁移到 v2 后事件、cursor、幂等行与审计逐条保留；重复打开幂等（schema 文本逐字节不变）；版本高于已知版本的库拒绝打开且零写入。"
  - "管理 mutation 的状态、引用与审计在同一事务提交；注入审计写失败/磁盘满/约束冲突时整事务回滚，不出现半条授权或「提交成功但没有持久信任」。"
  - "重启后撤销仍有效、未确认配对不再可用、已配对身份与 profile/workspace/provider 引用/seed 标记保留。"
  - "损坏记录与宽松权限在正式模式下对写路径失败关闭（PortError::Corrupt 等），只读查询仍可用。"
  - "imported 黄金列清单不变，端口与表都不出现远程会话正文；remove/drop 后审计行仍在。"
  - "npm run check 全绿（含合同漂移门禁对新端口签名与新 DDL 的逐条断言）；core/storage-sqlite 的 cargo 测试全绿。"
decision_bounds:
  - "可自主：模块内部实现细节、列命名与索引、测试组织、错误映射的具体分支、migration 语句顺序（除漂移门禁绑定的常量边界）。"
  - "需用户决策：把 p256/sha2 之外的依赖加入 core 闭包；改变 §11 的安全/协议语义或 §3 不变量；改变本变更的范围与验收判据。"
assumptions:
  - "§11.5–§11.9 的目标形状可直接实现且不需要协议侧改动；同期没有其他变更并行修改 core / storage-sqlite 的同一批文件。"
  - "fixtures/storage/v2/from-v1.sqlite3 需新建（v1 目录只有 empty 与 too-new 两个夹具），内容为含会话/事件/cursor/幂等/审计数据的 v1 库。"
  - "p256 0.13 / sha2 0.11 已在 workspace.dependencies 预留，core 只需 workspace = true 引用，不新增 Cargo.lock 版本选择。"
```

## Capabilities

### New Capabilities

- `admin-state-persistence`: 设备、节点、配对、Export/Import 管理状态的原子写集、撤销、重启恢复与审计同事务语义。
- `peer-identity-material`: 对端公钥值对象与信任材料的写入/读取/撤销契约，含双角色身份一致性与身份变化处理。
- `local-agent-config`: 本地 Agent profile、workspace、Provider 引用与首次初始化种子状态的持久化，以及凭据到子进程环境变量的解析边界。
- `workspace-resolution`: workspace alias 到本机规范化路径的解析、失败分类与路径不泄漏。
- `storage-schema-v2-migration`: 文件格式/表结构版本 v2 的升级、幂等、失败关闭、保留与 imported 无正文不变式。

### Modified Capabilities

- 无（`openspec/specs/` 目前为空，本次全部为新增能力）。

## Impact

- **Rust**：`crates/core`（`model/identity.rs`、`model/error.rs`、`model/session.rs`、`model/mod.rs`、`ports.rs`、`use_cases.rs`、`broker.rs` 的测试替身、`Cargo.toml`）、`crates/storage-sqlite`（`migrate.rs`、`session_store.rs`、新增管理 store 模块、`error.rs`、`tests/*`）、根 `Cargo.toml`（core 的 p256/sha2 引用）。
- **合同与文档**：`docs/CORE_PORTS_AND_STORAGE.md`（§5.3、§7.2、§7.3、§7.4、§9、§11）、`docs/IDENTITY_AND_AUTH_CONTRACT.md`、`docs/MODULE_ARCHITECTURE.md`、`docs/CONFIG_REFERENCE.md`、`AGENTS.md` §12。
- **门禁与夹具**：`scripts/check-crate-boundaries.mjs` 的 `CORE_ALLOWED_CLOSURE`、`fixtures/storage/v2/*`、`fixtures/storage/v1/too-new.sqlite3` 的用途改由 v2 版本号更高的夹具承担。
- **不受影响**：`schemas/`、`fixtures/{sync,node-link,acp,local-admin}/` 与 `compatibility/` 的封闭词表；wire 语义与错误码不变。
