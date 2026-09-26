//! Node Link 的入站适配层（`docs/MODULE_ARCHITECTURE.md` §4.9、`design.md` D2/D3）。
//!
//! 分层：`transport::net` 只负责连接与字节/帧规则（listener 绑定、TLS 终止、Host/代理头边界、path 路由、
//! 请求体上限、WS upgrade 规则），本模块负责 Node Link 的协议语义；两者之间只经 `HttpHandler`/
//! `HttpRequest`/`HttpResponse`/`PeerInfo`/`WsHandler`/`WsConnection` 的公开形状通信，不 import `axum` 类型。
//!
//! 本切片落地的范围（`node-link-owner`）：配对 HTTP（[`pairing`] 的 claim/status 端点，WP3）与
//! WSS 连接生命周期（[`conn`] 的握手/信封/序号/limits/心跳/发送队列，WP4）。`catalog`、`resource`、
//! `command` 属 WP5–WP6，经 [`conn::MessageRoute`] 接入认证后消息的分派。
//!
//! WP3 内部的两条固定限流（`NODE_LINK_PROTOCOL.md` §2.5，不可配置）：claim 10 次/分钟/IP（复用接入层的
//! `SlidingWindowLimiter`，键是 `PeerInfo::client_ip`）与 status 60 次/分钟/`pairingId`（`pairing` 内部的
//! `PairingIdWindow`，因为接入层限流器的键固定是 `IpAddr`，而 `pairingId` 不是地址）。超限一律 429。
//!
//! 依赖纪律（`docs/MODULE_ARCHITECTURE.md` §4.9、`AGENTS.md` §4/§5）：
//!
//! - 只调用 `core::use_cases`（配对通道以 `Actor::PairingClaimant`）与 `identity-auth` 的公开入口，
//!   不查 SQLite、不调用 `node-link-client`，也不调用 `server::local_admin` 等平级 adapter；
//! - 不读系统时间：时间一律来自状态机注入的 `Clock`（`Authority::now`）；
//! - 不读配置文件：组合根把判定所需的配置快照注入构造器（[`PairingHttpConfig`]）；
//! - 不注册路由、不绑定 listener：接线属组合根（WP7）；但注册时**必须**把
//!   [`pairing::PairingHttp::default_response_headers`] 传给 `NetListener::register_post`，否则接入层在调用
//!   处理器前产生的 413/Host 400 不带 §13.1 要求的四个安全头。
//!
//! 唯一权威：`docs/NODE_LINK_PROTOCOL.md` §13（配对 HTTP）与 §2.5（固定限流），行为范围以
//! `openspec/changes/node-link-owner/specs/node-link-pairing-http/spec.md` 的 R19–R35 为准。

pub mod conn;
pub mod pairing;

#[cfg(test)]
mod tests;

pub use conn::{
    MessageRoute, NodeLinkConfig, NodeLinkConn, RouteOutcome, WS_PATH, WS_SUBPROTOCOL, WsEndpoint,
};
pub use pairing::{CLAIM_PATH, PairingHttp, PairingHttpConfig, STATUS_PATH};
