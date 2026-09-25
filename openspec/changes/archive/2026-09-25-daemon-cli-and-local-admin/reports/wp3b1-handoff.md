# coder 报告 · WP3b1（`server::local_admin` 方法路由：本地配置族、daemon 生命周期、Export/Import、审计导出，交付单元 DU1）

- task_id: `2.10`、`2.12`（工作包 WP3 后半，记为 WP3b1；`2.11`/`2.13` 与 WP4/WP5 不在本轮范围）
- role: coder
- phase: implement
- stage: work-package
- agent_context: 独立子 Agent（worker / coder 角色），主 worktree 的分支 `feat/daemon-cli-and-local-admin`；只继承任务单给出的契约输入（`docs/LOCAL_ADMIN_PROTOCOL.md`、`docs/SECURITY_DESIGN.md` §9.2/§14.2、`docs/MODULE_ARCHITECTURE.md` §5、两份 spec、`tasks.md` 2.10/2.12、`core::use_cases`/`core::ports`/`core::model`、`identity-auth` 的 keystore 端口、WP3a 交接的冻结形状），不继承规划阶段对话；本变更各波次串行（同一时刻只有一个写入者）
- base_revision: `51dc6b1`（任务单给定起点，开工前 `git log --oneline -3` 核对一致）
- target_revision: **`f1a3cd4`**（本 WP 的交付提交）。**同一 WP 的另一部分文件位于 `fee6073`**：主 Agent 在实现期间误用 `git add -A` 把当时工作树里的 `crates/server/Cargo.toml`（新增 core/identity-auth/base64 依赖与说明）、`Cargo.lock` 与 `crates/server/src/local_admin/{audit,daemon,params,view}.rs` 的**中间版本**一起提交进了 `fee6073`；`f1a3cd4` 含这四个文件的最终差分与其余全部新增文件。两个提交合起来构成本 WP 的完整组成（见「提交与交付对应」）
- scope（写入范围）：`crates/server/**`（含 `Cargo.toml`）、`Cargo.lock`、`openspec/changes/daemon-cli-and-local-admin/reports/`。**未改**任何其它 crate、`docs/`、`vendor/`、规划文件（`plan.md`/`tasks.md`/`verification.md`/`design.md`/`proposal.md`/`specs/`）、`schemas/`/`fixtures/`。**未实现** `device.*`/`node.*`（WP3b2）与 `node.rotate-key.begin`（口径待裁决），**未改** schema
- result: `PASS`（本工作包的检查与交付条件全部满足；**不代表**独立 review、集成或合并已完成）

## 提交与交付对应

| 提交 | 类型 | 内容 | 覆盖任务 |
| --- | --- | --- | --- |
| `fee6073`（主 Agent 误提交，内容即当时工作树状态） | `docs(server)`（提交信息是主 Agent 的记录变更，与实现无关） | `crates/server/Cargo.toml`（+core/identity-auth/base64）、`Cargo.lock`、`local_admin/{audit,daemon,params,view}.rs` 的中间版本 | 2.10、2.12 的一部分文件 |
| `f1a3cd4` | `feat(server)` | `local_admin/router.rs`（新增 1398 行）、`local_admin/test_support.rs`（新增 1167 行）、`local_admin/mod.rs`、`lib.rs` 与 `{audit,daemon,params,view}.rs` 的最终差分 | 2.10、2.12 |
| 本报告提交 | `docs(server)` | `reports/wp3b1-handoff.md`（本文件） | 2.10/2.12 的交接 |

代码提交经 `.husky/pre-commit`（`cargo fmt --check` + `npm run check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`，三步全过）与 `commitlint` 通过；**未使用** `--no-verify`、未强推、未改已有提交。

`git status --porcelain`（本报告写入前，即代码提交后）：

```text
（空）
```

## 冻结的公开形状（WP4 要消费）

```rust
use server::local_admin::{
    AdminError, AdminRequest, AdminResponse, DaemonAgent, DaemonControl, DaemonCounts,
    DaemonLink, DaemonLinkState, DaemonStatus, LocalAdminDeps, LocalAdminRouter,
    LocalAdminHandler, LocalErrorCode, MAX_STOP_GRACE_MS,
};

// 1) daemon 生命周期注入口（由 app::daemon 实现）
#[async_trait::async_trait]
pub trait DaemonControl: Send + Sync {
    async fn status(&self) -> DaemonStatus;                              // 无 Result：组合根持状态快照
    async fn stop(&self, grace_ms: Option<u64>) -> Result<(), AdminError>; // None = 用 daemon.shutdown_grace_ms
}

pub struct DaemonStatus {          // §5.2 的 12 个字段；to_json() 产出 camelCase result
    pub version: String,
    pub instance_id: String,
    pub node_id: NodeId,                       // core newtype（canonical 小写 UUID）
    pub node_public_key: PeerPublicKey,        // core newtype（wire 上编码为无填充 base64url 65 bytes）
    pub started_at: Timestamp,
    pub uptime_ms: u64,
    pub data_dir: String,                      // 绝对路径
    pub listen: Vec<String>,                   // 本切片恒空数组（无网络 listener）
    pub public_origin: Option<String>,
    pub counts: DaemonCounts,                  // { devices, nodes, exports, imports }（u64）
    pub agents: Vec<DaemonAgent>,              // { agent_id: AgentId, available: bool }
    pub links: Vec<DaemonLink>,                // 本切片恒空数组（重连任务未落地）
}
pub enum DaemonLinkState { Connected, Connecting, Offline }   // as_str(): "connected"|"connecting"|"offline"
pub struct DaemonLink { pub node_id: NodeId, pub state: DaemonLinkState,
                        pub last_connected_at: Option<Timestamp>, pub next_retry_at: Option<Timestamp> }
pub const MAX_STOP_GRACE_MS: u64 = 60_000;

// 2) 路由装配（app 组合根注入具体实现）
pub struct LocalAdminDeps {
    pub daemon: Arc<dyn DaemonControl>,
    pub core: Arc<core::use_cases::UseCases>,
    pub keystore: Arc<dyn identity_auth::IdentityKeystore>,
    pub audit: Arc<dyn core::ports::AuditStore>,
    pub clock: Arc<dyn core::ports::Clock>,
}
impl LocalAdminRouter { pub fn new(deps: LocalAdminDeps) -> Self; }
impl LocalAdminHandler for LocalAdminRouter { /* 恰好一个响应、回带同一 id */ }
```

装配示例（与 WP3a 的 `serve_connection` 直接对接，传输层不动）：

```rust
let router = Arc::new(LocalAdminRouter::new(LocalAdminDeps { daemon, core, keystore, audit, clock }));
let handlers = Arc::new(LocalConnectionHandlers::new(router));   // router: Arc<dyn LocalAdminHandler>
```

`UnroutedAdminHandler` 保留（WP3a 的通道测试仍在使用），但生产装配应换成本路由。

### Provider 凭据的 keystore 条目方案（WP4 的 `CredentialResolver` 必须按同一规则解析）

```text
ProviderRef.keystore_ref = "<providerDigest>@v<version>"
  providerDigest = SHA-256(providerId) 的前 16 个 hex 字符（小写）
  字段条目标签     = "<keystore_ref>.<fieldName>"
  端口调用         = put_secret(SecretPurpose::ProviderCredential, "<keystore_ref>.<field>", SecretBytes)
  组合根解析       = keystore 里查标签 "<ProviderRef.keystore_ref>.<ProviderEnvBinding.field>"
```

- 用摘要而不是 `providerId` 本身：`identity-keystore` 的标签上限是 128 字节，而 `providerId` 与字段名各自可达 64 字符，`<id>.<field>` 最长 129 字节会越界；摘要把标识收敛到定宽（最坏 103 字节，有测试断言）。
- 版本进入标签是 `CORE_PORTS_AND_STORAGE.md` §11.6 第 7 条「先写新的、带版本的 keystore 条目，再提交 SQLite 引用；失败时旧引用继续有效」的实现：提交失败时旧引用指向的旧条目仍然完整，本次新条目被回滚；提交成功后旧版本条目才被清理。
- `provider.configure` 每次调用把该 `(providerId, kind)` 的凭据集合**整体替换**为新版本（`configuredFields` 即本次 `values` 的字段名，保序）。

## 方法 → result 形状摘要（§5.2/§5.5/§5.6）

| 方法 | 实现路径 | result | 主要失败码 |
| --- | --- | --- | --- |
| `daemon.status` | `DaemonControl::status` → `DaemonStatus::to_json` | §5.2 的 12 个字段（`listen`/`links` 恒 `[]`） | `local.invalid_params`（`params` 非 `{}`） |
| `daemon.stop` | `DaemonControl::stop(graceMs)` | `{ accepted: true }` | `local.invalid_params`（越界/非整数）、组合根错误原样（如 `local.unavailable`） |
| `workspace.select` | `workspace()`（读旧 `createdAt`）→ `put_workspace` | `{ workspace: { alias, displayName, rootPath, createdAt } }`（`rootPath` = canonicalize 结果） | `local.invalid_params`（相对路径/含 `..`/不存在/非目录/alias 或 displayName 形态）、端口类 |
| `agent.configure` | `profile()`（读旧 `createdAt`/`env`）→ `put_profile` | `{ agent: { agentId, displayName, default } }` | `local.invalid_params`、端口类 |
| `provider.configure` | keystore `put_secret`（新版本条目）→ `put_provider_ref` → 清理旧版本条目 | `{ provider: { providerId, kind, configuredFields } }`，**永不回显 `values`** | `local.unavailable`（keystore 不可用）、`local.invalid_params`、端口类 |
| `export.create` | `export()`（查重）→ 逐 alias `workspace()`（存在性）→ `put_export` | `{ export: ExportView }`（`revokedAt` 为 `null`） | `local.conflict`（`exportId` 已存在）、`local.not_found`（alias 未建立）、`local.invalid_params` |
| `export.list` | `exports()` | `{ exports: ExportView[] }`（含已撤销） | `local.invalid_params`（`params` 非 `{}`）、端口类 |
| `export.revoke` | `revoke_export` → `export()` 读回持久 `revokedAt` | `{ exportId, revokedAt }` | `local.not_found`（不存在/已撤销=重试）、`local.internal`（读不回撤销时间） |
| `import.add` | 不调用任何依赖 | —（恒失败） | **恒 `local.unavailable`**（本切片无 Node Link catalog 快照；不校验参数、不写记录） |
| `import.list` | `imports()` | `{ imports: ImportRecord[] }` | `local.invalid_params`、端口类 |
| `import.remove` | `remove_import` | `{}` | `local.not_found`（不存在=重试）、`local.invalid_params` |
| `audit.export` | `AuditStore::query`（时间区间 + 类别）→ 升序写出 | `{ outputPath, recordCount, sha256 }` | `local.conflict`（输出文件已存在）、`local.invalid_params`、端口类 |
| 其余 12 个方法（`device.pair.*`/`device.list`/`device.revoke`/`node.*`） | 未实现（WP3b2） | — | `local.unsupported`（§6） |

`audit.export` 的列集合（`jsonl` 每行一个对象，`csv` 首行同一集合，顺序固定）：
`at, action, actorKind, actorId, viaNodeId, localPrincipalRef, targetKind, targetId, outcome, detailDigest`
——与 `CORE_PORTS_AND_STORAGE.md` §7.3 的 `owned_audit` 列一一对应；`AuditRecord` 本身不含 prompt/回复/diff/终端/`rawJson`/附件/凭据/secret，本模块也不会凭空加入任何字段。

## `PortError` → `local.*` 映射表（§6）

| `core::model::PortError` | `local.*` | 备注 |
| --- | --- | --- |
| `NotFound(EntityRef)` | `local.not_found` | 消息含 `kind:id`（非敏感标识） |
| `Conflict(ConflictKind::Expired)` | `local.expired` | §6 的 `local.expired` 只服务 pairing 过期/被拒 |
| `Conflict(_)` 其余 8 个取值 | `local.conflict` | `AlreadyExists`/`IdentityMismatch`/`DuplicateOwnership` 等全部映射（逐条断言 `ConflictKind::ALL`） |
| `Unavailable(_)` 全部 7 个取值（含 `KeystoreUnavailable`/`IoError`/`StorageFull`） | `local.unavailable` | 逐条断言 `UnavailableKind::ALL` |
| `InvalidRequest(_)`（含 `require_local` 的 `authorization.scope_denied`） | `local.invalid_params` | core 的具名构造错误 `InvalidValue` 经 `From` 也落到这里 |
| `Corrupt(_)` | `local.internal` | **不转述**内层文本（可能是适配器诊断） |
| `Backend(_)` | `local.internal` | 同上；测试用含 `SELECT`/`db.sqlite` 的注入文本断言不泄漏 |

keystore 端口错误 → `local.*`：`KeystoreError::Unavailable` → `local.unavailable`（正式模式不得降级为明文存储）；其余（`EntryInvalid`/`EntryCorrupt`/`PurposeMismatch`/…）→ `local.internal`（实现缺陷，不转述内层文本）。

## 任务 2.10 / 2.12 的实现要点

| 文件 | 内容 |
| --- | --- |
| `local_admin/daemon.rs` | `DaemonControl`、`DaemonStatus`/`DaemonCounts`/`DaemonAgent`/`DaemonLink`/`DaemonLinkState`、`MAX_STOP_GRACE_MS`、`DaemonStatus::to_json`（camelCase、时间戳 §1.1、`nodePublicKey` 无填充 base64url） |
| `local_admin/params.rs` | 全部方法的参数形状知识：closed object（未知字段 → `local.invalid_params`）、`workspace.select` 的 canonicalize 与路径规则（绝对 → 无 `..` → 存在 → 是目录 → canonicalize）、`provider.configure` 的 id/字段名与凭据值校验、`export.create` 的首切片约束、`audit.export` 的过滤器解析（类别去重保序）、`map_port_error` |
| `local_admin/view.rs` | core 值对象 → `result` 投影（`workspace`/`agent`/`provider`/`export`/`import`，`camelCase`、数组保序） |
| `local_admin/audit.rs` | 审计导出写出：时间升序、`jsonl`（每行一个对象）/`csv`（RFC 4180 最小实现，表头也加引号）、`create_new` 拒绝覆盖、`sha2` 小写 hex 摘要 |
| `local_admin/router.rs` | `LocalAdminDeps`/`LocalAdminRouter`；12 个方法实现 + 未实现方法回 `local.unsupported`；`provider.configure` 的 keystore 条目方案与回滚/清理；失败只写结构化日志（method + code，不含参数） |
| `local_admin/test_support.rs` | `#![cfg(test)]` 的最小 fake 端口（只实现被本轮路由触及的方法，其余 `unreachable!("本轮路由测试未触及")`）与 `TestWorld`（临时目录随 drop 清理） |
| `crates/server/Cargo.toml` | 新增 `core = { path = "../core" }`、`identity-auth = { path = "../identity-auth" }`、`base64.workspace = true`；dev-dependencies 同步声明 core/identity-auth（依赖矩阵 §5 的 server 行允许，`[PV2]` 通过） |

实现口径（未在契约中明写、但在本 WP 内自洽并已登记为待确认项，见「未执行项与待澄清问题」）：

1. `agent.configure` 的 params **不承载** `ProviderEnvBinding`（§5.2 的六个字段里没有它），更新 profile 时**保留既有绑定**，避免一次 profile 编辑静默清空凭据注入；若新的 `envAllowlist` 不再包含某个绑定名，`AgentProfile::try_new` 拒绝整次写入（白名单是注入上限）。有测试覆盖。
2. `agent.configure` 的语义级词表（环境变量名与保留前缀）**只有 core 一份定义**：本层只做类型/结构校验，语义拒绝由 core 的构造器返回 `InvalidRequest` 再收敛为 `local.invalid_params`。
3. `provider.configure` 的 `values` 值必须非空：keystore 的 Provider 凭据条目要求长度 ≥1，空值只会在端口层报错，这里提前判定为 `local.invalid_params`（避免把「参数写错」报成「keystore 不可用」）。
4. `export.create` 的嵌套对象（`workspaceAliases[]`/`templates[]`）也是 closed object，`params`/`cachePolicy` 等字段必须显式给出（`params` 必须为空数组）。
5. `workspace.select` 的 alias 用 core 的 `WorkspaceAlias`（`^[a-z0-9][a-z0-9._-]{0,63}$`，权威是 node-link 的 `workspaceAlias`）而不是 §5.5 表格里更宽的 `^[A-Za-z0-9._-]{1,64}$`：Export 的 alias 必须已存在于 `owned_workspace`，两者必须同一命名空间（§5.2 明确「本地不得比 wire 宽」）。
6. 错误消息只回显**标识符类**对端输入（参数名、方法名、audit 类别名），**不回显自由文本值**（路径、命令、显示名、凭据值）——与 WP3a 已登记的同类口径一致。
7. `export.revoke` 的 `revokedAt` 取**读回的持久值**（`revoke_export` 提交后再 `export()`），读不回时回 `local.internal`，不用本层时钟猜。

## checks（逐 Check ID 汇总）

| Check ID | 命令 / 目录 | 配置与环境 | 结果 | 日志 |
| --- | --- | --- | --- | --- |
| [PV3] | `cargo test --locked -p server --all-features` @ 仓库根 | Windows x64；cargo/rustc 1.98.1；`--locked` | 退出码 0；**91 个用例**通过、0 失败、0 ignored（lib 63 + channel 14 + schema drift 6 + naming 4 + windows 4；unix 文件在 Windows 上 0 用例，属 `#![cfg(unix)]`） | `reports/wp3-server-methods.log` (3) |
| [PV3] | `cargo clippy --locked -p server --all-targets --all-features -- -D warnings` | Windows x64 | 退出码 0，无警告 | `reports/wp3-server-methods.log` (2) |
| [PV3]（跨目标编译） | `cargo clippy --locked -p server --all-targets --all-features --target x86_64-unknown-linux-gnu -- -D warnings` | 已安装 Linux std；**只编译不执行** | 退出码 0（无平台回归） | `reports/wp3-server-methods.log` (4) |
| [PV3] | `cargo fmt --all -- --check` | 默认 rustfmt | 退出码 0 | `reports/wp3-server-methods.log` (1) |
| [PV2] | `node scripts/check-crate-boundaries.mjs` @ 仓库根 | Node v24.19.0；`cargo metadata --no-deps` | 退出码 0：`crate boundaries OK: 11 个 crate ...`；`cargo tree -p server` 的工作区内依赖只有 `core`、`identity-auth`、`windows-local-ipc`（§5 矩阵 server 行 ✓） | `reports/wp3-server-methods.log` (5)(5b) |
| [PV1] | `npm run verify`（= `npm run check` 的 10 道门禁 + `cargo fmt --check` + `cargo clippy --locked --workspace ...` + `cargo test --locked --workspace --all-features`）@ 仓库根 | Node v24.19.0 / npm 12.0.2；离线 | 退出码 0；workspace **697 个用例**通过、0 失败、1 ignored（`storage-sqlite` 的夹具重建用例，属既有 `#[ignore]`）；10 道合同门禁全绿（含 `crate boundaries OK: 11`、`agentic 宿主入口检查完成：17 个文件`） | `reports/wp3-server-methods.log` (6)(10) |
| PC1 | `npm run check`（单独一轮） | 同上 | 退出码 0 | `reports/wp3-server-methods.log` (6) |
| LC1 | `rg -n "unsafe" crates/server/src` | ripgrep | 退出码 1（**零命中**） | `reports/wp3-server-methods.log` (7) |
| LC2 | 自检「正常路径无 `unwrap()`/`expect()`/`panic!`」 | 脚本逐行比对首个 `#[cfg(test)]` 行号 | 197 处命中，全部落在测试代码内：`local_admin/{audit,daemon,envelope,params,router}.rs` 与 `transport/local/{audit,framing,platform/unix}.rs` 的命中都在文件末尾的 `#[cfg(test)] mod tests` 内；`local_admin/test_support.rs` 的全部命中由文件首行 `#![cfg(test)]` 自我声明（脚本输出 `violations: []`） | `reports/wp3-server-methods.log` (8) |
| LC3 | `cargo tree --locked -p server --edges normal --depth 1` | — | 退出码 0；新增的工作区内依赖边只有 `core` 与 `identity-auth`（矩阵允许） | `reports/wp3-server-methods.log` (5b) |
| LC4 | 提交钩子（`.husky/pre-commit`：`fmt` + `npm run check` + `clippy --workspace -- -D warnings`）与 `commitlint` | 本机 | 三步全过、commitlint 通过；未使用 `--no-verify` | 提交输出（`f1a3cd4`） |

### 新增测试清单（38 个，全部无 `#[ignore]`、无跳过）

| 分组 | 用例 |
| --- | --- |
| `params.rs`（12） | `daemon_status_rejects_any_parameter`、`daemon_stop_checks_the_grace_bounds`、`workspace_select_canonicalizes_an_existing_directory`、`workspace_select_rejects_relative_parented_missing_and_file_paths`、`workspace_select_checks_alias_and_display_name_shapes`、`agent_configure_requires_all_six_fields`、`provider_configure_validates_ids_fields_and_values`、`export_create_accepts_the_documented_shape`、`export_create_enforces_the_single_slice_rules`、`export_revoke_and_import_remove_validate_the_id_shape`、`audit_export_parses_filters_and_rejects_bad_shapes`、`port_errors_map_to_the_documented_local_codes` |
| `audit.rs`（4） | `jsonl_is_ascending_one_object_per_line`、`csv_starts_with_the_column_header_and_escapes_fields`、`an_existing_output_file_is_never_overwritten`、`an_empty_selection_writes_an_empty_document` |
| `daemon.rs`（3） | `status_serializes_the_documented_shape`、`node_public_key_is_unpadded_base64url_of_the_65_bytes`、`link_state_tokens_match_the_document` |
| `router.rs`（19） | `daemon_status_returns_the_daemon_control_snapshot`、`daemon_status_rejects_params_and_daemon_stop_validates_the_grace`、`daemon_stop_propagates_a_composition_root_failure`、`workspace_select_persists_the_canonical_path_and_keeps_created_at`、`workspace_select_maps_storage_failures`、`agent_configure_writes_the_profile_and_reports_the_result_shape`、`agent_configure_preserves_existing_provider_env_bindings`、`provider_configure_writes_credentials_to_the_keystore_only`、`provider_configure_rejects_bad_params_and_unavailable_keystore`、`provider_configure_rolls_back_credentials_when_a_later_step_fails`、`export_create_requires_registered_aliases_and_a_free_id`、`export_list_includes_revoked_records_and_export_revoke_returns_the_persisted_time`、`import_add_is_always_unavailable_and_import_remove_maps_not_found`、`import_list_maps_a_dependency_failure_without_leaking_details`、`audit_export_writes_only_metadata_and_reports_the_digest`、`audit_export_maps_a_store_failure`、`unimplemented_methods_answer_local_unsupported`、`responses_always_carry_the_request_id_and_a_shape_valid_envelope`、`provider_keystore_refs_stay_within_the_label_budget` |

按任务要求逐方法核对「至少一条覆盖校验/错误路径的测试」：`daemon.status`（未知字段）、`daemon.stop`（越界/非整数/缺字段/组合根失败）、`workspace.select`（相对/`..`/不存在/文件/alias/displayName/端口失败）、`agent.configure`（六个字段逐个缺失、越界 id、类型不符、未知字段、白名单收窄）、`provider.configure`（id/kind/displayName/values 形状、空值、keystore 不可用、回滚）、`export.create`（首切片 12 类拒绝 + `local.not_found` + `local.conflict`）、`export.list`（未知参数 + 端口失败映射）、`export.revoke`（不存在/重试）、`import.add`（恒 `unavailable`）、`import.list`（端口失败不泄漏）、`import.remove`（不存在/重试）、`audit.export`（路径/格式/时间/类别/缺字段/未知字段、已存在文件冲突、端口失败）。方法级端到端行为（真实 SQLite + 真实 keystore + 真实 IPC）仍由 WP4 的 [PV4] 承担。

## 已知限制与偏差

1. **`AuditStore` 在工作区内没有生产实现**：全仓库只有 `core::broker::test_support` 的测试替身实现了 `AuditStore`，`storage-sqlite` 只建了 `owned_audit`/`imported_audit` 表与迁移。因此 `audit.export` 在本轮与 WP4 都需要组合根补一个 `AuditStore` 实现（`app` 可直接实现，或由 `storage-sqlite` 补），否则该方法的 [PV4] 端到端无法执行。**按依赖项登记，不写成已通过**。
2. **本机未执行 Linux/Unix 运行**：本机无 WSL/Linux，Unix 相关路径只做了 `--target x86_64-unknown-linux-gnu` 的 clippy 编译（[PV3]），真实执行属 Linux CI。本 WP 本身不含平台分支。
3. **`cargo-deny`（`deps`/`advisories`）与 `gitleaks` 只在 CI 运行**：本地无等价物，本轮未执行，不声称通过（新增依赖 `base64` 已在 lock 中、许可证属 `deny.toml` allow 列表，但按仓库口径仍以 CI 判定为准）。
4. **路由层失败的审计**：§7 要求「方法失败」也记审计事件，但 §14.2 的类别是闭合表且没有对应取值（最接近的 `authorization.denied` 只表达授权拒绝，误用会让事后无法区分）。本轮实现为**结构化日志**（`tracing::warn!` 只带 method + 错误码，不含 params）并**保留接线点**；连接级拒绝已由 WP3a 的 `AuditHook` 承担；撤销类与 `provider.configure` 的审计由 core 写集同事务提交，**未绕过 core 直接写库**。是否新增类别由主 Agent 决定。
5. **`import.add` 不校验参数**（按 spec 场景的字面要求：无论参数是否合法都回 `local.unavailable`），因此坏参数与合法参数得到同一错误码。
6. **`daemon.status` 的 `listen`/`links` 恒为空数组**：本切片没有网络 listener 与 Node Link 重连任务；类型注释已写明这是本切片的既定形状而非占位。

## 未执行项与待澄清问题（交主 Agent）

1. **`agent.configure` 保留既有 env 绑定**（本报告「实现口径」第 1 条）：需确认这是期望行为；若要「CLI 编辑即重置绑定」，需要 `params` 增字段（属不兼容变更，须改 §5.2 与 schema）。
2. **keystore 条目方案**（`keystore_ref` + 每字段标签）尚未写进任何权威文档；它是 `server` 与 WP4 的 `CredentialResolver` 之间的实际契约。建议在 `CORE_PORTS_AND_STORAGE.md` §5.3（`CredentialResolver`）或 §11.6 补一句，否则 WP4 只能照本报告实现。
3. **错误消息回显标识符类输入**（参数名、方法名、类别名）：与 WP3a 已登记的同类口径一致，若主 Agent 要求错误消息完全不回显对端输入，需要一并改口径（WP3a 的 `unknown_method` 已在同一条）。
4. **`values` 值必须非空**、**`export.create` 嵌套对象也是 closed object**、**`workspace.select` alias 采用 node-link 的更严正则**：三条都是本层在契约沉默处做的收窄，已在上文逐条说明理由，请确认是否要写回 §5.2/§5.5。
5. **`AuditStore` 缺生产实现**（限制第 1 条）：需要主 Agent 决定由 `app` 还是 `storage-sqlite` 提供，并在 WP4 派发时说明。
6. `tasks.md` 2.10/2.12 的勾选与 `verification.md` 的证据登记由主 Agent 负责（本 WP 不改规划文件）；独立 review（RV1 的 WP3 部分）尚未返回，本报告不把它写成 PASS。

## 资源释放

| 资源 | 归属 | 状态 |
| --- | --- | --- |
| 测试用的临时目录 | 本 WP（`TestWorld`） | 由 `TestWorld::drop` 删除 `acpr-wp3b1-router-*`；`params.rs`/`audit.rs` 的单测各自删除 `acpr-wp3b1-params-*`/`acpr-wp3b1-audit-*` 文件与目录 |
| 本机 Named Pipe / Unix socket | 不属于本 WP | 本 WP 未创建任何 endpoint（传输层未改动） |
| `reports/wp3-server-methods.log` | 本 WP | 新建（按仓库约定 `*.log` 不入库） |
| 仓库根 `target/` | 共享 | 未清空（增量缓存） |
| `git` 工作区 | 本 WP | 代码提交后 `git status --porcelain` 为空；本报告提交后同样应为空；未暂存任何文件 |

```yaml
handoff_index:
  - task_id: "2.10"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "f1a3cd4"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3b1-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "WP3b1 轮：cargo fmt --all -- --check（退出码 0）、cargo clippy --locked -p server --all-targets --all-features -- -D warnings（退出码 0）、cargo test --locked -p server --all-features（退出码 0；91 个用例通过、0 失败、0 ignored）在 Windows x64 本机执行；[R37]–[R41] 的方法级校验与结果形状由 lib 测试逐条覆盖（local_admin::params + local_admin::router）。跨目标 clippy（--target x86_64-unknown-linux-gnu）退出码 0。日志 reports/wp3-server-methods.log 的 (1)(2)(3)(4) 节。完整组成 = fee6073（Cargo.toml/Cargo.lock 与 4 个文件中间版本）+ f1a3cd4（最终差分与其余新增文件）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.12"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "f1a3cd4"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3b1-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "WP3b1 轮：[R49]–[R58] 的 Export/Import 与审计导出行为由 local_admin::params（首切片约束、过滤器解析）、local_admin::audit（升序/格式/摘要/不覆盖）与 local_admin::router（alias 存在性 local.not_found、exportId 重复 local.conflict、import.add 恒 local.unavailable、remove 重试 local.not_found、audit.export 文件冲突与元数据列集合）覆盖；同一轮 [PV3] 命令与退出码见上一行。PASS 表示本层行为与自检条件满足，不含 WP4 的真实 SQLite/keystore 端到端。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.10"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "f1a3cd4"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3b1-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "WP3b1 新增 core/identity-auth/base64 依赖后的 [PV2] 轮：node scripts/check-crate-boundaries.mjs 退出码 0（crate boundaries OK: 11 个 crate ...）；cargo tree --locked -p server --edges normal --depth 1 显示工作区内依赖边只有 core、identity-auth、windows-local-ipc，均在 §5 矩阵的 server 行允许范围内。日志 reports/wp3-server-methods.log 的 (5)(5b) 节。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.10"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "f1a3cd4"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3b1-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "WP3b1 轮的 npm run check（10 道合同门禁全绿，含 check:boundaries/check:drift/check:agentic）与 npm run verify（= check + fmt --check + workspace clippy + workspace test：697 passed / 0 failed / 1 既有 ignored）在仓库根执行，退出码均为 0；代码提交本身经 .husky/pre-commit 三步与 commitlint 通过。日志 reports/wp3-server-methods.log 的 (6)(10) 节。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.10"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "f1a3cd4"
    evidence_type: VALIDATION
    evidence_id: LC1
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3b1-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "附加本地检查：rg -n \"unsafe\" crates/server/src 零命中（退出码 1）；「正常路径无 unwrap/expect/panic」自检 197 处命中全部落在测试代码内（各文件 #[cfg(test)] mod tests 或首行 #![cfg(test)] 的 local_admin/test_support.rs），脚本输出 violations: []。日志 reports/wp3-server-methods.log 的 (7)(8) 节。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.10"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "f1a3cd4"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3b1-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "WP3b1 交付物：fee6073（crates/server/Cargo.toml 的 core/identity-auth/base64 依赖、Cargo.lock、local_admin/{audit,daemon,params,view}.rs 中间版本）+ f1a3cd4（local_admin/{router,test_support}.rs 新增、local_admin/mod.rs、lib.rs 与四个文件的最终差分，8 个文件 +2747/-95）；对 WP4 冻结的形状见本报告「冻结的公开形状」（DaemonControl/DaemonStatus/LocalAdminDeps/LocalAdminRouter 与 keystore 条目方案）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.12"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "f1a3cd4"
    evidence_type: CHECK
    evidence_id: PV4
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3b1-handoff.md
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "方法级端到端（真实 SQLite + 真实 keystore + 真实 IPC）属 WP4 的集成轮，本 WP 不执行；另有一项 WP4 前置依赖：工作区内尚无 AuditStore 的生产实现（storage-sqlite 只建表与迁移），audit.export 的端到端需要组合根补实现。本行不写成 PASS。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.6"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "f1a3cd4"
    evidence_type: REVIEW
    evidence_id: RV1
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "WP3 的独立 review 尚未返回；须由不继承本实现对话的执行者检视 WP3b1 的方法级校验、凭据不落盘/不回显、PortError 映射与审计导出列集合。本报告不把自检当作其结论。"
    source_evidence: NOT_APPLICABLE
```
