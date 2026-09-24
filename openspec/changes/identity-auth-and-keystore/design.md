## Context

- 切片 3 的两个 crate 都不存在：`crates/` 只有 `acpr-transcript`、`acpr-wire`、`core`、`storage-sqlite`、`sync-protocol`、`node-link-protocol`、`acp-protocol`、`agent-host`（`README.md` 的「仓库当前状态」、`docs/DEVELOPMENT_PLAN.md` §2）。`docs/IDENTITY_AND_AUTH_CONTRACT.md` 自称「编码前合同」，其中 §4.1/§5.1/§7 的入口与端口形状需要在本变更内定型并写回。
- 可复用的既有设施（本设计只消费，不重建）：`core` 的 `Actor`/`DeviceRecord`/`NodeRecord`/`PairingRecord`/`PairingPeer`/`PairingClaim`/`PairingSettlement`/`PeerPublicKey`/`ScopeSet`/`GrantSet`/`Nonce`/`Digest`/`Timestamp`（`crates/core/src/model/identity.rs`、`scalars.rs`）；`core::ports` 的 `TrustStore`/`AuditStore`/`Clock`（`crates/core/src/ports.rs`）与 §11.6 的写集（`PairingWrite`/`PairingClaimWrite`/`PairingSettlementWrite`/`ExpiryWrite`/`DeviceWrite`/`NodeWrite`）；`acpr-transcript` 的表驱动编解码与 `base64url`/`u16be`/`u64be`/`nul-joined` 辅助；`sync-protocol::domains::DOMAINS` 与 `node-link-protocol::domains::DOMAINS` 各 6 个 domain。
- 固定向量已在仓库内：`fixtures/sync/v1/transcripts/*.json` 与 `fixtures/node-link/v1/transcripts/*.json` 各有 6 个正向向量（含 `transcriptBase64url`/`transcriptSha256Hex`，HMAC 域含 `hmacKeyBase64url`/`hmacSha256`，SAS 域含 `sas`，签名域含 `publicKey`/`p1363Signature`/`privateJwk`）与 `invalid/` 负例（含公钥长度、非曲线点、非法 base64url、tag 乱序/重复、未知 tag、坏 magic/codecVersion）；`compatibility/transcripts/v1/transcripts.json` 是这些 transcript 的唯一机器登记。`docs/SECURITY_DESIGN.md` §18.1 要求把它们固化为常驻 Rust 测试。
- `compatibility/commands/v1/commands.json` 已含 `packs`/`presets`/`grants` 三张表（12 个命令、4 个 pack、2 个 preset、5 个 grant）与 `localCapabilities` 7 项；`scripts/check-command-catalog.mjs` 已校验 packs/grants 与命令集合的一致性，但没有任何 Rust 消费方做「展开」。
- 已冻结、本设计必须遵守而不再论证的约束：`MODULE_ARCHITECTURE.md` §3.1/§4.8/§4.12/§5（依赖矩阵与平台隔离）、`IDENTITY_AND_AUTH_CONTRACT.md` §2–§8（状态机边界、密码学硬约束、失败关闭清单）、`SECURITY_DESIGN.md` §9.2/§13.1/§14.1/§20（密钥档位、数据保护、日志、待收口选择）、`AGENTS.md` §7/§12（`unsafe_code = \"forbid\"`、MSRV 1.85、依赖审查）。

## Goals / Non-Goals

**Goals:**

- 冻结两个 crate 的公共形状（入口、端口、错误分类、写集/事实返回值），使切片 4–7 只需映射 wire 与装配，不必改身份边界。
- 把 `docs/IDENTITY_AND_AUTH_CONTRACT.md` §4.1/§5.1/§7 从「目标形状」定型为「已实现形状」（含熵源端口、持久事实快照入参与 keystore 端口最终签名），并让两个 crate 的测试成为 §8「每个失败路径都必须有测试」的可执行证据。
- 用 fixtures 驱动的常驻回归测试取代一次性探针（transcript 重算、HMAC、P1363 验签、SAS、畸形输入）。
- 让平台差异只存在于 `identity-keystore` 的 `cfg` 子模块，`identity-auth` 的其余部分在所有平台编译与单测。

**Non-Goals:**

- 不复述 proposal 的范围外事项（切片 4–7、macOS/Linux keystore 后端、前端、wire 变更）。
- 不在本变更引入 daemon 生命周期、配置装载、本地 IPC 与 CLI（切片 4 的 `app`/`server::local_admin`），也不接入 `TrustStore` 的读路径（组合根在切片 4/5 装配）。
- 不设计 Node Link/Sync 的 wire↔身份映射细节（切片 5/7 的 adapter 拥有）；本设计只保证入口吸收**已解码**的结构化字段，并给出映射面清单。
- 不做 Node key 轮换、不引入 CNG/TPM 不可导出档位、不提供持久化 fallback。
- 不修改 `core`、`storage-sqlite` 或任何协议 crate 的行为与形状。

## Decisions

### D1 两个 crate 的边界与依赖

**`identity-auth`（纯状态机）**

| 依赖 | 用途 | 备注 |
|---|---|---|
| `core`（`acp_core`） | 值对象、写集 DTO、错误、`Clock` | §5 矩阵已有 ✓ |
| `sync-protocol` / `node-link-protocol` | 只取 `domains::DOMAINS`（domain 与 field tag 表） | 不复制常量，**不使用**其业务类型或业务规则（§5 表下规则、§4.13） |
| `acpr-transcript` | 表驱动 transcript 编解码与 `base64url`/`u16be` 辅助 | 唯一编码实现 |
| `p256`（`features = [\"ecdsa\"]`） | proof 验签（**不**签名、**不**用 `from_der`） | 在 workspace 基线上增量开启 `ecdsa`（`arithmetic` 已由 `core` 使用） |
| `sha2` / `hmac` | SAS 与 pairing 证明的 HMAC-SHA256 | workspace 已登记 |
| `thiserror` | 失败分类 | |
| `async-trait` | keystore 端口是 dyn 化异步 trait | 只有 proc-macro，不引入 runtime |

- *不*依赖：`acpr-wire`（§5 矩阵中该格为空——uuid16 字段字节用 16 字节文本解码，不需要跨协议值对象）、Tokio、serde、SQLite、WebSocket、`identity-keystore`。
- 不使用 `uuid` crate：`core` 的 ID 已经是严格规范的小写 UUID 文本（`crates/core/src/model/ids.rs::is_uuid`），转 16 字节只是一次去连字符 + 十六进制解码，不需要新增依赖。
- *不*写 `cfg` 平台分支、不读系统时间、不做文件/网络 IO：`cfg` 只允许出现在 `#\[cfg(test)\]`。

**`identity-keystore`**

| 依赖 | 用途 | 备注 |
|---|---|---|
| `identity-auth` | 端口 trait 与端口值类型（`KeyPurpose`/`SecretPurpose`/`KeyHandle`/`SecretBytes`/`P1363Signature`/`KeystoreError`/熵源端口） | §5 矩阵允许（唯一允许的依赖） |
| `p256`（`features = [\"ecdsa\"]`） | 进程内签名；私钥存储为 32 字节标量，公钥输出 65 字节 SEC1 | **不**启用 `pkcs8`：不引入编码栈，签名输出天然 64 字节 P1363 |
| `getrandom`（`0.4`，lock 中已有 `0.4.3`） | OS 随机数（密钥生成、Provider 凭据以外的秘密材料） | MIT OR Apache-2.0；实现时复核 MSRV ≤ 1.85 |
| `thiserror` | 端口错误映射 | |
| `windows-dpapi`（`cfg(windows)`，候选 `0.2.0`） | 当前用户 scope 的 DPAPI 包裹/解包 | 选型核验见 D7；`anyhow` 错误在端口边界映射，不外泄 |

- *不*依赖 `core`、任何协议 crate、Tokio、SQLite、tracing（本 crate 不打日志；秘密永不进日志）。
- `Cargo.toml` 的 `[workspace.dependencies]` 曾注明「本次增量无 crate 使用」的密码学原语注释在本变更后失效，随本变更修正为「`identity-auth`/`identity-keystore` 已消费」，并登记 `getrandom` 与 DPAPI wrapper。

### D2 transcript 装配与固定向量消费

- 每个 domain 一个**字段装配**函数：输入是已校验的结构化值（`core` 的 ID/`Nonce`/`Timestamp`/`PeerPublicKey`，加 `CanonicalOrigin`/节点端点/`clientKind` 文本/特性列表），输出按 `acpr_transcript::table::encode_transcript(spec, values)` 的域内顺序给出的字段字节序列。domain 表由 `sync_protocol::domains::DOMAINS` / `node_link_protocol::domains::DOMAINS` 按字符串查找（`table::domain_spec`），`fieldTag` 与宽度不写死在 `identity-auth` 里。
- 装配规则固定为：UUID → 16 字节原始字节；Unix 秒 → `u64be`；协议版本 → `u16be`；公钥 → 65 字节原始字节；nonce → 32 字节原始字节（由无填充 base64url 文本解出）；`negotiatedFeatures` → 排序后 NUL 连接；文本字段 → UTF-8 原始字节。字段集合与顺序完全由表决定，**不**允许按条件增删（v1 的每个集合都是固定集合）。
- HMAC 与签名按表的 `Proof` 判别选择：`Proof::Hmac` 用 `hmac::Hmac<Sha256>` + `verify_slice` 做常量时间比较；`Proof::Signature` 用 `p256::ecdsa::VerifyingKey::from_sec1_bytes` + `Signature::from_slice`（64 字节）验签。验签前先断言长度，再解析点；`from_der` 不出现在代码里。
- 证据：两个 `fixtures/*/v1/transcripts/` 的 12 个正向向量逐一重算 `transcriptBase64url`（逐字节）、`transcriptSha256Hex`、HMAC 值，以及 SAS 域的 `sas`；签名域用 fixture 的 `publicKey` 验证 `p1363Signature`，并额外断言「同一 transcript 用另一把合法公钥验证失败」。`invalid/` 负例逐一断言被拒绝，且拒绝必须发生在曲线点/签名范围校验之前或之中（按 registry 的字段顺序）。

### D3 配对状态机与内存态所有权

- 一个 `Authority` 持有注入的 `keystore`/`entropy`/`clock` 与一份**进程内状态**：配对 secret 明文、挑战缓存（`challengeId` → `{serverNonce, expiresAt, peer, binding}`）、每配对失败计数、已终结但尚未提交的记录。持久事实（配对记录、对端行、信任记录）由调用方以快照入参提供与本变更的写集返回值落库，状态机**不**直接访问存储。
- 状态机入口（同步，除挑战签发需要经端口签名外均为纯计算）：
  - 创建：`begin_pairing(pairing_id, spec, requested, display_name, created_at, expires_at)` → `PairingDraft`（含 `PairingRecord` 草稿 + 只在内存的 secret + 派生 SAS 需要的本机 nonce/请求标识）；`expires_at` 由调用方按「不超过 5 分钟」规则算出后传入（状态机只复核上界）。
  - 认领：`verify_claim(pairing: &PairingRecord, existing: Option<&ClaimedPairing>, fields: &ClaimFields)` → 结构 → 状态/过期 → 绑定校验（设备：canonical origin + Host；节点：端点 host）→ 集合校验 → HMAC 校验（用内存 secret）→ `ClaimOutcome`（`Claimed` 带 65 字节公钥，`to_claim()` 给出 core 的 `PairingClaim`）；相同载荷重发得到 `Repeat`（幂等，不产生第二次写入）；失败按配对累计计数，第 5 次给出 `ClaimRejection::TooManyFailures`（调用方据此提交一次拒绝落定）并把该配对在内存标记为不可用。
  - 落定：`settle(pairing: &PairingRecord, decision: &PairingDecision, at: &Timestamp)` → `PairingSettlement`（批准时已校验「最终集合不超出请求值」）；拒绝/过期不产生信任，两条路径都清除内存 secret。
  - 过期与重启：`due_pairings(pairings, at)` 与 `unrecoverable_after_restart(pairings)` 返回需要由调用方终结的配对标识。

**实现期的口径收窄（与上面原表述的差异，已回写合同 §4.1）**：四个入口**返回领域值**（`PairingDraft`/`ClaimOutcome`/`PairingSettlement`/`PairingId`），**不**直接返回 §11.6 的写集 DTO。理由：写集是存储层形状（含审计意图与事务字段），而原子性本就由 `TrustStore` 的单事务语义承担；状态机只负责「算出该发生什么」。代价：切片 4–7 的 adapter 多一步组装（`ClaimedPairing::to_claim()` 已在 `identity-auth` 提供），好处是 `identity-auth` 不认识 `core::ports` 的写集类型。
  - SAS：`pairing_sas(&self, pairing: &PairingRecord, peer: &ClaimedPairing)`（内部调用公开的
    `derive_sas(transcript, &PairingSecret)`，使「只有一处实现」可被固定向量直接测试） → 6 位十进制字符串（HMAC-SHA256 前 4 字节 u32be `% 1_000_000`，左补零）。
- 并发与原子性由**调用方提交**保证：状态机只产生写集，`TrustStore` 的单事务语义（`claim_pairing`/`settle_pairing`/`expire_pairings`）是唯一提交点，因此 §4.2 的 5 条规则不需要在内存里再实现一遍锁语义。内存态与已提交状态的关系固定为「内存态只允许比已提交状态更严格」（例如内存里把配对标记为失效，但绝不出现「内存里批准、库里没有信任」）。
- 锁粒度：内存态用 `std::sync::Mutex`（不引入 runtime）。**不跨 `await` 持锁**：需要签名的路径（挑战签发）先取状态做计算，再在锁外调用 `keystore.sign`，最后短锁写入挑战缓存；`verify_claim`/`verify_proof` 全程同步（HMAC/验签都是 CPU 计算），不需要异步。

### D4 握手入口与持久事实快照

`IDENTITY_AND_AUTH_CONTRACT.md` §5.1 冻结的三个入口保留，并按实现需要补一个**持久事实快照**入参；本设计把这一定型写入合同 §5.1（同一变更内）：

```text
hello(request: &ChallengeRequest, trust: &PeerTrust) -> Result<ChallengeIssue, HandshakeError>
verify_proof(submission: &ProofSubmission, trust: &PeerTrust) -> Result<Authenticated, HandshakeFailure>
complete_auth(fact, pairing: Option<&PairingId>, at: &Timestamp) -> Completion   // consume_pairing 交调用方组装写集
```

- `hello` 是唯一带 `await` 的入口（签宿主证明）：**先签名、成功后才登记挑战**，失败不留半成品。
- `Completion` 只带「需要推进为 consumed 的配对」与同一时间戳；审计与 `last_seen` 写集由调用方按 §11.6 组装（与上面配对入口同一口径）。

- `PeerTrust` 是调用方从 `TrustStore` 读到的**当次**快照：对端标识、公钥、凭据状态、持久化绑定（`host_binding`：设备为 canonical origin、节点为 endpoint）与节点角色（`node_kind`），以及当前 scope/grant。`verify_proof` 只使用这份快照里的公钥验签（合同 §5.1 硬约束），并据记录状态派生 `CredentialStatus`（`Active`/`ScopeReduced`/`Revoked`/`Unknown`）。这样「验签公钥只来自持久化信任」与「授权从最新持久记录计算」都不需要状态机做 IO，且两件事都有直接测试。
- 被否的备选：让 `identity-auth` 直接依赖 `core::ports::TrustStore` 做读。否掉的理由：会把纯状态机耦合到异步存储端口，使「同一输入必得同一结果」的测试必须携带存储替身，且违反 `IDENTITY_AND_AUTH_CONTRACT.md` §5.1「三个入口都不碰 SQLite」的原始意图；读面快照入参同样满足「不得取握手消息里自带的公钥」。
- 检查顺序固定并逐条测试：① 字段长度/类型（含 base64url 无填充、P1363 恰 64 字节）→ ② 挑战存在且未被消费且未过期（注入 `Clock`，TTL 15 秒）→ ③ 绑定一致 → ④ 从快照取公钥并验签。①–④ 任一步失败都返回**同一个**证明失败分类（不区分原因），并追加 `device.auth_failed` 审计意图；只有审计写集与失败分类交给调用方。
- 一次性消费在 ③ 之后、④ 之前落定：验签成功即消费；验签失败也消费（防重放穷举），两者的区别只在审计结果。重放同一 proof 因此必然失败。
- 未知对端照常签发挑战：`hello` 对 `PeerTrust { key: None }` 返回完整挑战（含宿主证明）；`verify_proof` 在 ④ 因无公钥失败。测试断言两种情况（未知 vs 已撤销）的失败分类一致、响应不含存在性信息。

### D5 授权展开表

- `identity-auth::authorization` 内维护一张 `PACKS`/`PRESETS`/`GRANTS` 表（`&[(name, &[member])]`），展开函数返回 `ScopeSet`/`GrantSet`（core 类型，自带名称形状校验）。
- 展开规则：pack → 命令名集合；preset → 其包集合的并集（递归一层到命令名，v1 的 preset 只引用 pack）；grant → 命令名集合。`local.*` 7 项显式列为拒绝集合：既不接受它们作为请求输入，也不允许任何展开结果包含它们。
- 漂移证据：一个集成测试读取 `compatibility/commands/v1/commands.json`（dev-dependency：`serde_json`），断言三张表与文件逐项相等（名称集合与成员集合），并断言 `localCapabilities` 与拒绝集合相等。这与 `node-link-protocol/tests/schema_drift.rs` 读取同一文件的既有做法一致，不新增机器资产。
- 未知名称返回具名失败（不部分展开）；调用方（切片 4/5 的 adapter）负责把失败映射到 wire 错误码，本 crate 不定义 wire 词汇。

### D6 `identity-keystore` 的结构与存储

- 模块：`lib.rs`（`PlatformKeystore::open(root) -> Result<..>`、`EphemeralKeystore::new(entropy)`、`OsEntropy`）、`error.rs`（`KeystoreError` → 端口类型）、`entry.rs`（条目编码/解码）、`platform/`（`cfg(windows)` 的 DPAPI 模块与 `cfg(not(windows))` 的失败关闭模块）。
- 条目：`<root>/<purpose>/<label>.<version>`，内容 = 自研二进制头（magic `ACPK`、格式版本 u16be、`purpose` u8、`label` 长度 + 标签、32 字节盐；私钥标量在被包裹的那一段里，头部不含私钥）+ DPAPI 包裹。包裹使用 `Scope::User` 并带**域分离的附加熵**（`purpose` + `label` + 格式版本 + 本节点用途字符串），因此同一台机器上不同用途/标签的包裹不可互换。写入用「同目录临时文件 + 原子重命名」，目录创建时按平台设置权限（Unix `0700`/`0600`；Windows 依赖 `%LOCALAPPDATA%` 的用户 ACL 与 DPAPI 本身，启动期宽松权限检查属切片 4 的 daemon 启动检查，本变更只保证不写共享临时路径）。
- Provider 凭据（`SecretPurpose`）走同一封装路径，但只经 `get_secret`/`put_secret`/`delete_secret` 进出，`SecretBytes` 不实现 `Debug`/`Serialize`/`Display`，也不进入 `KeyHandle`。
- `identity-keystore` 不实现「引用与条目的分布式事务」，只保证：写入是新版本条目、旧的未被引用条目可被 `delete` 回收；引用缺失/解包失败一律 `KeystoreUnavailable`（不静默重建）。
- 合同收口（写入 `IDENTITY_AND_AUTH_CONTRACT.md` §7）：`KeyPurpose` 只保留 `NodeIdentity`——`DeviceIdentity` 在第一阶段没有调用方（PWA/原生客户端的设备密钥由客户端平台自持），按合同 §7 的说明在本次实现变更里**删除**，不保留无人使用的分支。
- 熵：`OsEntropy` 实现 `identity-auth` 的熵源端口（`getrandom`）；`EphemeralKeystore`/`PlatformKeystore` 都只依赖该端口，测试用计数器式 fake 熵源，因此密钥生成在单测里可重复。

### D7 平台差异与 DPAPI wrapper 选型

- `identity-auth` 不出现任何平台 `cfg`；`identity-keystore` 的平台差异只在 `platform/` 下，`#[cfg(windows)]` 用 DPAPI、`#[cfg(not(windows))]` 返回 `KeystoreUnavailable`（编译通过、运行期明确失败）。CI 在 Linux 上编译并断言失败关闭；DPAPI 往返与签名一致性测试只在本地 Windows x64 执行并留证（与 `agent-host` 的 Windows 进程树证据同一种处理）。
- DPAPI 只能经 wrapper crate（`unsafe_code = \"forbid\"` 且 §4.12 明确「本 crate 不得直接 FFI」）。首选候选 `windows-dpapi 0.2.0`：安全函数 `encrypt_data`/`decrypt_data` + `Scope::User`，许可证 MIT OR Apache-2.0，`edition 2021`。**已知代价**：它依赖已停止维护的 `winapi 0.3`（+ `anyhow`、`log`），作者单人、未声明 `rust-version`、反向依赖极少。
- 实现第一步必须按 `SECURITY_DESIGN.md` §20 实证：`cargo metadata`/`cargo tree` 的传递依赖、许可证集合（`deny.toml` allow 列表）、`rust-version`/edition 与 1.85 的兼容性、`Scope::User` 的语义与我们需要的「包裹 + 进程内签名」一致。任一项不合格 → **回到用户决策**（换 wrapper、自写 wrapper crate 并新增 ADR，或抬 MSRV），不擅自放开 `unsafe`、不静默选择另一个未核验的 crate。选型结论与版本口径写回 `MODULE_ARCHITECTURE.md` §4.12/§3.1。
- 被否的备选：直接用 `windows` crate（`CryptProtectData` 是 `unsafe fn`，与 workspace `unsafe_code = \"forbid\"` 冲突）；`keyring`（面向 Credential Manager/Keychain 的更高层抽象，不适合「DPAPI 包裹任意私钥字节」）；`dpapi-core`/`dpapi-offline`（面向离线/取证解密，不提供当前用户 master key 的加密路径）。

### D8 测试基座

- **fixture 驱动（无网络、无平台依赖）**：读 `compatibility/transcripts/v1/transcripts.json` 取登记项，读 `fixtures/{sync,node-link}/v1/transcripts/*.json` 的 `input`/`expected`，按 D2 的装配规则重算并逐字节比对；`invalid/` 负例逐一断言被拒绝。辅助代码照 `node-link-protocol/tests/support/mod.rs` 的既有形态（`CARGO_MANIFEST_DIR` → 仓库根）。
- **值对象等价语料**：`CanonicalOrigin`/`NodeEndpoint` 与协议 crate 的 wire 值对象（`sync_protocol::pairing::CanonicalOrigin`、`node_link_protocol::pairing::Endpoint`）用同一份接受/拒绝语料做**等价比对**（同一输入两边必须同为接受或同为拒绝）。语料含：带路径/查询/fragment 的 origin、非 `https`/`wss` 方案、大写主机、端口、IPv6 字面量、空串、超长串、尾随点。
- **状态机测试**：fake keystore（可注入失败/不可用）、fake 熵源（可注入固定字节）、fake `Clock`（可推进时间）。覆盖 §8 的失败路径清单：配对过期、并发认领（同一状态机两次认领的冲突结果）、重复认领（相同幂等/不同冲突）、proof 重放、密钥变化（快照公钥换一把）、撤销后再认证、错误 scope、keystore 不可用、撤销提交成功但连接清理失败（后者在本 crate 表现为「已产生写集、失败分类正确」，组合根行为留给切片 4/5）。
- **平台测试**：`#[cfg(windows)]` 的 DPAPI 往返、公钥/签名一致性、私钥材料不进 `Debug` 的断言（后者跨平台）；`#[cfg(not(windows))]` 的失败关闭断言。
- 不使用真实网络、不使用真实 Agent、不引入外部服务。

### D9 门禁与文档同步面

- `docs/IDENTITY_AND_AUTH_CONTRACT.md`：§2（类型归属补熵源端口）、§4.1（创建/认领/落定入口的最终签名与快照入参）、§5.1（三个入口 + `PeerTrust` 快照）、§7（keystore 端口最终形状、删除 `DeviceIdentity`、熵源端口）；版本号与日期注记同步更新。
- `docs/MODULE_ARCHITECTURE.md`：§3/§3.1（两个 crate 从「待落地」移入已落地；依赖口径与 wrapper/`getrandom` 版本登记）、§4.8/§4.12（实现已落地 + DPAPI wrapper 结论）、§5（矩阵行列已存在，只需确认无新增边；`app` 行已允许两者）。
- `Cargo.toml`：`members` 增加两个 crate；`[workspace.dependencies]` 登记 `getrandom` 与 DPAPI wrapper，修正密码学原语的「无 crate 使用」注释。
- `README.md`「仓库当前状态」、`docs/DEVELOPMENT_PLAN.md` §2、`AGENTS.md` §4 的模块状态表：把 `identity-auth`/`identity-keystore` 从「待落地」移入已落地行（`AGENTS.md` 状态表里 `server`/`app` 仍待落地）。
- `.gitleaks.toml`：自研 keystore 条目格式定稿时同步规则——诚实说明 DPAPI 包裹后的字节**不可**用模式识别（它是加密材料），因此规则只覆盖「明文自研头 + 未包裹私钥标量」这一不可能出现的形态，并在 `.gitleaks.toml` 的注释里记录该限制，不假装覆盖了密文。
- `scripts/check-crate-boundaries.mjs` 以 §5 文档为唯一判据，无需改脚本；`core` 的 allow-list 不受影响。

## Risks / Trade-offs

1. [DPAPI wrapper 依赖已停止维护的 `winapi 0.3`，且为单人维护、未声明 MSRV] → D7 的实证步骤 + 「不合格就回到用户决策」；同时用 `platform/` 模块隔离，替换 wrapper 不改端口。
2. [`identity-auth` 增加熵源端口与 `PeerTrust` 快照入参，属对合同 §5.1/§7 的定型] → 同一变更写回合同文档；`IDENTITY_AND_AUTH_CONTRACT.md` **没有**合同漂移门禁（`check-contract-drift.mjs` 只覆盖 `CORE_PORTS_AND_STORAGE.md` §5/§7），因此靠「文档 + 实现同改 + reviewer 核对」而非机械门禁，风险记录在此。
3. [内存态与已提交状态的偏差（如第 5 次失败后内存已失效但写集尚未提交）] → 固定不变式「内存态只允许比已提交状态更严格」，并用测试覆盖「写集未提交时该配对仍不可用于新的认领」。
4. [DPAPI 包裹 + 进程内签名意味着签名瞬间私钥在内存] → 已由 `SECURITY_DESIGN.md` §9.2 接受并记录；本变更不引入任何额外缓存或导出路径，签名函数不返回私钥材料。
5. [Linux CI 无法覆盖 DPAPI 路径] → 失败关闭路径在 CI 覆盖，DPAPI 往返与签名一致性在本地 Windows 执行并留证；最终验收如实记录 CI 未覆盖的部分。
6. [自研 keystore 条目格式是新的密钥材料载体] → 格式带 magic/版本/用途/标签，写入原子且不落共享临时路径；`.gitleaks.toml` 规则只覆盖明文形态并写明限制（见 D9）。
7. [Windows 目录 ACL 检查不在本变更] → 本变更只保证使用 `%LOCALAPPDATA%` 下的私有目录与 DPAPI 保护；宽松权限检查属 `SECURITY_DESIGN.md` §13.2 的启动检查，规划在切片 4（`app` 启动路径），本变更在路径选择上不制造共享可写位置。
8. [展开表与 `commands.json` 可能漂移] → 集成测试逐项比对（名称与成员集合），漂移即失败。
9. [`identity-auth` 不使用协议 crate 的业务类型，因此 `CanonicalOrigin`/`NodeEndpoint` 出现同名校验逻辑] → D8 的接受/拒绝语料等价比对测试；若用户认为应复用协议类型，需要放宽 §5 表下「不使用协议 crate 业务类型」的说明（用户决策）。
10. [跨 `await` 持锁导致死锁或阻塞] → D3 的锁粒度约束（不跨 `await` 持锁），并要求 reviewer 把「持锁跨越 `.await`」列为阻断项。
11. [新增 `getrandom` 直接依赖] → 该版本已在 `Cargo.lock`（`uuid` 的 `v4` 用过 `0.4.3`），MIT OR Apache-2.0，`deny.toml` allow 命中；implement 时复核 MSRV。
12. [`npm run check` 的文档引用门禁对「同一子句内紧邻指名」保守归因] → 文档改动的引用关系靠通读核对（`AGENTS.md` §10 已明确该门禁不能代替通读）。

## Migration Plan

- **无数据迁移、无 wire 变更**：不改 `schemas/`、`fixtures/`、`compatibility/`、`storage-sqlite` 表结构、`core` 端口签名与值对象；两个新 crate 不读写既有数据库（由切片 4 的组合根装配 `TrustStore`）。
- **引入顺序（同一交付单元内到位）**：`Cargo.toml` 的 `members` 与依赖口径 → 合同/架构文档（`IDENTITY_AND_AUTH_CONTRACT.md`、`MODULE_ARCHITECTURE.md`、`README.md`、`DEVELOPMENT_PLAN.md`、`AGENTS.md`）→ `identity-auth`（含 fixtures 与状态机测试）→ `identity-keystore`（含平台测试）。`check:boundaries` 以文档 §5 为判据，成员先于依赖登记会让 `npm run check` 在中间状态为红（允许，最终必须绿）。
- **锁文件**：`Cargo.lock` 在本变更内一次性更新（新增成员与 DPAPI wrapper/`getrandom` 的直接依赖边）；本变更之后的轨道不得再 `cargo update`。
- **回滚**：`git revert` 该交付单元即可——没有持久化状态、没有 wire 兼容承诺、没有迁移步骤；唯一需要注意的是回滚后 `Cargo.lock` 与 `members` 必须一起回退（同一提交内）。
- **兼容性**：不改变任何既有 crate 的行为；`core`、`storage-sqlite`、四个协议 crate 与 `agent-host` 的代码与测试保持原样；`.gitleaks.toml`/`deny.toml` 的既有条目不被削弱。
