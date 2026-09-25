//! 平台本地管理通道（`docs/LOCAL_ADMIN_PROTOCOL.md`，`docs/MODULE_ARCHITECTURE.md` §4.9）。
//!
//! 模块组成：
//!
//! - [`framing`]：`u32be` 长度前缀 + channel 字节的帧编解码与上限（§3）；
//! - [`connection`]：一条连接的用途绑定、未完成请求上限、管理请求分派与 `0x02` 的失败方式；
//! - [`endpoint`]：platform 无关的命名规则、配置与错误（§2.1）；
//! - `platform`：Windows Named Pipe 与 Unix socket 的创建、权限与对端凭据校验（§2.1/§2.2）；
//! - [`attachment`]：`0x02` 的 `FacadeAttachmentId` 生命周期（§3.1）；
//! - [`audit`]：连接级 `authorization.denied` 事件的接线点（`SECURITY_DESIGN.md` §14.2）。
//!
//! 本层不放任何业务语义：管理载荷交给 [`crate::local_admin`]，ACP 字节流在切片 6 交给 `server::acp_facade`。
//!
//! 背压与资源上限：
//!
//! - 管理连接同时最多 [`MAX_IN_FLIGHT_REQUESTS`] 条未完成请求（§4 规则 2），响应按 `id` 关联写回；
//! - 帧读取使用固定粒度的缓冲（每次系统调用最多 8 KiB），未消费字节不会无界增长；
//! - `0x02` 方向「单方向未消费字节上限 1 MiB」的接线点在切片 6 的 facade 分发点（本切片该方向即连即关，
//!   不存在跨连接的字节累积）。

pub mod attachment;
pub mod audit;
pub mod connection;
pub mod endpoint;
pub mod framing;
pub mod platform;

pub use attachment::{FacadeAttachmentId, FacadeAttachmentRegistry};
pub use audit::{AuditHook, AuthorizationDenied, DeniedReason, LoggingAuditHook};
pub use connection::{CloseReason, LocalConnectionHandlers, TransportError, serve_connection};
pub use endpoint::{
    EndpointError, InstanceId, InvalidInstanceId, LocalEndpointConfig, UnsafePathKind,
    named_pipe_path, unix_endpoint_directory, unix_socket_path, user_identity_hash,
};
pub use framing::{
    CHANNEL_ACP_STREAM_BYTE, CHANNEL_LOCAL_ADMIN_BYTE, ChannelKind, Frame, FrameError, FrameReader,
    LENGTH_PREFIX_BYTES, MAX_FRAME_PAYLOAD_BYTES,
};
pub use platform::{LocalEndpoint, LocalStream};

/// 同一管理连接上同时允许的未完成请求上限（§4 规则 2、§3.1 的并发上限同值）。
///
/// 与 `schemas/local-admin/v1/envelope.schema.json#/$defs/framing.maxInFlightRequests` 同值，
/// 由常驻漂移测试断言。
pub const MAX_IN_FLIGHT_REQUESTS: usize = 32;
