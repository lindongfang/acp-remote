//! Node Link WSS 连接生命周期：握手驱动、信封/序号校验、limits、心跳与每连接发送队列
//! （`docs/NODE_LINK_PROTOCOL.md` §2/§12.2，`design.md` D2/D3/D9）。
//!
//! 模块组成：
//!
//! - [`limits`]：可下调七项（`CONFIG_REFERENCE.md` §3）与「只下调」的构造期不变量；
//! - [`wire`]：信封/序号/错误 body 的判定与装配（纯函数）；
//! - [`handshake`]：版本与 feature 协商、`PeerTrust` 装配、本机 endpoint 推导（纯函数）；
//! - [`registry`]：连接注册表与每连接句柄（WP5 的事件扇出、WP6 的撤销传播消费）；
//! - [`session`]：一条连接一个任务的会话驱动与 [`session::MessageRoute`] 分派口。
//!
//! 依赖纪律（`docs/MODULE_ARCHITECTURE.md` §4.9）：只调用 `core::use_cases` 与 `identity-auth` 的公开
//! 入口，不查 SQLite、不调用平级 adapter；时间与签名都经 `identity_auth::Authority`，本层不读系统时间、
//! 不持私钥。注册路由与绑定 listener 属组合根（WP7）：`NetListener::register_ws(WS_PATH, WS_SUBPROTOCOL,
//! conn.ws_handler())`。
//!
//! 唯一权威：`docs/NODE_LINK_PROTOCOL.md` §2/§11.3/§12.2/§14，行为范围以
//! `openspec/changes/node-link-owner/specs/node-link-owner-server/spec.md` 的 R36–R50、R79–R81、
//! R32（衔接侧）、R82/R83（连接侧）为准。

pub mod handshake;
pub mod limits;
pub mod registry;
pub mod session;
mod wire;

#[cfg(test)]
mod tests;

pub use handshake::{
    NegotiationFault, REQUIRED_FEATURE, SUPPORTED_FEATURES, negotiate_features, node_endpoint,
    peer_trust,
};
pub use limits::{NodeLinkConfig, SessionLimits};
pub use registry::{ConnectionHandle, ConnectionRegistry, PendingQueue, SendFault};
pub use session::{
    HEARTBEAT_SILENCE_TIMEOUT, MAX_IN_FLIGHT_HANDSHAKES_PER_IP, MessageRoute, NodeLinkConn,
    RouteOutcome, WsEndpoint,
};

use std::time::Duration;

/// Node Link WSS 端点（§2.1）。
pub const WS_PATH: &str = "/node-link/v1";

/// Node Link 的 WebSocket subprotocol（§2.1；接入层按它做 upgrade 白名单）。
pub const WS_SUBPROTOCOL: &str = "acp-remote.nodelink.v1.json";

/// 整个握手（`node.hello` → `node.ready`）的上限（§2.5：15 秒，固定常量、不可配置）。
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

/// 单 IP 新认证尝试的固定限流（§2.5：10 次/分钟，不可配置）。
pub const AUTH_ATTEMPTS_PER_MINUTE: u32 = 10;

/// 认证限流的窗口（§2.5 的固定 1 分钟）。
pub const AUTH_WINDOW: Duration = Duration::from_secs(60);

/// 在途握手配额表的条目上限（与接入层限流器同口径：地址族条目有界，不随源地址无界增长）。
pub const MAX_TRACKED_IPS: usize = crate::transport::net::ratelimit::MAX_TRACKED_KEYS;

/// v1 的 WebSocket close code（§14.2，与 Sync 同一集合）。
pub mod close {
    /// 正常关闭。
    pub const NORMAL: u16 = 1000;
    /// 协议或 schema 错误（含 binary frame）。
    pub const PROTOCOL: u16 = 4400;
    /// 未认证 / 认证超时。
    pub const UNAUTHENTICATED: u16 = 4401;
    /// 协议版本不兼容。
    pub const VERSION: u16 = 4406;
    /// heartbeat/handshake 超时。
    pub const TIMEOUT: u16 = 4408;
    /// identity 或状态冲突。
    pub const STATE_CONFLICT: u16 = 4409;
    /// 节点已撤销。
    pub const REVOKED: u16 = 4410;
    /// 限流。
    pub const RATE_LIMITED: u16 = 4429;
    /// 服务端暂时不可用。
    pub const UNAVAILABLE: u16 = 4500;
}
