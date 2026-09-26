//! 共享 HTTP/WSS 接入面：listener 绑定、path 路由、Host/代理头边界、TLS 终止与连接级上限
//! （`design.md` D2/D9/D10，`docs/MODULE_ARCHITECTURE.md` §4.9）。
//!
//! 模块组成：
//!
//! - [`config`]：`NetConfig` 与 `TlsMode`（`daemon.listen`/`daemon.tls.*`/`daemon.allowed_hosts`/
//!   `daemon.trusted_proxies`/`dev_mode.allow_plaintext` 的传输层形状）；
//! - [`listener`]：绑定、失败关闭、非 loopback 告警、`serve` 与带上限的连接排空；
//! - [`route`]：path → 处理器注册表与请求边界中间件（未注册 path 一律 404）；
//! - [`host`]、[`proxy`]：`Host` 白名单/`public_origin` 校验与可信代理后的真实对端地址；
//! - [`ws`]、[`http`]：上层（`server::node_link`）实现的处理器 trait 与帧/请求值对象；
//! - [`tls`]、[`permissions`]：`direct` 模式的 PEM 加载、失败关闭与平台权限判定；
//! - [`ratelimit`]：进程内滑动窗口限流器（键 = 对端真实 IP，供 `node_link` 的认证/配对限流使用）。
//!
//! **本层不含任何 Node Link 业务语义**：不知道消息类型、不import `node-link-protocol`，也不把
//! `axum` 类型暴露给上层。上层只经 [`WsHandler`]/[`HttpHandler`] 注入处理器并消费本模块的值对象，
//! 因此协议语义（帧类型拒绝、握手状态机、错误码、close code）留在 `server::node_link`。
//!
//! 唯一权威：`docs/CONFIG_REFERENCE.md` §1/§10（配置键与三形态表）、`docs/NODE_LINK_PROTOCOL.md`
//! §2.1/§2.5/§13.4（端点、subprotocol、压缩拒绝、上限）、`docs/SECURITY_DESIGN.md` §7.1/§7.3/§13.2
//! （TLS 边界、明文限制、文件权限）。

pub mod config;
pub mod host;
pub mod http;
pub mod listener;
pub mod permissions;
pub mod proxy;
pub mod ratelimit;
pub mod route;
pub mod shutdown;
pub mod tls;
pub mod ws;

#[cfg(test)]
mod test_client;
#[cfg(test)]
mod tests;

pub use config::{NetConfig, TlsMode};
pub use host::HostPolicy;
pub use http::{HttpRequest, HttpResponse, PeerInfo};
pub use listener::{ListenerWarning, NetError, NetListener};
pub use proxy::{ProxyPolicy, ResolvedPeer};
pub use ratelimit::{RateLimit, SlidingWindowLimiter};
pub use route::{HttpHandler, RouteError};
pub use shutdown::{Shutdown, ShutdownHandle};
pub use tls::TlsFile;
pub use ws::{WsConnection, WsError, WsHandler, WsMessage};

/// `daemon.listen` 的内置默认值（`CONFIG_REFERENCE.md` §1）。
///
/// 默认只监听 loopback；非 loopback 必须显式配置并在启动输出中告警（`[ListenerWarning::NonLoopbackListen]`）。
pub const DEFAULT_LISTEN: &str = "127.0.0.1:8765";

/// 单条 WebSocket message 的默认上限（`NODE_LINK_PROTOCOL.md` §2.5：1 MiB）。
///
/// 上限可经 `daemon` 配置下调（`node_link.max_message_bytes` → `node.ready.limits.maxMessageBytes`），
/// 传递到本模块的 [`NetConfig::max_message_bytes`]；超限在分配大缓冲前以 `1009` 关闭。
pub const DEFAULT_MAX_MESSAGE_BYTES: usize = 1024 * 1024;

/// 配对 HTTP 请求体的默认上限（`NODE_LINK_PROTOCOL.md` §13.4：超限返回 413）。
///
/// §13.4 只要求「有上限」，未冻结数值；claim/status 请求是小型 JSON，因此默认取 64 KiB，
/// 由 [`NetConfig::max_body_bytes`] 决定（`CONFIG_REFERENCE.md` §3 目前没有对应配置键）。
pub const DEFAULT_MAX_BODY_BYTES: usize = 64 * 1024;

/// 默认的关闭排空宽限（对应 `daemon.shutdown_grace_ms` 的默认值 10000）。
pub const DEFAULT_DRAIN_GRACE_MS: u64 = 10_000;
