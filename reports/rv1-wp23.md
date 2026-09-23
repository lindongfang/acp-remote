# rv1-wp23.md — WP2/WP3（core 端口 / 用例 / broker）独立对抗性 review

## 0. 检视对象、范围与方法

- 仓库 / 分支：`D:\Project\acp-remote`，`feat/admin-state-persistence-v2`
- Review ID：`RV1-WP23`；Review Type：branch（固定版本）；Review Stage：WP2/WP3 交付前独立检视（任务 `3.4`，与 `3.3` 并行）
- 被检视 revision（固定）：`git rev-parse HEAD` = `5404610d35b22c9d427b88b0cea2c61d1e7a5e15`；变更起点（基线）= `28f8cb9`；W0 契约基线 = `013f2b9`
- 被检视 diff：`git diff 28f8cb9..013f2b9 -- crates/core/src/ports.rs crates/core/src/use_cases.rs crates/core/src/broker.rs`（`ports.rs` +236/−44、`use_cases.rs` +554/−92、`broker.rs` +198/−79）。`git diff 013f2b9..5404610 -- crates/core/src/` 在本切片内为空（后续三个提交只改 `crates/core/src/model/{identity,tests}.rs` 与 `crates/storage-sqlite/**`），因此 `5404610` 工作树里这三个文件与 `013f2b9` 逐字节相同，本报告以工作树内容为准。
- `git status --porcelain`（检视时刻）：只有宿主未跟踪项 `?? .omp/`、`?? .pi/prompts/opsx-verify.md`、`?? .pi/skills/openspec-verify-change/`、`?? reports/parked-idle-identity-retention.patch`（与本变更无关，未纳入检视）。
- 检视范围：`crates/core/src/ports.rs`、`crates/core/src/use_cases.rs`、`crates/core/src/broker.rs`（含 `test_support` 的测试替身与 `port_error_public`）。**范围外**：`crates/core/src/model/**`（`3.2`）、`crates/storage-sqlite/**`（`3.6`）、`docs/**`（`3.8`）——对这三者的阅读只用于核对跨界分发点与 §-引用的正文归属，不产出针对它们的发现。
- 对照的权威：`CORE_PORTS_AND_STORAGE.md`（§3.1 / §3.5 / §3.6 / §3.7 / §4 / §5.1 / §5.3 / §6 / §7.3 / §7.4 / §9 / §11.1 至 §11.9）、`SECURITY_DESIGN.md` §14.2、`LOCAL_ADMIN_PROTOCOL.md`（§5.3 / §5.4 / §6）、`MODULE_ARCHITECTURE.md`（§3.1 / §4.7 / §5）、`IDENTITY_AND_AUTH_CONTRACT.md`（§2 / §3）、变更仓库内五份 spec、`proposal.md` / `design.md` / `tasks.md`，以及 `verification.md` 的 Check Plan Changes（偏差登记的权威）。
- 只读命令与退出码：`git rev-parse` / `git status` / `git diff` / `git log` / `git grep`（均退出 0）；`node scripts/check-doc-links.mjs`（本报告落盘后复跑，结果见 §4）；`node scripts/check-crate-boundaries.mjs`（只读，退出 0：`crate boundaries OK: 6 个 crate 的依赖方向与 §5 矩阵一致`）。
- **未运行** `cargo build/test/clippy/fmt`（assignment 禁止并行构建目录争用），因此本报告不声称任何编译或测试结果。`reports/*.log` 是实现者证据，下文凡引用处均标注「未独立复算」，仅按源码与合同判定。

## 1. 逐条核对结论

### 1.1 端口纯度 —— 成立

- `crates/core/src/ports.rs` 的 `use` 只有 `std::sync::Arc`、`async_trait::async_trait` 与 `crate::model::*`。`grep -nE "sqlx|serde|tokio|axum|std::process|Command::new|std::path|PathBuf|cfg\("` 在该文件只有 1 处命中，且是模块头注释里「不出现 `sqlx`…」的措辞（`ports.rs:7`）——没有 wire DTO、SQLite record、runtime、子进程或平台类型。
- `crates/core/src/**` 无平台 `cfg`（`cfg(windows|unix|target_os|target_family|target_arch)` 无命中）；`use_cases.rs` 只用 `std::fs` / `std::path` 做 §5.1 明文要求的 workspace 解析，属 std 而非平台条件代码。
- 16 个写集 DTO（`DeviceWrite` `ports.rs:619`、`NodeWrite` `626`、`DeviceRevocation` `634`、`NodeRevocation` `642`、`PairingWrite` `650`、`PairingClaimWrite` `658`、`PairingSettlementWrite` `666`、`ExpiryWrite` `674`、`ExportWrite` `680`、`ExportRevocation` `687`、`ImportWrite` `698`、`ImportRemoval` `707`、`ProfileWrite` `715`、`WorkspaceWrite` `722`、`ProviderRefWrite` `729`、`SeedWrite` `736`）与 `PendingAudit`（`597`）、`WriteContext`（`612`）只携带 core 值对象与集合（`Vec<ExportId>` / `Vec<AgentProfile>` / `Vec<PendingAudit>`），无 `serde_json::Value`、无数据库列名、无 wire discriminator。
- `PendingAudit` 的字段与 `AuditRecord` 一一对应（只少 `at`）✔；`WriteContext.at` 由调用方从 `Clock` 取，`UseCases` 的每条管理入口都先 `self.clock.now()`，与 `CORE_PORTS_AND_STORAGE.md` §2 的「存储层不读系统时间」一致 ✔。
- `CredentialResolver`（`ports.rs:841`）的文档与签名（白名单 ∩ 绑定、引用失效 → `Unavailable(KeystoreUnavailable)`、失败关闭、不落盘不进日志）与 §5.3 的转录一致；返回 `Vec<(String, SecretValue)>`，而 `SecretValue` 不实现 `Debug`，因此返回值结构上不可能被 `Debug` 打印 ✔。该 trait 在本变更内**没有实现**（`crates/core` 无 `impl CredentialResolver`），详见 §3 的范围判定。

### 1.2 写集语义 —— 每条管理入口都成立（一处审计缺口见 WP23-2）

逐入口核对（一次端口调用 = 一个写集 = 状态 + 引用 + 审计）：

- `put_device`（`use_cases.rs:419`）：先 `trust.device` 读面取旧行，再一次 `put_device(DeviceWrite)`；仅当「已存行 scopes 变化」时携带 `DeviceScopesChanged`，与 §11.6 第 1 条逐字一致；新建行无对应已登记安全动作（`AuditAction` 无「设备新建」取值），空审计合法 ✔。
- `revoke_device`（`440`）/ `revoke_node`（`511`）：一次写集 + `DeviceRevoked` / `NodeTrustRevoked` ✔（§11.6 第 5 条）。提交后才由组合根关连接，core 不做连接清理 ✔。
- `create_pairing`（`525`）：一次 `PairingWrite` + `PairingCreated` ✔。
- `claim_pairing`（`543`）：一次 `PairingClaimWrite` + `PairingClaimed`，target 取 `EntityRef::Pairing(claim.pairing())` ✔（§11.6 第 3 条）。
- `settle_pairing`（`580`，动作选择在 `589-594`）：一次 `PairingSettlementWrite`；动作由 `settlement.is_approved()` 在 `PairingApproved` / `PairingRejected` 之间选择，与消费侧的两个分支（`crates/storage-sqlite/src/admin/trust.rs` 的 Rejected / Approved 分支各调用一次 `insert_audit_rows`）一一对应，拒绝路径不创建信任 ✔。**节点批准少一个已登记安全动作（WP23-2）。**
- `expire_pairings`（`605`）：一次 `ExpiryWrite`，`context.audit` 为空。§11.6 第 6 条把 `pairing.expired` 的写入定在扫描路径（存储层按配对一行补写），`verification.md` 的 Check Plan Changes 第 17 与 23 条已登记该语义（含「本轮不采纳加固建议」），故按 assignment 第 2 条不重复报为缺陷。消费侧核对：`crates/storage-sqlite/src/admin/mod.rs:72-105` 在 `context.audit` 为空时退回 `Actor::LocalCli`（不 panic、不静默丢弃真实数据），当前唯一调用方传空集合 → 无静默丢弃 ✔。
- `put_export`（`633`）/ `revoke_export`（`647`）：一次写集 + `ExportCreated` / `ExportRevoked` ✔（§11.6 第 7 条）；`export.update` 也写 `export.created`，与该条一致。
- `add_import`（`678`）：空 `export_ids` 先以 `InvalidRequest` 拒绝，再以 `exports = record.export_ids().to_vec()` 构造写集（两处集合恒等）＋ `ImportAdded` ✔（§11.6 第 7 条与 Check Plan Changes 第 3 条）。
- `remove_import`（`703`）：**只发起一次 `ImportRemoval`**，不再串联 `deliveries.drop_import`；`UseCases::deliveries()`（`842`）只把交付端口暴露给接入层做连接级清空 ✔（§11.6 的「两处删除权威不得重叠」）。用例 `remove_import_is_one_atomic_write_set_with_audit`（`1353-1386`）断言 `drop_import_calls == 0`、管理行被删、`write_audits == [ImportAdded, ImportRemoved]` ✔。
- 本地配置四入口：`put_profile`（`741`）/ `put_workspace`（`771`）/ `mark_seeded`（`822`）都传空审计，三者都不是已登记安全动作（§11.6 的 `WriteContext.audit` 条正好点名 `agent.configure` 与 `workspace.select`）✔；`put_provider_ref`（`796`）携带 `ProviderConfigured` 且 target 为 `EntityRef::Provider(reference.id())` ✔。
- 「先写状态再补审计」在本切片已消失：`audit_action`（`954`）保留 `#[allow(dead_code)]` 且**无任何调用点**（`grep` 只有定义处命中），`verification.md` Check Plan Changes 第 5 条已登记该保留 ✔。
- **消费侧分发点核对（跨边界，逐条读被测方之外的代码）**：16 条管理写路径在 `crates/storage-sqlite/src/admin/**` 内都调用 `insert_audit_rows(&mut tx, &write.context.at, &write.context.audit)`（`trust.rs` 6 处、`export.rs` 4 处、`local_config.rs` 4 处），不存在「写集携带的审计被静默丢弃」的分支；`ImportWrite.exports` 的分歧检查落在 `crates/storage-sqlite/src/admin/export.rs:394-399`，重复归属 → `Conflict(DuplicateOwnership)`（同文件 `416-426`）；`NodeWrite` 的双角色指纹一致、`DeviceWrite` 的不换绑/不复活、`PairingClaimWrite` 的本机绑定比对、`PairingSettlementWrite` 的「peer 公钥从对端行读回」都有对应实现。因此 `ports.rs` 各 DTO 文档里的断言在消费点均成立。

### 1.3 错误映射 `port_error_public` —— 成立

- `broker.rs:2502-2547`：`PortError::Conflict(kind)` 与 `PortError::Unavailable(kind)` 现在都是**逐值穷举匹配，没有 `_` 通配臂**——`ConflictKind` 的 9 个取值与 `UnavailableKind` 的 7 个取值与 `crates/core/src/model/error.rs:15-34` 和 `:74-90` 的声明集合一一对应，因此「新增取值时编译器会报错」成立；注释所引 `CORE_PORTS_AND_STORAGE.md` §11.6 的 `[已裁定]` 段确有该隐性陷阱的规则正文 ✔。
- `ConflictKind::AlreadyExists` / `IdentityMismatch` / `DuplicateOwnership`（`broker.rs:2519-2524`）与 `UnavailableKind::KeystoreUnavailable`（`2534-2536`）都有**显式分支**，无一被通配臂吞掉 ✔。四者的映射值与旧通配臂相同（`internal.unavailable`），把管理冲突留给本地管理适配器映射为 `local.conflict` / keystore 不可用映射为 `local.unavailable`；`LOCAL_ADMIN_PROTOCOL.md` §6 确实定义了这两个码 ✔，与 `design.md` D4 的意图（消除静默落臂的陷阱）一致。
- 公开错误不泄漏：全部分支的 `code` / `message` 都是静态 `&str`，`PublicError::coded` 不携带 `details`；`PortError::InvalidRequest(_)` 分支丢弃 reason 文本、统一写「请求不合法」✔。既有取值（`VersionMismatch` / `AlreadyResolved` / `IdempotencyConflict` / `RemoteUnavailable`）的码、文案与 retryable 相对改动前逐字不变（`git diff` 逐行比对）✔，无行为回归。
- 新取值是否会在命令路径上静默降级：管理写入口全部 `require_local(Actor::LocalCli)`，新 `ConflictKind` 取值只由管理 store 产生，命令路径不可达；即便出现也走具名分支（同上），不落通配臂 ✔。

### 1.4 workspace 解析 —— 校验与分类成立；解析时机有缺陷（WP23-1）

- `resolve_workspace`（`use_cases.rs:1005-1022`）：拒绝非绝对路径与含 `..` 的输入（`Component::ParentDir`）、要求 `fs::metadata().is_dir()`、以 `fs::canonicalize` 结果为权威值（解析 symlink / junction / 大小写 / `.`）、再经 `ResolvedWorkspace::try_new` 约束绝对路径形状；任一失败统一 `Unavailable(UnavailableKind::IoError)` ✔，与 `CORE_PORTS_AND_STORAGE.md` §5.1 的 workspace 解析规则（绝对、存在、目录、`canonicalize` 权威、拒绝相对路径与 `..`）逐项相符。
- 时机与后端输入：解析在 `self.broker.create_session(...)`（`use_cases.rs:150`）之前完成，而 `SessionBackendFactory::create` 在 core 内的**唯一**调用点是 `broker.rs:1210-1212`（`commit` 之后）→「解析发生在后端被调用之前、后端只收到 `ResolvedWorkspace`」成立 ✔（§5.1）。
- 失败分类：未登记别名 → `InvalidRequest`（参数类，`use_cases.rs:145-147`）；别名已登记但本机解析失败 → `Unavailable(IoError)`（`148`），**没有**降级为参数错误 ✔。Export 声明校验按 `verification.md` Check Plan Changes 第 2 条归 `server::node_link`（本次未实现，已登记偏差）。
- 路径不泄漏：`canonical_path` 只出现在 `CreateSessionRequest.workspace`，而该请求在 core 内的唯一去向是 `SessionBackendFactory::create`——broker 组装 `NewSession` 时只带 `title` / `agent`（`broker.rs:1188-1191`），`owned_session` 的 v2 DDL 也没有 workspace 列；错误侧 `Unavailable(IoError)` 是无 payload 的 unit 变体，经 §1.3 的映射只输出静态文案。因此**结构上成立**，但**没有任何用例断言「事件/错误里不出现路径」**（`FakeBackend` 不记录入参，三个新用例只断言错误分类），下表的该场景因此记为「无用例覆盖（结构性保证）」。
- **缺陷**：本地存储读 + 文件系统访问发生在 `broker.authorize` 之前（`use_cases.rs:140-148` 先于 `150` 的 `broker.create_session`，而授权在 `broker.rs:1180-1182` 才发生）→ WP23-1。

### 1.5 注释里的 §-引用核对（逐条给正文依据）

| 位置 | 引用文本 | 核对结果 |
|---|---|---|
| `use_cases.rs:489` | 「配对确认路径负责写 `node.paired`（§11.6 第 2 条）」 | `CORE_PORTS_AND_STORAGE.md` §11.6 第 2 条讲的是 `put_node` 的四条规则，**没有**「确认路径写 `node.paired`」这条；且 `node.paired` 在实现里无任何写入方 → WP23-2 |
| `use_cases.rs:738` | 「至多一个默认，§11.7 的部分唯一索引」 | `CORE_PORTS_AND_STORAGE.md` §11.7 的正文把九张表与两个索引**声明在 §7.3**，索引本体不在 §11.7；`CORE_PORTS_AND_STORAGE.md` §11.6 的空审计 bullet 写的是「§7.3 的部分唯一索引」→ WP23-3 |
| `use_cases.rs:804` | 「本变更新增 `EntityRef::Provider`（§3.1/§11.6）」 | `CORE_PORTS_AND_STORAGE.md` §3.1 的 `EntityRef` 行确有 `Provider(String)` 与 `kind()` 的 `provider` token ✔；§11.6 通篇没有该变体（§3.1 的「`Provider` 变体见 §11.6」因此也无落点）→ 与 WP23-3 同类，见该条 |
| `use_cases.rs:126` / `1000` / `1258` / `1309` / `1322` | §11.9 | `CORE_PORTS_AND_STORAGE.md` §11.9 现在是「已并入 §5.1 / §3.6」的指针加设计理由，正文自述「解析时机、校验、失败分类、别名命名空间与不泄漏规则在 §5.1」；引用可按指针解析到规则正文，判**可接受**（严格口径下 §11.9 本身已不含规则） |
| `use_cases.rs:704` | §11.6（两处删除权威不重叠） | 该节确有该 bullet ✔ |
| `use_cases.rs:489` / `740` / `770` | §11.6 的写集语义与空审计规则 | 该节的「提交模型」段与 7 条写集语义、`WriteContext.audit` 的允许/必须条件都在 ✔ |
| `ports.rs:593` / `607` / `617` / `624` / `632` / `640` / `648` / `655` / `663` / `672` / `678` / `685` / `692` / `704` / `712` / `720` / `727` / `734` / `741` | §11.6 | 该节保留的 7 条写集语义覆盖这些 DTO 的文档断言（`put_device` / `put_node` / claim / settle / revoke / expire / Export-Import），逐条对照一致 ✔ |
| `ports.rs:761` | §11.5（确认事务从对端行读回公钥） | 该节的「认领事务把公钥写进 `owned_pairing_peer`；确认事务在同一事务内把它读出来写入 `owned_peer_key`」✔ |
| `ports.rs:748-749` | §11.5（禁止取第一行） | 该节的读取面 bullet 确有该禁止 ✔ |
| `use_cases.rs:212` | §11.4 | 既有行，不在本 diff 内，按判据不报 |
| `broker.rs:2519-2531` | §11.6 的 `[已裁定]` 段、`LOCAL_ADMIN_PROTOCOL.md` §6 | 两处正文都在 ✔ |

### 1.6 `verification.md` 四条登记偏差与代码的一致性

| Check Plan Changes | 代码证据 | 判定 |
|---|---|---|
| 1 `EntityRef::Provider(String)` | `crates/core/src/model/ids.rs:504` 有变体、`kind()` 返回 `"provider"`；`use_cases.rs:804-806` 用它作审计 target。跨边界核对：`owned_audit.target_kind` 是自由 `TEXT NOT NULL`（`crates/storage-sqlite/src/migrate.rs:172`，无 CHECK），`'provider'` 可落库；`provider.configured` 在两条审计 CHECK 的字面量里（同文件 `165-168`）✔ | 一致 |
| 2 `create_session` 增加 `workspace_alias` 参数、Export 声明校验归 `server::node_link` | 签名（`use_cases.rs:134-139`）与两个用例调用一致；文档注释同样把 Export 校验归 `server::node_link` | 一致 |
| 3 `ImportWrite.exports == record.export_ids()` | `ports.rs:698-706` 的 DTO 文档、`use_cases.rs:679-684`（core 侧复制 `export_ids()`，两处恒等）、消费侧 `crates/storage-sqlite/src/admin/export.rs:394-399` 的 `same_set` 校验 | 一致 |
| 4 `audit_action` 保留 `#[allow(dead_code)]` | `use_cases.rs:954-955` 保留、无调用点 | 一致 |

未发现「代码注释声称记录在案但 `verification.md` 查不到」的偏差；`node.paired`（WP23-2）**不是**已登记偏差——`verification.md` 的 WP6-2 行只声称「`settle_pairing` 的节点分支写 … + `node.paired` 审计」，而 core 的写集里是 `PairingApproved`（见该 Findings 条）。

### 1.7 测试替身与新增用例

- 替身改动与新签名一致：`FakeTrust` / `FakeExports` / `FakeLocalConfig` 逐方法对齐 §5.3（`broker.rs:4157-4400`），`FakeWorld` 新增 `profiles` / `workspaces` / `provider_refs` / `seed` / `write_audits` / `drop_import_calls`（`broker.rs:3079-3086`）。`FakeLocalConfig::mark_seeded` 在已种子时仍会重写 profile（与真实 store 的「已初始化即零写入」不同），属替身简化，不影响被检切片判定。
- 新增 5 个 core 用例断言的都是可观察结果（端口返回值分类、`FakeWorld` 的调用计数与写集审计列表、`resolve_workspace` 的返回值），没有断言源码文本或私有分支 ✔。
- 覆盖面缺口（不构成缺陷，见 §3）：`claim_pairing` / `settle_pairing` 的替身直接返回 `Err(NotFound)`，因此**生产写集的审计动作选择没有用例**；`put_device` / `create_pairing` / `put_export` / `put_provider_ref` 等入口的替身不记录 `write_audits`，只有 `revoke_device` 与 import 两入口有「审计随写集」断言。

## 2. Findings

| ID | 严重度 | 位置 | 触发条件与证据 | 影响 | 建议与假想回退 |
|---|---|---|---|---|---|
| WP23-2 | 阻断（修在 core 写集侧或改述合同/注释，二选一必须先决定） | `crates/core/src/use_cases.rs:489`（注释断言）+ `589-594`（写集动作） | 触发：Owner 侧 `node.pair.confirm` 批准任何节点配对。证据：① core 的 `settle_pairing` 只按 `settlement.is_approved()` 在 `PairingApproved` / `PairingRejected` 之间选择，节点与设备两族共用同一动作；② 存储层只把写集携带的审计落库（`crates/storage-sqlite/src/admin/trust.rs` 的 Approved / Rejected 分支各 `insert_audit_rows(... write.context.audit)`），`approve_node` 自身不写任何审计；③ 全仓库 `git grep "node\.paired"` 的命中只有枚举定义（`crates/core/src/model/identity.rs:872`）、两族审计表的 CHECK 字面量、夹具/测试数据，**没有任何写入方**；④ 唯一断言过 `node.paired` 行的用例是 `crates/storage-sqlite/tests/admin_store.rs:792-806`，而那一行的动作是**用例自己**塞进写集的（`audit(AuditAction::NodePaired, ...)`），生产调用方不会构造它。⑤ 注释所引 `CORE_PORTS_AND_STORAGE.md` §11.6 第 2 条只描述 `put_node`，不含该规则。 | 本变更登记的安全动作里 `node.paired` 永远无法出现在审计日志中：`SECURITY_DESIGN.md` §14.2 的最低审计集合含 `node.paired`，事后无法回答「某个节点是否经配对批准」；`enum_coverage`（只断言 DDL 字面量与 `AuditAction::ALL` 相等）与上述存储用例都会保持绿，因此该缺口不会被任何现有门禁发现。若判定「节点配对的对应审计就是 `pairing.approved`」，则注释与合同措辞必须改（并说明 `NodePaired` 的预期写入方）；若判定应由本路径写，则必须补写入。 | 三种闭合方式择一：① core 在 settle 前读一次 `trust.pairing(id)` 并按目标族追加/选择 `NodePaired`；② 存储层像 `insert_expiry_audit` 那样在 `approve_node` 内按配对补写；③ 改述 `CORE_PORTS_AND_STORAGE.md` §11.6 与 `use_cases.rs:497` 的注释（点明 `pairing.approved` 覆盖节点配对、`node.paired` 留给后续握手路径）。**假想回退**：实现改回「节点批准也写 `node.paired`」后，现有全部用例仍会通过——`admin_store` 的 `node.paired` 计数来自用例自造的写集，`enum_coverage` 只看字面量，`migration` 只看夹具行；即当前**没有任何断言能发现这条路径的审计缺口**，必须新增一条「用 core 生产写集批准节点配对并断言审计动作」的用例（core 侧需让 `FakeTrust` 记录写集审计并返回成功，或由存储侧用例驱动 `UseCases`）。 |
| WP23-1 | 建议级（中） | `crates/core/src/use_cases.rs:140-148` | 触发：任何**将被拒绝**的 `session.create` 调用（`Actor::Device` 的 scopes 不含 `session.create`，或 `Actor::Node` 名下没有覆盖 `grant.remote-work` 的 Import / Export）。证据：`create_session` 先做 `config.workspace(&alias)` 读与 `fs::metadata` / `fs::canonicalize`（`140-148`），再调用 `self.broker.create_session`（`150`），而授权判断在 `broker.rs:1180-1182` 才发生；`broker.rs:493` 的既有约定是「**所有用例入口的第一步**：按 actor 的 scope/grant 与 Export 交集判定」，本文件其余入口（如 `list_sessions` `158-166`、`read_session` `173-181`）都先授权后访问。 | ① 未授权调用方仍能让 core 读取本机 `owned_workspace` 并对解析结果做文件系统访问（`metadata` + `canonicalize`），授权拒绝不再先于本机访问。② 该调用方拿到的不是授权拒绝，而是解析类错误：未登记别名 → `InvalidRequest`（wire `protocol.schema_invalid`）、已登记但目录缺失/非目录 → `Unavailable(IoError)`（wire `internal.unavailable`，retryable），因此在 wire 上可区分「别名已登记且目录当前存在」（得到 `authorization.scope_denied`）与「别名已登记但目录缺失」——对本机文件系统/登记状态的一个可观察预言机。无数据损坏、无授权绕过（调用最终仍失败关闭）。 | 在解析之前先授权，例如在 `use_cases.rs:140` 之前插入一次 `self.broker.authorize(actor, "session.create", None, &self.ids.request_id()).await.map_err(Denied::into_port_error)?;`（拒绝路径只写一条 `authorization.denied` 审计，且随后 `broker.create_session` 的授权对本地 actor 恒成功、不会重复写审计），或把 alias 解析下移到 `Broker::create_session` 内授权之后。**假想回退**：把解析块移到 `broker.create_session` 之后（或让 broker 内部先授权再解析）→ 未授权调用方对同一别名的三种输入都会得到同一个 `authorization.scope_denied`、且不再触发本机访问；现有 3 个 workspace 用例全用 `Actor::LocalCli`，**都不会失败**，因此该顺序回归当前无用例覆盖（需补一条「非 `LocalCli` actor + 未登记/缺失目录别名必须得到授权拒绝且不触达配置端口」的用例）。 |
| WP23-3 | 信息级 | `crates/core/src/use_cases.rs:738` | 注释写「切换默认是同一写集（至多一个默认，§11.7 的部分唯一索引）」。证据：`CORE_PORTS_AND_STORAGE.md` §11.7 的标题与首段把九张管理表与两个索引声明在 §7.3 / §7.4（该节不含 DDL 本体）；同一文档的 §11.6 在讲 `put_profile` 时写的是「§7.3 的部分唯一索引」，`crates/storage-sqlite/src/admin/local_config.rs:236` 的注释也引 §11.6。同类第二处：`use_cases.rs:804` 的「（§3.1/§11.6）」——§3.1 确有 `EntityRef::Provider`，但 §11.6 没有该变体（文档自己的 §3.1 也写「`Provider` 变体见 §11.6」，同样无落点）。 | 读者按注释去 §11.7 找索引会看不到 DDL（被引到 §7.3 的指针）；不影响运行行为，但属「引用与正文不符」，且会把 §3.1→§11.6 的断链再传播一次。 | 把该注释的引用改成 §7.3（或改为「§11.6 的 `put_profile` 条」），并同步修正 §3.1 的「见 §11.6」指针（后者属 `3.8` 的文档面，本报告只报不改）。**假想回退**：无运行期断言可覆盖注释引用；判据只能是人工核对（本报告 §1.5 的逐条表即为核对记录）。 |

## 3. 规格场景覆盖

判定口径：本切片只负责 core 侧（端口形状、用例写集、错误映射、workspace 解析）；落在存储/文档/后续 crate 的场景标注范围，不因此判本切片不通过。

| 规格 / 场景 | 对应用例 或 范围判定 |
|---|---|
| admin-state-persistence：审计写入失败时状态不落库 | 存储侧用例 `a_failed_audit_write_rolls_back_the_whole_write_set`（`crates/storage-sqlite/tests/admin_store.rs`，未独立复算，见 `reports/wp6-admin-store-tests.log`）+ 本切片保证「审计随写集而非事后补写」（`revoke_device_carries_its_audit_in_the_write_set`、`remove_import_is_one_atomic_write_set_with_audit`） |
| admin-state-persistence：约束冲突时不留下半条授权 | 一次端口调用 = 一个写集的构造在 `1.2` 逐入口核实；行为断言在存储侧（`a_constraint_failure_during_approval_leaves_no_half_authorization`，未独立复算） |
| admin-state-persistence：并发认领只有一个成功 | 存储侧（`second_claim_of_the_same_pairing_is_rejected`）；core 侧无用例（`FakeTrust::claim_pairing` 直接返回 `Err(NotFound)`） |
| admin-state-persistence：拒绝或过期不创建信任 | 拒绝路径：`settle_pairing` 写 `PairingRejected` 且不建信任（本切片 `589-594` 逐行核实）、存储侧 `rejected_settlement_creates_no_trust_and_ends_the_pairing`；「已过期」子分支由存储层在写任何行之前返回 `Conflict(Expired)`，`reports/rv1-wp6.md` 已记为低优先级缺口（终态与 `pairing.expired` 由过期扫描补），与本切片无关 |
| admin-state-persistence：重启后撤销仍然有效 | 存储侧（`device_revocation_is_idempotent_and_survives_reopen`、`node_revocation_covers_both_roles_and_survives_reopen`） |
| admin-state-persistence：重启终结已过期的未确认配对 | core 提供 `expire_pairings` 入口（`605`，空审计，已登记偏差）；终结行为在存储侧（`expired_pairing_is_refused_on_claim_and_terminated_by_restart_sweep`） |
| admin-state-persistence：同一 Export 归属冲突被拒 | core 侧 `add_import` 只透传 `export_ids()`；冲突判定在存储侧（`import_ownership_is_exclusive_and_write_set_divergence_is_rejected`），`ports.rs:698-706` 的 DTO 文档与该行为一致 |
| admin-state-persistence：完整移除后审计仍在 | core 侧 `remove_import_is_one_atomic_write_set_with_audit`（单次写集、无 `drop_import` 串联、写集带 `ImportRemoved`）；「审计行仍在」在存储侧同用例 |
| admin-state-persistence：未登记动作无法写入 | 范围外于本切片：`AuditAction` 是闭合枚举＋两族 DDL 的 `action` CHECK 逐值（`crates/storage-sqlite/tests/enum_coverage.rs`）。**但 `node.paired` 属登记动作却无写入方 → WP23-2** |
| admin-state-persistence：失败请求的审计不含内容 | 本切片：`PendingAudit` 只带 `action/actor/via_node/local_principal_ref/target/outcome/detail_digest`（`ports.rs:597-605`），其中 `target` 是 `EntityRef`（id 文本）、`detail_digest` 是 `Digest`，结构上不可能承载正文；错误侧公开映射只输出静态文案（§1.3）。无「失败审计列级」专门断言 |
| workspace-resolution：后端只收到已解析路径 | 代码路径断言成立（解析在 `use_cases.rs:150` 之前，`backends.create` 的唯一调用点 `broker.rs:1210-1212`，`NewSession` 只带 `title`/`agent`）；**无用例断言「后端收到的请求含 `ResolvedWorkspace`」**（`FakeBackend` 不记录入参）→ 无对应断言 |
| workspace-resolution：未登记别名在调用后端前失败 | `create_session_rejects_unregistered_workspace_alias`（断言 `InvalidRequest`）；未断言「未触达后端」（替身无调用计数）→ 部分覆盖 |
| workspace-resolution：规范化结果作为权威值 | `workspace_resolution_canonicalizes_and_rejects_invalid_inputs` 断言解析结果存在且为绝对路径；**symlink / junction / 大小写同一化未覆盖**（无该用例） |
| workspace-resolution：拒绝非绝对路径与父目录引用 | 同用例：相对路径、含 `..`、不存在路径、非目录（文件）四类都断言 `Unavailable(IoError)` ✔ |
| workspace-resolution：已声明别名解析失败报服务端错误 | `create_session_reports_local_resolution_failure_as_unavailable` ✔（与参数类错误可区分） |
| workspace-resolution：未声明别名报参数错误 | 核心的「已登记」判定覆盖相邻语义（`create_session_rejects_unregistered_workspace_alias`）；Export 声明校验按 Check Plan Changes 第 2 条归 `server::node_link`（未实现，已登记） |
| workspace-resolution：事件与错误都不含路径 | 结构性成立（见 `1.4`：请求只进 `backends.create`、错误为无 payload 的 unit 变体）；**无用例断言**（无事件/错误快照检查） |
| local-agent-config：默认 profile 唯一且切换原子 | 存储侧（`default_profile_switch_is_atomic_and_unique`）；core 侧只透传 `ProfileWrite`（`741`），注释所指 `CORE_PORTS_AND_STORAGE.md` §7.3 的部分唯一索引在存储侧落地 |
| local-agent-config：非法绑定在写入时被拒 | 存储侧（`profile_bindings_must_reference_registered_provider_fields`）；白名单/重复/保留名由 core 值对象构造器拒（`crates/core/src/model/config.rs`，本切片范围外） |
| local-agent-config：空种子也标记已初始化 / 重复打开不重导种子 | 存储侧（`seed_marks_initialized_once_and_ignores_later_seeds`）；core 侧 `mark_seeded` 只透传 `SeedWrite`（`822`），写集注释与 §11.6 一致 |
| local-agent-config：引用版本推进 | 存储侧（`provider_reference_version_must_advance`）；core 侧无版本判据（按 §11.6 由适配器判） |
| local-agent-config：引用失效即不可用 / 白名单是上限 / 引用失效时失败关闭 / 日志只记录变量名与数量 | **无实现、无用例**：`CredentialResolver` 只有端口与文档（`ports.rs:837-851`），`crates/core` 无实现，`proposal.md` 的 non_goals 明确「不实现 identity-keystore 与组合根接线」，`design.md` D2 把实现归组合根 → 判为**有意的范围外**，不是缺陷；本变更交付的是形状与失败关闭的错误取值（`KeystoreUnavailable` 已落地且在 `port_error_public` 有显式分支） |
| local-agent-config：使用不存在目录时明确失败 | `create_session_reports_local_resolution_failure_as_unavailable` ✔（且不修改 workspace 记录） |
| local-agent-config：远程目录不泄漏本机路径 | 本切片：`workspaces()` / `workspace()` 全 `require_local`（`756-769`），`agents()`（`227`）只返回 `AgentDescriptor`，catalog 投影不在本变更；无路径进入对端可见输出的代码路径 ✔（无用例断言） |

## 4. 结论

- **有 1 条阻断项（修在 core 写集侧或合同/注释侧，必须先决定）**：`WP23-2` —— 本变更登记的安全动作 `node.paired` 在实现里**没有任何写入方**（全仓库 `git grep "node.paired"` 只有枚举、DDL CHECK 与测试数据），而 `crates/core/src/use_cases.rs:489` 的注释声称「配对确认路径负责写 `node.paired`（`CORE_PORTS_AND_STORAGE.md` §11.6 第 2 条）」，该节正文并不含这条规则；core 的节点批准写集只携带 `pairing.approved`。唯一看起来覆盖它的存储用例是**用例自己**构造 `AuditAction::NodePaired` 写集的，因此现有门禁（`enum_coverage` 只看字面量）无法发现该缺口。无论最终判定「该由本路径写」还是「`pairing.approved` 已覆盖」，当前状态（注释断言与实现相反 + 登记动作无写入方）都不能原样交付，必须由作者决定并同步修代码或注释/合同。
- **WP2/WP3 其余部分与合同一致**：端口纯度（无 runtime/DB/wire/平台类型与平台 `cfg`）、16 个写集 DTO 的形状与「一次端口调用 = 一个写集」、`remove_import` 单次 `ImportRemoval`（无 `drop_import` 串联）、`port_error_public` 对 `ConflictKind` / `UnavailableKind` 的逐值穷举（无通配臂、无泄漏、既有取值零行为回归）、workspace 解析的校验/分类/路径不泄漏、以及 `verification.md` 四条登记偏差与代码的一致性，均逐条核对通过（§1）。
- **建议一并闭合（不阻断）**：`WP23-1` —— `create_session` 在授权之前做本机存储读与文件系统访问（与 `broker.rs:493` 的「所有用例入口的第一步」约定相悖，并让未授权调用方可区分「已登记别名 + 目录缺失」）。`WP23-3`（信息级）—— `use_cases.rs:738` 把「部分唯一索引」指到 `CORE_PORTS_AND_STORAGE.md` §11.7，而索引在 §7.3；`use_cases.rs:804` 的「§11.6」同类（§11.6 不含 `EntityRef::Provider`）。
- **检视限制**：本轮为静态只读检视，**未运行** `cargo fmt/clippy/test`，因此不声称任何编译或测试结论；凡引用 `reports/*.log` 的证据均标注「未独立复算」。`crates/core/src/model/**`（`3.2`）、`crates/storage-sqlite/**`（`3.6`）与 `docs/**`（`3.8`）不在本轮范围，本报告对其的引用仅用于核对跨界分发点与 §-引用的正文归属。
- 检视基线：`5404610`（固定工作树，对应 `crates/core/src/{ports,use_cases,broker}.rs` 的 `28f8cb9..013f2b9` diff）；本报告是本轮唯一写入的仓库文件。
- 报告落盘后复跑 `node scripts/check-doc-links.mjs`，本文件 0 命中、整门禁退出码 0（`doc links OK: 367 relative links, 2076 section refs across 101 markdown files`）。
