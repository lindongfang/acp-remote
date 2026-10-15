//! 共享 HTTP/WSS 接入面：listener 绑定、path 路由、Host/代理头边界、TLS 终止与连接级上限
//! （`design.md` D2/D9/D10，`docs/MODULE_ARCHITECTURE.md` §4.9）。
//!
//! 模块组成：
//!
//! - [`config`]：`NetConfig` 与 `TlsMode`（`daemon.listen`/`daemon.tls.*`/`daemon.allowed_hosts`/
//!   `daemon.trusted_proxies`/`dev_mode.allow_plaintext` 的传输层形状）；
//! - [`listener`]：绑定、失败关闭、非 loopback 告警、`serve` 与带上限的连接排空（`direct` 模式的
//!   TLS 握手在接入选层的有界并发池里完成，不占 accept 关键路径）；
//! - [`route`]：path → 处理器注册表与请求边界中间件（ws/post 端点 + 显式注册的静态面；未注册且不在
//!   静态面内的 path 一律 404）；
//! - [`static_ui`]：第三类端点「静态 GET」——前端入口文档、单页回退、内容类型映射与安全响应头
//!   （资源源经 [`UiAssetSource`] 注入，本层不读磁盘也不嵌入产物）；
//! - [`host`]、[`proxy`]：`Host` 白名单/`public_origin` 校验与可信代理后的真实对端地址；
//! - [`origin`]：升级请求的**来源**（HTTP `Origin`）——只如实取得并归一化，不判定它是否与设备绑定一致；
//! - [`ws`]、[`http`]：上层（`server::node_link`）实现的处理器 trait 与帧/请求值对象；
//! - [`tls`]、[`permissions`]：`direct` 模式的 PEM 加载、失败关闭与平台权限判定；
//! - [`ratelimit`]：进程内滑动窗口限流器（键 = 对端真实 IP，供 `node_link` 的认证/配对限流使用）。
//!
//! **本层不含任何 Node Link 业务语义**：不知道消息类型、不import `node-link-protocol`，也不暴露
//! `axum` 的路由、提取器与服务设施——上层只经 [`WsHandler`]/[`HttpHandler`] 注入处理器，并消费本模块的
//! 值对象（[`HttpRequest`]/[`HttpResponse`]/[`WsConnection`]/[`PeerInfo`]/[`RequestOrigin`]）。这些值对象
//! 的字段直接使用 `http`/`bytes` 的值类型（`Method`/`HeaderMap`/`StatusCode`/`Bytes` 等，经 `axum` 再导出），
//! 含义不因经过本层而改变，上层按 `axum` 的公开路径引用它们即可（本模块不重复导出这批值类型）。因此协议
//! 语义（帧类型拒绝、握手状态机、错误码、close code、来源与设备绑定的比较）留在上层
//! （`server::node_link` 与 `server::sync`）。
//!
//! 唯一权威：`docs/CONFIG_REFERENCE.md` §1/§10（配置键与三形态表）、`docs/NODE_LINK_PROTOCOL.md`
//! §2.1/§2.5/§13.4（端点、subprotocol、压缩拒绝、上限）、`docs/SECURITY_DESIGN.md` §7.1/§7.3/§13.2
//! （TLS 边界、明文限制、文件权限）、`docs/SYNC_PROTOCOL.md` §3.2（来源校验与归一化口径）。
//!
//! **来源与 `Host` 不可互相替代**：`Host` 是**虚拟主机**概念——它回答「这个请求打到了哪个虚拟主机」，
//! 由 [`HostPolicy`] 按 `daemon.allowed_hosts`/`daemon.public_origin` 判定；来源（HTTP `Origin`）是
//! **协议层**概念——它回答「发起请求的页面从哪个来源加载」，并且它本身就是 Sync 设备身份持久化的绑定
//! 对象。反向代理形态下两者可以合法地不同（对外是 `https://pc.example.ts.net`，代理转发时可以把 `Host`
//! 改写成内部名），所以既不能用 `Host` 顶替来源（会误拒合法的反代连接），也不能因为 `Host` 通过白名单
//! 就认定来源合法（来源与 `Host` 是两次独立判定，`docs/SYNC_PROTOCOL.md` §15 的「`Origin`、Host key、
//! device key 和 scopes 都是独立检查，任一通过不能替代其他检查」）。归一化口径与 `docs/SYNC_PROTOCOL.md`
//! §3.2 的逐条对应见 [`origin`] 模块文档。

pub mod config;
pub mod host;
pub mod http;
pub mod listener;
pub mod origin;
pub mod permissions;
pub mod proxy;
pub mod ratelimit;
pub mod route;
pub mod shutdown;
pub mod static_ui;
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
pub use origin::RequestOrigin;
pub use proxy::{ProxyPolicy, ResolvedPeer};
pub use ratelimit::{RateLimit, SlidingWindowLimiter};
pub use route::{HttpHandler, RouteError};
pub use shutdown::{Shutdown, ShutdownHandle};
pub use static_ui::UiAssetSource;
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

/// 配对 HTTP 请求体的默认上限（`NODE_LINK_PROTOCOL.md` §13.4 与 `SYNC_PROTOCOL.md` §14：超限返回 413）。
///
/// **取 16 KiB 的理由（`sync-server-connection` 任务 6.1 的决定）**：两条配对面共用本模块**唯一**的
/// 每 path 请求体上限（[`NetConfig::max_body_bytes`]；`route::serve_post` 在调用处理器**之前**按它判
/// 413），而 `SYNC_PROTOCOL.md` §14 把 Sync 配对 HTTP 的 body 冻结为 16 KiB。取更大的值会让 16–64 KiB
/// 区间在 Sync 面上**不**按 §14 返回 413（偏差被 80 KiB 级别的用例掩盖）；取 16 KiB 对 Node Link 面
/// 同样合规——§13.4 只要求「有上限」，未冻结数值，且 claim/status 都是小型 JSON。
/// 为 route 层新增 per-route 上限能力是另一条路（改动更大、且收益只是让两面取不同数值），未采纳。
///
/// 该值不随配置变化（`CONFIG_REFERENCE.md` §2/§3 没有对应配置键）；调低它只会让更大的请求更早被拒，
/// 不会放宽任何一面。
pub const DEFAULT_MAX_BODY_BYTES: usize = 16 * 1024;

/// 默认的关闭排空宽限（对应 `daemon.shutdown_grace_ms` 的默认值 10000）。
pub const DEFAULT_DRAIN_GRACE_MS: u64 = 10_000;
