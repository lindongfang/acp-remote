# rv1-wp1.md — WP1（core 值对象与依赖边界）独立对抗性 review

## 0. 检视对象、范围与方法

- 仓库 / 分支：`D:\Project\acp-remote`，`feat/admin-state-persistence-v2`
- Review ID：`RV1-WP1`；Review Type：branch（切片实现后）；Review Stage：实现后、独立 review（任务 3.2）
- Base Revision：`28f8cb9`（变更起点）；Target Revision：`5404610`（`git rev-parse HEAD` = `5404610d35b22c9d427b88b0cea2c61d1e7a5e15`），切片内容由 `013f2b9`（W0 基线）与 `f43f7a7` 两次提交合入
- 检视切片：`crates/core/Cargo.toml`、`crates/core/src/model/{backend,config,error,identity,ids,mod,tests}.rs`、`scripts/check-crate-boundaries.mjs` 的 `CORE_ALLOWED_CLOSURE`
- 非目标（未检视）：`crates/core/src/{ports,use_cases,broker}.rs`（3.4 负责）、`crates/storage-sqlite/**`（W0/WP6 负责）、`docs/**` 的文本编辑正确性（3.6/3.8 负责）。但**代码注释所引用的合同小节是否真的含所声明的规则**属本轮判据（见 §1.6）
- 对照的权威：`CORE_PORTS_AND_STORAGE.md` §2/§3.1/§3.5/§3.6/§3.7/§5.1/§5.3/§7.2/§7.3/§9（判据 13、14、23–29）/§11.1–§11.9、`IDENTITY_AND_AUTH_CONTRACT.md` §2/§3、`AGENTS.md` §1/§3/§4/§9/§10/§12、`MODULE_ARCHITECTURE.md` §3.1/§5、`SECURITY_DESIGN.md` §13.1/§14.2、`LOCAL_ADMIN_PROTOCOL.md` §5.2 至 §5.5、`INITIAL_DESIGN.md` §16、`SYNC_PROTOCOL.md` §7.0、`NODE_LINK_PROTOCOL.md` §13.2、5 份 spec、`design.md`、`tasks.md`（2.3 至 2.7）、`verification.md`（Check Plan Changes 与 Review Findings）
- **版本稳定性说明**：检视期间工作区被主 Agent 改动（`git status --porcelain` 出现 6 个已跟踪文档的未提交改动，属文档切片）。本报告的**判据一律以 `5404610` 的 blob 为准**；关键结论用 `git show 5404610:<path>` 复核过（§3.5/§11.7 等），没有拿工作区副本当 5404610 的内容。这些文档改动不改写本轮结论。
- `git status --porcelain`（检视时刻）：

```text
 M README.md
 M docs/CONFIG_REFERENCE.md
 M docs/CORE_PORTS_AND_STORAGE.md
 M docs/DEVELOPMENT_PLAN.md
 M docs/MODULE_ARCHITECTURE.md
 M openspec/changes/admin-state-persistence-v2/verification.md
?? .omp/
?? .pi/prompts/opsx-verify.md
?? .pi/skills/openspec-verify-change/
?? reports/parked-idle-identity-retention.patch
?? reports/rv1-wp5.md
```

（`.omp/`、`.pi/**`、`reports/parked-idle-identity-retention.patch` 是宿主安装或与本变更无关的文件；`reports/rv1-wp5.md` 是另一位 reviewer 的报告。除本报告（`reports/rv1-wp1.md`，未跟踪）外，本轮**没有**写入任何仓库文件。）
- **后续版本漂移（定稿时观察）**：本报告定稿期间工作区又被改动，`git status --porcelain` 新增 `docs/IDENTITY_AND_AUTH_CONTRACT.md` 与 `scripts/check-crate-boundaries.mjs` 的未提交改动，并出现其他 reviewer 的报告 `reports/rv1-wp4.md`、`reports/rv1-wp5.md`、`reports/rv1-wp23.md`。本报告的判据与命令结果一律以 `5404610` 为准；§1.5 的 allow-list 结论针对的是该提交版本的 `scripts/check-crate-boundaries.mjs`，工作区的新改动不在本轮范围内。

- 只读命令与实际结果：
  - 只读 `git`（`rev-parse`、`log --oneline`、`status --porcelain`、`diff`、`diff --stat`、`show --stat`、`ls-tree`、`grep`）：全部正常返回（未单独记录数值退出码）
  - `node scripts/check-crate-boundaries.mjs` → **退出码 0**：`crate boundaries OK: 6 个 crate 的依赖方向与 §5 矩阵一致（6 个已在矩阵登记）`
  - `cargo tree -p core --edges normal --no-dedupe --locked` → **退出码 0**（读依赖闭包，非构建）
  - `cargo tree -p core -e normal -i hmac --locked`（同法查 `pem-rfc7468`、`signature`）→ **退出码 0**，用于反查新依赖的归属路径
  - `node scripts/check-doc-links.mjs` → **退出码 1**：13 个 error 全部落在 `reports/rv1-wp5.md`（另一位 reviewer 的报告，非本轮范围）。本报告写入后复跑，`reports/rv1-wp1.md` 命中的 error 数为 **0**（结论见 §5）
  - **未执行**：`cargo build` / `cargo test` / `cargo clippy` / `cargo fmt`（assignment 明确禁止，避免与并行轨道争用构建目录）。因此本报告**没有**独立复算编译与 core 测试；模型层用例证据来自他人：主 Agent 在 `5404610` 上产出的 `reports/verify-5404610.log`（`cargo test --locked --workspace --all-features` 退出码 0，其中 `acp_core` 单测 `test result: ok. 75 passed; 0 failed`，并逐条列名 `model::tests::peer_public_key_is_validated_at_construction_and_derives_its_fingerprint`、`model::tests::local_config_values_enforce_their_invariants`、`model::tests::audit_record_carries_no_content_and_closed_action_set`、`model::tests::port_error_and_kinds_are_wired_to_the_model_errors` 为 `ok`），以及实现者日志 `reports/wp1-core-tests.log`（`75 passed`；其 header 写 `commit: 28f8cb9`，而该提交只有 68 个 `#[test]`，75 与 `013f2b9`/`5404610` 的计数一致 —— 该日志实际取自 WP1 的未提交工作区，`verification.md` 的 Dependency Handoffs 记的也是「`28f8cb9` + 工作区」，记录无误，只是 header 易被误读为「基线即 75 用例」）。**以上两条日志均未由本轮复算。**

## 1. 逐条核对

### 1.1 `PeerPublicKey` 的值对象不变量 —— 成立

- **私有字段 + 单一构造路径**：`crates/core/src/model/identity.rs:609-610`（`pub struct PeerPublicKey([u8; 65]);`，字段无 `pub`）、`:620-628`（`try_from_bytes`）。另有 `:657-663` 的 `impl TryFrom<&[u8]>`，实现体只有一行 `Self::try_from_bytes(bytes)`，不是旁路。没有 `From<[u8; 65]>`、没有 `Default`、没有 `FromStr`（见发现 2），`as_bytes()`（`:630-633`）只是读取。
- **构造顺序（长度先于解析）**：`:621-624` 先判 `bytes.len() != 65 || bytes.first() != Some(&0x04)` 并早返回 `InvalidValue::PublicKey`，之后才调 `p256::PublicKey::from_sec1_bytes`。与 `CORE_PORTS_AND_STORAGE.md` §3.5 的 `PeerPublicKey` 行、§11.5 的「长度断言必须在解析之前」逐条一致。
- **指纹唯一入口**：`:638-645` = `SHA-256(65 字节原始公钥)` → 64 字符小写 hex，交给 `crates/core/src/model/ids.rs:295-302` 的 `Fingerprint::from_lower_hex`（`pub(crate)`）。全仓只有这一处生成指纹（`Fingerprint::from_lower_hex` 的调用点只有 `identity.rs:644`），下游存储层读库时也用同一入口派生而不是读指纹列（`crates/storage-sqlite/src/admin/trust.rs:225-231`，属 WP6 切片，仅作跨边界核对）。
- **假想回退与失败点**：把 `try_from_bytes` 改成「先 `from_sec1_bytes` 再长度断言」→ 33 字节压缩点被接受，`crates/core/src/model/tests.rs:2059-2062` 失败（构造 33 字节压缩点期望 `Err(InvalidValue::PublicKey)`）；把指纹输入从 65 字节原始公钥换成 base64url 文本 → `tests.rs:2038-2052` 的独立复算失败。
- **无用例覆盖**：无（该类型的其余形态——如 `Clone`/`Ord` 派生——不构成契约）。

### 1.2 `PairingPeer.public_key` 替换 `public_key_fingerprint`（含 `f43f7a7` 后的形状）—— 成立

- 字段替换：`crates/core/src/model/identity.rs:699-705` 现在是 `{ id: PeerIdentity, display_name: String, public_key: PeerPublicKey, host_binding: String, client_nonce: Nonce }`；`git diff 28f8cb9..5404610` 显示旧字段 `-public_key_fingerprint: Fingerprint` 已被 `+public_key: PeerPublicKey` 取代。`:738-740` 的 `public_key()` 是验签材料的唯一来源；`:747-750` 的 `public_key_fingerprint()` 只是 `public_key.fingerprint()` 的便捷方法，**不是独立字段**（因此「指纹与公钥不一致」在类型上不可表达）。
- 与合同逐字一致：`git show 5404610:docs/CORE_PORTS_AND_STORAGE.md` 第 142 行的 `PairingPeer` 行与第 138 行的 `PairingRecord` 行，字段集合/顺序/长度上限与代码相同；`fingerprint` 不再单独存放这一点两处都写了。
- `host_binding` 的引入与消费：`identity.rs:443`（`PairingRecord`）+ `:703`（`PairingPeer`）两个字段都在构造期 `require_bounded(host_binding, 1, 2048)`（`:476`、`:717`），空串 → `InvalidValue::Empty`；消费侧在任何写入之前做「逐字相等」比对（`crates/storage-sqlite/src/admin/trust.rs:606-608`，属 WP6 切片）。语义与 `CORE_PORTS_AND_STORAGE.md` §7.3 的列注释（`crates/storage-sqlite/src/migrate.rs:973`：设备为 canonical origin、节点为 owner endpoint）以及 `NODE_LINK_PROTOCOL.md` §13.2 的 claim `endpoint`（Owner 的 WSS 端点）同源，**不存在「拿对端 endpoint 比本机 endpoint 而必然不等」的语义错位**。
- **假想回退与失败点**：删掉 `host_binding` 的 `require_bounded` → `tests.rs:1569-1587`（`PairingRecord` 空绑定）与 `tests.rs:1631-1642`（`PairingPeer` 空绑定）失败；把 `PairingPeer` 改回存指纹字段 → `tests.rs:1592-1608` 编译失败（`public_key()` 不存在）。

### 1.3 `SecretValue` 与本地配置值对象 —— 成立

- `SecretValue`：`crates/core/src/model/config.rs:401` 是单字段私有元组结构体，**未**实现 `Debug`/`Serialize`/`Display`/`Clone`，只有 `new`/`expose_secret`/`len`/`is_empty`（`:403-423`）。与 `CORE_PORTS_AND_STORAGE.md` §3.7 的 `SecretValue` 行、§11.6 的凭据注入边界一致：类型上没有可打印路径，就不会「进日志/进错误消息」。
- `ProviderEnvBinding`：`config.rs:45-79`，保留名（`ACP_REMOTE_` 前缀、大小写不敏感，`:27-29`）、环境变量名模式（`:17-26`）、provider id 模式（`:32-34`）都在构造期执行；「`name ∈ env_allowlist`」只能在 profile 层判定，落在 `config.rs:137-139`。
- `AgentProfile`：`config.rs:114-145` 依次校验显示名 1..=128、命令 1..=1024、命令无 NUL、参数逐个 ≤4096 且无 NUL、白名单项合法非保留且不重复（`:126-134`）、每个绑定的 `name` 必须在白名单内（`:137-139`）、`(provider_id, field)` 不重复（`:140-144`）。逐条对应 `CORE_PORTS_AND_STORAGE.md` §3.7 的 `AgentProfile`/`ProviderEnvBinding` 不变式。「至多一个 `default = true`」不在模型层：`CORE_PORTS_AND_STORAGE.md` §7.3 的部分唯一索引 + `LocalConfigStore::put_profile` 承担（WP6）。
- `WorkspaceRecord`/`ResolvedWorkspace`：`config.rs:218-229` 与 `:436-441` 要求 `canonical_path` 非空、≤4096、无 NUL、`Path::is_absolute()`（相对路径被拒）。`canonicalize`、目录存在性与 `..` 拒绝属 `CORE_PORTS_AND_STORAGE.md` §5.1 的解析路径（core 不读文件系统），模型只做形状校验，是**合同内的分工**。
- `ProviderRef`/`ProviderRefKind`/`SeedState`：`config.rs:275-360`（id 模式、显示名 1..=128、字段名合法且不重复、`keystore_ref` 1..=256、`version ≥ 1`）与 `:365-395`（`seeded_at` 非空 ⟺ `seeded`）。
- `CreateSessionRequest.workspace`：`crates/core/src/model/backend.rs:160` 已是 `Option<ResolvedWorkspace>`，`new` 的形参同步（`:168-172`），`WorkspaceAlias` 的 `use` 已从该文件移除；core 内无遗留的 `workspace_alias` 字段引用（只剩 `crates/core/src/use_cases.rs:138` 的形参，那是 `CORE_PORTS_AND_STORAGE.md` §5.1 的设计）。
- **无用例覆盖**：`SecretValue` 的「不实现 Debug/Serialize」没有断言（见 §2 末尾与发现 5）；`AgentProfile` 的若干负例见发现 5。

### 1.4 枚举新取值与 `ALL`/`as_str` 一致性 —— 成立

- `ConflictKind` 新增 `AlreadyExists`/`IdentityMismatch`/`DuplicateOwnership`（`crates/core/src/model/error.rs:28-33`）；`ALL` 由 `[Self; 6]` 改为 `[Self; 9]` 并补齐三项（`:38-48`）；`as_str` 三条新标记（`:59-61`）。`UnavailableKind::KeystoreUnavailable`（`:88-89`）、`ALL` `[Self; 7]`（`:94-102`）、`as_str`（`:113`）。`InvalidValue::PublicKey`（`:193-194`）与稳定消息（`:238`）。
- `AuditAction` 新增 5 个（`crates/core/src/model/identity.rs:875-879`）。`ALL`、`as_str`、`FromStr` 都由 `token_enum!` 从同一份变体列表机械生成（`crates/core/src/model/mod.rs:93-118`），因此不存在「加了变体忘加 `ALL`」这一类空隙；20 个取值与 `SECURITY_DESIGN.md` §14.2 的封闭清单逐条同名。
- 跨边界（消费侧）核对：`crates/core/src/broker.rs:2503-2540` 的 `port_error_public` 对 `ConflictKind` 与 `UnavailableKind` 逐值显式分支、没有通配臂，新取值不会被静默吞成 `internal.unavailable`；DDL 侧 `crates/storage-sqlite/src/migrate.rs:162-166`、`:405-409`、`:447-451`、`:487-491` 已含 5 个新 token，`crates/storage-sqlite/tests/enum_coverage.rs:177-206` 用 `AuditAction::ALL` 与两张审计表的 CHECK 做**集合相等**断言（不是子集包含），因此「枚举多一个/DDL 少一个」两种方向都会红。
- `CORE_PORTS_AND_STORAGE.md` §3.5 的 `AuditAction` 行在 `5404610` 上已含这 5 个取值且去掉了 `[待实现]`（`git show 5404610:docs/CORE_PORTS_AND_STORAGE.md` 第 147 行）。
- **假想回退与失败点**：删掉任一 `as_str` 分支 → 编译失败（穷举 match）；从 `AuditAction` 删一个取值而保留 DDL → `crates/storage-sqlite/tests/enum_coverage.rs` 的集合相等断言失败。**无用例覆盖**：`ConflictKind::ALL` 少写一项而枚举不变（长度断言仍为 9）不会被任何断言发现（见 §2）。

### 1.5 core 依赖边界（`Cargo.toml` / allow-list / 合同 / AGENTS）—— 成立，但 feature 面偏宽（发现 1）

- `crates/core/Cargo.toml:19-25` 新增 `p256.workspace = true` 与 `sha2.workspace = true`（并写了同步 §9 判据 13 / allow-list / `AGENTS.md` §12 的义务）。必要性成立：`PeerPublicKey` 的点校验与 SHA-256 指纹需要这两者，core 不得自行实现曲线运算。
- **allow-list 与实测闭包集合相等**：本轮实跑 `node scripts/check-crate-boundaries.mjs` 退出码 0；脚本对 `crateNormalClosure()` 与 `CORE_ALLOWED_CLOSURE`（`scripts/check-crate-boundaries.mjs:172-220`）做双向比较，任一侧多出都会红。
- **四处定义一致**：`CORE_PORTS_AND_STORAGE.md` §9 判据 13、`AGENTS.md` §12 第一条、`MODULE_ARCHITECTURE.md` §3.1 的 `[workspace.dependencies]` 条目都写「core 的直接依赖固定为 `async-trait`/`thiserror`/`p256`/`sha2`」，与 `CORE_ALLOWED_CLOSURE` 的前四项一致（本轮逐文件读取核对，非复算文档生成）；`CORE_FORBIDDEN`（`scripts/check-crate-boundaries.mjs:76-97`）未被削弱。
- 偏宽处（`p256` 继承 workspace 的 `features = ["ecdsa"]`）见发现 1。

### 1.6 注释与文档里的 §-引用逐条核对（要求 1 与要求 4）—— 1 处失配（发现 3）

| 代码位置 | 注释引用 | 复核结果（以 `5404610` 的 blob 为准） |
|---|---|---|
| `crates/core/src/model/identity.rs:1-12` | `CORE_PORTS_AND_STORAGE.md` §3.5，以及 `LOCAL_ADMIN_PROTOCOL.md` §5.3/§5.4/§5.5、`SECURITY_DESIGN.md` §10.2/§13.1/§14.2 | 成立：各小节主题与引用内容相符 |
| `identity.rs:431-433` | `CORE_PORTS_AND_STORAGE.md` §7.3 的 `owned_pairing.host_binding`、§11.2 第 1 条 | 成立：§7.3 有该列及「设备 canonical origin / 节点 owner endpoint」注释；§11.2 第 1 条正文明确写出「两侧 `host_binding` 逐字相等，不一致 → `Conflict(IdentityMismatch)`」 |
| `identity.rs:605/607/618-619/637` | `CORE_PORTS_AND_STORAGE.md` §11.5、`SECURITY_DESIGN.md` §13.1、`INITIAL_DESIGN.md` §16 第 6 条 | 成立：§11.5 保留「构造顺序」「长度断言先于解析」「`fingerprint()` 唯一入口」「私钥不进 core」；`INITIAL_DESIGN.md` §16 第 6 条正是 P-256 互操作并含 33 字节压缩点的实测约束 |
| `identity.rs:404` | `SYNC_PROTOCOL.md` §7.0 | 成立：§7.0 是配对状态机 |
| `identity.rs:117/133/176/199/268/302/396/422` | 本合同 §6 第 6 条（未解析引用）、§7.3 的 `actor_id`、`LOCAL_ADMIN_PROTOCOL.md` §5.3/§5.4 等 | 成立 |
| `crates/core/src/model/config.rs:1`、`:26`、`:40`、`:84`、`:205`、`:266`、`:273`、`:363` | `CORE_PORTS_AND_STORAGE.md` §11.6/§11.9 | 可接受：§11.6 的正文（提交模型、写集语义、种子语义、凭据注入边界）与 §11.9（解析归 core 的理由）确实覆盖这些规则；但**形状**的权威处已是 §3.7/§3.6（§11.6 自己写了「值对象…在 §3.7」）。属措辞可优化（信息级） |
| `config.rs:102`、`:288` | 「与 `CORE_PORTS_AND_STORAGE.md` §11.7 的 `owned_agent_profile`／`owned_provider_ref` 列一一对应」 | **失配（发现 3）**：`5404610` 的 §11.7 正文已改为「九张 `owned_*` 管理表…与两个索引在 §7.3」，列清单不在 §11.7 |
| `config.rs:352` | `CORE_PORTS_AND_STORAGE.md` §11.2 第 7 条 | 成立：该条要求换绑递增版本、新引用提交后才清理旧条目 |
| `crates/core/src/model/error.rs:13`、`:28`、`:30`、`:32`、`:87`、`:124`、`:211`、`:213` | `CORE_PORTS_AND_STORAGE.md` §2、§11.6、§3.4、§3.3、`MODULE_ARCHITECTURE.md` §8 | 成立：§11.6 末段的 `[已裁定]` 段落逐名列了三个 `ConflictKind` 新取值与 `KeystoreUnavailable`，并写明 `port_error_public` 通配臂的陷阱；`local.conflict`/`local.unavailable` 确实存在于 `schemas/local-admin/v1/envelope.schema.json` |
| `crates/core/src/model/ids.rs:300`、`:503` | `CORE_PORTS_AND_STORAGE.md` §11.5 的单一指纹入口、§11.6 的 Provider 审计目标 | 成立 |
| `crates/core/src/model/backend.rs:150`、`:154-156` | `CORE_PORTS_AND_STORAGE.md` §11.9、§5.1、§3.6、§3.1 | 成立：§11.9 保留「解析归 core、后端只收已解析路径」的理由（规则正文在 §5.1） |
| `crates/core/src/model/tests.rs:2/73/194/366/695/1132/1335/1719/1932/1995` | 与合同同名小节（§3.1/§3.2/§3.3/§3.4/§3.5/§3.6/§2） | 成立：分节注释与合同小节一一对应 |
| `crates/core/Cargo.toml:19-23` | `CORE_PORTS_AND_STORAGE.md` §11.5、§9 判据 13、`AGENTS.md` §12 | 成立（但 §11.5 现在是「并入 §3.5」的设计理由节；对「`PeerPublicKey` 的构造校验理由」这一用途仍成立） |

- **要求 2（登记偏差核对）**：`crates/core/src/**` 没有「记录在案」这类措辞；本变更已登记的 WP1 相关偏差在代码里都能对上——`verification.md` 的 Check Plan Changes 第 1 条（`EntityRef::Provider(String)`）对应 `crates/core/src/model/ids.rs:500-504` 与 `:520`/`:539`；第 2 条（`create_session` 增加 `workspace_alias` 形参）对应 `crates/core/src/use_cases.rs:136-139`；第 5 条（`audit_action` 保留 `#[allow(dead_code)]`）对应 `crates/core/src/use_cases.rs:954`。**本轮新发现的 3 处偏差（发现 1/2/4）在 `verification.md` 里查不到。**

## 2. 回归不变量：枚举穷举与长度断言能不能真的拦住回退（判据 3）

| 断言 | 位置 | 能拦住的假想回退 | 失败点 / 缺口 |
|---|---|---|---|
| `AuditAction::ALL.len() == 20` | `crates/core/src/model/tests.rs:1677` | 增删 `AuditAction` 变体却不动测试 → 该断言先红 | 失败点是长度；取值级一致性由 `crates/storage-sqlite/tests/enum_coverage.rs:177-206` 的集合相等断言承担（更强，且 DDL 侧受漂移门禁约束） |
| `ConflictKind::ALL.len() == 9`、`UnavailableKind::ALL.len() == 7` | `tests.rs:2016-2017` | 删除 `ALL` 里的某一项而枚举不变 → 长度变化即红 | **缺口**：`ALL` 是手写数组（`crates/core/src/model/error.rs:38`、`:94`），**新增**一个变体却漏加进 `ALL` 时长度仍是 9/7，`as_str` 因穷举 match 仍编译通过 —— 没有任何断言能发现。当前无生产消费方遍历这两个 `ALL`，因此不是阻断项，但它使「长度断言 = 完整性证明」的说法不成立 |
| `PeerPublicKey` 四个负例（33 字节压缩点、64 字节、前缀 `0x05`、off-curve）+ 独立 SHA-256 复算 | `tests.rs:2032-2081` | 交换长度断言与曲线解析的顺序、指纹改用别的编码、放宽长度 | 失败点明确：`Err(InvalidValue::PublicKey)` 与指纹相等；off-curve 的构造（`bytes[64] ^= 0x01`）对固定基点 G 确定性成立 |
| 空绑定 → `InvalidValue::Empty`（`PairingRecord`、`PairingPeer`） | `tests.rs:1569-1587`、`:1631-1642` | 删掉 `host_binding` 的构造校验、或把下限改成 0 | 失败点明确 |
| `peer.public_key_fingerprint() == peer.public_key().fingerprint()` | `tests.rs:1604-1608` | —— | **在当前实现下是同义反复**：`public_key_fingerprint()` 的实现就是 `self.public_key.fingerprint()`（`identity.rs:748-750`），两侧是同一字节上的同一次计算，永不失败。真正拦住「回退成独立指纹字段」的是**编译器**（`public_key()` 不存在 → 编译失败）。这条不计为发现，但它不是「拦住回退」的断言 |
| `local_config_values_enforce_their_invariants` | `tests.rs:2083-2228` | 白名单外绑定、保留名、白名单重复项、相对路径、`version == 0`、`SeedState` 不自洽、`ResolvedWorkspace` 相对路径 | 失败点明确；未覆盖的分支见发现 5 |
| `entity_ref_exposes_kind_and_target_id_for_storage_columns` | `tests.rs:169-192` | `kind()`/`target_id()` 的 `Session`/`Command`/`Export` 分支退化成常量 | **缺口**：新增的 `Provider` 分支（`ids.rs:520`、`:539`）没有断言（见发现 4） |
| `SecretValue` 的「不实现 `Debug`/`Serialize`/`Display`/`Clone`」 | 无 | 给 `SecretValue` 加 `#[derive(Debug)]` 或 `impl Display`（`config.rs:401`） | **无用例覆盖**：类型上没有可断言的公开面，回退只能靠评审发现。合同（`CORE_PORTS_AND_STORAGE.md` §3.7）允许不测，但应知道这条不变量的防线是评审而非测试 |

## 3. 规格场景覆盖（core 模型部分）

| 规格 / 场景 | 对应用例 或 范围判定 |
|---|---|
| `specs/peer-identity-material/spec.md`：接受合法未压缩公钥 | `tests.rs:2032-2052`（合法点 + 由测试侧独立重算 SHA-256 比对指纹） |
| 同上：拒绝压缩点与非法长度 | `tests.rs:2059-2080`（33 字节压缩点、非 65 字节、前缀错、非曲线点） |
| 同上：认领后重启仍能取到验签公钥 / 双角色共享一条材料 / 按节点撤销覆盖两种角色 / 已绑定换钥被拒 | **WP1 范围外**（模型只提供类型与不变量）；落库面在 `crates/storage-sqlite/tests/admin_store.rs`，已在 `reports/rv1-wp6.md` 与 `reports/rv1-wp6b.md` 的覆盖表里逐条对应，本轮未复跑 |
| 同上：已撤销身份不能经普通写入复活 | 同上（模型层无可表达的「复活」路径；写入语义在存储层） |
| 同上：指纹与公钥不一致无法落库 | 模型层由「指纹不是独立字段」保证（`identity.rs:747-750`）；**无用例覆盖**：core 侧无法构造「带外来指纹的 `PairingPeer`」来断言拒绝（字段已不存在，编译期即不可表达），这条在 core 只能算范围判定 |
| 同上：库内不出现秘密材料 | **WP1 范围外**（落库面）；模型侧的对应保证是 `SecretValue` 无任何可打印/可序列化路径、且身份与配对记录里不出现秘密字段（本轮逐类型核对） |
| `specs/local-agent-config/spec.md`：非法绑定在写入时被拒 | 模型层 `tests.rs:2089-2152` 覆盖「白名单外绑定」「保留名（白名单与绑定两处）」「白名单重复项」；**无用例覆盖**：`(provider_id, field)` 重复绑定、命令/参数长度与 NUL、`configured_fields` 重复与非法字符、`is_env_name` 超长、`ProviderRef.id` 超长（发现 5）。Provider 字段存在性属存储层（WP6） |
| 同上：默认 profile 唯一且切换原子 | **WP1 范围外**（`LocalConfigStore::put_profile` + `CORE_PORTS_AND_STORAGE.md` §7.3 的部分唯一索引）；模型层只提供 `AgentProfile.is_default` |
| 同上：空种子也标记已初始化 / 重复打开不重导种子 | **WP1 范围外**（`SeedState` 只是值对象：`config.rs:365-395`、`tests.rs:2203-2209` 覆盖自洽性；标记语义在 `LocalConfigStore`） |
| 同上：引用失效即不可用 / 白名单是上限 / 解析失败关闭 / 日志只记变量名与数量 | **WP1 范围外**（`CredentialResolver` 尚未实现）；模型层的对应物是 `ProviderEnvBinding.name ∈ env_allowlist`（`config.rs:137-139`，`tests.rs:2110-2123` 覆盖）与 `SecretValue` 的可打印面为零 |
| 同上：引用版本推进 | 模型层 `ProviderRef.version ≥ 1`（`config.rs:303-305`，`tests.rs:2190-2201` 覆盖 `version = 0` 被拒）；换绑递增属存储层 |
| 同上：使用不存在目录时明确失败 | **WP1 范围外**（`CORE_PORTS_AND_STORAGE.md` §5.1 的解析路径，在 `use_cases`） |
| 同上：远程目录不泄漏本机路径 | **WP1 范围外**（catalog/事件面）；模型层的对应物是 `ResolvedWorkspace` 只作为 `CreateSessionRequest` 的入参存在（`backend.rs:160`） |
| `specs/workspace-resolution/spec.md`：别名 → 规范化路径的解析归属与后端输入 | 模型层：`CreateSessionRequest.workspace: Option<ResolvedWorkspace>`（`backend.rs:158-172`）；行为在 `use_cases`（3.4） |
| 同上：路径校验与规范化（绝对路径、`..`、`canonicalize`） | 模型层只做「绝对路径形状」（`config.rs:218-229`、`:436-441`，相对路径由 `tests.rs:2167-2176`、`:2221-2227` 覆盖）；`..` 与 `canonicalize` 属 `CORE_PORTS_AND_STORAGE.md` §5.1 的解析路径（core 不读文件系统） |
| 同上：解析失败的分类 / 路径不泄漏 | **WP1 范围外**（`use_cases.rs` 的 `create_session`） |
| `specs/admin-state-persistence/spec.md`：未登记动作无法写入 | 模型层：`AuditAction` 是闭合枚举，没有构造未登记取值的入口；DDL 侧的集合相等断言在 `crates/storage-sqlite/tests/enum_coverage.rs` |
| 同上：审计不含内容 / 失败请求的审计不含内容 | **WP1 范围外**（审计写入路径） |
| `specs/storage-schema-v2-migration/spec.md`：全部场景 | **WP1 范围外**（W0 的 `migrate.rs`、夹具与 `tests/migration.rs`）；本轮只核对衔接点：`AuditAction::ALL` ↔ 两张审计表 `action` CHECK 的集合相等（§1.4） |
| `CORE_PORTS_AND_STORAGE.md` §9 判据 13（端口纯度） | 本轮实跑 `node scripts/check-crate-boundaries.mjs` = 0；`cargo public-api -p core` 快照那半条**未执行**（需 nightly + 外部工具；按该判据应记为未执行并说明替代判据，不得声称已通过） |
| `CORE_PORTS_AND_STORAGE.md` §9 判据 14（审计取值闭合） | 模型侧 20 个取值与 `SECURITY_DESIGN.md` §14.2 对齐（§1.4）；「每个取值都要有写入用例」属存储侧 |
| `CORE_PORTS_AND_STORAGE.md` §9 判据 23 至 29（管理状态的原子性/重启/容量/失败关闭） | **WP1 范围外**（WP6 与 W0） |

## 4. 未解决发现

| ID | 级别 | 位置 | 问题与证据 | 假想回退 / 失败点 | 建议 |
|---|---|---|---|---|---|
| WP1-1 | 建议（中） | `crates/core/Cargo.toml:24` | `p256.workspace = true` 继承了根 `Cargo.toml:39` 的 `p256 = { version = "0.13", features = ["ecdsa"] }`（该行是变更前既有声明，本变更未改），于是 `p256` 的整支签名/编码栈进入 core 的**冻结普通闭包**：本轮 `cargo tree -p core -e normal -i hmac --locked` 只有一条来源路径 `hmac ← rfc6979 ← ecdsa ← p256 ← core`（`signature` 同源；`pem-rfc7468`/`base64ct`/`pkcs8`/`spki` 也经由 `ecdsa`/`pkcs8` 进入），而 `crates/core/src/**` 里没有任何 `ecdsa`/`VerifyingKey`/`Signature` 用法，core 也没有签名端口。这与同一变更写入的 `AGENTS.md` §12 与 `MODULE_ARCHITECTURE.md` §3.1 的「`hmac`/`base64` 属协议与身份边界，**不在 core**」直接矛盾，也与 `tasks.md` 2.3 的「按需最小 feature」不符；`verification.md` 的 Check Plan Changes 里查不到这项偏差。 | 只改文档措辞（把「不在 core」删掉）→ `CORE_ALLOWED_CLOSURE` 与文字仍不一致，后续审查会继续按「hmac 不在 core」推理；只改 feature → `node scripts/check-crate-boundaries.mjs` 会因闭包变小而报「冻结 allow-list 里的 X 已不在依赖闭包中」，必须同批更新 allow-list 与 `CORE_PORTS_AND_STORAGE.md` §9 判据 13。**本轮无法证明最小 feature 集足够**（禁止 `cargo build`/`cargo test`），因此这条建议需在放开构建的轨道上复算。 | 二选一：① 把根 `Cargo.toml:39` 收敛为 `p256 = { version = "0.13", default-features = false, features = ["arithmetic"] }`（core 是当前唯一消费者；将来 `identity-auth` 需要签名时用自己的 `features = ["ecdsa"]` 追加），然后重跑 `node scripts/check-crate-boundaries.mjs` 并按实测结果同步 `CORE_ALLOWED_CLOSURE`、`CORE_PORTS_AND_STORAGE.md` §9 判据 13、`AGENTS.md` §12、`MODULE_ARCHITECTURE.md` §3.1；② 保留现状但在 `verification.md` 的 Check Plan Changes 登记「接受整支 ecdsa/签名栈进入 core 闭包」及理由，并修正那两句文档措辞。 |
| WP1-2 | 建议（中） | `crates/core/src/model/identity.rs:657-663` | `PeerPublicKey` 的入口面与文档不符：`CORE_PORTS_AND_STORAGE.md` §3.5 的 `PeerPublicKey` 行写「私有字段 + `try_from_bytes`/`FromStr`」，`IDENTITY_AND_AUTH_CONTRACT.md` §3 与 `design.md:51` 同样写「`try_from_bytes`/`FromStr` 任一步失败即 `InvalidValue`」，但代码**没有** `FromStr`（全仓无 `impl FromStr for PeerPublicKey`），实际给的是 `TryFrom<&[u8]>`。core 也无法实现一个文本入口：`AGENTS.md` §12 不允许把 `base64` 放进 core，而 `hex_to_bytes` 是 `#[cfg(test)]`（`identity.rs:682-693`）。 | 若按文档补 `FromStr` → core 必须新增 `base64` 依赖，立即撞 `scripts/check-crate-boundaries.mjs` 的 `CORE_ALLOWED_CLOSURE` 与 `AGENTS.md` §12（门禁红）；若不补 → 按 `IDENTITY_AND_AUTH_CONTRACT.md` §3 实现的 `identity-auth` 找不到该入口，只能自行解 base64url 后调 `try_from_bytes`，与「统一在 core 校验」的口径产生解释空间。 | 首选改文档：把三处的 `try_from_bytes`/`FromStr` 改为「`try_from_bytes(&[u8])`（另有 `TryFrom<&[u8]>`）；文本形式由协议/身份边界解 base64url 后再进入本类型」。若确实需要 core 侧文本入口，先按 `AGENTS.md` §7 审批 `base64` 并同批更新 allow-list。落笔属文档切片（3.6/3.8）。 |
| WP1-3 | 信息 | `crates/core/src/model/config.rs:102`、`:288` | 两处 `#[allow(clippy::too_many_arguments)]` 的理由注释写「与 `CORE_PORTS_AND_STORAGE.md` §11.7 的 `owned_agent_profile`／`owned_provider_ref` 列一一对应」，但 `5404610` 的 §11.7 正文已改为「九张 `owned_*` 管理表…与两个索引在 §7.3」——列清单不在 §11.7，引用的小节不再含所声明的规则。 | 把注释改指 `CORE_PORTS_AND_STORAGE.md` §7.3 → 无编译/测试影响；不改则没有任何门禁会红（漂移门禁只看 §5/§7），因此这类失配只能靠评审发现，属本轮判据要求报出的项。 | 两处注释改为指向该合同的 §7.3（列定义的真正位置）。 |
| WP1-4 | 信息 | `crates/core/src/model/ids.rs:520`、`:539`（用例缺口在 `crates/core/src/model/tests.rs:169-192`） | 新增的 `EntityRef::Provider(String)` 的落库映射（`kind()` → `target_kind` = `"provider"`、`target_id()` → 引用 id）**无用例覆盖**：core 的 `entity_ref_exposes_kind_and_target_id_for_storage_columns` 只断言 `Session`/`Command`/`Export`；存储侧 `crates/storage-sqlite/tests/admin_store.rs:533-539` 的 `audit_rows` 只按 `action` 计数，`crates/storage-sqlite/src/migrate.rs:172` 的 `owned_audit.target_kind` 也没有 CHECK，写错不会被库拒绝。 | 把 `ids.rs:520` 的返回值改成 `"provider_ref"`（或 `:539` 返回空串）→ 本轮所有已跑用例仍绿（`audit_rows` 只数动作），只有人工读代码能发现；存储层也没有断言会失败。 | 在 `entity_ref_exposes_kind_and_target_id_for_storage_columns` 末尾补一条 `EntityRef::Provider("openai".to_owned())` 的 `kind()`/`target_id()`/`to_string()` 断言（core 侧即可，无需改存储）。 |
| WP1-5 | 信息 | `crates/core/src/model/config.rs:119-124`、`:140-144`、`:306-315` | 新增校验的若干分支无用例覆盖：`(provider_id, field)` 重复绑定（`CORE_PORTS_AND_STORAGE.md` §3.7 明写的不变式）、`command` 的超长与 NUL、`arg` 的 NUL、`configured_fields` 的重复项与非法字符、`ProviderRef.id` 超过 64 字符、`is_env_name` 超过 128 字符。现有用例只覆盖了白名单外绑定、保留名、白名单重复项、相对路径、`version = 0` 与 `SeedState`/`ResolvedWorkspace` 的形状（`tests.rs:2083-2228`）。 | 删掉 `config.rs:141-143` 的 `seen_bindings` 去重判断 → `local_config_values_enforce_their_invariants` 仍全绿；删掉 `:116-118` 的 `no_nul(command)` → 仍全绿。两条都是「实现回退而测试无声」。 | 在 `local_config_values_enforce_their_invariants` 里按 `CORE_PORTS_AND_STORAGE.md` §3.7 的不变式补这几条负例（每条一行 `assert!(... .is_err())`）。 |

**阻断项：无。**

## 5. 结论

- **结论：`PASS`**（针对 Target Revision `5404610` 的 WP1 切片）。值对象不变量（`PeerPublicKey` 的构造顺序与唯一指纹入口、`PairingPeer.public_key` 与 `host_binding`、`SecretValue` 无可打印路径、`AgentProfile`/`ProviderEnvBinding`/`WorkspaceRecord`/`ProviderRef`/`SeedState`/`ResolvedWorkspace` 的构造校验、`CreateSessionRequest.workspace`）、枚举新取值与 `ALL`/`as_str` 一致性、core 依赖边界的门禁一致性与 §-引用的真实性都已逐条核对；**没有**发现阻断交付的缺陷。
- **未独立复算的部分（不得当作已验证）**：编译、core 单测、clippy/fmt 全部未由本轮执行（assignment 禁止 `cargo`）；§0 列出的两条日志是本轮的唯一运行证据，其中 `reports/verify-5404610.log` 是主 Agent 在 `5404610` 上产出的完整工作区测试（退出码 0，`acp_core` 75 passed）。`cargo public-api` 那半条判据未执行。
- **建议在本轮闭合（均非阻断，2 条属文档切片）**：WP1-1（`p256` feature 面与三处文档措辞、`tasks.md` 2.3 的「按需最小 feature」之间的冲突，需在放开构建的环境复算最小集）、WP1-2（`PeerPublicKey` 的 `FromStr` 入口在合同与代码之间不一致）。
- **信息级待办**：WP1-3（两处 §11.7 引用应指 §7.3）、WP1-4（`EntityRef::Provider` 的列映射无用例）、WP1-5（若干构造校验分支无用例、`SecretValue` 的不实现 `Debug`/`Serialize` 只能靠评审守住）。
- **撰写约束自查**：本报告的每一处章节引用都把文档名紧邻在被引小节之前（例如 `CORE_PORTS_AND_STORAGE.md` §7.3、`AGENTS.md` §12），避免 `node scripts/check-doc-links.mjs` 把引用归因到同子句里另一份文档；写入后复跑该脚本：退出码 0（`doc links OK: 367 relative links, 2113 section refs across 101 markdown files`）；写入前那一次运行时的 13 个 error 全部落在 `reports/rv1-wp5.md`（已由该报告作者修正），本报告没有贡献任何 error。
- 本报告是唯一写入：未修改代码、测试、文档、plan/tasks/verification 或他人的报告；未执行任何 `cargo` 构建、写分支或提交。
