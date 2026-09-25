//! 管理信封（channel `0x01`）的编解码、`local.*` 错误码与方法分发表
//! （`docs/LOCAL_ADMIN_PROTOCOL.md` §4–§6，`docs/MODULE_ARCHITECTURE.md` §4.9）。
//!
//! 模块组成：
//!
//! - [`envelope`]：请求/响应值对象、closed object 校验、`v = 1` 规则、`id`/`method`/`params` 校验；
//! - [`method`]：v1 方法集与方法名语法；
//! - [`error`]：九个 `local.*` 错误码与 `error{code,message}` 对象；
//! - [`handler`]：[`LocalAdminHandler`] 契约与 WP3a 的空路由。
//!
//! 词表的唯一机器定义在 `schemas/local-admin/v1/envelope.schema.json`：方法集与错误码枚举逐项等于
//! [`method::Method::ALL`] 与 [`error::LocalErrorCode::ALL`]，由常驻漂移测试断言；`fixtures/local-admin/v1/`
//! 的 valid/invalid 信封逐条做往返/拒绝测试。本模块不复制第二份词表。
//!
//! 本切片（WP3a）只到信封层：方法路由留空，所有已知方法返回 `local.unsupported`。业务语义与 `core::use_cases`
//! 接线属 WP3b。

pub mod envelope;
pub mod error;
pub mod handler;
pub mod method;

pub use envelope::{
    AdminOutcome, AdminRequest, AdminResponse, CHANNEL_VERSION, EnvelopeEncodeError,
    InvalidRequestReason, JsonObject, RequestDecodeError, RequestDecodeOutcome, RequestId,
    ResponseDecodeError, decode_request, decode_response,
};
pub use error::{AdminError, LocalErrorCode};
pub use handler::{LocalAdminHandler, UnroutedAdminHandler};
pub use method::{Method, is_method_name};
