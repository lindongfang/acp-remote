//! 由组合根注入的配置。
//!
//! 这一层刻意很薄：`agent-host` 不读启动配置文件（`docs/MODULE_ARCHITECTURE.md` §4.5），
//! profile 来自 [`acp_core::ports::LocalConfigStore`]，而空闲回收与客户端能力声明由组合根决定后注入。

use std::path::PathBuf;
use std::time::Duration;

use acp_protocol::capability::Implementation;

/// 客户端能力声明的公开入口（组合根构造 [`HostConfig::client_capabilities`] 时不需要直接依赖
/// `acp-protocol`）。
pub use acp_protocol::capability::ClientCapabilities;

/// `agent-host` 的运行配置。
#[derive(Debug, Clone, Default)]
pub struct HostConfig {
    /// `initialize` 里宣告的客户端能力。
    ///
    /// 默认是 [`ClientCapabilities::none`]：本阶段没有可供用户作答的上游客户端，宣告 elicitation/fs/terminal
    /// 就是虚报。组合根只有在端到端确实具备时才把它打开。
    pub client_capabilities: ClientCapabilities,
    /// 客户端信息（可省略）。
    pub client_info: Option<Implementation>,
    /// 会话空闲超时；`None` 或零表示**不因空闲关闭**（`sessions.idle_timeout_ms = 0`）。
    pub idle_timeout: Option<Duration>,
    /// 完整关闭证明文件。组合根注入固定内部路径；`None` 表示不生成或消费证明。
    ///
    /// 该文件不是会话数据或权威状态，只证明上一次 AgentHost 已完整停止其受管进程。启动时读取并
    /// 原子消费；缺失、损坏或删除失败都不足以信任“没有 descriptor 等于已释放”。
    pub clean_shutdown_fence: Option<PathBuf>,
}

impl HostConfig {
    /// 空闲回收是否启用。
    #[must_use]
    pub fn idle_timeout(&self) -> Option<Duration> {
        self.idle_timeout.filter(|value| !value.is_zero())
    }
}
