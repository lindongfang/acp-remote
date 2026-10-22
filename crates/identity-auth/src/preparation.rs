//! 准备凭据（design D2）：专用 domain 的**纯**签发/验证边界与绑定校验。
//!
//! 准备凭据（receipt）证明「某个 `sessionId` 由本节点分配给这台设备、这个标签页、这个 Agent 与目录」。
//! 它**不是** bearer 登录凭证：本模块没有任何「只凭 receipt 就得到认证事实」的入口，也没有任何
//! 授权/scope 输出。验证入口要求调用方提供当前连接由 challenge-response 得到的 [`Authenticated`]，
//! 以及最新持久设备记录里的凭据状态；`identity-auth` 自身不读存储、不读时钟、不碰网络。
//!
//! 检查顺序（与 design D3.1 的前两段一致，固定不因调用方式变化）：
//!
//! 1. **当前连接认证与撤销**：必须是设备连接；`Active` 之外的凭据状态立即失败（撤销/未知/缩减）。
//! 2. **凭据签名**：按登记表重装配 `acp-remote/session-preparation/v1` transcript，用本节点身份公钥
//!    验 P1363 签名。签名失败与结构失败是不同分类。
//! 3. **claims 绑定**：节点、设备、连接 origin、持久 `storeEpoch`、会话、标签页、Agent、目录路径与
//!    目录来源逐项比较；`storeEpoch` 不一致是独立的 [`PreparationError::StoreRebuilt`]（调用方映射
//!    `preparation.store_rebuilt`，不可重试），不是 `preparation.stale`。
//!
//! **调用方契约**：第三段通过后返回 [`PreparationAdmission`]；它只能由本模块构造（字段私有），因此
//! 资源查询/重建/启动入口应接收 `&PreparationAdmission`，在拿到它之前不得读取会话、首条受理记录或
//! 临时资源。验签公钥 MUST 是本节点身份密钥的公钥（取自同一 keystore 条目），MUST NOT 使用对端在
//! 消息里提供的任何公钥。

use std::fmt;

use acp_core::model::{AgentId, NodeId, Nonce, PeerPublicKey, ServerEpoch, SessionId};

use crate::error::{ProofError, TranscriptError};
use crate::port::KeystoreError;
use crate::transcript::{PreparationClaims, verify_signature};
use crate::types::{
    Authenticated, CredentialStatus, IdentityFact, NormalizedWorkspacePath, P1363Signature,
    PreparationWorkspaceSource,
};

/// 验证准备凭据时，调用方**即将使用**的绑定值。
///
/// `restore` 这类请求只携带 receipt 与 `sessionId`；此时调用方从 receipt 的 claims 复制
/// `agentId`/目录/`tabNonce`，但只有在 [`verify_preparation_receipt`] 通过后才能据此读取或重建资源。
/// `configure`/`renew`/`release` 与附件重绑应传入当前运行期记录里的值，从而把「本次操作使用的值」
/// 与签名覆盖的 claims 钉在一起。`storeEpoch` MUST 是持久 `meta.server_epoch` 的当前值，不是进程
/// epoch；`owner_node_id` MUST 是本节点标识。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparationExpectation {
    /// 本节点标识（claims 的 `ownerNodeId` 必须等于它）。
    pub owner_node_id: NodeId,
    /// 持久 `meta.server_epoch`（正常重启不变；整体重建存储时改变）。
    pub store_epoch: ServerEpoch,
    /// 本次请求指向的会话标识。
    pub session_id: SessionId,
    /// 标签页归属（运行期记录里的值；`restore` 重建时可取自 receipt）。
    pub tab_nonce: Nonce,
    /// 当前准备资源使用的 Agent 选择。
    pub agent_id: AgentId,
    /// 规范化后的工作目录（精确字符串比较）。
    pub normalized_workspace_path: NormalizedWorkspacePath,
    /// 目录来源（只说明路径由哪条输入得出）。
    pub workspace_source: PreparationWorkspaceSource,
}

/// 绑定比较失败的**本地**分类。
///
/// 细分只服务本地决策、日志分类与测试；对端可见错误 MUST 由 adapter 折叠（附件帧用
/// `attachment.binding_mismatch`，控制面用已登记的业务错误），且 MUST NOT 回显本枚举或任何 claims 值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PreparationBinding {
    /// `ownerNodeId` 不是本节点。
    OwnerNode,
    /// connections/claims 的 canonical origin 不一致。
    Origin,
    /// claims 的 `deviceId` 与当前已认证设备不一致（跨设备使用）。
    Device,
    /// `tabNonce` 与当前标签页记录不一致（跨标签页使用）。
    TabNonce,
    /// `sessionId` 与请求/运行期记录不一致。
    Session,
    /// `agentId` 与当前准备资源不一致。
    Agent,
    /// `normalizedWorkspacePath` 与当前目录不一致。
    WorkspacePath,
    /// `workspaceSource` 与当前目录来源不一致。
    WorkspaceSource,
}

impl PreparationBinding {
    /// 稳定 token（日志分类与测试断言用；不是 wire 字段）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OwnerNode => "owner_node",
            Self::Origin => "origin",
            Self::Device => "device",
            Self::TabNonce => "tab_nonce",
            Self::Session => "session",
            Self::Agent => "agent",
            Self::WorkspacePath => "workspace_path",
            Self::WorkspaceSource => "workspace_source",
        }
    }
}

impl fmt::Display for PreparationBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 准备凭据的签发/验证失败分类。
///
/// 三类必须可分：**结构/编码**、**密码学**、**绑定**。`Display`/`Debug` 只带固定文案与字段类别，
/// 不带 claims 值（路径、Agent、origin）、签名内容或任何密钥材料。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PreparationError {
    /// claims 无法按登记表装配，或签名不是规范无填充 base64url / 不是 64 字节 P1363。
    #[error("准备凭据编码或结构非法：{0}")]
    InvalidEncoding(#[from] TranscriptError),
    /// 签名不是该 claims 在本节点密钥下的有效 P-256 P1363 签名（含跨 domain 重放）。
    #[error("准备凭据签名校验失败")]
    Signature,
    /// 本节点公钥不是合法的 P-256 未压缩点（身份材料损坏）。
    #[error("本节点公钥不是合法的 P-256 未压缩点")]
    PublicKey,
    /// 当前连接没有已认证的设备身份（receipt 不能替代逐连接认证）。
    #[error("当前连接没有已认证的设备身份")]
    Unauthenticated,
    /// 设备已撤销。
    #[error("设备已撤销")]
    DeviceRevoked,
    /// 设备未知（没有持久信任记录）。
    #[error("设备未知")]
    DeviceUnknown,
    /// 凭据状态要求重新认证（范围缩减）。
    #[error("设备凭据已缩减，必须重新认证")]
    ReauthenticationRequired,
    /// claims 与当前连接/持久/运行期事实不一致。
    #[error("准备凭据绑定不一致：{0}")]
    Binding(PreparationBinding),
    /// claims 的 `storeEpoch` 与当前持久值不一致：存储已整体重建，旧凭据统一失效。
    ///
    /// 调用方 MUST 映射为 `preparation.store_rebuilt`（不可重试），MUST NOT 归入
    /// `preparation.stale`，也 MUST NOT 拿旧 receipt 重试。
    #[error("存储已整体重建，准备凭据失效")]
    StoreRebuilt,
    /// keystore 不可用（签名入口）。
    #[error("keystore 不可用：{0}")]
    Keystore(#[from] KeystoreError),
}

/// 节点签发的准备凭据：闭合 claims + 64 字节 P1363 签名（wire 上为无填充 base64url）。
///
/// 本类型**不含**任何秘密：没有私钥、认证材料、正文、附件或 Provider/MCP 凭据。但 claims 携带本机
/// 规范化目录、origin 与设备/会话标识，签名也是可传递的证明材料，因此 `Debug` **不打印任何字段值**，
/// 只输出固定脱敏标记；需要 claims 时经 [`PreparationReceipt::claims`] 显式读取，并由调用方决定
/// 是否/如何记录（路径仍禁入日志）。
#[derive(Clone, PartialEq, Eq)]
pub struct PreparationReceipt {
    claims: PreparationClaims,
    signature: P1363Signature,
}

impl fmt::Debug for PreparationReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PreparationReceipt { claims: <redacted>, signature: <redacted> }")
    }
}

impl PreparationReceipt {
    /// 从已解析的 claims 与签名构造（签名仍由 [`verify_preparation_receipt`] 校验）。
    pub fn new(claims: PreparationClaims, signature: P1363Signature) -> Self {
        Self { claims, signature }
    }

    /// 从已解析 claims 与规范无填充 base64url 签名构造。
    ///
    /// 带填充、非字母表字符、DER 或非 64 字节一律返回 [`PreparationError::InvalidEncoding`]；
    /// 这不是密码学校验，签名是否匹配 claims 仍由验证入口判定。
    pub fn from_parts(
        claims: PreparationClaims,
        signature_base64url: &str,
    ) -> Result<Self, PreparationError> {
        let signature = P1363Signature::try_from_base64url(signature_base64url)
            .map_err(PreparationError::InvalidEncoding)?;
        Ok(Self::new(claims, signature))
    }

    /// 闭合 claims。
    pub fn claims(&self) -> &PreparationClaims {
        &self.claims
    }

    /// 64 字节 P1363 签名。
    pub fn signature(&self) -> &P1363Signature {
        &self.signature
    }

    /// 签名在 wire 上的规范无填充 base64url（86 字符）。
    pub fn signature_base64url(&self) -> String {
        self.signature.to_base64url()
    }
}

/// 认证与绑定校验通过的记号（design D3.1 的资源读取门槛）。
///
/// 字段私有：只有本模块的 [`verify_preparation_receipt`] 能构造它，因此「未校验就读取资源」在类型
/// 层上不需要额外的布尔开关。它只携带已验证的会话标识，**不携带**名称、授权、scope、端点、密钥或
/// 任何可代替连接认证的事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparationAdmission {
    session_id: SessionId,
}

impl PreparationAdmission {
    /// 已验证的会话标识（claims 与调用方期望一致的那个）。
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }
}

/// 验证准备凭据并返回资源读取门槛。
///
/// 顺序固定为：当前连接认证/撤销 → 签名（含 domain 分离）→ claims 绑定。**认证与撤销先于一切**：
/// 即使 receipt 的签名损坏，已撤销设备也只得到 [`PreparationError::DeviceRevoked`]，不会得到密码学
/// 分类，也不会触发任何资源读取。
///
/// `owner_public_key` MUST 是本节点身份密钥的公钥（与签发使用同一 keystore 条目），MUST NOT 来自
/// 对端消息。`authenticated` MUST 是当前连接 challenge-response 的结果；`credential` 字段 MUST 由
/// 调用方在调用前刷新为最新持久设备记录的状态（本 crate 不读存储）。
pub fn verify_preparation_receipt(
    authenticated: &Authenticated,
    owner_public_key: &PeerPublicKey,
    receipt: &PreparationReceipt,
    expected: &PreparationExpectation,
) -> Result<PreparationAdmission, PreparationError> {
    // 1. 当前连接认证与撤销：receipt 不是 bearer 凭证，不能自己证明任何身份。
    let IdentityFact::Device { device } = &authenticated.fact else {
        return Err(PreparationError::Unauthenticated);
    };
    match authenticated.credential {
        CredentialStatus::Active => {}
        CredentialStatus::Revoked => return Err(PreparationError::DeviceRevoked),
        CredentialStatus::Unknown => return Err(PreparationError::DeviceUnknown),
        CredentialStatus::ScopeReduced => return Err(PreparationError::ReauthenticationRequired),
    }

    let claims = receipt.claims();

    // 2. 专用 domain 的签名：未验签的 claims 不参与任何绑定判断。
    let transcript = claims.transcript()?;
    match verify_signature(owner_public_key, &transcript, receipt.signature()) {
        Ok(()) => {}
        Err(ProofError::Signature | ProofError::Hmac) => return Err(PreparationError::Signature),
        Err(ProofError::PublicKey) => return Err(PreparationError::PublicKey),
        Err(ProofError::Transcript(error)) => return Err(PreparationError::InvalidEncoding(error)),
    }

    // 3. claims 绑定。设备与 origin 先于 storeEpoch：跨设备/跨 origin 的使用不因存储重建而改变分类。
    if claims.owner_node_id != expected.owner_node_id {
        return Err(PreparationError::Binding(PreparationBinding::OwnerNode));
    }
    if claims.device_id != *device {
        return Err(PreparationError::Binding(PreparationBinding::Device));
    }
    let Some(origin) = authenticated.connection.canonical_origin.as_ref() else {
        return Err(PreparationError::Binding(PreparationBinding::Origin));
    };
    if claims.origin != *origin {
        return Err(PreparationError::Binding(PreparationBinding::Origin));
    }
    if claims.store_epoch != expected.store_epoch {
        return Err(PreparationError::StoreRebuilt);
    }
    if claims.session_id != expected.session_id {
        return Err(PreparationError::Binding(PreparationBinding::Session));
    }
    if claims.tab_nonce != expected.tab_nonce {
        return Err(PreparationError::Binding(PreparationBinding::TabNonce));
    }
    if claims.agent_id != expected.agent_id {
        return Err(PreparationError::Binding(PreparationBinding::Agent));
    }
    if claims.normalized_workspace_path != expected.normalized_workspace_path {
        return Err(PreparationError::Binding(PreparationBinding::WorkspacePath));
    }
    if claims.workspace_source != expected.workspace_source {
        return Err(PreparationError::Binding(
            PreparationBinding::WorkspaceSource,
        ));
    }

    Ok(PreparationAdmission {
        session_id: claims.session_id.clone(),
    })
}
