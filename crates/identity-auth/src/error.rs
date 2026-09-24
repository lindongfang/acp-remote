//! 具名错误类型：transcript 装配、证明校验、配对、握手与授权。
//!
//! 约定（`AGENTS.md` §7）：错误是可读的返回值，不是字符串拼接；失败分类必须让调用方能区分
//! 「结构错误」「密码学失败」「状态不允许」与「不可用」。**对端可见的差异由 adapter 收敛**：
//! 配对/握手的任何失败在对端一律表现为同一类认证失败（[`pairing::ClaimRejection::public_class`]），
//! 本层的细分只服务本地决策与测试。

use acp_core::model::InvalidValue;

/// transcript 装配错误：domain 查表、字段顺序/宽度、以及编码前的文本字段转换。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TranscriptError {
    #[error("domain 不在协议表中：{0}")]
    UnknownDomain(&'static str),
    #[error("字段数量与登记表不符：期望 {expected}，实际 {got}")]
    FieldCount { expected: usize, got: usize },
    #[error("第 {position} 个字段的 tag 与登记表不符：期望 {expected}，实际 {got}")]
    FieldOrder {
        position: usize,
        expected: u16,
        got: u16,
    },
    #[error("字段 {tag} 的宽度与登记表不符：期望 {expected}，实际 {got}")]
    FieldWidth { tag: u16, expected: u32, got: u32 },
    #[error("时间戳不是规范 RFC 3339 或无法换算为 Unix 秒：{0}")]
    Timestamp(String),
    #[error("base64url 字段不是规范无填充形式或长度不符：{0}")]
    Base64Url(&'static str),
    #[error("UUID 文本不是规范小写 8-4-4-4-12：{0}")]
    Uuid(&'static str),
    #[error("字段长度超过 u32 上限")]
    FieldTooLong,
    #[error("表驱动编解码失败：{0}")]
    Table(#[from] acpr_transcript::table::TableError),
    #[error("transcript 编解码失败：{0}")]
    Codec(#[from] acpr_transcript::TranscriptError),
}

/// 证明校验错误。签名与 HMAC 失败各自独立，便于测试区分「密码学失败」与「结构错误」。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProofError {
    #[error(transparent)]
    Transcript(#[from] TranscriptError),
    #[error("签名校验失败")]
    Signature,
    #[error("HMAC 校验失败")]
    Hmac,
    #[error("公钥不是合法的 P-256 未压缩点")]
    PublicKey,
}

/// 配对状态机错误（内部决策用；对端可见分类见 [`super::pairing::ClaimRejection`]）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PairingError {
    #[error("目标与请求集合不一致：设备配对不得带 grants、节点配对不得带 scopes")]
    CapabilityKindMismatch,
    #[error("配对有效期非法：必须为正且不超过 5 分钟")]
    InvalidWindow,
    #[error("配对目标与配对记录的目标不一致")]
    TargetMismatch,
    #[error("认领载荷与配对记录的标识或对端类别不一致")]
    ClaimMismatch,
    #[error("配对记录状态不允许该操作")]
    WrongState,
    #[error("配对已过期")]
    Expired,
    #[error("配对当前不可认领")]
    NotClaimable,
    #[error("本机绑定不一致")]
    BindingMismatch,
    #[error("请求集合超出登记值")]
    CapabilitiesExceedRegistered,
    #[error("最终集合超出请求值")]
    CapabilitiesExceedRequested,
    #[error("配对 secret 缺失或已清除")]
    SecretUnavailable,
    #[error("配对证明无效")]
    Proof(#[from] ProofError),
    #[error(transparent)]
    Transcript(#[from] TranscriptError),
    #[error("熵源不可用")]
    EntropyUnavailable,
    #[error("值非法：{0}")]
    Invalid(#[from] InvalidValue),
}

/// 握手错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HandshakeError {
    #[error("连接类型与绑定字段不一致（设备侧需要 canonical origin，节点侧需要 endpoint）")]
    BindingKind,
    #[error("绑定不一致：Host 与登记绑定不匹配")]
    BindingMismatch,
    #[error("未知、已消费或已过期的挑战")]
    UnknownChallenge,
    #[error("对端 nonce 与该挑战记录不一致")]
    NonceMismatch,
    #[error("对端没有可用的持久化信任材料，或信任快照不属于本次提交的主体")]
    UntrustedPeer,
    #[error("对端信任状态与请求的连接类型不一致")]
    TrustKind,
    #[error("缺失或多余的 catalogRevision（Node Link 必填、Sync 必须不出现）")]
    MissingCatalogRevision,
    #[error("keystore 不可用：{0}")]
    Keystore(#[from] super::port::KeystoreError),
    #[error("证明无效")]
    Proof(#[from] ProofError),
    #[error(transparent)]
    Transcript(#[from] TranscriptError),
    #[error("熵源不可用")]
    EntropyUnavailable,
    #[error("值非法：{0}")]
    Invalid(#[from] InvalidValue),
}

/// 授权词表展开错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthorizationError {
    #[error("未知的授权名称：{0}")]
    UnknownName(String),
    #[error("本地管理能力不可远程授予，也不参与展开：{0}")]
    LocalCapability(String),
    #[error("值非法：{0}")]
    Invalid(#[from] InvalidValue),
}

/// crate 级错误（供 adapter 与组合根统一处理）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdentityError {
    #[error(transparent)]
    Pairing(#[from] PairingError),
    #[error(transparent)]
    Handshake(#[from] HandshakeError),
    #[error(transparent)]
    Authorization(#[from] AuthorizationError),
    #[error(transparent)]
    Transcript(#[from] TranscriptError),
    #[error(transparent)]
    Keystore(#[from] super::port::KeystoreError),
    #[error("熵源不可用")]
    EntropyUnavailable,
}
