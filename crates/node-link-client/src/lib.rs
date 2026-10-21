//! Access Node 侧的 Node Link 出站适配器（`docs/MODULE_ARCHITECTURE.md` §4.6）。
//!
//! 已落地的能力（本工作包范围内）：
//!
//! - [`transport`]：最小 `Transport` 接口与 WSS 实现（TLS 信任锚显式注入，无「跳过校验」路径）；
//! - [`handshake`]：`node.hello` → `node.challenge`（验 Owner 签名）→ `node.proof` → `node.ready`；
//! - [`link`]：已认证连接的请求复用、序号、心跳、catalog/事件分派、`resource.ack`，
//!   以及「断线后结果不确定、重连只做 `command.status` 查询」；
//! - [`runtime`]：有界内存目录、来源事件校验/缓冲、来源屏障等待，以及在线回源读取与 Owner 命令查询；
//!   不写数据库、临时文件或日志；
//! - [`remote_session`]：远程 `SessionEndpoint`（prompt/cancel/set_mode/set_config/交互解析/
//!   `session.read` 原样结果）与 `SessionBackendFactory`（只接受远程引用）；
//! - [`manager`]：每个 Owner 一个 supervisor（连接所有权、指数退避重连、撤销后停止、无 detached task）；
//! - [`attachment`]：独立认证附件连接上的有界内存转发（`upload`/`chunk`/`seal`/`read`/清单/
//!   `abort`，停等、不落盘、已转发不等于已提交；`revoke`/`shutdown` 用同一关闭通道中止在途传输）；
//! - [`pairing`]：`node pair --mode access` 的 HTTPS claim/status（二维码固定 Owner 公钥、验签
//!   `ownerProof`、`approved` 后才由 `app::access::AccessManager` 落盘信任）。
//!
//! 组合根接线（`conversation-area-complete` WP5，2026-10-20）：`app` 用
//! [`crate::manager::NodeLinkClient`] 装配出站连接，`app::access::AccessManager` 负责 claim、
//! 启动实例化、路由与撤销；本 crate 不再是「只被库级调用方使用」的状态。
//!
//! 边界纪律：依赖方向是 `node-link-client -> core + node-link-protocol + identity-auth + acpr-wire`；
//! 不依赖 `server`/`agent-host`/`storage-sqlite`；不读系统时钟、不持私钥；远程正文只在有界内存中
//! 转发（`no-content-cache`）。

pub mod attachment;
pub mod error;
pub mod handshake;
pub mod identity;
pub mod link;
pub mod manager;
pub mod pairing;
pub mod remote_session;
pub mod runtime;
pub mod transport;

pub use attachment::{
    AttachmentForwarder, ReadChunk, SealedAttachment, SealedManifest, TransferStatus, UploadOutcome,
};
pub use error::ClientError;
pub use handshake::{
    ATTACHMENT_FEATURE, CORE_FEATURE, HandshakeConfig, OwnerTrust, PENDING_INTERACTIONS_FEATURE,
    RAW_ACP_FEATURE, REMOTE_HISTORY_FEATURE, REQUIRED_FEATURES, SESSION_ACTIVITY_FEATURE,
    SESSION_CREATE_FEATURE, SESSION_TITLE_FEATURE, SUPPORTED_FEATURES, TYPED_MEDIA_FEATURE,
};
pub use identity::{KeystoreIdentity, NodeIdentity};
pub use manager::{ClientConfig, NodeLinkClient, OwnerLink, ReconnectPolicy};
pub use pairing::{
    CLAIM_PATH as PAIRING_CLAIM_PATH, ClaimOutcome, ClaimSession, PAIR_PATH, PairingTicket,
    STATUS_PATH as PAIRING_STATUS_PATH, StatusOutcome, claim, status,
};
pub use remote_session::{RemoteSession, RemoteSessionFactory};
pub use runtime::{RemoteRuntime, RuntimeEvent, STATELESS_ORIGIN_FEATURE};
pub use transport::{
    ConnectionTarget, TlsTrust, Transport, TransportFactory, WssTransport, WssTransportFactory,
};
