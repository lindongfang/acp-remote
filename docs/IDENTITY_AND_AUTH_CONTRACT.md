# ACP Remote 身份与认证合同（`identity-auth`）

> 状态：编码前合同（v1 目标形状）。`identity-auth` 与 `identity-keystore` 两个 crate 均未落地；本文冻结实现前必须定型的内部边界，**不代表已实现**。
> 版本：0.1（2026-09-23：首次冻结。补上 `docs/CORE_PORTS_AND_STORAGE.md` §1 明确排除的「`identity-auth` 内部状态机」与 `docs/MODULE_ARCHITECTURE.md` §4.8 只给职责、未给签名的那一段）
> 上位文档：[MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md) §2/§4.8/§4.12/§5、[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §9/§10/§13/§14、[SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §7/§8、[NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) §8/§9/§13、[CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §3.5/§11、[LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) §2.2/§5.3/§5.4、[CONFIG_REFERENCE.md](./CONFIG_REFERENCE.md) §8、[adr/0006-identity-keystore-split.md](./adr/0006-identity-keystore-split.md)
> 作用：冻结 `identity-auth` 的状态机边界、握手入口契约（输入、输出与交给 core 的 fact）、授权展开、nonce/重放/时钟规则、撤销传播与 `identity-keystore` 端口 trait。**wire 编码不在本文**：设备与节点配对的 HTTP 载荷、二维码、transcript domain 与字段集合仍以 Sync / Node Link 协议为准；持久化面以 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §11 为准。
> 标记约定：`[决定]` = 本合同新定且不改变既有协议语义；`[待确认]` = 触及协议或产品语义，需用户确认；`[open]` = 明确留到实现阶段。

## 1. 范围与非目标

范围：`identity-auth` 的对外函数边界、内部状态机（配对、握手、授权）、它持有的内存状态（nonce/挑战/计数）、它写入与读取的持久化事实，以及它与 `identity-keystore` 的端口。

非目标：

- 不定义 ACP、Sync、Node Link 的 wire 形状（各自协议文档为唯一权威）；
- 不定义 `core::model` 值对象（[CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §3.5 与 §11.5）；
- 不定义 Export Policy 的业务交集与命令授权判定（core，见 §6）；
- 不定义平台 keystore 的实现（`identity-keystore`，见 §7）；
- 不定义 WebSocket/HTTP 监听、限流与连接生命周期（`server::transport`/`server::sync`/`server::node_link`）；
- 不引入 Noise、SQLCipher 或字段加密（`SECURITY_DESIGN.md` §20 未收口前不得新增）。

## 2. 边界与所有权

`[决定]` `identity-auth` 是**纯状态机**：不依赖任何平台 API、不写 `cfg` 平台分支、不做文件或网络 IO；所有密钥操作经 §7 的端口，所有时间经注入的 `Clock`（[CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §2）。依赖方向只有 `identity-keystore -> identity-auth`，且除 `app` 外没有 crate 依赖 `identity-keystore`（`AGENTS.md` §4、[adr/0006-identity-keystore-split.md](./adr/0006-identity-keystore-split.md)）。

依赖矩阵（谁拥有什么，避免实现时按「方便」放错层）：

| 关注点 | 所有者 | 说明 |
|---|---|---|
| 长期密钥生成、签名、平台存储 | `identity-keystore`（经 §7 端口） | `identity-auth` 只持不透明 handle |
| challenge/nonce 生成与一次性校验、proof 验签 | `identity-auth` | §5 |
| pairing secret 的内存持有、SAS 计算、配对状态机 | `identity-auth` | §4；secret 不落库、不进 keystore |
| `pack.*`/`preset.*`/`grant.*` → 命令级 scope 展开 | `identity-auth`（`authorization/`） | §6.1；输入形式只在这里出现 |
| 命令级授权判定与 Export 交集 | `core`（`broker`） | [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §6.5 |
| 按 IP/连接的认证速率与并发上限 | `server::transport` | [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §14 |
| 单个配对的 proof 失败次数与失效 | `identity-auth` | §4.5；需要配对状态与原子回写 |
| 撤销后的连接关闭 | 组合根（提交后） | [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §11.2 第 3 条 |

`[决定]` `identity-auth` 的输出只有两类：**已验证的 fact**（`Actor` 与其凭据状态）和**明确的失败分类**。它**不得**向 core 传「已授权」标志，也不得代替 core 判定命令级 scope（`AGENTS.md` §3、[CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §6.5）。

`[决定]` 本文引入的类型名归属（避免与 `core::model` 已有类型重复）：

- 已有并直接复用：`Actor`、`DeviceRecord`、`NodeRecord`、`NodeKind`、`PairingRecord`、`PairingState`、`PairingPeer`、`PairingClaim`、`PairingSettlement`、`PeerIdentity`、`ScopeSet`、`GrantSet`、`Fingerprint`、`Nonce`、`Digest`、`Timestamp`（[CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §3.5）。
- 实现变更要新增到 `core::model`：`PeerPublicKey`（[CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §11.5）与写集相关 DTO（§11.6）。节点角色直接复用已有的 `NodeKind`，**不要**新增 `NodeRole`。
- 只存在于 `identity-auth`（不进 `core::model`）：`ConnectionKind`、`ConnectionBinding`、`ChallengeRequest`、`ChallengeIssue`、`ProofSubmission`、`HandshakeCompletion`、`IdentityFact`、`Authenticated`、`CredentialStatus`、`PairingTarget`、`PairingDecision`、`PairingDraft`、`ClaimVerification`、`SettlementRequest`、`RequestedCapabilities`、`CanonicalOrigin`、`NodeEndpoint`、`PairingSecret`、`ChallengeId`、`P1363Signature`（64 字节 P1363）。
- 只存在于 `identity-keystore` 边界：`KeyPurpose`、`SecretPurpose`、`KeyHandle`、`SecretBytes`（§7）。

## 3. 密钥与身份材料

- `[决定]` **用途分离**：Node Identity Key 与 Device Identity Key 使用不同的 key purpose、不同的信任记录类型与不同签名 domain tag，即使算法相同也不得互换（[NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) §8.1、[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §9.6）。一个节点只有一把 Node Identity Key，同时用于 Sync 的 `hostProof` 与 Node Link 的 `nodeProof`（domain tag 与 transcript 字段集合不同，密钥不复制、不派生第二把）。
- `[决定]` 公钥值对象与指纹规则沿用 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §11.5：65 字节 SEC1 未压缩点、构造即校验、指纹 = `SHA-256(65 字节)` 的 64 字符小写 hex、指纹只能由 `PeerPublicKey::fingerprint()` 派生。**配对 DTO 必须携带公钥本身**，不能只带指纹：设备与节点配对的 claim 载荷本来就传 `devicePublicKey`/`accessPublicKey`（[SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §7.2、[NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) §13.2），core 的 `PairingPeer` 必须把它收下并落库，因为后续 WSS 握手不能假设对端重发公钥（[CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §11.1/§11.5）。
- `[决定]` 密码学原语固定为 `p256` + `sha2` + `hmac`（纯 Rust、无原生依赖；版本口径见 [MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md) §3.1）。实现约束（一次性探针的实测结论，[INITIAL_DESIGN.md](./INITIAL_DESIGN.md) §16 第 6 条）：
  - 验签前**先断言 65 字节**，再交给 `from_sec1_bytes`——它接受 33 字节压缩点；
  - wire 上只接受 64 字节 P1363（`r || s`），禁止 DER（`from_der` 不得出现在实现里）；
  - 合法 high-S 与 low-S 都必须接受，`r`/`s` 为 0 必须拒绝；
  - base64url 必须无填充，带填充或非字母表字符必须拒绝。
- `[决定]` 这些约束必须固化为**常驻回归测试**，而不是停留在探针记录：用 `fixtures/sync/v1/transcripts/` 与 `fixtures/node-link/v1/transcripts/` 的固定向量重算 transcript、验签、HMAC 与 SAS，并跑畸形输入负例（transcript 结构错误、非法公钥）。探针在 `sha2`/`base64` 升版后没有重跑，因此这批测试是它唯一的替代证据。
- `[决定]` 私钥、keystore 材料与 pairing secret 明文不得出现在 `core::model`、SQLite、日志、错误、测试快照或协议载荷里（[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §13.1/§14.1）。公钥与指纹是非秘密认证材料，可以落库与展示。

## 4. 配对状态机

### 4.1 状态与转移

`[决定]` 设备配对与节点配对共用同一状态机（[SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §7.0 为设备侧权威，节点侧同构），差异只在目标与集合字段：

| 维度 | 设备配对（`PairingTarget::Device`） | 节点配对（`PairingTarget::Node`） |
|---|---|---|
| 绑定校验 | canonical origin + Host | `endpoint` 的 host 与本地配置一致 |
| 集合字段 | 只带 `scopes`，**不得**带 grants | 只带 `grants`，**不得**带 scopes |
| proof 域 | pairing proof / status domain（Sync） | `node-link-pairing-proof/v1`（Node Link） |
| 创建者 | 本节点（二维码） | 两种方向都有：本节点创建（`mode = owner`）或对端创建、本节点 claim（`mode = access`） |
| 确认后写入 | `owned_device` + `owned_peer_key` | `owned_node`（角色取 claim 的 `nodeKind`，即 `core::model` 的 `NodeKind`：`access`/`owner`）+ `owned_peer_key` |

`[决定]` 目标形状的入口（实现时新增到 `identity-auth`，不在 `core::ports`）：

```rust
pub enum PairingTarget {
    Device { canonical_origin: CanonicalOrigin },
    Node { endpoint: NodeEndpoint, kind: NodeKind },
}

pub enum PairingDecision {
    Approve { granted_scopes: ScopeSet, granted_grants: GrantSet },
    Reject { reason: Option<String> },
}

/// 创建：只生成过程状态与内存 secret，落库走 TrustStore::create_pairing（§11.6）。
pub struct PairingDraft {
    pub target: PairingTarget,
    pub display_name: Option<String>,
    pub requested: RequestedCapabilities,
    pub expires_at: Timestamp,
    pub secret: PairingSecret,          // 只在内存；只有 digest 进写集
}

/// 认领校验：HMAC/proof 与绑定校验的**唯一入口**。输入是已解码的 claim 字段，
/// 输出可直接送入 §11.6 的 `PairingClaimWrite`（含 65 字节公钥）。
pub struct ClaimVerification {
    pub pairing: PairingId,
    pub peer: PairingPeer,                 // 已含 public_key
    pub requested: RequestedCapabilities,
}

/// 落定：由本地管理入口调用；先做状态/过期/peer 固定校验，再产出 PairingSettlement。
pub struct SettlementRequest {
    pub pairing: PairingId,
    pub decision: PairingDecision,
}

/// 配对上请求的集合：设备侧只允许 `scopes`，节点侧只允许 `grants`（与 §3.5 的不变式一致）。
pub struct RequestedCapabilities {
    pub scopes: ScopeSet,
    pub grants: GrantSet,
}
```

- 构造校验沿用 `PairingRecord` 的既有不变式（[CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §3.5）：`claimed_at` 非空 ⟺ 状态不是 `created`；`approved_at` 非空 ⟺ `approved`/`consumed`；`terminal_at` 非空 ⟺ `rejected`/`expired`/`consumed`；设备配对不对带 grants、节点配对不得带 scopes。
- `claimed` 只存在于服务端事务与审计，**不对外可见**（[SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §7.0）。

### 4.2 并发与原子性

`[决定]` 认领与落定必须是单事务写集，规则与 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §11.2/§11.6 逐条对应：

1. 一条配对只允许从 `created` 成功认领一次；并发认领只有一个成功，其余得到 `Conflict(AlreadyClaimed)`（路由到 Sync 的 `409 pairing.already_claimed`）。
2. 认领成功后不再接受第二个 claim（唯一 peer 行 + 条件更新共同保证）；重复的**相同**认领内容返回首次结果（幂等重试），不同内容返回冲突。
3. 确认事务同时完成：过期/状态检查、peer 固定校验、最终 scopes/grants 校验、信任记录创建、`owned_peer_key` 写入、配对状态推进与审计。任一步失败全回滚，**绝不能**返回成功却没有持久信任。
4. 拒绝或过期**不创建**信任，也不写入任何授权。
5. 已撤销身份不得经普通写入自动激活，只能按协议重新配对。

### 4.3 pairing secret 生命周期

`[决定]`：

- secret 只在创建方（或 claim 方）内存中存在；落库的只有 SHA-256 digest（`owned_pairing.secret_digest`）。
- 最迟在 `expires_at` 清除；`approved` 配对在首次 WSS 认证成功时提前清除；`rejected` 可为可靠轮询保留到原过期时间，但**不得超过**该时间。
- 二维码 fragment 与完整配对载荷读取后立即清除，不得进入日志或 analytics（[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §14.1）。
- Daemon 重启不能凭 digest 恢复 secret：启动时终结「未确认且无法继续验密」的配对，客户端重新发起；已批准的信任记录保留，正常重连不要求重新配对（[CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §11.3）。

### 4.4 SAS

`[决定]` SAS 由双方各自计算，Daemon **不得**把本机结果当作对端结果下发（[SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §7.2）：取 pairing SAS transcript 的 HMAC-SHA256 输出前 4 字节按 u32be 解释后 `% 1_000_000`，左侧补零为 6 位十进制。展示必须包含设备/节点名称、密钥指纹、SAS、请求的 scopes/grants 与过期时间（[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §9.4）。HMAC 与签名比较一律 constant-time。

### 4.5 失败计数

`[决定]` 单个配对的 proof 失败计数归 `identity-auth`（第 5 次失败后使该 pairing 失效，[SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §14）；按 IP/连接的认证尝试限流归 `server::transport`。失败一律返回 `401`，响应不得泄露具体校验差异，也不得暴露设备或配对是否存在。

## 5. 握手契约

### 5.1 职责切分

`[决定]` wire 侧的四步握手（`client_hello` → `server_challenge` → `client_proof` → `authenticated`，[SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §8；Node Link 为 `node.hello`/`node.challenge`/`node.proof`/`node.ready`，[NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) §12.2）由 `server` 的两个 adapter 负责编解码与连接状态。`identity-auth` 提供三个入口，不接触任何 IO：

```rust
pub enum ConnectionKind { SyncDevice, NodeLink }

/// 连接维度的事实：设备侧是 canonical origin + Host，节点侧是 owner endpoint。
pub struct ConnectionBinding {
    pub canonical_origin: Option<CanonicalOrigin>,
    pub host: String,
    pub endpoint: Option<NodeEndpoint>,
}

/// 1. hello 校验与挑战签发。输入是**已解码的结构化字段**，不是已拼好的字节。
pub struct ChallengeRequest {
    pub kind: ConnectionKind,
    pub peer: PeerIdentity,          // 未知 id 也必须生成挑战，不得用错误区分设备是否存在
    pub binding: ConnectionBinding,
    pub supported_features: Vec<String>,
}

pub struct ChallengeIssue {
    pub challenge_id: ChallengeId,
    pub server_nonce: Nonce,
    pub expires_at: Timestamp,
    /// 本节点身份对 host/owner proof transcript 的签名（P1363）。
    pub host_proof: P1363Signature,
}

/// 2. proof 校验。同样只接收结构化字段。
pub struct ProofSubmission {
    pub kind: ConnectionKind,
    pub challenge_id: ChallengeId,
    pub server_nonce: Nonce,
    pub peer: PeerIdentity,
    pub client_nonce: Nonce,
    pub signature: P1363Signature,
}

/// 3. 收尾：认证成功后的**唯一**副作用入口（pairing 转 consumed、last_seen、审计）。
pub struct HandshakeCompletion {
    pub fact: IdentityFact,
    pub at: Timestamp,
}
```

- `[决定]` **transcript 由 `identity-auth` 自己编码**：它依赖 `sync-protocol`/`node-link-protocol` 的 domain/字段 tag 表与 `acpr-transcript` 的 codec（[MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md) §5），因此入口只接收结构化字段，**不**接收调用方拼好的 transcript 字节——否则调用方可以自己选 domain，域分离失效。它也不得使用那些协议 crate 的业务类型或业务规则。
- `[决定]` 验签用的公钥**只能**来自持久化信任（`owned_peer_key`，[CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §11.7），不得取握手消息里自带的公钥——否则任何持有配对 ID 的对端都能用自选密钥通过握手。握手载荷里对端公钥只用于在配对时建立绑定，重连时不参与验证。
- `[决定]` 三个入口都不读系统时间、不碰 SQLite：持久化事实由返回值带着交给调用方，由写集端口落库（[CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §11.6）。

```rust
/// 交给 core 的已验证事实。core 只消费它，不重新验证签名。
pub enum IdentityFact {
    Device { device: DeviceId, scopes: ScopeSet },
    Node { node: NodeId, kind: NodeKind, grants: GrantSet },
}

pub struct Authenticated {
    pub fact: IdentityFact,
    /// 供 server 决定连接生命周期与错误码，不参与授权：`ScopeReduced` 必须主动关闭连接
    /// 要求重新认证以刷新客户端显示（SECURITY_DESIGN.md §9.5）。
    pub credential: CredentialStatus,
    pub connection: ConnectionBinding,
}

pub enum CredentialStatus { Active, ScopeReduced, Revoked, Unknown }
```

- `[决定]` core 的 `Actor` 只能由本节的验证输出构造（`Actor::Device`/`Actor::Node`）。`Actor::LocalCli` 只能由本地通道适配器在 OS 用户边界校验后构造（[LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) §2.2）；`localPrincipalRef` 只进审计，**不是** Owner 认证的最终用户身份（[NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) §8.3）。
- `[决定]` `Revoked`/`Unknown` 必须映射为 `auth.device_revoked`/`auth.device_unknown`（节点侧为 Node Link 的对应码），不得降级为 `authorization.scope_denied`——两类的可重试性与客户端行为不同。

### 5.2 nonce、重放与时钟

`[决定]`：

- `challenge` 与 `server_nonce` 每个连接生成一次，**一次性消费**；已消费或未知的 challenge 一律 `auth.proof_invalid`。
- 挑战缓存只存在于内存，TTL 取固定常量 15 秒（认证超时，[SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §14）；进程重启即丢弃，客户端重连重新握手。
- 重放同一 proof（相同 challenge、相同 nonce）必须失败，并追加 `device.auth_failed` 审计。
- 时间判定一律用注入的 `Clock`（便于测试注入过期、回拨），存储层与状态机都不得读系统时间。
- `[open]` 是否容忍客户端与服务端的时钟偏移：v1 不做偏移容忍——挑战由服务端签发并自带 TTL，客户端时间不参与判定。将来若引入带时间戳的客户端语句，必须先在本节定义窗口。
- `[决定]` 每个新 WSS 连接完整执行 challenge-response，不签发长期 bearer/session/refresh token（`AGENTS.md` §3）。

## 6. 授权

### 6.1 `pack.*` / `preset.*` / `grant.*` → scope 展开

`[决定]` 展开只在 `identity-auth` 的 `authorization/` 发生，展开结果是**命令名集合**（`ScopeSet`），其唯一机器权威是 [`compatibility/commands/v1/commands.json`](../compatibility/commands/v1/commands.json)：

| 输入形式 | 展开规则 |
|---|---|
| `pack.*` | 该 pack 直接列出的命令名集合 |
| `preset.*` | 该 preset 列出的 pack 集合的并集（递归到命令名） |
| `grant.*` | 该 grant 直接列出的命令名集合（Node Link Export 的 `scopes` 用同一张表） |

- 展开结果进入 wire 前必须是独立 scope，不允许把 `pack.*`/`preset.*` 当作 scope 下发（[LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) §5.3 `device.pair.begin`）。
- 未知或拼错的 pack/preset/grant 必须明确拒绝，不得忽略、不得部分展开。
- `[决定]` core 只看展开后的 scope 与 grant fact，不认识 pack/preset 名称（[MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md) §4.8）。
- `[决定]` 7 个 `local.*` 能力永不可远程授予，也不参与展开（[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §10.3、[LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) §5.1）。

### 6.2 有效权限交集

`[决定]` 有效权限固定为 `Owner grant ∩ Access local grant ∩ runtime capability`（[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §9.6）。`identity-auth` 只负责 principal 侧的两项（该 principal 的 scope/grant 集合），**不**计算 capability，也不缓存授权结论：授权从最新持久记录计算，不能只信任旧连接上的缓存（[CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §11.2 第 4 条）。

### 6.3 撤销与身份变化

`[决定]`：

- 撤销必须先提交持久状态，再由组合根关闭该身份的 active connection；提交失败即返回失败，不得把内存撤销当作持久成功（[CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §11.2 第 3 条）。
- scope 缩减立即作用于后续命令；高风险缩减主动关闭连接并要求重新认证。
- Node identity 变化（指纹/公钥不一致）进入 `identity_changed`，**不得**自动接受；只有按协议重新配对才能恢复（[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §9.4）。
- 第一阶段不存在传递信任：Owner 信任 Access Node 不代表信任其下游设备或其他节点。

## 7. `identity-keystore` 端口（目标形状）

`[待实现]` 端口 trait 由 `identity-auth` 定义、由 `identity-keystore` 实现（[adr/0006-identity-keystore-split.md](./adr/0006-identity-keystore-split.md) 决策 1/3）。目标形状：

```rust
pub enum KeyPurpose { NodeIdentity, DeviceIdentity }
pub enum SecretPurpose { ProviderCredential }
```

`[决定]` `KeyPurpose::DeviceIdentity` 是为将来可能在 Daemon 侧产生的设备身份预留的档位，**第一阶段没有调用方**：PWA 与原生客户端的设备密钥由客户端平台自己持有（[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §9.3 的 WebCrypto IndexedDB / Android Keystore / iOS Keychain），不经 Rust 的 `identity-keystore`。实现时若确认用不到，应在实现变更里删掉该取值，而不是留一条无人使用的分支。

```rust
/// 不透明引用：不是密钥材料。可以存进 SQLite 的引用列，但不得进日志（SECURITY_DESIGN.md §14.1）。
pub struct KeyHandle(String);

pub struct SecretBytes(Vec<u8>);   // 只经端口进出，不实现 Debug/Serde

#[async_trait]
pub trait IdentityKeystore: Send + Sync {
    async fn generate(&self, purpose: KeyPurpose, label: &str) -> Result<KeyHandle, KeystoreError>;
    /// 返回 65 字节 SEC1 未压缩公钥；实现必须保证与 sign 使用同一私钥。
    async fn public_key(&self, handle: &KeyHandle) -> Result<PeerPublicKey, KeystoreError>;
    /// 唯一使用私钥的操作：对已编码 transcript 签名，返回 64 字节 P1363。
    async fn sign(&self, handle: &KeyHandle, transcript: &[u8]) -> Result<P1363Signature, KeystoreError>;
    async fn delete(&self, handle: &KeyHandle) -> Result<(), KeystoreError>;

    async fn get_secret(&self, purpose: SecretPurpose, key: &str) -> Result<Option<SecretBytes>, KeystoreError>;
    async fn put_secret(&self, purpose: SecretPurpose, key: &str, value: &SecretBytes) -> Result<(), KeystoreError>;
    async fn delete_secret(&self, purpose: SecretPurpose, key: &str) -> Result<(), KeystoreError>;
}
```

`[决定]`：

1. 私钥不出端口——`sign` 是唯一使用点，端口不提供「导出私钥」方法；`SecretBytes` 不实现 `Debug`/`Serialize`，避免被顺手打日志或写库。
2. `secret` 端口只服务 Provider 凭据（[LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) §5.2 `provider.configure`）：写入由本地管理入口发起，读出由组合根组装 `CredentialResolver` 时在子进程启动前完成（[CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §11.6）；pairing secret 不进 keystore，它只在内存（§4.3）。
3. keystore 条目与 SQLite 引用之间**没有**分布式事务：先写新的带版本条目，再提交 SQLite 引用，失败时旧引用继续有效、未引用条目作为孤儿回收；新引用提交后才能清理旧条目；重启发现引用缺失即明确不可用，不得静默生成新身份（[CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §11.2 第 7 条）。
4. 端口必须允许「非硬件保护」的实现存在（[ADR-0006](./adr/0006-identity-keystore-split.md) 决策 5）。这里的「默认不启用」指的是 §9.2 基线下限**之外**的降级后端（例如 Linux 的持久化加密文件）；**Windows 的 DPAPI 包裹是 §9.2 明列的基线下限，属于 `identity.keystore = "platform"` 这一档，默认生效**，不需要新 ADR。正式模式下 keystore 不可用时失败关闭，由 `identity.fail_closed_on_missing_keystore` 决定启动失败（[CONFIG_REFERENCE.md](./CONFIG_REFERENCE.md) §8）。
5. 实现不得为平台差异在 `identity-auth` 里写 `cfg` 分支；平台分支只存在于 `identity-keystore`。
6. `[决定]`（2026-09-23）**Windows 第一档位已定案**：DPAPI（当前用户 scope）包裹私钥字节 + 进程内 `p256` 签名；**Linux 保持失败关闭**，持久化 fallback 与 CNG/TPM 不可导出档位都需单独 ADR（[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §9.2/§20）。实现约束：本 crate 与 `identity-keystore` 都不得直接 FFI（workspace 固定 `unsafe_code = "forbid"`），DPAPI/Secret Service 都必须经 wrapper crate；候选的 MSRV、维护状态与许可证先按 §20 核验，DPAPI 只能以「包裹 + 进程内签名」的方式使用（私钥在签名瞬间存在于内存）。

## 8. 失败关闭与不可协商的约束

- `[决定]` 网络可达不等于应用授权；Tailscale 身份不能代替设备或节点身份（`AGENTS.md` §3）。
- `[决定]` 握手与配对失败一律不泄露「设备/节点是否存在」；错误消息不得包含 secret、cookie、token、完整公钥载荷或堆栈（[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §14.1）。
- `[决定]` 本章的任何一个失败路径都必须有测试：配对过期、并发认领、重复认领、proof 重放、密钥变化、撤销后再认证、错误 scope、keystore 不可用、撤销提交成功但连接清理失败。缺一项即视为该功能未完成（与 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §11.4 同标准）。

## 9. 待确认与开放项

- `[待确认]` `identity-keystore` 与 `agent-host` 的 **wrapper crate 选型**（DPAPI wrapper、Windows Job Object wrapper）：档位已定（§7 第 6 条、[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §20），但具体 crate 必须在实现切片里按 `AGENTS.md` §7 核验后选定，并把结论写回 `MODULE_ARCHITECTURE.md` §4.5/§4.12；若候选的 MSRV 高于 `rust-version = 1.85`，先单独决定是否抬 MSRV。
- `[已裁定]`（2026-09-23）为实现管理写集而新增的 `ConflictKind` 取值 `AlreadyExists`/`IdentityMismatch`/`DuplicateOwnership` 与 `UnavailableKind` 取值 `KeystoreUnavailable`：见 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §11.6，枚举本体在实现变更里与本合同同时落地。
- `[open]` 客户端与服务端时钟偏移容忍（§5.2）。
- `[open]` 组织级用户身份：第一阶段明确只有节点级信任，需要 Owner 直接认证操作人时必须新增独立协议，不能把 `localPrincipalRef` 升格为安全凭据（[NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) §8.3）。
- `[open]` `node.rotate-key.*`（`post_mvp`）：Node key 轮换的协议消息与本地方法已登记但不在首切片（[LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) §5.7）。
