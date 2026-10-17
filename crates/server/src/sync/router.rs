//! 认证后消息的路由组合（变更 `sync-server-commands` 的任务 6.1）。
//!
//! [`SyncConn::with_route`](crate::sync::conn::SyncConn::with_route) 只有**一个**路由槽，而 Sync 的入站
//! 面按家族拆成两条路由：订阅面
//! ([`SubscribeRoute`](crate::sync::subscribe::SubscribeRoute)：`sync.subscribe`/`sync.snapshot_request`/
//! `sync.ack`) 与命令面 ([`CommandRoute`](crate::sync::command::CommandRoute)：`command`)。本模块是它们的
//! 组合件，也是组合根注册进 `/sync/v1` 的那一条路由。
//!
//! 两条不变量：
//!
//! - `route` **按注册顺序询问**，第一个 [`RouteOutcome::Claimed`] 的胜出；全部未认领时返回
//!   [`RouteOutcome::Unclaimed`]，`conn` 因此对未知或尚无实现的 v1 类型照旧回
//!   `protocol.type_unsupported`（对端可见的显式拒绝，绝不静默丢弃）。组合件不承载任何业务规则，也不
//!   重算任何判定。
//! - `on_authenticated` **转发给每一个子路由**：这是本次协商结果的唯一交接点（wire 上没有第二个承载它
//!   的字段，见 [`MessageRoute::on_authenticated`]）。漏转发不会报错，只会让门控静默取到空集（失败关闭
//!   的方向），因此这里按「一个不落」实现，而不是「谁需要谁自己拿」。
//!
//! 与 `server::node_link` 的组合件**同形而各自定义**（`AGENTS.md` §5/§6：平级的入站适配器互不调用、
//! 类型各自定义），差别只有一处：Node Link 的连接面今天不交接 feature，那条组合件因此没有
//! `on_authenticated` 这一腿。

use std::sync::Arc;

use sync_protocol::envelope::Envelope;

use crate::sync::conn::{ConnectionHandle, MessageRoute, RouteOutcome};

/// 多路复用：按注册顺序询问每个路由，第一个认领的胜出；`on_authenticated` 一个不落地转发。
///
/// 组合根的注册顺序固定为**订阅面 → 命令面**：两面的 type 词表不相交（`sync.*` 与 `command`），顺序因此
/// 不改变判定结果，但固定顺序让询问次数可预期，也让用例能钉住「订阅面不被命令面抢先」。
pub struct SyncRoutes {
    parts: Vec<Arc<dyn MessageRoute>>,
}

impl SyncRoutes {
    /// 装配（顺序即询问顺序，也是 [`MessageRoute::on_authenticated`] 的转发顺序）。
    ///
    /// 入参是订阅面与命令面的两条路由：接受 `Vec` 只为与 `server::node_link` 的组合件同形——本端点今天
    /// 只有这两个家族。
    pub fn new(parts: Vec<Arc<dyn MessageRoute>>) -> Self {
        Self { parts }
    }
}

#[async_trait::async_trait]
impl MessageRoute for SyncRoutes {
    async fn route(&self, session: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
        for part in &self.parts {
            if part.route(session, message).await == RouteOutcome::Claimed {
                return RouteOutcome::Claimed;
            }
        }
        RouteOutcome::Unclaimed
    }

    fn on_authenticated(&self, session: &ConnectionHandle, features: &[String]) {
        for part in &self.parts {
            part.on_authenticated(session, features);
        }
    }
}

