//! `agent-host` 的错误类型与到 [`acp_core::ports::PortError`] 的映射。
//!
//! 映射遵循 `docs/CORE_PORTS_AND_STORAGE.md` §2/§8：适配器错误必须先收敛成 core 的具名错误。
//! 错误消息只包含协议元数据（方法名、字段名、状态），不得包含 prompt 正文、凭据值或规范化路径。

use acp_core::model::{ConflictKind, PortError, UnavailableKind};

/// `agent-host` 内部错误。
#[derive(Debug, thiserror::Error)]
pub enum HostError {
    /// profile 未登记。
    #[error("agent profile 未登记")]
    UnknownProfile,

    /// Agent 进程未运行（尚未启动、已退出或已关闭）。
    #[error("Agent 进程未运行")]
    NotRunning,

    /// 启动失败（可执行文件不存在、权限不足等）。
    #[error("Agent 进程启动失败：{detail}")]
    SpawnFailed {
        /// 失败原因（不含环境变量值）。
        detail: String,
    },

    /// 请求超时。
    #[error("请求 {method} 超时")]
    Timeout {
        /// 方法名。
        method: String,
    },

    /// 进程在请求完成前退出。
    #[error("Agent 进程已退出（{status}）")]
    AgentExited {
        /// 退出状态的可读描述。
        status: String,
    },

    /// 协议层错误（信封、上限、方向、未实现的方法）。
    #[error(transparent)]
    Protocol(#[from] acp_protocol::AcpError),

    /// Agent 返回了明确的 JSON-RPC 错误。
    #[error("Agent 返回错误（{code}）：{message}")]
    AgentRejected {
        /// JSON-RPC 错误码。
        code: i64,
        /// 错误消息（对端给的，只用于日志与错误分类，不进入 wire 的其他字段）。
        message: String,
    },

    /// 能力未宣告，因此不得调用。
    #[error("Agent 未宣告能力 {capability}")]
    CapabilityNotDeclared {
        /// 能力路径。
        capability: String,
    },

    /// 同一会话重复建立。
    #[error("会话 {session} 已经有活动 endpoint")]
    DuplicateSession {
        /// 会话标识文本。
        session: String,
    },

    /// 引用不存在的会话。
    #[error("会话没有活动 endpoint")]
    UnknownSession,

    /// 会话已关闭。
    #[error("会话已关闭")]
    SessionClosed,

    /// 交互已被解析过（或不存在）。
    #[error("交互不存在或已有结论")]
    UnknownInteraction,

    /// 环境变量名不合法（例如含 `=` 或 NUL）。
    #[error("环境变量名不合法")]
    InvalidEnvName,

    /// 凭据解析返回了不在 profile 白名单里的变量名（白名单是上限）。
    #[error("凭据变量不在白名单内")]
    EnvNotAllowed {
        /// 变量名（不是值）。
        name: String,
    },

    /// 凭据解析失败。
    #[error("凭据解析失败")]
    CredentialUnavailable,

    /// 组合根注入的 id/时钟不可用（UUID 生成失败等）。
    #[error("标识分配失败")]
    IdUnavailable,

    /// 解析结果与交互类别不匹配（权限 ↔ elicitation 不能互答）。
    #[error("解析结果与交互类别不匹配")]
    InvalidResolution,

    /// prompt 载荷不合法（内容块数量越界、内容块不是 JSON 对象）。
    #[error("prompt 载荷不合法")]
    InvalidPrompt,

    /// 请求还处于未完成状态（关闭时统一收敛）。
    #[error("关闭时未完成的请求")]
    ShutdownPending,
}

impl HostError {
    /// 映射成 core 的端口错误。
    #[must_use]
    pub fn to_port_error(&self) -> PortError {
        match self {
            Self::UnknownProfile => PortError::InvalidRequest("agent profile is not registered"),
            Self::DuplicateSession { .. } => PortError::Conflict(ConflictKind::AlreadyExists),
            Self::UnknownSession => PortError::InvalidRequest("no endpoint for this session"),
            Self::SessionClosed => PortError::InvalidRequest("session endpoint is closed"),
            Self::CredentialUnavailable => {
                PortError::Unavailable(UnavailableKind::KeystoreUnavailable)
            }
            Self::NotRunning | Self::AgentExited { .. } => {
                PortError::Unavailable(UnavailableKind::IoError)
            }
            Self::SpawnFailed { .. } => PortError::Unavailable(UnavailableKind::IoError),
            Self::Timeout { .. } => PortError::Unavailable(UnavailableKind::Busy),
            Self::CapabilityNotDeclared { .. } => {
                PortError::InvalidRequest("capability not declared")
            }
            Self::UnknownInteraction => PortError::Conflict(ConflictKind::AlreadyResolved),
            Self::Protocol(_) | Self::AgentRejected { .. } | Self::ShutdownPending => {
                PortError::InvalidRequest("agent protocol error")
            }
            Self::InvalidEnvName => PortError::InvalidRequest("invalid environment variable name"),
            Self::EnvNotAllowed { .. } => {
                PortError::InvalidRequest("credential variable is not allow-listed")
            }
            Self::InvalidResolution | Self::InvalidPrompt => {
                PortError::InvalidRequest("invalid agent-host request")
            }
            Self::IdUnavailable => PortError::Unavailable(UnavailableKind::IoError),
        }
    }
}

impl From<HostError> for PortError {
    fn from(error: HostError) -> Self {
        error.to_port_error()
    }
}
