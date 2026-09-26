//! 值对象与已验证事实（`docs/IDENTITY_AND_AUTH_CONTRACT.md` §2 的 `identity-auth` 专属类型）。
//!
//! 这些类型**不进** `core::model`：它们要么是本层的过程状态（配对草稿、认领结果、挑战记录），
//! 要么是 wire 边界的结构化输入。core 已有的值对象（`NodeId`/`DeviceId`/`PairingId`/`NodeKind`/
//! `PeerIdentity`/`PeerPublicKey`/`ScopeSet`/`GrantSet`/`Nonce`/`Digest`/`Timestamp` 与
//! `PairingRecord`/`PairingPeer`/`PairingClaim`/`PairingSettlement`/`PairingTarget`）直接复用，不重复定义。

use std::fmt;
use std::str::FromStr;

use acp_core::model::InvalidValue;
use acp_core::model::{
    AuditAction, DeviceId, Digest, Fingerprint, GrantSet, NodeId, NodeKind, Nonce, PairingId,
    PairingRecord, PairingState, PairingTarget, PeerIdentity, PeerPublicKey, ScopeSet, Timestamp,
};

use crate::error::TranscriptError;
use crate::port::{EntropyError, EntropySource};

/// transcript 协议版本（v1）。
pub const PROTOCOL_VERSION: u16 = 1;

/// 配对有效期上限：5 分钟（`docs/SYNC_PROTOCOL.md` §7、§14）。
pub const PAIRING_MAX_SECONDS: u64 = 300;

/// 挑战（认证）有效期：15 秒（`docs/SYNC_PROTOCOL.md` §14）。
pub const CHALLENGE_TTL_SECONDS: u64 = 15;

/// 单个配对的 proof 失败次数上限；达到后该配对失效（合同 §4.5）。
pub const PAIRING_MAX_FAILURES: u32 = 5;

// ---------------------------------------------------------------------------------------------
// 绑定值对象（与协议侧同名值对象的校验语义**等价比对**由测试语料保证，见 tests/parity.rs）
// ---------------------------------------------------------------------------------------------

/// `https://<authority>`：不含 path、query、fragment、用户信息，无空白/控制字符，≤2048 字符。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CanonicalOrigin(String);

impl CanonicalOrigin {
    /// 解析。语义与 `sync_protocol::pairing::CanonicalOrigin::parse` 逐条一致（等价比对测试）。
    pub fn parse(text: &str) -> Result<Self, TranscriptError> {
        let authority = text.strip_prefix("https://");
        let well_formed = text.chars().count() <= 2048
            && authority.is_some_and(|authority| {
                !authority.is_empty()
                    && !authority.contains(['/', '?', '#'])
                    && !authority
                        .chars()
                        .any(|character| character.is_whitespace() || character.is_control())
            });
        if !well_formed {
            return Err(TranscriptError::Base64Url("canonical origin"));
        }
        Ok(Self(text.to_owned()))
    }

    /// 文本。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// authority（`host[:port]`）。
    pub fn authority(&self) -> &str {
        self.0.trim_start_matches("https://")
    }
}

/// `wss://<authority>/node-link/v1`：authority 非空且不含 `/`、`?`、`#` 与空白，≤2048 字符。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeEndpoint(String);

impl NodeEndpoint {
    /// 解析。语义与 `node_link_protocol::pairing::Endpoint::parse` 逐条一致（等价比对测试）。
    pub fn parse(text: &str) -> Result<Self, TranscriptError> {
        const SCHEME: &str = "wss://";
        const PATH: &str = "/node-link/v1";
        let authority = text
            .strip_prefix(SCHEME)
            .and_then(|rest| rest.strip_suffix(PATH));
        let well_formed = text.chars().count() <= 2048
            && authority.is_some_and(|authority| {
                !authority.is_empty()
                    && !authority.contains(['/', '?', '#'])
                    && !authority
                        .chars()
                        .any(|character| character.is_whitespace() || character.is_control())
            });
        if !well_formed {
            return Err(TranscriptError::Base64Url("endpoint"));
        }
        Ok(Self(text.to_owned()))
    }

    /// 文本。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// authority（`host[:port]`）。
    pub fn authority(&self) -> &str {
        self.0
            .trim_start_matches("wss://")
            .trim_end_matches("/node-link/v1")
    }
}

/// 生成一个「规范小写 UUID 文本」 newtype：wire 上是 UUID，transcript 里编码为 16 字节。
macro_rules! uuid_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// 从规范 UUID 文本构造。
            pub fn parse(text: &str) -> Result<Self, TranscriptError> {
                if !is_canonical_uuid(text) {
                    return Err(TranscriptError::Uuid(stringify!($name)));
                }
                Ok(Self(text.to_owned()))
            }

            /// 从 16 字节构造（置 UUID v4 版本与变体位，保证形态规范且不暴露随机源细节）。
            pub fn from_bytes(mut bytes: [u8; 16]) -> Self {
                bytes[6] = (bytes[6] & 0x0f) | 0x40;
                bytes[8] = (bytes[8] & 0x3f) | 0x80;
                Self(format_uuid(&bytes))
            }

            /// 用注入的熵源生成一个新标识。
            pub fn generate(entropy: &dyn EntropySource) -> Result<Self, EntropyError> {
                let mut bytes = [0u8; 16];
                entropy.fill(&mut bytes)?;
                Ok(Self::from_bytes(bytes))
            }

            /// 文本。
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

uuid_id!(
    /// 连接标识：wire 上的 `connectionId`（每次连接生成一次，挑战缓存的一次性键）。
    ChallengeId
);

uuid_id!(
    /// 配对认领的请求标识：wire 上的 `pairingRequestId`（配对 claim 响应与状态查询共用）。
    PairingRequestId
);

/// 64 字节 P1363 签名（`r || s`）。构造即断言长度：DER（71/72 字节）在这一步就被拒绝。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P1363Signature([u8; 64]);

impl P1363Signature {
    /// 签名长度。
    pub const LEN: usize = 64;

    /// 从原始字节构造（必须恰好 64 字节）。
    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self, TranscriptError> {
        let bytes: [u8; 64] = bytes
            .try_into()
            .map_err(|_| TranscriptError::Base64Url("signature"))?;
        Ok(Self(bytes))
    }

    /// 从规范无填充 base64url 文本构造。
    pub fn try_from_base64url(text: &str) -> Result<Self, TranscriptError> {
        let bytes = acpr_transcript::decode_base64url(text)
            .map_err(|_| TranscriptError::Base64Url("signature"))?;
        Self::try_from_bytes(&bytes)
    }

    /// 原始字节。
    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }

    /// 编码为规范无填充 base64url。
    pub fn to_base64url(self) -> String {
        acpr_transcript::encode_base64url(&self.0)
    }

    /// `r` 与 `s` 是否都非零（零值分量一律拒绝，与 `docs/SYNC_PROTOCOL.md` §6.2 一致）。
    pub fn components_nonzero(&self) -> bool {
        self.0[..32].iter().any(|byte| *byte != 0) && self.0[32..].iter().any(|byte| *byte != 0)
    }
}

/// 32 字节配对证明（HMAC-SHA256 输出）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairingProof([u8; 32]);

impl PairingProof {
    /// 长度。
    pub const LEN: usize = 32;

    /// 从原始字节构造。
    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self, TranscriptError> {
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| TranscriptError::Base64Url("pairing proof"))?;
        Ok(Self(bytes))
    }

    /// 从规范无填充 base64url 文本构造。
    pub fn try_from_base64url(text: &str) -> Result<Self, TranscriptError> {
        let bytes = acpr_transcript::decode_base64url(text)
            .map_err(|_| TranscriptError::Base64Url("pairing proof"))?;
        Self::try_from_bytes(&bytes)
    }

    /// 原始字节。
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// pairing secret（32 字节）。明文只在内存；落库的只有 [`PairingSecret::digest`]。
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PairingSecret([u8; 32]);

impl PairingSecret {
    /// 长度。
    pub const LEN: usize = 32;

    /// 从注入的熵源生成。
    pub fn generate(entropy: &dyn EntropySource) -> Result<Self, EntropyError> {
        let mut bytes = [0u8; 32];
        entropy.fill(&mut bytes)?;
        Ok(Self(bytes))
    }

    /// 从 32 字节构造（从二维码/载荷读入时使用）。
    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self, TranscriptError> {
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| TranscriptError::Base64Url("pairing secret"))?;
        Ok(Self(bytes))
    }

    /// 原始字节（**只在内存**：对端 HMAC 校验与配对 URL 组装）。
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// 摘要：`SHA-256(secret)` 的规范无填充 base64url 文本（唯一落库形态）。
    ///
    /// 返回 `Result` 而不是在内部断言：`AGENTS.md` §7 要求**正常运行路径**不使用
    /// `unwrap`/`expect`，即使这里的不变式（32 字节 SHA-256 的 base64url 文本必然是规范
    /// `Digest` 形状）客观成立。理论上不可达的分支交回调用方按错误处理，不靠 panic 表达。
    pub fn digest(&self) -> Result<Digest, InvalidValue> {
        use sha2::Digest as _;
        let digest = sha2::Sha256::digest(self.0);
        Digest::new(&acpr_transcript::encode_base64url(&digest))
    }

    /// 比较（常量时间）。用于「摘要与内存 secret 是否一致」这类判定。
    pub fn matches(&self, other: &Self) -> bool {
        let mut diff = 0u8;
        for (left, right) in self.0.iter().zip(other.0.iter()) {
            diff |= left ^ right;
        }
        diff == 0
    }
}

impl fmt::Debug for PairingSecret {
    /// 秘密不进日志：只表明类型。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PairingSecret(<redacted>)")
    }
}

/// 6 位十进制短验证码（`docs/SYNC_PROTOCOL.md` §7.2）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sas(String);

impl Sas {
    /// 由 HMAC-SHA256 输出前 4 字节（大端 u32）对 1,000,000 取模、左补零为 6 位十进制派生。
    pub fn from_hmac_output(output: &[u8]) -> Result<Self, TranscriptError> {
        let head: [u8; 4] = output
            .get(..4)
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or(TranscriptError::Base64Url("sas"))?;
        Ok(Self(format!("{:06}", u32::from_be_bytes(head) % 1_000_000)))
    }

    /// 文本（6 位十进制）。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Sas {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `clientKind`（`docs/SYNC_PROTOCOL.md` §6.3 的 4 个取值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClientKind {
    /// 浏览器 PWA（含安装到主屏幕的实例）。
    Pwa,
    /// Android 原生客户端。
    Android,
    /// iOS 原生客户端。
    Ios,
    /// 桌面原生客户端。
    Desktop,
}

impl ClientKind {
    /// 全部取值（顺序即协议表顺序）。
    pub const ALL: &'static [Self] = &[Self::Pwa, Self::Android, Self::Ios, Self::Desktop];

    /// 稳定 token。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pwa => "pwa",
            Self::Android => "android",
            Self::Ios => "ios",
            Self::Desktop => "desktop",
        }
    }
}

impl fmt::Display for ClientKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ClientKind {
    type Err = TranscriptError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|kind| kind.as_str() == text)
            .ok_or(TranscriptError::UnknownDomain("clientKind"))
    }
}

// ---------------------------------------------------------------------------------------------
// 配对输入与结果
// ---------------------------------------------------------------------------------------------

/// 配对目标与绑定：设备为 canonical origin，节点为端点与角色。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingSpec {
    /// 设备配对：绑定是本机 canonical origin。
    Device {
        /// 本机 canonical origin。
        canonical_origin: CanonicalOrigin,
    },
    /// 节点配对：绑定是本机在该配对中的 endpoint。
    Node {
        /// 本机 endpoint。
        endpoint: NodeEndpoint,
        /// 本机在该配对中的角色。
        kind: NodeKind,
    },
}

impl PairingSpec {
    /// 目标族（core 的 `PairingTarget`）。
    pub fn target(&self) -> PairingTarget {
        match self {
            Self::Device { .. } => PairingTarget::Device,
            Self::Node { .. } => PairingTarget::Node,
        }
    }

    /// 登记绑定文本：认领时对端必须逐字回显（`CORE_PORTS_AND_STORAGE.md` §11.2 第 1 条）。
    pub fn host_binding(&self) -> &str {
        match self {
            Self::Device { canonical_origin } => canonical_origin.as_str(),
            Self::Node { endpoint, .. } => endpoint.as_str(),
        }
    }

    /// 节点角色（设备配对为 `None`）。
    pub fn node_kind(&self) -> Option<NodeKind> {
        match self {
            Self::Device { .. } => None,
            Self::Node { kind, .. } => Some(*kind),
        }
    }
}

/// 配对上请求的集合：设备侧只允许 `scopes`，节点侧只允许 `grants`。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestedCapabilities {
    /// 请求的 scope（仅设备配对）。
    pub scopes: ScopeSet,
    /// 请求的 grant（仅节点配对）。
    pub grants: GrantSet,
}

impl RequestedCapabilities {
    /// 空集合。
    pub fn empty() -> Self {
        Self::default()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.scopes.is_empty() && self.grants.is_empty()
    }

    /// 与目标族是否一致（设备不得带 grants、节点不得带 scopes）。
    pub fn matches_target(&self, target: PairingTarget) -> bool {
        match target {
            PairingTarget::Device => self.grants.is_empty(),
            PairingTarget::Node => self.scopes.is_empty(),
        }
    }

    /// 请求集合是否不超出 `registered`（登记值是本机承诺的上限）。
    pub fn within(&self, registered: &Self) -> bool {
        self.scopes
            .iter()
            .all(|name| registered.scopes.contains(name))
            && self
                .grants
                .iter()
                .all(|name| registered.grants.contains(name))
    }
}

/// 配对的创建结果：待落库的记录草稿 + 只在内存的 secret 与其派生 SAS 需要的本机值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingDraft {
    /// 状态为 `created` 的配对记录草稿（含 digest、绑定与过期时间）。
    pub record: PairingRecord,
    /// pairing secret 明文（只在内存；落库只有 [`PairingDraft::record`] 的 digest）。
    pub secret: PairingSecret,
    /// 本机在配对期内使用的 server nonce（SAS transcript 字段 `serverNonce`）。
    pub server_nonce: Nonce,
    /// 本机为该配对生成的请求标识（SAS transcript 字段 `pairingRequestId`）。
    pub pairing_request_id: PairingRequestId,
}

/// 配对通道可读的**非秘密**请求材料：claim 响应与 Owner 证明需要的 server nonce 与请求标识。
///
/// 二者与 secret 同生同灭（`state::PairingMaterial`），因此 secret 已被清除时这里也读不到（`None`）；
/// 它们本身不是凭据（wire 上会原样发给对端），所以只经
/// [`Authority::pairing_request_material`](crate::Authority::pairing_request_material) 读取，不由状态机
/// 主动下发，也不与 secret 一起暴露。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingRequestMaterial {
    /// 本机为该配对生成的 server nonce（`node-link-pairing-owner-proof/v1` 的 `serverNonce`）。
    pub server_nonce: Nonce,
    /// 本机为该配对生成的请求标识（claim 响应与状态查询共用）。
    pub pairing_request_id: PairingRequestId,
}

/// 本地确认（批准/拒绝）的决策。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingDecision {
    /// 批准：`granted_*` 是用户确认的最终集合。
    Approve {
        /// 最终 scope（仅设备配对）。
        granted_scopes: ScopeSet,
        /// 最终 grant（仅节点配对）。
        granted_grants: GrantSet,
    },
    /// 拒绝：可选简短原因（≤256 字符）。
    Reject {
        /// 原因（不进审计正文）。
        reason: Option<String>,
    },
}

/// 认领载荷里与目标族相关的字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimKindFields {
    /// 设备侧声明自己的客户端类别。
    Device {
        /// `clientKind`。
        client_kind: ClientKind,
    },
    /// 节点侧声明自己的角色。
    Node {
        /// 该对端在本机记录中的角色。
        node_kind: NodeKind,
    },
}

impl ClaimKindFields {
    /// 与目标族一致时返回 `true`。
    pub fn matches_target(self, target: PairingTarget) -> bool {
        matches!(
            (self, target),
            (Self::Device { .. }, PairingTarget::Device) | (Self::Node { .. }, PairingTarget::Node)
        )
    }

    /// 节点角色（设备为 `None`）。
    pub fn node_kind(self) -> Option<NodeKind> {
        match self {
            Self::Device { .. } => None,
            Self::Node { node_kind } => Some(node_kind),
        }
    }
}

/// 认领载荷（**已解码**的结构化字段；wire 解码与 base64url→字节由 adapter 完成）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimFields {
    /// 配对标识（必须等于配对记录的目标）。
    pub pairing: PairingId,
    /// 对端身份（设备或节点）。
    pub peer: PeerIdentity,
    /// 与目标族相关的声明字段。
    pub kind: ClaimKindFields,
    /// 对端展示名（设备 `deviceName`，节点 `nodeName`）。
    pub display_name: String,
    /// 回显的绑定（设备 `canonicalOrigin`，节点 `endpoint`）。
    pub host_binding: String,
    /// 对端公钥（65 字节 SEC1 未压缩；由 `core::model::PeerPublicKey` 构造校验）。
    pub public_key: PeerPublicKey,
    /// 对端 nonce。
    pub client_nonce: Nonce,
    /// 请求集合（不得超过登记值）。
    pub requested: RequestedCapabilities,
    /// 配对证明（HMAC-SHA256，32 字节）。
    pub proof: PairingProof,
}

/// 认领拒绝的原因（**本地决策用**；对端可见分类一律相同）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimRejection {
    /// 结构/字段非法（含标识不一致、目标族与载荷不一致）。
    Malformed,
    /// 其他对端已认领、状态不是 `created`、或已被内存标记失效。
    NotClaimable,
    /// 已过期。
    Expired,
    /// 绑定与登记值不一致。
    BindingMismatch,
    /// 请求集合超出登记值。
    CapabilitiesExceedRegistered,
    /// 证明（HMAC）无效。
    ProofInvalid,
    /// 失败次数达到上限：调用方 **MUST** 提交一次拒绝落定把失效持久化（合同 §4.5）。
    TooManyFailures,
}

impl ClaimRejection {
    /// 对端可见的失败分类：**所有**拒绝原因都收敛为同一类，不暴露配对是否存在或校验差异
    /// （`docs/IDENTITY_AND_AUTH_CONTRACT.md` §4.5/§8）。
    pub const fn public_class(self) -> ClaimFailureClass {
        ClaimFailureClass::AuthenticationFailed
    }
}

/// 对端可见的认领失败分类（v1 只有一种）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimFailureClass {
    /// 统一认证失败。
    AuthenticationFailed,
}

/// 首次认领的结果快照：相同载荷重发时可原样重放（幂等重试，不产生第二次写入）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedPairing {
    /// 配对标识。
    pub pairing: PairingId,
    /// 已固定的对端身份。
    pub peer: PeerIdentity,
    /// 对端展示名。
    pub display_name: String,
    /// 首次认领的对端公钥。
    pub public_key: PeerPublicKey,
    /// 首次认领的回显绑定。
    pub host_binding: String,
    /// 首次认领的对端 nonce。
    pub client_nonce: Nonce,
    /// 首次认领的请求集合。
    pub requested: RequestedCapabilities,
}

impl ClaimedPairing {
    /// 是否与本次载荷完全一致（相同内容 → 幂等重试；不同内容 → 冲突）。
    pub fn matches(&self, fields: &ClaimFields) -> bool {
        self.peer == fields.peer
            && self.host_binding == fields.host_binding
            && self.client_nonce == fields.client_nonce
            && self.public_key == fields.public_key
            && self.pairing == fields.pairing
            && self.requested == fields.requested
    }
}

/// 认领结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimOutcome {
    /// 首次认领成功：调用方提交 [`acp_core::ports::PairingClaimWrite`]（单事务）。
    Claimed(Box<ClaimedPairing>),
    /// 同一对端以相同载荷重发：返回首次结果（幂等），**不**产生第二次写入。
    Repeat(Box<ClaimedPairing>),
    /// 具名拒绝。
    Rejected(ClaimRejection),
}

// ---------------------------------------------------------------------------------------------
// 握手与已验证事实
// ---------------------------------------------------------------------------------------------

/// 连接类型：决定走 Sync 设备域还是 Node Link 节点域。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConnectionKind {
    /// Sync 设备连接。
    SyncDevice,
    /// Node Link 节点连接。
    NodeLink,
}

impl ConnectionKind {
    /// 稳定 token。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SyncDevice => "sync-device",
            Self::NodeLink => "node-link",
        }
    }
}

/// 连接维度的事实：设备侧是 canonical origin + Host，节点侧是 endpoint + Host。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionBinding {
    /// 设备侧：已验证 HTTP Origin 推出的 canonical origin。
    pub canonical_origin: Option<CanonicalOrigin>,
    /// `Host` 头（或等价连接信息）。
    pub host: String,
    /// 节点侧：本机对该对端登记的 endpoint。
    pub endpoint: Option<NodeEndpoint>,
}

/// 凭据状态：供 server 决定连接生命周期与错误码，**不参与授权**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialStatus {
    /// 有效。
    Active,
    /// 范围缩减（`ScopeReduced` 必须主动关闭连接要求重新认证）。
    ScopeReduced,
    /// 已撤销。
    Revoked,
    /// 未知（无信任记录）。
    Unknown,
}

/// 调用方从持久化信任读到的**当次**快照：验签公钥的唯一来源（合同 §5.1）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerTrust {
    /// 对端身份。
    pub peer: PeerIdentity,
    /// 对端公钥；`None` = 未知对端（hello 仍必须照常签发挑战）。
    pub public_key: Option<PeerPublicKey>,
    /// 凭据状态。
    pub credential: CredentialStatus,
    /// 持久化信任里的绑定：设备为 canonical origin，节点为 endpoint（用于 Origin/Host 校验）。
    pub host_binding: Option<String>,
    /// 该对端在本机的节点角色（`NodeRecord.kind`；设备侧为 `None`）。
    pub node_kind: Option<NodeKind>,
    /// 设备侧当前 scope。
    pub scopes: ScopeSet,
    /// 节点侧当前 grant。
    pub grants: GrantSet,
}

impl PeerTrust {
    /// 未知对端：没有公钥、没有绑定、状态 `Unknown`。
    pub fn unknown(peer: PeerIdentity) -> Self {
        Self {
            peer,
            public_key: None,
            credential: CredentialStatus::Unknown,
            host_binding: None,
            node_kind: None,
            scopes: ScopeSet::empty(),
            grants: GrantSet::empty(),
        }
    }
}

/// 交给 core 的已验证事实。core 只消费它，不重新验证签名，也不据此跳过授权判定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityFact {
    /// 设备主体及其 scope。
    Device {
        /// 设备标识。
        device: DeviceId,
        /// 该设备的 scope（最新持久记录的展开结果）。
        scopes: ScopeSet,
    },
    /// 节点主体及其 grant。
    Node {
        /// 节点标识。
        node: NodeId,
        /// 该对端在本机的角色。
        kind: NodeKind,
        /// 该节点的 grant。
        grants: GrantSet,
    },
}

/// 认证结果：事实 + 凭据状态 + 连接绑定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authenticated {
    /// 已验证事实。
    pub fact: IdentityFact,
    /// 凭据状态。
    pub credential: CredentialStatus,
    /// 连接绑定。
    pub connection: ConnectionBinding,
}

/// 握手失败的本地分类（含对端可见分类与应记录的审计动作）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("握手失败：{reason}")]
pub struct HandshakeFailure {
    /// 具体原因（本地决策与测试用）。
    pub reason: crate::error::HandshakeError,
    /// 发生失败时请求的连接类型。
    pub kind: ConnectionKind,
}

impl HandshakeFailure {
    /// 构造。
    pub fn new(reason: crate::error::HandshakeError, kind: ConnectionKind) -> Self {
        Self { reason, kind }
    }

    /// 对端可见的失败分类：**所有**原因都收敛为同一类，不泄露差异。
    pub const fn public_class(&self) -> HandshakeFailureClass {
        HandshakeFailureClass::AuthenticationFailed
    }

    /// §14.2 登记集合里对应的审计动作。
    ///
    /// 失败分类与服务端映射的分工：Sync 设备侧认证失败是 `device.auth_failed`，Node Link 侧是
    /// `node.auth_failed`（两个动作于 2026-09-26 随 design D12 进入审计词表，落库列由存储的 v3 重建
    /// 扩宽）。返回的是**动作**而非审计行：具体行由调用方按本次连接的对端身份组装并落库。
    pub fn audit(&self) -> Option<AuditAction> {
        match self.kind {
            ConnectionKind::SyncDevice => Some(AuditAction::DeviceAuthFailed),
            ConnectionKind::NodeLink => Some(AuditAction::NodeAuthFailed),
        }
    }
}

/// 握手失败的对端可见分类（v1 只有一种）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandshakeFailureClass {
    /// 统一认证失败。
    AuthenticationFailed,
}

/// 配对状态查询可对外展示的视图（设备/节点共用的形状）。
///
/// `created` 阶段只允许暴露状态与过期时间：对端标识、名称、指纹、SAS 与请求集合都必须为 `None`
/// （`docs/SYNC_PROTOCOL.md` §7.0；`docs/LOCAL_ADMIN_PROTOCOL.md` §5.3/§5.4）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingStatusView {
    /// 对外可见状态（过期会覆盖非批准态）。
    pub state: PairingState,
    /// 对端展示名（认领后才有）。
    pub display_name: Option<String>,
    /// 对端公钥指纹（认领后才有）。
    pub public_key_fingerprint: Option<Fingerprint>,
    /// 6 位 SAS（认领后才有，且由调用方传入而不由本层计算对端结果）。
    pub sas: Option<Sas>,
    /// 请求集合（认领后才有）。
    pub requested: Option<RequestedCapabilities>,
    /// 对端身份（认领后才有）。
    pub peer: Option<PeerIdentity>,
    /// 过期时间（始终可见）。
    pub expires_at: Timestamp,
}

/// 认证收尾的返回值：需要调用方落库的配对消费目标（写集与审计组装属 §11.6/§14.2）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// 本次认证的事实。
    pub fact: IdentityFact,
    /// 认证时间（调用方用同一时间写 `last_seen` 与审计）。
    pub at: Timestamp,
    /// 需要推进为 `consumed` 的配对（已批准配对的首次认证成功时）。
    pub consume_pairing: Option<PairingId>,
}

// ---------------------------------------------------------------------------------------------
// 辅助
// ---------------------------------------------------------------------------------------------

/// 规范小写 UUID 文本（`^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$`）。
pub(crate) fn is_canonical_uuid(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                *byte == b'-'
            } else {
                matches!(byte, b'0'..=b'9' | b'a'..=b'f')
            }
        })
}

/// UUID 文本 → 16 字节（调用方需先经 [`is_canonical_uuid`] 校验）。
pub(crate) fn uuid_bytes(text: &str) -> [u8; 16] {
    let mut out = [0u8; 16];
    let mut index = 0usize;
    for byte in text.bytes() {
        if byte == b'-' {
            continue;
        }
        let value = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            _ => 0,
        };
        if index % 2 == 0 {
            out[index / 2] = value << 4;
        } else {
            out[index / 2] |= value;
        }
        index += 1;
    }
    out
}

/// 16 字节 → 规范小写 UUID 文本。
pub(crate) fn format_uuid(bytes: &[u8; 16]) -> String {
    let mut text = String::with_capacity(36);
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 4 | 6 | 8 | 10) {
            text.push('-');
        }
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/// 状态与失效判定用的时间比较：固定 RFC 3339 文本的字典序即时间序。
pub(crate) fn at_or_after(now: &Timestamp, instant: &Timestamp) -> bool {
    now.as_str() >= instant.as_str()
}

/// 配对记录的状态是否属于「未确认」：重启后无法继续验密，必须终结（合同 §4.3）。
pub(crate) fn is_unconfirmed(state: PairingState) -> bool {
    matches!(
        state,
        PairingState::Created | PairingState::PendingConfirmation
    )
}
