//! 身份、配对、签名与授权的**纯状态机**（`docs/IDENTITY_AND_AUTH_CONTRACT.md`）。
//!
//! 本 crate 的三条硬边界：
//!
//! - **不依赖平台 API**：不写 `cfg` 平台分支、不做文件或网络 IO、不读系统时间（时间经注入的
//!   [`acp_core::ports::Clock`]），不访问数据库（持久事实由调用方以快照入参提供、以领域值返回）；
//! - **密钥只在端口后面**：签名经注入的 [`port::IdentityKeystore`]，随机性经注入的
//!   [`port::EntropySource`]；本 crate 不持私钥、不提供导出私钥的方法；
//! - **transcript 只有一份实现**：domain 与 field tag 表来自 `sync-protocol` / `node-link-protocol`
//!   的 `DOMAINS`，编解码来自叶子 crate `acpr-transcript`；本 crate 不复制常量、不手拼字节。
//!
//! 模块划分：`types`（值对象与事实）、`transcript`（12 个 domain 的输入与编解码/验签/HMAC 入口）、
//! `pairing`（配对状态机）、`handshake`（逐连接挑战-证明）、`authorization`（授权词表展开）、
//! `port`（keystore 与熵源端口）、`state`（内存态：配对 secret、挑战缓存、失败计数）。

mod authority;
pub mod authorization;
pub mod error;
pub mod handshake;
pub mod pairing;
pub mod port;
mod state;
pub mod transcript;
pub mod types;

// 端口与公开 API 里出现的 `core::model` 值对象：这里如实转出，使端口**实现方**（`identity-keystore`）
// 只依赖本 crate 就能写出 `IdentityKeystore`，而不需要为了签名类型反向依赖 `core`
// （`docs/MODULE_ARCHITECTURE.md` §5 的依赖矩阵不允许 `identity-keystore -> core`）。
pub use acp_core::model::{
    AuditAction, DeviceId, Digest, Fingerprint, GrantSet, InvalidValue, NodeId, NodeKind, Nonce,
    PairingClaim, PairingId, PairingPeer, PairingRecord, PairingSettlement, PairingState,
    PairingTarget, PeerIdentity, PeerPublicKey, ScopeSet, Timestamp,
};

pub use authority::Authority;
pub use error::{
    AuthorizationError, HandshakeError, IdentityError, PairingError, ProofError, TranscriptError,
};
pub use handshake::{ChallengeIssue, ChallengeRequest, ProofSubmission};
pub use port::{
    EntropyError, EntropySource, IdentityKeystore, KeyHandle, KeyPurpose, KeystoreError,
    SecretBytes, SecretPurpose,
};
pub use state::MAX_CHALLENGES;
pub use transcript::{
    FeatureList, NodeLinkChallenge, NodeLinkPairingOwnerProof, NodeLinkPairingProof,
    NodeLinkPairingSas, NodeLinkPairingStatus, NodeLinkProof, SyncDeviceProof, SyncHostChallenge,
    SyncPairingHostProof, SyncPairingProof, SyncPairingSas, SyncPairingStatus, derive_sas,
    timestamp_from_unix_seconds,
};
pub use types::{
    Authenticated, CHALLENGE_TTL_SECONDS, CanonicalOrigin, ChallengeId, ClaimFailureClass,
    ClaimFields, ClaimKindFields, ClaimOutcome, ClaimRejection, ClaimedPairing, ClientKind,
    Completion, ConnectionBinding, ConnectionKind, CredentialStatus, HandshakeFailure,
    HandshakeFailureClass, IdentityFact, NodeEndpoint, P1363Signature, PAIRING_MAX_FAILURES,
    PAIRING_MAX_SECONDS, PROTOCOL_VERSION, PairingDecision, PairingDraft, PairingProof,
    PairingRequestId, PairingSecret, PairingSpec, PairingStatusView, PeerTrust,
    RequestedCapabilities, Sas,
};
