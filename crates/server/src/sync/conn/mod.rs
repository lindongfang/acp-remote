//! Sync WSS 连接生命周期：握手驱动、信封/序号校验、限额、心跳与每连接发送队列
//! （`docs/SYNC_PROTOCOL.md` §3/§4/§8/§12/§13/§14，`design.md` D1/D2/D4/D5）。
//!
//! 模块组成：
//!
//! - [`limits`]：`auth.authenticated.limits` 的三项可下调值与「只下调」的构造期不变量；
//! - [`wire`]：信封/序号/错误 body 的判定与装配（纯函数）；
//! - [`handshake`]：版本与 feature 协商、`PeerTrust` 装配、连接绑定（纯函数）；
//! - [`registry`]：连接注册表与每连接句柄（WP4 的事件扇出、WP6 的撤销钩子消费）；
//! - [`session`]：一条连接一个任务的会话驱动与 [`session::MessageRoute`] 分派口。
//!
//! 依赖纪律（`docs/MODULE_ARCHITECTURE.md` §4.9、`AGENTS.md` §5）：只调用 `core::use_cases` 与
//! `sync-protocol`/`identity-auth` 的公开面，不 import `storage-sqlite`、不查 SQLite、不调用平级 adapter
//! （`server::node_link`）；时间与签名都经 `identity_auth::Authority`，本层不读系统时间、不持私钥。
//! 注册路由与绑定 listener 属组合根：`NetListener::register_ws(WS_PATH, WS_SUBPROTOCOL, handler)`。
//!
//! **结构照搬 `server::node_link::conn`，类型各自定义**（`AGENTS.md` §6：两个边界不复用对方 wire DTO）：
//! 一条连接一个会话任务、一封信封解码分派、一个连接注册表加出站账本，这三件事两侧同形；而消息类型、
//! 限额形状、绑定的取值对象（canonical origin vs endpoint）与注册表的键（`(hostId, deviceId)` vs
//! `connectionId`）各自定义。
//!
//! 唯一权威：`docs/SYNC_PROTOCOL.md` §3.1/§3.2/§4/§5/§8/§12/§13/§14/§15，行为范围以
//! `openspec/changes/archive/2026-10-15-sync-server-connection/specs/sync-device-auth/spec.md` 的八条要求为准。

pub mod handshake;
pub mod limits;
pub mod registry;
pub mod session;
mod wire;

pub use handshake::{
    NegotiationFault, PROTOCOL_VERSION, SUPPORTED_FEATURES, connection_binding, negotiate_features,
    negotiated_version, peer_trust, pending_pairing, transcript_features, wire_features,
};
pub use limits::{SessionLimits, SyncConfig};
pub use registry::{ConnectionHandle, ConnectionRegistry, PendingQueue, SendFault};
pub use session::{
    HEARTBEAT_SILENCE_TIMEOUT, MAX_IN_FLIGHT_HANDSHAKES_PER_IP, MessageRoute, RouteOutcome,
    SyncConn, WsEndpoint,
};

use std::time::Duration;

/// Sync WSS 端点（§3.1）。
pub const WS_PATH: &str = "/sync/v1";

/// Sync 的 WebSocket subprotocol（§3.1；接入层按它做 upgrade 白名单，机器登记见
/// `compatibility/errors/v1/errors.json` 的 `protocols.sync.subprotocol`）。
pub const WS_SUBPROTOCOL: &str = "acp-remote.sync.v1.json";

/// 整个认证（`auth.client_hello` → `auth.authenticated`）的上限（§14：15 秒，固定常量、不可配置）。
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

/// 单 IP 新认证尝试的固定限流（§14：10 次/分钟，不可配置）。
pub const AUTH_ATTEMPTS_PER_MINUTE: u32 = 10;

/// 认证限流的窗口（§14 的固定 1 分钟）。
pub const AUTH_WINDOW: Duration = Duration::from_secs(60);

/// 在途握手配额表的条目上限（与接入层限流器同口径：地址族条目有界，不随源地址无界增长）。
pub const MAX_TRACKED_IPS: usize = crate::transport::net::ratelimit::MAX_TRACKED_KEYS;

/// v1 的 WebSocket close code（§12.3，与 `compatibility/errors/v1/errors.json` 的
/// `protocols.sync.closeCodes` 逐条相等）。
pub mod close {
    /// 正常关闭。
    pub const NORMAL: u16 = 1000;
    /// WebSocket message 过大（由接入层在分配大缓冲前以它关闭）。
    pub const MESSAGE_TOO_LARGE: u16 = 1009;
    /// 协议或 schema 错误（含 binary frame）。
    pub const PROTOCOL: u16 = 4400;
    /// 未认证 / 认证超时。
    pub const UNAUTHENTICATED: u16 = 4401;
    /// 授权拒绝（业务阶段的 scope 判定由 WP4 消费）。
    pub const FORBIDDEN: u16 = 4403;
    /// 协议版本不兼容（版本交集为空或必需 feature 未满足）。
    pub const VERSION: u16 = 4406;
    /// heartbeat / handshake 超时。
    pub const TIMEOUT: u16 = 4408;
    /// identity 或状态冲突。
    pub const STATE_CONFLICT: u16 = 4409;
    /// 设备已撤销。
    pub const REVOKED: u16 = 4410;
    /// 同一设备的新连接已经替换当前连接。
    pub const REPLACED: u16 = 4411;
    /// 限流。
    pub const RATE_LIMITED: u16 = 4429;
    /// 服务端暂时不可用。
    pub const UNAVAILABLE: u16 = 4500;
}
