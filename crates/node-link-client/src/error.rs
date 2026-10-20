//! `node-link-client` 的错误边界：外部错误在适配器边界收敛成 [`ClientError`]，
//! 再由 [`ClientError::to_port_error`] 映射成 core 的 [`PortError`]（`docs/MODULE_ARCHITECTURE.md` §8）。
//!
//! 两条纪律：
//!
//! - 错误消息只带**协议元数据**（错误码、命令名、连接阶段），不带正文、凭据、pairing secret 或完整路径；
//! - 「Owner 不可达」「能力未协商」「命令结果不确定」是**三种不同事实**，不合并成一个泛化失败——
//!   core 与本地客户端据此分别呈现。`uncertain` 不是错误值，它由命令结果类型表达（见
//!   [`crate::remote_session::CommandOutcome`]），但连接层仍需把「发出后断线」标成
//!   [`ClientError::Uncertain`]，供调用方决定是否显式重查而不是重做。

use acp_core::model::{PortError, PublicError, UnavailableKind};

/// 适配器错误。
#[derive(Debug, Clone, thiserror::Error)]
pub enum ClientError {
    /// 连接层失败（TCP/TLS/WebSocket 读写）。消息只含操作系统级原因，不含正文。
    #[error("node-link transport failure: {0}")]
    Transport(String),
    /// 等待对端回复超时。
    #[error("node-link operation timed out")]
    Timeout,
    /// 连接已关闭（正常关闭、撤销或对端断开）。
    #[error("node-link connection is closed")]
    Closed,
    /// 对端撤回信任（`node.trust.revoked`）或本机停止重连。
    #[error("the owner node revoked this access node")]
    Revoked,
    /// Owner 不在线或连接尚未建立：明确不可用，不返回空结果。
    #[error("the owner node is not reachable")]
    OwnerOffline,
    /// 握手失败（版本/feature 无交集、证明无效、角色不符）。
    #[error("node-link handshake failed: {0}")]
    Handshake(String),
    /// wire 层拒绝（信封、schema、序号或阶段不符）。
    #[error("node-link protocol violation: {0}")]
    Protocol(String),
    /// 本机响应无法装配（编码、id 生成或存储写入）。
    #[error("node-link client cannot complete the request: {0}")]
    Local(&'static str),
    /// Owner 对命令给出结构化拒绝（`command.rejected`）或终态失败。
    ///
    /// `PublicError` 是 wire 上的结构化错误，原样保留（错误码的唯一来源是
    /// `compatibility/errors/v1/errors.json`）。
    #[error("the owner rejected the command: {code}")]
    Rejected {
        /// wire 错误码。
        code: String,
        /// 可重试标记。
        retryable: bool,
        /// 结构化错误本体。
        error: PublicError,
    },
    /// 命令已发出但结果无法确认（断线发生在派发之后、终态之前）。
    ///
    /// **不得**据此自动重发副作用命令；调用方必须重新查询（`command.status`）或向用户显示不确定。
    #[error("the command result is uncertain")]
    Uncertain,
    /// 对端未协商所需能力（或本机已知对端不会接受该命令）。
    #[error("the owner does not support this operation")]
    Unsupported,
    /// 附件传输被中途取消（撤销、路由失效或本机放弃）。
    #[error("the attachment transfer was aborted")]
    TransferAborted,
    /// 附件传输超出双方约定的预算或校验失败。
    #[error("the attachment transfer failed: {0}")]
    Transfer(&'static str),
    /// 配对邀请或 claim/status 失败（二维码形状、过期、身份不一致、Owner 拒绝）。
    ///
    /// 消息只含协议元数据（HTTP 状态与 `code`），不含 pairing secret、完整 URL 或响应正文。
    #[error("node-link pairing failed: {0}")]
    Pairing(String),
}

impl ClientError {
    /// 映射到 core 端口错误。
    ///
    /// 映射口径（`CORE_PORTS_AND_STORAGE.md` §5 的 `UnavailableKind` 语义）：
    ///
    /// - 不可达 / 撤销 → `RemoteUnavailable` / `OwnerOffline`：调用方据此显式降级，**不**返回空历史；
    /// - 未协商 / 不支持的命令 → `BackendUnsupported`；
    /// - 协议或本机装配失败 → `InvalidRequest`/`Backend`；
    /// - `uncertain` → `Unavailable(RemoteUnavailable)`：它同样是「现在不能确认」，绝不能当成成功。
    pub fn to_port_error(&self) -> PortError {
        match self {
            Self::Transport(_) | Self::Timeout | Self::Closed => {
                PortError::Unavailable(UnavailableKind::RemoteUnavailable)
            }
            Self::OwnerOffline => PortError::Unavailable(UnavailableKind::OwnerOffline),
            Self::Revoked => PortError::Unavailable(UnavailableKind::OwnerOffline),
            Self::Handshake(reason) | Self::Protocol(reason) => PortError::Backend(Box::new(
                std::io::Error::other(format!("node-link client: {reason}")),
            )),
            Self::Local(reason) => PortError::InvalidRequest(reason),
            Self::Rejected { error, .. } => {
                PortError::Backend(Box::new(RejectedError(error.clone())))
            }
            Self::Uncertain => PortError::Unavailable(UnavailableKind::RemoteUnavailable),
            Self::Unsupported => PortError::Unavailable(UnavailableKind::BackendUnsupported),
            Self::TransferAborted | Self::Transfer(_) => {
                PortError::Unavailable(UnavailableKind::RemoteUnavailable)
            }
            // 配对失败只在本地管理调用面上呈现（`AccessPairing` 直接读 `ClientError`）；映射到端口错误时
            // 只保留静态分类，不把动态原因伪装成 `'static` 文本。
            Self::Pairing(_) => PortError::InvalidRequest("node-link pairing failed"),
        }
    }

    /// 该错误是否表示「现在联系不上 Owner」（而不是对端明确拒绝）。
    pub fn is_offline(&self) -> bool {
        matches!(
            self,
            Self::Transport(_) | Self::Timeout | Self::Closed | Self::OwnerOffline | Self::Revoked
        )
    }
}

/// `PortError::Backend` 需要 `std::error::Error`；`PublicError` 是值对象（不实现 Error）。
#[derive(Debug)]
struct RejectedError(PublicError);

impl std::fmt::Display for RejectedError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0.code())
    }
}

impl std::error::Error for RejectedError {}

impl From<ClientError> for PortError {
    fn from(error: ClientError) -> Self {
        error.to_port_error()
    }
}
