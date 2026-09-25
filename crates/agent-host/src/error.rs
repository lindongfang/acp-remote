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

#[cfg(test)]
mod tests {
    use super::*;

    /// 端口错误的**形状**：只保留调用方据以决策的类别与 kind，丢掉诊断消息文本。
    ///
    /// 消息文案是诊断细节（`design.md` 决策 3），钉死它会让改文案变成破坏测试；类别与
    /// `ConflictKind`/`UnavailableKind` 才是调用方（`server::*`）据以分支的端口合同。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Shape {
        NotFound,
        Conflict(ConflictKind),
        InvalidRequest,
        Unavailable(UnavailableKind),
        Corrupt,
        Backend,
    }

    fn shape(error: &PortError) -> Shape {
        match error {
            PortError::NotFound(_) => Shape::NotFound,
            PortError::Conflict(kind) => Shape::Conflict(*kind),
            PortError::InvalidRequest(_) => Shape::InvalidRequest,
            PortError::Unavailable(kind) => Shape::Unavailable(*kind),
            PortError::Corrupt(_) => Shape::Corrupt,
            PortError::Backend(_) => Shape::Backend,
        }
    }

    /// 逐变体钉死映射表。
    ///
    /// `HostError` 目前有 19 个变体（`tasks.md`/`design.md` 里写的「16 个」是更早版本的计数）；
    /// **新增变体时必须在这里补一行**——映射是 core 端口合同的一部分，合法调整应显式发生，
    /// 而不是被静默改坏。
    #[test]
    fn to_port_error_is_pinned_per_variant() {
        let cases: [(&str, HostError, Shape); 19] = [
            (
                "UnknownProfile",
                HostError::UnknownProfile,
                Shape::InvalidRequest,
            ),
            (
                "NotRunning",
                HostError::NotRunning,
                Shape::Unavailable(UnavailableKind::IoError),
            ),
            (
                "SpawnFailed",
                HostError::SpawnFailed {
                    detail: "找不到可执行文件".to_owned(),
                },
                Shape::Unavailable(UnavailableKind::IoError),
            ),
            (
                "Timeout",
                HostError::Timeout {
                    method: "initialize".to_owned(),
                },
                Shape::Unavailable(UnavailableKind::Busy),
            ),
            (
                "AgentExited",
                HostError::AgentExited {
                    status: "exit code 3".to_owned(),
                },
                Shape::Unavailable(UnavailableKind::IoError),
            ),
            (
                "Protocol",
                HostError::Protocol(acp_protocol::AcpError::Json {
                    detail: "测试用协议错误".to_owned(),
                }),
                Shape::InvalidRequest,
            ),
            (
                "AgentRejected",
                HostError::AgentRejected {
                    code: -32601,
                    message: "method not found".to_owned(),
                },
                Shape::InvalidRequest,
            ),
            (
                "CapabilityNotDeclared",
                HostError::CapabilityNotDeclared {
                    capability: "session.template".to_owned(),
                },
                Shape::InvalidRequest,
            ),
            (
                "DuplicateSession",
                HostError::DuplicateSession {
                    session: "SESSION".to_owned(),
                },
                Shape::Conflict(ConflictKind::AlreadyExists),
            ),
            (
                "UnknownSession",
                HostError::UnknownSession,
                Shape::InvalidRequest,
            ),
            (
                "SessionClosed",
                HostError::SessionClosed,
                Shape::InvalidRequest,
            ),
            (
                "UnknownInteraction",
                HostError::UnknownInteraction,
                Shape::Conflict(ConflictKind::AlreadyResolved),
            ),
            (
                "InvalidEnvName",
                HostError::InvalidEnvName,
                Shape::InvalidRequest,
            ),
            (
                "EnvNotAllowed",
                HostError::EnvNotAllowed {
                    name: "ACPR_NODE_KEY".to_owned(),
                },
                Shape::InvalidRequest,
            ),
            (
                "CredentialUnavailable",
                HostError::CredentialUnavailable,
                Shape::Unavailable(UnavailableKind::KeystoreUnavailable),
            ),
            (
                "IdUnavailable",
                HostError::IdUnavailable,
                Shape::Unavailable(UnavailableKind::IoError),
            ),
            (
                "InvalidResolution",
                HostError::InvalidResolution,
                Shape::InvalidRequest,
            ),
            (
                "InvalidPrompt",
                HostError::InvalidPrompt,
                Shape::InvalidRequest,
            ),
            (
                "ShutdownPending",
                HostError::ShutdownPending,
                Shape::InvalidRequest,
            ),
        ];

        // 变体标签不得重复（重复行会把「逐变体覆盖」悄悄变成少测一行）。
        let labels: std::collections::BTreeSet<&str> =
            cases.iter().map(|(label, _, _)| *label).collect();
        assert_eq!(labels.len(), cases.len(), "变体标签不得重复");

        for (label, error, expected) in cases {
            assert_eq!(
                shape(&error.to_port_error()),
                expected,
                "变体 {label} 的端口错误映射与固定承诺不符"
            );
            // `From<HostError> for PortError` 必须与 `to_port_error()` 同源，不得各写一套映射。
            assert_eq!(
                shape(&PortError::from(error)),
                expected,
                "变体 {label} 的 `From` 映射必须与 `to_port_error()` 一致"
            );
        }
    }
}
