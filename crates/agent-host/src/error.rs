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

    /// Agent 的**响应**无法解码成该方法预期的形状（请求已经发出、且已经收到答复）。
    ///
    /// 与 [`HostError::SpawnFailed`] 刻意分开：这类失败**不能**证明 Agent 侧没有产生会话——它恰恰说明
    /// Agent 收到了请求并答复了（`session/new` 的响应解不开时，会话可能已经在 Agent 侧建立）。
    #[error("Agent 响应无法解码（{method}）：{detail}")]
    MalformedResponse {
        /// 方法名（只是协议元数据，不含任何载荷）。
        method: &'static str,
        /// 解码失败原因（serde 的字段/类型描述，不含响应正文）。
        detail: String,
    },

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

    /// 组合根注入的 id/时钟不可用（UUID 生成失败等）。
    #[error("标识分配失败")]
    IdUnavailable,

    /// 解析结果与交互类别不匹配（权限 ↔ elicitation 不能互答）。
    #[error("解析结果与交互类别不匹配")]
    InvalidResolution,

    /// prompt 载荷不合法（内容块数量越界、内容块不是 JSON 对象）。
    #[error("prompt 载荷不合法")]
    InvalidPrompt,

    /// prompt 内容无法按 Agent 能力与 ACP 帧预算派发。
    ///
    /// 这是**派发前**的整条拒绝：没有残缺 prompt 被发出，也没有为超限内容读取附件字节。
    #[error("prompt 内容不可派发：{detail}")]
    PromptRejected {
        /// 拒绝原因（只含协议元数据，不含正文、路径或附件内容）。
        detail: String,
    },

    /// 请求还处于未完成状态（关闭时统一收敛）。
    #[error("关闭时未完成的请求")]
    ShutdownPending,

    /// 未知或已释放的终端标识（含跨会话访问）。
    #[error("未知终端")]
    UnknownTerminal,

    /// 终端任务无法创建或已不可操作（进程启动失败、活动终端数达到上限等）。
    ///
    /// `detail` 只用于诊断分类，**不含**命令行、参数、环境变量值或路径。
    #[error("终端任务不可用：{detail}")]
    TerminalRejected {
        /// 拒绝原因（协议元数据级别）。
        detail: String,
    },
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
            Self::NotRunning | Self::AgentExited { .. } => {
                PortError::Unavailable(UnavailableKind::IoError)
            }
            Self::SpawnFailed { .. } => PortError::Unavailable(UnavailableKind::IoError),
            Self::Timeout { .. } => PortError::Unavailable(UnavailableKind::Busy),
            Self::CapabilityNotDeclared { .. } => {
                PortError::Unavailable(UnavailableKind::BackendUnsupported)
            }
            Self::UnknownInteraction => PortError::Conflict(ConflictKind::AlreadyResolved),
            Self::Protocol(_)
            | Self::MalformedResponse { .. }
            | Self::AgentRejected { .. }
            | Self::ShutdownPending => PortError::InvalidRequest("agent protocol error"),
            Self::InvalidEnvName => PortError::InvalidRequest("invalid environment variable name"),
            Self::EnvNotAllowed { .. } => {
                PortError::InvalidRequest("reserved environment variable name")
            }
            Self::InvalidResolution | Self::InvalidPrompt | Self::PromptRejected { .. } => {
                PortError::InvalidRequest("invalid agent-host request")
            }
            Self::UnknownTerminal | Self::TerminalRejected { .. } => {
                PortError::InvalidRequest("terminal task is not available")
            }
            Self::IdUnavailable => PortError::Unavailable(UnavailableKind::IoError),
        }
    }

    /// 本次失败是否**可证明** Agent 侧没有产生会话（`session.create`/`session.resume` 的终态判据，
    /// `sync-session-create` 的「可证明 / 说不清」、`local-agent-host` 的「可证明与无法证明可区分」）。
    ///
    /// 判据是「这次失败说明请求走到哪一步了」：
    ///
    /// - **可证明**（`true`）：进程根本没起来（profile / 凭据 / 环境变量 / spawn 失败），
    ///   或请求**没有发出去**（能力未宣告时拒绝发送、进程不运行时拒绝发送），或 Agent 对请求给出了
    ///   **明确的错误答复**（`session/new` 返回 JSON-RPC 错误）。三者都不可能让 Agent 侧留下会话。
    ///   `ShutdownPending` **不在**这一类：它只投给**已登记**的未完成请求，而出站队列里的文档还可能被
    ///   写出去，因此它不能证明请求未发出（见 `process.rs` 的先登记后写出）。
    /// - **说不清**（`false`）：请求已经发出之后才失败（短请求超时、进程退出、连接断开、协议破坏、
    ///   响应解不开），或会话其实已经建立（映射已存在、会话已关闭），或本机自造的标识失败。
    ///   **默认方向是"说不清"**：宁可让客户端看到 `uncertain`，也不声称「什么也没发生」。
    #[must_use]
    pub const fn proves_no_session(&self) -> bool {
        match self {
            Self::UnknownProfile
            | Self::SpawnFailed { .. }
            | Self::NotRunning
            | Self::CapabilityNotDeclared { .. }
            | Self::AgentRejected { .. }
            | Self::EnvNotAllowed { .. }
            | Self::InvalidEnvName => true,
            Self::Timeout { .. }
            | Self::AgentExited { .. }
            | Self::Protocol(_)
            | Self::MalformedResponse { .. }
            | Self::DuplicateSession { .. }
            | Self::UnknownSession
            | Self::SessionClosed
            | Self::UnknownInteraction
            | Self::InvalidResolution
            | Self::InvalidPrompt
            | Self::PromptRejected { .. }
            | Self::UnknownTerminal
            | Self::TerminalRejected { .. }
            | Self::ShutdownPending
            | Self::IdUnavailable => false,
        }
    }

    /// **会话建立**（`SessionBackendFactory::create`/`resume`）路径的端口映射：可证明未产生会话的一类
    /// 收敛成 [`UnavailableKind::SessionNotEstablished`]，让 core 能把终态判为 `failed`（其余一律
    /// `uncertain`）；说不清的一类沿用 [`HostError::to_port_error`] 的既有映射。
    ///
    /// 为什么需要这条**按操作**的映射：同一个变体在不同操作里的含义不同——`AgentRejected` 对
    /// `session/new` 是「明确拒绝 ⇒ 可证明」，对 `session/prompt` 则是「Agent 拒绝了这一轮」。因此
    /// 分类只在这两个建立入口上生效，不改变其它操作错误映射的语义。
    #[must_use]
    pub fn to_establishment_error(&self) -> PortError {
        if self.proves_no_session() {
            PortError::Unavailable(UnavailableKind::SessionNotEstablished)
        } else {
            self.to_port_error()
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
    /// `HostError` 目前有 22 个变体（`tasks.md`/`design.md` 里写的「16 个」是更早版本的计数）；
    /// **新增变体时必须在这里补一行**——映射是 core 端口合同的一部分，合法调整应显式发生，
    /// 而不是被静默改坏。
    ///
    /// 这里钉的是**通用**映射（[`HostError::to_port_error`]）。会话建立路径用的是
    /// [`HostError::to_establishment_error`]，它的分类由
    /// `session_establishment_failures_are_classified` 单独钉死。
    #[test]
    fn to_port_error_is_pinned_per_variant() {
        let cases: [(&str, HostError, Shape); 22] = [
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
                "MalformedResponse",
                HostError::MalformedResponse {
                    method: "session/new",
                    detail: "缺少 sessionId".to_owned(),
                },
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
                Shape::Unavailable(UnavailableKind::BackendUnsupported),
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
                "PromptRejected",
                HostError::PromptRejected {
                    detail: "prompt 超出 ACP 单条消息预算".to_owned(),
                },
                Shape::InvalidRequest,
            ),
            (
                "ShutdownPending",
                HostError::ShutdownPending,
                Shape::InvalidRequest,
            ),
            (
                "UnknownTerminal",
                HostError::UnknownTerminal,
                Shape::InvalidRequest,
            ),
            (
                "TerminalRejected",
                HostError::TerminalRejected {
                    detail: "活动终端数已达上限".to_owned(),
                },
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

    /// 会话建立（`create`/`resume`）失败的**两档分类**：可证明 Agent 侧未产生会话 vs 说不清。
    ///
    /// 这是 `sync-session-create` 的终态判据（`failed` vs `uncertain`）在 `agent-host` 边界的唯一来源，
    /// 因此在这里逐类钉死；两档必须可区分，且「说不清」的一类必须保持既有的通用映射（core 侧默认按
    /// `uncertain` 处理）。
    #[test]
    fn session_establishment_failures_are_classified() {
        let provable: [(&str, HostError); 7] = [
            ("UnknownProfile", HostError::UnknownProfile),
            (
                "SpawnFailed",
                HostError::SpawnFailed {
                    detail: "找不到可执行文件".to_owned(),
                },
            ),
            ("NotRunning", HostError::NotRunning),
            (
                "CapabilityNotDeclared",
                HostError::CapabilityNotDeclared {
                    capability: "session/resume".to_owned(),
                },
            ),
            (
                "AgentRejected",
                HostError::AgentRejected {
                    code: -32000,
                    message: "cannot create session".to_owned(),
                },
            ),
            (
                "EnvNotAllowed",
                HostError::EnvNotAllowed {
                    name: "ACPR_NODE_KEY".to_owned(),
                },
            ),
            ("InvalidEnvName", HostError::InvalidEnvName),
        ];
        for (label, error) in provable {
            assert!(error.proves_no_session(), "{label} 必须可证明未产生会话");
            assert_eq!(
                shape(&error.to_establishment_error()),
                Shape::Unavailable(UnavailableKind::SessionNotEstablished),
                "{label} 在建立路径上必须收敛成 SessionNotEstablished"
            );
        }

        // 说不清的一类：短请求超时、请求发出后掉线、协议破坏、响应解不开、会话其实已建立。
        let unprovable: [(&str, HostError); 5] = [
            (
                "Timeout",
                HostError::Timeout {
                    method: "session/new".to_owned(),
                },
            ),
            (
                "AgentExited",
                HostError::AgentExited {
                    status: "exit code 1".to_owned(),
                },
            ),
            (
                "Protocol",
                HostError::Protocol(acp_protocol::AcpError::Json {
                    detail: "测试用协议错误".to_owned(),
                }),
            ),
            (
                "MalformedResponse",
                HostError::MalformedResponse {
                    method: "session/new",
                    detail: "缺少 sessionId".to_owned(),
                },
            ),
            (
                "DuplicateSession",
                HostError::DuplicateSession {
                    session: "SESSION".to_owned(),
                },
            ),
        ];
        for (label, error) in unprovable {
            assert!(!error.proves_no_session(), "{label} 必须归入「说不清」");
            assert_eq!(
                shape(&error.to_establishment_error()),
                shape(&error.to_port_error()),
                "{label} 在建立路径上必须沿用通用映射（不得谎报可证明）"
            );
        }
    }
}
