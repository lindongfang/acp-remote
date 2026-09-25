//! 管理信封（channel `0x01`）的编解码、`local.*` 错误码与方法分发表
//! （`docs/LOCAL_ADMIN_PROTOCOL.md` §4–§6，`docs/MODULE_ARCHITECTURE.md` §4.9）。
//!
//! 模块组成：
//!
//! - [`envelope`]：请求/响应值对象、closed object 校验、`v = 1` 规则、`id`/`method`/`params` 校验；
//! - [`method`]：v1 方法集与方法名语法；
//! - [`error`]：九个 `local.*` 错误码与 `error{code,message}` 对象；
//! - [`handler`]：[`LocalAdminHandler`] 契约与 WP3a 的空路由；
//! - [`params`]：方法级参数校验与 `PortError` → `local.*` 映射；
//! - [`router`]：[`LocalAdminRouter`]（方法路由，替换空路由）；
//! - [`daemon`]：[`DaemonControl`] 注入口与 `daemon.status` 的字段集合；
//! - [`view`]／[`audit`]：core 值对象到 `result` 的投影与审计导出写出。
//!
//! 词表的唯一机器定义在 `schemas/local-admin/v1/envelope.schema.json`：方法集与错误码枚举逐项等于
//! [`method::Method::ALL`] 与 [`error::LocalErrorCode::ALL`]，由常驻漂移测试断言；`fixtures/local-admin/v1/`
//! 的 valid/invalid 信封逐条做往返/拒绝测试。本模块不复制第二份词表。
//!
//! 本切片（WP3a + WP3b1）的信封层与方法路由已就位：方法集里的本地配置族、`daemon.*`、
//! Export/Import 与 `audit.export` 已实现（`tasks.md` 2.10/2.12）；设备与节点配对族（2.11）仍由
//! [`router`] 回 `local.unsupported`。

pub(crate) mod audit;
pub mod daemon;
pub mod envelope;
pub mod error;
pub mod handler;
pub mod method;
pub(crate) mod params;
pub mod router;

#[cfg(test)]
pub(crate) mod test_support;

pub(crate) mod view;

pub use daemon::{
    DaemonAgent, DaemonControl, DaemonCounts, DaemonLink, DaemonLinkState, DaemonStatus,
    MAX_STOP_GRACE_MS,
};
pub use envelope::{
    AdminOutcome, AdminRequest, AdminResponse, CHANNEL_VERSION, EnvelopeEncodeError,
    InvalidRequestReason, JsonObject, RequestDecodeError, RequestDecodeOutcome, RequestId,
    ResponseDecodeError, decode_request, decode_response,
};
pub use error::{AdminError, LocalErrorCode};
pub use handler::{LocalAdminHandler, UnroutedAdminHandler};
pub use method::{Method, is_method_name};
pub use router::{LocalAdminDeps, LocalAdminRouter};
