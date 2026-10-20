//! `node.pair.*` 的 **Access 方向**注入口（`LOCAL_ADMIN_PROTOCOL.md` §5.4、
//! `NODE_LINK_PROTOCOL.md` §13）。
//!
//! 为什么需要这个端口：`mode = "access"` 要对 Owner 发起出站 HTTPS claim/status 并落盘 Owner 信任，
//! 而 HTTPS 客户端与 TLS 信任锚属于**出站适配器**（`node-link-client` + 组合根加载的系统根/自定义 CA）。
//! `server` 不得依赖出站适配器（入站适配器之间也不得互相调用），因此这条能力只能由组合根实现、
//! 经本 trait 注入 `LocalAdminRouter`（与 [`DaemonControl`](super::DaemonControl) 同一种缝）。
//!
//! 返回 `JsonObject` 而不是 server 侧类型：`result` 的字段集合由 §5.4 冻结，状态机（配对 phase、
//! 是否已落盘）属于实现者；本层只做参数校验与错误码投影。
//!
//! 三个方法都**不**接收 pairing secret：secret 从 `pairingUrl` 的 fragment 读出后只留在实现者的内存里
//! （`NODE_LINK_PROTOCOL.md` §13.1）。实现者不得把 URL、fragment、secret 或完整响应正文写进错误消息、
//! 日志或审计。

use crate::local_admin::envelope::JsonObject;
use crate::local_admin::error::AdminError;

/// Access 方向的节点配对（claim + status + 幂等确认）。
#[async_trait::async_trait]
pub trait AccessPairing: Send + Sync {
    /// `node.pair.begin`（`mode = "access"`）：解析 Owner 的配对 URL、发起 claim。
    ///
    /// `display_name` 是本节点在对端界面上的展示名（可空，实现者回落到配置 `[node] name`）。
    /// 返回值是 §5.4 的 `result`：`pairingId`/`pairingUrl`/`state`/`expiresAt`/
    /// `peerNodeId`/`peerPublicKeyFingerprint`。
    async fn begin(
        &self,
        pairing_url: &str,
        display_name: Option<&str>,
    ) -> Result<JsonObject, AdminError>;

    /// `node.pair.status`：`pairingId` 不属于本实现时返回 `None`（路由层据此回落到 owner 方向）。
    ///
    /// 非终态时实现者向 Owner 刷新状态；刷新失败**不改变**已有状态，只在 `lastRefreshError` 里给出
    /// 简短原因（不含 URL/正文）。
    async fn status(&self, pairing_id: &str) -> Result<Option<JsonObject>, AdminError>;

    /// `node.pair.confirm`：只用于幂等回读（Access 侧的信任由 Owner 确认后自动建立）。
    ///
    /// 未命中返回 `None`。已批准时返回 `{nodeId, confirmedAt}`；未批准时返回
    /// `local.conflict`（Access 不能代替 Owner 确认）。
    async fn confirm(&self, pairing_id: &str) -> Result<Option<JsonObject>, AdminError>;

    /// `node.pair.reject`：终结本机的 claim 状态（不向 Owner 发请求，协议没有对应的拒绝端点）。
    ///
    /// 返回 `true` 表示该 `pairingId` 由本实现持有并已终结。`reason` 只用于日志类别，不回显。
    async fn reject(&self, pairing_id: &str, reason: Option<&str>) -> Result<bool, AdminError>;
}
