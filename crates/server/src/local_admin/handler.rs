//! 管理请求的处理器契约与 WP3a 的空路由。
//!
//! 方法实现属 WP3b（`tasks.md` 2.10–2.12）：`app` 在切片 4 的后续波次把持有 `core::use_cases` 的
//! `LocalAdminRouter` 装配进来，本 crate 只定义它必须满足的契约。

use crate::local_admin::envelope::{AdminRequest, AdminResponse};
use crate::local_admin::error::AdminError;

/// 处理一条已通过信封校验的管理请求。
///
/// 契约：
///
/// - 每个请求恰好返回一个响应，并把请求的 `id` 原样回带（§4 规则 5）；
/// - `params` 校验失败返回 `local.invalid_params`（§4 规则 4）；
/// - 实现**不得**在响应里回显凭据值（§4 规则 6）；
/// - 实现只调用 `core::use_cases`（`docs/MODULE_ARCHITECTURE.md` §4.9），不查询 SQLite、不启动 Agent；
/// - 被取消（连接结束）时不得留下半提交状态：core 的写集是单事务，取消即回滚（`CORE_PORTS_AND_STORAGE.md` §11.2）。
#[async_trait::async_trait]
pub trait LocalAdminHandler: Send + Sync {
    /// 处理一个请求并返回唯一的响应。
    async fn handle(&self, request: AdminRequest) -> AdminResponse;
}

/// WP3a 的空路由：v1 方法集内的每个方法都还没有实现，因此都返回 `local.unsupported`（§6：
/// 「方法在当前 delivery 阶段尚未实现」）。方法名不在 v1 方法集里的请求更早一步由
/// [`crate::local_admin::envelope::decode_request`] 拦下，同样回 `local.unsupported`（§4 规则 3）。
pub struct UnroutedAdminHandler;

#[async_trait::async_trait]
impl LocalAdminHandler for UnroutedAdminHandler {
    async fn handle(&self, request: AdminRequest) -> AdminResponse {
        AdminResponse::failure(
            request.id().clone(),
            AdminError::unsupported_method(request.method()),
        )
    }
}
