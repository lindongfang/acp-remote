//! 本机 ACP Agent 的宿主：进程监督、会话端点与目录装配。
//!
//! 本 crate 是 `core` 的出站适配器（`docs/MODULE_ARCHITECTURE.md` §4.5），只做四件事：
//!
//! 1. **进程监督**（[`process::Supervisor`]）：以数组参数直接 spawn（不经 shell）、stdio 管道、
//!    LF 分隔 JSON 分帧、request id 单调分配与 pending 注册表、固定超时、stderr 有界环形缓冲，
//!    以及「停止接受新请求 → 未完成请求收敛 → 关 stdin → grace → 结束整棵进程树 → join 任务」的关闭顺序。
//! 2. **会话映射**（[`session::AcpSession`]）：把 ACP 通知与请求映射成 core 的
//!    [`acp_core::model::EndpointEvent`]，把 turn/交互的终态收敛成**唯一**事件，并保持原始 request id 的类型与
//!    字面量。
//! 3. **目录与能力**（[`host::AgentHost`]）：`AgentCatalog::agents` 只做只读探测（**不启动进程**），
//!    而 `AgentCatalog::agent_capabilities` 按设计需要真实 `initialize` 协商，因此会启动（或复用）进程，
//!    协商结果按进程代缓存；未宣告的能力显式不支持。
//! 4. **单会话释放**（[`host::AgentHost::release_session`]）：能力门控的 ACP `session/close`，只有 Agent
//!    ack 后才摘除映射并报告 [`ReleaseOutcome`]；失败/超时保留真实资源与容量名额，且绝不终止共享进程。
//!    本地已 closed 但未获 Agent 确认的绑定（重绑/进程级让出）同样按未释放处理，不交还名额。
//!
//! 边界纪律：
//!
//! - 不读启动配置文件（profile 只来自 `LocalConfigStore`），不持有 Node/Device 密钥；
//! - 子进程环境先清空，再注入「凭据端口给出的绑定 + 必要进程环境」；
//! - 「能力不支持」与「协议/字段损坏」是两类错误，必须可区分；
//! - **平台分支**的 `cfg` 只出现在 [`platform`]（`rg "cfg\(" crates/agent-host/src` 的判据）；
//!   其余 `#[cfg(test)]` 都是测试模块（例如 `launch.rs`、`terminal.rs`），不是平台分支，`bin/` 零命中。
//!
//! 与 core 的分工（`CORE_PORTS_AND_STORAGE.md` §6 第 19 条）：本 crate 只产出 ACP 派生投影
//! （`block`/`title`/`options`/`deltaIndex` 等），**不**填 `turnId`/`version`——两者由 core 在提交前注入
//! （`SessionEndpoint::prompt` 的签名不携带 core 的 `TurnId`，会话版本也由 core 掌握）；`tests/view_contract.rs`
//! 用真实适配器输出断言这一分工与 §10.3 的其余最低字段。

pub mod config;
pub mod error;
pub mod host;
pub mod launch;
pub mod limits;
pub mod mapper;
pub mod node;
pub mod platform;
pub mod process;
mod prompt;
pub mod release;
pub mod session;
mod terminal;
#[cfg(any(test, feature = "test-support"))]
mod test_support;

pub use config::HostConfig;
pub use error::HostError;
pub use host::{
    AgentHost, SessionResourceFacts, runtime_generation, runtime_running, spawn_idle_sweep,
};
pub use launch::{LaunchSpec, resolve_launch};
pub use node::{NodeEventError, NodeEvents, PublicErrorView};
pub use release::ReleaseOutcome;
pub use session::{AcpSession, Endpoint};
